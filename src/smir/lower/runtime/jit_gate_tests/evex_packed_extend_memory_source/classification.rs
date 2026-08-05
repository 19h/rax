//! Exhaustive encoding, graph, feature, and lowering admission checks.

use super::*;
use crate::smir::ir::ops::{SmirOp, X86OpHint};
use crate::smir::ir::types::{Address, OpId, OpWidth, SignExtend, SrcOperand, VirtualId};

fn case(spec: ExtendSpec, w: bool, ll: u8, control: MaskControl) -> ExtendCase {
    ExtendCase {
        spec,
        w,
        ll,
        destination: 0,
        control,
    }
}

#[test]
fn all_198_scanner_encodings_lift_optimize_admit_and_lower_at_o0_o1_o2() {
    let mut encodings = 0usize;
    let mut optimized_graphs = 0usize;
    for mut instruction in all_cases() {
        instruction.destination = [0, 8, 16, 31][encodings & 3];
        let bytes = instruction.bytes();
        let classified = X86InstructionBytes::new(&bytes)
            .unwrap()
            .evex_packed_extend_memory_encoding()
            .unwrap_or_else(|| panic!("{instruction:?} {bytes:02X?}"));
        assert_eq!(classified.source_elem, instruction.spec.source_elem);
        assert_eq!(
            classified.destination_elem,
            instruction.spec.destination_elem
        );
        assert_eq!(classified.width, instruction.width());
        assert_eq!(classified.source_width, instruction.source_width());
        assert_eq!(classified.lanes, instruction.lanes());
        assert_eq!(classified.memory_size(), instruction.memory_size());
        assert_eq!(classified.destination, instruction.destination);
        assert_eq!(classified.writemask, (instruction.mask() != 0).then_some(1));
        assert_eq!(classified.zeroing, instruction.zeroing());
        assert_eq!(classified.signed, instruction.spec.signed);
        assert_eq!(classified.opcode, instruction.spec.opcode);
        assert_eq!(classified.w, instruction.w);
        assert_eq!(classified.needs_avx512vl, instruction.ll != 2);
        assert_eq!(
            classified.instruction_needs_avx512bw,
            instruction.spec.instruction_needs_avx512bw()
        );
        assert_eq!(
            classified.transfer_width(),
            if instruction.source_width() == VecWidth::V64 {
                VecWidth::V128
            } else {
                instruction.source_width()
            }
        );
        match classified.replay {
            X86EvexPackedExtendMemoryReplay::Vector {
                scratch,
                register_instruction,
            } => {
                assert_eq!(instruction.control, MaskControl::None);
                assert_eq!(scratch, instruction.scratch());
                assert_eq!(
                    register_instruction.as_slice(),
                    instruction.expected_replay()
                );
            }
            X86EvexPackedExtendMemoryReplay::MaskedVector { stack_instruction } => {
                assert_ne!(instruction.control, MaskControl::None);
                assert_eq!(stack_instruction.as_slice(), instruction.expected_replay());
            }
        }

        for level in LEVELS {
            let function = optimize(lift_case(instruction), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {instruction:?} {bytes:02X?}"));
            assert_eq!(exact.encoding, classified, "{level:?} {instruction:?}");
            assert_eq!(exact.consumed, function.blocks[0].ops.len());
            assert_eq!(exact.address_offset, 2);
            assert_eq!(exact.memory_size, instruction.memory_size());
            assert!(matches!(
                function.blocks[0].ops[exact.address_offset].kind,
                OpKind::Lea { .. }
            ));
            assert!(sequence(&function, false).is_none());
            let (code, _) = lower(&function, instruction);
            assert!(!code.is_empty(), "{level:?} {instruction:?}");
            optimized_graphs += 1;
        }
        encodings += 1;
    }
    assert_eq!(encodings, 198);
    assert_eq!(optimized_graphs, 198 * LEVELS.len());
}

#[test]
fn classifier_owns_exactly_the_22_map0f38_opcode_pp_w_selectors() {
    let template = case(SPECS[0], false, 0, MaskControl::None).bytes();
    let mut accepted = 0usize;
    for map in 0..=7u8 {
        for opcode in 0..=u8::MAX {
            for pp in 0..=3u8 {
                for w in [false, true] {
                    let mut bytes = template.clone();
                    bytes[1] = (bytes[1] & !7) | map;
                    bytes[2] = (bytes[2] & !(0x80 | 3)) | (u8::from(w) << 7) | pp;
                    bytes[4] = opcode;
                    let expected = map == 2
                        && pp == 1
                        && SPECS
                            .iter()
                            .any(|spec| spec.opcode == opcode && spec.w_values().contains(&w));
                    let actual = X86InstructionBytes::new(&bytes)
                        .unwrap()
                        .evex_packed_extend_memory_encoding()
                        .is_some();
                    assert_eq!(actual, expected, "{bytes:02X?}");
                    accepted += usize::from(actual);
                }
            }
        }
    }
    assert_eq!(accepted, 22);
}

#[test]
fn all_12_masked_memory_families_match_independent_llvm_23_anchors() {
    // llvm-mc 23.0.0git: destination ZMM17, K3 zeroing, R10 plus a
    // tuple-scaled disp8 of 127, LL=2. WIG families use LLVM's W=0 choice.
    const LLVM: [[u8; 7]; 12] = [
        [0x62, 0xC2, 0x7D, 0xCB, 0x20, 0x4A, 0x7F],
        [0x62, 0xC2, 0x7D, 0xCB, 0x21, 0x4A, 0x7F],
        [0x62, 0xC2, 0x7D, 0xCB, 0x22, 0x4A, 0x7F],
        [0x62, 0xC2, 0x7D, 0xCB, 0x23, 0x4A, 0x7F],
        [0x62, 0xC2, 0x7D, 0xCB, 0x24, 0x4A, 0x7F],
        [0x62, 0xC2, 0x7D, 0xCB, 0x25, 0x4A, 0x7F],
        [0x62, 0xC2, 0x7D, 0xCB, 0x30, 0x4A, 0x7F],
        [0x62, 0xC2, 0x7D, 0xCB, 0x31, 0x4A, 0x7F],
        [0x62, 0xC2, 0x7D, 0xCB, 0x32, 0x4A, 0x7F],
        [0x62, 0xC2, 0x7D, 0xCB, 0x33, 0x4A, 0x7F],
        [0x62, 0xC2, 0x7D, 0xCB, 0x34, 0x4A, 0x7F],
        [0x62, 0xC2, 0x7D, 0xCB, 0x35, 0x4A, 0x7F],
    ];

    for (spec, bytes) in SPECS.into_iter().zip(LLVM) {
        let encoding = X86InstructionBytes::new(&bytes)
            .unwrap()
            .evex_packed_extend_memory_encoding()
            .unwrap_or_else(|| panic!("{} {bytes:02X?}", spec.name));
        assert_eq!(encoding.source_elem, spec.source_elem, "{}", spec.name);
        assert_eq!(
            encoding.destination_elem, spec.destination_elem,
            "{}",
            spec.name
        );
        assert_eq!(encoding.signed, spec.signed, "{}", spec.name);
        assert_eq!(encoding.destination, 17, "{}", spec.name);
        assert_eq!(encoding.writemask, Some(3), "{}", spec.name);
        assert!(encoding.zeroing, "{}", spec.name);
        assert_eq!(encoding.width, VecWidth::V512, "{}", spec.name);
        let X86EvexPackedExtendMemoryReplay::MaskedVector { stack_instruction } = encoding.replay
        else {
            panic!("{}: expected masked stack replay", spec.name)
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
        let valid = case(spec, false, 1, MaskControl::Zero).bytes();
        let mut mutations = Vec::new();

        let mut wrong_map = valid.clone();
        wrong_map[1] = (wrong_map[1] & !7) | 1;
        mutations.push(("map", wrong_map));
        let mut wrong_pp = valid.clone();
        wrong_pp[2] = (wrong_pp[2] & !3) | 2;
        mutations.push(("pp", wrong_pp));
        let mut vvvv = valid.clone();
        vvvv[2] &= !0x08;
        mutations.push(("vvvv", vvvv));
        let mut v_prime = valid.clone();
        v_prime[3] &= !0x08;
        mutations.push(("V'", v_prime));
        let mut broadcast = valid.clone();
        broadcast[3] |= 0x10;
        mutations.push(("EVEX.b", broadcast));
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

        if matches!(spec.opcode, 0x25 | 0x35) {
            let mut reserved_w = valid.clone();
            reserved_w[2] |= 0x80;
            mutations.push(("reserved W=1", reserved_w));
        }

        for (name, bytes) in mutations {
            assert!(
                X86InstructionBytes::new(&bytes)
                    .and_then(|instruction| instruction.evex_packed_extend_memory_encoding())
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
            &[0x64, 0x67, 0x62, 0xC2, 0x7D, 0xCB, 0x20, 0x4C, 0x8A, 0x7F],
            false,
        ),
        (
            "RIP relative",
            &[0x62, 0xE2, 0x7D, 0xCB, 0x20, 0x0D, 0xFC, 0x01, 0x00, 0x00],
            false,
        ),
        (
            "SIB",
            &[0x62, 0xC2, 0x7D, 0xCB, 0x20, 0x4C, 0x8A, 0x7F],
            false,
        ),
        ("APX B4", &[0x62, 0xFA, 0x7D, 0xC9, 0x20, 0x02], true),
        ("APX X4", &[0x62, 0xF2, 0x79, 0xC9, 0x20, 0x04, 0x8A], true),
    ];

    for &(name, bytes, needs_apx) in address_cases {
        let classified = X86InstructionBytes::new(bytes)
            .unwrap()
            .evex_packed_extend_memory_encoding()
            .unwrap_or_else(|| panic!("{name} {bytes:02X?}"));
        let X86EvexPackedExtendMemoryReplay::MaskedVector { stack_instruction } = classified.replay
        else {
            panic!("{name}: masked stack replay")
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
        case(SPECS[0], true, 2, MaskControl::None),
        case(SPECS[2], false, 0, MaskControl::Zero),
        case(SPECS[4], true, 1, MaskControl::Merge),
        case(SPECS[11], false, 2, MaskControl::Merge),
    ];

    for instruction in representatives {
        let function = optimize(lift_case(instruction), OptLevel::O2);
        let exact = sequence(&function, true).unwrap_or_else(|| panic!("{instruction:?}"));
        assert_eq!(
            replay_kind(exact),
            if instruction.control == MaskControl::None {
                "vector"
            } else {
                "masked-vector"
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

        let mut wrong_sign = function.clone();
        let sign = wrong_sign.blocks[0]
            .ops
            .iter_mut()
            .find_map(|op| match &mut op.kind {
                OpKind::VExtractLane { sign, .. } => Some(sign),
                _ => None,
            })
            .expect("extension extract");
        *sign = if *sign == SignExtend::Sign {
            SignExtend::Zero
        } else {
            SignExtend::Sign
        };
        assert_rejected("extension signedness", &wrong_sign);

        let mut wrong_hint = function.clone();
        wrong_hint.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::VExtractLane { .. }))
            .unwrap()
            .x86_hint = Some(X86OpHint::MovImmModRm);
        assert_rejected("semantic hint", &wrong_hint);

        let mut wrong_pc = function.clone();
        wrong_pc.blocks[0].ops.last_mut().unwrap().guest_pc += 1;
        assert_rejected("split guest PC", &wrong_pc);

        let source = match function.blocks[0].ops[1].kind {
            OpKind::VBroadcast { dst, .. } => dst,
            ref other => panic!("source container: {other:?}"),
        };
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

        let mut wrong_lane_address = function.clone();
        let changed = wrong_lane_address.blocks[0].ops.iter_mut().any(|op| {
            let addr = match &mut op.kind {
                OpKind::Load { addr, .. } | OpKind::PredLoad { addr, .. } => addr,
                _ => return false,
            };
            if let Address::BaseOffset { offset, .. } = addr {
                *offset += 1;
                true
            } else {
                false
            }
        });
        assert!(changed, "{instruction:?}: lane address");
        assert_rejected("lane address", &wrong_lane_address);

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

    let apx = [0x62, 0xFA, 0x7D, 0xC9, 0x20, 0x02];
    let mut missing_apx = lift_bytes(&apx);
    assert!(matches!(
        missing_apx.blocks[0].ops[0].kind,
        OpKind::X86RequireApx
    ));
    missing_apx.blocks[0].ops.remove(0);
    assert_rejected("missing APX guard", &missing_apx);
}

#[test]
fn lowerer_rejects_the_avx_only_vector_bridge() {
    let instruction = ExtendCase {
        destination: 17,
        ..case(SPECS[2], false, 2, MaskControl::Merge)
    };
    let function = lift_case(instruction);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(true);
    lowerer.set_jit_fault_deopt_guards(true);
    assert!(lowerer.lower_function(&function).is_err());
}
