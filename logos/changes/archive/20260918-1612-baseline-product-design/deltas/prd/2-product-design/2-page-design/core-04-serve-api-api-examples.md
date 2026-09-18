# Delta: prd/2-product-design/2-page-design — core-04-serve-api-api-examples.md

> target: logos/resources/prd/2-product-design/2-page-design/core-04-serve-api-api-examples.md(全新文档)

## ADDED — core-04 REST 服务 API 调用示例原型

# core-04 REST 服务 — API 调用示例原型

> 配套规格：`core-04-serve-api-design.md`（S05 / S13）
> 形式：HTTP 调用示例。端点路径为**代表性示例**（157 条路由的完整清单以 `crates/octos-cli/src/api/router.rs` 为准）；请求/响应字段为示意结构，以实际响应为准。

## 示例 1：启动服务

```console
$ octos serve
octos serve — REST API + dashboard

  listening:  http://127.0.0.1:50080
  dashboard:  http://127.0.0.1:50080/
  admin API:  /api/admin/*（需要 admin bearer token）

^C  received SIGINT, shutting down gracefully
```

## 示例 2：公开路由 — 版本（无需认证）

```console
$ curl -s http://127.0.0.1:50080/api/version | jq
{
  "version": "0.x.y",
  "name": "octos"
}
```

## 示例 3：用户级路由 — 我的会话列表（Bearer 认证）

```console
$ curl -s http://127.0.0.1:50080/api/my/sessions \
    -H "Authorization: Bearer $OCTOS_USER_TOKEN" | jq
{
  "sessions": [
    { "key": "api:local:default#a1b2", "updated_at": "2026-09-18T07:12:33Z", "message_count": 42 }
  ]
}
```

无 token：

```console
$ curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:50080/api/my/sessions
401
```

## 示例 4：流式对话（增量读取）

```console
$ curl -N http://127.0.0.1:50080/api/stream/chat \
    -H "Authorization: Bearer $OCTOS_USER_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"message": "用一句话介绍 octos", "session": "demo"}'

data: {"type":"delta","text":"octos 是一个"}
data: {"type":"delta","text":" Rust 原生的"}
data: {"type":"delta","text":"多租户 Agentic OS。"}
data: {"type":"done","session":"demo","usage":{"input_tokens":512,"output_tokens":24}}
```

## 示例 5：管理面 — profile 列表（admin token）

```console
$ curl -s http://127.0.0.1:50080/api/admin/profiles \
    -H "Authorization: Bearer $OCTOS_ADMIN_TOKEN" | jq
{
  "profiles": [
    { "id": "default", "enabled": true, "status": "running" },
    { "id": "team-a",  "enabled": true, "status": "running" }
  ]
}
```

普通用户 token 访问管理面：

```console
$ curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:50080/api/admin/profiles \
    -H "Authorization: Bearer $OCTOS_USER_TOKEN"
403
```

## 示例 6：仅本机绑定验证（安全默认）

```console
# 服务器上（默认启动）
$ octos serve &
$ ss -ltn | grep 50080
LISTEN 0  511  127.0.0.1:50080  0.0.0.0:*

# 从另一台机器
$ curl --max-time 3 http://192.168.1.10:50080/api/version
curl: (7) Failed to connect to 192.168.1.10 port 50080: Connection refused

# 显式对外（管理员明确决策）
$ octos serve --host 0.0.0.0 --port 50080
  listening:  http://0.0.0.0:50080
  ⚠️ 已对局域网/公网开放，请确认已配置认证与反代 TLS
```

## 示例 7：ACP 接入 Zed（S13）

Zed `settings.json`：

```json
{
  "agent_servers": {
    "octos": { "command": "octos", "args": ["acp"] }
  }
}
```

交互：Agent Panel 选择 octos → 发起对话 → 流式回显；文件修改以 Zed diff review 呈现，确认后落盘。
