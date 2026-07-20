//! Fault-precise MONITOR/MWAIT lowering without host power-management ops.

use crate::smir::ir::ops::{OpKind, SmirOp, X86MonitorMwaitOp};
use crate::smir::ir::types::{Address, ArchReg, MemWidth, SignExtend, VReg, X86Reg};
use crate::smir::lower::regalloc::PhysReg;
use crate::smir::lower::{LowerError, X86_GUEST_CPL_OFFSET};

use super::{X86_64Lowerer, X86Cond, X86Emitter};

fn x86_monitor_address_shape_valid(addr: &Address, stack_segment: bool) -> bool {
    let inner = match addr {
        Address::X86Addr32(inner) if !matches!(inner.as_ref(), Address::X86Addr32(_)) => {
            inner.as_ref()
        }
        other => other,
    };
    if stack_segment {
        matches!(
            inner,
            Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rax)))
        )
    } else {
        matches!(
            inner,
            Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rax)))
                | Address::SegmentRel {
                    segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase | X86Reg::GsBase)),
                    base: Some(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
                    index: None,
                    scale: 1,
                    disp: 0,
                }
        )
    }
}

/// Validate the exact fixed implicit operands and address forms emitted by the
/// two x86 encodings. MONITOR reads RCX/EDX and RAX; MWAIT reads RCX/EAX.
pub(crate) fn x86_monitor_mwait_shape_valid(kind: &OpKind) -> bool {
    match kind {
        OpKind::X86MonitorMwait(X86MonitorMwaitOp {
            rcx: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
            hint: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
            addr: Some(addr),
            stack_segment,
        }) => x86_monitor_address_shape_valid(addr, *stack_segment),
        OpKind::X86MonitorMwait(X86MonitorMwaitOp {
            rcx: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
            hint: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
            addr: None,
            stack_segment: false,
        }) => true,
        _ => false,
    }
}

impl X86_64Lowerer {
    pub(crate) fn emit_x86_monitor_mwait(&mut self, op: &SmirOp) -> Result<(), LowerError> {
        if !self.jit_fault_deopt_guards {
            return Err(LowerError::UnsupportedOp {
                op: "X86MonitorMwait requires JIT fault-deoptimization guards".to_string(),
            });
        }
        if !x86_monitor_mwait_shape_valid(&op.kind) {
            return Err(LowerError::InvalidOperand {
                op: "X86MonitorMwait".to_string(),
                operand: "requires exact architectural implicit operands and address".to_string(),
            });
        }
        let OpKind::X86MonitorMwait(X86MonitorMwaitOp { addr, .. }) = &op.kind else {
            unreachable!("validated X86MonitorMwait shape");
        };
        if addr.is_some() && !self.mem_helpers {
            return Err(LowerError::UnsupportedOp {
                op: "MONITOR requires JIT MMU helpers".to_string(),
            });
        }
        if addr
            .as_ref()
            .is_some_and(|addr| !addr.is_x86_state_backed_shape())
        {
            return Err(LowerError::InvalidOperand {
                op: "X86MonitorMwait".to_string(),
                operand: "requires a state-backed x86 monitor address".to_string(),
            });
        }

        // Publish every identity-mapped GPR before borrowing RAX as the state
        // base. Both architectural failures deoptimize at the instruction PC;
        // the direct path then delivers #UD (CPL) or #GP(0) (RCX) precisely.
        self.code.emit_u8(0x50); // push guest RAX
        self.emit_load_state_ptr_rax();
        self.code.emit_u8(0x9C); // pushfq
        self.emit_spill_legacy_gprs_to_state_from_rax(8);

        let mut fault_branches = Vec::with_capacity(2);
        self.code.emit_bytes(&[0x48, 0x83, 0xB8]); // cmp qword [rax+cpl],0
        self.code.emit_u32(X86_GUEST_CPL_OFFSET as u32);
        self.code.emit_u8(0);
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::Ne));

        self.code.emit_bytes(&[0x48, 0x83, 0xB8]); // cmp qword [rax+RCX slot],0
        self.code.emit_u32(8);
        self.code.emit_u8(0);
        fault_branches.push(self.emit_jcc_placeholder(X86Cond::Ne));

        self.code.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx,rax
        self.emit_reload_all(PhysReg::Rcx);
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
        self.code.emit_u8(0x9D);
        self.emit_flag_preserving_stack_pop8();
        self.emit_native_exit(op.guest_pc);
        self.patch_rel32_to_current(done)?;

        let Some(addr) = addr else {
            return Ok(());
        };

        // Reserve an aligned scratch qword for the helper's discarded load
        // result. MONITOR is an ordered faulting load even though its value is
        // not architecturally visible.
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
        Ok(())
    }
}
