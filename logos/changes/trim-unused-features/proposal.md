# 变更提案：trim-unused-features

> module: core | created: 2026-09-29 | branch: dorname-trim

## 变更原因

用户决定对项目进行功能与冗余裁剪，让新分支 `dorname-trim` 更干净。经 2026-09-29 全量代码调研与用户逐项确认，确定裁剪范围：

1. **IM 通道精简**：保留 CI 基准 7 通道（telegram / discord / whatsapp / feishu / twilio / wecom / wecom-bot）+ matrix / matrix-user（`api` feature 编译耦合，暂不可拆）+ api / cli 本地通道；**删除 6 个通道**：dingtalk、slack、line、email、qq-bot、wechat。
2. **删除 FFI 绑定家族**：octos-ffi / octos-uniffi / octos-pyo3（合计 4.4k 行）——它们解决「把 octos 嵌入 Python/Swift/Kotlin/C 应用」的问题，当前无此使用场景；octos-wasm（浏览器客户端绑定）不依赖 ffi（仅注释提及），保留。
3. **冗余内容清理**：根目录一次性修复笔记（DEEP_RESEARCH_TIMEOUT_FIX.md、OUTER_LOOP_REVIEW.md 等）归档至 `docs/archive/`；审查 `scripts/`（55 个）删除明确遗留/重复的脚本。

明确**保留**：fleet / fleet-worker（多机执行演进预留）、swarm / dora-mcp、全部 15 个技能 crate、docs/ 下 ADR 正文。

## 变更类型

设计级（功能裁剪：删除通道适配与绑定层能力，需更新需求/设计/架构文档 + 代码；无对外 API/DB 契约变更——裁剪对象均为构建期 feature 与未部署的绑定 crate）

## 变更范围

- 影响的需求文档：`core-01-requirements.md`（基线数字、1.1 通道数）
- 影响的功能规格：`core-03-gateway-channels-design.md`（17 通道清单 → 11）；core-04/05/07/08 中零散通道引用核查
- 影响的业务场景：S04（gateway 通道场景引用核查）
- 影响的技术架构：`core-01-architecture-overview.md`（L5 FFI 层移除、通道数字、crate 数 23→20）、`core-system-map.md`（crate 清单删 3 行、member 38→35、通道 17→11）
- 影响的部署方案：`core-01-deployment-plan.md`（feature 清单核查）
- 影响的测试规格：`core-S01-test-cases.md`、`core-S16-test-cases.md`（被删通道相关用例核查/点名删除）
- 影响的 API：无契约变更
- 影响的 DB 表：无
- 影响的编排测试：无
- 影响的 smoke 测试：无
- 影响的代码：根 `Cargo.toml`（members）、`octos-bus`（6 个通道文件 + lib.rs + Cargo.toml features）、`octos-cli`（gateway adapters、config.rs、profiles.rs、channels.rs、account.rs、api/webhook_proxy.rs、ui_protocol_transport.rs、session_actor.rs 等引用点、Cargo.toml features）、`scripts/milestone-ci.sh`、`scripts/check-oup-bindings.py`、`README.md`、`CLAUDE.md`、删除 `crates/octos-ffi|octos-uniffi|octos-pyo3` 三个目录

## 部署影响

- 是否需要部署：否
- 部署原因：裁剪对象均为构建期 feature（被删通道不在 CI 基准安装特性内）与未部署的绑定 crate；k8s 部署的 web pod 仅用 `api` feature，不受影响
- 影响环境：无
- 是否涉及数据迁移：否
- 是否需要回滚预案：否（git 历史即回滚）
- 是否需要 smoke：否

## 变更概述

本变更在 `dorname-trim` 分支上做三件事：

1. **删通道**：删除 dingtalk / slack / line / email / qq-bot / wechat 六个通道的全部实现与引用（bus 通道文件、cli gateway 适配器、config/profiles/api 引用点、feature 定义、文档清单）。删除后通道实现从 17 个变为 11 个（9 IM + api/cli）。
2. **删 FFI 家族**：从 workspace 移除 octos-ffi / octos-uniffi / octos-pyo3 三个 crate 目录及全部引用（milestone-ci.sh、check-oup-bindings.py、README、架构文档 L5 层）。平台 crate 从 23 个变为 20 个，workspace member 从 38 变为 35。
3. **清冗余**：根目录一次性笔记归档 `docs/archive/`；`scripts/` 逐个人工审查，删除明确遗留/重复脚本（审查清单在 [code] 切片中给出）。

裁剪后必须通过：`cargo build --workspace`、`cargo test --workspace`、`node scripts/check-mermaid.mjs`、以及 canonical feature 安装命令（`cargo install --path crates/octos-cli --features "api,telegram,discord,whatsapp,feishu,twilio,wecom,wecom-bot,audio_mp3"`）可用。
