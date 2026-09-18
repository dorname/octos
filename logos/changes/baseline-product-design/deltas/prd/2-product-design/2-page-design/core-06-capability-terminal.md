# Delta: prd/2-product-design/2-page-design — core-06-capability-terminal.md

> target: logos/resources/prd/2-product-design/2-page-design/core-06-capability-terminal.md(全新文档)

## ADDED — core-06 能力扩展与韧性 终端交互原型

# core-06 能力扩展与韧性 — 终端交互原型

> 配套规格：`core-06-capability-design.md`（S08 / S09 / S10 / S12）
> 形式：终端交互模拟。

## 原型 1：S08 记忆检索与 refresh（正常路径）

```console
$ octos memory search "OAuth 刷新" --limit 5
检索模式: hybrid (vector 0.7 + bm25 0.3)

1. [episode] 2026-09-17 · 修复 OAuth 设备码刷新
   "chatgpt-oauth-codex：device code 流适配新 deviceauth JSON API，刷新合并凭证…"
2. [note] 2026-09-16 · 决策记录
   "订阅 token 只对 Codex 后端有效，不做 api.openai.com 回退…"

$ octos memory refresh --dry-run
memory-refresh（干跑）:
  提取 pass: 3 个会话 / 5 个 episode 待处理
  合并 pass: 2 条将写入 MEMORY.md，4 条将写入 daily/2026-09-18.md
  （未实际写入）

$ octos memory refresh
✓ 已提取 5 条经验，合并 6 条到长期记忆。
```

## 原型 2：S08 无 embedding 降级（异常但可用）

```console
$ octos memory search "部署"
检索模式: bm25-only（未配置 embedding provider，向量通道已降级）

1. [note] 2026-09-12 · 部署约定
   "staging 部署需人工确认…"
```

## 原型 3：S09 技能安装与调用（正常路径）

```console
$ octos skills install weather
✓ 已安装 weather@1.2.0
  工具: weather_current, weather_forecast
  SKILL.md 已注册（将注入系统提示词）

$ octos skills list
name       version  tools  status
weather    1.2.0    2      ✅ 可用
smart-home 0.9.4    5      ❌ 不可用（缺 env: SMART_HOME_TOKEN）

$ octos chat
➜ 北京明天天气怎么样
  ▶ weather_forecast  {"city": "北京", "days": 1}
● 北京明天晴，12–22℃，北风 3 级。
```

## 原型 4：S09 spawn_only 后台执行（正常路径）

```console
➜ 用 podcast 技能把这篇周报生成播客

  ▶ podcast_generate  {"source": "docs/weekly/2026-W38.md"}
  ⏏ 任务已转入后台执行（task: bg-7f3a），完成后我会把结果发给你。

➜ （继续聊别的）
● …（几轮后）
📬 后台任务 bg-7f3a 已完成：podcast-2026-W38.mp3（3m12s），已保存到工作区。
```

## 原型 5：S10 MCP 注册与 schema 拒绝（正常+异常）

```console
$ octos chat
启动时日志:
  ✓ mcp server "fs-tools": 握手成功，注册 6 个工具
  ⚠ mcp server "legacy-svc": 工具 "deep_query" schema 深度 12 超过上限 10，已拒绝注册（其余 3 个工具可用）

➜ 用 fs-tools 列出 ~/notes 下的 markdown 文件
  ▶ mcp:fs-tools.list_files  {"path": "~/notes", "glob": "*.md"}
● 共 14 个文件：…
```

## 原型 6：S12 故障转移（正常路径，--verbose 视角）

```console
$ octos chat -v
➜ 继续昨天的重构

  ⚠ anthropic: 429 rate limited，退避重试 (1s)…
  ⚠ anthropic: 429 rate limited，退避重试 (2s)…
  ↪ 切换到备用提供商 openai（gpt-5）

● （回答正常返回，用户无感）
```

## 原型 7：S12 全链路失败（异常路径）

```console
➜ 继续昨天的重构

  ✗ anthropic: 429 quota exceeded（已重试 3 次）
  ✗ openai: connection refused（base_url 不可达）

Error: 所有 LLM 提供商均不可用。
  → 检查 anthropic 配额或稍后重试
  → 检查 openai 的 base-url 配置（当前: http://127.0.0.1:9000/v1）
```
