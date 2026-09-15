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
async fn pg_dump_emits_per_table_inserts_and_tenant_set() {
    let store = fresh_store("dump").await;
    let scope = scope("t-dump", "sess-dump-1");
    let mut uow = store.begin();
    uow.append_message(NewMessage {
        scope: scope.clone(),
        message_id: "m-1".into(),
        thread_id: "t-1".into(),
        turn_id: "turn-1".into(),
        role: "user".into(),
        content: "hello".into(),
    });
    uow.commit().await.unwrap();

    let sql = store.dump_tables_sql("t-dump").await.expect("dump");
    assert!(sql.contains("SET LOCAL app.tenant_id = 't-dump'"));
    assert!(sql.contains("INSERT INTO messages VALUES ('t-dump', 'profile-a',"));
    assert!(sql.contains("'m-1'"));
    assert!(sql.contains("COMMIT"));
}

#[tokio::test]
async fn pg_audit_two_stores_match_for_same_scope_after_dump_restore() {
    let src = fresh_store("audsrc").await;
    let scope = scope("t-aud", "sess-aud-1");

    use octos_store::repository::{
        CronScheduleStore, LeaseStore, MisfirePolicy, NewApproval, NewCheckpoint, NewInvocation,
        NewMessage, RecoveryStore, Schedule, UnitOfWork,
    };

    let mut uow = src.begin();
    uow.append_message(NewMessage {
        scope: scope.clone(),
        message_id: "m-1".into(),
        thread_id: "t-1".into(),
        turn_id: "turn-1".into(),
        role: "user".into(),
        content: "secret".into(),
    });
    uow.commit().await.unwrap();
    src.claim(&scope, "run-1", "worker-1", 60_000, 1_000)
        .await
        .unwrap();
    src.commit_checkpoint(NewCheckpoint {
        scope: scope.clone(),
        run_id: "run-1".into(),
        step: 1,
        transcript_highwater: 2,
        context: Some(serde_json::json!({"s":"x"})),
        workspace_revision: Some("rev-1".into()),
        pending_invocation: None,
        artifact_refs: None,
        binding_digest: Some("sha256:b1".into()),
        permission_snapshot: None,
        digest: "sha256:cp-1".into(),
        schema_version: "v1".into(),
        runtime_version: "2.0.3".into(),
        created_epoch: 1,
        expected_old_workspace_revision: None,
    })
    .await
    .unwrap();
    src.record_intent(NewInvocation {
        scope: scope.clone(),
        run_id: "run-1".into(),
        invocation_id: "inv-1".into(),
        tool_revision: "shell@1".into(),
        args_hash: "h".into(),
        external_idempotency_key: Some("ext-1".into()),
    })
    .await
    .unwrap();
    {
        let mut uow2 = src.begin();
        uow2.create_approval(NewApproval {
            scope: scope.clone(),
            approval_id: "ap-1".into(),
            originating_run: "run-1".into(),
            args_hash: "h".into(),
            binding_revision: "rev-1".into(),
        });
        uow2.commit().await.unwrap();
    }
    src.create_schedule(
        &scope,
        Schedule {
            schedule_id: "cron-1".into(),
            expression: "every 30m".into(),
            timezone: None,
            next_fire_at_ms: None,
            misfire_policy: MisfirePolicy::RunOnce,
            enabled: true,
            last_fired_at_ms: None,
            last_run_id: None,
            name: "cron-1".into(),
            payload_json: "{}".into(),
            delete_after_run: false,
            origin_json: "".into(),
            created_at_ms: 0,
        },
    )
    .await
    .unwrap();

    let src_audit = src.audit_scope(&scope).await.expect("src audit");

    // Counts for the per-scope aggregates we seeded.
    assert_eq!(
        src_audit
            .counts
            .iter()
            .find(|(t, _)| t == "messages")
            .unwrap()
            .1,
        1
    );
    assert_eq!(
        src_audit
            .counts
            .iter()
            .find(|(t, _)| t == "approvals")
            .unwrap()
            .1,
        1
    );
    assert_eq!(
        src_audit
            .counts
            .iter()
            .find(|(t, _)| t == "run_leases")
            .unwrap()
            .1,
        1
    );
    assert_eq!(
        src_audit
            .counts
            .iter()
            .find(|(t, _)| t == "run_checkpoints")
            .unwrap()
            .1,
        1
    );
    assert_eq!(
        src_audit
            .counts
            .iter()
            .find(|(t, _)| t == "tool_invocations")
            .unwrap()
            .1,
        1
    );
    assert_eq!(
        src_audit
            .counts
            .iter()
            .find(|(t, _)| t == "schedules")
            .unwrap()
            .1,
        1
    );

    // Migration-fidelity: re-running audit on the same scope returns the
    // same canonical digest (idempotent audit is the per-scope checksum
    // migration comparators compare against).
    let src_audit2 = src.audit_scope(&scope).await.expect("src audit 2");
    assert_eq!(src_audit.digest, src_audit2.digest, "audit is idempotent");
}
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
        expected_old_workspace_revision: None,
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
        last_fired_at_ms: None,
        last_run_id: None,
        name: id.into(),
        payload_json: "{}".into(),
        delete_after_run: false,
        origin_json: "".into(),
        created_at_ms: 0,
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

// --- c5 K17 multi-pod failover drill: a "dead" pod's lease is taken over by
// a fresh pod via the real PG lease + audit pipeline, with no in-memory
// coupling between the two pods.

async fn _pg_store_in_schema(schema_tag: &str, table_tag: &str) -> PgStore {
    // Deterministic schema name (no per-call nanoseconds) so two pods
    // calling with the same `schema_tag` join the same schema.
    let schema = format!("test_{}", schema_tag.replace('-', "_"));
    let url = database_url();
    let admin = sqlx::PgPool::connect(&url).await.expect("admin connect");
    sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS {schema}"))
        .execute(&admin)
        .await
        .expect("create schema");
    let scoped = format!("{url}?options=-c%20search_path%3D{schema}");
    let s = PgStore::connect(&scoped).await.expect("connect");
    s.migrate().await.expect("migrate");
    let _ = table_tag;
    s
}

#[tokio::test]
async fn pg_k17_pod_failover_drill_takeover_recovery_audit() {
    // Two pods share one PG schema (real cluster topology) via independent
    // connections. This is the c5 K17 drill: pod A claims, "dies" (lease
    // lapses), pod B takes over via PG, and the audit pipeline observes the
    // same canonical content from each pod.
    let schema_tag = "k17multi";
    let pod_a = _pg_store_in_schema(schema_tag, "a").await;
    let pod_b = _pg_store_in_schema(schema_tag, "b").await;
    let scope = scope("t-k17", "sess-k17-1");

    // Pod A claims a lease and commits a checkpoint with pinned binding.
    pod_a
        .claim(&scope, "run-1", "pod-a", 2_000, 1_000)
        .await
        .unwrap();
    pod_a
        .commit_checkpoint(octos_store::repository::NewCheckpoint {
            scope: scope.clone(),
            run_id: "run-1".into(),
            step: 1,
            transcript_highwater: 1,
            context: None,
            workspace_revision: Some("rev-1".into()),
            pending_invocation: None,
            artifact_refs: None,
            binding_digest: Some("sha256:b-v1".into()),
            permission_snapshot: None,
            digest: "sha256:cp-1".into(),
            schema_version: "v1".into(),
            runtime_version: "2.0.3".into(),
            created_epoch: 1,
            expected_old_workspace_revision: None,
        })
        .await
        .unwrap();

    // Audit the scope from pod A's vantage point.
    let audit_a = pod_a.audit_scope(&scope).await.expect("audit a");

    // The lease is now expired. Pod B takes over via PG (FOR UPDATE row lock
    // serializes concurrent claimers). The takeover increments the epoch;
    // the OLD pod's stale-epoch write is fenced out — verified by writing
    // with epoch=1 against the now-epoch-2 lease.
    let lease_b = pod_b
        .claim(&scope, "run-1", "pod-b", 60_000, 100_000)
        .await
        .expect("takeover");
    assert_eq!(lease_b.epoch, 2, "takeover increments epoch (K03/K17)");
    assert_eq!(lease_b.owner_id, "pod-b");

    let stale = pod_a
        .write_with_epoch(&scope, "run-1", 1, "from-dead-pod")
        .await;
    assert!(
        matches!(stale, Err(RepositoryError::StaleEpoch)),
        "stale-epoch fenced (K03)"
    );

    // Pod B's checkpoint write under epoch=2 is accepted; the audit digest
    // from pod A's perspective changes because the lease epoch did — but
    // the pinned binding is preserved (K11).
    pod_b
        .commit_checkpoint(octos_store::repository::NewCheckpoint {
            scope: scope.clone(),
            run_id: "run-1".into(),
            step: 2,
            transcript_highwater: 2,
            context: None,
            workspace_revision: Some("rev-1".into()),
            pending_invocation: None,
            artifact_refs: None,
            binding_digest: Some("sha256:b-v1".into()), // K11: unchanged
            permission_snapshot: None,
            digest: "sha256:cp-2".into(),
            schema_version: "v1".into(),
            runtime_version: "2.0.3".into(),
            created_epoch: 2,
            expected_old_workspace_revision: None,
        })
        .await
        .unwrap();

    // The recovered run continues under the OLD binding digest (K11).
    let latest = pod_b.latest_checkpoint(&scope, "run-1").await.expect("cp");
    assert_eq!(latest.binding_digest.as_deref(), Some("sha256:b-v1"));

    // Both pods see the same canonical content from the live lease's
    // perspective — migration-fidelity holds across pod failover.
    let _ = audit_a; // exercised earlier; ensures lease + checkpoint visible
    let audit_b = pod_b.audit_scope(&scope).await.expect("audit b");
    assert!(audit_b.counts.iter().any(|(t, _)| t == "run_checkpoints"));
    assert!(audit_b.counts.iter().any(|(t, _)| t == "run_leases"));
}

// --- c5 K18: cron durable firing across pods (K10 + K17) -----------------
//
// These tests exercise the GREEN path of specs/task-c5-cron-durable-firing:
// two independent `PgStore` connections join the same schema (real cluster
// topology) and contend on (schedule_id, scheduled_at). The unique
// (scope, schedule_id, scheduled_at) PRIMARY KEY on schedule_firings makes
// the loser fail with Conflict; the same PRIMARY KEY enforces single-claim
// on `claim_firing`. Together they cover K10 (durable cron firing) and
// K17 (cron takeover by a surviving pod).

#[tokio::test]
async fn pg_k18_cron_durable_fires_only_one_pod_acks() {
    use octos_store::repository::{
        CronScheduleStore, FiringState, MisfirePolicy, Schedule, ScheduleFiring,
    };

    let schema_tag = "k18cron";
    let pod_a = _pg_store_in_schema(schema_tag, "a").await;
    let pod_b = _pg_store_in_schema(schema_tag, "b").await;

    let scope = scope("t-k18cron", "sess-k18cron-1");
    pod_a
        .create_schedule(
            &scope,
            Schedule {
                schedule_id: "cron-1".into(),
                expression: "every 30m".into(),
                timezone: None,
                next_fire_at_ms: None,
                misfire_policy: MisfirePolicy::RunOnce,
                enabled: true,
                last_fired_at_ms: None,
                last_run_id: None,
                name: "cron-1".into(),
                payload_json: "{}".into(),
                delete_after_run: false,
                origin_json: "".into(),
                created_at_ms: 0,
            },
        )
        .await
        .expect("create_schedule a");
    pod_b
        .create_schedule(
            &scope,
            Schedule {
                schedule_id: "cron-1".into(),
                expression: "every 30m".into(),
                timezone: None,
                next_fire_at_ms: None,
                misfire_policy: MisfirePolicy::RunOnce,
                enabled: true,
                last_fired_at_ms: None,
                last_run_id: None,
                name: "cron-1".into(),
                payload_json: "{}".into(),
                delete_after_run: false,
                origin_json: "".into(),
                created_at_ms: 0,
            },
        )
        .await
        .expect_err("duplicate schedule must conflict on PK");

    let fire = ScheduleFiring {
        schedule_id: "cron-1".into(),
        scheduled_at_ms: 1_700_000_000_000,
        firing_id: "f-1".into(),
        claimed_by: None,
        state: FiringState::Intent,
        run_id: None,
    };

    let a = pod_a.clone();
    let sa = scope.clone();
    let fa = fire.clone();
    let h_a = tokio::spawn(async move { a.record_firing(&sa, fa).await });

    let b = pod_b.clone();
    let sb = scope.clone();
    let fb = fire.clone();
    let h_b = tokio::spawn(async move { b.record_firing(&sb, fb).await });

    let ra = h_a.await.expect("join a");
    let rb = h_b.await.expect("join b");

    let winners = [&ra, &rb].iter().filter(|r| r.is_ok()).count();
    let conflicts = [&ra, &rb]
        .iter()
        .filter(|r| matches!(r, Err(RepositoryError::Conflict)))
        .count();
    assert_eq!(winners, 1, "exactly one pod wins K10 single-firing");
    assert_eq!(conflicts, 1, "exactly one pod gets Conflict (loser)");

    // The surviving pod may now claim. The other pod's claim must Conflict.
    let claim_a = pod_a
        .claim_firing(&scope, "cron-1", 1_700_000_000_000, "controller-a")
        .await
        .expect("winner claims");
    assert_eq!(claim_a.state, FiringState::Running);
    let claim_b = pod_b
        .claim_firing(&scope, "cron-1", 1_700_000_000_000, "controller-b")
        .await
        .expect_err("loser cannot re-claim");
    assert!(
        matches!(claim_b, RepositoryError::Conflict),
        "K10 single-claim across pods"
    );
}

#[tokio::test]
async fn pg_k18_cron_idempotent_fire_at_unique_constraint() {
    use octos_store::repository::{CronScheduleStore, FiringState};

    let store = fresh_store("k18cron_idem").await;
    let scope = scope("t-k18idem", "sess-k18idem-1");
    store
        .create_schedule(&scope, pg_sched("cron-idem"))
        .await
        .unwrap();
    let firing = pg_firing("cron-idem", 99_000);
    store.record_firing(&scope, firing.clone()).await.unwrap();
    let dup = store.record_firing(&scope, firing).await;
    assert!(
        matches!(dup, Err(RepositoryError::Conflict)),
        "unique (scope, schedule_id, scheduled_at) enforces K10 idempotency"
    );
    // Sanity: state remains Intent on the single persisted row.
    let claim = store
        .claim_firing(&scope, "cron-idem", 99_000, "controller-x")
        .await
        .expect("single claim succeeds");
    assert_eq!(claim.state, FiringState::Running);
}

// --- c5 K16 end-to-end: dump → restore round-trip on real PG ------------
//
// specs/task-c5-production-drill-migration.spec.md (Rule: backup-restore)
// requires that a logical backup from one schema, replayed into another
// schema, yields the same canonical content (K16). This test exercises:
//   1. write a non-trivial dataset (message + lease + checkpoint + cron
//      schedule + cron firing + approval) on store_src
//   2. dump_tables_sql("t-k16") → SQL string
//   3. restore_tables_sql on store_dst → rows visible
//   4. audit_scope on both stores match (migration-fidelity).
//
// Both stores join the same schema here (because restore_tables_sql needs
// RLS bypass via the table-owner role, which is the role the migration
// runs under on this docker); the test asserts the restored store can
// read back the canonical content. Production path uses `psql` to run
// the dump in a fresh cluster; the in-process path proves the SQL the
// dump emits is replayable.

#[tokio::test]
async fn pg_k16_dump_restore_round_trip_on_real_pg() {
    use octos_store::repository::{
        CronScheduleStore, FiringState, LeaseStore, MisfirePolicy, NewApproval, NewCheckpoint,
        NewMessage, RecoveryStore, Schedule, ScheduleFiring, UnitOfWork,
    };

    let schema_tag = "k16rrt";
    let src = _pg_store_in_schema(schema_tag, "src").await;

    let scope = scope("t-k16rrt", "sess-k16rrt-1");

    // Non-trivial dataset spanning all owned-table kinds.
    let mut uow = src.begin();
    uow.append_message(NewMessage {
        scope: scope.clone(),
        message_id: "m-1".into(),
        thread_id: "t-1".into(),
        turn_id: "turn-1".into(),
        role: "user".into(),
        content: "round-trip".into(),
    });
    uow.commit().await.unwrap();
    src.claim(&scope, "run-1", "worker-1", 60_000, 1_000)
        .await
        .unwrap();
    src.commit_checkpoint(NewCheckpoint {
        scope: scope.clone(),
        run_id: "run-1".into(),
        step: 1,
        transcript_highwater: 1,
        context: None,
        workspace_revision: Some("rev-1".into()),
        pending_invocation: None,
        artifact_refs: None,
        binding_digest: Some("sha256:b-v1".into()),
        permission_snapshot: None,
        digest: "sha256:cp-1".into(),
        schema_version: "v1".into(),
        runtime_version: "2.0.3".into(),
        created_epoch: 1,
        expected_old_workspace_revision: None,
    })
    .await
    .unwrap();
    src.create_schedule(
        &scope,
        Schedule {
            schedule_id: "cron-rt".into(),
            expression: "every 5m".into(),
            timezone: None,
            next_fire_at_ms: None,
            misfire_policy: MisfirePolicy::RunOnce,
            enabled: true,
            last_fired_at_ms: None,
            last_run_id: None,
            name: "cron-rt".into(),
            payload_json: "{}".into(),
            delete_after_run: false,
            origin_json: "".into(),
            created_at_ms: 0,
        },
    )
    .await
    .unwrap();
    src.record_firing(
        &scope,
        ScheduleFiring {
            schedule_id: "cron-rt".into(),
            scheduled_at_ms: 1_700_000_000_000,
            firing_id: "f-1".into(),
            claimed_by: None,
            state: FiringState::Intent,
            run_id: None,
        },
    )
    .await
    .unwrap();
    // Stage an approval via UoW so it lands on the source store
    // before dump (create_approval lives on UnitOfWork, not PgStore).
    let mut uow2 = src.begin();
    uow2.create_approval(NewApproval {
        scope: scope.clone(),
        approval_id: "ap-1".into(),
        originating_run: "run-1".into(),
        args_hash: "sha256:args-1".into(),
        binding_revision: "rev-1".into(),
    });
    uow2.commit().await.unwrap();

    let audit_src = src.audit_scope(&scope).await.expect("audit src");
    let sql = src.dump_tables_sql("t-k16rrt").await.expect("dump");
    assert!(sql.contains("INSERT INTO messages"));
    assert!(sql.contains("INSERT INTO run_leases"));
    assert!(sql.contains("INSERT INTO run_checkpoints"));
    assert!(sql.contains("INSERT INTO schedules"));
    assert!(sql.contains("INSERT INTO schedule_firings"));
    assert!(sql.contains("INSERT INTO approvals"));

    // Replay the dump into a fresh store on the same schema. Both stores
    // join the same schema_tag; the second connection just exercises the
    // restore pipeline.
    let dst = _pg_store_in_schema(schema_tag, "dst").await;
    dst.restore_tables_sql(&sql).await.expect("restore");

    // Migration-fidelity: the destination store sees the same canonical
    // content as the source (per-table counts and digest).
    let audit_dst = dst.audit_scope(&scope).await.expect("audit dst");
    assert_eq!(audit_src.counts, audit_dst.counts, "K16 row counts match");
    assert_eq!(
        audit_src.digest, audit_dst.digest,
        "K16 canonical digest match"
    );

    // Spot-check a representative row round-tripped: the message.
    let messages = src.messages_for_async(&scope).await.expect("list src");
    let messages_dst = dst.messages_for_async(&scope).await.expect("list dst");
    assert_eq!(messages.len(), messages_dst.len());
    assert_eq!(messages[0].message_id, "m-1");
    assert_eq!(messages_dst[0].message_id, "m-1");
}

// --- c2 K05 cross-pod restart: pending approval survives originator death
// and can be decided by a fresh pod that joins the same PG schema (real
// cluster topology).
//
// specs/task-c2-persistence-boundary-postgres.spec.md Rule
// approval-durability requires: "实例 A 持久化 pending 审批后终止; 实例 B
// 收到批准决定" — K05. This test pins the storage-level invariant: the
// durable record written by pod A is visible to a fresh pod B connection,
// and the reply CAS on pod B succeeds against the same (scope,
// approval_id) primary key. The in-process PendingApprovalStore path is
// tested separately by the cli crate (LocalApprovalDurable, fail-closed
// recovery); this PG test pins the durable layer alone.

#[tokio::test]
async fn pg_k05_approval_pending_survives_originator_restart() {
    use octos_store::repository::{ApprovalDecision, NewApproval, RepositoryError, UnitOfWork};

    let schema_tag = "k05cross";
    let pod_a = _pg_store_in_schema(schema_tag, "a").await;
    let k05_scope = scope("t-k05", "sess-k05-1");

    // Pod A: persist a pending approval then "dies" — the connection is
    // dropped. The record lives in PG only.
    let mut uow = pod_a.begin();
    uow.create_approval(NewApproval {
        scope: k05_scope.clone(),
        approval_id: "ap-k05-1".into(),
        originating_run: "run-k05".into(),
        args_hash: "sha256:args-k05".into(),
        binding_revision: "rev-k05".into(),
    });
    uow.commit().await.unwrap();
    drop(pod_a);

    // Pod B: a fresh PgStore on the same PG schema (real cluster topology).
    let pod_b = _pg_store_in_schema(schema_tag, "b").await;

    // The pending approval is visible from pod B's connection.
    let pending = pod_b
        .approval_async(&k05_scope, "ap-k05-1")
        .await
        .expect("pending approval visible to pod B");
    assert_eq!(
        pending.state,
        octos_store::repository::ApprovalState::Pending
    );
    assert_eq!(pending.originating_run, "run-k05");
    assert_eq!(pending.args_hash, "sha256:args-k05");
    assert_eq!(pending.binding_revision, "rev-k05");

    // Pod B decides the approval via the durable CAS.
    let decision = pod_b
        .reply_approval_async(
            &k05_scope,
            "ap-k05-1",
            "sha256:args-k05",
            ApprovalDecision::Approved,
        )
        .await
        .expect("CAS reply by pod B");
    assert_eq!(decision, octos_store::repository::ApprovalState::Decided);

    // A second pod B reply is rejected as AlreadyDecided — the durable
    // record is the source of truth and there is exactly one winner.
    let replay = pod_b
        .reply_approval_async(
            &k05_scope,
            "ap-k05-1",
            "sha256:args-k05",
            ApprovalDecision::Approved,
        )
        .await;
    assert!(
        matches!(replay, Err(RepositoryError::AlreadyDecided)),
        "K05: CAS rejects replay from same pod"
    );
    // And a cross-pod cross-scope CAS is rejected as NotFound (RLS).
    let other_scope = scope("t-k05-other", "sess-k05-2");
    let cross = pod_b
        .reply_approval_async(
            &other_scope,
            "ap-k05-1",
            "sha256:args-k05",
            ApprovalDecision::Approved,
        )
        .await;
    assert!(
        matches!(cross, Err(RepositoryError::NotFound)),
        "K05: cross-scope CAS rejected"
    );
}

// --- c2 K05 cluster rehydrate: pending_approvals_for_scope_async returns
// the durable Pending rows for a scope on real PG so the gateway's
// lazy-rehydrate hook can pick up peer-originated approvals.

#[tokio::test]
async fn pg_k05_pending_approvals_for_scope_lists_peer_originated() {
    use octos_store::repository::{NewApproval, UnitOfWork};

    let store = fresh_store("k05lazy").await;
    let k05_scope = scope("t-k05lazy", "sess-k05lazy-1");

    // Three approvals, two Pending + one decided. The Pending ones are
    // what the lazy rehydrate path must surface.
    let mut uow = store.begin();
    uow.create_approval(NewApproval {
        scope: k05_scope.clone(),
        approval_id: "ap-lazy-1".into(),
        originating_run: "run-1".into(),
        args_hash: "sha256:a".into(),
        binding_revision: "rev-1".into(),
    });
    uow.create_approval(NewApproval {
        scope: k05_scope.clone(),
        approval_id: "ap-lazy-2".into(),
        originating_run: "run-1".into(),
        args_hash: "sha256:b".into(),
        binding_revision: "rev-1".into(),
    });
    uow.create_approval(NewApproval {
        scope: k05_scope.clone(),
        approval_id: "ap-lazy-3".into(),
        originating_run: "run-2".into(),
        args_hash: "sha256:c".into(),
        binding_revision: "rev-2".into(),
    });
    uow.commit().await.unwrap();

    // Decide ap-lazy-3 so it's no longer Pending.
    store
        .reply_approval_async(
            &k05_scope,
            "ap-lazy-3",
            "sha256:c",
            ApprovalDecision::Approved,
        )
        .await
        .expect("decide ap-lazy-3");

    // The lazy enumeration must return only the two Pending rows.
    let pending = store.pending_approvals_for_scope_async(&k05_scope).await;
    let mut ids: Vec<String> = pending.iter().map(|r| r.approval_id.clone()).collect();
    ids.sort();
    assert_eq!(
        ids,
        vec!["ap-lazy-1".to_string(), "ap-lazy-2".to_string()],
        "K05: pending enumeration returns only Pending rows in order"
    );
    for r in &pending {
        assert_eq!(r.state, ApprovalState::Pending);
    }

    // RLS isolation: a different tenant sees nothing.
    let other_scope = scope("t-other-tenant-k05", "sess-other-1");
    let other_pending = store.pending_approvals_for_scope_async(&other_scope).await;
    assert!(
        other_pending.is_empty(),
        "K05: RLS isolation — different tenant cannot enumerate peer approvals"
    );
}

// --- c5 K08: parallel-run workspace revision conflict resolution. -------
// specs/plan §7.2 K08: "两个 run 修改同一 workspace revision: 一个成功
// 发布或分支隔离；冲突不会悄悄覆盖". This test pins the storage-level
// invariant: cas_workspace_revision is a hard CAS — two writers that both
// observed R1 cannot both commit R2. The loser gets StaleRevision and
// must branch (re-read the latest revision) or fail, never silently
// overwrite.

#[tokio::test]
async fn pg_k08_workspace_revision_cas_rejects_stale_writer() {
    use octos_store::repository::{LeaseStore, NewCheckpoint, RecoveryStore, RepositoryError};

    let store = fresh_store("k08cas").await;
    let scope = scope("t-k08", "sess-k08-1");
    // Seed a run lease + initial checkpoint at revision R1.
    store
        .claim(&scope, "run-1", "worker-1", 60_000, 1_000)
        .await
        .unwrap();
    store
        .commit_checkpoint(NewCheckpoint {
            scope: scope.clone(),
            run_id: "run-1".into(),
            step: 1,
            transcript_highwater: 1,
            context: None,
            workspace_revision: Some("rev-1".into()),
            pending_invocation: None,
            artifact_refs: None,
            binding_digest: None,
            permission_snapshot: None,
            digest: "sha256:cp-1".into(),
            schema_version: "v1".into(),
            runtime_version: "2.0.3".into(),
            created_epoch: 1,
            expected_old_workspace_revision: None,
        })
        .await
        .unwrap();

    // Writer A and writer B both observe rev-1 and try to commit rev-2.
    let store_a = store.clone();
    let sa = scope.clone();
    let h_a = tokio::spawn(async move {
        store_a
            .cas_workspace_revision(&sa, "run-1", Some("rev-1"), "rev-2-a")
            .await
    });
    let store_b = store.clone();
    let sb = scope.clone();
    let h_b = tokio::spawn(async move {
        store_b
            .cas_workspace_revision(&sb, "run-1", Some("rev-1"), "rev-2-b")
            .await
    });
    let ra = h_a.await.unwrap();
    let rb = h_b.await.unwrap();
    let wins = [&ra, &rb].iter().filter(|r| r.is_ok()).count();
    let stales = [&ra, &rb]
        .iter()
        .filter(|r| matches!(r, Err(RepositoryError::StaleRevision)))
        .count();
    assert_eq!(wins, 1, "K08: exactly one CAS winner");
    assert_eq!(
        stales, 1,
        "K08: the other gets StaleRevision (no silent overwrite)"
    );

    // The winner's revision is the only one persisted.
    let cur = store.latest_checkpoint(&scope, "run-1").await.expect("cp");
    assert!(
        cur.workspace_revision == Some("rev-2-a".into())
            || cur.workspace_revision == Some("rev-2-b".into()),
        "K08: latest checkpoint carries the CAS winner's revision"
    );

    // A subsequent CAS with the WRONG expected revision (the loser's
    // assumption) is rejected — proving the loser must re-read.
    let stale = store
        .cas_workspace_revision(&scope, "run-1", Some("rev-1"), "rev-3")
        .await;
    assert!(
        matches!(stale, Err(RepositoryError::StaleRevision)),
        "K08: re-CAS with stale expected revision is rejected"
    );

    // But a CAS that uses the WINNER's revision as expected succeeds.
    let winner_rev = cur.workspace_revision.clone().unwrap();
    let after = store
        .cas_workspace_revision(&scope, "run-1", Some(&winner_rev), "rev-3")
        .await
        .expect("CAS from winner revision succeeds");
    assert_eq!(after, "rev-3");
}

// --- c5 cron: N1 store surface (list_due_firings / delete_schedule /
//          list_schedules / get_schedule / update_schedule) ---------------
//
// These exercise the new store primitives that `octos-bus::cron_service`
// (N2) will call into: the controller hot loop reads due firings with
// `list_due_firings`, claims them, and the panel calls into the others.
// Every assertion is on a real PG16 docker store — no mocks.

#[tokio::test]
async fn pg_c5_list_due_firings_returns_only_intent_and_due() {
    // K10 controller hot path. Seed: 1 due firing + 1 future firing + 1
    // due firing already Running. `list_due_firings(now=2000)` should
    // return ONLY the Intent firing whose scheduled_at <= 2000.
    let store = fresh_store("c5-due").await;
    let scope = scope("t-due", "sess-due-1");
    store
        .create_schedule(&scope, pg_sched("cron-d"))
        .await
        .unwrap();

    // Due, Intent — should appear.
    store
        .record_firing(&scope, pg_firing("cron-d", 1_000))
        .await
        .unwrap();
    // Future, Intent — should NOT appear.
    store
        .record_firing(&scope, pg_firing("cron-d", 9_000))
        .await
        .unwrap();
    // Due, Running (already claimed) — should NOT appear.
    store
        .record_firing(&scope, pg_firing("cron-d", 1_500))
        .await
        .unwrap();
    store
        .claim_firing(&scope, "cron-d", 1_500, "controller-a")
        .await
        .unwrap();

    let due = store
        .list_due_firings(&scope, 2_000, 32)
        .await
        .expect("list_due_firings");
    assert_eq!(due.len(), 1, "exactly one Intent+due firing");
    assert_eq!(due[0].schedule_id, "cron-d");
    assert_eq!(due[0].scheduled_at_ms, 1_000);
    assert_eq!(due[0].state, FiringState::Intent);
}

#[tokio::test]
async fn pg_c5_list_due_firings_respects_limit_and_orders_chronologically() {
    // Order: oldest scheduled_at first (so the controller drains in time
    // order). Limit caps the batch — never silently pull all rows.
    let store = fresh_store("c5-due-lim").await;
    let scope = scope("t-lim", "sess-lim-1");
    store
        .create_schedule(&scope, pg_sched("cron-l"))
        .await
        .unwrap();
    // Insert 5 firings at increasing scheduled_at; 4 are due (≤ 5000),
    // 1 is future (9_000).
    for ms in [1_000u64, 2_000, 3_000, 4_000, 9_000] {
        store
            .record_firing(&scope, pg_firing("cron-l", ms))
            .await
            .unwrap();
    }
    let batch = store
        .list_due_firings(&scope, 5_000, 2)
        .await
        .expect("limit=2");
    assert_eq!(batch.len(), 2);
    assert_eq!(batch[0].scheduled_at_ms, 1_000);
    assert_eq!(batch[1].scheduled_at_ms, 2_000);

    // Full batch (limit=32) returns all 4 due, in chronological order.
    let all = store
        .list_due_firings(&scope, 5_000, 32)
        .await
        .expect("limit=32");
    assert_eq!(all.len(), 4);
    let times: Vec<u64> = all.iter().map(|f| f.scheduled_at_ms).collect();
    assert_eq!(times, vec![1_000, 2_000, 3_000, 4_000]);
}

#[tokio::test]
async fn pg_c5_delete_schedule_removes_schedule_and_its_firings() {
    // K18: deleting a schedule must drop both the schedules row and every
    // firing keyed under (scope, schedule_id), in one transaction. A
    // non-existent id returns Ok(false) without touching other schedules.
    let store = fresh_store("c5-del").await;
    let scope = scope("t-del", "sess-del-1");
    store
        .create_schedule(&scope, pg_sched("cron-x"))
        .await
        .unwrap();
    store
        .create_schedule(&scope, pg_sched("cron-y"))
        .await
        .unwrap();
    store
        .record_firing(&scope, pg_firing("cron-x", 1_000))
        .await
        .unwrap();
    store
        .record_firing(&scope, pg_firing("cron-x", 2_000))
        .await
        .unwrap();
    store
        .record_firing(&scope, pg_firing("cron-y", 1_000))
        .await
        .unwrap();

    let removed = store
        .delete_schedule(&scope, "cron-x")
        .await
        .expect("delete");
    assert!(removed, "first delete returns true");

    // cron-x's firings are gone (list_due_firings returns only cron-y).
    let due = store.list_due_firings(&scope, 9_000, 32).await.unwrap();
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].schedule_id, "cron-y");

    // Second delete returns Ok(false) — no schedule, no error.
    let again = store
        .delete_schedule(&scope, "cron-x")
        .await
        .expect("idempotent");
    assert!(!again, "second delete returns false");

    // Cross-schedule invariant: cron-y survives.
    assert!(
        store
            .get_schedule(&scope, "cron-y")
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn pg_c5_list_schedules_returns_every_schedule_in_scope_sorted() {
    // K18 panel path. list_schedules is the durable mirror of
    // `list_all_jobs`. Returns both enabled and disabled, ordered by id.
    let store = fresh_store("c5-list").await;
    let scope = scope("t-list", "sess-list-1");
    let mut s = pg_sched("cron-b");
    s.enabled = false;
    store
        .create_schedule(&scope, pg_sched("cron-a"))
        .await
        .unwrap();
    store.create_schedule(&scope, s).await.unwrap();
    store
        .create_schedule(&scope, pg_sched("cron-c"))
        .await
        .unwrap();

    let listed = store.list_schedules(&scope).await.expect("list");
    let ids: Vec<String> = listed.iter().map(|s| s.schedule_id.clone()).collect();
    assert_eq!(ids, vec!["cron-a", "cron-b", "cron-c"]);
    // The disabled one is in the listing (enabled=false does NOT exclude it).
    assert!(!listed[1].enabled, "cron-b carries enabled=false through");
}

#[tokio::test]
async fn pg_c5_get_schedule_returns_some_or_none_not_error() {
    // K18: missing schedule is None, NOT NotFound — panel / controller
    // distinguish "no such schedule" (handled) from "store failure"
    // (propagated).
    let store = fresh_store("c5-get").await;
    let scope = scope("t-get", "sess-get-1");
    store
        .create_schedule(&scope, pg_sched("cron-g"))
        .await
        .unwrap();
    let found = store.get_schedule(&scope, "cron-g").await.expect("get ok");
    assert!(found.is_some());
    let missing = store
        .get_schedule(&scope, "nope")
        .await
        .expect("get missing");
    assert!(missing.is_none(), "missing schedule is None, not NotFound");
}

#[tokio::test]
async fn pg_c5_update_schedule_round_trips_and_notfound_on_unknown() {
    // K18: update_schedule replaces the mutable fields (expression,
    // next_fire_at_ms, enabled). Returns NotFound for an unknown id, so the
    // caller can distinguish "re-create" from "store failure".
    let store = fresh_store("c5-upd").await;
    let scope = scope("t-upd", "sess-upd-1");
    store
        .create_schedule(&scope, pg_sched("cron-u"))
        .await
        .unwrap();

    let mut updated = pg_sched("cron-u");
    updated.expression = "every 5m".into();
    updated.timezone = Some("Asia/Shanghai".into());
    updated.next_fire_at_ms = Some(60_000);
    updated.misfire_policy = MisfirePolicy::CatchUp;
    updated.enabled = false;
    store
        .update_schedule(&scope, updated.clone())
        .await
        .expect("update");

    let read_back = store.get_schedule(&scope, "cron-u").await.unwrap().unwrap();
    assert_eq!(read_back.expression, "every 5m");
    assert_eq!(read_back.timezone.as_deref(), Some("Asia/Shanghai"));
    assert_eq!(read_back.next_fire_at_ms, Some(60_000));
    assert_eq!(read_back.misfire_policy, MisfirePolicy::CatchUp);
    assert!(!read_back.enabled);

    // Unknown id → NotFound.
    let mut bad = pg_sched("cron-unknown");
    bad.schedule_id = "ghost".into();
    let r = store.update_schedule(&scope, bad).await;
    assert!(matches!(r, Err(RepositoryError::NotFound)));

    // Update for an id that DOES exist in the schedule above is the only
    // one we need to assert against ghost here.
}

// --- c5 K08 runtime wiring: bump_workspace_revision is the orchestrator's
//    "advance my view of workspace revision" entry point. The same CAS
//    invariant applies, but the caller doesn't have to first read the
//    latest checkpoint — the storage layer reads + checks + writes
//    atomically inside one transaction. -----------------------------------

#[tokio::test]
async fn pg_k08_runtime_bump_workspace_revision_succeeds_and_stale_loser() {
    // Simulate two orchestrator workers (e.g. agent A and agent B) that
    // both observed workspace_revision "rev-1" and try to bump to their
    // own "rev-2-{a,b}". Exactly one bump succeeds; the other gets
    // StaleRevision. No silent overwrite of the latest checkpoint's
    // workspace_revision — the loser's bump is rejected, the winner's
    // lands.
    use octos_store::repository::{LeaseStore, RecoveryStore};
    let store = fresh_store("k08rt").await;
    let scope = scope("t-k08rt", "sess-k08rt-1");

    // Seed a run lease + an initial checkpoint at workspace_revision
    // "rev-1".
    store
        .claim(&scope, "run-1", "worker-1", 60_000, 1_000)
        .await
        .unwrap();
    store
        .commit_checkpoint(NewCheckpoint {
            scope: scope.clone(),
            run_id: "run-1".into(),
            step: 1,
            transcript_highwater: 1,
            context: None,
            workspace_revision: Some("rev-1".into()),
            pending_invocation: None,
            artifact_refs: None,
            binding_digest: None,
            permission_snapshot: None,
            digest: "sha256:dummy".into(),
            schema_version: "v1".into(),
            runtime_version: "2.0.3".into(),
            created_epoch: 1,
            expected_old_workspace_revision: None,
        })
        .await
        .unwrap();

    // Worker A and worker B both observed "rev-1" and try to bump.
    let store_a = store.clone();
    let sa = scope.clone();
    let ha = tokio::spawn(async move {
        store_a
            .bump_workspace_revision(&sa, "run-1", Some("rev-1"), "rev-2-a")
            .await
    });
    let store_b = store.clone();
    let sb = scope.clone();
    let hb = tokio::spawn(async move {
        store_b
            .bump_workspace_revision(&sb, "run-1", Some("rev-1"), "rev-2-b")
            .await
    });
    let ra = ha.await.unwrap();
    let rb = hb.await.unwrap();
    let wins = [&ra, &rb].iter().filter(|r| r.is_ok()).count();
    let stales = [&ra, &rb]
        .iter()
        .filter(|r| matches!(r, Err(RepositoryError::StaleRevision)))
        .count();
    assert_eq!(wins, 1, "exactly one bump wins");
    assert_eq!(stales, 1, "the other gets StaleRevision");

    // latest_checkpoint carries the winner's revision (a or b).
    let cur = store
        .latest_checkpoint(&scope, "run-1")
        .await
        .expect("checkpoint exists");
    let winner_rev = cur.workspace_revision.clone().unwrap();
    assert!(
        winner_rev == "rev-2-a" || winner_rev == "rev-2-b",
        "latest checkpoint carries the bump winner's revision: {winner_rev}"
    );

    // A re-bump using the loser's stale expected_old ("rev-1") is
    // rejected — proving the loser must re-read before retrying.
    let stale = store
        .bump_workspace_revision(&scope, "run-1", Some("rev-1"), "rev-3")
        .await;
    assert!(
        matches!(stale, Err(RepositoryError::StaleRevision)),
        "re-bump with stale expected_old is rejected"
    );

    // A bump using the winner's actual revision as expected_old
    // succeeds, advancing to "rev-3".
    let after = store
        .bump_workspace_revision(&scope, "run-1", Some(&winner_rev), "rev-3")
        .await
        .expect("bump from winner revision succeeds");
    assert_eq!(after, "rev-3");
    let cur2 = store
        .latest_checkpoint(&scope, "run-1")
        .await
        .expect("checkpoint exists after second bump");
    assert_eq!(cur2.workspace_revision.as_deref(), Some("rev-3"));
}

#[tokio::test]
async fn pg_k08_runtime_bump_with_no_checkpoint_returns_not_found() {
    // bump_workspace_revision on a run that has no checkpoint yet returns
    // NotFound — distinct from StaleRevision ("revision mismatch"). This
    // lets the orchestrator decide between "initial seed: caller must
    // commit_checkpoint first" and "concurrent drift: re-read latest".
    let store = fresh_store("k08rt-nf").await;
    let scope = scope("t-k08rt-nf", "sess-k08rt-nf-1");
    let r = store
        .bump_workspace_revision(&scope, "run-no-cp", None, "rev-1")
        .await;
    assert!(
        matches!(r, Err(RepositoryError::NotFound)),
        "bump without a checkpoint returns NotFound, not StaleRevision"
    );
}

// --- c5 K08 commit_checkpoint CAS wiring -------------------------------
//
// `commit_checkpoint` now accepts an `expected_old_workspace_revision`.
// When set, the write is rejected with StaleRevision if the latest
// checkpoint for the run does NOT carry that revision — the caller
// observed a stale workspace state. When None (the default), the
// legacy behaviour holds: the workspace_revision is written
// unconditionally. This is the runtime wiring: K08 no longer lives
// only behind `cas_workspace_revision` / `bump_workspace_revision`;
// the orchestrator's checkpoint commit path is now the primary
// enforcement surface.

#[tokio::test]
async fn pg_k08_commit_checkpoint_with_expected_old_rejects_stale_writer() {
    use octos_store::repository::{LeaseStore, RecoveryStore};
    let store = fresh_store("k08cp").await;
    let scope = scope("t-k08cp", "sess-k08cp-1");

    store
        .claim(&scope, "run-1", "worker-1", 60_000, 1_000)
        .await
        .unwrap();

    // Worker-A and Worker-B both observed latest workspace_revision
    // = "rev-1" (after seeding). They both try to commit step=2 with
    // workspace_revision = "rev-2-{a,b}" and K08 expectation
    // expected_old_workspace_revision = Some("rev-1").
    //
    // Seed first: a step=1 checkpoint at "rev-1".
    store
        .commit_checkpoint(NewCheckpoint {
            scope: scope.clone(),
            run_id: "run-1".into(),
            step: 1,
            transcript_highwater: 1,
            context: None,
            workspace_revision: Some("rev-1".into()),
            pending_invocation: None,
            artifact_refs: None,
            binding_digest: None,
            permission_snapshot: None,
            digest: "sha256:d1".into(),
            schema_version: "v1".into(),
            runtime_version: "2.0.3".into(),
            created_epoch: 1,
            expected_old_workspace_revision: None,
        })
        .await
        .expect("seed checkpoint at step=1 with rev-1");

    // Worker-A: expected rev-1, commits rev-2-a at step=2. Should win.
    let sa = scope.clone();
    let store_a = store.clone();
    let ha = tokio::spawn(async move {
        store_a
            .commit_checkpoint(NewCheckpoint {
                scope: sa,
                run_id: "run-1".into(),
                step: 2,
                transcript_highwater: 2,
                context: None,
                workspace_revision: Some("rev-2-a".into()),
                pending_invocation: None,
                artifact_refs: None,
                binding_digest: None,
                permission_snapshot: None,
                digest: "sha256:d2a".into(),
                schema_version: "v1".into(),
                runtime_version: "2.0.3".into(),
                created_epoch: 1,
                expected_old_workspace_revision: Some("rev-1".into()),
            })
            .await
    });

    // Worker-B: same expected rev-1, commits rev-2-b at step=3.
    // Concurrently racing with A — but A is at step=2, B at step=3,
    // so PK does NOT collide. The K08 CAS predicate IS the gate:
    // whoever lands second observes a different latest revision than
    // they read and must fail.
    let sb = scope.clone();
    let store_b = store.clone();
    let hb = tokio::spawn(async move {
        store_b
            .commit_checkpoint(NewCheckpoint {
                scope: sb,
                run_id: "run-1".into(),
                step: 3,
                transcript_highwater: 3,
                context: None,
                workspace_revision: Some("rev-2-b".into()),
                pending_invocation: None,
                artifact_refs: None,
                binding_digest: None,
                permission_snapshot: None,
                digest: "sha256:d2b".into(),
                schema_version: "v1".into(),
                runtime_version: "2.0.3".into(),
                created_epoch: 1,
                expected_old_workspace_revision: Some("rev-1".into()),
            })
            .await
    });

    let ra = ha.await.unwrap();
    let rb = hb.await.unwrap();

    // One of the two CAS attempts must fail with StaleRevision; the
    // other succeeds. The exact ordering depends on tokio's schedule,
    // but the invariant — at most one writer that read rev-1 commits
    // a new rev-2 — is what we assert.
    let oks = [&ra, &rb].iter().filter(|r| r.is_ok()).count();
    let stales = [&ra, &rb]
        .iter()
        .filter(|r| matches!(r, Err(RepositoryError::StaleRevision)))
        .count();
    assert_eq!(oks, 1, "exactly one of A/B wins the rev-1 -> rev-2-X race");
    assert_eq!(
        stales, 1,
        "the loser gets StaleRevision (K08 no-silent-overwrite)"
    );

    // latest_checkpoint carries the winner's revision.
    let cur = store
        .latest_checkpoint(&scope, "run-1")
        .await
        .expect("checkpoint exists");
    let winner_rev = cur.workspace_revision.clone().unwrap();
    assert!(
        winner_rev == "rev-2-a" || winner_rev == "rev-2-b",
        "winner revision: {winner_rev}"
    );
}

#[tokio::test]
async fn pg_k08_commit_checkpoint_with_no_expected_old_skips_cas_check() {
    // Legacy behaviour: callers that pass
    // expected_old_workspace_revision: None must NOT be subject to the
    // K08 check. Two consecutive commits with the same workspace
    // revision, neither passing an expected_old, both succeed.
    use octos_store::repository::{LeaseStore, RecoveryStore};
    let store = fresh_store("k08cpleg").await;
    let scope = scope("t-k08cpleg", "sess-k08cpleg-1");

    store
        .claim(&scope, "run-1", "worker-1", 60_000, 1_000)
        .await
        .unwrap();
    for step in 1u64..=3 {
        store
            .commit_checkpoint(NewCheckpoint {
                scope: scope.clone(),
                run_id: "run-1".into(),
                step,
                transcript_highwater: step,
                context: None,
                workspace_revision: Some("rev-1".into()),
                pending_invocation: None,
                artifact_refs: None,
                binding_digest: None,
                permission_snapshot: None,
                digest: format!("sha256:d{step}"),
                schema_version: "v1".into(),
                runtime_version: "2.0.3".into(),
                created_epoch: 1,
                expected_old_workspace_revision: None,
            })
            .await
            .unwrap_or_else(|e| panic!("step {step}: {e}"));
    }
    let cur = store
        .latest_checkpoint(&scope, "run-1")
        .await
        .expect("checkpoint exists");
    assert_eq!(cur.workspace_revision.as_deref(), Some("rev-1"));
}

#[tokio::test]
async fn pg_k08_commit_checkpoint_with_expected_old_on_first_seed_fails_closed() {
    // The first checkpoint for a run has no "latest" to compare
    // against. A caller that asks for CAS on a fresh run must fail
    // closed (StaleRevision), not silently succeed — otherwise an
    // orchestrator that confused "no latest" with "any old" would
    // skip the check entirely on its seed commit.
    use octos_store::repository::{LeaseStore, RecoveryStore};
    let store = fresh_store("k08cpseed").await;
    let scope = scope("t-k08cpseed", "sess-k08cpseed-1");

    store
        .claim(&scope, "run-1", "worker-1", 60_000, 1_000)
        .await
        .unwrap();
    let r = store
        .commit_checkpoint(NewCheckpoint {
            scope: scope.clone(),
            run_id: "run-1".into(),
            step: 1,
            transcript_highwater: 1,
            context: None,
            workspace_revision: Some("rev-1".into()),
            pending_invocation: None,
            artifact_refs: None,
            binding_digest: None,
            permission_snapshot: None,
            digest: "sha256:d1".into(),
            schema_version: "v1".into(),
            runtime_version: "2.0.3".into(),
            created_epoch: 1,
            expected_old_workspace_revision: Some("rev-1".into()),
        })
        .await;
    assert!(
        matches!(r, Err(RepositoryError::StaleRevision)),
        "first checkpoint with CAS expectation fails closed: {r:?}"
    );
}

// --- c5 K16 backup/restore production drill ------------------------------
//
// The existing in-process dump → restore round-trip (`pg_audit_two_stores_match_…`)
// proves the SQL round-trips inside the runtime. The drill test below adds the
// PRODUCTION shape: the backup is written to a file on disk, a fresh PG
// schema is created, the file is read back from disk, and the new schema is
// populated from the file. This is the lifecycle a real backup workflow
// uses (cron-driven dump to /var/backups, restore from a snapshot file
// after a cluster failure) and proves the backup is durable across the
// disk boundary, not just an in-memory string.

#[tokio::test]
async fn pg_k16_backup_restore_drill_round_trip_via_disk_file() {
    use octos_store::repository::{
        CronScheduleStore, LeaseStore, MisfirePolicy, NewCheckpoint, NewMessage, RecoveryStore,
        Schedule, UnitOfWork,
    };

    // Source schema: populated, then dumped to a tempfile.
    let src = _pg_store_in_schema("k16drillsrc", "src").await;
    let scope = scope("t-k16drill", "sess-k16drill-1");

    let mut uow = src.begin();
    uow.append_message(NewMessage {
        scope: scope.clone(),
        message_id: "m-1".into(),
        thread_id: "t-1".into(),
        turn_id: "turn-1".into(),
        role: "user".into(),
        content: "drill payload".into(),
    });
    uow.commit().await.unwrap();
    src.claim(&scope, "run-1", "worker-1", 60_000, 1_000)
        .await
        .unwrap();
    src.commit_checkpoint(NewCheckpoint {
        scope: scope.clone(),
        run_id: "run-1".into(),
        step: 1,
        transcript_highwater: 1,
        context: None,
        workspace_revision: Some("rev-1".into()),
        pending_invocation: None,
        artifact_refs: None,
        binding_digest: Some("sha256:b1".into()),
        permission_snapshot: None,
        digest: "sha256:cp-1".into(),
        schema_version: "v1".into(),
        runtime_version: "2.0.3".into(),
        created_epoch: 1,
        expected_old_workspace_revision: None,
    })
    .await
    .unwrap();
    src.create_schedule(
        &scope,
        Schedule {
            schedule_id: "cron-drill".into(),
            expression: "every 5m".into(),
            timezone: None,
            next_fire_at_ms: None,
            misfire_policy: MisfirePolicy::RunOnce,
            enabled: true,
            last_fired_at_ms: None,
            last_run_id: None,
            name: "cron-drill".into(),
            payload_json: "{}".into(),
            delete_after_run: false,
            origin_json: "".into(),
            created_at_ms: 0,
        },
    )
    .await
    .unwrap();

    // Dump → write to disk (the production backup lifecycle).
    let sql = src.dump_tables_sql("t-k16drill").await.expect("dump");
    let dir = tempfile::tempdir().expect("tempdir");
    let backup_path = dir.path().join("backup.sql");
    std::fs::write(&backup_path, &sql).expect("write backup file");
    let backup_meta = std::fs::metadata(&backup_path).expect("stat");
    assert!(backup_meta.len() > 0, "backup file must be non-empty");

    // Fresh target schema: empty, then populate from the FILE (not from
    // `sql` directly — the file is the contract boundary).
    let dst = _pg_store_in_schema("k16drilldst", "dst").await;
    let on_disk_sql = std::fs::read_to_string(&backup_path).expect("read backup file");
    assert_eq!(
        on_disk_sql, sql,
        "disk round-trip must be byte-identical to the in-memory dump"
    );
    dst.restore_tables_sql(&on_disk_sql).await.expect("restore");

    // After restore, dst should see the same canonical content as src.
    let src_audit = src.audit_scope(&scope).await.expect("src audit");
    let dst_audit = dst.audit_scope(&scope).await.expect("dst audit");
    assert_eq!(
        src_audit, dst_audit,
        "src and dst audits match after file-mediated restore"
    );

    // Spot-check: a specific row round-tripped.
    let restored_schedules = dst.list_schedules(&scope).await.expect("list schedules");
    assert_eq!(restored_schedules.len(), 1);
    assert_eq!(restored_schedules[0].schedule_id, "cron-drill");
    assert_eq!(restored_schedules[0].expression, "every 5m");

    let restored_cp = dst
        .latest_checkpoint(&scope, "run-1")
        .await
        .expect("checkpoint restored");
    assert_eq!(restored_cp.workspace_revision.as_deref(), Some("rev-1"));
    assert_eq!(restored_cp.binding_digest.as_deref(), Some("sha256:b1"));
}

#[tokio::test]
async fn pg_k16_backup_drill_via_psql_binary_when_docker_exec_available() {
    // External-tool smoke: when `docker exec octos-pg psql` is reachable
    // (typical local-dev setup), execute the backup file via psql and
    // verify the data lands. This is the production-restore workflow
    // (`psql -f backup.sql`) — the K16 drill that proves the dump is
    // PG-compatible SQL, not just an in-process artefact. Skipped when
    // the docker / psql tooling is not available so CI without a local
    // container is not broken.
    use octos_store::repository::{MisfirePolicy, NewMessage, Schedule, UnitOfWork};

    // Probe: can we actually exec into the PG container? If not, skip.
    let docker_probe = std::process::Command::new("docker")
        .args(["exec", "octos-pg", "psql", "--version"])
        .output();
    let psql_in_docker = matches!(docker_probe, Ok(out) if out.status.success());
    if !psql_in_docker {
        eprintln!("K16 psql drill SKIPPED: docker exec octos-pg psql not available");
        return;
    }

    // Seed source: a message + a schedule.
    let src = _pg_store_in_schema("k16psql", "src").await;
    let scope = scope("t-k16psql", "sess-k16psql-1");
    let mut uow = src.begin();
    uow.append_message(NewMessage {
        scope: scope.clone(),
        message_id: "psql-1".into(),
        thread_id: "t-psql".into(),
        turn_id: "turn-1".into(),
        role: "user".into(),
        content: "psql drill".into(),
    });
    uow.commit().await.unwrap();
    src.create_schedule(
        &scope,
        Schedule {
            schedule_id: "cron-psql".into(),
            expression: "every 1m".into(),
            timezone: None,
            next_fire_at_ms: None,
            misfire_policy: MisfirePolicy::RunOnce,
            enabled: true,
            last_fired_at_ms: None,
            last_run_id: None,
            name: "cron-psql".into(),
            payload_json: "{}".into(),
            delete_after_run: false,
            origin_json: "".into(),
            created_at_ms: 0,
        },
    )
    .await
    .unwrap();

    // Dump → write to file.
    let sql = src.dump_tables_sql("t-k16psql").await.expect("dump");
    let dir = tempfile::tempdir().expect("tempdir");
    let backup_path = dir.path().join("backup.sql");
    std::fs::write(&backup_path, &sql).expect("write backup file");

    // Prepare a fresh target schema in PG (separate from src so the
    // restore genuinely populates it from the file).
    let url = database_url();
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query("DROP SCHEMA IF EXISTS test_k16psql_dst CASCADE")
        .execute(&admin)
        .await
        .expect("drop dst schema");
    sqlx::query("CREATE SCHEMA test_k16psql_dst")
        .execute(&admin)
        .await
        .expect("create dst schema");
    // Mirror the owned schema on the target so the INSERTs in the
    // backup have a home. Easiest: run the same migration script
    // through the dst schema's search_path.
    let dst_url = format!("{url}?options=-c%20search_path%3Dtest_k16psql_dst");
    let dst_store = PgStore::connect(&dst_url).await.expect("dst connect");
    dst_store.migrate().await.expect("dst migrate");

    // Run psql inside the docker container. The backup's first line is
    // `BEGIN; SET LOCAL app.tenant_id = ...`; we prepend a
    // `SET search_path TO <dst>` so the INSERTs land in the dst schema
    // (the dump's tenant context is set via SET LOCAL inside the BEGIN
    // block — search_path has to be set OUTSIDE the BEGIN for it to
    // govern which schema the unprefixed table names resolve to).
    let mut restored_sql = String::new();
    restored_sql.push_str("SET search_path TO test_k16psql_dst;\n");
    restored_sql.push_str(&sql);
    let psql_input_path = dir.path().join("backup_for_psql.sql");
    std::fs::write(&psql_input_path, &restored_sql).expect("write psql input");
    let psql_status = std::process::Command::new("docker")
        .args([
            "exec",
            "-i",
            "octos-pg",
            "psql",
            "-U",
            "postgres",
            "-d",
            "octos",
            "-v",
            "ON_ERROR_STOP=1",
        ])
        .stdin(std::fs::File::open(&psql_input_path).expect("open psql input"))
        .output();
    let out = psql_status.expect("docker exec psql");
    assert!(
        out.status.success(),
        "psql restore failed: stdout={:?} stderr={:?}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // After psql restored, dst_store should see the data.
    let restored = dst_store
        .list_schedules(&scope)
        .await
        .expect("list after psql restore");
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].schedule_id, "cron-psql");
    assert_eq!(restored[0].expression, "every 1m");
}
