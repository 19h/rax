//! Non-saturating scalar integer AdvSIMD and integer comparisons against zero.

use crate::isa::arm::ExecutionState;

use super::aarch64::Aarch64Decoder;
use super::{DecodedInsn, FpRegSize, FpRegister, Immediate, Mnemonic, Operand};

impl Aarch64Decoder {
    pub(super) fn decode_simd_integer_scalar_or_zero(raw: u32) -> Option<DecodedInsn> {
        let q = (raw >> 30) & 1;
        let u = (raw >> 29) & 1;
        let scalar = raw & (1 << 28) != 0;
        let mut size = (raw >> 22) & 3;
        let mut binary = false;
        let mut immediate = None;
        let mut pairwise = false;

        let scalar_three_same = raw & 0xdf20_0400 == 0x5e20_0400;
        let two_misc = raw & 0x9f3e_0c00 == 0x0e20_0800 || raw & 0xdf3e_0c00 == 0x5e20_0800;
        let scalar_shift = raw & 0xdf80_0400 == 0x5f00_0400;
        let scalar_pairwise = raw & 0xdf3f_0c00 == 0x5e31_0800;

        let mnemonic = if scalar_three_same {
            let opcode = (raw >> 11) & 31;
            binary = true;
            match (u, opcode) {
                (_, 6) => Mnemonic::VCGT,
                (_, 7) => Mnemonic::VCGE,
                (_, 8) => Mnemonic::VSHL,
                (_, 10) => Mnemonic::VRSHL,
                (0, 16) => Mnemonic::VADD,
                (1, 16) => Mnemonic::VSUB,
                (0, 17) => Mnemonic::VTST,
                (1, 17) => Mnemonic::VCEQ,
                _ => return None,
            }
        } else if two_misc {
            let opcode = (raw >> 12) & 31;
            match (u, opcode) {
                (0, 8) => {
                    immediate = Some(0);
                    Mnemonic::VCGT
                }
                (1, 8) => {
                    immediate = Some(0);
                    Mnemonic::VCGE
                }
                (0, 9) => {
                    immediate = Some(0);
                    Mnemonic::VCEQ
                }
                (1, 9) => {
                    immediate = Some(0);
                    Mnemonic::VCLE
                }
                (0, 10) => {
                    immediate = Some(0);
                    Mnemonic::VCLT
                }
                (0, 11) => Mnemonic::VABS,
                (1, 11) => Mnemonic::VNEG,
                _ => return None,
            }
        } else if scalar_shift {
            let opcode = (raw >> 11) & 31;
            let mnemonic = match (u, opcode) {
                (_, 0) => Mnemonic::VSHR,
                (_, 2) => Mnemonic::VSRA,
                (_, 4) => Mnemonic::VRSHR,
                (_, 6) => Mnemonic::VRSRA,
                (0, 10) => Mnemonic::VSHL,
                _ => return None,
            };
            let immh = (raw >> 19) & 15;
            if immh == 0 {
                return Some(DecodedInsn::new(
                    Mnemonic::UNDEFINED,
                    ExecutionState::Aarch64,
                    raw,
                    4,
                ));
            }
            size = 31 - immh.leading_zeros();
            let bits = 8 << size;
            let encoded_amount = (raw >> 16) & 127;
            immediate = Some(if opcode == 10 {
                encoded_amount - bits
            } else {
                2 * bits - encoded_amount
            });
            mnemonic
        } else if scalar_pairwise && (raw >> 12) & 31 == 27 {
            pairwise = true;
            if u != 0 {
                return Some(DecodedInsn::new(
                    Mnemonic::UNDEFINED,
                    ExecutionState::Aarch64,
                    raw,
                    4,
                ));
            }
            Mnemonic::VPADD
        } else {
            return None;
        };

        let size_valid = if scalar {
            size == 3
        } else {
            size != 3 || q == 1
        };
        if !size_valid {
            return Some(DecodedInsn::new(
                Mnemonic::UNDEFINED,
                ExecutionState::Aarch64,
                raw,
                4,
            ));
        }
        let register_size = if scalar || q == 0 {
            FpRegSize::D
        } else {
            FpRegSize::Q
        };
        let register = |num, size| Operand::FpReg(FpRegister { num, size });
        let source_size = if pairwise {
            FpRegSize::Q
        } else {
            register_size
        };
        let mut instruction = DecodedInsn::new(mnemonic, ExecutionState::Aarch64, raw, 4)
            .with_operand(register((raw & 31) as u8, register_size))
            .with_operand(register(((raw >> 5) & 31) as u8, source_size));
        if binary {
            instruction =
                instruction.with_operand(register(((raw >> 16) & 31) as u8, register_size));
        }
        if let Some(value) = immediate {
            instruction = instruction.with_operand(Operand::Imm(Immediate::new(i64::from(value))));
        }
        Some(instruction)
    }
}
