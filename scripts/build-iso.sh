#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

mode="${1:-normal}"
case "$mode" in
  normal)
    features=()
    stem="generic"
    ;;
  smoke)
    features=(--features smoke)
    stem="generic-smoke"
    ;;
  *)
    echo "usage: $0 [normal|smoke]" >&2
    exit 2
    ;;
esac

cargo build --locked -p generic-kernel --target x86_64-unknown-none --release "${features[@]}"
kernel="target/x86_64-unknown-none/release/generic-kernel"
uefi="build/${stem}-uefi.img"
bios="build/${stem}-bios.img"
iso="build/${stem}.iso"

cargo run --locked -p generic-image --release -- uefi "$kernel" "$uefi"
cargo run --locked -p generic-image --release -- bios "$kernel" "$bios"
python3 tools/uefi_iso.py "$uefi" "$bios" "$iso"

echo "Generic hybrid BIOS + UEFI ISO: $iso"
