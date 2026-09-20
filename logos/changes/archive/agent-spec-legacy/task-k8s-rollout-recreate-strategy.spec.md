spec: task
name: "task-k8s-rollout-recreate-strategy"
tags: [deploy, k8s, bugfix, critical]
---

## Intent

Fix the stuck octos rollout on docker-desktop K8s and its user-visible
symptom: the SPA at `http://localhost:9091/app/chat` reports "Unable to
establish the UI Protocol connection" because (a) the
`kubectl port-forward` died when its target pod was replaced, and (b) the
rollout that replaced it wedged — the new ReplicaSet pod crash-loops on
`OCTOS_DATA_DIR_LOCKED` while the old pod keeps serving.

Root cause of the wedge: the octos Deployment uses the default
`RollingUpdate` strategy (`maxSurge: 25%`). With `replicas: 1` plus a
ReadWriteOnce PVC at `/tmp/octos-data` and octos's data-dir lockfile, any
`kubectl rollout restart` or template change starts the new pod BEFORE the
old pod terminates; the new pod sees the lock, exits, and never becomes
Ready, so the rollout never completes. 问题 5 in
`deploy/docs/K8S_DEPLOY_PROVEN.md` documented this lock conflict for
`replicas: 2` but the same constraint forbids two concurrent pods during
ANY rollout, even at `replicas: 1`.

修复 docker-desktop K8s 上卡住的 octos rollout 及其用户可见症状
（前端 WS 连接失败）。根因：Deployment 默认 RollingUpdate
(maxSurge 25%) 在 replicas=1 + ReadWriteOnce PVC + data-dir lockfile
下，rollout 期间新 pod 先于旧 pod 退出启动，撞上
`OCTOS_DATA_DIR_LOCKED` 后 CrashLoopBackOff，rollout 永远无法完成。
必须把 strategy 改为 `Recreate`（先杀旧 pod 再起新 pod）。

## Decisions

D1: The octos Deployment in `deploy/k8s/03-cluster-with-config.yaml`
    declares an explicit `spec.strategy` with `type: Recreate`. Recreate
    terminates the old pod fully (releasing the data-dir lockfile and the
    RWO PVC) before the new pod starts. The brief downtime is acceptable
    for single-node local deployment; multi-replica HA is explicitly out
    of scope (needs RWX storage + coordination, per the deploy doc).

D2: `replicas: 1` stays unchanged. Recreate — not more replicas — is the
    fix; the lockfile forbids two concurrent servers on the same data dir
    regardless of replica count.

D3: The pg Deployment is untouched: it has its own PVC and no octos
    lockfile, and its rollout behavior is not part of this bug.

D4: `deploy/docs/K8S_DEPLOY_PROVEN.md` gains a "问题 6" entry recording
    this failure mode (RollingUpdate surge + RWO lock → stuck rollout) and
    a "问题 7" entry recording that `kubectl port-forward` to a Service
    dies when its target pod is replaced, so the runbook must restart the
    port-forward after every rollout. Both entries include the fix and the
    verification commands.

D5: 已确定的技术选择：
    - octos Deployment 显式声明 `strategy: type: Recreate`
    - `replicas: 1` 保持不变
    - pg Deployment 不在本次改动范围
    - 部署记录文档补充问题 6（rollout 锁冲突）与问题 7
      （port-forward 随 pod 替换死亡，需随 rollout 重启）

## Boundaries

### Allowed Changes

- specs/task-k8s-rollout-recreate-strategy.spec.md
- deploy/k8s/03-cluster-with-config.yaml
- deploy/docs/K8S_DEPLOY_PROVEN.md
- crates/octos-bus/tests/deploy_directory_layout.rs

### Forbidden

- Do NOT change `replicas` (stays 1) or switch the PVC to a different
  access mode.
- Do NOT modify the pg Deployment, Services, ConfigMap init script
  semantics, or Secret wiring.
- Do NOT modify any runtime code under `crates/*/src/` — this is a
  deployment-manifest fix; only the test file under
  `crates/octos-bus/tests/` may change.
- Do NOT weaken or delete existing manifest assertions in
  `pg_persistence_matrix.rs` or `deploy_directory_layout.rs`.
- 禁止：改 replicas、动 pg/Service/ConfigMap/Secret、改 crates 运行时
  源码、削弱既有 manifest 断言。

## Completion Criteria

Rule: recreate-strategy — octos Deployment rolls out pod-by-pod with no overlap

Scenario: octos Deployment declares Recreate strategy
  标签: critical
  Test:
    Package: octos-bus
    Filter: test_octos_deployment_uses_recreate_strategy
    Targets: deploy/k8s/03-cluster-with-config.yaml 的 octos Deployment
  Given the manifest at `deploy/k8s/03-cluster-with-config.yaml`
  When the octos Deployment spec is inspected
  Then `spec.strategy.type` is `Recreate`
  And the strategy block contains no `rollingUpdate` surge settings

Scenario: Default RollingUpdate surge would reintroduce the lock conflict (error path)
  Test:
    Package: octos-bus
    Filter: test_octos_deployment_has_no_rolling_update_surge
    Targets: octos Deployment 无 rollingUpdate 配置块
  Given the manifest at `deploy/k8s/03-cluster-with-config.yaml`
  When the octos Deployment spec is inspected
  Then it does NOT rely on the implicit default RollingUpdate strategy
  And no `maxSurge`/`maxUnavailable` rollingUpdate block exists for the
    octos Deployment, so two octos pods can never race the data-dir lock

Scenario: Replica count stays at 1 while the PVC is ReadWriteOnce (error path)
  Test:
    Package: octos-bus
    Filter: test_octos_deployment_single_replica_for_rwo_pvc
    Targets: octos Deployment replicas 与 octos-data volume
  Given the manifest at `deploy/k8s/03-cluster-with-config.yaml`
  When the octos Deployment spec is inspected
  Then `spec.replicas` is exactly 1
  And the octos-data volume remains a persistentVolumeClaim, so the
    single-writer invariant (lockfile + RWO) is preserved

Rule: deploy-doc — the deployment record captures this failure mode and the port-forward runbook

Scenario: K8S_DEPLOY_PROVEN.md documents the Recreate fix and port-forward restart step
  Test:
    Package: octos-bus
    Filter: test_k8s_deploy_proven_doc_records_recreate_and_portforward
    Targets: deploy/docs/K8S_DEPLOY_PROVEN.md
  Given the deployment record at `deploy/docs/K8S_DEPLOY_PROVEN.md`
  When an operator hits a stuck rollout or a dead localhost:9091
  Then the document contains a problem entry explaining the RollingUpdate
    surge vs data-dir lock conflict and its `strategy: Recreate` fix
  And the document instructs restarting `kubectl port-forward` after every
    rollout because the forward dies when its target pod is replaced
