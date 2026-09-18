# 变更提案：fix-archived-spec-refs

> module: core | created: 2026-09-18

## 变更原因
migrate-agent-spec 将 `specs/` 归档至 `logos/changes/archive/agent-spec-legacy/` 后,源码注释与文档中仍残留 3 处指向旧路径的引用,已失效需修正。

## 变更类型
设计级(纯文档/注释修正,不涉及代码,不改变运行时行为)

## 变更范围
- 影响的需求文档:无
- 影响的功能规格:无
- 影响的业务场景:无
- 影响的 API:无
- 影响的 DB 表:无
- 影响的编排测试:无
- 修正的引用:`crates/octos-agent/src/compaction.rs`(文档注释)、`crates/octos-cli/tests/serve_broken_pipe.rs`(模块注释)、`docs/OCTOS_UI_PROTOCOL_CHANGE_REQUEST_UPCR_2026_030_INCOMPLETE_TURN_RESULTS.md`
- 不修正:`crates/octos-agent/src/tools/send_app_card.rs` 引用的是外部 robrix2 仓库的 specs 路径,与本仓库归档无关

## 部署影响
- 是否需要部署:否
- 部署原因:仅注释/文档路径修正
- 影响环境:无
- 是否涉及数据迁移:否
- 是否需要回滚预案:否
- 是否需要 smoke:否

## UI/UX 变更声明

```yaml
ui_impact: false
design_system_mode: generated
design_system_fallback_reason: ""
pages: []
```

## 变更概述
将 3 处 `specs/...` 引用更新为 `logos/changes/archive/agent-spec-legacy/...` 归档路径,均为单行注释/文档改动。
