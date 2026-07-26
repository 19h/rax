//! Native replay coverage for register-only AMD XOP VPERMIL2 instructions.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x5E1E_C702;
const OPERANDS: [(u8, u8, u8, u8); 9] = [
    (1, 2, 3, 4),
    (9, 10, 11, 12),
    (1, 1, 2, 3),
    (1, 2, 1, 3),
    (1, 2, 3, 1),
    (1, 2, 2, 3),
    (1, 2, 3, 2),
    (1, 2, 3, 3),
    (1, 1, 1, 1),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Vpermil2Case {
    opcode: u8,
    w: bool,
    l: bool,
    dst: u8,
    src1: u8,
    rm: u8,
    is4: u8,
    ignored_low: u8,
    clear_ignored_x: bool,
}

impl Vpermil2Case {
    fn element_bits(self) -> usize {
        if self.opcode == 0x48 { 32 } else { 64 }
    }

    fn width_bytes(self) -> usize {
        if self.l { 32 } else { 16 }
    }
}

fn encoding(case: Vpermil2Case) -> [u8; 6] {
    assert!(matches!(case.opcode, 0x48 | 0x49));
    assert!(
        case.dst < 16 && case.src1 < 16 && case.rm < 16 && case.is4 < 16 && case.ignored_low < 16
    );
    let mut p0 = 0xE3;
    if case.dst >= 8 {
        p0 &= !0x80;
    }
    if case.clear_ignored_x {
        p0 &= !0x40;
    }
    if case.rm >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        (u8::from(case.w) << 7) | ((!case.src1 & 0x0F) << 3) | (u8::from(case.l) << 2) | 1,
        case.opcode,
        0xC0 | ((case.dst & 7) << 3) | (case.rm & 7),
        (case.is4 << 4) | case.ignored_low,
    ]
}

fn cases() -> Vec<Vpermil2Case> {
    let mut result = Vec::new();
    let mut ordinal = 0_usize;
    for opcode in [0x48, 0x49] {
        for w in [false, true] {
            for l in [false, true] {
                for (dst, src1, rm, is4) in OPERANDS {
                    result.push(Vpermil2Case {
                        opcode,
                        w,
                        l,
                        dst,
                        src1,
                        rm,
                        is4,
                        ignored_low: ordinal as u8 & 0x0F,
                        clear_ignored_x: ordinal & 1 != 0,
                    });
                    ordinal += 1;
                }
            }
        }
    }
    result
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
fn replay_features_require_avx_xop_and_select_the_ymm16_state_boundary() {
    let case = Vpermil2Case {
        opcode: 0x49,
        w: true,
        l: true,
        dst: 15,
        src1: 14,
        rm: 13,
        is4: 12,
        ignored_low: 15,
        clear_ignored_x: true,
    };
    let function = function(&encoding(case));
    let excluded = std::collections::HashMap::new();
    let requirements = x86_native_replay_feature_requirements(&function, &excluded);
    assert!(requirements.any);
    assert!(requirements.all_spans_support_avx_ymm16);
    assert!(requirements.needs_avx);
    assert!(requirements.needs_xop);
    assert!(!requirements.needs_sse3);
    assert!(!requirements.needs_avx2);
    assert!(!requirements.needs_fma);
    assert!(!requirements.needs_fma4);
    assert!(!requirements.needs_avx512bw);
    assert!(!requirements.needs_avx512vl);
    assert!(!requirements.needs_avx512dq);
    assert!(!requirements.needs_avx512fp16);
    assert!(!requirements.needs_avx512cd);
    assert!(!requirements.needs_gfni);
    assert!(!requirements.needs_avx512vp2intersect);
    assert!(!requirements.needs_pclmulqdq);
    assert!(!requirements.needs_vpclmulqdq);
    assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
        &function, &excluded
    ));

    #[cfg(target_arch = "x86_64")]
    {
        let expected = std::is_x86_feature_detected!("avx") && x86_host_has_xop();
        assert_eq!(requirements.x86_host_supported(), expected);
        assert_eq!(
            x86_native_vector_features_supported_excluding(&function, &excluded),
            expected
        );
    }

    let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
    assert_eq!(
        x86_native_replay_feature_requirements(&function, &excluded),
        X86NativeReplayFeatureRequirements::default()
    );
}

#[test]
fn replay_admits_and_emits_144_o0_o2_family_role_alias_and_extension_shapes() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let cases = cases();
    assert_eq!(cases.len(), 72);
    let mut lowered = 0_usize;
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
    assert_eq!(lowered, 144);

    let case = Vpermil2Case {
        opcode: 0x48,
        w: false,
        l: false,
        dst: 1,
        src1: 2,
        rm: 3,
        is4: 4,
        ignored_low: 0,
        clear_ignored_x: false,
    };
    let bytes = encoding(case);
    let mut missing = function(&bytes);
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing));

    let mut memory_bytes = bytes;
    memory_bytes[4] &= 0x3F;
    let mut memory_metadata = function(&bytes);
    memory_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&memory_bytes).unwrap(),
    );
    assert!(!is_native_clobber_safe(&memory_metadata));

    let mut wrong_opcode = bytes;
    wrong_opcode[3] = 0x47;
    let mut wrong_metadata = function(&bytes);
    wrong_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&wrong_opcode).unwrap(),
    );
    assert!(!is_native_clobber_safe(&wrong_metadata));
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Vpermil2State {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

fn element_bits(vector: &[u64; 8], width: usize, lane: usize) -> u64 {
    match width {
        32 => (vector[lane / 2] >> ((lane & 1) * 32)) & u64::from(u32::MAX),
        64 => vector[lane],
        _ => unreachable!(),
    }
}

fn set_element_bits(vector: &mut [u64; 8], width: usize, lane: usize, value: u64) {
    match width {
        32 => {
            let shift = (lane & 1) * 32;
            let mask = u64::from(u32::MAX) << shift;
            vector[lane / 2] =
                (vector[lane / 2] & !mask) | ((value & u64::from(u32::MAX)) << shift);
        }
        64 => vector[lane] = value,
        _ => unreachable!(),
    }
}

fn initial_state(case: Vpermil2Case, ordinal: usize) -> Vpermil2State {
    let mut state = Vpermil2State {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEF_u64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0xA55A_6996_F00F_3CC3_u64
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
    };

    let selector_register = if case.w { case.rm } else { case.is4 };
    let selector = &mut state.vectors[usize::from(selector_register)];
    let element_width = case.element_bits();
    let lanes = case.width_bytes() * 8 / element_width;
    let block_lanes = 128 / element_width;
    for lane in 0..lanes {
        let selected = (lane + ordinal) % (2 * block_lanes);
        let m = (lane + ordinal) & 1;
        let selector_bits = if element_width == 32 {
            selected
        } else {
            selected << 1
        };
        let ignored_bit0 = usize::from(element_width == 64) & (lane ^ ordinal) & 1;
        let noise = 0xA5A5_0000_5A5A_0000_u64.rotate_left((lane * 9) as u32);
        set_element_bits(
            selector,
            element_width,
            lane,
            noise | (m as u64 * 8) | selector_bits as u64 | ignored_bit0 as u64,
        );
    }
    state
}

fn architectural_expected(case: Vpermil2Case, initial: &Vpermil2State) -> Vpermil2State {
    let src1 = initial.vectors[usize::from(case.src1)];
    let rm = initial.vectors[usize::from(case.rm)];
    let is4 = initial.vectors[usize::from(case.is4)];
    let (src2, selector) = if case.w { (is4, rm) } else { (rm, is4) };
    let element_width = case.element_bits();
    let lanes = case.width_bytes() * 8 / element_width;
    let block_lanes = 128 / element_width;
    let m2z = case.ignored_low & 3;
    let mut expected = initial.clone();
    let destination = &mut expected.vectors[usize::from(case.dst)];
    destination.fill(0);
    for lane in 0..lanes {
        let control = element_bits(&selector, element_width, lane);
        let selected = if element_width == 32 {
            control as usize & 7
        } else {
            (control as usize >> 1) & 3
        };
        let block = lane / block_lanes * block_lanes;
        let value = if selected < block_lanes {
            element_bits(&src1, element_width, block + selected)
        } else {
            element_bits(&src2, element_width, block + selected - block_lanes)
        };
        let m = control & 8 != 0;
        let zero = match m2z {
            0 | 1 => false,
            2 => m,
            3 => !m,
            _ => unreachable!(),
        };
        set_element_bits(
            destination,
            element_width,
            lane,
            if zero { 0 } else { value },
        );
    }
    expected
}

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

fn interpret(
    bytes: &[u8; 6],
    initial: &Vpermil2State,
    level: crate::smir::optimize::OptLevel,
) -> Vpermil2State {
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
    let mut vectors = [[0_u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    Vpermil2State {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_amd_o0_o2_equations_roles_aliases_m2z_and_upper_zeroing() {
    let cases = cases();
    assert_eq!(cases.len(), 72);
    for (ordinal, case) in cases.into_iter().enumerate() {
        let bytes = encoding(case);
        let initial = initial_state(case, ordinal);
        let expected = architectural_expected(case, &initial);
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
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8; 6],
    initial: &Vpermil2State,
    level: crate::smir::optimize::OptLevel,
) -> Vpermil2State {
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
    let exec = ExecMem::new(&code).expect("map VPERMIL2 replay");
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

    let mut vectors = [[0_u64; 8]; 32];
    for (index, value) in vectors.iter_mut().enumerate() {
        *value = registers.get_zmm(index);
    }
    Vpermil2State {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_VPERMIL2_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[Vpermil2Case], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let bytes = encoding(case);
        let initial = initial_state(case, ordinal);
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
        .expect("run isolated native VPERMIL2 differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = cases();
    assert_eq!(cases.len(), 72);
    if let Some(range) = child_range() {
        execute_native_case_range(&cases, range);
        return;
    }

    let whole = run_child_range(test_name, 0..cases.len());
    if whole.status.success() {
        return;
    }
    let mut start = 0_usize;
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
        "isolated native VPERMIL2 failure at case {start}/{}: \
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
fn replay_matches_amd_o0_o2_equations_roles_aliases_m2z_and_full_state() {
    if !std::is_x86_feature_detected!("avx") || !x86_host_has_xop() {
        eprintln!("skipping native VPERMIL2 differential: host lacks AVX/XOP");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_vpermil2_replay::\
         replay_matches_amd_o0_o2_equations_roles_aliases_m2z_and_full_state",
    );
}
