# 变更提案：mint-respects-legacy-data

> module: core | created: 2026-09-22

## 变更原因
黑板 #46（验收门 B 门失败整改）。外环 probe（第七次注入 a39d8c01 后）：旧会话
open（web-1789871791268-87x3ps,profile_id=admin）被铸造为 admin:api:web-…→ hydrate
messages=0；旧裸键 ledger(/tmp/octos-data/ui-protocol/<hex(裸id)>）与 JSONL 成孤儿。
新世界门全过。

## 变更类型
代码级（缺陷修复：铸造前探数据存在性，不改对外契约形状）。

## 变更概述（键跟数据走）
open 铸造前探数据存在性：若裸键在显式 profile 下已有数据（ui-protocol ledger 目录
`<data>/ui-protocol/<hex(裸键)>` 存在 或 profiles/<P>/data/sessions/<裸键>.jsonl 存在）
则**跳过铸造**沿用裸键（读取经 ③ Admin 连接语义命中 admin 管理器→旧账本重放）；
仅无数据的新会话铸造。禁止读侧翻译层（与 #41 否决 guard 映射同理）。

## 测试（hermetic,UT-S16-67..70）
有 ledger 的裸键 open→不铸造 / 有 JSONL 无 ledger→不铸造 / 全新裸键→铸造 /
裸 vs 规范键探测隔离（不互染）。legacy hydrate 重放由 no-history + ③ legacy 链覆盖。

## 验收门（外环第八次注入后复跑 A/B 双门）
B 门：存量会话 open→hydrate 重放旧 messages（非 0)。
