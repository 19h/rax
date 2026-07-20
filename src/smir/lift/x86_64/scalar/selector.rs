//! Legacy system-segment selector stores.

use crate::smir::ir::ops::{
    OpKind, SmirOp, X86SystemSelector, X86SystemSelectorStoreOp, X86SystemSelectorTarget,
};
use crate::smir::ir::types::OpId;
use crate::smir::lift::x86_64::{X86_64Lifter, X86Prefix, decode_modrm};
use crate::smir::lift::{LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    /// Lift SLDT/STR (`0F 00 /0` and `/1`). Register destinations follow the
    /// encoded 16-/32-/64-bit operand width; memory destinations are fixed at
    /// 2 bytes. Protected-mode, APX, and UMIP checks remain dynamic.
    pub(crate) fn lift_system_selector_store_0f00(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..bytes.len().min(1)].to_vec(),
            });
        }

        let modrm = decode_modrm(bytes, prefix, pc)?;
        let selector = match (modrm.byte >> 3) & 7 {
            0 => X86SystemSelector::Ldtr,
            1 => X86SystemSelector::Tr,
            _ => unreachable!("0F 00 selector-store dispatcher admitted another group"),
        };
        let bytes_consumed = prefix.cursor + modrm.bytes_consumed;
        let target = if let Some(x86_addr) = modrm.addr.as_ref() {
            let next_pc = pc.wrapping_add(bytes_consumed as u64);
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            debug_assert!(pre_ops.is_empty());
            X86SystemSelectorTarget::Memory { addr }
        } else {
            X86SystemSelectorTarget::Register {
                dst: self.gpr(modrm.rm),
                width: prefix.op_width(),
            }
        };

        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(
                OpId(0),
                pc,
                OpKind::X86SystemSelectorStore(X86SystemSelectorStoreOp {
                    selector,
                    target,
                    requires_apx: prefix.rex2.is_some(),
                }),
            )],
            bytes_consumed,
        ))
    }
}
