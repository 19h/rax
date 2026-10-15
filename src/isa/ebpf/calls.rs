//! Shared call-frame policy for direct and SMIR eBPF execution.

use super::{
    EbpfCallFrame, EbpfConfig, EbpfDialect, EbpfTrap, FRAME_PTR_REG, INSN_SIZE, MM_STACK_START,
};

impl EbpfConfig {
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
            && (target_pc < start || target_pc.saturating_add(INSN_SIZE as u64) > end)
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
