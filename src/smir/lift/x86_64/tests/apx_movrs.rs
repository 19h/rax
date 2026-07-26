//! Strict lifting, interpretation, optimization, and JIT admission for MOVRS.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, SmirMemory};

const INITIAL_DESTINATION: u64 = 0xA1B2_C3D4_E5F6_7788;
const INITIAL_FLAGS: u64 = 0xCD7;
const SOURCE: u64 = 0x0123_4567_89AB_CDEF;

fn legacy_encoding(width: OpWidth) -> Vec<u8> {
    match width {
        OpWidth::W8 => vec![0x44, 0x0F, 0x38, 0x8A, 0x03],
        OpWidth::W16 => vec![0x66, 0x44, 0x0F, 0x38, 0x8B, 0x03],
        OpWidth::W32 => vec![0x44, 0x0F, 0x38, 0x8B, 0x03],
        OpWidth::W64 => vec![0x4C, 0x0F, 0x38, 0x8B, 0x03],
        _ => unreachable!(),
    }
}

fn apx_encoding(width: OpWidth) -> Vec<u8> {
    let (p1, opcode) = match width {
        OpWidth::W8 => (0x78, 0x8A),
        OpWidth::W16 => (0x79, 0x8B),
        OpWidth::W32 => (0x78, 0x8B),
        OpWidth::W64 => (0xF8, 0x8B),
        _ => unreachable!(),
    };
    vec![0x62, 0xEC, p1, 0x08, opcode, 0x44, 0x91, 0x20]
}

fn width_mask(width: OpWidth) -> u64 {
    match width {
        OpWidth::W8 => 0xFF,
        OpWidth::W16 => 0xFFFF,
        OpWidth::W32 => 0xFFFF_FFFF,
        OpWidth::W64 => u64::MAX,
        _ => unreachable!(),
    }
}

fn merge_gpr(old: u64, value: u64, width: OpWidth) -> u64 {
    match width {
        OpWidth::W8 | OpWidth::W16 => (old & !width_mask(width)) | (value & width_mask(width)),
        OpWidth::W32 => value as u32 as u64,
        OpWidth::W64 => value,
        _ => unreachable!(),
    }
}

fn movrs_function(bytes: &[u8]) -> SmirFunction {
    let result = lift_single(bytes).expect("strictly lift MOVRS");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut function = SmirFunction::new(FunctionId(0), block.id, 0x1000);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), 0x1000),
        crate::smir::ir::X86InstructionBytes::new(bytes).expect("MOVRS instruction provenance"),
    );
    function
}

#[test]
fn movrs_strictly_lifts_all_legacy_and_apx_widths() {
    for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
        let legacy_bytes = legacy_encoding(width);
        let legacy = lift_single(&legacy_bytes).expect("legacy MOVRS");
        assert_eq!(legacy.bytes_consumed, legacy_bytes.len());
        assert!(matches!(legacy.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            legacy.ops.as_slice(),
            [SmirOp {
                id: OpId(0),
                guest_pc: 0x1000,
                kind: OpKind::Load {
                    dst,
                    addr: Address::Direct(base),
                    width: got_width,
                    sign: SignExtend::Zero,
                },
                x86_hint: None,
            }] if *dst == x86_gpr(8)
                && *base == x86_gpr(3)
                && *got_width == width.to_mem_width()
        ));

        let apx_bytes = apx_encoding(width);
        let apx = lift_single(&apx_bytes).expect("APX MOVRS");
        assert_eq!(apx.bytes_consumed, apx_bytes.len());
        assert!(matches!(apx.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            apx.ops.as_slice(),
            [
                SmirOp {
                    id: OpId(0),
                    guest_pc: 0x1000,
                    kind: OpKind::X86RequireApx,
                    x86_hint: None,
                },
                SmirOp {
                    id: OpId(1),
                    guest_pc: 0x1000,
                    kind: OpKind::Load {
                        dst,
                        addr: Address::BaseIndexScale {
                            base: Some(base),
                            index,
                            scale: 4,
                            disp: 0x20,
                            disp_size: DispSize::Disp8,
                        },
                        width: got_width,
                        sign: SignExtend::Zero,
                    },
                    x86_hint: None,
                },
            ] if *dst == x86_gpr(16)
                && *base == x86_gpr(17)
                && *index == x86_gpr(18)
                && *got_width == width.to_mem_width()
        ));
    }

    let legacy_w_precedence =
        lift_single(&[0x66, 0x4C, 0x0F, 0x38, 0x8B, 0x03]).expect("REX.W+66 MOVRS");
    assert!(matches!(
        legacy_w_precedence.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Load {
                width: MemWidth::B8,
                ..
            },
            ..
        }]
    ));
    let apx_w_precedence =
        lift_single(&[0x62, 0xEC, 0xF9, 0x08, 0x8B, 0x44, 0x91, 0x20]).expect("APX W1+66 MOVRS");
    assert!(matches!(
        apx_w_precedence.ops.last(),
        Some(SmirOp {
            kind: OpKind::Load {
                width: MemWidth::B8,
                ..
            },
            ..
        })
    ));
}

#[test]
fn legacy_movrs_high_byte_merge_occurs_only_after_the_load() {
    let result = lift_single(&[0x0F, 0x38, 0x8A, 0x23]).expect("MOVRS AH,[RBX]");
    assert_eq!(result.bytes_consumed, 4);
    assert_eq!(result.ops.len(), 5);
    let loaded = match result.ops[0].kind {
        OpKind::Load {
            dst,
            addr: Address::Direct(base),
            width: MemWidth::B1,
            sign: SignExtend::Zero,
        } if base == x86_gpr(3) => dst,
        ref other => panic!("unexpected MOVRS AH load: {other:?}"),
    };
    assert!(matches!(loaded, VReg::Virtual(_)));
    assert!(matches!(
        result.ops[1].kind,
        OpKind::And {
            src1,
            src2: SrcOperand::Imm(0xFF),
            flags: FlagUpdate::None,
            ..
        } if src1 == loaded
    ));
    assert!(matches!(
        result.ops[4].kind,
        OpKind::Or {
            dst,
            flags: FlagUpdate::None,
            ..
        } if dst == x86_gpr(0)
    ));
    for (index, op) in result.ops.iter().enumerate() {
        assert_eq!(op.id, OpId(index as u16));
    }

    let rex = lift_single(&[0x40, 0x0F, 0x38, 0x8A, 0x23]).expect("MOVRS SPL,[RBX]");
    assert!(matches!(
        rex.ops.as_slice(),
        [SmirOp {
            kind: OpKind::Load {
                dst,
                width: MemWidth::B1,
                ..
            },
            ..
        }] if *dst == x86_gpr(4)
    ));
}

fn assert_movrs_ud(bytes: &[u8], expected_len: usize, name: &str) {
    let result = lift_single(bytes)
        .unwrap_or_else(|error| panic!("{name} must strictly lift to #UD: {error:?}"));
    assert_invalid_opcode_trap(&result, expected_len);
}

#[test]
fn movrs_invalid_forms_are_terminal_at_the_earliest_decoded_frontier() {
    for (bytes, expected_len, name) in [
        (&[0xF0, 0x0F, 0x38, 0x8B, 0x03][..], 4, "legacy LOCK"),
        (&[0xF2, 0x0F, 0x38, 0x8B, 0x03][..], 4, "legacy F2"),
        (&[0xF3, 0x0F, 0x38, 0x8B, 0x03][..], 4, "legacy F3"),
        (&[0x44, 0x0F, 0x38, 0x8B, 0xC0][..], 5, "legacy register"),
        (&[0x62, 0xEC, 0x7A, 0x08, 0x8B][..], 5, "APX F3 pp"),
        (&[0x62, 0xEC, 0x7B, 0x08, 0x8B][..], 5, "APX F2 pp"),
        (&[0x62, 0xEC, 0x78, 0x18, 0x8B][..], 5, "APX ND"),
        (&[0x62, 0xEC, 0x78, 0x0C, 0x8B][..], 5, "APX NF"),
        (&[0x62, 0xEC, 0x78, 0x88, 0x8B][..], 5, "APX z"),
        (&[0x62, 0xEC, 0x78, 0x28, 0x8B][..], 5, "APX LL"),
        (&[0x62, 0xEC, 0x78, 0x48, 0x8B][..], 5, "APX L-prime"),
        (&[0x62, 0xEC, 0x78, 0x09, 0x8B][..], 5, "APX payload 0"),
        (&[0x62, 0xEC, 0x78, 0x0A, 0x8B][..], 5, "APX payload 1"),
        (&[0x62, 0xEC, 0x38, 0x08, 0x8B][..], 5, "APX V3:0"),
        (&[0x62, 0xEC, 0x78, 0x00, 0x8B][..], 5, "APX V4"),
        (&[0x62, 0xEC, 0xF8, 0x08, 0x8A][..], 5, "APX byte W"),
        (&[0x62, 0xEC, 0x79, 0x08, 0x8A][..], 5, "APX byte 66"),
        (&[0x62, 0xEC, 0x78, 0x08, 0x8B, 0xC0][..], 6, "APX register"),
    ] {
        assert_movrs_ud(bytes, expected_len, name);
    }

    // Fixed fields establish #UD without demanding an apparent SIB or
    // displacement tail from the following ModR/M byte.
    assert_movrs_ud(
        &[0x62, 0xEC, 0x78, 0x18, 0x8B, 0x84],
        5,
        "APX ND apparent SIB tail",
    );

    for bytes in [
        &[0xF0, 0x62, 0xEC, 0x78, 0x08, 0x8B, 0x03][..],
        &[0x66, 0x62, 0xEC, 0x78, 0x08, 0x8B, 0x03],
        &[0xF2, 0x62, 0xEC, 0x78, 0x08, 0x8B, 0x03],
        &[0xF3, 0x62, 0xEC, 0x78, 0x08, 0x8B, 0x03],
        &[0x48, 0x62, 0xEC, 0x78, 0x08, 0x8B, 0x03],
    ] {
        let result = lift_single(bytes).expect("legacy prefix before APX MOVRS must be #UD");
        assert_invalid_opcode_trap(&result, 2);
    }
}

#[test]
fn apx_movrs_incomplete_lengths_are_absolute() {
    assert!(matches!(
        lift_single(&[0x62, 0xEC, 0x78, 0x08, 0x8B]),
        Err(LiftError::Incomplete {
            have: 5,
            need: 6,
            ..
        })
    ));
    assert!(matches!(
        lift_single(&[0x62, 0xEC, 0x78, 0x08, 0x8B, 0x84]),
        Err(LiftError::Incomplete {
            have: 6,
            need: 7,
            ..
        })
    ));
    assert!(matches!(
        lift_single(&[0x62, 0xEC, 0x78, 0x08, 0x8B, 0x84, 0x03]),
        Err(LiftError::Incomplete {
            have: 7,
            need: 11,
            ..
        })
    ));
}

#[test]
fn apx_movrs_preserves_segment_addr32_and_egpr_addressing() {
    let fs =
        lift_single(&[0x64, 0x62, 0xEC, 0xF8, 0x08, 0x8B, 0x44, 0x91, 0x20]).expect("FS APX MOVRS");
    assert!(matches!(fs.ops[0].kind, OpKind::X86RequireApx));
    assert!(matches!(
        &fs.ops[1].kind,
        OpKind::Load {
            dst,
            addr: Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                base: Some(base),
                index: Some(index),
                scale: 4,
                disp: 0x20,
            },
            width: MemWidth::B8,
            ..
        } if *dst == x86_gpr(16) && *base == x86_gpr(17) && *index == x86_gpr(18)
    ));

    let addr32 = lift_single(&[0x67, 0x62, 0xEC, 0x78, 0x08, 0x8B, 0x44, 0x91, 0x20])
        .expect("addr32 APX MOVRS");
    assert!(matches!(
        &addr32.ops[1].kind,
        OpKind::Load {
            dst,
            addr: Address::X86Addr32(inner),
            width: MemWidth::B4,
            ..
        } if *dst == x86_gpr(16) && matches!(
            inner.as_ref(),
            Address::BaseIndexScale {
                base: Some(base),
                index,
                scale: 4,
                disp: 0x20,
                disp_size: DispSize::Disp8,
            } if *base == x86_gpr(17) && *index == x86_gpr(18)
        )
    ));
}

#[test]
fn movrs_interpretation_and_o2_preserve_partial_writes_flags_and_fault_order() {
    const ADDRESS: u64 = 0x20;

    for apx in [false, true] {
        for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            let encoding = if apx {
                apx_encoding(width)
            } else {
                legacy_encoding(width)
            };
            let original = movrs_function(&encoding);
            let mut optimized = original.clone();
            crate::smir::optimize::optimize_function(
                &mut optimized,
                crate::smir::optimize::OptLevel::O2,
            );

            for function in [&original, &optimized] {
                let mut context = SmirContext::new_x86_64();
                context.write_vreg(
                    if apx { x86_gpr(16) } else { x86_gpr(8) },
                    INITIAL_DESTINATION,
                );
                if apx {
                    context.write_vreg(x86_gpr(17), ADDRESS - 0x20);
                    context.write_vreg(x86_gpr(18), 0);
                    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                        unreachable!()
                    };
                    x86.apx_enabled = true;
                } else {
                    context.write_vreg(x86_gpr(3), ADDRESS);
                }
                context.flags.materialized = MaterializedFlags::from_rflags(INITIAL_FLAGS);
                let mut memory = FlatMemory::new(0x100);
                memory.write(ADDRESS, &SOURCE.to_le_bytes()).unwrap();
                let execution = SmirInterpreter::new().execute_block(
                    &mut context,
                    &mut memory,
                    function.entry_block().unwrap(),
                );
                assert!(matches!(execution, BlockResult::Exit(ExitReason::Halt)));
                assert_eq!(
                    context.read_vreg(if apx { x86_gpr(16) } else { x86_gpr(8) }),
                    merge_gpr(INITIAL_DESTINATION, SOURCE, width),
                    "APX={apx} width={width:?}"
                );
                assert_eq!(context.flags.materialized.to_rflags(), INITIAL_FLAGS);
            }
        }
    }

    let high_byte = movrs_function(&[0x0F, 0x38, 0x8A, 0x23]);
    let mut context = SmirContext::new_x86_64();
    context.write_vreg(x86_gpr(0), INITIAL_DESTINATION);
    context.write_vreg(x86_gpr(3), ADDRESS);
    context.flags.materialized = MaterializedFlags::from_rflags(INITIAL_FLAGS);
    let mut memory = FlatMemory::new(0x100);
    memory.write(ADDRESS, &[0x5A]).unwrap();
    let execution = SmirInterpreter::new().execute_block(
        &mut context,
        &mut memory,
        high_byte.entry_block().unwrap(),
    );
    assert!(matches!(execution, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(
        context.read_vreg(x86_gpr(0)),
        (INITIAL_DESTINATION & !0xFF00) | 0x5A00
    );
    assert_eq!(context.flags.materialized.to_rflags(), INITIAL_FLAGS);

    let apx_fault = movrs_function(&apx_encoding(OpWidth::W64));
    for enabled in [false, true] {
        let mut context = SmirContext::new_x86_64();
        context.write_vreg(x86_gpr(16), INITIAL_DESTINATION);
        context.write_vreg(x86_gpr(17), 0x200);
        context.write_vreg(x86_gpr(18), 0);
        context.flags.materialized = MaterializedFlags::from_rflags(INITIAL_FLAGS);
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.apx_enabled = enabled;
        let execution = SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(0x40),
            apx_fault.entry_block().unwrap(),
        );
        if enabled {
            assert!(matches!(
                execution,
                BlockResult::Exit(ExitReason::MemoryFault {
                    addr: 0x220,
                    write: false,
                })
            ));
        } else {
            assert!(matches!(
                execution,
                BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
            ));
        }
        assert_eq!(context.read_vreg(x86_gpr(16)), INITIAL_DESTINATION);
        assert_eq!(context.flags.materialized.to_rflags(), INITIAL_FLAGS);
    }
}

#[cfg(feature = "smir-jit")]
#[test]
fn movrs_jit_gate_admits_every_helper_backed_memory_shape() {
    use crate::smir::lower::runtime::{is_native_clobber_safe, is_native_clobber_safe_excluding};

    for apx in [false, true] {
        for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            let encoding = if apx {
                apx_encoding(width)
            } else {
                legacy_encoding(width)
            };
            let mut function = movrs_function(&encoding);
            let entry = function.entry;
            function
                .get_block_mut(entry)
                .unwrap()
                .set_terminator(Terminator::Return { values: vec![] });
            let mut optimized = function.clone();
            crate::smir::optimize::optimize_function(
                &mut optimized,
                crate::smir::optimize::OptLevel::O2,
            );
            for candidate in [&function, &optimized] {
                assert!(!is_native_clobber_safe(candidate));
                assert!(is_native_clobber_safe_excluding(
                    candidate,
                    &std::collections::HashMap::new(),
                    true,
                ));
            }
        }
    }

    let mut high_byte = movrs_function(&[0x0F, 0x38, 0x8A, 0x23]);
    let entry = high_byte.entry;
    high_byte
        .get_block_mut(entry)
        .unwrap()
        .set_terminator(Terminator::Return { values: vec![] });
    let mut optimized_high_byte = high_byte.clone();
    crate::smir::optimize::optimize_function(
        &mut optimized_high_byte,
        crate::smir::optimize::OptLevel::O2,
    );
    for candidate in [&high_byte, &optimized_high_byte] {
        assert!(is_native_clobber_safe_excluding(
            candidate,
            &std::collections::HashMap::new(),
            true,
        ));
    }

    for encoding in [
        &[0x40, 0x0F, 0x38, 0x8A, 0x23][..],
        &[0x66, 0x0F, 0x38, 0x8B, 0x2B],
        &[0x0F, 0x38, 0x8B, 0x23],
        &[0x48, 0x0F, 0x38, 0x8B, 0x2B],
        &[0x62, 0xF4, 0x78, 0x08, 0x8A, 0x23],
        &[0x62, 0xF4, 0x79, 0x08, 0x8B, 0x2B],
        &[0x62, 0xF4, 0x78, 0x08, 0x8B, 0x23],
        &[0x62, 0xF4, 0xF8, 0x08, 0x8B, 0x2B],
    ] {
        let mut state_backed = movrs_function(encoding);
        let entry = state_backed.entry;
        state_backed
            .get_block_mut(entry)
            .unwrap()
            .set_terminator(Terminator::Return { values: vec![] });
        let mut optimized = state_backed.clone();
        crate::smir::optimize::optimize_function(
            &mut optimized,
            crate::smir::optimize::OptLevel::O2,
        );
        for candidate in [&state_backed, &optimized] {
            assert!(
                is_native_clobber_safe_excluding(
                    candidate,
                    &std::collections::HashMap::new(),
                    true,
                ),
                "state-backed MOVRS {encoding:02X?}"
            );
        }

        // The MOVRS-specific recognizer still demands exact opcode provenance.
        // Without it the same IR is admitted only as a generic helper-backed
        // scalar load into a state-backed destination, which commits through the
        // `GuestRegs` slot and is therefore equally safe.
        let mut missing_provenance = state_backed.clone();
        missing_provenance.x86_instruction_bytes.clear();
        let block = missing_provenance.entry_block().unwrap();
        assert!(
            crate::smir::lower::runtime::x86_jit_movrs_state_backed_load_sequence_len(
                block, 0, true, None,
            )
            .is_none(),
            "the MOVRS recognizer must fail closed without provenance: {encoding:02X?}"
        );
        assert!(
            is_native_clobber_safe_excluding(
                &missing_provenance,
                &std::collections::HashMap::new(),
                true,
            ),
            "the generic state-backed load path must still admit it: {encoding:02X?}"
        );
    }
}
