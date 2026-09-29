## MODIFIED — 总览

octos 是 Rust 工作区(2024 edition),根 `Cargo.toml` 声明 35 个 member:20 个平台 crate + 14 个 app-skills(含 4 个 harness-starter-{generic,report,audio,coding})+ 1 个 platform-skill(voice)。主二进制为 `octos`(crates/octos-cli)。(2026-09-29 trim-unused-features:member 38 → 35,平台 crate 23 → 20,移除 octos-ffi / octos-uniffi / octos-pyo3)

## MODIFIED — 分层(边均核验自各 crate Cargo.toml 内部依赖声明)

```
L5  octos-cli (CLI/配置/api 入口,依赖下方全部)
L4  octos-server → core, agent, llm, bus, store, services, workflows, pipeline, plugin
    octos-fleet-worker → agent, core, fleet, llm, memory
L3  octos-pipeline → core, agent, plugin, llm, memory
    octos-workflows → core, agent, pipeline
    octos-swarm → agent        octos-dora-mcp → agent
L2  octos-agent → core, bus, memory, llm, plugin
    octos-services → core, llm, bus
    octos-embed-llama → llm
L1  octos-bus → core           octos-llm → core
    octos-memory → core        octos-store → core
    octos-plugin → core        octos-diagnostics → core
L0  octos-core (Task/Message/Error; 无内部依赖)
    octos-sandbox / octos-wasm(→core) / octos-fleet(→core)
```

## MODIFIED — 平台 crate 清单(23)

## 平台 crate 清单(20)

| crate | 路径 | 职责(依据) |
|---|---|---|
| `octos-core` | `crates/octos-core` | octos-core — Task/Message/Error 基础类型,无内部依赖 |
| `octos-diagnostics` | `crates/octos-diagnostics` | octos-diagnostics — 诊断支持包(doctor) |
| `octos-memory` | `crates/octos-memory` | octos-memory — EpisodeStore(redb)/MemoryStore/HybridSearch(BM25+向量) |
| `octos-llm` | `crates/octos-llm` | octos-llm — LlmProvider 抽象 + 各厂商 provider + registry + failover |
| `octos-agent` | `crates/octos-agent` | octos-agent — Agent 循环、工具系统、沙箱、MCP、compaction、插件 |
| `octos-bus` | `crates/octos-bus` | octos-bus — 消息总线、11 个通道实现(9 IM + api/cli 本地通道)、会话、coalescing、cron、heartbeat |
| `octos-workflows` | `crates/octos-workflows` | octos-workflows — 工作流编排(依赖 agent/pipeline) |
| `octos-server` | `crates/octos-server` | octos-server — 服务端运行时(聚合 agent/bus/store/services/pipeline) |
| `octos-store` | `crates/octos-store` | octos-store — 持久化存储(依赖 core) |
| `octos-services` | `crates/octos-services` | octos-services — 服务层(core/llm/bus) |
| `octos-cli` | `crates/octos-cli` | octos-cli — CLI 二进制:clap 命令、配置加载、config watcher、api |
| `octos-dora-mcp` | `crates/octos-dora-mcp` | octos-dora-mcp — dora MCP 集成(依赖 agent) |
| `octos-pipeline` | `crates/octos-pipeline` | octos-pipeline — DOT 图流水线引擎(fan-out/checkpoint/human gate) |
| `octos-plugin` | `crates/octos-plugin` | octos-plugin — 插件 SDK:manifest 解析、发现、门控 |
| `octos-sandbox` | `crates/octos-sandbox` | octos-sandbox — 平台沙箱助手(无内部依赖) |
| `octos-swarm` | `crates/octos-swarm` | octos-swarm — swarm 协调(依赖 agent) |
| `octos-fleet` | `crates/octos-fleet` | octos-fleet — fleet 核心(依赖 core) |
| `octos-fleet-worker` | `crates/octos-fleet-worker` | octos-fleet-worker — fleet worker(agent/core/fleet/llm/memory) |
| `octos-embed-llama` | `crates/octos-embed-llama` | octos-embed-llama — 内嵌 llama(依赖 llm) |
| `octos-wasm` | `crates/octos-wasm` | octos-wasm — WASM 目标(依赖 core) |

## REMOVED-ITEMS — 平台 crate 清单(23)

- octos-ffi — C FFI 绑定,随 trim-unused-features 移除(无嵌入非 Rust 应用的使用场景)
- octos-uniffi — UniFFI 绑定,同上移除
- octos-pyo3 — PyO3 Python 绑定,同上移除

## MODIFIED — 关键入口

- CLI 入口:`crates/octos-cli/src/main.rs` → clap `Command` 枚举(30 个顶层子命令,2026-09-29 核验 `crates/octos-cli/src/commands/mod.rs`)
- REST 入口:`crates/octos-cli/src/api/router.rs`(184 处 route( 注册 / 70 条唯一路由路径,2026-09-29 grep 实测;全 crate 合计 226 处注册 / 76 条唯一路径;其余 api/*.rs 含少量附加路由与测试引用)
- Agent 循环:`crates/octos-agent/src/agent.rs`(构建消息→LLM+工具规格→工具执行→压缩)
- 工具注册:`crates/octos-agent/src/tools/registry.rs` `with_builtins_and_permissions`(35 个 register 调用,含 feature-gated 与别名,2026-09-29 核验)
- 通道实现:`crates/octos-bus/src/*_channel.rs`(11 个 = 9 个 IM 通道 + api/cli 2 个本地通道;dingtalk/slack/line/email/qq-bot/wechat 已随 trim-unused-features 移除)

> 备注:末节「逆向基线来源」YAML 为 2026-09-18 逆向种子轮的冻结记录(verified:false 恒成立),不随本次裁剪改写;其中 `17 通道` 与 ffi/uniffi/pyo3 条目的口径以本节为准。
