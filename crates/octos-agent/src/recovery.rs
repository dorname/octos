//! Run recovery from a committed checkpoint (c3 / D3, checkpoint-resume).
//!
//! ADR `docs/adr/cluster-state-and-execution.md` D3; spec
//! `specs/task-c3-recoverable-execution-leases.spec.md` (checkpoint-resume,
//! corrupted_checkpoint_digest_fails_closed, binding-pin).
//!
//! When a worker dies and another takes over (new lease epoch), the new worker
//! rebuilds the run's execution context from the latest COMMITTED checkpoint —
//! never from in-process memory. This module is the agent-side recovery
//! component: it validates checkpoint integrity (fail closed on a corrupt
//! digest), restores the transcript high-water mark and pinned binding, and
//! decides whether an in-flight tool side effect is reused or reconciled.
//!
//! The durable store stays behind [`CheckpointSource`] so `octos-agent` keeps
//! no database dependency; `octos-cli` implements it over octos-store.

use std::sync::Arc;

/// A committed checkpoint as read from the durable store (c3 schema).
#[derive(Debug, Clone)]
pub struct RecoveryCheckpoint {
    pub step: u64,
    pub transcript_highwater: u64,
    pub context: Option<serde_json::Value>,
    pub workspace_revision: Option<String>,
    pub pending_invocation: Option<String>,
    pub binding_digest: Option<String>,
    pub permission_snapshot: Option<serde_json::Value>,
    /// Integrity digest over the checkpoint payload; a mismatch fails closed.
    pub digest: String,
    pub schema_version: String,
    pub runtime_version: String,
}

/// Source of committed checkpoints (the durable `RecoveryStore` in octos-cli).
#[async_trait::async_trait]
#[async_trait::async_trait]
pub trait CheckpointSource: Send + Sync {
    /// The latest valid checkpoint for a run, if any.
    async fn latest(&self, run_id: &str) -> Option<RecoveryCheckpoint>;
}

/// Why a checkpoint cannot be used for recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryError {
    /// No checkpoint committed yet — nothing to recover from.
    NoCheckpoint,
    /// The integrity digest does not match the payload — fail closed, do NOT
    /// guess the content (spec: corrupted_checkpoint_digest_fails_closed).
    CorruptDigest,
    /// The checkpoint's schema/runtime version is incompatible with this
    /// runtime — fail closed rather than mis-interpret the payload.
    IncompatibleVersion { found: String },
}

impl std::fmt::Display for RecoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecoveryError::NoCheckpoint => write!(f, "no committed checkpoint"),
            RecoveryError::CorruptDigest => write!(f, "checkpoint digest mismatch"),
            RecoveryError::IncompatibleVersion { found } => {
                write!(f, "incompatible checkpoint schema version: {found}")
            }
        }
    }
}
impl std::error::Error for RecoveryError {}

/// The recovered execution context a takeover resumes from.
#[derive(Debug, Clone)]
pub struct RecoveredRun {
    /// Rebuild the transcript/context up to this high-water mark — confirmed
    /// tool results before it are reused, not re-executed (K04).
    pub transcript_highwater: u64,
    /// The step the run resumes after.
    pub resume_step: u64,
    /// The pinned binding the run MUST continue under (K11) — even if the
    /// Binding was updated after the run started.
    pub binding_digest: Option<String>,
    pub permission_snapshot: Option<serde_json::Value>,
    pub workspace_revision: Option<String>,
    pub context: Option<serde_json::Value>,
    /// An in-flight tool invocation at crash time, to consult the side-effect
    /// ledger for (reuse confirmed / reconcile unknown — never blind retry).
    pub pending_invocation: Option<String>,
}

/// Compute the integrity digest over a checkpoint payload. The durable store
/// stores this alongside; recovery recomputes it and fails closed on mismatch.
pub fn checkpoint_digest(cp: &RecoveryCheckpoint) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(cp.step.to_le_bytes());
    h.update(cp.transcript_highwater.to_le_bytes());
    if let Some(c) = &cp.context {
        h.update(c.to_string().as_bytes());
    }
    if let Some(w) = &cp.workspace_revision {
        h.update(w.as_bytes());
    }
    if let Some(b) = &cp.binding_digest {
        h.update(b.as_bytes());
    }
    format!("sha256:{:x}", h.finalize())
}

/// Recover a run from its latest committed checkpoint.
///
/// Fails closed on a corrupt digest or incompatible schema version — a
/// takeover never guesses a damaged checkpoint into a phantom completion
/// (K15). `current_schema_version` is this runtime's checkpoint schema; a
/// mismatch is rejected.
pub fn recover_run(
    cp: RecoveryCheckpoint,
    current_schema_version: &str,
) -> Result<RecoveredRun, RecoveryError> {
    // Integrity first: recompute and compare the digest.
    if checkpoint_digest(&cp) != cp.digest {
        return Err(RecoveryError::CorruptDigest);
    }
    // Version compatibility: refuse to interpret a foreign schema.
    if cp.schema_version != current_schema_version {
        return Err(RecoveryError::IncompatibleVersion {
            found: cp.schema_version.clone(),
        });
    }
    Ok(RecoveredRun {
        transcript_highwater: cp.transcript_highwater,
        resume_step: cp.step + 1,
        binding_digest: cp.binding_digest,
        permission_snapshot: cp.permission_snapshot,
        workspace_revision: cp.workspace_revision,
        context: cp.context,
        pending_invocation: cp.pending_invocation,
    })
}

/// Convenience: recover from a [`CheckpointSource`], mapping "no checkpoint"
/// to [`RecoveryError::NoCheckpoint`].
pub async fn recover_run_from(
    source: &Arc<dyn CheckpointSource>,
    run_id: &str,
    current_schema_version: &str,
) -> Result<RecoveredRun, RecoveryError> {
    let cp = source
        .latest(run_id)
        .await
        .ok_or(RecoveryError::NoCheckpoint)?;
    recover_run(cp, current_schema_version)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_cp() -> RecoveryCheckpoint {
        let mut cp = RecoveryCheckpoint {
            step: 3,
            transcript_highwater: 30,
            context: Some(serde_json::json!({"summary": "s"})),
            workspace_revision: Some("rev-3".into()),
            pending_invocation: None,
            binding_digest: Some("sha256:binding-v1".into()),
            permission_snapshot: Some(serde_json::json!({"allow": ["read"]})),
            digest: String::new(),
            schema_version: "v1".into(),
            runtime_version: "2.0.3".into(),
        };
        cp.digest = checkpoint_digest(&cp);
        cp
    }

    /// checkpoint-resume: a valid checkpoint yields a RecoveredRun resuming
    /// after its step with the pinned binding intact (K11).
    #[test]
    fn recovers_from_valid_checkpoint_with_pinned_binding() {
        let cp = valid_cp();
        let recovered = recover_run(cp, "v1").expect("valid checkpoint recovers");
        assert_eq!(recovered.transcript_highwater, 30);
        assert_eq!(recovered.resume_step, 4);
        assert_eq!(
            recovered.binding_digest.as_deref(),
            Some("sha256:binding-v1")
        );
        assert_eq!(recovered.workspace_revision.as_deref(), Some("rev-3"));
    }

    /// corrupted_checkpoint_digest_fails_closed: a tampered checkpoint is
    /// rejected, never guessed.
    #[test]
    fn corrupt_checkpoint_digest_fails_closed() {
        let mut cp = valid_cp();
        cp.transcript_highwater = 999; // tamper after digest was computed
        let err = recover_run(cp, "v1").unwrap_err();
        assert_eq!(err, RecoveryError::CorruptDigest);
    }

    /// An incompatible schema version fails closed (K15: no phantom resume).
    #[test]
    fn incompatible_schema_version_fails_closed() {
        let cp = valid_cp();
        let err = recover_run(cp, "v2").unwrap_err();
        assert!(matches!(err, RecoveryError::IncompatibleVersion { .. }));
    }
}
