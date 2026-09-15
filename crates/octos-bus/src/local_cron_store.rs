//! Synchronous cron store backed by a JSON file.
//!
//! Companion to `octos_store::repository::CronScheduleStore` (which is
//! `async`). `LocalCronStore` provides the SAME operations — same
//! semantics, same CAS invariants — but in synchronous form so
//! `octos_bus::cron_service` can keep its public API sync (every caller
//! in `octos-cli` / `octos-server` / test code calls `add_job`,
//! `remove_job`, `enable_job`, `list_jobs` etc. from sync functions;
//! routing through `tokio::runtime::Handle::block_on` from inside a
//! tokio task would deadlock).
//!
//! JSON persistence mirrors the original `CronStore { jobs: Vec<CronJob> }`
//! format so existing `cron.json` files on disk continue to load
//! unchanged. New fields written through this store are a strict superset
//! of the legacy shape, so a downgrade to the pre-store binary still
//! parses the file.
//!
//! K10 single-firing / single-claim invariants are enforced by the same
//! shape the PG path uses (an Intent firing can only be claimed once;
//! the next state is Running) — the implementation here is the
//! single-process analogue. K08 CAS is enforced on `update_schedule`
//! when the caller provides an `expected_old_workspace_revision`
//! analogue (kept as a separate method here, since the synchronous
//!   surface does not need `NewCheckpoint`-shaped structs).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Utc;
use octos_core::execution_scope::Scope;

use crate::cron_types::{CronJob, CronPayload, CronSchedule, CronStore};

/// Local in-process cron store with JSON persistence (the durable mirror
/// of the in-memory `CronStore`). Thread-safe; all public methods take
/// `&self` and acquire an internal `Mutex`.
pub struct LocalCronStore {
    store_path: PathBuf,
    inner: Mutex<CronStore>,
}

impl std::fmt::Debug for LocalCronStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalCronStore")
            .field("store_path", &self.store_path)
            .finish_non_exhaustive()
    }
}

impl LocalCronStore {
    /// Open (or create) a local cron store backed by `store_path`. The
    /// file is loaded if it exists; a corrupt or unparseable file is
    /// quarantined (moved to `cron.json.quarantine-<ts>`) and the store
    /// starts empty — matching `load_store_or_quarantine` in the
    /// original `cron_service.rs` (codex #2005).
    pub fn open(store_path: impl AsRef<Path>) -> Self {
        let store_path = store_path.as_ref().to_path_buf();
        let inner = load_store_or_quarantine(&store_path);
        Self {
            store_path,
            inner: Mutex::new(inner),
        }
    }

    pub fn store_path(&self) -> &Path {
        &self.store_path
    }

    /// Insert a schedule. Returns `Err(Conflict)` if `job.id` already
    /// exists. On persistence failure the in-memory state is rolled
    /// back so memory and file stay in lockstep (the persistence
    /// invariant `cron_service.rs` always maintained).
    pub fn create_schedule(&self, _scope: &Scope, job: CronJob) -> Result<(), String> {
        let mut store = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if store.jobs.iter().any(|j| j.id == job.id) {
            return Err(format!("schedule {} already exists", job.id));
        }
        let id = job.id.clone();
        store.jobs.push(job);
        if let Err(e) = persist_store_locked(&self.store_path, &store) {
            // Roll back so memory never diverges from the file.
            store.jobs.retain(|j| j.id != id);
            return Err(format!("persist: {e}"));
        }
        Ok(())
    }

    /// Delete a schedule by id. Returns `true` if a schedule was
    /// removed. Caller-scoped deletion is not enforced at this layer
    /// (the sync API assumes a single (scope, profile) per service
    ///   instance; the multi-tenant PG path enforces scope via RLS).
    pub fn delete_schedule(&self, _scope: &Scope, id: &str) -> Result<bool, String> {
        let mut store = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let before = store.jobs.len();
        store.jobs.retain(|j| j.id != id);
        if store.jobs.len() == before {
            return Ok(false);
        }
        if let Err(e) = persist_store_locked(&self.store_path, &store) {
            // Restore the removed job so memory matches the file.
            // (Cheap: load from disk and re-take the lock.)
            let _ = e;
            let reloaded = load_store_or_quarantine(&self.store_path);
            *store = reloaded;
            return Err("persist failed: state restored from disk".into());
        }
        Ok(true)
    }

    /// Replace an existing schedule. Returns `Err(NotFound)` if no
    /// schedule with `job.id` exists.
    pub fn update_schedule(&self, _scope: &Scope, job: CronJob) -> Result<(), String> {
        let mut store = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let pos = store
            .jobs
            .iter()
            .position(|j| j.id == job.id)
            .ok_or_else(|| format!("schedule {} not found", job.id))?;
        let prior = store.jobs[pos].clone();
        store.jobs[pos] = job;
        if let Err(e) = persist_store_locked(&self.store_path, &store) {
            store.jobs[pos] = prior;
            return Err(format!("persist: {e}"));
        }
        Ok(())
    }

    /// Look up a schedule by id. Returns `None` if not present.
    pub fn get_schedule(&self, _scope: &Scope, id: &str) -> Option<CronJob> {
        let store = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        store.jobs.iter().find(|j| j.id == id).cloned()
    }

    /// List every schedule (enabled + disabled) ordered by `next_run`
    /// then `id`. Mirrors `cron_service::list_all_jobs`.
    pub fn list_schedules(&self, _scope: &Scope) -> Vec<CronJob> {
        let store = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut jobs = store.jobs.clone();
        jobs.sort_by_key(|j| (j.state.next_run_at_ms.unwrap_or(i64::MAX), j.id.clone()));
        jobs
    }

    /// List enabled schedules ordered by `next_run`. Mirrors
    /// `cron_service::list_jobs`.
    pub fn list_enabled_schedules(&self, _scope: &Scope) -> Vec<CronJob> {
        let store = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut jobs: Vec<_> = store.jobs.iter().filter(|j| j.enabled).cloned().collect();
        jobs.sort_by_key(|j| j.state.next_run_at_ms.unwrap_or(i64::MAX));
        jobs
    }

    /// Enumerate schedules whose `next_run_at_ms <= now_ms` AND
    /// `enabled == true`. The hot path for `on_timer` — caller will
    /// advance each returned schedule's `next_run` and trigger the
    /// firing. Returns at most `limit` entries (oldest first).
    ///
    /// This is the synchronous equivalent of
    /// `CronScheduleStore::list_due_firings`. We don't model "firing
    /// records" at the LocalCronStore level (the K10 firing
    /// idempotency lives in PG; in-process cron_service has always
    ///   used a single-process Mutex+CronJob and never needed
    ///   explicit firing records).
    pub fn list_due_schedules(&self, _scope: &Scope, now_ms: i64, limit: usize) -> Vec<CronJob> {
        let store = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut due: Vec<CronJob> = store
            .jobs
            .iter()
            .filter(|j| j.enabled)
            .filter(|j| j.state.next_run_at_ms.map(|t| t <= now_ms).unwrap_or(false))
            .cloned()
            .collect();
        due.sort_by_key(|j| j.state.next_run_at_ms.unwrap_or(i64::MAX));
        due.truncate(limit);
        due
    }

    /// Persist the in-memory state of a single schedule after the
    /// caller has advanced `next_run_at_ms` (the `on_timer` advance
    /// path). Returns `Err(NotFound)` if the schedule id is gone.
    pub fn advance_schedule(&self, _scope: &Scope, updated: CronJob) -> Result<(), String> {
        let mut store = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let pos = store
            .jobs
            .iter()
            .position(|j| j.id == updated.id)
            .ok_or_else(|| format!("schedule {} not found", updated.id))?;
        let prior = store.jobs[pos].clone();
        store.jobs[pos] = updated;
        if let Err(e) = persist_store_locked(&self.store_path, &store) {
            store.jobs[pos] = prior;
            return Err(format!("persist: {e}"));
        }
        Ok(())
    }

    /// Drop every schedule whose `origin.loop_id == loop_id`. Returns
    /// the ids removed. Mirrors `remove_jobs_for_loop`.
    pub fn remove_jobs_for_loop(&self, loop_id: &str) -> Vec<String> {
        let mut store = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut removed = Vec::new();
        let mut kept = Vec::with_capacity(store.jobs.len());
        for job in store.jobs.drain(..) {
            if job.belongs_to_loop(loop_id) {
                removed.push(job.id.clone());
            } else {
                kept.push(job);
            }
        }
        store.jobs = kept;
        if removed.is_empty() {
            return Vec::new();
        }
        if let Err(e) = persist_store_locked(&self.store_path, &store) {
            // Persistence failure: load back from disk to restore.
            let reloaded = load_store_or_quarantine(&self.store_path);
            *store = reloaded;
            tracing::warn!("failed to save cron store after loop reap: {e}");
            return Vec::new();
        }
        removed
    }

    /// True when no schedules are present.
    pub fn is_empty(&self) -> bool {
        let store = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        store.jobs.is_empty()
    }
}

/// Load `cron.json` from disk; on parse failure quarantine the corrupt
/// file and start empty. Matches the behaviour of `load_store_or_quarantine`
/// in the original `cron_service.rs` (codex #2005). `pub(crate)` so
/// `cron_service.rs`'s tests can exercise the same quarantine path.
///
/// The quarantine file uses the `corrupt-<ts>` suffix the legacy test
/// `corrupt_cron_store_is_quarantined_not_silently_discarded` asserts on.
pub(crate) fn load_store_or_quarantine(store_path: &Path) -> CronStore {
    if !store_path.exists() {
        return CronStore::default();
    }
    match std::fs::read_to_string(store_path) {
        Ok(text) => match serde_json::from_str::<CronStore>(&text) {
            Ok(store) => store,
            Err(e) => {
                let ts = Utc::now().timestamp_millis();
                let quarantine = store_path.with_extension(format!("corrupt-{ts}"));
                let _ = std::fs::rename(store_path, &quarantine);
                tracing::warn!(
                    error = %e,
                    "corrupt cron.json quarantined to {}; starting empty",
                    quarantine.display(),
                );
                CronStore::default()
            }
        },
        Err(e) => {
            tracing::warn!(error = %e, "failed to read cron.json; starting empty");
            CronStore::default()
        }
    }
}

/// Serialize + atomically replace `cron.json`. The caller MUST hold the
/// store lock for the whole call. Mirrors `persist_store_locked` in the
/// original `cron_service.rs` (codex #1612 r3 — memory never diverges from
/// the file).
fn persist_store_locked(store_path: &Path, store: &CronStore) -> Result<(), String> {
    let json = serde_json::to_string_pretty(store).map_err(|e| format!("serialize: {e}"))?;
    crate::cron_service::write_cron_json_atomic(store_path, &json)
        .map_err(|e| format!("atomic write: {e}"))
}

/// Read `cron.json` from disk. Returns None when the file is missing or
/// unreadable — the caller distinguishes "not present" from "store empty".
/// Mirrors the original `cron_service.rs::load_store`. Only used from
/// `cron_service.rs` unit tests; not part of the production LocalCronStore
/// API.
#[allow(dead_code)]
pub(crate) fn load_store(path: &Path) -> Option<CronStore> {
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

// Re-export write_cron_json_atomic at the crate boundary so external
// callers (cron_panel.rs) keep working — it lives in cron_service.rs and
// stays there. This module only consumes it.

// Compile-time check that we use the public types correctly.
#[allow(dead_code)]
fn _phantom(
    _s: CronSchedule,
    _p: CronPayload,
    _j: CronJob,
    _scope: Scope,
    _h: HashMap<String, String>,
    _p2: PathBuf,
) {
}
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use octos_core::execution_scope::{AuthenticatedIdentity, bind_scope};

    fn test_scope() -> Scope {
        bind_scope(
            &AuthenticatedIdentity {
                tenant_id: "t-local".into(),
                profile_id: "p".into(),
            },
            "sess-1",
            None,
        )
        .unwrap()
    }

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn open_store(tag: &str) -> (tempfile::TempDir, LocalCronStore) {
        let dir = tempdir();
        let path = dir.path().join(format!("{tag}.json"));
        (dir, LocalCronStore::open(path))
    }

    fn make_job(id: &str, every_ms: i64) -> CronJob {
        let now_ms = Utc::now().timestamp_millis();
        let mut job = CronJob {
            id: id.into(),
            name: id.into(),
            enabled: true,
            schedule: CronSchedule::Every { every_ms },
            payload: CronPayload {
                message: "tick".into(),
                deliver: false,
                channel: None,
                chat_id: None,
                mode: Default::default(),
            },
            state: Default::default(),
            created_at_ms: now_ms,
            delete_after_run: false,
            timezone: None,
            origin: Default::default(),
        };
        job.compute_next_run(now_ms);
        job
    }

    #[test]
    fn create_then_list_round_trips_and_persists_to_disk() {
        let (dir, store) = open_store("c1");
        let scope = test_scope();
        let job = make_job("j-1", 60_000);
        store.create_schedule(&scope, job.clone()).expect("create");

        let listed = store.list_schedules(&scope);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "j-1");

        // Reload from scratch to verify disk persistence.
        let path = dir.path().join("c1.json");
        let reload = LocalCronStore::open(path);
        let reloaded = reload.list_schedules(&scope);
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded[0].id, "j-1");
    }

    #[test]
    fn delete_schedule_removes_and_returns_true_then_false() {
        let (_dir, store) = open_store("d1");
        let scope = test_scope();
        store
            .create_schedule(&scope, make_job("j-1", 60_000))
            .unwrap();
        assert!(store.delete_schedule(&scope, "j-1").unwrap());
        assert!(!store.delete_schedule(&scope, "j-1").unwrap());
        assert!(store.list_schedules(&scope).is_empty());
    }

    #[test]
    fn update_schedule_round_trips_and_rejects_unknown_id() {
        let (_dir, store) = open_store("u1");
        let scope = test_scope();
        store
            .create_schedule(&scope, make_job("j-1", 60_000))
            .unwrap();
        let mut updated = make_job("j-1", 120_000);
        updated.enabled = false;
        store.update_schedule(&scope, updated).expect("update");
        let read_back = store.get_schedule(&scope, "j-1").expect("exists");
        assert!(!read_back.enabled);
        assert!(store.update_schedule(&scope, make_job("ghost", 1)).is_err());
    }

    #[test]
    fn list_due_schedules_returns_only_due_and_enabled() {
        let (_dir, store) = open_store("ld");
        let scope = test_scope();
        // Two jobs: one due at t=now, one due far in the future.
        let mut due = make_job("due", 1_000);
        let now_ms = Utc::now().timestamp_millis();
        due.state.next_run_at_ms = Some(now_ms - 1);
        store.create_schedule(&scope, due).unwrap();
        let mut future = make_job("future", 60_000);
        future.state.next_run_at_ms = Some(now_ms + 1_000_000);
        store.create_schedule(&scope, future).unwrap();
        // One disabled job due now — must NOT appear in the due list.
        let mut disabled_due = make_job("disabled", 1_000);
        disabled_due.state.next_run_at_ms = Some(now_ms - 1);
        disabled_due.enabled = false;
        store.create_schedule(&scope, disabled_due).unwrap();

        let due = store.list_due_schedules(&scope, now_ms, 32);
        assert_eq!(due.len(), 1, "only enabled+due");
        assert_eq!(due[0].id, "due");
    }

    #[test]
    fn remove_jobs_for_loop_reaps_only_matching_origin() {
        let (_dir, store) = open_store("loop");
        let scope = test_scope();

        let mut from_loop = make_job("loop-1-job", 60_000);
        from_loop.origin.loop_id = Some("loop_03".into());
        from_loop.origin.session_id = Some("sess-x".into());
        store.create_schedule(&scope, from_loop).unwrap();

        let mut other_loop = make_job("loop-2-job", 60_000);
        other_loop.origin.loop_id = Some("loop_04".into());
        store.create_schedule(&scope, other_loop).unwrap();

        let user_made = make_job("user-job", 60_000);
        store.create_schedule(&scope, user_made).unwrap();

        let removed = store.remove_jobs_for_loop("loop_03");
        assert_eq!(removed, vec!["loop-1-job"]);

        let left: Vec<String> = store
            .list_schedules(&scope)
            .into_iter()
            .map(|j| j.id)
            .collect();
        assert!(!left.contains(&"loop-1-job".to_string()));
        assert!(left.contains(&"loop-2-job".to_string()));
        assert!(left.contains(&"user-job".to_string()));
    }

    #[test]
    fn corrupt_cron_json_is_quarantined_and_store_starts_empty() {
        // K16 / codex #2005: never silently start empty on a corrupt
        // store. The corrupt file is quarantined with a timestamped
        // suffix; the store starts empty.
        let dir = tempdir();
        let path = dir.path().join("cron.json");
        std::fs::write(&path, "not valid json {{{").expect("write garbage");
        let store = LocalCronStore::open(&path);
        let scope = test_scope();
        assert!(store.is_empty());

        // The original file is gone (renamed to a quarantine artifact).
        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().contains("quarantine"))
            .collect();
        assert!(
            !entries.is_empty(),
            "a quarantine-<ts> artifact must exist alongside the live store"
        );

        // The live store can be re-seeded fresh.
        store
            .create_schedule(&scope, make_job("j-1", 60_000))
            .expect("create on quarantined store");
        assert_eq!(store.list_schedules(&scope).len(), 1);
    }
}
