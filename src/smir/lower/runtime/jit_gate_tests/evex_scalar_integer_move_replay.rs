//! Native replay coverage for register-only EVEX scalar-integer moves.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x1012;

fn vmovq_encoding(opcode: u8, destination: u8, source: u8) -> [u8; 6] {
    assert!(matches!(opcode, 0x7E | 0xD6));
    assert!(destination < 32 && source < 32);
    let (reg, rm, pp) = if opcode == 0x7E {
        (destination, source, 2)
    } else {
        (source, destination, 1)
    };
    let mut p0 = 0xF1;
    if reg & 0x08 != 0 {
        p0 &= !0x80;
    }
    if reg & 0x10 != 0 {
        p0 &= !0x10;
    }
    if rm & 0x08 != 0 {
        p0 &= !0x20;
    }
    if rm & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0xFC | pp,
        0x08,
        opcode,
        0xC0 | ((reg & 0x07) << 3) | (rm & 0x07),
    ]
}

fn vmovw_encoding(opcode: u8, w: bool, xmm: u8, gpr: u8) -> [u8; 6] {
    assert!(matches!(opcode, 0x6E | 0x7E));
    assert!(xmm < 32 && gpr < 16);
    let mut p0 = 0xF5;
    if xmm & 0x08 != 0 {
        p0 &= !0x80;
    }
    if xmm & 0x10 != 0 {
        p0 &= !0x10;
    }
    if gpr & 0x08 != 0 {
        p0 &= !0x20;
    }
    [
        0x62,
        p0,
        0x7D | if w { 0x80 } else { 0 },
        0x08,
        opcode,
        0xC0 | ((xmm & 0x07) << 3) | (gpr & 0x07),
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
fn replay_feature_aggregation_distinguishes_vmovq_from_vmovw() {
    for (bytes, needs_fp16) in [
        (vmovq_encoding(0x7E, 31, 30), false),
        (vmovq_encoding(0xD6, 31, 30), false),
        (vmovw_encoding(0x6E, false, 31, 15), true),
        (vmovw_encoding(0x7E, true, 31, 15), true),
    ] {
        let function = function(&bytes);
        let actual =
            x86_native_replay_feature_requirements(&function, &std::collections::HashMap::new());
        assert!(actual.any, "{bytes:02X?}");
        assert!(actual.needs_avx512bw, "{bytes:02X?}");
        assert!(!actual.needs_avx512vl, "{bytes:02X?}");
        assert!(!actual.needs_avx512dq, "{bytes:02X?}");
        assert_eq!(actual.needs_avx512fp16, needs_fp16, "{bytes:02X?}");
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
fn replay_admits_and_emits_60_o0_o2_safe_encodings_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut cases = Vec::new();
    for opcode in [0x7E, 0xD6] {
        for (destination, source) in [
            (1, 2),
            (9, 10),
            (17, 18),
            (25, 26),
            (31, 31),
            (1, 17),
            (18, 2),
        ] {
            cases.push((vmovq_encoding(opcode, destination, source), false));
        }
    }
    for opcode in [0x6E, 0x7E] {
        for w in [false, true] {
            for (xmm, gpr) in [(1, 0), (9, 3), (17, 8), (25, 12)] {
                cases.push((vmovw_encoding(opcode, w, xmm, gpr), true));
            }
        }
    }
    assert_eq!(cases.len(), 30);

    let mut lowered = 0usize;
    for (bytes, needs_fp16) in cases {
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
                && std::is_x86_feature_detected!("avx512bw")
                && (!needs_fp16 || std::is_x86_feature_detected!("avx512fp16"));
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
    assert_eq!(lowered, 60);

    for bytes in [
        vmovq_encoding(0x7E, 1, 2),
        vmovw_encoding(0x6E, false, 1, 0),
    ] {
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

    for gpr in [4, 5] {
        for opcode in [0x6E, 0x7E] {
            let bytes = vmovw_encoding(opcode, false, 1, gpr);
            let unsafe_function = function(&bytes);
            assert!(!is_native_clobber_safe(&unsafe_function), "{bytes:02X?}");
        }
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
fn interpret(bytes: &[u8], initial: &MoveState) -> MoveState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::TrapKind;
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::memory::FlatMemory;

    let mut function = function(bytes);
    function.blocks[0].set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
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
fn execute_native(bytes: &[u8], initial: &MoveState) -> MoveState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = function(bytes);
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map EVEX scalar-integer-move replay");
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
fn replay_matches_interpreter_for_vmovq_directions_extensions_aliases_and_full_state() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX VMOVQ differential: host lacks AVX-512F/BW");
        return;
    }
    let initial = initial_state();
    let operands = [
        (1, 2),
        (9, 10),
        (17, 18),
        (25, 26),
        (31, 31),
        (1, 17),
        (18, 2),
    ];
    let mut executed = 0usize;
    for opcode in [0x7E, 0xD6] {
        for (destination, source) in operands {
            let bytes = vmovq_encoding(opcode, destination, source);
            assert_eq!(
                execute_native(&bytes, &initial),
                interpret(&bytes, &initial),
                "{bytes:02X?}"
            );
            executed += 1;
        }
    }
    assert_eq!(executed, 14);
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_interpreter_for_vmovw_directions_wig_extensions_and_full_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512fp16")
    {
        eprintln!("skipping native EVEX VMOVW differential: host lacks AVX-512F/BW/FP16");
        return;
    }
    let initial = initial_state();
    let operands = [(1, 0), (9, 3), (17, 8), (25, 12), (31, 15)];
    let mut executed = 0usize;
    for opcode in [0x6E, 0x7E] {
        for w in [false, true] {
            for (xmm, gpr) in operands {
                let bytes = vmovw_encoding(opcode, w, xmm, gpr);
                assert_eq!(
                    execute_native(&bytes, &initial),
                    interpret(&bytes, &initial),
                    "{bytes:02X?}"
                );
                executed += 1;
            }
        }
    }
    assert_eq!(executed, 20);
}
