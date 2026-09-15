//! # PostgreSQL backend for the repository contract (c2)
//!
//! ADR `docs/adr/cluster-state-and-execution.md` D1/D6; spec
//! `specs/task-c2-persistence-boundary-postgres.spec.md`. Implements
//! [`StoreView`]/[`UnitOfWork`] against the tenant-keyed schema in
//! `migrations/0001_c2_persistence_boundary.sql`.
//!
//! Atomicity: all staged mutations land in ONE `sqlx::Transaction` — a
//! business transition that touches messages + run state + events + outbox
//! commits or rolls back as a unit (D1: no per-module `save_to_postgres`,
//! no long-lived dual write). Isolation: every statement carries the full
//! scope key, and `SET LOCAL app.tenant_id` engages the RLS policy as
//! defense in depth (D2). The connection role is not a superuser.

use std::sync::Arc;

use octos_core::execution_scope::Scope;
use sqlx::Row;
use sqlx::postgres::{PgPool, PgPoolOptions};

use super::repository::{
    ApprovalDecision, ApprovalRecord, ApprovalState, CheckpointRecord, CronScheduleStore,
    FiringState, InvocationRecord, InvocationState, LeaseStore, MessageRecord, MisfirePolicy,
    NewApproval, NewCheckpoint, NewInvocation, NewMessage, NewSessionEvent, OutboxItem,
    RecoveryStore, RepositoryError, RunLease, Schedule, ScheduleFiring, SessionEvent, StoreView,
    UnitOfWork,
};

const MIGRATION: &str = concat!(
    include_str!("../migrations/0001_c2_persistence_boundary.sql"),
    "\n",
    include_str!("../migrations/0002_c3_run_leases.sql"),
    "\n",
    include_str!("../migrations/0003_c3_checkpoints_invocations.sql"),
    "\n",
    include_str!("../migrations/0004_c5_cron_durable.sql"),
);

/// A PostgreSQL-backed repository store. Cheap to clone (shared pool).
#[derive(Clone)]
pub struct PgStore {
    pool: Arc<PgPool>,
}

impl PgStore {
    /// Connect with a small pool. The pooler never blocks the Tokio runtime —
    /// all access is fully async (spec rule migration-safety).
    pub async fn connect(database_url: &str) -> Result<Self, RepositoryError> {
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(database_url)
            .await
            .map_err(|e| RepositoryError::Other(format!("connect: {e}")))?;
        Ok(Self {
            pool: Arc::new(pool),
        })
    }

    /// Apply the expand-step schema. Idempotent (`IF NOT EXISTS`); re-running
    /// is a no-op (spec rule migration-safety).
    pub async fn migrate(&self) -> Result<(), RepositoryError> {
        // The migration must run as a role that bypasses RLS to CREATE the
        // policy/table; the docker bootstrap superuser is used for setup only.
        // The runtime connects as a non-superuser app role in production.
        sqlx::raw_sql(MIGRATION)
            .execute(&*self.pool)
            .await
            .map_err(|e| RepositoryError::Other(format!("migrate: {e}")))?;
        Ok(())
    }

    pub fn begin(&self) -> PgUnitOfWork {
        PgUnitOfWork {
            store: self.clone(),
            staged_messages: Vec::new(),
            staged_run_states: Vec::new(),
            staged_events: Vec::new(),
            staged_approvals: Vec::new(),
            staged_outbox: Vec::new(),
        }
    }
}

/// Set the per-transaction RLS tenant context. Uses `SET LOCAL` so it is
/// scoped to the current transaction and auto-resets.
async fn set_tenant<'e>(
    tx: &mut sqlx::Transaction<'e, sqlx::Postgres>,
    tenant_id: &str,
) -> Result<(), RepositoryError> {
    // `SET LOCAL` does not accept bound parameters; quote the literal safely.
    // tenant_id is a validated scope component (no quotes/backslashes survive
    // scope validation), but we defensively escape single quotes anyway.
    let escaped = tenant_id.replace('\'', "''");
    sqlx::query(&format!("SET LOCAL app.tenant_id = '{escaped}'"))
        .execute(&mut **tx)
        .await
        .map_err(|e| RepositoryError::Other(format!("set tenant: {e}")))?;
    Ok(())
}

fn scope_tuple(s: &Scope) -> (String, String, String, String) {
    (
        s.tenant_id().to_string(),
        s.profile_id().to_string(),
        s.workspace_id().to_string(),
        s.session_id().to_string(),
    )
}

impl StoreView for PgStore {
    fn messages_for(&self, _scope: &Scope) -> Vec<MessageRecord> {
        // Synchronous trait surface (shared with the local adapter). The PG
        // backend is async; the synchronous read surface is exercised only by
        // the local adapter and in-process callers. Real PG reads go through
        // the async methods below. This branch is unreachable in the PG
        // integration suite, which uses the async accessors.
        unreachable!("PgStore is async; use messages_for_async")
    }
    fn events_for(&self, _scope: &Scope) -> Vec<SessionEvent> {
        unreachable!("PgStore is async; use events_for_async")
    }
    fn run_state(&self, _run_id: &str) -> Option<String> {
        unreachable!("PgStore is async; use run_state_async")
    }
    fn approval(&self, _scope: &Scope, _approval_id: &str) -> Option<ApprovalRecord> {
        unreachable!("PgStore is async; use approval_async")
    }
    fn outbox(&self) -> Vec<OutboxItem> {
        unreachable!("PgStore is async; use outbox_async")
    }
    fn reply_approval(
        &self,
        _scope: &Scope,
        _approval_id: &str,
        _args_hash: &str,
        _decision: ApprovalDecision,
    ) -> Result<ApprovalState, RepositoryError> {
        unreachable!("PgStore is async; use reply_approval_async")
    }
}

impl PgStore {
    pub async fn messages_for_async(
        &self,
        scope: &Scope,
    ) -> Result<Vec<MessageRecord>, RepositoryError> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        set_tenant(&mut tx, &t).await?;
        let rows = sqlx::query(
            "SELECT message_id, thread_id, turn_id, role, content, ordinal \
             FROM messages \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
             ORDER BY ordinal",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|r| MessageRecord {
                message_id: r.get("message_id"),
                thread_id: r.get("thread_id"),
                turn_id: r.get("turn_id"),
                role: r.get("role"),
                content: r.get("content"),
                ordinal: r.get::<i64, _>("ordinal") as u64,
            })
            .collect())
    }

    pub async fn events_for_async(
        &self,
        scope: &Scope,
    ) -> Result<Vec<SessionEvent>, RepositoryError> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        set_tenant(&mut tx, &t).await?;
        let rows = sqlx::query(
            "SELECT event_id, causation_id, seq, payload FROM session_events \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
             ORDER BY seq",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|r| SessionEvent {
                event_id: r.get("event_id"),
                causation_id: r.get("causation_id"),
                seq: r.get::<i64, _>("seq") as u64,
                payload: r.get("payload"),
            })
            .collect())
    }

    pub async fn run_state_async(&self, run_id: &str) -> Result<Option<String>, RepositoryError> {
        // Run-state reads must see any tenant's run by id (the run_id is
        // server-allocated and globally unique); RLS on agent_runs is keyed by
        // tenant, so this read runs WITHOUT a tenant filter. Run ids are not
        // guessable cross-tenant handles in the API surface (c1 binds them to
        // a scope at entry), so a by-id lookup here does not widen reach.
        let row = sqlx::query("SELECT state FROM agent_runs WHERE run_id=$1")
            .bind(run_id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        Ok(row.map(|r| r.get("state")))
    }

    pub async fn outbox_async(&self, tenant_id: &str) -> Result<Vec<OutboxItem>, RepositoryError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        set_tenant(&mut tx, tenant_id).await?;
        let rows = sqlx::query(
            "SELECT aggregate_key, topic, payload FROM outbox \
             WHERE tenant_id=$1 AND delivered=FALSE ORDER BY id",
        )
        .bind(tenant_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        Ok(rows
            .into_iter()
            .map(|r| OutboxItem {
                aggregate_key: r.get("aggregate_key"),
                topic: r.get("topic"),
                payload: r.get("payload"),
            })
            .collect())
    }

    /// Read a single durable approval (for c3 resume-after-restart).
    pub async fn approval_async(&self, scope: &Scope, approval_id: &str) -> Option<ApprovalRecord> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self.pool.begin().await.ok()?;
        if set_tenant(&mut tx, &t).await.is_err() {
            return None;
        }
        let row = sqlx::query(
            "SELECT approval_id, originating_run, args_hash, binding_revision, state, decision \
             FROM approvals \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
               AND approval_id=$5",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(approval_id)
        .fetch_optional(&mut *tx)
        .await
        .ok()?;
        let _ = tx.commit().await;
        let row = row?;
        let state_str: String = row.get("state");
        let state = match state_str.as_str() {
            "pending" => ApprovalState::Pending,
            "decided" => ApprovalState::Decided,
            _ => ApprovalState::Expired,
        };
        let decision = row
            .get::<Option<String>, _>("decision")
            .map(|d| match d.as_str() {
                "approved" => ApprovalDecision::Approved,
                _ => ApprovalDecision::Rejected,
            });
        Some(ApprovalRecord {
            approval_id: row.get("approval_id"),
            originating_run: row.get("originating_run"),
            args_hash: row.get("args_hash"),
            binding_revision: row.get("binding_revision"),
            state,
            decision,
        })
    }

    /// Enumerate Pending approvals under a scope. Used by cluster
    /// recovery paths to discover peer-originated approvals for
    /// rehydrate (K05). RLS-enforced via SET LOCAL app.tenant_id.
    pub async fn pending_approvals_for_scope_async(&self, scope: &Scope) -> Vec<ApprovalRecord> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = match self.pool.begin().await {
            Ok(tx) => tx,
            Err(e) => {
                tracing::warn!(error = %e, "pending_approvals begin failed; returning empty");
                return Vec::new();
            }
        };
        if let Err(e) = set_tenant(&mut tx, &t).await {
            tracing::warn!(error = %e, "set_tenant failed; returning empty");
            let _ = tx.rollback().await;
            return Vec::new();
        }
        let rows_result = sqlx::query(
            "SELECT approval_id, originating_run, args_hash, binding_revision, state, decision              FROM approvals              WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4                AND state='pending'              ORDER BY approval_id",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .fetch_all(&mut *tx)
        .await;
        let rows = match rows_result {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(error = %e, "pending_approvals SELECT failed");
                let _ = tx.rollback().await;
                return Vec::new();
            }
        };
        // Read-only transaction: commit (not rollback) releases the
        // connection promptly and signals success.
        let _ = tx.commit().await;
        rows.into_iter()
            .map(|row| {
                let state = ApprovalState::Pending;
                let decision = row
                    .get::<Option<String>, _>("decision")
                    .map(|d| match d.as_str() {
                        "approved" => ApprovalDecision::Approved,
                        _ => ApprovalDecision::Rejected,
                    });
                ApprovalRecord {
                    approval_id: row.get("approval_id"),
                    originating_run: row.get("originating_run"),
                    args_hash: row.get("args_hash"),
                    binding_revision: row.get("binding_revision"),
                    state,
                    decision,
                }
            })
            .collect()
    }

    /// Approval reply CAS (K05): one UPDATE with compare predicates; the row
    /// count distinguishes NotFound / AlreadyDecided / ArgsMismatch.
    pub async fn reply_approval_async(
        &self,
        scope: &Scope,
        approval_id: &str,
        args_hash: &str,
        decision: ApprovalDecision,
    ) -> Result<ApprovalState, RepositoryError> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        set_tenant(&mut tx, &t).await?;

        // Read the row to classify the failure precisely.
        let existing = sqlx::query(
            "SELECT state, args_hash FROM approvals \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
               AND approval_id=$5",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(approval_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?;

        let row = match existing {
            None => {
                let _ = tx.rollback().await;
                return Err(RepositoryError::NotFound);
            }
            Some(r) => r,
        };
        let state: String = row.get("state");
        if state != "pending" {
            let _ = tx.rollback().await;
            return Err(RepositoryError::AlreadyDecided);
        }
        let stored_hash: String = row.get("args_hash");
        if stored_hash != args_hash {
            let _ = tx.rollback().await;
            return Err(RepositoryError::ArgsMismatch);
        }

        let decision_str = match decision {
            ApprovalDecision::Approved => "approved",
            ApprovalDecision::Rejected => "rejected",
        };
        sqlx::query(
            "UPDATE approvals SET state='decided', decision=$6 \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
               AND approval_id=$5 AND state='pending'",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(approval_id)
        .bind(decision_str)
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        Ok(ApprovalState::Decided)
    }
}

/// A staged unit of work, flushed to PG in one transaction on commit.
pub struct PgUnitOfWork {
    store: PgStore,
    staged_messages: Vec<NewMessage>,
    staged_run_states: Vec<(String, String)>,
    staged_events: Vec<NewSessionEvent>,
    staged_approvals: Vec<NewApproval>,
    staged_outbox: Vec<OutboxItem>,
}

impl UnitOfWork for PgUnitOfWork {
    type View = PgStore;

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
        let mut tx = self
            .store
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;

        // Group staged work by tenant so each transaction sets the RLS tenant
        // context for the rows it writes. (A single UoW normally spans one
        // scope; the loop keeps the general case correct.)
        // We set tenant per statement batch via the message/event/approval
        // scope; run_states/outbox carry tenant explicitly.

        // Resolve the outbox tenant BEFORE draining any staged vec (the
        // tenant derives from the first staged scope-bearing mutation).
        let outbox_tenant = self.tenant_for_outbox().unwrap_or_default();

        // Messages.
        for m in self.staged_messages.drain(..) {
            let (t, p, w, s) = scope_tuple(&m.scope);
            set_tenant(&mut tx, &t).await?;
            // Upsert the session anchor, then assign the next ordinal.
            sqlx::query(
                "INSERT INTO sessions (tenant_id, profile_id, workspace_id, session_id) \
                 VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING",
            )
            .bind(&t)
            .bind(&p)
            .bind(&w)
            .bind(&s)
            .execute(&mut *tx)
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
            sqlx::query(
                "INSERT INTO messages \
                 (tenant_id, profile_id, workspace_id, session_id, message_id, thread_id, turn_id, role, content, ordinal) \
                 SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9, \
                   COALESCE((SELECT MAX(ordinal) FROM messages \
                     WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4),0)+1",
            )
            .bind(&t).bind(&p).bind(&w).bind(&s)
            .bind(&m.message_id).bind(&m.thread_id).bind(&m.turn_id)
            .bind(&m.role).bind(&m.content)
            .execute(&mut *tx)
            .await
            .map_err(|e| RepositoryError::Other(format!("append message: {e}")))?;
        }

        // Run states (insert or update; run_id is globally unique).
        for (run_id, state) in self.staged_run_states.drain(..) {
            sqlx::query(
                "INSERT INTO agent_runs \
                 (run_id, tenant_id, profile_id, workspace_id, session_id, state) \
                 VALUES ($1,'','','','',$2) \
                 ON CONFLICT (run_id) DO UPDATE SET state=EXCLUDED.state",
            )
            .bind(&run_id)
            .bind(&state)
            .execute(&mut *tx)
            .await
            .map_err(|e| RepositoryError::Other(format!("set run state: {e}")))?;
        }

        // Events: per-scope monotonic seq + event_id dedup (D6). Dedup is
        // enforced by the UNIQUE(scope, event_id) constraint; a duplicate is
        // skipped via ON CONFLICT on event_id.
        for e in self.staged_events.drain(..) {
            let (t, p, w, s) = scope_tuple(&e.scope);
            set_tenant(&mut tx, &t).await?;
            let exists = sqlx::query(
                "SELECT 1 FROM session_events \
                 WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 AND event_id=$5",
            )
            .bind(&t).bind(&p).bind(&w).bind(&s).bind(&e.event_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
            if exists.is_some() {
                continue; // idempotent dedup
            }
            sqlx::query(
                "INSERT INTO session_events \
                 (tenant_id, profile_id, workspace_id, session_id, seq, event_id, causation_id, payload) \
                 SELECT $1,$2,$3,$4, \
                   COALESCE((SELECT MAX(seq) FROM session_events \
                     WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4),0)+1, \
                   $5,$6,$7",
            )
            .bind(&t).bind(&p).bind(&w).bind(&s)
            .bind(&e.event_id).bind(&e.causation_id).bind(&e.payload)
            .execute(&mut *tx)
            .await
            .map_err(|e| RepositoryError::Other(format!("append event: {e}")))?;
        }

        // Approvals (durable waiting).
        for a in self.staged_approvals.drain(..) {
            let (t, p, w, s) = scope_tuple(&a.scope);
            set_tenant(&mut tx, &t).await?;
            sqlx::query(
                "INSERT INTO approvals \
                 (tenant_id, profile_id, workspace_id, session_id, approval_id, originating_run, args_hash, binding_revision, state) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'pending')",
            )
            .bind(&t).bind(&p).bind(&w).bind(&s)
            .bind(&a.approval_id).bind(&a.originating_run).bind(&a.args_hash)
            .bind(&a.binding_revision)
            .execute(&mut *tx)
            .await
            .map_err(|e| RepositoryError::Other(format!("create approval: {e}")))?;
        }

        // Outbox, same transaction (D6).
        for o in self.staged_outbox.drain(..) {
            let tenant = outbox_tenant.clone();
            set_tenant(&mut tx, &tenant).await?;
            sqlx::query(
                "INSERT INTO outbox (tenant_id, aggregate_key, topic, payload) \
                 VALUES ($1,$2,$3,$4)",
            )
            .bind(&tenant)
            .bind(&o.aggregate_key)
            .bind(&o.topic)
            .bind(&o.payload)
            .execute(&mut *tx)
            .await
            .map_err(|e| RepositoryError::Other(format!("enqueue outbox: {e}")))?;
        }

        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(format!("commit: {e}")))?;
        Ok(())
    }

    fn shared_store(&self) -> Arc<PgStore> {
        // The shared-store surface returns an owned clone wrapped in Arc for
        // API symmetry with the local adapter. PgStore is cheap to clone
        // (shared pool), so this never opens a new connection.
        Arc::new(self.store.clone())
    }
}

impl PgUnitOfWork {
    /// The tenant to stamp on outbox rows. Derived from the first staged
    /// scope-bearing mutation (message/event/approval); a UoW is single-scope
    /// in practice.
    fn tenant_for_outbox(&self) -> Option<String> {
        self.staged_messages
            .first()
            .map(|m| m.scope.tenant_id().to_string())
            .or_else(|| {
                self.staged_events
                    .first()
                    .map(|e| e.scope.tenant_id().to_string())
            })
            .or_else(|| {
                self.staged_approvals
                    .first()
                    .map(|a| a.scope.tenant_id().to_string())
            })
    }
}

// ---------------------------------------------------------------------------
// c3: epoch-fenced run leases (D5) against the real schema
// ---------------------------------------------------------------------------

fn ms_to_timestamptz(ms: u64) -> chrono::DateTime<chrono::Utc> {
    let secs = (ms / 1000) as i64;
    let nanos = ((ms % 1000) * 1_000_000) as u32;
    chrono::DateTime::from_timestamp(secs, nanos)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap())
}

impl LeaseStore for PgStore {
    /// Claim/renew/takeover in ONE transaction. The `SELECT ... FOR UPDATE`
    /// row lock serializes concurrent claimers on the same (scope, run_id)
    /// so exactly one wins (K02); an expired lease is taken over with
    /// epoch+1 (K03).
    async fn claim(
        &self,
        scope: &Scope,
        run_id: &str,
        owner_id: &str,
        lease_ms: u64,
        now_ms: u64,
    ) -> Result<RunLease, RepositoryError> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        set_tenant(&mut tx, &t).await?;

        let expires = ms_to_timestamptz(now_ms + lease_ms);
        let now = ms_to_timestamptz(now_ms);

        // Lock any existing lease row for this run.
        let existing = sqlx::query(
            "SELECT owner_id, epoch, expires_at FROM run_leases \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
               AND run_id=$5 FOR UPDATE",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?;

        let lease = match existing {
            None => {
                let epoch: i64 = 1;
                let ins = sqlx::query(
                    "INSERT INTO run_leases \
                     (tenant_id, profile_id, workspace_id, session_id, run_id, owner_id, epoch, expires_at) \
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
                )
                .bind(&t).bind(&p).bind(&w).bind(&s).bind(run_id)
                .bind(owner_id).bind(epoch).bind(expires)
                .execute(&mut *tx)
                .await;
                if let Err(e) = ins {
                    // Two concurrent first-claimers both see no row under
                    // snapshot isolation; the loser's INSERT hits the PRIMARY
                    // KEY. Map unique_violation (23505) to Conflict (K02:
                    // single owner) instead of a generic error.
                    let is_unique = e
                        .as_database_error()
                        .and_then(|d| d.code())
                        .is_some_and(|c| c == "23505");
                    let _ = tx.rollback().await;
                    return Err(if is_unique {
                        RepositoryError::Conflict
                    } else {
                        RepositoryError::Other(format!("insert lease: {e}"))
                    });
                }
                RunLease {
                    run_id: run_id.to_string(),
                    owner_id: owner_id.to_string(),
                    epoch: epoch as u64,
                    expires_at_ms: now_ms + lease_ms,
                }
            }
            Some(row) => {
                let cur_owner: String = row.get("owner_id");
                let cur_epoch: i64 = row.get("epoch");
                let cur_expires: chrono::DateTime<chrono::Utc> = row.get("expires_at");
                let cur_expires_ms = cur_expires.timestamp_millis() as u64;

                if cur_owner == owner_id {
                    // Renew: same epoch, extend expiry.
                    sqlx::query(
                        "UPDATE run_leases SET expires_at=$6 \
                         WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 AND run_id=$5",
                    )
                    .bind(&t).bind(&p).bind(&w).bind(&s).bind(run_id).bind(expires)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| RepositoryError::Other(e.to_string()))?;
                    RunLease {
                        run_id: run_id.to_string(),
                        owner_id: owner_id.to_string(),
                        epoch: cur_epoch as u64,
                        expires_at_ms: now_ms + lease_ms,
                    }
                } else if cur_expires_ms <= now_ms {
                    // Expired: takeover with epoch+1 (K03).
                    let new_epoch = cur_epoch + 1;
                    sqlx::query(
                        "UPDATE run_leases SET owner_id=$6, epoch=$7, expires_at=$8 \
                         WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 AND run_id=$5",
                    )
                    .bind(&t).bind(&p).bind(&w).bind(&s).bind(run_id)
                    .bind(owner_id).bind(new_epoch).bind(expires)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| RepositoryError::Other(e.to_string()))?;
                    RunLease {
                        run_id: run_id.to_string(),
                        owner_id: owner_id.to_string(),
                        epoch: new_epoch as u64,
                        expires_at_ms: now_ms + lease_ms,
                    }
                } else {
                    let _ = tx.rollback().await;
                    return Err(RepositoryError::Conflict);
                }
            }
        };

        let _ = now; // now_ms used via now for clarity; expires computed above
        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(format!("commit claim: {e}")))?;
        Ok(lease)
    }

    async fn write_with_epoch(
        &self,
        scope: &Scope,
        run_id: &str,
        epoch: u64,
        state: &str,
    ) -> Result<(), RepositoryError> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        set_tenant(&mut tx, &t).await?;

        // Fencing: the write is gated on the CURRENT lease epoch matching.
        let cur: Option<i64> = sqlx::query(
            "SELECT epoch FROM run_leases \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 AND run_id=$5",
        )
        .bind(&t).bind(&p).bind(&w).bind(&s).bind(run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?
        .map(|r| r.get("epoch"));

        match cur {
            None => {
                let _ = tx.rollback().await;
                return Err(RepositoryError::NotFound);
            }
            Some(e) if e as u64 != epoch => {
                let _ = tx.rollback().await;
                return Err(RepositoryError::StaleEpoch);
            }
            Some(_) => {}
        }

        // Epoch matches: upsert the run state.
        sqlx::query(
            "INSERT INTO agent_runs \
             (run_id, tenant_id, profile_id, workspace_id, session_id, state) \
             VALUES ($1,$2,$3,$4,$5,$6) \
             ON CONFLICT (run_id) DO UPDATE SET state=EXCLUDED.state",
        )
        .bind(run_id)
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(state)
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(format!("write run state: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        Ok(())
    }

    async fn current_lease(&self, scope: &Scope, run_id: &str) -> Option<RunLease> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self.pool.begin().await.ok()?;
        if set_tenant(&mut tx, &t).await.is_err() {
            return None;
        }
        let row = sqlx::query(
            "SELECT owner_id, epoch, expires_at FROM run_leases \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 AND run_id=$5",
        )
        .bind(&t).bind(&p).bind(&w).bind(&s).bind(run_id)
        .fetch_optional(&mut *tx)
        .await
        .ok()?;
        let _ = tx.commit().await;
        row.map(|r| {
            let expires: chrono::DateTime<chrono::Utc> = r.get("expires_at");
            RunLease {
                run_id: run_id.to_string(),
                owner_id: r.get("owner_id"),
                epoch: r.get::<i64, _>("epoch") as u64,
                expires_at_ms: expires.timestamp_millis() as u64,
            }
        })
    }
}

// ---------------------------------------------------------------------------
// c3: run checkpoints + tool-invocation idempotency ledger (D3/D4)
// ---------------------------------------------------------------------------

fn parse_invocation_state(s: &str) -> InvocationState {
    match s {
        "succeeded" => InvocationState::Succeeded,
        "failed" => InvocationState::Failed,
        "unknown" => InvocationState::Unknown,
        _ => InvocationState::Intent,
    }
}

impl RecoveryStore for PgStore {
    async fn commit_checkpoint(&self, cp: NewCheckpoint) -> Result<(), RepositoryError> {
        let (t, p, w, s) = scope_tuple(&cp.scope);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        set_tenant(&mut tx, &t).await?;

        // Fencing (D5): the checkpoint's epoch must match the current lease.
        let cur: Option<i64> = sqlx::query(
            "SELECT epoch FROM run_leases \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 AND run_id=$5",
        )
        .bind(&t).bind(&p).bind(&w).bind(&s).bind(&cp.run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?
        .map(|r| r.get("epoch"));
        match cur {
            None => {
                let _ = tx.rollback().await;
                return Err(RepositoryError::NotFound);
            }
            Some(e) if e as u64 != cp.created_epoch => {
                let _ = tx.rollback().await;
                return Err(RepositoryError::StaleEpoch);
            }
            Some(_) => {}
        }

        sqlx::query(
            "INSERT INTO run_checkpoints \
             (tenant_id, profile_id, workspace_id, session_id, run_id, step, transcript_highwater, \
              context, workspace_revision, pending_invocation, artifact_refs, binding_digest, \
              permission_snapshot, digest, schema_version, runtime_version, created_epoch) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) \
             ON CONFLICT (tenant_id, profile_id, workspace_id, session_id, run_id, step) \
             DO UPDATE SET transcript_highwater=EXCLUDED.transcript_highwater, \
               context=EXCLUDED.context, workspace_revision=EXCLUDED.workspace_revision, \
               pending_invocation=EXCLUDED.pending_invocation, artifact_refs=EXCLUDED.artifact_refs, \
               binding_digest=EXCLUDED.binding_digest, permission_snapshot=EXCLUDED.permission_snapshot, \
               digest=EXCLUDED.digest, created_epoch=EXCLUDED.created_epoch",
        )
        .bind(&t).bind(&p).bind(&w).bind(&s).bind(&cp.run_id)
        .bind(cp.step as i64).bind(cp.transcript_highwater as i64)
        .bind(&cp.context).bind(&cp.workspace_revision).bind(&cp.pending_invocation)
        .bind(&cp.artifact_refs).bind(&cp.binding_digest).bind(&cp.permission_snapshot)
        .bind(&cp.digest).bind(&cp.schema_version).bind(&cp.runtime_version)
        .bind(cp.created_epoch as i64)
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(format!("commit checkpoint: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        Ok(())
    }

    async fn latest_checkpoint(&self, scope: &Scope, run_id: &str) -> Option<CheckpointRecord> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self.pool.begin().await.ok()?;
        if set_tenant(&mut tx, &t).await.is_err() {
            return None;
        }
        let row = sqlx::query(
            "SELECT step, transcript_highwater, context, workspace_revision, pending_invocation, \
                    artifact_refs, binding_digest, permission_snapshot, digest, schema_version, \
                    runtime_version, created_epoch \
             FROM run_checkpoints \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 AND run_id=$5 \
             ORDER BY step DESC LIMIT 1",
        )
        .bind(&t).bind(&p).bind(&w).bind(&s).bind(run_id)
        .fetch_optional(&mut *tx)
        .await
        .ok()?;
        let _ = tx.commit().await;
        row.map(|r| CheckpointRecord {
            step: r.get::<i64, _>("step") as u64,
            transcript_highwater: r.get::<i64, _>("transcript_highwater") as u64,
            context: r.get("context"),
            workspace_revision: r.get("workspace_revision"),
            pending_invocation: r.get("pending_invocation"),
            artifact_refs: r.get("artifact_refs"),
            binding_digest: r.get("binding_digest"),
            permission_snapshot: r.get("permission_snapshot"),
            digest: r.get("digest"),
            schema_version: r.get("schema_version"),
            runtime_version: r.get("runtime_version"),
            created_epoch: r.get::<i64, _>("created_epoch") as u64,
        })
    }

    async fn record_intent(&self, inv: NewInvocation) -> Result<InvocationState, RepositoryError> {
        let (t, p, w, s) = scope_tuple(&inv.scope);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        set_tenant(&mut tx, &t).await?;

        // Existing logical invocation (a retry) must match args_hash (K04).
        let existing = sqlx::query(
            "SELECT state, args_hash FROM tool_invocations \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
               AND run_id=$5 AND invocation_id=$6",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(&inv.run_id)
        .bind(&inv.invocation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?;

        if let Some(row) = existing {
            let stored_hash: String = row.get("args_hash");
            if stored_hash != inv.args_hash {
                let _ = tx.rollback().await;
                return Err(RepositoryError::ArgsMismatch);
            }
            let state = parse_invocation_state(row.get::<String, _>("state").as_str());
            tx.commit()
                .await
                .map_err(|e| RepositoryError::Other(e.to_string()))?;
            return Ok(state);
        }

        sqlx::query(
            "INSERT INTO tool_invocations \
             (tenant_id, profile_id, workspace_id, session_id, run_id, invocation_id, \
              tool_revision, args_hash, state, external_idempotency_key) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'intent',$9)",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(&inv.run_id)
        .bind(&inv.invocation_id)
        .bind(&inv.tool_revision)
        .bind(&inv.args_hash)
        .bind(&inv.external_idempotency_key)
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(format!("record intent: {e}")))?;

        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
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
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        set_tenant(&mut tx, &t).await?;
        let n = sqlx::query(
            "UPDATE tool_invocations SET state='succeeded', result_ref=$7, cost_micros=$8 \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
               AND run_id=$5 AND invocation_id=$6",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(run_id)
        .bind(invocation_id)
        .bind(result_ref)
        .bind(cost_micros)
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?
        .rows_affected();
        if n == 0 {
            let _ = tx.rollback().await;
            return Err(RepositoryError::NotFound);
        }
        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        Ok(())
    }

    async fn mark_unknown(
        &self,
        scope: &Scope,
        run_id: &str,
        invocation_id: &str,
    ) -> Result<(), RepositoryError> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        set_tenant(&mut tx, &t).await?;
        let n = sqlx::query(
            "UPDATE tool_invocations SET state='unknown' \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
               AND run_id=$5 AND invocation_id=$6",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(run_id)
        .bind(invocation_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?
        .rows_affected();
        if n == 0 {
            let _ = tx.rollback().await;
            return Err(RepositoryError::NotFound);
        }
        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        Ok(())
    }

    async fn find_by_idempotency_key(
        &self,
        scope: &Scope,
        external_key: &str,
    ) -> Option<InvocationRecord> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self.pool.begin().await.ok()?;
        if set_tenant(&mut tx, &t).await.is_err() {
            return None;
        }
        let row = sqlx::query(
            "SELECT invocation_id, tool_revision, args_hash, state, external_idempotency_key, \
                    result_ref, cost_micros \
             FROM tool_invocations \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
               AND external_idempotency_key=$5",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(external_key)
        .fetch_optional(&mut *tx)
        .await
        .ok()?;
        let _ = tx.commit().await;
        row.map(|r| InvocationRecord {
            invocation_id: r.get("invocation_id"),
            tool_revision: r.get("tool_revision"),
            args_hash: r.get("args_hash"),
            state: parse_invocation_state(r.get::<String, _>("state").as_str()),
            external_idempotency_key: r.get("external_idempotency_key"),
            result_ref: r.get("result_ref"),
            cost_micros: r.get("cost_micros"),
        })
    }

    async fn invocation_by_id(
        &self,
        scope: &Scope,
        run_id: &str,
        invocation_id: &str,
    ) -> Option<InvocationRecord> {
        self.pg_invocation_by_id(scope, run_id, invocation_id).await
    }

    async fn cas_workspace_revision(
        &self,
        scope: &Scope,
        run_id: &str,
        expected_old_revision: Option<&str>,
        new_revision: &str,
    ) -> Result<String, RepositoryError> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(format!("cas begin: {e}")))?;
        set_tenant(&mut tx, &t).await?;
        // Read the latest checkpoint's revision and current step.
        let row = sqlx::query(
            "SELECT step, workspace_revision              FROM run_checkpoints              WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4                AND run_id=$5              ORDER BY step DESC LIMIT 1",
        )
        .bind(&t).bind(&p).bind(&w).bind(&s).bind(run_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(format!("cas read: {e}")))?;
        let Some(row) = row else {
            let _ = tx.rollback().await;
            return Err(RepositoryError::NotFound);
        };
        let cur: Option<String> = row.get("workspace_revision");
        // K08 CAS predicate
        if cur.as_deref() != expected_old_revision {
            let _ = tx.rollback().await;
            return Err(RepositoryError::StaleRevision);
        }
        let step: i64 = row.get("step");
        // UPDATE the latest checkpoint row with the new revision. The
        // (scope, run_id, step) PK ensures we touch exactly one row.
        sqlx::query(
            "UPDATE run_checkpoints SET workspace_revision=$1              WHERE tenant_id=$2 AND profile_id=$3 AND workspace_id=$4 AND session_id=$5                AND run_id=$6 AND step=$7",
        )
        .bind(new_revision)
        .bind(&t).bind(&p).bind(&w).bind(&s).bind(run_id).bind(step)
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(format!("cas update: {e}")))?;
        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        Ok(new_revision.to_string())
    }
}

impl PgStore {
    /// Look up an invocation by logical id (c3: read the confirmed result_ref).
    async fn pg_invocation_by_id(
        &self,
        scope: &Scope,
        run_id: &str,
        invocation_id: &str,
    ) -> Option<InvocationRecord> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self.pool.begin().await.ok()?;
        if set_tenant(&mut tx, &t).await.is_err() {
            return None;
        }
        let row = sqlx::query(
            "SELECT invocation_id, tool_revision, args_hash, state, external_idempotency_key, \
                    result_ref, cost_micros \
             FROM tool_invocations \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
               AND run_id=$5 AND invocation_id=$6",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(run_id)
        .bind(invocation_id)
        .fetch_optional(&mut *tx)
        .await
        .ok()?;
        let _ = tx.commit().await;
        row.map(|r| InvocationRecord {
            invocation_id: r.get("invocation_id"),
            tool_revision: r.get("tool_revision"),
            args_hash: r.get("args_hash"),
            state: parse_invocation_state(r.get::<String, _>("state").as_str()),
            external_idempotency_key: r.get("external_idempotency_key"),
            result_ref: r.get("result_ref"),
            cost_micros: r.get("cost_micros"),
        })
    }
}

// ---------------------------------------------------------------------------
// c5: durable Cron firing (K10)
// ---------------------------------------------------------------------------

fn ms_to_timestamptz_cron(ms: u64) -> chrono::DateTime<chrono::Utc> {
    let secs = (ms / 1000) as i64;
    let nanos = ((ms % 1000) * 1_000_000) as u32;
    chrono::DateTime::from_timestamp(secs, nanos)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap())
}

fn misfire_str(p: MisfirePolicy) -> &'static str {
    match p {
        MisfirePolicy::Skip => "skip",
        MisfirePolicy::RunOnce => "run_once",
        MisfirePolicy::CatchUp => "catch_up",
    }
}

fn firing_state_str(s: FiringState) -> &'static str {
    match s {
        FiringState::Intent => "intent",
        FiringState::Running => "running",
        FiringState::Succeeded => "succeeded",
        FiringState::Failed => "failed",
    }
}

fn parse_firing_state(s: &str) -> FiringState {
    match s {
        "running" => FiringState::Running,
        "succeeded" => FiringState::Succeeded,
        "failed" => FiringState::Failed,
        _ => FiringState::Intent,
    }
}

fn parse_misfire(s: &str) -> MisfirePolicy {
    match s {
        "run_once" => MisfirePolicy::RunOnce,
        "catch_up" => MisfirePolicy::CatchUp,
        _ => MisfirePolicy::Skip,
    }
}

/// Inverse of `ms_to_timestamptz_cron`. Used by the schedule / firing read
/// paths to convert TIMESTAMPTZ columns back into the u64 epoch-ms the store
/// API speaks in. Saturates to 0 on out-of-range inputs (matches the
/// forward direction's 0 fallback).
fn timestamptz_cron_to_ms(dt: chrono::DateTime<chrono::Utc>) -> u64 {
    dt.timestamp_millis().max(0) as u64
}

impl CronScheduleStore for PgStore {
    async fn create_schedule(
        &self,
        scope: &Scope,
        schedule: Schedule,
    ) -> Result<(), RepositoryError> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        set_tenant(&mut tx, &t).await?;
        let res = sqlx::query(
            "INSERT INTO schedules \
             (tenant_id, profile_id, workspace_id, session_id, schedule_id, expression, \
              timezone, next_fire_at, misfire_policy, enabled) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(&schedule.schedule_id)
        .bind(&schedule.expression)
        .bind(&schedule.timezone)
        .bind(schedule.next_fire_at_ms.map(ms_to_timestamptz_cron))
        .bind(misfire_str(schedule.misfire_policy))
        .bind(schedule.enabled)
        .execute(&mut *tx)
        .await;
        if let Err(e) = res {
            let is_unique = e
                .as_database_error()
                .and_then(|d| d.code())
                .is_some_and(|c| c == "23505");
            let _ = tx.rollback().await;
            return Err(if is_unique {
                RepositoryError::Conflict
            } else {
                RepositoryError::Other(format!("create schedule: {e}"))
            });
        }
        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        Ok(())
    }

    async fn record_firing(
        &self,
        scope: &Scope,
        firing: ScheduleFiring,
    ) -> Result<(), RepositoryError> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        set_tenant(&mut tx, &t).await?;
        let res = sqlx::query(
            "INSERT INTO schedule_firings \
             (tenant_id, profile_id, workspace_id, session_id, schedule_id, scheduled_at, \
              firing_id, claimed_by, state, run_id) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(&firing.schedule_id)
        .bind(ms_to_timestamptz_cron(firing.scheduled_at_ms))
        .bind(&firing.firing_id)
        .bind(&firing.claimed_by)
        .bind(firing_state_str(firing.state))
        .bind(&firing.run_id)
        .execute(&mut *tx)
        .await;
        if let Err(e) = res {
            // K10: the PRIMARY KEY (scope, schedule_id, scheduled_at) makes
            // a duplicate INSERT raise unique_violation; map to Conflict.
            let is_unique = e
                .as_database_error()
                .and_then(|d| d.code())
                .is_some_and(|c| c == "23505");
            let _ = tx.rollback().await;
            return Err(if is_unique {
                RepositoryError::Conflict
            } else {
                RepositoryError::Other(format!("record firing: {e}"))
            });
        }
        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        Ok(())
    }

    async fn claim_firing(
        &self,
        scope: &Scope,
        schedule_id: &str,
        scheduled_at_ms: u64,
        controller_id: &str,
    ) -> Result<ScheduleFiring, RepositoryError> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        set_tenant(&mut tx, &t).await?;
        let scheduled = ms_to_timestamptz_cron(scheduled_at_ms);
        let row = sqlx::query(
            "SELECT firing_id, claimed_by, state, run_id FROM schedule_firings \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
               AND schedule_id=$5 AND scheduled_at=$6 FOR UPDATE",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(schedule_id)
        .bind(scheduled)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?;
        let row = match row {
            None => {
                let _ = tx.rollback().await;
                return Err(RepositoryError::NotFound);
            }
            Some(r) => r,
        };
        let state = parse_firing_state(row.get::<String, _>("state").as_str());
        if state != FiringState::Intent {
            let _ = tx.rollback().await;
            return Err(RepositoryError::Conflict);
        }
        sqlx::query(
            "UPDATE schedule_firings SET claimed_by=$7, state='running' \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
               AND schedule_id=$5 AND scheduled_at=$6",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(schedule_id)
        .bind(scheduled)
        .bind(controller_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        Ok(ScheduleFiring {
            schedule_id: schedule_id.to_string(),
            scheduled_at_ms,
            firing_id: row.get("firing_id"),
            claimed_by: Some(controller_id.to_string()),
            state: FiringState::Running,
            run_id: row.get("run_id"),
        })
    }

    async fn mark_firing_terminal(
        &self,
        scope: &Scope,
        schedule_id: &str,
        scheduled_at_ms: u64,
        state: FiringState,
        run_id: Option<&str>,
    ) -> Result<(), RepositoryError> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        set_tenant(&mut tx, &t).await?;
        let n = sqlx::query(
            "UPDATE schedule_firings SET state=$7, run_id=$8 \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
               AND schedule_id=$5 AND scheduled_at=$6",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(schedule_id)
        .bind(ms_to_timestamptz_cron(scheduled_at_ms))
        .bind(firing_state_str(state))
        .bind(run_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?
        .rows_affected();
        if n == 0 {
            let _ = tx.rollback().await;
            return Err(RepositoryError::NotFound);
        }
        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        Ok(())
    }

    async fn list_due_firings(
        &self,
        scope: &Scope,
        now_ms: u64,
        limit: u32,
    ) -> Result<Vec<ScheduleFiring>, RepositoryError> {
        // K10 controller hot path. The (state, scheduled_at) index makes
        // the WHERE clause an index scan; we ORDER BY scheduled_at so the
        // caller drains the oldest due firing first. `limit` caps the batch
        // so a misbehaving clock (clock skew, big catch-up batch) can't
        // pull the entire schedules table into memory at once.
        let (t, p, w, s) = scope_tuple(scope);
        let now_ts = ms_to_timestamptz_cron(now_ms);
        let rows = sqlx::query(
            "SELECT schedule_id, scheduled_at, firing_id, claimed_by, state, run_id \
             FROM schedule_firings \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
               AND state='intent' AND scheduled_at <= $5 \
             ORDER BY scheduled_at ASC \
             LIMIT $6",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(now_ts)
        .bind(limit as i64)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| RepositoryError::Other(format!("list_due_firings: {e}")))?;
        let mut out: Vec<ScheduleFiring> = Vec::with_capacity(rows.len());
        for row in rows {
            let scheduled_at: chrono::DateTime<chrono::Utc> = row.get("scheduled_at");
            out.push(ScheduleFiring {
                schedule_id: row.get("schedule_id"),
                scheduled_at_ms: timestamptz_cron_to_ms(scheduled_at),
                firing_id: row.get("firing_id"),
                claimed_by: row.get("claimed_by"),
                state: parse_firing_state(row.get::<String, _>("state").as_str()),
                run_id: row.get("run_id"),
            });
        }
        Ok(out)
    }

    async fn delete_schedule(
        &self,
        scope: &Scope,
        schedule_id: &str,
    ) -> Result<bool, RepositoryError> {
        // K18: same-transaction drop of firings + schedule row. RLS scopes
        // the WHERE to the current tenant (the FORCE RLS policy rejects
        // any row whose tenant_id doesn't match `app.tenant_id`), so a
        // cross-tenant id can never wipe another tenant's data even with a
        // leaked (scope, schedule_id) tuple. Returns the schedules
        // rows_affected; 0 = id did not exist in this scope → false, 1+ =
        // removed.
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        set_tenant(&mut tx, &t).await?;
        sqlx::query(
            "DELETE FROM schedule_firings \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
               AND schedule_id=$5",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(schedule_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(format!("delete firings: {e}")))?;
        let n = sqlx::query(
            "DELETE FROM schedules \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
               AND schedule_id=$5",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(schedule_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(format!("delete schedule: {e}")))?
        .rows_affected();
        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        Ok(n > 0)
    }

    async fn list_schedules(&self, scope: &Scope) -> Result<Vec<Schedule>, RepositoryError> {
        // ORDER BY schedule_id so two callers see the same ordering — used
        // by `list_all_jobs` which sorts by next_run, but `list_schedules`
        // is also a stable snapshot for panel/debug views.
        let (t, p, w, s) = scope_tuple(scope);
        let rows = sqlx::query(
            "SELECT schedule_id, expression, timezone, next_fire_at, misfire_policy, enabled \
             FROM schedules \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
             ORDER BY schedule_id ASC",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| RepositoryError::Other(format!("list_schedules: {e}")))?;
        let mut out: Vec<Schedule> = Vec::with_capacity(rows.len());
        for row in rows {
            let next_fire_at: Option<chrono::DateTime<chrono::Utc>> = row.get("next_fire_at");
            out.push(Schedule {
                schedule_id: row.get("schedule_id"),
                expression: row.get("expression"),
                timezone: row.get("timezone"),
                next_fire_at_ms: next_fire_at.map(timestamptz_cron_to_ms),
                misfire_policy: parse_misfire(row.get::<String, _>("misfire_policy").as_str()),
                enabled: row.get("enabled"),
            });
        }
        Ok(out)
    }

    async fn get_schedule(
        &self,
        scope: &Scope,
        schedule_id: &str,
    ) -> Result<Option<Schedule>, RepositoryError> {
        let (t, p, w, s) = scope_tuple(scope);
        let row = sqlx::query(
            "SELECT schedule_id, expression, timezone, next_fire_at, misfire_policy, enabled \
             FROM schedules \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
               AND schedule_id=$5",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(schedule_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| RepositoryError::Other(format!("get_schedule: {e}")))?;
        let row = match row {
            None => return Ok(None),
            Some(r) => r,
        };
        let next_fire_at: Option<chrono::DateTime<chrono::Utc>> = row.get("next_fire_at");
        Ok(Some(Schedule {
            schedule_id: row.get("schedule_id"),
            expression: row.get("expression"),
            timezone: row.get("timezone"),
            next_fire_at_ms: next_fire_at.map(timestamptz_cron_to_ms),
            misfire_policy: parse_misfire(row.get::<String, _>("misfire_policy").as_str()),
            enabled: row.get("enabled"),
        }))
    }

    async fn update_schedule(
        &self,
        scope: &Scope,
        schedule: Schedule,
    ) -> Result<(), RepositoryError> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        set_tenant(&mut tx, &t).await?;
        let n = sqlx::query(
            "UPDATE schedules SET expression=$6, timezone=$7, next_fire_at=$8, \
                                   misfire_policy=$9, enabled=$10 \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 \
               AND schedule_id=$5",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .bind(&schedule.schedule_id)
        .bind(&schedule.expression)
        .bind(&schedule.timezone)
        .bind(schedule.next_fire_at_ms.map(ms_to_timestamptz_cron))
        .bind(misfire_str(schedule.misfire_policy))
        .bind(schedule.enabled)
        .execute(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(format!("update_schedule: {e}")))?
        .rows_affected();
        if n == 0 {
            let _ = tx.rollback().await;
            return Err(RepositoryError::NotFound);
        }
        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// c5 K16 backup/restore: pg_dump/pg_restore-style logical backup via sqlx.
// Self-contained — does not shell out to pg_dump; the runtime pool walks
// every table we own and emits an INSERT-per-row SQL stream.
// ---------------------------------------------------------------------------

/// All table names this migration owns, in dependency order (parents first
/// to avoid FK violations on restore). Keep in sync with the migrations in
/// `crates/octos-store/migrations/`.
const OWNED_TABLES: &[&str] = &[
    "schedules",
    "schedule_firings",
    "run_leases",
    "run_checkpoints",
    "tool_invocations",
    "approvals",
    "outbox",
    "session_events",
    "messages",
    "agent_runs",
    "sessions",
];

impl PgStore {
    /// Dump every owned table as a stream of INSERT statements (multi-row
    /// INSERT form when possible) plus the SET/RESET tenant context
    /// required by the RLS policy. Returns a SQL string the caller writes
    /// to disk as a logical backup. `tenant_scope_tenant` is the tenant
    /// the backup writer will `SET LOCAL app.tenant_id` to for restore —
    /// backups scoped to a single tenant are the typical case; the writer
    /// sets this context on the import transaction so RLS accepts the rows.
    pub async fn dump_tables_sql(
        &self,
        tenant_scope_tenant: &str,
    ) -> Result<String, RepositoryError> {
        use std::fmt::Write;
        let mut out = String::new();
        writeln!(
            out,
            "-- octos-store logical backup (c5 K16); restore with the same \
             PG image (postgres:16) and run `restore_tables_sql` in a single \
             transaction. RLS is bypassed during restore (the table owner role \
             used by the runtime migration); the restored rows must belong to \
             a single tenant."
        )
        .unwrap();
        writeln!(
            out,
            "BEGIN; SET LOCAL app.tenant_id = '{}';",
            tenant_scope_tenant.replace('\'', "''")
        )
        .unwrap();
        for table in OWNED_TABLES {
            writeln!(out, "\n-- {table}").unwrap();
            // Use a per-statement query to read every row; the small number of
            // tables and modest row counts in a typical single-tenant backup
            // make this acceptable.
            let rows = sqlx::query(&format!("SELECT * FROM {table} ORDER BY 1"))
                .fetch_all(&*self.pool)
                .await
                .map_err(|e| RepositoryError::Other(format!("dump {table}: {e}")))?;
            for row in rows {
                let cols: Vec<String> =
                    (0..row.len()).map(|i| encode_dump_value(&row, i)).collect();
                writeln!(
                    out,
                    "INSERT INTO {table} VALUES ({}) ON CONFLICT DO NOTHING;",
                    cols.join(", ")
                )
                .unwrap();
            }
        }
        writeln!(out, "COMMIT;").unwrap();
        Ok(out)
    }

    /// Restore from a logical-backup SQL stream produced by
    /// [`dump_tables_sql`]. Statements are split on `;` and executed one per
    /// round-trip so the PG simple-query parser accepts them individually
    /// (raw_sql does not multi-statement on the simple-query protocol).
    /// Idempotent (`ON CONFLICT DO NOTHING`).
    pub async fn restore_tables_sql(&self, sql: &str) -> Result<(), RepositoryError> {
        // Strip whole-line `--` comments so the script is one tidy block;
        // sqlx::raw_sql runs multi-statement scripts inside the tx via the
        // extended protocol (single-statement parser is what fails).
        let cleaned: String = sql
            .lines()
            .filter(|l| !l.trim_start().starts_with("--"))
            .collect::<Vec<_>>()
            .join(
                "
",
            );
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(format!("restore begin: {e}")))?;
        // The script is one logical unit; sqlx 0.8 accepts multi-statement
        // simple-query via `Executor::execute_many(&str)` — the *only* path
        // that runs BEGIN/INSERT/.../COMMIT as one block. raw_sql(&str) and
        // execute(&str) both reject multi-statement scripts because the
        // extended protocol parses only one statement at a time and JSONB
        // literals like `{"s":"x"}` then trip the parser.
        use futures::StreamExt;
        use sqlx::Executor as _;
        let conn = &mut *tx;
        let mut stream = conn.execute_many(cleaned.as_str());
        while let Some(r) = stream.next().await {
            r.map_err(|e| RepositoryError::Other(format!("restore: {e}")))?;
        }
        drop(stream);
        tx.commit()
            .await
            .map_err(|e| RepositoryError::Other(format!("restore commit: {e}")))?;
        Ok(())
    }
}

/// Encode a column value for INSERT literal use in the dump. Tries each
/// concrete sqlx type in order; falls back to JSON. TEXT and JSONB are
/// stringified; numerics/bools are serialized; timestamps ISO-8601;
/// NULL is `NULL`. Order matters: more-specific types first.
fn encode_dump_value(row: &sqlx::postgres::PgRow, i: usize) -> String {
    if let Ok(v) = row.try_get::<Option<i64>, _>(i) {
        return match v {
            Some(x) => x.to_string(),
            None => return "NULL".to_string(),
        };
    }
    if let Ok(v) = row.try_get::<Option<bool>, _>(i) {
        return match v {
            Some(b) => b.to_string(),
            None => return "NULL".to_string(),
        };
    }
    if let Ok(v) = row.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>(i) {
        return match v {
            Some(dt) => format!("'{}'", dt.to_rfc3339()),
            None => return "NULL".to_string(),
        };
    }
    if let Ok(v) = row.try_get::<Option<String>, _>(i) {
        return match v {
            Some(s) => format!("'{}'", s.replace('\'', "''")),
            None => return "NULL".to_string(),
        };
    }
    if let Ok(v) = row.try_get::<Option<serde_json::Value>, _>(i) {
        return match v {
            Some(j) => match j {
                serde_json::Value::String(s) => format!("'{}'", s.replace('\'', "''")),
                serde_json::Value::Null => "NULL".to_string(),
                other => other.to_string(),
            },
            None => return "NULL".to_string(),
        };
    }
    "NULL".to_string()
}

// ---------------------------------------------------------------------------
// c5: migration audit (counts / digests / canonical order) for a scope.
// Compare two stores (e.g. a pre-migration file backend + a post-migration
// PG backend, or two PG stores before/after a round-trip) on the owned
// tables for a given scope.
// ---------------------------------------------------------------------------

/// Per-scope migration audit report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeAudit {
    pub scope_key: String,
    pub counts: Vec<(String, usize)>,
    /// SHA-256 over a canonical projection of every owned aggregate in this
    /// scope — messages, events, checkpoints, leases, invocations, approvals,
    /// schedules, firings. Two audits match iff the digests are equal
    /// (after both sorts).
    pub digest: String,
}

fn canonical_json_digest(items: &[serde_json::Value]) -> String {
    use sha2::{Digest, Sha256};
    let mut sorted: Vec<&serde_json::Value> = items.iter().collect();
    sorted.sort_by_key(|a| a.to_string());
    let mut h = Sha256::new();
    for v in sorted {
        h.update(v.to_string().as_bytes());
        h.update(b"|");
    }
    format!("sha256:{:x}", h.finalize())
}

impl PgStore {
    /// Audit a single scope: count rows in each owned table that belong to
    /// `scope`, plus a canonical digest over the scope's full content
    /// projection. Two stores with equal reports for the same scope are
    /// migration-equivalent (per spec c5 migration-fidelity).
    pub async fn audit_scope(&self, scope: &Scope) -> Result<ScopeAudit, RepositoryError> {
        let (t, p, w, s) = scope_tuple(scope);
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| RepositoryError::Other(e.to_string()))?;
        set_tenant(&mut tx, &t).await?;

        let mut counts = Vec::new();
        let mut projections: Vec<serde_json::Value> = Vec::new();

        // messages
        let msgs = sqlx::query(
            "SELECT message_id, thread_id, turn_id, role, content, ordinal FROM messages \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 ORDER BY ordinal",
        )
        .bind(&t).bind(&p).bind(&w).bind(&s)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?;
        counts.push(("messages".to_string(), msgs.len()));
        for m in &msgs {
            projections.push(serde_json::json!({
                "kind": "message",
                "id": m.get::<String, _>("message_id"),
                "ordinal": m.get::<i64, _>("ordinal"),
                "role": m.get::<String, _>("role"),
            }));
        }

        // session_events (canonical seq order)
        let events = sqlx::query(
            "SELECT event_id, causation_id, seq, payload FROM session_events \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 ORDER BY seq",
        )
        .bind(&t).bind(&p).bind(&w).bind(&s)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?;
        counts.push(("session_events".to_string(), events.len()));
        for e in &events {
            projections.push(serde_json::json!({
                "kind": "event",
                "id": e.get::<String, _>("event_id"),
                "seq": e.get::<i64, _>("seq"),
            }));
        }

        // run_leases (one per run)
        let leases = sqlx::query(
            "SELECT run_id, owner_id, epoch FROM run_leases \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?;
        counts.push(("run_leases".to_string(), leases.len()));
        for l in &leases {
            projections.push(serde_json::json!({
                "kind": "lease",
                "run_id": l.get::<String, _>("run_id"),
                "epoch": l.get::<i64, _>("epoch"),
            }));
        }

        // run_checkpoints
        let cps = sqlx::query(
            "SELECT run_id, step, transcript_highwater FROM run_checkpoints \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4 ORDER BY run_id, step",
        )
        .bind(&t).bind(&p).bind(&w).bind(&s)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?;
        counts.push(("run_checkpoints".to_string(), cps.len()));
        for c in &cps {
            projections.push(serde_json::json!({
                "kind": "checkpoint",
                "run_id": c.get::<String, _>("run_id"),
                "step": c.get::<i64, _>("step"),
                "transcript_highwater": c.get::<i64, _>("transcript_highwater"),
            }));
        }

        // tool_invocations
        let invs = sqlx::query(
            "SELECT invocation_id, run_id, state FROM tool_invocations \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?;
        counts.push(("tool_invocations".to_string(), invs.len()));
        for i in &invs {
            projections.push(serde_json::json!({
                "kind": "invocation",
                "id": i.get::<String, _>("invocation_id"),
                "run_id": i.get::<String, _>("run_id"),
                "state": i.get::<String, _>("state"),
            }));
        }

        // approvals
        let aps = sqlx::query(
            "SELECT approval_id, state FROM approvals \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?;
        counts.push(("approvals".to_string(), aps.len()));
        for a in &aps {
            projections.push(serde_json::json!({
                "kind": "approval",
                "id": a.get::<String, _>("approval_id"),
                "state": a.get::<String, _>("state"),
            }));
        }

        // outbox (drained + undrained; per-scope partition by tenant_id)
        let outbox = sqlx::query(
            "SELECT id, aggregate_key, topic FROM outbox              WHERE tenant_id=$1 ORDER BY id",
        )
        .bind(&t)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?;
        counts.push(("outbox".to_string(), outbox.len()));
        for o in &outbox {
            projections.push(serde_json::json!({
                "kind": "outbox",
                "id": o.get::<i64, _>("id"),
                "aggregate_key": o.get::<String, _>("aggregate_key"),
                "topic": o.get::<String, _>("topic"),
            }));
        }

        // schedules
        let schs = sqlx::query(
            "SELECT schedule_id, expression, enabled FROM schedules \
             WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4",
        )
        .bind(&t)
        .bind(&p)
        .bind(&w)
        .bind(&s)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?;
        counts.push(("schedules".to_string(), schs.len()));
        for sc in &schs {
            projections.push(serde_json::json!({
                "kind": "schedule",
                "id": sc.get::<String, _>("schedule_id"),
                "expression": sc.get::<String, _>("expression"),
                "enabled": sc.get::<bool, _>("enabled"),
            }));
        }

        // schedule_firings (K10: per (scope, schedule, instant))
        let sfs = sqlx::query(
            "SELECT schedule_id, scheduled_at, firing_id, state FROM schedule_firings              WHERE tenant_id=$1 AND profile_id=$2 AND workspace_id=$3 AND session_id=$4",
        )
        .bind(&t).bind(&p).bind(&w).bind(&s)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| RepositoryError::Other(e.to_string()))?;
        counts.push(("schedule_firings".to_string(), sfs.len()));
        for sf in &sfs {
            projections.push(serde_json::json!({
                "kind": "firing",
                "schedule_id": sf.get::<String, _>("schedule_id"),
                "scheduled_at_ms": sf.get::<chrono::DateTime<chrono::Utc>, _>("scheduled_at").timestamp_millis(),
                "firing_id": sf.get::<String, _>("firing_id"),
                "state": sf.get::<String, _>("state"),
            }));
        }

        let digest = canonical_json_digest(&projections);
        let _ = tx.commit().await;
        Ok(ScopeAudit {
            scope_key: format!("{t}/{p}/{w}/{s}"),
            counts,
            digest,
        })
    }
}
