use super::*;
use rax::smir::ir::types::{AtomicOp, FenceKind, MemWidth, MemoryOrder};

struct ChangingMemory {
    inner: FlatMemory,
    reads: usize,
}
impl SmirMemory for ChangingMemory {
    fn read(&mut self, addr: u64, out: &mut [u8]) -> Result<(), MemoryError> {
        if addr == 0x100 {
            self.reads += 1;
            out.copy_from_slice(&(self.reads as u64).to_le_bytes()[..out.len()]);
            Ok(())
        } else {
            SmirMemory::read(&mut self.inner, addr, out)
        }
    }
    fn write(&mut self, a: u64, b: &[u8]) -> Result<(), MemoryError> {
        self.inner.write(a, b)
    }
    fn probe(&self, a: u64, n: usize, w: bool) -> Result<(), MemoryError> {
        self.inner.probe(a, n, w)
    }
    fn atomic_load(&mut self, a: u64, w: MemWidth, o: MemoryOrder) -> Result<u64, MemoryError> {
        self.inner.atomic_load(a, w, o)
    }
    fn atomic_store(
        &mut self,
        a: u64,
        v: u64,
        w: MemWidth,
        o: MemoryOrder,
    ) -> Result<(), MemoryError> {
        self.inner.atomic_store(a, v, w, o)
    }
    fn atomic_rmw(
        &mut self,
        a: u64,
        op: AtomicOp,
        v: u64,
        w: MemWidth,
        o: MemoryOrder,
    ) -> Result<u64, MemoryError> {
        self.inner.atomic_rmw(a, op, v, w, o)
    }
    fn compare_and_swap(
        &mut self,
        a: u64,
        e: u64,
        n: u64,
        w: MemWidth,
        s: MemoryOrder,
        f: MemoryOrder,
    ) -> Result<(u64, bool), MemoryError> {
        self.inner.compare_and_swap(a, e, n, w, s, f)
    }
    fn load_exclusive(&mut self, a: u64, w: MemWidth) -> Result<u64, MemoryError> {
        self.inner.load_exclusive(a, w)
    }
    fn store_exclusive(&mut self, a: u64, v: u64, w: MemWidth) -> Result<bool, MemoryError> {
        self.inner.store_exclusive(a, v, w)
    }
    fn clear_exclusive(&mut self) {
        self.inner.clear_exclusive();
    }
    fn fence(&mut self, _: FenceKind) {}
}

fn interpreter(program: &[u8], version: Option<SbpfVersion>, opt: OptLevel) -> SmirInterpreter {
    let mut lifter = match version {
        Some(v) => EbpfLifter::solana(v),
        None => EbpfLifter::rfc9669(),
    };
    let mut function = lifter
        .lift_function(
            0,
            &CodeSlice(program),
            &mut LiftContext::new(lifter.source_arch()),
        )
        .unwrap();
    optimize_function(&mut function, opt);
    let mut interpreter = SmirInterpreter::new();
    interpreter.add_function(function);
    interpreter
}

#[test]
fn optimization_preserves_load_count_and_observable_memory_changes() {
    let program = [
        encode_insn(0x79, 0, 2, 0, 0, Endian::Little),
        encode_insn(0x79, 1, 2, 0, 0, Endian::Little),
        encode_insn(0x1f, 0, 1, 0, 0, Endian::Little),
        encode_insn(0x95, 0, 0, 0, 0, Endian::Little),
    ]
    .concat();
    for opt in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut memory = ChangingMemory {
            inner: FlatMemory::new(512),
            reads: 0,
        };
        let mut context = SmirContext::new_ebpf();
        context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(2)), 0x100);
        assert!(
            matches!(
                interpreter(&program, None, opt).run(&mut context, &mut memory),
                ExitReason::Return { to: u64::MAX }
            ),
            "{opt:?}"
        );
        assert_eq!(memory.reads, 2);
    }
}

#[test]
fn stepping_and_breakpoints_stop_at_guest_instruction_boundaries() {
    let program = [
        encode_insn(0xb7, 0, 0, 0, 1, Endian::Little),
        encode_insn(0x07, 0, 0, 0, 2, Endian::Little),
        encode_insn(0x95, 0, 0, 0, 0, Endian::Little),
    ]
    .concat();
    for opt in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut context = SmirContext::new_ebpf();
        context.debug.single_step = true;
        let mut interpreter = interpreter(&program, None, opt);
        let mut memory = FlatMemory::new(32);
        assert!(matches!(
            interpreter.run(&mut context, &mut memory),
            ExitReason::SingleStep
        ));
        assert_eq!(context.pc, 8);
        assert_eq!(context.insn_count, 1);
        assert_eq!(context.read_arch_reg(ArchReg::Ebpf(EbpfReg::R(0))), 1);
        assert!(matches!(
            interpreter.run(&mut context, &mut memory),
            ExitReason::SingleStep
        ));
        assert_eq!(context.pc, 16);
        assert_eq!(context.insn_count, 2);
        assert!(matches!(
            interpreter.run(&mut context, &mut memory),
            ExitReason::Return { to: 3 }
        ));
        let mut context = SmirContext::new_ebpf();
        context.debug.add_breakpoint(8);
        assert!(matches!(
            interpreter.run(&mut context, &mut memory),
            ExitReason::Breakpoint { addr: 8 }
        ));
        assert_eq!(context.insn_count, 1);
        assert_eq!(context.pc, 8);
        context.debug.remove_breakpoint(8);
        assert!(matches!(
            interpreter.run(&mut context, &mut memory),
            ExitReason::Return { to: 3 }
        ));
    }
}

#[test]
fn sbpf_division_traps_preserve_exact_cause_pc_and_destination() {
    for version in [
        SbpfVersion::V0,
        SbpfVersion::V1,
        SbpfVersion::V2,
        SbpfVersion::V3,
        SbpfVersion::V4,
    ] {
        let cases = if version == SbpfVersion::V2 {
            vec![
                (0x4e, 1, 0, false),
                (0x5e, 1, 0, false),
                (0xce, i32::MIN as u32 as u64, u64::MAX, true),
                (0xde, i64::MIN as u64, u64::MAX, true),
            ]
        } else {
            vec![(0x3c, 1, 0, false), (0x3f, 1, 0, false)]
        };
        for (opcode, dividend, divisor, overflow) in cases {
            let expected = if overflow {
                EbpfTrap::DivideOverflow { pc: 8 }
            } else {
                EbpfTrap::DivideByZero { pc: 8 }
            };
            let program = [
                encode_insn(0xb7, 0, 0, 0, 5, Endian::Little),
                encode_insn(opcode, 1, 2, 0, 0, Endian::Little),
                encode_insn(0x95, 0, 0, 0, 0, Endian::Little),
            ]
            .concat();
            for opt in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                let mut context = SmirContext::new_ebpf();
                context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(1)), dividend);
                context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(2)), divisor);
                let outcome = interpreter(&program, Some(version), opt)
                    .run(&mut context, &mut FlatMemory::new(32));
                assert!(
                    matches!(outcome, ExitReason::EbpfTrap(actual) if actual == expected),
                    "{version:?} {opt:?}: {outcome:?}"
                );
                assert_eq!(context.pc, 8);
                assert_eq!(context.insn_count, 2);
                assert_eq!(
                    context.read_arch_reg(ArchReg::Ebpf(EbpfReg::R(1))),
                    dividend
                );
            }
        }
    }
}

#[test]
fn legacy_call_key_collision_obeys_reference_external_then_internal_order() {
    for version in [SbpfVersion::V0, SbpfVersion::V1, SbpfVersion::V2] {
        let program = [
            encode_insn(0x85, 0, 0, 0, 1234, Endian::Little),
            encode_insn(0x07, 0, 0, 0, 2, Endian::Little),
            encode_insn(0x95, 0, 0, 0, 0, Endian::Little),
            encode_insn(0x07, 0, 0, 0, 0, Endian::Little),
            encode_insn(0xb7, 6, 0, 0, 600, Endian::Little),
            encode_insn(0x95, 0, 0, 0, 0, Endian::Little),
        ]
        .concat();
        let mut cpu = EbpfCpu::solana(version);
        cpu.regs.r[1] = 40;
        cpu.regs.r[6] = 6;
        cpu.register_function(1234, 24);
        cpu.register_syscall(1234, |registers, _| Ok(registers[1]));
        let mut memory = FlatMemory::new(64);
        memory.load(0, &program);
        assert_eq!(cpu.run(&mut memory), EbpfExit::Exit { r0: 42 });
        assert_eq!(cpu.regs.r[6], 6);
        for opt in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let mut lifter = EbpfLifter::solana(version);
            lifter.register_function(1234, 24);
            lifter.register_syscall(1234);
            let functions = lifter
                .lift_program(
                    0,
                    &CodeSlice(&program),
                    &mut LiftContext::new(lifter.source_arch()),
                )
                .unwrap();
            let mut interpreter = SmirInterpreter::new();
            for mut function in functions {
                optimize_function(&mut function, opt);
                interpreter.add_function(function);
            }
            let mut context = SmirContext::new_ebpf_with_config(EbpfConfig::solana(version));
            context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(1)), 40);
            context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(6)), 6);
            assert!(matches!(
                interpreter.run(&mut context, &mut memory),
                ExitReason::Syscall { num: 1234, .. }
            ));
            context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(0)), 40);
            assert!(
                matches!(
                    interpreter.run(&mut context, &mut memory),
                    ExitReason::Return { to: 42 }
                ),
                "{version:?} {opt:?}"
            );
            assert_eq!(context.read_arch_reg(ArchReg::Ebpf(EbpfReg::R(6))), 6);
            assert_eq!(context.insn_count, cpu.insn_count);
            assert_eq!(context.pc, cpu.regs.pc);
        }
    }
}

#[test]
fn static_sbpf_call_kinds_other_than_zero_and_one_are_rejected() {
    for version in [SbpfVersion::V3, SbpfVersion::V4] {
        let bytes = encode_insn(0x85, 0, 2, 0, 1234, Endian::Little);
        assert!(
            rax::isa::ebpf::decode_insn(&bytes, EbpfDialect::Solana(version), Endian::Little)
                .is_err()
        );
    }
}
