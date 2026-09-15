//! RED tests for the c2 persistence-boundary repository contract.
//!
//! Spec: specs/task-c2-persistence-boundary-postgres.spec.md
//! ADR: docs/adr/cluster-state-and-execution.md (D1, D6, D8)
//!
//! These tests pin the shared repository + UnitOfWork contract that BOTH the
//! local file adapter and the PostgreSQL backend must satisfy. They compile
//! against `octos_store::repository`, which does not exist yet (RED).

use octos_core::execution_scope::{AuthenticatedIdentity, bind_scope};
use octos_store::repository::{
    ApprovalDecision, ApprovalRecord, ApprovalState, NewApproval, NewMessage, NewSessionEvent,
    OutboxItem, RepositoryError, SessionEvent, StoreView, UnitOfWork,
};

fn scope_a() -> octos_core::execution_scope::Scope {
    bind_scope(
        &AuthenticatedIdentity {
            tenant_id: "tenant-a".into(),
            profile_id: "profile-a".into(),
        },
        "sess-1",
        None,
    )
    .unwrap()
}

fn scope_b() -> octos_core::execution_scope::Scope {
    bind_scope(
        &AuthenticatedIdentity {
            tenant_id: "tenant-b".into(),
            profile_id: "profile-a".into(),
        },
        "sess-1",
        None,
    )
    .unwrap()
}

/// Rule uow-atomicity: 消息+运行状态+事件+outbox 同事务。
/// A UoW groups message + run-state + event + outbox mutations and commits
/// them atomically; a failure before commit leaves nothing visible.
#[tokio::test]
async fn uow_commits_message_runstate_event_outbox_atomically() {
    let mut uow = octos_store::repository::local::LocalUnitOfWork::begin();
    let scope = scope_a();

    uow.append_message(NewMessage {
        scope: scope.clone(),
        message_id: "m-1".into(),
        thread_id: "t-1".into(),
        turn_id: "turn-1".into(),
        role: "user".into(),
        content: "hello".into(),
    });
    uow.set_run_state("run-1", "completed");
    uow.append_event(NewSessionEvent {
        scope: scope.clone(),
        event_id: "evt-1".into(),
        causation_id: Some("cmd-1".into()),
        payload: serde_json::json!({"kind":"turn_completed"}),
    });
    uow.enqueue_outbox(OutboxItem {
        aggregate_key: "sess-1".into(),
        topic: "session.event".into(),
        payload: serde_json::json!({"event_id":"evt-1"}),
    });

    uow.commit().await.expect("commit succeeds");

    // After commit, all four aggregates are visible.
    let msgs = uow.committed_messages(&scope);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].message_id, "m-1");
    assert_eq!(uow.committed_run_state("run-1"), Some("completed".into()));
    let events = uow.committed_events(&scope);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].seq, 1, "first event in scope gets seq 1");
    assert_eq!(uow.committed_outbox().len(), 1);
}

/// Rule uow-atomicity: 事务失败不发布内存结果。
/// A UoW that is dropped without commit publishes nothing.
#[tokio::test]
async fn failed_transaction_does_not_publish_in_memory_success() {
    let scope = scope_a();
    let committed;
    {
        let mut uow = octos_store::repository::local::LocalUnitOfWork::begin();
        uow.append_message(NewMessage {
            scope: scope.clone(),
            message_id: "m-x".into(),
            thread_id: "t-1".into(),
            turn_id: "turn-1".into(),
            role: "user".into(),
            content: "lost".into(),
        });
        // Drop without commit.
        committed = uow.shared_store();
    }
    assert!(
        committed.messages_for(&scope).is_empty(),
        "uncommitted message must not be visible"
    );
}

/// Rule tenant-isolation: 跨租户查询不命中 (K07)。
/// Messages written under tenant-a's scope are invisible to tenant-b's scope
/// even when both use the same wire session id.
#[tokio::test]
async fn cross_tenant_queries_return_no_rows() {
    let a = scope_a();
    let b = scope_b();
    assert_eq!(a.session_id(), b.session_id(), "same wire session id");
    assert_ne!(a, b, "different scopes");

    let mut uow = octos_store::repository::local::LocalUnitOfWork::begin();
    uow.append_message(NewMessage {
        scope: a.clone(),
        message_id: "m-a".into(),
        thread_id: "t".into(),
        turn_id: "turn".into(),
        role: "user".into(),
        content: "tenant-a secret".into(),
    });
    uow.commit().await.unwrap();

    let store = uow.shared_store();
    assert_eq!(store.messages_for(&a).len(), 1);
    assert!(
        store.messages_for(&b).is_empty(),
        "tenant-b must not see tenant-a messages"
    );
}

/// Rule event-ledger: 事件按 Scope 单调 seq 且去重。
#[tokio::test]
async fn session_events_monotonic_seq_and_idempotent_event_id() {
    let scope = scope_a();
    let mut uow = octos_store::repository::local::LocalUnitOfWork::begin();
    for i in 0..3 {
        uow.append_event(NewSessionEvent {
            scope: scope.clone(),
            event_id: format!("evt-{i}"),
            causation_id: None,
            payload: serde_json::json!({"i": i}),
        });
    }
    // Duplicate event_id must be deduped, not double-appended.
    uow.append_event(NewSessionEvent {
        scope: scope.clone(),
        event_id: "evt-1".into(),
        causation_id: None,
        payload: serde_json::json!({"i": 1, "dup": true}),
    });
    uow.commit().await.unwrap();

    let events = uow.committed_events(&scope);
    assert_eq!(events.len(), 3, "duplicate event_id deduped");
    let seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, vec![1, 2, 3], "monotonic gap-free seq per scope");
}

/// Rule approval-durability: reply CAS 拒绝重复/跨租户/参数变化 (K05)。
#[tokio::test]
async fn approval_reply_cas_rejects_replay_and_tamper() {
    let a = scope_a();
    let mut uow = octos_store::repository::local::LocalUnitOfWork::begin();
    uow.create_approval(NewApproval {
        scope: a.clone(),
        approval_id: "ap-1".into(),
        originating_run: "run-1".into(),
        args_hash: "hash-abc".into(),
        binding_revision: "rev-1".into(),
    });
    uow.commit().await.unwrap();

    let store = uow.shared_store();

    // First reply wins.
    let first = store.reply_approval(&a, "ap-1", "hash-abc", ApprovalDecision::Approved);
    assert!(matches!(first, Ok(ApprovalState::Decided)));

    // Replay (same decision) is rejected as already-decided.
    let replay = store.reply_approval(&a, "ap-1", "hash-abc", ApprovalDecision::Approved);
    assert!(matches!(replay, Err(RepositoryError::AlreadyDecided)));

    // Tampered args_hash is rejected.
    let b = scope_b();
    let mut uow2 = octos_store::repository::local::LocalUnitOfWork::begin();
    uow2.create_approval(NewApproval {
        scope: b.clone(),
        approval_id: "ap-2".into(),
        originating_run: "run-2".into(),
        args_hash: "hash-abc".into(),
        binding_revision: "rev-1".into(),
    });
    uow2.commit().await.unwrap();
    let store2 = uow2.shared_store();
    let tampered = store2.reply_approval(&b, "ap-2", "hash-DIFFERENT", ApprovalDecision::Approved);
    assert!(matches!(tampered, Err(RepositoryError::ArgsMismatch)));

    // Cross-tenant reply (scope_a cannot decide scope_b's approval).
    let cross = store2.reply_approval(&a, "ap-2", "hash-abc", ApprovalDecision::Approved);
    assert!(matches!(cross, Err(RepositoryError::NotFound)));
}

// Silence unused-import warnings for the contract surface these tests pin.
#[allow(dead_code)]
fn _contract_surface(
    _m: NewMessage,
    _e: NewSessionEvent,
    _o: OutboxItem,
    _s: SessionEvent,
    _a: ApprovalRecord,
) {
}

// --- c3: lease-claim / fencing (K02, K03) --------------------------------
//
// These run against BOTH backends: the local adapter below, and the real
// PostgreSQL backend in tests/repository_postgres.rs (same scenarios).

use octos_store::repository::LeaseStore;
use octos_store::repository::local::LocalStore;

/// K02: concurrent claims on the SAME scope yield exactly one owner; a
/// different scope is claimed independently.
#[tokio::test]
async fn concurrent_lease_claim_yields_single_owner() {
    let store = Arc::new(LocalStore::default());
    let scope = scope_a();

    let owner1 = store
        .claim(&scope, "run-1", "worker-1", 10_000, 1_000)
        .await;
    assert!(owner1.is_ok(), "first claim wins");
    let lease1 = owner1.unwrap();
    assert_eq!(lease1.epoch, 1);
    assert_eq!(lease1.owner_id, "worker-1");

    // A different worker's claim on the same run, while the lease is live,
    // is rejected (single owner).
    let owner2 = store
        .claim(&scope, "run-1", "worker-2", 10_000, 1_500)
        .await;
    assert!(matches!(owner2, Err(RepositoryError::Conflict)));

    // Same owner renews successfully, epoch unchanged.
    let renew = store
        .claim(&scope, "run-1", "worker-1", 10_000, 2_000)
        .await;
    assert!(matches!(renew, Ok(l) if l.epoch == 1 && l.owner_id == "worker-1"));

    // A DIFFERENT scope's run is claimed independently (no false coupling).
    let scope_b = scope_b();
    let other = store
        .claim(&scope_b, "run-1", "worker-2", 10_000, 2_000)
        .await;
    assert!(other.is_ok(), "different scope claims independently");
}

/// K03: an expired lease is taken over with epoch+1, and the OLD owner's
/// stale-epoch write is fenced out.
#[tokio::test]
async fn expired_lease_takeover_increments_epoch_and_fences_stale_writer() {
    let store = Arc::new(LocalStore::default());
    let scope = scope_a();

    // worker-1 claims with a short lease.
    let lease1 = store
        .claim(&scope, "run-1", "worker-1", 1_000, 1_000)
        .await
        .unwrap();
    assert_eq!(lease1.epoch, 1);
    // worker-1 writes at epoch 1 — accepted.
    store
        .write_with_epoch(&scope, "run-1", 1, "running")
        .await
        .expect("epoch-1 write accepted");

    // Lease expires; worker-2 takes over, epoch increments to 2.
    let lease2 = store
        .claim(&scope, "run-1", "worker-2", 10_000, 5_000)
        .await
        .expect("takeover of expired lease");
    assert_eq!(lease2.epoch, 2);
    assert_eq!(lease2.owner_id, "worker-2");

    // The partitioned OLD owner (worker-1) tries to write with its stale
    // epoch 1 — fenced out (K03).
    let stale = store
        .write_with_epoch(&scope, "run-1", 1, "corrupted")
        .await;
    assert!(matches!(stale, Err(RepositoryError::StaleEpoch)));

    // The new owner writes at epoch 2 — accepted.
    store
        .write_with_epoch(&scope, "run-1", 2, "recovered")
        .await
        .expect("epoch-2 write accepted");
}

// --- c3: side-effect idempotency + checkpoint resume (K04, K11) ----------

use octos_store::repository::{InvocationState, NewCheckpoint, NewInvocation, RecoveryStore};

fn cp(
    scope: &octos_core::execution_scope::Scope,
    run: &str,
    step: u64,
    epoch: u64,
) -> NewCheckpoint {
    NewCheckpoint {
        scope: scope.clone(),
        run_id: run.into(),
        step,
        transcript_highwater: step * 10,
        context: Some(serde_json::json!({"summary": "..."})),
        workspace_revision: Some(format!("rev-{step}")),
        pending_invocation: None,
        artifact_refs: None,
        binding_digest: Some("sha256:binding-v1".into()),
        permission_snapshot: Some(serde_json::json!({"allow": ["read"]})),
        digest: format!("sha256:cp-{step}"),
        schema_version: "v1".into(),
        runtime_version: "2.0.3".into(),
        created_epoch: epoch,
    }
}

/// K04: a tool side effect is recorded as an intent; a post-crash takeover
/// reuses the confirmed result via the idempotency key instead of re-firing.
#[tokio::test]
async fn external_success_then_crash_reuses_confirmed_result_not_duplicate() {
    let store = LocalStore::default();
    let scope = scope_a();

    // Worker-1: record intent, external effect succeeds, mark succeeded.
    let inv = NewInvocation {
        scope: scope.clone(),
        run_id: "run-1".into(),
        invocation_id: "inv-1".into(),
        tool_revision: "shell@1".into(),
        args_hash: "hash-cmd".into(),
        external_idempotency_key: Some("ext-key-1".into()),
    };
    assert_eq!(
        store.record_intent(inv).await.unwrap(),
        InvocationState::Intent
    );
    store
        .mark_succeeded(&scope, "run-1", "inv-1", "obj://result/1", Some(1200))
        .await
        .unwrap();

    // Worker-1 is killed. A takeover looks up the idempotency key and finds
    // the confirmed effect — it does NOT re-fire.
    let found = store
        .find_by_idempotency_key(&scope, "ext-key-1")
        .await
        .expect("idempotency lookup hits");
    assert_eq!(found.state, InvocationState::Succeeded);
    assert_eq!(found.result_ref.as_deref(), Some("obj://result/1"));
    assert_eq!(found.cost_micros, Some(1200), "cost recorded once (K09)");
}

/// K04: an invocation with NO idempotency capability whose result was not
/// persisted is marked Unknown and routed to reconciliation — NOT re-fired.
#[tokio::test]
async fn unknown_side_effect_goes_to_reconciliation_not_retry() {
    let store = LocalStore::default();
    let scope = scope_a();
    let inv = NewInvocation {
        scope: scope.clone(),
        run_id: "run-1".into(),
        invocation_id: "inv-2".into(),
        tool_revision: "shell@1".into(),
        args_hash: "hash-cmd".into(),
        external_idempotency_key: None, // no idempotency capability
    };
    store.record_intent(inv).await.unwrap();
    // Worker killed mid-effect; result never persisted.
    store.mark_unknown(&scope, "run-1", "inv-2").await.unwrap();

    // A takeover re-recording the same logical intent sees Unknown and must
    // NOT auto-retry.
    let retry = NewInvocation {
        scope: scope.clone(),
        run_id: "run-1".into(),
        invocation_id: "inv-2".into(),
        tool_revision: "shell@1".into(),
        args_hash: "hash-cmd".into(),
        external_idempotency_key: None,
    };
    assert_eq!(
        store.record_intent(retry).await.unwrap(),
        InvocationState::Unknown,
        "Unknown is surfaced for reconciliation, not retried"
    );

    // A tampered retry (different args on the same id) is rejected.
    let tampered = NewInvocation {
        scope: scope.clone(),
        run_id: "run-1".into(),
        invocation_id: "inv-2".into(),
        tool_revision: "shell@1".into(),
        args_hash: "hash-DIFFERENT".into(),
        external_idempotency_key: None,
    };
    assert!(matches!(
        store.record_intent(tampered).await,
        Err(RepositoryError::ArgsMismatch)
    ));
}

/// Checkpoint resume (D3): the latest committed checkpoint is returned and a
/// stale-epoch write is fenced (D5). K11: the checkpoint pins the binding
/// digest/permission snapshot the run started under.
#[tokio::test]
async fn checkpoint_resume_uses_latest_and_fences_stale_epoch() {
    use octos_store::repository::LeaseStore;
    let store = LocalStore::default();
    let scope = scope_a();

    // Claim a lease so checkpoints can be written under epoch 1.
    store
        .claim(&scope, "run-1", "worker-1", 60_000, 1_000)
        .await
        .unwrap();

    store
        .commit_checkpoint(cp(&scope, "run-1", 1, 1))
        .await
        .unwrap();
    store
        .commit_checkpoint(cp(&scope, "run-1", 2, 1))
        .await
        .unwrap();

    let latest = store.latest_checkpoint(&scope, "run-1").await.unwrap();
    assert_eq!(latest.step, 2, "latest checkpoint wins");
    assert_eq!(latest.transcript_highwater, 20);
    assert_eq!(latest.binding_digest.as_deref(), Some("sha256:binding-v1"));

    // Takeover increments the epoch to 2; a checkpoint written under the OLD
    // epoch 1 is fenced out (D5/K03 at the checkpoint boundary).
    store
        .claim(&scope, "run-1", "worker-2", 60_000, 100_000)
        .await
        .unwrap();
    let stale = store.commit_checkpoint(cp(&scope, "run-1", 3, 1)).await;
    assert!(matches!(stale, Err(RepositoryError::StaleEpoch)));

    // The new epoch's checkpoint is accepted and becomes latest.
    store
        .commit_checkpoint(cp(&scope, "run-1", 3, 2))
        .await
        .unwrap();
    let latest = store.latest_checkpoint(&scope, "run-1").await.unwrap();
    assert_eq!(latest.step, 3);
}

// --- c5: cron durable firing (K10) ----------------------------------------
//
// Two Cron Controllers scanning concurrently at the same instant must yield
// exactly one firing and one run; the (scope, schedule_id, scheduled_at)
// PRIMARY KEY makes the loser's INSERT fail with Conflict. A misfire
// policy fills the gap after a Controller lapses.

use octos_store::repository::{
    CronScheduleStore, FiringState, MisfirePolicy, Schedule, ScheduleFiring,
};

use std::sync::Arc;
fn sched(id: &str) -> Schedule {
    Schedule {
        schedule_id: id.into(),
        expression: "every 30m".into(),
        timezone: None,
        next_fire_at_ms: None,
        misfire_policy: MisfirePolicy::RunOnce,
        enabled: true,
        last_fired_at_ms: None,
        last_run_id: None,
        name: id.into(),
        payload_json: "{}".into(),
        delete_after_run: false,
        origin_json: "".into(),
        created_at_ms: 0,
    }
}

fn firing(schedule_id: &str, when_ms: u64) -> ScheduleFiring {
    ScheduleFiring {
        schedule_id: schedule_id.into(),
        scheduled_at_ms: when_ms,
        firing_id: format!("f-{}", when_ms),
        claimed_by: None,
        state: FiringState::Intent,
        run_id: None,
    }
}

/// K10 single-firing: two concurrent record_firing for the same instant
/// yield exactly one stored record; the other is rejected as Conflict.
#[tokio::test]
async fn concurrent_cron_controllers_produce_single_firing() {
    let store = Arc::new(LocalStore::default());
    let scope = scope_a();
    store
        .create_schedule(&scope, sched("cron-1"))
        .await
        .unwrap();

    let s1 = store.clone();
    let sc1 = scope.clone();
    let h1 = tokio::spawn(async move { s1.record_firing(&sc1, firing("cron-1", 1_000)).await });
    let s2 = store.clone();
    let sc2 = scope.clone();
    let h2 = tokio::spawn(async move { s2.record_firing(&sc2, firing("cron-1", 1_000)).await });
    let r1 = h1.await.unwrap();
    let r2 = h2.await.unwrap();
    let wins = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
    let conflicts = [&r1, &r2]
        .iter()
        .filter(|r| matches!(r, Err(RepositoryError::Conflict)))
        .count();
    assert_eq!(wins, 1, "exactly one Controller records the firing");
    assert_eq!(conflicts, 1, "the other is rejected with Conflict (K10)");

    // K10 single-claim: only one Controller claims the firing.
    let claim_a = store
        .claim_firing(&scope, "cron-1", 1_000, "controller-a")
        .await;
    assert!(claim_a.is_ok());
    let claim_b = store
        .claim_firing(&scope, "cron-1", 1_000, "controller-b")
        .await;
    assert!(matches!(claim_b, Err(RepositoryError::Conflict)));
}

/// K10 misfire: a different firing instant is allowed; same instant
/// distinct schedules are independent.
#[tokio::test]
async fn cron_misfire_policy_backfills_once() {
    let store = LocalStore::default();
    let scope = scope_a();
    store
        .create_schedule(&scope, sched("cron-2"))
        .await
        .unwrap();

    // A controller lapsed; record the missed instant (run_once policy
    // means we record it once; a second attempt at the same instant
    // is rejected).
    store
        .record_firing(&scope, firing("cron-2", 5_000))
        .await
        .unwrap();
    let dup = store.record_firing(&scope, firing("cron-2", 5_000)).await;
    assert!(matches!(dup, Err(RepositoryError::Conflict)));

    // A different instant is a fresh firing — no double-firing, no
    // accidental skip.
    store
        .record_firing(&scope, firing("cron-2", 6_000))
        .await
        .unwrap();

    // Distinct schedules share no state: independent firings.
    store
        .create_schedule(&scope, sched("cron-3"))
        .await
        .unwrap();
    store
        .record_firing(&scope, firing("cron-3", 5_000))
        .await
        .unwrap();

    // Mark the first firing terminal — K10 + spec rule
    // "concurrent_cron_controllers_produce_single_firing".
    store
        .mark_firing_terminal(
            &scope,
            "cron-2",
            5_000,
            FiringState::Succeeded,
            Some("run-1"),
        )
        .await
        .unwrap();
}
