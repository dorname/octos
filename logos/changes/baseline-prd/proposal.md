# 变更提案：baseline-prd

> module: core | created: 2026-09-18

## 变更原因
octos 已进入 launched 生命周期，但 Phase 1 需求文档（`logos/resources/prd/1-product-requirements/`）为空——目前只有逆向种子基线（core-system-map.md / core-scenario-candidates.md，均 `verified: false`）和一个已由变更落地的场景 S01。缺少权威需求文档会导致：产品设计、架构设计、场景建模无输入源；场景编号体系无锚点；后续变更的影响分析无基线可对。本变更基于仓库现状（29 个 CLI 命令、157 条 REST 路由、17 个消息通道、36 个内置工具、pipeline/memory/sandbox 等子系统）补齐 Phase 1 需求文档基线。

## 变更类型
需求级（纯文档基线新增，无行为变更、无代码变更）

## 变更范围
- 影响的需求文档：**新增** `logos/resources/prd/1-product-requirements/core-01-requirements.md`（产品定位、用户痛点 P01–P10、场景总览 S01–S15、P0/P1 场景验收条件、约束与不做清单）
- 影响的功能规格：无（由后续 baseline-product-design 变更承接）
- 影响的业务场景：声明 S02–S15 场景编号与验收条件；S01（ChatGPT 订阅登录与 Codex 后端对话）已由 chatgpt-oauth-codex 变更落地，本变更仅在场景总览中收录引用，不改动其测试规格
- 影响的部署方案：无
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
基于仓库现状逆向梳理并正向定义 octos 的产品需求基线：明确"Rust 原生、API 优先的多租户 Agentic OS"的产品定位与三类目标用户画像；提炼 10 条带因果链的用户痛点（P01–P10）；以业务目标为导向定义 14 个新场景（S02–S15，编号接续已占用的 S01），覆盖首次上手、CLI 交互、消息网关、REST 服务、定时任务、流水线编排、记忆、技能、MCP、子代理、故障转移、ACP、多租户运维、安全策略；为全部 P0/P1 场景编写 GIVEN/WHEN/THEN 验收条件（每个 ≥1 正常 + ≥1 异常）；明确技术约束与"不做"清单。本变更只产文档，不产生代码与测试任务。
