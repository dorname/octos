# 合并指令

## 变更提案
- 提案名称：baseline-architecture
- 提案目录：logos/changes/baseline-architecture/

## 提案内容

# 变更提案：baseline-architecture

> module: core | created: 2026-09-18

## 变更原因
Phase 1 需求基线（S01–S15）与 Phase 2 产品设计基线已合入，但 Phase 3 技术架构目录只有一份 `verified: false` 的逆向种子结构图（core-system-map.md），缺少权威的架构概要（系统边界、技术选型、非功能约束）与场景时序图。架构图是时序图参与方一致性的前提，时序图是后续 API/测试设计的来源。本变更基于仓库实际代码结构（23 个平台 crate 分层、三种运行时、36+20 工具、17 通道）补齐 Phase 3 Step 0 架构概要与 Step 1 场景时序图基线；图按层级拆分为总览图 + 子系统子图，避免单图过载。

## 变更类型
设计级（纯文档基线新增，无行为变更、无代码变更）

## 变更范围
- 影响的需求文档：无
- 影响的功能规格：无
- 影响的技术架构：
  - **新增** `logos/resources/prd/3-technical-plan/1-architecture/core-01-architecture-overview.md`（系统上下文图、分层架构图、三种运行时部署视图、agent loop / 工具与沙箱 / 消息总线 / LLM 路由 / 记忆 / 插件与 MCP 子图、系统处理流程图、技术选型表、非功能约束、外部依赖与测试策略）
  - **新增** `logos/resources/prd/3-technical-plan/2-scenario-implementation/core-00-scenario-overview.md`（场景地图与索引）
  - **新增** 场景时序图 `core-S02` ~ `core-S15` 各一份（P0/P1 场景含完整步骤说明与 EX 异常用例；P2 场景为主路径时序图）
- 影响的业务场景：S02–S15（技术视角展开；S01 已落地，仅在概览中索引）
- 影响的部署方案：无（完整部署方案由后续 deployment-designer 承接）
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：无
- 影响的 smoke 测试：无

## 部署影响
- 是否需要部署：否
- 部署原因：纯文档基线，不改动任何运行时代码与配置
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述
补齐 Phase 3 技术架构基线：以逆向种子结构图为原料、以代码为准核验，产出权威架构概要——含系统上下文、L0–L5 分层架构、三种运行时（chat/gateway/serve）处理流程总图与各子系统（agent loop、工具执行与沙箱、消息总线与通道、LLM 提供商路由、记忆、插件与 MCP）分层子图、技术选型表（每项带理由与备选）、非功能约束（性能/安全/可观测）、外部依赖与测试策略；随后将 S02–S15 场景展开为 Mermaid 时序图（参与方与架构图组件一致、箭头 Step N 编号、步骤叙事 + EX 异常用例），并生成场景概览索引。同步更新 logos-project.yaml 的 tech_stack / scenarios / external_dependencies / resource_index。本变更只产文档，不产生代码与测试任务。


## 需要合并的 Delta 文件

### 1. deltas/prd/3-technical-plan/1-architecture/core-01-architecture-overview.md

- Delta 文件：`logos/changes/baseline-architecture/deltas/prd/3-technical-plan/1-architecture/core-01-architecture-overview.md`
- 目标目录：`logos/resources/prd/3-technical-plan/1-architecture/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 2. deltas/prd/3-technical-plan/2-scenario-implementation/core-00-scenario-overview.md

- Delta 文件：`logos/changes/baseline-architecture/deltas/prd/3-technical-plan/2-scenario-implementation/core-00-scenario-overview.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 3. deltas/prd/3-technical-plan/2-scenario-implementation/core-S02-first-run.md

- Delta 文件：`logos/changes/baseline-architecture/deltas/prd/3-technical-plan/2-scenario-implementation/core-S02-first-run.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 4. deltas/prd/3-technical-plan/2-scenario-implementation/core-S03-cli-chat.md

- Delta 文件：`logos/changes/baseline-architecture/deltas/prd/3-technical-plan/2-scenario-implementation/core-S03-cli-chat.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 5. deltas/prd/3-technical-plan/2-scenario-implementation/core-S04-gateway-channel.md

- Delta 文件：`logos/changes/baseline-architecture/deltas/prd/3-technical-plan/2-scenario-implementation/core-S04-gateway-channel.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 6. deltas/prd/3-technical-plan/2-scenario-implementation/core-S05-serve-api.md

- Delta 文件：`logos/changes/baseline-architecture/deltas/prd/3-technical-plan/2-scenario-implementation/core-S05-serve-api.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 7. deltas/prd/3-technical-plan/2-scenario-implementation/core-S06-cron-automation.md

- Delta 文件：`logos/changes/baseline-architecture/deltas/prd/3-technical-plan/2-scenario-implementation/core-S06-cron-automation.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 8. deltas/prd/3-technical-plan/2-scenario-implementation/core-S07-pipeline.md

- Delta 文件：`logos/changes/baseline-architecture/deltas/prd/3-technical-plan/2-scenario-implementation/core-S07-pipeline.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 9. deltas/prd/3-technical-plan/2-scenario-implementation/core-S08-memory.md

- Delta 文件：`logos/changes/baseline-architecture/deltas/prd/3-technical-plan/2-scenario-implementation/core-S08-memory.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 10. deltas/prd/3-technical-plan/2-scenario-implementation/core-S09-skills.md

- Delta 文件：`logos/changes/baseline-architecture/deltas/prd/3-technical-plan/2-scenario-implementation/core-S09-skills.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 11. deltas/prd/3-technical-plan/2-scenario-implementation/core-S10-mcp.md

- Delta 文件：`logos/changes/baseline-architecture/deltas/prd/3-technical-plan/2-scenario-implementation/core-S10-mcp.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 12. deltas/prd/3-technical-plan/2-scenario-implementation/core-S11-sub-agent.md

- Delta 文件：`logos/changes/baseline-architecture/deltas/prd/3-technical-plan/2-scenario-implementation/core-S11-sub-agent.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 13. deltas/prd/3-technical-plan/2-scenario-implementation/core-S12-llm-failover.md

- Delta 文件：`logos/changes/baseline-architecture/deltas/prd/3-technical-plan/2-scenario-implementation/core-S12-llm-failover.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 14. deltas/prd/3-technical-plan/2-scenario-implementation/core-S13-acp-ide.md

- Delta 文件：`logos/changes/baseline-architecture/deltas/prd/3-technical-plan/2-scenario-implementation/core-S13-acp-ide.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 15. deltas/prd/3-technical-plan/2-scenario-implementation/core-S14-admin-ops.md

- Delta 文件：`logos/changes/baseline-architecture/deltas/prd/3-technical-plan/2-scenario-implementation/core-S14-admin-ops.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 16. deltas/prd/3-technical-plan/2-scenario-implementation/core-S15-security-config.md

- Delta 文件：`logos/changes/baseline-architecture/deltas/prd/3-technical-plan/2-scenario-implementation/core-S15-security-config.md`
- 目标目录：`logos/resources/prd/3-technical-plan/2-scenario-implementation/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

## 执行要求

1. 逐个 Delta 文件处理，每处理完一个报告修改摘要
2. 对于 ADDED 标记：在主文档的指定位置插入新内容
3. 对于 MODIFIED 标记：替换主文档中同名章节的内容
4. 对于 REMOVED 标记：从主文档中删除对应章节
5. 保持主文档的原有格式和风格
6. 如果主文档有"最后更新"时间戳，同步更新
7. 所有变更完成后，列出修改清单
8. 所有变更合并完成后，自动执行 git commit（告知用户，无需确认）：
   git add -A && git commit -m "docs(baseline-architecture): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive baseline-architecture`。
