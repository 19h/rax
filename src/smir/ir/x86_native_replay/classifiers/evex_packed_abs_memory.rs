//! EVEX packed integer absolute-value memory-source classification.

use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use super::{X86EvexIntegerArithmeticMemoryReplay, X86InstructionBytes};
use crate::smir::ir::types::{VecElementType, VecWidth};

/// Exact EVEX `VPABSB`/`VPABSW`/`VPABSD`/`VPABSQ` memory encoding and its
/// byte-validated native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexPackedAbsMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) opcode: u8,
    pub(crate) w: bool,
    pub(crate) replay: X86EvexIntegerArithmeticMemoryReplay,
    pub(crate) needs_avx512vl: bool,
}

impl X86InstructionBytes {
    /// Validate one complete EVEX packed integer absolute-value memory source
    /// and select an exact helper-backed native replay.
    ///
    /// All four operations use Type E4/E4.nb exception semantics: inactive
    /// writemask lanes suppress their corresponding 1/2/4/8-byte accesses.
    /// `VPABSD` and `VPABSQ` additionally accept m32bcst and m64bcst. Segment,
    /// address-size, and APX B4/X4 controls remain confined to helper address
    /// evaluation and are removed from the stack/register replay.
    pub(crate) fn evex_packed_abs_memory_encoding(&self) -> Option<X86EvexPackedAbsMemoryEncoding> {
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
        let w = p1 & 0x80 != 0;
        let elem = match opcode {
            // VPABSB and VPABSW specify WIG.
            0x1C => VecElementType::I8,
            0x1D => VecElementType::I16,
            0x1E if !w => VecElementType::I32,
            0x1F if w => VecElementType::I64,
            _ => return None,
        };
        let mask = p2 & 0x07;
        let zeroing = p2 & 0x80 != 0;
        let broadcast = p2 & 0x10 != 0;
        if p0 & 0x07 != 2
            || p1 & 0x78 != 0x78
            || p1 & 0x03 != 1
            || p2 & 0x08 == 0
            || modrm >> 6 == 3
            || (zeroing && mask == 0)
            || (broadcast && !matches!(elem, VecElementType::I32 | VecElementType::I64))
            || p2 & 0x60 == 0x60
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
        let writemask = (mask != 0).then_some(mask);
        let needs_avx512vl = width != VecWidth::V512;

        let stack_instruction = || {
            X86InstructionBytes::new(&[
                0x62,
                // Preserve R/R' and map 0F38, select unextended RSP, and
                // remove APX B4 from the helper-owned address.
                (p0 & 0x97) | 0x60,
                // Preserve W/vvvv/pp and restore the ordinary EVEX.U bit.
                p1 | 0x04,
                // Preserve z, L'L, b, V', and aaa exactly.
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
            ])
            .unwrap()
        };

        // Revalidate the operation, W/WIG interpretation, destination,
        // vector length, and writemask through the existing register-only
        // classifier. The memory broadcast bit is removed for this probe.
        let register_probe = X86InstructionBytes::new(&[
            0x62,
            (p0 & 0x97) | 0x60,
            p1 | 0x04,
            p2 & !0x10,
            opcode,
            0xC0 | (modrm & 0x38),
        ])
        .unwrap();
        if register_probe.evex_register_packed_abs_needs_vl() != Some(needs_avx512vl) {
            return None;
        }

        let replay = if broadcast {
            X86EvexIntegerArithmeticMemoryReplay::Broadcast {
                stack_instruction: stack_instruction(),
            }
        } else if writemask.is_some() {
            X86EvexIntegerArithmeticMemoryReplay::MaskedVector {
                stack_instruction: stack_instruction(),
            }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| *candidate != destination)
                .expect("one destination cannot consume every low vector register");
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
            if register_instruction.evex_register_packed_abs_needs_vl() != Some(needs_avx512vl) {
                return None;
            }
            X86EvexIntegerArithmeticMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };

        Some(X86EvexPackedAbsMemoryEncoding {
            width,
            elem,
            destination,
            writemask,
            zeroing,
            opcode,
            w,
            replay,
            needs_avx512vl,
        })
    }
}
