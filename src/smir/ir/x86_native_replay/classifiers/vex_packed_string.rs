//! Register-only legacy SSE4.2 and AVX VEX packed-string comparison replay.

use super::X86InstructionBytes;
use crate::smir::ir::ops::{OpKind, SmirOp, X86PackedStringKind};
use crate::smir::ir::types::{ArchReg, OpWidth, VReg, X86Reg};

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
struct X86PackedStringRegisterEncoding {
    kind: X86PackedStringKind,
    source1: u8,
    source2: u8,
    immediate: u8,
    length_width: OpWidth,
    zero_upper: bool,
}

impl X86InstructionBytes {
    fn legacy_register_packed_string_encoding(&self) -> Option<X86PackedStringRegisterEncoding> {
        let bytes = self.as_slice();
        let (rex, suffix) = match bytes {
            [0x66, 0x0F, 0x3A, ..] if bytes.len() == 6 => (0, &bytes[1..]),
            [0x66, rex @ 0x40..=0x4F, 0x0F, 0x3A, ..] if bytes.len() == 7 => (*rex, &bytes[2..]),
            _ => return None,
        };
        let [0x0F, 0x3A, opcode, modrm, immediate] = suffix else {
            return None;
        };
        let (opcode, modrm, immediate) = (*opcode, *modrm, *immediate);
        if modrm >> 6 != 3 {
            return None;
        }
        let kind = match opcode {
            0x60 => X86PackedStringKind::ExplicitMask,
            0x61 => X86PackedStringKind::ExplicitIndex,
            0x62 => X86PackedStringKind::ImplicitMask,
            0x63 => X86PackedStringKind::ImplicitIndex,
            _ => return None,
        };
        Some(X86PackedStringRegisterEncoding {
            kind,
            source1: ((modrm >> 3) & 7) | ((rex & 4) << 1),
            source2: (modrm & 7) | ((rex & 1) << 3),
            immediate,
            length_width: if kind.is_explicit() && rex & 8 != 0 {
                OpWidth::W64
            } else {
                OpWidth::W32
            },
            zero_upper: false,
        })
    }

    fn vex_register_packed_string_encoding(&self) -> Option<X86PackedStringRegisterEncoding> {
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
        Some(X86PackedStringRegisterEncoding {
            kind,
            source1: (u8::from(p0 & 0x80 == 0) << 3) | ((modrm >> 3) & 7),
            source2: (u8::from(p0 & 0x20 == 0) << 3) | (modrm & 7),
            immediate: bytes[5],
            length_width: if kind.is_explicit() && p1 & 0x80 != 0 {
                OpWidth::W64
            } else {
                OpWidth::W32
            },
            zero_upper: kind.returns_mask(),
        })
    }

    /// Validate one register-only legacy SSE4.2 `PCMPxSTRx` instruction.
    ///
    /// Intel SDM Vol. 2B assigns opcodes 60H through 63H in map 0F3A with
    /// mandatory 66H. A final REX prefix may select XMM0 through XMM15; REX.W
    /// selects 64-bit explicit lengths and is ignored by implicit-length
    /// forms. REX.X is ignored for a register ModR/M operand. Memory forms
    /// remain excluded so native replay cannot bypass guest-memory faults.
    pub fn is_legacy_register_packed_string_compare(&self) -> bool {
        self.legacy_register_packed_string_encoding().is_some()
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

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn gpr(register: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(register))
}

/// Validate the complete canonical SMIR graph for one register-only legacy or
/// VEX packed-string comparison. Source bytes alone are insufficient: exact
/// replay must fail closed if optimizer or provenance corruption changes any
/// operand, output form, length width, immediate, or upper-state policy.
pub(crate) fn x86_register_packed_string_shape_matches(
    ops: &[SmirOp],
    instruction: &X86InstructionBytes,
) -> bool {
    let Some(encoding) = instruction
        .legacy_register_packed_string_encoding()
        .or_else(|| instruction.vex_register_packed_string_encoding())
    else {
        return false;
    };
    let [operation] = ops else {
        return false;
    };
    if operation.x86_hint.is_some() {
        return false;
    }
    let expected_destination = if encoding.kind.returns_mask() {
        xmm(0)
    } else {
        gpr(X86Reg::Rcx)
    };
    let expected_lengths = if encoding.kind.is_explicit() {
        (Some(gpr(X86Reg::Rax)), Some(gpr(X86Reg::Rdx)))
    } else {
        (None, None)
    };
    matches!(
        operation.kind,
        OpKind::X86PackedStringCompare {
            dst,
            src1,
            src2,
            len1,
            len2,
            length_width,
            kind,
            imm,
            zero_upper,
        } if dst == expected_destination
            && src1 == xmm(encoding.source1)
            && src2 == xmm(encoding.source2)
            && (len1, len2) == expected_lengths
            && length_width == encoding.length_width
            && kind == encoding.kind
            && imm == encoding.immediate
            && zero_upper == encoding.zero_upper
    )
}
