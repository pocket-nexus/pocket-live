use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::tracking::{BodyCoordinateSpace, BodyJoint, BodyObservation};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BodyCalibration {
    pub body_space: BodyCoordinateSpace,
    pub shoulder_width: f32,
    pub torso_length: f32,
    pub left_upper_arm_length: f32,
    pub left_forearm_length: f32,
    pub right_upper_arm_length: f32,
    pub right_forearm_length: f32,
    pub samples: u16,
}

#[derive(Clone, Copy, Debug)]
struct Sample {
    shoulder_width: f32,
    torso_length: f32,
    left_upper_arm_length: f32,
    left_forearm_length: f32,
    right_upper_arm_length: f32,
    right_forearm_length: f32,
}

/// Collects robust median body proportions from consecutive neutral frames.
/// Invalid observations are ignored rather than advancing calibration.
pub struct CalibrationAccumulator {
    required_samples: u16,
    body_space: Option<BodyCoordinateSpace>,
    samples: Vec<Sample>,
}

impl CalibrationAccumulator {
    pub fn new(required_samples: u16) -> Self {
        assert!(required_samples > 0);
        Self {
            required_samples,
            body_space: None,
            samples: Vec::with_capacity(required_samples as usize),
        }
    }

    pub fn reset(&mut self) {
        self.body_space = None;
        self.samples.clear();
    }

    pub fn progress(&self) -> f32 {
        (self.samples.len() as f32 / self.required_samples as f32).clamp(0.0, 1.0)
    }

    pub fn push(
        &mut self,
        body_space: BodyCoordinateSpace,
        body: &BodyObservation,
    ) -> Option<BodyCalibration> {
        if self.body_space.is_some_and(|space| space != body_space) {
            self.reset();
        }
        self.body_space = Some(body_space);
        let sample = sample(body)?;
        self.samples.push(sample);
        if self.samples.len() < self.required_samples as usize {
            return None;
        }

        let calibration = BodyCalibration {
            body_space,
            shoulder_width: median(self.samples.iter().map(|s| s.shoulder_width)),
            torso_length: median(self.samples.iter().map(|s| s.torso_length)),
            left_upper_arm_length: median(self.samples.iter().map(|s| s.left_upper_arm_length)),
            left_forearm_length: median(self.samples.iter().map(|s| s.left_forearm_length)),
            right_upper_arm_length: median(self.samples.iter().map(|s| s.right_upper_arm_length)),
            right_forearm_length: median(self.samples.iter().map(|s| s.right_forearm_length)),
            samples: self.required_samples,
        };
        Some(calibration)
    }
}

fn sample(body: &BodyObservation) -> Option<Sample> {
    let point = |joint| {
        let point = body.get(joint)?;
        (point.confidence >= 0.5).then_some(Vec3::from_array(point.position))
    };
    let root = point(BodyJoint::Root)?;
    let neck = point(BodyJoint::Neck)?;
    let left_shoulder = point(BodyJoint::LeftShoulder)?;
    let left_elbow = point(BodyJoint::LeftElbow)?;
    let left_wrist = point(BodyJoint::LeftWrist)?;
    let right_shoulder = point(BodyJoint::RightShoulder)?;
    let right_elbow = point(BodyJoint::RightElbow)?;
    let right_wrist = point(BodyJoint::RightWrist)?;
    let sample = Sample {
        shoulder_width: left_shoulder.distance(right_shoulder),
        torso_length: root.distance(neck),
        left_upper_arm_length: left_shoulder.distance(left_elbow),
        left_forearm_length: left_elbow.distance(left_wrist),
        right_upper_arm_length: right_shoulder.distance(right_elbow),
        right_forearm_length: right_elbow.distance(right_wrist),
    };
    let values = [
        sample.shoulder_width,
        sample.torso_length,
        sample.left_upper_arm_length,
        sample.left_forearm_length,
        sample.right_upper_arm_length,
        sample.right_forearm_length,
    ];
    values
        .iter()
        .all(|value| value.is_finite() && *value > 1e-4)
        .then_some(sample)
}

fn median(values: impl Iterator<Item = f32>) -> f32 {
    let mut values: Vec<f32> = values.collect();
    values.sort_by(f32::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) * 0.5
    } else {
        values[middle]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracking::TrackedPoint3;

    fn body(scale: f32) -> BodyObservation {
        let mut body = BodyObservation::default();
        let mut set = |joint, p: [f32; 3]| {
            body.set(joint, TrackedPoint3::new(p.map(|value| value * scale), 1.0))
        };
        set(BodyJoint::Root, [0.0, 0.0, 0.0]);
        set(BodyJoint::Neck, [0.0, 2.0, 0.0]);
        set(BodyJoint::LeftShoulder, [-1.0, 1.8, 0.0]);
        set(BodyJoint::LeftElbow, [-2.0, 1.8, 0.0]);
        set(BodyJoint::LeftWrist, [-3.0, 1.8, 0.0]);
        set(BodyJoint::RightShoulder, [1.0, 1.8, 0.0]);
        set(BodyJoint::RightElbow, [2.0, 1.8, 0.0]);
        set(BodyJoint::RightWrist, [3.0, 1.8, 0.0]);
        body
    }

    #[test]
    fn needs_the_requested_number_of_valid_samples() {
        let mut accumulator = CalibrationAccumulator::new(3);
        assert_eq!(
            accumulator.push(BodyCoordinateSpace::CameraRelativeMeters, &body(1.0)),
            None
        );
        assert_eq!(accumulator.progress(), 1.0 / 3.0);
        accumulator.push(BodyCoordinateSpace::CameraRelativeMeters, &body(1.0));
        let calibration = accumulator
            .push(BodyCoordinateSpace::CameraRelativeMeters, &body(1.0))
            .unwrap();
        assert_eq!(calibration.shoulder_width, 2.0);
        assert_eq!(calibration.left_forearm_length, 1.0);
        assert_eq!(calibration.samples, 3);
    }

    #[test]
    fn median_rejects_a_single_scale_outlier() {
        let mut accumulator = CalibrationAccumulator::new(3);
        accumulator.push(BodyCoordinateSpace::ImageNormalized, &body(1.0));
        accumulator.push(BodyCoordinateSpace::ImageNormalized, &body(100.0));
        let calibration = accumulator
            .push(BodyCoordinateSpace::ImageNormalized, &body(1.0))
            .unwrap();
        assert_eq!(calibration.shoulder_width, 2.0);
    }

    #[test]
    fn coordinate_space_change_restarts_collection() {
        let mut accumulator = CalibrationAccumulator::new(2);
        accumulator.push(BodyCoordinateSpace::ImageNormalized, &body(1.0));
        assert_eq!(accumulator.progress(), 0.5);
        accumulator.push(BodyCoordinateSpace::CameraRelativeMeters, &body(1.0));
        assert_eq!(accumulator.progress(), 0.5);
    }
}
