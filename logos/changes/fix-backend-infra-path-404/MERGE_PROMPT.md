# 合并指令

## 变更提案
- 提案名称：fix-backend-infra-path-404
- 提案目录：logos/changes/fix-backend-infra-path-404/

## 提案内容

# 变更提案：fix-backend-infra-path-404

> module: core | created: 2026-09-22

## 变更原因
Issue #17（fork dorname/octos）：`octos serve` 后端的 SPA fallback（`crates/octos-cli/src/api/static_files.rs`）对未注册路径返回 `307 Location: /app/` 而非 `404 application/json`——`/v1/chat/completions`、`/openapi.json`、`/docs` 等 API 形态路径被重定向进 SPA，导致 Playwright/reqwest 等客户端 `Max redirect count exceeded` 或拿到 HTML body。

根因已定位：`is_api_or_infra_path`（static_files.rs:317）的前缀白名单只有 `["api", "webhook", "internal"]`——`v1`、`openapi.json`、`docs` 不在其中，于是穿过 404 JSON 分支（:61）落入默认 UI 重定向。代码库对 `/api/*` 已确立「未注册基础设施路径必须 404 JSON」的契约（Bug 1 注释 + 既有测试 :431/:452/:462），本提案把同一契约扩展到其余基础设施路径。

## 变更类型
代码级（行为修复，契约与既有 Bug 1 一致，仅扩大适用面）。

## 变更范围
- 影响的需求文档：无
- 影响的功能规格：`logos/resources/prd/2-product-design/1-feature-specs/core-04-serve-api-design.md` 增补「未注册基础设施路径 404 契约」小节
- 影响的业务场景：S05（REST API 服务与流式集成）
- 影响的 API：未注册路径的响应码（307 → 404 JSON），已注册路由不变
- 影响的 DB 表：无
- 影响的编排测试：无

## 部署影响
- 是否需要部署：是
- 部署原因：k8s pod 内为旧二进制，issue 现场在 k8s；重建 musl 二进制滚动验证
- 影响环境：本地（docker-desktop k8s `octos` ns）
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：是（直连 50080 验证 `/v1/x`、`/openapi.json` → 404 JSON；`/health`、SPA 路径不回退）

## 变更概述
将 `is_api_or_infra_path` 的白名单从 `["api", "webhook", "internal"]` 扩展为 `["api", "webhook", "internal", "v1", "health", "openapi.json", "docs"]`（精确或前缀段匹配，防 `v1beta` 类兄弟路径误判），使未注册的基础设施路径一律返回 `404 application/json {"error":"not_found","path":...}`。`health` 虽已有注册路由（router.rs:899），列入白名单作防御——一旦路由表变动，它仍不会落进 SPA 重定向。


## 需要合并的 Delta 文件

### 1. deltas/prd/2-product-design/1-feature-specs/core-04-serve-api-design.md

- Delta 文件：`logos/changes/fix-backend-infra-path-404/deltas/prd/2-product-design/1-feature-specs/core-04-serve-api-design.md`
- 目标目录：`logos/resources/prd/2-product-design/1-feature-specs/`
- 操作：读取 delta 中的 ADDED / MODIFIED / REMOVED 标记，合并到目标目录中对应的主文档

## 执行要求

1. 逐个 Delta 文件处理，每处理完一个报告修改摘要
2. 对于 ADDED 标记：在主文档的指定位置插入新内容
3. 对于 MODIFIED 标记：替换主文档中同名章节的内容
4. 对于 REMOVED 标记：从主文档中删除对应章节
5. 保持主文档的原有格式和风格
6. 如果主文档有"最后更新"时间戳，同步更新
7. 所有变更完成后，列出修改清单
8. 所有变更合并完成后，自动执行 git commit（告知用户，无需确认）：
   git add -A && git commit -m "docs(fix-backend-infra-path-404): merge spec deltas"
   然后提示用户：按更新后的规格实现代码，代码完成后运行 `openlogos verify` 验收，验收通过后明确授权执行 `openlogos archive fix-backend-infra-path-404`。
