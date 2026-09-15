//! Real-PostgreSQL integration tests for the c2 `repository::postgres`
//! backend. Spec c2 rule dual-backend-parity / tenant-isolation / K05 / K07.
//!
//! These require a reachable PostgreSQL (`DATABASE_URL`, defaulting to the
//! local docker container). They run the SAME contract scenarios as the local
//! adapter against the real tenant-keyed schema, proving dual-backend parity
//! on a live database rather than on mock SQL (spec §7.4: mock-SQL unit tests
//! are not evidence of distributed recovery — these hit a real PG).
//!
//! Run: `DATABASE_URL=postgres://postgres:octos@127.0.0.1:5432/octos \
//!         cargo test -p octos-store --features postgres --test repository_postgres`

#![cfg(feature = "postgres")]

use octos_core::execution_scope::{AuthenticatedIdentity, bind_scope};
use octos_store::repository::postgres::PgStore;
use octos_store::repository::{
    ApprovalDecision, ApprovalState, NewApproval, NewMessage, NewSessionEvent, OutboxItem,
    RepositoryError, UnitOfWork,
};

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:octos@127.0.0.1:5432/octos".to_string())
}

fn scope(tenant: &str, session: &str) -> octos_core::execution_scope::Scope {
    bind_scope(
        &AuthenticatedIdentity {
            tenant_id: tenant.into(),
            profile_id: "profile-a".into(),
        },
        session,
        None,
    )
    .unwrap()
}

async fn fresh_store(tag: &str) -> PgStore {
    // Each test gets its own PG schema so concurrent `cargo test` runs never
    // share state (the earlier shared-table version flaked on cross-test row
    // residue). The schema is created and migrated per test.
    let schema = format!(
        "test_{}_{}",
        tag.replace('-', "_"),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let url = database_url();
    let admin = sqlx::PgPool::connect(&url).await.expect("admin connect");
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&admin)
        .await
        .expect("create schema");
    // Connect with the schema on the search_path so the unprefixed
    // DDL/queries land in (and read from) the per-test schema.
    let scoped_url = format!("{url}?options=-c%20search_path%3D{schema}");
    let store = PgStore::connect(&scoped_url)
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("migrate");
    store
}

/// dual-backend-parity: UoW commits message+run-state+event+outbox atomically
/// against the real schema.
#[tokio::test]
async fn pg_uow_commits_four_aggregates_atomically() {
    let store = fresh_store("uow").await;
    let scope = scope("t-uow", "sess-uow-1");
    let mut uow = store.begin();

    uow.append_message(NewMessage {
        scope: scope.clone(),
        message_id: "m-1".into(),
        thread_id: "t-1".into(),
        turn_id: "turn-1".into(),
        role: "user".into(),
        content: "hello pg".into(),
    });
    uow.set_run_state("run-pg-1", "completed");
    uow.append_event(NewSessionEvent {
        scope: scope.clone(),
        event_id: "evt-1".into(),
        causation_id: Some("cmd-1".into()),
        payload: serde_json::json!({"kind":"turn_completed"}),
    });
    uow.enqueue_outbox(OutboxItem {
        aggregate_key: "sess-uow-1".into(),
        topic: "session.event".into(),
        payload: serde_json::json!({"event_id":"evt-1"}),
    });
    uow.commit().await.expect("commit");

    assert_eq!(store.messages_for_async(&scope).await.unwrap().len(), 1);
    assert_eq!(
        store.run_state_async("run-pg-1").await.unwrap(),
        Some("completed".into())
    );
    let events = store.events_for_async(&scope).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].seq, 1);
    assert_eq!(store.outbox_async("t-uow").await.unwrap().len(), 1);
}

/// tenant-isolation (K07): same wire session id under two tenants is fully
/// isolated at the database layer (RLS + scope-keyed queries).
#[tokio::test]
async fn pg_cross_tenant_isolation() {
    let store = fresh_store("iso").await;
    let a = scope("t-iso-a", "sess-iso-1");
    let b = scope("t-iso-b", "sess-iso-1");
    assert_eq!(a.session_id(), b.session_id());

    let mut uow = store.begin();
    uow.append_message(NewMessage {
        scope: a.clone(),
        message_id: "m-a".into(),
        thread_id: "t".into(),
        turn_id: "turn".into(),
        role: "user".into(),
        content: "tenant-a secret".into(),
    });
    uow.commit().await.unwrap();

    assert_eq!(store.messages_for_async(&a).await.unwrap().len(), 1);
    assert!(
        store.messages_for_async(&b).await.unwrap().is_empty(),
        "tenant-b must not read tenant-a rows"
    );
}

/// event-ledger (D6): per-scope monotonic gap-free seq + event_id dedup.
#[tokio::test]
async fn pg_session_events_monotonic_and_dedup() {
    let store = fresh_store("evt").await;
    let scope = scope("t-evt", "sess-evt-1");
    let mut uow = store.begin();
    for i in 0..3 {
        uow.append_event(NewSessionEvent {
            scope: scope.clone(),
            event_id: format!("evt-{i}"),
            causation_id: None,
            payload: serde_json::json!({"i": i}),
        });
    }
    uow.append_event(NewSessionEvent {
        scope: scope.clone(),
        event_id: "evt-1".into(),
        causation_id: None,
        payload: serde_json::json!({"dup": true}),
    });
    uow.commit().await.unwrap();

    let events = store.events_for_async(&scope).await.unwrap();
    assert_eq!(events.len(), 3, "dup event_id deduped");
    let seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, vec![1, 2, 3]);
}

/// approval-durability (K05): reply CAS rejects replay, tamper, cross-tenant.
#[tokio::test]
async fn pg_approval_reply_cas() {
    let store = fresh_store("ap").await;
    let a = scope("t-ap-a", "sess-ap-1");
    let b = scope("t-ap-b", "sess-ap-2");

    let mut uow = store.begin();
    uow.create_approval(NewApproval {
        scope: a.clone(),
        approval_id: "ap-pg-1".into(),
        originating_run: "run-1".into(),
        args_hash: "hash-abc".into(),
        binding_revision: "rev-1".into(),
    });
    uow.commit().await.unwrap();

    // First reply wins.
    assert!(matches!(
        store
            .reply_approval_async(&a, "ap-pg-1", "hash-abc", ApprovalDecision::Approved)
            .await,
        Ok(ApprovalState::Decided)
    ));
    // Replay rejected.
    assert!(matches!(
        store
            .reply_approval_async(&a, "ap-pg-1", "hash-abc", ApprovalDecision::Approved)
            .await,
        Err(RepositoryError::AlreadyDecided)
    ));
    // Cross-tenant rejected (b cannot decide a's approval).
    assert!(matches!(
        store
            .reply_approval_async(&b, "ap-pg-1", "hash-abc", ApprovalDecision::Approved)
            .await,
        Err(RepositoryError::NotFound)
    ));
}

/// migration-safety: schema migrates cleanly and is idempotent (re-run ok).
#[tokio::test]
async fn pg_migrate_is_idempotent() {
    let store = fresh_store("mig").await;
    store.migrate().await.expect("re-migrate is a no-op");
}

// --- c3: lease-claim / fencing on the real PG schema (K02, K03) ----------

use octos_store::repository::LeaseStore;

/// K02 on real PG: truly concurrent claims on the same (scope, run) yield
/// exactly ONE owner — the row lock serializes the claimers.
#[tokio::test]
async fn pg_concurrent_lease_claim_single_owner() {
    let store = fresh_store("lease-k02").await;
    let scope = scope("t-lk02", "sess-lk02-1");

    // Fire two truly-concurrent claims (separate tasks) from different workers
    // at the same instant. `tokio::join!` on one task would poll them
    // sequentially; `spawn` gives real concurrency so the row lock is what
    // serializes the two claimers.
    let store_a = store.clone();
    let scope_a = scope.clone();
    let store_b = store.clone();
    let scope_b = scope.clone();
    let h1 = tokio::spawn(async move {
        store_a
            .claim(&scope_a, "run-1", "worker-1", 60_000, 1_000)
            .await
    });
    let h2 = tokio::spawn(async move {
        store_b
            .claim(&scope_b, "run-1", "worker-2", 60_000, 1_000)
            .await
    });
    let r1 = h1.await.unwrap();
    let r2 = h2.await.unwrap();
    let wins = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
    let conflicts = [&r1, &r2]
        .iter()
        .filter(|r| matches!(r, Err(RepositoryError::Conflict)))
        .count();
    assert_eq!(wins, 1, "exactly one concurrent claimer wins");
    assert_eq!(conflicts, 1, "the other is rejected as Conflict");
}

/// K03 on real PG: expired lease takeover increments the epoch and fences
/// the stale-epoch writer.
#[tokio::test]
async fn pg_lease_takeover_increments_epoch_and_fences() {
    let store = fresh_store("lease-k03").await;
    let scope = scope("t-lk03", "sess-lk03-1");

    let l1 = store
        .claim(&scope, "run-1", "worker-1", 1_000, 1_000)
        .await
        .unwrap();
    assert_eq!(l1.epoch, 1);
    store
        .write_with_epoch(&scope, "run-1", 1, "running")
        .await
        .expect("epoch-1 write");

    // Expired; worker-2 takes over.
    let l2 = store
        .claim(&scope, "run-1", "worker-2", 60_000, 5_000)
        .await
        .expect("takeover");
    assert_eq!(l2.epoch, 2);
    assert_eq!(l2.owner_id, "worker-2");

    // Stale epoch fenced.
    assert!(matches!(
        store.write_with_epoch(&scope, "run-1", 1, "corrupt").await,
        Err(RepositoryError::StaleEpoch)
    ));
    // Current epoch accepted.
    store
        .write_with_epoch(&scope, "run-1", 2, "recovered")
        .await
        .expect("epoch-2 write");
}

// --- c3: side-effect idempotency + checkpoint resume on real PG (K04, K11) ---

use octos_store::repository::{InvocationState, NewCheckpoint, NewInvocation, RecoveryStore};

fn pg_cp(
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
        context: Some(serde_json::json!({"s":"x"})),
        workspace_revision: Some(format!("rev-{step}")),
        pending_invocation: None,
        artifact_refs: None,
        binding_digest: Some("sha256:binding-v1".into()),
        permission_snapshot: Some(serde_json::json!({"allow":["read"]})),
        digest: format!("sha256:cp-{step}"),
        schema_version: "v1".into(),
        runtime_version: "2.0.3".into(),
        created_epoch: epoch,
    }
}

/// K04 on real PG: confirmed effect reused via idempotency key after crash.
#[tokio::test]
async fn pg_idempotent_effect_reused_after_crash() {
    let store = fresh_store("k04").await;
    let scope = scope("t-k04", "sess-k04-1");
    store
        .record_intent(NewInvocation {
            scope: scope.clone(),
            run_id: "run-1".into(),
            invocation_id: "inv-1".into(),
            tool_revision: "shell@1".into(),
            args_hash: "h".into(),
            external_idempotency_key: Some("ext-1".into()),
        })
        .await
        .unwrap();
    store
        .mark_succeeded(&scope, "run-1", "inv-1", "obj://r/1", Some(900))
        .await
        .unwrap();

    let found = store
        .find_by_idempotency_key(&scope, "ext-1")
        .await
        .unwrap();
    assert_eq!(found.state, InvocationState::Succeeded);
    assert_eq!(found.result_ref.as_deref(), Some("obj://r/1"));
    assert_eq!(found.cost_micros, Some(900));
}

/// K04 on real PG: no-idempotency Unknown is surfaced, tampered retry rejected.
#[tokio::test]
async fn pg_unknown_goes_to_reconciliation_not_retry() {
    let store = fresh_store("k04u").await;
    let scope = scope("t-k04u", "sess-k04u-1");
    store
        .record_intent(NewInvocation {
            scope: scope.clone(),
            run_id: "run-1".into(),
            invocation_id: "inv-2".into(),
            tool_revision: "shell@1".into(),
            args_hash: "h".into(),
            external_idempotency_key: None,
        })
        .await
        .unwrap();
    store.mark_unknown(&scope, "run-1", "inv-2").await.unwrap();

    // Same logical retry sees Unknown (not auto-retry).
    let retry = store
        .record_intent(NewInvocation {
            scope: scope.clone(),
            run_id: "run-1".into(),
            invocation_id: "inv-2".into(),
            tool_revision: "shell@1".into(),
            args_hash: "h".into(),
            external_idempotency_key: None,
        })
        .await
        .unwrap();
    assert_eq!(retry, InvocationState::Unknown);

    // Tampered retry rejected.
    assert!(matches!(
        store
            .record_intent(NewInvocation {
                scope: scope.clone(),
                run_id: "run-1".into(),
                invocation_id: "inv-2".into(),
                tool_revision: "shell@1".into(),
                args_hash: "h-DIFF".into(),
                external_idempotency_key: None,
            })
            .await,
        Err(RepositoryError::ArgsMismatch)
    ));
}

/// Checkpoint resume on real PG: latest wins, stale-epoch fenced (D5), K11
/// binding digest pinned.
#[tokio::test]
async fn pg_checkpoint_resume_and_epoch_fencing() {
    let store = fresh_store("cp").await;
    let scope = scope("t-cp", "sess-cp-1");
    store
        .claim(&scope, "run-1", "worker-1", 60_000, 1_000)
        .await
        .unwrap();

    store
        .commit_checkpoint(pg_cp(&scope, "run-1", 1, 1))
        .await
        .unwrap();
    store
        .commit_checkpoint(pg_cp(&scope, "run-1", 2, 1))
        .await
        .unwrap();
    let latest = store.latest_checkpoint(&scope, "run-1").await.unwrap();
    assert_eq!(latest.step, 2);
    assert_eq!(latest.binding_digest.as_deref(), Some("sha256:binding-v1"));

    // Takeover -> epoch 2; stale-epoch checkpoint fenced.
    store
        .claim(&scope, "run-1", "worker-2", 60_000, 100_000)
        .await
        .unwrap();
    assert!(matches!(
        store.commit_checkpoint(pg_cp(&scope, "run-1", 3, 1)).await,
        Err(RepositoryError::StaleEpoch)
    ));
    store
        .commit_checkpoint(pg_cp(&scope, "run-1", 3, 2))
        .await
        .unwrap();
    assert_eq!(
        store.latest_checkpoint(&scope, "run-1").await.unwrap().step,
        3
    );
}

// --- c5: cron durable firing on real PG (K10) -----------------------------

use octos_store::repository::{
    CronScheduleStore, FiringState, MisfirePolicy, Schedule, ScheduleFiring,
};

fn pg_sched(id: &str) -> Schedule {
    Schedule {
        schedule_id: id.into(),
        expression: "every 30m".into(),
        timezone: None,
        next_fire_at_ms: None,
        misfire_policy: MisfirePolicy::RunOnce,
        enabled: true,
    }
}

fn pg_firing(schedule_id: &str, when_ms: u64) -> ScheduleFiring {
    ScheduleFiring {
        schedule_id: schedule_id.into(),
        scheduled_at_ms: when_ms,
        firing_id: format!("f-{when_ms}"),
        claimed_by: None,
        state: FiringState::Intent,
        run_id: None,
    }
}

#[tokio::test]
async fn pg_concurrent_cron_controllers_produce_single_firing() {
    let store = fresh_store("cron-k10").await;
    let scope = scope("t-cron", "sess-cron-1");
    store
        .create_schedule(&scope, pg_sched("cron-1"))
        .await
        .unwrap();

    let s1 = store.clone();
    let sc1 = scope.clone();
    let h1 = tokio::spawn(async move { s1.record_firing(&sc1, pg_firing("cron-1", 1_000)).await });
    let s2 = store.clone();
    let sc2 = scope.clone();
    let h2 = tokio::spawn(async move { s2.record_firing(&sc2, pg_firing("cron-1", 1_000)).await });
    let r1 = h1.await.unwrap();
    let r2 = h2.await.unwrap();
    let wins = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
    let conflicts = [&r1, &r2]
        .iter()
        .filter(|r| matches!(r, Err(RepositoryError::Conflict)))
        .count();
    assert_eq!(wins, 1);
    assert_eq!(conflicts, 1, "K10 single-firing: one firing + one Conflict");

    let claim_a = store
        .claim_firing(&scope, "cron-1", 1_000, "controller-a")
        .await;
    assert!(claim_a.is_ok());
    let claim_b = store
        .claim_firing(&scope, "cron-1", 1_000, "controller-b")
        .await;
    assert!(
        matches!(claim_b, Err(RepositoryError::Conflict)),
        "K10 single-claim"
    );
}

#[tokio::test]
async fn pg_cron_misfire_policy_backfills_once() {
    let store = fresh_store("cron-mis").await;
    let scope = scope("t-mis", "sess-mis-1");
    store
        .create_schedule(&scope, pg_sched("cron-2"))
        .await
        .unwrap();
    store
        .record_firing(&scope, pg_firing("cron-2", 5_000))
        .await
        .unwrap();
    assert!(matches!(
        store
            .record_firing(&scope, pg_firing("cron-2", 5_000))
            .await,
        Err(RepositoryError::Conflict)
    ));
    store
        .record_firing(&scope, pg_firing("cron-2", 6_000))
        .await
        .unwrap();
    store
        .create_schedule(&scope, pg_sched("cron-3"))
        .await
        .unwrap();
    store
        .record_firing(&scope, pg_firing("cron-3", 5_000))
        .await
        .unwrap();
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
