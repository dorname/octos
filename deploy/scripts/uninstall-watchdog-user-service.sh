#!/usr/bin/env bash
# 卸载 S17 Watchdog unit 与配置；监督状态及项目事实源默认保留。
set -euo pipefail

usage() {
  echo "用法：$0 --project /absolute/project [--keep-state]" >&2
}

PROJECT=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --project)
      PROJECT="${2:-}"
      shift 2
      ;;
    --keep-state)
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

if [[ -z "$PROJECT" || ! -d "$PROJECT" ]]; then
  usage
  exit 2
fi
PROJECT="$(realpath "$PROJECT")"
CONFIG_HOME="${XDG_CONFIG_HOME:-${HOME}/.config}"
PROJECT_ID="$(printf '%s' "$PROJECT" | sha256sum | cut -c1-24)"
UNIT="octos-watchdog@${PROJECT_ID}.service"
UNIT_FILE="${CONFIG_HOME}/systemd/user/octos-watchdog@.service"
CONFIG_FILE="${CONFIG_HOME}/octos/watchdog/${PROJECT_ID}.toml"

systemctl --user disable --now "$UNIT" >/dev/null 2>&1 || true
rm -f "$CONFIG_FILE" "$UNIT_FILE"
systemctl --user daemon-reload

echo "已卸载 ${UNIT}；监督 state/alerts 与项目黑板、goal ledger、checkpoint、Git 提交均已保留。"
