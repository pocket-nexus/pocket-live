use glam::Vec3;

use crate::tracking::{BodyJoint, BodyObservation, TrackedPoint3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OneEuroConfig {
    /// Smoothing at low speed, in Hz. Lower values remove more jitter.
    pub min_cutoff: f32,
    /// How much cutoff rises with velocity.
    pub beta: f32,
    /// Derivative low-pass cutoff, in Hz.
    pub derivative_cutoff: f32,
}

impl Default for OneEuroConfig {
    fn default() -> Self {
        Self {
            min_cutoff: 1.2,
            beta: 0.08,
            derivative_cutoff: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OneEuroScalar {
    cfg: OneEuroConfig,
    last_time: Option<f64>,
    value: f32,
    derivative: f32,
    initialized: bool,
}

impl OneEuroScalar {
    pub fn new(cfg: OneEuroConfig) -> Self {
        assert!(cfg.min_cutoff > 0.0);
        assert!(cfg.beta >= 0.0);
        assert!(cfg.derivative_cutoff > 0.0);
        Self {
            cfg,
            last_time: None,
            value: 0.0,
            derivative: 0.0,
            initialized: false,
        }
    }

    pub fn reset(&mut self) {
        self.last_time = None;
        self.value = 0.0;
        self.derivative = 0.0;
        self.initialized = false;
    }

    /// Filter a sample timestamped with monotonic seconds.
    ///
    /// Non-finite samples and non-monotonic timestamps are ignored so a bad
    /// platform observation cannot poison all future poses.
    pub fn filter(&mut self, time: f64, sample: f32) -> f32 {
        if !time.is_finite() || !sample.is_finite() {
            return self.value;
        }
        if !self.initialized {
            self.last_time = Some(time);
            self.value = sample;
            self.initialized = true;
            return sample;
        }
        let Some(last_time) = self.last_time else {
            return self.value;
        };
        let dt = (time - last_time) as f32;
        if !dt.is_finite() || dt <= 0.0 {
            return self.value;
        }

        let raw_derivative = (sample - self.value) / dt;
        let derivative_alpha = smoothing_alpha(dt, self.cfg.derivative_cutoff);
        self.derivative += derivative_alpha * (raw_derivative - self.derivative);

        let cutoff = self.cfg.min_cutoff + self.cfg.beta * self.derivative.abs();
        let value_alpha = smoothing_alpha(dt, cutoff);
        self.value += value_alpha * (sample - self.value);
        self.last_time = Some(time);
        self.value
    }
}

#[derive(Clone, Copy, Debug)]
pub struct OneEuroVec3 {
    axes: [OneEuroScalar; 3],
}

impl OneEuroVec3 {
    pub fn new(cfg: OneEuroConfig) -> Self {
        Self {
            axes: [OneEuroScalar::new(cfg); 3],
        }
    }

    pub fn reset(&mut self) {
        self.axes.iter_mut().for_each(OneEuroScalar::reset);
    }

    pub fn filter(&mut self, time: f64, sample: Vec3) -> Vec3 {
        Vec3::new(
            self.axes[0].filter(time, sample.x),
            self.axes[1].filter(time, sample.y),
            self.axes[2].filter(time, sample.z),
        )
    }
}

/// One filter per body joint. Missing joints remain missing; their filter
/// state is retained so a short occlusion does not restart from zero.
#[derive(Clone, Copy, Debug)]
pub struct BodyPoseFilter {
    joints: [OneEuroVec3; BodyJoint::COUNT],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyStabilizerConfig {
    /// Ignore points below this confidence before they reach pose solving.
    pub min_confidence: f32,
    /// Maximum normalized-image velocity accepted for a joint.
    pub max_speed_per_second: f32,
}

impl Default for BodyStabilizerConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.3,
            // A fast hand sweep is normally below 4 image widths/s. The
            // observed 12 widths/s spikes are detector identity jumps.
            max_speed_per_second: 4.0,
        }
    }
}

/// One-Euro filtering plus a hard velocity gate. Rejected observations keep
/// the last filtered point, allowing the host's per-chain lifecycle to hold
/// and fade the pose instead of snapping back to the authored idle clip.
#[derive(Clone, Copy, Debug)]
pub struct BodyPoseStabilizer {
    filters: [OneEuroVec3; BodyJoint::COUNT],
    last_raw: [Option<Vec3>; BodyJoint::COUNT],
    last_output: [Option<TrackedPoint3>; BodyJoint::COUNT],
    last_accepted_time: [Option<f64>; BodyJoint::COUNT],
    cfg: BodyStabilizerConfig,
}

impl BodyPoseStabilizer {
    pub fn new(filter: OneEuroConfig, cfg: BodyStabilizerConfig) -> Self {
        assert!((0.0..=1.0).contains(&cfg.min_confidence));
        assert!(cfg.max_speed_per_second > 0.0);
        Self {
            filters: [OneEuroVec3::new(filter); BodyJoint::COUNT],
            last_raw: [None; BodyJoint::COUNT],
            last_output: [None; BodyJoint::COUNT],
            last_accepted_time: [None; BodyJoint::COUNT],
            cfg,
        }
    }

    pub fn reset(&mut self) {
        self.filters.iter_mut().for_each(OneEuroVec3::reset);
        self.last_raw = [None; BodyJoint::COUNT];
        self.last_output = [None; BodyJoint::COUNT];
        self.last_accepted_time = [None; BodyJoint::COUNT];
    }

    pub fn filter(&mut self, time: f64, input: &BodyObservation) -> BodyObservation {
        let mut output = BodyObservation::default();
        for joint in BodyJoint::ALL {
            let index = joint.index();
            let accepted = input.get(joint).filter(|point| {
                if point.confidence < self.cfg.min_confidence {
                    return false;
                }
                let raw = Vec3::from_array(point.position);
                match (self.last_raw[index], self.last_accepted_time[index]) {
                    (Some(previous), Some(previous_time)) if time > previous_time => {
                        raw.distance(previous) / (time - previous_time) as f32
                            <= self.cfg.max_speed_per_second
                    }
                    _ => true,
                }
            });

            if let Some(point) = accepted {
                let raw = Vec3::from_array(point.position);
                let filtered = self.filters[index].filter(time, raw);
                let point = TrackedPoint3 {
                    position: filtered.to_array(),
                    confidence: point.confidence,
                };
                self.last_raw[index] = Some(raw);
                self.last_output[index] = Some(point);
                self.last_accepted_time[index] = Some(time);
            }
            output.set(joint, self.last_output[index]);
        }
        output
    }
}

impl BodyPoseFilter {
    pub fn new(cfg: OneEuroConfig) -> Self {
        Self {
            joints: [OneEuroVec3::new(cfg); BodyJoint::COUNT],
        }
    }

    pub fn reset(&mut self) {
        self.joints.iter_mut().for_each(OneEuroVec3::reset);
    }

    pub fn filter(&mut self, time: f64, input: &BodyObservation) -> BodyObservation {
        let mut output = BodyObservation::default();
        for joint in BodyJoint::ALL {
            let Some(point) = input.get(joint) else {
                continue;
            };
            let value = Vec3::from_array(point.position);
            let filtered = self.joints[joint.index()].filter(time, value);
            output.set(
                joint,
                Some(TrackedPoint3 {
                    position: filtered.to_array(),
                    confidence: point.confidence,
                }),
            );
        }
        output
    }
}

fn smoothing_alpha(dt: f32, cutoff: f32) -> f32 {
    let tau = 1.0 / (2.0 * core::f32::consts::PI * cutoff.max(f32::EPSILON));
    1.0 / (1.0 + tau / dt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sample_passes_through() {
        let mut f = OneEuroScalar::new(OneEuroConfig::default());
        assert_eq!(f.filter(1.0, 42.0), 42.0);
    }

    #[test]
    fn constant_input_converges() {
        let mut f = OneEuroScalar::new(OneEuroConfig::default());
        f.filter(0.0, 0.0);
        let mut out = 0.0;
        for i in 1..=300 {
            out = f.filter(i as f64 / 60.0, 1.0);
        }
        assert!((out - 1.0).abs() < 1e-4, "out={out}");
    }

    #[test]
    fn invalid_sample_does_not_poison_filter() {
        let mut f = OneEuroScalar::new(OneEuroConfig::default());
        assert_eq!(f.filter(0.0, 2.0), 2.0);
        assert_eq!(f.filter(0.1, f32::NAN), 2.0);
        assert!(f.filter(0.2, 3.0).is_finite());
    }

    #[test]
    fn rejects_time_travel() {
        let mut f = OneEuroScalar::new(OneEuroConfig::default());
        assert_eq!(f.filter(1.0, 2.0), 2.0);
        assert_eq!(f.filter(0.5, 9.0), 2.0);
    }

    #[test]
    fn body_filter_preserves_missing_joints_and_confidence() {
        let mut filter = BodyPoseFilter::new(OneEuroConfig::default());
        let mut input = BodyObservation::default();
        input.set(BodyJoint::Head, TrackedPoint3::new([0.5, 0.8, 0.0], 0.9));
        let output = filter.filter(1.0, &input);
        assert_eq!(output.get(BodyJoint::Head).unwrap().confidence, 0.9);
        assert_eq!(output.get(BodyJoint::LeftWrist), None);
    }

    #[test]
    fn stabilizer_holds_missing_and_rejects_impossible_velocity() {
        let mut filter = BodyPoseStabilizer::new(
            OneEuroConfig::default(),
            BodyStabilizerConfig {
                min_confidence: 0.3,
                max_speed_per_second: 2.0,
            },
        );
        let mut input = BodyObservation::default();
        input.set(
            BodyJoint::LeftWrist,
            TrackedPoint3::new([0.2, 0.5, 0.0], 0.9),
        );
        let first = filter
            .filter(1.0, &input)
            .get(BodyJoint::LeftWrist)
            .unwrap();

        let missing = filter.filter(1.05, &BodyObservation::default());
        assert_eq!(missing.get(BodyJoint::LeftWrist), Some(first));

        input.set(
            BodyJoint::LeftWrist,
            TrackedPoint3::new([0.9, 0.5, 0.0], 0.9),
        );
        let rejected = filter.filter(1.1, &input);
        assert_eq!(rejected.get(BodyJoint::LeftWrist), Some(first));
    }
}
