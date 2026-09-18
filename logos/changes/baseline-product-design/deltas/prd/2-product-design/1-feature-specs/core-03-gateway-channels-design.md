# Delta: prd/2-product-design/1-feature-specs — core-03-gateway-channels-design.md

> target: logos/resources/prd/2-product-design/1-feature-specs/core-03-gateway-channels-design.md(全新文档)

## ADDED — core-03 网关与通道 功能规格

# core-03 网关与通道 — 功能规格

> 覆盖场景：S04（团队 IM 通道接入与消息网关）、S06（定时任务与无人值守自动化）、S14（多租户运维与管理面）
> 需求来源：core-01-requirements.md（Phase 1）
> 配套原型：`core-03-gateway-channels-dialogue.md`

## 一、S04: 团队 IM 通道接入与消息网关 — 交互规格

### 1.1 `octos gateway` 与 `octos channels`

**命令格式**：
- `octos gateway [--profile <NAME>]` — 以常驻进程运行多通道网关
- `octos channels <list|add|remove|test> ...` — 通道配置管理

**通道配置**（config.json，以 telegram 为例）：

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token_env": "TELEGRAM_BOT_TOKEN",
      "require_mention": true
    }
  }
}
```

**支持的 17 个通道**：api、cli、dingtalk、discord、email（feature-gated）、feishu、line、matrix、matrix-user、qq-bot、slack、telegram、twilio、wechat、wecom、wecom-bot、whatsapp。

**交互流程**：
1. 用户在 config.json 配置通道凭据（token 走环境变量，不落盘明文）并启用通道
2. 运行 `octos gateway`；CLI 加载配置 → 构建 LLM/工具/记忆栈 → 启动各通道适配器（轮询或 webhook）→ 打印已启动通道清单
3. 入站消息到达：通道适配 → 解析/创建会话（session key 含通道与 chat 标识）→ 入队给会话 actor → agent 处理（LLM + 工具）
4. 回复出站：按通道格式渲染 → coalescing 分片（段落 > 换行 > 句子 > 空格 > 硬切，逐通道长度上限，≤ 50 片，UTF-8 安全边界）→ 逐片发送
5. 会话命令：用户在 IM 中发 `/new` 开新会话（fork，保留 parent 链）、`/s` 或 `/sessions` 查看、`/back` 返回上一会话
6. 停止：SIGINT/SIGTERM 触发优雅关闭（AtomicBool 停机信号），在处理的轮次完成后退出

#### 验收条件（交互级）

##### 正常：启动并打印通道清单
- **GIVEN** config.json 启用 telegram 与 feishu 两个通道且凭据有效
- **WHEN** 用户运行 `octos gateway`
- **THEN** 启动日志逐条显示两个通道已进入监听；进程常驻前台；退出码仅在收到停机信号后归 0

##### 正常：长回复自动分片
- **GIVEN** telegram 通道已启动，agent 产出约 9000 字符的回复
- **WHEN** 网关发回该回复
- **THEN** 用户在 Telegram 收到多条顺序消息：每片 ≤ 通道上限（Telegram 4000），分片处不切断单词/UTF-8 字符，首片带回复引用

##### 正常：群内 require_mention
- **GIVEN** 群聊通道配置 `require_mention = true`
- **WHEN** 群成员发送不含 @bot 的消息
- **THEN** 网关忽略该消息（不产生会话写入与 LLM 调用）；被 @ 的消息正常处理

##### 异常：凭据缺失的通道不启动
- **GIVEN** telegram 启用了但 `TELEGRAM_BOT_TOKEN` 未设置
- **WHEN** 启动 `octos gateway`
- **THEN** 启动日志显示 telegram 通道被跳过及缺失的环境变量名；其余通道正常启动；进程不退出

##### 异常：会话文件达到上限
- **GIVEN** 某会话 JSONL 达到 10MB 上限
- **WHEN** 新消息继续写入
- **THEN** 追加被拒绝并记录告警，会话可读历史不损坏（原子写保证不出现半行 JSON）；用户可 `/new` 开新会话继续

## 二、S06: 定时任务与无人值守自动化 — 交互规格

### 2.1 `octos cron`

**命令格式**：
- `octos cron add --name <NAME> --message <TEXT> [--every <SECONDS> | --cron "<EXPR>" | --at <TIME>] [--deliver --channel <CH> --to <TARGET>]`
- `octos cron list`
- `octos cron remove --name <NAME>`

**参数设计**：

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| --name | string | 是 | 任务名（唯一标识） |
| --message | string | 是 | 触发时注入 agent 的消息 |
| --every | int | 三选一 | 间隔秒数 |
| --cron | string | 三选一 | cron 表达式（可带时区） |
| --at | string | 三选一 | 一次性触发时间 |
| --deliver | flag | 否 | 执行结果投递到指定通道 |
| --channel | string | 配合 deliver | 目标通道 |
| --to | string | 配合 deliver | 目标会话/联系人 |

**交互流程**：
1. 用户 `octos cron add` 创建任务（持久化到 cron 存储，损坏文件隔离而非静默丢弃）
2. gateway 运行中，调度器到点将任务消息注入对应会话（sender 标记为 cron/heartbeat 系统来源）
3. agent 执行任务；`--deliver` 时结果经通道分片投递
4. `octos cron list` 显示任务、调度与下次触发时间；执行记录可查

#### 验收条件（交互级）

##### 正常：创建并列出一个周期任务
- **GIVEN** gateway 运行中
- **WHEN** 用户运行 `octos cron add --name daily-summary --message "汇总昨日 git 提交" --cron "0 9 * * *" --deliver --channel telegram --to 12345`，随后运行 `octos cron list`
- **THEN** add 输出创建成功；list 输出包含 daily-summary、cron 表达式、下次触发时间与投递目标；退出码 0

##### 正常：无人值守执行与投递
- **GIVEN** daily-summary 已创建且到达触发时间
- **WHEN** 调度器触发
- **THEN** agent 在独立会话上下文执行任务（无人值守迭代上限 UNATTENDED_MAX_ITERATIONS_FALLBACK=50 兜底），结果投递到 telegram:12345；执行记录含成功/失败状态

##### 异常：调度参数缺失
- **GIVEN** 用户运行 `octos cron add --name t1 --message "hi"`（未给 --every/--cron/--at）
- **WHEN** 命令解析校验
- **THEN** CLI 报错提示必须三选一提供调度参数，退出码非 0，不创建任务

## 三、S14: 多租户运维与管理面 — 交互规格

### 3.1 管理面形态

管理面有两类入口（同一套能力）：
- **REST 管理路由**：`/api/admin/*`（90 条，需 admin token），覆盖 profiles、allowed-emails、monitor、audit、platform-skills 等
- **admin 工具集**：20 个 `admin/*` 工具（list_profiles / start_profile / stop_profile / restart_profile / enable_profile / update_profile / view_logs / system_health / system_metrics / provider_metrics / manage_watchdog / view_sessions / cron_status / check_config / list_sub_accounts / create_sub_account / manage_skills / platform_skills / update_octos），供管理会话中的 agent 调用
- **CLI**：`octos admin`（租户与隧道）、`octos account`（子账户）、`octos profile`（便携导出）

**交互流程（管理员日常巡检）**：
1. 管理员调用 `GET /api/admin/.../health` 或在管理会话中问 agent "各 profile 状态如何"
2. agent 调用 admin/list_profiles + admin/system_health 工具汇总
3. 异常 profile 用 admin/restart_profile 恢复；操作留审计记录
4. 指标面：admin/system_metrics、admin/provider_metrics 输出用量与错误率

#### 验收条件（交互级）

##### 正常：巡检闭环
- **GIVEN** serve 运行且管理员持有 admin token
- **WHEN** 管理员请求 profile 列表与健康汇总
- **THEN** 返回各 profile 的启用状态、运行状态、最近错误；系统健康项逐条列出；响应 200

##### 异常：非 admin token 访问
- **GIVEN** 调用方持有的是普通用户 token
- **WHEN** 请求 `/api/admin/*` 任意路由
- **THEN** 返回 403，响应体不含 profile 清单等内部信息

**原型**：`core-03-gateway-channels-dialogue.md`（IM 侧用户与 bot 的对话脚本，覆盖 S04 分片/会话命令与 S06 定时推送）
