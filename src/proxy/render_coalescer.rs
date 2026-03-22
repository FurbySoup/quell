use std::time::{Duration, Instant};
use tracing::{debug, trace};

/// Timer-based render coalescing state machine.
///
/// Coalesces rapid output into fewer, larger renders to reduce flicker.
/// Three states:
/// - Idle: no pending data, no timer
/// - NormalPending: data arrived, deadline = first_data_time + render_delay
/// - SyncPending: sync block completed, deadline = last_sync_time + sync_delay
#[derive(Debug)]
pub struct RenderCoalescer {
    state: CoalescerState,
    render_delay: Duration,
    sync_delay: Duration,
    min_frame_time: Duration,
    last_render_time: Option<Instant>,
}

#[derive(Debug)]
enum CoalescerState {
    Idle,
    NormalPending { deadline: Instant },
    SyncPending { deadline: Instant },
}

impl RenderCoalescer {
    pub fn new(render_delay: Duration, sync_delay: Duration, min_frame_time: Duration) -> Self {
        debug!(
            render_delay_ms = render_delay.as_millis() as u64,
            sync_delay_ms = sync_delay.as_millis() as u64,
            min_frame_time_ms = min_frame_time.as_millis() as u64,
            "initializing render coalescer"
        );
        Self {
            state: CoalescerState::Idle,
            render_delay,
            sync_delay,
            min_frame_time,
            last_render_time: None,
        }
    }

    /// Normal data arrived — start or maintain normal deadline.
    pub fn notify_data(&mut self) {
        match self.state {
            CoalescerState::Idle => {
                let deadline = Instant::now() + self.render_delay;
                trace!("coalescer: idle → normal pending");
                self.state = CoalescerState::NormalPending { deadline };
            }
            CoalescerState::NormalPending { .. } => {
                // Do NOT reset deadline — keeps latency bounded
                trace!("coalescer: normal pending, additional data (deadline unchanged)");
            }
            CoalescerState::SyncPending { .. } => {
                // Sync overrides normal — ignore normal data during sync
                trace!("coalescer: sync pending, ignoring normal data notification");
            }
        }
    }

    /// Sync block completed — switch to sync delay (resets deadline).
    pub fn notify_sync_block(&mut self) {
        let deadline = Instant::now() + self.sync_delay;
        trace!("coalescer: → sync pending (deadline reset)");
        self.state = CoalescerState::SyncPending { deadline };
    }

    /// Whether it's time to render: deadline passed AND fps cap satisfied.
    pub fn should_render(&self) -> bool {
        let deadline_passed = match &self.state {
            CoalescerState::Idle => false,
            CoalescerState::NormalPending { deadline }
            | CoalescerState::SyncPending { deadline } => Instant::now() >= *deadline,
        };

        if !deadline_passed {
            return false;
        }

        // Check fps cap
        if let Some(last) = self.last_render_time
            && last.elapsed() < self.min_frame_time
        {
            return false;
        }

        true
    }

    /// Duration until the next render should fire. Returns `None` if idle.
    pub fn time_until_render(&self) -> Option<Duration> {
        let deadline = match &self.state {
            CoalescerState::Idle => return None,
            CoalescerState::NormalPending { deadline }
            | CoalescerState::SyncPending { deadline } => *deadline,
        };

        let now = Instant::now();

        // Also consider fps cap
        let fps_deadline = self.last_render_time
            .map(|last| last + self.min_frame_time)
            .unwrap_or(now);

        let effective_deadline = deadline.max(fps_deadline);

        if now >= effective_deadline {
            Some(Duration::ZERO)
        } else {
            Some(effective_deadline - now)
        }
    }

    /// Mark that a render just happened. Resets to idle.
    pub fn mark_rendered(&mut self) {
        trace!("coalescer: rendered, → idle");
        self.state = CoalescerState::Idle;
        self.last_render_time = Some(Instant::now());
    }

    /// Whether the coalescer is in idle state (no pending data).
    pub fn is_idle(&self) -> bool {
        matches!(self.state, CoalescerState::Idle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    fn test_coalescer() -> RenderCoalescer {
        RenderCoalescer::new(
            Duration::from_millis(5),
            Duration::from_millis(50),
            Duration::from_millis(16),
        )
    }

    #[test]
    fn test_idle_no_timeout() {
        let c = test_coalescer();
        assert!(c.is_idle());
        assert!(!c.should_render());
        assert!(c.time_until_render().is_none());
    }

    #[test]
    fn test_normal_deadline() {
        let mut c = test_coalescer();
        c.notify_data();
        assert!(!c.is_idle());
        // Immediately after notify, deadline hasn't passed
        assert!(!c.should_render());
        // time_until_render should be Some
        assert!(c.time_until_render().is_some());
    }

    #[test]
    fn test_normal_deadline_fires() {
        let mut c = RenderCoalescer::new(
            Duration::from_millis(1),
            Duration::from_millis(50),
            Duration::ZERO, // no fps cap for this test
        );
        c.notify_data();
        thread::sleep(Duration::from_millis(5));
        assert!(c.should_render());
    }

    #[test]
    fn test_sync_deadline() {
        let mut c = test_coalescer();
        c.notify_sync_block();
        assert!(!c.is_idle());
        // sync delay is 50ms, shouldn't fire immediately
        assert!(!c.should_render());
    }

    #[test]
    fn test_sync_overrides_normal() {
        let mut c = RenderCoalescer::new(
            Duration::from_millis(1),
            Duration::from_millis(50),
            Duration::ZERO,
        );
        c.notify_data();
        // Before normal deadline fires, sync arrives
        c.notify_sync_block();
        // Even after normal delay passes, should still wait for sync delay
        thread::sleep(Duration::from_millis(5));
        assert!(!c.should_render());
    }

    #[test]
    fn test_normal_deadline_does_not_reset() {
        let mut c = RenderCoalescer::new(
            Duration::from_millis(10),
            Duration::from_millis(50),
            Duration::ZERO,
        );
        c.notify_data();
        thread::sleep(Duration::from_millis(5));
        // More data arrives — should NOT reset the deadline
        c.notify_data();
        thread::sleep(Duration::from_millis(8));
        // Original deadline (10ms) should have passed by now (13ms total)
        assert!(c.should_render());
    }

    #[test]
    fn test_sync_deadline_resets() {
        let mut c = RenderCoalescer::new(
            Duration::from_millis(5),
            Duration::from_millis(20),
            Duration::ZERO,
        );
        c.notify_sync_block();
        thread::sleep(Duration::from_millis(10));
        // Another sync block resets the deadline
        c.notify_sync_block();
        thread::sleep(Duration::from_millis(10));
        // Only 10ms since last sync, 20ms delay — should not fire
        assert!(!c.should_render());
        thread::sleep(Duration::from_millis(15));
        // Now 25ms since last sync — should fire
        assert!(c.should_render());
    }

    #[test]
    fn test_fps_cap() {
        let mut c = RenderCoalescer::new(
            Duration::from_millis(1),
            Duration::from_millis(50),
            Duration::from_millis(100), // aggressive fps cap
        );
        // First render
        c.notify_data();
        thread::sleep(Duration::from_millis(5));
        assert!(c.should_render());
        c.mark_rendered();

        // Second render — fps cap should block
        c.notify_data();
        thread::sleep(Duration::from_millis(5));
        assert!(!c.should_render(), "fps cap should prevent render");
    }

    #[test]
    fn test_mark_rendered_resets_to_idle() {
        let mut c = test_coalescer();
        c.notify_data();
        assert!(!c.is_idle());
        c.mark_rendered();
        assert!(c.is_idle());
        assert!(!c.should_render());
        assert!(c.time_until_render().is_none());
    }

    #[test]
    fn test_time_until_render_zero_when_past_deadline() {
        let mut c = RenderCoalescer::new(
            Duration::from_millis(1),
            Duration::from_millis(50),
            Duration::ZERO,
        );
        c.notify_data();
        thread::sleep(Duration::from_millis(5));
        let t = c.time_until_render().unwrap();
        assert_eq!(t, Duration::ZERO);
    }
}
