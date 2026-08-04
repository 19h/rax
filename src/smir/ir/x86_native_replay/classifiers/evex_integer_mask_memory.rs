//! EVEX packed integer comparison/test memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{VecCmpCond, VecElementType, VecWidth};

/// Native replay strategy for one exact packed integer comparison/test memory
/// encoding that writes an opmask destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexPackedIntegerMaskMemoryReplay {
    /// A complete vector helper load followed by a register-source rewrite
    /// using one nonarchitectural low vector register.
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    /// A scalar helper load followed by the original broadcast operation
    /// rewritten to consume the staged value from `[rsp]`.
    Broadcast {
        stack_instruction: X86InstructionBytes,
    },
    /// Per-active-lane scalar helper loads accumulated in a nonarchitectural
    /// stack vector, followed by the original operation rewritten to consume
    /// that vector from `[rsp]`.
    MaskedVector {
        stack_instruction: X86InstructionBytes,
    },
}

/// Architectural packed integer mask operation selected by one exact EVEX
/// encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexPackedIntegerMaskOperation {
    /// Fixed- or immediate-predicate packed comparison.
    Compare {
        condition: Option<VecCmpCond>,
        constant: Option<bool>,
        /// Low three bits of an immediate predicate; absent for fixed forms.
        predicate: Option<u8>,
    },
    /// Packed bit test (`false`) or inverted packed bit test (`true`).
    Test { inverted: bool },
}

/// Exact EVEX packed integer comparison/test memory encoding and its
/// byte-validated native replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexPackedIntegerMaskMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) source1: u8,
    pub(crate) writemask: Option<u8>,
    pub(crate) operation: X86EvexPackedIntegerMaskOperation,
    pub(crate) replay: X86EvexPackedIntegerMaskMemoryReplay,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512bw: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RegisterFields {
    width: VecWidth,
    elem: VecElementType,
    destination: u8,
    source1: u8,
    source2: u8,
    writemask: Option<u8>,
    operation: X86EvexPackedIntegerMaskOperation,
}

fn immediate_compare_operation(signed: bool, predicate: u8) -> X86EvexPackedIntegerMaskOperation {
    let predicate = predicate & 7;
    let (condition, constant) = match predicate {
        0 => (Some(VecCmpCond::Eq), None),
        1 => (
            Some(if signed {
                VecCmpCond::Lt
            } else {
                VecCmpCond::Ltu
            }),
            None,
        ),
        2 => (
            Some(if signed {
                VecCmpCond::Le
            } else {
                VecCmpCond::Leu
            }),
            None,
        ),
        3 => (None, Some(false)),
        4 => (Some(VecCmpCond::Ne), None),
        5 => (
            Some(if signed {
                VecCmpCond::Ge
            } else {
                VecCmpCond::Geu
            }),
            None,
        ),
        6 => (
            Some(if signed {
                VecCmpCond::Gt
            } else {
                VecCmpCond::Gtu
            }),
            None,
        ),
        7 => (None, Some(true)),
        _ => unreachable!("three-bit packed integer comparison predicate"),
    };
    X86EvexPackedIntegerMaskOperation::Compare {
        condition,
        constant,
        predicate: Some(predicate),
    }
}

fn packed_integer_mask_operation(
    map: u8,
    pp: u8,
    w: bool,
    opcode: u8,
    immediate: Option<u8>,
) -> Option<(VecElementType, X86EvexPackedIntegerMaskOperation)> {
    let fixed_compare = |elem, condition| {
        Some((
            elem,
            X86EvexPackedIntegerMaskOperation::Compare {
                condition: Some(condition),
                constant: None,
                predicate: None,
            },
        ))
    };
    match (map, pp, opcode, w, immediate) {
        (1, 1, 0x64, _, None) => fixed_compare(VecElementType::I8, VecCmpCond::Gt),
        (1, 1, 0x65, _, None) => fixed_compare(VecElementType::I16, VecCmpCond::Gt),
        (1, 1, 0x66, false, None) => fixed_compare(VecElementType::I32, VecCmpCond::Gt),
        (1, 1, 0x74, _, None) => fixed_compare(VecElementType::I8, VecCmpCond::Eq),
        (1, 1, 0x75, _, None) => fixed_compare(VecElementType::I16, VecCmpCond::Eq),
        (1, 1, 0x76, false, None) => fixed_compare(VecElementType::I32, VecCmpCond::Eq),
        (2, 1, 0x29, true, None) => fixed_compare(VecElementType::I64, VecCmpCond::Eq),
        (2, 1, 0x37, true, None) => fixed_compare(VecElementType::I64, VecCmpCond::Gt),
        (3, 1, opcode @ (0x1E | 0x1F), w, Some(predicate)) => Some((
            if w {
                VecElementType::I64
            } else {
                VecElementType::I32
            },
            immediate_compare_operation(opcode == 0x1F, predicate),
        )),
        (3, 1, opcode @ (0x3E | 0x3F), w, Some(predicate)) => Some((
            if w {
                VecElementType::I16
            } else {
                VecElementType::I8
            },
            immediate_compare_operation(opcode == 0x3F, predicate),
        )),
        (2, pp @ (1 | 2), 0x26, w, None) => Some((
            if w {
                VecElementType::I16
            } else {
                VecElementType::I8
            },
            X86EvexPackedIntegerMaskOperation::Test { inverted: pp == 2 },
        )),
        (2, pp @ (1 | 2), 0x27, w, None) => Some((
            if w {
                VecElementType::I64
            } else {
                VecElementType::I32
            },
            X86EvexPackedIntegerMaskOperation::Test { inverted: pp == 2 },
        )),
        _ => None,
    }
}

fn instruction_with_optional_immediate(
    prefix_and_opcode: [u8; 6],
    immediate: Option<u8>,
) -> X86InstructionBytes {
    let mut bytes = prefix_and_opcode.to_vec();
    bytes.extend(immediate);
    X86InstructionBytes::new(&bytes).expect("integer mask replay instruction is at most nine bytes")
}

impl X86InstructionBytes {
    fn evex_register_packed_integer_mask_fields(&self) -> Option<RegisterFields> {
        let bytes = self.as_slice();
        if !matches!(bytes.len(), 6 | 7) || bytes.first() != Some(&0x62) {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        let immediate = (bytes.len() == 7).then(|| bytes[6]);
        let (elem, operation) =
            packed_integer_mask_operation(p0 & 0x0F, p1 & 3, p1 & 0x80 != 0, opcode, immediate)?;
        let ll = (p2 >> 5) & 3;
        if p0 & 0x90 != 0x90 || p1 & 0x04 == 0 || p2 & 0x90 != 0 || ll == 3 || modrm >> 6 != 3 {
            return None;
        }
        let width = match ll {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!("reserved packed vector length rejected"),
        };
        let mask = p2 & 7;
        Some(RegisterFields {
            width,
            elem,
            destination: (modrm >> 3) & 7,
            source1: ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4),
            source2: (modrm & 7)
                | (u8::from(p0 & 0x20 == 0) << 3)
                | (u8::from(p0 & 0x40 == 0) << 4),
            writemask: (mask != 0).then_some(mask),
            operation,
        })
    }

    /// Validate register-only EVEX packed integer bit tests that write an
    /// opmask destination and return whether the vector length requires
    /// AVX-512VL.
    pub fn evex_register_packed_test_needs_vl(&self) -> Option<bool> {
        let fields = self.evex_register_packed_integer_mask_fields()?;
        matches!(
            fields.operation,
            X86EvexPackedIntegerMaskOperation::Test { .. }
        )
        .then_some(fields.width != VecWidth::V512)
    }

    /// Validate register-only EVEX fixed- and immediate-predicate packed
    /// integer comparisons that write an opmask destination and return whether
    /// the vector length requires AVX-512VL.
    pub fn evex_register_packed_compare_needs_vl(&self) -> Option<bool> {
        let fields = self.evex_register_packed_integer_mask_fields()?;
        matches!(
            fields.operation,
            X86EvexPackedIntegerMaskOperation::Compare { .. }
        )
        .then_some(fields.width != VecWidth::V512)
    }

    /// Validate one EVEX packed integer comparison/test memory source and
    /// select an exact helper-backed native replay.
    ///
    /// Intel SDM revision 092 assigns dword/quadword forms to Type E4 and
    /// byte/word forms to E4.nb. Only dword/quadword memory sources permit
    /// scalar broadcast. `EVEX.z` is reserved because the destination is an
    /// opmask; inactive destination bits are always zeroed.
    pub(crate) fn evex_packed_integer_mask_memory_encoding(
        &self,
    ) -> Option<X86EvexPackedIntegerMaskMemoryEncoding> {
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
        let immediate =
            match packed_integer_mask_operation(p0 & 7, p1 & 3, p1 & 0x80 != 0, opcode, None) {
                Some(_) => {
                    if operand_end != bytes.len() {
                        return None;
                    }
                    None
                }
                None => {
                    let immediate = *bytes.get(operand_end)?;
                    if operand_end + 1 != bytes.len() {
                        return None;
                    }
                    Some(immediate)
                }
            };
        let (elem, operation) =
            packed_integer_mask_operation(p0 & 7, p1 & 3, p1 & 0x80 != 0, opcode, immediate)?;
        let ll = (p2 >> 5) & 3;
        let broadcast = p2 & 0x10 != 0;
        if p0 & 0x90 != 0x90
            || p2 & 0x80 != 0
            || ll == 3
            || modrm >> 6 == 3
            || (broadcast && !matches!(elem, VecElementType::I32 | VecElementType::I64))
        {
            return None;
        }
        let width = match ll {
            0 => VecWidth::V128,
            1 => VecWidth::V256,
            2 => VecWidth::V512,
            _ => unreachable!("reserved packed vector length rejected"),
        };
        let destination = (modrm >> 3) & 7;
        let source1 = ((!p1 >> 3) & 0x0F) | (u8::from(p2 & 0x08 == 0) << 4);
        let mask = p2 & 7;
        let writemask = (mask != 0).then_some(mask);

        let register_probe = instruction_with_optional_immediate(
            [
                0x62,
                (p0 & 0x97) | 0x60,
                p1 | 0x04,
                p2 & !0x10,
                opcode,
                0xC0 | (modrm & 0x38),
            ],
            immediate,
        );
        let expected_probe = RegisterFields {
            width,
            elem,
            destination,
            source1,
            source2: 0,
            writemask,
            operation,
        };
        if register_probe.evex_register_packed_integer_mask_fields() != Some(expected_probe) {
            return None;
        }

        let stack_instruction = || {
            let mut rewritten = vec![
                0x62,
                (p0 & 0x97) | 0x60,
                p1 | 0x04,
                p2,
                opcode,
                (modrm & 0x38) | 0x04,
                0x24,
            ];
            rewritten.extend(immediate);
            X86InstructionBytes::new(&rewritten)
                .expect("integer mask stack replay is at most nine bytes")
        };
        let replay = if broadcast {
            X86EvexPackedIntegerMaskMemoryReplay::Broadcast {
                stack_instruction: stack_instruction(),
            }
        } else if writemask.is_some() {
            X86EvexPackedIntegerMaskMemoryReplay::MaskedVector {
                stack_instruction: stack_instruction(),
            }
        } else {
            let scratch = (0..16u8)
                .find(|candidate| *candidate != source1)
                .expect("one source cannot consume every low vector register");
            let register_instruction = instruction_with_optional_immediate(
                [
                    0x62,
                    (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                    p1 | 0x04,
                    p2,
                    opcode,
                    0xC0 | (modrm & 0x38) | (scratch & 7),
                ],
                immediate,
            );
            let expected = RegisterFields {
                source2: scratch,
                ..expected_probe
            };
            if register_instruction.evex_register_packed_integer_mask_fields() != Some(expected) {
                return None;
            }
            X86EvexPackedIntegerMaskMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };
        Some(X86EvexPackedIntegerMaskMemoryEncoding {
            width,
            elem,
            destination,
            source1,
            writemask,
            operation,
            replay,
            needs_avx512vl: width != VecWidth::V512,
            needs_avx512bw: matches!(elem, VecElementType::I8 | VecElementType::I16),
        })
    }
}
