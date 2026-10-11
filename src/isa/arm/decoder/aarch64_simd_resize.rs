//! A64 integer AdvSIMD long, narrow, and pairwise-long encoding classes.

use crate::isa::arm::ExecutionState;

use super::aarch64::Aarch64Decoder;
use super::{DecodedInsn, FpRegSize, FpRegister, Immediate, Mnemonic, Operand};

impl Aarch64Decoder {
    pub(super) fn decode_simd_resize(raw: u32) -> Option<DecodedInsn> {
        let q = (raw >> 30) & 1;
        let u = (raw >> 29) & 1;
        let scalar = raw & (1 << 28) != 0;
        let size = (raw >> 22) & 3;
        let mut amount = None;
        let mut binary = false;
        let long;
        let mut pairwise = false;
        let mut wide_first = false;
        let mut element_size = size;

        let three_different = raw & 0x9f20_0c00 == 0x0e20_0000 || raw & 0xdf20_0c00 == 0x5e20_0000;
        let two_misc = raw & 0x9f3e_0c00 == 0x0e20_0800 || raw & 0xdf3e_0c00 == 0x5e20_0800;
        let shift_immediate = raw & 0x9f80_0400 == 0x0f00_0400 || raw & 0xdf80_0400 == 0x5f00_0400;

        let (mnemonic, valid) = if three_different {
            let opcode = (raw >> 12) & 15;
            let mnemonic = match opcode {
                0 => Mnemonic::VADDL,
                1 => Mnemonic::VADDW,
                2 => Mnemonic::VSUBL,
                3 => Mnemonic::VSUBW,
                4 if u == 0 => Mnemonic::VADDHN,
                4 => Mnemonic::VRADDHN,
                5 => Mnemonic::VABAL,
                6 if u == 0 => Mnemonic::VSUBHN,
                6 => Mnemonic::VRSUBHN,
                7 => Mnemonic::VABDL,
                8 => Mnemonic::VMLAL,
                9 => Mnemonic::VQDMLAL,
                10 => Mnemonic::VMLSL,
                11 => Mnemonic::VQDMLSL,
                12 => Mnemonic::VMULL,
                13 => Mnemonic::VQDMULL,
                _ => return None,
            };
            let doubling = matches!(opcode, 9 | 11 | 13);
            binary = true;
            long = !matches!(opcode, 4 | 6);
            wide_first = matches!(opcode, 1 | 3);
            let valid = size < 3 && (!doubling || (size > 0 && u == 0)) && (!scalar || doubling);
            (mnemonic, valid)
        } else if two_misc {
            let opcode = (raw >> 12) & 31;
            let mnemonic = match (u, opcode) {
                (_, 2) => Mnemonic::VPADDL,
                (_, 6) => Mnemonic::VPADAL,
                (0, 18) => Mnemonic::VMOVN,
                (1, 18) => Mnemonic::VQMOVUN,
                (1, 19) => Mnemonic::VMOVL,
                (_, 20) => Mnemonic::VQMOVN,
                _ => return None,
            };
            pairwise = matches!(opcode, 2 | 6);
            long = pairwise || opcode == 19;
            if opcode == 19 {
                amount = Some(8u32 << size);
            }
            let valid = size < 3 && (!scalar || (!long && (u != 0 || opcode != 18)));
            (mnemonic, valid)
        } else if shift_immediate {
            let opcode = (raw >> 11) & 31;
            if !(16..=20).contains(&opcode) {
                return None;
            }
            let immh = (raw >> 19) & 15;
            if immh == 0 {
                return None;
            }
            element_size = 31 - immh.leading_zeros();
            let bits = 8 << element_size;
            let encoded = (raw >> 16) & 127;
            let mnemonic = match (u, opcode) {
                (_, 20) => Mnemonic::VMOVL,
                (0, 16) => Mnemonic::VSHRN,
                (0, 17) => Mnemonic::VRSHRN,
                (1, 16) => Mnemonic::VQSHRUN,
                (1, 17) => Mnemonic::VQRSHRUN,
                (_, 18) => Mnemonic::VQSHRN,
                (_, 19) => Mnemonic::VQRSHRN,
                _ => return None,
            };
            long = opcode == 20;
            amount = Some(match opcode {
                20 => encoded - bits,
                _ => 2 * bits - encoded,
            });
            let valid = element_size < 3 && (!scalar || (!long && (u != 0 || opcode >= 18)));
            (mnemonic, valid)
        } else {
            return None;
        };

        if !valid {
            return Some(DecodedInsn::new(
                Mnemonic::UNDEFINED,
                ExecutionState::Aarch64,
                raw,
                4,
            ));
        }

        let narrow_scalar_size = match element_size {
            0 => FpRegSize::B,
            1 => FpRegSize::H,
            _ => FpRegSize::S,
        };
        let wide_scalar_size = match element_size {
            0 => FpRegSize::H,
            1 => FpRegSize::S,
            _ => FpRegSize::D,
        };
        let half_size = match q {
            0 => FpRegSize::D,
            _ => FpRegSize::Q,
        };
        let (destination_size, source_size) = match (scalar, long, pairwise) {
            (true, true, _) => (wide_scalar_size, narrow_scalar_size),
            (true, false, _) => (narrow_scalar_size, wide_scalar_size),
            (false, _, true) => (half_size, half_size),
            (false, true, false) => (FpRegSize::Q, half_size),
            (false, false, false) => (half_size, FpRegSize::Q),
        };
        let register = |num, size| Operand::FpReg(FpRegister { num, size });
        let first_size = match wide_first {
            true => FpRegSize::Q,
            false => source_size,
        };
        let mut instruction = DecodedInsn::new(mnemonic, ExecutionState::Aarch64, raw, 4)
            .with_operand(register((raw & 31) as u8, destination_size))
            .with_operand(register(((raw >> 5) & 31) as u8, first_size));
        if binary {
            instruction = instruction.with_operand(register(((raw >> 16) & 31) as u8, source_size));
        }
        if let Some(amount) = amount {
            instruction = instruction.with_operand(Operand::Imm(Immediate::new(i64::from(amount))));
        }
        Some(instruction)
    }
}
