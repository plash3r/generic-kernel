#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

source_file="userspace/recontrol/kernel_probe.rcl"
llvm_file="userspace/recontrol/kernel_probe.ll"

if [[ -n "${RCL:-}" ]]; then
  compiler=("$RCL")
elif command -v rcl >/dev/null 2>&1; then
  compiler=(rcl)
elif [[ -n "${RECONTROL_ROOT:-}" && -f "$RECONTROL_ROOT/Cargo.toml" ]]; then
  compiler=(cargo run --quiet --manifest-path "$RECONTROL_ROOT/Cargo.toml" -p rcl --)
else
  echo "Recontrol compiler not found." >&2
  echo "Set RCL=/path/to/rcl, install rcl, or set RECONTROL_ROOT=/path/to/recontrol-lang." >&2
  exit 2
fi

"${compiler[@]}" check "$source_file"
"${compiler[@]}" emit-llvm "$source_file"

echo "Recontrol kernel probe regenerated: $llvm_file"
