# Owned page-table handoff

Generic takes ownership of its x86_64 address translation before heap setup.
The permanent PMM allocates every level of the new four-level page-table tree.
CR3 switches only after cloning completes; neither new heap mappings nor future
mapping edits modify the bootloader tables.

## Boot sequence

1. Normalize usable RAM into the PMM and verify allocation/free/coalescing.
2. Copy the active page-table tree through the physical direct map. Copy 4 KiB,
   2 MiB and 1 GiB leaves verbatim, preserving address, permissions, cache flags
   and PAT bits. Allocate new frames for every non-leaf table.
3. Check PMM accounting, disable global TLB caching temporarily and load the new
   CR3. Restore CR4 and enable CR0.WP so supervisor writes respect read-only pages.
4. Map the existing RW+NX heap and verify its allocation and guard pages.
5. Continue storage/console boot validation. The existing Recontrol ABI probe
   has already run before memory initialization.

The success marker is `[ok] Generic-owned CR3 ...`. Every disk/ISO/storage smoke
run must observe it and reach `GENERIC: READY` with QEMU's success exit code.

## Failure and ownership rules

- All table allocation is completed before activation. Exhaustion, malformed
  huge-page entries, recursive mappings and allocation-limit failures release
  the clone's partial tree from children to parents, leaving the source unchanged.
- Cloning uses the partial destination tree as its allocation journal: no heap
  and stack usage bounded by four levels, even for broad firmware direct maps.
  The table-frame budget is 16384 (64 MiB); PMM exhaustion also rolls back.
  The adapter trusts the active bootloader tables and physical direct map; it
  is not a parser for untrusted page-table data.
- Four-level paging, one CPU and disabled interrupts are required. Bootstrap
  PCID and LA57 are rejected. Recursive mappings are not configured by Generic.
- Newly owned table frames remain allocated for the kernel's lifetime. Source
  page-table frames stay reserved; reclamation of bootloader memory is deferred.
- Hardware may update accessed/dirty bits while cloning. The transition does
  not treat their historical values as accounting information.

## Scope

This establishes ownership, not userspace isolation. Leaf permissions are
inherited, and the physical direct map is retained. Full text/rodata/data W^X,
removing writable aliases of executable pages, user address spaces, cross-CPU
TLB shootdowns and page-table reclamation are not implemented by this change.
Recontrol still runs its existing freestanding probe; moving init/services into
ring 3 depends on the later syscall and process stages.

## Tests

`cargo test --locked -p kernel-core` tests all cloned table levels, leaf flags,
PAT and huge pages, preservation of non-present metadata, independence from the
source, rollback at each allocation depth, table limits and invalid topology.
The existing CI boots UEFI disk, UEFI ISO, BIOS ISO and persistent storage twice
with this new CR3, covering downstream memory, framebuffer and disk consumers.
