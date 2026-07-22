//! Intel APX conditional compare/test lifting tests.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::lift::x86_64::*;
use crate::smir::optimize::{OptLevel, optimize_function};

const CF: u64 = 1 << 0;
const PF: u64 = 1 << 2;
const AF: u64 = 1 << 4;
const ZF: u64 = 1 << 6;
const SF: u64 = 1 << 7;
const DF: u64 = 1 << 10;
const OF: u64 = 1 << 11;
const AC: u64 = 1 << 18;
const STATUS: u64 = CF | PF | AF | ZF | SF | OF;
const PRESERVED: u64 = DF | AC;

#[test]
fn lift_apx_ccmp_registers_use_conditional_flag_sequence_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 23: `ccmpo {dfv=cf,zf} rax, rbx` has no trailing DFV byte.
    let ccmpo = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0x9C, 0x00, 0x39, 0xD8], &mut ctx)
        .unwrap();
    assert_eq!(ccmpo.bytes_consumed, 6);
    assert_apx_conditional_flag_shape(&ccmpo, Condition::Overflow, 0x47);
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
    assert_apx_conditional_flag_shape(&ccmpno, Condition::NoOverflow, 0x47);
}
#[test]
fn lift_apx_ctest_register_and_immediate_like_llvm() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 23: `ctesto {dfv=sf,of} rax, rbx`.
    let ctest = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xE4, 0x00, 0x85, 0xD8], &mut ctx)
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
            &[0x62, 0xF4, 0xE4, 0x05, 0xF7, 0xC0, 0x0F, 0x00, 0x00, 0x00],
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
            OpKind::Load {
                addr: Address::PcRel { offset, base, .. },
                ..
            } => Some((*offset, *base)),
            _ => None,
        })
        .expect("CTEST imm RIP-relative memory must lift to a PcRel Load");
    assert_eq!(
        base,
        Some(expected_base),
        "RIP-relative base must include the immediate bytes (post-instruction RIP)",
    );
    assert_eq!(offset, 0x10, "RIP-relative displacement must be preserved");
}
#[test]
fn lift_apx_ccmp_ctest_memory_forms_use_unconditional_loads() {
    let mut lifter = X86_64Lifter::strict();
    let mut ctx = LiftContext::new(SourceArch::X86_64);

    // LLVM 20: `ccmpnz {dfv=of,sf} rax, [rbx]`.
    let ccmp_mem = lifter
        .lift_insn(0x1000, &[0x62, 0xF4, 0xE4, 0x05, 0x3B, 0x03], &mut ctx)
        .unwrap();
    assert_eq!(ccmp_mem.bytes_consumed, 6);
    assert_apx_conditional_flag_shape_with_true_ops(&ccmp_mem, Condition::Ne, 0x882, 1);
    let loaded = assert_apx_conditional_load(&ccmp_mem, 0, MemWidth::B8);
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
    assert_apx_conditional_flag_shape_with_true_ops(&ccmp_imm_mem, Condition::Uge, 0x882, 1);
    let loaded = assert_apx_conditional_load(&ccmp_imm_mem, 0, MemWidth::B8);
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
    assert_apx_conditional_flag_shape_with_true_ops(&ctest_mem, Condition::Ult, 0x882, 1);
    let loaded = assert_apx_conditional_load(&ctest_mem, 0, MemWidth::B8);
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
    assert_apx_conditional_flag_shape_with_true_ops(&ctest_imm_mem, Condition::Negative, 0x882, 1);
    let loaded = assert_apx_conditional_load(&ctest_imm_mem, 0, MemWidth::B8);
    match &ctest_imm_mem.ops[5].kind {
        OpKind::Test {
            src1,
            src2: SrcOperand::Imm(0xF0),
            width: OpWidth::W64,
        } => assert_eq!(*src1, loaded),
        other => panic!("expected APX CTEST memory immediate test, got {other:?}"),
    }
}

fn conditional_prefix(dfv: u8, w: bool, pp: u8, scc: u8, u: bool) -> [u8; 4] {
    let p1 = (if w { 0x80 } else { 0 }) | ((dfv & 0x0F) << 3) | (if u { 0x04 } else { 0 }) | pp;
    [0x62, 0xF4, p1, scc & 0x0F]
}

fn assert_conditional_ud(bytes: &[u8], expected_len: usize) {
    let result = lift_single(bytes).unwrap_or_else(|error| {
        panic!(
            "reserved APX CCMP/CTEST encoding must strictly lift to #UD: {bytes:02X?}: {error:?}"
        )
    });
    assert_invalid_opcode_trap(&result, expected_len);
}

#[test]
fn apx_map4_opcode_82_is_ud_at_the_opcode_frontier() {
    let mut bytes = conditional_prefix(0, false, 0, 0, true).to_vec();
    bytes.push(0x82);
    assert_conditional_ud(&bytes, 5);

    for modrm in 0..=u8::MAX {
        let mut with_modrm = bytes.clone();
        with_modrm.push(modrm);
        assert_conditional_ud(&with_modrm, 5);
    }
}

#[test]
fn fixed_ccmp_ctest_reserved_payload_and_pp_trap_at_opcode() {
    for opcode in [0x38, 0x3A, 0x84] {
        for pp in 1..=3 {
            let mut bytes = conditional_prefix(0, true, pp, 0, true).to_vec();
            bytes.push(opcode);
            assert_conditional_ud(&bytes, 5);
        }
    }
    for opcode in [0x39, 0x3B, 0x85] {
        for pp in 2..=3 {
            let mut bytes = conditional_prefix(0, true, pp, 0, true).to_vec();
            bytes.push(opcode);
            assert_conditional_ud(&bytes, 5);
        }
    }

    for reserved_nibble in 1..=0x0F {
        for opcode in [0x38, 0x39, 0x3A, 0x3B, 0x84, 0x85] {
            let mut bytes = conditional_prefix(0, true, 0, 0, true).to_vec();
            bytes[3] |= reserved_nibble << 4;
            bytes.push(opcode);
            assert_conditional_ud(&bytes, 5);
        }
    }
}

#[test]
fn conditional_register_u_zero_traps_at_modrm() {
    for opcode in [0x38, 0x39, 0x3A, 0x3B, 0x84, 0x85] {
        let mut bytes = conditional_prefix(0, opcode & 1 != 0, 0, 0, false).to_vec();
        bytes.extend_from_slice(&[opcode, 0xC0]);
        assert_conditional_ud(&bytes, 6);
    }
}

#[test]
fn grouped_ccmp_ctest_reserved_fields_trap_at_modrm_before_operands() {
    let cases = [
        (0x80, 7),
        (0x81, 7),
        (0x83, 7),
        (0xF6, 0),
        (0xF6, 1),
        (0xF7, 0),
        (0xF7, 1),
    ];
    for reserved_nibble in 1..=0x0F {
        for (opcode, group) in cases {
            // mod=00,r/m=100 would require a SIB; the reserved conditional form
            // is already known from ModR/M.reg and must not demand it or an
            // immediate.
            let mut bytes =
                conditional_prefix(0, opcode != 0x80 && opcode != 0xF6, 0, 0, true).to_vec();
            bytes[3] |= reserved_nibble << 4;
            bytes.extend_from_slice(&[opcode, group << 3 | 0x04]);
            assert_conditional_ud(&bytes, 6);
        }
    }

    for (opcode, group) in cases {
        let mut bytes =
            conditional_prefix(0, opcode != 0x80 && opcode != 0xF6, 0, 0, false).to_vec();
        bytes.extend_from_slice(&[opcode, 0xC0 | group << 3]);
        assert_conditional_ud(&bytes, 6);
    }

    for (opcode, group) in cases {
        let first_invalid_pp = if matches!(opcode, 0x80 | 0xF6) { 1 } else { 2 };
        for invalid_pp in first_invalid_pp..=3 {
            let mut bytes = conditional_prefix(0, true, invalid_pp, 0, true).to_vec();
            bytes.push(opcode);
            assert!(matches!(
                lift_single(&bytes),
                Err(LiftError::Incomplete {
                    have: 5,
                    need: 6,
                    ..
                })
            ));
            bytes.push(0xC0 | group << 3);
            assert_conditional_ud(&bytes, 6);
        }
    }
}

#[test]
fn conditional_decode_frontiers_distinguish_opcode_modrm_and_immediate() {
    for opcode in [
        0x38, 0x39, 0x3A, 0x3B, 0x80, 0x81, 0x83, 0x84, 0x85, 0xF6, 0xF7,
    ] {
        let mut bytes = conditional_prefix(0, true, 0, 0x0A, true).to_vec();
        bytes.push(opcode);
        assert!(matches!(
            lift_single(&bytes),
            Err(LiftError::Incomplete {
                have: 5,
                need: 6,
                ..
            })
        ));
    }

    for (opcode, group, pp, relative_need) in [
        (0x80, 7, 0, 2),
        (0x81, 7, 0, 5),
        (0x81, 7, 1, 3),
        (0x83, 7, 0, 2),
        (0xF6, 0, 0, 2),
        (0xF6, 1, 0, 2),
        (0xF7, 0, 0, 5),
        (0xF7, 1, 1, 3),
    ] {
        let w = pp == 0 && !matches!(opcode, 0x80 | 0xF6);
        let mut bytes = conditional_prefix(0, w, pp, 0x0A, true).to_vec();
        bytes.extend_from_slice(&[opcode, 0xC0 | group << 3]);
        assert!(matches!(
            lift_single(&bytes),
            Err(LiftError::Incomplete {
                have: 1,
                need,
                ..
            }) if need == relative_need
        ));
    }
}

#[test]
fn conditional_memory_u_zero_selects_egpr_index_and_allows_prefixes() {
    let mut bytes = vec![0x64, 0x67];
    bytes.extend_from_slice(&conditional_prefix(0, true, 0, 0x0A, false));
    bytes.extend_from_slice(&[0xF7, 0x0C, 0x03, 0xFF, 0xFF, 0xFF, 0xFF]);
    let result = lift_single(&bytes).expect("CTESTT FS:[EBX+R16D],-1");
    assert_eq!(result.bytes_consumed, bytes.len());
    let OpKind::Load {
        addr: Address::X86Addr32(inner),
        width: MemWidth::B8,
        sign: SignExtend::Zero,
        ..
    } = &result.ops[0].kind
    else {
        panic!(
            "expected addr32 APX conditional load, got {:?}",
            result.ops[0]
        );
    };
    assert!(matches!(
        inner.as_ref(),
        Address::SegmentRel {
            segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
            base: Some(base),
            index: Some(index),
            scale: 1,
            disp: 0,
        } if *base == x86_gpr(3) && *index == x86_gpr(16)
    ));
    assert!(result.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Test {
            src2: SrcOperand::Imm(-1),
            width: OpWidth::W64,
            ..
        }
    )));
}

#[test]
fn conditional_egpr_registers_and_sign_extended_imm32_are_exact() {
    let mut egpr = conditional_prefix(0, true, 0, 0x0A, true).to_vec();
    egpr[1] = 0xEC; // R4=1 and B4=1: ModR/M.reg/rm address R16-R31.
    egpr.extend_from_slice(&[0x39, 0xD1]);
    let result = lift_single(&egpr).expect("CCMPT R17,R18");
    assert!(matches!(
        result.ops.iter().find_map(|op| match &op.kind {
            OpKind::Cmp { src1, src2, width } => Some((*src1, src2.clone(), *width)),
            _ => None,
        }),
        Some((src1, SrcOperand::Reg(src2), OpWidth::W64))
            if src1 == x86_gpr(17) && src2 == x86_gpr(18)
    ));

    let mut immediate = conditional_prefix(0, true, 0, 0x0A, true).to_vec();
    immediate.extend_from_slice(&[0x81, 0xF8, 0xFF, 0xFF, 0xFF, 0xFF]);
    let (exit, context) = execute_conditional(&immediate, OptLevel::O2, 0x2, u64::MAX, 0);
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.flags.materialized.to_rflags() & STATUS, PF | ZF);
}

#[test]
fn conditional_forms_reject_forbidden_legacy_prefixes() {
    for prefix in [0x66, 0xF2, 0xF3, 0xF0, 0x48] {
        let mut bytes = vec![prefix];
        bytes.extend_from_slice(&conditional_prefix(0, true, 0, 0x0A, true));
        bytes.extend_from_slice(&[0x39, 0xD8]);
        assert!(matches!(
            lift_single(&bytes),
            Err(LiftError::InvalidEncoding { .. })
        ));
    }
}

#[test]
fn ctest_immediate_group_one_lifts_identically_to_group_zero() {
    for group in [0, 1] {
        let mut bytes = conditional_prefix(0, true, 0, 0x0A, true).to_vec();
        bytes.extend_from_slice(&[0xF7, 0xC0 | group << 3, 0x0F, 0, 0, 0]);
        let result = lift_single(&bytes)
            .unwrap_or_else(|error| panic!("legal APX CTEST F7 /{group} must lift: {error:?}"));
        assert_eq!(result.bytes_consumed, 10);
        assert!(
            result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::Test { .. }))
        );
    }
}

#[test]
fn scc_true_false_and_dfv_cf_have_apx_specific_shapes() {
    assert_eq!(X86_64Lifter::apx_ccmp_default_rflags(0), 0x02);
    assert_eq!(
        X86_64Lifter::apx_ccmp_default_rflags(1),
        0x07,
        "DFV.CF also supplies PF"
    );

    let mut always = conditional_prefix(1, true, 0, 0x0A, true).to_vec();
    always.extend_from_slice(&[0x39, 0xD8]);
    let always = lift_single(&always).unwrap();
    assert!(always.ops.iter().any(|op| {
        matches!(
            op.kind,
            OpKind::SetCC {
                cond: Condition::Always,
                ..
            }
        ) || matches!(
            op.kind,
            OpKind::Mov {
                src: SrcOperand::Imm(1),
                ..
            }
        )
    }));

    let mut never = conditional_prefix(1, true, 0, 0x0B, true).to_vec();
    never.extend_from_slice(&[0x39, 0xD8]);
    let never = lift_single(&never).unwrap();
    assert!(never.ops.iter().any(|op| {
        matches!(
            op.kind,
            OpKind::Mov {
                src: SrcOperand::Imm(0),
                ..
            }
        )
    }));
}

#[test]
fn conditional_memory_operands_use_unconditional_loads() {
    for bytes in [
        &[0x62, 0xF4, 0xE4, 0x0B, 0x3B, 0x03][..],
        &[0x62, 0xF4, 0xE4, 0x0B, 0x85, 0x03][..],
        &[0x62, 0xF4, 0xE4, 0x0B, 0x83, 0x3B, 0x01][..],
        &[0x62, 0xF4, 0xE4, 0x0B, 0xF7, 0x03, 0x01, 0, 0, 0][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(
            result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::Load { .. })),
            "{bytes:02X?}"
        );
        assert!(
            !result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. })),
            "{bytes:02X?}"
        );
    }
}

fn conditional_block(bytes: &[u8], level: OptLevel) -> SmirBlock {
    let lifted = lift_single(bytes).expect("strict APX conditional lift");
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = lifted.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut function = SmirFunction::new(FunctionId(0), block.id, block.guest_pc);
    function.add_block(block);
    optimize_function(&mut function, level);
    function.entry_block().unwrap().clone()
}

fn default_status(dfv: u8) -> u64 {
    (if dfv & 0x1 != 0 { CF | PF } else { 0 })
        | (if dfv & 0x2 != 0 { ZF } else { 0 })
        | (if dfv & 0x4 != 0 { SF } else { 0 })
        | (if dfv & 0x8 != 0 { OF } else { 0 })
}

fn scc_holds(scc: u8, rflags: u64) -> bool {
    let cf = rflags & CF != 0;
    let zf = rflags & ZF != 0;
    let sf = rflags & SF != 0;
    let of = rflags & OF != 0;
    match scc {
        0x0 => of,
        0x1 => !of,
        0x2 => cf,
        0x3 => !cf,
        0x4 => zf,
        0x5 => !zf,
        0x6 => cf || zf,
        0x7 => !cf && !zf,
        0x8 => sf,
        0x9 => !sf,
        0xA => true,
        0xB => false,
        0xC => sf != of,
        0xD => sf == of,
        0xE => zf || sf != of,
        0xF => !zf && sf == of,
        _ => unreachable!(),
    }
}

fn execute_conditional(
    bytes: &[u8],
    level: OptLevel,
    rflags: u64,
    rax: u64,
    rbx: u64,
) -> (BlockResult, SmirContext) {
    let mut context = SmirContext::new_x86_64();
    context.flags.materialized = MaterializedFlags::from_rflags(rflags);
    context.write_vreg(x86_gpr(0), rax);
    context.write_vreg(x86_gpr(3), rbx);
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        &conditional_block(bytes, level),
    );
    (result, context)
}

#[test]
fn apx_conditional_interpreter_and_optimizer_cover_every_scc() {
    let patterns = [0, STATUS, CF | ZF, SF];
    let dfv = 0x0D;
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for scc in 0..=0x0F {
            let mut bytes = conditional_prefix(dfv, true, 0, scc, true).to_vec();
            bytes.extend_from_slice(&[0x39, 0xD8]);
            for status in patterns {
                let initial = 0x2 | PRESERVED | status;
                let (result, context) = execute_conditional(&bytes, level, initial, 5, 5);
                assert!(
                    matches!(result, BlockResult::Exit(ExitReason::Halt)),
                    "level={level:?} SCC={scc:X} initial={initial:#x}"
                );
                let selected = if scc_holds(scc, initial) {
                    PF | ZF
                } else {
                    default_status(dfv)
                };
                assert_eq!(
                    context.flags.materialized.to_rflags() & (STATUS | PRESERVED | 0x2),
                    0x2 | PRESERVED | selected,
                    "level={level:?} SCC={scc:X} initial={initial:#x}"
                );
            }
        }
    }
}

#[test]
fn apx_conditional_interpreter_and_optimizer_cover_every_dfv() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        for dfv in 0..=0x0F {
            for tail in [&[0x39, 0xD8][..], &[0xF7, 0xC8, 0xFF, 0x00, 0x00, 0x00][..]] {
                let mut bytes = conditional_prefix(dfv, true, 0, 0x0B, true).to_vec();
                bytes.extend_from_slice(tail);
                let initial = 0x2 | PRESERVED | STATUS;
                let (result, context) = execute_conditional(&bytes, level, initial, 0xFFFF, 1);
                assert!(
                    matches!(result, BlockResult::Exit(ExitReason::Halt)),
                    "level={level:?} DFV={dfv:X} tail={tail:02X?}"
                );
                assert_eq!(
                    context.flags.materialized.to_rflags() & (STATUS | PRESERVED | 0x2),
                    0x2 | PRESERVED | default_status(dfv),
                    "level={level:?} DFV={dfv:X} tail={tail:02X?}"
                );
            }
        }
    }
}

#[test]
fn false_scc_memory_faults_precede_any_interpreter_flag_commit() {
    let mut bytes = conditional_prefix(0x0F, true, 0, 0x0B, true).to_vec();
    bytes.extend_from_slice(&[0x39, 0x03]);
    let initial = 0x2 | PRESERVED | STATUS;
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut context = SmirContext::new_x86_64();
        context.flags.materialized = MaterializedFlags::from_rflags(initial);
        context.write_vreg(x86_gpr(0), 1);
        context.write_vreg(x86_gpr(3), 0x1000);
        let result = SmirInterpreter::new().execute_block(
            &mut context,
            &mut FlatMemory::new(8),
            &conditional_block(&bytes, level),
        );
        assert!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::MemoryFault {
                    addr: 0x1000,
                    write: false
                })
            ),
            "level={level:?} result={result:?}"
        );
        assert_eq!(
            context.flags.materialized.to_rflags(),
            initial,
            "level={level:?}"
        );
        assert!(context.flags.lazy.is_none(), "level={level:?}");
    }
}
