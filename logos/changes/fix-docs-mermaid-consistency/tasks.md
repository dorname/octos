# 实现任务

## [delta] 规格变更

- [x] 产出 delta 文件到 `deltas/prd/3-technical-plan/2-scenario-implementation/core-S17-octoloop-night-watchdog.md` — 修复时序图消息文本中的 ASCII 分号（mermaid 语句分隔符）导致的整图解析失败
- [x] 产出 delta 文件到 `deltas/prd/3-technical-plan/1-architecture/core-01-architecture-overview.md` — 数字口径核验修正（IM 通道 ×15+api/cli、clap 命令 ×30、路由口径）+ 可移植性存疑语法按需改写
- [x] 产出 delta 文件到 `deltas/prd/3-technical-plan/1-architecture/core-system-map.md` — 注明「17 通道」口径 = 15 IM + api/cli 本地通道
- [x] 产出 delta 文件到 `deltas/prd/1-product-requirements/core-01-requirements.md` — 基线数字声明更新为 2026-09-29 核验值并标注核验日期

## [code] 代码实现

（本段在 plan 段留空：本提案包含少量非规格文件工作——新增 `scripts/check-mermaid.mjs` 文档 mermaid 语法校验脚本（防回归）、CLAUDE.md 顶部架构数字对齐——`[code]` 切片由 merge 后的 `slice-planner` 基于已合并规格统一规划。此处仅保留 `## [code]` 标题，勿提前填写切片项。）
