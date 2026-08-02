//! Exact helper-backed VEX BMI2 memory-source variable-shift coverage.

use super::*;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, OpWidth, SignExtend,
    SrcOperand, VReg, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xB280;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

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

    fn digit(self) -> u8 {
        match self {
            Self::Shlx => 4,
            Self::Shrx => 5,
            Self::Sarx => 7,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MemoryShiftCase {
    kind: ShiftKind,
    width: OpWidth,
    destination: u8,
    count: u8,
}

impl MemoryShiftCase {
    /// Encode `SHLX`/`SHRX`/`SARX destination,[RBX],count`.
    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 16 && self.count < 16, "{self:?}");
        let mut p0 = 0xE2; // X'=B'=1, map 0F38.
        if self.destination >= 8 {
            p0 &= !0x80;
        }
        vec![
            0xC4,
            p0,
            (u8::from(self.width == OpWidth::W64) << 7)
                | (((!self.count) & 0x0F) << 3)
                | self.kind.pp(),
            0xF7,
            ((self.destination & 7) << 3) | 3,
        ]
    }

    fn mem_width(self) -> MemWidth {
        match self.width {
            OpWidth::W32 => MemWidth::B4,
            OpWidth::W64 => MemWidth::B8,
            _ => unreachable!("VEX BMI2 shifts have only W32/W64 forms"),
        }
    }

    fn needs_state_bridge(self) -> bool {
        [self.destination, self.count]
            .into_iter()
            .any(|index| matches!(index, 4 | 5))
    }
}

fn x86(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::gpr(index)))
}

fn lift_case(case: MemoryShiftCase) -> SmirFunction {
    lift_bytes(case, &case.bytes(), &Address::Direct(x86(3)))
}

fn lift_bytes(case: MemoryShiftCase, bytes: &[u8], expected_addr: &Address) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{case:?} {bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    assert_exact_pair(&result.ops, case, expected_addr);

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
}

fn assert_exact_pair(ops: &[SmirOp], case: MemoryShiftCase, expected_addr: &Address) {
    let [load, consumer] = ops else {
        panic!("{case:?}: expected Load + variable shift, got {ops:?}")
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
        other => panic!("{case:?}: expected exact scalar load, got {other:?}"),
    };
    assert_eq!(consumer.guest_pc, load.guest_pc, "{case:?}");
    assert_eq!(consumer.x86_hint, None, "{case:?}");
    let valid = match (&consumer.kind, case.kind) {
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
    assert!(valid, "{case:?}: unexpected consumer {consumer:?}");
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn lower(function: &SmirFunction) -> (Vec<u8>, usize) {
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

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed BMI2 shift lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize helper-backed BMI2 shift"),
        result.entry_offset,
    )
}

#[test]
fn all_1536_vex_memory_destination_count_kind_width_shapes_are_admitted_and_lowerable() {
    let mut lifted = 0usize;
    let mut lowered = 0usize;
    for kind in ShiftKind::ALL {
        for width in [OpWidth::W32, OpWidth::W64] {
            for destination in 0..16 {
                for count in 0..16 {
                    let case = MemoryShiftCase {
                        kind,
                        width,
                        destination,
                        count,
                    };
                    let function = lift_case(case);
                    lifted += 1;
                    for level in LEVELS {
                        let function = optimize(function.clone(), level);
                        assert_exact_pair(&function.blocks[0].ops, case, &Address::Direct(x86(3)));
                        let (code, _) = lower(&function);
                        assert!(!code.is_empty(), "{level:?} {case:?}");
                        lowered += 1;
                    }
                }
            }
        }
    }
    assert_eq!(lifted, 3 * 2 * 16 * 16);
    assert_eq!(lowered, lifted * LEVELS.len());
}

fn vex_sib_bytes(case: MemoryShiftCase, base: u8, index: u8, scale: u8) -> Vec<u8> {
    assert!(base < 16 && index < 16 && index != 4);
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
    vec![
        0xC4,
        p0,
        (u8::from(case.width == OpWidth::W64) << 7)
            | (((!case.count) & 0x0F) << 3)
            | case.kind.pp(),
        0xF7,
        0x40 | ((case.destination & 7) << 3) | 4,
        ((scale.trailing_zeros() as u8) << 6) | ((index & 7) << 3) | (base & 7),
        0x80,
    ]
}

#[test]
fn all_1920_vex_base_index_scale_address_encodings_lift_and_lower_exactly() {
    let mut count = 0usize;
    for width in [OpWidth::W32, OpWidth::W64] {
        let case = MemoryShiftCase {
            kind: ShiftKind::Shlx,
            width,
            destination: 8,
            count: 10,
        };
        for base in 0..16 {
            for index in (0..16).filter(|index| *index != 4) {
                for scale in [1, 2, 4, 8] {
                    let expected = Address::BaseIndexScale {
                        base: Some(x86(base)),
                        index: x86(index),
                        scale,
                        disp: -128,
                        disp_size: DispSize::Disp8,
                    };
                    let function =
                        lift_bytes(case, &vex_sib_bytes(case, base, index, scale), &expected);
                    let (code, _) = lower(&function);
                    assert!(
                        !code.is_empty(),
                        "{case:?} base={base} index={index} scale={scale}"
                    );
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 2 * 16 * 15 * 4);
}

#[test]
fn memory_bmi2_shifts_lift_special_x86_address_classes_exactly() {
    let cases = [
        (
            "FS plus addr32",
            MemoryShiftCase {
                kind: ShiftKind::Shlx,
                width: OpWidth::W64,
                destination: 0,
                count: 1,
            },
            vec![0x64, 0x67, 0xC4, 0xE2, 0xF1, 0xF7, 0x44, 0x93, 0x20],
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                base: Some(x86(3)),
                index: Some(x86(2)),
                scale: 4,
                disp: 0x20,
            })),
        ),
        (
            "GS-relative",
            MemoryShiftCase {
                kind: ShiftKind::Sarx,
                width: OpWidth::W64,
                destination: 0,
                count: 1,
            },
            vec![0x65, 0xC4, 0xE2, 0xF2, 0xF7, 0x44, 0x93, 0x20],
            Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::GsBase)),
                base: Some(x86(3)),
                index: Some(x86(2)),
                scale: 4,
                disp: 0x20,
            },
        ),
        (
            "RIP-relative",
            MemoryShiftCase {
                kind: ShiftKind::Shrx,
                width: OpWidth::W64,
                destination: 0,
                count: 1,
            },
            vec![0xC4, 0xE2, 0xF3, 0xF7, 0x05, 0xFC, 0xFF, 0xFF, 0xFF],
            Address::PcRel {
                offset: -4,
                disp_size: DispSize::Disp32,
                base: Some(PC + 9),
            },
        ),
        (
            "SIB no base",
            MemoryShiftCase {
                kind: ShiftKind::Sarx,
                width: OpWidth::W64,
                destination: 0,
                count: 1,
            },
            vec![0xC4, 0xE2, 0xF2, 0xF7, 0x04, 0x95, 0x78, 0x56, 0x34, 0x12],
            Address::BaseIndexScale {
                base: None,
                index: x86(2),
                scale: 4,
                disp: 0x1234_5678,
                disp_size: DispSize::Disp32,
            },
        ),
    ];

    for (name, case, bytes, expected_addr) in cases {
        let function = lift_bytes(case, &bytes, &expected_addr);
        let (code, _) = lower(&function);
        assert!(!code.is_empty(), "{name}");
    }
}

#[test]
fn memory_bmi2_shift_emits_independently_decoded_direct_and_state_bridge_cores() {
    // LLVM 23 independently decodes the asserted classic shift cores as:
    //   48 D3 7C 24 08  -> sar qword ptr [rsp + 8], cl
    //   D3 64 24 10     -> shl dword ptr [rsp + 16], cl
    //   48 D3 6C 24 10  -> shr qword ptr [rsp + 16], cl
    for (case, core) in [
        (
            MemoryShiftCase {
                kind: ShiftKind::Sarx,
                width: OpWidth::W64,
                destination: 7,
                count: 1,
            },
            &[0x48, 0xD3, 0x7C, 0x24, 0x08][..],
        ),
        (
            MemoryShiftCase {
                kind: ShiftKind::Shlx,
                width: OpWidth::W32,
                destination: 8,
                count: 10,
            },
            &[0xD3, 0x64, 0x24, 0x10][..],
        ),
        (
            MemoryShiftCase {
                kind: ShiftKind::Shrx,
                width: OpWidth::W64,
                destination: 4,
                count: 5,
            },
            &[0x48, 0xD3, 0x6C, 0x24, 0x10][..],
        ),
    ] {
        assert_eq!(
            case.needs_state_bridge(),
            matches!(case.destination, 4 | 5) || matches!(case.count, 4 | 5)
        );
        let (code, _) = lower(&lift_case(case));
        assert!(
            code.windows(core.len()).any(|window| window == core),
            "{case:?}: missing classic shift core {core:02X?} in {code:02X?}"
        );
    }
}

fn manual_function(addr: Address, width: OpWidth, dst: VReg, count: SrcOperand) -> SmirFunction {
    let temporary = VReg::Virtual(VirtualId(0xB3));
    let mem_width = match width {
        OpWidth::W16 => MemWidth::B2,
        OpWidth::W32 => MemWidth::B4,
        OpWidth::W64 => MemWidth::B8,
        _ => MemWidth::B1,
    };
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = vec![
        SmirOp::new(
            OpId(0),
            PC,
            OpKind::Load {
                dst: temporary,
                addr,
                width: mem_width,
                sign: SignExtend::Zero,
            },
        ),
        SmirOp::new(
            OpId(1),
            PC,
            OpKind::Shl {
                dst,
                src: temporary,
                amount: count,
                width,
                flags: FlagUpdate::None,
            },
        ),
    ];
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
}

#[test]
fn malformed_memory_bmi2_shift_pairs_fail_closed_and_apx_shape_is_admitted() {
    let exact = manual_function(
        Address::Direct(x86(3)),
        OpWidth::W64,
        x86(1),
        SrcOperand::Reg(x86(7)),
    );
    let mut malformed = Vec::new();

    let mut case = exact.clone();
    if let OpKind::Load { sign, .. } = &mut case.blocks[0].ops[0].kind {
        *sign = SignExtend::Sign;
    }
    malformed.push(("signed load", case));

    let mut case = exact.clone();
    if let OpKind::Load { width, .. } = &mut case.blocks[0].ops[0].kind {
        *width = MemWidth::B4;
    }
    malformed.push(("load/operation width mismatch", case));

    let mut case = exact.clone();
    case.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    malformed.push(("load hint", case));

    let mut case = exact.clone();
    case.blocks[0].ops[1].x86_hint = Some(X86OpHint::Mulx);
    malformed.push(("consumer hint", case));

    let mut case = exact.clone();
    if let OpKind::Shl { flags, .. } = &mut case.blocks[0].ops[1].kind {
        *flags = FlagUpdate::All;
    }
    malformed.push(("flag update", case));

    let mut case = exact.clone();
    if let OpKind::Shl { src, .. } = &mut case.blocks[0].ops[1].kind {
        *src = VReg::Virtual(VirtualId(9));
    }
    malformed.push(("different temporary", case));

    let mut case = exact.clone();
    if let OpKind::Shl { amount, .. } = &mut case.blocks[0].ops[1].kind {
        *amount = SrcOperand::Imm(3);
    }
    malformed.push(("immediate count", case));

    let mut case = exact.clone();
    if let OpKind::Shl { amount, .. } = &mut case.blocks[0].ops[1].kind {
        *amount = SrcOperand::Reg(VReg::Virtual(VirtualId(8)));
    }
    malformed.push(("virtual count", case));

    let mut case = exact.clone();
    let OpKind::Shl {
        dst,
        src,
        amount,
        width,
        flags,
    } = case.blocks[0].ops[1].kind.clone()
    else {
        unreachable!()
    };
    case.blocks[0].ops[1].kind = OpKind::Ror {
        dst,
        src,
        amount,
        width,
        flags,
    };
    malformed.push(("RORX consumer", case));

    let mut case = exact.clone();
    case.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("different guest PC", case));

    let mut case = exact.clone();
    case.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC,
        OpKind::Mov {
            dst: x86(0),
            src: SrcOperand::Reg(VReg::Virtual(VirtualId(0xB3))),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("extra temporary use", case));

    let mut case = exact.clone();
    case.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xB3)),
            src: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("duplicate temporary definition", case));

    let mut case = exact.clone();
    if let OpKind::Load { addr, .. } = &mut case.blocks[0].ops[0].kind {
        *addr = Address::BaseIndexScale {
            base: Some(x86(3)),
            index: x86(1),
            scale: 3,
            disp: 0,
            disp_size: DispSize::Auto,
        };
    }
    malformed.push(("invalid scale", case));

    let mut case = exact.clone();
    if let OpKind::Load { addr, .. } = &mut case.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(7)));
    }
    malformed.push(("virtual address", case));

    let mut case = exact.clone();
    if let OpKind::Load { addr, .. } = &mut case.blocks[0].ops[0].kind {
        *addr = Address::X86Addr32(Box::new(Address::X86Addr32(Box::new(Address::Direct(
            x86(3),
        )))));
    }
    malformed.push(("nested addr32", case));

    malformed.push((
        "unsupported W16",
        manual_function(
            Address::Direct(x86(3)),
            OpWidth::W16,
            x86(1),
            SrcOperand::Reg(x86(7)),
        ),
    ));

    for (name, function) in malformed {
        assert!(
            !is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), true,),
            "{name}: gate admitted malformed pair"
        );
        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        assert!(
            lowerer.lower_function(&function).is_err(),
            "{name}: lowerer accepted malformed pair"
        );
    }

    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let apx = lifter
        .lift_insn(
            PC,
            // LLVM 23: `{evex} shlx rax, qword ptr [rbx], rcx`.
            &[0x62, 0xF2, 0xF5, 0x08, 0xF7, 0x03],
            &mut context,
        )
        .expect("lift APX memory SHLX");
    assert_eq!(
        apx.ops.len(),
        3,
        "APX uses canonical requirement + Load + shift shape"
    );
    assert!(
        matches!(apx.ops[0].kind, OpKind::X86RequireApx),
        "APX requirement must precede the memory operation"
    );
    assert_exact_pair(
        &apx.ops[1..],
        MemoryShiftCase {
            kind: ShiftKind::Shlx,
            width: OpWidth::W64,
            destination: 0,
            count: 1,
        },
        &Address::Direct(x86(3)),
    );
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = apx.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    for level in LEVELS {
        let function = optimize(function.clone(), level);
        assert!(
            is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), true),
            "{level:?}: APX canonical memory shift must enter the helper-backed gate"
        );
        let (code, _) = lower(&function);
        assert!(!code.is_empty(), "{level:?}: APX memory shift lowering");
    }
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
fn expected_shift(kind: ShiftKind, width: OpWidth, source: u64, count: u64) -> u64 {
    let amount = (count & if width == OpWidth::W64 { 0x3F } else { 0x1F }) as u32;
    match (kind, width) {
        (ShiftKind::Shlx, OpWidth::W32) => u64::from((source as u32) << amount),
        (ShiftKind::Shlx, OpWidth::W64) => source << amount,
        (ShiftKind::Shrx, OpWidth::W32) => u64::from((source as u32) >> amount),
        (ShiftKind::Shrx, OpWidth::W64) => source >> amount,
        (ShiftKind::Sarx, OpWidth::W32) => u64::from(((source as u32 as i32) >> amount) as u32),
        (ShiftKind::Sarx, OpWidth::W64) => ((source as i64) >> amount) as u64,
        _ => unreachable!("VEX BMI2 shifts have only W32/W64 forms"),
    }
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
fn native_memory_bmi2_shifts_are_fault_precise_and_preserve_complete_guest_state() {
    use crate::smir::lower::runtime::ExecMem;

    const TUPLES: [(u8, u8); 12] = [
        (0, 1),
        (8, 10),
        (15, 13),
        (1, 1),
        (1, 3),
        (3, 1),
        (3, 3),
        (4, 5),
        (5, 4),
        (4, 4),
        (5, 5),
        (1, 4),
    ];
    const SOURCES: [u64; 6] = [
        0,
        1,
        0x0000_0000_8000_0001,
        0x0000_0000_FFFF_FFFF,
        0x8000_0000_0000_0001,
        0xFEDC_BA98_7654_3210,
    ];
    const COUNTS: [u64; 10] = [0, 1, 2, 31, 32, 33, 63, 64, 65, u64::MAX];

    let mut successes = 0usize;
    let mut faults = 0usize;
    for kind in ShiftKind::ALL {
        for width in [OpWidth::W32, OpWidth::W64] {
            for (destination, count) in TUPLES {
                let case = MemoryShiftCase {
                    kind,
                    width,
                    destination,
                    count,
                };
                for level in LEVELS {
                    let function = optimize(lift_case(case), level);
                    let (code, entry) = lower(&function);
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
                                expected_shift(kind, width, source, count_value);

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
                    faults += 1;
                }
            }
        }
    }

    eprintln!(
        "executed {successes} successful and {faults} faulting native memory BMI2 shift cases"
    );
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
fn native_memory_bmi2_shifts_compute_addr32_segment_rip_and_no_base_addresses() {
    use crate::smir::lower::runtime::ExecMem;

    let cases = [
        (
            "FS plus addr32",
            MemoryShiftCase {
                kind: ShiftKind::Shlx,
                width: OpWidth::W64,
                destination: 0,
                count: 1,
            },
            vec![0x64, 0x67, 0xC4, 0xE2, 0xF1, 0xF7, 0x44, 0x93, 0x20],
            Address::X86Addr32(Box::new(Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                base: Some(x86(3)),
                index: Some(x86(2)),
                scale: 4,
                disp: 0x20,
            })),
            0x1234_5678_0000_0000,
            0,
            0xAAAA_BBBB_FFFF_FFF0,
            8,
            0x1234_5678_0000_0030,
        ),
        (
            "GS-relative",
            MemoryShiftCase {
                kind: ShiftKind::Sarx,
                width: OpWidth::W64,
                destination: 0,
                count: 1,
            },
            vec![0x65, 0xC4, 0xE2, 0xF2, 0xF7, 0x44, 0x93, 0x20],
            Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::GsBase)),
                base: Some(x86(3)),
                index: Some(x86(2)),
                scale: 4,
                disp: 0x20,
            },
            0,
            0x2000_0000_0000_0000,
            0x1000,
            0x20,
            0x2000_0000_0000_10A0,
        ),
        (
            "RIP-relative",
            MemoryShiftCase {
                kind: ShiftKind::Shrx,
                width: OpWidth::W64,
                destination: 0,
                count: 1,
            },
            vec![0xC4, 0xE2, 0xF3, 0xF7, 0x05, 0xFC, 0xFF, 0xFF, 0xFF],
            Address::PcRel {
                offset: -4,
                disp_size: DispSize::Disp32,
                base: Some(PC + 9),
            },
            0,
            0,
            0,
            0,
            PC + 5,
        ),
        (
            "SIB no base",
            MemoryShiftCase {
                kind: ShiftKind::Sarx,
                width: OpWidth::W64,
                destination: 0,
                count: 1,
            },
            vec![0xC4, 0xE2, 0xF2, 0xF7, 0x04, 0x95, 0x78, 0x56, 0x34, 0x12],
            Address::BaseIndexScale {
                base: None,
                index: x86(2),
                scale: 4,
                disp: 0x1234_5678,
                disp_size: DispSize::Disp32,
            },
            0,
            0,
            0,
            0x10,
            0x1234_56B8,
        ),
    ];

    for (name, case, bytes, expected_shape, fs_base, gs_base, rbx, rdx, expected_addr) in cases {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_bytes(case, &bytes, &expected_shape), level);
            let (code, entry) = lower(&function);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{name} {level:?}: {error:?}"));
            let mut context = MemoryContext {
                value: 0xFEDC_BA98_7654_3210,
                ok: 1,
                ..MemoryContext::default()
            };
            let mut registers = full_guest_regs(0x66);
            registers.gpr[1] = 5;
            registers.gpr[2] = rdx;
            registers.gpr[3] = rbx;
            registers.fs_base = fs_base;
            registers.gs_base = gs_base;
            registers.ctx = (&mut context as *mut MemoryContext) as u64;
            registers.load_fn = load_helper as usize as u64;
            let mut expected = registers;
            expected.gpr[0] = expected_shift(case.kind, case.width, context.value, 5);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{name} {level:?}");
            assert_eq!(context.calls, 1, "{name} {level:?}");
            assert_eq!(context.last_addr, expected_addr, "{name} {level:?}");
            assert_eq!(context.last_size, 8, "{name} {level:?}");
            assert_eq!(context.last_signed, 0, "{name} {level:?}");
        }
    }
}
