//! Fail-closed native admission and state detection for MXCSR memory operations.

use super::*;
use crate::smir::ir::ops::{SmirOp, X86OpHint, X86SsePrefix, X86VecMap};
use crate::smir::ir::types::{OpId, VecWidth};
use crate::smir::lower::x86_64::{x86_load_mxcsr_shape_valid, x86_store_mxcsr_shape_valid};

const PC: u64 = 0x1000;

fn store(addr: Address, hint: Option<X86OpHint>) -> SmirOp {
    store_with_apx(addr, hint, false)
}

fn store_with_apx(addr: Address, hint: Option<X86OpHint>, requires_apx: bool) -> SmirOp {
    let kind = OpKind::X86StoreMxcsr { addr, requires_apx };
    match hint {
        Some(hint) => SmirOp::with_hint(OpId(0), PC, kind, hint),
        None => SmirOp::new(OpId(0), PC, kind),
    }
}

fn load(addr: Address, hint: Option<X86OpHint>, requires_apx: bool, next_pc: u64) -> SmirOp {
    let kind = OpKind::X86LoadMxcsr {
        addr,
        requires_apx,
        next_pc,
    };
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
fn mxcsr_store_gate_requires_helpers_and_accepts_exact_legacy_vex_and_rex2_shapes() {
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

    let rex2 = store_with_apx(
        Address::BaseIndexScale {
            base: Some(x86(X86Reg::R20)),
            index: x86(X86Reg::R31),
            scale: 8,
            disp: 0x40,
            disp_size: DispSize::Disp8,
        },
        None,
        true,
    );
    assert!(x86_store_mxcsr_shape_valid(&rex2));
    assert!(gate(rex2, true));
}

#[test]
fn mxcsr_load_gate_requires_helpers_and_accepts_exact_legacy_vex_and_rex2_shapes() {
    let addresses = [
        Address::Absolute(0x4000),
        Address::Direct(x86(X86Reg::Rsp)),
        Address::BaseOffset {
            base: x86(X86Reg::Rbp),
            offset: -4,
            disp_size: DispSize::Disp8,
        },
        Address::X86Addr32(Box::new(Address::SegmentRel {
            segment: x86(X86Reg::FsBase),
            base: Some(x86(X86Reg::Rax)),
            index: Some(x86(X86Reg::Rcx)),
            scale: 2,
            disp: 0x20,
        })),
    ];

    for addr in addresses {
        for (hint, next_pc) in [
            (None, PC + 3),
            (Some(vex_hint(false)), PC + 4),
            (Some(vex_hint(true)), PC + 15),
        ] {
            let op = load(addr.clone(), hint, false, next_pc);
            assert!(!op.kind.is_jit_safe(), "{op:?}");
            assert!(!op.is_jit_safe(), "{op:?}");
            assert!(x86_load_mxcsr_shape_valid(&op), "{op:?}");
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

    let rex2 = load(
        Address::BaseIndexScale {
            base: Some(x86(X86Reg::R20)),
            index: x86(X86Reg::R31),
            scale: 8,
            disp: 0x40,
            disp_size: DispSize::Disp8,
        },
        None,
        true,
        PC + 4,
    );
    assert!(x86_load_mxcsr_shape_valid(&rex2));
    assert!(gate(rex2, true));
}

#[test]
fn mxcsr_gates_reject_malformed_hints_frontiers_and_non_x86_addresses() {
    let exact_addr = Address::Direct(x86(X86Reg::Rax));

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

    for malformed in [
        load(exact_addr.clone(), None, false, PC + 2),
        load(exact_addr.clone(), None, false, PC + 16),
        load(exact_addr.clone(), None, true, PC + 3),
        load(exact_addr.clone(), Some(vex_hint(false)), true, PC + 4),
        load(
            exact_addr.clone(),
            Some(X86OpHint::RexByteReg),
            false,
            PC + 3,
        ),
        load(Address::Direct(x86(X86Reg::R31)), None, false, PC + 4),
        load(
            Address::Direct(x86(X86Reg::R31)),
            Some(vex_hint(false)),
            false,
            PC + 4,
        ),
        load(Address::Direct(VReg::virt(0)), None, false, PC + 3),
        load(Address::Direct(arm_x(0)), None, false, PC + 3),
        load(Address::GpRel { offset: 0 }, None, false, PC + 3),
    ] {
        assert!(!x86_load_mxcsr_shape_valid(&malformed), "{malformed:?}");
        assert!(!gate(malformed, true));
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

    let unguarded_egpr = store(Address::Direct(x86(X86Reg::R31)), None);
    assert!(!x86_store_mxcsr_shape_valid(&unguarded_egpr));
    assert!(!gate(unguarded_egpr, true));

    // VEX cannot carry APX provenance or encode an EGPR address.
    for hint in [Some(vex_hint(false)), Some(vex_hint(true))] {
        let op = store_with_apx(Address::Direct(x86(X86Reg::R31)), hint, true);
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

    let mut store_function = function(store(
        Address::Direct(x86(X86Reg::Rsp)),
        Some(vex_hint(true)),
    ));
    let excluded = std::collections::HashMap::new();
    assert!(uses_x86_mxcsr_state_excluding(&store_function, &excluded));

    let mut excluded_entry = std::collections::HashMap::new();
    excluded_entry.insert(store_function.entry, PC);
    assert!(!uses_x86_mxcsr_state_excluding(
        &store_function,
        &excluded_entry
    ));

    crate::smir::optimize::optimize_function(
        &mut store_function,
        crate::smir::optimize::OptLevel::O2,
    );
    assert_eq!(
        store_function
            .entry_block()
            .unwrap()
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86StoreMxcsr { .. }))
            .count(),
        1
    );
    assert!(matches!(
        store_function.entry_block().unwrap().ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86StoreMxcsr {
                requires_apx: false,
                ..
            },
            ..
        }]
    ));
    assert!(uses_x86_mxcsr_state_excluding(&store_function, &excluded));
    assert!(is_native_clobber_safe_excluding(
        &store_function,
        &excluded,
        true
    ));

    let mut load_function = function(load(
        Address::Direct(x86(X86Reg::Rbx)),
        Some(vex_hint(false)),
        false,
        PC + 4,
    ));
    crate::smir::optimize::optimize_function(
        &mut load_function,
        crate::smir::optimize::OptLevel::O2,
    );
    assert!(matches!(
        load_function.entry_block().unwrap().ops.as_slice(),
        [SmirOp {
            kind: OpKind::X86LoadMxcsr {
                requires_apx: false,
                next_pc,
                ..
            },
            ..
        }] if *next_pc == PC + 4
    ));
    assert!(uses_x86_mxcsr_state_excluding(&load_function, &excluded));
    assert!(is_native_clobber_safe_excluding(
        &load_function,
        &excluded,
        true
    ));
}
