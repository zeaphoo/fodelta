#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN="${FODELTA_BIN:-$ROOT/target/release/fodelta}"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

fail() { echo "e2e failed: $*" >&2; exit 1; }
expect() { printf '%s\n' "$2" | grep -Fq -- "$1" || fail "missing '$1' in: $2"; }

cd "$WORK"
"$BIN" init >/dev/null

printf 'v1\n' > a.txt
printf 'b1\n' > b.txt
"$BIN" commit -m initial >/dev/null

printf 'v2\n' > a.txt
printf 'b2\n' > b.txt
OUT=$("$BIN" commit -m "only a" -- a.txt)
expect "1 file(s) changed" "$OUT"
expect "1 change(s) remain" "$OUT"

OUT=$("$BIN" status)
expect "modified: b.txt" "$OUT"

"$BIN" commit -m all >/dev/null
OUT=$("$BIN" status)
expect "working tree clean" "$OUT"

OUT=$("$BIN" log --oneline)
[ "$(printf '%s\n' "$OUT" | wc -l)" -eq 3 ] || fail "expected 3 commits"

OUT=$("$BIN" show HEAD:a.txt)
[ "$OUT" = "v2" ] || fail "show blob returned '$OUT'"

OUT=$("$BIN" diff HEAD~1 HEAD)
expect "b/b.txt" "$OUT"

"$BIN" restore HEAD~2 -- a.txt >/dev/null
OUT=$("$BIN" status)
expect "modified: a.txt" "$OUT"

"$BIN" restore -- a.txt >/dev/null
OUT=$("$BIN" status)
expect "working tree clean" "$OUT"

printf 'dirty\n' > a.txt
OUT=$("$BIN" restore -- a.txt)
expect "safety snapshot" "$OUT"
SAFETY=$(printf '%s\n' "$OUT" | awk '/safety snapshot/{print $3}')
[ -n "$SAFETY" ] || fail "no safety id"

OUT=$("$BIN" safety)
expect "$SAFETY" "$OUT"

"$BIN" rollback HEAD~1 >/dev/null
OUT=$("$BIN" log --oneline)
[ "$(printf '%s\n' "$OUT" | wc -l)" -eq 4 ] || fail "expected 4 commits after rollback"

OUT=$("$BIN" status)
expect "working tree clean" "$OUT"

"$BIN" gc --dry-run >/dev/null
"$BIN" fsck >/dev/null
"$BIN" diff --exit-code >/dev/null || fail "diff --exit-code should be clean"

echo "e2e: ok"
