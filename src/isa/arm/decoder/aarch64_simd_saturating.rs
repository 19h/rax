//! A64 saturating arithmetic, same-width shifts, and doubling multiply high.

use crate::isa::arm::ExecutionState;

use super::aarch64::Aarch64Decoder;
use super::{DecodedInsn, FpRegSize, FpRegister, Immediate, Mnemonic, Operand};

impl Aarch64Decoder {
    pub(super) fn decode_simd_saturating(raw: u32) -> Option<DecodedInsn> {
        let q = (raw >> 30) & 1;
        let u = (raw >> 29) & 1;
        let scalar = raw & (1 << 28) != 0;
        let mut size = (raw >> 22) & 3;
        let mut binary = false;
        let mut immediate = None;
        let mut indexed = false;
        let mut rm = ((raw >> 16) & 31) as u8;

        let three_same = raw & 0x9f20_0400 == 0x0e20_0400 || raw & 0xdf20_0400 == 0x5e20_0400;
        let two_misc = raw & 0x9f3e_0c00 == 0x0e20_0800 || raw & 0xdf3e_0c00 == 0x5e20_0800;
        let shift = raw & 0x9f80_0400 == 0x0f00_0400 || raw & 0xdf80_0400 == 0x5f00_0400;
        let by_element = raw & 0x9f00_0400 == 0x0f00_0000 || raw & 0xdf00_0400 == 0x5f00_0000;

        let (mnemonic, class_valid) = if three_same {
            let opcode = (raw >> 11) & 31;
            let mnemonic = match (u, opcode) {
                (_, 1) => Mnemonic::VQADD,
                (_, 5) => Mnemonic::VQSUB,
                (_, 9) => Mnemonic::VQSHL,
                (_, 11) => Mnemonic::VQRSHL,
                (0, 22) => Mnemonic::VQDMULH,
                (1, 22) => Mnemonic::VQRDMULH,
                _ => return None,
            };
            binary = true;
            (mnemonic, opcode != 22 || matches!(size, 1 | 2))
        } else if two_misc {
            let opcode = (raw >> 12) & 31;
            let mnemonic = match (u, opcode) {
                (0, 3) => Mnemonic::SUQADD,
                (1, 3) => Mnemonic::USQADD,
                (0, 7) => Mnemonic::VQABS,
                (1, 7) => Mnemonic::VQNEG,
                _ => return None,
            };
            (mnemonic, true)
        } else if shift {
            let opcode = (raw >> 11) & 31;
            let mnemonic = match (u, opcode) {
                (_, 14) => Mnemonic::VQSHL,
                (1, 12) => Mnemonic::VQSHLU,
                _ => return None,
            };
            let immh = (raw >> 19) & 15;
            if immh == 0 {
                return None;
            }
            size = 31 - immh.leading_zeros();
            immediate = Some(((raw >> 16) & 127) - (8 << size));
            (mnemonic, true)
        } else if by_element {
            let opcode = (raw >> 12) & 15;
            let mnemonic = match (u, opcode) {
                (0, 12) => Mnemonic::VQDMULH,
                (0, 13) => Mnemonic::VQRDMULH,
                _ => return None,
            };
            let h = (raw >> 11) & 1;
            let l = (raw >> 21) & 1;
            let m = (raw >> 20) & 1;
            let lane = match size {
                1 => {
                    rm &= 15;
                    (h << 2) | (l << 1) | m
                }
                _ => (h << 1) | l,
            };
            binary = true;
            indexed = true;
            immediate = Some(lane);
            (mnemonic, matches!(size, 1 | 2))
        } else {
            return None;
        };

        let arrangement_valid = scalar || size != 3 || q == 1;
        if !class_valid || !arrangement_valid {
            return Some(DecodedInsn::new(
                Mnemonic::UNDEFINED,
                ExecutionState::Aarch64,
                raw,
                4,
            ));
        }
        let scalar_size = match size {
            0 => FpRegSize::B,
            1 => FpRegSize::H,
            2 => FpRegSize::S,
            _ => FpRegSize::D,
        };
        let size = match (scalar, q) {
            (true, _) => scalar_size,
            (false, 0) => FpRegSize::D,
            _ => FpRegSize::Q,
        };
        let register = |num, size| Operand::FpReg(FpRegister { num, size });
        let mut instruction = DecodedInsn::new(mnemonic, ExecutionState::Aarch64, raw, 4)
            .with_operand(register((raw & 31) as u8, size))
            .with_operand(register(((raw >> 5) & 31) as u8, size));
        if binary {
            let source_size = match indexed {
                true => FpRegSize::Q,
                false => size,
            };
            instruction = instruction.with_operand(register(rm, source_size));
        }
        if let Some(value) = immediate {
            instruction = instruction.with_operand(Operand::Imm(Immediate::new(i64::from(value))));
        }
        Some(instruction)
    }
}
