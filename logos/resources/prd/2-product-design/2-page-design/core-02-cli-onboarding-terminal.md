# core-02 CLI 上手与对话 — 终端交互原型

> 配套规格：`core-02-cli-onboarding-design.md`（S02 / S03 / S15）
> 形式：终端交互模拟。输出文本为示意（表情符号与措辞以实现为准），交互结构、退出码与关键提示语以功能规格为准。

## 原型 1：S02 全新环境上手（正常路径）

```console
$ octos init
octos init — 首次配置

? 选择 LLM 提供商:
  ❯ anthropic
    openai
    gemini
    openrouter
    local (llama.cpp / lmstudio / openai-compatible)
    ...

? 选择默认模型: ❯ claude-sonnet-5
? API key 来源: ❯ 稍后使用 octos auth login（推荐）

✓ 已写入 ~/.octos/config.json
  provider = anthropic
  model    = claude-sonnet-5

下一步:
  1. octos doctor        # 检查环境
  2. octos auth login    # 配置凭证
  3. octos chat          # 开始对话

$ echo $?
0

$ octos doctor
octos doctor — 环境诊断

  ✅ 配置文件        ~/.octos/config.json 有效
  ❌ 凭证            anthropic: 未找到 API key
                     → 修复: octos auth login -p anthropic 或设置 ANTHROPIC_API_KEY
  ✅ 沙箱            auto → bwrap (Linux)
  ⚠️  local 服务     未发现 127.0.0.1:8080 的本地模型服务（如不使用 local 家族可忽略）
  ✅ 数据目录        ~/.octos 可写，磁盘余量充足

1 项需要处理，0 项警告。按提示修复后重试。

$ octos auth login -p anthropic
? 选择认证方式:
  ❯ 粘贴 API token
    （该提供商不支持 OAuth）

请粘贴 anthropic 的 API key（输入不回显）:
✓ 已保存到 ~/.octos/auth.json (权限 0600)

$ octos auth status
provider    status     source       method
anthropic   ✅ 已认证   auth store   paste_token
openai      — 未配置

$ octos doctor --strict
  ✅ 配置文件 / 凭证 / 沙箱 / 数据目录 全部通过
$ echo $?
0
```

## 原型 2：S02 设备码登录（无浏览器环境，正常路径）

```console
$ octos auth login -p openai --device-code
正在请求设备码...

  请在任意设备的浏览器打开:  https://chatgpt.com/codex/device
  并输入授权码:  ABCD-EFGH

等待授权中...（约 15 分钟有效）

✓ 授权成功
  凭证类型: ChatGPT 订阅 (plan: plus)
  注意: 订阅凭证将路由到 Codex 后端（chatgpt.com/backend-api/codex），
        仅支持订阅模型（gpt-5* / codex*）。

$ octos auth status
provider    status     source       method       note
openai      ✅ 已认证   auth store   device_code  ChatGPT 订阅 → Codex 后端
```

## 原型 3：S02 异常 — 未认证发起对话

```console
$ octos chat -m "hi"
Error: 未找到 anthropic 的 API 凭证。
  → 运行 octos auth login -p anthropic
  → 或设置环境变量 ANTHROPIC_API_KEY
未发起模型请求。
$ echo $?
1
```

## 原型 4：S03 交互式任务（正常路径，含工具调用与审批）

```console
$ octos chat
octos chat (anthropic / claude-sonnet-5) — 输入 /help 查看命令，/new 开新会话

➜ 创建一个 hello.py 打印当前时间并运行它

● 我来创建并运行这个脚本。

  ▶ write_file  hello.py  (86 B)
  ▶ shell       python3 hello.py   [sandbox: bwrap]

  输出: 2026-09-18 15:42:07

✅ 完成。hello.py 已创建并通过沙箱运行成功。

➜ 把 /etc/hosts 复制到当前目录

● shell  cp /etc/hosts ./hosts-copy
  ⚠️ 该命令需要读取工作区外文件，审批策略为 ask。
  允许执行? [y/N] n
  已拒绝。我不会执行这条命令。如需读取工作区外文件，
  请以 --sandbox workspace-write 配合显式批准，或将文件放入工作区。

➜ /new
✓ 已 fork 新会话（父会话: cli:local:default#a1b2，可 /back 返回）

➜ exit
会话已保存 (~/.octos/sessions/)。
```

## 原型 5：S03 异常 — 显式沙箱不可用（fail-closed）

```console
$ octos chat --sandbox workspace-write
➜ 跑一下 pytest

  ▶ shell  pytest -q
  ❌ 沙箱不可用（fail-closed）：
     配置的显式后端 landlock 在当前主机不可用（需要 Linux 5.13+ 且内核启用 Landlock）。
     该命令未被执行。
     修复建议（macOS）:
       - sandbox.mode = "macos"（sandbox-exec）
       - 或 sandbox.mode = "docker"
       - 或 sandbox.mode = "auto" 自动选择
       - 显式豁免: sandbox.enabled = false（不推荐）

● 执行被沙箱策略拒绝，我没有运行 pytest。你可以按上述建议调整后重试。
```

## 原型 6：S03 长会话压缩可见（正常路径）

```console
➜ （第 38 轮对话，上下文接近预算）
⋯ 上下文已自动压缩：早期 24 轮已摘要，最近 6 轮与工具结果完整保留。

➜ 刚才那个测试文件为什么用 tmp_path？

● 因为第 12 轮我们约定测试不污染工作区（该结论保留在压缩摘要中）……
```

## 原型 7：S15 沙箱决策验证（doctor --verbose）

```console
$ octos doctor --verbose
  ...
  ✅ 沙箱
     配置: mode=auto, fail_closed=false, enabled=true
     决策: bwrap（Linux 6.8，/usr/bin/bwrap 可用）
     候选: landlock(内核支持) → bwrap(可用) → docker(未检测到守护进程)
     工具策略: allow=[group:fs, group:web, shell], deny=[browser]
```
