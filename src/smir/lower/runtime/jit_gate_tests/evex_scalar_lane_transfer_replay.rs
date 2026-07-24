//! Native replay coverage for register-only EVEX scalar lane transfers.

use super::*;
use crate::smir::lower::runtime::*;

const PC: u64 = 0x1013;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WMode {
    Ignored,
    W0,
    W1,
}

impl WMode {
    fn values(self) -> &'static [bool] {
        match self {
            Self::Ignored => &[false, true],
            Self::W0 => &[false],
            Self::W1 => &[true],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GprField {
    None,
    Reg,
    Rm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TransferKind {
    Vextractps,
    Vinsertps,
    Vpextrb,
    Vpextrd,
    Vpextrq,
    VpextrwMap1,
    VpextrwMap3,
    Vpinsrb,
    Vpinsrd,
    Vpinsrq,
    Vpinsrw,
}

impl TransferKind {
    const ALL: [Self; 11] = [
        Self::Vextractps,
        Self::Vinsertps,
        Self::Vpextrb,
        Self::Vpextrd,
        Self::Vpextrq,
        Self::VpextrwMap1,
        Self::VpextrwMap3,
        Self::Vpinsrb,
        Self::Vpinsrd,
        Self::Vpinsrq,
        Self::Vpinsrw,
    ];

    fn fields(self) -> (u8, u8, WMode, bool, bool, GprField) {
        match self {
            Self::Vextractps => (3, 0x17, WMode::Ignored, false, true, GprField::Rm),
            Self::Vinsertps => (3, 0x21, WMode::W0, false, false, GprField::None),
            Self::Vpextrb => (3, 0x14, WMode::Ignored, false, true, GprField::Rm),
            Self::Vpextrd => (3, 0x16, WMode::W0, true, true, GprField::Rm),
            Self::Vpextrq => (3, 0x16, WMode::W1, true, true, GprField::Rm),
            Self::VpextrwMap1 => (1, 0xC5, WMode::Ignored, false, true, GprField::Reg),
            Self::VpextrwMap3 => (3, 0x15, WMode::Ignored, false, true, GprField::Rm),
            Self::Vpinsrb => (3, 0x20, WMode::Ignored, false, false, GprField::Rm),
            Self::Vpinsrd => (3, 0x22, WMode::W0, true, false, GprField::Rm),
            Self::Vpinsrq => (3, 0x22, WMode::W1, true, false, GprField::Rm),
            Self::Vpinsrw => (1, 0xC4, WMode::Ignored, false, false, GprField::Rm),
        }
    }
}

fn encoding(
    kind: TransferKind,
    w: bool,
    destination: u8,
    merge: u8,
    source: u8,
    immediate: u8,
) -> [u8; 7] {
    let (map, opcode, w_mode, _, reserved_vvvv, gpr_field) = kind.fields();
    assert!(w_mode.values().contains(&w));
    assert!(destination < 32 && merge < 32 && source < 32);
    match gpr_field {
        GprField::None => {}
        GprField::Reg => assert!(destination < 16),
        GprField::Rm if reserved_vvvv => assert!(destination < 16),
        GprField::Rm => assert!(source < 16),
    }

    let (reg, rm) = match gpr_field {
        GprField::Reg => (destination, source),
        GprField::Rm if reserved_vvvv => (source, destination),
        GprField::Rm | GprField::None => (destination, source),
    };
    let mut p0 = 0xF0 | map;
    if reg & 0x08 != 0 {
        p0 &= !0x80;
    }
    if reg & 0x10 != 0 {
        p0 &= !0x10;
    }
    if rm & 0x08 != 0 {
        p0 &= !0x20;
    }
    if rm & 0x10 != 0 {
        p0 &= !0x40;
    }
    let (vvvv, v_prime) = if reserved_vvvv {
        (0x78, 0x08)
    } else {
        (((!merge) & 0x0F) << 3, if merge < 16 { 0x08 } else { 0 })
    };

    [
        0x62,
        p0,
        vvvv | 0x04 | 0x01 | if w { 0x80 } else { 0 },
        v_prime,
        opcode,
        0xC0 | ((reg & 0x07) << 3) | (rm & 0x07),
        immediate,
    ]
}

fn operands(kind: TransferKind) -> &'static [(u8, u8, u8)] {
    let (_, _, _, _, reserved_vvvv, gpr_field) = kind.fields();
    match gpr_field {
        GprField::Reg => &[(0, 0, 1), (3, 0, 9), (8, 0, 17), (12, 0, 25), (15, 0, 31)],
        GprField::Rm if reserved_vvvv => {
            &[(0, 0, 1), (3, 0, 9), (8, 0, 17), (12, 0, 25), (15, 0, 31)]
        }
        GprField::Rm => &[
            (1, 2, 0),
            (9, 10, 3),
            (17, 18, 8),
            (25, 26, 12),
            (31, 31, 15),
        ],
        GprField::None => &[
            (1, 17, 1),
            (9, 10, 11),
            (17, 18, 19),
            (25, 26, 27),
            (31, 31, 31),
        ],
    }
}

fn native_immediates(kind: TransferKind) -> Vec<u8> {
    if kind == TransferKind::Vinsertps {
        (u8::MIN..=u8::MAX).collect()
    } else {
        (0..=15).chain([0x40, 0x80, 0xC0, 0xFF]).collect()
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

#[test]
fn replay_feature_aggregation_distinguishes_dq_lane_transfers() {
    for kind in TransferKind::ALL {
        let (_, _, w_mode, needs_dq, _, _) = kind.fields();
        for &w in w_mode.values() {
            let &(destination, merge, source) = operands(kind).last().unwrap();
            let bytes = encoding(kind, w, destination, merge, source, 0xFF);
            let function = function(&bytes);
            let actual = x86_native_replay_feature_requirements(
                &function,
                &std::collections::HashMap::new(),
            );
            assert!(actual.any, "{kind:?} {bytes:02X?}");
            assert!(actual.needs_avx512bw, "{kind:?} {bytes:02X?}");
            assert!(!actual.needs_avx512vl, "{kind:?} {bytes:02X?}");
            assert_eq!(actual.needs_avx512dq, needs_dq, "{kind:?} {bytes:02X?}");
            assert!(!actual.needs_avx512fp16, "{kind:?} {bytes:02X?}");
            assert!(!actual.needs_avx512cd, "{kind:?} {bytes:02X?}");
            assert!(!actual.needs_gfni, "{kind:?} {bytes:02X?}");
            assert!(!actual.needs_avx512vp2intersect, "{kind:?} {bytes:02X?}");
            assert!(!actual.needs_vpclmulqdq, "{kind:?} {bytes:02X?}");

            let excluded = std::collections::HashMap::from([(BlockId(0), PC)]);
            assert_eq!(
                x86_native_replay_feature_requirements(&function, &excluded),
                X86NativeReplayFeatureRequirements::default(),
                "{kind:?} {bytes:02X?}"
            );
        }
    }
}

#[test]
fn replay_admits_and_emits_680_o0_o2_safe_encodings_and_fails_closed() {
    use crate::smir::lower::SmirLowerer;
    use crate::smir::lower::x86_64::X86_64Lowerer;

    let mut lowered = 0usize;
    for kind in TransferKind::ALL {
        let (_, _, w_mode, needs_dq, _, _) = kind.fields();
        for &w in w_mode.values() {
            for &(destination, merge, source) in operands(kind) {
                for immediate in [0, 3, 0x5A, 0xFF] {
                    let bytes = encoding(kind, w, destination, merge, source, immediate);
                    for optimize in [false, true] {
                        let mut function = function(&bytes);
                        if optimize {
                            crate::smir::optimize::optimize_function(
                                &mut function,
                                crate::smir::optimize::OptLevel::O2,
                            );
                        }
                        assert!(is_native_clobber_safe(&function), "{kind:?} {bytes:02X?}");
                        assert!(
                            uses_x86_native_vectors_excluding(
                                &function,
                                &std::collections::HashMap::new()
                            ),
                            "{kind:?} {bytes:02X?}"
                        );

                        #[cfg(target_arch = "x86_64")]
                        let expected_features = std::is_x86_feature_detected!("avx512f")
                            && std::is_x86_feature_detected!("avx512bw")
                            && (!needs_dq || std::is_x86_feature_detected!("avx512dq"));
                        #[cfg(not(target_arch = "x86_64"))]
                        let expected_features = false;
                        assert_eq!(
                            x86_native_vector_features_supported_excluding(
                                &function,
                                &std::collections::HashMap::new()
                            ),
                            expected_features,
                            "{kind:?} {bytes:02X?}"
                        );

                        let mut lowerer = X86_64Lowerer::new();
                        lowerer
                            .lower_function(&function)
                            .unwrap_or_else(|error| panic!("{kind:?} {bytes:02X?}: {error:?}"));
                        let code = lowerer
                            .finalize()
                            .unwrap_or_else(|error| panic!("{kind:?} {bytes:02X?}: {error:?}"));
                        assert!(
                            code.windows(bytes.len()).any(|window| window == bytes),
                            "{kind:?} {bytes:02X?}"
                        );
                        lowered += 1;
                    }
                }
            }
        }
    }
    assert_eq!(lowered, 680);

    for kind in [
        TransferKind::VpextrwMap1,
        TransferKind::Vpinsrw,
        TransferKind::Vinsertps,
    ] {
        let &(destination, merge, source) = operands(kind).first().unwrap();
        let bytes = encoding(
            kind,
            kind.fields().2.values()[0],
            destination,
            merge,
            source,
            0,
        );
        let mut missing = function(&bytes);
        missing.x86_instruction_bytes.clear();
        crate::smir::optimize::optimize_function(&mut missing, crate::smir::optimize::OptLevel::O2);
        assert!(!is_native_clobber_safe(&missing), "{kind:?} {bytes:02X?}");

        let mut memory = bytes;
        memory[5] &= 0x3F;
        let mut malformed = function(&bytes);
        malformed.x86_instruction_bytes.insert(
            (BlockId(0), PC),
            crate::smir::ir::X86InstructionBytes::new(&memory).unwrap(),
        );
        assert!(
            !is_native_clobber_safe(&malformed),
            "{kind:?} {memory:02X?}"
        );
    }

    for kind in TransferKind::ALL {
        let (_, _, w_mode, _, reserved_vvvv, gpr_field) = kind.fields();
        if gpr_field == GprField::None {
            continue;
        }
        for &gpr in &[4, 5] {
            let (destination, merge, source) = if reserved_vvvv {
                (gpr, 0, 1)
            } else {
                (1, 2, gpr)
            };
            let bytes = encoding(kind, w_mode.values()[0], destination, merge, source, 0);
            assert!(
                !is_native_clobber_safe(&function(&bytes)),
                "{kind:?} {bytes:02X?}"
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct TransferState {
    gprs: [u64; 32],
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    rflags: u64,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn initial_state() -> TransferState {
    TransferState {
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
        rflags: 0x2 | 0x8D5,
        mxcsr: 0x1F80 | (2 << 13) | (1 << 6) | (1 << 15),
    }
}

#[cfg(target_arch = "x86_64")]
fn interpret(bytes: &[u8], initial: &TransferState) -> TransferState {
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
    TransferState {
        gprs: x86.gpr,
        vectors,
        masks: x86.k,
        rflags: x86.rflags,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(bytes: &[u8], initial: &TransferState) -> TransferState {
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
    let exec = ExecMem::new(&code).expect("map EVEX scalar-lane-transfer replay");
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
    TransferState {
        gprs: registers.gpr,
        vectors,
        masks: registers.k,
        rflags: registers.rflags,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn run_native_differential(needs_dq: bool) -> usize {
    let initial = initial_state();
    let mut executed = 0usize;
    for kind in TransferKind::ALL {
        let (_, _, w_mode, kind_needs_dq, _, _) = kind.fields();
        if kind_needs_dq != needs_dq {
            continue;
        }
        for &w in w_mode.values() {
            for &(destination, merge, source) in operands(kind) {
                for immediate in native_immediates(kind) {
                    let bytes = encoding(kind, w, destination, merge, source, immediate);
                    assert_eq!(
                        execute_native(&bytes, &initial),
                        interpret(&bytes, &initial),
                        "{kind:?} {bytes:02X?}"
                    );
                    executed += 1;
                }
            }
        }
    }
    executed
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_interpreter_for_bw_f_lane_controls_extensions_aliases_and_full_state() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX scalar lane-transfer differential: host lacks AVX-512F/BW");
        return;
    }
    assert_eq!(run_native_differential(false), 2_480);
}

#[cfg(target_arch = "x86_64")]
#[test]
fn replay_matches_interpreter_for_dq_lane_controls_extensions_aliases_and_full_state() {
    if !std::is_x86_feature_detected!("avx512f")
        || !std::is_x86_feature_detected!("avx512bw")
        || !std::is_x86_feature_detected!("avx512dq")
    {
        eprintln!(
            "skipping native EVEX DQ scalar lane-transfer differential: host lacks AVX-512F/BW/DQ"
        );
        return;
    }
    assert_eq!(run_native_differential(true), 400);
}
