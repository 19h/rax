//! Exhaustive encoding, graph, feature, and lowering admission checks.

use super::*;
use crate::smir::ir::ops::{SmirOp, X86OpHint};
use crate::smir::ir::types::{Address, FpRoundMode, OpId, OpWidth, SrcOperand, VReg, VirtualId};

fn case(spec: ConvertSpec, ll: u8, form: SourceForm, control: MaskControl) -> ConvertCase {
    ConvertCase {
        spec,
        ll,
        destination: 0,
        form,
        control,
    }
}

#[test]
fn all_396_scanner_encodings_lift_optimize_admit_and_lower_at_o0_o1_o2() {
    let mut encodings = 0usize;
    let mut optimized_graphs = 0usize;
    for spec in SPECS {
        for ll in 0..=2 {
            for form in [SourceForm::Vector, SourceForm::Broadcast] {
                for control in MaskControl::ALL {
                    let instruction = ConvertCase {
                        destination: [0, 8, 16, 31][encodings & 3],
                        ..case(spec, ll, form, control)
                    };
                    let bytes = instruction.bytes();
                    let classified = X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .evex_packed_fp16_convert_memory_encoding()
                        .unwrap_or_else(|| panic!("{instruction:?} {bytes:02X?}"));
                    assert_eq!(classified.kind, spec.kind, "{instruction:?}");
                    assert_eq!(classified.operation_width, instruction.operation_width());
                    assert_eq!(classified.source_width, instruction.source_width());
                    assert_eq!(
                        classified.destination_width,
                        instruction.destination_width()
                    );
                    assert_eq!(classified.lanes, instruction.lanes());
                    assert_eq!(classified.destination, instruction.destination);
                    assert_eq!(classified.writemask, (instruction.mask() != 0).then_some(1));
                    assert_eq!(classified.zeroing, instruction.zeroing());
                    assert_eq!(classified.broadcast, instruction.broadcast());
                    assert_eq!(classified.map, spec.map);
                    assert_eq!(classified.pp, spec.pp);
                    assert_eq!(classified.w, spec.w);
                    assert_eq!(classified.opcode, spec.opcode);
                    assert_eq!(classified.needs_avx512vl, ll != 2);
                    assert_eq!(
                        classified.kind.round(),
                        if spec.truncates() {
                            FpRoundMode::RoundTowardZero
                        } else {
                            FpRoundMode::Dynamic
                        }
                    );
                    match classified.replay {
                        X86EvexPackedFp16ConvertMemoryReplay::Vector {
                            scratch,
                            register_instruction,
                        } => {
                            assert_eq!((form, control), (SourceForm::Vector, MaskControl::None));
                            assert_eq!(scratch, instruction.scratch());
                            assert_eq!(
                                register_instruction.as_slice(),
                                instruction.expected_replay()
                            );
                        }
                        X86EvexPackedFp16ConvertMemoryReplay::Broadcast { stack_instruction } => {
                            assert_eq!(form, SourceForm::Broadcast);
                            assert_eq!(stack_instruction.as_slice(), instruction.expected_replay());
                        }
                        X86EvexPackedFp16ConvertMemoryReplay::MaskedVector {
                            stack_instruction,
                        } => {
                            assert_eq!(form, SourceForm::Vector);
                            assert_ne!(control, MaskControl::None);
                            assert_eq!(stack_instruction.as_slice(), instruction.expected_replay());
                        }
                    }

                    for level in LEVELS {
                        let function = optimize(lift_case(instruction), level);
                        let exact = sequence(&function, true)
                            .unwrap_or_else(|| panic!("{level:?} {instruction:?} {bytes:02X?}"));
                        assert_eq!(exact.encoding, classified, "{level:?} {instruction:?}");
                        assert_eq!(exact.consumed, function.blocks[0].ops.len());
                        assert_eq!(exact.memory_size, instruction.memory_size());
                        assert!(sequence(&function, false).is_none());
                        let address_op = &function.blocks[0].ops[exact.address_offset].kind;
                        match (form, control, spec.kind) {
                            (SourceForm::Vector, MaskControl::None, IntToFp16 { .. }) => {
                                assert!(matches!(address_op, OpKind::VLoad { .. }));
                            }
                            (SourceForm::Vector, MaskControl::None, FpPrecision { .. })
                                if instruction.memory_size() >= 8 =>
                            {
                                assert!(matches!(address_op, OpKind::VLoad { .. }));
                            }
                            (SourceForm::Vector, _, _) => {
                                assert!(matches!(address_op, OpKind::Lea { .. }));
                            }
                            (SourceForm::Broadcast, MaskControl::None, _) => {
                                assert!(matches!(address_op, OpKind::Load { .. }));
                            }
                            (SourceForm::Broadcast, _, IntToFp16 { .. }) => {
                                assert!(matches!(address_op, OpKind::Lea { .. }));
                            }
                            (SourceForm::Broadcast, _, _) => {
                                assert!(matches!(address_op, OpKind::PredLoad { .. }));
                            }
                        }
                        let (code, _) = lower(&function, instruction);
                        assert!(!code.is_empty(), "{level:?} {instruction:?}");
                        optimized_graphs += 1;
                    }
                    encodings += 1;
                }
            }
        }
    }
    assert_eq!(encodings, 22 * 3 * 2 * 3);
    assert_eq!(optimized_graphs, encodings * LEVELS.len());
}

#[test]
fn classifier_owns_exactly_the_22_map5_map6_opcode_pp_w_selectors() {
    let template = case(SPECS[0], 0, SourceForm::Vector, MaskControl::None).bytes();
    let mut accepted = 0usize;
    for map in 0..=7u8 {
        for opcode in 0..=u8::MAX {
            for pp in 0..=3u8 {
                for w in [false, true] {
                    let mut bytes = template.clone();
                    bytes[1] = (bytes[1] & !7) | map;
                    bytes[2] = (bytes[2] & !(0x80 | 3)) | (u8::from(w) << 7) | pp;
                    bytes[4] = opcode;
                    let expected = SPECS.iter().any(|spec| {
                        let expected_map = match spec.map {
                            X86VecMap::Map5 => 5,
                            X86VecMap::Map6 => 6,
                            _ => unreachable!(),
                        };
                        (expected_map, spec.opcode, spec.pp, spec.w) == (map, opcode, pp, w)
                    });
                    let actual = X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .evex_packed_fp16_convert_memory_encoding()
                        .is_some();
                    assert_eq!(actual, expected, "{bytes:02X?}");
                    accepted += usize::from(actual);
                }
            }
        }
    }
    assert_eq!(accepted, SPECS.len());
}

#[test]
fn all_22_masked_broadcast_encodings_match_independent_llvm_23_anchors() {
    // llvm-mc 23.0.0git, destination 17, K3 zeroing, R10+127*Tuple1,
    // LL=2, and each instruction's scalar broadcast.
    const LLVM: [[u8; 7]; 22] = [
        [0x62, 0xC5, 0xFD, 0xDB, 0x5A, 0x4A, 0x7F],
        [0x62, 0xC5, 0x7C, 0xDB, 0x5A, 0x4A, 0x7F],
        [0x62, 0xC5, 0x7D, 0xDB, 0x1D, 0x4A, 0x7F],
        [0x62, 0xC6, 0x7D, 0xDB, 0x13, 0x4A, 0x7F],
        [0x62, 0xC5, 0x7C, 0xDB, 0x5B, 0x4A, 0x7F],
        [0x62, 0xC5, 0xFC, 0xDB, 0x5B, 0x4A, 0x7F],
        [0x62, 0xC5, 0x7F, 0xDB, 0x7A, 0x4A, 0x7F],
        [0x62, 0xC5, 0xFF, 0xDB, 0x7A, 0x4A, 0x7F],
        [0x62, 0xC5, 0x7E, 0xDB, 0x7D, 0x4A, 0x7F],
        [0x62, 0xC5, 0x7F, 0xDB, 0x7D, 0x4A, 0x7F],
        [0x62, 0xC5, 0x7D, 0xDB, 0x5B, 0x4A, 0x7F],
        [0x62, 0xC5, 0x7E, 0xDB, 0x5B, 0x4A, 0x7F],
        [0x62, 0xC5, 0x7D, 0xDB, 0x7B, 0x4A, 0x7F],
        [0x62, 0xC5, 0x7D, 0xDB, 0x7A, 0x4A, 0x7F],
        [0x62, 0xC5, 0x7C, 0xDB, 0x79, 0x4A, 0x7F],
        [0x62, 0xC5, 0x7C, 0xDB, 0x78, 0x4A, 0x7F],
        [0x62, 0xC5, 0x7D, 0xDB, 0x79, 0x4A, 0x7F],
        [0x62, 0xC5, 0x7D, 0xDB, 0x78, 0x4A, 0x7F],
        [0x62, 0xC5, 0x7D, 0xDB, 0x7D, 0x4A, 0x7F],
        [0x62, 0xC5, 0x7D, 0xDB, 0x7C, 0x4A, 0x7F],
        [0x62, 0xC5, 0x7C, 0xDB, 0x7D, 0x4A, 0x7F],
        [0x62, 0xC5, 0x7C, 0xDB, 0x7C, 0x4A, 0x7F],
    ];

    for (spec, bytes) in SPECS.into_iter().zip(LLVM) {
        let encoding = X86InstructionBytes::new(&bytes)
            .unwrap()
            .evex_packed_fp16_convert_memory_encoding()
            .unwrap_or_else(|| panic!("{} {bytes:02X?}", spec.name));
        assert_eq!(encoding.kind, spec.kind, "{}", spec.name);
        assert_eq!(encoding.map, spec.map, "{}", spec.name);
        assert_eq!(encoding.destination, 17, "{}", spec.name);
        assert_eq!(encoding.writemask, Some(3), "{}", spec.name);
        assert!(encoding.zeroing && encoding.broadcast, "{}", spec.name);
        assert_eq!(encoding.operation_width, VecWidth::V512, "{}", spec.name);
        let X86EvexPackedFp16ConvertMemoryReplay::Broadcast { stack_instruction } = encoding.replay
        else {
            panic!("{}: expected broadcast replay", spec.name)
        };
        assert_eq!(
            stack_instruction.as_slice(),
            [
                0x62,
                (bytes[1] & 0x97) | 0x60,
                bytes[2] | 0x04,
                bytes[3],
                bytes[4],
                (bytes[5] & 0x38) | 0x04,
                0x24,
            ],
            "{}",
            spec.name
        );
    }
}

#[test]
fn reserved_fields_register_sources_and_trailing_bytes_fail_closed() {
    for spec in SPECS {
        let valid = case(spec, 1, SourceForm::Broadcast, MaskControl::Zero).bytes();
        let mut mutations = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !7) | 1;
        mutations.push(("map", wrong_map));
        let mut vvvv = valid.clone();
        vvvv[2] &= !0x08;
        mutations.push(("vvvv", vvvv));
        let mut v_prime = valid.clone();
        v_prime[3] &= !0x08;
        mutations.push(("V'", v_prime));
        let mut ll3 = valid.clone();
        ll3[3] = (ll3[3] & !0x60) | 0x60;
        mutations.push(("LL=3", ll3));
        let mut z_k0 = valid.clone();
        z_k0[3] &= !7;
        mutations.push(("z with k0", z_k0));
        let mut register = valid.clone();
        register[5] |= 0xC0;
        mutations.push(("register source", register));
        let mut trailing = valid.clone();
        trailing.push(0x90);
        mutations.push(("trailing byte", trailing));

        for (name, bytes) in mutations {
            assert!(
                X86InstructionBytes::new(&bytes)
                    .and_then(|instruction| {
                        instruction.evex_packed_fp16_convert_memory_encoding()
                    })
                    .is_none(),
                "{} {name} {bytes:02X?}",
                spec.name
            );
        }
    }
}

#[test]
fn segment_addr32_sib_rip_relative_and_apx_addresses_preserve_helper_provenance() {
    let address_cases: &[(&str, &[u8], bool)] = &[
        (
            "FS addr32 SIB",
            &[0x64, 0x67, 0x62, 0xC5, 0xFD, 0xDB, 0x5A, 0x4C, 0x8A, 0x7F],
            false,
        ),
        (
            "RIP relative",
            &[0x62, 0xE5, 0xFD, 0xDB, 0x5A, 0x0D, 0xFC, 0x01, 0x00, 0x00],
            false,
        ),
        (
            "SIB",
            &[0x62, 0xC5, 0xFD, 0xDB, 0x5A, 0x4C, 0x8A, 0x7F],
            false,
        ),
        ("APX B4", &[0x62, 0xFD, 0xFD, 0xD9, 0x5A, 0x02], true),
        ("APX X4", &[0x62, 0xF5, 0xF9, 0xD9, 0x5A, 0x04, 0x8A], true),
    ];

    for &(name, bytes, needs_apx) in address_cases {
        let classified = X86InstructionBytes::new(bytes)
            .unwrap()
            .evex_packed_fp16_convert_memory_encoding()
            .unwrap_or_else(|| panic!("{name} {bytes:02X?}"));
        let X86EvexPackedFp16ConvertMemoryReplay::Broadcast { stack_instruction } =
            classified.replay
        else {
            panic!("{name}: broadcast replay")
        };
        assert_eq!(stack_instruction.as_slice()[1] & 0x68, 0x60, "{name}");
        assert_ne!(stack_instruction.as_slice()[2] & 0x04, 0, "{name}");

        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            assert_eq!(
                function.blocks[0]
                    .ops
                    .first()
                    .is_some_and(|op| matches!(op.kind, OpKind::X86RequireApx)),
                needs_apx,
                "{name} {level:?}"
            );
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{name} {level:?} {bytes:02X?}"));
            assert_eq!(exact.encoding, classified, "{name} {level:?}");
        }
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(sequence(function, true).is_none(), "{name}: exact sequence");
    assert!(
        !is_native_clobber_safe_excluding(function, &HashMap::new(), true),
        "{name}: clobber gate"
    );
}

#[test]
fn sequence_fails_closed_for_provenance_semantic_ssa_address_and_frontier_mutations() {
    let representatives = [
        case(SPECS[0], 2, SourceForm::Vector, MaskControl::None),
        case(SPECS[1], 0, SourceForm::Vector, MaskControl::None),
        case(SPECS[4], 1, SourceForm::Broadcast, MaskControl::Merge),
        case(SPECS[12], 0, SourceForm::Vector, MaskControl::Zero),
    ];

    for instruction in representatives {
        let function = optimize(lift_case(instruction), OptLevel::O2);
        let exact = sequence(&function, true).unwrap_or_else(|| panic!("{instruction:?}"));
        assert_eq!(
            replay_kind(exact),
            match (instruction.form, instruction.control) {
                (SourceForm::Vector, MaskControl::None) => "vector",
                (SourceForm::Vector, _) => "masked-vector",
                (SourceForm::Broadcast, _) => "broadcast",
            }
        );

        let mut missing_provenance = function.clone();
        missing_provenance.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing_provenance);

        let mut wrong_provenance = function.clone();
        let mut bytes = instruction.bytes();
        bytes[4] ^= 1;
        wrong_provenance
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        assert_rejected("wrong provenance", &wrong_provenance);

        let conversion_index = function.blocks[0].ops.len() - 1;
        let mut wrong_hint = function.clone();
        wrong_hint.blocks[0].ops[conversion_index].x86_hint = Some(X86OpHint::MovImmModRm);
        assert_rejected("semantic hint", &wrong_hint);

        let mut wrong_pc = function.clone();
        wrong_pc.blocks[0].ops[conversion_index].guest_pc += 1;
        assert_rejected("split guest PC", &wrong_pc);

        let source = function.blocks[0].ops[conversion_index]
            .kind
            .source_vregs()
            .into_iter()
            .find(|register| matches!(register, VReg::Virtual(_)))
            .expect("conversion virtual source");
        let mut escaped_source = function.clone();
        escaped_source.blocks[0].ops.push(SmirOp::new(
            OpId(0xFF00),
            PC + 1,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xFF00)),
                src: SrcOperand::Reg(source),
                width: OpWidth::W64,
            },
        ));
        assert_rejected("source escapes", &escaped_source);

        let mut duplicate_definition = function.clone();
        duplicate_definition.blocks[0].ops.push(SmirOp::new(
            OpId(0xFF01),
            PC + 1,
            OpKind::Mov {
                dst: source,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        assert_rejected("source redefined", &duplicate_definition);

        let mut same_pc_tail = function.clone();
        same_pc_tail.blocks[0]
            .ops
            .push(SmirOp::new(OpId(0xFF02), PC, OpKind::Nop));
        assert_rejected("same-PC tail", &same_pc_tail);

        let mut preceding_same_pc = function.clone();
        preceding_same_pc.blocks[0]
            .ops
            .insert(0, SmirOp::new(OpId(0xFF03), PC, OpKind::Nop));
        assert_rejected("same-PC prefix", &preceding_same_pc);

        let mut unexpected_apx = function.clone();
        unexpected_apx.blocks[0]
            .ops
            .insert(0, SmirOp::new(OpId(0xFF04), PC, OpKind::X86RequireApx));
        assert_rejected("unnecessary APX guard", &unexpected_apx);
    }

    let reconstructed = optimize(
        lift_case(case(SPECS[4], 2, SourceForm::Broadcast, MaskControl::Merge)),
        OptLevel::O2,
    );
    let mut wrong_broadcast_address = reconstructed.clone();
    let OpKind::PredLoad { addr, .. } = wrong_broadcast_address.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .map(|op| &mut op.kind)
        .expect("reconstructed broadcast PredLoad")
    else {
        unreachable!()
    };
    let Address::BaseOffset { offset, .. } = addr else {
        unreachable!()
    };
    *offset = i64::from(SPECS[4].source_elem().bytes());
    assert_rejected("broadcast lane offset", &wrong_broadcast_address);

    let aggregate = optimize(
        lift_case(case(SPECS[0], 2, SourceForm::Broadcast, MaskControl::Merge)),
        OptLevel::O2,
    );
    let mut wrong_aggregate_mask = aggregate.clone();
    let OpKind::And {
        src2: SrcOperand::Imm(bits),
        ..
    } = wrong_aggregate_mask.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::And { .. }))
        .map(|op| &mut op.kind)
        .expect("aggregate mask")
    else {
        unreachable!()
    };
    *bits ^= 1;
    assert_rejected("aggregate mask bits", &wrong_aggregate_mask);

    let quarter = case(SPECS[12], 0, SourceForm::Vector, MaskControl::None);
    assert_eq!(quarter.source_width(), VecWidth::V64);
    assert_eq!(quarter.lanes(), 2);
    assert_eq!(quarter.memory_size(), 4);
    let quarter_function = optimize(lift_case(quarter), OptLevel::O2);
    assert_eq!(sequence(&quarter_function, true).unwrap().memory_size, 4);

    let apx = [0x62, 0xFD, 0xFD, 0xD9, 0x5A, 0x02];
    let mut missing_apx = lift_bytes(&apx);
    assert!(matches!(
        missing_apx.blocks[0].ops[0].kind,
        OpKind::X86RequireApx
    ));
    missing_apx.blocks[0].ops.remove(0);
    assert_rejected("missing APX guard", &missing_apx);
}

fn collapse_normalized_broadcast_predicate_to_raw(mut function: SmirFunction) -> SmirFunction {
    let block = &mut function.blocks[0];
    let (and_index, raw_condition) = block
        .ops
        .iter()
        .enumerate()
        .find_map(|(index, op)| match op.kind {
            OpKind::And {
                dst,
                src2: SrcOperand::Imm(bits),
                width: OpWidth::W64,
                ..
            } if bits > 1 => Some((index, dst)),
            _ => None,
        })
        .expect("aggregate opmask AND");
    let pred_index = block
        .ops
        .iter()
        .enumerate()
        .skip(and_index + 1)
        .find_map(|(index, op)| matches!(op.kind, OpKind::PredLoad { .. }).then_some(index))
        .expect("aggregate predicated load");
    let current_condition = match block.ops[pred_index].kind {
        OpKind::PredLoad { cond, .. } => cond,
        _ => unreachable!(),
    };
    if current_condition != raw_condition {
        let OpKind::PredLoad { cond, .. } = &mut block.ops[pred_index].kind else {
            unreachable!()
        };
        *cond = raw_condition;
        block.ops.drain(and_index + 1..pred_index);
    }
    function
}

#[test]
fn raw_multibit_aggregate_predload_predicate_fails_closed() {
    let function = optimize(
        lift_case(case(
            SPECS[10],
            2,
            SourceForm::Broadcast,
            MaskControl::Merge,
        )),
        OptLevel::O0,
    );
    let raw = collapse_normalized_broadcast_predicate_to_raw(function);
    assert_rejected("raw multi-bit aggregate predicate", &raw);
}

#[test]
fn lowerer_rejects_the_avx_only_vector_bridge() {
    let instruction = ConvertCase {
        destination: 17,
        ..case(SPECS[12], 2, SourceForm::Vector, MaskControl::Merge)
    };
    let function = lift_case(instruction);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    lowerer.set_jit_fault_deopt_guards(true);
    assert!(lowerer.lower_function(&function).is_err());
}
