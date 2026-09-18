#!/usr/bin/env node
// Baseline seed generator for octos (brownfield-adopter step 2/4).
// Driven via: node -e "process.argv[2]=...;import('file:///abs/path/gen-baseline.mjs')"
// Modes:
//   manifest            -> writes logos/changes/baseline-seed-plan.json
//   render <stagingDir> -> writes the two artifacts into staging
//
// Key rule (must mirror openlogos baseline-provenance.js):
//   key = "core::" + sha256(NFKC(anchor).trim().toLowerCase().collapseWs).hex[0:12]
import { createHash } from 'node:crypto';
import { writeFileSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';

const MODULE = 'core';
const norm = (a) => a.normalize('NFKC').trim().toLowerCase().replace(/\s+/g, ' ');
const key = (anchor) => `${MODULE}::${createHash('sha256').update(norm(anchor)).digest('hex').slice(0, 12)}`;

// ── Scanned inventory (verified against code on 2026-09-18) ─────────────────

// Workspace platform crates — root Cargo.toml [workspace.members]; edges from
// each crate's Cargo.toml single-line internal deps.
const CRATES = [
  ['crate:octos-core', 'octos-core — Task/Message/Error 基础类型,无内部依赖', 'crates/octos-core'],
  ['crate:octos-diagnostics', 'octos-diagnostics — 诊断支持包(doctor)', 'crates/octos-diagnostics'],
  ['crate:octos-memory', 'octos-memory — EpisodeStore(redb)/MemoryStore/HybridSearch(BM25+向量)', 'crates/octos-memory'],
  ['crate:octos-llm', 'octos-llm — LlmProvider 抽象 + 各厂商 provider + registry + failover', 'crates/octos-llm'],
  ['crate:octos-agent', 'octos-agent — Agent 循环、工具系统、沙箱、MCP、compaction、插件', 'crates/octos-agent'],
  ['crate:octos-bus', 'octos-bus — 消息总线、17 通道、会话、coalescing、cron、heartbeat', 'crates/octos-bus'],
  ['crate:octos-workflows', 'octos-workflows — 工作流编排(依赖 agent/pipeline)', 'crates/octos-workflows'],
  ['crate:octos-server', 'octos-server — 服务端运行时(聚合 agent/bus/store/services/pipeline)', 'crates/octos-server'],
  ['crate:octos-store', 'octos-store — 持久化存储(依赖 core)', 'crates/octos-store'],
  ['crate:octos-services', 'octos-services — 服务层(core/llm/bus)', 'crates/octos-services'],
  ['crate:octos-cli', 'octos-cli — CLI 二进制:clap 命令、配置加载、config watcher、api', 'crates/octos-cli'],
  ['crate:octos-dora-mcp', 'octos-dora-mcp — dora MCP 集成(依赖 agent)', 'crates/octos-dora-mcp'],
  ['crate:octos-pipeline', 'octos-pipeline — DOT 图流水线引擎(fan-out/checkpoint/human gate)', 'crates/octos-pipeline'],
  ['crate:octos-plugin', 'octos-plugin — 插件 SDK:manifest 解析、发现、门控', 'crates/octos-plugin'],
  ['crate:octos-sandbox', 'octos-sandbox — 平台沙箱助手(无内部依赖)', 'crates/octos-sandbox'],
  ['crate:octos-swarm', 'octos-swarm — swarm 协调(依赖 agent)', 'crates/octos-swarm'],
  ['crate:octos-fleet', 'octos-fleet — fleet 核心(依赖 core)', 'crates/octos-fleet'],
  ['crate:octos-fleet-worker', 'octos-fleet-worker — fleet worker(agent/core/fleet/llm/memory)', 'crates/octos-fleet-worker'],
  ['crate:octos-embed-llama', 'octos-embed-llama — 内嵌 llama(依赖 llm)', 'crates/octos-embed-llama'],
  ['crate:octos-ffi', 'octos-ffi — C FFI 绑定(core/agent/llm/memory/cli/embed-llama)', 'crates/octos-ffi'],
  ['crate:octos-uniffi', 'octos-uniffi — UniFFI 绑定(依赖 ffi)', 'crates/octos-uniffi'],
  ['crate:octos-wasm', 'octos-wasm — WASM 目标(依赖 core)', 'crates/octos-wasm'],
  ['crate:octos-pyo3', 'octos-pyo3 — Python 绑定(依赖 ffi)', 'crates/octos-pyo3'],
];

// CLI commands — crates/octos-cli/src/commands/mod.rs `enum Command` (doc comments).
const CLI = [
  ['account', '管理 profile 下的子账户'],
  ['acp', '以 ACP 协议(stdio)运行 agent(Zed 等)'],
  ['admin', '租户与隧道管理'],
  ['auth', 'LLM 提供商认证管理(login/logout/status)'],
  ['cache', '构建缓存池 status/gc/gate'],
  ['channels', '消息通道管理'],
  ['chat', '交互式多轮对话'],
  ['clean', '清理过期状态与缓存文件'],
  ['completions', '生成 shell 补全'],
  ['config', '查看已保存启动配置(show/path,只读)'],
  ['cron', '定时任务管理'],
  ['doctor', '本地环境诊断(flutter-doctor 风格)'],
  ['docs', '生成工具与提供商文档'],
  ['gateway', '以持久消息网关运行'],
  ['goal', '目标状态迁移(重开 blocked/paused、终态归档)'],
  ['init', '初始化 .octos 配置'],
  ['inbox', '查询 inbox notes 文件路径(只读,OLP 可观测)'],
  ['ledger', 'goal-ledger 只读查看(findings/escalations/decisions)'],
  ['mcp', 'OAuth MCP 服务器 login/logout'],
  ['mcp-serve', '作为 MCP server 运行,供外部编排器调用'],
  ['memory', '查看/驱动 memory-refresh 流水线'],
  ['office', 'Office 文件操作(extract/unpack/pack/clean/add-slide/validate)'],
  ['peer', '只读 peer 列表(OLP 可观测)'],
  ['profile', 'profile 便携导出(QR)与载荷检查'],
  ['serve', 'REST API server(feature: api)'],
  ['skills', '技能管理(list/install/remove)'],
  ['status', '系统状态'],
  ['steer', '向会话注入外部 reviewer steer(OLP 控制)'],
  ['update', '检查新版本(--check)'],
];

// REST route groups — crates/octos-cli/src/api/router.rs, grouped by first
// path segment; counts measured with grep on 2026-09-18 (157 unique paths).
const API = [
  ['admin', 90, '管理面:profiles/allowed-emails/monitor/ominix/platform-skills/audit 等'],
  ['my', 40, '终端用户自助面(我的会话/配置/资源)'],
  ['ui-protocol', 12, 'UI 协议传输层(仪表盘前端协议)'],
  ['auth', 10, '登录/令牌/OAuth 流'],
  ['register', 6, '注册'],
  ['preview-signed', 6, '签名预览令牌'],
  ['files', 5, '文件读写'],
  ['swarm', 4, 'swarm 协调'],
  ['tasks', 3, '任务'],
  ['site-preview', 3, '站点预览'],
  ['preview', 3, '预览'],
  ['voice', 2, '语音'],
  ['version', 2, '版本'],
  ['upload', 2, '上传'],
  ['stream', 2, '流式'],
  ['voices', 1, '声音列表'],
  ['slides', 1, '幻灯片'],
  ['site-files', 1, '站点文件'],
  ['private-asr', 1, '私有 ASR'],
  ['internal', 1, '内部'],
  ['integrations', 1, '集成'],
  ['events', 1, '事件'],
  ['cost', 1, '成本'],
];

// Channel implementations — crates/octos-bus/src/*_channel.rs.
const CHANNELS = [
  ['api', 'API 通道(serve 对外)'],
  ['cli', 'CLI 本地通道'],
  ['dingtalk', '钉钉'],
  ['discord', 'Discord'],
  ['email', 'Email(async-imap/lettre,feature-gated)'],
  ['feishu', '飞书'],
  ['line', 'LINE'],
  ['matrix', 'Matrix(appservice)'],
  ['matrix-user', 'Matrix(用户态)'],
  ['qq-bot', 'QQ 机器人'],
  ['slack', 'Slack'],
  ['telegram', 'Telegram'],
  ['twilio', 'Twilio SMS'],
  ['wechat', '微信(ilink)'],
  ['wecom', '企业微信'],
  ['wecom-bot', '企业微信机器人'],
  ['whatsapp', 'WhatsApp'],
];

// Builtin tools — ToolRegistry::with_builtins_and_permissions,
// crates/octos-agent/src/tools/registry.rs L1253-1380; names verified against
// coding_tools.rs contract tests. git=feature"git", code_structure=feature"ast".
const TOOLS = [
  ['shell', '沙箱内执行 shell 命令(SafePolicy)'],
  ['exec_command', '结构化命令执行(Codex 兼容)'],
  ['bash', 'bash 别名(与 shell/exec_command 同策略同沙箱)'],
  ['write_stdin', '向运行中进程写 stdin'],
  ['update_plan', '更新计划'],
  ['request_user_input', '请求用户输入'],
  ['ask_user_question', '结构化提问(UPCR-2026-023)'],
  ['spawn', '派生子代理(注册时联动 spawn_agent/delegate 别名)'],
  ['spawn_agent', '派生并管理子代理'],
  ['delegate', 'Codex 兼容 delegate 包装(#1172)'],
  ['send_input', '向子代理发送输入'],
  ['resume_agent', '恢复子代理'],
  ['wait_agent', '等待子代理'],
  ['close_agent', '关闭子代理'],
  ['read_file', '读文件(O_NOFOLLOW 防符号链接)'],
  ['apply_patch', '应用补丁'],
  ['diff_edit', '差异编辑'],
  ['edit_file', '编辑文件'],
  ['write_file', '写文件'],
  ['glob', 'glob 匹配'],
  ['grep', 'grep 搜索'],
  ['list_dir', '列目录'],
  ['web_search', '网页搜索'],
  ['web_fetch', '网页抓取(SSRF 防护)'],
  ['browser', '无头浏览器(CDP,feature-gated)'],
  ['check_workspace_contract', '工作区契约检查'],
  ['workspace_log', '工作区日志'],
  ['workspace_show', '工作区快照'],
  ['workspace_diff', '工作区差异'],
  ['check', '项目静态检查(#1772,共享会话沙箱)'],
  ['git', 'git 工具(feature: git)'],
  ['code_structure', '代码结构分析(feature: ast)'],
  ['view_image', '查看图片(限工作区范围)'],
  ['tool_search', '工具检索(#972,活目录)'],
  ['tool_suggest', '工具建议(#972)'],
  ['image_generation', '图像生成(#1149,当前返回未绑定后端的类型化错误)'],
];

// Admin tools — crates/octos-agent/src/tools/admin/mod.rs L144-175.
const ADMIN_TOOLS = [
  ['list_profiles', '列出 profiles'],
  ['profile_status', 'profile 状态'],
  ['start_profile', '启动 profile'],
  ['stop_profile', '停止 profile'],
  ['restart_profile', '重启 profile'],
  ['enable_profile', '启用 profile'],
  ['update_profile', '更新 profile'],
  ['view_logs', '查看日志'],
  ['system_health', '系统健康'],
  ['system_metrics', '系统指标'],
  ['provider_metrics', '提供商指标'],
  ['manage_watchdog', '看门狗管理'],
  ['view_sessions', '查看会话'],
  ['cron_status', 'cron 状态'],
  ['check_config', '配置检查'],
  ['list_sub_accounts', '列出子账户'],
  ['create_sub_account', '创建子账户'],
  ['manage_skills', '技能管理(管理面)'],
  ['platform_skills', '平台技能管理'],
  ['update_octos', '更新 octos'],
];

const SYS_PATH = 'logos/resources/prd/3-technical-plan/1-architecture/core-system-map.md';
const SCN_PATH = 'logos/resources/prd/3-technical-plan/2-scenario-implementation/core-scenario-candidates.md';

const sysCandidates = CRATES.map(([anchor, display]) => ({
  key: key(anchor), anchor, display, state: 'active', verified: false, aliases: [], superseded_by: [],
}));
const scnEntries = [
  ...CLI.map(([n, d]) => [`cli:${n}`, `${n} — ${d}`, 'crates/octos-cli/src/commands/mod.rs enum Command']),
  ...API.map(([g, c, d]) => [`api:${g}`, `/api/${g}/* (${c} 条路由) — ${d}`, 'crates/octos-cli/src/api/router.rs']),
  ...CHANNELS.map(([n, d]) => [`channel:${n}`, `${n} — ${d}`, 'crates/octos-bus/src/']),
  ...TOOLS.map(([n, d]) => [`tool:${n}`, `${n} — ${d}`, 'crates/octos-agent/src/tools/registry.rs with_builtins_and_permissions']),
  ...ADMIN_TOOLS.map(([n, d]) => [`tool:admin/${n}`, `${n} — ${d}`, 'crates/octos-agent/src/tools/admin/mod.rs']),
];
const scnCandidates = scnEntries.map(([anchor, display]) => ({
  key: key(anchor), anchor, display, state: 'active', verified: false, aliases: [], superseded_by: [],
}));

const yamlCandidates = (cands) =>
  'candidates:\n' + cands.map((c) =>
    `  - key: "${c.key}"\n    anchor: "${c.anchor}"\n    display: "${c.display.replace(/"/g, '\\"')}"\n    state: active\n    verified: false\n    aliases: []\n    superseded_by: []`
  ).join('\n');

const PROV = '> provenance: reverse-engineered · verified: false · seed pass 2026-09-18 · 非权威意图,仅为现状快照';

const sysDoc = `# core 系统结构图(逆向种子基线)

${PROV}

> 本文件由 brownfield-adopter 逆向扫描生成:内容全部可从代码核验,但未经人工确认(\`verified: false\` 恒成立)。

## 总览

octos 是 Rust 工作区(2024 edition),根 \`Cargo.toml\` 声明 39 个 member:23 个平台 crate + 14 个 app-skills + 1 个 platform-skill(voice)+ 4 个 harness-starter(harness-starter-{generic,report,audio,coding},计入 app-skills 合计 14 个)。主二进制为 \`octos\`(crates/octos-cli)。

## 分层(边均核验自各 crate Cargo.toml 内部依赖声明)

\`\`\`
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
\`\`\`

## 平台 crate 清单(23)

| crate | 路径 | 职责(依据) |
|---|---|---|
${CRATES.map(([a, d, p]) => `| \`${a.replace('crate:', '')}\` | \`${p}\` | ${d} |`).join('\n')}

## 技能 crate(插件二进制协议:\`./binary <tool>\`,JSON stdin/stdout)

- app-skills(14): news, deep-search, deep-crawl, send-email, account-manager, time, weather, smart-home, wechat-bridge, skill-evolve, harness-starter-{generic,report,audio,coding}
- platform-skills(1): voice

## 关键入口

- CLI 入口:\`crates/octos-cli/src/main.rs\` → clap \`Command\` 枚举(29 个子命令,见场景候选清单)
- REST 入口:\`crates/octos-cli/src/api/router.rs\`(157 条唯一路由路径,grep 实测;其余 api/*.rs 含少量附加路由与测试引用)
- Agent 循环:\`crates/octos-agent/src/agent.rs\`(构建消息→LLM+工具规格→工具执行→压缩)
- 工具注册:\`crates/octos-agent/src/tools/registry.rs\` \`with_builtins_and_permissions\`(L1253-1380)
- 通道实现:\`crates/octos-bus/src/*_channel.rs\`(17 个)

## 备注(本次未逐项核验)

- 依赖边覆盖各 Cargo.toml 中的内部依赖声明;个别 crate 若使用多行写法可能遗漏边。
- feature-gated 项:\`serve\`(api)、\`browser\`、\`git\` 工具、\`code_structure\`(ast)、email 通道等。
- 运行时数据目录约定 \`~/.octos\`(config.json/auth.json/sessions),未在本 seed 轮次逐文件核验。

## 逆向基线来源

\`\`\`yaml
${yamlCandidates(sysCandidates)}
\`\`\`
`;

const scnDoc = `# core 场景候选清单(逆向种子基线)

${PROV}

> 候选 = 用户可触达的入口点(CLI 子命令 / REST 路由组 / 消息通道 / 工具),均核验自代码注册点。
> 仅为候选,不是已建模场景;后续 scenario-architect 在此基础上合并/拆分/命名 S 编号。

## A. CLI 命令场景(29)— 来源:\`crates/octos-cli/src/commands/mod.rs\` \`enum Command\`

| anchor | 说明 |
|---|---|
${CLI.map(([n, d]) => `| \`cli:${n}\` | ${d} |`).join('\n')}

## B. REST API 路由组(23 组 / 157 条)— 来源:\`crates/octos-cli/src/api/router.rs\`(grep 实测计数)

| anchor | 路由数 | 说明 |
|---|---|---|
${API.map(([g, c, d]) => `| \`api:${g}\` | ${c} | ${d} |`).join('\n')}

## C. 消息通道(17)— 来源:\`crates/octos-bus/src/*_channel.rs\`

| anchor | 说明 |
|---|---|
${CHANNELS.map(([n, d]) => `| \`channel:${n}\` | ${d} |`).join('\n')}

## D. 内置工具(36)— 来源:\`tools/registry.rs\` \`with_builtins_and_permissions\`(L1253-1380)

| anchor | 说明 |
|---|---|
${TOOLS.map(([n, d]) => `| \`tool:${n}\` | ${d} |`).join('\n')}

## E. 管理员工具(20)— 来源:\`tools/admin/mod.rs\`(L144-175)

| anchor | 说明 |
|---|---|
${ADMIN_TOOLS.map(([n, d]) => `| \`tool:admin/${n}\` | ${d} |`).join('\n')}

## 逆向基线来源

\`\`\`yaml
${yamlCandidates(scnCandidates)}
\`\`\`
`;

const manifest = {
  module: MODULE,
  expected: [
    { kind: 'system-map', target_path: SYS_PATH, candidate_keys: sysCandidates.map((c) => c.key) },
    { kind: 'scenario-candidates', target_path: SCN_PATH, candidate_keys: scnCandidates.map((c) => c.key) },
  ],
};

const mode = process.argv[2];
if (mode === 'manifest') {
  writeFileSync('logos/changes/baseline-seed-plan.json', JSON.stringify(manifest, null, 2) + '\n');
  console.log(`manifest written: system-map=${sysCandidates.length} candidates, scenario-candidates=${scnCandidates.length} candidates`);
} else if (mode === 'render') {
  const staging = process.argv[3];
  if (!staging) { console.error('usage: render <stagingDir>'); process.exit(1); }
  for (const [p, doc] of [[SYS_PATH, sysDoc], [SCN_PATH, scnDoc]]) {
    const abs = join(staging, p);
    mkdirSync(dirname(abs), { recursive: true });
    writeFileSync(abs, doc);
    console.log(`staged: ${p} (${doc.length} bytes)`);
  }
} else {
  console.error('usage: gen-baseline.mjs manifest|render <stagingDir>');
  process.exit(1);
}
