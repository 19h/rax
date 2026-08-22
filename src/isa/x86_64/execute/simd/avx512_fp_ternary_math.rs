//! EVEX floating-point ternary arithmetic (`VSCALEF*` and `VRANGE*`).

use super::{
    apply_evex_mask, evex_mask, evex_scaled_disp8_addr, evex_three_op, f64_to_fp_bits,
    fp_bits_to_f64, fp_is_nan, fp_is_quiet_nan, fp_qnan_indefinite, fp_quiet_nan, load_mem_bytes,
    read_lane_u64, read_reg_bytes, vl_bytes_of, write_lane_bits, write_vec_vl,
};
use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

#[derive(Clone, Copy)]
pub enum FpTernaryMathOp {
    ScaleF,
    Range,
}

/// VRANGE on non-NaN operands (NaN cases are resolved at the bit level by the
/// caller). `a` is SRC1, `b` is SRC2. imm[1:0] selects the compare (min / max /
/// min-magnitude / max-magnitude); imm[3:2] selects the sign of the result.
fn fp_range_result(a: f64, b: f64, imm: u8) -> f64 {
    let result = match imm & 0x03 {
        0 => a.min(b),
        1 => a.max(b),
        2 => {
            if a.abs() <= b.abs() {
                a
            } else {
                b
            }
        }
        _ => {
            if a.abs() >= b.abs() {
                a
            } else {
                b
            }
        }
    };

    match (imm >> 2) & 0x03 {
        0 => result.abs().copysign(a), // sign of SRC1
        1 => result,                   // sign of the selected value
        2 => result.abs(),             // force positive
        _ => -(result.abs()),          // force negative
    }
}

/// VRANGE element including NaN handling, on the raw element bits. An SNaN in
/// either source forces a quieted NaN result (SRC1 priority); a QNaN is
/// "transparent" and yields the other source (SRC1 priority when both QNaN).
fn fp_range_bits(a_bits: u64, b_bits: u64, elem_size: usize, imm: u8) -> u64 {
    let a_nan = fp_is_nan(a_bits, elem_size);
    let b_nan = fp_is_nan(b_bits, elem_size);
    if a_nan && !fp_is_quiet_nan(a_bits, elem_size) {
        fp_quiet_nan(a_bits, elem_size)
    } else if b_nan && !fp_is_quiet_nan(b_bits, elem_size) {
        fp_quiet_nan(b_bits, elem_size)
    } else if a_nan && b_nan {
        fp_quiet_nan(a_bits, elem_size)
    } else if a_nan {
        b_bits
    } else if b_nan {
        a_bits
    } else {
        let a = fp_bits_to_f64(a_bits, elem_size);
        let b = fp_bits_to_f64(b_bits, elem_size);
        f64_to_fp_bits(fp_range_result(a, b, imm), elem_size)
    }
}

fn fp_layout(elem_size: usize) -> (u64, u64) {
    match elem_size {
        2 => (0x8000, 0x7c00),
        4 => (0x8000_0000, 0x7f80_0000),
        8 => (0x8000_0000_0000_0000, 0x7ff0_0000_0000_0000),
        _ => unreachable!("validated VSCALEF/VRANGE element width"),
    }
}

/// VSCALEF special-value handling from the Intel operand table. Work on raw
/// bits so NaN sign/payload and signed zero survive without an f64 round trip.
fn fp_scale_bits(a_bits: u64, b_bits: u64, elem_size: usize) -> u64 {
    let (sign, exponent) = fp_layout(elem_size);
    let a_nan = fp_is_nan(a_bits, elem_size);
    let b_nan = fp_is_nan(b_bits, elem_size);
    let a_snan = a_nan && !fp_is_quiet_nan(a_bits, elem_size);
    let b_snan = b_nan && !fp_is_quiet_nan(b_bits, elem_size);

    if a_snan || (a_nan && b_snan) {
        return fp_quiet_nan(a_bits, elem_size);
    }
    if b_snan {
        return fp_quiet_nan(b_bits, elem_size);
    }

    let a_infinite = a_bits & !sign == exponent;
    let b_infinite = b_bits & !sign == exponent;
    let b_negative = b_bits & sign != 0;
    if a_nan {
        return if b_infinite {
            if b_negative { 0 } else { exponent }
        } else {
            fp_quiet_nan(a_bits, elem_size)
        };
    }
    if b_nan {
        return fp_quiet_nan(b_bits, elem_size);
    }

    let a_zero = a_bits & !sign == 0;
    if a_infinite {
        return if b_infinite && b_negative {
            fp_qnan_indefinite(elem_size)
        } else {
            a_bits
        };
    }
    if a_zero {
        return if b_infinite && !b_negative {
            fp_qnan_indefinite(elem_size)
        } else {
            a_bits
        };
    }
    if b_infinite {
        return if b_negative {
            a_bits & sign
        } else {
            (a_bits & sign) | exponent
        };
    }

    let a = fp_bits_to_f64(a_bits, elem_size);
    let b = fp_bits_to_f64(b_bits, elem_size);
    f64_to_fp_bits(a * 2.0f64.powf(b.floor()), elem_size)
}

fn read_second_source(
    vcpu: &mut X86_64Vcpu,
    evex: &crate::isa::x86_64::cpu::EvexPrefix,
    src2_reg: u8,
    is_memory: bool,
    addr: u64,
    vl_bytes: usize,
    elem_size: usize,
    scalar: bool,
) -> Result<[u8; 64]> {
    if !is_memory {
        return Ok(read_reg_bytes(vcpu, src2_reg, vl_bytes));
    }

    let num_elems = if scalar { 1 } else { vl_bytes / elem_size };
    if evex.broadcast && !scalar {
        let value = load_mem_bytes(vcpu, addr, elem_size, 1)?;
        let mut data = [0u8; 64];
        for lane in 0..num_elems {
            let base = lane * elem_size;
            data[base..base + elem_size].copy_from_slice(&value[..elem_size]);
        }
        Ok(data)
    } else {
        load_mem_bytes(vcpu, addr, elem_size, num_elems)
    }
}

/// Execute VSCALEF or VRANGE packed/scalar forms in O(VL / element width)
/// time and O(1) auxiliary space.
pub fn evex_fp_ternary_math(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    elem_size: usize,
    op: FpTernaryMathOp,
    scalar: bool,
    has_imm: bool,
) -> Result<Option<VcpuExit>> {
    let evex = ctx
        .evex
        .ok_or_else(|| Error::Emulator("EVEX FP ternary math requires EVEX prefix".to_string()))?;
    let modrm_start = ctx.cursor;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let imm = if has_imm { ctx.consume_u8()? } else { 0 };

    let (dest, src1, src2_reg) = evex_three_op(&evex, reg, rm);
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

    let src1_bytes = read_reg_bytes(vcpu, src1, vl_bytes);
    let src2_bytes = read_second_source(
        vcpu, &evex, src2_reg, is_memory, addr, vl_bytes, elem_size, scalar,
    )?;

    let mut raw = if scalar { src1_bytes } else { [0u8; 64] };
    for lane in 0..num_elems {
        let a_bits = read_lane_u64(&src1_bytes, lane, elem_size);
        let b_bits = read_lane_u64(&src2_bytes, lane, elem_size);
        let out_bits = match op {
            FpTernaryMathOp::ScaleF => fp_scale_bits(a_bits, b_bits, elem_size),
            FpTernaryMathOp::Range => fp_range_bits(a_bits, b_bits, elem_size, imm),
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

#[cfg(test)]
mod tests {
    use super::fp_scale_bits;

    #[test]
    fn scale_f_special_values_follow_the_operand_table() {
        const POSITIVE_INFINITY: u64 = 0x7f80_0000;
        const NEGATIVE_INFINITY: u64 = 0xff80_0000;
        const FIRST_QNAN: u64 = 0x7fc1_2345;
        const FIRST_SNAN: u64 = 0x7f81_2345;
        const SECOND_SNAN: u64 = 0xff81_4321;
        const INDEFINITE: u64 = 0xffc0_0000;

        assert_eq!(
            fp_scale_bits(FIRST_QNAN, POSITIVE_INFINITY, 4),
            POSITIVE_INFINITY
        );
        assert_eq!(fp_scale_bits(FIRST_QNAN, NEGATIVE_INFINITY, 4), 0);
        assert_eq!(
            fp_scale_bits(FIRST_SNAN, 1.0f32.to_bits().into(), 4),
            FIRST_QNAN
        );
        assert_eq!(fp_scale_bits(FIRST_QNAN, SECOND_SNAN, 4), FIRST_QNAN);
        assert_eq!(
            fp_scale_bits(POSITIVE_INFINITY, NEGATIVE_INFINITY, 4),
            INDEFINITE
        );
        assert_eq!(fp_scale_bits(0, POSITIVE_INFINITY, 4), INDEFINITE);
        assert_eq!(
            fp_scale_bits((-1.0f32).to_bits().into(), POSITIVE_INFINITY, 4),
            NEGATIVE_INFINITY
        );
        assert_eq!(
            fp_scale_bits((-1.0f32).to_bits().into(), NEGATIVE_INFINITY, 4),
            0x8000_0000
        );
    }
}
