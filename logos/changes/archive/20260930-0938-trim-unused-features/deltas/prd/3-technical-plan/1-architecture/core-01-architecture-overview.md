## MODIFIED — 一、系统上下文

octos 是单二进制、自托管的智能体运行平台。对外边界：LLM 提供商 API（OpenAI/Anthropic/Gemini/OpenRouter 及各 OpenAI 兼容家族、本地 llama.cpp/LM Studio）、9 个 IM 平台、MCP 工具生态、IDE（ACP）、调用方应用（REST/WebSocket）。

```mermaid
graph TB
    subgraph "用户侧"
        DEV["开发者<br/>CLI / IDE(Zed)"]
        TEAM["团队用户<br/>IM 群/私聊"]
        ADMIN["管理员<br/>Dashboard / REST"]
        APP["调用方应用<br/>REST / WebSocket"]
    end

    subgraph "octos 单二进制"
        CORE["octos<br/>chat / gateway / serve"]
    end

    subgraph "外部服务"
        LLM["LLM 提供商<br/>OpenAI / Anthropic / Gemini<br/>OpenRouter / 本地 llama.cpp"]
        IM["IM 平台 ×9<br/>Telegram / Discord / 飞书<br/>企微 / WhatsApp / Twilio ..."]
        MCP["MCP 工具服务器<br/>stdio / HTTP"]
        SKILL["技能插件<br/>自包含二进制"]
    end

    DEV -->|"stdio / ACP"| CORE
    TEAM -->|"bot 消息"| IM
    IM -->|"轮询 / webhook 入站"| CORE
    CORE -->|"出站回复"| IM
    ADMIN -->|"HTTP /api/admin"| CORE
    APP -->|"HTTP /api/my · WS /api/ui-protocol"| CORE
    CORE -->|"HTTPS chat/responses"| LLM
    CORE -->|"JSON-RPC stdio/http"| MCP
    CORE -->|"二进制协议 JSON stdin/stdout"| SKILL
```

> 口径说明（2026-09-29，trim-unused-features）：IM 通道 9 个（telegram / discord / whatsapp / feishu / twilio / wecom / wecom-bot / matrix / matrix-user），另有 api/cli 两个本地通道，`*_channel.rs` 合计 11 个；dingtalk / slack / line / email / qq-bot / wechat 已随裁剪移除。

## MODIFIED — 二、分层架构（crate 级）

Rust 工作区（edition 2024，rust 1.85+），20 个平台 crate + 15 个技能 crate（14 app-skills + 1 platform-skill）。依赖方向自上而下，禁止反向依赖。

```mermaid
graph TB
    subgraph "L5 入口"
        CLI["octos-cli<br/>clap 命令 ×30 / 配置 / config watcher / api"]
    end
    subgraph "L4 服务端运行时"
        SRV["octos-server<br/>聚合运行时"]
        FLW["octos-fleet-worker<br/>fleet 执行体"]
    end
    subgraph "L3 编排层"
        PIPE["octos-pipeline<br/>DOT 图流水线引擎"]
        WF["octos-workflows<br/>工作流编排"]
        SWM["octos-swarm"]
        DORA["octos-dora-mcp"]
    end
    subgraph "L2 智能体核心"
        AGENT["octos-agent<br/>Agent loop / 工具 / 沙箱 / MCP / compaction / 插件"]
        SVC["octos-services"]
        EMB["octos-embed-llama<br/>内嵌 embedder"]
    end
    subgraph "L1 能力层"
        BUS["octos-bus<br/>消息总线 / 11 通道实现 / 会话 / coalesce / cron / heartbeat"]
        LLMC["octos-llm<br/>LlmProvider 抽象 / 各厂商 / registry / failover"]
        MEM["octos-memory<br/>EpisodeStore / MemoryStore / HybridSearch"]
        STORE["octos-store"]
        PLUG["octos-plugin<br/>插件 SDK"]
        DIAG["octos-diagnostics"]
    end
    subgraph "L0 基础层"
        COREC["octos-core<br/>Task / Message / Error（无内部依赖）"]
        SBX["octos-sandbox"]
        WASM["octos-wasm"]
        FLEET["octos-fleet"]
    end

    CLI --> SRV
    CLI --> AGENT
    CLI --> BUS
    CLI --> LLMC
    CLI --> MEM
    CLI --> PIPE
    CLI --> PLUG
    SRV --> AGENT
    SRV --> BUS
    SRV --> STORE
    SRV --> SVC
    SRV --> PIPE
    FLW --> AGENT
    FLW --> FLEET
    PIPE --> AGENT
    PIPE --> PLUG
    PIPE --> LLMC
    PIPE --> MEM
    WF --> PIPE
    SWM --> AGENT
    DORA --> AGENT
    AGENT --> BUS
    AGENT --> MEM
    AGENT --> LLMC
    AGENT --> PLUG
    SVC --> BUS
    SVC --> LLMC
    EMB --> LLMC
    BUS --> COREC
    MEM --> COREC
    STORE --> COREC
    PLUG --> COREC
    DIAG --> COREC
    LLMC --> COREC
```

依赖规则：`octos-core` 无内部依赖；技能 crate（app-skills / platform-skills）不进入平台依赖图，运行期以二进制协议接入。

> 口径说明（2026-09-29，trim-unused-features）：FFI 绑定家族（octos-ffi / octos-uniffi / octos-pyo3）已移除，平台 crate 23 → 20；octos-bus 通道实现 11 个（9 IM + api/cli）；clap 顶层子命令 30 个。

## MODIFIED — 三、三种运行时部署视图

同一内核（agent loop + 工具 + 记忆）支撑三种进程形态：

```mermaid
graph LR
    subgraph "octos chat（交互式 CLI）"
        C1["REPL / -m 单条"] --> C2["Agent loop"]
        C2 --> C3["工具执行<br/>沙箱 + SafePolicy"]
        C2 --> C4["compaction"]
    end
    subgraph "octos gateway（常驻网关）"
        G1["通道适配器 ×9（IM）"] --> G2["入站队列"]
        G2 --> G3["会话 actor"]
        G3 --> G4["Agent loop"]
        G4 --> G5["coalesce 分片"]
        G5 --> G6["出站队列 → 通道"]
        G7["cron / heartbeat"] --> G2
    end
    subgraph "octos serve（REST + Dashboard）"
        S1["axum router<br/>REST / WS / SSE 全量路由"] --> S2["认证中间件"]
        S2 --> S3["handler → Agent loop"]
        S3 --> S4["SSE / WS 流式"]
        S5["static: dashboard"]
    end
    subgraph "共享内核"
        K1["octos-agent loop"]
        K2["octos-llm 提供商栈"]
        K3["octos-memory"]
        K4["octos-bus 会话"]
    end
    C2 -.共享.-> K1
    G4 -.共享.-> K1
    S3 -.共享.-> K1
```

> 口径说明（2026-09-29）：gateway 通道适配器为 9 个 IM 通道（api/cli 为本地通道，不经 gateway 适配）；路由数不设固定口径（实测 route( 注册点 226 处、唯一路径 76 条，随开发演进）。

## MODIFIED — 4.3 消息总线与通道（gateway 内核）

```mermaid
graph LR
    subgraph "入站"
        I1["通道适配器 ×9（IM）<br/>轮询 / webhook"] --> I2["统一 InboundMessage"]
        I2 --> I3["会话解析/创建<br/>SessionKey(channel+chat+profile)"]
        I3 --> I4["会话 actor<br/>每会话串行"]
        I4 --> I5["Agent loop"]
    end
    subgraph "会话存储"
        SS["SessionManager<br/>JSONL + LRU 缓存<br/>10MB 上限 / tmp+rename 原子写<br/>/new fork（parent_key）"]
    end
    I4 <--> SS
    subgraph "出站"
        O1["agent 回复"] --> O2["coalesce 分片<br/>段落>换行>句子>空格>硬切<br/>≤50 片 / UTF-8 安全"]
        O2 --> O3["通道格式渲染 → 发送"]
    end
    I5 --> O1
    CR["cron / heartbeat<br/>系统来源注入"] --> I2
```

> 口径说明（2026-09-29）：通道适配器 9 个 IM 通道，同三节口径。

## MODIFIED — 八、外部依赖与测试策略

| 依赖 | 用途 | 测试策略 |
|------|------|---------|
| LLM 提供商 API | 全部对话能力 | mock-service（录制/桩 provider）；真实调用测试 `#[ignore]` 手动触发 |
| IM 平台（9 通道） | 消息收发 | mock-service（通道适配层以假 InboundMessage 单测；端到端手动验证） |
| MCP 服务器 | 工具扩展 | mock-service（内置测试 server fixture） |
| 沙箱后端（bwrap/docker 等） | 命令隔离 | env-disable + 纯决策层矩阵单测（HostOs × Probe 全组合可跨宿主测试） |
| OAuth IdP（OpenAI 等） | 登录 | mock-service + fixed-value（本地 callback 仿真） |
| embedding provider | 记忆向量通道 | env-disable（降级 BM25-only 路径即默认测试路径） |
