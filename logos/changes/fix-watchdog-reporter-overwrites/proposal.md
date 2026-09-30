# 变更提案：修复 watchdog 测试 reporter 清空全量账本问题

> module: core | created: 2026-09-30

## 变更原因

`octos-cli/src/commands/watchdog/tests.rs` 中的 OpenLogos reporter 在初始化阶段调用 `fs::write(&path, b"")` 无条件清空 `logos/resources/verify/test-results.jsonl`。该文件是多场景共享的测试结果账本；每次运行 `cargo test -p octos-cli` 都会把其他场景（S01、S16 等）的历史 reporter 记录抹掉，只保留 S17 自身条目。这导致 `openlogos verify` 在 octos-cli 测试执行后出现 Uncovered=105 的伪失败，必须反复人工重建 reporter。

## 变更类型

代码级

## 变更范围

- 影响的需求文档：无
- 影响的功能规格：无
- 影响的业务场景：无
- 影响的 API：无
- 影响的 DB 表：无
- 影响的编排测试：`logos/resources/verify/test-results.jsonl`（写入行为由「清空」改为「保留追加」）
- 影响代码：`crates/octos-cli/src/commands/watchdog/tests.rs`

## 部署影响

- 是否需要部署：否
- 部署原因：仅本地测试基础设施行为修正，与运行态服务无关
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述

将 `watchdog/tests.rs` 中 reporter 初始化逻辑从「无条件清空账本」改为「不存在时创建目录与空文件，存在时保留追加」。这样多场景测试可以按 last-write-wins 原则安全追加记录，不再互相破坏。

本次修复不改动 reporter 写入格式、ID 规范或 last-write-wins 语义；仅移除对历史数据的破坏性清空，使 `openlogos verify` 在正常工作流下即可保持完整覆盖。
