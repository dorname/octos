## MODIFIED — octos 需求文档

> 最后更新：2026-09-29
> 基线说明：本文档基于仓库现状正向定义产品需求基线。场景编号接续已占用的 S01（chatgpt-oauth-codex 变更落地）。基线数字 2026-09-29 复核（trim-unused-features 裁剪后口径）：30 个顶层 CLI 命令、17 个通道实现裁剪至 11 个（9 个 IM 通道 + api/cli 2 个本地通道）、平台 crate 23 个裁剪至 20 个（移除 octos-ffi / octos-uniffi / octos-pyo3 绑定家族）、35 个内置工具注册（含 feature-gated 与别名）、pipeline/memory/sandbox/plugin 子系统。

## MODIFIED — 1.1 产品定位

octos 是一个 **Rust 原生、API 优先的多租户智能体操作系统（Agentic OS）**：单一二进制即可在本地、服务器或容器中运行 AI 智能体，通过 CLI、REST API 和 9 个 IM 通道（Telegram / Discord / 飞书 / 企业微信 / WhatsApp / Twilio 等）触达用户，内置工具系统、五后端沙箱、混合检索记忆、DOT 图流水线编排与二进制协议插件生态。

一句话定位：**为开发者和团队提供"跑在任何地方、接入任何通道、可安全放权"的 AI 智能体运行平台。**

与单一形态的 AI 助手（纯 CLI / 纯 Web UI）不同，octos 的核心差异是：

1. **三种运行时同一内核**：`octos chat`（交互式 CLI）、`octos gateway`（多通道常驻网关）、`octos serve`（REST API + Web 仪表盘）共享同一个 agent loop、工具系统与记忆体系。
2. **安全放权是一等公民**：五个沙箱后端按平台自动决策，显式模式不可用时 fail-closed 拒绝而非静默裸奔；工具策略、SSRF 防护、环境变量消毒贯穿所有执行面。
3. **可编排的长任务**：DOT 图流水线支持并行 fan-out、checkpoint 断点续跑、human gate 人工卡点，定时任务与子代理让无人值守自动化成为默认能力。
