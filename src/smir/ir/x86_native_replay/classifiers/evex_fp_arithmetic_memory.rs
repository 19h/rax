//! EVEX packed binary32/binary64 arithmetic memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

const OPCODES: [u8; 6] = [0x58, 0x59, 0x5C, 0x5D, 0x5E, 0x5F];

/// Native replay strategy for one exact packed binary32/binary64 arithmetic
/// memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexPackedFpArithmeticMemoryReplay {
    /// A complete vector helper load followed by a register-source rewrite
    /// using one nonarchitectural low vector register.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// A scalar helper load followed by the original broadcast operation
    /// rewritten to consume the staged value from `[rsp]`.
    Broadcast {
        stack_instruction: X86InstructionBytes,
    },
    /// Per-active-lane scalar helper loads accumulated in a nonarchitectural
    /// stack vector, followed by the original writemasked operation rewritten
    /// to consume that vector from `[rsp]`.
    MaskedVector {
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact EVEX packed binary32/binary64 arithmetic memory encoding and its
/// byte-validated native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexPackedFpArithmeticMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) opcode: u8,
    pub(crate) replay: X86EvexPackedFpArithmeticMemoryReplay,
    pub(crate) needs_avx512vl: bool,
}

impl X86InstructionBytes {
    /// Validate one EVEX `VADD`/`VMUL`/`VSUB`/`VMIN`/`VDIV`/`VMAX` packed
    /// binary32 or binary64 memory source and select an exact native replay.
    ///
    /// Intel SDM Vol. 2 assigns these operations to map 0F opcodes
    /// 58H/59H/5CH/5DH/5EH/5FH. Packed single precision uses W0.NP; packed
    /// double precision uses W1.66. `L'L` selects 128/256/512 bits. For a
    /// memory source, `EVEX.b=1` selects m32bcst or m64bcst and never embedded
    /// rounding or SAE. All admitted forms retain dynamic MXCSR control.
    ///
    /// Segment/address-size prefixes and APX B4/X4 address extensions are
    /// consumed only by the helper-computed guest address. Rewrites therefore
    /// remove those address controls while preserving every architectural
    /// vector operand, opmask, zeroing, vector-length, and opcode field.
    pub(crate) fn evex_packed_fp_arithmetic_memory_encoding(
        &self,
    ) -> Option<X86EvexPackedFpArithmeticMemoryEncoding> {
        let bytes = self.as_slice();
        let start = vector_legacy_prefix_len(bytes);
        if bytes.get(start) != Some(&0x62) {
            return None;
        }

        let p0 = *bytes.get(start + 1)?;
        let p1 = *bytes.get(start + 2)?;
        let p2 = *bytes.get(start + 3)?;
        let opcode = *bytes.get(start + 4)?;
        let modrm_index = start + 5;
        let modrm = *bytes.get(modrm_index)?;
        let elem = match p1 & 0x83 {
            0x00 => VecElementType::F32,
            0x81 => VecElementType::F64,
            _ => return None,
        };
        let zeroing = p2 & 0x80 != 0;
        let mask = p2 & 0x07;
        let broadcast = p2 & 0x10 != 0;
        if p0 & 0x07 != 1
            || !OPCODES.contains(&opcode)
            || modrm >> 6 == 3
            || (zeroing && mask == 0)
            || p2 & 0x60 == 0x60
            || memory_operand_end(bytes, modrm_index)? != bytes.len()
        {
            return None;
        }

        let width = match (p2 >> 5) & 3 {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!("reserved vector length rejected"),
        };
        let destination =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
        let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let writemask = (mask != 0).then_some(mask);
        let needs_avx512vl = width != VecWidth::V512;

        let stack_instruction = || {
            X86InstructionBytes::new(&[
                0x62,
                // Preserve R/R' and map 0F, select unextended SIB index/base,
                // and clear APX B4 because the rewritten base is RSP.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/pp and restore the ordinary EVEX.U bit.
                p1 | 0x04,
                // Preserve z, L'L, b, V', and aaa exactly.
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
            ])
            .unwrap()
        };

        let replay = if broadcast {
            X86EvexPackedFpArithmeticMemoryReplay::Broadcast {
                stack_instruction: stack_instruction(),
            }
        } else if writemask.is_some() {
            let register_probe = X86InstructionBytes::new(&[
                0x62,
                (p0 & 0x97) | 0x60,
                p1 | 0x04,
                p2,
                opcode,
                0xC0 | (modrm & 0x38),
            ])
            .unwrap();
            if register_probe.evex_register_fp_arithmetic_needs_vl() != Some(needs_avx512vl) {
                return None;
            }
            X86EvexPackedFpArithmeticMemoryReplay::MaskedVector {
                stack_instruction: stack_instruction(),
            }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| *candidate != destination && *candidate != source1)
                .expect("two operands cannot consume every low vector register");
            let register_instruction = X86InstructionBytes::new(&[
                0x62,
                // Register EVEX.X/B encode scratch bits 4/3 with inverted
                // polarity. Clear APX B4 and retain destination extensions.
                (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                p1 | 0x04,
                p2,
                opcode,
                0xC0 | (modrm & 0x38) | (scratch & 7),
            ])
            .unwrap();
            if register_instruction.evex_register_fp_arithmetic_needs_vl() != Some(needs_avx512vl) {
                return None;
            }
            X86EvexPackedFpArithmeticMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexPackedFpArithmeticMemoryEncoding {
            width,
            elem,
            destination,
            source1,
            writemask,
            zeroing,
            opcode,
            replay,
            needs_avx512vl,
        })
    }
}
