use glam::Vec3;

use crate::tracking::HandObservation;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GestureGateConfig {
    pub start_threshold: f32,
    pub release_threshold: f32,
    pub min_on_ns: u64,
    pub cooldown_ns: u64,
}

impl Default for GestureGateConfig {
    fn default() -> Self {
        Self {
            start_threshold: 0.82,
            release_threshold: 0.55,
            min_on_ns: 120_000_000,
            cooldown_ns: 250_000_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GestureEvent {
    Started,
    Ended,
}

/// Geometric evidence for the classic web-shoot pose: index and little
/// fingers extended while middle and ring fingers are curled. This score is
/// deterministic and scale-independent; the temporal [`GestureGate`] is
/// responsible for debouncing it.
pub fn web_shoot_score(hand: &HandObservation) -> f32 {
    // wrist is slot 0; each finger then occupies four consecutive points.
    let index_extended = finger_extension(hand, 5);
    let middle_curled = 1.0 - finger_extension(hand, 9);
    let ring_curled = 1.0 - finger_extension(hand, 13);
    let little_extended = finger_extension(hand, 17);
    index_extended
        .min(middle_curled)
        .min(ring_curled)
        .min(little_extended)
        * hand.confidence.clamp(0.0, 1.0)
}

fn finger_extension(hand: &HandObservation, first: usize) -> f32 {
    let points = [first, first + 1, first + 2, first + 3]
        .map(|index| hand.points[index].map(|point| Vec3::from_array(point.position)));
    let [Some(a), Some(b), Some(c), Some(d)] = points else {
        return 0.0;
    };
    let path = a.distance(b) + b.distance(c) + c.distance(d);
    if !path.is_finite() || path <= 1e-6 {
        return 0.0;
    }
    let straightness = a.distance(d) / path;
    ((straightness - 0.55) / 0.4).clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GateState {
    Idle,
    Candidate,
    Active,
    Cooldown,
}

/// Turns noisy per-frame gesture evidence into debounced start/end events.
#[derive(Clone, Copy, Debug)]
pub struct GestureGate {
    cfg: GestureGateConfig,
    state: GateState,
    state_started_ns: u64,
}

impl GestureGate {
    pub fn new(cfg: GestureGateConfig) -> Self {
        assert!(cfg.release_threshold < cfg.start_threshold);
        assert!((0.0..=1.0).contains(&cfg.release_threshold));
        assert!((0.0..=1.0).contains(&cfg.start_threshold));
        Self {
            cfg,
            state: GateState::Idle,
            state_started_ns: 0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.state == GateState::Active
    }

    /// `score` must already combine landmark geometry and hand confidence.
    pub fn update(&mut self, now_ns: u64, score: f32) -> Option<GestureEvent> {
        let score = if score.is_finite() {
            score.clamp(0.0, 1.0)
        } else {
            0.0
        };
        match self.state {
            GateState::Idle => {
                if score >= self.cfg.start_threshold {
                    self.enter(GateState::Candidate, now_ns);
                }
            }
            GateState::Candidate => {
                if score < self.cfg.start_threshold {
                    self.enter(GateState::Idle, now_ns);
                } else if now_ns.saturating_sub(self.state_started_ns) >= self.cfg.min_on_ns {
                    self.enter(GateState::Active, now_ns);
                    return Some(GestureEvent::Started);
                }
            }
            GateState::Active => {
                if score <= self.cfg.release_threshold {
                    self.enter(GateState::Cooldown, now_ns);
                    return Some(GestureEvent::Ended);
                }
            }
            GateState::Cooldown => {
                if now_ns.saturating_sub(self.state_started_ns) >= self.cfg.cooldown_ns
                    && score < self.cfg.start_threshold
                {
                    self.enter(GateState::Idle, now_ns);
                }
            }
        }
        None
    }

    fn enter(&mut self, state: GateState, now_ns: u64) {
        self.state = state;
        self.state_started_ns = now_ns;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracking::{Handedness, TrackedPoint3};

    const MS: u64 = 1_000_000;

    #[test]
    fn rejects_a_single_noisy_peak() {
        let mut gate = GestureGate::new(GestureGateConfig::default());
        assert_eq!(gate.update(0, 0.9), None);
        assert_eq!(gate.update(16 * MS, 0.1), None);
        assert!(!gate.is_active());
    }

    #[test]
    fn debounces_and_applies_release_hysteresis() {
        let mut gate = GestureGate::new(GestureGateConfig::default());
        gate.update(0, 0.9);
        assert_eq!(gate.update(119 * MS, 0.9), None);
        assert_eq!(gate.update(120 * MS, 0.9), Some(GestureEvent::Started));
        assert!(gate.is_active());
        assert_eq!(gate.update(140 * MS, 0.7), None);
        assert_eq!(gate.update(160 * MS, 0.5), Some(GestureEvent::Ended));
        assert!(!gate.is_active());
    }

    #[test]
    fn cooldown_requires_release_before_rearming() {
        let mut gate = GestureGate::new(GestureGateConfig::default());
        gate.update(0, 1.0);
        gate.update(120 * MS, 1.0);
        gate.update(130 * MS, 0.0);
        assert_eq!(gate.update(500 * MS, 1.0), None);
        assert_eq!(gate.update(510 * MS, 0.0), None);
        assert_eq!(gate.update(520 * MS, 1.0), None);
        assert_eq!(gate.update(640 * MS, 1.0), Some(GestureEvent::Started));
    }

    fn set_finger(hand: &mut HandObservation, first: usize, extended: bool) {
        let points = if extended {
            [
                [0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 2.0, 0.0],
                [0.0, 3.0, 0.0],
            ]
        } else {
            [
                [0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.8, 1.0, 0.0],
                [0.1, 0.1, 0.0],
            ]
        };
        for (offset, position) in points.into_iter().enumerate() {
            hand.points[first + offset] = TrackedPoint3::new(position, 1.0);
        }
    }

    #[test]
    fn web_score_requires_the_full_hand_shape() {
        let mut hand = HandObservation::empty(Handedness::Left);
        hand.confidence = 1.0;
        set_finger(&mut hand, 5, true);
        set_finger(&mut hand, 9, false);
        set_finger(&mut hand, 13, false);
        set_finger(&mut hand, 17, true);
        assert!(web_shoot_score(&hand) > 0.8);

        set_finger(&mut hand, 9, true);
        assert_eq!(web_shoot_score(&hand), 0.0);
    }
}
