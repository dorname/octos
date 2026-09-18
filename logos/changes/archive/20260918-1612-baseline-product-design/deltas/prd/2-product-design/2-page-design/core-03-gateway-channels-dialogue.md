# Delta: prd/2-product-design/2-page-design — core-03-gateway-channels-dialogue.md

> target: logos/resources/prd/2-product-design/2-page-design/core-03-gateway-channels-dialogue.md(全新文档)

## ADDED — core-03 网关与通道 对话脚本原型

# core-03 网关与通道 — 对话脚本原型

> 配套规格：`core-03-gateway-channels-design.md`（S04 / S06 / S14）
> 形式：IM 对话脚本（以 Telegram 为例，其他通道行为一致、仅长度上限与渲染格式不同）。

## 脚本 1：S04 群内问答（require_mention，正常路径）

> 前提：bot 已入群，`require_mention = true`。

```
群成员 A: 今天 CI 又红了
bot:       （无反应 —— 未 @，消息不入会话、不消耗模型调用）

群成员 A: @octos-bot 帮我看看 today 分支最近一次失败是什么原因
bot:       ▶ 正在处理…
           （agent 调用 shell/git 工具查询 CI 日志）
bot:       失败在 integration-tests 阶段的 `auth_refresh_test`：
           token 刷新接口返回 401，与 9 月 17 日的 OAuth 端点变更相关。
           建议先重跑确认是否瞬时问题，若稳定复现需检查 refresh 流程。
```

## 脚本 2：S04 长回复自动分片（正常路径）

```
群成员 B: @octos-bot 把上周的变更整理成周报
bot:       ▶ 正在处理…（汇总 37 个提交）
bot:       【片 1/3】📋 本周变更周报（09-08 ~ 09-14）
           一、概览
           本周共 37 个提交，主要围绕认证链路与网关稳定性……
           （≤ 4000 字符，按段落边界切断）
bot:       【片 2/3】二、分类明细
           feat: 12 个 · fix: 9 个 · docs: 6 个 ……
bot:       【片 3/3】三、下周建议
           1. 完成 OAuth 设备码文档补齐 ……
```

## 脚本 3：S04 会话命令（/new fork，正常路径）

```
群成员 B: /new
bot:       ✓ 已开启新会话（topic: t2，父会话: default）。
           当前会话历史从此开始，输入 /back 可返回上一会话。

群成员 B: @octos-bot 这个会话我们只讨论发布计划
bot:       好的，本会话聚焦发布计划。当前上下文已与日常问答隔离。

群成员 B: /back
bot:       ✓ 已返回会话 default（topic t2 已保留，可 /s 查看全部）。
```

## 脚本 4：S04 多用户隔离（正常路径）

```
群成员 A: @octos-bot 我的 API key 前缀是 sk-a1…，帮我记下
bot:       已记录在当前会话上下文中。（安全提示：敏感凭证建议走
           octos auth login 或 keychain，不建议留在聊天记录）

群成员 C: @octos-bot 刚才 A 让你记的 key 前缀是什么？
bot:       我看不到其他会话的内容——每个群成员/聊天都有独立会话。
           你可以告诉我你自己的 key 前缀，我帮你记录。
```

## 脚本 5：S06 定时任务推送（正常路径）

> 前提：管理员已执行
> `octos cron add --name daily-summary --message "汇总昨日 git 提交" --cron "0 9 * * *" --deliver --channel telegram --to -1001234567890`

```
[次日 09:00]
bot:       ⏰ 定时任务 daily-summary
           📋 昨日提交汇总（09-17，共 14 个提交）
           - feat(auth): ChatGPT 订阅 OAuth 登录路由 Codex 后端
           - fix(init): 去除 preset 重复项 ……
           ✅ 全部 CI 通过。
```

## 脚本 6：S06 定时任务执行失败（异常路径）

```
[次日 09:00，主备提供商均持续 429]
bot:       ⏰ 定时任务 daily-summary
           ❌ 本次执行失败：LLM 提供商链全部不可用（主: 429 限流 / 备: 429 限流）。
           调度不受影响，下次触发：09-20 09:00。
           管理员可运行 octos cron list 查看状态，或检查提供商配额。
```

## 脚本 7：S04 通道故障隔离（异常路径，管理视角）

> gateway 日志（管理员在服务器查看，非 IM 内）：

```console
[2026-09-18 09:00:01] INFO  channel telegram: polling started
[2026-09-18 09:00:01] ERROR channel discord: unauthorized (401) — token 已失效，进入退避重试（下次 30s 后）
[2026-09-18 09:00:02] INFO  channel telegram: 正常处理中（discord 故障不影响本通道）
```
