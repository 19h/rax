//! Fault-precise WAITPKG lowering without host power-management instructions.

use crate::smir::ir::ops::{OpKind, SmirOp, X86WaitPkgOp};
use crate::smir::ir::types::{Address, ArchReg, MemWidth, OpWidth, SignExtend, VReg, X86Reg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{
    LowerError, X86_GUEST_CPL_OFFSET, X86_GUEST_CR0_OFFSET, X86_GUEST_CR4_OFFSET,
};

use super::{X86_64Lowerer, X86Cond, X86Emitter};

fn x86_waitpkg_gpr(reg: &VReg) -> Option<u8> {
    match reg {
        VReg::Arch(ArchReg::X86(reg)) => reg.gpr_index(),
        _ => None,
    }
}

fn x86_waitpkg_monitor_address_shape_valid(addr: &Address, stack_segment: bool) -> bool {
    let inner = match addr {
        Address::X86Addr32(inner) if !matches!(inner.as_ref(), Address::X86Addr32(_)) => {
            inner.as_ref()
        }
        other => other,
    };
    match inner {
        Address::Direct(base) => x86_waitpkg_gpr(base).is_some(),
        Address::SegmentRel {
            segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase | X86Reg::GsBase)),
            base: Some(base),
            index: None,
            scale: 1,
            disp: 0,
        } => !stack_segment && x86_waitpkg_gpr(base).is_some(),
        _ => false,
    }
}

/// Accept only the exact architectural operands emitted by the x86 lifter.
pub(crate) fn x86_waitpkg_shape_valid(op: &SmirOp) -> bool {
    if op.x86_hint.is_some() {
        return false;
    }
    match &op.kind {
        OpKind::X86WaitPkg(X86WaitPkgOp::Umonitor {
            addr,
            stack_segment,
        }) => x86_waitpkg_monitor_address_shape_valid(addr, *stack_segment),
        OpKind::X86WaitPkg(
            X86WaitPkgOp::Umwait {
                control,
                deadline_low: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                deadline_high: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
            }
            | X86WaitPkgOp::Tpause {
                control,
                deadline_low: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                deadline_high: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
            },
        ) => x86_waitpkg_gpr(control).is_some(),
        _ => false,
    }
}

impl X86_64Lowerer {
    pub(crate) fn emit_x86_waitpkg(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !x86_waitpkg_shape_valid(op) {
            return Err(LowerError::InvalidOperand {
                op: "X86WaitPkg".to_string(),
                operand: "requires exact UMONITOR address or wait control/deadline operands"
                    .to_string(),
            });
        }

        let OpKind::X86WaitPkg(wait) = &op.kind else {
            unreachable!("validated X86WaitPkg shape changed")
        };
        if let X86WaitPkgOp::Umonitor { addr, .. } = wait {
            if !self.mem_helpers {
                return Err(LowerError::UnsupportedOp {
                    op: "UMONITOR requires JIT MMU helpers".to_string(),
                });
            }
            if !addr.is_x86_state_backed_shape() {
                return Err(LowerError::InvalidOperand {
                    op: "X86WaitPkg UMONITOR".to_string(),
                    operand: "requires a state-backed x86 address".to_string(),
                });
            }

            // UMONITOR is an ordered, faulting byte read whose data result is
            // discarded. Helper failure hands the exact instruction back to
            // direct execution before any architectural state is committed.
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, -16);
            }
            self.emit_jit_mem_op(
                op.guest_pc,
                true,
                None,
                Some(16),
                None,
                None,
                None,
                addr,
                MemWidth::B1,
                SignExtend::Zero,
                16,
            )?;
            {
                let mut emitter = X86Emitter::new(&mut self.code);
                emitter.emit_lea(PhysReg::Rsp, PhysReg::Rsp, 16);
            }
            return Ok(());
        }

        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "UMWAIT/TPAUSE require JIT fault-deoptimization guards".to_string(),
            });
        }
        let control = match wait {
            X86WaitPkgOp::Umwait { control, .. } | X86WaitPkgOp::Tpause { control, .. } => {
                x86_waitpkg_gpr(control).expect("validated WAITPKG control")
            }
            X86WaitPkgOp::Umonitor { .. } => unreachable!(),
        };

        // Publish identity-mapped GPRs before borrowing RAX as the state base.
        // All guard failures restore the complete entry image and deoptimize at
        // the WAITPKG instruction before its deterministic flag update.
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.code.emit_u8(0x9C); // pushfq
        self.emit_spill_legacy_gprs_to_state_from_rax(8);

        let mut fault_branches = Vec::with_capacity(2);
        self.code.emit_bytes(&[0xF7, 0x80]); // test dword [rax+control],!1
        self.code.emit_u32(u32::from(control) * 8);
        self.code.emit_u32(!1_u32);
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::Ne));

        // #GP(0) iff protected mode is active, CR4.TSD=1, and effective CPL
        // is nonzero. Runtime admission records virtual-8086 mode as CPL3.
        self.code.emit_bytes(&[0xF7, 0x80]); // test dword [rax+cr0],1
        self.code.emit_u32(X86_GUEST_CR0_OFFSET as u32);
        self.code.emit_u32(1);
        let real_mode = self.emit_jcc_placeholder(X86Cond::E);

        self.code.emit_bytes(&[0xF7, 0x80]); // test dword [rax+cr4],TSD
        self.code.emit_u32(X86_GUEST_CR4_OFFSET as u32);
        self.code.emit_u32(1 << 2);
        let tsd_clear = self.emit_jcc_placeholder(X86Cond::E);

        self.code.emit_bytes(&[0x48, 0x83, 0xB8]); // cmp qword [rax+cpl],0
        self.code.emit_u32(X86_GUEST_CPL_OFFSET as u32);
        self.code.emit_u8(0);
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::Ne));

        self.patch_rel32_to_current(real_mode)?;
        self.patch_rel32_to_current(tsd_clear)?;

        self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
        self.emit_reload_all(PhysReg::Rcx);
        // The deterministic immediate wake clears every WAITPKG status flag
        // while preserving all control and system bits in the saved image.
        self.code.emit_bytes(&[
            0x48, 0x81, 0x24, 0x24, 0x2A, 0xF7, 0xFF, 0xFF, // and [rsp],!0x08D5
        ]);
        self.code.emit_u8(0x9D); // popfq
        self.emit_flag_preserving_stack_pop8();
        self.code.emit_u8(0xE9);
        let done = self.code.position();
        self.code.emit_u32(0);

        let fault = self.code.position();
        for branch in fault_branches {
            let rel = fault as i64 - branch as i64 - 4;
            if rel < i32::MIN as i64 || rel > i32::MAX as i64 {
                return Err(LowerError::RelocationOutOfRange {
                    offset: branch,
                    target: fault,
                });
            }
            self.code.patch_i32(branch, rel as i32);
        }
        self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
        self.emit_reload_all(PhysReg::Rcx);
        self.code.emit_u8(0x9D); // original flags
        self.emit_flag_preserving_stack_pop8();
        self.emit_native_exit(op.guest_pc);

        self.patch_rel32_to_current(done)?;
        Ok(())
    }
}
