//! # ExecutionScope — the authoritative cluster scope contract
//!
//! Cluster deployments need an unambiguous internal scope that wire-level
//! session IDs cannot provide: two tenants may legitimately present the
//! same wire session ID, and a client-supplied cwd string is a runner
//! mapping location, never an identity. This module defines the value
//! objects from `docs/adr/cluster-state-and-execution.md` (D2):
//!
//! ```text
//! Scope     = tenant_id + profile_id + workspace_id + session_id
//! Execution = Scope + thread_id + run_id + attempt_id
//! ```
//!
//! Binding happens ONCE at the authenticated entry point
//! ([`bind_scope`]); downstream components (repositories, caches,
//! broadcast, approvals, tools, object-store keys, audit) receive the
//! `Scope`/`Execution` and MUST NOT re-derive tenant/profile from global
//! state or client payloads. Like [`crate::session_scope`], this module
//! contains only types and validating constructors — wiring the entry
//! points onto it lands in the c1 integration commits.

use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

/// Process-local counter for server-allocated identity components. Scoped
/// uniqueness is all the value objects promise; cross-process uniqueness
/// comes from the database sequences in c2, not from this counter.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_server_id(prefix: &str) -> String {
    let n = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{n:016x}-{:016x}", std::process::id())
}

/// Errors produced while binding an authenticated identity to a scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeError {
    /// The identity carries no tenant (unauthenticated or anonymous).
    Unauthenticated,
    /// The identity carries no profile.
    MissingProfile,
    /// The wire session ID was empty or whitespace.
    EmptySessionId,
    /// The client-supplied workspace hint is unsafe (traversal, absolute,
    /// or separator-bearing) and cannot be recorded even as a mapping.
    UnsafeWorkspaceHint(String),
}

impl fmt::Display for ScopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScopeError::Unauthenticated => write!(f, "unauthenticated identity: no tenant_id"),
            ScopeError::MissingProfile => write!(f, "identity carries no profile_id"),
            ScopeError::EmptySessionId => write!(f, "wire session id is empty"),
            ScopeError::UnsafeWorkspaceHint(h) => {
                write!(f, "unsafe client workspace hint rejected: {h:?}")
            }
        }
    }
}

impl std::error::Error for ScopeError {}

/// The authenticated identity as derived from request credentials. This
/// is the ONLY legitimate source of tenant/profile for scope binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedIdentity {
    pub tenant_id: String,
    pub profile_id: String,
}

impl AuthenticatedIdentity {
    /// An identity carrying no tenant/profile — used to assert that
    /// anonymous requests cannot mint scopes.
    pub fn anonymous() -> Option<Self> {
        None
    }

    /// True when the identity is usable for scope binding.
    pub fn is_authenticated(&self) -> bool {
        !self.tenant_id.trim().is_empty() && !self.profile_id.trim().is_empty()
    }
}

/// Validate a client-supplied workspace hint (cwd). Returns the hint as a
/// runner mapping location when it is a plain relative name; rejects
/// traversal, absolute paths, and separator-bearing hints. The hint never
/// becomes the `workspace_id`.
fn validate_workspace_hint(hint: &str) -> Result<&str, ScopeError> {
    let h = hint.trim();
    if h.is_empty() {
        return Ok("");
    }
    let unsafe_hint = h.starts_with('/')
        || h.starts_with('\\')
        || h.contains("..")
        || h.contains('/')
        || h.contains('\\')
        || h.contains(':')
        || h.chars().any(char::is_control);
    if unsafe_hint {
        return Err(ScopeError::UnsafeWorkspaceHint(hint.to_string()));
    }
    Ok(h)
}

/// The authoritative cluster scope: `tenant_id + profile_id +
/// workspace_id + session_id`. Immutable once bound; equality and hashing
/// cover all four components so same-wire-session collisions across
/// tenants are impossible by construction (K07).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Scope {
    tenant_id: String,
    profile_id: String,
    /// Server-allocated logical workspace identity — NEVER the client
    /// cwd string.
    workspace_id: String,
    /// The wire session ID, kept as one scope component only.
    session_id: String,
    /// Client cwd hint, recorded as a runner mapping location only.
    workspace_hint: Option<String>,
}

impl Scope {
    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }
    pub fn workspace_id(&self) -> &str {
        &self.workspace_id
    }
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
    /// The runner mapping location (client cwd hint), if one was
    /// recorded. Not an identity.
    pub fn workspace_hint(&self) -> Option<&str> {
        self.workspace_hint.as_deref()
    }
}

/// Bind an authenticated identity plus a wire session ID to a `Scope`.
/// This is the single entry-point constructor: it validates the identity,
/// allocates a server-side `workspace_id`, and records any client
/// workspace hint as a runner mapping (never identity).
pub fn bind_scope(
    identity: &AuthenticatedIdentity,
    wire_session_id: &str,
    workspace_hint: Option<&str>,
) -> Result<Scope, ScopeError> {
    if identity.tenant_id.trim().is_empty() {
        return Err(ScopeError::Unauthenticated);
    }
    if identity.profile_id.trim().is_empty() {
        return Err(ScopeError::MissingProfile);
    }
    let session_id = wire_session_id.trim();
    if session_id.is_empty() {
        return Err(ScopeError::EmptySessionId);
    }
    let hint = match workspace_hint {
        Some(h) => {
            let v = validate_workspace_hint(h)?;
            if v.is_empty() {
                None
            } else {
                Some(v.to_string())
            }
        }
        None => None,
    };
    Ok(Scope {
        tenant_id: identity.tenant_id.clone(),
        profile_id: identity.profile_id.clone(),
        workspace_id: next_server_id("ws"),
        session_id: session_id.to_string(),
        workspace_hint: hint,
    })
}

/// An execution within a scope: `Scope + thread_id + run_id + attempt_id`.
/// `run_id` is server-allocated; `attempt_id` starts at 1 and increments
/// monotonically per retry within the same run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution {
    scope: Scope,
    thread_id: String,
    run_id: String,
    attempt_id: u64,
}

impl Execution {
    pub fn new(scope: &Scope, thread_id: &str) -> Self {
        Self {
            scope: scope.clone(),
            thread_id: thread_id.to_string(),
            run_id: next_server_id("run"),
            attempt_id: 1,
        }
    }

    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
    pub fn attempt_id(&self) -> u64 {
        self.attempt_id
    }

    /// The next attempt of the same run (retry/接管); `run_id` is stable.
    pub fn next_attempt(&self) -> Self {
        Self {
            attempt_id: self.attempt_id + 1,
            ..self.clone()
        }
    }
}

/// Verdict of the idempotency-key check for an incoming request (K01).
/// This is the type-level pure function; the persistent unique
/// constraint lands in c2 (`commands` table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdempotencyVerdict {
    /// No record under this key — create a new command/run.
    Fresh,
    /// Same key and same payload hash — return the existing run/state.
    Replay,
    /// Same key but a different payload hash — reject as a conflict.
    Conflict,
}

/// Classify an incoming request against the stored payload hash for its
/// idempotency key. `stored_payload_hash` is `None` when the key has
/// never been seen.
pub fn classify_idempotent_submit(
    _idempotency_key: &str,
    payload_hash: &str,
    stored_payload_hash: Option<&str>,
) -> IdempotencyVerdict {
    match stored_payload_hash {
        None => IdempotencyVerdict::Fresh,
        Some(stored) if stored == payload_hash => IdempotencyVerdict::Replay,
        Some(_) => IdempotencyVerdict::Conflict,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsafe_hints_rejected() {
        for h in ["../x", "/abs", "a/b", "a\\b", "C:\\x", "..", "a:b"] {
            assert!(
                validate_workspace_hint(h).is_err(),
                "hint {h:?} must be rejected"
            );
        }
    }

    #[test]
    fn plain_relative_hint_accepted_as_mapping() {
        assert_eq!(validate_workspace_hint("myproj").unwrap(), "myproj");
    }
}
