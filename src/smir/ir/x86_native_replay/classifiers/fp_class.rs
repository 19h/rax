//! EVEX `VFPCLASS*` replay classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{MemWidth, VecElementType, VecWidth};

/// Native replay strategy for one exact `VFPCLASS*` memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexFpClassMemoryReplay {
    /// A complete vector helper load followed by a register-source rewrite.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// One scalar helper load followed by a memory-broadcast `[rsp]` replay.
    Broadcast {
        stack_instruction: X86InstructionBytes,
    },
    /// Per-active-lane helper loads followed by a full-vector `[rsp]` replay.
    MaskedVector {
        stack_instruction: X86InstructionBytes,
    },
    /// One optionally predicated scalar helper load and a scalar `[rsp]` replay.
    Scalar {
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact EVEX `VFPCLASS*` memory encoding and byte-validated native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexFpClassMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) immediate: u8,
    pub(crate) scalar: bool,
    pub(crate) memory_width: MemWidth,
    pub(crate) replay: X86EvexFpClassMemoryReplay,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512dq: bool,
    pub(crate) needs_avx512fp16: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RegisterFields {
    width: VecWidth,
    elem: VecElementType,
    destination: u8,
    source: u8,
    writemask: Option<u8>,
    immediate: u8,
    scalar: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MemoryFields {
    width: VecWidth,
    elem: VecElementType,
    destination: u8,
    writemask: Option<u8>,
    immediate: u8,
    scalar: bool,
    broadcast: bool,
    memory_width: MemWidth,
}

fn fp_class_element(pp: u8, w: bool) -> Option<VecElementType> {
    match (pp, w) {
        (0, false) => Some(VecElementType::F16),
        (1, false) => Some(VecElementType::F32),
        (1, true) => Some(VecElementType::F64),
        _ => None,
    }
}

fn memory_width(elem: VecElementType) -> MemWidth {
    match elem {
        VecElementType::F16 => MemWidth::B2,
        VecElementType::F32 => MemWidth::B4,
        VecElementType::F64 => MemWidth::B8,
        _ => unreachable!("validated VFPCLASS element"),
    }
}

impl X86InstructionBytes {
    fn evex_register_fp_class_fields(&self) -> Option<RegisterFields> {
        let [0x62, p0, p1, p2, opcode @ (0x66 | 0x67), modrm, immediate] = self.as_slice() else {
            return None;
        };
        let scalar = *opcode == 0x67;
        let ll = (p2 >> 5) & 3;
        let elem = fp_class_element(p1 & 3, p1 & 0x80 != 0)?;
        if p0 & 0x0F != 3
            || p0 & 0x90 != 0x90
            || p1 & 0x04 == 0
            || p1 & 0x78 != 0x78
            || p2 & 0x08 == 0
            || p2 & 0x90 != 0
            || (!scalar && ll == 3)
            || modrm >> 6 != 3
        {
            return None;
        }
        let width = match (scalar, ll) {
            (true, _) | (false, 0) => VecWidth::V128,
            (false, 1) => VecWidth::V256,
            (false, 2) => VecWidth::V512,
            (false, _) => unreachable!("reserved packed vector length rejected"),
        };
        let mask = p2 & 7;
        Some(RegisterFields {
            width,
            elem,
            destination: (modrm >> 3) & 7,
            source: (modrm & 7) | (u8::from(p0 & 0x20 == 0) << 3) | (u8::from(p0 & 0x40 == 0) << 4),
            writemask: (mask != 0).then_some(mask),
            immediate: *immediate,
            scalar,
        })
    }

    /// Validate one register-only EVEX VFPCLASS* instruction.
    ///
    /// Returns `(needs_avx512vl, needs_avx512dq, needs_avx512fp16)`. Packed
    /// 128-bit and 256-bit forms need AVX-512VL; binary32/binary64 forms need
    /// AVX-512DQ; binary16 forms need AVX-512-FP16. Scalar L'L is ignored and
    /// never creates an AVX-512VL requirement. Memory forms and every reserved
    /// EVEX field fail closed.
    pub fn evex_register_fp_class_requirements(&self) -> Option<(bool, bool, bool)> {
        let fields = self.evex_register_fp_class_fields()?;
        Some((
            !fields.scalar && fields.width != VecWidth::V512,
            fields.elem != VecElementType::F16,
            fields.elem == VecElementType::F16,
        ))
    }

    /// Return the equivalent L'L=00 host image for a register-only scalar
    /// `VFPCLASS*`. Although scalar L'L is architecturally ignored, some hosts
    /// reject nonzero encodings during native execution.
    pub(crate) fn evex_scalar_fp_class_llig_canonical_ll0(&self) -> Option<Self> {
        let fields = self.evex_register_fp_class_fields()?;
        if !fields.scalar {
            return None;
        }

        let mut canonical = *self;
        canonical.bytes[3] &= !0x60;
        debug_assert_eq!(canonical.evex_register_fp_class_fields(), Some(fields));
        Some(canonical)
    }

    fn evex_fp_class_memory_fields(&self) -> Option<(u8, u8, u8, u8, MemoryFields)> {
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
        let scalar = opcode == 0x67;
        let ll = (p2 >> 5) & 3;
        let elem = fp_class_element(p1 & 3, p1 & 0x80 != 0)?;
        let broadcast = p2 & 0x10 != 0;
        if p0 & 7 != 3
            || p0 & 0x90 != 0x90
            || p1 & 0x78 != 0x78
            || p2 & 0x08 == 0
            || p2 & 0x80 != 0
            || !matches!(opcode, 0x66 | 0x67)
            || modrm >> 6 == 3
            || (!scalar && ll == 3)
            || (scalar && broadcast)
            || operand_end + 1 != bytes.len()
        {
            return None;
        }
        let width = match (scalar, ll) {
            (true, _) | (false, 0) => VecWidth::V128,
            (false, 1) => VecWidth::V256,
            (false, 2) => VecWidth::V512,
            (false, _) => unreachable!("reserved packed vector length rejected"),
        };
        let mask = p2 & 7;
        Some((
            p0,
            p1,
            p2,
            modrm,
            MemoryFields {
                width,
                elem,
                destination: (modrm >> 3) & 7,
                writemask: (mask != 0).then_some(mask),
                immediate,
                scalar,
                broadcast,
                memory_width: memory_width(elem),
            },
        ))
    }

    /// Validate one packed or scalar EVEX `VFPCLASS*` memory source and
    /// synthesize an exact helper-backed native replay.
    ///
    /// Intel SDM revision 092 specifies packed forms as Type E4 with optional
    /// 2/4/8-byte embedded broadcast, and scalar forms as Type E6/E10 Tuple1
    /// Scalar. `EVEX.z`, vvvv/V', extended K destinations, packed L'L=3, and
    /// scalar `EVEX.b` are rejected. Only active writemask lanes access memory;
    /// scalar forms use bit 0 and replay with the equivalent L'L=00 encoding.
    /// Segment/address-size prefixes and APX B4/X4 controls remain confined to
    /// helper address evaluation.
    pub(crate) fn evex_fp_class_memory_encoding(&self) -> Option<X86EvexFpClassMemoryEncoding> {
        let (p0, p1, p2, modrm, fields) = self.evex_fp_class_memory_fields()?;
        let opcode = if fields.scalar { 0x67 } else { 0x66 };
        let stack_instruction = || {
            X86InstructionBytes::new(&[
                0x62,
                // Preserve fixed R/R' and map, select ordinary RSP/SIB, and
                // remove the helper-owned APX B4 address channel.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/pp and remove APX X4 from the stack address.
                p1 | 0x04,
                if fields.scalar { p2 & !0x60 } else { p2 },
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
                fields.immediate,
            ])
            .expect("VFPCLASS stack replay is eight bytes")
        };

        let expected_stack_fields = MemoryFields {
            broadcast: fields.broadcast,
            ..fields
        };
        let (_, _, _, _, rewritten_fields) = stack_instruction().evex_fp_class_memory_fields()?;
        if rewritten_fields != expected_stack_fields {
            return None;
        }

        let replay = if fields.scalar {
            X86EvexFpClassMemoryReplay::Scalar {
                stack_instruction: stack_instruction(),
            }
        } else if fields.broadcast {
            X86EvexFpClassMemoryReplay::Broadcast {
                stack_instruction: stack_instruction(),
            }
        } else if fields.writemask.is_some() {
            X86EvexFpClassMemoryReplay::MaskedVector {
                stack_instruction: stack_instruction(),
            }
        } else {
            let scratch = 0;
            let register_instruction = X86InstructionBytes::new(&[
                0x62,
                // Register EVEX.X/B encode source bits 4/3 with inverted
                // polarity; scratch ZMM0 therefore sets both bits.
                (p0 & 0x97) | 0x60,
                p1 | 0x04,
                p2,
                opcode,
                0xC0 | (modrm & 0x38) | scratch,
                fields.immediate,
            ])?;
            let expected = RegisterFields {
                width: fields.width,
                elem: fields.elem,
                destination: fields.destination,
                source: scratch,
                writemask: fields.writemask,
                immediate: fields.immediate,
                scalar: fields.scalar,
            };
            if register_instruction.evex_register_fp_class_fields() != Some(expected) {
                return None;
            }
            X86EvexFpClassMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexFpClassMemoryEncoding {
            width: fields.width,
            elem: fields.elem,
            destination: fields.destination,
            writemask: fields.writemask,
            immediate: fields.immediate,
            scalar: fields.scalar,
            memory_width: fields.memory_width,
            replay,
            needs_avx512vl: !fields.scalar && fields.width != VecWidth::V512,
            needs_avx512dq: fields.elem != VecElementType::F16,
            needs_avx512fp16: fields.elem == VecElementType::F16,
        })
    }
}
