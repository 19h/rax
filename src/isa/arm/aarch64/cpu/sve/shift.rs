//! shift.rs

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
    /// Execute SVE predicated shift by immediate (destructive, merging). The
    /// element size AND shift amount are jointly encoded in tsz:imm: esize is
    /// the lowest set bit of tsize=tszh:tszl; for ASR/LSR amount = 2*esize -
    /// tszimm, for LSL amount = tszimm - esize.
    pub(crate) fn exec_sve_shift_imm(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let tszh = (insn >> 22) & 0x3;
        let tszl = (insn >> 8) & 0x3;
        let imm3 = (insn >> 5) & 0x7;
        let tsize = (tszh << 2) | tszl;
        if tsize == 0 {
            return Ok(CpuExit::Undefined(insn));
        }
        // esize from the highest set bit of tsize (0001->8, 001x->16, 01xx->32,
        // 1xxx->64).
        let bits: u32 = if tsize & 0b1000 != 0 {
            64
        } else if tsize & 0b0100 != 0 {
            32
        } else if tsize & 0b0010 != 0 {
            16
        } else {
            8
        };
        let esize = (bits / 8) as usize;
        let tszimm = (tsize << 3) | imm3;
        // The operation is the full bits[21:16]; the low three bits alone do not
        // distinguish e.g. ASRD (000_100) from SRSHR (001_100).
        let op6 = (insn >> 16) & 0x3F;
        if !matches!(
            op6,
            0b000_000
                | 0b000_001
                | 0b000_011
                | 0b000_100
                | 0b000_110
                | 0b000_111
                | 0b001_100
                | 0b001_101
                | 0b001_111
        ) {
            return Ok(CpuExit::Undefined(insn));
        }
        // Shift-left ops take amount = tszimm - bits; shift-right ops take
        // amount = 2*bits - tszimm.
        let is_shl = matches!(op6, 0b000_011 | 0b000_110 | 0b000_111 | 0b001_111);
        let amount = if is_shl {
            tszimm - bits
        } else {
            2 * bits - tszimm
        };
        let pg = ((insn >> 10) & 0x7) as usize;
        let zd = (insn & 0x1F) as usize;
        let pred = self.sve_p[pg];
        let elements = 16 / esize;
        let mask = elem_mask(bits);
        let a_reg = self.v[zd].to_le_bytes();
        let mut dst = a_reg;
        for e in 0..elements {
            if (pred >> (e * esize)) & 1 == 0 {
                continue;
            }
            let off = e * esize;
            let v = read_elem(&a_reg, off, esize);
            let r = match op6 {
                0b000_000 => (sext_elem(v, bits) >> amount) as u64 & mask, // ASR
                0b000_001 => (uext_elem(v, bits) >> amount) as u64 & mask, // LSR
                0b000_011 => (uext_elem(v, bits) << amount) as u64 & mask, // LSL
                0b000_100 => {
                    // ASRD: signed shift-right rounding toward zero (divide).
                    let n = sext_elem(v, bits);
                    let bias = if n < 0 { (1i128 << amount) - 1 } else { 0 };
                    ((n + bias) >> amount) as u64 & mask
                }
                0b000_110 => {
                    // SQSHL: signed saturating shift left.
                    if bits == 64 {
                        sqrshl_d(v as i64, amount as i64, false, true) as u64 & mask
                    } else {
                        sqrshl_bhs(sext_elem(v, bits) as i32, amount as i32, bits, false, true)
                            as u64
                            & mask
                    }
                }
                0b000_111 => {
                    // UQSHL: unsigned saturating shift left.
                    if bits == 64 {
                        uqrshl_d(uext_elem(v, bits) as u64, amount as i64, false, true) & mask
                    } else {
                        uqrshl_bhs(uext_elem(v, bits) as u32, amount as i32, bits, false, true)
                            as u64
                            & mask
                    }
                }
                0b001_100 => sve_srshr(sext_elem(v, bits) as i64, amount) as u64 & mask, // SRSHR
                0b001_101 => sve_urshr(uext_elem(v, bits) as u64, amount) & mask,        // URSHR
                0b001_111 => {
                    // SQSHLU: signed shift left, saturating into the unsigned range.
                    let src = sext_elem(v, bits);
                    if src < 0 {
                        0
                    } else if bits == 64 {
                        uqrshl_d(src as u64, amount as i64, false, true) & mask
                    } else {
                        uqrshl_bhs(src as u32, amount as i32, bits, false, true) as u64 & mask
                    }
                }
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            write_elem(&mut dst, off, esize, r);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }

    /// Execute SVE unpredicated shift by immediate. The element size and shift
    /// amount are encoded in tszh:tszl:imm3.
    pub(crate) fn exec_sve_shift_imm_unpred(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let tsize = (((insn >> 22) & 0x3) << 2) | ((insn >> 19) & 0x3);
        if tsize == 0 {
            return Ok(CpuExit::Undefined(insn));
        }
        let bits: u32 = if tsize & 0b1000 != 0 {
            64
        } else if tsize & 0b0100 != 0 {
            32
        } else if tsize & 0b0010 != 0 {
            16
        } else {
            8
        };
        let imm3 = (insn >> 16) & 0x7;
        let tszimm = (tsize << 3) | imm3;
        let op = (insn >> 10) & 0x3F;
        let is_lsl = op == 0b100111;
        let amount = if is_lsl {
            tszimm - bits
        } else {
            2 * bits - tszimm
        };
        let esize = (bits / 8) as usize;
        let elements = 16 / esize;
        let mask = elem_mask(bits);
        let zn = ((insn >> 5) & 0x1F) as usize;
        let zd = (insn & 0x1F) as usize;
        let src = self.v[zn].to_le_bytes();
        let mut dst = [0u8; 16];
        for e in 0..elements {
            let off = e * esize;
            let v = read_elem(&src, off, esize);
            let r = match op {
                0b100100 => (sext_elem(v, bits) >> amount) as u64 & mask,
                0b100101 => {
                    if amount >= bits {
                        0
                    } else {
                        (v >> amount) & mask
                    }
                }
                0b100111 => (v << amount) & mask,
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            write_elem(&mut dst, off, esize, r);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }

    /// Execute SVE predicated shift by vector (destructive): Zdn = shift(Zdn,
    /// Zm) per active element. opc=bits[18:16]: 000=ASR, 001=LSR, 011=LSL. The
    /// shift amount is the (unsigned) Zm element; out-of-range gives 0 (LSR/LSL)
    /// or a full arithmetic shift (ASR). Pg is byte-granular.
    pub(crate) fn exec_sve_shift_pred(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        pg: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        let opc = (insn >> 16) & 0x7;
        let pred = self.sve_p[pg];
        let elements = 16 / esize;
        let bits = (esize * 8) as u32;
        let mask = elem_mask(bits);
        let a_reg = self.v[zd].to_le_bytes(); // Zdn
        let b_reg = self.v[zn].to_le_bytes(); // Zm-field
        // bit18 selects the reversed form (ASRR/LSRR/LSLR), which swaps the
        // value and shift-amount operands; base op is bits[17:16].
        let reversed = opc & 0b100 != 0;
        let base_op = opc & 0b011;
        if base_op == 0b010 {
            return Ok(CpuExit::Undefined(insn));
        }
        let mut dst = a_reg;
        for e in 0..elements {
            if (pred >> (e * esize)) & 1 == 0 {
                continue;
            }
            let off = e * esize;
            let za = read_elem(&a_reg, off, esize);
            let zb = read_elem(&b_reg, off, esize);
            let (a, sh) = if reversed { (zb, za) } else { (za, zb) };
            let r = match base_op {
                0b000 => {
                    let s = sh.min((bits - 1) as u64);
                    (sext_elem(a, bits) >> s) as u64 & mask
                }
                0b001 => {
                    if sh >= bits as u64 {
                        0
                    } else {
                        (a >> sh) & mask
                    }
                }
                0b011 => {
                    if sh >= bits as u64 {
                        0
                    } else {
                        (a << sh) & mask
                    }
                }
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            write_elem(&mut dst, off, esize, r);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }

    /// Execute SVE predicated shift by wide elements. Element sizes B/H/S use
    /// the 64-bit Zm lane that covers the destination element as the shift
    /// amount; D elements are unallocated.
    pub(crate) fn exec_sve_shift_wide_pred(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        pg: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        if esize == 8 {
            return Ok(CpuExit::Undefined(insn));
        }

        let opc = (insn >> 16) & 0x7;
        let pred = self.sve_p[pg];
        let elements = 16 / esize;
        let bits = (esize * 8) as u32;
        let mask = elem_mask(bits);
        let a_reg = self.v[zd].to_le_bytes(); // Zdn
        let b_reg = self.v[zn].to_le_bytes(); // Zm.D shift amounts
        let mut dst = a_reg;
        for e in 0..elements {
            let off = e * esize;
            if (pred >> off) & 1 == 0 {
                continue;
            }
            let a = read_elem(&a_reg, off, esize);
            let sh = read_elem(&b_reg, (off / 8) * 8, 8);
            let r = match opc {
                0b000 => {
                    let s = sh.min((bits - 1) as u64);
                    (sext_elem(a, bits) >> s) as u64 & mask
                }
                0b001 => {
                    if sh >= bits as u64 {
                        0
                    } else {
                        (a >> sh) & mask
                    }
                }
                0b011 => {
                    if sh >= bits as u64 {
                        0
                    } else {
                        (a << sh) & mask
                    }
                }
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            write_elem(&mut dst, off, esize, r);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }

    /// Execute SVE unpredicated shift by wide elements. Element sizes B/H/S use
    /// the 64-bit Zm lane that covers the source element as the shift amount; D
    /// elements are unallocated.
    pub(crate) fn exec_sve_shift_wide_unpred(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        zm: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        if esize == 8 {
            return Ok(CpuExit::Undefined(insn));
        }

        let opc = (insn >> 10) & 0x7;
        let elements = 16 / esize;
        let bits = (esize * 8) as u32;
        let mask = elem_mask(bits);
        let a_reg = self.v[zn].to_le_bytes();
        let b_reg = self.v[zm].to_le_bytes(); // Zm.D shift amounts
        let mut dst = [0u8; 16];
        for e in 0..elements {
            let off = e * esize;
            let a = read_elem(&a_reg, off, esize);
            let sh = read_elem(&b_reg, (off / 8) * 8, 8);
            let r = match opc {
                0b000 => {
                    let s = sh.min((bits - 1) as u64);
                    (sext_elem(a, bits) >> s) as u64 & mask
                }
                0b001 => {
                    if sh >= bits as u64 {
                        0
                    } else {
                        (a >> sh) & mask
                    }
                }
                0b011 => {
                    if sh >= bits as u64 {
                        0
                    } else {
                        (a << sh) & mask
                    }
                }
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            write_elem(&mut dst, off, esize, r);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }
}
