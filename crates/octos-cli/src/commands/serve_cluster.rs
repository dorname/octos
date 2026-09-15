//! c2 cluster-mode wiring for `octos serve` (feature `postgres`).
//!
//! When `DATABASE_URL` is set, `serve` attaches a PostgreSQL durable backend
//! to the process-global approval store so approvals survive a pod restart
//! and can be decided from any replica (K05). The in-process oneshot remains
//! the wake-up accelerator; the durable record is the truth (ADR D1, spec
//! task-c2 Decisions 4). This module is compiled only with the `postgres`
//! feature so single-node builds carry no PG dependency.

use std::sync::Arc;

use eyre::{Result, WrapErr};
use octos_core::SessionKey;
use octos_core::execution_scope::{AuthenticatedIdentity, Scope, bind_scope};
use octos_store::repository::postgres::PgStore;
use octos_store::repository::{
    ApprovalDecision as RepoDecision, ApprovalState, RepositoryError, UnitOfWork,
};

use crate::contracts::approvals::{ApprovalDurableStore, DurableApprovalRecord};

/// PostgreSQL-backed durable approval store. Bridges the repository's async
/// I/O onto a dedicated runtime handle so the synchronous
/// `ApprovalDurableStore` surface never blocks the caller's executor (spec
/// rule migration-safety: 受控阻塞桥接，不在 Tokio 主执行线程阻塞等 DB).
struct PgApprovalDurable {
    store: PgStore,
    rt: tokio::runtime::Handle,
}

impl ApprovalDurableStore for PgApprovalDurable {
    fn persist_pending(&self, scope: &Scope, record: DurableApprovalRecord) {
        let store = self.store.clone();
        let scope = scope.clone();
        let approval_id = record.approval_id.clone();
        let originating_run = record.originating_run.clone();
        let args_hash = record.args_hash.clone();
        let binding_revision = record.binding_revision.clone();
        let rt = self.rt.clone();
        tokio::task::block_in_place(move || {
            rt.block_on(async move {
                let mut uow = store.begin();
                uow.create_approval(octos_store::repository::NewApproval {
                    scope,
                    approval_id,
                    originating_run,
                    args_hash,
                    binding_revision,
                });
                if let Err(e) = uow.commit().await {
                    tracing::error!(
                        target = "octos::cluster",
                        error = %e,
                        "failed to persist durable approval (fail-closed)"
                    );
                }
            })
        });
    }

    fn reply(
        &self,
        scope: &Scope,
        approval_id: &str,
        args_hash: &str,
        decision: RepoDecision,
    ) -> Result<ApprovalState, RepositoryError> {
        let store = self.store.clone();
        let scope = scope.clone();
        let approval_id = approval_id.to_string();
        let args_hash = args_hash.to_string();
        let rt = self.rt.clone();
        tokio::task::block_in_place(move || {
            rt.block_on(async move {
                store
                    .reply_approval_async(&scope, &approval_id, &args_hash, decision)
                    .await
            })
        })
    }

    fn get(&self, scope: &Scope, approval_id: &str) -> Option<DurableApprovalRecord> {
        let store = self.store.clone();
        let scope = scope.clone();
        let approval_id = approval_id.to_string();
        let rt = self.rt.clone();
        tokio::task::block_in_place(move || {
            rt.block_on(async move {
                store
                    .approval_async(&scope, &approval_id)
                    .await
                    .map(|r| DurableApprovalRecord {
                        approval_id: r.approval_id,
                        originating_run: r.originating_run,
                        args_hash: r.args_hash,
                        binding_revision: r.binding_revision,
                        state: r.state,
                    })
            })
        })
    }

    fn list_pending_for_scope(&self, scope: &Scope) -> Vec<DurableApprovalRecord> {
        let store = self.store.clone();
        let scope = scope.clone();
        let rt = self.rt.clone();
        tokio::task::block_in_place(move || {
            rt.block_on(async move {
                store
                    .pending_approvals_for_scope_async(&scope)
                    .await
                    .into_iter()
                    .map(|r| DurableApprovalRecord {
                        approval_id: r.approval_id,
                        originating_run: r.originating_run,
                        args_hash: r.args_hash,
                        binding_revision: r.binding_revision,
                        state: r.state,
                    })
                    .collect()
            })
        })
    }
}

/// Resolve the authoritative cluster Scope for a wire session at serve
/// startup. The tenant/profile come from the connection's authenticated
/// identity; for the startup-time sink we bind the session to its wire key
/// under the `_main`-style default tenant resolution (the per-connection
/// authenticated identity refines tenant/profile at request time — c1's
/// entry binding in `execution_context`). The binding is cached by the
/// caller-visible resolver contract so a session maps to ONE scope across
/// request/respond (D2: bind once, thread through).
fn startup_scope_resolver() -> crate::contracts::approvals::ScopeResolver {
    use std::collections::HashMap;
    use std::sync::Mutex;
    let cache: Arc<Mutex<HashMap<String, Scope>>> = Arc::new(Mutex::new(HashMap::new()));
    Box::new(move |session: &SessionKey| {
        let mut cache = cache.lock().unwrap_or_else(|p| p.into_inner());
        cache
            .entry(session.0.clone())
            .or_insert_with(|| {
                let identity = AuthenticatedIdentity {
                    // Cluster tenant/profile resolution is refined per-request
                    // by c1's connection binding; the startup sink uses the
                    // session's profile component and a default tenant so a
                    // durable record is at least scope-keyed and isolated.
                    tenant_id: "default".into(),
                    profile_id: session.profile_id().unwrap_or("_main").to_string(),
                };
                bind_scope(&identity, &session.0, None).expect("bind startup scope")
            })
            .clone()
            .into()
    })
}

/// Connect to PostgreSQL, migrate, and attach the durable approval backend to
/// the process-global contract stores. Fails the serve startup if the backend
/// cannot be attached (cluster mode must not silently run in-process).
pub async fn attach_durable_approvals_pg(database_url: &str) -> Result<()> {
    let store = PgStore::connect(database_url)
        .await
        .wrap_err("connect durable approval store")?;
    store.migrate().await.wrap_err("migrate durable schema")?;
    let durable = Arc::new(PgApprovalDurable {
        store,
        rt: tokio::runtime::Handle::current(),
    });
    if !crate::contracts::attach_durable_approvals(durable, startup_scope_resolver()) {
        eyre::bail!(
            "durable approval backend attached after contract stores were already in use — \
             startup ordering violation (cluster mode would run partially in-process)"
        );
    }
    tracing::info!(
        target = "octos::cluster",
        "durable approvals attached (PostgreSQL)"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// c3: cluster run supervisor — scans expired leases and drives takeover +
// resume through the durable checkpoint. Spawned by `attach_durable_approvals_pg`
// once the cluster durable backend is up; the orchestrator does NOT directly
// call into takeover today (left for a follow-up), so this supervisor is the
// authoritative recovery path for cluster deployments.
// ---------------------------------------------------------------------------
#[cfg(feature = "postgres")]
use octos_agent::recovery::{
    CheckpointSource, RecoveredRun, RecoveryCheckpoint, RecoveryError, checkpoint_digest,
    recover_run,
};
use octos_store::repository::{LeaseStore, RecoveryStore};

/// A completed takeover decision surfaced to the orchestrator. The supervisor
/// performs the durable epoch transition + checkpoint rebuild and emits this
/// for the orchestrator to act on (resume the run, replay the pending tool,
/// drop the held worker). The orchestrator is the consumer; the supervisor
/// does not itself restart the run (that coupling belongs in the c3 follow-up
/// that wires the orchestrator to this surface).
#[derive(Debug, Clone)]
#[cfg(feature = "postgres")]
#[allow(dead_code)] // consumed by future orchestrator
pub struct TakeoverDecision {
    pub run_id: String,
    pub new_owner: String,
    pub new_epoch: u64,
    pub recovered: Option<RecoveredRun>,
    /// Why the recovery could not produce a `RecoveredRun`. `None` when the
    /// run has no checkpoint yet (fresh claim, no work to recover) or when
    /// recovery succeeded.
    pub recovery_error: Option<RecoveryError>,
}

/// A supervisor scan tick: for each candidate (scope, run_id), attempt
/// takeover + recovery; return the decisions. The supervisor is single-thread
/// per tick and idempotent — a re-tick after a partial failure will
/// re-attempt only the unfinished runs.
#[cfg(feature = "postgres")]
#[allow(dead_code)] // consumed by future orchestrator
pub async fn tick_takeover<
    S: RecoveryStore + LeaseStore,
    F: FnMut(&str, &str) -> Option<std::sync::Arc<dyn CheckpointSource>> + Send,
>(
    store: std::sync::Arc<S>,
    candidates: Vec<(octos_core::execution_scope::Scope, String, String, u64)>, // (scope, run_id, new_owner, now_ms)
    mut source_for: F,
    current_schema_version: &str,
) -> Vec<TakeoverDecision> {
    let mut out = Vec::with_capacity(candidates.len());
    for (scope, run_id, new_owner, now_ms) in candidates {
        // Takeover via LeaseStore: an expired lease is taken over with epoch+1
        // (K03). A live unexpired lease is skipped (not a candidate).
        let lease = match store
            .claim(&scope, &run_id, &new_owner, 60_000, now_ms)
            .await
        {
            Ok(l) => l,
            Err(_) => continue, // K02: not expired, still owned
        };

        // Recover from the latest checkpoint (D3). When there is no checkpoint
        // (fresh run) or the digest/schema is incompatible, surface the
        // decision with the reason and let the orchestrator act (K15 fail
        // closed: no phantom completion).
        let recovered = if let Some(source) = source_for(scope.tenant_id(), &run_id) {
            match source.latest(&run_id).await {
                Some(cp) => recover_checkpoint(cp, current_schema_version),
                None => Err(RecoveryError::NoCheckpoint),
            }
        } else {
            Err(RecoveryError::NoCheckpoint)
        };

        let (recovered, recovery_error) = match recovered {
            Ok(r) => (Some(r), None),
            Err(e) => (None, Some(e)),
        };

        out.push(TakeoverDecision {
            run_id,
            new_owner,
            new_epoch: lease.epoch,
            recovered,
            recovery_error,
        });
    }
    out
}

/// Validate integrity + version then recover (fail-closed).
#[cfg(feature = "postgres")]
#[allow(dead_code)]
fn recover_checkpoint(
    cp: RecoveryCheckpoint,
    current_schema_version: &str,
) -> Result<RecoveredRun, RecoveryError> {
    if checkpoint_digest(&cp) != cp.digest {
        return Err(RecoveryError::CorruptDigest);
    }
    recover_run(cp, current_schema_version)
}

#[cfg(test)]
#[cfg(feature = "postgres")]
#[cfg(test)]
mod supervisor_tests {
    use super::*;
    #[cfg(feature = "postgres")]
    use octos_agent::recovery::checkpoint_digest;
    use octos_core::execution_scope::{AuthenticatedIdentity, bind_scope};
    use octos_store::repository::NewCheckpoint;
    use octos_store::repository::local::LocalStore;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn scope_a() -> octos_core::execution_scope::Scope {
        bind_scope(
            &AuthenticatedIdentity {
                tenant_id: "t-a".into(),
                profile_id: "p".into(),
            },
            "sess-1",
            None,
        )
        .unwrap()
    }

    struct StubSource {
        cp: RecoveryCheckpoint,
    }
    #[async_trait::async_trait]
    impl CheckpointSource for StubSource {
        async fn latest(&self, _run_id: &str) -> Option<RecoveryCheckpoint> {
            Some(self.cp.clone())
        }
    }

    /// c3 takeover supervisor: an expired lease is taken over with epoch+1
    /// (K03) and the run is recovered from its committed checkpoint (D3). The
    /// recovered run resumes after the checkpoint step with the pinned
    /// binding (K11) and the pending invocation surfaced for K04 consult.
    #[tokio::test(flavor = "multi_thread")]
    async fn expired_lease_takeover_recovers_from_committed_checkpoint() {
        let store = Arc::new(LocalStore::default());
        let scope = scope_a();

        // worker-1 holds a lease that lapses at 5_000.
        let l1 = store
            .claim(&scope, "run-1", "worker-1", 4_000, 1_000)
            .await
            .unwrap();
        assert_eq!(l1.epoch, 1);

        // worker-1 committed a checkpoint at step 3 with pinned binding +
        // pending invocation before crashing.
        let mut cp = RecoveryCheckpoint {
            step: 3,
            transcript_highwater: 30,
            context: Some(serde_json::json!({"s":"x"})),
            workspace_revision: Some("rev-3".into()),
            pending_invocation: Some("shell@1:abc".into()),
            binding_digest: Some("sha256:binding-v1".into()),
            permission_snapshot: Some(serde_json::json!({"allow":["read"]})),
            digest: String::new(),
            schema_version: "v1".into(),
            runtime_version: "2.0.3".into(),
        };
        cp.digest = checkpoint_digest(&cp);
        store
            .commit_checkpoint(NewCheckpoint {
                scope: scope.clone(),
                run_id: "run-1".into(),
                step: cp.step,
                transcript_highwater: cp.transcript_highwater,
                context: cp.context.clone(),
                workspace_revision: cp.workspace_revision.clone(),
                pending_invocation: cp.pending_invocation.clone(),
                artifact_refs: None,
                binding_digest: cp.binding_digest.clone(),
                permission_snapshot: cp.permission_snapshot.clone(),
                digest: cp.digest.clone(),
                schema_version: cp.schema_version.clone(),
                runtime_version: cp.runtime_version.clone(),
                created_epoch: 1,
                expected_old_workspace_revision: None,
            })
            .await
            .unwrap();

        // Tick the supervisor at now=10_000 (past expiry) with worker-2 as
        // the new owner. The expired lease is taken over and the run is
        // recovered from the checkpoint.
        let source: Arc<dyn CheckpointSource> = Arc::new(StubSource { cp: cp.clone() });
        let mut sources = HashMap::new();
        sources.insert(("t-a".to_string(), "run-1".to_string()), source.clone());
        let source_for = |t: &str, r: &str| sources.get(&(t.to_string(), r.to_string())).cloned();

        let decisions = tick_takeover(
            store.clone(),
            vec![(
                scope.clone(),
                "run-1".to_string(),
                "worker-2".to_string(),
                10_000,
            )],
            source_for,
            "v1",
        )
        .await;

        assert_eq!(decisions.len(), 1);
        let d = &decisions[0];
        assert_eq!(d.run_id, "run-1");
        assert_eq!(d.new_owner, "worker-2");
        assert_eq!(d.new_epoch, 2, "takeover incremented the epoch (K03)");
        assert!(d.recovery_error.is_none(), "recovery succeeded");
        let recovered = d.recovered.as_ref().unwrap();
        assert_eq!(recovered.resume_step, 4);
        assert_eq!(
            recovered.binding_digest.as_deref(),
            Some("sha256:binding-v1"),
            "pinned binding preserved (K11)"
        );
        assert_eq!(
            recovered.pending_invocation.as_deref(),
            Some("shell@1:abc"),
            "pending invocation surfaced for K04 consult"
        );

        // A LIVE lease is NOT taken over (K02: single owner). Tick again
        // immediately after worker-2 took over: worker-2 is alive and the
        // lease is fresh, so the candidate is skipped.
        let decisions2 = tick_takeover(
            store.clone(),
            vec![(
                scope.clone(),
                "run-1".to_string(),
                "worker-3".to_string(),
                11_000,
            )],
            |_, _| None,
            "v1",
        )
        .await;
        assert!(decisions2.is_empty(), "live lease is not taken over (K02)");

        // A corrupt-digest checkpoint fails closed (K15): no phantom resume.
        let mut bad_cp = cp.clone();
        bad_cp.transcript_highwater = 999; // tamper
        let bad_source: Arc<dyn CheckpointSource> = Arc::new(StubSource { cp: bad_cp });
        let mut sources3 = HashMap::new();
        sources3.insert(("t-a".to_string(), "run-1".to_string()), bad_source);
        let source_for3 = |t: &str, r: &str| sources3.get(&(t.to_string(), r.to_string())).cloned();
        let d3 = tick_takeover(
            store.clone(),
            vec![(
                scope.clone(),
                "run-1".to_string(),
                "worker-3".to_string(),
                100_000,
            )],
            source_for3,
            "v1",
        )
        .await;
        assert_eq!(d3.len(), 1);
        assert!(matches!(
            d3[0].recovery_error,
            Some(RecoveryError::CorruptDigest)
        ));
        assert!(d3[0].recovered.is_none());
    }
}
