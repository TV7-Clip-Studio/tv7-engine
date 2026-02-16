#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
cd "$repo_root"

echo "[engine-sidecar-bootstrap] cargo check"
cargo check

echo "[engine-sidecar-bootstrap] cargo test"
cargo test

echo "[engine-sidecar-bootstrap] cargo clippy"
cargo clippy --all-targets --all-features -- -D warnings
