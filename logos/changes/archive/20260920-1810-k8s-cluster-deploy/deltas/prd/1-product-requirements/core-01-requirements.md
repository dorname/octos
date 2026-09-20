# Delta: prd/1-product-requirements — core-01-requirements.md

> target: logos/resources/prd/1-product-requirements/core-01-requirements.md

## MODIFIED — 二、用户痛点分析

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

### P11: 多副本部署状态无法共享
因为会话、审批、cron、检查点等业务状态绑在单机 JSONL/redb 与进程内缓存 → 导致把 `octos serve` 放进多副本 Deployment 后出现会话并发写冲突、审批无法跨 Pod 恢复、后台任务失联 → 造成无法水平扩缩与滚动升级，生产可用性卡在单实例。

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

## ADDED — S16: K8s 多副本无状态化部署与故障恢复

### S16: K8s 多副本无状态化部署与故障恢复

- **触发条件**：运维/平台需要在 Kubernetes 上以多副本方式运行 `octos serve`（及逻辑 Worker/Scheduler），并在 Pod 滚动或故障后保持会话、审批、cron 与执行连续性
- **用户价值**：业务状态以 PostgreSQL 为唯一真相源，Pod 可任意销毁重建；审批与任务可跨副本恢复；支持滚动升级与水平扩缩（← P09, P11）
- **优先级**：P0
- **主路径**：选择部署形态（baseline / hostpath / cluster）→ 应用 `deploy/k8s/*.yaml` 并注入 ConfigMap/Secret → 运行 PG 迁移 → 多副本启动 → 客户端经 Service 访问 → 杀 Pod / 滚动后按 Scope 从 PG 回放事件与恢复租约 → 会话与审批不丢

#### 验收条件

##### 正常：cluster 形态多副本可服务
- **GIVEN** 本地 k8s（如 docker-desktop）可用，镜像与配置已按 `deploy/docs/K8S_INSTALL.md` 准备
- **WHEN** 执行 `./deploy/scripts/deploy-k8s.sh cluster` 且副本数 ≥ 2
- **THEN** Service 可访问仪表盘与 UI Protocol；`GET /api/version` 返回 200；PG 中存在迁移后的业务表

##### 正常：Pod 销毁后会话可续
- **GIVEN** 集群模式运行中，客户端已在某会话产生事件（PG `session_events` 有单调 seq）
- **WHEN** 删除处理该连接的 Pod，客户端重连并按 seq 回放
- **THEN** 无缺口或显式 resync；已确认业务状态不丢；新 Pod 可接管 Scope 租约

##### 异常：未配置 PG 却启用集群模式
- **GIVEN** 配置声明集群/PG 后端但 `DATABASE_URL` 缺失或不可达
- **WHEN** 启动 serve 集群角色
- **THEN** 启动失败并给出可操作错误（缺连接串/迁移未应用），不以 local JSONL 静默顶替集群真相源

## MODIFIED — 5.1 技术约束

### 5.1 技术约束

- **语言与工具链**：Rust edition 2024，rust-version 1.85.0；纯 Rust TLS（rustls），无 OpenSSL 依赖；`deny(unsafe_code)` 工作区级 lint
- **跨平台**：Linux / macOS / Windows 三平台行为一致（shell、进程管理、二进制发现的平台差异已抽象）；沙箱后端能力依赖宿主机（bwrap/Landlock 仅 Linux，sandbox-exec 仅 macOS，AppContainer 仅 Windows，Docker 跨平台）
- **外部凭据**：LLM 提供商 key/OAuth、各 IM 通道 bot token 依赖第三方平台配额与可用性
- **运行模型**：本地/单机模式数据目录为 `~/.octos`（config.json / auth.json / sessions / episodes.redb），多实例/多租户靠 profile 隔离；**集群模式**以 PostgreSQL 为业务状态唯一真相源（会话/事件/审批/租约/检查点/cron），本机文件降级为 local adapter 或迁移来源（见 ADR cluster-state-and-execution）
- **feature 构建约束**：`serve`(api)、`browser`、`git` 工具、`code_structure`(ast)、email 通道、`embed-llama` 等为 feature-gated，默认安装包含 embed-llama（需 cmake + C++ 工具链）

