//! state part 3 tests

use super::*;
use crate::smir::lower::x86_64::tests::*;
use crate::smir::lower::x86_64::*;

#[test]
fn lower_state_backed_gpr_bswap_emits_slot_commits_and_rejects_malformed_shapes() {
    let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
    let word = lower_single_op(OpKind::Bswap {
        dst: x86(X86Reg::Rbp),
        src: x86(X86Reg::R16),
        width: OpWidth::W16,
    });
    assert!(word.contains(&0x9C), "word Bswap must save RFLAGS");
    assert!(word.contains(&0x9D), "word Bswap must restore RFLAGS");
    assert!(
        word.windows(4)
            .any(|bytes| bytes == [0x66, 0xC1, 0xC2, 0x08]),
        "word Bswap must rotate DX by one byte: {word:02X?}"
    );
    assert!(
        word.windows(4)
            .any(|bytes| bytes == [0x66, 0x89, 0x55, 0x00]),
        "word Bswap must partially synchronize guest RBP: {word:02X?}"
    );

    let dword = lower_single_op(OpKind::Bswap {
        dst: x86(X86Reg::R16),
        src: x86(X86Reg::Rsp),
        width: OpWidth::W32,
    });
    assert!(
        dword.windows(2).any(|bytes| bytes == [0x0F, 0xCA]),
        "dword Bswap must operate on EDX: {dword:02X?}"
    );
    assert!(
        dword
            .windows(7)
            .any(|bytes| bytes == [0x48, 0x89, 0x90, 0x80, 0x00, 0x00, 0x00]),
        "dword Bswap must fully commit GuestRegs.gpr[16]: {dword:02X?}"
    );

    for malformed in [
        OpKind::Bswap {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rax),
            width: OpWidth::W8,
        },
        OpKind::Bswap {
            dst: x86(X86Reg::R16),
            src: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
            width: OpWidth::W64,
        },
    ] {
        assert!(
            matches!(
                lower_single_op_err(malformed),
                LowerError::InvalidOperand { .. }
            ),
            "malformed state-backed Bswap must fail lowering"
        );
    }

    let hinted = OpKind::Bswap {
        dst: x86(X86Reg::R16),
        src: x86(X86Reg::Rax),
        width: OpWidth::W64,
    };
    assert!(matches!(
        lower_single_hinted_op_err(hinted, X86OpHint::Mulx),
        LowerError::InvalidOperand { .. }
    ));
}
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_state_backed_gpr_bswap_preserves_widths_flags_and_host_stack() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const STATUS: u64 = 0x8D5;

    struct Case {
        name: &'static str,
        dst: X86Reg,
        src: X86Reg,
        width: OpWidth,
    }

    let cases = [
        Case {
            name: "MOVBE BP,R16W partial destination",
            dst: X86Reg::Rbp,
            src: X86Reg::R16,
            width: OpWidth::W16,
        },
        Case {
            name: "MOVBE R16D,ESP zero-extending destination",
            dst: X86Reg::R16,
            src: X86Reg::Rsp,
            width: OpWidth::W32,
        },
        Case {
            name: "BSWAP RSP full in-place destination",
            dst: X86Reg::Rsp,
            src: X86Reg::Rsp,
            width: OpWidth::W64,
        },
        Case {
            name: "MOVBE R31,RBP full state-to-state copy",
            dst: X86Reg::R31,
            src: X86Reg::Rbp,
            width: OpWidth::W64,
        },
        Case {
            name: "BSWAP R16D zero-extending in-place destination",
            dst: X86Reg::R16,
            src: X86Reg::R16,
            width: OpWidth::W32,
        },
    ];

    let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
    for case in cases {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Bswap {
                dst: x86(case.dst),
                src: x86(case.src),
                width: case.width,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });

        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
        let code = lowerer
            .finalize()
            .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
        let exec = ExecMem::new(&code)
            .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

        let mut regs = GuestRegs::default();
        for (index, value) in regs.gpr.iter_mut().enumerate() {
            *value = 0xA1A2_0000_0000_8000u64
                .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
        }
        regs.rflags = STATUS;
        let mut expected = regs;
        let dst_idx = case.dst.gpr_index().unwrap() as usize;
        let src_idx = case.src.gpr_index().unwrap() as usize;
        let source = regs.gpr[src_idx];
        expected.gpr[dst_idx] = match case.width {
            OpWidth::W16 => (regs.gpr[dst_idx] & !0xFFFF) | u64::from((source as u16).swap_bytes()),
            OpWidth::W32 => u64::from((source as u32).swap_bytes()),
            OpWidth::W64 => source.swap_bytes(),
            _ => unreachable!(),
        };

        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
        assert_eq!(regs.rflags & STATUS, STATUS, "{} status flags", case.name);
    }
}
#[test]
fn lower_state_backed_gpr_xchg_emits_slot_commits_and_rejects_malformed_shapes() {
    let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
    let byte = lower_single_op(OpKind::Xchg {
        reg1: x86(X86Reg::Rbp),
        reg2: x86(X86Reg::R17),
        width: OpWidth::W8,
    });
    assert!(
        byte.windows(3).any(|bytes| bytes == [0x88, 0x55, 0x00]),
        "byte Xchg must partially synchronize saved guest RBP: {byte:02X?}"
    );
    assert!(
        byte.windows(7)
            .any(|bytes| bytes == [0x40, 0x88, 0xB8, 0x88, 0x00, 0x00, 0x00]),
        "byte Xchg must partially commit GuestRegs.gpr[17]: {byte:02X?}"
    );

    let word = lower_single_op(OpKind::Xchg {
        reg1: x86(X86Reg::Rsp),
        reg2: x86(X86Reg::R16),
        width: OpWidth::W16,
    });
    assert!(
        word.windows(4)
            .any(|bytes| bytes == [0x66, 0x89, 0x50, 0x20]),
        "word Xchg must partially commit GuestRegs.gpr[4]: {word:02X?}"
    );
    assert!(
        word.windows(7)
            .any(|bytes| bytes == [0x66, 0x89, 0xB8, 0x80, 0x00, 0x00, 0x00]),
        "word Xchg must partially commit GuestRegs.gpr[16]: {word:02X?}"
    );

    let dword = lower_single_op(OpKind::Xchg {
        reg1: x86(X86Reg::Rbp),
        reg2: x86(X86Reg::R17),
        width: OpWidth::W32,
    });
    assert!(
        dword
            .windows(4)
            .any(|bytes| bytes == [0x48, 0x89, 0x55, 0x00]),
        "dword Xchg must synchronize the zero-extended guest RBP: {dword:02X?}"
    );

    for malformed in [
        OpKind::Xchg {
            reg1: x86(X86Reg::R16),
            reg2: x86(X86Reg::Rax),
            width: OpWidth::W128,
        },
        OpKind::Xchg {
            reg1: x86(X86Reg::R16),
            reg2: VReg::Virtual(crate::smir::ir::types::VirtualId(0)),
            width: OpWidth::W64,
        },
    ] {
        assert!(
            matches!(
                lower_single_op_err(malformed),
                LowerError::InvalidOperand { .. }
            ),
            "malformed state-backed Xchg must fail lowering"
        );
    }

    let hinted = OpKind::Xchg {
        reg1: x86(X86Reg::R16),
        reg2: x86(X86Reg::Rax),
        width: OpWidth::W64,
    };
    assert!(matches!(
        lower_single_hinted_op_err(hinted, X86OpHint::Mulx),
        LowerError::InvalidOperand { .. }
    ));
}
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_state_backed_gpr_xchg_preserves_widths_flags_and_host_stack() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const STATUS: u64 = 0x8D5;

    struct Case {
        name: &'static str,
        reg1: X86Reg,
        reg2: X86Reg,
        width: OpWidth,
    }

    let cases = [
        Case {
            name: "XCHG AL,R16B partial exchange",
            reg1: X86Reg::Rax,
            reg2: X86Reg::R16,
            width: OpWidth::W8,
        },
        Case {
            name: "XCHG BPL,R17B partial exchange and saved-frame commit",
            reg1: X86Reg::Rbp,
            reg2: X86Reg::R17,
            width: OpWidth::W8,
        },
        Case {
            name: "XCHG SPL,BPL partial state-to-state exchange",
            reg1: X86Reg::Rsp,
            reg2: X86Reg::Rbp,
            width: OpWidth::W8,
        },
        Case {
            name: "XCHG R16B,R16B partial self exchange",
            reg1: X86Reg::R16,
            reg2: X86Reg::R16,
            width: OpWidth::W8,
        },
        Case {
            name: "XCHG AX,R16W partial exchange",
            reg1: X86Reg::Rax,
            reg2: X86Reg::R16,
            width: OpWidth::W16,
        },
        Case {
            name: "XCHG EBP,R17D zero-extending exchange",
            reg1: X86Reg::Rbp,
            reg2: X86Reg::R17,
            width: OpWidth::W32,
        },
        Case {
            name: "XCHG RSP,R31 full exchange",
            reg1: X86Reg::Rsp,
            reg2: X86Reg::R31,
            width: OpWidth::W64,
        },
        Case {
            name: "XCHG SP,BP partial state-to-state exchange",
            reg1: X86Reg::Rsp,
            reg2: X86Reg::Rbp,
            width: OpWidth::W16,
        },
        Case {
            name: "XCHG R16D,R16D zero-extending self exchange",
            reg1: X86Reg::R16,
            reg2: X86Reg::R16,
            width: OpWidth::W32,
        },
    ];

    let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
    for case in cases {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Xchg {
                reg1: x86(case.reg1),
                reg2: x86(case.reg2),
                width: case.width,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });

        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
        let code = lowerer
            .finalize()
            .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
        let exec = ExecMem::new(&code)
            .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

        let mut regs = GuestRegs::default();
        for (index, value) in regs.gpr.iter_mut().enumerate() {
            *value = 0xA1A2_0000_0000_8000u64
                .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
        }
        regs.rflags = STATUS;
        let mut expected = regs;
        let reg1_idx = case.reg1.gpr_index().unwrap() as usize;
        let reg2_idx = case.reg2.gpr_index().unwrap() as usize;
        let old_reg1 = regs.gpr[reg1_idx];
        let old_reg2 = regs.gpr[reg2_idx];
        match case.width {
            OpWidth::W8 => {
                expected.gpr[reg1_idx] = (old_reg1 & !0xFF) | (old_reg2 & 0xFF);
                expected.gpr[reg2_idx] = (old_reg2 & !0xFF) | (old_reg1 & 0xFF);
            }
            OpWidth::W16 => {
                expected.gpr[reg1_idx] = (old_reg1 & !0xFFFF) | (old_reg2 & 0xFFFF);
                expected.gpr[reg2_idx] = (old_reg2 & !0xFFFF) | (old_reg1 & 0xFFFF);
            }
            OpWidth::W32 => {
                expected.gpr[reg1_idx] = old_reg2 & 0xFFFF_FFFF;
                expected.gpr[reg2_idx] = old_reg1 & 0xFFFF_FFFF;
            }
            OpWidth::W64 => {
                expected.gpr[reg1_idx] = old_reg2;
                expected.gpr[reg2_idx] = old_reg1;
            }
            _ => unreachable!(),
        }

        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
        assert_eq!(regs.rflags & STATUS, STATUS, "{} status flags", case.name);
    }
}
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_state_backed_and_not_preserves_gprs_flags_and_aliases() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    struct Case {
        name: &'static str,
        flagful: bool,
        dst: X86Reg,
        src1: X86Reg,
        src2: X86Reg,
        width: OpWidth,
        src1_value: u64,
        src2_value: u64,
    }
    let cases = [
        Case {
            name: "ANDN RSP,RSP,RBP destination-first-source alias",
            flagful: true,
            dst: X86Reg::Rsp,
            src1: X86Reg::Rsp,
            src2: X86Reg::Rbp,
            width: OpWidth::W64,
            src1_value: 0xF0F0_00FF_AA55_5A5A,
            src2_value: 0x70F0_F000_AA00_0A0A,
        },
        Case {
            name: "ANDN EBP,ESP,EBP destination-second-source alias",
            flagful: true,
            dst: X86Reg::Rbp,
            src1: X86Reg::Rsp,
            src2: X86Reg::Rbp,
            width: OpWidth::W32,
            src1_value: 0,
            src2_value: u64::MAX,
        },
        Case {
            name: "ANDN R16D,R31D,R31D source alias",
            flagful: true,
            dst: X86Reg::R16,
            src1: X86Reg::R31,
            src2: X86Reg::R31,
            width: OpWidth::W32,
            src1_value: 0xAABB_CCDD_8000_0018,
            src2_value: 0xAABB_CCDD_8000_0018,
        },
        Case {
            name: "NF ANDN R31,R31,R31 all operands alias",
            flagful: false,
            dst: X86Reg::R31,
            src1: X86Reg::R31,
            src2: X86Reg::R31,
            width: OpWidth::W64,
            src1_value: 0xDEAD_BEEF_1357_2418,
            src2_value: 0xDEAD_BEEF_1357_2418,
        },
    ];
    let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
    let defined = FlagSet::CF
        .union(FlagSet::ZF)
        .union(FlagSet::SF)
        .union(FlagSet::OF);
    const STATUS: u64 = 0x8D5;

    for case in cases {
        let flags = if case.flagful {
            FlagUpdate::Specific(defined)
        } else {
            FlagUpdate::None
        };
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::AndNot {
                dst: x86(case.dst),
                src1: x86(case.src1),
                src2: SrcOperand::Reg(x86(case.src2)),
                width: case.width,
                flags,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
        let code = lowerer
            .finalize()
            .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
        let exec = ExecMem::new(&code)
            .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

        let mut regs = GuestRegs::default();
        for (index, value) in regs.gpr.iter_mut().enumerate() {
            *value = 0x2468_0000_1357_0000u64
                .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
        }
        let dst_idx = case.dst.gpr_index().unwrap() as usize;
        let src1_idx = case.src1.gpr_index().unwrap() as usize;
        let src2_idx = case.src2.gpr_index().unwrap() as usize;
        regs.gpr[src1_idx] = case.src1_value;
        regs.gpr[src2_idx] = case.src2_value;
        regs.rflags = 0x2 | STATUS;

        let mut expected = regs;
        let src1 = regs.gpr[src1_idx] & case.width.mask();
        let src2 = regs.gpr[src2_idx] & case.width.mask();
        let result = (src1 & !src2) & case.width.mask();
        expected.gpr[dst_idx] = result;
        if case.flagful {
            expected.rflags &= !0x8C1;
            expected.rflags |= u64::from(result == 0) << 6;
            expected.rflags |= ((result >> (case.width.bits() - 1)) & 1) << 7;
        }

        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
        if case.width == OpWidth::W32 {
            assert_eq!(regs.gpr[dst_idx] >> 32, 0, "{} zero extension", case.name);
        }
        assert_eq!(
            regs.rflags & STATUS,
            expected.rflags & STATUS,
            "{} status flags",
            case.name
        );
    }
}
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_state_backed_bls_preserves_gprs_flags_and_aliases() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    if !std::is_x86_feature_detected!("bmi1") {
        return;
    }
    struct Case {
        name: &'static str,
        kind: X86BlsKind,
        flagful: bool,
        dst: X86Reg,
        src: X86Reg,
        width: OpWidth,
        source: u64,
    }
    let cases = [
        Case {
            name: "BLSR RSP,RBP zero source",
            kind: X86BlsKind::Blsr,
            flagful: true,
            dst: X86Reg::Rsp,
            src: X86Reg::Rbp,
            width: OpWidth::W64,
            source: 0,
        },
        Case {
            name: "BLSMSK EBP,ESP zero source",
            kind: X86BlsKind::Blsmsk,
            flagful: true,
            dst: X86Reg::Rbp,
            src: X86Reg::Rsp,
            width: OpWidth::W32,
            source: 0,
        },
        Case {
            name: "BLSI R31D,R16D",
            kind: X86BlsKind::Blsi,
            flagful: true,
            dst: X86Reg::R31,
            src: X86Reg::R16,
            width: OpWidth::W32,
            source: 0xAABB_CCDD_8000_0018,
        },
        Case {
            name: "NF BLSR R16,R16 source-destination alias",
            kind: X86BlsKind::Blsr,
            flagful: false,
            dst: X86Reg::R16,
            src: X86Reg::R16,
            width: OpWidth::W64,
            source: 0xDEAD_BEEF_1357_2418,
        },
    ];
    let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
    let defined = FlagSet::CF
        .union(FlagSet::ZF)
        .union(FlagSet::SF)
        .union(FlagSet::OF);
    const STATUS: u64 = 0x8D5;

    for case in cases {
        let flags = if case.flagful {
            FlagUpdate::Specific(defined)
        } else {
            FlagUpdate::None
        };
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::X86Bls {
                dst: x86(case.dst),
                src: x86(case.src),
                width: case.width,
                kind: case.kind,
                flags,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
        let code = lowerer
            .finalize()
            .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
        let exec = ExecMem::new(&code)
            .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

        let mut regs = GuestRegs::default();
        for (index, value) in regs.gpr.iter_mut().enumerate() {
            *value = 0x2468_0000_1357_0000u64
                .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
        }
        let dst_idx = case.dst.gpr_index().unwrap() as usize;
        let src_idx = case.src.gpr_index().unwrap() as usize;
        regs.gpr[src_idx] = case.source;
        regs.rflags = 0x2 | STATUS;

        let mut expected = regs;
        let source = regs.gpr[src_idx] & case.width.mask();
        let result = match case.kind {
            X86BlsKind::Blsr => source & source.wrapping_sub(1),
            X86BlsKind::Blsmsk => source ^ source.wrapping_sub(1),
            X86BlsKind::Blsi => source.wrapping_neg() & source,
        } & case.width.mask();
        expected.gpr[dst_idx] = result;
        if case.flagful {
            expected.rflags &= !0x8C1;
            let carry = match case.kind {
                X86BlsKind::Blsr | X86BlsKind::Blsmsk => source == 0,
                X86BlsKind::Blsi => source != 0,
            };
            let zero = case.kind != X86BlsKind::Blsmsk && result == 0;
            expected.rflags |= u64::from(carry);
            expected.rflags |= u64::from(zero) << 6;
            expected.rflags |= ((result >> (case.width.bits() - 1)) & 1) << 7;
        }

        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
        if case.width == OpWidth::W32 {
            assert_eq!(regs.gpr[dst_idx] >> 32, 0, "{} zero extension", case.name);
        }
        assert_eq!(
            regs.rflags & STATUS,
            expected.rflags & STATUS,
            "{} status flags",
            case.name
        );
    }
}
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_state_backed_bextr_bzhi_preserve_gprs_flags_and_aliases() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    struct Case {
        name: &'static str,
        bzhi: bool,
        flagful: bool,
        dst: X86Reg,
        src: X86Reg,
        control: X86Reg,
        width: OpWidth,
        source: u64,
        control_value: u64,
    }
    let cases = [
        Case {
            name: "BEXTR RSP,RBP,R16",
            bzhi: false,
            flagful: true,
            dst: X86Reg::Rsp,
            src: X86Reg::Rbp,
            control: X86Reg::R16,
            width: OpWidth::W64,
            source: 0xFEDC_BA98_7654_3210,
            control_value: (20 << 8) | 12,
        },
        Case {
            name: "BZHI EBP,ESP,ECX",
            bzhi: true,
            flagful: true,
            dst: X86Reg::Rbp,
            src: X86Reg::Rsp,
            control: X86Reg::Rcx,
            width: OpWidth::W32,
            source: 0xAABB_CCDD_DEAD_BEEF,
            control_value: 40,
        },
        Case {
            name: "NF BEXTR R31D,R16D,R31D destination-control alias",
            bzhi: false,
            flagful: false,
            dst: X86Reg::R31,
            src: X86Reg::R16,
            control: X86Reg::R31,
            width: OpWidth::W32,
            source: 0x0123_4567_89AB_CDEF,
            control_value: (12 << 8) | 7,
        },
        Case {
            name: "NF BZHI R16,R16,R16 all operands alias",
            bzhi: true,
            flagful: false,
            dst: X86Reg::R16,
            src: X86Reg::R16,
            control: X86Reg::R16,
            width: OpWidth::W64,
            source: 0,
            control_value: 0xDEAD_BEEF_1357_2420,
        },
    ];
    let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
    const STATUS: u64 = 0x8D5;

    for case in cases {
        if (case.bzhi && !std::is_x86_feature_detected!("bmi2"))
            || (!case.bzhi && !std::is_x86_feature_detected!("bmi1"))
        {
            continue;
        }
        let flags = if case.flagful {
            if case.bzhi {
                FlagUpdate::Specific(
                    FlagSet::CF
                        .union(FlagSet::ZF)
                        .union(FlagSet::SF)
                        .union(FlagSet::OF),
                )
            } else {
                FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF).union(FlagSet::OF))
            }
        } else {
            FlagUpdate::None
        };
        let kind = if case.bzhi {
            OpKind::Bzhi {
                dst: x86(case.dst),
                src: x86(case.src),
                index: x86(case.control),
                width: case.width,
                flags,
            }
        } else {
            OpKind::Bextr {
                dst: x86(case.dst),
                src: x86(case.src),
                control: x86(case.control),
                width: case.width,
                flags,
            }
        };
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
        let code = lowerer
            .finalize()
            .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
        let exec = ExecMem::new(&code)
            .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

        let mut regs = GuestRegs::default();
        for (index, value) in regs.gpr.iter_mut().enumerate() {
            *value = 0x2468_0000_1357_0000u64
                .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
        }
        let dst_idx = case.dst.gpr_index().unwrap() as usize;
        let src_idx = case.src.gpr_index().unwrap() as usize;
        let control_idx = case.control.gpr_index().unwrap() as usize;
        regs.gpr[src_idx] = case.source;
        regs.gpr[control_idx] = case.control_value;
        regs.rflags = 0x2 | STATUS;

        let mut expected = regs;
        let bits = case.width.bits();
        let operand_mask = case.width.mask();
        let source = regs.gpr[src_idx] & operand_mask;
        let control = regs.gpr[control_idx] & operand_mask;
        let result = if case.bzhi {
            let index = (control & 0xFF) as u32;
            if index >= bits {
                source
            } else {
                source & ((1u64 << index) - 1)
            }
        } else {
            let start = (control & 0xFF) as u32;
            let length = ((control >> 8) & 0xFF) as u32;
            if start >= bits || length == 0 {
                0
            } else {
                let field_bits = length.min(bits - start);
                let shifted = source >> start;
                if field_bits == 64 {
                    shifted
                } else {
                    shifted & ((1u64 << field_bits) - 1)
                }
            }
        };
        expected.gpr[dst_idx] = result;
        if case.flagful {
            if case.bzhi {
                let index = (control & 0xFF) as u32;
                expected.rflags &= !0x8C1;
                expected.rflags |= u64::from(index >= bits);
                expected.rflags |= u64::from(result == 0) << 6;
                expected.rflags |= ((result >> (bits - 1)) & 1) << 7;
            } else {
                expected.rflags &= !0x841;
                expected.rflags |= u64::from(result == 0) << 6;
            }
        }

        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
        if case.width == OpWidth::W32 {
            assert_eq!(regs.gpr[dst_idx] >> 32, 0, "{} zero extension", case.name);
        }
        assert_eq!(
            regs.rflags & STATUS,
            expected.rflags & STATUS,
            "{} status flags",
            case.name
        );
    }
}
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_state_backed_pdep_pext_preserve_gprs_flags_and_aliases() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    if !std::is_x86_feature_detected!("bmi2") {
        return;
    }
    fn pdep(mut src: u64, mut mask: u64) -> u64 {
        let mut result = 0u64;
        while mask != 0 {
            let bit = mask & mask.wrapping_neg();
            if src & 1 != 0 {
                result |= bit;
            }
            src >>= 1;
            mask &= mask - 1;
        }
        result
    }
    fn pext(src: u64, mut mask: u64) -> u64 {
        let mut result = 0u64;
        let mut output_bit = 1u64;
        while mask != 0 {
            let bit = mask & mask.wrapping_neg();
            if src & bit != 0 {
                result |= output_bit;
            }
            output_bit <<= 1;
            mask &= mask - 1;
        }
        result
    }

    struct Case {
        name: &'static str,
        extract: bool,
        dst: X86Reg,
        src: X86Reg,
        mask: X86Reg,
        width: OpWidth,
        source: u64,
        mask_value: u64,
    }
    let cases = [
        Case {
            name: "PDEP RSP,RBP,R16",
            extract: false,
            dst: X86Reg::Rsp,
            src: X86Reg::Rbp,
            mask: X86Reg::R16,
            width: OpWidth::W64,
            source: 0x0123_4567_89AB_CDEF,
            mask_value: 0xF0F0_00FF_AA55_5A5A,
        },
        Case {
            name: "PEXT EBP,ESP,ECX",
            extract: true,
            dst: X86Reg::Rbp,
            src: X86Reg::Rsp,
            mask: X86Reg::Rcx,
            width: OpWidth::W32,
            source: 0xAABB_CCDD_DEAD_BEEF,
            mask_value: 0xFFFF_0000_F0F0_55AA,
        },
        Case {
            name: "PDEP R31D,R16D,R31D destination-mask alias",
            extract: false,
            dst: X86Reg::R31,
            src: X86Reg::R16,
            mask: X86Reg::R31,
            width: OpWidth::W32,
            source: 0xFEDC_BA98_7654_3210,
            mask_value: 0xAAAA_5555,
        },
        Case {
            name: "PEXT R16,R16,R16 all operands alias",
            extract: true,
            dst: X86Reg::R16,
            src: X86Reg::R16,
            mask: X86Reg::R16,
            width: OpWidth::W64,
            source: 0xDEAD_BEEF_1357_2468,
            mask_value: 0xA5A5_5A5A_C3C3_3C3C,
        },
    ];
    let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
    const FLAGS: u64 = 0x8D5;

    for case in cases {
        let kind = if case.extract {
            OpKind::Pext {
                dst: x86(case.dst),
                src: x86(case.src),
                mask: x86(case.mask),
                width: case.width,
            }
        } else {
            OpKind::Pdep {
                dst: x86(case.dst),
                src: x86(case.src),
                mask: x86(case.mask),
                width: case.width,
            }
        };
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
        let code = lowerer
            .finalize()
            .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
        let exec = ExecMem::new(&code)
            .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

        let mut regs = GuestRegs::default();
        for (index, value) in regs.gpr.iter_mut().enumerate() {
            *value = 0x2468_0000_1357_0000u64
                .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
        }
        let dst_idx = case.dst.gpr_index().unwrap() as usize;
        let src_idx = case.src.gpr_index().unwrap() as usize;
        let mask_idx = case.mask.gpr_index().unwrap() as usize;
        regs.gpr[src_idx] = case.source;
        regs.gpr[mask_idx] = case.mask_value;
        regs.rflags = 0x2 | FLAGS;

        let mut expected = regs;
        let operand_mask = case.width.mask();
        let source = regs.gpr[src_idx] & operand_mask;
        let mask = regs.gpr[mask_idx] & operand_mask;
        expected.gpr[dst_idx] = if case.extract {
            pext(source, mask)
        } else {
            pdep(source, mask)
        };

        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
        assert_eq!(
            regs.rflags & FLAGS,
            expected.rflags & FLAGS,
            "{} flags",
            case.name
        );
    }
}
#[test]
fn lower_state_backed_gpr_bit_tests_emit_cf_merge_and_reject_malformed_shapes() {
    let x86 = |reg| VReg::Arch(ArchReg::X86(reg));

    let bt = lower_single_op(OpKind::Bt {
        src: x86(X86Reg::Rsp),
        index: SrcOperand::Reg(x86(X86Reg::Rbp)),
        width: OpWidth::W64,
    });
    assert!(
        bt.windows(4).any(|bytes| bytes == [0x48, 0x0F, 0xA3, 0xFA]),
        "state-backed BT must test RDX by RDI: {bt:02X?}"
    );
    assert_eq!(
        bt.iter().filter(|byte| **byte == 0x9C).count(),
        2,
        "state-backed BT must save old and new RFLAGS: {bt:02X?}"
    );
    assert_eq!(bt.iter().filter(|byte| **byte == 0x9D).count(), 1);

    let bts = lower_single_op(OpKind::Bts {
        dst: x86(X86Reg::Rbp),
        src: x86(X86Reg::Rbp),
        index: SrcOperand::Imm(15),
        width: OpWidth::W16,
    });
    assert!(
        bts.windows(5)
            .any(|bytes| bytes == [0x66, 0x0F, 0xBA, 0xEA, 0x0F]),
        "state-backed BTS must update DX by immediate: {bts:02X?}"
    );
    assert!(
        bts.windows(4)
            .any(|bytes| bytes == [0x66, 0x89, 0x55, 0x00]),
        "word BTS must partially synchronize guest RBP: {bts:02X?}"
    );

    let btr = lower_single_op(OpKind::Btr {
        dst: x86(X86Reg::R16),
        src: x86(X86Reg::R16),
        index: SrcOperand::Reg(x86(X86Reg::R31)),
        width: OpWidth::W32,
    });
    assert!(
        btr.windows(3).any(|bytes| bytes == [0x0F, 0xB3, 0xFA]),
        "state-backed BTR must update EDX by EDI: {btr:02X?}"
    );
    assert!(
        btr.windows(7)
            .any(|bytes| bytes == [0x48, 0x89, 0x90, 0x80, 0x00, 0x00, 0x00]),
        "dword BTR must fully commit GuestRegs.gpr[16]: {btr:02X?}"
    );

    for malformed in [
        OpKind::Bt {
            src: x86(X86Reg::Rsp),
            index: SrcOperand::Imm(0),
            width: OpWidth::W8,
        },
        OpKind::Btc {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::Rsp),
            index: SrcOperand::Imm(0),
            width: OpWidth::W64,
        },
        OpKind::Bts {
            dst: x86(X86Reg::R16),
            src: x86(X86Reg::R16),
            index: SrcOperand::Reg(VReg::Virtual(crate::smir::ir::types::VirtualId(0))),
            width: OpWidth::W64,
        },
    ] {
        assert!(
            matches!(
                lower_single_op_err(malformed),
                LowerError::InvalidOperand { .. }
            ),
            "malformed state-backed bit test must fail lowering"
        );
    }

    let hinted = OpKind::Btr {
        dst: x86(X86Reg::R16),
        src: x86(X86Reg::R16),
        index: SrcOperand::Imm(7),
        width: OpWidth::W64,
    };
    assert!(matches!(
        lower_single_hinted_op_err(hinted, X86OpHint::Mulx),
        LowerError::InvalidOperand { .. }
    ));
}
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_state_backed_gpr_bit_tests_preserve_width_and_cf_contracts() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const STATUS_MASK: u64 = 0x8D5;

    struct Case {
        name: &'static str,
        kind: BitTestRegOp,
        operand: X86Reg,
        index: SrcOperand,
        source: u64,
        index_value: u64,
        width: OpWidth,
    }

    let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
    let cases = [
        Case {
            name: "BT RSP,RBP register index",
            kind: BitTestRegOp::Test,
            operand: X86Reg::Rsp,
            index: SrcOperand::Reg(x86(X86Reg::Rbp)),
            source: 1u64 << 63,
            index_value: 63,
            width: OpWidth::W64,
        },
        Case {
            name: "BTS BP,15 partial destination",
            kind: BitTestRegOp::Set,
            operand: X86Reg::Rbp,
            index: SrcOperand::Imm(15),
            source: 0x3344_5566_8765_0000,
            index_value: 15,
            width: OpWidth::W16,
        },
        Case {
            name: "BTR R16D,R31D zero-extending destination",
            kind: BitTestRegOp::Reset,
            operand: X86Reg::R16,
            index: SrcOperand::Reg(x86(X86Reg::R31)),
            source: u64::MAX,
            index_value: 31,
            width: OpWidth::W32,
        },
        Case {
            name: "BTC R31,R16 extended destination and index",
            kind: BitTestRegOp::Complement,
            operand: X86Reg::R31,
            index: SrcOperand::Reg(x86(X86Reg::R16)),
            source: 0,
            index_value: 63,
            width: OpWidth::W64,
        },
        Case {
            name: "BT R16W,SP masked register index",
            kind: BitTestRegOp::Test,
            operand: X86Reg::R16,
            index: SrcOperand::Reg(x86(X86Reg::Rsp)),
            source: 1,
            index_value: 16,
            width: OpWidth::W16,
        },
        Case {
            name: "BTR RSP,63 full destination",
            kind: BitTestRegOp::Reset,
            operand: X86Reg::Rsp,
            index: SrcOperand::Imm64(63),
            source: 1u64 << 63,
            index_value: 63,
            width: OpWidth::W64,
        },
    ];

    for case in cases {
        let operand = x86(case.operand);
        let kind = match case.kind {
            BitTestRegOp::Test => OpKind::Bt {
                src: operand,
                index: case.index.clone(),
                width: case.width,
            },
            BitTestRegOp::Set => OpKind::Bts {
                dst: operand,
                src: operand,
                index: case.index.clone(),
                width: case.width,
            },
            BitTestRegOp::Reset => OpKind::Btr {
                dst: operand,
                src: operand,
                index: case.index.clone(),
                width: case.width,
            },
            BitTestRegOp::Complement => OpKind::Btc {
                dst: operand,
                src: operand,
                index: case.index.clone(),
                width: case.width,
            },
        };
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });

        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
        let code = lowerer
            .finalize()
            .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
        let exec = ExecMem::new(&code)
            .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

        let mut regs = GuestRegs::default();
        for (index, value) in regs.gpr.iter_mut().enumerate() {
            *value = 0xA1A2_0000_0000_8000u64
                .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
        }
        let operand_idx = case.operand.gpr_index().unwrap() as usize;
        regs.gpr[operand_idx] = case.source;
        if let SrcOperand::Reg(index) = case.index {
            let index_idx = X86_64Lowerer::x86_gpr_index(index).unwrap() as usize;
            regs.gpr[index_idx] = case.index_value;
        }
        regs.rflags = STATUS_MASK;

        let mut expected = regs;
        let bit = case.index_value & (u64::from(case.width.bits()) - 1);
        let value = case.source & case.width.mask();
        let cf = (value >> bit) & 1;
        let result = match case.kind {
            BitTestRegOp::Test => None,
            BitTestRegOp::Set => Some(value | (1u64 << bit)),
            BitTestRegOp::Reset => Some(value & !(1u64 << bit)),
            BitTestRegOp::Complement => Some(value ^ (1u64 << bit)),
        };
        if let Some(result) = result {
            expected.gpr[operand_idx] = match case.width {
                OpWidth::W16 => (regs.gpr[operand_idx] & !case.width.mask()) | result,
                OpWidth::W32 | OpWidth::W64 => result,
                OpWidth::W8 | OpWidth::W128 => unreachable!(),
            };
        }
        expected.rflags = (expected.rflags & !1) | cf;

        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
        assert_eq!(
            regs.rflags & STATUS_MASK,
            expected.rflags & STATUS_MASK,
            "{} status flags",
            case.name
        );
    }
}
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_x86_alignment_check_covers_addresses_preserves_state_and_deopts_precisely() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
    let marker = 0xD1CE_BA5E_F00D_CAFEu64;
    let sentinel_pc = 0xABCD_EF01_2345_6789u64;
    let status = 0x8D5u64; // CF, PF, AF, ZF, SF, OF

    let mut cases = Vec::new();

    let mut direct = GuestRegs::default();
    direct.gpr[0] = 0x1000;
    cases.push((
        "direct/aligned16",
        Address::Direct(x86(X86Reg::Rax)),
        16,
        direct,
        false,
    ));

    let mut stack = GuestRegs::default();
    stack.gpr[4] = 0x2011;
    cases.push((
        "rsp+disp/misaligned32",
        Address::BaseOffset {
            base: x86(X86Reg::Rsp),
            offset: -16,
            disp_size: DispSize::Disp8,
        },
        32,
        stack,
        true,
    ));

    let mut sib = GuestRegs::default();
    sib.gpr[5] = 0x3000;
    sib.gpr[9] = 8;
    cases.push((
        "rbp+r9*8-64/aligned64",
        Address::BaseIndexScale {
            base: Some(x86(X86Reg::Rbp)),
            index: x86(X86Reg::R9),
            scale: 8,
            disp: -64,
            disp_size: DispSize::Disp8,
        },
        64,
        sib,
        false,
    ));

    cases.push((
        "pcrel/misaligned64",
        Address::PcRel {
            offset: -1,
            disp_size: DispSize::Disp8,
            base: Some(0x4000),
        },
        64,
        GuestRegs::default(),
        true,
    ));
    cases.push((
        "absolute/aligned32",
        Address::Absolute(0x5000),
        32,
        GuestRegs::default(),
        false,
    ));

    let mut segmented = GuestRegs::default();
    segmented.fs_base = 0x5F00;
    segmented.gpr[4] = 0x80;
    segmented.gpr[16] = 0x20;
    cases.push((
        "fs+rsp+r16*4/aligned64",
        Address::SegmentRel {
            segment: x86(X86Reg::FsBase),
            base: Some(x86(X86Reg::Rsp)),
            index: Some(x86(X86Reg::R16)),
            scale: 4,
            disp: 0,
        },
        64,
        segmented,
        false,
    ));

    let mut wide_disp = GuestRegs::default();
    wide_disp.gs_base = 0x8000_0000_0000_6001;
    cases.push((
        "gs+wide-disp/aligned16",
        Address::SegmentRel {
            segment: x86(X86Reg::GsBase),
            base: None,
            index: None,
            scale: 1,
            disp: i64::MIN,
        },
        16,
        wide_disp,
        true,
    ));

    for (name, addr, alignment, mut regs, should_fault) in cases {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, OpKind::X86CheckAlignment { addr, alignment });
        builder.push_op(
            0x1001,
            OpKind::Mov {
                dst: x86(X86Reg::R11),
                src: SrcOperand::Imm64(marker as i64),
                width: OpWidth::W64,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .unwrap_or_else(|error| panic!("lower {name}: {error:?}"));
        let exec = ExecMem::new(&lowerer.finalize().expect("finalize alignment check"))
            .expect("map alignment check");

        regs.gpr[11] = 0x1111_2222_3333_4444;
        regs.rflags = 0x2 | status;
        regs.exit_pc = sentinel_pc;
        let before = regs;
        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(regs.rflags & status, status, "{name}: status flags");
        for index in 0..32 {
            let expected = if index == 11 && !should_fault {
                marker
            } else {
                before.gpr[index]
            };
            assert_eq!(regs.gpr[index], expected, "{name}: gpr[{index}]");
        }
        assert_eq!(
            regs.exit_pc,
            if should_fault { 0x1000 } else { sentinel_pc },
            "{name}: precise resume PC"
        );
    }
}
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_xgetbv_selects_guest_state_and_deopts_faults_precisely() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1234_5678);
    builder.push_op(
        0x1234_5678,
        OpKind::X86XGetBv {
            dst_low: rax,
            dst_high: rdx,
            selector: rcx,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&builder.finish())
        .expect("lower state-backed XGETBV");
    let exec = ExecMem::new(&lowerer.finalize().expect("finalize state-backed XGETBV"))
        .expect("map state-backed XGETBV");

    let flags = 0x2 | 0x8D5;
    let mut regs = GuestRegs::default();
    regs.cr4 = 1 << 18;
    regs.xcr0 = 0xFEDC_BA98_7654_3217;
    regs.xgetbv1 = 0x00F0_00F0_FFFF_00F5;
    regs.gpr[0] = u64::MAX;
    regs.gpr[1] = 0;
    regs.gpr[2] = u64::MAX;
    regs.rflags = flags;
    regs.exit_pc = 0xAAAA_BBBB_CCCC_DDDD;
    exec.run(lowered.entry_offset, &mut regs);
    assert_eq!(regs.gpr[0], 0x7654_3217);
    assert_eq!(regs.gpr[2], 0xFEDC_BA98);
    assert_eq!(regs.gpr[1], 0, "XGETBV preserves RCX");
    assert_eq!(regs.rflags & 0x8D5, flags & 0x8D5);
    assert_eq!(regs.exit_pc, 0xAAAA_BBBB_CCCC_DDDD);

    regs.gpr[0] = u64::MAX;
    regs.gpr[1] = 1;
    regs.gpr[2] = u64::MAX;
    regs.rflags = flags;
    exec.run(lowered.entry_offset, &mut regs);
    let xinuse = regs.xgetbv1 & regs.xcr0;
    assert_eq!(regs.gpr[0], xinuse as u32 as u64);
    assert_eq!(regs.gpr[2], (xinuse >> 32) as u32 as u64);
    assert_eq!(regs.gpr[1], 1);
    assert_eq!(regs.rflags & 0x8D5, flags & 0x8D5);

    for (name, cr4, selector) in [
        ("OSXSAVE clear", 0, 0),
        ("invalid selector", 1 << 18, 2),
        ("large low-32 selector", 1 << 18, 0xFFFF_FFFF),
    ] {
        let mut fault = GuestRegs::default();
        fault.cr4 = cr4;
        fault.xcr0 = regs.xcr0;
        fault.xgetbv1 = regs.xgetbv1;
        fault.gpr[0] = 0x1111_2222_3333_4444;
        fault.gpr[1] = selector;
        fault.gpr[2] = 0x5555_6666_7777_8888;
        fault.rflags = flags;
        fault.exit_pc = 0;
        exec.run(lowered.entry_offset, &mut fault);
        assert_eq!(fault.exit_pc, 0x1234_5678, "{name}: precise restart PC");
        assert_eq!(fault.gpr[0], 0x1111_2222_3333_4444, "{name}: RAX");
        assert_eq!(fault.gpr[1], selector, "{name}: RCX");
        assert_eq!(fault.gpr[2], 0x5555_6666_7777_8888, "{name}: RDX");
        assert_eq!(fault.rflags & 0x8D5, flags & 0x8D5, "{name}: flags");
    }
}
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_xsetbv_validates_state_commits_and_hands_off_precisely() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1234_5000);
    builder.push_op(
        0x1234_5000,
        OpKind::X86XSetBv {
            selector: rcx,
            src_low: rax,
            src_high: rdx,
        },
    );
    builder.push_op(0x1234_5003, OpKind::Nop);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    let lowered = lowerer
        .lower_function(&builder.finish())
        .expect("lower state-backed XSETBV");
    let exec = ExecMem::new(&lowerer.finalize().expect("finalize state-backed XSETBV"))
        .expect("map state-backed XSETBV");

    let flags = 0x2 | 0x8D5;
    let run = |value: u64, cr4: u64, cr0: u64, cpl: u64, apx_enabled: bool, selector: u32| {
        let mut regs = GuestRegs::default();
        regs.cr4 = cr4;
        regs.cr0 = cr0;
        regs.cpl = cpl;
        regs.apx_enabled = u64::from(apx_enabled);
        regs.xcr0 = 3;
        regs.gpr[0] = 0xA5A5_A5A5_0000_0000 | (value as u32 as u64);
        regs.gpr[1] = 0x5A5A_5A5A_0000_0000 | u64::from(selector);
        regs.gpr[2] = 0xC3C3_C3C3_0000_0000 | ((value >> 32) as u32 as u64);
        regs.rflags = flags;
        regs.exit_pc = 0;
        let inputs = (regs.gpr[0], regs.gpr[1], regs.gpr[2]);
        exec.run(lowered.entry_offset, &mut regs);
        assert_eq!(
            (regs.gpr[0], regs.gpr[1], regs.gpr[2]),
            inputs,
            "XSETBV must preserve EDX:EAX and ECX"
        );
        assert_eq!(regs.rflags & 0x8D5, flags & 0x8D5);
        regs
    };

    for (name, value, apx_enabled) in [
        ("x87", 1, false),
        ("x87+sse", 3, false),
        ("avx", 7, false),
        ("avx512", 0xE7, false),
        ("pkru", 0x2E7, false),
        ("apx", 0x0008_00E7, true),
    ] {
        let regs = run(value, 1 << 18, 1, 0, apx_enabled, 0);
        assert_eq!(regs.xcr0, value, "{name}: committed XCR0");
        assert_eq!(regs.exit_pc, 0x1234_5003, "{name}: next PC handoff");
    }

    // CPL is ignored outside protected mode.
    let real_mode = run(7, 1 << 18, 0, 3, false, 0);
    assert_eq!(real_mode.xcr0, 7);
    assert_eq!(real_mode.exit_pc, 0x1234_5003);

    for (name, value, cr4, cr0, cpl, apx_enabled, selector) in [
        ("OSXSAVE clear", 7, 0, 1, 0, false, 0),
        ("protected CPL3", 7, 1 << 18, 1, 3, false, 0),
        ("selector one", 7, 1 << 18, 1, 0, false, 1),
        ("x87 disabled", 0, 1 << 18, 1, 0, false, 0),
        ("unsupported bit", 9, 1 << 18, 1, 0, false, 0),
        ("AVX without SSE", 5, 1 << 18, 1, 0, false, 0),
        ("partial AVX512", 0x27, 1 << 18, 1, 0, false, 0),
        ("AVX512 without AVX", 0xE3, 1 << 18, 1, 0, false, 0),
        ("APX disabled", 0x0008_0001, 1 << 18, 1, 0, false, 0),
        ("high unsupported", (1u64 << 63) | 1, 1 << 18, 1, 0, true, 0),
    ] {
        let regs = run(value, cr4, cr0, cpl, apx_enabled, selector);
        assert_eq!(regs.xcr0, 3, "{name}: XCR0 must not commit");
        assert_eq!(regs.exit_pc, 0x1234_5000, "{name}: fault restart PC");
    }
}
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_helper_backed_immediate_memory_bit_update_commits_cf_only_after_store() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    #[repr(C)]
    struct LoadResult {
        value: u64,
        ok: u64,
    }

    #[derive(Default)]
    struct MemoryContext {
        load_value: u64,
        store_value: u64,
        committed_value: u64,
        loads: u64,
        stores: u64,
        store_ok: u64,
    }

    extern "C" fn load(
        context: *mut MemoryContext,
        _addr: u64,
        _size: u64,
        _signed: u64,
    ) -> LoadResult {
        let context = unsafe { &mut *context };
        context.loads += 1;
        LoadResult {
            value: context.load_value,
            ok: 1,
        }
    }

    extern "C" fn store(context: *mut MemoryContext, _addr: u64, value: u64, _size: u64) -> u64 {
        let context = unsafe { &mut *context };
        context.stores += 1;
        context.store_value = value;
        if context.store_ok != 0 {
            context.committed_value = value;
        }
        context.store_ok
    }

    let old = VReg::Virtual(crate::smir::ir::types::VirtualId(35));
    let mask = VReg::Virtual(crate::smir::ir::types::VirtualId(36));
    let result = VReg::Virtual(crate::smir::ir::types::VirtualId(37));
    let address = Address::Absolute(0x4000);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Load {
            dst: old,
            addr: address.clone(),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::Mov {
            dst: mask,
            src: SrcOperand::Imm(1),
            width: OpWidth::W64,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::Shl {
            dst: mask,
            src: mask,
            amount: SrcOperand::Imm(5),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::Not {
            dst: mask,
            src: mask,
            width: OpWidth::W64,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::And {
            dst: result,
            src1: old,
            src2: SrcOperand::Reg(mask),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::Store {
            src: result,
            addr: address,
            width: MemWidth::B8,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::Bt {
            src: old,
            index: SrcOperand::Imm(5),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    let lowered = lowerer
        .lower_function(&builder.finish())
        .expect("lower helper-backed memory BTR");
    let exec = ExecMem::new(&lowerer.finalize().expect("finalize memory BTR"))
        .expect("map helper-backed memory BTR");

    const STATUS_MASK: u64 = 0x8D5;
    const INCOMING_FLAGS: u64 = 0x2 | 0x8D4; // CF clear
    let initial_gprs = {
        let mut gprs = [0u64; 32];
        for (index, value) in gprs.iter_mut().enumerate() {
            *value = 0xA500_0000_0000_0000 | index as u64;
        }
        gprs
    };

    let mut success_context = MemoryContext {
        load_value: 1 << 5,
        committed_value: 0xDEAD_BEEF_DEAD_BEEF,
        store_ok: 1,
        ..MemoryContext::default()
    };
    let mut success = GuestRegs::default();
    success.gpr = initial_gprs;
    success.rflags = INCOMING_FLAGS;
    success.exit_pc = 0xAAAA_BBBB_CCCC_DDDD;
    success.ctx = (&mut success_context as *mut MemoryContext) as u64;
    success.load_fn = load as usize as u64;
    success.store_fn = store as usize as u64;
    exec.run(lowered.entry_offset, &mut success);

    assert_eq!(success_context.loads, 1);
    assert_eq!(success_context.stores, 1);
    assert_eq!(success_context.store_value, 0, "BTR store value");
    assert_eq!(success_context.committed_value, 0);
    assert_eq!(success.gpr, initial_gprs, "success must preserve every GPR");
    assert_eq!(
        success.rflags & STATUS_MASK,
        (INCOMING_FLAGS & STATUS_MASK) | 1,
        "successful BTR must replace only CF"
    );
    assert_eq!(success.exit_pc, 0xAAAA_BBBB_CCCC_DDDD);

    let mut fault_context = MemoryContext {
        load_value: 1 << 5,
        committed_value: 0xDEAD_BEEF_DEAD_BEEF,
        store_ok: 0,
        ..MemoryContext::default()
    };
    let mut fault = GuestRegs::default();
    fault.gpr = initial_gprs;
    fault.rflags = INCOMING_FLAGS;
    fault.ctx = (&mut fault_context as *mut MemoryContext) as u64;
    fault.load_fn = load as usize as u64;
    fault.store_fn = store as usize as u64;
    exec.run(lowered.entry_offset, &mut fault);

    assert_eq!(fault_context.loads, 1);
    assert_eq!(fault_context.stores, 1);
    assert_eq!(fault_context.store_value, 0, "updated value reaches store");
    assert_eq!(
        fault_context.committed_value, 0xDEAD_BEEF_DEAD_BEEF,
        "failed store must not commit memory"
    );
    assert_eq!(fault.gpr, initial_gprs, "fault must preserve every GPR");
    assert_eq!(
        fault.rflags & STATUS_MASK,
        INCOMING_FLAGS & STATUS_MASK,
        "fault must preserve every arithmetic status flag"
    );
    assert_eq!(fault.exit_pc, 0x1000, "fault must restart current PC");
}
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_helper_backed_memory_destination_alu_commits_flags_only_after_store() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    #[repr(C)]
    struct LoadResult {
        value: u64,
        ok: u64,
    }

    #[derive(Default)]
    struct MemoryContext {
        load_value: u64,
        store_value: u64,
        committed_value: u64,
        last_addr: u64,
        last_size: u64,
        loads: u64,
        stores: u64,
        store_ok: u64,
    }

    extern "C" fn load(
        context: *mut MemoryContext,
        addr: u64,
        size: u64,
        _signed: u64,
    ) -> LoadResult {
        let context = unsafe { &mut *context };
        context.loads += 1;
        context.last_addr = addr;
        context.last_size = size;
        LoadResult {
            value: context.load_value,
            ok: 1,
        }
    }

    extern "C" fn store(context: *mut MemoryContext, addr: u64, value: u64, size: u64) -> u64 {
        let context = unsafe { &mut *context };
        context.stores += 1;
        context.last_addr = addr;
        context.last_size = size;
        context.store_value = value;
        if context.store_ok != 0 {
            context.committed_value = value;
        }
        context.store_ok
    }

    let old = VReg::Virtual(crate::smir::ir::types::VirtualId(40));
    let result = VReg::Virtual(crate::smir::ir::types::VirtualId(41));
    let flags_result = VReg::Virtual(crate::smir::ir::types::VirtualId(42));
    let source = VReg::Arch(ArchReg::X86(X86Reg::R16));
    let address = Address::Absolute(0x4000);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Load {
            dst: old,
            addr: address.clone(),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::Add {
            dst: result,
            src1: old,
            src2: SrcOperand::Reg(source),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::Store {
            src: result,
            addr: address,
            width: MemWidth::B8,
        },
    );
    builder.push_op(
        0x1000,
        OpKind::Add {
            dst: flags_result,
            src1: old,
            src2: SrcOperand::Reg(source),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    let lowered = lowerer
        .lower_function(&builder.finish())
        .expect("lower helper-backed scalar RMW");
    let exec =
        ExecMem::new(&lowerer.finalize().expect("finalize scalar RMW")).expect("map scalar RMW");

    const INCOMING_FLAGS: u64 = 0x2 | 0x8D5;
    let initial_gprs = {
        let mut gprs = [0u64; 32];
        for (index, value) in gprs.iter_mut().enumerate() {
            *value = 0xA500_0000_0000_0000 | index as u64;
        }
        gprs[16] = 1;
        gprs
    };

    let mut success_context = MemoryContext {
        load_value: u64::MAX,
        committed_value: 0xDEAD_BEEF_DEAD_BEEF,
        store_ok: 1,
        ..MemoryContext::default()
    };
    let mut success = GuestRegs::default();
    success.gpr = initial_gprs;
    success.rflags = INCOMING_FLAGS;
    success.exit_pc = 0xAAAA_BBBB_CCCC_DDDD;
    success.ctx = (&mut success_context as *mut MemoryContext) as u64;
    success.load_fn = load as usize as u64;
    success.store_fn = store as usize as u64;
    exec.run(lowered.entry_offset, &mut success);

    assert_eq!(success_context.loads, 1);
    assert_eq!(success_context.stores, 1);
    assert_eq!(success_context.last_addr, 0x4000);
    assert_eq!(success_context.last_size, 8);
    assert_eq!(success_context.store_value, 0);
    assert_eq!(success_context.committed_value, 0);
    assert_eq!(success.gpr, initial_gprs, "success must preserve every GPR");
    assert_eq!(success.rflags & 0x8D5, 0x55, "ADD result flags");
    assert_eq!(success.exit_pc, 0xAAAA_BBBB_CCCC_DDDD);

    let mut fault_context = MemoryContext {
        load_value: u64::MAX,
        committed_value: 0xDEAD_BEEF_DEAD_BEEF,
        store_ok: 0,
        ..MemoryContext::default()
    };
    let mut fault = GuestRegs::default();
    fault.gpr = initial_gprs;
    fault.rflags = INCOMING_FLAGS;
    fault.ctx = (&mut fault_context as *mut MemoryContext) as u64;
    fault.load_fn = load as usize as u64;
    fault.store_fn = store as usize as u64;
    exec.run(lowered.entry_offset, &mut fault);

    assert_eq!(fault_context.loads, 1);
    assert_eq!(fault_context.stores, 1);
    assert_eq!(fault_context.store_value, 0, "computed value reaches store");
    assert_eq!(
        fault_context.committed_value, 0xDEAD_BEEF_DEAD_BEEF,
        "failed store must not commit memory"
    );
    assert_eq!(fault.gpr, initial_gprs, "fault must preserve every GPR");
    assert_eq!(
        fault.rflags & 0x8D5,
        INCOMING_FLAGS & 0x8D5,
        "fault must preserve every arithmetic status flag"
    );
    assert_eq!(fault.exit_pc, 0x1000, "fault must restart current PC");
}
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_helper_backed_memory_destination_unary_commits_flags_only_after_store() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    #[repr(C)]
    struct LoadResult {
        value: u64,
        ok: u64,
    }

    #[derive(Default)]
    struct MemoryContext {
        load_value: u64,
        store_value: u64,
        committed_value: u64,
        loads: u64,
        stores: u64,
        store_ok: u64,
    }

    extern "C" fn load(
        context: *mut MemoryContext,
        _addr: u64,
        _size: u64,
        _signed: u64,
    ) -> LoadResult {
        let context = unsafe { &mut *context };
        context.loads += 1;
        LoadResult {
            value: context.load_value,
            ok: 1,
        }
    }

    extern "C" fn store(context: *mut MemoryContext, _addr: u64, value: u64, _size: u64) -> u64 {
        let context = unsafe { &mut *context };
        context.stores += 1;
        context.store_value = value;
        if context.store_ok != 0 {
            context.committed_value = value;
        }
        context.store_ok
    }

    let old = VReg::Virtual(crate::smir::ir::types::VirtualId(50));
    let result = VReg::Virtual(crate::smir::ir::types::VirtualId(51));
    let flags_result = VReg::Virtual(crate::smir::ir::types::VirtualId(52));
    let address = Address::Absolute(0x5000);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x2000);
    builder.push_op(
        0x2000,
        OpKind::Load {
            dst: old,
            addr: address.clone(),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
    );
    builder.push_op(
        0x2000,
        OpKind::Inc {
            dst: result,
            src: old,
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0x2000,
        OpKind::Store {
            src: result,
            addr: address,
            width: MemWidth::B8,
        },
    );
    builder.push_op(
        0x2000,
        OpKind::Inc {
            dst: flags_result,
            src: old,
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    let lowered = lowerer
        .lower_function(&builder.finish())
        .expect("lower helper-backed scalar unary RMW");
    let exec = ExecMem::new(
        &lowerer
            .finalize()
            .expect("finalize helper-backed scalar unary RMW"),
    )
    .expect("map helper-backed scalar unary RMW");

    const INCOMING_FLAGS: u64 = 0x2 | 0x8D5;
    const INITIAL_VALUE: u64 = 0x7FFF_FFFF_FFFF_FFFF;
    const RESULT_VALUE: u64 = 0x8000_0000_0000_0000;
    let initial_gprs = {
        let mut gprs = [0u64; 32];
        for (index, value) in gprs.iter_mut().enumerate() {
            *value = 0xB600_0000_0000_0000 | index as u64;
        }
        gprs
    };

    let mut success_context = MemoryContext {
        load_value: INITIAL_VALUE,
        committed_value: 0xDEAD_BEEF_DEAD_BEEF,
        store_ok: 1,
        ..MemoryContext::default()
    };
    let mut success = GuestRegs::default();
    success.gpr = initial_gprs;
    success.rflags = INCOMING_FLAGS;
    success.exit_pc = 0xAAAA_BBBB_CCCC_DDDD;
    success.ctx = (&mut success_context as *mut MemoryContext) as u64;
    success.load_fn = load as usize as u64;
    success.store_fn = store as usize as u64;
    exec.run(lowered.entry_offset, &mut success);

    assert_eq!(success_context.loads, 1);
    assert_eq!(success_context.stores, 1);
    assert_eq!(success_context.store_value, RESULT_VALUE);
    assert_eq!(success_context.committed_value, RESULT_VALUE);
    assert_eq!(success.gpr, initial_gprs, "success must preserve every GPR");
    assert_eq!(
        success.rflags & 0x8D5,
        0x895,
        "INC result flags must preserve incoming CF"
    );
    assert_eq!(success.exit_pc, 0xAAAA_BBBB_CCCC_DDDD);

    let mut fault_context = MemoryContext {
        load_value: INITIAL_VALUE,
        committed_value: 0xDEAD_BEEF_DEAD_BEEF,
        store_ok: 0,
        ..MemoryContext::default()
    };
    let mut fault = GuestRegs::default();
    fault.gpr = initial_gprs;
    fault.rflags = INCOMING_FLAGS;
    fault.ctx = (&mut fault_context as *mut MemoryContext) as u64;
    fault.load_fn = load as usize as u64;
    fault.store_fn = store as usize as u64;
    exec.run(lowered.entry_offset, &mut fault);

    assert_eq!(fault_context.loads, 1);
    assert_eq!(fault_context.stores, 1);
    assert_eq!(fault_context.store_value, RESULT_VALUE);
    assert_eq!(
        fault_context.committed_value, 0xDEAD_BEEF_DEAD_BEEF,
        "failed store must not commit memory"
    );
    assert_eq!(fault.gpr, initial_gprs, "fault must preserve every GPR");
    assert_eq!(
        fault.rflags & 0x8D5,
        INCOMING_FLAGS & 0x8D5,
        "fault must preserve every arithmetic status flag"
    );
    assert_eq!(fault.exit_pc, 0x2000, "fault must restart current PC");
}
#[cfg(feature = "smir-jit")]
#[test]
fn helper_backed_memory_destination_cl_shift_emits_both_native_replays() {
    let old = VReg::Virtual(crate::smir::ir::types::VirtualId(63));
    let result = VReg::Virtual(crate::smir::ir::types::VirtualId(64));
    let flags_result = VReg::Virtual(crate::smir::ir::types::VirtualId(65));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let address = Address::Absolute(0x7000);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x4000);
    builder.push_op(
        0x4000,
        OpKind::Load {
            dst: old,
            addr: address.clone(),
            width: MemWidth::B4,
            sign: SignExtend::Zero,
        },
    );
    builder.push_op(
        0x4000,
        OpKind::Shl {
            dst: result,
            src: old,
            amount: SrcOperand::Reg(rcx),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0x4000,
        OpKind::Store {
            src: result,
            addr: address,
            width: MemWidth::B4,
        },
    );
    builder.push_op(
        0x4000,
        OpKind::Shl {
            dst: flags_result,
            src: old,
            amount: SrcOperand::Reg(rcx),
            width: OpWidth::W32,
            flags: FlagUpdate::All,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer
        .lower_function(&builder.finish())
        .expect("lower helper-backed CL shift RMW");
    let code = lowerer
        .finalize()
        .expect("finalize helper-backed CL shift RMW");
    assert_eq!(
        code.windows(2)
            .filter(|bytes| *bytes == [0xD3, 0xE0])
            .count(),
        2,
        "SHL EAX,CL must execute once speculatively and once after store"
    );
}
#[cfg(feature = "smir-jit")]
#[test]
fn helper_backed_subword_cl_shifts_emit_original_operand_boundary_cf_merge() {
    let lower = |tag: u8, mem_width: MemWidth| {
        let old = VReg::Virtual(crate::smir::ir::types::VirtualId(66));
        let result = VReg::Virtual(crate::smir::ir::types::VirtualId(67));
        let flags_result = VReg::Virtual(crate::smir::ir::types::VirtualId(68));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let address = Address::Absolute(0x7100);
        let width = mem_width.to_op_width().unwrap();
        let shift = |dst, src, flags| match tag {
            4 => OpKind::Shl {
                dst,
                src,
                amount: SrcOperand::Reg(rcx),
                width,
                flags,
            },
            5 => OpKind::Shr {
                dst,
                src,
                amount: SrcOperand::Reg(rcx),
                width,
                flags,
            },
            _ => unreachable!(),
        };
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x4100);
        builder.push_op(
            0x4100,
            OpKind::Load {
                dst: old,
                addr: address.clone(),
                width: mem_width,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(0x4100, shift(result, old, FlagUpdate::None));
        builder.push_op(
            0x4100,
            OpKind::Store {
                src: result,
                addr: address,
                width: mem_width,
            },
        );
        builder.push_op(0x4100, shift(flags_result, old, FlagUpdate::All));
        builder.set_terminator(Terminator::Return { values: vec![] });

        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        lowerer
            .lower_function(&builder.finish())
            .expect("lower helper-backed subword CL shift RMW");
        lowerer
            .finalize()
            .expect("finalize helper-backed subword CL shift RMW")
    };

    for (name, code, replay, boundary_merge) in [
        (
            "SHL r/m8,CL",
            lower(4, MemWidth::B1),
            &[0xD2, 0xE0][..],
            &[
                0x48, 0x8B, 0x44, 0x24, 0x08, // mov rax,[rsp+8]
                0x48, 0x83, 0xE0, 0x01, // and rax,1
                0x48, 0x09, 0x04, 0x24, // or [rsp],rax
            ][..],
        ),
        (
            "SHR r/m16,CL",
            lower(5, MemWidth::B2),
            &[0x66, 0xD3, 0xE8][..],
            &[
                0x48, 0x8B, 0x44, 0x24, 0x08, // mov rax,[rsp+8]
                0x48, 0xC1, 0xE8, 0x0F, // shr rax,15
                0x48, 0x83, 0xE0, 0x01, // and rax,1
                0x48, 0x09, 0x04, 0x24, // or [rsp],rax
            ][..],
        ),
    ] {
        assert_eq!(
            code.windows(replay.len())
                .filter(|bytes| *bytes == replay)
                .count(),
            2,
            "{name} must execute once speculatively and once after store"
        );
        assert!(
            code.windows(boundary_merge.len())
                .any(|bytes| bytes == boundary_merge),
            "{name} must reconstruct boundary CF from the original operand: {code:02X?}"
        );
    }
}
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_helper_backed_memory_destination_shift_commits_flags_only_after_store() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    #[repr(C)]
    struct LoadResult {
        value: u64,
        ok: u64,
    }

    #[derive(Default)]
    struct MemoryContext {
        load_value: u64,
        store_value: u64,
        committed_value: u64,
        loads: u64,
        stores: u64,
        store_ok: u64,
    }

    extern "C" fn load(
        context: *mut MemoryContext,
        _addr: u64,
        _size: u64,
        _signed: u64,
    ) -> LoadResult {
        let context = unsafe { &mut *context };
        context.loads += 1;
        LoadResult {
            value: context.load_value,
            ok: 1,
        }
    }

    extern "C" fn store(context: *mut MemoryContext, _addr: u64, value: u64, size: u64) -> u64 {
        let context = unsafe { &mut *context };
        let value = match size {
            1 => value & u64::from(u8::MAX),
            2 => value & u64::from(u16::MAX),
            4 => value & u64::from(u32::MAX),
            8 => value,
            _ => return 0,
        };
        context.stores += 1;
        context.store_value = value;
        if context.store_ok != 0 {
            context.committed_value = value;
        }
        context.store_ok
    }

    let old = VReg::Virtual(crate::smir::ir::types::VirtualId(60));
    let result = VReg::Virtual(crate::smir::ir::types::VirtualId(61));
    let flags_result = VReg::Virtual(crate::smir::ir::types::VirtualId(62));
    let address = Address::Absolute(0x6000);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x3000);
    builder.push_op(
        0x3000,
        OpKind::Load {
            dst: old,
            addr: address.clone(),
            width: MemWidth::B2,
            sign: SignExtend::Zero,
        },
    );
    builder.push_op(
        0x3000,
        OpKind::Shl {
            dst: result,
            src: old,
            amount: SrcOperand::Imm(4),
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        },
    );
    builder.push_op(
        0x3000,
        OpKind::Store {
            src: result,
            addr: address,
            width: MemWidth::B2,
        },
    );
    builder.push_op(
        0x3000,
        OpKind::Shl {
            dst: flags_result,
            src: old,
            amount: SrcOperand::Imm(4),
            width: OpWidth::W16,
            flags: FlagUpdate::All,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    let lowered = lowerer
        .lower_function(&builder.finish())
        .expect("lower helper-backed scalar shift RMW");
    let exec = ExecMem::new(
        &lowerer
            .finalize()
            .expect("finalize helper-backed scalar shift RMW"),
    )
    .expect("map helper-backed scalar shift RMW");

    const INCOMING_FLAGS: u64 = 0x2 | 0x8D5;
    const INITIAL_VALUE: u64 = 0xF123;
    const RESULT_VALUE: u64 = 0x1230;
    const RESULT_STATUS: u64 = 0x15; // CF | PF | preserved AF
    let initial_gprs = {
        let mut gprs = [0u64; 32];
        for (index, value) in gprs.iter_mut().enumerate() {
            *value = 0xC700_0000_0000_0000 | index as u64;
        }
        gprs
    };

    let mut success_context = MemoryContext {
        load_value: INITIAL_VALUE,
        committed_value: 0xDEAD_BEEF_DEAD_BEEF,
        store_ok: 1,
        ..MemoryContext::default()
    };
    let mut success = GuestRegs::default();
    success.gpr = initial_gprs;
    success.rflags = INCOMING_FLAGS;
    success.exit_pc = 0xAAAA_BBBB_CCCC_DDDD;
    success.ctx = (&mut success_context as *mut MemoryContext) as u64;
    success.load_fn = load as usize as u64;
    success.store_fn = store as usize as u64;
    exec.run(lowered.entry_offset, &mut success);

    assert_eq!(success_context.loads, 1);
    assert_eq!(success_context.stores, 1);
    assert_eq!(success_context.store_value, RESULT_VALUE);
    assert_eq!(success_context.committed_value, RESULT_VALUE);
    assert_eq!(success.gpr, initial_gprs, "success must preserve every GPR");
    assert_eq!(success.rflags & 0x8D5, RESULT_STATUS, "SHL result flags");
    assert_eq!(success.exit_pc, 0xAAAA_BBBB_CCCC_DDDD);

    let mut fault_context = MemoryContext {
        load_value: INITIAL_VALUE,
        committed_value: 0xDEAD_BEEF_DEAD_BEEF,
        store_ok: 0,
        ..MemoryContext::default()
    };
    let mut fault = GuestRegs::default();
    fault.gpr = initial_gprs;
    fault.rflags = INCOMING_FLAGS;
    fault.ctx = (&mut fault_context as *mut MemoryContext) as u64;
    fault.load_fn = load as usize as u64;
    fault.store_fn = store as usize as u64;
    exec.run(lowered.entry_offset, &mut fault);

    assert_eq!(fault_context.loads, 1);
    assert_eq!(fault_context.stores, 1);
    assert_eq!(fault_context.store_value, RESULT_VALUE);
    assert_eq!(
        fault_context.committed_value, 0xDEAD_BEEF_DEAD_BEEF,
        "failed store must not commit memory"
    );
    assert_eq!(fault.gpr, initial_gprs, "fault must preserve every GPR");
    assert_eq!(
        fault.rflags & 0x8D5,
        INCOMING_FLAGS & 0x8D5,
        "fault must preserve every arithmetic status flag"
    );
    assert_eq!(fault.exit_pc, 0x3000, "fault must restart current PC");
}
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_state_backed_crc32c_preserves_gprs_and_flags() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    if !std::is_x86_feature_detected!("sse4.2") {
        return;
    }
    fn crc32c(mut crc: u32, data: u64, width: OpWidth) -> u32 {
        const POLY: u32 = 0x82F6_3B78;
        for byte in 0..(width.bits() / 8) {
            crc ^= ((data >> (byte * 8)) & 0xff) as u32;
            for _ in 0..8 {
                crc = (crc >> 1) ^ (POLY & 0u32.wrapping_sub(crc & 1));
            }
        }
        crc
    }

    struct Case {
        name: &'static str,
        dst: X86Reg,
        data: X86Reg,
        width: OpWidth,
        accumulator: u64,
        source: u64,
    }
    let cases = [
        Case {
            name: "CRC32 EBP,BPL alias",
            dst: X86Reg::Rbp,
            data: X86Reg::Rbp,
            width: OpWidth::W8,
            accumulator: 0x1234_56A5,
            source: 0x1234_56A5,
        },
        Case {
            name: "CRC32 ESP,BP",
            dst: X86Reg::Rsp,
            data: X86Reg::Rbp,
            width: OpWidth::W16,
            accumulator: 0x89AB_CDEF,
            source: 0x0123_4567_89AB_BEEF,
        },
        Case {
            name: "CRC32 EBP,ESP",
            dst: X86Reg::Rbp,
            data: X86Reg::Rsp,
            width: OpWidth::W32,
            accumulator: 0x1020_3040,
            source: 0xAABB_CCDD_DEAD_BEEF,
        },
        Case {
            name: "CRC32 RSP,RBP",
            dst: X86Reg::Rsp,
            data: X86Reg::Rbp,
            width: OpWidth::W64,
            accumulator: 0x7654_3210,
            source: 0x0123_4567_89AB_CDEF,
        },
        Case {
            name: "state-backed CRC32 R31,R16",
            dst: X86Reg::R31,
            data: X86Reg::R16,
            width: OpWidth::W64,
            accumulator: 0xA5A5_5A5A,
            source: 0xFEDC_BA98_7654_3210,
        },
    ];
    let x86 = |reg| VReg::Arch(ArchReg::X86(reg));
    const FLAGS: u64 = 0x8D5;

    for case in cases {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            OpKind::Crc32C {
                dst: x86(case.dst),
                crc: x86(case.dst),
                data: x86(case.data),
                data_width: case.width,
            },
        );
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
        let code = lowerer
            .finalize()
            .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
        let exec = ExecMem::new(&code)
            .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

        let mut regs = GuestRegs::default();
        for (index, value) in regs.gpr.iter_mut().enumerate() {
            *value = 0x1357_0000_2468_0000u64
                .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0101));
        }
        let dst_idx = case.dst.gpr_index().unwrap() as usize;
        let data_idx = case.data.gpr_index().unwrap() as usize;
        regs.gpr[dst_idx] = case.accumulator;
        if data_idx != dst_idx {
            regs.gpr[data_idx] = case.source;
        }
        regs.rflags = 0x2 | FLAGS;

        let mut expected = regs;
        expected.gpr[dst_idx] = u64::from(crc32c(
            regs.gpr[dst_idx] as u32,
            regs.gpr[data_idx],
            case.width,
        ));

        exec.run(lowered.entry_offset, &mut regs);

        assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
        assert_eq!(
            regs.rflags & FLAGS,
            expected.rflags & FLAGS,
            "{} flags",
            case.name
        );
    }
}
