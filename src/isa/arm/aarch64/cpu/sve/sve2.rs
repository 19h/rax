//! sve2.rs

use crate::isa::arm::aarch64::cpu::*;
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
    /// Execute SVE integer dot product (SDOT/UDOT/USDOT/SUDOT), vector and
    /// indexed. Each destination element (S from 8-bit sources, D from 16-bit)
    /// accumulates a 4-element dot product; the indexed form broadcasts the
    /// index-th 4-element group of Zm across the segment. Sign treatment is
    /// per-operand; no saturation.
    pub(crate) fn exec_sve_dot(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let indexed = (insn >> 21) & 1 == 1;
        let d_esize = if (insn >> 22) & 1 == 0 { 4usize } else { 8 };
        let s_esize = d_esize / 4;
        let s_bits = (s_esize * 8) as u32;
        let d_bits = (d_esize * 8) as u32;
        let mask = elem_mask(d_bits);
        let (n_signed, m_signed) = if indexed {
            match (insn >> 10) & 0x3F {
                0b000000 => (true, true),   // SDOT
                0b000001 => (false, false), // UDOT
                0b000110 => (false, true),  // USDOT (Zn unsigned, Zm signed)
                0b000111 => (true, false),  // SUDOT (Zn signed, Zm unsigned)
                _ => return Ok(CpuExit::Undefined(insn)),
            }
        } else if (insn >> 10) & 0x3F == 0b011110 {
            (false, true) // USDOT vector
        } else {
            let u = (insn >> 10) & 1 == 1;
            (!u, !u) // SDOT(u=0) / UDOT(u=1)
        };
        let zd = (insn & 0x1F) as usize;
        let zn = ((insn >> 5) & 0x1F) as usize;
        let (zm, index) = if indexed {
            if d_esize == 4 {
                (((insn >> 16) & 0x7) as usize, ((insn >> 19) & 0x3) as usize)
            } else {
                (((insn >> 16) & 0xF) as usize, ((insn >> 20) & 1) as usize)
            }
        } else {
            (((insn >> 16) & 0x1F) as usize, 0)
        };
        let n = self.v[zn].to_le_bytes();
        let m = self.v[zm].to_le_bytes();
        let a = self.v[zd].to_le_bytes();
        let ext = |b: &[u8; 16], off: usize, s: bool| -> i128 {
            if s {
                sext_elem(read_elem(b, off, s_esize), s_bits)
            } else {
                uext_elem(read_elem(b, off, s_esize), s_bits) as i128
            }
        };
        let mut dst = [0u8; 16];
        for i in 0..(16 / d_esize) {
            let mut acc = sext_elem(read_elem(&a, i * d_esize, d_esize), d_bits);
            for k in 0..4 {
                let n_off = i * d_esize + k * s_esize;
                let m_off = if indexed {
                    (index * 4 + k) * s_esize
                } else {
                    n_off
                };
                acc += ext(&n, n_off, n_signed) * ext(&m, m_off, m_signed);
            }
            write_elem(&mut dst, i * d_esize, d_esize, acc as u64 & mask);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }

    /// Execute the SVE2 predicated integer ALU group at 0x44 bits[15:14]==10:
    /// saturating/rounding shifts by vector (SRSHL/URSHL/SQSHL/UQSHL/SQRSHL/
    /// UQRSHL and their reversed forms), halving add/sub (SHADD/UHADD/SHSUB/
    /// UHSUB/SRHADD/URHADD/SHSUBR/UHSUBR), saturating add/sub (SQADD/UQADD/
    /// SQSUB/UQSUB/SUQADD/USQADD/SQSUBR/UQSUBR) at bits[15:13]==100, and the
    /// unary SQABS/SQNEG at bits[15:13]==101 bits[21:19]==001. All merge under
    /// Pg. The op is keyed on bits[21:16]; reversed forms swap the operands.
    pub(crate) fn exec_sve2_pred_alu(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let esize = 1usize << ((insn >> 22) & 0x3);
        let bits = (esize * 8) as u32;
        let mask = elem_mask(bits);
        let pg = ((insn >> 10) & 0x7) as usize;
        let rd = (insn & 0x1F) as usize;
        let rfield = ((insn >> 5) & 0x1F) as usize;
        let pred = self.sve_p[pg];
        let dst_prior = self.v[rd].to_le_bytes();
        let mut dst = dst_prior;
        let elements = 16 / esize;

        if (insn >> 13) & 0x7 == 0b101 {
            // Unary, source = rfield, dest = rd, merging. bits[21:19]==001 ->
            // SQABS/SQNEG; bits[21:19]==000 -> URECPE/URSQRTE (S-only unsigned
            // reciprocal estimates).
            let opc6 = (insn >> 16) & 0x3F;
            let src = self.v[rfield].to_le_bytes();
            let recip = opc6 >> 3 == 0b000;
            if !matches!(opc6, 0b000000 | 0b000001 | 0b001000 | 0b001001) {
                return Ok(CpuExit::Undefined(insn));
            }
            if recip && esize != 4 {
                return Ok(CpuExit::Undefined(insn));
            }
            let sel = (insn >> 16) & 1 == 1; // SQNEG / URSQRTE
            for e in 0..elements {
                let off = e * esize;
                if (pred >> off) & 1 == 0 {
                    continue;
                }
                let r = if recip {
                    let a = read_elem(&src, off, 4) as u32;
                    (if sel {
                        unsigned_rsqrt_estimate(a)
                    } else {
                        unsigned_recip_estimate(a)
                    }) as u64
                } else {
                    let n = sext_elem(read_elem(&src, off, esize), bits);
                    if sel {
                        sat_signed(-n, bits)
                    } else {
                        sat_signed(n.abs(), bits)
                    }
                };
                write_elem(&mut dst, off, esize, r);
            }
            self.v[rd] = u128::from_le_bytes(dst);
            return Ok(CpuExit::Continue);
        }

        let opc6 = (insn >> 16) & 0x3F;
        if !matches!(
            opc6,
            0b000010
                | 0b000011
                | 0b000110
                | 0b000111
                | 0b001000..=0b001111
                | 0b010000..=0b011111
        ) {
            return Ok(CpuExit::Undefined(insn));
        }
        let reversed = matches!(
            opc6,
            0b000110
                | 0b000111
                | 0b001100
                | 0b001101
                | 0b001110
                | 0b001111
                | 0b010110
                | 0b010111
                | 0b011110
                | 0b011111
        );
        let field = self.v[rfield].to_le_bytes();
        let do_shift = |val: u64, sh: i64, signed: bool, round: bool, sat: bool| -> u64 {
            if bits == 64 {
                if signed {
                    sqrshl_d(val as i64, sh, round, sat) as u64
                } else {
                    uqrshl_d(val, sh, round, sat)
                }
            } else if signed {
                sqrshl_bhs(sext_elem(val, bits) as i32, sh as i32, bits, round, sat) as u64 & mask
            } else {
                uqrshl_bhs((val & mask) as u32, sh as i32, bits, round, sat) as u64 & mask
            }
        };
        for e in 0..elements {
            let off = e * esize;
            if (pred >> off) & 1 == 0 {
                continue;
            }
            let rd_v = read_elem(&dst_prior, off, esize);
            let fv = read_elem(&field, off, esize);
            let (a, b) = if reversed { (fv, rd_v) } else { (rd_v, fv) };
            let (sa, sb) = (sext_elem(a, bits), sext_elem(b, bits));
            let (ua, ub) = (uext_elem(a, bits) as i128, uext_elem(b, bits) as i128);
            let r: u64 = match opc6 {
                0b000010 | 0b000110 => do_shift(a, sb as i64, true, true, false), // SRSHL(R)
                0b000011 | 0b000111 => do_shift(a, sb as i64, false, true, false), // URSHL(R)
                0b001000 | 0b001100 => do_shift(a, sb as i64, true, false, true), // SQSHL(R)
                0b001001 | 0b001101 => do_shift(a, sb as i64, false, false, true), // UQSHL(R)
                0b001010 | 0b001110 => do_shift(a, sb as i64, true, true, true),  // SQRSHL(R)
                0b001011 | 0b001111 => do_shift(a, sb as i64, false, true, true), // UQRSHL(R)
                0b010000 => ((sa + sb) >> 1) as u64 & mask,                       // SHADD
                0b010001 => ((ua + ub) >> 1) as u64 & mask,                       // UHADD
                0b010010 | 0b010110 => ((sa - sb) >> 1) as u64 & mask,            // SHSUB(R)
                0b010011 | 0b010111 => ((ua - ub) >> 1) as u64 & mask,            // UHSUB(R)
                0b010100 => ((sa + sb + 1) >> 1) as u64 & mask,                   // SRHADD
                0b010101 => ((ua + ub + 1) >> 1) as u64 & mask,                   // URHADD
                0b011000 => sat_signed(sa + sb, bits),                            // SQADD
                0b011001 => sat_unsigned(ua + ub, bits),                          // UQADD
                0b011010 | 0b011110 => sat_signed(sa - sb, bits),                 // SQSUB(R)
                0b011011 | 0b011111 => sat_unsigned(ua - ub, bits),               // UQSUB(R)
                0b011100 => sat_signed(sa + ub, bits),                            // SUQADD
                0b011101 => sat_unsigned(ua + sb, bits),                          // USQADD
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            write_elem(&mut dst, off, esize, r);
        }
        self.v[rd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }

    /// Execute SVE2 widening multiply-add long by an indexed element. The
    /// narrow source elements (half the destination width) are sign- or
    /// zero-extended; `Zm[index]` is the shared broadcast factor and bit10 (T)
    /// selects the odd/even narrow lane of Zn. SQDMULL doubles-and-saturates;
    /// the SQDMLAL/SQDMLSL accumulate saturates a second time.
    pub(crate) fn exec_sve2_mull_indexed(
        &mut self,
        insn: u32,
        zn: usize,
        zd: usize,
    ) -> Result<CpuExit, ArmError> {
        let size = (insn >> 22) & 0x3; // 2=.s (h src), 3=.d (s src)
        if size < 2 {
            return Ok(CpuExit::Undefined(insn));
        }
        let d_esize = 1usize << size;
        let s_esize = d_esize / 2;
        let s_bits = (s_esize * 8) as u32;
        let d_bits = (d_esize * 8) as u32;
        let top = (insn >> 10) & 1 == 1;
        let op = (insn >> 12) & 0xF;
        // Index and Zm packing differ per size: .s uses a 3-bit index
        // (bit20:bit19:bit11) with Zm in z0-z7; .d a 2-bit index (bit20:bit11)
        // with Zm in z0-z15.
        let (index, zm) = if size == 2 {
            let idx = (((insn >> 20) & 1) << 2) | (((insn >> 19) & 1) << 1) | ((insn >> 11) & 1);
            (idx as usize, ((insn >> 16) & 0x7) as usize)
        } else {
            let idx = (((insn >> 20) & 1) << 1) | ((insn >> 11) & 1);
            (idx as usize, ((insn >> 16) & 0xF) as usize)
        };
        let n = self.v[zn].to_le_bytes();
        let m = self.v[zm].to_le_bytes();
        let acc = self.v[zd].to_le_bytes();
        let mut dst = [0u8; 16];
        let mask = elem_mask(d_bits);
        let hi = (1i128 << (d_bits - 1)) - 1;
        let lo = -(1i128 << (d_bits - 1));
        let m_raw = read_elem(&m, index * s_esize, s_esize);
        let elements = 16 / d_esize;
        for e in 0..elements {
            let n_raw = read_elem(&n, (2 * e + top as usize) * s_esize, s_esize);
            let aa = read_elem(&acc, e * d_esize, d_esize);
            let aa_s = sext_elem(aa, d_bits);
            let nm_s = sext_elem(n_raw, s_bits) * sext_elem(m_raw, s_bits);
            let nm_u = (uext_elem(n_raw, s_bits) * uext_elem(m_raw, s_bits)) as i128;
            let sqdmull = (2 * nm_s).clamp(lo, hi); // saturating doubling product
            let r: u64 = match op {
                0b1100 => nm_s as u64 & mask,                           // SMULLB/T
                0b1101 => nm_u as u64 & mask,                           // UMULLB/T
                0b1000 => (aa_s + nm_s) as u64 & mask,                  // SMLALB/T
                0b1001 => (aa_s + nm_u) as u64 & mask,                  // UMLALB/T
                0b1010 => (aa_s - nm_s) as u64 & mask,                  // SMLSLB/T
                0b1011 => (aa_s - nm_u) as u64 & mask,                  // UMLSLB/T
                0b1110 => sqdmull as u64 & mask,                        // SQDMULLB/T
                0b0010 => (aa_s + sqdmull).clamp(lo, hi) as u64 & mask, // SQDMLALB/T
                0b0011 => (aa_s - sqdmull).clamp(lo, hi) as u64 & mask, // SQDMLSLB/T
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            write_elem(&mut dst, e * d_esize, d_esize, r);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }

    /// Execute the SVE2 crypto group (AES round/mix, SM4, SHA3 RAX1). At
    /// VL=128 every operation works on the single 128-bit segment, so it reuses
    /// the NEON primitives directly. AES family: bits[15:11]==11100, sub-decoded
    /// by bits[23:16] and bit10; SM4EKEY/RAX1: bits[15:11]==11110, bit10.
    pub(crate) fn exec_sve2_crypto(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let zd = (insn & 0x1F) as usize;
        let n = ((insn >> 5) & 0x1F) as usize; // bits[9:5] (Zm for AES/SM4E, Zn for *KEY/RAX1)
        let m = ((insn >> 16) & 0x1F) as usize; // bits[20:16] (Zm for *KEY/RAX1)
        let inv = (insn >> 10) & 1 == 1;
        match (insn >> 11) & 0x1F {
            0b11100 => {
                match (insn >> 16) & 0xFF {
                    // AESMC / AESIMC: Zd = (Inv)MixColumns(Zd). bits[9:5] must be 0.
                    0x20 if n == 0 => self.v[zd] = aes_mix_columns(self.v[zd], inv),
                    // AESE / AESD: Zd = (Inv)SubBytes((Inv)ShiftRows(Zd ^ Zm)).
                    0x22 => {
                        let st = self.v[zd] ^ self.v[n];
                        self.v[zd] = aes_sub_bytes(aes_shift_rows(st, inv), inv);
                    }
                    // SM4E: Zd = SM4E(Zd, Zm).
                    0x23 if !inv => self.v[zd] = sm4_rounds(self.v[zd], self.v[n], true),
                    _ => return Ok(CpuExit::Undefined(insn)),
                }
                Ok(CpuExit::Continue)
            }
            0b11110 => {
                if (insn >> 21) & 0x7 != 0b001 {
                    return Ok(CpuExit::Undefined(insn));
                }
                if !inv {
                    // SM4EKEY: Zd = SM4EKEY(Zn, Zm).
                    self.v[zd] = sm4_rounds(self.v[n], self.v[m], false);
                } else {
                    // RAX1: per 64-bit element, Zd = Zn ^ ROL(Zm, 1).
                    let (zn, zm) = (self.v[n], self.v[m]);
                    let lo = (zn as u64) ^ (zm as u64).rotate_left(1);
                    let hi = ((zn >> 64) as u64) ^ ((zm >> 64) as u64).rotate_left(1);
                    self.v[zd] = (lo as u128) | ((hi as u128) << 64);
                }
                Ok(CpuExit::Continue)
            }
            _ => Ok(CpuExit::Undefined(insn)),
        }
    }

    /// Execute SVE2 integer multiply / multiply-add by an indexed element.
    /// `Zm[index]` (selected within the single 128-bit segment for VL=128) is
    /// the shared second factor for every destination lane. MUL/MLA/MLS take
    /// the truncated low half of the product; SQDMULH/SQRDMULH take the
    /// saturating (optionally rounded) doubled high half.
    pub(crate) fn exec_sve2_mul_indexed(
        &mut self,
        insn: u32,
        zn: usize,
        zd: usize,
    ) -> Result<CpuExit, ArmError> {
        // Element size and (index, Zm) packing differ per size: H uses a 3-bit
        // index (bit22:bit20:bit19) with Zm in z0-z7; S a 2-bit index
        // (bit20:bit19) with Zm in z0-z7; D a 1-bit index (bit20), Zm in z0-z15.
        let (esize, index, zm): (usize, usize, usize) = if (insn >> 23) & 1 == 0 {
            let idx = (((insn >> 22) & 1) << 2) | (((insn >> 20) & 1) << 1) | ((insn >> 19) & 1);
            (2, idx as usize, ((insn >> 16) & 0x7) as usize)
        } else if (insn >> 22) & 1 == 0 {
            let idx = (((insn >> 20) & 1) << 1) | ((insn >> 19) & 1);
            (4, idx as usize, ((insn >> 16) & 0x7) as usize)
        } else {
            (
                8,
                ((insn >> 20) & 1) as usize,
                ((insn >> 16) & 0xF) as usize,
            )
        };
        let bits = (esize * 8) as u32;
        let mask = elem_mask(bits);
        let op = (insn >> 10) & 0x3F;
        let n = self.v[zn].to_le_bytes();
        let m = self.v[zm].to_le_bytes();
        let acc = self.v[zd].to_le_bytes();
        let mut dst = acc;
        let m_val = read_elem(&m, index * esize, esize);
        let m_s = sext_elem(m_val, bits);
        let lo = -(1i128 << (bits - 1));
        let hi = (1i128 << (bits - 1)) - 1;
        let elements = 16 / esize;
        for e in 0..elements {
            let off = e * esize;
            let a = read_elem(&n, off, esize);
            let res: u64 = match op {
                0b111110 => a.wrapping_mul(m_val) & mask, // MUL (low half)
                0b000010 => read_elem(&acc, off, esize).wrapping_add(a.wrapping_mul(m_val)) & mask, // MLA
                0b000011 => read_elem(&acc, off, esize).wrapping_sub(a.wrapping_mul(m_val)) & mask, // MLS
                0b111100 => {
                    // SQDMULH: sat((2*a*b) >> bits) == sat((a*b) >> (bits-1)).
                    let prod = sext_elem(a, bits) * m_s;
                    (prod >> (bits - 1)).clamp(lo, hi) as u64 & mask
                }
                0b111101 => {
                    // SQRDMULH: sat((2*a*b + 2^(bits-1)) >> bits), rewritten as
                    // sat((a*b + 2^(bits-2)) >> (bits-1)) to avoid i128 overflow.
                    let prod = sext_elem(a, bits) * m_s;
                    ((prod + (1i128 << (bits - 2))) >> (bits - 1)).clamp(lo, hi) as u64 & mask
                }
                0b000100 | 0b000101 => {
                    // SQRDMLAH (000100) / SQRDMLSH (000101): Zda + rounded
                    // doubling-high of (+/-)a*Zm[idx]; the accumulate saturates.
                    // The product is negated before the rounding bias, matching
                    // qemu (differs from negating the rounded result at ties).
                    let prod = sext_elem(a, bits) * m_s;
                    let p = if op == 0b000101 { -prod } else { prod };
                    let sdrh = (p + (1i128 << (bits - 2))) >> (bits - 1);
                    let cur = sext_elem(read_elem(&acc, off, esize), bits);
                    (cur + sdrh).clamp(lo, hi) as u64 & mask
                }
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            write_elem(&mut dst, off, esize, res);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }
}
