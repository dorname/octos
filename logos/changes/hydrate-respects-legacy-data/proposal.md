# 变更提案：hydrate-respects-legacy-data

> module: core | created: 2026-09-22

## 变更原因
黑板 #51（B 门 hydrate 腿 FAIL)。第八次注入后 WS 实测：open 保留裸键 ✅(#46),
但紧接 session/hydrate(bare)→ unknown_session(admin:api:web-…)。#46 只修了 open 铸造点,
hydrate(:25934)与 REST messages(:28497)两处 #43(b) 入口规范化仍无条件铸键。

## 变更概述(键跟数据走,禁止翻译层)
两处铸键前同调 legacy_session_data_exists(#46 已 pub(crate))——命中存量则保持裸键做
snapshot/session_known/REST 读;仅全新裸键铸键。抽出共用 helper
`normalize_session_key_at_entry`(open/hydrate/messages 三入口统一)。

## 测试(hermetic)
UT-S16-71(有 JSONL 裸键 hydrate/messages 入口→不铸)/ UT-S16-72(全新→铸,幂等);
open 腿由 #46 UT-S16-67..70 覆盖;幂等再 open 再 hydrate 不变(72 的幂等断言)。

## 验收门(外环第九次注入复跑 B 门)
open 裸键保留 → hydrate(bare)重放旧 messages(非 unknown_session)。
