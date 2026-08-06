//! EVEX packed conversion memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::ops::X86VecMap;
use crate::smir::ir::types::{FpRoundMode, VecElementType, VecWidth};

/// Semantic operation selected by one exact packed conversion encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexPackedConvertMemoryKind {
    FpPrecision {
        from: VecElementType,
        to: VecElementType,
    },
    IntToFp {
        int_elem: VecElementType,
        fp_elem: VecElementType,
        signed: bool,
    },
    FpToInt {
        fp_elem: VecElementType,
        int_elem: VecElementType,
        signed: bool,
        truncate: bool,
    },
}

impl X86EvexPackedConvertMemoryKind {
    pub(crate) fn source_elem(self) -> VecElementType {
        match self {
            Self::FpPrecision { from, .. } => from,
            Self::IntToFp { int_elem, .. } => int_elem,
            Self::FpToInt { fp_elem, .. } => fp_elem,
        }
    }

    fn destination_elem(self) -> VecElementType {
        match self {
            Self::FpPrecision { to, .. } => to,
            Self::IntToFp { fp_elem, .. } => fp_elem,
            Self::FpToInt { int_elem, .. } => int_elem,
        }
    }

    pub(crate) fn round(self) -> FpRoundMode {
        match self {
            Self::FpToInt { truncate: true, .. } => FpRoundMode::RoundTowardZero,
            _ => FpRoundMode::Dynamic,
        }
    }

    fn needs_avx512dq(self) -> bool {
        match self {
            Self::FpPrecision { .. } => false,
            Self::IntToFp { int_elem, .. } | Self::FpToInt { int_elem, .. } => {
                int_elem == VecElementType::I64
            }
        }
    }
}

/// Native helper replay selected for one packed conversion memory source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexPackedConvertMemoryReplay {
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

/// Exact EVEX packed conversion memory encoding and byte-validated replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexPackedConvertMemoryEncoding {
    pub(crate) kind: X86EvexPackedConvertMemoryKind,
    pub(crate) map: X86VecMap,
    pub(crate) operation_width: VecWidth,
    pub(crate) source_width: VecWidth,
    pub(crate) destination_width: VecWidth,
    pub(crate) lanes: u8,
    pub(crate) destination: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) broadcast: bool,
    pub(crate) pp: u8,
    pub(crate) w: bool,
    pub(crate) opcode: u8,
    pub(crate) replay: X86EvexPackedConvertMemoryReplay,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512dq: bool,
}

impl X86EvexPackedConvertMemoryEncoding {
    pub(crate) fn transfer_width(self) -> VecWidth {
        match self.source_width {
            VecWidth::V64 => VecWidth::V128,
            width => width,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PackedConvertFields {
    kind: X86EvexPackedConvertMemoryKind,
    map: X86VecMap,
    operation_width: VecWidth,
    source_width: VecWidth,
    destination_width: VecWidth,
    lanes: u8,
    destination: u8,
    writemask: Option<u8>,
    zeroing: bool,
    pp: u8,
    w: bool,
    opcode: u8,
    needs_avx512vl: bool,
    needs_avx512dq: bool,
}

fn conversion_kind(
    map: u8,
    opcode: u8,
    pp: u8,
    w: bool,
) -> Option<(X86VecMap, X86EvexPackedConvertMemoryKind)> {
    use VecElementType::{F16, F32, F64, I32, I64};
    use X86EvexPackedConvertMemoryKind::{FpPrecision, FpToInt, IntToFp};

    let kind = match (map, opcode, pp, w) {
        (2, 0x13, 1, false) => FpPrecision { from: F16, to: F32 },
        (1, 0x5A, 0, false) => FpPrecision { from: F32, to: F64 },
        (1, 0x5A, 1, true) => FpPrecision { from: F64, to: F32 },

        (1, 0x5B, 0, false) => IntToFp {
            int_elem: I32,
            fp_elem: F32,
            signed: true,
        },
        (1, 0x5B, 0, true) => IntToFp {
            int_elem: I64,
            fp_elem: F32,
            signed: true,
        },
        (1, 0xE6, 2, false) => IntToFp {
            int_elem: I32,
            fp_elem: F64,
            signed: true,
        },
        (1, 0xE6, 2, true) => IntToFp {
            int_elem: I64,
            fp_elem: F64,
            signed: true,
        },
        (1, 0x7A, 3, false) => IntToFp {
            int_elem: I32,
            fp_elem: F32,
            signed: false,
        },
        (1, 0x7A, 3, true) => IntToFp {
            int_elem: I64,
            fp_elem: F32,
            signed: false,
        },
        (1, 0x7A, 2, false) => IntToFp {
            int_elem: I32,
            fp_elem: F64,
            signed: false,
        },
        (1, 0x7A, 2, true) => IntToFp {
            int_elem: I64,
            fp_elem: F64,
            signed: false,
        },

        (1, 0x5B, 1, false) => FpToInt {
            fp_elem: F32,
            int_elem: I32,
            signed: true,
            truncate: false,
        },
        (1, 0x5B, 2, false) => FpToInt {
            fp_elem: F32,
            int_elem: I32,
            signed: true,
            truncate: true,
        },
        (1, 0xE6, 3, true) => FpToInt {
            fp_elem: F64,
            int_elem: I32,
            signed: true,
            truncate: false,
        },
        (1, 0xE6, 1, true) => FpToInt {
            fp_elem: F64,
            int_elem: I32,
            signed: true,
            truncate: true,
        },
        (1, 0x7B, 1, false) => FpToInt {
            fp_elem: F32,
            int_elem: I64,
            signed: true,
            truncate: false,
        },
        (1, 0x7A, 1, false) => FpToInt {
            fp_elem: F32,
            int_elem: I64,
            signed: true,
            truncate: true,
        },
        (1, 0x7B, 1, true) => FpToInt {
            fp_elem: F64,
            int_elem: I64,
            signed: true,
            truncate: false,
        },
        (1, 0x7A, 1, true) => FpToInt {
            fp_elem: F64,
            int_elem: I64,
            signed: true,
            truncate: true,
        },
        (1, 0x79, 0, false) => FpToInt {
            fp_elem: F32,
            int_elem: I32,
            signed: false,
            truncate: false,
        },
        (1, 0x78, 0, false) => FpToInt {
            fp_elem: F32,
            int_elem: I32,
            signed: false,
            truncate: true,
        },
        (1, 0x79, 0, true) => FpToInt {
            fp_elem: F64,
            int_elem: I32,
            signed: false,
            truncate: false,
        },
        (1, 0x78, 0, true) => FpToInt {
            fp_elem: F64,
            int_elem: I32,
            signed: false,
            truncate: true,
        },
        (1, 0x79, 1, false) => FpToInt {
            fp_elem: F32,
            int_elem: I64,
            signed: false,
            truncate: false,
        },
        (1, 0x78, 1, false) => FpToInt {
            fp_elem: F32,
            int_elem: I64,
            signed: false,
            truncate: true,
        },
        (1, 0x79, 1, true) => FpToInt {
            fp_elem: F64,
            int_elem: I64,
            signed: false,
            truncate: false,
        },
        (1, 0x78, 1, true) => FpToInt {
            fp_elem: F64,
            int_elem: I64,
            signed: false,
            truncate: true,
        },
        _ => return None,
    };
    let map = match map {
        1 => X86VecMap::Map0F,
        2 => X86VecMap::Map0F38,
        _ => return None,
    };
    Some((map, kind))
}

fn exact_width(bytes: u32) -> VecWidth {
    match bytes {
        0..=8 => VecWidth::V64,
        9..=16 => VecWidth::V128,
        17..=32 => VecWidth::V256,
        _ => VecWidth::V512,
    }
}

fn register_width(bytes: u32) -> VecWidth {
    match bytes {
        0..=16 => VecWidth::V128,
        17..=32 => VecWidth::V256,
        _ => VecWidth::V512,
    }
}

fn packed_convert_fields(
    p0: u8,
    p1: u8,
    p2: u8,
    opcode: u8,
    modrm: u8,
) -> Option<PackedConvertFields> {
    let map_bits = p0 & 7;
    let pp = p1 & 3;
    let w = p1 & 0x80 != 0;
    let (map, kind) = conversion_kind(map_bits, opcode, pp, w)?;
    let ll = (p2 >> 5) & 3;
    let mask = p2 & 7;
    let zeroing = p2 & 0x80 != 0;
    if p1 & 0x78 != 0x78
        || p2 & 8 == 0
        || ll == 3
        || (zeroing && mask == 0)
        || (matches!(
            kind,
            X86EvexPackedConvertMemoryKind::FpPrecision {
                from: VecElementType::F16,
                to: VecElementType::F32
            }
        ) && p2 & 0x10 != 0)
    {
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
    let (lanes, source_bytes, destination_bytes) =
        if destination_elem.bytes() >= source_elem.bytes() {
            let lanes = operation_width.bytes() / destination_elem.bytes();
            (lanes, lanes * source_elem.bytes(), operation_width.bytes())
        } else {
            let lanes = operation_width.bytes() / source_elem.bytes();
            (
                lanes,
                operation_width.bytes(),
                lanes * destination_elem.bytes(),
            )
        };
    Some(PackedConvertFields {
        kind,
        map,
        operation_width,
        source_width: exact_width(source_bytes),
        destination_width: register_width(destination_bytes),
        lanes: u8::try_from(lanes).ok()?,
        destination: (u8::from(p0 & 0x80 == 0) << 3)
            | (u8::from(p0 & 0x10 == 0) << 4)
            | ((modrm >> 3) & 7),
        writemask: (mask != 0).then_some(mask),
        zeroing,
        pp,
        w,
        opcode,
        needs_avx512vl: operation_width != VecWidth::V512,
        needs_avx512dq: kind.needs_avx512dq(),
    })
}

fn register_packed_convert_fields(bytes: &[u8]) -> Option<PackedConvertFields> {
    let [0x62, p0, p1, p2, opcode, modrm] = bytes else {
        return None;
    };
    if modrm >> 6 != 3 || p2 & 0x10 != 0 {
        return None;
    }
    packed_convert_fields(*p0, *p1, *p2, *opcode, *modrm)
}

impl X86InstructionBytes {
    /// Validate an EVEX memory source for 27 packed F16/F32/F64/I32/I64
    /// precision and integer-conversion mnemonics and synthesize an exact
    /// helper-backed replay. Map-0F38 `VCVTPH2PS` uses the Type-E11 half-memory
    /// tuple and rejects memory-source `EVEX.b`; the other conversions use
    /// map 0F and their instruction-defined E2/E4/E5 tuple forms.
    ///
    /// Intel SDM revision 092 specifies 128-/256-/512-bit Type E11/E2/E4/E5
    /// memory forms with writemask fault suppression and, where defined, scalar
    /// broadcast. `vvvv/V'` are reserved. Segment/address-size prefixes and APX
    /// B4/X4 address channels remain confined to helper address evaluation.
    pub(crate) fn evex_packed_convert_memory_encoding(
        &self,
    ) -> Option<X86EvexPackedConvertMemoryEncoding> {
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
        let fields = packed_convert_fields(p0, p1, p2, opcode, modrm)?;
        let broadcast = p2 & 0x10 != 0;
        let scratch = (0..16u8)
            .find(|candidate| *candidate != fields.destination)
            .expect("one destination leaves at least fifteen low scratch registers");
        let register_probe = X86InstructionBytes::new(&[
            0x62,
            // Preserve R/R' and map, select scratch bits 4/3 with inverted
            // X/B polarity, and remove APX B4 from the helper-owned address.
            (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
            p1 | 0x04,
            p2 & !0x10,
            opcode,
            0xC0 | (modrm & 0x38) | (scratch & 7),
        ])?;
        if register_packed_convert_fields(register_probe.as_slice()) != Some(fields) {
            return None;
        }

        let stack_instruction = || {
            X86InstructionBytes::new(&[
                0x62,
                // Preserve destination R/R' and map, select ordinary RSP,
                // and clear APX B4/X4 address channels.
                (p0 & 0x97) | 0x60,
                p1 | 0x04,
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
            ])
            .expect("seven-byte EVEX stack replay")
        };
        let replay = if broadcast {
            X86EvexPackedConvertMemoryReplay::Broadcast {
                stack_instruction: stack_instruction(),
            }
        } else if fields.writemask.is_some() {
            X86EvexPackedConvertMemoryReplay::MaskedVector {
                stack_instruction: stack_instruction(),
            }
        } else {
            X86EvexPackedConvertMemoryReplay::Vector {
                scratch,
                register_instruction: register_probe,
            }
        };

        Some(X86EvexPackedConvertMemoryEncoding {
            kind: fields.kind,
            map: fields.map,
            operation_width: fields.operation_width,
            source_width: fields.source_width,
            destination_width: fields.destination_width,
            lanes: fields.lanes,
            destination: fields.destination,
            writemask: fields.writemask,
            zeroing: fields.zeroing,
            broadcast,
            pp: fields.pp,
            w: fields.w,
            opcode: fields.opcode,
            replay,
            needs_avx512vl: fields.needs_avx512vl,
            needs_avx512dq: fields.needs_avx512dq,
        })
    }
}
