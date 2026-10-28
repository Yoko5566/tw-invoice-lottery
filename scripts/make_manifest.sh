#!/usr/bin/env bash
# 產生更新清單 data/manifest.json
#
# 用法：./scripts/make_manifest.sh <raw-base-url>
# 例：  ./scripts/make_manifest.sh https://raw.githubusercontent.com/Yoko5566/tw-invoice-lottery-data/main

set -euo pipefail
BASE="${1:-https://raw.githubusercontent.com/Yoko5566/tw-invoice-lottery-data/main}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DATA="$ROOT/data"

sha() { if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1
        else shasum -a 256 "$1" | cut -d' ' -f1; fi; }

DS="$DATA/draws.json"
[ -f "$DS" ] || { echo "找不到 $DS，先執行 twiv fetch"; exit 1; }

DS_SHA=$(sha "$DS"); DS_BYTES=$(wc -c < "$DS" | tr -d ' ')
LATEST=$(python3 - "$DS" <<'PY'
import json,sys
d=json.load(open(sys.argv[1]))
print(d["periods"][0]["label"] if d.get("periods") else "")
PY
)

CLOUD_JSON="[]"
for f in "$DATA"/cloud_*.bin; do
  [ -e "$f" ] || continue
  name=$(basename "$f" .bin); period="${name#cloud_}"
  c_sha=$(sha "$f"); c_bytes=$(wc -c < "$f" | tr -d ' ')
  entry=$(printf '{"period":"%s","url":"%s/data/%s.bin","bytes":%s,"sha256":"%s"}' \
    "$period" "$BASE" "$name" "$c_bytes" "$c_sha")
  if [ "$CLOUD_JSON" = "[]" ]; then CLOUD_JSON="[$entry]"; else CLOUD_JSON="${CLOUD_JSON%]},$entry]"; fi
done

cat > "$DATA/manifest.json" <<JSON
{
  "schema": 1,
  "app_version": "$(grep -m1 '^version' "$ROOT/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')",
  "generated_at": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "period_latest": "$LATEST",
  "dataset_url": "$BASE/data/draws.json",
  "dataset_bytes": $DS_BYTES,
  "dataset_sha256": "$DS_SHA",
  "cloud": $CLOUD_JSON
}
JSON
echo "已產生 $DATA/manifest.json"
cat "$DATA/manifest.json"
