//! Exact native replay for the residual EVEX FMA3 register control domain.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, X86FmaOp};
use crate::smir::ir::types::{BlockId, FpRoundMode, FunctionId, SourceArch};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86InstructionBytes, x86_native_replay_spans,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    X86NativeReplayFeatureRequirements, is_native_clobber_safe, uses_x86_native_vectors_excluding,
    x86_native_replay_feature_requirements,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::{OptLevel, optimize_function};

const PC: u64 = 0xF3E0;
const PACKED_OPCODES: [u8; 18] = [
    0x96, 0x97, 0x98, 0x9A, 0x9C, 0x9E, 0xA6, 0xA7, 0xA8, 0xAA, 0xAC, 0xAE, 0xB6, 0xB7, 0xB8, 0xBA,
    0xBC, 0xBE,
];
const SCALAR_OPCODES: [u8; 12] = [
    0x99, 0x9B, 0x9D, 0x9F, 0xA9, 0xAB, 0xAD, 0xAF, 0xB9, 0xBB, 0xBD, 0xBF,
];
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const SCANNER_SOURCES: [u8; 3] = [0, 1, 15];
const SCANNER_MASKS: [(u8, bool); 3] = [(0, false), (1, false), (1, true)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FmaFormat {
    F16,
    F32,
    F64,
}

impl FmaFormat {
    const ALL: [Self; 3] = [Self::F16, Self::F32, Self::F64];

    const fn map(self) -> u8 {
        match self {
            Self::F16 => 6,
            Self::F32 | Self::F64 => 2,
        }
    }

    const fn w(self) -> bool {
        matches!(self, Self::F64)
    }

    const fn needs_fp16(self) -> bool {
        matches!(self, Self::F16)
    }

    const fn packed_lanes(self) -> u8 {
        match self {
            Self::F16 => 32,
            Self::F32 => 16,
            Self::F64 => 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FmaCase {
    format: FmaFormat,
    scalar: bool,
    opcode: u8,
    destination: u8,
    source1: u8,
    source2: u8,
    ll: u8,
    mask: u8,
    zeroing: bool,
    embedded_rounding: bool,
}

impl FmaCase {
    fn bytes(self) -> [u8; 6] {
        assert!(self.destination < 32 && self.source1 < 32 && self.source2 < 32);
        assert!(self.ll < 4 && self.mask < 8 && (!self.zeroing || self.mask != 0));
        assert!(if self.scalar {
            SCALAR_OPCODES.contains(&self.opcode)
        } else {
            PACKED_OPCODES.contains(&self.opcode)
        });
        assert!(self.embedded_rounding || self.scalar || self.ll < 3);

        [
            0x62,
            (if self.destination & 0x08 == 0 {
                0x80
            } else {
                0
            }) | (if self.source2 & 0x10 == 0 { 0x40 } else { 0 })
                | (if self.source2 & 0x08 == 0 { 0x20 } else { 0 })
                | (if self.destination & 0x10 == 0 {
                    0x10
                } else {
                    0
                })
                | self.format.map(),
            (u8::from(self.format.w()) << 7) | (((!self.source1) & 0x0F) << 3) | 0x05,
            (u8::from(self.zeroing) << 7)
                | (self.ll << 5)
                | (u8::from(self.embedded_rounding) << 4)
                | (if self.source1 & 0x10 == 0 { 0x08 } else { 0 })
                | self.mask,
            self.opcode,
            0xC0 | ((self.destination & 0x07) << 3) | (self.source2 & 0x07),
        ]
    }

    fn classify(self) -> Option<bool> {
        let bytes = X86InstructionBytes::new(&self.bytes()).unwrap();
        match (self.format, self.scalar) {
            (FmaFormat::F16, false) => bytes.evex_register_packed_fp16_fma_needs_vl(),
            (FmaFormat::F16, true) => bytes.evex_register_scalar_fp16_fma_needs_vl(),
            (FmaFormat::F32 | FmaFormat::F64, false) => bytes.evex_register_packed_fma_needs_vl(),
            (FmaFormat::F32 | FmaFormat::F64, true) => bytes.evex_register_scalar_fma_needs_vl(),
        }
    }

    const fn round(self) -> FpRoundMode {
        if !self.embedded_rounding {
            return FpRoundMode::Dynamic;
        }
        match self.ll {
            0 => FpRoundMode::RoundNearest,
            1 => FpRoundMode::RoundDown,
            2 => FpRoundMode::RoundUp,
            3 => FpRoundMode::RoundTowardZero,
            _ => unreachable!(),
        }
    }
}

fn function(bytes: &[u8; 6]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
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
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(bytes).unwrap());
    function
}

fn optimized_function(bytes: &[u8; 6], level: OptLevel) -> SmirFunction {
    let mut function = function(bytes);
    optimize_function(&mut function, level);
    function
}

fn opcodes(scalar: bool) -> &'static [u8] {
    if scalar {
        &SCALAR_OPCODES
    } else {
        &PACKED_OPCODES
    }
}

#[test]
fn classifier_accepts_all_2_949_120_register_extension_cells() {
    let mut accepted = 0usize;
    for format in FmaFormat::ALL {
        for scalar in [false, true] {
            for &opcode in opcodes(scalar) {
                for destination in 0..32 {
                    for source1 in 0..32 {
                        for source2 in 0..32 {
                            let case = FmaCase {
                                format,
                                scalar,
                                opcode,
                                destination,
                                source1,
                                source2,
                                ll: 0,
                                mask: 0,
                                zeroing: false,
                                embedded_rounding: true,
                            };
                            assert_eq!(case.classify(), Some(false), "{case:?}");
                            accepted += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(accepted, 90 * 32 * 32 * 32);
}

#[test]
fn classifier_accepts_every_defined_control_and_fails_closed() {
    let mut embedded = 0usize;
    let mut dynamic = 0usize;
    for format in FmaFormat::ALL {
        for scalar in [false, true] {
            for &opcode in opcodes(scalar) {
                for ll in 0..4 {
                    for mask in 0..8 {
                        for zeroing in [false, true] {
                            if zeroing && mask == 0 {
                                continue;
                            }
                            let case = FmaCase {
                                format,
                                scalar,
                                opcode,
                                destination: 17,
                                source1: 18,
                                source2: 19,
                                ll,
                                mask,
                                zeroing,
                                embedded_rounding: true,
                            };
                            assert_eq!(case.classify(), Some(false), "{case:?}");
                            embedded += 1;
                        }
                    }
                }

                for ll in 0..if scalar { 4 } else { 3 } {
                    for mask in 0..8 {
                        for zeroing in [false, true] {
                            if zeroing && mask == 0 {
                                continue;
                            }
                            let case = FmaCase {
                                format,
                                scalar,
                                opcode,
                                destination: 17,
                                source1: 18,
                                source2: 19,
                                ll,
                                mask,
                                zeroing,
                                embedded_rounding: false,
                            };
                            assert_eq!(
                                case.classify(),
                                Some(if scalar { false } else { ll < 2 }),
                                "{case:?}"
                            );
                            dynamic += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(embedded, 90 * 4 * 15);
    assert_eq!(dynamic, (54 * 3 + 36 * 4) * 15);

    for format in FmaFormat::ALL {
        for scalar in [false, true] {
            for &opcode in opcodes(scalar) {
                let base = FmaCase {
                    format,
                    scalar,
                    opcode,
                    destination: 17,
                    source1: 18,
                    source2: 19,
                    ll: 0,
                    mask: 0,
                    zeroing: false,
                    embedded_rounding: true,
                };

                let mut zeroing_k0 = base.bytes();
                zeroing_k0[3] |= 0x80;
                assert_eq!(
                    match (format, scalar) {
                        (FmaFormat::F16, false) => X86InstructionBytes::new(&zeroing_k0)
                            .unwrap()
                            .evex_register_packed_fp16_fma_needs_vl(),
                        (FmaFormat::F16, true) => X86InstructionBytes::new(&zeroing_k0)
                            .unwrap()
                            .evex_register_scalar_fp16_fma_needs_vl(),
                        (_, false) => X86InstructionBytes::new(&zeroing_k0)
                            .unwrap()
                            .evex_register_packed_fma_needs_vl(),
                        (_, true) => X86InstructionBytes::new(&zeroing_k0)
                            .unwrap()
                            .evex_register_scalar_fma_needs_vl(),
                    },
                    None,
                    "{zeroing_k0:02X?}"
                );

                let mut memory = base.bytes();
                memory[5] &= 0x3F;
                let instruction = X86InstructionBytes::new(&memory).unwrap();
                assert_eq!(
                    match (format, scalar) {
                        (FmaFormat::F16, false) => {
                            instruction.evex_register_packed_fp16_fma_needs_vl()
                        }
                        (FmaFormat::F16, true) => {
                            instruction.evex_register_scalar_fp16_fma_needs_vl()
                        }
                        (_, false) => instruction.evex_register_packed_fma_needs_vl(),
                        (_, true) => instruction.evex_register_scalar_fma_needs_vl(),
                    },
                    None,
                    "{memory:02X?}"
                );
            }
        }
    }

    for format in FmaFormat::ALL {
        for scalar in [false, true] {
            let case = FmaCase {
                format,
                scalar,
                opcode: opcodes(scalar)[0],
                destination: 17,
                source1: 18,
                source2: 19,
                ll: 2,
                mask: 1,
                zeroing: true,
                embedded_rounding: true,
            };
            for mutate in [
                |bytes: &mut [u8; 6]| bytes[1] = (bytes[1] & 0xF0) | 1,
                |bytes: &mut [u8; 6]| bytes[2] &= !0x04,
                |bytes: &mut [u8; 6]| bytes[2] &= !0x03,
            ] {
                let mut bytes = case.bytes();
                mutate(&mut bytes);
                let instruction = X86InstructionBytes::new(&bytes).unwrap();
                assert!(
                    instruction.evex_register_packed_fma_needs_vl().is_none()
                        && instruction.evex_register_scalar_fma_needs_vl().is_none()
                        && instruction
                            .evex_register_packed_fp16_fma_needs_vl()
                            .is_none()
                        && instruction
                            .evex_register_scalar_fp16_fma_needs_vl()
                            .is_none(),
                    "{bytes:02X?}"
                );
            }
        }
    }
}

#[test]
fn six_encodings_match_independent_llvm_23_anchors() {
    let anchors = [
        (
            FmaCase {
                format: FmaFormat::F64,
                scalar: false,
                opcode: 0x98,
                destination: 17,
                source1: 18,
                source2: 19,
                ll: 0,
                mask: 1,
                zeroing: true,
                embedded_rounding: true,
            },
            [0x62, 0xA2, 0xED, 0x91, 0x98, 0xCB],
        ),
        (
            FmaCase {
                format: FmaFormat::F32,
                scalar: false,
                opcode: 0xA7,
                destination: 20,
                source1: 21,
                source2: 22,
                ll: 1,
                mask: 2,
                zeroing: false,
                embedded_rounding: true,
            },
            [0x62, 0xA2, 0x55, 0x32, 0xA7, 0xE6],
        ),
        (
            FmaCase {
                format: FmaFormat::F16,
                scalar: false,
                opcode: 0xBC,
                destination: 23,
                source1: 24,
                source2: 25,
                ll: 2,
                mask: 3,
                zeroing: true,
                embedded_rounding: true,
            },
            [0x62, 0x86, 0x3D, 0xD3, 0xBC, 0xF9],
        ),
        (
            FmaCase {
                format: FmaFormat::F64,
                scalar: true,
                opcode: 0x9F,
                destination: 26,
                source1: 27,
                source2: 28,
                ll: 3,
                mask: 4,
                zeroing: false,
                embedded_rounding: true,
            },
            [0x62, 0x02, 0xA5, 0x74, 0x9F, 0xD4],
        ),
        (
            FmaCase {
                format: FmaFormat::F32,
                scalar: true,
                opcode: 0xAB,
                destination: 29,
                source1: 30,
                source2: 31,
                ll: 1,
                mask: 5,
                zeroing: true,
                embedded_rounding: true,
            },
            [0x62, 0x02, 0x0D, 0xB5, 0xAB, 0xEF],
        ),
        (
            FmaCase {
                format: FmaFormat::F16,
                scalar: true,
                opcode: 0xB9,
                destination: 17,
                source1: 18,
                source2: 19,
                ll: 2,
                mask: 6,
                zeroing: false,
                embedded_rounding: true,
            },
            [0x62, 0xA6, 0x6D, 0x56, 0xB9, 0xCB],
        ),
    ];
    for (case, expected) in anchors {
        assert_eq!(case.bytes(), expected, "{case:?}");
    }
}

#[test]
fn lift_graphs_preserve_rounding_width_mask_and_fp_format() {
    for format in FmaFormat::ALL {
        for scalar in [false, true] {
            for ll in 0..4 {
                let case = FmaCase {
                    format,
                    scalar,
                    opcode: opcodes(scalar)[0],
                    destination: 17,
                    source1: 18,
                    source2: 19,
                    ll,
                    mask: 3,
                    zeroing: true,
                    embedded_rounding: true,
                };
                let function = function(&case.bytes());
                let fma = function.blocks[0]
                    .ops
                    .iter()
                    .find(|op| matches!(op.kind, OpKind::X86Fma(_) | OpKind::X86FP16Fma { .. }))
                    .unwrap_or_else(|| panic!("{case:?}: {:#?}", function.blocks[0].ops));
                match (&fma.kind, format) {
                    (
                        OpKind::X86Fma(X86FmaOp { round, lanes, .. }),
                        FmaFormat::F32 | FmaFormat::F64,
                    ) => {
                        assert_eq!(*round, case.round(), "{case:?}");
                        assert_eq!(
                            *lanes,
                            if scalar { 1 } else { format.packed_lanes() },
                            "{case:?}"
                        );
                    }
                    (
                        OpKind::X86FP16Fma {
                            round, lanes, mask, ..
                        },
                        FmaFormat::F16,
                    ) => {
                        assert_eq!(*round, case.round(), "{case:?}");
                        assert_eq!(*lanes, if scalar { 1 } else { 32 }, "{case:?}");
                        assert!(mask.is_some(), "{case:?}");
                    }
                    (other, _) => panic!("{case:?}: unexpected FMA operation {other:#?}"),
                }
                assert!(
                    function.blocks[0]
                        .ops
                        .iter()
                        .all(|op| !op.kind.reads_memory() && !op.kind.writes_memory()),
                    "{case:?}"
                );
            }

            if scalar {
                for ll in 0..3 {
                    let case = FmaCase {
                        format,
                        scalar: true,
                        opcode: SCALAR_OPCODES[11],
                        destination: 17,
                        source1: 18,
                        source2: 19,
                        ll,
                        mask: 1,
                        zeroing: false,
                        embedded_rounding: false,
                    };
                    let function = function(&case.bytes());
                    assert!(function.blocks[0].ops.iter().any(|op| matches!(
                        op.kind,
                        OpKind::X86Fma(X86FmaOp {
                            round: FpRoundMode::Dynamic,
                            lanes: 1,
                            ..
                        }) | OpKind::X86FP16Fma {
                            round: FpRoundMode::Dynamic,
                            lanes: 1,
                            ..
                        }
                    )));
                }
            }
        }
    }
}

fn assert_admits_and_lowers(case: FmaCase) -> usize {
    let bytes = case.bytes();
    let mut replay = bytes;
    if case.scalar && !case.embedded_rounding {
        replay[3] &= !0x60;
    }
    let mut lowered = 0usize;
    for level in LEVELS {
        let function = optimized_function(&bytes, level);
        let spans = x86_native_replay_spans(&function.blocks[0], &function.x86_instruction_bytes);
        let span = spans
            .get(&0)
            .unwrap_or_else(|| panic!("{level:?} {case:?}: {:#?}", function.blocks[0].ops));
        assert_eq!(span.end, function.blocks[0].ops.len(), "{level:?} {case:?}");
        assert_eq!(span.instruction.as_slice(), replay, "{level:?} {case:?}");
        assert!(!span.needs_avx512vl, "{level:?} {case:?}");
        assert_eq!(
            span.needs_avx512fp16,
            case.format.needs_fp16(),
            "{level:?} {case:?}"
        );
        assert!(is_native_clobber_safe(&function), "{level:?} {case:?}");
        assert!(
            uses_x86_native_vectors_excluding(&function, &HashMap::new()),
            "{level:?} {case:?}"
        );

        let mut lowerer = X86_64Lowerer::new();
        lowerer
            .lower_function(&function)
            .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
        let code = lowerer
            .finalize()
            .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
        assert!(
            code.windows(replay.len()).any(|window| window == replay),
            "{level:?} {case:?}"
        );
        lowered += 1;
    }
    lowered
}

#[test]
fn all_3_888_residual_scanner_cells_admit_and_lower_at_o0_o1_o2() {
    let mut admitted = 0usize;
    let mut lowered = 0usize;
    for format in FmaFormat::ALL {
        for scalar in [false, true] {
            for &opcode in opcodes(scalar) {
                for source1 in SCANNER_SOURCES {
                    for ll in 0..4 {
                        for (mask, zeroing) in SCANNER_MASKS {
                            let case = FmaCase {
                                format,
                                scalar,
                                opcode,
                                destination: 0,
                                source1,
                                source2: 2,
                                ll,
                                mask,
                                zeroing,
                                embedded_rounding: true,
                            };
                            assert_eq!(case.classify(), Some(false), "{case:?}");
                            lowered += assert_admits_and_lowers(case);
                            admitted += 1;
                        }
                    }

                    if scalar {
                        for ll in [1, 2] {
                            for (mask, zeroing) in SCANNER_MASKS {
                                let case = FmaCase {
                                    format,
                                    scalar: true,
                                    opcode,
                                    destination: 0,
                                    source1,
                                    source2: 2,
                                    ll,
                                    mask,
                                    zeroing,
                                    embedded_rounding: false,
                                };
                                assert_eq!(case.classify(), Some(false), "{case:?}");
                                lowered += assert_admits_and_lowers(case);
                                admitted += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(admitted, 3_888);
    assert_eq!(lowered, 3_888 * LEVELS.len());
}

#[test]
fn all_324_llig3_scalar_cells_admit_and_lower_at_o0_o1_o2() {
    let mut admitted = 0usize;
    let mut lowered = 0usize;
    for format in FmaFormat::ALL {
        for opcode in SCALAR_OPCODES {
            for source1 in SCANNER_SOURCES {
                for (mask, zeroing) in SCANNER_MASKS {
                    let case = FmaCase {
                        format,
                        scalar: true,
                        opcode,
                        destination: 0,
                        source1,
                        source2: 2,
                        ll: 3,
                        mask,
                        zeroing,
                        embedded_rounding: false,
                    };
                    assert_eq!(case.classify(), Some(false), "{case:?}");
                    lowered += assert_admits_and_lowers(case);
                    admitted += 1;
                }
            }
        }
    }
    assert_eq!(admitted, 324);
    assert_eq!(lowered, 324 * LEVELS.len());
}

#[test]
fn replay_feature_requirements_and_provenance_are_exact_and_fail_closed() {
    for format in FmaFormat::ALL {
        let case = FmaCase {
            format,
            scalar: false,
            opcode: PACKED_OPCODES[17],
            destination: 17,
            source1: 18,
            source2: 19,
            ll: 3,
            mask: 1,
            zeroing: true,
            embedded_rounding: true,
        };
        let function = function(&case.bytes());
        let requirements = x86_native_replay_feature_requirements(&function, &HashMap::new());
        assert!(requirements.any, "{case:?}");
        assert!(requirements.needs_avx512bw, "{case:?}");
        assert!(!requirements.needs_avx512vl, "{case:?}");
        assert!(!requirements.needs_avx512dq, "{case:?}");
        assert_eq!(
            requirements.needs_avx512fp16,
            format.needs_fp16(),
            "{case:?}"
        );
        assert!(!requirements.needs_fma, "{case:?}");

        let excluded = HashMap::from([(BlockId(0), PC)]);
        assert_eq!(
            x86_native_replay_feature_requirements(&function, &excluded),
            X86NativeReplayFeatureRequirements::default(),
            "{case:?}"
        );

        let mut missing = function.clone();
        missing.x86_instruction_bytes.clear();
        assert!(!is_native_clobber_safe(&missing), "{case:?}");

        let mut memory = case.bytes();
        memory[5] &= 0x3F;
        let mut mismatched = function.clone();
        mismatched
            .x86_instruction_bytes
            .insert((BlockId(0), PC), X86InstructionBytes::new(&memory).unwrap());
        assert!(!is_native_clobber_safe(&mismatched), "{case:?}");
    }

    for format in FmaFormat::ALL {
        let case = FmaCase {
            format,
            scalar: false,
            opcode: PACKED_OPCODES[0],
            destination: 17,
            source1: 18,
            source2: 19,
            ll: 2,
            mask: 1,
            zeroing: false,
            embedded_rounding: false,
        };
        let mut invalid_ll = case.bytes();
        invalid_ll[3] = (invalid_ll[3] & !0x60) | 0x60;
        let instruction = X86InstructionBytes::new(&invalid_ll).unwrap();
        let classification = match format {
            FmaFormat::F16 => instruction.evex_register_packed_fp16_fma_needs_vl(),
            FmaFormat::F32 | FmaFormat::F64 => instruction.evex_register_packed_fma_needs_vl(),
        };
        assert_eq!(classification, None, "{invalid_ll:02X?}");

        let mut lifter = X86_64Lifter::strict();
        let mut context = LiftContext::new(SourceArch::X86_64);
        assert!(
            lifter.lift_insn(PC, &invalid_ll, &mut context).is_err(),
            "{invalid_ll:02X?}"
        );
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct FmaState {
    vectors: [[u64; 8]; 32],
    masks: [u64; 8],
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn patterned_vector(format: FmaFormat, register: usize) -> [u64; 8] {
    const F64_VALUES: [u64; 8] = [
        0x3FF0_0000_0000_0000,
        0xBFF0_0000_0000_0000,
        0x3FF8_0000_0000_0000,
        0xC004_0000_0000_0000,
        0x0010_0000_0000_0000,
        0x7FEF_FFFF_FFFF_FFFF,
        0,
        0x8000_0000_0000_0000,
    ];
    const F32_VALUES: [u32; 16] = [
        0x3F80_0000,
        0xBF80_0000,
        0x3FC0_0000,
        0xC020_0000,
        0x0080_0000,
        0x7F7F_FFFF,
        0,
        0x8000_0000,
        0x3F00_0000,
        0x4000_0000,
        0x4040_0000,
        0xC080_0000,
        0x3F80_0001,
        0x3F7F_FFFF,
        0x3380_0000,
        0xB380_0000,
    ];
    const F16_VALUES: [u16; 16] = [
        0x3C00, 0xBC00, 0x3E00, 0xC100, 0x0400, 0x7BFF, 0, 0x8000, 0x3800, 0x4000, 0x4200, 0xC400,
        0x3C01, 0x3BFF, 0x0001, 0x8001,
    ];

    let mut bytes = [0u8; 64];
    match format {
        FmaFormat::F64 => {
            for lane in 0..8 {
                let value = F64_VALUES[(lane + register * 3) % F64_VALUES.len()];
                bytes[lane * 8..lane * 8 + 8].copy_from_slice(&value.to_le_bytes());
            }
        }
        FmaFormat::F32 => {
            for lane in 0..16 {
                let value = F32_VALUES[(lane + register * 5) % F32_VALUES.len()];
                bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        FmaFormat::F16 => {
            for lane in 0..32 {
                let value = F16_VALUES[(lane + register * 7) % F16_VALUES.len()];
                bytes[lane * 2..lane * 2 + 2].copy_from_slice(&value.to_le_bytes());
            }
        }
    }
    std::array::from_fn(|word| {
        u64::from_le_bytes(bytes[word * 8..word * 8 + 8].try_into().unwrap())
    })
}

#[cfg(target_arch = "x86_64")]
fn initial_state(format: FmaFormat, mxcsr: u32) -> FmaState {
    let mut masks = [0u64; 8];
    masks[1] = 0xA5A5_A5A5_A5A5_A5A5;
    masks[2] = 0;
    masks[3] = u64::MAX;
    FmaState {
        vectors: std::array::from_fn(|register| patterned_vector(format, register)),
        masks,
        mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn interpret(case: FmaCase, initial: &FmaState, level: OptLevel) -> FmaState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::TrapKind;
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::memory::FlatMemory;

    let mut function = function(&case.bytes());
    function.blocks[0].set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    optimize_function(&mut function, level);

    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        for (register, value) in initial.vectors.iter().enumerate() {
            x86.xmm[register][..8].copy_from_slice(value);
        }
        x86.k = initial.masks;
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
    for (register, value) in vectors.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[register][..8]);
    }
    FmaState {
        vectors,
        masks: x86.k,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn execute_native(case: FmaCase, initial: &FmaState, level: OptLevel) -> FmaState {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let function = optimized_function(&case.bytes(), level);
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
    let mut replay = case.bytes();
    if case.scalar && !case.embedded_rounding {
        replay[3] &= !0x60;
    }
    assert!(code.windows(replay.len()).any(|window| window == replay));
    let executable = ExecMem::new(&code).expect("map EVEX FMA3 register replay");

    let mut registers = GuestRegs {
        vector_active: 1,
        mxcsr: initial.mxcsr,
        ..GuestRegs::default()
    };
    for (register, value) in initial.vectors.iter().enumerate() {
        registers.set_zmm(register, *value);
    }
    registers.k = initial.masks;
    executable.run(lowered.entry_offset, &mut registers);

    FmaState {
        vectors: std::array::from_fn(|register| registers.get_zmm(register)),
        masks: registers.k,
        mxcsr: registers.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_replay_matches_interpreter_for_rounding_sae_masks_aliases_and_llig() {
    if !std::is_x86_feature_detected!("avx512f") || !std::is_x86_feature_detected!("avx512bw") {
        eprintln!("skipping native EVEX FMA3 differential: host lacks AVX-512F/BW state");
        return;
    }

    let base_cases = [
        FmaCase {
            format: FmaFormat::F64,
            scalar: false,
            opcode: 0x98,
            destination: 17,
            source1: 18,
            source2: 19,
            ll: 0,
            mask: 1,
            zeroing: false,
            embedded_rounding: true,
        },
        FmaCase {
            format: FmaFormat::F32,
            scalar: false,
            opcode: 0xA7,
            destination: 20,
            source1: 20,
            source2: 21,
            ll: 1,
            mask: 1,
            zeroing: true,
            embedded_rounding: true,
        },
        FmaCase {
            format: FmaFormat::F64,
            scalar: true,
            opcode: 0x9F,
            destination: 22,
            source1: 23,
            source2: 22,
            ll: 3,
            mask: 1,
            zeroing: false,
            embedded_rounding: true,
        },
        FmaCase {
            format: FmaFormat::F32,
            scalar: true,
            opcode: 0xAB,
            destination: 24,
            source1: 24,
            source2: 25,
            ll: 2,
            mask: 1,
            zeroing: true,
            embedded_rounding: true,
        },
        FmaCase {
            format: FmaFormat::F64,
            scalar: true,
            opcode: 0xB9,
            destination: 26,
            source1: 27,
            source2: 28,
            ll: 1,
            mask: 1,
            zeroing: false,
            embedded_rounding: false,
        },
        FmaCase {
            format: FmaFormat::F32,
            scalar: true,
            opcode: 0x9D,
            destination: 29,
            source1: 30,
            source2: 31,
            ll: 3,
            mask: 1,
            zeroing: false,
            embedded_rounding: false,
        },
        FmaCase {
            format: FmaFormat::F64,
            scalar: true,
            opcode: 0xAF,
            destination: 17,
            source1: 18,
            source2: 19,
            ll: 2,
            mask: 2,
            zeroing: false,
            embedded_rounding: false,
        },
    ];

    for case in base_cases {
        let mxcsr = if case.embedded_rounding {
            0x1F80 | 0x25
        } else {
            0x1F80 | (u32::from(case.ll & 0x03) << 13)
        };
        let initial = initial_state(case.format, mxcsr);
        for level in LEVELS {
            let interpreted = interpret(case, &initial, level);
            let native = execute_native(case, &initial, level);
            assert_eq!(native, interpreted, "{level:?} {case:?}");
            if case.embedded_rounding || case.mask == 2 {
                assert_eq!(interpreted.mxcsr, initial.mxcsr, "{level:?} {case:?}");
            }
        }
    }

    if !std::is_x86_feature_detected!("avx512fp16") {
        eprintln!("skipping FP16 FMA3 differential subset: host lacks AVX-512-FP16");
        return;
    }

    for case in [
        FmaCase {
            format: FmaFormat::F16,
            scalar: false,
            opcode: 0xBC,
            destination: 17,
            source1: 18,
            source2: 19,
            ll: 2,
            mask: 1,
            zeroing: true,
            embedded_rounding: true,
        },
        FmaCase {
            format: FmaFormat::F16,
            scalar: true,
            opcode: 0xB9,
            destination: 20,
            source1: 21,
            source2: 20,
            ll: 3,
            mask: 1,
            zeroing: false,
            embedded_rounding: true,
        },
        FmaCase {
            format: FmaFormat::F16,
            scalar: true,
            opcode: 0x9D,
            destination: 22,
            source1: 22,
            source2: 23,
            ll: 3,
            mask: 1,
            zeroing: false,
            embedded_rounding: false,
        },
    ] {
        let initial = initial_state(case.format, 0x1F80 | 0x15);
        for level in LEVELS {
            let interpreted = interpret(case, &initial, level);
            let native = execute_native(case, &initial, level);
            assert_eq!(native, interpreted, "{level:?} {case:?}");
            if case.embedded_rounding {
                assert_eq!(interpreted.mxcsr, initial.mxcsr, "{level:?} {case:?}");
            }
        }
    }
}
