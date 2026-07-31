//! EVEX VPSHLD*/VPSHRD* packed funnel-shift lifting.

use crate::smir::ir::ops::{OpKind, SmirOp, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::*;
use crate::smir::lift::x86_64::*;

impl X86_64Lifter {
    pub(crate) fn lift_evex_packed_funnel_shift(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let variable = prefix.map == X86VecMap::Map0F38;
        if prefix.encoding != VecEncodingKind::Evex
            || !matches!(prefix.map, X86VecMap::Map0F38 | X86VecMap::Map0F3A)
            || prefix.pp != X86SsePrefix::OpSize
            || prefix.l_bits == 3
            || (prefix.zeroing && prefix.aaa == 0)
            || !matches!(opcode, 0x70..=0x73)
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let elem = match (opcode & 1, prefix.w) {
            (0, true) => VecElementType::I16,
            (1, false) => VecElementType::I32,
            (1, true) => VecElementType::I64,
            _ => {
                return Err(LiftError::InvalidEncoding {
                    addr: pc,
                    bytes: bytes.to_vec(),
                });
            }
        };
        let cursor = prefix.bytes + 1;
        let modrm = decode_modrm(&bytes[cursor..], &prefix.modrm_prefix(cursor), pc)?;
        if prefix.b && (!modrm.is_memory || elem == VecElementType::I16) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }
        let end = cursor + modrm.bytes_consumed;
        if !variable && bytes.len() <= end {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: end + 1,
            });
        }
        let bytes_consumed = end + usize::from(!variable);
        let next_pc = pc + bytes_consumed as u64;
        let mut ops = Vec::new();
        let rm_operand = if modrm.is_memory {
            let broadcast = prefix.b;
            let tuple_bytes = if broadcast {
                elem.bytes()
            } else {
                prefix.width.bytes()
            };
            let (addr, pre_ops) = self.vec_disp8_addr_to_smir(
                prefix,
                modrm.addr.as_ref().unwrap(),
                next_pc,
                tuple_bytes,
                ctx,
            );
            ops.extend(pre_ops);
            let mask = (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa))));
            match (mask, broadcast) {
                (Some(mask), true) => self.append_masked_broadcast_memory_source(
                    addr,
                    elem,
                    prefix.width,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                ),
                (Some(mask), false) => self.append_evex_masked_vector_source(
                    addr,
                    elem,
                    prefix.width,
                    false,
                    mask,
                    pc,
                    ctx,
                    &mut ops,
                ),
                (None, true) => {
                    self.append_broadcast_memory_source(addr, elem, prefix.width, pc, ctx, &mut ops)
                }
                (None, false) => {
                    let vector = ctx.alloc_vreg();
                    ops.push(SmirOp::new(
                        OpId(ops.len() as u16),
                        pc,
                        OpKind::VLoad {
                            dst: vector,
                            addr,
                            width: prefix.width,
                        },
                    ));
                    vector
                }
            }
        } else {
            self.vec_reg(modrm.rm + if prefix.rm_high { 16 } else { 0 }, prefix.width)
        };
        let dst = self.vec_reg(
            modrm.reg + if prefix.reg_high { 16 } else { 0 },
            prefix.width,
        );
        let vvvv = self.vec_reg(
            prefix.vvvv + if prefix.v_high { 16 } else { 0 },
            prefix.width,
        );
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86PackedFunnelShift {
                dst,
                src: if variable { dst } else { vvvv },
                fill: if variable { vvvv } else { rm_operand },
                count: variable.then_some(rm_operand),
                mask: (prefix.aaa != 0).then_some(VReg::Arch(ArchReg::X86(X86Reg::K(prefix.aaa)))),
                amount: if variable { 0 } else { bytes[end] },
                width: prefix.width,
                elem,
                left: opcode <= 0x71,
                zeroing: prefix.zeroing,
            },
        ));
        Ok(LiftResult::fallthrough(ops, bytes_consumed))
    }
}
