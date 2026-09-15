-- c3 recoverable execution — run leases with epoch fencing (expand step).
-- ADR docs/adr/cluster-state-and-execution.md D5; spec
-- specs/task-c3-recoverable-execution-leases.spec.md (lease-claim, fencing).
--
-- A run's execution scope is owned by at most one worker at a time. Claiming
-- and epoch increment happen in ONE transaction (K02: concurrent claims yield
-- a single owner). Any business-state write carries the current epoch; a
-- stale-epoch write is rejected (K03: a partitioned old owner cannot write).

CREATE TABLE IF NOT EXISTS run_leases (
    tenant_id     TEXT NOT NULL,
    profile_id    TEXT NOT NULL,
    workspace_id  TEXT NOT NULL,
    session_id    TEXT NOT NULL,
    run_id        TEXT NOT NULL,
    owner_id      TEXT NOT NULL,
    epoch         BIGINT NOT NULL,
    expires_at    TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (tenant_id, profile_id, workspace_id, session_id, run_id)
);

-- Epoch must be positive and monotonically increasing per run (enforced by
-- the claim transaction; the CHECK guards against direct misuse). PG has no
-- `ADD CONSTRAINT IF NOT EXISTS`, so guard with a DO block for idempotency.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'run_leases_epoch_positive'
    ) THEN
        ALTER TABLE run_leases ADD CONSTRAINT run_leases_epoch_positive CHECK (epoch > 0);
    END IF;
END $$;

ALTER TABLE run_leases ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_isolation ON run_leases;
CREATE POLICY tenant_isolation ON run_leases
    USING (tenant_id = current_setting('app.tenant_id', true));
ALTER TABLE run_leases FORCE ROW LEVEL SECURITY;

-- Recovery scan index: find expired leases to take over.
CREATE INDEX IF NOT EXISTS run_leases_expires ON run_leases (expires_at);
