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
parser.add_argument(
    "--iso",
    action="store_true",
    help="boot the hybrid ISO as a virtual optical disc",
)
parser.add_argument(
    "--bios",
    action="store_true",
    help="use legacy BIOS instead of UEFI (ISO mode only)",
)
parser.add_argument(
    "--storage",
    action="store_true",
    help="attach build/generic-storage.img as a persistent legacy virtio-blk disk",
)
parser.add_argument(
    "--expect-storage-recovered",
    action="store_true",
    help="smoke mode: require GenericFS to recover an existing persistent volume",
)
args = parser.parse_args()

if args.smoke and args.debug:
    parser.error("--smoke and --debug are mutually exclusive")
if args.smoke and args.graphical:
    parser.error("--smoke and --graphical are mutually exclusive")
if args.bios and not args.iso:
    parser.error("--bios currently requires --iso")
if args.expect_storage_recovered and not (args.smoke and args.storage):
    parser.error("--expect-storage-recovered requires --smoke --storage")

qemu = shutil.which("qemu-system-x86_64")
if not qemu:
    sys.exit("qemu-system-x86_64 not found; install QEMU")

firmware = None
if not args.bios:
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

if args.iso:
    stem = "generic-smoke" if args.smoke else "generic"
    image = root / "build" / f"{stem}.iso"
else:
    stem = "generic-smoke-uefi" if args.smoke else "generic-uefi"
    image = root / "build" / f"{stem}.img"

if not image.is_file():
    if args.iso:
        sys.exit(
            "build the ISO first: bash scripts/build-iso.sh "
            + ("smoke" if args.smoke else "normal")
        )
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
]

if firmware is not None:
    cmd += ["-drive", f"if=pflash,format=raw,readonly=on,file={firmware}"]

if args.iso:
    cmd += ["-cdrom", str(image), "-boot", "d"]
else:
    boot_drive = f"format=raw,file={image}"
    if args.storage:
        boot_drive += ",snapshot=on"
    cmd += ["-drive", boot_drive]

if args.storage:
    storage = root / "build" / "generic-storage.img"
    if not storage.is_file():
        sys.exit("create persistent storage first: bash scripts/storage.sh create")
    cmd += [
        "-drive",
        f"id=generic-storage,if=none,format=raw,file={storage}",
        "-device",
        "virtio-blk-pci,drive=generic-storage,disable-modern=on",
    ]

cmd += [
    "-net",
    "none",
    "-serial",
    "stdio",
    "-monitor",
    "none",
    "-no-reboot",
]
if not args.storage:
    cmd += ["-snapshot"]

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

if args.iso and args.bios:
    log_name = "smoke-iso-bios.log"
elif args.iso:
    log_name = "smoke-iso-uefi.log"
elif args.storage:
    log_name = "smoke-storage.log"
else:
    log_name = "smoke.log"

(root / "build" / log_name).write_bytes(output)
sys.stdout.buffer.write(output)
if code != 33 or b"GENERIC: READY" not in output or b"GENERIC: PANIC" in output:
    sys.exit(f"smoke FAILED (QEMU exit={code}); see build/{log_name}")
if b"[ok] Generic-owned CR3 " not in output:
    sys.exit(f"page-table ownership smoke FAILED; see build/{log_name}")
if b"[ok] interrupt event loop + PIT timer 100 Hz" not in output:
    sys.exit(f"interrupt/timer smoke FAILED; see build/{log_name}")
if b"[ok] scheduler context switch:" not in output:
    sys.exit(f"scheduler smoke FAILED; see build/{log_name}")
if b"[ok] process address space:" not in output or b"private page-table tree" not in output:
    sys.exit(f"process CR3 isolation smoke FAILED; see build/{log_name}")
if b"[ok] userspace scheduler task:" not in output or b"ready->running->exited" not in output:
    sys.exit(f"userspace scheduler integration smoke FAILED; see build/{log_name}")
if b"GENERIC USER: /bin/init via validated write syscall" not in output:
    sys.exit(f"userspace pointer/write syscall smoke FAILED; see build/{log_name}")
if b"[ok] userspace ELF /bin/init:" not in output or b"CPL3" not in output:
    sys.exit(f"userspace ELF/ring3 smoke FAILED; see build/{log_name}")
if b"[ok] framebuffer console smoke" not in output:
    sys.exit(f"framebuffer console smoke FAILED; see build/{log_name}")
if args.storage and b"GenericFS" not in output:
    sys.exit(f"storage smoke FAILED; see build/{log_name}")
if args.expect_storage_recovered and b"GenericFS recovered persistent volume" not in output:
    sys.exit(f"persistent recovery FAILED; see build/{log_name}")

if args.iso:
    firmware_name = "BIOS" if args.bios else "UEFI"
    print(
        f"smoke PASSED from hybrid ISO ({firmware_name}): "
        "boot, APIC/timer, scheduler, isolated userspace ELF/ring3, framebuffer console, physical RAM and breakpoint"
    )
elif args.storage:
    print("smoke PASSED with APIC/timer, scheduler, isolated userspace ELF/ring3, framebuffer console and persistent virtio-blk GenericFS")
else:
    print("smoke PASSED from disk: boot, APIC/timer, scheduler, isolated userspace ELF/ring3, framebuffer console, physical RAM and breakpoint")
