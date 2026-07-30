//! Register-only AVX VEX packed-string comparison replay.

use super::X86InstructionBytes;
use crate::smir::ir::ops::X86PackedStringKind;
use crate::smir::ir::types::OpWidth;

/// One complete VEX packed-string memory encoding rewritten to consume a
/// helper-loaded value from a nonarchitectural low XMM register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86VexPackedStringMemoryEncoding {
    pub(crate) kind: X86PackedStringKind,
    pub(crate) source1: u8,
    pub(crate) scratch: u8,
    pub(crate) immediate: u8,
    pub(crate) length_width: OpWidth,
    pub(crate) memory_size: u32,
    pub(crate) register_instruction: X86InstructionBytes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct X86VexPackedStringRegisterEncoding {
    kind: X86PackedStringKind,
    source1: u8,
    source2: u8,
    immediate: u8,
    length_width: OpWidth,
}

impl X86InstructionBytes {
    fn vex_register_packed_string_encoding(&self) -> Option<X86VexPackedStringRegisterEncoding> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0xC4 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let opcode = bytes[3];
        let modrm = bytes[4];
        if p0 & 0x1F != 3 || p1 & 0x7F != 0x79 || modrm >> 6 != 3 {
            return None;
        }
        let kind = match opcode {
            0x60 => X86PackedStringKind::ExplicitMask,
            0x61 => X86PackedStringKind::ExplicitIndex,
            0x62 => X86PackedStringKind::ImplicitMask,
            0x63 => X86PackedStringKind::ImplicitIndex,
            _ => return None,
        };
        Some(X86VexPackedStringRegisterEncoding {
            kind,
            source1: (u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
            source2: (u8::from(p0 & 0x20 == 0) << 3) | (modrm & 7),
            immediate: bytes[5],
            length_width: if kind.is_explicit() && p1 & 0x80 != 0 {
                OpWidth::W64
            } else {
                OpWidth::W32
            },
        })
    }

    /// Validate one register-only VEX.128 `VPCMPxSTRx` instruction.
    ///
    /// Intel SDM Vol. 2B assigns opcodes 60H through 63H in map 0F3A with
    /// mandatory 66H, VEX.L=0, and reserved VEX.vvvv=1111b. Both VEX.W values
    /// are valid: W selects 32- versus 64-bit explicit lengths and is ignored
    /// by implicit-length forms. R and B may select XMM0 through XMM15; X is
    /// ignored for a register ModR/M operand. Memory forms remain excluded so
    /// native replay cannot bypass guest-memory translation or fault handling.
    pub fn is_vex_register_packed_string_compare(&self) -> bool {
        self.vex_register_packed_string_encoding().is_some()
    }

    /// Return whether a validated register-only VEX packed-string instruction
    /// writes XMM0. Index-return forms write ECX instead.
    pub(crate) fn vex_register_packed_string_returns_mask(&self) -> Option<bool> {
        Some(
            self.vex_register_packed_string_encoding()?
                .kind
                .returns_mask(),
        )
    }

    /// Validate one complete VEX.128 packed-string memory source and rewrite
    /// only its ModR/M r/m operand to a borrowed low XMM register.
    ///
    /// The helper performs the complete unaligned 16-byte guest read before
    /// native execution. Segment/address-size prefixes, SIB, and displacement
    /// bytes are therefore omitted from the register rewrite. VEX.W and the
    /// full imm8 are retained exactly.
    pub(crate) fn vex_packed_string_memory_encoding(
        &self,
    ) -> Option<X86VexPackedStringMemoryEncoding> {
        let (fields, immediate) = self.vex_memory_fields_with_imm8()?;
        if fields.map != 3
            || fields.pp != 1
            || fields.width_256
            || fields.source1 != 0
            || !matches!(fields.opcode, 0x60..=0x63)
        {
            return None;
        }
        let kind = match fields.opcode {
            0x60 => X86PackedStringKind::ExplicitMask,
            0x61 => X86PackedStringKind::ExplicitIndex,
            0x62 => X86PackedStringKind::ImplicitMask,
            0x63 => X86PackedStringKind::ImplicitIndex,
            _ => unreachable!("validated packed-string opcode"),
        };
        let length_width = if kind.is_explicit() && fields.w {
            OpWidth::W64
        } else {
            OpWidth::W32
        };
        // XMM0 is the architectural mask destination. Avoid borrowing it for
        // every form so one scratch-selection rule is valid for both mask and
        // index results.
        let scratch = (1..16u8)
            .find(|candidate| *candidate != fields.destination)
            .expect("one source cannot consume every nonzero low XMM register");
        let memory_instruction =
            X86InstructionBytes::new(&self.as_slice()[..self.as_slice().len().checked_sub(1)?])?;
        let rewritten = memory_instruction.vex_memory_with_register_source(scratch)?;
        let rewritten_bytes = rewritten.as_slice();
        let len = rewritten_bytes.len().checked_add(1)?;
        let mut bytes = [0u8; 15];
        if len > bytes.len() {
            return None;
        }
        bytes[..rewritten_bytes.len()].copy_from_slice(rewritten_bytes);
        bytes[rewritten_bytes.len()] = immediate;
        let register_instruction = X86InstructionBytes::new(&bytes[..len])?;
        let register = register_instruction.vex_register_packed_string_encoding()?;
        if register.kind != kind
            || register.source1 != fields.destination
            || register.source2 != scratch
            || register.immediate != immediate
            || register.length_width != length_width
        {
            return None;
        }

        Some(X86VexPackedStringMemoryEncoding {
            kind,
            source1: fields.destination,
            scratch,
            immediate,
            length_width,
            memory_size: 16,
            register_instruction,
        })
    }
}
