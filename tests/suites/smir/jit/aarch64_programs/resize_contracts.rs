//! Width-transition metadata, invalid IR, and native admission boundaries.

use rax::isa::arm::decoder::{Mnemonic, aarch64::Aarch64Decoder};
use rax::smir::interpret::{BlockResult, SmirInterpreter};
use rax::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use rax::smir::ir::ops::{
    ArmSimdLongKind, ArmSimdLongOp, ArmSimdNarrowKind, ArmSimdNarrowMode, ArmSimdNarrowOp,
    ArmSimdPart, OpKind,
};
use rax::smir::ir::types::{
    ArchReg, ArmReg, FunctionId, SourceArch, VReg, VecElementType, VecWidth,
};
use rax::smir::ir::{FunctionBuilder, SmirFunction, Terminator};
use rax::smir::lift::aarch64::Aarch64Lifter;
use rax::smir::lift::{LiftContext, SmirLifter};
use rax::smir::lower::{SmirLowerer, aarch64::Aarch64Lowerer};
use rax::smir::optimize::{OptLevel, optimize_function};

use super::{CODE_ADDR, lift_program};

fn v(index: u8) -> VReg {
    VReg::Arch(ArchReg::Arm(ArmReg::V(index)))
}

fn long_operation() -> ArmSimdLongOp {
    ArmSimdLongOp {
        dst: v(0),
        src1: v(1),
        src2: Some(v(2)),
        elem: VecElementType::I16,
        kind: ArmSimdLongKind::Multiply,
        signed: true,
        part: ArmSimdPart::LowerHalf,
    }
}

fn narrow_operation() -> ArmSimdNarrowOp {
    ArmSimdNarrowOp {
        dst: v(0),
        src1: v(1),
        src2: None,
        elem: VecElementType::I16,
        kind: ArmSimdNarrowKind::Extract {
            mode: ArmSimdNarrowMode::Truncate,
        },
        part: ArmSimdPart::LowerHalf,
    }
}

fn function_with_operation(kind: OpKind) -> SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), CODE_ADDR);
    builder.push_op(CODE_ADDR, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.finish()
}

#[test]
fn width_transition_metadata_tracks_accumulators_merges_and_sticky_qc() {
    let fpsr = VReg::Arch(ArchReg::Arm(ArmReg::Fpsr));
    let cases = [
        (0x0e62_8020, vec![v(1), v(2), v(0)], vec![v(0)], false), // SMLAL
        (
            0x4e62_9020,
            vec![v(1), v(2), v(0), fpsr],
            vec![v(0), fpsr],
            true,
        ), // SQDMLAL2
        (0x4e60_2820, vec![v(1)], vec![v(0)], false),             // SADDLP
        (0x4e60_6820, vec![v(1), v(0)], vec![v(0)], false),       // SADALP
        (0x4e21_2820, vec![v(1), v(0)], vec![v(0)], false),       // XTN2
        (0x2e21_2820, vec![v(1), fpsr], vec![v(0), fpsr], true),  // SQXTUN
    ];
    for (instruction, sources, destinations, side_effects) in cases {
        let function = lift_program(&[instruction]);
        let operation = &function.blocks[0].ops[0];
        assert_eq!(
            operation.kind.source_vregs(),
            sources,
            "{instruction:#010x}"
        );
        assert_eq!(operation.kind.dests(), destinations, "{instruction:#010x}");
        assert_eq!(
            operation.kind.has_side_effects(),
            side_effects,
            "{instruction:#010x}"
        );
        assert!(!operation.kind.is_jit_safe(), "generic opcode admission");
        assert!(!operation.is_jit_safe(), "generic operation admission");
    }
}

#[test]
fn reserved_width_transition_encodings_are_undefined_frontiers() {
    let instructions = [
        0x0ee2_0020, // SADDL with size=0b11.
        0x0e22_9020, // SQDMLAL with 8-bit inputs.
        0x2e62_9020, // Reserved unsigned doubling multiply-add.
        0x5e62_0020, // Scalar SADDL does not exist.
        0x0ee1_2820, // XTN with size=0b11.
        0x5e21_2820, // Scalar non-saturating XTN does not exist.
        0x5e60_2820, // Scalar pairwise long does not exist.
        0x0f40_8420, // SHRN with immh[3]=1.
        0x5f08_8420, // Scalar non-saturating SHRN does not exist.
        0x5f08_a420, // Scalar SSHLL does not exist.
        0x2ee1_3820, // SHLL with size=0b11.
    ];
    for instruction in instructions {
        let decoded = Aarch64Decoder::decode(instruction).unwrap();
        assert_eq!(decoded.mnemonic, Mnemonic::UNDEFINED, "{instruction:#010x}");
        let mut lifter = Aarch64Lifter::new();
        let mut context = LiftContext::new(SourceArch::Aarch64);
        if let Ok(result) = lifter.lift_insn(CODE_ADDR, &instruction.to_le_bytes(), &mut context) {
            assert!(result.control_flow.ends_block(), "{instruction:#010x}");
        }
    }
}

fn malformed_operations() -> Vec<OpKind> {
    let mut operations = Vec::new();
    for elem in [
        VecElementType::I64,
        VecElementType::F16,
        VecElementType::F32,
    ] {
        operations.push(OpKind::ArmSimdLong(ArmSimdLongOp {
            elem,
            ..long_operation()
        }));
        operations.push(OpKind::ArmSimdNarrow(ArmSimdNarrowOp {
            elem,
            ..narrow_operation()
        }));
    }
    for reg in [
        v(32),
        v(255),
        VReg::Imm(0),
        VReg::Arch(ArchReg::Arm(ArmReg::X(0))),
    ] {
        for operand in 0..3 {
            let mut long = long_operation();
            match operand {
                0 => long.dst = reg,
                1 => long.src1 = reg,
                _ => long.src2 = Some(reg),
            }
            operations.push(OpKind::ArmSimdLong(long));
        }
        let mut narrow = narrow_operation();
        narrow.src1 = reg;
        operations.push(OpKind::ArmSimdNarrow(narrow));
    }
    operations.push(OpKind::ArmSimdLong(ArmSimdLongOp {
        src2: None,
        ..long_operation()
    }));
    for (elem, signed) in [(VecElementType::I8, true), (VecElementType::I16, false)] {
        operations.push(OpKind::ArmSimdLong(ArmSimdLongOp {
            kind: ArmSimdLongKind::SaturatingDoublingMultiply,
            elem,
            signed,
            ..long_operation()
        }));
    }
    operations.push(OpKind::ArmSimdLong(ArmSimdLongOp {
        part: ArmSimdPart::Scalar,
        ..long_operation()
    }));
    operations.push(OpKind::ArmSimdNarrow(ArmSimdNarrowOp {
        part: ArmSimdPart::Scalar,
        ..narrow_operation()
    }));
    for (amount, signed) in [(17, false), (16, true)] {
        operations.push(OpKind::ArmSimdLong(ArmSimdLongOp {
            src2: None,
            kind: ArmSimdLongKind::ShiftLeft { amount },
            signed,
            ..long_operation()
        }));
    }
    for width in [VecWidth::V256, VecWidth::V512] {
        operations.push(OpKind::ArmSimdLong(ArmSimdLongOp {
            src2: None,
            kind: ArmSimdLongKind::PairwiseAdd {
                width,
                accumulate: false,
            },
            ..long_operation()
        }));
    }
    operations.push(OpKind::ArmSimdNarrow(ArmSimdNarrowOp {
        src2: Some(v(2)),
        ..narrow_operation()
    }));
    operations.push(OpKind::ArmSimdNarrow(ArmSimdNarrowOp {
        kind: ArmSimdNarrowKind::HighArithmetic {
            subtract: false,
            rounding: false,
        },
        ..narrow_operation()
    }));
    for amount in [0, 17] {
        operations.push(OpKind::ArmSimdNarrow(ArmSimdNarrowOp {
            kind: ArmSimdNarrowKind::ShiftRight {
                amount,
                rounding: true,
                mode: ArmSimdNarrowMode::SignedSaturate,
            },
            ..narrow_operation()
        }));
    }
    operations
}

#[test]
fn malformed_resize_ir_rejects_before_state_mutation_at_exact_guest_pc() {
    for kind in malformed_operations() {
        assert!(
            kind.has_side_effects(),
            "invalid IR must survive DCE: {kind:?}"
        );
        let original = function_with_operation(kind);
        assert!(Aarch64Lowerer::new().lower_function(&original).is_err());
        #[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
        super::assert_native_rejected(&original);

        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let mut function = original.clone();
            optimize_function(&mut function, level);
            let mut context = SmirContext::new_aarch64();
            context.pc = 0xdead_0000; // Must not become the fault address.
            let ArchRegState::Aarch64(regs) = &mut context.arch_regs else {
                panic!("expected AArch64 register state");
            };
            regs.v = [[0xa55a_ffff_0123_4567, 0x5aa5_1234_ffff_0000]; 32];
            regs.x = [0x1234; 31];
            regs.fpsr = 0x0090;
            regs.nzcv = 0xa000_0000;
            let before = regs.clone();
            let mut memory = rax::smir::ir::memory::FlatMemory::new(1);
            let result = SmirInterpreter::new().execute_block(
                &mut context,
                &mut memory,
                &function.blocks[0],
            );
            assert!(matches!(
                result,
                BlockResult::Exit(ExitReason::Undefined {
                    addr: CODE_ADDR,
                    ..
                })
            ));
            let ArchRegState::Aarch64(regs) = &context.arch_regs else {
                panic!("execution changed architecture");
            };
            assert_eq!(regs.v, before.v);
            assert_eq!(regs.x, before.x);
            assert_eq!(regs.fpsr, before.fpsr);
            assert_eq!(regs.nzcv, before.nzcv);
        }
    }
}

#[test]
fn arm_resize_rejects_a_foreign_register_state_at_the_guest_frontier() {
    for kind in [
        OpKind::ArmSimdLong(long_operation()),
        OpKind::ArmSimdNarrow(narrow_operation()),
    ] {
        let function = function_with_operation(kind);
        let mut context = SmirContext::new_x86_64();
        context.pc = 0xdead_0000;
        let mut memory = rax::smir::ir::memory::FlatMemory::new(1);
        let result =
            SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::Undefined {
                addr: CODE_ADDR,
                ..
            })
        ));
        assert!(matches!(context.arch_regs, ArchRegState::X86_64(_)));
    }
}

#[test]
fn saturating_virtual_results_retain_observable_qc_after_dead_code_elimination() {
    let kind = OpKind::ArmSimdNarrow(ArmSimdNarrowOp {
        dst: VReg::virt(0),
        elem: VecElementType::I8,
        kind: ArmSimdNarrowKind::Extract {
            mode: ArmSimdNarrowMode::SignedSaturate,
        },
        ..narrow_operation()
    });
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut function = function_with_operation(kind.clone());
        optimize_function(&mut function, level);
        assert_eq!(function.blocks[0].ops.len(), 1, "{level:?}");
        let mut context = SmirContext::new_aarch64();
        let ArchRegState::Aarch64(regs) = &mut context.arch_regs else {
            panic!("expected AArch64 state");
        };
        regs.v[1] = [0x7fff_8000_00ff_ff00; 2];
        regs.fpsr = 0x0090;
        let mut memory = rax::smir::ir::memory::FlatMemory::new(1);
        let result =
            SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::Return { .. })
        ));
        assert_eq!(
            context.read_arch_reg(ArchReg::Arm(ArmReg::Fpsr)),
            0x0800_0090
        );
    }
}

#[test]
#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
fn native_resize_rejects_virtual_registers_and_foreign_provenance() {
    use rax::smir::ir::ops::X86OpHint;

    for mut operation in [
        OpKind::ArmSimdLong(long_operation()),
        OpKind::ArmSimdNarrow(narrow_operation()),
    ] {
        match &mut operation {
            OpKind::ArmSimdLong(long) => long.dst = VReg::virt(0),
            OpKind::ArmSimdNarrow(narrow) => narrow.src1 = VReg::virt(0),
            _ => panic!("unexpected operation"),
        }
        super::assert_native_rejected(&function_with_operation(operation));
    }
    let mut function = function_with_operation(OpKind::ArmSimdLong(long_operation()));
    function.blocks[0].ops[0].x86_hint = Some(X86OpHint::XopVpcom);
    super::assert_native_rejected(&function);
}
