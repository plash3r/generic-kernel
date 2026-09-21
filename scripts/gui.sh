#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

gui_root="${GENERIC_GUI_ROOT:-}"
if [[ -z "$gui_root" ]]; then
  for candidate in "../generic-gui" ".deps/generic-gui"; do
    if [[ -f "$candidate/Cargo.toml" ]]; then
      gui_root="$candidate"
      break
    fi
  done
fi

if [[ -z "$gui_root" || ! -f "$gui_root/Cargo.toml" ]]; then
  echo "generic-gui checkout not found; set GENERIC_GUI_ROOT or clone it beside generic-kernel" >&2
  exit 2
fi

gui_root="$(cd "$gui_root" && pwd)"
(
  cd "$gui_root"
  cargo build -p generic-gui-user --target x86_64-unknown-none --release >&2
)

gui_elf="$gui_root/target/x86_64-unknown-none/release/generic-gui-user"
if [[ ! -f "$gui_elf" ]]; then
  echo "Generic GUI ELF missing after build: $gui_elf" >&2
  exit 1
fi

printf '%s\n' "$gui_elf"
