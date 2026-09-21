use x86_64::instructions::port::Port;

const CONFIG_ADDRESS: u16 = 0x0cf8;
const CONFIG_DATA: u16 = 0x0cfc;
const VIRTIO_VENDOR: u16 = 0x1af4;
const VIRTIO_BLOCK_LEGACY: u16 = 0x1001;

#[derive(Clone, Copy, Debug)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub io_base: u16,
}

pub fn find_legacy_virtio_block() -> Option<PciDevice> {
    for bus in 0u16..=255 {
        for device in 0u8..32 {
            for function in 0u8..8 {
                let vendor = read_u16(bus as u8, device, function, 0x00);
                if vendor == 0xffff {
                    continue;
                }
                let device_id = read_u16(bus as u8, device, function, 0x02);
                if vendor != VIRTIO_VENDOR || device_id != VIRTIO_BLOCK_LEGACY {
                    continue;
                }

                let bar0 = read_u32(bus as u8, device, function, 0x10);
                if bar0 & 1 == 0 {
                    continue;
                }
                let io_base = (bar0 & !0x3) as u16;
                let command = read_u16(bus as u8, device, function, 0x04);
                write_u16(
                    bus as u8,
                    device,
                    function,
                    0x04,
                    command | 0x0001 | 0x0004,
                );
                return Some(PciDevice {
                    bus: bus as u8,
                    device,
                    function,
                    io_base,
                });
            }
        }
    }
    None
}

fn address(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    0x8000_0000
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xfc)
}

fn read_u32(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    // SAFETY: 0xcf8/0xcfc are the standard PCI configuration mechanism #1 ports.
    unsafe {
        let mut address_port = Port::<u32>::new(CONFIG_ADDRESS);
        let mut data_port = Port::<u32>::new(CONFIG_DATA);
        address_port.write(address(bus, device, function, offset));
        data_port.read()
    }
}

fn read_u16(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let value = read_u32(bus, device, function, offset);
    ((value >> ((offset & 2) * 8)) & 0xffff) as u16
}

fn write_u16(bus: u8, device: u8, function: u8, offset: u8, value: u16) {
    let aligned = offset & !3;
    let shift = ((offset & 2) * 8) as u32;
    let current = read_u32(bus, device, function, aligned);
    let updated = (current & !(0xffff << shift)) | ((value as u32) << shift);
    // SAFETY: standard PCI configuration mechanism #1.
    unsafe {
        let mut address_port = Port::<u32>::new(CONFIG_ADDRESS);
        let mut data_port = Port::<u32>::new(CONFIG_DATA);
        address_port.write(address(bus, device, function, aligned));
        data_port.write(updated);
    }
}
