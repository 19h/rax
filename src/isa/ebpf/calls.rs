//! Shared call-frame policy for direct and SMIR eBPF execution.

use super::{
    EbpfCallFrame, EbpfConfig, EbpfDialect, EbpfTrap, FRAME_PTR_REG, INSN_SIZE, MM_STACK_START,
};

/// How a call names its target; the reference interpreter orders its frame
/// push and target check differently for each form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallForm {
    /// `CALLX` (register target).
    Register,
    /// `CALL_IMM` resolved through a legacy (V0-V2) function registry.
    Registry,
    /// `CALL_IMM` with a PC-relative immediate (RFC 9669 `src=1`, SBPF
    /// V3/V4 `src=1`).
    Relative,
}

impl CallForm {
    /// Form of a lifted SMIR call: register calls are indirect; direct guest
    /// targets are registry entries only in versions without static syscalls.
    pub(crate) fn of_guest_call(dialect: EbpfDialect, indirect: bool) -> Self {
        match (indirect, dialect.sbpf_version()) {
            (true, _) => Self::Register,
            (false, Some(version)) if !version.static_syscalls() => Self::Registry,
            (false, _) => Self::Relative,
        }
    }
}

impl EbpfConfig {
    /// Enter a call at `fault_pc` in the reference order, leaving every
    /// register and frame unchanged when it traps.
    ///
    /// Anza `Interpreter::step` pushes the frame before checking a register
    /// or registry target (`CallDepthExceeded` precedes
    /// `CallOutsideTextSegment`), but resolves a V3/V4 relative call only
    /// when its target lies in the text; an unresolved relative call is an
    /// unsupported instruction (`UnsupportedSyscall` here) without a frame.
    /// RFC 9669 defines no order; its calls check the target first.
    pub(crate) fn enter_call(
        &self,
        form: CallForm,
        regs: &mut [u64; 11],
        frames: &mut Vec<EbpfCallFrame>,
        fault_pc: u64,
        target_pc: u64,
        return_pc: u64,
        key: u32,
    ) -> std::result::Result<(), EbpfTrap> {
        match (self.dialect, form) {
            (EbpfDialect::Solana(_), CallForm::Register | CallForm::Registry) => {
                let saved_frame_pointer = regs[FRAME_PTR_REG as usize];
                self.push_frame(regs, frames, fault_pc, return_pc)?;
                if let Err(trap) = self.validate_target_pc(fault_pc, target_pc) {
                    frames.pop();
                    regs[FRAME_PTR_REG as usize] = saved_frame_pointer;
                    return Err(trap);
                }
                Ok(())
            }
            (EbpfDialect::Solana(_), CallForm::Relative) => {
                if self.validate_target_pc(fault_pc, target_pc).is_err() {
                    return Err(EbpfTrap::UnsupportedSyscall { pc: fault_pc, key });
                }
                self.push_frame(regs, frames, fault_pc, return_pc)
            }
            (EbpfDialect::Rfc9669, _) => {
                self.validate_target_pc(fault_pc, target_pc)?;
                self.push_frame(regs, frames, fault_pc, return_pc)
            }
        }
    }

    pub(crate) fn initial_frame_pointer(&self) -> u64 {
        let stack_size = if self
            .dialect
            .sbpf_version()
            .is_some_and(|v| v.manual_stack_frame_bump())
        {
            self.stack_frame_size
                .saturating_mul(self.max_call_depth as u64)
        } else {
            self.stack_frame_size
        };
        MM_STACK_START.saturating_add(stack_size)
    }

    pub(crate) fn validate_target_pc(
        &self,
        fault_pc: u64,
        target_pc: u64,
    ) -> std::result::Result<(), EbpfTrap> {
        if target_pc % (INSN_SIZE as u64) != 0 {
            return Err(EbpfTrap::CallOutsideTextSegment {
                pc: fault_pc,
                target_pc,
            });
        }
        if let Some((start, end)) = self.text_range
            && (target_pc < start
                || target_pc
                    .checked_add(INSN_SIZE as u64)
                    .is_none_or(|next_pc| next_pc > end))
        {
            return Err(EbpfTrap::CallOutsideTextSegment {
                pc: fault_pc,
                target_pc,
            });
        }
        Ok(())
    }

    pub(crate) fn push_frame(
        &self,
        regs: &mut [u64; 11],
        frames: &mut Vec<EbpfCallFrame>,
        fault_pc: u64,
        return_pc: u64,
    ) -> std::result::Result<(), EbpfTrap> {
        if frames.len() + 1 >= self.max_call_depth {
            return Err(EbpfTrap::CallDepthExceeded { pc: fault_pc });
        }
        let frame = EbpfCallFrame {
            caller_saved_regs: [regs[6], regs[7], regs[8], regs[9]],
            frame_pointer: regs[FRAME_PTR_REG as usize],
            return_pc,
        };
        frames.push(frame);

        match self.dialect {
            EbpfDialect::Solana(ver) => {
                if !ver.manual_stack_frame_bump() {
                    let mult = if ver.stack_frame_gaps() && self.enable_stack_frame_gaps {
                        2
                    } else {
                        1
                    };
                    let step = self.stack_frame_size.wrapping_mul(mult);
                    regs[FRAME_PTR_REG as usize] = regs[FRAME_PTR_REG as usize].wrapping_add(step);
                }
            }
            EbpfDialect::Rfc9669 => {
                regs[FRAME_PTR_REG as usize] =
                    regs[FRAME_PTR_REG as usize].wrapping_sub(self.stack_frame_size);
            }
        }
        Ok(())
    }
}
