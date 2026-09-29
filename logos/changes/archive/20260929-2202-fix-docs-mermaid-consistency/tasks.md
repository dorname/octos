# 实现任务

## [delta] 规格变更

- [x] 产出 delta 文件到 `deltas/prd/3-technical-plan/2-scenario-implementation/core-S17-octoloop-night-watchdog.md` — 修复时序图消息文本中的 ASCII 分号（mermaid 语句分隔符）导致的整图解析失败
- [x] 产出 delta 文件到 `deltas/prd/3-technical-plan/1-architecture/core-01-architecture-overview.md` — 数字口径核验修正（IM 通道 ×15+api/cli、clap 命令 ×30、路由口径）+ 可移植性存疑语法按需改写
- [x] 产出 delta 文件到 `deltas/prd/3-technical-plan/1-architecture/core-system-map.md` — 注明「17 通道」口径 = 15 IM + api/cli 本地通道
- [x] 产出 delta 文件到 `deltas/prd/1-product-requirements/core-01-requirements.md` — 基线数字声明更新为 2026-09-29 核验值并标注核验日期

## [code] 代码实现

切片评分（slice-planner 六维）：影响范围 1（2 文件：scripts/check-mermaid.mjs + CLAUDE.md）+ 行为复杂度 0（单一路径扫描脚本 + 文档数字对齐）+ 契约 0 + 测试 0（无 UT/ST，纯文档修正变更）+ 风险 0（易回滚）+ 不确定性 0 = **1 分 → 单切片**。删后续自检：无后续切片可删。测试 ID：无 UT/ST（proposal「复用测试 ID」节已声明；脚本为文档校验工具，非业务代码）。

### 切片 S1：mermaid 校验脚本 + CLAUDE.md 数字对齐（唯一切片，闭环）

- [x] S1.1 `scripts/check-mermaid.mjs`（新增）— 扫描 logos/ 下所有 .md 的 mermaid 块，用 mermaid.parse 逐块校验语法（jsdom 环境），输出 文件:行号 + 失败原因；缺依赖时给出安装提示；退出码非零表示存在语法失败块
- [x] S1.2 CLAUDE.md 顶部架构描述数字对齐 —「14 channels」→ 15 个 IM 通道（17 个通道实现含 api/cli）；「91 REST endpoints」→ 与 system-map 一致的核验口径
- [x] S1.3 验证：全新克隆视角运行 `node scripts/check-mermaid.mjs`（按提示装依赖后）对 live 文档零失败
