//! arith.rs

use crate::isa::arm::aarch64::cpu::simd::*;
use crate::isa::arm::aarch64::cpu::*;
use std::collections::HashSet;
use std::fmt::Debug;

use crate::isa::arm::aarch64::exceptions::{
    ExceptionType, SyndromeRegister, build_spsr, exception_target_el, parse_spsr, vector_offset,
};
use crate::isa::arm::aarch64::gic::{Gic, GicConfig};
use crate::isa::arm::aarch64::mmu::{Mmu, MmuConfig, TranslationFault, TranslationGranule};
use crate::isa::arm::aarch64::sysregs::SystemRegisters;
use crate::isa::arm::aarch64::{NUM_ELS, NUM_GPRS, NUM_SIMD_REGS, sctlr};

use crate::isa::arm::common::cpu::{
    ArmCpu, ArmError, ArmException, ArmProfile, ArmVersion, CpuExit, MemoryFaultInfo,
    MemoryFaultType, ProcessorState, WatchpointKind,
};
use crate::isa::arm::common::features::ArmFeatures;
use crate::isa::arm::common::memory::ArmMemory;
use crate::isa::arm::common::sysreg::Aarch64SysRegEncoding;
use crate::vm::vcpu::Aarch64SystemRegisters;

impl AArch64Cpu {


    /// Execute SIMD three-different (disparate) instructions.
    /// These are widening/narrowing operations like multiply-accumulate long.
    pub(crate) fn exec_simd_three_different(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        if (insn >> 31) & 1 != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let q = (insn >> 30) & 1;
        let u = (insn >> 29) & 1;
        let size = (insn >> 22) & 0x3;
        let rm = ((insn >> 16) & 0x1F) as usize;
        let opcode = (insn >> 12) & 0xF;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;

        let bits = 8u32 << size; // source element (or narrowing destination) size
        let esize = (bits / 8) as usize;
        let dbits = 2 * bits; // doubled (wide) element size
        let part = q as usize; // "2" forms use the upper half of the narrow source
        let signed = u == 0;

        let vn = self.v[rn];
        let vm = self.v[rm];
        let vd = self.v[rd];
        let vn_b = vn.to_le_bytes();
        let vm_b = vm.to_le_bytes();

        match opcode {
            // ---- ADDHN/RADDHN (0100), SUBHN/RSUBHN (0110): add/sub then take
            //      the high half, narrowing 2*bits -> bits. ----
            0b0100 | 0b0110 => {
                if size == 0b11 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                let rounding = u == 1;
                let add = opcode == 0b0100;
                let elements = 64 / bits as usize;
                let dmask = elem_mask_u128(dbits);
                let mut packed = 0u64;
                for e in 0..elements {
                    let a = (vn >> (e * dbits as usize)) & dmask;
                    let b = (vm >> (e * dbits as usize)) & dmask;
                    let mut s = if add {
                        a.wrapping_add(b) & dmask
                    } else {
                        a.wrapping_sub(b) & dmask
                    };
                    if rounding {
                        s = s.wrapping_add(1u128 << (bits - 1)) & dmask;
                    }
                    let narrowed = ((s >> bits) & elem_mask_u128(bits)) as u64;
                    packed |= (narrowed & elem_mask(bits)) << (e * bits as usize);
                }
                let mut bytes = vd.to_le_bytes();
                bytes[part * 8..part * 8 + 8].copy_from_slice(&packed.to_le_bytes());
                if part == 0 {
                    bytes[8..16].copy_from_slice(&[0u8; 8]);
                }
                self.v[rd] = u128::from_le_bytes(bytes);
                Ok(CpuExit::Continue)
            }
            // ---- SADDW/UADDW (0001), SSUBW/USUBW (0011): Vn is already wide. ----
            0b0001 | 0b0011 => {
                if size == 0b11 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                let add = opcode == 0b0001;
                let elements = 64 / bits as usize;
                let mut result = 0u128;
                for e in 0..elements {
                    let aw = (vn >> (e * dbits as usize)) & elem_mask_u128(dbits);
                    let awide: i128 = if signed {
                        sext_elem_wide(aw, dbits)
                    } else {
                        aw as i128
                    };
                    let bn = read_elem(&vm_b, part * 8 + e * esize, esize);
                    let bwide: i128 = if signed {
                        sext_elem(bn, bits)
                    } else {
                        uext_elem(bn, bits) as i128
                    };
                    let r = if add { awide + bwide } else { awide - bwide };
                    result |= ((r as u128) & elem_mask_u128(dbits)) << (e * dbits as usize);
                }
                self.v[rd] = result;
                Ok(CpuExit::Continue)
            }
            // ---- Widening L-forms ----
            _ => {
                // PMULL.1Q (size==11) is the only size-3 form.
                if size == 0b11 && opcode != 0b1110 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                if size == 0b11 && opcode == 0b1110 {
                    // PMULL/PMULL2 of 64-bit -> 128-bit polynomial product.
                    if u == 1 {
                        return Err(ArmError::UndefinedInstruction(insn));
                    }
                    let a = (vn >> (part * 64)) as u64;
                    let b = (vm >> (part * 64)) as u64;
                    self.v[rd] = poly_mul_64(a, b);
                    return Ok(CpuExit::Continue);
                }
                // SQDMLAL/SQDMLSL/SQDMULL need a 16- or 32-bit source.
                if matches!(opcode, 0b1001 | 0b1011 | 0b1101) && size == 0b00 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                // PMULL (vector form here) is 8-bit source only.
                if opcode == 0b1110 && size != 0b00 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                let elements = 64 / bits as usize;
                let dmask = elem_mask_u128(dbits);
                let mut result = 0u128;
                for e in 0..elements {
                    let off = part * 8 + e * esize;
                    let an = read_elem(&vn_b, off, esize);
                    let bn = read_elem(&vm_b, off, esize);
                    let (av, bv): (i128, i128) = if signed {
                        (sext_elem(an, bits), sext_elem(bn, bits))
                    } else {
                        (uext_elem(an, bits) as i128, uext_elem(bn, bits) as i128)
                    };
                    let dval = ((vd >> (e * dbits as usize)) & dmask) as u64;
                    let r: u128 = match opcode {
                        0b0000 => ((av + bv) as u128) & dmask,         // SADDL/UADDL
                        0b0010 => ((av - bv) as u128) & dmask,         // SSUBL/USUBL
                        0b0111 => (((av - bv).abs()) as u128) & dmask, // SABDL/UABDL
                        0b0101 => {
                            ((sext_elem_wide(dval as u128, dbits) + (av - bv).abs()) as u128)
                                & dmask
                            // SABAL/UABAL
                        }
                        0b1000 => {
                            ((sext_elem_wide(dval as u128, dbits) + av * bv) as u128) & dmask // SMLAL/UMLAL
                        }
                        0b1010 => {
                            ((sext_elem_wide(dval as u128, dbits) - av * bv) as u128) & dmask // SMLSL/UMLSL
                        }
                        0b1100 => ((av * bv) as u128) & dmask, // SMULL/UMULL
                        0b1110 => {
                            if u == 1 {
                                return Err(ArmError::UndefinedInstruction(insn));
                            }
                            poly_mul_wide(an, bn, bits) as u128 & dmask // PMULL (8->16)
                        }
                        0b1001 | 0b1011 | 0b1101 => {
                            // SQDMLAL / SQDMLSL / SQDMULL (signed only).
                            if u == 1 {
                                return Err(ArmError::UndefinedInstruction(insn));
                            }
                            let dmin = -(1i128 << (dbits - 1));
                            let dmax = (1i128 << (dbits - 1)) - 1;
                            let raw_prod = 2 * av * bv;
                            let prod_saturated = raw_prod < dmin || raw_prod > dmax;
                            let prod = raw_prod.clamp(dmin, dmax);
                            let acc = match opcode {
                                0b1001 => sext_elem_wide(dval as u128, dbits) + prod,
                                0b1011 => sext_elem_wide(dval as u128, dbits) - prod,
                                _ => prod,
                            };
                            let (r, acc_saturated) = sat_signed_q(acc, dbits);
                            if prod_saturated || acc_saturated {
                                self.fpsr |= FPSR_QC;
                            }
                            r as u128 & dmask
                        }
                        _ => return Err(ArmError::UndefinedInstruction(insn)),
                    };
                    result |= r << (e * dbits as usize);
                }
                self.v[rd] = result;
                Ok(CpuExit::Continue)
            }
        }
    }



    /// Execute FCADD / FCMLA: floating-point complex add / fused multiply-add
    /// over interleaved (real, imaginary) element pairs (FEAT_FCMA). `is_fcmla`
    /// selects FCMLA (2-bit rotation) vs FCADD (1-bit rotation).
    pub(crate) fn exec_simd_complex(&mut self, insn: u32, is_fcmla: bool) -> Result<CpuExit, ArmError> {
        if (insn >> 31) & 1 != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let q = (insn >> 30) & 1;
        let size = (insn >> 22) & 0x3;
        let rm = ((insn >> 16) & 0x1F) as usize;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;
        // size: 01=f16, 10=f32, 11=f64. size==00 is reserved.
        if size == 0b00 {
            return Ok(CpuExit::Undefined(insn));
        }
        let esize = 8u32 << size; // 16 / 32 / 64
        if esize == 64 && q == 0 {
            return Ok(CpuExit::Undefined(insn)); // a 64-bit complex pair needs 128 bits
        }
        let datasize = if q == 1 { 128 } else { 64 };
        let pairs = datasize / (2 * esize as usize);
        let mask = elem_mask(esize) as u128;
        let op1 = self.v[rn];
        let op2 = self.v[rm];
        let op3 = self.v[rd];
        let elem = |v: u128, idx: usize| -> u64 { ((v >> (idx * esize as usize)) & mask) as u64 };
        let mut result = 0u128;
        for e in 0..pairs {
            let re = 2 * e;
            let im = 2 * e + 1;
            let (a_re, a_im) = (elem(op1, re), elem(op1, im));
            let (b_re, b_im) = (elem(op2, re), elem(op2, im));
            let (r_re, r_im) = if is_fcmla {
                let rot = (insn >> 11) & 0x3;
                let (a_re_raw, a_im_raw, b_re_raw, b_im_raw) = (a_re, a_im, b_re, b_im);
                let (d_re_raw, d_im_raw) = (elem(op3, re), elem(op3, im));
                let (a_re, a_im) = (
                    fp_flush_input_bits_with_fpcr(a_re, esize, self.fpcr),
                    fp_flush_input_bits_with_fpcr(a_im, esize, self.fpcr),
                );
                let (b_re, b_im) = (
                    fp_flush_input_bits_with_fpcr(b_re, esize, self.fpcr),
                    fp_flush_input_bits_with_fpcr(b_im, esize, self.fpcr),
                );
                let (d_re, d_im) = (
                    fp_flush_input_bits_with_fpcr(d_re_raw, esize, self.fpcr),
                    fp_flush_input_bits_with_fpcr(d_im_raw, esize, self.fpcr),
                );
                // result_re += x_re * y_re; result_im += x_im * y_im.
                let (xr, yr, xi, yi) = match rot {
                    0b00 => (a_re, b_re, a_re, b_im),
                    0b01 => (
                        a_im,
                        fp_neg_bits_with_fpcr(b_im, esize, self.fpcr),
                        a_im,
                        b_re,
                    ),
                    0b10 => (
                        a_re,
                        fp_neg_bits_with_fpcr(b_re, esize, self.fpcr),
                        a_re,
                        fp_neg_bits_with_fpcr(b_im, esize, self.fpcr),
                    ),
                    _ => (
                        a_im,
                        b_im,
                        a_im,
                        fp_neg_bits_with_fpcr(b_re, esize, self.fpcr),
                    ),
                };
                let (xr_raw, yr_raw, xi_raw, yi_raw) = match rot {
                    0b00 => (a_re_raw, b_re_raw, a_re_raw, b_im_raw),
                    0b01 => (
                        a_im_raw,
                        fp_neg_bits_with_fpcr(b_im_raw, esize, self.fpcr),
                        a_im_raw,
                        b_re_raw,
                    ),
                    0b10 => (
                        a_re_raw,
                        fp_neg_bits_with_fpcr(b_re_raw, esize, self.fpcr),
                        a_re_raw,
                        fp_neg_bits_with_fpcr(b_im_raw, esize, self.fpcr),
                    ),
                    _ => (
                        a_im_raw,
                        b_im_raw,
                        a_im_raw,
                        fp_neg_bits_with_fpcr(b_re_raw, esize, self.fpcr),
                    ),
                };
                let r_re = fp_fcmla_muladd_bits_with_fpcr(d_re, xr, yr, esize, self.fpcr);
                let r_im = fp_fcmla_muladd_bits_with_fpcr(d_im, xi, yi, esize, self.fpcr);
                let es = (esize / 8) as usize;
                self.fpsr |= fp_status_fma_with_fpcr(es, d_re_raw, xr_raw, yr_raw, r_re, self.fpcr);
                self.fpsr |= fp_status_fma_with_fpcr(es, d_im_raw, xi_raw, yi_raw, r_im, self.fpcr);
                (r_re, r_im)
            } else {
                // FCADD: rot==0 (90deg): re = a_re + (-b_im), im = a_im + b_re.
                //        rot==1 (270deg): re = a_re + b_im, im = a_im + (-b_re).
                let rot = (insn >> 12) & 1;
                let (add_re, add_im) = if rot == 0 {
                    (fp_neg_bits_with_fpcr(b_im, esize, self.fpcr), b_re)
                } else {
                    (b_im, fp_neg_bits_with_fpcr(b_re, esize, self.fpcr))
                };
                let r_re = fp_add_bits_with_fpcr(a_re, add_re, esize, self.fpcr);
                let r_im = fp_add_bits_with_fpcr(a_im, add_im, esize, self.fpcr);
                let es = (esize / 8) as usize;
                self.fpsr |=
                    fp_status_binop_with_fpcr(es, FpKind::Add, a_re, add_re, r_re, self.fpcr);
                self.fpsr |=
                    fp_status_binop_with_fpcr(es, FpKind::Add, a_im, add_im, r_im, self.fpcr);
                (r_re, r_im)
            };
            result |= (r_re as u128 & mask) << (re * esize as usize);
            result |= (r_im as u128 & mask) << (im * esize as usize);
        }
        self.v[rd] = result;
        Ok(CpuExit::Continue)
    }



    /// Execute FCMLA by element: like vector FCMLA, but the Vm complex pair is
    /// selected once by the H:L (f16) / H (f32) index and reused for every lane.
    pub(crate) fn exec_simd_complex_indexed(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        if (insn >> 31) & 1 != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let q = (insn >> 30) & 1;
        let size = (insn >> 22) & 0x3;
        let rot = (insn >> 13) & 0x3;
        let l = (insn >> 21) & 1;
        let m = (insn >> 20) & 1;
        let h = (insn >> 11) & 1;
        let rm = (((insn >> 16) & 0xF) | (m << 4)) as usize; // Vm = M:Rm
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;
        // Only f16 (size=01) and f32 (size=10) are allocated.
        if size != 0b01 && size != 0b10 {
            return Ok(CpuExit::Undefined(insn));
        }
        let esize = 8u32 << size; // 16 or 32
        let index = if size == 0b01 {
            ((h << 1) | l) as usize
        } else {
            h as usize
        };
        if size == 0b10 && (l == 1 || q == 0) {
            return Ok(CpuExit::Undefined(insn));
        }
        if size == 0b01 && h == 1 && q == 0 {
            return Ok(CpuExit::Undefined(insn));
        }
        let datasize = if q == 1 { 128 } else { 64 };
        let pairs = datasize / (2 * esize as usize);
        let mask = elem_mask(esize) as u128;
        let es = esize as usize;
        let es_bytes = (esize / 8) as usize;
        let op1 = self.v[rn];
        let op2 = self.v[rm];
        let op3 = self.v[rd];
        let elem = |v: u128, idx: usize| -> u64 { ((v >> (idx * es)) & mask) as u64 };
        let m_re_raw = elem(op2, index * 2);
        let m_im_raw = elem(op2, index * 2 + 1);
        let m_re = fp_flush_input_bits_with_fpcr(m_re_raw, esize, self.fpcr);
        let m_im = fp_flush_input_bits_with_fpcr(m_im_raw, esize, self.fpcr);
        let mut result = 0u128;
        for e in 0..pairs {
            let (a_re_raw, a_im_raw) = (elem(op1, 2 * e), elem(op1, 2 * e + 1));
            let (d_re_raw, d_im_raw) = (elem(op3, 2 * e), elem(op3, 2 * e + 1));
            let (a_re, a_im) = (
                fp_flush_input_bits_with_fpcr(a_re_raw, esize, self.fpcr),
                fp_flush_input_bits_with_fpcr(a_im_raw, esize, self.fpcr),
            );
            let (d_re, d_im) = (
                fp_flush_input_bits_with_fpcr(d_re_raw, esize, self.fpcr),
                fp_flush_input_bits_with_fpcr(d_im_raw, esize, self.fpcr),
            );
            let (xr, yr, xi, yi) = match rot {
                0b00 => (a_re, m_re, a_re, m_im),
                0b01 => (
                    a_im,
                    fp_neg_bits_with_fpcr(m_im, esize, self.fpcr),
                    a_im,
                    m_re,
                ),
                0b10 => (
                    a_re,
                    fp_neg_bits_with_fpcr(m_re, esize, self.fpcr),
                    a_re,
                    fp_neg_bits_with_fpcr(m_im, esize, self.fpcr),
                ),
                _ => (
                    a_im,
                    m_im,
                    a_im,
                    fp_neg_bits_with_fpcr(m_re, esize, self.fpcr),
                ),
            };
            let (xr_raw, yr_raw, xi_raw, yi_raw) = match rot {
                0b00 => (a_re_raw, m_re_raw, a_re_raw, m_im_raw),
                0b01 => (
                    a_im_raw,
                    fp_neg_bits_with_fpcr(m_im_raw, esize, self.fpcr),
                    a_im_raw,
                    m_re_raw,
                ),
                0b10 => (
                    a_re_raw,
                    fp_neg_bits_with_fpcr(m_re_raw, esize, self.fpcr),
                    a_re_raw,
                    fp_neg_bits_with_fpcr(m_im_raw, esize, self.fpcr),
                ),
                _ => (
                    a_im_raw,
                    m_im_raw,
                    a_im_raw,
                    fp_neg_bits_with_fpcr(m_re_raw, esize, self.fpcr),
                ),
            };
            let r_re = fp_fcmla_muladd_bits_with_fpcr(d_re, xr, yr, esize, self.fpcr);
            let r_im = fp_fcmla_muladd_bits_with_fpcr(d_im, xi, yi, esize, self.fpcr);
            self.fpsr |=
                fp_status_fma_with_fpcr(es_bytes, d_re_raw, xr_raw, yr_raw, r_re, self.fpcr);
            self.fpsr |=
                fp_status_fma_with_fpcr(es_bytes, d_im_raw, xi_raw, yi_raw, r_im, self.fpcr);
            result |= (r_re as u128 & mask) << (2 * e * es);
            result |= (r_im as u128 & mask) << ((2 * e + 1) * es);
        }
        self.v[rd] = result;
        Ok(CpuExit::Continue)
    }



    /// Execute SDOT/UDOT/USDOT: the 8-bit -> 32-bit four-way dot product. Each
    /// 32-bit lane accumulates four byte-wise products of the corresponding
    /// Vn/Vm bytes. `op1_signed`/`op2_signed` give the byte signedness:
    /// SDOT = (s,s), UDOT = (u,u), USDOT = (u,s).
    pub(crate) fn exec_simd_dot(
        &mut self,
        insn: u32,
        op1_signed: bool,
        op2_signed: bool,
    ) -> Result<CpuExit, ArmError> {
        if (insn >> 31) & 1 != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let q = (insn >> 30) & 1;
        let rm = ((insn >> 16) & 0x1F) as usize;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;
        let lanes = if q == 1 { 4 } else { 2 }; // 32-bit accumulator lanes
        let op1 = self.v[rn];
        let op2 = self.v[rm];
        let byte = |v: u128, sh: usize, signed: bool| -> i64 {
            let b = (v >> sh) as u8;
            if signed { b as i8 as i64 } else { b as i64 }
        };
        let mut result = self.v[rd];
        for e in 0..lanes {
            let mut res: i64 = 0;
            for i in 0..4 {
                let sh = (4 * e + i) * 8;
                res += byte(op1, sh, op1_signed) * byte(op2, sh, op2_signed);
            }
            let lane = (result >> (e * 32)) as u32;
            let updated = (lane as i64).wrapping_add(res) as u32;
            result = (result & !(0xFFFF_FFFFu128 << (e * 32))) | ((updated as u128) << (e * 32));
        }
        if q == 0 {
            result &= 0xFFFF_FFFF_FFFF_FFFF;
        }
        self.v[rd] = result;
        Ok(CpuExit::Continue)
    }



    /// Execute USDOT/SUDOT by element (FEAT_I8MM). The index (H:L) selects a
    /// 4-byte group of Vm reused for every lane. `op1_signed`/`op2_signed` give
    /// the Vn/Vm byte signedness (USDOT = (false,true), SUDOT = (true,false)).
    pub(crate) fn exec_simd_dot_indexed_mixed(
        &mut self,
        insn: u32,
        op1_signed: bool,
        op2_signed: bool,
    ) -> Result<CpuExit, ArmError> {
        if (insn >> 31) & 1 != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let q = (insn >> 30) & 1;
        let l = (insn >> 21) & 1;
        let m = (insn >> 20) & 1;
        let h = (insn >> 11) & 1;
        let rm = (((insn >> 16) & 0xF) | (m << 4)) as usize; // Vm = M:Rm
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;
        let index = ((h << 1) | l) as usize; // H:L, selects a 32-bit group
        let lanes = if q == 1 { 4 } else { 2 };
        let op1 = self.v[rn];
        let op2 = self.v[rm];
        let byte = |v: u128, sh: usize, signed: bool| -> i64 {
            let b = (v >> sh) as u8;
            if signed { b as i8 as i64 } else { b as i64 }
        };
        let base = index * 4;
        let mut result = self.v[rd];
        for e in 0..lanes {
            let mut res: i64 = 0;
            for i in 0..4 {
                res +=
                    byte(op1, (4 * e + i) * 8, op1_signed) * byte(op2, (base + i) * 8, op2_signed);
            }
            let lane = (result >> (e * 32)) as u32;
            let updated = (lane as i64).wrapping_add(res) as u32;
            result = (result & !(0xFFFF_FFFFu128 << (e * 32))) | ((updated as u128) << (e * 32));
        }
        if q == 0 {
            result &= 0xFFFF_FFFF_FFFF_FFFF;
        }
        self.v[rd] = result;
        Ok(CpuExit::Continue)
    }



    /// Execute BFMLALB/BFMLALT (FEAT_BF16): widening bf16 -> f32 fused
    /// multiply-accumulate. Q (bit30) selects the Bottom (0) or Top (1) bf16 of
    /// each f32 pair. The result is always a full 128-bit, 4-lane f32 vector.
    pub(crate) fn exec_simd_bfmlal(&mut self, insn: u32, is_indexed: bool) -> Result<CpuExit, ArmError> {
        if (insn >> 31) & 1 != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let sel = ((insn >> 30) & 1) as usize; // Q: 0=B (low 16), 1=T (high 16)
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;
        let op1 = self.v[rn];
        let op3 = self.v[rd];
        let bf16 = |v: u128, lane: usize| -> u16 { (v >> (lane * 16)) as u16 };
        let (op2, idx): (u128, Option<usize>) = if is_indexed {
            let l = (insn >> 21) & 1;
            let m = (insn >> 20) & 1;
            let h = (insn >> 11) & 1;
            let rm = ((insn >> 16) & 0xF) as usize; // 4-bit, V0..V15
            (self.v[rm], Some(((h << 2) | (l << 1) | m) as usize)) // index = H:L:M
        } else {
            let rm = ((insn >> 16) & 0x1F) as usize;
            (self.v[rm], None)
        };
        let mut result = 0u128;
        for e in 0..4 {
            let b1 = bf16(op1, 2 * e + sel);
            let b2 = match idx {
                // The by-element form selects a single bf16 (Vm.H[index]); the
                // vector form takes the B/T half of pair e.
                Some(ix) => bf16(op2, ix),
                None => bf16(op2, 2 * e + sel),
            };
            let a_raw = (op3 >> (e * 32)) as u64;
            let b1_raw = (b1 as u32 as u64) << 16;
            let b2_raw = (b2 as u32 as u64) << 16;
            let a = bfmlal_f32_input_with_fpcr(a_raw as u32, self.fpcr);
            let b1 = bfmlal_f32_input_with_fpcr(b1_raw as u32, self.fpcr);
            let b2 = bfmlal_f32_input_with_fpcr(b2_raw as u32, self.fpcr);
            // Single-rounded fused multiply-add (FPMulAdd) with ARM-correct NaN
            // selection (addend first); bf16 widens to f32 by a 16-bit shift.
            let r = bfmlal_ah_result(a_raw as u32, b1_raw as u32, b2_raw as u32, self.fpcr)
                .unwrap_or_else(|| {
                    fp_muladd_bits_with_fpcr(a as u64, b1 as u64, b2 as u64, 32, self.fpcr) as u32
                });
            let mut status = if self.fpcr & FPCR_AH != 0 {
                0
            } else {
                fp_status_fma(4, a as u64, b1 as u64, b2 as u64, r as u64)
            };
            if self.fpcr & FPCR_AH == 0
                && fp_fz_fma_output(4, a as u64, b1 as u64, b2 as u64, r as u64, self.fpcr)
                    .is_some()
            {
                status &= !FPSR_IXC;
            }
            self.fpsr |= status
                | bfmlal_f32_input_status(a_raw, self.fpcr)
                | bfmlal_f32_input_status(b1_raw, self.fpcr)
                | bfmlal_f32_input_status(b2_raw, self.fpcr);
            result |= (r as u128) << (e * 32);
        }
        self.v[rd] = result;
        Ok(CpuExit::Continue)
    }



    /// Execute BFDOT (FEAT_BF16): 2-way bf16 dot product accumulated into f32
    /// lanes. The two bf16 products and the f32 accumulator are summed in
    /// unrounded precision and rounded once to f32 with round-to-odd (the
    /// standard FPCR.EBF==0 path).
    pub(crate) fn exec_simd_bfdot(&mut self, insn: u32, is_indexed: bool) -> Result<CpuExit, ArmError> {
        if (insn >> 31) & 1 != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let q = (insn >> 30) & 1;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;
        let lanes = if q == 1 { 4 } else { 2 };
        let op1 = self.v[rn];
        let op3 = self.v[rd];
        let bf16 = |v: u128, lane: usize| -> u16 { (v >> (lane * 16)) as u16 };
        let (op2, idx): (u128, Option<usize>) = if is_indexed {
            let l = (insn >> 21) & 1;
            let m = (insn >> 20) & 1;
            let h = (insn >> 11) & 1;
            let rm = (((insn >> 16) & 0xF) | (m << 4)) as usize; // Vm = M:Rm
            (self.v[rm], Some(((h << 1) | l) as usize)) // index H:L selects a bf16 pair
        } else {
            let rm = ((insn >> 16) & 0x1F) as usize;
            (self.v[rm], None)
        };
        let _ = &bf16;
        let mut result = self.v[rd];
        for e in 0..lanes {
            let acc_bits = (op3 >> (e * 32)) as u32;
            let n_pair = (op1 >> (e * 32)) as u32;
            let m_pair = match idx {
                Some(ix) => (op2 >> (ix * 32)) as u32,
                None => (op2 >> (e * 32)) as u32,
            };
            let r = bf16_dot_result_with_fpcr(bfdotadd_ebf0(acc_bits, n_pair, m_pair), self.fpcr);
            result = (result & !(0xFFFF_FFFFu128 << (e * 32))) | ((r as u128) << (e * 32));
        }
        if q == 0 {
            result &= 0xFFFF_FFFF_FFFF_FFFF;
        }
        self.v[rd] = result;
        Ok(CpuExit::Continue)
    }



    /// Execute BFCVTN/BFCVTN2 (FEAT_BF16): narrow 4 f32 lanes to 4 bf16 lanes
    /// (round-to-nearest-even). BFCVTN (Q=0) writes the low 64 bits and zeroes
    /// the high half; BFCVTN2 (Q=1) writes the high 64 bits, preserving the low.
    pub(crate) fn exec_simd_bfcvtn(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        if (insn >> 31) & 1 != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let q = (insn >> 30) & 1;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;
        let op = self.v[rn];
        let mut narrowed = 0u64;
        for e in 0..4 {
            let x = (op >> (e * 32)) as u32;
            let bf = f32_to_bf16_with_fpcr(x, self.fpcr);
            self.fpsr |= fp_status_bfcvt_with_fpcr(x, bf, self.fpcr);
            narrowed |= (bf as u64) << (e * 16);
        }
        if q == 0 {
            self.v[rd] = narrowed as u128;
        } else {
            self.v[rd] = (self.v[rd] & 0xFFFF_FFFF_FFFF_FFFF) | ((narrowed as u128) << 64);
        }
        Ok(CpuExit::Continue)
    }



    /// Execute BFMMLA (FEAT_BF16): 2x4-by-4x2 bf16 matrix multiply accumulating
    /// into a 2x2 f32 matrix, with the same round-to-odd accumulation as BFDOT.
    pub(crate) fn exec_simd_bfmmla(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        if (insn >> 31) & 1 != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rm = ((insn >> 16) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;
        let op1 = self.v[rn];
        let op2 = self.v[rm];
        let acc = self.v[rd];
        let mut result = 0u128;
        for i in 0..2 {
            for j in 0..2 {
                let lane = 2 * i + j;
                let acc_bits = (acc >> (lane * 32)) as u32;
                // Two bfdotadd steps over the k=0,1 and k=2,3 bf16 pairs, exactly
                // as qemu gvec_bfmmla processes each output lane.
                let n01 = (op1 >> ((4 * i) * 16)) as u32; // bf16 lanes 4i, 4i+1
                let m01 = (op2 >> ((4 * j) * 16)) as u32; // bf16 lanes 4j, 4j+1
                let n23 = (op1 >> ((4 * i + 2) * 16)) as u32; // lanes 4i+2, 4i+3
                let m23 = (op2 >> ((4 * j + 2) * 16)) as u32; // lanes 4j+2, 4j+3
                let s = bfdotadd_ebf0(acc_bits, n01, m01);
                let r = bf16_dot_result_with_fpcr(bfdotadd_ebf0(s, n23, m23), self.fpcr);
                result |= (r as u128) << (lane * 32);
            }
        }
        self.v[rd] = result;
        Ok(CpuExit::Continue)
    }



    /// Execute the SIMD modified-immediate group: MOVI, MVNI, ORR (imm),
    /// BIC (imm) and FMOV (vector immediate).
    pub(crate) fn exec_simd_modified_imm(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        // bit31 is a fixed 0 for the Advanced SIMD modified-immediate group; a
        // set bit31 is a different (unallocated here) encoding and must trap.
        if (insn >> 31) & 1 != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let q = (insn >> 30) & 1;
        let op = (insn >> 29) & 1;
        let cmode = (insn >> 12) & 0xF;
        let rd = (insn & 0x1F) as usize;
        // imm8 = abc:defgh
        let abc = (insn >> 16) & 0x7;
        let defgh = (insn >> 5) & 0x1F;
        let imm8 = ((abc << 5) | defgh) as u8;

        // FP16 FMOV vector immediate (FEAT_FP16): cmode==1111, op==0, o2(bit11)==1.
        // Broadcast the 8-bit half-precision immediate to .4h (Q=0) / .8h (Q=1).
        if cmode == 0b1111 && op == 0 && (insn >> 11) & 1 == 1 {
            let h = vfp_expand_imm_f16(imm8) as u128;
            let lane = h | (h << 16) | (h << 32) | (h << 48);
            self.v[rd] = if q == 1 { lane | (lane << 64) } else { lane };
            return Ok(CpuExit::Continue);
        }

        // Apart from the FP16 FMOV form handled above (cmode==1111, op==0,
        // o2==1), o2 (bit11) is a fixed 0; any other encoding with o2==1 is
        // unallocated and must trap rather than execute as an o2==0 instruction.
        if (insn >> 11) & 1 != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }

        // Some (op, cmode, Q) combinations are UNDEFINED.
        //  - FMOV f64 (op=1, cmode=1111) requires Q==1.
        if op == 1 && cmode == 0b1111 && q == 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        //  - op=1, cmode=1110 is MOVI(64-bit); op=1, cmode=0xx0/10x0 is MVNI;
        //    these are all allocated. The only fully-unallocated case in this
        //    group is handled by the cmode match returning a defined value.

        let imm64 = adv_simd_expand_imm(op, cmode, imm8);

        // ORR/BIC immediate: cmode = 0xx1 or 10x1.
        let orr_bic = (cmode & 1) == 1 && (cmode >> 1) < 0b110;
        if orr_bic {
            let imm128 = (imm64 as u128) | ((imm64 as u128) << 64);
            let cur = self.v[rd];
            let r = if op == 0 { cur | imm128 } else { cur & !imm128 };
            self.v[rd] = if q == 1 { r } else { r & elem_mask_u128(64) };
            return Ok(CpuExit::Continue);
        }

        // MOVI / MVNI / FMOV. MVNI inverts for op=1 except the cmode=1110
        // (MOVI 64-bit) and cmode=1111 (FMOV) special cases.
        let val = if op == 1 && cmode != 0b1110 && cmode != 0b1111 {
            !imm64
        } else {
            imm64
        };
        let result = if q == 1 {
            (val as u128) | ((val as u128) << 64)
        } else {
            val as u128
        };
        self.v[rd] = result;
        Ok(CpuExit::Continue)
    }



    /// Execute Advanced SIMD "vector x indexed element" instructions: the second
    /// multiplicand is a single broadcast lane of Vm. Covers integer MUL/MLA/MLS,
    /// the saturating doubling family, the widening L-forms, and FP FMUL/FMLA/
    /// FMLS/FMULX. FMLAL and FCMLA indexed forms are dispatched before this
    /// generic handler because they overlap the indexed-element opcode space.
    pub(crate) fn exec_simd_indexed(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        if (insn >> 31) & 1 != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let q = (insn >> 30) & 1;
        let u = (insn >> 29) & 1;
        let size = (insn >> 22) & 0x3;
        let l = (insn >> 21) & 1;
        let m = (insn >> 20) & 1;
        let opcode = (insn >> 12) & 0xF;
        let h = (insn >> 11) & 1;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;
        let scalar = ((insn >> 24) & 0x1F) == 0b11111;

        // Element size, second-source register and broadcast lane index.
        // size==00 is the half-precision FP form (FMUL/FMLA/FMLS/FMULX by
        // element); it shares the H:L:M index and 4-bit Vm of the integer H form.
        let (bits, vm_reg, index): (u32, usize, usize) = match size {
            0b00 | 0b01 => (
                16,
                ((insn >> 16) & 0xF) as usize,
                ((h << 2) | (l << 1) | m) as usize,
            ),
            0b10 => (
                32,
                ((m << 4) | ((insn >> 16) & 0xF)) as usize,
                ((h << 1) | l) as usize,
            ),
            0b11 => (64, ((m << 4) | ((insn >> 16) & 0xF)) as usize, h as usize),
            _ => return Err(ArmError::UndefinedInstruction(insn)),
        };
        let esize = (bits / 8) as usize;
        let emask = elem_mask(bits);
        let vm_elem = ((self.v[vm_reg] >> (index * bits as usize)) & (emask as u128)) as u64;

        // ---- Floating-point indexed: FMLA/FMLS/FMUL/FMULX ----
        let fp_kind = match (u, opcode) {
            (0, 0b0001) => Some(FpKind::Mla),
            (0, 0b0101) => Some(FpKind::Mls),
            (0, 0b1001) => Some(FpKind::Mul),
            (1, 0b1001) => Some(FpKind::Mulx),
            _ => None,
        };
        if let Some(kind) = fp_kind {
            if size == 0b01 {
                // Half precision uses size==00; size==01 is unallocated for FP.
                return Err(ArmError::UndefinedInstruction(insn));
            }
            if bits == 64 && l == 1 {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            if bits == 64 && q == 0 && !scalar {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            let datasize = if scalar {
                esize
            } else if q == 1 {
                16
            } else {
                8
            };
            let elements = datasize / esize;
            let vn = self.v[rn].to_le_bytes();
            let vd_old = self.v[rd].to_le_bytes();
            let mut dst = [0u8; 16];
            for e in 0..elements {
                let off = e * esize;
                let a = read_elem(&vn, off, esize);
                let d = read_elem(&vd_old, off, esize);
                let (r, status) = if bits == 16 {
                    let an = a as u16;
                    let bn = vm_elem as u16;
                    let dn = d as u16;
                    let raw_r = match kind {
                        FpKind::Mul => sve_fp16_binop_with_fpcr(FpKind::Mul, an, bn, self.fpcr),
                        FpKind::Mulx => sve_fp16_binop_with_fpcr(FpKind::Mulx, an, bn, self.fpcr),
                        FpKind::Mla => {
                            fp_muladd_bits_with_fpcr(dn as u64, an as u64, bn as u64, 16, self.fpcr)
                                as u16
                        }
                        FpKind::Mls => fp_muladd_bits_with_fpcr(
                            dn as u64,
                            fp_neg_bits_with_fpcr(an as u64, 16, self.fpcr),
                            bn as u64,
                            16,
                            self.fpcr,
                        ) as u16,
                        _ => return Err(ArmError::UndefinedInstruction(insn)),
                    };
                    let status = match kind {
                        FpKind::Mul => fp_status_binop_with_fpcr(
                            esize,
                            FpKind::Mul,
                            a,
                            vm_elem,
                            raw_r as u64,
                            self.fpcr,
                        ),
                        FpKind::Mulx => {
                            fp_status_mulx_with_fpcr(esize, a, vm_elem, raw_r as u64, self.fpcr)
                        }
                        FpKind::Mla => {
                            fp_status_fma_with_fpcr(esize, d, a, vm_elem, raw_r as u64, self.fpcr)
                        }
                        FpKind::Mls => fp_status_fma_with_fpcr(
                            esize,
                            d,
                            fp_neg_bits_with_fpcr(a, bits, self.fpcr),
                            vm_elem,
                            raw_r as u64,
                            self.fpcr,
                        ),
                        _ => 0,
                    };
                    let (r, status) = fp16_flush_output_status_with_fpcr(raw_r, status, self.fpcr);
                    (r as u64, status)
                } else if bits == 32 {
                    let r = fp_three_same_f32_with_fpcr(
                        kind,
                        a as u32,
                        vm_elem as u32,
                        d as u32,
                        self.fpcr,
                    ) as u64;
                    let status = match kind {
                        FpKind::Mul => {
                            fp_status_binop_with_fpcr(esize, FpKind::Mul, a, vm_elem, r, self.fpcr)
                        }
                        FpKind::Mulx => fp_status_mulx_with_fpcr(esize, a, vm_elem, r, self.fpcr),
                        FpKind::Mla => fp_status_fma_with_fpcr(esize, d, a, vm_elem, r, self.fpcr),
                        FpKind::Mls => fp_status_fma_with_fpcr(
                            esize,
                            d,
                            fp_neg_bits_with_fpcr(a, bits, self.fpcr),
                            vm_elem,
                            r,
                            self.fpcr,
                        ),
                        _ => 0,
                    };
                    (r, status)
                } else {
                    let r = fp_three_same_f64_with_fpcr(kind, a, vm_elem, d, self.fpcr);
                    let status = match kind {
                        FpKind::Mul => {
                            fp_status_binop_with_fpcr(esize, FpKind::Mul, a, vm_elem, r, self.fpcr)
                        }
                        FpKind::Mulx => fp_status_mulx_with_fpcr(esize, a, vm_elem, r, self.fpcr),
                        FpKind::Mla => fp_status_fma_with_fpcr(esize, d, a, vm_elem, r, self.fpcr),
                        FpKind::Mls => fp_status_fma_with_fpcr(
                            esize,
                            d,
                            fp_neg_bits_with_fpcr(a, bits, self.fpcr),
                            vm_elem,
                            r,
                            self.fpcr,
                        ),
                        _ => 0,
                    };
                    (r, status)
                };
                self.fpsr |= status;
                write_elem(&mut dst, off, esize, r);
            }
            self.v[rd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // SDOT/UDOT by element (opcode 1110): the index selects a 32-bit
        // (4-byte) group of Vm that is reused for every output lane.
        if opcode == 0b1110 {
            // SDOT/UDOT by element are vector-only; the scalar indexed-element
            // form (bits[28:24]==11111) is unallocated and must trap.
            if scalar || size != 0b10 {
                return Ok(CpuExit::Undefined(insn));
            }
            let signed = u == 0;
            let lanes = if q == 1 { 4 } else { 2 };
            let op1 = self.v[rn];
            let vm_bytes = vm_elem as u32; // the selected 4-byte group
            let mut result = self.v[rd];
            for e in 0..lanes {
                let mut res: i64 = 0;
                for i in 0..4 {
                    let b1 = (op1 >> ((4 * e + i) * 8)) as u8;
                    let b2 = (vm_bytes >> (i * 8)) as u8;
                    res += if signed {
                        (b1 as i8 as i64) * (b2 as i8 as i64)
                    } else {
                        (b1 as i64) * (b2 as i64)
                    };
                }
                let lane = (result >> (e * 32)) as u32;
                let updated = (lane as i64).wrapping_add(res) as u32;
                result =
                    (result & !(0xFFFF_FFFFu128 << (e * 32))) | ((updated as u128) << (e * 32));
            }
            if q == 0 {
                result &= 0xFFFF_FFFF_FFFF_FFFF;
            }
            self.v[rd] = result;
            return Ok(CpuExit::Continue);
        }

        // Integer indexed ops use 16- or 32-bit elements only.
        if size != 0b01 && size != 0b10 {
            return Err(ArmError::UndefinedInstruction(insn));
        }

        // ---- Widening L-forms: SMULL/UMULL/SMLAL/UMLAL/SMLSL/UMLSL/SQDMULL/SQDMLAL/SQDMLSL ----
        let widening = matches!(opcode, 0b0010 | 0b0011 | 0b0110 | 0b0111 | 0b1010 | 0b1011);
        if widening {
            let dst_bits = 2 * bits;
            // Scalar by-element (SQDMLAL <Dd>,<Sn>,<Vm>.s[i] etc.) produces one
            // widened element in lane 0, zeroing the rest; the vector "2" form
            // reads the upper half of Vn.
            let elements = if scalar { 1 } else { 64 / bits as usize };
            let part = if scalar { 0 } else { q as usize };
            let signed = u == 0;
            let sat_double = matches!(opcode, 0b0011 | 0b0111 | 0b1011);
            let accum = matches!(opcode, 0b0010 | 0b0110 | 0b0011 | 0b0111);
            let subtract = matches!(opcode, 0b0110 | 0b0111);
            if scalar && !sat_double {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            // SQDMULL/SQDMLAL/SQDMLSL are signed-only.
            if sat_double && u == 1 {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            let vn = self.v[rn].to_le_bytes();
            let vd_old = self.v[rd];
            let dmin = -(1i128 << (dst_bits - 1));
            let dmax = (1i128 << (dst_bits - 1)) - 1;
            let mut result: u128 = 0;
            for e in 0..elements {
                let off = part * 8 + e * esize;
                let a = read_elem(&vn, off, esize);
                let (av, bv): (i128, i128) = if signed {
                    (sext_elem(a, bits), sext_elem(vm_elem, bits))
                } else {
                    (uext_elem(a, bits) as i128, uext_elem(vm_elem, bits) as i128)
                };
                let mut prod = av * bv;
                if sat_double {
                    let raw_prod = prod * 2;
                    if raw_prod < dmin || raw_prod > dmax {
                        self.fpsr |= FPSR_QC;
                    }
                    prod = raw_prod.clamp(dmin, dmax);
                }
                let elem: u128 = if accum {
                    let d = ((vd_old >> (e * dst_bits as usize)) & elem_mask_u128(dst_bits)) as u64;
                    if sat_double {
                        let acc = sext_elem(d, dst_bits) + if subtract { -prod } else { prod };
                        let (r, saturated) = sat_signed_q(acc, dst_bits);
                        if saturated {
                            self.fpsr |= FPSR_QC;
                        }
                        r as u128
                    } else {
                        let r = if subtract {
                            (d as i128).wrapping_sub(prod)
                        } else {
                            (d as i128).wrapping_add(prod)
                        };
                        (r as u128) & elem_mask_u128(dst_bits)
                    }
                } else {
                    (prod as u128) & elem_mask_u128(dst_bits)
                };
                result |= elem << (e * dst_bits as usize);
            }
            self.v[rd] = result;
            return Ok(CpuExit::Continue);
        }

        // ---- Same-size: MUL/MLA/MLS and the saturating doubling-high family ----
        if bits == 64 && q == 0 && !scalar {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        if scalar
            && !matches!(
                (u, opcode),
                (0, 0b1100) | (0, 0b1101) | (1, 0b1101) | (1, 0b1111)
            )
        {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let datasize = if scalar {
            esize
        } else if q == 1 {
            16
        } else {
            8
        };
        let elements = datasize / esize;
        let vn = self.v[rn].to_le_bytes();
        let vd_old = self.v[rd].to_le_bytes();
        let mut dst = [0u8; 16];
        for e in 0..elements {
            let off = e * esize;
            let a = read_elem(&vn, off, esize);
            let d = read_elem(&vd_old, off, esize);
            let r = match (u, opcode) {
                (0, 0b1000) => {
                    ((uext_elem(a, bits) * uext_elem(vm_elem, bits)) as u64) & emask // MUL
                }
                (1, 0b0000) => {
                    let p = (uext_elem(a, bits) * uext_elem(vm_elem, bits)) as u64;
                    d.wrapping_add(p) & emask // MLA
                }
                (1, 0b0100) => {
                    let p = (uext_elem(a, bits) * uext_elem(vm_elem, bits)) as u64;
                    d.wrapping_sub(p) & emask // MLS
                }
                (0, 0b1100) => {
                    let min = -(1i128 << (bits - 1));
                    if sext_elem(a, bits) == min && sext_elem(vm_elem, bits) == min {
                        self.fpsr |= FPSR_QC;
                    }
                    adv_simd_three_same_int(0, 0b10110, bits, a, vm_elem, 0).0 // SQDMULH
                }
                (0, 0b1101) => {
                    let min = -(1i128 << (bits - 1));
                    if sext_elem(a, bits) == min && sext_elem(vm_elem, bits) == min {
                        self.fpsr |= FPSR_QC;
                    }
                    adv_simd_three_same_int(1, 0b10110, bits, a, vm_elem, 0).0 // SQRDMULH
                }
                (1, 0b1101) => {
                    // SQRDMLAH: accumulate the (unsaturated) rounded doubling
                    // product, then saturate once.
                    let prod = sext_elem(a, bits) * sext_elem(vm_elem, bits);
                    let rounded = (prod * 2 + (1i128 << (bits - 1))) >> bits;
                    let (r, saturated) = sat_signed_q(sext_elem(d, bits) + rounded, bits);
                    if saturated {
                        self.fpsr |= FPSR_QC;
                    }
                    r
                }
                (1, 0b1111) => {
                    // SQRDMLSH
                    let prod = sext_elem(a, bits) * sext_elem(vm_elem, bits);
                    let rounded = (-prod * 2 + (1i128 << (bits - 1))) >> bits;
                    let (r, saturated) = sat_signed_q(sext_elem(d, bits) + rounded, bits);
                    if saturated {
                        self.fpsr |= FPSR_QC;
                    }
                    r
                }
                _ => return Err(ArmError::UndefinedInstruction(insn)),
            };
            write_elem(&mut dst, off, esize, r);
        }
        self.v[rd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }



    /// Execute SIMD table lookup (TBL, TBX).
    pub(crate) fn exec_simd_table(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let q = (insn >> 30) & 1;
        let rm = ((insn >> 16) & 0x1F) as usize;
        let len = ((insn >> 13) & 0x3) as usize;
        let op = (insn >> 12) & 1;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;

        let datasize = if q == 1 { 16 } else { 8 };

        // Build table from consecutive registers
        let mut table = [0u8; 64];
        for i in 0..=len {
            let reg = (rn + i) % 32;
            let bytes = self.v[reg].to_le_bytes();
            table[i * 16..(i + 1) * 16].copy_from_slice(&bytes);
        }
        let table_size = (len + 1) * 16;

        let indices = self.v[rm].to_le_bytes();
        let mut dst = if op == 1 {
            // TBX: keep original values for out-of-range indices
            self.v[rd].to_le_bytes()
        } else {
            [0u8; 16]
        };

        for i in 0..datasize {
            let idx = indices[i] as usize;
            if idx < table_size {
                dst[i] = table[idx];
            }
            // For TBL (op=0), out-of-range stays 0
            // For TBX (op=1), out-of-range keeps original
        }
        // Q==0 zeroes the upper 64 bits (TBX kept Vd's upper half otherwise).
        if q == 0 {
            for b in 8..16 {
                dst[b] = 0;
            }
        }

        self.v[rd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }



    /// Execute SIMD three-same register instructions.
    pub(crate) fn exec_simd_three_same(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        if (insn >> 31) != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let q = (insn >> 30) & 1;
        let u = (insn >> 29) & 1;
        let size = (insn >> 22) & 0x3;
        let opcode = (insn >> 11) & 0x1F;
        let rm = ((insn >> 16) & 0x1F) as usize;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;
        let scalar = ((insn >> 24) & 0x1F) == 0b11110;

        // Floating-point three-same opcodes (0b11000..=0b11111).
        if opcode >= 0b11000 {
            return self.exec_simd_three_same_fp(insn, scalar);
        }

        // Logical operations (opcode 0b00011) act on the whole register; the
        // `size` field selects the operation rather than the element size.
        if opcode == 0b00011 {
            let n1 = self.v[rn];
            let n2 = self.v[rm];
            let dd = self.v[rd];
            let result = match (u, size) {
                (0, 0b00) => n1 & n2,                // AND
                (0, 0b01) => n1 & !n2,               // BIC
                (0, 0b10) => n1 | n2,                // ORR
                (0, 0b11) => n1 | !n2,               // ORN
                (1, 0b00) => n1 ^ n2,                // EOR
                (1, 0b01) => n2 ^ (dd & (n2 ^ n1)),  // BSL
                (1, 0b10) => dd ^ ((dd ^ n1) & n2),  // BIT
                (1, 0b11) => dd ^ ((dd ^ n1) & !n2), // BIF
                _ => unreachable!(),
            };
            let mask = if q == 1 {
                u128::MAX
            } else {
                0xFFFF_FFFF_FFFF_FFFF
            };
            self.v[rd] = result & mask;
            return Ok(CpuExit::Continue);
        }

        let bits = 8u32 << size; // 8, 16, 32 or 64
        let esize = (bits / 8) as usize;

        if scalar {
            // The scalar form allows only a subset of opcodes. The non-saturating
            // arithmetic/compare/shift ops (ADD/SUB, CMGT/CMGE/CMHI/CMHS,
            // CMTST/CMEQ, SSHL/USHL, SRSHL/URSHL) are defined for 64-bit (D)
            // elements only; the saturating ops allow all sizes; everything else
            // is unallocated as a scalar.
            let scalar_d_only = matches!(
                opcode,
                0b00110 | 0b00111 | 0b01000 | 0b01010 | 0b10000 | 0b10001
            );
            let scalar_any_size = matches!(opcode, 0b00001 | 0b00101 | 0b01001 | 0b01011);
            let scalar_sqdmulh = opcode == 0b10110;
            if scalar_d_only {
                if size != 0b11 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
            } else if !scalar_any_size && !scalar_sqdmulh {
                return Err(ArmError::UndefinedInstruction(insn));
            }
        }

        // Reject UNDEFINED (opcode, size) combinations. These integer opcodes
        // have no 64-bit (size==0b11) vector form.
        let no_64 = matches!(
            opcode,
            0b00000
                | 0b00010
                | 0b00100
                | 0b01100
                | 0b01101
                | 0b01110
                | 0b01111
                | 0b10010
                | 0b10100
                | 0b10101
        );
        if size == 0b11 && no_64 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        // 64-bit elements need the 2D (Q==1) arrangement; "1D" is not a valid
        // vector form. (Scalar uses a single element and is handled separately.)
        if size == 0b11 && q == 0 && !scalar {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        match opcode {
            0b10011 => {
                // MUL: no 64-bit form; PMUL: 8-bit only.
                if u == 0 && size == 0b11 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                if u == 1 && size != 0b00 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
            }
            0b10110 => {
                // SQDMULH/SQRDMULH: 16- or 32-bit only.
                if size == 0b00 || size == 0b11 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
            }
            0b10111 => {
                // ADDP is U==0 only; U==1 at this opcode is unallocated.
                if u == 1 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
            }
            _ => {}
        }

        let datasize = if scalar {
            esize
        } else if q == 1 {
            16
        } else {
            8
        };
        let elements = datasize / esize;

        // SMAXP/SMINP/ADDP take their operands pairwise from the Vn:Vm concat.
        let pairwise = matches!(opcode, 0b10100 | 0b10101 | 0b10111);

        let src1 = self.v[rn].to_le_bytes();
        let src2 = self.v[rm].to_le_bytes();
        let old_d = self.v[rd].to_le_bytes();
        let mut dst = [0u8; 16];

        let mut concat = [0u8; 32];
        if pairwise {
            concat[..datasize].copy_from_slice(&src1[..datasize]);
            concat[datasize..datasize * 2].copy_from_slice(&src2[..datasize]);
        }

        for e in 0..elements {
            let off = e * esize;
            let (a, b) = if pairwise {
                (
                    read_elem(&concat, (2 * e) * esize, esize),
                    read_elem(&concat, (2 * e + 1) * esize, esize),
                )
            } else {
                (read_elem(&src1, off, esize), read_elem(&src2, off, esize))
            };
            let d = read_elem(&old_d, off, esize);
            let (res, saturated) = adv_simd_three_same_int(u, opcode, bits, a, b, d);
            if saturated {
                self.fpsr |= FPSR_QC;
            }
            write_elem(&mut dst, off, esize, res);
        }

        self.v[rd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }



    /// Execute SIMD two-register miscellaneous instructions.
    pub(crate) fn exec_simd_two_reg(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        if (insn >> 31) != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let q = (insn >> 30) & 1;
        let u = (insn >> 29) & 1;
        let size = (insn >> 22) & 0x3;
        let opcode = (insn >> 12) & 0x1F;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;
        // Scalar AdvSIMD two-reg-misc (top byte 0x5E/0x7E) operates on a single
        // element of the low lane, zeroing the rest of the destination.
        let scalar = (insn >> 24) & 0x1F == 0b11110;

        let esize = 1usize << size;
        let datasize = if q == 1 { 16 } else { 8 };
        let elements = if scalar { 1 } else { datasize / esize };

        // ---- REV64 / REV32 / REV16: reverse elements within a container. ----
        if (u == 0 && opcode == 0b00000)
            || (u == 1 && opcode == 0b00000)
            || (u == 0 && opcode == 0b00001)
        {
            let container = if opcode == 0b00001 {
                16usize // REV16
            } else if u == 1 {
                32 // REV32
            } else {
                64 // REV64
            };
            let cbytes = container / 8;
            if esize >= cbytes || (8 << size) > container {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            let epc = cbytes / esize; // elements per container
            let src = self.v[rn].to_le_bytes();
            let mut dst = [0u8; 16];
            for c in 0..(datasize / cbytes) {
                for i in 0..epc {
                    let from = (c * epc + (epc - 1 - i)) * esize;
                    let to = (c * epc + i) * esize;
                    dst[to..to + esize].copy_from_slice(&src[from..from + esize]);
                }
            }
            self.v[rd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // ---- NOT (size==00) / RBIT (size==01): per-byte, U==1 opcode 0b00101. ----
        if u == 1 && opcode == 0b00101 {
            if size > 0b01 {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            let src = self.v[rn].to_le_bytes();
            let mut dst = [0u8; 16];
            for b in 0..datasize {
                dst[b] = if size == 0b00 {
                    !src[b]
                } else {
                    src[b].reverse_bits()
                };
            }
            self.v[rd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        // ---- Same-size integer ops (CLS/CLZ/CNT/ABS/NEG/SQABS/SQNEG/CMxx#0/
        //      SUQADD/USQADD). ----
        {
            let bits = (8u32) << size;
            // Probe whether this (u, opcode) is one we handle here.
            if adv_simd_two_reg_int(u, opcode, bits, 0, 0).is_some() {
                // CNT is byte-only; NOT/RBIT handled above.
                if opcode == 0b00101 && size != 0b00 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                if scalar && matches!(opcode, 0b01000 | 0b01001 | 0b01010) && size != 0b11 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                if scalar && opcode == 0b01011 && size != 0b11 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                // CLS/CLZ have no 64-bit element form.
                if opcode == 0b00100 && size == 0b11 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                // 64-bit elements need the 2D (Q==1) arrangement.
                if size == 0b11 && q == 0 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                let accumulate = opcode == 0b00011; // SUQADD / USQADD read Vd
                let src = self.v[rn].to_le_bytes();
                let old = self.v[rd].to_le_bytes();
                let mut dst = [0u8; 16];
                for e in 0..elements {
                    let off = e * esize;
                    let a = read_elem(&src, off, esize);
                    let d = if accumulate {
                        read_elem(&old, off, esize)
                    } else {
                        0
                    };
                    let (r, saturated) = adv_simd_two_reg_int(u, opcode, bits, a, d).unwrap();
                    if saturated {
                        self.fpsr |= FPSR_QC;
                    }
                    write_elem(&mut dst, off, esize, r);
                }
                self.v[rd] = u128::from_le_bytes(dst);
                return Ok(CpuExit::Continue);
            }
        }

        // ---- SADDLP/UADDLP (00010), SADALP/UADALP (00110): pairwise widening. ----
        if opcode == 0b00010 || opcode == 0b00110 {
            if scalar || size == 0b11 {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            let bits = 8u32 << size;
            let dbits = 2 * bits;
            let src_elems = datasize / esize;
            let out_elems = src_elems / 2;
            let signed = u == 0;
            let accumulate = opcode == 0b00110;
            let src = self.v[rn].to_le_bytes();
            let vd = self.v[rd];
            let mut result = 0u128;
            for o in 0..out_elems {
                let a = read_elem(&src, (2 * o) * esize, esize);
                let b = read_elem(&src, (2 * o + 1) * esize, esize);
                let sum: i128 = if signed {
                    sext_elem(a, bits) + sext_elem(b, bits)
                } else {
                    uext_elem(a, bits) as i128 + uext_elem(b, bits) as i128
                };
                let mut val = (sum as u128) & elem_mask_u128(dbits);
                if accumulate {
                    let d = (vd >> (o * dbits as usize)) & elem_mask_u128(dbits);
                    val = val.wrapping_add(d) & elem_mask_u128(dbits);
                }
                result |= val << (o * dbits as usize);
            }
            self.v[rd] = result;
            return Ok(CpuExit::Continue);
        }

        // ---- XTN/SQXTUN (10010), SQXTN/UQXTN (10100): narrowing. ----
        if opcode == 0b10010 || opcode == 0b10100 {
            if size == 0b11 {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            if scalar && u == 0 && opcode == 0b10010 {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            let bits = 8u32 << size;
            let dbits = 2 * bits;
            // Scalar narrowing (SQXTN/UQXTN/SQXTUN <Bd>,<Hn> etc.) writes a single
            // element into lane 0 and zeroes the rest; the vector form fills the
            // low (part=0) or high (part=1, the "2" variant) 64-bit half.
            let out_elems = if scalar { 1 } else { 8 / esize };
            let part = if scalar { 0 } else { q as usize };
            let vn = self.v[rn];
            let mut packed = 0u64;
            for e in 0..out_elems {
                let s = ((vn >> (e * dbits as usize)) & elem_mask_u128(dbits)) as u64;
                let (r, saturated): (u64, bool) = match (u, opcode) {
                    (0, 0b10010) => (s & elem_mask(bits), false), // XTN
                    (1, 0b10010) => sat_unsigned_q(sext_elem(s, dbits), bits), // SQXTUN
                    (0, 0b10100) => sat_signed_q(sext_elem(s, dbits), bits), // SQXTN
                    _ => sat_unsigned_q(uext_elem(s, dbits) as i128, bits), // UQXTN
                };
                if saturated {
                    self.fpsr |= FPSR_QC;
                }
                packed |= (r & elem_mask(bits)) << (e * bits as usize);
            }
            let mut bytes = self.v[rd].to_le_bytes();
            bytes[part * 8..part * 8 + 8].copy_from_slice(&packed.to_le_bytes());
            if part == 0 {
                bytes[8..16].copy_from_slice(&[0u8; 8]);
            }
            self.v[rd] = u128::from_le_bytes(bytes);
            return Ok(CpuExit::Continue);
        }

        // ---- SHLL/SHLL2 (U==1, 10011): shift left long by the element size. ----
        if u == 1 && opcode == 0b10011 {
            if scalar || size == 0b11 {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            let bits = 8u32 << size;
            let dbits = 2 * bits;
            let part = q as usize;
            let src = self.v[rn].to_le_bytes();
            let mut result = 0u128;
            for e in 0..(8 / esize) {
                let a = read_elem(&src, part * 8 + e * esize, esize);
                let val = (uext_elem(a, bits) << bits) & elem_mask_u128(dbits);
                result |= val << (e * dbits as usize);
            }
            self.v[rd] = result;
            return Ok(CpuExit::Continue);
        }

        // ---- Floating-point two-register-misc (deterministic subset). The
        //      estimate ops (FRECPE/FRSQRTE/URECPE/URSQRTE) and FP narrow/long
        //      fall through to the legacy handling below. ----
        if let Some(r) = self.exec_simd_two_reg_fp(insn) {
            return r;
        }

        Err(ArmError::UndefinedInstruction(insn))
    }



    // FP helper functions
    pub(crate) fn fp_maxnm_f32(&self, a: f32, b: f32) -> f32 {
        if a.is_nan() {
            b
        } else if b.is_nan() {
            a
        } else {
            a.max(b)
        }
    }



    pub(crate) fn fp_minnm_f32(&self, a: f32, b: f32) -> f32 {
        if a.is_nan() {
            b
        } else if b.is_nan() {
            a
        } else {
            a.min(b)
        }
    }



    pub(crate) fn fp_nmul_f32(&self, a: f32, b: f32) -> f32 {
        -(a * b)
    }



    pub(crate) fn fp_maxnm_f64(&self, a: f64, b: f64) -> f64 {
        if a.is_nan() {
            b
        } else if b.is_nan() {
            a
        } else {
            a.max(b)
        }
    }



    pub(crate) fn fp_minnm_f64(&self, a: f64, b: f64) -> f64 {
        if a.is_nan() {
            b
        } else if b.is_nan() {
            a
        } else {
            a.min(b)
        }
    }



    pub(crate) fn fp_nmul_f64(&self, a: f64, b: f64) -> f64 {
        -(a * b)
    }
}
