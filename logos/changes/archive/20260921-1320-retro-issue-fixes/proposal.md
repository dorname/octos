# 变更提案：retro-issue-fixes

> module: core | created: 2026-09-21
> **性质：追溯补建（retroactive）**——本提案管辖的改动已在此前落地，当时未走 OpenLogos 变更流程；本提案补齐追溯链，使 guard/提案/归档与 git 历史对齐。operator 2026-09-21 指令补建。

## 变更原因

fork dorname/octos 的 7 个 OPEN issue 分三批修复（operator goal 指令「拉取 issue 开始修复」，黑板条目 #4/#5/#6 派单，内环 octoscode 执行，外环逐批复验采认），全部 issue 闭环。但三批的落库改动均未走 openlogos change 提案流程（直接 commit），违反「每次变更必须先创建 logos/changes/ 变更提案」的项目约定。operator 于 2026-09-21 增设内环流程硬约束（通告三）并指令对既往未走流程的 issue 修复补建提案。

## 变更类型

代码级（文档/清单/前端/测试修复；无需求/规格/API 层变更）

## 变更范围（已落地改动清单，追溯管辖）

- **批 1 · deploy/k8s 文档与配置族（issue #1 #2 #3）** — commit `371d9391`：
  - K8S_INSTALL.md/K8S_DEPLOY_PROVEN.md：WSL binary 提供方式专节（三层路径语义对照 + 宿主 HTTP :8088/:18088 + init wget 流程）；PVC 残留 profile 覆盖语义与三档清理；部署后 Smoke（SMOKE-S16）入口与 4 项判定表；修正「octos migrate 不存在子命令」陈旧排查建议
  - 03-cluster-with-config.yaml：Secret PLACEHOLDER GUARD 注释
  - deploy-k8s.sh：cluster 变体 REPLACE_ME 残留拒发校验
  - 测试：deploy_directory_layout 12/12 + pg_persistence_matrix 10/10（外环独立重跑一致）；#3 前半项核销（fcacde31→0b7ad0c9 证据链）
- **批 2 · octos-web 前端族（issue #4 #5）** — submodule `8cea119`+`ec44326`、主仓指针 `bfde941e`：
  - #4：package.json engines.node>=22.13 + README Node 版本下限与 Node 20 npm fallback
  - #5：hydrate-projection 三键全缺 legacy 行不丢弃（message_id 兜底 + hydrate-legacy 共享车道）+ 回归测试（vitest 3/3 外环独立复现）
- **批 3 · AppUI 真机族（issue #7 #8）** — commit `d284997a`：
  - #7：cluster-worker-profile ConfigMap 真相源 + readOnly subPath 挂载 + replicas=1；断言 test_octos_deployment_mounts_cluster_worker_profile_configmap（13/13 外环独立重跑）；真机跨滚动验证
  - #8：分类结案（部署面同根于 #7，量化证据 WS 24ms/hello 11ms/open 10ms + live secret REPLACE_ME 佐证），无代码改动
- 全部 issue 回帖已发（-R dorname/octos）

## 部署影响

- 是否需要部署：是（已随修复过程在本地 docker-desktop octos ns 真机验证：集群恢复 2/2→1/1 Ready、#7 跨滚动配置保留）
- 部署原因：S16 集群路径的行为性修复已在修复时验证；本提案为追溯补档，不引入新部署
- 影响环境：本地（docker-desktop/kind）
- 是否涉及数据迁移：否
- 是否需要回滚预案：否（逐 commit 可回退）
- 是否需要 smoke：否（S16 smoke 体系已覆盖；#7 有真机跨滚动验证）

## 变更概述

对三批 issue 修复（7 issue 全闭环：修复 6 + 分类结案 1）的已落地 commit 建立追溯提案管辖，补齐 OpenLogos 流程链。所有改动的独立复验证据（测试重跑、真机核对、回帖核对）已在黑板条目 #4/#5/#6 的外环采认批注中留档。
