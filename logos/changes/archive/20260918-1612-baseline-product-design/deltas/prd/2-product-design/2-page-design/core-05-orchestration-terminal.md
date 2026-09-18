# Delta: prd/2-product-design/2-page-design — core-05-orchestration-terminal.md

> target: logos/resources/prd/2-product-design/2-page-design/core-05-orchestration-terminal.md(全新文档)

## ADDED — core-05 编排与自动化 终端交互原型

# core-05 编排与自动化 — 终端交互原型

> 配套规格：`core-05-orchestration-design.md`（S07 / S11）
> 形式：终端交互模拟。节点进度与工具输出为示意结构，字段以实现为准。

## 原型 1：S07 端到端流水线（正常路径）

```console
$ octos chat
➜ 运行 pipelines/release_notes.dot 这条流水线

● 我来运行 release_notes 流水线。

  ▶ run_pipeline  release_notes.dot
  ┌─ pipeline: release_notes
  │  ✓ 校验通过（5 节点 / 4 边）
  │  ▶ collect  [model: claude-sonnet-5]  完成 (12.4k tokens)
  │  ▶ fanout   [parallel ×3 workers]
  │      ├ worker-1 审阅 feat 提交   完成
  │      ├ worker-2 审阅 fix 提交    完成
  │      └ worker-3 审阅 docs 提交   完成
  │  ⏸ gate     [human_gate] 等待负责人确认（已发送到 telegram: 负责人会话）
  │
  │  （负责人在 Telegram 回复"通过"）
  │  ▶ gate     已确认（by @lead, 2026-09-18 16:02）
  │  ▶ publish  [model: claude-sonnet-5]  完成 (8.1k tokens)
  └─ ✅ pipeline 完成

  PipelineResult:
    总耗时: 3m42s · tokens: 47.2k
    节点摘要: collect=汇总 37 提交 / fanout=3 路审阅通过 / gate=人工通过 / publish=周报已生成
    修改文件: docs/weekly/2026-W38.md

✅ 周报已生成到 docs/weekly/2026-W38.md。
```

## 原型 2：S07 校验前置拦截（异常路径）

```console
➜ 运行 pipelines/broken.dot

  ▶ run_pipeline  broken.dot
  ❌ 校验失败：
     节点 "gate" 声明了 human_gate 但未提供 resolver 属性。
     human_gate 节点必须显式声明 resolver（如 resolver="channel"）。
  未启动任何节点执行。

● 流水线定义有一个校验错误，需要补上 gate 节点的 resolver 属性再运行。
```

## 原型 3：S07 断点续跑（正常路径）

```console
➜ （上次执行在 fanout 后进程被 kill）重新运行 release_notes 流水线

  ▶ run_pipeline  release_notes.dot  [--resume]
  ┌─ pipeline: release_notes
  │  ⏭ collect  checkpoint 命中，跳过
  │  ⏭ fanout   checkpoint 命中，跳过（3/3 worker 结果已落盘）
  │  ▶ gate     [human_gate] 等待负责人确认…
  │  ▶ publish  完成 (8.3k tokens)
  └─ ✅ pipeline 完成（本次新增 8.3k tokens，已完成节点未重复扣费）
```

## 原型 4：S11 并行子代理（正常路径）

```console
➜ 并行审查 crates/octos-agent、crates/octos-bus、crates/octos-llm 里的 unsafe 用法并汇总

● 我把三个模块分给三个子代理并行审查。

  ▶ spawn_agent  agent-1  "审查 octos-agent unsafe 用法"
  ▶ spawn_agent  agent-2  "审查 octos-bus unsafe 用法"
  ▶ spawn_agent  agent-3  "审查 octos-llm unsafe 用法"
  ⋯ 子代理后台运行中（wait_agent 收集结果）
  ✓ agent-2 完成（0 处 unsafe；deny(unsafe_code) 已工作区级启用）
  ✓ agent-1 完成（0 处 unsafe）
  ✓ agent-3 完成（0 处 unsafe）

● 汇总：三个 crate 均无 unsafe 代码，与工作区级 `#![deny(unsafe_code)]` lint 一致。
  - octos-agent: 0 处
  - octos-bus: 0 处
  - octos-llm: 0 处
```

## 原型 5：S11 子代理失败回注（异常路径）

```console
  ✓ agent-1 完成
  ✗ agent-2 失败：提供商链全部不可用（主 429 / 备 5xx）
  ✓ agent-3 完成

● 汇总（2/3 完成）：
  - octos-agent: 0 处 unsafe
  - octos-bus: ⚠️ 未完成审查（子代理因提供商限流失败，可稍后重试该模块）
  - octos-llm: 0 处 unsafe
```
