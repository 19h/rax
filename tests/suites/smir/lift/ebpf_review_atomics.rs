use super::*;
use rax::isa::ebpf::{
    EbpfMemFault, EbpfMemWidth, EbpfMemory, SbpfMemoryMapping, SbpfMemoryRegion, decode_insn,
};
use std::sync::{Arc, Barrier};
use vm_memory::{GuestAddress, GuestMemoryMmap};

#[test]
fn segmented_memory_implements_all_exposed_atomic_operations() {
    use rax::smir::ir::types::{AtomicOp, MemWidth, MemoryOrder};
    for width in [MemWidth::B4, MemWidth::B8] {
        let mask = if width == MemWidth::B4 {
            u32::MAX as u64
        } else {
            u64::MAX
        };
        let sign_bit = if width == MemWidth::B4 {
            0x8000_0000
        } else {
            0x8000_0000_0000_0000
        };
        for (op, old, operand, expected) in [
            (AtomicOp::Neg, 3, 0, mask - 2),
            (AtomicOp::Nand, mask, 1, mask - 1),
            (AtomicOp::Max, sign_bit, 1, 1),
            (AtomicOp::Min, sign_bit, 1, sign_bit),
            (AtomicOp::Umax, sign_bit, 1, sign_bit),
            (AtomicOp::Umin, sign_bit, 1, 1),
        ] {
            let mut memory = SbpfMemoryMapping::with_regions(vec![SbpfMemoryRegion::new_writable(
                0x100,
                vec![0; 8],
            )]);
            memory
                .atomic_store(0x100, old, width, MemoryOrder::SeqCst)
                .unwrap();
            assert_eq!(
                memory
                    .atomic_rmw(0x100, op, operand, width, MemoryOrder::SeqCst)
                    .unwrap(),
                old
            );
            assert_eq!(
                memory
                    .atomic_load(0x100, width, MemoryOrder::SeqCst)
                    .unwrap(),
                expected,
                "{op:?} {width:?}"
            );
        }
    }
}

#[test]
fn shared_smir_backend_keeps_big_endian_rmw_indivisible() {
    use rax::smir::ir::types::{AtomicOp, FenceKind, MemWidth, MemoryOrder};
    use std::sync::Mutex;
    #[derive(Clone)]
    struct Shared(Arc<Mutex<FlatMemory>>);
    impl SmirMemory for Shared {
        fn probe(&self, addr: u64, size: usize, write: bool) -> Result<(), MemoryError> {
            self.0.lock().unwrap().probe(addr, size, write)
        }
        fn read(&mut self, a: u64, b: &mut [u8]) -> Result<(), MemoryError> {
            self.0.lock().unwrap().read(a, b)
        }
        fn write(&mut self, a: u64, b: &[u8]) -> Result<(), MemoryError> {
            self.0.lock().unwrap().write(a, b)
        }
        fn atomic_load(&mut self, a: u64, w: MemWidth, o: MemoryOrder) -> Result<u64, MemoryError> {
            self.0.lock().unwrap().atomic_load(a, w, o)
        }
        fn atomic_store(
            &mut self,
            a: u64,
            v: u64,
            w: MemWidth,
            o: MemoryOrder,
        ) -> Result<(), MemoryError> {
            self.0.lock().unwrap().atomic_store(a, v, w, o)
        }
        fn atomic_rmw(
            &mut self,
            a: u64,
            op: AtomicOp,
            v: u64,
            w: MemWidth,
            o: MemoryOrder,
        ) -> Result<u64, MemoryError> {
            self.0.lock().unwrap().atomic_rmw(a, op, v, w, o)
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
            self.0.lock().unwrap().compare_and_swap(a, e, n, w, s, f)
        }
        fn load_exclusive(&mut self, a: u64, w: MemWidth) -> Result<u64, MemoryError> {
            self.0.lock().unwrap().load_exclusive(a, w)
        }
        fn store_exclusive(&mut self, a: u64, v: u64, w: MemWidth) -> Result<bool, MemoryError> {
            self.0.lock().unwrap().store_exclusive(a, v, w)
        }
        fn clear_exclusive(&mut self) {
            self.0.lock().unwrap().clear_exclusive();
        }
        fn fence(&mut self, _: FenceKind) {}
    }
    for opcode in [0xc3, 0xdb] {
        let shared = Shared(Arc::new(Mutex::new(FlatMemory::new(4096))));
        let barrier = Arc::new(Barrier::new(4));
        let program = [
            encode_insn(opcode, 1, 2, 0, 0, Endian::Big),
            encode_insn(0x95, 0, 0, 0, 0, Endian::Big),
        ]
        .concat();
        let mut lifter = EbpfLifter::new(EbpfDialect::Rfc9669, Endian::Big);
        let mut function = lifter
            .lift_function(
                0,
                &CodeSlice(&program),
                &mut LiftContext::new(lifter.source_arch()),
            )
            .unwrap();
        optimize_function(&mut function, OptLevel::O2);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let mut memory = shared.clone();
                let barrier = Arc::clone(&barrier);
                let function = function.clone();
                scope.spawn(move || {
                    let mut interpreter = SmirInterpreter::new();
                    interpreter.add_function(function);
                    let mut context = SmirContext::new_ebpf();
                    context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(1)), 0x100);
                    context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(2)), 1);
                    barrier.wait();
                    for _ in 0..1024 {
                        context.pc = 0;
                        assert!(matches!(
                            interpreter.run(&mut context, &mut memory),
                            ExitReason::Return { .. }
                        ));
                    }
                });
            }
        });
        let width = if opcode == 0xc3 {
            MemWidth::B4
        } else {
            MemWidth::B8
        };
        let raw = shared
            .clone()
            .atomic_load(0x100, width, MemoryOrder::SeqCst)
            .unwrap();
        let value = if opcode == 0xc3 {
            u64::from((raw as u32).swap_bytes())
        } else {
            raw.swap_bytes()
        };
        assert_eq!(value, 4096);
    }
}

#[test]
fn atomic_register_aliases_read_every_input_before_writing_the_fetch_result() {
    for endian in [Endian::Little, Endian::Big] {
        for (base, source, imm) in [(1, 1, 1), (0, 2, 0xf1), (1, 0, 0xf1), (1, 0, 0xe1)] {
            let mut regs = [0; 11];
            regs[base] = SCRATCH_BASE;
            regs[source] = if base == source { SCRATCH_BASE } else { 7 };
            let initial = match endian {
                Endian::Little => 7u64.to_le_bytes(),
                Endian::Big => 7u64.to_be_bytes(),
            };
            let program = [
                encode_insn(0xdb, base as u8, source as u8, 0, imm, endian),
                encode_insn(0x95, 0, 0, 0, 0, endian),
            ]
            .concat();
            assert_direct_smir_parity(
                "atomic_alias",
                &program,
                EbpfDialect::Rfc9669,
                endian,
                &regs,
                &initial,
            );
        }
    }
}

#[test]
fn atomic_operations_preserve_guest_byte_order_values_and_fetch_results() {
    for endian in [Endian::Little, Endian::Big] {
        for (opcode, bytes) in [(0xc3, 4usize), (0xdb, 8usize)] {
            let mask = if bytes == 4 {
                u32::MAX as u64
            } else {
                u64::MAX
            };
            for immediate in [0, 1, 0x40, 0x41, 0x50, 0x51, 0xa0, 0xa1, 0xe1, 0xf1] {
                for old in [0, mask, 0x0100_0000, 0x0123_4567_89ab_cdef & mask] {
                    for operand in [1, 0x0102_0304_5566_7788 & mask] {
                        for matches in [true, false] {
                            let expected = if matches { old } else { old ^ 1 };
                            let mut regs = [0u64; 11];
                            regs[0] = expected;
                            regs[1] = SCRATCH_BASE;
                            regs[2] = operand;
                            let encoded = match endian {
                                Endian::Little => old.to_le_bytes(),
                                Endian::Big => old.to_be_bytes(),
                            };
                            let initial = if endian == Endian::Big {
                                encoded[8 - bytes..].to_vec()
                            } else {
                                encoded[..bytes].to_vec()
                            };
                            let program = [
                                encode_insn(opcode, 1, 2, 0, immediate, endian),
                                encode_insn(0x95, 0, 0, 0, 0, endian),
                            ]
                            .concat();
                            let replacement = match immediate {
                                0 | 1 => old.wrapping_add(operand) & mask,
                                0x40 | 0x41 => old | operand,
                                0x50 | 0x51 => old & operand,
                                0xa0 | 0xa1 => old ^ operand,
                                0xe1 => operand,
                                0xf1 => {
                                    if matches {
                                        operand
                                    } else {
                                        old
                                    }
                                }
                                _ => unreachable!(),
                            };
                            let expected_bytes = match endian {
                                Endian::Little => replacement.to_le_bytes(),
                                Endian::Big => replacement.to_be_bytes(),
                            };
                            let expected_bytes = if endian == Endian::Big {
                                &expected_bytes[8 - bytes..]
                            } else {
                                &expected_bytes[..bytes]
                            };
                            let (direct, memory) =
                                run_direct(&program, EbpfDialect::Rfc9669, endian, &regs, &initial);
                            assert_eq!(
                                memory, expected_bytes,
                                "{endian:?} width={bytes} imm={immediate:#x}"
                            );
                            let NormalizedOutcome::Exited { regs: result, .. } = direct else {
                                panic!("atomic trapped");
                            };
                            assert_eq!(
                                result[2],
                                if immediate & 1 != 0 && immediate != 0xf1 {
                                    old
                                } else {
                                    operand
                                }
                            );
                            assert_eq!(result[0], if immediate == 0xf1 { old } else { expected });
                            assert_direct_smir_parity(
                                "atomic_matrix",
                                &program,
                                EbpfDialect::Rfc9669,
                                endian,
                                &regs,
                                &initial,
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn shared_guest_memory_atomic_increments_have_no_lost_updates() {
    const THREADS: usize = 4;
    const ITERATIONS: usize = 4096;
    for endian in [Endian::Little, Endian::Big] {
        for (opcode, width) in [(0xc3, EbpfMemWidth::B4), (0xdb, EbpfMemWidth::B8)] {
            let memory: Arc<GuestMemoryMmap> =
                Arc::new(GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 4096)]).unwrap());
            let start = Arc::new(Barrier::new(THREADS));
            let instruction = decode_insn(
                &encode_insn(opcode, 1, 2, 0, 0, endian),
                EbpfDialect::Rfc9669,
                endian,
            )
            .unwrap();
            std::thread::scope(|scope| {
                for _ in 0..THREADS {
                    let memory = Arc::clone(&memory);
                    let start = Arc::clone(&start);
                    scope.spawn(move || {
                        let mut cpu = EbpfCpu::new(EbpfConfig {
                            endian,
                            ..EbpfConfig::default()
                        });
                        cpu.regs.r[1] = 0x100;
                        cpu.regs.r[2] = 1;
                        let mut backing = memory.as_ref();
                        start.wait();
                        for _ in 0..ITERATIONS {
                            cpu.execute_decoded(&instruction, &mut backing).unwrap();
                        }
                    });
                }
            });
            assert_eq!(
                EbpfMemory::load(&mut memory.as_ref(), 0x100, width, false, endian).unwrap(),
                (THREADS * ITERATIONS) as u64,
                "{endian:?} {width:?}"
            );
        }
    }
}

#[test]
fn byte_only_memory_rejects_atomics_without_a_transaction_implementation() {
    struct BytesOnly(FlatMemory);
    impl EbpfMemory for BytesOnly {
        fn read_bytes(&mut self, addr: u64, out: &mut [u8]) -> Result<(), EbpfMemFault> {
            EbpfMemory::read_bytes(&mut self.0, addr, out)
        }
        fn write_bytes(&mut self, addr: u64, data: &[u8]) -> Result<(), EbpfMemFault> {
            EbpfMemory::write_bytes(&mut self.0, addr, data)
        }
    }
    let mut memory = BytesOnly(FlatMemory::new(64));
    let mut cpu = EbpfCpu::rfc9669();
    cpu.regs.r[1] = 32;
    cpu.regs.r[2] = 1;
    let instruction = decode_insn(
        &encode_insn(0xdb, 1, 2, 0, 1, Endian::Little),
        EbpfDialect::Rfc9669,
        Endian::Little,
    )
    .unwrap();
    assert!(cpu.execute_decoded(&instruction, &mut memory).is_err());
    assert_eq!(cpu.regs.pc, 0);
    assert_eq!(cpu.regs.r[2], 1);
    assert_eq!(
        EbpfMemory::load(&mut memory, 32, EbpfMemWidth::B8, false, Endian::Little).unwrap(),
        0
    );
}

#[test]
fn failed_compare_exchange_still_checks_write_permissions_before_register_updates() {
    let mut memory =
        SbpfMemoryMapping::with_regions(vec![SbpfMemoryRegion::new_readonly(0x100, vec![1; 8])]);
    let mut cpu = EbpfCpu::rfc9669();
    cpu.regs.r[0] = 0;
    cpu.regs.r[1] = 0x100;
    cpu.regs.r[2] = 2;
    let instruction = decode_insn(
        &encode_insn(0xdb, 1, 2, 0, 0xf1, Endian::Little),
        EbpfDialect::Rfc9669,
        Endian::Little,
    )
    .unwrap();
    assert!(matches!(
        cpu.execute_decoded(&instruction, &mut memory),
        Err(EbpfTrap::MemoryFault {
            fault: EbpfMemFault { write: true, .. },
            ..
        })
    ));
    assert_eq!(cpu.regs.r[0], 0);
    assert_eq!(cpu.regs.pc, 0);
}
