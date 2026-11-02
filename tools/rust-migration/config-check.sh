#!/usr/bin/env bash
set -euo pipefail
CONFIG_FILE="${1:?pakai: config-check.sh <out/.config> CONFIG_...}"
shift
for opt in "$@"; do
  if grep -qE "^${opt}=" "$CONFIG_FILE" 2>/dev/null; then
    echo "$opt=$(grep -E "^${opt}=" "$CONFIG_FILE" | cut -d= -f2)"
  elif grep -qE "^# ${opt} is not set" "$CONFIG_FILE" 2>/dev/null; then
    echo "$opt=is not set"
  else
    if grep -rq "config ${opt#CONFIG_}" --include=Kconfig* . 2>/dev/null; then
      echo "$opt=ada di Kconfig, tidak ada di .config"
    else
      echo "$opt=tidak ada di Kconfig"
    fi
  fi
done
