//! Reuse exact calendar aggregates across dashboard date and grain changes.
//! Coverage is assembled separately from the caller's current read transaction.
use super::BucketTotals;
use crate::{ErrorCode, StoreResult};
use rusqlite::Transaction;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex, Weak},
};
use token_pulse_core::{calendar::Grain, protocol::UsageFilter};

const LIMIT: usize = 20;
const BUDGET: usize = 8 * 1024 * 1024;
type Entry = (String, Arc<Vec<BucketTotals>>, usize);

#[derive(Default)]
pub(crate) struct SeriesCache {
    values: Mutex<VecDeque<Entry>>,
    flights: Mutex<BTreeMap<String, Weak<Mutex<()>>>>,
}

impl SeriesCache {
    pub(crate) fn get(
        &self,
        tx: &Transaction<'_>,
        filter: &UsageFilter,
        grain: Grain,
    ) -> StoreResult<Arc<Vec<BucketTotals>>> {
        filter.validate()?;
        let version = crate::database::read_usage_revision(tx)?;
        let mut normalized = filter.clone();
        for selection in [
            &mut normalized.sources,
            &mut normalized.models,
            &mut normalized.projects,
            &mut normalized.sessions,
        ] {
            if let token_pulse_core::protocol::DimensionSelection::Ids { ids, .. } = selection {
                ids.sort();
                ids.dedup();
            }
        }
        let key = serde_json::to_string(&(1, version, normalized, grain))?;
        let lock = {
            let mut flights = self.flights.lock().map_err(|_| ErrorCode::DbWriteFailed)?;
            flights.retain(|_, lock| lock.strong_count() > 0);
            if let Some(lock) = flights.get(&key).and_then(Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(Mutex::new(()));
                flights.insert(key.clone(), Arc::downgrade(&lock));
                lock
            }
        };
        let _exclusive = lock.lock().map_err(|_| ErrorCode::DbWriteFailed)?;
        let started = std::time::Instant::now();
        {
            let mut values = self.values.lock().map_err(|_| ErrorCode::DbWriteFailed)?;
            if let Some(index) = values.iter().position(|entry| entry.0 == key) {
                let entry = values.remove(index).ok_or(ErrorCode::DbCorrupt)?;
                let value = entry.1.clone();
                values.push_back(entry);
                crate::query_timing::record("series_cache_hit", started);
                return Ok(value);
            }
        }
        let value = Arc::new(super::series(tx, filter, grain)?);
        crate::query_timing::record("series_cache_compute", started);
        let bytes = serde_json::to_vec(value.as_ref())?.len();
        if bytes <= BUDGET {
            let mut values = self.values.lock().map_err(|_| ErrorCode::DbWriteFailed)?;
            while values.len() >= LIMIT
                || values.iter().map(|entry| entry.2).sum::<usize>() + bytes > BUDGET
            {
                values.pop_front();
            }
            values.push_back((key, value.clone(), bytes));
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests;
