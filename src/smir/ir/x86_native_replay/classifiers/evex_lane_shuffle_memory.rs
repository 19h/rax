//! EVEX VPSHUFD/VPSHUFHW/VPSHUFLW memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{MemWidth, VecElementType, VecWidth};

/// Exact packed lane-shuffle operation selected by an EVEX memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexLaneShuffleKind {
    Dword,
    HighWord,
    LowWord,
}

impl X86EvexLaneShuffleKind {
    pub(crate) const fn element(self) -> VecElementType {
        match self {
            Self::Dword => VecElementType::I32,
            Self::HighWord | Self::LowWord => VecElementType::I16,
        }
    }

    pub(crate) const fn high_words(self) -> Option<bool> {
        match self {
            Self::Dword => None,
            Self::HighWord => Some(true),
            Self::LowWord => Some(false),
        }
    }
}

/// Native replay selected for one exact EVEX packed lane-shuffle memory
/// source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexLaneShuffleMemoryReplay {
    /// A complete vector tuple staged in an otherwise unused low vector
    /// register.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// One dword broadcast tuple staged in a 16-byte stack slot.
    Broadcast {
        memory_width: MemWidth,
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact EVEX VPSHUFD/VPSHUFHW/VPSHUFLW memory encoding and its
/// byte-validated native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexLaneShuffleMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) kind: X86EvexLaneShuffleKind,
    pub(crate) destination: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) immediate: u8,
    pub(crate) w: bool,
    pub(crate) replay: X86EvexLaneShuffleMemoryReplay,
    pub(crate) memory_size: u32,
    pub(crate) needs_avx512vl: bool,
}

impl X86InstructionBytes {
    /// Validate one EVEX VPSHUFD/VPSHUFHW/VPSHUFLW memory source and select
    /// an exact helper-backed native replay.
    ///
    /// Intel assigns these operations Type E4NF/E4NF.nb semantics and a Full
    /// tuple. Full-vector sources therefore read 16/32/64 bytes, while the
    /// VPSHUFD embedded-broadcast form reads one 4-byte dword, irrespective of
    /// the destination writemask. VPSHUFHW/LW are WIG; both W encodings are
    /// retained exactly. Segment/address-size prefixes and APX B4/X4 address
    /// extensions remain confined to helper address evaluation.
    pub(crate) fn evex_lane_shuffle_memory_encoding(
        &self,
    ) -> Option<X86EvexLaneShuffleMemoryEncoding> {
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
        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        let kind = match (pp, w) {
            (1, false) => X86EvexLaneShuffleKind::Dword,
            (2, _) => X86EvexLaneShuffleKind::HighWord,
            (3, _) => X86EvexLaneShuffleKind::LowWord,
            _ => return None,
        };
        let mask = p2 & 0x07;
        let zeroing = p2 & 0x80 != 0;
        let broadcast = p2 & 0x10 != 0;
        if p0 & 0x07 != 1
            || p1 & 0x78 != 0x78
            || p2 & 0x08 == 0
            || opcode != 0x70
            || p2 & 0x60 == 0x60
            || modrm >> 6 == 3
            || (zeroing && mask == 0)
            || (broadcast && kind != X86EvexLaneShuffleKind::Dword)
            || operand_end.checked_add(1)? != bytes.len()
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
        let scratch = (0..16u8)
            .find(|candidate| *candidate != destination)
            .expect("one destination cannot consume every low vector register");
        let needs_avx512vl = width != VecWidth::V512;
        let register_instruction = X86InstructionBytes::new(&[
            0x62,
            // Register EVEX.X/B encode scratch bits 4/3 with inverted
            // polarity. Scratch is low, so X is one; clear APX B4.
            (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
            // Preserve W/pp and restore ordinary EVEX.U. Reserved vvvv was
            // validated above.
            p1 | 0x04,
            // Preserve z, L'L, V', and aaa; register replay clears EVEX.b.
            p2 & !0x10,
            opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
            immediate,
        ])
        .unwrap();
        if register_instruction.evex_register_lane_shuffle_needs_vl() != Some(needs_avx512vl) {
            return None;
        }

        let replay = if broadcast {
            let stack_instruction = X86InstructionBytes::new(&[
                0x62,
                // Preserve R/R' and map 0F, select unextended SIB
                // index/base, and clear APX B4 for rewritten RSP.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/pp and restore ordinary EVEX.U.
                p1 | 0x04,
                // Preserve z, L'L, broadcast, V', and aaa exactly.
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
                immediate,
            ])
            .unwrap();
            X86EvexLaneShuffleMemoryReplay::Broadcast {
                memory_width: MemWidth::B4,
                stack_instruction,
            }
        } else {
            X86EvexLaneShuffleMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };
        let memory_size = match replay {
            X86EvexLaneShuffleMemoryReplay::Vector { .. } => width.bytes(),
            X86EvexLaneShuffleMemoryReplay::Broadcast { memory_width, .. } => memory_width.bytes(),
        };

        Some(X86EvexLaneShuffleMemoryEncoding {
            width,
            kind,
            destination,
            writemask: (mask != 0).then_some(mask),
            zeroing,
            immediate,
            w,
            replay,
            memory_size,
            needs_avx512vl,
        })
    }
}
