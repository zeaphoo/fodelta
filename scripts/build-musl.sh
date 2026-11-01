#!/usr/bin/env bash
# Build a statically linked Linux binary with musl.
set -euo pipefail

target="${1:-x86_64-unknown-linux-musl}"
cargo build --release --target "$target"
echo "built target/$target/release/fodelta"
