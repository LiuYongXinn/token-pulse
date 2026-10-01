//! Bounded coalescing of file work. Overflow demands reconciliation rather than silently losing hints.
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};
pub const MAX_PENDING_FILES: usize = 4096;
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WorkPriority {
    Live,
    Startup,
    Historical,
}
struct Work {
    priority: WorkPriority,
    first_seen: Instant,
    not_before: Instant,
    sequence: u64,
}
#[derive(Default)]
pub struct WorkQueue {
    pending: BTreeMap<String, Work>,
    sequence: u64,
    reconcile_required: bool,
}
impl WorkQueue {
    pub fn enqueue(
        &mut self,
        file_id: String,
        priority: WorkPriority,
        now: Instant,
        debounce: Duration,
    ) -> bool {
        let debounce = debounce.min(Duration::from_secs(2));
        if let Some(work) = self.pending.get_mut(&file_id) {
            if priority == WorkPriority::Live && work.priority == WorkPriority::Live {
                work.not_before = (now + debounce).min(work.first_seen + Duration::from_secs(2));
            } else if priority < work.priority {
                work.not_before = now + debounce;
                work.first_seen = now;
            }
            work.priority = work.priority.min(priority);
            return true;
        }
        if self.pending.len() >= MAX_PENDING_FILES || self.sequence == u64::MAX {
            self.reconcile_required = true;
            return false;
        }
        self.sequence += 1;
        self.pending.insert(
            file_id,
            Work {
                priority,
                first_seen: now,
                not_before: now + debounce,
                sequence: self.sequence,
            },
        );
        true
    }
    pub fn pop_ready(&mut self, now: Instant) -> Option<String> {
        let key = self
            .pending
            .iter()
            .filter(|(_, w)| w.not_before <= now)
            .min_by_key(|(_, w)| (w.priority, w.sequence))
            .map(|(key, _)| key.clone())?;
        self.pending.remove(&key);
        Some(key)
    }
    pub fn len(&self) -> usize {
        self.pending.len()
    }
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
    pub fn take_reconcile_required(&mut self) -> bool {
        std::mem::take(&mut self.reconcile_required)
    }
}
