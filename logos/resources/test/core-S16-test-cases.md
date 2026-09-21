# core-S16 集群部署与前端族修复 — 测试用例（retroactive 登记）

> 场景：批 1/2/3 issue 修复（fork dorname/octos #1-#8）的自动化测试回填登记。
> 依据：黑板条目 #7（OpenLogos 流程回填，loop.md 第 6 条首次实战）。
> 提案：`logos/changes/retro-issue-flow-backfill/`（本文件为其 delta 产物）。
> 场景域：S16（k8s 集群部署族），编号全局唯一，不与 S01-S17 撞号（S16 由
> `20260920-1810-k8s-cluster-deploy` 占用，本登记沿用该域，场景复用不重开）。
> Reporter：每个用例的 pass 记录已按本表 ID 追加到
> `logos/resources/verify/test-results.jsonl`（见 retro-issue-flow-backfill
> tasks.md 验收节）。

## 批 1（issue #1 #2 #3 — deploy/k8s 族）

### UT-S16-01 ~ UT-S16-12 — crates/octos-bus/tests/deploy_directory_layout.rs

| ID | 测试函数 | 描述 |
|----|----------|------|
| UT-S16-01 | test_deploy_directory_layout | deploy/ 目录布局合规（无散落 yaml/脚本） |
| UT-S16-02 | test_no_scattered_deploy_files | 仓库根目录无散落部署文件 |
| UT-S16-03 | test_deploy_script_variants | deploy-k8s.sh 三变体（baseline/hostpath/cluster）可解析 |
| UT-S16-04 | test_deploy_script_unknown_variant | 未知变体报错退出 |
| UT-S16-05 | test_deploy_script_syntax | deploy-k8s.sh `bash -n` 语法通过（含批 1 #2 REPLACE_ME 拒发校验段） |
| UT-S16-06 | test_k8s_install_doc_sections | K8S_INSTALL.md 章节齐备（含批 1 #1 WSL 专节、批 3 #3 smoke 专节） |
| UT-S16-07 | test_k8s_install_prerequisites | K8S_INSTALL.md 前置条件节完整 |
| UT-S16-08 | test_k8s_install_troubleshooting | 故障排查节含 migrations 等关键词（批 1 #3 lazy-migrate 修正） |
| UT-S16-09 | test_octos_deployment_uses_recreate_strategy | Deployment strategy=Recreate（RWO PVC 锁文件设计） |
| UT-S16-10 | test_octos_deployment_has_no_rolling_update_surge | 无 RollingUpdate maxSurge（避免双 pod 抢锁） |
| UT-S16-11 | test_octos_deployment_single_replica_for_rwo_pvc | replicas=1 + octos-data 为 persistentVolumeClaim（批 3 #7 漂移防回退） |
| UT-S16-12 | test_k8s_deploy_proven_doc_records_recreate_and_portforward | K8S_DEPLOY_PROVEN.md 记录 Recreate 与 port-forward 实况 |

### UT-S16-13 ~ UT-S16-22 — crates/octos-bus/tests/pg_persistence_matrix.rs

| ID | 测试函数 | 描述 |
|----|----------|------|
| UT-S16-13 | test_pg_matrix_doc_covers_all_tables | PG 接入矩阵文档覆盖 11 张表 |
| UT-S16-14 | test_pg_matrix_doc_never_pg_categories | 文档覆盖 4 类永不接 PG 类别 |
| UT-S16-15 | test_k8s_manifest_uses_pvc_not_emptydir | manifest 用 PVC 非 emptyDir（批 1 #2 / 批 3 #7 防回退） |
| UT-S16-16 | test_k8s_manifest_uses_init_container | manifest 使用 initContainer（profile 注入 + binary 拉取） |
| UT-S16-17 | test_k8s_manifest_no_migrate_subcommand | manifest 不引用不存在的 `octos migrate` 子命令（批 1 #3 lazy-migrate 认知） |
| UT-S16-18 | test_k8s_manifest_binary_path_consistent | binary mountPath 与 command 一致（/opt/octos/octos） |
| UT-S16-19 | test_k8s_manifest_yaml_parse | manifest YAML 解析通过（11 docs） |
| UT-S16-20 | test_init_script_creates_required_dirs | init 脚本创建必需目录（data/profiles/inbox） |
| UT-S16-21 | test_init_script_writes_profile_config | init 脚本写 profile 配置（flat `<id>.json`，含 created_at/updated_at） |
| UT-S16-22 | test_init_script_documents_pg_migration_timing | init 脚本声明 PG lazy-migrate 时序（#2436 feature 要求） |

## 批 2（issue #4 #5 — octos-web 前端族）

### UT-S16-23 ~ UT-S16-25 — octos-web/src/runtime/hydrate-projection.test.ts（vitest）

| ID | 测试名 | 描述 |
|----|--------|------|
| UT-S16-23 | restores the real direct-voice Learn hydrate shape without browser state | hydrate 无浏览器态时还原真实 direct-voice 形状 |
| UT-S16-24 | prefers a canonical projection snapshot when the server supplies one | server 提供 canonical snapshot 时优先采用 |
| UT-S16-25 | does NOT drop durable rows that carry none of the three id keys | **批 2 #5 回归**：thread/turn/cmid 三键全缺的 durable 行不丢弃（合入 hydrate-legacy 共享车道） |

## 批 3（issue #7 #8 — AppUI 集群恢复前置 + 真机族）

### UT-S16-26 — crates/octos-bus/tests/deploy_directory_layout.rs

| ID | 测试函数 | 描述 |
|----|----------|------|
| UT-S16-26 | test_octos_deployment_mounts_cluster_worker_profile_configmap | **批 3 #7 断言**：Deployment 以 readOnly+subPath 挂载 cluster-worker-profile CM（profile 作为配置而非 PVC 状态，防滚动丢失） |

## 批 4（黑板 #8 — serve LLM 鉴权类失败快速失败，新规范全流程实战）

### UT-S16-27 ~ UT-S16-29 — crates/octos-llm

| ID | 测试函数 | 描述 |
|----|----------|------|
| UT-S16-27 | retry::tests::should_not_retry_on_401_authentication_failure | 模拟 401 provider：RetryProvider 单尝试即 Err，is_retryable=false，且错误仍 typed 为 Authentication（快速失败落态前提） |
| UT-S16-28 | retry::tests::should_not_retry_on_403_authentication_failure | 模拟 403 provider：同上，403 路径不重试 |
| UT-S16-29 | error::tests::should_render_401_with_provider_label_status_and_summary_for_turn_failfast | LlmError Display 在上游 401 时携带 provider 标签 + HTTP 状态码 + kind 摘要 + 上游正文摘要（turn/error message 复用同一 Display） |

## 批 5（黑板 #9 — nightly 双失败修复：Windows 测试 + E2E 会话管线）

### UT-S16-30 ~ UT-S16-32 — 缺陷 A（Windows）+ 缺陷 B（E2E）

| ID | 测试函数 | 描述 |
|----|----------|------|
| UT-S16-30 | matrix_channel::tests::test_handle_room_query_requires_token | 缺陷 A：appservice 监听端口固定 100ms sleep → 轮询等端口就绪（≤1s），消除 Windows CI 时序假设（nightly run 35514856517 Windows 516/1 失败） |
| UT-S16-31 | api::ui_protocol_transport::tests::fallback_enabled_profile_id_returns_none_for_explicit_non_main | 缺陷 B 门形：显式非 _main profile id 不被静默 remap（未解析显式 id 仍 error，快速失败） |
| UT-S16-32 | api::ui_protocol_transport::tests::fallback_enabled_profile_id_picks_first_enabled_for_main | 缺陷 B：裸 _main 引用在有 enabled profile 的 store 上回退到第一个 enabled（name 排序稳定）；无 store 时仍 None 保持原错误面 |

## 批 6（黑板 #11 — issue #10 会话切换身份-内容绑定，三面统一修复）

### UT-S16-33 ~ UT-S16-35 — octos-web vitest

| ID | 测试 | 描述 |
|----|------|------|
| UT-S16-33 | store/projection-store-switch-race.test.ts | 切换竞态守卫：beginSnapshot 同 key 重入拒绝（慢 A 不覆盖新 B 快照），释放后可重入 |
| UT-S16-34 | store/projection-store-hydrate-failed.test.ts | 静默失败标记：markHydrateFailed/isHydrateFailed/clearHydrateFailed 三面（失败标记而非空引导态） |
| UT-S16-35 | runtime/hydrate-projection.test.ts · issue #10 epoch | epoch 修复：hydrate user envelope meta 含 persisted_at，渲染兜底不渲染 epoch/序号 |

## 批 7（黑板 #13 — issue #11 治本：CM 种子 + PVC 可写覆盖合并）

### UT-S16-36 ~ UT-S16-40 — crates/octos-cli profiles（方案 c 合并/保存语义）

| ID | 测试函数 | 描述 |
|----|----------|------|
| UT-S16-36 | profiles::tests::overlay_get_returns_seed_when_no_overlay | 种子+无覆盖：get 返回种子原样 |
| UT-S16-37 | profiles::tests::overlay_get_merges_ui_marked_overlay_over_seed | 种子+带 managed_by=ui 覆盖：深合并，覆盖层 llm/env_vars 优先（UI 配 key 即用） |
| UT-S16-38 | profiles::tests::overlay_get_ignores_unmarked_leftover_seed_wins | 种子+无标记覆盖（历史残留）：种子胜（#7 防遮蔽语义保留） |
| UT-S16-39 | profiles::tests::overlay_save_to_readonly_seed_writes_marked_overlay | save 到只读 CM 种子路径：改写 override.json 并打 managed_by=ui + updated_at，get 合并生效 |
| UT-S16-40 | profiles::tests::overlay_save_to_writable_path_writes_seed_no_override | save 到可写路径：照旧写 <id>.json，不产生 override |

## 批 8（黑板 #15 — 修复 #13 缺陷：seed_readonly 写探测，挂载语义只读）

### UT-S16-41 ~ UT-S16-44 — crates/octos-cli profiles（写探测）

| ID | 测试函数 | 描述 |
|----|----------|------|
| UT-S16-41 | profiles::tests::probe_readonly_detects_chmod_444_readonly | 权限位只读（chmod 444）判只读：save 改写 override（保留 UT-S16-39 语义）；root 跳过 |
| UT-S16-42 | profiles::tests::probe_readonly_detects_mount_level_readonly_via_dir_0555 | **挂载层只读**（0644 可写位但 0555 目录/FS 拒绝写）判只读——#14 暴露的 `permissions().readonly()` 缺口，写探测覆盖；root 跳过 |
| UT-S16-43 | profiles::tests::probe_readonly_reports_writable_for_normal_path | 可写路径判可写：save 写 seed 不产生 override（保留 UT-S16-40 语义） |
| UT-S16-44 | profiles::tests::probe_readonly_cleans_up_probe_file | 写探测后 `.probe-readonly-*` 临时文件被清理（可写与只读两路径都不残留） |

## 批 9（黑板 #20 — 修复 #15 写探测盲区：目录可写 + subPath 单文件只读）

### UT-S16-45 ~ UT-S16-46 — crates/octos-cli profiles（目标文件写探测）

| ID | 测试函数 | 描述 |
|----|----------|------|
| UT-S16-45 | profiles::tests::probe_readonly_detects_subpath_file_ro_dir_writable | **subPath 盲区**（目录 0755 可写 + 目标文件 0444 只读）判只读：save 改写 override 而非 rename ro 种子——#19 铁证入案，#15 目录探测误判此形；root 跳过 |
| UT-S16-46 | profiles::tests::probe_readonly_target_probe_does_not_modify_seed | 目标文件 write(true) 探测不 create/truncate/写字节：探测前后 seed 内容字节不变 |

## 备注
- 批 1 的 12 项 + 批 3 的 1 项同处 `deploy_directory_layout.rs`（文件共 13 个测试函数），用例 ID 按批分别登记（UT-S16-01..12 属批 1，UT-S16-26 属批 3），文件物理共存不冲突。
- pg_persistence_matrix.rs 的 10 项全部为批 1 产物（其中 UT-S16-15/16/17 兼覆盖批 3 #7 防回退语义）。
- 批 2 的 3 项 vitest 中 UT-S16-25 为 #5 专属回归（新写），UT-S16-23/24 为既有 hydrate 基础用例（同批复验时一并计入）。
