//! Fail-closed native admission for MOVQ2DQ/MOVDQ2Q.

use super::*;

fn mm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Mm(index)))
}

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn transfer(dst: VReg, src: VReg, prefix: X86SsePrefix) -> crate::smir::ir::ops::SmirOp {
    crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(1),
        0x1000,
        OpKind::X86MovdQ {
            dst,
            src,
            width: OpWidth::W64,
            zero_upper: false,
        },
        X86OpHint::SseOp {
            prefix,
            opcode: 0xD6,
        },
    )
}

fn paired_function(
    ops: impl IntoIterator<Item = crate::smir::ir::ops::SmirOp>,
) -> crate::smir::ir::SmirFunction {
    let ops = ops.into_iter().collect::<Vec<_>>();
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for op in &ops {
        builder.push_op(
            op.guest_pc,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            },
        );
        builder.push_op(op.guest_pc, op.kind.clone());
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    for (index, op) in ops.into_iter().enumerate() {
        function.blocks[0].ops[index * 2 + 1].x86_hint = op.x86_hint;
    }
    function
}

#[test]
fn every_encodable_register_pair_is_mmx_admitted_and_xmm_state_backed() {
    let mut probes = 0usize;
    for xmm_index in 0..16 {
        for mm_index in 0..8 {
            for op in [
                transfer(xmm(xmm_index), mm(mm_index), X86SsePrefix::Rep),
                transfer(mm(mm_index), xmm(xmm_index), X86SsePrefix::Repne),
            ] {
                assert!(
                    crate::smir::lower::x86_64::x86_mmx_xmm_transfer_shape_valid(&op),
                    "{op:?}"
                );
                assert!(is_x86_native_mmx_op(&op), "{op:?}");
                assert!(!x86_native_mmx_op_requires_ssse3(&op));
                assert!(!x86_native_vector_smir_op(&op));

                let function = paired_function([op]);
                let excluded = std::collections::HashMap::new();
                assert!(is_native_clobber_safe_excluding(
                    &function, &excluded, false
                ));
                assert!(x86_native_mmx_pairs_valid_excluding(&function, &excluded));
                assert!(uses_x86_native_mmx_excluding(&function, &excluded));
                assert!(uses_x86_xmm_state_excluding(&function, &excluded));
                assert!(!uses_x86_native_vectors_excluding(&function, &excluded));
                assert!(!is_x86_aarch64_native_clobber_safe_excluding(
                    &function, &excluded
                ));
                probes += 1;
            }
        }
    }
    assert_eq!(probes, 16 * 8 * 2);
}

#[test]
fn malformed_register_width_hint_and_register_file_shapes_are_rejected() {
    let exact_q2dq = transfer(xmm(15), mm(7), X86SsePrefix::Rep);
    let exact_dq2q = transfer(mm(7), xmm(15), X86SsePrefix::Repne);
    for malformed in [
        {
            let mut op = exact_q2dq.clone();
            let OpKind::X86MovdQ { width, .. } = &mut op.kind else {
                unreachable!()
            };
            *width = OpWidth::W32;
            op
        },
        {
            let mut op = exact_q2dq.clone();
            let OpKind::X86MovdQ { zero_upper, .. } = &mut op.kind else {
                unreachable!()
            };
            *zero_upper = true;
            op
        },
        {
            let mut op = exact_q2dq.clone();
            op.x86_hint = Some(X86OpHint::SseOp {
                prefix: X86SsePrefix::Repne,
                opcode: 0xD6,
            });
            op
        },
        {
            let mut op = exact_dq2q.clone();
            op.x86_hint = Some(X86OpHint::SseOp {
                prefix: X86SsePrefix::Repne,
                opcode: 0x6E,
            });
            op
        },
        transfer(xmm(16), mm(7), X86SsePrefix::Rep),
        transfer(mm(8), xmm(15), X86SsePrefix::Repne),
        transfer(VReg::Virtual(VirtualId(1)), mm(7), X86SsePrefix::Rep),
        transfer(mm(7), VReg::Virtual(VirtualId(2)), X86SsePrefix::Repne),
        transfer(xmm(15), xmm(7), X86SsePrefix::Rep),
        transfer(mm(7), mm(6), X86SsePrefix::Repne),
    ] {
        assert!(
            !crate::smir::lower::x86_64::x86_mmx_xmm_transfer_shape_valid(&malformed),
            "{malformed:?}"
        );
        assert!(!is_x86_native_mmx_op(&malformed), "{malformed:?}");
        let function = paired_function([malformed]);
        let excluded = std::collections::HashMap::new();
        assert!(!is_native_clobber_safe_excluding(
            &function, &excluded, false
        ));
        assert!(!uses_x86_xmm_state_excluding(&function, &excluded));
    }
}

#[test]
fn exact_state_markers_are_required_at_the_same_guest_frontier() {
    let op = transfer(xmm(3), mm(2), X86SsePrefix::Rep);
    let exact = paired_function([op.clone()]);
    let excluded = std::collections::HashMap::new();
    assert!(x86_native_mmx_pairs_valid_excluding(&exact, &excluded));

    let mut missing = exact.clone();
    missing.blocks[0].ops.remove(0);
    assert!(!x86_native_mmx_pairs_valid_excluding(&missing, &excluded));

    let mut wrong_pc = exact.clone();
    wrong_pc.blocks[0].ops[0].guest_pc += 1;
    assert!(!x86_native_mmx_pairs_valid_excluding(&wrong_pc, &excluded));

    let mut hinted_marker = exact.clone();
    hinted_marker.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0x77,
    });
    assert!(!x86_native_mmx_pairs_valid_excluding(
        &hinted_marker,
        &excluded
    ));

    let mut addressed_marker = exact;
    let OpKind::X86X87Control { addr, .. } = &mut addressed_marker.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *addr = Some(Address::Absolute(0x2000));
    assert!(!x86_native_mmx_pairs_valid_excluding(
        &addressed_marker,
        &excluded
    ));
}
