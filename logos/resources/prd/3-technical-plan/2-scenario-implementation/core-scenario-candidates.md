# core 场景候选清单(逆向种子基线)

> provenance: reverse-engineered · verified: false · seed pass 2026-09-18 · 非权威意图,仅为现状快照

> 候选 = 用户可触达的入口点(CLI 子命令 / REST 路由组 / 消息通道 / 工具),均核验自代码注册点。
> 仅为候选,不是已建模场景;后续 scenario-architect 在此基础上合并/拆分/命名 S 编号。

## A. CLI 命令场景(29)— 来源:`crates/octos-cli/src/commands/mod.rs` `enum Command`

| anchor | 说明 |
|---|---|
| `cli:account` | 管理 profile 下的子账户 |
| `cli:acp` | 以 ACP 协议(stdio)运行 agent(Zed 等) |
| `cli:admin` | 租户与隧道管理 |
| `cli:auth` | LLM 提供商认证管理(login/logout/status) |
| `cli:cache` | 构建缓存池 status/gc/gate |
| `cli:channels` | 消息通道管理 |
| `cli:chat` | 交互式多轮对话 |
| `cli:clean` | 清理过期状态与缓存文件 |
| `cli:completions` | 生成 shell 补全 |
| `cli:config` | 查看已保存启动配置(show/path,只读) |
| `cli:cron` | 定时任务管理 |
| `cli:doctor` | 本地环境诊断(flutter-doctor 风格) |
| `cli:docs` | 生成工具与提供商文档 |
| `cli:gateway` | 以持久消息网关运行 |
| `cli:goal` | 目标状态迁移(重开 blocked/paused、终态归档) |
| `cli:init` | 初始化 .octos 配置 |
| `cli:inbox` | 查询 inbox notes 文件路径(只读,OLP 可观测) |
| `cli:ledger` | goal-ledger 只读查看(findings/escalations/decisions) |
| `cli:mcp` | OAuth MCP 服务器 login/logout |
| `cli:mcp-serve` | 作为 MCP server 运行,供外部编排器调用 |
| `cli:memory` | 查看/驱动 memory-refresh 流水线 |
| `cli:office` | Office 文件操作(extract/unpack/pack/clean/add-slide/validate) |
| `cli:peer` | 只读 peer 列表(OLP 可观测) |
| `cli:profile` | profile 便携导出(QR)与载荷检查 |
| `cli:serve` | REST API server(feature: api) |
| `cli:skills` | 技能管理(list/install/remove) |
| `cli:status` | 系统状态 |
| `cli:steer` | 向会话注入外部 reviewer steer(OLP 控制) |
| `cli:update` | 检查新版本(--check) |

## B. REST API 路由组(23 组 / 157 条)— 来源:`crates/octos-cli/src/api/router.rs`(grep 实测计数)

| anchor | 路由数 | 说明 |
|---|---|---|
| `api:admin` | 90 | 管理面:profiles/allowed-emails/monitor/ominix/platform-skills/audit 等 |
| `api:my` | 40 | 终端用户自助面(我的会话/配置/资源) |
| `api:ui-protocol` | 12 | UI 协议传输层(仪表盘前端协议) |
| `api:auth` | 10 | 登录/令牌/OAuth 流 |
| `api:register` | 6 | 注册 |
| `api:preview-signed` | 6 | 签名预览令牌 |
| `api:files` | 5 | 文件读写 |
| `api:swarm` | 4 | swarm 协调 |
| `api:tasks` | 3 | 任务 |
| `api:site-preview` | 3 | 站点预览 |
| `api:preview` | 3 | 预览 |
| `api:voice` | 2 | 语音 |
| `api:version` | 2 | 版本 |
| `api:upload` | 2 | 上传 |
| `api:stream` | 2 | 流式 |
| `api:voices` | 1 | 声音列表 |
| `api:slides` | 1 | 幻灯片 |
| `api:site-files` | 1 | 站点文件 |
| `api:private-asr` | 1 | 私有 ASR |
| `api:internal` | 1 | 内部 |
| `api:integrations` | 1 | 集成 |
| `api:events` | 1 | 事件 |
| `api:cost` | 1 | 成本 |

## C. 消息通道(17)— 来源:`crates/octos-bus/src/*_channel.rs`

| anchor | 说明 |
|---|---|
| `channel:api` | API 通道(serve 对外) |
| `channel:cli` | CLI 本地通道 |
| `channel:dingtalk` | 钉钉 |
| `channel:discord` | Discord |
| `channel:email` | Email(async-imap/lettre,feature-gated) |
| `channel:feishu` | 飞书 |
| `channel:line` | LINE |
| `channel:matrix` | Matrix(appservice) |
| `channel:matrix-user` | Matrix(用户态) |
| `channel:qq-bot` | QQ 机器人 |
| `channel:slack` | Slack |
| `channel:telegram` | Telegram |
| `channel:twilio` | Twilio SMS |
| `channel:wechat` | 微信(ilink) |
| `channel:wecom` | 企业微信 |
| `channel:wecom-bot` | 企业微信机器人 |
| `channel:whatsapp` | WhatsApp |

## D. 内置工具(36)— 来源:`tools/registry.rs` `with_builtins_and_permissions`(L1253-1380)

| anchor | 说明 |
|---|---|
| `tool:shell` | 沙箱内执行 shell 命令(SafePolicy) |
| `tool:exec_command` | 结构化命令执行(Codex 兼容) |
| `tool:bash` | bash 别名(与 shell/exec_command 同策略同沙箱) |
| `tool:write_stdin` | 向运行中进程写 stdin |
| `tool:update_plan` | 更新计划 |
| `tool:request_user_input` | 请求用户输入 |
| `tool:ask_user_question` | 结构化提问(UPCR-2026-023) |
| `tool:spawn` | 派生子代理(注册时联动 spawn_agent/delegate 别名) |
| `tool:spawn_agent` | 派生并管理子代理 |
| `tool:delegate` | Codex 兼容 delegate 包装(#1172) |
| `tool:send_input` | 向子代理发送输入 |
| `tool:resume_agent` | 恢复子代理 |
| `tool:wait_agent` | 等待子代理 |
| `tool:close_agent` | 关闭子代理 |
| `tool:read_file` | 读文件(O_NOFOLLOW 防符号链接) |
| `tool:apply_patch` | 应用补丁 |
| `tool:diff_edit` | 差异编辑 |
| `tool:edit_file` | 编辑文件 |
| `tool:write_file` | 写文件 |
| `tool:glob` | glob 匹配 |
| `tool:grep` | grep 搜索 |
| `tool:list_dir` | 列目录 |
| `tool:web_search` | 网页搜索 |
| `tool:web_fetch` | 网页抓取(SSRF 防护) |
| `tool:browser` | 无头浏览器(CDP,feature-gated) |
| `tool:check_workspace_contract` | 工作区契约检查 |
| `tool:workspace_log` | 工作区日志 |
| `tool:workspace_show` | 工作区快照 |
| `tool:workspace_diff` | 工作区差异 |
| `tool:check` | 项目静态检查(#1772,共享会话沙箱) |
| `tool:git` | git 工具(feature: git) |
| `tool:code_structure` | 代码结构分析(feature: ast) |
| `tool:view_image` | 查看图片(限工作区范围) |
| `tool:tool_search` | 工具检索(#972,活目录) |
| `tool:tool_suggest` | 工具建议(#972) |
| `tool:image_generation` | 图像生成(#1149,当前返回未绑定后端的类型化错误) |

## E. 管理员工具(20)— 来源:`tools/admin/mod.rs`(L144-175)

| anchor | 说明 |
|---|---|
| `tool:admin/list_profiles` | 列出 profiles |
| `tool:admin/profile_status` | profile 状态 |
| `tool:admin/start_profile` | 启动 profile |
| `tool:admin/stop_profile` | 停止 profile |
| `tool:admin/restart_profile` | 重启 profile |
| `tool:admin/enable_profile` | 启用 profile |
| `tool:admin/update_profile` | 更新 profile |
| `tool:admin/view_logs` | 查看日志 |
| `tool:admin/system_health` | 系统健康 |
| `tool:admin/system_metrics` | 系统指标 |
| `tool:admin/provider_metrics` | 提供商指标 |
| `tool:admin/manage_watchdog` | 看门狗管理 |
| `tool:admin/view_sessions` | 查看会话 |
| `tool:admin/cron_status` | cron 状态 |
| `tool:admin/check_config` | 配置检查 |
| `tool:admin/list_sub_accounts` | 列出子账户 |
| `tool:admin/create_sub_account` | 创建子账户 |
| `tool:admin/manage_skills` | 技能管理(管理面) |
| `tool:admin/platform_skills` | 平台技能管理 |
| `tool:admin/update_octos` | 更新 octos |

## 逆向基线来源
```yaml
candidates:
  - key: core::0059c03d9827
    anchor: channel:discord
    display: discord — Discord
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::00bad4ad6d07
    anchor: tool:image_generation
    display: image_generation — 图像生成(#1149,当前返回未绑定后端的类型化错误)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::00bc2fb82345
    anchor: tool:update_plan
    display: update_plan — 更新计划
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::00f71c201f0e
    anchor: api:swarm
    display: /api/swarm/* (4 条路由) — swarm 协调
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::01ec9ab64e1a
    anchor: tool:admin/system_metrics
    display: system_metrics — 系统指标
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::02038f611fc2
    anchor: tool:admin/update_octos
    display: update_octos — 更新 octos
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::020fdd789825
    anchor: tool:code_structure
    display: "code_structure — 代码结构分析(feature: ast)"
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::05d6d36129ad
    anchor: tool:resume_agent
    display: resume_agent — 恢复子代理
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::066ee8317dbc
    anchor: api:ui-protocol
    display: /api/ui-protocol/* (12 条路由) — UI 协议传输层(仪表盘前端协议)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::07eed3a43f73
    anchor: tool:wait_agent
    display: wait_agent — 等待子代理
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::0b943390b77a
    anchor: channel:qq-bot
    display: qq-bot — QQ 机器人
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::10b4cf26996c
    anchor: api:integrations
    display: /api/integrations/* (1 条路由) — 集成
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::1b09760c2b1c
    anchor: cli:serve
    display: "serve — REST API server(feature: api)"
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::1beb1c4e1da4
    anchor: tool:admin/profile_status
    display: profile_status — profile 状态
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::1dccba9200ae
    anchor: tool:request_user_input
    display: request_user_input — 请求用户输入
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::1effb860b663
    anchor: tool:tool_search
    display: tool_search — 工具检索(#972,活目录)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::1f0134b74cdf
    anchor: cli:office
    display: office — Office 文件操作(extract/unpack/pack/clean/add-slide/validate)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::20a0fdebf48f
    anchor: tool:edit_file
    display: edit_file — 编辑文件
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::26388161db5c
    anchor: cli:peer
    display: peer — 只读 peer 列表(OLP 可观测)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::28a17d224667
    anchor: cli:channels
    display: channels — 消息通道管理
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::28a6749ebef8
    anchor: tool:view_image
    display: view_image — 查看图片(限工作区范围)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::29f2acfb4139
    anchor: tool:read_file
    display: read_file — 读文件(O_NOFOLLOW 防符号链接)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::2ad6c5448334
    anchor: cli:completions
    display: completions — 生成 shell 补全
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::2d62ce6813e8
    anchor: tool:tool_suggest
    display: tool_suggest — 工具建议(#972)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::2e48d2664779
    anchor: api:cost
    display: /api/cost/* (1 条路由) — 成本
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::3026d538a080
    anchor: channel:wecom
    display: wecom — 企业微信
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::320d2704ec40
    anchor: tool:spawn
    display: spawn — 派生子代理(注册时联动 spawn_agent/delegate 别名)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::373ed9dd1246
    anchor: api:slides
    display: /api/slides/* (1 条路由) — 幻灯片
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::38c179446bf8
    anchor: tool:admin/list_sub_accounts
    display: list_sub_accounts — 列出子账户
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::3a8926517670
    anchor: tool:admin/cron_status
    display: cron_status — cron 状态
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::3c282cdd9e75
    anchor: tool:admin/update_profile
    display: update_profile — 更新 profile
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::3c512545d22a
    anchor: channel:cli
    display: cli — CLI 本地通道
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::3ccf292fba55
    anchor: tool:admin/start_profile
    display: start_profile — 启动 profile
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::3d675ac33fec
    anchor: tool:admin/restart_profile
    display: restart_profile — 重启 profile
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::3f609477f9a2
    anchor: cli:update
    display: update — 检查新版本(--check)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::42b543059e92
    anchor: channel:wechat
    display: wechat — 微信(ilink)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::43c115afbd3d
    anchor: tool:shell
    display: shell — 沙箱内执行 shell 命令(SafePolicy)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::4789ed01f61a
    anchor: tool:write_file
    display: write_file — 写文件
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::480bed7963fb
    anchor: api:upload
    display: /api/upload/* (2 条路由) — 上传
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::4f1db44aaf2a
    anchor: tool:bash
    display: bash — bash 别名(与 shell/exec_command 同策略同沙箱)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::5019df90d7ca
    anchor: cli:gateway
    display: gateway — 以持久消息网关运行
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::5705f6f4a328
    anchor: tool:apply_patch
    display: apply_patch — 应用补丁
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::5940266c4f9a
    anchor: tool:admin/platform_skills
    display: platform_skills — 平台技能管理
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::59b2efc1bd13
    anchor: api:stream
    display: /api/stream/* (2 条路由) — 流式
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::5c7a654cfe73
    anchor: tool:admin/stop_profile
    display: stop_profile — 停止 profile
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::66ae706fb60e
    anchor: cli:cache
    display: cache — 构建缓存池 status/gc/gate
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::66f06165415e
    anchor: api:events
    display: /api/events/* (1 条路由) — 事件
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::6a744edd1b30
    anchor: channel:api
    display: api — API 通道(serve 对外)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::6b7a46c9e545
    anchor: tool:admin/manage_watchdog
    display: manage_watchdog — 看门狗管理
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::71e5e7bac340
    anchor: channel:telegram
    display: telegram — Telegram
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::75a4cdaad41b
    anchor: channel:whatsapp
    display: whatsapp — WhatsApp
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::76bc58885d9c
    anchor: tool:delegate
    display: delegate — Codex 兼容 delegate 包装(#1172)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::77b6f3dfa901
    anchor: cli:init
    display: init — 初始化 .octos 配置
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::77c1aac5429a
    anchor: tool:close_agent
    display: close_agent — 关闭子代理
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::7939562649d1
    anchor: channel:feishu
    display: feishu — 飞书
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::7e32bbb91540
    anchor: api:register
    display: /api/register/* (6 条路由) — 注册
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::7ee87f61a596
    anchor: channel:wecom-bot
    display: wecom-bot — 企业微信机器人
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::85bcd2d69864
    anchor: channel:dingtalk
    display: dingtalk — 钉钉
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::85fe5d47de8d
    anchor: tool:workspace_log
    display: workspace_log — 工作区日志
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::872513d3b058
    anchor: channel:twilio
    display: twilio — Twilio SMS
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::883abf79e9bf
    anchor: cli:config
    display: config — 查看已保存启动配置(show/path,只读)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::8a3c87812b4f
    anchor: tool:exec_command
    display: exec_command — 结构化命令执行(Codex 兼容)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::8a40893d5596
    anchor: tool:check
    display: check — 项目静态检查(#1772,共享会话沙箱)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::8aed0bf2a3c3
    anchor: tool:glob
    display: glob — glob 匹配
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::8f8a6802836a
    anchor: cli:docs
    display: docs — 生成工具与提供商文档
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::93cbf5c645a7
    anchor: tool:ask_user_question
    display: ask_user_question — 结构化提问(UPCR-2026-023)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::977d821025d1
    anchor: tool:admin/manage_skills
    display: manage_skills — 技能管理(管理面)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::97f0639b528c
    anchor: tool:admin/system_health
    display: system_health — 系统健康
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::9959bcdc1a5f
    anchor: tool:admin/check_config
    display: check_config — 配置检查
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::9a7c36760020
    anchor: cli:doctor
    display: doctor — 本地环境诊断(flutter-doctor 风格)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::9adc0200158e
    anchor: tool:admin/provider_metrics
    display: provider_metrics — 提供商指标
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::9b49fb3e64b5
    anchor: cli:admin
    display: admin — 租户与隧道管理
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::9d70cadcff06
    anchor: cli:memory
    display: memory — 查看/驱动 memory-refresh 流水线
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::9ef3f59f135a
    anchor: api:preview
    display: /api/preview/* (3 条路由) — 预览
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::a30e4bdf2fa2
    anchor: tool:browser
    display: browser — 无头浏览器(CDP,feature-gated)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::a3134ff9b52b
    anchor: channel:line
    display: line — LINE
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::a6e72bc2f101
    anchor: channel:slack
    display: slack — Slack
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::a89735b93983
    anchor: tool:admin/enable_profile
    display: enable_profile — 启用 profile
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::a8c94c000ce3
    anchor: api:internal
    display: /api/internal/* (1 条路由) — 内部
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::a9f4add5879f
    anchor: cli:clean
    display: clean — 清理过期状态与缓存文件
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::ae7fa530a636
    anchor: api:tasks
    display: /api/tasks/* (3 条路由) — 任务
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::b1075c9af57e
    anchor: tool:send_input
    display: send_input — 向子代理发送输入
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::b4178837caee
    anchor: api:files
    display: /api/files/* (5 条路由) — 文件读写
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::b56b7e396e26
    anchor: tool:workspace_show
    display: workspace_show — 工作区快照
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::b65ee426c6a3
    anchor: cli:goal
    display: goal — 目标状态迁移(重开 blocked/paused、终态归档)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::b68587596def
    anchor: channel:matrix-user
    display: matrix-user — Matrix(用户态)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::bb828a0495df
    anchor: api:site-files
    display: /api/site-files/* (1 条路由) — 站点文件
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::bc52fc908217
    anchor: cli:skills
    display: skills — 技能管理(list/install/remove)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::bf6a2e9ff894
    anchor: tool:admin/view_logs
    display: view_logs — 查看日志
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::c0f2b273fa67
    anchor: cli:mcp-serve
    display: mcp-serve — 作为 MCP server 运行,供外部编排器调用
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::c2be04227d96
    anchor: tool:admin/create_sub_account
    display: create_sub_account — 创建子账户
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::c2c4c3cf6fca
    anchor: cli:acp
    display: acp — 以 ACP 协议(stdio)运行 agent(Zed 等)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::c69d79aa346e
    anchor: channel:matrix
    display: matrix — Matrix(appservice)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::c72cefcb4b4c
    anchor: api:private-asr
    display: /api/private-asr/* (1 条路由) — 私有 ASR
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::c90f42576683
    anchor: tool:write_stdin
    display: write_stdin — 向运行中进程写 stdin
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::c9b0080673f0
    anchor: tool:workspace_diff
    display: workspace_diff — 工作区差异
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::ca7fd9c414b3
    anchor: cli:ledger
    display: ledger — goal-ledger 只读查看(findings/escalations/decisions)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::cbaaaa85c16c
    anchor: tool:list_dir
    display: list_dir — 列目录
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::ce6dcf3a5644
    anchor: cli:inbox
    display: inbox — 查询 inbox notes 文件路径(只读,OLP 可观测)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::d1920a4a5e11
    anchor: cli:steer
    display: steer — 向会话注入外部 reviewer steer(OLP 控制)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::d1b56a275c3a
    anchor: tool:web_search
    display: web_search — 网页搜索
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::d3710d637c19
    anchor: cli:chat
    display: chat — 交互式多轮对话
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::d49acd850026
    anchor: tool:spawn_agent
    display: spawn_agent — 派生并管理子代理
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::d4b975cec7e6
    anchor: tool:admin/view_sessions
    display: view_sessions — 查看会话
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::d89c02434daa
    anchor: cli:mcp
    display: mcp — OAuth MCP 服务器 login/logout
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::da0992ba20be
    anchor: api:site-preview
    display: /api/site-preview/* (3 条路由) — 站点预览
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::dbb9d072ee24
    anchor: channel:email
    display: email — Email(async-imap/lettre,feature-gated)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::e06bb9cd2340
    anchor: cli:cron
    display: cron — 定时任务管理
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::e435716228cd
    anchor: api:version
    display: /api/version/* (2 条路由) — 版本
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::e528a8453fed
    anchor: api:preview-signed
    display: /api/preview-signed/* (6 条路由) — 签名预览令牌
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::e5a29a947ecb
    anchor: cli:profile
    display: profile — profile 便携导出(QR)与载荷检查
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::e7a4baf7624a
    anchor: cli:account
    display: account — 管理 profile 下的子账户
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::eaa0a1305122
    anchor: cli:auth
    display: auth — LLM 提供商认证管理(login/logout/status)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::ec77c2b04726
    anchor: tool:grep
    display: grep — grep 搜索
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::ec7ead7c9dee
    anchor: tool:git
    display: "git — git 工具(feature: git)"
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::ed44b165a33f
    anchor: tool:check_workspace_contract
    display: check_workspace_contract — 工作区契约检查
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::ed8911328441
    anchor: api:my
    display: /api/my/* (40 条路由) — 终端用户自助面(我的会话/配置/资源)
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::eea38f4a0a1c
    anchor: api:voice
    display: /api/voice/* (2 条路由) — 语音
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::eeb59e621e20
    anchor: api:admin
    display: /api/admin/* (90 条路由) — 管理面:profiles/allowed-emails/monitor/ominix/platform-skills/audit 等
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::eec1a2a5cd04
    anchor: cli:status
    display: status — 系统状态
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::efbff6821236
    anchor: tool:admin/list_profiles
    display: list_profiles — 列出 profiles
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::f199bb686d54
    anchor: api:auth
    display: /api/auth/* (10 条路由) — 登录/令牌/OAuth 流
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::f4610f9f8153
    anchor: api:voices
    display: /api/voices/* (1 条路由) — 声音列表
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::f52db634c2f3
    anchor: tool:diff_edit
    display: diff_edit — 差异编辑
    state: active
    verified: false
    aliases: []
    superseded_by: []
    confirmed_by: null
    evidence: null
    confirmed_at: null
    retired_by: null
    retire_event_id: null
  - key: core::fd7a5a76b8ea
    anchor: tool:web_fetch
    display: web_fetch — 网页抓取(SSRF 防护)
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
