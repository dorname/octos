# 实现任务

## [delta] 规格变更
- [x] 无（缺陷修复：只读检测方式修正，不改对外契约形状）

## [code] 代码实现
- [x] probe_readonly_fs 改优先探目标文件本身（OpenOptions write(true) 不 create 不 truncate，EROFS/PermissionDenied 判只读）
- [x] 目录临时文件探测降级为 fallback（目标文件不存在等场景）
- [x] UT-S16-45 起新增「目录可写+目标文件 ro」盲区场景（#19 铁证入案）
- [x] 保留 UT-S16-41..44 全绿
- [x] 用例登记 core-S16-test-cases.md + reporter test-results.jsonl

## 验收
- profiles:: 全绿（--test-threads=4）
- root 环境权限位模拟断言跳过（current_euid_is_root，同 #15 惯例）
- 本轮不部署（外环重做注入链验收）
