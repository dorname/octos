# 实现任务

## [delta] 规格变更
- [x] 无需 delta：纯前端渲染条件修复，行为契约即 issue #13 期望（只渲染用户可见字段），在 proposal.md「变更概述」中固化

## [code] 代码实现
- [x] 单切片：(1) `RouterModeMenu` 在 `adaptiveMode === null` 时返回 null；(2) `CostBar` 在 `visible === "none"` 时返回 null。含 vitest UT（menu：null→不渲染/有模式→渲染；cost-bar：双 none→不渲染/model 有效 provider none→渲染 model；model 为 none provider 有效→渲染 provider）+ OpenLogos reporter 写入 test-results.jsonl（OW-HEADER-LEAK-01/02，octos-web a83fe27）

## [deploy] 部署任务
- [x] 重建 octos-web dist（npm run build）打包注入 web pod（index-B_qsEXra.js），playwright 真机复验：新会话与历史会话（单 provider）头部均无 Router pill / 裸 `none`；issue 的 `1` 定位为 files 面板角标（`chat-layout.tsx` `sessionFiles.length` 计数徽章，合法 UI，非调试泄漏）
