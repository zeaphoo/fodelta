#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN="${FODELTA_BIN:-$ROOT/target/release/fodelta}"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

cd "$WORK"
python3 - <<'PY'
import os
os.makedirs("data", exist_ok=True)
for i in range(20_000):
    with open(f"data/file-{i}.txt", "w", encoding="utf-8") as f:
        f.write(f"contents {i}\n")
        for j in range(5):
            f.write(f"line {i}-{j}\n")
PY

"$BIN" init >/dev/null
/usr/bin/time -f "commit:   %e s" "$BIN" commit -m "20k files" >/dev/null
/usr/bin/time -f "status:   %e s" "$BIN" status >/dev/null
set +e
/usr/bin/time -f "noop:     %e s" -o /tmp/fodelta-noop.time "$BIN" commit -m noop >/dev/null 2>/dev/null
cat /tmp/fodelta-noop.time
rm -f /tmp/fodelta-noop.time
set -e
/usr/bin/time -f "commit-x: %e s" sh -c "printf 'x\n' >> data/file-42.txt && '$BIN' commit -m select -- data/file-42.txt >/dev/null"
/usr/bin/time -f "log:      %e s" "$BIN" log -n 50 --oneline >/dev/null
"$BIN" fsck >/dev/null
echo "perf: ok"
