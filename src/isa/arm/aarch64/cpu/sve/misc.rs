//! misc.rs

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
    /// Execute SVE integer unpredicated operations.
    pub(crate) fn exec_sve_int_unpred(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        zm: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        // bits[12:10]: 000=ADD 001=SUB 100=SQADD 101=UQADD 110=SQSUB 111=UQSUB.
        // Map each to the verified NEON three-same integer core (u, opcode).
        let opc = (insn >> 10) & 0x7;
        let (u, neon_op) = match opc {
            0b000 => (0, 0b10000), // ADD
            0b001 => (1, 0b10000), // SUB
            0b100 => (0, 0b00001), // SQADD
            0b101 => (1, 0b00001), // UQADD
            0b110 => (0, 0b00101), // SQSUB
            0b111 => (1, 0b00101), // UQSUB
            _ => return Ok(CpuExit::Undefined(insn)),
        };
        let bits = (esize * 8) as u32;
        let elements = 16 / esize;
        let src = self.v[zn].to_le_bytes();
        let src2 = self.v[zm].to_le_bytes();
        let mut dst = [0u8; 16];
        for e in 0..elements {
            let off = e * esize;
            let a = read_elem(&src, off, esize);
            let b = read_elem(&src2, off, esize);
            let r = adv_simd_three_same_int(u, neon_op, bits, a, b, 0).0;
            write_elem(&mut dst, off, esize, r);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }

    /// Execute SVE unpredicated bitwise logical (AND/ORR/EOR/BIC), selected by
    /// bits[23:22], over the whole vector (element size is irrelevant).
    pub(crate) fn exec_sve_logical_unpred(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        zm: usize,
    ) -> Result<CpuExit, ArmError> {
        let a = self.v[zn];
        let b = self.v[zm];
        self.v[zd] = match (insn >> 22) & 0x3 {
            0b00 => a & b, // AND
            0b01 => a | b, // ORR
            0b10 => a ^ b, // EOR
            _ => a & !b,   // BIC
        };
        Ok(CpuExit::Continue)
    }

    /// Execute SVE FP predicated operations.
    /// FEAT_SVE_B16B16 bf16 data-processing. Returns `Some(result)` if `insn` is
    /// a recognised bf16 op, else `None` (so the caller continues its normal
    /// f16/f32/f64 dispatch). All forms are 8-bit-exponent bf16 with FPCR-default
    /// (round-to-nearest, no flush, propagate-NaN) handling via `bf16_binop` /
    /// `bf16_fma`. The bf16 ops occupy the size==00 encoding slots (and, for the
    /// indexed forms, use bit22 as the high index bit).
    pub(crate) fn try_exec_sve_bf16(&mut self, insn: u32) -> Option<Result<CpuExit, ArmError>> {
        let top = (insn >> 24) & 0xFF;
        let zd = (insn & 0x1F) as usize;
        let has_sve_b16b16 = self.config.features.contains(ArmFeatures::SVE_B16B16);
        // ---- 0x65: unpredicated 3-same, predicated binary, predicated FMA ----
        if top == 0b01100101 && (insn >> 22) & 0x3 == 0b00 {
            let bit21 = (insn >> 21) & 1;
            // Unpredicated BFADD/BFSUB/BFMUL: bit21==0, bits[15:12]==0000,
            // opc=bits[11:10]. Zm=bits[20:16], Zn=bits[9:5].
            if bit21 == 0 && (insn >> 12) & 0xF == 0b0000 {
                if !has_sve_b16b16 {
                    return Some(Ok(CpuExit::Undefined(insn)));
                }
                let kind = match (insn >> 10) & 0x3 {
                    0b00 => FpKind::Add,
                    0b01 => FpKind::Sub,
                    0b10 => FpKind::Mul,
                    _ => return Some(Ok(CpuExit::Undefined(insn))),
                };
                let n = self.v[((insn >> 5) & 0x1F) as usize].to_le_bytes();
                let m = self.v[((insn >> 16) & 0x1F) as usize].to_le_bytes();
                let mut dst = [0u8; 16];
                for e in 0..8 {
                    let r = bf16_binop(
                        kind,
                        read_elem(&n, e * 2, 2) as u16,
                        read_elem(&m, e * 2, 2) as u16,
                    );
                    write_elem(&mut dst, e * 2, 2, r as u64);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                return Some(Ok(CpuExit::Continue));
            }
            // Predicated BFADD/.../BFMIN (merging): bit21==0, bits[15:13]==100,
            // opc=bits[20:16]. Zdn=Zd, Zm=bits[9:5], Pg=bits[12:10].
            if bit21 == 0 && (insn >> 13) & 0x7 == 0b100 {
                if !has_sve_b16b16 {
                    return Some(Ok(CpuExit::Undefined(insn)));
                }
                let kind = match (insn >> 16) & 0x1F {
                    0b00000 => FpKind::Add,
                    0b00001 => FpKind::Sub,
                    0b00010 => FpKind::Mul,
                    0b00100 => FpKind::MaxNm,
                    0b00101 => FpKind::MinNm,
                    0b00110 => FpKind::Max,
                    0b00111 => FpKind::Min,
                    _ => return Some(Ok(CpuExit::Undefined(insn))),
                };
                let pred = self.sve_p[((insn >> 10) & 0x7) as usize];
                let dn = self.v[zd].to_le_bytes();
                let m = self.v[((insn >> 5) & 0x1F) as usize].to_le_bytes();
                let mut dst = dn;
                for e in 0..8 {
                    if (pred >> (e * 2)) & 1 == 0 {
                        continue;
                    }
                    let r = bf16_binop(
                        kind,
                        read_elem(&dn, e * 2, 2) as u16,
                        read_elem(&m, e * 2, 2) as u16,
                    );
                    write_elem(&mut dst, e * 2, 2, r as u64);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                return Some(Ok(CpuExit::Continue));
            }
            // Predicated BFMLA (bit13==0) / BFMLS (bit13==1) (merging): bit21==1,
            // bits[15:14]==00. Zda=Zd, Zn=bits[9:5], Zm=bits[20:16], Pg=bits[12:10].
            if bit21 == 1 && (insn >> 14) & 0x3 == 0b00 {
                if !has_sve_b16b16 {
                    return Some(Ok(CpuExit::Undefined(insn)));
                }
                let sub = (insn >> 13) & 1 == 1;
                let pred = self.sve_p[((insn >> 10) & 0x7) as usize];
                let a = self.v[zd].to_le_bytes();
                let n = self.v[((insn >> 5) & 0x1F) as usize].to_le_bytes();
                let m = self.v[((insn >> 16) & 0x1F) as usize].to_le_bytes();
                let mut dst = a;
                for e in 0..8 {
                    if (pred >> (e * 2)) & 1 == 0 {
                        continue;
                    }
                    let r = bf16_fma(
                        read_elem(&a, e * 2, 2) as u16,
                        read_elem(&n, e * 2, 2) as u16,
                        read_elem(&m, e * 2, 2) as u16,
                        sub,
                    );
                    write_elem(&mut dst, e * 2, 2, r as u64);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                return Some(Ok(CpuExit::Continue));
            }
            return None;
        }
        // ---- 0x64: BFCLAMP and indexed BFMUL/BFMLA/BFMLS ----
        if top == 0b01100100 && (insn >> 23) & 1 == 0 && (insn >> 21) & 1 == 1 {
            let op6 = (insn >> 10) & 0x3F;
            // BFCLAMP (size==00): Zd = bf16 minnum(maxnum(Zn, Zd), Zm).
            if op6 == 0b001001 && (insn >> 22) & 0x3 == 0b00 {
                if !has_sve_b16b16 {
                    return Some(Ok(CpuExit::Undefined(insn)));
                }
                let n = self.v[((insn >> 5) & 0x1F) as usize].to_le_bytes();
                let m = self.v[((insn >> 16) & 0x1F) as usize].to_le_bytes();
                let d = self.v[zd].to_le_bytes();
                let mut dst = [0u8; 16];
                for e in 0..8 {
                    let lo = bf16_binop(
                        FpKind::MaxNm,
                        read_elem(&n, e * 2, 2) as u16,
                        read_elem(&d, e * 2, 2) as u16,
                    );
                    let r = bf16_binop(FpKind::MinNm, lo, read_elem(&m, e * 2, 2) as u16);
                    write_elem(&mut dst, e * 2, 2, r as u64);
                }
                self.v[zd] = u128::from_le_bytes(dst);
                return Some(Ok(CpuExit::Continue));
            }
            // Indexed BFMUL/BFMLA/BFMLS (.h): index=(bit22<<2)|bits[20:19],
            // Zm=bits[18:16], Zn=bits[9:5]. The index-th bf16 of each 128-bit
            // segment is the broadcast second factor (Zm.h[index] at VL=128).
            let (mul, sub) = match op6 {
                0b001010 => (true, false),  // BFMUL
                0b000010 => (false, false), // BFMLA
                0b000011 => (false, true),  // BFMLS
                _ => return None,
            };
            if !has_sve_b16b16 {
                return Some(Ok(CpuExit::Undefined(insn)));
            }
            let index = ((((insn >> 22) & 1) << 2) | ((insn >> 19) & 0x3)) as usize;
            let n = self.v[((insn >> 5) & 0x1F) as usize].to_le_bytes();
            let m = self.v[((insn >> 16) & 0x7) as usize].to_le_bytes();
            let a = self.v[zd].to_le_bytes();
            let mb = read_elem(&m, index * 2, 2) as u16; // Zm.h[index]
            let mut dst = [0u8; 16];
            for e in 0..8 {
                let ne = read_elem(&n, e * 2, 2) as u16;
                let r = if mul {
                    bf16_binop(FpKind::Mul, ne, mb)
                } else {
                    bf16_fma(read_elem(&a, e * 2, 2) as u16, ne, mb, sub)
                };
                write_elem(&mut dst, e * 2, 2, r as u64);
            }
            self.v[zd] = u128::from_le_bytes(dst);
            return Some(Ok(CpuExit::Continue));
        }
        None
    }

    /// Execute SVE FCVT (predicated FP precision conversion between fp16/fp32/
    /// fp64). The per-element container size is the larger of the source and
    /// destination widths; the source value occupies the low bits of its
    /// container and the (zero-extended) result is written back. Predication is
    /// byte-granular at the container size and merges (inactive lanes keep Zd).
    pub(crate) fn exec_sve_fcvt(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        pg: usize,
    ) -> Result<CpuExit, ArmError> {
        let opc = (insn >> 22) & 0x3;
        let opc2 = (insn >> 16) & 0x3;
        // round_odd marks FCVTX (double->single, round-to-odd) which shares the
        // (8,4) widths with regular FCVT double->single but uses RO rounding.
        let (src_sz, dst_sz, round_odd, bf): (usize, usize, bool, bool) = match (opc, opc2) {
            (0b10, 0b01) => (2, 4, false, false), // half   -> single
            (0b11, 0b01) => (2, 8, false, false), // half   -> double
            (0b10, 0b00) => (4, 2, false, false), // single -> half
            (0b11, 0b11) => (4, 8, false, false), // single -> double
            (0b11, 0b00) => (8, 2, false, false), // double -> half
            (0b11, 0b10) => (8, 4, false, false), // double -> single
            (0b00, 0b10) => (8, 4, true, false),  // FCVTX  double -> single (round-to-odd)
            (0b10, 0b10) => (4, 2, false, true),  // BFCVT  single -> bf16
            _ => return Ok(CpuExit::Undefined(insn)),
        };
        let cont = src_sz.max(dst_sz);
        let elements = 16 / cont;
        let pred = self.sve_p[pg];
        let operand = self.v[zn].to_le_bytes();
        let mut dst = self.v[zd].to_le_bytes(); // merging: start from Zd
        for e in 0..elements {
            let off = e * cont;
            if (pred >> off) & 1 == 0 {
                continue;
            }
            let x = read_elem(&operand, off, src_sz);
            let res = if !bf && fp_is_nan_bits(src_sz, x) {
                fp_convert_nan(x, src_sz, dst_sz)
            } else {
                if bf {
                    f32_to_bf16_with_fpcr(x as u32, self.fpcr) as u64
                } else {
                    fp_cvt_elem(x, src_sz, dst_sz, round_odd, self.fpcr)
                }
            };
            self.fpsr |= if bf {
                fp_status_bfcvt_with_fpcr(x as u32, res as u16, self.fpcr)
            } else {
                fp_status_cvt_precision_with_fpcr_rounding(
                    x, src_sz, dst_sz, res, round_odd, self.fpcr,
                )
            };
            write_elem(&mut dst, off, cont, res);
        }
        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }
}
