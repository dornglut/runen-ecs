#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
toolchain="nightly-2026-08-25"
target="x86_64-unknown-linux-gnu"

cd "$repo_root"
export RUSTFLAGS=-Zsanitizer=thread
export TSAN_OPTIONS=halt_on_error=1:abort_on_error=1:report_signal_unsafe=0

cargo +"$toolchain" test \
  -Zbuild-std \
  --target "$target" \
  -p runen-ecs --test tsan_smoke --locked -- --nocapture

cargo +"$toolchain" test \
  -Zbuild-std \
  --target "$target" \
  -p runen-ecs --lib system::concurrent::tests --locked -- --nocapture
