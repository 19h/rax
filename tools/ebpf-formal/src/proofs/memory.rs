//! The memory contract the CPU and interpreter proofs execute against, and
//! the proof that `FlatMemory` (the backend of every plane) meets it.
//!
//! [`ProofMemory`] is the platform memory of `crate::spec`: one byte array at
//! address 0, where an access that does not fit faults with its start
//! address, width, and direction, and an atomic is an indivisible
//! read-modify-write at its width and byte order.
//!
//! The direct CPU reaches memory through `EbpfMemory::read_bytes`,
//! `write_bytes`, `atomic`, and the default methods built on them; the SMIR
//! interpreter executes eBPF IR through `SmirMemory::read`, `write`,
//! `atomic_rmw_endian`, and `compare_and_swap_endian` (its eBPF memory-fault
//! trap is computed from the operation, not from the backend error). A step
//! over `FlatMemory` therefore equals the step over `ProofMemory` once those
//! methods agree in result and memory effect for every input, which the
//! harnesses below prove.

use rax::isa::ebpf::{AtomicKind, EbpfMemFault, EbpfMemWidth, EbpfMemory};
use rax::smir::ir::memory::{MemoryError, SmirMemory};
use rax::smir::ir::types::{AtomicOp, Endian, FenceKind, MemWidth, MemoryOrder};

use super::{MEMORY, flat_memory, memory_bytes};

/// The specification memory.
#[derive(Clone, Copy)]
pub(crate) struct ProofMemory {
    pub bytes: [u8; MEMORY],
}

impl ProofMemory {
    fn fits(addr: u64, len: usize) -> bool {
        addr.checked_add(len as u64)
            .is_some_and(|end| end <= MEMORY as u64)
    }

    /// The `len`-byte value at `addr` in `endian` order (`fits` holds).
    fn get(&self, addr: u64, len: usize, endian: Endian) -> u64 {
        let at = addr as usize;
        let mut value = [0u8; 8];
        match endian {
            Endian::Little => {
                value[..len].copy_from_slice(&self.bytes[at..at + len]);
                u64::from_le_bytes(value)
            }
            Endian::Big => {
                value[8 - len..].copy_from_slice(&self.bytes[at..at + len]);
                u64::from_be_bytes(value)
            }
        }
    }

    /// Store the low `len` bytes of `value` at `addr` in `endian` order.
    fn put(&mut self, addr: u64, len: usize, value: u64, endian: Endian) {
        let at = addr as usize;
        match endian {
            Endian::Little => {
                self.bytes[at..at + len].copy_from_slice(&value.to_le_bytes()[..len]);
            }
            Endian::Big => {
                self.bytes[at..at + len].copy_from_slice(&value.to_be_bytes()[8 - len..]);
            }
        }
    }

    fn mask(len: usize) -> u64 {
        if len == 8 {
            u64::MAX
        } else {
            (1 << (8 * len)) - 1
        }
    }
}

impl EbpfMemory for ProofMemory {
    fn read_bytes(&mut self, addr: u64, buf: &mut [u8]) -> Result<(), EbpfMemFault> {
        if !Self::fits(addr, buf.len()) {
            return Err(EbpfMemFault {
                addr,
                size: buf.len(),
                write: false,
            });
        }
        let at = addr as usize;
        buf.copy_from_slice(&self.bytes[at..at + buf.len()]);
        Ok(())
    }

    fn write_bytes(&mut self, addr: u64, data: &[u8]) -> Result<(), EbpfMemFault> {
        if !Self::fits(addr, data.len()) {
            return Err(EbpfMemFault {
                addr,
                size: data.len(),
                write: true,
            });
        }
        let at = addr as usize;
        self.bytes[at..at + data.len()].copy_from_slice(data);
        Ok(())
    }

    /// RFC 9669 section 5.3 at the access width; the old value is returned
    /// zero-extended. 8- and 16-bit atomics do not exist.
    fn atomic(
        &mut self,
        addr: u64,
        width: EbpfMemWidth,
        endian: Endian,
        kind: AtomicKind,
        operand: u64,
        expected: u64,
    ) -> Result<u64, EbpfMemFault> {
        let len = width.bytes();
        let fault = EbpfMemFault {
            addr,
            size: len,
            write: true,
        };
        if !matches!(len, 4 | 8) || !Self::fits(addr, len) {
            return Err(fault);
        }
        let mask = Self::mask(len);
        let old = self.get(addr, len, endian);
        let operand = operand & mask;
        let new = match kind {
            AtomicKind::Add { .. } => old.wrapping_add(operand),
            AtomicKind::Or { .. } => old | operand,
            AtomicKind::And { .. } => old & operand,
            AtomicKind::Xor { .. } => old ^ operand,
            AtomicKind::Xchg => operand,
            AtomicKind::Cmpxchg => {
                if old == expected & mask {
                    operand
                } else {
                    old
                }
            }
        };
        self.put(addr, len, new & mask, endian);
        Ok(old)
    }
}

/// The SMIR interpreter reaches eBPF memory through `read`, `write`, and the
/// guest-order atomics, which are the specification here; the little-endian
/// primitives below exist for the trait and are not used by eBPF IR.
impl SmirMemory for ProofMemory {
    /// RFC 9669 section 5.3: one indivisible read-modify-write at the guest
    /// byte order, returning the old value.
    fn atomic_rmw_endian(
        &mut self,
        addr: u64,
        op: AtomicOp,
        operand: u64,
        size: MemWidth,
        _: MemoryOrder,
        endian: Endian,
    ) -> Result<u64, MemoryError> {
        let len = size.bytes() as usize;
        if !Self::fits(addr, len) {
            return Err(MemoryError::OutOfBounds { addr });
        }
        let mask = Self::mask(len);
        let old = self.get(addr, len, endian);
        let operand = operand & mask;
        let new = match op {
            AtomicOp::Add => old.wrapping_add(operand),
            AtomicOp::Or => old | operand,
            AtomicOp::And => old & operand,
            AtomicOp::Xor => old ^ operand,
            AtomicOp::Swap => operand,
            // The eBPF lifter emits only the operations above.
            _ => return Err(MemoryError::ExclusiveFailed),
        };
        self.put(addr, len, new & mask, endian);
        Ok(old)
    }

    /// Compare-and-exchange at the guest byte order; a failed comparison
    /// still requires a writable location.
    fn compare_and_swap_endian(
        &mut self,
        addr: u64,
        expected: u64,
        new: u64,
        size: MemWidth,
        _: MemoryOrder,
        _: MemoryOrder,
        endian: Endian,
    ) -> Result<(u64, bool), MemoryError> {
        let len = size.bytes() as usize;
        if !Self::fits(addr, len) {
            return Err(MemoryError::OutOfBounds { addr });
        }
        let mask = Self::mask(len);
        let old = self.get(addr, len, endian);
        let exchanged = old == expected & mask;
        if exchanged {
            self.put(addr, len, new & mask, endian);
        }
        Ok((old, exchanged))
    }

    fn read(&mut self, addr: u64, buf: &mut [u8]) -> Result<(), MemoryError> {
        EbpfMemory::read_bytes(self, addr, buf).map_err(|_| MemoryError::OutOfBounds { addr })
    }

    fn write(&mut self, addr: u64, data: &[u8]) -> Result<(), MemoryError> {
        EbpfMemory::write_bytes(self, addr, data).map_err(|_| MemoryError::OutOfBounds { addr })
    }

    fn atomic_load(
        &mut self,
        addr: u64,
        size: MemWidth,
        _: MemoryOrder,
    ) -> Result<u64, MemoryError> {
        let len = size.bytes() as usize;
        if !Self::fits(addr, len) {
            return Err(MemoryError::OutOfBounds { addr });
        }
        Ok(self.get(addr, len, Endian::Little))
    }

    fn atomic_store(
        &mut self,
        addr: u64,
        value: u64,
        size: MemWidth,
        _: MemoryOrder,
    ) -> Result<(), MemoryError> {
        let len = size.bytes() as usize;
        if !Self::fits(addr, len) {
            return Err(MemoryError::OutOfBounds { addr });
        }
        self.put(addr, len, value, Endian::Little);
        Ok(())
    }

    fn compare_and_swap(
        &mut self,
        addr: u64,
        expected: u64,
        new: u64,
        size: MemWidth,
        _: MemoryOrder,
        _: MemoryOrder,
    ) -> Result<(u64, bool), MemoryError> {
        let old = self.atomic_load(addr, size, MemoryOrder::SeqCst)?;
        let mask = Self::mask(size.bytes() as usize);
        let success = old == expected & mask;
        if success {
            self.atomic_store(addr, new, size, MemoryOrder::SeqCst)?;
        }
        Ok((old, success))
    }

    fn atomic_rmw(
        &mut self,
        addr: u64,
        op: AtomicOp,
        operand: u64,
        size: MemWidth,
        _: MemoryOrder,
    ) -> Result<u64, MemoryError> {
        let old = self.atomic_load(addr, size, MemoryOrder::SeqCst)?;
        let mask = Self::mask(size.bytes() as usize);
        let operand = operand & mask;
        let new = match op {
            AtomicOp::Add => old.wrapping_add(operand),
            AtomicOp::Or => old | operand,
            AtomicOp::And => old & operand,
            AtomicOp::Xor => old ^ operand,
            AtomicOp::Swap => operand,
            // The eBPF lifter emits only the operations above.
            _ => return Err(MemoryError::ExclusiveFailed),
        };
        self.atomic_store(addr, new & mask, size, MemoryOrder::SeqCst)?;
        Ok(old)
    }

    fn load_exclusive(&mut self, _: u64, _: MemWidth) -> Result<u64, MemoryError> {
        Err(MemoryError::ExclusiveFailed)
    }

    fn store_exclusive(&mut self, _: u64, _: u64, _: MemWidth) -> Result<bool, MemoryError> {
        Err(MemoryError::ExclusiveFailed)
    }

    fn clear_exclusive(&mut self) {}

    fn fence(&mut self, _: FenceKind) {}

    fn probe(&self, addr: u64, size: usize, _: bool) -> Result<(), MemoryError> {
        if Self::fits(addr, size) {
            Ok(())
        } else {
            Err(MemoryError::OutOfBounds { addr })
        }
    }
}

/// Every byte agrees: one symbolic index stands for all of them (a
/// loop-free comparison, so the harness unwind bound applies only to the
/// engine's own loops).
fn same_memory(flat: &mut rax::smir::ir::memory::FlatMemory, proof: &ProofMemory) {
    let bytes = memory_bytes(flat);
    let index: usize = kani::any();
    kani::assume(index < MEMORY);
    assert!(bytes[index] == proof.bytes[index]);
}

fn symbolic_len() -> usize {
    let len: usize = kani::any();
    kani::assume(matches!(len, 1 | 2 | 4 | 8));
    len
}

fn symbolic_endian() -> Endian {
    if kani::any() {
        Endian::Little
    } else {
        Endian::Big
    }
}

#[kani::proof]
#[kani::unwind(33)]
fn flat_memory_reads_and_writes_meet_the_contract() {
    let bytes: [u8; MEMORY] = kani::any();
    let addr: u64 = kani::any();
    let len = symbolic_len();
    let mut flat = flat_memory(&bytes);
    let mut proof = ProofMemory { bytes };

    let mut flat_buf = [0u8; 8];
    let mut proof_buf = [0u8; 8];
    let read = EbpfMemory::read_bytes(&mut flat, addr, &mut flat_buf[..len]);
    assert!(read == EbpfMemory::read_bytes(&mut proof, addr, &mut proof_buf[..len]));
    assert!(flat_buf == proof_buf);
    let fetched = EbpfMemory::fetch_bytes(&mut flat, addr, &mut flat_buf[..len]);
    assert!(fetched == read);

    let data: [u8; 8] = kani::any();
    let wrote = EbpfMemory::write_bytes(&mut flat, addr, &data[..len]);
    assert!(wrote == EbpfMemory::write_bytes(&mut proof, addr, &data[..len]));
    assert!(memory_bytes(&mut flat) == proof.bytes);

    // SMIR accesses: success exactly when the contract admits the access.
    let mut smir_buf = [0u8; 8];
    assert!(SmirMemory::read(&mut flat, addr, &mut smir_buf[..len]).is_ok() == read.is_ok());
    assert!(SmirMemory::probe(&flat, addr, len, kani::any()).is_ok() == read.is_ok());
    let wrote = SmirMemory::write(&mut flat, addr, &data[..len]).is_ok();
    assert!(wrote == SmirMemory::write(&mut proof, addr, &data[..len]).is_ok());
    assert!(memory_bytes(&mut flat) == proof.bytes);
}

#[kani::proof]
#[kani::unwind(33)]
fn flat_memory_ebpf_atomics_meet_the_contract() {
    let bytes: [u8; MEMORY] = kani::any();
    let addr: u64 = kani::any();
    let width = if kani::any() {
        EbpfMemWidth::B4
    } else {
        EbpfMemWidth::B8
    };
    let fetch: bool = kani::any();
    let kind = match kani::any::<u8>() % 6 {
        0 => AtomicKind::Add { fetch },
        1 => AtomicKind::Or { fetch },
        2 => AtomicKind::And { fetch },
        3 => AtomicKind::Xor { fetch },
        4 => AtomicKind::Xchg,
        _ => AtomicKind::Cmpxchg,
    };
    let endian = symbolic_endian();
    let operand: u64 = kani::any();
    let expected: u64 = kani::any();
    let mut flat = flat_memory(&bytes);
    let mut proof = ProofMemory { bytes };
    let result = EbpfMemory::atomic(&mut flat, addr, width, endian, kind, operand, expected);
    assert!(result == EbpfMemory::atomic(&mut proof, addr, width, endian, kind, operand, expected));
    assert!(memory_bytes(&mut flat) == proof.bytes);
}

/// The big-endian read-modify-write of `FlatMemory` is a compare-and-swap
/// retry loop over its little-endian primitives. Single-threaded, the first
/// exchange succeeds; the unwinding assertion of the bound below proves that
/// no execution reaches a third iteration.
#[kani::proof]
#[kani::unwind(3)]
fn flat_memory_smir_atomics_meet_the_contract() {
    let bytes: [u8; MEMORY] = kani::any();
    let addr: u64 = kani::any();
    let size = if kani::any() {
        MemWidth::B4
    } else {
        MemWidth::B8
    };
    let endian = symbolic_endian();
    let operand: u64 = kani::any();
    let expected: u64 = kani::any();
    let order = MemoryOrder::SeqCst;
    let mut flat = flat_memory(&bytes);
    let mut proof = ProofMemory { bytes };
    if kani::any() {
        let op = match kani::any::<u8>() % 5 {
            0 => AtomicOp::Add,
            1 => AtomicOp::Or,
            2 => AtomicOp::And,
            3 => AtomicOp::Xor,
            _ => AtomicOp::Swap,
        };
        let a = flat.atomic_rmw_endian(addr, op, operand, size, order, endian);
        let b = proof.atomic_rmw_endian(addr, op, operand, size, order, endian);
        assert!(a.is_ok() == b.is_ok());
        assert!(a.ok() == b.ok());
    } else {
        let a = flat.compare_and_swap_endian(addr, expected, operand, size, order, order, endian);
        let b = proof.compare_and_swap_endian(addr, expected, operand, size, order, order, endian);
        assert!(a.is_ok() == b.is_ok());
        assert!(a.ok() == b.ok());
    }
    same_memory(&mut flat, &proof);
}
