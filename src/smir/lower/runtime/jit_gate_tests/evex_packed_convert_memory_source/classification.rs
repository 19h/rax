//! Exhaustive encoding, graph, feature, and lowering admission checks.

use super::*;
use crate::smir::ir::ops::{SmirOp, X86OpHint, X86VecMap};
use crate::smir::ir::types::{
    Address, ArchReg, FpRoundMode, OpId, OpWidth, SrcOperand, VReg, VirtualId, X86Reg,
};

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
fn all_468_scanner_encodings_lift_optimize_admit_and_lower_at_o0_o1_o2() {
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
                        .evex_packed_convert_memory_encoding()
                        .unwrap_or_else(|| panic!("{instruction:?} {bytes:02X?}"));
                    assert_eq!(classified.kind, spec.kind, "{instruction:?}");
                    assert_eq!(classified.map, X86VecMap::Map0F, "{instruction:?}");
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
                    assert_eq!(classified.pp, spec.pp);
                    assert_eq!(classified.w, spec.w);
                    assert_eq!(classified.opcode, spec.opcode);
                    assert_eq!(classified.needs_avx512vl, ll != 2);
                    assert_eq!(classified.needs_avx512dq, spec.needs_avx512dq());
                    assert_eq!(
                        classified.kind.round(),
                        if spec.truncates() {
                            FpRoundMode::RoundTowardZero
                        } else {
                            FpRoundMode::Dynamic
                        }
                    );
                    match classified.replay {
                        X86EvexPackedConvertMemoryReplay::Vector {
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
                        X86EvexPackedConvertMemoryReplay::Broadcast { stack_instruction } => {
                            assert_eq!(form, SourceForm::Broadcast);
                            assert_eq!(stack_instruction.as_slice(), instruction.expected_replay());
                        }
                        X86EvexPackedConvertMemoryReplay::MaskedVector { stack_instruction } => {
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
                        match (form, control) {
                            (SourceForm::Vector, MaskControl::None) => {
                                assert!(matches!(address_op, OpKind::VLoad { .. }));
                            }
                            (SourceForm::Vector, _) => {
                                assert!(matches!(address_op, OpKind::Lea { .. }));
                            }
                            (SourceForm::Broadcast, MaskControl::None) => {
                                assert!(matches!(address_op, OpKind::Load { .. }));
                            }
                            (SourceForm::Broadcast, _) => {
                                assert!(matches!(
                                    address_op,
                                    OpKind::PredLoad { .. } | OpKind::Lea { .. }
                                ));
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
    assert_eq!(encodings, 26 * 3 * 2 * 3);
    assert_eq!(optimized_graphs, encodings * LEVELS.len());
}

#[test]
fn all_9_fp16_widen_memory_encodings_lift_optimize_admit_and_lower_at_o0_o1_o2() {
    let mut encodings = 0usize;
    let mut optimized_graphs = 0usize;
    for ll in 0..=2 {
        for control in MaskControl::ALL {
            let instruction = ConvertCase {
                destination: [0, 17, 31][usize::from(ll)],
                ..case(FP16_WIDEN_SPEC, ll, SourceForm::Vector, control)
            };
            let bytes = instruction.bytes();
            let classified = X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_packed_convert_memory_encoding()
                .unwrap_or_else(|| panic!("{instruction:?} {bytes:02X?}"));
            assert_eq!(classified.kind, FP16_WIDEN_SPEC.kind);
            assert_eq!(classified.map, X86VecMap::Map0F38);
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
            assert!(!classified.broadcast);
            assert_eq!(
                (classified.pp, classified.w, classified.opcode),
                (1, false, 0x13)
            );
            assert_eq!(classified.needs_avx512vl, ll != 2);
            assert!(!classified.needs_avx512dq);
            assert_eq!(classified.kind.round(), FpRoundMode::Dynamic);
            match classified.replay {
                X86EvexPackedConvertMemoryReplay::Vector {
                    scratch,
                    register_instruction,
                } => {
                    assert_eq!(control, MaskControl::None);
                    assert_eq!(scratch, instruction.scratch());
                    assert_eq!(
                        register_instruction.as_slice(),
                        instruction.expected_replay()
                    );
                }
                X86EvexPackedConvertMemoryReplay::MaskedVector { stack_instruction } => {
                    assert_ne!(control, MaskControl::None);
                    assert_eq!(stack_instruction.as_slice(), instruction.expected_replay());
                }
                X86EvexPackedConvertMemoryReplay::Broadcast { .. } => {
                    panic!("Type-E11 VCVTPH2PS does not broadcast")
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
                assert!(matches!(
                    function.blocks[0].ops[exact.address_offset].kind,
                    OpKind::VLoad { .. } | OpKind::Lea { .. }
                ));
                let (code, _) = lower(&function, instruction);
                assert!(!code.is_empty(), "{level:?} {instruction:?}");
                optimized_graphs += 1;
            }
            encodings += 1;
        }
    }
    assert_eq!(encodings, 9);
    assert_eq!(optimized_graphs, encodings * LEVELS.len());
}

#[test]
fn classifier_owns_exactly_the_27_map_opcode_pp_w_selectors() {
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
                    let expected = SPECS.iter().chain([FP16_WIDEN_SPEC].iter()).any(|spec| {
                        (spec.map, spec.opcode, spec.pp, spec.w) == (map, opcode, pp, w)
                    });
                    let actual = X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .evex_packed_convert_memory_encoding()
                        .is_some();
                    assert_eq!(actual, expected, "{bytes:02X?}");
                    accepted += usize::from(actual);
                }
            }
        }
    }
    assert_eq!(accepted, SPECS.len() + 1);
}

#[test]
fn fp16_widen_memory_and_register_forms_match_independent_llvm_23_anchors() {
    // Produced by llvm-mc 23.0.0git. The memory displacements are exactly
    // 127 * Tuple-E11 size: 1,016/2,032/4,064 bytes for LL=0/1/2.
    const MEMORY: [[u8; 7]; 3] = [
        [0x62, 0xC2, 0x7D, 0x8B, 0x13, 0x4A, 0x7F],
        [0x62, 0xC2, 0x7D, 0xAB, 0x13, 0x4A, 0x7F],
        [0x62, 0xC2, 0x7D, 0xCB, 0x13, 0x4A, 0x7F],
    ];
    const REGISTER: [[u8; 6]; 3] = [
        [0x62, 0xA2, 0x7D, 0x8B, 0x13, 0xCA],
        [0x62, 0xA2, 0x7D, 0xAB, 0x13, 0xCA],
        [0x62, 0xA2, 0x7D, 0xCB, 0x13, 0xCA],
    ];

    for ll in 0..=2u8 {
        let bytes = MEMORY[usize::from(ll)];
        let encoding = X86InstructionBytes::new(&bytes)
            .unwrap()
            .evex_packed_convert_memory_encoding()
            .unwrap_or_else(|| panic!("LL={ll} {bytes:02X?}"));
        let expected = case(FP16_WIDEN_SPEC, ll, SourceForm::Vector, MaskControl::Zero);
        assert_eq!(encoding.kind, FP16_WIDEN_SPEC.kind);
        assert_eq!(encoding.map, X86VecMap::Map0F38);
        assert_eq!(encoding.destination, 17);
        assert_eq!(encoding.writemask, Some(3));
        assert!(encoding.zeroing && !encoding.broadcast);
        assert_eq!(encoding.operation_width, expected.operation_width());
        assert_eq!(encoding.source_width, expected.source_width());
        assert_eq!(encoding.destination_width, expected.destination_width());
        assert_eq!(encoding.lanes, expected.lanes());
        assert_eq!(
            encoding.replay,
            X86EvexPackedConvertMemoryReplay::MaskedVector {
                stack_instruction: X86InstructionBytes::new(&[
                    0x62, 0xE2, 0x7D, bytes[3], 0x13, 0x0C, 0x24,
                ])
                .unwrap(),
            }
        );

        let register = REGISTER[usize::from(ll)];
        assert!(
            X86InstructionBytes::new(&register)
                .unwrap()
                .evex_packed_convert_memory_encoding()
                .is_none(),
            "valid register source must not enter the memory classifier"
        );
        for level in LEVELS {
            let function = optimize(lift_bytes(&bytes), level);
            assert!(sequence(&function, true).is_some(), "LL={ll} {level:?}");
            let register_function = optimize(lift_bytes(&register), level);
            assert!(
                is_native_clobber_safe_excluding(&register_function, &HashMap::new(), true),
                "register anchor LL={ll} {level:?}"
            );
        }
    }
}

#[test]
fn all_26_masked_broadcast_encodings_match_independent_llvm_23_anchors() {
    // Produced by llvm-mc 23.0.0git for destination 17, K3 zeroing,
    // [R10 + 127*Tuple1], LL=2, and each instruction's scalar broadcast.
    const LLVM: [[u8; 7]; 26] = [
        [0x62, 0xC1, 0x7C, 0xDB, 0x5A, 0x4A, 0x7F],
        [0x62, 0xC1, 0xFD, 0xDB, 0x5A, 0x4A, 0x7F],
        [0x62, 0xC1, 0x7C, 0xDB, 0x5B, 0x4A, 0x7F],
        [0x62, 0xC1, 0xFC, 0xDB, 0x5B, 0x4A, 0x7F],
        [0x62, 0xC1, 0x7E, 0xDB, 0xE6, 0x4A, 0x7F],
        [0x62, 0xC1, 0xFE, 0xDB, 0xE6, 0x4A, 0x7F],
        [0x62, 0xC1, 0x7F, 0xDB, 0x7A, 0x4A, 0x7F],
        [0x62, 0xC1, 0xFF, 0xDB, 0x7A, 0x4A, 0x7F],
        [0x62, 0xC1, 0x7E, 0xDB, 0x7A, 0x4A, 0x7F],
        [0x62, 0xC1, 0xFE, 0xDB, 0x7A, 0x4A, 0x7F],
        [0x62, 0xC1, 0x7D, 0xDB, 0x5B, 0x4A, 0x7F],
        [0x62, 0xC1, 0x7E, 0xDB, 0x5B, 0x4A, 0x7F],
        [0x62, 0xC1, 0xFF, 0xDB, 0xE6, 0x4A, 0x7F],
        [0x62, 0xC1, 0xFD, 0xDB, 0xE6, 0x4A, 0x7F],
        [0x62, 0xC1, 0x7D, 0xDB, 0x7B, 0x4A, 0x7F],
        [0x62, 0xC1, 0x7D, 0xDB, 0x7A, 0x4A, 0x7F],
        [0x62, 0xC1, 0xFD, 0xDB, 0x7B, 0x4A, 0x7F],
        [0x62, 0xC1, 0xFD, 0xDB, 0x7A, 0x4A, 0x7F],
        [0x62, 0xC1, 0x7C, 0xDB, 0x79, 0x4A, 0x7F],
        [0x62, 0xC1, 0x7C, 0xDB, 0x78, 0x4A, 0x7F],
        [0x62, 0xC1, 0xFC, 0xDB, 0x79, 0x4A, 0x7F],
        [0x62, 0xC1, 0xFC, 0xDB, 0x78, 0x4A, 0x7F],
        [0x62, 0xC1, 0x7D, 0xDB, 0x79, 0x4A, 0x7F],
        [0x62, 0xC1, 0x7D, 0xDB, 0x78, 0x4A, 0x7F],
        [0x62, 0xC1, 0xFD, 0xDB, 0x79, 0x4A, 0x7F],
        [0x62, 0xC1, 0xFD, 0xDB, 0x78, 0x4A, 0x7F],
    ];

    for (spec, bytes) in SPECS.into_iter().zip(LLVM) {
        let encoding = X86InstructionBytes::new(&bytes)
            .unwrap()
            .evex_packed_convert_memory_encoding()
            .unwrap_or_else(|| panic!("{} {bytes:02X?}", spec.name));
        assert_eq!(encoding.kind, spec.kind, "{}", spec.name);
        assert_eq!(encoding.destination, 17, "{}", spec.name);
        assert_eq!(encoding.writemask, Some(3), "{}", spec.name);
        assert!(encoding.zeroing && encoding.broadcast, "{}", spec.name);
        assert_eq!(encoding.operation_width, VecWidth::V512, "{}", spec.name);
        let X86EvexPackedConvertMemoryReplay::Broadcast { stack_instruction } = encoding.replay
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
        wrong_map[1] = (wrong_map[1] & !7) | 2;
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
                    .and_then(|instruction| instruction.evex_packed_convert_memory_encoding())
                    .is_none(),
                "{} {name} {bytes:02X?}",
                spec.name
            );
        }
    }
}

#[test]
fn fp16_type_e11_reserved_fields_broadcast_and_noncanonical_graphs_fail_closed() {
    let instruction = case(FP16_WIDEN_SPEC, 1, SourceForm::Vector, MaskControl::Zero);
    let valid = instruction.bytes();
    assert!(
        X86InstructionBytes::new(&valid)
            .unwrap()
            .evex_packed_convert_memory_encoding()
            .is_some()
    );

    let mut mutations = Vec::new();
    let mut wrong_map = valid.clone();
    wrong_map[1] = (wrong_map[1] & !7) | 1;
    mutations.push(("map 0F", wrong_map));
    let mut wrong_pp = valid.clone();
    wrong_pp[2] &= !3;
    mutations.push(("pp", wrong_pp));
    let mut w1 = valid.clone();
    w1[2] |= 0x80;
    mutations.push(("W=1", w1));
    let mut broadcast = valid.clone();
    broadcast[3] |= 0x10;
    mutations.push(("EVEX.b", broadcast));
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
                .and_then(|instruction| instruction.evex_packed_convert_memory_encoding())
                .is_none(),
            "{name} {bytes:02X?}"
        );
    }

    let function = optimize(
        lift_case(case(
            FP16_WIDEN_SPEC,
            0,
            SourceForm::Vector,
            MaskControl::None,
        )),
        OptLevel::O2,
    );
    assert!(sequence(&function, true).is_some());

    let mut nonzero_seed = function.clone();
    let OpKind::Mov {
        src: SrcOperand::Imm(seed),
        ..
    } = &mut nonzero_seed.blocks[0].ops[0].kind
    else {
        unreachable!()
    };
    *seed = 1;
    assert_rejected("nonzero F16 seed", &nonzero_seed);

    let mut wrong_broadcast = function.clone();
    let OpKind::VBroadcast { elem, .. } = &mut wrong_broadcast.blocks[0].ops[1].kind else {
        unreachable!()
    };
    *elem = VecElementType::F32;
    assert_rejected("wrong F16 seed element", &wrong_broadcast);

    let mut wrong_load_width = function.clone();
    let OpKind::VLoad { width, .. } = &mut wrong_load_width.blocks[0].ops[2].kind else {
        unreachable!()
    };
    *width = VecWidth::V128;
    assert_rejected("wrong Type-E11 tuple width", &wrong_load_width);

    let mut wrong_map_hint = function.clone();
    let Some(X86OpHint::EvexOp { map, .. }) = &mut wrong_map_hint.blocks[0].ops[3].x86_hint else {
        unreachable!()
    };
    *map = X86VecMap::Map0F;
    assert_rejected("wrong VCVTPH2PS map hint", &wrong_map_hint);

    let mut reports_denormal = function.clone();
    let OpKind::X86PackedFpConvert {
        report_fp16_denormal,
        ..
    } = &mut reports_denormal.blocks[0].ops[3].kind
    else {
        unreachable!()
    };
    *report_fp16_denormal = true;
    assert_rejected("invented FP16 denormal reporting", &reports_denormal);

    let masked = optimize(
        lift_case(case(
            FP16_WIDEN_SPEC,
            0,
            SourceForm::Vector,
            MaskControl::Merge,
        )),
        OptLevel::O2,
    );
    let mut masked_nonzero_seed = masked.clone();
    let OpKind::Mov {
        src: SrcOperand::Imm(seed),
        ..
    } = &mut masked_nonzero_seed.blocks[0].ops[3].kind
    else {
        unreachable!()
    };
    *seed = 1;
    assert_rejected("nonzero masked lane seed", &masked_nonzero_seed);

    let mut masked_wrong_lane_width = masked;
    let OpKind::PredLoad { width, .. } = masked_wrong_lane_width.blocks[0]
        .ops
        .iter_mut()
        .find(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .map(|op| &mut op.kind)
        .expect("masked FP16 lane PredLoad")
    else {
        unreachable!()
    };
    *width = crate::smir::ir::types::MemWidth::B4;
    assert_rejected(
        "masked lane read is not two bytes",
        &masked_wrong_lane_width,
    );
}

#[test]
fn fp16_type_e11_apx_address_channels_are_guarded_and_removed_from_replay() {
    let cases: &[(&str, &[u8])] = &[
        ("APX B4", &[0x62, 0xFA, 0x7D, 0x08, 0x13, 0x02]),
        ("APX X4", &[0x62, 0xF2, 0x79, 0x08, 0x13, 0x04, 0x8A]),
    ];
    for &(name, bytes) in cases {
        let classified = X86InstructionBytes::new(bytes)
            .unwrap()
            .evex_packed_convert_memory_encoding()
            .unwrap_or_else(|| panic!("{name} {bytes:02X?}"));
        let X86EvexPackedConvertMemoryReplay::Vector {
            register_instruction,
            ..
        } = classified.replay
        else {
            panic!("{name}: vector replay")
        };
        assert_ne!(register_instruction.as_slice()[2] & 0x04, 0, "{name}");
        for level in LEVELS {
            let function = optimize(lift_bytes(bytes), level);
            assert!(
                matches!(function.blocks[0].ops[0].kind, OpKind::X86RequireApx),
                "{name} {level:?}"
            );
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{name} {level:?} {bytes:02X?}"));
            assert_eq!(exact.encoding, classified, "{name} {level:?}");
        }
    }
}

#[test]
fn segment_addr32_sib_rip_relative_and_apx_addresses_preserve_helper_provenance() {
    // First three encodings are independent llvm-mc 23 anchors.
    let address_cases: &[(&str, &[u8], bool)] = &[
        (
            "FS addr32 SIB",
            &[0x64, 0x67, 0x62, 0xC1, 0x7C, 0xDB, 0x5A, 0x4C, 0x8A, 0x7F],
            false,
        ),
        (
            "RIP relative",
            &[0x62, 0xE1, 0x7C, 0xDB, 0x5A, 0x0D, 0xFC, 0x01, 0x00, 0x00],
            false,
        ),
        (
            "SIB",
            &[0x62, 0xC1, 0x7C, 0xDB, 0x5A, 0x4C, 0x8A, 0x7F],
            false,
        ),
        // APX B4 selects R18 as the direct base; encoded X4 selects R17 as
        // the SIB index. Both must be guarded and removed from native replay.
        ("APX B4", &[0x62, 0xF9, 0x7C, 0xD9, 0x5A, 0x02], true),
        ("APX X4", &[0x62, 0xF1, 0x78, 0xD9, 0x5A, 0x04, 0x8A], true),
    ];

    for &(name, bytes, needs_apx) in address_cases {
        let classified = X86InstructionBytes::new(bytes)
            .unwrap()
            .evex_packed_convert_memory_encoding()
            .unwrap_or_else(|| panic!("{name} {bytes:02X?}"));
        let X86EvexPackedConvertMemoryReplay::Broadcast { stack_instruction } = classified.replay
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
fn sequence_fails_closed_for_provenance_semantic_ssa_and_frontier_mutations() {
    let representatives = [
        case(SPECS[0], 2, SourceForm::Vector, MaskControl::None),
        case(SPECS[1], 1, SourceForm::Broadcast, MaskControl::Merge),
        case(SPECS[2], 0, SourceForm::Broadcast, MaskControl::Zero),
        case(SPECS[17], 2, SourceForm::Vector, MaskControl::Merge),
        case(FP16_WIDEN_SPEC, 0, SourceForm::Vector, MaskControl::None),
        case(FP16_WIDEN_SPEC, 2, SourceForm::Vector, MaskControl::Merge),
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
        lift_case(case(SPECS[2], 2, SourceForm::Broadcast, MaskControl::Merge)),
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
    *offset = i64::from(SPECS[2].source_elem().bytes());
    assert_rejected("broadcast lane offset", &wrong_broadcast_address);

    let aggregate = optimize(
        lift_case(case(SPECS[0], 2, SourceForm::Broadcast, MaskControl::Merge)),
        OptLevel::O2,
    );
    let mut wrong_aggregate_mask = aggregate.clone();
    let OpKind::And {
        src2: SrcOperand::Imm(bits),
        ..
    } = &mut wrong_aggregate_mask.blocks[0].ops[1].kind
    else {
        unreachable!()
    };
    *bits ^= 1;
    assert_rejected("aggregate mask bits", &wrong_aggregate_mask);

    let apx = [0x62, 0xF9, 0x7C, 0xD9, 0x5A, 0x02];
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
        lift_case(case(SPECS[0], 2, SourceForm::Broadcast, MaskControl::Merge)),
        OptLevel::O0,
    );
    let raw = collapse_normalized_broadcast_predicate_to_raw(function);
    assert_rejected("raw multi-bit aggregate predicate", &raw);
}

#[test]
fn lowerer_rejects_the_avx_only_vector_bridge() {
    let instruction = ConvertCase {
        destination: 17,
        ..case(SPECS[17], 2, SourceForm::Vector, MaskControl::Merge)
    };
    let function = lift_case(instruction);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    lowerer.set_jit_fault_deopt_guards(true);
    assert!(lowerer.lower_function(&function).is_err());
}
