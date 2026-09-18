# Delta: prd/1-product-requirements — core-01-requirements.md

> target: logos/resources/prd/1-product-requirements/core-01-requirements.md(全新文档)

## ADDED — octos 需求文档

# octos 需求文档

> 最后更新：2026-09-18
> 基线说明：本文档基于仓库现状（29 个 CLI 命令、157 条 REST 路由、17 个消息通道、36+20 个工具、pipeline/memory/sandbox/plugin 子系统）正向定义产品需求基线。场景编号接续已占用的 S01（chatgpt-oauth-codex 变更落地）。

## 一、产品背景与目标

### 1.1 产品定位

octos 是一个 **Rust 原生、API 优先的多租户智能体操作系统（Agentic OS）**：单一二进制即可在本地、服务器或容器中运行 AI 智能体，通过 CLI、REST API 和 17 个消息通道（Telegram / Discord / Slack / 飞书 / 企业微信 / WhatsApp / Email 等）触达用户，内置工具系统、五后端沙箱、混合检索记忆、DOT 图流水线编排与二进制协议插件生态。

一句话定位：**为开发者和团队提供"跑在任何地方、接入任何通道、可安全放权"的 AI 智能体运行平台。**

与单一形态的 AI 助手（纯 CLI / 纯 Web UI）不同，octos 的核心差异是：

1. **三种运行时同一内核**：`octos chat`（交互式 CLI）、`octos gateway`（多通道常驻网关）、`octos serve`（REST API + Web 仪表盘，157 条路由）共享同一个 agent loop、工具系统与记忆体系。
2. **安全放权是一等公民**：五个沙箱后端按平台自动决策，显式模式不可用时 fail-closed 拒绝而非静默裸奔；工具策略、SSRF 防护、环境变量消毒贯穿所有执行面。
3. **可编排的长任务**：DOT 图流水线支持并行 fan-out、checkpoint 断点续跑、human gate 人工卡点，定时任务与子代理让无人值守自动化成为默认能力。

### 1.2 核心目标

| 目标 | 衡量指标 |
|------|---------|
| 上手零摩擦 | 新用户从安装到首次对话 ≤ 4 条命令（init → doctor → auth login → chat） |
| 安全放权 | agent 执行的 shell/文件操作 100% 经过 SafePolicy 与沙箱决策；显式沙箱不可用时 0 次静默降级 |
| 通道接入低成本 | 新增一个 IM 通道只需实现通道适配层，消息分片/会话隔离/投递由 bus 统一承担 |
| 长任务可靠 | 流水线崩溃后可从 checkpoint 恢复，已完成节点零重复执行 |
| 经验可沉淀 | 任务摘要自动写入 EpisodeStore，相关记忆在后续会话自动注入上下文 |

### 1.3 目标用户画像

**画像 A：独立开发者 / AI 重度用户（主要）**
- 特征：每天与 LLM 协作编程，持有多个厂商的 API key 或 ChatGPT/Claude 订阅，在本地终端工作。
- 诉求：一个趁手的本地 agent，能操作文件、跑命令、查网页，key 和订阅凭证统一管理，长对话不丢失上下文。

**画像 B：小团队技术负责人**
- 特征：5–30 人团队，日常协作在飞书/企业微信/Slack/Telegram 等 IM 中，希望把 CI 摘要、日报汇总、告警处理等交给 agent 自动完成。
- 诉求：把 agent 接进团队 IM，定时任务无人值守跑，复杂多步任务能编排、能人工卡点、崩了能续跑。

**画像 C：平台运维 / 管理员**
- 特征：为团队或客户托管多个 agent 实例（多租户 profile），关心资源占用、故障与安全边界。
- 诉求：统一的管理面（REST 管理路由 + admin 工具 + doctor 诊断），可观测、可审计、故障可自愈。

## 二、用户痛点分析

### P01: LLM 凭证管理割裂
因为各厂商认证方式不同（API key / OAuth PKCE / device code / 订阅 token），且订阅 token 与 API key 的可用端点不同（如 ChatGPT 订阅 token 只对 Codex 后端有效）→ 导致用户混用凭证后频繁 403、配置散落多处 → 造成上手成本高、配错后排查耗时，甚至误以为产品不可用。

### P02: AI 助手被困在单一终端
因为多数 AI 助手只能在专用终端或 Web UI 中使用 → 导致团队协作场景（IM 群里的问题、告警群的处理）无法触达 agent → 造成 agent 成果与真实工作流断裂，价值局限在个人把玩。

### P03: 复杂任务靠人肉串联
因为多步任务（调研→写码→测试→报告）只能靠用户逐条 prompt 驱动 → 导致无法并行、无法断点续跑、无法在关键步骤人工确认 → 造成长任务可靠性差，中途失败前功尽弃。

### P04: 不敢放权 agent 自动执行
因为 agent 能执行任意 shell 命令与文件写操作 → 导致一次幻觉调用（如 `rm -rf`、读敏感文件、外发数据）就可能造成真实损失 → 造成用户只敢让 agent "只读建议"，自动化价值无法释放。

### P05: 长会话质量退化
因为多轮对话与工具结果不断累积 → 导致上下文窗口被填满后关键早期信息被丢弃、token 成本飙升 → 造成长任务后半段 agent "失忆"，输出质量断崖式下降。

### P06: 会话经验不沉淀
因为每次会话结束即消逝 → 导致同类问题要重复教 agent（项目约定、用户偏好、历史决策）→ 造成知识无法复用，agent 永远"第一天上班"。

### P07: 多通道接入各自为政
因为每个 IM 平台的消息格式、长度限制、会话模型都不同 → 导致每接一个通道都要重写长消息分片、会话隔离、错误重试 → 造成接入成本高、行为不一致。

### P08: agent 能力扩展依赖改代码
因为新工具/新能力需要改主程序代码并重新发布 → 导致能力上线慢、第三方无法参与 → 造成生态封闭，长尾需求（查天气、发邮件、智能家居）无人满足。

### P09: 多实例运维缺统一控制面
因为多个 agent 实例（多 profile/多租户）各自运行 → 导致状态不可见、日志分散、配置漂移 → 造成故障排查靠猜，管理操作靠 SSH 登机器。

### P10: LLM 调用单点脆弱
因为单一提供商会遇到限流（429）与宕机（5xx）→ 导致无人值守任务在凌晨直接中断 → 造成自动化场景（cron、gateway）不可用，用户被迫盯梢。

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

## 四、核心场景详述

### S02: 开发者首次上手与认证

- **触发条件**：新用户安装 octos 二进制后首次使用
- **用户价值**：4 条命令内完成从安装到首次对话，凭证一次配置长期可用（← P01）
- **优先级**：P0
- **主路径**：`octos init` 初始化配置（选择提供商与模型）→ `octos doctor` 诊断环境 → `octos auth login` 完成 OAuth/设备码/粘贴 token 认证 → `octos chat` 发起首次对话

#### 验收条件

##### 正常：全新环境完整上手
- **GIVEN** 一台已安装 octos 二进制、无 `~/.octos` 目录的机器
- **WHEN** 用户依次执行 `octos init`（按引导选择提供商与默认模型）、`octos doctor`、`octos auth login`（设备码流程，在浏览器完成授权）、`octos chat` 并发送一条消息
- **THEN** init 生成 `~/.octos/config.json`；doctor 逐项输出检查结果并对缺失项给出修复建议；`octos auth status` 显示对应提供商已登录；chat 返回模型回复

##### 正常：多种认证方式并存
- **GIVEN** 用户持有 OpenAI OAuth 凭证与 Anthropic API key
- **WHEN** 用户用 `octos auth login` 完成 OpenAI 设备码登录，并通过环境变量提供 Anthropic key
- **THEN** `octos auth status` 显示 OAuth 凭证来自 auth store，Anthropic 走 env var；auth store 中的凭证优先于同名 env var 被使用

##### 异常：未认证即发起对话
- **GIVEN** 用户未完成任何认证（auth store 无凭证且未设置任何 API key 环境变量）
- **WHEN** 用户在 `octos chat` 中发送消息
- **THEN** 系统明确提示缺少 API 凭证并给出修复路径（`octos auth login` 或设置对应环境变量），不发出会产生费用的模型请求

##### 异常：设备码授权未完成
- **GIVEN** 用户启动设备码登录但未在浏览器完成授权（拒绝或码过期）
- **WHEN** 轮询直至超时/过期
- **THEN** CLI 提示授权未完成或已过期，不写入任何凭证到 `~/.octos/auth.json`，退出码非 0

### S03: CLI 交互式多轮任务执行

- **触发条件**：用户在终端向 agent 发起需要工具协作的任务
- **用户价值**：agent 自主调用工具完成任务，且执行安全可控、长会话不失忆（← P04, P05）
- **优先级**：P0
- **主路径**：用户描述任务 → agent loop 构建消息（系统提示词 + 历史 + 记忆）→ LLM 返回工具调用 → 工具经策略过滤与沙箱决策后执行 → 结果回注 → 循环直至任务完成

#### 验收条件

##### 正常：工具协作完成文件任务
- **GIVEN** 用户已认证，在一个可写的工作区目录中启动 `octos chat`
- **WHEN** 用户输入"创建一个 hello.py 打印当前时间并运行它"
- **THEN** agent 依次调用文件写入工具与 shell 工具；shell 命令经 SafePolicy 检查与沙箱执行；hello.py 落盘、运行输出回显在会话中，agent 给出完成总结

##### 正常：长会话自动压缩
- **GIVEN** 会话上下文的 token 估算接近预算上限
- **WHEN** 用户继续多轮对话
- **THEN** compaction 自动触发：剥离历史工具参数、摘要早期内容、保留最近工具调用/结果对；会话不中断、不报错

##### 异常：显式沙箱模式不可用
- **GIVEN** 用户配置了显式沙箱模式（如 `mode = "docker"`）但当前主机无 Docker
- **WHEN** agent 尝试执行 shell 命令
- **THEN** 该命令被 fail-closed 拒绝（`SandboxUnavailable` / RefusingSandbox），并输出针对当前 OS 的修复指引；绝不静默降级为无沙箱执行

##### 异常：危险命令被策略拒绝
- **GIVEN** 任意沙箱配置
- **WHEN** agent 试图执行 SafePolicy 命中的危险命令（如 `rm -rf /`、`dd` 写盘、fork 炸弹）
- **THEN** SafePolicy 在空白字符归一化匹配后拒绝执行，返回拒绝原因，命令不进入沙箱

### S04: 团队 IM 通道接入与消息网关

- **触发条件**：团队希望在既有 IM 中直接 @ 或私聊 agent
- **用户价值**：agent 进入团队真实工作流，多平台接入行为一致（← P02, P07）
- **优先级**：P0
- **主路径**：在 config 中配置通道凭据 → `octos gateway` 常驻运行 → 通道接收消息 → 解析/创建会话 → agent 处理 → 回复按通道限制自动分片发回

#### 验收条件

##### 正常：Telegram 端到端问答
- **GIVEN** config.json 中已配置有效的 Telegram bot token，gateway 已启动
- **WHEN** 群成员在 Telegram 中向 bot 发送一个问题
- **THEN** 网关为该 chat 解析/创建会话，agent 在会话上下文中处理，回复按 Telegram 长度限制自动分片发回（段落 > 换行 > 句子 > 空格 > 硬切的降级顺序），同一会话内多轮上下文连续

##### 正常：多会话隔离
- **GIVEN** 两个不同 chat_id 的用户同时与 bot 对话
- **WHEN** 网关并发处理两条消息
- **THEN** 两个会话各自独立持久化（JSONL 文件 + LRU 缓存），历史互不串扰

##### 异常：单通道凭据失效
- **GIVEN** gateway 同时运行 Telegram 与 Discord 两个通道，其中 Discord token 已失效
- **WHEN** Discord 通道轮询/连接返回 401
- **THEN** 该通道报错并退避重试，Telegram 通道处理不受影响，进程不退出

##### 异常：超长回复分片上限
- **GIVEN** agent 产出了一条远超通道单条上限的回复
- **WHEN** coalescing 分片发送
- **THEN** 分片数不超过 MAX_CHUNKS（50）的 DoS 上限，超出部分被截断，分片边界 UTF-8 安全（不出现乱码半字符）

### S05: REST API 服务与流式集成

- **触发条件**：开发者或前端应用需要程序化调用 agent 能力
- **用户价值**：以标准 HTTP/WebSocket 集成 agent，仪表盘开箱即用（← P02）
- **优先级**：P0
- **主路径**：`octos serve` 启动（默认绑定 127.0.0.1）→ 客户端携带凭证调用对话接口（WebSocket UI 协议或流式路由）→ 请求进入 agent/session → 流式响应逐段返回 → 浏览器访问 dashboard 管理

#### 验收条件

##### 正常：流式对话
- **GIVEN** `octos serve` 已启动且凭证有效
- **WHEN** 客户端调用流式对话接口
- **THEN** 响应以流式方式逐段返回模型输出，结束后完整消息落入会话历史，客户端可断连重试

##### 正常：仪表盘可访问
- **GIVEN** `octos serve` 已启动
- **WHEN** 浏览器访问服务地址
- **THEN** 返回 Web 仪表盘页面，可查看会话/状态等界面

##### 异常：默认仅本机绑定
- **GIVEN** 用户未提供 `--host` 参数
- **WHEN** 从另一台机器访问 serve 端口
- **THEN** 连接被拒绝（仅监听 127.0.0.1），不发生对外暴露

##### 异常：管理面未授权访问
- **GIVEN** serve 已启动
- **WHEN** 客户端无有效凭证调用管理面路由（如 /api/admin/*）
- **THEN** 返回 401/403，响应体不泄露内部配置与状态细节

### S06: 定时任务与无人值守自动化

- **触发条件**：用户需要周期性自动执行的任务（日报、巡检、汇总）
- **用户价值**：无人值守自动化，失败可见可恢复（← P03, P10）
- **优先级**：P1
- **主路径**：`octos cron` 添加定时任务 → 调度器到点触发 → agent 执行任务 → 结果投递到配置的会话/通道

#### 验收条件

##### 正常：定时任务全链路
- **GIVEN** gateway 常驻运行中
- **WHEN** 用户添加一个 cron 任务（如每日 9 点"汇总昨日 git 提交并发送到 Telegram"）
- **THEN** `octos cron` 列表可见该任务及下次触发时间；到点后 agent 自动执行，结果投递到目标通道，执行记录可查

##### 异常：执行时提供商持续限流
- **GIVEN** cron 任务触发时 LLM 持续返回 429，重试与故障转移均耗尽
- **WHEN** 本次执行失败
- **THEN** 该次执行标记为失败并记录原因，不影响后续调度周期，进程保持运行

### S07: 流水线编排多步工作流

- **触发条件**：多步、可并行、需要人工卡点的复杂任务
- **用户价值**：复杂任务结构化执行，可并行、可续跑、可卡点（← P03）
- **优先级**：P1
- **主路径**：编写 DOT 图流水线定义 → 触发执行 → 节点按依赖调度（fan-out 并行 N worker）→ checkpoint 逐节点落盘 → human gate 暂停等待确认 → 完成输出 PipelineResult

#### 验收条件

##### 正常：含并行与人工卡点的流水线
- **GIVEN** 一个包含并行 fan-out 节点与 human gate 的 DOT 流水线定义
- **WHEN** 用户触发执行
- **THEN** 并行节点按配置的 worker 数并发执行；每个节点完成后写 checkpoint；到达 human gate 时暂停并等待人工确认；确认后续跑至完成，输出 PipelineResult（含总 token 用量、逐节点摘要与修改文件清单）

##### 正常：节点级模型选择
- **GIVEN** 流水线通过 ModelStylesheet 为不同节点指定不同模型
- **WHEN** 执行经过这些节点
- **THEN** 各节点使用各自指定的模型调用 LLM

##### 异常：崩溃后断点续跑
- **GIVEN** 流水线执行到中途进程崩溃（已有若干节点完成并落 checkpoint）
- **WHEN** 用户重新触发同一流水线
- **THEN** 从最近 checkpoint 恢复，已完成节点不重复执行、不重复扣费

### S08: 记忆沉淀与检索复用

- **触发条件**：agent 完成任务后经验需要在后续会话复用
- **用户价值**：agent 越用越懂项目与用户（← P06）
- **优先级**：P1
- **主路径**：任务完成 → 摘要写入 EpisodeStore（redb）→ 后续会话构建系统提示词时 HybridSearch（向量 + BM25）检索相关记忆注入

#### 验收条件

##### 正常：经验沉淀与注入
- **GIVEN** `save_episodes` 开启，agent 刚完成一个任务
- **WHEN** 任务结束，且用户随后开启相关话题的新会话
- **THEN** 任务摘要已写入 EpisodeStore（`.octos/episodes.redb`）；新会话系统提示词中包含经混合检索（默认向量 0.7 / BM25 0.3 权重）排序的相关记忆

##### 正常：长期记忆窗口
- **GIVEN** MEMORY.md 与每日笔记中存在 7 天内的记录
- **WHEN** 新会话构建系统提示词
- **THEN** 7 天窗口内的近期记忆被纳入上下文

##### 异常：无 embedding 提供商
- **GIVEN** 未配置任何 embedding provider
- **WHEN** 触发记忆检索
- **THEN** 自动降级为 BM25-only 排序并正常返回结果，不报错、不中断会话

### S09: 技能插件安装与使用

- **触发条件**：用户需要主程序之外的长尾能力（天气、邮件、深度搜索等）
- **用户价值**：能力即装即用，生态可持续生长（← P08）
- **优先级**：P1
- **主路径**：`octos skills list/install` 安装技能 → manifest 门控检查（binary/env/OS）→ agent 调用技能工具 → 二进制协议执行（JSON stdin/stdout）→ 结果返回

#### 验收条件

##### 正常：安装并调用技能
- **GIVEN** 技能源可用
- **WHEN** 用户执行 `octos skills install weather` 并在 chat 中询问天气
- **THEN** 技能 manifest 门控通过（二进制存在、env 齐备、OS 匹配）；agent 调用该技能工具，插件以 `./binary <tool_name>` 二进制协议执行（JSON 输入/输出），结果回注会话

##### 正常：spawn_only 后台执行
- **GIVEN** 已安装的技能中含 `spawn_only: true` 的工具，且其 SKILL.md 已自动注入系统提示词
- **WHEN** agent 调用该工具
- **THEN** 执行自动转入后台任务并立即返回任务句柄，agent 无需特殊配合即可继续会话

##### 异常：门控不满足
- **GIVEN** 某技能 manifest 声明的必需环境变量未设置
- **WHEN** 用户查看 `octos skills list` 或 agent 尝试调用该技能
- **THEN** list 中该技能标记为不可用并显示缺失项；调用被拒绝并说明原因，不影响其他技能

### S10: MCP 服务器接入与工具扩展

- **触发条件**：用户要接入已有 MCP 生态中的工具服务器
- **用户价值**：复用 MCP 生态，工具边界安全可控（← P08）
- **优先级**：P1
- **主路径**：config 声明 stdio MCP server → agent 启动时 JSON-RPC 握手 → 工具 schema 校验注册 → LLM 调用 MCP 工具

#### 验收条件

##### 正常：接入并调用 MCP 工具
- **GIVEN** 用户在 config 中声明了一个可用的 stdio MCP server
- **WHEN** agent 启动并收到匹配该工具的用户请求
- **THEN** octos 通过 JSON-RPC stdio 完成握手；schema 合法（深度 ≤ 10、大小 ≤ 64KB）的工具注册进 ToolRegistry；LLM 调用后结果正常回注

##### 异常：schema 超限拒绝注册
- **GIVEN** MCP server 暴露了一个 schema 深度或大小超限的工具
- **WHEN** 工具注册阶段
- **THEN** 该工具被拒绝注册并记录原因，同一 server 的其余合法工具不受影响

##### 异常：MCP server 进程崩溃
- **GIVEN** MCP server 进程在会话中途崩溃
- **WHEN** agent 调用其工具
- **THEN** 返回工具级错误信息给 agent 继续处理，主进程与会话不崩溃

### S12: LLM 故障转移与自适应路由

- **触发条件**：提供商限流、宕机或质量退化时保持服务可用
- **用户价值**：无人值守场景下 LLM 调用韧性（← P10）
- **优先级**：P1
- **主路径**：请求经 AdaptiveRouter（熔断/打分）→ ProviderChain（主备链）→ RetryProvider（指数退避）→ 实际提供商；失败逐层兜底

#### 验收条件

##### 正常：主提供商限流自动切换
- **GIVEN** 配置了主备两个提供商
- **WHEN** 主提供商持续返回 429，RetryProvider 指数退避重试后仍失败
- **THEN** ProviderChain 自动切换到备用提供商完成请求，用户侧无感知失败

##### 正常：自适应路由熔断
- **GIVEN** AdaptiveRouter 启用且某 lane 连续失败达到熔断阈值
- **WHEN** 新请求到达
- **THEN** 熔断期内请求不再打到该 lane，优先选择打分健康的 lane；恢复后自动半开试探

##### 异常：全部提供商不可用
- **GIVEN** 所有配置的提供商均不可用
- **WHEN** 请求发出并穷尽重试与转移
- **THEN** 返回明确的链路失败错误（包含各跳失败原因），不静默返回空内容或伪造成功

### S11: 子代理派生与并行协作（P2）

- **触发条件**：主 agent 需要并行推进多个独立子任务
- **用户价值**：复杂任务并行加速，子任务上下文隔离（← P03）
- **优先级**：P2
- **主路径**：agent 调用 spawn/spawn_agent 派生子代理 → send_input 下发任务 → wait_agent 等待结果 → close_agent 回收；Codex 兼容的 delegate 包装同路径

### S13: ACP 协议接入 IDE（P2）

- **触发条件**：用户在 Zed 等支持 ACP 的 IDE 中使用 agent
- **用户价值**：agent 进入编辑器工作流（← P02）
- **优先级**：P2
- **主路径**：IDE 以 stdio 启动 `octos acp` → ACP 协议握手 → 编辑器内发起会话 → agent 在 IDE 上下文执行工具 → 结果回显编辑器

### S14: 多租户运维与管理面（P2）

- **触发条件**：管理员托管多个 profile / 子账户的 agent 实例
- **用户价值**：统一控制面，可观测、可审计（← P09）
- **优先级**：P2
- **主路径**：管理员通过 /api/admin/* 路由或 admin 工具集（list_profiles / start_profile / view_logs / system_health / provider_metrics 等 20 个）完成实例生命周期管理与观测；`octos doctor` 本地诊断

### S15: 安全策略与沙箱配置管理（P2）

- **触发条件**：用户需要按环境收紧或调整执行安全边界
- **用户价值**：安全边界可按需配置且行为可预期（← P04）
- **优先级**：P2
- **主路径**：配置工具策略（allow/deny、通配、group 组、byProvider 覆盖）→ 配置沙箱（mode auto/bwrap/landlock/macos/appcontainer/docker、fail_closed）→ `octos doctor` 验证决策结果 → 显式模式不可用时收到 fail-closed 拒绝与修复指引

## 五、约束与边界

### 5.1 技术约束

- **语言与工具链**：Rust edition 2024，rust-version 1.85.0；纯 Rust TLS（rustls），无 OpenSSL 依赖；`deny(unsafe_code)` 工作区级 lint
- **跨平台**：Linux / macOS / Windows 三平台行为一致（shell、进程管理、二进制发现的平台差异已抽象）；沙箱后端能力依赖宿主机（bwrap/Landlock 仅 Linux，sandbox-exec 仅 macOS，AppContainer 仅 Windows，Docker 跨平台）
- **外部凭据**：LLM 提供商 key/OAuth、各 IM 通道 bot token 依赖第三方平台配额与可用性
- **单机模型**：数据目录为 `~/.octos`（config.json / auth.json / sessions / episodes.redb），无中心数据库；多实例/多租户靠 profile 隔离
- **feature 构建约束**：`serve`(api)、`browser`、`git` 工具、`code_structure`(ast)、email 通道、`embed-llama` 等为 feature-gated，默认安装包含 embed-llama（需 cmake + C++ 工具链）

### 5.2 资源与时间约束

- 本基线为存量产品的需求逆向定义，不涉及新功能交付排期
- 文档维护遵循 Delta 变更工作流：任何需求变更先创建 `logos/changes/` 提案

### 5.3 "不做"清单

- **不做自有 IM 平台**：只接入既有消息通道，不自建聊天网络
- **不做 LLM 自研与模型托管**：只做提供商接入与路由（本地推理仅通过 OpenAI 兼容的 local 家族/embed-llama embedder）
- **不做原生移动 App**：移动端通过 IM 通道与 Web 仪表盘触达
- **不做多人实时协同编辑类 GUI**：仪表盘定位为观测与管理，不做协作文档
- **不做计费系统**：仅提供成本观测（cost 路由），不做订阅计费与扣费
- **不做中心化管理 SaaS**：octos 以单二进制/自托管为形态，不提供官方云托管控制面
