# core 系统结构图(逆向种子基线)

> provenance: reverse-engineered · verified: false · seed pass 2026-09-18 · 非权威意图,仅为现状快照

> 本文件由 brownfield-adopter 逆向扫描生成:内容全部可从代码核验,但未经人工确认(`verified: false` 恒成立)。

## 总览

octos 是 Rust 工作区(2024 edition),根 `Cargo.toml` 声明 39 个 member:23 个平台 crate + 14 个 app-skills + 1 个 platform-skill(voice)+ 4 个 harness-starter(harness-starter-{generic,report,audio,coding},计入 app-skills 合计 14 个)。主二进制为 `octos`(crates/octos-cli)。

## 分层(边均核验自各 crate Cargo.toml 内部依赖声明)

```
L5  octos-cli (CLI/配置/api 入口,依赖下方全部)
      └─ octos-ffi → octos-uniffi / octos-pyo3 (绑定层; ffi→core/agent/llm/memory/cli/embed-llama)
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

## 平台 crate 清单(23)

| crate | 路径 | 职责(依据) |
|---|---|---|
| `octos-core` | `crates/octos-core` | octos-core — Task/Message/Error 基础类型,无内部依赖 |
| `octos-diagnostics` | `crates/octos-diagnostics` | octos-diagnostics — 诊断支持包(doctor) |
| `octos-memory` | `crates/octos-memory` | octos-memory — EpisodeStore(redb)/MemoryStore/HybridSearch(BM25+向量) |
| `octos-llm` | `crates/octos-llm` | octos-llm — LlmProvider 抽象 + 各厂商 provider + registry + failover |
| `octos-agent` | `crates/octos-agent` | octos-agent — Agent 循环、工具系统、沙箱、MCP、compaction、插件 |
| `octos-bus` | `crates/octos-bus` | octos-bus — 消息总线、17 通道、会话、coalescing、cron、heartbeat |
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
| `octos-ffi` | `crates/octos-ffi` | octos-ffi — C FFI 绑定(core/agent/llm/memory/cli/embed-llama) |
| `octos-uniffi` | `crates/octos-uniffi` | octos-uniffi — UniFFI 绑定(依赖 ffi) |
| `octos-wasm` | `crates/octos-wasm` | octos-wasm — WASM 目标(依赖 core) |
| `octos-pyo3` | `crates/octos-pyo3` | octos-pyo3 — Python 绑定(依赖 ffi) |

## 技能 crate(插件二进制协议:`./binary <tool>`,JSON stdin/stdout)

- app-skills(14): news, deep-search, deep-crawl, send-email, account-manager, time, weather, smart-home, wechat-bridge, skill-evolve, harness-starter-{generic,report,audio,coding}
- platform-skills(1): voice

## 关键入口

- CLI 入口:`crates/octos-cli/src/main.rs` → clap `Command` 枚举(29 个子命令,见场景候选清单)
- REST 入口:`crates/octos-cli/src/api/router.rs`(157 条唯一路由路径,grep 实测;其余 api/*.rs 含少量附加路由与测试引用)
- Agent 循环:`crates/octos-agent/src/agent.rs`(构建消息→LLM+工具规格→工具执行→压缩)
- 工具注册:`crates/octos-agent/src/tools/registry.rs` `with_builtins_and_permissions`(L1253-1380)
- 通道实现:`crates/octos-bus/src/*_channel.rs`(17 个)

## 备注(本次未逐项核验)

- 依赖边覆盖各 Cargo.toml 中的内部依赖声明;个别 crate 若使用多行写法可能遗漏边。
- feature-gated 项:`serve`(api)、`browser`、`git` 工具、`code_structure`(ast)、email 通道等。
- 运行时数据目录约定 `~/.octos`(config.json/auth.json/sessions),未在本 seed 轮次逐文件核验。

## 逆向基线来源
```yaml
candidates:
  - key: core::033a8deeaf15
    anchor: crate:octos-agent
    display: octos-agent — Agent 循环、工具系统、沙箱、MCP、compaction、插件
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::2965fa2b5697
    anchor: crate:octos-ffi
    display: octos-ffi — C FFI 绑定(core/agent/llm/memory/cli/embed-llama)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::314782ae0e54
    anchor: crate:octos-bus
    display: octos-bus — 消息总线、17 通道、会话、coalescing、cron、heartbeat
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::323866afcf28
    anchor: crate:octos-plugin
    display: octos-plugin — 插件 SDK:manifest 解析、发现、门控
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::3318f822ae3f
    anchor: crate:octos-sandbox
    display: octos-sandbox — 平台沙箱助手(无内部依赖)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::336028a8885c
    anchor: crate:octos-dora-mcp
    display: octos-dora-mcp — dora MCP 集成(依赖 agent)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::456fe3c14ce3
    anchor: crate:octos-wasm
    display: octos-wasm — WASM 目标(依赖 core)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::59ddb15900b5
    anchor: crate:octos-pyo3
    display: octos-pyo3 — Python 绑定(依赖 ffi)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::5b59438f58cb
    anchor: crate:octos-llm
    display: octos-llm — LlmProvider 抽象 + 各厂商 provider + registry + failover
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::5d2662075ce9
    anchor: crate:octos-services
    display: octos-services — 服务层(core/llm/bus)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::60b3a22586fa
    anchor: crate:octos-fleet-worker
    display: octos-fleet-worker — fleet worker(agent/core/fleet/llm/memory)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::63d3dc39560e
    anchor: crate:octos-workflows
    display: octos-workflows — 工作流编排(依赖 agent/pipeline)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::79950e79c709
    anchor: crate:octos-server
    display: octos-server — 服务端运行时(聚合 agent/bus/store/services/pipeline)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::84d4da25917b
    anchor: crate:octos-store
    display: octos-store — 持久化存储(依赖 core)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::8f04768a2d81
    anchor: crate:octos-diagnostics
    display: octos-diagnostics — 诊断支持包(doctor)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::ada226afa6c9
    anchor: crate:octos-memory
    display: octos-memory — EpisodeStore(redb)/MemoryStore/HybridSearch(BM25+向量)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::b33eb7116b89
    anchor: crate:octos-core
    display: octos-core — Task/Message/Error 基础类型,无内部依赖
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::c8c02c4f023b
    anchor: crate:octos-fleet
    display: octos-fleet — fleet 核心(依赖 core)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::d45f8cd71f91
    anchor: crate:octos-uniffi
    display: octos-uniffi — UniFFI 绑定(依赖 ffi)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::ea6d2e2f17dd
    anchor: crate:octos-embed-llama
    display: octos-embed-llama — 内嵌 llama(依赖 llm)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::f1084344787e
    anchor: crate:octos-cli
    display: octos-cli — CLI 二进制:clap 命令、配置加载、config watcher、api
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::f1825145b41e
    anchor: crate:octos-swarm
    display: octos-swarm — swarm 协调(依赖 agent)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::f5512b818779
    anchor: crate:octos-pipeline
    display: octos-pipeline — DOT 图流水线引擎(fan-out/checkpoint/human gate)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
```
