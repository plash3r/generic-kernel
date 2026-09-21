#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

mode="${1:-normal}"
case "$mode" in
  normal)
    bash scripts/build.sh normal
    disk="build/generic-uefi.img"
    iso="build/generic-uefi.iso"
    ;;
  smoke)
    bash scripts/build.sh smoke
    disk="build/generic-smoke-uefi.img"
    iso="build/generic-smoke-uefi.iso"
    ;;
  *)
    echo "usage: $0 [normal|smoke]" >&2
    exit 2
    ;;
esac

python3 tools/uefi_iso.py "$disk" "$iso"
echo "Generic UEFI ISO: $iso"
