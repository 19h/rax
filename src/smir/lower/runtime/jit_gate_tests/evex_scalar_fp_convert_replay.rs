//! Native replay coverage for EVEX scalar FP precision conversions.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x5A1D;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Conversion {
    F64ToF32,
    F32ToF64,
    F64ToF16,
    F16ToF64,
    F32ToF16,
    F16ToF32,
}

impl Conversion {
    const ALL: [Self; 6] = [
        Self::F64ToF32,
        Self::F32ToF64,
        Self::F64ToF16,
        Self::F16ToF64,
        Self::F32ToF16,
        Self::F16ToF32,
    ];

    fn fields(self) -> (u8, u8, u8, bool, bool) {
        match self {
            Self::F64ToF32 => (1, 0x5A, 3, true, false),
            Self::F32ToF64 => (1, 0x5A, 2, false, false),
            Self::F64ToF16 => (5, 0x5A, 3, true, true),
            Self::F16ToF64 => (5, 0x5A, 2, false, true),
            Self::F32ToF16 => (5, 0x1D, 0, false, true),
            Self::F16ToF32 => (6, 0x13, 0, false, true),
        }
    }

    fn has_embedded_rounding(self) -> bool {
        matches!(self, Self::F64ToF32 | Self::F64ToF16 | Self::F32ToF16)
    }

    fn valid_control(self, ll: u8, embedded_control: bool) -> bool {
        ll != 3 || (embedded_control && self.has_embedded_rounding())
    }

    #[cfg(target_arch = "x86_64")]
    fn source_width(self) -> u32 {
        match self {
            Self::F64ToF32 | Self::F64ToF16 => 64,
            Self::F32ToF64 | Self::F32ToF16 => 32,
            Self::F16ToF64 | Self::F16ToF32 => 16,
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn source_patterns(self) -> Vec<u64> {
        let mut patterns = match self.source_width() {
            16 => vec![
                0x0000, 0x8000, 0x0001, 0x8001, 0x03FF, 0x83FF, 0x0400, 0x8400, 0x3555, 0x3BFF,
                0x3C00, 0x3C01, 0x7BFF, 0xFBFF, 0x7C00, 0xFC00, 0x7D01, 0xFD55, 0x7E01, 0xFEA5,
            ],
            32 => vec![
                0x0000_0000,
                0x8000_0000,
                0x0000_0001,
                0x8000_0001,
                0x007F_FFFF,
                0x807F_FFFF,
                0x0080_0000,
                0x8080_0000,
                0x3300_0000,
                0x3380_0000,
                0x387F_C000,
                0x3880_0000,
                0x3F7F_FFFF,
                0x3F80_0000,
                0x3F80_0001,
                65_504.0f32.to_bits() as u64,
                65_519.0f32.to_bits() as u64,
                65_520.0f32.to_bits() as u64,
                f32::MAX.to_bits() as u64,
                f32::INFINITY.to_bits() as u64,
                f32::NEG_INFINITY.to_bits() as u64,
                0x7F81_2345,
                0xFF81_2345,
                0x7FC1_2345,
                0xFFC1_2345,
            ],
            64 => {
                let f32_midpoint = (1.0f64 + 2.0f64.powi(-24)).to_bits();
                let f16_midpoint = (1.0f64 + 2.0f64.powi(-11)).to_bits();
                let double_rounding = (1.0f64 + 2.0f64.powi(-11) + 2.0f64.powi(-30)).to_bits();
                vec![
                    0x0000_0000_0000_0000,
                    0x8000_0000_0000_0000,
                    0x0000_0000_0000_0001,
                    0x8000_0000_0000_0001,
                    0x000F_FFFF_FFFF_FFFF,
                    0x800F_FFFF_FFFF_FFFF,
                    0x0010_0000_0000_0000,
                    0x8010_0000_0000_0000,
                    0x3FEF_FFFF_FFFF_FFFF,
                    0x3FF0_0000_0000_0000,
                    0x3FF0_0000_0000_0001,
                    f32_midpoint - 1,
                    f32_midpoint,
                    f32_midpoint + 1,
                    f32_midpoint | (1 << 63),
                    f16_midpoint,
                    double_rounding,
                    65_504.0f64.to_bits(),
                    65_520.0f64.to_bits(),
                    f64::MAX.to_bits(),
                    f64::INFINITY.to_bits(),
                    f64::NEG_INFINITY.to_bits(),
                    0x7FF0_1234_5678_9ABC,
                    0xFFF0_1234_5678_9ABC,
                    0x7FF8_1234_5678_9ABC,
                    0xFFF8_1234_5678_9ABC,
                ]
            }
            _ => unreachable!(),
        };
        patterns.sort_unstable();
        patterns.dedup();
        assert!(
            patterns.len() <= 32,
            "{self:?}: {} patterns",
            patterns.len()
        );
        patterns
    }
}

#[allow(clippy::too_many_arguments)]
fn encoding(
    conversion: Conversion,
    ll: u8,
    embedded_control: bool,
    destination: u8,
    merge: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
) -> [u8; 6] {
    assert!(ll < 4 && destination < 32 && merge < 32 && source < 32 && mask < 8);
    assert!(!zeroing || mask != 0);
    let (map, opcode, pp, w, _) = conversion.fields();
    let mut p0 = 0xF0 | map;
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
        (if w { 0x80 } else { 0 }) | ((!merge & 0x0F) << 3) | 0x04 | pp,
        (if zeroing { 0x80 } else { 0 })
            | (ll << 5)
            | if embedded_control { 0x10 } else { 0 }
            | if merge & 0x10 == 0 { 0x08 } else { 0 }
            | mask,
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
fn replay_feature_aggregation_requires_fp16_only_for_fp16_conversions() {
    for conversion in Conversion::ALL {
        let ll = if conversion.has_embedded_rounding() {
            3
        } else {
            2
        };
        let bytes = encoding(conversion, ll, true, 31, 30, 29, 7, true);
        let function = function(&bytes);
        let actual =
            x86_native_replay_feature_requirements(&function, &std::collections::HashMap::new());
        assert!(actual.any, "{conversion:?} {bytes:02X?}");
        assert!(actual.needs_avx512bw, "{conversion:?} {bytes:02X?}");
        assert!(!actual.needs_avx512vl, "{conversion:?} {bytes:02X?}");
        assert!(!actual.needs_avx512dq, "{conversion:?} {bytes:02X?}");
        assert_eq!(
            actual.needs_avx512fp16,
            conversion.fields().4,
            "{conversion:?} {bytes:02X?}"
        );
        assert!(!actual.needs_avx512cd, "{conversion:?} {bytes:02X?}");
        assert!(!actual.needs_gfni, "{conversion:?} {bytes:02X?}");
        assert!(
            !actual.needs_avx512vp2intersect,
            "{conversion:?} {bytes:02X?}"
        );
        assert!(!actual.needs_vpclmulqdq, "{conversion:?} {bytes:02X?}");

        let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
        assert_eq!(
            x86_native_replay_feature_requirements(&function, &excluded),
            X86NativeReplayFeatureRequirements::default(),
            "{conversion:?} {bytes:02X?}"
        );
    }
}

#[test]
fn replay_admits_and_emits_312_o0_o2_mask_control_shapes_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let triples = [
        (0u8, 0u8, 0u8),
        (3, 2, 3),
        (8, 8, 15),
        (15, 16, 16),
        (16, 16, 16),
        (17, 24, 17),
        (24, 17, 24),
        (31, 30, 29),
    ];
    let masks = [(0u8, false), (1, false), (2, true), (7, true)];
    let mut lowered = 0usize;
    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for conversion in Conversion::ALL {
            for ll in 0..=3 {
                for embedded_control in [false, true] {
                    if !conversion.valid_control(ll, embedded_control) {
                        continue;
                    }
                    for (mask, zeroing) in masks {
                        let (destination, merge, source) = triples[lowered % triples.len()];
                        let bytes = encoding(
                            conversion,
                            ll,
                            embedded_control,
                            destination,
                            merge,
                            source,
                            mask,
                            zeroing,
                        );
                        let mut function = function(&bytes);
                        crate::smir::optimize::optimize_function(&mut function, level);
                        assert!(
                            is_native_clobber_safe(&function),
                            "{level:?} {conversion:?} {bytes:02X?}"
                        );
                        assert!(
                            uses_x86_native_vectors_excluding(
                                &function,
                                &std::collections::HashMap::new()
                            ),
                            "{level:?} {conversion:?} {bytes:02X?}"
                        );

                        #[cfg(target_arch = "x86_64")]
                        let expected_features = std::is_x86_feature_detected!("avx512f")
                            && std::is_x86_feature_detected!("avx512bw")
                            && (!conversion.fields().4
                                || std::is_x86_feature_detected!("avx512fp16"));
                        #[cfg(not(target_arch = "x86_64"))]
                        let expected_features = false;
                        assert_eq!(
                            x86_native_vector_features_supported_excluding(
                                &function,
                                &std::collections::HashMap::new()
                            ),
                            expected_features,
                            "{level:?} {conversion:?} {bytes:02X?}"
                        );

                        let mut lowerer = X86_64Lowerer::new();
                        lowerer.lower_function(&function).unwrap_or_else(|error| {
                            panic!("{level:?} {conversion:?} {bytes:02X?}: {error:?}")
                        });
                        let code = lowerer.finalize().unwrap_or_else(|error| {
                            panic!("{level:?} {conversion:?} {bytes:02X?}: {error:?}")
                        });
                        assert!(
                            code.windows(bytes.len()).any(|window| window == bytes),
                            "{level:?} {conversion:?} {bytes:02X?}"
                        );
                        lowered += 1;
                    }
                }
            }
        }
    }
    assert_eq!(lowered, 312);

    let replay_only = encoding(Conversion::F64ToF16, 3, true, 31, 30, 29, 7, true);
    let mut missing = function(&replay_only);
    missing.x86_instruction_bytes.clear();
    crate::smir::optimize::optimize_function(&mut missing, crate::smir::optimize::OptLevel::O2);
    assert!(!is_native_clobber_safe(&missing), "{replay_only:02X?}");

    for malformed_bytes in {
        let mut memory = replay_only;
        memory[5] &= 0x3F;
        let mut zeroing_k0 = replay_only;
        zeroing_k0[3] = (zeroing_k0[3] & !0x07) | 0x80;
        [memory, zeroing_k0]
    } {
        let mut malformed = function(&replay_only);
        malformed.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&malformed_bytes).unwrap(),
        );
        assert!(
            !is_native_clobber_safe(&malformed),
            "{malformed_bytes:02X?}"
        );
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct ConversionState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn initial_state(
    conversion: Conversion,
    source: u8,
    source_value: u64,
    mxcsr: u32,
) -> ConversionState {
    let gprs = std::array::from_fn(|register| {
        0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
            ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
    });
    let mut vectors = std::array::from_fn(|register| {
        std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((register * 11 + word * 5) as u32)
                ^ (register as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
        })
    });
    let source_mask = match conversion.source_width() {
        16 => u16::MAX as u64,
        32 => u32::MAX as u64,
        64 => u64::MAX,
        _ => unreachable!(),
    };
    vectors[source as usize][0] =
        (vectors[source as usize][0] & !source_mask) | (source_value & source_mask);
    ConversionState {
        gprs,
        vectors,
        masks: [
            0x6996_F00F_3CC3_A55A,
            0,
            1,
            0x0123_4567_89AB_CDEF,
            0x5555_AAAA_3333_CCCC,
            0x8000_0000_0000_0000,
            0xF0F0_0F0F_A5A5_5A5A,
            0,
        ],
        rflags: 0x2 | 0x8D5,
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
    initial: &ConversionState,
    level: crate::smir::optimize::OptLevel,
) -> ConversionState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::memory::FlatMemory;

    let function = optimized_function(bytes, level, true);
    let mut context = SmirContext::new_x86_64();
    context.pc = PC;
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gprs;
        for (index, value) in initial.vectors.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.masks;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
    }
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
    ConversionState {
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
    initial: &ConversionState,
    level: crate::smir::optimize::OptLevel,
) -> ConversionState {
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
    let exec = ExecMem::new(&code).expect("map EVEX scalar FP conversion replay");
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
const CHILD_RANGE_ENV: &str = "RAX_SCALAR_FP_CONVERT_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: crate::smir::optimize::OptLevel,
    conversion: Conversion,
    ll: u8,
    embedded_control: bool,
    destination: u8,
    merge: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
    source_value: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    let has_fp16 = std::is_x86_feature_detected!("avx512fp16");
    let triples = [
        (0u8, 0u8, 0u8),
        (3, 2, 3),
        (8, 8, 15),
        (15, 16, 16),
        (16, 16, 16),
        (17, 24, 17),
        (24, 17, 24),
        (31, 30, 29),
    ];
    let masks = [
        (0u8, false, true),
        (1, false, false),
        (2, true, true),
        (7, true, false),
    ];
    let mut cases = Vec::new();
    let mut cursors = std::collections::BTreeMap::new();
    let mut seen = std::collections::BTreeMap::<Conversion, std::collections::BTreeSet<u64>>::new();
    let mut shape = 0usize;

    for (level_index, level) in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ]
    .into_iter()
    .enumerate()
    {
        for conversion in Conversion::ALL {
            if conversion.fields().4 && !has_fp16 {
                continue;
            }
            let patterns = conversion.source_patterns();
            for ll in 0..=3 {
                for embedded_control in [false, true] {
                    if !conversion.valid_control(ll, embedded_control) {
                        continue;
                    }
                    for (mask, zeroing, active) in masks {
                        let source_value = if active {
                            let cursor = cursors.entry(conversion).or_insert(0usize);
                            let value = patterns[*cursor % patterns.len()];
                            *cursor += 1;
                            seen.entry(conversion).or_default().insert(value);
                            value
                        } else {
                            match conversion.source_width() {
                                16 => 0x7D55,
                                32 => 0x7F81_2345,
                                64 => 0x7FF0_1234_5678_9ABC,
                                _ => unreachable!(),
                            }
                        };
                        let (destination, merge, source) = triples[shape % triples.len()];
                        // L'L and MXCSR.RC are independent whenever L'L is
                        // ignored or carries embedded control. Offset RC by
                        // DAZ selection so the three legal LLIG encodings still
                        // exercise all four dynamic rounding modes.
                        let prior_status = [0, 1, 1 << 1, 1 << 3, 1 << 4, 1 << 5][(level_index
                            * 32
                            + usize::from(ll) * 8
                            + usize::from(embedded_control) * 4
                            + usize::from(mask))
                            % 6];
                        let rc = ((u32::from(ll) + u32::from(mask == 2)) & 3) << 13;
                        let daz = if mask == 2 { 1 << 6 } else { 0 };
                        let ftz = if level_index == 1 { 1 << 15 } else { 0 };
                        cases.push(NativeCase {
                            level,
                            conversion,
                            ll,
                            embedded_control,
                            destination,
                            merge,
                            source,
                            mask,
                            zeroing,
                            source_value,
                            mxcsr: 0x1F80 | prior_status | rc | daz | ftz,
                        });
                        shape += 1;
                    }
                }
            }
        }
    }

    // The exact F32-to-F64 boundary corpus has 25 values, while the legal
    // three-value LLIG matrix supplies 24 active O0/O2 mask/control slots.
    // Append any uncovered source values through a canonical legal control so
    // fail-closed admission does not reduce semantic boundary coverage.
    for conversion in Conversion::ALL {
        if conversion.fields().4 && !has_fp16 {
            continue;
        }
        for source_value in conversion.source_patterns() {
            if seen.entry(conversion).or_default().insert(source_value) {
                let (destination, merge, source) = triples[shape % triples.len()];
                cases.push(NativeCase {
                    level: crate::smir::optimize::OptLevel::O0,
                    conversion,
                    ll: 0,
                    embedded_control: false,
                    destination,
                    merge,
                    source,
                    mask: 0,
                    zeroing: false,
                    source_value,
                    mxcsr: 0x1F80,
                });
                shape += 1;
            }
        }
    }

    for conversion in Conversion::ALL {
        if conversion.fields().4 && !has_fp16 {
            continue;
        }
        assert_eq!(
            seen.get(&conversion)
                .map_or(0, std::collections::BTreeSet::len),
            conversion.source_patterns().len(),
            "{conversion:?} active source-pattern coverage"
        );
    }
    assert_eq!(cases.len(), if has_fp16 { 313 } else { 105 });
    cases
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_case_matrix_covers_formats_masks_aliases_register_banks_and_boundaries() {
    let cases = native_cases();
    assert!(
        cases
            .iter()
            .any(|case| case.conversion == Conversion::F64ToF32)
    );
    assert!(
        cases
            .iter()
            .any(|case| case.conversion == Conversion::F32ToF64)
    );
    assert_eq!(
        cases.iter().any(|case| case.conversion.fields().4),
        std::is_x86_feature_detected!("avx512fp16")
    );
    assert!(cases.iter().any(|case| case.destination >= 16));
    assert!(cases.iter().any(|case| case.merge >= 16));
    assert!(cases.iter().any(|case| case.source >= 16));
    assert!(cases.iter().any(|case| case.destination == case.merge));
    assert!(cases.iter().any(|case| case.destination == case.source));
    assert!(cases.iter().any(|case| case.merge == case.source));
    assert!(cases.iter().any(|case| case.mask == 0));
    assert!(cases.iter().any(|case| case.mask == 1 && !case.zeroing));
    assert!(cases.iter().any(|case| case.mask == 2 && case.zeroing));
    assert!(cases.iter().any(|case| case.mask == 7 && case.zeroing));

    for conversion in Conversion::ALL {
        if conversion.fields().4 && !std::is_x86_feature_detected!("avx512fp16") {
            continue;
        }
        let active = cases
            .iter()
            .filter(|case| case.conversion == conversion && matches!(case.mask, 0 | 2))
            .collect::<Vec<_>>();
        for embedded_control in [false, true] {
            let controls = active
                .iter()
                .filter(|case| case.embedded_control == embedded_control)
                .map(|case| {
                    (
                        (case.mxcsr >> 13) & 3,
                        case.mxcsr & (1 << 6) != 0,
                        case.mxcsr & (1 << 15) != 0,
                    )
                })
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                controls
                    .iter()
                    .map(|control| control.0)
                    .collect::<std::collections::BTreeSet<_>>(),
                std::collections::BTreeSet::from([0, 1, 2, 3]),
                "{conversion:?}: b={} MXCSR.RC",
                u8::from(embedded_control)
            );
            assert_eq!(
                controls
                    .iter()
                    .map(|control| control.1)
                    .collect::<std::collections::BTreeSet<_>>(),
                std::collections::BTreeSet::from([false, true]),
                "{conversion:?}: b={} MXCSR.DAZ",
                u8::from(embedded_control)
            );
            assert_eq!(
                controls
                    .iter()
                    .map(|control| control.2)
                    .collect::<std::collections::BTreeSet<_>>(),
                std::collections::BTreeSet::from([false, true]),
                "{conversion:?}: b={} MXCSR.FTZ",
                u8::from(embedded_control)
            );
        }
        assert_eq!(
            active
                .iter()
                .map(|case| (case.mxcsr >> 13) & 3)
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from([0, 1, 2, 3]),
            "{conversion:?}: MXCSR.RC"
        );
        assert_eq!(
            active
                .iter()
                .map(|case| case.mxcsr & (1 << 6) != 0)
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from([false, true]),
            "{conversion:?}: MXCSR.DAZ"
        );
        assert_eq!(
            active
                .iter()
                .map(|case| case.mxcsr & (1 << 15) != 0)
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from([false, true]),
            "{conversion:?}: MXCSR.FTZ"
        );
    }
}

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
fn execute_native_case_range(cases: &[NativeCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    for case in &cases[range] {
        let bytes = encoding(
            case.conversion,
            case.ll,
            case.embedded_control,
            case.destination,
            case.merge,
            case.source,
            case.mask,
            case.zeroing,
        );
        let initial = initial_state(case.conversion, case.source, case.source_value, case.mxcsr);
        assert_eq!(
            execute_native(&bytes, &initial, case.level),
            interpret(&bytes, &initial, case.level),
            "{case:?} bytes={bytes:02X?}"
        );
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
        .expect("run isolated native scalar FP conversion differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    if let Some(range) = child_range() {
        execute_native_case_range(&cases, range);
        return;
    }

    let whole = run_child_range(test_name, 0..cases.len());
    if whole.status.success() {
        return;
    }

    // Raw source replay can terminate the child with SIGILL or SIGFPE before
    // Rust reports assertion context. Bisect in O(log N) child launches and
    // report the exact guest encoding without terminating the parent binary.
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
    let bytes = encoding(
        case.conversion,
        case.ll,
        case.embedded_control,
        case.destination,
        case.merge,
        case.source,
        case.mask,
        case.zeroing,
    );
    panic!(
        "isolated native scalar FP conversion failure at case {start}/{}: \
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
fn replay_matches_o0_o2_interpretation_for_formats_masks_controls_values_and_mxcsr() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX scalar FP conversion differential: host lacks AVX-512F/BW");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::evex_scalar_fp_convert_replay::\
         replay_matches_o0_o2_interpretation_for_formats_masks_controls_values_and_mxcsr",
    );
}
