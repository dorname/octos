//! Idempotent tool side-effect execution (c3 / K04).
//!
//! ADR `docs/adr/cluster-state-and-execution.md` D4; spec
//! `specs/task-c3-recoverable-execution-leases.spec.md` (side-effect-idempotency).
//!
//! A tool with an external side effect must not be blindly re-fired after a
//! worker crash. This module wraps a [`Tool`] so the agent records an intent
//! BEFORE execution, then marks the outcome. On a takeover, the ledger tells
//! the new worker whether the effect is already confirmed (reuse the result),
//! unknown (route to reconciliation — never auto-retry), or fresh (execute).
//!
//! The durable store lives behind [`SideEffectLedger`] so `octos-agent` keeps
//! no database dependency; `octos-cli` implements it over
//! `octos-store`'s `RecoveryStore`.

use std::sync::Arc;

use eyre::Result;

use super::{Tool, ToolContext, ToolResult};

/// The durable verdict for a logical tool invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideEffectVerdict {
    /// No record — safe to execute.
    Fresh,
    /// A confirmed result exists — reuse it, do NOT re-fire (K04).
    ReuseConfirmed,
    /// The effect may have happened but was never confirmed — route to
    /// reconciliation, do NOT auto-retry (K04).
    Reconcile,
    /// A prior attempt failed cleanly with no side effect — safe to retry.
    Retryable,
}

/// The durable record the agent consults before/after a side effect. The
/// agent layer is deliberately free of any store/DB type; the CLI maps this
/// onto `octos_store::repository::RecoveryStore`.
#[async_trait::async_trait]
pub trait SideEffectLedger: Send + Sync {
    /// Consult + record the intent for a logical invocation. Returns the
    /// verdict to act on. `idempotency_key` is `Some` only when the external
    /// system supports idempotent replay.
    async fn consult(
        &self,
        invocation_id: &str,
        tool_revision: &str,
        args_hash: &str,
        idempotency_key: Option<&str>,
    ) -> SideEffectVerdict;

    /// The confirmed result to reuse for a `ReuseConfirmed` verdict, if any.
    async fn confirmed_result(&self, invocation_id: &str) -> Option<String>;

    /// Mark the invocation succeeded with a durable result reference.
    async fn mark_succeeded(&self, invocation_id: &str, result_ref: &str);

    /// Mark the invocation Unknown (effect uncertain) → reconciliation (K04).
    async fn mark_unknown(&self, invocation_id: &str);
}

/// Supplies the external idempotency key for a given args payload, when the
/// external system supports idempotent replay; `None` means the tool has no
/// idempotency capability and an unconfirmed effect goes to reconciliation.
pub type IdempotencyKeyFn = Arc<dyn Fn(&serde_json::Value) -> Option<String> + Send + Sync>;

/// A [`Tool`] decorator that routes external side effects through a
/// [`SideEffectLedger`] (c3/K04). Wraps any tool whose execution has an
/// external side effect; read-only tools need no wrapping.
pub struct IdempotentToolExecutor {
    inner: Arc<dyn Tool>,
    ledger: Arc<dyn SideEffectLedger>,
    /// Stable tool revision (name@version) for the ledger.
    tool_revision: String,
    idempotency_key_for: Option<IdempotencyKeyFn>,
}

impl IdempotentToolExecutor {
    pub fn new(
        inner: Arc<dyn Tool>,
        ledger: Arc<dyn SideEffectLedger>,
        tool_revision: impl Into<String>,
    ) -> Self {
        Self {
            inner,
            ledger,
            tool_revision: tool_revision.into(),
            idempotency_key_for: None,
        }
    }

    pub fn with_idempotency_key(mut self, f: IdempotencyKeyFn) -> Self {
        self.idempotency_key_for = Some(f);
        self
    }

    fn args_hash(args: &serde_json::Value) -> String {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(args.to_string().as_bytes());
        format!("{:x}", h.finalize())
    }

    /// A stable logical invocation id for a tool call within a run. Derived
    /// from the tool revision + args hash so a retry of the SAME logical call
    /// maps to the same ledger entry (K04).
    fn invocation_id(&self, args: &serde_json::Value) -> String {
        format!("{}:{}", self.tool_revision, Self::args_hash(args))
    }
}

#[async_trait::async_trait]
impl Tool for IdempotentToolExecutor {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn input_schema(&self) -> serde_json::Value {
        self.inner.input_schema()
    }

    async fn execute(&self, args: &serde_json::Value) -> Result<ToolResult> {
        self.execute_with_context(&ToolContext::zero(), args).await
    }

    async fn execute_with_context(
        &self,
        ctx: &ToolContext,
        args: &serde_json::Value,
    ) -> Result<ToolResult> {
        let invocation_id = self.invocation_id(args);
        let args_hash = Self::args_hash(args);
        let idem_key = self.idempotency_key_for.as_ref().and_then(|f| f(args));

        match self
            .ledger
            .consult(
                &invocation_id,
                &self.tool_revision,
                &args_hash,
                idem_key.as_deref(),
            )
            .await
        {
            SideEffectVerdict::Fresh | SideEffectVerdict::Retryable => {
                // Execute, then mark the outcome. A crash between external
                // success and mark leaves the entry non-Succeeded → a takeover
                // consults and reconciles rather than re-firing.
                match self.inner.execute_with_context(ctx, args).await {
                    Ok(result) if result.success => {
                        let result_ref = format!("inline:{}", invocation_id);
                        self.ledger
                            .mark_succeeded(&invocation_id, &result_ref)
                            .await;
                        Ok(result)
                    }
                    Ok(result) => {
                        // Clean failure, no side effect — retryable.
                        Ok(result)
                    }
                    Err(e) => {
                        // Execution errored; if the tool has an external side
                        // effect the outcome is uncertain → Unknown (K04).
                        if idem_key.is_some() {
                            self.ledger.mark_unknown(&invocation_id).await;
                        }
                        Err(e)
                    }
                }
            }
            SideEffectVerdict::ReuseConfirmed => {
                // Reuse the confirmed result; do NOT re-fire the side effect.
                let output = self
                    .ledger
                    .confirmed_result(&invocation_id)
                    .await
                    .unwrap_or_else(|| format!("reused confirmed result for {invocation_id}"));
                Ok(ToolResult {
                    output,
                    success: true,
                    ..Default::default()
                })
            }
            SideEffectVerdict::Reconcile => {
                // Never auto-retry an unknown side effect (K04).
                Ok(ToolResult {
                    output: format!(
                        "side effect of {invocation_id} is unknown; routed to reconciliation, not retried"
                    ),
                    success: false,
                    ..Default::default()
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// A counting tool: records how many times its side effect fired.
    struct CountingTool {
        calls: Arc<Mutex<u32>>,
        succeed: bool,
    }

    #[async_trait::async_trait]
    impl Tool for CountingTool {
        fn name(&self) -> &str {
            "counting"
        }
        fn description(&self) -> &str {
            "counts calls"
        }
        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        async fn execute(&self, _args: &serde_json::Value) -> Result<ToolResult> {
            *self.calls.lock().unwrap() += 1;
            Ok(ToolResult {
                output: "fired".into(),
                success: self.succeed,
                ..Default::default()
            })
        }
    }

    /// An in-memory ledger capturing the K04 state machine.
    struct MemLedger {
        states: Mutex<HashMap<String, (SideEffectVerdict, Option<String>)>>,
    }
    impl MemLedger {
        fn new() -> Self {
            Self {
                states: Mutex::new(HashMap::new()),
            }
        }
        fn seed(&self, id: &str, v: SideEffectVerdict, result: Option<String>) {
            self.states.lock().unwrap().insert(id.into(), (v, result));
        }
    }
    #[async_trait::async_trait]
    impl SideEffectLedger for MemLedger {
        async fn consult(
            &self,
            id: &str,
            _rev: &str,
            _hash: &str,
            _k: Option<&str>,
        ) -> SideEffectVerdict {
            self.states
                .lock()
                .unwrap()
                .get(id)
                .map(|(v, _)| *v)
                .unwrap_or(SideEffectVerdict::Fresh)
        }
        async fn confirmed_result(&self, id: &str) -> Option<String> {
            self.states
                .lock()
                .unwrap()
                .get(id)
                .and_then(|(_, r)| r.clone())
        }
        async fn mark_succeeded(&self, id: &str, result_ref: &str) {
            self.states.lock().unwrap().insert(
                id.into(),
                (SideEffectVerdict::ReuseConfirmed, Some(result_ref.into())),
            );
        }
        async fn mark_unknown(&self, id: &str) {
            self.states
                .lock()
                .unwrap()
                .insert(id.into(), (SideEffectVerdict::Reconcile, None));
        }
    }

    fn args() -> serde_json::Value {
        serde_json::json!({"command":"rm -rf /tmp/x"})
    }

    /// K04: a fresh invocation executes once and is marked succeeded.
    #[tokio::test]
    async fn fresh_invocation_executes_and_marks_succeeded() {
        let calls = Arc::new(Mutex::new(0));
        let tool = Arc::new(CountingTool {
            calls: Arc::clone(&calls),
            succeed: true,
        });
        let ledger = Arc::new(MemLedger::new());
        let exec = IdempotentToolExecutor::new(
            tool,
            Arc::clone(&ledger) as Arc<dyn SideEffectLedger>,
            "shell@1",
        );

        let out = exec.execute(&args()).await.unwrap();
        assert!(out.success);
        assert_eq!(*calls.lock().unwrap(), 1, "side effect fired once");

        // A second call on the same logical invocation reuses, not re-fires.
        let out2 = exec.execute(&args()).await.unwrap();
        assert_eq!(
            *calls.lock().unwrap(),
            1,
            "confirmed result reused, not re-fired"
        );
        assert!(out2.success);
    }

    /// K04: a ReuseConfirmed verdict never executes the tool.
    #[tokio::test]
    async fn confirmed_verdict_reuses_without_executing() {
        let calls = Arc::new(Mutex::new(0));
        let tool = Arc::new(CountingTool {
            calls: Arc::clone(&calls),
            succeed: true,
        });
        let ledger = Arc::new(MemLedger::new());
        let exec = IdempotentToolExecutor::new(
            tool,
            Arc::clone(&ledger) as Arc<dyn SideEffectLedger>,
            "shell@1",
        );
        let id = exec.invocation_id(&args());
        ledger.seed(
            &id,
            SideEffectVerdict::ReuseConfirmed,
            Some("obj://result/1".into()),
        );

        let out = exec.execute(&args()).await.unwrap();
        assert_eq!(*calls.lock().unwrap(), 0, "no re-fire on confirmed verdict");
        assert_eq!(out.output, "obj://result/1");
        assert!(out.success);
    }

    /// K04: a Reconcile (unknown) verdict is NOT retried — returns failure
    /// routed to reconciliation, and the tool never executes.
    #[tokio::test]
    async fn unknown_verdict_goes_to_reconciliation_not_retry() {
        let calls = Arc::new(Mutex::new(0));
        let tool = Arc::new(CountingTool {
            calls: Arc::clone(&calls),
            succeed: true,
        });
        let ledger = Arc::new(MemLedger::new());
        let exec = IdempotentToolExecutor::new(
            tool,
            Arc::clone(&ledger) as Arc<dyn SideEffectLedger>,
            "shell@1",
        );
        let id = exec.invocation_id(&args());
        ledger.seed(&id, SideEffectVerdict::Reconcile, None);

        let out = exec.execute(&args()).await.unwrap();
        assert_eq!(
            *calls.lock().unwrap(),
            0,
            "unknown side effect not auto-retried"
        );
        assert!(!out.success);
        assert!(out.output.contains("reconciliation"));
    }
}
