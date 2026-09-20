#!/usr/bin/env bash
# 安装 S17 Watchdog 的 systemd user unit 与项目配置；默认不启用服务。
set -euo pipefail

usage() {
  echo "用法：$0 --project /absolute/project [--octos-bin /absolute/octos]" >&2
}

PROJECT=""
OCTOS_BIN=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --project)
      PROJECT="${2:-}"
      shift 2
      ;;
    --octos-bin)
      OCTOS_BIN="${2:-}"
      shift 2
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
if [[ "$PROJECT" != /* ]]; then
  echo "project 必须解析为绝对路径" >&2
  exit 2
fi

if [[ -z "$OCTOS_BIN" ]]; then
  OCTOS_BIN="$(command -v octos || true)"
fi
if [[ -z "$OCTOS_BIN" || ! -x "$OCTOS_BIN" ]]; then
  echo "找不到可执行 octos；请传 --octos-bin" >&2
  exit 2
fi
OCTOS_BIN="$(realpath "$OCTOS_BIN")"

CONFIG_HOME="${XDG_CONFIG_HOME:-${HOME}/.config}"
STATE_HOME="${XDG_STATE_HOME:-${HOME}/.local/state}"
UNIT_DIR="${CONFIG_HOME}/systemd/user"
WATCHDOG_CONFIG_DIR="${CONFIG_HOME}/octos/watchdog"
PROJECT_ID="$(printf '%s' "$PROJECT" | sha256sum | cut -c1-24)"
CONFIG_FILE="${WATCHDOG_CONFIG_DIR}/${PROJECT_ID}.toml"
UNIT_FILE="${UNIT_DIR}/octos-watchdog@.service"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEMPLATE="${SCRIPT_DIR}/../systemd/octos-watchdog@.service.in"

if [[ ! -f "$TEMPLATE" ]]; then
  echo "unit 模板不存在：$TEMPLATE" >&2
  exit 1
fi

mkdir -p "$UNIT_DIR" "$WATCHDOG_CONFIG_DIR" "${STATE_HOME}/octos/watchdog/${PROJECT_ID}"
chmod 700 "$WATCHDOG_CONFIG_DIR" "${STATE_HOME}/octos/watchdog/${PROJECT_ID}"

if [[ ! -f "$CONFIG_FILE" ]]; then
  {
    printf 'version = 1\n'
    printf 'project = "%s"\n' "${PROJECT//\/\\}"
    printf 'poll_interval_secs = 5\n'
    printf 'idle_after_secs = 120\n'
    printf 'observation_window_secs = 120\n'
    printf 'max_no_progress_retries = 3\n\n'
    printf '[sources]\n'
    printf 'board = ".octos/OUTER_LOOP_REVIEW.md"\n\n'
    printf '[agents]\n'
    printf 'inner_kind = "octoscode"\n'
    printf 'outer_kind = "claude"\n\n'
    printf '[alerts]\n'
    printf 'stderr = true\n'
  } >"$CONFIG_FILE"
fi
chmod 600 "$CONFIG_FILE"

escape_unit() {
  printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' -e 's/%/%%/g'
}

BIN_ESC="$(escape_unit "$OCTOS_BIN")"
CONFIG_ESC="$(escape_unit "$CONFIG_HOME")"
STATE_ESC="$(escape_unit "$STATE_HOME")"
PATH_ESC="$(escape_unit "$PATH")"
sed \
  -e "s|@OCTOS_BIN@|${BIN_ESC}|g" \
  -e "s|@CONFIG_HOME@|${CONFIG_ESC}|g" \
  -e "s|@STATE_HOME@|${STATE_ESC}|g" \
  -e "s|@EXEC_PATH@|${PATH_ESC}|g" \
  "$TEMPLATE" >"$UNIT_FILE"
chmod 600 "$UNIT_FILE"

systemctl --user daemon-reload

LINGER="$(loginctl show-user "${USER}" -p Linger --value 2>/dev/null || true)"
if [[ "$LINGER" != "yes" ]]; then
  echo "提示：当前用户 linger 未启用；登出后是否继续运行由 operator 决定，本脚本不会修改。" >&2
fi

echo "已安装但未启用：octos-watchdog@${PROJECT_ID}.service"
echo "配置：${CONFIG_FILE}"
echo "显式启用：systemctl --user enable --now octos-watchdog@${PROJECT_ID}.service"
