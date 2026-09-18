# 变更提案：migrate-agent-spec

> module: core | created: 2026-09-18

## 变更原因
项目原有的工作流基于 agent-spec(`specs/*.spec.md` 任务合约 + agent-spec lint/guard/verify 工具链)。现决定将研发工作流整体迁移到 OpenLogos 方法论：统一使用 `openlogos` CLI、`logos/` 目录结构、变更提案(Delta)驱动的迭代方式,以及 Claude Code 钩子(会话阶段提示 + 写操作守卫)。

## 变更类型
设计级(纯文档/工作流迁移,不涉及代码,不改变运行时行为)

## 变更范围
- 影响的需求文档:`docs/requirements/REQ-SERVE-BP-001.md`(更新 agent-spec 合约的归档路径)
- 影响的功能规格:`specs/` 下 19 份历史 agent-spec 任务合约 → 归档至 `logos/changes/archive/agent-spec-legacy/`
- 影响的业务场景:无(运行时行为不变)
- 影响的 API:无
- 影响的 DB 表:无
- 影响的编排测试:无
- 新增基础设施:`logos/`(config/project yaml/skills/spec/resources/changes)、`AGENTS.md`、`CLAUDE.md` 追加 OPENLOGOS 段、`.claude/` 下 openlogos 命令/agent/钩子

## 部署影响
- 是否需要部署:否
- 部署原因:仅仓库工作流与文档结构变更,不涉及发布产物
- 影响环境:无
- 是否涉及数据迁移:否(仅 git 内文件移动,历史保留)
- 是否需要回滚预案:否(回滚 = revert 本次提交)
- 是否需要 smoke:否

## UI/UX 变更声明

```yaml
ui_impact: false            # 本次是否触及界面(GUI 项目才有意义)
design_system_mode: generated   # generated | fallback(fallback 时须填 design_system_fallback_reason)
design_system_fallback_reason: ""
pages: []                   # 每项 {id, prototype: core-NN-<slug>.html, description}
```

## 变更概述
通过 `openlogos adopt --locale zh --ai-tool claude-code` 将仓库接入 OpenLogos(bootstrap: adopted,lifecycle: launched),生成 `logos/` 标准结构、AI 指令文件(AGENTS.md / CLAUDE.md OPENLOGOS 段)与 Claude Code 插件(10 个命令 + change-reviewer agent + SessionStart/PreToolUse 钩子)。

随后将 agent-spec 时代的 19 份历史任务合约从 `specs/` 迁移归档到 `logos/changes/archive/agent-spec-legacy/`(附 README 说明出处),并同步修正 `docs/requirements/REQ-SERVE-BP-001.md`、`docs/ARC_AGENT_TASK_MCP.md` 中的引用路径。后续所有迭代一律走 OpenLogos 变更提案流程(`openlogos change` → deltas → `openlogos merge`),agent-spec 工具链不再使用。
