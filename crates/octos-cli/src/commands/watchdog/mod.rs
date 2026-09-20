//! `octos watchdog`：OctoLoop 夜间监督与有界续推。
//!
//! 本模块只负责观察、分类、去重、门铃与告警。它不会获取 outer-duty，
//! 也不会执行 OpenLogos 确认点或复制黑板中的业务指令。

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::Utc;
use clap::{Args, Subcommand};
use eyre::{Context, Result, bail, eyre};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::Executable;

const STATE_VERSION: u32 = 1;
const CONFIG_VERSION: u32 = 1;

/// Watchdog 命令入口。
#[derive(Debug, Args)]
pub struct WatchdogCommand {
    #[command(subcommand)]
    action: WatchdogAction,
}

#[derive(Debug, Subcommand)]
enum WatchdogAction {
    /// 前台持续巡检；常驻生命周期应由 systemd user service 管理。
    Run(CommonArgs),
    /// 执行一个确定性巡检周期。
    RunOnce {
        #[command(flatten)]
        common: CommonArgs,
        /// 输出机器可读 JSON。
        #[arg(long)]
        json: bool,
    },
    /// 只读显示持久状态，不发现或唤醒 agent。
    Status {
        #[command(flatten)]
        common: CommonArgs,
        /// 输出机器可读 JSON。
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Args)]
struct CommonArgs {
    /// 被监督项目；使用 --config 时可由配置中的 project 提供。
    #[arg(long, value_name = "PATH")]
    project: Option<PathBuf>,
    /// 覆盖按 project-id 发现的默认配置。
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,
}

impl WatchdogCommand {
    pub(crate) fn emits_json(&self) -> bool {
        matches!(
            self.action,
            WatchdogAction::RunOnce { json: true, .. } | WatchdogAction::Status { json: true, .. }
        )
    }
}

impl Executable for WatchdogCommand {
    fn execute(self) -> Result<()> {
        match self.action {
            WatchdogAction::Run(common) => run_forever(resolve_config(&common)?),
            WatchdogAction::RunOnce { common, json } => {
                let config = resolve_config(&common)?;
                let result = run_cycle(&config, &SystemAdapters, now_ms())?;
                print_cycle(&result, json)
            }
            WatchdogAction::Status { common, json } => {
                let config = resolve_config(&common)?;
                print_status(&read_status(&config)?, json)
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct WatchdogConfigFile {
    version: u32,
    project: PathBuf,
    #[serde(default = "default_poll_interval")]
    poll_interval_secs: u64,
    #[serde(default = "default_idle_after")]
    idle_after_secs: u64,
    #[serde(default = "default_observation_window")]
    observation_window_secs: u64,
    #[serde(default = "default_max_retries")]
    max_no_progress_retries: u8,
    #[serde(default)]
    sources: SourcesConfig,
    #[serde(default)]
    agents: AgentsConfig,
    #[serde(default)]
    alerts: AlertsConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcesConfig {
    #[serde(default = "default_board")]
    board: PathBuf,
    events: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentsConfig {
    #[serde(default = "default_inner_kind")]
    inner_kind: String,
    #[serde(default = "default_outer_kind")]
    outer_kind: String,
}

impl Default for AgentsConfig {
    fn default() -> Self {
        Self {
            inner_kind: default_inner_kind(),
            outer_kind: default_outer_kind(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct AlertsConfig {
    #[serde(default = "default_true")]
    stderr: bool,
    file: Option<String>,
}

impl Default for AlertsConfig {
    fn default() -> Self {
        Self {
            stderr: true,
            file: None,
        }
    }
}

fn default_poll_interval() -> u64 {
    5
}
fn default_idle_after() -> u64 {
    120
}
fn default_observation_window() -> u64 {
    120
}
fn default_max_retries() -> u8 {
    3
}
fn default_board() -> PathBuf {
    PathBuf::from(".octos/OUTER_LOOP_REVIEW.md")
}
fn default_inner_kind() -> String {
    "octoscode".to_owned()
}
fn default_outer_kind() -> String {
    "claude".to_owned()
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Clone)]
struct ResolvedConfig {
    project: PathBuf,
    project_id: String,
    board: PathBuf,
    events: PathBuf,
    poll_interval_secs: u64,
    idle_after_secs: u64,
    observation_window_secs: u64,
    max_no_progress_retries: u8,
    inner_kind: String,
    outer_kind: String,
    state_dir: PathBuf,
    alerts_stderr: bool,
    alerts_file: PathBuf,
}

fn resolve_config(args: &CommonArgs) -> Result<ResolvedConfig> {
    let project_hint = match &args.project {
        Some(path) => Some(canonical_project(path)?),
        None => None,
    };
    let discovered_config = match (&args.config, &project_hint) {
        (Some(path), _) => Some(path.clone()),
        (None, Some(project)) => {
            let id = project_id(project);
            let path = xdg_config_home()
                .join("octos/watchdog")
                .join(format!("{id}.toml"));
            path.is_file().then_some(path)
        }
        (None, None) => None,
    };

    let raw = if let Some(path) = discovered_config {
        let text = fs::read_to_string(&path)
            .wrap_err_with(|| format!("读取 Watchdog 配置失败：{}", path.display()))?;
        toml::from_str::<WatchdogConfigFile>(&text)
            .wrap_err_with(|| format!("解析 Watchdog 配置失败：{}", path.display()))?
    } else {
        let project = project_hint
            .clone()
            .ok_or_else(|| eyre!("必须提供 --project 或 --config"))?;
        WatchdogConfigFile {
            version: CONFIG_VERSION,
            project,
            poll_interval_secs: default_poll_interval(),
            idle_after_secs: default_idle_after(),
            observation_window_secs: default_observation_window(),
            max_no_progress_retries: default_max_retries(),
            sources: SourcesConfig {
                board: default_board(),
                events: None,
            },
            agents: AgentsConfig::default(),
            alerts: AlertsConfig::default(),
        }
    };
    validate_raw_config(&raw)?;
    let project = canonical_project(&raw.project)?;
    if let Some(hint) = project_hint
        && hint != project
    {
        bail!("--project 与配置中的 project 不一致");
    }
    let id = project_id(&project);
    let board = resolve_source_path(&project, &raw.sources.board);
    let events = match raw.sources.events {
        Some(path) => resolve_source_path(&project, &path),
        None => discover_events(&project)?,
    };
    let state_dir = xdg_state_home().join("octos/watchdog").join(&id);
    let alerts_file = raw
        .alerts
        .file
        .as_deref()
        .map(expand_tilde)
        .unwrap_or_else(|| state_dir.join("alerts.jsonl"));
    Ok(ResolvedConfig {
        project,
        project_id: id,
        board,
        events,
        poll_interval_secs: raw.poll_interval_secs,
        idle_after_secs: raw.idle_after_secs,
        observation_window_secs: raw.observation_window_secs,
        max_no_progress_retries: raw.max_no_progress_retries,
        inner_kind: raw.agents.inner_kind,
        outer_kind: raw.agents.outer_kind,
        state_dir,
        alerts_stderr: raw.alerts.stderr,
        alerts_file,
    })
}

fn validate_raw_config(raw: &WatchdogConfigFile) -> Result<()> {
    if raw.version != CONFIG_VERSION {
        bail!("不支持的配置 version={}，仅支持 1", raw.version);
    }
    if !raw.project.is_absolute() {
        bail!("配置字段 project 必须是绝对路径");
    }
    if raw.poll_interval_secs == 0 {
        bail!("配置字段 poll_interval_secs 必须大于 0");
    }
    if raw.observation_window_secs == 0 {
        bail!("配置字段 observation_window_secs 必须大于 0");
    }
    if !(1..=3).contains(&raw.max_no_progress_retries) {
        bail!("配置字段 max_no_progress_retries 只允许 1..=3");
    }
    if raw.agents.inner_kind.trim().is_empty() || raw.agents.outer_kind.trim().is_empty() {
        bail!("配置字段 agents.inner_kind/outer_kind 不得为空");
    }
    Ok(())
}

fn canonical_project(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("project 必须是绝对路径：{}", path.display());
    }
    let canonical = dunce::canonicalize(path)
        .wrap_err_with(|| format!("project 无法 canonicalize：{}", path.display()))?;
    if !canonical.is_dir() {
        bail!("project 不是目录：{}", canonical.display());
    }
    Ok(canonical)
}

fn resolve_source_path(project: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project.join(path)
    }
}

fn discover_events(project: &Path) -> Result<PathBuf> {
    let state_home = std::env::var_os("OCTOS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".octos"));
    let runtime = super::obs::resolve_instance_runtime_root(&state_home, project);
    let profile =
        super::obs::resolve_profile_data_root(&state_home, project, super::obs::DEFAULT_PROFILE_ID);
    let candidates = [
        runtime.join("events.jsonl"),
        profile.join("events.jsonl"),
        profile.join("runtime/events.jsonl"),
    ];
    let found: Vec<_> = candidates
        .into_iter()
        .filter(|path| path.is_file())
        .collect();
    match found.as_slice() {
        [only] => Ok(only.clone()),
        [] => bail!("无法按项目唯一发现 events.jsonl；请在 [sources].events 显式配置绝对路径"),
        _ => bail!("发现多个 events.jsonl 候选；请显式配置以避免跨实例误投递"),
    }
}

fn project_id(project: &Path) -> String {
    sha256_hex(project.to_string_lossy().as_bytes())[..24].to_owned()
}

fn xdg_config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".config"))
}

fn xdg_state_home() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/state"))
}

fn expand_tilde(value: &str) -> PathBuf {
    if value == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(value));
    }
    if let Some(rest) = value.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(value)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
struct SourceCursor {
    file_id: String,
    offset: u64,
    generation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum SignalKind {
    BoardAck,
    GoalBlocked,
    Escalation,
    BudgetLimited,
    InnerIdlePending,
    WatchdogFault,
}

impl SignalKind {
    fn targets_outer(&self) -> bool {
        matches!(
            self,
            Self::BoardAck | Self::GoalBlocked | Self::Escalation | Self::BudgetLimited
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Signal {
    id: String,
    kind: SignalKind,
    object_id: String,
    source: String,
    position: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PendingSignal {
    signal: Signal,
    attempts: u32,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
struct TaskState {
    retry_count: u8,
    prompts_sent: u8,
    fused: bool,
    observation_deadline_ms: Option<i64>,
    last_prompt_signal_id: Option<String>,
    last_progress_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct WatchdogState {
    version: u32,
    project: String,
    board_cursor: SourceCursor,
    event_cursor: SourceCursor,
    pending: Vec<PendingSignal>,
    delivered_signal_ids: BTreeSet<String>,
    tasks: BTreeMap<String, TaskState>,
    last_progress_fingerprint: String,
    last_progress_at: Option<String>,
    last_signal_at: Option<String>,
    last_outer_duty: String,
    idle_since_ms: Option<i64>,
    updated_at: String,
}

impl WatchdogState {
    fn baseline<A: WatchdogAdapters>(
        config: &ResolvedConfig,
        adapters: &A,
        now: i64,
    ) -> Result<Self> {
        Ok(Self {
            version: STATE_VERSION,
            project: config.project.to_string_lossy().into_owned(),
            board_cursor: cursor_at_eof(&config.board)?,
            event_cursor: cursor_at_eof(&config.events)?,
            pending: Vec::new(),
            delivered_signal_ids: BTreeSet::new(),
            tasks: BTreeMap::new(),
            last_progress_fingerprint: progress_fingerprint(config, adapters)?,
            last_progress_at: Some(timestamp(now)),
            last_signal_at: None,
            last_outer_duty: "unknown".to_owned(),
            idle_since_ms: Some(now),
            updated_at: timestamp(now),
        })
    }
}

fn cursor_at_eof(path: &Path) -> Result<SourceCursor> {
    let metadata =
        fs::metadata(path).wrap_err_with(|| format!("监督源不存在或不可读：{}", path.display()))?;
    Ok(SourceCursor {
        file_id: file_identity(path, &metadata)?,
        offset: metadata.len(),
        generation: 0,
    })
}

#[cfg(unix)]
fn file_identity(_path: &Path, metadata: &fs::Metadata) -> Result<String> {
    use std::os::unix::fs::MetadataExt;
    Ok(format!("{}:{}", metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn file_identity(path: &Path, metadata: &fs::Metadata) -> Result<String> {
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |value| value.as_nanos());
    Ok(sha256_hex(
        format!("{}:{modified}", path.display()).as_bytes(),
    ))
}

fn state_path(config: &ResolvedConfig) -> PathBuf {
    config.state_dir.join("state.json")
}

fn load_state(config: &ResolvedConfig) -> Result<Option<WatchdogState>> {
    let path = state_path(config);
    if !path.exists() {
        return Ok(None);
    }
    let text =
        fs::read_to_string(&path).wrap_err_with(|| format!("读取状态失败：{}", path.display()))?;
    let state: WatchdogState = serde_json::from_str(&text)
        .wrap_err_with(|| format!("状态损坏，已 fail closed：{}", path.display()))?;
    if state.version != STATE_VERSION {
        bail!("状态版本不兼容：{}", state.version);
    }
    if state.project != config.project.to_string_lossy() {
        bail!("状态 project 与当前项目不一致，拒绝归零游标");
    }
    Ok(Some(state))
}

fn ensure_private_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).wrap_err_with(|| format!("创建目录失败：{}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn private_open(path: &Path, append: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).write(true).read(true).append(append);
    if !append {
        options.truncate(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

fn save_state(config: &ResolvedConfig, state: &WatchdogState) -> Result<()> {
    ensure_private_dir(&config.state_dir)?;
    let path = state_path(config);
    let temp = config
        .state_dir
        .join(format!(".state.{}.tmp", std::process::id()));
    let mut file = private_open(&temp, false)?;
    serde_json::to_writer_pretty(&mut file, state)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::rename(&temp, &path).wrap_err_with(|| format!("原子替换状态失败：{}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

struct CycleLock(File);

impl CycleLock {
    fn acquire(config: &ResolvedConfig) -> Result<Self> {
        ensure_private_dir(&config.state_dir)?;
        let path = config.state_dir.join("watchdog.lock");
        let file = private_open(&path, false)?;
        file.try_lock_exclusive()
            .map_err(|_| eyre!("已有 Watchdog 周期持有项目单写锁"))?;
        Ok(Self(file))
    }
}

impl Drop for CycleLock {
    fn drop(&mut self) {
        // 显式走 fs2 trait：std 的同名 `File::unlock` 1.89 才稳定，超出 workspace MSRV 1.85。
        let _ = fs2::FileExt::unlock(&self.0);
    }
}

#[derive(Debug)]
struct NewLine {
    position: u64,
    text: String,
}

fn read_incremental(path: &Path, cursor: &SourceCursor) -> Result<(SourceCursor, Vec<NewLine>)> {
    let metadata = fs::metadata(path)
        .wrap_err_with(|| format!("读取监督源 metadata 失败：{}", path.display()))?;
    let current_id = file_identity(path, &metadata)?;
    let mut next = cursor.clone();
    if current_id != cursor.file_id || metadata.len() < cursor.offset {
        next.file_id = current_id;
        next.offset = 0;
        next.generation = cursor.generation.saturating_add(1);
    }
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(next.offset))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let content = String::from_utf8(bytes).map_err(|_| eyre!("监督源不是有效 UTF-8"))?;
    let base = next.offset;
    let mut consumed = 0_u64;
    let mut lines = Vec::new();
    for piece in content.split_inclusive('\n') {
        if !piece.ends_with('\n') {
            break;
        }
        let position = base + consumed;
        consumed += piece.len() as u64;
        lines.push(NewLine {
            position,
            text: piece.trim_end_matches(['\r', '\n']).to_owned(),
        });
    }
    next.offset = base + consumed;
    Ok((next, lines))
}

fn classify_board(
    config: &ResolvedConfig,
    cursor: &SourceCursor,
    lines: &[NewLine],
) -> Vec<Signal> {
    lines
        .iter()
        .filter_map(|line| {
            let lower = line.text.to_ascii_lowercase();
            let outcome = ["done", "wontdo", "blocked"]
                .into_iter()
                .find(|value| lower.contains(&format!("ack({value})")))?;
            let object = extract_hash_number(&line.text)
                .map(|value| format!("item-{value}"))
                .unwrap_or_else(|| format!("ack-{outcome}"));
            Some(make_signal(
                config,
                cursor,
                SignalKind::BoardAck,
                object,
                line.position,
                "board",
            ))
        })
        .collect()
}

fn classify_events(
    config: &ResolvedConfig,
    cursor: &SourceCursor,
    lines: &[NewLine],
) -> Result<Vec<Signal>> {
    let mut signals = Vec::new();
    for line in lines {
        if line.text.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(&line.text)
            .wrap_err_with(|| format!("events.jsonl offset {} JSON 非法", line.position))?;
        let event_type = find_string(&value, &["type", "event", "kind"]).unwrap_or_default();
        let status = find_string(&value, &["to", "status", "state"]).unwrap_or_default();
        let goal = find_string(&value, &["goal_id", "goalId", "id"])
            .unwrap_or_else(|| format!("event-{}", line.position));
        let kind = if event_type.eq_ignore_ascii_case("escalation") {
            Some(SignalKind::Escalation)
        } else if event_type.eq_ignore_ascii_case("goal_transition")
            && status.eq_ignore_ascii_case("blocked")
        {
            Some(SignalKind::GoalBlocked)
        } else if (event_type.eq_ignore_ascii_case("goal_transition")
            && status.eq_ignore_ascii_case("budget_limited"))
            || event_type.eq_ignore_ascii_case("budget_limited")
        {
            Some(SignalKind::BudgetLimited)
        } else {
            None
        };
        if let Some(kind) = kind {
            signals.push(make_signal(
                config,
                cursor,
                kind,
                goal,
                line.position,
                "events",
            ));
        }
    }
    Ok(signals)
}

fn find_string(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(text) = map.get(*key).and_then(Value::as_str) {
                    return Some(text.to_owned());
                }
            }
            map.values().find_map(|child| find_string(child, keys))
        }
        Value::Array(items) => items.iter().find_map(|child| find_string(child, keys)),
        _ => None,
    }
}

fn make_signal(
    config: &ResolvedConfig,
    cursor: &SourceCursor,
    kind: SignalKind,
    object_id: String,
    position: u64,
    source: &str,
) -> Signal {
    let identity = format!("{}:{}", cursor.file_id, cursor.generation);
    let normalized = format!(
        "{}|{}|{}|{:?}|{}",
        config.project_id, identity, position, kind, object_id
    );
    Signal {
        id: sha256_hex(normalized.as_bytes()),
        kind,
        object_id,
        source: source.to_owned(),
        position,
    }
}

fn extract_hash_number(line: &str) -> Option<u64> {
    // 取第一个“后随 ASCII 数字”的 `#`：markdown 标题自身以 `### ` 开头，
    // 简单 split_once('#') 会落在标题记号上而提取不到条目号。
    for (index, _) in line.match_indices('#') {
        let digits: String = line[index + 1..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if !digits.is_empty() {
            return digits.parse().ok();
        }
    }
    None
}

fn minimum_unacked_active_item(board: &Path) -> Result<Option<String>> {
    let text = fs::read_to_string(board)?;
    let mut in_active = false;
    let mut current: Option<(u64, bool)> = None;
    let mut pending = Vec::new();
    let flush = |current: &mut Option<(u64, bool)>, pending: &mut Vec<u64>| {
        if let Some((number, acked)) = current.take()
            && !acked
        {
            pending.push(number);
        }
    };
    for line in text.lines() {
        if line.starts_with("## ") {
            flush(&mut current, &mut pending);
            in_active = line.trim().eq_ignore_ascii_case("## active");
            continue;
        }
        if !in_active {
            continue;
        }
        if line.starts_with("### ") {
            flush(&mut current, &mut pending);
            if let Some(number) = extract_hash_number(line) {
                current = Some((number, false));
            }
        } else if line.to_ascii_lowercase().contains("ack(")
            && let Some((number, acked)) = current.as_mut()
        {
            // ACK 按编号归属：编号不匹配（如小节内的历史 #9 ACK）不误伤当前条目；
            // 无编号 ACK 才兜底归属于所在小节。
            let belongs = match extract_hash_number(line) {
                Some(ack_number) => ack_number == *number,
                None => true,
            };
            if belongs {
                *acked = true;
            }
        }
    }
    flush(&mut current, &mut pending);
    Ok(pending.into_iter().min().map(|value| value.to_string()))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ProgressFacts {
    board_highwater: u64,
    active_item_ack_state: String,
    goal_ledger_highwater: String,
    git_head: String,
}

fn progress_facts<A: WatchdogAdapters>(
    config: &ResolvedConfig,
    adapters: &A,
) -> Result<ProgressFacts> {
    let board_highwater = fs::metadata(&config.board)?.len();
    let active_item_ack_state = minimum_unacked_active_item(&config.board)?
        .map(|value| format!("pending:{value}"))
        .unwrap_or_else(|| "all-acked".to_owned());
    let state_home = std::env::var_os("OCTOS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".octos"));
    let ledger_root = super::obs::resolve_profile_data_root(
        &state_home,
        &config.project,
        super::obs::DEFAULT_PROFILE_ID,
    )
    .join("goal-ledgers");
    let goal_ledger_highwater = directory_highwater(&ledger_root)?;
    let git_head = adapters.git_head(&config.project)?;
    Ok(ProgressFacts {
        board_highwater,
        active_item_ack_state,
        goal_ledger_highwater,
        git_head,
    })
}

fn progress_fingerprint<A: WatchdogAdapters>(
    config: &ResolvedConfig,
    adapters: &A,
) -> Result<String> {
    let facts = progress_facts(config, adapters)?;
    Ok(sha256_hex(serde_json::to_vec(&facts)?.as_slice()))
}

fn directory_highwater(root: &Path) -> Result<String> {
    if !root.exists() {
        return Ok("missing".to_owned());
    }
    let mut entries = Vec::new();
    collect_highwater(root, root, &mut entries)?;
    entries.sort();
    Ok(sha256_hex(entries.join("\n").as_bytes()))
}

fn collect_highwater(root: &Path, current: &Path, out: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect_highwater(root, &path, out)?;
        } else if metadata.is_file() {
            let modified = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |value| value.as_nanos());
            out.push(format!(
                "{}:{}:{modified}",
                path.strip_prefix(root).unwrap_or(&path).display(),
                metadata.len()
            ));
        }
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct AgentInfo {
    #[serde(alias = "kind", alias = "name")]
    agent: String,
    #[serde(default, alias = "status")]
    agent_status: String,
    #[serde(default)]
    cwd: Option<PathBuf>,
    #[serde(default)]
    foreground_cwd: Option<PathBuf>,
    #[serde(alias = "pane", alias = "paneId")]
    pane_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MatchResult {
    Unique(AgentInfo),
    None,
    Ambiguous(Vec<String>),
}

fn match_agent(agents: &[AgentInfo], role: &str, project: &Path) -> MatchResult {
    let mut matches = Vec::new();
    for agent in agents {
        if agent.agent != role {
            continue;
        }
        let exact = [agent.foreground_cwd.as_deref(), agent.cwd.as_deref()]
            .into_iter()
            .flatten()
            .any(|path| dunce::canonicalize(path).ok().as_deref() == Some(project));
        if exact {
            matches.push(agent.clone());
        }
    }
    match matches.len() {
        0 => MatchResult::None,
        1 => MatchResult::Unique(matches.remove(0)),
        _ => MatchResult::Ambiguous(matches.into_iter().map(|item| item.pane_id).collect()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DutyState {
    Held { holder: String },
    Vacant,
    Error(String),
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromptReceipt {
    Accepted,
    Blocked,
    Stalled,
    Timeout,
    Failed,
}

trait WatchdogAdapters {
    fn agent_list(&self) -> Result<Vec<AgentInfo>>;
    fn duty_check(&self, project: &Path) -> Result<DutyState>;
    fn prompt(&self, pane: &str, text: &str) -> Result<PromptReceipt>;
    fn git_head(&self, project: &Path) -> Result<String>;
}

struct SystemAdapters;

impl WatchdogAdapters for SystemAdapters {
    fn agent_list(&self) -> Result<Vec<AgentInfo>> {
        let output = ProcessCommand::new("herdr")
            .args(["agent", "list"])
            .output()
            .wrap_err("运行 herdr agent list 失败")?;
        if !output.status.success() {
            bail!("herdr agent list 返回非零状态");
        }
        parse_agent_list(&output.stdout)
    }

    fn duty_check(&self, project: &Path) -> Result<DutyState> {
        let output = ProcessCommand::new("octoscode")
            .args(["outer-duty", "check", "--project"])
            .arg(project)
            .output()
            .wrap_err("运行 outer-duty check 失败")?;
        Ok(parse_duty_output(&output))
    }

    fn prompt(&self, pane: &str, text: &str) -> Result<PromptReceipt> {
        let output = ProcessCommand::new("herdr")
            .args(["agent", "prompt", pane, text])
            .output()
            .wrap_err("运行 herdr agent prompt 失败")?;
        Ok(parse_prompt_output(&output))
    }

    fn git_head(&self, project: &Path) -> Result<String> {
        let output = ProcessCommand::new("git")
            .arg("-C")
            .arg(project)
            .args(["rev-parse", "HEAD"])
            .output()?;
        if !output.status.success() {
            bail!("读取 Git HEAD 失败");
        }
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }
}

fn parse_agent_list(bytes: &[u8]) -> Result<Vec<AgentInfo>> {
    let value: Value = serde_json::from_slice(bytes).wrap_err("herdr agent list 输出不是 JSON")?;
    fn agents_value(value: &Value) -> Option<&Value> {
        match value {
            Value::Object(map) => map
                .get("agents")
                .or_else(|| map.values().find_map(agents_value)),
            Value::Array(_) => Some(value),
            _ => None,
        }
    }
    let agents = agents_value(&value).ok_or_else(|| eyre!("herdr 输出缺少 agents"))?;
    serde_json::from_value(agents.clone()).wrap_err("herdr agents 字段不符合契约")
}

fn parse_duty_output(output: &Output) -> DutyState {
    if !output.status.success() {
        return DutyState::Error("outer-duty check 返回非零状态".to_owned());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();
    match lines.next().unwrap_or_default().trim() {
        "HELD" => {
            let holder = lines
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .find_map(|value| find_string(&value, &["holder", "signature"]))
                .unwrap_or_default();
            if holder.is_empty() {
                DutyState::Error("HELD 缺少 holder 证据".to_owned())
            } else {
                DutyState::Held { holder }
            }
        }
        "VACANT" => DutyState::Vacant,
        "unsupported" | "UNSUPPORTED" => DutyState::Unsupported,
        other => DutyState::Error(format!("不可信 outer-duty 状态：{other}")),
    }
}

fn parse_prompt_output(output: &Output) -> PromptReceipt {
    if !output.status.success() {
        return PromptReceipt::Failed;
    }
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_ascii_lowercase();
    if text.contains("agent_blocked") || text.contains("\"status\":\"blocked\"") {
        PromptReceipt::Blocked
    } else if text.contains("agent_prompt_stalled") || text.contains("stalled") {
        PromptReceipt::Stalled
    } else if text.contains("timeout") || text.contains("timed out") {
        PromptReceipt::Timeout
    } else if text.contains("accepted") || text.contains("\"accepted\":true") {
        PromptReceipt::Accepted
    } else {
        PromptReceipt::Failed
    }
}

#[derive(Debug, Clone, Serialize)]
struct CycleResult {
    project: String,
    baseline_created: bool,
    detected: usize,
    delivered: usize,
    pending: usize,
    fused_tasks: Vec<String>,
    state_path: String,
}

fn run_cycle<A: WatchdogAdapters>(
    config: &ResolvedConfig,
    adapters: &A,
    now: i64,
) -> Result<CycleResult> {
    let _lock = CycleLock::acquire(config)?;
    let Some(mut state) = load_state(config)? else {
        let state = WatchdogState::baseline(config, adapters, now)?;
        save_state(config, &state)?;
        return Ok(cycle_result(config, &state, true, 0, 0));
    };

    let (next_board, board_lines) = read_incremental(&config.board, &state.board_cursor)?;
    let (next_events, event_lines) = read_incremental(&config.events, &state.event_cursor)?;
    let mut signals = classify_board(config, &next_board, &board_lines);
    signals.extend(classify_events(config, &next_events, &event_lines)?);
    let detected = signals.len();
    for signal in signals {
        if state.delivered_signal_ids.contains(&signal.id)
            || state.pending.iter().any(|item| item.signal.id == signal.id)
        {
            continue;
        }
        state.pending.push(PendingSignal {
            signal,
            attempts: 0,
            last_error: None,
        });
        state.last_signal_at = Some(timestamp(now));
    }
    state.board_cursor = next_board;
    state.event_cursor = next_events;

    let fingerprint = progress_fingerprint(config, adapters)?;
    if fingerprint != state.last_progress_fingerprint {
        for task in state.tasks.values_mut() {
            task.retry_count = 0;
            task.prompts_sent = 0;
            task.fused = false;
            task.observation_deadline_ms = None;
            task.last_progress_fingerprint = fingerprint.clone();
        }
        state.last_progress_fingerprint = fingerprint.clone();
        state.last_progress_at = Some(timestamp(now));
    }
    state.updated_at = timestamp(now);
    save_state(config, &state)?;

    let needs_agents = state
        .pending
        .iter()
        .any(|item| item.signal.kind.targets_outer())
        || minimum_unacked_active_item(&config.board)?.is_some();
    let agents = if needs_agents {
        match adapters.agent_list() {
            Ok(value) => Some(value),
            Err(error) => {
                write_alert(
                    config,
                    "high",
                    None,
                    "agent_list_failed",
                    0,
                    &error.to_string(),
                )?;
                None
            }
        }
    } else {
        None
    };

    let mut delivered = 0;
    if let Some(agents) = agents.as_deref() {
        delivered += dispatch_outer_pending(config, adapters, agents, &mut state, now)?;
        delivered += handle_inner_idle(config, adapters, agents, &mut state, &fingerprint, now)?;
    }
    state.updated_at = timestamp(now);
    save_state(config, &state)?;
    Ok(cycle_result(config, &state, false, detected, delivered))
}

fn dispatch_outer_pending<A: WatchdogAdapters>(
    config: &ResolvedConfig,
    adapters: &A,
    agents: &[AgentInfo],
    state: &mut WatchdogState,
    now: i64,
) -> Result<usize> {
    let outer = match match_agent(agents, &config.outer_kind, &config.project) {
        MatchResult::Unique(agent) => agent,
        MatchResult::None => {
            if state
                .pending
                .iter()
                .any(|item| item.signal.kind.targets_outer())
            {
                write_alert(
                    config,
                    "high",
                    None,
                    "outer_missing",
                    0,
                    "没有项目精确匹配的外环",
                )?;
            }
            return Ok(0);
        }
        MatchResult::Ambiguous(panes) => {
            write_alert(
                config,
                "high",
                None,
                "outer_ambiguous",
                0,
                &format!("候选窗格：{}", panes.join(",")),
            )?;
            return Ok(0);
        }
    };
    let mut delivered = 0;
    let mut delivered_ids = Vec::new();
    for item in state
        .pending
        .iter_mut()
        .filter(|item| item.signal.kind.targets_outer())
    {
        let duty = adapters.duty_check(&config.project)?;
        state.last_outer_duty = duty_label(&duty).to_owned();
        let DutyState::Held { holder } = duty else {
            item.attempts = item.attempts.saturating_add(1);
            item.last_error = Some("outer-duty 非 HELD".to_owned());
            write_alert(
                config,
                "high",
                Some(&item.signal.id),
                "outer_duty_not_held",
                item.attempts,
                "信号保留 pending，未获取或接管 outer-duty",
            )?;
            continue;
        };
        let text = outer_prompt(config, &item.signal, &holder);
        item.attempts = item.attempts.saturating_add(1);
        match adapters.prompt(&outer.pane_id, &text)? {
            PromptReceipt::Accepted => {
                item.last_error = None;
                delivered_ids.push(item.signal.id.clone());
                delivered += 1;
                state.last_signal_at = Some(timestamp(now));
            }
            receipt => {
                item.last_error = Some(format!("prompt 未接受：{receipt:?}"));
                write_alert(
                    config,
                    "high",
                    Some(&item.signal.id),
                    "outer_prompt_not_accepted",
                    item.attempts,
                    "外环门铃未被明确接受；信号保持 pending",
                )?;
            }
        }
    }
    for id in delivered_ids {
        state.delivered_signal_ids.insert(id.clone());
        state.pending.retain(|item| item.signal.id != id);
    }
    Ok(delivered)
}

fn handle_inner_idle<A: WatchdogAdapters>(
    config: &ResolvedConfig,
    adapters: &A,
    agents: &[AgentInfo],
    state: &mut WatchdogState,
    fingerprint: &str,
    now: i64,
) -> Result<usize> {
    let Some(item_id) = minimum_unacked_active_item(&config.board)? else {
        state.idle_since_ms = None;
        return Ok(0);
    };
    let inner = match match_agent(agents, &config.inner_kind, &config.project) {
        MatchResult::Unique(agent) => agent,
        MatchResult::None => return Ok(0),
        MatchResult::Ambiguous(panes) => {
            write_alert(
                config,
                "high",
                None,
                "inner_ambiguous",
                0,
                &format!("候选窗格：{}", panes.join(",")),
            )?;
            return Ok(0);
        }
    };
    let status = inner.agent_status.to_ascii_lowercase();
    if status == "blocked" {
        write_alert(
            config,
            "high",
            None,
            "inner_blocked",
            0,
            "内环处于 blocked，需外环裁决",
        )?;
        return Ok(0);
    }
    if status != "idle" && status != "done" {
        state.idle_since_ms = None;
        return Ok(0);
    }
    let idle_since = *state.idle_since_ms.get_or_insert(now);
    if now.saturating_sub(idle_since) < (config.idle_after_secs as i64 * 1000) {
        return Ok(0);
    }

    let task = state.tasks.entry(item_id.clone()).or_default();
    if task.last_progress_fingerprint.is_empty() {
        task.last_progress_fingerprint = fingerprint.to_owned();
    }
    if task.last_progress_fingerprint != fingerprint {
        task.retry_count = 0;
        task.prompts_sent = 0;
        task.fused = false;
        task.observation_deadline_ms = None;
        task.last_progress_fingerprint = fingerprint.to_owned();
    }
    if let Some(deadline) = task.observation_deadline_ms {
        if now < deadline {
            return Ok(0);
        }
        task.retry_count = task.retry_count.saturating_add(1);
        task.observation_deadline_ms = None;
        if task.retry_count >= config.max_no_progress_retries {
            task.fused = true;
            write_alert(
                config,
                "critical",
                task.last_prompt_signal_id.as_deref(),
                "inner_no_progress_fused",
                u32::from(task.retry_count),
                "已停止内环自动续推；需要外环或 operator 裁决",
            )?;
            return Ok(0);
        }
    }
    if task.fused {
        return Ok(0);
    }
    let signal = make_inner_signal(config, &item_id, task.prompts_sent.saturating_add(1));
    let text = inner_prompt(config, &signal, &item_id);
    match adapters.prompt(&inner.pane_id, &text)? {
        PromptReceipt::Accepted => {
            task.prompts_sent = task.prompts_sent.saturating_add(1);
            task.last_prompt_signal_id = Some(signal.id.clone());
            task.observation_deadline_ms =
                Some(now.saturating_add(config.observation_window_secs as i64 * 1000));
            state.delivered_signal_ids.insert(signal.id);
            state.last_signal_at = Some(timestamp(now));
            Ok(1)
        }
        receipt => {
            write_alert(
                config,
                "high",
                Some(&signal.id),
                "inner_prompt_not_accepted",
                u32::from(task.prompts_sent.saturating_add(1)),
                &format!("基础设施回执：{receipt:?}；不增加业务无进展计数"),
            )?;
            Ok(0)
        }
    }
}

fn make_inner_signal(config: &ResolvedConfig, item_id: &str, attempt: u8) -> Signal {
    let normalized = format!(
        "{}|inner_idle_pending|{}|{}",
        config.project_id, item_id, attempt
    );
    Signal {
        id: sha256_hex(normalized.as_bytes()),
        kind: SignalKind::InnerIdlePending,
        object_id: item_id.to_owned(),
        source: "board-active".to_owned(),
        position: u64::from(attempt),
    }
}

fn duty_label(duty: &DutyState) -> &'static str {
    match duty {
        DutyState::Held { .. } => "HELD",
        DutyState::Vacant => "VACANT",
        DutyState::Error(_) => "ERROR",
        DutyState::Unsupported => "unsupported",
    }
}

fn outer_prompt(config: &ResolvedConfig, signal: &Signal, holder: &str) -> String {
    format!(
        "[octos-watchdog signal_id={}] 项目={}；信号={:?}；证据={}:{}；对象={}；当前 holder={}。请从权威源复验并裁决；本门铃不授权执行 merge/verify/deploy/smoke/archive/push，且绝不放行 loop-exhausted。",
        signal.id,
        config.project.display(),
        signal.kind,
        signal.source,
        signal.position,
        signal.object_id,
        holder
    )
}

fn inner_prompt(config: &ResolvedConfig, signal: &Signal, item_id: &str) -> String {
    format!(
        "[octos-watchdog signal_id={}] 项目={}；内环已空闲且 Active 最小未 ACK 条目为 #{}。请读取黑板权威原文后继续一个回合；本门铃不扩大授权边界。",
        signal.id,
        config.project.display(),
        item_id
    )
}

#[derive(Debug, Serialize)]
struct AlertRecord<'a> {
    severity: &'a str,
    project: String,
    signal_id: Option<&'a str>,
    reason: &'a str,
    attempt: u32,
    next_action: String,
    timestamp: String,
}

fn write_alert(
    config: &ResolvedConfig,
    severity: &str,
    signal_id: Option<&str>,
    reason: &str,
    attempt: u32,
    next_action: &str,
) -> Result<()> {
    let record = AlertRecord {
        severity,
        project: config.project.to_string_lossy().into_owned(),
        signal_id,
        reason,
        attempt,
        next_action: redact(next_action),
        timestamp: Utc::now().to_rfc3339(),
    };
    let line = serde_json::to_string(&record)?;
    if config.alerts_stderr {
        eprintln!("{line}");
    }
    if let Some(parent) = config.alerts_file.parent() {
        ensure_private_dir(parent)?;
    }
    let mut file = private_open(&config.alerts_file, true)?;
    writeln!(file, "{line}")?;
    file.flush()?;
    Ok(())
}

fn redact(input: &str) -> String {
    let mut output = input.replace(['\n', '\r'], " ");
    for marker in ["Bearer ", "token=", "api_key=", "password="] {
        let marker_lower = marker.to_ascii_lowercase();
        // 游标前移：跳过已写入的 "[REDACTED]"，避免对同一 marker 反复替换造成死循环。
        let mut search_from = 0;
        while let Some(rel) = output[search_from..]
            .to_ascii_lowercase()
            .find(&marker_lower)
        {
            let start = search_from + rel;
            let value_start = start + marker.len();
            let value_end = output[value_start..]
                .find(char::is_whitespace)
                .map_or(output.len(), |offset| value_start + offset);
            output.replace_range(value_start..value_end, "[REDACTED]");
            search_from = value_start + "[REDACTED]".len();
        }
    }
    output.chars().take(256).collect()
}

fn cycle_result(
    config: &ResolvedConfig,
    state: &WatchdogState,
    baseline_created: bool,
    detected: usize,
    delivered: usize,
) -> CycleResult {
    CycleResult {
        project: state.project.clone(),
        baseline_created,
        detected,
        delivered,
        pending: state.pending.len(),
        fused_tasks: state
            .tasks
            .iter()
            .filter(|(_, task)| task.fused)
            .map(|(id, _)| id.clone())
            .collect(),
        state_path: state_path(config).to_string_lossy().into_owned(),
    }
}

#[derive(Debug, Serialize)]
struct StatusView {
    service: &'static str,
    project: String,
    sources: BTreeMap<&'static str, &'static str>,
    outer_duty: String,
    pending_count: usize,
    fused_tasks: Vec<String>,
    last_signal_at: Option<String>,
    last_progress_at: Option<String>,
    board_cursor: SourceCursor,
    event_cursor: SourceCursor,
}

fn read_status(config: &ResolvedConfig) -> Result<StatusView> {
    let state = load_state(config)?;
    let mut sources = BTreeMap::new();
    sources.insert(
        "board",
        if config.board.is_file() {
            "healthy"
        } else {
            "fault"
        },
    );
    sources.insert(
        "events",
        if config.events.is_file() {
            "healthy"
        } else {
            "fault"
        },
    );
    if let Some(state) = state {
        Ok(StatusView {
            service: "unknown",
            project: state.project,
            sources,
            outer_duty: state.last_outer_duty,
            pending_count: state.pending.len(),
            fused_tasks: state
                .tasks
                .iter()
                .filter(|(_, task)| task.fused)
                .map(|(id, _)| id.clone())
                .collect(),
            last_signal_at: state.last_signal_at,
            last_progress_at: state.last_progress_at,
            board_cursor: state.board_cursor,
            event_cursor: state.event_cursor,
        })
    } else {
        Ok(StatusView {
            service: "inactive",
            project: config.project.to_string_lossy().into_owned(),
            sources,
            outer_duty: "unknown".to_owned(),
            pending_count: 0,
            fused_tasks: Vec::new(),
            last_signal_at: None,
            last_progress_at: None,
            board_cursor: SourceCursor::default(),
            event_cursor: SourceCursor::default(),
        })
    }
}

fn print_cycle(result: &CycleResult, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string(result)?);
    } else {
        println!(
            "watchdog cycle: detected={} delivered={} pending={} fused={}",
            result.detected,
            result.delivered,
            result.pending,
            result.fused_tasks.len()
        );
    }
    Ok(())
}

fn print_status(status: &StatusView, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string(status)?);
    } else {
        println!(
            "watchdog status: service={} project={} pending={} fused={} outer-duty={}",
            status.service,
            status.project,
            status.pending_count,
            status.fused_tasks.len(),
            status.outer_duty
        );
    }
    Ok(())
}

fn run_forever(config: ResolvedConfig) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        #[cfg(unix)]
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        loop {
            let result = run_cycle(&config, &SystemAdapters, now_ms())?;
            tracing::info!(
                detected = result.detected,
                delivered = result.delivered,
                pending = result.pending,
                "Watchdog 巡检完成"
            );
            #[cfg(unix)]
            tokio::select! {
                _ = tokio::signal::ctrl_c() => break,
                _ = terminate.recv() => break,
                _ = tokio::time::sleep(Duration::from_secs(config.poll_interval_secs)) => {}
            }
            #[cfg(not(unix))]
            tokio::select! {
                _ = tokio::signal::ctrl_c() => break,
                _ = tokio::time::sleep(Duration::from_secs(config.poll_interval_secs)) => {}
            }
        }
        Ok(())
    })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

fn timestamp(ms: i64) -> String {
    chrono::DateTime::<Utc>::from_timestamp_millis(ms)
        .unwrap_or_else(Utc::now)
        .to_rfc3339()
}

#[cfg(test)]
mod tests;
