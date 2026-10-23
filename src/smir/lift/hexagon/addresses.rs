//! Explicit modulo-2^32 effective addresses in otherwise architecture-neutral IR.

use super::*;

impl HexagonLifter {
    pub(super) fn emit_address32(
        &self,
        operations: &mut Vec<SmirOp>,
        id: &mut u16,
        pc: GuestAddr,
        context: &mut LiftContext,
        address: Address,
    ) -> VReg {
        // GpRel hides its architecture-specific input from generic address
        // metadata. Expose GP so packet-old read rewriting and optimization
        // preserve the actual address dependency.
        let address = match address {
            Address::GpRel { offset } => Address::BaseOffset {
                base: VReg::Arch(ArchReg::Hexagon(HexagonReg::Gp)),
                offset: i64::from(offset),
                disp_size: DispSize::Auto,
            },
            other => other,
        };
        let wide = context.alloc_vreg();
        operations.push(SmirOp::new(
            OpId(*id),
            pc,
            OpKind::Lea {
                dst: wide,
                addr: address,
            },
        ));
        *id += 1;
        let narrow = context.alloc_vreg();
        operations.push(SmirOp::new(
            OpId(*id),
            pc,
            OpKind::And {
                dst: narrow,
                src1: wide,
                src2: SrcOperand::Imm(u32::MAX as i64),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ));
        *id += 1;
        narrow
    }

    /// Normalize the actual scalar memory operands as well as their guards.
    /// Materialization immediately before the access preserves instruction-side
    /// base updates. These pure operations neither touch RAM nor fault when a
    /// predicate cancels the following access.
    pub(super) fn normalize_memory_addresses(
        &self,
        operations: Vec<SmirOp>,
        context: &mut LiftContext,
    ) -> Vec<SmirOp> {
        let mut normalized = Vec::with_capacity(operations.len());
        let mut id = 0;
        for mut operation in operations {
            let address = memory_address_mut(&mut operation.kind);
            if let Some(address) = address {
                let effective = self.emit_address32(
                    &mut normalized,
                    &mut id,
                    operation.guest_pc,
                    context,
                    address.clone(),
                );
                *address = Address::Direct(effective);
            }
            operation.id = OpId(id);
            id += 1;
            normalized.push(operation);
        }
        normalized
    }
}

pub(super) fn memory_address_mut(operation: &mut OpKind) -> Option<&mut Address> {
    match operation {
        OpKind::Load { addr, .. }
        | OpKind::Store { addr, .. }
        | OpKind::PredLoad { addr, .. }
        | OpKind::PredStore { addr, .. }
        | OpKind::LoadPair { addr, .. }
        | OpKind::StorePair { addr, .. }
        | OpKind::LoadExclusive { addr, .. }
        | OpKind::StoreExclusive { addr, .. } => Some(addr),
        _ => None,
    }
}
