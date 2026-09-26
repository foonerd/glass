#!/bin/bash
# The workshop pass CI runs: formatting, lints with warnings denied, tests,
# and the documentation with warnings denied. Run it before a commit.
set -euo pipefail
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$ROOT"

echo "check: format"
cargo fmt --all -- --check

echo "check: clippy"
cargo clippy --workspace --all-targets --locked -- -D warnings

echo "check: tests"
cargo test --workspace --locked

echo "check: documentation"
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked

echo "check: ALSA templates"
scripts/asound_check.sh

echo "check: plugin"
if command -v node >/dev/null 2>&1; then
  node --check plugin/index.js
  node --test plugin/manager/test/*.test.js
elif command -v docker >/dev/null 2>&1; then
  docker run --rm -v "$ROOT/plugin:/plugin:ro" -w /plugin node:20-slim sh -c 'node --check index.js && node --test manager/test/*.test.js'
else
  echo "check: plugin: neither node nor docker found, skipped" >&2
fi

echo "check: clean"
