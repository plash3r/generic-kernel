use core::{ptr, slice};

const SDT_HEADER_SIZE: usize = 36;
const MADT_HEADER_SIZE: usize = 44;
const MAX_TABLE_SIZE: usize = 1024 * 1024;
const MAX_IO_APICS: usize = 4;

#[derive(Clone, Copy, Debug, Default)]
pub struct IoApicInfo {
    pub address: u32,
    pub gsi_base: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct InterruptOverride {
    pub source_irq: u8,
    pub gsi: u32,
    pub flags: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct ApicInfo {
    pub local_apic_address: u64,
    pub io_apics: [IoApicInfo; MAX_IO_APICS],
    pub io_apic_count: usize,
    pub overrides: [Option<InterruptOverride>; 16],
}

pub fn discover_apic(rsdp_physical: u64, physical_offset: u64) -> Result<ApicInfo, &'static str> {
    let rsdp_v1 = physical_slice(physical_offset, rsdp_physical, 20)?;
    if &rsdp_v1[0..8] != b"RSD PTR " {
        return Err("invalid ACPI RSDP signature");
    }
    if checksum(rsdp_v1) != 0 {
        return Err("invalid ACPI RSDP checksum");
    }

    let revision = rsdp_v1[15];
    let rsdt = read_u32(rsdp_v1, 16) as u64;
    let (root, entry_size) = if revision >= 2 {
        let prefix = physical_slice(physical_offset, rsdp_physical, 36)?;
        let length = read_u32(prefix, 20) as usize;
        if !(36..=4096).contains(&length) {
            return Err("invalid ACPI RSDP length");
        }
        let full = physical_slice(physical_offset, rsdp_physical, length)?;
        if checksum(full) != 0 {
            return Err("invalid ACPI extended RSDP checksum");
        }
        let xsdt = read_u64(full, 24);
        if xsdt != 0 {
            (xsdt, 8usize)
        } else {
            (rsdt, 4usize)
        }
    } else {
        (rsdt, 4usize)
    };

    if root == 0 {
        return Err("ACPI root table address is zero");
    }

    let root_table = sdt(physical_offset, root)?;
    let payload = &root_table[SDT_HEADER_SIZE..];
    if payload.len() % entry_size != 0 {
        return Err("malformed ACPI root table");
    }

    for entry in payload.chunks_exact(entry_size) {
        let address = if entry_size == 8 {
            read_u64(entry, 0)
        } else {
            read_u32(entry, 0) as u64
        };
        if address == 0 {
            continue;
        }
        let table = sdt(physical_offset, address)?;
        if &table[0..4] == b"APIC" {
            return parse_madt(table);
        }
    }

    Err("ACPI MADT not found")
}

fn parse_madt(table: &[u8]) -> Result<ApicInfo, &'static str> {
    if table.len() < MADT_HEADER_SIZE {
        return Err("ACPI MADT is truncated");
    }

    let mut result = ApicInfo {
        local_apic_address: read_u32(table, 36) as u64,
        io_apics: [IoApicInfo::default(); MAX_IO_APICS],
        io_apic_count: 0,
        overrides: [None; 16],
    };

    let mut offset = MADT_HEADER_SIZE;
    while offset < table.len() {
        if offset + 2 > table.len() {
            return Err("truncated ACPI MADT entry");
        }
        let entry_type = table[offset];
        let length = table[offset + 1] as usize;
        if length < 2 || offset + length > table.len() {
            return Err("invalid ACPI MADT entry length");
        }
        let entry = &table[offset..offset + length];

        match entry_type {
            1 if length >= 12 => {
                if result.io_apic_count < result.io_apics.len() {
                    result.io_apics[result.io_apic_count] = IoApicInfo {
                        address: read_u32(entry, 4),
                        gsi_base: read_u32(entry, 8),
                    };
                    result.io_apic_count += 1;
                }
            }
            2 if length >= 10 => {
                let bus = entry[2];
                let source_irq = entry[3];
                if bus == 0 && (source_irq as usize) < result.overrides.len() {
                    result.overrides[source_irq as usize] = Some(InterruptOverride {
                        source_irq,
                        gsi: read_u32(entry, 4),
                        flags: read_u16(entry, 8),
                    });
                }
            }
            5 if length >= 12 => {
                result.local_apic_address = read_u64(entry, 4);
            }
            _ => {}
        }

        offset += length;
    }

    if result.local_apic_address == 0 {
        return Err("ACPI MADT has no local APIC address");
    }
    if result.io_apic_count == 0 {
        return Err("ACPI MADT has no IOAPIC");
    }

    Ok(result)
}

fn sdt(physical_offset: u64, physical: u64) -> Result<&'static [u8], &'static str> {
    let header = physical_slice(physical_offset, physical, SDT_HEADER_SIZE)?;
    let length = read_u32(header, 4) as usize;
    if !(SDT_HEADER_SIZE..=MAX_TABLE_SIZE).contains(&length) {
        return Err("invalid ACPI SDT length");
    }
    let table = physical_slice(physical_offset, physical, length)?;
    if checksum(table) != 0 {
        return Err("invalid ACPI SDT checksum");
    }
    Ok(table)
}

fn physical_slice(
    physical_offset: u64,
    physical: u64,
    length: usize,
) -> Result<&'static [u8], &'static str> {
    let virtual_address = physical_offset
        .checked_add(physical)
        .ok_or("ACPI physical address overflow")?;
    if length == 0 {
        return Err("zero-length ACPI mapping");
    }
    let pointer = virtual_address as *const u8;
    // SAFETY: the boot contract provides a physical direct map. ACPI firmware
    // tables are immutable while Generic is running; lengths are bounded before
    // larger slices are requested.
    Ok(unsafe { slice::from_raw_parts(pointer, length) })
}

fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte))
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    // SAFETY: callers validate table/entry lengths before accessing fields.
    unsafe { ptr::read_unaligned(bytes.as_ptr().add(offset).cast::<u16>()) }.to_le()
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    // SAFETY: callers validate table/entry lengths before accessing fields.
    unsafe { ptr::read_unaligned(bytes.as_ptr().add(offset).cast::<u32>()) }.to_le()
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    // SAFETY: callers validate table/entry lengths before accessing fields.
    unsafe { ptr::read_unaligned(bytes.as_ptr().add(offset).cast::<u64>()) }.to_le()
}
