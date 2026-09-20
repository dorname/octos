use super::*;

use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Mutex, Once, OnceLock};
use std::time::Instant;

static REPORT_INIT: Once = Once::new();
static REPORT_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn result_path() -> PathBuf {
    std::env::var_os("OPENLOGOS_RESULT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            // cargo test 的工作目录是 crate 目录；账本固定在 workspace 根的
            // logos/resources/verify/test-results.jsonl。
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../logos/resources/verify/test-results.jsonl")
        })
}

fn report(id: &str, status: &str, error: Option<&str>, duration_ms: u128) {
    let path = result_path();
    let _guard = REPORT_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("reporter lock");
    REPORT_INIT.call_once(|| {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("创建 reporter 目录");
        }
        fs::write(&path, b"").expect("清空 reporter 结果");
    });
    let mut record = serde_json::json!({
        "id": id,
        "status": status,
        "duration_ms": duration_ms,
        "timestamp": Utc::now().to_rfc3339(),
        "scenario": "S17"
    });
    if let Some(error) = error {
        record["error"] = Value::String(error.chars().take(500).collect());
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("打开 reporter 结果");
    writeln!(file, "{}", serde_json::to_string(&record).unwrap()).unwrap();
}

fn case(id: &str, body: impl FnOnce()) {
    let start = Instant::now();
    let result = catch_unwind(AssertUnwindSafe(body));
    match result {
        Ok(()) => report(id, "pass", None, start.elapsed().as_millis()),
        Err(payload) => {
            let error = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| {
                    payload
                        .downcast_ref::<&str>()
                        .map(|text| (*text).to_owned())
                })
                .unwrap_or_else(|| "测试 panic".to_owned());
            report(id, "fail", Some(&error), start.elapsed().as_millis());
            std::panic::resume_unwind(payload);
        }
    }
}

fn skip(id: &str, reason: &str) {
    report(id, "skip", Some(reason), 0);
}

struct Fixture {
    _temp: tempfile::TempDir,
    config: ResolvedConfig,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("tempdir");
        let project = temp.path().join("project");
        fs::create_dir_all(project.join(".octos")).unwrap();
        let board = project.join(".octos/OUTER_LOOP_REVIEW.md");
        fs::write(&board, "# OLP\n\n## Active\n\n### #1 待办\n尚未完成\n").unwrap();
        let events = temp.path().join("events.jsonl");
        fs::write(&events, "").unwrap();
        let state_dir = temp.path().join("state");
        let alerts_file = state_dir.join("alerts.jsonl");
        Self {
            _temp: temp,
            config: ResolvedConfig {
                project: dunce::canonicalize(&project).unwrap(),
                project_id: project_id(&dunce::canonicalize(&project).unwrap()),
                board,
                events,
                poll_interval_secs: 1,
                idle_after_secs: 0,
                observation_window_secs: 1,
                max_no_progress_retries: 3,
                inner_kind: "octoscode".to_owned(),
                outer_kind: "claude".to_owned(),
                state_dir,
                alerts_stderr: false,
                alerts_file,
            },
        }
    }

    fn append_board(&self, text: &str) {
        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.config.board)
            .unwrap();
        write!(file, "{text}").unwrap();
    }

    fn append_event(&self, value: Value) {
        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.config.events)
            .unwrap();
        writeln!(file, "{}", serde_json::to_string(&value).unwrap()).unwrap();
    }
}

struct FakeAdapters {
    agents: Mutex<Vec<AgentInfo>>,
    duty: Mutex<DutyState>,
    receipts: Mutex<VecDeque<PromptReceipt>>,
    prompts: Mutex<Vec<(String, String)>>,
    git_head: Mutex<String>,
    agent_list_calls: Mutex<u32>,
    duty_calls: Mutex<u32>,
}

impl FakeAdapters {
    fn for_project(project: &Path) -> Self {
        Self {
            agents: Mutex::new(vec![
                AgentInfo {
                    agent: "octoscode".to_owned(),
                    agent_status: "idle".to_owned(),
                    cwd: Some(project.to_path_buf()),
                    foreground_cwd: None,
                    pane_id: "inner:p1".to_owned(),
                },
                AgentInfo {
                    agent: "claude".to_owned(),
                    agent_status: "idle".to_owned(),
                    cwd: Some(project.to_path_buf()),
                    foreground_cwd: None,
                    pane_id: "outer:p2".to_owned(),
                },
            ]),
            duty: Mutex::new(DutyState::Held {
                holder: "claude-holder".to_owned(),
            }),
            receipts: Mutex::new(VecDeque::new()),
            prompts: Mutex::new(Vec::new()),
            git_head: Mutex::new("head-a".to_owned()),
            agent_list_calls: Mutex::new(0),
            duty_calls: Mutex::new(0),
        }
    }

    fn push_receipts(&self, values: impl IntoIterator<Item = PromptReceipt>) {
        self.receipts.lock().unwrap().extend(values);
    }

    fn prompt_count(&self) -> usize {
        self.prompts.lock().unwrap().len()
    }
}

impl WatchdogAdapters for FakeAdapters {
    fn agent_list(&self) -> Result<Vec<AgentInfo>> {
        *self.agent_list_calls.lock().unwrap() += 1;
        Ok(self.agents.lock().unwrap().clone())
    }

    fn duty_check(&self, _project: &Path) -> Result<DutyState> {
        *self.duty_calls.lock().unwrap() += 1;
        Ok(self.duty.lock().unwrap().clone())
    }

    fn prompt(&self, pane: &str, text: &str) -> Result<PromptReceipt> {
        self.prompts
            .lock()
            .unwrap()
            .push((pane.to_owned(), text.to_owned()));
        Ok(self
            .receipts
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(PromptReceipt::Accepted))
    }

    fn git_head(&self, _project: &Path) -> Result<String> {
        Ok(self.git_head.lock().unwrap().clone())
    }
}

fn baseline(fixture: &Fixture, fake: &FakeAdapters, now: i64) -> CycleResult {
    run_cycle(&fixture.config, fake, now).expect("baseline")
}

fn state(fixture: &Fixture) -> WatchdogState {
    load_state(&fixture.config).unwrap().unwrap()
}

#[test]
fn ut_s17_01_board_baseline_only_new_domain() {
    case("UT-S17-01", || {
        let fixture = Fixture::new();
        fixture.append_board("ACK(done): #1 历史\n");
        let fake = FakeAdapters::for_project(&fixture.config.project);
        let first = baseline(&fixture, &fake, 1_000);
        let second = run_cycle(&fixture.config, &fake, 1_001).unwrap();
        assert!(first.baseline_created);
        assert_eq!(second.detected, 0);
    });
}

#[test]
fn ut_s17_02_classify_v1_ack() {
    case("UT-S17-02", || {
        let fixture = Fixture::new();
        let cursor = cursor_at_eof(&fixture.config.board).unwrap();
        let lines = ["ACK(done): #2", "ACK(wontdo): #3", "ACK(blocked): #4"]
            .into_iter()
            .enumerate()
            .map(|(position, text)| NewLine {
                position: position as u64,
                text: text.to_owned(),
            })
            .collect::<Vec<_>>();
        let signals = classify_board(&fixture.config, &cursor, &lines);
        assert_eq!(signals.len(), 3);
        assert!(
            signals
                .iter()
                .all(|signal| signal.kind == SignalKind::BoardAck)
        );
    });
}

#[test]
fn ut_s17_03_classify_runtime_negative_signals() {
    case("UT-S17-03", || {
        let fixture = Fixture::new();
        let cursor = cursor_at_eof(&fixture.config.events).unwrap();
        let values = [
            serde_json::json!({"type":"goal_transition","to":"blocked","goal_id":"g1"}),
            serde_json::json!({"type":"escalation","goal_id":"g2"}),
            serde_json::json!({"type":"goal_transition","status":"budget_limited","goal_id":"g3"}),
        ];
        let lines = values
            .iter()
            .enumerate()
            .map(|(position, value)| NewLine {
                position: position as u64,
                text: value.to_string(),
            })
            .collect::<Vec<_>>();
        let signals = classify_events(&fixture.config, &cursor, &lines).unwrap();
        assert_eq!(signals.len(), 3);
        assert_eq!(signals[0].kind, SignalKind::GoalBlocked);
        assert_eq!(signals[1].kind, SignalKind::Escalation);
        assert_eq!(signals[2].kind, SignalKind::BudgetLimited);
    });
}

#[test]
fn ut_s17_04_signal_id_stable() {
    case("UT-S17-04", || {
        let fixture = Fixture::new();
        let cursor = cursor_at_eof(&fixture.config.board).unwrap();
        let one = make_signal(
            &fixture.config,
            &cursor,
            SignalKind::BoardAck,
            "1".into(),
            7,
            "board",
        );
        let two = make_signal(
            &fixture.config,
            &cursor,
            SignalKind::BoardAck,
            "1".into(),
            7,
            "board",
        );
        assert_eq!(one.id, two.id);
    });
}

#[test]
fn ut_s17_05_delivered_deduplicates() {
    case("UT-S17-05", || {
        let fixture = Fixture::new();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        baseline(&fixture, &fake, 1_000);
        fixture.append_board("ACK(done): #1 新增\n");
        run_cycle(&fixture.config, &fake, 1_001).unwrap();
        let count = fake.prompt_count();
        run_cycle(&fixture.config, &fake, 1_002).unwrap();
        assert_eq!(fake.prompt_count(), count);
    });
}

#[test]
fn ut_s17_06_exact_canonical_cwd_match() {
    case("UT-S17-06", || {
        let fixture = Fixture::new();
        let prefix = fixture.config.project.parent().unwrap().to_path_buf();
        let agents = vec![
            AgentInfo {
                agent: "claude".into(),
                agent_status: "idle".into(),
                cwd: Some(prefix),
                foreground_cwd: None,
                pane_id: "wrong".into(),
            },
            AgentInfo {
                agent: "claude".into(),
                agent_status: "idle".into(),
                cwd: Some(fixture.config.project.clone()),
                foreground_cwd: None,
                pane_id: "right".into(),
            },
        ];
        assert!(
            matches!(match_agent(&agents, "claude", &fixture.config.project), MatchResult::Unique(agent) if agent.pane_id == "right")
        );
    });
}

#[test]
fn ut_s17_07_multiple_candidates_fail_closed() {
    case("UT-S17-07", || {
        let fixture = Fixture::new();
        let agents = ["a", "b"]
            .into_iter()
            .map(|pane| AgentInfo {
                agent: "claude".into(),
                agent_status: "idle".into(),
                cwd: Some(fixture.config.project.clone()),
                foreground_cwd: None,
                pane_id: pane.into(),
            })
            .collect::<Vec<_>>();
        assert!(
            matches!(match_agent(&agents, "claude", &fixture.config.project), MatchResult::Ambiguous(panes) if panes.len() == 2)
        );
    });
}

#[test]
fn ut_s17_08_held_outer_dispatches() {
    case("UT-S17-08", || {
        let fixture = Fixture::new();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        baseline(&fixture, &fake, 1_000);
        fixture.append_board("ACK(done): #1\n");
        let result = run_cycle(&fixture.config, &fake, 1_001).unwrap();
        assert_eq!(result.delivered, 1);
        assert!(fake.prompts.lock().unwrap()[0].0.starts_with("outer"));
    });
}

#[test]
fn ut_s17_09_untrusted_duty_does_not_dispatch() {
    case("UT-S17-09", || {
        for duty in [
            DutyState::Vacant,
            DutyState::Error("x".into()),
            DutyState::Unsupported,
        ] {
            let fixture = Fixture::new();
            let fake = FakeAdapters::for_project(&fixture.config.project);
            *fake.duty.lock().unwrap() = duty;
            baseline(&fixture, &fake, 1_000);
            fixture.append_board("ACK(done): #1\n");
            let result = run_cycle(&fixture.config, &fake, 1_001).unwrap();
            assert_eq!(result.delivered, 0);
            assert_eq!(result.pending, 1);
        }
    });
}

#[test]
fn ut_s17_10_fingerprint_covers_three_progress_classes() {
    case("UT-S17-10", || {
        let base = ProgressFacts {
            board_highwater: 1,
            active_item_ack_state: "p".into(),
            goal_ledger_highwater: "l1".into(),
            git_head: "h1".into(),
        };
        let hash = |facts: &ProgressFacts| sha256_hex(&serde_json::to_vec(facts).unwrap());
        let mut board = base.clone();
        board.board_highwater = 2;
        let mut ledger = base.clone();
        ledger.goal_ledger_highwater = "l2".into();
        let mut git = base.clone();
        git.git_head = "h2".into();
        assert_ne!(hash(&base), hash(&board));
        assert_ne!(hash(&base), hash(&ledger));
        assert_ne!(hash(&base), hash(&git));
    });
}

#[test]
fn ut_s17_11_agent_status_jitter_not_progress() {
    case("UT-S17-11", || {
        let facts = ProgressFacts {
            board_highwater: 1,
            active_item_ack_state: "p".into(),
            goal_ledger_highwater: "l".into(),
            git_head: "h".into(),
        };
        let before = sha256_hex(&serde_json::to_vec(&facts).unwrap());
        let _status_changes = ["working", "idle", "done"];
        let after = sha256_hex(&serde_json::to_vec(&facts).unwrap());
        assert_eq!(before, after);
    });
}

#[test]
fn ut_s17_12_progress_resets_retry() {
    case("UT-S17-12", || {
        let fixture = Fixture::new();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        baseline(&fixture, &fake, 1_000);
        {
            let mut current = state(&fixture);
            current.tasks.insert(
                "1".into(),
                TaskState {
                    retry_count: 2,
                    fused: true,
                    last_progress_fingerprint: current.last_progress_fingerprint.clone(),
                    ..Default::default()
                },
            );
            save_state(&fixture.config, &current).unwrap();
        }
        *fake.git_head.lock().unwrap() = "head-b".into();
        run_cycle(&fixture.config, &fake, 1_001).unwrap();
        let task = state(&fixture).tasks["1"].clone();
        assert_eq!(task.retry_count, 0);
        assert!(!task.fused);
    });
}

#[test]
fn ut_s17_13_third_no_progress_fuses() {
    case("UT-S17-13", || {
        let fixture = Fixture::new();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        baseline(&fixture, &fake, 0);
        for now in [1, 1_002, 2_003, 3_004, 4_005] {
            run_cycle(&fixture.config, &fake, now).unwrap();
        }
        let task = state(&fixture).tasks["1"].clone();
        assert_eq!(task.retry_count, 3);
        assert!(task.fused);
        assert_eq!(fake.prompt_count(), 3);
    });
}

#[test]
fn ut_s17_14_atomic_state_roundtrip() {
    case("UT-S17-14", || {
        let fixture = Fixture::new();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        baseline(&fixture, &fake, 1_000);
        let mut expected = state(&fixture);
        expected.delivered_signal_ids.insert("s1".into());
        expected.tasks.insert(
            "7".into(),
            TaskState {
                retry_count: 2,
                fused: true,
                ..Default::default()
            },
        );
        save_state(&fixture.config, &expected).unwrap();
        assert_eq!(load_state(&fixture.config).unwrap().unwrap(), expected);
    });
}

#[test]
fn ut_s17_15_corrupt_state_fails_closed() {
    case("UT-S17-15", || {
        let fixture = Fixture::new();
        ensure_private_dir(&fixture.config.state_dir).unwrap();
        fs::write(state_path(&fixture.config), "not-json").unwrap();
        assert!(load_state(&fixture.config).is_err());
        let fake = FakeAdapters::for_project(&fixture.config.project);
        assert!(run_cycle(&fixture.config, &fake, 1_000).is_err());
    });
}

#[test]
fn ut_s17_16_event_rotation_generation() {
    case("UT-S17-16", || {
        let fixture = Fixture::new();
        fs::write(&fixture.config.events, "{\"type\":\"escalation\"}\n").unwrap();
        let cursor = cursor_at_eof(&fixture.config.events).unwrap();
        let old = fixture.config.events.with_extension("old");
        fs::rename(&fixture.config.events, old).unwrap();
        fs::write(&fixture.config.events, "{\"type\":\"escalation\"}\n").unwrap();
        let (next, lines) = read_incremental(&fixture.config.events, &cursor).unwrap();
        assert_eq!(next.generation, 1);
        assert_eq!(lines.len(), 1);
    });
}

#[test]
fn ut_s17_17_only_accepted_is_delivered() {
    case("UT-S17-17", || {
        let fixture = Fixture::new();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        baseline(&fixture, &fake, 1_000);
        fixture.append_board("ACK(done): #1\n");
        fake.push_receipts([
            PromptReceipt::Blocked,
            PromptReceipt::Stalled,
            PromptReceipt::Timeout,
            PromptReceipt::Accepted,
        ]);
        for now in 1_001..=1_004 {
            run_cycle(&fixture.config, &fake, now).unwrap();
        }
        assert_eq!(state(&fixture).pending.len(), 0);
        assert_eq!(state(&fixture).delivered_signal_ids.len(), 1);
    });
}

#[test]
fn ut_s17_18_config_rejects_unsafe_fields() {
    case("UT-S17-18", || {
        let relative = r#"version=1
project="relative"
max_no_progress_retries=3
"#;
        let raw: WatchdogConfigFile = toml::from_str(relative).unwrap();
        assert!(validate_raw_config(&raw).is_err());
        let unlimited = r#"version=1
project="/tmp"
max_no_progress_retries=99
"#;
        let raw: WatchdogConfigFile = toml::from_str(unlimited).unwrap();
        assert!(validate_raw_config(&raw).is_err());
        let shell = r#"version=1
project="/tmp"
shell_command="rm -rf x"
"#;
        assert!(toml::from_str::<WatchdogConfigFile>(shell).is_err());
    });
}

#[test]
fn ut_s17_19_status_is_read_only() {
    case("UT-S17-19", || {
        let fixture = Fixture::new();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        baseline(&fixture, &fake, 1_000);
        let before = fs::metadata(state_path(&fixture.config))
            .unwrap()
            .modified()
            .unwrap();
        let calls = *fake.agent_list_calls.lock().unwrap();
        let view = read_status(&fixture.config).unwrap();
        let after = fs::metadata(state_path(&fixture.config))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(calls, *fake.agent_list_calls.lock().unwrap());
        assert_eq!(view.pending_count, 0);
    });
}

#[test]
fn ut_s17_20_alert_redacts_secrets() {
    case("UT-S17-20", || {
        let text = redact("Bearer secret token=abc api_key=def password=ghi\n完整 prompt");
        assert!(!text.contains("secret"));
        assert!(!text.contains("abc"));
        assert!(!text.contains("def"));
        assert!(!text.contains("ghi"));
        assert!(!text.contains('\n'));
    });
}

#[test]
fn st_s17_01_first_start_baseline() {
    case("ST-S17-01", || {
        let fixture = Fixture::new();
        fixture.append_board("ACK(done): #9 历史\n");
        fixture.append_event(serde_json::json!({"type":"escalation","goal_id":"old"}));
        let fake = FakeAdapters::for_project(&fixture.config.project);
        assert!(baseline(&fixture, &fake, 1_000).baseline_created);
        let result = run_cycle(&fixture.config, &fake, 1_001).unwrap();
        assert_eq!(result.detected, 0);
        assert_eq!(fake.prompt_count(), 1); // 当前 Active idle 门铃，不是历史信号重放。
    });
}

#[test]
fn st_s17_02_new_ack_wakes_holder_once() {
    case("ST-S17-02", || {
        let fixture = Fixture::new();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        baseline(&fixture, &fake, 1_000);
        // 避免本用例中的内环门铃干扰外环计数。
        fake.agents
            .lock()
            .unwrap()
            .retain(|agent| agent.agent == "claude");
        fixture.append_board("ACK(done): #1\n");
        assert_eq!(
            run_cycle(&fixture.config, &fake, 1_001).unwrap().delivered,
            1
        );
        assert_eq!(
            run_cycle(&fixture.config, &fake, 1_002).unwrap().delivered,
            0
        );
        assert_eq!(fake.prompt_count(), 1);
    });
}

#[test]
fn st_s17_03_blocked_and_escalation_wake_outer() {
    case("ST-S17-03", || {
        let fixture = Fixture::new();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        baseline(&fixture, &fake, 1_000);
        fake.agents
            .lock()
            .unwrap()
            .retain(|agent| agent.agent == "claude");
        fixture.append_event(
            serde_json::json!({"type":"goal_transition","to":"blocked","goal_id":"g1"}),
        );
        fixture.append_event(serde_json::json!({"type":"escalation","goal_id":"g2"}));
        let result = run_cycle(&fixture.config, &fake, 1_001).unwrap();
        assert_eq!(result.detected, 2);
        assert_eq!(result.delivered, 2);
    });
}

#[test]
fn st_s17_04_budget_limited_preserves_checkpoint() {
    case("ST-S17-04", || {
        let fixture = Fixture::new();
        let checkpoint = fixture.config.project.join("checkpoint.json");
        fs::write(&checkpoint, "immutable").unwrap();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        baseline(&fixture, &fake, 1_000);
        fake.agents
            .lock()
            .unwrap()
            .retain(|agent| agent.agent == "claude");
        fixture.append_event(
            serde_json::json!({"type":"goal_transition","to":"budget_limited","goal_id":"g"}),
        );
        assert_eq!(
            run_cycle(&fixture.config, &fake, 1_001).unwrap().delivered,
            1
        );
        assert_eq!(fs::read_to_string(checkpoint).unwrap(), "immutable");
    });
}

#[test]
fn st_s17_05_idle_pending_wakes_inner() {
    case("ST-S17-05", || {
        let fixture = Fixture::new();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        baseline(&fixture, &fake, 0);
        let result = run_cycle(&fixture.config, &fake, 1).unwrap();
        assert_eq!(result.delivered, 1);
        let prompts = fake.prompts.lock().unwrap();
        assert_eq!(prompts[0].0, "inner:p1");
        assert!(prompts[0].1.contains("#1"));
    });
}

#[test]
fn st_s17_06_real_progress_resets_state() {
    case("ST-S17-06", || {
        let fixture = Fixture::new();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        baseline(&fixture, &fake, 0);
        run_cycle(&fixture.config, &fake, 1).unwrap();
        {
            let mut current = state(&fixture);
            let task = current.tasks.get_mut("1").unwrap();
            task.retry_count = 2;
            save_state(&fixture.config, &current).unwrap();
        }
        fixture.append_board("ACK(done): #1\n");
        run_cycle(&fixture.config, &fake, 2).unwrap();
        let task = state(&fixture).tasks["1"].clone();
        assert_eq!(task.retry_count, 0);
        assert!(!task.fused);
    });
}

#[test]
fn st_s17_07_exactly_three_prompts_then_fuse() {
    case("ST-S17-07", || {
        let fixture = Fixture::new();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        baseline(&fixture, &fake, 0);
        for now in [1, 1_002, 2_003, 3_004, 4_005, 5_006] {
            run_cycle(&fixture.config, &fake, now).unwrap();
        }
        assert_eq!(fake.prompt_count(), 3);
        assert!(state(&fixture).tasks["1"].fused);
    });
}

#[test]
fn st_s17_08_restart_recovers_state() {
    case("ST-S17-08", || {
        let fixture = Fixture::new();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        baseline(&fixture, &fake, 0);
        run_cycle(&fixture.config, &fake, 1).unwrap();
        let before = state(&fixture);
        let replacement = FakeAdapters::for_project(&fixture.config.project);
        run_cycle(&fixture.config, &replacement, 2).unwrap();
        let after = state(&fixture);
        assert_eq!(
            before.tasks["1"].prompts_sent,
            after.tasks["1"].prompts_sent
        );
        assert_eq!(replacement.prompt_count(), 0);
    });
}

#[test]
fn st_s17_09_vacant_outer_keeps_pending() {
    case("ST-S17-09", || {
        let fixture = Fixture::new();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        *fake.duty.lock().unwrap() = DutyState::Vacant;
        baseline(&fixture, &fake, 1_000);
        fake.agents
            .lock()
            .unwrap()
            .retain(|agent| agent.agent == "claude");
        fixture.append_board("ACK(done): #1\n");
        let result = run_cycle(&fixture.config, &fake, 1_001).unwrap();
        assert_eq!(result.pending, 1);
        assert_eq!(fake.prompt_count(), 0);
    });
}

#[test]
fn st_s17_10_projects_and_outers_are_isolated() {
    case("ST-S17-10", || {
        let fixture = Fixture::new();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        let other = fixture.config.project.parent().unwrap().join("other");
        fs::create_dir_all(&other).unwrap();
        fake.agents.lock().unwrap().push(AgentInfo {
            agent: "claude".into(),
            agent_status: "idle".into(),
            cwd: Some(other),
            foreground_cwd: None,
            pane_id: "outer-other".into(),
        });
        baseline(&fixture, &fake, 1_000);
        fake.agents
            .lock()
            .unwrap()
            .retain(|agent| agent.agent == "claude");
        fixture.append_board("ACK(done): #1\n");
        run_cycle(&fixture.config, &fake, 1_001).unwrap();
        assert_eq!(fake.prompts.lock().unwrap()[0].0, "outer:p2");
    });
}

#[test]
fn st_s17_11_prompt_failure_then_recovery() {
    case("ST-S17-11", || {
        let fixture = Fixture::new();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        baseline(&fixture, &fake, 1_000);
        fake.agents
            .lock()
            .unwrap()
            .retain(|agent| agent.agent == "claude");
        fake.push_receipts([PromptReceipt::Timeout, PromptReceipt::Accepted]);
        fixture.append_board("ACK(done): #1\n");
        assert_eq!(run_cycle(&fixture.config, &fake, 1_001).unwrap().pending, 1);
        assert_eq!(run_cycle(&fixture.config, &fake, 1_002).unwrap().pending, 0);
        assert_eq!(fake.prompt_count(), 2);
    });
}

#[test]
fn st_s17_12_rotation_has_no_gap_or_replay() {
    case("ST-S17-12", || {
        let fixture = Fixture::new();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        baseline(&fixture, &fake, 1_000);
        fake.agents
            .lock()
            .unwrap()
            .retain(|agent| agent.agent == "claude");
        fixture.append_event(serde_json::json!({"type":"escalation","goal_id":"old-tail"}));
        run_cycle(&fixture.config, &fake, 1_001).unwrap();
        fs::rename(
            &fixture.config.events,
            fixture.config.events.with_extension("old"),
        )
        .unwrap();
        fs::write(
            &fixture.config.events,
            "{\"type\":\"escalation\",\"goal_id\":\"new\"}\n",
        )
        .unwrap();
        let result = run_cycle(&fixture.config, &fake, 1_002).unwrap();
        assert_eq!(result.detected, 1);
        assert_eq!(fake.prompt_count(), 2);
    });
}

#[test]
fn st_s17_13_systemd_restart_environment_gate() {
    // 真正的 unit 崩溃拉起由隔离部署 smoke 执行；普通 cargo test 无 user systemd 时显式 skip。
    skip(
        "ST-S17-13",
        "需要隔离的 systemd --user 部署环境，由 smoke runner 覆盖",
    );
}

#[test]
fn st_s17_14_authorization_boundary() {
    case("ST-S17-14", || {
        let fixture = Fixture::new();
        let fake = FakeAdapters::for_project(&fixture.config.project);
        baseline(&fixture, &fake, 1_000);
        fake.agents
            .lock()
            .unwrap()
            .retain(|agent| agent.agent == "claude");
        fixture.append_event(serde_json::json!({"type":"goal_transition","to":"budget_limited","goal_id":"gate:implement:loop-exhausted"}));
        run_cycle(&fixture.config, &fake, 1_001).unwrap();
        let prompt = &fake.prompts.lock().unwrap()[0].1;
        assert!(prompt.contains("不授权执行 merge/verify/deploy/smoke/archive/push"));
        assert!(prompt.contains("绝不放行 loop-exhausted"));
    });
}
