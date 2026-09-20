-- c5 Cron durable firing — schedules + firings with K10 unique constraint.
-- ADR docs/adr/cluster-state-and-execution.md D5 (durable Cron firing);
-- spec specs/task-c5-production-drill-migration.spec.md (cron-dedup).
--
-- K10: two Cron Controllers scanning concurrently must yield exactly one
-- firing and one run; the unique (schedule_id, scheduled_at) makes the
-- loser fail with a unique-violation instead of double-firing. A
-- misfire policy fills the gap after a Controller lapses.

CREATE TABLE IF NOT EXISTS schedules (
    tenant_id     TEXT NOT NULL,
    profile_id    TEXT NOT NULL,
    workspace_id  TEXT NOT NULL,
    session_id    TEXT NOT NULL,
    schedule_id   TEXT NOT NULL,
    expression    TEXT NOT NULL,        -- cron expression / interval spec
    timezone      TEXT,
    next_fire_at  TIMESTAMPTZ,
    misfire_policy TEXT NOT NULL DEFAULT 'skip', -- skip | run_once | catch_up
    enabled       BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, profile_id, workspace_id, session_id, schedule_id)
);

-- K10: a schedule can fire at most once per `scheduled_at` instant across the
-- cluster. The PRIMARY KEY enforces it transactionally: two concurrent
-- inserters race, the loser's INSERT raises unique_violation and is mapped
-- to Conflict.
CREATE TABLE IF NOT EXISTS schedule_firings (
    tenant_id     TEXT NOT NULL,
    profile_id    TEXT NOT NULL,
    workspace_id  TEXT NOT NULL,
    session_id    TEXT NOT NULL,
    schedule_id   TEXT NOT NULL,
    scheduled_at  TIMESTAMPTZ NOT NULL,
    firing_id     TEXT NOT NULL,
    claimed_by    TEXT,                 -- controller id; NULL until claimed
    state         TEXT NOT NULL,        -- intent | running | succeeded | failed
    run_id        TEXT,                 -- back-ref to the run this firing spawned
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, profile_id, workspace_id, session_id, schedule_id, scheduled_at)
);

ALTER TABLE schedules         ENABLE ROW LEVEL SECURITY;
ALTER TABLE schedule_firings  ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_isolation ON schedules;
CREATE POLICY tenant_isolation ON schedules
    USING (tenant_id = current_setting('app.tenant_id', true));
DROP POLICY IF EXISTS tenant_isolation ON schedule_firings;
CREATE POLICY tenant_isolation ON schedule_firings
    USING (tenant_id = current_setting('app.tenant_id', true));
ALTER TABLE schedules         FORCE ROW LEVEL SECURITY;
ALTER TABLE schedule_firings  FORCE ROW LEVEL SECURITY;

CREATE INDEX IF NOT EXISTS schedule_firings_claim ON schedule_firings (state, scheduled_at);
