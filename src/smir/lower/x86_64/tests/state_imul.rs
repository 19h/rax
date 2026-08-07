//! State-backed register IMUL lowering coverage.

use super::*;
use crate::smir::OpId;

fn gpr(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::gpr(index)))
}

fn muls(
    dst_lo: u8,
    dst_hi: Option<u8>,
    src1: u8,
    src2: SrcOperand,
    width: OpWidth,
    flags: FlagUpdate,
) -> OpKind {
    OpKind::MulS {
        dst_lo: gpr(dst_lo),
        dst_hi: dst_hi.map(gpr),
        src1: gpr(src1),
        src2,
        width,
        flags,
    }
}

#[test]
fn state_imul_reads_all_sources_before_committing_results() {
    let register = lower_single_op(muls(
        4,
        None,
        5,
        SrcOperand::Reg(gpr(16)),
        OpWidth::W64,
        FlagUpdate::All,
    ));
    assert!(
        register
            .windows(4)
            .any(|bytes| bytes == [0x48, 0x8B, 0x50, 0x28]),
        "must read guest RBP before any commit: {register:02X?}"
    );
    assert!(
        register
            .windows(7)
            .any(|bytes| bytes == [0x48, 0x8B, 0xB8, 0x80, 0x00, 0x00, 0x00]),
        "must read guest R16 from its canonical slot: {register:02X?}"
    );
    assert!(
        register
            .windows(4)
            .any(|bytes| bytes == [0x48, 0x0F, 0xAF, 0xD7]),
        "must multiply the snapshotted operands in scratch GPRs: {register:02X?}"
    );
    assert!(
        register
            .windows(4)
            .any(|bytes| bytes == [0x48, 0x89, 0x50, 0x20]),
        "must commit guest RSP through GuestRegs: {register:02X?}"
    );

    let word = lower_single_hinted_op(
        muls(
            5,
            None,
            4,
            SrcOperand::Imm(0x1234),
            OpWidth::W16,
            FlagUpdate::All,
        ),
        X86OpHint::ImulImm32,
    );
    assert!(
        word.windows(5)
            .any(|bytes| bytes == [0x66, 0x69, 0xD2, 0x34, 0x12]),
        "word IMUL must retain its imm16-bearing opcode form: {word:02X?}"
    );
    assert!(
        word.windows(4)
            .any(|bytes| bytes == [0x66, 0x89, 0x50, 0x28]),
        "word IMUL must partially commit the RBP slot: {word:02X?}"
    );
    assert!(
        word.windows(4)
            .any(|bytes| bytes == [0x66, 0x89, 0x55, 0x00]),
        "word IMUL must synchronize the prologue-saved RBP word: {word:02X?}"
    );

    let imm8 = lower_single_hinted_op(
        muls(
            16,
            None,
            5,
            SrcOperand::Imm(-7),
            OpWidth::W64,
            FlagUpdate::All,
        ),
        X86OpHint::ImulImm8,
    );
    assert!(
        imm8.windows(4)
            .any(|bytes| bytes == [0x48, 0x6B, 0xD2, 0xF9]),
        "imm8 IMUL must retain sign-extension encoding: {imm8:02X?}"
    );
    assert!(
        imm8.windows(7)
            .any(|bytes| bytes == [0x48, 0x89, 0x90, 0x80, 0x00, 0x00, 0x00]),
        "IMUL must commit APX R16 through GuestRegs: {imm8:02X?}"
    );
}

#[test]
fn state_implicit_imul_emits_every_architectural_width() {
    for (width, expected) in [
        (OpWidth::W8, &[0x40, 0xF6, 0xEF][..]),
        (OpWidth::W16, &[0x66, 0xF7, 0xEF][..]),
        (OpWidth::W32, &[0xF7, 0xEF][..]),
        (OpWidth::W64, &[0x48, 0xF7, 0xEF][..]),
    ] {
        let bytes = lower_single_op(muls(
            0,
            (width != OpWidth::W8).then_some(2),
            0,
            SrcOperand::Reg(gpr(4)),
            width,
            FlagUpdate::All,
        ));
        assert!(
            bytes
                .windows(expected.len())
                .any(|window| window == expected),
            "implicit {width:?} IMUL is absent: {bytes:02X?}"
        );
        assert!(
            bytes
                .windows(4)
                .any(|window| window == [0x48, 0x8B, 0x78, 0x20])
                || bytes.windows(3).any(|window| window == [0x8B, 0x78, 0x20])
                || bytes
                    .windows(4)
                    .any(|window| window == [0x66, 0x8B, 0x78, 0x20])
                || bytes
                    .windows(4)
                    .any(|window| window == [0x40, 0x8A, 0x78, 0x20]),
            "implicit {width:?} IMUL must source guest RSP state: {bytes:02X?}"
        );
    }

    let nf = lower_single_op(muls(
        0,
        Some(2),
        0,
        SrcOperand::Reg(gpr(5)),
        OpWidth::W64,
        FlagUpdate::None,
    ));
    let group3 = nf
        .windows(3)
        .position(|bytes| bytes == [0x48, 0xF7, 0xEF])
        .expect("flag-suppressed implicit IMUL");
    assert!(nf[..group3].contains(&0x9C), "missing PUSHFQ: {nf:02X?}");
    assert!(nf[group3 + 3..].contains(&0x9D), "missing POPFQ: {nf:02X?}");
}

#[test]
fn state_imul_lowering_rejects_malformed_candidates() {
    let malformed = [
        muls(
            4,
            None,
            5,
            SrcOperand::Reg(gpr(6)),
            OpWidth::W8,
            FlagUpdate::All,
        ),
        muls(
            0,
            Some(4),
            0,
            SrcOperand::Reg(gpr(5)),
            OpWidth::W16,
            FlagUpdate::All,
        ),
        muls(
            4,
            None,
            5,
            SrcOperand::Reg(VReg::Virtual(crate::smir::ir::types::VirtualId(0))),
            OpWidth::W64,
            FlagUpdate::All,
        ),
    ];
    for kind in malformed {
        let op = SmirOp::new(OpId(0), 0x1000, kind.clone());
        assert!(x86_state_imul_candidate(&op));
        assert!(!x86_state_imul_valid(&op));
        assert!(matches!(
            lower_single_op_err(kind),
            LowerError::InvalidOperand { .. } | LowerError::InvalidRegister(_)
        ));
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_state_imul_matches_independent_signed_product_oracle() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    const CF: u64 = 1 << 0;
    const OF: u64 = 1 << 11;

    fn index(reg: VReg) -> usize {
        match reg {
            VReg::Arch(ArchReg::X86(reg)) => reg.gpr_index().unwrap() as usize,
            _ => unreachable!(),
        }
    }

    fn signed(value: u64, width: OpWidth) -> i128 {
        match width {
            OpWidth::W8 => i128::from(value as i8),
            OpWidth::W16 => i128::from(value as i16),
            OpWidth::W32 => i128::from(value as i32),
            OpWidth::W64 => i128::from(value as i64),
            OpWidth::W128 => unreachable!(),
        }
    }

    fn write(old: u64, value: u64, width: OpWidth) -> u64 {
        match width {
            OpWidth::W8 => (old & !0xFF) | (value & 0xFF),
            OpWidth::W16 => (old & !0xFFFF) | (value & 0xFFFF),
            OpWidth::W32 => value & 0xFFFF_FFFF,
            OpWidth::W64 => value,
            OpWidth::W128 => unreachable!(),
        }
    }

    fn oracle(kind: &OpKind, entry: &[u64; 32]) -> ([u64; 32], bool) {
        let OpKind::MulS {
            dst_lo,
            dst_hi,
            src1,
            src2,
            width,
            ..
        } = kind
        else {
            unreachable!()
        };
        let left = signed(entry[index(*src1)], *width);
        let right = match src2 {
            SrcOperand::Reg(reg) => signed(entry[index(*reg)], *width),
            SrcOperand::Imm(value) => signed(*value as u64, *width),
            _ => unreachable!(),
        };
        let product = left * right;
        let bits = width.bits() as u32;
        let mask = (1u128 << bits) - 1;
        let encoded = product as u128;
        let low = (encoded & mask) as u64;
        let high = ((encoded >> bits) & mask) as u64;
        let truncated = signed(low, *width);
        let overflow = truncated != product;

        let mut expected = *entry;
        if *width == OpWidth::W8 && dst_hi.is_none() {
            expected[index(*dst_lo)] =
                write(entry[index(*dst_lo)], low | (high << 8), OpWidth::W16);
        } else {
            expected[index(*dst_lo)] = write(entry[index(*dst_lo)], low, *width);
            if let Some(dst_hi) = dst_hi {
                expected[index(*dst_hi)] = write(entry[index(*dst_hi)], high, *width);
            }
        }
        (expected, overflow)
    }

    let cases = [
        (
            "word stack/EGPR",
            muls(
                5,
                None,
                4,
                SrcOperand::Reg(gpr(16)),
                OpWidth::W16,
                FlagUpdate::All,
            ),
            None,
        ),
        (
            "dword destination/source alias",
            muls(
                4,
                None,
                5,
                SrcOperand::Reg(gpr(4)),
                OpWidth::W32,
                FlagUpdate::All,
            ),
            None,
        ),
        (
            "qword EGPR destination alias",
            muls(
                16,
                None,
                17,
                SrcOperand::Reg(gpr(16)),
                OpWidth::W64,
                FlagUpdate::All,
            ),
            None,
        ),
        (
            "word imm16",
            muls(
                4,
                None,
                5,
                SrcOperand::Imm(0x1234),
                OpWidth::W16,
                FlagUpdate::All,
            ),
            Some(X86OpHint::ImulImm32),
        ),
        (
            "qword imm8",
            muls(
                5,
                None,
                4,
                SrcOperand::Imm(-7),
                OpWidth::W64,
                FlagUpdate::All,
            ),
            Some(X86OpHint::ImulImm8),
        ),
        (
            "APX NF preserves flags",
            muls(
                4,
                None,
                5,
                SrcOperand::Reg(gpr(16)),
                OpWidth::W64,
                FlagUpdate::None,
            ),
            None,
        ),
        (
            "implicit byte",
            muls(
                0,
                None,
                0,
                SrcOperand::Reg(gpr(4)),
                OpWidth::W8,
                FlagUpdate::All,
            ),
            None,
        ),
        (
            "implicit word",
            muls(
                0,
                Some(2),
                0,
                SrcOperand::Reg(gpr(5)),
                OpWidth::W16,
                FlagUpdate::All,
            ),
            None,
        ),
        (
            "implicit dword EGPR",
            muls(
                0,
                Some(2),
                0,
                SrcOperand::Reg(gpr(16)),
                OpWidth::W32,
                FlagUpdate::All,
            ),
            None,
        ),
        (
            "implicit qword stack",
            muls(
                0,
                Some(2),
                0,
                SrcOperand::Reg(gpr(4)),
                OpWidth::W64,
                FlagUpdate::All,
            ),
            None,
        ),
    ];

    for (name, kind, hint) in cases {
        for small in [false, true] {
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(0x1000, kind.clone());
            builder.set_terminator(Terminator::Return { values: vec![] });
            let mut function = builder.finish();
            function.blocks[0].ops[0].x86_hint = hint;

            let mut lowerer = X86_64Lowerer::new();
            let lowered = lowerer
                .lower_function(&function)
                .unwrap_or_else(|error| panic!("{name}: {error:?}"));
            let code = lowerer.finalize().unwrap();
            let exec = ExecMem::new(&code).unwrap();

            let mut regs = GuestRegs::default();
            for (slot, value) in regs.gpr.iter_mut().enumerate() {
                *value = if small {
                    (slot as u64) + 2
                } else {
                    0x8123_4567_89AB_CDEFu64
                        .wrapping_add((slot as u64).wrapping_mul(0x1111_2222_3333_4444))
                };
            }
            regs.rflags = 0x2 | 0x8D5;
            let entry = regs.gpr;
            let entry_flags = regs.rflags;
            let (expected, overflow) = oracle(&kind, &entry);

            exec.run(lowered.entry_offset, &mut regs);
            assert_eq!(regs.gpr, expected, "{name}, small={small}: GPRs");
            if matches!(
                kind,
                OpKind::MulS {
                    flags: FlagUpdate::None,
                    ..
                }
            ) {
                assert_eq!(regs.rflags, entry_flags, "{name}, small={small}: RFLAGS");
            } else {
                assert_eq!(regs.rflags & CF != 0, overflow, "{name}: CF");
                assert_eq!(regs.rflags & OF != 0, overflow, "{name}: OF");
            }
        }
    }
}
