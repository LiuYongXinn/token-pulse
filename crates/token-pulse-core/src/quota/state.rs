use super::*;
use crate::{
    error::ErrorCode,
    numeric::{DecimalInt, EpochMs},
    protocol::{QuotaSnapshot, QuotaState},
};
use std::collections::BTreeMap;
pub const QUOTA_REQUEST_TIMEOUT_MS: u64 = 10_000;
pub const QUOTA_MIN_REFRESH_MS: u64 = 5_000;
pub const QUOTA_STALE_MS: u64 = 300_000;
#[derive(Debug, Clone, Copy)]
pub struct QuotaTime {
    pub wall: EpochMs,
    pub monotonic_ms: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotaReadToken {
    pub connection_epoch: String,
    pub request_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaRefreshDecision {
    Started(QuotaReadToken),
    InFlight,
    RateLimited { retry_after_ms: u64 },
    NotDue,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaReplyOutcome {
    Published,
    Expired,
    Ignored,
}
#[derive(Clone)]
struct Cached {
    bucket: QuotaBucket,
    at: QuotaTime,
    publication: u64,
}
struct Pending {
    token: QuotaReadToken,
    deadline: u64,
    publication: u64,
}
pub struct QuotaCoordinator {
    instance: String,
    generation: u64,
    revision: u64,
    request_sequence: u64,
    publication: u64,
    state: QuotaState,
    eligible: bool,
    proven_read: bool,
    book: BTreeMap<String, Cached>,
    explicit_selection: Option<String>,
    pending: Option<Pending>,
    failures: usize,
    not_before: u64,
    last_attempt: Option<EpochMs>,
    last_read_mono: Option<u64>,
    error: Option<ErrorCode>,
    last_mono: u64,
}
impl QuotaCoordinator {
    pub fn new(instance: &str) -> Result<Self, ErrorCode> {
        if instance.is_empty()
            || instance.len() > 64
            || !instance
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return Err(ErrorCode::InvalidQuery);
        }
        Ok(Self {
            instance: instance.into(),
            generation: 0,
            revision: 0,
            request_sequence: 0,
            publication: 0,
            state: QuotaState::Disconnected,
            eligible: false,
            proven_read: false,
            book: BTreeMap::new(),
            explicit_selection: None,
            pending: None,
            failures: 0,
            not_before: 0,
            last_attempt: None,
            last_read_mono: None,
            error: None,
            last_mono: 0,
        })
    }
    pub fn epoch(&self) -> String {
        format!("quota-{}-{}", self.instance, self.generation)
    }
    fn bump(&mut self) -> Result<(), ErrorCode> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(ErrorCode::NumericOverflow)?;
        Ok(())
    }
    fn clock(&mut self, at: QuotaTime) -> Result<(), ErrorCode> {
        if at.monotonic_ms < self.last_mono {
            return Err(ErrorCode::InvalidQuery);
        }
        self.last_mono = at.monotonic_ms;
        Ok(())
    }
    fn rotate(&mut self, state: QuotaState, at: QuotaTime) -> Result<String, ErrorCode> {
        self.clock(at)?;
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(ErrorCode::NumericOverflow)?;
        self.bump()?;
        self.generation = generation;
        self.state = state;
        self.eligible = false;
        self.proven_read = false;
        self.book.clear();
        self.explicit_selection = None;
        self.pending = None;
        self.failures = 0;
        self.not_before = at.monotonic_ms;
        self.last_attempt = None;
        self.last_read_mono = None;
        self.error = None;
        Ok(self.epoch())
    }
    pub fn begin_connection(&mut self, at: QuotaTime) -> Result<String, ErrorCode> {
        self.rotate(QuotaState::Connecting, at)
    }
    /// Any observable identity change clears quota before a new account/read + quota/read proof.
    pub fn account_changed(&mut self, at: QuotaTime) -> Result<String, ErrorCode> {
        if matches!(self.state, QuotaState::Disconnected) {
            return Err(ErrorCode::QuotaDisconnected);
        }
        self.rotate(QuotaState::Connecting, at)
    }
    pub fn disconnect(&mut self, at: QuotaTime) -> Result<String, ErrorCode> {
        self.rotate(QuotaState::Disconnected, at)
    }
    pub fn account_result(
        &mut self,
        epoch: &str,
        availability: AccountAvailability,
        at: QuotaTime,
    ) -> Result<bool, ErrorCode> {
        if epoch != self.epoch() || matches!(self.state, QuotaState::Disconnected) {
            return Ok(false);
        }
        self.clock(at)?;
        self.bump()?;
        self.eligible = availability == AccountAvailability::QuotaEligible;
        self.state = match availability {
            AccountAvailability::QuotaEligible => QuotaState::Connecting,
            AccountAvailability::AuthorizationRequired => QuotaState::AuthorizationRequired,
            AccountAvailability::Unsupported => QuotaState::Unsupported,
        };
        self.error = match availability {
            AccountAvailability::QuotaEligible => None,
            AccountAvailability::AuthorizationRequired => Some(ErrorCode::QuotaAuthRequired),
            AccountAvailability::Unsupported => Some(ErrorCode::QuotaUnsupported),
        };
        if !self.eligible {
            self.book.clear();
            self.pending = None;
            self.proven_read = false;
        }
        Ok(true)
    }
    fn selected_id(&self) -> Option<&str> {
        if let Some(explicit) = &self.explicit_selection {
            return self
                .book
                .contains_key(explicit)
                .then_some(explicit.as_str());
        }
        if self.book.contains_key("codex") {
            Some("codex")
        } else if self.book.len() == 1 {
            self.book.keys().next().map(String::as_str)
        } else {
            None
        }
    }
    pub fn select_limit(&mut self, id: &str) -> Result<(), ErrorCode> {
        if !self.book.contains_key(id) {
            return Err(ErrorCode::InvalidQuery);
        }
        if self.explicit_selection.as_deref() != Some(id) {
            self.bump()?;
            self.explicit_selection = Some(id.into());
        }
        Ok(())
    }
    pub fn begin_refresh(
        &mut self,
        at: QuotaTime,
        explicit: bool,
        visible: bool,
    ) -> Result<QuotaRefreshDecision, ErrorCode> {
        self.clock(at)?;
        if !self.eligible {
            return Err(match self.state {
                QuotaState::AuthorizationRequired => ErrorCode::QuotaAuthRequired,
                QuotaState::Unsupported => ErrorCode::QuotaUnsupported,
                _ => ErrorCode::QuotaDisconnected,
            });
        }
        if self.pending.is_some() {
            return Ok(QuotaRefreshDecision::InFlight);
        }
        if at.monotonic_ms < self.not_before {
            return Ok(QuotaRefreshDecision::RateLimited {
                retry_after_ms: self.not_before - at.monotonic_ms,
            });
        }
        let data_mono = self
            .selected_id()
            .and_then(|id| self.book.get(id))
            .map(|e| e.at.monotonic_ms)
            .or(self.last_read_mono);
        if !explicit
            && self.error.is_none()
            && data_mono.is_some_and(|mono| {
                at.monotonic_ms.saturating_sub(mono) < if visible { 60_000 } else { 300_000 }
            })
        {
            return Ok(QuotaRefreshDecision::NotDue);
        }
        let deadline = at
            .monotonic_ms
            .checked_add(QUOTA_REQUEST_TIMEOUT_MS)
            .ok_or(ErrorCode::NumericOverflow)?;
        let sequence = self
            .request_sequence
            .checked_add(1)
            .ok_or(ErrorCode::NumericOverflow)?;
        let token = QuotaReadToken {
            connection_epoch: self.epoch(),
            request_id: format!("{}-read-{sequence}", self.epoch()),
        };
        self.bump()?;
        self.request_sequence = sequence;
        self.pending = Some(Pending {
            token: token.clone(),
            deadline,
            publication: self.publication,
        });
        self.last_attempt = Some(at.wall);
        Ok(QuotaRefreshDecision::Started(token))
    }
    fn accepts(&self, token: &QuotaReadToken) -> bool {
        self.pending.as_ref().is_some_and(|p| p.token == *token)
            && token.connection_epoch == self.epoch()
    }
    fn publish(
        &mut self,
        book: QuotaBook,
        at: QuotaTime,
        read_since: Option<u64>,
    ) -> Result<(), ErrorCode> {
        let publication = self
            .publication
            .checked_add(1)
            .ok_or(ErrorCode::NumericOverflow)?;
        let mut next: BTreeMap<String, Cached> = book
            .into_iter()
            .map(|(id, bucket)| {
                (
                    id,
                    Cached {
                        bucket,
                        at,
                        publication,
                    },
                )
            })
            .collect();
        // A notification received after this request began can be newer than its reply.
        // Keep that entire bucket (and its actual timestamp), never merge its percentages.
        if let Some(since) = read_since {
            for (id, entry) in &self.book {
                if entry.publication > since {
                    next.insert(id.clone(), entry.clone());
                }
            }
        }
        if next.len() > MAX_QUOTA_BUCKETS {
            return Err(ErrorCode::QuotaProtocolError);
        }
        self.bump()?;
        self.book = next;
        self.publication = publication;
        Ok(())
    }
    pub fn read_succeeded(
        &mut self,
        token: &QuotaReadToken,
        book: QuotaBook,
        at: QuotaTime,
    ) -> Result<QuotaReplyOutcome, ErrorCode> {
        if !self.accepts(token) {
            return Ok(QuotaReplyOutcome::Ignored);
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|p| at.monotonic_ms >= p.deadline)
        {
            self.read_failed(token, ErrorCode::QuotaTimeout, at)?;
            return Ok(QuotaReplyOutcome::Expired);
        }
        if book.len() > MAX_QUOTA_BUCKETS {
            return Err(ErrorCode::QuotaProtocolError);
        }
        self.clock(at)?;
        let not_before = at
            .monotonic_ms
            .checked_add(QUOTA_MIN_REFRESH_MS)
            .ok_or(ErrorCode::NumericOverflow)?;
        let since = self.pending.as_ref().expect("checked request").publication;
        self.publish(book, at, Some(since))?;
        self.pending = None;
        self.proven_read = true;
        self.state = QuotaState::Ready;
        self.error = None;
        self.failures = 0;
        self.not_before = not_before;
        self.last_read_mono = Some(at.monotonic_ms);
        Ok(QuotaReplyOutcome::Published)
    }
    pub fn read_failed(
        &mut self,
        token: &QuotaReadToken,
        error: ErrorCode,
        at: QuotaTime,
    ) -> Result<bool, ErrorCode> {
        if !self.accepts(token) {
            return Ok(false);
        }
        self.clock(at)?;
        if !matches!(
            error,
            ErrorCode::QuotaTimeout
                | ErrorCode::QuotaProtocolError
                | ErrorCode::QuotaServiceUnavailable
                | ErrorCode::QuotaUnsupported
                | ErrorCode::QuotaAuthRequired
        ) {
            return Err(ErrorCode::InvalidQuery);
        }
        let failures = (self.failures + 1).min(4);
        let delay = [5_000, 15_000, 30_000, 60_000][failures - 1];
        let next = at
            .monotonic_ms
            .checked_add(delay)
            .ok_or(ErrorCode::NumericOverflow)?;
        self.bump()?;
        self.pending = None;
        self.failures = failures;
        self.not_before = next;
        self.error = Some(error);
        if matches!(
            error,
            ErrorCode::QuotaAuthRequired | ErrorCode::QuotaUnsupported
        ) {
            self.eligible = false;
            self.proven_read = false;
            self.book.clear();
            self.state = if error == ErrorCode::QuotaAuthRequired {
                QuotaState::AuthorizationRequired
            } else {
                QuotaState::Unsupported
            };
        } else {
            self.state = if self.selected_id().is_some() {
                QuotaState::Stale
            } else {
                QuotaState::Error
            };
        }
        Ok(true)
    }
    pub fn notification(
        &mut self,
        epoch: &str,
        update: QuotaUpdate,
        at: QuotaTime,
    ) -> Result<bool, ErrorCode> {
        if epoch != self.epoch() || !self.eligible || !self.proven_read {
            return Ok(false);
        }
        self.clock(at)?;
        let (next, updates_selected) = match update {
            QuotaUpdate::Replace(book) => (book, true),
            QuotaUpdate::Bucket(bucket) => {
                if self.book.len() >= MAX_QUOTA_BUCKETS
                    && !self.book.contains_key(&bucket.limit.limit_id)
                {
                    return Err(ErrorCode::QuotaProtocolError);
                }
                let id = bucket.limit.limit_id.clone();
                let publication = self
                    .publication
                    .checked_add(1)
                    .ok_or(ErrorCode::NumericOverflow)?;
                self.bump()?;
                self.book.insert(
                    bucket.limit.limit_id.clone(),
                    Cached {
                        bucket,
                        at,
                        publication,
                    },
                );
                self.publication = publication;
                if self.selected_id() == Some(id.as_str()) {
                    self.state = QuotaState::Ready;
                    self.error = None;
                    self.failures = 0;
                }
                return Ok(true);
            }
            QuotaUpdate::Unidentified(bucket) => {
                if self.book.len() != 1
                    || !self
                        .book
                        .values()
                        .next()
                        .is_some_and(|e| e.bucket.legacy_identity)
                {
                    return Ok(false);
                }
                (
                    BTreeMap::from([(bucket.limit.limit_id.clone(), bucket)]),
                    true,
                )
            }
        };
        self.publish(next, at, None)?;
        if updates_selected {
            self.state = QuotaState::Ready;
            self.error = None;
            self.failures = 0;
        }
        Ok(true)
    }
    /// The service owns scheduling; this clock step never changes a remaining percentage.
    pub fn tick(&mut self, at: QuotaTime) -> Result<bool, ErrorCode> {
        self.clock(at)?;
        if let Some(token) = self
            .pending
            .as_ref()
            .filter(|p| at.monotonic_ms >= p.deadline)
            .map(|p| p.token.clone())
        {
            return self.read_failed(&token, ErrorCode::QuotaTimeout, at);
        }
        if matches!(self.state, QuotaState::Ready)
            && self
                .selected_id()
                .and_then(|id| self.book.get(id))
                .is_some_and(|e| {
                    at.monotonic_ms.saturating_sub(e.at.monotonic_ms) >= QUOTA_STALE_MS
                })
        {
            self.bump()?;
            self.state = QuotaState::Stale;
            return Ok(true);
        }
        Ok(false)
    }
    pub fn snapshot(&self) -> QuotaSnapshot {
        let selected = self.selected_id();
        let entry = selected.and_then(|id| self.book.get(id));
        QuotaSnapshot {
            connection_epoch: self.epoch(),
            quota_revision: DecimalInt::from_nonnegative(self.revision.into())
                .expect("u64 fits nonnegative i128"),
            state: self.state,
            selected_limit_id: selected.map(str::to_owned),
            available_limits: self.book.values().map(|e| e.bucket.limit.clone()).collect(),
            fetched_at_ms: entry.map(|e| e.at.wall),
            last_attempt_at_ms: self.last_attempt,
            windows: entry.map(|e| e.bucket.windows.clone()).unwrap_or_default(),
            error_code: self.error.map(|e| e.to_string()),
        }
    }
}
