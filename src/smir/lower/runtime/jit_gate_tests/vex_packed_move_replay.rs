//! Native replay coverage for register-only VEX `VMOVAPS`/`VMOVAPD` and
//! `VMOVDQA`/`VMOVDQU`.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x1011;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveKind {
    F32,
    F64,
    Dqa,
    Dqu,
}

impl MoveKind {
    const ALL: [Self; 4] = [Self::F32, Self::F64, Self::Dqa, Self::Dqu];

    fn pp(self) -> u8 {
        match self {
            Self::F32 => 0,
            Self::F64 | Self::Dqa => 1,
            Self::Dqu => 2,
        }
    }

    fn opcode(self, direction: Direction) -> u8 {
        match (self, direction) {
            (Self::F32 | Self::F64, Direction::Load) => 0x28,
            (Self::F32 | Self::F64, Direction::Store) => 0x29,
            (Self::Dqa | Self::Dqu, Direction::Load) => 0x6F,
            (Self::Dqa | Self::Dqu, Direction::Store) => 0x7F,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Direction {
    Load,
    Store,
}

impl Direction {
    const ALL: [Self; 2] = [Self::Load, Self::Store];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncodingForm {
    VexC5,
    VexC4W0,
    VexC4W1IgnoredX,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MoveCase {
    form: EncodingForm,
    kind: MoveKind,
    direction: Direction,
    l: bool,
    dst: u8,
    src: u8,
}

fn encoding(case: MoveCase) -> Vec<u8> {
    let MoveCase {
        form,
        kind,
        direction,
        l,
        dst,
        src,
    } = case;
    assert!(dst < 16 && src < 16);
    let opcode = kind.opcode(direction);
    let (reg, rm) = match direction {
        Direction::Load => (dst, src),
        Direction::Store => (src, dst),
    };

    match form {
        EncodingForm::VexC5 => {
            assert!(rm < 8);
            vec![
                0xC5,
                (if reg < 8 { 0x80 } else { 0 }) | 0x78 | (u8::from(l) << 2) | kind.pp(),
                opcode,
                0xC0 | ((reg & 7) << 3) | rm,
            ]
        }
        EncodingForm::VexC4W0 | EncodingForm::VexC4W1IgnoredX => {
            let mut p0 = 0xE1;
            if reg >= 8 {
                p0 &= !0x80;
            }
            if form == EncodingForm::VexC4W1IgnoredX {
                p0 &= !0x40;
            }
            if rm >= 8 {
                p0 &= !0x20;
            }
            vec![
                0xC4,
                p0,
                (if form == EncodingForm::VexC4W1IgnoredX {
                    0x80
                } else {
                    0
                }) | 0x78
                    | (u8::from(l) << 2)
                    | kind.pp(),
                opcode,
                0xC0 | ((reg & 7) << 3) | (rm & 7),
            ]
        }
    }
}

fn cases() -> Vec<MoveCase> {
    let mut cases = Vec::new();
    for kind in MoveKind::ALL {
        for direction in Direction::ALL {
            for l in [false, true] {
                for form in [
                    EncodingForm::VexC5,
                    EncodingForm::VexC4W0,
                    EncodingForm::VexC4W1IgnoredX,
                ] {
                    let operands: &[(u8, u8)] = match (form, direction) {
                        (EncodingForm::VexC5, Direction::Load) => &[(1, 3), (9, 3), (1, 1)],
                        (EncodingForm::VexC5, Direction::Store) => &[(3, 1), (3, 9), (1, 1)],
                        _ => &[(1, 3), (9, 11), (1, 1), (1, 9), (9, 1)],
                    };
                    for &(dst, src) in operands {
                        cases.push(MoveCase {
                            form,
                            kind,
                            direction,
                            l,
                            dst,
                            src,
                        });
                    }
                }
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

#[test]
fn replay_uses_the_avx_ymm16_bridge_without_avx512() {
    for case in [
        MoveCase {
            form: EncodingForm::VexC5,
            kind: MoveKind::F32,
            direction: Direction::Load,
            l: false,
            dst: 9,
            src: 3,
        },
        MoveCase {
            form: EncodingForm::VexC4W1IgnoredX,
            kind: MoveKind::F64,
            direction: Direction::Store,
            l: true,
            dst: 11,
            src: 9,
        },
        MoveCase {
            form: EncodingForm::VexC5,
            kind: MoveKind::Dqa,
            direction: Direction::Load,
            l: true,
            dst: 9,
            src: 3,
        },
        MoveCase {
            form: EncodingForm::VexC4W1IgnoredX,
            kind: MoveKind::Dqu,
            direction: Direction::Store,
            l: false,
            dst: 11,
            src: 9,
        },
    ] {
        let bytes = encoding(case);
        let function = function(&bytes);
        let excluded = std::collections::HashMap::new();
        let requirements = x86_native_replay_feature_requirements(&function, &excluded);
        assert!(requirements.any, "{case:?} {bytes:02X?}");
        assert!(requirements.needs_avx, "{case:?}");
        assert!(requirements.all_spans_support_avx_ymm16, "{case:?}");
        assert!(!requirements.needs_avx2, "{case:?}");
        assert!(!requirements.needs_vex_unaligned_packed_fp_move, "{case:?}");
        assert!(!requirements.needs_avx512bw, "{case:?}");
        assert!(!requirements.needs_avx512vl, "{case:?}");
        assert!(!requirements.needs_avx512dq, "{case:?}");
        assert!(!requirements.needs_avx512fp16, "{case:?}");
        assert!(
            x86_native_vector_uses_avx_ymm16_only_excluding(&function, &excluded),
            "{case:?}"
        );

        #[cfg(target_arch = "x86_64")]
        {
            let expected = std::is_x86_feature_detected!("avx");
            assert_eq!(requirements.x86_host_supported(), expected, "{case:?}");
            assert_eq!(
                x86_native_vector_features_supported_excluding(&function, &excluded),
                expected,
                "{case:?}"
            );
        }
    }
}

#[test]
fn replay_admits_and_emits_416_optimized_legal_encodings_and_nonmatching_provenance_stays_general()
{
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let cases = cases();
    assert_eq!(cases.len(), 208);
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
    assert_eq!(lowered, 416);

    let case = MoveCase {
        form: EncodingForm::VexC5,
        kind: MoveKind::Dqa,
        direction: Direction::Load,
        l: false,
        dst: 1,
        src: 3,
    };
    let bytes = encoding(case);
    let mut missing = function(&bytes);
    missing.x86_instruction_bytes.clear();

    let mut memory_bytes = bytes.clone();
    *memory_bytes.last_mut().unwrap() &= 0x3F;
    let mut memory_metadata = function(&bytes);
    memory_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&memory_bytes).unwrap(),
    );

    let mut nonreserved_vvvv = bytes.clone();
    nonreserved_vvvv[1] &= !0x08;
    let mut invalid_metadata = function(&bytes);
    invalid_metadata.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        crate::smir::ir::X86InstructionBytes::new(&nonreserved_vvvv).unwrap(),
    );

    for nonmatching in [missing, memory_metadata, invalid_metadata] {
        let excluded = std::collections::HashMap::new();
        assert!(is_native_clobber_safe(&nonmatching));
        assert!(
            uses_x86_native_vectors_excluding(&nonmatching, &excluded),
            "VMov remains available through the general vector lowerer"
        );
        assert!(
            !x86_native_replay_feature_requirements(&nonmatching, &excluded).any,
            "nonmatching provenance must not become an exact replay span"
        );
        assert!(
            !x86_native_vector_uses_avx_ymm16_only_excluding(&nonmatching, &excluded),
            "the AVX-only state bridge requires exact source-byte provenance"
        );
    }
}

fn upper_clear_postlude(destination: u8) -> Vec<u8> {
    assert!(destination < 16);
    let mut bytes = vec![0x9C, 0x50, 0x48, 0x8B, 0x45, X86_STATE_PTR_AT_RBP as u8];
    let upper = X86_GUEST_ZMM_OFFSET + i32::from(destination) * 64 + 32;
    for offset in (upper..upper + 32).step_by(8) {
        bytes.extend_from_slice(&[0x48, 0xC7, 0x80]);
        bytes.extend_from_slice(&(offset as u32).to_le_bytes());
        bytes.extend_from_slice(&[0, 0, 0, 0]);
    }
    bytes.extend_from_slice(&[0x58, 0x9D]);
    bytes
}

#[test]
fn ymm16_replay_clears_the_exact_state_backed_destination_upper_half() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    for case in [
        MoveCase {
            form: EncodingForm::VexC5,
            kind: MoveKind::F32,
            direction: Direction::Load,
            l: false,
            dst: 9,
            src: 3,
        },
        MoveCase {
            form: EncodingForm::VexC4W1IgnoredX,
            kind: MoveKind::F64,
            direction: Direction::Store,
            l: true,
            dst: 11,
            src: 9,
        },
        MoveCase {
            form: EncodingForm::VexC5,
            kind: MoveKind::Dqa,
            direction: Direction::Load,
            l: true,
            dst: 9,
            src: 3,
        },
        MoveCase {
            form: EncodingForm::VexC4W1IgnoredX,
            kind: MoveKind::Dqu,
            direction: Direction::Store,
            l: false,
            dst: 11,
            src: 9,
        },
    ] {
        let bytes = encoding(case);
        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_avx_ymm16_vector_state(true);
        lowerer
            .lower_function(&function(&bytes))
            .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
        let code = lowerer
            .finalize()
            .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
        let mut expected = bytes.clone();
        expected.extend_from_slice(&upper_clear_postlude(case.dst));
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{case:?} {bytes:02X?}"
        );
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MoveState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

fn initial_state(ordinal: usize) -> MoveState {
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
        rflags: 0x2 | 0x0CD5,
        mxcsr: [
            0x1F80,
            0x1F80 | 0x15,
            0x1F80 | (1 << 13) | (1 << 6),
            0x1F80 | (3 << 13) | (1 << 15),
        ][ordinal % 4],
    }
}

fn architectural_expected(case: MoveCase, initial: &MoveState) -> MoveState {
    let mut expected = initial.clone();
    let source = initial.vectors[usize::from(case.src)];
    let destination = &mut expected.vectors[usize::from(case.dst)];
    let words = if case.l { 4 } else { 2 };
    destination[..words].copy_from_slice(&source[..words]);
    destination[words..].fill(0);
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
    initial: &MoveState,
    level: crate::smir::optimize::OptLevel,
) -> MoveState {
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
    MoveState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_intel_o0_o2_state_for_directions_lengths_aliases_and_upper_lanes() {
    let cases = cases();
    assert_eq!(cases.len(), 208);
    for (ordinal, case) in cases.into_iter().enumerate() {
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
                "{level:?} {case:?} {bytes:02X?}"
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8],
    initial: &MoveState,
    level: crate::smir::optimize::OptLevel,
) -> MoveState {
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
    let exec = ExecMem::new(&code).expect("map VEX packed-move replay");
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
    MoveState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_PACKED_MOVE_CHILD_RANGE";

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
fn execute_native_case_range(cases: &[MoveCase], range: std::ops::Range<usize>) {
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
        .expect("run isolated native VEX packed-move differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = cases();
    assert_eq!(cases.len(), 208);
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
        "isolated native VEX packed-move failure at case {start}/{}: \
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
fn replay_matches_intel_o0_o2_state_for_directions_lengths_aliases_and_upper_lanes() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX packed-move differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_packed_move_replay::\
         replay_matches_intel_o0_o2_state_for_directions_lengths_aliases_and_upper_lanes",
    );
}
