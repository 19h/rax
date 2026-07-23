//! AVX-512 packed integer compare instruction implementations.

use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

use super::avx512::{
    elem_signed, elem_unsigned, evex_mask, evex_scaled_disp8_addr, load_mem_bytes, read_reg_bytes,
    vl_bytes_of,
};

/// Integer compare predicate (for the EQ/GT fixed forms and the imm8 VPCMP form).
#[derive(Clone, Copy, PartialEq)]
pub enum CmpPred {
    Eq,
    Lt,
    Le,
    /// "False" (never true) – predicate 3 for VPCMP.
    FalseP,
    Ne,
    Nlt,
    Nle,
    /// "True" (always true) – predicate 7 for VPCMP.
    TrueP,
    /// Greater-than (used by the dedicated VPCMPGT* forms).
    Gt,
}

impl CmpPred {
    fn from_imm(imm: u8) -> CmpPred {
        match imm & 0x7 {
            0 => CmpPred::Eq,
            1 => CmpPred::Lt,
            2 => CmpPred::Le,
            3 => CmpPred::FalseP,
            4 => CmpPred::Ne,
            5 => CmpPred::Nlt,
            6 => CmpPred::Nle,
            _ => CmpPred::TrueP,
        }
    }
}

/// Evaluate the compare predicate over two signed integers represented as
/// sign-extended `i128` values.
fn cmp_eval_signed(pred: CmpPred, a: i128, b: i128) -> bool {
    match pred {
        CmpPred::Eq => a == b,
        CmpPred::Lt => a < b,
        CmpPred::Le => a <= b,
        CmpPred::FalseP => false,
        CmpPred::Ne => a != b,
        CmpPred::Nlt => a >= b,
        CmpPred::Nle => a > b,
        CmpPred::TrueP => true,
        CmpPred::Gt => a > b,
    }
}

/// Evaluate the compare predicate over two unsigned integers represented as
/// zero-extended `u128` values.
fn cmp_eval_unsigned(pred: CmpPred, a: u128, b: u128) -> bool {
    match pred {
        CmpPred::Eq => a == b,
        CmpPred::Lt => a < b,
        CmpPred::Le => a <= b,
        CmpPred::FalseP => false,
        CmpPred::Ne => a != b,
        CmpPred::Nlt => a >= b,
        CmpPred::Nle => a > b,
        CmpPred::TrueP => true,
        CmpPred::Gt => a > b,
    }
}

/// Generic EVEX integer compare into a K-mask destination.
///
/// `elem_size` is 1, 2, 4, or 8 bytes. `has_imm` selects the
/// VPCMP[U]B/W/D/Q immediate-predicate forms; otherwise `fixed_pred` supplies
/// the VPCMPEQ*/VPCMPGT* predicate. Embedded broadcast is defined only for
/// 32-bit and 64-bit memory elements.
pub fn evex_int_cmp(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    elem_size: usize,
    signed: bool,
    fixed_pred: CmpPred,
    has_imm: bool,
) -> Result<Option<VcpuExit>> {
    let evex = ctx
        .evex
        .ok_or_else(|| Error::Emulator("EVEX compare requires EVEX prefix".to_string()))?;

    // Every integer compare in this helper writes K0-K7, so EVEX.R/R' may
    // not extend ModR/M.reg. Opmask destinations use zeroing-only semantics
    // intrinsically and therefore reserve EVEX.z. EVEX.b is valid only for a
    // 32-bit or 64-bit memory broadcast; register sources and byte/word forms
    // must encode it as zero.
    let modrm = ctx.peek_u8()?;
    let register_source = modrm >> 6 == 3;
    if !evex.r
        || !evex.r_prime
        || evex.z
        || evex.ll == 3
        || (evex.broadcast && (register_source || elem_size < 4))
    {
        return vcpu.inject_undefined_instruction();
    }

    let modrm_start = ctx.cursor;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    // Destination is a K-mask register (ModR/M.reg, low 3 bits).
    let k_dst = (reg & 0x7) as usize;
    let src1 = (evex.vvvv ^ 0xF) | if evex.v_prime { 0 } else { 16 };
    let src2_reg = (rm & 0x07) | if evex.b { 0 } else { 8 } | if evex.x { 0 } else { 16 };

    let vl_bytes = vl_bytes_of(evex.ll);
    let num_elems = vl_bytes / elem_size;
    let broadcast_ok = elem_size == 4 || elem_size == 8;
    let addr = if is_memory {
        let scale = if evex.broadcast && broadcast_ok {
            elem_size
        } else {
            vl_bytes
        };
        evex_scaled_disp8_addr(ctx, modrm_start, addr, scale)
    } else {
        addr
    };

    let src1_bytes = read_reg_bytes(vcpu, src1, vl_bytes);
    let src2_bytes = if is_memory {
        if evex.broadcast && broadcast_ok {
            let elem = vcpu.read_mem(addr, elem_size as u8)?;
            let elem_le = elem.to_le_bytes();
            let mut data = [0u8; 64];
            for i in 0..num_elems {
                let base = i * elem_size;
                data[base..base + elem_size].copy_from_slice(&elem_le[..elem_size]);
            }
            data
        } else {
            load_mem_bytes(vcpu, addr, elem_size, num_elems)?
        }
    } else {
        read_reg_bytes(vcpu, src2_reg, vl_bytes)
    };

    let pred = if has_imm {
        let imm = ctx.consume_u8()?;
        CmpPred::from_imm(imm)
    } else {
        fixed_pred
    };
    let writemask = evex_mask(vcpu, evex.aaa, num_elems);

    let mut result = 0u64;
    for i in 0..num_elems {
        // Only compute for active elements; inactive result bits are zero.
        if (writemask >> i) & 1 == 0 {
            continue;
        }
        let base = i * elem_size;
        let cond = if signed {
            cmp_eval_signed(
                pred,
                elem_signed(&src1_bytes[base..base + elem_size], elem_size),
                elem_signed(&src2_bytes[base..base + elem_size], elem_size),
            )
        } else {
            cmp_eval_unsigned(
                pred,
                elem_unsigned(&src1_bytes[base..base + elem_size], elem_size),
                elem_unsigned(&src2_bytes[base..base + elem_size], elem_size),
            )
        };
        if cond {
            result |= 1u64 << i;
        }
    }

    vcpu.regs.k[k_dst] = result;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
