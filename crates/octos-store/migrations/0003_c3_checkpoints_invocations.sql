-- c3 recoverable execution — checkpoints + tool-invocation idempotency ledger
-- (expand step). ADR docs/adr/cluster-state-and-execution.md D3/D4; spec
-- specs/task-c3-recoverable-execution-leases.spec.md (side-effect-idempotency,
-- checkpoint-resume, binding-pin).
--
-- Recovery rebuilds a run from its latest committed checkpoint — never from
-- in-process memory (D3). Tool side effects are recorded as intents BEFORE
-- execution with an external idempotency key, so a post-crash takeover can
-- reuse a confirmed result instead of re-firing (K04), and an Unknown (no
-- idempotency, result not persisted) goes to reconciliation, not auto-retry.

CREATE TABLE IF NOT EXISTS run_checkpoints (
    tenant_id        TEXT NOT NULL,
    profile_id       TEXT NOT NULL,
    workspace_id     TEXT NOT NULL,
    session_id       TEXT NOT NULL,
    run_id           TEXT NOT NULL,
    step             BIGINT NOT NULL,
    transcript_highwater BIGINT NOT NULL,
    context          JSONB,
    workspace_revision TEXT,
    pending_invocation TEXT,
    artifact_refs    JSONB,
    binding_digest   TEXT,           -- pinned definition/binding revision (K11)
    permission_snapshot JSONB,
    digest           TEXT NOT NULL,  -- integrity; corruption fails closed
    schema_version   TEXT NOT NULL,
    runtime_version  TEXT NOT NULL,
    created_epoch    BIGINT NOT NULL, -- fencing: written under a lease epoch
    PRIMARY KEY (tenant_id, profile_id, workspace_id, session_id, run_id, step)
);

CREATE TABLE IF NOT EXISTS tool_invocations (
    tenant_id     TEXT NOT NULL,
    profile_id    TEXT NOT NULL,
    workspace_id  TEXT NOT NULL,
    session_id    TEXT NOT NULL,
    run_id        TEXT NOT NULL,
    invocation_id TEXT NOT NULL,     -- logical invocation id (stable across retry)
    tool_revision TEXT NOT NULL,
    args_hash     TEXT NOT NULL,
    state         TEXT NOT NULL,     -- intent | succeeded | failed | unknown
    external_idempotency_key TEXT,   -- NULL = no idempotency capability
    result_ref    TEXT,
    cost_micros   BIGINT,            -- for join-once accounting (K09)
    PRIMARY KEY (tenant_id, profile_id, workspace_id, session_id, run_id, invocation_id),
    -- A logical invocation is unique per run; a retry reuses invocation_id and
    -- must match the recorded args_hash (tamper ⇒ mismatch).
    UNIQUE (tenant_id, profile_id, workspace_id, session_id, external_idempotency_key)
);

ALTER TABLE run_checkpoints  ENABLE ROW LEVEL SECURITY;
ALTER TABLE tool_invocations ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_isolation ON run_checkpoints;
CREATE POLICY tenant_isolation ON run_checkpoints
    USING (tenant_id = current_setting('app.tenant_id', true));
DROP POLICY IF EXISTS tenant_isolation ON tool_invocations;
CREATE POLICY tenant_isolation ON tool_invocations
    USING (tenant_id = current_setting('app.tenant_id', true));
ALTER TABLE run_checkpoints  FORCE ROW LEVEL SECURITY;
ALTER TABLE tool_invocations FORCE ROW LEVEL SECURITY;

CREATE INDEX IF NOT EXISTS tool_invocations_state ON tool_invocations (state);
