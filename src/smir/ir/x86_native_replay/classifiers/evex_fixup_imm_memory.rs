//! EVEX VFIXUPIMM register and memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Native replay strategy for one exact VFIXUPIMM memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexFixupImmMemoryReplay {
    /// A complete vector helper load followed by a register-source rewrite
    /// using one nonarchitectural low vector register.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// A scalar helper load followed by the original packed broadcast
    /// operation rewritten to consume the staged value from `[rsp]`.
    Broadcast {
        stack_instruction: X86InstructionBytes,
    },
    /// Per-active-lane scalar helper loads accumulated in a nonarchitectural
    /// stack vector, followed by the original writemasked packed operation
    /// rewritten to consume that vector from `[rsp]`.
    MaskedVector {
        stack_instruction: X86InstructionBytes,
    },
    /// A scalar helper load followed by the original scalar operation
    /// rewritten to consume the staged value from `[rsp]`.
    Scalar {
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact VFIXUPIMM memory encoding and its byte-validated native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexFixupImmMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) immediate: u8,
    pub(crate) scalar: bool,
    pub(crate) suppress_exceptions: bool,
    pub(crate) replay: X86EvexFixupImmMemoryReplay,
    pub(crate) needs_avx512vl: bool,
}

impl X86InstructionBytes {
    /// Validate the register-source shape used as an independent semantic
    /// probe for helper-backed VFIXUPIMM rewrites.
    ///
    /// Packed non-SAE forms use L'L=00B/01B/10B for 128/256/512 bits.
    /// Scalar forms are LLIG, including the encoded value 11B, and EVEX.b
    /// selects SAE without changing their 128-bit architectural register
    /// shape. Packed SAE is intentionally outside this probe because memory
    /// EVEX.b denotes broadcast and helper replay never rewrites it to packed
    /// register SAE.
    pub(crate) fn evex_register_fixup_imm_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 7 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        let scalar = opcode == 0x55;
        let zeroing = p2 & 0x80 != 0;
        let mask = p2 & 0x07;
        if p0 & 0x07 != 3
            || p0 & 0x08 != 0
            || p1 & 0x04 == 0
            || p1 & 0x03 != 1
            || !matches!(opcode, 0x54 | 0x55)
            || modrm >> 6 != 3
            || (zeroing && mask == 0)
            || (!scalar && p2 & 0x10 != 0)
        {
            return None;
        }
        if scalar {
            return Some(false);
        }
        match (p2 >> 5) & 3 {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate one packed or scalar AVX-512 VFIXUPIMM memory source and
    /// select an exact helper-backed native replay.
    ///
    /// Intel SDM Vol. 2 assigns both forms to map 0F3A with mandatory 66H:
    /// opcode 54H is packed and opcode 55H is scalar. W0/W1 select binary32
    /// and binary64. Packed L'L selects 128/256/512 bits and memory EVEX.b
    /// selects m32bcst/m64bcst. Scalar L'L is ignored, including 11B, while
    /// scalar memory EVEX.b is reserved because scalar instructions do not
    /// support broadcast and SAE applies only to register sources. Scalar
    /// helper replay canonicalizes LLIG to L'L=00B: this preserves the guest
    /// semantics while avoiding processor-specific #UD behavior for ignored
    /// values in a newly emitted hosted instruction. Every form carries an
    /// unconstrained imm8 response/reporting control.
    ///
    /// Segment/address-size prefixes and APX B4/X4 extensions remain confined
    /// to helper address evaluation. Rewrites therefore remove those address
    /// controls while preserving every architectural vector operand, opmask,
    /// zeroing, vector-length, and immediate field.
    pub(crate) fn evex_fixup_imm_memory_encoding(&self) -> Option<X86EvexFixupImmMemoryEncoding> {
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
        let operand_end = memory_operand_end(bytes, modrm_index)?;
        let immediate = *bytes.get(operand_end)?;
        let scalar = opcode == 0x55;
        let ll = (p2 >> 5) & 3;
        let zeroing = p2 & 0x80 != 0;
        let mask = p2 & 0x07;
        if p0 & 0x07 != 3
            || p1 & 0x03 != 1
            || !matches!(opcode, 0x54 | 0x55)
            || (zeroing && mask == 0)
            || (scalar && p2 & 0x10 != 0)
            || (!scalar && ll == 3)
            || operand_end + 1 != bytes.len()
        {
            return None;
        }

        let elem = if p1 & 0x80 == 0 {
            VecElementType::F32
        } else {
            VecElementType::F64
        };
        let width = if scalar {
            VecWidth::V128
        } else {
            match ll {
                0 => VecWidth::V128,
                1 => VecWidth::V256,
                2 => VecWidth::V512,
                _ => unreachable!("reserved packed vector length rejected"),
            }
        };
        let destination =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | (modrm >> 3) & 7;
        let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let writemask = (mask != 0).then_some(mask);
        let suppress_exceptions = false;
        let needs_avx512vl = !scalar && width != VecWidth::V512;

        // Convert the memory source to an arbitrary low register and validate
        // every non-address semantic field through the independent
        // register-source classifier. Packed broadcast clears b for this
        // probe because register b denotes SAE rather than broadcast.
        let register_probe = X86InstructionBytes::new(&[
            0x62,
            (p0 & 0x97) | 0x60,
            p1 | 0x04,
            if scalar { p2 } else { p2 & !0x10 },
            opcode,
            0xC0 | (modrm & 0x38),
            immediate,
        ])
        .unwrap();
        if register_probe.evex_register_fixup_imm_needs_vl() != Some(needs_avx512vl) {
            return None;
        }

        let stack_instruction = || {
            X86InstructionBytes::new(&[
                0x62,
                // Preserve R/R' and map 0F3A, select unextended SIB
                // index/base, and clear APX B4 for the rewritten RSP base.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/pp and restore the ordinary EVEX.U bit.
                p1 | 0x04,
                // Preserve every meaningful control. Scalar L'L is ignored by
                // the guest ISA, so canonicalize it for hosted replay; packed
                // L'L still selects the architectural vector width.
                if scalar { p2 & !0x60 } else { p2 },
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
                immediate,
            ])
            .unwrap()
        };

        let replay = if scalar {
            X86EvexFixupImmMemoryReplay::Scalar {
                stack_instruction: stack_instruction(),
            }
        } else if p2 & 0x10 != 0 {
            X86EvexFixupImmMemoryReplay::Broadcast {
                stack_instruction: stack_instruction(),
            }
        } else if writemask.is_some() {
            X86EvexFixupImmMemoryReplay::MaskedVector {
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
                immediate,
            ])
            .unwrap();
            if register_instruction.evex_register_fixup_imm_needs_vl() != Some(needs_avx512vl) {
                return None;
            }
            X86EvexFixupImmMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexFixupImmMemoryEncoding {
            width,
            elem,
            destination,
            source1,
            writemask,
            zeroing,
            immediate,
            scalar,
            suppress_exceptions,
            replay,
            needs_avx512vl,
        })
    }
}
