# core-S16 smoke 测试用例

> 关联场景：S16（K8s 多副本无状态化）  
> 关联部署方案：`core-01-deployment-plan.md` §八  
> 环境：本地 docker-desktop / namespace `octos`

## 用例列表

| ID | 检查项 | 期望 | 类型 |
|----|--------|------|------|
| SMOKE-S16-01 | cluster Service 可达 | Pod Ready，`GET /health` 200 | auto |
| SMOKE-S16-02 | version 探活 | `GET /api/version` 200 且返回 service=octos | auto |
| SMOKE-S16-03 | PG 迁移 | `\dt` 含 sessions / session_events / approvals / run_leases 等表 | auto |
| SMOKE-S16-04 | 删 Pod 后续活 | 删除 octos Pod 后 Deployment 恢复 Ready，health 再次 200 | auto |

## Runner

```bash
./scripts/smoke-s16-k8s.sh
```

结果写入 `logos/resources/verify/smoke-results.jsonl`（或 `OPENLOGOS_SMOKE_RESULT_PATH`）。
