//! Fail-closed native admission and state detection for STMXCSR/VSTMXCSR.

use super::*;
use crate::smir::ir::ops::{SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{OpId, VecWidth};
use crate::smir::lower::x86_64::x86_store_mxcsr_shape_valid;

const PC: u64 = 0x1000;

fn store(addr: Address, hint: Option<X86OpHint>) -> SmirOp {
    let kind = OpKind::X86StoreMxcsr { addr };
    match hint {
        Some(hint) => SmirOp::with_hint(OpId(0), PC, kind, hint),
        None => SmirOp::new(OpId(0), PC, kind),
    }
}

fn vex_hint(w: bool) -> X86OpHint {
    X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::None,
        opcode: 0xAE,
        width: VecWidth::V128,
        w,
    }
}

fn function(op: SmirOp) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
    builder.push_op(PC, op.kind.clone());
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[0] = op;
    function
}

fn gate(op: SmirOp, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(&function(op), &std::collections::HashMap::new(), allow_mem)
}

#[test]
fn mxcsr_store_gate_requires_helpers_and_accepts_exact_legacy_and_vex_wig_shapes() {
    let addresses = [
        Address::Absolute(0x4000),
        Address::Direct(x86(X86Reg::Rsp)),
        Address::BaseOffset {
            base: x86(X86Reg::Rbp),
            offset: -4,
            disp_size: DispSize::Disp8,
        },
        Address::SegmentRel {
            segment: x86(X86Reg::FsBase),
            base: Some(x86(X86Reg::Rax)),
            index: Some(x86(X86Reg::Rcx)),
            scale: 2,
            disp: 0x20,
        },
        Address::SegmentRel {
            segment: x86(X86Reg::GsBase),
            base: Some(x86(X86Reg::R15)),
            index: None,
            scale: 1,
            disp: -8,
        },
    ];

    for addr in addresses {
        for hint in [None, Some(vex_hint(false)), Some(vex_hint(true))] {
            let op = store(addr.clone(), hint);
            assert!(!op.kind.is_jit_safe(), "{op:?}");
            assert!(!op.is_jit_safe(), "{op:?}");
            assert!(x86_store_mxcsr_shape_valid(&op), "{op:?}");
            assert!(!gate(op.clone(), false), "{op:?}");
            assert!(gate(op.clone(), true), "{op:?}");
            assert!(x86_jit_op_uses_mem_helper(&op.kind), "{op:?}");
            assert!(uses_x86_mxcsr_state_excluding(
                &function(op.clone()),
                &std::collections::HashMap::new()
            ));
            assert!(!is_x86_aarch64_native_clobber_safe_excluding(
                &function(op),
                &std::collections::HashMap::new(),
            ));
        }
    }
}

#[test]
fn mxcsr_store_gate_rejects_loads_malformed_hints_and_non_x86_addresses() {
    let exact_addr = Address::Direct(x86(X86Reg::Rax));
    assert!(!gate(
        SmirOp::new(
            OpId(0),
            PC,
            OpKind::X86LoadMxcsr {
                addr: exact_addr.clone(),
            },
        ),
        true,
    ));

    for hint in [
        X86OpHint::VexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::None,
            opcode: 0xAE,
            width: VecWidth::V128,
            w: false,
        },
        X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::OpSize,
            opcode: 0xAE,
            width: VecWidth::V128,
            w: false,
        },
        X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::None,
            opcode: 0xAF,
            width: VecWidth::V128,
            w: false,
        },
        X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::None,
            opcode: 0xAE,
            width: VecWidth::V256,
            w: false,
        },
        X86OpHint::EvexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::None,
            opcode: 0xAE,
            width: VecWidth::V128,
            w: false,
        },
        X86OpHint::RexByteReg,
    ] {
        let op = store(exact_addr.clone(), Some(hint));
        assert!(!x86_store_mxcsr_shape_valid(&op), "{op:?}");
        assert!(!gate(op, true));
    }

    for addr in [
        Address::Direct(VReg::virt(0)),
        Address::Direct(arm_x(0)),
        Address::GpRel { offset: 0 },
    ] {
        let op = store(addr, None);
        assert!(!x86_store_mxcsr_shape_valid(&op), "{op:?}");
        assert!(!gate(op, true));
    }

    // VEX has no EGPR address extension, and the legacy operation itself does
    // not retain enough provenance to prove its preceding REX2/APX guard.
    for hint in [None, Some(vex_hint(false)), Some(vex_hint(true))] {
        let op = store(Address::Direct(x86(X86Reg::R31)), hint);
        assert!(!x86_store_mxcsr_shape_valid(&op), "{op:?}");
        assert!(!gate(op, true));
    }
}

#[test]
fn mxcsr_state_marker_is_append_only_exclusion_aware_and_retained_at_o2() {
    assert_eq!(GuestRegs::default().mxcsr_state_active, 0);
    assert_eq!(
        std::mem::offset_of!(GuestRegs, mxcsr_state_active),
        std::mem::offset_of!(GuestRegs, xmm_state_active) + std::mem::size_of::<u64>()
    );

    let mut function = function(store(
        Address::Direct(x86(X86Reg::Rsp)),
        Some(vex_hint(true)),
    ));
    let excluded = std::collections::HashMap::new();
    assert!(uses_x86_mxcsr_state_excluding(&function, &excluded));

    let mut excluded_entry = std::collections::HashMap::new();
    excluded_entry.insert(function.entry, PC);
    assert!(!uses_x86_mxcsr_state_excluding(&function, &excluded_entry));

    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);
    assert_eq!(
        function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86StoreMxcsr { .. }))
            .count(),
        1
    );
    assert!(uses_x86_mxcsr_state_excluding(&function, &excluded));
    assert!(is_native_clobber_safe_excluding(&function, &excluded, true));
}
