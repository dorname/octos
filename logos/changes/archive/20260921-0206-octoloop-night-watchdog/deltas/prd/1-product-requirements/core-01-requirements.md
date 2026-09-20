# Delta: prd/1-product-requirements — core-01-requirements.md

> target: logos/resources/prd/1-product-requirements/core-01-requirements.md

## MODIFIED — P03: 复杂任务靠人肉串联

### P03: 复杂任务靠人肉串联
因为多步任务（调研→写码→测试→报告）只能靠用户逐条 prompt 驱动，且内外环在 ACK、阻塞或迭代预算耗尽后缺少独立监督器继续按铃 → 导致无法并行、无法断点续跑、无法在关键步骤及时裁决 → 造成长任务可靠性差，尤其夜间容易静默停摆并依赖人工续推。

## MODIFIED — P10: LLM 调用单点脆弱

### P10: LLM 调用单点脆弱
因为单一提供商会遇到限流（429）与宕机（5xx），模型会话本身也可能在迭代预算、阻塞或进程退出后停止 → 导致无人值守任务在凌晨直接中断，且仅靠模型内自续无法证明监督链仍存活 → 造成自动化场景（cron、gateway、OctoLoop）不可用，用户被迫盯梢。

## MODIFIED — 三、场景总览

## 三、场景总览

| 编号 | 场景名称 | 触发条件 | 关联痛点 | 优先级 |
|------|---------|---------|---------|--------|
| S01 | ChatGPT 订阅登录与 Codex 后端对话（已落地，见 chatgpt-oauth-codex） | 用户持 ChatGPT 订阅执行 auth login | P01 | P0 |
| S02 | 开发者首次上手与认证 | 新用户安装 octos 后首次使用 | P01 | P0 |
| S03 | CLI 交互式多轮任务执行 | 用户在终端发起需要工具协作的任务 | P04, P05 | P0 |
| S04 | 团队 IM 通道接入与消息网关 | 团队希望在 IM 中直接使用 agent | P02, P07 | P0 |
| S05 | REST API 服务与流式集成 | 开发者/前端需要程序化调用 agent | P02 | P0 |
| S06 | 定时任务与无人值守自动化 | 用户需要周期性自动执行的任务 | P03, P10 | P1 |
| S07 | 流水线编排多步工作流 | 多步、可并行、需人工卡点的复杂任务 | P03 | P1 |
| S08 | 记忆沉淀与检索复用 | agent 完成任务后经验需要复用 | P06 | P1 |
| S09 | 技能插件安装与使用 | 用户需要主程序之外的长尾能力 | P08 | P1 |
| S10 | MCP 服务器接入与工具扩展 | 用户要接入已有 MCP 生态工具 | P08 | P1 |
| S11 | 子代理派生与并行协作 | 主 agent 需要并行推进多个子任务 | P03 | P2 |
| S12 | LLM 故障转移与自适应路由 | 提供商限流/宕机时保持可用 | P10 | P1 |
| S13 | ACP 协议接入 IDE | 用户在 Zed 等 IDE 中使用 agent | P02 | P2 |
| S14 | 多租户运维与管理面 | 管理员托管多个 profile/子账户 | P09 | P2 |
| S15 | 安全策略与沙箱配置管理 | 用户需要收紧/调整执行安全边界 | P04 | P2 |
| S16 | K8s 多副本无状态化部署与故障恢复 | 需要在 Kubernetes 上水平扩缩 serve/worker | P09, P11 | P0 |
| S17 | OctoLoop 夜间监督与有界续推 | 双环进入 ACK、阻塞、预算耗尽或空闲待办状态 | P03, P10 | P0 |

## MODIFIED — S06: 定时任务与无人值守自动化

### S06: 定时任务与无人值守自动化

- **触发条件**：用户需要周期性自动执行的任务（日报、巡检、汇总）
- **用户价值**：无人值守自动化，失败可见可恢复；普通 cron 失败按周期隔离，OctoLoop goal 的 `budget_limited` 则交由 S17 外环裁决与有界续推（← P03, P10）
- **优先级**：P1
- **主路径**：`octos cron` 添加定时任务 → 调度器到点触发 → agent 执行任务 → 结果投递到配置的会话/通道；若任务属于受监督 OctoLoop goal，则监督器只消费已配置的运行时事件并按 S17 处理，不改变 cron 的调度语义

#### 验收条件

##### 正常：定时任务全链路
- **GIVEN** gateway 常驻运行中
- **WHEN** 用户添加一个 cron 任务（如每日 9 点“汇总昨日 git 提交并发送到 Telegram”）
- **THEN** `octos cron` 列表可见该任务及下次触发时间；到点后 agent 自动执行，结果投递到目标通道，执行记录可查

##### 正常：受监督 goal 达到迭代预算
- **GIVEN** cron 触发的工作进入已启用 S17 监督的 OctoLoop goal，且 agent 达到无人值守迭代上限并写出 checkpoint
- **WHEN** goal 转为 `budget_limited`
- **THEN** 本次 cron 执行不伪装为成功；S17 监督器以幂等信号唤醒当前 `outer-duty` holder 裁决，保留 checkpoint，禁止 cron 自身无限重试

##### 异常：执行时提供商持续限流
- **GIVEN** cron 任务触发时 LLM 持续返回 429，重试与故障转移均耗尽
- **WHEN** 本次执行失败
- **THEN** 该次执行标记为失败并记录原因，不影响后续调度周期，进程保持运行；仅在显式配置 S17 监督且形成受支持事件时才触发外环门铃

## ADDED — S17: OctoLoop 夜间监督与有界续推

### S17: OctoLoop 夜间监督与有界续推

- **触发条件**：operator 为一个已铺设 OLP 黑板、存在内环与持锁外环的项目显式启用 Watchdog，双环随后出现新增 ACK、`blocked` / `escalation` / `budget_limited`、或内环 idle 且仍有未 ACK 条目
- **用户价值**：夜间无人值守时，系统持续落入“继续推进、唤醒外环裁决、或有界停止并告警”之一，避免静默停摆，同时不靠无限重试掩盖失败（← P03, P10）
- **优先级**：P0
- **主路径**：Watchdog 恢复持久游标 → 按项目 cwd 发现内外环 → 读取黑板新增域、运行时事件、goal/ledger 与 Git HEAD → 分类信号 → ACK/负信号只唤醒当前 `outer-duty` holder，空闲未 ACK 只唤醒内环 → 记录投递回执与进展指纹 → 外环裁决并回执内环 → 新进展清零重试计数

#### 活性保证

在 Watchdog 进程、所配置的事件源与 herdr 可用期间，每个新监督信号最终必须处于以下可审计终态之一：

1. 已接受门铃并观察到新的协议进展；
2. 已唤醒外环等待技术或 operator 裁决；
3. 连续 3 次自动续推仍无进展，熔断停止内环重试并产生明确告警。

该保证不承诺任务必然完成，不覆盖机器休眠、全部模型车道失效、operator-only 审批长期无人处理或外部依赖永久不可用。

#### 验收条件

##### 正常：新增 ACK 唤醒外环且只投递一次
- **GIVEN** Watchdog 已建立黑板基线游标，`outer-duty check` 返回 `HELD`，外环与内环 cwd 均精确匹配项目
- **WHEN** 内环在基线之后新增 `ACK(done|wontdo|blocked): ...`
- **THEN** Watchdog 仅向持锁外环注入一次带项目、条目与证据指针的门铃；重启或重复扫描不重放同一信号

##### 正常：预算耗尽转外环裁决
- **GIVEN** goal 写出 checkpoint 并转为 `budget_limited`
- **WHEN** Watchdog 消费对应新事件
- **THEN** 保留 checkpoint，不把状态改成成功，不直接替外环做裁决；只向当前 holder 投递一次门铃

##### 正常：空闲待办有界续推
- **GIVEN** 内环为 idle，黑板 Active 区存在未 ACK 条目，且距上次协议进展已超过配置的 idle 阈值
- **WHEN** Watchdog 计算进展指纹未变化且该任务无熔断
- **THEN** 向 cwd 匹配的内环注入一次“读取最小未 ACK 条目”的指针；检测到黑板、goal ledger 或 Git HEAD 任一有效进展后计数清零

##### 异常：连续三次无进展
- **GIVEN** 同一任务已被自动续推 2 次且进展指纹均未变化
- **WHEN** 第 3 次续推后再次超过观察窗仍无变化
- **THEN** Watchdog 将任务置为 fused，停止继续唤醒内环，尝试唤醒持锁外环并输出 operator 可见告警；不得进行第 4 次自动续推

##### 异常：外环锁不可信
- **GIVEN** `outer-duty check` 返回 `VACANT`、`ERROR`、unsupported 或 holder 元数据无法与项目匹配
- **WHEN** 出现需外环处理的信号
- **THEN** 保留信号为 pending 并告警，不向任意外环注入、不自动 acquire/hold、不通过 TTL 推断接管权

##### 安全：授权边界不扩张
- **GIVEN** Watchdog 被启用且 agent 通用 auto mode 开启
- **WHEN** 工作流到达 OpenLogos merge、verify、部署、smoke、archive、push 或 `gate:implement:loop-exhausted`
- **THEN** Watchdog 只能发送状态指针，不能执行或伪造授权；仅 `openlogos next --auto` 的既有 standing 授权生效，且 loop-exhausted 仍硬阻塞

## MODIFIED — 5.1 技术约束

### 5.1 技术约束

- **语言与工具链**：Rust edition 2024，rust-version 1.85.0；纯 Rust TLS（rustls），无 OpenSSL 依赖；`deny(unsafe_code)` 工作区级 lint
- **跨平台**：Linux / macOS / Windows 三平台核心行为一致（shell、进程管理、二进制发现的平台差异已抽象）；沙箱后端能力依赖宿主机（bwrap/Landlock 仅 Linux，sandbox-exec 仅 macOS，AppContainer 仅 Windows，Docker 跨平台）；S17 常驻部署第一阶段仅保证 Linux systemd user service，其他平台只提供前台 `run` / `run-once` 并显式说明非守护保证
- **外部凭据**：LLM 提供商 key/OAuth、各 IM 通道 bot token 依赖第三方平台配额与可用性
- **运行模型**：本地/单机模式数据目录为 `~/.octos`（config.json / auth.json / sessions / episodes.redb），多实例/多租户靠 profile 隔离；**集群模式**以 PostgreSQL 为业务状态唯一真相源（会话/事件/审批/租约/检查点/cron），本机文件降级为 local adapter 或迁移来源（见 ADR cluster-state-and-execution）；S17 Watchdog 只持有可重建的本地监督游标、去重与熔断状态，不成为黑板、goal ledger、Git 或 outer-duty 的真相源
- **feature 构建约束**：`serve`(api)、`browser`、`git` 工具、`code_structure`(ast)、email 通道、`embed-llama` 等为 feature-gated，默认安装包含 embed-llama（需 cmake + C++ 工具链）
