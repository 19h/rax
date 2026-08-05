//! EVEX packed binary16 conversion memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::ops::X86VecMap;
use crate::smir::ir::types::{FpRoundMode, VecElementType, VecWidth};

/// Semantic operation selected by one exact packed AVX-512-FP16 conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexPackedFp16ConvertMemoryKind {
    FpPrecision {
        from: VecElementType,
        to: VecElementType,
    },
    IntToFp16 {
        int_elem: VecElementType,
        signed: bool,
    },
    Fp16ToInt {
        int_elem: VecElementType,
        signed: bool,
        truncate: bool,
    },
}

impl X86EvexPackedFp16ConvertMemoryKind {
    pub(crate) fn source_elem(self) -> VecElementType {
        match self {
            Self::FpPrecision { from, .. } => from,
            Self::IntToFp16 { int_elem, .. } => int_elem,
            Self::Fp16ToInt { .. } => VecElementType::F16,
        }
    }

    fn destination_elem(self) -> VecElementType {
        match self {
            Self::FpPrecision { to, .. } => to,
            Self::IntToFp16 { .. } => VecElementType::F16,
            Self::Fp16ToInt { int_elem, .. } => int_elem,
        }
    }

    pub(crate) fn round(self) -> FpRoundMode {
        match self {
            Self::Fp16ToInt { truncate: true, .. } => FpRoundMode::RoundTowardZero,
            _ => FpRoundMode::Dynamic,
        }
    }
}

/// Helper-backed native source replay selected for one packed conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexPackedFp16ConvertMemoryReplay {
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    Broadcast {
        stack_instruction: X86InstructionBytes,
    },
    MaskedVector {
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact EVEX packed binary16 conversion memory encoding and replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexPackedFp16ConvertMemoryEncoding {
    pub(crate) kind: X86EvexPackedFp16ConvertMemoryKind,
    pub(crate) operation_width: VecWidth,
    pub(crate) source_width: VecWidth,
    pub(crate) destination_width: VecWidth,
    pub(crate) lanes: u8,
    pub(crate) destination: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) broadcast: bool,
    pub(crate) map: X86VecMap,
    pub(crate) pp: u8,
    pub(crate) w: bool,
    pub(crate) opcode: u8,
    pub(crate) replay: X86EvexPackedFp16ConvertMemoryReplay,
    pub(crate) needs_avx512vl: bool,
}

impl X86EvexPackedFp16ConvertMemoryEncoding {
    /// A native vector register has no 64-bit encoding; transfer a V64 SMIR
    /// source through the low 64 bits of one XMM register.
    pub(crate) fn transfer_width(self) -> VecWidth {
        match self.source_width {
            VecWidth::V64 => VecWidth::V128,
            width => width,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PackedFp16ConvertFields {
    kind: X86EvexPackedFp16ConvertMemoryKind,
    operation_width: VecWidth,
    source_width: VecWidth,
    destination_width: VecWidth,
    lanes: u8,
    destination: u8,
    writemask: Option<u8>,
    zeroing: bool,
    map: X86VecMap,
    pp: u8,
    w: bool,
    opcode: u8,
    needs_avx512vl: bool,
}

fn conversion_kind(
    map: X86VecMap,
    opcode: u8,
    pp: u8,
    w: bool,
) -> Option<X86EvexPackedFp16ConvertMemoryKind> {
    use VecElementType::{F16, F32, F64, I16, I32, I64};
    use X86EvexPackedFp16ConvertMemoryKind::{Fp16ToInt, FpPrecision, IntToFp16};

    match (map, opcode, pp, w) {
        (X86VecMap::Map5, 0x5A, 1, true) => Some(FpPrecision { from: F64, to: F16 }),
        (X86VecMap::Map5, 0x5A, 0, false) => Some(FpPrecision { from: F16, to: F64 }),
        (X86VecMap::Map5, 0x1D, 1, false) => Some(FpPrecision { from: F32, to: F16 }),
        (X86VecMap::Map6, 0x13, 1, false) => Some(FpPrecision { from: F16, to: F32 }),

        (X86VecMap::Map5, 0x5B, 0, false) => Some(IntToFp16 {
            int_elem: I32,
            signed: true,
        }),
        (X86VecMap::Map5, 0x5B, 0, true) => Some(IntToFp16 {
            int_elem: I64,
            signed: true,
        }),
        (X86VecMap::Map5, 0x7A, 3, false) => Some(IntToFp16 {
            int_elem: I32,
            signed: false,
        }),
        (X86VecMap::Map5, 0x7A, 3, true) => Some(IntToFp16 {
            int_elem: I64,
            signed: false,
        }),
        (X86VecMap::Map5, 0x7D, 2, false) => Some(IntToFp16 {
            int_elem: I16,
            signed: true,
        }),
        (X86VecMap::Map5, 0x7D, 3, false) => Some(IntToFp16 {
            int_elem: I16,
            signed: false,
        }),

        (X86VecMap::Map5, 0x5B, 1, false) => Some(Fp16ToInt {
            int_elem: I32,
            signed: true,
            truncate: false,
        }),
        (X86VecMap::Map5, 0x5B, 2, false) => Some(Fp16ToInt {
            int_elem: I32,
            signed: true,
            truncate: true,
        }),
        (X86VecMap::Map5, 0x7B, 1, false) => Some(Fp16ToInt {
            int_elem: I64,
            signed: true,
            truncate: false,
        }),
        (X86VecMap::Map5, 0x7A, 1, false) => Some(Fp16ToInt {
            int_elem: I64,
            signed: true,
            truncate: true,
        }),
        (X86VecMap::Map5, 0x79, 0, false) => Some(Fp16ToInt {
            int_elem: I32,
            signed: false,
            truncate: false,
        }),
        (X86VecMap::Map5, 0x78, 0, false) => Some(Fp16ToInt {
            int_elem: I32,
            signed: false,
            truncate: true,
        }),
        (X86VecMap::Map5, 0x79, 1, false) => Some(Fp16ToInt {
            int_elem: I64,
            signed: false,
            truncate: false,
        }),
        (X86VecMap::Map5, 0x78, 1, false) => Some(Fp16ToInt {
            int_elem: I64,
            signed: false,
            truncate: true,
        }),
        (X86VecMap::Map5, 0x7D, 1, false) => Some(Fp16ToInt {
            int_elem: I16,
            signed: true,
            truncate: false,
        }),
        (X86VecMap::Map5, 0x7C, 1, false) => Some(Fp16ToInt {
            int_elem: I16,
            signed: true,
            truncate: true,
        }),
        (X86VecMap::Map5, 0x7D, 0, false) => Some(Fp16ToInt {
            int_elem: I16,
            signed: false,
            truncate: false,
        }),
        (X86VecMap::Map5, 0x7C, 0, false) => Some(Fp16ToInt {
            int_elem: I16,
            signed: false,
            truncate: true,
        }),
        _ => None,
    }
}

fn exact_width(bytes: u32) -> VecWidth {
    match bytes {
        0..=8 => VecWidth::V64,
        9..=16 => VecWidth::V128,
        17..=32 => VecWidth::V256,
        _ => VecWidth::V512,
    }
}

fn packed_fp16_convert_fields(
    p0: u8,
    p1: u8,
    p2: u8,
    opcode: u8,
    modrm: u8,
) -> Option<PackedFp16ConvertFields> {
    let map = match p0 & 7 {
        5 => X86VecMap::Map5,
        6 => X86VecMap::Map6,
        _ => return None,
    };
    let pp = p1 & 3;
    let w = p1 & 0x80 != 0;
    let kind = conversion_kind(map, opcode, pp, w)?;
    let ll = (p2 >> 5) & 3;
    let mask = p2 & 7;
    let zeroing = p2 & 0x80 != 0;
    if p1 & 0x78 != 0x78 || p2 & 8 == 0 || ll == 3 || (zeroing && mask == 0) {
        return None;
    }

    let operation_width = match ll {
        0 => VecWidth::V128,
        1 => VecWidth::V256,
        2 => VecWidth::V512,
        _ => unreachable!("reserved LL rejected"),
    };
    let source_elem = kind.source_elem();
    let destination_elem = kind.destination_elem();
    let lane_bytes = source_elem.bytes().max(destination_elem.bytes());
    let lanes = operation_width.bytes() / lane_bytes;
    let source_bytes = lanes * source_elem.bytes();
    let destination_bytes = lanes * destination_elem.bytes();
    Some(PackedFp16ConvertFields {
        kind,
        operation_width,
        source_width: exact_width(source_bytes),
        destination_width: exact_width(destination_bytes),
        lanes: u8::try_from(lanes).ok()?,
        destination: (u8::from(p0 & 0x80 == 0) << 3)
            | (u8::from(p0 & 0x10 == 0) << 4)
            | ((modrm >> 3) & 7),
        writemask: (mask != 0).then_some(mask),
        zeroing,
        map,
        pp,
        w,
        opcode,
        needs_avx512vl: operation_width != VecWidth::V512,
    })
}

fn register_packed_fp16_convert_fields(bytes: &[u8]) -> Option<PackedFp16ConvertFields> {
    let [0x62, p0, p1, p2, opcode, modrm] = bytes else {
        return None;
    };
    if modrm >> 6 != 3 || p2 & 0x10 != 0 || p1 & 4 == 0 {
        return None;
    }
    packed_fp16_convert_fields(*p0, *p1, *p2, *opcode, *modrm)
}

impl X86InstructionBytes {
    /// Validate one of the 22 packed AVX-512-FP16 conversion memory forms and
    /// synthesize an exact helper-backed native replay.
    ///
    /// Intel SDM revision 092 specifies Type E2 full/half/quarter memory
    /// tuples, optional scalar broadcast, and writemask fault suppression for
    /// this family. `vvvv/V'` are reserved. Segment/address-size prefixes and
    /// APX B4/X4 address channels remain confined to helper address evaluation.
    pub(crate) fn evex_packed_fp16_convert_memory_encoding(
        &self,
    ) -> Option<X86EvexPackedFp16ConvertMemoryEncoding> {
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
        if modrm >> 6 == 3 || memory_operand_end(bytes, modrm_index)? != bytes.len() {
            return None;
        }

        let fields = packed_fp16_convert_fields(p0, p1, p2, opcode, modrm)?;
        let broadcast = p2 & 0x10 != 0;
        let scratch = (0..16u8)
            .find(|candidate| *candidate != fields.destination)
            .expect("one destination leaves at least fifteen low scratch registers");
        let register_probe = X86InstructionBytes::new(&[
            0x62,
            // Preserve destination R/R' and map, select scratch bits 4/3,
            // and remove APX B4 from the helper-owned address.
            (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
            p1 | 0x04,
            // Register EVEX.b carries ER/SAE, not memory broadcast.
            p2 & !0x10,
            opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
        ])?;
        if register_packed_fp16_convert_fields(register_probe.as_slice()) != Some(fields) {
            return None;
        }

        let stack_instruction = || {
            X86InstructionBytes::new(&[
                0x62,
                // Preserve destination R/R' and map, select ordinary RSP,
                // and clear APX B4/X4 address channels.
                (p0 & 0x97) | 0x60,
                p1 | 0x04,
                // Preserve z, L'L, memory broadcast, V', and aaa exactly.
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
            ])
            .expect("seven-byte EVEX stack replay")
        };
        let replay = if broadcast {
            X86EvexPackedFp16ConvertMemoryReplay::Broadcast {
                stack_instruction: stack_instruction(),
            }
        } else if fields.writemask.is_some() {
            X86EvexPackedFp16ConvertMemoryReplay::MaskedVector {
                stack_instruction: stack_instruction(),
            }
        } else {
            X86EvexPackedFp16ConvertMemoryReplay::Vector {
                scratch,
                register_instruction: register_probe,
            }
        };

        Some(X86EvexPackedFp16ConvertMemoryEncoding {
            kind: fields.kind,
            operation_width: fields.operation_width,
            source_width: fields.source_width,
            destination_width: fields.destination_width,
            lanes: fields.lanes,
            destination: fields.destination,
            writemask: fields.writemask,
            zeroing: fields.zeroing,
            broadcast,
            map: fields.map,
            pp: fields.pp,
            w: fields.w,
            opcode: fields.opcode,
            replay,
            needs_avx512vl: fields.needs_avx512vl,
        })
    }
}
