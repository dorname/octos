-- c2 persistence boundary — initial PostgreSQL schema (expand step of
-- expand/contract). ADR docs/adr/cluster-state-and-execution.md D1/D6.
-- Spec specs/task-c2-persistence-boundary-postgres.spec.md Decisions 3, 7.
--
-- Every tenant business table carries tenant_id as the leading key
-- component; uniqueness is enforced per-scope (tenant+profile+workspace+
-- session), never on the wire session id alone (K07). The application role
-- is not a superuser and does not BYPASSRLS; RLS is enabled as defense in
-- depth beneath the application's structural scope binding.

-- Sessions: the scope anchor + run/version bookkeeping.
CREATE TABLE IF NOT EXISTS sessions (
    tenant_id     TEXT NOT NULL,
    profile_id    TEXT NOT NULL,
    workspace_id  TEXT NOT NULL,
    session_id    TEXT NOT NULL,
    version       BIGINT NOT NULL DEFAULT 1,
    next_event_seq BIGINT NOT NULL DEFAULT 1,
    PRIMARY KEY (tenant_id, profile_id, workspace_id, session_id)
);

-- Canonical messages (thread/turn/fork/rollback preserved by id ordering).
CREATE TABLE IF NOT EXISTS messages (
    tenant_id     TEXT NOT NULL,
    profile_id    TEXT NOT NULL,
    workspace_id  TEXT NOT NULL,
    session_id    TEXT NOT NULL,
    message_id    TEXT NOT NULL,
    thread_id     TEXT NOT NULL,
    turn_id       TEXT NOT NULL,
    role          TEXT NOT NULL,
    content       TEXT NOT NULL,
    ordinal       BIGINT NOT NULL,
    PRIMARY KEY (tenant_id, profile_id, workspace_id, session_id, ordinal),
    UNIQUE (tenant_id, profile_id, workspace_id, session_id, message_id)
);

-- Run / agent execution state (business state machine; handle stays
-- in-process). c3 adds leases/checkpoints on top.
CREATE TABLE IF NOT EXISTS agent_runs (
    run_id            TEXT PRIMARY KEY,
    tenant_id         TEXT NOT NULL,
    profile_id        TEXT NOT NULL,
    workspace_id      TEXT NOT NULL,
    session_id        TEXT NOT NULL,
    parent_run_id     TEXT,
    definition_rev    TEXT,
    binding_rev       TEXT,
    runtime_version   TEXT,
    state             TEXT NOT NULL,
    attempt           BIGINT NOT NULL DEFAULT 1,
    cancel_requested  BOOLEAN NOT NULL DEFAULT FALSE
);

-- Session events: per-scope monotonic gap-free seq + event_id dedup (D6).
CREATE TABLE IF NOT EXISTS session_events (
    tenant_id     TEXT NOT NULL,
    profile_id    TEXT NOT NULL,
    workspace_id  TEXT NOT NULL,
    session_id    TEXT NOT NULL,
    seq           BIGINT NOT NULL,
    event_id      TEXT NOT NULL,
    causation_id  TEXT,
    payload       JSONB NOT NULL,
    PRIMARY KEY (tenant_id, profile_id, workspace_id, session_id, seq),
    UNIQUE (tenant_id, profile_id, workspace_id, session_id, event_id)
);

-- Outbox rows committed in the same transaction as their event (D6).
CREATE TABLE IF NOT EXISTS outbox (
    id            BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    tenant_id     TEXT NOT NULL,
    aggregate_key TEXT NOT NULL,
    topic         TEXT NOT NULL,
    payload       JSONB NOT NULL,
    delivered     BOOLEAN NOT NULL DEFAULT FALSE
);

-- Durable approvals/questions (K05): pending/decision/timeout; the oneshot
-- channel is only an in-process accelerator, never the source of truth.
CREATE TABLE IF NOT EXISTS approvals (
    tenant_id        TEXT NOT NULL,
    profile_id       TEXT NOT NULL,
    workspace_id     TEXT NOT NULL,
    session_id       TEXT NOT NULL,
    approval_id      TEXT NOT NULL,
    originating_run  TEXT NOT NULL,
    args_hash        TEXT NOT NULL,
    binding_revision TEXT NOT NULL,
    state            TEXT NOT NULL DEFAULT 'pending',
    decision         TEXT,
    expires_at       TIMESTAMPTZ,
    PRIMARY KEY (tenant_id, profile_id, workspace_id, session_id, approval_id)
);

-- Defense-in-depth row isolation beneath the app's structural scope binding.
ALTER TABLE sessions       ENABLE ROW LEVEL SECURITY;
ALTER TABLE messages       ENABLE ROW LEVEL SECURITY;
ALTER TABLE agent_runs     ENABLE ROW LEVEL SECURITY;
ALTER TABLE session_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE outbox         ENABLE ROW LEVEL SECURITY;
ALTER TABLE approvals      ENABLE ROW LEVEL SECURITY;

-- The connection sets `app.tenant_id` per transaction; the policy enforces
-- it. The migrations run as the table owner (which bypasses RLS), so the
-- policy is FORCEd to also constrain the owner. CREATE POLICY has no
-- IF NOT EXISTS in PG16, so each policy is dropped-then-created to keep the
-- migration idempotent (spec rule migration-safety: re-run is a no-op).
DROP POLICY IF EXISTS tenant_isolation ON sessions;
CREATE POLICY tenant_isolation ON sessions
    USING (tenant_id = current_setting('app.tenant_id', true));
DROP POLICY IF EXISTS tenant_isolation ON messages;
CREATE POLICY tenant_isolation ON messages
    USING (tenant_id = current_setting('app.tenant_id', true));
DROP POLICY IF EXISTS tenant_isolation ON agent_runs;
CREATE POLICY tenant_isolation ON agent_runs
    USING (tenant_id = current_setting('app.tenant_id', true));
DROP POLICY IF EXISTS tenant_isolation ON session_events;
CREATE POLICY tenant_isolation ON session_events
    USING (tenant_id = current_setting('app.tenant_id', true));
DROP POLICY IF EXISTS tenant_isolation ON outbox;
CREATE POLICY tenant_isolation ON outbox
    USING (tenant_id = current_setting('app.tenant_id', true));
DROP POLICY IF EXISTS tenant_isolation ON approvals;
CREATE POLICY tenant_isolation ON approvals
    USING (tenant_id = current_setting('app.tenant_id', true));

ALTER TABLE sessions       FORCE ROW LEVEL SECURITY;
ALTER TABLE messages       FORCE ROW LEVEL SECURITY;
ALTER TABLE agent_runs     FORCE ROW LEVEL SECURITY;
ALTER TABLE session_events FORCE ROW LEVEL SECURITY;
ALTER TABLE outbox         FORCE ROW LEVEL SECURITY;
ALTER TABLE approvals      FORCE ROW LEVEL SECURITY;
