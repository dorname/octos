-- c5 N2-full: extend schedules with the c2/c3 fields cron_service needs to
-- round-trip a full CronJob through the durable store. New columns are
-- NULLABLE so a row written by an older binary loads cleanly (the
-- octos-store `create_schedule` path always populates them, but `cron.json`
-- legacy migration imports may leave them NULL — handled in code by
-- defaulting to "" / None / false / 0).
--
-- The shape is intentionally flat rather than nested JSONB: this table is
-- queried on (scope, schedule_id) and (state, scheduled_at) only, and
-- keeping the columns first-class makes the panel's "list all jobs" view
-- cheap (no JSON extraction on every render).

ALTER TABLE schedules
    ADD COLUMN IF NOT EXISTS last_fired_at     TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_run_id       TEXT,
    ADD COLUMN IF NOT EXISTS name              TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS payload_json      TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS delete_after_run  BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS origin_json       TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS created_at_ms     BIGINT NOT NULL DEFAULT 0;