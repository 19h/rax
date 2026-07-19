//! MMX memory-helper admission tests.

use super::*;
use crate::smir::ir::ops::X86VecAlign;
use crate::smir::lower::runtime::*;

fn mmx_movq_memory_function(is_load: bool) -> crate::smir::ir::SmirFunction {
    let mm7 = VReg::Arch(ArchReg::X86(X86Reg::Mm(7)));
    let addr = Address::BaseOffset {
        base: VReg::Arch(ArchReg::X86(X86Reg::Rbx)),
        offset: 8,
        disp_size: DispSize::Disp8,
    };
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        if is_load {
            OpKind::VLoad {
                dst: mm7,
                addr,
                width: VecWidth::V64,
            }
        } else {
            OpKind::VStore {
                src: mm7,
                addr,
                width: VecWidth::V64,
            }
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
    function.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseMov {
        prefix: X86SsePrefix::None,
        opcode: if is_load { 0x6F } else { 0x7F },
    });
    function
}

fn mmx_movntq_memory_function() -> crate::smir::ir::SmirFunction {
    let mut function = mmx_movq_memory_function(false);
    function.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    function
}

fn mmx_scalar_memory_function(is_load: bool, width: OpWidth) -> crate::smir::ir::SmirFunction {
    let temporary = VReg::Virtual(VirtualId(7));
    let mm3 = VReg::Arch(ArchReg::X86(X86Reg::Mm(3)));
    let mem_width = match width {
        OpWidth::W32 => MemWidth::B4,
        OpWidth::W64 => MemWidth::B8,
        _ => panic!("test requires W32 or W64"),
    };
    let addr = Address::BaseOffset {
        base: VReg::Arch(ArchReg::X86(X86Reg::Rbx)),
        offset: 8,
        disp_size: DispSize::Disp8,
    };
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    if is_load {
        builder.push_op(
            0x1000,
            OpKind::Load {
                dst: temporary,
                addr,
                width: mem_width,
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
            OpKind::X86MovdQ {
                dst: mm3,
                src: temporary,
                width,
                zero_upper: false,
            },
        );
    } else {
        builder.push_op(
            0x1000,
            OpKind::X86MovdQ {
                dst: temporary,
                src: mm3,
                width,
                zero_upper: false,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Store {
                src: temporary,
                addr,
                width: mem_width,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            },
        );
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    let operation = if is_load { 2 } else { 0 };
    function.blocks[0].ops[operation].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: if is_load { 0x6E } else { 0x7E },
    });
    function
}

fn scalar_transfer_sequence(
    function: &crate::smir::ir::SmirFunction,
    allow_mem: bool,
) -> Option<X86MmxScalarMemoryTransferSequence> {
    let mut definitions = std::collections::HashMap::new();
    let mut uses = std::collections::HashMap::new();
    for op in &function.blocks[0].ops {
        for reg in op.kind.dests() {
            if matches!(reg, VReg::Virtual(_)) {
                *definitions.entry(reg).or_insert(0usize) += 1;
            }
        }
        for reg in op.kind.source_vregs() {
            if matches!(reg, VReg::Virtual(_)) {
                *uses.entry(reg).or_insert(0usize) += 1;
            }
        }
    }
    x86_jit_mmx_scalar_memory_transfer_sequence(
        &function.blocks[0],
        0,
        allow_mem,
        &definitions,
        &uses,
    )
}

fn assert_scalar_transfer_rejected(function: &crate::smir::ir::SmirFunction) {
    assert!(
        scalar_transfer_sequence(function, true).is_none(),
        "unexpected scalar MMX transfer: {:#?}",
        function.blocks[0].ops
    );
    assert!(!is_native_clobber_safe_excluding(
        function,
        &std::collections::HashMap::new(),
        true
    ));
}

#[test]
fn x86_mmx_movq_memory_helpers_require_exact_state_pairs() {
    let excluded = std::collections::HashMap::new();
    for is_load in [true, false] {
        let function = mmx_movq_memory_function(is_load);
        assert!(
            is_native_clobber_safe_excluding(&function, &excluded, true),
            "exact MMX MOVQ memory form should pass the helper-backed gate"
        );
        assert!(x86_native_mmx_pairs_valid_excluding(&function, &excluded));
        assert!(uses_x86_native_mmx_excluding(&function, &excluded));
        assert!(!uses_x86_native_vectors_excluding(&function, &excluded));
    }
}

#[test]
fn x86_mmx_movq_memory_gate_rejects_malformed_and_unpaired_shapes() {
    let excluded = std::collections::HashMap::new();
    let exact_function = mmx_movq_memory_function(true);
    let exact = &exact_function.blocks[0].ops[0];
    assert!(x86_jit_mmx_mem_shape_valid(exact));
    assert!(!is_native_clobber_safe_excluding(
        &exact_function,
        &excluded,
        false
    ));

    let mut malformed = Vec::new();
    let mut wrong_opcode = exact.clone();
    wrong_opcode.x86_hint = Some(X86OpHint::SseMov {
        prefix: X86SsePrefix::None,
        opcode: 0x7F,
    });
    malformed.push(wrong_opcode);

    let mut wrong_prefix = exact.clone();
    wrong_prefix.x86_hint = Some(X86OpHint::SseMov {
        prefix: X86SsePrefix::OpSize,
        opcode: 0x6F,
    });
    malformed.push(wrong_prefix);

    let mut wrong_width = exact.clone();
    if let OpKind::VLoad { width, .. } = &mut wrong_width.kind {
        *width = VecWidth::V128;
    }
    malformed.push(wrong_width);

    let mut wrong_register = exact.clone();
    if let OpKind::VLoad { dst, .. } = &mut wrong_register.kind {
        *dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(7)));
    }
    malformed.push(wrong_register);

    let mut virtual_register = exact.clone();
    if let OpKind::VLoad { dst, .. } = &mut virtual_register.kind {
        *dst = VReg::Virtual(VirtualId(7));
    }
    malformed.push(virtual_register);

    for op in malformed {
        assert!(!x86_jit_mmx_mem_shape_valid(&op), "{op:?}");
    }

    let mut orphan = exact_function.clone();
    orphan.blocks[0].ops.pop();
    assert!(!x86_native_mmx_pairs_valid_excluding(&orphan, &excluded));

    let mut wrong_pc = exact_function;
    wrong_pc.blocks[0].ops[1].guest_pc = 0x1001;
    assert!(!x86_native_mmx_pairs_valid_excluding(&wrong_pc, &excluded));

    for is_load in [true, false] {
        let mut marker_before_fault = mmx_movq_memory_function(is_load);
        marker_before_fault.blocks[0].ops.swap(0, 1);
        assert!(!x86_native_mmx_pairs_valid_excluding(
            &marker_before_fault,
            &excluded
        ));
    }
}

#[test]
fn x86_mmx_movntq_memory_gate_requires_exact_hint_shape_and_fault_order() {
    let excluded = std::collections::HashMap::new();
    let exact_function = mmx_movntq_memory_function();
    let exact = &exact_function.blocks[0].ops[0];
    assert!(x86_jit_mmx_mem_shape_valid(exact));
    assert!(is_native_clobber_safe_excluding(
        &exact_function,
        &excluded,
        true
    ));
    assert!(!is_native_clobber_safe_excluding(
        &exact_function,
        &excluded,
        false
    ));
    assert!(x86_native_mmx_pairs_valid_excluding(
        &exact_function,
        &excluded
    ));
    assert!(uses_x86_native_mmx_excluding(&exact_function, &excluded));
    assert!(!uses_x86_native_vectors_excluding(
        &exact_function,
        &excluded
    ));

    let mut malformed = Vec::new();
    let mut missing_hint = exact.clone();
    missing_hint.x86_hint = None;
    malformed.push(missing_hint);

    let mut aligned = exact.clone();
    aligned.x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
    malformed.push(aligned);

    let mut synthetic_opcode = exact.clone();
    synthetic_opcode.x86_hint = Some(X86OpHint::SseMov {
        prefix: X86SsePrefix::None,
        opcode: 0xE7,
    });
    malformed.push(synthetic_opcode);

    let mut wrong_width = exact.clone();
    if let OpKind::VStore { width, .. } = &mut wrong_width.kind {
        *width = VecWidth::V128;
    }
    malformed.push(wrong_width);

    let mut wrong_register = exact.clone();
    if let OpKind::VStore { src, .. } = &mut wrong_register.kind {
        *src = VReg::Arch(ArchReg::X86(X86Reg::Xmm(7)));
    }
    malformed.push(wrong_register);

    let mut unsafe_address = exact.clone();
    if let OpKind::VStore { addr, .. } = &mut unsafe_address.kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(7)));
    }
    malformed.push(unsafe_address);

    let mut wrong_direction = exact.clone();
    if let OpKind::VStore { src, addr, width } = wrong_direction.kind {
        wrong_direction.kind = OpKind::VLoad {
            dst: src,
            addr,
            width,
        };
    }
    malformed.push(wrong_direction);

    for op in malformed {
        assert!(!x86_jit_mmx_mem_shape_valid(&op), "{op:?}");
    }

    let mut marker_before_fault = exact_function;
    marker_before_fault.blocks[0].ops.swap(0, 1);
    assert!(!x86_native_mmx_pairs_valid_excluding(
        &marker_before_fault,
        &excluded
    ));
}

#[test]
fn x86_mmx_scalar_memory_transfers_require_exact_width_direction_and_state() {
    let excluded = std::collections::HashMap::new();
    for is_load in [true, false] {
        for width in [OpWidth::W32, OpWidth::W64] {
            let function = mmx_scalar_memory_function(is_load, width);
            let sequence = scalar_transfer_sequence(&function, true)
                .expect("exact MMX scalar-memory transfer sequence");
            assert_eq!(sequence.consumed, 3);
            assert_eq!(sequence.memory_offset, usize::from(!is_load));
            assert_eq!(sequence.marker_offset, if is_load { 1 } else { 2 });
            assert_eq!(sequence.encoding.is_load, is_load);
            assert_eq!(sequence.encoding.opcode, if is_load { 0x6E } else { 0x7E });
            assert_eq!(sequence.encoding.mm_index, 3);
            assert_eq!(
                sequence.encoding.mem_width,
                if width == OpWidth::W32 {
                    MemWidth::B4
                } else {
                    MemWidth::B8
                }
            );
            assert_eq!(sequence.encoding.rex_w, width == OpWidth::W64);
            assert!(is_native_clobber_safe_excluding(&function, &excluded, true));
            assert!(x86_native_mmx_pairs_valid_excluding(&function, &excluded));
            assert!(uses_x86_native_mmx_excluding(&function, &excluded));
            assert!(!uses_x86_native_vectors_excluding(&function, &excluded));
            assert!(scalar_transfer_sequence(&function, false).is_none());
            assert!(!is_native_clobber_safe_excluding(
                &function, &excluded, false
            ));
        }
    }

    let mut marker_after = mmx_scalar_memory_function(true, OpWidth::W32);
    marker_after.blocks[0].ops.swap(1, 2);
    let sequence =
        scalar_transfer_sequence(&marker_after, true).expect("post-load-operation EnterMmx order");
    assert_eq!(sequence.marker_offset, 2);
    assert!(is_native_clobber_safe_excluding(
        &marker_after,
        &excluded,
        true
    ));
}

#[test]
fn x86_mmx_scalar_memory_load_gate_rejects_malformed_chains() {
    let exact = mmx_scalar_memory_function(true, OpWidth::W32);
    let mut malformed = Vec::new();

    let mut wrong_memory_width = exact.clone();
    if let OpKind::Load { width, .. } = &mut wrong_memory_width.blocks[0].ops[0].kind {
        *width = MemWidth::B8;
    }
    malformed.push(wrong_memory_width);

    let mut signed_load = exact.clone();
    if let OpKind::Load { sign, .. } = &mut signed_load.blocks[0].ops[0].kind {
        *sign = SignExtend::Sign;
    }
    malformed.push(signed_load);

    let mut load_hint = exact.clone();
    load_hint.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(load_hint);

    let mut unsafe_address = exact.clone();
    if let OpKind::Load { addr, .. } = &mut unsafe_address.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(99)));
    }
    malformed.push(unsafe_address);

    let mut wrong_operation_width = exact.clone();
    if let OpKind::X86MovdQ { width, .. } = &mut wrong_operation_width.blocks[0].ops[2].kind {
        *width = OpWidth::W64;
    }
    malformed.push(wrong_operation_width);

    let mut zero_upper = exact.clone();
    if let OpKind::X86MovdQ { zero_upper, .. } = &mut zero_upper.blocks[0].ops[2].kind {
        *zero_upper = true;
    }
    malformed.push(zero_upper);

    let mut wrong_opcode = exact.clone();
    wrong_opcode.blocks[0].ops[2].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0x7E,
    });
    malformed.push(wrong_opcode);

    let mut wrong_prefix = exact.clone();
    wrong_prefix.blocks[0].ops[2].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::OpSize,
        opcode: 0x6E,
    });
    malformed.push(wrong_prefix);

    let mut wrong_mm = exact.clone();
    if let OpKind::X86MovdQ { dst, .. } = &mut wrong_mm.blocks[0].ops[2].kind {
        *dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(3)));
    }
    malformed.push(wrong_mm);

    let mut wrong_pc = exact.clone();
    wrong_pc.blocks[0].ops[2].guest_pc += 1;
    malformed.push(wrong_pc);

    let mut memory_marker = exact;
    if let OpKind::X86X87Control { addr, .. } = &mut memory_marker.blocks[0].ops[1].kind {
        *addr = Some(Address::Absolute(0x2000));
    }
    malformed.push(memory_marker);

    for function in malformed {
        assert_scalar_transfer_rejected(&function);
    }
}

#[test]
fn x86_mmx_scalar_memory_store_gate_rejects_malformed_and_fault_unsafe_chains() {
    let exact = mmx_scalar_memory_function(false, OpWidth::W32);
    let mut malformed = Vec::new();

    let mut wrong_memory_width = exact.clone();
    if let OpKind::Store { width, .. } = &mut wrong_memory_width.blocks[0].ops[1].kind {
        *width = MemWidth::B8;
    }
    malformed.push(wrong_memory_width);

    let mut store_hint = exact.clone();
    store_hint.blocks[0].ops[1].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(store_hint);

    let mut wrong_store_source = exact.clone();
    if let OpKind::Store { src, .. } = &mut wrong_store_source.blocks[0].ops[1].kind {
        *src = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    }
    malformed.push(wrong_store_source);

    let mut unsafe_address = exact.clone();
    if let OpKind::Store { addr, .. } = &mut unsafe_address.blocks[0].ops[1].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(99)));
    }
    malformed.push(unsafe_address);

    let mut wrong_operation_width = exact.clone();
    if let OpKind::X86MovdQ { width, .. } = &mut wrong_operation_width.blocks[0].ops[0].kind {
        *width = OpWidth::W64;
    }
    malformed.push(wrong_operation_width);

    let mut wrong_opcode = exact.clone();
    wrong_opcode.blocks[0].ops[0].x86_hint = Some(X86OpHint::SseOp {
        prefix: X86SsePrefix::None,
        opcode: 0x6E,
    });
    malformed.push(wrong_opcode);

    let mut marker_before_fault = exact.clone();
    marker_before_fault.blocks[0].ops.swap(1, 2);
    malformed.push(marker_before_fault);

    let mut wrong_pc = exact.clone();
    wrong_pc.blocks[0].ops[1].guest_pc += 1;
    malformed.push(wrong_pc);

    let mut reused_temporary = exact;
    let extra = reused_temporary.blocks[0].ops[1].clone();
    reused_temporary.blocks[0].ops.push(extra);
    malformed.push(reused_temporary);

    for function in malformed {
        assert_scalar_transfer_rejected(&function);
    }
}
