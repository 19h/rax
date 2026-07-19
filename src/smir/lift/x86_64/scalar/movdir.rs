//! MOVDIRI and MOVDIR64B lifting.

use crate::smir::lift::x86_64::*;

use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::*;
use crate::smir::lift::{LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    /// Lift legacy MOVDIRI m32,r32 / m64,r64 (0F 38 F9 /r).
    pub(crate) fn lift_movdiri_0f38(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        // 66 selects no defined 16-bit form. F2/F3 are ignorable legacy
        // prefixes, while LOCK and a legacy REX2 form are not valid encodings.
        if prefix.lock || prefix.operand_size_override || prefix.rex2.is_some() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let modrm = decode_modrm(bytes, prefix, pc)?;
        if !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..modrm.bytes_consumed.min(bytes.len())].to_vec(),
            });
        }

        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let (addr, mut ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::Store {
                src: self.gpr(modrm.reg),
                addr,
                width: if prefix.rex_w() {
                    MemWidth::B8
                } else {
                    MemWidth::B4
                },
            },
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }

    /// Lift legacy MOVDIR64B r64,m512 (66 0F 38 F8 /r) in 64-bit mode.
    pub(crate) fn lift_movdir64b_0f38(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        // 66 is mandatory. F2/F3 are refining-prefix conflicts, and neither
        // LOCK nor the legacy REX2 encoding defines a MOVDIR64B form.
        if prefix.lock
            || !prefix.operand_size_override
            || prefix.rep_prefix.is_some()
            || prefix.rex2.is_some()
        {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes.to_vec(),
            });
        }

        let modrm = decode_modrm(bytes, prefix, pc)?;
        if !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..modrm.bytes_consumed.min(bytes.len())].to_vec(),
            });
        }

        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let (source_addr, mut ops) =
            self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
        let destination_offset = Address::Direct(self.gpr(modrm.reg));
        let destination_addr = if prefix.address_size_override {
            Address::X86Addr32(Box::new(destination_offset))
        } else {
            destination_offset
        };

        // Architecturally the destination alignment fault precedes the source
        // read. VLoad buffers the complete source before the single VStore,
        // preserving overlap behavior and the 64-byte write transaction.
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::X86CheckAlignment {
                addr: destination_addr.clone(),
                alignment: 64,
            },
        ));
        let value = ctx.alloc_vreg();
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VLoad {
                dst: value,
                addr: source_addr,
                width: VecWidth::V512,
            },
        ));
        ops.push(SmirOp::new(
            OpId(ops.len() as u16),
            pc,
            OpKind::VStore {
                src: value,
                addr: destination_addr,
                width: VecWidth::V512,
            },
        ));

        Ok(LiftResult::fallthrough(
            ops,
            prefix.cursor + modrm.bytes_consumed,
        ))
    }
}
