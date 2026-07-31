//! EVEX packed binary16 arithmetic memory-source replay classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::VecWidth;

const OPCODES: [u8; 6] = [0x58, 0x59, 0x5C, 0x5D, 0x5E, 0x5F];

/// Native source-replay strategy for one exact packed binary16 arithmetic
/// memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexPackedFp16ArithmeticMemoryReplay {
    /// A complete vector helper load followed by a register-source rewrite
    /// using one nonarchitectural low vector register.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// A scalar helper load followed by the original broadcast operation
    /// rewritten to consume the helper-staged binary16 value from `[rsp]`.
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

/// Exact EVEX packed binary16 arithmetic memory encoding and its
/// byte-validated native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexPackedFp16ArithmeticMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) opcode: u8,
    pub(crate) replay: X86EvexPackedFp16ArithmeticMemoryReplay,
    pub(crate) needs_avx512vl: bool,
}

impl X86InstructionBytes {
    /// Validate one register-only packed AVX-512-FP16 binary arithmetic form
    /// using dynamic MXCSR rounding and report whether AVX-512VL is required.
    ///
    /// This internal validator is also an independent check on register
    /// rewrites synthesized for full-vector memory sources. Embedded-control
    /// forms are intentionally handled by the public replay classifier in
    /// `fp_arithmetic`.
    pub(crate) fn evex_register_packed_fp16_arithmetic_needs_vl(&self) -> Option<bool> {
        let [0x62, p0, p1, p2, opcode, modrm] = self.as_slice() else {
            return None;
        };
        if p0 & 0x0F != 5
            || p1 & 0x87 != 0x04
            || p2 & 0x10 != 0
            || !OPCODES.contains(opcode)
            || modrm >> 6 != 3
            || (p2 & 0x80 != 0 && p2 & 0x07 == 0)
        {
            return None;
        }
        match (p2 >> 5) & 3 {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate one EVEX `VADDPH`/`VMULPH`/`VSUBPH`/`VMINPH`/`VDIVPH`/
    /// `VMAXPH` memory source and select an exact native replay.
    ///
    /// Intel SDM Vol. 2 assigns these operations to MAP5.W0.NP opcodes
    /// 58H/59H/5CH/5DH/5EH/5FH. `L'L` selects 128/256/512 bits. For memory,
    /// `EVEX.b=1` selects an m16 broadcast; it never carries embedded
    /// rounding or SAE. Unmasked full vectors are helper-loaded into a
    /// nonarchitectural low vector register. Writemasked full vectors and all
    /// broadcasts are rewritten to an equivalent `[rsp]` source while
    /// retaining the exact destination, first source, opmask, zeroing, width,
    /// and opcode controls. Segment/address-size prefixes and APX extended
    /// address bits are consumed only by the helper-computed guest address.
    pub(crate) fn evex_packed_fp16_arithmetic_memory_encoding(
        &self,
    ) -> Option<X86EvexPackedFp16ArithmeticMemoryEncoding> {
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
        let zeroing = p2 & 0x80 != 0;
        let mask = p2 & 0x07;
        let broadcast = p2 & 0x10 != 0;
        if p0 & 0x07 != 5
            || p1 & 0x83 != 0
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
                // Preserve R/R' and MAP5, select unextended SIB index/base,
                // and clear APX B4 because the rewritten base is RSP.
                (p0 & 0x97) | 0x60,
                // Preserve vvvv and restore the ordinary EVEX.U fixed bit.
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
            X86EvexPackedFp16ArithmeticMemoryReplay::Broadcast {
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
            if register_probe.evex_register_packed_fp16_arithmetic_needs_vl()
                != Some(needs_avx512vl)
            {
                return None;
            }
            X86EvexPackedFp16ArithmeticMemoryReplay::MaskedVector {
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
            if register_instruction.evex_register_packed_fp16_arithmetic_needs_vl()
                != Some(needs_avx512vl)
            {
                return None;
            }
            X86EvexPackedFp16ArithmeticMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexPackedFp16ArithmeticMemoryEncoding {
            width,
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
