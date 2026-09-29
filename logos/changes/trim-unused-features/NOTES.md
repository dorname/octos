# NOTES — 核查结论（无实改文件）

2026-09-29 对以下文件做被删通道（dingtalk / slack / line / email / qq-bot / wechat）与 FFI（octos-ffi / uniffi / pyo3）词边界精确核查（`grep -niE '\b(...)\b'`），结论均为**无引用、无需 delta**：

| 文件 | 结论 |
|---|---|
| `prd/3-technical-plan/3-deployment/core-01-deployment-plan.md` | 无被删通道 / FFI 引用；feature 清单仅泛写 `--features "api,..."`，不含具体被删 feature |
| `prd/3-technical-plan/2-scenario-implementation/core-S04-gateway-channel.md` | 以 Telegram 为示例通道，其余均为「通道适配器」泛称，无被删通道具名引用 |
| `test/core-S01-test-cases.md` | 无被删通道 / FFI 相关用例（早前粗筛命中为 `baseline` 等子串误报） |
| `test/core-S16-test-cases.md` | 同上，无引用 |
| `prd/2-product-design/1-feature-specs/core-04-serve-api-design.md` | 无引用 |
| `prd/2-product-design/1-feature-specs/core-05-orchestration-design.md` | 无引用 |
| `prd/2-product-design/1-feature-specs/core-07-k8s-cluster-design.md` | 无引用 |
| `prd/2-product-design/1-feature-specs/core-08-octoloop-watchdog-design.md` | 无引用 |

因此 tasks.md 中对应 5 项 delta 任务以「核查无实改」闭环，不产 delta 文件。

## 切片 1 验证记录（2026-09-30）

### 门槛结果

| 门槛 | 结果 |
|---|---|
| `cargo build --workspace` | PASS（2m01s，35 member） |
| 逐 crate 全量回归 | 33/35 crate 全绿；octos-agent 2884+151 全绿；octos-cli 3927+151 通过、7 个 root 环境伪失败（见下） |

### 环境性失败（均预存，非 trim 引入；trim 对应文件零改动）

1. **octos-bus `cron_service_pg` ×4**：需本地 PostgreSQL（默认 `postgres://postgres:octos@127.0.0.1:5432/octos`），本机 5432 未监听（k8s PG 容器未映射宿主端口，不动用户集群数据）。门控跳过 `--skip cron_service_pg`。
2. **root 权限注入伪失败 ×7（octos-cli）+ ×1（octos-pipeline）**：测试用 chmod 0o555/0o000 注入「不可写/不可读」失败，root 的 CAP_DAC_OVERRIDE 击穿权限检查导致注入失效；另有 1 例 HOME=/root 触发 `workspace_root_escape_under_system_path` 守卫（/root 属受限系统路径）。**root 下必现，普通用户环境应通过**。门控跳过对应测试名。
3. **octos-agent `execute_timeout_returns_error` ×1**：WSL2 下 `/usr/bin/kill`（procps-ng）对**不带 `--` 的负 pid 参数**解析歧义，实际执行 `kill(0, SIGKILL)` 团灭调用者进程组（strace + 隔离实验确证）。触发点 `plugins/tool.rs:2938` 与 `:3056`（guard :2361 的 `--` 形式正确）。macOS/BSD 与 util-linux kill 不受影响——解释了测试注释「passes on macOS and bare-metal Linux」。该测试有 docker 检测跳过门，但 WSL2（`0::/init.scope`）不命中。**这是潜在生产级 bug：Linux + procps 环境下任何插件 stdin 超时/等待超时都会团灭 octos 进程组**。

### 上述第 3 项的修复建议（未实施——按守卫规则需另立变更提案）

`crates/octos-agent/src/plugins/tool.rs` 两处 `args(["-9", &format!("-{child_pid}")])` 改为 `args(["-9", "--", &format!("-{child_pid}")])`（与 :2361 guard 一致），并给该测试补 WSL2 跳过条件（如 `systemd-detect-virt = wsl` 或 `/proc/version` 检测）。

### 本切片实际改动清单

- 删除：`crates/octos-ffi/`（7 文件）、`crates/octos-uniffi/`（6 文件）、`crates/octos-pyo3/`（8 文件）、`scripts/check-oup-bindings.py`
- 根 `Cargo.toml`：members 38 → 35
- `scripts/milestone-ci.sh`：oup-runtime 去除 ffi/uniffi 构建与 check-oup-bindings 调用
- `crates/octos-cli/src/{config.rs,profiles.rs,runtime/profile.rs,commands/chat.rs,commands/mod.rs}`：ffi 提及注释中性化（5 处，无逻辑改动）
- `crates/octos-wasm/{src/lib.rs,Cargo.toml,README.md}`、根 `README.md`、`README-zh.md`：ffi 嵌入章节/提及清理

## 切片 2 验证记录（2026-09-30）

### 门槛结果

| 门槛 | 结果 |
|---|---|
| `cargo build --workspace --jobs 4` | PASS（1m30s，0 error 0 warning） |
| `cargo check -p octos-cli --features "api,telegram,discord,whatsapp,feishu,twilio,wecom,wecom-bot,audio_mp3"` | PASS（canonical feature 组合，默认含 embed-llama） |
| `cargo test -p octos-bus` | 263+13+15+2 = 293 PASS，0 failed |
| `cargo test -p octos-agent --lib` | 2884 PASS，3 ignored |
| `cargo test -p octos-cli --lib` | 3919 PASS，7 failed（全部定性见下） |
| `cargo test -p octos-cli --tests` | 23/23 集成测试文件全绿 |
| `cargo test -p octos-server` | 空壳 crate，0 test OK |

### octos-cli lib 7 个失败定性

**trim 引入（1，已修复）**：
- `profiles::tests::test_config_from_profile` — HEAD 原版构造 Telegram+Slack 双通道断言 `channels.len()==2`；删 Slack 条目后断言漏改。修复：断言 2→1。修复后转绿（3918→3919）。

**环境伪失败（5，root CAP_DAC_OVERRIDE / HOME=/root，均权限注入类）**：
- `commands::gateway::tests::test_delete_bot_keeps_route_when_profile_delete_fails`（profiles_dir 0o555 注入）
- `api::cron_panel::tests::toggle_via_service_surfaces_persistence_failures`（dir 0o555 @cron_panel.rs:680）
- `api::cron_panel::tests::toggle_via_service_adopts_external_writes_instead_of_erasing_them`（同套件；见预存说明）
- `autonomy::agent_orchestrator::tests::goal_verifier_ledger_write_failure_fails_closed`（ledger 0o444 @agent_orchestrator.rs:39842）
- `autonomy::agent_orchestrator::tests::outer_verifier_runtime_unwritable_new_ledger_spends_zero_calls`（data 0o555 @agent_orchestrator.rs:41842）
- `api::ui_protocol_transport::tests::should_expand_home_prefix_when_listing_workspace`（HOME=/root 触发 `workspace_list_root_escape` 守卫，"/root is rooted under the system path /root"）

**预存失败（2，git 考古锤死与 trim 无关，切片 2 diff 零触碰相关文件）**：
1. `commands::oup_client::tests::embedded_client_uses_real_oup_negotiation_and_rpc_errors` — 断言未知方法返回 -32004（#2265，2026-09-05 写入），但 #12（042848b5，2026-09-23）已把 dispatch 改为未知方法返回 -32601 且未同步该测试（`git show 042848b5 --stat | grep -c oup_client` = 0）。自 #12 起必失败。
2. `api::cron_panel::tests::toggle_via_service_adopts_external_writes_instead_of_erasing_them` — `CronService::toggle_job_reconciling`（cron_service.rs:375）注释宣称 "re-read cron.json into memory (adopting writes from other owners)"，但实现直接 `store.get_schedule`（local_cron_store.rs:130 只查内存从不读盘），reconcile 语义未实现/在 db396b8f（2026-09-15 "cron_service store-backed rewrite"）中退化 → 外部新增 job toggle 返回 NotFound。

> 修复建议（待新提案）：① oup 测试断言 -32004 → -32601 对齐 #12 契约；② toggle_job_reconciling 真正先 `load_store_or_quarantine` 重读磁盘再 toggle。两者均超出本提案范围，仅记录。

### 保留决策（向后兼容）

- `ui_protocol_transport.rs::is_registered_channel_name` 与 `octos-core/types.rs:626` SessionKey 验证名单**保留**已删通道名（dingtalk/slack/line/email/qq-bot/wechat）：历史会话键（如 `cloud--root:wechat:group-42`）必须能正确解析，已在代码注释标注。
- `profiles.rs` `validate_profile_id` 保留名单同理：新 profile ID 不得占用历史通道名。
- `otp.rs` 的 SMTP 逻辑属 dashboard OTP 邮件验证码功能，与 email 通道无关，保留。
- `profiles.rs:2552/2809/3473/3487` 等注释/测试中的 `slack`/`line` 字样均为保留名单示例，非通道支持声明。

### 改动清单（切片 2）

删除文件：`crates/octos-bus/src/{dingtalk,slack,line,email,qq_bot,wechat}_channel.rs`、`crates/octos-cli/src/commands/gateway/adapters/{dingtalk,email,line,qq_bot,slack,wechat}.rs`

修改文件：`crates/octos-bus/src/lib.rs`、`crates/octos-bus/Cargo.toml`、`crates/octos-cli/Cargo.toml`、`crates/octos-server/Cargo.toml`、`crates/octos-cli/src/commands/gateway/adapters/mod.rs`、`crates/octos-cli/src/profiles.rs`、`crates/octos-cli/src/commands/account.rs`、`crates/octos-cli/src/commands/gateway/account_handler.rs`、`crates/octos-cli/src/api/admin.rs`、`crates/octos-cli/src/api/auth_handlers.rs`、`crates/octos-cli/src/api/router.rs`、`crates/octos-cli/src/api/webhook_proxy.rs`、`crates/octos-cli/src/api/ui_protocol_transport.rs`（仅注释）、`crates/octos-cli/src/api/ui_protocol_tests.rs`（仅注释）、`crates/octos-cli/src/autonomy/escalation_notify.rs`、`crates/octos-cli/src/process_manager.rs`、`crates/octos-cli/src/commands/gateway/mod.rs`、`crates/octos-cli/src/commands/gateway/gateway_runtime.rs`、`crates/octos-cli/src/commands/channels.rs`、`crates/octos-cli/src/commands/gateway/prompt.rs`、`crates/octos-cli/src/config.rs`、`crates/octos-cli/src/session_actor.rs`（仅注释）、`crates/octos-agent/src/tools/message.rs`（工具描述示例 slack→discord）

Reporter：`logos/resources/verify/test-results.jsonl` TRIM-S2-*（6 条）；工作区 jsonl 曾被 S17 复跑覆盖致 TRIM-S1-* 丢失，已自 8e975c7e 恢复追加（append-only 语义保全 S17 最新结果）。
