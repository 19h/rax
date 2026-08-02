//! Exhaustive native admission and differential coverage for VEX/APX `MULX`.

use super::*;
use crate::smir::ir::ops::SmirOp;
use crate::smir::ir::types::OpId;
use crate::smir::ir::{SmirBlock, SmirFunction, TrapKind};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::aarch64::Aarch64Lowerer;
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xB180;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const VALUE_PAIRS: [(u64, u64); 8] = [
    (0, 0),
    (0, u64::MAX),
    (1, u64::MAX),
    (0xFFFF_FFFF, 0xFFFF_FFFF),
    (0x1_0000_0000, 0x1_0000_0000),
    (0x8000_0000_0000_0000, 2),
    (0xFEDC_BA98_7654_3210, 0x0123_4567_89AB_CDEF),
    (u64::MAX, u64::MAX),
];
const VEX_TUPLES: [(u8, u8, u8); 20] = [
    (0, 1, 3),
    (8, 9, 10),
    (15, 14, 13),
    (4, 1, 3),
    (1, 4, 3),
    (1, 3, 4),
    (5, 1, 3),
    (1, 5, 3),
    (1, 3, 5),
    (4, 4, 3),
    (5, 5, 3),
    (4, 5, 4),
    (5, 4, 5),
    (2, 1, 3),
    (1, 2, 3),
    (1, 3, 2),
    (2, 2, 2),
    (8, 8, 10),
    (8, 9, 8),
    (8, 9, 9),
];
const APX_TUPLES: [(u8, u8, u8); 14] = [
    (0, 1, 3),
    (4, 5, 3),
    (16, 17, 18),
    (31, 30, 29),
    (16, 16, 16),
    (31, 31, 31),
    (16, 17, 2),
    (2, 16, 17),
    (17, 2, 16),
    (4, 16, 5),
    (16, 5, 4),
    (5, 4, 16),
    (2, 31, 2),
    (31, 2, 31),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncodingKind {
    Vex,
    Apx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MulxCase {
    encoding: EncodingKind,
    width: OpWidth,
    dst_lo: u8,
    dst_hi: u8,
    src2: u8,
}

impl MulxCase {
    fn bytes(self) -> Vec<u8> {
        match self.encoding {
            EncodingKind::Vex => {
                assert!(self.dst_lo < 16 && self.dst_hi < 16 && self.src2 < 16);
                let mut p0 = 0xE2;
                if self.dst_hi >= 8 {
                    p0 &= !0x80;
                }
                if self.src2 >= 8 {
                    p0 &= !0x20;
                }
                vec![
                    0xC4,
                    p0,
                    (u8::from(self.width == OpWidth::W64) << 7)
                        | (((!self.dst_lo) & 0x0F) << 3)
                        | 0x03,
                    0xF6,
                    0xC0 | ((self.dst_hi & 7) << 3) | (self.src2 & 7),
                ]
            }
            EncodingKind::Apx => {
                assert!(self.dst_lo < 32 && self.dst_hi < 32 && self.src2 < 32);
                let mut p0 = 0x42; // X3=1 and map 2.
                if self.dst_hi & 8 == 0 {
                    p0 |= 0x80;
                }
                if self.dst_hi & 16 == 0 {
                    p0 |= 0x10;
                }
                if self.src2 & 8 == 0 {
                    p0 |= 0x20;
                }
                if self.src2 & 16 != 0 {
                    p0 |= 0x08;
                }
                vec![
                    0x62,
                    p0,
                    (u8::from(self.width == OpWidth::W64) << 7)
                        | (((!self.dst_lo) & 0x0F) << 3)
                        | 0x07, // U=1 for ModRM.Mod=3 and mandatory F2.
                    if self.dst_lo < 16 { 0x08 } else { 0x00 },
                    0xF6,
                    0xC0 | ((self.dst_hi & 7) << 3) | (self.src2 & 7),
                ]
            }
        }
    }

    fn is_legacy(self) -> bool {
        [self.dst_lo, self.dst_hi, self.src2]
            .into_iter()
            .all(|index| index < 16)
    }

    fn needs_x86_state_bridge(self) -> bool {
        [self.dst_lo, self.dst_hi, self.src2]
            .into_iter()
            .any(|index| index >= 16 || matches!(index, 4 | 5))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MulxState {
    gpr: [u64; 32],
    rflags: u64,
}

fn lift(bytes: &[u8]) -> crate::smir::lift::LiftResult {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"))
}

fn function_from_ops(ops: Vec<SmirOp>, halt: bool) -> SmirFunction {
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = ops;
    block.set_terminator(if halt {
        Terminator::Trap {
            kind: TrapKind::Halt,
        }
    } else {
        Terminator::Return { values: Vec::new() }
    });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
}

fn function(case: MulxCase, halt: bool) -> SmirFunction {
    let bytes = case.bytes();
    let result = lift(&bytes);
    assert_eq!(result.bytes_consumed, bytes.len(), "{case:?} {bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert_exact_mulx(&result.ops, case);
    function_from_ops(result.ops, halt)
}

fn optimized_function(case: MulxCase, level: OptLevel, halt: bool) -> SmirFunction {
    let mut function = function(case, halt);
    crate::smir::optimize::optimize_function(&mut function, level);
    assert_exact_mulx(&function.blocks[0].ops, case);
    function
}

fn assert_exact_mulx(ops: &[SmirOp], case: MulxCase) {
    let ops = match case.encoding {
        EncodingKind::Vex => {
            assert!(
                !matches!(
                    ops.first(),
                    Some(SmirOp {
                        kind: OpKind::X86RequireApx,
                        ..
                    })
                ),
                "{case:?}: VEX form has an APX requirement"
            );
            ops
        }
        EncodingKind::Apx => {
            assert!(
                matches!(
                    ops.first(),
                    Some(SmirOp {
                        kind: OpKind::X86RequireApx,
                        ..
                    })
                ),
                "{case:?}: APX form lacks its dynamic requirement"
            );
            &ops[1..]
        }
    };
    let [op] = ops else {
        panic!("{case:?}: expected one MULX operation, got {ops:?}")
    };
    assert_eq!(op.x86_hint, Some(X86OpHint::Mulx), "{case:?}");
    assert!(matches!(
        &op.kind,
        OpKind::MulU {
            dst_lo,
            dst_hi: Some(dst_hi),
            src1,
            src2: SrcOperand::Reg(src2),
            width,
            flags: FlagUpdate::None,
        } if *dst_lo == x86(X86Reg::gpr(case.dst_lo))
            && *dst_hi == x86(X86Reg::gpr(case.dst_hi))
            && *src1 == x86(X86Reg::Rdx)
            && *src2 == x86(X86Reg::gpr(case.src2))
            && *width == case.width
    ));
    assert!(
        x86_mulx_arch_shape_valid(op),
        "{case:?}: architectural shape"
    );
    assert_eq!(
        x86_mulx_shape_valid(op),
        !case.needs_x86_state_bridge(),
        "{case:?}: direct identity shape"
    );
    assert_eq!(
        crate::smir::lower::x86_64::x86_state_backed_gpr_mulx_candidate(op),
        case.needs_x86_state_bridge(),
        "{case:?}: state-backed candidacy"
    );
    assert_eq!(
        crate::smir::lower::x86_64::x86_state_backed_gpr_mulx_valid(op),
        case.needs_x86_state_bridge(),
        "{case:?}: state-backed validity"
    );
}

fn initial_state(case: MulxCase, lhs: u64, rhs: u64, ordinal: usize) -> MulxState {
    let mut gpr = core::array::from_fn(|index| {
        0x1020_3040_5060_7080u64.wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_3333))
    });
    gpr[2] = lhs;
    if case.src2 != 2 {
        gpr[usize::from(case.src2)] = rhs;
    }
    MulxState {
        gpr,
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
    }
}

fn expected(case: MulxCase, initial: &MulxState) -> MulxState {
    let mut expected = initial.clone();
    let lhs = initial.gpr[2];
    let rhs = initial.gpr[usize::from(case.src2)];
    let (lo, hi) = match case.width {
        OpWidth::W32 => {
            let product = u64::from(lhs as u32) * u64::from(rhs as u32);
            (product & 0xFFFF_FFFF, product >> 32)
        }
        OpWidth::W64 => {
            let product = u128::from(lhs) * u128::from(rhs);
            (product as u64, (product >> 64) as u64)
        }
        _ => unreachable!("MULX supports only W32/W64"),
    };
    expected.gpr[usize::from(case.dst_lo)] = lo;
    expected.gpr[usize::from(case.dst_hi)] = hi;
    expected
}

fn interpret(case: MulxCase, initial: &MulxState, level: OptLevel) -> MulxState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let function = optimized_function(case, level, true);
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.gpr = initial.gpr;
    x86.rflags = initial.rflags;
    x86.apx_enabled = case.encoding == EncodingKind::Apx;
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
    MulxState {
        gpr: x86.gpr,
        rflags: x86.rflags,
    }
}

fn lower_x86(case: MulxCase, level: OptLevel) -> (Vec<u8>, usize) {
    let function = optimized_function(case, level, false);
    assert!(
        is_native_clobber_safe(&function),
        "{level:?} {case:?}: x86-64 admission"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(true);
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
    (code, lowered.entry_offset)
}

fn lower_aarch64(case: MulxCase, level: OptLevel) -> (Vec<u8>, usize) {
    let function = optimized_function(case, level, false);
    assert!(
        is_x86_aarch64_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(),),
        "{level:?} {case:?}: x86-on-AArch64 admission"
    );
    let mut lowerer = Aarch64Lowerer::new();
    lowerer.set_x86_guest_state_guards(true);
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
    (code, lowered.entry_offset)
}

#[test]
fn vex_mulx_all_8192_register_encodings_are_admitted_and_lowerable_on_both_hosts() {
    assert_eq!(
        MulxCase {
            encoding: EncodingKind::Vex,
            width: OpWidth::W64,
            dst_lo: 9,
            dst_hi: 8,
            src2: 10,
        }
        .bytes(),
        [0xC4, 0x42, 0xB3, 0xF6, 0xC2]
    );

    let mut shapes = 0usize;
    let mut admissions = 0usize;
    let mut lowerings = 0usize;
    for width in [OpWidth::W32, OpWidth::W64] {
        for dst_lo in 0..16 {
            for dst_hi in 0..16 {
                for src2 in 0..16 {
                    let case = MulxCase {
                        encoding: EncodingKind::Vex,
                        width,
                        dst_lo,
                        dst_hi,
                        src2,
                    };
                    for level in LEVELS {
                        let function = optimized_function(case, level, false);
                        assert!(is_native_clobber_safe(&function), "{level:?} {case:?}");
                        assert!(
                            is_x86_aarch64_native_clobber_safe_excluding(
                                &function,
                                &std::collections::HashMap::new(),
                            ),
                            "AArch64 {level:?} {case:?}"
                        );
                        admissions += 2;
                        if !matches!(level, OptLevel::O1) {
                            assert!(!lower_x86(case, level).0.is_empty());
                            assert!(!lower_aarch64(case, level).0.is_empty());
                            lowerings += 2;
                        }
                    }
                    shapes += 1;
                }
            }
        }
    }
    assert_eq!(shapes, 8_192);
    assert_eq!(admissions, 49_152);
    assert_eq!(lowerings, 32_768);
}

#[test]
fn apx_mulx_all_65536_register_encodings_lift_and_use_exact_host_frontiers() {
    assert_eq!(
        MulxCase {
            encoding: EncodingKind::Apx,
            width: OpWidth::W64,
            dst_lo: 19,
            dst_hi: 20,
            src2: 3,
        }
        .bytes(),
        [0x62, 0xE2, 0xE7, 0x00, 0xF6, 0xE3]
    );

    for (width, expected_core) in [
        (OpWidth::W32, [0xC4, 0xC2, 0x73, 0xF6, 0xF8]),
        (OpWidth::W64, [0xC4, 0xC2, 0xF3, 0xF6, 0xF8]),
    ] {
        let case = MulxCase {
            encoding: EncodingKind::Apx,
            width,
            dst_lo: 16,
            dst_hi: 31,
            src2: 4,
        };
        let code = lower_x86(case, OptLevel::O0).0;
        assert!(
            code.windows(expected_core.len())
                .any(|window| window == expected_core),
            "{case:?}: missing scratch-register MULX {expected_core:02X?} in {code:02X?}"
        );
    }

    let mut shapes = 0usize;
    let mut x86_admissions = 0usize;
    let mut x86_lowerings = 0usize;
    let mut aarch64_admissions = 0usize;
    let mut aarch64_lowerings = 0usize;
    for width in [OpWidth::W32, OpWidth::W64] {
        for dst_lo in 0..32 {
            for dst_hi in 0..32 {
                for src2 in 0..32 {
                    let case = MulxCase {
                        encoding: EncodingKind::Apx,
                        width,
                        dst_lo,
                        dst_hi,
                        src2,
                    };
                    for level in LEVELS {
                        let function = optimized_function(case, level, false);
                        assert!(
                            is_native_clobber_safe(&function),
                            "x86-64 {level:?} {case:?}"
                        );
                        x86_admissions += 1;
                        assert_eq!(
                            is_x86_aarch64_native_clobber_safe_excluding(
                                &function,
                                &std::collections::HashMap::new(),
                            ),
                            case.is_legacy(),
                            "AArch64 {level:?} {case:?}"
                        );
                        aarch64_admissions += usize::from(case.is_legacy());
                        if !matches!(level, OptLevel::O1) {
                            assert!(!lower_x86(case, level).0.is_empty());
                            x86_lowerings += 1;
                            if case.is_legacy() {
                                assert!(!lower_aarch64(case, level).0.is_empty());
                                aarch64_lowerings += 1;
                            }
                        }
                    }
                    shapes += 1;
                }
            }
        }
    }
    assert_eq!(shapes, 65_536);
    assert_eq!(x86_admissions, 196_608);
    assert_eq!(x86_lowerings, 131_072);
    assert_eq!(aarch64_admissions, 24_576);
    assert_eq!(aarch64_lowerings, 16_384);
}

#[test]
fn vex_apx_mulx_interpreter_matches_primary_spec_at_width_and_alias_boundaries() {
    let mut cases = 0usize;
    for (encoding, tuples) in [
        (EncodingKind::Vex, &VEX_TUPLES[..]),
        (EncodingKind::Apx, &APX_TUPLES[..]),
    ] {
        for width in [OpWidth::W32, OpWidth::W64] {
            for &(dst_lo, dst_hi, src2) in tuples {
                let case = MulxCase {
                    encoding,
                    width,
                    dst_lo,
                    dst_hi,
                    src2,
                };
                for (ordinal, (lhs, rhs)) in VALUE_PAIRS.into_iter().enumerate() {
                    let initial = initial_state(case, lhs, rhs, ordinal);
                    let expected = expected(case, &initial);
                    for level in LEVELS {
                        assert_eq!(
                            interpret(case, &initial, level),
                            expected,
                            "{level:?} {case:?} lhs={lhs:#018X} rhs={rhs:#018X}"
                        );
                    }
                    cases += 1;
                }
            }
        }
    }
    assert_eq!(cases, 544);
}

#[test]
fn vex_apx_mulx_reserved_forms_fail_closed_and_memory_stays_x86_host_only() {
    let vex = MulxCase {
        encoding: EncodingKind::Vex,
        width: OpWidth::W64,
        dst_lo: 1,
        dst_hi: 0,
        src2: 3,
    };
    let mut vex_l1 = vex.bytes();
    vex_l1[2] |= 0x04;
    let result = lift(&vex_l1);
    assert_eq!(result.bytes_consumed, 4);
    assert!(result.ops.is_empty());
    assert!(matches!(
        result.control_flow,
        ControlFlow::Trap {
            kind: TrapKind::InvalidOpcode
        }
    ));

    let apx = MulxCase {
        encoding: EncodingKind::Apx,
        width: OpWidth::W64,
        dst_lo: 19,
        dst_hi: 20,
        src2: 3,
    };
    for (name, mutate) in [("NF=1", 0u8), ("L=1", 1), ("U=0 for register form", 2)] {
        let mut bytes = apx.bytes();
        match mutate {
            0 => bytes[3] |= 0x04,
            1 => bytes[3] |= 0x20,
            2 => bytes[2] &= !0x04,
            _ => unreachable!(),
        }
        let result = lift(&bytes);
        assert!(
            matches!(
                result.control_flow,
                ControlFlow::Trap {
                    kind: TrapKind::InvalidOpcode
                }
            ),
            "{name}: {bytes:02X?} {result:?}"
        );
        assert!(result.ops.is_empty(), "{name}: {bytes:02X?}");
    }

    for (name, bytes) in [
        ("VEX memory source", vec![0xC4, 0xE2, 0xF3, 0xF6, 0x03]),
        (
            "APX memory source",
            vec![0x62, 0xEA, 0xE3, 0x00, 0xF6, 0x64, 0x91, 0x20],
        ),
    ] {
        let result = lift(&bytes);
        assert_eq!(
            result.ops.len(),
            if name == "APX memory source" { 3 } else { 2 },
            "{name}"
        );
        assert_eq!(
            matches!(
                result.ops.first(),
                Some(SmirOp {
                    kind: OpKind::X86RequireApx,
                    ..
                })
            ),
            name == "APX memory source",
            "{name}"
        );
        let function = function_from_ops(result.ops, false);
        assert!(
            is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), true,),
            "{name}: x86-64 gate"
        );
        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        lowerer.set_jit_fault_deopt_guards(true);
        lowerer
            .lower_function(&function)
            .unwrap_or_else(|error| panic!("{name}: x86-64 lowering failed: {error:?}"));
        assert!(
            !is_x86_aarch64_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
            ),
            "{name}: AArch64 gate"
        );
    }

    let mut ignored_x = vex.bytes();
    ignored_x[1] &= !0x40;
    assert_exact_mulx(&lift(&ignored_x).ops, vex);
}

#[test]
fn malformed_state_backed_mulx_shapes_fail_closed_before_native_execution() {
    let r16 = x86(X86Reg::R16);
    let rsp = x86(X86Reg::Rsp);
    let rbp = x86(X86Reg::Rbp);
    let rdx = x86(X86Reg::Rdx);
    let malformed = [
        SmirOp::with_hint(
            OpId(0),
            PC,
            OpKind::MulU {
                dst_lo: r16,
                dst_hi: None,
                src1: rdx,
                src2: SrcOperand::Reg(rbp),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            X86OpHint::Mulx,
        ),
        SmirOp::with_hint(
            OpId(0),
            PC,
            OpKind::MulU {
                dst_lo: r16,
                dst_hi: Some(rsp),
                src1: x86(X86Reg::Rax),
                src2: SrcOperand::Reg(rbp),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            X86OpHint::Mulx,
        ),
        SmirOp::with_hint(
            OpId(0),
            PC,
            OpKind::MulU {
                dst_lo: r16,
                dst_hi: Some(rsp),
                src1: rdx,
                src2: SrcOperand::Imm(1),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            X86OpHint::Mulx,
        ),
        SmirOp::with_hint(
            OpId(0),
            PC,
            OpKind::MulU {
                dst_lo: r16,
                dst_hi: Some(rsp),
                src1: rdx,
                src2: SrcOperand::Reg(rbp),
                width: OpWidth::W16,
                flags: FlagUpdate::None,
            },
            X86OpHint::Mulx,
        ),
        SmirOp::with_hint(
            OpId(0),
            PC,
            OpKind::MulU {
                dst_lo: r16,
                dst_hi: Some(rsp),
                src1: rdx,
                src2: SrcOperand::Reg(rbp),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            X86OpHint::Mulx,
        ),
        SmirOp::with_hint(
            OpId(0),
            PC,
            OpKind::MulU {
                dst_lo: r16,
                dst_hi: Some(rsp),
                src1: rdx,
                src2: SrcOperand::Reg(VReg::Virtual(crate::smir::ir::types::VirtualId(7))),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
            X86OpHint::Mulx,
        ),
        SmirOp::new(
            OpId(0),
            PC,
            OpKind::MulU {
                dst_lo: r16,
                dst_hi: Some(rsp),
                src1: rdx,
                src2: SrcOperand::Reg(rbp),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        ),
    ];

    for op in malformed {
        assert!(!x86_mulx_arch_shape_valid(&op), "{op:?}");
        let function = function_from_ops(vec![op.clone()], false);
        assert!(!is_native_clobber_safe(&function), "{op:?}");
        assert!(
            !is_x86_aarch64_native_clobber_safe_excluding(
                &function,
                &std::collections::HashMap::new(),
            ),
            "{op:?}"
        );
        let mut lowerer = X86_64Lowerer::new();
        assert!(lowerer.lower_function(&function).is_err(), "{op:?}");
    }
}

#[test]
fn state_backed_mulx_requires_bmi2_only_when_its_block_can_execute_natively() {
    let case = MulxCase {
        encoding: EncodingKind::Apx,
        width: OpWidth::W64,
        dst_lo: 16,
        dst_hi: 31,
        src2: 4,
    };
    let function = function(case, false);
    assert!(
        !uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
        "state-backed MULX is scalar"
    );

    let mut excluded = std::collections::HashMap::new();
    excluded.insert(function.entry, PC);
    assert!(
        x86_native_scalar_features_supported_excluding(&function, &excluded),
        "an excluded block has no host BMI2 requirement"
    );

    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_scalar_features_supported_excluding(
            &function,
            &std::collections::HashMap::new(),
        ),
        std::is_x86_feature_detected!("bmi2")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_scalar_features_supported_excluding(
        &function,
        &std::collections::HashMap::new(),
    ));
}

#[cfg(target_arch = "x86_64")]
#[test]
fn vex_apx_mulx_native_matches_primary_spec_and_preserves_complete_guest_state() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    if !std::is_x86_feature_detected!("bmi2") {
        return;
    }
    let mut executions = 0usize;
    for (encoding, tuples) in [
        (EncodingKind::Vex, &VEX_TUPLES[..]),
        (EncodingKind::Apx, &APX_TUPLES[..]),
    ] {
        for width in [OpWidth::W32, OpWidth::W64] {
            for &(dst_lo, dst_hi, src2) in tuples {
                let case = MulxCase {
                    encoding,
                    width,
                    dst_lo,
                    dst_hi,
                    src2,
                };
                for level in LEVELS {
                    let (code, entry_offset) = lower_x86(case, level);
                    let exec = ExecMem::new(&code)
                        .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
                    for (ordinal, (lhs, rhs)) in VALUE_PAIRS.into_iter().enumerate() {
                        let initial = initial_state(case, lhs, rhs, ordinal);
                        let expected_core = expected(case, &initial);
                        let mut registers = GuestRegs {
                            gpr: initial.gpr,
                            rflags: initial.rflags,
                            mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F),
                            apx_enabled: u64::from(case.encoding == EncodingKind::Apx),
                            k: core::array::from_fn(|index| {
                                0x0102_0304_0506_0708u64.rotate_left(index as u32)
                            }),
                            ..GuestRegs::default()
                        };
                        for (index, vector) in registers.zmm.iter_mut().enumerate() {
                            *vector = core::array::from_fn(|lane| {
                                0x1122_3344_5566_7788u64.wrapping_add((index * 8 + lane) as u64)
                            });
                        }
                        let mut expected_full = registers;
                        expected_full.gpr = expected_core.gpr;
                        expected_full.rflags = expected_core.rflags;
                        exec.run(entry_offset, &mut registers);
                        expected_full.host_mxcsr = registers.host_mxcsr;
                        assert_eq!(
                            registers, expected_full,
                            "{level:?} {case:?} lhs={lhs:#018X} rhs={rhs:#018X}"
                        );
                        executions += 1;
                    }
                }
            }
        }
    }
    eprintln!("executed {executions} native VEX/APX MULX cases");
    assert_eq!(executions, 1_632);
}

#[cfg(target_arch = "aarch64")]
#[test]
fn vex_apx_mulx_aarch64_native_matches_primary_spec_and_preserves_complete_guest_state() {
    use crate::smir::lower::runtime::{Aarch64GuestRegs, ExecMem};

    let apx_legacy = &APX_TUPLES[..2];
    let mut executions = 0usize;
    for (encoding, tuples) in [
        (EncodingKind::Vex, &VEX_TUPLES[..]),
        (EncodingKind::Apx, apx_legacy),
    ] {
        for width in [OpWidth::W32, OpWidth::W64] {
            for &(dst_lo, dst_hi, src2) in tuples {
                let case = MulxCase {
                    encoding,
                    width,
                    dst_lo,
                    dst_hi,
                    src2,
                };
                for level in LEVELS {
                    let (code, entry_offset) = lower_aarch64(case, level);
                    let exec = ExecMem::new(&code)
                        .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
                    for (ordinal, (lhs, rhs)) in VALUE_PAIRS.into_iter().enumerate() {
                        let initial = initial_state(case, lhs, rhs, ordinal);
                        let expected_core = expected(case, &initial);
                        let mut registers = Aarch64GuestRegs {
                            x: core::array::from_fn(|index| {
                                if index < 16 {
                                    initial.gpr[index]
                                } else {
                                    0xA500_0000_0000_0000 | index as u64
                                }
                            }),
                            sp: 0x0000_7FFF_FFFF_E000,
                            pc: 0xDEAD_BEEF_CAFE_BABE,
                            nzcv: ((ordinal as u64) & 0xF) << 28,
                            fpcr: 0x0040_0000,
                            fpsr: 0x0000_009F,
                            x86_apx_enabled: u64::from(case.encoding == EncodingKind::Apx),
                            v: core::array::from_fn(|index| {
                                0x1122_3344_5566_7788u64.wrapping_add(index as u64)
                            }),
                            exit_flags: 0xA5A5_5A5A_A5A5_5A5A,
                            ..Aarch64GuestRegs::default()
                        };
                        let mut expected_full = registers;
                        expected_full.x[usize::from(case.dst_lo)] =
                            expected_core.gpr[usize::from(case.dst_lo)];
                        expected_full.x[usize::from(case.dst_hi)] =
                            expected_core.gpr[usize::from(case.dst_hi)];
                        exec.run_aarch64_identity(entry_offset, &mut registers);
                        assert_eq!(
                            registers, expected_full,
                            "{level:?} {case:?} lhs={lhs:#018X} rhs={rhs:#018X}"
                        );
                        executions += 1;
                    }
                }
            }
        }
    }
    eprintln!("executed {executions} AArch64-native VEX/APX MULX cases");
    assert_eq!(executions, 1_056);
}
