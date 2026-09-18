# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo build --workspace          # Build all crates
cargo test --workspace           # Run all tests
cargo test -p octos-agent         # Test single crate
cargo test -p octos-agent test_name  # Run single test
cargo clippy --workspace         # Lint
cargo fmt --all                  # Format
cargo fmt --all -- --check       # Check formatting
cargo install --path crates/octos-cli \
    --features "api,telegram,discord,whatsapp,feishu,twilio,wecom,wecom-bot,audio_mp3"
                                          # Install CLI locally with the
                                          # canonical feature default
                                          # (matches scripts/milestone-ci.sh).
                                          # Default features include
                                          # `embed-llama` (the bundled
                                          # llama.cpp embedder): building
                                          # needs cmake + a C++ toolchain;
                                          # add `embed-llama-metal` on Apple
                                          # Silicon, or use
                                          # `--no-default-features --features api`
                                          # to skip it.
                                          # `api` is required for `octos serve`.
                                          # `audio_mp3` is required for the
                                          # `podcast_generate` workspace
                                          # contract to validate mp3 output
                                          # (see #1025).
```

## Architecture

octos is a Rust-native, API-first Agentic OS — multi-tenant AI agent platform. 8-crate workspace + bundled skills, layered:

```
octos-cli  (CLI: clap commands, config loading, config watcher)
    |
octos-agent  (Agent loop, tool system, sandbox, MCP, compaction, plugins)
    |          \
octos-memory   octos-llm  (hybrid search + memory store | LLM providers)
    \           /
    octos-core  (Task, Message, Error types, truncate_utf8 - no internal deps)
```

Alongside octos-agent:
- **octos-bus**: Message bus, 14 channels (Telegram/Discord/Slack/WhatsApp/Email/WeChat/...), sessions, coalescing, cron, heartbeat
- **octos-pipeline**: DOT-graph pipeline engine — per-node model selection, parallel fan-out, checkpoints, human gates
- **octos-plugin**: Plugin SDK — manifest parsing, discovery, gating (binary/env/OS checks)

Bundled skills in `crates/app-skills/` (weather, time, news, deep-search, etc.) and `crates/platform-skills/` (voice).

Commands: chat, init, status, gateway, serve, clean, completions, cron, channels, auth (login/logout/status), skills (list/install/remove).

Three runtime modes: `octos chat` (interactive CLI), `octos gateway` (multi-channel), `octos serve` (web dashboard + 91 REST endpoints).

Auth module (`octos-cli/src/auth/`): OAuth PKCE + device code for OpenAI, paste-token for others. Stored in `~/.octos/auth.json`. `config.rs` checks auth store before env vars.

### Key Flow: Agent Loop (`octos-agent/src/agent.rs`)

1. Build messages (system prompt + conversation history + memory context)
2. Call LLM with tool specs (filtered by ToolPolicy + provider policy)
3. If tool calls returned -> execute tools -> append results -> loop
4. If EndTurn or budget exceeded -> return result
5. Context compaction kicks in when token budget fills (`compaction.rs`)

### Tool System (`octos-agent/src/tools/`)

All tools implement `Tool` trait (`spec() -> ToolSpec`, `execute(&Value) -> ToolResult`). Registered in `ToolRegistry` (HashMap). Tools: shell, read_file, write_file, edit_file, glob, grep, list_dir, web_search, web_fetch, message, spawn, cron, browser (feature-gated). Tool argument size limit: 1MB (non-allocating `estimate_json_size` with escape accounting). File tools use `O_NOFOLLOW` (Unix) for symlink-safe I/O. Shared SSRF protection in `tools/ssrf.rs`.

**Tool Policies** (`tools/policy.rs`): Allow/deny lists with deny-wins semantics, wildcard matching (`exec*`), and named groups (`group:fs`, `group:runtime`, `group:search`, `group:web`, `group:sessions`). Provider-specific policies via `tools.byProvider` in config.

### Sandbox (`octos-agent/src/sandbox/`)

Five sandbox backends: `Bwrap` (Linux), `Landlock` (Linux, octos-sandbox helper), `Macos` (sandbox-exec), `AppContainer` (Windows, octos-sandbox.exe helper), `Docker` (any OS). Resolution is a pure decision layer (`decide_sandbox` over `HostOs` + `HostBackendProbe` — every platform's matrix unit-tested from any host): an explicit mode that cannot be honored on this host FAILS CLOSED with a typed `SandboxUnavailable` refusal (`RefusingSandbox` — every command refuses with per-OS remediation; never a silent `NoSandbox`, never a blind ENOENT backend). `SandboxMode::Auto` picks the best available backend; with none it degrades to `NoSandbox` loudly (warned once per process, surfaced by `octos doctor`) unless `sandbox.fail_closed = true` (default false) turns the degradation into a refusal. `enabled = false` / `mode = "none"` remain the explicit unconfined opt-outs and beat `fail_closed`. Shared `BLOCKED_ENV_VARS` constant (18 env vars) across all backends and MCP server spawning. Docker supports mount modes (none/ro/rw), resource limits (CPU/memory/PIDs), network isolation. Path validation rejects injection characters (`:`, `\0`, `\n`, `\r` for Docker; control chars, `(`, `)`, `\`, `"` for macOS SBPL).

### MCP (`octos-agent/src/mcp.rs`)

JSON-RPC stdio transport for MCP servers. Env var sanitization via shared `BLOCKED_ENV_VARS`. Input schema validation: max depth 10, max size 64KB — tools with invalid schemas are rejected at registration.

### Context Compaction (`octos-agent/src/compaction.rs`)

Token-aware message compaction: estimates tokens, strips tool arguments, summarizes to first lines, preserves recent tool call/result pairs.

### LLM Providers (`octos-llm/src/`)

`LlmProvider` trait with `chat()` method. Four native providers: `AnthropicProvider`, `OpenAIProvider`, `GeminiProvider`, `OpenRouterProvider`. OpenAI-compatible families via `with_base_url()`, registered in `registry/` (one module per family + one line in `ALL`; `model_catalog.json` is the SSOT for model names/defaults and gates onboarding visibility). The unified `local` family (aliases: llamacpp/llama.cpp/llama-server/lmstudio/openai-compatible) covers any local OpenAI-compatible server — keyless, zero-config default `http://127.0.0.1:8080/v1`; `local_discovery.rs` holds candidate ports + `/v1/models` parsing, used by `octos doctor`. 3-layer failover: `RetryProvider` (exponential backoff on 429/5xx) → `ProviderChain` → `AdaptiveRouter` (hedge racing, lane scoring, circuit breakers).

### Plugin System (`octos-agent/src/plugins/`, `octos-plugin/`)

Skills are self-contained binaries with `manifest.json` declarations. Binary protocol: `./skill_binary <tool_name>` with JSON on stdin, JSON `{success, output, files_to_send}` on stdout. Discovery scans directories with precedence rules. Gating checks binary existence, env, and OS requirements.

**spawn_only tools**: Manifest field `spawn_only: true` marks tools for background execution. Auto-intercepted in the execution loop — wrapped in `tokio::spawn`, returns immediately. No LLM cooperation needed. SKILL.md auto-injected as system prompt for skills with spawn_only tools.

### Pipeline Engine (`octos-pipeline/`)

DOT-graph based multi-step agent workflows. Per-node model selection via `ModelStylesheet`. Parallel fan-out spawns N concurrent workers at runtime. Includes artifact store, checkpoints, condition evaluation, human gates. `PipelineResult` tracks output, token usage, per-node summaries, modified files.

### Tool Visibility (RFC-0, #1289)

All enabled tools are sent to the LLM every turn (full schema) — there is no LRU-by-recency deferral, no `activate_tools` meta-tool, and no config-driven group deferral. `spawn_only` tools still auto-redirect to background execution but remain visible in `specs()`. `internal_hidden` tools (mofa_make dispatcher targets) remain hidden from `specs()` but callable internally via `get()`. `provider_policy` and `context_filter` are the only remaining `specs()` filters.

### Memory (`octos-memory/src/`)

- `EpisodeStore`: redb database at `.octos/episodes.redb`, stores task completion summaries
- `MemoryStore`: Long-term memory (MEMORY.md), daily notes, recent memories (7-day window)
- `HybridSearch`: BM25 + vector (cosine similarity) hybrid ranking with HNSW index (`hnsw_rs`). Configurable weights via `with_weights()` (default 0.7 vector / 0.3 BM25). Named HNSW constants. BM25 epsilon prevents NaN. Falls back to BM25-only without embedding provider.

### Message Coalescing (`octos-bus/src/coalesce.rs`)

Splits long messages into channel-safe chunks (paragraph > newline > sentence > space > hard cut). Per-channel limits. MAX_CHUNKS (50) DoS limit. UTF-8 safe boundary detection.

### Session Management (`octos-bus/src/session.rs`)

JSONL persistence with LRU in-memory cache. Session forking (`/new` command) with parent_key tracking. Percent-encoded filenames with hash suffix on truncation (prevents collisions). File size limit: 10MB. Atomic write-then-rename for crash safety.

### Hooks (`octos-agent/src/hooks.rs`)

Lifecycle hook system for running shell commands at agent events. 4 events: `before_tool_call`, `after_tool_call`, `before_llm_call`, `after_llm_call`. Before-hooks can deny operations (exit code 1). Shell protocol: JSON payload on stdin, exit code semantics (0=allow, 1=deny, 2+=error). Circuit breaker auto-disables hooks after 3 consecutive failures (configurable via `HookExecutor::with_threshold()`). Commands use argv array (no shell interpretation). Environment sanitized via shared `BLOCKED_ENV_VARS`. Tilde expansion supports `~/` and `~username/`. Config: `hooks` array in config.json with `event`, `command`, `timeout_ms` (default 5000), `tool_filter`. Wired in chat.rs, gateway.rs, serve.rs. Hook changes trigger restart via config_watcher.

### Config Hot-Reload (`octos-cli/src/config_watcher.rs`)

SHA-256 hash-based change detection. Hot-reload for system prompt; restart-required for provider/model/hooks changes.

## Key Types

- `Task` (octos-core): UUID v7 ID, kind (Code/Plan/Review/Custom), status, context
- `Message` (octos-core): role (System/User/Assistant/Tool), content, tool_call_id. `MessageRole` has `as_str()` and `Display` impl.
- `ChatResponse` (octos-llm): content, tool_calls, stop_reason, token usage
- `AgentConfig` (octos-agent): max_iterations (default 0 = unlimited for interactive chat/ACP; unattended gateway/session actors fall back to `UNATTENDED_MAX_ITERATIONS_FALLBACK` = 50 when `gateway.max_iterations` is unset), max_tokens, save_episodes
- `truncate_utf8`/`truncated_utf8` (octos-core): Shared UTF-8 safe string truncation (in-place and copying variants)

## TDD - Test Driven Development

All code changes follow the RED -> GREEN -> REFACTOR cycle. See `.claude/rules/tdd.md` for full details.

- **New features/bug fixes**: Write a failing test first, then implement
- **Unit tests**: Inline `#[cfg(test)]` modules in the same file
- **Integration tests**: `crates/*/tests/` directory, `#[ignore]` for tests needing external services
- **Verify**: `cargo test -p <crate> <test_name>` after each step, full suite before done
- **Naming**: `should_<expected>_when_<condition>`

## Project Conventions

- Edition 2024, rust-version 1.85.0
- Pure Rust TLS via rustls (no OpenSSL dependency)
- `eyre`/`color-eyre` for error handling (not `anyhow`)
- `Arc<dyn Trait>` for shared providers/tools/reporters
- `AtomicBool` for shutdown signaling (Release on store, Acquire on load)
- API keys from env vars via `api_key_env` or OAuth via `octos auth login`
- Email channel feature-gated: `async-imap` + `lettre` + `mailparse`
- Browser tool feature-gated: headless Chrome via CDP over `tokio-tungstenite` + `which`
- `ShellTool` has `SafePolicy` that denies dangerous commands (rm -rf /, dd, mkfs, fork bomb). Whitespace-normalized before matching. Timeout clamped to [1, 600]s.
- `BLOCKED_ENV_VARS` shared across sandbox backends, MCP, hooks, and browser tool (18 vars: LD_PRELOAD, DYLD_*, NODE_OPTIONS, etc.)
- Shared SSRF protection (`tools/ssrf.rs`): blocks private IPs, IPv6 ULA/link-local, IPv4-mapped/compatible addresses
- Symlink-safe file I/O via `O_NOFOLLOW` on Unix (eliminates TOCTOU races); symlink-check fallback on Windows
- Cross-platform: shell via `cmd /C` on Windows, `sh -c` on Unix; process kill via `taskkill` on Windows, `kill` signals on Unix; `where` on Windows, `which` on Unix for binary discovery
- Plugin skills use binary protocol: `./binary <tool_name>` with JSON stdin/stdout
- `deny(unsafe_code)` workspace-wide lint
- API server (`octos serve`) binds to 127.0.0.1 by default (`--host` to override)

<!-- OPENLOGOS:BEGIN -->
# AI Assistant Instructions

This project follows the **OpenLogos** methodology.
Read `logos/logos-project.yaml` first to understand the project resource index.

## Project Context
- Config: `logos/logos.config.json`
- Resource Index: `logos/logos-project.yaml`

## ⚠️ 语言策略（最高优先级）

本项目的文档语言为 **中文**（配置于 `logos/logos.config.json` → `locale: "zh"`）。

**你的所有输出——包括生成的文档、代码注释、回复消息——必须使用中文。**
即使 Skill 文件使用其他语言编写，你的输出也必须是中文。
违反此规则将导致产出不可用。

## Methodology Rules
1. Never write code without first completing the design documents
2. Follow the Why → What → How progression
3. All API designs must originate from scenario sequence diagrams
4. All code changes must have corresponding API orchestration tests
5. Use the Delta change workflow for iterations (see logos/changes/ directory)
6. All generated test code must include an OpenLogos reporter (see logos/spec/test-results.md)

## Interaction Guidelines
When the user's request is vague or they ask "what should I do next":
1. Scan `logos/resources/` to determine the current project phase
2. Suggest the specific next step based on what's missing
3. Provide a ready-to-use prompt the user can directly say
4. Never start generating documents without confirming key information

Phase 检测逻辑（检测到对应阶段时，**必须先读取** Skill 文件并按其步骤执行）：
- `logos/resources/prd/1-product-requirements/` 为空 → Phase 1 → **读取 `logos/skills/prd-writer/SKILL.md` 并按其步骤执行**
- 需求存在但 `2-product-design/` 为空 → Phase 2 → **读取 `logos/skills/product-designer/SKILL.md` 并按其步骤执行**
- 设计存在但 `3-technical-plan/1-architecture/` 为空 → Phase 3 Step 0 → **读取 `logos/skills/architecture-designer/SKILL.md` 并按其步骤执行**
- 架构存在但 `3-technical-plan/2-scenario-implementation/` 为空 → Phase 3 Step 1 → **读取 `logos/skills/scenario-architect/SKILL.md` 并按其步骤执行**
- 场景存在但 `logos/resources/api/` 为空 → Phase 3 Step 2 → **读取 `logos/skills/api-designer/SKILL.md` 和 `logos/skills/db-designer/SKILL.md` 并按其步骤执行**
- API / DB 设计完成后但 `3-technical-plan/3-deployment/` 为空 → Phase 3 Step 3 → **读取 `logos/skills/deployment-designer/SKILL.md` 并按其步骤执行**
- 部署方案存在但 `logos/resources/test/` 为空 → Phase 3 Step 4a → **读取 `logos/skills/test-writer/SKILL.md` 并按其步骤执行**（如需部署需同时设计 smoke）
- 测试用例存在但 `logos/resources/scenario/` 为空 → Phase 3 Step 4b → **读取 `logos/skills/test-orchestrator/SKILL.md` 并按其步骤执行**（仅 API 项目）
- 编排测试存在但 `logos/resources/implementation/` 为空 → Phase 3 Step 5 → **读取 `logos/skills/code-implementor/SKILL.md` 并按其步骤执行**（完成后可用 `logos/skills/code-reviewer/SKILL.md` 进行代码审查）
- 代码已生成但 `logos/resources/verify/acceptance-report.md` 不存在 → Phase 3 Step 6（运行测试后 `openlogos verify`）
- 部署完成但 `smoke-report.md` / `SMOKE_PASS` 缺失 → Phase 3 Step 8（`openlogos smoke`，人类确认点）

文件命名规范（模块前缀）：
- 所有设计文档遵循 `<module>-<序号>-<类型>.md` 格式，初始项目默认使用 `core-` 前缀
- 场景实现文件：`<module>-SXX-<slug>.md`（如 `core-S01-cli-init.md`）
- 测试用例文件：`<module>-SXX-test-cases.md`（如 `core-S01-test-cases.md`）
- 场景编号全局唯一，由 `logos-project.yaml` 的 `scenario_counter.next_id` 维护，严禁不同模块从 S01 重新开始
- 多模块状态：`openlogos status` 聚合展示所有模块（in-progress 置顶）；`openlogos next` 单模块直接建议，多模块并列列出，无 in-progress 时提示 `module add`

Step 5 执行规则（大任务）：
1. 大任务可按场景/子模块分批实现，但每一批必须闭环
2. 每一批必须同时包含：业务代码 + UT/ST 测试代码 + OpenLogos reporter
3. 输出代码前，先列出本批覆盖的 UT/ST 用例 ID，并确保与 `logos/resources/test/*.md` 对齐
4. 不允许将全部测试推迟到最终批次统一补写

Step 5 分批执行提示词（可直接复用）：
- `请按 Phase 3 Step 5 执行本次实现。若任务较大可分批，但每批必须同时交付：（1）业务代码，（2）对应 UT/ST 测试代码，（3）写入 logos/resources/verify/test-results.jsonl 的 OpenLogos reporter。输出代码前请先列出本批覆盖的 UT/ST 用例 ID。`

## 文档修改后的验证（强制）

每次**写入或修改** Markdown / 文本类规格文档（例如 `logos/resources/`、`logos/changes/`、`logos/spec/` 或项目根 `spec/` 下的 `.md`，以及根目录 `AGENTS.md` / `CLAUDE.md`）后：

1. **必须**用当前环境可用的方式**从磁盘重新读取**本次修改涉及的片段（例如 Read 工具、或终端 `sed` / `rg`），向用户展示**文件中的实际原文**（可省略无关段落并标注 `...`）。
2. **禁止**仅以自然语言概括「已改为……」作为唯一交付物，而不附带可对照的原文佐证。
3. **例外**：纯 typo 或单字符标点修改时，至少读回**受影响的那一行**，或展示等价的 diff 片段。

**目的**：避免工具声称已保存、但实际未落盘或路径错误导致内容「丢失」而不自知。


## Active Skills
**重要**：当你识别到当前 Phase 后，必须先读取对应的 Skill 文件（使用上方 Phase 检测逻辑中指定的路径），按 Skill 中定义的步骤逐步执行。不要跳过 Skill 文件直接生成内容。

### OpenLogos 方法论 Skills
- `logos/skills/project-init/SKILL.md` — 项目初始化与结构搭建
- `logos/skills/prd-writer/SKILL.md` — 需求文档编写
- `logos/skills/product-designer/SKILL.md` — 产品设计与原型
- `logos/skills/ui-ux-pro-max/SKILL.md` — UI/UX 设计智能（67 风格 / 96 调色板 / 57 字体配对 / 25 图表 / 13 技术栈）。Phase 2 处理 GUI 类产品（Web / Mobile / Desktop）设计时由 product-designer 自动调用。
- `logos/skills/architecture-designer/SKILL.md` — 技术架构与技术选型
- `logos/skills/scenario-architect/SKILL.md` — 业务场景建模与时序图
- `logos/skills/api-designer/SKILL.md` — OpenAPI 规格设计
- `logos/skills/db-designer/SKILL.md` — 数据库 Schema 设计
- `logos/skills/deployment-designer/SKILL.md` — 部署方案与 smoke 策略设计（Step 3）
- `logos/skills/test-writer/SKILL.md` — 单元测试 + 场景测试用例设计（Step 4a）
- `logos/skills/test-orchestrator/SKILL.md` — API 编排测试设计（Step 4b，仅 API 项目）
- `logos/skills/code-implementor/SKILL.md` — 基于规格链的代码与测试代码生成（Step 5）
- `logos/skills/code-reviewer/SKILL.md` — 代码审查与规范检查
- `logos/skills/change-writer/SKILL.md` — 变更提案编写与影响分析
- `logos/skills/slice-planner/SKILL.md` — merge 后 [code] 切片规划：六维打分 + 垂直/横向判别器 + 删后续证伪门（launched 变更下 [code] 切片的唯一事实源）
- `logos/skills/deployment-executor/SKILL.md` — verify 通过后的人类确认部署执行
- `logos/skills/merge-executor/SKILL.md` — 通过 MERGE_PROMPT.md 执行 Delta 合并

### 项目专属 Skills
- `.claude/skills/<skill>/SKILL.md` 中的项目技能保持项目归属，不会进入 OpenLogos 官方插件或 `logos/skills/`；如存在，请按项目语义单独调用和维护。

## ⛔ 变更管理（强制执行）

### Guard 机制
本项目使用 `logos/.openlogos-guard` 锁文件来追踪活跃变更。
- **有 guard 文件** → 可以修改代码，但 **只能在该提案范围内** 修改
- **无 guard 文件** → **禁止修改任何源代码**，必须先运行 `openlogos change <slug>`

### 变更流程
1. 运行 `openlogos change <slug>` 创建提案（自动写入 guard 文件）
2. 使用 change-writer Skill 填写 `proposal.md` + `tasks.md`
3. **等待用户确认后** 再开始产出 delta
4. delta 产出完成后提醒用户明确授权运行 `openlogos merge <slug>`
5. merge 完成后 AI 自动 commit 规格文档（告知用户，无需确认）
6. 按合并后的规格实现代码，完成后 AI 自动 commit 代码（告知用户，无需确认）
7. 提醒用户明确授权运行 `openlogos verify` 验收
8. 如存在 `[deploy]` section，验收通过后提醒用户明确授权 AI 按部署方案执行部署
9. 部署完成后提醒用户明确授权运行 `openlogos smoke`
10. verify 通过且无部署任务，或部署完成且 smoke 通过后，提醒用户明确授权运行 `openlogos archive <slug>`（自动删除 guard 文件）
11. archive 完成后 AI 自动 commit 归档（告知用户，无需确认）
12. 提醒用户确认是否执行 `git push`（人类确认点）

**两档授权语义（半自动 / 全自动）：**
- **半自动 / 手动模式（默认，无 `--auto`）**：`openlogos merge`、`openlogos verify`、部署执行、`openlogos smoke`、`openlogos archive` 和 `git push` 是人类确认点。AI 未经用户明确授权不得自行执行；用户明确要求执行（包括使用对应 slash command）时，AI 可以代为执行。不得在"顺手完成流程"、"按流程走完"等隐式场景中自动触发。
- **全自动 / 无人值守模式（`openlogos next --auto`）**：用户选择 `--auto` 即构成对该提案全链路的 standing 授权——AI driver 被授权**自动执行** verify、部署、smoke、archive、git push（以及可跳 flow 门 plan/spec/slice/deliver），无需逐步人类确认。`git push` 无需额外机制（PreToolUse guard 安全白名单本就放行）。
- **硬红线（任何模式、含 `--auto` 都不放行）**：达迭代上限仍未过测试的未收敛代码（`gate:implement:loop-exhausted`）——全自动也照常阻塞，**绝不放行未通过测试的代码**。

### 行为约束
- **发现 bug/问题时**：只输出分析和修复方案，**禁止直接修改代码**，等待用户决定是否创建变更提案
- **修改代码前**：先确认 guard 文件存在且当前修改在提案范围内
- **唯一例外**：纯 typo 修复（不改变语义）、`.gitignore`/`README.md` 等非方法论文件

**违反此规则将破坏项目的变更可追溯性。**

## ⚠️ openlogos CLI 规则

运行任何 `openlogos` 命令之前，**必须先 cd 到项目根目录**（即 `logos/logos.config.json` 所在目录）。
在子目录（如 `src/`、`src-tauri/`）下直接运行会导致 `logos.config.json not found` 错误。

正确写法：
```bash
cd <项目根目录> && openlogos <command>
```

## Conventions
- 遵循 OpenLogos 三层推进模型（Why → What → How）
- 每次变更必须先创建 logos/changes/ 变更提案
<!-- OPENLOGOS:END -->
