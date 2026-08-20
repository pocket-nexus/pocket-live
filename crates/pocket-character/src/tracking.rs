//! Client for the local Apple Vision helper.
//!
//! The helper writes newline-delimited, versioned `TrackingFrame` JSON to
//! stdout. The reader owns a single latest-value slot: if Vision produces
//! faster than rendering consumes, old frames are overwritten rather than
//! queued.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use pocket_live_core::{
    BodyCoordinateSpace, BodyObservation, FaceObservation, HandObservation, TrackingFrame,
};
use serde::Deserialize;

use crate::frame_share::{FrameShareReader, VideoFrame};

const MAX_LINE_BYTES: usize = 1 << 20;

#[derive(Clone, Debug)]
pub struct VisionLaunch {
    executable: PathBuf,
    args: Vec<String>,
    wait_for_first: bool,
    share_video: bool,
    face: Option<FaceLaunch>,
}

#[derive(Clone, Debug)]
pub struct FaceLaunch {
    pub python: PathBuf,
    pub script: PathBuf,
    pub model: PathBuf,
    pub pose_model: PathBuf,
    pub hand_model: PathBuf,
}

impl VisionLaunch {
    pub fn camera(executable: PathBuf, device: Option<String>, face: Option<FaceLaunch>) -> Self {
        let mut args = vec![
            "--tracking-fps".into(),
            "30".into(),
            "--body-mode".into(),
            if face.is_some() {
                "none".into()
            } else {
                "2d".into()
            },
            "--face-mode".into(),
            if face.is_some() {
                "none".into()
            } else {
                "landmarks".into()
            },
        ];
        if let Some(device) = device {
            args.push("--device".into());
            args.push(device);
        }
        Self {
            executable,
            args,
            wait_for_first: false,
            share_video: true,
            face,
        }
    }

    pub fn mock(executable: PathBuf) -> Self {
        Self {
            executable,
            args: vec!["--mock".into()],
            wait_for_first: true,
            share_video: true,
            face: None,
        }
    }

    pub fn waits_for_first_frame(&self) -> bool {
        self.wait_for_first
    }
}

#[derive(Default)]
struct Shared {
    latest: Mutex<Option<TrackingFrame>>,
    latest_controls: Mutex<Option<TimedControls>>,
    latest_video: Mutex<Option<Arc<VideoFrame>>>,
    accepted: AtomicU64,
    face_accepted: AtomicU64,
    face_backend_samples: AtomicU64,
    face_backend_detected: AtomicU64,
    pose_backend_detected: AtomicU64,
    hand_backend_detected: AtomicU64,
    last_body_controls_merged_ns: AtomicU64,
    rejected: AtomicU64,
    video_accepted: AtomicU64,
    video_with_mask: AtomicU64,
    first_capture_ns: AtomicU64,
    last_capture_ns: AtomicU64,
}

#[derive(Clone, Debug)]
struct TimedControls {
    captured_at_ns: u64,
    face: Option<FaceObservation>,
    body: Option<BodyObservation>,
    body_space: Option<BodyCoordinateSpace>,
    hands: Option<[HandObservation; 2]>,
    received_at: Instant,
}

#[derive(Deserialize)]
struct MediaPipeEnvelope {
    captured_at_ns: u64,
    face: Option<FaceObservation>,
    body: Option<BodyObservation>,
    body_space: Option<BodyCoordinateSpace>,
    hands: Option<[HandObservation; 2]>,
}

pub struct TrackingClient {
    shared: Arc<Shared>,
    child: Child,
    reader: Option<JoinHandle<()>>,
    face_child: Option<Child>,
    face_reader: Option<JoinHandle<()>>,
}

impl TrackingClient {
    pub fn spawn(launch: &VisionLaunch) -> Result<Self> {
        if !launch.executable.is_file() {
            bail!(
                "Vision bridge missing at {}; run `bun run vision:build`",
                launch.executable.display()
            );
        }
        let (frame_share, share_name) = if launch.share_video {
            let (reader, name) = FrameShareReader::create(1920, 1080)?;
            (Some(reader), Some(name))
        } else {
            (None, None)
        };
        let mut command = Command::new(&launch.executable);
        command.args(&launch.args);
        if let Some(name) = &share_name {
            command.arg("--frame-share-name").arg(name);
        }
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("starting {}", launch.executable.display()))?;
        let stdout = child
            .stdout
            .take()
            .context("Vision bridge stdout is not piped")?;
        let shared = Arc::new(Shared::default());
        let thread_shared = shared.clone();
        let reader = thread::Builder::new()
            .name("pocket-vision-json".into())
            .spawn(move || read_frames(stdout, &thread_shared, frame_share))
            .context("starting Vision bridge reader")?;
        let (face_child, face_reader) =
            if let (Some(face), Some(share_name)) = (&launch.face, share_name.as_ref()) {
                for path in [
                    &face.python,
                    &face.script,
                    &face.model,
                    &face.pose_model,
                    &face.hand_model,
                ] {
                    if !path.is_file() {
                        bail!("local face runtime is missing {}", path.display());
                    }
                }
                let mut child = Command::new(&face.python)
                    .arg(&face.script)
                    .arg("--model")
                    .arg(&face.model)
                    .arg("--pose-model")
                    .arg(&face.pose_model)
                    .arg("--hand-model")
                    .arg(&face.hand_model)
                    .arg("--frame-share-name")
                    .arg(share_name)
                    .arg("--fps")
                    .arg("15")
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .context("starting local MediaPipe tracking bridge")?;
                let stdout = child
                    .stdout
                    .take()
                    .context("MediaPipe tracking bridge stdout is not piped")?;
                let face_shared = shared.clone();
                let reader = thread::Builder::new()
                    .name("pocket-mediapipe-json".into())
                    .spawn(move || read_faces(stdout, &face_shared))
                    .context("starting MediaPipe face reader")?;
                (Some(child), Some(reader))
            } else {
                (None, None)
            };
        Ok(Self {
            shared,
            child,
            reader: Some(reader),
            face_child,
            face_reader,
        })
    }

    /// Clone the latest complete frame without waiting for the reader lock.
    pub fn latest(&self) -> Option<TrackingFrame> {
        self.shared
            .latest
            .try_lock()
            .ok()
            .and_then(|frame| frame.clone())
    }

    pub fn latest_video(&self) -> Option<Arc<VideoFrame>> {
        self.shared
            .latest_video
            .try_lock()
            .ok()
            .and_then(|frame| frame.clone())
    }

    pub fn counts(&self) -> (u64, u64) {
        (
            self.shared.accepted.load(Ordering::Relaxed),
            self.shared.rejected.load(Ordering::Relaxed),
        )
    }

    pub fn face_count(&self) -> u64 {
        self.shared.face_accepted.load(Ordering::Relaxed)
    }

    pub fn face_backend_counts(&self) -> (u64, u64) {
        (
            self.shared.face_backend_samples.load(Ordering::Relaxed),
            self.shared.face_backend_detected.load(Ordering::Relaxed),
        )
    }

    pub fn pose_backend_count(&self) -> u64 {
        self.shared.pose_backend_detected.load(Ordering::Relaxed)
    }

    pub fn hand_backend_count(&self) -> u64 {
        self.shared.hand_backend_detected.load(Ordering::Relaxed)
    }

    pub fn video_counts(&self) -> (u64, u64) {
        (
            self.shared.video_accepted.load(Ordering::Relaxed),
            self.shared.video_with_mask.load(Ordering::Relaxed),
        )
    }

    pub fn observed_fps(&self) -> f64 {
        let accepted = self.shared.accepted.load(Ordering::Relaxed);
        let first = self.shared.first_capture_ns.load(Ordering::Relaxed);
        let last = self.shared.last_capture_ns.load(Ordering::Relaxed);
        if accepted < 2 || last <= first {
            0.0
        } else {
            (accepted - 1) as f64 * 1_000_000_000.0 / (last - first) as f64
        }
    }

    pub fn wait_for_first(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if self.shared.accepted.load(Ordering::Acquire) > 0 {
                return true;
            }
            thread::sleep(Duration::from_millis(5));
        }
        false
    }
}

impl Drop for TrackingClient {
    fn drop(&mut self) {
        if let Some(child) = self.face_child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        if let Some(reader) = self.face_reader.take() {
            let _ = reader.join();
        }
    }
}

fn read_faces(stdout: impl std::io::Read, shared: &Shared) {
    let mut reader = BufReader::new(stdout);
    let mut bytes = Vec::with_capacity(4 * 1024);
    loop {
        bytes.clear();
        match reader.read_until(b'\n', &mut bytes) {
            Ok(0) => break,
            Ok(_) if bytes.len() > MAX_LINE_BYTES => {
                log::warn!("MediaPipe face frame exceeds {MAX_LINE_BYTES} bytes; dropped");
            }
            Ok(_) => {
                let Ok(envelope) = serde_json::from_slice::<MediaPipeEnvelope>(&bytes) else {
                    log::warn!("MediaPipe tracking bridge emitted malformed JSON");
                    continue;
                };
                shared.face_backend_samples.fetch_add(1, Ordering::Relaxed);
                let face = envelope.face.filter(|face| face.is_valid());
                if face.is_some() {
                    shared.face_backend_detected.fetch_add(1, Ordering::Relaxed);
                }
                *shared
                    .latest_controls
                    .lock()
                    .expect("tracking controls mutex poisoned") = Some(TimedControls {
                    captured_at_ns: envelope.captured_at_ns,
                    face,
                    body: envelope.body,
                    body_space: envelope.body_space,
                    hands: envelope.hands,
                    received_at: Instant::now(),
                });
            }
            Err(error) => {
                log::warn!("MediaPipe face bridge read failed: {error}");
                break;
            }
        }
    }
}

fn read_frames(
    stdout: impl std::io::Read,
    shared: &Shared,
    mut frame_share: Option<FrameShareReader>,
) {
    let mut reader = BufReader::new(stdout);
    let mut bytes = Vec::with_capacity(16 * 1024);
    loop {
        bytes.clear();
        match reader.read_until(b'\n', &mut bytes) {
            Ok(0) => break,
            Ok(_) if bytes.len() > MAX_LINE_BYTES => {
                shared.rejected.fetch_add(1, Ordering::Relaxed);
                log::warn!("Vision bridge frame exceeds {MAX_LINE_BYTES} bytes; dropped");
            }
            Ok(_) => {
                if let Some(sequence) = accept_line(&bytes, shared)
                    && let Some(frame_share) = frame_share.as_mut()
                {
                    match frame_share.read(sequence) {
                        Ok(Some(video)) => {
                            shared.video_accepted.fetch_add(1, Ordering::Relaxed);
                            if !video.person_mask.is_empty() {
                                shared.video_with_mask.fetch_add(1, Ordering::Relaxed);
                            }
                            *shared
                                .latest_video
                                .lock()
                                .expect("tracking video mutex poisoned") = Some(Arc::new(video));
                        }
                        Ok(None) => {}
                        Err(error) => {
                            shared.rejected.fetch_add(1, Ordering::Relaxed);
                            log::warn!("Vision shared frame rejected: {error:#}");
                        }
                    }
                }
            }
            Err(error) => {
                log::warn!("Vision bridge read failed: {error}");
                break;
            }
        }
    }
}

fn accept_line(bytes: &[u8], shared: &Shared) -> Option<u64> {
    let Ok(mut frame) = serde_json::from_slice::<TrackingFrame>(bytes) else {
        shared.rejected.fetch_add(1, Ordering::Relaxed);
        log::warn!("Vision bridge emitted malformed TrackingFrame JSON");
        return None;
    };
    let controls = shared
        .latest_controls
        .lock()
        .expect("tracking controls mutex poisoned")
        .as_ref()
        .filter(|controls| controls.received_at.elapsed() <= Duration::from_millis(200))
        .cloned();
    if let Some(controls) = controls {
        if let Some(face) = controls.face {
            frame.face = Some(face);
        }
        let body_is_new =
            controls.captured_at_ns > shared.last_body_controls_merged_ns.load(Ordering::Relaxed);
        if body_is_new && let Some(body) = controls.body {
            shared
                .last_body_controls_merged_ns
                .store(controls.captured_at_ns, Ordering::Relaxed);
            frame.body_space = controls
                .body_space
                .unwrap_or(BodyCoordinateSpace::ImageNormalized);
            frame.body = body;
        }
        if let Some(hands) = controls.hands {
            frame.hands = hands;
        }
    }
    if frame.body.confident_joint_count(0.3) >= 6 {
        shared.pose_backend_detected.fetch_add(1, Ordering::Relaxed);
    }
    if frame.hands.iter().any(|hand| hand.confidence >= 0.3) {
        shared.hand_backend_detected.fetch_add(1, Ordering::Relaxed);
    }
    if let Err(error) = frame.validate() {
        shared.rejected.fetch_add(1, Ordering::Relaxed);
        log::warn!("Vision bridge emitted invalid TrackingFrame: {error:?}");
        return None;
    }

    let mut latest = shared
        .latest
        .lock()
        .expect("tracking latest mutex poisoned");
    if latest
        .as_ref()
        .is_some_and(|previous| frame.sequence <= previous.sequence)
    {
        shared.rejected.fetch_add(1, Ordering::Relaxed);
        log::warn!(
            "Vision bridge emitted non-monotonic sequence {}",
            frame.sequence
        );
        return None;
    }
    let sequence = frame.sequence;
    let captured_at_ns = frame.captured_at_ns;
    let has_face = frame.face.is_some();
    *latest = Some(frame);
    let _ = shared.first_capture_ns.compare_exchange(
        0,
        captured_at_ns,
        Ordering::Relaxed,
        Ordering::Relaxed,
    );
    shared
        .last_capture_ns
        .store(captured_at_ns, Ordering::Relaxed);
    shared.accepted.fetch_add(1, Ordering::Relaxed);
    if has_face {
        shared.face_accepted.fetch_add(1, Ordering::Relaxed);
    }
    Some(sequence)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocket_live_core::{BodyJoint, TrackedPoint3};

    const MOCK: &[u8] = br#"{"body":{"joints":[null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null]},"body_space":"ImageNormalized","captured_at_ns":1000000000,"face":{"brow_raise":0.1,"confidence":0.9,"eye_blink":[0.2,0.3],"eye_look":[0.1,-0.1],"head_rotation_radians":[0.0,0.0,0.0],"mouth_open":0.2,"smile":0.1},"hands":[{"confidence":0,"handedness":"Left","points":[null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null]},{"confidence":0,"handedness":"Right","points":[null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null,null]}],"image_size":[1920,1080],"schema_version":3,"sequence":1}"#;

    #[test]
    fn accepts_bridge_contract_and_rejects_replay() {
        let shared = Shared::default();
        accept_line(MOCK, &shared);
        accept_line(MOCK, &shared);
        assert_eq!(shared.accepted.load(Ordering::Relaxed), 1);
        assert_eq!(shared.rejected.load(Ordering::Relaxed), 1);
        assert_eq!(shared.latest.lock().unwrap().as_ref().unwrap().sequence, 1);
    }

    #[test]
    fn rejects_malformed_json() {
        let shared = Shared::default();
        accept_line(b"not json", &shared);
        assert_eq!(shared.accepted.load(Ordering::Relaxed), 0);
        assert_eq!(shared.rejected.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn each_mediapipe_body_sample_is_merged_only_once() {
        let shared = Shared::default();
        let mut body = BodyObservation::default();
        body.set(
            BodyJoint::Root,
            Some(TrackedPoint3 {
                position: [0.0, 1.0, 0.0],
                confidence: 0.9,
            }),
        );
        *shared.latest_controls.lock().unwrap() = Some(TimedControls {
            captured_at_ns: 2_000_000_000,
            face: None,
            body: Some(body),
            body_space: Some(BodyCoordinateSpace::CameraRelativeMeters),
            hands: None,
            received_at: Instant::now(),
        });

        let mut first: TrackingFrame = serde_json::from_slice(MOCK).unwrap();
        first.face = None;
        accept_line(&serde_json::to_vec(&first).unwrap(), &shared);
        assert_eq!(
            shared
                .latest
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .body
                .confident_joint_count(0.3),
            1
        );

        let mut second = first;
        second.sequence = 2;
        second.captured_at_ns += 33_000_000;
        accept_line(&serde_json::to_vec(&second).unwrap(), &shared);
        assert_eq!(
            shared
                .latest
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .body
                .confident_joint_count(0.3),
            0
        );
    }
}
