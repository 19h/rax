//! tests::apx tests

use super::*;
use crate::smir::interpret::*;
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::flags::{FlagSet, FlagUpdate, MaterializedFlags};
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::types::ShiftOp;
use crate::smir::optimize::{OptLevel, optimize_function};

fn execute_optimized_apx_group3(
    bytes: &[u8],
    level: OptLevel,
    ctx: &mut SmirContext,
) -> BlockResult {
    use crate::smir::ir::types::SourceArch;
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    let mut lifter = X86_64Lifter::strict();
    let mut lctx = LiftContext::new(SourceArch::X86_64);
    let result = lifter.lift_insn(0x1000, bytes, &mut lctx).unwrap();
    assert_eq!(result.bytes_consumed, bytes.len());

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut function = builder.finish();
    function.blocks[0].ops = result.ops;
    optimize_function(&mut function, level);
    SmirInterpreter::new().execute_block(ctx, &mut FlatMemory::new(0x1000), &function.blocks[0])
}

#[test]
fn lifted_apx_group3_implicit_matches_nf_contract_at_o0_o1_o2() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    const CF_OF: u64 = (1 << 0) | (1 << 11);
    const STATUS: u64 = 0x08D5;
    const SEED: u64 = 0x2 | STATUS;

    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for nf in [false, true] {
            let p2 = if nf { 0x0C } else { 0x08 };
            for group in 4..=7 {
                let mut ctx = SmirContext::new_x86_64();
                ctx.flags.materialized = MaterializedFlags::from_rflags(SEED);
                ctx.flags.lazy = None;

                match group {
                    4 => {
                        ctx.write_vreg(rax, 1 << 63);
                        ctx.write_vreg(rdx, 0xAABB_CCDD_EEFF_0011);
                        ctx.write_vreg(rbx, 2);
                    }
                    5 => {
                        ctx.write_vreg(rax, i64::MAX as u64);
                        ctx.write_vreg(rdx, 0xAABB_CCDD_EEFF_0011);
                        ctx.write_vreg(rbx, 2);
                    }
                    6 => {
                        ctx.write_vreg(rax, 5);
                        ctx.write_vreg(rdx, 1);
                        ctx.write_vreg(rbx, 10);
                    }
                    7 => {
                        ctx.write_vreg(rax, (-100_i64) as u64);
                        ctx.write_vreg(rdx, u64::MAX);
                        ctx.write_vreg(rbx, 7);
                    }
                    _ => unreachable!(),
                }

                let bytes = [0x62, 0xF4, 0xFC, p2, 0xF7, 0xC3 | (group << 3)];
                let exit = execute_optimized_apx_group3(&bytes, level, &mut ctx);
                assert!(
                    matches!(exit, BlockResult::Exit(ExitReason::Halt)),
                    "group=/{group} NF={nf} {level:?}: {exit:?}"
                );

                match group {
                    4 => {
                        assert_eq!(ctx.read_vreg(rax), 0);
                        assert_eq!(ctx.read_vreg(rdx), 1);
                    }
                    5 => {
                        assert_eq!(ctx.read_vreg(rax), u64::MAX - 1);
                        assert_eq!(ctx.read_vreg(rdx), 0);
                    }
                    6 => {
                        let dividend = (1_u128 << 64) | 5;
                        assert_eq!(ctx.read_vreg(rax), (dividend / 10) as u64);
                        assert_eq!(ctx.read_vreg(rdx), (dividend % 10) as u64);
                    }
                    7 => {
                        assert_eq!(ctx.read_vreg(rax), (-14_i64) as u64);
                        assert_eq!(ctx.read_vreg(rdx), (-2_i64) as u64);
                    }
                    _ => unreachable!(),
                }

                ctx.flags.materialize_all();
                let flags = ctx.flags.materialized.to_rflags();
                if nf {
                    assert_eq!(flags & STATUS, SEED & STATUS, "NF flag image");
                } else if matches!(group, 4 | 5) {
                    assert_eq!(flags & CF_OF, CF_OF, "defined multiply flags");
                }
            }
        }
    }
}

#[test]
fn lifted_apx_group3_divide_errors_remain_noncommitting_at_o0_o1_o2() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    const SEED: u64 = 0x08D7;

    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for nf in [false, true] {
            let p2 = if nf { 0x0C } else { 0x08 };
            for (group, rax_value, rdx_value, divisor, name) in [
                (6, 0x1234, 0, 0, "DIV zero"),
                (6, 0, 1, 1, "DIV quotient overflow"),
                (7, 0, 1 << 63, u64::MAX, "IDIV quotient overflow"),
            ] {
                let mut ctx = SmirContext::new_x86_64();
                ctx.write_vreg(rax, rax_value);
                ctx.write_vreg(rdx, rdx_value);
                ctx.write_vreg(rbx, divisor);
                ctx.flags.materialized = MaterializedFlags::from_rflags(SEED);
                ctx.flags.lazy = None;

                let bytes = [0x62, 0xF4, 0xFC, p2, 0xF7, 0xC3 | (group << 3)];
                let exit = execute_optimized_apx_group3(&bytes, level, &mut ctx);
                assert!(
                    matches!(
                        exit,
                        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
                    ),
                    "{name} NF={nf} {level:?}: {exit:?}"
                );
                assert_eq!(ctx.read_vreg(rax), rax_value, "{name}: RAX commit");
                assert_eq!(ctx.read_vreg(rdx), rdx_value, "{name}: RDX commit");
                ctx.flags.materialize_all();
                assert_eq!(
                    ctx.flags.materialized.to_rflags() & 0x08D5,
                    SEED & 0x08D5,
                    "{name}: flags commit"
                );
            }
        }
    }
}

#[test]
fn lifted_apx_ndd_double_shifts_execute_aliases_partial_writes_and_nf_exactly() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();

    let base = 0x0123_4567_89AB_CDEFu64;
    let fill = 0xFEDC_BA98_7654_3210u64;
    ctx.write_vreg(rax, base);
    ctx.write_vreg(rbx, fill);
    execute_lifted_x86(
        &[0x62, 0xF4, 0xE4, 0x18, 0x24, 0xD8, 0x04],
        &mut ctx,
        &mut memory,
    );
    assert_eq!(ctx.read_vreg(rbx), (base << 4) | (fill >> 60));
    assert_eq!(ctx.read_vreg(rax), base);

    ctx.write_vreg(rax, base);
    ctx.write_vreg(rbx, fill);
    ctx.write_vreg(rcx, 4);
    execute_lifted_x86(&[0x62, 0xF4, 0xF4, 0x18, 0xAD, 0xD8], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rcx), (base >> 4) | (fill << 60));
    assert_eq!(ctx.read_vreg(rbx), fill);

    let old_rbx = 0xAABB_CCDD_EEFF_8001u64;
    ctx.write_vreg(rax, 0x1122_3344_5566_1234);
    ctx.write_vreg(rbx, old_rbx);
    execute_lifted_x86(
        &[0x62, 0xF4, 0x65, 0x18, 0x24, 0xD8, 0x04],
        &mut ctx,
        &mut memory,
    );
    let expected_low = ((0x1234u64 << 4) | (0x8001 >> 12)) & 0xFFFF;
    assert_eq!(
        ctx.read_vreg(rbx),
        (old_rbx & !0xFFFF) | expected_low,
        "W16 NDD writes must preserve the old destination's upper bits"
    );

    let rbp = VReg::Arch(ArchReg::X86(X86Reg::Rbp));
    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let r16 = VReg::Arch(ArchReg::X86(X86Reg::R16));
    let r31 = VReg::Arch(ArchReg::X86(X86Reg::R31));
    let undefined_seed = 0x08D7;
    for bytes in [
        &[0x66, 0x0F, 0xA4, 0xE5, 0x11][..],
        &[0x66, 0x0F, 0xAC, 0xE5, 0x11][..],
    ] {
        ctx.write_vreg(rbp, 0x3344_5566_8765_1357);
        ctx.write_vreg(rsp, 0x2233_4455_6677_8001);
        ctx.flags.materialized = MaterializedFlags::from_rflags(undefined_seed);
        ctx.flags.lazy = None;
        execute_lifted_x86(bytes, &mut ctx, &mut memory);
        ctx.flags.materialize_all();
        assert_eq!(ctx.read_vreg(rbp), 0x3344_5566_8765_1357);
        assert_eq!(
            ctx.flags.materialized.to_rflags() & 0x08D5,
            undefined_seed & 0x08D5,
            "W16 destructive double shift above the width must preserve flags"
        );
    }

    ctx.write_vreg(rbp, 0x3344_5566_8765_1357);
    ctx.write_vreg(r16, 0xAABB_CCDD_EEFF_2468);
    ctx.write_vreg(r31, 0xFFEE_DDCC_BBAA_8001);
    ctx.flags.materialized = MaterializedFlags::from_rflags(undefined_seed);
    ctx.flags.lazy = None;
    execute_lifted_x86(
        &[0x62, 0x64, 0x7D, 0x10, 0x24, 0xFD, 0x11],
        &mut ctx,
        &mut memory,
    );
    ctx.flags.materialize_all();
    assert_eq!(ctx.read_vreg(r16), 0xAABB_CCDD_EEFF_1357);
    assert_eq!(
        ctx.flags.materialized.to_rflags() & 0x08D5,
        undefined_seed & 0x08D5,
        "W16 NDD double shift above the width must preserve flags"
    );

    const STATUS_MASK: u64 = 0x08D5;
    let seed_flags = 0x0AD7;
    ctx.write_vreg(rax, base);
    ctx.write_vreg(rbx, fill);
    ctx.flags.materialized = MaterializedFlags::from_rflags(seed_flags);
    ctx.flags.lazy = None;
    execute_lifted_x86(
        &[0x62, 0xF4, 0xE4, 0x1C, 0x24, 0xD8, 0x04],
        &mut ctx,
        &mut memory,
    );
    ctx.flags.materialize_all();
    assert_eq!(
        ctx.flags.materialized.to_rflags() & STATUS_MASK,
        seed_flags & STATUS_MASK,
        "APX NF double shift must preserve every status flag"
    );
}
#[test]
fn lifted_apx_ndd_single_shift_cl_alias_executes_widths_and_nf_exactly() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();
    let source = 0x0123_4567_89AB_C081u64;

    for (bytes, width, old_rcx) in [
        (
            &[0x62, 0xF4, 0x74, 0x18, 0xD2, 0xE0][..],
            OpWidth::W8,
            0xAABB_CCDD_EEFF_0004u64,
        ),
        (
            &[0x62, 0xF4, 0x75, 0x18, 0xD3, 0xE0][..],
            OpWidth::W16,
            0xAABB_CCDD_EEFF_0004,
        ),
        (
            &[0x62, 0xF4, 0x74, 0x18, 0xD3, 0xE0][..],
            OpWidth::W32,
            0xAABB_CCDD_0000_0004,
        ),
        (&[0x62, 0xF4, 0xF4, 0x18, 0xD3, 0xE0][..], OpWidth::W64, 4),
    ] {
        ctx.write_vreg(rax, source);
        ctx.write_vreg(rcx, old_rcx);
        execute_lifted_x86(bytes, &mut ctx, &mut memory);
        let low = (source << 4) & width.mask();
        let expected = match width {
            OpWidth::W8 | OpWidth::W16 => (old_rcx & !width.mask()) | low,
            OpWidth::W32 | OpWidth::W64 => low,
            OpWidth::W128 => unreachable!(),
        };
        assert_eq!(ctx.read_vreg(rcx), expected, "{width:?}");
    }

    const STATUS_MASK: u64 = 0x08D5;
    let seed_flags = 0x08D7;
    ctx.write_vreg(rax, source);
    ctx.write_vreg(rcx, 4);
    ctx.flags.materialized = MaterializedFlags::from_rflags(seed_flags);
    ctx.flags.lazy = None;
    execute_lifted_x86(&[0x62, 0xF4, 0xF4, 0x1C, 0xD3, 0xE0], &mut ctx, &mut memory);
    ctx.flags.materialize_all();
    assert_eq!(
        ctx.flags.materialized.to_rflags() & STATUS_MASK,
        seed_flags & STATUS_MASK,
        "APX NF single shift must preserve every status flag"
    );
}
#[test]
fn smir_x86_adx_matches_width_carry_chain_and_flag_contracts() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    const CF: u64 = 1 << 0;
    const PF: u64 = 1 << 2;
    const AF: u64 = 1 << 4;
    const ZF: u64 = 1 << 6;
    const SF: u64 = 1 << 7;
    const OF: u64 = 1 << 11;
    const STATUS: u64 = CF | PF | AF | ZF | SF | OF;
    let initial = 0x2 | STATUS;

    let (value, got_flags) = exec_x86_rax_op(
        OpKind::X86Adx {
            dst: rax,
            src1: rax,
            src2: rcx,
            width: OpWidth::W64,
            kind: X86AdxKind::Adcx,
            flags: FlagUpdate::Specific(FlagSet::CF),
        },
        u64::MAX,
        0,
        initial,
    );
    assert_eq!(value, 0);
    assert_ne!(got_flags & CF, 0, "ADCX reports unsigned carry-out");
    assert_eq!(
        got_flags & !CF,
        initial & !CF,
        "ADCX preserves every non-CF status bit"
    );

    let (value, got_flags) = exec_x86_rax_op(
        OpKind::X86Adx {
            dst: rax,
            src1: rax,
            src2: rcx,
            width: OpWidth::W64,
            kind: X86AdxKind::Adox,
            flags: FlagUpdate::Specific(FlagSet::OF),
        },
        5,
        3,
        initial,
    );
    assert_eq!(value, 9);
    assert_eq!(
        got_flags & OF,
        0,
        "ADOX clears OF when the chain has no carry-out"
    );
    assert_eq!(
        got_flags & !OF,
        initial & !OF,
        "ADOX preserves every non-OF status bit"
    );

    let (value, got_flags) = exec_x86_rax_op(
        OpKind::X86Adx {
            dst: rax,
            src1: rax,
            src2: rcx,
            width: OpWidth::W32,
            kind: X86AdxKind::Adcx,
            flags: FlagUpdate::Specific(FlagSet::CF),
        },
        u64::MAX,
        0,
        initial,
    );
    assert_eq!(value, 0, "32-bit ADCX zero-extends its destination");
    assert_ne!(got_flags & CF, 0);

    let (value, got_flags) = exec_x86_rax_op(
        OpKind::X86Adx {
            dst: rax,
            src1: rax,
            src2: rax,
            width: OpWidth::W64,
            kind: X86AdxKind::Adox,
            flags: FlagUpdate::None,
        },
        7,
        0,
        initial,
    );
    assert_eq!(
        value, 15,
        "suppressed-output alias reads both sources before writing"
    );
    assert_eq!(
        got_flags, initial,
        "suppressed ADX output preserves interpreter flags"
    );
}
