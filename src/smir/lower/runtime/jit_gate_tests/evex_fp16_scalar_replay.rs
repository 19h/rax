//! Native replay coverage for scalar AVX-512-FP16 arithmetic and square root.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x1000;
const SCALAR_FP16_ARITHMETIC_OPCODES: [u8; 7] = [0x51, 0x58, 0x59, 0x5C, 0x5D, 0x5E, 0x5F];

fn scalar_fp16_function(bytes: &[u8; 6]) -> crate::smir::ir::SmirFunction {
    use crate::smir::ir::{SmirBlock, SmirFunction, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(bytes).unwrap());
    function
}

#[test]
fn scalar_fp16_arithmetic_replay_admits_every_opcode_llig_and_embedded_control() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let expected_host_features = || {
        #[cfg(target_arch = "x86_64")]
        {
            std::is_x86_feature_detected!("avx512f")
                && std::is_x86_feature_detected!("avx512bw")
                && std::is_x86_feature_detected!("avx512fp16")
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            false
        }
    };

    let mut admitted = 0usize;
    for opcode in SCALAR_FP16_ARITHMETIC_OPCODES {
        let encodings = [
            [0x62, 0xF5, 0x7E, 0x09, opcode, 0xC8],
            [0x62, 0xF5, 0x7E, 0x29, opcode, 0xC8],
            [0x62, 0xF5, 0x7E, 0x49, opcode, 0xC8],
            [0x62, 0xF5, 0x7E, 0x69, opcode, 0xC8],
            [0x62, 0xF5, 0x7E, 0x19, opcode, 0xC8],
            [0x62, 0xF5, 0x7E, 0x39, opcode, 0xC8],
            [0x62, 0xF5, 0x7E, 0x59, opcode, 0xC8],
            [0x62, 0xF5, 0x7E, 0x79, opcode, 0xC8],
            [0x62, 0xA5, 0x6E, 0x81, opcode, 0xCB],
        ];

        for bytes in &encodings {
            let mut function = scalar_fp16_function(bytes);
            let mut missing_provenance = function.clone();
            missing_provenance.x86_instruction_bytes.clear();
            crate::smir::optimize::optimize_function(
                &mut missing_provenance,
                crate::smir::optimize::OptLevel::O2,
            );
            assert!(!is_native_clobber_safe(&missing_provenance), "{bytes:02X?}");

            crate::smir::optimize::optimize_function(
                &mut function,
                crate::smir::optimize::OptLevel::O2,
            );
            assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
            assert!(
                uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
                "{bytes:02X?}"
            );
            assert_eq!(
                x86_native_vector_features_supported_excluding(
                    &function,
                    &std::collections::HashMap::new()
                ),
                expected_host_features(),
                "{bytes:02X?}"
            );

            let mut lowerer = X86_64Lowerer::new();
            lowerer
                .lower_function(&function)
                .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
            assert!(
                code.windows(bytes.len()).any(|window| window == bytes),
                "{bytes:02X?}"
            );
            admitted += 1;
        }

        let high_registers = [0x62, 0xA5, 0x6E, 0x81, opcode, 0xCB];
        let mut memory_metadata = scalar_fp16_function(&high_registers);
        memory_metadata.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&[0x62, 0xA5, 0x6E, 0x81, opcode, 0x0B])
                .unwrap(),
        );
        assert!(
            !is_native_clobber_safe(&memory_metadata),
            "opcode={opcode:#04x}"
        );
    }
    assert_eq!(admitted, 63);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct ScalarFp16State {
    dst: [u64; 8],
    src1: [u64; 8],
    src2: [u64; 8],
    mask: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn interpret_scalar_fp16(bytes: &[u8; 6], initial: &ScalarFp16State) -> ScalarFp16State {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::TrapKind;
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::memory::FlatMemory;

    let mut function = scalar_fp16_function(bytes);
    function.blocks[0].set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.xmm[17][..8].copy_from_slice(&initial.dst);
        x86.xmm[18][..8].copy_from_slice(&initial.src1);
        x86.xmm[19][..8].copy_from_slice(&initial.src2);
        x86.k[1] = initial.mask;
        x86.mxcsr = initial.mxcsr;
    }
    let mut memory = FlatMemory::new(1);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut dst = [0u64; 8];
    let mut src1 = [0u64; 8];
    let mut src2 = [0u64; 8];
    dst.copy_from_slice(&x86.xmm[17][..8]);
    src1.copy_from_slice(&x86.xmm[18][..8]);
    src2.copy_from_slice(&x86.xmm[19][..8]);
    ScalarFp16State {
        dst,
        src1,
        src2,
        mask: x86.k[1],
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native_scalar_fp16(bytes: &[u8; 6], initial: &ScalarFp16State) -> ScalarFp16State {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = scalar_fp16_function(bytes);
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map scalar FP16 replay");
    let mut registers = GuestRegs {
        vector_active: 1,
        mxcsr: initial.mxcsr,
        ..GuestRegs::default()
    };
    registers.set_zmm(17, initial.dst);
    registers.set_zmm(18, initial.src1);
    registers.set_zmm(19, initial.src2);
    registers.k[1] = initial.mask;
    exec.run(lowered.entry_offset, &mut registers);

    ScalarFp16State {
        dst: registers.get_zmm(17),
        src1: registers.get_zmm(18),
        src2: registers.get_zmm(19),
        mask: registers.k[1],
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn assert_scalar_fp16_native_matches_interpreter(
    bytes: &[u8; 6],
    initial: &ScalarFp16State,
) -> ScalarFp16State {
    let interpreted = interpret_scalar_fp16(bytes, initial);
    let native = execute_native_scalar_fp16(bytes, initial);
    assert_eq!(native, interpreted, "{bytes:02X?}");
    interpreted
}

#[cfg(target_arch = "x86_64")]
#[test]
fn scalar_fp16_arithmetic_replay_matches_interpreter_masks_rounding_sae_and_upper_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512fp16")
    {
        eprintln!("skipping native scalar FP16 differential: host lacks AVX-512-FP16 state");
        return;
    }

    let destination = [0xA55A_A55A_A55A_3555; 8];
    let mut source1 = [0x1122_3344_5566_7788; 8];
    source1[0] = (source1[0] & !0xFFFF) | 0x3E00; // 1.5
    let mut source2 = [0x8877_6655_4433_2211; 8];
    source2[0] = (source2[0] & !0xFFFF) | 0x4000; // 2.0
    let initial = ScalarFp16State {
        dst: destination,
        src1: source1,
        src2: source2,
        mask: 1,
        mxcsr: 0x1F80,
    };

    for opcode in SCALAR_FP16_ARITHMETIC_OPCODES {
        let bytes = [0x62, 0xA5, 0x6E, 0x81, opcode, 0xCB];
        let result = assert_scalar_fp16_native_matches_interpreter(&bytes, &initial);
        assert_eq!(result.src1, source1, "{bytes:02X?}");
        assert_eq!(result.src2, source2, "{bytes:02X?}");
        assert_eq!(result.mask, 1, "{bytes:02X?}");
        assert_eq!(
            result.dst[0] & !0xFFFF,
            source1[0] & !0xFFFF,
            "{bytes:02X?}"
        );
        assert_eq!(result.dst[1], source1[1], "{bytes:02X?}");
        assert_eq!(&result.dst[2..], &[0; 6], "{bytes:02X?}");
    }

    // 1.0 + 2^-11 is halfway between adjacent FP16 values. Embedded RU-SAE
    // must override MXCSR.RD, choose 0x3C01, and suppress precision status.
    let mut rounded = initial.clone();
    rounded.src1[0] = (rounded.src1[0] & !0xFFFF) | 0x3C00;
    rounded.src2[0] = (rounded.src2[0] & !0xFFFF) | 0x1000;
    rounded.mxcsr = 0x3F80;
    let result = assert_scalar_fp16_native_matches_interpreter(
        &[0x62, 0xA5, 0x6E, 0xD1, 0x58, 0xCB],
        &rounded,
    );
    assert_eq!(result.dst[0] & 0xFFFF, 0x3C01);
    assert_eq!(result.mxcsr, rounded.mxcsr);

    // A zero-masked 0/0 division must neither inspect the exceptional operands
    // nor set MXCSR status; scalar upper bits still merge from source 1.
    let mut masked = initial.clone();
    masked.src1[0] &= !0xFFFF;
    masked.src2[0] &= !0xFFFF;
    masked.mask = 0;
    let result = assert_scalar_fp16_native_matches_interpreter(
        &[0x62, 0xA5, 0x6E, 0x81, 0x5E, 0xCB],
        &masked,
    );
    assert_eq!(result.dst[0] & 0xFFFF, 0);
    assert_eq!(result.mxcsr, masked.mxcsr);

    // VMAXSH {sae} selects source 2's signaling-NaN payload without accruing
    // invalid status in MXCSR.
    let mut sae = initial.clone();
    sae.src2[0] = (sae.src2[0] & !0xFFFF) | 0x7C01;
    let result =
        assert_scalar_fp16_native_matches_interpreter(&[0x62, 0xA5, 0x6E, 0x91, 0x5F, 0xCB], &sae);
    assert_eq!(result.dst[0] & 0xFFFF, 0x7C01);
    assert_eq!(result.mxcsr, sae.mxcsr);
}
