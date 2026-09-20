#!/usr/bin/env bash
# OLP 黑板原子追加助手(随仓库发行版)。
#
# Mirror 注记(2026-09-20,提案 olp-board-scaffold):本脚本与 octoscode 仓库
# scripts/olp-board-append.sh 同源。收录本仓的原因:外环黑板
# .octos/OUTER_LOOP_REVIEW.md 的写入方(外环/内环)在 octos 工作区内即可达
# 本脚本,而 octoscode 仓库路径在内环沙箱视图不可达(黑板 #1 握手实证)。
# 上游语义变更时以 octoscode 版为准同步本 mirror。
#
# 外环写黑板的唯一正道:flock 互斥 + 整条目一次性追加,防多写者交错与
# 撞号。条目正文从 stdin 喂入。
#
# 用法:
#   scripts/olp-board-append.sh <board.md>  <<'EOF'
#   ### <编号>. <标题>(<日期>,<署名>)
#   ...正文...
#   EOF
#
# 注:若你是常驻外环且自建了"自写登记"机制(监视器抵扣自触发),请在
# 你自己的包装脚本里做登记——本脚本保持纯追加,保证其他外环的监视器
# 能感知你的写入。
set -euo pipefail
BOARD="${1:?用法: olp-board-append.sh <board.md> (正文从 stdin 喂)}"
LOCK="${BOARD}.lock"
TMP=$(mktemp)
trap 'rm -f "$TMP"' EXIT
cat > "$TMP"
exec 9>"$LOCK"
flock -x 9
cat "$TMP" >> "$BOARD"
