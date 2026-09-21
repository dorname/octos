# 变更提案：seed-readonly-write-probe

> module: core | created: 2026-09-21 | 依据：黑板条目 #15（修复 #13 缺陷：seed_readonly 改写探测）

## 变更原因
#14 blocked 实证（2026-09-21）：#13 的 `seed_readonly` 检测用 `permissions().readonly()`，对 644 文件返回 false（644 有 owner write 位），但 CM subPath 挂载使路径为**文件系统层只读**（挂载语义，非权限位）——save 误判 seed 可写 → 直接 rename → EROFS（`failed to rename profile`），UI 供 key 真机仍不通（issue #11 四条标准未达）。pod 内 `echo test >> cluster-worker.json` 实测 `Read-only file system` exit=1。

## 变更类型
代码级（缺陷修复：检测方式）+ 测试级（UT 覆盖挂载只读场景）。

## 选型设计（写探测，放弃 permissions().readonly()）
**选写探测**，理由：
- `permissions().readonly()` 只读**文件权限位**——644 有 owner write 位，永远返回 false，检测不到**挂载层只读**（bind-mount ro / ConfigMap subPath ro 都是文件系统层拒绝写，不改权限位）。
- **写探测**（在同目录创建临时文件，EROFS/PermissionDenied 判只读，探测后清理）直接探测**真实的写能力**——同时覆盖权限位只读（chmod 444）与挂载层只读（CM 挂载）两种场景，是唯一对两种只读形态都正确的检测。

## 实现（保留 #13 现有语义）
`seed_readonly` 改为写探测：
1. 在 `path` 的父目录创建临时文件（`<id>.json.probe-<pid>`）；
2. 写成功 → 删除临时文件 → **可写**（seed_readonly=false）；
3. 写失败且错误为 EROFS / PermissionDenied → **只读**（seed_readonly=true）；
4. 其他错误（目录不存在等）→ 保守判**可写**（不阻断正常 save 路径）。

**完整保留 #13 语义**：
- 无标记历史残留忽略（种子胜）——UT-S16-38 语义不变；
- 无 override 时种子+secret——UT-S16-36 语义不变；
- 可写路径 save 不产生 override——UT-S16-40 语义不变；
- 只读（挂载）路径 save 改写 override 打 managed_by=ui——UT-S16-39 语义不变，但现在对**挂载只读**也生效（#14 暴露的缺口）。

## 变更范围
- 影响的 API：ProfileStore::save（seed_readonly 检测方式）
- 影响的编排测试：crates/octos-cli profiles 测试
- 业务场景：S16 集群域（issue #11 集群模式 LLM key 配置）

## 本批覆盖的 UT 用例 ID（挂 S16 域接 UT-S16-41 起）
- **UT-S16-41** — 写探测对权限位只读（chmod 444）判只读：save 改写 override（保留 UT-S16-39 语义）
- **UT-S16-42** — 写探测对**挂载层只读**（权限位 644 可写但文件系统层拒绝写）判只读：save 改写 override（#14 暴露的缺口，模拟方式见实现注）
- **UT-S16-43** — 写探测对可写路径判可写：save 写 seed 不产生 override（保留 UT-S16-40 语义）
- **UT-S16-44** — 写探测后临时文件被清理（不残留 probe 文件）
与 `logos/resources/test/core-S16-test-cases.md` 对齐（merge 时追加）。

## 部署影响
- 是否需要部署：是（修复后由外环安排重做 #14 重建+注入+滚动——**本轮不做部署**）
- 影响环境：本地 docker-desktop octos ns
- 是否涉及数据迁移：否
- 是否需要回滚预案：否
- 是否需要 smoke：由外环重做 #14 时验收

## 实现注（挂载层只读的 UT 模拟）
UT 无法真实 bind-mount。模拟方式：在测试中用 `std::fs::set_permissions` 把**目录**设为只读（0555），使该目录下创建临时文件被 EROFS/PermissionDenied 拒绝——这与挂载只读在写探测上的行为一致（都是"权限位文件 644 可写但所在文件系统/目录拒绝写"）。这正是 #14 暴露场景的最小等价模拟。
