//! AMD XOP scalar TBM lifting.

use crate::smir::ir::TrapKind;
use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{OpKind, SmirOp, X86TbmKind};
use crate::smir::ir::types::{MemWidth, OpId, OpWidth, SignExtend, VReg};
use crate::smir::lift::x86_64::{X86_64Lifter, X86Prefix, build_rex, decode_modrm};
use crate::smir::lift::{ControlFlow, LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    pub(crate) fn xop_invalid(bytes_consumed: usize) -> LiftResult {
        LiftResult {
            ops: Vec::new(),
            bytes_consumed,
            control_flow: ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode,
            },
            branch_targets: Vec::new(),
        }
    }

    fn xop_unsupported(pc: u64, map: u8, opcode: u8) -> LiftError {
        LiftError::Unsupported {
            addr: pc,
            mnemonic: format!("XOP map {map:#04x} opcode {opcode:#04x}"),
        }
    }

    /// Lift AMD XOP maps 8-10. This semantic unit admits all ten scalar TBM
    /// instructions and delegates packed rotate/shift, VPCMOV, and VPCOM cells
    /// to the vector XOP lifter; other assigned XOP cells remain explicit
    /// interpreter frontiers rather than being mislabeled #UD.
    pub(crate) fn lift_xop(
        &self,
        bytes: &[u8],
        legacy: &X86Prefix,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<LiftResult, LiftError> {
        let lead = legacy.cursor;
        let opcode_end = lead + 4;
        if bytes.len() < opcode_end {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: opcode_end,
            });
        }

        // XOP accepts only segment and address-size overrides before 8FH.
        let forbidden_legacy_prefix = bytes[..lead]
            .iter()
            .any(|byte| !matches!(byte, 0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x67));
        if forbidden_legacy_prefix
            || legacy.rex.is_some()
            || legacy.rex2.is_some()
            || legacy.operand_size_override
            || legacy.rep_prefix.is_some()
            || legacy.lock
        {
            return Ok(Self::xop_invalid(opcode_end));
        }

        let p0 = bytes[lead + 1];
        let p1 = bytes[lead + 2];
        let opcode = bytes[lead + 3];
        let map = p0 & 0x1f;
        if !(8..=10).contains(&map) {
            return Ok(Self::xop_invalid(opcode_end));
        }

        let w = p1 & 0x80 != 0;
        let vvvv = ((p1 >> 3) & 0x0f) ^ 0x0f;
        let l = (p1 >> 2) & 1;
        let pp = p1 & 3;
        let scalar_tbm =
            (map == 9 && matches!(opcode, 0x01 | 0x02)) || (map == 10 && opcode == 0x10);
        let packed_bit = (map == 8 && matches!(opcode, 0xC0..=0xC3))
            || (map == 9 && matches!(opcode, 0x90..=0x9B));
        if packed_bit {
            return self.lift_xop_packed_bit(
                bytes, legacy, pc, ctx, lead, p0, opcode, map, w, vvvv, l, pp,
            );
        }
        if map == 8 && opcode == 0xA2 {
            return self.lift_xop_vpcmov(bytes, legacy, pc, ctx, lead, p0, w, vvvv, l, pp);
        }
        if map == 8 && matches!(opcode, 0xCC..=0xCF | 0xEC..=0xEF) {
            return self.lift_xop_vpcom(bytes, legacy, pc, ctx, lead, p0, opcode, w, vvvv, l, pp);
        }
        if !scalar_tbm {
            return Err(Self::xop_unsupported(pc, map, opcode));
        }
        if l != 0 || pp != 0 || (map == 10 && vvvv != 0) {
            return Ok(Self::xop_invalid(opcode_end));
        }

        let r = ((p0 >> 7) & 1) ^ 1;
        let x = ((p0 >> 6) & 1) ^ 1;
        let b = ((p0 >> 5) & 1) ^ 1;
        let modrm_offset = opcode_end;
        let modrm_prefix = X86Prefix {
            rex: build_rex(r, x, b, w),
            address_size_override: legacy.address_size_override,
            segment_override: legacy.segment_override,
            cursor: modrm_offset,
            ..X86Prefix::default()
        };

        let modrm =
            decode_modrm(&bytes[modrm_offset..], &modrm_prefix, pc).map_err(
                |error| match error {
                    LiftError::Incomplete { addr, have, need } => LiftError::Incomplete {
                        addr,
                        have: modrm_offset + have,
                        need: modrm_offset + need,
                    },
                    error => error,
                },
            )?;

        let kind = if map == 9 {
            match (opcode, (modrm.byte >> 3) & 7) {
                (0x01, 1) => Some(X86TbmKind::Blcfill),
                (0x01, 2) => Some(X86TbmKind::Blsfill),
                (0x01, 3) => Some(X86TbmKind::Blcs),
                (0x01, 4) => Some(X86TbmKind::Tzmsk),
                (0x01, 5) => Some(X86TbmKind::Blcic),
                (0x01, 6) => Some(X86TbmKind::Blsic),
                (0x01, 7) => Some(X86TbmKind::T1mskc),
                (0x02, 1) => Some(X86TbmKind::Blcmsk),
                (0x02, 6) => Some(X86TbmKind::Blci),
                _ => return Ok(Self::xop_invalid(modrm_offset + 1)),
            }
        } else {
            None
        };

        let immediate_bytes = if map == 10 { 4 } else { 0 };
        let bytes_consumed = modrm_offset + modrm.bytes_consumed + immediate_bytes;
        if bytes.len() < bytes_consumed {
            return Err(LiftError::Incomplete {
                addr: pc,
                have: bytes.len(),
                need: bytes_consumed,
            });
        }
        let next_pc = pc.wrapping_add(bytes_consumed as u64);
        let width = if w { OpWidth::W64 } else { OpWidth::W32 };
        let mem_width = if w { MemWidth::B8 } else { MemWidth::B4 };

        // The dynamic feature guard must remain before address-generation ops
        // and the Load so disabled TBM deoptimizes to the direct #UD path
        // without exposing #GP/#PF/#AC from the source operand.
        let mut ops = vec![SmirOp::new(OpId(0), pc, OpKind::X86RequireTbm)];
        let src = if modrm.is_memory {
            let (addr, pre_ops) = self.x86_addr_to_smir(
                modrm.addr.as_ref().expect("decoded XOP memory address"),
                next_pc,
                ctx,
            );
            ops.extend(pre_ops);
            let tmp = ctx.alloc_vreg();
            ops.push(SmirOp::new(
                OpId(ops.len() as u16),
                pc,
                OpKind::Load {
                    dst: tmp,
                    addr,
                    width: mem_width,
                    sign: SignExtend::Zero,
                },
            ));
            tmp
        } else {
            self.gpr(modrm.rm)
        };

        let defined = FlagUpdate::Specific(
            FlagSet::CF
                .union(FlagSet::ZF)
                .union(FlagSet::SF)
                .union(FlagSet::OF),
        );
        let op = if let Some(kind) = kind {
            OpKind::X86Tbm {
                dst: self.gpr(vvvv),
                src,
                width,
                kind,
                flags: defined,
            }
        } else {
            let control_offset = modrm_offset + modrm.bytes_consumed;
            let control = u32::from_le_bytes(
                bytes[control_offset..control_offset + 4]
                    .try_into()
                    .expect("validated immediate length"),
            );
            OpKind::Bextr {
                dst: self.gpr(modrm.reg),
                src,
                control: VReg::Imm(i64::from(control)),
                width,
                flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF)),
            }
        };
        ops.push(SmirOp::new(OpId(ops.len() as u16), pc, op));

        Ok(LiftResult::fallthrough(ops, bytes_consumed))
    }
}
