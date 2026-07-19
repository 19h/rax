//! Helper-backed MMX memory-source admission tests.

use super::*;
use crate::smir::ir::ops::X86VecAlign;
use crate::smir::lower::runtime::*;

fn paddb_m64_function() -> crate::smir::ir::SmirFunction {
    let temporary = VReg::Virtual(VirtualId(7));
    let mm3 = VReg::Arch(ArchReg::X86(X86Reg::Mm(3)));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::VLoad {
            dst: temporary,
            addr: Address::BaseOffset {
                base: VReg::Arch(ArchReg::X86(X86Reg::Rbx)),
                offset: 8,
                disp_size: DispSize::Disp8,
            },
            width: VecWidth::V64,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EnterMmx,
            addr: None,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::VAdd {
            dst: mm3,
            src1: mm3,
            src2: temporary,
            elem: VecElementType::I8,
            lanes: 8,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[2].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0xFC,
    });
    function
}

fn punpcklbw_m32_function() -> crate::smir::ir::SmirFunction {
    let scalar = VReg::Virtual(VirtualId(7));
    let loaded = VReg::Virtual(VirtualId(8));
    let mm3 = VReg::Arch(ArchReg::X86(X86Reg::Mm(3)));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Load {
            dst: scalar,
            addr: Address::BaseOffset {
                base: VReg::Arch(ArchReg::X86(X86Reg::Rbx)),
                offset: 4,
                disp_size: DispSize::Disp8,
            },
            width: MemWidth::B4,
            sign: SignExtend::Zero,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::VBroadcast {
            dst: loaded,
            scalar,
            elem: VecElementType::I64,
            lanes: 1,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EnterMmx,
            addr: None,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::VInterleave {
            dst: mm3,
            src1: mm3,
            src2: loaded,
            elem: VecElementType::I8,
            lanes: 8,
            block_lanes: 8,
            high: false,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[3].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0x60,
    });
    function
}

fn pinsrw_m16_function() -> crate::smir::ir::SmirFunction {
    let scalar = VReg::Virtual(VirtualId(7));
    let mm3 = VReg::Arch(ArchReg::X86(X86Reg::Mm(3)));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Load {
            dst: scalar,
            addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rbx))),
            width: MemWidth::B2,
            sign: SignExtend::Zero,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EnterMmx,
            addr: None,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::VInsertLane {
            dst: mm3,
            vec: mm3,
            scalar,
            lane: 2,
            elem: VecElementType::I16,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[2].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0xC4,
    });
    function
}

fn virtual_counts(
    function: &crate::smir::ir::SmirFunction,
) -> (
    std::collections::HashMap<VReg, usize>,
    std::collections::HashMap<VReg, usize>,
) {
    let mut definitions = std::collections::HashMap::new();
    let mut uses = std::collections::HashMap::new();
    for op in &function.blocks[0].ops {
        for reg in op.kind.dests() {
            if matches!(reg, VReg::Virtual(_)) {
                *definitions.entry(reg).or_insert(0) += 1;
            }
        }
        for reg in op.kind.source_vregs() {
            if matches!(reg, VReg::Virtual(_)) {
                *uses.entry(reg).or_insert(0) += 1;
            }
        }
    }
    (definitions, uses)
}

fn sequence(function: &crate::smir::ir::SmirFunction) -> Option<X86MmxMemorySourceSequence> {
    sequence_with_mem(function, true)
}

fn sequence_with_mem(
    function: &crate::smir::ir::SmirFunction,
    allow_mem: bool,
) -> Option<X86MmxMemorySourceSequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_mmx_memory_source_sequence(&function.blocks[0], 0, allow_mem, &definitions, &uses)
}

fn assert_sequence_rejected(function: &crate::smir::ir::SmirFunction) {
    let excluded = std::collections::HashMap::new();
    assert!(
        sequence(function).is_none(),
        "unexpected MMX memory sequence: {:#?}",
        function.blocks[0].ops
    );
    assert!(!is_native_clobber_safe_excluding(function, &excluded, true));
}

#[test]
fn x86_mmx_m64_source_requires_exact_helper_backed_fusion() {
    let function = paddb_m64_function();
    let excluded = std::collections::HashMap::new();

    assert!(is_native_clobber_safe_excluding(&function, &excluded, true));
    assert!(x86_native_mmx_pairs_valid_excluding(&function, &excluded));
    assert!(uses_x86_native_mmx_excluding(&function, &excluded));
    assert!(!uses_x86_native_vectors_excluding(&function, &excluded));
    assert!(!is_native_clobber_safe_excluding(
        &function, &excluded, false
    ));
    let exact = sequence(&function).expect("exact PADDB m64 sequence");
    assert_eq!(exact.consumed, 3);
    assert_eq!(exact.marker_offset, 1);
    assert_eq!(exact.encoding.map, X86VecMap::Map0F);
    assert_eq!(exact.encoding.opcode, 0xFC);
    assert_eq!(exact.encoding.dst_index, 3);
    assert_eq!(exact.encoding.immediate, None);
    assert_eq!(exact.encoding.mem_width, MemWidth::B8);
    assert!(!exact.encoding.requires_ssse3);

    let mut optimizer_aligned = function.clone();
    optimizer_aligned.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
    assert!(sequence(&optimizer_aligned).is_some());
    assert!(is_native_clobber_safe_excluding(
        &optimizer_aligned,
        &excluded,
        true
    ));

    let mut marker_after = function.clone();
    marker_after.blocks[0].ops.swap(1, 2);
    let exact = sequence(&marker_after).expect("post-operation EnterMmx order");
    assert_eq!(exact.marker_offset, 2);
    assert!(is_native_clobber_safe_excluding(
        &marker_after,
        &excluded,
        true
    ));
}

#[test]
fn x86_mmx_m64_source_gate_rejects_malformed_encoding_state_and_ssa_shapes() {
    let exact = paddb_m64_function();
    let excluded = std::collections::HashMap::new();
    let mut malformed = Vec::new();

    let mut wrong_width = exact.clone();
    if let OpKind::VLoad { width, .. } = &mut wrong_width.blocks[0].ops[0].kind {
        *width = VecWidth::V128;
    }
    malformed.push(wrong_width);

    let mut wrong_load_hint = exact.clone();
    wrong_load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0x6F,
    });
    malformed.push(wrong_load_hint);

    let mut unsafe_address = exact.clone();
    if let OpKind::VLoad { addr, .. } = &mut unsafe_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(99)));
    }
    malformed.push(unsafe_address);

    let mut reused_temporary = exact.clone();
    if let OpKind::VAdd { src1, .. } = &mut reused_temporary.blocks[0].ops[2].kind {
        *src1 = VReg::Virtual(VirtualId(7));
    }
    malformed.push(reused_temporary);

    let mut wrong_opcode = exact.clone();
    wrong_opcode.blocks[0].ops[2].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0xFD,
    });
    malformed.push(wrong_opcode);

    let mut wrong_prefix = exact.clone();
    wrong_prefix.blocks[0].ops[2].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::OpSize,
        opcode: 0xFC,
    });
    malformed.push(wrong_prefix);

    let mut wrong_pc = exact.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(wrong_pc);

    let mut memory_marker = exact;
    if let OpKind::X86X87Control { addr, .. } = &mut memory_marker.blocks[0].ops[1].kind {
        *addr = Some(Address::Absolute(0x2000));
    }
    malformed.push(memory_marker);

    for function in malformed {
        assert!(
            sequence(&function).is_none(),
            "{:#?}",
            function.blocks[0].ops
        );
        assert!(!is_native_clobber_safe_excluding(
            &function, &excluded, true
        ));
        assert!(
            !x86_native_mmx_pairs_valid_excluding(&function, &excluded),
            "{:#?}",
            function.blocks[0].ops
        );
    }
}

#[test]
fn x86_mmx_m32_and_m16_sources_require_exact_helper_backed_fusion() {
    let excluded = std::collections::HashMap::new();
    let unpack = punpcklbw_m32_function();
    let exact = sequence(&unpack).expect("exact PUNPCKLBW m32 sequence");
    assert_eq!(exact.consumed, 4);
    assert_eq!(exact.marker_offset, 2);
    assert_eq!(exact.encoding.map, X86VecMap::Map0F);
    assert_eq!(exact.encoding.opcode, 0x60);
    assert_eq!(exact.encoding.dst_index, 3);
    assert_eq!(exact.encoding.immediate, None);
    assert_eq!(exact.encoding.mem_width, MemWidth::B4);
    assert!(!exact.encoding.requires_ssse3);
    assert!(is_native_clobber_safe_excluding(&unpack, &excluded, true));
    assert!(x86_native_mmx_pairs_valid_excluding(&unpack, &excluded));
    assert!(uses_x86_native_mmx_excluding(&unpack, &excluded));
    assert!(!uses_x86_native_vectors_excluding(&unpack, &excluded));
    assert!(sequence_with_mem(&unpack, false).is_none());
    assert!(!is_native_clobber_safe_excluding(&unpack, &excluded, false));

    let mut unpack_marker_after = unpack.clone();
    unpack_marker_after.blocks[0].ops.swap(2, 3);
    let exact = sequence(&unpack_marker_after).expect("post-unpack EnterMmx order");
    assert_eq!(exact.marker_offset, 3);
    assert!(is_native_clobber_safe_excluding(
        &unpack_marker_after,
        &excluded,
        true
    ));

    let pinsrw = pinsrw_m16_function();
    let exact = sequence(&pinsrw).expect("exact PINSRW m16 sequence");
    assert_eq!(exact.consumed, 3);
    assert_eq!(exact.marker_offset, 1);
    assert_eq!(exact.encoding.map, X86VecMap::Map0F);
    assert_eq!(exact.encoding.opcode, 0xC4);
    assert_eq!(exact.encoding.dst_index, 3);
    assert_eq!(exact.encoding.immediate, Some(2));
    assert_eq!(exact.encoding.mem_width, MemWidth::B2);
    assert!(!exact.encoding.requires_ssse3);
    assert!(is_native_clobber_safe_excluding(&pinsrw, &excluded, true));
    assert!(x86_native_mmx_pairs_valid_excluding(&pinsrw, &excluded));
    assert!(!uses_x86_native_vectors_excluding(&pinsrw, &excluded));

    let mut pinsrw_marker_after = pinsrw;
    pinsrw_marker_after.blocks[0].ops.swap(1, 2);
    let exact = sequence(&pinsrw_marker_after).expect("post-PINSRW EnterMmx order");
    assert_eq!(exact.marker_offset, 2);
    assert!(is_native_clobber_safe_excluding(
        &pinsrw_marker_after,
        &excluded,
        true
    ));
}

#[test]
fn x86_mmx_narrow_source_gate_rejects_width_sign_hint_pc_and_ssa_mismatches() {
    let exact = punpcklbw_m32_function();
    let mut malformed = Vec::new();

    let mut wrong_width = exact.clone();
    if let OpKind::Load { width, .. } = &mut wrong_width.blocks[0].ops[0].kind {
        *width = MemWidth::B8;
    }
    malformed.push(wrong_width);

    let mut signed_load = exact.clone();
    if let OpKind::Load { sign, .. } = &mut signed_load.blocks[0].ops[0].kind {
        *sign = SignExtend::Sign;
    }
    malformed.push(signed_load);

    let mut load_hint = exact.clone();
    load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(load_hint);

    let mut broadcast_hint = exact.clone();
    broadcast_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
    malformed.push(broadcast_hint);

    let mut wrong_broadcast_elem = exact.clone();
    if let OpKind::VBroadcast { elem, .. } = &mut wrong_broadcast_elem.blocks[0].ops[1].kind {
        *elem = VecElementType::I32;
    }
    malformed.push(wrong_broadcast_elem);

    let mut wrong_broadcast_lanes = exact.clone();
    if let OpKind::VBroadcast { lanes, .. } = &mut wrong_broadcast_lanes.blocks[0].ops[1].kind {
        *lanes = 2;
    }
    malformed.push(wrong_broadcast_lanes);

    let mut reused_scalar = exact.clone();
    if let OpKind::VInterleave { src2, .. } = &mut reused_scalar.blocks[0].ops[3].kind {
        *src2 = VReg::Virtual(VirtualId(7));
    }
    malformed.push(reused_scalar);

    let mut reused_broadcast = exact.clone();
    if let OpKind::VInterleave { src1, .. } = &mut reused_broadcast.blocks[0].ops[3].kind {
        *src1 = VReg::Virtual(VirtualId(8));
    }
    malformed.push(reused_broadcast);

    let mut high_unpack = exact.clone();
    if let OpKind::VInterleave { high, .. } = &mut high_unpack.blocks[0].ops[3].kind {
        *high = true;
    }
    high_unpack.blocks[0].ops[3].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0x68,
    });
    malformed.push(high_unpack);

    let mut wrong_opcode = exact.clone();
    wrong_opcode.blocks[0].ops[3].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0x61,
    });
    malformed.push(wrong_opcode);

    let mut wrong_prefix = exact.clone();
    wrong_prefix.blocks[0].ops[3].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::OpSize,
        opcode: 0x60,
    });
    malformed.push(wrong_prefix);

    let mut wrong_pc = exact;
    wrong_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(wrong_pc);

    for function in malformed {
        assert_sequence_rejected(&function);
    }
}

#[test]
fn x86_mmx_pinsrw_m16_gate_rejects_noncanonical_lane_and_encoding_shapes() {
    let exact = pinsrw_m16_function();
    let mut malformed = Vec::new();

    let mut wrong_width = exact.clone();
    if let OpKind::Load { width, .. } = &mut wrong_width.blocks[0].ops[0].kind {
        *width = MemWidth::B4;
    }
    malformed.push(wrong_width);

    let mut signed_load = exact.clone();
    if let OpKind::Load { sign, .. } = &mut signed_load.blocks[0].ops[0].kind {
        *sign = SignExtend::Sign;
    }
    malformed.push(signed_load);

    let mut wrong_lane = exact.clone();
    if let OpKind::VInsertLane { lane, .. } = &mut wrong_lane.blocks[0].ops[2].kind {
        *lane = 4;
    }
    malformed.push(wrong_lane);

    let mut wrong_elem = exact.clone();
    if let OpKind::VInsertLane { elem, .. } = &mut wrong_elem.blocks[0].ops[2].kind {
        *elem = VecElementType::I32;
    }
    malformed.push(wrong_elem);

    let mut nondestructive = exact.clone();
    if let OpKind::VInsertLane { vec, .. } = &mut nondestructive.blocks[0].ops[2].kind {
        *vec = VReg::Arch(ArchReg::X86(X86Reg::Mm(2)));
    }
    malformed.push(nondestructive);

    let mut wrong_scalar = exact.clone();
    if let OpKind::VInsertLane { scalar, .. } = &mut wrong_scalar.blocks[0].ops[2].kind {
        *scalar = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    }
    malformed.push(wrong_scalar);

    let mut wrong_opcode = exact.clone();
    wrong_opcode.blocks[0].ops[2].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0xC5,
    });
    malformed.push(wrong_opcode);

    let mut wrong_prefix = exact;
    wrong_prefix.blocks[0].ops[2].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::OpSize,
        opcode: 0xC4,
    });
    malformed.push(wrong_prefix);

    for function in malformed {
        assert_sequence_rejected(&function);
    }
}

#[test]
fn x86_mmx_m64_gate_rejects_low_unpack_that_would_overread_m32() {
    let mut synthetic = paddb_m64_function();
    let temporary = VReg::Virtual(VirtualId(7));
    let mm3 = VReg::Arch(ArchReg::X86(X86Reg::Mm(3)));
    synthetic.blocks[0].ops[2].kind = OpKind::VInterleave {
        dst: mm3,
        src1: mm3,
        src2: temporary,
        elem: VecElementType::I8,
        lanes: 8,
        block_lanes: 8,
        high: false,
    };
    synthetic.blocks[0].ops[2].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0x60,
    });

    assert_sequence_rejected(&synthetic);
}

#[test]
fn x86_mmx_variable_shift_m64_requires_exact_extract_chain() {
    let loaded = VReg::Virtual(VirtualId(7));
    let count = VReg::Virtual(VirtualId(8));
    let mm3 = VReg::Arch(ArchReg::X86(X86Reg::Mm(3)));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::VLoad {
            dst: loaded,
            addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rbx))),
            width: VecWidth::V64,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::VExtractLane {
            dst: count,
            vec: loaded,
            lane: 0,
            elem: VecElementType::I64,
            sign: SignExtend::Zero,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::X86PackedShift {
            dst: mm3,
            src: mm3,
            count,
            width: VecWidth::V64,
            elem: VecElementType::I16,
            shift: ShiftOp::Asr,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EnterMmx,
            addr: None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[2].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0xE1,
    });

    let exact = sequence(&function).expect("exact PSRAW m64 extract chain");
    assert_eq!(exact.consumed, 4);
    assert_eq!(exact.marker_offset, 3);
    assert_eq!(exact.encoding.opcode, 0xE1);
    assert!(is_native_clobber_safe_excluding(
        &function,
        &std::collections::HashMap::new(),
        true
    ));

    let mut wrong_lane = function.clone();
    if let OpKind::VExtractLane { lane, .. } = &mut wrong_lane.blocks[0].ops[1].kind {
        *lane = 1;
    }
    assert!(sequence(&wrong_lane).is_none());
    assert!(!is_native_clobber_safe_excluding(
        &wrong_lane,
        &std::collections::HashMap::new(),
        true
    ));

    let mut missing_hint = function;
    missing_hint.blocks[0].ops[2].x86_hint = None;
    assert!(sequence(&missing_hint).is_none());
    assert!(!is_native_clobber_safe_excluding(
        &missing_hint,
        &std::collections::HashMap::new(),
        true
    ));
}
