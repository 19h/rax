//! Register-form XADD lowering and native differential coverage.

use super::*;
use crate::smir::ir::ops::{OpKind, X86GprOperand, X86OpHint, X86XaddOp};
use crate::smir::lower::x86_64::x86_xadd_shape_valid;

fn xadd(dst: X86GprOperand, src: X86GprOperand, width: OpWidth, flags: FlagUpdate) -> OpKind {
    OpKind::X86Xadd(X86XaddOp {
        dst,
        src,
        width,
        flags,
    })
}

#[test]
fn lower_xadd_emits_direct_high_byte_rex_and_flag_preservation_encodings() {
    let rax_rbx = lower_single_op(xadd(
        X86GprOperand::low(X86Reg::Rax),
        X86GprOperand::low(X86Reg::Rbx),
        OpWidth::W64,
        FlagUpdate::All,
    ));
    assert!(
        rax_rbx
            .windows(4)
            .any(|bytes| bytes == [0x48, 0x0F, 0xC1, 0xD8]),
        "XADD RAX,RBX: {rax_rbx:02X?}"
    );

    let ah_bh = lower_single_op(xadd(
        X86GprOperand::high(X86Reg::Rax),
        X86GprOperand::high(X86Reg::Rbx),
        OpWidth::W8,
        FlagUpdate::All,
    ));
    assert!(
        ah_bh.windows(3).any(|bytes| bytes == [0x0F, 0xC0, 0xFC]),
        "XADD AH,BH: {ah_bh:02X?}"
    );

    let sil_dil = lower_single_op(xadd(
        X86GprOperand::low(X86Reg::Rsi),
        X86GprOperand::low(X86Reg::Rdi),
        OpWidth::W8,
        FlagUpdate::None,
    ));
    assert!(
        sil_dil
            .windows(6)
            .any(|bytes| bytes == [0x9C, 0x40, 0x0F, 0xC0, 0xFE, 0x9D]),
        "flag-preserving XADD SIL,DIL: {sil_dil:02X?}"
    );
}

#[test]
fn lower_xadd_state_snapshot_commits_stack_and_egpr_lanes() {
    let spl_bpl = lower_single_op(xadd(
        X86GprOperand::low(X86Reg::Rsp),
        X86GprOperand::low(X86Reg::Rbp),
        OpWidth::W8,
        FlagUpdate::All,
    ));
    assert!(
        spl_bpl
            .windows(4)
            .any(|bytes| bytes == [0x40, 0x0F, 0xC0, 0xFA]),
        "state-backed scratch XADD: {spl_bpl:02X?}"
    );
    assert!(
        spl_bpl.windows(3).any(|bytes| bytes == [0x88, 0x50, 0x20]),
        "SPL destination slot commit: {spl_bpl:02X?}"
    );
    assert!(
        spl_bpl
            .windows(4)
            .any(|bytes| bytes == [0x40, 0x88, 0x78, 0x28]),
        "BPL source slot commit: {spl_bpl:02X?}"
    );
    assert!(
        spl_bpl
            .windows(4)
            .any(|bytes| bytes == [0x48, 0x89, 0x55, 0x00]),
        "saved guest RBP synchronization: {spl_bpl:02X?}"
    );

    let egpr = lower_single_op(xadd(
        X86GprOperand::low(X86Reg::R16),
        X86GprOperand::low(X86Reg::R31),
        OpWidth::W32,
        FlagUpdate::All,
    ));
    assert!(
        egpr.windows(7)
            .any(|bytes| bytes == [0x48, 0x89, 0xB8, 0xF8, 0x00, 0x00, 0x00]),
        "old R16D source write to R31 slot: {egpr:02X?}"
    );
    assert!(
        egpr.windows(7)
            .any(|bytes| bytes == [0x48, 0x89, 0x90, 0x80, 0x00, 0x00, 0x00]),
        "R16D sum destination slot: {egpr:02X?}"
    );
}

#[test]
fn lower_xadd_rejects_every_unencodable_shape_and_hint() {
    for malformed in [
        xadd(
            X86GprOperand::low(X86Reg::Xmm(0)),
            X86GprOperand::low(X86Reg::Rax),
            OpWidth::W64,
            FlagUpdate::All,
        ),
        xadd(
            X86GprOperand::high(X86Reg::Rsi),
            X86GprOperand::low(X86Reg::Rax),
            OpWidth::W8,
            FlagUpdate::All,
        ),
        xadd(
            X86GprOperand::high(X86Reg::Rax),
            X86GprOperand::low(X86Reg::R8),
            OpWidth::W8,
            FlagUpdate::All,
        ),
        xadd(
            X86GprOperand::high(X86Reg::Rax),
            X86GprOperand::high(X86Reg::Rbx),
            OpWidth::W16,
            FlagUpdate::All,
        ),
        xadd(
            X86GprOperand::low(X86Reg::Rax),
            X86GprOperand::low(X86Reg::Rbx),
            OpWidth::W64,
            FlagUpdate::Specific(FlagSet::ZF),
        ),
    ] {
        assert!(matches!(
            lower_single_op_err(malformed),
            LowerError::InvalidOperand { .. }
        ));
    }

    let exact = xadd(
        X86GprOperand::low(X86Reg::Rax),
        X86GprOperand::low(X86Reg::Rbx),
        OpWidth::W64,
        FlagUpdate::All,
    );
    assert!(matches!(
        lower_single_hinted_op_err(exact, X86OpHint::RexByteReg),
        LowerError::InvalidOperand { .. }
    ));
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_xadd_matches_direct_flag_oracle_for_aliases_widths_and_state_slots() {
    use crate::isa::x86_64::flags;
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const ARITHMETIC_FLAGS: u64 = 0x8D5;
    struct Case {
        name: &'static str,
        dst: X86GprOperand,
        src: X86GprOperand,
        width: OpWidth,
        update_flags: bool,
    }
    let cases = [
        Case {
            name: "XADD RAX,RBX direct",
            dst: X86GprOperand::low(X86Reg::Rax),
            src: X86GprOperand::low(X86Reg::Rbx),
            width: OpWidth::W64,
            update_flags: true,
        },
        Case {
            name: "XADD R8D,R8D direct self alias",
            dst: X86GprOperand::low(X86Reg::R8),
            src: X86GprOperand::low(X86Reg::R8),
            width: OpWidth::W32,
            update_flags: true,
        },
        Case {
            name: "XADD AH,BH high bytes",
            dst: X86GprOperand::high(X86Reg::Rax),
            src: X86GprOperand::high(X86Reg::Rbx),
            width: OpWidth::W8,
            update_flags: true,
        },
        Case {
            name: "XADD AL,AH parent alias",
            dst: X86GprOperand::low(X86Reg::Rax),
            src: X86GprOperand::high(X86Reg::Rax),
            width: OpWidth::W8,
            update_flags: true,
        },
        Case {
            name: "XADD SPL,BPL state bytes",
            dst: X86GprOperand::low(X86Reg::Rsp),
            src: X86GprOperand::low(X86Reg::Rbp),
            width: OpWidth::W8,
            update_flags: true,
        },
        Case {
            name: "XADD BP,R16W state partial",
            dst: X86GprOperand::low(X86Reg::Rbp),
            src: X86GprOperand::low(X86Reg::R16),
            width: OpWidth::W16,
            update_flags: true,
        },
        Case {
            name: "XADD R16D,R31D EGPR zero extension",
            dst: X86GprOperand::low(X86Reg::R16),
            src: X86GprOperand::low(X86Reg::R31),
            width: OpWidth::W32,
            update_flags: true,
        },
        Case {
            name: "XADD RSP,RSP state self alias",
            dst: X86GprOperand::low(X86Reg::Rsp),
            src: X86GprOperand::low(X86Reg::Rsp),
            width: OpWidth::W64,
            update_flags: true,
        },
        Case {
            name: "flagless XADD R16,RAX",
            dst: X86GprOperand::low(X86Reg::R16),
            src: X86GprOperand::low(X86Reg::Rax),
            width: OpWidth::W64,
            update_flags: false,
        },
    ];

    let read = |gpr: &[u64; 32], operand: X86GprOperand, width: OpWidth| {
        let value = gpr[operand.gpr_index().unwrap() as usize];
        if operand.high_byte {
            (value >> 8) & 0xFF
        } else {
            value & width.mask()
        }
    };
    let write = |gpr: &mut [u64; 32], operand: X86GprOperand, width: OpWidth, value: u64| {
        let slot = &mut gpr[operand.gpr_index().unwrap() as usize];
        if operand.high_byte {
            *slot = (*slot & !0xFF00) | ((value & 0xFF) << 8);
        } else {
            *slot = match width {
                OpWidth::W8 => (*slot & !0xFF) | (value & 0xFF),
                OpWidth::W16 => (*slot & !0xFFFF) | (value & 0xFFFF),
                OpWidth::W32 => value & 0xFFFF_FFFF,
                OpWidth::W64 => value,
                _ => unreachable!(),
            };
        }
    };

    for case in cases {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(
            0x1000,
            xadd(
                case.dst,
                case.src,
                case.width,
                if case.update_flags {
                    FlagUpdate::All
                } else {
                    FlagUpdate::None
                },
            ),
        );
        builder.set_terminator(Terminator::Return { values: Vec::new() });
        let mut lowerer = X86_64Lowerer::new();
        let lowered = lowerer
            .lower_function(&builder.finish())
            .unwrap_or_else(|error| panic!("{} lower: {error:?}", case.name));
        let code = lowerer
            .finalize()
            .unwrap_or_else(|error| panic!("{} finalize: {error:?}", case.name));
        let exec = ExecMem::new(&code)
            .unwrap_or_else(|error| panic!("{} executable mapping: {error:?}", case.name));

        let mut regs = GuestRegs::default();
        for (index, value) in regs.gpr.iter_mut().enumerate() {
            *value = 0x8123_4567_89AB_00F1u64
                .wrapping_add((index as u64).wrapping_mul(0x0101_1111_2222_0137));
        }
        regs.rflags = ARITHMETIC_FLAGS;
        let mut expected = regs;
        let old_dst = read(&expected.gpr, case.dst, case.width);
        let old_src = read(&expected.gpr, case.src, case.width);
        let sum = old_dst.wrapping_add(old_src) & case.width.mask();
        write(&mut expected.gpr, case.src, case.width, old_dst);
        write(&mut expected.gpr, case.dst, case.width, sum);
        if case.update_flags {
            flags::update_flags_add(
                &mut expected.rflags,
                old_dst,
                old_src,
                sum,
                (case.width.bits() / 8) as u8,
            );
        }

        exec.run(lowered.entry_offset, &mut regs);
        assert_eq!(regs.gpr, expected.gpr, "{} GPR file", case.name);
        assert_eq!(
            regs.rflags & ARITHMETIC_FLAGS,
            expected.rflags & ARITHMETIC_FLAGS,
            "{} arithmetic flags",
            case.name
        );
    }
}

#[test]
fn xadd_validator_accepts_only_unhinted_architectural_shapes() {
    let op = crate::smir::ir::ops::SmirOp::new(
        crate::smir::ir::types::OpId(0),
        0x1000,
        xadd(
            X86GprOperand::low(X86Reg::Rax),
            X86GprOperand::low(X86Reg::Rbx),
            OpWidth::W64,
            FlagUpdate::All,
        ),
    );
    assert!(x86_xadd_shape_valid(&op));
}
