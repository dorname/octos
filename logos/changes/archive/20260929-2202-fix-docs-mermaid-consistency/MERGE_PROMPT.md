# 合并指令

## 变更提案
- 提案名称：fix-docs-mermaid-consistency
- 提案目录：logos/changes/fix-docs-mermaid-consistency/

## 提案内容

# 变更提案：fix-docs-mermaid-consistency

> module: core | created: 2026-09-29

## 变更原因

用户反馈需求/设计/实现文档中的 mermaid 图「存在很多错位的情况」，经确认为两类问题并存：

1. **渲染错位/失败**：全量语法校验（mermaid v10 + v11 双版本，jsdom 解析全部 35 个 live 块）发现 `core-S17-octoloop-night-watchdog.md` 时序图第 37 行消息文本含 ASCII 分号 `;`，mermaid 将其解释为语句分隔符导致**整图解析失败**（渲染为语法错误框）。该写法是唯一语法级错误，但同类隐患（消息文本中的特殊字符）需一并排查。
2. **图文不符（内容错位）**：文档基线数字冻结于 2026-09-18 接入时点，代码已演进，多处声明与现实不符：
   - 「IM 平台 ×17」／「17 个 IM 平台」——实际 `*_channel.rs` 共 17 个通道实现，其中 **IM 通道 15 个** + api/cli 本地通道 2 个；「IM 平台 ×17」系误标（core-03-gateway-channels-design.md 的 17 通道清单含 api/cli，本身自洽）。
   - 「clap 命令 ×29」——实际顶层子命令 **30 个**（`pub enum Command` 变体计数，2026-09-29 核验）。
   - 「23 组 / 157 路由」——实际 axum `route(` 注册点 **226 处**、唯一路径 **76 条**（2026-09-29 核验）；原口径不可复现。
   - CLAUDE.md 顶部架构描述「14 channels」「91 REST endpoints」与上述口径均不一致，需同步对齐。

## 变更类型

设计级（仅文档修正，无代码变更；修正文档使其与既有实现一致，不改变任何已定义行为）

## 变更范围

- 影响的需求文档：`logos/resources/prd/1-product-requirements/core-01-requirements.md`（基线数字声明）
- 影响的功能规格：`logos/resources/prd/2-product-design/1-feature-specs/core-03-gateway-channels-design.md`（核对 17 通道清单措辞，仅在不自洽时微调）
- 影响的业务场景：`core-S17-octoloop-night-watchdog.md`（时序图语法修复）；其余 S02–S16 场景图经双版本语法校验全部通过，仅做措辞级核查
- 影响的技术架构：`logos/resources/prd/3-technical-plan/1-architecture/core-01-architecture-overview.md`（一/二/三/八节数字与图标签）、`core-system-map.md`（17 通道口径注明）
- 影响的部署方案：无
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：无
- 影响的 smoke 测试：无

## 部署影响

- 是否需要部署：否
- 部署原因：纯文档修正，不涉及任何运行时代码、配置或部署产物
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述

本次变更只做两件事，全部落在文档层：

1. **mermaid 语法修复与加固**：修复 S17 时序图的 ASCII 分号解析错误（改为中文分号）；对全部 35 个 live mermaid 块建立可复跑的语法校验脚本（`scripts/check-mermaid.mjs`，mermaid v10+v11 双版本），作为后续文档修改的防回归手段。对可移植性存疑的语法（flowchart 双向箭头 `<-->`、多目标边 `A --> B & C`）评估后按需改写为等价的基础语法，降低旧版渲染器（Typora/旧 VS Code 插件）错位风险。
2. **数字口径对齐现实**：以 2026-09-29 核验值修正「IM 平台 ×17 → IM 通道 ×15（通道实现共 17，含 api/cli）」「clap 命令 ×29 → ×30」「23 组 / 157 路由 → 按核验口径改写」，并在需求基线声明处标注核验日期；CLAUDE.md 顶部架构描述的 channels/endpoints 数字同步对齐。

## 复用测试 ID

- 无 —— 本变更为纯文档修正，不涉及业务代码与测试代码；防回归手段为新增脚本 `scripts/check-mermaid.mjs`（文档语法校验，非 UT/ST 用例）


## 需要合并的 Delta 文件

### 1. deltas/prd/1-product-requirements/core-01-requirements.md

- Delta 文件：`logos/changes/fix-docs-mermaid-consistency/deltas/prd/1-product-requirements/core-01-requirements.md`
- 目标目录：`logos/resources/prd/1-product-requirements/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 2. deltas/prd/3-technical-plan/1-architecture/core-01-architecture-overview.md

- Delta 文件：`logos/changes/fix-docs-mermaid-consistency/deltas/prd/3-technical-plan/1-architecture/core-01-architecture-overview.md`
- 目标目录：`logos/resources/prd/3-technical-plan/1-architecture/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 3. deltas/prd/3-technical-plan/1-architecture/core-system-map.md

- Delta 文件：`logos/changes/fix-docs-mermaid-consistency/deltas/prd/3-technical-plan/1-architecture/core-system-map.md`
- 目标目录：`logos/resources/prd/3-technical-plan/1-architecture/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

### 4. deltas/prd/3-technical-plan/2-scenario-implementation/core-S17-octoloop-night-watchdog.md

- Delta 文件：`logos/changes/fix-docs-mermaid-consistency/deltas/prd/3-technical-plan/2-scenario-implementation/core-S17-octoloop-night-watchdog.md`
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
   git add -A && git commit -m "docs(fix-docs-mermaid-consistency): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive fix-docs-mermaid-consistency`。
