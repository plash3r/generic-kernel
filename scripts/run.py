#!/usr/bin/env python3
"""Run only a disposable QEMU guest, never write physical disks."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("--smoke", action="store_true")
parser.add_argument("--debug", action="store_true", help="wait for GDB on localhost:1234")
parser.add_argument(
    "--graphical",
    action="store_true",
    help="open the framebuffer console in a QEMU window",
)
args = parser.parse_args()
if args.smoke and args.debug:
    parser.error("--smoke and --debug are mutually exclusive")
if args.smoke and args.graphical:
    parser.error("--smoke and --graphical are mutually exclusive")
qemu = shutil.which("qemu-system-x86_64")
if not qemu:
    sys.exit("qemu-system-x86_64 not found; install QEMU")
firmware = os.environ.get("OVMF_CODE")
if not firmware:
    firmware = next(
        (
            str(p)
            for p in map(
                Path,
                [
                    "/usr/share/OVMF/OVMF_CODE_4M.fd",
                    "/usr/share/OVMF/OVMF_CODE.fd",
                    "/usr/share/edk2/x64/OVMF_CODE.fd",
                ],
            )
            if p.is_file()
        ),
        None,
    )
if not firmware or not Path(firmware).is_file():
    sys.exit("set OVMF_CODE to an OVMF firmware file")
image = root / "build" / (
    "generic-smoke-uefi.img" if args.smoke else "generic-uefi.img"
)
if not image.is_file():
    sys.exit(
        "build the image first: bash scripts/build.sh "
        + ("smoke" if args.smoke else "normal")
    )
cmd = [
    qemu,
    "-machine",
    "q35",
    "-accel",
    "tcg",
    "-cpu",
    "qemu64",
    "-m",
    "256M",
    "-smp",
    "1",
    "-drive",
    f"if=pflash,format=raw,readonly=on,file={firmware}",
    "-drive",
    f"format=raw,file={image}",
    "-snapshot",
    "-net",
    "none",
    "-serial",
    "stdio",
    "-monitor",
    "none",
    "-no-reboot",
]
if not args.graphical:
    cmd += ["-display", "none"]
if args.debug:
    cmd += ["-S", "-gdb", "tcp:127.0.0.1:1234"]
if not args.smoke:
    sys.exit(subprocess.call(cmd))
cmd += ["-device", "isa-debug-exit,iobase=0xf4,iosize=0x04"]
try:
    result = subprocess.run(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=60,
    )
    output = result.stdout
    code = result.returncode
except subprocess.TimeoutExpired as exc:
    output = exc.stdout or b""
    code = None
(root / "build" / "smoke.log").write_bytes(output)
sys.stdout.buffer.write(output)
if code != 33 or b"GENERIC: READY" not in output or b"GENERIC: PANIC" in output:
    sys.exit(f"smoke FAILED (QEMU exit={code}); see build/smoke.log")
print("smoke PASSED: boot, framebuffer init, physical RAM and breakpoint")
