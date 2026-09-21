# 实现任务

## [delta] 规格变更
- [x] 无（缺陷修复：检测方式变更，不改对外契约形状）

## [code] 代码实现
- [x] seed_readonly 改写探测（probe_readonly_fs：同目录创建临时文件，EROFS/PermissionDenied 判只读，探测后清理）
- [x] UT-S16-41..44（chmod-444/mount-ro 0555/可写/probe 清理）
- [x] root 环境 3 断言跳过（current_euid_is_root 读 /proc/self/status 无 unsafe）
- [x] UT-S16-39 root 跳过补（overlay_save_to_readonly_seed，uid 0 绕过 444）
- [x] 用例登记 core-S16-test-cases.md 批 8 节 + reporter test-results.jsonl +4 行

## 验收
- profiles:: 106/106 绿（--test-threads=4）
- probe/save-redirect 逻辑环境无关（CI/非 root 跑）
