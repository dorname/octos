# 变更提案：修复 CronService toggle_job_reconciling 未从磁盘重载

> module: core | created: 2026-09-30

## 变更原因

`crates/octos-bus/src/cron_service.rs` 的 `CronService::toggle_job_reconciling` 注释宣称："under one store-lock hold, re-read `cron.json` into memory (adopting writes from other owners)"，但实际实现直接调用 `LocalCronStore::get_schedule`（仅查内存），从未读取磁盘。这导致外部所有者（如 gateway 子进程或 CLI）向 `cron.json` 新增 job 后，长生命周期 `CronService` 通过 `/api/my/cron` toggle 时无法发现新 job，返回 `NotFound`（见 `crates/octos-cli/src/api/cron_panel.rs` 测试 `toggle_via_service_adopts_external_writes_instead_of_erasing_them` 稳定失败）。

该问题在 `k8s-stateless c5 N2-full`（db396b8f，2026-09-15）CronService store-backed rewrite 中引入并退化。

## 变更类型

代码级

## 变更范围

- 影响的需求文档：无
- 影响的功能规格：无
- 影响的业务场景：S06（cron 自动化）
- 影响的 API：无（内部行为修正）
- 影响的 DB 表：无
- 影响的编排测试：无
- 影响代码：`crates/octos-bus/src/cron_service.rs`、`crates/octos-bus/src/local_cron_store.rs`

## 部署影响

- 是否需要部署：否
- 部署原因：纯运行时行为修复，与部署形态无关
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：否

## 变更概述

在 `LocalCronStore` 新增「从磁盘重载 store」的私有/包内方法（复用已有的 `load_store_or_quarantine`），并在 `toggle_job_reconciling` 的锁保护下先重载磁盘最新状态，再查找目标 job 并更新/持久化。修复后外部写入的 cron job 会被服务在 toggle 前采纳，同时保留原 `update_schedule` 的内存状态与持久化语义。

同步修正 `toggle_via_service_adopts_external_writes_instead_of_erasing_them` 测试的断言，使其反映真实的