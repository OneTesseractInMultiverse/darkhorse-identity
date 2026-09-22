#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR="$PWD/target/integration-coverage"
# Source only the pinned local instrumentation tool's shell environment.
eval "$(cargo llvm-cov show-env --sh)"
cargo llvm-cov clean --workspace
cargo test --workspace --lib --locked --offline
"${NODE:-node}" scripts/postgres-test.mjs
"${NODE:-node}" scripts/cli-test.mjs
"${PYTHON:-python3}" scripts/cli-terminal-test.py
"${NODE:-node}" scripts/redis-test.mjs
cargo llvm-cov report --html
cargo llvm-cov report --summary-only --show-missing-lines --fail-under-lines 100
