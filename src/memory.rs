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
    /// The size reported to the guest via e820
    reported_size: u64,
    /// The actual allocated size (includes padding for per-CPU overflow)
    actual_size: u64,
}

impl GuestMemoryWrapper {
    pub fn new(size_bytes: u64) -> Result<Self> {
        let reported_size = align_up(size_bytes, PAGE_SIZE);

        // Add extra padding for per-CPU allocation that may end up slightly past
        // the reported memory size. The Linux kernel's per-CPU allocator sometimes
        // computes addresses that land just past the end of RAM. By allocating
        // extra physical memory (but not reporting it to the kernel via e820),
        // these accesses will succeed.
        //
        // Observed offsets past RAM:
        // - 64GB guest: ~200MB past end
        // - 32GB guest: ~1.5GB past end
        // - 1GB guest: ~2GB past end
        // Use 4GB padding as a safe margin.
        let padding = 4 * 1024 * 1024 * 1024u64; // 4GB
        let actual_size = align_up(reported_size + padding, PAGE_SIZE);

        info!(
            reported_bytes = reported_size,
            actual_bytes = actual_size,
            padding_bytes = padding,
            "allocating guest memory with per-CPU padding"
        );
        let mem = GuestMemoryMmap::from_ranges(&[(GuestAddress(0), actual_size as usize)])?;
        Ok(GuestMemoryWrapper {
            mem,
            reported_size,
            actual_size,
        })
    }

    #[cfg(all(feature = "kvm", target_os = "linux"))]
    pub fn register(&self, vm_fd: &VmFd) -> Result<()> {
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

    /// Returns the size reported to the guest via e820.
    /// This is less than the actual allocated size (which includes padding).
    pub fn size(&self) -> u64 {
        self.reported_size
    }

    /// Returns the actual allocated size including padding.
    #[allow(dead_code)]
    pub fn actual_size(&self) -> u64 {
        self.actual_size
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
