//! Scalar memory operand contracts shared by the CPU and lifter.

use super::decode::{AddrMode, DecodedInsn, MemWidth, PredCond};
use crate::error::MemoryAccessKind;

#[derive(Clone, Copy)]
pub(crate) struct ScalarMemoryAccess {
    pub address: AddrMode,
    pub width: MemWidth,
    pub access: MemoryAccessKind,
    pub predicate: Option<PredCond>,
}

pub(crate) fn width_bytes(width: MemWidth) -> u32 {
    match width {
        MemWidth::Byte => 1,
        MemWidth::Half => 2,
        MemWidth::Word => 4,
        MemWidth::Double => 8,
    }
}

pub(crate) fn scalar_memory_access(insn: &DecodedInsn) -> Option<ScalarMemoryAccess> {
    use MemoryAccessKind::{Read, Write};
    let (address, width, access, predicate) = match insn {
        DecodedInsn::Load {
            addr, width, pred, ..
        }
        | DecodedInsn::LoadAlign {
            addr, width, pred, ..
        } => (*addr, *width, Read, *pred),
        DecodedInsn::LoadUnpack {
            addr, count, pred, ..
        } => {
            let width = match count {
                2 => MemWidth::Half,
                4 => MemWidth::Word,
                _ => return None,
            };
            (*addr, width, Read, *pred)
        }
        DecodedInsn::Store {
            addr, width, pred, ..
        }
        | DecodedInsn::StoreImm {
            addr, width, pred, ..
        }
        | DecodedInsn::StoreNew {
            addr, width, pred, ..
        } => (*addr, *width, Write, *pred),
        DecodedInsn::LoadLocked { base, width, .. } => (
            AddrMode::Offset {
                base: *base,
                offset: 0,
            },
            *width,
            Read,
            None,
        ),
        DecodedInsn::StoreCond {
            base,
            width,
            success_pred: None,
            ..
        } => (
            AddrMode::Offset {
                base: *base,
                offset: 0,
            },
            *width,
            Write,
            None,
        ),
        DecodedInsn::MemOp {
            base,
            offset,
            width,
            ..
        } => (
            AddrMode::Offset {
                base: *base,
                offset: *offset,
            },
            *width,
            Write,
            None,
        ),
        DecodedInsn::AllocFrame { base, .. } => (
            AddrMode::Offset {
                base: *base,
                offset: -8,
            },
            MemWidth::Double,
            Write,
            None,
        ),
        DecodedInsn::DeallocFrame { base, .. } => (
            AddrMode::Offset {
                base: *base,
                offset: 0,
            },
            MemWidth::Double,
            Read,
            None,
        ),
        DecodedInsn::DeallocReturn { base, pred, .. } => (
            AddrMode::Offset {
                base: *base,
                offset: 0,
            },
            MemWidth::Double,
            Read,
            *pred,
        ),
        _ => return None,
    };
    Some(ScalarMemoryAccess {
        address,
        width,
        access,
        predicate,
    })
}
