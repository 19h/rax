//! EVEX scalar-lane and vector-chunk extraction to memory.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{MemWidth, VecElementType, VecWidth};

/// One complete unmasked EVEX.128 scalar extraction to memory, rewritten to
/// target preserved host RAX.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexScalarExtractMemoryEncoding {
    pub(crate) source: u8,
    pub(crate) lane: u8,
    pub(crate) elem: VecElementType,
    pub(crate) memory_width: MemWidth,
    pub(crate) w: bool,
    pub(crate) opcode: u8,
    pub(crate) immediate: u8,
    pub(crate) register_instruction: X86InstructionBytes,
    pub(crate) needs_avx512bw: bool,
    pub(crate) needs_avx512dq: bool,
}

/// One complete Type-E6NF EVEX vector-chunk extraction to memory, rewritten
/// to use private unextended `[rsp]` while retaining its architectural mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexChunkExtractMemoryEncoding {
    pub(crate) source_width: VecWidth,
    pub(crate) chunk_width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) source: u8,
    pub(crate) first_lane: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) w: bool,
    pub(crate) opcode: u8,
    pub(crate) immediate: u8,
    pub(crate) stack_instruction: X86InstructionBytes,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512dq: bool,
}

#[derive(Clone, Copy)]
struct EvexMemoryImm8Fields {
    p0: u8,
    p1: u8,
    p2: u8,
    opcode: u8,
    modrm: u8,
    immediate: u8,
}

impl EvexMemoryImm8Fields {
    fn source(self) -> u8 {
        (u8::from(self.p0 & 0x80 == 0) << 3)
            | (u8::from(self.p0 & 0x10 == 0) << 4)
            | ((self.modrm >> 3) & 7)
    }

    fn stack_instruction(self) -> X86InstructionBytes {
        X86InstructionBytes::new(&[
            0x62,
            // Preserve R/R' and map 0F3A, select unextended RSP, and clear
            // APX B4/X4 address state.
            (self.p0 & 0x97) | 0x60,
            self.p1 | 0x04,
            self.p2,
            self.opcode,
            (self.modrm & 0x38) | 0x04,
            0x24,
            self.immediate,
        ])
        .expect("eight-byte EVEX stack extraction")
    }

    fn rax_instruction(self) -> X86InstructionBytes {
        X86InstructionBytes::new(&[
            0x62,
            // Preserve R/R' and map 0F3A, select architectural RAX, and
            // remove memory-only APX address extensions.
            (self.p0 & 0x97) | 0x60,
            self.p1 | 0x04,
            self.p2,
            self.opcode,
            0xC0 | (self.modrm & 0x38),
            self.immediate,
        ])
        .expect("seven-byte EVEX scalar extraction")
    }
}

impl X86InstructionBytes {
    fn evex_memory_imm8_fields(&self) -> Option<EvexMemoryImm8Fields> {
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
        if operand_end.checked_add(1)? != bytes.len() {
            return None;
        }
        Some(EvexMemoryImm8Fields {
            p0,
            p1,
            p2,
            opcode,
            modrm,
            immediate: bytes[operand_end],
        })
    }

    /// Validate and rewrite one Type-E9NF EVEX scalar extraction whose
    /// destination is memory.
    ///
    /// `VEXTRACTPS` and `VPEXTRB/W/D/Q` are EVEX.128.66.0F3A, reserve
    /// EVEX.vvvv/V', EVEX.b, every mask control, and EVEX.L'L, and use a
    /// Tuple1 Scalar 1-/2-/4-/8-byte destination. W is ignored for byte,
    /// word, and `VEXTRACTPS` forms. Segment, address-size, and APX B4/X4
    /// controls remain confined to guest helper address evaluation.
    pub(crate) fn evex_scalar_extract_memory_encoding(
        &self,
    ) -> Option<X86EvexScalarExtractMemoryEncoding> {
        let fields = self.evex_memory_imm8_fields()?;
        if fields.p0 & 7 != 3 || fields.p1 & 3 != 1 || fields.p1 & 0x78 != 0x78 || fields.p2 != 0x08
        {
            return None;
        }
        let w = fields.p1 & 0x80 != 0;
        let (elem, memory_width, lane_mask, needs_avx512bw, needs_avx512dq) =
            match (fields.opcode, w) {
                (0x14, _) => (VecElementType::I8, MemWidth::B1, 0x0F, true, false),
                (0x15, _) => (VecElementType::I16, MemWidth::B2, 0x07, true, false),
                (0x16, false) => (VecElementType::I32, MemWidth::B4, 0x03, false, true),
                (0x16, true) => (VecElementType::I64, MemWidth::B8, 0x01, false, true),
                (0x17, _) => (VecElementType::I32, MemWidth::B4, 0x03, false, false),
                _ => return None,
            };
        Some(X86EvexScalarExtractMemoryEncoding {
            source: fields.source(),
            lane: fields.immediate & lane_mask,
            elem,
            memory_width,
            w,
            opcode: fields.opcode,
            immediate: fields.immediate,
            register_instruction: fields.rax_instruction(),
            needs_avx512bw,
            needs_avx512dq,
        })
    }

    /// Validate and rewrite one Type-E6NF EVEX chunk extraction whose
    /// destination is memory.
    ///
    /// Opcodes 19H/39H select a 128-bit F/I chunk from a 256- or 512-bit
    /// source; 1BH/3BH select a 256-bit F/I chunk from a 512-bit source. W
    /// selects 32- or 64-bit mask granularity. EVEX.vvvv/V', EVEX.b, and
    /// EVEX.z are reserved. The rewritten private-stack instruction retains
    /// the architectural writemask because E6NF requires a complete
    /// destination read/merge/write rather than per-lane fault suppression.
    pub(crate) fn evex_chunk_extract_memory_encoding(
        &self,
    ) -> Option<X86EvexChunkExtractMemoryEncoding> {
        let fields = self.evex_memory_imm8_fields()?;
        let ll = (fields.p2 >> 5) & 3;
        let half_chunk = matches!(fields.opcode, 0x1B | 0x3B);
        if fields.p0 & 7 != 3
            || fields.p1 & 3 != 1
            || fields.p1 & 0x78 != 0x78
            || fields.p2 & 0x98 != 0x08
            || !matches!(fields.opcode, 0x19 | 0x1B | 0x39 | 0x3B)
            || !matches!((half_chunk, ll), (false, 1 | 2) | (true, 2))
        {
            return None;
        }

        let w = fields.p1 & 0x80 != 0;
        let source_width = if ll == 1 {
            VecWidth::V256
        } else {
            VecWidth::V512
        };
        let chunk_width = if half_chunk {
            VecWidth::V256
        } else {
            VecWidth::V128
        };
        let elem = match (fields.opcode < 0x30, w) {
            (true, false) => VecElementType::F32,
            (true, true) => VecElementType::F64,
            (false, false) => VecElementType::I32,
            (false, true) => VecElementType::I64,
        };
        let chunks = u8::try_from(source_width.bytes() / chunk_width.bytes())
            .expect("at most four EVEX extraction chunks");
        let chunk_lanes =
            u8::try_from(chunk_width.lanes(elem)).expect("at most eight EVEX extraction lanes");
        let mask = fields.p2 & 7;
        Some(X86EvexChunkExtractMemoryEncoding {
            source_width,
            chunk_width,
            elem,
            source: fields.source(),
            first_lane: (fields.immediate & (chunks - 1)) * chunk_lanes,
            writemask: (mask != 0).then_some(mask),
            w,
            opcode: fields.opcode,
            immediate: fields.immediate,
            stack_instruction: fields.stack_instruction(),
            needs_avx512vl: source_width != VecWidth::V512,
            needs_avx512dq: w != half_chunk,
        })
    }
}
