//! Instruction fetch, execution, and retirement accounting.

use super::*;

impl RiscVCpu {
    /// Fetch, decode and execute one instruction.
    pub fn step(&mut self) -> RiscVExit {
        if let Some(trap) = self.pending_machine_interrupt() {
            self.deliver_trap(trap, self.pc);
            return RiscVExit::Continue;
        }

        let pc = self.pc;
        let insn = match decode_at(self.mem.as_ref(), pc, self.cfg.xlen, &self.cfg.isa) {
            Ok(i) => i,
            Err(DecodeError::Fetch(_)) => {
                let trap = Trap {
                    cause: cause::INSTR_ACCESS_FAULT,
                    tval: pc,
                };
                self.deliver_trap(trap, pc);
                return RiscVExit::Trap(trap);
            }
        };
        self.cycle = self.cycle.wrapping_add(1);
        match self.execute(&insn, pc) {
            Ok(exit) => {
                self.account_retired_exit(exit);
                exit
            }
            Err(trap) => {
                self.deliver_trap(trap, pc);
                RiscVExit::Trap(trap)
            }
        }
    }

    /// Run until a non-`Continue` exit or `max_insns` instructions retire.
    /// Returns the exit that stopped the loop (`Continue` only if the budget
    /// was exhausted).
    pub fn run(&mut self, max_insns: u64) -> RiscVExit {
        for _ in 0..max_insns {
            match self.step() {
                RiscVExit::Continue => {}
                other => return other,
            }
        }
        RiscVExit::Continue
    }

    /// Account for an instruction that returned through the non-trap path.
    /// ECALL and EBREAK still raise synchronous exceptions through embedder
    /// exits, so they do not retire. WFI completes as a hint in this model.
    #[inline]
    pub(super) fn account_retired_exit(&mut self, exit: RiscVExit) {
        if matches!(exit, RiscVExit::Continue | RiscVExit::Wfi) {
            self.instret = self.instret.wrapping_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::isa::riscv::FlatMemory;

    const CODE: u64 = 0x100;

    fn cpu(isa: Isa) -> RiscVCpu {
        RiscVCpu::new(
            RiscVConfig {
                xlen: Xlen::Rv64,
                isa,
            },
            Box::new(FlatMemory::new(0, 0x1000)),
        )
    }

    fn run_word(cpu: &mut RiscVCpu, word: u32) -> RiscVExit {
        cpu.write_memory(CODE, &word.to_le_bytes()).unwrap();
        cpu.set_pc(CODE);
        cpu.step()
    }

    fn csrr(rd: u32, csr: u32) -> u32 {
        (csr << 20) | (0b010 << 12) | (rd << 7) | 0x73
    }

    #[test]
    fn only_normally_completed_instructions_increment_instret() {
        for (word, expected_exit, expected_instret) in [
            (0x0000_0013, RiscVExit::Continue, 1), // addi x0, x0, 0
            (0x1050_0073, RiscVExit::Wfi, 1),      // wfi completes as a hint
            (0x0000_0073, RiscVExit::Ecall, 0),
            (0x0010_0073, RiscVExit::Ebreak, 0),
        ] {
            let mut cpu = cpu(Isa::rv64gc());
            assert_eq!(run_word(&mut cpu, word), expected_exit);
            assert_eq!(cpu.cycle, 1, "word={word:#010x}");
            assert_eq!(cpu.instret(), expected_instret, "word={word:#010x}");
        }
    }

    #[test]
    fn unavailable_csrs_trap_before_register_commit() {
        let isa = Isa {
            zicsr: true,
            ..Isa::rv_i()
        };
        for csr in [0xC80, 0x001, 0x017, 0x008] {
            let mut cpu = cpu(isa);
            cpu.set_x(1, 0xfeed_face);
            assert_eq!(
                run_word(&mut cpu, csrr(1, csr)),
                RiscVExit::Trap(Trap::illegal(0)),
                "csr={csr:#05x}"
            );
            assert_eq!(cpu.x(1), 0xfeed_face, "csr={csr:#05x}");
            assert_eq!(cpu.instret(), 0, "csr={csr:#05x}");
        }
    }
}
