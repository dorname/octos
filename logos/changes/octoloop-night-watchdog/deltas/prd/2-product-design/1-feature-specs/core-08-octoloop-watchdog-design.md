# Delta: prd/2-product-design/1-feature-specs — core-08-octoloop-watchdog-design.md

> target: logos/resources/prd/2-product-design/1-feature-specs/core-08-octoloop-watchdog-design.md(全新文档)

## ADDED — core-08 OctoLoop 夜间监督 — 功能规格

# core-08 OctoLoop 夜间监督 — 功能规格

> 覆盖场景：S17（OctoLoop 夜间监督与有界续推）
> 需求来源：core-01-requirements.md §四 S17
> 接口来源：core-S17-octoloop-night-watchdog.md 的主时序与异常路径
> 协议来源：OLP 的黑板、ACK、outer-duty 与 herdr 门铃契约

## 一、产品边界

Watchdog 是独立于内环、外环模型会话的 Rust 常驻监督进程。它只做发现、分类、去重、门铃与告警，不执行复验、不代写 ACK、不自动获取 outer-duty，也不替代内环的 turn-continuation 或 maintenance loop。

权威事实源保持分离：

| 事实 | 唯一事实源 | Watchdog 本地状态的角色 |
|------|------------|--------------------------|
| 评审任务与 ACK | `<project>/.octos/OUTER_LOOP_REVIEW.md` | 只保存读取游标与已投递 signal id |
| goal 状态与交付 | goal status / goal ledger / runtime events | 只保存最近观察到的版本与指纹 |
| 代码进展 | 项目 Git HEAD | 只保存最近 hash |
| 外环主审权 | `octoscode outer-duty check --project <project>` | 不缓存为授权，不 acquire/hold |
| agent 位置与状态 | `herdr agent list` 的当前结果 | 每次投递前重新发现，不长期绑定 pane |

## 二、CLI 契约

接口由 S17 时序 Step 1、6、9、12、15 推导：

```text
octos watchdog run --project <PATH> [--config <FILE>]
octos watchdog run-once --project <PATH> [--config <FILE>] [--json]
octos watchdog status --project <PATH> [--config <FILE>] [--json]
```

| 命令 | 语义 | 退出条件 |
|------|------|----------|
| `run` | 前台常驻，持续执行“采样→分类→处置→持久化” | SIGINT/SIGTERM 优雅退出；致命配置错误非 0 |
| `run-once` | 恢复状态，完成一个确定性巡检周期后退出 | 无信号也为 0；源损坏、配置非法或处置失败为非 0 并输出结构化原因 |
| `status` | 只读展示 service/state、源健康、cursor、pending、retry/fuse 与最近投递 | 不唤醒任何 agent，不改变游标 |

第一阶段不提供 `start`/`stop` 子命令；常驻生命周期由 systemd user service 管理，避免 CLI 自建 pidfile 与 systemd 双重所有权。其他平台可直接使用 `run`，但不得宣称崩溃自动拉起。

## 三、配置契约

默认配置发现路径为 `$XDG_CONFIG_HOME/octos/watchdog/<project-id>.toml`；`project-id` 由 canonical project path 的稳定 SHA-256 摘要生成，避免同名仓库碰撞。`--config` 可覆盖发现路径。

```toml
version = 1
project = "/absolute/canonical/path"
poll_interval_secs = 5
idle_after_secs = 120
observation_window_secs = 120
max_no_progress_retries = 3

[sources]
board = ".octos/OUTER_LOOP_REVIEW.md"
events = "/absolute/discovered/path/events.jsonl"

[agents]
inner_kind = "octoscode"
outer_kind = "claude"

[alerts]
stderr = true
file = "~/.local/state/octos/watchdog/<project-id>/alerts.jsonl"
```

约束：

- `project` 必须 canonicalize 成绝对目录；相对 source 路径仅相对该目录解析。
- events 路径可以由 operator 显式给出，或由发现器在 `~/.octos/instances` 中按项目 cwd 证明唯一匹配；0 个或多个候选都 fail closed。
- `max_no_progress_retries` 第一版固定允许 1–3，默认 3；不得配置为无限。
- 配置不接受任意 shell command、prompt 模板命令替换或自动 acquire 参数。
- 首次启动以文件当前 EOF/当前事件 highwater 建立 baseline；除非显式 `--replay-from-start` 的未来受控接口，本变更不重放历史信号。

## 四、信号分类与处置

| signal kind | 证据 | 目标 | 处置 |
|-------------|------|------|------|
| `board_ack` | 基线后新增且符合 v1 语法的 ACK 行 | 外环 | 先 check HELD，再 prompt holder |
| `goal_blocked` | 新 `goal_transition` 到 blocked | 外环 | 带 goal id、事件 offset 与状态指针 |
| `escalation` | 新 escalation 事件 | 外环 | 按 R3 提示裁决，不替 operator 批准 |
| `budget_limited` | 新 goal_transition 到 budget_limited | 外环 | 指向 checkpoint/ledger，禁止直接恢复为成功 |
| `inner_idle_pending` | 内环 idle + Active 最小编号条目未 ACK + idle 超阈值 | 内环 | 指向条目编号；进入观察窗 |
| `watchdog_fault` | 源、herdr、outer-duty 或状态持久化异常 | 外环或 operator 告警 | 不推进 cursor 到“已处理”状态 |

`signal_id` 必须由 `project-id + source identity + monotonic position + normalized kind + object id` 稳定生成。门铃文本只包含项目、信号类型、唯一指针、期望动作与授权提醒；业务指令仍留在黑板/ledger，禁止复制成第二份真相。

## 五、发现与投递规则

1. 每次投递前运行 `herdr agent list`，只接受 `cwd` 或 `foreground_cwd` canonicalize 后与项目完全相等的候选。
2. 同一角色出现多个候选时不猜测；标记 ambiguous 并告警。
3. 外环投递前必须重新运行 `octoscode outer-duty check --project <project>`；stdout 第一态为 `HELD` 且 holder 证据可读才允许向匹配外环 prompt。
4. `VACANT`、`ERROR`、unsupported、超时或 holder/agent 无法唯一对应时，信号保持 pending；不按 terminal title、TTL 或最近活动时间推断 authority。
5. `herdr agent prompt <pane> <text>` 返回 accepted 才写 delivered；`agent_blocked`、`agent_prompt_stalled`、timeout 与进程失败均写 attempt 但不写 delivered。
6. 内环 prompt 只在 agent_status 为 idle/done 时发送；working 时保留 pending，blocked 时转外环告警。

## 六、进展指纹与有界熔断

进展指纹为以下稳定字段的组合 hash：

```text
board_highwater + active_item_ack_state
+ goal_id/status/ledger_highwater
+ git_head
```

- 任一权威源出现与当前任务相关的新高水位即视为进展并把 retry_count 清零。
- 单纯 agent 状态从 working 回 idle、日志时间戳变化、重复相同 ACK 或 Watchdog 自身写 state 不算进展。
- `inner_idle_pending` 投递后进入 observation window；窗口结束仍无进展才把 retry_count 加一。
- retry_count 达 3 后写 `fused=true`，停止内环 prompt，生成高优先级告警，并在 holder 可用时唤醒外环。
- 熔断只在检测到真实进展或 operator 执行未来显式 reset 操作时解除；进程重启不能清零。

## 七、状态与崩溃一致性

状态默认写入 `$XDG_STATE_HOME/octos/watchdog/<project-id>/state.json`：

```json
{
  "version": 1,
  "project": "/canonical/project",
  "board_cursor": {"file_id": "...", "offset": 0},
  "event_cursor": {"file_id": "...", "offset": 0},
  "pending": [],
  "delivered_signal_ids": [],
  "tasks": {},
  "last_progress_fingerprint": "...",
  "updated_at": "RFC3339"
}
```

状态以同目录临时文件写入、flush 后 atomic rename；先记录 pending，再投递，再记录 delivered。崩溃发生在投递与 delivered 持久化之间时，恢复后可再次提示，但 prompt 必须带同一 `signal_id`，接收侧按 id 去重；不得为了“恰好一次”而丢失未确认信号。

检测到 source truncate/rotate 时以 file identity 区分：旧文件已消费 offset 保留审计，新文件从 0 读取，但通过 signal_id 与 delivered 集合避免重放；无法判断时停在 fault，不盲目跳到 EOF。

## 八、状态输出与告警

`status --json` 至少返回：

```json
{
  "service": "active|inactive|unknown",
  "project": "/canonical/project",
  "sources": {"board": "healthy", "events": "healthy"},
  "outer_duty": "HELD|VACANT|ERROR|unsupported",
  "pending_count": 0,
  "fused_tasks": [],
  "last_signal_at": null,
  "last_progress_at": null
}
```

告警同时写 stderr/journal 与可选 append-only JSONL。每条含 severity、project、signal_id、reason、attempt、next_action，不写凭据、完整 prompt 或无关终端内容。

## 九、禁用与回滚交互

operator 通过 `systemctl --user disable --now octos-watchdog@<instance>.service` 停用。停用后保留 state 供审计；显式删除 state 仅使下次启动重新建立 baseline，不删除黑板、goal ledger、checkpoint、Git commit 或 herdr 会话。默认安装不自动 enable。
