# core-02 CLI 上手与对话 — 功能规格

> 覆盖场景：S02（开发者首次上手与认证）、S03（CLI 交互式多轮任务执行）、S15（安全策略与沙箱配置管理）
> 产品类型判断：octos 为非 GUI 主导的混合型产品（CLI + API + 对话式），CLI 是其主交互面；本规格按 CLI 工具形态组织（命令格式 / 参数表 / 输出格式 / 错误提示 / 退出码）。
> 需求来源：core-01-requirements.md（Phase 1）
> 配套原型：`core-02-cli-onboarding-terminal.md`

## 一、CLI 信息架构（命令树）

octos 是单二进制 CLI（clap 解析），共 29 个子命令，按用户目标分 6 组：

```
octos
├── 上手与诊断
│   ├── init                # 初始化 ~/.octos 配置（交互式选择提供商与模型）
│   ├── doctor              # 本地环境诊断（--json/--verbose/--strict）
│   ├── status              # 系统状态总览
│   ├── update              # 检查新版本（--check）
│   └── completions         # 生成 shell 补全
├── 认证与账户
│   ├── auth                # LLM 提供商认证：login / logout / status
│   ├── account             # 管理 profile 下的子账户
│   └── profile             # profile 便携导出（QR）与载荷检查
├── 三种运行时
│   ├── chat                # 交互式多轮对话（默认运行时）
│   ├── gateway             # 多通道常驻消息网关
│   └── serve               # REST API + Web 仪表盘（feature: api）
├── 自动化与编排
│   ├── cron                # 定时任务管理（add / list / remove ...）
│   ├── memory              # 记忆查看与 memory-refresh 流水线
│   └── goal / ledger / inbox / peer / steer   # OLP 目标与可观测面
├── 能力扩展
│   ├── skills              # 技能管理（list / install / remove）
│   ├── mcp                 # OAuth MCP 服务器 login / logout
│   ├── mcp-serve           # 作为 MCP server 对外提供工具
│   └── acp                 # 以 ACP 协议接入 Zed 等 IDE
└── 运维与管理
    ├── admin               # 租户与隧道管理
    ├── channels            # 消息通道管理
    ├── config              # 查看启动配置（show / path，只读）
    ├── cache               # 构建缓存池 status / gc / gate
    ├── clean               # 清理过期状态与缓存
    ├── docs                # 生成工具与提供商文档
    └── office              # Office 文件操作
```

全局约定：
- 数据目录默认 `~/.octos`（可用 `--data-dir` / `OCTOS_HOME` 覆盖）；配置文件 `config.json`、凭证文件 `auth.json`
- 多数命令支持 `--json` 机器可读输出；人类输出使用 ✅/⚠️/❌ 状态符号
- 错误约定：用户可修复的错误给出修复指引文案并以非 0 退出码结束；成功退出码恒为 0

## 二、S02: 开发者首次上手与认证 — 交互规格

### 2.1 `octos init`

**命令格式**：`octos init`

**交互流程**：
1. 用户运行 `octos init`
2. CLI 检查 `~/.octos/config.json` 是否已存在；已存在则进入保留已有配置的更新路径，不静默覆盖用户修改
3. 交互式引导选择 LLM 提供商（内置 preset 清单，含各 OpenAI 兼容家族与 local 家族）
4. 引导选择 API 类型（如 chat-completions / responses，仅部分家族适用）与默认模型（来自 model_catalog.json 的家族默认）
5. 检测已有 auth 凭证形态（OAuth/device_code 时订阅模型默认切换，如 OpenAI 订阅默认 gpt-5 系）
6. 写入 `~/.octos/config.json`，输出配置摘要与下一步建议（auth login / chat）

#### 验收条件（交互级）

##### 正常：首次初始化
- **GIVEN** 机器上无 `~/.octos/config.json`
- **WHEN** 用户运行 `octos init`，按提示选择提供商 `anthropic` 与默认模型
- **THEN** 终端逐项输出提供商/模型选择摘要；`~/.octos/config.json` 被创建且含所选 provider 与 model；最后一行给出下一步建议；退出码 0

##### 异常：非 TTY 环境
- **GIVEN** 在 CI 管道（stdin 非 TTY）中
- **WHEN** 用户运行 `octos init`
- **THEN** CLI 不进入交互式引导，输出需要显式参数/非交互方式的错误提示，退出码非 0，不写入半成品配置

### 2.2 `octos doctor`

**命令格式**：`octos doctor [--json] [--verbose] [--strict] [--data-dir <PATH>]`

**参数设计**：

| 参数 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| --json | flag | 否 | false | 机器可读 JSON 输出 |
| --verbose | flag | 否 | false | 输出各检查项细节 |
| --strict | flag | 否 | false | 任一检查失败时退出码非 0（供 CI 门禁） |
| --data-dir | path | 否 | ~/.octos | 指定数据目录 |

**交互流程**：
1. 用户运行 `octos doctor`
2. CLI 逐项检查：配置有效性、凭证可用性（auth store / env var）、沙箱后端决策结果（含 auto 降级告警与 fail_closed 状态）、本地 OpenAI 兼容服务发现（local 家族候选端口探测）、磁盘/会话文件健康度
3. 逐项输出 ✅/⚠️/❌ 与修复建议；末尾输出汇总

#### 验收条件（交互级）

##### 正常：环境健康
- **GIVEN** 已完成 init 且凭证齐备
- **WHEN** 用户运行 `octos doctor`
- **THEN** 各检查项输出 ✅，无 ❌；汇总行为 "ready" 语义；退出码 0

##### 异常：strict 模式缺凭证
- **GIVEN** 未配置任何 LLM 凭证
- **WHEN** 用户运行 `octos doctor --strict`
- **THEN** 凭证检查项输出 ❌ 并给出修复路径（`octos auth login` 或设置 env var）；退出码非 0

### 2.3 `octos auth`

**命令格式**：
- `octos auth login --provider <NAME> [--device-code]`
- `octos auth logout --provider <NAME>`
- `octos auth status [--json]`

**参数设计**：

| 参数 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| --provider, -p | string | 是（login/logout） | — | 提供商名（registry 家族名或别名，如 openai / anthropic） |
| --device-code | flag | 否 | false | 使用设备码流程（无浏览器环境）；默认浏览器 PKCE |
| --json | flag | 否 | false | status 的机器可读输出 |

**交互流程（浏览器 PKCE）**：
1. 用户运行 `octos auth login -p openai`
2. CLI 生成 PKCE verifier/challenge 与 state，构造授权 URL 并打开浏览器，同时本地监听 `http://localhost:1455/auth/callback`
3. 用户在浏览器完成授权；CLI 校验 state、用 code + verifier 换 token
4. 写入 `~/.octos/auth.json`（文件权限 0600），输出登录成功与凭证形态（订阅/平台 key 分类提示）

**交互流程（设备码）**：
1. 用户运行 `octos auth login -p openai --device-code`
2. CLI 请求设备码接口，显示 `user_code` 与验证 URL
3. 用户在任何设备的浏览器输入码完成授权；CLI 轮询直至成功/过期
4. 二次交换换取 token 后写入 auth.json

**凭证优先级**（status 与实际生效一致）：auth store（auth.json）→ profile env_vars（含 keychain 引用）→ 进程环境变量。

#### 验收条件（交互级）

##### 正常：设备码登录成功
- **GIVEN** 用户持有 ChatGPT 订阅，机器无浏览器
- **WHEN** 用户运行 `octos auth login -p openai --device-code`，在另一台设备完成授权
- **THEN** 终端先显示 user_code 与验证 URL；授权完成后显示登录成功及订阅 plan 提示；`octos auth status` 显示 openai 已登录（auth method 为 device_code）；auth.json 权限为 0600；退出码 0

##### 正常：status 总览
- **GIVEN** openai 走 OAuth 登录、anthropic 走 env var
- **WHEN** 用户运行 `octos auth status`
- **THEN** 输出逐提供商的认证状态与来源（auth store / env var / 未配置）；不打印完整 token 明文

##### 异常：授权超时
- **GIVEN** 用户启动设备码登录但一直未完成授权
- **WHEN** 轮询超过设备码有效期
- **THEN** CLI 提示设备码已过期，auth.json 不产生任何写入，退出码非 0

##### 异常：logout 未登录的提供商
- **GIVEN** anthropic 无任何已存凭证
- **WHEN** 用户运行 `octos auth logout -p anthropic`
- **THEN** CLI 提示该提供商当前无已存凭证，不报错崩溃，退出码非 0 或 0 但明确提示"无凭证可删"（二者取实现现状；输出文案必须明确）

## 三、S03: CLI 交互式多轮任务执行 — 交互规格

### 3.1 `octos chat`

**命令格式**：`octos chat [OPTIONS]`

**参数设计**（常用子集，全量以 `--help` 为准）：

| 参数 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| --cwd, -c | path | 否 | 当前目录 | 工作目录（会话工作区根） |
| --provider | string | 否 | config 值 | 覆盖 LLM 提供商 |
| --model | string | 否 | config 值 | 覆盖模型 |
| --base-url | string | 否 | config 值 | 自定义 API 端点 |
| --api-type | string | 否 | 家族默认 | API 协议形态（chat-completions / responses / anthropic 等，别名 --api-style） |
| --message, -m | string | 否 | — | 发送单条消息并退出（非交互模式，供脚本化） |
| --json | flag | 否 | false | 机器可读输出 |
| --verbose, -v | flag | 否 | false | 显示工具调用细节 |
| --max-iterations | int | 否 | 0（交互式=不限制） | agent loop 迭代上限 |
| --no-retry | flag | 否 | false | 关闭瞬时错误自动重试 |
| --sandbox | enum | 否 | workspace-write | read-only / workspace-write / danger-full-access |
| --ask-for-approval | enum | 否 | 配置值 | ask（危险操作前询问）/ never（边界处直接拒绝） |
| --profile | string | 否 | default | 使用指定 profile |
| --dangerously-bypass-approvals-and-sandbox | flag | 否 | false | 跳过审批与沙箱（--yolo 别名，需显式确认风险） |

**交互流程**：
1. 用户在工作区目录运行 `octos chat`，进入 REPL
2. 输入任务描述；agent loop 开始：构建消息（系统提示词 + 会话历史 + 记忆注入 + 技能 SKILL.md）→ LLM 流式输出
3. LLM 返回工具调用时，agent 展示工具名与参数摘要（--verbose 显示完整输出），经策略/沙箱执行后回注结果
4. 循环直至 LLM 给出最终回答；长会话接近 token 预算时自动 compaction（用户可见一条压缩提示）
5. 用户可用斜杠命令管理会话（如 `/new` fork 新会话并保留 parent 链）
6. Ctrl-C 中断当前轮次；exit/quit 退出，会话已持久化（JSONL）

#### 验收条件（交互级）

##### 正常：单条消息非交互模式
- **GIVEN** 已认证且在工作区目录
- **WHEN** 用户运行 `octos chat -m "用一句话总结 README.md 的前 20 行" --json`
- **THEN** agent 调用读文件工具后将摘要输出为 JSON（含内容与 token 用量字段），进程自行退出，退出码 0

##### 正常：审批提示（ask 模式）
- **GIVEN** `--ask-for-approval ask` 生效
- **WHEN** agent 准备执行被判定为有风险的命令（如写入工作区外路径）
- **THEN** 终端在执行前暂停并显示命令全文与批准选项（允许/拒绝）；用户拒绝时该命令不执行，agent 收到拒绝反馈并继续对话

##### 正常：长会话压缩可见
- **GIVEN** 会话 token 接近预算
- **WHEN** 继续对话触发 compaction
- **THEN** 终端输出一条上下文已压缩的提示；随后的回答仍记得压缩摘要中的关键结论；会话不中断

##### 异常：沙箱拒绝的提示可行动
- **GIVEN** 显式沙箱模式在当前主机不可用（如 macOS 主机配置 `mode = "landlock"`）
- **WHEN** agent 尝试执行 shell
- **THEN** 终端显示 fail-closed 拒绝原因与针对当前 OS 的修复指引（可用后端清单或关闭方式），拒绝信息中明确"未执行该命令"；agent 不反复重试同一被拒命令

##### 异常：无凭证快速失败
- **GIVEN** 无任何可用凭证
- **WHEN** 用户运行 `octos chat -m "hi"`
- **THEN** 首轮即输出缺少凭证的错误与修复路径（auth login / env var），不进入重试风暴；退出码非 0

## 四、S15: 安全策略与沙箱配置管理 — 交互规格

**配置面**（config.json，非独立子命令）：

```json
{
  "sandbox": { "enabled": true, "mode": "auto", "fail_closed": false },
  "tools": {
    "allow": ["group:fs", "group:web", "shell"],
    "deny": ["browser"],
    "byProvider": { "local": { "deny": ["web_fetch"] } }
  }
}
```

**关键配置项**：

| 配置项 | 取值 | 说明 |
|--------|------|------|
| sandbox.enabled | true / false | false = 显式选择无沙箱（与 mode="none" 同为显式豁免） |
| sandbox.mode | auto / none / bwrap / landlock / macos / appcontainer / docker | auto 自动选最佳后端；显式后端不可用时 fail-closed 拒绝 |
| sandbox.fail_closed | true / false | true 时 auto 无可用后端也从"响亮降级"升级为拒绝执行 |
| tools.allow / tools.deny | 工具名、通配（exec*）、group:fs/runtime/search/web/sessions | deny 优先；provider 级可用 byProvider 覆盖 |

**交互流程**：
1. 用户编辑 config.json 的 sandbox / tools 段
2. 运行 `octos doctor` 验证：输出沙箱决策结果（当前 OS × 配置的解析结论）与生效的工具策略摘要
3. 在 `octos chat` 中要求 agent 执行一条边界命令，观察拒绝/批准行为是否符合预期
4. 配置变更在下一轮 agent 构建时生效（config watcher 对需重启项提示重启）

#### 验收条件（交互级）

##### 正常：auto 模式决策可见
- **GIVEN** Linux 主机已安装 bwrap，配置 `sandbox.mode = "auto"`
- **WHEN** 用户运行 `octos doctor --verbose`
- **THEN** 沙箱检查项显示决策结果为 bwrap；无降级告警；退出码 0

##### 正常：fail_closed 收紧
- **GIVEN** 主机无任何可用沙箱后端，配置 `sandbox.fail_closed = true`
- **WHEN** agent 尝试执行 shell 命令
- **THEN** 命令被拒绝并提示"无可用沙箱后端且 fail_closed 已启用"； doctor 沙箱项显示该拒绝策略已生效

##### 异常：工具策略 deny 命中
- **GIVEN** 配置 `tools.deny = ["shell"]`
- **WHEN** agent 在 chat 中尝试调用 shell 工具
- **THEN** 工具规格不发送给 LLM（若 allow/deny 使其不可见）或执行被策略拒绝（provider 级策略），终端显示拒绝原因；其余工具不受影响

**原型**：`core-02-cli-onboarding-terminal.md`（含 S02 上手下线全流程、S03 对话轮次、S15 沙箱拒绝的终端模拟）
