# 变更提案：seed-readonly-target-file-probe

> module: core | created: 2026-09-21

## 变更原因
黑板条目 #20（洋葱第二层）。#19 真机铁证：k8s ConfigMap **subPath** 挂载是
「**目录可写 + 单文件只读**」——`/tmp/octos-data/profiles` 目录 `touch` 可写（PVC），
但 `cluster-worker.json` 单独以 `ro,relatime` 成行挂载，`echo >>` 报 EROFS。
#15 的写探测只探 **parent 目录**（创建临时文件）→ 目录可写 → 误判 seed 可写 →
save 直接 `rename` 种子 → EROFS，UI 供 key 真机仍不通（issue #11 四条标准未达）。

这是只读检测的演进链：`permissions().readonly()` 探不到挂载只读（#14）→
目录写探测探不到单文件只读（#19）→ 必须探 **目标文件本身**。

## 变更类型
代码级（缺陷修复：只读检测方式修正，不改对外契约形状；save→override 语义不变）。

## 变更范围
- 影响的需求文档：无
- 影响的功能规格：profile 加载/保存的只读种子处理（`crates/octos-cli/src/profiles.rs`）
- 影响的业务场景：k8s CM subPath 挂载种子（目录可写+单文件 ro）下 UI 保存 profile
  → 正确改写 `<id>.override.json`（`managed_by:"ui"`）而非 rename 种子 EROFS
- 影响的 API：无（内部 `probe_readonly_fs` 语义修正）
- 影响的 DB 表：无
- 影响的编排测试：profiles:: 单测（UT-S16-41..44 保留全绿 + 新增 UT-S16-45..）

## 部署影响
- 是否需要部署：是（由外环重做 #14/#19 注入链验收；本提案不自行部署）
- 部署原因：binary 需含本修复，真机验收 save→override 生效
- 影响环境：k8s 集群
- 是否涉及数据迁移：否
- 是否需要回滚预案：否（纯检测逻辑修正，读写路径行为对可写场景不变）
- 是否需要 smoke：是（外环真机：PUT profile → override.json 生成含 managed_by:"ui"）

## 变更概述
`probe_readonly_fs(path)` 改为**优先探目标文件本身**：对 seed path 以
`OpenOptions::new().write(true)` 打开（**不 create、不 truncate**，纯测写打开权限），
EROFS/PermissionDenied 判只读；保留目录临时文件探测作为 **fallback**（目标文件
不存在等无法直接打开的场景）。可选加固（读 `/proc/self/mounts` 判 ro 挂载）在
proposal 权衡后**不采纳**——直接写打开探测已覆盖 subPath 单文件 ro（打开写即
EROFS），且无可移植性负担（/proc 仅 Linux）。

### 选型理由
- **目标文件写打开（采纳）**：直接对 seed path `write(true)` 打开——subPath 单文件
  ro 挂载上打开写立即 EROFS，正是 #19 盲区；同时对 chmod-444（PermissionDenied）
  与 mount-ro 目录（EROFS）也正确，一个探测覆盖全部三种只读形态。不 create 不
  truncate，对存在文件无副作用；文件不存在时 `open` 返回 NotFound，落入 fallback。
- **目录临时文件探测（降级 fallback）**：保留以覆盖「目标文件尚不存在、父目录
  只读」的场景（save 新建 seed 时目录 ro 仍应判只读）。
- **读 /proc/self/mounts（不采纳）**：最可靠但仅 Linux，且需解析挂载表+路径前缀
  匹配（subPath 单文件成行需精确匹配），复杂且不可移植；写打开探测已等效。

### 必须保留 / 新增 UT
- 保留 UT-S16-41..44 全绿（chmod-444 / mount-ro 目录 / 可写 / probe 清理）。
- 新增 UT-S16-45 起，覆盖 #19 盲区「目录可写 + 目标文件 ro」：目录 0755（可写）
  + seed 文件自身 ro（模拟 subPath：文件单独 ro，目录可写）→ probe 必须判只读、
  save 必须改写 override。 Unix 上用 `chmod 0444` 文件 + `chmod 0755` 目录模拟
  （root 跳过权限位模拟，同 #15 惯例）；并断言目录临时文件探测单独会误判（证明
  目标文件探测的必要性）。

## 实现约束
CPU 限载（CARGO_BUILD_JOBS=1，--test-threads=4）。本轮不部署。
