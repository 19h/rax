//! Native replay coverage for register-only legacy SSE and AVX VEX square root.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x5151;

#[derive(Clone, Copy, Debug)]
enum SqrtKind {
    PackedF32,
    PackedF64,
    ScalarF32,
    ScalarF64,
}

impl SqrtKind {
    const ALL: [Self; 4] = [
        Self::PackedF32,
        Self::PackedF64,
        Self::ScalarF32,
        Self::ScalarF64,
    ];

    fn pp(self) -> u8 {
        match self {
            Self::PackedF32 => 0,
            Self::PackedF64 => 1,
            Self::ScalarF32 => 2,
            Self::ScalarF64 => 3,
        }
    }

    fn packed(self) -> bool {
        matches!(self, Self::PackedF32 | Self::PackedF64)
    }

    fn elem_bits(self) -> u8 {
        if matches!(self, Self::PackedF32 | Self::ScalarF32) {
            32
        } else {
            64
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum EncodingForm {
    Legacy,
    LegacyRex,
    VexC5,
    VexC4W0,
    VexC4W1IgnoredX,
}

impl EncodingForm {
    fn is_vex(self) -> bool {
        matches!(self, Self::VexC5 | Self::VexC4W0 | Self::VexC4W1IgnoredX)
    }
}

fn encoding(form: EncodingForm, kind: SqrtKind, l: bool, dst: u8, src1: u8, src2: u8) -> Vec<u8> {
    assert!(dst < 16 && src1 < 16 && src2 < 16);
    assert!(kind.packed() || !l);
    let pp = kind.pp();
    match form {
        EncodingForm::Legacy | EncodingForm::LegacyRex => {
            assert!(!l);
            if matches!(form, EncodingForm::Legacy) {
                assert!(dst < 8 && src2 < 8);
            }
            let mut bytes = Vec::new();
            match pp {
                0 => {}
                1 => bytes.push(0x66),
                2 => bytes.push(0xF3),
                3 => bytes.push(0xF2),
                _ => unreachable!(),
            }
            if matches!(form, EncodingForm::LegacyRex) {
                // W and X are ignored for a register ModR/M source; R and B
                // select the architectural XMM registers.
                bytes.push(
                    0x4A | (if dst >= 8 { 0x04 } else { 0 }) | (if src2 >= 8 { 1 } else { 0 }),
                );
            }
            bytes.extend([0x0F, 0x51, 0xC0 | ((dst & 7) << 3) | (src2 & 7)]);
            bytes
        }
        EncodingForm::VexC5 => {
            assert!(src2 < 8);
            let encoded_vvvv = if kind.packed() { 0x0F } else { !src1 & 0x0F };
            vec![
                0xC5,
                (if dst < 8 { 0x80 } else { 0 })
                    | (encoded_vvvv << 3)
                    | (if l { 0x04 } else { 0 })
                    | pp,
                0x51,
                0xC0 | ((dst & 7) << 3) | src2,
            ]
        }
        EncodingForm::VexC4W0 | EncodingForm::VexC4W1IgnoredX => {
            let mut p0 = 0xE1;
            if dst >= 8 {
                p0 &= !0x80;
            }
            if matches!(form, EncodingForm::VexC4W1IgnoredX) {
                p0 &= !0x40;
            }
            if src2 >= 8 {
                p0 &= !0x20;
            }
            let encoded_vvvv = if kind.packed() { 0x0F } else { !src1 & 0x0F };
            vec![
                0xC4,
                p0,
                (if matches!(form, EncodingForm::VexC4W1IgnoredX) {
                    0x80
                } else {
                    0
                }) | (encoded_vvvv << 3)
                    | (if l { 0x04 } else { 0 })
                    | pp,
                0x51,
                0xC0 | ((dst & 7) << 3) | (src2 & 7),
            ]
        }
    }
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

fn cases() -> Vec<(EncodingForm, SqrtKind, bool, u8, u8, u8)> {
    let mut cases = Vec::new();
    for kind in SqrtKind::ALL {
        let lengths: &[bool] = if kind.packed() {
            &[false, true]
        } else {
            &[false]
        };
        for &l in lengths {
            for form in [
                EncodingForm::Legacy,
                EncodingForm::LegacyRex,
                EncodingForm::VexC5,
                EncodingForm::VexC4W0,
                EncodingForm::VexC4W1IgnoredX,
            ] {
                if !form.is_vex() && l {
                    continue;
                }
                let operands: &[(u8, u8, u8)] = match form {
                    EncodingForm::Legacy => &[(1, 0, 3), (1, 0, 1)],
                    EncodingForm::LegacyRex => &[(9, 0, 11), (9, 0, 9)],
                    EncodingForm::VexC5 => &[(1, 2, 3), (9, 10, 3), (1, 1, 2), (1, 2, 1)],
                    EncodingForm::VexC4W0 | EncodingForm::VexC4W1IgnoredX => {
                        &[(1, 2, 3), (9, 10, 11), (1, 1, 2), (1, 2, 1)]
                    }
                };
                for &(dst, src1, src2) in operands {
                    cases.push((form, kind, l, dst, src1, src2));
                }
            }
        }
    }
    cases
}

#[test]
fn replay_features_use_avx_ymm16_boundary_for_legacy_and_vex() {
    for form in [EncodingForm::LegacyRex, EncodingForm::VexC4W1IgnoredX] {
        let bytes = encoding(form, SqrtKind::ScalarF64, false, 9, 10, 11);
        let function = function(&bytes);
        let requirements =
            x86_native_replay_feature_requirements(&function, &std::collections::HashMap::new());
        assert!(requirements.any);
        assert!(requirements.all_spans_support_avx_ymm16);
        assert!(requirements.needs_avx);
        assert!(!requirements.needs_fma);
        assert!(!requirements.needs_avx512bw);
        assert!(!requirements.needs_avx512vl);
        assert!(!requirements.needs_avx512dq);
        assert!(!requirements.needs_avx512fp16);

        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            requirements.x86_host_supported(),
            std::is_x86_feature_detected!("avx")
        );
    }
}

#[test]
fn replay_admits_and_emits_176_legacy_vex_shapes_at_o0_o2_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let cases = cases();
    assert_eq!(cases.len(), 88);
    let mut lowered = 0usize;
    for (form, kind, l, dst, src1, src2) in cases {
        let bytes = encoding(form, kind, l, dst, src1, src2);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            let mut function = function(&bytes);
            crate::smir::optimize::optimize_function(&mut function, level);
            assert!(is_native_clobber_safe(&function), "{level:?} {bytes:02X?}");
            assert!(
                uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
                "{level:?} {bytes:02X?}"
            );
            let mut lowerer = X86_64Lowerer::new();
            lowerer
                .lower_function(&function)
                .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
            assert!(
                code.windows(bytes.len()).any(|window| window == bytes),
                "{level:?} {bytes:02X?}"
            );
            lowered += 1;
        }
    }
    assert_eq!(lowered, 176);

    let bytes = encoding(EncodingForm::VexC5, SqrtKind::ScalarF32, false, 1, 2, 3);
    let mut missing = function(&bytes);
    missing.x86_instruction_bytes.clear();
    assert!(!is_native_clobber_safe(&missing));

    let mut memory_bytes = bytes.clone();
    *memory_bytes.last_mut().unwrap() &= 0x3F;
    let mut memory_metadata = function(&bytes);
    memory_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&memory_bytes).unwrap(),
    );
    assert!(!is_native_clobber_safe(&memory_metadata));

    let mut scalar_l1 = bytes.clone();
    scalar_l1[1] |= 0x04;
    let scalar_l1_function = function(&scalar_l1);
    assert!(is_native_clobber_safe(&scalar_l1_function));
    let mut lowerer = X86_64Lowerer::new();
    lowerer
        .lower_function(&scalar_l1_function)
        .expect("lower canonical scalar VEX.L=1 square-root replay");
    let code = lowerer
        .finalize()
        .expect("finalize canonical scalar VEX.L=1 square-root replay");
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    assert!(
        !code
            .windows(scalar_l1.len())
            .any(|window| window == scalar_l1)
    );
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct SqrtState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn set_lane(vector: &mut [u64; 8], lane: u8, elem_bits: u8, value: u64) {
    match elem_bits {
        32 => {
            let word = usize::from(lane / 2);
            let shift = u32::from(lane & 1) * 32;
            vector[word] = (vector[word] & !(u64::from(u32::MAX) << shift))
                | (u64::from(value as u32) << shift);
        }
        64 => vector[usize::from(lane)] = value,
        _ => unreachable!(),
    }
}

#[cfg(target_arch = "x86_64")]
fn initial_state(case: (EncodingForm, SqrtKind, bool, u8, u8, u8), ordinal: usize) -> SqrtState {
    let (_, kind, _, _, _, src2) = case;
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((register * 11 + word * 5) as u32)
                ^ (register as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
        })
    });
    if kind.elem_bits() == 32 {
        let values = [
            u64::from(2.0f32.to_bits()),
            0xFF80_0000,
            0x7F80_0001,
            u64::from((-1.0f32).to_bits()),
            1,
            u64::from(4.0f32.to_bits()),
            0,
            0x8000_0000,
        ];
        for lane in 0..16u8 {
            set_lane(
                &mut vectors[usize::from(src2)],
                lane,
                32,
                values[usize::from(lane) % values.len()],
            );
        }
    } else {
        let values = [
            2.0f64.to_bits(),
            0xFFF0_0000_0000_0000,
            0x7FF0_0000_0000_0001,
            (-1.0f64).to_bits(),
            1,
            4.0f64.to_bits(),
            0,
            0x8000_0000_0000_0000,
        ];
        for lane in 0..8u8 {
            set_lane(
                &mut vectors[usize::from(src2)],
                lane,
                64,
                values[usize::from(lane)],
            );
        }
    }
    // Keep every SIMD exception masked. The CPU-level JIT admission gate
    // rejects native vector execution whenever any MXCSR mask is clear, so
    // this differential exercises native status accrual without permitting a
    // host SIGFPE to escape the replay boundary.
    let mxcsr = [
        0x1F80,
        0x1F80 | (1 << 13),
        0x1F80 | (2 << 13),
        0x1F80 | (3 << 13) | (1 << 6) | (1 << 15),
    ][ordinal % 4];
    SqrtState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors,
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
        mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
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

#[cfg(target_arch = "x86_64")]
fn interpret(
    bytes: &[u8],
    initial: &SqrtState,
    level: crate::smir::optimize::OptLevel,
) -> SqrtState {
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
    SqrtState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8],
    initial: &SqrtState,
    level: crate::smir::optimize::OptLevel,
) -> SqrtState {
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
    let exec = ExecMem::new(&code).expect("map legacy/VEX square-root replay");
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
    SqrtState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_LEGACY_VEX_SQRT_CHILD_RANGE";

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
fn execute_native_case_range(
    cases: &[(EncodingForm, SqrtKind, bool, u8, u8, u8)],
    range: std::ops::Range<usize>,
) {
    assert!(range.start < range.end && range.end <= cases.len());
    for (ordinal, &case) in cases.iter().enumerate().take(range.end).skip(range.start) {
        let (form, kind, l, dst, src1, src2) = case;
        let bytes = encoding(form, kind, l, dst, src1, src2);
        let initial = initial_state(case, ordinal);
        for level in [
            crate::smir::optimize::OptLevel::O0,
            crate::smir::optimize::OptLevel::O2,
        ] {
            assert_eq!(
                execute_native(&bytes, &initial, level),
                interpret(&bytes, &initial, level),
                "{level:?} {case:?} {bytes:02X?}"
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
        .expect("run isolated native legacy/VEX square-root differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = cases();
    assert_eq!(cases.len(), 88);
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
    let bytes = encoding(case.0, case.1, case.2, case.3, case.4, case.5);
    panic!(
        "isolated native legacy/VEX square-root failure at case {start}/{}: \
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
fn replay_matches_o0_o2_interpretation_for_legacy_vex_widths_aliases_and_mxcsr() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native legacy/VEX square-root differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::legacy_vex_fp_sqrt_replay::\
         replay_matches_o0_o2_interpretation_for_legacy_vex_widths_aliases_and_mxcsr",
    );
}
