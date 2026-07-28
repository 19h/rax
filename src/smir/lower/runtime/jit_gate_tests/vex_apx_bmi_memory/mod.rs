//! Exact helper-backed VEX/APX scalar BMI memory-source coverage.

use super::*;
use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::ops::{OpKind, SmirOp, X86BlsKind, X86OpHint};
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
    x86_native_scalar_feature_requirements_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xB390;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BmiKind {
    Andn,
    Blsr,
    Blsmsk,
    Blsi,
    Bzhi,
    Bextr,
    Pdep,
    Pext,
    Rorx,
}

impl BmiKind {
    const ALL: [Self; 9] = [
        Self::Andn,
        Self::Blsr,
        Self::Blsmsk,
        Self::Blsi,
        Self::Bzhi,
        Self::Bextr,
        Self::Pdep,
        Self::Pext,
        Self::Rorx,
    ];

    fn uses_arch_source(self) -> bool {
        matches!(
            self,
            Self::Andn | Self::Bzhi | Self::Bextr | Self::Pdep | Self::Pext
        )
    }

    fn bls_kind(self) -> Option<X86BlsKind> {
        match self {
            Self::Blsr => Some(X86BlsKind::Blsr),
            Self::Blsmsk => Some(X86BlsKind::Blsmsk),
            Self::Blsi => Some(X86BlsKind::Blsi),
            _ => None,
        }
    }

    fn flags(self, suppressed: bool) -> FlagUpdate {
        if suppressed || matches!(self, Self::Pdep | Self::Pext | Self::Rorx) {
            return FlagUpdate::None;
        }
        match self {
            Self::Bextr => FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF)),
            Self::Andn | Self::Blsr | Self::Blsmsk | Self::Blsi | Self::Bzhi => {
                FlagUpdate::Specific(
                    FlagSet::CF
                        .union(FlagSet::ZF)
                        .union(FlagSet::SF)
                        .union(FlagSet::OF),
                )
            }
            Self::Pdep | Self::Pext | Self::Rorx => FlagUpdate::None,
        }
    }

    fn scalar_feature_requirements(self) -> (bool, bool) {
        match self {
            Self::Bzhi | Self::Pdep | Self::Pext => (true, false),
            Self::Blsr | Self::Blsmsk | Self::Blsi | Self::Bextr => (false, true),
            Self::Andn | Self::Rorx => (false, false),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MemoryBmiCase {
    kind: BmiKind,
    width: OpWidth,
    destination: u8,
    other: u8,
    suppressed: bool,
}

impl MemoryBmiCase {
    /// Encode `kind destination, [RBX], other` (or the exact two-operand form)
    /// with a three-byte VEX prefix.
    fn vex_bytes(self) -> Vec<u8> {
        assert!(self.destination < 16 && self.other < 16, "{self:?}");
        assert!(!self.suppressed, "VEX BMI does not encode APX NF");
        let map = if self.kind == BmiKind::Rorx {
            0x03
        } else {
            0x02
        };
        let destination_in_modrm =
            !matches!(self.kind, BmiKind::Blsr | BmiKind::Blsmsk | BmiKind::Blsi);
        let mut p0 = 0xE0 | map;
        if destination_in_modrm && self.destination >= 8 {
            p0 &= !0x80;
        }
        let w = u8::from(self.width == OpWidth::W64) << 7;
        let (vvvv, pp, opcode, modrm) = match self.kind {
            BmiKind::Andn => (self.other, 0, 0xF2, ((self.destination & 7) << 3) | 3),
            BmiKind::Blsr | BmiKind::Blsmsk | BmiKind::Blsi => {
                let digit = match self.kind {
                    BmiKind::Blsr => 1,
                    BmiKind::Blsmsk => 2,
                    BmiKind::Blsi => 3,
                    _ => unreachable!(),
                };
                (self.destination, 0, 0xF3, (digit << 3) | 3)
            }
            BmiKind::Bzhi => (self.other, 0, 0xF5, ((self.destination & 7) << 3) | 3),
            BmiKind::Bextr => (self.other, 0, 0xF7, ((self.destination & 7) << 3) | 3),
            BmiKind::Pdep => (self.other, 3, 0xF5, ((self.destination & 7) << 3) | 3),
            BmiKind::Pext => (self.other, 2, 0xF5, ((self.destination & 7) << 3) | 3),
            BmiKind::Rorx => (0, 3, 0xF0, ((self.destination & 7) << 3) | 3),
        };
        let p1 = w | (((!vvvv) & 0x0F) << 3) | pp;
        let mut bytes = vec![0xC4, p0, p1, opcode, modrm];
        if self.kind == BmiKind::Rorx {
            bytes.push(0xAD);
        }
        bytes
    }

    fn mem_width(self) -> MemWidth {
        match self.width {
            OpWidth::W32 => MemWidth::B4,
            OpWidth::W64 => MemWidth::B8,
            _ => unreachable!("scalar BMI has only W32/W64 forms"),
        }
    }
}

fn x86(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::gpr(index)))
}

fn function_from_ops(ops: Vec<SmirOp>) -> SmirFunction {
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
}

fn lift_raw(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
    function_from_ops(result.ops)
}

fn lift_case(case: MemoryBmiCase) -> SmirFunction {
    let function = lift_raw(&case.vex_bytes());
    assert_exact_pair(&function.blocks[0].ops, case, &Address::Direct(x86(3)));
    function
}

fn assert_exact_pair(ops: &[SmirOp], case: MemoryBmiCase, expected_addr: &Address) {
    let [load, consumer] = ops else {
        panic!("{case:?}: expected Load + BMI consumer, got {ops:?}")
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
    let expected_dst = x86(case.destination);
    let expected_other = x86(case.other);
    let valid = match (&consumer.kind, case.kind) {
        (
            OpKind::AndNot {
                dst,
                src1,
                src2: SrcOperand::Reg(src2),
                width,
                flags,
            },
            BmiKind::Andn,
        ) => {
            *dst == expected_dst
                && *src1 == temporary
                && *src2 == expected_other
                && *width == case.width
                && *flags == case.kind.flags(case.suppressed)
        }
        (
            OpKind::X86Bls {
                dst,
                src,
                width,
                kind,
                flags,
            },
            BmiKind::Blsr | BmiKind::Blsmsk | BmiKind::Blsi,
        ) => {
            *dst == expected_dst
                && *src == temporary
                && *width == case.width
                && Some(*kind) == case.kind.bls_kind()
                && *flags == case.kind.flags(case.suppressed)
        }
        (
            OpKind::Bzhi {
                dst,
                src,
                index,
                width,
                flags,
            },
            BmiKind::Bzhi,
        ) => {
            *dst == expected_dst
                && *src == temporary
                && *index == expected_other
                && *width == case.width
                && *flags == case.kind.flags(case.suppressed)
        }
        (
            OpKind::Bextr {
                dst,
                src,
                control,
                width,
                flags,
            },
            BmiKind::Bextr,
        ) => {
            *dst == expected_dst
                && *src == temporary
                && *control == expected_other
                && *width == case.width
                && *flags == case.kind.flags(case.suppressed)
        }
        (
            OpKind::Pdep {
                dst,
                src,
                mask,
                width,
            },
            BmiKind::Pdep,
        )
        | (
            OpKind::Pext {
                dst,
                src,
                mask,
                width,
            },
            BmiKind::Pext,
        ) => {
            *dst == expected_dst
                && *src == expected_other
                && *mask == temporary
                && *width == case.width
        }
        (
            OpKind::Ror {
                dst,
                src,
                amount: SrcOperand::Imm(0xAD),
                width,
                flags: FlagUpdate::None,
            },
            BmiKind::Rorx,
        ) => *dst == expected_dst && *src == temporary && *width == case.width,
        _ => false,
    };
    assert!(valid, "{case:?}: unexpected BMI consumer {consumer:?}");
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
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed BMI lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer.finalize().expect("finalize helper-backed BMI"),
        result.entry_offset,
    )
}

fn manual_function(case: MemoryBmiCase, addr: Address) -> SmirFunction {
    let temporary = VReg::Virtual(VirtualId(0xB4));
    let consumer = match case.kind {
        BmiKind::Andn => OpKind::AndNot {
            dst: x86(case.destination),
            src1: temporary,
            src2: SrcOperand::Reg(x86(case.other)),
            width: case.width,
            flags: case.kind.flags(case.suppressed),
        },
        BmiKind::Blsr | BmiKind::Blsmsk | BmiKind::Blsi => OpKind::X86Bls {
            dst: x86(case.destination),
            src: temporary,
            width: case.width,
            kind: case.kind.bls_kind().unwrap(),
            flags: case.kind.flags(case.suppressed),
        },
        BmiKind::Bzhi => OpKind::Bzhi {
            dst: x86(case.destination),
            src: temporary,
            index: x86(case.other),
            width: case.width,
            flags: case.kind.flags(case.suppressed),
        },
        BmiKind::Bextr => OpKind::Bextr {
            dst: x86(case.destination),
            src: temporary,
            control: x86(case.other),
            width: case.width,
            flags: case.kind.flags(case.suppressed),
        },
        BmiKind::Pdep => OpKind::Pdep {
            dst: x86(case.destination),
            src: x86(case.other),
            mask: temporary,
            width: case.width,
        },
        BmiKind::Pext => OpKind::Pext {
            dst: x86(case.destination),
            src: x86(case.other),
            mask: temporary,
            width: case.width,
        },
        BmiKind::Rorx => OpKind::Ror {
            dst: x86(case.destination),
            src: temporary,
            amount: SrcOperand::Imm(0xAD),
            width: case.width,
            flags: FlagUpdate::None,
        },
    };
    function_from_ops(vec![
        SmirOp::new(
            OpId(0),
            PC,
            OpKind::Load {
                dst: temporary,
                addr,
                width: case.mem_width(),
                sign: SignExtend::Zero,
            },
        ),
        SmirOp::new(OpId(1), PC, consumer),
    ])
}

fn assert_rejected(name: &str, function: &SmirFunction) {
    assert!(
        !is_native_clobber_safe_excluding(function, &std::collections::HashMap::new(), true),
        "{name}: gate admitted malformed pair"
    );
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    assert!(
        lowerer.lower_function(function).is_err(),
        "{name}: lowerer accepted malformed pair"
    );
}

#[test]
fn all_2688_vex_memory_bmi_register_width_shapes_are_admitted_and_lowerable() {
    let mut lifted = 0usize;
    let mut lowered = 0usize;
    for kind in BmiKind::ALL {
        for width in [OpWidth::W32, OpWidth::W64] {
            for destination in 0..16 {
                let other_limit = if kind.uses_arch_source() { 16 } else { 1 };
                for other in 0..other_limit {
                    let case = MemoryBmiCase {
                        kind,
                        width,
                        destination,
                        other,
                        suppressed: false,
                    };
                    let function = lift_case(case);
                    lifted += 1;
                    for level in LEVELS {
                        let function = optimize(function.clone(), level);
                        assert_exact_pair(&function.blocks[0].ops, case, &Address::Direct(x86(3)));
                        assert!(!lower(&function).0.is_empty(), "{level:?} {case:?}");
                        lowered += 1;
                    }
                }
            }
        }
    }
    assert_eq!(lifted, 2 * (5 * 16 * 16 + 4 * 16));
    assert_eq!(lowered, lifted * LEVELS.len());
}

#[test]
fn representative_apx_memory_forms_enter_the_same_exact_pair_lowerer() {
    // LLVM 23 accepts each APX encoding. The set covers NF=0/NF=1, every
    // consumer kind, EGPR operands, EGPR SIB components, and FS/GS segments.
    let forms: &[(&str, &[u8])] = &[
        ("andn", &[0x62, 0x72, 0xFC, 0x08, 0xF2, 0x03]),
        ("blsr", &[0x62, 0xF2, 0xBC, 0x08, 0xF3, 0x0B]),
        ("blsmsk", &[0x62, 0xF2, 0xBC, 0x08, 0xF3, 0x13]),
        ("blsi", &[0x62, 0xF2, 0xBC, 0x08, 0xF3, 0x1B]),
        ("bzhi NF", &[0x62, 0x72, 0xF4, 0x0C, 0xF5, 0x03]),
        ("bextr", &[0x62, 0x72, 0xF4, 0x08, 0xF7, 0x03]),
        (
            "pdep FS EGPR SIB",
            &[0x64, 0x62, 0xEA, 0xE3, 0x00, 0xF5, 0x24, 0x91],
        ),
        (
            "pext EGPR SIB",
            &[0x62, 0xEA, 0xE2, 0x00, 0xF5, 0x64, 0x91, 0x20],
        ),
        (
            "rorx GS EGPR SIB",
            &[0x65, 0x62, 0xEB, 0xFB, 0x08, 0xF0, 0x64, 0x91, 0x20, 0x0D],
        ),
    ];

    for (name, bytes) in forms {
        let function = lift_raw(bytes);
        assert_eq!(function.blocks[0].ops.len(), 2, "{name}");
        for level in LEVELS {
            let function = optimize(function.clone(), level);
            assert!(
                is_native_clobber_safe_excluding(
                    &function,
                    &std::collections::HashMap::new(),
                    true
                ),
                "{level:?} {name}"
            );
            assert!(!lower(&function).0.is_empty(), "{level:?} {name}");
        }
    }
}

#[test]
fn bmi_memory_feature_requirements_are_accumulated_from_fused_consumers() {
    for kind in BmiKind::ALL {
        let function = manual_function(
            MemoryBmiCase {
                kind,
                width: OpWidth::W64,
                destination: 20,
                other: 19,
                suppressed: kind == BmiKind::Bzhi,
            },
            Address::Direct(x86(17)),
        );
        let (bmi2, bmi1, lzcnt, popcnt, adx) = x86_native_scalar_feature_requirements_excluding(
            &function,
            &std::collections::HashMap::new(),
        );
        assert_eq!((bmi2, bmi1), kind.scalar_feature_requirements(), "{kind:?}");
        assert!(!lzcnt && !popcnt && !adx, "{kind:?}");
    }
}

#[test]
fn malformed_bmi_memory_pairs_fail_closed_before_emission() {
    let exact = manual_function(
        MemoryBmiCase {
            kind: BmiKind::Andn,
            width: OpWidth::W64,
            destination: 20,
            other: 19,
            suppressed: false,
        },
        Address::Direct(x86(17)),
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
    malformed.push(("load/consumer width mismatch", case));

    let mut case = exact.clone();
    case.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    malformed.push(("load hint", case));

    let mut case = exact.clone();
    case.blocks[0].ops[1].x86_hint = Some(X86OpHint::Mulx);
    malformed.push(("consumer hint", case));

    let mut case = exact.clone();
    case.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("different guest PC", case));

    let mut case = exact.clone();
    case.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC,
        OpKind::Mov {
            dst: x86(0),
            src: SrcOperand::Reg(VReg::Virtual(VirtualId(0xB4))),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("extra temporary use", case));

    let mut case = exact.clone();
    case.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC,
        OpKind::Mov {
            dst: VReg::Virtual(VirtualId(0xB4)),
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
    malformed.push(("invalid address scale", case));

    let mut case = exact.clone();
    if let OpKind::AndNot { flags, .. } = &mut case.blocks[0].ops[1].kind {
        *flags = FlagUpdate::All;
    }
    malformed.push(("inexact ANDN flags", case));

    for (name, function) in malformed {
        assert_rejected(name, &function);
    }

    for kind in BmiKind::ALL {
        let descriptor = MemoryBmiCase {
            kind,
            width: OpWidth::W64,
            destination: 20,
            other: 19,
            suppressed: matches!(kind, BmiKind::Andn | BmiKind::Blsr | BmiKind::Bzhi),
        };
        let mut function = manual_function(descriptor, Address::Direct(x86(17)));
        match &mut function.blocks[0].ops[1].kind {
            OpKind::AndNot { src1, .. }
            | OpKind::X86Bls { src: src1, .. }
            | OpKind::Bzhi { src: src1, .. }
            | OpKind::Bextr { src: src1, .. }
            | OpKind::Ror { src: src1, .. } => *src1 = VReg::Virtual(VirtualId(0xEE)),
            OpKind::Pdep { mask, .. } | OpKind::Pext { mask, .. } => {
                *mask = VReg::Virtual(VirtualId(0xEE))
            }
            _ => unreachable!(),
        }
        assert_rejected(&format!("{kind:?} wrong temporary role"), &function);
    }

    let mut immediate_bextr = manual_function(
        MemoryBmiCase {
            kind: BmiKind::Bextr,
            width: OpWidth::W64,
            destination: 20,
            other: 19,
            suppressed: false,
        },
        Address::Direct(x86(17)),
    );
    if let OpKind::Bextr { control, .. } = &mut immediate_bextr.blocks[0].ops[1].kind {
        *control = VReg::Imm(0x0808);
    }
    assert_rejected(
        "immediate-control BEXTR is not a VEX/APX pair",
        &immediate_bextr,
    );

    for amount in [-1, 256] {
        let mut rorx = manual_function(
            MemoryBmiCase {
                kind: BmiKind::Rorx,
                width: OpWidth::W64,
                destination: 20,
                other: 0,
                suppressed: true,
            },
            Address::Direct(x86(17)),
        );
        if let OpKind::Ror { amount: value, .. } = &mut rorx.blocks[0].ops[1].kind {
            *value = SrcOperand::Imm(amount);
        }
        assert_rejected(&format!("RORX immediate {amount}"), &rorx);
    }
}

#[test]
fn helper_backed_bmi_emits_independently_decoded_scratch_cores() {
    // LLVM 23 independently decodes these exact scratch-register cores:
    //   C4 E2 E8 F3 CF       blsr rdx,rdi
    //   C4 E2 B8 F5 D7       bzhi rdx,rdi,r8
    //   C4 E2 B8 F7 D7       bextr rdx,rdi,r8
    //   C4 C2 C3 F5 D0       pdep rdx,rdi,r8
    //   C4 C2 C2 F5 D0       pext rdx,rdi,r8
    //   48 C1 CA AD          ror rdx,0xad
    let cores: &[(BmiKind, &[u8])] = &[
        (BmiKind::Blsr, &[0xC4, 0xE2, 0xE8, 0xF3, 0xCF]),
        (BmiKind::Bzhi, &[0xC4, 0xE2, 0xB8, 0xF5, 0xD7]),
        (BmiKind::Bextr, &[0xC4, 0xE2, 0xB8, 0xF7, 0xD7]),
        (BmiKind::Pdep, &[0xC4, 0xC2, 0xC3, 0xF5, 0xD0]),
        (BmiKind::Pext, &[0xC4, 0xC2, 0xC2, 0xF5, 0xD0]),
        (BmiKind::Rorx, &[0x48, 0xC1, 0xCA, 0xAD]),
    ];
    for (kind, core) in cores {
        let function = manual_function(
            MemoryBmiCase {
                kind: *kind,
                width: OpWidth::W64,
                destination: 20,
                other: 19,
                suppressed: false,
            },
            Address::Direct(x86(17)),
        );
        let (code, _) = lower(&function);
        assert!(
            code.windows(core.len()).any(|window| window == *core),
            "{kind:?}: missing core {core:02X?} in {code:02X?}"
        );
    }

    let andn = lower(&manual_function(
        MemoryBmiCase {
            kind: BmiKind::Andn,
            width: OpWidth::W64,
            destination: 20,
            other: 19,
            suppressed: false,
        },
        Address::Direct(x86(17)),
    ))
    .0;
    for core in [
        &[0x4C, 0x89, 0xC2][..],
        &[0x48, 0xF7, 0xD2][..],
        &[0x48, 0x21, 0xFA][..],
    ] {
        assert!(
            andn.windows(core.len()).any(|window| window == core),
            "ANDN missing scalar core {core:02X?} in {andn:02X?}"
        );
    }
}

#[cfg(target_arch = "x86_64")]
mod native;
