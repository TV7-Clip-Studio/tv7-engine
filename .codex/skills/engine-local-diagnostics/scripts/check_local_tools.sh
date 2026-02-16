#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
cd "$repo_root"

required_tools=(
  "rustc"
  "cargo"
)

optional_tools=(
  "ffmpeg"
  "ffprobe"
)

echo "Checking required tools..."
for tool in "${required_tools[@]}"; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "MISSING: $tool"
    exit 1
  fi
  echo "FOUND: $tool -> $(command -v "$tool")"
  "$tool" --version | head -n 1
done

echo
echo "Checking optional media tools..."
for tool in "${optional_tools[@]}"; do
  if command -v "$tool" >/dev/null 2>&1; then
    echo "FOUND: $tool -> $(command -v "$tool")"
    "$tool" -version 2>/dev/null | head -n 1 || true
  else
    echo "MISSING (optional): $tool"
  fi
done
