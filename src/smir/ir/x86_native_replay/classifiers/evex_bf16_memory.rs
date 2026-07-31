//! EVEX AVX512_BF16 memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::VecWidth;

/// Exact AVX512_BF16 operation selected by one memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexBf16MemoryKind {
    ConvertOne,
    ConvertTwo,
    DotProduct,
}

/// Native replay strategy for one exact AVX512_BF16 memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexBf16MemoryReplay {
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
    /// stack vector, followed by the original writemasked conversion or dot
    /// product rewritten to consume that vector from `[rsp]`.
    MaskedVector {
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact VCVTNEPS2BF16, VCVTNE2PS2BF16, or VDPBF16PS memory encoding and its
/// byte-validated native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexBf16MemoryEncoding {
    pub(crate) kind: X86EvexBf16MemoryKind,
    pub(crate) width: VecWidth,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) replay: X86EvexBf16MemoryReplay,
    pub(crate) needs_avx512vl: bool,
}

fn bf16_kind(p1: u8, opcode: u8) -> Option<X86EvexBf16MemoryKind> {
    match (p1 & 0x83, opcode) {
        (0x02, 0x72) => Some(X86EvexBf16MemoryKind::ConvertOne),
        (0x03, 0x72) => Some(X86EvexBf16MemoryKind::ConvertTwo),
        (0x02, 0x52) => Some(X86EvexBf16MemoryKind::DotProduct),
        _ => None,
    }
}

impl X86InstructionBytes {
    fn evex_register_bf16_kind_needs_vl(&self) -> Option<(X86EvexBf16MemoryKind, bool)> {
        let bytes = self.as_slice();
        let start = vector_legacy_prefix_len(bytes);
        let p0 = *bytes.get(start + 1)?;
        let p1 = *bytes.get(start + 2)?;
        let p2 = *bytes.get(start + 3)?;
        let opcode = *bytes.get(start + 4)?;
        let modrm = *bytes.get(start + 5)?;
        let kind = bf16_kind(p1, opcode)?;
        if bytes.get(start) != Some(&0x62)
            || start + 6 != bytes.len()
            || p0 & 0x0F != 2
            || p1 & 0x04 == 0
            || p2 & 0x10 != 0
            || p2 & 0x60 == 0x60
            || (p2 & 0x80 != 0 && p2 & 7 == 0)
            || modrm >> 6 != 3
        {
            return None;
        }
        Some((kind, p2 & 0x60 != 0x40))
    }

    /// Validate one EVEX VCVTNEPS2BF16, VCVTNE2PS2BF16, or VDPBF16PS memory
    /// source and select an exact helper-backed native replay.
    ///
    /// VCVTNE2PS2BF16 is Type E4NF: every memory form performs one complete
    /// source access regardless of its destination writemask. VCVTNEPS2BF16
    /// and VDPBF16PS are Type E4: zero writemask bits suppress their
    /// corresponding 4-byte source access, while a broadcast issues at most
    /// one 4-byte access. Segment/address-size prefixes and APX B4/X4
    /// extensions remain confined to helper address evaluation.
    pub(crate) fn evex_bf16_memory_encoding(&self) -> Option<X86EvexBf16MemoryEncoding> {
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
        let kind = bf16_kind(p1, opcode)?;
        let mask = p2 & 7;
        let zeroing = p2 & 0x80 != 0;
        let broadcast = p2 & 0x10 != 0;
        if p0 & 7 != 2
            || (kind == X86EvexBf16MemoryKind::ConvertOne
                && (((!p1 >> 3) & 0x0F) != 0 || p2 & 0x08 == 0))
            || p2 & 0x60 == 0x60
            || (zeroing && mask == 0)
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
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | (modrm >> 3) & 7;
        let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let writemask = (mask != 0).then_some(mask);
        let needs_avx512vl = width != VecWidth::V512;

        let stack_instruction = || {
            X86InstructionBytes::new(&[
                0x62,
                // Preserve R/R' and map 0F38, select unextended SIB
                // index/base, and clear APX B4 for the rewritten RSP base.
                (p0 & 0x97) | 0x60,
                // Preserve vvvv/pp and restore the ordinary EVEX.U bit.
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
            X86EvexBf16MemoryReplay::Broadcast {
                stack_instruction: stack_instruction(),
            }
        } else if kind != X86EvexBf16MemoryKind::ConvertTwo && writemask.is_some() {
            X86EvexBf16MemoryReplay::MaskedVector {
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
            if register_instruction.evex_register_bf16_kind_needs_vl()
                != Some((kind, needs_avx512vl))
            {
                return None;
            }
            X86EvexBf16MemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexBf16MemoryEncoding {
            kind,
            width,
            destination,
            source1,
            writemask,
            zeroing,
            replay,
            needs_avx512vl,
        })
    }
}
