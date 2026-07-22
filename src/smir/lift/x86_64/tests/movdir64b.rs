//! Strict lifting, interpretation, and optimization coverage for legacy MOVDIR64B.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::memory::{FlatMemory, SmirMemory};

fn transfer(result: &LiftResult) -> (&Address, VReg, &Address) {
    let alignment = result
        .ops
        .iter()
        .find_map(|op| match &op.kind {
            OpKind::X86CheckAlignment {
                addr,
                alignment: 64,
            } => Some(addr),
            _ => None,
        })
        .expect("MOVDIR64B must check 64-byte destination alignment");
    let (value, source) = result
        .ops
        .iter()
        .find_map(|op| match &op.kind {
            OpKind::VLoad {
                dst,
                addr,
                width: VecWidth::V512,
            } => Some((*dst, addr)),
            _ => None,
        })
        .expect("MOVDIR64B must read one 64-byte source transaction");
    let (stored, destination) = result
        .ops
        .iter()
        .find_map(|op| match &op.kind {
            OpKind::VStore {
                src,
                addr,
                width: VecWidth::V512,
            } => Some((*src, addr)),
            _ => None,
        })
        .expect("MOVDIR64B must write one 64-byte destination transaction");
    assert_eq!(stored, value, "the buffered source must feed the store");
    assert_eq!(alignment, destination);
    (source, value, destination)
}

fn block_for(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).unwrap();
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

#[test]
fn legacy_movdir64b_lifts_exact_register_source_and_destination_forms() {
    let base = lift_single(&[0x66, 0x0F, 0x38, 0xF8, 0x08]).unwrap();
    assert_eq!(base.bytes_consumed, 5);
    assert!(matches!(base.control_flow, ControlFlow::Fallthrough));
    let (source, value, destination) = transfer(&base);
    assert!(matches!(value, VReg::Virtual(_)));
    assert!(matches!(
        source,
        Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rax)))
    ));
    assert!(matches!(
        destination,
        Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rcx)))
    ));
    assert_eq!(base.ops.len(), 3);
    assert!(base.ops[0].kind.has_side_effects());
    assert!(base.ops[1].kind.reads_memory());
    assert!(!base.ops[1].kind.writes_memory());
    assert!(base.ops[2].kind.writes_memory());

    let high = lift_single(&[
        0x66, 0x47, 0x0F, 0x38, 0xF8, 0x8C, 0xA8, 0x34, 0x12, 0x00, 0x00,
    ])
    .unwrap();
    assert_eq!(high.bytes_consumed, 11);
    let (source, _, destination) = transfer(&high);
    assert!(matches!(
        source,
        Address::BaseIndexScale {
            base: Some(VReg::Arch(ArchReg::X86(X86Reg::R8))),
            index: VReg::Arch(ArchReg::X86(X86Reg::R13)),
            scale: 4,
            disp: 0x1234,
            ..
        }
    ));
    assert!(matches!(
        destination,
        Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::R9)))
    ));

    let rip_relative =
        lift_single(&[0x66, 0x0F, 0x38, 0xF8, 0x0D, 0x20, 0x00, 0x00, 0x00]).unwrap();
    let (source, _, _) = transfer(&rip_relative);
    assert!(matches!(
        source,
        Address::PcRel {
            offset: 0x20,
            base: Some(0x1009),
            ..
        }
    ));
}

#[test]
fn legacy_movdir64b_applies_addr32_and_segment_only_to_the_correct_operands() {
    let result =
        lift_single(&[0x64, 0x67, 0x66, 0x44, 0x0F, 0x38, 0xF8, 0x4C, 0x88, 0x20]).unwrap();
    assert_eq!(result.bytes_consumed, 10);
    let (source, _, destination) = transfer(&result);
    assert!(matches!(
        source,
        Address::X86Addr32(inner) if matches!(
            inner.as_ref(),
            Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                base: Some(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
                index: Some(VReg::Arch(ArchReg::X86(X86Reg::Rcx))),
                scale: 4,
                disp: 0x20,
            }
        )
    ));
    assert!(matches!(
        destination,
        Address::X86Addr32(inner) if matches!(
            inner.as_ref(),
            Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::R9)))
        )
    ));

    let extended = lift_single(&[0x66, 0x44, 0x0F, 0x38, 0xF8, 0x08]).unwrap();
    assert!(matches!(
        transfer(&extended).2,
        Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::R9)))
    ));
    let invalidated_rex = lift_single(&[0x44, 0x66, 0x0F, 0x38, 0xF8, 0x08]).unwrap();
    assert!(matches!(
        transfer(&invalidated_rex).2,
        Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rcx)))
    ));
}

#[test]
fn legacy_movdir64b_rejects_invalid_and_incomplete_encodings() {
    for bytes in [
        &[0x0F, 0x38, 0xF8, 0x08][..],
        &[0xF0, 0x66, 0x0F, 0x38, 0xF8, 0x08][..],
        &[0xF2, 0x66, 0x0F, 0x38, 0xF8, 0x08][..],
        &[0xF3, 0x66, 0x0F, 0x38, 0xF8, 0x08][..],
        &[0x66, 0x0F, 0x38, 0xF8, 0xC8][..],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "invalid MOVDIR64B encoding accepted: {bytes:02X?}",
        );
    }

    let reserved_escape = lift_single(&[0x66, 0xD5, 0x00, 0x0F, 0x38, 0xF8, 0x08])
        .expect("REX2 followed by 0F is an explicit #UD");
    assert_invalid_opcode_trap(&reserved_escape, 4);

    for bytes in [
        &[0x66, 0x0F, 0x38, 0xF8][..],
        &[0x66, 0x0F, 0x38, 0xF8, 0x48][..],
        &[0x66, 0x0F, 0x38, 0xF8, 0x8C, 0x88, 0x00, 0x00][..],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::Incomplete { .. })),
            "truncated MOVDIR64B encoding did not report Incomplete: {bytes:02X?}",
        );
    }
}

#[test]
fn legacy_movdir64b_remains_ordered_in_a_strict_o2_loop() {
    let memory = TestMemory::new(
        0x2000,
        vec![
            0x66, 0x44, 0x0F, 0x38, 0xF8, 0x08, // MOVDIR64B r9,[rax]
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
    let alignment = entry
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 64, .. }))
        .unwrap();
    let load = entry
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V512,
                    ..
                }
            )
        })
        .unwrap();
    let store = entry
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VStore {
                    width: VecWidth::V512,
                    ..
                }
            )
        })
        .unwrap();
    assert!(alignment < load && load < store);
    assert!(!matches!(entry.terminator, Terminator::Return { .. }));
}

#[test]
fn legacy_movdir64b_interpreter_buffers_overlap_and_truncates_addr32_offsets() {
    let mut memory = FlatMemory::with_base(0x2000, 0x2000);
    let original: Vec<u8> = (0..96).map(|byte| byte ^ 0xA5).collect();
    memory.write(0x2000, &original).unwrap();

    let mut context = SmirContext::new_x86_64();
    context.write_vreg(x86_gpr(0), 0x2020);
    context.write_vreg(x86_gpr(1), 0x2000);
    let initial_rax = context.read_vreg(x86_gpr(0));
    let initial_rcx = context.read_vreg(x86_gpr(1));
    assert!(matches!(
        SmirInterpreter::new().execute_block(
            &mut context,
            &mut memory,
            &block_for(&[0x66, 0x0F, 0x38, 0xF8, 0x08]),
        ),
        BlockResult::Exit(ExitReason::Halt)
    ));
    let mut copied = [0u8; 64];
    memory.read(0x2000, &mut copied).unwrap();
    assert_eq!(copied.as_slice(), &original[32..96]);
    assert_eq!(context.read_vreg(x86_gpr(0)), initial_rax);
    assert_eq!(context.read_vreg(x86_gpr(1)), initial_rcx);

    let source: Vec<u8> = (0u8..64).map(|byte| byte.wrapping_mul(3)).collect();
    memory.write(0x2000, &source).unwrap();
    let mut addr32 = SmirContext::new_x86_64();
    addr32.write_vreg(x86_gpr(0), 0xFFFF_0000_0000_2000);
    addr32.write_vreg(x86_gpr(1), 0xFFFF_0000_0000_3040);
    assert!(matches!(
        SmirInterpreter::new().execute_block(
            &mut addr32,
            &mut memory,
            &block_for(&[0x67, 0x66, 0x0F, 0x38, 0xF8, 0x08]),
        ),
        BlockResult::Exit(ExitReason::Halt)
    ));
    let mut observed = [0u8; 64];
    memory.read(0x3040, &mut observed).unwrap();
    assert_eq!(observed.as_slice(), source.as_slice());
}

#[test]
fn legacy_movdir64b_interpreter_preserves_fault_priority_and_direction() {
    let block = block_for(&[0x66, 0x0F, 0x38, 0xF8, 0x08]);

    let mut misaligned = SmirContext::new_x86_64();
    misaligned.write_vreg(x86_gpr(0), 0xDEAD_0000);
    misaligned.write_vreg(x86_gpr(1), 0x3001);
    let mut memory = FlatMemory::with_base(0x3000, 0x100);
    assert!(matches!(
        SmirInterpreter::new().execute_block(&mut misaligned, &mut memory, &block),
        BlockResult::Exit(ExitReason::GeneralProtection {
            addr: 0x1000,
            error_code: 0,
        })
    ));

    let mut source_fault = SmirContext::new_x86_64();
    source_fault.write_vreg(x86_gpr(0), 0xDEAD_0000);
    source_fault.write_vreg(x86_gpr(1), 0x3000);
    assert!(matches!(
        SmirInterpreter::new().execute_block(&mut source_fault, &mut memory, &block),
        BlockResult::Exit(ExitReason::MemoryFault {
            addr: 0xDEAD_0000,
            write: false,
        })
    ));

    let mut source_memory = FlatMemory::with_base(0x2000, 64);
    source_memory.write(0x2000, &[0x5A; 64]).unwrap();
    let mut destination_fault = SmirContext::new_x86_64();
    destination_fault.write_vreg(x86_gpr(0), 0x2000);
    destination_fault.write_vreg(x86_gpr(1), 0x3000);
    assert!(matches!(
        SmirInterpreter::new().execute_block(&mut destination_fault, &mut source_memory, &block,),
        BlockResult::Exit(ExitReason::MemoryFault {
            addr: 0x3000,
            write: true,
        })
    ));
}

#[cfg(feature = "smir-jit")]
#[test]
fn legacy_movdir64b_native_gate_remains_fail_closed_for_the_buffered_temporary() {
    let mut block = block_for(&[0x66, 0x0F, 0x38, 0xF8, 0x08]);
    block.set_terminator(Terminator::Return { values: vec![] });
    let mut function = SmirFunction::new(FunctionId(0), block.id, 0x1000);
    function.add_block(block);
    let excluded = HashMap::new();

    assert!(
        !crate::smir::lower::runtime::is_native_clobber_safe_excluding(&function, &excluded, false,)
    );
    assert!(
        !crate::smir::lower::runtime::is_native_clobber_safe_excluding(&function, &excluded, true,)
    );
}

#[test]
fn apx_movdir64b_strictly_lifts_egpr_and_complex_address_forms() {
    // LLVM 23: `movdir64b r16, [r17]`.
    let base = [0x62, 0xEC, 0x7D, 0x08, 0xF8, 0x01];
    let result = lift_single(&base).expect("APX MOVDIR64B r16,[r17]");
    assert_eq!(result.bytes_consumed, base.len());
    assert!(matches!(result.ops[0].kind, OpKind::X86RequireApx));
    let (source, _, destination) = transfer(&result);
    assert!(matches!(
        source,
        Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::R17)))
    ));
    assert!(matches!(
        destination,
        Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::R16)))
    ));

    // LLVM 23: `movdir64b r9, [r20 + 4*r21 + 64]`.
    let sib = [0x62, 0x7C, 0x79, 0x08, 0xF8, 0x4C, 0xAC, 0x40];
    let result = lift_single(&sib).expect("APX MOVDIR64B EGPR SIB");
    assert_eq!(result.bytes_consumed, sib.len());
    assert!(matches!(result.ops[0].kind, OpKind::X86RequireApx));
    let (source, _, destination) = transfer(&result);
    assert!(matches!(
        source,
        Address::BaseIndexScale {
            base: Some(VReg::Arch(ArchReg::X86(X86Reg::R20))),
            index: VReg::Arch(ArchReg::X86(X86Reg::R21)),
            scale: 4,
            disp: 0x40,
            disp_size: DispSize::Disp8,
        }
    ));
    assert!(matches!(
        destination,
        Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::R9)))
    ));
    for (index, op) in result.ops.iter().enumerate() {
        assert_eq!(op.id, OpId(index as u16));
    }

    let fs_addr32 = [0x64, 0x67, 0x62, 0x7C, 0x79, 0x08, 0xF8, 0x4C, 0xAC, 0x40];
    let result = lift_single(&fs_addr32).expect("APX MOVDIR64B FS addr32");
    assert_eq!(result.bytes_consumed, fs_addr32.len());
    assert!(matches!(result.ops[0].kind, OpKind::X86RequireApx));
    let (source, _, destination) = transfer(&result);
    assert!(matches!(
        source,
        Address::X86Addr32(inner) if matches!(
            inner.as_ref(),
            Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                base: Some(VReg::Arch(ArchReg::X86(X86Reg::R20))),
                index: Some(VReg::Arch(ArchReg::X86(X86Reg::R21))),
                scale: 4,
                disp: 0x40,
            }
        )
    ));
    assert!(matches!(
        destination,
        Address::X86Addr32(inner) if matches!(
            inner.as_ref(),
            Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::R9)))
        )
    ));
}

#[test]
fn apx_movdir64b_rejects_reserved_fields_and_reports_absolute_incomplete_lengths() {
    for (bytes, name) in [
        (&[0x62, 0xEC, 0xFD, 0x08, 0xF8, 0x01][..], "W=1"),
        (&[0x62, 0xEC, 0x7D, 0x18, 0xF8, 0x01][..], "ND"),
        (&[0x62, 0xEC, 0x7D, 0x0C, 0xF8, 0x01][..], "NF"),
        (&[0x62, 0xEC, 0x7D, 0x88, 0xF8, 0x01][..], "z"),
        (&[0x62, 0xEC, 0x7D, 0x28, 0xF8, 0x01][..], "LL"),
        (&[0x62, 0xEC, 0x7D, 0x09, 0xF8, 0x01][..], "aaa"),
        (&[0x62, 0xEC, 0x75, 0x08, 0xF8, 0x01][..], "V3:0"),
        (&[0x62, 0xEC, 0x7D, 0x00, 0xF8, 0x01][..], "V4"),
        (&[0x62, 0xEC, 0x7D, 0x08, 0xF8, 0xC1][..], "mod=3"),
        (
            &[0x66, 0x62, 0xEC, 0x7D, 0x08, 0xF8, 0x01][..],
            "leading 66",
        ),
    ] {
        let error = lift_single(bytes).expect_err(name);
        assert!(
            matches!(error, LiftError::InvalidEncoding { .. }),
            "{name}: {error:?}"
        );
    }

    assert!(matches!(
        lift_single(&[0x62, 0xEC, 0x7D, 0x08, 0xF8]),
        Err(LiftError::Incomplete {
            have: 5,
            need: 6,
            ..
        })
    ));
    assert!(matches!(
        lift_single(&[0x62, 0xEC, 0x7D, 0x08, 0xF8, 0x84]),
        Err(LiftError::Incomplete {
            have: 6,
            need: 7,
            ..
        })
    ));
    assert!(matches!(
        lift_single(&[0x62, 0xEC, 0x7D, 0x08, 0xF8, 0x84, 0xAC]),
        Err(LiftError::Incomplete {
            have: 7,
            need: 11,
            ..
        })
    ));
}

#[test]
fn apx_movdir64b_dynamic_guard_precedes_alignment_and_memory_faults() {
    let block = block_for(&[0x62, 0xEC, 0x7D, 0x08, 0xF8, 0x01]);
    assert!(matches!(block.ops[0].kind, OpKind::X86RequireApx));

    for enabled in [false, true] {
        let mut context = SmirContext::new_x86_64();
        context.write_vreg(x86_gpr(16), 0x3001);
        context.write_vreg(x86_gpr(17), 0xDEAD_0000);
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.apx_enabled = enabled;
        let execution = SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::with_base(0x3000, 0x100),
            &block,
        );
        if enabled {
            assert!(matches!(
                execution,
                BlockResult::Exit(ExitReason::GeneralProtection {
                    addr: 0x1000,
                    error_code: 0,
                })
            ));
        } else {
            assert!(matches!(
                execution,
                BlockResult::Exit(ExitReason::Undefined {
                    addr: 0x1000,
                    opcode: 0,
                })
            ));
        }
    }

    let source: Vec<u8> = (0u8..64).map(|byte| byte ^ 0x5A).collect();
    let mut memory = FlatMemory::with_base(0x2000, 0x1100);
    memory.write(0x2000, &source).unwrap();
    let mut context = SmirContext::new_x86_64();
    context.write_vreg(x86_gpr(16), 0x3000);
    context.write_vreg(x86_gpr(17), 0x2000);
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.apx_enabled = true;
    assert!(matches!(
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &block),
        BlockResult::Exit(ExitReason::Halt)
    ));
    let mut observed = [0u8; 64];
    memory.read(0x3000, &mut observed).unwrap();
    assert_eq!(observed.as_slice(), source.as_slice());
}

#[cfg(feature = "smir-jit")]
#[test]
fn apx_movdir64b_native_gate_remains_fail_closed_for_buffered_512_bit_transfer() {
    let mut block = block_for(&[0x62, 0xEC, 0x7D, 0x08, 0xF8, 0x01]);
    block.set_terminator(Terminator::Return { values: vec![] });
    let mut function = SmirFunction::new(FunctionId(0), block.id, 0x1000);
    function.add_block(block);
    let excluded = HashMap::new();

    assert!(
        !crate::smir::lower::runtime::is_native_clobber_safe_excluding(&function, &excluded, false,)
    );
    assert!(
        !crate::smir::lower::runtime::is_native_clobber_safe_excluding(&function, &excluded, true,)
    );
}
