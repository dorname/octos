//! RED tests for `execution_scope` — task c1-cluster-scope-execution-context.
//!
//! These tests pin the contract of the cluster Scope / Execution value
//! objects and the idempotency-key verdict pure function BEFORE the module
//! exists. Spec: specs/task-c1-cluster-scope-execution-context.spec.md.

use octos_core::execution_scope::{
    Execution, IdempotencyVerdict, ScopeError, bind_scope, classify_idempotent_submit,
};

fn authenticated() -> octos_core::execution_scope::AuthenticatedIdentity {
    octos_core::execution_scope::AuthenticatedIdentity {
        tenant_id: "tenant-a".into(),
        profile_id: "profile-a".into(),
    }
}

// --- Rule: scope-binding ------------------------------------------------

/// Scenario: 同一 wire session 在两个租户下解析为不同 Scope (K07)
#[test]
fn scope_binding_distinguishes_tenants_with_same_wire_session() {
    let a = bind_scope(&authenticated(), "sess-1", Some("proj")).unwrap();
    let b = bind_scope(
        &octos_core::execution_scope::AuthenticatedIdentity {
            tenant_id: "tenant-b".into(),
            profile_id: "profile-a".into(),
        },
        "sess-1",
        Some("proj"),
    )
    .unwrap();
    assert_ne!(a, b, "same wire session across tenants must not collide");
    assert_eq!(a.session_id(), "sess-1");
    assert_eq!(b.session_id(), "sess-1");
    assert_ne!(a.tenant_id(), b.tenant_id());
}

/// Scenario: 客户端提交的 workspace 字符串不被当作身份
#[test]
fn scope_binding_rejects_client_supplied_workspace_identity() {
    let err = bind_scope(&authenticated(), "sess-1", Some("../etc/passwd"));
    assert!(err.is_err(), "traversal workspace hints must be rejected");
    // A relative cwd hint is recorded as a runner mapping, not identity:
    let scope = bind_scope(&authenticated(), "sess-1", Some("myproj")).unwrap();
    assert_ne!(
        scope.workspace_id(),
        "myproj",
        "workspace_id is server-allocated, never the client cwd string"
    );
}

/// Scenario: 未认证请求不构造 Scope
#[test]
fn scope_binding_requires_authenticated_identity() {
    let anon = octos_core::execution_scope::AuthenticatedIdentity::anonymous();
    assert!(
        anon.is_none(),
        "anonymous identity carries no tenant/profile"
    );
    assert!(matches!(
        bind_scope(
            &octos_core::execution_scope::AuthenticatedIdentity {
                tenant_id: String::new(),
                profile_id: "p".into(),
            },
            "sess-1",
            None,
        ),
        Err(ScopeError::Unauthenticated)
    ));
}

/// Scenario: Execution 派生自 Scope 且 attempt 由服务端生成
#[test]
fn execution_derives_from_scope_with_server_allocated_ids() {
    let scope = bind_scope(&authenticated(), "sess-1", None).unwrap();
    let exec = Execution::new(&scope, "thread-1");
    assert_eq!(exec.scope(), &scope);
    assert!(!exec.run_id().is_empty());
    assert_eq!(exec.attempt_id(), 1);
    let retried = exec.next_attempt();
    assert_eq!(retried.attempt_id(), 2);
    assert_eq!(retried.run_id(), exec.run_id());
}

// --- Rule: idempotent-submit (K01, type-level verdict; storage in c2) ----

/// Scenario: 同键同 payload 判重放
#[test]
fn idempotency_same_key_same_payload_is_replay() {
    assert_eq!(
        classify_idempotent_submit("req-1", "hash-a", Some("hash-a")),
        IdempotencyVerdict::Replay
    );
}

/// Scenario: 同一幂等键不同 payload 判冲突 (critical, K01)
#[test]
fn idempotency_key_conflict_on_different_payload() {
    assert_eq!(
        classify_idempotent_submit("req-1", "hash-b", Some("hash-a")),
        IdempotencyVerdict::Conflict
    );
}

/// Scenario: 新键判新建
#[test]
fn idempotency_new_key_is_fresh() {
    assert_eq!(
        classify_idempotent_submit("req-9", "hash-a", None),
        IdempotencyVerdict::Fresh
    );
}
