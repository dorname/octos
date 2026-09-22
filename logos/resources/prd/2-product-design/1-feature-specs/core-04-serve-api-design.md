# core-04 REST 服务与 IDE 接入 — 功能规格

> 覆盖场景：S05（REST API 服务与流式集成）、S13（ACP 协议接入 IDE）
> 需求来源：core-01-requirements.md（Phase 1）
> 配套原型：`core-04-serve-api-api-examples.md`

## 一、S05: REST API 服务与流式集成 — 交互规格

### 1.1 `octos serve`

**命令格式**：`octos serve [--host <ADDR>] [--port <PORT>] [--cwd <PATH>] [--auth-token <TOKEN>]`

**参数设计**：

| 参数 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| --host | string | 否 | 127.0.0.1 | 监听地址（安全默认仅本机；对外需显式指定） |
| --port | int | 否 | 50080 | 监听端口 |
| --cwd | path | 否 | 当前目录 | 工作区根目录 |
| --auth-token | string | 否 | — | 管理面 bearer token（也可用环境变量/配置提供） |

**交互流程**：
1. 用户运行 `octos serve`；CLI 加载配置 → 构建运行时 → axum router 挂载 23 个路由组（157 条路由）→ 绑定监听
2. 启动输出监听地址与管理面入口；浏览器访问 `http://127.0.0.1:50080/` 打开 Web 仪表盘
3. 客户端调用 REST 路由：公开路由（auth/version 等）→ 用户级路由（`/api/my/*`、files、tasks、stream 等，需用户凭证）→ 管理路由（`/api/admin/*`，需 admin token）
4. 对话类请求进入 agent/session 运行时，流式接口以 SSE/分片形式持续返回增量，结束后完整消息落会话
5. Ctrl-C 优雅停机

**路由组概览**（完整清单以 `crates/octos-cli/src/api/router.rs` 为准）：

| 分组 | 条数 | 面 | 认证 |
|------|------|----|------|
| /api/admin/* | 90 | 管理面：profiles / allowed-emails / monitor / audit / platform-skills 等 | admin token |
| /api/my/* | 40 | 终端用户自助面：我的会话/配置/资源 | 用户凭证 |
| /api/ui-protocol/* | 12 | 仪表盘前端协议传输 | 用户凭证 |
| /api/auth/* | 10 | 登录/令牌/OAuth 流 | 公开 |
| /api/files/* /api/tasks/* /api/stream/* 等 | 15+ | 文件、任务、流式 | 用户凭证 |
| /api/version 等 | 若干 | 版本/健康 | 公开 |

#### 验收条件（交互级）

##### 正常：启动与仪表盘
- **GIVEN** 已完成 init 与认证
- **WHEN** 用户运行 `octos serve`
- **THEN** 启动输出含监听地址 `http://127.0.0.1:50080`；浏览器访问返回仪表盘页面；`GET /api/version` 返回 200 与版本信息；退出码（停机后）为 0

##### 正常：用户级流式调用
- **GIVEN** serve 运行中，客户端持有有效用户凭证
- **WHEN** 客户端调用流式对话路由并持续读取响应
- **THEN** 响应按增量分片持续到达（流式），终止分片后完整消息已写入对应会话；断开后重连可查询到该会话历史

##### 异常：仅本机绑定
- **GIVEN** 用户以默认参数启动 serve
- **WHEN** 从局域网另一台机器连接 `http://<host-ip>:50080`
- **THEN** 连接被拒绝；启动输出中监听地址显示 127.0.0.1（对外暴露必须显式 `--host`）

##### 异常：无凭证访问用户级路由
- **GIVEN** serve 运行中
- **WHEN** 客户端不带 token 请求 `/api/my/*` 路由
- **THEN** 返回 401；带普通用户 token 请求 `/api/admin/*` 返回 403；响应体不泄露内部细节

### 1附.1 集群模式启动

当配置/环境启用 PostgreSQL 集群后端时，`octos serve` 进入集群角色（见 `serve_cluster` / 部署清单）：

1. 校验 `DATABASE_URL` 与迁移版本
2. 挂载 UI Protocol / admin 路由（与单机相同对外契约）
3. 会话事件、审批、租约写入 PG；WS 支持 `replay_from_pg`
4. Cron 使用 `CronServicePg`（若启用定时任务）

#### 验收条件（交互级增量）

##### 正常：集群 serve 探活
- **GIVEN** cluster 形态已部署
- **WHEN** 客户端访问 `GET /api/version` 与仪表盘
- **THEN** 行为与单机 serve 一致（200 + 页面），后端状态在 PG

##### 异常：缺 DATABASE_URL
- **GIVEN** 集群标志已开但无可用 PG
- **WHEN** 启动
- **THEN** 失败并提示修复路径，不静默回落 JSONL 真相源

### 1附.2 Profile LLM 运行时热加载语义

AppUI `profile/llm/select`、`profile/llm/upsert`、`profile/llm/delete` 三个 mutation 持久化提交后，运行时转换对**所有** profile 一视同仁——不再区分「启动期钉住（startup-pinned）」与「动态」两类：

1. **驱逐**：会话运行时缓存按 profile 失效（`invalidate_profile`）；
2. **代际守卫**：动态 ProfileRuntime 缓存的代际先递增再清槽，使在途的旧配置引导在插入时被拒（#2164 机制复用到启动 profile）；
3. **重建**：从 profile store 的已提交文件重引导 `ProfileRuntime`（纯配置快照，幂等），写入动态缓存——`resolve_session_profile_runtime` 的动态优先语义使下一轮对话即时命中新模型，无需重启进程。

`state.profiles` 启动快照保持不可变，仅充当：(a) 无任何 LLM mutation 发生时的引导兜底；(b) steer sweep 等需要枚举 profile 的场景源。一旦某 profile 发生过 LLM mutation，其运行时真相移交动态缓存。

**响应契约**：三个 mutation 的响应仍携带 `restart_required` 字段（恒在，客户端不得以字段缺失作判断），但启动 profile 场景不再出现 `restart_required=true`；转换结果（`disposition`）取值收窄为 `reloaded` / `deferred` / `persisted_but_not_live`（`restart_required` disposition 退役，仅历史兼容保留枚举）。`runtime_policy_stamp` 的 `model`/`provider` 按「动态缓存 → 启动快照」的真实服务优先级报告，与下一轮实际服务的模型一致。

#### 验收条件（交互级）

##### 正常：启动 profile 切换模型即时生效
- **GIVEN** `octos serve` 运行中，目标 profile 为启动期钉住（来自启动配置）
- **WHEN** 客户端调用 `profile/llm/select`（或 upsert）切换主模型并成功提交
- **THEN** 响应 `restart_required=false`、`disposition=reloaded`；同一 profile 的下一轮对话实际由新模型服务（stamp 与 LLM 请求均命中新模型），进程未重启

##### 正常：无 mutation 的冷启动解析不重复引导
- **GIVEN** serve 刚启动，某启动 profile 从未发生过 LLM mutation
- **WHEN** 该 profile 的会话解析运行时
- **THEN** 直接使用启动快照，不触发额外的 ProfileRuntime 引导（无双倍内存/双重 redb 占用）

##### 异常：重建与并发提交竞争
- **GIVEN** 一次 LLM mutation 提交后运行时在重建中
- **WHEN** 第二次 mutation 在重建完成前提交
- **THEN** 代际守卫拒绝在途旧 runtime 写入缓存；最终缓存中的 runtime 来自最后一次提交的文件；客户端收到可读错误并被提示重试，不会静默服务旧模型

##### 异常：重建失败不破坏现状
- **GIVEN** mutation 已持久化但新配置无法引导（如凭证缺失）
- **WHEN** 提交后转换尝试重建
- **THEN** 响应 `disposition=persisted_but_not_live` 并附可读错误；旧运行时已被驱逐，下一轮对话重试引导并呈报同一错误，而非静默回落旧模型

### 1附.3 未注册基础设施路径 404 契约

`octos serve` 的 SPA fallback 只对**前端路由**负责。以下基础设施路径族在未注册到路由表时，必须返回 `404 application/json`（`{"error":"not_found","path":"<request-path>"}`），绝不 `307` 重定向进 SPA——API 客户端（Playwright `apiRequestContext`、reqwest 等）会被重定向上限或 HTML body 打断：

- 前缀族（段边界匹配）：`api`、`webhook`、`internal`、`v1`
- 精确路径：`health`、`openapi.json`、`docs`（及 `docs/` 前缀）

段边界约束：`/v1beta`、`/apiculture`、`/healthcare` 等兄弟路径**不**命中白名单，维持原 SPA 逻辑。

#### 验收条件（交互级）

##### 正常：未注册 API 形态路径 404 JSON
- **GIVEN** serve 运行中
- **WHEN** 客户端 GET/POST `/v1/chat/completions`、`/openapi.json`、`/docs`（均未注册）
- **THEN** 均返回 404 `application/json`，body 含 `"error":"not_found"` 与请求路径；无 307/Location 头

##### 正常：兄弟路径不误判
- **GIVEN** 同上
- **WHEN** 请求 `/v1beta` 或 `/apiculture`
- **THEN** 不返回 404 JSON；按原 SPA 逻辑处理

##### 正常：已注册路由不回退
- **GIVEN** 同上
- **WHEN** 请求 `/health`（已注册）与 `/app/`（SPA）
- **THEN** `/health` 仍由 handler 返回 200 JSON；`/app/` 仍返回 SPA HTML——白名单只影响未注册路径

## 二、S13: ACP 协议接入 IDE — 交互规格

### 2.1 `octos acp`

**命令格式**：`octos acp`（stdio 模式，由 IDE 作为子进程拉起，非用户直接交互使用）

**交互流程**：
1. 用户在 IDE（如 Zed）的 agent 配置中登记外部 agent server：命令 `octos acp`（stdio 传输）
2. IDE 拉起子进程，按 ACP 协议完成握手（initialize → 能力协商 → 新建会话）
3. 用户在 IDE 面板发起对话；ACP 请求映射到 octos 会话运行时（同一 agent loop、工具与沙箱策略）
4. agent 流式输出经 ACP 事件回显 IDE；文件编辑经 IDE 的 diff 确认界面呈现
5. IDE 关闭面板/退出时终止子进程

**Zed 配置示例**（settings.json 片段，键名以 Zed 当时版本为准）：

```json
{
  "agent_servers": {
    "octos": {
      "command": "octos",
      "args": ["acp"]
    }
  }
}
```

#### 验收条件（交互级）

##### 正常：IDE 内端到端问答
- **GIVEN** Zed 已配置 octos agent server 且本机已认证
- **WHEN** 用户在 Zed 的 agent 面板选择 octos 并提问"解释当前文件的 main 函数"
- **THEN** IDE 拉起 `octos acp` 完成握手；回答流式回显面板；引用的文件路径可点击跳转

##### 正常：IDE 内文件编辑确认
- **GIVEN** 同上
- **WHEN** agent 需要修改项目文件
- **THEN** 修改以 IDE diff 视图呈现，用户确认后落盘；拒绝则 agent 收到拒绝反馈并继续对话

##### 异常：octos 二进制不在 PATH
- **GIVEN** IDE 配置的命令无法解析
- **WHEN** IDE 拉起 agent server
- **THEN** IDE 显示 agent server 启动失败；octos 不产生任何后台驻留进程

**原型**：`core-04-serve-api-api-examples.md`
