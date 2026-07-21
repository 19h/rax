//! Strict lifting and optimization coverage for legacy MOVDIRI.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ExitReason, SmirContext};
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::x86_64::X86_64Lowerer;

fn store(result: &LiftResult) -> (&VReg, &Address, MemWidth) {
    result
        .ops
        .iter()
        .find_map(|op| match &op.kind {
            OpKind::Store { src, addr, width } => Some((src, addr, *width)),
            _ => None,
        })
        .expect("MOVDIRI must emit one precise Store")
}

#[test]
fn legacy_movdiri_lifts_exact_width_register_and_address_forms() {
    let base = lift_single(&[0x0F, 0x38, 0xF9, 0x08]).unwrap();
    assert_eq!(base.bytes_consumed, 4);
    assert!(matches!(base.control_flow, ControlFlow::Fallthrough));
    assert!(matches!(
        store(&base),
        (
            VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
            Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
            MemWidth::B4,
        )
    ));
    assert!(base.ops[0].kind.has_side_effects());
    assert!(base.ops[0].kind.writes_memory());
    assert!(!base.ops[0].kind.reads_memory());

    let high = lift_single(&[0x4D, 0x0F, 0x38, 0xF9, 0x48, 0x08]).unwrap();
    assert_eq!(high.bytes_consumed, 6);
    assert!(matches!(
        store(&high),
        (
            VReg::Arch(ArchReg::X86(X86Reg::R9)),
            Address::BaseOffset {
                base: VReg::Arch(ArchReg::X86(X86Reg::R8)),
                offset: 8,
                ..
            },
            MemWidth::B8,
        )
    ));

    let sib = lift_single(&[0x4F, 0x0F, 0x38, 0xF9, 0xBC, 0xAC, 0x34, 0x12, 0x00, 0x00]).unwrap();
    assert_eq!(sib.bytes_consumed, 10);
    assert!(matches!(
        store(&sib),
        (
            VReg::Arch(ArchReg::X86(X86Reg::R15)),
            Address::BaseIndexScale {
                base: Some(VReg::Arch(ArchReg::X86(X86Reg::R12))),
                index: VReg::Arch(ArchReg::X86(X86Reg::R13)),
                scale: 4,
                disp: 0x1234,
                ..
            },
            MemWidth::B8,
        )
    ));

    let rip_relative =
        lift_single(&[0x48, 0x0F, 0x38, 0xF9, 0x0D, 0x20, 0x00, 0x00, 0x00]).unwrap();
    assert_eq!(rip_relative.bytes_consumed, 9);
    assert!(matches!(
        store(&rip_relative),
        (
            VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
            Address::PcRel {
                offset: 0x20,
                base: Some(0x1009),
                ..
            },
            MemWidth::B8,
        )
    ));
}

#[test]
fn legacy_movdiri_preserves_addr32_segments_and_ignorable_rep_prefixes() {
    let addr32 = lift_single(&[0x64, 0x67, 0x45, 0x0F, 0x38, 0xF9, 0x4C, 0x88, 0x20]).unwrap();
    assert_eq!(addr32.bytes_consumed, 9);
    assert!(matches!(
        store(&addr32),
        (
            VReg::Arch(ArchReg::X86(X86Reg::R9)),
            Address::X86Addr32(inner),
            MemWidth::B4,
        ) if matches!(
            inner.as_ref(),
            Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                base: Some(VReg::Arch(ArchReg::X86(X86Reg::R8))),
                index: Some(VReg::Arch(ArchReg::X86(X86Reg::Rcx))),
                scale: 4,
                disp: 0x20,
            }
        )
    ));

    for bytes in [
        &[0xF2, 0x0F, 0x38, 0xF9, 0x08][..],
        &[0xF3, 0x48, 0x0F, 0x38, 0xF9, 0x08][..],
        // A REX prefix preceding F2 is ignored; the data width is therefore 32 bits.
        &[0x48, 0xF2, 0x0F, 0x38, 0xF9, 0x08][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    }
    assert_eq!(
        store(&lift_single(&[0xF3, 0x48, 0x0F, 0x38, 0xF9, 0x08]).unwrap()).2,
        MemWidth::B8,
    );
    assert_eq!(
        store(&lift_single(&[0x48, 0xF2, 0x0F, 0x38, 0xF9, 0x08]).unwrap()).2,
        MemWidth::B4,
    );
}

#[test]
fn legacy_movdiri_rejects_invalid_and_incomplete_encodings() {
    for bytes in [
        &[0x66, 0x0F, 0x38, 0xF9, 0x08][..],
        &[0xF0, 0x0F, 0x38, 0xF9, 0x08][..],
        &[0x0F, 0x38, 0xF9, 0xC8][..],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "invalid MOVDIRI encoding accepted: {bytes:02X?}",
        );
    }

    let reserved_escape = lift_single(&[0xD5, 0x00, 0x0F, 0x38, 0xF9, 0x08])
        .expect("REX2 followed by 0F is an explicit #UD");
    assert_invalid_opcode_trap(&reserved_escape, 3);

    for bytes in [
        &[0x0F, 0x38, 0xF9][..],
        &[0x0F, 0x38, 0xF9, 0x48][..],
        &[0x0F, 0x38, 0xF9, 0x8C, 0x88, 0x00, 0x00][..],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::Incomplete { .. })),
            "truncated MOVDIRI encoding did not report Incomplete: {bytes:02X?}",
        );
    }
}

#[test]
fn legacy_movdiri_remains_in_a_strict_o2_loop_without_a_frontier() {
    let memory = TestMemory::new(
        0x2000,
        vec![
            0x4D, 0x0F, 0x38, 0xF9, 0x48, 0x08, // MOVDIRI [r8+8],r9
            0xFF, 0xC9, // DEC ecx
            0x75, 0xF6, // JNZ 0x2000
            0xF4, // HLT
        ],
    );
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    let mut context = LiftContext::new(SourceArch::X86_64);
    let mut function = lifter.lift_function(0x2000, &memory, &mut context).unwrap();
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);

    let entry = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x2000)
        .unwrap();
    assert_eq!(
        entry
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::Store { .. }))
            .count(),
        1,
        "O2 must retain the faulting, side-effecting MOVDIRI store",
    );
    assert!(entry.ops.iter().any(|op| {
        op.guest_pc == 0x2000
            && matches!(
                op.kind,
                OpKind::Store {
                    src: VReg::Arch(ArchReg::X86(X86Reg::R9)),
                    width: MemWidth::B8,
                    ..
                }
            )
    }));
    assert!(!matches!(entry.terminator, Terminator::Return { .. }));
}

#[test]
fn legacy_movdiri_executes_exact_store_through_smir_interpreter() {
    for (bytes, source, expected) in [
        (
            &[0x45, 0x0F, 0x38, 0xF9, 0x48, 0x08][..],
            0xA1B2_C3D4_5566_7788,
            &[0x88, 0x77, 0x66, 0x55][..],
        ),
        (
            &[0x4D, 0x0F, 0x38, 0xF9, 0x48, 0x08][..],
            0x0123_4567_89AB_CDEF,
            &[0xEF, 0xCD, 0xAB, 0x89, 0x67, 0x45, 0x23, 0x01][..],
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.ops = result.ops;
        block.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });

        let mut context = SmirContext::new_x86_64();
        context.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::R8)), 0x4000);
        context.write_vreg(VReg::Arch(ArchReg::X86(X86Reg::R9)), source);
        let mut memory = FlatMemory::with_base(0x4000, 0x20);

        assert!(matches!(
            SmirInterpreter::new().execute_block(&mut context, &mut memory, &block),
            BlockResult::Exit(ExitReason::Halt)
        ));
        let mut observed = vec![0; expected.len()];
        memory.read(0x4008, &mut observed).unwrap();
        assert_eq!(observed, expected, "MOVDIRI {bytes:02X?}");
        assert_eq!(
            context.read_vreg(VReg::Arch(ArchReg::X86(X86Reg::R9))),
            source,
            "MOVDIRI must preserve its source register",
        );
    }
}

#[cfg(feature = "smir-jit")]
#[test]
fn legacy_movdiri_is_helper_backed_x86_jit_admissible_and_lowerable() {
    let result = lift_single(&[0x4D, 0x0F, 0x38, 0xF9, 0x48, 0x08]).unwrap();
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: vec![] });
    let mut function = SmirFunction::new(FunctionId(0), block.id, 0x1000);
    function.add_block(block);
    let excluded = HashMap::new();

    assert!(
        !crate::smir::lower::runtime::is_native_clobber_safe_excluding(&function, &excluded, false,),
        "MOVDIRI must remain fail-closed when memory helpers are disabled",
    );
    assert!(
        crate::smir::lower::runtime::is_native_clobber_safe_excluding(&function, &excluded, true,),
        "the exact Store shape must pass the helper-backed x86 JIT gate",
    );

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower helper-backed MOVDIRI Store");
    assert!(lowered.relocations.is_empty());
    assert!(
        !lowerer
            .finalize()
            .expect("finalize MOVDIRI JIT code")
            .is_empty()
    );
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn legacy_movdiri_executes_through_helper_backed_x86_jit() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    #[derive(Default)]
    struct StoreObservation {
        calls: usize,
        addr: u64,
        value: u64,
        size: u64,
        succeed: u64,
        committed: [u8; 8],
    }

    extern "C" fn store(context: *mut StoreObservation, addr: u64, value: u64, size: u64) -> u64 {
        // SAFETY: each native invocation receives the live, uniquely borrowed
        // observation pointer installed in `GuestRegs::ctx` immediately below.
        let observation = unsafe { &mut *context };
        observation.calls += 1;
        observation.addr = addr;
        observation.value = value;
        observation.size = size;
        if observation.succeed != 0 {
            let bytes = value.to_le_bytes();
            let committed_len = (size as usize).min(bytes.len());
            observation.committed[..committed_len].copy_from_slice(&bytes[..committed_len]);
        }
        observation.succeed
    }

    const STATUS_FLAGS: u64 = 0x8D5;
    const INITIAL_FLAGS: u64 = 0xCD7;
    const SENTINEL_PC: u64 = 0xAAAA_BBBB_CCCC_DDDD;
    const SOURCE: u64 = 0xA1B2_C3D4_5566_7788;

    for (bytes, expected_bytes, expected_size) in [
        (
            &[0x45, 0x0F, 0x38, 0xF9, 0x48, 0x08][..],
            &[0x88, 0x77, 0x66, 0x55][..],
            4,
        ),
        (
            &[0x4D, 0x0F, 0x38, 0xF9, 0x48, 0x08][..],
            &[0x88, 0x77, 0x66, 0x55, 0xD4, 0xC3, 0xB2, 0xA1][..],
            8,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.ops = result.ops;
        block.set_terminator(Terminator::Return { values: vec![] });
        let mut function = SmirFunction::new(FunctionId(0), block.id, 0x1000);
        function.add_block(block);

        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        let lowered = lowerer
            .lower_function(&function)
            .expect("lower helper-backed MOVDIRI");
        let executable = ExecMem::new(&lowerer.finalize().expect("finalize MOVDIRI"))
            .expect("map native MOVDIRI");

        for (succeed, expected_exit_pc) in [(1, SENTINEL_PC), (0, 0x1000)] {
            let mut observation = StoreObservation {
                succeed,
                committed: [0xA5; 8],
                ..StoreObservation::default()
            };
            let mut registers = GuestRegs::default();
            registers.gpr[8] = 0x4000;
            registers.gpr[9] = SOURCE;
            registers.rflags = INITIAL_FLAGS;
            registers.exit_pc = SENTINEL_PC;
            registers.ctx = (&mut observation as *mut StoreObservation) as u64;
            registers.store_fn = store as usize as u64;
            let initial_gprs = registers.gpr;

            executable.run(lowered.entry_offset, &mut registers);

            assert_eq!(observation.calls, 1, "MOVDIRI {bytes:02X?}");
            assert_eq!(observation.addr, 0x4008, "MOVDIRI {bytes:02X?}");
            assert_eq!(observation.value, SOURCE, "MOVDIRI {bytes:02X?}");
            assert_eq!(observation.size, expected_size, "MOVDIRI {bytes:02X?}");
            let mut expected_committed = [0xA5; 8];
            if succeed != 0 {
                expected_committed[..expected_size as usize].copy_from_slice(expected_bytes);
            }
            assert_eq!(observation.committed, expected_committed);
            assert_eq!(registers.gpr, initial_gprs, "MOVDIRI must preserve GPRs");
            assert_eq!(
                registers.rflags & STATUS_FLAGS,
                INITIAL_FLAGS & STATUS_FLAGS
            );
            assert_eq!(registers.exit_pc, expected_exit_pc);
        }
    }
}
