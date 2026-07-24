//! Native replay coverage for register-only EVEX FP16 widening conversions.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x5A13;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WidenKind {
    ToF64,
    ToF32,
    ToF32X,
}

impl WidenKind {
    const ALL: [Self; 3] = [Self::ToF64, Self::ToF32, Self::ToF32X];

    fn fields(self) -> (u8, u8, u8, bool, usize) {
        match self {
            Self::ToF64 => (5, 0, 0x5A, true, 8),
            Self::ToF32 => (2, 1, 0x13, false, 4),
            Self::ToF32X => (6, 1, 0x13, true, 4),
        }
    }
}

fn requirements(kind: WidenKind, ll: u8, suppress_exceptions: bool) -> (bool, bool) {
    (!suppress_exceptions && ll != 2, kind.fields().3)
}

fn encoding(
    kind: WidenKind,
    ll: u8,
    suppress_exceptions: bool,
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
) -> [u8; 6] {
    assert!(ll < 3 && destination < 32 && source < 32 && mask < 8);
    assert!(!suppress_exceptions || ll == 0);
    assert!(!zeroing || mask != 0);
    let (map, pp, opcode, _, _) = kind.fields();
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
        0x7C | pp,
        (if zeroing { 0x80 } else { 0 })
            | (ll << 5)
            | if suppress_exceptions { 0x10 } else { 0 }
            | 0x08
            | mask,
        opcode,
        0xC0 | ((destination & 0x07) << 3) | (source & 0x07),
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
fn replay_feature_aggregation_requires_bw_and_exact_vl_fp16_features() {
    for kind in WidenKind::ALL {
        for (ll, suppress_exceptions) in [(0, false), (1, false), (2, false), (0, true)] {
            let bytes = encoding(kind, ll, suppress_exceptions, 17, 18, 1, false);
            let function = function(&bytes);
            let (needs_vl, needs_fp16) = requirements(kind, ll, suppress_exceptions);
            let actual = x86_native_replay_feature_requirements(
                &function,
                &std::collections::HashMap::new(),
            );
            assert!(actual.any, "{bytes:02X?}");
            assert!(actual.needs_avx512bw, "{bytes:02X?}");
            assert_eq!(actual.needs_avx512vl, needs_vl, "{bytes:02X?}");
            assert!(!actual.needs_avx512dq, "{bytes:02X?}");
            assert_eq!(actual.needs_avx512fp16, needs_fp16, "{bytes:02X?}");
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
}

#[test]
fn replay_admits_and_emits_180_optimized_legal_encodings_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let operands = [(1u8, 2u8), (9, 10), (17, 18), (25, 26), (31, 31)];
    let masks = [(0u8, false), (1, false), (1, true)];
    let mut admitted = 0usize;
    let mut missing_provenance_checked = false;
    let mut memory_metadata_checked = false;

    for kind in WidenKind::ALL {
        for (ll, suppress_exceptions) in [(0, false), (1, false), (2, false), (0, true)] {
            let (needs_vl, needs_fp16) = requirements(kind, ll, suppress_exceptions);
            for (destination, source) in operands {
                for (mask, zeroing) in masks {
                    let bytes = encoding(
                        kind,
                        ll,
                        suppress_exceptions,
                        destination,
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
    assert_eq!(admitted, 180);
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct WidenState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
const F16_PATTERNS: [u16; 16] = [
    0x0000, 0x8000, 0x3C00, 0xC000, 0x0001, 0x8001, 0x03FF, 0x83FF, 0x0400, 0x7BFF, 0x7C00, 0xFC00,
    0x7E01, 0xFE01, 0x7C01, 0xFC01,
];

#[cfg(target_arch = "x86_64")]
fn patterned_vector(register: usize) -> [u64; 8] {
    let mut bytes = [0u8; 64];
    for lane in 0..32 {
        let value = F16_PATTERNS[(lane + register * 5) % F16_PATTERNS.len()];
        bytes[lane * 2..lane * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

#[cfg(target_arch = "x86_64")]
fn initial_state(mxcsr: u32) -> WidenState {
    let mut masks = [0u64; 8];
    masks[1] = 0xA55A_3CC3_F00F_9696;
    masks[2] = 0;
    masks[3] = u64::MAX;
    WidenState {
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
    initial: &WidenState,
    level: crate::smir::optimize::OptLevel,
) -> WidenState {
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
    WidenState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(
    bytes: &[u8; 6],
    initial: &WidenState,
    level: crate::smir::optimize::OptLevel,
) -> WidenState {
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
    let exec = ExecMem::new(&code).expect("map EVEX FP16 widening replay");
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
    WidenState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_o0_o2_interpretation_for_formats_controls_masks_aliases_and_mxcsr() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native FP16 widening differential: host lacks AVX-512F/BW");
        return;
    }
    let has_vl = std::is_x86_feature_detected!("avx512vl");
    let has_fp16 = std::is_x86_feature_detected!("avx512fp16");
    let operands = [(1u8, 2u8), (9, 10), (17, 18), (25, 26), (31, 31), (2, 2)];
    let masks = [(0u8, false), (1, false), (1, true), (2, false), (3, true)];
    let mut available_controls = 0usize;
    let mut executed = 0usize;

    for level in [
        crate::smir::optimize::OptLevel::O0,
        crate::smir::optimize::OptLevel::O2,
    ] {
        for kind in WidenKind::ALL {
            for (ll, suppress_exceptions) in [(0, false), (1, false), (2, false), (0, true)] {
                let (needs_vl, needs_fp16) = requirements(kind, ll, suppress_exceptions);
                if (needs_vl && !has_vl) || (needs_fp16 && !has_fp16) {
                    continue;
                }
                available_controls += 1;
                for (operand_index, (destination, source)) in operands.into_iter().enumerate() {
                    for (mask, zeroing) in masks {
                        let bytes = encoding(
                            kind,
                            ll,
                            suppress_exceptions,
                            destination,
                            source,
                            mask,
                            zeroing,
                        );
                        // All six exception masks remain set. DAZ is ignored
                        // by these widening conversions; RC and FTZ are varied
                        // to expose accidental host-state coupling.
                        let prior_status = if operand_index % 3 == 0 { 1 << 5 } else { 0 };
                        let rc = ((operand_index as u32 + u32::from(ll)) & 3) << 13;
                        let daz_ftz = if operand_index & 1 == 0 {
                            0
                        } else {
                            (1 << 6) | (1 << 15)
                        };
                        let initial = initial_state(0x1F80 | prior_status | rc | daz_ftz);
                        let interpreted = interpret(&bytes, &initial, level);
                        let native = execute_native(&bytes, &initial, level);
                        assert_eq!(
                            native, interpreted,
                            "level={level:?} {kind:?} {bytes:02X?} operand={operand_index}"
                        );
                        executed += 1;
                    }
                }
            }
        }
    }

    assert!(available_controls > 0, "feature-selected widening controls");
    assert_eq!(executed, available_controls * operands.len() * masks.len());
}
