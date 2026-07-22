//! Original VEX and Intel APX-promoted EVEX CMPccXADD lifting.

use super::*;

impl X86_64Lifter {
    fn cmpccxadd_modrm_byte(prefix_bytes: usize, bytes: &[u8], pc: u64) -> Result<u8, LiftError> {
        bytes
            .get(prefix_bytes + 1)
            .copied()
            .ok_or(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: prefix_bytes + 2,
            })
    }

    pub(crate) fn lift_cmpccxadd(
        &self,
        prefix: VecPrefix,
        opcode: u8,
        bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let (modrm_prefix, add, width, prefix_bytes) = match prefix.encoding {
            VecEncodingKind::Vex => {
                // Intel SDM Vol. 2A, CMPccXADD, permits exactly
                // VEX.128.66.0F38.W{0,1}; pp and L are known at the opcode
                // frontier and do not make a following ModR/M observable.
                if prefix.pp != X86SsePrefix::OpSize || prefix.width != VecWidth::V128 {
                    return Ok(Self::apx_invalid_opcode(prefix.bytes + 1));
                }
                let width = if prefix.w { MemWidth::B8 } else { MemWidth::B4 };
                (
                    X86Prefix {
                        rex: prefix.rex,
                        operand_size_override: true,
                        address_size_override: prefix.address_size_override,
                        segment_override: prefix.segment_override,
                        cursor: prefix.bytes + 1,
                        ..X86Prefix::default()
                    },
                    self.gpr(prefix.vvvv),
                    width,
                    prefix.bytes,
                )
            }
            VecEncodingKind::Evex => {
                let apx = decode_apx_evex_prefix_for_map(bytes, pc, 2)?;
                // APX revision 5.0 exception class APX-EVEX-CMPCCXADD permits
                // only V4 in payload byte 2. L is listed separately as #UD,
                // so both L bits, NF, ND, masking, zeroing, and every other
                // payload decoration are rejected here.
                if apx.pp != 1 || bytes[apx.bytes - 1] & !0x08 != 0 {
                    return Ok(Self::apx_invalid_opcode(apx.bytes + 1));
                }
                let width = if apx.w { MemWidth::B8 } else { MemWidth::B4 };
                (
                    apx.as_modrm_prefix(apx.bytes + 1),
                    self.gpr(apx.vvvv_reg()),
                    width,
                    apx.bytes,
                )
            }
        };

        let modrm_byte = Self::cmpccxadd_modrm_byte(prefix_bytes, bytes, pc)?;
        if modrm_byte >> 6 == 3 {
            return Ok(Self::apx_invalid_opcode(prefix_bytes + 2));
        }

        let modrm = decode_modrm(&bytes[prefix_bytes + 1..], &modrm_prefix, pc)?;
        debug_assert!(modrm.is_memory);
        let next_pc = pc + prefix_bytes as u64 + 1 + modrm.bytes_consumed as u64;
        let x86_addr = modrm.addr.as_ref().unwrap();
        let (addr, mut ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
        let cmp = self.gpr(modrm.reg);
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::AtomicCmpXadd {
                dst_old: cmp,
                addr,
                cmp,
                add,
                cond: self.x86_cond(opcode & 0x0F),
                width,
                order: MemoryOrder::SeqCst,
            },
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix_bytes + 1 + modrm.bytes_consumed,
        ))
    }
}
