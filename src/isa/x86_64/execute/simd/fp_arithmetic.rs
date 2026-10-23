//! Legacy scalar/packed SSE arithmetic with guest MXCSR and precise commit.

use crate::error::Result;
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::smir::interpret::{SmirInterpreter, X86SimdArithmetic};
use crate::vm::vcpu::VcpuExit;

/// Evaluate up to four lanes in O(lanes) time and O(1) space. Load the complete
/// source before changing MXCSR or XMM state. Intel SDM Vol. 1 section 11.5.3
/// gives unmasked pre-computation exceptions priority over post-computation
/// exceptions; an unmasked exception leaves the destination and RIP unchanged.
pub(super) fn execute(
    cpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    operation: X86SimdArithmetic,
) -> Result<Option<VcpuExit>> {
    let (destination, source, is_memory, address, _) = cpu.decode_modrm(ctx)?;
    let scalar = matches!(ctx.rep_prefix, Some(0xf2 | 0xf3));
    let double =
        ctx.rep_prefix == Some(0xf2) || (ctx.rep_prefix != Some(0xf3) && ctx.operand_size_override);
    let element_bits = if double { 64 } else { 32 };
    let source_words = if !is_memory {
        cpu.regs.xmm[source as usize]
    } else if scalar {
        [cpu.read_mem(address, element_bits / 8)?, 0]
    } else {
        [
            cpu.read_mem(address, 8)?,
            cpu.read_mem(address.wrapping_add(8), 8)?,
        ]
    };
    let original = cpu.regs.xmm[destination as usize];
    let first = u128::from(original[0]) | (u128::from(original[1]) << 64);
    let second = u128::from(source_words[0]) | (u128::from(source_words[1]) << 64);
    let mut result = first;
    let mask = if double {
        u64::MAX
    } else {
        u64::from(u32::MAX)
    };
    let lanes = if scalar { 1 } else { 128 / element_bits };
    let mxcsr_before = cpu.mxcsr;
    let mut status = 0;
    for lane in 0..lanes {
        let shift = lane * element_bits;
        let (bits, raised) = SmirInterpreter::x86_eval_simd_arithmetic(
            operation,
            (first >> shift) as u64 & mask,
            (second >> shift) as u64 & mask,
            double,
            mxcsr_before,
        );
        status |= raised;
        result = (result & !(u128::from(mask) << shift)) | (u128::from(bits & mask) << shift);
    }
    let masks = (mxcsr_before >> 7) & 0x3f;
    let pre_status = status & 7;
    let reported_status = if pre_status & !masks != 0 {
        pre_status
    } else {
        status
    };
    cpu.mxcsr |= reported_status;
    if reported_status & !masks != 0 {
        let vector = if cpu.sregs.cr4 & (1 << 10) != 0 {
            19
        } else {
            6
        };
        cpu.inject_exception(vector, None)?;
        return Ok(None);
    }
    cpu.regs.xmm[destination as usize] = [result as u64, (result >> 64) as u64];
    cpu.regs.rip = cpu.regs.rip.wrapping_add(ctx.cursor as u64);
    Ok(None)
}
