//! Fail-closed runtime admission for VEX-encoded AVX-512 opmask operations.

use super::*;
use crate::smir::ir::ops::{
    SmirOp, X86OpmaskBinaryKind, X86OpmaskMoveDestination, X86OpmaskMoveSource, X86OpmaskOp,
    X86OpmaskShiftKind, X86OpmaskTestKind,
};
use crate::smir::ir::types::OpId;
use crate::smir::lower::x86_64::{x86_opmask_native_shape_valid, x86_opmask_needs_avx512dq};

fn k(index: u8) -> VReg {
    x86(X86Reg::K(index))
}

fn function_with(op: SmirOp) -> crate::smir::ir::SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    let hint = op.x86_hint;
    builder.push_op(op.guest_pc, op.kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    function.blocks[0].ops[0].x86_hint = hint;
    function
}

fn plain_function(op: X86OpmaskOp) -> crate::smir::ir::SmirFunction {
    function_with(SmirOp::new(OpId(0), 0x1000, OpKind::X86Opmask(op)))
}

fn gate(op: X86OpmaskOp, allow_mem: bool) -> bool {
    is_native_clobber_safe_excluding(
        &plain_function(op),
        &std::collections::HashMap::new(),
        allow_mem,
    )
}

fn register_shapes() -> Vec<X86OpmaskOp> {
    vec![
        X86OpmaskOp::MoveToMask {
            dst: k(1),
            src: X86OpmaskMoveSource::Mask(k(2)),
            width: OpWidth::W8,
        },
        X86OpmaskOp::MoveToMask {
            dst: k(1),
            src: X86OpmaskMoveSource::Gpr(x86(X86Reg::R15)),
            width: OpWidth::W64,
        },
        X86OpmaskOp::MoveFromMask {
            dst: X86OpmaskMoveDestination::Gpr(x86(X86Reg::R8)),
            src: k(1),
            width: OpWidth::W16,
        },
        X86OpmaskOp::Not {
            dst: k(1),
            src: k(2),
            width: OpWidth::W64,
        },
        X86OpmaskOp::Binary {
            kind: X86OpmaskBinaryKind::AndNot,
            dst: k(1),
            src1: k(2),
            src2: k(3),
            width: OpWidth::W32,
        },
        X86OpmaskOp::Unpack {
            dst: k(1),
            src1: k(2),
            src2: k(3),
            width: OpWidth::W16,
        },
        X86OpmaskOp::Shift {
            kind: X86OpmaskShiftKind::Left,
            dst: k(1),
            src: k(2),
            width: OpWidth::W8,
            count: 0xFF,
        },
        X86OpmaskOp::Test {
            kind: X86OpmaskTestKind::Or,
            src1: k(1),
            src2: k(2),
            width: OpWidth::W64,
        },
    ]
}

#[test]
fn target_gate_admits_every_exact_register_shape_and_rejects_aarch64_paths() {
    for opmask in register_shapes() {
        let kind = OpKind::X86Opmask(opmask.clone());
        let op = SmirOp::new(OpId(0), 0x1000, kind.clone());
        assert!(x86_opmask_native_shape_valid(&opmask), "{opmask:?}");
        assert!(is_x86_native_vector_op(&kind), "{opmask:?}");
        assert!(x86_native_vector_smir_op(&op), "{opmask:?}");
        assert!(!kind.is_jit_safe(), "generic admission must remain closed");
        assert!(!op.is_jit_safe(), "generic admission must remain closed");
        assert!(gate(opmask.clone(), false), "{opmask:?}");
        assert!(!aarch64_gate(vec![kind.clone()], false), "{opmask:?}");
        assert!(!x86_aarch64_gate(vec![kind]), "{opmask:?}");
    }

    for opmask in [
        X86OpmaskOp::MoveToMask {
            dst: k(1),
            src: X86OpmaskMoveSource::Gpr(x86(X86Reg::Rsp)),
            width: OpWidth::W64,
        },
        X86OpmaskOp::MoveFromMask {
            dst: X86OpmaskMoveDestination::Gpr(x86(X86Reg::Rbp)),
            src: k(1),
            width: OpWidth::W32,
        },
    ] {
        assert!(
            gate(opmask, false),
            "RSP/RBP KMOV is explicitly state-backed"
        );
    }
}

#[test]
fn target_gate_rejects_malformed_operands_widths_addresses_and_hints() {
    for (label, opmask) in [
        (
            "K8",
            X86OpmaskOp::Not {
                dst: k(8),
                src: k(1),
                width: OpWidth::W16,
            },
        ),
        (
            "virtual K source",
            X86OpmaskOp::Not {
                dst: k(1),
                src: VReg::Virtual(VirtualId(0)),
                width: OpWidth::W16,
            },
        ),
        (
            "APX GPR",
            X86OpmaskOp::MoveToMask {
                dst: k(1),
                src: X86OpmaskMoveSource::Gpr(x86(X86Reg::R16)),
                width: OpWidth::W64,
            },
        ),
        (
            "W128",
            X86OpmaskOp::Binary {
                kind: X86OpmaskBinaryKind::Xor,
                dst: k(1),
                src1: k(2),
                src2: k(3),
                width: OpWidth::W128,
            },
        ),
        (
            "KUNPCKB",
            X86OpmaskOp::Unpack {
                dst: k(1),
                src1: k(2),
                src2: k(3),
                width: OpWidth::W8,
            },
        ),
    ] {
        let kind = OpKind::X86Opmask(opmask.clone());
        assert!(!x86_opmask_native_shape_valid(&opmask), "{label}");
        assert!(!is_x86_native_vector_op(&kind), "{label}");
        assert!(!gate(opmask, true), "{label}");
    }

    let invalid_address = X86OpmaskOp::MoveToMask {
        dst: k(1),
        src: X86OpmaskMoveSource::Memory(Address::GpRel { offset: 0 }),
        width: OpWidth::W16,
    };
    assert!(x86_opmask_native_shape_valid(&invalid_address));
    assert!(!gate(invalid_address, true));

    let opmask = X86OpmaskOp::Not {
        dst: k(1),
        src: k(2),
        width: OpWidth::W16,
    };
    let hinted = SmirOp::with_hint(
        OpId(0),
        0x1000,
        OpKind::X86Opmask(opmask),
        X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::None,
            opcode: 0x44,
            width: VecWidth::V128,
            w: false,
        },
    );
    assert!(!x86_native_vector_smir_op(&hinted));
    assert!(!is_native_clobber_safe(&function_with(hinted)));
}

#[test]
fn kmov_memory_gate_requires_memory_mode_exact_address_and_vector_helper_preservation() {
    let addresses = [
        Address::Direct(x86(X86Reg::Rsp)),
        Address::BaseIndexScale {
            base: Some(x86(X86Reg::Rbp)),
            index: x86(X86Reg::R15),
            scale: 8,
            disp: -64,
            disp_size: DispSize::Disp8,
        },
        Address::X86Addr32(Box::new(Address::SegmentRel {
            segment: x86(X86Reg::FsBase),
            base: Some(x86(X86Reg::R10)),
            index: Some(x86(X86Reg::R11)),
            scale: 2,
            disp: 8,
        })),
        Address::PcRel {
            offset: 0x20,
            disp_size: DispSize::Disp32,
            base: Some(0x1008),
        },
    ];

    for (index, address) in addresses.into_iter().enumerate() {
        let width = [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64][index];
        for opmask in [
            X86OpmaskOp::MoveToMask {
                dst: k(1),
                src: X86OpmaskMoveSource::Memory(address.clone()),
                width,
            },
            X86OpmaskOp::MoveFromMask {
                dst: X86OpmaskMoveDestination::Memory(address.clone()),
                src: k(1),
                width,
            },
        ] {
            let kind = OpKind::X86Opmask(opmask.clone());
            assert!(x86_opmask_native_shape_valid(&opmask));
            assert!(!gate(opmask.clone(), false));
            assert!(gate(opmask, true));
            assert!(x86_jit_op_uses_mem_helper(&kind));
        }
    }
}

#[test]
fn opmask_regions_force_full_k_state_and_feature_requirements_are_exact() {
    let excluded = std::collections::HashMap::new();
    for (opmask, needs_dq) in [
        (
            X86OpmaskOp::Binary {
                kind: X86OpmaskBinaryKind::And,
                dst: k(1),
                src1: k(2),
                src2: k(3),
                width: OpWidth::W16,
            },
            false,
        ),
        (
            X86OpmaskOp::Binary {
                kind: X86OpmaskBinaryKind::Add,
                dst: k(1),
                src1: k(2),
                src2: k(3),
                width: OpWidth::W16,
            },
            true,
        ),
        (
            X86OpmaskOp::Test {
                kind: X86OpmaskTestKind::And,
                src1: k(1),
                src2: k(2),
                width: OpWidth::W16,
            },
            true,
        ),
        (
            X86OpmaskOp::Not {
                dst: k(1),
                src: k(2),
                width: OpWidth::W8,
            },
            true,
        ),
        (
            X86OpmaskOp::Not {
                dst: k(1),
                src: k(2),
                width: OpWidth::W32,
            },
            false,
        ),
    ] {
        assert_eq!(x86_opmask_needs_avx512dq(&opmask), needs_dq, "{opmask:?}");
        let function = plain_function(opmask);
        assert!(uses_x86_native_vectors_excluding(&function, &excluded));
        assert!(
            !x86_native_vector_uses_k16_opmasks_excluding(&function, &excluded),
            "opmask destinations require full 64-bit K-state commit"
        );

        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_vector_features_supported_excluding(&function, &excluded),
            std::is_x86_feature_detected!("avx512f")
                && std::is_x86_feature_detected!("avx512bw")
                && (!needs_dq || std::is_x86_feature_detected!("avx512dq")),
            "{function:#?}"
        );
        #[cfg(not(target_arch = "x86_64"))]
        assert!(!x86_native_vector_features_supported_excluding(
            &function, &excluded
        ));
    }

    let mut excluded_entry = std::collections::HashMap::new();
    let function = plain_function(X86OpmaskOp::Not {
        dst: k(1),
        src: k(2),
        width: OpWidth::W16,
    });
    excluded_entry.insert(function.entry, 0x1000);
    assert!(!uses_x86_native_vectors_excluding(
        &function,
        &excluded_entry
    ));
    assert!(x86_native_vector_features_supported_excluding(
        &function,
        &excluded_entry
    ));
}
