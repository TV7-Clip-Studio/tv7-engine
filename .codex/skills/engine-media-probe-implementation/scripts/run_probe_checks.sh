#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
cd "$repo_root"

echo "[engine-media-probe] cargo check -p engine-core"
cargo check -p engine-core

echo "[engine-media-probe] cargo test -p engine-core"
cargo test -p engine-core

echo "[engine-media-probe] cargo clippy -p engine-core"
cargo clippy -p engine-core --all-targets --all-features -- -D warnings
