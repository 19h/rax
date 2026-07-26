//! Native admission and differential coverage for VEX BMI2 variable shifts.

use super::*;
use crate::smir::ir::{SmirBlock, SmirFunction, TrapKind};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::aarch64::Aarch64Lowerer;
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xB120;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
const OPERAND_TUPLES: [(u8, u8, u8); 19] = [
    (0, 3, 1),
    (8, 9, 10),
    (15, 14, 13),
    (4, 5, 6),
    (5, 4, 7),
    (1, 1, 1),
    (1, 1, 2),
    (1, 2, 1),
    (1, 2, 2),
    (4, 4, 4),
    (5, 5, 5),
    (4, 5, 4),
    (5, 4, 5),
    (8, 8, 10),
    (8, 9, 8),
    (8, 9, 9),
    (8, 8, 8),
    (8, 1, 10),
    (8, 1, 8),
];
const SOURCES: [u64; 6] = [
    0,
    1,
    0x0000_0000_8000_0001,
    0x0000_0000_FFFF_FFFF,
    0x8000_0000_0000_0001,
    0xFEDC_BA98_7654_3210,
];
const COUNTS: [u64; 14] = [
    0,
    1,
    2,
    30,
    31,
    32,
    33,
    62,
    63,
    64,
    65,
    0xFF,
    0x1_0000_0021,
    u64::MAX,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShiftKind {
    Shlx,
    Sarx,
    Shrx,
}

impl ShiftKind {
    const ALL: [Self; 3] = [Self::Shlx, Self::Sarx, Self::Shrx];

    fn pp(self) -> u8 {
        match self {
            Self::Shlx => 1,
            Self::Sarx => 2,
            Self::Shrx => 3,
        }
    }

    fn classic_modrm(self) -> u8 {
        match self {
            Self::Shlx => 0xE2,
            Self::Shrx => 0xEA,
            Self::Sarx => 0xFA,
        }
    }

    fn classic_destination_modrm(self, destination: u8) -> u8 {
        let group = match self {
            Self::Shlx => 0xE0,
            Self::Shrx => 0xE8,
            Self::Sarx => 0xF8,
        };
        group | (destination & 7)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ShiftCase {
    kind: ShiftKind,
    width: OpWidth,
    destination: u8,
    source: u8,
    count: u8,
    clear_ignored_x: bool,
}

impl ShiftCase {
    fn w(self) -> bool {
        match self.width {
            OpWidth::W32 => false,
            OpWidth::W64 => true,
            _ => unreachable!("VEX BMI2 shifts have only 32-bit and 64-bit forms"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ShiftState {
    gpr: [u64; 32],
    rflags: u64,
}

fn encoding(case: ShiftCase) -> [u8; 5] {
    assert!(
        case.destination < 16 && case.source < 16 && case.count < 16,
        "{case:?}"
    );
    let mut p0 = 0xE2;
    if case.destination >= 8 {
        p0 &= !0x80;
    }
    if case.clear_ignored_x {
        p0 &= !0x40;
    }
    if case.source >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        (u8::from(case.w()) << 7) | (((!case.count) & 0x0F) << 3) | case.kind.pp(),
        0xF7,
        0xC0 | ((case.destination & 7) << 3) | (case.source & 7),
    ]
}

fn lift(bytes: &[u8]) -> crate::smir::lift::LiftResult {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"))
}

fn function(bytes: &[u8]) -> SmirFunction {
    let result = lift(bytes);
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
}

fn optimized_function(case: ShiftCase, level: OptLevel, halt: bool) -> SmirFunction {
    let bytes = encoding(case);
    let mut function = function(&bytes);
    if halt {
        function.blocks[0].set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
    }
    crate::smir::optimize::optimize_function(&mut function, level);
    assert_exact_shift(&function, case);
    function
}

fn assert_exact_shift(function: &SmirFunction, case: ShiftCase) {
    let [op] = function.blocks[0].ops.as_slice() else {
        panic!(
            "{case:?}: expected one shift operation, got {:?}",
            function.blocks[0].ops
        )
    };
    assert_eq!(op.x86_hint, None, "{case:?}");
    let expected_dst = x86(X86Reg::gpr(case.destination));
    let expected_src = x86(X86Reg::gpr(case.source));
    let expected_count = x86(X86Reg::gpr(case.count));
    match (case.kind, &op.kind) {
        (
            ShiftKind::Shlx,
            OpKind::Shl {
                dst,
                src,
                amount: SrcOperand::Reg(count),
                width,
                flags: FlagUpdate::None,
            },
        )
        | (
            ShiftKind::Sarx,
            OpKind::Sar {
                dst,
                src,
                amount: SrcOperand::Reg(count),
                width,
                flags: FlagUpdate::None,
            },
        )
        | (
            ShiftKind::Shrx,
            OpKind::Shr {
                dst,
                src,
                amount: SrcOperand::Reg(count),
                width,
                flags: FlagUpdate::None,
            },
        ) => {
            assert_eq!(*dst, expected_dst, "{case:?}");
            assert_eq!(*src, expected_src, "{case:?}");
            assert_eq!(*count, expected_count, "{case:?}");
            assert_eq!(*width, case.width, "{case:?}");
        }
        (_, other) => panic!("{case:?}: unexpected operation {other:?}"),
    }
    let needs_state_bridge = [case.destination, case.source, case.count]
        .into_iter()
        .any(|index| matches!(index, 4 | 5));
    assert_eq!(
        crate::smir::lower::x86_64::x86_state_backed_gpr_shift_candidate(op),
        needs_state_bridge,
        "{case:?}: state-backed candidacy"
    );
    assert_eq!(
        crate::smir::lower::x86_64::x86_state_backed_gpr_shift_valid(op),
        needs_state_bridge,
        "{case:?}: state-backed validity"
    );
}

fn expected(case: ShiftCase, initial: &ShiftState) -> ShiftState {
    let mut expected = initial.clone();
    let width_mask = case.width.mask();
    let count_mask = if case.width == OpWidth::W64 {
        0x3F
    } else {
        0x1F
    };
    let source = initial.gpr[usize::from(case.source)] & width_mask;
    let count = initial.gpr[usize::from(case.count)] & count_mask;
    let result = match (case.kind, case.width) {
        (ShiftKind::Shlx, _) => (source << count) & width_mask,
        (ShiftKind::Shrx, _) => source >> count,
        (ShiftKind::Sarx, OpWidth::W32) => u64::from(((source as u32 as i32) >> count) as u32),
        (ShiftKind::Sarx, OpWidth::W64) => ((source as i64) >> count) as u64,
        (_, _) => unreachable!(),
    };
    expected.gpr[usize::from(case.destination)] = result;
    expected
}

fn initial_state(case: ShiftCase, source: u64, count: u64, ordinal: usize) -> ShiftState {
    let mut gpr = [0u64; 32];
    for (index, value) in gpr.iter_mut().enumerate() {
        *value = 0x1020_3040_5060_7080u64
            .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_3333));
    }
    gpr[usize::from(case.source)] = source;
    gpr[usize::from(case.count)] = if case.source == case.count {
        source ^ count.rotate_left((ordinal & 63) as u32)
    } else {
        count
    };
    ShiftState {
        gpr,
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
    }
}

fn interpret(case: ShiftCase, initial: &ShiftState, level: OptLevel) -> ShiftState {
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
    ShiftState {
        gpr: x86.gpr,
        rflags: x86.rflags,
    }
}

fn lower(case: ShiftCase, level: OptLevel) -> (Vec<u8>, usize) {
    let function = optimized_function(case, level, false);
    assert!(
        is_native_clobber_safe(&function),
        "{level:?} {case:?}: exact lifted shift must enter the native gate"
    );
    assert!(
        !uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
        "{level:?} {case:?}: scalar state-backed lowering must not require vector replay"
    );
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
    (code, lowered.entry_offset)
}

fn lower_aarch64(case: ShiftCase, level: OptLevel) -> (Vec<u8>, usize) {
    let function = optimized_function(case, level, false);
    assert!(
        is_x86_aarch64_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(),),
        "{level:?} {case:?}: exact lifted shift must enter the x86-on-AArch64 gate"
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
fn vex_bmi2_shift_all_24_576_register_shapes_are_single_op_admitted_and_lowerable() {
    assert_eq!(
        encoding(ShiftCase {
            kind: ShiftKind::Shlx,
            width: OpWidth::W32,
            destination: 8,
            source: 9,
            count: 10,
            clear_ignored_x: false,
        }),
        [0xC4, 0x42, 0x29, 0xF7, 0xC1]
    );
    assert_eq!(
        encoding(ShiftCase {
            kind: ShiftKind::Sarx,
            width: OpWidth::W64,
            destination: 14,
            source: 15,
            count: 13,
            clear_ignored_x: true,
        }),
        [0xC4, 0x02, 0x92, 0xF7, 0xF7]
    );

    for (case, classic) in [
        (
            ShiftCase {
                kind: ShiftKind::Shlx,
                width: OpWidth::W32,
                destination: 0,
                source: 3,
                count: 1,
                clear_ignored_x: false,
            },
            vec![0xD3, ShiftKind::Shlx.classic_destination_modrm(0)],
        ),
        (
            ShiftCase {
                kind: ShiftKind::Sarx,
                width: OpWidth::W64,
                destination: 4,
                source: 5,
                count: 6,
                clear_ignored_x: false,
            },
            vec![0x48, 0xD3, ShiftKind::Sarx.classic_modrm()],
        ),
        (
            ShiftCase {
                kind: ShiftKind::Shrx,
                width: OpWidth::W32,
                destination: 8,
                source: 9,
                count: 10,
                clear_ignored_x: false,
            },
            vec![0x41, 0xD3, ShiftKind::Shrx.classic_destination_modrm(8)],
        ),
    ] {
        let (code, _) = lower(case, OptLevel::O0);
        assert!(
            code.windows(classic.len()).any(|window| window == classic),
            "{case:?}: missing classic host shift {classic:02X?} in {code:02X?}"
        );
    }

    let mut shapes = 0usize;
    let mut admissions = 0usize;
    let mut lowerings = 0usize;
    let mut aarch64_admissions = 0usize;
    let mut aarch64_lowerings = 0usize;
    for kind in ShiftKind::ALL {
        for width in [OpWidth::W32, OpWidth::W64] {
            for destination in 0..16 {
                for source in 0..16 {
                    for count in 0..16 {
                        let case = ShiftCase {
                            kind,
                            width,
                            destination,
                            source,
                            count,
                            clear_ignored_x: shapes & 1 != 0,
                        };
                        for level in LEVELS {
                            let function = optimized_function(case, level, false);
                            assert!(
                                is_native_clobber_safe(&function),
                                "{level:?} {case:?} {:02X?}",
                                encoding(case)
                            );
                            admissions += 1;
                            assert!(
                                is_x86_aarch64_native_clobber_safe_excluding(
                                    &function,
                                    &std::collections::HashMap::new(),
                                ),
                                "{level:?} {case:?}: x86-on-AArch64 gate"
                            );
                            aarch64_admissions += 1;
                            if !matches!(level, OptLevel::O1) {
                                let (code, _) = lower(case, level);
                                assert!(!code.is_empty(), "{level:?} {case:?}");
                                lowerings += 1;
                                let (code, _) = lower_aarch64(case, level);
                                assert!(!code.is_empty(), "AArch64 {level:?} {case:?}");
                                aarch64_lowerings += 1;
                            }
                        }
                        shapes += 1;
                    }
                }
            }
        }
    }
    assert_eq!(shapes, 24_576);
    assert_eq!(admissions, 73_728);
    assert_eq!(lowerings, 49_152);
    assert_eq!(aarch64_admissions, 73_728);
    assert_eq!(aarch64_lowerings, 49_152);
}

#[test]
fn vex_bmi2_shift_reserved_l1_and_memory_frontiers_are_precise() {
    for kind in ShiftKind::ALL {
        for width in [OpWidth::W32, OpWidth::W64] {
            for high in [false, true] {
                let case = ShiftCase {
                    kind,
                    width,
                    destination: if high { 15 } else { 0 },
                    source: if high { 14 } else { 3 },
                    count: if high { 13 } else { 1 },
                    clear_ignored_x: high,
                };
                let mut l1 = encoding(case);
                l1[2] |= 0x04;
                let result = lift(&l1);
                assert_eq!(result.bytes_consumed, 4, "{case:?} {l1:02X?}");
                assert!(matches!(
                    result.control_flow,
                    ControlFlow::Trap {
                        kind: TrapKind::InvalidOpcode
                    }
                ));
                assert!(result.ops.is_empty(), "{case:?} {l1:02X?}");
            }
        }
    }

    let case = ShiftCase {
        kind: ShiftKind::Sarx,
        width: OpWidth::W64,
        destination: 0,
        source: 3,
        count: 1,
        clear_ignored_x: false,
    };
    let mut memory = encoding(case);
    memory[4] &= 0x3F;
    let memory_function = function(&memory);
    assert_eq!(memory_function.blocks[0].ops.len(), 2);
    assert!(
        is_native_clobber_safe_excluding(&memory_function, &std::collections::HashMap::new(), true,),
        "exact memory-source BMI2 shift must enter the helper-backed x86 gate"
    );
    assert!(
        !is_native_clobber_safe_excluding(
            &memory_function,
            &std::collections::HashMap::new(),
            false,
        ),
        "memory-source BMI2 shift must require helper-backed memory"
    );
    assert!(
        !is_x86_aarch64_native_clobber_safe_excluding(
            &memory_function,
            &std::collections::HashMap::new(),
        ),
        "memory-source BMI2 shift must remain fail-closed on the x86-on-AArch64 path"
    );

    let ignored_x_set = encoding(case);
    let mut ignored_x_clear = ignored_x_set;
    ignored_x_clear[1] &= !0x40;
    assert_exact_shift(&function(&ignored_x_set), case);
    let mut x_alias_case = case;
    x_alias_case.clear_ignored_x = true;
    assert_eq!(ignored_x_clear, encoding(x_alias_case));
    assert_exact_shift(&function(&ignored_x_clear), x_alias_case);
    assert_eq!(ignored_x_set[1] ^ ignored_x_clear[1], 0x40);
}

#[test]
fn vex_bmi2_shift_interpreter_matches_primary_spec_at_count_and_alias_boundaries() {
    let mut cases = 0usize;
    for kind in ShiftKind::ALL {
        for width in [OpWidth::W32, OpWidth::W64] {
            for (destination, source, count) in OPERAND_TUPLES {
                let case = ShiftCase {
                    kind,
                    width,
                    destination,
                    source,
                    count,
                    clear_ignored_x: cases & 1 != 0,
                };
                for (source_ordinal, source_value) in SOURCES.into_iter().enumerate() {
                    for (count_ordinal, count_value) in COUNTS.into_iter().enumerate() {
                        let ordinal = source_ordinal * COUNTS.len() + count_ordinal;
                        let initial = initial_state(case, source_value, count_value, ordinal);
                        let expected = expected(case, &initial);
                        for level in LEVELS {
                            assert_eq!(
                                interpret(case, &initial, level),
                                expected,
                                "{level:?} {case:?} source={source_value:#018X} count={count_value:#018X}"
                            );
                        }
                        cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(cases, 9_576);
}

#[cfg(target_arch = "x86_64")]
#[test]
fn vex_bmi2_shift_native_matches_primary_spec_and_preserves_complete_guest_state() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let mut executions = 0usize;
    for kind in ShiftKind::ALL {
        for width in [OpWidth::W32, OpWidth::W64] {
            for (tuple_ordinal, (destination, source, count)) in
                OPERAND_TUPLES.into_iter().enumerate()
            {
                let case = ShiftCase {
                    kind,
                    width,
                    destination,
                    source,
                    count,
                    clear_ignored_x: tuple_ordinal & 1 != 0,
                };
                for level in LEVELS {
                    let (code, entry_offset) = lower(case, level);
                    let exec = ExecMem::new(&code)
                        .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
                    for (source_ordinal, source_value) in SOURCES.into_iter().enumerate() {
                        for (count_ordinal, count_value) in COUNTS.into_iter().enumerate() {
                            let ordinal = source_ordinal * COUNTS.len() + count_ordinal;
                            let initial = initial_state(case, source_value, count_value, ordinal);
                            let expected_core = expected(case, &initial);
                            let mut registers = GuestRegs {
                                gpr: initial.gpr,
                                rflags: initial.rflags,
                                mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F),
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
                            // `host_mxcsr` is trampoline-private host-thread
                            // scratch, not persistent guest architectural state.
                            expected_full.host_mxcsr = registers.host_mxcsr;
                            assert_eq!(
                                registers, expected_full,
                                "{level:?} {case:?} source={source_value:#018X} count={count_value:#018X}"
                            );
                            executions += 1;
                        }
                    }
                }
            }
        }
    }
    eprintln!("executed {executions} native VEX BMI2 variable-shift cases");
    assert_eq!(executions, 28_728);
}

#[cfg(target_arch = "aarch64")]
#[test]
fn vex_bmi2_shift_aarch64_native_matches_primary_spec_and_preserves_complete_guest_state() {
    use crate::smir::lower::runtime::{Aarch64GuestRegs, ExecMem};

    let mut executions = 0usize;
    for kind in ShiftKind::ALL {
        for width in [OpWidth::W32, OpWidth::W64] {
            for (tuple_ordinal, (destination, source, count)) in
                OPERAND_TUPLES.into_iter().enumerate()
            {
                let case = ShiftCase {
                    kind,
                    width,
                    destination,
                    source,
                    count,
                    clear_ignored_x: tuple_ordinal & 1 != 0,
                };
                for level in LEVELS {
                    let (code, entry_offset) = lower_aarch64(case, level);
                    let exec = ExecMem::new(&code)
                        .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
                    for (source_ordinal, source_value) in SOURCES.into_iter().enumerate() {
                        for (count_ordinal, count_value) in COUNTS.into_iter().enumerate() {
                            let ordinal = source_ordinal * COUNTS.len() + count_ordinal;
                            let initial = initial_state(case, source_value, count_value, ordinal);
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
                                v: core::array::from_fn(|index| {
                                    0x1122_3344_5566_7788u64.wrapping_add(index as u64)
                                }),
                                exit_flags: 0xA5A5_5A5A_A5A5_5A5A,
                                ..Aarch64GuestRegs::default()
                            };
                            let mut expected_full = registers;
                            expected_full.x[usize::from(case.destination)] =
                                expected_core.gpr[usize::from(case.destination)];
                            exec.run_aarch64_identity(entry_offset, &mut registers);
                            assert_eq!(
                                registers, expected_full,
                                "{level:?} {case:?} source={source_value:#018X} count={count_value:#018X}"
                            );
                            executions += 1;
                        }
                    }
                }
            }
        }
    }
    eprintln!("executed {executions} AArch64-native VEX BMI2 variable-shift cases");
    assert_eq!(executions, 28_728);
}
