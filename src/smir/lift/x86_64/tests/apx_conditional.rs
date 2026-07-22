//! Intel APX conditional compare/test lifting tests.

use super::*;
use crate::smir::lift::x86_64::*;

#[test]
fn lift_apx_ccmp_registers_use_conditional_flag_sequence_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 23: `ccmpo {dfv=cf,zf} rax, rbx` has no trailing DFV byte.
    let ccmpo = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0x9C, 0x00, 0x39, 0xD8], &mut ctx)
        .unwrap();
    assert_eq!(ccmpo.bytes_consumed, 6);
    assert_apx_conditional_flag_shape(&ccmpo, Condition::Overflow, 0x43);
    match &ccmpo.ops[4].kind {
        OpKind::Cmp {
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
        } => {
            assert_eq!(*src1, x86_gpr(0));
            assert_eq!(*src2, x86_gpr(3));
        }
        other => panic!("expected APX CCMP register compare, got {other:?}"),
    }

    // LLVM 23: `ccmpno {dfv=cf,zf} rax, rbx`.
    let ccmpno = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0x9C, 0x01, 0x39, 0xD8], &mut ctx)
        .unwrap();
    assert_eq!(ccmpno.bytes_consumed, 6);
    assert_apx_conditional_flag_shape(&ccmpno, Condition::NoOverflow, 0x43);
}
#[test]
fn lift_apx_ctest_register_and_immediate_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 23: `ctesto {dfv=sf,of} rax, rbx`.
    let ctest = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xE4, 0x40, 0x85, 0xD8], &mut ctx)
        .unwrap();
    assert_eq!(ctest.bytes_consumed, 6);
    assert_apx_conditional_flag_shape(&ctest, Condition::Overflow, 0x882);
    match &ctest.ops[4].kind {
        OpKind::Test {
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
        } => {
            assert_eq!(*src1, x86_gpr(0));
            assert_eq!(*src2, x86_gpr(3));
        }
        other => panic!("expected APX CTEST register test, got {other:?}"),
    }

    // CTESTNZ rax, 0x0f, with DFV embedded in EVEX.vvvv.
    let ctest_imm = lifter
        .lift_insn(
            0x1000,
            &[0x62, 0xF4, 0xE4, 0x45, 0xF7, 0xC0, 0x0F, 0x00, 0x00, 0x00],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(ctest_imm.bytes_consumed, 10);
    assert_apx_conditional_flag_shape(&ctest_imm, Condition::Ne, 0x882);
    match &ctest_imm.ops[4].kind {
        OpKind::Test {
            src1,
            src2: SrcOperand::Imm(0x0F),
            width: OpWidth::W64,
        } => assert_eq!(*src1, x86_gpr(0)),
        other => panic!("expected APX CTEST immediate test, got {other:?}"),
    }
}
// Regression for issue #19: an APX CTEST immediate memory form using a
// RIP-relative operand must base its effective address on the address AFTER the
// whole instruction — including the immediate bytes. The lifter previously
// computed next_pc before adding imm_size, so the RIP-relative base (and thus
// the loaded address) was `imm_size` bytes too low.
#[test]
fn issue_19_apx_ctest_imm_riprel_uses_post_immediate_rip() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // ctests {dfv=of,sf} qword ptr [rip + 0x10], 0xf0
    //   62 F4 E4 08   EVEX prefix
    //   F7            group-3 opcode (immediate form)
    //   05            ModRM mod=00 reg=000 (group 0 = CTEST) rm=101 -> RIP-relative
    //   10 00 00 00   disp32 = 0x10
    //   F0 00 00 00   imm32 = 0xF0
    let pc = 0x1000u64;
    let bytes = [
        0x62, 0xF4, 0xE4, 0x08, 0xF7, 0x05, 0x10, 0x00, 0x00, 0x00, 0xF0, 0x00, 0x00, 0x00,
    ];
    let result = lifter.lift_insn(pc, &bytes, &mut ctx).unwrap();
    assert_eq!(result.bytes_consumed, 14);

    // The RIP base must be the address one past the entire instruction
    // (pc + length, immediate included), NOT pc + length - imm_size.
    let expected_base = pc + result.bytes_consumed as u64;
    let (offset, base) = result
        .ops
        .iter()
        .find_map(|op| match &op.kind {
            OpKind::PredLoad {
                addr: Address::PcRel { offset, base, .. },
                ..
            } => Some((*offset, *base)),
            _ => None,
        })
        .expect("CTEST imm RIP-relative memory must lift to a PcRel PredLoad");
    assert_eq!(
        base,
        Some(expected_base),
        "RIP-relative base must include the immediate bytes (post-instruction RIP)",
    );
    assert_eq!(offset, 0x10, "RIP-relative displacement must be preserved");
}
#[test]
fn lift_apx_ccmp_ctest_memory_forms_use_predload_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20: `ccmpnz {dfv=of,sf} rax, [rbx]`.
    let ccmp_mem = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xE4, 0x05, 0x3B, 0x03], &mut ctx)
        .unwrap();
    assert_eq!(ccmp_mem.bytes_consumed, 6);
    let cond = assert_apx_conditional_flag_shape_with_true_ops(&ccmp_mem, Condition::Ne, 0x882, 2);
    let loaded = assert_apx_conditional_predload(&ccmp_mem, cond, 4, MemWidth::B8);
    match &ccmp_mem.ops[5].kind {
        OpKind::Cmp {
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
        } => {
            assert_eq!(*src1, x86_gpr(0));
            assert_eq!(*src2, loaded);
        }
        other => panic!("expected APX CCMP memory compare, got {other:?}"),
    }

    // LLVM 20: `ccmpae {dfv=of,sf} qword ptr [rbx], 100`.
    let ccmp_imm_mem = lifter
        .lift_insn(
            0x1000,
            &[0x62, 0xF4, 0xE4, 0x03, 0x83, 0x3B, 0x64],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(ccmp_imm_mem.bytes_consumed, 7);
    let cond =
        assert_apx_conditional_flag_shape_with_true_ops(&ccmp_imm_mem, Condition::Uge, 0x882, 2);
    let loaded = assert_apx_conditional_predload(&ccmp_imm_mem, cond, 4, MemWidth::B8);
    match &ccmp_imm_mem.ops[5].kind {
        OpKind::Cmp {
            src1,
            src2: SrcOperand::Imm(100),
            width: OpWidth::W64,
        } => assert_eq!(*src1, loaded),
        other => panic!("expected APX CCMP memory immediate compare, got {other:?}"),
    }

    // LLVM 20: `ctestb {dfv=of,sf} [rbx], rcx`.
    let ctest_mem = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xE4, 0x02, 0x85, 0x0B], &mut ctx)
        .unwrap();
    assert_eq!(ctest_mem.bytes_consumed, 6);
    let cond =
        assert_apx_conditional_flag_shape_with_true_ops(&ctest_mem, Condition::Ult, 0x882, 2);
    let loaded = assert_apx_conditional_predload(&ctest_mem, cond, 4, MemWidth::B8);
    match &ctest_mem.ops[5].kind {
        OpKind::Test {
            src1,
            src2: SrcOperand::Reg(src2),
            width: OpWidth::W64,
        } => {
            assert_eq!(*src1, loaded);
            assert_eq!(*src2, x86_gpr(1));
        }
        other => panic!("expected APX CTEST memory test, got {other:?}"),
    }

    // LLVM 20: `ctests {dfv=of,sf} qword ptr [rbx], 0xf0`.
    let ctest_imm_mem = lifter
        .lift_insn(
            0x1000,
            &[0x62, 0xF4, 0xE4, 0x08, 0xF7, 0x03, 0xF0, 0x00, 0x00, 0x00],
            &mut ctx,
        )
        .unwrap();
    assert_eq!(ctest_imm_mem.bytes_consumed, 10);
    let cond = assert_apx_conditional_flag_shape_with_true_ops(
        &ctest_imm_mem,
        Condition::Negative,
        0x882,
        2,
    );
    let loaded = assert_apx_conditional_predload(&ctest_imm_mem, cond, 4, MemWidth::B8);
    match &ctest_imm_mem.ops[5].kind {
        OpKind::Test {
            src1,
            src2: SrcOperand::Imm(0xF0),
            width: OpWidth::W64,
        } => assert_eq!(*src1, loaded),
        other => panic!("expected APX CTEST memory immediate test, got {other:?}"),
    }
}
