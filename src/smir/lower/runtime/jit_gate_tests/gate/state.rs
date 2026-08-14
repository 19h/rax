//! gate::state tests

use super::*;
use crate::smir::lower::runtime::jit_gate_tests::*;
use crate::smir::lower::runtime::*;
use crate::smir::lower::x86_64::x86_rdpid_shape_valid;

#[test]
fn x86_movd_q_gate_validates_direction_width_upper_state_and_encoding() {
    let gpr = |reg| x86(reg);
    let xmm = |index| x86(X86Reg::Xmm(index));
    let movd_q = |dst, src, width, zero_upper| OpKind::X86MovdQ {
        dst,
        src,
        width,
        zero_upper,
    };

    for (kind, hint) in [
        (
            movd_q(xmm(1), gpr(X86Reg::Rax), OpWidth::W32, false),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x6E,
            },
        ),
        (
            movd_q(gpr(X86Reg::R8), xmm(9), OpWidth::W64, false),
            X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x7E,
            },
        ),
        (
            movd_q(xmm(2), gpr(X86Reg::Rdx), OpWidth::W64, true),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x6E,
                width: VecWidth::V128,
                w: true,
            },
        ),
        (
            movd_q(gpr(X86Reg::R9), xmm(10), OpWidth::W32, false),
            X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x7E,
                width: VecWidth::V128,
                w: false,
            },
        ),
        (
            movd_q(xmm(17), gpr(X86Reg::R10), OpWidth::W64, true),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x6E,
                width: VecWidth::V128,
                w: true,
            },
        ),
        (
            movd_q(gpr(X86Reg::R11), xmm(18), OpWidth::W32, false),
            X86OpHint::EvexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: 0x7E,
                width: VecWidth::V128,
                w: false,
            },
        ),
    ] {
        let smir_op = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            kind.clone(),
            hint,
        );
        assert!(x86_movd_q_shape_valid(&kind), "{kind:?}");
        assert!(is_x86_native_vector_op(&kind), "{kind:?}");
        assert!(x86_native_vector_smir_op(&smir_op), "{smir_op:?}");

        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = Some(hint);
        assert!(is_native_clobber_safe(&function), "{smir_op:?}");
    }

    for malformed in [
        movd_q(xmm(1), gpr(X86Reg::Rsp), OpWidth::W32, false),
        movd_q(gpr(X86Reg::Rbp), xmm(1), OpWidth::W64, false),
        movd_q(VReg::Virtual(VirtualId(63)), xmm(1), OpWidth::W32, false),
        movd_q(xmm(1), x86(X86Reg::Ymm(2)), OpWidth::W32, true),
        movd_q(gpr(X86Reg::Rax), xmm(1), OpWidth::W32, true),
        movd_q(xmm(1), gpr(X86Reg::Rax), OpWidth::W16, false),
        movd_q(xmm(32), gpr(X86Reg::Rax), OpWidth::W32, true),
    ] {
        assert!(!x86_movd_q_shape_valid(&malformed), "{malformed:?}");
        assert!(!is_x86_native_vector_op(&malformed), "{malformed:?}");
    }

    let vex_vector = movd_q(xmm(1), gpr(X86Reg::Rax), OpWidth::W32, true);
    let unhinted = crate::smir::ir::ops::SmirOp::new(
        crate::smir::ir::types::OpId(0),
        0x1000,
        vex_vector.clone(),
    );
    assert!(!x86_native_vector_smir_op(&unhinted));

    for hint in [
        X86OpHint::SseOp {
            prefix: X86SsePrefix::OpSize,
            opcode: 0x6E,
        },
        X86OpHint::VexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: 0x6E,
            width: VecWidth::V128,
            w: false,
        },
        X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::None,
            opcode: 0x6E,
            width: VecWidth::V128,
            w: false,
        },
        X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::OpSize,
            opcode: 0x7E,
            width: VecWidth::V128,
            w: false,
        },
        X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::OpSize,
            opcode: 0x6E,
            width: VecWidth::V256,
            w: false,
        },
        X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::OpSize,
            opcode: 0x6E,
            width: VecWidth::V128,
            w: true,
        },
    ] {
        let malformed = crate::smir::ir::ops::SmirOp::with_hint(
            crate::smir::ir::types::OpId(0),
            0x1000,
            vex_vector.clone(),
            hint,
        );
        assert!(!x86_native_vector_smir_op(&malformed), "{malformed:?}");
    }

    let high_vex = crate::smir::ir::ops::SmirOp::with_hint(
        crate::smir::ir::types::OpId(0),
        0x1000,
        movd_q(xmm(17), gpr(X86Reg::Rax), OpWidth::W32, true),
        X86OpHint::VexOp {
            map: X86VecMap::Map0F,
            pp: X86SsePrefix::OpSize,
            opcode: 0x6E,
            width: VecWidth::V128,
            w: false,
        },
    );
    assert!(!x86_native_vector_smir_op(&high_vex));
}
#[test]
fn x86_count_gate_accepts_state_backed_gprs_and_rejects_unsafe_ir() {
    for op in [
        OpKind::X86Count {
            dst: x86(X86Reg::Rbp),
            src: x86(X86Reg::Rsp),
            width: OpWidth::W16,
            kind: X86CountKind::Popcnt,
            flags: FlagUpdate::All,
        },
        OpKind::X86Count {
            dst: x86(X86Reg::Rsp),
            src: x86(X86Reg::Rbp),
            width: OpWidth::W64,
            kind: X86CountKind::Tzcnt,
            flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF)),
        },
        OpKind::X86Count {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            width: OpWidth::W32,
            kind: X86CountKind::Lzcnt,
            flags: FlagUpdate::None,
        },
        OpKind::X86Count {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rax),
            width: OpWidth::W64,
            kind: X86CountKind::Popcnt,
            flags: FlagUpdate::Specific(FlagSet::ZF),
        },
    ] {
        assert!(op.is_jit_safe());
        assert!(x86_gate(op));
    }

    for (name, op) in [
        (
            "byte width",
            OpKind::X86Count {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W8,
                kind: X86CountKind::Popcnt,
                flags: FlagUpdate::All,
            },
        ),
        (
            "undefined flag request",
            OpKind::X86Count {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W64,
                kind: X86CountKind::Tzcnt,
                flags: FlagUpdate::All,
            },
        ),
        (
            "virtual source",
            OpKind::X86Count {
                dst: x86(X86Reg::R16),
                src: VReg::Virtual(VirtualId(0)),
                width: OpWidth::W64,
                kind: X86CountKind::Lzcnt,
                flags: FlagUpdate::None,
            },
        ),
        (
            "foreign architecture source",
            OpKind::X86Count {
                dst: x86(X86Reg::R16),
                src: arm_x(0),
                width: OpWidth::W64,
                kind: X86CountKind::Popcnt,
                flags: FlagUpdate::All,
            },
        ),
    ] {
        assert!(
            !x86_gate(op),
            "malformed {name} state-backed count must deopt"
        );
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86Count {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rax),
            width: OpWidth::W64,
            kind: X86CountKind::Popcnt,
            flags: FlagUpdate::All,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    assert!(
        !is_native_clobber_safe(&hinted),
        "hinted state-backed count must fail closed"
    );
}
#[test]
fn neg_gate_accepts_state_backed_gpr_flag_contracts_and_rejects_unsafe_ir() {
    for op in [
        OpKind::Neg {
            dst: x86(X86Reg::Rsp),
            src: x86(X86Reg::Rsp),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        },
        OpKind::Neg {
            dst: x86(X86Reg::Rbp),
            src: x86(X86Reg::R16),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        },
        OpKind::Neg {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rax),
            width: OpWidth::W32,
            flags: FlagUpdate::All,
        },
        OpKind::Neg {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::Rbp),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    ] {
        assert!(op.is_jit_safe());
        assert!(x86_gate(op));
    }

    for (name, op) in [
        (
            "wide operand",
            OpKind::Neg {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W128,
                flags: FlagUpdate::All,
            },
        ),
        (
            "partial flag update",
            OpKind::Neg {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::CF),
            },
        ),
        (
            "virtual source",
            OpKind::Neg {
                dst: x86(X86Reg::R16),
                src: VReg::Virtual(VirtualId(0)),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
        (
            "foreign architecture source",
            OpKind::Neg {
                dst: x86(X86Reg::R16),
                src: arm_x(0),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
    ] {
        assert!(!x86_gate(op), "malformed {name} Neg must deopt");
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Neg {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rax),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    assert!(
        !is_native_clobber_safe(&hinted),
        "hinted state-backed Neg must fail closed"
    );
}
#[test]
fn inc_dec_gate_accepts_state_backed_gpr_flag_contracts_and_rejects_unsafe_ir() {
    for op in [
        OpKind::Inc {
            dst: x86(X86Reg::Rsp),
            src: x86(X86Reg::Rsp),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        },
        OpKind::Dec {
            dst: x86(X86Reg::Rbp),
            src: x86(X86Reg::R16),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        },
        OpKind::Inc {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rax),
            width: OpWidth::W32,
            flags: FlagUpdate::All,
        },
        OpKind::Dec {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::Rbp),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    ] {
        assert!(op.is_jit_safe());
        assert!(x86_gate(op));
    }

    for (name, op) in [
        (
            "wide operand",
            OpKind::Inc {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W128,
                flags: FlagUpdate::All,
            },
        ),
        (
            "partial flag update",
            OpKind::Dec {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::CF),
            },
        ),
        (
            "virtual source",
            OpKind::Inc {
                dst: x86(X86Reg::R16),
                src: VReg::Virtual(VirtualId(0)),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
        (
            "foreign architecture source",
            OpKind::Dec {
                dst: x86(X86Reg::R16),
                src: arm_x(0),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
    ] {
        assert!(!x86_gate(op), "malformed {name} Inc/Dec must deopt");
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Inc {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rax),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    assert!(
        !is_native_clobber_safe(&hinted),
        "hinted state-backed Inc/Dec must fail closed"
    );
}
#[test]
fn not_gate_accepts_state_backed_gpr_widths_and_rejects_unsafe_ir() {
    for op in [
        OpKind::Not {
            dst: x86(X86Reg::Rsp),
            src: x86(X86Reg::Rsp),
            width: OpWidth::W8,
        },
        OpKind::Not {
            dst: x86(X86Reg::Rbp),
            src: x86(X86Reg::R16),
            width: OpWidth::W16,
        },
        OpKind::Not {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rax),
            width: OpWidth::W32,
        },
        OpKind::Not {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::Rbp),
            width: OpWidth::W64,
        },
    ] {
        assert!(op.is_jit_safe());
        assert!(x86_gate(op));
    }

    for (name, op) in [
        (
            "wide operand",
            OpKind::Not {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W128,
            },
        ),
        (
            "virtual source",
            OpKind::Not {
                dst: x86(X86Reg::R16),
                src: VReg::Virtual(VirtualId(0)),
                width: OpWidth::W64,
            },
        ),
        (
            "foreign architecture source",
            OpKind::Not {
                dst: x86(X86Reg::R16),
                src: arm_x(0),
                width: OpWidth::W64,
            },
        ),
    ] {
        assert!(!x86_gate(op), "malformed {name} Not must deopt");
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Not {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rax),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    assert!(
        !is_native_clobber_safe(&hinted),
        "hinted state-backed Not must fail closed"
    );
}
#[test]
fn x86_alignment_gate_admits_exact_state_backed_shapes_without_memory_mode() {
    for (alignment, addr) in [
        (16, Address::Direct(x86(X86Reg::Rax))),
        (
            32,
            Address::BaseOffset {
                base: x86(X86Reg::Rsp),
                offset: i64::MIN,
                disp_size: DispSize::Disp32,
            },
        ),
        (
            64,
            Address::BaseIndexScale {
                base: Some(x86(X86Reg::Rbp)),
                index: x86(X86Reg::R16),
                scale: 8,
                disp: -64,
                disp_size: DispSize::Disp8,
            },
        ),
        (
            16,
            Address::PcRel {
                offset: -32,
                disp_size: DispSize::Disp32,
                base: Some(0x1020),
            },
        ),
        (32, Address::Absolute(0x2000)),
        (
            64,
            Address::SegmentRel {
                segment: x86(X86Reg::GsBase),
                base: Some(x86(X86Reg::Rsp)),
                index: Some(x86(X86Reg::R31)),
                scale: 4,
                disp: i64::MAX,
            },
        ),
    ] {
        let op = OpKind::X86CheckAlignment { addr, alignment };
        assert!(!op.is_jit_safe(), "alignment checks are control operations");
        assert!(
            x86_gate(op),
            "validated alignment check must not require MMU-helper mode"
        );
    }

    for malformed in [
        OpKind::X86CheckAlignment {
            addr: Address::Direct(x86(X86Reg::Rax)),
            alignment: 8,
        },
        OpKind::X86CheckAlignment {
            addr: Address::Direct(VReg::Virtual(VirtualId(4))),
            alignment: 16,
        },
        OpKind::X86CheckAlignment {
            addr: Address::BaseIndexScale {
                base: None,
                index: x86(X86Reg::Rax),
                scale: 3,
                disp: 0,
                disp_size: DispSize::Auto,
            },
            alignment: 32,
        },
        OpKind::X86CheckAlignment {
            addr: Address::PcRel {
                offset: 0,
                disp_size: DispSize::Auto,
                base: None,
            },
            alignment: 64,
        },
        OpKind::X86CheckAlignment {
            addr: Address::GpRel { offset: 0 },
            alignment: 16,
        },
    ] {
        assert!(!x86_gate(malformed), "malformed alignment check must deopt");
    }
}
#[test]
fn x86_stack_arithmetic_gate_admits_only_exact_state_backed_shapes() {
    for op in [
        OpKind::Add {
            dst: x86(X86Reg::Rsp),
            src1: x86(X86Reg::Rsp),
            src2: SrcOperand::Imm(8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Sub {
            dst: x86(X86Reg::R16),
            src1: x86(X86Reg::Rbp),
            src2: SrcOperand::Reg(x86(X86Reg::R31)),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        },
        OpKind::Add {
            dst: x86(X86Reg::Rax),
            src1: x86(X86Reg::Rcx),
            src2: SrcOperand::Reg(x86(X86Reg::Rsp)),
            width: OpWidth::W32,
            flags: FlagUpdate::All,
        },
    ] {
        assert!(x86_state_backed_stack_alu_valid(&op));
        assert!(x86_gate(op), "validated stack arithmetic must enter JIT");
    }

    for malformed in [
        OpKind::Add {
            dst: x86(X86Reg::Rsp),
            src1: VReg::Virtual(VirtualId(1)),
            src2: SrcOperand::Imm(8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Sub {
            dst: x86(X86Reg::Rsp),
            src1: x86(X86Reg::Rsp),
            src2: SrcOperand::Imm64(8),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Add {
            dst: x86(X86Reg::Rsp),
            src1: x86(X86Reg::Rsp),
            src2: SrcOperand::Imm(i64::from(i32::MAX) + 1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Add {
            dst: x86(X86Reg::Rax),
            src1: x86(X86Reg::Rcx),
            src2: SrcOperand::Reg(x86(X86Reg::Rdx)),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
        OpKind::Sub {
            dst: x86(X86Reg::Rbp),
            src1: x86(X86Reg::Rbp),
            src2: SrcOperand::Imm(8),
            width: OpWidth::W128,
            flags: FlagUpdate::None,
        },
        OpKind::Add {
            dst: x86(X86Reg::Rsp),
            src1: x86(X86Reg::Rsp),
            src2: SrcOperand::Imm(8),
            width: OpWidth::W64,
            flags: FlagUpdate::Specific(FlagSet::ZF),
        },
    ] {
        assert!(!x86_state_backed_stack_alu_valid(&malformed));
    }
}
#[test]
fn x86_rdpid_gate_admits_all_32_gprs_and_rejects_non_gprs_and_cross_host_execution() {
    for index in 0u8..32 {
        let op = OpKind::X86ReadPid {
            dst: x86(X86Reg::gpr(index)),
        };
        assert!(op.is_jit_safe(), "RDPID must be class-whitelisted");
        assert!(x86_rdpid_shape_valid(&op), "GPR {index}");
        assert!(x86_gate(op), "GPR {index} must enter the x86 native tier");
    }

    for (name, dst) in [
        ("virtual", VReg::Virtual(VirtualId(1))),
        ("SIMD", x86(X86Reg::Xmm(0))),
        ("instruction pointer", x86(X86Reg::Rip)),
    ] {
        let op = OpKind::X86ReadPid { dst };
        assert!(op.is_jit_safe(), "{name} remains class-whitelisted");
        assert!(!x86_rdpid_shape_valid(&op), "{name}");
        assert!(!x86_gate(op), "malformed {name} RDPID must deopt");
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::X86ReadPid {
            dst: x86(X86Reg::Rsp),
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    assert!(
        !is_x86_aarch64_native_clobber_safe_excluding(
            &builder.finish(),
            &std::collections::HashMap::new(),
        ),
        "x86 RDPID must remain interpreter-only on an AArch64 host"
    );
}
#[test]
fn bit_scan_gate_accepts_state_backed_gprs_and_rejects_unsafe_ir() {
    let zf_only = FlagUpdate::Specific(FlagSet::ZF);
    for op in [
        OpKind::Bsf {
            dst: x86(X86Reg::Rbp),
            src: x86(X86Reg::Rsp),
            width: OpWidth::W16,
            flags: zf_only,
        },
        OpKind::Bsr {
            dst: x86(X86Reg::Rsp),
            src: x86(X86Reg::Rbp),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
        OpKind::Bsf {
            dst: x86(X86Reg::R31),
            src: x86(X86Reg::R16),
            width: OpWidth::W32,
            flags: zf_only,
        },
        OpKind::Bsr {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rax),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    ] {
        assert!(op.is_jit_safe());
        assert!(x86_gate(op));
    }

    for (name, op) in [
        (
            "byte width",
            OpKind::Bsf {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W8,
                flags: zf_only,
            },
        ),
        (
            "undefined flag request",
            OpKind::Bsr {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rax),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
        (
            "virtual source",
            OpKind::Bsf {
                dst: x86(X86Reg::R16),
                src: VReg::Virtual(VirtualId(0)),
                width: OpWidth::W64,
                flags: zf_only,
            },
        ),
        (
            "foreign architecture source",
            OpKind::Bsr {
                dst: x86(X86Reg::R16),
                src: arm_x(0),
                width: OpWidth::W64,
                flags: zf_only,
            },
        ),
    ] {
        assert!(
            !x86_gate(op),
            "malformed {name} state-backed bit scan must deopt"
        );
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Bsf {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rax),
            width: OpWidth::W64,
            flags: zf_only,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    assert!(
        !is_native_clobber_safe(&hinted),
        "hinted state-backed bit scan must fail closed"
    );
}
#[test]
fn x86_state_backed_rotate_gate_accepts_exact_shapes_and_fails_closed() {
    let rotate_flags = FlagSet::CF.union(FlagSet::OF);
    for (name, op) in [
        (
            "ROL RSP,RBP,1",
            OpKind::Rol {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(rotate_flags),
            },
        ),
        (
            "ROR R31B,R16B,SP",
            OpKind::Ror {
                dst: x86(X86Reg::R31),
                src: x86(X86Reg::R16),
                amount: SrcOperand::Reg(x86(X86Reg::Rsp)),
                width: OpWidth::W8,
                flags: FlagUpdate::All,
            },
        ),
        (
            "NF ROL BP,R31W,9",
            OpKind::Rol {
                dst: x86(X86Reg::Rbp),
                src: x86(X86Reg::R31),
                amount: SrcOperand::Imm(9),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        ),
        (
            "ROR R16D,R16D,R16 all alias",
            OpKind::Ror {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::R16),
                amount: SrcOperand::Reg(x86(X86Reg::R16)),
                width: OpWidth::W32,
                flags: FlagUpdate::Specific(rotate_flags),
            },
        ),
    ] {
        assert!(
            x86_gate(op),
            "valid state-backed or guarded {name} must JIT"
        );
    }

    for (name, op) in [
        (
            "128-bit width",
            OpKind::Rol {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rsp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W128,
                flags: FlagUpdate::Specific(rotate_flags),
            },
        ),
        (
            "virtual source",
            OpKind::Ror {
                dst: x86(X86Reg::R31),
                src: VReg::Virtual(VirtualId(0)),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(rotate_flags),
            },
        ),
        (
            "Imm64 count",
            OpKind::Rol {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm64(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(rotate_flags),
            },
        ),
        (
            "incomplete flag set",
            OpKind::Ror {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::CF),
            },
        ),
    ] {
        assert!(!x86_gate(op), "malformed state-backed {name} must deopt");
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Rol {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rsp),
            amount: SrcOperand::Reg(x86(X86Reg::Rbp)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    assert!(
        !is_native_clobber_safe(&hinted),
        "hinted state-backed rotate must fail closed"
    );
}
#[test]
fn x86_state_backed_shift_gate_accepts_exact_shapes_and_fails_closed() {
    for (name, op) in [
        (
            "SHL RSP,RBP,1",
            OpKind::Shl {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
        (
            "SHR R31B,R16B,SP",
            OpKind::Shr {
                dst: x86(X86Reg::R31),
                src: x86(X86Reg::R16),
                amount: SrcOperand::Reg(x86(X86Reg::Rsp)),
                width: OpWidth::W8,
                flags: FlagUpdate::All,
            },
        ),
        (
            "NF SAR BP,R31W,9",
            OpKind::Sar {
                dst: x86(X86Reg::Rbp),
                src: x86(X86Reg::R31),
                amount: SrcOperand::Imm(9),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        ),
        (
            "SAR R16D,R16D,R16 all alias",
            OpKind::Sar {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::R16),
                amount: SrcOperand::Reg(x86(X86Reg::R16)),
                width: OpWidth::W32,
                flags: FlagUpdate::All,
            },
        ),
    ] {
        assert!(
            x86_gate(op),
            "valid state-backed or guarded {name} must JIT"
        );
    }

    for (name, op) in [
        (
            "128-bit width",
            OpKind::Shl {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rsp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W128,
                flags: FlagUpdate::All,
            },
        ),
        (
            "virtual source",
            OpKind::Shr {
                dst: x86(X86Reg::R31),
                src: VReg::Virtual(VirtualId(0)),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
        (
            "Imm64 count",
            OpKind::Sar {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm64(1),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
        (
            "partial flag set",
            OpKind::Shl {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::CF),
            },
        ),
    ] {
        assert!(!x86_gate(op), "malformed state-backed {name} must deopt");
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Shr {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rsp),
            amount: SrcOperand::Reg(x86(X86Reg::Rbp)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    assert!(
        !is_native_clobber_safe(&hinted),
        "hinted state-backed shift must fail closed"
    );
}
#[test]
fn x86_state_backed_carry_rotate_gate_accepts_exact_shapes_and_fails_closed() {
    let rotate_flags = FlagSet::CF.union(FlagSet::OF);
    for (name, op) in [
        (
            "RCL RSP,RBP,1",
            OpKind::Rcl {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(rotate_flags),
            },
        ),
        (
            "RCR R31B,R16B,SP",
            OpKind::Rcr {
                dst: x86(X86Reg::R31),
                src: x86(X86Reg::R16),
                amount: SrcOperand::Reg(x86(X86Reg::Rsp)),
                width: OpWidth::W8,
                flags: FlagUpdate::All,
            },
        ),
        (
            "RCL BP,R31W,9",
            OpKind::Rcl {
                dst: x86(X86Reg::Rbp),
                src: x86(X86Reg::R31),
                amount: SrcOperand::Imm(9),
                width: OpWidth::W16,
                flags: FlagUpdate::Specific(rotate_flags),
            },
        ),
        (
            "NF RCR R16D,R16D,R16 all alias",
            OpKind::Rcr {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::R16),
                amount: SrcOperand::Reg(x86(X86Reg::R16)),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        ),
    ] {
        assert!(x86_gate(op), "valid state-backed {name} must JIT");
    }

    for (name, op) in [
        (
            "128-bit width",
            OpKind::Rcl {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rsp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W128,
                flags: FlagUpdate::Specific(rotate_flags),
            },
        ),
        (
            "virtual source",
            OpKind::Rcr {
                dst: x86(X86Reg::R31),
                src: VReg::Virtual(VirtualId(0)),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(rotate_flags),
            },
        ),
        (
            "Imm64 count",
            OpKind::Rcl {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm64(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(rotate_flags),
            },
        ),
        (
            "partial flag set",
            OpKind::Rcr {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::CF),
            },
        ),
    ] {
        assert!(!x86_gate(op), "malformed state-backed {name} must deopt");
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Rcl {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rsp),
            amount: SrcOperand::Reg(x86(X86Reg::Rbp)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    assert!(
        !is_native_clobber_safe(&hinted),
        "hinted state-backed carry rotate must fail closed"
    );
}
#[test]
fn x86_state_backed_double_shift_gate_accepts_exact_shapes_and_fails_closed() {
    for (name, op) in [
        (
            "SHLD RSP,RBP,1",
            OpKind::Shld {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
        (
            "SHRD R31W,R16W,SP",
            OpKind::Shrd {
                dst: x86(X86Reg::R31),
                src: x86(X86Reg::R16),
                amount: SrcOperand::Reg(x86(X86Reg::Rsp)),
                width: OpWidth::W16,
                flags: FlagUpdate::All,
            },
        ),
        (
            "NF SHLD R16D,R16D,R16 aliases",
            OpKind::Shld {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::R16),
                amount: SrcOperand::Reg(x86(X86Reg::R16)),
                width: OpWidth::W32,
                flags: FlagUpdate::None,
            },
        ),
        (
            "SHRD RAX,RDX,BP state count",
            OpKind::Shrd {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rdx),
                amount: SrcOperand::Reg(x86(X86Reg::Rbp)),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
        (
            "NDD SHLD R16,RSP,R31,4",
            OpKind::X86NddDoubleShift {
                dst: x86(X86Reg::R16),
                base: x86(X86Reg::Rsp),
                fill: x86(X86Reg::R31),
                amount: SrcOperand::Imm(4),
                width: OpWidth::W64,
                left: true,
                flags: FlagUpdate::All,
            },
        ),
        (
            "NF NDD SHRD SP,BP,R31,CL",
            OpKind::X86NddDoubleShift {
                dst: x86(X86Reg::Rsp),
                base: x86(X86Reg::Rbp),
                fill: x86(X86Reg::R31),
                amount: SrcOperand::Reg(x86(X86Reg::Rcx)),
                width: OpWidth::W16,
                left: false,
                flags: FlagUpdate::None,
            },
        ),
        (
            "direct NDD SHLD DX,AX,BX,17 needs deterministic guard",
            OpKind::X86NddDoubleShift {
                dst: x86(X86Reg::Rdx),
                base: x86(X86Reg::Rax),
                fill: x86(X86Reg::Rbx),
                amount: SrcOperand::Imm(17),
                width: OpWidth::W16,
                left: true,
                flags: FlagUpdate::All,
            },
        ),
        (
            "direct legacy SHLD AX,BX,17 needs deterministic guard",
            OpKind::Shld {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rbx),
                amount: SrcOperand::Imm(17),
                width: OpWidth::W16,
                flags: FlagUpdate::All,
            },
        ),
        (
            "direct legacy SHRD AX,BX,CL needs runtime guard",
            OpKind::Shrd {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rbx),
                amount: SrcOperand::Reg(x86(X86Reg::Rcx)),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
        ),
    ] {
        assert!(x86_gate(op), "valid state-backed {name} must JIT");
    }

    for (name, op) in [
        (
            "byte width",
            OpKind::Shld {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rsp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W8,
                flags: FlagUpdate::All,
            },
        ),
        (
            "virtual fill",
            OpKind::Shrd {
                dst: x86(X86Reg::R31),
                src: VReg::Virtual(VirtualId(0)),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
        (
            "Imm64 count",
            OpKind::Shld {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm64(1),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ),
        (
            "partial flag set",
            OpKind::Shrd {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rbp),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            },
        ),
        (
            "NDD non-CL register count",
            OpKind::X86NddDoubleShift {
                dst: x86(X86Reg::R16),
                base: x86(X86Reg::Rsp),
                fill: x86(X86Reg::R31),
                amount: SrcOperand::Reg(x86(X86Reg::Rbp)),
                width: OpWidth::W64,
                left: true,
                flags: FlagUpdate::All,
            },
        ),
        (
            "NDD partial flag set",
            OpKind::X86NddDoubleShift {
                dst: x86(X86Reg::R16),
                base: x86(X86Reg::Rsp),
                fill: x86(X86Reg::R31),
                amount: SrcOperand::Imm(1),
                width: OpWidth::W64,
                left: false,
                flags: FlagUpdate::Specific(FlagSet::ZF),
            },
        ),
    ] {
        assert!(!x86_gate(op), "malformed state-backed {name} must deopt");
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Shld {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rsp),
            amount: SrcOperand::Reg(x86(X86Reg::Rbp)),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    assert!(
        !is_native_clobber_safe(&hinted),
        "hinted state-backed double shift must fail closed"
    );
}
#[test]
fn clobber_gate_admits_state_backed_gpr_extensions_and_fails_closed() {
    let gate = |op: OpKind, hint: Option<X86OpHint>| {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, op);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = hint;
        is_native_clobber_safe(&function)
    };

    for (name, op, hint) in [
        (
            "MOVZX SP,BL",
            OpKind::ZeroExtend {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rbx),
                from_width: OpWidth::W8,
                to_width: OpWidth::W16,
            },
            None,
        ),
        (
            "MOVSX EBP,SP",
            OpKind::SignExtend {
                dst: x86(X86Reg::Rbp),
                src: x86(X86Reg::Rsp),
                from_width: OpWidth::W16,
                to_width: OpWidth::W32,
            },
            None,
        ),
        (
            "MOVSX BP,BX same-width copy",
            OpKind::SignExtend {
                dst: x86(X86Reg::Rbp),
                src: x86(X86Reg::Rbx),
                from_width: OpWidth::W16,
                to_width: OpWidth::W16,
            },
            None,
        ),
        (
            "MOVZX R16,EBP",
            OpKind::ZeroExtend {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rbp),
                from_width: OpWidth::W32,
                to_width: OpWidth::W64,
            },
            None,
        ),
        (
            "MOVSX RAX,R16B",
            OpKind::SignExtend {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::R16),
                from_width: OpWidth::W8,
                to_width: OpWidth::W64,
            },
            Some(X86OpHint::RexByteReg),
        ),
        (
            "MOVZX SP,AH",
            OpKind::ZeroExtend {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rax),
                from_width: OpWidth::W8,
                to_width: OpWidth::W16,
            },
            Some(X86OpHint::LegacyHighByteReg),
        ),
        (
            "MOVZX RAX,SPL",
            OpKind::ZeroExtend {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rsp),
                from_width: OpWidth::W8,
                to_width: OpWidth::W64,
            },
            Some(X86OpHint::RexByteReg),
        ),
    ] {
        assert!(gate(op, hint), "{name} must enter the native tier");
    }

    for (name, op, hint) in [
        (
            "unhinted ambiguous SPL/AH source",
            OpKind::ZeroExtend {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rsp),
                from_width: OpWidth::W8,
                to_width: OpWidth::W64,
            },
            None,
        ),
        (
            "virtual source",
            OpKind::ZeroExtend {
                dst: x86(X86Reg::Rsp),
                src: VReg::Virtual(VirtualId(7)),
                from_width: OpWidth::W8,
                to_width: OpWidth::W16,
            },
            None,
        ),
        (
            "irrelevant encoding hint",
            OpKind::ZeroExtend {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rbx),
                from_width: OpWidth::W8,
                to_width: OpWidth::W64,
            },
            Some(X86OpHint::Mulx),
        ),
        (
            "legacy high byte with EGPR destination",
            OpKind::SignExtend {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rbx),
                from_width: OpWidth::W8,
                to_width: OpWidth::W32,
            },
            Some(X86OpHint::LegacyHighByteReg),
        ),
        (
            "legacy high byte with REX.W destination",
            OpKind::ZeroExtend {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rax),
                from_width: OpWidth::W8,
                to_width: OpWidth::W64,
            },
            Some(X86OpHint::LegacyHighByteReg),
        ),
    ] {
        assert!(!gate(op, hint), "{name} must fail closed");
    }
}
#[test]
fn clobber_gate_admits_state_backed_gpr_cmov_and_fails_closed() {
    let gate = |op: OpKind, hint: Option<X86OpHint>| {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, op);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = hint;
        is_native_clobber_safe(&function)
    };

    for (name, op) in [
        (
            "CMOVNE SP,BX",
            OpKind::CMove {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rbx),
                cond: Condition::Ne,
                width: OpWidth::W16,
            },
        ),
        (
            "CMOVE EBP,ESP",
            OpKind::CMove {
                dst: x86(X86Reg::Rbp),
                src: x86(X86Reg::Rsp),
                cond: Condition::Eq,
                width: OpWidth::W32,
            },
        ),
        (
            "CMOVS R16,RBP",
            OpKind::CMove {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rbp),
                cond: Condition::Negative,
                width: OpWidth::W64,
            },
        ),
        (
            "CMOVP RAX,R16",
            OpKind::CMove {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::R16),
                cond: Condition::Parity,
                width: OpWidth::W64,
            },
        ),
        (
            "CMOVNE SP,SP alias",
            OpKind::CMove {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rsp),
                cond: Condition::Ne,
                width: OpWidth::W16,
            },
        ),
    ] {
        assert!(gate(op, None), "{name} must enter the native tier");
    }

    assert!(
        gate(
            OpKind::CMove {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::Rbx),
                cond: Condition::Eq,
                width: OpWidth::W64,
            },
            None,
        ),
        "ordinary identity-register CMOV must remain eligible"
    );

    for (name, op, hint) in [
        (
            "byte width",
            OpKind::CMove {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rbx),
                cond: Condition::Ne,
                width: OpWidth::W8,
            },
            None,
        ),
        (
            "virtual source",
            OpKind::CMove {
                dst: x86(X86Reg::Rsp),
                src: VReg::Virtual(VirtualId(7)),
                cond: Condition::Eq,
                width: OpWidth::W16,
            },
            None,
        ),
        (
            "unconditional condition",
            OpKind::CMove {
                dst: x86(X86Reg::R16),
                src: x86(X86Reg::Rbx),
                cond: Condition::Always,
                width: OpWidth::W64,
            },
            None,
        ),
        (
            "irrelevant encoding hint",
            OpKind::CMove {
                dst: x86(X86Reg::Rax),
                src: x86(X86Reg::R16),
                cond: Condition::Ne,
                width: OpWidth::W64,
            },
            Some(X86OpHint::Mulx),
        ),
    ] {
        assert!(!gate(op, hint), "{name} must fail closed");
    }
}
#[test]
fn clobber_gate_admits_state_backed_gpr_setcc_and_fails_closed() {
    let gate = |op: OpKind, hint: Option<X86OpHint>| {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, op);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut function = builder.finish();
        function.blocks[0].ops[0].x86_hint = hint;
        is_native_clobber_safe(&function)
    };

    for (name, op) in [
        (
            "SETNE SPL",
            OpKind::SetCC {
                dst: x86(X86Reg::Rsp),
                cond: Condition::Ne,
                width: OpWidth::W8,
            },
        ),
        (
            "SETE BPL",
            OpKind::SetCC {
                dst: x86(X86Reg::Rbp),
                cond: Condition::Eq,
                width: OpWidth::W8,
            },
        ),
        (
            "SETS R16B",
            OpKind::SetCC {
                dst: x86(X86Reg::R16),
                cond: Condition::Negative,
                width: OpWidth::W8,
            },
        ),
        (
            "SETZUO R16",
            OpKind::SetCC {
                dst: x86(X86Reg::R16),
                cond: Condition::Overflow,
                width: OpWidth::W64,
            },
        ),
        (
            "SETZUNE RBP",
            OpKind::SetCC {
                dst: x86(X86Reg::Rbp),
                cond: Condition::Ne,
                width: OpWidth::W64,
            },
        ),
    ] {
        assert!(gate(op, None), "{name} must enter the native tier");
    }

    for width in [OpWidth::W8, OpWidth::W64] {
        assert!(
            gate(
                OpKind::SetCC {
                    dst: x86(X86Reg::Rax),
                    cond: Condition::Eq,
                    width,
                },
                None,
            ),
            "ordinary identity-register SETcc {width:?} must remain eligible"
        );
    }

    for (name, op, hint) in [
        (
            "word width",
            OpKind::SetCC {
                dst: x86(X86Reg::Rsp),
                cond: Condition::Ne,
                width: OpWidth::W16,
            },
            None,
        ),
        (
            "dword width",
            OpKind::SetCC {
                dst: x86(X86Reg::R16),
                cond: Condition::Eq,
                width: OpWidth::W32,
            },
            None,
        ),
        (
            "unconditional condition",
            OpKind::SetCC {
                dst: x86(X86Reg::Rbp),
                cond: Condition::Always,
                width: OpWidth::W8,
            },
            None,
        ),
        (
            "irrelevant encoding hint",
            OpKind::SetCC {
                dst: x86(X86Reg::R16),
                cond: Condition::Overflow,
                width: OpWidth::W64,
            },
            Some(X86OpHint::Mulx),
        ),
    ] {
        assert!(!gate(op, hint), "{name} must fail closed");
    }
}
