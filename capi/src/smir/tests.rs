use super::*;

fn lift(bytes: &[u8]) -> (RaxSmirInstruction, Vec<RaxSmirOp>) {
    let mut out = RaxSmirInstruction::default();
    assert_eq!(
        rax_smir_lift_x64(
            0x1000,
            bytes.as_ptr().cast(),
            bytes.len(),
            &mut out,
            std::ptr::null_mut(),
            0
        ),
        RaxStatus::Ok
    );
    assert_eq!(out.flags, RAX_SMIR_COMPLETE);
    let mut ops = vec![RaxSmirOp::default(); out.required_op_count as usize];
    // A zero-op instruction still needs a non-null array to exercise fill.
    let mut sentinel = RaxSmirOp::default();
    let ptr = if ops.is_empty() {
        &mut sentinel
    } else {
        ops.as_mut_ptr()
    };
    assert_eq!(
        rax_smir_lift_x64(
            0x1000,
            bytes.as_ptr().cast(),
            bytes.len(),
            &mut out,
            ptr,
            ops.len()
        ),
        RaxStatus::Ok
    );
    assert_eq!(out.op_count as usize, ops.len());
    (out, ops)
}

#[test]
fn scalar_operands_and_flags_are_lossless() {
    let (out, ops) = lift(&[0x48, 0x01, 0xd8]); // add rax,rbx
    assert_eq!(out.bytes_consumed, 3);
    let add = ops.iter().find(|o| o.opcode == RAX_SMIR_ADD).unwrap();
    assert_eq!((add.a.kind, add.a.id, add.b.kind, add.b.id), (2, 0, 2, 3));
    assert_eq!((add.width_bits, add.flags_mask), (64, 63));
    assert!(ops.iter().all(|o| o.guest_pc == 0x1000
        && o.struct_size as usize == size_of::<RaxSmirOp>()
        && o.abi_version == 1));
}

#[test]
fn every_scalar_width_and_partial_register_forms_survive() {
    for bytes in [
        vec![0x88, 0xe0],
        vec![0x66, 0x01, 0xd8],
        vec![0x01, 0xd8],
        vec![0x48, 0x01, 0xd8],
        vec![0x48, 0x0f, 0xc8],
        vec![0x48, 0xc1, 0xc0, 0x11],
        vec![0x48, 0x0f, 0xaf, 0xc3],
    ] {
        let (_, ops) = lift(&bytes);
        assert!(!ops.is_empty());
        assert!(ops.iter().all(|o| matches!(o.width_bits, 8 | 16 | 32 | 64)));
    }
}

#[test]
fn address_size_segment_and_branch_targets_are_explicit() {
    let (_, ops) = lift(&[0x65, 0x67, 0x48, 0x8b, 0x44, 0x8b, 0x10]);
    let load = ops.iter().find(|o| o.opcode == RAX_SMIR_LOAD).unwrap();
    assert_eq!(
        (
            load.address.address_bits,
            load.address.segment.id,
            load.address.scale,
            load.address.displacement
        ),
        (32, 35, 4, 16)
    );
    let (out, _) = lift(&[0x75, 0xf0]);
    assert_eq!(
        (out.flow, out.condition, out.target, out.fallthrough),
        (2, 1, 0xff2, 0x1002)
    );
    let (out, _) = lift(&[0xe8, 0x10, 0, 0, 0]);
    assert_eq!((out.flow, out.target, out.fallthrough), (6, 0x1015, 0x1005));
}

#[test]
fn truncation_never_publishes_an_executable_prefix() {
    let bytes = [0x48, 0x01, 0xd8];
    let sentinel = RaxSmirOp {
        opcode: 0xdeadbeef,
        ..Default::default()
    };
    let mut storage = [sentinel; 3];
    let mut out = RaxSmirInstruction::default();
    assert_eq!(
        rax_smir_lift_x64(
            0,
            bytes.as_ptr().cast(),
            bytes.len(),
            &mut out,
            storage.as_mut_ptr(),
            0
        ),
        RaxStatus::Bounds
    );
    assert_eq!(out.flags, RAX_SMIR_TRUNCATED);
    assert_eq!(out.op_count, 0);
    assert!(out.required_op_count > 0);
    assert_eq!(storage, [sentinel; 3]);
}

#[test]
fn unsupported_and_invalid_inputs_fail_closed() {
    let mut out = RaxSmirInstruction::default();
    let mut ops = [RaxSmirOp::default(); 8];
    // SSE output cannot be silently projected into scalar registers.
    let simd = [0x0f, 0x57, 0xc0];
    assert_eq!(
        rax_smir_lift_x64(
            0,
            simd.as_ptr().cast(),
            simd.len(),
            &mut out,
            ops.as_mut_ptr(),
            8
        ),
        RaxStatus::Unsupported
    );
    assert_eq!((out.flags, out.op_count), (RAX_SMIR_UNSUPPORTED, 0));
    for len in [0, 16, usize::MAX] {
        assert_eq!(
            rax_smir_lift_x64(
                0,
                simd.as_ptr().cast(),
                len,
                &mut out,
                std::ptr::null_mut(),
                0
            ),
            RaxStatus::Arg
        );
    }
    assert_eq!(
        rax_smir_lift_x64(
            0,
            simd.as_ptr().cast(),
            3,
            &mut out,
            std::ptr::null_mut(),
            1
        ),
        RaxStatus::Arg
    );
    assert_eq!(
        rax_smir_lift_x64(
            0,
            simd.as_ptr().cast(),
            3,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0
        ),
        RaxStatus::Arg
    );
}

#[test]
fn c_layout_has_fixed_width_fields() {
    assert_eq!(size_of::<RaxSmirOperand>(), 16);
    assert_eq!(size_of::<RaxSmirAddress>(), 64);
    assert_eq!(size_of::<RaxSmirOp>(), 184);
    assert_eq!(size_of::<RaxSmirInstruction>(), 136);
}

#[test]
fn legacy_high_byte_extension_keeps_its_semantic_hint() {
    let (_, ops) = lift(&[0x0f, 0xb6, 0xc4]);
    let ext = ops
        .iter()
        .find(|o| o.opcode == RAX_SMIR_ZERO_EXTEND)
        .unwrap();
    assert_eq!(ext.auxiliary & 0x100, 0x100);
    assert_eq!(ext.a.id, 0);
    assert_eq!(ext.source_width_bits, 8);
}

#[test]
fn scalar_bit_test_family_has_explicit_carry_and_operands() {
    for (bytes, opcode, width, register, index) in [
        (
            vec![0x48, 0x0f, 0xba, 0xf8, 0x19],
            RAX_SMIR_BTC,
            64,
            0,
            0x19,
        ),
        (vec![0x66, 0x0f, 0xba, 0xe0, 0x0f], RAX_SMIR_BT, 16, 0, 0xf),
        (vec![0x0f, 0xba, 0xe8, 0x1f], RAX_SMIR_BTS, 32, 0, 0x1f),
        (
            vec![0x48, 0x0f, 0xba, 0xf3, 0x3f],
            RAX_SMIR_BTR,
            64,
            3,
            0x3f,
        ),
    ] {
        let (_, ops) = lift(&bytes);
        let bit = ops.iter().find(|op| op.opcode == opcode).unwrap();
        assert_eq!(
            (
                bit.width_bits,
                bit.flags_mask,
                bit.a.kind,
                bit.a.id,
                bit.b.kind,
                bit.b.value
            ),
            (width, 1, 2, register, 3, index)
        );
        assert_eq!(bit.dst.kind, if opcode == RAX_SMIR_BT { 0 } else { 2 });
    }
}

#[test]
fn memory_bit_test_preserves_accesses_and_rejects_atomic_projection() {
    let (_, ops) = lift(&[0x48, 0x0f, 0xba, 0x28, 3]); // bts qword [rax],3
    assert!(ops.iter().any(|op| op.opcode == RAX_SMIR_LOAD));
    assert!(
        ops.iter()
            .any(|op| op.opcode == RAX_SMIR_BT && op.flags_mask == 1)
    );
    assert!(ops.iter().any(|op| op.opcode == RAX_SMIR_STORE));
    let load = ops
        .iter()
        .position(|op| op.opcode == RAX_SMIR_LOAD)
        .unwrap();
    let store = ops
        .iter()
        .position(|op| op.opcode == RAX_SMIR_STORE)
        .unwrap();
    let flags = ops.iter().position(|op| op.opcode == RAX_SMIR_BT).unwrap();
    assert!(
        load < store && store < flags,
        "CF must commit after the faultable store"
    );
    let input = [0xf0, 0x48, 0x0f, 0xba, 0x38, 3]; // lock btc qword [rax],3
    let mut result = RaxSmirInstruction::default();
    let mut out = [RaxSmirOp::default(); 64];
    assert_eq!(
        rax_smir_lift_x64(
            0x1000,
            input.as_ptr().cast(),
            input.len(),
            &mut result,
            out.as_mut_ptr(),
            out.len()
        ),
        RaxStatus::Unsupported
    );
    assert_eq!((result.flags, result.op_count), (RAX_SMIR_UNSUPPORTED, 0));
}
