//! # Repository contract — the c2 persistence boundary
//!
//! ADR `docs/adr/cluster-state-and-execution.md` (D1, D6, D8) and spec
//! `specs/task-c2-persistence-boundary-postgres.spec.md`.
//!
//! Business state that used to live in ad-hoc JSONL files / in-process
//! registries / oneshot channels is unified behind async **repositories**
//! grouped by a **Unit of Work**. A single business transition that touches
//! messages + run state + events + outbox commits them in ONE transaction —
//! never per-module `save_to_postgres()`, never a long-lived dual write.
//!
//! This module defines the shared contract and the **local adapter**
//! reference implementation (an in-memory store behind the same traits —
//! the real single-node file backend swaps in behind the identical traits
//! without touching callers). The PostgreSQL backend
//! (`octos-store-postgres`) implements the same traits against the
//! tenant-keyed schema; both backends run the same contract suite
//! (`crates/octos-store/tests/repository_contract.rs`).
//!
//! The traits use native `async fn` in trait (Rust ≥1.75); no `async-trait`
//! dependency is introduced. Implementations must never block the Tokio
//! runtime — a transitional synchronous bridge is bounded and off the main
//! executor (spec rule migration-safety).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use octos_core::execution_scope::Scope;

/// Errors a repository can return. These are backend-agnostic: the PG
/// backend maps its constraint violations onto the same variants so callers
/// never match on SQLSTATE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryError {
    /// The addressed aggregate does not exist (in this scope). Cross-scope
    /// lookups report `NotFound`, never the other tenant's row.
    NotFound,
    /// A unique constraint was violated (idempotency key, event_id, seq).
    Conflict,
    /// An approval/question already has a terminal decision — the CAS
    /// compare step failed (K05: replay rejected).
    AlreadyDecided,
    /// The reply's args_hash does not match the recorded pending request —
    /// a tampered or stale reply (K05: parameter change rejected).
    ArgsMismatch,
    /// The lease epoch carried by the write is stale (c3 fencing).
    StaleEpoch,
    /// Anything else; carries a human-readable message.
    Other(String),
}

impl std::fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RepositoryError::NotFound => write!(f, "not found"),
            RepositoryError::Conflict => write!(f, "conflict"),
            RepositoryError::AlreadyDecided => write!(f, "already decided"),
            RepositoryError::ArgsMismatch => write!(f, "args hash mismatch"),
            RepositoryError::StaleEpoch => write!(f, "stale epoch"),
            RepositoryError::Other(m) => write!(f, "{m}"),
        }
    }
}
impl std::error::Error for RepositoryError {}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// A canonical message to append within a scope.
#[derive(Debug, Clone)]
pub struct NewMessage {
    pub scope: Scope,
    pub message_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub role: String,
    pub content: String,
}

/// A stored canonical message.
#[derive(Debug, Clone)]
pub struct MessageRecord {
    pub message_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub role: String,
    pub content: String,
    /// Canonical order within the scope (fork/rollback preserved by the
    /// caller-provided id ordering; the store never reorders).
    pub ordinal: u64,
}

/// A session event to append to the scope's ledger.
#[derive(Debug, Clone)]
pub struct NewSessionEvent {
    pub scope: Scope,
    pub event_id: String,
    pub causation_id: Option<String>,
    pub payload: serde_json::Value,
}

/// A stored session event with its per-scope monotonic sequence (D6).
#[derive(Debug, Clone)]
pub struct SessionEvent {
    pub event_id: String,
    pub causation_id: Option<String>,
    /// Monotonic, gap-free, unique within the scope: `UNIQUE(Scope, seq)`.
    pub seq: u64,
    pub payload: serde_json::Value,
}

/// An outbox item committed in the same transaction as its event (D6).
#[derive(Debug, Clone)]
pub struct OutboxItem {
    pub aggregate_key: String,
    pub topic: String,
    pub payload: serde_json::Value,
}

/// State of a durable approval/question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalState {
    Pending,
    Decided,
    Expired,
}

/// The decision written by a reply CAS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    Approved,
    Rejected,
}

/// A pending approval to persist (durable waiting — the oneshot is only an
/// in-process accelerator, never the source of truth).
#[derive(Debug, Clone)]
pub struct NewApproval {
    pub scope: Scope,
    pub approval_id: String,
    pub originating_run: String,
    pub args_hash: String,
    pub binding_revision: String,
}

/// A stored approval record.
#[derive(Debug, Clone)]
pub struct ApprovalRecord {
    pub approval_id: String,
    pub originating_run: String,
    pub args_hash: String,
    pub binding_revision: String,
    pub state: ApprovalState,
    pub decision: Option<ApprovalDecision>,
}

// ---------------------------------------------------------------------------
// Read-side repository trait (queries are always scope-bound)
// ---------------------------------------------------------------------------

/// Read-side access to committed state. Every query is bound to a `Scope`;
/// cross-scope isolation is structural, not filter-based.
pub trait StoreView: Send + Sync {
    fn messages_for(&self, scope: &Scope) -> Vec<MessageRecord>;
    fn events_for(&self, scope: &Scope) -> Vec<SessionEvent>;
    fn run_state(&self, run_id: &str) -> Option<String>;
    fn approval(&self, scope: &Scope, approval_id: &str) -> Option<ApprovalRecord>;
    /// Enumerate Pending approvals for a scope. Used by cluster pod
    /// startup / lazy rehydrate paths to discover durable approvals
    /// that originated on a peer and need to be re-registered into the
    /// in-process PendingApprovalStore on first access (K05 cross-pod
    /// recovery). Defaults to empty so backends that don't track a
    /// per-scope index don't have to implement it.
    fn pending_approvals_for_scope(&self, scope: &Scope) -> Vec<ApprovalRecord> {
        let _ = scope;
        Vec::new()
    }
    fn outbox(&self) -> Vec<OutboxItem>;

    /// Approval reply CAS (K05): the first matching reply wins; replay,
    /// cross-scope, and args-tamper are rejected. This is a single atomic
    /// compare-and-set on the stored record.
    fn reply_approval(
        &self,
        scope: &Scope,
        approval_id: &str,
        args_hash: &str,
        decision: ApprovalDecision,
    ) -> Result<ApprovalState, RepositoryError>;
}

// ---------------------------------------------------------------------------
// Unit of Work
// ---------------------------------------------------------------------------

/// The single transaction boundary for a business transition (D1).
///
/// Mutations are staged on the UoW and become visible only on `commit()`.
/// Dropping a UoW without commit publishes nothing. Implementations must
/// guarantee atomicity across all four aggregates in one commit.
pub trait UnitOfWork {
    type View: StoreView;

    fn append_message(&mut self, msg: NewMessage);
    fn set_run_state(&mut self, run_id: &str, state: &str);
    fn append_event(&mut self, event: NewSessionEvent);
    fn create_approval(&mut self, approval: NewApproval);
    fn enqueue_outbox(&mut self, item: OutboxItem);

    /// Commit all staged mutations atomically.
    fn commit(&mut self) -> impl std::future::Future<Output = Result<(), RepositoryError>> + Send;

    /// A handle to the shared committed view (for tests / read-after-commit).
    fn shared_store(&self) -> Arc<Self::View>;
}

// ---------------------------------------------------------------------------
// Run leases (c3 / D5): epoch-fenced single ownership of an execution scope
// ---------------------------------------------------------------------------

/// A held run lease. `epoch` is the fencing token: it increments on every
/// claim/takeover, and every business-state write must carry the current
/// epoch to be accepted (a stale epoch ⇒ [`RepositoryError::StaleEpoch`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunLease {
    pub run_id: String,
    pub owner_id: String,
    pub epoch: u64,
    /// Milliseconds since the Unix epoch when the lease lapses.
    pub expires_at_ms: u64,
}

/// Epoch-fenced lease operations. Claiming and epoch increment happen in ONE
/// transaction; the trait is separate from [`UnitOfWork`] because a lease
/// claim is its own atomic transaction, not part of a business transition.
pub trait LeaseStore: Send + Sync {
    /// Claim (or renew) the lease for `run_id` as `owner_id`. Returns the new
    /// lease with its epoch. Fails with [`RepositoryError::Conflict`] when a
    /// different owner holds an unexpired lease (K02: single owner). Same-owner
    /// re-claim renews. An EXPIRED lease is taken over with epoch+1 (K03).
    fn claim(
        &self,
        scope: &Scope,
        run_id: &str,
        owner_id: &str,
        lease_ms: u64,
        now_ms: u64,
    ) -> impl std::future::Future<Output = Result<RunLease, RepositoryError>> + Send;

    /// Write business state gated on the epoch: succeeds only when `epoch`
    /// equals the run's current lease epoch. Returns
    /// [`RepositoryError::StaleEpoch`] otherwise (K03 fencing).
    fn write_with_epoch(
        &self,
        scope: &Scope,
        run_id: &str,
        epoch: u64,
        state: &str,
    ) -> impl std::future::Future<Output = Result<(), RepositoryError>> + Send;

    /// The current lease for a run, if any (recovery scan / takeover).
    fn current_lease(
        &self,
        scope: &Scope,
        run_id: &str,
    ) -> impl std::future::Future<Output = Option<RunLease>> + Send;
}

// ---------------------------------------------------------------------------
// c3: run checkpoints + tool-invocation idempotency ledger (D3/D4)
// ---------------------------------------------------------------------------

/// A committed recovery checkpoint for a run (D3). Recovery rebuilds the
/// runtime from the latest valid checkpoint — never from in-process memory.
#[derive(Debug, Clone)]
pub struct NewCheckpoint {
    pub scope: Scope,
    pub run_id: String,
    pub step: u64,
    pub transcript_highwater: u64,
    pub context: Option<serde_json::Value>,
    pub workspace_revision: Option<String>,
    pub pending_invocation: Option<String>,
    pub artifact_refs: Option<serde_json::Value>,
    /// Pinned binding digest + permission snapshot (K11): a resumed run keeps
    /// the revision it started under even after the Binding is updated.
    pub binding_digest: Option<String>,
    pub permission_snapshot: Option<serde_json::Value>,
    /// Integrity digest over the checkpoint payload; mismatch on load fails
    /// closed (no guessing a corrupt checkpoint).
    pub digest: String,
    pub schema_version: String,
    pub runtime_version: String,
    /// The lease epoch this checkpoint was written under (fencing, D5).
    pub created_epoch: u64,
}

/// A stored checkpoint.
#[derive(Debug, Clone)]
pub struct CheckpointRecord {
    pub step: u64,
    pub transcript_highwater: u64,
    pub context: Option<serde_json::Value>,
    pub workspace_revision: Option<String>,
    pub pending_invocation: Option<String>,
    pub artifact_refs: Option<serde_json::Value>,
    pub binding_digest: Option<String>,
    pub permission_snapshot: Option<serde_json::Value>,
    pub digest: String,
    pub schema_version: String,
    pub runtime_version: String,
    pub created_epoch: u64,
}

/// Lifecycle of a tool side effect (D4). Recorded as an intent BEFORE
/// execution; the external idempotency key lets a post-crash takeover reuse a
/// confirmed result instead of re-firing (K04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvocationState {
    /// Intent persisted, execution not yet confirmed.
    Intent,
    /// External effect confirmed and result persisted.
    Succeeded,
    /// Execution failed cleanly (no side effect).
    Failed,
    /// Side effect may have happened but the result was not persisted (e.g.
    /// worker killed after external success). MUST go to explicit
    /// reconciliation — never auto-retried (K04).
    Unknown,
}

/// A tool-invocation intent to record before execution.
#[derive(Debug, Clone)]
pub struct NewInvocation {
    pub scope: Scope,
    pub run_id: String,
    pub invocation_id: String,
    pub tool_revision: String,
    pub args_hash: String,
    pub external_idempotency_key: Option<String>,
}

/// A stored tool invocation.
#[derive(Debug, Clone)]
pub struct InvocationRecord {
    pub invocation_id: String,
    pub tool_revision: String,
    pub args_hash: String,
    pub state: InvocationState,
    pub external_idempotency_key: Option<String>,
    pub result_ref: Option<String>,
    pub cost_micros: Option<i64>,
}

/// Checkpoint + tool-invocation store (c3). Separate from [`UnitOfWork`]
/// because checkpoints and invocation intents are committed at their own
/// boundaries (before/after a tool call, on approval park, on turn end),
/// not as part of a message/event business transition.
pub trait RecoveryStore: Send + Sync {
    /// Commit a checkpoint. Gated on `created_epoch` matching the run's
    /// current lease epoch (D5 fencing): a stale-epoch checkpoint write is
    /// rejected with [`RepositoryError::StaleEpoch`].
    fn commit_checkpoint(
        &self,
        cp: NewCheckpoint,
    ) -> impl std::future::Future<Output = Result<(), RepositoryError>> + Send;

    /// The latest valid checkpoint for a run (highest step). The caller
    /// verifies `digest` before trusting the payload.
    fn latest_checkpoint(
        &self,
        scope: &Scope,
        run_id: &str,
    ) -> impl std::future::Future<Output = Option<CheckpointRecord>> + Send;

    /// Record a tool-invocation intent before execution. A retry reuses the
    /// same `invocation_id`; a mismatched `args_hash` for a known id is
    /// rejected ([`RepositoryError::ArgsMismatch`]) so a tampered retry can't
    /// ride a prior idempotency record (K04).
    fn record_intent(
        &self,
        inv: NewInvocation,
    ) -> impl std::future::Future<Output = Result<InvocationState, RepositoryError>> + Send;

    /// Mark an invocation succeeded with its persisted result reference.
    fn mark_succeeded(
        &self,
        scope: &Scope,
        run_id: &str,
        invocation_id: &str,
        result_ref: &str,
        cost_micros: Option<i64>,
    ) -> impl std::future::Future<Output = Result<(), RepositoryError>> + Send;

    /// Mark an invocation Unknown (side effect uncertain) — goes to explicit
    /// reconciliation, NOT auto-retry (K04).
    fn mark_unknown(
        &self,
        scope: &Scope,
        run_id: &str,
        invocation_id: &str,
    ) -> impl std::future::Future<Output = Result<(), RepositoryError>> + Send;

    /// Look up an invocation by its external idempotency key: a takeover uses
    /// this to reuse an already-confirmed effect instead of re-firing (K04).
    fn find_by_idempotency_key(
        &self,
        scope: &Scope,
        external_key: &str,
    ) -> impl std::future::Future<Output = Option<InvocationRecord>> + Send;

    /// Look up an invocation by its logical id (for reading the confirmed
    /// result_ref of a `ReuseConfirmed` verdict).
    fn invocation_by_id(
        &self,
        scope: &Scope,
        run_id: &str,
        invocation_id: &str,
    ) -> impl std::future::Future<Output = Option<InvocationRecord>> + Send;
}

// ---------------------------------------------------------------------------
// c5: durable Cron firing (K10). Two concurrent Controller scans at the
// same instant yield exactly one firing and one run; the
// (schedule_id, scheduled_at) PRIMARY KEY enforces it transactionally.
// ---------------------------------------------------------------------------

/// A registered Cron-style schedule (c5). `next_fire_at` is computed by the
/// owning controller; `enabled=false` suspends firing without losing the
/// definition.
#[derive(Debug, Clone)]
pub struct Schedule {
    pub schedule_id: String,
    pub expression: String,
    pub timezone: Option<String>,
    pub next_fire_at_ms: Option<u64>,
    pub misfire_policy: MisfirePolicy,
    pub enabled: bool,
}

/// What a schedule does after a missed fire (controller lapsed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MisfirePolicy {
    /// Do not fire for any missed instant.
    Skip,
    /// Fire exactly once for the most-recent missed instant.
    RunOnce,
    /// Fire for every missed instant (capped, in implementation).
    CatchUp,
}

/// The lifecycle of a firing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FiringState {
    Intent,
    Running,
    Succeeded,
    Failed,
}

/// A stored firing record.
#[derive(Debug, Clone)]
pub struct ScheduleFiring {
    pub schedule_id: String,
    pub scheduled_at_ms: u64,
    pub firing_id: String,
    pub claimed_by: Option<String>,
    pub state: FiringState,
    pub run_id: Option<String>,
}

/// Cron durable operations. A firing can be claimed at most once per
/// (schedule_id, scheduled_at); concurrent claimers produce one winner
/// (K10) and one Conflict.
pub trait CronScheduleStore: Send + Sync {
    /// Create a schedule. Duplicate `(scope, schedule_id)` returns
    /// [`RepositoryError::Conflict`].
    fn create_schedule(
        &self,
        scope: &Scope,
        schedule: Schedule,
    ) -> impl std::future::Future<Output = Result<(), RepositoryError>> + Send;

    /// Record a firing. Duplicate `(scope, schedule_id, scheduled_at)`
    /// returns [`RepositoryError::Conflict`] — K10 single-firing.
    fn record_firing(
        &self,
        scope: &Scope,
        firing: ScheduleFiring,
    ) -> impl std::future::Future<Output = Result<(), RepositoryError>> + Send;

    /// Claim a pending firing as `controller_id`; transitions `Intent` to
    /// `Running`. Returns the firing on success, [`RepositoryError::Conflict`]
    /// if another controller already claimed it (K10 single claim).
    fn claim_firing(
        &self,
        scope: &Scope,
        schedule_id: &str,
        scheduled_at_ms: u64,
        controller_id: &str,
    ) -> impl std::future::Future<Output = Result<ScheduleFiring, RepositoryError>> + Send;

    /// Mark a firing as terminal.
    fn mark_firing_terminal(
        &self,
        scope: &Scope,
        schedule_id: &str,
        scheduled_at_ms: u64,
        state: FiringState,
        run_id: Option<&str>,
    ) -> impl std::future::Future<Output = Result<(), RepositoryError>> + Send;
}

// ---------------------------------------------------------------------------
// Local adapter — reference implementation over an in-memory store
// ---------------------------------------------------------------------------

/// The shared committed state behind the local adapter. Thread-safe; the
/// single-node file backend swaps in behind `StoreView`/`UnitOfWork`
/// unchanged.
#[derive(Debug, Default)]
pub struct LocalStore {
    inner: Mutex<LocalInner>,
}

#[derive(Debug, Default)]
struct LocalInner {
    /// scope-keyed → ordered messages.
    messages: HashMap<ScopeKey, Vec<MessageRecord>>,
    events: HashMap<ScopeKey, Vec<SessionEvent>>,
    run_states: HashMap<String, String>,
    approvals: HashMap<ScopeKey, HashMap<String, ApprovalRecord>>,
    outbox: Vec<OutboxItem>,
    /// Per-scope next event seq (gap-free, monotonic).
    next_seq: HashMap<ScopeKey, u64>,
    /// Per-scope next message ordinal.
    next_ordinal: HashMap<ScopeKey, u64>,
    /// c3: run leases, keyed by (scope, run_id). Epoch fencing (D5).
    leases: HashMap<(ScopeKey, String), RunLease>,
    /// c3: checkpoints, keyed by (scope, run_id) → ordered by step.
    checkpoints: HashMap<(ScopeKey, String), Vec<CheckpointRecord>>,
    /// c3: tool invocations, keyed by (scope, run_id, invocation_id).
    invocations: HashMap<(ScopeKey, String, String), InvocationRecord>,
    /// c5: cron schedules, keyed by (scope, schedule_id).
    schedules: HashMap<(ScopeKey, String), Schedule>,
    /// c5: schedule firings, keyed by (scope, schedule_id, scheduled_at_ms).
    firings: HashMap<(ScopeKey, String, u64), ScheduleFiring>,
}

/// A hashable key derived from a `Scope` (the four identity components).
/// Two scopes are equal iff all four components match — the wire session id
/// alone NEVER identifies a bucket (K07).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ScopeKey {
    tenant_id: String,
    profile_id: String,
    workspace_id: String,
    session_id: String,
}

impl From<&Scope> for ScopeKey {
    fn from(s: &Scope) -> Self {
        Self {
            tenant_id: s.tenant_id().to_string(),
            profile_id: s.profile_id().to_string(),
            workspace_id: s.workspace_id().to_string(),
            session_id: s.session_id().to_string(),
        }
    }
}

impl StoreView for LocalStore {
    fn messages_for(&self, scope: &Scope) -> Vec<MessageRecord> {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner
            .messages
            .get(&ScopeKey::from(scope))
            .cloned()
            .unwrap_or_default()
    }

    fn events_for(&self, scope: &Scope) -> Vec<SessionEvent> {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner
            .events
            .get(&ScopeKey::from(scope))
            .cloned()
            .unwrap_or_default()
    }

    fn run_state(&self, run_id: &str) -> Option<String> {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.run_states.get(run_id).cloned()
    }

    fn approval(&self, scope: &Scope, approval_id: &str) -> Option<ApprovalRecord> {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner
            .approvals
            .get(&ScopeKey::from(scope))
            .and_then(|m| m.get(approval_id))
            .cloned()
    }

    fn outbox(&self) -> Vec<OutboxItem> {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.outbox.clone()
    }

    fn reply_approval(
        &self,
        scope: &Scope,
        approval_id: &str,
        args_hash: &str,
        decision: ApprovalDecision,
    ) -> Result<ApprovalState, RepositoryError> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let record = inner
            .approvals
            .get_mut(&ScopeKey::from(scope))
            .and_then(|m| m.get_mut(approval_id))
            .ok_or(RepositoryError::NotFound)?;
        // CAS: compare state (must be Pending) then args_hash.
        if record.state != ApprovalState::Pending {
            return Err(RepositoryError::AlreadyDecided);
        }
        if record.args_hash != args_hash {
            return Err(RepositoryError::ArgsMismatch);
        }
        record.state = ApprovalState::Decided;
        record.decision = Some(decision);
        Ok(ApprovalState::Decided)
    }
}

/// Staged mutations, flushed to the shared `LocalStore` on commit.
pub struct LocalUnitOfWork {
    store: Arc<LocalStore>,
    staged_messages: Vec<NewMessage>,
    staged_run_states: Vec<(String, String)>,
    staged_events: Vec<NewSessionEvent>,
    staged_approvals: Vec<NewApproval>,
    staged_outbox: Vec<OutboxItem>,
}

impl LocalUnitOfWork {
    pub fn begin() -> Self {
        Self::with_store(Arc::new(LocalStore::default()))
    }

    pub fn with_store(store: Arc<LocalStore>) -> Self {
        Self {
            store,
            staged_messages: Vec::new(),
            staged_run_states: Vec::new(),
            staged_events: Vec::new(),
            staged_approvals: Vec::new(),
            staged_outbox: Vec::new(),
        }
    }

    // Test/convenience read-backs over the committed store.
    pub fn committed_messages(&self, scope: &Scope) -> Vec<MessageRecord> {
        self.store.messages_for(scope)
    }
    pub fn committed_events(&self, scope: &Scope) -> Vec<SessionEvent> {
        self.store.events_for(scope)
    }
    pub fn committed_run_state(&self, run_id: &str) -> Option<String> {
        self.store.run_state(run_id)
    }
    pub fn committed_outbox(&self) -> Vec<OutboxItem> {
        self.store.outbox()
    }
}

impl LeaseStore for LocalStore {
    async fn claim(
        &self,
        scope: &Scope,
        run_id: &str,
        owner_id: &str,
        lease_ms: u64,
        now_ms: u64,
    ) -> Result<RunLease, RepositoryError> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let key = (ScopeKey::from(scope), run_id.to_string());
        let new_lease = match inner.leases.get(&key) {
            None => RunLease {
                run_id: run_id.to_string(),
                owner_id: owner_id.to_string(),
                epoch: 1,
                expires_at_ms: now_ms + lease_ms,
            },
            Some(existing) if existing.owner_id == owner_id => RunLease {
                // Same owner renews: epoch unchanged, expiry extended.
                expires_at_ms: now_ms + lease_ms,
                ..existing.clone()
            },
            Some(existing) if existing.expires_at_ms <= now_ms => RunLease {
                // Expired: takeover increments the epoch (K03 fencing).
                owner_id: owner_id.to_string(),
                epoch: existing.epoch + 1,
                expires_at_ms: now_ms + lease_ms,
                ..existing.clone()
            },
            Some(_) => {
                // A different owner holds an unexpired lease (K02).
                return Err(RepositoryError::Conflict);
            }
        };
        inner.leases.insert(key, new_lease.clone());
        Ok(new_lease)
    }

    async fn write_with_epoch(
        &self,
        scope: &Scope,
        run_id: &str,
        epoch: u64,
        state: &str,
    ) -> Result<(), RepositoryError> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let key = (ScopeKey::from(scope), run_id.to_string());
        match inner.leases.get(&key) {
            Some(lease) if lease.epoch == epoch => {
                inner
                    .run_states
                    .insert(run_id.to_string(), state.to_string());
                Ok(())
            }
            Some(_) => Err(RepositoryError::StaleEpoch),
            None => Err(RepositoryError::NotFound),
        }
    }

    async fn current_lease(&self, scope: &Scope, run_id: &str) -> Option<RunLease> {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner
            .leases
            .get(&(ScopeKey::from(scope), run_id.to_string()))
            .cloned()
    }
}

impl RecoveryStore for LocalStore {
    async fn commit_checkpoint(&self, cp: NewCheckpoint) -> Result<(), RepositoryError> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let lease_key = (ScopeKey::from(&cp.scope), cp.run_id.clone());
        // Fencing (D5): the checkpoint's epoch must match the current lease.
        match inner.leases.get(&lease_key) {
            Some(lease) if lease.epoch == cp.created_epoch => {}
            Some(_) => return Err(RepositoryError::StaleEpoch),
            None => return Err(RepositoryError::NotFound),
        }
        let key = (ScopeKey::from(&cp.scope), cp.run_id.clone());
        let list = inner.checkpoints.entry(key).or_default();
        // Replace an existing checkpoint at the same step (idempotent recommit)
        // or append; keep ordered by step.
        let record = CheckpointRecord {
            step: cp.step,
            transcript_highwater: cp.transcript_highwater,
            context: cp.context,
            workspace_revision: cp.workspace_revision,
            pending_invocation: cp.pending_invocation,
            artifact_refs: cp.artifact_refs,
            binding_digest: cp.binding_digest,
            permission_snapshot: cp.permission_snapshot,
            digest: cp.digest,
            schema_version: cp.schema_version,
            runtime_version: cp.runtime_version,
            created_epoch: cp.created_epoch,
        };
        match list.binary_search_by_key(&record.step, |c| c.step) {
            Ok(i) => list[i] = record,
            Err(i) => list.insert(i, record),
        }
        Ok(())
    }

    async fn latest_checkpoint(&self, scope: &Scope, run_id: &str) -> Option<CheckpointRecord> {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner
            .checkpoints
            .get(&(ScopeKey::from(scope), run_id.to_string()))
            .and_then(|list| list.last().cloned())
    }

    async fn record_intent(&self, inv: NewInvocation) -> Result<InvocationState, RepositoryError> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let key = (
            ScopeKey::from(&inv.scope),
            inv.run_id.clone(),
            inv.invocation_id.clone(),
        );
        if let Some(existing) = inner.invocations.get(&key) {
            // A retry reusing the logical id must match the recorded args_hash
            // (K04: a tampered retry can't ride a prior idempotency record).
            if existing.args_hash != inv.args_hash {
                return Err(RepositoryError::ArgsMismatch);
            }
            // Already known: report its current state so the caller can reuse
            // a confirmed result (Succeeded) or route Unknown to
            // reconciliation instead of re-firing.
            return Ok(existing.state);
        }
        inner.invocations.insert(
            key,
            InvocationRecord {
                invocation_id: inv.invocation_id,
                tool_revision: inv.tool_revision,
                args_hash: inv.args_hash,
                state: InvocationState::Intent,
                external_idempotency_key: inv.external_idempotency_key,
                result_ref: None,
                cost_micros: None,
            },
        );
        Ok(InvocationState::Intent)
    }

    async fn mark_succeeded(
        &self,
        scope: &Scope,
        run_id: &str,
        invocation_id: &str,
        result_ref: &str,
        cost_micros: Option<i64>,
    ) -> Result<(), RepositoryError> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let key = (
            ScopeKey::from(scope),
            run_id.to_string(),
            invocation_id.to_string(),
        );
        let rec = inner
            .invocations
            .get_mut(&key)
            .ok_or(RepositoryError::NotFound)?;
        rec.state = InvocationState::Succeeded;
        rec.result_ref = Some(result_ref.to_string());
        rec.cost_micros = cost_micros;
        Ok(())
    }

    async fn mark_unknown(
        &self,
        scope: &Scope,
        run_id: &str,
        invocation_id: &str,
    ) -> Result<(), RepositoryError> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let key = (
            ScopeKey::from(scope),
            run_id.to_string(),
            invocation_id.to_string(),
        );
        let rec = inner
            .invocations
            .get_mut(&key)
            .ok_or(RepositoryError::NotFound)?;
        rec.state = InvocationState::Unknown;
        Ok(())
    }

    async fn find_by_idempotency_key(
        &self,
        scope: &Scope,
        external_key: &str,
    ) -> Option<InvocationRecord> {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let sk = ScopeKey::from(scope);
        inner
            .invocations
            .iter()
            .find(|((k, _, _), rec)| {
                *k == sk && rec.external_idempotency_key.as_deref() == Some(external_key)
            })
            .map(|(_, rec)| rec.clone())
    }

    async fn invocation_by_id(
        &self,
        scope: &Scope,
        run_id: &str,
        invocation_id: &str,
    ) -> Option<InvocationRecord> {
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner
            .invocations
            .get(&(
                ScopeKey::from(scope),
                run_id.to_string(),
                invocation_id.to_string(),
            ))
            .cloned()
    }
}

impl CronScheduleStore for LocalStore {
    async fn create_schedule(
        &self,
        scope: &Scope,
        schedule: Schedule,
    ) -> Result<(), RepositoryError> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let key = (ScopeKey::from(scope), schedule.schedule_id.clone());
        if inner.schedules.contains_key(&key) {
            return Err(RepositoryError::Conflict);
        }
        inner.schedules.insert(key, schedule);
        Ok(())
    }

    async fn record_firing(
        &self,
        scope: &Scope,
        firing: ScheduleFiring,
    ) -> Result<(), RepositoryError> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let key = (
            ScopeKey::from(scope),
            firing.schedule_id.clone(),
            firing.scheduled_at_ms,
        );
        if inner.firings.contains_key(&key) {
            // K10: a second controller racing the same instant must NOT
            // produce a second firing. Surface Conflict so the caller
            // routes the duplicate to the existing record instead of
            // double-running.
            return Err(RepositoryError::Conflict);
        }
        inner.firings.insert(key, firing);
        Ok(())
    }

    async fn claim_firing(
        &self,
        scope: &Scope,
        schedule_id: &str,
        scheduled_at_ms: u64,
        controller_id: &str,
    ) -> Result<ScheduleFiring, RepositoryError> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let key = (
            ScopeKey::from(scope),
            schedule_id.to_string(),
            scheduled_at_ms,
        );
        let firing = inner
            .firings
            .get_mut(&key)
            .ok_or(RepositoryError::NotFound)?;
        // K10 single-claim: only an Intent firing may be claimed; a running
        // firing belongs to another controller and is rejected.
        if firing.state != FiringState::Intent {
            return Err(RepositoryError::Conflict);
        }
        firing.claimed_by = Some(controller_id.to_string());
        firing.state = FiringState::Running;
        Ok(firing.clone())
    }

    async fn mark_firing_terminal(
        &self,
        scope: &Scope,
        schedule_id: &str,
        scheduled_at_ms: u64,
        state: FiringState,
        run_id: Option<&str>,
    ) -> Result<(), RepositoryError> {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let key = (
            ScopeKey::from(scope),
            schedule_id.to_string(),
            scheduled_at_ms,
        );
        let firing = inner
            .firings
            .get_mut(&key)
            .ok_or(RepositoryError::NotFound)?;
        firing.state = state;
        firing.run_id = run_id.map(str::to_string);
        Ok(())
    }
}

impl UnitOfWork for LocalUnitOfWork {
    type View = LocalStore;
    fn append_message(&mut self, msg: NewMessage) {
        self.staged_messages.push(msg);
    }
    fn set_run_state(&mut self, run_id: &str, state: &str) {
        self.staged_run_states
            .push((run_id.to_string(), state.to_string()));
    }
    fn append_event(&mut self, event: NewSessionEvent) {
        self.staged_events.push(event);
    }
    fn create_approval(&mut self, approval: NewApproval) {
        self.staged_approvals.push(approval);
    }
    fn enqueue_outbox(&mut self, item: OutboxItem) {
        self.staged_outbox.push(item);
    }

    async fn commit(&mut self) -> Result<(), RepositoryError> {
        let mut inner = self.store.inner.lock().unwrap_or_else(|p| p.into_inner());

        // Messages: assign per-scope ordinals, preserving caller order.
        for m in self.staged_messages.drain(..) {
            let key = ScopeKey::from(&m.scope);
            let ordinal = {
                let o = inner.next_ordinal.entry(key.clone()).or_insert(1);
                let cur = *o;
                *o += 1;
                cur
            };
            inner.messages.entry(key).or_default().push(MessageRecord {
                message_id: m.message_id,
                thread_id: m.thread_id,
                turn_id: m.turn_id,
                role: m.role,
                content: m.content,
                ordinal,
            });
        }

        // Run states.
        for (run_id, state) in self.staged_run_states.drain(..) {
            inner.run_states.insert(run_id, state);
        }

        // Events: per-scope monotonic gap-free seq + event_id dedup (D6).
        for e in self.staged_events.drain(..) {
            let key = ScopeKey::from(&e.scope);
            // Dedup against already-committed event_ids for this scope.
            let already_present = inner
                .events
                .get(&key)
                .is_some_and(|evs| evs.iter().any(|ev| ev.event_id == e.event_id));
            if already_present {
                continue; // idempotent: duplicate event_id deduped
            }
            let seq = {
                let s = inner.next_seq.entry(key.clone()).or_insert(1);
                let cur = *s;
                *s += 1;
                cur
            };
            inner.events.entry(key).or_default().push(SessionEvent {
                event_id: e.event_id,
                causation_id: e.causation_id,
                seq,
                payload: e.payload,
            });
        }

        // Approvals (durable waiting).
        for a in self.staged_approvals.drain(..) {
            let key = ScopeKey::from(&a.scope);
            inner.approvals.entry(key).or_default().insert(
                a.approval_id.clone(),
                ApprovalRecord {
                    approval_id: a.approval_id,
                    originating_run: a.originating_run,
                    args_hash: a.args_hash,
                    binding_revision: a.binding_revision,
                    state: ApprovalState::Pending,
                    decision: None,
                },
            );
        }

        // Outbox, same commit.
        inner.outbox.append(&mut self.staged_outbox);

        Ok(())
    }

    fn shared_store(&self) -> Arc<LocalStore> {
        Arc::clone(&self.store)
    }
}

/// Backend namespaces. The local (single-node, no-PostgreSQL) adapter and
/// the PostgreSQL backend sit side-by-side behind the same traits; callers
/// name `repository::local::*` or `repository::postgres::*` so the chosen
/// backend is explicit at the construction site (ADR D8).
pub mod local {
    pub use super::{LocalStore, LocalUnitOfWork};
}

// The PostgreSQL backend (`repository::postgres`) implements the same
// traits behind the `postgres` feature (spec Decisions 3, 7).
#[cfg(feature = "postgres")]
pub mod postgres {
    pub use crate::repository_postgres::{PgStore, PgUnitOfWork};
}
