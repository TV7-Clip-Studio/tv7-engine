#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "Usage: $0 <source-path> [artifact-name]" >&2
  exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
cd "$repo_root"

source_path="$1"
artifact_name="${2:-}"
dest_root="${TV7_ENGINE_CONTRACT_DEST_ROOT:-docs/contracts/tv7-axum-engine}"

if [[ ! -e "$source_path" ]]; then
  echo "Source path not found: $source_path" >&2
  exit 1
fi

mkdir -p "$dest_root"

if [[ -n "$artifact_name" ]]; then
  destination="$dest_root/$artifact_name"
else
  destination="$dest_root/$(basename "$source_path")"
fi

if [[ -d "$source_path" ]]; then
  rm -rf "$destination"
  cp -R "$source_path" "$destination"
else
  mkdir -p "$(dirname "$destination")"
  cp "$source_path" "$destination"
fi

echo "Published: $source_path -> $destination"

if command -v shasum >/dev/null 2>&1; then
  if [[ -d "$destination" ]]; then
    find "$destination" -type f -print0 | sort -z | xargs -0 shasum -a 256
  else
    shasum -a 256 "$destination"
  fi
else
  echo "shasum not found; skipping digest output"
fi
