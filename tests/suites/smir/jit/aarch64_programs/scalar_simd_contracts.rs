//! Reserved scalar encodings, exact native shapes, and private compositions.

use rax::isa::arm::decoder::{Mnemonic, aarch64::Aarch64Decoder};
use rax::isa::arm::{AArch64Config, AArch64Cpu, ArmCpu, ArmError, FlatMemory};
use rax::smir::ir::ops::{OpKind, X86OpHint};
use rax::smir::ir::types::{
    ArchReg, ArmReg, SourceArch, VReg, VecCmpCond, VecElementType, VecReduceOp,
};
use rax::smir::ir::{Terminator, TrapKind};
use rax::smir::lift::aarch64::Aarch64Lifter;
use rax::smir::lift::{ControlFlow, LiftContext, LiftError, SmirLifter};
use rax::smir::lower::{SmirLowerer, aarch64::Aarch64Lowerer};

use super::{CODE_ADDR, MEMORY_SIZE, initial_state, lift_program};

fn v(index: u8) -> VReg {
    VReg::Arch(ArchReg::Arm(ArmReg::V(index)))
}

#[test]
fn scalar_integer_reserved_sizes_and_vector_one_d_are_undefined() {
    for base in [
        0x5ee2_8420,
        0x5ee2_3420,
        0x5ee2_4420,
        0x5ee0_b820,
        0x5ee0_9820,
        0x5ef1_b820,
    ] {
        for size in 0..3 {
            let instruction = (base & !(3 << 22)) | (size << 22);
            assert_eq!(
                Aarch64Decoder::decode(instruction).unwrap().mnemonic,
                Mnemonic::UNDEFINED,
                "{instruction:#010x}"
            );
        }
    }
    for instruction in [
        0x0ee0_8820,
        0x2ee0_8820,
        0x0ee0_9820,
        0x2ee0_9820,
        0x0ee0_a820,
        0x0ee0_b820,
        0x2ee0_b820,
        0x0ee2_8420,
        0x2ee2_8420,
        0x0ee2_3420,
        0x0ef1_b820,
        0x4ef1_b820,
        0x0eb1_b820,
        0x5f20_5420,
        0x5f20_0420,
        0x5f20_2420,
        0x5f20_3420,
        0x7ef1_b820,
    ] {
        assert_eq!(
            Aarch64Decoder::decode(instruction).unwrap().mnemonic,
            Mnemonic::UNDEFINED,
            "{instruction:#010x}"
        );
    }
}

#[test]
fn reserved_unary_and_reduction_encodings_fault_before_register_writes() {
    for instruction in [
        0x0ee0_b820u32,
        0x2ee0_b820,
        0x0ef1_b820,
        0x4ef1_b820,
        0x0eb1_b820,
    ] {
        let state = initial_state(0x8000_0000_0000_0001);
        let mut cpu = AArch64Cpu::new(
            AArch64Config::default(),
            Box::new(FlatMemory::new(0, MEMORY_SIZE)),
        );
        #[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
        cpu.set_jit_enabled(false);
        cpu.write_memory(CODE_ADDR, &instruction.to_le_bytes())
            .unwrap();
        cpu.set_pc(CODE_ADDR);
        for (index, value) in state.x.iter().enumerate() {
            cpu.set_x(index as u8, *value);
        }
        for (index, value) in state.v.iter().enumerate() {
            let bits = u128::from(value[0]) | (u128::from(value[1]) << 64);
            cpu.set_simd(index as u8, bits);
        }
        cpu.set_nzcv_bits((state.nzcv >> 28) as u8);
        cpu.set_fpsr(state.fpsr).unwrap();

        assert!(
            matches!(cpu.step(), Err(ArmError::UndefinedInstruction(word)) if word == instruction),
            "{instruction:#010x}"
        );
        assert_eq!(cpu.get_pc(), CODE_ADDR, "{instruction:#010x}");
        for (index, expected) in state.x.iter().enumerate() {
            assert_eq!(cpu.get_x(index as u8), *expected, "X{index}");
        }
        for (index, expected) in state.v.iter().enumerate() {
            let bits = u128::from(expected[0]) | (u128::from(expected[1]) << 64);
            assert_eq!(cpu.get_simd(index as u8), bits, "V{index}");
        }
        assert_eq!(u32::from(cpu.nzcv_bits()) << 28, state.nzcv);
        assert_eq!(cpu.get_fpsr().unwrap(), state.fpsr);

        let mut context = LiftContext::new(SourceArch::Aarch64);
        let result = Aarch64Lifter::new()
            .lift_insn(CODE_ADDR, &instruction.to_le_bytes(), &mut context)
            .unwrap();
        assert!(
            matches!(
                result.control_flow,
                ControlFlow::Trap {
                    kind: TrapKind::Undefined
                }
            ),
            "reserved encoding {instruction:#010x} must produce an undefined trap frontier"
        );
        assert!(result.ops.is_empty(), "{instruction:#010x}");
        assert_eq!(result.bytes_consumed, 4, "{instruction:#010x}");

        let strict_result = Aarch64Lifter::strict().lift_insn(
            CODE_ADDR,
            &instruction.to_le_bytes(),
            &mut LiftContext::new(SourceArch::Aarch64),
        );
        assert!(
            matches!(strict_result, Err(LiftError::Unsupported { addr, .. }) if addr == CODE_ADDR),
            "strict lifting must reject reserved encoding {instruction:#010x}"
        );
    }
}

#[test]
fn integer_reductions_reject_reserved_source_arrangements() {
    for operation in [
        VecReduceOp::Add,
        VecReduceOp::SMax,
        VecReduceOp::UMax,
        VecReduceOp::SMin,
        VecReduceOp::UMin,
        VecReduceOp::SAddLong,
        VecReduceOp::UAddLong,
    ] {
        for (element, lanes) in [(VecElementType::I32, 2), (VecElementType::I64, 1)] {
            let mut function = lift_program(&[0x5ef1_b820]);
            function.blocks[0].ops[0].kind = OpKind::VReduce {
                dst: v(0),
                src: v(1),
                elem: element,
                lanes,
                op: operation,
            };
            assert!(
                Aarch64Lowerer::new().lower_function(&function).is_err(),
                "{operation:?} {element:?}x{lanes}"
            );
        }
    }
}

#[test]
fn scalar_and_zero_compare_metadata_preserve_status_without_false_writes() {
    for instruction in [
        0x5ee2_8420u32,
        0x7ee2_8420,
        0x5ee0_b820,
        0x7ee0_b820,
        0x5ee0_8820,
        0x7ee0_8820,
        0x5ee0_9820,
        0x7ee0_9820,
        0x5ee0_a820,
        0x4e20_9820,
        0x5ef1_b820,
    ] {
        let function = lift_program(&[instruction]);
        assert_eq!(function.blocks[0].ops.len(), 1, "{instruction:#010x}");
        let operation = &function.blocks[0].ops[0];
        assert_eq!(operation.kind.dests(), vec![v(0)], "{instruction:#010x}");
        assert!(!operation.kind.has_side_effects(), "{instruction:#010x}");
        let sources = operation.kind.source_vregs();
        assert!(sources.contains(&v(1)), "{instruction:#010x}: {sources:?}");
        assert!(!sources.contains(&VReg::Arch(ArchReg::Arm(ArmReg::Fpsr))));
    }
}

#[test]
fn native_scalar_operations_reject_foreign_provenance_and_invalid_registers() {
    for instruction in [
        0x5ee2_8420u32,
        0x5ee2_3420,
        0x5ee0_b820,
        0x5ef1_b820,
        0x5f41_5420,
    ] {
        let mut function = lift_program(&[instruction]);
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::XopVpcom);
        assert!(
            Aarch64Lowerer::new().lower_function(&function).is_err(),
            "{instruction:#010x}"
        );
        #[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
        super::assert_native_rejected(&function);
    }
    for source in [v(32), v(255), VReg::virt(0), VReg::Imm(1)] {
        let mut function = lift_program(&[0x5ee0_9820]);
        let OpKind::VCmp { src1, .. } = &mut function.blocks[0].ops[0].kind else {
            panic!("scalar CMEQ zero must use VCmp");
        };
        *src1 = source;
        assert!(
            Aarch64Lowerer::new().lower_function(&function).is_err(),
            "{source:?}"
        );
    }
}

#[test]
fn native_zero_comparisons_require_supported_conditions_and_a_literal_zero() {
    for cond in [
        VecCmpCond::Ne,
        VecCmpCond::Gtu,
        VecCmpCond::Geu,
        VecCmpCond::Ltu,
        VecCmpCond::Leu,
    ] {
        let mut function = lift_program(&[0x5ee0_9820]);
        let OpKind::VCmp {
            cond: comparison, ..
        } = &mut function.blocks[0].ops[0].kind
        else {
            panic!("scalar CMEQ zero must use VCmp");
        };
        *comparison = cond;
        assert!(
            Aarch64Lowerer::new().lower_function(&function).is_err(),
            "{cond:?}"
        );
    }
    let mut function = lift_program(&[0x5ee0_9820]);
    let OpKind::VCmp { src2, .. } = &mut function.blocks[0].ops[0].kind else {
        panic!("scalar CMEQ zero must use VCmp");
    };
    *src2 = VReg::Imm(1);
    assert!(Aarch64Lowerer::new().lower_function(&function).is_err());
}

#[test]
fn scalar_bit_test_and_shift_compositions_reject_escaping_or_misattributed_temporaries() {
    for instruction in [0x5ee2_8c20u32, 0x5f41_2420, 0x7f40_3420] {
        let function = lift_program(&[instruction]);
        assert!(function.blocks[0].ops.len() >= 2, "{instruction:#010x}");
        let temporary = function.blocks[0].ops[0].kind.dests()[0];
        assert!(matches!(temporary, VReg::Virtual(_)));

        let mut escaping = function.clone();
        escaping.blocks[0].terminator = Terminator::Return {
            values: vec![temporary],
        };
        assert!(Aarch64Lowerer::new().lower_function(&escaping).is_err());
        #[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
        super::assert_native_rejected(&escaping);

        let mut wrong_pc = function.clone();
        wrong_pc.blocks[0].ops[1].guest_pc += 4;
        assert!(Aarch64Lowerer::new().lower_function(&wrong_pc).is_err());

        let mut hinted = function;
        hinted.blocks[0].ops[1].x86_hint = Some(X86OpHint::XopVpcom);
        assert!(Aarch64Lowerer::new().lower_function(&hinted).is_err());
    }
}

#[test]
fn integer_scalar_admission_does_not_enable_unallocated_operations() {
    let mut function = lift_program(&[0x5ee2_8420]);
    function.blocks[0].ops[0].kind = OpKind::VMul {
        dst: v(0),
        src1: v(1),
        src2: v(2),
        elem: VecElementType::I64,
        lanes: 1,
    };
    assert!(Aarch64Lowerer::new().lower_function(&function).is_err());
    for (elem, lanes) in [
        (VecElementType::I8, 1),
        (VecElementType::I16, 1),
        (VecElementType::I32, 1),
        (VecElementType::I64, 3),
        (VecElementType::F64, 1),
    ] {
        let mut function = lift_program(&[0x5ee0_9820]);
        let OpKind::VCmp {
            elem: element,
            lanes: lane_count,
            ..
        } = &mut function.blocks[0].ops[0].kind
        else {
            panic!("scalar CMEQ zero must use VCmp");
        };
        *element = elem;
        *lane_count = lanes;
        assert!(
            Aarch64Lowerer::new().lower_function(&function).is_err(),
            "{elem:?}x{lanes}"
        );
    }
}
