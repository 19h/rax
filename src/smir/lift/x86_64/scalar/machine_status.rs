//! Legacy machine-status instruction lifting.

use crate::smir::ir::ops::{
    OpKind, SmirOp, X86DescriptorTable, X86DescriptorTableLoadOp, X86DescriptorTableStoreOp,
    X86InvlpgOp, X86LmswOp, X86LmswSource, X86SmswOp, X86SmswTarget,
};
use crate::smir::ir::types::OpId;
use crate::smir::lift::x86_64::{X86_64Lifter, X86Prefix, decode_modrm};
use crate::smir::lift::{LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    /// Route Group 7 (`0F 01`) memory-only SGDT/SIDT/LGDT/LIDT and all
    /// SMSW/LMSW forms before the fixed-encoding system dispatcher. The
    /// ModR/M.reg opcode extension is not extended by REX or REX2; only the
    /// r/m register or memory address consumes B/X extensions. ModR/M.mod=11b
    /// `/0` through `/3` encodings belong to the fixed system-instruction
    /// space, not descriptor-table instructions.
    pub(crate) fn lift_group7_0f01(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        match bytes.first().map(|modrm| (modrm >> 6, (modrm >> 3) & 7)) {
            Some((mode, 0)) if mode != 3 => self.lift_descriptor_table_store_0f01(
                bytes,
                prefix,
                pc,
                ctx,
                X86DescriptorTable::Gdt,
            ),
            Some((mode, 1)) if mode != 3 => self.lift_descriptor_table_store_0f01(
                bytes,
                prefix,
                pc,
                ctx,
                X86DescriptorTable::Idt,
            ),
            Some((mode, 2)) if mode != 3 => self.lift_descriptor_table_load_0f01(
                bytes,
                prefix,
                pc,
                ctx,
                X86DescriptorTable::Gdt,
            ),
            Some((mode, 3)) if mode != 3 => self.lift_descriptor_table_load_0f01(
                bytes,
                prefix,
                pc,
                ctx,
                X86DescriptorTable::Idt,
            ),
            Some((_, 4)) => self.lift_smsw_0f01(bytes, prefix, pc, ctx),
            Some((_, 6)) => self.lift_lmsw_0f01(bytes, prefix, pc, ctx),
            Some((mode, 7)) if mode != 3 => self.lift_invlpg_0f01(bytes, prefix, pc, ctx),
            _ => self.lift_xcr_0f01(bytes, prefix, pc, ctx),
        }
    }

    /// Lift memory-only SGDT/SIDT (`0F 01 /0` and `/1`) in long mode.
    /// Operand-size, REX.W, and repeat prefixes do not change the fixed
    /// 10-byte payload; address-size and segment prefixes remain part of the
    /// effective-address representation.
    fn lift_descriptor_table_store_0f01(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
        table: X86DescriptorTable,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..bytes.len().min(1)].to_vec(),
            });
        }

        let modrm = decode_modrm(bytes, prefix, pc)?;
        if modrm.addr.is_none() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..bytes.len().min(modrm.bytes_consumed)].to_vec(),
            });
        }
        let bytes_consumed = prefix.cursor + modrm.bytes_consumed;
        let next_pc = pc.wrapping_add(bytes_consumed as u64);
        let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
        debug_assert!(pre_ops.is_empty());

        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(
                OpId(0),
                pc,
                OpKind::X86DescriptorTableStore(X86DescriptorTableStoreOp {
                    addr,
                    table,
                    requires_apx: prefix.rex2.is_some(),
                }),
            )],
            bytes_consumed,
        ))
    }

    /// Lift memory-only LGDT/LIDT (`0F 01 /2` and `/3`) in long mode. The
    /// operand is always the fixed 10-byte limit:base form; operand-size,
    /// REX.W, and repeat prefixes are ignored, while address-size and segment
    /// prefixes remain part of the effective address.
    fn lift_descriptor_table_load_0f01(
        &self,
        bytes: &[u8],
        prefix: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
        table: X86DescriptorTable,
    ) -> Result<LiftResult, LiftError> {
        if prefix.lock {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..bytes.len().min(1)].to_vec(),
            });
        }

        let modrm = decode_modrm(bytes, prefix, pc)?;
        if modrm.addr.is_none() {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..bytes.len().min(modrm.bytes_consumed)].to_vec(),
            });
        }
        let bytes_consumed = prefix.cursor + modrm.bytes_consumed;
        let next_pc = pc.wrapping_add(bytes_consumed as u64);
        let (addr, pre_ops) = self.x86_addr_to_smir(modrm.addr.as_ref().unwrap(), next_pc, ctx);
        debug_assert!(pre_ops.is_empty());

        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(
                OpId(0),
                pc,
                OpKind::X86DescriptorTableLoad(X86DescriptorTableLoadOp {
                    addr,
                    table,
                    requires_apx: prefix.rex2.is_some(),
                    next_pc,
                }),
            )],
            bytes_consumed,
        ))
    }

    /// Lift `SMSW r16/r32/r64` and `SMSW m16` (`0F 01 /4`).
    ///
    /// In 64-bit mode the register width follows 66H/default/REX.W, while a
    /// memory destination is always exactly 2 bytes. APX availability and
    /// CR4.UMIP privilege checks remain dynamic architectural state and are
    /// represented by the operation itself.
    fn lift_smsw_0f01(
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
        debug_assert_eq!((modrm.byte >> 3) & 7, 4);
        let bytes_consumed = prefix.cursor + modrm.bytes_consumed;
        let target = if let Some(x86_addr) = modrm.addr.as_ref() {
            let next_pc = pc.wrapping_add(bytes_consumed as u64);
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            debug_assert!(pre_ops.is_empty());
            X86SmswTarget::Memory { addr }
        } else {
            X86SmswTarget::Register {
                dst: self.gpr(modrm.rm),
                width: prefix.op_width(),
            }
        };

        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(
                OpId(0),
                pc,
                OpKind::X86Smsw(X86SmswOp {
                    target,
                    requires_apx: prefix.rex2.is_some(),
                }),
            )],
            bytes_consumed,
        ))
    }

    /// Lift fixed-width `LMSW r/m16` (`0F 01 /6`).
    ///
    /// Operand-size prefixes are ignored. APX availability and effective CPL
    /// remain dynamic architectural state; the operation also owns the exact
    /// post-serialization handoff frontier.
    fn lift_lmsw_0f01(
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
        debug_assert_eq!((modrm.byte >> 3) & 7, 6);
        let bytes_consumed = prefix.cursor + modrm.bytes_consumed;
        let source = if let Some(x86_addr) = modrm.addr.as_ref() {
            let next_pc = pc.wrapping_add(bytes_consumed as u64);
            let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
            debug_assert!(pre_ops.is_empty());
            X86LmswSource::Memory { addr }
        } else {
            X86LmswSource::Register {
                src: self.gpr(modrm.rm),
            }
        };

        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(
                OpId(0),
                pc,
                OpKind::X86Lmsw(X86LmswOp {
                    source,
                    requires_apx: prefix.rex2.is_some(),
                    next_pc: pc.wrapping_add(bytes_consumed as u64),
                }),
            )],
            bytes_consumed,
        ))
    }

    /// Lift memory-only `INVLPG m` (`0F 01 /7`). The address expression is
    /// retained without a load: privilege, APX availability, canonicality, and
    /// translation-cache invalidation belong to the atomic system operation.
    fn lift_invlpg_0f01(
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
        let Some(x86_addr) = modrm.addr.as_ref() else {
            return Err(LiftError::InvalidEncoding {
                addr: pc,
                bytes: bytes[..bytes.len().min(modrm.bytes_consumed)].to_vec(),
            });
        };
        let bytes_consumed = prefix.cursor + modrm.bytes_consumed;
        let next_pc = pc.wrapping_add(bytes_consumed as u64);
        let (addr, pre_ops) = self.x86_addr_to_smir(x86_addr, next_pc, ctx);
        debug_assert!(pre_ops.is_empty());

        Ok(LiftResult::fallthrough(
            vec![SmirOp::new(
                OpId(0),
                pc,
                OpKind::X86Invlpg(X86InvlpgOp {
                    addr,
                    requires_apx: prefix.rex2.is_some(),
                    next_pc,
                }),
            )],
            bytes_consumed,
        ))
    }
}
