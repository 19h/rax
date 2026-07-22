//! Admission validation for native x86 memory-source CRC32 fusion.

use crate::smir::ir::ops::OpKind;
use crate::smir::ir::types::{ArchReg, MemWidth, OpWidth, SignExtend, VReg};

use super::x86_jit_mem_address_shape_valid;

/// Validate the exact two-op shape emitted for a memory-source x86 CRC32.
/// The virtual load result must be single-definition/single-use so native
/// lowering can eliminate it without creating an identity-map GPR alias.
pub(crate) fn x86_mem_crc32_pair_valid(
    block: &crate::smir::ir::SmirBlock,
    index: usize,
    allow_mem: bool,
    virtual_definitions: &std::collections::HashMap<VReg, usize>,
    virtual_uses: &std::collections::HashMap<VReg, usize>,
) -> bool {
    if !allow_mem {
        return false;
    }
    let (load_pc, temporary, addr, width) = match block.ops.get(index) {
        Some(op) => match &op.kind {
            OpKind::Load {
                dst: VReg::Virtual(temporary),
                addr,
                width,
                sign: SignExtend::Zero,
            } => (op.guest_pc, VReg::Virtual(*temporary), addr, *width),
            _ => return false,
        },
        None => return false,
    };
    let data_width = match width {
        MemWidth::B1 => OpWidth::W8,
        MemWidth::B2 => OpWidth::W16,
        MemWidth::B4 => OpWidth::W32,
        MemWidth::B8 => OpWidth::W64,
        _ => return false,
    };
    let crc = match block.ops.get(index + 1) {
        Some(op) if op.guest_pc == load_pc => op,
        _ => return false,
    };
    let accumulator_valid = matches!(
        &crc.kind,
        OpKind::Crc32C {
            dst,
            crc,
            data,
            data_width: crc_width,
        } if dst == crc
            && *data == temporary
            && *crc_width == data_width
            && matches!(dst, VReg::Arch(ArchReg::X86(x86)) if x86.gpr_index().is_some())
    );
    if !accumulator_valid || !x86_jit_mem_address_shape_valid(addr) {
        return false;
    }

    virtual_definitions.get(&temporary) == Some(&1) && virtual_uses.get(&temporary) == Some(&1)
}
