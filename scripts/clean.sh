#!/usr/bin/env bash
#
# Remove build artifacts to reclaim disk space. The Cargo `target/` directory
# is only a build cache and is safe to delete; it is regenerated on the next
# build (which will then be slower).
#
# Usage:
#   scripts/clean.sh          # build caches + test/tooling output (keeps node_modules, dist)
#   scripts/clean.sh --dist   # also remove dist/
#   scripts/clean.sh --all    # also remove node_modules/ and dist/
#
set -euo pipefail

cd "$(dirname "$0")/.."

remove_dist=false
remove_all=false
for argument in "$@"; do
  case "$argument" in
    --dist) remove_dist=true ;;
    --all) remove_all=true; remove_dist=true ;;
    *)
      echo "clean: unknown argument: $argument" >&2
      echo "usage: $0 [--dist|--all]" >&2
      exit 2
      ;;
  esac
done

echo "Cleaning build artifacts in $(pwd) ..."

# Cargo build caches (workspace root + legacy per-crate dir).
rm -rf target src-tauri/target

# Test / tooling output.
rm -rf test-results playwright-report blob-report .playwright .playwright-mcp coverage

# Frontend tooling caches.
rm -rf .vite node_modules/.vite node_modules/.cache

# Python caches left behind by build_guard.py and the smoke/benchmark scripts.
find . -type d \( -name __pycache__ -o -name .pytest_cache -o -name .mypy_cache -o -name .ruff_cache \) \
  -not -path './node_modules/*' -not -path './target/*' -prune -exec rm -rf {} +

if [[ "$remove_dist" == true ]]; then
  echo "  also removing dist/"
  rm -rf dist
fi

if [[ "$remove_all" == true ]]; then
  echo "  also removing node_modules/"
  rm -rf node_modules
fi

echo "Done."
if command -v du >/dev/null 2>&1; then
  du -sh . 2>/dev/null || true
fi
