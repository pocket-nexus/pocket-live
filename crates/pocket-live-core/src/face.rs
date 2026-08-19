use crate::{FaceObservation, OneEuroConfig, OneEuroScalar};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceNeutralCalibration {
    pub head_rotation_radians: [f32; 3],
    pub eye_blink: [f32; 2],
    pub mouth_open: f32,
    pub smile: f32,
    pub brow_raise: f32,
    pub samples: usize,
}

#[derive(Clone, Debug)]
pub struct FaceCalibrationAccumulator {
    target_samples: usize,
    samples: Vec<FaceObservation>,
}

impl FaceCalibrationAccumulator {
    pub fn new(target_samples: usize) -> Self {
        assert!(target_samples > 0);
        Self {
            target_samples,
            samples: Vec::with_capacity(target_samples),
        }
    }

    pub fn push(&mut self, face: FaceObservation) -> Option<FaceNeutralCalibration> {
        // Only learn a neutral baseline from open eyes, closed jaw, relaxed
        // mouth, and a near-frontal head. This prevents a startup gesture from
        // becoming the permanent neutral pose.
        if face.confidence < 0.5
            || face.eye_blink.iter().any(|value| *value > 0.35)
            || face.mouth_open > 0.2
            || face.smile > 0.35
            || face.head_rotation_radians[1].abs() > 25f32.to_radians()
            || face.head_rotation_radians[2].abs() > 25f32.to_radians()
        {
            return None;
        }
        self.samples.push(face);
        if self.samples.len() < self.target_samples {
            return None;
        }
        let median = |values: Vec<f32>| -> f32 {
            let mut values = values;
            values.sort_by(f32::total_cmp);
            values[values.len() / 2]
        };
        Some(FaceNeutralCalibration {
            head_rotation_radians: core::array::from_fn(|axis| {
                median(
                    self.samples
                        .iter()
                        .map(|face| face.head_rotation_radians[axis])
                        .collect(),
                )
            }),
            eye_blink: core::array::from_fn(|eye| {
                median(
                    self.samples
                        .iter()
                        .map(|face| face.eye_blink[eye])
                        .collect(),
                )
            }),
            mouth_open: median(self.samples.iter().map(|face| face.mouth_open).collect()),
            smile: median(self.samples.iter().map(|face| face.smile).collect()),
            brow_raise: median(self.samples.iter().map(|face| face.brow_raise).collect()),
            samples: self.samples.len(),
        })
    }

    pub fn progress(&self) -> f32 {
        (self.samples.len() as f32 / self.target_samples as f32).clamp(0.0, 1.0)
    }
}

impl FaceNeutralCalibration {
    pub fn normalize(self, face: FaceObservation) -> FaceObservation {
        fn gain(value: f32, neutral: f32, full_scale: f32) -> f32 {
            ((value - neutral) / (full_scale - neutral).max(0.1)).clamp(0.0, 1.0)
        }
        FaceObservation {
            head_rotation_radians: core::array::from_fn(|axis| {
                face.head_rotation_radians[axis] - self.head_rotation_radians[axis]
            }),
            eye_blink: core::array::from_fn(|eye| {
                gain(face.eye_blink[eye], self.eye_blink[eye] + 0.03, 0.55)
            }),
            eye_look: face.eye_look,
            mouth_open: gain(face.mouth_open, self.mouth_open + 0.02, 0.7),
            smile: gain(face.smile, self.smile + 0.04, 0.8),
            brow_raise: gain(face.brow_raise, self.brow_raise + 0.03, 0.7),
            confidence: face.confidence,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FaceControlFilter {
    values: [OneEuroScalar; 10],
}

impl FaceControlFilter {
    pub fn new(config: OneEuroConfig) -> Self {
        Self {
            values: [OneEuroScalar::new(config); 10],
        }
    }

    pub fn reset(&mut self) {
        self.values.iter_mut().for_each(OneEuroScalar::reset);
    }

    pub fn filter(&mut self, time: f64, face: FaceObservation) -> FaceObservation {
        let mut index = 0;
        let mut next = || {
            let current = index;
            index += 1;
            current
        };
        FaceObservation {
            head_rotation_radians: core::array::from_fn(|axis| {
                let i = next();
                self.values[i].filter(time, face.head_rotation_radians[axis])
            }),
            eye_blink: core::array::from_fn(|eye| {
                let i = next();
                self.values[i].filter(time, face.eye_blink[eye])
            }),
            eye_look: core::array::from_fn(|axis| {
                let i = next();
                self.values[i].filter(time, face.eye_look[axis])
            }),
            mouth_open: {
                let i = next();
                self.values[i].filter(time, face.mouth_open)
            },
            smile: {
                let i = next();
                self.values[i].filter(time, face.smile)
            },
            brow_raise: {
                let i = next();
                self.values[i].filter(time, face.brow_raise)
            },
            confidence: face.confidence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neutral() -> FaceObservation {
        FaceObservation {
            head_rotation_radians: [-0.35, -0.07, 0.02],
            eye_blink: [0.08, 0.09],
            eye_look: [0.0, 0.0],
            mouth_open: 0.01,
            smile: 0.03,
            brow_raise: 0.02,
            confidence: 1.0,
        }
    }

    #[test]
    fn neutral_calibration_removes_camera_bias_and_expands_shapes() {
        let mut accumulator = FaceCalibrationAccumulator::new(3);
        assert!(accumulator.push(neutral()).is_none());
        assert!(accumulator.push(neutral()).is_none());
        let calibration = accumulator.push(neutral()).unwrap();
        let mut expressive = neutral();
        expressive.head_rotation_radians[1] += 0.4;
        expressive.eye_blink = [0.65, 0.65];
        expressive.mouth_open = 0.7;
        expressive.smile = 0.8;
        let normalized = calibration.normalize(expressive);
        assert!((normalized.head_rotation_radians[1] - 0.4).abs() < 1e-5);
        assert_eq!(normalized.eye_blink, [1.0, 1.0]);
        assert_eq!(normalized.mouth_open, 1.0);
        assert_eq!(normalized.smile, 1.0);
    }

    #[test]
    fn gestures_are_not_learned_as_neutral() {
        let mut accumulator = FaceCalibrationAccumulator::new(1);
        let mut smiling = neutral();
        smiling.smile = 0.9;
        assert!(accumulator.push(smiling).is_none());
        assert_eq!(accumulator.progress(), 0.0);
    }
}
