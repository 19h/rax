//! Byte classification, graph provenance, and address-frontier coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, MemWidth, OpId, OpWidth, SrcOperand, VReg, VirtualId, X86Reg,
};

#[test]
fn evex_scalar_insert_rewrites_match_ten_independent_llvm_23_anchors() {
    // Memory and register encodings were independently produced by
    // llvm-mc 23.0.0git. Memory displacements are 127 compressed Tuple1
    // Scalar units. VINSERTPS clears Count_S for the register rewrite.
    let anchors: &[(&[u8], &[u8])] = &[
        (
            &[0x62, 0x43, 0x0D, 0x00, 0x21, 0x7A, 0x7F, 0xAF],
            &[0x62, 0x63, 0x0D, 0x00, 0x21, 0xF8, 0x2F],
        ),
        (
            &[0x62, 0x43, 0x0D, 0x00, 0x20, 0x7A, 0x7F, 0xAF],
            &[0x62, 0x63, 0x0D, 0x00, 0x20, 0xF8, 0xAF],
        ),
        (
            &[0x62, 0x41, 0x0D, 0x00, 0xC4, 0x7A, 0x7F, 0xAF],
            &[0x62, 0x61, 0x0D, 0x00, 0xC4, 0xF8, 0xAF],
        ),
        (
            &[0x62, 0x43, 0x0D, 0x00, 0x22, 0x7A, 0x7F, 0xAF],
            &[0x62, 0x63, 0x0D, 0x00, 0x22, 0xF8, 0xAF],
        ),
        (
            &[0x62, 0x43, 0x8D, 0x00, 0x22, 0x7A, 0x7F, 0xAF],
            &[0x62, 0x63, 0x8D, 0x00, 0x22, 0xF8, 0xAF],
        ),
    ];
    for (memory, replay) in anchors {
        let encoding = X86InstructionBytes::new(memory)
            .unwrap()
            .evex_scalar_insert_memory_encoding()
            .unwrap_or_else(|| panic!("{memory:02X?}"));
        assert_eq!(encoding.register_instruction.as_slice(), *replay);
    }
}

#[test]
fn classifier_crosses_every_shape_register_apx_axis_and_immediate() {
    let mut structural_cells = 0usize;
    for shape in SHAPES {
        for destination in 0..32u8 {
            for source1 in 0..32u8 {
                for base_high in [false, true] {
                    for index_high in [false, true] {
                        for immediate in [0x00, 0xAF, 0xFF] {
                            let case = InsertCase {
                                shape,
                                destination,
                                source1,
                                immediate,
                            };
                            let mut bytes = memory_encoding(case, true);
                            bytes[1] |= u8::from(base_high) << 3;
                            if index_high {
                                bytes[2] &= !0x04;
                            }
                            let encoding = X86InstructionBytes::new(&bytes)
                                .unwrap()
                                .evex_scalar_insert_memory_encoding()
                                .unwrap_or_else(|| panic!("{bytes:02X?}"));
                            assert_eq!(encoding.destination, destination);
                            assert_eq!(encoding.source1, source1);
                            assert_eq!(encoding.kind, shape.kind);
                            assert_eq!(encoding.immediate, immediate);
                            assert_eq!(encoding.w, shape.w);
                            assert_eq!(encoding.scratch, case.scratch());
                            assert_eq!(encoding.needs_avx512bw, shape.needs_avx512bw());
                            assert_eq!(encoding.needs_avx512dq, shape.needs_avx512dq());
                            assert_eq!(encoding.register_instruction, case.expected_replay());
                            let replay = encoding.register_instruction.as_slice();
                            assert_eq!(replay[1] & 0x08, 0, "{bytes:02X?}: APX B4");
                            assert_eq!(replay[2] & 0x04, 0x04, "{bytes:02X?}: APX X4");
                            assert_eq!(replay[5] >> 6, 3, "{bytes:02X?}");
                            structural_cells += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(structural_cells, 7 * 32 * 32 * 4 * 3);

    let mut immediate_cells = 0usize;
    for shape in SHAPES {
        for immediate in u8::MIN..=u8::MAX {
            let case = InsertCase {
                shape,
                destination: 31,
                source1: 30,
                immediate,
            };
            let encoding = X86InstructionBytes::new(&case.bytes())
                .unwrap()
                .evex_scalar_insert_memory_encoding()
                .unwrap();
            assert_eq!(encoding.immediate, immediate);
            assert_eq!(encoding.register_instruction, case.expected_replay());
            immediate_cells += 1;
        }
    }
    assert_eq!(immediate_cells, 7 * 256);
}

#[test]
fn classifier_rejects_reserved_nonowned_and_trailing_shapes() {
    let case = InsertCase {
        shape: SHAPES[0],
        destination: 31,
        source1: 30,
        immediate: 0xAF,
    };
    let valid = case.bytes();
    let mut malformed = vec![valid[..valid.len() - 1].to_vec()];
    let mut trailing = valid.clone();
    trailing.push(0);
    malformed.push(trailing);
    let mut register = valid.clone();
    register[5] |= 0xC0;
    malformed.push(register);
    for (index, mask) in [
        (1, 0x02), // map 0F3A -> non-owned map
        (2, 0x01), // mandatory prefix
        (2, 0x80), // VINSERTPS requires W0
        (3, 0x20), // reserved L'L
        (3, 0x10), // reserved EVEX.b
        (3, 0x01), // reserved writemask
        (3, 0x80), // reserved EVEX.z
        (4, 0x04), // non-owned opcode
    ] {
        let mut bytes = valid.clone();
        bytes[index] ^= mask;
        malformed.push(bytes);
    }
    let mut forbidden_legacy = valid.clone();
    forbidden_legacy.insert(0, 0x66);
    malformed.push(forbidden_legacy);
    for bytes in malformed {
        assert!(
            X86InstructionBytes::new(&bytes)
                .unwrap()
                .evex_scalar_insert_memory_encoding()
                .is_none(),
            "{bytes:02X?}"
        );
    }

    for shape in [SHAPES[1], SHAPES[3]] {
        let low = InsertCase {
            shape: InsertShape { w: false, ..shape },
            destination: 17,
            source1: 18,
            immediate: 0xA5,
        };
        let high = InsertCase {
            shape: InsertShape { w: true, ..shape },
            ..low
        };
        assert!(
            X86InstructionBytes::new(&low.bytes())
                .unwrap()
                .evex_scalar_insert_memory_encoding()
                .is_some()
        );
        assert!(
            X86InstructionBytes::new(&high.bytes())
                .unwrap()
                .evex_scalar_insert_memory_encoding()
                .is_some()
        );
    }

    let mut prefixed = vec![0x64, 0x67];
    prefixed.extend_from_slice(&valid);
    assert!(
        X86InstructionBytes::new(&prefixed)
            .unwrap()
            .evex_scalar_insert_memory_encoding()
            .is_some()
    );
}

#[test]
fn matcher_rejects_graph_provenance_width_address_and_escape_mutations() {
    let case = InsertCase {
        shape: SHAPES[2],
        destination: 17,
        source1: 18,
        immediate: 0xAF,
    };
    let base = optimize(lift_case(case), OptLevel::O2);
    assert!(sequence(&base, true).is_some());

    let mut wrong_width = base.clone();
    let OpKind::Load { width, .. } = &mut wrong_width.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *width = MemWidth::B2;
    assert!(sequence(&wrong_width, true).is_none());

    let mut wrong_address = base.clone();
    let OpKind::Load { addr, .. } = &mut wrong_address.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *addr = Address::GpRel { offset: 1 };
    assert!(sequence(&wrong_address, true).is_none());

    let mut wrong_destination = base.clone();
    let final_op = wrong_destination.blocks[0].ops.last_mut().unwrap();
    let OpKind::VMov { dst, .. } = &mut final_op.kind else {
        unreachable!()
    };
    *dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(19)));
    assert!(sequence(&wrong_destination, true).is_none());

    let mut wrong_provenance = base.clone();
    wrong_provenance.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(
            &InsertCase {
                destination: 19,
                ..case
            }
            .bytes(),
        )
        .unwrap(),
    );
    assert!(sequence(&wrong_provenance, true).is_none());

    let loaded = match base.blocks[0].ops[0].kind {
        OpKind::Load { dst, .. } => dst,
        _ => unreachable!(),
    };
    let mut escaped = base.clone();
    escaped.blocks[0].ops.push(SmirOp::new(
        OpId(u16::MAX),
        PC + 1,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(u32::MAX)),
            src: SrcOperand::Reg(loaded),
            width: OpWidth::W64,
        },
    ));
    assert!(sequence(&escaped, true).is_none());

    let mut same_pc_head = base;
    same_pc_head.blocks[0]
        .ops
        .insert(0, SmirOp::new(OpId(0x7FFE), PC, OpKind::Nop));
    assert!(sequence(&same_pc_head, true).is_none());
}

#[test]
fn segment_addr32_rip_and_apx_addresses_admit_and_lower() {
    let case = InsertCase {
        shape: SHAPES[6],
        destination: 31,
        source1: 30,
        immediate: 0x81,
    };
    let mut rip = case.bytes();
    let immediate = rip.pop().unwrap();
    rip[5] = (rip[5] & 0x38) | 5;
    rip.extend_from_slice(&0x20i32.to_le_bytes());
    rip.push(immediate);
    let mut addr32 = case.bytes();
    addr32.insert(0, 0x67);
    let mut fs = case.bytes();
    fs.insert(0, 0x64);
    for (name, bytes) in [("RIP", rip), ("addr32", addr32), ("FS", fs)] {
        let base = lift_bytes(&bytes);
        for level in LEVELS {
            let function = optimize(base.clone(), level);
            sequence(&function, true)
                .unwrap_or_else(|| panic!("{name} {level:?}: {:#?}", function.blocks[0].ops));
            lower(&function, case);
        }
    }

    let mut apx = memory_encoding(case, true);
    apx[1] |= 0x08;
    apx[2] &= !0x04;
    let base = lift_bytes(&apx);
    assert!(matches!(base.blocks[0].ops[0].kind, OpKind::X86RequireApx));
    let mut missing_guard = base.clone();
    missing_guard.blocks[0].ops.remove(0);
    assert!(sequence(&missing_guard, true).is_none());
    for level in LEVELS {
        let function = optimize(base.clone(), level);
        sequence(&function, true)
            .unwrap_or_else(|| panic!("APX {level:?}: {:#?}", function.blocks[0].ops));
        lower(&function, case);
    }
}
