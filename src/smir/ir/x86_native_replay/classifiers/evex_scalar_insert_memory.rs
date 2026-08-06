//! Complete Type-E9NF EVEX scalar-insert memory-source classification.

use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use super::{X86InstructionBytes, X86ScalarInsertMemoryKind};

/// One byte-validated EVEX.128 scalar insertion whose source is memory.
///
/// `register_instruction` replaces the helper-owned memory operand with either
/// a private low XMM scratch register (`VINSERTPS`) or preserved host RAX
/// (`VPINSR*`). Segment, address-size, and APX B4/X4 controls occur only in the
/// precise helper address evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexScalarInsertMemoryEncoding {
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) kind: X86ScalarInsertMemoryKind,
    pub(crate) immediate: u8,
    pub(crate) w: bool,
    pub(crate) scratch: u8,
    pub(crate) register_instruction: X86InstructionBytes,
    pub(crate) needs_avx512bw: bool,
    pub(crate) needs_avx512dq: bool,
}

impl X86InstructionBytes {
    /// Validate and rewrite one EVEX.128 `VINSERTPS`, `VPINSRB`, `VPINSRW`,
    /// `VPINSRD`, or `VPINSRQ` Type-E9NF memory source.
    ///
    /// Every form requires 66H, L'L=0, `aaa=000`, `z=0`, and `b=0`.
    /// `VINSERTPS` requires W0; W is ignored for byte/word insertion and
    /// selects dword/quadword insertion for opcode 22H. The scalar access is
    /// unconditional, including when `VINSERTPS.imm8[3:0]` zeroes the selected
    /// destination lane. The fixed 15-byte architectural instruction bound
    /// makes classification O(1) time and O(1) space.
    pub(crate) fn evex_scalar_insert_memory_encoding(
        &self,
    ) -> Option<X86EvexScalarInsertMemoryEncoding> {
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
        if operand_end.checked_add(1)? != bytes.len() || p1 & 3 != 1 || p2 & !0x08 != 0 {
            return None;
        }

        let w = p1 & 0x80 != 0;
        let kind = match (p0 & 7, opcode, w) {
            (1, 0xC4, _) => X86ScalarInsertMemoryKind::Vpinsrw,
            (3, 0x20, _) => X86ScalarInsertMemoryKind::Vpinsrb,
            (3, 0x21, false) => X86ScalarInsertMemoryKind::Vinsertps,
            (3, 0x22, false) => X86ScalarInsertMemoryKind::Vpinsrd,
            (3, 0x22, true) => X86ScalarInsertMemoryKind::Vpinsrq,
            _ => return None,
        };
        let destination =
            (u8::from(p0 & 0x80 == 0) << 3) | (u8::from(p0 & 0x10 == 0) << 4) | ((modrm >> 3) & 7);
        let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let scratch = (0..16u8)
            .find(|candidate| *candidate != destination && *candidate != source1)
            .expect("two EVEX operands leave a low vector scratch register");
        let register_source = if kind == X86ScalarInsertMemoryKind::Vinsertps {
            scratch
        } else {
            0 // preserved host RAX
        };
        let register_immediate = if kind == X86ScalarInsertMemoryKind::Vinsertps {
            // Memory INSERTPS has no Count_S selector. Scratch lane zero owns
            // the loaded dword, so select it while preserving Count_D/zeroing.
            immediate & 0x3F
        } else {
            immediate
        };
        let register_instruction = X86InstructionBytes::new(&[
            0x62,
            // Preserve R/R' and the opcode map. Reconstruct register-direct
            // X/B polarity and clear helper-owned APX B4.
            (p0 & 0x97) | 0x40 | if register_source & 8 == 0 { 0x20 } else { 0 },
            // Preserve W/vvvv/66 and remove helper-owned APX X4.
            p1 | 0x04,
            p2,
            opcode,
            0xC0 | ((destination & 7) << 3) | (register_source & 7),
            register_immediate,
        ])?;
        let needs_avx512bw = matches!(
            kind,
            X86ScalarInsertMemoryKind::Vpinsrb | X86ScalarInsertMemoryKind::Vpinsrw
        );
        let needs_avx512dq = matches!(
            kind,
            X86ScalarInsertMemoryKind::Vpinsrd | X86ScalarInsertMemoryKind::Vpinsrq
        );
        if register_instruction.evex_register_scalar_lane_transfer_requires_dq()
            != Some(needs_avx512dq)
        {
            return None;
        }

        Some(X86EvexScalarInsertMemoryEncoding {
            destination,
            source1,
            kind,
            immediate,
            w,
            scratch,
            register_instruction,
            needs_avx512bw,
            needs_avx512dq,
        })
    }
}
