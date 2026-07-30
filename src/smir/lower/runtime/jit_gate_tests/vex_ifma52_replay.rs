//! Native source replay coverage for register-only AVX-IFMA
//! `VPMADD52LUQ`/`VPMADD52HUQ`.

use super::*;
use crate::smir::ir::SmirFunction;
use crate::smir::lower::runtime::*;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0x1F52;
const MASK52: u64 = (1u64 << 52) - 1;
const OPERANDS: [(u8, u8, u8); 8] = [
    (0, 1, 2),
    (0, 0, 2),
    (0, 1, 0),
    (0, 1, 1),
    (0, 0, 0),
    (9, 10, 11),
    (15, 14, 13),
    (15, 15, 15),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IfmaCase {
    high: bool,
    width: VecWidth,
    destination: u8,
    source1: u8,
    source2: u8,
    clear_ignored_x: bool,
}

impl IfmaCase {
    fn opcode(self) -> u8 {
        if self.high { 0xB5 } else { 0xB4 }
    }

    fn bytes(self) -> [u8; 5] {
        assert!(self.destination < 16 && self.source1 < 16 && self.source2 < 16);
        let mut p0 = 0xE2;
        if self.destination >= 8 {
            p0 &= !0x80;
        }
        if self.clear_ignored_x {
            p0 &= !0x40;
        }
        if self.source2 >= 8 {
            p0 &= !0x20;
        }
        [
            0xC4,
            p0,
            0x80 | (((!self.source1) & 0x0F) << 3)
                | (u8::from(self.width == VecWidth::V256) << 2)
                | 1,
            self.opcode(),
            0xC0 | ((self.destination & 7) << 3) | (self.source2 & 7),
        ]
    }
}

fn cases() -> Vec<IfmaCase> {
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for high in [false, true] {
        for width in [VecWidth::V128, VecWidth::V256] {
            for (destination, source1, source2) in OPERANDS {
                cases.push(IfmaCase {
                    high,
                    width,
                    destination,
                    source1,
                    source2,
                    clear_ignored_x: ordinal & 1 != 0,
                });
                ordinal += 1;
            }
        }
    }
    cases
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("AVX-IFMA VEX uses XMM or YMM"),
    }))
}

fn function(bytes: &[u8]) -> SmirFunction {
    use crate::smir::ir::{SmirBlock, X86InstructionBytes};
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
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("architectural x86 instruction length"),
    );
    function
}

#[test]
fn strict_lift_and_replay_cover_all_32_768_defined_register_images() {
    use crate::smir::ir::ops::OpKind;
    use crate::smir::ir::types::SourceArch;
    use crate::smir::ir::{SmirBlock, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    let mut lifted = 0usize;
    let mut lifter = X86_64Lifter::strict();
    for opcode in [0xB4, 0xB5] {
        for extension_bits in (0u8..8).map(|value| value << 5) {
            for encoded_vvvv in 0u8..16 {
                for width in [VecWidth::V128, VecWidth::V256] {
                    for modrm in 0xC0u8..=0xFF {
                        let bytes = [
                            0xC4,
                            extension_bits | 2,
                            0x80 | (encoded_vvvv << 3)
                                | (u8::from(width == VecWidth::V256) << 2)
                                | 1,
                            opcode,
                            modrm,
                        ];
                        let mut context = LiftContext::new(SourceArch::X86_64);
                        let result = lifter
                            .lift_insn(PC, &bytes, &mut context)
                            .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
                        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
                        let [op] = result.ops.as_slice() else {
                            panic!("{bytes:02X?}: expected one operation")
                        };
                        let destination =
                            (u8::from(extension_bits & 0x80 == 0) << 3) | ((modrm >> 3) & 7);
                        let source1 = (!encoded_vvvv) & 0x0F;
                        let source2 = (u8::from(extension_bits & 0x20 == 0) << 3) | (modrm & 7);
                        assert!(
                            matches!(
                                op.kind,
                                OpKind::VMultiplyAdd52 {
                                    dst,
                                    acc,
                                    src1,
                                    src2,
                                    mask: None,
                                    width: actual_width,
                                    high,
                                    zeroing: false,
                                } if dst == vector(destination, width)
                                    && acc == dst
                                    && src1 == vector(source1, width)
                                    && src2 == vector(source2, width)
                                    && actual_width == width
                                    && high == (opcode == 0xB5)
                            ),
                            "{bytes:02X?}: {:?}",
                            op.kind
                        );

                        let mut block = SmirBlock::new(BlockId(7), PC);
                        block.ops = result.ops;
                        let instruction = X86InstructionBytes::new(&bytes).unwrap();
                        let provenance =
                            std::collections::HashMap::from([((BlockId(7), PC), instruction)]);
                        let spans = crate::smir::ir::x86_native_replay_spans(&block, &provenance);
                        let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
                        assert_eq!(span.end, 1, "{bytes:02X?}");
                        assert_eq!(span.instruction, instruction, "{bytes:02X?}");
                        lifted += 1;
                    }
                }
            }
        }
    }
    assert_eq!(lifted, 32_768);
}

#[test]
fn replay_feature_requirements_select_avx_ifma_and_the_ymm16_bridge() {
    let case = IfmaCase {
        high: true,
        width: VecWidth::V256,
        destination: 15,
        source1: 14,
        source2: 13,
        clear_ignored_x: true,
    };
    let function = function(&case.bytes());
    let excluded = std::collections::HashMap::new();
    let requirements = x86_native_replay_feature_requirements(&function, &excluded);
    assert!(requirements.any);
    assert!(requirements.all_spans_support_avx_ymm16);
    assert!(requirements.needs_avx);
    assert!(requirements.needs_avx_ifma);
    assert!(!requirements.needs_avx2);
    assert!(!requirements.needs_avx_vnni);
    assert!(!requirements.needs_avx_vnni_int8);
    assert!(!requirements.needs_avx_vnni_int16);
    assert!(!requirements.needs_fma);
    assert!(!requirements.needs_avx512bw);
    assert!(!requirements.needs_avx512vl);
    assert!(!requirements.needs_avx512dq);
    assert!(!requirements.needs_avx512fp16);
    assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
        &function, &excluded
    ));

    #[cfg(target_arch = "x86_64")]
    {
        let expected = std::is_x86_feature_detected!("avx") && x86_host_has_avx_ifma();
        assert_eq!(requirements.x86_host_supported(), expected);
        assert_eq!(
            x86_native_vector_features_supported_excluding(&function, &excluded),
            expected
        );
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[test]
fn replay_admits_emits_and_upper_clears_every_variant_alias_shape_at_o0_o1_o2() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    assert_eq!(
        IfmaCase {
            high: false,
            width: VecWidth::V128,
            destination: 0,
            source1: 1,
            source2: 2,
            clear_ignored_x: false,
        }
        .bytes(),
        [0xC4, 0xE2, 0xF1, 0xB4, 0xC2]
    );
    assert_eq!(
        IfmaCase {
            high: true,
            width: VecWidth::V256,
            destination: 15,
            source1: 14,
            source2: 13,
            clear_ignored_x: true,
        }
        .bytes(),
        [0xC4, 0x02, 0x8D, 0xB5, 0xFD]
    );

    let cases = cases();
    assert_eq!(cases.len(), 32);
    let mut lowered = 0usize;
    for case in cases {
        let bytes = case.bytes();
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let mut function = function(&bytes);
            crate::smir::optimize::optimize_function(&mut function, level);
            assert!(
                is_native_clobber_safe(&function),
                "{level:?} {case:?} {bytes:02X?}"
            );
            assert!(x86_native_vector_uses_avx_ymm16_only_excluding(
                &function,
                &std::collections::HashMap::new()
            ));

            let mut lowerer = X86_64Lowerer::new();
            lowerer.set_avx_ymm16_vector_state(true);
            lowerer
                .lower_function(&function)
                .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let instruction_offset = find_subslice(&code, &bytes)
                .unwrap_or_else(|| panic!("{level:?} {case:?} {bytes:02X?}"));
            let tail = &code[instruction_offset + bytes.len()..];
            let upper =
                crate::smir::lower::X86_GUEST_ZMM_OFFSET + i32::from(case.destination) * 64 + 32;
            for offset in (upper..upper + 32).step_by(8) {
                let mut clear = vec![0x48, 0xC7, 0x80];
                clear.extend_from_slice(&(offset as u32).to_le_bytes());
                clear.extend_from_slice(&0u32.to_le_bytes());
                assert!(
                    find_subslice(tail, &clear).is_some(),
                    "missing upper clear {offset}: {level:?} {case:?}"
                );
            }
            lowered += 1;
        }
    }
    assert_eq!(lowered, 96);
}

#[test]
fn replay_fails_closed_without_exact_register_provenance() {
    let case = cases()[0];
    let bytes = case.bytes();
    let mut missing = function(&bytes);
    missing.x86_instruction_bytes.clear();
    assert!(
        crate::smir::ir::x86_native_replay_spans(
            &missing.blocks[0],
            &missing.x86_instruction_bytes
        )
        .is_empty()
    );
    assert!(!x86_native_vector_uses_avx_ymm16_only_excluding(
        &missing,
        &std::collections::HashMap::new()
    ));

    let mutations: [fn(&mut [u8; 5]); 4] = [
        |bytes| bytes[4] &= 0x3F,
        |bytes| bytes[2] &= !0x80,
        |bytes| bytes[2] = (bytes[2] & !3) | 2,
        |bytes| bytes[1] = (bytes[1] & !0x1F) | 1,
    ];
    for mutate in mutations {
        let mut invalid = bytes;
        mutate(&mut invalid);
        let mut function = function(&bytes);
        function.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&invalid).unwrap(),
        );
        assert!(
            crate::smir::ir::X86InstructionBytes::new(&invalid)
                .unwrap()
                .vex_register_ifma52_fields()
                .is_none(),
            "{invalid:02X?}"
        );
        let requirements =
            x86_native_replay_feature_requirements(&function, &std::collections::HashMap::new());
        assert!(!requirements.needs_avx_ifma, "{invalid:02X?}");
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct IfmaState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

const VECTOR_PATTERNS: [u64; 12] = [
    0,
    1,
    MASK52 - 1,
    MASK52,
    MASK52 + 1,
    u64::MAX,
    0x0008_0000_0000_0000,
    0x000F_FFFF_FFFF_FFFE,
    0x0010_0000_0000_0001,
    0x5555_AAAA_3333_CCCC,
    0xA5A5_5A5A_3CC3_C33C,
    0x0123_4567_89AB_CDEF,
];

fn initial_state(case: IfmaCase, ordinal: usize) -> IfmaState {
    let vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            VECTOR_PATTERNS[(ordinal + register * 3 + word * 7) % VECTOR_PATTERNS.len()]
        })
    });
    IfmaState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
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
        mxcsr: [
            0x1F80,
            0x1F80 | 0x15,
            0x1F80 | (1 << 13),
            0x1F80 | (3 << 13),
        ][ordinal % 4],
    }
}

fn product_term(high: bool, first: u64, second: u64) -> u64 {
    let product = u128::from(first & MASK52) * u128::from(second & MASK52);
    if high {
        ((product >> 52) as u64) & MASK52
    } else {
        (product as u64) & MASK52
    }
}

fn architectural_expected(case: IfmaCase, initial: &IfmaState) -> IfmaState {
    let mut expected = initial.clone();
    expected.vectors[usize::from(case.destination)] = [0; 8];
    for lane in 0..case.width.bytes() as usize / 8 {
        expected.vectors[usize::from(case.destination)][lane] =
            initial.vectors[usize::from(case.destination)][lane].wrapping_add(product_term(
                case.high,
                initial.vectors[usize::from(case.source1)][lane],
                initial.vectors[usize::from(case.source2)][lane],
            ));
    }
    expected
}

fn optimized_function(bytes: &[u8], level: OptLevel, halt: bool) -> SmirFunction {
    let mut function = function(bytes);
    if halt {
        function.blocks[0].set_terminator(Terminator::Trap {
            kind: crate::smir::ir::TrapKind::Halt,
        });
    }
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn interpret(bytes: &[u8], initial: &IfmaState, level: OptLevel) -> IfmaState {
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
    IfmaState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[test]
fn interpreter_matches_intel_equations_at_o0_o2_for_boundaries_aliases_and_upper_zeroing() {
    for (ordinal, case) in cases().into_iter().enumerate() {
        let bytes = case.bytes();
        let initial = initial_state(case, ordinal);
        let expected = architectural_expected(case, &initial);
        for level in [OptLevel::O0, OptLevel::O2] {
            assert_eq!(
                interpret(&bytes, &initial, level),
                expected,
                "{level:?} {case:?} {bytes:02X?}"
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(bytes: &[u8], initial: &IfmaState, level: OptLevel) -> IfmaState {
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
    let exec = ExecMem::new(&code).expect("map AVX-IFMA replay");
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
    IfmaState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
const CHILD_ENV: &str = "RAX_VEX_IFMA52_CHILD";

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_intel_equations_on_an_avx_ifma_host() {
    if std::env::var_os(CHILD_ENV).is_some() {
        for (ordinal, case) in cases().into_iter().enumerate() {
            let bytes = case.bytes();
            let initial = initial_state(case, ordinal);
            let expected = architectural_expected(case, &initial);
            for level in [OptLevel::O0, OptLevel::O2] {
                assert_eq!(
                    execute_native(&bytes, &initial, level),
                    expected,
                    "{level:?} {case:?} {bytes:02X?}"
                );
            }
        }
        return;
    }
    if !std::is_x86_feature_detected!("avx") || !x86_host_has_avx_ifma() {
        eprintln!("skipping real AVX-IFMA execution: host feature unavailable");
        return;
    }

    let test_name = "smir::lower::runtime::jit_gate_tests::vex_ifma52_replay::\
                     replay_matches_intel_equations_on_an_avx_ifma_host";
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg(test_name)
        .arg("--nocapture")
        .env(CHILD_ENV, "1")
        .output()
        .expect("spawn isolated AVX-IFMA replay test");
    assert!(
        output.status.success(),
        "isolated AVX-IFMA replay failed: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

mod memory_source;
