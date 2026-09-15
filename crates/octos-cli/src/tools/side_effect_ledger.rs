//! Bridge from octos-agent's `SideEffectLedger` (c3/K04) to octos-store's
//! durable `RecoveryStore`. Lets an `IdempotentToolExecutor` in a cluster
//! serve consult/persist real durable invocation records (local adapter or
//! PostgreSQL) instead of an in-memory stand-in.
//!
//! The agent layer stays free of any store/DB type; this adapter owns the
//! mapping, including the sync→async bridge (`block_in_place` + `block_on`)
//! so the durable I/O never blocks the agent's Tokio executor directly (spec
//! rule migration-safety).

use std::sync::Arc;

use octos_agent::tools::{SideEffectLedger, SideEffectVerdict};
use octos_core::execution_scope::Scope;
use octos_store::repository::{InvocationState, NewInvocation, RecoveryStore};

/// A `SideEffectLedger` over a durable `RecoveryStore`, scoped to one run's
/// execution scope. Constructed per-run at the point the orchestrator wraps
/// side-effecting tools.
pub struct DurableSideEffectLedger<S: RecoveryStore + 'static> {
    store: Arc<S>,
    scope: Scope,
    run_id: String,
    rt: tokio::runtime::Handle,
}

impl<S: RecoveryStore + 'static> DurableSideEffectLedger<S> {
    pub fn new(store: Arc<S>, scope: Scope, run_id: impl Into<String>) -> Self {
        Self {
            store,
            scope,
            run_id: run_id.into(),
            rt: tokio::runtime::Handle::current(),
        }
    }

    /// Bridge an async repository call off the current task. `serve` runs a
    /// multi-thread runtime, so `block_in_place` is available.
    fn block<F, T>(&self, f: F) -> T
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let rt = self.rt.clone();
        tokio::task::block_in_place(move || rt.block_on(f))
    }
}

#[async_trait::async_trait]
impl<S: RecoveryStore + 'static> SideEffectLedger for DurableSideEffectLedger<S> {
    async fn consult(
        &self,
        invocation_id: &str,
        tool_revision: &str,
        args_hash: &str,
        idempotency_key: Option<&str>,
    ) -> SideEffectVerdict {
        let store = Arc::clone(&self.store);
        let scope = self.scope.clone();
        let run_id = self.run_id.clone();
        let invocation_id = invocation_id.to_string();
        let tool_revision = tool_revision.to_string();
        let args_hash = args_hash.to_string();
        let idem = idempotency_key.map(str::to_string);

        // First: if we have an idempotency key, a CONFIRMED effect under it is
        // reused regardless of the logical id (K04 reuse-by-key).
        if let Some(key) = idem.clone() {
            let store2 = Arc::clone(&store);
            let scope2 = scope.clone();
            let found =
                self.block(async move { store2.find_by_idempotency_key(&scope2, &key).await });
            if let Some(rec) = found {
                if rec.state == InvocationState::Succeeded {
                    return SideEffectVerdict::ReuseConfirmed;
                }
            }
        }

        // Then: consult/record the logical intent.
        let state = self.block(async move {
            store
                .record_intent(NewInvocation {
                    scope,
                    run_id,
                    invocation_id,
                    tool_revision,
                    args_hash,
                    external_idempotency_key: idem,
                })
                .await
        });
        match state {
            Ok(InvocationState::Intent) => SideEffectVerdict::Fresh,
            Ok(InvocationState::Succeeded) => SideEffectVerdict::ReuseConfirmed,
            Ok(InvocationState::Failed) => SideEffectVerdict::Retryable,
            Ok(InvocationState::Unknown) => SideEffectVerdict::Reconcile,
            // A tampered args_hash (ArgsMismatch) or any store error fails
            // closed to reconciliation — never silently execute a possibly-
            // duplicate side effect.
            Err(_) => SideEffectVerdict::Reconcile,
        }
    }

    async fn confirmed_result(&self, invocation_id: &str) -> Option<String> {
        let store = Arc::clone(&self.store);
        let scope = self.scope.clone();
        let run_id = self.run_id.clone();
        let invocation_id = invocation_id.to_string();
        self.block(async move {
            store
                .invocation_by_id(&scope, &run_id, &invocation_id)
                .await
                .and_then(|rec| rec.result_ref)
        })
    }

    async fn mark_succeeded(&self, invocation_id: &str, result_ref: &str) {
        let store = Arc::clone(&self.store);
        let scope = self.scope.clone();
        let run_id = self.run_id.clone();
        let invocation_id = invocation_id.to_string();
        let result_ref = result_ref.to_string();
        self.block(async move {
            let _ = store
                .mark_succeeded(&scope, &run_id, &invocation_id, &result_ref, None)
                .await;
        });
    }

    async fn mark_unknown(&self, invocation_id: &str) {
        let store = Arc::clone(&self.store);
        let scope = self.scope.clone();
        let run_id = self.run_id.clone();
        let invocation_id = invocation_id.to_string();
        self.block(async move {
            let _ = store.mark_unknown(&scope, &run_id, &invocation_id).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use octos_agent::tools::{IdempotentToolExecutor, Tool, ToolResult};
    use octos_core::execution_scope::{AuthenticatedIdentity, bind_scope};
    use octos_store::repository::local::LocalStore;

    struct CountingTool {
        calls: Arc<std::sync::Mutex<u32>>,
    }
    #[async_trait::async_trait]
    impl Tool for CountingTool {
        fn name(&self) -> &str {
            "side-effecting"
        }
        fn description(&self) -> &str {
            "has an external side effect"
        }
        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn execute(&self, _args: &serde_json::Value) -> eyre::Result<ToolResult> {
            *self.calls.lock().unwrap() += 1;
            Ok(ToolResult {
                output: "external effect applied".into(),
                success: true,
                ..Default::default()
            })
        }
    }

    fn test_scope() -> Scope {
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

    /// K04 end-to-end: an idempotent executor over the durable ledger fires
    /// the side effect ONCE; a "restarted" executor (a NEW executor over the
    /// SAME durable store) reuses the confirmed result without re-firing.
    #[tokio::test(flavor = "multi_thread")]
    async fn durable_ledger_reuses_confirmed_effect_across_restart() {
        let store = Arc::new(LocalStore::default());
        let scope = test_scope();
        let args = serde_json::json!({"command": "charge-card"});

        // First process: execute and confirm.
        let calls1 = Arc::new(std::sync::Mutex::new(0));
        let tool1 = Arc::new(CountingTool {
            calls: Arc::clone(&calls1),
        });
        let ledger1 = Arc::new(DurableSideEffectLedger::new(
            Arc::clone(&store),
            scope.clone(),
            "run-1",
        ));
        let exec1 =
            IdempotentToolExecutor::new(tool1, ledger1 as Arc<dyn SideEffectLedger>, "payment@1");
        let out1 = exec1.execute(&args).await.unwrap();
        assert!(out1.success);
        assert_eq!(*calls1.lock().unwrap(), 1, "first process fired once");

        // "Restart": a brand-new executor over the SAME durable store. The
        // takeover consults the ledger, finds the confirmed effect, and
        // reuses it — the side effect is NOT re-fired.
        let calls2 = Arc::new(std::sync::Mutex::new(0));
        let tool2 = Arc::new(CountingTool {
            calls: Arc::clone(&calls2),
        });
        let ledger2 = Arc::new(DurableSideEffectLedger::new(
            Arc::clone(&store),
            scope.clone(),
            "run-1",
        ));
        let exec2 =
            IdempotentToolExecutor::new(tool2, ledger2 as Arc<dyn SideEffectLedger>, "payment@1");
        let out2 = exec2.execute(&args).await.unwrap();
        assert!(out2.success, "reused confirmed result");
        assert_eq!(*calls2.lock().unwrap(), 0, "no re-fire after restart (K04)");
    }
}

// ---------------------------------------------------------------------------
// CheckpointSource bridge (c3 checkpoint-resume): recover from the durable
// RecoveryStore's committed checkpoints.
// ---------------------------------------------------------------------------

use octos_agent::recovery::{CheckpointSource, RecoveryCheckpoint};

/// A `CheckpointSource` over the durable `RecoveryStore`, scoped to one
/// execution scope. Used by the takeover path to rebuild a run from its
/// latest committed checkpoint (D3) — never from in-process memory.
pub struct DurableCheckpointSource<S: RecoveryStore + 'static> {
    store: Arc<S>,
    scope: Scope,
    rt: tokio::runtime::Handle,
}

impl<S: RecoveryStore + 'static> DurableCheckpointSource<S> {
    pub fn new(store: Arc<S>, scope: Scope) -> Self {
        Self {
            store,
            scope,
            rt: tokio::runtime::Handle::current(),
        }
    }
}

#[async_trait::async_trait]
impl<S: RecoveryStore + 'static> CheckpointSource for DurableCheckpointSource<S> {
    async fn latest(&self, run_id: &str) -> Option<RecoveryCheckpoint> {
        let store = Arc::clone(&self.store);
        let scope = self.scope.clone();
        let run_id = run_id.to_string();
        let rt = self.rt.clone();
        tokio::task::block_in_place(move || {
            rt.block_on(async move {
                store
                    .latest_checkpoint(&scope, &run_id)
                    .await
                    .map(|r| RecoveryCheckpoint {
                        step: r.step,
                        transcript_highwater: r.transcript_highwater,
                        context: r.context,
                        workspace_revision: r.workspace_revision,
                        pending_invocation: r.pending_invocation,
                        binding_digest: r.binding_digest,
                        permission_snapshot: r.permission_snapshot,
                        digest: r.digest,
                        schema_version: r.schema_version,
                        runtime_version: r.runtime_version,
                    })
            })
        })
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use octos_agent::recovery::{RecoveryCheckpoint, checkpoint_digest, recover_run_from};
    use octos_core::execution_scope::{AuthenticatedIdentity, bind_scope};
    use octos_store::repository::local::LocalStore;
    use octos_store::repository::{LeaseStore, NewCheckpoint, RecoveryStore};

    fn test_scope() -> Scope {
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

    /// checkpoint-resume end-to-end: a run checkpoints to the durable store;
    /// after a takeover (new lease epoch), the new worker recovers from the
    /// committed checkpoint via DurableCheckpointSource and resumes after the
    /// checkpoint step with the pinned binding (K11) — the pending invocation
    /// is surfaced for side-effect-ledger consultation (K04).
    #[tokio::test(flavor = "multi_thread")]
    async fn takeover_recovers_from_committed_checkpoint() {
        let store = Arc::new(LocalStore::default());
        let scope = test_scope();

        // Worker-1: claim lease (epoch 1) and commit a checkpoint at step 3
        // with a pending in-flight tool invocation.
        store
            .claim(&scope, "run-1", "worker-1", 60_000, 1_000)
            .await
            .unwrap();
        let cp = RecoveryCheckpoint {
            step: 3,
            transcript_highwater: 30,
            context: Some(serde_json::json!({"summary":"s"})),
            workspace_revision: Some("rev-3".into()),
            pending_invocation: Some("shell@1:abc".into()),
            binding_digest: Some("sha256:binding-v1".into()),
            permission_snapshot: Some(serde_json::json!({"allow":["read"]})),
            digest: String::new(),
            schema_version: "v1".into(),
            runtime_version: "2.0.3".into(),
        };
        let digest = checkpoint_digest(&cp);
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
                digest,
                schema_version: cp.schema_version.clone(),
                runtime_version: cp.runtime_version.clone(),
                created_epoch: 1,
            })
            .await
            .unwrap();

        // Takeover: worker-2 claims with a NEW epoch, then recovers from the
        // durable checkpoint via the CheckpointSource bridge.
        store
            .claim(&scope, "run-1", "worker-2", 60_000, 100_000)
            .await
            .unwrap();
        let source: Arc<dyn CheckpointSource> = Arc::new(DurableCheckpointSource::new(
            Arc::clone(&store),
            scope.clone(),
        ));
        let recovered = recover_run_from(&source, "run-1", "v1")
            .await
            .expect("recover from committed checkpoint");

        assert_eq!(recovered.transcript_highwater, 30);
        assert_eq!(recovered.resume_step, 4);
        assert_eq!(
            recovered.binding_digest.as_deref(),
            Some("sha256:binding-v1")
        );
        assert_eq!(
            recovered.pending_invocation.as_deref(),
            Some("shell@1:abc"),
            "in-flight invocation surfaced for K04 consult"
        );
    }
}
