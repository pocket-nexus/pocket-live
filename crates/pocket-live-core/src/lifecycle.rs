#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackingState {
    Idle,
    Acquiring,
    Tracking,
    Holding,
    Recovering,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackingLifecycleConfig {
    pub acquire_frames: u16,
    pub hold_ns: u64,
    pub recover_ns: u64,
}

impl Default for TrackingLifecycleConfig {
    fn default() -> Self {
        Self {
            acquire_frames: 5,
            hold_ns: 250_000_000,
            recover_ns: 500_000_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackingLifecycle {
    cfg: TrackingLifecycleConfig,
    state: TrackingState,
    valid_streak: u16,
    state_started_ns: u64,
    weight: f32,
}

impl TrackingLifecycle {
    pub fn new(cfg: TrackingLifecycleConfig) -> Self {
        assert!(cfg.acquire_frames > 0);
        assert!(cfg.recover_ns > 0);
        Self {
            cfg,
            state: TrackingState::Idle,
            valid_streak: 0,
            state_started_ns: 0,
            weight: 0.0,
        }
    }

    pub fn state(&self) -> TrackingState {
        self.state
    }

    pub fn weight(&self) -> f32 {
        self.weight
    }

    /// Advance from a monotonically increasing capture timestamp.
    pub fn update(&mut self, now_ns: u64, valid_pose: bool) -> f32 {
        match self.state {
            TrackingState::Idle => {
                self.weight = 0.0;
                if valid_pose {
                    self.enter(TrackingState::Acquiring, now_ns);
                    self.valid_streak = 1;
                    self.weight = self.acquisition_weight();
                }
            }
            TrackingState::Acquiring => {
                if valid_pose {
                    self.valid_streak = self.valid_streak.saturating_add(1);
                    self.weight = self.acquisition_weight();
                    if self.valid_streak >= self.cfg.acquire_frames {
                        self.enter(TrackingState::Tracking, now_ns);
                        self.weight = 1.0;
                    }
                } else {
                    self.enter(TrackingState::Idle, now_ns);
                    self.weight = 0.0;
                }
            }
            TrackingState::Tracking => {
                self.weight = 1.0;
                if !valid_pose {
                    self.enter(TrackingState::Holding, now_ns);
                }
            }
            TrackingState::Holding => {
                if valid_pose {
                    self.enter(TrackingState::Tracking, now_ns);
                    self.weight = 1.0;
                } else if now_ns.saturating_sub(self.state_started_ns) >= self.cfg.hold_ns {
                    self.enter(TrackingState::Recovering, now_ns);
                }
            }
            TrackingState::Recovering => {
                if valid_pose {
                    self.enter(TrackingState::Acquiring, now_ns);
                    self.valid_streak = 1;
                    self.weight = self.acquisition_weight();
                } else {
                    let elapsed = now_ns.saturating_sub(self.state_started_ns);
                    self.weight = 1.0 - elapsed as f32 / self.cfg.recover_ns as f32;
                    self.weight = self.weight.clamp(0.0, 1.0);
                    if elapsed >= self.cfg.recover_ns {
                        self.enter(TrackingState::Idle, now_ns);
                        self.weight = 0.0;
                    }
                }
            }
        }
        self.weight
    }

    fn enter(&mut self, state: TrackingState, now_ns: u64) {
        self.state = state;
        self.state_started_ns = now_ns;
        if state != TrackingState::Acquiring {
            self.valid_streak = 0;
        }
    }

    fn acquisition_weight(&self) -> f32 {
        (self.valid_streak as f32 / self.cfg.acquire_frames as f32).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: u64 = 1_000_000;

    fn tracker() -> TrackingLifecycle {
        TrackingLifecycle::new(TrackingLifecycleConfig {
            acquire_frames: 3,
            hold_ns: 250 * MS,
            recover_ns: 500 * MS,
        })
    }

    #[test]
    fn requires_consecutive_valid_frames() {
        let mut t = tracker();
        assert_eq!(t.update(0, true), 1.0 / 3.0);
        assert_eq!(t.state(), TrackingState::Acquiring);
        t.update(16 * MS, true);
        assert_eq!(t.state(), TrackingState::Acquiring);
        assert_eq!(t.update(32 * MS, true), 1.0);
        assert_eq!(t.state(), TrackingState::Tracking);
    }

    #[test]
    fn holds_then_blends_to_idle() {
        let mut t = tracker();
        t.update(0, true);
        t.update(16 * MS, true);
        t.update(32 * MS, true);
        t.update(100 * MS, false);
        assert_eq!(t.state(), TrackingState::Holding);
        assert_eq!(t.update(349 * MS, false), 1.0);
        t.update(350 * MS, false);
        assert_eq!(t.state(), TrackingState::Recovering);
        let mid = t.update(600 * MS, false);
        assert!((mid - 0.5).abs() < 1e-6, "mid={mid}");
        assert_eq!(t.update(850 * MS, false), 0.0);
        assert_eq!(t.state(), TrackingState::Idle);
    }

    #[test]
    fn brief_loss_returns_directly_to_tracking() {
        let mut t = tracker();
        t.update(0, true);
        t.update(16 * MS, true);
        t.update(32 * MS, true);
        t.update(100 * MS, false);
        assert_eq!(t.update(200 * MS, true), 1.0);
        assert_eq!(t.state(), TrackingState::Tracking);
    }
}
