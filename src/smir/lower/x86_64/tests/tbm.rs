//! Native x86-64 lowering coverage for AMD TBM semantics and its live guard.

use super::*;
use crate::smir::OpId;
use crate::smir::ir::ops::X86TbmKind;
use crate::smir::lower::x86_64::{
    x86_state_backed_gpr_tbm_candidate, x86_state_backed_gpr_tbm_valid,
};
use crate::smir::lower::{
    X86_GUEST_CPUID_TBM_OFFSET, X86_GUEST_CR0_OFFSET, X86_GUEST_RFLAGS_OFFSET,
};

const CF: u64 = 1 << 0;
const PF: u64 = 1 << 2;
const AF: u64 = 1 << 4;
const ZF: u64 = 1 << 6;
const SF: u64 = 1 << 7;
const OF: u64 = 1 << 11;
const DEFINED: FlagSet = FlagSet::CF
    .union(FlagSet::ZF)
    .union(FlagSet::SF)
    .union(FlagSet::OF);

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn tbm_op(kind: X86TbmKind, width: OpWidth, flags: FlagUpdate) -> OpKind {
    OpKind::X86Tbm {
        dst: x86(X86Reg::Rax),
        src: x86(X86Reg::Rcx),
        width,
        kind,
        flags,
    }
}

fn lower_guard(fault_guards: bool, hinted: bool) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x2345, OpKind::X86RequireTbm);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    if hinted {
        function.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    }

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    let lowered = lowerer.lower_function(&function)?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn tbm_guard_requires_deoptimization_and_encodes_state_and_fault_pc() {
    assert!(matches!(
        lower_guard(false, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    assert!(matches!(
        lower_guard(true, true),
        Err(LowerError::InvalidOperand { .. })
    ));

    let (code, _) = lower_guard(true, false).expect("lower exact TBM guard");
    assert!(
        code.windows(4)
            .any(|window| window == (X86_GUEST_CPUID_TBM_OFFSET as u32).to_le_bytes()),
        "missing TBM feature-state displacement: {code:02X?}"
    );
    for (name, offset) in [
        ("CR0", X86_GUEST_CR0_OFFSET),
        ("RFLAGS", X86_GUEST_RFLAGS_OFFSET),
    ] {
        assert!(
            code.windows(4)
                .any(|window| window == (offset as u32).to_le_bytes()),
            "missing TBM {name} mode-state displacement: {code:02X?}"
        );
    }
    assert!(
        code.windows(4)
            .any(|window| window == 0x2345_u32.to_le_bytes()),
        "missing exact deoptimization PC: {code:02X?}"
    );
}

#[test]
fn tbm_lowering_accepts_only_the_exact_scalar_and_flag_contract() {
    for kind in [
        X86TbmKind::Blcfill,
        X86TbmKind::Blci,
        X86TbmKind::Blcic,
        X86TbmKind::Blcmsk,
        X86TbmKind::Blcs,
        X86TbmKind::Blsfill,
        X86TbmKind::Blsic,
        X86TbmKind::T1mskc,
        X86TbmKind::Tzmsk,
    ] {
        for width in [OpWidth::W32, OpWidth::W64] {
            let code = lower_single_op(tbm_op(kind, width, FlagUpdate::Specific(DEFINED)));
            assert!(!code.is_empty(), "{kind:?}, {width:?}");
        }
    }

    for (name, kind) in [
        (
            "unsupported width",
            OpKind::X86Tbm {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rcx),
                width: OpWidth::W16,
                kind: X86TbmKind::Blcfill,
                flags: FlagUpdate::Specific(DEFINED),
            },
        ),
        (
            "partial flag set",
            OpKind::X86Tbm {
                dst: x86(X86Reg::Rsp),
                src: x86(X86Reg::Rcx),
                width: OpWidth::W64,
                kind: X86TbmKind::Blcfill,
                flags: FlagUpdate::Specific(FlagSet::CF),
            },
        ),
    ] {
        let op = SmirOp::new(OpId(0), 0x1000, kind);
        assert!(x86_state_backed_gpr_tbm_candidate(&op), "{name}");
        assert!(!x86_state_backed_gpr_tbm_valid(&op), "{name}");
        let error = lower_single_op_err(op.kind);
        assert!(
            matches!(
                error,
                LowerError::InvalidOperand { .. }
                    | LowerError::InvalidRegister(_)
                    | LowerError::UnsupportedOp { .. }
            ),
            "{name}: {error:?}"
        );
    }
}

fn reference(kind: X86TbmKind, src: u64, mask: u64) -> (u64, bool) {
    let src = src & mask;
    let incremented = src.wrapping_add(1) & mask;
    let decremented = src.wrapping_sub(1) & mask;
    let result = match kind {
        X86TbmKind::Blcfill => src & incremented,
        X86TbmKind::Blci => src | !incremented,
        X86TbmKind::Blcic => !src & incremented,
        X86TbmKind::Blcmsk => src ^ incremented,
        X86TbmKind::Blcs => src | incremented,
        X86TbmKind::Blsfill => src | decremented,
        X86TbmKind::Blsic => !src | decremented,
        X86TbmKind::T1mskc => !src | incremented,
        X86TbmKind::Tzmsk => !src & decremented,
    } & mask;
    let carry = if matches!(
        kind,
        X86TbmKind::Blsfill | X86TbmKind::Blsic | X86TbmKind::Tzmsk
    ) {
        src == 0
    } else {
        src == mask
    };
    (result, carry)
}

fn bextr_reference(src: u64, control: u64, width: OpWidth) -> u64 {
    let bits = width.bits();
    let start = (control & 0xff) as u32;
    let length = ((control >> 8) & 0xff) as u32;
    if start >= bits || length == 0 {
        return 0;
    }
    let shifted = (src & width.mask()) >> start;
    let field_bits = length.min(bits - start);
    if field_bits == 64 {
        shifted
    } else {
        shifted & ((1_u64 << field_bits) - 1)
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn execute_native_op(
    kind: X86TbmKind,
    width: OpWidth,
    source: u64,
    flags: FlagUpdate,
) -> crate::smir::lower::runtime::GuestRegs {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, tbm_op(kind, width, flags));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&builder.finish())
        .expect("lower state-backed TBM");
    let code = lowerer.finalize().expect("finalize state-backed TBM");
    let exec = ExecMem::new(&code).expect("map state-backed TBM");
    let mut regs = GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.gpr[1] = source;
    regs.rflags = 0x2 | PF | AF | ZF | OF;
    exec.run(lowered.entry_offset, &mut regs);
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_tbm_matches_reference_for_every_kind_width_and_boundary() {
    for kind in [
        X86TbmKind::Blcfill,
        X86TbmKind::Blci,
        X86TbmKind::Blcic,
        X86TbmKind::Blcmsk,
        X86TbmKind::Blcs,
        X86TbmKind::Blsfill,
        X86TbmKind::Blsic,
        X86TbmKind::T1mskc,
        X86TbmKind::Tzmsk,
    ] {
        for width in [OpWidth::W32, OpWidth::W64] {
            let mask = width.mask();
            for source in [0, 1, 2, 0x7E, 0x7F, mask - 1, mask] {
                let regs = execute_native_op(kind, width, source, FlagUpdate::Specific(DEFINED));
                let (expected, carry) = reference(kind, source, mask);
                assert_eq!(regs.gpr[0], expected, "{kind:?}, {width:?}, {source:#x}");
                assert_eq!(regs.gpr[1], source, "{kind:?}: source");
                let expected_defined = (u64::from(carry) * CF)
                    | (u64::from(expected == 0) * ZF)
                    | (u64::from(expected & (1 << (width.bits() - 1)) != 0) * SF);
                assert_eq!(
                    regs.rflags & (CF | ZF | SF | OF),
                    expected_defined,
                    "{kind:?}, {width:?}, {source:#x}"
                );
                assert_eq!(regs.rflags & (PF | AF), PF | AF);
            }
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_state_backed_tbm_preserves_rsp_rbp_and_unrelated_gprs() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    for (name, dst, src, dst_idx, src_idx) in [
        (
            "stack destination and source",
            X86Reg::Rsp,
            X86Reg::Rbp,
            4_usize,
            5_usize,
        ),
        ("stack alias", X86Reg::Rbp, X86Reg::Rbp, 5, 5),
    ] {
        for kind in [
            X86TbmKind::Blcfill,
            X86TbmKind::Blci,
            X86TbmKind::Blcic,
            X86TbmKind::Blcmsk,
            X86TbmKind::Blcs,
            X86TbmKind::Blsfill,
            X86TbmKind::Blsic,
            X86TbmKind::T1mskc,
            X86TbmKind::Tzmsk,
        ] {
            for width in [OpWidth::W32, OpWidth::W64] {
                let mask = width.mask();
                for source in [0, mask] {
                    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
                    builder.push_op(
                        0x1000,
                        OpKind::X86Tbm {
                            dst: x86(dst),
                            src: x86(src),
                            width,
                            kind,
                            flags: FlagUpdate::Specific(DEFINED),
                        },
                    );
                    builder.set_terminator(Terminator::Return { values: vec![] });
                    let mut lowerer = X86_64Lowerer::new();
                    let lowered = lowerer
                        .lower_function(&builder.finish())
                        .expect("lower state-backed TBM");
                    let code = lowerer.finalize().expect("finalize state-backed TBM");
                    let exec = ExecMem::new(&code).expect("map state-backed TBM");
                    let mut regs = GuestRegs::default();
                    for (index, value) in regs.gpr.iter_mut().enumerate() {
                        *value = 0xA500_0000_0000_0000 | index as u64;
                    }
                    regs.gpr[src_idx] = source;
                    regs.rflags = 0x2 | PF | AF | ZF | OF;
                    let initial = regs.gpr;
                    exec.run(lowered.entry_offset, &mut regs);

                    let (expected, carry) = reference(kind, source, mask);
                    let mut expected_gpr = initial;
                    expected_gpr[dst_idx] = expected;
                    assert_eq!(
                        regs.gpr, expected_gpr,
                        "{name}, {kind:?}, {width:?}, {source:#x}"
                    );
                    let expected_defined = (u64::from(carry) * CF)
                        | (u64::from(expected == 0) * ZF)
                        | (u64::from(expected & (1 << (width.bits() - 1)) != 0) * SF);
                    assert_eq!(
                        regs.rflags & (CF | PF | AF | ZF | SF | OF),
                        PF | AF | expected_defined,
                        "{name}, {kind:?}, {width:?}, {source:#x}"
                    );
                }
            }
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_tbm_flag_suppression_preserves_all_status_flags() {
    let regs = execute_native_op(
        X86TbmKind::Blcmsk,
        OpWidth::W64,
        0xFFFF_FFFD,
        FlagUpdate::None,
    );
    assert_eq!(regs.gpr[0], 3);
    assert_eq!(
        regs.rflags & (CF | PF | AF | ZF | SF | OF),
        PF | AF | ZF | OF
    );
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_tbm_immediate_bextr_matches_boundaries_aliases_and_flag_policy() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let source = 0xFEDC_BA98_7654_3210;
    for (name, dst, src, dst_idx, src_idx) in [
        ("distinct", X86Reg::Rax, X86Reg::Rcx, 0_usize, 1_usize),
        ("destination-source alias", X86Reg::Rcx, X86Reg::Rcx, 1, 1),
        (
            "state-backed stack operands",
            X86Reg::Rsp,
            X86Reg::Rbp,
            4,
            5,
        ),
        ("state-backed stack alias", X86Reg::Rbp, X86Reg::Rbp, 5, 5),
    ] {
        for width in [OpWidth::W32, OpWidth::W64] {
            for control in [0, 0x0804, 0x0840, 0x4004] {
                let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
                builder.push_op(0x1000, OpKind::X86RequireTbm);
                builder.push_op(
                    0x1000,
                    OpKind::Bextr {
                        dst: x86(dst),
                        src: x86(src),
                        control: VReg::Imm(control),
                        width,
                        flags: FlagUpdate::Specific(
                            FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF),
                        ),
                    },
                );
                builder.set_terminator(Terminator::Return { values: vec![] });
                let mut lowerer = X86_64Lowerer::new();
                lowerer.set_jit_fault_deopt_guards(true);
                let lowered = lowerer
                    .lower_function(&builder.finish())
                    .expect("lower guarded immediate BEXTR");
                let code = lowerer.finalize().expect("finalize immediate BEXTR");
                let exec = ExecMem::new(&code).expect("map immediate BEXTR");

                let mut regs = GuestRegs::default();
                for (index, value) in regs.gpr.iter_mut().enumerate() {
                    *value = 0xA500_0000_0000_0000 | index as u64;
                }
                regs.gpr[src_idx] = source;
                regs.rflags = 0x2 | CF | PF | AF | ZF | SF | OF;
                regs.cr0 = 1;
                regs.cs_l = 1;
                regs.cpuid_tbm = 1;
                regs.exit_pc = 0xDEAD_BEEF_CAFE_BABE;
                let initial = regs.gpr;
                exec.run(lowered.entry_offset, &mut regs);

                let expected = bextr_reference(source, control as u64, width);
                let mut expected_gpr = initial;
                expected_gpr[dst_idx] = expected;
                assert_eq!(regs.gpr, expected_gpr, "{name}, {width:?}, {control:#06x}");
                assert_eq!(
                    regs.rflags & (CF | PF | AF | ZF | SF | OF),
                    PF | AF | SF | (u64::from(expected == 0) * ZF),
                    "{name}, {width:?}, {control:#06x}"
                );
                assert_eq!(regs.exit_pc, 0xDEAD_BEEF_CAFE_BABE);
            }
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_tbm_guard_is_dynamic_precise_noncommitting_and_flag_neutral() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x2345, OpKind::X86RequireTbm);
    builder.push_op(
        0x2345,
        OpKind::Mov {
            dst: x86(X86Reg::Rbx),
            src: SrcOperand::Imm(0x1357_9BDF_2468_ACE0_u64 as i64),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(true);
    let lowered = lowerer
        .lower_function(&builder.finish())
        .expect("lower TBM-guarded sequence");
    let code = lowerer.finalize().expect("finalize TBM-guarded sequence");
    let exec = ExecMem::new(&code).expect("map TBM-guarded sequence");

    for (feature, protected_mode, long_mode, virtual_8086) in [
        (false, true, true, false),
        (true, false, true, false),
        (true, true, false, false),
        (true, true, true, true),
        (true, true, true, false),
    ] {
        let admitted = feature && protected_mode && long_mode && !virtual_8086;
        let mut regs = GuestRegs::default();
        for (index, value) in regs.gpr.iter_mut().enumerate() {
            *value = 0xA500_0000_0000_0000 | index as u64;
        }
        regs.rflags = 0x2
            | CF
            | PF
            | AF
            | ZF
            | SF
            | OF
            | if virtual_8086 {
                crate::isa::x86_64::flags::bits::VM
            } else {
                0
            };
        regs.cr0 = u64::from(protected_mode);
        regs.cs_l = u64::from(long_mode);
        regs.cpuid_tbm = u64::from(feature);
        regs.exit_pc = 0xDEAD_BEEF_CAFE_BABE;
        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(
            regs.exit_pc,
            if admitted {
                0xDEAD_BEEF_CAFE_BABE
            } else {
                0x2345
            },
            "TBM={feature}, PE={protected_mode}, CS.L={long_mode}, VM={virtual_8086}"
        );
        assert_eq!(
            regs.gpr[3],
            if admitted {
                0x1357_9BDF_2468_ACE0
            } else {
                0xA500_0000_0000_0003
            },
            "TBM={feature}, PE={protected_mode}, CS.L={long_mode}, VM={virtual_8086}"
        );
        assert_eq!(
            regs.rflags & (CF | PF | AF | ZF | SF | OF),
            CF | PF | AF | ZF | SF | OF
        );
    }
}
