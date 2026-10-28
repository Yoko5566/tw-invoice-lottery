#!/usr/bin/env bash
# 技能入口：呼叫同目錄或 PATH 上的 twiv 執行檔。
# 用法：./run.sh check AB12345678 --period 11504
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"

for cand in "$HERE/twiv" "$HERE/twiv.exe" "$(command -v twiv || true)"; do
  if [ -n "${cand:-}" ] && [ -x "$cand" ]; then
    exec "$cand" "$@"
  fi
done

cat >&2 <<'MSG'
找不到 twiv 執行檔。
請把編譯好的執行檔放到 skill/scripts/ 目錄下，或加入 PATH：
  cargo build --release -p invoice-bin
  cp target/release/twiv skill/scripts/
MSG
exit 127
