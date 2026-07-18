//! shift.rs

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
}
