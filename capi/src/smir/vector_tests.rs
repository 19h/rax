use super::{tests::lift, *};

#[test]
fn movdq_retains_direction_width_and_distinct_xmm_bank() {
    for (bytes, width, to_vector, index) in [
        (vec![0x66, 0x0f, 0x6e, 0xc2], 32, true, 0),
        (vec![0x66, 0x48, 0x0f, 0x6e, 0xc2], 64, true, 0),
        (vec![0x66, 0x4c, 0x0f, 0x6e, 0xc2], 64, true, 8),
        (vec![0x66, 0x0f, 0x7e, 0xc2], 32, false, 0),
        (vec![0x66, 0x48, 0x0f, 0x7e, 0xc2], 64, false, 0),
    ] {
        let (_, ops) = lift(&bytes);
        let op = ops
            .iter()
            .find(|op| op.opcode == RAX_SMIR_X86_MOVDQ)
            .unwrap();
        assert_eq!(op.width_bits, width);
        let (vector, scalar) = if to_vector {
            (op.dst, op.a)
        } else {
            (op.a, op.dst)
        };
        assert_eq!((vector.kind, vector.id), (4, index));
        assert_eq!((scalar.kind, scalar.id), (2, 2));
    }
}

#[test]
fn movlhps_snapshots_lane_before_aliasing_insert() {
    for bytes in [&[0x0f, 0x16, 0xc0][..], &[0x45, 0x0f, 0x16, 0xc0][..]] {
        let (_, ops) = lift(bytes);
        let extract = ops
            .iter()
            .position(|op| op.opcode == RAX_SMIR_VEXTRACT)
            .unwrap();
        let insert = ops
            .iter()
            .position(|op| op.opcode == RAX_SMIR_VINSERT)
            .unwrap();
        assert!(extract < insert);
        assert_eq!(ops[extract].auxiliary, 0);
        assert_eq!(ops[insert].auxiliary, 1);
        assert_eq!(ops[extract].source_width_bits, 64);
        assert_eq!(ops[insert].source_width_bits, 64);
        assert_eq!(ops[extract].dst, ops[insert].b);
        assert_eq!(ops[extract].a, ops[insert].dst);
    }
}

#[test]
fn packed_memory_keeps_full_width_and_ordered_alignment() {
    for (bytes, opcode, aligned) in [
        (vec![0x0f, 0x10, 0x01], RAX_SMIR_VLOAD, false),
        (vec![0x0f, 0x11, 0x01], RAX_SMIR_VSTORE, false),
        (vec![0x0f, 0x28, 0x01], RAX_SMIR_VLOAD, true),
        (vec![0x0f, 0x29, 0x01], RAX_SMIR_VSTORE, true),
        (vec![0x66, 0x0f, 0x6f, 0x01], RAX_SMIR_VLOAD, true),
        (vec![0xf3, 0x0f, 0x7f, 0x01], RAX_SMIR_VSTORE, false),
    ] {
        let (_, ops) = lift(&bytes);
        let memory = ops.iter().position(|op| op.opcode == opcode).unwrap();
        assert_eq!(ops[memory].width_bits, 128);
        assert_eq!(
            (ops[memory].address.base.kind, ops[memory].address.base.id),
            (2, 1)
        );
        let check = ops
            .iter()
            .position(|op| op.opcode == RAX_SMIR_CHECK_ALIGNMENT);
        assert_eq!(check.is_some(), aligned);
        if let Some(check) = check {
            assert!(check < memory);
            assert_eq!(ops[check].auxiliary, 16);
        }
    }
}

#[test]
fn scalar_memory_moves_retain_zero_broadcast_and_xmm_lanes() {
    let (_, movss) = lift(&[0xf3, 0x0f, 0x10, 0x01]);
    let inserts: Vec<_> = movss
        .iter()
        .filter(|op| op.opcode == RAX_SMIR_VINSERT)
        .collect();
    assert_eq!(inserts.len(), 4);
    assert!(
        inserts
            .iter()
            .all(|op| op.dst.kind == 4 && op.source_width_bits == 32)
    );
    assert!(inserts[1..].iter().all(|op| op.b == inserts[1].b));
    assert!(movss.iter().any(|op| op.opcode == RAX_SMIR_MOV
        && op.a.kind == 3
        && op.a.value == 0
        && op.dst == inserts[1].b));
    let (_, movq) = lift(&[0xf3, 0x0f, 0x7e, 0x01]);
    let broadcast = movq
        .iter()
        .find(|op| op.opcode == RAX_SMIR_VBROADCAST)
        .unwrap();
    assert_eq!(broadcast.dst.kind, 1);
    assert_eq!(broadcast.auxiliary, 1);
    assert!(
        movq.iter()
            .any(|op| op.opcode == RAX_SMIR_VINSERT && op.dst.kind == 4)
    );
}

#[test]
fn vector_capacity_and_unsupported_encodings_publish_no_prefix() {
    let sentinel = RaxSmirOp {
        opcode: 0xfeed,
        ..Default::default()
    };
    let mut ops = [sentinel; 16];
    let mut out = RaxSmirInstruction::default();
    let aligned = [0x0fu8, 0x29, 0x01];
    assert_eq!(
        rax_smir_lift_x64(
            0,
            aligned.as_ptr().cast(),
            aligned.len(),
            &mut out,
            ops.as_mut_ptr(),
            1
        ),
        RaxStatus::Bounds
    );
    assert_eq!(out.op_count, 0);
    assert_eq!(ops, [sentinel; 16]);
    for bytes in [
        vec![0xc5u8, 0xf8, 0x10, 0xc1],
        vec![0xc5u8, 0xfc, 0x10, 0xc1],
        vec![0x0f, 0x6f, 0xc1],
        vec![0x0f, 0x58, 0xc1],
        vec![0x62, 0xf1, 0x7c, 0x48, 0x10, 0xc1],
    ] {
        assert_eq!(
            rax_smir_lift_x64(
                0,
                bytes.as_ptr().cast(),
                bytes.len(),
                &mut out,
                ops.as_mut_ptr(),
                ops.len()
            ),
            RaxStatus::Unsupported
        );
        assert_eq!((out.flags, out.op_count), (RAX_SMIR_UNSUPPORTED, 0));
        assert_eq!(ops, [sentinel; 16]);
    }
}
