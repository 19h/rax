//! Native replay coverage for register-only EVEX VMOVHLPS/VMOVLHPS.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x1014;

fn encoding(opcode: u8, destination: u8, merge: u8, source: u8) -> [u8; 6] {
    assert!(matches!(opcode, 0x12 | 0x16));
    assert!(destination < 32 && merge < 32 && source < 32);

    let mut p0 = 0xF1;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source & 0x10 != 0 {
        p0 &= !0x40;
    }

    [
        0x62,
        p0,
        ((!merge) & 0x0F) << 3 | 0x04,
        if merge < 16 { 0x08 } else { 0 },
        opcode,
        0xC0 | ((destination & 0x07) << 3) | (source & 0x07),
    ]
}

fn function(bytes: &[u8]) -> crate::smir::ir::SmirFunction {
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
fn replay_feature_aggregation_requires_only_base_avx512_state() {
    for bytes in [encoding(0x12, 31, 30, 29), encoding(0x16, 17, 16, 31)] {
        let function = function(&bytes);
        let actual =
            x86_native_replay_feature_requirements(&function, &std::collections::HashMap::new());
        assert!(actual.any, "{bytes:02X?}");
        assert!(actual.needs_avx512bw, "{bytes:02X?}");
        assert!(!actual.needs_avx512vl, "{bytes:02X?}");
        assert!(!actual.needs_avx512dq, "{bytes:02X?}");
        assert!(!actual.needs_avx512fp16, "{bytes:02X?}");
        assert!(!actual.needs_avx512cd, "{bytes:02X?}");
        assert!(!actual.needs_gfni, "{bytes:02X?}");
        assert!(!actual.needs_avx512vp2intersect, "{bytes:02X?}");
        assert!(!actual.needs_vpclmulqdq, "{bytes:02X?}");

        let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
        assert_eq!(
            x86_native_replay_feature_requirements(&function, &excluded),
            X86NativeReplayFeatureRequirements::default(),
            "{bytes:02X?}"
        );
    }
}

#[test]
fn replay_admits_and_emits_40_o0_o2_safe_encodings_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let operands = [
        (1, 2, 3),
        (9, 10, 11),
        (17, 18, 19),
        (25, 26, 27),
        (31, 31, 31),
        (1, 1, 3),
        (2, 3, 2),
        (4, 4, 4),
        (1, 17, 30),
        (31, 0, 16),
    ];

    let mut lowered = 0usize;
    for opcode in [0x12, 0x16] {
        for (destination, merge, source) in operands {
            let bytes = encoding(opcode, destination, merge, source);
            for optimize in [false, true] {
                let mut function = function(&bytes);
                if optimize {
                    crate::smir::optimize::optimize_function(
                        &mut function,
                        crate::smir::optimize::OptLevel::O2,
                    );
                }
                assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
                assert!(
                    uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
                    "{bytes:02X?}"
                );

                #[cfg(target_arch = "x86_64")]
                let expected_features = std::is_x86_feature_detected!("avx512f")
                    && std::is_x86_feature_detected!("avx512bw");
                #[cfg(not(target_arch = "x86_64"))]
                let expected_features = false;
                assert_eq!(
                    x86_native_vector_features_supported_excluding(
                        &function,
                        &std::collections::HashMap::new()
                    ),
                    expected_features,
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
                lowered += 1;
            }
        }
    }
    assert_eq!(lowered, 40);

    for opcode in [0x12, 0x16] {
        let bytes = encoding(opcode, 1, 2, 3);
        let mut missing = function(&bytes);
        missing.x86_instruction_bytes.clear();
        crate::smir::optimize::optimize_function(&mut missing, crate::smir::optimize::OptLevel::O2);
        assert!(!is_native_clobber_safe(&missing), "{bytes:02X?}");

        let mut memory = bytes;
        memory[5] &= 0x3F;
        let mut malformed = function(&bytes);
        malformed.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&memory).unwrap(),
        );
        assert!(!is_native_clobber_safe(&malformed), "{memory:02X?}");
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct MoveState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn initial_state() -> MoveState {
    MoveState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0xF0E1_D2C3_B4A5_9687u64.rotate_left((register * 11 + word * 5) as u32)
                    ^ (register as u64).wrapping_mul(0x1111_2222_3333_4444)
                    ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
            })
        }),
        masks: [
            0x6996_F00F_3CC3_A55A,
            0,
            1,
            0x0123_4567_89AB_CDEF,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0000,
            0xF0F0_0F0F_A5A5_5A5A,
            u64::MAX,
        ],
        rflags: 0x2 | 0x8D5,
        mxcsr: 0x1F80 | (2 << 13) | (1 << 6) | (1 << 15),
    }
}

#[cfg(target_arch = "x86_64")]
fn interpret(bytes: &[u8], initial: &MoveState, optimize: bool) -> MoveState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::TrapKind;
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::memory::FlatMemory;

    let mut function = function(bytes);
    function.blocks[0].set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    if optimize {
        crate::smir::optimize::optimize_function(
            &mut function,
            crate::smir::optimize::OptLevel::O2,
        );
    }
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gprs;
        for (index, value) in initial.vectors.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.masks;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
    }
    let mut memory = FlatMemory::new(1);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut vectors = [[0u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    MoveState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(bytes: &[u8], initial: &MoveState, optimize: bool) -> MoveState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut function = function(bytes);
    if optimize {
        crate::smir::optimize::optimize_function(
            &mut function,
            crate::smir::optimize::OptLevel::O2,
        );
    }
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map EVEX VMOVHLPS/VMOVLHPS replay");
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
        vector_active: 1,
        k: initial.masks,
        mxcsr: initial.mxcsr,
        ..GuestRegs::default()
    };
    for (index, value) in initial.vectors.iter().enumerate() {
        registers.set_zmm(index, *value);
    }
    exec.run(lowered.entry_offset, &mut registers);

    let mut vectors = [[0u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        *value = registers.get_zmm(index);
    }
    MoveState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn interpretation_matches_intel_lane_equations_and_zeroes_upper_state() {
    let initial = initial_state();
    let operands = [
        (1, 2, 3),
        (2, 2, 3),
        (3, 2, 3),
        (4, 4, 4),
        (5, 6, 6),
        (17, 18, 19),
    ];
    for opcode in [0x12, 0x16] {
        for (destination, merge, source) in operands {
            let bytes = encoding(opcode, destination, merge, source);
            let mut expected = initial.clone();
            expected.vectors[destination as usize] = [0; 8];
            if opcode == 0x12 {
                expected.vectors[destination as usize][0] = initial.vectors[source as usize][1];
                expected.vectors[destination as usize][1] = initial.vectors[merge as usize][1];
            } else {
                expected.vectors[destination as usize][0] = initial.vectors[merge as usize][0];
                expected.vectors[destination as usize][1] = initial.vectors[source as usize][0];
            }
            for optimize in [false, true] {
                assert_eq!(
                    interpret(&bytes, &initial, optimize),
                    expected,
                    "O{} {bytes:02X?}",
                    if optimize { 2 } else { 0 }
                );
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_interpreter_for_extensions_aliases_lane_equations_and_full_state() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX VMOVHLPS/VMOVLHPS differential: host lacks AVX-512F/BW");
        return;
    }

    let initial = initial_state();
    let operands = [
        (1, 2, 3),
        (9, 10, 11),
        (17, 18, 19),
        (25, 26, 27),
        (31, 31, 31),
        (1, 1, 3),
        (2, 3, 2),
        (4, 4, 4),
        (1, 17, 30),
        (31, 0, 16),
    ];
    let mut executed = 0usize;
    for opcode in [0x12, 0x16] {
        for (destination, merge, source) in operands {
            let bytes = encoding(opcode, destination, merge, source);
            for optimize in [false, true] {
                assert_eq!(
                    execute_native(&bytes, &initial, optimize),
                    interpret(&bytes, &initial, optimize),
                    "O{} {bytes:02X?}",
                    if optimize { 2 } else { 0 }
                );
                executed += 1;
            }
        }
    }
    assert_eq!(executed, 40);
}
