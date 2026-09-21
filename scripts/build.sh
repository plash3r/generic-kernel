#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mode="${1:-normal}"
case "$mode" in
  normal) features=(); image=iron-uefi.img ;;
  smoke) features=(--features smoke); image=iron-smoke-uefi.img ;;
  *) echo "usage: $0 [normal|smoke]" >&2; exit 2 ;;
esac
cargo build --locked -p iron-kernel --target x86_64-unknown-none --release "${features[@]}"
cargo run --locked -p iron-image --release -- target/x86_64-unknown-none/release/iron-kernel "build/$image"
