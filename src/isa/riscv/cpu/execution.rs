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

    fn sc(funct3: u32) -> u32 {
        (0b00011 << 27) | (2 << 20) | (1 << 15) | (funct3 << 12) | (3 << 7) | 0x2f
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

    #[test]
    fn failed_store_conditionals_check_address_without_writing() {
        const VALID: u64 = 0x200;
        const INVALID: u64 = 0x1000;
        const ORIGINAL: [u8; 8] = 0x0123_4567_89ab_cdefu64.to_le_bytes();

        for (funct3, name) in [(0b010, "SC.W"), (0b011, "SC.D")] {
            let word = sc(funct3);

            let mut direct_fault = cpu(Isa::rv64gc());
            direct_fault.set_x(1, INVALID);
            direct_fault.set_x(2, u64::MAX);
            direct_fault.set_x(3, 0xfeed_face);
            direct_fault.reservation = Some(VALID);
            assert_eq!(
                run_word(&mut direct_fault, word),
                RiscVExit::Trap(Trap {
                    cause: cause::STORE_ACCESS_FAULT,
                    tval: INVALID,
                }),
                "direct {name}"
            );
            assert_eq!(direct_fault.x(3), 0xfeed_face, "direct {name}");
            assert_eq!(direct_fault.instret(), 0, "direct {name}");
            assert_eq!(direct_fault.reservation, None, "direct {name}");

            let mut direct_valid = cpu(Isa::rv64gc());
            direct_valid.write_memory(VALID, &ORIGINAL).unwrap();
            direct_valid.set_x(1, VALID);
            direct_valid.set_x(2, u64::MAX);
            assert_eq!(run_word(&mut direct_valid, word), RiscVExit::Continue);
            assert_eq!(direct_valid.x(3), 1, "direct {name}");
            assert_eq!(direct_valid.instret(), 1, "direct {name}");
            assert_eq!(direct_valid.reservation, None, "direct {name}");
            let mut observed = [0; 8];
            direct_valid.read_memory(VALID, &mut observed).unwrap();
            assert_eq!(observed, ORIGINAL, "direct {name}");

            #[cfg(all(
                feature = "smir-jit",
                any(target_arch = "x86_64", target_arch = "aarch64")
            ))]
            for level in [
                crate::smir::optimize::OptLevel::O0,
                crate::smir::optimize::OptLevel::O2,
            ] {
                let mut jit_fault = cpu(Isa::rv64gc());
                jit_fault.set_x(1, INVALID);
                jit_fault.set_x(2, u64::MAX);
                jit_fault.set_x(3, 0xfeed_face);
                jit_fault.reservation = Some(VALID);
                assert_eq!(
                    {
                        jit_fault.write_memory(CODE, &word.to_le_bytes()).unwrap();
                        jit_fault.set_pc(CODE);
                        jit_fault.step_jit(level)
                    },
                    RiscVExit::Trap(Trap {
                        cause: cause::STORE_ACCESS_FAULT,
                        tval: INVALID,
                    }),
                    "{level:?} {name}"
                );
                assert_eq!(jit_fault.x(3), 0xfeed_face, "{level:?} {name}");
                assert_eq!(jit_fault.instret(), 0, "{level:?} {name}");
                assert_eq!(jit_fault.reservation, None, "{level:?} {name}");
                assert_eq!(
                    jit_fault.jit_stats().native_executions,
                    1,
                    "{level:?} {name}"
                );

                let mut jit_valid = cpu(Isa::rv64gc());
                jit_valid.write_memory(VALID, &ORIGINAL).unwrap();
                jit_valid.write_memory(CODE, &word.to_le_bytes()).unwrap();
                jit_valid.set_pc(CODE);
                jit_valid.set_x(1, VALID);
                jit_valid.set_x(2, u64::MAX);
                assert_eq!(jit_valid.step_jit(level), RiscVExit::Continue);
                assert_eq!(jit_valid.x(3), 1, "{level:?} {name}");
                assert_eq!(jit_valid.instret(), 1, "{level:?} {name}");
                assert_eq!(jit_valid.reservation, None, "{level:?} {name}");
                let mut observed = [0; 8];
                jit_valid.read_memory(VALID, &mut observed).unwrap();
                assert_eq!(observed, ORIGINAL, "{level:?} {name}");
                assert_eq!(
                    jit_valid.jit_stats().native_executions,
                    1,
                    "{level:?} {name}"
                );
            }
        }
    }
}
