//! MMX memory-helper admission tests.

use super::*;
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
}
