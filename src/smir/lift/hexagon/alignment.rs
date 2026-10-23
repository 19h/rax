//! Scalar alignment checks before instruction-side register or memory effects.

use super::*;
use crate::isa::hexagon::memory_access::scalar_memory_access;

impl HexagonLifter {
    pub(super) fn alignment_prefix(
        &self,
        insn: &DecodedInsn,
        address: GuestAddr,
        context: &mut LiftContext,
    ) -> Vec<SmirOp> {
        let Some(operand) = scalar_memory_access(insn) else {
            return Vec::new();
        };
        let mut operations = Vec::new();
        let mut id = 0;
        // Internal frame/locked/RMW addresses are already fully decoded;
        // their synthetic offsets must not consume an extender for another field.
        let saved_extender = context.extended_imm;
        if matches!(
            insn,
            DecodedInsn::AllocFrame { .. }
                | DecodedInsn::DeallocFrame { .. }
                | DecodedInsn::DeallocReturn { .. }
                | DecodedInsn::LoadLocked { .. }
                | DecodedInsn::StoreCond { .. }
                | DecodedInsn::MemOp { .. }
        ) {
            context.extended_imm = None;
        }
        let effective = self
            .emit_postinc_load_ea(&mut operations, &mut id, address, context, &operand.address)
            .unwrap_or_else(|| self.hex_addr(&operand.address, context));
        context.extended_imm = saved_extender;
        let effective32 =
            self.emit_address32(&mut operations, &mut id, address, context, effective);
        let condition = operand.predicate.map(|predicate| {
            self.emit_pred_truth(
                &mut operations,
                &mut id,
                address,
                context,
                predicate.pred,
                predicate.sense,
            )
        });
        operations.push(SmirOp::new(
            OpId(id),
            self.packet_start_pc,
            OpKind::CheckMemoryAlignment {
                addr: Address::Direct(effective32),
                width: self.hex_mem_width(operand.width),
                access: operand.access,
                cond: condition,
            },
        ));
        operations
    }
}
