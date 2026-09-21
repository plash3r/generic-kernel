use crate::{arch::pci, mm};
use core::sync::atomic::{compiler_fence, Ordering};
use kernel_core::block::{BlockDevice, BlockError, SECTOR_SIZE};
use spin::Mutex;
use x86_64::instructions::port::Port;

const HOST_FEATURES: u16 = 0x00;
const GUEST_FEATURES: u16 = 0x04;
const QUEUE_PFN: u16 = 0x08;
const QUEUE_SIZE: u16 = 0x0c;
const QUEUE_SELECT: u16 = 0x0e;
const QUEUE_NOTIFY: u16 = 0x10;
const DEVICE_STATUS: u16 = 0x12;
const ISR_STATUS: u16 = 0x13;
const DEVICE_CONFIG: u16 = 0x14;

const STATUS_ACKNOWLEDGE: u8 = 1;
const STATUS_DRIVER: u8 = 2;
const STATUS_DRIVER_OK: u8 = 4;
const STATUS_FAILED: u8 = 128;

const DESC_NEXT: u16 = 1;
const DESC_WRITE: u16 = 2;
const REQ_READ: u32 = 0;
const REQ_WRITE: u32 = 1;
const REQUEST_DATA_OFFSET: u64 = 16;
const REQUEST_STATUS_OFFSET: u64 = REQUEST_DATA_OFFSET + SECTOR_SIZE as u64;

pub struct VirtioBlock {
    capacity: u64,
    state: Mutex<State>,
}

struct State {
    io_base: u16,
    queue_size: u16,
    queue_phys: u64,
    queue_virt: u64,
    request_phys: u64,
    request_virt: u64,
    avail_idx: u16,
    last_used: u16,
}

impl VirtioBlock {
    pub fn probe() -> Result<Option<Self>, &'static str> {
        let Some(device) = pci::find_legacy_virtio_block() else {
            return Ok(None);
        };

        let base = device.io_base;
        write_u8(base + DEVICE_STATUS, 0);
        write_u8(base + DEVICE_STATUS, STATUS_ACKNOWLEDGE);
        write_u8(base + DEVICE_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER);

        let _features = read_u32(base + HOST_FEATURES);
        // Generic currently relies only on the mandatory legacy block feature set.
        write_u32(base + GUEST_FEATURES, 0);
        write_u16(base + QUEUE_SELECT, 0);
        let queue_size = read_u16(base + QUEUE_SIZE);
        if queue_size < 3 || queue_size > 1024 {
            write_u8(base + DEVICE_STATUS, STATUS_FAILED);
            return Err("virtio-blk reported invalid queue size");
        }
        if read_u32(base + QUEUE_PFN) != 0 {
            write_u8(base + DEVICE_STATUS, STATUS_FAILED);
            return Err("virtio-blk queue already active");
        }

        let desc_bytes = queue_size as u64 * 16;
        let avail_bytes = 6 + queue_size as u64 * 2;
        let used_offset = align_up(desc_bytes + avail_bytes, 4096);
        let used_bytes = 6 + queue_size as u64 * 8;
        let queue_bytes = used_offset + used_bytes;
        let queue_pages = (queue_bytes + 4095) / 4096;

        let queue = mm::allocate_dma(queue_pages).ok_or("no DMA memory for virtqueue")?;
        let request = mm::allocate_dma(1).ok_or("no DMA memory for virtio request")?;
        let pfn =
            u32::try_from(queue.physical >> 12).map_err(|_| "virtqueue above legacy PFN range")?;
        write_u32(base + QUEUE_PFN, pfn);

        let capacity = read_u64_ports(base + DEVICE_CONFIG);
        if capacity == 0 {
            write_u8(base + DEVICE_STATUS, STATUS_FAILED);
            return Err("virtio-blk has zero capacity");
        }

        write_u8(
            base + DEVICE_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_DRIVER_OK,
        );

        crate::log!(
            "[ok] virtio-blk {:02x}:{:02x}.{} io={:#x}, {} sectors\n",
            device.bus,
            device.device,
            device.function,
            base,
            capacity
        );

        Ok(Some(Self {
            capacity,
            state: Mutex::new(State {
                io_base: base,
                queue_size,
                queue_phys: queue.physical,
                queue_virt: queue.virtual_address,
                request_phys: request.physical,
                request_virt: request.virtual_address,
                avail_idx: 0,
                last_used: 0,
            }),
        }))
    }
}

impl BlockDevice for VirtioBlock {
    fn sector_count(&self) -> u64 {
        self.capacity
    }

    fn read_sector(&self, sector: u64, buffer: &mut [u8; SECTOR_SIZE]) -> Result<(), BlockError> {
        if sector >= self.capacity {
            return Err(BlockError::OutOfRange);
        }
        let mut state = self.state.lock();
        state.submit(REQ_READ, sector, None)?;
        // SAFETY: request DMA page remains owned by this driver for its lifetime.
        unsafe {
            core::ptr::copy_nonoverlapping(
                (state.request_virt + REQUEST_DATA_OFFSET) as *const u8,
                buffer.as_mut_ptr(),
                SECTOR_SIZE,
            );
        }
        Ok(())
    }

    fn write_sector(&self, sector: u64, buffer: &[u8; SECTOR_SIZE]) -> Result<(), BlockError> {
        if sector >= self.capacity {
            return Err(BlockError::OutOfRange);
        }
        let mut state = self.state.lock();
        // SAFETY: request DMA page remains owned by this driver for its lifetime.
        unsafe {
            core::ptr::copy_nonoverlapping(
                buffer.as_ptr(),
                (state.request_virt + REQUEST_DATA_OFFSET) as *mut u8,
                SECTOR_SIZE,
            );
        }
        state.submit(REQ_WRITE, sector, Some(()))
    }
}

impl State {
    fn submit(
        &mut self,
        request_type: u32,
        sector: u64,
        write_request: Option<()>,
    ) -> Result<(), BlockError> {
        let header = self.request_virt as *mut u8;
        // SAFETY: all addresses below point into DMA pages exclusively owned by
        // this State and no request is concurrent because State is mutex-guarded.
        unsafe {
            write_u32_mem(header, request_type);
            write_u32_mem(header.add(4), 0);
            write_u64_mem(header.add(8), sector);
            core::ptr::write_volatile((self.request_virt + REQUEST_STATUS_OFFSET) as *mut u8, 0xff);
        }

        let data_flags = DESC_NEXT
            | if write_request.is_none() {
                DESC_WRITE
            } else {
                0
            };
        self.write_desc(0, self.request_phys, 16, DESC_NEXT, 1);
        self.write_desc(
            1,
            self.request_phys + REQUEST_DATA_OFFSET,
            SECTOR_SIZE as u32,
            data_flags,
            2,
        );
        self.write_desc(
            2,
            self.request_phys + REQUEST_STATUS_OFFSET,
            1,
            DESC_WRITE,
            0,
        );

        let desc_bytes = self.queue_size as u64 * 16;
        let avail_ring = self.queue_virt + desc_bytes;
        let slot = self.avail_idx % self.queue_size;
        // SAFETY: ring slot lies within the allocated virtqueue.
        unsafe {
            write_u16_mem((avail_ring + 4 + slot as u64 * 2) as *mut u8, 0);
        }
        compiler_fence(Ordering::SeqCst);
        self.avail_idx = self.avail_idx.wrapping_add(1);
        // SAFETY: avail index is a 16-bit field at offset 2.
        unsafe {
            write_u16_mem((avail_ring + 2) as *mut u8, self.avail_idx);
        }
        compiler_fence(Ordering::SeqCst);
        write_u16(self.io_base + QUEUE_NOTIFY, 0);

        let used_offset = align_up(desc_bytes + 6 + self.queue_size as u64 * 2, 4096);
        let used_idx_ptr = (self.queue_virt + used_offset + 2) as *const u16;
        let mut completed = false;
        for _ in 0..20_000_000 {
            compiler_fence(Ordering::SeqCst);
            // SAFETY: used index belongs to the device-owned side of this queue.
            let used = unsafe { core::ptr::read_volatile(used_idx_ptr) };
            if used != self.last_used {
                self.last_used = used;
                completed = true;
                break;
            }
            core::hint::spin_loop();
        }
        if !completed {
            return Err(BlockError::Io);
        }

        let _ = read_u8(self.io_base + ISR_STATUS);
        compiler_fence(Ordering::SeqCst);
        // SAFETY: the status byte is written by the device after request completion.
        let status = unsafe {
            core::ptr::read_volatile((self.request_virt + REQUEST_STATUS_OFFSET) as *const u8)
        };
        if status == 0 {
            Ok(())
        } else {
            Err(BlockError::Io)
        }
    }

    fn write_desc(&self, index: u16, address: u64, len: u32, flags: u16, next: u16) {
        let ptr = (self.queue_virt + index as u64 * 16) as *mut u8;
        // SAFETY: index 0..2 is valid because probe rejects queue sizes below 3.
        unsafe {
            write_u64_mem(ptr, address);
            write_u32_mem(ptr.add(8), len);
            write_u16_mem(ptr.add(12), flags);
            write_u16_mem(ptr.add(14), next);
        }
    }
}

fn align_up(value: u64, alignment: u64) -> u64 {
    (value + alignment - 1) & !(alignment - 1)
}

fn read_u8(port: u16) -> u8 {
    // SAFETY: caller supplies a discovered virtio I/O BAR port.
    unsafe { Port::<u8>::new(port).read() }
}

fn write_u8(port: u16, value: u8) {
    // SAFETY: caller supplies a discovered virtio I/O BAR port.
    unsafe { Port::<u8>::new(port).write(value) }
}

fn read_u16(port: u16) -> u16 {
    // SAFETY: caller supplies a discovered virtio I/O BAR port.
    unsafe { Port::<u16>::new(port).read() }
}

fn write_u16(port: u16, value: u16) {
    // SAFETY: caller supplies a discovered virtio I/O BAR port.
    unsafe { Port::<u16>::new(port).write(value) }
}

fn read_u32(port: u16) -> u32 {
    // SAFETY: caller supplies a discovered virtio I/O BAR port.
    unsafe { Port::<u32>::new(port).read() }
}

fn write_u32(port: u16, value: u32) {
    // SAFETY: caller supplies a discovered virtio I/O BAR port.
    unsafe { Port::<u32>::new(port).write(value) }
}

fn read_u64_ports(port: u16) -> u64 {
    read_u32(port) as u64 | ((read_u32(port + 4) as u64) << 32)
}

unsafe fn write_u16_mem(ptr: *mut u8, value: u16) {
    // SAFETY: caller guarantees aligned-enough, valid DMA memory.
    unsafe { core::ptr::write_volatile(ptr.cast::<u16>(), value.to_le()) };
}

unsafe fn write_u32_mem(ptr: *mut u8, value: u32) {
    // SAFETY: caller guarantees aligned-enough, valid DMA memory.
    unsafe { core::ptr::write_volatile(ptr.cast::<u32>(), value.to_le()) };
}

unsafe fn write_u64_mem(ptr: *mut u8, value: u64) {
    // SAFETY: caller guarantees aligned-enough, valid DMA memory.
    unsafe { core::ptr::write_volatile(ptr.cast::<u64>(), value.to_le()) };
}
