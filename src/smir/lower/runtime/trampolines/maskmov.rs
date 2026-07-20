//! Exact helper-backed x86 MASKMOVDQU/VMASKMOVDQU sequence admission.

use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, MemWidth, OpWidth, SignExtend, SrcOperand, VReg,
    VecElementType, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86MaskmovdquSequence {
    pub(crate) consumed: usize,
    pub(crate) data_index: u8,
    pub(crate) mask_index: u8,
    pub(crate) address_size_32: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum X86MaskmovdquAddressKind {
    Rdi,
    FsRdi,
    GsRdi,
}

fn xmm_index(reg: VReg) -> Option<u8> {
    match reg {
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(index @ 0..=15))) => Some(index),
        _ => None,
    }
}

fn x86_maskmovdqu_lane_address_kind(
    addr: &Address,
    expected_base: VReg,
    expected_disp: i64,
) -> Option<X86MaskmovdquAddressKind> {
    match addr {
        Address::BaseOffset {
            base,
            offset,
            disp_size: DispSize::Auto,
        } if *base == expected_base && *offset == expected_disp => {
            Some(X86MaskmovdquAddressKind::Rdi)
        }
        Address::SegmentRel {
            segment: VReg::Arch(ArchReg::X86(segment @ (X86Reg::FsBase | X86Reg::GsBase))),
            base: Some(base),
            index: None,
            scale: 1,
            disp,
        } if *base == expected_base && *disp == expected_disp => Some(match segment {
            X86Reg::FsBase => X86MaskmovdquAddressKind::FsRdi,
            X86Reg::GsBase => X86MaskmovdquAddressKind::GsRdi,
            _ => return None,
        }),
        _ => None,
    }
}

/// Validate the exact sixteen-lane byte-store expansion emitted for legacy
/// `MASKMOVDQU` and VEX.128 `VMASKMOVDQU`. Every temporary must be SSA-local,
/// every address and mask test must match the canonical lifter shape, and
/// address-size-overridden lane additions must wrap at 32 bits before optional
/// FS/GS segmentation.
pub(crate) fn x86_jit_maskmovdqu_sequence(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> Option<X86MaskmovdquSequence> {
    if !allow_mem {
        return None;
    }

    let first = block.ops.get(index)?;
    let guest_pc = first.guest_pc;
    let (lane_ops_offset, address_size_32, address_base) = match &first.kind {
        OpKind::And {
            dst: truncated @ VReg::Virtual(_),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Rdi)),
            src2: SrcOperand::Imm(0xFFFF_FFFF),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        } if first.x86_hint.is_none()
            && virtual_definitions.get(truncated) == Some(&1)
            && virtual_uses.get(truncated) == Some(&16) =>
        {
            (1, true, *truncated)
        }
        _ => (0, false, VReg::Arch(ArchReg::X86(X86Reg::Rdi))),
    };

    let mut data_index = None;
    let mut mask_index = None;
    let mut address_kind = None;
    let mut cursor = index + lane_ops_offset;
    for lane in 0..16u8 {
        let (lane_address_base, lane_disp) = if address_size_32 && lane != 0 {
            let wrap = block.ops.get(cursor)?;
            let wrapped = match &wrap.kind {
                OpKind::Add {
                    dst: temporary @ VReg::Virtual(_),
                    src1,
                    src2: SrcOperand::Imm(offset),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                } if *src1 == address_base
                    && *offset == i64::from(lane)
                    && wrap.guest_pc == guest_pc
                    && wrap.x86_hint.is_none() =>
                {
                    *temporary
                }
                _ => return None,
            };
            if virtual_definitions.get(&wrapped) != Some(&1)
                || virtual_uses.get(&wrapped) != Some(&1)
            {
                return None;
            }
            cursor += 1;
            (wrapped, 0)
        } else {
            (
                address_base,
                if address_size_32 { 0 } else { i64::from(lane) },
            )
        };

        let mask_extract = block.ops.get(cursor)?;
        let shift = block.ops.get(cursor + 1)?;
        let data_extract = block.ops.get(cursor + 2)?;
        let store = block.ops.get(cursor + 3)?;
        if [mask_extract, shift, data_extract, store]
            .iter()
            .any(|op| op.guest_pc != guest_pc || op.x86_hint.is_some())
        {
            return None;
        }

        let (mask_byte, actual_mask_index) = match &mask_extract.kind {
            OpKind::VExtractLane {
                dst: temporary @ VReg::Virtual(_),
                vec,
                lane: actual_lane,
                elem: VecElementType::I8,
                sign: SignExtend::Zero,
            } if *actual_lane == lane => (*temporary, xmm_index(*vec)?),
            _ => return None,
        };
        let active = match &shift.kind {
            OpKind::Shr {
                dst: temporary @ VReg::Virtual(_),
                src,
                amount: SrcOperand::Imm(7),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            } if *src == mask_byte => *temporary,
            _ => return None,
        };
        let (data_byte, actual_data_index) = match &data_extract.kind {
            OpKind::VExtractLane {
                dst: temporary @ VReg::Virtual(_),
                vec,
                lane: actual_lane,
                elem: VecElementType::I8,
                sign: SignExtend::Zero,
            } if *actual_lane == lane => (*temporary, xmm_index(*vec)?),
            _ => return None,
        };
        let actual_address_kind = match &store.kind {
            OpKind::PredStore {
                src: SrcOperand::Reg(src),
                cond,
                addr,
                width: MemWidth::B1,
            } if *src == data_byte && *cond == active => {
                x86_maskmovdqu_lane_address_kind(addr, lane_address_base, lane_disp)?
            }
            _ => return None,
        };
        if [mask_byte, active, data_byte].iter().any(|temporary| {
            virtual_definitions.get(temporary) != Some(&1)
                || virtual_uses.get(temporary) != Some(&1)
        }) {
            return None;
        }

        match mask_index {
            None => mask_index = Some(actual_mask_index),
            Some(index) if index == actual_mask_index => {}
            Some(_) => return None,
        }
        match data_index {
            None => data_index = Some(actual_data_index),
            Some(index) if index == actual_data_index => {}
            Some(_) => return None,
        }
        match address_kind {
            None => address_kind = Some(actual_address_kind),
            Some(kind) if kind == actual_address_kind => {}
            Some(_) => return None,
        }
        cursor += 4;
    }

    Some(X86MaskmovdquSequence {
        consumed: cursor - index,
        data_index: data_index?,
        mask_index: mask_index?,
        address_size_32,
    })
}

pub(crate) fn x86_jit_maskmovdqu_sequence_len(
    block: &SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> Option<usize> {
    x86_jit_maskmovdqu_sequence(block, index, allow_mem, virtual_definitions, virtual_uses)
        .map(|sequence| sequence.consumed)
}

/// Whether a region needs low XMM source state copied into `GuestRegs` even
/// though it has no independently admitted native vector operation.
pub(crate) fn uses_x86_maskmovdqu_state_excluding(
    function: &SmirFunction,
    excluded: &std::collections::HashMap<BlockId, u64>,
) -> bool {
    function
        .blocks
        .iter()
        .filter(|block| !excluded.contains_key(&block.id))
        .any(|block| {
            let mut definitions = std::collections::HashMap::new();
            let mut uses = std::collections::HashMap::new();
            for op in &block.ops {
                for reg in op.kind.dests() {
                    if matches!(reg, VReg::Virtual(_)) {
                        *definitions.entry(reg).or_insert(0usize) += 1;
                    }
                }
                for reg in op.kind.source_vregs() {
                    if matches!(reg, VReg::Virtual(_)) {
                        *uses.entry(reg).or_insert(0usize) += 1;
                    }
                }
            }
            (0..block.ops.len()).any(|index| {
                x86_jit_maskmovdqu_sequence_len(block, index, true, &definitions, &uses).is_some()
            })
        })
}

/// Operations that reach the x86 MMU helper path after exact sequence gates.
pub(crate) fn x86_jit_op_uses_mem_helper(op: &OpKind) -> bool {
    matches!(
        op,
        OpKind::Load { .. }
            | OpKind::Store { .. }
            | OpKind::VLoad { .. }
            | OpKind::VStore { .. }
            | OpKind::PredStore { .. }
            | OpKind::X86DescriptorTableStore(..)
            | OpKind::X86DescriptorTableLoad(..)
            | OpKind::X86SystemSelectorLoad(..)
            | OpKind::X86FarJump(..)
            | OpKind::X86FarCall(..)
            | OpKind::X86SystemSelectorStore(crate::smir::ir::ops::X86SystemSelectorStoreOp {
                target: crate::smir::ir::ops::X86SystemSelectorTarget::Memory { .. },
                ..
            },)
    )
}
