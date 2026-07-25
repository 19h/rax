//! Native replay coverage for register-only VEX GFNI instructions.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x6F00;
const OPERANDS: [(u8, u8, u8); 10] = [
    (1, 2, 3),
    (9, 10, 11),
    (1, 1, 3),
    (1, 2, 1),
    (1, 2, 2),
    (9, 9, 11),
    (9, 10, 9),
    (15, 15, 15),
    (15, 8, 13),
    (13, 14, 15),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GfniKind {
    Multiply,
    Affine,
    AffineInverse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct GfniCase {
    kind: GfniKind,
    ymm: bool,
    destination: u8,
    source1: u8,
    source2: u8,
    immediate: u8,
    clear_ignored_x: bool,
}

fn encoding(case: GfniCase) -> Vec<u8> {
    assert!(case.destination < 16 && case.source1 < 16 && case.source2 < 16);
    let (map, w, opcode, has_immediate) = match case.kind {
        GfniKind::Multiply => (2, false, 0xCF, false),
        GfniKind::Affine => (3, true, 0xCE, true),
        GfniKind::AffineInverse => (3, true, 0xCF, true),
    };
    let mut p0 = 0xE0 | map;
    if case.destination >= 8 {
        p0 &= !0x80;
    }
    if case.clear_ignored_x {
        p0 &= !0x40;
    }
    if case.source2 >= 8 {
        p0 &= !0x20;
    }
    let mut bytes = vec![
        0xC4,
        p0,
        (u8::from(w) << 7) | (((!case.source1) & 0x0F) << 3) | (u8::from(case.ymm) << 2) | 1,
        opcode,
        0xC0 | ((case.destination & 7) << 3) | (case.source2 & 7),
    ];
    if has_immediate {
        bytes.push(case.immediate);
    }
    bytes
}

fn exhaustive_cases() -> Vec<GfniCase> {
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for seed in u8::MIN..=u8::MAX {
        for ymm in [false, true] {
            let (destination, source1, source2) = OPERANDS[ordinal % OPERANDS.len()];
            cases.push(GfniCase {
                kind: GfniKind::Multiply,
                ymm,
                destination,
                source1,
                source2,
                immediate: seed,
                clear_ignored_x: ordinal & 1 != 0,
            });
            ordinal += 1;
        }
    }
    for kind in [GfniKind::Affine, GfniKind::AffineInverse] {
        for immediate in u8::MIN..=u8::MAX {
            for ymm in [false, true] {
                let (destination, source1, source2) = OPERANDS[ordinal % OPERANDS.len()];
                cases.push(GfniCase {
                    kind,
                    ymm,
                    destination,
                    source1,
                    source2,
                    immediate,
                    clear_ignored_x: ordinal & 1 != 0,
                });
                ordinal += 1;
            }
        }
    }
    cases
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

fn representative_case(kind: GfniKind, ymm: bool) -> GfniCase {
    GfniCase {
        kind,
        ymm,
        destination: 13,
        source1: 14,
        source2: 15,
        immediate: 0xA5,
        clear_ignored_x: true,
    }
}

#[test]
fn replay_features_require_exactly_avx_gfni_and_ymm16_state() {
    for kind in [
        GfniKind::Multiply,
        GfniKind::Affine,
        GfniKind::AffineInverse,
    ] {
        for ymm in [false, true] {
            let case = representative_case(kind, ymm);
            let bytes = encoding(case);
            let function = function(&bytes);
            let excluded = std::collections::HashMap::new();
            let requirements = x86_native_replay_feature_requirements(&function, &excluded);
            assert!(requirements.any, "{case:?}");
            assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
            assert!(requirements.needs_avx, "{case:?}");
            assert!(requirements.needs_gfni, "{case:?}");
            assert!(!requirements.needs_sse3, "{case:?}");
            assert!(!requirements.needs_avx2, "{case:?}");
            assert!(!requirements.needs_fma, "{case:?}");
            assert!(!requirements.needs_fma4, "{case:?}");
            assert!(!requirements.needs_avx512bw, "{case:?}");
            assert!(!requirements.needs_avx512vl, "{case:?}");
            assert!(!requirements.needs_avx512dq, "{case:?}");
            assert!(!requirements.needs_avx512fp16, "{case:?}");
            assert!(!requirements.needs_avx512cd, "{case:?}");
            assert!(!requirements.needs_avx512vp2intersect, "{case:?}");
            assert!(!requirements.needs_pclmulqdq, "{case:?}");
            assert!(!requirements.needs_vpclmulqdq, "{case:?}");
            assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
                &function, &excluded
            ));

            #[cfg(target_arch = "x86_64")]
            let expected_features =
                std::is_x86_feature_detected!("avx") && std::is_x86_feature_detected!("gfni");
            #[cfg(not(target_arch = "x86_64"))]
            let expected_features = false;
            assert_eq!(
                x86_native_vector_features_supported_excluding(&function, &excluded),
                expected_features,
                "{case:?}"
            );

            let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
            assert_eq!(
                x86_native_replay_feature_requirements(&function, &excluded),
                X86NativeReplayFeatureRequirements::default(),
                "{case:?}"
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_host_gate_conjoins_avx_and_gfni() {
    let host_avx = std::is_x86_feature_detected!("avx");
    let host_gfni = std::is_x86_feature_detected!("gfni");
    for needs_avx in [false, true] {
        for needs_gfni in [false, true] {
            let requirements = X86NativeReplayFeatureRequirements {
                needs_avx,
                needs_gfni,
                ..X86NativeReplayFeatureRequirements::default()
            };
            assert_eq!(
                requirements.x86_host_supported(),
                (!needs_avx || host_avx) && (!needs_gfni || host_gfni)
            );
        }
    }
}

#[test]
fn replay_admits_and_emits_238_o0_o2_shapes_and_rejects_unsafe_provenance() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    assert_eq!(
        encoding(GfniCase {
            kind: GfniKind::Multiply,
            ymm: false,
            destination: 9,
            source1: 10,
            source2: 11,
            immediate: 0,
            clear_ignored_x: false,
        }),
        [0xC4, 0x42, 0x29, 0xCF, 0xCB]
    );
    assert_eq!(
        encoding(GfniCase {
            kind: GfniKind::Multiply,
            ymm: true,
            destination: 13,
            source1: 14,
            source2: 15,
            immediate: 0,
            clear_ignored_x: false,
        }),
        [0xC4, 0x42, 0x0D, 0xCF, 0xEF]
    );
    assert_eq!(
        encoding(GfniCase {
            kind: GfniKind::Affine,
            ymm: false,
            destination: 9,
            source1: 10,
            source2: 11,
            immediate: 0x63,
            clear_ignored_x: false,
        }),
        [0xC4, 0x43, 0xA9, 0xCE, 0xCB, 0x63]
    );
    assert_eq!(
        encoding(GfniCase {
            kind: GfniKind::AffineInverse,
            ymm: true,
            destination: 13,
            source1: 14,
            source2: 15,
            immediate: 0xA5,
            clear_ignored_x: false,
        }),
        [0xC4, 0x43, 0x8D, 0xCF, 0xEF, 0xA5]
    );

    let cases: Vec<_> = exhaustive_cases().into_iter().step_by(13).collect();
    assert_eq!(cases.len(), 119);
    let mut lowered = 0usize;
    for case in cases {
        let bytes = encoding(case);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            let mut function = function(&bytes);
            crate::smir::optimize::optimize_function(&mut function, level);
            assert!(
                is_native_clobber_safe(&function),
                "{level:?} {case:?} {bytes:02X?}"
            );
            assert!(
                uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
                "{level:?} {case:?} {bytes:02X?}"
            );
            let mut lowerer = X86_64Lowerer::new();
            lowerer
                .lower_function(&function)
                .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            assert!(
                code.windows(bytes.len()).any(|window| window == bytes),
                "{level:?} {case:?} {bytes:02X?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 238);

    let bytes = encoding(representative_case(GfniKind::AffineInverse, true));
    let mut missing = function(&bytes);
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing));

    let mut memory_bytes = bytes.clone();
    memory_bytes[4] &= 0x3F;
    let mut memory_metadata = function(&bytes);
    memory_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&memory_bytes).unwrap(),
    );
    assert!(!is_native_clobber_safe(&memory_metadata));
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GfniState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

fn initial_state(ordinal: usize) -> GfniState {
    GfniState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0xA55A_6996_F00F_3CC3u64
                    .rotate_left(((ordinal * 3 + register * 11 + word * 17) & 63) as u32)
                    ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
                    ^ (register as u64).wrapping_mul(0x1111_1111_1111_1111)
                    ^ (word as u64).wrapping_mul(0x0123_4567_89AB_CDEF)
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
        rflags: 0x2 | 0x0CD5,
        mxcsr: [
            0x1F80,
            0x1F80 | 0x15,
            0x1F80 | (1 << 13) | (1 << 6),
            0x1F80 | (3 << 13) | (1 << 15),
        ][ordinal % 4],
    }
}

fn gf_mul_reference(a: u8, b: u8) -> u8 {
    let mut product = 0u16;
    for bit in 0..8 {
        if b & (1 << bit) != 0 {
            product ^= u16::from(a) << bit;
        }
    }
    for degree in (8..=14).rev() {
        if product & (1 << degree) != 0 {
            product ^= 0x11B << (degree - 8);
        }
    }
    product as u8
}

fn gf_inverse_reference(value: u8) -> u8 {
    let mut result = 1u8;
    let mut power = value;
    let mut exponent = 254u8;
    while exponent != 0 {
        if exponent & 1 != 0 {
            result = gf_mul_reference(result, power);
        }
        power = gf_mul_reference(power, power);
        exponent >>= 1;
    }
    result
}

fn vector_byte(vector: &[u64; 8], index: usize) -> u8 {
    (vector[index / 8] >> ((index % 8) * 8)) as u8
}

fn architectural_expected(case: GfniCase, initial: &GfniState) -> GfniState {
    let mut result = [0u64; 8];
    let bytes = if case.ymm { 32 } else { 16 };
    for lane in 0..bytes {
        let input = vector_byte(&initial.vectors[usize::from(case.source1)], lane);
        let output = match case.kind {
            GfniKind::Multiply => gf_mul_reference(
                input,
                vector_byte(&initial.vectors[usize::from(case.source2)], lane),
            ),
            GfniKind::Affine | GfniKind::AffineInverse => {
                let input = if case.kind == GfniKind::AffineInverse {
                    gf_inverse_reference(input)
                } else {
                    input
                };
                let qword_base = lane & !7;
                let mut output = 0u8;
                for bit in 0..8 {
                    let matrix_row = vector_byte(
                        &initial.vectors[usize::from(case.source2)],
                        qword_base + 7 - bit,
                    );
                    let parity = (matrix_row & input).count_ones() as u8 & 1;
                    output |= (parity ^ ((case.immediate >> bit) & 1)) << bit;
                }
                output
            }
        };
        result[lane / 8] |= u64::from(output) << ((lane % 8) * 8);
    }

    let mut expected = initial.clone();
    expected.vectors[usize::from(case.destination)] = result;
    expected
}

fn optimized_function(
    bytes: &[u8],
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

fn interpret(
    bytes: &[u8],
    initial: &GfniState,
    level: crate::smir::optimize::OptLevel,
) -> GfniState {
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
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &function.blocks[0],
    );
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut vectors = [[0u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    GfniState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_independent_o0_o2_field_equations_and_full_state() {
    let cases = exhaustive_cases();
    assert_eq!(cases.len(), 1_536);
    let mut seen_multiply_inputs = [false; 256];
    let mut seen_multiply_multipliers = [false; 256];
    let mut seen_affine_inputs = [false; 256];
    let mut seen_inverse_inputs = [false; 256];
    let mut seen_matrix_bytes = [false; 256];
    for (ordinal, case) in cases.into_iter().enumerate() {
        let bytes = encoding(case);
        let initial = initial_state(ordinal);
        let expected = architectural_expected(case, &initial);
        let lanes = if case.ymm { 32 } else { 16 };
        for lane in 0..lanes {
            let input = vector_byte(&initial.vectors[usize::from(case.source1)], lane);
            let source2 = vector_byte(&initial.vectors[usize::from(case.source2)], lane);
            match case.kind {
                GfniKind::Multiply => {
                    seen_multiply_inputs[usize::from(input)] = true;
                    seen_multiply_multipliers[usize::from(source2)] = true;
                }
                GfniKind::Affine => {
                    seen_affine_inputs[usize::from(input)] = true;
                    seen_matrix_bytes[usize::from(source2)] = true;
                }
                GfniKind::AffineInverse => {
                    seen_inverse_inputs[usize::from(input)] = true;
                    seen_matrix_bytes[usize::from(source2)] = true;
                }
            }
        }
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            assert_eq!(
                interpret(&bytes, &initial, level),
                expected,
                "{level:?} {case:?} {bytes:02X?}"
            );
        }
    }
    for (name, seen) in [
        ("multiply input", seen_multiply_inputs),
        ("multiply multiplier", seen_multiply_multipliers),
        ("affine input", seen_affine_inputs),
        ("inverse input", seen_inverse_inputs),
        ("matrix byte", seen_matrix_bytes),
    ] {
        assert!(
            seen.into_iter().all(std::convert::identity),
            "case corpus omitted at least one {name} byte value"
        );
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8],
    initial: &GfniState,
    level: crate::smir::optimize::OptLevel,
) -> GfniState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = optimized_function(bytes, level, false);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(true);
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map VEX GFNI replay");
    let mut registers = GuestRegs {
        gpr: initial.gprs,
        rflags: initial.rflags,
        vector_active: X86_VECTOR_STATE_YMM16,
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
    GfniState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_GFNI_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
fn child_range() -> Option<std::ops::Range<usize>> {
    let value = std::env::var(CHILD_RANGE_ENV).ok()?;
    let (start, end) = value
        .split_once(':')
        .unwrap_or_else(|| panic!("invalid {CHILD_RANGE_ENV}: {value}"));
    Some(
        start
            .parse::<usize>()
            .unwrap_or_else(|_| panic!("invalid {CHILD_RANGE_ENV} start: {value}"))
            ..end
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("invalid {CHILD_RANGE_ENV} end: {value}")),
    )
}

#[cfg(target_arch = "x86_64")]
fn execute_native_case_range(cases: &[GfniCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let bytes = encoding(case);
        let initial = initial_state(ordinal);
        let expected = architectural_expected(case, &initial);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            assert_eq!(
                interpret(&bytes, &initial, level),
                expected,
                "interpreter {level:?} {case:?} {bytes:02X?}"
            );
            assert_eq!(
                execute_native(&bytes, &initial, level),
                expected,
                "native {level:?} {case:?} {bytes:02X?}"
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn run_child_range(test_name: &str, range: std::ops::Range<usize>) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("current unit-test executable"))
        .arg(test_name)
        .arg("--exact")
        .arg("--nocapture")
        .env(CHILD_RANGE_ENV, format!("{}:{}", range.start, range.end))
        .output()
        .expect("run isolated native VEX GFNI differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    if !std::is_x86_feature_detected!("avx") || !std::is_x86_feature_detected!("gfni") {
        eprintln!("skipping native VEX GFNI differential: host lacks AVX or GFNI");
        return;
    }
    let cases = exhaustive_cases();
    assert_eq!(cases.len(), 1_536);
    eprintln!("executing {} native VEX GFNI cases", cases.len());
    if let Some(range) = child_range() {
        execute_native_case_range(&cases, range);
        return;
    }

    let whole = run_child_range(test_name, 0..cases.len());
    if whole.status.success() {
        return;
    }
    let mut start = 0usize;
    let mut end = cases.len();
    while end - start > 1 {
        let middle = start + (end - start) / 2;
        if run_child_range(test_name, start..middle).status.success() {
            start = middle;
        } else {
            end = middle;
        }
    }
    let singleton = run_child_range(test_name, start..end);
    let case = cases[start];
    let bytes = encoding(case);
    panic!(
        "isolated native VEX GFNI failure at case {start}/{}: \
         {case:?} {bytes:02X?}; whole status {}; singleton status {}; \
         singleton stdout: {}; singleton stderr: {}",
        cases.len(),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_independent_o0_o2_field_equations_aliases_and_full_state() {
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_gfni_replay::\
         replay_matches_independent_o0_o2_field_equations_aliases_and_full_state",
    );
}
