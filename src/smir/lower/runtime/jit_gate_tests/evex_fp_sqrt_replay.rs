//! Native replay coverage for register-only EVEX floating-point square root.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x5100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SqrtKind {
    PackedF16,
    PackedF32,
    PackedF64,
    ScalarF32,
    ScalarF64,
}

impl SqrtKind {
    const ALL: [Self; 5] = [
        Self::PackedF16,
        Self::PackedF32,
        Self::PackedF64,
        Self::ScalarF32,
        Self::ScalarF64,
    ];

    fn fields(self) -> (u8, u8, bool, bool, bool, usize) {
        match self {
            Self::PackedF16 => (5, 0, false, false, true, 2),
            Self::PackedF32 => (1, 0, false, false, false, 4),
            Self::PackedF64 => (1, 1, true, false, false, 8),
            Self::ScalarF32 => (1, 2, false, true, false, 4),
            Self::ScalarF64 => (1, 3, true, true, false, 8),
        }
    }

    fn controls(self) -> Vec<(u8, bool)> {
        if self.fields().3 {
            (0..=2)
                .flat_map(|ll| [(ll, false), (ll, true)])
                .chain([(3, true)])
                .collect()
        } else {
            (0..=2)
                .map(|ll| (ll, false))
                .chain((0..=3).map(|ll| (ll, true)))
                .collect()
        }
    }
}

fn requirements(kind: SqrtKind, ll: u8, embedded_control: bool) -> (bool, bool) {
    let (_, _, _, scalar, fp16, _) = kind.fields();
    (!scalar && !embedded_control && ll != 2, fp16)
}

fn encoding(
    kind: SqrtKind,
    ll: u8,
    embedded_control: bool,
    destination: u8,
    merge: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
) -> [u8; 6] {
    let (map, pp, w, scalar, _, _) = kind.fields();
    assert!(ll < 4 && destination < 32 && merge < 32 && source < 32 && mask < 8);
    assert!(scalar || merge == 0);
    assert!(scalar || embedded_control || ll < 3);
    assert!(!zeroing || mask != 0);

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
    let encoded_merge = if scalar { merge } else { 0 };
    [
        0x62,
        p0,
        (((!encoded_merge) & 0x0F) << 3) | 0x04 | pp | if w { 0x80 } else { 0 },
        (if zeroing { 0x80 } else { 0 })
            | (ll << 5)
            | if embedded_control { 0x10 } else { 0 }
            | if encoded_merge < 16 { 0x08 } else { 0 }
            | mask,
        0x51,
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
fn replay_feature_aggregation_requires_bw_and_exact_vl_fp16_features() {
    for (kind, ll, embedded_control) in [
        (SqrtKind::PackedF16, 0, false),
        (SqrtKind::PackedF16, 3, true),
        (SqrtKind::PackedF32, 1, false),
        (SqrtKind::PackedF64, 2, false),
        (SqrtKind::ScalarF32, 2, false),
        (SqrtKind::ScalarF64, 2, true),
    ] {
        let scalar = kind.fields().3;
        let bytes = encoding(
            kind,
            ll,
            embedded_control,
            17,
            if scalar { 18 } else { 0 },
            24,
            1,
            false,
        );
        let function = function(&bytes);
        let expected = requirements(kind, ll, embedded_control);
        let actual =
            x86_native_replay_feature_requirements(&function, &std::collections::HashMap::new());
        assert!(actual.any, "{bytes:02X?}");
        assert!(actual.needs_avx512bw, "{bytes:02X?}");
        assert_eq!(actual.needs_avx512vl, expected.0, "{bytes:02X?}");
        assert!(!actual.needs_avx512dq, "{bytes:02X?}");
        assert_eq!(actual.needs_avx512fp16, expected.1, "{bytes:02X?}");
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
fn replay_admits_and_emits_525_optimized_legal_encodings_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let operands = [
        (1u8, 2u8, 3u8),
        (9, 10, 11),
        (17, 18, 19),
        (25, 26, 27),
        (31, 31, 31),
    ];
    let masks = [(0u8, false), (1, false), (1, true)];
    let mut admitted = 0usize;
    let mut missing_provenance_checked = false;
    let mut memory_metadata_checked = false;

    for kind in SqrtKind::ALL {
        let scalar = kind.fields().3;
        for (ll, embedded_control) in kind.controls() {
            let (needs_vl, needs_fp16) = requirements(kind, ll, embedded_control);
            for (destination, merge, source) in operands {
                for (mask, zeroing) in masks {
                    let bytes = encoding(
                        kind,
                        ll,
                        embedded_control,
                        destination,
                        if scalar { merge } else { 0 },
                        source,
                        mask,
                        zeroing,
                    );
                    let mut function = function(&bytes);
                    if !missing_provenance_checked {
                        let mut missing = function.clone();
                        missing.x86_instruction_bytes.clear();
                        crate::smir::optimize::optimize_function(
                            &mut missing,
                            crate::smir::optimize::OptLevel::O2,
                        );
                        assert!(!is_native_clobber_safe(&missing));
                        missing_provenance_checked = true;
                    }
                    if !memory_metadata_checked {
                        let mut memory = bytes;
                        memory[5] &= 0x3F;
                        let mut malformed = function.clone();
                        malformed.x86_instruction_bytes.insert(
                            (BlockId(0), PC),
                            crate::smir::ir::X86InstructionBytes::new(&memory).unwrap(),
                        );
                        assert!(!is_native_clobber_safe(&malformed));
                        memory_metadata_checked = true;
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

                    #[cfg(target_arch = "x86_64")]
                    let expected_features = std::is_x86_feature_detected!("avx512f")
                        && std::is_x86_feature_detected!("avx512bw")
                        && (!needs_vl || std::is_x86_feature_detected!("avx512vl"))
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
                    admitted += 1;
                }
            }
        }
    }

    assert!(missing_provenance_checked && memory_metadata_checked);
    assert_eq!(admitted, 525);
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
const F16_PATTERNS: [u64; 14] = [
    0x0000, 0x8000, 0x3C00, 0x4000, 0x4400, 0x0001, 0x8001, 0x0400, 0x7C00, 0xFC00, 0x7E01, 0x7C01,
    0xBC00, 0x3555,
];
#[cfg(target_arch = "x86_64")]
const F32_PATTERNS: [u64; 14] = [
    0x0000_0000,
    0x8000_0000,
    0x3F80_0000,
    0x4000_0000,
    0x4080_0000,
    0x0000_0001,
    0x8000_0001,
    0x0080_0000,
    0x7F80_0000,
    0xFF80_0000,
    0x7FC0_0001,
    0x7F80_0001,
    0xBF80_0000,
    0x3F00_0001,
];
#[cfg(target_arch = "x86_64")]
const F64_PATTERNS: [u64; 14] = [
    0x0000_0000_0000_0000,
    0x8000_0000_0000_0000,
    0x3FF0_0000_0000_0000,
    0x4000_0000_0000_0000,
    0x4010_0000_0000_0000,
    0x0000_0000_0000_0001,
    0x8000_0000_0000_0001,
    0x0010_0000_0000_0000,
    0x7FF0_0000_0000_0000,
    0xFFF0_0000_0000_0000,
    0x7FF8_0000_0000_0001,
    0x7FF0_0000_0000_0001,
    0xBFF0_0000_0000_0000,
    0x3FE0_0000_0000_0001,
];

#[cfg(target_arch = "x86_64")]
fn patterned_vector(kind: SqrtKind, register: usize) -> [u64; 8] {
    let element_size = kind.fields().5;
    let patterns: &[u64] = match element_size {
        2 => &F16_PATTERNS,
        4 => &F32_PATTERNS,
        8 => &F64_PATTERNS,
        _ => unreachable!(),
    };
    let mut bytes = [0u8; 64];
    for lane in 0..64 / element_size {
        let value = patterns[(lane + register * 3) % patterns.len()].to_le_bytes();
        let base = lane * element_size;
        bytes[base..base + element_size].copy_from_slice(&value[..element_size]);
    }
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

#[cfg(target_arch = "x86_64")]
fn initial_state(kind: SqrtKind, mxcsr: u32) -> SqrtState {
    let mut masks = [0u64; 8];
    masks[1] = 0xA55A_3CC3_F00F_9696;
    masks[2] = 0;
    masks[3] = u64::MAX;
    SqrtState {
        gprs: std::array::from_fn(|register| {
            0x0123_4567_89AB_CDEFu64.rotate_left((register * 7) as u32)
                ^ (register as u64).wrapping_mul(0x1020_4081_0204_0810)
        }),
        vectors: std::array::from_fn(|register| patterned_vector(kind, register)),
        masks,
        rflags: 0x2 | 0x8D5,
        mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn interpret(bytes: &[u8], initial: &SqrtState) -> SqrtState {
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
    SqrtState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(bytes: &[u8], initial: &SqrtState) -> SqrtState {
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
    let exec = ExecMem::new(&code).expect("map EVEX square-root replay");
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
    SqrtState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_interpreter_for_formats_controls_masks_extensions_aliases_and_mxcsr() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX square-root differential: host lacks AVX-512F/BW");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let has_fp16 = std::is_x86_feature_detected!("avx512fp16");
    let operands = [
        (1u8, 2u8, 3u8),
        (9, 10, 11),
        (17, 18, 19),
        (25, 26, 27),
        (1, 1, 3),
        (2, 3, 2),
        (4, 4, 4),
    ];
    let masks = [(0u8, false), (1, false), (1, true), (2, false), (3, true)];
    let mut executed = 0usize;
    let mut available_controls = 0usize;

    for kind in SqrtKind::ALL {
        let scalar = kind.fields().3;
        for (ll, embedded_control) in kind.controls() {
            let (needs_vl, needs_fp16) = requirements(kind, ll, embedded_control);
            if (needs_vl && !has_vl) || (needs_fp16 && !has_fp16) {
                continue;
            }
            available_controls += 1;
            for (operand_index, (destination, merge, source)) in operands.into_iter().enumerate() {
                for (mask, zeroing) in masks {
                    let bytes = encoding(
                        kind,
                        ll,
                        embedded_control,
                        destination,
                        if scalar { merge } else { 0 },
                        source,
                        mask,
                        zeroing,
                    );
                    // All exception masks remain set. Vary RC, DAZ, and FTZ;
                    // source replay is never admitted by the CPU JIT boundary
                    // when a guest exception mask is clear.
                    let rc = ((ll as u32 + operand_index as u32) & 3) << 13;
                    let denormal_controls = if operand_index & 1 == 0 {
                        0
                    } else {
                        (1 << 6) | (1 << 15)
                    };
                    let initial = initial_state(kind, 0x1F80 | rc | denormal_controls);
                    let interpreted = interpret(&bytes, &initial);
                    let native = execute_native(&bytes, &initial);
                    assert_eq!(
                        native, interpreted,
                        "{kind:?} {bytes:02X?} operand={operand_index}"
                    );
                    executed += 1;
                }
            }
        }
    }

    assert!(
        available_controls > 0,
        "feature-selected square-root controls"
    );
    assert_eq!(executed, available_controls * operands.len() * masks.len());
}
