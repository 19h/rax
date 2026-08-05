//! EVEX floating-point special-function execution frontiers.

use super::{
    FpUnaryMathOp, apply_evex_mask, evex_mask, evex_reg_vec, evex_rm_vec, evex_scaled_disp8_addr,
    evex_three_op, fixup_response_f32, fixup_response_f64, fp_is_nan, fp_quiet_nan,
    fp_unary_math_bits, load_mem_bytes, read_fp_elem, read_lane_u64, read_reg_bytes, vl_bytes_of,
    write_lane_bits, write_vec_vl,
};
use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

/// EVEX VFIXUPIMMPS/PD/SS/SD: table-driven fixup of special FP values.
pub fn evex_fixupimm(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    elem_size: usize,
    scalar: bool,
) -> Result<Option<VcpuExit>> {
    let evex = ctx
        .evex
        .ok_or_else(|| Error::Emulator("EVEX FIXUPIMM requires EVEX prefix".to_string()))?;
    let modrm_is_memory = ctx.peek_u8()? >> 6 != 3;

    // Scalar EVEX instructions do not support broadcast. EVEX.b selects SAE
    // only for a register source, so the memory-source combination is reserved.
    if scalar && evex.broadcast && modrm_is_memory {
        return vcpu.inject_undefined_instruction();
    }

    let modrm_start = ctx.cursor;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    debug_assert_eq!(is_memory, modrm_is_memory);
    let imm = ctx.consume_u8()?;
    let _ = imm;

    let (dest, src1, src2_reg) = evex_three_op(&evex, reg, rm);
    let vl_bytes = if scalar { 16 } else { vl_bytes_of(evex.ll) };
    let num_elems = if scalar { 1 } else { vl_bytes / elem_size };
    let addr = if is_memory {
        let scale = if scalar || evex.broadcast {
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
        if evex.broadcast && !scalar {
            let elem = vcpu.read_mem(addr, elem_size as u8)?;
            let elem_le = elem.to_le_bytes();
            let mut data = [0u8; 64];
            for lane in 0..num_elems {
                let base = lane * elem_size;
                data[base..base + elem_size].copy_from_slice(&elem_le[..elem_size]);
            }
            data
        } else {
            load_mem_bytes(vcpu, addr, elem_size, num_elems)?
        }
    } else {
        read_reg_bytes(vcpu, src2_reg, vl_bytes)
    };

    let dest_old = read_reg_bytes(vcpu, dest, vl_bytes);
    let mut raw = dest_old;
    for lane in 0..num_elems {
        let dest_bits = read_fp_elem(&dest_old, lane, elem_size);
        let src_bits = read_fp_elem(&src1_bytes, lane, elem_size);
        let table = read_fp_elem(&src2_bytes, lane, elem_size);
        let fixed = match elem_size {
            4 => fixup_response_f32(dest_bits as u32, src_bits as u32, table) as u64,
            8 => fixup_response_f64(dest_bits, src_bits, table),
            _ => {
                return Err(Error::Emulator(format!(
                    "EVEX FIXUPIMM invalid element size {elem_size}"
                )));
            }
        };
        write_lane_bits(&mut raw, lane, elem_size, fixed);
    }

    let result = if scalar {
        let mut result = [0u8; 64];
        result[elem_size..16].copy_from_slice(&src1_bytes[elem_size..16]);
        let active = evex.aaa == 0 || (vcpu.regs.k[evex.aaa as usize] & 1) != 0;
        if active {
            result[..elem_size].copy_from_slice(&raw[..elem_size]);
        } else if !evex.z {
            result[..elem_size].copy_from_slice(&dest_old[..elem_size]);
        }
        result
    } else {
        apply_evex_mask(vcpu, &evex, dest, vl_bytes, elem_size, &raw)
    };

    write_vec_vl(vcpu, dest, vl_bytes, &result);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// EVEX FP unary/transcendental helper for VGETEXP, VRCP*, VRSQRT*, VEXP2,
/// VRNDSCALE, VREDUCE, and VGETMANT packed/scalar forms.
pub fn evex_fp_unary_math(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    elem_size: usize,
    op: FpUnaryMathOp,
    scalar: bool,
    has_imm: bool,
) -> Result<Option<VcpuExit>> {
    let evex = ctx
        .evex
        .ok_or_else(|| Error::Emulator("EVEX FP unary math requires EVEX prefix".to_string()))?;
    let modrm_is_memory = ctx.peek_u8()? >> 6 != 3;

    let scalar_b_is_reserved = modrm_is_memory
        || matches!(op, FpUnaryMathOp::Rcp14 | FpUnaryMathOp::Rsqrt14)
        || (elem_size == 2 && matches!(op, FpUnaryMathOp::Rcp | FpUnaryMathOp::Rsqrt));
    if scalar && evex.broadcast && scalar_b_is_reserved {
        return vcpu.inject_undefined_instruction();
    }

    let modrm_start = ctx.cursor;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    debug_assert_eq!(is_memory, modrm_is_memory);
    let imm = if has_imm { ctx.consume_u8()? } else { 0 };

    let dest = evex_reg_vec(&evex, reg);
    let src = evex_rm_vec(&evex, rm);
    let vl_bytes = if scalar { 16 } else { vl_bytes_of(evex.ll) };
    let num_elems = if scalar { 1 } else { vl_bytes / elem_size };
    let addr = if is_memory {
        let scale = if evex.broadcast || scalar {
            elem_size
        } else {
            vl_bytes
        };
        evex_scaled_disp8_addr(ctx, modrm_start, addr, scale)
    } else {
        addr
    };

    let src_bytes = if is_memory {
        if evex.broadcast && !scalar {
            let elem = vcpu.read_mem(addr, elem_size as u8)?;
            let elem_le = elem.to_le_bytes();
            let mut data = [0u8; 64];
            for lane in 0..num_elems {
                let base = lane * elem_size;
                data[base..base + elem_size].copy_from_slice(&elem_le[..elem_size]);
            }
            data
        } else {
            load_mem_bytes(vcpu, addr, elem_size, num_elems)?
        }
    } else {
        read_reg_bytes(vcpu, src, vl_bytes)
    };

    let mut raw = if scalar {
        read_reg_bytes(vcpu, ctx.evex_vvvv(), 16)
    } else {
        [0u8; 64]
    };
    for lane in 0..num_elems {
        let in_bits = read_lane_u64(&src_bytes, lane, elem_size);
        // A NaN input is returned quieted with its sign+payload intact for all
        // of these ops (getexp/getmant/reduce/rndscale/rcp/rsqrt/exp2); the f64
        // detour would otherwise canonicalize the payload away.
        let out_bits = if fp_is_nan(in_bits, elem_size) {
            fp_quiet_nan(in_bits, elem_size)
        } else {
            fp_unary_math_bits(op, in_bits, elem_size, imm, vcpu.mxcsr)
        };
        write_lane_bits(&mut raw, lane, elem_size, out_bits);
    }

    let result = if scalar {
        let mut result = raw;
        let dest_old = read_reg_bytes(vcpu, dest, 16);
        let active = (evex_mask(vcpu, evex.aaa, 1) & 1) != 0;
        if !active {
            if evex.z {
                result[..elem_size].fill(0);
            } else {
                result[..elem_size].copy_from_slice(&dest_old[..elem_size]);
            }
        }
        result
    } else {
        apply_evex_mask(vcpu, &evex, dest, vl_bytes, elem_size, &raw)
    };

    write_vec_vl(vcpu, dest, vl_bytes, &result);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
