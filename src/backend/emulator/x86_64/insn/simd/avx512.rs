//! AVX-512 SIMD instruction implementations (EVEX-encoded).

use crate::cpu::VcpuExit;
use crate::error::{Error, Result};

use super::super::super::cpu::{InsnContext, X86_64Vcpu};

/// Helper to get ZMM register value (all 8 qwords)
fn get_zmm(vcpu: &X86_64Vcpu, reg: u8) -> [u64; 8] {
    if reg < 16 {
        // ZMM0-15: composed of xmm[0..2], ymm_high[0..2], zmm_high[0..4]
        let r = reg as usize;
        [
            vcpu.regs.xmm[r][0],
            vcpu.regs.xmm[r][1],
            vcpu.regs.ymm_high[r][0],
            vcpu.regs.ymm_high[r][1],
            vcpu.regs.zmm_high[r][0],
            vcpu.regs.zmm_high[r][1],
            vcpu.regs.zmm_high[r][2],
            vcpu.regs.zmm_high[r][3],
        ]
    } else {
        // ZMM16-31: stored in zmm_ext
        let r = (reg - 16) as usize;
        vcpu.regs.zmm_ext[r]
    }
}

/// Helper to set ZMM register value (all 8 qwords)
fn set_zmm(vcpu: &mut X86_64Vcpu, reg: u8, val: [u64; 8]) {
    if reg < 16 {
        let r = reg as usize;
        vcpu.regs.xmm[r][0] = val[0];
        vcpu.regs.xmm[r][1] = val[1];
        vcpu.regs.ymm_high[r][0] = val[2];
        vcpu.regs.ymm_high[r][1] = val[3];
        vcpu.regs.zmm_high[r][0] = val[4];
        vcpu.regs.zmm_high[r][1] = val[5];
        vcpu.regs.zmm_high[r][2] = val[6];
        vcpu.regs.zmm_high[r][3] = val[7];
    } else {
        let r = (reg - 16) as usize;
        vcpu.regs.zmm_ext[r] = val;
    }
}

/// Helper to get XMM register value (2 qwords)
fn get_xmm(vcpu: &X86_64Vcpu, reg: u8) -> [u64; 2] {
    if reg < 16 {
        vcpu.regs.xmm[reg as usize]
    } else {
        let r = (reg - 16) as usize;
        [vcpu.regs.zmm_ext[r][0], vcpu.regs.zmm_ext[r][1]]
    }
}

/// Helper to set XMM register value (2 qwords), zeroing upper bits
fn set_xmm_zero_upper(vcpu: &mut X86_64Vcpu, reg: u8, val: [u64; 2]) {
    if reg < 16 {
        let r = reg as usize;
        vcpu.regs.xmm[r] = val;
        vcpu.regs.ymm_high[r] = [0, 0];
        vcpu.regs.zmm_high[r] = [0, 0, 0, 0];
    } else {
        let r = (reg - 16) as usize;
        vcpu.regs.zmm_ext[r] = [val[0], val[1], 0, 0, 0, 0, 0, 0];
    }
}

/// Helper to get YMM register value (4 qwords)
fn get_ymm(vcpu: &X86_64Vcpu, reg: u8) -> [u64; 4] {
    if reg < 16 {
        let r = reg as usize;
        [
            vcpu.regs.xmm[r][0],
            vcpu.regs.xmm[r][1],
            vcpu.regs.ymm_high[r][0],
            vcpu.regs.ymm_high[r][1],
        ]
    } else {
        let r = (reg - 16) as usize;
        [
            vcpu.regs.zmm_ext[r][0],
            vcpu.regs.zmm_ext[r][1],
            vcpu.regs.zmm_ext[r][2],
            vcpu.regs.zmm_ext[r][3],
        ]
    }
}

/// Helper to set YMM register value (4 qwords), zeroing upper bits
fn set_ymm_zero_upper(vcpu: &mut X86_64Vcpu, reg: u8, val: [u64; 4]) {
    if reg < 16 {
        let r = reg as usize;
        vcpu.regs.xmm[r][0] = val[0];
        vcpu.regs.xmm[r][1] = val[1];
        vcpu.regs.ymm_high[r][0] = val[2];
        vcpu.regs.ymm_high[r][1] = val[3];
        vcpu.regs.zmm_high[r] = [0, 0, 0, 0];
    } else {
        let r = (reg - 16) as usize;
        vcpu.regs.zmm_ext[r] = [val[0], val[1], val[2], val[3], 0, 0, 0, 0];
    }
}

/// VPMULLQ - Multiply Packed Signed Quadword Integers and Store Low Result
/// EVEX.128/256/512.66.0F38.W1 40 /r
///
/// Multiplies packed signed qword integers and stores the low 64 bits
/// of each 128-bit result.
pub fn vpmullq(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let evex = ctx.evex.ok_or_else(|| {
        Error::Emulator("VPMULLQ requires EVEX prefix".to_string())
    })?;

    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    // Calculate full register indices using EVEX extension bits
    let dest = (reg & 0x07) | if evex.r { 0 } else { 8 } | if evex.r_prime { 0 } else { 16 };
    let src1 = (evex.vvvv ^ 0xF) | if evex.v_prime { 0 } else { 16 };
    let src2_base = (rm & 0x07) | if evex.b { 0 } else { 8 } | if evex.x { 0 } else { 16 };

    let vl = match evex.ll {
        0 => 128,
        1 => 256,
        2 => 512,
        _ => 128,
    };
    let num_elements = vl / 64; // Number of qword elements

    // Get source 1 register
    let src1_val = get_zmm(vcpu, src1);

    // Get source 2 (register or memory, with broadcast support)
    let src2_val = if is_memory {
        if evex.broadcast {
            // Broadcast single qword to all elements
            let elem = vcpu.read_mem(addr, 8)?;
            [elem, elem, elem, elem, elem, elem, elem, elem]
        } else {
            // Read vector from memory
            let mut val = [0u64; 8];
            for i in 0..num_elements {
                val[i] = vcpu.read_mem(addr + (i as u64) * 8, 8)?;
            }
            val
        }
    } else {
        get_zmm(vcpu, src2_base)
    };

    // Get opmask register value (k0 means no masking)
    let mask = if evex.aaa == 0 {
        0xFF // No masking
    } else {
        vcpu.regs.k[evex.aaa as usize] as u8
    };

    // Get current destination value (for merge-masking)
    let dest_val = get_zmm(vcpu, dest);

    // Perform multiplication
    let mut result = [0u64; 8];
    for i in 0..num_elements {
        let bit = 1 << i;
        if mask & bit != 0 {
            // Multiply and keep low 64 bits
            let a = src1_val[i] as i64;
            let b = src2_val[i] as i64;
            result[i] = a.wrapping_mul(b) as u64;
        } else if evex.z {
            // Zeroing-masking
            result[i] = 0;
        } else {
            // Merge-masking: keep original value
            result[i] = dest_val[i];
        }
    }

    // Zero upper elements beyond vector length
    for i in num_elements..8 {
        result[i] = 0;
    }

    // Write result based on vector length
    match vl {
        128 => set_xmm_zero_upper(vcpu, dest, [result[0], result[1]]),
        256 => set_ymm_zero_upper(vcpu, dest, [result[0], result[1], result[2], result[3]]),
        512 => set_zmm(vcpu, dest, result),
        _ => {}
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// VPMULLD (EVEX) - Multiply Packed Signed Dword Integers and Store Low Result
/// EVEX.128/256/512.66.0F38.W0 40 /r
pub fn vpmulld_evex(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let evex = ctx.evex.ok_or_else(|| {
        Error::Emulator("VPMULLD requires EVEX prefix".to_string())
    })?;

    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;

    // Calculate full register indices
    let dest = (reg & 0x07) | if evex.r { 0 } else { 8 } | if evex.r_prime { 0 } else { 16 };
    let src1 = (evex.vvvv ^ 0xF) | if evex.v_prime { 0 } else { 16 };
    let src2_base = (rm & 0x07) | if evex.b { 0 } else { 8 } | if evex.x { 0 } else { 16 };

    let vl = match evex.ll {
        0 => 128,
        1 => 256,
        2 => 512,
        _ => 128,
    };
    let num_elements = vl / 32; // Number of dword elements

    // Get source 1 register
    let src1_val = get_zmm(vcpu, src1);

    // Get source 2 (register or memory, with broadcast support)
    let src2_val = if is_memory {
        if evex.broadcast {
            // Broadcast single dword to all elements
            let elem = vcpu.read_mem(addr, 4)? as u32;
            let mut val = [0u64; 8];
            for i in 0..8 {
                val[i] = ((elem as u64) << 32) | (elem as u64);
            }
            val
        } else {
            // Read vector from memory
            let mut val = [0u64; 8];
            for i in 0..(num_elements / 2) {
                val[i] = vcpu.read_mem(addr + (i as u64) * 8, 8)?;
            }
            val
        }
    } else {
        get_zmm(vcpu, src2_base)
    };

    // Get opmask register value
    let mask = if evex.aaa == 0 {
        0xFFFF // No masking
    } else {
        vcpu.regs.k[evex.aaa as usize] as u16
    };

    // Get current destination value
    let dest_val = get_zmm(vcpu, dest);

    // Perform multiplication on dwords
    let mut result = [0u64; 8];
    for qword_idx in 0..(num_elements / 2) {
        let src1_qword = src1_val[qword_idx];
        let src2_qword = src2_val[qword_idx];

        // Process two dwords per qword
        for dword_idx in 0..2 {
            let elem_idx = qword_idx * 2 + dword_idx;
            let bit = 1 << elem_idx;
            let shift = dword_idx * 32;

            if mask & bit != 0 {
                let a = ((src1_qword >> shift) & 0xFFFFFFFF) as i32;
                let b = ((src2_qword >> shift) & 0xFFFFFFFF) as i32;
                let prod = a.wrapping_mul(b) as u32;
                result[qword_idx] |= (prod as u64) << shift;
            } else if evex.z {
                // Zeroing: leave as 0
            } else {
                // Merge: keep original dword
                result[qword_idx] |= (dest_val[qword_idx] >> shift) & 0xFFFFFFFF << shift;
            }
        }
    }

    // Write result based on vector length
    match vl {
        128 => set_xmm_zero_upper(vcpu, dest, [result[0], result[1]]),
        256 => set_ymm_zero_upper(vcpu, dest, [result[0], result[1], result[2], result[3]]),
        512 => set_zmm(vcpu, dest, result),
        _ => {}
    }

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
