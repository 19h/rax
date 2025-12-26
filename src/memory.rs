use crate::error::Result;
#[cfg(all(feature = "kvm", target_os = "linux"))]
use kvm_bindings::kvm_userspace_memory_region;
#[cfg(all(feature = "kvm", target_os = "linux"))]
use kvm_ioctls::VmFd;
use tracing::info;
#[cfg(all(feature = "kvm", target_os = "linux"))]
use vm_memory::{Address, GuestAddress, GuestMemory, GuestMemoryMmap, GuestMemoryRegion};
#[cfg(not(all(feature = "kvm", target_os = "linux")))]
use vm_memory::{GuestAddress, GuestMemoryMmap};

pub const PAGE_SIZE: u64 = 4096;

pub fn align_up(value: u64, align: u64) -> u64 {
    if align == 0 {
        return value;
    }
    let mask = align - 1;
    (value + mask) & !mask
}

pub fn align_down(value: u64, align: u64) -> u64 {
    if align == 0 {
        return value;
    }
    value & !(align - 1)
}

pub struct GuestMemoryWrapper {
    mem: GuestMemoryMmap,
    size: u64,
}

impl GuestMemoryWrapper {
    pub fn new(size_bytes: u64) -> Result<Self> {
        let size = align_up(size_bytes, PAGE_SIZE);
        info!(mem_bytes = size, "allocating guest memory");
        let mem = GuestMemoryMmap::from_ranges(&[(GuestAddress(0), size as usize)])?;
        Ok(GuestMemoryWrapper { mem, size })
    }

    #[cfg(all(feature = "kvm", target_os = "linux"))]
    pub fn register(&self, vm_fd: &VmFd) -> Result<()> {
        panic!("NOTE TO CLAUDE: YOU PROBABLY MEANT TO USE THE EMULATOR BACKEND!");
        for (slot, region) in self.mem.iter().enumerate() {
            info!(
                slot,
                guest_start = region.start_addr().raw_value(),
                region_len = region.len(),
                "registering guest memory"
            );
            let host_addr = region.as_ptr() as u64;
            let mem_region = kvm_userspace_memory_region {
                slot: slot as u32,
                guest_phys_addr: region.start_addr().raw_value(),
                memory_size: region.len(),
                userspace_addr: host_addr,
                flags: 0,
            };
            unsafe { vm_fd.set_user_memory_region(mem_region)? };
        }
        Ok(())
    }

    pub fn memory(&self) -> &GuestMemoryMmap {
        &self.mem
    }

    pub fn size(&self) -> u64 {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_up_down() {
        assert_eq!(align_up(1, 4096), 4096);
        assert_eq!(align_down(4097, 4096), 4096);
    }
}
