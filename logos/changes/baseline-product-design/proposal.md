# 变更提案：baseline-product-design

> module: core | created: 2026-09-18

## 变更原因
Phase 1 需求文档基线（core-01-requirements.md，场景 S01–S15）已由 baseline-prd 变更合入，但 Phase 2 产品设计目录（`logos/resources/prd/2-product-design/`）为空。缺少功能规格与交互原型会导致：技术架构与场景建模没有交互层输入、命令/参数/输出格式无权威定义、后续变更的交互影响分析无基线。本变更基于 Phase 1 场景与仓库实际命令面补齐 Phase 2 产品设计基线。

## 变更类型
设计级（纯文档基线新增，无行为变更、无代码变更）

## 产品类型判断（product-designer Step 1）
octos 为**非 GUI 主导的混合型产品**：CLI 工具（主交互面）+ 纯 API 服务（octos serve）+ AI 对话式交互（agent 会话）。Web 仪表盘定位为观测/管理面（需求文档"不做"清单已排除协同 GUI），本变更不为其产出 HTML 原型。因此 ui-ux-pro-max 子流程**不触发**，原型形式按交付物分别选择：CLI 场景用终端交互模拟（`-terminal.md`）、对话式场景用对话脚本（`-dialogue.md`）、API 场景用调用示例（`-api-examples.md`）。

## 变更范围
- 影响的需求文档：无（输入为已合入的 core-01-requirements.md，不改动）
- 影响的功能规格：**新增** `logos/resources/prd/2-product-design/1-feature-specs/` 下 5 份设计文档（按场景分组：CLI 上手与对话、网关与通道、REST API 服务、编排与自动化、能力扩展与安全），含 CLI 命令树信息架构
- 影响的原型：**新增** `logos/resources/prd/2-product-design/2-page-design/` 下 5 份原型（终端模拟 ×3、对话脚本 ×1、API 示例 ×1）
- 影响的业务场景：S02–S15（细化交互规格与交互级验收条件；S01 已由 chatgpt-oauth-codex 落地，本变更不改动）
- 影响的部署方案：无
- 影响的 API：无（REST 路由的交互示例仅作文档引用，不改 API 定义）
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
以 Phase 1 场景（S02–S15）为骨架补齐产品设计：定义 CLI 命令树信息架构（29 个子命令分组与全局约定）；为每个场景细化交互规格（命令格式、参数表、输出格式、错误提示、退出码；通道消息行为；API 调用形态），并将 Phase 1 的 GIVEN/WHEN/THEN 细化到交互元素级别（具体参数值、输出片段、退出码）；按产品类型产出配套原型（终端交互模拟、对话脚本、API 调用示例）。本变更只产文档，不产生代码与测试任务。
