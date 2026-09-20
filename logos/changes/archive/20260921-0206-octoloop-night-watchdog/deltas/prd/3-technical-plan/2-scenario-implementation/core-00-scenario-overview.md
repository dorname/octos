# Delta: prd/3-technical-plan/2-scenario-implementation — core-00-scenario-overview.md

> target: logos/resources/prd/3-technical-plan/2-scenario-implementation/core-00-scenario-overview.md

## MODIFIED — 业务场景概览（技术实现）

# 业务场景概览（技术实现）

> 最后更新：2026-09-20
> 场景编号全局唯一：S01 由 chatgpt-oauth-codex 落地；S02–S15 由 baseline-prd 分配；S16 为 K8s 集群场景；S17 为 OctoLoop 夜间监督与有界续推。合并 S17 时 `scenario_counter.next_id` 从 17 前移到 18。
> 说明：core 模块 skip_phases = [api, database, scenario]（单二进制 CLI/本地存储形态，无独立 REST API 设计与 API 编排阶段），故下表对应列标记为“跳过”；S17 的 CLI/本地接口直接由场景时序推导。

## 场景地图

| 编号 | 场景名称 | Phase 1 | Phase 2 | Phase 3 时序图 | API | 编排 | 状态 |
|------|---------|---------|---------|--------------|-----|------|------|
| S01 | ChatGPT 订阅登录与 Codex 后端对话 | ✅（变更落地） | —（代码级变更） | —（见变更测试规格） | 跳过 | 跳过 | 已落地 |
| S02 | 开发者首次上手与认证 | ✅ | ✅ | ✅ | 跳过 | 跳过 | 文档基线完成 |
| S03 | CLI 交互式多轮任务执行 | ✅ | ✅ | ✅ | 跳过 | 跳过 | 文档基线完成 |
| S04 | 团队 IM 通道接入与消息网关 | ✅ | ✅ | ✅ | 跳过 | 跳过 | 文档基线完成 |
| S05 | REST API 服务与流式集成 | ✅ | ✅ | ✅ | 跳过 | 跳过 | 文档基线完成 |
| S06 | 定时任务与无人值守自动化 | ✅ | ✅ | ✅ | 跳过 | 跳过 | 文档基线完成 |
| S07 | 流水线编排多步工作流 | ✅ | ✅ | ✅ | 跳过 | 跳过 | 文档基线完成 |
| S08 | 记忆沉淀与检索复用 | ✅ | ✅ | ✅ | 跳过 | 跳过 | 文档基线完成 |
| S09 | 技能插件安装与使用 | ✅ | ✅ | ✅ | 跳过 | 跳过 | 文档基线完成 |
| S10 | MCP 服务器接入与工具扩展 | ✅ | ✅ | ✅ | 跳过 | 跳过 | 文档基线完成 |
| S11 | 子代理派生与并行协作 | ✅ | ✅ | ✅（主路径） | 跳过 | 跳过 | 文档基线完成 |
| S12 | LLM 故障转移与自适应路由 | ✅ | ✅ | ✅ | 跳过 | 跳过 | 文档基线完成 |
| S13 | ACP 协议接入 IDE | ✅ | ✅ | ✅（主路径） | 跳过 | 跳过 | 文档基线完成 |
| S14 | 多租户运维与管理面 | ✅ | ✅ | ✅（主路径） | 跳过 | 跳过 | 文档基线完成 |
| S15 | 安全策略与沙箱配置管理 | ✅ | ✅ | ✅（主路径） | 跳过 | 跳过 | 文档基线完成 |
| S16 | K8s 多副本无状态化部署与故障恢复 | ✅ | ✅ | ✅ | 跳过 | 跳过 | 文档基线进行中 |
| S17 | OctoLoop 夜间监督与有界续推 | ✅ | ✅ | ✅ | 跳过 | 跳过 | delta 待合并 |

## 场景依赖关系

```text
S02 上手与认证（一切前提：凭证与配置）
  ├── S03 CLI 对话（依赖凭证；S15 安全配置影响其执行边界）
  ├── S04 消息网关（依赖凭证；复用 S03 同一 agent loop；S06 cron 注入其会话）
  ├── S05 REST 服务（依赖凭证；复用同一 agent loop；S14 管理面运行其上）
  ├── S13 ACP（依赖凭证与 S03 同内核）
  └── S12 故障转移（横切：S03–S07 的 LLM 调用都经过提供商栈）
S07 流水线（依赖 S03 的会话与工具面；可经 S04 通道做 human gate）
S08 记忆（横切：S03/S04 的会话沉淀经验，后续会话复用）
S09 技能 / S10 MCP（横切：扩展 S03/S04/S05 的工具面）
S11 子代理（依赖 S03 agent loop；为 S07 之外的另一种并行形态）
S16 k8s 无状态化（横切：S05 集群部署；PG 真相源；影响 S06 cron / 审批跨副本）
S17 OctoLoop Watchdog（依赖 OLP 黑板、goal/events、herdr 与 outer-duty；补强 S06 无人值守活性，但不替代 cron/agent loop）
```

## 场景索引

| 编号 | 需求（Phase 1） | 设计（Phase 2） | 时序图（Phase 3） |
|------|----------------|----------------|------------------|
| S01 | core-01-requirements.md §三 | （chatgpt-oauth-codex 变更） | logos/resources/test/core-S01-test-cases.md |
| S02 | core-01-requirements.md §四 S02 | core-02-cli-onboarding-design.md §二 | core-S02-first-run.md |
| S03 | core-01-requirements.md §四 S03 | core-02-cli-onboarding-design.md §三 | core-S03-cli-chat.md |
| S04 | core-01-requirements.md §四 S04 | core-03-gateway-channels-design.md §一 | core-S04-gateway-channel.md |
| S05 | core-01-requirements.md §四 S05 | core-04-serve-api-design.md §一 | core-S05-serve-api.md |
| S06 | core-01-requirements.md §四 S06 | core-03-gateway-channels-design.md §二 | core-S06-cron-automation.md |
| S07 | core-01-requirements.md §四 S07 | core-05-orchestration-design.md §一 | core-S07-pipeline.md |
| S08 | core-01-requirements.md §四 S08 | core-06-capability-design.md §一 | core-S08-memory.md |
| S09 | core-01-requirements.md §四 S09 | core-06-capability-design.md §二 | core-S09-skills.md |
| S10 | core-01-requirements.md §四 S10 | core-06-capability-design.md §三 | core-S10-mcp.md |
| S11 | core-01-requirements.md §四 S11 | core-05-orchestration-design.md §二 | core-S11-sub-agent.md |
| S12 | core-01-requirements.md §四 S12 | core-06-capability-design.md §四 | core-S12-llm-failover.md |
| S13 | core-01-requirements.md §四 S13 | core-04-serve-api-design.md §二 | core-S13-acp-ide.md |
| S14 | core-01-requirements.md §四 S14 | core-03-gateway-channels-design.md §三 | core-S14-admin-ops.md |
| S15 | core-01-requirements.md §四 S15 | core-02-cli-onboarding-design.md §四 | core-S15-security-config.md |
| S16 | core-01-requirements.md §四 S16 | core-07-k8s-cluster-design.md | core-S16-k8s-cluster.md |
| S17 | core-01-requirements.md §四 S17 | core-08-octoloop-watchdog-design.md | core-S17-octoloop-night-watchdog.md |
