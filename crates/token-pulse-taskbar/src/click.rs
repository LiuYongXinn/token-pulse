//! Bounded display intentions. Monotonic times are supplied by the Win32 event loop.
use crate::{HostAction, MAX_HOST_ACTIONS};
use std::collections::VecDeque;
const ACTION_LIFETIME_MS: u64 = 5000;
#[derive(Default)]
pub(crate) struct ClickQueue {
    pending: Option<u64>,
    double_up: bool,
    actions: VecDeque<(u64, HostAction)>,
}
impl ClickQueue {
    pub(crate) fn release(&mut self, now: u64, double_click_ms: u32) -> bool {
        if self.double_up {
            self.double_up = false;
            return false;
        }
        if self.pending.take().is_some() {
            self.push(now, HostAction::OpenFloat {});
        }
        self.pending = Some(now.saturating_add(u64::from(double_click_ms.max(1))));
        true
    }
    pub(crate) fn double_click(&mut self, now: u64) {
        self.pending = None;
        self.double_up = true;
        self.push(now, HostAction::OpenStats {});
    }
    pub(crate) fn tick(&mut self, now: u64) {
        if self.pending.is_some_and(|deadline| now >= deadline) {
            self.pending = None;
            self.push(now, HostAction::OpenFloat {});
        }
    }
    pub(crate) fn push(&mut self, now: u64, action: HostAction) {
        self.actions
            .retain(|(time, _)| now.saturating_sub(*time) <= ACTION_LIFETIME_MS);
        if self.actions.len() < MAX_HOST_ACTIONS {
            self.actions.push_back((now, action));
        }
    }
    pub(crate) fn take(&mut self, now: u64) -> Vec<HostAction> {
        self.actions
            .drain(..)
            .filter(|(time, _)| now.saturating_sub(*time) <= ACTION_LIFETIME_MS)
            .map(|(_, action)| action)
            .collect()
    }
    pub(crate) fn clear(&mut self) {
        self.pending = None;
        self.double_up = false;
        self.actions.clear();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn single_click_obeys_system_deadline_and_double_click_emits_only_stats() {
        let mut clicks = ClickQueue::default();
        assert!(clicks.release(100, 600));
        clicks.tick(699);
        assert!(clicks.take(699).is_empty());
        clicks.tick(700);
        assert_eq!(clicks.take(700), [HostAction::OpenFloat {}]);
        clicks.tick(701);
        assert!(clicks.take(701).is_empty());
        clicks.release(1000, 600);
        clicks.double_click(1200);
        assert!(!clicks.release(1210, 600));
        clicks.tick(2000);
        assert_eq!(clicks.take(2000), [HostAction::OpenStats {}]);
    }
    #[test]
    fn queue_is_bounded_expires_and_clear_removes_pending_and_queued_intentions() {
        let mut clicks = ClickQueue::default();
        for _ in 0..30 {
            clicks.push(50, HostAction::OpenFloat {});
        }
        assert_eq!(clicks.take(5050).len(), 4);
        clicks.push(60, HostAction::OpenStats {});
        assert!(clicks.take(5061).is_empty());
        clicks.release(6000, 500);
        clicks.push(6000, HostAction::OpenStats {});
        clicks.clear();
        clicks.tick(7000);
        assert!(clicks.take(7000).is_empty());
        assert!(clicks.release(u64::MAX - 1, 500));
        clicks.tick(u64::MAX);
        assert_eq!(clicks.take(u64::MAX), [HostAction::OpenFloat {}]);
    }
}
