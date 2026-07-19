//! Helper-backed MMX m64-source admission tests.

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

fn sequence(function: &crate::smir::ir::SmirFunction) -> Option<X86MmxM64SourceSequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_mmx_m64_source_sequence(&function.blocks[0], 0, true, &definitions, &uses)
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
