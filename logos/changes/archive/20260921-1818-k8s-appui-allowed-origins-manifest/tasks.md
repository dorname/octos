# 实现任务

## [delta] 规格变更
- [x] 无（配置级：Deployment env 值追加 + 文档，不改对外功能契约形状）

## [code] 代码实现
- [x] deploy/k8s/03-cluster-with-config.yaml: OCTOS_APPUI_ALLOWED_ORIGINS 加 5174（含 localhost 变体）+ 注释注明 PF/5174 须显式列出
- [x] deploy/docs/K8S_DEPLOY_PROVEN.md: kubectl set env 示例加 5174；新增 env 语义段（空/未设=仅无 Origin 头通道放行，WS 必带 Origin 须显式白名单；勿只 live set env 滚动即丢）

## 验收
- YAML 语法校验 OK（python3 yaml.safe_load_all，8 docs）
- 无 UT（纯清单+文档，proposal 已说明理由）
- 部署+真机验收由外环在 #14 重做时执行（本提案不部署）
