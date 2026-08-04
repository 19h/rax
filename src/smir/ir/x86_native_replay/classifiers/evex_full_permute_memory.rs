//! Exact EVEX one-table full-permute memory-source classification.

use super::X86InstructionBytes;
use super::evex_memory::{memory_operand_end, vector_legacy_prefix_len};
use crate::smir::ir::types::{MemWidth, VecElementType, VecWidth};

/// Control source for one VPERM*/VPERMIL* one-table permutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexFullPermuteControl {
    Variable {
        indices: u8,
    },
    Immediate {
        immediate: u8,
        domain_lanes: u8,
        repeat_lanes: u8,
        control_bits: u8,
    },
}

impl X86EvexFullPermuteControl {
    pub(crate) fn source_lane(self, lane: u8) -> Option<u8> {
        let Self::Immediate {
            immediate,
            domain_lanes,
            repeat_lanes,
            control_bits,
        } = self
        else {
            return None;
        };
        let selector_mask = (1u8 << control_bits) - 1;
        let domain_base = lane / domain_lanes * domain_lanes;
        let shift = (lane % repeat_lanes) * control_bits;
        Some(domain_base + ((immediate >> shift) & selector_mask))
    }
}

/// Native replay selected for one exact Type-E4NF one-table permutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86EvexFullPermuteMemoryReplay {
    Vector {
        scratch: u8,
        register_instruction: X86InstructionBytes,
    },
    Broadcast {
        memory_width: MemWidth,
        stack_instruction: X86InstructionBytes,
    },
}

/// Exact EVEX VPERMB/W/D/Q/PS/PD or immediate VPERMILPS/PD memory encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86EvexFullPermuteMemoryEncoding {
    pub(crate) width: VecWidth,
    pub(crate) elem: VecElementType,
    pub(crate) destination: u8,
    pub(crate) control: X86EvexFullPermuteControl,
    pub(crate) writemask: Option<u8>,
    pub(crate) zeroing: bool,
    pub(crate) replay: X86EvexFullPermuteMemoryReplay,
    pub(crate) memory_size: u32,
    pub(crate) needs_avx512vl: bool,
    pub(crate) needs_avx512vbmi: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FullPermuteFields {
    width: VecWidth,
    elem: VecElementType,
    destination: u8,
    source2: Option<u8>,
    control: X86EvexFullPermuteControl,
    writemask: Option<u8>,
    zeroing: bool,
    broadcast: bool,
    opcode: u8,
    map: u8,
}

fn width(ll: u8, allow_128: bool) -> Option<VecWidth> {
    match ll {
        0 if allow_128 => Some(VecWidth::V128),
        1 => Some(VecWidth::V256),
        2 => Some(VecWidth::V512),
        _ => None,
    }
}

fn operation(
    map: u8,
    opcode: u8,
    w: bool,
    ll: u8,
    encoded_vvvv: u8,
    encoded_v_high: bool,
    immediate: Option<u8>,
) -> Option<(VecWidth, VecElementType, X86EvexFullPermuteControl)> {
    match (map, opcode, w, immediate) {
        (2, 0x8D, false, None) => Some((
            width(ll, true)?,
            VecElementType::I8,
            X86EvexFullPermuteControl::Variable {
                indices: ((!encoded_vvvv) & 0x0F) | (u8::from(!encoded_v_high) << 4),
            },
        )),
        (2, 0x8D, true, None) => Some((
            width(ll, true)?,
            VecElementType::I16,
            X86EvexFullPermuteControl::Variable {
                indices: ((!encoded_vvvv) & 0x0F) | (u8::from(!encoded_v_high) << 4),
            },
        )),
        (2, 0x16, false, None) => Some((
            width(ll, false)?,
            VecElementType::F32,
            X86EvexFullPermuteControl::Variable {
                indices: ((!encoded_vvvv) & 0x0F) | (u8::from(!encoded_v_high) << 4),
            },
        )),
        (2, 0x16, true, None) => Some((
            width(ll, false)?,
            VecElementType::F64,
            X86EvexFullPermuteControl::Variable {
                indices: ((!encoded_vvvv) & 0x0F) | (u8::from(!encoded_v_high) << 4),
            },
        )),
        (2, 0x36, false, None) => Some((
            width(ll, false)?,
            VecElementType::I32,
            X86EvexFullPermuteControl::Variable {
                indices: ((!encoded_vvvv) & 0x0F) | (u8::from(!encoded_v_high) << 4),
            },
        )),
        (2, 0x36, true, None) => Some((
            width(ll, false)?,
            VecElementType::I64,
            X86EvexFullPermuteControl::Variable {
                indices: ((!encoded_vvvv) & 0x0F) | (u8::from(!encoded_v_high) << 4),
            },
        )),
        (3, 0x00, true, Some(immediate)) | (3, 0x01, true, Some(immediate))
            if encoded_vvvv == 0x0F && encoded_v_high =>
        {
            Some((
                width(ll, false)?,
                if opcode == 0x00 {
                    VecElementType::I64
                } else {
                    VecElementType::F64
                },
                X86EvexFullPermuteControl::Immediate {
                    immediate,
                    domain_lanes: 4,
                    repeat_lanes: 4,
                    control_bits: 2,
                },
            ))
        }
        (3, 0x04, false, Some(immediate)) if encoded_vvvv == 0x0F && encoded_v_high => Some((
            width(ll, true)?,
            VecElementType::F32,
            X86EvexFullPermuteControl::Immediate {
                immediate,
                domain_lanes: 4,
                repeat_lanes: 4,
                control_bits: 2,
            },
        )),
        (3, 0x05, true, Some(immediate)) if encoded_vvvv == 0x0F && encoded_v_high => Some((
            width(ll, true)?,
            VecElementType::F64,
            X86EvexFullPermuteControl::Immediate {
                immediate,
                domain_lanes: 2,
                repeat_lanes: 8,
                control_bits: 1,
            },
        )),
        _ => None,
    }
}

fn fields(bytes: &[u8], memory: bool) -> Option<(FullPermuteFields, usize)> {
    let start = if memory {
        vector_legacy_prefix_len(bytes)
    } else {
        0
    };
    if bytes.get(start) != Some(&0x62) {
        return None;
    }
    let p0 = *bytes.get(start + 1)?;
    let p1 = *bytes.get(start + 2)?;
    let p2 = *bytes.get(start + 3)?;
    let opcode = *bytes.get(start + 4)?;
    let modrm_index = start + 5;
    let modrm = *bytes.get(modrm_index)?;
    if p1 & 0x03 != 1
        || (!memory && p1 & 0x04 == 0)
        || (memory == (modrm >> 6 == 3))
        || (p2 & 0x80 != 0 && p2 & 0x07 == 0)
    {
        return None;
    }
    let operand_end = if memory {
        memory_operand_end(bytes, modrm_index)?
    } else {
        modrm_index + 1
    };
    let map = p0 & 0x07;
    let immediate_form = map == 3 && matches!(opcode, 0x00 | 0x01 | 0x04 | 0x05);
    let immediate = if immediate_form {
        Some(*bytes.get(operand_end)?)
    } else {
        None
    };
    let expected_end = operand_end + usize::from(immediate_form);
    if expected_end != bytes.len() {
        return None;
    }
    let (width, elem, control) = operation(
        map,
        opcode,
        p1 & 0x80 != 0,
        (p2 >> 5) & 3,
        (p1 >> 3) & 0x0F,
        p2 & 0x08 != 0,
        immediate,
    )?;
    let broadcast = p2 & 0x10 != 0;
    if (!memory && broadcast)
        || (broadcast && matches!(elem, VecElementType::I8 | VecElementType::I16))
    {
        return None;
    }
    let source2 = (!memory)
        .then_some((modrm & 7) | (u8::from(p0 & 0x20 == 0) << 3) | (u8::from(p0 & 0x40 == 0) << 4));
    Some((
        FullPermuteFields {
            width,
            elem,
            destination: (u8::from(p0 & 0x80 == 0) << 3)
                | (u8::from(p0 & 0x10 == 0) << 4)
                | ((modrm >> 3) & 7),
            source2,
            control,
            writemask: (p2 & 7 != 0).then_some(p2 & 7),
            zeroing: p2 & 0x80 != 0,
            broadcast,
            opcode,
            map,
        },
        modrm_index,
    ))
}

impl X86InstructionBytes {
    /// Validate one Type-E4NF one-table full-permute memory encoding and
    /// construct a byte-checked helper-backed native replay.
    ///
    /// Covered forms are variable-control VPERMB/W/D/Q/PS/PD and
    /// immediate-control VPERMQ/PD/VPERMILPS/PD. Full tuples are unconditional
    /// even under a zero writemask. Segment/address-size prefixes and APX
    /// B4/X4 address extensions remain confined to helper address evaluation.
    pub(crate) fn evex_full_permute_memory_encoding(
        &self,
    ) -> Option<X86EvexFullPermuteMemoryEncoding> {
        let bytes = self.as_slice();
        let (classified, modrm_index) = fields(bytes, true)?;
        let start = vector_legacy_prefix_len(bytes);
        let p0 = bytes[start + 1];
        let p1 = bytes[start + 2];
        let p2 = bytes[start + 3];
        let opcode = bytes[start + 4];
        let modrm = bytes[modrm_index];
        let immediate = match classified.control {
            X86EvexFullPermuteControl::Immediate { immediate, .. } => Some(immediate),
            X86EvexFullPermuteControl::Variable { .. } => None,
        };

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
            if let Some(immediate) = immediate {
                rewritten.push(immediate);
            }
            X86InstructionBytes::new(&rewritten).unwrap()
        };

        let replay = if classified.broadcast {
            let memory_width = match classified.elem {
                VecElementType::I32 | VecElementType::F32 => MemWidth::B4,
                VecElementType::I64 | VecElementType::F64 => MemWidth::B8,
                _ => return None,
            };
            let stack_instruction = stack_instruction();
            let (stack_fields, stack_modrm) = fields(stack_instruction.as_slice(), true)?;
            if stack_fields != classified || stack_instruction.as_slice()[stack_modrm] & 7 != 4 {
                return None;
            }
            X86EvexFullPermuteMemoryReplay::Broadcast {
                memory_width,
                stack_instruction,
            }
        } else {
            let indices = match classified.control {
                X86EvexFullPermuteControl::Variable { indices } => Some(indices),
                X86EvexFullPermuteControl::Immediate { .. } => None,
            };
            let scratch = (0..16u8)
                .find(|candidate| {
                    *candidate != classified.destination && Some(*candidate) != indices
                })
                .expect("at most two operands consume low vector registers");
            let mut rewritten = vec![
                0x62,
                (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                p1 | 0x04,
                p2 & !0x10,
                opcode,
                0xC0 | (modrm & 0x38) | (scratch & 7),
            ];
            if let Some(immediate) = immediate {
                rewritten.push(immediate);
            }
            let register_instruction = X86InstructionBytes::new(&rewritten).unwrap();
            let expected = FullPermuteFields {
                source2: Some(scratch),
                broadcast: false,
                ..classified
            };
            if fields(register_instruction.as_slice(), false)?.0 != expected {
                return None;
            }
            X86EvexFullPermuteMemoryReplay::Vector {
                scratch,
                register_instruction,
            }
        };
        let memory_size = match replay {
            X86EvexFullPermuteMemoryReplay::Vector { .. } => classified.width.bytes(),
            X86EvexFullPermuteMemoryReplay::Broadcast { memory_width, .. } => memory_width.bytes(),
        };
        Some(X86EvexFullPermuteMemoryEncoding {
            width: classified.width,
            elem: classified.elem,
            destination: classified.destination,
            control: classified.control,
            writemask: classified.writemask,
            zeroing: classified.zeroing,
            replay,
            memory_size,
            needs_avx512vl: classified.width != VecWidth::V512,
            needs_avx512vbmi: classified.elem == VecElementType::I8,
        })
    }
}
