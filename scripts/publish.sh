#!/usr/bin/env bash
set -euo pipefail

# Publish storypole crates to crates.io in strict topological dependency order.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

UA="gaugeblock-publisher/1.0"
CRATES=(
  "storypole-core"
  "storypole-mcp"
  "storypole-cli"
  "storypole"
)

VERSION="$(awk -F'"' '/^version = / {print $2; exit}' Cargo.toml)"

echo "================================================================================"
echo "📦 Publishing storypole v${VERSION} crates to crates.io"
echo "================================================================================"

is_dry_run=false
for arg in "$@"; do
  if [[ "$arg" == "--dry-run" ]]; then
    is_dry_run=true
  fi
done

for idx in "${!CRATES[@]}"; do
  crate="${CRATES[$idx]}"
  step=$((idx + 1))
  echo ""
  echo "[$step/${#CRATES[@]}] Target: ${crate} v${VERSION}"

  # Check if this exact version is already published on crates.io
  http_code=$(curl -s -A "$UA" -o /dev/null -w "%{http_code}" "https://crates.io/api/v1/crates/${crate}/${VERSION}")
  if [[ "$http_code" == "200" ]]; then
    echo "⏩ Skipping ${crate} v${VERSION} (already published on crates.io)"
    continue
  fi

  echo "🚀 Publishing ${crate} v${VERSION}..."
  cargo publish -p "${crate}" "$@"

  if ! $is_dry_run && [[ $step -lt ${#CRATES[@]} ]]; then
    echo "⏳ Waiting 20s for crates.io index propagation before next crate..."
    sleep 20
  fi
done

echo ""
echo "🎉 storypole v${VERSION} publication sequence complete!"
