//! Guest-order atomics built from the memory backend's indivisible primitives.

use super::{MemoryError, SmirMemory};
use crate::smir::ir::types::{AtomicOp, Endian, MemWidth, MemoryOrder};

fn mask(size: MemWidth, addr: u64) -> Result<u64, MemoryError> {
    match size {
        MemWidth::B1 => Ok(0xff),
        MemWidth::B2 => Ok(0xffff),
        MemWidth::B4 => Ok(0xffff_ffff),
        MemWidth::B8 => Ok(u64::MAX),
        _ => Err(MemoryError::AccessViolation { addr, write: true }),
    }
}

fn convert(value: u64, size: MemWidth, endian: Endian) -> u64 {
    match (endian, size) {
        (Endian::Big, MemWidth::B2) => u64::from((value as u16).swap_bytes()),
        (Endian::Big, MemWidth::B4) => u64::from((value as u32).swap_bytes()),
        (Endian::Big, MemWidth::B8) => value.swap_bytes(),
        _ => value,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn compare_exchange<M: SmirMemory + ?Sized>(
    memory: &mut M,
    addr: u64,
    expected: u64,
    new: u64,
    size: MemWidth,
    success: MemoryOrder,
    failure: MemoryOrder,
    endian: Endian,
) -> Result<(u64, bool), MemoryError> {
    let mask = mask(size, addr)?;
    memory.probe(addr, size.bytes() as usize, true)?;
    let (old, exchanged) = memory.compare_and_swap(
        addr,
        convert(expected & mask, size, endian),
        convert(new & mask, size, endian),
        size,
        success,
        failure,
    )?;
    Ok((convert(old & mask, size, endian), exchanged))
}

pub(super) fn rmw<M: SmirMemory + ?Sized>(
    memory: &mut M,
    addr: u64,
    op: AtomicOp,
    operand: u64,
    size: MemWidth,
    order: MemoryOrder,
    endian: Endian,
) -> Result<u64, MemoryError> {
    let mask = mask(size, addr)?;
    memory.probe(addr, size.bytes() as usize, true)?;
    if endian == Endian::Little || size == MemWidth::B1 {
        return memory.atomic_rmw(addr, op, operand, size, order);
    }
    let operand = operand & mask;
    let bits = size.bytes() * 8;
    let signed = |value: u64| ((value << (64 - bits)) as i64) >> (64 - bits);
    let mut observed = memory.atomic_load(addr, size, order.cas_failure())? & mask;
    loop {
        let old = convert(observed, size, endian);
        let new = match op {
            AtomicOp::Add => old.wrapping_add(operand),
            AtomicOp::Sub => old.wrapping_sub(operand),
            AtomicOp::Neg => 0u64.wrapping_sub(old),
            AtomicOp::And => old & operand,
            AtomicOp::Or => old | operand,
            AtomicOp::Xor => old ^ operand,
            AtomicOp::Nand => !(old & operand),
            AtomicOp::Min => signed(old).min(signed(operand)) as u64,
            AtomicOp::Max => signed(old).max(signed(operand)) as u64,
            AtomicOp::Umin => old.min(operand),
            AtomicOp::Umax => old.max(operand),
            AtomicOp::Swap => operand,
        } & mask;
        let (actual, success) = memory.compare_and_swap(
            addr,
            observed,
            convert(new, size, endian),
            size,
            order,
            order.cas_failure(),
        )?;
        if success {
            return Ok(old);
        }
        observed = actual & mask;
    }
}
