use core::array;

use serde::{Deserialize, Serialize};

/// Version of the bridge-to-host data contract.
pub const TRACKING_SCHEMA_VERSION: u16 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BodyCoordinateSpace {
    /// X/Y are normalized image coordinates with lower-left origin; Z is 0.
    ImageNormalized,
    /// XYZ are camera-relative meters supplied by Vision 3D body pose.
    CameraRelativeMeters,
}

/// Body joints needed by the half-body MVP.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum BodyJoint {
    Root = 0,
    Spine,
    Neck,
    Head,
    LeftShoulder,
    LeftElbow,
    LeftWrist,
    RightShoulder,
    RightElbow,
    RightWrist,
    LeftHip,
    LeftKnee,
    LeftAnkle,
    RightHip,
    RightKnee,
    RightAnkle,
}

impl BodyJoint {
    pub const COUNT: usize = 16;
    pub const ALL: [Self; Self::COUNT] = [
        Self::Root,
        Self::Spine,
        Self::Neck,
        Self::Head,
        Self::LeftShoulder,
        Self::LeftElbow,
        Self::LeftWrist,
        Self::RightShoulder,
        Self::RightElbow,
        Self::RightWrist,
        Self::LeftHip,
        Self::LeftKnee,
        Self::LeftAnkle,
        Self::RightHip,
        Self::RightKnee,
        Self::RightAnkle,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }
}

/// A camera-normalized observation: +X right, +Y up, +Z away from camera.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackedPoint3 {
    pub position: [f32; 3],
    pub confidence: f32,
}

impl TrackedPoint3 {
    pub fn new(position: [f32; 3], confidence: f32) -> Option<Self> {
        let point = Self {
            position,
            confidence,
        };
        point.is_valid().then_some(point)
    }

    pub fn is_valid(self) -> bool {
        self.position.iter().all(|v| v.is_finite())
            && self.confidence.is_finite()
            && (0.0..=1.0).contains(&self.confidence)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BodyObservation {
    pub joints: [Option<TrackedPoint3>; BodyJoint::COUNT],
}

impl Default for BodyObservation {
    fn default() -> Self {
        Self {
            joints: array::from_fn(|_| None),
        }
    }
}

impl BodyObservation {
    pub fn get(&self, joint: BodyJoint) -> Option<TrackedPoint3> {
        self.joints[joint.index()]
    }

    pub fn set(&mut self, joint: BodyJoint, point: Option<TrackedPoint3>) {
        self.joints[joint.index()] = point;
    }

    pub fn confident_joint_count(&self, threshold: f32) -> usize {
        self.joints
            .iter()
            .flatten()
            .filter(|point| point.confidence >= threshold)
            .count()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Handedness {
    Left,
    Right,
}

/// Vision hand-landmark order is normalized by the platform bridge into this
/// fixed order: wrist, then four joints for thumb/index/middle/ring/little.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HandObservation {
    pub handedness: Handedness,
    pub points: [Option<TrackedPoint3>; 21],
    pub confidence: f32,
}

/// Backend-neutral facial controls derived from local face landmarks.
/// Rotations are pitch/yaw/roll radians. Other values are normalized so the
/// Pocket host does not depend on Apple- or MediaPipe-specific point sets.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FaceObservation {
    pub head_rotation_radians: [f32; 3],
    pub eye_blink: [f32; 2],
    pub eye_look: [f32; 2],
    pub mouth_open: f32,
    pub smile: f32,
    pub brow_raise: f32,
    pub confidence: f32,
}

impl FaceObservation {
    pub fn is_valid(self) -> bool {
        self.head_rotation_radians
            .iter()
            .all(|value| value.is_finite())
            && self
                .eye_blink
                .iter()
                .chain(
                    [
                        self.mouth_open,
                        self.smile,
                        self.brow_raise,
                        self.confidence,
                    ]
                    .iter(),
                )
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
            && self
                .eye_look
                .iter()
                .all(|value| value.is_finite() && (-1.0..=1.0).contains(value))
    }
}

impl HandObservation {
    pub fn empty(handedness: Handedness) -> Self {
        Self {
            handedness,
            points: array::from_fn(|_| None),
            confidence: 0.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackingFrame {
    pub schema_version: u16,
    pub sequence: u64,
    /// Monotonic timestamp captured with the camera frame.
    pub captured_at_ns: u64,
    pub image_size: [u32; 2],
    pub body_space: BodyCoordinateSpace,
    pub body: BodyObservation,
    pub hands: [HandObservation; 2],
    #[serde(default)]
    pub face: Option<FaceObservation>,
}

impl TrackingFrame {
    pub fn validate(&self) -> Result<(), TrackingFrameError> {
        if self.schema_version != TRACKING_SCHEMA_VERSION {
            return Err(TrackingFrameError::UnsupportedSchema {
                got: self.schema_version,
            });
        }
        if self.image_size[0] == 0 || self.image_size[1] == 0 {
            return Err(TrackingFrameError::InvalidImageSize);
        }
        for (index, point) in self.body.joints.iter().enumerate() {
            if point.is_some_and(|point| !point.is_valid()) {
                return Err(TrackingFrameError::InvalidBodyPoint { index });
            }
        }
        for (hand, observation) in self.hands.iter().enumerate() {
            if !observation.confidence.is_finite() || !(0.0..=1.0).contains(&observation.confidence)
            {
                return Err(TrackingFrameError::InvalidHandConfidence { hand });
            }
            for (index, point) in observation.points.iter().enumerate() {
                if point.is_some_and(|point| !point.is_valid()) {
                    return Err(TrackingFrameError::InvalidHandPoint { hand, index });
                }
            }
        }
        if self.face.is_some_and(|face| !face.is_valid()) {
            return Err(TrackingFrameError::InvalidFace);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackingFrameError {
    UnsupportedSchema { got: u16 },
    InvalidImageSize,
    InvalidBodyPoint { index: usize },
    InvalidHandConfidence { hand: usize },
    InvalidHandPoint { hand: usize, index: usize },
    InvalidFace,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> TrackingFrame {
        TrackingFrame {
            schema_version: TRACKING_SCHEMA_VERSION,
            sequence: 7,
            captured_at_ns: 123,
            image_size: [1920, 1080],
            body_space: BodyCoordinateSpace::ImageNormalized,
            body: BodyObservation::default(),
            hands: [
                HandObservation::empty(Handedness::Left),
                HandObservation::empty(Handedness::Right),
            ],
            face: None,
        }
    }

    #[test]
    fn accepts_a_well_formed_frame() {
        assert_eq!(frame().validate(), Ok(()));
    }

    #[test]
    fn rejects_non_finite_points() {
        let mut frame = frame();
        frame.body.joints[BodyJoint::Head.index()] = Some(TrackedPoint3 {
            position: [0.0, f32::NAN, 0.0],
            confidence: 1.0,
        });
        assert_eq!(
            frame.validate(),
            Err(TrackingFrameError::InvalidBodyPoint {
                index: BodyJoint::Head.index()
            })
        );
    }

    #[test]
    fn validates_face_controls() {
        let mut frame = frame();
        frame.face = Some(FaceObservation {
            head_rotation_radians: [0.1, -0.2, 0.0],
            eye_blink: [0.2, 0.3],
            eye_look: [0.5, -0.5],
            mouth_open: 0.4,
            smile: 0.1,
            brow_raise: 0.2,
            confidence: 0.9,
        });
        assert_eq!(frame.validate(), Ok(()));
        frame.face.as_mut().unwrap().mouth_open = 1.1;
        assert_eq!(frame.validate(), Err(TrackingFrameError::InvalidFace));
    }

    #[test]
    fn serde_round_trip_preserves_contract() {
        // Compile-time coverage of all Serialize/Deserialize derives without
        // making serde_json a runtime dependency of this core crate.
        fn assert_serde<T: Serialize + for<'de> Deserialize<'de>>() {}
        assert_serde::<TrackingFrame>();
    }
}
