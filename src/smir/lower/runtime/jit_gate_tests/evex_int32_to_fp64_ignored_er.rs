//! Native differential coverage for ignored I32-to-F64 EVEX control encodings.

use super::*;
#[cfg(target_arch = "x86_64")]
use crate::smir::lower::runtime::*;

const PC: u64 = 0xE67A;

fn encoding(signed: bool, ll: u8, destination: u8, source: u8, mask: u8, zeroing: bool) -> [u8; 6] {
    assert!(ll < 4 && destination < 32 && source < 32 && mask < 8);
    assert!(!zeroing || mask != 0);
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
        0x7E,
        (u8::from(zeroing) << 7) | (ll << 5) | 0x18 | mask,
        if signed { 0xE6 } else { 0x7A },
        0xC0 | ((destination & 0x07) << 3) | (source & 0x07),
    ]
}

fn canonical_encoding(mut bytes: [u8; 6]) -> [u8; 6] {
    bytes[3] = (bytes[3] & !0x70) | 0x40;
    bytes
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct ConversionState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

const I32_PATTERNS: [u32; 16] = [
    0,
    1,
    u32::MAX,
    i32::MAX as u32,
    i32::MIN as u32,
    0x0102_0304,
    0x7FFF_FFFE,
    0x8000_0001,
    2,
    u32::MAX - 1,
    0x4000_0000,
    0xC000_0000,
    0x00FF_00FF,
    0xFF00_FF00,
    0x5555_AAAA,
    0xAAAA_5555,
];
const OPERANDS: [(u8, u8); 3] = [(1, 2), (17, 18), (31, 31)];
const MASKS: [(u8, bool); 5] = [(0, false), (1, false), (1, true), (2, false), (3, true)];

fn patterned_vector(register: usize) -> [u64; 8] {
    let mut bytes = [0u8; 64];
    for lane in 0..16 {
        let value = I32_PATTERNS[(lane + register * 3) % I32_PATTERNS.len()];
        bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

fn initial_state(mxcsr: u32) -> ConversionState {
    let mut masks = [0u64; 8];
    masks[1] = 0b1010_1101;
    masks[2] = 0;
    masks[3] = u64::MAX;
    ConversionState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(patterned_vector),
        masks,
        rflags: 0x2 | 0x8D5,
        mxcsr,
    }
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
    initial: &ConversionState,
    level: crate::smir::optimize::OptLevel,
) -> ConversionState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
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
    ConversionState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

fn expected_state(
    initial: &ConversionState,
    signed: bool,
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
) -> ConversionState {
    let mut expected = initial.clone();
    let source_before = initial.vectors[usize::from(source)];
    let destination_before = initial.vectors[usize::from(destination)];
    let mask_bits = if mask == 0 {
        u64::MAX
    } else {
        initial.masks[usize::from(mask)]
    };
    for lane in 0..8 {
        let source_word = source_before[lane / 2];
        let raw = (source_word >> ((lane % 2) * 32)) as u32;
        expected.vectors[usize::from(destination)][lane] = if mask_bits & (1 << lane) != 0 {
            if signed {
                f64::from(raw as i32).to_bits()
            } else {
                f64::from(raw).to_bits()
            }
        } else if zeroing {
            0
        } else {
            destination_before[lane]
        };
    }
    expected
}

#[test]
fn interpretation_matches_exact_oracle_for_all_ignored_controls_and_optimization_levels() {
    let initial = initial_state(0xDFA5);
    let mut interpreted = 0usize;

    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O1,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for signed in [false, true] {
            for (destination, source) in OPERANDS {
                for (mask, zeroing) in MASKS {
                    let expected =
                        expected_state(&initial, signed, destination, source, mask, zeroing);
                    let mut ll_reference = None;
                    for ll in 0..=3 {
                        let bytes = encoding(signed, ll, destination, source, mask, zeroing);
                        let actual = interpret(&bytes, &initial, level);
                        assert_eq!(
                            actual, expected,
                            "level={level:?} signed={signed} {bytes:02X?}"
                        );
                        if let Some(reference) = &ll_reference {
                            assert_eq!(&actual, reference, "ignored L'L changed {bytes:02X?}");
                        } else {
                            ll_reference = Some(actual);
                        }
                        interpreted += 1;
                    }
                }
            }
        }
    }

    assert_eq!(
        interpreted,
        3 * 2 * OPERANDS.len() * MASKS.len() * 4,
        "all optimization/sign/register/mask/L'L combinations interpreted"
    );
}

#[test]
fn production_gate_and_lowerer_accept_all_ignored_controls_and_emit_canonical_evex() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut lowered_count = 0usize;
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O1,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for signed in [false, true] {
            for (destination, source) in OPERANDS {
                for (mask, zeroing) in MASKS {
                    for ll in 0..=3 {
                        let bytes = encoding(signed, ll, destination, source, mask, zeroing);
                        let function = optimized_function(&bytes, level, false);
                        assert!(
                            is_native_clobber_safe_excluding(
                                &function,
                                &std::collections::HashMap::new(),
                                true,
                            ),
                            "{level:?} {bytes:02X?}"
                        );

                        let mut lowerer = X86_64Lowerer::new();
                        lowerer
                            .lower_function(&function)
                            .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
                        let code = lowerer
                            .finalize()
                            .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
                        let canonical = canonical_encoding(bytes);
                        assert!(
                            code.windows(canonical.len())
                                .any(|window| window == canonical),
                            "{level:?} source={bytes:02X?} canonical={canonical:02X?}"
                        );
                        lowered_count += 1;
                    }
                }
            }
        }
    }

    assert_eq!(
        lowered_count,
        3 * 2 * OPERANDS.len() * MASKS.len() * 4,
        "all optimization/sign/register/mask/L'L combinations lowered"
    );
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8; 6],
    initial: &ConversionState,
    level: crate::smir::optimize::OptLevel,
) -> ConversionState {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let function = optimized_function(bytes, level, false);
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
    let canonical = canonical_encoding(*bytes);
    assert!(
        code.windows(canonical.len())
            .any(|window| window == canonical),
        "{level:?} source={bytes:02X?} canonical={canonical:02X?}"
    );

    let exec = ExecMem::new(&code).expect("map EVEX I32-to-F64 conversion");
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
    ConversionState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn canonical_native_lowering_matches_interpretation_for_all_ignored_controls() {
    if !std::is_x86_feature_detected!("avx512f") {
        eprintln!("skipping native I32-to-F64 differential: host lacks AVX-512F");
        return;
    }

    let mut executed = 0usize;

    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O1,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for signed in [false, true] {
            for ll in 0..=3 {
                for (operand_index, (destination, source)) in OPERANDS.into_iter().enumerate() {
                    for (mask, zeroing) in MASKS {
                        let bytes = encoding(signed, ll, destination, source, mask, zeroing);
                        let rc = ((operand_index as u32 + u32::from(ll)) & 3) << 13;
                        let prior_status = if operand_index & 1 == 0 { 1 << 5 } else { 0 };
                        let initial = initial_state(0x1F80 | rc | prior_status);
                        let interpreted = interpret(&bytes, &initial, level);
                        let native = execute_native(&bytes, &initial, level);
                        assert_eq!(
                            native, interpreted,
                            "level={level:?} signed={signed} {bytes:02X?} operand={operand_index}"
                        );
                        executed += 1;
                    }
                }
            }
        }
    }

    assert_eq!(
        executed,
        3 * 2 * 4 * OPERANDS.len() * MASKS.len(),
        "all optimization/sign/L'L/register/mask combinations executed"
    );
}
