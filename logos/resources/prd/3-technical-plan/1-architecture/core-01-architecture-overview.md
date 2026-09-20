# octos 技术架构概要

> 最后更新：2026-09-20
> 输入：core-01-requirements.md（S01–S15）、core-02~06 产品设计、core-system-map.md（逆向种子，本文以代码核验为准）
> 架构文件全局唯一：后续架构修改始终更新本文档，不新建文件。

## 一、系统上下文

octos 是单二进制、自托管的智能体运行平台。对外边界：LLM 提供商 API（OpenAI/Anthropic/Gemini/OpenRouter 及各 OpenAI 兼容家族、本地 llama.cpp/LM Studio）、17 个 IM 平台、MCP 工具生态、IDE（ACP）、调用方应用（REST/WebSocket）。

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
        IM["IM 平台 ×17<br/>Telegram / Discord / Slack<br/>飞书 / 企微 / WhatsApp / Email ..."]
        MCP["MCP 工具服务器<br/>stdio / HTTP"]
        SKILL["技能插件<br/>自包含二进制"]
    end

    DEV -->|"stdio / ACP"| CORE
    TEAM -->|"bot 消息"| IM
    IM <-->|"轮询 / webhook"| CORE
    ADMIN -->|"HTTP /api/admin"| CORE
    APP -->|"HTTP /api/my · WS /api/ui-protocol"| CORE
    CORE -->|"HTTPS chat/responses"| LLM
    CORE -->|"JSON-RPC stdio/http"| MCP
    CORE -->|"二进制协议 JSON stdin/stdout"| SKILL
```

## 二、分层架构（crate 级）

Rust 工作区（edition 2024，rust 1.85+），23 个平台 crate + 15 个技能 crate（14 app-skills + 1 platform-skill）。依赖方向自上而下，禁止反向依赖。

```mermaid
graph TB
    subgraph "L5 入口与绑定"
        CLI["octos-cli<br/>clap 命令 ×29 / 配置 / config watcher / api"]
        FFI["octos-ffi → octos-uniffi / octos-pyo3<br/>C / Kotlin·Swift / Python 绑定"]
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
        BUS["octos-bus<br/>消息总线 / 17 通道 / 会话 / coalesce / cron / heartbeat"]
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

    CLI --> SRV & AGENT & BUS & LLMC & MEM & PIPE & PLUG
    FFI --> CLI
    SRV --> AGENT & BUS & STORE & SVC & PIPE
    FLW --> AGENT & FLEET
    PIPE --> AGENT & PLUG & LLMC & MEM
    WF --> PIPE
    SWM --> AGENT
    DORA --> AGENT
    AGENT --> BUS & MEM & LLMC & PLUG
    SVC --> BUS & LLMC
    EMB --> LLMC
    BUS & MEM & STORE & PLUG & DIAG --> COREC
    LLMC --> COREC
```

依赖规则：`octos-core` 无内部依赖；技能 crate（app-skills / platform-skills）不进入平台依赖图，运行期以二进制协议接入。

## 三、三种运行时部署视图

同一内核（agent loop + 工具 + 记忆）支撑三种进程形态：

```mermaid
graph LR
    subgraph "octos chat（交互式 CLI）"
        C1["REPL / -m 单条"] --> C2["Agent loop"]
        C2 --> C3["工具执行<br/>沙箱 + SafePolicy"]
        C2 --> C4["compaction"]
    end
    subgraph "octos gateway（常驻网关）"
        G1["通道适配器 ×17"] --> G2["入站队列"]
        G2 --> G3["会话 actor"]
        G3 --> G4["Agent loop"]
        G4 --> G5["coalesce 分片"]
        G5 --> G6["出站队列 → 通道"]
        G7["cron / heartbeat"] --> G2
    end
    subgraph "octos serve（REST + Dashboard）"
        S1["axum router<br/>23 组 / 157 路由"] --> S2["认证中间件"]
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

## 四、子系统架构（分层子图）

### 4.1 Agent loop 与上下文压缩

```mermaid
graph TB
    subgraph "Agent loop（crates/octos-agent/src/agent.rs）"
        A1["构建消息<br/>系统提示词 + 历史 + 记忆 + SKILL.md"] --> A2["调用 LLM<br/>工具规格（policy 过滤后）"]
        A2 --> A3{"stop_reason?"}
        A3 -->|"ToolUse"| A4["执行工具<br/>hooks: before/after_tool_call"]
        A4 --> A5["结果回注消息"]
        A5 --> A6{"token 预算?"}
        A6 -->|"接近上限"| A7["compaction<br/>剥参数/摘要/保留最近对"]
        A7 --> A2
        A6 -->|"充足"| A2
        A3 -->|"EndTurn"| A8["返回结果"]
        A3 -->|"预算耗尽"| A8
    end
    A2 -.-> H1["hooks: before/after_llm_call"]
```

### 4.2 工具系统与沙箱决策

```mermaid
graph TB
    subgraph "工具面（tools/）"
        T0["Tool trait: spec + execute"] --> T1["ToolRegistry<br/>HashMap 注册/分发"]
        T1 --> T2["ToolPolicy 过滤<br/>deny 优先 / 通配 / group:* / byProvider"]
        T2 --> T3["specs() 输出<br/>仅 provider_policy + context_filter 过滤"]
    end
    subgraph "执行面"
        E1["shell / exec_command / bash"] --> E2["SafePolicy<br/>危险命令拒绝（空白归一化匹配）"]
        E2 --> E3["decide_sandbox<br/>HostOs × HostBackendProbe 纯决策"]
        E3 --> E4{"mode 解析"}
        E4 -->|"显式后端不可用"| E5["RefusingSandbox<br/>fail-closed + 按 OS 修复指引"]
        E4 -->|"auto 有可用"| E6["Bwrap / Landlock / Macos<br/>AppContainer / Docker"]
        E4 -->|"auto 无可用"| E7["NoSandbox 响亮降级<br/>（fail_closed=true 则转 E5）"]
        E4 -->|"enabled=false / none"| E8["NoSandbox（显式豁免）"]
    end
    T1 --> E1
    T9["文件工具"] --> T10["O_NOFOLLOW 防符号链接 I/O"]
    T11["web_fetch"] --> T12["SSRF 防护<br/>私网/ULA/IPv4-mapped 拦截"]
    T13["MCP / hooks / browser"] --> T14["BLOCKED_ENV_VARS ×18 消毒"]
```

### 4.3 消息总线与通道（gateway 内核）

```mermaid
graph LR
    subgraph "入站"
        I1["通道适配器 ×17<br/>轮询 / webhook"] --> I2["统一 InboundMessage"]
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

### 4.4 LLM 提供商栈与故障转移

```mermaid
graph TB
    REQ["Agent 请求"] --> AR["AdaptiveRouter<br/>lane 打分 / 熔断 / hedge racing"]
    AR --> PC["ProviderChain<br/>主备链 + 熔断"]
    PC --> RP["RetryProvider<br/>429/5xx 指数退避"]
    RP --> P1["AnthropicProvider"]
    RP --> P2["OpenAIProvider"]
    RP --> P3["GeminiProvider"]
    RP --> P4["OpenRouterProvider"]
    RP --> P5["OpenAI 兼容家族<br/>with_base_url + registry/ 每家族一模块"]
    RP --> P6["local 家族<br/>llama.cpp / LM Studio<br/>默认 127.0.0.1:8080/v1 免 key"]
    CAT["model_catalog.json<br/>模型名/默认 SSOT<br/>门禁 onboarding 可见性"] -.-> P5
    CRED["凭证解析<br/>auth store → env_vars(含 keychain) → 进程 env"] -.-> P2
    CRED -->|"ChatGPT 订阅 OAuth"| P7["OpenAI Responses<br/>Codex 后端路由"]
```

### 4.5 记忆系统

```mermaid
graph LR
    subgraph "写入"
        W1["任务完成"] --> W2["EpisodeStore<br/>.octos/episodes.redb<br/>任务摘要"]
        W3["memory-refresh 流水线<br/>提取 pass / 合并 pass"] --> W4["MEMORY.md + 每日笔记"]
    end
    subgraph "读取（系统提示词注入）"
        R1["HybridSearch<br/>向量 0.7 + BM25 0.3<br/>HNSW 索引"] --> R2["相关 episode"]
        R3["MemoryStore<br/>7 天窗口近期记忆"] --> R4["注入上下文"]
        R2 --> R4
        R5["无 embedding → BM25-only 降级"] -.-> R1
    end
```

### 4.6 插件与 MCP

```mermaid
graph TB
    subgraph "技能插件（octos-plugin + app-skills ×14 + voice）"
        P1["manifest.json<br/>id / tools / requires / spawn_only"] --> P2["discovery<br/>profile > user > bundled > legacy"]
        P2 --> P3["gating<br/>binary / env / OS 检查"]
        P3 --> P4["PluginTool 适配"]
        P4 --> P5["二进制协议<br/>./binary tool_name<br/>JSON stdin → JSON stdout"]
        P4 --> P6["spawn_only → 自动 tokio::spawn<br/>SKILL.md 注入系统提示词"]
    end
    subgraph "MCP（agent/mcp.rs）"
        M1["config 声明 server"] --> M2["JSON-RPC stdio 握手"]
        M2 --> M3["schema 校验<br/>深度 ≤10 / ≤64KB"]
        M3 -->|"合法"| M4["注册进 ToolRegistry"]
        M3 -->|"超限"| M5["拒绝注册并记录"]
    end
    P4 --> TR["ToolRegistry"]
    M4 --> TR
```

## 五、系统处理流程（端到端）

### 5.1 octos chat 单轮请求流程

```mermaid
graph TB
    U1["用户输入消息"] --> U2["加载 config<br/>解析凭证链 auth→env"]
    U2 --> U3["构建系统提示词<br/>bootstrap 文件 + 记忆 + 技能"]
    U3 --> U4["LLM 调用（提供商栈）"]
    U4 --> U5{"返回工具调用?"}
    U5 -->|"是"| U6["hooks before_tool_call<br/>可拒绝(exit 1)"]
    U6 --> U7["策略检查 → 沙箱执行"]
    U7 --> U8["hooks after_tool_call"]
    U8 --> U9["结果回注 → 写会话 JSONL"]
    U9 --> U10{"预算/迭代检查"}
    U10 -->|"继续"| U4
    U5 -->|"否"| U11["输出回答 → 写会话 → 记 episode"]
```

### 5.2 gateway 消息处理流程

```mermaid
graph TB
    G1["IM 消息到达"] --> G2["通道适配 → InboundMessage"]
    G2 --> G3{"会话命令?"}
    G3 -->|"/new /back /s"| G4["会话管理<br/>fork / 切换"]
    G3 -->|"普通消息"| G5["会话 actor 队列"]
    G5 --> G6["agent loop 处理<br/>（同 5.1 内核）"]
    G6 --> G7["回复 → coalesce 分片"]
    G7 --> G8["通道渲染 → 分片发送"]
    G9["cron 到点 / heartbeat"] --> G2
```

### 5.3 serve 请求处理流程

```mermaid
graph TB
    S1["HTTP/WS 请求"] --> S2["认证中间件<br/>公开 / 用户 / admin 三层"]
    S2 -->|"401/403"| S3["拒绝（不泄露内部）"]
    S2 -->|"放行"| S4["handler 解析 profile/会话"]
    S4 --> S5["Agent loop 执行"]
    S5 --> S6{"流式?"}
    S6 -->|"是"| S7["SSE/WS 增量推送"]
    S6 -->|"否"| S8["JSON 响应"]
    S7 --> S9["完整消息落会话"]
    S8 --> S9
```

### 5.4 pipeline 执行流程

```mermaid
graph TB
    P1["run_pipeline 工具调用"] --> P2["DOT 解析 + 校验<br/>human_gate 必须带 resolver"]
    P2 --> P3["构建执行计划<br/>ModelStylesheet 分配模型"]
    P3 --> P4["依赖调度"]
    P4 --> P5{"节点类型"}
    P5 -->|"agent"| P6["子 agent 执行节点 prompt"]
    P5 -->|"parallel"| P7["运行期展开 N worker 并发"]
    P5 -->|"human_gate"| P8["暂停 → resolver 请求确认<br/>确认/拒绝恢复"]
    P6 --> P9["checkpoint 落盘"]
    P7 --> P9
    P8 --> P9
    P9 --> P4
    P4 -->|"全部完成"| P10["PipelineResult<br/>输出/token/逐节点摘要/修改文件"]
    P1 -.恢复.-> P11["读取 checkpoint → 跳过已完成节点"]
```

## 六、技术选型

| 维度 | 选型 | 理由 | 备选方案 |
|------|------|------|---------|
| 语言 | Rust（edition 2024，1.85+） | 单二进制分发、内存安全（deny(unsafe_code)）、跨平台抽象 | Go（GC 暂停与二进制体积） |
| 异步运行时 | tokio | agent loop 并发工具执行、通道 IO、fan-out 的事实标准 | async-std（生态较小） |
| TLS | rustls（纯 Rust） | 无 OpenSSL 系统依赖，交叉编译与审计友好 | native-tls（引入系统依赖） |
| HTTP 框架 | axum | tokio 原生、类型安全路由、中间件生态 | actix-web（学习曲线） |
| 嵌入式存储 | redb | 纯 Rust 嵌入式 KV，episode 存储零运维 | sled（维护停滞）、sqlite（C 依赖） |
| 向量检索 | hnsw_rs | 纯 Rust HNSW，内存索引无外部服务 | qdrant（外部服务，违背单二进制） |
| LLM 抽象 | 自研 LlmProvider trait + registry | 4 原生 + N 兼容家族统一；3 层 failover 自研可控 | langchain-rust（不成熟） |
| 沙箱 | bwrap/Landlock/sandbox-exec/AppContainer/Docker 五后端 | 按平台用足 OS 原生隔离；纯决策层可单测 | 仅 Docker（Linux 开发机过重） |
| 通道框架 | 各平台 SDK/HTTP（teloxide 等） | 每通道最薄适配层，行为一致性由 bus 保证 | 桥接服务（引入外部依赖） |
| 流水线 | 自研 DOT 图引擎 | 与 agent/工具/模型栈深度集成，checkpoint/human gate 原生 | temporal（重型外部服务） |
| 插件协议 | 自包含二进制 + JSON stdin/stdout | 语言无关、进程隔离、无需 ABI 稳定 | 动态链接（ABI 脆弱）、WASM（已备 wasm crate 作演进方向） |
| 错误处理 | eyre / color-eyre | 调用链上下文丰富，CLI 友好 | anyhow（约定不如 eyre 报告） |

## 七、非功能性约束

### 7.1 性能
- 交互首 token：取决于提供商，本地不做固定 SLA；CLI 渲染保持流式即时回显
- gateway 单消息处理：会话 actor 串行保证一致性，跨会话并发
- 工具参数上限 1MB（非分配估算防 OOM）；会话文件上限 10MB；coalesce 分片 ≤ 50
- 无人值守迭代兜底：UNATTENDED_MAX_ITERATIONS_FALLBACK = 50

### 7.2 安全
- 执行面：SafePolicy 危险命令拒绝 → 沙箱五后端 fail-closed → O_NOFOLLOW 文件 I/O
- 网络面：SSRF 防护（web_fetch）、私网拦截；serve 默认 127.0.0.1
- 凭证面：auth.json 0600；keychain 集成；BLOCKED_ENV_VARS ×18 贯穿沙箱/MCP/hooks/browser
- 工具面：ToolPolicy deny 优先；MCP schema 深度/大小上限
- API 面：三层认证中间件（公开/用户/admin）

### 7.3 可扩展性
- 单机自托管为主；多实例/多租户以 profile 隔离；fleet/swarm crate 为多机演进预留
- 新 LLM 家族：registry/ 一个模块 + model_catalog.json 一行
- 新通道：bus 适配层一个实现
- 新工具：内置 Tool trait / MCP / 技能二进制三选一

### 7.4 可观测性
- `octos doctor` 自检（配置/凭证/沙箱/本地服务发现/磁盘）
- admin 工具：system_health / system_metrics / provider_metrics / view_logs
- hooks 生命周期事件（4 事件）+ 熔断自动禁用（3 连败）
- config watcher：SHA-256 变更检测（系统提示词热加载，provider/model/hooks 需重启）

### 7.5 开发体验
- `cargo build/test/clippy/fmt --workspace`；milestone-ci.sh 为特性默认基准
- TDD：RED→GREEN→REFACTOR；单测内联、集成测试 crates/*/tests/

## 八、外部依赖与测试策略

| 依赖 | 用途 | 测试策略 |
|------|------|---------|
| LLM 提供商 API | 全部对话能力 | mock-service（录制/桩 provider）；真实调用测试 `#[ignore]` 手动触发 |
| IM 平台（17 通道） | 消息收发 | mock-service（通道适配层以假 InboundMessage 单测；端到端手动验证） |
| MCP 服务器 | 工具扩展 | mock-service（内置测试 server fixture） |
| 沙箱后端（bwrap/docker 等） | 命令隔离 | env-disable + 纯决策层矩阵单测（HostOs × Probe 全组合可跨宿主测试） |
| OAuth IdP（OpenAI 等） | 登录 | mock-service + fixed-value（本地 callback 仿真） |
| embedding provider | 记忆向量通道 | env-disable（降级 BM25-only 路径即默认测试路径） |

## 九、部署约束（交接 deployment-designer）

- **交付形态**：单二进制 `octos`（cargo install / release 二进制）；默认特性含 embed-llama（需 cmake + C++ 工具链；可 `--no-default-features --features api` 精简）
- **运行环境**：Linux / macOS / Windows；gateway/serve 建议常驻（systemd / pm2 / Docker）
- **数据目录**：`~/.octos`（config.json / sessions / episodes.redb / cron 存储），备份即目录拷贝；凭证独立存放于 XDG `~/.config/octos/auth.json`（legacy `~/.octos/auth.json` 启动时自动迁移，0600）
- **配置与密钥**：config.json + auth.json（0600）+ env vars + keychain；不入库、不入仓
- **健康检查**：`octos doctor`（CLI）、`GET /api/version`（serve）、进程存活（gateway）
- **smoke 最小链路**：init → auth status → chat -m 单轮 → serve /api/version → gateway 通道回环
- **部署环境**：当前模块 deployment_required=true（staging 环境），完整方案待 Phase 3 Step 3 deployment-designer 产出

## 十、集群部署架构视图（S16）

> 决策权威：docs/adr/cluster-state-and-execution.md。本节是架构概要的部署侧补充，不替代 ADR 条文。

### 10.1 逻辑部署单位

```mermaid
flowchart LR
  Edge["API / WS Edge"] --> PG["PostgreSQL"]
  Worker["Agent Worker"] --> PG
  Sched["Scheduler / Cron"] --> PG
  Edge --> Worker
```

第一阶段允许同一二进制角色化启动；扩缩容依据与本地状态允许范围按角色分离。

### 10.2 状态边界

| 层 | 允许 | 禁止 |
|----|------|------|
| PG | 会话 canonical、事件 seq、审批、租约、检查点、cron | — |
| Pod 本地 | 连接、缓存、加速层 | 作为跨 Pod 真相源 |
| 工作区 FS | 工具工作区（可 PVC） | 替代 PG 会话账本 |

### 10.3 与单机模式关系

- `octos chat` / `octos gateway` 保留 local adapter（JSONL/redb）
- `octos serve` 在集群配置下切换 PG 后端；对外 UI Protocol / REST 契约保持兼容

