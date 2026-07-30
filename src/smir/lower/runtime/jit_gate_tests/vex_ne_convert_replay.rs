//! Native source replay coverage for register-only AVX_NE_CONVERT
//! `VCVTNEPS2BF16`.

use super::*;
use crate::smir::ir::SmirFunction;
use crate::smir::ir::ops::OpKind;
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::*;
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xAEC0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RegisterCase {
    width: VecWidth,
    destination: u8,
    source: u8,
    clear_ignored_x: bool,
}

impl RegisterCase {
    fn bytes(self) -> [u8; 5] {
        assert!(self.destination < 16 && self.source < 16);
        [
            0xC4,
            (if self.destination < 8 { 0x80 } else { 0 })
                | (if self.clear_ignored_x { 0 } else { 0x40 })
                | (if self.source < 8 { 0x20 } else { 0 })
                | 2,
            0x7A | (u8::from(self.width == VecWidth::V256) << 2),
            0x72,
            0xC0 | ((self.destination & 7) << 3) | (self.source & 7),
        ]
    }
}

fn vector(index: u8, width: VecWidth) -> VReg {
    VReg::Arch(ArchReg::X86(match width {
        VecWidth::V128 => X86Reg::Xmm(index),
        VecWidth::V256 => X86Reg::Ymm(index),
        _ => unreachable!("AVX_NE_CONVERT VEX input width"),
    }))
}

fn cases() -> Vec<RegisterCase> {
    let operands = [
        (0, 0),
        (0, 1),
        (1, 0),
        (1, 1),
        (7, 8),
        (8, 7),
        (9, 10),
        (15, 15),
    ];
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for width in [VecWidth::V128, VecWidth::V256] {
        for (destination, source) in operands {
            cases.push(RegisterCase {
                width,
                destination,
                source,
                clear_ignored_x: ordinal & 1 != 0,
            });
            ordinal += 1;
        }
    }
    cases
}

fn function(bytes: &[u8]) -> SmirFunction {
    use crate::smir::ir::{SmirBlock, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};

    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));

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

fn assert_exact_op(function: &SmirFunction, expected: RegisterCase) {
    let [op] = function.blocks[0].ops.as_slice() else {
        panic!("{expected:?}: expected one conversion operation")
    };
    assert_eq!(op.guest_pc, PC);
    assert_eq!(op.x86_hint, None);
    assert!(
        matches!(
            op.kind,
            OpKind::VCvtFP32ToBF16 {
                dst,
                src1,
                src2: None,
                mask: None,
                width,
                zeroing: false,
            } if dst == vector(expected.destination, VecWidth::V128)
                && src1 == vector(expected.source, expected.width)
                && width == expected.width
        ),
        "{expected:?}: {:?}",
        op.kind
    );
}

#[test]
fn strict_lift_and_replay_cover_all_1_024_defined_register_images() {
    use crate::smir::ir::{SmirBlock, X86InstructionBytes};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    let mut lifted = 0usize;
    let mut lifter = X86_64Lifter::strict();
    for extension_bits in (0u8..8).map(|value| value << 5) {
        for width in [VecWidth::V128, VecWidth::V256] {
            for modrm in 0xC0u8..=0xFF {
                let bytes = [
                    0xC4,
                    extension_bits | 2,
                    0x7A | (u8::from(width == VecWidth::V256) << 2),
                    0x72,
                    modrm,
                ];
                let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
                let result = lifter
                    .lift_insn(PC, &bytes, &mut context)
                    .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
                assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
                let destination = (u8::from(extension_bits & 0x80 == 0) << 3) | ((modrm >> 3) & 7);
                let source = (u8::from(extension_bits & 0x20 == 0) << 3) | (modrm & 7);
                let [op] = result.ops.as_slice() else {
                    panic!("{bytes:02X?}: expected one conversion operation")
                };
                assert!(
                    matches!(
                        op.kind,
                        OpKind::VCvtFP32ToBF16 {
                            dst,
                            src1,
                            src2: None,
                            mask: None,
                            width: actual_width,
                            zeroing: false,
                        } if dst == vector(destination, VecWidth::V128)
                            && src1 == vector(source, width)
                            && actual_width == width
                    ),
                    "{bytes:02X?}: {:?}",
                    op.kind
                );

                let mut block = SmirBlock::new(BlockId(7), PC);
                block.ops = result.ops;
                let instruction = X86InstructionBytes::new(&bytes).unwrap();
                let provenance = std::collections::HashMap::from([((BlockId(7), PC), instruction)]);
                let spans = crate::smir::ir::x86_native_replay_spans(&block, &provenance);
                let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
                assert_eq!(span.end, 1, "{bytes:02X?}");
                assert_eq!(span.instruction, instruction, "{bytes:02X?}");
                lifted += 1;
            }
        }
    }
    assert_eq!(lifted, 1_024);
}

#[test]
fn replay_feature_requirements_select_only_avx_ne_convert_and_the_ymm16_bridge() {
    let case = RegisterCase {
        width: VecWidth::V256,
        destination: 15,
        source: 14,
        clear_ignored_x: true,
    };
    let function = function(&case.bytes());
    assert_exact_op(&function, case);
    let excluded = std::collections::HashMap::new();
    let requirements = x86_native_replay_feature_requirements(&function, &excluded);
    assert!(requirements.any);
    assert!(requirements.all_spans_support_avx_ymm16);
    assert!(requirements.needs_avx);
    assert!(requirements.needs_avx_ne_convert);
    assert!(!requirements.needs_avx2);
    assert!(!requirements.needs_avx_ifma);
    assert!(!requirements.needs_avx_vnni);
    assert!(!requirements.needs_avx_vnni_int8);
    assert!(!requirements.needs_avx_vnni_int16);
    assert!(!requirements.needs_f16c);
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
        let expected = std::is_x86_feature_detected!("avx") && x86_host_has_avx_ne_convert();
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
fn replay_admits_emits_and_upper_clears_alias_and_extension_shapes_at_o0_o1_o2() {
    assert_eq!(
        RegisterCase {
            width: VecWidth::V128,
            destination: 9,
            source: 10,
            clear_ignored_x: false,
        }
        .bytes(),
        [0xC4, 0x42, 0x7A, 0x72, 0xCA]
    );
    assert_eq!(
        RegisterCase {
            width: VecWidth::V256,
            destination: 15,
            source: 14,
            clear_ignored_x: true,
        }
        .bytes(),
        [0xC4, 0x02, 0x7E, 0x72, 0xFE]
    );

    let cases = cases();
    assert_eq!(cases.len(), 16);
    let expected_lowered = cases.len() * LEVELS.len();
    let mut lowered = 0usize;
    for case in cases {
        let bytes = case.bytes();
        for level in LEVELS {
            let mut function = function(&bytes);
            crate::smir::optimize::optimize_function(&mut function, level);
            assert_exact_op(&function, case);
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
    assert_eq!(lowered, expected_lowered);
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

    let mutations: [fn(&mut [u8; 5]); 6] = [
        |bytes| bytes[4] &= 0x3F,
        |bytes| bytes[2] &= !0x08,
        |bytes| bytes[2] |= 0x80,
        |bytes| bytes[2] = (bytes[2] & !3) | 1,
        |bytes| bytes[1] = (bytes[1] & !0x1F) | 1,
        |bytes| bytes[3] = 0x73,
    ];
    for mutate in mutations {
        let mut invalid = bytes;
        mutate(&mut invalid);
        let mut mutated = function(&bytes);
        mutated.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&invalid).unwrap(),
        );
        assert!(
            crate::smir::ir::X86InstructionBytes::new(&invalid)
                .unwrap()
                .vex_register_ne_convert_fields()
                .is_none(),
            "{invalid:02X?}"
        );
        let requirements =
            x86_native_replay_feature_requirements(&mutated, &std::collections::HashMap::new());
        assert!(!requirements.needs_avx_ne_convert, "{invalid:02X?}");
    }
}

#[cfg(target_arch = "x86_64")]
fn register_initial(case: RegisterCase, ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x0123_4567_89AB_CDEFu64.rotate_left((index * 7) as u32)
                ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_YMM16,
        mxcsr: [
            0x1F80,
            0x1F80 | 0x15,
            0x1F80 | (1 << 13) | (1 << 6),
            0x1F80 | (3 << 13) | (1 << 15),
        ][ordinal % 4],
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 11 + word * 5) as u32)
                ^ (index as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
                ^ (ordinal as u64).wrapping_mul(0x0804_0201_1020_4081)
        });
    }

    let values = [
        0x0000_0000u32,
        0x8000_0000,
        0x0000_0001,
        0x007F_FFFF,
        0x0080_0000,
        0x3F80_0000,
        0x3F80_8000,
        0x3F81_8000,
        0x7F80_0000,
        0xFF80_0000,
        0x7FC1_2345,
        0x7F81_2345,
        0xBF80_8000,
        0x7F7F_FFFF,
        0x0080_8000,
        0x8080_8000,
    ];
    let mut bytes = [0u8; 64];
    let lanes = case.width.bytes() as usize / 4;
    for (lane, chunk) in bytes[..lanes * 4].chunks_exact_mut(4).enumerate() {
        chunk.copy_from_slice(&values[(ordinal + lane) % values.len()].to_le_bytes());
    }
    registers.zmm[usize::from(case.source)] = std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    });
    registers
}

#[cfg(target_arch = "x86_64")]
fn optimized_function(bytes: &[u8], level: OptLevel) -> SmirFunction {
    let mut function = function(bytes);
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

#[cfg(target_arch = "x86_64")]
fn interpret_register(function: &SmirFunction, initial: &GuestRegs) -> GuestRegs {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, value) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.k;
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
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut expected = *initial;
    expected.gpr = x86.gpr;
    for (index, value) in x86.xmm.iter().enumerate() {
        expected.zmm[index].copy_from_slice(&value[..8]);
    }
    expected.k = x86.k;
    expected.rflags = x86.rflags;
    expected.mxcsr = x86.mxcsr;
    expected
}

#[cfg(target_arch = "x86_64")]
fn execute_register(function: &SmirFunction, initial: &GuestRegs, bytes: &[u8]) -> GuestRegs {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_avx_ymm16_vector_state(true);
    let lowered = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert!(code.windows(bytes.len()).any(|window| window == bytes));
    let exec = ExecMem::new(&code).expect("map AVX_NE_CONVERT register replay");
    let mut registers = *initial;
    exec.run(lowered.entry_offset, &mut registers);
    registers
}

#[cfg(target_arch = "x86_64")]
const NATIVE_CHILD_ENV: &str = "RAX_VEX_NE_CONVERT_REGISTER_CHILD";

#[cfg(target_arch = "x86_64")]
#[test]
fn register_replay_matches_interpretation_on_an_avx_ne_convert_host() {
    if std::env::var_os(NATIVE_CHILD_ENV).is_some() {
        for (ordinal, case) in cases().into_iter().enumerate() {
            let bytes = case.bytes();
            let initial = register_initial(case, ordinal);
            for level in [OptLevel::O0, OptLevel::O2] {
                let function = optimized_function(&bytes, level);
                let mut expected = interpret_register(&function, &initial);
                let actual = execute_register(&function, &initial, &bytes);
                expected.host_mxcsr = actual.host_mxcsr;
                assert_eq!(actual, expected, "{level:?} {case:?} {bytes:02X?}");
            }
        }
        return;
    }
    if !std::is_x86_feature_detected!("avx") || !x86_host_has_avx_ne_convert() {
        eprintln!("skipping native AVX_NE_CONVERT register differential: host feature unavailable");
        return;
    }

    let test_name = "smir::lower::runtime::jit_gate_tests::vex_ne_convert_replay::\
                     register_replay_matches_interpretation_on_an_avx_ne_convert_host";
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg(test_name)
        .arg("--nocapture")
        .env(NATIVE_CHILD_ENV, "1")
        .output()
        .expect("spawn isolated AVX_NE_CONVERT register differential");
    assert!(
        output.status.success(),
        "isolated AVX_NE_CONVERT register differential failed: {}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
