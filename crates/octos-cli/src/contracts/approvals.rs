use std::collections::HashMap;
use std::sync::RwLock;

use octos_core::SessionKey;
use octos_core::execution_scope::Scope;
use octos_core::ui_protocol::{
    ApprovalDecidedEvent, ApprovalDecision, ApprovalId, ApprovalRequestedEvent,
    ApprovalRespondParams, ApprovalRespondResult, RpcError, TurnId, methods, rpc_error_codes,
};
use octos_store::repository::ApprovalState;
use serde_json::json;

#[derive(Debug)]
struct ApprovalEntry {
    session_id: SessionKey,
    state: ApprovalEntryState,
    request: Option<ApprovalRequestedEvent>,
    /// CAS args_hash the durable reply compares on. Populated from
    /// `args_hash_for(request)` at request_runtime time so a rehydrated
    /// entry (no `request` clone available — durable record carries
    /// only the hash) can pass the CAS unchanged. `None` for the legacy
    /// `insert_pending` test path that never carried a request.
    args_hash: Option<String>,
    runtime_resumable: bool,
    response_tx: Option<tokio::sync::oneshot::Sender<ApprovalDecision>>,
}

#[derive(Debug)]
enum ApprovalEntryState {
    #[allow(dead_code)]
    Pending,
    Responded {
        decision: ApprovalDecision,
    },
    /// The server administratively cancelled this approval before any client
    /// could respond. Late `respond` calls now return a typed error so the
    /// client can distinguish "moot" from "approved/denied".
    Cancelled {
        reason: String,
    },
}

/// One cancelled approval surfaced by [`PendingApprovalStore::cancel_pending_for_turn`].
#[derive(Debug, Clone)]
pub(crate) struct CancelledApproval {
    pub(crate) approval_id: ApprovalId,
    pub(crate) turn_id: TurnId,
}

#[derive(Default)]
pub(crate) struct PendingApprovalStore {
    entries: RwLock<HashMap<ApprovalId, ApprovalEntry>>,
    /// c2 durable backend (K05): when present, every request is persisted as
    /// a durable Pending record and every respond first wins the repository
    /// CAS before waking the in-process oneshot. `None` preserves the
    /// single-node in-process behavior exactly (local adapter / tests).
    ///
    /// Interior `OnceLock`: the process-global store is shared behind `&self`
    /// (every connection's requester), so the durable backend is attached by
    /// reference at serve startup — set once, never changed afterwards.
    durable: std::sync::OnceLock<DurableApprovalSink>,
}

/// Resolves the wire `SessionKey` to the authoritative cluster `Scope` (c1).
/// Production binds the connection's entry scope; the resolution must be
/// STABLE for a session (bind_scope allocates a fresh workspace_id per call).
pub(crate) type ScopeResolver = Box<dyn Fn(&SessionKey) -> Option<Scope> + Send + Sync>;

/// The durable side of the approval lifecycle, injected at serve startup.
/// Backend-agnostic: the local adapter and PostgreSQL both implement
/// [`ApprovalDurableStore`]. `scope_for` resolves the wire `SessionKey` to
/// the authoritative cluster `Scope` (c1).
pub(crate) struct DurableApprovalSink {
    store: std::sync::Arc<dyn ApprovalDurableStore>,
    scope_for: ScopeResolver,
}

/// Narrow durable contract for approvals (c2/K05). Implemented over the
/// repository Unit of Work by each backend; synchronous so the in-process
/// store stays lock-free around its own RwLock (the PG impl bridges its
/// async I/O off the Tokio runtime per spec rule migration-safety).
pub(crate) trait ApprovalDurableStore: Send + Sync {
    /// Persist a freshly-requested approval as durable Pending.
    fn persist_pending(&self, scope: &Scope, record: DurableApprovalRecord);
    /// Reply CAS: first matching reply wins; replay/cross-scope/args-tamper
    /// rejected. Returns the resulting durable state. The decision uses the
    /// repository's own type so the durable layer never depends on the wire
    /// protocol enum (which has a forward-compat `Unknown` arm the durable
    /// record must not store).
    fn reply(
        &self,
        scope: &Scope,
        approval_id: &str,
        args_hash: &str,
        decision: octos_store::repository::ApprovalDecision,
    ) -> Result<ApprovalState, octos_store::repository::RepositoryError>;
    /// The durable record, for resume-after-restart. Used by the recovery
    /// path that re-registers a persisted Pending approval after a pod
    /// restart (wired when c3 recovery lands); kept in the contract now so
    /// the backend trait is complete for both adapters.
    #[allow(dead_code)]
    fn get(&self, scope: &Scope, approval_id: &str) -> Option<DurableApprovalRecord>;
}

/// A durable approval record (the truth; the oneshot is only the accelerator).
/// Fields are consumed by `persist_pending` (write) and the recovery path via
/// `ApprovalDurableStore::get` (read) — the reader lands with c3 recovery.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct DurableApprovalRecord {
    pub(crate) approval_id: String,
    pub(crate) originating_run: String,
    pub(crate) args_hash: String,
    pub(crate) binding_revision: String,
    pub(crate) state: ApprovalState,
}

impl std::fmt::Debug for DurableApprovalSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DurableApprovalSink")
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for PendingApprovalStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingApprovalStore")
            .finish_non_exhaustive()
    }
}

/// Context recovered from the original `ApprovalRequestedEvent` at `respond`
/// time. The scope policy needs `tool_name` and `turn_id` to decide what
/// `MatchKey` to record under, but the client's `ApprovalRespondParams`
/// carries only `approval_id` — we therefore lift the missing fields off the
/// stored entry and hand them back to the caller alongside the existing
/// result. `None` for the legacy `insert_pending` path that never carried
/// a request.
#[derive(Debug, Clone)]
pub(crate) struct RespondedApprovalContext {
    pub(crate) tool_name: String,
    pub(crate) turn_id: TurnId,
}

#[derive(Debug, Clone)]
pub(crate) struct RespondOutcome {
    pub(crate) result: ApprovalRespondResult,
    pub(crate) context: Option<RespondedApprovalContext>,
}

impl PendingApprovalStore {
    /// Decide an approval and snapshot the metadata the audit/ledger path
    /// needs. Equivalent to [`Self::respond`] for callers that don't care
    /// about the captured request.
    pub(crate) fn respond_with_context(
        &self,
        params: ApprovalRespondParams,
    ) -> Result<RespondOutcome, RpcError> {
        let mut entries = self.entries.write().unwrap_or_else(|p| p.into_inner());
        let Some(entry) = entries.get_mut(&params.approval_id) else {
            return Err(approval_not_found_error(&params));
        };

        if entry.session_id != params.session_id {
            return Err(approval_not_found_error(&params));
        }

        match &entry.state {
            ApprovalEntryState::Pending => {
                // c2/K05: when a durable backend is attached, the repository
                // CAS is the decision of record — it must win BEFORE the
                // in-process oneshot is woken. A replay / cross-scope /
                // tampered reply is rejected by the CAS and never reaches the
                // parked runtime, even if the in-process entry was somehow
                // still Pending (e.g. after a restart recovered the entry).
                if let Some(sink) = self.durable.get()
                    && let Some(scope) = (sink.scope_for)(&params.session_id)
                {
                    let args_hash = entry
                        .args_hash
                        .clone()
                        .or_else(|| entry.request.as_ref().map(Self::args_hash_for))
                        .unwrap_or_default();
                    let repo_decision = match &params.decision {
                        ApprovalDecision::Approve => {
                            octos_store::repository::ApprovalDecision::Approved
                        }
                        ApprovalDecision::Deny => {
                            octos_store::repository::ApprovalDecision::Rejected
                        }
                        // Forward-compat unknown: never written durable.
                        ApprovalDecision::Unknown(_) => {
                            return Err(approval_not_pending_error(
                                &params,
                                params.decision.clone(),
                                entry.request.as_ref().map(|r| r.title.as_str()),
                            ));
                        }
                    };
                    let cas = sink.store.reply(
                        &scope,
                        params.approval_id.0.to_string().as_str(),
                        &args_hash,
                        repo_decision,
                    );
                    match cas {
                        Ok(ApprovalState::Decided) => {}
                        Ok(_) | Err(_) => {
                            return Err(approval_not_pending_error(
                                &params,
                                params.decision.clone(),
                                entry.request.as_ref().map(|r| r.title.as_str()),
                            ));
                        }
                    }
                }
                // FIX-01 made `ApprovalDecision` non-Copy (added `Unknown(String)`
                // for forward-compat); clone the decision out so we can both
                // store it on the entry and forward it to the runtime channel.
                let decision_for_state = params.decision.clone();
                let decision_for_runtime = params.decision.clone();
                entry.state = ApprovalEntryState::Responded {
                    decision: decision_for_state,
                };
                let runtime_resumed = entry
                    .response_tx
                    .take()
                    // FIX-01 made `ApprovalDecision` non-Copy; FIX-06 needs
                    // the decision to live across recording + return. Use
                    // the pre-cloned `decision_for_runtime`.
                    .is_some_and(|tx| tx.send(decision_for_runtime).is_ok());
                let context = entry
                    .request
                    .as_ref()
                    .map(|request| RespondedApprovalContext {
                        tool_name: request.tool_name.clone(),
                        turn_id: request.turn_id.clone(),
                    });
                Ok(RespondOutcome {
                    result: ApprovalRespondResult::accepted_with_runtime_resumed(
                        params.approval_id,
                        entry.runtime_resumable && runtime_resumed,
                    ),
                    context,
                })
            }
            ApprovalEntryState::Responded { decision } => {
                let request_title = entry.request.as_ref().map(|request| request.title.as_str());
                Err(approval_not_pending_error(
                    &params,
                    decision.clone(),
                    request_title,
                ))
            }
            ApprovalEntryState::Cancelled { reason } => Err(approval_cancelled_error(
                &params,
                reason,
                entry.request.as_ref().map(|request| &request.turn_id),
            )),
        }
    }

    #[cfg(test)]
    pub(crate) fn respond(
        &self,
        params: ApprovalRespondParams,
    ) -> Result<RespondOutcome, RpcError> {
        self.respond_with_context(params)
    }

    /// Atomically cancel every still-pending approval that belongs to the
    /// given turn. Idempotent: a second call after all entries are already
    /// `Cancelled`/`Responded` returns an empty list.
    ///
    /// FIX-06 interaction: this only touches per-call pending entries. Scope
    /// entries (`approve_for_session`) live in a separate store and are not
    /// affected here; `approve_for_turn` scopes are evicted by the caller via
    /// `evict_turn` already wired into `handle_turn_interrupt`.
    ///
    /// TODO(M9-FIX-07-followup): emit an audit entry per cancellation
    /// (`decision: "cancelled"` with `reason`) so the audit log mirrors the
    /// durable ledger. Out of scope for FIX-08 — flagged so a follow-up can
    /// pick it up without re-reading the spec.
    pub(crate) fn cancel_pending_for_turn(
        &self,
        session_id: &SessionKey,
        turn_id: &TurnId,
        reason: &str,
    ) -> Vec<CancelledApproval> {
        let mut entries = self.entries.write().unwrap_or_else(|p| p.into_inner());
        let mut cancelled = Vec::new();
        for (approval_id, entry) in entries.iter_mut() {
            if entry.session_id != *session_id {
                continue;
            }
            if !matches!(&entry.state, ApprovalEntryState::Pending) {
                continue;
            }
            let entry_turn_id = match entry.request.as_ref().map(|request| &request.turn_id) {
                Some(turn) => turn,
                None => continue,
            };
            if entry_turn_id != turn_id {
                continue;
            }
            entry.state = ApprovalEntryState::Cancelled {
                reason: reason.to_owned(),
            };
            // Drop any pending runtime waiter; the aborted task will see the
            // closed receiver and treat it as a denial — matching pre-fix
            // behaviour for the runtime side of the channel.
            entry.response_tx = None;
            cancelled.push(CancelledApproval {
                approval_id: approval_id.clone(),
                turn_id: entry_turn_id.clone(),
            });
        }
        cancelled
    }

    pub(crate) fn cancel_pending_approval(
        &self,
        session_id: &SessionKey,
        approval_id: &ApprovalId,
        fallback_turn_id: &TurnId,
        reason: &str,
    ) -> Option<CancelledApproval> {
        let mut entries = self.entries.write().unwrap_or_else(|p| p.into_inner());
        let entry = entries.get_mut(approval_id)?;
        if entry.session_id != *session_id {
            return None;
        }
        if !matches!(&entry.state, ApprovalEntryState::Pending) {
            return None;
        }
        let turn_id = entry
            .request
            .as_ref()
            .map(|request| request.turn_id.clone())
            .unwrap_or_else(|| fallback_turn_id.clone());
        entry.state = ApprovalEntryState::Cancelled {
            reason: reason.to_owned(),
        };
        // Drop any pending runtime waiter, but keep the approval entry so a
        // late client response receives the typed cancellation error.
        entry.response_tx = None;
        Some(CancelledApproval {
            approval_id: approval_id.clone(),
            turn_id,
        })
    }

    #[allow(dead_code)]
    /// Attach the c2 durable backend. Called once at serve startup in cluster
    /// mode; single-node `chat`/`gateway` leave it unset (pure in-process).
    /// Interior-`OnceLock`: set at most once; a second call returns `false`.
    pub(crate) fn attach_durable(
        &self,
        store: std::sync::Arc<dyn ApprovalDurableStore>,
        scope_for: ScopeResolver,
    ) -> bool {
        self.durable
            .set(DurableApprovalSink { store, scope_for })
            .is_ok()
    }

    /// Derive the args-hash the durable record + reply CAS compare on. The
    /// wire approval carries no canonical args, so we hash the decision-
    /// relevant request fields (tool + title + body). A reply that tampered
    /// any of these is rejected (K05 ArgsMismatch).
    fn args_hash_for(event: &ApprovalRequestedEvent) -> String {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(event.tool_name.as_bytes());
        h.update(b"\x00");
        h.update(event.title.as_bytes());
        h.update(b"\x00");
        h.update(event.body.as_bytes());
        format!("{:x}", h.finalize())
    }

    /// Persist a durable Pending record for a request, when a cluster durable
    /// backend is attached. Best-effort-is-NOT-acceptable here: this is the
    /// truth write, so a durable persist failure must surface (the caller
    /// treats it as fail-closed — see `request_runtime`). Returns the
    /// args-hash the reply CAS will compare on, so `request_runtime` can
    /// stash it on the in-process entry for rehydrate paths. `None` when
    /// no durable backend is attached or the wire session has no resolvable
    /// cluster scope.
    fn persist_pending(&self, event: &ApprovalRequestedEvent) -> Option<String> {
        let sink = self.durable.get()?;
        let scope = (sink.scope_for)(&event.session_id)?;
        let args_hash = Self::args_hash_for(event);
        sink.store.persist_pending(
            &scope,
            DurableApprovalRecord {
                approval_id: event.approval_id.0.to_string(),
                originating_run: event.turn_id.0.to_string(),
                args_hash: args_hash.clone(),
                binding_revision: String::new(),
                state: ApprovalState::Pending,
            },
        );
        Some(args_hash)
    }

    /// Rehydrate a pending approval from the durable backend into the
    /// in-process `RwLock`. Used by a fresh `PendingApprovalStore` (Pod B)
    /// that joined the cluster after the originator (Pod A) crashed: the
    /// durable record is the source of truth, and this method installs a
    /// minimal `ApprovalEntry` so `respond_with_context` can pass the CAS
    /// without a full request clone (the durable record carries the
    /// `args_hash` we need).
    ///
    /// Returns `true` when an entry was rehydrated (durable record was
    /// Pending and matched `session_id`); `false` when no durable record
    /// exists, the record is already terminal, or the durable backend is
    /// not attached.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn rehydrate_pending(
        &self,
        session_id: &SessionKey,
        approval_id: &ApprovalId,
    ) -> bool {
        let Some(sink) = self.durable.get() else {
            return false;
        };
        let Some(scope) = (sink.scope_for)(session_id) else {
            return false;
        };
        let Some(rec) = sink.store.get(&scope, &approval_id.0.to_string()) else {
            return false;
        };
        if rec.state != ApprovalState::Pending {
            return false;
        }
        // The durable record is scope-keyed (tenant_id+profile_id+workspace_id
        // +session_id per Scope) and RLS-enforced; cross-scope CAS is
        // rejected by the repository, so rehydrate can trust the lookup.
        let mut entries = self.entries.write().unwrap_or_else(|p| p.into_inner());
        // Idempotent: if the entry already exists, do nothing.
        if entries.contains_key(approval_id) {
            return false;
        }
        entries.insert(
            approval_id.clone(),
            ApprovalEntry {
                session_id: session_id.clone(),
                state: ApprovalEntryState::Pending,
                request: None,
                args_hash: Some(rec.args_hash),
                runtime_resumable: true,
                response_tx: None,
            },
        );
        true
    }

    // Test-only legacy entry point (all callers are `#[cfg(test)]`); the
    // workspace clippy gate runs `--all-targets` (where it is used), but a
    // bare `--lib` clippy sees no non-test caller. Allow the dead-code lint
    // in that configuration rather than gating the method on `#[cfg(test)]`,
    // which would change its visibility.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn insert_pending(&self, session_id: SessionKey, approval_id: ApprovalId) {
        let mut entries = self.entries.write().unwrap_or_else(|p| p.into_inner());
        entries.insert(
            approval_id,
            ApprovalEntry {
                session_id,
                state: ApprovalEntryState::Pending,
                request: None,
                args_hash: None,
                runtime_resumable: false,
                response_tx: None,
            },
        );
    }

    pub(crate) fn request(&self, event: ApprovalRequestedEvent) -> ApprovalRequestedEvent {
        let args_hash = Self::args_hash_for(&event);
        let mut entries = self.entries.write().unwrap_or_else(|p| p.into_inner());
        entries.insert(
            event.approval_id.clone(),
            ApprovalEntry {
                session_id: event.session_id.clone(),
                state: ApprovalEntryState::Pending,
                request: Some(event.clone()),
                args_hash: Some(args_hash),
                runtime_resumable: false,
                response_tx: None,
            },
        );
        event
    }

    pub(crate) fn request_runtime(
        &self,
        event: ApprovalRequestedEvent,
    ) -> tokio::sync::oneshot::Receiver<ApprovalDecision> {
        // c2/K05: persist the durable Pending record BEFORE parking the
        // in-process oneshot, so the durable store is the truth and a restart
        // can recover it. The oneshot is only the wake-up accelerator.
        let args_hash = self.persist_pending(&event);
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut entries = self.entries.write().unwrap_or_else(|p| p.into_inner());
        entries.insert(
            event.approval_id.clone(),
            ApprovalEntry {
                session_id: event.session_id.clone(),
                state: ApprovalEntryState::Pending,
                request: Some(event),
                args_hash,
                runtime_resumable: true,
                response_tx: Some(tx),
            },
        );
        rx
    }

    pub(crate) fn pending_for_session(
        &self,
        session_id: &SessionKey,
    ) -> Vec<ApprovalRequestedEvent> {
        let entries = self.entries.read().unwrap_or_else(|p| p.into_inner());
        entries
            .values()
            .filter(|entry| {
                entry.session_id == *session_id
                    && matches!(&entry.state, ApprovalEntryState::Pending)
            })
            .filter_map(|entry| entry.request.clone())
            .collect()
    }

    #[allow(dead_code)]
    pub(crate) fn remove_pending(&self, session_id: &SessionKey, approval_id: &ApprovalId) -> bool {
        let mut entries = self.entries.write().unwrap_or_else(|p| p.into_inner());
        let should_remove = entries.get(approval_id).is_some_and(|entry| {
            entry.session_id == *session_id && matches!(&entry.state, ApprovalEntryState::Pending)
        });
        if should_remove {
            entries.remove(approval_id);
        }
        should_remove
    }

    #[cfg(test)]
    pub(crate) fn requested_event(
        &self,
        approval_id: &ApprovalId,
    ) -> Option<ApprovalRequestedEvent> {
        let entries = self.entries.read().unwrap_or_else(|p| p.into_inner());
        entries
            .get(approval_id)
            .and_then(|entry| entry.request.clone())
    }
}

/// Build the [`ApprovalDecidedEvent`] that gets durably appended to the
/// ledger and (separately) recorded in the audit log.
///
/// Callers populate `decided_by` from their auth context. For auto-resolved
/// decisions (M9-FIX-06's path), set `auto_resolved = true` and supply a
/// `policy_id` after construction — see the auto-resolved emission site in
/// `UiProtocolApprovalRequester::request_approval`.
///
/// `outcome.context` is `None` for the legacy `insert_pending` test path
/// that never carried a request; in that case we synthesize a fresh
/// `TurnId` so the event still serializes.
pub(crate) fn build_decided_event(
    params: &ApprovalRespondParams,
    outcome: &RespondOutcome,
    decided_by: impl Into<String>,
    decided_at: chrono::DateTime<chrono::Utc>,
) -> ApprovalDecidedEvent {
    let turn_id = outcome
        .context
        .as_ref()
        .map(|ctx| ctx.turn_id.clone())
        .unwrap_or_default();
    ApprovalDecidedEvent {
        session_id: params.session_id.clone(),
        topic: params.session_id.topic().map(ToOwned::to_owned),
        approval_id: params.approval_id.clone(),
        turn_id,
        // FIX-01: `ApprovalDecision` is non-Copy (`Unknown(String)`); clone
        // for the event so the caller can keep using `params`.
        decision: params.decision.clone(),
        scope: params.approval_scope.clone(),
        decided_at,
        decided_by: decided_by.into(),
        auto_resolved: false,
        policy_id: None,
        client_note: params.client_note.clone(),
    }
}

fn approval_not_found_error(params: &ApprovalRespondParams) -> RpcError {
    RpcError::new(
        rpc_error_codes::UNKNOWN_APPROVAL_ID,
        "approval/respond target was not found for this session",
    )
    .with_data(json!({
        "kind": "unknown_approval",
        "method": methods::APPROVAL_RESPOND,
        "session_id": params.session_id,
        "approval_id": params.approval_id,
        "legacy_kind": "approval_not_found",
    }))
}

fn approval_not_pending_error(
    params: &ApprovalRespondParams,
    recorded_decision: ApprovalDecision,
    request_title: Option<&str>,
) -> RpcError {
    RpcError::new(
        rpc_error_codes::APPROVAL_NOT_PENDING,
        "approval/respond target is no longer pending",
    )
    .with_data(json!({
        "kind": "approval_not_pending",
        "method": methods::APPROVAL_RESPOND,
        "session_id": params.session_id,
        "approval_id": params.approval_id,
        "recorded_decision": recorded_decision,
        "request_title": request_title,
    }))
}

fn approval_cancelled_error(
    params: &ApprovalRespondParams,
    reason: &str,
    turn_id: Option<&TurnId>,
) -> RpcError {
    RpcError::new(
        rpc_error_codes::APPROVAL_CANCELLED,
        "approval/respond target was cancelled before a response arrived",
    )
    .with_data(json!({
        "kind": "approval_cancelled",
        "method": methods::APPROVAL_RESPOND,
        "session_id": params.session_id,
        "approval_id": params.approval_id,
        "turn_id": turn_id,
        "reason": reason,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use octos_core::execution_scope::{AuthenticatedIdentity, bind_scope};
    use octos_core::ui_protocol::{ApprovalRespondStatus, TurnId};
    use octos_store::repository::local::{LocalStore, LocalUnitOfWork};
    use octos_store::repository::{
        ApprovalDecision as RepoDecision, RepositoryError, StoreView, UnitOfWork,
    };

    /// A `LocalStore`-backed `ApprovalDurableStore` for tests: persists
    /// pending via a UoW commit and CAS-replies through `StoreView`.
    struct LocalApprovalDurable {
        store: std::sync::Arc<LocalStore>,
    }

    impl ApprovalDurableStore for LocalApprovalDurable {
        fn persist_pending(&self, scope: &Scope, record: DurableApprovalRecord) {
            let mut uow = LocalUnitOfWork::with_store(std::sync::Arc::clone(&self.store));
            uow.create_approval(octos_store::repository::NewApproval {
                scope: scope.clone(),
                approval_id: record.approval_id,
                originating_run: record.originating_run,
                args_hash: record.args_hash,
                binding_revision: record.binding_revision,
            });
            futures::executor::block_on(uow.commit()).expect("persist pending");
        }
        fn reply(
            &self,
            scope: &Scope,
            approval_id: &str,
            args_hash: &str,
            decision: RepoDecision,
        ) -> Result<ApprovalState, RepositoryError> {
            self.store
                .reply_approval(scope, approval_id, args_hash, decision)
        }
        fn get(&self, scope: &Scope, approval_id: &str) -> Option<DurableApprovalRecord> {
            self.store
                .approval(scope, approval_id)
                .map(|r| DurableApprovalRecord {
                    approval_id: r.approval_id,
                    originating_run: r.originating_run,
                    args_hash: r.args_hash,
                    binding_revision: r.binding_revision,
                    state: r.state,
                })
        }
    }

    fn scope_for_session(session: &SessionKey) -> Option<Scope> {
        // Scope binding must be STABLE for a session across request/respond
        // (bind_scope allocates a fresh workspace_id per call — D2 binds once
        // at entry and threads the Scope through). The resolver caches per
        // wire session so the reply CAS compares against the SAME scope the
        // request persisted under. Production resolves the connection's
        // entry-bound scope the same way.
        use std::collections::HashMap as Map;
        use std::sync::{Mutex, OnceLock};
        static CACHE: OnceLock<Mutex<Map<String, Scope>>> = OnceLock::new();
        let cache = CACHE.get_or_init(|| Mutex::new(Map::new()));
        let mut cache = cache.lock().unwrap_or_else(|p| p.into_inner());
        cache
            .entry(session.0.clone())
            .or_insert_with(|| {
                let identity = AuthenticatedIdentity {
                    tenant_id: "t-a".into(),
                    profile_id: session.profile_id().unwrap_or("_main").to_string(),
                };
                bind_scope(&identity, &session.0, None).expect("bind scope")
            })
            .clone()
            .into()
    }

    fn durable_store() -> (PendingApprovalStore, std::sync::Arc<LocalStore>) {
        let local = std::sync::Arc::new(LocalStore::default());
        let store = PendingApprovalStore::default();
        store.attach_durable(
            std::sync::Arc::new(LocalApprovalDurable {
                store: std::sync::Arc::clone(&local),
            }),
            Box::new(scope_for_session),
        );
        (store, local)
    }

    fn request_event(session: &SessionKey, approval_id: &ApprovalId) -> ApprovalRequestedEvent {
        ApprovalRequestedEvent::generic(
            session.clone(),
            approval_id.clone(),
            TurnId::new(),
            "shell",
            "Run command",
            "rm -rf /tmp/x",
        )
    }

    /// K05: with a durable backend attached, a request is persisted Pending
    /// and the first reply decides it via the durable CAS; a replay is
    /// rejected by the CAS even though the in-process entry was consumed.
    #[test]
    fn durable_reply_decides_once_and_replay_is_rejected() {
        let (store, _local) = durable_store();
        let session_id = SessionKey("local:test".into());
        let approval_id = ApprovalId::new();
        let _rx = store.request_runtime(request_event(&session_id, &approval_id));

        let first = store
            .respond_with_context(ApprovalRespondParams::new(
                session_id.clone(),
                approval_id.clone(),
                ApprovalDecision::Approve,
            ))
            .expect("first reply decides");
        assert!(first.result.accepted);

        // The in-process entry is now Responded, so a replay hits the
        // in-process not-pending error — the durable CAS already fired once.
        let replay = store.respond_with_context(ApprovalRespondParams::new(
            session_id,
            approval_id,
            ApprovalDecision::Approve,
        ));
        assert!(replay.is_err());
    }

    /// K05 cross-restart: a pending approval persisted durable survives the
    /// loss of the in-process entry. A FRESH store over the SAME durable
    /// backend (no in-process entry) cannot be replied (fail-closed), proving
    /// the durable record alone is not a bypass — resume requires the
    /// recovery path, not a bare respond.
    #[test]
    fn durable_record_alone_is_not_a_respond_bypass() {
        let (store, local) = durable_store();
        let session_id = SessionKey("local:test".into());
        let approval_id = ApprovalId::new();
        let _rx = store.request_runtime(request_event(&session_id, &approval_id));

        // The durable record is Pending.
        let scope = scope_for_session(&session_id).unwrap();
        let rec = LocalApprovalDurable {
            store: std::sync::Arc::clone(&local),
        }
        .get(&scope, &approval_id.0.to_string())
        .expect("durable pending persisted");
        assert_eq!(rec.state, ApprovalState::Pending);

        // A fresh in-process store (restart analogue) has no entry; a bare
        // respond is not-found (fail closed) — recovery must re-register.
        let fresh = PendingApprovalStore::default();
        let err = fresh.respond_with_context(ApprovalRespondParams::new(
            session_id,
            approval_id,
            ApprovalDecision::Approve,
        ));
        assert!(err.is_err());
    }

    /// K05 cross-pod in-process: two `PendingApprovalStore` instances
    /// share ONE `LocalStore` (the test's `LocalApprovalDurable` adapter)
    /// — simulating two pods sharing one durable backend. Pod A requests
    /// an approval, the durable record is written, and Pod A is dropped
    /// (process exits). A fresh `PendingApprovalStore` (Pod B) is
    /// constructed over the SAME durable backend: the request that Pod A
    /// issued is still visible to Pod B via the durable record, and a
    /// direct reply (recover-and-decide path) is rejected as not-pending
    /// because the in-process entry on Pod B is empty (fail-closed).
    /// This pins the K05 invariant at the in-process layer in parallel
    /// to the storage-level drill (pg_k05_approval_pending_survives_*
    /// in octos-store).
    #[test]
    fn k05_durable_record_visible_across_pod_stores() {
        let (store_a, local) = durable_store();
        let session_id = SessionKey("local:k05pod".into());
        let approval_id = ApprovalId::new();
        let _rx = store_a.request_runtime(request_event(&session_id, &approval_id));
        // Pod A: persisted via UoW. The durable record exists on the
        // shared LocalStore; the in-process entry is on store_a.
        let scope = scope_for_session(&session_id).expect("scope");
        let rec = local
            .approval(&scope, &approval_id.0.to_string())
            .expect("durable pending");
        assert_eq!(rec.state, ApprovalState::Pending);
        drop(store_a);
        // Pod B: a fresh PendingApprovalStore over the SAME LocalStore.
        let store_b = PendingApprovalStore::default();
        store_b.attach_durable(
            std::sync::Arc::new(LocalApprovalDurable {
                store: std::sync::Arc::clone(&local),
            }),
            Box::new(scope_for_session),
        );
        // The durable record is visible to Pod B (storage layer).
        let rec_b = local
            .approval(&scope, &approval_id.0.to_string())
            .expect("pod B sees durable record");
        assert_eq!(rec_b.state, ApprovalState::Pending);
        // But a bare respond on Pod B fails closed (no in-process entry).
        let bare = store_b.respond_with_context(ApprovalRespondParams::new(
            session_id.clone(),
            approval_id.clone(),
            ApprovalDecision::Approve,
        ));
        assert!(
            bare.is_err(),
            "K05 fail-closed: pod B cannot respond without recovery"
        );
    }

    /// K05 positive rehydrate: a fresh `PendingApprovalStore` (Pod B)
    /// can recover a pending approval that Pod A persisted to the
    /// durable backend and the in-process entry then lost. The
    /// `rehydrate_pending` method installs a minimal `ApprovalEntry`
    /// whose `args_hash` matches the durable record's stored hash, so
    /// `respond_with_context` passes the durable CAS and the decision
    /// settles the durable record (Pending → Decided).
    ///
    /// Companion to k05_durable_record_visible_across_pod_stores
    /// (which pins the fail-closed bare-respond path) — together they
    /// prove both halves of the K05 cross-pod invariant.
    #[test]
    fn k05_rehydrate_pending_allows_pod_b_to_decide() {
        let (store_a, local) = durable_store();
        let session_id = SessionKey("local:k05rehydrate".into());
        let approval_id = ApprovalId::new();
        let event = request_event(&session_id, &approval_id);
        let expected_args_hash = PendingApprovalStore::args_hash_for(&event);
        let _rx = store_a.request_runtime(event);
        // Pod A: persisted durable Pending. The in-process entry exists
        // on store_a.
        let scope = scope_for_session(&session_id).expect("scope");
        let rec = local
            .approval(&scope, &approval_id.0.to_string())
            .expect("durable pending");
        assert_eq!(rec.state, ApprovalState::Pending);
        assert_eq!(rec.args_hash, expected_args_hash);
        drop(store_a);

        // Pod B: a fresh PendingApprovalStore over the SAME LocalStore +
        // LocalApprovalDurable adapter.
        let store_b = PendingApprovalStore::default();
        store_b.attach_durable(
            std::sync::Arc::new(LocalApprovalDurable {
                store: std::sync::Arc::clone(&local),
            }),
            Box::new(scope_for_session),
        );
        // Bare respond fails closed before rehydrate.
        let pre = store_b.respond_with_context(ApprovalRespondParams::new(
            session_id.clone(),
            approval_id.clone(),
            ApprovalDecision::Approve,
        ));
        assert!(
            pre.is_err(),
            "K05 fail-closed: bare respond without rehydrate rejected"
        );
        // Rehydrate from the durable backend.
        let ok = store_b.rehydrate_pending(&session_id, &approval_id);
        assert!(
            ok,
            "rehydrate_pending must succeed for an existing durable Pending"
        );
        // The rehydrated entry is now in-process: respond_with_context
        // passes the durable CAS and decides the approval.
        let outcome = store_b
            .respond_with_context(ApprovalRespondParams::new(
                session_id.clone(),
                approval_id.clone(),
                ApprovalDecision::Approve,
            ))
            .expect("post-rehydrate respond decides the approval");
        assert!(outcome.result.accepted);
        // The durable record is now Decided.
        let after = local
            .approval(&scope, &approval_id.0.to_string())
            .expect("durable after");
        assert_eq!(after.state, ApprovalState::Decided);
        // Re-rehydrate is idempotent (returns false because entry exists).
        let again = store_b.rehydrate_pending(&session_id, &approval_id);
        assert!(!again, "rehydrate is idempotent: entry already exists");
        // Replay is rejected by the CAS.
        let replay = store_b.respond_with_context(ApprovalRespondParams::new(
            session_id,
            approval_id,
            ApprovalDecision::Approve,
        ));
        assert!(replay.is_err(), "K05 CAS: replay after decide is rejected");
    }

    #[test]
    fn known_pending_approval_accepts_once() {
        let store = PendingApprovalStore::default();
        let session_id = SessionKey("local:test".into());
        let approval_id = ApprovalId::new();
        store.insert_pending(session_id.clone(), approval_id.clone());

        let outcome = store
            .respond(ApprovalRespondParams::new(
                session_id.clone(),
                approval_id.clone(),
                ApprovalDecision::Approve,
            ))
            .expect("pending approval should accept");

        assert!(outcome.result.accepted);
        assert_eq!(outcome.result.status, ApprovalRespondStatus::Accepted);
        assert!(!outcome.result.runtime_resumed);
        // `insert_pending` doesn't carry a request — context is `None`.
        assert!(outcome.context.is_none());

        let error = store
            .respond(ApprovalRespondParams::new(
                session_id,
                approval_id,
                ApprovalDecision::Deny,
            ))
            .expect_err("responded approval is not pending");

        assert_eq!(error.code, rpc_error_codes::APPROVAL_NOT_PENDING);
        assert_eq!(
            error.data.as_ref().and_then(|data| data.get("kind")),
            Some(&json!("approval_not_pending"))
        );
    }

    #[test]
    fn approval_request_is_stored_and_can_be_responded_to() {
        let store = PendingApprovalStore::default();
        let session_id = SessionKey("local:test".into());
        let approval_id = ApprovalId::new();
        let turn_id = TurnId::new();

        let event = store.request(ApprovalRequestedEvent::generic(
            session_id.clone(),
            approval_id.clone(),
            turn_id.clone(),
            "shell",
            "Run command",
            "cargo test",
        ));

        assert_eq!(event.approval_id, approval_id);
        assert_eq!(
            store
                .requested_event(&approval_id)
                .as_ref()
                .map(|event| event.title.as_str()),
            Some("Run command")
        );

        let outcome = store
            .respond(ApprovalRespondParams::new(
                session_id,
                approval_id,
                ApprovalDecision::Approve,
            ))
            .expect("stored approval request should accept");

        assert!(outcome.result.accepted);
        assert_eq!(outcome.result.status, ApprovalRespondStatus::Accepted);
        assert!(!outcome.result.runtime_resumed);
        // Context recovered from the stored `ApprovalRequestedEvent`.
        let context = outcome.context.expect("context should be present");
        assert_eq!(context.tool_name, "shell");
        assert_eq!(context.turn_id, turn_id);
    }

    #[tokio::test]
    async fn runtime_approval_response_resumes_waiting_tool() {
        let store = PendingApprovalStore::default();
        let session_id = SessionKey("local:test".into());
        let approval_id = ApprovalId::new();
        let response_rx = store.request_runtime(ApprovalRequestedEvent::generic(
            session_id.clone(),
            approval_id.clone(),
            TurnId::new(),
            "shell",
            "Run command",
            "printf approved",
        ));

        let outcome = store
            .respond(ApprovalRespondParams::new(
                session_id,
                approval_id,
                ApprovalDecision::Approve,
            ))
            .expect("runtime approval should accept");

        assert!(outcome.result.runtime_resumed);
        assert_eq!(
            response_rx.await.expect("approval receiver"),
            ApprovalDecision::Approve
        );
    }

    #[test]
    fn missing_approval_is_typed_not_found() {
        let store = PendingApprovalStore::default();
        let error = store
            .respond(ApprovalRespondParams::new(
                SessionKey("local:test".into()),
                ApprovalId::new(),
                ApprovalDecision::Approve,
            ))
            .expect_err("missing approval should fail");

        assert_eq!(error.code, rpc_error_codes::UNKNOWN_APPROVAL_ID);
        assert_eq!(
            error.data.as_ref().and_then(|data| data.get("kind")),
            Some(&json!("unknown_approval"))
        );
    }

    #[test]
    fn pending_approval_survives_cross_session_reconnect_probe() {
        let store = PendingApprovalStore::default();
        let session_id = SessionKey("local:test".into());
        let other_session_id = SessionKey("local:other".into());
        let approval_id = ApprovalId::new();
        store.insert_pending(session_id.clone(), approval_id.clone());

        let wrong_session = store
            .respond(ApprovalRespondParams::new(
                other_session_id,
                approval_id.clone(),
                ApprovalDecision::Approve,
            ))
            .expect_err("approval must be scoped to its owning session");
        assert_eq!(wrong_session.code, rpc_error_codes::UNKNOWN_APPROVAL_ID);

        let outcome = store
            .respond(ApprovalRespondParams::new(
                session_id,
                approval_id,
                ApprovalDecision::Approve,
            ))
            .expect("owning session can still approve after reconnect");
        assert_eq!(outcome.result.status, ApprovalRespondStatus::Accepted);
    }

    #[test]
    fn pending_for_session_returns_only_unanswered_requests() {
        let store = PendingApprovalStore::default();
        let session_id = SessionKey("local:test".into());
        let other_session_id = SessionKey("local:other".into());
        let pending_id = ApprovalId::new();
        let answered_id = ApprovalId::new();

        store.request(ApprovalRequestedEvent::generic(
            session_id.clone(),
            pending_id.clone(),
            TurnId::new(),
            "shell",
            "Pending command",
            "cargo test",
        ));
        store.request(ApprovalRequestedEvent::generic(
            session_id.clone(),
            answered_id.clone(),
            TurnId::new(),
            "shell",
            "Answered command",
            "cargo fmt",
        ));
        store.request(ApprovalRequestedEvent::generic(
            other_session_id,
            ApprovalId::new(),
            TurnId::new(),
            "shell",
            "Other session",
            "cargo check",
        ));
        store
            .respond(ApprovalRespondParams::new(
                session_id.clone(),
                answered_id,
                ApprovalDecision::Deny,
            ))
            .expect("answer one approval");

        let pending = store.pending_for_session(&session_id);

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].approval_id, pending_id);
        assert_eq!(pending[0].title, "Pending command");
    }

    #[test]
    fn removed_pending_approval_is_not_found_for_late_response() {
        let store = PendingApprovalStore::default();
        let session_id = SessionKey("local:test".into());
        let approval_id = ApprovalId::new();
        store.insert_pending(session_id.clone(), approval_id.clone());

        assert!(store.remove_pending(&session_id, &approval_id));
        let error = store
            .respond(ApprovalRespondParams::new(
                session_id,
                approval_id,
                ApprovalDecision::Approve,
            ))
            .expect_err("late response after timeout removal should miss");

        assert_eq!(error.code, rpc_error_codes::UNKNOWN_APPROVAL_ID);
        assert_eq!(
            error.data.as_ref().and_then(|data| data.get("kind")),
            Some(&json!("unknown_approval"))
        );
    }

    #[test]
    fn cancel_pending_for_turn_returns_only_matching_pending_entries() {
        let store = PendingApprovalStore::default();
        let session_id = SessionKey("local:test".into());
        let other_session = SessionKey("local:other".into());
        let interrupted_turn = TurnId::new();
        let surviving_turn = TurnId::new();

        let cancel_target = ApprovalId::new();
        let other_turn = ApprovalId::new();
        let other_session_target = ApprovalId::new();
        let already_responded = ApprovalId::new();

        store.request(ApprovalRequestedEvent::generic(
            session_id.clone(),
            cancel_target.clone(),
            interrupted_turn.clone(),
            "shell",
            "Should cancel",
            "rm -rf /tmp/x",
        ));
        store.request(ApprovalRequestedEvent::generic(
            session_id.clone(),
            other_turn.clone(),
            surviving_turn.clone(),
            "shell",
            "Different turn",
            "ls",
        ));
        store.request(ApprovalRequestedEvent::generic(
            other_session.clone(),
            other_session_target.clone(),
            interrupted_turn.clone(),
            "shell",
            "Different session, same turn id",
            "ls",
        ));
        store.request(ApprovalRequestedEvent::generic(
            session_id.clone(),
            already_responded.clone(),
            interrupted_turn.clone(),
            "shell",
            "Already approved",
            "ls",
        ));
        store
            .respond(ApprovalRespondParams::new(
                session_id.clone(),
                already_responded.clone(),
                ApprovalDecision::Approve,
            ))
            .expect("approve before cancel");

        let cancelled =
            store.cancel_pending_for_turn(&session_id, &interrupted_turn, "turn_interrupted");

        assert_eq!(cancelled.len(), 1);
        assert_eq!(cancelled[0].approval_id, cancel_target);
        assert_eq!(cancelled[0].turn_id, interrupted_turn);

        // The previously-responded entry keeps its recorded decision.
        let recorded = store
            .respond(ApprovalRespondParams::new(
                session_id.clone(),
                already_responded,
                ApprovalDecision::Deny,
            ))
            .expect_err("recorded decision is preserved");
        assert_eq!(recorded.code, rpc_error_codes::APPROVAL_NOT_PENDING);

        // Surviving turn (same session, different turn id) is untouched.
        let survive = store
            .respond(ApprovalRespondParams::new(
                session_id.clone(),
                other_turn,
                ApprovalDecision::Approve,
            ))
            .expect("surviving turn still pending");
        // FIX-06 wrapped the result in `RespondOutcome { result, context }`.
        assert!(survive.result.accepted);

        // Other session with the same turn id is untouched.
        let foreign = store
            .respond(ApprovalRespondParams::new(
                other_session,
                other_session_target,
                ApprovalDecision::Approve,
            ))
            .expect("foreign session still pending");
        assert!(foreign.result.accepted);
    }

    #[test]
    fn cancel_pending_for_turn_is_idempotent() {
        let store = PendingApprovalStore::default();
        let session_id = SessionKey("local:test".into());
        let turn_id = TurnId::new();
        let approval_id = ApprovalId::new();
        store.request(ApprovalRequestedEvent::generic(
            session_id.clone(),
            approval_id.clone(),
            turn_id.clone(),
            "shell",
            "Pending",
            "ls",
        ));

        let first = store.cancel_pending_for_turn(&session_id, &turn_id, "turn_interrupted");
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].approval_id, approval_id);

        let second = store.cancel_pending_for_turn(&session_id, &turn_id, "turn_interrupted");
        assert!(
            second.is_empty(),
            "second cancel must be a no-op for already-cancelled entries",
        );
    }

    #[test]
    fn cancel_with_no_pending_approvals_is_noop() {
        let store = PendingApprovalStore::default();
        let session_id = SessionKey("local:test".into());
        let turn_id = TurnId::new();

        let cancelled = store.cancel_pending_for_turn(&session_id, &turn_id, "turn_interrupted");
        assert!(
            cancelled.is_empty(),
            "interrupt on a session with no pending approvals must be a no-op",
        );
    }

    #[test]
    fn respond_to_cancelled_approval_returns_typed_error() {
        let store = PendingApprovalStore::default();
        let session_id = SessionKey("local:test".into());
        let turn_id = TurnId::new();
        let approval_id = ApprovalId::new();
        store.request(ApprovalRequestedEvent::generic(
            session_id.clone(),
            approval_id.clone(),
            turn_id.clone(),
            "shell",
            "Pending",
            "ls",
        ));

        store.cancel_pending_for_turn(&session_id, &turn_id, "turn_interrupted");

        let err = store
            .respond(ApprovalRespondParams::new(
                session_id.clone(),
                approval_id.clone(),
                ApprovalDecision::Approve,
            ))
            .expect_err("late respond against cancelled approval must fail");
        assert_eq!(err.code, rpc_error_codes::APPROVAL_CANCELLED);
        let data = err.data.expect("typed error data");
        assert_eq!(data["kind"], json!("approval_cancelled"));
        assert_eq!(data["reason"], json!("turn_interrupted"));
        assert_eq!(data["approval_id"], json!(approval_id));
        assert_eq!(data["turn_id"], json!(turn_id));
    }

    #[tokio::test]
    async fn cancel_drops_runtime_waiter_so_it_resolves_to_deny() {
        let store = PendingApprovalStore::default();
        let session_id = SessionKey("local:test".into());
        let turn_id = TurnId::new();
        let approval_id = ApprovalId::new();
        let rx = store.request_runtime(ApprovalRequestedEvent::generic(
            session_id.clone(),
            approval_id.clone(),
            turn_id.clone(),
            "shell",
            "Pending",
            "ls",
        ));

        store.cancel_pending_for_turn(&session_id, &turn_id, "turn_interrupted");

        // Runtime waiter sees the receiver close as Err, which the agent code
        // unwraps to Deny — preserving pre-fix runtime semantics for the
        // already-aborted task.
        assert!(
            rx.await.is_err(),
            "cancel must drop the runtime sender so the receiver errors",
        );
    }

    #[test]
    fn cancelled_approval_is_excluded_from_pending_for_session() {
        let store = PendingApprovalStore::default();
        let session_id = SessionKey("local:test".into());
        let turn_id = TurnId::new();
        let approval_id = ApprovalId::new();
        store.request(ApprovalRequestedEvent::generic(
            session_id.clone(),
            approval_id,
            turn_id.clone(),
            "shell",
            "Pending",
            "ls",
        ));

        store.cancel_pending_for_turn(&session_id, &turn_id, "turn_interrupted");
        assert!(
            store.pending_for_session(&session_id).is_empty(),
            "cancelled approvals must not replay as fresh pending cards"
        );
    }

    #[test]
    fn exact_cancel_preserves_cancelled_error_for_late_respond() {
        let store = PendingApprovalStore::default();
        let session_id = SessionKey("local:test".into());
        let turn_id = TurnId::new();
        let approval_id = ApprovalId::new();
        store.request(ApprovalRequestedEvent::generic(
            session_id.clone(),
            approval_id.clone(),
            turn_id.clone(),
            "shell",
            "Pending",
            "ls",
        ));

        let cancelled = store
            .cancel_pending_approval(&session_id, &approval_id, &turn_id, "request_send_failed")
            .expect("approval cancelled");
        assert_eq!(cancelled.approval_id, approval_id);
        assert_eq!(cancelled.turn_id, turn_id);

        let error = store
            .respond(ApprovalRespondParams::new(
                session_id,
                approval_id,
                ApprovalDecision::Approve,
            ))
            .expect_err("late response should see cancelled state");

        assert_eq!(error.code, rpc_error_codes::APPROVAL_CANCELLED);
        assert_eq!(error.data.as_ref().unwrap()["kind"], "approval_cancelled");
        assert_eq!(
            error.data.as_ref().unwrap()["reason"],
            "request_send_failed"
        );
    }

    #[test]
    fn reconnect_retry_preserves_recorded_approval_decision() {
        let store = PendingApprovalStore::default();
        let session_id = SessionKey("local:test".into());
        let approval_id = ApprovalId::new();
        store.insert_pending(session_id.clone(), approval_id.clone());

        store
            .respond(ApprovalRespondParams::new(
                session_id.clone(),
                approval_id.clone(),
                ApprovalDecision::Deny,
            ))
            .expect("first response records decision");
        let error = store
            .respond(ApprovalRespondParams::new(
                session_id,
                approval_id,
                ApprovalDecision::Approve,
            ))
            .expect_err("reconnect retry should see recorded decision");

        assert_eq!(error.code, rpc_error_codes::APPROVAL_NOT_PENDING);
        assert_eq!(
            error.data.as_ref().unwrap()["recorded_decision"],
            json!(ApprovalDecision::Deny)
        );
    }

    fn pending_request_fixture(store: &PendingApprovalStore) -> (SessionKey, ApprovalId, TurnId) {
        let session_id = SessionKey("local:audit".into());
        let approval_id = ApprovalId::new();
        let turn_id = TurnId::new();
        store.request(ApprovalRequestedEvent::generic(
            session_id.clone(),
            approval_id.clone(),
            turn_id.clone(),
            "shell",
            "Run command",
            "cargo test",
        ));
        (session_id, approval_id, turn_id)
    }

    #[test]
    fn decision_emits_approval_decided_durable_notification() {
        let store = PendingApprovalStore::default();
        let (session_id, approval_id, turn_id) = pending_request_fixture(&store);

        let mut params = ApprovalRespondParams::new(
            session_id.clone(),
            approval_id.clone(),
            ApprovalDecision::Approve,
        );
        params.approval_scope = Some("session".into());
        params.client_note = Some("ok".into());
        let outcome = store
            .respond_with_context(params.clone())
            .expect("decide manually");
        let event = build_decided_event(&params, &outcome, "user:tester", chrono::Utc::now());

        assert_eq!(event.turn_id, turn_id);
        assert_eq!(event.scope.as_deref(), Some("session"));
        assert_eq!(event.client_note.as_deref(), Some("ok"));
        assert_eq!(event.decided_by, "user:tester");
        assert!(!event.auto_resolved);
        assert_eq!(
            outcome.context.as_ref().map(|c| c.tool_name.as_str()),
            Some("shell")
        );
        assert!(store.pending_for_session(&session_id).is_empty());
        // Round-trips through the wire-shaped UiNotification carrier.
        let notification = octos_core::ui_protocol::UiNotification::ApprovalDecided(event.clone());
        let wire = notification
            .clone()
            .into_rpc_notification()
            .expect("serialize");
        assert_eq!(wire.method, methods::APPROVAL_DECIDED);
        assert_eq!(
            octos_core::ui_protocol::UiNotification::from_rpc_notification(wire).expect("decode"),
            notification
        );
    }

    #[test]
    fn auto_resolved_emits_approval_decided_with_auto_resolved_true() {
        // The auto-resolved emission lives at the request site (see
        // `UiProtocolApprovalRequester`); this unit test exercises just the
        // shape-side helper: an auto-resolved decision is built exactly
        // like a manual decision plus `auto_resolved = true` and a
        // `policy_id` set on the constructed event.
        let store = PendingApprovalStore::default();
        let (session_id, approval_id, _) = pending_request_fixture(&store);
        let outcome = store
            .respond_with_context(ApprovalRespondParams::new(
                session_id.clone(),
                approval_id.clone(),
                ApprovalDecision::Approve,
            ))
            .expect("decide auto");
        let mut event = build_decided_event(
            &ApprovalRespondParams::new(session_id, approval_id, ApprovalDecision::Approve),
            &outcome,
            "",
            chrono::Utc::now(),
        );
        event.auto_resolved = true;
        event.policy_id = Some("policy:trusted_shell".into());

        let wire = octos_core::ui_protocol::UiNotification::ApprovalDecided(event.clone())
            .into_rpc_notification()
            .expect("serialize");
        assert_eq!(wire.params["auto_resolved"], json!(true));
        assert_eq!(wire.params["policy_id"], json!("policy:trusted_shell"));
    }
}
