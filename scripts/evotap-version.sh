#!/bin/bash
# Print evotap's own version: the one in bins/evotap/Cargo.toml, which
# crates/tap/Cargo.toml must carry too (the readers are built from it).
# evotap moves at its own pace, not Glass's: the number changes when the
# tap changes, and a release of evotap is cut when the number does.
set -euo pipefail
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
lib=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/bins/evotap/Cargo.toml" | head -n1)
readers=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/crates/tap/Cargo.toml" | head -n1)
[ -n "$lib" ] || { echo "evotap-version: no version in bins/evotap/Cargo.toml" >&2; exit 1; }
[ "$lib" = "$readers" ] || { echo "evotap-version: bins/evotap/Cargo.toml says $lib, crates/tap/Cargo.toml says $readers; they are one version" >&2; exit 1; }
echo "$lib"
