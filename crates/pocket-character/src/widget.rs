//! The widget game: owns the character's per-tick pipeline.
//!
//! Tick order mirrors airi's VRMModel update (mixer → humanoid → lookAt →
//! blink → expressions → constraints → springs), mapped onto the Pocket
//! shape: sample clip locals → eye look-at → spring bones → globals →
//! palette; blink lands as morph weights, uploaded only when it changes.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use glam::{EulerRot, Mat4, Quat, Vec3};
use pocket_character_core::{CharacterSim, TrackingMode};
use pocket_live_core::{
    BodyCalibration, BodyCoordinateSpace, BodyJoint, BodyObservation, BodyPoseStabilizer,
    BodyStabilizerConfig, CalibrationAccumulator, FaceCalibrationAccumulator, FaceControlFilter,
    FaceNeutralCalibration, FaceObservation, GestureEvent, GestureGate, GestureGateConfig,
    Handedness, OneEuroConfig, TrackingLifecycle, TrackingLifecycleConfig, TrackingState,
    rotation_between, web_shoot_score,
};
use pocket_vrm::{SpringSolver, VrmDoc};
use pocket3d::anim::NodeTrs;
use pocket3d::app::Game;
use pocket3d::camera::Camera;
use pocket3d::gpu::Gpu;
use pocket3d::hud::Hud;
use pocket3d::input::Input;
use pocket3d::model::{ModelAsset, ModelInstance, ModelLoadOptions};
use pocket3d::renderer::Renderer;
use pocket3d::scene::Scene;

use crate::compositor::{BackgroundMode, CompositorConfig, VideoCompositor};
use crate::guest::{CharacterGuest, Command, TickEvent, TickState};
use crate::plugins::CharacterRenderConfig;
use crate::tracking::{TrackingClient, VisionLaunch};

pub struct WidgetConfig {
    pub model_path: PathBuf,
    pub vrma_path: PathBuf,
    pub bundle_path: PathBuf,
    pub render: CharacterRenderConfig,
    pub size: (u32, u32),
    /// Render N frames then exit (verification runs).
    pub frames: Option<u32>,
    /// Ignore these initial window frames in paced performance statistics.
    pub frame_warmup: u32,
    pub vision: Option<VisionLaunch>,
    pub compositor: Option<CompositorConfig>,
}

/// Rolling frame stats fed to the guest and to the measurement harness.
struct FrameStats {
    frames: u32,
    cpu_ms_acc: f32,
    window_start: Instant,
    pub fps: f32,
    pub frame_ms: f32,
}

impl FrameStats {
    fn new() -> Self {
        Self {
            frames: 0,
            cpu_ms_acc: 0.0,
            window_start: Instant::now(),
            fps: 0.0,
            frame_ms: 0.0,
        }
    }

    fn record(&mut self, cpu_ms: f32) {
        self.frames += 1;
        self.cpu_ms_acc += cpu_ms;
        let elapsed = self.window_start.elapsed().as_secs_f32();
        if elapsed >= 1.0 {
            self.fps = self.frames as f32 / elapsed;
            self.frame_ms = self.cpu_ms_acc / self.frames.max(1) as f32;
            self.frames = 0;
            self.cpu_ms_acc = 0.0;
            self.window_start = Instant::now();
        }
    }
}

pub struct Widget {
    cfg: WidgetConfig,
    guest: Option<CharacterGuest>,

    // Loaded in init (needs the GPU).
    model: Option<Arc<ModelAsset>>,
    vrm: Option<VrmDoc>,
    clips: Vec<(String, pocket3d::anim::Clip)>,
    springs: Option<SpringSolver>,

    // Pose pipeline state.
    sim: CharacterSim,
    locals: Vec<NodeTrs>,
    globals: Vec<Mat4>,
    clip_index: usize,
    clip_time: f32,
    clip_looping: bool,
    blink_binds: Vec<(usize, usize, f32)>, // (morph mesh slot, target, weight)
    blink_left_binds: Vec<(usize, usize, f32)>,
    blink_right_binds: Vec<(usize, usize, f32)>,

    scene: Scene,
    camera: Camera,
    hud: Hud,
    anchor: Vec3,

    stats: FrameStats,
    tick_count: u64,
    hovered: bool,
    pending_events: Vec<TickEvent>,
    exit: bool,
    rendered_frames: u32,
    render_started: Option<Instant>,
    last_render_at: Option<Instant>,
    render_intervals_ms: Vec<f64>,

    // Local Vision tracking. The helper/client boundary retains only the
    // newest complete frame; filtering and pose math run in this host.
    tracking: Option<TrackingClient>,
    tracking_clock: Instant,
    tracking_last_received: Option<Instant>,
    face_last_received: Option<Instant>,
    last_tracking_sequence: u64,
    body_filter: BodyPoseStabilizer,
    tracked_body: Option<BodyObservation>,
    tracked_face: Option<FaceObservation>,
    face_filter: FaceControlFilter,
    face_calibration_accumulator: FaceCalibrationAccumulator,
    face_calibration: Option<FaceNeutralCalibration>,
    tracked_body_space: BodyCoordinateSpace,
    calibration_accumulator: CalibrationAccumulator,
    calibration: Option<BodyCalibration>,
    tracking_lifecycle: TrackingLifecycle,
    left_arm_lifecycle: TrackingLifecycle,
    right_arm_lifecycle: TrackingLifecycle,
    face_lifecycle: TrackingLifecycle,
    torso_observed: bool,
    left_arm_observed: bool,
    right_arm_observed: bool,
    torso_weight: f32,
    left_arm_weight: f32,
    right_arm_weight: f32,
    face_weight: f32,
    tracking_weight: f32,
    gesture_gates: [GestureGate; 2],
    compositor: Option<VideoCompositor>,
}

impl Widget {
    pub fn new(cfg: WidgetConfig) -> Self {
        // Seed fixed for reproducible measurement runs; behavior parity is
        // distributional, not per-run.
        let sim = CharacterSim::new(0x0c9a_11e0, Vec3::ZERO);
        Self {
            cfg,
            guest: None,
            model: None,
            vrm: None,
            clips: Vec::new(),
            springs: None,
            sim,
            locals: Vec::new(),
            globals: Vec::new(),
            clip_index: 0,
            clip_time: 0.0,
            clip_looping: true,
            blink_binds: Vec::new(),
            blink_left_binds: Vec::new(),
            blink_right_binds: Vec::new(),
            scene: Scene::default(),
            camera: Camera::default(),
            hud: Hud::default(),
            anchor: Vec3::ZERO,
            stats: FrameStats::new(),
            tick_count: 0,
            hovered: false,
            pending_events: Vec::new(),
            exit: false,
            rendered_frames: 0,
            render_started: None,
            last_render_at: None,
            render_intervals_ms: Vec::new(),
            tracking: None,
            tracking_clock: Instant::now(),
            tracking_last_received: None,
            face_last_received: None,
            last_tracking_sequence: 0,
            body_filter: BodyPoseStabilizer::new(
                OneEuroConfig {
                    min_cutoff: 0.9,
                    beta: 0.06,
                    derivative_cutoff: 1.0,
                },
                BodyStabilizerConfig::default(),
            ),
            tracked_body: None,
            tracked_face: None,
            face_filter: FaceControlFilter::new(OneEuroConfig {
                min_cutoff: 1.8,
                beta: 0.12,
                derivative_cutoff: 1.0,
            }),
            face_calibration_accumulator: FaceCalibrationAccumulator::new(20),
            face_calibration: None,
            tracked_body_space: BodyCoordinateSpace::ImageNormalized,
            calibration_accumulator: CalibrationAccumulator::new(60),
            calibration: None,
            tracking_lifecycle: TrackingLifecycle::new(TrackingLifecycleConfig::default()),
            left_arm_lifecycle: TrackingLifecycle::new(TrackingLifecycleConfig {
                acquire_frames: 3,
                hold_ns: 2_000_000_000,
                recover_ns: 1_200_000_000,
            }),
            right_arm_lifecycle: TrackingLifecycle::new(TrackingLifecycleConfig {
                acquire_frames: 3,
                hold_ns: 2_000_000_000,
                recover_ns: 1_200_000_000,
            }),
            face_lifecycle: TrackingLifecycle::new(TrackingLifecycleConfig::default()),
            torso_observed: false,
            left_arm_observed: false,
            right_arm_observed: false,
            torso_weight: 0.0,
            left_arm_weight: 0.0,
            right_arm_weight: 0.0,
            face_weight: 0.0,
            tracking_weight: 0.0,
            gesture_gates: [GestureGate::new(GestureGateConfig::default()); 2],
            compositor: None,
        }
    }

    fn update_tracking(&mut self) {
        let now_ns = self
            .tracking_clock
            .elapsed()
            .as_nanos()
            .min(u64::MAX as u128) as u64;
        if let Some(frame) = self
            .tracking
            .as_ref()
            .and_then(TrackingClient::latest)
            .filter(|frame| frame.sequence > self.last_tracking_sequence)
        {
            self.last_tracking_sequence = frame.sequence;
            let received_at = Instant::now();
            self.tracking_last_received = Some(received_at);
            let frame_has_body = frame.body.confident_joint_count(0.3) > 0;
            if frame_has_body && frame.body_space != self.tracked_body_space {
                log::info!(
                    "body coordinate space changed: {:?} -> {:?}; resetting pose filters",
                    self.tracked_body_space,
                    frame.body_space
                );
                self.body_filter.reset();
                self.tracked_body = None;
                self.calibration_accumulator.reset();
                self.calibration = None;
                self.tracking_lifecycle.reset();
                self.left_arm_lifecycle.reset();
                self.right_arm_lifecycle.reset();
                self.torso_observed = false;
                self.left_arm_observed = false;
                self.right_arm_observed = false;
                self.torso_weight = 0.0;
                self.left_arm_weight = 0.0;
                self.right_arm_weight = 0.0;
            }
            // The Swift capture bridge intentionally emits an empty 2D body
            // while MediaPipe owns pose estimation. A temporarily stale
            // MediaPipe result must not make that empty carrier frame reset a
            // live 3D filter back to image space.
            if frame_has_body {
                self.tracked_body_space = frame.body_space;
            }
            if let Some(face) = frame.face {
                let time = frame.captured_at_ns as f64 / 1_000_000_000.0;
                if self.face_calibration.is_none()
                    && let Some(calibration) = self.face_calibration_accumulator.push(face)
                {
                    log::info!(
                        "face calibration complete: pitch={:.1}° yaw={:.1}° samples={}",
                        calibration.head_rotation_radians[0].to_degrees(),
                        calibration.head_rotation_radians[1].to_degrees(),
                        calibration.samples,
                    );
                    self.face_calibration = Some(calibration);
                }
                // MediaPipe blendshapes and the facial transform are already
                // useful before neutral calibration completes. The previous
                // all-zero fallback made every facial control look dead when
                // the user started with a smile, open mouth, or turned head
                // and therefore never satisfied the neutral gate.
                let normalized = self
                    .face_calibration
                    .map_or(face, |calibration| calibration.normalize(face));
                self.tracked_face = Some(self.face_filter.filter(time, normalized));
                self.face_last_received = Some(received_at);
            }
            self.torso_observed = torso_observed(&frame.body);
            let left_arm_acquired = self.tracked_body.as_ref().is_some_and(|body| {
                arm_chain_available(
                    body,
                    [
                        BodyJoint::LeftShoulder,
                        BodyJoint::LeftElbow,
                        BodyJoint::LeftWrist,
                    ],
                )
            });
            let right_arm_acquired = self.tracked_body.as_ref().is_some_and(|body| {
                arm_chain_available(
                    body,
                    [
                        BodyJoint::RightShoulder,
                        BodyJoint::RightElbow,
                        BodyJoint::RightWrist,
                    ],
                )
            });
            self.left_arm_observed = arm_observed(
                &frame.body,
                [
                    BodyJoint::LeftShoulder,
                    BodyJoint::LeftElbow,
                    BodyJoint::LeftWrist,
                ],
                left_arm_acquired,
            );
            self.right_arm_observed = arm_observed(
                &frame.body,
                [
                    BodyJoint::RightShoulder,
                    BodyJoint::RightElbow,
                    BodyJoint::RightWrist,
                ],
                right_arm_acquired,
            );
            let body = self
                .body_filter
                .filter(frame.captured_at_ns as f64 / 1_000_000_000.0, &frame.body);
            if self.calibration.is_none()
                && let Some(calibration) =
                    self.calibration_accumulator.push(frame.body_space, &body)
            {
                log::info!(
                    "body calibration complete: shoulder={:.3} torso={:.3} samples={}",
                    calibration.shoulder_width,
                    calibration.torso_length,
                    calibration.samples
                );
                self.calibration = Some(calibration);
            }
            let tracked = self
                .tracked_body
                .get_or_insert_with(BodyObservation::default);
            if self.torso_observed {
                copy_joints(
                    &body,
                    tracked,
                    &[
                        BodyJoint::Root,
                        BodyJoint::Spine,
                        BodyJoint::Neck,
                        BodyJoint::Head,
                        BodyJoint::LeftShoulder,
                        BodyJoint::RightShoulder,
                    ],
                );
            }
            if self.left_arm_observed {
                copy_joints(
                    &body,
                    tracked,
                    &[
                        BodyJoint::LeftShoulder,
                        BodyJoint::LeftElbow,
                        BodyJoint::LeftWrist,
                    ],
                );
            }
            if self.right_arm_observed {
                copy_joints(
                    &body,
                    tracked,
                    &[
                        BodyJoint::RightShoulder,
                        BodyJoint::RightElbow,
                        BodyJoint::RightWrist,
                    ],
                );
            }

            for (index, hand) in frame.hands.iter().enumerate() {
                if let Some(event) = self.gesture_gates[index].update(now_ns, web_shoot_score(hand))
                {
                    let avatar_hand = mirrored_handedness(hand.handedness);
                    self.pending_events.push(match event {
                        GestureEvent::Started => TickEvent::WebShootStart(avatar_hand),
                        GestureEvent::Ended => TickEvent::WebShootEnd(avatar_hand),
                    });
                }
            }
        }

        let body_fresh = self
            .tracking_last_received
            .is_some_and(|received| received.elapsed().as_millis() <= 150);
        let face_fresh = self
            .face_last_received
            .is_some_and(|received| received.elapsed().as_millis() <= 150);
        self.torso_weight = self
            .tracking_lifecycle
            .update(now_ns, body_fresh && self.torso_observed);
        self.left_arm_weight = self
            .left_arm_lifecycle
            .update(now_ns, body_fresh && self.left_arm_observed);
        self.right_arm_weight = self
            .right_arm_lifecycle
            .update(now_ns, body_fresh && self.right_arm_observed);
        self.face_weight = self.face_lifecycle.update(now_ns, face_fresh);
        self.tracking_weight = self
            .torso_weight
            .max(self.left_arm_weight)
            .max(self.right_arm_weight)
            .max(self.face_weight);
    }

    fn apply_commands(&mut self, commands: Vec<Command>) {
        for cmd in commands {
            match cmd {
                Command::SetTracking(mode) => {
                    self.sim.tracking = match mode.as_str() {
                        "mouse" => TrackingMode::Mouse,
                        _ => TrackingMode::None,
                    };
                }
                Command::SetExpression(name, w) => {
                    let Some((vrm, model)) = self.vrm.as_ref().zip(self.model.as_ref()) else {
                        continue;
                    };
                    apply_expression(vrm, model, &mut self.scene, &name, w);
                }
                Command::PlayClip { name, looping } => {
                    if let Some(i) = self.clips.iter().position(|(n, _)| *n == name) {
                        self.clip_index = i;
                        self.clip_time = 0.0;
                        self.clip_looping = looping;
                    } else {
                        log::warn!("character.playClip: unknown clip '{name}'");
                    }
                }
                Command::SetMaxFps(_fps) => {
                    // The app loop owns pacing; a runtime-adjustable cap needs
                    // an AppConfig hook (candidate follow-up).
                    log::warn!("character.setMaxFps: fixed at launch for now");
                }
                Command::Quit => self.exit = true,
            }
        }
    }
}

/// Resolve a named VRM expression to morph weights on the instance.
fn apply_expression(vrm: &VrmDoc, model: &Arc<ModelAsset>, scene: &mut Scene, name: &str, w: f32) {
    let Some(inst) = scene.models.first_mut() else {
        return;
    };
    let Some(morph) = inst.morph.as_mut() else {
        return;
    };
    for expr in &vrm.expressions {
        if expr.name == name {
            for bind in &expr.binds {
                if let Some(slot) = model.morph_mesh_slot(bind.mesh) {
                    morph.set_weight(slot, bind.target, w * bind.weight);
                }
            }
        }
    }
}

/// Apply the first expression exposed by the current VRM from a list of
/// common VRM 0.x/authoring-tool aliases.
fn apply_first_expression(
    vrm: &VrmDoc,
    model: &Arc<ModelAsset>,
    scene: &mut Scene,
    aliases: &[&str],
    weight: f32,
) {
    let Some(name) = vrm
        .expressions
        .iter()
        .find(|expression| {
            aliases
                .iter()
                .any(|alias| expression.name.eq_ignore_ascii_case(alias))
        })
        .map(|expression| expression.name.clone())
    else {
        return;
    };
    apply_expression(vrm, model, scene, &name, weight.clamp(0.0, 1.0));
}

fn apply_tracked_face_rotation(
    vrm: &VrmDoc,
    locals: &mut [NodeTrs],
    face: FaceObservation,
    tracking_weight: f32,
) {
    let Some(head) = vrm.humanoid_node("head") else {
        return;
    };
    let delta = camera_face_to_avatar_rotation(face.head_rotation_radians);
    let weight = (tracking_weight * face.confidence).clamp(0.0, 1.0);
    locals[head].rotation =
        (locals[head].rotation * Quat::IDENTITY.slerp(delta, weight)).normalize();
}

/// Convert local face-tracker Euler angles into this VRM0 stage's bone space.
///
/// Yaw was verified to follow the person already. The imported VRM head bone
/// uses the opposite local X direction from MediaPipe's facial transform, so
/// pitch must change sign or looking up drives the avatar down. Roll follows
/// the same camera-facing handedness conversion as yaw.
fn camera_face_to_avatar_rotation([pitch, yaw, roll]: [f32; 3]) -> Quat {
    let pitch = pitch.clamp(-30f32.to_radians(), 30f32.to_radians());
    let yaw = yaw.clamp(-60f32.to_radians(), 60f32.to_radians());
    let roll = roll.clamp(-30f32.to_radians(), 30f32.to_radians());
    Quat::from_euler(EulerRot::YXZ, -yaw, -pitch, -roll)
}

fn mirrored_handedness(handedness: Handedness) -> Handedness {
    match handedness {
        Handedness::Left => Handedness::Right,
        Handedness::Right => Handedness::Left,
    }
}

fn apply_tracked_upper_body(
    model: &ModelAsset,
    vrm: &VrmDoc,
    locals: &mut [NodeTrs],
    globals: &mut Vec<Mat4>,
    body: &BodyObservation,
    body_space: BodyCoordinateSpace,
    calibration: Option<&BodyCalibration>,
    torso_weight: f32,
    left_arm_weight: f32,
    right_arm_weight: f32,
) {
    if torso_weight > 0.0 {
        apply_torso(
            model,
            vrm,
            locals,
            globals,
            body,
            body_space,
            calibration,
            torso_weight,
        );
    }
    // The front-facing camera reports anatomical left/right opposite to the
    // avatar control expected by the mirrored live view. Swap the source arm
    // chains once, here at the camera-to-avatar boundary; downstream bone
    // solving stays in normal VRM humanoid space.
    if right_arm_weight > 0.0 {
        apply_arm(
            model,
            vrm,
            locals,
            globals,
            body,
            body_space,
            calibration.map(|value| [value.right_upper_arm_length, value.right_forearm_length]),
            right_arm_weight,
            [
                BodyJoint::RightShoulder,
                BodyJoint::RightElbow,
                BodyJoint::RightWrist,
            ],
            ["leftUpperArm", "leftLowerArm", "leftHand"],
            AvatarArmSide::Left,
            true,
        );
    }
    if left_arm_weight > 0.0 {
        apply_arm(
            model,
            vrm,
            locals,
            globals,
            body,
            body_space,
            calibration.map(|value| [value.left_upper_arm_length, value.left_forearm_length]),
            left_arm_weight,
            [
                BodyJoint::LeftShoulder,
                BodyJoint::LeftElbow,
                BodyJoint::LeftWrist,
            ],
            ["rightUpperArm", "rightLowerArm", "rightHand"],
            AvatarArmSide::Right,
            true,
        );
    }
}

fn arm_chain_available(body: &BodyObservation, joints: [BodyJoint; 3]) -> bool {
    joints.iter().all(|joint| {
        body.get(*joint)
            .is_some_and(|point| point.confidence >= 0.3)
    })
}

fn arm_observed(body: &BodyObservation, joints: [BodyJoint; 3], chain_was_acquired: bool) -> bool {
    let observed = joints
        .iter()
        .filter(|joint| {
            body.get(**joint)
                .is_some_and(|point| point.confidence >= 0.3)
        })
        .count();

    // Initially require a complete shoulder/elbow/wrist chain. Afterwards the
    // stabilizer can safely retain missing parent joints while a new wrist or
    // any two joints update the existing chain. This avoids releasing the arm
    // whenever monocular Vision misses one point for a frame.
    observed == joints.len()
        || (chain_was_acquired
            && (observed >= 2
                || body
                    .get(joints[2])
                    .is_some_and(|point| point.confidence >= 0.3)))
}

fn torso_observed(body: &BodyObservation) -> bool {
    let head_chain = [BodyJoint::Neck, BodyJoint::Head].iter().all(|joint| {
        body.get(*joint)
            .is_some_and(|point| point.confidence >= 0.3)
    });
    let shoulders = [BodyJoint::LeftShoulder, BodyJoint::RightShoulder]
        .iter()
        .all(|joint| {
            body.get(*joint)
                .is_some_and(|point| point.confidence >= 0.3)
        });
    head_chain || shoulders
}

fn copy_joints(from: &BodyObservation, to: &mut BodyObservation, joints: &[BodyJoint]) {
    for joint in joints {
        if let Some(point) = from.get(*joint) {
            to.set(*joint, Some(point));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_torso(
    model: &ModelAsset,
    vrm: &VrmDoc,
    locals: &mut [NodeTrs],
    globals: &mut Vec<Mat4>,
    body: &BodyObservation,
    body_space: BodyCoordinateSpace,
    calibration: Option<&BodyCalibration>,
    weight: f32,
) {
    if let Some(calibration) = calibration
        && calibration.body_space == body_space
        && (!segment_plausible(
            body,
            BodyJoint::Root,
            BodyJoint::Neck,
            calibration.torso_length,
        ) || !segment_plausible(
            body,
            BodyJoint::LeftShoulder,
            BodyJoint::RightShoulder,
            calibration.shoulder_width,
        ))
    {
        return;
    }

    let Some(torso_target) = tracked_direction(body, body_space, BodyJoint::Root, BodyJoint::Neck)
    else {
        return;
    };
    let Some(spine) = vrm.humanoid_node("spine") else {
        return;
    };
    let chest = vrm
        .humanoid_node("chest")
        .or_else(|| vrm.humanoid_node("upperChest"))
        .or_else(|| vrm.humanoid_node("neck"));
    if let Some(chest) = chest {
        apply_bone_direction(
            model,
            locals,
            globals,
            spine,
            chest,
            torso_target,
            weight * 0.45,
            35f32.to_radians(),
        );
    }

    // The shoulder line adds roll in 2D and roll/yaw in Vision 3D. Blend it
    // gently into the chest so noisy monocular depth cannot twist the body.
    if let (Some(chest), Some(left), Some(right), Some(shoulder_target)) = (
        vrm.humanoid_node("chest")
            .or_else(|| vrm.humanoid_node("upperChest")),
        vrm.humanoid_node("leftShoulder"),
        vrm.humanoid_node("rightShoulder"),
        tracked_direction(
            body,
            body_space,
            BodyJoint::LeftShoulder,
            BodyJoint::RightShoulder,
        ),
    ) {
        model.skeleton.globals_from_locals(locals, globals);
        let current = globals[right].w_axis.truncate() - globals[left].w_axis.truncate();
        apply_orientation_delta(
            model,
            locals,
            chest,
            current,
            shoulder_target,
            weight * 0.35,
            30f32.to_radians(),
        );
    }

    if let (Some(neck), Some(head), Some(head_target)) = (
        vrm.humanoid_node("neck"),
        vrm.humanoid_node("head"),
        tracked_direction(body, body_space, BodyJoint::Neck, BodyJoint::Head),
    ) {
        apply_bone_direction(
            model,
            locals,
            globals,
            neck,
            head,
            head_target,
            weight * 0.3,
            25f32.to_radians(),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_arm(
    model: &ModelAsset,
    vrm: &VrmDoc,
    locals: &mut [NodeTrs],
    globals: &mut Vec<Mat4>,
    body: &BodyObservation,
    body_space: BodyCoordinateSpace,
    expected_lengths: Option<[f32; 2]>,
    weight: f32,
    joints: [BodyJoint; 3],
    bones: [&str; 3],
    side: AvatarArmSide,
    mirror_horizontal: bool,
) {
    if let Some(expected) = expected_lengths
        && (!segment_plausible(body, joints[0], joints[1], expected[0])
            || !segment_plausible(body, joints[1], joints[2], expected[1]))
    {
        return;
    }
    let Some(upper_target) =
        tracked_arm_direction(body, body_space, joints[0], joints[1], mirror_horizontal)
    else {
        return;
    };
    let Some(lower_target) =
        tracked_arm_direction(body, body_space, joints[1], joints[2], mirror_horizontal)
    else {
        return;
    };
    let upper_target = constrain_upper_arm_target(upper_target, side);
    let (Some(upper), Some(lower), Some(hand)) = (
        vrm.humanoid_node(bones[0]),
        vrm.humanoid_node(bones[1]),
        vrm.humanoid_node(bones[2]),
    ) else {
        return;
    };

    // High arm elevation should rotate the clavicle as well as the upper arm.
    // Otherwise a loose sleeve is dragged through the neck by a single joint.
    let shoulder_name = match side {
        AvatarArmSide::Left => "leftShoulder",
        AvatarArmSide::Right => "rightShoulder",
    };
    let shoulder_lift = ((upper_target.y - 0.15) / 0.7).clamp(0.0, 1.0);
    if shoulder_lift > 0.0
        && let Some(shoulder) = vrm.humanoid_node(shoulder_name)
    {
        apply_bone_direction(
            model,
            locals,
            globals,
            shoulder,
            upper,
            upper_target,
            weight * shoulder_lift * 0.35,
            25f32.to_radians(),
        );
    }

    apply_bone_direction(
        model,
        locals,
        globals,
        upper,
        lower,
        upper_target,
        weight,
        120f32.to_radians(),
    );
    apply_bone_direction(
        model,
        locals,
        globals,
        lower,
        hand,
        lower_target,
        weight,
        120f32.to_radians(),
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AvatarArmSide {
    Left,
    Right,
}

fn constrain_upper_arm_target(mut target: Vec3, side: AvatarArmSide) -> Vec3 {
    // Allow modest adduction, but do not let a noisy monocular solve rotate an
    // upper arm all the way through the neck or torso. When the intended pose
    // is across the body, bend it toward the camera so it passes in front.
    let inward = match side {
        AvatarArmSide::Left => target.x.max(0.0),
        AvatarArmSide::Right => (-target.x).max(0.0),
    };
    // Smoothstep is essential here: a hard inward threshold makes a
    // continuous hand sweep jump abruptly from torso-plane to front-plane.
    let blend = smoothstep01((inward - 0.05) / 0.8);
    let capped_inward = match side {
        AvatarArmSide::Left => 0.2,
        AvatarArmSide::Right => -0.2,
    };
    target.x += (capped_inward - target.x) * blend;
    let front_target = target.z.min(-0.8);
    target.z += (front_target - target.z) * blend;
    target.normalize_or_zero()
}

fn smoothstep01(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn tracked_arm_direction(
    body: &BodyObservation,
    body_space: BodyCoordinateSpace,
    from: BodyJoint,
    to: BodyJoint,
    mirror_horizontal: bool,
) -> Option<Vec3> {
    let direction = tracked_direction(body, body_space, from, to)?;
    // Swapping anatomical source chains is only half of a mirrored-camera
    // mapping. Reflect their directions too; otherwise a source right arm
    // applied to the avatar's left bones points inward through the torso.
    Some(if mirror_horizontal {
        Vec3::new(-direction.x, direction.y, direction.z)
    } else {
        direction
    })
}

fn segment_plausible(
    body: &BodyObservation,
    from: BodyJoint,
    to: BodyJoint,
    expected: f32,
) -> bool {
    let (Some(from), Some(to)) = (body.get(from), body.get(to)) else {
        return false;
    };
    let length = Vec3::from_array(to.position).distance(Vec3::from_array(from.position));
    length.is_finite()
        && expected.is_finite()
        && length >= expected * 0.45
        && length <= expected * 2.0
}

fn tracked_direction(
    body: &BodyObservation,
    body_space: BodyCoordinateSpace,
    from: BodyJoint,
    to: BodyJoint,
) -> Option<Vec3> {
    let from = body.get(from)?;
    let to = body.get(to)?;
    if from.confidence < 0.3 || to.confidence < 0.3 {
        return None;
    }
    let delta = Vec3::from_array(to.position) - Vec3::from_array(from.position);
    // Vision image X grows to the right. The Pocket VRM0 camera views the
    // -Z-facing model from -Z, so screen-right maps to model -X.
    let model_delta = match body_space {
        BodyCoordinateSpace::ImageNormalized => Vec3::new(-delta.x, delta.y, delta.z),
        // Camera/world pose Z decreases toward the camera. Pocket's avatar
        // also faces toward -Z, so depth retains its sign while X flips into
        // the model's screen orientation.
        BodyCoordinateSpace::CameraRelativeMeters => Vec3::new(-delta.x, delta.y, delta.z),
    };
    (model_delta.length_squared() > 1e-8).then_some(model_delta.normalize())
}

fn apply_bone_direction(
    model: &ModelAsset,
    locals: &mut [NodeTrs],
    globals: &mut Vec<Mat4>,
    bone: usize,
    child: usize,
    target: Vec3,
    weight: f32,
    max_angle: f32,
) {
    model.skeleton.globals_from_locals(locals, globals);
    let current = globals[child].w_axis.truncate() - globals[bone].w_axis.truncate();
    if current.length_squared() <= 1e-8 {
        return;
    }
    let delta = rotation_between(current, target);
    let (axis, angle) = delta.to_axis_angle();
    let delta = if axis.is_finite() && angle.is_finite() {
        Quat::from_axis_angle(axis, angle.min(max_angle))
    } else {
        Quat::IDENTITY
    };
    // Do not decompose the global matrix here. Imported VRM hierarchies can
    // contain non-uniform or reflected scale, and matrix decomposition can
    // feed that reflection back as a bogus bone rotation. Compose only the
    // local quaternions along the parent chain.
    let current_global = global_rotation(model, locals, bone);
    let desired_global = (delta * current_global).normalize();
    let parent = model.skeleton.parents[bone];
    let parent_global = if parent == usize::MAX {
        Quat::IDENTITY
    } else {
        global_rotation(model, locals, parent)
    };
    let desired_local = (parent_global.inverse() * desired_global).normalize();
    locals[bone].rotation = locals[bone]
        .rotation
        .slerp(desired_local, weight.clamp(0.0, 1.0));
}

fn apply_orientation_delta(
    model: &ModelAsset,
    locals: &mut [NodeTrs],
    bone: usize,
    current: Vec3,
    target: Vec3,
    weight: f32,
    max_angle: f32,
) {
    if current.length_squared() <= 1e-8 || target.length_squared() <= 1e-8 {
        return;
    }
    let delta = rotation_between(current, target);
    let (axis, angle) = delta.to_axis_angle();
    if !axis.is_finite() || !angle.is_finite() {
        return;
    }
    let delta = Quat::from_axis_angle(axis, angle.min(max_angle));
    let current_global = global_rotation(model, locals, bone);
    let desired_global = (delta * current_global).normalize();
    let parent = model.skeleton.parents[bone];
    let parent_global = if parent == usize::MAX {
        Quat::IDENTITY
    } else {
        global_rotation(model, locals, parent)
    };
    let desired_local = (parent_global.inverse() * desired_global).normalize();
    locals[bone].rotation = locals[bone]
        .rotation
        .slerp(desired_local, weight.clamp(0.0, 1.0));
}

fn global_rotation(model: &ModelAsset, locals: &[NodeTrs], node: usize) -> Quat {
    const MAX_HIERARCHY_DEPTH: usize = 64;
    let mut chain = [usize::MAX; MAX_HIERARCHY_DEPTH];
    let mut len = 0;
    let mut cursor = node;
    while cursor != usize::MAX && len < MAX_HIERARCHY_DEPTH {
        chain[len] = cursor;
        len += 1;
        cursor = model.skeleton.parents[cursor];
    }
    let mut rotation = Quat::IDENTITY;
    while len > 0 {
        len -= 1;
        rotation = (rotation * locals[chain[len]].rotation).normalize();
    }
    rotation
}

fn tracking_state_name(state: TrackingState) -> &'static str {
    match state {
        TrackingState::Idle => "idle",
        TrackingState::Acquiring => "acquiring",
        TrackingState::Tracking => "tracking",
        TrackingState::Holding => "holding",
        TrackingState::Recovering => "recovering",
    }
}

impl Game for Widget {
    fn init(&mut self, gpu: &Gpu, renderer: &mut Renderer) -> Result<()> {
        let t0 = Instant::now();
        // Texture budget belongs to the character plugin because authoring
        // resolution and the intended framing are properties of that asset.
        let model = ModelAsset::load_glb_opts(
            gpu,
            &renderer.model_material_layout,
            &renderer.samplers,
            &self.cfg.model_path,
            &ModelLoadOptions {
                max_texture_dim: Some(self.cfg.render.max_texture_dimension),
            },
        )
        .context("loading VRM model")?;
        let vrm = VrmDoc::from_path(&self.cfg.model_path).context("parsing VRM extension")?;

        // Retarget the idle animation onto this rig.
        let vrma_bytes = std::fs::read(&self.cfg.vrma_path).context("reading vrma")?;
        let vrma = pocket_vrm::load_vrma_bytes(&vrma_bytes)?;
        let clip = pocket_vrm::retarget(&vrma, &vrm.humanoid, &model.skeleton)?;
        let clip_name = self
            .cfg
            .vrma_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "idle".into());
        self.clips = vec![(clip_name, clip)];

        // Springs seeded from the rest pose.
        model
            .skeleton
            .sample_locals(None, 0.0, false, &mut self.locals);
        self.springs = Some(SpringSolver::new(
            &vrm.springs,
            &model.skeleton,
            &self.locals,
        ));

        // Blink expressions → morph slots. Prefer independent left/right
        // presets when the model exposes both; fall back to the generic VRM0
        // `blink` preset for simpler rigs.
        for expr in &vrm.expressions {
            let target = if expr.name.eq_ignore_ascii_case("blink") {
                Some(&mut self.blink_binds)
            } else if ["blink_l", "blinkleft", "leftblink"]
                .iter()
                .any(|name| expr.name.eq_ignore_ascii_case(name))
            {
                Some(&mut self.blink_left_binds)
            } else if ["blink_r", "blinkright", "rightblink"]
                .iter()
                .any(|name| expr.name.eq_ignore_ascii_case(name))
            {
                Some(&mut self.blink_right_binds)
            } else {
                None
            };
            if let Some(target) = target {
                for b in &expr.binds {
                    if let Some(slot) = model.morph_mesh_slot(b.mesh) {
                        target.push((slot, b.target, b.weight));
                    }
                }
            }
        }
        let split_blink = !self.blink_left_binds.is_empty() && !self.blink_right_binds.is_empty();
        if self.blink_binds.is_empty() && !split_blink {
            log::warn!("model has no 'blink' expression; blinking disabled");
        }

        // Scene: one instance, transparent background, near-unlit shading
        // (MToon reads mostly flat; sun/hemisphere would double-shade it).
        let mut inst = ModelInstance::new(model.clone());
        inst.morph = model.create_morph_state(gpu);
        inst.cutout = 0.5;
        inst.lit = 0.25;
        self.scene.transparent_clear = true;
        self.scene.models.push(inst);
        if let Some(config) = self.cfg.compositor.clone() {
            self.compositor = Some(VideoCompositor::new(gpu, renderer.color_format, config));
        }

        // Camera: airi's VRM defaults — fov 40°, 1 m from the model anchor
        // on the -Z side (VRM0 rigs face -Z; airi's default camera sits at
        // z = -1 too).
        // Anchor at chest height (airi frames the bust: head fills the top
        // of its 450×600 stage) rather than the AABB midpoint.
        let aabb = model.aabb;
        let height = aabb.1.y - aabb.0.y;
        self.anchor = Vec3::new(
            0.0,
            aabb.0.y + height * self.cfg.render.anchor_height_ratio,
            0.0,
        );
        self.camera.fov_y = self.cfg.render.fov_y_degrees.to_radians();
        self.camera.znear = 0.05;
        let split_screen = self
            .cfg
            .compositor
            .as_ref()
            .is_some_and(|config| config.mode == BackgroundMode::Split);
        let camera_distance = if split_screen {
            self.cfg.render.split_camera_distance
        } else {
            self.cfg.render.camera_distance
        };
        self.camera.pos = self.anchor + Vec3::new(0.0, 0.0, -camera_distance);
        self.camera.look_at(self.anchor);
        if split_screen {
            let aspect = self.cfg.size.0 as f32 / self.cfg.size.1.max(1) as f32;
            let distance = (self.camera.pos - self.anchor).length();
            let half_visible_width = distance * (self.camera.fov_y * 0.5).tan() * aspect;
            // This VRM faces +Z from a camera whose screen-right vector is
            // world -X, hence the negative world translation.
            self.scene.models[0].transform =
                Mat4::from_translation(Vec3::new(-half_visible_width * 0.5, 0.0, 0.0));
        }
        self.sim.look_base = self.camera.pos;
        self.sim.mouse_target = self.camera.pos;

        // Guest boots last so its boot table reflects the loaded assets.
        let bundle = std::fs::read_to_string(&self.cfg.bundle_path)
            .with_context(|| format!("reading bundle {}", self.cfg.bundle_path.display()))?;
        let clip_names: Vec<String> = self.clips.iter().map(|(n, _)| n.clone()).collect();
        let expr_names: Vec<String> = vrm.expressions.iter().map(|e| e.name.clone()).collect();
        let model_name = vrm
            .meta
            .title
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .map(str::to_owned)
            .or_else(|| {
                self.cfg
                    .model_path
                    .file_stem()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "character".into());
        log::info!(
            "model: name={model_name} expressions={} split_blink={split_blink}",
            expr_names.join(",")
        );
        self.guest = Some(CharacterGuest::boot(
            &bundle,
            &model_name,
            &clip_names,
            &expr_names,
        )?);

        self.vrm = Some(vrm);
        self.model = Some(model);
        if let Some(launch) = &self.cfg.vision {
            let tracking = TrackingClient::spawn(launch).context("starting local tracking")?;
            if launch.waits_for_first_frame()
                && !tracking.wait_for_first(std::time::Duration::from_secs(1))
            {
                log::warn!("timed out waiting for mock Vision frame");
            }
            self.tracking = Some(tracking);
            self.tracking_clock = Instant::now();
            log::info!("local Vision tracking enabled");
        }
        log::info!("init: {:.0} ms", t0.elapsed().as_secs_f32() * 1000.0);
        Ok(())
    }

    fn frame(&mut self, _dt: f32, input: &Input) {
        let hovered = input.cursor().is_some();
        if hovered != self.hovered {
            self.hovered = hovered;
            self.pending_events.push(if hovered {
                TickEvent::HoverStart
            } else {
                TickEvent::HoverEnd
            });
        }
    }

    fn tick(&mut self, dt: f32, input: &Input) {
        let t0 = Instant::now();
        self.update_tracking();
        let (Some(model), Some(vrm)) = (self.model.clone(), self.vrm.as_ref()) else {
            return;
        };
        self.tick_count += 1;
        if self.tick_count.is_multiple_of(60) {
            if let Some(face) = self.tracked_face {
                log::debug!(
                    "controls: face_w={:.2} yaw={:.1}° pitch={:.1}° blink={:.2}/{:.2} mouth={:.2} smile={:.2} arms={:.2}/{:.2}",
                    self.face_weight,
                    face.head_rotation_radians[1].to_degrees(),
                    face.head_rotation_radians[0].to_degrees(),
                    face.eye_blink[0],
                    face.eye_blink[1],
                    face.mouth_open,
                    face.smile,
                    self.left_arm_weight,
                    self.right_arm_weight,
                );
            } else {
                log::debug!(
                    "controls: face=missing arms={:.2}/{:.2}",
                    self.left_arm_weight,
                    self.right_arm_weight,
                );
            }
        }
        if self.tick_count.is_multiple_of(600)
            && let Some(tracking) = &self.tracking
        {
            let (accepted, rejected) = tracking.counts();
            log::info!("tracking: accepted={accepted} rejected={rejected}");
        }

        // --- sim --------------------------------------------------------
        let out = self.sim.tick(dt);

        // --- clip -------------------------------------------------------
        self.clip_time += dt;
        let clip = self.clips.get(self.clip_index).map(|(_, c)| c);
        model
            .skeleton
            .sample_locals(clip, self.clip_time, self.clip_looping, &mut self.locals);

        // Untracked bones retain the authored idle clip. Only reliable arm
        // chains blend toward the local camera pose.
        if let Some(body) = &self.tracked_body
            && (self.torso_weight > 0.0
                || self.left_arm_weight > 0.0
                || self.right_arm_weight > 0.0)
        {
            apply_tracked_upper_body(
                &model,
                vrm,
                &mut self.locals,
                &mut self.globals,
                body,
                self.tracked_body_space,
                self.calibration.as_ref(),
                self.torso_weight,
                self.left_arm_weight,
                self.right_arm_weight,
            );
        }
        if let Some(face) = self.tracked_face
            && self.face_weight > 0.0
        {
            apply_tracked_face_rotation(vrm, &mut self.locals, face, self.face_weight);
        }

        // --- eyes -------------------------------------------------------
        // Yaw/pitch from the head toward the look target (model space).
        self.globals.resize(self.locals.len(), Mat4::IDENTITY);
        model
            .skeleton
            .globals_from_locals(&self.locals, &mut self.globals);
        let head = vrm
            .humanoid_node("head")
            .map(|n| self.globals[n].w_axis.truncate());
        if let Some(head_pos) = head {
            let tracked_eyes = self
                .tracked_face
                .filter(|face| self.face_weight > 0.0 && face.confidence >= 0.25);
            let (yaw, pitch) = if let Some(face) = tracked_eyes {
                // Landmark coordinates are normalized in the face box. Keep
                // the range deliberately small so imperfect pupil points do
                // not produce an uncanny hard stare.
                (
                    face.eye_look[0] * 15.0 * self.face_weight,
                    face.eye_look[1] * 10.0 * self.face_weight,
                )
            } else {
                // Character forward is -Z; yaw > 0 = its left (-X), pitch > 0 = up.
                let d = out.look_target - head_pos;
                (
                    (-d.x).atan2(-d.z).to_degrees(),
                    d.y.atan2(Vec3::new(d.x, 0.0, d.z).length()).to_degrees(),
                )
            };
            pocket_vrm::apply_eye_look(
                &mut self.locals,
                &model.skeleton.rest,
                vrm.humanoid_node("leftEye"),
                vrm.humanoid_node("rightEye"),
                &vrm.look_at,
                yaw,
                pitch,
            );
        }

        // --- springs ----------------------------------------------------
        if let Some(springs) = self.springs.as_mut() {
            springs.step(dt, &model.skeleton, &mut self.locals, Mat4::IDENTITY);
        }

        // --- pose + blink -----------------------------------------------
        model
            .skeleton
            .globals_from_locals(&self.locals, &mut self.globals);
        let face_weight = self.tracked_face.map_or(0.0, |face| {
            (self.face_weight * face.confidence).clamp(0.0, 1.0)
        });
        // Generic VRM blink bindings usually close both eyes together. Using
        // the stronger eye prevents a real blink from being halved when the
        // face tracker sees one eyelid a frame earlier than the other.
        let tracked_blinks = self
            .tracked_face
            .map_or([out.blink; 2], |face| face.eye_blink);
        let resolved_blinks =
            tracked_blinks.map(|tracked| out.blink * (1.0 - face_weight) + tracked * face_weight);
        let resolved_blink = resolved_blinks[0].max(resolved_blinks[1]);
        if self.tracking.is_some() {
            let (mouth_open, smile, brow_raise) =
                self.tracked_face.map_or((0.0, 0.0, 0.0), |face| {
                    (
                        face.mouth_open * face_weight,
                        face.smile * face_weight,
                        face.brow_raise * face_weight,
                    )
                });
            apply_first_expression(vrm, &model, &mut self.scene, &["aa", "a"], mouth_open);
            apply_first_expression(vrm, &model, &mut self.scene, &["happy", "joy"], smile);
            apply_first_expression(
                vrm,
                &model,
                &mut self.scene,
                &["surprised", "surprise"],
                brow_raise,
            );
        }
        let inst = &mut self.scene.models[0];
        inst.pose = Some(self.globals.clone());
        if out.blink_changed || self.tracking.is_some() {
            if let Some(morph) = inst.morph.as_mut() {
                if !self.blink_left_binds.is_empty() && !self.blink_right_binds.is_empty() {
                    for &(slot, target, w) in &self.blink_left_binds {
                        morph.set_weight(slot, target, resolved_blinks[0] * w);
                    }
                    for &(slot, target, w) in &self.blink_right_binds {
                        morph.set_weight(slot, target, resolved_blinks[1] * w);
                    }
                } else {
                    for &(slot, target, w) in &self.blink_binds {
                        morph.set_weight(slot, target, resolved_blink * w);
                    }
                }
            }
        }

        // --- guest turn -------------------------------------------------
        let mut events: Vec<TickEvent> = std::mem::take(&mut self.pending_events);
        if input.mouse_button_pressed(pocket3d::winit::event::MouseButton::Left) {
            events.push(TickEvent::Click);
        }
        let state = TickState {
            t: self.tick_count as f64 * dt as f64,
            blink: resolved_blink,
            clip: self
                .clips
                .get(self.clip_index)
                .map(|(n, _)| n.clone())
                .unwrap_or_default(),
            hovered: self.hovered,
            tracking: match self.sim.tracking {
                TrackingMode::None => "none",
                TrackingMode::Mouse => "mouse",
            },
            fps: self.stats.fps,
            frame_ms: self.stats.frame_ms,
            body_tracking: if self.tracking.is_none() {
                "off"
            } else {
                tracking_state_name(self.tracking_lifecycle.state())
            },
            tracking_weight: self.tracking_weight,
            tracking_age_ms: self
                .tracking_last_received
                .map_or(f32::INFINITY, |received| {
                    received.elapsed().as_secs_f32() * 1000.0
                }),
            calibration_progress: if self.calibration.is_some() {
                1.0
            } else {
                self.calibration_accumulator.progress()
            },
        };
        if let Some(guest) = &self.guest {
            match guest.turn(&state, &events) {
                Ok(commands) => self.apply_commands(commands),
                Err(e) => log::error!("guest turn: {e:#}"),
            }
        }

        self.stats.record(t0.elapsed().as_secs_f32() * 1000.0);
    }

    fn compose(&mut self, _alpha: f32, time: f32, size: (u32, u32)) -> (&Scene, &Camera, &Hud) {
        self.scene.time = time;
        self.rendered_frames += 1;
        let now = Instant::now();
        if self.cfg.frames.is_some() && self.rendered_frames > self.cfg.frame_warmup {
            if self.render_started.is_none() {
                self.render_started = Some(now);
                self.last_render_at = Some(now);
            } else if let Some(previous) = self.last_render_at.replace(now) {
                self.render_intervals_ms
                    .push((now - previous).as_secs_f64() * 1000.0);
            }
        }
        if let Some(n) = self.cfg.frames
            && self.rendered_frames >= n
        {
            let started = self.render_started.unwrap_or(now);
            let elapsed = started.elapsed().as_secs_f64();
            let paced_fps = self.render_intervals_ms.len() as f64 / elapsed.max(1e-6);
            let one_percent_low_fps = one_percent_low_fps(&self.render_intervals_ms);
            let (tracking_frames, rejected_tracking_frames) = self
                .tracking
                .as_ref()
                .map_or((0, 0), TrackingClient::counts);
            let (shared_video_frames, person_matte_frames) = self
                .tracking
                .as_ref()
                .map_or((0, 0), TrackingClient::video_counts);
            let observed_tracking_fps = self
                .tracking
                .as_ref()
                .map_or(0.0, TrackingClient::observed_fps);
            let face_frames = self.tracking.as_ref().map_or(0, TrackingClient::face_count);
            let (face_backend_samples, face_backend_detected) = self
                .tracking
                .as_ref()
                .map_or((0, 0), TrackingClient::face_backend_counts);
            let pose_backend_detected = self
                .tracking
                .as_ref()
                .map_or(0, TrackingClient::pose_backend_count);
            let hand_backend_detected = self
                .tracking
                .as_ref()
                .map_or(0, TrackingClient::hand_backend_count);
            println!(
                "WINDOW_BENCH {}",
                serde_json::json!({
                    "schema_version": 1,
                    "frames": self.rendered_frames,
                    "warmup_frames": self.cfg.frame_warmup,
                    "measured_frames": self.render_intervals_ms.len() + 1,
                    "seconds": elapsed,
                    "paced_fps": paced_fps,
                    "one_percent_low_fps": one_percent_low_fps,
                    "width": size.0,
                    "height": size.1,
                    "tracking_frames": tracking_frames,
                    "face_frames": face_frames,
                    "face_backend_samples": face_backend_samples,
                    "face_backend_detected": face_backend_detected,
                    "pose_backend_detected": pose_backend_detected,
                    "hand_backend_detected": hand_backend_detected,
                    "rejected_tracking_frames": rejected_tracking_frames,
                    "shared_video_frames": shared_video_frames,
                    "person_matte_frames": person_matte_frames,
                    "observed_tracking_fps": observed_tracking_fps,
                })
            );
            self.exit = true;
        }
        (&self.scene, &self.camera, &self.hud)
    }

    fn overlay(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        _format: wgpu::TextureFormat,
        size: (u32, u32),
    ) {
        let Some(compositor) = self.compositor.as_mut() else {
            return;
        };
        if let Some(video) = self
            .tracking
            .as_ref()
            .and_then(TrackingClient::latest_video)
        {
            compositor.update(gpu, video);
        }
        compositor.draw(gpu, encoder, view, self.scene.time, size);
    }

    fn wants_exit(&self) -> bool {
        self.exit
    }
}

fn one_percent_low_fps(intervals_ms: &[f64]) -> f64 {
    if intervals_ms.is_empty() {
        return 0.0;
    }
    let mut slowest = intervals_ms.to_vec();
    slowest.sort_by(|left, right| right.total_cmp(left));
    let count = ((slowest.len() as f64 * 0.01).ceil() as usize).max(1);
    let average_ms = slowest.iter().take(count).sum::<f64>() / count as f64;
    1000.0 / average_ms.max(1e-6)
}

#[cfg(test)]
mod tracking_policy_tests {
    use super::*;
    use pocket_live_core::TrackedPoint3;

    const LEFT_ARM: [BodyJoint; 3] = [
        BodyJoint::LeftShoulder,
        BodyJoint::LeftElbow,
        BodyJoint::LeftWrist,
    ];

    fn body_with(joints: &[BodyJoint]) -> BodyObservation {
        let mut body = BodyObservation::default();
        for (index, joint) in joints.iter().enumerate() {
            body.set(
                *joint,
                Some(TrackedPoint3 {
                    position: [index as f32 * 0.1, 0.5, 0.0],
                    confidence: 0.9,
                }),
            );
        }
        body
    }

    #[test]
    fn arm_needs_a_complete_chain_before_acquisition() {
        let partial = body_with(&LEFT_ARM[1..]);
        assert!(!arm_observed(&partial, LEFT_ARM, false));
        assert!(arm_observed(&body_with(&LEFT_ARM), LEFT_ARM, false));
    }

    #[test]
    fn acquired_arm_accepts_partial_updates_but_not_an_unrelated_shoulder() {
        assert!(arm_observed(
            &body_with(&[BodyJoint::LeftWrist]),
            LEFT_ARM,
            true
        ));
        assert!(arm_observed(
            &body_with(&[BodyJoint::LeftShoulder, BodyJoint::LeftElbow]),
            LEFT_ARM,
            true
        ));
        assert!(!arm_observed(
            &body_with(&[BodyJoint::LeftShoulder]),
            LEFT_ARM,
            true
        ));
    }

    #[test]
    fn camera_pitch_changes_sign_but_yaw_does_not_change_policy() {
        let input = [12f32.to_radians(), 20f32.to_radians(), 0.0];
        let mapped = camera_face_to_avatar_rotation(input);
        let (mapped_yaw, mapped_pitch, mapped_roll) = mapped.to_euler(EulerRot::YXZ);
        assert!((mapped_yaw + input[1]).abs() < 1e-5);
        assert!((mapped_pitch + input[0]).abs() < 1e-5);
        assert!(mapped_roll.abs() < 1e-5);
    }

    #[test]
    fn camera_hands_map_to_the_opposite_avatar_hands() {
        assert_eq!(mirrored_handedness(Handedness::Left), Handedness::Right);
        assert_eq!(mirrored_handedness(Handedness::Right), Handedness::Left);
    }

    #[test]
    fn swapped_camera_arm_chains_also_mirror_their_directions() {
        let mut body = BodyObservation::default();
        for (joint, position) in [
            (BodyJoint::LeftShoulder, [0.62, 0.7, 0.0]),
            (BodyJoint::LeftElbow, [0.73, 0.6, 0.0]),
            (BodyJoint::RightShoulder, [0.38, 0.7, 0.0]),
            (BodyJoint::RightElbow, [0.27, 0.6, 0.0]),
        ] {
            body.set(
                joint,
                Some(TrackedPoint3 {
                    position,
                    confidence: 0.9,
                }),
            );
        }

        let avatar_left = tracked_arm_direction(
            &body,
            BodyCoordinateSpace::ImageNormalized,
            BodyJoint::RightShoulder,
            BodyJoint::RightElbow,
            true,
        )
        .expect("right camera arm should drive avatar left arm");
        let avatar_right = tracked_arm_direction(
            &body,
            BodyCoordinateSpace::ImageNormalized,
            BodyJoint::LeftShoulder,
            BodyJoint::LeftElbow,
            true,
        )
        .expect("left camera arm should drive avatar right arm");

        assert!(
            avatar_left.x < 0.0,
            "avatar left arm must point screen-left"
        );
        assert!(
            avatar_right.x > 0.0,
            "avatar right arm must point screen-right"
        );
    }

    #[test]
    fn camera_relative_depth_toward_camera_maps_to_avatar_forward() {
        let mut body = BodyObservation::default();
        for (joint, position) in [
            (BodyJoint::LeftShoulder, [0.1, 0.5, -0.1]),
            (BodyJoint::LeftElbow, [0.2, 0.4, -0.3]),
        ] {
            body.set(
                joint,
                Some(TrackedPoint3 {
                    position,
                    confidence: 0.9,
                }),
            );
        }

        let direction = tracked_arm_direction(
            &body,
            BodyCoordinateSpace::CameraRelativeMeters,
            BodyJoint::LeftShoulder,
            BodyJoint::LeftElbow,
            true,
        )
        .expect("3D arm direction should be available");

        assert!(direction.z < 0.0, "avatar forward is toward model -Z");
    }

    #[test]
    fn deeply_cross_body_upper_arms_are_kept_in_front_of_the_torso() {
        let left = constrain_upper_arm_target(Vec3::X, AvatarArmSide::Left);
        let right = constrain_upper_arm_target(Vec3::NEG_X, AvatarArmSide::Right);

        assert!(left.x <= 0.35 && left.z < -0.8);
        assert!(right.x >= -0.35 && right.z < -0.8);
    }

    #[test]
    fn outward_upper_arm_targets_are_not_changed() {
        let left = Vec3::new(-0.8, 0.5, -0.1).normalize();
        let right = Vec3::new(0.8, 0.5, -0.1).normalize();

        assert!(constrain_upper_arm_target(left, AvatarArmSide::Left).abs_diff_eq(left, 1e-6));
        assert!(constrain_upper_arm_target(right, AvatarArmSide::Right).abs_diff_eq(right, 1e-6));
    }

    #[test]
    fn cross_body_constraint_is_continuous_during_a_sweep() {
        let mut previous =
            constrain_upper_arm_target(Vec3::new(-1.0, 0.6, 0.0).normalize(), AvatarArmSide::Left);
        for step in 1..=100 {
            let x = -1.0 + step as f32 * 0.02;
            let current =
                constrain_upper_arm_target(Vec3::new(x, 0.6, 0.0).normalize(), AvatarArmSide::Left);
            assert!(
                previous.angle_between(current) < 0.08,
                "constraint jumped at x={x}: {previous:?} -> {current:?}"
            );
            previous = current;
        }
    }
}
