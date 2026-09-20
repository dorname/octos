# Delta: prd/2-product-design/1-feature-specs — core-03-gateway-channels-design.md

> target: logos/resources/prd/2-product-design/1-feature-specs/core-03-gateway-channels-design.md

## MODIFIED — 二、S06: 定时任务与无人值守自动化 — 交互规格

## 二、S06: 定时任务与无人值守自动化 — 交互规格

### 2.1 `octos cron`

**命令格式**：
- `octos cron add --name <NAME> --message <TEXT> [--every <SECONDS> | --cron "<EXPR>" | --at <TIME>] [--deliver --channel <CH> --to <TARGET>]`
- `octos cron list`
- `octos cron remove --name <NAME>`

**参数设计**：

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| --name | string | 是 | 任务名（唯一标识） |
| --message | string | 是 | 触发时注入 agent 的消息 |
| --every | int | 三选一 | 间隔秒数 |
| --cron | string | 三选一 | cron 表达式（可带时区） |
| --at | string | 三选一 | 一次性触发时间 |
| --deliver | flag | 否 | 执行结果投递到指定通道 |
| --channel | string | 配合 deliver | 目标通道 |
| --to | string | 配合 deliver | 目标会话/联系人 |

**交互流程**：
1. 用户 `octos cron add` 创建任务（持久化到 cron 存储，损坏文件隔离而非静默丢弃）
2. gateway 运行中，调度器到点将任务消息注入对应会话（sender 标记为 cron/heartbeat 系统来源）
3. agent 执行任务；`--deliver` 时结果经通道分片投递
4. `octos cron list` 显示任务、调度与下次触发时间；执行记录可查

#### 验收条件（交互级）

##### 正常：创建并列出一个周期任务
- **GIVEN** gateway 运行中
- **WHEN** 用户运行 `octos cron add --name daily-summary --message "汇总昨日 git 提交" --cron "0 9 * * *" --deliver --channel telegram --to 12345`，随后运行 `octos cron list`
- **THEN** add 输出创建成功；list 输出包含 daily-summary、cron 表达式、下次触发时间与投递目标；退出码 0

##### 正常：无人值守执行与投递
- **GIVEN** daily-summary 已创建且到达触发时间
- **WHEN** 调度器触发
- **THEN** agent 在独立会话上下文执行任务（无人值守迭代上限 UNATTENDED_MAX_ITERATIONS_FALLBACK=50 兜底），结果投递到 telegram:12345；执行记录含成功/失败状态

##### 异常：调度参数缺失
- **GIVEN** 用户运行 `octos cron add --name t1 --message "hi"`（未给 --every/--cron/--at）
- **WHEN** 命令解析校验
- **THEN** CLI 报错提示必须三选一提供调度参数，退出码非 0，不创建任务

### 2.2 与 S17 Watchdog 的边界

Watchdog 不是第二个 cron 调度器，也不修改 cron 的 next-run 或成功状态。只有项目显式启用 S17 且 cron 工作进入受监督的 OctoLoop goal 时，Watchdog 才把 `budget_limited`、`blocked` 或 escalation 作为外环门铃输入。

| 情况 | cron 责任 | Watchdog 责任 |
|------|-----------|----------------|
| 普通任务成功/失败 | 记录本次结果并计算下次触发 | 不介入 |
| 提供商重试耗尽 | 标记失败，可投递失败摘要 | 仅在出现受支持事件时告警，不替 cron 重跑 |
| goal `budget_limited` | 不伪装成功、不无限重试 | 唤醒当前 outer-duty holder 裁决 |
| Watchdog 未启用或停机 | cron 行为保持原样 | 无守护保证，状态命令明确显示 inactive |

#### 验收条件（协同级）

##### 正常：cron 与 Watchdog 不重复调度
- **GIVEN** 同一受监督任务已经由 cron 触发，goal 转为 `budget_limited`
- **WHEN** Watchdog 消费该事件
- **THEN** 只产生一个外环门铃；cron 的下一触发时间不变；Watchdog 不创建新的 cron execution

##### 异常：Watchdog 不可用
- **GIVEN** Watchdog 服务未运行
- **WHEN** 普通 cron 到点
- **THEN** cron 仍按既有语义执行并记录结果；系统不得声称具备 S17 夜间活性保证
