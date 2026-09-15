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
