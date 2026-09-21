#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mode="${1:-normal}"
case "$mode" in
  normal) features=(); image=generic-uefi.img ;;
  smoke) features=(--features smoke); image=generic-smoke-uefi.img ;;
  *) echo "usage: $0 [normal|smoke]" >&2; exit 2 ;;
esac

if [[ "$mode" == "normal" && -z "${GENERIC_GUI_ELF:-}" ]]; then
  if [[ -n "${GENERIC_GUI_ROOT:-}" || -f ../generic-gui/Cargo.toml || -f .deps/generic-gui/Cargo.toml ]]; then
    export GENERIC_GUI_ELF
    GENERIC_GUI_ELF="$(bash scripts/gui.sh)"
    echo "Including Generic GUI: $GENERIC_GUI_ELF"
  fi
fi

cargo build --locked -p generic-kernel --target x86_64-unknown-none --release "${features[@]}"
cargo run --locked -p generic-image --release -- target/x86_64-unknown-none/release/generic-kernel "build/$image"
