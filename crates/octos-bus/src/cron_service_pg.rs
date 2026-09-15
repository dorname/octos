//! PG-backed cron service (N3): the multi-replica counterpart of
//! `cron_service.rs`. All public methods are async because the durable
//! store is `PgStore` (async). Single-process deployments continue to
//! use `CronService` (sync + LocalCronStore); multi-replica cluster
//! deployments use `CronServicePg` (async + PgStore).
//!
//! The public API mirrors `CronService` method-for-method so a caller
//! migrating from single-process to cluster only has to add `.await`
//! to each call site — the method names, argument shapes, and return
//! types are identical.

use std::sync::Arc;

use chrono::Utc;
use octos_core::InboundMessage;
use octos_core::execution_scope::Scope;
use octos_store::repository::{
    CronScheduleStore, FiringState, MisfirePolicy, Schedule, ScheduleFiring,
};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

/// Object-safe wrapper for `CronScheduleStore` (which is not dyn
/// compatible because its methods return `impl Future`). The
/// `async_trait` attribute boxes the futures internally.
#[async_trait::async_trait]
pub trait CronScheduleStoreObj: Send + Sync {
    async fn create_schedule(&self, scope: &Scope, schedule: Schedule) -> Result<(), String>;
    async fn record_firing(&self, scope: &Scope, firing: ScheduleFiring) -> Result<(), String>;
    async fn claim_firing(
        &self,
        scope: &Scope,
        schedule_id: &str,
        scheduled_at_ms: u64,
        controller_id: &str,
    ) -> Result<ScheduleFiring, String>;
    async fn mark_firing_terminal(
        &self,
        scope: &Scope,
        schedule_id: &str,
        scheduled_at_ms: u64,
        state: FiringState,
        run_id: Option<&str>,
    ) -> Result<(), String>;
    async fn list_due_firings(
        &self,
        scope: &Scope,
        now_ms: u64,
        limit: u32,
    ) -> Result<Vec<ScheduleFiring>, String>;
    async fn delete_schedule(&self, scope: &Scope, schedule_id: &str) -> Result<bool, String>;
    async fn list_schedules(&self, scope: &Scope) -> Result<Vec<Schedule>, String>;
    async fn get_schedule(
        &self,
        scope: &Scope,
        schedule_id: &str,
    ) -> Result<Option<Schedule>, String>;
    async fn update_schedule(&self, scope: &Scope, schedule: Schedule) -> Result<(), String>;
}

/// Blanket impl for any type that implements `CronScheduleStore`.
#[async_trait::async_trait]
impl<T: CronScheduleStore + Send + Sync + 'static> CronScheduleStoreObj for T {
    async fn create_schedule(&self, scope: &Scope, schedule: Schedule) -> Result<(), String> {
        CronScheduleStore::create_schedule(self, scope, schedule)
            .await
            .map_err(|e| e.to_string())
    }
    async fn record_firing(&self, scope: &Scope, firing: ScheduleFiring) -> Result<(), String> {
        CronScheduleStore::record_firing(self, scope, firing)
            .await
            .map_err(|e| e.to_string())
    }
    async fn claim_firing(
        &self,
        scope: &Scope,
        schedule_id: &str,
        scheduled_at_ms: u64,
        controller_id: &str,
    ) -> Result<ScheduleFiring, String> {
        CronScheduleStore::claim_firing(self, scope, schedule_id, scheduled_at_ms, controller_id)
            .await
            .map_err(|e| e.to_string())
    }
    async fn mark_firing_terminal(
        &self,
        scope: &Scope,
        schedule_id: &str,
        scheduled_at_ms: u64,
        state: FiringState,
        run_id: Option<&str>,
    ) -> Result<(), String> {
        CronScheduleStore::mark_firing_terminal(
            self,
            scope,
            schedule_id,
            scheduled_at_ms,
            state,
            run_id,
        )
        .await
        .map_err(|e| e.to_string())
    }
    async fn list_due_firings(
        &self,
        scope: &Scope,
        now_ms: u64,
        limit: u32,
    ) -> Result<Vec<ScheduleFiring>, String> {
        CronScheduleStore::list_due_firings(self, scope, now_ms, limit)
            .await
            .map_err(|e| e.to_string())
    }
    async fn delete_schedule(&self, scope: &Scope, schedule_id: &str) -> Result<bool, String> {
        CronScheduleStore::delete_schedule(self, scope, schedule_id)
            .await
            .map_err(|e| e.to_string())
    }
    async fn list_schedules(&self, scope: &Scope) -> Result<Vec<Schedule>, String> {
        CronScheduleStore::list_schedules(self, scope)
            .await
            .map_err(|e| e.to_string())
    }
    async fn get_schedule(
        &self,
        scope: &Scope,
        schedule_id: &str,
    ) -> Result<Option<Schedule>, String> {
        CronScheduleStore::get_schedule(self, scope, schedule_id)
            .await
            .map_err(|e| e.to_string())
    }
    async fn update_schedule(&self, scope: &Scope, schedule: Schedule) -> Result<(), String> {
        CronScheduleStore::update_schedule(self, scope, schedule)
            .await
            .map_err(|e| e.to_string())
    }
}

use crate::cron_types::{CronJob, CronMode, CronOrigin, CronPayload, CronSchedule};

/// PG-backed cron service. All methods are async.
pub struct CronServicePg {
    store: Arc<dyn CronScheduleStoreObj>,
    scope: Scope,
    controller_id: String,
    inbound_tx: mpsc::Sender<InboundMessage>,
    running: std::sync::atomic::AtomicBool,
    shutdown_notify: tokio::sync::Notify,
}

impl std::fmt::Debug for CronServicePg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CronServicePg")
            .field("controller_id", &self.controller_id)
            .field(
                "running",
                &self.running.load(std::sync::atomic::Ordering::Relaxed),
            )
            .finish_non_exhaustive()
    }
}

impl CronServicePg {
    /// Create a new PG-backed cron service. `controller_id` is the
    /// unique id of this Pod / process — used by `claim_firing` to
    /// enforce K10 single-claim (two Pods racing the same firing
    /// produce one winner and one Conflict).
    pub fn new(
        store: Arc<dyn CronScheduleStoreObj>,
        scope: Scope,
        controller_id: String,
        inbound_tx: mpsc::Sender<InboundMessage>,
    ) -> Self {
        Self {
            store,
            scope,
            controller_id,
            inbound_tx,
            running: std::sync::atomic::AtomicBool::new(false),
            shutdown_notify: tokio::sync::Notify::new(),
        }
    }

    /// Start the cron service: recompute next runs and arm the timer.
    pub async fn start(self: &Arc<Self>) {
        self.running
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let now_ms = Utc::now().timestamp_millis();

        // Recompute next_fire_at for any schedule that's missing it.
        let schedules = self
            .store
            .list_schedules(&self.scope)
            .await
            .unwrap_or_default();
        for mut sched in schedules {
            if sched.enabled && sched.next_fire_at_ms.is_none() {
                sched.next_fire_at_ms = Some(now_ms as u64 + 60_000);
                let _ = self.store.update_schedule(&self.scope, sched).await;
            }
        }

        self.arm_timer();
        info!(controller_id = %self.controller_id, "cron service (PG) started");
    }

    /// Stop the cron service.
    pub async fn stop(&self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.shutdown_notify.notify_waiters();
        info!(controller_id = %self.controller_id, "cron service (PG) stopped");
    }

    /// Synchronous shutdown signal.
    pub fn shutdown_signal(&self) {
        self.running
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.shutdown_notify.notify_waiters();
    }

    /// Add a new cron job.
    pub async fn add_job(
        self: &Arc<Self>,
        name: String,
        schedule: CronSchedule,
        payload: CronPayload,
    ) -> Result<CronJob, String> {
        self.add_job_with_tz(name, schedule, payload, None).await
    }

    /// Add a new cron job with an optional IANA timezone.
    pub async fn add_job_with_tz(
        self: &Arc<Self>,
        name: String,
        schedule: CronSchedule,
        payload: CronPayload,
        timezone: Option<String>,
    ) -> Result<CronJob, String> {
        self.add_job_with_origin(name, schedule, payload, timezone, CronOrigin::default())
            .await
    }

    /// Add a job that records what created it.
    pub async fn add_job_with_origin(
        self: &Arc<Self>,
        name: String,
        schedule: CronSchedule,
        payload: CronPayload,
        timezone: Option<String>,
        origin: CronOrigin,
    ) -> Result<CronJob, String> {
        let now_ms = Utc::now().timestamp_millis();
        let id = short_id();
        let delete_after_run = matches!(schedule, CronSchedule::At { .. });

        let mut job = CronJob {
            id: id.clone(),
            name: name.clone(),
            enabled: true,
            schedule: schedule.clone(),
            payload: payload.clone(),
            state: Default::default(),
            created_at_ms: now_ms,
            delete_after_run,
            timezone: timezone.clone(),
            origin: origin.clone(),
        };
        job.compute_next_run(now_ms);

        let sched = cron_job_to_schedule(&job);
        self.store
            .create_schedule(&self.scope, sched)
            .await
            .map_err(|e| format!("create schedule: {e}"))?;

        self.arm_timer();
        debug!(id = %id, "added cron job (PG)");
        Ok(job)
    }

    /// Remove a cron job by ID. Returns true if found and removed.
    pub async fn remove_job(self: &Arc<Self>, id: &str) -> bool {
        let removed = self
            .store
            .delete_schedule(&self.scope, id)
            .await
            .unwrap_or(false);
        if removed {
            self.arm_timer();
            debug!(id = %id, "removed cron job (PG)");
        }
        removed
    }

    /// Remove every job created by `loop_id`. Returns the ids removed.
    pub async fn remove_jobs_for_loop(self: &Arc<Self>, loop_id: &str) -> Vec<String> {
        let schedules = self
            .store
            .list_schedules(&self.scope)
            .await
            .unwrap_or_default();
        let mut removed = Vec::new();
        for sched in schedules {
            if let Ok(job) = schedule_to_cron_job(&sched)
                && job.belongs_to_loop(loop_id)
            {
                #[allow(clippy::collapsible_if)]
                if self
                    .store
                    .delete_schedule(&self.scope, &sched.schedule_id)
                    .await
                    .unwrap_or(false)
                {
                    removed.push(sched.schedule_id);
                }
            }
        }
        if !removed.is_empty() {
            self.arm_timer();
        }
        removed
    }

    /// List all enabled jobs, sorted by next run time.
    pub async fn list_jobs(&self) -> Vec<CronJob> {
        let schedules = self
            .store
            .list_schedules(&self.scope)
            .await
            .unwrap_or_default();
        let mut jobs: Vec<CronJob> = schedules
            .into_iter()
            .filter(|s| s.enabled)
            .filter_map(|s| schedule_to_cron_job(&s).ok())
            .collect();
        jobs.sort_by_key(|j| j.state.next_run_at_ms.unwrap_or(i64::MAX));
        jobs
    }

    /// List all jobs (including disabled), sorted by next run time.
    pub async fn list_all_jobs(&self) -> Vec<CronJob> {
        let schedules = self
            .store
            .list_schedules(&self.scope)
            .await
            .unwrap_or_default();
        let mut jobs: Vec<CronJob> = schedules
            .into_iter()
            .filter_map(|s| schedule_to_cron_job(&s).ok())
            .collect();
        jobs.sort_by_key(|j| j.state.next_run_at_ms.unwrap_or(i64::MAX));
        jobs
    }

    /// Enable or disable a cron job. Returns true if found.
    pub async fn enable_job(self: &Arc<Self>, id: &str, enabled: bool) -> bool {
        let Some(mut sched) = self
            .store
            .get_schedule(&self.scope, id)
            .await
            .unwrap_or(None)
        else {
            return false;
        };
        sched.enabled = enabled;
        if enabled {
            sched.next_fire_at_ms = Some(Utc::now().timestamp_millis() as u64 + 60_000);
        } else {
            sched.next_fire_at_ms = None;
        }
        let found = self.store.update_schedule(&self.scope, sched).await.is_ok();
        if found {
            self.arm_timer();
            debug!(id = %id, enabled = %enabled, "toggled cron job (PG)");
        }
        found
    }

    /// Enable/disable a job with reconciliation. For PG-backed store
    /// this is identical to `enable_job` — the store is already the
    /// durable truth source, so no reload-from-disk is needed.
    pub async fn toggle_job_reconciling(
        self: &Arc<Self>,
        id: &str,
        enabled: bool,
    ) -> Result<Option<CronJob>, String> {
        let Some(mut sched) = self
            .store
            .get_schedule(&self.scope, id)
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        sched.enabled = enabled;
        if enabled {
            sched.next_fire_at_ms = Some(Utc::now().timestamp_millis() as u64 + 60_000);
        } else {
            sched.next_fire_at_ms = None;
        }
        self.store
            .update_schedule(&self.scope, sched.clone())
            .await
            .map_err(|e| e.to_string())?;
        self.arm_timer();
        Ok(schedule_to_cron_job(&sched).ok())
    }

    /// Arm a timer for the earliest due schedule. The PG path uses a
    /// fixed-interval poll (1s) instead of a computed sleep — the
    /// `list_due_firings` query is cheap (indexed on state + scheduled_at)
    /// and the poll cadence is bounded. Not async: the method spawns
    /// the sleeper task and returns immediately.
    fn arm_timer(self: &Arc<Self>) {
        if !self.running.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let notified = this.shutdown_notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if !this.running.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            tokio::select! {
                biased;
                _ = &mut notified => {}
                _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                    if this.running.load(std::sync::atomic::Ordering::Relaxed) {
                        this.on_timer().await;
                    }
                }
            }
        });
    }

    /// Called when the timer fires: enumerate due firings, claim each
    /// (K10 single-claim), fire the winner, mark terminal, re-arm.
    async fn on_timer(self: &Arc<Self>) {
        if !self.running.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        let now_ms = Utc::now().timestamp_millis() as u64;
        let due = self
            .store
            .list_due_firings(&self.scope, now_ms, 32)
            .await
            .unwrap_or_default();
        for firing in due {
            // K10 single-claim: only one Pod wins the firing.
            let claimed = self
                .store
                .claim_firing(
                    &self.scope,
                    &firing.schedule_id,
                    firing.scheduled_at_ms,
                    &self.controller_id,
                )
                .await;
            let Ok(claimed_firing) = claimed else {
                // Another Pod claimed it first — skip.
                continue;
            };
            // Fire: push the job's message onto the bus.
            if let Some(sched) = self
                .store
                .get_schedule(&self.scope, &claimed_firing.schedule_id)
                .await
                .unwrap_or(None)
            {
                if let Ok(job) = schedule_to_cron_job(&sched) {
                    self.execute_job(&job).await;
                }
            }
            // Mark terminal (K18: durable record of the firing outcome).
            let _ = self
                .store
                .mark_firing_terminal(
                    &self.scope,
                    &claimed_firing.schedule_id,
                    claimed_firing.scheduled_at_ms,
                    FiringState::Succeeded,
                    claimed_firing.run_id.as_deref(),
                )
                .await;
        }
        self.arm_timer();
    }

    /// Fire a single job by sending an InboundMessage into the bus.
    async fn execute_job(&self, job: &CronJob) {
        if job.payload.mode == CronMode::Notify {
            debug!(job_id = %job.id, "cron notify (PG): delivering verbatim");
            return;
        }
        let msg = InboundMessage {
            channel: "system".into(),
            sender_id: "cron".into(),
            chat_id: job.payload.chat_id.clone().unwrap_or_default(),
            content: job.payload.message.clone(),
            timestamp: Utc::now(),
            metadata: serde_json::Value::Null,
            message_id: None,
            origin: octos_core::MessageOrigin::ExternalUser,
            media: Vec::new(),
        };
        let notified = self.shutdown_notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if !self.running.load(std::sync::atomic::Ordering::Relaxed) {
            warn!(job_id = %job.id, "cron service (PG) stopped; dropping delivery");
            return;
        }
        tokio::select! {
            biased;
            _ = &mut notified => {
                warn!(job_id = %job.id, "cron service (PG) shut down during delivery");
            }
            res = self.inbound_tx.send(msg) => {
                if let Err(e) = res {
                    warn!(error = %e, job_id = %job.id, "failed to send cron message (PG)");
                }
            }
        }
    }
}

/// Convert a `CronJob` to a `Schedule` for the store. The payload and
/// origin are serialized to JSON strings; the schedule kind is
/// flattened to a string expression.
fn cron_job_to_schedule(job: &CronJob) -> Schedule {
    let expression = match &job.schedule {
        CronSchedule::At { at_ms } => format!("at:{at_ms}"),
        CronSchedule::Every { every_ms } => format!("every:{every_ms}"),
        CronSchedule::Cron { expr } => format!("cron:{expr}"),
    };
    Schedule {
        schedule_id: job.id.clone(),
        expression,
        timezone: job.timezone.clone(),
        next_fire_at_ms: job.state.next_run_at_ms.map(|t| t as u64),
        misfire_policy: MisfirePolicy::RunOnce,
        enabled: job.enabled,
        last_fired_at_ms: job.state.last_run_at_ms.map(|t| t as u64),
        last_run_id: None,
        name: job.name.clone(),
        payload_json: serde_json::to_string(&job.payload).unwrap_or_default(),
        delete_after_run: job.delete_after_run,
        origin_json: serde_json::to_string(&job.origin).unwrap_or_default(),
        created_at_ms: job.created_at_ms,
    }
}

/// Convert a `Schedule` back to a `CronJob`. Returns `Err` when the
/// expression cannot be parsed back to a `CronSchedule` (e.g. a
/// schedule written by a different binary version).
fn schedule_to_cron_job(sched: &Schedule) -> Result<CronJob, String> {
    let schedule = if let Some(rest) = sched.expression.strip_prefix("at:") {
        let at_ms: i64 = rest.parse().map_err(|e| format!("at: {e}"))?;
        CronSchedule::At { at_ms }
    } else if let Some(rest) = sched.expression.strip_prefix("every:") {
        let every_ms: i64 = rest.parse().map_err(|e| format!("every: {e}"))?;
        CronSchedule::Every { every_ms }
    } else if let Some(rest) = sched.expression.strip_prefix("cron:") {
        CronSchedule::Cron {
            expr: rest.to_string(),
        }
    } else {
        return Err(format!("unknown expression: {}", sched.expression));
    };
    let payload: CronPayload =
        serde_json::from_str(&sched.payload_json).unwrap_or_else(|_| CronPayload {
            message: String::new(),
            deliver: false,
            channel: None,
            chat_id: None,
            mode: CronMode::Agent,
        });
    let origin: CronOrigin = serde_json::from_str(&sched.origin_json).unwrap_or_default();
    Ok(CronJob {
        id: sched.schedule_id.clone(),
        name: sched.name.clone(),
        enabled: sched.enabled,
        schedule,
        payload,
        state: crate::cron_types::CronJobState {
            next_run_at_ms: sched.next_fire_at_ms.map(|t| t as i64),
            last_run_at_ms: sched.last_fired_at_ms.map(|t| t as i64),
            last_status: None,
        },
        created_at_ms: sched.created_at_ms,
        delete_after_run: sched.delete_after_run,
        timezone: sched.timezone.clone(),
        origin,
    })
}

/// Generate a short 8-char hex ID (same algorithm as cron_service).
fn short_id() -> String {
    let id = uuid::Uuid::now_v7();
    let hex = format!("{:032x}", id.as_u128());
    hex[hex.len() - 8..].to_string()
}
