use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint};
use crate::smir::ir::types::{OpId, OpWidth, SrcOperand, VReg, VecWidth, VirtualId};

#[test]
fn scalar_compare_rewrites_match_six_independent_llvm_23_anchors() {
    let anchors: &[(&[u8], &[u8], ScalarFormat, u8, u8, u8, u8)] = &[
        (
            &[0x62, 0xF1, 0x76, 0x08, 0xC2, 0x12, 0x00],
            &[0x62, 0xF1, 0x76, 0x08, 0xC2, 0x14, 0x24, 0x00],
            ScalarFormat::F32,
            2,
            1,
            0,
            0,
        ),
        (
            &[0x62, 0xF1, 0x76, 0x03, 0xC2, 0x6C, 0x24, 0x04, 0x1F],
            &[0x62, 0xF1, 0x76, 0x03, 0xC2, 0x2C, 0x24, 0x1F],
            ScalarFormat::F32,
            5,
            17,
            3,
            31,
        ),
        (
            &[0x62, 0xF1, 0x87, 0x06, 0xC2, 0x3C, 0x48, 0x0D],
            &[0x62, 0xF1, 0x87, 0x06, 0xC2, 0x3C, 0x24, 0x0D],
            ScalarFormat::F64,
            7,
            31,
            6,
            13,
        ),
        (
            &[
                0x62, 0xD1, 0xBF, 0x08, 0xC2, 0x8D, 0x7F, 0x00, 0x00, 0x00, 0x04,
            ],
            &[0x62, 0xF1, 0xBF, 0x08, 0xC2, 0x0C, 0x24, 0x04],
            ScalarFormat::F64,
            1,
            8,
            0,
            4,
        ),
        (
            &[0x62, 0xD3, 0x06, 0x06, 0xC2, 0x3C, 0x24, 0x07],
            &[0x62, 0xF3, 0x06, 0x06, 0xC2, 0x3C, 0x24, 0x07],
            ScalarFormat::F16,
            7,
            31,
            6,
            7,
        ),
        (
            &[0x62, 0x93, 0x7E, 0x00, 0xC2, 0x5C, 0x88, 0x20, 0x18],
            &[0x62, 0xF3, 0x7E, 0x00, 0xC2, 0x1C, 0x24, 0x18],
            ScalarFormat::F16,
            3,
            16,
            0,
            24,
        ),
    ];

    for (memory, stack, format, destination, source1, mask, predicate) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_scalar_fp_compare_memory_encoding()
            .unwrap_or_else(|| panic!("LLVM anchor rejected: {memory:02X?}"));
        assert_eq!(encoding.elem, format.elem(), "{memory:02X?}");
        assert_eq!(encoding.destination, *destination, "{memory:02X?}");
        assert_eq!(encoding.source1, *source1, "{memory:02X?}");
        assert_eq!(
            encoding.writemask,
            (*mask != 0).then_some(*mask),
            "{memory:02X?}"
        );
        assert_eq!(encoding.predicate, *predicate, "{memory:02X?}");
        assert_eq!(
            encoding.memory_width,
            format.memory_width(),
            "{memory:02X?}"
        );
        assert_eq!(
            encoding.stack_instruction.as_slice(),
            *stack,
            "{memory:02X?}"
        );
    }
}

#[test]
fn scalar_compare_classifier_exhausts_2_359_296_control_operand_and_apx_cells() {
    let mut accepted = 0usize;
    for format in ScalarFormat::ALL {
        for ll in 0..=2u8 {
            for destination in 0..8u8 {
                for source1 in 0..32u8 {
                    for mask in 0..8u8 {
                        for predicate in 0..32u8 {
                            let canonical = memory_encoding(
                                format,
                                destination,
                                source1,
                                ll,
                                mask,
                                predicate,
                                2,
                            );
                            for base_high in [false, true] {
                                for index_high in [false, true] {
                                    let mut bytes = canonical;
                                    bytes[1] |= u8::from(base_high) << 3;
                                    if index_high {
                                        bytes[2] &= !0x04;
                                    }
                                    let encoding = X86InstructionBytes::new(&bytes)
                                        .unwrap()
                                        .evex_scalar_fp_compare_memory_encoding()
                                        .unwrap_or_else(|| panic!("{bytes:02X?}"));
                                    assert_eq!(encoding.elem, format.elem(), "{bytes:02X?}");
                                    assert_eq!(encoding.destination, destination, "{bytes:02X?}");
                                    assert_eq!(encoding.source1, source1, "{bytes:02X?}");
                                    assert_eq!(
                                        encoding.writemask,
                                        (mask != 0).then_some(mask),
                                        "{bytes:02X?}"
                                    );
                                    assert_eq!(encoding.predicate, predicate, "{bytes:02X?}");
                                    assert_eq!(encoding.ll, ll, "{bytes:02X?}");
                                    assert_eq!(
                                        encoding.memory_width,
                                        format.memory_width(),
                                        "{bytes:02X?}"
                                    );
                                    assert_eq!(
                                        encoding.needs_avx512fp16,
                                        format == ScalarFormat::F16,
                                        "{bytes:02X?}"
                                    );
                                    assert_eq!(
                                        encoding.stack_instruction.as_slice(),
                                        stack_encoding(
                                            format,
                                            destination,
                                            source1,
                                            ll,
                                            mask,
                                            predicate,
                                        ),
                                        "{bytes:02X?}"
                                    );
                                    accepted += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 3 * 3 * 8 * 32 * 8 * 32 * 2 * 2);
}

#[test]
fn scalar_compare_classifier_rejects_reserved_non_owned_and_trailing_shapes() {
    let canonical = memory_encoding(ScalarFormat::F32, 2, 17, 2, 3, 31, 2).to_vec();
    let rejects = |bytes: &[u8]| {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .and_then(|instruction| instruction.evex_scalar_fp_compare_memory_encoding()),
            None,
            "{bytes:02X?}"
        );
    };

    let mut register = canonical.clone();
    register[5] |= 0xC0;
    rejects(&register);
    let mut broadcast_or_sae = canonical.clone();
    broadcast_or_sae[3] |= 0x10;
    rejects(&broadcast_or_sae);
    let mut zeroing = canonical.clone();
    zeroing[3] |= 0x80;
    rejects(&zeroing);
    let mut reserved_ll = canonical.clone();
    reserved_ll[3] = (reserved_ll[3] & !0x60) | 0x60;
    rejects(&reserved_ll);
    let mut reserved_predicate = canonical.clone();
    *reserved_predicate.last_mut().unwrap() |= 0x20;
    rejects(&reserved_predicate);
    let mut noncanonical_k_high = canonical.clone();
    noncanonical_k_high[1] &= !0x10;
    rejects(&noncanonical_k_high);
    let mut noncanonical_k_higher = canonical.clone();
    noncanonical_k_higher[1] &= !0x80;
    rejects(&noncanonical_k_higher);
    let mut wrong_map = canonical.clone();
    wrong_map[1] = (wrong_map[1] & !0x07) | 7;
    rejects(&wrong_map);
    let mut wrong_w = canonical.clone();
    wrong_w[2] |= 0x80;
    rejects(&wrong_w);
    let mut wrong_prefix = canonical.clone();
    wrong_prefix[2] = (wrong_prefix[2] & !3) | 1;
    rejects(&wrong_prefix);
    let mut wrong_opcode = canonical.clone();
    wrong_opcode[4] ^= 1;
    rejects(&wrong_opcode);
    let mut trailing = canonical.clone();
    trailing.push(0);
    rejects(&trailing);
    rejects(&canonical[..canonical.len() - 1]);
    let mut legacy_mandatory = canonical.clone();
    legacy_mandatory.insert(0, 0x66);
    rejects(&legacy_mandatory);
}

#[test]
fn all_54_scalar_compare_scanner_cells_optimize_admit_and_lower_exactly() {
    let cases = all_cases();
    assert_eq!(cases.len(), 54);
    let mut lowerings = 0usize;
    for case in cases {
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let sequence = sequence(&function).unwrap_or_else(|| panic!("{level:?} {case:?}"));
            assert_eq!(
                sequence.encoding.elem,
                case.format.elem(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.destination,
                case.destination(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.source1, case.source1,
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.writemask,
                (case.mask() != 0).then_some(case.mask()),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.encoding.predicate, case.predicate,
                "{level:?} {case:?}"
            );
            assert_eq!(sequence.encoding.ll, case.ll, "{level:?} {case:?}");
            assert_eq!(
                sequence.encoding.memory_width,
                case.format.memory_width(),
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.load_offset,
                if case.control == MaskControl::None {
                    1
                } else {
                    2
                },
                "{level:?} {case:?}"
            );
            assert_eq!(
                sequence.consumed,
                function.blocks[0].ops.len(),
                "{level:?} {case:?}"
            );
            let (code, _) = lower(&function, case);
            let occurrences = code
                .windows(case.stack_instruction().len())
                .filter(|window| *window == case.stack_instruction())
                .count();
            assert_eq!(occurrences, 1, "{level:?} {case:?}: {code:02X?}");
            lowerings += 1;
        }
    }
    assert_eq!(lowerings, 54 * LEVELS.len());
}

#[test]
fn scalar_compare_type_e3_graph_has_exact_scalar_access_and_compare_shape() {
    for format in ScalarFormat::ALL {
        for control in MaskControl::ALL {
            let case = ScalarCompareMemoryCase {
                format,
                source1: 17,
                ll: 1,
                control,
                predicate: 29,
            };
            let function = lift_case(case);
            let ops = &function.blocks[0].ops;
            assert!(matches!(ops[0].kind, OpKind::Mov { .. }), "{case:?}");
            let load_index = if control == MaskControl::None {
                1
            } else {
                assert!(matches!(ops[1].kind, OpKind::And { .. }), "{case:?}");
                2
            };
            match (&ops[load_index].kind, control) {
                (OpKind::Load { width, .. }, MaskControl::None)
                | (OpKind::PredLoad { width, .. }, MaskControl::Masked) => {
                    assert_eq!(*width, format.memory_width(), "{case:?}")
                }
                _ => panic!("{case:?}: wrong Type E3 load"),
            }
            assert!(matches!(
                ops[load_index + 1].kind,
                OpKind::VBroadcast {
                    elem,
                    lanes: 1,
                    ..
                } if elem == format.elem()
            ));
            assert!(matches!(
                ops[load_index + 2].kind,
                OpKind::X86VectorFpCompare {
                    elem,
                    width: VecWidth::V128,
                    lanes: 1,
                    predicate: 29,
                    scalar: true,
                    mask_destination: true,
                    suppress_exceptions: false,
                    ..
                } if elem == format.elem()
            ));
        }
    }
}

#[test]
fn scalar_compare_segment_addr32_rip_sib_and_apx_addresses_admit_and_lower() {
    let case = ScalarCompareMemoryCase {
        format: ScalarFormat::F64,
        source1: 30,
        ll: 2,
        control: MaskControl::Masked,
        predicate: 14,
    };
    let canonical = case.bytes();

    let mut rip = canonical.to_vec();
    rip[5] = (rip[5] & 0x38) | 5;
    rip.splice(6..6, [0x44, 0x33, 0x22, 0x11]);

    let mut addr32 = canonical.to_vec();
    addr32.insert(0, 0x67);

    let mut fs_sib = canonical.to_vec();
    fs_sib[5] = (fs_sib[5] & 0x38) | 4;
    fs_sib.insert(6, 0x4A);
    fs_sib.insert(0, 0x64);

    let mut gs_addr32_sib = canonical.to_vec();
    gs_addr32_sib[5] = (gs_addr32_sib[5] & 0x38) | 0x44;
    gs_addr32_sib.insert(6, 0x8B);
    gs_addr32_sib.insert(7, 2);
    gs_addr32_sib.insert(0, 0x67);
    gs_addr32_sib.insert(0, 0x65);

    let mut apx_base = canonical.to_vec();
    apx_base[1] |= 0x08;

    let mut apx_index = canonical.to_vec();
    apx_index[5] = (apx_index[5] & 0x38) | 4;
    apx_index.insert(6, 0x0A);
    apx_index[2] &= !0x04;

    for (name, bytes) in [
        ("RIP+disp32", rip),
        ("addr32", addr32),
        ("FS SIB", fs_sib),
        ("GS addr32 SIB", gs_addr32_sib),
        ("APX R18 base", apx_base),
        ("APX R17 index", apx_index),
    ] {
        for level in LEVELS {
            let function = optimize(function_from_bytes(&bytes, name), level);
            assert!(
                sequence(&function).is_some(),
                "{level:?} {name}: {bytes:02X?}"
            );
            let (code, _) = lower(&function, case);
            assert!(
                code.windows(case.stack_instruction().len())
                    .any(|window| window == case.stack_instruction()),
                "{level:?} {name}"
            );
        }
    }
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(sequence(function).is_none(), "{name}");
    let excluded = HashMap::new();
    assert!(
        !is_native_clobber_safe_excluding(function, &excluded, true),
        "{name}: mutated sequence reached native admission"
    );
}

#[test]
fn scalar_compare_sequence_fails_closed_for_provenance_graph_and_frontier_mutations() {
    let case = ScalarCompareMemoryCase {
        format: ScalarFormat::F32,
        source1: 17,
        ll: 2,
        control: MaskControl::Masked,
        predicate: 19,
    };
    for level in LEVELS {
        let function = optimize(lift_case(case), level);

        let mut missing_provenance = function.clone();
        missing_provenance.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing_provenance);

        let mut wrong_provenance = function.clone();
        let mut bytes = case.bytes();
        bytes[6] ^= 1;
        wrong_provenance
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        assert_rejected("wrong predicate provenance", &wrong_provenance);

        let compare_index = function.blocks[0]
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86VectorFpCompare { .. }))
            .unwrap();
        for (name, mutate) in [
            ("missing compare hint", 0u8),
            ("wrong predicate", 1),
            ("wrong scalar flag", 2),
            ("wrong exception flag", 3),
            ("wrong source width", 4),
        ] {
            let mut mutated = function.clone();
            let op = &mut mutated.blocks[0].ops[compare_index];
            match mutate {
                0 => op.x86_hint = None,
                1 => {
                    let OpKind::X86VectorFpCompare { predicate, .. } = &mut op.kind else {
                        unreachable!()
                    };
                    *predicate ^= 1;
                }
                2 => {
                    let OpKind::X86VectorFpCompare { scalar, .. } = &mut op.kind else {
                        unreachable!()
                    };
                    *scalar = false;
                }
                3 => {
                    let OpKind::X86VectorFpCompare {
                        suppress_exceptions,
                        ..
                    } = &mut op.kind
                    else {
                        unreachable!()
                    };
                    *suppress_exceptions = true;
                }
                4 => {
                    let OpKind::X86VectorFpCompare { width, .. } = &mut op.kind else {
                        unreachable!()
                    };
                    *width = VecWidth::V256;
                }
                _ => unreachable!(),
            }
            assert_rejected(name, &mutated);
        }

        let load_index = function.blocks[0]
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::Load { .. } | OpKind::PredLoad { .. }))
            .unwrap();
        let mut wrong_load_width = function.clone();
        match &mut wrong_load_width.blocks[0].ops[load_index].kind {
            OpKind::Load { width, .. } | OpKind::PredLoad { width, .. } => *width = MemWidth::B8,
            _ => unreachable!(),
        }
        assert_rejected("wrong load width", &wrong_load_width);

        let mut wrong_guest_pc = function.clone();
        wrong_guest_pc.blocks[0].ops[compare_index].guest_pc += 1;
        assert_rejected("split guest-PC frontier", &wrong_guest_pc);

        let mut same_pc_tail = function.clone();
        same_pc_tail.blocks[0].ops.push(SmirOp::new(
            OpId(0xFFFE),
            PC,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xFFFE)),
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        assert_rejected("same-PC trailing operation", &same_pc_tail);

        let mut spurious_apx = function.clone();
        spurious_apx.blocks[0]
            .ops
            .insert(0, SmirOp::new(OpId(0xFFFD), PC, OpKind::X86RequireApx));
        assert_rejected("spurious APX guard", &spurious_apx);

        let mut apx_bytes = case.bytes();
        apx_bytes[1] |= 0x08;
        let mut missing_apx = optimize(function_from_bytes(&apx_bytes, "APX"), level);
        let guard = missing_apx.blocks[0]
            .ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86RequireApx))
            .expect("APX address guard");
        missing_apx.blocks[0].ops.remove(guard);
        assert_rejected("missing APX guard", &missing_apx);
    }
}

#[test]
fn scalar_compare_matcher_rejects_disabled_memory_and_avx_only_bridge() {
    let case = ScalarCompareMemoryCase {
        format: ScalarFormat::F64,
        source1: 30,
        ll: 2,
        control: MaskControl::Masked,
        predicate: 31,
    };
    let function = lift_case(case);
    let (definitions, uses) = virtual_counts(&function);
    assert!((0..function.blocks[0].ops.len()).all(|index| {
        x86_jit_evex_scalar_fp_compare_memory_sequence(
            &function.blocks[0],
            index,
            false,
            &function.x86_instruction_bytes,
            &definitions,
            &uses,
        )
        .is_none()
    }));

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    assert!(lowerer.lower_function(&function).is_err());
}
