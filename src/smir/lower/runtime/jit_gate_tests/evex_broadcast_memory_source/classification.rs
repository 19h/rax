//! Exhaustive encoding, graph, feature, and lowering admission checks.

use super::*;
use crate::smir::ir::ops::{SmirOp, X86OpHint};
use crate::smir::ir::types::{Address, OpId, OpWidth, SrcOperand, VirtualId};

fn case(shape: Shape, control: MaskControl) -> BroadcastMemoryCase {
    BroadcastMemoryCase {
        shape,
        destination: 1,
        base: 2,
        control,
    }
}

#[test]
fn all_102_legal_shapes_lift_optimize_admit_and_lower_at_o0_o1_o2() {
    let mut encodings = 0usize;
    let mut optimized_graphs = 0usize;
    for mut instruction in all_cases() {
        instruction.destination = [0, 8, 17, 31][encodings & 3];
        let bytes = instruction.bytes();
        let classified = X86InstructionBytes::new(&bytes)
            .unwrap()
            .evex_broadcast_memory_encoding()
            .unwrap_or_else(|| panic!("{instruction:?} {bytes:02X?}"));
        assert_eq!(classified.width, instruction.shape.width);
        assert_eq!(classified.elem, instruction.shape.elem);
        assert_eq!(classified.source_lanes, instruction.shape.source_lanes);
        assert_eq!(classified.destination, instruction.destination);
        assert_eq!(
            classified.writemask,
            (instruction.mask() != 0).then_some(instruction.mask())
        );
        assert_eq!(classified.zeroing, instruction.zeroing());
        assert_eq!(classified.opcode, instruction.shape.opcode);
        assert_eq!(classified.w, instruction.shape.w);
        assert_eq!(classified.memory_size, instruction.shape.memory_size());
        assert_eq!(
            classified.stack_instruction.as_slice(),
            instruction.stack_instruction()
        );
        assert_eq!(
            classified.needs_avx512vl,
            instruction.shape.width != VecWidth::V512
        );
        assert_eq!(classified.needs_avx512bw, instruction.shape.needs_avx512bw);
        assert_eq!(classified.needs_avx512dq, instruction.shape.needs_avx512dq);

        for level in LEVELS {
            let function = optimize(lift_case(instruction), level);
            let exact = sequence(&function, true)
                .unwrap_or_else(|| panic!("{level:?} {instruction:?} {bytes:02X?}"));
            assert_eq!(exact.encoding, classified, "{level:?} {instruction:?}");
            assert_eq!(exact.consumed, function.blocks[0].ops.len());
            let expected_address_offset = function.blocks[0]
                .ops
                .iter()
                .position(|op| matches!(op.kind, OpKind::Lea { .. }))
                .expect("broadcast address LEA");
            assert_eq!(exact.address_offset, expected_address_offset);
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
    assert_eq!(encodings, 34 * 3);
    assert_eq!(optimized_graphs, encodings * LEVELS.len());
}

#[test]
fn classifier_owns_exactly_the_34_map0f38_opcode_pp_w_ll_selectors() {
    let template = case(SHAPES[0], MaskControl::None).bytes();
    let mut accepted = 0usize;
    for map in 0..=7u8 {
        for opcode in u8::MIN..=u8::MAX {
            for pp in 0..=3u8 {
                for w in [false, true] {
                    for ll in 0..=3u8 {
                        let mut bytes = template;
                        bytes[1] = (bytes[1] & !7) | map;
                        bytes[2] = (bytes[2] & !(0x80 | 3)) | (u8::from(w) << 7) | pp;
                        bytes[3] = (bytes[3] & !0x60) | (ll << 5);
                        bytes[4] = opcode;
                        let expected = map == 2
                            && pp == 1
                            && SHAPES.iter().any(|shape| {
                                (shape.opcode, shape.w, shape.ll()) == (opcode, w, ll)
                            });
                        let actual = X86InstructionBytes::new(&bytes)
                            .unwrap()
                            .evex_broadcast_memory_encoding()
                            .is_some();
                        assert_eq!(actual, expected, "{bytes:02X?}");
                        accepted += usize::from(actual);
                    }
                }
            }
        }
    }
    assert_eq!(accepted, SHAPES.len());
}

#[test]
fn all_16_instruction_families_match_independent_llvm_23_anchors() {
    // llvm-mc 23.0.0git, Intel syntax: ZMM17{k3}{z}, R10 plus the maximum
    // tuple-scaled positive disp8 (127). These anchors independently cover
    // every architectural mnemonic, including VBROADCASTI64X4, which is not
    // represented by the external scanner used for the rolling gap census.
    const LLVM: [([u8; 7], VecElementType, u8); 16] = [
        (
            [0x62, 0xC2, 0x7D, 0xCB, 0x18, 0x4A, 0x7F],
            VecElementType::F32,
            1,
        ),
        (
            [0x62, 0xC2, 0xFD, 0xCB, 0x19, 0x4A, 0x7F],
            VecElementType::F64,
            1,
        ),
        (
            [0x62, 0xC2, 0x7D, 0xCB, 0x19, 0x4A, 0x7F],
            VecElementType::F32,
            2,
        ),
        (
            [0x62, 0xC2, 0x7D, 0xCB, 0x1A, 0x4A, 0x7F],
            VecElementType::F32,
            4,
        ),
        (
            [0x62, 0xC2, 0xFD, 0xCB, 0x1A, 0x4A, 0x7F],
            VecElementType::F64,
            2,
        ),
        (
            [0x62, 0xC2, 0x7D, 0xCB, 0x1B, 0x4A, 0x7F],
            VecElementType::F32,
            8,
        ),
        (
            [0x62, 0xC2, 0xFD, 0xCB, 0x1B, 0x4A, 0x7F],
            VecElementType::F64,
            4,
        ),
        (
            [0x62, 0xC2, 0x7D, 0xCB, 0x78, 0x4A, 0x7F],
            VecElementType::I8,
            1,
        ),
        (
            [0x62, 0xC2, 0x7D, 0xCB, 0x79, 0x4A, 0x7F],
            VecElementType::I16,
            1,
        ),
        (
            [0x62, 0xC2, 0x7D, 0xCB, 0x58, 0x4A, 0x7F],
            VecElementType::I32,
            1,
        ),
        (
            [0x62, 0xC2, 0xFD, 0xCB, 0x59, 0x4A, 0x7F],
            VecElementType::I64,
            1,
        ),
        (
            [0x62, 0xC2, 0x7D, 0xCB, 0x59, 0x4A, 0x7F],
            VecElementType::I32,
            2,
        ),
        (
            [0x62, 0xC2, 0x7D, 0xCB, 0x5A, 0x4A, 0x7F],
            VecElementType::I32,
            4,
        ),
        (
            [0x62, 0xC2, 0xFD, 0xCB, 0x5A, 0x4A, 0x7F],
            VecElementType::I64,
            2,
        ),
        (
            [0x62, 0xC2, 0x7D, 0xCB, 0x5B, 0x4A, 0x7F],
            VecElementType::I32,
            8,
        ),
        (
            [0x62, 0xC2, 0xFD, 0xCB, 0x5B, 0x4A, 0x7F],
            VecElementType::I64,
            4,
        ),
    ];

    for (bytes, elem, source_lanes) in LLVM {
        let classified = X86InstructionBytes::new(&bytes)
            .unwrap()
            .evex_broadcast_memory_encoding()
            .unwrap_or_else(|| panic!("{bytes:02X?}"));
        assert_eq!(classified.width, VecWidth::V512, "{bytes:02X?}");
        assert_eq!(classified.elem, elem, "{bytes:02X?}");
        assert_eq!(classified.source_lanes, source_lanes, "{bytes:02X?}");
        assert_eq!(classified.destination, 17, "{bytes:02X?}");
        assert_eq!(classified.writemask, Some(3), "{bytes:02X?}");
        assert!(classified.zeroing, "{bytes:02X?}");
        assert_eq!(
            classified.memory_size,
            u32::from(source_lanes) * elem.bytes(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn reserved_fields_register_sources_incomplete_and_trailing_bytes_fail_closed() {
    for shape in SHAPES {
        let valid = case(shape, MaskControl::Zero).bytes().to_vec();
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
        let mut embedded_control = valid.clone();
        embedded_control[3] |= 0x10;
        mutations.push(("EVEX.b", embedded_control));
        let mut ll3 = valid.clone();
        ll3[3] = (ll3[3] & !0x60) | 0x60;
        mutations.push(("LL=3", ll3));
        let mut zeroing_k0 = valid.clone();
        zeroing_k0[3] &= !7;
        mutations.push(("z with k0", zeroing_k0));
        let mut register = valid.clone();
        register[5] |= 0xC0;
        mutations.push(("register source", register));
        let mut trailing = valid.clone();
        trailing.push(0x90);
        mutations.push(("trailing byte", trailing));
        let mut forbidden_legacy = vec![0xF3];
        forbidden_legacy.extend_from_slice(&valid);
        mutations.push(("forbidden legacy prefix", forbidden_legacy));
        mutations.push(("incomplete ModR/M", valid[..5].to_vec()));

        for (name, bytes) in mutations {
            assert!(
                X86InstructionBytes::new(&bytes)
                    .and_then(|instruction| instruction.evex_broadcast_memory_encoding())
                    .is_none(),
                "{shape:?} {name} {bytes:02X?}"
            );
        }
    }
}

#[test]
fn segment_addr32_sib_rip_relative_and_apx_addresses_preserve_helper_provenance() {
    let address_cases: &[(&str, &[u8], bool)] = &[
        (
            "FS addr32 SIB",
            &[0x64, 0x67, 0x62, 0xC2, 0x7D, 0xCB, 0x1A, 0x4C, 0x8A, 0x7F],
            false,
        ),
        (
            "RIP relative",
            &[0x62, 0xE2, 0x7D, 0xCB, 0x1A, 0x0D, 0xFC, 0x01, 0x00, 0x00],
            false,
        ),
        (
            "SIB",
            &[0x62, 0xC2, 0x7D, 0xCB, 0x1A, 0x4C, 0x8A, 0x7F],
            false,
        ),
        ("APX B4", &[0x62, 0xFA, 0x7D, 0xC9, 0x18, 0x02], true),
        ("APX X4", &[0x62, 0xF2, 0x79, 0xC9, 0x18, 0x04, 0x8A], true),
    ];

    for &(name, bytes, needs_apx) in address_cases {
        let classified = X86InstructionBytes::new(bytes)
            .unwrap()
            .evex_broadcast_memory_encoding()
            .unwrap_or_else(|| panic!("{name} {bytes:02X?}"));
        assert_eq!(classified.stack_instruction.as_slice()[1] & 0x68, 0x60);
        assert_ne!(classified.stack_instruction.as_slice()[2] & 0x04, 0);

        for level in LEVELS {
            let function = optimize(function_from_bytes(bytes), level);
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
fn sequence_fails_closed_for_provenance_graph_ssa_address_and_frontier_mutations() {
    let representatives = [
        case(SHAPES[0], MaskControl::None),
        case(SHAPES[11], MaskControl::Merge),
        case(SHAPES[27], MaskControl::Zero),
    ];

    for instruction in representatives {
        let function = optimize(lift_case(instruction), OptLevel::O2);
        let exact = sequence(&function, true).unwrap_or_else(|| panic!("{instruction:?}"));

        let mut missing_provenance = function.clone();
        missing_provenance.x86_instruction_bytes.clear();
        assert_rejected("missing provenance", &missing_provenance);

        let mut wrong_provenance = function.clone();
        let mut bytes = instruction.bytes();
        bytes[4] = 0x17;
        wrong_provenance
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&bytes).unwrap());
        assert_rejected("wrong provenance", &wrong_provenance);

        let mut wrong_hint = function.clone();
        wrong_hint.blocks[0]
            .ops
            .iter_mut()
            .find(|op| matches!(op.kind, OpKind::Load { .. } | OpKind::PredLoad { .. }))
            .unwrap()
            .x86_hint = Some(X86OpHint::MovImmModRm);
        assert_rejected("memory hint", &wrong_hint);

        let mut wrong_pc = function.clone();
        wrong_pc.blocks[0].ops.last_mut().unwrap().guest_pc += 1;
        assert_rejected("split guest PC", &wrong_pc);

        let mut wrong_lane_address = function.clone();
        let changed = wrong_lane_address.blocks[0].ops.iter_mut().any(|op| {
            let address = match &mut op.kind {
                OpKind::Load { addr, .. } | OpKind::PredLoad { addr, .. } => addr,
                _ => return false,
            };
            if let Address::BaseOffset { offset, .. } = address {
                *offset += 1;
                true
            } else {
                false
            }
        });
        assert!(changed, "{instruction:?}: lane address");
        assert_rejected("lane address", &wrong_lane_address);

        let base = match function.blocks[0].ops[exact.address_offset].kind {
            OpKind::Lea { dst, .. } => dst,
            ref other => panic!("address op: {other:?}"),
        };
        let mut escaped_base = function.clone();
        escaped_base.blocks[0].ops.push(SmirOp::new(
            OpId(0xFF00),
            PC + 1,
            OpKind::Mov {
                dst: VReg::Virtual(VirtualId(0xFF00)),
                src: SrcOperand::Reg(base),
                width: OpWidth::W64,
            },
        ));
        assert_rejected("address base escapes", &escaped_base);

        let mut duplicate_base = function.clone();
        duplicate_base.blocks[0].ops.push(SmirOp::new(
            OpId(0xFF01),
            PC + 1,
            OpKind::Mov {
                dst: base,
                src: SrcOperand::Imm(0),
                width: OpWidth::W64,
            },
        ));
        assert_rejected("address base redefined", &duplicate_base);

        let mut wrong_insert_lane = function.clone();
        let lane = wrong_insert_lane.blocks[0]
            .ops
            .iter_mut()
            .find_map(|op| match &mut op.kind {
                OpKind::VInsertLane { lane, .. } => Some(lane),
                _ => None,
            })
            .expect("source insertion");
        *lane ^= 1;
        assert_rejected("source insertion lane", &wrong_insert_lane);

        if instruction.mask() != 0 {
            let mut wrong_predicate = function.clone();
            let applicable = wrong_predicate.blocks[0]
                .ops
                .iter_mut()
                .find_map(|op| match &mut op.kind {
                    OpKind::And {
                        src1: VReg::Arch(ArchReg::X86(X86Reg::K(3))),
                        src2: SrcOperand::Imm(bits),
                        ..
                    } => Some(bits),
                    _ => None,
                })
                .expect("aggregate mask predicate");
            *applicable ^= 1;
            assert_rejected("aggregate predicate mask", &wrong_predicate);
        }

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

    let apx = [0x62, 0xF2, 0x79, 0xC9, 0x18, 0x04, 0x8A];
    let mut missing_apx = function_from_bytes(&apx);
    assert!(matches!(
        missing_apx.blocks[0].ops[0].kind,
        OpKind::X86RequireApx
    ));
    missing_apx.blocks[0].ops.remove(0);
    assert_rejected("missing APX guard", &missing_apx);
}

#[test]
fn lowerer_rejects_the_avx_only_vector_bridge() {
    let instruction = BroadcastMemoryCase {
        shape: SHAPES[11],
        destination: 17,
        base: 2,
        control: MaskControl::Merge,
    };
    let function = lift_case(instruction);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_native_vector_state_active(true);
    lowerer.set_avx_ymm16_vector_state(true);
    lowerer.set_jit_fault_deopt_guards(true);
    assert!(lowerer.lower_function(&function).is_err());
}

#[test]
fn architectural_destination_register_width_matches_every_shape() {
    for shape in SHAPES {
        for destination in [0, 15, 16, 31] {
            assert_eq!(
                vector(destination, shape.width),
                match shape.width {
                    VecWidth::V128 => VReg::Arch(ArchReg::X86(X86Reg::Xmm(destination))),
                    VecWidth::V256 => VReg::Arch(ArchReg::X86(X86Reg::Ymm(destination))),
                    VecWidth::V512 => VReg::Arch(ArchReg::X86(X86Reg::Zmm(destination))),
                    _ => unreachable!(),
                }
            );
        }
    }
}
