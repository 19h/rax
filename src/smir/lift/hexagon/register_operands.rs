//! Explicit .new data operands and instruction-local register/address aliases.

use super::addresses::memory_address_mut;
use super::packet_registers::{copy, register_index};
use super::*;
use crate::isa::hexagon::decode::{CmpJumpKind, CombineOperand};

fn map_address(address: &mut Address, frozen: &[Option<VReg>; 32]) {
    let replace = |register: &mut VReg| {
        if let Some(index) = register_index(*register)
            && let Some(value) = frozen[index]
        {
            *register = value;
        }
    };
    match address {
        Address::Direct(register) | Address::BaseOffset { base: register, .. } => replace(register),
        Address::BaseIndexScale { base, index, .. } => {
            if let Some(base) = base {
                replace(base);
            }
            replace(index);
        }
        _ => {}
    }
}

impl HexagonLifter {
    fn new_register_source(&self, register: u8, pc: GuestAddr) -> Result<VReg, LiftError> {
        self.packet_registers
            .latest(register)
            .ok_or_else(|| LiftError::Unsupported {
                addr: pc,
                mnemonic: "Hexagon .new GPR without a represented producer".into(),
            })
    }

    pub(super) fn rewrite_register_operands(
        &self,
        instruction: &DecodedInsn,
        operations: &mut Vec<SmirOp>,
        pc: GuestAddr,
        context: &mut LiftContext,
    ) -> Result<(), LiftError> {
        // .new store data is distinct from an ordinary address read of the same
        // GPR. Rewrite payload fields only; address inputs remain packet-old.
        let store_source = match instruction {
            DecodedInsn::StoreNew { nt, .. } => Some(*nt),
            DecodedInsn::Store {
                src, src_new: true, ..
            } => Some(*src),
            _ => None,
        };
        if let Some(encoded) = store_source {
            let register =
                self.resolve_new_value_src(encoded)
                    .ok_or_else(|| LiftError::Unsupported {
                        addr: pc,
                        mnemonic: "unresolved Hexagon .new store source".into(),
                    })?;
            let value = self.new_register_source(register, pc)?;
            let mut replaced = false;
            for operation in operations.iter_mut() {
                match &mut operation.kind {
                    OpKind::Store { src, .. } => {
                        *src = value;
                        replaced = true;
                    }
                    OpKind::PredStore { src, .. } => {
                        *src = SrcOperand::Reg(value);
                        replaced = true;
                    }
                    OpKind::StorePair { src1, src2, .. } => {
                        *src1 = self.new_register_source(register & !1, pc)?;
                        *src2 = self.new_register_source((register & !1) + 1, pc)?;
                        replaced = true;
                    }
                    _ => {}
                }
            }
            if !replaced {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: "unrepresented Hexagon .new store payload".into(),
                });
            }
        }
        if let DecodedInsn::CompoundCmpJump {
            src1,
            kind,
            new_value: true,
            ..
        } = instruction
        {
            let register =
                self.resolve_new_value_src(*src1)
                    .ok_or_else(|| LiftError::Unsupported {
                        addr: pc,
                        mnemonic: "unresolved Hexagon .new compare source".into(),
                    })?;
            let value = self.new_register_source(register, pc)?;
            let mut replaced = false;
            for operation in operations.iter_mut() {
                let source = match (&mut operation.kind, kind) {
                    (OpKind::And { src1, .. }, CmpJumpKind::TstBit0) => Some(src1),
                    (OpKind::Cmp { src1, .. }, _) => Some(src1),
                    _ => None,
                };
                if let Some(source) = source
                    && register_index(*source) == Some(register as usize)
                {
                    *source = value;
                    replaced = true;
                    break;
                }
            }
            if !replaced {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: "unrepresented Hexagon .new compare operand".into(),
                });
            }
        }

        let mut prefix = Vec::new();
        let combine = match instruction {
            DecodedInsn::Combine {
                dst,
                high: CombineOperand::Reg(high),
                ..
            } => Some((*dst, *high)),
            DecodedInsn::Unknown(word) => decode_word(*word).and_then(|decoded| {
                if decoded.opcode == Opcode::A2_combinew {
                    Some((
                        (decoded.field(b'd')?.value as u8) & !1,
                        decoded.field(b's')?.value as u8,
                    ))
                } else {
                    None
                }
            }),
            _ => None,
        };
        if let Some((destination, high)) = combine
            && destination == high
        {
            // Both inputs are packet-old even when the high source aliases the
            // low destination, including the opaque A2_combinew opcode path.
            let saved = context.alloc_vreg();
            copy(&mut prefix, pc, saved, self.hex_reg(high));
            for operation in operations.iter_mut() {
                if let OpKind::Mov { dst, src, .. } = &mut operation.kind
                    && register_index(*dst) == Some(destination as usize + 1)
                {
                    *src = SrcOperand::Reg(saved);
                }
            }
        }

        // Scalar memory addresses are instruction inputs. Freeze address GPRs
        // that an earlier result within this instruction could overwrite, e.g.
        // the low half of a predicated pair load aliasing its own base.
        let mut written = [false; 32];
        let mut address_inputs = [false; 32];
        for operation in operations.iter_mut() {
            for destination in operation.kind.dests() {
                if let Some(index) = register_index(destination) {
                    written[index] = true;
                }
            }
            if let Some(address) = memory_address_mut(&mut operation.kind) {
                for register in address.regs() {
                    if let Some(index) = register_index(register) {
                        address_inputs[index] = true;
                    }
                }
            }
        }
        let frame_base = match instruction {
            DecodedInsn::DeallocFrame { base, .. } | DecodedInsn::DeallocReturn { base, .. } => {
                Some(*base as usize)
            }
            _ => None,
        };
        let mut frozen = [None; 32];
        if !matches!(instruction, DecodedInsn::AllocFrame { .. }) {
            for index in 0..32 {
                if (written[index] && address_inputs[index]) || frame_base == Some(index) {
                    let value = context.alloc_vreg();
                    copy(&mut prefix, pc, value, self.hex_reg(index as u8));
                    frozen[index] = Some(value);
                }
            }
            for operation in operations.iter_mut() {
                if let Some(address) = memory_address_mut(&mut operation.kind) {
                    map_address(address, &frozen);
                }
                if let Some(base) = frame_base
                    && let OpKind::Add { dst, src1, .. } = &mut operation.kind
                    && register_index(*dst) == Some(29)
                    && register_index(*src1) == Some(base)
                {
                    *src1 = frozen[base].unwrap();
                }
            }
        }
        prefix.append(operations);
        *operations = prefix;
        Ok(())
    }
}
