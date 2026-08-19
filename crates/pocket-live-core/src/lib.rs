//! Deterministic real-time tracking primitives for Pocket Live.
//!
//! This crate deliberately has no camera, Apple-framework, GPU, filesystem,
//! network, or threading dependencies. Platform adapters produce
//! [`TrackingFrame`] values; the Pocket host consumes the validated and
//! filtered outputs.

pub mod calibration;
pub mod face;
pub mod filter;
pub mod gesture;
pub mod lifecycle;
pub mod pose;
pub mod tracking;

pub use calibration::{BodyCalibration, CalibrationAccumulator};
pub use face::{FaceCalibrationAccumulator, FaceControlFilter, FaceNeutralCalibration};
pub use filter::{
    BodyPoseFilter, BodyPoseStabilizer, BodyStabilizerConfig, OneEuroConfig, OneEuroScalar,
    OneEuroVec3,
};
pub use gesture::{GestureEvent, GestureGate, GestureGateConfig, web_shoot_score};
pub use lifecycle::{TrackingLifecycle, TrackingLifecycleConfig, TrackingState};
pub use pose::{BoneRotation, HumanoidBone, rotation_between};
pub use tracking::{
    BodyCoordinateSpace, BodyJoint, BodyObservation, FaceObservation, HandObservation, Handedness,
    TrackedPoint3, TrackingFrame, TrackingFrameError,
};
