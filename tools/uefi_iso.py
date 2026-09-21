#!/usr/bin/env python3
"""Create a UEFI-bootable El Torito ISO from Generic's GPT disk image."""
import argparse
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import uuid

SECTOR_SIZE = 512
GPT_SIGNATURE = b"EFI PART"
EFI_SYSTEM_PARTITION = uuid.UUID("c12a7328-f81f-11d2-ba4b-00a0c93ec93b")


class IsoError(Exception):
    pass


def find_efi_partition(image: Path) -> tuple[int, int]:
    image_size = image.stat().st_size
    with image.open("rb") as handle:
        handle.seek(SECTOR_SIZE)
        header = handle.read(92)
        if len(header) < 92 or header[:8] != GPT_SIGNATURE:
            raise IsoError("input image does not contain a GPT header")

        entries_lba = struct.unpack_from("<Q", header, 72)[0]
        entry_count = struct.unpack_from("<I", header, 80)[0]
        entry_size = struct.unpack_from("<I", header, 84)[0]
        if entry_size < 128 or entry_count == 0:
            raise IsoError("invalid GPT partition table")

        handle.seek(entries_lba * SECTOR_SIZE)
        for _ in range(entry_count):
            entry = handle.read(entry_size)
            if len(entry) != entry_size:
                raise IsoError("truncated GPT partition table")
            if entry[:16] == bytes(16):
                continue

            partition_type = uuid.UUID(bytes_le=entry[:16])
            if partition_type != EFI_SYSTEM_PARTITION:
                continue

            first_lba, last_lba = struct.unpack_from("<QQ", entry, 32)
            if last_lba < first_lba:
                raise IsoError("invalid EFI System Partition range")

            offset = first_lba * SECTOR_SIZE
            length = (last_lba - first_lba + 1) * SECTOR_SIZE
            if offset + length > image_size:
                raise IsoError("EFI System Partition extends beyond image")
            return offset, length

    raise IsoError("EFI System Partition not found")


def copy_range(source: Path, target: Path, offset: int, length: int) -> None:
    remaining = length
    with source.open("rb") as src, target.open("wb") as dst:
        src.seek(offset)
        while remaining:
            chunk = src.read(min(1024 * 1024, remaining))
            if not chunk:
                raise IsoError("unexpected end of disk image")
            dst.write(chunk)
            remaining -= len(chunk)


def build_iso(disk_image: Path, output: Path) -> None:
    xorriso = shutil.which("xorriso")
    if not xorriso:
        raise IsoError("xorriso not found; install the xorriso package")

    offset, length = find_efi_partition(disk_image)
    output.parent.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="generic-iso-") as temp:
        root = Path(temp) / "root"
        boot = root / "boot"
        boot.mkdir(parents=True)
        esp = boot / "efi.img"
        copy_range(disk_image, esp, offset, length)

        subprocess.run(
            [
                xorriso,
                "-as",
                "mkisofs",
                "-iso-level",
                "3",
                "-R",
                "-J",
                "-V",
                "GENERIC_OS",
                "-c",
                "boot/boot.cat",
                "-e",
                "boot/efi.img",
                "-no-emul-boot",
                "-o",
                str(output),
                str(root),
            ],
            check=True,
        )

    if not output.is_file() or output.stat().st_size == 0:
        raise IsoError("ISO creation did not produce an output file")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("disk_image", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()

    try:
        if not args.disk_image.is_file():
            raise IsoError(f"disk image not found: {args.disk_image}")
        build_iso(args.disk_image, args.output)
    except (IsoError, OSError, subprocess.CalledProcessError) as exc:
        print(f"generic-iso: {exc}", file=sys.stderr)
        return 1

    print(args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
