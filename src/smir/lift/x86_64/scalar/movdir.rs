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
        self.lift_movdiri_modrm(modrm, prefix, bytes, pc, ctx, false)
    }

    /// Lift APX-promoted `EVEX.LLZ.NP.MAP4 F9 !(11):rrr:bbb` MOVDIRI.
    pub(crate) fn lift_apx_movdiri(
        &self,
        prefix: ApxEvexPrefix,
        bytes: &[u8],
        full_bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != 0 || !Self::apx_movdir_fields_valid(prefix) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: full_bytes.to_vec(),
            });
        }

        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = Self::decode_apx_movdir_modrm(bytes, &modrm_prefix, pc)?;
        self.lift_movdiri_modrm(modrm, &modrm_prefix, full_bytes, pc, ctx, true)
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
        self.lift_movdir64b_modrm(modrm, prefix, bytes, pc, ctx, false)
    }

    /// Lift APX-promoted `EVEX.LLZ.66.MAP4.W0 F8 !(11):rrr:bbb` MOVDIR64B.
    pub(crate) fn lift_apx_movdir64b(
        &self,
        prefix: ApxEvexPrefix,
        bytes: &[u8],
        full_bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        if prefix.pp != 1 || prefix.w || !Self::apx_movdir_fields_valid(prefix) {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: full_bytes.to_vec(),
            });
        }

        let modrm_prefix = prefix.as_modrm_prefix(prefix.bytes + 1);
        let modrm = Self::decode_apx_movdir_modrm(bytes, &modrm_prefix, pc)?;
        self.lift_movdir64b_modrm(modrm, &modrm_prefix, full_bytes, pc, ctx, true)
    }

    fn apx_movdir_fields_valid(prefix: ApxEvexPrefix) -> bool {
        !prefix.nd
            && !prefix.nf
            && !prefix.z
            && prefix.ll == 0
            && prefix.aaa == 0
            && prefix.vvvv_reg() == 0
    }

    fn decode_apx_movdir_modrm(
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
    ) -> Result<ModRm, LiftError> {
        decode_modrm(bytes, prefix, pc).map_err(|error| match error {
            LiftError::Incomplete { addr, have, need } => LiftError::Incomplete {
                addr,
                have: prefix.cursor + have,
                need: prefix.cursor + need,
            },
            other => other,
        })
    }

    fn lift_movdiri_modrm(
        &self,
        modrm: ModRm,
        prefix: &X86Prefix,
        invalid_bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
        requires_apx: bool,
    ) -> Result<LiftResult, LiftError> {
        if !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: if requires_apx {
                    invalid_bytes.to_vec()
                } else {
                    invalid_bytes[..modrm.bytes_consumed.min(invalid_bytes.len())].to_vec()
                },
            });
        }

        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let (addr, address_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
        let mut ops = if requires_apx {
            vec![SmirOp::new(OpId(0), pc, OpKind::X86RequireApx)]
        } else {
            Vec::new()
        };
        ops.extend(address_ops);
        for (index, op) in ops.iter_mut().enumerate() {
            op.id = OpId(index as u16);
        }
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

    fn lift_movdir64b_modrm(
        &self,
        modrm: ModRm,
        prefix: &X86Prefix,
        invalid_bytes: &[u8],
        pc: u64,
        ctx: &mut LiftContext,
        requires_apx: bool,
    ) -> Result<LiftResult, LiftError> {
        if !modrm.is_memory {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: if requires_apx {
                    invalid_bytes.to_vec()
                } else {
                    invalid_bytes[..modrm.bytes_consumed.min(invalid_bytes.len())].to_vec()
                },
            });
        }

        let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
        let (source_addr, mut ops) =
            self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
        if requires_apx {
            ops.insert(0, SmirOp::new(OpId(0), pc, OpKind::X86RequireApx));
            for (index, op) in ops.iter_mut().enumerate() {
                op.id = OpId(index as u16);
            }
        }
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
