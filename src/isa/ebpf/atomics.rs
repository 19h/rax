//! Direct CPU atomic transactions for private and shared guest memory.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use vm_memory::{GuestAddress, GuestMemory, GuestMemoryMmap, VolatileMemory};

use super::{AtomicKind, EbpfMemFault, EbpfMemWidth};
use crate::smir::ir::memory::SmirMemory;
use crate::smir::ir::types::{AtomicOp, Endian, MemWidth, MemoryOrder};

// Serialize overlapping mixed-width atomics between eBPF contexts: Rust's
// atomic reference contract forbids concurrent, overlapping mixed-size accesses.
// No guest callback or re-entry occurs while holding this lock. Host atomic
// users outside this interface must use a consistent width for each object.
static GUEST_ATOMIC_LOCK: Mutex<()> = Mutex::new(());

pub(super) fn private_memory(
    memory: &mut dyn SmirMemory,
    addr: u64,
    width: EbpfMemWidth,
    endian: Endian,
    kind: AtomicKind,
    operand: u64,
    expected: u64,
) -> Result<u64, EbpfMemFault> {
    let fault = || EbpfMemFault {
        addr,
        size: width.bytes(),
        write: true,
    };
    let size = match width {
        EbpfMemWidth::B4 => MemWidth::B4,
        EbpfMemWidth::B8 => MemWidth::B8,
        _ => return Err(fault()),
    };
    let order = MemoryOrder::SeqCst;
    if kind == AtomicKind::Cmpxchg {
        return memory
            .compare_and_swap_endian(addr, expected, operand, size, order, order, endian)
            .map(|(old, _)| old)
            .map_err(|_| fault());
    }
    let op = match kind {
        AtomicKind::Add { .. } => AtomicOp::Add,
        AtomicKind::Or { .. } => AtomicOp::Or,
        AtomicKind::And { .. } => AtomicOp::And,
        AtomicKind::Xor { .. } => AtomicOp::Xor,
        AtomicKind::Xchg => AtomicOp::Swap,
        AtomicKind::Cmpxchg => return Err(fault()),
    };
    memory
        .atomic_rmw_endian(addr, op, operand, size, order, endian)
        .map_err(|_| fault())
}

pub(super) fn shared_memory(
    memory: &GuestMemoryMmap,
    addr: u64,
    width: EbpfMemWidth,
    endian: Endian,
    kind: AtomicKind,
    operand: u64,
    expected: u64,
) -> Result<u64, EbpfMemFault> {
    let fault = || EbpfMemFault {
        addr,
        size: width.bytes(),
        write: true,
    };
    if !matches!(width, EbpfMemWidth::B4 | EbpfMemWidth::B8) {
        return Err(fault());
    }
    let _guard = GUEST_ATOMIC_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let slice = memory
        .get_slice(GuestAddress(addr), width.bytes())
        .map_err(|_| fault())?;
    // vm-memory checks bounds, host alignment, and the reference lifetime. This
    // mapping uses the () bitmap; writes require no dirty-bitmap notification.
    macro_rules! transaction {
        ($atomic:ty, $word:ty) => {{
            let atomic = slice.get_atomic_ref::<$atomic>(0).map_err(|_| fault())?;
            let mut observed = atomic.load(Ordering::SeqCst);
            loop {
                let old = match endian {
                    Endian::Little => <$word>::from_le(observed),
                    Endian::Big => <$word>::from_be(observed),
                };
                let operand = operand as $word;
                if kind == AtomicKind::Cmpxchg && old != expected as $word {
                    return Ok(old as u64);
                }
                let replacement = match kind {
                    AtomicKind::Add { .. } => old.wrapping_add(operand),
                    AtomicKind::Or { .. } => old | operand,
                    AtomicKind::And { .. } => old & operand,
                    AtomicKind::Xor { .. } => old ^ operand,
                    AtomicKind::Xchg | AtomicKind::Cmpxchg => operand,
                };
                let replacement = match endian {
                    Endian::Little => replacement.to_le(),
                    Endian::Big => replacement.to_be(),
                };
                match atomic.compare_exchange_weak(
                    observed,
                    replacement,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(_) => return Ok(old as u64),
                    Err(actual) => observed = actual,
                }
            }
        }};
    }
    match width {
        EbpfMemWidth::B4 => transaction!(AtomicU32, u32),
        EbpfMemWidth::B8 => transaction!(AtomicU64, u64),
        _ => Err(fault()),
    }
}
