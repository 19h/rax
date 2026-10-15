//! Memory interfaces and Solana SBPF segmented virtual address space support.

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use crate::isa::ebpf::decode::EbpfMemWidth;
use crate::smir::ir::memory::{FlatMemory, MemoryError, SmirMemory};
use crate::smir::ir::types::Endian;

/// Upper 32 bits of a Solana SBPF virtual address select the memory region.
pub const VIRTUAL_ADDRESS_BITS: usize = 32;

/// Size and alignment of each Solana SBPF virtual memory region (`4 GiB`).
pub const MM_REGION_SIZE: u64 = 1 << VIRTUAL_ADDRESS_BITS;

/// Virtual base address of the read-only data region in SBPF V3+.
pub const MM_RODATA_START: u64 = 0;

/// Virtual base address of the bytecode region (`0x1_0000_0000`).
pub const MM_BYTECODE_START: u64 = MM_REGION_SIZE;

/// Virtual base address of the stack region (`0x2_0000_0000`).
pub const MM_STACK_START: u64 = MM_REGION_SIZE * 2;

/// Virtual base address of the heap region (`0x3_0000_0000`).
pub const MM_HEAP_START: u64 = MM_REGION_SIZE * 3;

/// Virtual base address of the program input region (`0x4_0000_0000`).
pub const MM_INPUT_START: u64 = MM_REGION_SIZE * 4;

/// Memory access fault reported by the eBPF / Solana SBPF execution engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EbpfMemFault {
    /// Guest virtual address of the faulting access.
    pub addr: u64,
    /// Size of the attempted access in bytes.
    pub size: usize,
    /// `true` if the access was a store or atomic write.
    pub write: bool,
}

/// Abstract byte-addressable guest memory for the eBPF / Solana SBPF interpreter.
pub trait EbpfMemory {
    /// Read `buf.len()` bytes starting at `addr`.
    fn read_bytes(&mut self, addr: u64, buf: &mut [u8]) -> Result<(), EbpfMemFault>;

    /// Write `data.len()` bytes starting at `addr`.
    fn write_bytes(&mut self, addr: u64, data: &[u8]) -> Result<(), EbpfMemFault>;

    /// Load an unsigned value of `width` bytes in the given `endian`, optionally
    /// sign-extending to 64 bits.
    fn load(
        &mut self,
        addr: u64,
        width: EbpfMemWidth,
        sign_extend: bool,
        endian: Endian,
    ) -> Result<u64, EbpfMemFault> {
        let mut buf = [0u8; 8];
        let n = width.bytes();
        self.read_bytes(addr, &mut buf[..n])?;
        let val = match (width, endian) {
            (EbpfMemWidth::B1, _) => {
                if sign_extend {
                    buf[0] as i8 as i64 as u64
                } else {
                    u64::from(buf[0])
                }
            }
            (EbpfMemWidth::B2, Endian::Little) => {
                let v = u16::from_le_bytes([buf[0], buf[1]]);
                if sign_extend {
                    v as i16 as i64 as u64
                } else {
                    u64::from(v)
                }
            }
            (EbpfMemWidth::B2, Endian::Big) => {
                let v = u16::from_be_bytes([buf[0], buf[1]]);
                if sign_extend {
                    v as i16 as i64 as u64
                } else {
                    u64::from(v)
                }
            }
            (EbpfMemWidth::B4, Endian::Little) => {
                let v = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
                if sign_extend {
                    v as i32 as i64 as u64
                } else {
                    u64::from(v)
                }
            }
            (EbpfMemWidth::B4, Endian::Big) => {
                let v = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
                if sign_extend {
                    v as i32 as i64 as u64
                } else {
                    u64::from(v)
                }
            }
            (EbpfMemWidth::B8, Endian::Little) => u64::from_le_bytes(buf),
            (EbpfMemWidth::B8, Endian::Big) => u64::from_be_bytes(buf),
        };
        Ok(val)
    }

    /// Store the low `width` bytes of `val` to `addr` in the given `endian`.
    fn store(
        &mut self,
        addr: u64,
        val: u64,
        width: EbpfMemWidth,
        endian: Endian,
    ) -> Result<(), EbpfMemFault> {
        match (width, endian) {
            (EbpfMemWidth::B1, _) => self.write_bytes(addr, &[val as u8]),
            (EbpfMemWidth::B2, Endian::Little) => {
                self.write_bytes(addr, &(val as u16).to_le_bytes())
            }
            (EbpfMemWidth::B2, Endian::Big) => self.write_bytes(addr, &(val as u16).to_be_bytes()),
            (EbpfMemWidth::B4, Endian::Little) => {
                self.write_bytes(addr, &(val as u32).to_le_bytes())
            }
            (EbpfMemWidth::B4, Endian::Big) => self.write_bytes(addr, &(val as u32).to_be_bytes()),
            (EbpfMemWidth::B8, Endian::Little) => self.write_bytes(addr, &val.to_le_bytes()),
            (EbpfMemWidth::B8, Endian::Big) => self.write_bytes(addr, &val.to_be_bytes()),
        }
    }
}

impl EbpfMemory for FlatMemory {
    fn read_bytes(&mut self, addr: u64, buf: &mut [u8]) -> Result<(), EbpfMemFault> {
        SmirMemory::read(self, addr, buf).map_err(|_| EbpfMemFault {
            addr,
            size: buf.len(),
            write: false,
        })
    }

    fn write_bytes(&mut self, addr: u64, data: &[u8]) -> Result<(), EbpfMemFault> {
        SmirMemory::write(self, addr, data).map_err(|_| EbpfMemFault {
            addr,
            size: data.len(),
            write: true,
        })
    }
}

impl EbpfMemory for GuestMemoryMmap {
    fn read_bytes(&mut self, addr: u64, buf: &mut [u8]) -> Result<(), EbpfMemFault> {
        self.read_slice(buf, GuestAddress(addr))
            .map_err(|_| EbpfMemFault {
                addr,
                size: buf.len(),
                write: false,
            })
    }

    fn write_bytes(&mut self, addr: u64, data: &[u8]) -> Result<(), EbpfMemFault> {
        self.write_slice(data, GuestAddress(addr))
            .map_err(|_| EbpfMemFault {
                addr,
                size: data.len(),
                write: true,
            })
    }
}

impl EbpfMemory for &GuestMemoryMmap {
    fn read_bytes(&mut self, addr: u64, buf: &mut [u8]) -> Result<(), EbpfMemFault> {
        self.read_slice(buf, GuestAddress(addr))
            .map_err(|_| EbpfMemFault {
                addr,
                size: buf.len(),
                write: false,
            })
    }

    fn write_bytes(&mut self, addr: u64, data: &[u8]) -> Result<(), EbpfMemFault> {
        self.write_slice(data, GuestAddress(addr))
            .map_err(|_| EbpfMemFault {
                addr,
                size: data.len(),
                write: true,
            })
    }
}

/// A single region in a Solana SBPF segmented virtual address space.
#[derive(Debug, Clone)]
pub struct SbpfMemoryRegion {
    /// Guest virtual base address (e.g. `MM_STACK_START`).
    pub vm_addr: u64,
    /// Backing bytes for this region.
    pub data: Vec<u8>,
    /// Whether guest stores to this region are permitted.
    pub writable: bool,
}

impl SbpfMemoryRegion {
    /// Create a read-only virtual memory region.
    pub fn new_readonly(vm_addr: u64, data: Vec<u8>) -> Self {
        Self {
            vm_addr,
            data,
            writable: false,
        }
    }

    /// Create a read-write virtual memory region.
    pub fn new_writable(vm_addr: u64, data: Vec<u8>) -> Self {
        Self {
            vm_addr,
            data,
            writable: true,
        }
    }
}

/// Segmented virtual memory mapping for Solana SBPF programs.
#[derive(Debug, Clone, Default)]
pub struct SbpfMemoryMapping {
    regions: Vec<SbpfMemoryRegion>,
}

impl SbpfMemoryMapping {
    /// Create an empty segmented memory map.
    pub fn new() -> Self {
        Self {
            regions: Vec::new(),
        }
    }

    /// Create a segmented memory map from a list of regions.
    pub fn with_regions(regions: Vec<SbpfMemoryRegion>) -> Self {
        Self { regions }
    }

    /// Create a standard Solana SBPF 5-region virtual address map (`MM_RODATA_START`,
    /// `MM_BYTECODE_START`, `MM_STACK_START`, `MM_HEAP_START`, `MM_INPUT_START`).
    pub fn new_solana_default(
        bytecode: &[u8],
        rodata: &[u8],
        stack_size: usize,
        heap_size: usize,
        input: &[u8],
    ) -> Self {
        let mut map = Self::new();
        map.add_region(MM_RODATA_START, rodata.to_vec(), false);
        map.add_region(MM_BYTECODE_START, bytecode.to_vec(), false);
        map.add_region(MM_STACK_START, vec![0u8; stack_size], true);
        map.add_region(MM_HEAP_START, vec![0u8; heap_size], true);
        map.add_region(MM_INPUT_START, input.to_vec(), true);
        map
    }

    /// Add or replace a memory region at `vm_addr`.
    pub fn add_region(&mut self, vm_addr: u64, data: Vec<u8>, writable: bool) {
        self.regions.push(SbpfMemoryRegion {
            vm_addr,
            data,
            writable,
        });
    }

    /// Borrow a region's data starting at `vm_addr`.
    pub fn region_slice(&self, vm_addr: u64) -> Option<&[u8]> {
        self.regions
            .iter()
            .find(|r| r.vm_addr == vm_addr)
            .map(|r| r.data.as_slice())
    }

    fn find_slice(&self, addr: u64, len: usize) -> Result<&[u8], EbpfMemFault> {
        let end = addr.checked_add(len as u64).ok_or(EbpfMemFault {
            addr,
            size: len,
            write: false,
        })?;
        for region in &self.regions {
            let reg_end = region.vm_addr.saturating_add(region.data.len() as u64);
            if addr >= region.vm_addr && end <= reg_end {
                let off = (addr - region.vm_addr) as usize;
                return Ok(&region.data[off..off + len]);
            }
        }
        Err(EbpfMemFault {
            addr,
            size: len,
            write: false,
        })
    }

    fn find_slice_mut(&mut self, addr: u64, len: usize) -> Result<&mut [u8], EbpfMemFault> {
        let end = addr.checked_add(len as u64).ok_or(EbpfMemFault {
            addr,
            size: len,
            write: true,
        })?;
        for region in &mut self.regions {
            let reg_end = region.vm_addr.saturating_add(region.data.len() as u64);
            if addr >= region.vm_addr && end <= reg_end {
                if !region.writable {
                    return Err(EbpfMemFault {
                        addr,
                        size: len,
                        write: true,
                    });
                }
                let off = (addr - region.vm_addr) as usize;
                return Ok(&mut region.data[off..off + len]);
            }
        }
        Err(EbpfMemFault {
            addr,
            size: len,
            write: true,
        })
    }
}

impl EbpfMemory for SbpfMemoryMapping {
    fn read_bytes(&mut self, addr: u64, buf: &mut [u8]) -> Result<(), EbpfMemFault> {
        let slice = self.find_slice(addr, buf.len())?;
        buf.copy_from_slice(slice);
        Ok(())
    }

    fn write_bytes(&mut self, addr: u64, data: &[u8]) -> Result<(), EbpfMemFault> {
        let slice = self.find_slice_mut(addr, data.len())?;
        slice.copy_from_slice(data);
        Ok(())
    }
}

impl SmirMemory for SbpfMemoryMapping {
    fn read(&mut self, addr: u64, buf: &mut [u8]) -> Result<(), MemoryError> {
        self.read_bytes(addr, buf)
            .map_err(|f| MemoryError::AccessViolation {
                addr: f.addr,
                write: false,
            })
    }

    fn write(&mut self, addr: u64, data: &[u8]) -> Result<(), MemoryError> {
        self.write_bytes(addr, data)
            .map_err(|f| MemoryError::AccessViolation {
                addr: f.addr,
                write: true,
            })
    }

    fn atomic_load(
        &mut self,
        addr: u64,
        size: crate::smir::ir::types::MemWidth,
        _order: crate::smir::ir::types::MemoryOrder,
    ) -> Result<u64, MemoryError> {
        let mut buf = [0u8; 8];
        let n = size.bytes() as usize;
        self.read(addr, &mut buf[..n])?;
        Ok(u64::from_le_bytes(buf))
    }

    fn atomic_store(
        &mut self,
        addr: u64,
        value: u64,
        size: crate::smir::ir::types::MemWidth,
        _order: crate::smir::ir::types::MemoryOrder,
    ) -> Result<(), MemoryError> {
        let bytes = value.to_le_bytes();
        self.write(addr, &bytes[..size.bytes() as usize])
    }

    fn atomic_rmw(
        &mut self,
        addr: u64,
        op: crate::smir::ir::types::AtomicOp,
        operand: u64,
        size: crate::smir::ir::types::MemWidth,
        order: crate::smir::ir::types::MemoryOrder,
    ) -> Result<u64, MemoryError> {
        let old = self.atomic_load(addr, size, order)?;
        let bits = size.bytes() * 8;
        let mask = if bits >= 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        let o = operand & mask;
        let old_m = old & mask;
        let new = match op {
            crate::smir::ir::types::AtomicOp::Add => old_m.wrapping_add(o),
            crate::smir::ir::types::AtomicOp::Sub => old_m.wrapping_sub(o),
            crate::smir::ir::types::AtomicOp::And => old_m & o,
            crate::smir::ir::types::AtomicOp::Or => old_m | o,
            crate::smir::ir::types::AtomicOp::Xor => old_m ^ o,
            crate::smir::ir::types::AtomicOp::Swap => o,
            _ => old_m,
        } & mask;
        self.atomic_store(addr, new, size, order)?;
        Ok(old_m)
    }

    fn compare_and_swap(
        &mut self,
        addr: u64,
        expected: u64,
        new: u64,
        size: crate::smir::ir::types::MemWidth,
        success_order: crate::smir::ir::types::MemoryOrder,
        _failure_order: crate::smir::ir::types::MemoryOrder,
    ) -> Result<(u64, bool), MemoryError> {
        let old = self.atomic_load(addr, size, success_order)?;
        let bits = size.bytes() * 8;
        let mask = if bits >= 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        let old_m = old & mask;
        let exp_m = expected & mask;
        if old_m == exp_m {
            self.atomic_store(addr, new & mask, size, success_order)?;
            Ok((old_m, true))
        } else {
            Ok((old_m, false))
        }
    }

    fn load_exclusive(
        &mut self,
        addr: u64,
        size: crate::smir::ir::types::MemWidth,
    ) -> Result<u64, MemoryError> {
        self.atomic_load(addr, size, crate::smir::ir::types::MemoryOrder::Acquire)
    }

    fn store_exclusive(
        &mut self,
        addr: u64,
        value: u64,
        size: crate::smir::ir::types::MemWidth,
    ) -> Result<bool, MemoryError> {
        self.atomic_store(
            addr,
            value,
            size,
            crate::smir::ir::types::MemoryOrder::Release,
        )?;
        Ok(true)
    }

    fn clear_exclusive(&mut self) {}

    fn fence(&mut self, _kind: crate::smir::ir::types::FenceKind) {}

    fn probe(&self, addr: u64, size: usize, write: bool) -> Result<(), MemoryError> {
        let end = addr
            .checked_add(size as u64)
            .ok_or(MemoryError::OutOfBounds { addr })?;
        for region in &self.regions {
            let reg_end = region.vm_addr.saturating_add(region.data.len() as u64);
            if addr >= region.vm_addr && end <= reg_end {
                if write && !region.writable {
                    return Err(MemoryError::AccessViolation { addr, write: true });
                }
                return Ok(());
            }
        }
        Err(MemoryError::OutOfBounds { addr })
    }
}

impl crate::smir::lift::MemoryReader for SbpfMemoryMapping {
    fn read(&self, addr: u64, size: usize) -> Result<Vec<u8>, MemoryError> {
        self.find_slice(addr, size)
            .map(|s| s.to_vec())
            .map_err(|_| MemoryError::OutOfBounds { addr })
    }
}
