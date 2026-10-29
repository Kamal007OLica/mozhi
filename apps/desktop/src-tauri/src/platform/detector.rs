//! Double-tap detection, shared by the macOS and Windows key hooks.
//!
//! A *tap* is the trigger key pressed and released on its own, within `max_tap_ms`,
//! with no other key or modifier involved. A *double tap* is a second tap whose press
//! starts within `double_tap_ms` of the first tap's release. Anything else in between
//! (Ctrl+C, Ctrl held for a shortcut, a stray letter) resets the detector.

use std::time::{Duration, Instant};

use crate::types::HotkeyConfig;

#[derive(Debug, Default)]
pub(super) struct DoubleTap {
    /// Trigger is down and nothing else has happened since it went down.
    down_at: Option<Instant>,
    /// Release time of the last clean tap.
    last_tap: Option<Instant>,
    /// The current press began inside the double-tap window.
    armed: bool,
}

impl DoubleTap {
    /// The trigger key went down. `others_held`: another modifier is already held.
    pub fn trigger_down(&mut self, now: Instant, others_held: bool, cfg: &HotkeyConfig) {
        if self.down_at.is_some() {
            return; // key repeat (Windows sends repeated key-downs for held modifiers)
        }
        if others_held {
            self.reset();
            return;
        }
        let gap = Duration::from_millis(cfg.double_tap_ms as u64);
        self.armed = self.last_tap.map(|t| now.saturating_duration_since(t) <= gap).unwrap_or(false);
        if !self.armed {
            self.last_tap = None;
        }
        self.down_at = Some(now);
    }

    /// The trigger key went up. Returns true when this completes a double tap.
    pub fn trigger_up(&mut self, now: Instant, cfg: &HotkeyConfig) -> bool {
        let Some(down) = self.down_at.take() else {
            self.reset();
            return false;
        };
        if now.saturating_duration_since(down) > Duration::from_millis(cfg.max_tap_ms as u64) {
            self.reset(); // a hold, not a tap
            return false;
        }
        if self.armed {
            self.reset();
            return true;
        }
        self.last_tap = Some(now);
        false
    }

    /// Any other key or modifier was pressed: whatever was in progress is not a double tap.
    pub fn other_input(&mut self) {
        self.reset();
    }

    fn reset(&mut self) {
        self.down_at = None;
        self.last_tap = None;
        self.armed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> HotkeyConfig {
        HotkeyConfig::default() // 380 ms gap, 260 ms max tap
    }

    fn ms(base: Instant, n: u64) -> Instant {
        base + Duration::from_millis(n)
    }

    #[test]
    fn clean_double_tap_fires_once() {
        let (c, t, mut d) = (cfg(), Instant::now(), DoubleTap::default());
        d.trigger_down(ms(t, 0), false, &c);
        assert!(!d.trigger_up(ms(t, 80), &c));
        d.trigger_down(ms(t, 200), false, &c);
        assert!(d.trigger_up(ms(t, 270), &c));
        // A third tap starts a new sequence rather than firing again.
        d.trigger_down(ms(t, 400), false, &c);
        assert!(!d.trigger_up(ms(t, 450), &c));
    }

    #[test]
    fn slow_second_tap_does_not_fire() {
        let (c, t, mut d) = (cfg(), Instant::now(), DoubleTap::default());
        d.trigger_down(ms(t, 0), false, &c);
        d.trigger_up(ms(t, 80), &c);
        d.trigger_down(ms(t, 600), false, &c);
        assert!(!d.trigger_up(ms(t, 650), &c));
        // …but that late tap can itself be the first of a new double tap.
        d.trigger_down(ms(t, 800), false, &c);
        assert!(d.trigger_up(ms(t, 850), &c));
    }

    #[test]
    fn ctrl_c_never_counts() {
        let (c, t, mut d) = (cfg(), Instant::now(), DoubleTap::default());
        d.trigger_down(ms(t, 0), false, &c);
        d.other_input(); // C
        d.trigger_up(ms(t, 90), &c);
        d.trigger_down(ms(t, 150), false, &c);
        assert!(!d.trigger_up(ms(t, 200), &c));
    }

    #[test]
    fn copy_then_quick_tap_does_not_fire() {
        let (c, t, mut d) = (cfg(), Instant::now(), DoubleTap::default());
        d.trigger_down(ms(t, 0), false, &c);
        assert!(!d.trigger_up(ms(t, 60), &c));
        d.trigger_down(ms(t, 120), false, &c);
        d.other_input(); // Ctrl+C on the second press
        assert!(!d.trigger_up(ms(t, 180), &c));
    }

    #[test]
    fn hold_is_not_a_tap() {
        let (c, t, mut d) = (cfg(), Instant::now(), DoubleTap::default());
        d.trigger_down(ms(t, 0), false, &c);
        assert!(!d.trigger_up(ms(t, 500), &c));
        d.trigger_down(ms(t, 600), false, &c);
        assert!(!d.trigger_up(ms(t, 650), &c));
    }

    #[test]
    fn with_other_modifier_held_is_ignored() {
        let (c, t, mut d) = (cfg(), Instant::now(), DoubleTap::default());
        d.trigger_down(ms(t, 0), true, &c);
        d.trigger_up(ms(t, 50), &c);
        d.trigger_down(ms(t, 120), true, &c);
        assert!(!d.trigger_up(ms(t, 170), &c));
    }

    #[test]
    fn key_repeat_downs_are_ignored() {
        let (c, t, mut d) = (cfg(), Instant::now(), DoubleTap::default());
        d.trigger_down(ms(t, 0), false, &c);
        d.trigger_down(ms(t, 30), false, &c);
        d.trigger_up(ms(t, 60), &c);
        d.trigger_down(ms(t, 140), false, &c);
        d.trigger_down(ms(t, 170), false, &c);
        assert!(d.trigger_up(ms(t, 200), &c));
    }
}
