# 实现任务

## [delta] 规格变更
- [x] 产出 delta：`core-04-serve-api-design.md` 增补「未注册基础设施路径 404 契约」——`api`/`webhook`/`internal`/`v1`/`health`/`openapi.json`/`docs` 前缀或精确路径未注册时返回 `404 application/json`，绝不 307 进 SPA

## [code] 代码实现
- [x] 单切片：扩展 `is_api_or_infra_path`（static_files.rs:317）白名单，含段边界防兄弟路径误判（`v1beta`、`apiculture` 等）。UT（TDD）：`/v1/chat/completions` GET/POST → 404 JSON；`/openapi.json`、`/docs`、`/docs/x` → 404 JSON；`/v1beta` → 不命中白名单（走原有 SPA 逻辑）；既有 `api/webhook/internal` 用例不回退。OpenLogos reporter 写入 test-results.jsonl

## [deploy] 部署任务
- [x] 重建 musl 二进制（nice+限核）经 18088 注入，滚动重建 octos pod，直连 50080 smoke：`GET /v1/x` → 404 JSON；`GET /openapi.json` → 404 JSON；`GET /health` → 200 JSON（注册路由不回退）；`GET /app/` → 200 HTML（SPA 不回退）
