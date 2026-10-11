//! QC liveness, precise malformed-IR frontiers, and native admission contracts.

use rax::isa::arm::decoder::{Mnemonic, aarch64::Aarch64Decoder};
use rax::smir::interpret::{BlockResult, SmirInterpreter};
use rax::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use rax::smir::ir::ops::{
    ArmSimdSaturatingForm, ArmSimdSaturatingKind, ArmSimdSaturatingOp, OpKind,
};
use rax::smir::ir::types::{ArchReg, ArmReg, FunctionId, VReg, VecElementType};
use rax::smir::ir::{FunctionBuilder, SmirFunction, Terminator};
use rax::smir::lower::{SmirLowerer, aarch64::Aarch64Lowerer};
use rax::smir::optimize::{OptLevel, optimize_function};

use super::{CODE_ADDR, lift_program};

fn v(index: u8) -> VReg {
    VReg::Arch(ArchReg::Arm(ArmReg::V(index)))
}

fn operation() -> ArmSimdSaturatingOp {
    ArmSimdSaturatingOp {
        dst: v(0),
        src1: v(1),
        src2: Some(v(2)),
        elem: VecElementType::I16,
        kind: ArmSimdSaturatingKind::Add,
        signed: true,
        form: ArmSimdSaturatingForm::Vector128,
    }
}

fn function_with_operation(operation: ArmSimdSaturatingOp) -> SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), CODE_ADDR);
    builder.push_op(CODE_ADDR, OpKind::ArmSimdSaturating(operation));
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.finish()
}

#[test]
fn saturating_metadata_tracks_qc_and_mixed_accumulator_without_generic_admission() {
    let fpsr = VReg::Arch(ArchReg::Arm(ArmReg::Fpsr));
    let cases = [
        (0x4e22_0c20, vec![v(1), v(2), fpsr]), // SQADD
        (0x6e22_2c20, vec![v(1), v(2), fpsr]), // UQSUB
        (0x4e20_3820, vec![v(1), v(0), fpsr]), // SUQADD
        (0x6e20_3820, vec![v(1), v(0), fpsr]), // USQADD
        (0x4e20_7820, vec![v(1), fpsr]),       // SQABS
        (0x5e60_b420, vec![v(1), v(0), fpsr]), // SQDMULH H0,H1,H0
    ];
    for (instruction, sources) in cases {
        let function = lift_program(&[instruction]);
        let op = &function.blocks[0].ops[0];
        assert_eq!(op.kind.source_vregs(), sources, "{instruction:#010x}");
        assert_eq!(op.kind.dests(), vec![v(0), fpsr]);
        assert!(op.kind.has_side_effects());
        assert!(!op.kind.is_jit_safe());
        assert!(!op.is_jit_safe());
    }
}

#[test]
fn reserved_saturating_shapes_decode_to_undefined() {
    for instruction in [
        0x0ee2_0c20, // Vector SQADD 1D is unallocated.
        0x0ee0_7820, // Vector SQABS 1D is unallocated.
        0x0f40_7420, // Vector SQSHL with 64-bit lanes and Q=0.
        0x0e22_b420, // SQDMULH cannot have 8-bit lanes.
        0x4ee2_b420, // SQDMULH cannot have 64-bit lanes.
        0x5f02_c020, // Scalar indexed SQDMULH cannot have 8-bit lanes.
        0x4fc2_d020, // Indexed SQRDMULH cannot have 64-bit lanes.
    ] {
        let decoded = Aarch64Decoder::decode(instruction).unwrap();
        assert_eq!(decoded.mnemonic, Mnemonic::UNDEFINED, "{instruction:#010x}");
    }
}

fn malformed_operations() -> Vec<ArmSimdSaturatingOp> {
    let mut cases = Vec::new();
    for elem in [
        VecElementType::F16,
        VecElementType::F32,
        VecElementType::F64,
    ] {
        cases.push(ArmSimdSaturatingOp {
            elem,
            ..operation()
        });
    }
    for reg in [
        v(32),
        v(255),
        VReg::Imm(1),
        VReg::Arch(ArchReg::Arm(ArmReg::X(0))),
    ] {
        cases.push(ArmSimdSaturatingOp {
            dst: reg,
            ..operation()
        });
        cases.push(ArmSimdSaturatingOp {
            src1: reg,
            ..operation()
        });
        cases.push(ArmSimdSaturatingOp {
            src2: Some(reg),
            ..operation()
        });
    }
    cases.push(ArmSimdSaturatingOp {
        src2: None,
        ..operation()
    });
    cases.push(ArmSimdSaturatingOp {
        elem: VecElementType::I64,
        form: ArmSimdSaturatingForm::Vector64,
        ..operation()
    });
    for kind in [
        ArmSimdSaturatingKind::Absolute,
        ArmSimdSaturatingKind::Negate,
        ArmSimdSaturatingKind::MixedAccumulate,
    ] {
        cases.push(ArmSimdSaturatingOp {
            kind,
            ..operation()
        }); // Unexpected second source.
    }
    for kind in [
        ArmSimdSaturatingKind::Absolute,
        ArmSimdSaturatingKind::Negate,
    ] {
        cases.push(ArmSimdSaturatingOp {
            kind,
            src2: None,
            signed: false,
            ..operation()
        });
    }
    for amount in [16, 255] {
        cases.push(ArmSimdSaturatingOp {
            kind: ArmSimdSaturatingKind::ShiftImmediate { amount },
            src2: None,
            ..operation()
        });
    }
    cases.push(ArmSimdSaturatingOp {
        kind: ArmSimdSaturatingKind::ShiftSignedToUnsigned { amount: 0 },
        src2: None,
        signed: true,
        ..operation()
    });
    for (elem, signed, lane, source) in [
        (VecElementType::I8, true, None, v(2)),
        (VecElementType::I64, true, None, v(2)),
        (VecElementType::I16, false, None, v(2)),
        (VecElementType::I16, true, Some(8), v(2)),
        (VecElementType::I32, true, Some(4), v(2)),
        (VecElementType::I16, true, Some(0), v(16)),
    ] {
        cases.push(ArmSimdSaturatingOp {
            kind: ArmSimdSaturatingKind::DoublingMultiplyHigh {
                rounding: true,
                lane,
            },
            elem,
            signed,
            src2: Some(source),
            ..operation()
        });
    }
    cases
}

#[test]
fn malformed_saturation_rejects_before_mutation_at_the_guest_pc_after_optimization() {
    for operation in malformed_operations() {
        let original = function_with_operation(operation);
        assert!(Aarch64Lowerer::new().lower_function(&original).is_err());
        #[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
        super::assert_native_rejected(&original);
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let mut function = original.clone();
            optimize_function(&mut function, level);
            let mut context = SmirContext::new_aarch64();
            context.pc = 0xdead_0000;
            let ArchRegState::Aarch64(regs) = &mut context.arch_regs else {
                panic!("expected AArch64 state");
            };
            regs.v = [[0xa55a_ffff_0123_4567, 0x5aa5_1234_ffff_0000]; 32];
            regs.x = [0x1234; 31];
            regs.fpsr = 0x0090;
            regs.nzcv = 0xa000_0000;
            let before = regs.clone();
            let result = SmirInterpreter::new().execute_block(
                &mut context,
                &mut rax::smir::ir::memory::FlatMemory::new(1),
                &function.blocks[0],
            );
            assert!(
                matches!(
                    result,
                    BlockResult::Exit(ExitReason::Undefined {
                        addr: CODE_ADDR,
                        ..
                    })
                ),
                "{operation:?} at {level:?}: {result:?}"
            );
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
fn dead_saturating_virtual_results_preserve_qc_after_optimization() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut function = function_with_operation(ArmSimdSaturatingOp {
            dst: VReg::virt(0),
            elem: VecElementType::I8,
            ..operation()
        });
        optimize_function(&mut function, level);
        assert_eq!(function.blocks[0].ops.len(), 1, "{level:?}");
        let mut context = SmirContext::new_aarch64();
        let ArchRegState::Aarch64(regs) = &mut context.arch_regs else {
            panic!("expected AArch64 state");
        };
        regs.v[1] = [0x7f7f_7f7f_7f7f_7f7f; 2];
        regs.v[2] = [0x0101_0101_0101_0101; 2];
        regs.fpsr = 0x0090;
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut rax::smir::ir::memory::FlatMemory::new(1),
            &function.blocks[0],
        );
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::Return { .. })
        ));
        let ArchRegState::Aarch64(regs) = context.arch_regs else {
            panic!("execution changed architecture");
        };
        assert_eq!(regs.fpsr, 0x0800_0090);
    }
}

#[test]
fn saturation_rejects_foreign_architectural_state_without_mutation() {
    let function = function_with_operation(operation());
    let mut context = SmirContext::new_x86_64();
    context.pc = 0xdead_0000;
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut rax::smir::ir::memory::FlatMemory::new(1),
        &function.blocks[0],
    );
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Undefined {
            addr: CODE_ADDR,
            ..
        })
    ));
    assert!(matches!(context.arch_regs, ArchRegState::X86_64(_)));
}

#[test]
fn native_saturation_requires_architectural_vectors_and_unambiguous_provenance() {
    for operation in [
        ArmSimdSaturatingOp {
            dst: VReg::virt(0),
            ..operation()
        },
        ArmSimdSaturatingOp {
            src1: VReg::virt(0),
            ..operation()
        },
        ArmSimdSaturatingOp {
            src2: Some(VReg::virt(0)),
            ..operation()
        },
    ] {
        let function = function_with_operation(operation);
        assert!(Aarch64Lowerer::new().lower_function(&function).is_err());
        #[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
        super::assert_native_rejected(&function);
    }
    let mut function = lift_program(&[0x4e22_0c20]);
    function.blocks[0].ops[0].x86_hint = Some(rax::smir::ir::ops::X86OpHint::XopVpcom);
    assert!(Aarch64Lowerer::new().lower_function(&function).is_err());
    #[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
    super::assert_native_rejected(&function);
}
