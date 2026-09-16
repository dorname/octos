spec: task
name: "task-deploy-k8s"
tags: [deploy, k8s, install, docker]
---

## Intent

Provide a unified `deploy/` directory containing all deployment and
installation artifacts for octos — k8s manifests, docker-compose,
shell scripts, and comprehensive documentation. Eliminate the
scattered layout (yaml files at repo root, scripts with hardcoded
paths) that makes it hard for new operators to find what they need.

提供统一的 `deploy/` 目录，包含 octos 的所有部署和安装文件
（k8s manifests、docker-compose、shell 脚本、详细文档）。消除
仓库根目录散落 yaml 文件、脚本硬编码路径等不利于新运维
上手的问题。

## Decisions

D1: `deploy/` is the single root for all deployment-related artifacts,
    organized by deployment flavor:
    - `deploy/k8s/` — Kubernetes manifests
    - `deploy/docker/` — docker-compose stacks
    - `deploy/scripts/` — install/upgrade/uninstall shell scripts
    - `deploy/docs/` — installation guides and decision records

D2: k8s manifests are numbered for clarity:
    - `01-baseline.yaml` — single-node, hardcoded defaults, demo only
    - `02-hostpath-dev.yaml` — hostPath-mounted binary for dev loops
    - `03-cluster-with-config.yaml` — production: ConfigMap + Secret + init script

D3: The deployment script `deploy/scripts/deploy-k8s.sh` accepts a
    variant selector (baseline / hostpath / cluster) and points at the
    matching manifest.

D4: Documentation is split into three documents with non-overlapping
    scope:
    - `deploy/README.md` — overview + quick-start
    - `deploy/docs/K8S_INSTALL.md` — full k8s install guide
    - `deploy/docs/DEPLOYMENT_VARIANTS.md` — comparison and migration path

D5: 已确定的技术选择：
    - `deploy/` 是所有部署相关产物的统一根目录
    - 按部署方式分类组织（k8s/docker/scripts/docs）
    - k8s manifests 用编号前缀区分变体
    - 部署脚本支持 variant 选择
    - 文档按职责拆分（总览/安装/对比）

## Boundaries

### Allowed Changes

- `deploy/` directory and all subdirectories (new files, edits,
  renames, deletes within the directory).
- Cross-references in top-level docs that mention old file locations.
- 允许修改：
  - `deploy/` 目录及其子目录内的所有文件
  - 任何引用旧路径的顶层文档

### Forbidden

- Do NOT move `Dockerfile` (multi-stage build, stays at repo root).
- Do NOT modify runtime code in `crates/` to "fix" deployment issues —
  deployment concerns live in `deploy/` only.
- Do NOT remove existing k8s manifests without checking — they may
  still be referenced by external operators.
- Do NOT add a new top-level `deploy/` sibling (e.g. `deployments/`,
  `k8s-deploy/`) — `deploy/` is canonical.
- 禁止：
  - 移动 `Dockerfile`
  - 修改 `crates/` 里的运行时代码
  - 删除现有 k8s manifests 而不检查
  - 在顶层创建 `deploy/` 的同级目录

## Completion Criteria

Rule: layout — deployment artifacts live under deploy/

Scenario: All deployment artifacts live under deploy/
  Test:
    Package: octos-bus
    Filter: test_deploy_directory_layout
    Targets: deploy/ 目录布局
  Given the repository root after this task merges
  When the operator lists deployment-related files
  Then every yaml/sh/compose file referencing deployment lives under `deploy/`
  And the root no longer contains scattered `k8s-*.yaml`,
    `docker-compose*.yml`, or top-level deploy scripts
  Given the repository root after this task merges
  When the operator lists deployment-related files
  Then every yaml/sh/compose file referencing deployment lives under `deploy/`
  And the root no longer contains scattered `k8s-*.yaml`,
    `docker-compose*.yml`, or top-level deploy scripts

Scenario: Top-level is free of scattered deployment files
  Test:
    Package: octos-bus
    Filter: test_no_scattered_deploy_files
  Given the repository root
  When listing files matching k8s-*.yaml, docker-compose*.yml, deploy*.sh
  Then no such files exist at the repo root (deploy/ is the only home)

Rule: variant-script — deploy-k8s.sh supports baseline/hostpath/cluster

Scenario: deploy-k8s.sh supports all three variants
  Test:
    Package: octos-bus
    Filter: test_deploy_script_variants
  Given the repo at any commit after this task merges
  When the operator runs `./deploy/scripts/deploy-k8s.sh baseline`
    or `./deploy/scripts/deploy-k8s.sh hostpath`
    or `./deploy/scripts/deploy-k8s.sh cluster`
  Then the script resolves a manifest under `deploy/k8s/` and prints it

Scenario: Unknown variant exits with usage hint (error path)
  Test:
    Package: octos-bus
    Filter: test_deploy_script_unknown_variant
  Given the repo at any commit after this task merges
  When the operator runs `./deploy/scripts/deploy-k8s.sh bogus-variant`
  Then the script exits non-zero
  And stdout contains "Usage:" with the list of valid variants

Scenario: deploy-k8s.sh passes bash -n syntax check
  Test:
    Package: octos-bus
    Filter: test_deploy_script_syntax
  Given the repo at any commit after this task merges
  When the operator runs `bash -n deploy/scripts/deploy-k8s.sh`
  Then exit status is zero

Rule: docs — K8S_INSTALL.md covers mandatory sections

Scenario: K8S_INSTALL.md contains all mandatory headings
  Test:
    Package: octos-bus
    Filter: test_k8s_install_doc_sections
  Given `deploy/docs/K8S_INSTALL.md`
  When the operator searches for required section headings
  Then the document contains: 架构概览 / 前置条件 / 镜像构建 /
    部署变体 / 配置注入 / 部署步骤 / 验证 / 故障排查 / 升级与回滚

Rule: layered — installation guide has prerequisites

Scenario: Prerequisites cover cluster, images, network, kubectl
  Test:
    Package: octos-bus
    Filter: test_k8s_install_prerequisites
  Given `deploy/docs/K8S_INSTALL.md`
  When the operator reads the 前置条件 section
  Then cluster requirements, image list, port table, and kubectl check
    commands are all documented

Rule: layered — installation guide covers troubleshooting

Scenario: Troubleshooting covers common failure modes
  Test:
    Package: octos-bus
    Filter: test_k8s_install_troubleshooting
  Given `deploy/docs/K8S_INSTALL.md`
  When the operator reads the 故障排查 section
  Then ContainerCreating hang, CrashLoopBackOff, missing API key,
    PG migrations not running, and hostPath on multi-node are all covered

场景: 所有部署文件在 deploy/ 下
  测试:
    包: octos-bus
    过滤: test_deploy_directory_layout
  假设 仓库在本任务合并后的根目录
  当 运维列出部署相关文件
  那么 所有 yaml/sh/compose 文件都在 `deploy/` 下
  并且 根目录没有散落的 `k8s-*.yaml`、`docker-compose*.yml` 或顶层部署脚本

场景: 根目录没有散落的部署文件
  测试:
    包: octos-bus
    过滤: test_no_scattered_deploy_files
  假设 仓库根目录
  当 列出匹配 k8s-*.yaml, docker-compose*.yml, deploy*.sh 的文件
  那么 根目录没有此类文件（deploy/ 是唯一存放位置）

场景: deploy-k8s.sh 支持三种变体
  测试:
    包: octos-bus
    过滤: test_deploy_script_variants
  假设 仓库在本任务合并后的任何 commit
  当 运维运行 `./deploy/scripts/deploy-k8s.sh baseline|hostpath|cluster`
  那么 脚本解析 `deploy/k8s/` 下的 manifest 并打印

场景: 未知变体非零退出并提示用法（错误路径）
  测试:
    包: octos-bus
    过滤: test_deploy_script_unknown_variant
  假设 仓库在本任务合并后的任何 commit
  当 运维运行 `./deploy/scripts/deploy-k8s.sh bogus-variant`
  那么 脚本非零退出
  并且 stdout 包含 "Usage:" 和有效变体列表

场景: deploy-k8s.sh 通过 bash -n 语法检查
  测试:
    包: octos-bus
    过滤: test_deploy_script_syntax
  假设 仓库在本任务合并后的任何 commit
  当 运维运行 `bash -n deploy/scripts/deploy-k8s.sh`
  那么 退出状态为 0

场景: K8S_INSTALL.md 包含所有必备章节
  测试:
    包: octos-bus
    过滤: test_k8s_install_doc_sections
  假设 `deploy/docs/K8S_INSTALL.md`
  当 运维搜索必备章节标题
  那么 文档包含：架构概览 / 前置条件 / 镜像构建 / 部署变体 /
    配置注入 / 部署步骤 / 验证 / 故障排查 / 升级与回滚

场景: 前置条件覆盖集群、镜像、网络、kubectl
  测试:
    包: octos-bus
    过滤: test_k8s_install_prerequisites
  假设 `deploy/docs/K8S_INSTALL.md`
  当 运维阅读 前置条件 章节
  那么 集群要求、镜像列表、端口表和 kubectl 检查命令都有文档

场景: 故障排查覆盖常见失败模式
  测试:
    包: octos-bus
    过滤: test_k8s_install_troubleshooting
  假设 `deploy/docs/K8S_INSTALL.md`
  当 运维阅读 故障排查 章节
  那么 ContainerCreating 卡住、CrashLoopBackOff、缺失 API key、
    PG migrations 未跑、hostPath 多节点问题都被覆盖

## Out of Scope

- Helm chart packaging (`deploy/helm/`).
- Terraform / Pulumi IaC for cloud-provisioned k8s.
- Image registry push automation (CI builds + pushes the
  `octos:k8s-stateless` image; this task only documents the
  manual `docker build` step).
- New runtime features (this task is layout-only).
- 不在本任务范围内：
  - Helm chart 打包
  - Terraform / Pulumi IaC
  - 镜像 registry 自动推送
  - 新运行时功能（本任务仅整理布局）