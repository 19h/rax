//! jit_gate_tests::mmx tests

use super::*;
use crate::smir::lower::runtime::*;

#[test]
fn x86_mmx_region_discriminator_tracks_precise_enter_state_and_exits() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EnterMmx,
            addr: None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let function = builder.finish();

    assert!(uses_x86_native_mmx_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    assert!(is_native_clobber_safe(&function));
    assert!(!x86_native_mmx_pairs_valid_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    assert!(!uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let excluded = std::collections::HashMap::from([(function.entry, 0x1001)]);
    assert!(!uses_x86_native_mmx_excluding(&function, &excluded));

    let mut x87 = FunctionBuilder::new(FunctionId(1), 0x2000);
    x87.push_op(
        0x2000,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::ClearExceptions,
            addr: None,
        },
    );
    x87.set_terminator(Terminator::Return { values: vec![] });
    assert!(!uses_x86_native_mmx_excluding(
        &x87.finish(),
        &std::collections::HashMap::new()
    ));

    let mut emms = FunctionBuilder::new(FunctionId(2), 0x3000);
    emms.push_op(
        0x3000,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EmptyMmx,
            addr: None,
        },
    );
    emms.set_terminator(Terminator::Return { values: vec![] });
    let emms = emms.finish();
    assert!(!uses_x86_native_mmx_excluding(
        &emms,
        &std::collections::HashMap::new()
    ));
    assert!(x86_native_mmx_pairs_valid_excluding(
        &emms,
        &std::collections::HashMap::new()
    ));
    assert!(is_native_clobber_safe(&emms));

    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    let mut three_d_now = FunctionBuilder::new(FunctionId(3), 0x4000);
    three_d_now.push_op(
        0x4000,
        OpKind::X86ThreeDNow {
            dst: mm(0),
            src1: mm(0),
            src2: mm(1),
            kind: X86ThreeDNowKind::PfAdd,
        },
    );
    three_d_now.push_op(
        0x4000,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EnterMmx,
            addr: None,
        },
    );
    three_d_now.set_terminator(Terminator::Return { values: vec![] });
    let three_d_now = three_d_now.finish();
    assert!(uses_x86_native_mmx_excluding(
        &three_d_now,
        &std::collections::HashMap::new()
    ));
    assert!(!is_native_clobber_safe(&three_d_now));
    assert!(!x86_native_mmx_pairs_valid_excluding(
        &three_d_now,
        &std::collections::HashMap::new()
    ));
}
#[test]
fn x86_mmx_logic_gate_requires_exact_hint_registers_and_state_pair() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    let kind = OpKind::VAndNot {
        dst: mm(2),
        src1: mm(2),
        src2: mm(7),
        width: VecWidth::V64,
    };
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x3000);
    builder.push_op(
        0x3000,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EnterMmx,
            addr: None,
        },
    );
    builder.push_op(0x3000, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[1].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0xDF,
    });

    assert!(is_x86_native_mmx_op(&function.blocks[0].ops[1]));
    assert!(is_native_clobber_safe(&function));
    assert!(x86_native_mmx_pairs_valid_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
    assert!(!uses_x86_native_vectors_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    function.blocks[0].ops[1].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::OpSize,
        opcode: 0xDF,
    });
    assert!(!is_x86_native_mmx_op(&function.blocks[0].ops[1]));
    assert!(!is_native_clobber_safe(&function));

    function.blocks[0].ops[1].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0xDB,
    });
    assert!(!is_x86_native_mmx_op(&function.blocks[0].ops[1]));

    function.blocks[0].ops[1].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0xDF,
    });
    function.blocks[0].ops[1].guest_pc = 0x3001;
    assert!(!x86_native_mmx_pairs_valid_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
}
#[test]
fn x86_mmx_packed_add_sub_gate_covers_all_classic_register_opcodes() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    let arithmetic = |opcode| match opcode {
        0xFC | 0xFD | 0xFE | 0xD4 => {
            let (elem, lanes) = match opcode {
                0xFC => (VecElementType::I8, 8),
                0xFD => (VecElementType::I16, 4),
                0xFE => (VecElementType::I32, 2),
                0xD4 => (VecElementType::I64, 1),
                _ => unreachable!(),
            };
            OpKind::VAdd {
                dst: mm(3),
                src1: mm(3),
                src2: mm(6),
                elem,
                lanes,
            }
        }
        0xF8 | 0xF9 | 0xFA | 0xFB => {
            let (elem, lanes) = match opcode {
                0xF8 => (VecElementType::I8, 8),
                0xF9 => (VecElementType::I16, 4),
                0xFA => (VecElementType::I32, 2),
                0xFB => (VecElementType::I64, 1),
                _ => unreachable!(),
            };
            OpKind::VSub {
                dst: mm(3),
                src1: mm(3),
                src2: mm(6),
                elem,
                lanes,
            }
        }
        _ => {
            let (elem, lanes, subtract, signed) = match opcode {
                0xEC => (VecElementType::I8, 8, false, true),
                0xED => (VecElementType::I16, 4, false, true),
                0xDC => (VecElementType::I8, 8, false, false),
                0xDD => (VecElementType::I16, 4, false, false),
                0xE8 => (VecElementType::I8, 8, true, true),
                0xE9 => (VecElementType::I16, 4, true, true),
                0xD8 => (VecElementType::I8, 8, true, false),
                0xD9 => (VecElementType::I16, 4, true, false),
                _ => unreachable!(),
            };
            OpKind::VAddSubSat {
                dst: mm(3),
                src1: mm(3),
                src2: mm(6),
                elem,
                lanes,
                subtract,
                signed,
            }
        }
    };

    for opcode in [
        0xFC, 0xFD, 0xFE, 0xD4, 0xF8, 0xF9, 0xFA, 0xFB, 0xEC, 0xED, 0xDC, 0xDD, 0xE8, 0xE9, 0xD8,
        0xD9,
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x4000);
        builder.push_op(
            0x4000,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            },
        );
        builder.push_op(0x4000, arithmetic(opcode));
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[1].x86_hint = Some(X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode,
        });
        assert!(is_x86_native_mmx_op(&function.blocks[0].ops[1]));
        assert!(is_native_clobber_safe(&function));
        assert!(x86_native_mmx_pairs_valid_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
    }

    let mut malformed = arithmetic(0xFC);
    if let OpKind::VAdd { lanes, .. } = &mut malformed {
        *lanes = 4;
    }
    let mut builder = FunctionBuilder::new(FunctionId(1), 0x5000);
    builder.push_op(0x5000, malformed);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0xFC,
    });
    assert!(!is_x86_native_mmx_op(&function.blocks[0].ops[0]));
    assert!(!is_native_clobber_safe(&function));
}
#[test]
fn x86_mmx_packed_compare_gate_covers_exact_classic_shapes() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    for (elem, lanes, cond, opcode) in [
        (VecElementType::I8, 8, VecCmpCond::Gt, 0x64),
        (VecElementType::I16, 4, VecCmpCond::Gt, 0x65),
        (VecElementType::I32, 2, VecCmpCond::Gt, 0x66),
        (VecElementType::I8, 8, VecCmpCond::Eq, 0x74),
        (VecElementType::I16, 4, VecCmpCond::Eq, 0x75),
        (VecElementType::I32, 2, VecCmpCond::Eq, 0x76),
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x6000);
        builder.push_op(
            0x6000,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            },
        );
        builder.push_op(
            0x6000,
            OpKind::VCmp {
                dst: mm(4),
                src1: mm(4),
                src2: mm(1),
                cond,
                elem,
                lanes,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[1].x86_hint = Some(X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode,
        });
        assert!(is_x86_native_mmx_op(&function.blocks[0].ops[1]));
        assert!(is_native_clobber_safe(&function));
        assert!(x86_native_mmx_pairs_valid_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
    }
}
#[test]
fn x86_mmx_interleave_gate_covers_exact_classic_shapes() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    for (elem, lanes, block_lanes, high, opcode) in [
        (VecElementType::I8, 8, 8, false, 0x60),
        (VecElementType::I16, 4, 4, false, 0x61),
        (VecElementType::I32, 2, 2, false, 0x62),
        (VecElementType::I8, 8, 8, true, 0x68),
        (VecElementType::I16, 4, 4, true, 0x69),
        (VecElementType::I32, 2, 2, true, 0x6A),
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x7000);
        builder.push_op(
            0x7000,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            },
        );
        builder.push_op(
            0x7000,
            OpKind::VInterleave {
                dst: mm(5),
                src1: mm(5),
                src2: mm(2),
                elem,
                lanes,
                block_lanes,
                high,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[1].x86_hint = Some(X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode,
        });
        assert!(is_x86_native_mmx_op(&function.blocks[0].ops[1]));
        assert!(is_native_clobber_safe(&function));
        assert!(x86_native_mmx_pairs_valid_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
    }
}
#[test]
fn x86_mmx_pack_gate_preserves_reversed_smir_source_order() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    for (src_elem, src_lanes, to_unsigned, opcode) in [
        (VecElementType::I16, 4, false, 0x63),
        (VecElementType::I16, 4, true, 0x67),
        (VecElementType::I32, 2, false, 0x6B),
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x8000);
        builder.push_op(
            0x8000,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            },
        );
        builder.push_op(
            0x8000,
            OpKind::VPackSat {
                dst: mm(6),
                src1: mm(3),
                src2: mm(6),
                src_elem,
                to_unsigned,
                src_lanes,
                block_lanes: src_lanes,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[1].x86_hint = Some(X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode,
        });
        assert!(is_x86_native_mmx_op(&function.blocks[0].ops[1]));
        assert!(is_native_clobber_safe(&function));
        assert!(x86_native_mmx_pairs_valid_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
    }
}
#[test]
fn x86_mmx_minmax_gate_accepts_post_op_state_pairs_only_once() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    for (elem, lanes, lane_op, signed, opcode) in [
        (VecElementType::I8, 8, VLaneOp::Min, false, 0xDA),
        (VecElementType::I8, 8, VLaneOp::Max, false, 0xDE),
        (VecElementType::I16, 4, VLaneOp::Min, true, 0xEA),
        (VecElementType::I16, 4, VLaneOp::Max, true, 0xEE),
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x9000);
        builder.push_op(
            0x9000,
            OpKind::VLane {
                dst: mm(7),
                src1: mm(7),
                src2: mm(0),
                elem,
                lanes,
                op: lane_op,
                signed,
                set_ovf: false,
            },
        );
        builder.push_op(
            0x9000,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode,
        });
        assert!(is_x86_native_mmx_op(&function.blocks[0].ops[0]));
        assert!(is_native_clobber_safe(&function));
        assert!(x86_native_mmx_pairs_valid_excluding(
            &function,
            &std::collections::HashMap::new()
        ));

        let duplicate = function.blocks[0].ops[0].clone();
        function.blocks[0].ops.push(duplicate);
        assert!(!x86_native_mmx_pairs_valid_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
    }
}
#[test]
fn x86_mmx_average_gate_covers_byte_and_word_register_forms() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    for (elem, lanes, opcode) in [
        (VecElementType::I8, 8, 0xE0),
        (VecElementType::I16, 4, 0xE3),
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x9800);
        builder.push_op(
            0x9800,
            OpKind::VLane {
                dst: mm(2),
                src1: mm(2),
                src2: mm(5),
                elem,
                lanes,
                op: VLaneOp::AvgRnd,
                signed: false,
                set_ovf: false,
            },
        );
        builder.push_op(
            0x9800,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode,
        });
        assert!(is_x86_native_mmx_op(&function.blocks[0].ops[0]));
        assert!(is_native_clobber_safe(&function));
        assert!(x86_native_mmx_pairs_valid_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
    }
}
#[test]
fn x86_mmx_maddwd_gate_accepts_only_non_accumulating_word_dot_product() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x9900);
    builder.push_op(
        0x9900,
        OpKind::VDotProduct {
            dst: mm(3),
            acc: VReg::Imm(0),
            src1: mm(3),
            src2: mm(6),
            mask: None,
            src_elem: VecElementType::I16,
            acc_elem: VecElementType::I32,
            width: VecWidth::V64,
            src1_unsigned: false,
            saturate: false,
            zeroing: false,
        },
    );
    builder.push_op(
        0x9900,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EnterMmx,
            addr: None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0xF5,
    });
    assert!(is_x86_native_mmx_op(&function.blocks[0].ops[0]));
    assert!(is_native_clobber_safe(&function));
    assert!(x86_native_mmx_pairs_valid_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    let OpKind::VDotProduct { acc, .. } = &mut function.blocks[0].ops[0].kind else {
        unreachable!()
    };
    *acc = mm(0);
    assert!(!is_x86_native_mmx_op(&function.blocks[0].ops[0]));
}
#[test]
fn x86_mmx_sad_bytes_gate_accepts_exact_v64_register_shape() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x9A00);
    builder.push_op(
        0x9A00,
        OpKind::VSadBytes {
            dst: mm(4),
            src1: mm(4),
            src2: mm(1),
            width: VecWidth::V64,
        },
    );
    builder.push_op(
        0x9A00,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EnterMmx,
            addr: None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0xF6,
    });
    assert!(is_x86_native_mmx_op(&function.blocks[0].ops[0]));
    assert!(is_native_clobber_safe(&function));
    assert!(x86_native_mmx_pairs_valid_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
}
#[test]
fn x86_mmx_shared_count_shift_gate_covers_all_classic_opcodes() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    for (elem, shift, opcode) in [
        (VecElementType::I16, ShiftOp::Lsr, 0xD1),
        (VecElementType::I32, ShiftOp::Lsr, 0xD2),
        (VecElementType::I64, ShiftOp::Lsr, 0xD3),
        (VecElementType::I16, ShiftOp::Asr, 0xE1),
        (VecElementType::I32, ShiftOp::Asr, 0xE2),
        (VecElementType::I16, ShiftOp::Lsl, 0xF1),
        (VecElementType::I32, ShiftOp::Lsl, 0xF2),
        (VecElementType::I64, ShiftOp::Lsl, 0xF3),
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x9B00);
        builder.push_op(
            0x9B00,
            OpKind::X86PackedShift {
                dst: mm(2),
                src: mm(2),
                count: mm(5),
                width: VecWidth::V64,
                elem,
                shift,
            },
        );
        builder.push_op(
            0x9B00,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode,
        });
        assert!(is_x86_native_mmx_op(&function.blocks[0].ops[0]));
        assert!(is_native_clobber_safe(&function));
        assert!(x86_native_mmx_pairs_valid_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
    }
}
#[test]
fn x86_mmx_immediate_shift_gate_covers_all_classic_group_forms() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    for (elem, shift, opcode) in [
        (VecElementType::I16, ShiftOp::Lsr, 0x71),
        (VecElementType::I16, ShiftOp::Asr, 0x71),
        (VecElementType::I16, ShiftOp::Lsl, 0x71),
        (VecElementType::I32, ShiftOp::Lsr, 0x72),
        (VecElementType::I32, ShiftOp::Asr, 0x72),
        (VecElementType::I32, ShiftOp::Lsl, 0x72),
        (VecElementType::I64, ShiftOp::Lsr, 0x73),
        (VecElementType::I64, ShiftOp::Lsl, 0x73),
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x9C00);
        builder.push_op(
            0x9C00,
            OpKind::X86PackedShiftImm {
                dst: mm(1),
                src: mm(1),
                width: VecWidth::V64,
                elem,
                shift,
                amount: 17,
                byte_lane: false,
            },
        );
        builder.push_op(
            0x9C00,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode,
        });
        assert!(is_x86_native_mmx_op(&function.blocks[0].ops[0]));
        assert!(is_native_clobber_safe(&function));
        assert!(x86_native_mmx_pairs_valid_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
    }
}
#[test]
fn x86_mmx_absolute_value_gate_covers_ssse3_byte_word_and_dword_forms() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    for (elem, lanes, opcode) in [
        (VecElementType::I8, 8, 0x1C),
        (VecElementType::I16, 4, 0x1D),
        (VecElementType::I32, 2, 0x1E),
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x9D00);
        builder.push_op(
            0x9D00,
            OpKind::VUnary {
                dst: mm(0),
                src: mm(1),
                elem,
                lanes,
                op: VecUnaryOp::Abs,
            },
        );
        builder.push_op(
            0x9D00,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode,
        });
        assert!(is_x86_native_mmx_op(&function.blocks[0].ops[0]));
        assert!(x86_native_mmx_op_requires_ssse3(&function.blocks[0].ops[0]));
        assert!(is_native_clobber_safe(&function));
        assert!(x86_native_mmx_pairs_valid_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
    }
}
#[test]
fn x86_mmx_sign_gate_covers_ssse3_byte_word_and_dword_forms() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    for (elem, lanes, opcode) in [
        (VecElementType::I8, 8, 0x08),
        (VecElementType::I16, 4, 0x09),
        (VecElementType::I32, 2, 0x0A),
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x9E00);
        builder.push_op(
            0x9E00,
            OpKind::VLane {
                dst: mm(0),
                src1: mm(0),
                src2: mm(1),
                elem,
                lanes,
                op: VLaneOp::Sign,
                signed: true,
                set_ovf: false,
            },
        );
        builder.push_op(
            0x9E00,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode,
        });
        assert!(is_x86_native_mmx_op(&function.blocks[0].ops[0]));
        assert!(x86_native_mmx_op_requires_ssse3(&function.blocks[0].ops[0]));
        assert!(is_native_clobber_safe(&function));
        assert!(x86_native_mmx_pairs_valid_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
    }
}
#[test]
fn x86_mmx_horizontal_gate_covers_all_ssse3_add_sub_forms() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    for (elem, lanes, subtract, saturating, opcode) in [
        (VecElementType::I16, 4, false, false, 0x01),
        (VecElementType::I32, 2, false, false, 0x02),
        (VecElementType::I16, 4, false, true, 0x03),
        (VecElementType::I16, 4, true, false, 0x05),
        (VecElementType::I32, 2, true, false, 0x06),
        (VecElementType::I16, 4, true, true, 0x07),
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x9F00);
        builder.push_op(
            0x9F00,
            OpKind::VHorizontalBin {
                dst: mm(0),
                src1: mm(0),
                src2: mm(1),
                elem,
                lanes,
                block_lanes: lanes,
                subtract,
                saturating,
            },
        );
        builder.push_op(
            0x9F00,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode,
        });
        assert!(is_x86_native_mmx_op(&function.blocks[0].ops[0]));
        assert!(x86_native_mmx_op_requires_ssse3(&function.blocks[0].ops[0]));
        assert!(is_native_clobber_safe(&function));
        assert!(x86_native_mmx_pairs_valid_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
    }
}
#[test]
fn x86_mmx_maddubs_gate_accepts_exact_ssse3_saturating_dot_product() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0xA100);
    builder.push_op(
        0xA100,
        OpKind::VDotProduct {
            dst: mm(0),
            acc: VReg::Imm(0),
            src1: mm(0),
            src2: mm(1),
            mask: None,
            src_elem: VecElementType::I8,
            acc_elem: VecElementType::I16,
            width: VecWidth::V64,
            src1_unsigned: true,
            saturate: true,
            zeroing: false,
        },
    );
    builder.push_op(
        0xA100,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EnterMmx,
            addr: None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0x04,
    });
    assert!(is_x86_native_mmx_op(&function.blocks[0].ops[0]));
    assert!(x86_native_mmx_op_requires_ssse3(&function.blocks[0].ops[0]));
    assert!(is_native_clobber_safe(&function));
    assert!(x86_native_mmx_pairs_valid_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
}
#[test]
fn x86_mmx_mulhrsw_gate_accepts_exact_ssse3_rounded_high_multiply() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0xA200);
    builder.push_op(
        0xA200,
        OpKind::VMulShiftSat {
            dst: mm(0),
            src1: mm(0),
            src2: mm(1),
            src_elem: VecElementType::I16,
            lanes: 4,
            signed1: true,
            signed2: true,
            shift_left: 0,
            round: true,
            sat_bits: 0,
            out_shift: 15,
        },
    );
    builder.push_op(
        0xA200,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EnterMmx,
            addr: None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0x0B,
    });
    assert!(is_x86_native_mmx_op(&function.blocks[0].ops[0]));
    assert!(x86_native_mmx_op_requires_ssse3(&function.blocks[0].ops[0]));
    assert!(is_native_clobber_safe(&function));
    assert!(x86_native_mmx_pairs_valid_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
}
#[test]
fn x86_mmx_byte_shuffle_gate_accepts_only_exact_ssse3_destructive_form() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    let exact = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(0),
        0xA300,
        OpKind::VByteShuffle {
            dst: mm(0),
            src: mm(0),
            control: mm(1),
            lanes: 8,
            block_lanes: 8,
        },
        X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode: 0x00,
        },
    );
    assert!(is_x86_native_mmx_op(&exact));
    assert!(x86_native_mmx_op_requires_ssse3(&exact));

    for malformed in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(1),
            0xA300,
            OpKind::VByteShuffle {
                dst: mm(0),
                src: mm(2),
                control: mm(1),
                lanes: 8,
                block_lanes: 8,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x00,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(2),
            0xA300,
            OpKind::VByteShuffle {
                dst: mm(0),
                src: mm(0),
                control: mm(1),
                lanes: 16,
                block_lanes: 8,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x00,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(3),
            0xA300,
            OpKind::VByteShuffle {
                dst: mm(0),
                src: mm(0),
                control: mm(1),
                lanes: 8,
                block_lanes: 8,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x00,
            },
        ),
    ] {
        assert!(!is_x86_native_mmx_op(&malformed), "{malformed:?}");
        assert!(!x86_native_mmx_op_requires_ssse3(&malformed));
    }
}
#[test]
fn x86_mmx_movemask_gate_accepts_only_exact_safe_gpr_form() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0xA400);
    builder.push_op(
        0xA400,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EnterMmx,
            addr: None,
        },
    );
    builder.push_op(
        0xA400,
        OpKind::X86MovMask {
            dst: VReg::Arch(ArchReg::X86(X86Reg::R8)),
            src: mm(1),
            elem: VecElementType::I8,
            lanes: 8,
            dst_width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[1].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0xD7,
    });
    assert!(is_x86_native_mmx_op(&function.blocks[0].ops[1]));
    assert!(!x86_native_mmx_op_requires_ssse3(
        &function.blocks[0].ops[1]
    ));
    assert!(is_native_clobber_safe(&function));
    assert!(x86_native_mmx_pairs_valid_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    for (dst, lanes, prefix) in [
        (VReg::Arch(ArchReg::X86(X86Reg::Rbp)), 8, X86SsePrefix::None),
        (VReg::Arch(ArchReg::X86(X86Reg::R8)), 16, X86SsePrefix::None),
        (
            VReg::Arch(ArchReg::X86(X86Reg::R8)),
            8,
            X86SsePrefix::OpSize,
        ),
    ] {
        let malformed = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0xA400,
            OpKind::X86MovMask {
                dst,
                src: mm(1),
                elem: VecElementType::I8,
                lanes,
                dst_width: OpWidth::W64,
            },
            X86OpHint::SseOp {
                prefix,
                opcode: 0xD7,
            },
        );
        assert!(!is_x86_native_mmx_op(&malformed), "{malformed:?}");
    }
}
#[test]
fn x86_mmx_movq_gate_accepts_only_exact_v64_register_forms() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    for opcode in [0x6F, 0x7F] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0xA470);
        builder.push_op(
            0xA470,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            },
        );
        builder.push_op(
            0xA470,
            OpKind::VMov {
                dst: mm(1),
                src: mm(2),
                width: VecWidth::V64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[1].x86_hint = Some(X86OpHint::SseMov {
            prefix: X86SsePrefix::None,
            opcode,
        });
        assert!(is_x86_native_mmx_op(&function.blocks[0].ops[1]));
        assert!(!x86_native_mmx_op_requires_ssse3(
            &function.blocks[0].ops[1]
        ));
        assert!(is_native_clobber_safe(&function));
        assert!(x86_native_mmx_pairs_valid_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
    }

    for malformed in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0xA470,
            OpKind::VMov {
                dst: mm(1),
                src: mm(2),
                width: VecWidth::V128,
            },
            X86OpHint::SseMov {
                prefix: X86SsePrefix::None,
                opcode: 0x6F,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(1),
            0xA470,
            OpKind::VMov {
                dst: mm(1),
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                width: VecWidth::V64,
            },
            X86OpHint::SseMov {
                prefix: X86SsePrefix::None,
                opcode: 0x6F,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(2),
            0xA470,
            OpKind::VMov {
                dst: mm(1),
                src: mm(2),
                width: VecWidth::V64,
            },
            X86OpHint::SseMov {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x6F,
            },
        ),
    ] {
        assert!(!is_x86_native_mmx_op(&malformed), "{malformed:?}");
    }
}
#[test]
fn x86_mmx_word_lane_gate_accepts_only_exact_safe_register_forms() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    let gpr = |reg| VReg::Arch(ArchReg::X86(reg));
    for (kind, opcode) in [
        (
            OpKind::VInsertLane {
                dst: mm(1),
                vec: mm(1),
                scalar: gpr(X86Reg::R10),
                lane: 3,
                elem: VecElementType::I16,
            },
            0xC4,
        ),
        (
            OpKind::VExtractLane {
                dst: gpr(X86Reg::R8),
                vec: mm(2),
                lane: 3,
                elem: VecElementType::I16,
                sign: SignExtend::Zero,
            },
            0xC5,
        ),
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0xA480);
        builder.push_op(
            0xA480,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            },
        );
        builder.push_op(0xA480, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[1].x86_hint = Some(X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode,
        });
        assert!(is_x86_native_mmx_op(&function.blocks[0].ops[1]));
        assert!(!x86_native_mmx_op_requires_ssse3(
            &function.blocks[0].ops[1]
        ));
        assert!(is_native_clobber_safe(&function));
        assert!(x86_native_mmx_pairs_valid_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
    }

    for malformed in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0xA480,
            OpKind::VInsertLane {
                dst: mm(1),
                vec: mm(2),
                scalar: gpr(X86Reg::R10),
                lane: 3,
                elem: VecElementType::I16,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0xC4,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(1),
            0xA480,
            OpKind::VInsertLane {
                dst: mm(1),
                vec: mm(1),
                scalar: gpr(X86Reg::Rbp),
                lane: 3,
                elem: VecElementType::I16,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0xC4,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(2),
            0xA480,
            OpKind::VExtractLane {
                dst: gpr(X86Reg::R8),
                vec: mm(2),
                lane: 4,
                elem: VecElementType::I16,
                sign: SignExtend::Zero,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0xC5,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(3),
            0xA480,
            OpKind::VExtractLane {
                dst: gpr(X86Reg::R8),
                vec: mm(2),
                lane: 3,
                elem: VecElementType::I16,
                sign: SignExtend::Sign,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xC5,
            },
        ),
    ] {
        assert!(!is_x86_native_mmx_op(&malformed), "{malformed:?}");
    }
}
#[test]
fn x86_mmx_movd_q_gate_accepts_exact_bidirectional_register_transfers() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    let gpr = |reg| VReg::Arch(ArchReg::X86(reg));
    for (kind, opcode) in [
        (
            OpKind::X86MovdQ {
                dst: mm(1),
                src: gpr(X86Reg::R10),
                width: OpWidth::W64,
                zero_upper: false,
            },
            0x6E,
        ),
        (
            OpKind::X86MovdQ {
                dst: gpr(X86Reg::R8),
                src: mm(2),
                width: OpWidth::W32,
                zero_upper: false,
            },
            0x7E,
        ),
    ] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0xA500);
        builder.push_op(
            0xA500,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            },
        );
        builder.push_op(0xA500, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[1].x86_hint = Some(X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode,
        });
        assert!(is_x86_native_mmx_op(&function.blocks[0].ops[1]));
        assert!(!x86_native_mmx_op_requires_ssse3(
            &function.blocks[0].ops[1]
        ));
        assert!(is_native_clobber_safe(&function));
        assert!(x86_native_mmx_pairs_valid_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
    }

    for malformed in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0xA500,
            OpKind::X86MovdQ {
                dst: mm(1),
                src: gpr(X86Reg::Rsp),
                width: OpWidth::W64,
                zero_upper: false,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x6E,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(1),
            0xA500,
            OpKind::X86MovdQ {
                dst: mm(1),
                src: gpr(X86Reg::R10),
                width: OpWidth::W64,
                zero_upper: true,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x6E,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(2),
            0xA500,
            OpKind::X86MovdQ {
                dst: gpr(X86Reg::R8),
                src: mm(2),
                width: OpWidth::W32,
                zero_upper: false,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x7E,
            },
        ),
    ] {
        assert!(!is_x86_native_mmx_op(&malformed), "{malformed:?}");
    }
}
#[test]
fn x86_mmx_align_right_gate_accepts_only_exact_ssse3_destructive_form() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0xA600);
    builder.push_op(
        0xA600,
        OpKind::X86PackedAlignRight {
            dst: mm(0),
            high: mm(0),
            low: mm(1),
            width: VecWidth::V64,
            amount: 0x25,
        },
    );
    builder.push_op(
        0xA600,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EnterMmx,
            addr: None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0x0F,
    });
    assert!(is_x86_native_mmx_op(&function.blocks[0].ops[0]));
    assert!(x86_native_mmx_op_requires_ssse3(&function.blocks[0].ops[0]));
    assert!(is_native_clobber_safe(&function));
    assert!(x86_native_mmx_pairs_valid_excluding(
        &function,
        &std::collections::HashMap::new()
    ));

    for malformed in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0xA600,
            OpKind::X86PackedAlignRight {
                dst: mm(0),
                high: mm(2),
                low: mm(1),
                width: VecWidth::V64,
                amount: 5,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x0F,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(1),
            0xA600,
            OpKind::X86PackedAlignRight {
                dst: mm(0),
                high: mm(0),
                low: mm(1),
                width: VecWidth::V128,
                amount: 5,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x0F,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(2),
            0xA600,
            OpKind::X86PackedAlignRight {
                dst: mm(0),
                high: mm(0),
                low: mm(1),
                width: VecWidth::V64,
                amount: 5,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x0F,
            },
        ),
    ] {
        assert!(!is_x86_native_mmx_op(&malformed), "{malformed:?}");
        assert!(!x86_native_mmx_op_requires_ssse3(&malformed));
    }
}
#[test]
fn x86_mmx_word_shuffle_gate_accepts_only_exact_immediate_form() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    let exact = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(0),
        0xA700,
        OpKind::X86PackedShuffleImm {
            dst: mm(0),
            src: mm(1),
            width: VecWidth::V64,
            elem: VecElementType::I16,
            imm: 0x1B,
            high_words: None,
        },
        X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode: 0x70,
        },
    );
    assert!(is_x86_native_mmx_op(&exact));
    assert!(!x86_native_mmx_op_requires_ssse3(&exact));

    for malformed in [
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(1),
            0xA700,
            OpKind::X86PackedShuffleImm {
                dst: mm(0),
                src: mm(1),
                width: VecWidth::V128,
                elem: VecElementType::I16,
                imm: 0x1B,
                high_words: None,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x70,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(2),
            0xA700,
            OpKind::X86PackedShuffleImm {
                dst: mm(0),
                src: mm(1),
                width: VecWidth::V64,
                elem: VecElementType::I16,
                imm: 0x1B,
                high_words: Some(true),
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x70,
            },
        ),
        crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(3),
            0xA700,
            OpKind::X86PackedShuffleImm {
                dst: mm(0),
                src: mm(1),
                width: VecWidth::V64,
                elem: VecElementType::I16,
                imm: 0x1B,
                high_words: None,
            },
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x70,
            },
        ),
    ] {
        assert!(!is_x86_native_mmx_op(&malformed), "{malformed:?}");
    }
}
#[test]
fn x86_mmx_word_multiply_gate_covers_low_signed_high_and_unsigned_high() {
    let mm = |index| VReg::Arch(ArchReg::X86(X86Reg::Mm(index)));
    let kinds = [
        OpKind::VMul {
            dst: mm(1),
            src1: mm(1),
            src2: mm(4),
            elem: VecElementType::I16,
            lanes: 4,
        },
        OpKind::VMulShiftSat {
            dst: mm(1),
            src1: mm(1),
            src2: mm(4),
            src_elem: VecElementType::I16,
            lanes: 4,
            signed1: false,
            signed2: false,
            shift_left: 0,
            round: false,
            sat_bits: 0,
            out_shift: 16,
        },
        OpKind::VMulShiftSat {
            dst: mm(1),
            src1: mm(1),
            src2: mm(4),
            src_elem: VecElementType::I16,
            lanes: 4,
            signed1: true,
            signed2: true,
            shift_left: 0,
            round: false,
            sat_bits: 0,
            out_shift: 16,
        },
    ];
    for (kind, opcode) in kinds.into_iter().zip([0xD5, 0xE4, 0xE5]) {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0xA000);
        builder.push_op(0xA000, kind);
        builder.push_op(
            0xA000,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
            prefix: X86SsePrefix::None,
            opcode,
        });
        assert!(is_x86_native_mmx_op(&function.blocks[0].ops[0]));
        assert!(is_native_clobber_safe(&function));
        assert!(x86_native_mmx_pairs_valid_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
    }
}
#[cfg(target_arch = "x86_64")]
#[test]
fn x86_mmx_trampoline_round_trips_all_registers_and_cleans_host_state() {
    // paddb mm0,mm1; movq mm7,mm0; ret. The trampoline must import both
    // sources, export both destinations and execute EMMS before Rust resumes.
    let exec =
        ExecMem::new(&[0x0F, 0xFC, 0xC1, 0x0F, 0x6F, 0xF8, 0xC3]).expect("map raw MMX block");
    let mut regs = GuestRegs {
        mm: [
            0x00ff_7f80_0102_0304,
            0x0102_0304_0506_0708,
            0x2222_2222_2222_2222,
            0x3333_3333_3333_3333,
            0x4444_4444_4444_4444,
            0x5555_5555_5555_5555,
            0x6666_6666_6666_6666,
            0x7777_7777_7777_7777,
        ],
        mmx_active: 1,
        x87_tag_word: 0xA5A5,
        ..GuestRegs::default()
    };
    let original = regs.mm;
    let expected = 0x0101_8284_0608_0A0C;

    exec.run(0, &mut regs);

    assert_eq!(regs.mm[0], expected);
    assert_eq!(regs.mm[7], expected);
    assert_eq!(&regs.mm[1..7], &original[1..7]);
    assert_eq!(
        regs.x87_tag_word, 0xA5A5,
        "host EMMS must not alter guest tag state"
    );

    // A scalar x87 operation after ExecMem::run is also an execution-level
    // probe that the trampoline did not leave the host in MMX tag state.
    let mut x87_result = std::mem::MaybeUninit::<f64>::uninit();
    unsafe {
        core::arch::asm!(
            "fld1",
            "fld1",
            "faddp st(1), st(0)",
            "fstp qword ptr [{out}]",
            out = in(reg) x87_result.as_mut_ptr(),
            options(nostack)
        );
        assert_eq!(x87_result.assume_init(), 2.0);
    }
}
