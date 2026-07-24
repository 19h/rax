//! Native replay coverage for register-only VCOMISH and VUCOMISH.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x2F00;
const STATUS_FLAGS: u64 = (1 << 0) | (1 << 2) | (1 << 4) | (1 << 6) | (1 << 7) | (1 << 11);

fn encoding(opcode: u8, src1: u8, src2: u8, ll: u8, suppress_exceptions: bool) -> [u8; 6] {
    assert!(matches!(opcode, 0x2E | 0x2F));
    assert!(src1 < 32 && src2 < 32 && ll < 4);
    let mut p0 = 0xF5;
    if src1 & 0x08 != 0 {
        p0 &= !0x80;
    }
    if src1 & 0x10 != 0 {
        p0 &= !0x10;
    }
    if src2 & 0x08 != 0 {
        p0 &= !0x20;
    }
    if src2 & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7C,
        (ll << 5) | if suppress_exceptions { 0x10 } else { 0 } | 0x08,
        opcode,
        0xC0 | ((src1 & 7) << 3) | (src2 & 7),
    ]
}

fn function(bytes: &[u8; 6]) -> crate::smir::ir::SmirFunction {
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
fn replay_admits_and_emits_all_control_and_register_extension_samples() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let registers = [(0, 1), (1, 9), (9, 10), (17, 18), (25, 26), (30, 31)];
    let mut admitted = 0usize;
    let mut checked_fail_closed = false;
    for opcode in [0x2E, 0x2F] {
        for ll in 0..4 {
            for suppress_exceptions in [false, true] {
                for (src1, src2) in registers {
                    let bytes = encoding(opcode, src1, src2, ll, suppress_exceptions);
                    let mut function = function(&bytes);

                    if !checked_fail_closed {
                        let mut missing = function.clone();
                        missing.x86_instruction_bytes.clear();
                        crate::smir::optimize::optimize_function(
                            &mut missing,
                            crate::smir::optimize::OptLevel::O2,
                        );
                        assert!(!is_native_clobber_safe(&missing));

                        let mut memory = bytes;
                        memory[5] &= 0x3F;
                        let mut malformed = function.clone();
                        malformed.x86_instruction_bytes.insert(
                            (BlockId(0), PC),
                            crate::smir::ir::X86InstructionBytes::new(&memory).unwrap(),
                        );
                        assert!(!is_native_clobber_safe(&malformed));
                        checked_fail_closed = true;
                    }

                    crate::smir::optimize::optimize_function(
                        &mut function,
                        crate::smir::optimize::OptLevel::O2,
                    );
                    assert!(is_native_clobber_safe(&function), "{bytes:02X?}");
                    assert!(
                        uses_x86_native_vectors_excluding(
                            &function,
                            &std::collections::HashMap::new()
                        ),
                        "{bytes:02X?}"
                    );
                    let requirements = x86_native_replay_feature_requirements(
                        &function,
                        &std::collections::HashMap::new(),
                    );
                    assert!(requirements.any, "{bytes:02X?}");
                    assert!(requirements.needs_avx512bw, "{bytes:02X?}");
                    assert!(requirements.needs_avx512fp16, "{bytes:02X?}");
                    assert!(!requirements.needs_avx512vl, "{bytes:02X?}");
                    assert!(!requirements.needs_avx512dq, "{bytes:02X?}");

                    #[cfg(target_arch = "x86_64")]
                    let expected_features = std::is_x86_feature_detected!("avx512f")
                        && std::is_x86_feature_detected!("avx512bw")
                        && std::is_x86_feature_detected!("avx512fp16");
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
                    admitted += 1;
                }
            }
        }
    }
    assert!(checked_fail_closed);
    assert_eq!(admitted, 96);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct CompareState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn initial_state(src1: usize, first: u16, src2: usize, second: u16, mxcsr: u32) -> CompareState {
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 9 + word * 5) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        })
    });
    vectors[src1][0] = (vectors[src1][0] & !0xFFFF) | u64::from(first);
    vectors[src2][0] = (vectors[src2][0] & !0xFFFF) | u64::from(second);
    CompareState {
        gprs: std::array::from_fn(|register| {
            0xA55A_6996_F00F_3CC3u64.rotate_left((register * 7) as u32)
        }),
        vectors,
        masks: [
            0x6996_F00F_3CC3_A55A,
            0xA55A_3CC3_F00F_9696,
            0,
            u64::MAX,
            0x0123_4567_89AB_CDEF,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0001,
            0xF0F0_0F0F_A5A5_5A5A,
        ],
        rflags: 0x2 | 0x8D5,
        mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn optimized_function(
    bytes: &[u8; 6],
    level: crate::smir::optimize::OptLevel,
    halt: bool,
) -> crate::smir::ir::SmirFunction {
    let mut function = function(bytes);
    if halt {
        function.blocks[0].set_terminator(Terminator::Trap {
            kind: crate::smir::ir::TrapKind::Halt,
        });
    }
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

#[cfg(target_arch = "x86_64")]
fn interpret(
    bytes: &[u8; 6],
    initial: &CompareState,
    level: crate::smir::optimize::OptLevel,
) -> CompareState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let function = optimized_function(bytes, level, true);
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
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    let mut memory = FlatMemory::new(1);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    context.flags.materialize_all();

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut vectors = [[0u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    CompareState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: (initial.rflags & !STATUS_FLAGS)
            | (context.flags.materialized.to_rflags() & STATUS_FLAGS),
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8; 6],
    initial: &CompareState,
    level: crate::smir::optimize::OptLevel,
) -> CompareState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = optimized_function(bytes, level, false);
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map FP16 flag-compare replay");
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
    CompareState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_o0_o2_interpretation_for_flags_nan_daz_sae_aliases_and_full_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512fp16")
    {
        eprintln!("skipping native FP16 flag-compare differential: host lacks AVX-512-FP16");
        return;
    }

    let values = [
        (0x3C00, 0x3C00, false), // equal
        (0x3C00, 0x4000, false), // less
        (0x4000, 0x3C00, false), // greater
        (0x0000, 0x8000, false), // signed-zero equality
        (0x7E01, 0x3C00, false), // QNaN
        (0x7C01, 0x3C00, false), // SNaN
        (0x0001, 0x0000, false), // positive denormal
        (0x8001, 0x8000, false), // negative denormal
        (0x7C00, 0x7C00, false), // infinity equality
        (0x3C00, 0x3C00, true),  // register alias
    ];
    let register_pairs = [(1, 2), (9, 10), (17, 18), (25, 26), (30, 31), (31, 31)];
    let mut executed = 0usize;
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for opcode in [0x2E, 0x2F] {
            for ll in 0..4 {
                for suppress_exceptions in [false, true] {
                    for (case, (first, second, force_alias)) in values.into_iter().enumerate() {
                        let (src1, mut src2) =
                            register_pairs[(case + usize::from(opcode == 0x2F) + usize::from(ll))
                                % register_pairs.len()];
                        if force_alias {
                            src2 = src1;
                        }
                        let bytes = encoding(opcode, src1, src2, ll, suppress_exceptions);
                        let prior_status = if case % 3 == 0 { 1 << 5 } else { 0 };
                        let rc = ((case as u32) & 3) << 13;
                        let daz_ftz = if case & 1 == 0 {
                            0
                        } else {
                            (1 << 6) | (1 << 15)
                        };
                        let initial = initial_state(
                            usize::from(src1),
                            first,
                            usize::from(src2),
                            second,
                            0x1F80 | prior_status | rc | daz_ftz,
                        );
                        let interpreted = interpret(&bytes, &initial, level);
                        let native = execute_native(&bytes, &initial, level);
                        assert_eq!(
                            native, interpreted,
                            "level={level:?} opcode={opcode:#04x} ll={ll} sae={suppress_exceptions} bytes={bytes:02X?}"
                        );
                        executed += 1;
                    }
                }
            }
        }
    }
    assert_eq!(executed, 320);
}
