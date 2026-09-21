#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

command="${1:-create}"
image="build/generic-storage.img"
size="${2:-16M}"

mkdir -p build
case "$command" in
  create)
    if [[ ! -f "$image" ]]; then
      truncate -s "$size" "$image"
      echo "created $image ($size)"
    else
      echo "$image already exists"
    fi
    ;;
  reset)
    rm -f "$image"
    truncate -s "$size" "$image"
    echo "reset $image ($size)"
    ;;
  *)
    echo "usage: $0 [create|reset] [SIZE]" >&2
    exit 2
    ;;
esac
