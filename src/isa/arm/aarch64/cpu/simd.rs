//! Advanced SIMD, FP, and crypto instruction execution

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

    /// Execute SIMD/FP instruction.
    pub(crate) fn exec_simd_fp(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        // Check if FP/SIMD is enabled
        let cpacr = self.sysregs.el1.cpacr;
        let fpen = (cpacr >> 20) & 0x3;

        if self.current_el == 0 && fpen != 0x3 {
            // FP/SIMD trapped at EL0
            return self.take_fp_access_trap();
        }
        if self.current_el == 1 && (fpen & 1) == 0 {
            // FP/SIMD trapped at EL1
            return self.take_fp_access_trap();
        }

        // Decode SIMD/FP instruction groups
        // Bits [28:25] = 0111 or 1111 for SIMD/FP
        // Bits [31:30] and [24:21] determine the specific group

        let op0 = (insn >> 28) & 0xF;
        let op1 = (insn >> 23) & 0x3;
        let op2 = (insn >> 19) & 0xF;
        let op3 = (insn >> 10) & 0x1FF;

        // Scalar FP data processing (three source): FMADD/FMSUB/FNMADD/FNMSUB.
        // bits[31:24] = 0001_1111
        if (insn >> 24) & 0xFF == 0b00011111 {
            let fp_type = (insn >> 22) & 0x3;
            let o1 = (insn >> 21) & 1;
            let rm = ((insn >> 16) & 0x1F) as usize;
            let o0 = (insn >> 15) & 1;
            let ra = ((insn >> 10) & 0x1F) as usize;
            let rn = ((insn >> 5) & 0x1F) as usize;
            let rd = (insn & 0x1F) as usize;
            match fp_type {
                0b00 | 0b01 | 0b11 => {
                    // Route through the canonicalising fused multiply-add. ARM
                    // negates the product (FMSUB/FNMADD) and/or the addend
                    // (FNMADD/FNMSUB) before FPMulAdd, which also flips the
                    // propagated NaN sign.
                    let eb: u32 = match fp_type {
                        0b00 => 32,
                        0b01 => 64,
                        _ => 16,
                    };
                    let m_mask: u64 = if eb == 64 { u64::MAX } else { (1u64 << eb) - 1 };
                    let n = self.v[rn] as u64 & m_mask;
                    let m = self.v[rm] as u64 & m_mask;
                    let a = self.v[ra] as u64 & m_mask;
                    let (nn, aa) = match (o1, o0) {
                        (0, 0) => (n, a),                                       // FMADD
                        (0, 1) => (fp_neg_bits_with_fpcr(n, eb, self.fpcr), a), // FMSUB
                        (1, 0) => (
                            fp_neg_bits_with_fpcr(n, eb, self.fpcr),
                            fp_neg_bits_with_fpcr(a, eb, self.fpcr),
                        ), // FNMADD
                        _ => (n, fp_neg_bits_with_fpcr(a, eb, self.fpcr)),      // FNMSUB
                    };
                    let r = fp_muladd_bits_with_fpcr(aa, nn, m, eb, self.fpcr);
                    self.fpsr |=
                        fp_status_fma_with_fpcr((eb / 8) as usize, aa, nn, m, r, self.fpcr);
                    self.v[rd] = (r & m_mask) as u128;
                }
                _ => return Err(ArmError::UndefinedInstruction(insn)),
            }
            return Ok(CpuExit::Continue);
        }

        // Scalar FP immediate (FMOV Sd/Dd/Hd, #imm): 00011110 ptype 1 imm8 100
        // 00000 Rd. imm8=bits[20:13]; the 8-bit float immediate expands per the
        // element size (h/s/d). Writes the low element and zeroes the upper bits.
        if (insn >> 24) & 0xFF == 0b00011110
            && (insn >> 21) & 1 == 1
            && (insn >> 10) & 0x7 == 0b100
            && (insn >> 5) & 0x1F == 0
        {
            let ptype = (insn >> 22) & 0x3;
            let esize = match ptype {
                0b00 => 4usize,
                0b01 => 8,
                0b11 => 2,
                _ => return Err(ArmError::UndefinedInstruction(insn)),
            };
            let imm8 = ((insn >> 13) & 0xFF) as u8;
            let rd = (insn & 0x1F) as usize;
            self.v[rd] = vfp_expand_imm(imm8, esize) as u128;
            return Ok(CpuExit::Continue);
        }

        // Scalar FP data processing (two source)
        // bits[31:24] = 0001_1110
        // bits[23:22] = type (size)
        // bit[21] = 1
        // bits[15:12] = opcode
        // bits[11:10] = 10
        if (insn >> 24) & 0xFF == 0b00011110 && (insn >> 21) & 1 == 1 && (insn >> 10) & 0x3 == 0b10
        {
            let fp_type = (insn >> 22) & 0x3;
            let opcode = (insn >> 12) & 0xF;
            let rm = ((insn >> 16) & 0x1F) as u8;
            let rn = ((insn >> 5) & 0x1F) as u8;
            let rd = (insn & 0x1F) as u8;

            // FMUL/FDIV/FADD/FSUB/FMAX/FMIN/FMAXNM/FMINNM via the canonicalising
            // helper; FNMUL = FPNeg(FPMul) flips the product's (incl. NaN) sign.
            let kind = match opcode {
                0b0000 | 0b1000 => FpKind::Mul,
                0b0001 => FpKind::Div,
                0b0010 => FpKind::Add,
                0b0011 => FpKind::Sub,
                0b0100 => FpKind::Max,
                0b0101 => FpKind::Min,
                0b0110 => FpKind::MaxNm,
                0b0111 => FpKind::MinNm,
                _ => return Err(ArmError::Unimplemented(format!("FP opcode {}", opcode))),
            };
            let nmul = opcode == 0b1000;
            match fp_type {
                0b00 => {
                    let (n, m) = (self.v[rn as usize] as u32, self.v[rm as usize] as u32);
                    let mut r = fp_three_same_f32_with_fpcr(kind, n, m, 0, self.fpcr);
                    self.fpsr |=
                        fp_status_binop_with_fpcr(4, kind, n as u64, m as u64, r as u64, self.fpcr);
                    if nmul {
                        r = fp_neg_bits_with_fpcr(r as u64, 32, self.fpcr) as u32;
                    }
                    self.v[rd as usize] = r as u128;
                }
                0b01 => {
                    let (n, m) = (self.v[rn as usize] as u64, self.v[rm as usize] as u64);
                    let mut r = fp_three_same_f64_with_fpcr(kind, n, m, 0, self.fpcr);
                    self.fpsr |= fp_status_binop_with_fpcr(8, kind, n, m, r, self.fpcr);
                    if nmul {
                        r = fp_neg_bits_with_fpcr(r, 64, self.fpcr);
                    }
                    self.v[rd as usize] = r as u128;
                }
                0b11 => {
                    let (n, m) = (self.v[rn as usize] as u16, self.v[rm as usize] as u16);
                    let mut r = sve_fp16_binop_with_fpcr(kind, n, m, self.fpcr);
                    self.fpsr |=
                        fp_status_binop_with_fpcr(2, kind, n as u64, m as u64, r as u64, self.fpcr);
                    if nmul {
                        r = fp_neg_bits_with_fpcr(r as u64, 16, self.fpcr) as u16;
                    }
                    self.v[rd as usize] = r as u128;
                }
                _ => return Err(ArmError::Unimplemented("FP16/reserved".to_string())),
            }
            return Ok(CpuExit::Continue);
        }

        // Scalar FP data processing (one source)
        // bits[31:24] = 0001_1110
        // bits[23:22] = type (size)
        // bit[21] = 1
        // bits[20:15] = opcode
        // bits[14:10] = 10000
        if (insn >> 24) & 0xFF == 0b00011110
            && (insn >> 21) & 1 == 1
            && (insn >> 10) & 0x1F == 0b10000
        {
            let fp_type = (insn >> 22) & 0x3;
            let opcode = (insn >> 15) & 0x1F;
            let rn = ((insn >> 5) & 0x1F) as u8;
            let rd = (insn & 0x1F) as u8;

            // BFCVT Hd, Sn (FEAT_BF16): single-precision -> bfloat16, RNE.
            // Encoded as ptype=01, opcode bits[20:15]=000110 (bits[19:15]=00110).
            if fp_type == 0b01 && opcode == 0b00110 {
                let x = self.v[rn as usize] as u32;
                let bf = f32_to_bf16_with_fpcr(x, self.fpcr);
                self.fpsr |= fp_status_bfcvt_with_fpcr(x, bf, self.fpcr);
                self.v[rd as usize] = bf as u128;
                return Ok(CpuExit::Continue);
            }

            // FCVT (precision change between h/s/d): opcode 0b001xx, bits[16:15]
            // (=opcode&3) select the destination precision (00=s,01=d,11=h),
            // ptype the source. Round-to-nearest-even; NaN via FPConvertNaN.
            if matches!(opcode, 0b00100 | 0b00101 | 0b00111) {
                let dst = opcode & 0x3;
                if dst == fp_type {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                let src_prec = match fp_type {
                    0b00 => 4usize,
                    0b01 => 8,
                    0b11 => 2,
                    _ => return Err(ArmError::UndefinedInstruction(insn)),
                };
                let dst_prec = match dst {
                    0b00 => 4usize,
                    0b01 => 8,
                    _ => 2,
                };
                let s = self.v[rn as usize] as u64;
                let r = fp_cvt_elem(s, src_prec, dst_prec, false, self.fpcr);
                self.fpsr |= fp_status_cvt_precision_with_fpcr(s, src_prec, dst_prec, r, self.fpcr);
                self.v[rd as usize] = r as u128;
                return Ok(CpuExit::Continue);
            }

            // FRINT32Z/X (opcode bits[19:15]=10000/10001) and FRINT64Z/X
            // (10010/10011): scalar FEAT_FRINTTS, f32 (ptype 00) / f64 (ptype 01).
            if matches!(opcode, 0b10000 | 0b10001 | 0b10010 | 0b10011) {
                let intsize = if opcode & 0b10 == 0 { 32 } else { 64 };
                let z = opcode & 1 == 0;
                self.v[rd as usize] = match fp_type {
                    0b00 => {
                        let a = self.v[rn as usize] as u32;
                        let r = frint_ts_f32_with_fpcr(a, intsize, z, self.fpcr);
                        self.fpsr |= fp_status_frint_ts_f32_with_fpcr(a, intsize, z, self.fpcr);
                        r as u128
                    }
                    0b01 => {
                        let a = self.v[rn as usize] as u64;
                        let r = frint_ts_f64_with_fpcr(a, intsize, z, self.fpcr);
                        self.fpsr |= fp_status_frint_ts_f64_with_fpcr(a, intsize, z, self.fpcr);
                        r as u128
                    }
                    _ => return Err(ArmError::UndefinedInstruction(insn)),
                };
                return Ok(CpuExit::Continue);
            }

            // FMOV is a plain copy; the FRINT/FABS/FNEG/FSQRT ops share the
            // verified two-reg FP element helpers (correct rounding modes).
            let kind = match opcode {
                0b00000 => None, // FMOV
                0b00001 => Some(TwoRegFp::Fabs),
                0b00010 => Some(TwoRegFp::Fneg),
                0b00011 => Some(TwoRegFp::Fsqrt),
                0b01000 => Some(TwoRegFp::RintN),
                0b01001 => Some(TwoRegFp::RintP),
                0b01010 => Some(TwoRegFp::RintM),
                0b01011 => Some(TwoRegFp::RintZ),
                0b01100 => Some(TwoRegFp::RintA),
                0b01110 => Some(TwoRegFp::RintX),
                0b01111 => Some(TwoRegFp::RintI),
                // 0b001xx with bit2 set are FCVT (precision change) -> handled by
                // the dedicated FCVT block; anything else is unallocated here.
                _ => return Err(ArmError::UndefinedInstruction(insn)),
            };
            match fp_type {
                0b00 => {
                    let a = self.v[rn as usize] as u32;
                    let r = match kind {
                        None => a,
                        Some(k) => fp_two_reg_f32_with_fpcr(k, a, self.fpcr),
                    };
                    self.fpsr |= fp_status_unop_with_fpcr(4, kind, a as u64, r as u64, self.fpcr);
                    self.v[rd as usize] = r as u128;
                }
                0b01 => {
                    let a = self.v[rn as usize] as u64;
                    let r = match kind {
                        None => a,
                        Some(k) => fp_two_reg_f64_with_fpcr(k, a, self.fpcr),
                    };
                    self.fpsr |= fp_status_unop_with_fpcr(8, kind, a, r, self.fpcr);
                    self.v[rd as usize] = r as u128;
                }
                0b11 => {
                    let a = self.v[rn as usize] as u16;
                    let r: u16 = match kind {
                        None => a, // FMOV
                        Some(TwoRegFp::Fabs) => {
                            fp_abs_bits_with_fpcr(a as u64, 16, self.fpcr) as u16
                        }
                        Some(TwoRegFp::Fneg) => {
                            fp_neg_bits_with_fpcr(a as u64, 16, self.fpcr) as u16
                        }
                        Some(TwoRegFp::Fsqrt) => fp16_sqrt_with_fpcr(a, self.fpcr),
                        Some(TwoRegFp::RintN) => fp16_frint_fixed_with_fpcr(a, 0, self.fpcr),
                        Some(TwoRegFp::RintX) | Some(TwoRegFp::RintI) => {
                            fp16_frint_with_fpcr(a, self.fpcr)
                        }
                        Some(TwoRegFp::RintM) => fp16_frint_fixed_with_fpcr(a, 1, self.fpcr),
                        Some(TwoRegFp::RintP) => fp16_frint_fixed_with_fpcr(a, 2, self.fpcr),
                        Some(TwoRegFp::RintZ) => fp16_frint_fixed_with_fpcr(a, 3, self.fpcr),
                        Some(TwoRegFp::RintA) => fp16_frint_fixed_with_fpcr(a, 4, self.fpcr),
                        _ => return Err(ArmError::UndefinedInstruction(insn)),
                    };
                    self.fpsr |= fp_status_unop_with_fpcr(2, kind, a as u64, r as u64, self.fpcr);
                    self.v[rd as usize] = r as u128;
                }
                _ => return Err(ArmError::UndefinedInstruction(insn)),
            }
            return Ok(CpuExit::Continue);
        }

        // FP compare
        // bits[31:24] = 0001_1110
        // bits[23:22] = type
        // bit[21] = 1
        // bits[15:14] = 00
        // bits[13:10] = 1000
        // bits[4:3] = opc
        // bits[2:0] = 0xx
        if (insn >> 24) & 0xFF == 0b00011110
            && (insn >> 21) & 1 == 1
            && (insn >> 14) & 0x3 == 0
            && (insn >> 10) & 0xF == 0b1000
            && (insn & 0x7) == 0
        {
            let fp_type = (insn >> 22) & 0x3;
            let rm = ((insn >> 16) & 0x1F) as u8;
            let rn = ((insn >> 5) & 0x1F) as u8;
            let opc = (insn >> 3) & 0x3;
            let cmp_with_zero = (insn & 0x8) != 0;
            let signal_all_nans = (insn & 0x10) != 0;

            match fp_type {
                0b00 => {
                    // Single precision
                    let raw1_bits = self.v[rn as usize] as u32;
                    let op1_bits = fp32_flush_input_with_fpcr(raw1_bits, self.fpcr);
                    let op1 = f32::from_bits(op1_bits);
                    let raw2_bits = self.v[rm as usize] as u32;
                    let op2_bits = if cmp_with_zero {
                        0
                    } else {
                        fp32_flush_input_with_fpcr(raw2_bits, self.fpcr)
                    };
                    let op2 = if cmp_with_zero {
                        0.0f32
                    } else {
                        f32::from_bits(op2_bits)
                    };

                    let (n, z, c, v) = if op1.is_nan() || op2.is_nan() {
                        if signal_all_nans || is_snan32(op1_bits) || is_snan32(op2_bits) {
                            self.fpsr |= FPSR_IOC;
                        }
                        (false, false, true, true)
                    } else if op1 == op2 {
                        (false, true, true, false)
                    } else if op1 < op2 {
                        (true, false, false, false)
                    } else {
                        (false, false, true, false)
                    };

                    if self.fpcr & FPCR_AH == 0
                        || !(is_nan32(raw1_bits) || !cmp_with_zero && is_nan32(raw2_bits))
                    {
                        self.fpsr |= fp_fz_input_status(4, raw1_bits as u64, self.fpcr)
                            | if cmp_with_zero {
                                0
                            } else {
                                fp_fz_input_status(4, raw2_bits as u64, self.fpcr)
                            };
                    }
                    self.set_n(n);
                    self.set_z(z);
                    self.set_c(c);
                    self.set_v(v);
                }
                0b01 => {
                    // Double precision
                    let raw1_bits = self.v[rn as usize] as u64;
                    let op1_bits = fp64_flush_input_with_fpcr(raw1_bits, self.fpcr);
                    let op1 = f64::from_bits(op1_bits);
                    let raw2_bits = self.v[rm as usize] as u64;
                    let op2_bits = if cmp_with_zero {
                        0
                    } else {
                        fp64_flush_input_with_fpcr(raw2_bits, self.fpcr)
                    };
                    let op2 = if cmp_with_zero {
                        0.0f64
                    } else {
                        f64::from_bits(op2_bits)
                    };

                    let (n, z, c, v) = if op1.is_nan() || op2.is_nan() {
                        if signal_all_nans || is_snan64(op1_bits) || is_snan64(op2_bits) {
                            self.fpsr |= FPSR_IOC;
                        }
                        (false, false, true, true)
                    } else if op1 == op2 {
                        (false, true, true, false)
                    } else if op1 < op2 {
                        (true, false, false, false)
                    } else {
                        (false, false, true, false)
                    };

                    if self.fpcr & FPCR_AH == 0
                        || !(is_nan64(raw1_bits) || !cmp_with_zero && is_nan64(raw2_bits))
                    {
                        self.fpsr |= fp_fz_input_status(8, raw1_bits, self.fpcr)
                            | if cmp_with_zero {
                                0
                            } else {
                                fp_fz_input_status(8, raw2_bits, self.fpcr)
                            };
                    }
                    self.set_n(n);
                    self.set_z(z);
                    self.set_c(c);
                    self.set_v(v);
                }
                0b11 => {
                    // Half precision (compared exactly via f64).
                    let op1_bits =
                        fp16_flush_input_with_fpcr(self.v[rn as usize] as u16, self.fpcr);
                    let op1 = fp16_to_f64(op1_bits);
                    let op2_bits = if cmp_with_zero {
                        0
                    } else {
                        fp16_flush_input_with_fpcr(self.v[rm as usize] as u16, self.fpcr)
                    };
                    let op2 = if cmp_with_zero {
                        0.0f64
                    } else {
                        fp16_to_f64(op2_bits)
                    };
                    let (n, z, c, v) = if op1.is_nan() || op2.is_nan() {
                        if signal_all_nans || fp16_is_snan(op1_bits) || fp16_is_snan(op2_bits) {
                            self.fpsr |= FPSR_IOC;
                        }
                        (false, false, true, true)
                    } else if op1 == op2 {
                        (false, true, true, false)
                    } else if op1 < op2 {
                        (true, false, false, false)
                    } else {
                        (false, false, true, false)
                    };
                    self.set_n(n);
                    self.set_z(z);
                    self.set_c(c);
                    self.set_v(v);
                }
                _ => return Err(ArmError::UndefinedInstruction(insn)),
            }
            return Ok(CpuExit::Continue);
        }

        // Floating-point conditional compare (FCCMP / FCCMPE)
        // bits[31:24]=0001_1110, bit21=1, bits[11:10]=01
        if (insn >> 24) & 0xFF == 0b00011110 && (insn >> 21) & 1 == 1 && (insn >> 10) & 0x3 == 0b01
        {
            let fp_type = (insn >> 22) & 0x3;
            let rm = ((insn >> 16) & 0x1F) as usize;
            let cond = ((insn >> 12) & 0xF) as u8;
            let rn = ((insn >> 5) & 0x1F) as usize;
            let nzcv_imm = (insn & 0xF) as u8;
            let signal_all_nans = ((insn >> 4) & 1) != 0;

            let to_f64 = |bits: u128| -> Option<f64> {
                Some(match fp_type {
                    0b00 => {
                        f32::from_bits(fp32_flush_input_with_fpcr(bits as u32, self.fpcr)) as f64
                    }
                    0b01 => f64::from_bits(fp64_flush_input_with_fpcr(bits as u64, self.fpcr)),
                    0b11 => fp16_to_f64(fp16_flush_input_with_fpcr(bits as u16, self.fpcr)),
                    _ => return None,
                })
            };
            let (a, b) = match (to_f64(self.v[rn]), to_f64(self.v[rm])) {
                (Some(a), Some(b)) => (a, b),
                _ => return Err(ArmError::UndefinedInstruction(insn)),
            };

            if self.condition_holds(cond) {
                let ah_nan = self.fpcr & FPCR_AH != 0
                    && match fp_type {
                        0b00 => is_nan32(self.v[rn] as u32) || is_nan32(self.v[rm] as u32),
                        0b01 => is_nan64(self.v[rn] as u64) || is_nan64(self.v[rm] as u64),
                        0b11 => fp16_is_nan(self.v[rn] as u16) || fp16_is_nan(self.v[rm] as u16),
                        _ => false,
                    };
                self.fpsr |= if ah_nan {
                    0
                } else {
                    match fp_type {
                        0b00 => {
                            fp_fz_input_status(4, self.v[rn] as u64, self.fpcr)
                                | fp_fz_input_status(4, self.v[rm] as u64, self.fpcr)
                        }
                        0b01 => {
                            fp_fz_input_status(8, self.v[rn] as u64, self.fpcr)
                                | fp_fz_input_status(8, self.v[rm] as u64, self.fpcr)
                        }
                        _ => 0,
                    }
                };
                let invalid = match fp_type {
                    0b00 => {
                        let an = self.v[rn] as u32;
                        let bm = self.v[rm] as u32;
                        (is_nan32(an) || is_nan32(bm))
                            && (signal_all_nans || is_snan32(an) || is_snan32(bm))
                    }
                    0b01 => {
                        let an = self.v[rn] as u64;
                        let bm = self.v[rm] as u64;
                        (is_nan64(an) || is_nan64(bm))
                            && (signal_all_nans || is_snan64(an) || is_snan64(bm))
                    }
                    0b11 => {
                        let an = self.v[rn] as u16;
                        let bm = self.v[rm] as u16;
                        (fp16_is_nan(an) || fp16_is_nan(bm))
                            && (signal_all_nans || fp16_is_snan(an) || fp16_is_snan(bm))
                    }
                    _ => false,
                };
                if invalid {
                    self.fpsr |= FPSR_IOC;
                }
                let (n, z, c, v) = if a.is_nan() || b.is_nan() {
                    (false, false, true, true)
                } else if a == b {
                    (false, true, true, false)
                } else if a < b {
                    (true, false, false, false)
                } else {
                    (false, false, true, false)
                };
                self.set_nzcv(n, z, c, v);
            } else {
                self.set_n(nzcv_imm & 0b1000 != 0);
                self.set_z(nzcv_imm & 0b0100 != 0);
                self.set_c(nzcv_imm & 0b0010 != 0);
                self.set_v(nzcv_imm & 0b0001 != 0);
            }
            return Ok(CpuExit::Continue);
        }

        // Floating-point conditional select (FCSEL)
        // bits[31:24]=0001_1110, bit21=1, bits[11:10]=11
        if (insn >> 24) & 0xFF == 0b00011110 && (insn >> 21) & 1 == 1 && (insn >> 10) & 0x3 == 0b11
        {
            let fp_type = (insn >> 22) & 0x3;
            let rm = ((insn >> 16) & 0x1F) as usize;
            let cond = ((insn >> 12) & 0xF) as u8;
            let rn = ((insn >> 5) & 0x1F) as usize;
            let rd = (insn & 0x1F) as usize;

            let width: u32 = match fp_type {
                0b00 => 32,
                0b01 => 64,
                0b11 => 16,
                _ => return Err(ArmError::UndefinedInstruction(insn)),
            };
            let src = if self.condition_holds(cond) {
                self.v[rn]
            } else {
                self.v[rm]
            };
            let mask = (1u128 << width) - 1;
            self.v[rd] = src & mask; // scalar result, upper bits zeroed
            return Ok(CpuExit::Continue);
        }

        // FMOV (general) - move between FP and general registers
        // bits[31] = sf
        // bits[30:24] = 0011110
        // bits[23:22] = type
        // bit[21] = 1
        // bits[20:19] = rmode
        // bits[18:16] = opcode
        // bits[15:10] = 000000
        if (insn >> 24) & 0x7F == 0b0011110
            && (insn >> 21) & 1 == 1
            && (insn >> 10) & 0x3F == 0
            && (insn >> 16) & 0x7 >= 0b110
        {
            let sf = (insn >> 31) & 1;
            let fp_type = (insn >> 22) & 0x3;
            let rmode = (insn >> 19) & 0x3;
            let opcode = (insn >> 16) & 0x7;
            let rn = ((insn >> 5) & 0x1F) as u8;
            let rd = (insn & 0x1F) as u8;

            match (sf, fp_type, rmode, opcode) {
                // FMOV Wd, Sn
                (0, 0b00, 0b00, 0b110) => {
                    let val = self.v[rn as usize] as u32;
                    self.set_w(rd, val);
                }
                // FMOV Sd, Wn
                (0, 0b00, 0b00, 0b111) => {
                    let val = self.get_w(rn);
                    self.v[rd as usize] = val as u128;
                }
                // FMOV Xd, Dn
                (1, 0b01, 0b00, 0b110) => {
                    let val = self.v[rn as usize] as u64;
                    self.set_x(rd, val);
                }
                // FMOV Dn, Xn
                (1, 0b01, 0b00, 0b111) => {
                    let val = self.get_x(rn);
                    self.v[rd as usize] = val as u128;
                }
                // FMOV Xd, Vn.D[1]
                (1, 0b10, 0b01, 0b110) => {
                    let val = (self.v[rn as usize] >> 64) as u64;
                    self.set_x(rd, val);
                }
                // FMOV Vd.D[1], Xn
                (1, 0b10, 0b01, 0b111) => {
                    let val = self.get_x(rn);
                    let lower = self.v[rd as usize] as u64;
                    self.v[rd as usize] = ((val as u128) << 64) | (lower as u128);
                }
                // FMOV Wd, Hn / FMOV Xd, Hn (FEAT_FP16): zero-extend the 16 bits.
                (_, 0b11, 0b00, 0b110) => {
                    let val = self.v[rn as usize] as u16 as u64;
                    if sf == 1 {
                        self.set_x(rd, val);
                    } else {
                        self.set_w(rd, val as u32);
                    }
                }
                // FMOV Hd, Wn / FMOV Hd, Xn (FEAT_FP16): low 16 bits -> Hd.
                (_, 0b11, 0b00, 0b111) => {
                    let val = if sf == 1 {
                        self.get_x(rn)
                    } else {
                        self.get_w(rn) as u64
                    };
                    self.v[rd as usize] = (val & 0xFFFF) as u128;
                }
                // FJCVTZS Wd, Dn (FEAT_JSCVT): JS double->int32 (round toward
                // zero, modulo 2^32). Z=1 iff exact (finite, integral, in range);
                // N=C=V=0.
                (0, 0b01, 0b11, 0b110) => {
                    let bits = self.v[rn as usize] as u64;
                    let x = f64::from_bits(bits);
                    let (res, exact): (i32, bool) = if !x.is_finite() {
                        (0, false)
                    } else {
                        let t = x.trunc();
                        // JS ToInt32: reduce trunc(x) modulo 2^32 to a signed 32-bit.
                        let m = t.rem_euclid(4294967296.0); // [0, 2^32)
                        let r = if m >= 2147483648.0 {
                            (m - 4294967296.0) as i64
                        } else {
                            m as i64
                        } as i32;
                        // -0.0 is "inexact" for JavaScript (qemu helper_fjcvtzs).
                        let neg_zero = x == 0.0 && x.is_sign_negative();
                        (r, x == r as f64 && !neg_zero)
                    };
                    self.set_w(rd, res as u32);
                    self.set_nzcv(false, exact, false, false);
                    self.fpsr |= fp_status_fjcvtzs_with_fpcr(bits, self.fpcr);
                }
                _ => {
                    return Err(ArmError::Unimplemented(format!(
                        "FMOV general variant sf={} type={} rmode={} op={}",
                        sf, fp_type, rmode, opcode
                    )));
                }
            }
            return Ok(CpuExit::Continue);
        }

        // FCVT - floating-point convert precision
        // bits[31:24] = 0001_1110
        // bits[23:22] = type (source)
        // bit[21] = 1
        // bits[20:17] = 0001
        // bits[16:15] = opc (dest)
        // bits[14:10] = 10000
        if (insn >> 24) & 0xFF == 0b00011110
            && (insn >> 21) & 1 == 1
            && (insn >> 17) & 0xF == 0b0001
            && (insn >> 10) & 0x1F == 0b10000
        {
            let src_type = (insn >> 22) & 0x3;
            let dst_type = (insn >> 15) & 0x3;
            let rn = ((insn >> 5) & 0x1F) as u8;
            let rd = (insn & 0x1F) as u8;

            match (src_type, dst_type) {
                // FCVT Dd, Sn (single to double)
                (0b00, 0b01) => {
                    let val = f32::from_bits(self.v[rn as usize] as u32);
                    let result = val as f64;
                    self.v[rd as usize] = result.to_bits() as u128;
                }
                // FCVT Sd, Dn (double to single)
                (0b01, 0b00) => {
                    let val = f64::from_bits(self.v[rn as usize] as u64);
                    self.v[rd as usize] = f64_to_f32_bits_with_fpcr(val, self.fpcr) as u128;
                }
                _ => {
                    return Err(ArmError::Unimplemented(format!(
                        "FCVT variant src={} dst={}",
                        src_type, dst_type
                    )));
                }
            }
            return Ok(CpuExit::Continue);
        }

        // Fixed-point conversion between FP and GPR (scalar): sf 0 0 11110 ptype
        // 0 rmode opcode scale Rn Rd, bit21==0. scale=bits[15:10], fbits=64-scale.
        // SCVTF/UCVTF (opcode 010/011, GPR int -> FP scaled by 2^-fbits) and
        // FCVTZS/FCVTZU (opcode 000/001, FP -> GPR int = trunc(FP * 2^fbits)).
        if (insn >> 24) & 0x7F == 0b0011110
            && (insn >> 21) & 1 == 0
            && matches!((insn >> 16) & 0x7, 0b000 | 0b001 | 0b010 | 0b011)
        {
            let sf = (insn >> 31) & 1;
            let ptype = (insn >> 22) & 0x3;
            let opcode = (insn >> 16) & 0x7;
            let scale = (insn >> 10) & 0x3F;
            let fbits = 64 - scale as i32;
            let rn = ((insn >> 5) & 0x1F) as u8;
            let rd = (insn & 0x1F) as u8;
            // 32-bit forms only support fbits 1..=32: scale<5> must be 1.
            // The rmode field must also be fixed (11 for FCVTZ*, 00 for *CVTF).
            if sf == 0 && (scale >> 5) & 1 == 0 {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            let rmode = (insn >> 19) & 0x3;
            let want_rmode = if opcode >= 0b010 { 0b00 } else { 0b11 };
            if rmode != want_rmode || ptype == 0b10 {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            if opcode == 0b010 || opcode == 0b011 {
                // GPR int -> FP, value = int / 2^fbits (the 2^-fbits scale is an
                // exact power of two, so a single FPRound of the integer suffices).
                let signed = opcode == 0b010;
                let (negative, raw_int) = if signed {
                    if sf == 1 {
                        let x = self.get_x(rn) as i64;
                        (x < 0, (x as i128).unsigned_abs())
                    } else {
                        let x = self.get_w(rn) as i32;
                        (x < 0, (x as i128).unsigned_abs())
                    }
                } else if sf == 1 {
                    (false, self.get_x(rn) as u128)
                } else {
                    (false, self.get_w(rn) as u128)
                };
                let (r, status): (u64, u32) = match ptype {
                    0b00 => {
                        let r = scaled_int_to_fp32_bits_with_fpcr(
                            raw_int,
                            negative,
                            fbits as u32,
                            self.fpcr,
                        );
                        (
                            r as u64,
                            fp_status_scaled_int_to_fp(raw_int, fbits as u32, 4, r as u64),
                        )
                    }
                    0b01 => {
                        let r = scaled_int_to_fp64_bits_with_fpcr(
                            raw_int,
                            negative,
                            fbits as u32,
                            self.fpcr,
                        );
                        (r, fp_status_scaled_int_to_fp(raw_int, fbits as u32, 8, r))
                    }
                    0b11 => {
                        let raw_r = scaled_int_to_fp16_bits_with_fpcr(
                            raw_int,
                            negative,
                            fbits as u32,
                            self.fpcr,
                        );
                        let status =
                            fp_status_scaled_int_to_fp(raw_int, fbits as u32, 2, raw_r as u64);
                        let (r, status) = fp16_int_to_fp_output_status_with_fpcr(
                            raw_int, raw_r, status, self.fpcr,
                        );
                        (r as u64, status)
                    }
                    _ => return Err(ArmError::UndefinedInstruction(insn)),
                };
                self.fpsr |= status;
                self.v[rd as usize] = r as u128;
                return Ok(CpuExit::Continue);
            }
            // FP -> GPR int, truncating toward zero: int = sat(trunc(FP * 2^fbits)).
            let signed = (opcode & 1) == 0;
            let input_status = match ptype {
                0b00 => fp_fz_input_status(4, self.v[rn as usize] as u64, self.fpcr),
                0b01 => fp_fz_input_status(8, self.v[rn as usize] as u64, self.fpcr),
                _ => 0,
            };
            let fval: f64 = match ptype {
                0b00 => f32::from_bits(fp32_flush_input_with_fpcr(
                    self.v[rn as usize] as u32,
                    self.fpcr,
                )) as f64,
                0b01 => f64::from_bits(fp64_flush_input_with_fpcr(
                    self.v[rn as usize] as u64,
                    self.fpcr,
                )),
                0b11 => fp16_to_f64(fp16_flush_input_with_fpcr(
                    self.v[rn as usize] as u16,
                    self.fpcr,
                )),
                _ => return Err(ArmError::UndefinedInstruction(insn)),
            };
            let scaled = fval * 2f64.powi(fbits);
            self.fpsr |= fp_status_merge_input_status(
                fp_to_int_status(scaled, signed, if sf == 1 { 64 } else { 32 }),
                input_status,
                self.fpcr,
            );
            let res: u64 = match (sf == 1, signed) {
                (true, true) => scaled as i64 as u64,
                (true, false) => scaled as u64,
                (false, true) => (scaled as i32) as u32 as u64,
                (false, false) => (scaled as u32) as u64,
            };
            if sf == 1 {
                self.set_x(rd, res);
            } else {
                self.set_w(rd, res as u32);
            }
            return Ok(CpuExit::Continue);
        }

        // Conversion between floating-point and integer (scalar, GPR-involved):
        // sf 0 0 11110 ptype 1 rmode opcode 000000 Rn Rd. rmode=bits[20:19],
        // opcode=bits[18:16]. FMOV (opcode 11x) is handled above; here are the
        // FP<->int conversions: SCVTF/UCVTF (opcode 010/011, int->FP) and
        // FCVTNS/NU/PS/PU/MS/MU/ZS/ZU/AS/AU (FP->int, rounding from rmode, or
        // ties-away for opcode 100/101).
        if (insn >> 24) & 0x7F == 0b0011110 && (insn >> 21) & 1 == 1 && (insn >> 10) & 0x3F == 0 {
            let sf = (insn >> 31) & 1;
            let ptype = (insn >> 22) & 0x3;
            let rmode = (insn >> 19) & 0x3;
            let opcode = (insn >> 16) & 0x7;
            let rn = ((insn >> 5) & 0x1F) as u8;
            let rd = (insn & 0x1F) as u8;
            if opcode >= 0b110 {
                // FMOV (general) is handled earlier; anything else is unallocated.
                return Err(ArmError::UndefinedInstruction(insn));
            }
            if opcode == 0b010 || opcode == 0b011 {
                if rmode != 0 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                // SCVTF / UCVTF: integer (GPR) -> floating-point.
                let signed = opcode == 0b010;
                let iv = if sf == 1 {
                    self.get_x(rn)
                } else {
                    self.get_w(rn) as u64
                };
                let (negative, raw_int) = if signed {
                    if sf == 1 {
                        let x = iv as i64;
                        (x < 0, (x as i128).unsigned_abs())
                    } else {
                        let x = iv as u32 as i32;
                        (x < 0, (x as i128).unsigned_abs())
                    }
                } else if sf == 1 {
                    (false, iv as u128)
                } else {
                    (false, (iv as u32) as u128)
                };
                let (r, status): (u64, u32) = match ptype {
                    0b00 => {
                        let r = int_to_fp32_bits_with_fpcr(raw_int, negative, self.fpcr);
                        (r as u64, fp_status_int_to_fp_scaled(raw_int, 4, r as u64))
                    }
                    0b01 => {
                        let r = int_to_fp64_bits_with_fpcr(raw_int, negative, self.fpcr);
                        (r, fp_status_int_to_fp_scaled(raw_int, 8, r))
                    }
                    0b11 => {
                        let raw_r = int_to_fp16_bits_with_fpcr(raw_int, negative, self.fpcr);
                        let status = fp_status_int_to_fp_scaled(raw_int, 2, raw_r as u64);
                        let (r, status) = fp16_int_to_fp_output_status_with_fpcr(
                            raw_int, raw_r, status, self.fpcr,
                        );
                        (r as u64, status)
                    }
                    _ => return Err(ArmError::UndefinedInstruction(insn)),
                };
                self.fpsr |= status;
                self.v[rd as usize] = r as u128;
                return Ok(CpuExit::Continue);
            }
            if opcode >= 0b100 && rmode != 0 {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            // FP -> integer. signed = even opcode; rounding from rmode (or
            // ties-away for FCVTA* opcode 100/101).
            let signed = (opcode & 1) == 0;
            let input_status = match ptype {
                0b00 => fp_fz_input_status(4, self.v[rn as usize] as u64, self.fpcr),
                0b01 => fp_fz_input_status(8, self.v[rn as usize] as u64, self.fpcr),
                _ => 0,
            };
            let fval: f64 = match ptype {
                0b00 => f32::from_bits(fp32_flush_input_with_fpcr(
                    self.v[rn as usize] as u32,
                    self.fpcr,
                )) as f64,
                0b01 => f64::from_bits(fp64_flush_input_with_fpcr(
                    self.v[rn as usize] as u64,
                    self.fpcr,
                )),
                0b11 => fp16_to_f64(fp16_flush_input_with_fpcr(
                    self.v[rn as usize] as u16,
                    self.fpcr,
                )),
                _ => return Err(ArmError::UndefinedInstruction(insn)),
            };
            let rounded = if opcode >= 0b100 {
                fval.round() // FCVTAS/FCVTAU: round to nearest, ties away
            } else {
                match rmode {
                    0b00 => fval.round_ties_even(),
                    0b01 => fval.ceil(),
                    0b10 => fval.floor(),
                    _ => fval.trunc(),
                }
            };
            self.fpsr |= fp_status_merge_input_status(
                fp_to_int_rounded_status(fval, rounded, signed, if sf == 1 { 64 } else { 32 }),
                input_status,
                self.fpcr,
            );
            // Saturate into the sf-width signed/unsigned range; NaN -> 0 (Rust's
            // float-to-int `as` already truncates/saturates/maps NaN to 0, and the
            // input is already integral).
            let res: u64 = match (sf == 1, signed) {
                (true, true) => rounded as i64 as u64,
                (true, false) => rounded as u64,
                (false, true) => (rounded as i32) as u32 as u64,
                (false, false) => (rounded as u32) as u64,
            };
            if sf == 1 {
                self.set_x(rd, res);
            } else {
                self.set_w(rd, res as u32);
            }
            return Ok(CpuExit::Continue);
        }

        // (FADD/FADDP FP16 fall through to the unified three-same FP16 handler
        // below; the previous dedicated add handler rounded incorrectly.)

        // SM3/SM4 crypto (bits[31:24]=0xCE). This MUST precede every Advanced
        // SIMD dispatch below: 0xCE has bits[28:24]=01110 and bit22=1/bit10=1,
        // so e.g. SM3SS1 would otherwise be captured by the FP16 three-same
        // group and executed as FMLA.
        if (insn >> 24) & 0xFF == 0xCE {
            return self.exec_crypto(insn);
        }

        // Advanced SIMD copy (DUP element/general, INS element/general, SMOV,
        // UMOV). Identified by bits[23:21]==000 (bit22==0 distinguishes it from
        // the FP16 three-same group, which has bit22==1). Must precede FP16.
        // Encoding: 0_Q_op_01110000_imm5_0_imm4_1_Rn_Rd
        // op_bits 11110 (top 0x5E) is the scalar form: DUP <V><d>,<Vn>.<T>[i]
        // (a.k.a. MOV), handled inside exec_simd_copy.
        if matches!((insn >> 24) & 0x1F, 0b01110 | 0b11110)
            && (insn >> 21) & 0x7 == 0
            && (insn >> 15) & 1 == 0
            && (insn >> 10) & 1 == 1
        {
            return self.exec_simd_copy(insn);
        }

        // Advanced SIMD three-same FP16 (vector and scalar)
        // FP16 uses bit[21]=0 (unlike regular three-same which has bit[21]=1)
        // Various FP16 ops use different bits[23:22] values:
        //   - FADD/FSUB/etc: bits[23:22]=11
        //   - FDIV/FRECPS/FRSQRTS: bits[23:22]=01
        let op_bits = (insn >> 24) & 0x1F;
        if (op_bits == 0b01110 || op_bits == 0b11110)
            && (insn >> 22) & 1 == 1       // bit[22]=1 for FP16 three-same
            && (insn >> 21) & 1 == 0       // bit[21]=0 for FP16 three-same
            && (insn >> 14) & 0x3 == 0b00  // bits[15:14]=00 for FP16 three-same
            && (insn >> 10) & 1 == 1
        {
            return self.exec_simd_fp16_three_same(insn);
        }

        // Advanced SIMD three-same (vector and scalar)
        // Vector encoding: 0_Q_U_01110_size_1_Rm_opcode_1_Rn_Rd (bits[28:24]=01110)
        // Scalar encoding: 0_1_U_11110_size_1_Rm_opcode_1_Rn_Rd (bits[28:24]=11110)
        let op_bits = (insn >> 24) & 0x1F;
        if (op_bits == 0b01110 || op_bits == 0b11110)
            && (insn >> 21) & 1 == 1
            && (insn >> 10) & 1 == 1
        {
            return self.exec_simd_three_same(insn);
        }

        // BFCVTN/BFCVTN2 (FEAT_BF16): f32 -> bf16 narrowing. Same two-reg-misc
        // slot as FCVTN (opcode 10110) but selected by size==10 (FCVTN uses
        // size 0x). Intercept before the generic two-reg-misc handler.
        if op_bits == 0b01110
            && (insn >> 29) & 1 == 0
            && (insn >> 22) & 0x3 == 0b10
            && (insn >> 17) & 0x1F == 0b10000
            && (insn >> 12) & 0x1F == 0b10110
            && (insn >> 10) & 0x3 == 0b10
        {
            return self.exec_simd_bfcvtn(insn);
        }

        // Advanced SIMD two-reg misc (vector and scalar)
        // Vector encoding: 0_Q_U_01110_size_10000_opcode_10_Rn_Rd (bits[28:24]=01110)
        // Scalar encoding: 0_1_U_11110_size_10000_opcode_10_Rn_Rd (bits[28:24]=11110)
        if (op_bits == 0b01110 || op_bits == 0b11110)
            && (insn >> 17) & 0x1F == 0b10000
            && (insn >> 10) & 0x3 == 0b10
        {
            return self.exec_simd_two_reg(insn);
        }

        // Advanced SIMD two-reg misc FP16 (vector and scalar)
        // Encoding pattern: bits[21:19]=111 distinguishes FP16 from normal two-reg misc
        // Vector: 0_Q_U_01110_size_111_opcode_10_Rn_Rd
        // Scalar: 0_1_U_11110_size_111_opcode_10_Rn_Rd
        if (op_bits == 0b01110 || op_bits == 0b11110)
            && (insn >> 19) & 0x7 == 0b111  // FP16 distinguishing bits
            && (insn >> 10) & 0x3 == 0b10
        {
            return self.exec_simd_fp16_two_reg(insn);
        }

        // Scalar three-different: SQDMULL/SQDMLAL/SQDMLSL <Dd>,<Sn>,<Sm> (and the
        // S<-H form). Top 0x5E (op_bits==11110), bit21==1, bits[11:10]==00,
        // opcode 1101/1001/1011. Signed doubling widening multiply, then
        // (optionally) a saturating accumulate; one element, rest zeroed.
        if op_bits == 0b11110 && (insn >> 21) & 1 == 1 && (insn >> 10) & 0x3 == 0b00 {
            if (insn >> 31) != 0 {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            let size = (insn >> 22) & 0x3;
            let opcode = (insn >> 12) & 0xF;
            let (accum, subtract) = match opcode {
                0b1101 => (false, false), // SQDMULL
                0b1001 => (true, false),  // SQDMLAL
                0b1011 => (true, true),   // SQDMLSL
                _ => return Err(ArmError::UndefinedInstruction(insn)),
            };
            if size != 0b01 && size != 0b10 {
                return Err(ArmError::UndefinedInstruction(insn)); // src H or S only
            }
            let bits = 8u32 << size; // narrow source element size
            let dbits = 2 * bits;
            let rm = ((insn >> 16) & 0x1F) as usize;
            let rn = ((insn >> 5) & 0x1F) as usize;
            let rd = (insn & 0x1F) as usize;
            let nv = sext_elem(self.v[rn] as u64 & elem_mask(bits), bits);
            let mv = sext_elem(self.v[rm] as u64 & elem_mask(bits), bits);
            let dmin = -(1i128 << (dbits - 1));
            let dmax = (1i128 << (dbits - 1)) - 1;
            let raw_prod = 2 * nv * mv;
            let prod_saturated = raw_prod < dmin || raw_prod > dmax;
            let prod = raw_prod.clamp(dmin, dmax);
            let (r, acc_saturated) = if accum {
                let d = sext_elem(self.v[rd] as u64 & elem_mask(dbits), dbits);
                sat_signed_q(d + if subtract { -prod } else { prod }, dbits)
            } else {
                (prod as u64 & elem_mask(dbits), false)
            };
            if prod_saturated || acc_saturated {
                self.fpsr |= FPSR_QC;
            }
            self.v[rd] = (r as u128) & elem_mask_u128(dbits);
            return Ok(CpuExit::Continue);
        }

        // Advanced SIMD three different (disparate) - widening/narrowing operations
        // Encoding: 0_Q_U_01110_size_1_Rm_opcode_00_Rn_Rd
        // bits[28:24]=01110, bit[21]=1, bits[11:10]=00
        if op_bits == 0b01110 && (insn >> 21) & 1 == 1 && (insn >> 10) & 0x3 == 0b00 {
            return self.exec_simd_three_different(insn);
        }

        // SDOT/UDOT (FEAT_DotProd, bits[15:10]=100101) and USDOT (FEAT_I8MM,
        // U==0, bits[15:10]=100111): 8-bit -> 32-bit dot product, bit21==0.
        if op_bits == 0b01110 && (insn >> 21) & 1 == 0 {
            let lo6 = (insn >> 10) & 0x3F;
            if lo6 == 0b100101 {
                if (insn >> 22) & 0x3 != 0b10 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                let signed = (insn >> 29) & 1 == 0; // SDOT (U=0) / UDOT (U=1)
                return self.exec_simd_dot(insn, signed, signed);
            }
            if lo6 == 0b100111 && (insn >> 29) & 1 == 0 {
                if (insn >> 22) & 0x3 != 0b10 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                // USDOT: Vn unsigned, Vm signed.
                return self.exec_simd_dot(insn, false, true);
            }
        }

        // Three-same-extra (bit21==0): SQRDMLAH/SQRDMLSH (FEAT_RDM, bits[15:10]==
        // 100001/100011, vector + scalar) and SMMLA/UMMLA/USMMLA (FEAT_I8MM,
        // bits[15:10]==101001/101011, vector only).
        if (op_bits == 0b01110 || op_bits == 0b11110) && (insn >> 21) & 1 == 0 {
            let lo6 = (insn >> 10) & 0x3F;
            if matches!(lo6, 0b100001 | 0b100011 | 0b101001 | 0b101011) {
                return self.exec_simd_three_same_extra(insn);
            }
        }

        // FCMLA (vector): 0_Q_1_01110_size_0_Rm_110_rot_1_Rn_Rd
        //   bits[15:13]=110, bit10=1, rot=bits[12:11].
        // FCADD: 0_Q_1_01110_size_0_Rm_111_rot_01_Rn_Rd
        //   bits[15:13]=111, bits[11:10]=01, rot=bit12.
        if op_bits == 0b01110 && (insn >> 29) & 1 == 1 && (insn >> 21) & 1 == 0 {
            if (insn >> 13) & 0x7 == 0b110 && (insn >> 10) & 1 == 1 {
                return self.exec_simd_complex(insn, true);
            }
            if (insn >> 13) & 0x7 == 0b111 && (insn >> 10) & 0x3 == 0b01 {
                return self.exec_simd_complex(insn, false);
            }
            // BF16 three-same-extra: BFDOT/BFMLAL (bits[15:10]=111111) and
            // BFMMLA (bits[15:10]=111011), sub-selected by size bits[23:22].
            let lo6 = (insn >> 10) & 0x3F;
            let size = (insn >> 22) & 0x3;
            if lo6 == 0b111111 {
                if size == 0b01 {
                    return self.exec_simd_bfdot(insn, false); // BFDOT vector
                }
                if size == 0b11 {
                    return self.exec_simd_bfmlal(insn, false); // BFMLALB/T vector
                }
                return Err(ArmError::UndefinedInstruction(insn));
            }
            if lo6 == 0b111011 {
                if (insn >> 30) & 1 == 1 && size == 0b01 {
                    return self.exec_simd_bfmmla(insn); // BFMMLA
                }
                return Err(ArmError::UndefinedInstruction(insn));
            }
        }

        // Cryptographic AES/SHA operations
        // AES: 0100 1110 00 1 01000 0 opcode 10 Rn Rd (bits[31:24]=0x4E)
        // SHA two-reg: 0101 1110 00 1 01000 0 opcode 10 Rn Rd (bits[31:24]=0x5E)
        // The bits[21:17]==10100 marker distinguishes AES/SHA two-register crypto
        // from across-lanes (11000) and two-reg-misc (10000), which share the
        // same bits[31:24] for Q==1.
        if ((insn >> 24) & 0xFF == 0x4E || (insn >> 24) & 0xFF == 0x5E)
            && (insn >> 17) & 0x1F == 0b10100
            && (insn >> 10) & 0x3 == 0b10
        {
            return self.exec_crypto(insn);
        }

        // SHA/SM3/SM4 three-register operations
        // SHA three-reg: 0101 1110 000 Rm 0 opcode 00 Rn Rd (bits[31:24]=0x5E, bits[11:10]=00)
        // SM3/SM4: various encodings with bits[31:24]=0xCE
        if (insn >> 24) & 0xFF == 0x5E && (insn >> 21) & 7 == 0 && (insn >> 10) & 0x3 == 0b00 {
            return self.exec_crypto(insn);
        }

        // Advanced SIMD across lanes (reduction operations like ADDV, SADDLV, etc.)
        // Encoding: 0_Q_U_01110_size_11000_opcode_10_Rn_Rd
        if op_bits == 0b01110 && (insn >> 17) & 0x1F == 0b11000 && (insn >> 10) & 0x3 == 0b10 {
            return self.exec_simd_across_lanes(insn);
        }

        // AdvSIMD scalar pairwise (ADDP/FADDP/FMAXP/FMINP/FMAXNMP/FMINNMP to a
        // scalar): top 0x5E/0x7E, bits[21:17]==11000, bits[11:10]==10.
        if op_bits == 0b11110 && (insn >> 17) & 0x1F == 0b11000 && (insn >> 10) & 0x3 == 0b10 {
            return self.exec_simd_scalar_pairwise(insn);
        }

        // FCMLA by element: 0_Q_1_01111_size_L_M_Rm_0_rot_1_H_0_Rn_Rd. Must
        // precede the generic indexed dispatch below, since its opcode field
        // bits[15:12]=0_rot_1 overlaps FMLA/FMLS-by-element. Discriminated by
        // U==1, bit15==0, bit12==1, bit10==0.
        if op_bits == 0b01111
            && (insn >> 29) & 1 == 1
            && (insn >> 15) & 1 == 0
            && (insn >> 12) & 1 == 1
            && (insn >> 10) & 1 == 0
        {
            return self.exec_simd_complex_indexed(insn);
        }

        // U=0 by-element group with opcode bits[15:12]==1111, bit10==0: the
        // FEAT_I8MM / FEAT_BF16 by-element instructions, sub-selected by the
        // size field bits[23:22]: 00=SUDOT, 01=BFDOT, 10=USDOT, 11=BFMLALB/T.
        // Must precede the generic indexed dispatch below.
        if op_bits == 0b01111
            && (insn >> 29) & 1 == 0
            && (insn >> 12) & 0xF == 0b1111
            && (insn >> 10) & 1 == 0
        {
            match (insn >> 22) & 0x3 {
                0b00 => return self.exec_simd_dot_indexed_mixed(insn, true, false), // SUDOT: Vn signed, Vm unsigned
                0b10 => return self.exec_simd_dot_indexed_mixed(insn, false, true), // USDOT: Vn unsigned, Vm signed
                0b01 => return self.exec_simd_bfdot(insn, true), // BFDOT by element
                0b11 => return self.exec_simd_bfmlal(insn, true), // BFMLALB/T by element
                _ => {}
            }
        }

        // FEAT_FP8FMA FMLALB/FMLALT (by element): 0 Q 0 01111 11 idx Rm 0000
        // idx 0 Rn Rd. FP8 (E5M2: an exact f16 with the mantissa truncated to
        // 2 bits, so widening is a left shift) multiplied into f16 lanes;
        // Q selects bottom (0) / top (1) source bytes.
        if op_bits == 0b01111
            && (insn >> 29) & 1 == 0
            && (insn >> 22) & 0x3 == 0b11
            && (insn >> 12) & 0xF == 0b0000
            && (insn >> 10) & 1 == 0
        {
            if (insn >> 31) != 0 {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            if self.config.version < ArmVersion::V9_4A {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            let top = (insn >> 30) & 1 == 1;
            let rm = ((insn >> 16) & 0xF) as usize;
            let rn = ((insn >> 5) & 0x1F) as usize;
            let rd = (insn & 0x1F) as usize;
            let idx = ((((insn >> 11) & 1) << 3)
                | (((insn >> 21) & 1) << 2)
                | (((insn >> 20) & 1) << 1)
                | ((insn >> 10) & 1)) as usize;
            let f8_to_f16 = |b: u8| (b as u16) << 8;
            let n = self.v[rn].to_le_bytes();
            let m = self.v[rm].to_le_bytes();
            let mut d = self.v[rd].to_le_bytes();
            let mb = f8_to_f16(m[idx & 0xF]);
            for i in 0..8 {
                let nb = f8_to_f16(n[2 * i + usize::from(top)]);
                let prod = fp16_mul(nb, mb);
                let cur = u16::from_le_bytes([d[2 * i], d[2 * i + 1]]);
                let r = fp16_add(cur, prod);
                d[2 * i..2 * i + 2].copy_from_slice(&r.to_le_bytes());
            }
            self.v[rd] = u128::from_le_bytes(d);
            return Ok(CpuExit::Continue);
        }

        // FEAT_FHM FMLAL/FMLSL/FMLAL2/FMLSL2 by element: 0Q U 01111 10 L M Rm
        // (top:sub:00) H 0 Rn Rd. sz==10, bits[13:12]==00, and bit15==U (which
        // distinguishes them from the integer MUL/MLAL-by-element forms that
        // share bits[13:12]==00). Must precede the generic indexed dispatch.
        if op_bits == 0b01111
            && (insn >> 22) & 0x3 == 0b10
            && (insn >> 12) & 0x3 == 0
            && (insn >> 10) & 1 == 0
            && (insn >> 15) & 1 == (insn >> 29) & 1
        {
            return self.exec_fmlal(insn, true);
        }

        // Advanced SIMD vector x indexed element
        // Encoding: 0_Q_U_01111_size_L_M_Rm_opcode_H_0_Rn_Rd  (bit10 = 0)
        if (op_bits == 0b01111 || op_bits == 0b11111) && (insn >> 10) & 1 == 0 {
            return self.exec_simd_indexed(insn);
        }

        // Advanced SIMD modified immediate (MOVI/MVNI/ORR/BIC/FMOV vector)
        // Encoding: 0_Q_op_0111100000_abc_cmode_o2_1_defgh_Rd
        if (insn >> 19) & 0x3FF == 0b0111100000 && (insn >> 10) & 1 == 1 {
            return self.exec_simd_modified_imm(insn);
        }

        // Advanced SIMD shift by immediate (vector: bits[28:23]==011110; scalar:
        // bits[28:23]==111110). bit[10]==1. Both route to exec_simd_shift_imm,
        // which detects scalar via the top byte (0x5F/0x7F).
        if matches!((insn >> 23) & 0x3F, 0b011110 | 0b111110) && (insn >> 10) & 1 == 1 {
            return self.exec_simd_shift_imm(insn);
        }

        // Advanced SIMD permute (ZIP, UZP, TRN)
        // Encoding: 0_Q_0_01110_size_0_Rm_0_opcode_10_Rn_Rd
        if op_bits == 0b01110
            && (insn >> 29) & 1 == 0
            && (insn >> 21) & 1 == 0
            && (insn >> 15) & 1 == 0
            && (insn >> 10) & 0x3 == 0b10
        {
            return self.exec_simd_permute(insn);
        }

        // Advanced SIMD table lookup (TBL, TBX)
        // Encoding: 0_Q_0_01110_00_0_Rm_0_len_op_00_Rn_Rd
        if op_bits == 0b01110
            && (insn >> 29) & 1 == 0
            && (insn >> 22) & 0x3 == 0b00
            && (insn >> 21) & 1 == 0
            && (insn >> 10) & 0x3 == 0b00
        {
            return self.exec_simd_table(insn);
        }

        // Advanced SIMD extract (EXT)
        // Encoding: 0_Q_10_1110_00_0_Rm_0_imm4_0_Rn_Rd
        if op_bits == 0b01110
            && (insn >> 29) & 1 == 1
            && (insn >> 22) & 0x3 == 0b00
            && (insn >> 15) & 1 == 0
        {
            return self.exec_simd_extract(insn);
        }

        // If we get here, it's an unimplemented SIMD/FP instruction
        Err(ArmError::Unimplemented(format!(
            "SIMD/FP insn 0x{:08x}",
            insn
        )))
    }


    /// Execute SIMD FP add (binary uniform add).
    pub(crate) fn exec_simd_fp_add_uniform(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let q = (insn >> 30) & 1;
        let pair = ((insn >> 29) & 1) != 0;
        let size = (insn >> 22) & 0x3;
        let rm = ((insn >> 16) & 0x1F) as usize;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;

        let esize = if size & 1 == 0 { 4 } else { 8 };
        let datasize = if q == 1 { 16 } else { 8 };
        let elements = datasize / esize;

        let src1 = self.v[rn].to_le_bytes();
        let src2 = self.v[rm].to_le_bytes();
        let mut dst = [0u8; 16];
        let mut concat = [0u8; 32];

        if pair {
            concat[..datasize].copy_from_slice(&src1[..datasize]);
            concat[datasize..datasize * 2].copy_from_slice(&src2[..datasize]);
        }

        for e in 0..elements {
            let out_off = e * esize;
            if esize == 4 {
                let (a, b) = if pair {
                    let idx1 = 2 * e;
                    let idx2 = idx1 + 1;
                    let a_off = idx1 * esize;
                    let b_off = idx2 * esize;
                    (
                        f32::from_le_bytes([
                            concat[a_off],
                            concat[a_off + 1],
                            concat[a_off + 2],
                            concat[a_off + 3],
                        ]),
                        f32::from_le_bytes([
                            concat[b_off],
                            concat[b_off + 1],
                            concat[b_off + 2],
                            concat[b_off + 3],
                        ]),
                    )
                } else {
                    let a_off = e * esize;
                    let b_off = e * esize;
                    (
                        f32::from_le_bytes([
                            src1[a_off],
                            src1[a_off + 1],
                            src1[a_off + 2],
                            src1[a_off + 3],
                        ]),
                        f32::from_le_bytes([
                            src2[b_off],
                            src2[b_off + 1],
                            src2[b_off + 2],
                            src2[b_off + 3],
                        ]),
                    )
                };

                let result = a + b;
                let bytes = result.to_le_bytes();
                dst[out_off..out_off + 4].copy_from_slice(&bytes);
            } else {
                let (a, b) = if pair {
                    let idx1 = 2 * e;
                    let idx2 = idx1 + 1;
                    let a_off = idx1 * esize;
                    let b_off = idx2 * esize;
                    (
                        f64::from_le_bytes([
                            concat[a_off],
                            concat[a_off + 1],
                            concat[a_off + 2],
                            concat[a_off + 3],
                            concat[a_off + 4],
                            concat[a_off + 5],
                            concat[a_off + 6],
                            concat[a_off + 7],
                        ]),
                        f64::from_le_bytes([
                            concat[b_off],
                            concat[b_off + 1],
                            concat[b_off + 2],
                            concat[b_off + 3],
                            concat[b_off + 4],
                            concat[b_off + 5],
                            concat[b_off + 6],
                            concat[b_off + 7],
                        ]),
                    )
                } else {
                    let a_off = e * esize;
                    let b_off = e * esize;
                    (
                        f64::from_le_bytes([
                            src1[a_off],
                            src1[a_off + 1],
                            src1[a_off + 2],
                            src1[a_off + 3],
                            src1[a_off + 4],
                            src1[a_off + 5],
                            src1[a_off + 6],
                            src1[a_off + 7],
                        ]),
                        f64::from_le_bytes([
                            src2[b_off],
                            src2[b_off + 1],
                            src2[b_off + 2],
                            src2[b_off + 3],
                            src2[b_off + 4],
                            src2[b_off + 5],
                            src2[b_off + 6],
                            src2[b_off + 7],
                        ]),
                    )
                };

                let result = a + b;
                let bytes = result.to_le_bytes();
                dst[out_off..out_off + 8].copy_from_slice(&bytes);
            }
        }

        self.v[rd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }


    /// Execute SIMD FP16 three-same register instructions.
    pub(crate) fn exec_simd_fp16_three_same(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        if (insn >> 31) != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let q = (insn >> 30) & 1;
        let u = (insn >> 29) & 1;
        let a = (insn >> 23) & 1; // Selects between two groups of operations
        let rm = ((insn >> 16) & 0x1F) as usize;
        let opcode = (insn >> 11) & 0x7; // 3 bits for FP16
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;

        // For scalar (bit28=1) only the low halfword is processed.
        let is_scalar = ((insn >> 28) & 1) == 1;
        let datasize = if is_scalar {
            2
        } else if q == 1 {
            16
        } else {
            8
        };
        let elements = datasize / 2;

        // The scalar three-same FP16 group is a subset: only FMULX, FCMEQ,
        // FRECPS, FRSQRTS, FCMGE, FACGE, FABD, FCMGT and FACGT have scalar
        // encodings. The element-wise arithmetic (FADD/FSUB/FMUL/FMAX/FMIN/
        // FMAXNM/FMINNM/FDIV), the fused FMLA/FMLS and the pairwise forms do
        // not, so reject them in scalar context.
        if is_scalar
            && !matches!(
                (u, a, opcode),
                (0, 0, 0b011)
                    | (0, 0, 0b100)
                    | (0, 0, 0b111)
                    | (0, 1, 0b111)
                    | (1, 0, 0b100)
                    | (1, 0, 0b101)
                    | (1, 1, 0b010)
                    | (1, 1, 0b100)
                    | (1, 1, 0b101)
            )
        {
            return Ok(CpuExit::Undefined(insn));
        }

        // Classify the operation. `Bin` is a per-lane binary op; `Mla`/`Mls`
        // are the fused multiply-accumulate forms (they read the destination);
        // `Pair` is a pairwise-reduction op. See the Arm "Advanced SIMD
        // three-same (FP16)" table indexed by (U, a=bit23, opcode=bits[13:11]).
        enum Fp16Op {
            Bin(fn(u16, u16) -> u16),
            Pair(fn(u16, u16) -> u16),
            Mla,
            Mls,
        }
        let op = match (u, a, opcode) {
            // U=0
            (0, 0, 0b000) => Fp16Op::Bin(fp16_maxnm),
            (0, 1, 0b000) => Fp16Op::Bin(fp16_minnm),
            (0, 0, 0b001) => Fp16Op::Mla,
            (0, 1, 0b001) => Fp16Op::Mls,
            (0, 0, 0b010) => Fp16Op::Bin(fp16_add),
            (0, 1, 0b010) => Fp16Op::Bin(fp16_sub),
            (0, 0, 0b011) => Fp16Op::Bin(fp16_mulx),
            (0, 0, 0b100) => Fp16Op::Bin(|x, y| fp16_cmp(x, y, 0)), // FCMEQ
            (0, 0, 0b110) => Fp16Op::Bin(fp16_max),
            (0, 1, 0b110) => Fp16Op::Bin(fp16_min),
            (0, 0, 0b111) => Fp16Op::Bin(fp16_recps),
            (0, 1, 0b111) => Fp16Op::Bin(fp16_rsqrts),
            // U=1
            (1, 0, 0b000) => Fp16Op::Pair(fp16_maxnm),
            (1, 1, 0b000) => Fp16Op::Pair(fp16_minnm),
            (1, 0, 0b010) => Fp16Op::Pair(fp16_add),
            (1, 1, 0b010) => Fp16Op::Bin(fp16_abd),
            (1, 0, 0b011) => Fp16Op::Bin(fp16_mul),
            (1, 0, 0b100) => Fp16Op::Bin(|x, y| fp16_cmp(x, y, 1)), // FCMGE
            (1, 1, 0b100) => Fp16Op::Bin(|x, y| fp16_cmp(x, y, 2)), // FCMGT
            (1, 0, 0b101) => Fp16Op::Bin(|x, y| fp16_cmp(x, y, 3)), // FACGE
            (1, 1, 0b101) => Fp16Op::Bin(|x, y| fp16_cmp(x, y, 4)), // FACGT
            (1, 0, 0b110) => Fp16Op::Pair(fp16_max),
            (1, 1, 0b110) => Fp16Op::Pair(fp16_min),
            (1, 0, 0b111) => Fp16Op::Bin(fp16_div),
            _ => return Ok(CpuExit::Undefined(insn)),
        };

        let lane = |v: u128, e: usize| -> u16 { (v >> (e * 16)) as u16 };
        let src1 = self.v[rn];
        let src2 = self.v[rm];
        let acc = self.v[rd];
        let mut dst = 0u128;

        match op {
            Fp16Op::Bin(f) => {
                for e in 0..elements {
                    let n = lane(src1, e);
                    let m = lane(src2, e);
                    let r = match (u, a, opcode) {
                        (0, 0, 0b010) => sve_fp16_binop_with_fpcr(FpKind::Add, n, m, self.fpcr),
                        (0, 1, 0b010) => sve_fp16_binop_with_fpcr(FpKind::Sub, n, m, self.fpcr),
                        (1, 1, 0b010) => sve_fp16_binop_with_fpcr(FpKind::Abd, n, m, self.fpcr),
                        (1, 0, 0b011) => sve_fp16_binop_with_fpcr(FpKind::Mul, n, m, self.fpcr),
                        (1, 0, 0b111) => sve_fp16_binop_with_fpcr(FpKind::Div, n, m, self.fpcr),
                        (0, 0, 0b111) => fp16_recps_with_fpcr(n, m, self.fpcr),
                        (0, 1, 0b111) => fp16_rsqrts_with_fpcr(n, m, self.fpcr),
                        (0, 0, 0b011) => sve_fp16_binop_with_fpcr(FpKind::Mulx, n, m, self.fpcr),
                        (0, 0, 0b100) => fp16_cmp_with_fpcr(n, m, 0, self.fpcr),
                        (1, 0, 0b100) => fp16_cmp_with_fpcr(n, m, 1, self.fpcr),
                        (1, 1, 0b100) => fp16_cmp_with_fpcr(n, m, 2, self.fpcr),
                        (1, 0, 0b101) => fp16_cmp_with_fpcr(n, m, 3, self.fpcr),
                        (1, 1, 0b101) => fp16_cmp_with_fpcr(n, m, 4, self.fpcr),
                        (0, 0, 0b000) => sve_fp16_binop_with_fpcr(FpKind::MaxNm, n, m, self.fpcr),
                        (0, 1, 0b000) => sve_fp16_binop_with_fpcr(FpKind::MinNm, n, m, self.fpcr),
                        (0, 0, 0b110) => sve_fp16_binop_with_fpcr(FpKind::Max, n, m, self.fpcr),
                        (0, 1, 0b110) => sve_fp16_binop_with_fpcr(FpKind::Min, n, m, self.fpcr),
                        _ => f(n, m),
                    };
                    self.fpsr |= fp16_three_same_status_with_fpcr(u, a, opcode, n, m, r, self.fpcr);
                    dst |= (r as u128) << (e * 16);
                }
            }
            Fp16Op::Mla => {
                for e in 0..elements {
                    let aa = lane(acc, e);
                    let n = lane(src1, e);
                    let m = lane(src2, e);
                    let r = fp_muladd_bits_with_fpcr(aa as u64, n as u64, m as u64, 16, self.fpcr)
                        as u16;
                    self.fpsr |= fp_status_fma_with_fpcr(
                        2, aa as u64, n as u64, m as u64, r as u64, self.fpcr,
                    );
                    dst |= (r as u128) << (e * 16);
                }
            }
            Fp16Op::Mls => {
                for e in 0..elements {
                    let aa = lane(acc, e);
                    let n = lane(src1, e);
                    let m = lane(src2, e);
                    let neg_n = fp_neg_bits_with_fpcr(n as u64, 16, self.fpcr);
                    let r =
                        fp_muladd_bits_with_fpcr(aa as u64, neg_n, m as u64, 16, self.fpcr) as u16;
                    self.fpsr |=
                        fp_status_fma_with_fpcr(2, aa as u64, neg_n, m as u64, r as u64, self.fpcr);
                    dst |= (r as u128) << (e * 16);
                }
            }
            Fp16Op::Pair(f) => {
                // Pairwise: the lower half of the result comes from adjacent
                // pairs of Vn, the upper half from adjacent pairs of Vm.
                let pairs = elements / 2;
                for i in 0..pairs {
                    let n = lane(src1, 2 * i);
                    let m = lane(src1, 2 * i + 1);
                    let r = match (u, a, opcode) {
                        (1, 0, 0b010) => sve_fp16_binop_with_fpcr(FpKind::Add, n, m, self.fpcr),
                        (1, 0, 0b000) => sve_fp_pairwise_reduce_combine_with_fpcr(
                            FpKind::MaxNmp,
                            2,
                            n as u64,
                            m as u64,
                            self.fpcr,
                        ) as u16,
                        (1, 1, 0b000) => sve_fp_pairwise_reduce_combine_with_fpcr(
                            FpKind::MinNmp,
                            2,
                            n as u64,
                            m as u64,
                            self.fpcr,
                        ) as u16,
                        (1, 0, 0b110) => sve_fp_pairwise_reduce_combine_with_fpcr(
                            FpKind::Maxp,
                            2,
                            n as u64,
                            m as u64,
                            self.fpcr,
                        ) as u16,
                        (1, 1, 0b110) => sve_fp_pairwise_reduce_combine_with_fpcr(
                            FpKind::Minp,
                            2,
                            n as u64,
                            m as u64,
                            self.fpcr,
                        ) as u16,
                        _ => f(n, m),
                    };
                    self.fpsr |= fp16_three_same_status_with_fpcr(u, a, opcode, n, m, r, self.fpcr);
                    dst |= (r as u128) << (i * 16);
                }
                for i in 0..pairs {
                    let n = lane(src2, 2 * i);
                    let m = lane(src2, 2 * i + 1);
                    let r = match (u, a, opcode) {
                        (1, 0, 0b010) => sve_fp16_binop_with_fpcr(FpKind::Add, n, m, self.fpcr),
                        (1, 0, 0b000) => sve_fp_pairwise_reduce_combine_with_fpcr(
                            FpKind::MaxNmp,
                            2,
                            n as u64,
                            m as u64,
                            self.fpcr,
                        ) as u16,
                        (1, 1, 0b000) => sve_fp_pairwise_reduce_combine_with_fpcr(
                            FpKind::MinNmp,
                            2,
                            n as u64,
                            m as u64,
                            self.fpcr,
                        ) as u16,
                        (1, 0, 0b110) => sve_fp_pairwise_reduce_combine_with_fpcr(
                            FpKind::Maxp,
                            2,
                            n as u64,
                            m as u64,
                            self.fpcr,
                        ) as u16,
                        (1, 1, 0b110) => sve_fp_pairwise_reduce_combine_with_fpcr(
                            FpKind::Minp,
                            2,
                            n as u64,
                            m as u64,
                            self.fpcr,
                        ) as u16,
                        _ => f(n, m),
                    };
                    self.fpsr |= fp16_three_same_status_with_fpcr(u, a, opcode, n, m, r, self.fpcr);
                    dst |= (r as u128) << ((pairs + i) * 16);
                }
            }
        }

        self.v[rd] = dst;
        Ok(CpuExit::Continue)
    }


    /// Execute SIMD FP16 two-reg misc instructions.
    pub(crate) fn exec_simd_fp16_two_reg(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        if (insn >> 31) != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let q = (insn >> 30) & 1;
        let u = (insn >> 29) & 1;
        let a = (insn >> 23) & 1; // bit23 sub-group selector (the FP16 "sz" low bit)
        let opcode = (insn >> 12) & 0x1F;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;

        // For scalar, bit[28]=1
        let is_scalar = ((insn >> 28) & 1) == 1;
        let datasize = if is_scalar {
            2
        } else if q == 1 {
            16
        } else {
            8
        };
        let elements = datasize / 2;

        // Validity: FABS/FNEG (01111), FRINT* (11000/11001) and the vector FSQRT
        // (U=1, 11111) have no SIMD-scalar encoding — their scalar variants live
        // in the floating-point data-processing groups. FRECPX (U=0, 11111) is
        // scalar-only and has no vector form. Reject the mismatched cases.
        if is_scalar {
            if opcode == 0b01111
                || opcode == 0b11000
                || opcode == 0b11001
                || (opcode == 0b11111 && u == 1)
            {
                return Ok(CpuExit::Undefined(insn));
            }
        } else if opcode == 0b11111 && u == 0 {
            return Ok(CpuExit::Undefined(insn));
        }

        let lane = |v: u128, e: usize| -> u16 { (v >> (e * 16)) as u16 };
        let src = self.v[rn];
        let mut dst = 0u128;

        for e in 0..elements {
            let s = lane(src, e);
            // Decode by (U, a=bit23, opcode=bits[16:12]) per the Arm "Advanced
            // SIMD two-register miscellaneous (FP16)" table. FCVT* produce a
            // 16-bit integer lane; SCVTF/UCVTF consume one; the rest are FP16.
            let r: u16 = match (u, a, opcode) {
                // Sign manipulation.
                (0, 1, 0b01111) => fp_abs_bits_with_fpcr(s as u64, 16, self.fpcr) as u16, // FABS
                (1, 1, 0b01111) => fp_neg_bits_with_fpcr(s as u64, 16, self.fpcr) as u16, // FNEG
                // Square root and reciprocal-family estimates.
                (1, 1, 0b11111) => fp16_sqrt_with_fpcr(s, self.fpcr), // FSQRT
                (0, 1, 0b11111) => fp16_recpx(fp16_flush_input_with_fpcr(s, self.fpcr)), // FRECPX
                (0, 1, 0b11101) => {
                    let raw =
                        fp16_recpe(fp_estimate_input_with_fpcr(s as u64, 16, self.fpcr) as u16);
                    fp16_flush_output_with_fpcr(raw, self.fpcr)
                } // FRECPE
                (1, 1, 0b11101) => {
                    let raw = fp16_rsqrte_with_fpcr(
                        fp_estimate_input_with_fpcr(s as u64, 16, self.fpcr) as u16,
                        self.fpcr,
                    );
                    fp16_flush_output_with_fpcr(raw, self.fpcr)
                } // FRSQRTE
                // Compare against zero.
                (0, 1, 0b01100) => fp16_cmp0_with_fpcr(s, 0, self.fpcr), // FCMGT #0
                (0, 1, 0b01101) => fp16_cmp0_with_fpcr(s, 2, self.fpcr), // FCMEQ #0
                (0, 1, 0b01110) => fp16_cmp0_with_fpcr(s, 4, self.fpcr), // FCMLT #0
                (1, 1, 0b01100) => fp16_cmp0_with_fpcr(s, 1, self.fpcr), // FCMGE #0
                (1, 1, 0b01101) => fp16_cmp0_with_fpcr(s, 3, self.fpcr), // FCMLE #0
                // Round to integral.
                (0, 0, 0b11000) => fp16_frint_fixed_with_fpcr(s, 0, self.fpcr), // FRINTN
                (0, 0, 0b11001) => fp16_frint_fixed_with_fpcr(s, 1, self.fpcr), // FRINTM
                (0, 1, 0b11000) => fp16_frint_fixed_with_fpcr(s, 2, self.fpcr), // FRINTP
                (0, 1, 0b11001) => fp16_frint_fixed_with_fpcr(s, 3, self.fpcr), // FRINTZ
                (1, 0, 0b11000) => fp16_frint_fixed_with_fpcr(s, 4, self.fpcr), // FRINTA
                (1, 0, 0b11001) => fp16_frint_with_fpcr(s, self.fpcr),          // FRINTX
                (1, 1, 0b11001) => fp16_frint_with_fpcr(s, self.fpcr),          // FRINTI
                // Floating-point to integer (signed).
                (0, 0, 0b11010) => fp16_to_int16(fp16_flush_input_with_fpcr(s, self.fpcr), true, 0), // FCVTNS
                (0, 0, 0b11011) => fp16_to_int16(fp16_flush_input_with_fpcr(s, self.fpcr), true, 1), // FCVTMS
                (0, 0, 0b11100) => fp16_to_int16(fp16_flush_input_with_fpcr(s, self.fpcr), true, 4), // FCVTAS
                (0, 1, 0b11010) => fp16_to_int16(fp16_flush_input_with_fpcr(s, self.fpcr), true, 2), // FCVTPS
                (0, 1, 0b11011) => fp16_to_int16(fp16_flush_input_with_fpcr(s, self.fpcr), true, 3), // FCVTZS
                // Floating-point to integer (unsigned).
                (1, 0, 0b11010) => {
                    fp16_to_int16(fp16_flush_input_with_fpcr(s, self.fpcr), false, 0)
                } // FCVTNU
                (1, 0, 0b11011) => {
                    fp16_to_int16(fp16_flush_input_with_fpcr(s, self.fpcr), false, 1)
                } // FCVTMU
                (1, 0, 0b11100) => {
                    fp16_to_int16(fp16_flush_input_with_fpcr(s, self.fpcr), false, 4)
                } // FCVTAU
                (1, 1, 0b11010) => {
                    fp16_to_int16(fp16_flush_input_with_fpcr(s, self.fpcr), false, 2)
                } // FCVTPU
                (1, 1, 0b11011) => {
                    fp16_to_int16(fp16_flush_input_with_fpcr(s, self.fpcr), false, 3)
                } // FCVTZU
                // Integer to floating-point.
                (0, 0, 0b11101) => {
                    let x = s as i16;
                    let raw =
                        int_to_fp16_bits_with_fpcr((x as i128).unsigned_abs(), x < 0, self.fpcr);
                    fp16_flush_output_with_fpcr(raw, self.fpcr)
                } // SCVTF
                (1, 0, 0b11101) => {
                    let raw = int_to_fp16_bits_with_fpcr(s as u128, false, self.fpcr);
                    fp16_flush_output_with_fpcr(raw, self.fpcr)
                } // UCVTF
                _ => return Ok(CpuExit::Undefined(insn)),
            };
            self.fpsr |= fp16_two_reg_status_with_fpcr(u, a, opcode, s, r, self.fpcr);
            dst |= (r as u128) << (e * 16);
        }

        self.v[rd] = dst;
        Ok(CpuExit::Continue)
    }


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


    /// Execute the AdvSIMD "three-same-extra" (bit21==0) ops: SQRDMLAH/SQRDMLSH
    /// (FEAT_RDM; vector + scalar) and SMMLA/UMMLA/USMMLA (FEAT_I8MM int8 2x2
    /// matrix multiply-accumulate; .4s,.16b,.16b, Q==1 only).
    pub(crate) fn exec_simd_three_same_extra(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        if (insn >> 31) & 1 != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let op_bits = (insn >> 24) & 0x1F;
        let scalar = op_bits == 0b11110;
        let q = (insn >> 30) & 1;
        let u = (insn >> 29) & 1;
        let size = (insn >> 22) & 0x3;
        let lo6 = (insn >> 10) & 0x3F;
        let rm = ((insn >> 16) & 0x1F) as usize;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;

        if lo6 == 0b101001 || lo6 == 0b101011 {
            // SMMLA(U=0)/UMMLA(U=1)/USMMLA(U=0,101011) int8 2x2 matrix MAC.
            if scalar || q == 0 || size != 0b10 {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            let (n_signed, m_signed) = if lo6 == 0b101011 {
                (false, true) // USMMLA: Vn unsigned, Vm signed
            } else if u == 1 {
                (false, false) // UMMLA
            } else {
                (true, true) // SMMLA
            };
            let n = self.v[rn].to_le_bytes();
            let m = self.v[rm].to_le_bytes();
            let a = self.v[rd];
            let mut res = 0u128;
            for i in 0..2 {
                for j in 0..2 {
                    let mut acc = (a >> ((i * 2 + j) * 32)) as u32 as i32 as i64;
                    for k in 0..8 {
                        let nv = if n_signed {
                            n[i * 8 + k] as i8 as i64
                        } else {
                            n[i * 8 + k] as i64
                        };
                        let mv = if m_signed {
                            m[j * 8 + k] as i8 as i64
                        } else {
                            m[j * 8 + k] as i64
                        };
                        acc += nv * mv;
                    }
                    res |= (acc as u32 as u128) << ((i * 2 + j) * 32);
                }
            }
            self.v[rd] = res;
            return Ok(CpuExit::Continue);
        }

        // SQRDMLAH (100001) / SQRDMLSH (100011): U==1, 16- or 32-bit elements.
        if u != 1 || size == 0b00 || size == 0b11 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let sub = lo6 == 0b100011;
        let bits = 8u32 << size;
        let esize = (bits / 8) as usize;
        let elements = if scalar {
            1
        } else if q == 1 {
            16 / esize
        } else {
            8 / esize
        };
        let n = self.v[rn].to_le_bytes();
        let m = self.v[rm].to_le_bytes();
        let a = self.v[rd].to_le_bytes();
        let mut dst = [0u8; 16];
        for e in 0..elements {
            let off = e * esize;
            let prod = sext_elem(read_elem(&n, off, esize), bits)
                * sext_elem(read_elem(&m, off, esize), bits);
            let prod = if sub { -prod } else { prod };
            let rounded = (prod * 2 + (1i128 << (bits - 1))) >> bits;
            let acc = sext_elem(read_elem(&a, off, esize), bits);
            let (r, saturated) = sat_signed_q(acc + rounded, bits);
            if saturated {
                self.fpsr |= FPSR_QC;
            }
            write_elem(&mut dst, off, esize, r);
        }
        self.v[rd] = u128::from_le_bytes(dst);
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


    /// Execute cryptographic operations (AES, SHA, SM3, SM4).
    /// For now, this is a stub that allows the instruction to execute.
    pub(crate) fn exec_crypto(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;

        // SHA512 (FEAT_SHA512) and SHA3 (FEAT_SHA3) 64-bit-lane crypto: 0xCE.
        if (insn >> 24) & 0xFF == 0xCE {
            let rm = ((insn >> 16) & 0x1F) as usize;
            let ra = ((insn >> 10) & 0x1F) as usize;
            let grp = (insn >> 21) & 0x7; // bits[23:21]
            let o = (insn >> 10) & 0x3F; // bits[15:10]
            let lanes = |v: u128| (v as u64, (v >> 64) as u64);
            let pack = |a: u64, b: u64| (a as u128) | ((b as u128) << 64);
            if grp == 0b011 && o == 0b100000 {
                // SHA512H
                let (d0, d1) = lanes(self.v[rd]);
                let (m0, m1) = lanes(self.v[rm]);
                let (n0, n1) = lanes(self.v[rn]);
                let s1 = |x: u64| x.rotate_right(14) ^ x.rotate_right(18) ^ x.rotate_right(41);
                let cho = |x: u64, y: u64, z: u64| (x & (y ^ z)) ^ z;
                let nd1 = d1.wrapping_add(s1(m1)).wrapping_add(cho(m1, n0, n1));
                let t = nd1.wrapping_add(m0);
                let nd0 = d0.wrapping_add(s1(t)).wrapping_add(cho(t, m1, n0));
                self.v[rd] = pack(nd0, nd1);
                return Ok(CpuExit::Continue);
            }
            if grp == 0b011 && o == 0b100001 {
                // SHA512H2
                let (d0, d1) = lanes(self.v[rd]);
                let (m0, m1) = lanes(self.v[rm]);
                let (n0, _n1) = lanes(self.v[rn]);
                let s0 = |x: u64| x.rotate_right(28) ^ x.rotate_right(34) ^ x.rotate_right(39);
                let maj = |x: u64, y: u64, z: u64| (x & y) | ((x | y) & z);
                let nd1 = d1.wrapping_add(s0(m0)).wrapping_add(maj(n0, m1, m0));
                let nd0 = d0.wrapping_add(s0(nd1)).wrapping_add(maj(nd1, m0, m1));
                self.v[rd] = pack(nd0, nd1);
                return Ok(CpuExit::Continue);
            }
            if grp == 0b110 && o == 0b100000 {
                // SHA512SU0
                let (d0, d1) = lanes(self.v[rd]);
                let (n0, _n1) = lanes(self.v[rn]);
                let sig0 = |x: u64| x.rotate_right(1) ^ x.rotate_right(8) ^ (x >> 7);
                self.v[rd] = pack(d0.wrapping_add(sig0(d1)), d1.wrapping_add(sig0(n0)));
                return Ok(CpuExit::Continue);
            }
            if grp == 0b011 && o == 0b100010 {
                // SHA512SU1
                let (d0, d1) = lanes(self.v[rd]);
                let (m0, m1) = lanes(self.v[rm]);
                let (n0, n1) = lanes(self.v[rn]);
                let sig1 = |x: u64| x.rotate_right(19) ^ x.rotate_right(61) ^ (x >> 6);
                self.v[rd] = pack(
                    d0.wrapping_add(sig1(n0)).wrapping_add(m0),
                    d1.wrapping_add(sig1(n1)).wrapping_add(m1),
                );
                return Ok(CpuExit::Continue);
            }
            if grp == 0b011 && o == 0b100011 {
                // RAX1: Vd[i] = Vn[i] ^ ROL64(Vm[i], 1)
                let (n0, n1) = lanes(self.v[rn]);
                let (m0, m1) = lanes(self.v[rm]);
                self.v[rd] = pack(n0 ^ m0.rotate_left(1), n1 ^ m1.rotate_left(1));
                return Ok(CpuExit::Continue);
            }
            if grp == 0b100 {
                // XAR: Vd[i] = ROR64(Vn[i] ^ Vm[i], imm6)
                let imm = o; // bits[15:10]
                let (n0, n1) = lanes(self.v[rn]);
                let (m0, m1) = lanes(self.v[rm]);
                self.v[rd] = pack((n0 ^ m0).rotate_right(imm), (n1 ^ m1).rotate_right(imm));
                return Ok(CpuExit::Continue);
            }
            if (grp == 0b000 || grp == 0b001) && (insn >> 15) & 1 == 0 {
                // EOR3 (grp 000): Vd = Vn ^ Vm ^ Va.  BCAX (grp 001): Vn ^ (Vm & ~Va).
                let (n, m, a) = (self.v[rn], self.v[rm], self.v[ra]);
                self.v[rd] = if grp == 0b000 {
                    n ^ m ^ a
                } else {
                    n ^ (m & !a)
                };
                return Ok(CpuExit::Continue);
            }
        }

        // AES single-block operations: bits[31:24]=0x4E, opcode bits[16:12].
        if (insn >> 24) & 0xFF == 0x4E {
            let opcode = (insn >> 12) & 0x1F;
            match opcode {
                0b00100 => {
                    // AESE: ShiftRows(SubBytes(Vd EOR Vn))
                    let st = self.v[rd] ^ self.v[rn];
                    self.v[rd] = aes_sub_bytes(aes_shift_rows(st, false), false);
                    return Ok(CpuExit::Continue);
                }
                0b00101 => {
                    // AESD: InvShiftRows then InvSubBytes of (Vd EOR Vn)
                    let st = self.v[rd] ^ self.v[rn];
                    self.v[rd] = aes_sub_bytes(aes_shift_rows(st, true), true);
                    return Ok(CpuExit::Continue);
                }
                0b00110 => {
                    // AESMC
                    self.v[rd] = aes_mix_columns(self.v[rn], false);
                    return Ok(CpuExit::Continue);
                }
                0b00111 => {
                    // AESIMC
                    self.v[rd] = aes_mix_columns(self.v[rn], true);
                    return Ok(CpuExit::Continue);
                }
                _ => {}
            }
        }

        let rm = ((insn >> 16) & 0x1F) as usize;

        // SHA-1 / SHA-256 (bits[31:24]=0x5E).
        if (insn >> 24) & 0xFF == 0x5E {
            // Two-register SHA: bits[21:17]==10100, opcode at bits[16:12].
            if (insn >> 17) & 0x1F == 0b10100 {
                let opcode = (insn >> 12) & 0x1F;
                match opcode {
                    0b00000 => {
                        // SHA1H Sd, Sn: ROL(Sn, 30) on the low 32 bits.
                        self.v[rd] = (self.v[rn] as u32).rotate_left(30) as u128;
                        return Ok(CpuExit::Continue);
                    }
                    0b00001 => {
                        // SHA1SU1 Vd.4S, Vn.4S
                        let op1 = self.v[rd];
                        let op2 = self.v[rn];
                        let t = op1 ^ (op2 >> 32);
                        let t0 = sha_elem(t, 0).rotate_left(1);
                        let t1 = sha_elem(t, 1).rotate_left(1);
                        let t2 = sha_elem(t, 2).rotate_left(1);
                        let t3 = sha_elem(t, 3).rotate_left(1) ^ sha_elem(t, 0).rotate_left(2);
                        let mut r = 0u128;
                        sha_set_elem(&mut r, 0, t0);
                        sha_set_elem(&mut r, 1, t1);
                        sha_set_elem(&mut r, 2, t2);
                        sha_set_elem(&mut r, 3, t3);
                        self.v[rd] = r;
                        return Ok(CpuExit::Continue);
                    }
                    0b00010 => {
                        // SHA256SU0 Vd.4S, Vn.4S
                        let x = self.v[rd];
                        let y = self.v[rn];
                        let t = (y << 96) | (x >> 32); // Y<31:0> : X<127:32>
                        let mut r = 0u128;
                        for e in 0..4 {
                            let elt = sha_elem(t, e);
                            let s = elt.rotate_right(7) ^ elt.rotate_right(18) ^ (elt >> 3);
                            sha_set_elem(&mut r, e, s.wrapping_add(sha_elem(x, e)));
                        }
                        self.v[rd] = r;
                        return Ok(CpuExit::Continue);
                    }
                    _ => {}
                }
            } else if (insn >> 21) & 7 == 0 && (insn >> 10) & 3 == 0 {
                // Three-register SHA: opcode at bits[14:12].
                let opcode = (insn >> 12) & 0x7;
                match opcode {
                    0b000 => {
                        // SHA1C
                        self.v[rd] =
                            sha1_hash(self.v[rd], self.v[rn] as u32, self.v[rm], sha_choose);
                        return Ok(CpuExit::Continue);
                    }
                    0b001 => {
                        // SHA1P
                        self.v[rd] =
                            sha1_hash(self.v[rd], self.v[rn] as u32, self.v[rm], sha_parity);
                        return Ok(CpuExit::Continue);
                    }
                    0b010 => {
                        // SHA1M
                        self.v[rd] =
                            sha1_hash(self.v[rd], self.v[rn] as u32, self.v[rm], sha_majority);
                        return Ok(CpuExit::Continue);
                    }
                    0b011 => {
                        // SHA1SU0 Vd.4S, Vn.4S, Vm.4S
                        let op1 = self.v[rd];
                        let op2 = self.v[rn];
                        let op3 = self.v[rm];
                        // result = (Vn<63:0> : Vd<127:64>) EOR Vd EOR Vm
                        let r = ((op2 << 64) | (op1 >> 64)) ^ op1 ^ op3;
                        self.v[rd] = r;
                        return Ok(CpuExit::Continue);
                    }
                    0b100 => {
                        // SHA256H Qd, Qn, Vm: SHA256hash(Vd, Vn, Vm, part1=true)
                        self.v[rd] = sha256_hash(self.v[rd], self.v[rn], self.v[rm], true);
                        return Ok(CpuExit::Continue);
                    }
                    0b101 => {
                        // SHA256H2 Qd, Qn, Vm: SHA256hash(Vn, Vd, Vm, part1=false)
                        self.v[rd] = sha256_hash(self.v[rn], self.v[rd], self.v[rm], false);
                        return Ok(CpuExit::Continue);
                    }
                    0b110 => {
                        // SHA256SU1 Vd.4S, Vn.4S, Vm.4S
                        let x = self.v[rd];
                        let y = self.v[rn];
                        let z = self.v[rm];
                        let t0 = (z << 96) | (y >> 32); // Z<31:0> : Y<127:32>
                        let mut r = 0u128;
                        // e = 0,1 use T1 = Z<127:64>
                        for e in 0..2 {
                            let elt = sha_elem(z >> 64, e); // Z<127:64> element e
                            let s = elt.rotate_right(17) ^ elt.rotate_right(19) ^ (elt >> 10);
                            let v = s.wrapping_add(sha_elem(x, e)).wrapping_add(sha_elem(t0, e));
                            sha_set_elem(&mut r, e, v);
                        }
                        // e = 2,3 use T1 = result<63:0>
                        for e in 2..4 {
                            let elt = sha_elem(r, e - 2); // result<63:0> element (e-2)
                            let s = elt.rotate_right(17) ^ elt.rotate_right(19) ^ (elt >> 10);
                            let v = s.wrapping_add(sha_elem(x, e)).wrapping_add(sha_elem(t0, e));
                            sha_set_elem(&mut r, e, v);
                        }
                        self.v[rd] = r;
                        return Ok(CpuExit::Continue);
                    }
                    _ => {}
                }
            }
        }

        // SM4 (bits[31:24]==0xCE).
        if (insn >> 24) & 0xFF == 0xCE {
            // SM4E Vd.4S, Vn.4S: 11001110 11000000 100001 Rn Rd.
            if (insn >> 16) & 0xFF == 0xC0 && (insn >> 10) & 0x3F == 0b100001 {
                self.v[rd] = sm4_rounds(self.v[rd], self.v[rn], true);
                return Ok(CpuExit::Continue);
            }
            // SM4EKEY Vd.4S, Vn.4S, Vm.4S: 11001110 011 Rm 110010 Rn Rd.
            if (insn >> 21) & 0x7 == 0b011 && (insn >> 10) & 0x3F == 0b110010 {
                self.v[rd] = sm4_rounds(self.v[rn], self.v[rm], false);
                return Ok(CpuExit::Continue);
            }

            // SM3 group.
            let grp = (insn >> 21) & 0x7;
            if grp == 0b010 {
                if (insn >> 15) & 1 == 0 {
                    // SM3SS1 Vd.4S, Vn.4S, Vm.4S, Va.4S (Va = Ra at bits[14:10]).
                    let ra = ((insn >> 10) & 0x1F) as usize;
                    let t = (self.v[rn] >> 96) as u32;
                    let val = t
                        .rotate_left(12)
                        .wrapping_add((self.v[rm] >> 96) as u32)
                        .wrapping_add((self.v[ra] >> 96) as u32)
                        .rotate_left(7);
                    self.v[rd] = (val as u128) << 96;
                    return Ok(CpuExit::Continue);
                } else if (insn >> 14) & 0x3 == 0b10 {
                    // SM3TT1A/SM3TT1B/SM3TT2A/SM3TT2B (sel = bits[11:10], i = imm2).
                    let i = (insn >> 12) & 0x3;
                    let sel = (insn >> 10) & 0x3;
                    self.v[rd] = sm3_tt(self.v[rd], self.v[rn], self.v[rm], i, sel);
                    return Ok(CpuExit::Continue);
                }
            } else if grp == 0b011 {
                if (insn >> 10) & 0x3F == 0b110000 {
                    self.v[rd] = sm3_partw1(self.v[rd], self.v[rn], self.v[rm]);
                    return Ok(CpuExit::Continue);
                }
                if (insn >> 10) & 0x3F == 0b110001 {
                    self.v[rd] = sm3_partw2(self.v[rd], self.v[rn], self.v[rm]);
                    return Ok(CpuExit::Continue);
                }
            }
        }

        // Any remaining crypto encoding is unallocated.
        Ok(CpuExit::Undefined(insn))
    }


    /// Execute SIMD across lanes (reduction operations).
    pub(crate) fn exec_simd_across_lanes(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        if (insn >> 31) & 1 != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let q = (insn >> 30) & 1;
        let u = (insn >> 29) & 1;
        let size = (insn >> 22) & 0x3;
        let opcode = (insn >> 12) & 0x1F;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;

        // ---- Floating-point reductions: FMAXNMV/FMINNMV (0b01100),
        //      FMAXV/FMINV (0b01111). U==1, f32 lanes only. bit23 picks min. ----
        if opcode == 0b01100 || opcode == 0b01111 {
            // FP max/min across lanes: f32 (U==1, 4S) or FP16 (U==0, .4h/.8h).
            // bit23 (size high) selects min; opcode 01100=NM variant. Reduced via
            // the ARM-correct combine (NaN propagation, sign-of-zero, sNaN quiet).
            let nm = opcode == 0b01100;
            let is_min = (size >> 1) & 1 == 1;
            let kind = match (nm, is_min) {
                (false, false) => FpKind::Max,
                (false, true) => FpKind::Min,
                (true, false) => FpKind::MaxNm,
                (true, true) => FpKind::MinNm,
            };
            let vn = self.v[rn];
            // ARM Reduce() is a recursive split-in-half tree (sve_fp_tree_reduce),
            // NOT a sequential fold — the order is observable when a NaN is
            // present (sNaN propagation / which numeric lane survives).
            let (esize, nlanes) = if u == 1 {
                if size & 1 != 0 || q == 0 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                (4usize, 4usize) // f32, 4S
            } else {
                (2usize, if q == 1 { 8 } else { 4 }) // FP16, .8h/.4h
            };
            let buf: Vec<u64> = (0..nlanes)
                .map(|e| (vn >> (e * esize * 8)) as u64 & elem_mask((esize * 8) as u32))
                .collect();
            let (r, status) = sve_fp_tree_reduce_status(&buf, kind, esize, self.fpcr);
            self.fpsr |= status;
            self.v[rd] = (r & elem_mask((esize * 8) as u32)) as u128;
            return Ok(CpuExit::Continue);
        }

        let bits = 8u32 << size;
        let esize = (bits / 8) as usize;
        let datasize = if q == 1 { 16 } else { 8 };
        let elements = datasize / esize;
        let src = self.v[rn].to_le_bytes();

        // Reductions are defined for 8B/16B/4H/8H and (Q==1) 4S; never 64-bit,
        // and 8B/4H also exclude the single-element degenerate cases.
        let valid_size = match size {
            0b00 => true,   // 8B / 16B
            0b01 => true,   // 4H / 8H
            0b10 => q == 1, // 4S only
            _ => false,
        };
        if !valid_size {
            return Err(ArmError::UndefinedInstruction(insn));
        }

        let (result, result_bits): (u64, u32) = match opcode {
            0b11011 => {
                // ADDV
                let mut acc = 0u64;
                for e in 0..elements {
                    acc = acc.wrapping_add(read_elem(&src, e * esize, esize));
                }
                (acc & elem_mask(bits), bits)
            }
            0b00011 => {
                // SADDLV (U=0) / UADDLV (U=1) -- widening sum across lanes.
                let mut acc = 0i128;
                for e in 0..elements {
                    let v = read_elem(&src, e * esize, esize);
                    acc += if u == 0 {
                        sext_elem(v, bits)
                    } else {
                        uext_elem(v, bits) as i128
                    };
                }
                ((acc as u64) & elem_mask(2 * bits), 2 * bits)
            }
            0b01010 => {
                // SMAXV (U=0) / UMAXV (U=1)
                let mut acc = read_elem(&src, 0, esize);
                for e in 1..elements {
                    let v = read_elem(&src, e * esize, esize);
                    acc = if u == 0 {
                        if sext_elem(v, bits) > sext_elem(acc, bits) {
                            v
                        } else {
                            acc
                        }
                    } else if uext_elem(v, bits) > uext_elem(acc, bits) {
                        v
                    } else {
                        acc
                    };
                }
                (acc & elem_mask(bits), bits)
            }
            0b11010 => {
                // SMINV (U=0) / UMINV (U=1)
                let mut acc = read_elem(&src, 0, esize);
                for e in 1..elements {
                    let v = read_elem(&src, e * esize, esize);
                    acc = if u == 0 {
                        if sext_elem(v, bits) < sext_elem(acc, bits) {
                            v
                        } else {
                            acc
                        }
                    } else if uext_elem(v, bits) < uext_elem(acc, bits) {
                        v
                    } else {
                        acc
                    };
                }
                (acc & elem_mask(bits), bits)
            }
            _ => return Err(ArmError::UndefinedInstruction(insn)),
        };

        self.v[rd] = (result as u128) & elem_mask_u128(result_bits);
        Ok(CpuExit::Continue)
    }


    /// AdvSIMD scalar pairwise: reduce the two elements of a vector to a scalar.
    /// ADDP (int, D only); FADDP/FMAXP/FMINP/FMAXNMP/FMINNMP for f16 (U=0),
    /// f32 (U=1, bit22=0) or f64 (U=1, bit22=1). bit23 selects min for the
    /// max/min forms. Writes lane 0, zeroing the rest.
    pub(crate) fn exec_simd_scalar_pairwise(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        if (insn >> 31) != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let u = (insn >> 29) & 1;
        let size = (insn >> 22) & 0x3;
        let opcode = (insn >> 12) & 0x1F;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;
        let vn = self.v[rn];

        if opcode == 0b11011 {
            // ADDP (scalar, .2d -> Dd).
            if u != 0 || size != 0b11 {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            self.v[rd] = (vn as u64).wrapping_add((vn >> 64) as u64) as u128;
            return Ok(CpuExit::Continue);
        }

        let (faddp, nm) = match opcode {
            0b01101 => (true, false),  // FADDP
            0b01100 => (false, true),  // FMAXNMP / FMINNMP
            0b01111 => (false, false), // FMAXP / FMINP
            _ => return Err(ArmError::UndefinedInstruction(insn)),
        };
        let min = (size >> 1) & 1 == 1;
        if u == 0 && (size & 1) != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let esize = if u == 0 {
            2usize // FP16
        } else if size & 1 == 0 {
            4 // f32
        } else {
            8 // f64
        };
        let kind = if faddp {
            FpKind::Add
        } else {
            match (nm, min) {
                (false, false) => FpKind::Max,
                (false, true) => FpKind::Min,
                (true, false) => FpKind::MaxNm,
                (true, true) => FpKind::MinNm,
            }
        };
        let mask = elem_mask((esize * 8) as u32);
        let e0 = vn as u64 & mask;
        let e1 = (vn >> (esize * 8)) as u64 & mask;
        let r = sve_fp_pairwise_reduce_combine_with_fpcr(kind, esize, e0, e1, self.fpcr);
        self.fpsr |= fp_pairwise_reduce_status_with_fpcr(esize, kind, e0, e1, r, self.fpcr);
        self.v[rd] = (r & mask) as u128;
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


    /// Execute SIMD shift by immediate.
    pub(crate) fn exec_simd_shift_imm(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        if (insn >> 31) & 1 != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let q = (insn >> 30) & 1;
        let u = (insn >> 29) & 1;
        let immh = (insn >> 19) & 0xF;
        let immb = (insn >> 16) & 0x7;
        let opcode = (insn >> 11) & 0x1F;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;
        // Scalar AdvSIMD shift-by-immediate has top byte 0x5F/0x7F (&0x1F==11111),
        // distinct from the scalar two-reg-misc class (0x5E/0x7E, 11110).
        let scalar = ((insn >> 24) & 0x1F) == 0b11111;

        // immh==0 belongs to the modified-immediate / other encoding.
        if immh == 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let size_idx = if immh & 0b1000 != 0 {
            3
        } else if immh & 0b0100 != 0 {
            2
        } else if immh & 0b0010 != 0 {
            1
        } else {
            0
        };
        let bits = 8u32 << size_idx; // element size the shift operates on
        let immhimmb = ((immh << 3) | immb) as u32;

        match opcode {
            // ---- Same element-size shifts ----
            0b00000 | 0b00010 | 0b00100 | 0b00110 | 0b01000 | 0b01010 | 0b01100 | 0b01110 => {
                // A few opcode slots are only allocated for one value of U.
                let valid = match opcode {
                    0b01000 => u == 1, // SRI (U==1 only)
                    0b01100 => u == 1, // SQSHLU (U==1 only)
                    _ => true,
                };
                if !valid {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                // 64-bit elements need 2D (Q==1) in the vector form.
                if bits == 64 && q == 0 && !scalar {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                if scalar
                    && bits != 64
                    && matches!(
                        opcode,
                        0b00000 | 0b00010 | 0b00100 | 0b00110 | 0b01000 | 0b01010
                    )
                {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                let is_left = matches!(opcode, 0b01010 | 0b01100 | 0b01110);
                let shift = if is_left {
                    immhimmb - bits
                } else {
                    2 * bits - immhimmb
                };
                let esize = (bits / 8) as usize;
                let datasize = if scalar {
                    esize
                } else if q == 1 {
                    16
                } else {
                    8
                };
                let elements = datasize / esize;
                let src = self.v[rn].to_le_bytes();
                let old = self.v[rd].to_le_bytes();
                let mut dst = [0u8; 16];
                for e in 0..elements {
                    let off = e * esize;
                    let a = read_elem(&src, off, esize);
                    let d = read_elem(&old, off, esize);
                    let (r, saturated) = adv_simd_shift_imm_elem(u, opcode, bits, shift, a, d);
                    if saturated {
                        self.fpsr |= FPSR_QC;
                    }
                    write_elem(&mut dst, off, esize, r);
                }
                self.v[rd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }
            // ---- Widening left shift: SSHLL / USHLL (SXTL/UXTL when shift==0) ----
            0b10100 => {
                if scalar {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                if bits == 64 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                let shift = immhimmb - bits;
                let esize = (bits / 8) as usize;
                let elements = 8 / esize; // source elements per 64-bit half
                let part = q as usize; // SSHLL2 uses the upper half of Vn
                let src = self.v[rn].to_le_bytes();
                let mut result: u128 = 0;
                for e in 0..elements {
                    let off = part * 8 + e * esize;
                    let a = read_elem(&src, off, esize);
                    let widened: u128 = if u == 0 {
                        ((sext_elem(a, bits) << shift) as u128) & elem_mask_u128(2 * bits)
                    } else {
                        (uext_elem(a, bits) << shift) & elem_mask_u128(2 * bits)
                    };
                    result |= widened << (e * 2 * bits as usize);
                }
                self.v[rd] = result;
                Ok(CpuExit::Continue)
            }
            // ---- Narrowing right shift ----
            0b10000 | 0b10001 | 0b10010 | 0b10011 => {
                if scalar && u == 0 && matches!(opcode, 0b10000 | 0b10001) {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                if bits == 64 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                let rounding = opcode == 0b10001 || opcode == 0b10011;
                let src_bits = 2 * bits;
                let shift = 2 * bits - immhimmb;
                let esize = (bits / 8) as usize;
                // Scalar narrowing shift (SQSHRN <Bd>,<Hn>,#imm etc.) writes one
                // element to lane 0, zeroing the rest; the vector "2" form fills
                // the upper 64 bits (part=1).
                let elements = if scalar { 1 } else { 8 / esize };
                let part = if scalar { 0 } else { q as usize };
                let vn = self.v[rn];
                let mut packed: u64 = 0;
                for e in 0..elements {
                    let s = ((vn >> (e * src_bits as usize)) & elem_mask_u128(src_bits)) as u64;
                    let (r, saturated): (u64, bool) = match (u, opcode) {
                        (0, 0b10000) | (0, 0b10001) => {
                            // SHRN / RSHRN: truncating narrow.
                            (
                                simd_rshift(s, shift, src_bits, false, rounding) & elem_mask(bits),
                                false,
                            )
                        }
                        (1, 0b10000) | (1, 0b10001) => {
                            // SQSHRUN / SQRSHRUN: signed source, unsigned saturate.
                            sat_unsigned_q(
                                simd_rshift_full(s, shift, src_bits, true, rounding),
                                bits,
                            )
                        }
                        (0, 0b10010) | (0, 0b10011) => {
                            // SQSHRN / SQRSHRN: signed source, signed saturate.
                            sat_signed_q(simd_rshift_full(s, shift, src_bits, true, rounding), bits)
                        }
                        _ => {
                            // UQSHRN / UQRSHRN: unsigned source, unsigned saturate.
                            sat_unsigned_q(
                                simd_rshift_full(s, shift, src_bits, false, rounding),
                                bits,
                            )
                        }
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
                Ok(CpuExit::Continue)
            }
            // ---- Fixed-point convert ----
            0b11100 | 0b11111 => {
                if size_idx < 1 {
                    return Err(ArmError::UndefinedInstruction(insn)); // 8-bit not defined
                }
                if bits == 64 && q == 0 && !scalar {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                let fbits = 2 * bits - immhimmb;
                let esize = (bits / 8) as usize;
                let datasize = if scalar {
                    esize
                } else if q == 1 {
                    16
                } else {
                    8
                };
                let elements = datasize / esize;
                let src = self.v[rn].to_le_bytes();
                let mut dst = [0u8; 16];
                for e in 0..elements {
                    let off = e * esize;
                    let a = read_elem(&src, off, esize);
                    let (r, status) = fixed_point_convert(opcode, u, bits, a, fbits, self.fpcr);
                    self.fpsr |= status;
                    write_elem(&mut dst, off, esize, r);
                }
                self.v[rd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }
            _ => Err(ArmError::UndefinedInstruction(insn)),
        }
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


    /// Execute the Advanced SIMD "copy" group: DUP (element/general), INS
    /// (element/general), SMOV, UMOV. Element size and lane index come from the
    /// `imm5` field (lowest set bit selects the size).
    pub(crate) fn exec_simd_copy(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        if (insn >> 31) & 1 != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let q = (insn >> 30) & 1;
        let op = (insn >> 29) & 1;
        let scalar = (insn >> 24) & 0x1F == 0b11110; // DUP <V><d>,<Vn>.<T>[i] (MOV)
        let imm5 = (insn >> 16) & 0x1F;
        let imm4 = (insn >> 11) & 0xF;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let rd = (insn & 0x1F) as u8;

        let size = if imm5 & 1 != 0 {
            0u32
        } else if imm5 & 2 != 0 {
            1
        } else if imm5 & 4 != 0 {
            2
        } else if imm5 & 8 != 0 {
            3
        } else {
            return Err(ArmError::UndefinedInstruction(insn));
        };
        let esize = 8u32 << size; // element size in bits
        let shift = esize as usize;
        let index = (imm5 >> (size + 1)) as usize;
        let emask = elem_mask_u128(esize);

        if scalar && (op != 0 || imm4 != 0b0000) {
            return Err(ArmError::UndefinedInstruction(insn));
        }

        if op == 1 {
            // INS (element): Vd[index] = Vn[src_index]. INS is a 128-bit-only
            // operation; the Q==0 encoding is unallocated and must trap.
            if q == 0 {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            let src_index = (imm4 >> size) as usize;
            let vn = self.v[rn as usize];
            let elem = (vn >> (src_index * shift)) & emask;
            let mut vd = self.v[rd as usize];
            vd &= !(emask << (index * shift));
            vd |= elem << (index * shift);
            self.v[rd as usize] = vd;
            return Ok(CpuExit::Continue);
        }

        match imm4 {
            0b0000 => {
                // DUP (element): broadcast Vn[index]. The scalar form (MOV
                // <V><d>,<Vn>.<T>[i]) extracts a single element into lane 0.
                if !scalar && size == 3 && q == 0 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                let vn = self.v[rn as usize];
                let elem = (vn >> (index * shift)) & emask;
                if scalar {
                    self.v[rd as usize] = elem;
                } else {
                    let datasize = if q == 1 { 128 } else { 64 };
                    let mut result = 0u128;
                    let mut p = 0;
                    while p < datasize {
                        result |= elem << p;
                        p += shift;
                    }
                    self.v[rd as usize] = result;
                }
            }
            0b0001 => {
                // DUP (general): broadcast Xn/Wn.
                if size == 3 && q == 0 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                let v = (self.get_x(rn) as u128) & emask;
                let datasize = if q == 1 { 128 } else { 64 };
                let mut result = 0u128;
                let mut p = 0;
                while p < datasize {
                    result |= v << p;
                    p += shift;
                }
                self.v[rd as usize] = result;
            }
            0b0011 => {
                // INS (general): Vd[index] = Xn/Wn. INS is 128-bit-only; the
                // Q==0 encoding is unallocated and must trap.
                if q == 0 {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                let v = (self.get_x(rn) as u128) & emask;
                let mut vd = self.v[rd as usize];
                vd &= !(emask << (index * shift));
                vd |= v << (index * shift);
                self.v[rd as usize] = vd;
            }
            0b0101 => {
                // SMOV: GPR = sign-extended Vn[index]. Valid: B/H -> W or X,
                // S -> X only; never D.
                if size == 3 || (size == 2 && q == 0) {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                let vn = self.v[rn as usize];
                let elem = ((vn >> (index * shift)) & emask) as u64;
                let signed = sext_elem(elem, esize) as u64;
                if q == 1 {
                    self.set_x(rd, signed);
                } else {
                    self.set_w(rd, signed as u32);
                }
            }
            0b0111 => {
                // UMOV: GPR = zero-extended Vn[index]. Valid: B/H/S -> W,
                // D -> X only.
                let valid = (size <= 2 && q == 0) || (size == 3 && q == 1);
                if !valid {
                    return Err(ArmError::UndefinedInstruction(insn));
                }
                let vn = self.v[rn as usize];
                let elem = ((vn >> (index * shift)) & emask) as u64;
                if q == 1 {
                    self.set_x(rd, elem);
                } else {
                    self.set_w(rd, elem as u32);
                }
            }
            _ => return Err(ArmError::UndefinedInstruction(insn)),
        }
        Ok(CpuExit::Continue)
    }


    /// Execute SIMD permute operations (ZIP, UZP, TRN).
    pub(crate) fn exec_simd_permute(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        if (insn >> 31) != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let q = (insn >> 30) & 1;
        let size = (insn >> 22) & 0x3;
        let rm = ((insn >> 16) & 0x1F) as usize;
        let opcode = (insn >> 12) & 0x7;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;

        // 64-bit elements need the 2D (Q==1) arrangement; "1D" is RESERVED.
        if size == 0b11 && q == 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }

        let esize = 1usize << size;
        let datasize = if q == 1 { 16 } else { 8 };
        let elements = datasize / esize;

        let src1 = self.v[rn].to_le_bytes();
        let src2 = self.v[rm].to_le_bytes();
        let mut dst = [0u8; 16];

        match opcode {
            0b001 => {
                // UZP1 - unzip, lower halves
                for e in 0..elements {
                    let src_idx = e * 2;
                    let dst_off = e * esize;
                    if src_idx < elements {
                        let src_off = src_idx * esize;
                        dst[dst_off..dst_off + esize]
                            .copy_from_slice(&src1[src_off..src_off + esize]);
                    } else {
                        let src_off = (src_idx - elements) * esize;
                        dst[dst_off..dst_off + esize]
                            .copy_from_slice(&src2[src_off..src_off + esize]);
                    }
                }
            }
            0b010 => {
                // TRN1 - transpose, lower halves
                for e in 0..(elements / 2) {
                    let dst_off1 = (e * 2) * esize;
                    let dst_off2 = (e * 2 + 1) * esize;
                    let src_off = (e * 2) * esize;
                    dst[dst_off1..dst_off1 + esize]
                        .copy_from_slice(&src1[src_off..src_off + esize]);
                    dst[dst_off2..dst_off2 + esize]
                        .copy_from_slice(&src2[src_off..src_off + esize]);
                }
            }
            0b011 => {
                // ZIP1 - zip, lower halves
                for e in 0..(elements / 2) {
                    let dst_off1 = (e * 2) * esize;
                    let dst_off2 = (e * 2 + 1) * esize;
                    let src_off = e * esize;
                    dst[dst_off1..dst_off1 + esize]
                        .copy_from_slice(&src1[src_off..src_off + esize]);
                    dst[dst_off2..dst_off2 + esize]
                        .copy_from_slice(&src2[src_off..src_off + esize]);
                }
            }
            0b101 => {
                // UZP2 - unzip, upper halves
                for e in 0..elements {
                    let src_idx = e * 2 + 1;
                    let dst_off = e * esize;
                    if src_idx < elements {
                        let src_off = src_idx * esize;
                        dst[dst_off..dst_off + esize]
                            .copy_from_slice(&src1[src_off..src_off + esize]);
                    } else {
                        let src_off = (src_idx - elements) * esize;
                        dst[dst_off..dst_off + esize]
                            .copy_from_slice(&src2[src_off..src_off + esize]);
                    }
                }
            }
            0b110 => {
                // TRN2 - transpose, upper halves
                for e in 0..(elements / 2) {
                    let dst_off1 = (e * 2) * esize;
                    let dst_off2 = (e * 2 + 1) * esize;
                    let src_off = (e * 2 + 1) * esize;
                    dst[dst_off1..dst_off1 + esize]
                        .copy_from_slice(&src1[src_off..src_off + esize]);
                    dst[dst_off2..dst_off2 + esize]
                        .copy_from_slice(&src2[src_off..src_off + esize]);
                }
            }
            0b111 => {
                // ZIP2 - zip, upper halves
                let half = elements / 2;
                for e in 0..half {
                    let dst_off1 = (e * 2) * esize;
                    let dst_off2 = (e * 2 + 1) * esize;
                    let src_off = (half + e) * esize;
                    dst[dst_off1..dst_off1 + esize]
                        .copy_from_slice(&src1[src_off..src_off + esize]);
                    dst[dst_off2..dst_off2 + esize]
                        .copy_from_slice(&src2[src_off..src_off + esize]);
                }
            }
            _ => return Ok(CpuExit::Undefined(insn)),
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


    /// Execute SIMD extract (EXT).
    pub(crate) fn exec_simd_extract(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        if (insn >> 31) != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let q = (insn >> 30) & 1;
        let rm = ((insn >> 16) & 0x1F) as usize;
        let imm4 = ((insn >> 11) & 0xF) as usize;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;

        let datasize = if q == 1 { 16 } else { 8 };

        // imm4 with bit 3 set is UNDEFINED for the 64-bit (Q==0) form.
        if q == 0 && imm4 >= 8 {
            return Err(ArmError::UndefinedInstruction(insn));
        }

        // Concatenate the low `datasize` bytes of Vn:Vm and extract `datasize`
        // bytes starting at byte `imm4`.
        let src1 = self.v[rn].to_le_bytes();
        let src2 = self.v[rm].to_le_bytes();
        let mut concat = [0u8; 32];
        concat[..datasize].copy_from_slice(&src1[..datasize]);
        concat[datasize..2 * datasize].copy_from_slice(&src2[..datasize]);

        let mut dst = [0u8; 16];
        for i in 0..datasize {
            dst[i] = concat[imm4 + i];
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


    /// Execute an Advanced SIMD three-same floating-point instruction.
    pub(crate) fn exec_simd_three_same_fp(&mut self, insn: u32, scalar: bool) -> Result<CpuExit, ArmError> {
        let q = (insn >> 30) & 1;
        let u = (insn >> 29) & 1;
        let size = (insn >> 22) & 0x3;
        let opcode = (insn >> 11) & 0x1F;
        let rm = ((insn >> 16) & 0x1F) as usize;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;

        let sz = size & 1; // 0 => f32, 1 => f64
        let a_bit = (size >> 1) & 1;

        if scalar {
            // The scalar AdvSIMD FP three-same table only defines compare/step
            // operations, FMULX, and FABD. Ordinary arithmetic,
            // FMA, min/max, pairwise, and FDIV forms are vector-only here.
            let legal_scalar = matches!(
                (u, a_bit, opcode, sz),
                (0, 0, 0b11011, _) // FMULX
                    | (0, 0, 0b11100, _) // FCMEQ
                    | (0, 0, 0b11111, _) // FRECPS
                    | (0, 1, 0b11111, _) // FRSQRTS
                    | (1, 0, 0b11100, _) // FCMGE
                    | (1, 0, 0b11101, _) // FACGE
                    | (1, 1, 0b11010, _) // FABD
                    | (1, 1, 0b11100, _) // FCMGT
                    | (1, 1, 0b11101, _) // FACGT
            );
            if !legal_scalar {
                return Err(ArmError::UndefinedInstruction(insn));
            }
        }

        // FEAT_FHM: FMLAL/FMLSL (U==0, opcode 0b11101) and FMLAL2/FMLSL2
        // (U==1, opcode 0b11001) widen FP16 lanes into FP32 accumulator lanes.
        // These are only defined for the vector (non-scalar) form.
        if !scalar && ((u == 0 && opcode == 0b11101) || (u == 1 && opcode == 0b11001)) {
            if size & 1 != 0 {
                return Err(ArmError::UndefinedInstruction(insn));
            }
            return self.exec_fmlal(insn, false);
        }

        let kind = match fp_three_same_decode(u, a_bit, opcode) {
            Some(k) => k,
            None => return Err(ArmError::UndefinedInstruction(insn)),
        };
        let esize = if sz == 0 { 4usize } else { 8 };

        // A 64-bit vector cannot hold a single f64 element (needs 2D / Q==1).
        if sz == 1 && q == 0 && !scalar {
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

        let pairwise = matches!(
            kind,
            FpKind::Addp | FpKind::Maxp | FpKind::Minp | FpKind::MaxNmp | FpKind::MinNmp
        );

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
            let res = if pairwise {
                sve_fp_pairwise_reduce_combine_with_fpcr(kind, esize, a, b, self.fpcr)
            } else if sz == 0 {
                fp_three_same_f32_with_fpcr(kind, a as u32, b as u32, d as u32, self.fpcr) as u64
            } else {
                fp_three_same_f64_with_fpcr(kind, a, b, d, self.fpcr)
            };
            self.fpsr |= if pairwise {
                fp_pairwise_reduce_status_with_fpcr(esize, kind, a, b, res, self.fpcr)
            } else {
                fp_three_same_status_with_fpcr(esize, kind, a, b, d, res, self.fpcr)
            };
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


    /// Deterministic FP two-register-misc ops (FABS/FNEG/FSQRT, FRINT*, FCVT* to
    /// integer, SCVTF/UCVTF, FCMxx #0). Returns `None` for the estimate ops and
    /// FP narrow/long forms so the caller can fall through.
    pub(crate) fn exec_simd_two_reg_fp(&mut self, insn: u32) -> Option<Result<CpuExit, ArmError>> {
        let q = (insn >> 30) & 1;
        let u = (insn >> 29) & 1;
        let sz_hi = (insn >> 23) & 1;
        let sz = (insn >> 22) & 1; // 0 => f32, 1 => f64
        let opcode = (insn >> 12) & 0x1F;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;
        let scalar = ((insn >> 24) & 0x1F) == 0b11110;

        // FRINT32X/Z (opcode 11110) and FRINT64X/Z (opcode 11111), sz_hi==0.
        // U selects X(round per mode)/Z(toward zero); bit22 selects f32/f64.
        if sz_hi == 0 && (opcode == 0b11110 || opcode == 0b11111) {
            let intsize = if opcode == 0b11110 { 32 } else { 64 };
            let z = u == 0;
            let esize = if sz == 0 { 4usize } else { 8 };
            if sz == 1 && q == 0 && !scalar {
                return Some(Err(ArmError::UndefinedInstruction(insn)));
            }
            let datasize = if scalar {
                esize
            } else if q == 1 {
                16
            } else {
                8
            };
            let src = self.v[rn].to_le_bytes();
            let mut dst = [0u8; 16];
            for e in 0..(datasize / esize) {
                let off = e * esize;
                let a = read_elem(&src, off, esize);
                let r = if esize == 4 {
                    frint_ts_f32_with_fpcr(a as u32, intsize, z, self.fpcr) as u64
                } else {
                    frint_ts_f64_with_fpcr(a, intsize, z, self.fpcr)
                };
                self.fpsr |= if esize == 4 {
                    fp_status_frint_ts_f32_with_fpcr(a as u32, intsize, z, self.fpcr)
                } else {
                    fp_status_frint_ts_f64_with_fpcr(a, intsize, z, self.fpcr)
                };
                write_elem(&mut dst, off, esize, r);
            }
            self.v[rd] = u128::from_le_bytes(dst);
            return Some(Ok(CpuExit::Continue));
        }

        // FCVTL/FCVTL2 (opcode 10111, U=0), FCVTN/FCVTN2 (10110, U=0) and
        // FCVTXN/FCVTXN2 (10110, U=1): FP convert long/narrow. sz(bit22) selects
        // the f16<->f32 (0) vs f32<->f64 (1) pair. (BFCVTN size==10 is handled
        // before reaching here.)
        if opcode == 0b10110 || opcode == 0b10111 {
            let long = opcode == 0b10111;
            let round_odd = !long && u == 1; // FCVTXN
            if round_odd && sz == 0 {
                return Some(Err(ArmError::UndefinedInstruction(insn)));
            }
            let part = q as usize; // FCVTL2/FCVTN2 use the upper half
            if long {
                // Widen: f16->f32 (sz=0) or f32->f64 (sz=1). The source 64-bit
                // half holds 8/sp elements (4 h or 2 s).
                let (sp, dp) = if sz == 0 { (2usize, 4usize) } else { (4, 8) };
                let nelem = 8 / sp;
                let src = self.v[rn].to_le_bytes();
                let mut dst = [0u8; 16];
                for e in 0..nelem {
                    let s = read_elem(&src, part * 8 + e * sp, sp);
                    let r = fp_cvt_elem(s, sp, dp, false, self.fpcr);
                    self.fpsr |= fp_status_cvt_precision_with_fpcr(s, sp, dp, r, self.fpcr);
                    write_elem(&mut dst, e * dp, dp, r);
                }
                self.v[rd] = u128::from_le_bytes(dst);
            } else {
                // Narrow: f32->f16 (sz=0) or f64->f32 (sz=1, or FCVTX round-odd).
                // The full 128-bit source holds 16/sp elements (4 s or 2 d).
                let (sp, dp) = if sz == 0 { (4usize, 2usize) } else { (8, 4) };
                let nelem = if scalar { 1 } else { 16 / sp };
                let src = self.v[rn].to_le_bytes();
                let mut dst = if scalar || part == 0 {
                    [0u8; 16]
                } else {
                    self.v[rd].to_le_bytes()
                };
                let base = if scalar { 0 } else { part * 8 };
                for e in 0..nelem {
                    let s = read_elem(&src, e * sp, sp);
                    let r = fp_cvt_elem(s, sp, dp, round_odd, self.fpcr);
                    self.fpsr |= fp_status_cvt_precision_with_fpcr_rounding(
                        s, sp, dp, r, round_odd, self.fpcr,
                    );
                    write_elem(&mut dst, base + e * dp, dp, r);
                }
                self.v[rd] = u128::from_le_bytes(dst);
            }
            return Some(Ok(CpuExit::Continue));
        }

        // SCVTF / UCVTF take an integer source, so they bypass the float helper.
        let cvtf = match (u, sz_hi, opcode) {
            (0, 0, 0b11101) => Some(false), // SCVTF
            (1, 0, 0b11101) => Some(true),  // UCVTF
            _ => None,
        };
        let kind = match (u, sz_hi, opcode) {
            (0, 1, 0b01111) => Some(TwoRegFp::Fabs),
            (1, 1, 0b01111) => Some(TwoRegFp::Fneg),
            (1, 1, 0b11111) => Some(TwoRegFp::Fsqrt),
            (0, 0, 0b11000) => Some(TwoRegFp::RintN),
            (0, 1, 0b11000) => Some(TwoRegFp::RintP),
            (1, 0, 0b11000) => Some(TwoRegFp::RintA),
            (0, 0, 0b11001) => Some(TwoRegFp::RintM),
            (0, 1, 0b11001) => Some(TwoRegFp::RintZ),
            (1, 0, 0b11001) => Some(TwoRegFp::RintX),
            (1, 1, 0b11001) => Some(TwoRegFp::RintI),
            (0, 0, 0b11010) => Some(TwoRegFp::CvtNS),
            (0, 1, 0b11010) => Some(TwoRegFp::CvtPS),
            (1, 0, 0b11010) => Some(TwoRegFp::CvtNU),
            (1, 1, 0b11010) => Some(TwoRegFp::CvtPU),
            (0, 0, 0b11011) => Some(TwoRegFp::CvtMS),
            (0, 1, 0b11011) => Some(TwoRegFp::CvtZS),
            (1, 0, 0b11011) => Some(TwoRegFp::CvtMU),
            (1, 1, 0b11011) => Some(TwoRegFp::CvtZU),
            (0, 0, 0b11100) => Some(TwoRegFp::CvtAS),
            (1, 0, 0b11100) => Some(TwoRegFp::CvtAU),
            (0, 1, 0b01100) => Some(TwoRegFp::CmGt),
            (1, 1, 0b01100) => Some(TwoRegFp::CmGe),
            (0, 1, 0b01101) => Some(TwoRegFp::CmEq),
            (1, 1, 0b01101) => Some(TwoRegFp::CmLe),
            (0, 1, 0b01110) => Some(TwoRegFp::CmLt),
            _ => None,
        };
        // URECPE (U=0) / URSQRTE (U=1): unsigned 32-bit integer estimates,
        // sz_hi=1, opcode 11100.
        if (insn >> 23) & 1 == 1 && opcode == 0b11100 {
            if sz != 0 {
                return Some(Err(ArmError::UndefinedInstruction(insn)));
            }
            let datasize = if scalar {
                4usize
            } else if q == 1 {
                16
            } else {
                8
            };
            let elements = datasize / 4;
            let src = self.v[rn].to_le_bytes();
            let mut dst = [0u8; 16];
            let is_rsqrt = (insn >> 29) & 1 == 1;
            for e in 0..elements {
                let off = e * 4;
                let a = read_elem(&src, off, 4) as u32;
                let r = if is_rsqrt {
                    unsigned_rsqrt_estimate(a)
                } else {
                    unsigned_recip_estimate(a)
                };
                write_elem(&mut dst, off, 4, r as u64);
            }
            self.v[rd] = u128::from_le_bytes(dst);
            return Some(Ok(CpuExit::Continue));
        }

        // FRECPE (U=0) / FRSQRTE (U=1): estimate ops, sz_hi=1, opcode 11101.
        if (insn >> 23) & 1 == 1 && opcode == 0b11101 {
            let is_rsqrt = (insn >> 29) & 1 == 1;
            if sz == 1 && q == 0 && !scalar {
                return Some(Err(ArmError::UndefinedInstruction(insn)));
            }
            let esize = if sz == 0 { 4usize } else { 8 };
            let datasize = if scalar {
                esize
            } else if q == 1 {
                16
            } else {
                8
            };
            let elements = datasize / esize;
            let src = self.v[rn].to_le_bytes();
            let mut dst = [0u8; 16];
            for e in 0..elements {
                let off = e * esize;
                let raw = read_elem(&src, off, esize);
                let a = fp_estimate_input_with_fpcr(raw, (esize * 8) as u32, self.fpcr);
                let r = match (is_rsqrt, sz == 0) {
                    (false, true) => fp_recip_estimate_f32(a as u32) as u64,
                    (false, false) => fp_recip_estimate_f64(a),
                    (true, true) => fp_rsqrt_estimate_f32_with_fpcr(a as u32, self.fpcr) as u64,
                    (true, false) => fp_rsqrt_estimate_f64_with_fpcr(a, self.fpcr),
                };
                self.fpsr |= fp_status_estimate_with_fpcr(esize, is_rsqrt, raw, r, self.fpcr);
                write_elem(&mut dst, off, esize, r);
            }
            self.v[rd] = u128::from_le_bytes(dst);
            return Some(Ok(CpuExit::Continue));
        }

        // FRECPX (reciprocal exponent): sz_hi=1, opcode 11111, U=0. Scalar-only
        // in AArch64 (no vector form), so only lane 0 is written and the rest of
        // the register is zeroed.
        if (insn >> 23) & 1 == 1 && opcode == 0b11111 && u == 0 {
            if !scalar {
                return Some(Err(ArmError::UndefinedInstruction(insn)));
            }
            let esize = if sz == 0 { 4usize } else { 8 };
            let raw = read_elem(&self.v[rn].to_le_bytes(), 0, esize);
            let a = fp_flush_input_bits_with_fpcr(raw, (esize * 8) as u32, self.fpcr);
            let mut dst = [0u8; 16];
            let r = sve_fp_recpx(esize, a);
            if fp_is_snan_bits(esize, a) {
                self.fpsr |= FPSR_IOC;
            }
            if self.fpcr & FPCR_AH == 0 {
                self.fpsr |= fp_fz_input_status(esize, raw, self.fpcr);
            }
            write_elem(&mut dst, 0, esize, r);
            self.v[rd] = u128::from_le_bytes(dst);
            return Some(Ok(CpuExit::Continue));
        }

        if kind.is_none() && cvtf.is_none() {
            return None;
        }

        if sz == 1 && q == 0 && !scalar {
            return Some(Err(ArmError::UndefinedInstruction(insn)));
        }
        let esize = if sz == 0 { 4usize } else { 8 };
        let datasize = if scalar {
            esize
        } else if q == 1 {
            16
        } else {
            8
        };
        let elements = datasize / esize;
        let src = self.v[rn].to_le_bytes();
        let mut dst = [0u8; 16];
        for e in 0..elements {
            let off = e * esize;
            let a = read_elem(&src, off, esize);
            let r = if let Some(unsigned) = cvtf {
                let (negative, raw_int) = if unsigned {
                    if sz == 0 {
                        (false, (a as u32) as u128)
                    } else {
                        (false, a as u128)
                    }
                } else if sz == 0 {
                    let x = a as u32 as i32;
                    (x < 0, (x as i128).unsigned_abs())
                } else {
                    let x = a as i64;
                    (x < 0, (x as i128).unsigned_abs())
                };
                let r = if sz == 0 {
                    int_to_fp32_bits_with_fpcr(raw_int, negative, self.fpcr) as u64
                } else {
                    int_to_fp64_bits_with_fpcr(raw_int, negative, self.fpcr)
                };
                self.fpsr |= fp_status_int_to_fp_scaled(raw_int, esize, r);
                r
            } else if sz == 0 {
                fp_two_reg_f32_with_fpcr(kind.unwrap(), a as u32, self.fpcr) as u64
            } else {
                fp_two_reg_f64_with_fpcr(kind.unwrap(), a, self.fpcr)
            };
            if let Some(kind) = kind {
                self.fpsr |= fp_status_unop_with_fpcr(esize, Some(kind), a, r, self.fpcr);
                self.fpsr |= fp_status_fp_to_int_unop_with_fpcr(esize, kind, a, self.fpcr);
            }
            write_elem(&mut dst, off, esize, r);
        }
        self.v[rd] = u128::from_le_bytes(dst);
        Some(Ok(CpuExit::Continue))
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


    pub(crate) fn exec_extract(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let sf = (insn >> 31) & 1;
        let opc = (insn >> 29) & 0x3;
        let n = (insn >> 22) & 1;
        let rm = ((insn >> 16) & 0x1F) as u8;
        let imms = ((insn >> 10) & 0x3F) as u32;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let rd = (insn & 0x1F) as u8;

        let datasize = if sf != 0 { 64u32 } else { 32 };
        if opc != 0 || (sf == 0 && (n != 0 || imms >= 32)) || (sf != 0 && n == 0) {
            return Err(ArmError::UndefinedInstruction(insn));
        }
        let lsb = imms;

        let operand1 = if sf != 0 {
            self.get_x(rn)
        } else {
            self.get_w(rn) as u64
        };

        let operand2 = if sf != 0 {
            self.get_x(rm)
        } else {
            self.get_w(rm) as u64
        };

        let result = if lsb == 0 {
            operand2
        } else {
            (operand1 << (datasize - lsb)) | (operand2 >> lsb)
        };

        if sf != 0 {
            self.set_x(rd, result);
        } else {
            self.set_w(rd, result as u32);
        }

        Ok(CpuExit::Continue)
    }
}
