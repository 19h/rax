//! VEX register and EVEX helper-backed packed-move replay classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Direction of one exact writemasked EVEX packed-move memory transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexPackedMoveMemoryKind {
    Load,
    Store,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EvexPackedMoveMemoryFields {
    kind: X86EvexPackedMoveMemoryKind,
    width: VecWidth,
    elem: VecElementType,
    vector: u8,
    writemask: Option<u8>,
    zeroing: bool,
    aligned: bool,
    needs_avx512bw: bool,
}

/// Exact writemasked EVEX packed-move memory encoding and its byte-validated
/// unaligned private-stack replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexPackedMoveMemoryEncoding {
    pub(crate) kind: X86EvexPackedMoveMemoryKind,
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) vector: u8,
    pub(crate) writemask: u8,
    pub(crate) zeroing: bool,
    pub(crate) alignment: Option<u8>,
    pub(crate) stack_instruction: X86InstructionBytes,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512bw: bool,
}

fn evex_width(ll: u8) -> Option<VecWidth> {
    match ll {
        0 => Some(VecWidth::V128),
        1 => Some(VecWidth::V256),
        2 => Some(VecWidth::V512),
        _ => None,
    }
}

fn evex_packed_move_operation(
    opcode: u8,
    pp: u8,
    w: bool,
) -> Option<(X86EvexPackedMoveMemoryKind, VecElementType, bool, u8, u8)> {
    let kind = match opcode {
        0x10 | 0x28 | 0x6F => X86EvexPackedMoveMemoryKind::Load,
        0x11 | 0x29 | 0x7F => X86EvexPackedMoveMemoryKind::Store,
        _ => return None,
    };
    let stack_opcode = match kind {
        X86EvexPackedMoveMemoryKind::Load => 0x10,
        X86EvexPackedMoveMemoryKind::Store => 0x11,
    };
    Some(match (opcode, pp, w) {
        // VMOVUPS/UPD.
        (0x10 | 0x11, 0, false) => (kind, VecElementType::F32, false, stack_opcode, 0),
        (0x10 | 0x11, 1, true) => (kind, VecElementType::F64, false, stack_opcode, 1),
        // VMOVAPS/APD; the private replay uses VMOVUPS/UPD after the guest
        // alignment precondition has been checked independently.
        (0x28 | 0x29, 0, false) => (kind, VecElementType::F32, true, stack_opcode, 0),
        (0x28 | 0x29, 1, true) => (kind, VecElementType::F64, true, stack_opcode, 1),
        // VMOVDQA32/64; the private replay uses VMOVDQU32/64 (F3 prefix).
        (0x6F | 0x7F, 1, false) => (kind, VecElementType::I32, true, opcode, 2),
        (0x6F | 0x7F, 1, true) => (kind, VecElementType::I64, true, opcode, 2),
        // VMOVDQU32/64 and VMOVDQU8/16.
        (0x6F | 0x7F, 2, false) => (kind, VecElementType::I32, false, opcode, 2),
        (0x6F | 0x7F, 2, true) => (kind, VecElementType::I64, false, opcode, 2),
        (0x6F | 0x7F, 3, false) => (kind, VecElementType::I8, false, opcode, 3),
        (0x6F | 0x7F, 3, true) => (kind, VecElementType::I16, false, opcode, 3),
        _ => return None,
    })
}

fn evex_packed_move_memory_fields(
    bytes: &[u8],
) -> Option<(u8, u8, u8, u8, u8, u8, EvexPackedMoveMemoryFields)> {
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
    let pp = p1 & 3;
    let w = p1 & 0x80 != 0;
    let (kind, elem, aligned, stack_opcode, stack_pp) = evex_packed_move_operation(opcode, pp, w)?;
    let width = evex_width((p2 >> 5) & 3)?;
    let mask = p2 & 7;
    let zeroing = p2 & 0x80 != 0;

    // EVEX map 0F, reserved vvvv/V'=11111b, b=0, and a complete memory
    // ModR/M operand are invariant. Payload-byte-1 bit 2 and payload-byte-0
    // bit 3 may encode APX X4/B4 for a memory address, so neither is treated
    // as a fixed ordinary-EVEX bit here.
    if p0 & 7 != 1
        || p1 & 0x78 != 0x78
        || p2 & 0x08 == 0
        || p2 & 0x10 != 0
        || modrm >> 6 == 3
        || (zeroing && mask == 0)
        || (kind == X86EvexPackedMoveMemoryKind::Store && zeroing)
        || memory_operand_end(bytes, modrm_index)? != bytes.len()
    {
        return None;
    }

    Some((
        p0,
        p1,
        p2,
        modrm,
        stack_opcode,
        stack_pp,
        EvexPackedMoveMemoryFields {
            kind,
            width,
            elem,
            vector: (u8::from(p0 & 0x80 == 0) << 3)
                | (u8::from(p0 & 0x10 == 0) << 4)
                | ((modrm >> 3) & 7),
            writemask: (mask != 0).then_some(mask),
            zeroing,
            aligned,
            needs_avx512bw: matches!(elem, VecElementType::I8 | VecElementType::I16),
        },
    ))
}

impl X86InstructionBytes {
    fn is_vex_register_packed_move_with_opcodes_and_prefixes(
        &self,
        load_opcode: u8,
        store_opcode: u8,
        prefixes: [u8; 2],
    ) -> bool {
        let bytes = self.as_slice();
        let (p1, opcode, modrm) = match bytes {
            [0xC5, p1, opcode, modrm] => (*p1, *opcode, *modrm),
            [0xC4, p0, p1, opcode, modrm] if p0 & 0x1F == 1 => (*p1, *opcode, *modrm),
            _ => return false,
        };

        p1 & 0x78 == 0x78
            && prefixes.contains(&(p1 & 0x03))
            && (opcode == load_opcode || opcode == store_opcode)
            && modrm >> 6 == 3
    }

    fn vex_register_packed_move_destination_index(
        &self,
        load_opcode: u8,
        store_opcode: u8,
        prefixes: [u8; 2],
    ) -> Option<u8> {
        if !self.is_vex_register_packed_move_with_opcodes_and_prefixes(
            load_opcode,
            store_opcode,
            prefixes,
        ) {
            return None;
        }
        let (reg_extension, rm_extension, opcode, modrm) = match self.as_slice() {
            [0xC5, p1, opcode, modrm] => (p1 & 0x80 == 0, false, *opcode, *modrm),
            [0xC4, p0, _, opcode, modrm] => (p0 & 0x80 == 0, p0 & 0x20 == 0, *opcode, *modrm),
            _ => unreachable!("VEX packed move shape was validated"),
        };
        let destination = if opcode == load_opcode {
            ((modrm >> 3) & 7) + if reg_extension { 8 } else { 0 }
        } else {
            debug_assert_eq!(opcode, store_opcode);
            (modrm & 7) + if rm_extension { 8 } else { 0 }
        };
        Some(destination)
    }

    /// Validate one register-only VEX `VMOVAPS` or `VMOVAPD` instruction in
    /// either opcode direction.
    ///
    /// Both VEX.128 and VEX.256 forms require AVX. C5 and C4 encodings are
    /// accepted; C4.W and C4.X are ignored for register operands. VEX.vvvv is
    /// reserved and must be encoded as `1111b`. Memory forms and every
    /// noncanonical byte shape fail closed.
    pub fn is_vex_register_aligned_packed_fp_move(&self) -> bool {
        self.is_vex_register_packed_move_with_opcodes_and_prefixes(0x28, 0x29, [0, 1])
    }

    /// Return the architectural destination register after exact validation.
    /// Opcode `28h` writes ModR/M.reg; opcode `29h` writes ModR/M.r/m. The
    /// AVX-only state bridge uses the result to clear the destination's
    /// state-backed ZMM[511:256] after the replayed VEX instruction zeros its
    /// architectural upper state.
    pub(crate) fn vex_aligned_packed_fp_move_destination_index(&self) -> Option<u8> {
        self.vex_register_packed_move_destination_index(0x28, 0x29, [0, 1])
    }

    /// Validate one register-only VEX `VMOVUPS` or `VMOVUPD` instruction in
    /// either opcode direction.
    ///
    /// Both VEX.128 and VEX.256 forms require AVX. C5 and C4 encodings are
    /// accepted; C4.W and C4.X are ignored for register operands. VEX.vvvv is
    /// reserved and must be encoded as `1111b`. Memory forms and every
    /// noncanonical byte shape fail closed.
    pub fn is_vex_register_unaligned_packed_fp_move(&self) -> bool {
        self.is_vex_register_packed_move_with_opcodes_and_prefixes(0x10, 0x11, [0, 1])
    }

    /// Return the architectural destination register after exact validation.
    /// Opcode `10h` writes ModR/M.reg; opcode `11h` writes ModR/M.r/m. The
    /// AVX-only state bridge uses the result to clear the destination's
    /// state-backed ZMM[511:256] after the replayed VEX instruction zeros its
    /// architectural upper state.
    pub(crate) fn vex_unaligned_packed_fp_move_destination_index(&self) -> Option<u8> {
        self.vex_register_packed_move_destination_index(0x10, 0x11, [0, 1])
    }

    /// Validate one register-only VEX `VMOVDQA` or `VMOVDQU` instruction in
    /// either opcode direction.
    ///
    /// Both VEX.128 and VEX.256 forms require AVX. C5 and C4 encodings are
    /// accepted; C4.W and C4.X are ignored for register operands. VEX.vvvv is
    /// reserved and must be encoded as `1111b`. Memory forms and every
    /// noncanonical byte shape fail closed.
    pub fn is_vex_register_packed_integer_move(&self) -> bool {
        self.is_vex_register_packed_move_with_opcodes_and_prefixes(0x6F, 0x7F, [1, 2])
    }

    /// Return the architectural destination register after exact validation.
    /// Opcode `6Fh` writes ModR/M.reg; opcode `7Fh` writes ModR/M.r/m. The
    /// AVX-only state bridge uses the result to clear the destination's
    /// state-backed ZMM[511:256] after the replayed VEX instruction zeros its
    /// architectural upper state.
    pub(crate) fn vex_packed_integer_move_destination_index(&self) -> Option<u8> {
        self.vex_register_packed_move_destination_index(0x6F, 0x7F, [1, 2])
    }

    /// Validate register-only EVEX packed moves and return whether the vector
    /// length requires AVX-512VL. This covers VMOVUPS/UPD, VMOVAPS/APD,
    /// VMOVDQA32/64, and VMOVDQU8/16/32/64 in both opcode directions. Exact
    /// mandatory-prefix and W combinations distinguish all ten mnemonics.
    /// Reserved EVEX.vvvv/V', EVEX.b, vector length, masking, and memory forms
    /// fail closed; aligned forms are safe because no memory is admitted.
    pub fn evex_register_packed_move_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        if p0 & 0x0F != 1
            || p1 & 0x04 == 0
            || p1 & 0x78 != 0x78
            || p2 & 0x08 == 0
            || modrm >> 6 != 3
        {
            return None;
        }

        let pp = p1 & 3;
        let w = p1 & 0x80 != 0;
        match (opcode, pp, w) {
            (0x10 | 0x11 | 0x28 | 0x29, 0, false)
            | (0x10 | 0x11 | 0x28 | 0x29, 1, true)
            | (0x6F | 0x7F, 1..=3, _) => {}
            _ => return None,
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 3;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 7;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    /// Validate one complete writemasked EVEX packed move with a memory
    /// operand and synthesize its exact private-stack replay.
    ///
    /// Intel SDM revision 092 assigns VMOVAPS/APD and VMOVDQA32/64 to Type E1:
    /// their 16/32/64-byte alignment check is unconditional, while inactive
    /// writemask lanes suppress address/page accesses. The native lowerer
    /// therefore checks guest alignment separately and uses an unaligned
    /// VMOVUPS/UPD or VMOVDQU stack encoding. Loads preserve the source mask
    /// and merge/zero control. Stores clear the private replay mask, snapshot
    /// the complete source vector, and apply the architectural mask only to
    /// per-lane guest helper stores. Segment, address-size, and APX B4/X4
    /// controls remain confined to helper address evaluation.
    pub(crate) fn evex_packed_move_memory_encoding(
        &self,
    ) -> Option<X86EvexPackedMoveMemoryEncoding> {
        let (p0, p1, p2, modrm, stack_opcode, stack_pp, fields) =
            evex_packed_move_memory_fields(self.as_slice())?;
        let writemask = fields.writemask?;
        let stack_p2 = match fields.kind {
            X86EvexPackedMoveMemoryKind::Load => p2,
            X86EvexPackedMoveMemoryKind::Store => p2 & !0x87,
        };
        let stack_instruction = X86InstructionBytes::new(&[
            0x62,
            // Preserve vector R/R' and map 0F; select ordinary unextended
            // RSP and clear APX X4/B4 from the helper-owned guest address.
            (p0 & 0x97) | 0x60,
            // Preserve W/vvvv, select the unaligned operation prefix, and
            // restore ordinary EVEX.U after removing APX X4.
            (p1 & !3) | stack_pp | 0x04,
            stack_p2,
            stack_opcode,
            (modrm & 0x38) | 0x04,
            0x24,
        ])?;
        let (_, _, _, _, _, _, rewritten) =
            evex_packed_move_memory_fields(stack_instruction.as_slice())?;
        let mut expected = fields;
        expected.aligned = false;
        if fields.kind == X86EvexPackedMoveMemoryKind::Store {
            expected.writemask = None;
            expected.zeroing = false;
        }
        if rewritten != expected {
            return None;
        }

        Some(X86EvexPackedMoveMemoryEncoding {
            kind: fields.kind,
            width: fields.width,
            elem: fields.elem,
            vector: fields.vector,
            writemask,
            zeroing: fields.zeroing,
            alignment: fields
                .aligned
                .then_some(u8::try_from(fields.width.bytes()).ok()?),
            stack_instruction,
            needs_avx512vl: fields.width != VecWidth::V512,
            needs_avx512bw: fields.needs_avx512bw,
        })
    }
}
