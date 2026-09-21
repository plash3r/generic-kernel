#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

mode="${1:-smoke}"
case "$mode" in
  build)
    mkdir -p build
    python3 tools/x128.py arch/x128/bootstrap.x128 --assemble build/generic-x128.img
    ;;
  run)
    python3 tools/x128.py arch/x128/bootstrap.x128
    ;;
  smoke)
    python3 tools/x128.py arch/x128/bootstrap.x128 --smoke
    ;;
  *)
    echo "usage: $0 [build|run|smoke]" >&2
    exit 2
    ;;
esac
