//! Entry-point wiring from the authenticated connection to the cluster
//! `Scope`/`Execution` value objects (task c1, UPCR-2026-030).
//!
//! The authoritative types live in `octos_core::execution_scope`; this
//! module is the octos-cli adapter that builds them from what the serve
//! path actually knows: the authenticated connection profile, the
//! tenant resolved from the host/headers, and the wire `SessionKey`.
//! Downstream consumers receive the `Scope` and MUST NOT re-derive
//! tenant/profile from client payloads (ADR cluster-state-and-execution
//! D2). Until the ledger/registry reads switch to `Scope` keys (c2), the
//! scope rides alongside the existing SessionKey-based flow.

use octos_core::SessionKey;
use octos_core::execution_scope::{
    AuthenticatedIdentity, Execution, Scope, ScopeError, bind_scope,
};

/// The scope context for one authenticated connection's turn: the bound
/// [`Scope`] plus the [`Execution`] for the run that owns the turn.
#[derive(Debug, Clone)]
pub struct TurnScopeContext {
    pub scope: Scope,
    pub execution: Execution,
}

/// Bind the authenticated identity + wire session to a cluster `Scope`.
///
/// `tenant_id` is the tenant resolved from the request host/headers (the
/// frps/AppUi tenant, NOT any client-submitted field); `profile_id` is
/// the authenticated connection profile. `workspace_hint` is the client
/// cwd, recorded only as a runner mapping location.
pub fn bind_connection_scope(
    tenant_id: &str,
    profile_id: &str,
    wire_session_id: &SessionKey,
    workspace_hint: Option<&str>,
) -> Result<Scope, ScopeError> {
    let identity = AuthenticatedIdentity {
        tenant_id: tenant_id.to_string(),
        profile_id: profile_id.to_string(),
    };
    bind_scope(&identity, &wire_session_id.0, workspace_hint)
}

/// Bind the scope and open the first execution of a turn on it.
pub fn open_turn_scope(
    tenant_id: &str,
    profile_id: &str,
    wire_session_id: &SessionKey,
    thread_id: &str,
    workspace_hint: Option<&str>,
) -> Result<TurnScopeContext, ScopeError> {
    let scope = bind_connection_scope(tenant_id, profile_id, wire_session_id, workspace_hint)?;
    let execution = Execution::new(&scope, thread_id);
    Ok(TurnScopeContext { scope, execution })
}

#[cfg(test)]
mod tests {
    use super::*;
    use octos_core::execution_scope::ScopeError;

    /// Spec c1 / Rule scope-binding: same wire session under two tenants
    /// binds to distinct scopes through the octos-cli adapter too (K07).
    #[test]
    fn bind_connection_scope_distinguishes_tenants_on_same_wire_session() {
        let wire = SessionKey::new("web", "tab-1");
        let a = bind_connection_scope("tenant-a", "profile-a", &wire, None).unwrap();
        let b = bind_connection_scope("tenant-b", "profile-a", &wire, None).unwrap();
        assert_ne!(a, b);
        assert_eq!(a.tenant_id(), "tenant-a");
        assert_eq!(b.tenant_id(), "tenant-b");
        assert_eq!(a.session_id(), wire.0);
    }

    /// Spec c1: a traversal cwd hint is rejected; a plain one is a runner
    /// mapping, never the workspace identity.
    #[test]
    fn bind_connection_scope_treats_cwd_as_mapping_not_identity() {
        let wire = SessionKey::new("web", "tab-1");
        assert!(matches!(
            bind_connection_scope("tenant-a", "profile-a", &wire, Some("../../etc")),
            Err(ScopeError::UnsafeWorkspaceHint(_))
        ));
        let scope = bind_connection_scope("tenant-a", "profile-a", &wire, Some("proj")).unwrap();
        assert_eq!(scope.workspace_hint(), Some("proj"));
        assert_ne!(scope.workspace_id(), "proj");
    }

    /// Spec c1: open_turn_scope yields a Scope + first-attempt Execution whose
    /// run_id is server-allocated and stable across attempts.
    #[test]
    fn open_turn_scope_binds_scope_and_first_attempt_execution() {
        let wire = SessionKey::with_profile("profile-a", "web", "tab-1");
        let ctx = open_turn_scope("tenant-a", "profile-a", &wire, "thread-1", None).unwrap();
        assert_eq!(ctx.execution.scope(), &ctx.scope);
        assert_eq!(ctx.execution.thread_id(), "thread-1");
        assert_eq!(ctx.execution.attempt_id(), 1);
        assert!(!ctx.execution.run_id().is_empty());
        let retry = ctx.execution.next_attempt();
        assert_eq!(retry.run_id(), ctx.execution.run_id());
        assert_eq!(retry.attempt_id(), 2);
    }

    /// Spec c1: unauthenticated (empty tenant) cannot open a turn scope.
    #[test]
    fn open_turn_scope_rejects_unauthenticated_tenant() {
        let wire = SessionKey::new("web", "tab-1");
        assert!(matches!(
            open_turn_scope("", "profile-a", &wire, "thread-1", None),
            Err(ScopeError::Unauthenticated)
        ));
    }
}
