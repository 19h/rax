//! Exhaustive APX BMI2 variable-shift native admission and execution coverage.

use super::*;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpWidth, SignExtend, SrcOperand,
    VReg, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, TrapKind};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::aarch64::Aarch64Lowerer;
use crate::smir::lower::runtime::{
    is_native_clobber_safe, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_native_scalar_features_supported_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xB2C0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];
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
const REGISTER_TUPLES: [(u8, u8, u8); 22] = [
    (0, 3, 1),
    (8, 9, 10),
    (15, 14, 13),
    (16, 17, 18),
    (31, 30, 29),
    (4, 5, 6),
    (5, 4, 7),
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
    (16, 16, 17),
    (16, 17, 16),
    (17, 16, 16),
    (31, 30, 30),
    (30, 31, 30),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShiftKind {
    Shlx,
    Shrx,
    Sarx,
}

impl ShiftKind {
    const ALL: [Self; 3] = [Self::Shlx, Self::Shrx, Self::Sarx];

    fn pp(self) -> u8 {
        match self {
            Self::Shlx => 1,
            Self::Sarx => 2,
            Self::Shrx => 3,
        }
    }

    fn classic_digit(self) -> u8 {
        match self {
            Self::Shlx => 4,
            Self::Shrx => 5,
            Self::Sarx => 7,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RegisterCase {
    kind: ShiftKind,
    width: OpWidth,
    destination: u8,
    source: u8,
    count: u8,
}

impl RegisterCase {
    fn bytes(self) -> Vec<u8> {
        assert!(
            self.destination < 32 && self.source < 32 && self.count < 32,
            "{self:?}"
        );
        let mut p0 = 0x42; // X=1 and map 0F38.
        if self.destination & 8 == 0 {
            p0 |= 0x80;
        }
        if self.destination & 16 == 0 {
            p0 |= 0x10;
        }
        if self.source & 8 == 0 {
            p0 |= 0x20;
        }
        if self.source & 16 != 0 {
            p0 |= 0x08;
        }
        vec![
            0x62,
            p0,
            (u8::from(self.width == OpWidth::W64) << 7)
                | (((!self.count) & 0x0F) << 3)
                | 0x04 // U=1 for ModRM.Mod=3.
                | self.kind.pp(),
            if self.count < 16 { 0x08 } else { 0x00 },
            0xF7,
            0xC0 | ((self.destination & 7) << 3) | (self.source & 7),
        ]
    }

    fn is_legacy(self) -> bool {
        [self.destination, self.source, self.count]
            .into_iter()
            .all(|index| index < 16)
    }

    fn needs_x86_state_bridge(self) -> bool {
        [self.destination, self.source, self.count]
            .into_iter()
            .any(|index| index >= 16 || matches!(index, 4 | 5))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MemoryCase {
    kind: ShiftKind,
    width: OpWidth,
    destination: u8,
    count: u8,
}

impl MemoryCase {
    /// Encode `SHLX`/`SHRX`/`SARX destination,[RBX],count`.
    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 32 && self.count < 32, "{self:?}");
        let mut p0 = 0x62; // X/B encode legacy RBX; map 0F38.
        if self.destination & 8 == 0 {
            p0 |= 0x80;
        }
        if self.destination & 16 == 0 {
            p0 |= 0x10;
        }
        vec![
            0x62,
            p0,
            (u8::from(self.width == OpWidth::W64) << 7)
                | (((!self.count) & 0x0F) << 3)
                | 0x04
                | self.kind.pp(),
            if self.count < 16 { 0x08 } else { 0x00 },
            0xF7,
            ((self.destination & 7) << 3) | 3,
        ]
    }

    fn mem_width(self) -> MemWidth {
        match self.width {
            OpWidth::W32 => MemWidth::B4,
            OpWidth::W64 => MemWidth::B8,
            _ => unreachable!("APX BMI2 shifts have only W32/W64 forms"),
        }
    }

    fn needs_x86_state_bridge(self) -> bool {
        [self.destination, self.count]
            .into_iter()
            .any(|index| index >= 16 || matches!(index, 4 | 5))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ShiftState {
    gpr: [u64; 32],
    rflags: u64,
}

fn x86(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::gpr(index)))
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

fn register_function(case: RegisterCase, halt: bool) -> SmirFunction {
    let bytes = case.bytes();
    let result = lift(&bytes);
    assert_eq!(result.bytes_consumed, bytes.len(), "{case:?} {bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert_register_shape(&result.ops, case);
    function_from_ops(result.ops, halt)
}

fn memory_function(case: MemoryCase, halt: bool) -> SmirFunction {
    memory_function_from_bytes(case, &case.bytes(), &Address::Direct(x86(3)), halt)
}

fn memory_function_from_bytes(
    case: MemoryCase,
    bytes: &[u8],
    expected_addr: &Address,
    halt: bool,
) -> SmirFunction {
    let result = lift(bytes);
    assert_eq!(result.bytes_consumed, bytes.len(), "{case:?} {bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert_memory_shape(&result.ops, case, expected_addr);
    function_from_ops(result.ops, halt)
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn guarded_payload<'a>(ops: &'a [SmirOp], case: &impl std::fmt::Debug) -> &'a [SmirOp] {
    assert!(
        matches!(
            ops.first(),
            Some(SmirOp {
                kind: OpKind::X86RequireApx,
                ..
            })
        ),
        "{case:?}: missing leading APX requirement: {ops:?}"
    );
    &ops[1..]
}

fn assert_register_shape(ops: &[SmirOp], case: RegisterCase) {
    let ops = guarded_payload(ops, &case);
    let [op] = ops else {
        panic!("{case:?}: expected one canonical shift, got {ops:?}")
    };
    assert_eq!(op.x86_hint, None, "{case:?}");
    let valid = match (&op.kind, case.kind) {
        (
            OpKind::Shl {
                dst,
                src,
                amount: SrcOperand::Reg(count),
                width,
                flags: FlagUpdate::None,
            },
            ShiftKind::Shlx,
        )
        | (
            OpKind::Shr {
                dst,
                src,
                amount: SrcOperand::Reg(count),
                width,
                flags: FlagUpdate::None,
            },
            ShiftKind::Shrx,
        )
        | (
            OpKind::Sar {
                dst,
                src,
                amount: SrcOperand::Reg(count),
                width,
                flags: FlagUpdate::None,
            },
            ShiftKind::Sarx,
        ) => {
            *dst == x86(case.destination)
                && *src == x86(case.source)
                && *count == x86(case.count)
                && *width == case.width
        }
        _ => false,
    };
    assert!(valid, "{case:?}: unexpected operation {op:?}");
    assert_eq!(
        crate::smir::lower::x86_64::x86_state_backed_gpr_shift_candidate(op),
        case.needs_x86_state_bridge(),
        "{case:?}: state-backed candidacy"
    );
    assert_eq!(
        crate::smir::lower::x86_64::x86_state_backed_gpr_shift_valid(op),
        case.needs_x86_state_bridge(),
        "{case:?}: state-backed validity"
    );
}

fn assert_memory_shape(ops: &[SmirOp], case: MemoryCase, expected_addr: &Address) {
    let ops = guarded_payload(ops, &case);
    let [load, shift] = ops else {
        panic!("{case:?}: expected canonical Load + shift, got {ops:?}")
    };
    assert_eq!(load.x86_hint, None, "{case:?}");
    let temporary = match &load.kind {
        OpKind::Load {
            dst: temporary @ VReg::Virtual(_),
            addr,
            width,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(addr, expected_addr, "{case:?}");
            assert_eq!(*width, case.mem_width(), "{case:?}");
            *temporary
        }
        other => panic!("{case:?}: unexpected load {other:?}"),
    };
    assert_eq!(shift.guest_pc, load.guest_pc, "{case:?}");
    assert_eq!(shift.x86_hint, None, "{case:?}");
    let valid = match (&shift.kind, case.kind) {
        (
            OpKind::Shl {
                dst,
                src,
                amount: SrcOperand::Reg(count),
                width,
                flags: FlagUpdate::None,
            },
            ShiftKind::Shlx,
        )
        | (
            OpKind::Shr {
                dst,
                src,
                amount: SrcOperand::Reg(count),
                width,
                flags: FlagUpdate::None,
            },
            ShiftKind::Shrx,
        )
        | (
            OpKind::Sar {
                dst,
                src,
                amount: SrcOperand::Reg(count),
                width,
                flags: FlagUpdate::None,
            },
            ShiftKind::Sarx,
        ) => {
            *dst == x86(case.destination)
                && *src == temporary
                && *count == x86(case.count)
                && *width == case.width
        }
        _ => false,
    };
    assert!(valid, "{case:?}: unexpected consumer {shift:?}");
}

fn optimized_register(case: RegisterCase, level: OptLevel, halt: bool) -> SmirFunction {
    let function = optimize(register_function(case, halt), level);
    assert_register_shape(&function.blocks[0].ops, case);
    function
}

fn optimized_memory(case: MemoryCase, level: OptLevel, halt: bool) -> SmirFunction {
    let function = optimize(memory_function(case, halt), level);
    assert_memory_shape(&function.blocks[0].ops, case, &Address::Direct(x86(3)));
    function
}

fn lower_register(case: RegisterCase, level: OptLevel) -> (Vec<u8>, usize) {
    let function = optimized_register(case, level, false);
    assert!(
        is_native_clobber_safe(&function),
        "{level:?} {case:?}: x86-64 gate"
    );
    assert!(
        !uses_x86_native_vectors_excluding(&function, &std::collections::HashMap::new()),
        "{level:?} {case:?}: scalar region"
    );
    assert!(
        x86_native_scalar_features_supported_excluding(
            &function,
            &std::collections::HashMap::new(),
        ),
        "{level:?} {case:?}: lowering uses baseline classic shifts, not host APX/BMI2"
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

fn lower_register_aarch64(case: RegisterCase, level: OptLevel) -> (Vec<u8>, usize) {
    let function = optimized_register(case, level, false);
    assert!(case.is_legacy(), "{case:?}");
    assert!(
        is_x86_aarch64_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(),),
        "{level:?} {case:?}: x86-on-AArch64 gate"
    );
    let mut lowerer = Aarch64Lowerer::new();
    lowerer.set_x86_guest_state_guards(true);
    let lowered = lowerer
        .lower_function(&function)
        .unwrap_or_else(|error| panic!("AArch64 {level:?} {case:?}: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("AArch64 {level:?} {case:?}: {error:?}"));
    (code, lowered.entry_offset)
}

fn lower_memory_function(function: &SmirFunction) -> (Vec<u8>, usize) {
    assert!(is_native_clobber_safe_excluding(
        function,
        &std::collections::HashMap::new(),
        true,
    ));
    assert!(!is_native_clobber_safe_excluding(
        function,
        &std::collections::HashMap::new(),
        false,
    ));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        function,
        &std::collections::HashMap::new(),
    ));
    assert!(x86_native_scalar_features_supported_excluding(
        function,
        &std::collections::HashMap::new(),
    ));

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    let lowered = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("APX memory shift lowering: {error:?}"));
    let code = lowerer
        .finalize()
        .unwrap_or_else(|error| panic!("APX memory shift finalize: {error:?}"));
    (code, lowered.entry_offset)
}

fn lower_memory(case: MemoryCase, level: OptLevel) -> (Vec<u8>, usize) {
    lower_memory_function(&optimized_memory(case, level, false))
}

fn initial_state(case: RegisterCase, source: u64, count: u64, ordinal: usize) -> ShiftState {
    let mut gpr = core::array::from_fn(|index| {
        0x1020_3040_5060_7080u64.wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_3333))
    });
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

fn shifted_value(kind: ShiftKind, width: OpWidth, source: u64, count: u64) -> u64 {
    let width_mask = width.mask();
    let count_mask = if width == OpWidth::W64 { 0x3F } else { 0x1F };
    let source = source & width_mask;
    let count = count & count_mask;
    match (kind, width) {
        (ShiftKind::Shlx, _) => (source << count) & width_mask,
        (ShiftKind::Shrx, _) => source >> count,
        (ShiftKind::Sarx, OpWidth::W32) => u64::from(((source as u32 as i32) >> count) as u32),
        (ShiftKind::Sarx, OpWidth::W64) => ((source as i64) >> count) as u64,
        (_, _) => unreachable!("APX BMI2 shifts have only W32/W64 forms"),
    }
}

fn expected_register(case: RegisterCase, initial: &ShiftState) -> ShiftState {
    let mut expected = initial.clone();
    expected.gpr[usize::from(case.destination)] = shifted_value(
        case.kind,
        case.width,
        initial.gpr[usize::from(case.source)],
        initial.gpr[usize::from(case.count)],
    );
    expected
}

fn interpret_register(case: RegisterCase, initial: &ShiftState, level: OptLevel) -> ShiftState {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let function = optimized_register(case, level, true);
    let mut context = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
        unreachable!()
    };
    x86.gpr = initial.gpr;
    x86.rflags = initial.rflags;
    x86.apx_enabled = true;
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

#[test]
fn apx_bmi2_shift_all_196608_register_encodings_are_canonical_admitted_and_lowerable() {
    assert_eq!(
        RegisterCase {
            kind: ShiftKind::Sarx,
            width: OpWidth::W64,
            destination: 20,
            source: 19,
            count: 3,
        }
        .bytes(),
        [0x62, 0xEA, 0xE6, 0x08, 0xF7, 0xE3]
    );
    assert_eq!(
        RegisterCase {
            kind: ShiftKind::Shlx,
            width: OpWidth::W32,
            destination: 31,
            source: 16,
            count: 29,
        }
        .bytes(),
        [0x62, 0x6A, 0x15, 0x00, 0xF7, 0xF8]
    );

    for (kind, expected_core) in [
        (ShiftKind::Shlx, [0x48, 0xD3, 0xE2]),
        (ShiftKind::Shrx, [0x48, 0xD3, 0xEA]),
        (ShiftKind::Sarx, [0x48, 0xD3, 0xFA]),
    ] {
        let case = RegisterCase {
            kind,
            width: OpWidth::W64,
            destination: 31,
            source: 16,
            count: 29,
        };
        let (code, _) = lower_register(case, OptLevel::O0);
        assert!(
            code.windows(expected_core.len())
                .any(|window| window == expected_core),
            "{case:?}: missing state-backed classic shift {expected_core:02X?} in {code:02X?}"
        );
    }

    let mut shapes = 0usize;
    let mut x86_admissions = 0usize;
    let mut x86_lowerings = 0usize;
    let mut aarch64_admissions = 0usize;
    let mut aarch64_lowerings = 0usize;
    for kind in ShiftKind::ALL {
        for width in [OpWidth::W32, OpWidth::W64] {
            for destination in 0..32 {
                for source in 0..32 {
                    for count in 0..32 {
                        let case = RegisterCase {
                            kind,
                            width,
                            destination,
                            source,
                            count,
                        };
                        for level in LEVELS {
                            let function = optimized_register(case, level, false);
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
                                assert!(!lower_register(case, level).0.is_empty());
                                x86_lowerings += 1;
                                if case.is_legacy() {
                                    assert!(!lower_register_aarch64(case, level).0.is_empty());
                                    aarch64_lowerings += 1;
                                }
                            }
                        }
                        shapes += 1;
                    }
                }
            }
        }
    }
    assert_eq!(shapes, 3 * 2 * 32 * 32 * 32);
    assert_eq!(x86_admissions, shapes * LEVELS.len());
    assert_eq!(x86_lowerings, shapes * 2);
    assert_eq!(aarch64_admissions, 3 * 2 * 16 * 16 * 16 * LEVELS.len());
    assert_eq!(aarch64_lowerings, 3 * 2 * 16 * 16 * 16 * 2);
}

#[test]
fn apx_bmi2_shift_interpreter_matches_primary_spec_at_count_alias_and_egpr_boundaries() {
    let mut cases = 0usize;
    for kind in ShiftKind::ALL {
        for width in [OpWidth::W32, OpWidth::W64] {
            for (destination, source, count) in REGISTER_TUPLES {
                let case = RegisterCase {
                    kind,
                    width,
                    destination,
                    source,
                    count,
                };
                for (source_ordinal, source_value) in SOURCES.into_iter().enumerate() {
                    for (count_ordinal, count_value) in COUNTS.into_iter().enumerate() {
                        let ordinal = source_ordinal * COUNTS.len() + count_ordinal;
                        let initial = initial_state(case, source_value, count_value, ordinal);
                        let expected = expected_register(case, &initial);
                        for level in LEVELS {
                            assert_eq!(
                                interpret_register(case, &initial, level),
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
    assert_eq!(
        cases,
        ShiftKind::ALL.len() * 2 * REGISTER_TUPLES.len() * SOURCES.len() * COUNTS.len()
    );
}

#[test]
fn apx_bmi2_shift_guard_is_dynamic_precise_and_noncommitting() {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

    let case = RegisterCase {
        kind: ShiftKind::Shlx,
        width: OpWidth::W64,
        destination: 0,
        source: 3,
        count: 1,
    };
    let initial = initial_state(case, 0x0123_4567_89AB_CDEF, 17, 0);
    let expected = expected_register(case, &initial);

    for level in [OptLevel::O0, OptLevel::O2] {
        let function = optimized_register(case, level, true);
        assert!(matches!(
            function.blocks[0].ops.first(),
            Some(SmirOp {
                kind: OpKind::X86RequireApx,
                ..
            })
        ));

        for enabled in [false, true] {
            let mut context = SmirContext::new_x86_64();
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.gpr = initial.gpr;
            x86.rflags = initial.rflags;
            x86.apx_enabled = enabled;
            context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
            context.flags.lazy = None;

            let execution = SmirInterpreter::new().execute_block(
                &mut context,
                &mut FlatMemory::new(1),
                &function.blocks[0],
            );
            let ArchRegState::X86_64(x86) = &context.arch_regs else {
                unreachable!()
            };
            if enabled {
                assert!(matches!(execution, BlockResult::Exit(ExitReason::Halt)));
                assert_eq!(x86.gpr, expected.gpr, "{level:?}");
                assert_eq!(x86.rflags, expected.rflags, "{level:?}");
            } else {
                assert!(matches!(
                    execution,
                    BlockResult::Exit(ExitReason::Undefined {
                        addr: PC,
                        opcode: 0,
                    })
                ));
                assert_eq!(x86.gpr, initial.gpr, "{level:?}");
                assert_eq!(x86.rflags, initial.rflags, "{level:?}");
            }
        }
    }
}

#[test]
fn apx_bmi2_shift_reserved_nf_l_and_register_u_forms_trap_before_modrm_execution() {
    let case = RegisterCase {
        kind: ShiftKind::Shlx,
        width: OpWidth::W64,
        destination: 20,
        source: 19,
        count: 3,
    };
    for (name, mutate) in [("NF=1", 0u8), ("L=1", 1), ("U=0", 2)] {
        let mut bytes = case.bytes();
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
}

#[test]
fn apx_bmi2_shift_all_6144_fixed_memory_destination_count_shapes_lower_exactly() {
    assert_eq!(
        MemoryCase {
            kind: ShiftKind::Shlx,
            width: OpWidth::W64,
            destination: 0,
            count: 1,
        }
        .bytes(),
        [0x62, 0xF2, 0xF5, 0x08, 0xF7, 0x03]
    );
    assert_eq!(
        MemoryCase {
            kind: ShiftKind::Shrx,
            width: OpWidth::W32,
            destination: 31,
            count: 29,
        }
        .bytes(),
        [0x62, 0x62, 0x17, 0x00, 0xF7, 0x3B]
    );

    let mut shapes = 0usize;
    let mut lowerings = 0usize;
    for kind in ShiftKind::ALL {
        for width in [OpWidth::W32, OpWidth::W64] {
            for destination in 0..32 {
                for count in 0..32 {
                    let case = MemoryCase {
                        kind,
                        width,
                        destination,
                        count,
                    };
                    for level in LEVELS {
                        let function = optimized_memory(case, level, false);
                        assert!(is_native_clobber_safe_excluding(
                            &function,
                            &std::collections::HashMap::new(),
                            true,
                        ));
                        assert_eq!(
                            case.needs_x86_state_bridge(),
                            destination >= 16
                                || count >= 16
                                || matches!(destination, 4 | 5)
                                || matches!(count, 4 | 5)
                        );
                        assert!(!lower_memory_function(&function).0.is_empty());
                        lowerings += 1;
                    }
                    shapes += 1;
                }
            }
        }
    }
    assert_eq!(shapes, 3 * 2 * 32 * 32);
    assert_eq!(lowerings, shapes * LEVELS.len());
}

fn apx_sib_bytes(case: MemoryCase, base: u8, index: u8, scale: u8) -> Vec<u8> {
    assert!(base < 32 && index < 32 && index != 4);
    let mut p0 = 0x02;
    if case.destination & 8 == 0 {
        p0 |= 0x80;
    }
    if index & 8 == 0 {
        p0 |= 0x40;
    }
    if base & 8 == 0 {
        p0 |= 0x20;
    }
    if case.destination & 16 == 0 {
        p0 |= 0x10;
    }
    if base & 16 != 0 {
        p0 |= 0x08;
    }
    vec![
        0x62,
        p0,
        (u8::from(case.width == OpWidth::W64) << 7)
            | (((!case.count) & 0x0F) << 3)
            | (u8::from(index < 16) << 2)
            | case.kind.pp(),
        if case.count < 16 { 0x08 } else { 0x00 },
        0xF7,
        0x40 | ((case.destination & 7) << 3) | 4,
        ((scale.trailing_zeros() as u8) << 6) | ((index & 7) << 3) | (base & 7),
        0x80,
    ]
}

#[test]
fn all_7936_apx_base_index_scale_shift_encodings_lift_and_lower_exactly() {
    let mut count = 0usize;
    for width in [OpWidth::W32, OpWidth::W64] {
        let case = MemoryCase {
            kind: ShiftKind::Shlx,
            width,
            destination: 20,
            count: 19,
        };
        for base in 0..32 {
            for index in (0..32).filter(|index| *index != 4) {
                for scale in [1, 2, 4, 8] {
                    let expected = Address::BaseIndexScale {
                        base: Some(x86(base)),
                        index: x86(index),
                        scale,
                        disp: -128,
                        disp_size: DispSize::Disp8,
                    };
                    let bytes = apx_sib_bytes(case, base, index, scale);
                    let function = memory_function_from_bytes(case, &bytes, &expected, false);
                    assert!(!lower_memory_function(&function).0.is_empty());
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 2 * 32 * 31 * 4);
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(ordinal: usize) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::GuestRegs;

    let mut registers = GuestRegs {
        gpr: core::array::from_fn(|index| {
            0xA500_0000_0000_0000u64.wrapping_add((index as u64) * 0x0101_0101)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F),
        ac_flag: (ordinal & 1) as u64,
        apx_enabled: 1,
        k: core::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left(index as u32)),
        ..GuestRegs::default()
    };
    for (index, vector) in registers.zmm.iter_mut().enumerate() {
        *vector = core::array::from_fn(|lane| {
            0x1122_3344_5566_7788u64.wrapping_add((index * 8 + lane) as u64)
        });
    }
    registers
}

#[cfg(target_arch = "x86_64")]
#[test]
fn apx_bmi2_shift_native_registers_match_spec_and_preserve_complete_guest_state() {
    use crate::smir::lower::runtime::ExecMem;

    let mut executions = 0usize;
    for kind in ShiftKind::ALL {
        for width in [OpWidth::W32, OpWidth::W64] {
            for (destination, source, count) in REGISTER_TUPLES {
                let case = RegisterCase {
                    kind,
                    width,
                    destination,
                    source,
                    count,
                };
                for level in LEVELS {
                    let (code, entry) = lower_register(case, level);
                    let exec = ExecMem::new(&code)
                        .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
                    for (source_ordinal, source_value) in SOURCES.into_iter().enumerate() {
                        for (count_ordinal, count_value) in COUNTS.into_iter().enumerate() {
                            let ordinal = source_ordinal * COUNTS.len() + count_ordinal;
                            let initial = initial_state(case, source_value, count_value, ordinal);
                            let expected_core = expected_register(case, &initial);
                            let mut registers = full_guest_regs(ordinal);
                            registers.gpr = initial.gpr;
                            registers.rflags = initial.rflags;
                            let mut expected = registers;
                            expected.gpr = expected_core.gpr;
                            expected.rflags = expected_core.rflags;

                            exec.run(entry, &mut registers);
                            expected.host_mxcsr = registers.host_mxcsr;
                            assert_eq!(
                                registers, expected,
                                "{level:?} {case:?} source={source_value:#018X} count={count_value:#018X}"
                            );
                            executions += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(
        executions,
        ShiftKind::ALL.len()
            * 2
            * REGISTER_TUPLES.len()
            * LEVELS.len()
            * SOURCES.len()
            * COUNTS.len()
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn apx_bmi2_shift_native_guard_is_dynamic_and_noncommitting() {
    use crate::smir::lower::runtime::ExecMem;

    const SENTINEL_PC: u64 = 0xAAAA_BBBB_CCCC_DDDD;
    let case = RegisterCase {
        kind: ShiftKind::Shlx,
        width: OpWidth::W64,
        destination: 0,
        source: 3,
        count: 1,
    };
    let (code, entry) = lower_register(case, OptLevel::O2);
    let executable = ExecMem::new(&code).expect("APX BMI2 shift executable");
    let initial = initial_state(case, 0x0123_4567_89AB_CDEF, 17, 0);
    let expected = expected_register(case, &initial);

    for enabled in [false, true] {
        let mut registers = full_guest_regs(0);
        registers.gpr = initial.gpr;
        registers.rflags = initial.rflags;
        registers.apx_enabled = u64::from(enabled);
        registers.exit_pc = SENTINEL_PC;
        let before = registers;

        executable.run(entry, &mut registers);
        if enabled {
            assert_eq!(registers.gpr, expected.gpr);
            assert_eq!(registers.rflags & 0x8D5, expected.rflags & 0x8D5);
            assert_eq!(registers.exit_pc, SENTINEL_PC);
        } else {
            assert_eq!(registers.gpr, before.gpr);
            assert_eq!(registers.rflags & 0x8D5, before.rflags & 0x8D5);
            assert_eq!(registers.exit_pc, PC);
        }
    }
}

#[cfg(target_arch = "aarch64")]
#[test]
fn apx_bmi2_shift_aarch64_native_matches_primary_spec_and_preserves_complete_guest_state() {
    use crate::smir::lower::runtime::{Aarch64GuestRegs, ExecMem};

    let legacy_tuples = REGISTER_TUPLES
        .into_iter()
        .filter(|&(destination, source, count)| {
            [destination, source, count]
                .into_iter()
                .all(|index| index < 16)
        })
        .collect::<Vec<_>>();
    assert_eq!(legacy_tuples.len(), 5);

    let mut executions = 0usize;
    for kind in ShiftKind::ALL {
        for width in [OpWidth::W32, OpWidth::W64] {
            for &(destination, source, count) in &legacy_tuples {
                let case = RegisterCase {
                    kind,
                    width,
                    destination,
                    source,
                    count,
                };
                for level in LEVELS {
                    let (code, entry_offset) = lower_register_aarch64(case, level);
                    let exec = ExecMem::new(&code)
                        .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
                    for (source_ordinal, source_value) in SOURCES.into_iter().enumerate() {
                        for (count_ordinal, count_value) in COUNTS.into_iter().enumerate() {
                            let ordinal = source_ordinal * COUNTS.len() + count_ordinal;
                            let initial = initial_state(case, source_value, count_value, ordinal);
                            let expected_core = expected_register(case, &initial);
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
                                x86_apx_enabled: 1,
                                ..Aarch64GuestRegs::default()
                            };
                            let mut expected = registers;
                            expected.x[usize::from(destination)] =
                                expected_core.gpr[usize::from(destination)];

                            exec.run_aarch64_identity(entry_offset, &mut registers);
                            assert_eq!(
                                registers, expected,
                                "{level:?} {case:?} source={source_value:#018X} count={count_value:#018X}"
                            );
                            executions += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(
        executions,
        ShiftKind::ALL.len()
            * 2
            * legacy_tuples.len()
            * LEVELS.len()
            * SOURCES.len()
            * COUNTS.len()
    );
}

#[cfg(target_arch = "x86_64")]
#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[cfg(target_arch = "x86_64")]
#[derive(Default)]
struct MemoryContext {
    value: u64,
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_size: u64,
    last_signed: u64,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn load_helper(
    context: *mut MemoryContext,
    addr: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    context.calls += 1;
    context.last_addr = addr;
    context.last_size = size;
    context.last_signed = signed;
    LoadResult {
        value: context.value,
        ok: context.ok,
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn apx_memory_bmi2_shifts_are_fault_precise_and_preserve_complete_guest_state() {
    use crate::smir::lower::runtime::ExecMem;

    const TUPLES: [(u8, u8); 13] = [
        (0, 1),
        (8, 10),
        (15, 13),
        (16, 17),
        (31, 30),
        (16, 16),
        (31, 31),
        (4, 5),
        (5, 4),
        (4, 16),
        (16, 5),
        (3, 17),
        (17, 3),
    ];

    let mut successes = 0usize;
    let mut faults = 0usize;
    for kind in ShiftKind::ALL {
        for width in [OpWidth::W32, OpWidth::W64] {
            for (destination, count) in TUPLES {
                let case = MemoryCase {
                    kind,
                    width,
                    destination,
                    count,
                };
                for level in LEVELS {
                    let (code, entry) = lower_memory(case, level);
                    let exec = ExecMem::new(&code)
                        .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
                    for (source_ordinal, source) in SOURCES.into_iter().enumerate() {
                        for (count_ordinal, count_value) in COUNTS.into_iter().enumerate() {
                            let ordinal = source_ordinal * COUNTS.len() + count_ordinal;
                            let mut context = MemoryContext {
                                value: source,
                                ok: 1,
                                ..MemoryContext::default()
                            };
                            let mut registers = full_guest_regs(ordinal);
                            registers.gpr[3] =
                                0x4000_0000_0000_1000 + (ordinal as u64).wrapping_mul(0x20);
                            registers.gpr[usize::from(count)] = count_value;
                            let expected_addr = registers.gpr[3];
                            registers.ctx = (&mut context as *mut MemoryContext) as u64;
                            registers.load_fn = load_helper as usize as u64;
                            let mut expected = registers;
                            expected.gpr[usize::from(destination)] =
                                shifted_value(kind, width, source, count_value);

                            exec.run(entry, &mut registers);
                            expected.host_mxcsr = registers.host_mxcsr;
                            assert_eq!(
                                registers, expected,
                                "{level:?} {case:?} source={source:#018X} count={count_value:#018X}"
                            );
                            assert_eq!(context.calls, 1, "{level:?} {case:?}");
                            assert_eq!(context.last_addr, expected_addr, "{level:?} {case:?}");
                            assert_eq!(
                                context.last_size,
                                u64::from(width.bits() / 8),
                                "{level:?} {case:?}"
                            );
                            assert_eq!(context.last_signed, 0, "{level:?} {case:?}");
                            successes += 1;
                        }
                    }

                    let mut context = MemoryContext {
                        value: u64::MAX,
                        ok: 0,
                        ..MemoryContext::default()
                    };
                    let mut registers = full_guest_regs(0x55);
                    registers.gpr[3] = 0x1234_5000;
                    registers.gpr[usize::from(count)] = 65;
                    let expected_addr = registers.gpr[3];
                    registers.ctx = (&mut context as *mut MemoryContext) as u64;
                    registers.load_fn = load_helper as usize as u64;
                    let mut expected = registers;
                    expected.exit_pc = PC;

                    exec.run(entry, &mut registers);
                    expected.host_mxcsr = registers.host_mxcsr;
                    assert_eq!(registers, expected, "fault {level:?} {case:?}");
                    assert_eq!(context.calls, 1, "fault {level:?} {case:?}");
                    assert_eq!(context.last_addr, expected_addr, "fault {level:?} {case:?}");
                    assert_eq!(
                        context.last_size,
                        u64::from(width.bits() / 8),
                        "fault {level:?} {case:?}"
                    );
                    assert_eq!(context.last_signed, 0, "fault {level:?} {case:?}");
                    faults += 1;
                }
            }
        }
    }
    assert_eq!(
        successes,
        ShiftKind::ALL.len() * 2 * TUPLES.len() * LEVELS.len() * SOURCES.len() * COUNTS.len()
    );
    assert_eq!(
        faults,
        ShiftKind::ALL.len() * 2 * TUPLES.len() * LEVELS.len()
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn apx_memory_bmi2_shift_uses_egpr_base_index_before_destination_commit() {
    use crate::smir::lower::runtime::ExecMem;

    let case = MemoryCase {
        kind: ShiftKind::Shlx,
        width: OpWidth::W64,
        destination: 17,
        count: 18,
    };
    let bytes = apx_sib_bytes(case, 17, 18, 4);
    let expected_addr = Address::BaseIndexScale {
        base: Some(x86(17)),
        index: x86(18),
        scale: 4,
        disp: -128,
        disp_size: DispSize::Disp8,
    };
    let function = memory_function_from_bytes(case, &bytes, &expected_addr, false);
    for level in LEVELS {
        let function = optimize(function.clone(), level);
        assert_memory_shape(&function.blocks[0].ops, case, &expected_addr);
        let (code, entry) = lower_memory_function(&function);
        let exec = ExecMem::new(&code).expect("map APX EGPR-address BMI2 shift");
        let mut context = MemoryContext {
            value: 0x0123_4567_89AB_CDEF,
            ok: 1,
            ..MemoryContext::default()
        };
        let mut registers = full_guest_regs(0x77);
        registers.gpr[17] = 0x4000;
        registers.gpr[18] = 3;
        registers.ctx = (&mut context as *mut MemoryContext) as u64;
        registers.load_fn = load_helper as usize as u64;
        let mut expected = registers;
        expected.gpr[17] = shifted_value(case.kind, case.width, context.value, registers.gpr[18]);

        exec.run(entry, &mut registers);
        expected.host_mxcsr = registers.host_mxcsr;
        assert_eq!(registers, expected, "{level:?}");
        assert_eq!(context.calls, 1, "{level:?}");
        assert_eq!(context.last_addr, 0x4000 + 3 * 4 - 128, "{level:?}");
        assert_eq!(context.last_size, 8, "{level:?}");
        assert_eq!(context.last_signed, 0, "{level:?}");
    }
}
