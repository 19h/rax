//! Profile-dependent register forms in legacy Group 15 (`0F AE`).

use crate::smir::lift::x86_64::*;

use crate::smir::ir::TrapKind;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::*;
use crate::smir::lift::{ControlFlow, LiftContext, LiftError, LiftResult};

impl X86_64Lifter {
    /// Construct the exact terminal result shared by reserved, profile-disabled,
    /// and illegal-prefix Group-15 encodings after their complete ModR/M form
    /// has been decoded.
    pub(super) fn group15_invalid_opcode(prefix: &X86Prefix, modrm: &ModRm) -> LiftResult {
        LiftResult {
            ops: Vec::new(),
            bytes_consumed: prefix.cursor + modrm.bytes_consumed,
            control_flow: ControlFlow::Trap {
                kind: TrapKind::InvalidOpcode,
            },
            branch_targets: Vec::new(),
        }
    }

    /// Lift Group-15 forms whose meaning depends on deterministic guest-profile
    /// feature enumeration rather than the generic XSAVE/fence dispatch.
    pub(crate) fn lift_group15_profile_form(
        &self,
        prefix: &X86Prefix,
        modrm: &ModRm,
        group: u8,
        pc: u64,
        ctx: &mut LiftContext,
    ) -> Result<Option<LiftResult>, LiftError> {
        // LOCK is rejected by the owning Group-15 dispatcher. PTWRITE is the
        // one profile form handled here despite LOCK because both independent
        // causes resolve to the same terminal #UD and no operand observation.
        if prefix.lock && !(group == 4 && prefix.rep_prefix == Some(0xF3)) {
            return Ok(None);
        }

        if !modrm.is_memory && matches!(group, 0..=3) && prefix.rep_prefix == Some(0xF3) {
            // FSGSBASE has only W32 and W64 forms. A 66h prefix without W=1
            // requests the nonexistent W16 form and is therefore #UD.
            if prefix.operand_size_override && !prefix.rex_w() {
                return Ok(Some(Self::group15_invalid_opcode(prefix, modrm)));
            }
            return Ok(Some(LiftResult::fallthrough(
                vec![SmirOp::new(
                    OpId(0),
                    pc,
                    OpKind::X86FsGsBase {
                        operand: self.gpr(modrm.rm),
                        base: VReg::Arch(ArchReg::X86(if matches!(group, 0 | 2) {
                            X86Reg::FsBase
                        } else {
                            X86Reg::GsBase
                        })),
                        write: matches!(group, 2 | 3),
                        width: if prefix.rex_w() {
                            OpWidth::W64
                        } else {
                            OpWidth::W32
                        },
                        requires_apx: prefix.rex2.is_some(),
                    },
                )],
                prefix.cursor + modrm.bytes_consumed,
            )));
        }

        if group == 4 && prefix.rep_prefix == Some(0xF3) {
            // F3 0F AE /4 is PTWRITE. The fixed guest CPUID profile returns
            // zero for leaf 14H, including EBX.PTWRITE[4], so every register
            // and memory form terminates with #UD before operand observation.
            return Ok(Some(Self::group15_invalid_opcode(prefix, modrm)));
        }

        if !modrm.is_memory && group == 5 && prefix.rep_prefix == Some(0xF3) {
            // F3 0F AE /5 is INCSSPD/INCSSPQ, not LFENCE. RAX does not
            // enumerate CET shadow stacks, so every register selector is #UD.
            return Ok(Some(Self::group15_invalid_opcode(prefix, modrm)));
        }

        if modrm.is_memory || group != 6 {
            return Ok(None);
        }

        let kind = if prefix.rep_prefix == Some(0xF3) {
            let x86_addr = X86Address {
                base: Some(modrm.rm),
                index: None,
                scale: 1,
                disp: 0,
                rip_relative: false,
                address_width: if prefix.address_size_override {
                    OpWidth::W32
                } else {
                    OpWidth::W64
                },
                disp_size: DispSize::Auto,
                segment: match prefix.segment_override {
                    Some(0x64) => Some(X86Reg::FsBase),
                    Some(0x65) => Some(X86Reg::GsBase),
                    _ => None,
                },
            };
            let next_pc = pc + prefix.cursor as u64 + modrm.bytes_consumed as u64;
            let addr = if prefix.address_size_override {
                Address::X86Addr32(Box::new(self.x86_addr32_state_address(&x86_addr, next_pc)))
            } else {
                let (addr, pre_ops) = self.x86_addr_to_smir(&x86_addr, next_pc, ctx);
                debug_assert!(pre_ops.is_empty());
                addr
            };
            Some(X86WaitPkgOp::Umonitor {
                addr,
                stack_segment: prefix.segment_override == Some(0x36),
            })
        } else if prefix.rep_prefix == Some(0xF2) {
            Some(X86WaitPkgOp::Umwait {
                control: self.gpr(modrm.rm),
                deadline_low: self.gpr(0),
                deadline_high: self.gpr(2),
            })
        } else if prefix.operand_size_override {
            Some(X86WaitPkgOp::Tpause {
                control: self.gpr(modrm.rm),
                deadline_low: self.gpr(0),
                deadline_high: self.gpr(2),
            })
        } else {
            None
        };
        let Some(kind) = kind else {
            return Ok(None);
        };

        Ok(Some(LiftResult::fallthrough(
            vec![SmirOp::new(OpId(0), pc, OpKind::X86WaitPkg(kind))],
            prefix.cursor + modrm.bytes_consumed,
        )))
    }
}
