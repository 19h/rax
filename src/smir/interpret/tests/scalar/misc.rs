//! scalar::misc tests

use super::*;
use crate::smir::interpret::tests::*;
use crate::smir::interpret::*;

#[test]
fn lifted_x86_xchg_preserves_word_uppers_and_zero_extends_dword_self_exchange() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();

    ctx.write_vreg(rax, 0x1122_3344_5566_1234);
    ctx.write_vreg(r8, 0xAABB_CCDD_EEFF_7788);
    execute_lifted_x86(&[0x66, 0x44, 0x87, 0xC0], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_7788);
    assert_eq!(ctx.read_vreg(r8), 0xAABB_CCDD_EEFF_1234);

    ctx.write_vreg(rax, 0xAABB_CCDD_1234_5678);
    execute_lifted_x86(&[0x87, 0xC0], &mut ctx, &mut memory);
    assert_eq!(
        ctx.read_vreg(rax),
        0x1234_5678,
        "XCHG EAX,EAX is a 32-bit write even though 90 is not"
    );
}
#[test]
fn lifted_cbw_cwde_cdqe_execute_with_x86_partial_write_semantics() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));

    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();
    ctx.write_vreg(rax, 0x1122_3344_5566_7780);
    assert!(matches!(
        execute_lifted_x86(&[0x66, 0x98], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_FF80, "CBW");

    ctx.write_vreg(rax, 0x1122_3344_0000_8001);
    execute_lifted_x86(&[0x98], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), 0x0000_0000_FFFF_8001, "CWDE");

    ctx.write_vreg(rax, 0x0000_0000_8000_0001);
    execute_lifted_x86(&[0x48, 0x98], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), 0xFFFF_FFFF_8000_0001, "CDQE");
}
#[test]
fn lifted_cwd_cdq_cqo_execute_partial_writes_and_preserve_flags() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
    const STATUS: u64 = 0x08D5;

    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();
    ctx.flags.materialized = MaterializedFlags::from_rflags(0x08D7);
    ctx.flags.lazy = None;

    ctx.write_vreg(rax, 0x1122_3344_5566_8001);
    ctx.write_vreg(rdx, 0xAABB_CCDD_EEFF_1234);
    execute_lifted_x86(&[0x66, 0x99], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_8001, "CWD RAX");
    assert_eq!(ctx.read_vreg(rdx), 0xAABB_CCDD_EEFF_FFFF, "CWD RDX");

    ctx.write_vreg(rax, 0x1122_3344_8000_0001);
    ctx.write_vreg(rdx, u64::MAX);
    execute_lifted_x86(&[0x99], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_8000_0001, "CDQ RAX");
    assert_eq!(ctx.read_vreg(rdx), 0x0000_0000_FFFF_FFFF, "CDQ RDX");

    ctx.write_vreg(rax, 0x8000_0000_0000_0001);
    ctx.write_vreg(rdx, 0);
    execute_lifted_x86(&[0x48, 0x99], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), 0x8000_0000_0000_0001, "CQO RAX");
    assert_eq!(ctx.read_vreg(rdx), u64::MAX, "CQO RDX");

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags() & STATUS, 0x08D5);
}
#[test]
fn lifted_popcnt_tzcnt_lzcnt_execute_results_aliases_and_flags() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();

    ctx.write_vreg(rbx, 0);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;
    execute_lifted_x86(&[0xF3, 0x0F, 0xB8, 0xC3], &mut ctx, &mut memory);
    ctx.flags.materialize_all();
    assert_eq!(ctx.read_vreg(rax), 0, "POPCNT zero result");
    assert_eq!(
        ctx.flags.materialized.to_rflags(),
        0x442,
        "POPCNT must preserve DF, set ZF, and clear CF/PF/AF/SF/OF"
    );

    ctx.write_vreg(rax, 0x0000_0000_0000_0100);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;
    execute_lifted_x86(&[0xF3, 0x0F, 0xBC, 0xC0], &mut ctx, &mut memory);
    ctx.flags.materialize_all();
    assert_eq!(ctx.read_vreg(rax), 8, "TZCNT source/destination alias");
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xC96);

    ctx.write_vreg(rax, 0);
    execute_lifted_x86(&[0xF3, 0x0F, 0xBC, 0xC0], &mut ctx, &mut memory);
    ctx.flags.materialize_all();
    assert_eq!(ctx.read_vreg(rax), 32, "TZCNT zero input");
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xC97, "TZCNT CF");

    ctx.write_vreg(rax, 0x8000_0000);
    execute_lifted_x86(&[0xF3, 0x0F, 0xBD, 0xC0], &mut ctx, &mut memory);
    ctx.flags.materialize_all();
    assert_eq!(ctx.read_vreg(rax), 0, "LZCNT high bit");
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD6, "LZCNT ZF");
}
#[test]
fn x86_count_ir_honors_full_partial_and_suppressed_flag_updates() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));

    let (result, flags) = exec_x86_rax_op(
        OpKind::X86Count {
            dst: rax,
            src: rcx,
            width: OpWidth::W64,
            kind: X86CountKind::Popcnt,
            flags: FlagUpdate::All,
        },
        0,
        0xF0,
        0xCD7,
    );
    assert_eq!(result, 4);
    assert_eq!(flags, 0x402, "POPCNT clears every arithmetic status flag");

    let (result, flags) = exec_x86_rax_op(
        OpKind::X86Count {
            dst: rax,
            src: rcx,
            width: OpWidth::W32,
            kind: X86CountKind::Tzcnt,
            flags: FlagUpdate::Specific(FlagSet::CF.union(FlagSet::ZF)),
        },
        u64::MAX,
        0,
        0xCD7,
    );
    assert_eq!(result, 32);
    assert_eq!(
        flags, 0xC97,
        "TZCNT replaces CF/ZF and retains undefined flags"
    );

    let (result, flags) = exec_x86_rax_op(
        OpKind::X86Count {
            dst: rax,
            src: rcx,
            width: OpWidth::W64,
            kind: X86CountKind::Lzcnt,
            flags: FlagUpdate::Specific(FlagSet::ZF),
        },
        0,
        1 << 63,
        0xC97,
    );
    assert_eq!(result, 0);
    assert_eq!(flags, 0xCD7, "partial update changes ZF only");

    let (result, flags) = exec_x86_rax_op(
        OpKind::X86Count {
            dst: rax,
            src: rcx,
            width: OpWidth::W16,
            kind: X86CountKind::Popcnt,
            flags: FlagUpdate::None,
        },
        0x1122_3344_5566_7788,
        0,
        0xCD7,
    );
    assert_eq!(result, 0x1122_3344_5566_0000);
    assert_eq!(flags, 0xCD7, "APX NF suppresses every flag update");
}
#[test]
fn lifted_bit_test_updates_execute_immediate_lock_and_fault_ordering() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let mut memory = FlatMemory::new(0x4000);
    let mut ctx = SmirContext::new_x86_64();

    ctx.write_vreg(rax, 0x2200);
    memory.write(0x2200, &0u32.to_le_bytes()).unwrap();
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;
    execute_lifted_x86(&[0x0F, 0xBA, 0x28, 37], &mut ctx, &mut memory); // BTS [RAX],37 => bit5
    let mut dword = [0u8; 4];
    memory.read(0x2200, &mut dword).unwrap();
    assert_eq!(u32::from_le_bytes(dword), 1 << 5);
    ctx.flags.materialize_all();
    assert_eq!(
        ctx.flags.materialized.to_rflags(),
        0xCD6,
        "old bit was zero"
    );

    ctx.write_vreg(rcx, 3);
    memory.write(0x2200, &0u64.to_le_bytes()).unwrap();
    execute_lifted_x86(&[0xF0, 0x48, 0x0F, 0xBB, 0x08], &mut ctx, &mut memory); // LOCK BTC [RAX],RCX
    let mut qword = [0u8; 8];
    memory.read(0x2200, &mut qword).unwrap();
    assert_eq!(u64::from_le_bytes(qword), 1 << 3);

    let mut inner = FlatMemory::new(0x4000);
    inner.write(0x2200, &1u32.to_le_bytes()).unwrap();
    let mut read_only = StoreFaultMemory {
        inner,
        stores_before_fault: 0,
    };
    ctx.write_vreg(rax, 0x2200);
    ctx.write_vreg(rcx, 0);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD6);
    ctx.flags.lazy = None;
    let exit = execute_lifted_x86(&[0x0F, 0xB3, 0x08], &mut ctx, &mut read_only);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { write: true, .. })
    ));
    ctx.flags.materialize_all();
    assert_eq!(
        ctx.flags.materialized.to_rflags(),
        0xCD6,
        "faulting BTR store must not commit CF"
    );
}
#[test]
fn lifted_lahf_sahf_execute_status_flag_transfers() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();
    ctx.write_vreg(rax, 0x1122_3344_5566_AA88);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0x8D5);
    ctx.flags.lazy = None;
    execute_lifted_x86(&[0x9F], &mut ctx, &mut memory);
    assert_eq!(
        ctx.read_vreg(rax),
        0x1122_3344_5566_D788,
        "LAHF must replace AH only and force bit 1"
    );

    ctx.write_vreg(rax, 0x0000_0000_0000_D500);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xC02); // OF|DF
    ctx.flags.lazy = None;
    execute_lifted_x86(&[0x9E], &mut ctx, &mut memory);
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7, "SAHF");
}
#[test]
fn lifted_group4_legacy_high_bytes_and_rex_low_bytes_are_distinct() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();

    ctx.write_vreg(rax, 0x1122_3344_5566_7FFF);
    ctx.flags.materialized = MaterializedFlags::from_rflags(1); // CF=1 must survive INC.
    ctx.flags.lazy = None;
    execute_lifted_x86(&[0xFE, 0xC4], &mut ctx, &mut memory); // INC AH
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_80FF);
    ctx.flags.materialize_all();
    assert!(ctx.flags.materialized.cf);
    assert!(ctx.flags.materialized.of);
    assert!(ctx.flags.materialized.sf);
    assert!(ctx.flags.materialized.af);
    assert!(!ctx.flags.materialized.zf);
    assert!(!ctx.flags.materialized.pf);

    ctx.write_vreg(rbx, 0xAABB_CCDD_EEFF_0011);
    execute_lifted_x86(&[0xFE, 0xCF], &mut ctx, &mut memory); // DEC BH
    assert_eq!(ctx.read_vreg(rbx), 0xAABB_CCDD_EEFF_FF11);
    ctx.flags.materialize_all();
    assert!(ctx.flags.materialized.cf);
    assert!(!ctx.flags.materialized.of);
    assert!(ctx.flags.materialized.sf);
    assert!(ctx.flags.materialized.af);
    assert!(!ctx.flags.materialized.zf);
    assert!(ctx.flags.materialized.pf);

    ctx.write_vreg(rsp, 0x1234_5678_9ABC_DE7F);
    execute_lifted_x86(&[0x40, 0xFE, 0xC4], &mut ctx, &mut memory); // INC SPL
    assert_eq!(ctx.read_vreg(rsp), 0x1234_5678_9ABC_DE80);
}
#[test]
fn lifted_mov_forms_read_and_write_legacy_high_bytes() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();

    ctx.write_vreg(rax, 0x1122_3344_5566_7788);
    execute_lifted_x86(&[0xB4, 0x5A], &mut ctx, &mut memory); // MOV AH,5Ah
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_5A88);
    execute_lifted_x86(&[0xC6, 0xC4, 0x33], &mut ctx, &mut memory); // MOV AH,33h
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_3388);

    ctx.write_vreg(rax, 0x1122_3344_5566_1234);
    execute_lifted_x86(&[0x88, 0xE0], &mut ctx, &mut memory); // MOV AL,AH
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_1212);
    ctx.write_vreg(rax, 0x1122_3344_5566_1234);
    execute_lifted_x86(&[0x88, 0xC4], &mut ctx, &mut memory); // MOV AH,AL
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_3434);

    ctx.write_vreg(rax, 0x1122_3344_5566_A5CC);
    ctx.write_vreg(rbx, 0x200);
    execute_lifted_x86(&[0x88, 0x23], &mut ctx, &mut memory); // MOV [RBX],AH
    let mut byte = [0u8; 1];
    memory.read(0x200, &mut byte).unwrap();
    assert_eq!(byte[0], 0xA5);
    memory.write(0x200, &[0x6D]).unwrap();
    execute_lifted_x86(&[0x8A, 0x23], &mut ctx, &mut memory); // MOV AH,[RBX]
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_6DCC);

    ctx.write_vreg(rsp, 0x1234_5678_9ABC_DE7F);
    execute_lifted_x86(&[0x40, 0xB4, 0x5A], &mut ctx, &mut memory); // MOV SPL,5Ah
    assert_eq!(ctx.read_vreg(rsp), 0x1234_5678_9ABC_DE5A);
}
#[test]
fn lifted_test_reads_legacy_high_bytes_without_modifying_registers() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();

    ctx.write_vreg(rax, 0x1122_3344_5566_F0AA);
    ctx.write_vreg(rbx, 0xAABB_CCDD_EEFF_0F55);
    execute_lifted_x86(&[0x84, 0xFC], &mut ctx, &mut memory); // TEST AH,BH
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_F0AA);
    assert_eq!(ctx.read_vreg(rbx), 0xAABB_CCDD_EEFF_0F55);
    ctx.flags.materialize_all();
    assert!(ctx.flags.materialized.zf);
    assert!(!ctx.flags.materialized.sf);
    assert!(!ctx.flags.materialized.cf);
    assert!(!ctx.flags.materialized.of);

    memory.write(0x200, &[0x80]).unwrap();
    ctx.write_vreg(rbx, 0x200);
    execute_lifted_x86(&[0x84, 0x23], &mut ctx, &mut memory); // TEST [RBX],AH
    ctx.flags.materialize_all();
    assert!(!ctx.flags.materialized.zf);
    assert!(ctx.flags.materialized.sf);
}
#[test]
fn lifted_binary_alu_reads_and_writes_legacy_high_bytes() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();

    ctx.write_vreg(rax, 0x1122_3344_5566_01AA);
    ctx.write_vreg(rbx, 0xAABB_CCDD_EEFF_0255);
    execute_lifted_x86(&[0x00, 0xFC], &mut ctx, &mut memory); // ADD AH,BH
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_03AA);
    assert_eq!(ctx.read_vreg(rbx), 0xAABB_CCDD_EEFF_0255);

    ctx.write_vreg(rax, 0x1122_3344_5566_02AA);
    ctx.write_vreg(rbx, 0xAABB_CCDD_EEFF_0555);
    execute_lifted_x86(&[0x28, 0xE7], &mut ctx, &mut memory); // SUB BH,AH
    assert_eq!(ctx.read_vreg(rbx), 0xAABB_CCDD_EEFF_0355);

    ctx.write_vreg(rax, 0x1122_3344_5566_F0AA);
    ctx.write_vreg(rbx, 0xAABB_CCDD_EEFF_0F55);
    execute_lifted_x86(&[0x30, 0xFC], &mut ctx, &mut memory); // XOR AH,BH
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_FFAA);

    ctx.write_vreg(rax, 0x1122_3344_5566_80AA);
    ctx.write_vreg(rbx, 0xAABB_CCDD_EEFF_8055);
    execute_lifted_x86(&[0x38, 0xFC], &mut ctx, &mut memory); // CMP AH,BH
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_80AA);
    assert_eq!(ctx.read_vreg(rbx), 0xAABB_CCDD_EEFF_8055);
    ctx.flags.materialize_all();
    assert!(ctx.flags.materialized.zf);

    memory.write(0x200, &[2]).unwrap();
    ctx.write_vreg(rax, 0x1122_3344_5566_01AA);
    ctx.write_vreg(rbx, 0x200);
    execute_lifted_x86(&[0x02, 0x23], &mut ctx, &mut memory); // ADD AH,[RBX]
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_03AA);
    execute_lifted_x86(&[0x00, 0x23], &mut ctx, &mut memory); // ADD [RBX],AH
    let mut byte = [0u8; 1];
    memory.read(0x200, &mut byte).unwrap();
    assert_eq!(byte[0], 5);

    ctx.write_vreg(rax, 1);
    ctx.write_vreg(rsp, 0x1234_5678_9ABC_DE7F);
    execute_lifted_x86(&[0x40, 0x00, 0xC4], &mut ctx, &mut memory); // ADD SPL,AL
    assert_eq!(ctx.read_vreg(rsp), 0x1234_5678_9ABC_DE80);

    ctx.write_vreg(rax, 0xFFFF_FFFF_1234_56AA);
    execute_lifted_x86(&[0x34, 0xFF], &mut ctx, &mut memory); // XOR AL,FFh
    assert_eq!(ctx.read_vreg(rax), 0xFFFF_FFFF_1234_5655);
    execute_lifted_x86(&[0x35, 0x55, 0x56, 0x34, 0x12], &mut ctx, &mut memory); // XOR EAX,12345655h
    assert_eq!(ctx.read_vreg(rax), 0);
}
#[test]
fn lifted_immediate_shift_and_group3_forms_handle_legacy_high_bytes() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();

    ctx.write_vreg(rax, 0x1122_3344_5566_01AA);
    execute_lifted_x86(&[0x80, 0xC4, 1], &mut ctx, &mut memory); // ADD AH,1
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_02AA);
    execute_lifted_x86(&[0x80, 0xFC, 2], &mut ctx, &mut memory); // CMP AH,2
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_02AA);
    ctx.flags.materialize_all();
    assert!(ctx.flags.materialized.zf);

    execute_lifted_x86(&[0xD0, 0xE4], &mut ctx, &mut memory); // SHL AH,1
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_04AA);
    ctx.write_vreg(rbx, 0xAABB_CCDD_EEFF_1055);
    execute_lifted_x86(&[0xC0, 0xCF, 4], &mut ctx, &mut memory); // ROR BH,4
    assert_eq!(ctx.read_vreg(rbx), 0xAABB_CCDD_EEFF_0155);

    ctx.write_vreg(rcx, 0x1122_3344_5566_0101);
    execute_lifted_x86(&[0xD2, 0xE5], &mut ctx, &mut memory); // SHL CH,CL
    assert_eq!(ctx.read_vreg(rcx), 0x1122_3344_5566_0201);

    ctx.write_vreg(rax, 0x1122_3344_5566_0FAA);
    execute_lifted_x86(&[0xF6, 0xD4], &mut ctx, &mut memory); // NOT AH
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_F0AA);
    ctx.write_vreg(rbx, 0xAABB_CCDD_EEFF_0155);
    execute_lifted_x86(&[0xF6, 0xDF], &mut ctx, &mut memory); // NEG BH
    assert_eq!(ctx.read_vreg(rbx), 0xAABB_CCDD_EEFF_FF55);

    execute_lifted_x86(&[0xF6, 0xC4, 0xF0], &mut ctx, &mut memory); // TEST AH,F0h
    ctx.flags.materialize_all();
    assert!(!ctx.flags.materialized.zf);
    ctx.write_vreg(rax, 0x1122_3344_5566_0304);
    execute_lifted_x86(&[0xF6, 0xE4], &mut ctx, &mut memory); // MUL AH
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_000C);

    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
    ctx.write_vreg(rax, 0x1122_3344_5566_0120);
    ctx.write_vreg(rbx, 0xAABB_CCDD_EEFF_0010);
    ctx.write_vreg(rdx, 0x8877_6655_4433_2211);
    execute_lifted_x86(&[0xF6, 0xF3], &mut ctx, &mut memory); // DIV BL
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_0012);
    assert_eq!(ctx.read_vreg(rdx), 0x8877_6655_4433_2211);

    ctx.write_vreg(rax, 0x1122_3344_5566_FFEB);
    ctx.write_vreg(rbx, 0xAABB_CCDD_EEFF_00FD);
    execute_lifted_x86(&[0xF6, 0xFB], &mut ctx, &mut memory); // IDIV BL
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_0007);
    assert_eq!(ctx.read_vreg(rdx), 0x8877_6655_4433_2211);
}
#[test]
fn lifted_setcc_writes_legacy_high_bytes_and_rex_low_bytes() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();

    ctx.flags.materialized = MaterializedFlags::from_rflags(1 << 6); // ZF=1
    ctx.flags.lazy = None;
    ctx.write_vreg(rax, 0x1122_3344_5566_AA55);
    execute_lifted_x86(&[0x0F, 0x94, 0xC4], &mut ctx, &mut memory); // SETE AH
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_0155);

    ctx.write_vreg(rbx, 0xAABB_CCDD_EEFF_AA55);
    execute_lifted_x86(&[0x0F, 0x95, 0xC7], &mut ctx, &mut memory); // SETNE BH
    assert_eq!(ctx.read_vreg(rbx), 0xAABB_CCDD_EEFF_0055);

    ctx.write_vreg(rsp, 0x1234_5678_9ABC_DE7F);
    execute_lifted_x86(&[0x40, 0x0F, 0x94, 0xC4], &mut ctx, &mut memory); // SETE SPL
    assert_eq!(ctx.read_vreg(rsp), 0x1234_5678_9ABC_DE01);
}
#[test]
fn lifted_cmpxchg_xadd_handle_legacy_high_bytes_and_aliases() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let rbp = VReg::Arch(ArchReg::X86(X86Reg::Rbp));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();

    ctx.write_vreg(rax, 0x1122_3344_5566_0505);
    ctx.write_vreg(rbx, 0xAABB_CCDD_EEFF_0955);
    execute_lifted_x86(&[0x0F, 0xB0, 0xFC], &mut ctx, &mut memory); // CMPXCHG AH,BH
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_0905);
    assert_eq!(ctx.read_vreg(rbx), 0xAABB_CCDD_EEFF_0955);
    ctx.flags.materialize_all();
    assert!(ctx.flags.materialized.zf);

    ctx.write_vreg(rax, 0x1122_3344_5566_0503);
    ctx.write_vreg(rbx, 0xAABB_CCDD_EEFF_0955);
    execute_lifted_x86(&[0x0F, 0xB0, 0xFC], &mut ctx, &mut memory); // mismatch
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_0505);
    assert_eq!(ctx.read_vreg(rbx), 0xAABB_CCDD_EEFF_0955);
    ctx.flags.materialize_all();
    assert!(!ctx.flags.materialized.zf);

    ctx.write_vreg(rax, 0x1122_3344_5566_0201);
    execute_lifted_x86(&[0x0F, 0xB0, 0xE0], &mut ctx, &mut memory); // CMPXCHG AL,AH
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_0202);

    ctx.write_vreg(rax, 0xFFFF_AAAA_1234_5678);
    ctx.write_vreg(rcx, 0xBBBB_CCCC_89AB_CDEF);
    ctx.write_vreg(rdx, 0xDDDD_EEEE_1234_5678);
    execute_lifted_x86(&[0x0F, 0xB1, 0xCA], &mut ctx, &mut memory); // CMPXCHG EDX,ECX
    assert_eq!(
        ctx.read_vreg(rax),
        0xFFFF_AAAA_1234_5678,
        "matching EAX must leave full RAX unchanged"
    );
    assert_eq!(ctx.read_vreg(rdx), 0x89AB_CDEF, "matching EDX zero-extends");

    ctx.write_vreg(rax, 0xFFFF_AAAA_1111_2222);
    ctx.write_vreg(rdx, 0xDDDD_EEEE_3333_4444);
    execute_lifted_x86(&[0x0F, 0xB1, 0xCA], &mut ctx, &mut memory); // mismatch
    assert_eq!(ctx.read_vreg(rax), 0x3333_4444, "mismatch zero-extends EAX");
    assert_eq!(
        ctx.read_vreg(rdx),
        0xDDDD_EEEE_3333_4444,
        "mismatch leaves the destination entirely unchanged"
    );

    ctx.write_vreg(rax, 0x7F);
    ctx.write_vreg(rsp, 0x1234_5678_9ABC_DE7F);
    ctx.write_vreg(rbp, 0xAABB_CCDD_EEFF_0055);
    execute_lifted_x86(&[0x40, 0x0F, 0xB0, 0xEC], &mut ctx, &mut memory); // CMPXCHG SPL,BPL
    assert_eq!(ctx.read_vreg(rsp), 0x1234_5678_9ABC_DE55);
    assert_eq!(ctx.read_vreg(rbp), 0xAABB_CCDD_EEFF_0055);

    ctx.write_vreg(rax, 0x1122_3344_5566_0201);
    ctx.write_vreg(rbx, 0xAABB_CCDD_EEFF_0355);
    execute_lifted_x86(&[0x0F, 0xC0, 0xFC], &mut ctx, &mut memory); // XADD AH,BH
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_0501);
    assert_eq!(ctx.read_vreg(rbx), 0xAABB_CCDD_EEFF_0255);

    ctx.write_vreg(rax, 0x1122_3344_5566_0201);
    execute_lifted_x86(&[0x0F, 0xC0, 0xE0], &mut ctx, &mut memory); // XADD AL,AH
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_0103);

    ctx.write_vreg(rax, 0x1122_3344_5566_0080);
    execute_lifted_x86(&[0x0F, 0xC0, 0xC0], &mut ctx, &mut memory); // XADD AL,AL
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_0000);
    ctx.flags.materialize_all();
    assert!(ctx.flags.materialized.cf);
    assert!(ctx.flags.materialized.of);
    assert!(ctx.flags.materialized.zf);

    ctx.write_vreg(rax, 0x1122_3344_5566_03AA);
    ctx.write_vreg(rbx, 0x200);
    memory.write(0x200, &[4]).unwrap();
    execute_lifted_x86(&[0x0F, 0xC0, 0x23], &mut ctx, &mut memory); // XADD [RBX],AH
    let mut byte = [0u8; 1];
    memory.read(0x200, &mut byte).unwrap();
    assert_eq!(byte[0], 7);
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_04AA);

    ctx.write_vreg(rax, 1);
    ctx.write_vreg(rsp, 0x1234_5678_9ABC_DE02);
    execute_lifted_x86(&[0x40, 0x0F, 0xC0, 0xC4], &mut ctx, &mut memory); // XADD SPL,AL
    assert_eq!(ctx.read_vreg(rsp), 0x1234_5678_9ABC_DE03);
    assert_eq!(ctx.read_vreg(rax), 2);
}
#[test]
fn lifted_xlat_executes_address_size_segment_and_fault_semantics() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let fs = VReg::Arch(ArchReg::X86(X86Reg::FsBase));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();

    memory.write(0x103, &[0xA5]).unwrap();
    ctx.write_vreg(rax, 0x1122_3344_5566_7703);
    ctx.write_vreg(rbx, 0x100);
    execute_lifted_x86(&[0xD7], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), 0x1122_3344_5566_77A5);

    memory.write(0x104, &[0x6D]).unwrap();
    ctx.write_vreg(rax, 0xAABB_CCDD_EEFF_0004);
    ctx.write_vreg(rbx, 0xFFFF_FFFF_0000_0100);
    execute_lifted_x86(&[0x67, 0xD7], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), 0xAABB_CCDD_EEFF_006D);

    memory.write(0x322, &[0x3C]).unwrap();
    ctx.write_vreg(rax, 0x0123_4567_89AB_CD02);
    ctx.write_vreg(rbx, 0x20);
    ctx.write_vreg(fs, 0x300);
    execute_lifted_x86(&[0x64, 0xD7], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), 0x0123_4567_89AB_CD3C);

    ctx.write_vreg(rax, 0xDEAD_BEEF_CAFE_BA10);
    ctx.write_vreg(rbx, 0x2000);
    let exit = execute_lifted_x86(&[0xD7], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));
    assert_eq!(ctx.read_vreg(rax), 0xDEAD_BEEF_CAFE_BA10);
}
#[test]
fn lifted_enter_executes_all_frame_phases_and_fault_ordering() {
    let rbp = VReg::Arch(ArchReg::X86(X86Reg::Rbp));
    let rsp = VReg::Arch(ArchReg::X86(X86Reg::Rsp));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();
    let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
        unreachable!()
    };
    x86.efer = 1 << 10;
    x86.cs_l = true;

    ctx.write_vreg(rbp, 0x600);
    ctx.write_vreg(rsp, 0x800);
    execute_lifted_x86(&[0xC8, 0x20, 0x00, 0], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rbp), 0x7F8);
    assert_eq!(ctx.read_vreg(rsp), 0x7D8);
    let mut qword = [0u8; 8];
    memory.read(0x7F8, &mut qword).unwrap();
    assert_eq!(u64::from_le_bytes(qword), 0x600);

    memory.write(0x6FE, &0xBEEFu16.to_le_bytes()).unwrap();
    ctx.write_vreg(rbp, 0x700);
    ctx.write_vreg(rsp, 0x900);
    execute_lifted_x86(&[0x66, 0xC8, 0x10, 0x00, 2], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rbp), 0x8FE);
    assert_eq!(ctx.read_vreg(rsp), 0x8EA);
    let mut word = [0u8; 2];
    memory.read(0x8FE, &mut word).unwrap();
    assert_eq!(u16::from_le_bytes(word), 0x700);
    memory.read(0x8FC, &mut word).unwrap();
    assert_eq!(u16::from_le_bytes(word), 0xBEEF);
    memory.read(0x8FA, &mut word).unwrap();
    assert_eq!(u16::from_le_bytes(word), 0x8FE);

    let mut fault_memory = FlatMemory::with_base(0x700, 0x100);
    ctx.write_vreg(rbp, 0x600);
    ctx.write_vreg(rsp, 0x800);
    let exit = execute_lifted_x86(&[0xC8, 0, 1, 0], &mut ctx, &mut fault_memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));
    assert_eq!(ctx.read_vreg(rsp), 0x800);
    assert_eq!(ctx.read_vreg(rbp), 0x600);
    fault_memory.read(0x7F8, &mut qword).unwrap();
    assert_eq!(u64::from_le_bytes(qword), 0x600);

    let mut late_read_fault = FlatMemory::with_base(0x700, 0x100);
    ctx.write_vreg(rbp, 0x600);
    ctx.write_vreg(rsp, 0x800);
    let exit = execute_lifted_x86(&[0xC8, 0, 0, 2], &mut ctx, &mut late_read_fault);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault { .. })
    ));
    assert_eq!(ctx.read_vreg(rsp), 0x800);
    assert_eq!(ctx.read_vreg(rbp), 0x600);
    late_read_fault.read(0x7F8, &mut qword).unwrap();
    assert_eq!(
        u64::from_le_bytes(qword),
        0x600,
        "the earlier display store precedes a later parent-read fault"
    );

    let mut canonicality_memory = FlatMemory::new(0x1000);
    let noncanonical_rsp = 0x0000_8000_0000_0008;
    ctx.write_vreg(rbp, 0x600);
    ctx.write_vreg(rsp, noncanonical_rsp);
    let exit = execute_lifted_x86(&[0xC8, 0, 0, 0], &mut ctx, &mut canonicality_memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::StackSegment {
            addr: 0x1000,
            error_code: 0
        })
    ));
    assert_eq!(ctx.read_vreg(rsp), noncanonical_rsp);
    assert_eq!(ctx.read_vreg(rbp), 0x600);

    ctx.write_vreg(rsp, 0x800);
    let ArchRegState::X86_64(x86) = &mut ctx.arch_regs else {
        unreachable!()
    };
    x86.apx_enabled = false;
    let exit = execute_lifted_x86(
        &[0xD5, 0x00, 0xC8, 0, 0, 0],
        &mut ctx,
        &mut canonicality_memory,
    );
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::Undefined { addr: 0x1000, .. })
    ));
    assert_eq!(ctx.read_vreg(rsp), 0x800);
    assert_eq!(ctx.read_vreg(rbp), 0x600);
}
#[test]
fn lifted_rdtsc_reads_cycle_counter_and_zero_extends_edx_eax() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
    let mut memory = FlatMemory::new(0x1000);
    let mut ctx = SmirContext::new_x86_64();
    ctx.cycle_count = 0x1234_5678_9ABC_DEF0;
    ctx.write_vreg(rax, u64::MAX);
    ctx.write_vreg(rdx, u64::MAX);

    execute_lifted_x86(&[0x0F, 0x31], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), 0x9ABC_DEF0);
    assert_eq!(ctx.read_vreg(rdx), 0x1234_5678);
    assert_eq!(ctx.cycle_count, 0x1234_5678_9ABC_DEF0);
}
#[test]
fn x87_legacy_environment_images_match_16_and_32_bit_protected_layouts() {
    fn raw(id: u8) -> [u8; 10] {
        [id, id, id, id, id, id, id, id, id, id]
    }

    let mut state = crate::smir::X86X87State::default();
    state.control_word = 0x1240;
    state.status_word = 0x5A80;
    state.tag_word = 0x39E4;
    state.instr_ptr = 0x1122_3344_5566_7788;
    state.data_ptr = 0x99AA_BBCC_DDEE_FF00;
    state.last_opcode = 0x0765;
    for physical in 0..8 {
        state.regs[physical] = raw(physical as u8);
    }

    let (env32, len32) = SmirInterpreter::x86_x87_environment_image(&state, X86X87EnvWidth::W32);
    assert_eq!(len32, 28);
    let mut expected32 = [0u8; 28];
    expected32[0..2].copy_from_slice(&0x1240u16.to_le_bytes());
    expected32[4..6].copy_from_slice(&0x5A80u16.to_le_bytes());
    expected32[8..10].copy_from_slice(&0x39E4u16.to_le_bytes());
    expected32[12..16].copy_from_slice(&0x5566_7788u32.to_le_bytes());
    expected32[18..20].copy_from_slice(&0x0765u16.to_le_bytes());
    expected32[20..24].copy_from_slice(&0xDDEE_FF00u32.to_le_bytes());
    assert_eq!(env32, expected32);

    let (env16, len16) = SmirInterpreter::x86_x87_environment_image(&state, X86X87EnvWidth::W16);
    assert_eq!(len16, 14);
    let mut expected16 = [0u8; 14];
    expected16[0..2].copy_from_slice(&0x1240u16.to_le_bytes());
    expected16[2..4].copy_from_slice(&0x5A80u16.to_le_bytes());
    expected16[4..6].copy_from_slice(&0x39E4u16.to_le_bytes());
    expected16[6..8].copy_from_slice(&0x7788u16.to_le_bytes());
    expected16[10..12].copy_from_slice(&0xFF00u16.to_le_bytes());
    assert_eq!(&env16[..14], &expected16);
    assert_eq!(&env16[14..], &[0; 14]);

    let (save32, save32_len) = SmirInterpreter::x86_x87_state_image(&state, X86X87EnvWidth::W32);
    assert_eq!(save32_len, 108);
    assert_eq!(&save32[..28], &expected32);
    for logical in 0..8u8 {
        let expected_physical = state.physical_index(logical) as u8;
        let offset = 28 + logical as usize * 10;
        assert_eq!(&save32[offset..offset + 10], &raw(expected_physical));
    }

    let (save16, save16_len) = SmirInterpreter::x86_x87_state_image(&state, X86X87EnvWidth::W16);
    assert_eq!(save16_len, 94);
    assert_eq!(&save16[..14], &expected16);
    for logical in 0..8u8 {
        let expected_physical = state.physical_index(logical) as u8;
        let offset = 14 + logical as usize * 10;
        assert_eq!(&save16[offset..offset + 10], &raw(expected_physical));
    }

    let mut restored = crate::smir::X86X87State::default();
    SmirInterpreter::restore_x86_x87_state(&mut restored, &save32, X86X87EnvWidth::W32);
    assert_eq!(restored.control_word, state.control_word);
    assert_eq!(restored.status_word, state.status_word);
    assert_eq!(restored.tag_word, state.tag_word);
    assert_eq!(restored.instr_ptr, 0x5566_7788);
    assert_eq!(restored.data_ptr, 0xDDEE_FF00);
    assert_eq!(restored.last_opcode, state.last_opcode);
    assert_eq!(restored.regs, state.regs);

    let mut restored16 = crate::smir::X86X87State::default();
    restored16.last_opcode = 0x0321;
    SmirInterpreter::restore_x86_x87_state(&mut restored16, &save16, X86X87EnvWidth::W16);
    assert_eq!(restored16.control_word, state.control_word);
    assert_eq!(restored16.status_word, state.status_word);
    assert_eq!(restored16.tag_word, state.tag_word);
    assert_eq!(restored16.instr_ptr, 0x7788);
    assert_eq!(restored16.data_ptr, 0xFF00);
    assert_eq!(restored16.last_opcode, 0x0321, "m14byte has no FOP field");
    assert_eq!(restored16.regs, state.regs);
}
#[test]
fn lifted_group9_cmpxchg_random_seed_and_rdpid_semantics() {
    fn read_u64(memory: &mut FlatMemory, addr: u64) -> u64 {
        let mut bytes = [0u8; 8];
        memory.read(addr, &mut bytes).unwrap();
        u64::from_le_bytes(bytes)
    }

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let rdx = VReg::Arch(ArchReg::X86(X86Reg::Rdx));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let rsi = VReg::Arch(ArchReg::X86(X86Reg::Rsi));
    let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.write_vreg(rsi, 0x100);

    // CMPXCHG8B success writes ECX:EBX, sets only ZF, and leaves the
    // implicit comparison registers unchanged.
    let expected8 = 0x1122_3344_5566_7788u64;
    memory.write(0x100, &expected8.to_le_bytes()).unwrap();
    ctx.write_vreg(rax, 0xAAAA_AAAA_5566_7788);
    ctx.write_vreg(rdx, 0xBBBB_BBBB_1122_3344);
    ctx.write_vreg(rbx, 0xDEAD_BEEF_CAFE_BABE);
    ctx.write_vreg(rcx, 0x0123_4567_89AB_CDEF);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0x895);
    ctx.flags.lazy = None;
    execute_lifted_x86(&[0x0F, 0xC7, 0x0E], &mut ctx, &mut memory);
    assert_eq!(read_u64(&mut memory, 0x100), 0x89AB_CDEF_CAFE_BABE);
    assert_eq!(ctx.read_vreg(rax), 0xAAAA_AAAA_5566_7788);
    assert_eq!(ctx.read_vreg(rdx), 0xBBBB_BBBB_1122_3344);
    ctx.flags.materialize_all();
    assert!(ctx.flags.materialized.zf);
    assert_eq!(
        ctx.flags.materialized.to_rflags() & !0x40,
        MaterializedFlags::from_rflags(0x895).to_rflags() & !0x40
    );

    // Failure writes no source value and zero-extends the memory halves
    // into EDX:EAX while preserving every non-ZF arithmetic flag.
    let observed8 = 0xA1A2_A3A4_B1B2_B3B4u64;
    memory.write(0x100, &observed8.to_le_bytes()).unwrap();
    ctx.write_vreg(rax, u64::MAX);
    ctx.write_vreg(rdx, u64::MAX);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0x8D5);
    ctx.flags.lazy = None;
    execute_lifted_x86(&[0xF0, 0x0F, 0xC7, 0x0E], &mut ctx, &mut memory);
    assert_eq!(read_u64(&mut memory, 0x100), observed8);
    assert_eq!(ctx.read_vreg(rax), 0xB1B2_B3B4);
    assert_eq!(ctx.read_vreg(rdx), 0xA1A2_A3A4);
    ctx.flags.materialize_all();
    assert!(!ctx.flags.materialized.zf);
    assert_eq!(
        ctx.flags.materialized.to_rflags() & !0x40,
        MaterializedFlags::from_rflags(0x8D5).to_rflags() & !0x40
    );

    let mut read_only = StoreFaultMemory {
        inner: FlatMemory::new(0x200),
        stores_before_fault: 0,
    };
    read_only
        .inner
        .write(0x100, &observed8.to_le_bytes())
        .unwrap();
    ctx.write_vreg(rax, 0x1111_1111);
    ctx.write_vreg(rdx, 0x2222_2222);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;
    let exit = execute_lifted_x86(&[0xF0, 0x0F, 0xC7, 0x0E], &mut ctx, &mut read_only);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::MemoryFault {
            addr: 0x100,
            write: true
        })
    ));
    assert_eq!(ctx.read_vreg(rax), 0x1111_1111);
    assert_eq!(ctx.read_vreg(rdx), 0x2222_2222);
    ctx.flags.materialize_all();
    assert_eq!(
        ctx.flags.materialized.to_rflags(),
        MaterializedFlags::from_rflags(0xCD7).to_rflags()
    );

    // CMPXCHG16B transfers full 128-bit pairs and requires 16-byte
    // alignment before any memory or register side effect.
    memory
        .write(0x100, &0x0102_0304_0506_0708u64.to_le_bytes())
        .unwrap();
    memory
        .write(0x108, &0x1112_1314_1516_1718u64.to_le_bytes())
        .unwrap();
    ctx.write_vreg(rax, 0x0102_0304_0506_0708);
    ctx.write_vreg(rdx, 0x1112_1314_1516_1718);
    ctx.write_vreg(rbx, 0x2122_2324_2526_2728);
    ctx.write_vreg(rcx, 0x3132_3334_3536_3738);
    execute_lifted_x86(&[0x48, 0x0F, 0xC7, 0x0E], &mut ctx, &mut memory);
    assert_eq!(read_u64(&mut memory, 0x100), 0x2122_2324_2526_2728);
    assert_eq!(read_u64(&mut memory, 0x108), 0x3132_3334_3536_3738);
    ctx.write_vreg(rax, 0);
    ctx.write_vreg(rdx, 0);
    execute_lifted_x86(&[0x48, 0x0F, 0xC7, 0x0E], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), 0x2122_2324_2526_2728);
    assert_eq!(ctx.read_vreg(rdx), 0x3132_3334_3536_3738);

    ctx.write_vreg(rsi, 0x108);
    ctx.write_vreg(rax, 0x55);
    ctx.write_vreg(rdx, 0x66);
    let before = read_u64(&mut memory, 0x108);
    let exit = execute_lifted_x86(&[0x48, 0x0F, 0xC7, 0x0E], &mut ctx, &mut memory);
    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::GeneralProtection {
            addr: 0x1000,
            error_code: 0
        })
    ));
    assert_eq!(ctx.read_vreg(rax), 0x55);
    assert_eq!(ctx.read_vreg(rdx), 0x66);
    assert_eq!(read_u64(&mut memory, 0x108), before);

    // RDRAND/RDSEED use the host entropy source when available. Both
    // success and architecturally permitted source-not-ready outcomes are
    // accepted; width and flag behavior are invariant.
    ctx.write_vreg(rax, 0xCAFE_BABE_DEAD_BEEF);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0x8D5);
    ctx.flags.lazy = None;
    execute_lifted_x86(&[0x66, 0x0F, 0xC7, 0xF0], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax) >> 16, 0xCAFE_BABE_DEAD);
    ctx.flags.materialize_all();
    assert!(!ctx.flags.materialized.of);
    assert!(!ctx.flags.materialized.sf);
    assert!(!ctx.flags.materialized.zf);
    assert!(!ctx.flags.materialized.af);
    assert!(!ctx.flags.materialized.pf);
    if !ctx.flags.materialized.cf {
        assert_eq!(ctx.read_vreg(rax) as u16, 0);
    }

    ctx.write_vreg(r8, u64::MAX);
    execute_lifted_x86(&[0x41, 0x0F, 0xC7, 0xF8], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(r8) >> 32, 0);
    if !ctx.flags.materialized.cf {
        assert_eq!(ctx.read_vreg(r8), 0);
    }

    // RDPID reads the common IA32_TSC_AUX state, ignores 66/REX.W, and
    // leaves all flags unchanged.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.tsc_aux = 0xA1B2_C3D4;
    }
    ctx.write_vreg(rax, u64::MAX);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;
    execute_lifted_x86(&[0x66, 0xF3, 0x0F, 0xC7, 0xF8], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), 0xA1B2_C3D4);
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
    ctx.write_vreg(rax, u64::MAX);
    execute_lifted_x86(&[0xF3, 0x48, 0x0F, 0xC7, 0xF8], &mut ctx, &mut memory);
    assert_eq!(ctx.read_vreg(rax), 0xA1B2_C3D4);
}
#[test]
fn lifted_x86_cache_control_preserves_state_and_faults_on_invalid_address() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x220);
    ctx.write_vreg(rax, 0x200);
    ctx.flags.materialized = MaterializedFlags::from_rflags(0xCD7);
    ctx.flags.lazy = None;
    let original = [0xA5u8; 16];
    memory.write(0x200, &original).unwrap();
    for bytes in [
        &[0x0F, 0xAE, 0x38][..],
        &[0x66, 0x0F, 0xAE, 0x38][..],
        &[0x66, 0x0F, 0xAE, 0x30][..],
    ] {
        assert!(matches!(
            execute_lifted_x86(bytes, &mut ctx, &mut memory),
            BlockResult::Exit(ExitReason::Halt)
        ));
    }
    let mut actual = [0u8; 16];
    memory.read(0x200, &mut actual).unwrap();
    assert_eq!(actual, original);
    ctx.write_vreg(rax, 0x400);
    assert!(matches!(
        execute_lifted_x86(&[0x0F, 0x1C, 0x00], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    assert!(matches!(
        execute_lifted_x86(&[0x0F, 0xAE, 0x38], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), 0xCD7);
}
#[test]
fn compute_address_wraps_signed_offsets() {
    let interp = SmirInterpreter::new();
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rbx = VReg::Arch(ArchReg::X86(X86Reg::Rbx));
    let fs_base = VReg::Arch(ArchReg::X86(X86Reg::FsBase));

    let mut ctx = SmirContext::new_x86_64();
    ctx.pc = i64::MIN as u64;
    ctx.write_vreg(rax, i64::MIN as u64);
    ctx.write_vreg(rbx, i64::MIN as u64);
    ctx.write_vreg(fs_base, i64::MIN as u64);

    assert_eq!(
        interp.compute_address(
            &ctx,
            &Address::BaseOffset {
                base: rax,
                offset: -1,
                disp_size: DispSize::Auto,
            },
        ),
        (i64::MIN as u64).wrapping_add((-1i64) as u64)
    );
    assert_eq!(
        interp.compute_address(
            &ctx,
            &Address::BaseIndexScale {
                base: Some(rax),
                index: rbx,
                scale: 8,
                disp: -1,
                disp_size: DispSize::Auto,
            },
        ),
        (i64::MIN as u64)
            .wrapping_add((i64::MIN as u64).wrapping_mul(8))
            .wrapping_add((-1i64) as u64)
    );
    assert_eq!(
        interp.compute_address(
            &ctx,
            &Address::PcRel {
                offset: -1,
                disp_size: DispSize::Auto,
                base: None,
            },
        ),
        (i64::MIN as u64).wrapping_add((-1i64) as u64)
    );
    assert_eq!(
        interp.compute_address(
            &ctx,
            &Address::SegmentRel {
                segment: fs_base,
                base: Some(rax),
                index: Some(rbx),
                scale: 8,
                disp: -1,
            },
        ),
        (i64::MIN as u64)
            .wrapping_add(i64::MIN as u64)
            .wrapping_add((i64::MIN as u64).wrapping_mul(8))
            .wrapping_add((-1i64) as u64)
    );
}
#[test]
fn gp_relative_address_wraps_negative_offsets() {
    let interp = SmirInterpreter::new();
    let mut ctx = SmirContext::new_hexagon();
    ctx.write_arch_reg(ArchReg::Hexagon(HexagonReg::Gp), 0);

    assert_eq!(
        interp.compute_address(&ctx, &Address::GpRel { offset: -1 }),
        u64::MAX
    );
}
#[test]
fn non_x86_divs_min_overflow_wraps_without_trap() {
    let quot = VReg::Virtual(VirtualId(1));
    let rem = VReg::Virtual(VirtualId(2));
    let src1 = VReg::Virtual(VirtualId(3));
    let src2 = VReg::Virtual(VirtualId(4));

    let mut ctx = SmirContext::new_aarch64();
    ctx.write_vreg(quot, 0x1111);
    ctx.write_vreg(rem, 0x2222);
    ctx.write_vreg(src1, i64::MIN as u64);
    ctx.write_vreg(src2, (-1i64) as u64);

    let interp = SmirInterpreter::new();
    let mut memory = FlatMemory::new(0x1000);
    interp
        .execute_op(
            &mut ctx,
            &mut memory,
            &SmirOp::new(
                OpId(0),
                0x1000,
                OpKind::DivS {
                    quot,
                    rem: Some(rem),
                    src1,
                    src2: SrcOperand::Reg(src2),
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                },
            ),
        )
        .unwrap();

    assert!(ctx.exit_reason.is_none());
    assert_eq!(ctx.read_vreg(quot), i64::MIN as u64);
    assert_eq!(ctx.read_vreg(rem), 0);
}
#[test]
fn executes_direct_masked_vbmi_permute_operand_roles() {
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x1000);
    let interp = SmirInterpreter::new();

    let byte_table1 = (0x10u8..0x20).collect::<Vec<_>>();
    let byte_table2 = (0x80u8..0x90).collect::<Vec<_>>();
    let byte_indices = [0u8, 15, 16, 31, 1, 14, 17, 30, 2, 13, 18, 29, 3, 12, 19, 28];
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = vec_from_bytes(&byte_indices);
        x86.xmm[2] = vec_from_bytes(&byte_table1);
        x86.xmm[3] = vec_from_bytes(&byte_table2);
        x86.k[2] = 0xA55A;
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x8A, 0x75, 0xCB], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for (lane, selected) in byte_indices.iter().copied().enumerate() {
            let expected = if (0xA55Au64 & (1 << lane)) == 0 {
                0
            } else if selected < 16 {
                byte_table1[selected as usize]
            } else {
                byte_table2[(selected - 16) as usize]
            };
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], lane as u8, 8),
                u64::from(expected)
            );
        }
    }

    let word_table1 = (0..16).map(|lane| 0x1000u16 + lane).collect::<Vec<_>>();
    let word_table2 = (0..16).map(|lane| 0x8000u16 + lane).collect::<Vec<_>>();
    let word_indices = [0u16, 31, 1, 30, 2, 29, 3, 28, 4, 27, 5, 26, 6, 25, 7, 24];
    let to_bytes = |words: &[u16]| {
        words
            .iter()
            .copied()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
    };
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[4] = vec_from_bytes(&to_bytes(&word_table1));
        x86.xmm[5] = vec_from_bytes(&to_bytes(&word_indices));
        x86.xmm[6] = vec_from_bytes(&to_bytes(&word_table2));
        x86.k[3] = 0x5AA5;
    }
    assert!(matches!(
        execute_lifted_x86(&[0x62, 0xF2, 0xD5, 0x2B, 0x7D, 0xE6], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for (lane, selected) in word_indices.iter().copied().enumerate() {
            let expected = if (0x5AA5u64 & (1 << lane)) == 0 {
                word_table1[lane]
            } else if selected < 16 {
                word_table1[selected as usize]
            } else {
                word_table2[(selected - 16) as usize]
            };
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[4], lane as u8, 16),
                u64::from(expected)
            );
        }
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = [u64::MAX; 16];
        x86.xmm[2] = vec_from_bytes(&byte_indices);
        x86.xmm[3] = vec_from_bytes(&byte_table1);
        x86.k[2] = 0xA55A;
    }
    interp
        .execute_op(
            &mut ctx,
            &mut memory,
            &SmirOp::new(
                OpId(2),
                0x1000,
                OpKind::X86PermuteBytesWords {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    table1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                    table2: None,
                    indices: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::I8,
                    width: VecWidth::V128,
                    overwrite_table: false,
                    zeroing: true,
                },
            ),
        )
        .unwrap();
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for (lane, selected) in byte_indices.iter().copied().enumerate() {
            let expected = if (0xA55Au64 & (1 << lane)) == 0 {
                0
            } else {
                byte_table1[(selected & 15) as usize]
            };
            assert_eq!(
                SmirInterpreter::get_lane(&x86.xmm[1], lane as u8, 8),
                u64::from(expected)
            );
        }
    }
}
#[test]
fn x86_unsigned_narrow_treats_high_bit_sources_as_unsigned() {
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
    let src = VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)));
    let mut source = [0u64; 16];
    for (lane, value) in [u32::MAX, 0, 255, 256].into_iter().enumerate() {
        SmirInterpreter::set_lane(&mut source, lane as u8, 32, u64::from(value));
    }
    let mut ctx = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[2] = source;
    }
    let mut memory = FlatMemory::new(0x1000);
    SmirInterpreter::new()
        .execute_op(
            &mut ctx,
            &mut memory,
            &SmirOp::new(
                OpId(0),
                0x1000,
                OpKind::X86NarrowInt {
                    dst,
                    src,
                    mask: None,
                    src_elem: VecElementType::I32,
                    dst_elem: VecElementType::I8,
                    width: VecWidth::V128,
                    mode: X86NarrowMode::UnsignedSaturate,
                    zeroing: false,
                },
            ),
        )
        .unwrap();

    let result = if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        x86.xmm[1]
    } else {
        unreachable!()
    };
    assert_eq!(
        result[0].to_le_bytes()[..4],
        [0xFF, 0x00, 0xFF, 0xFF],
        "unsigned saturation maps 0xffffffff to 0xff, not 0"
    );
}
#[test]
fn smir_bit_scan_updates_only_zf_and_preserves_undefined_status_flags() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    const CF: u64 = 1 << 0;
    const PF: u64 = 1 << 2;
    const AF: u64 = 1 << 4;
    const ZF: u64 = 1 << 6;
    const SF: u64 = 1 << 7;
    const OF: u64 = 1 << 11;
    const STATUS: u64 = CF | PF | AF | ZF | SF | OF;
    let initial = 0x2 | STATUS;
    let zf_only = FlagUpdate::Specific(FlagSet::ZF);

    let (value, flags) = exec_x86_rax_op(
        OpKind::Bsf {
            dst: rax,
            src: rax,
            width: OpWidth::W64,
            flags: zf_only,
        },
        0x100,
        0,
        initial,
    );
    assert_eq!(value, 8);
    assert_eq!(flags & ZF, 0, "nonzero BSF clears ZF");
    assert_eq!(
        flags & (STATUS & !ZF),
        initial & (STATUS & !ZF),
        "BSF must retain deterministic values for undefined flags"
    );

    let (value, flags) = exec_x86_rax_op(
        OpKind::Bsr {
            dst: rax,
            src: rax,
            width: OpWidth::W32,
            flags: zf_only,
        },
        0,
        0,
        initial & !ZF,
    );
    assert_eq!(value, 0, "interpreter's deterministic zero-source result");
    assert_ne!(flags & ZF, 0, "zero BSR sets ZF");
    assert_eq!(
        flags & (STATUS & !ZF),
        initial & (STATUS & !ZF),
        "BSR must retain deterministic values for undefined flags"
    );

    let (_, flags) = exec_x86_rax_op(
        OpKind::Bsf {
            dst: rax,
            src: rax,
            width: OpWidth::W16,
            flags: FlagUpdate::None,
        },
        1,
        0,
        initial,
    );
    assert_eq!(
        flags, initial,
        "flag-suppressed generic BSF preserves RFLAGS"
    );
}
#[test]
fn smir_x86_bls_matches_result_and_partial_flag_contracts() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    const CF: u64 = 1 << 0;
    const PF: u64 = 1 << 2;
    const AF: u64 = 1 << 4;
    const ZF: u64 = 1 << 6;
    const SF: u64 = 1 << 7;
    const OF: u64 = 1 << 11;
    const STATUS: u64 = CF | PF | AF | ZF | SF | OF;
    let defined = FlagUpdate::Specific(
        FlagSet::CF
            .union(FlagSet::ZF)
            .union(FlagSet::SF)
            .union(FlagSet::OF),
    );
    let initial = 0x2 | PF | AF | ZF | OF;

    for (kind, source, width, expected, expected_defined) in [
        (X86BlsKind::Blsr, 0, OpWidth::W64, 0, CF | ZF),
        (
            X86BlsKind::Blsmsk,
            0,
            OpWidth::W32,
            u64::from(u32::MAX),
            CF | SF,
        ),
        (
            X86BlsKind::Blsi,
            0x8000_0000_0000_0000,
            OpWidth::W64,
            0x8000_0000_0000_0000,
            CF | SF,
        ),
    ] {
        let (value, got_flags) = exec_x86_rax_op(
            OpKind::X86Bls {
                dst: rax,
                src: rcx,
                width,
                kind,
                flags: defined,
            },
            0xAAAA,
            source,
            initial,
        );
        assert_eq!(value, expected, "{kind:?} result");
        assert_eq!(
            got_flags & (CF | ZF | SF | OF),
            expected_defined,
            "{kind:?} defined flags"
        );
        assert_eq!(
            got_flags & (PF | AF),
            initial & (PF | AF),
            "{kind:?} preserves undefined PF/AF"
        );
    }

    let (value, got_flags) = exec_x86_rax_op(
        OpKind::X86Bls {
            dst: rax,
            src: rax,
            width: OpWidth::W64,
            kind: X86BlsKind::Blsi,
            flags: FlagUpdate::None,
        },
        0x18,
        0,
        0x2 | STATUS,
    );
    assert_eq!(value, 0x8, "aliased APX NF BLSI result");
    assert_eq!(got_flags, 0x2 | STATUS, "APX NF BLSI preserves RFLAGS");
}
#[test]
fn smir_x86_rol_ror_preserve_of_for_raw_multi_counts() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let preserved = 0x2 | 0x4 | 0x10 | 0x40 | 0x80 | 0x800;

    // The masked count is 17 while the effective W16 rotation is 1. OF is
    // undefined and follows Rax's deterministic preserve policy.
    let (value, flags) = exec_x86_rax_op(
        OpKind::Rol {
            dst: rax,
            src: rax,
            amount: SrcOperand::Imm(17),
            width: OpWidth::W16,
            flags: FlagUpdate::All,
        },
        0x0001,
        0,
        preserved,
    );
    assert_eq!(value & 0xFFFF, 0x0002);
    assert_eq!(flags & 0x8D5, preserved & 0x8D5);

    let (value, flags) = exec_x86_rax_op(
        OpKind::Ror {
            dst: rax,
            src: rax,
            amount: SrcOperand::Imm(17),
            width: OpWidth::W16,
            flags: FlagUpdate::All,
        },
        0x0001,
        0,
        preserved,
    );
    assert_eq!(value & 0xFFFF, 0x8000);
    assert_eq!(flags & 0x8D5, (preserved | 0x1) & 0x8D5);
}
#[test]
fn smir_x86_rcl_rcr_match_rotate_through_carry_oracle_cases() {
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
    let preserved = 0x2 | 0x4 | 0x10 | 0x40 | 0x80;

    // Legacy x86 rotate tests assert these same architectural cases:
    // RCL AL,1 with CF=1: 0x42 -> 0x85, CF=0, OF=1.
    let (value, flags) = exec_x86_rax_op(
        OpKind::Rcl {
            dst: rax,
            src: rax,
            amount: SrcOperand::Imm(1),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        },
        0x42,
        0,
        preserved | 0x1,
    );
    assert_eq!(value & 0xFF, 0x85);
    assert_eq!(flags & 0x8D5, (preserved | 0x800) & 0x8D5);

    // RCR AL,1 with CF=0: 0x81 -> 0x40, CF=1, OF=1.
    let (value, flags) = exec_x86_rax_op(
        OpKind::Rcr {
            dst: rax,
            src: rax,
            amount: SrcOperand::Imm(1),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        },
        0x81,
        0,
        preserved,
    );
    assert_eq!(value & 0xFF, 0x40);
    assert_eq!(flags & 0x8D5, (preserved | 0x1 | 0x800) & 0x8D5);

    // RCR AL,9 is a full 9-bit rotate-through-carry period: value and flags
    // are unchanged because the effective count is zero.
    let start_flags = preserved | 0x1 | 0x800;
    let (value, flags) = exec_x86_rax_op(
        OpKind::Rcr {
            dst: rax,
            src: rax,
            amount: SrcOperand::Imm(9),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        },
        0xA5,
        0,
        start_flags,
    );
    assert_eq!(value & 0xFF, 0xA5);
    assert_eq!(flags & 0x8D5, start_flags & 0x8D5);

    // Raw count 10 has effective count 1 for an 8-bit rotate through carry,
    // but OF is undefined and follows Rax's preserve policy because the raw
    // masked count is greater than one.
    let (value, flags) = exec_x86_rax_op(
        OpKind::Rcl {
            dst: rax,
            src: rax,
            amount: SrcOperand::Imm(10),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        },
        0x40,
        0,
        preserved,
    );
    assert_eq!(value & 0xFF, 0x80);
    assert_eq!(flags & 0x8D5, preserved & 0x8D5);

    let start_flags = preserved | 0x800;
    let (value, flags) = exec_x86_rax_op(
        OpKind::Rcr {
            dst: rax,
            src: rax,
            amount: SrcOperand::Imm(10),
            width: OpWidth::W8,
            flags: FlagUpdate::All,
        },
        0x01,
        0,
        start_flags,
    );
    assert_eq!(value & 0xFF, 0x00);
    assert_eq!(flags & 0x8D5, (start_flags | 0x1) & 0x8D5);

    // RCL RAX,32 and RCR RAX,CL mirror the legacy emulator's 64-bit cases.
    let (value, _) = exec_x86_rax_op(
        OpKind::Rcl {
            dst: rax,
            src: rax,
            amount: SrcOperand::Imm(32),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
        0x1234_5678_9ABC_DEF0,
        0,
        0x2,
    );
    assert_eq!(value, 0x9ABC_DEF0_091A_2B3C);

    let (value, _) = exec_x86_rax_op(
        OpKind::Rcr {
            dst: rax,
            src: rax,
            amount: SrcOperand::Reg(rcx),
            width: OpWidth::W64,
            flags: FlagUpdate::All,
        },
        0x1234_5678_9ABC_DEF0,
        16,
        0x2,
    );
    assert_eq!(value, 0xBDE0_1234_5678_9ABC);
}
#[test]
fn test_basic_arithmetic() {
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x1000);
    let mut interp = SmirInterpreter::new();

    // Build a simple function: v0 = 10 + 5
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    let v0 = builder.alloc_vreg();
    let v1 = builder.alloc_vreg();
    let v2 = builder.alloc_vreg();

    builder.push_op(
        0x1000,
        OpKind::Mov {
            dst: v0,
            src: SrcOperand::Imm(10),
            width: OpWidth::W64,
        },
    );

    builder.push_op(
        0x1004,
        OpKind::Mov {
            dst: v1,
            src: SrcOperand::Imm(5),
            width: OpWidth::W64,
        },
    );

    builder.push_op(
        0x1008,
        OpKind::Add {
            dst: v2,
            src1: v0,
            src2: SrcOperand::Reg(v1),
            width: OpWidth::W64,
            flags: FlagUpdate::None,
        },
    );

    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });

    let func = builder.finish();
    let block = func.blocks[0].clone();

    interp.add_block(0x1000, block);
    ctx.pc = 0x1000;

    let exit = interp.run(&mut ctx, &mut memory);

    assert!(matches!(exit, ExitReason::Halt));
    assert_eq!(ctx.read_vreg(v2), 15);
}
#[test]
fn test_conditional_branch() {
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x1000);
    let mut interp = SmirInterpreter::new();

    // Build: if (1) goto taken else goto not_taken
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    let v_cond = builder.alloc_vreg();
    let v_result = builder.alloc_vreg();

    let taken = builder.create_block(0x1100);
    let not_taken = builder.create_block(0x1200);

    // Entry block
    builder.push_op(
        0x1000,
        OpKind::Mov {
            dst: v_cond,
            src: SrcOperand::Imm(1),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::CondBranch {
        cond: v_cond,
        true_target: taken,
        false_target: not_taken,
    });

    // Taken block
    builder.switch_to_block(taken);
    builder.push_op(
        0x1100,
        OpKind::Mov {
            dst: v_result,
            src: SrcOperand::Imm(100),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });

    // Not taken block
    builder.switch_to_block(not_taken);
    builder.push_op(
        0x1200,
        OpKind::Mov {
            dst: v_result,
            src: SrcOperand::Imm(200),
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });

    let func = builder.finish();

    for block in &func.blocks {
        interp.add_block(block.guest_pc, block.clone());
    }

    ctx.pc = 0x1000;
    let exit = interp.run(&mut ctx, &mut memory);

    assert!(matches!(exit, ExitReason::Halt));
    assert_eq!(ctx.read_vreg(v_result), 100);
}
#[test]
fn test_vsatdw_clamp() {
    // {V1.w[i] : V0.w[i]} 64-bit -> signed 32 clamp.
    // lane: hi=0x0000_0001, lo=0x0000_0000 => 0x1_0000_0000 -> clamp i32 -> MAX.
    // lane: hi=0xFFFF_FFFF, lo=0x0000_0000 => -0x1_0000_0000 -> clamp -> MIN.
    let mut ctx = SmirContext::new_hexagon();
    let mut memory = FlatMemory::new(0x1000);
    let interp = SmirInterpreter::new();
    // word 0 of each: lo=0, hi=1 (positive overflow); make all words identical.
    if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
        hex.set_v(0, [0u64; 16]); // src_lo low words = 0
        hex.set_v(1, [0x0000_0001_0000_0001u64; 16]); // src_hi = 1 per word
    }
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let block = SmirBlock {
        id: BlockId(0),
        guest_pc: 0x1000,
        phis: vec![],
        ops: vec![SmirOp {
            id: OpId(0),
            guest_pc: 0x1000,
            kind: OpKind::VSatDW {
                dst: mkv(2),
                src_lo: mkv(0),
                src_hi: mkv(1),
            },
            x86_hint: None,
        }],
        terminator: Terminator::Trap {
            kind: TrapKind::Halt,
        },
        exec_count: 0,
    };
    interp.execute_block(&mut ctx, &mut memory, &block);
    if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
        // each word = i32::MAX = 0x7FFF_FFFF
        assert_eq!(hex.get_v(2), [0x7FFF_FFFF_7FFF_FFFFu64; 16]);
    } else {
        panic!("not hexagon");
    }
}
#[test]
fn test_vshiftacc() {
    // dst.h[i] += src.h[i] << 2, with dst seeded to 1 per halfword.
    let mut ctx = SmirContext::new_hexagon();
    let mut memory = FlatMemory::new(0x1000);
    let interp = SmirInterpreter::new();
    if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
        hex.set_v(0, [0x0003_0003_0003_0003u64; 16]); // src = 3
        hex.set_v(1, [0x0001_0001_0001_0001u64; 16]); // dst seed = 1
    }
    ctx.write_arch_reg(ArchReg::Hexagon(HexagonReg::R(0)), 2); // shift amount = 2
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let block = SmirBlock {
        id: BlockId(0),
        guest_pc: 0x1000,
        phis: vec![],
        ops: vec![SmirOp {
            id: OpId(0),
            guest_pc: 0x1000,
            kind: OpKind::VShiftAcc {
                dst: mkv(1),
                src: mkv(0),
                amount: SrcOperand::Reg(VReg::Arch(ArchReg::Hexagon(HexagonReg::R(0)))),
                shift: ShiftOp::Lsl,
                elem: VecElementType::I16,
                lanes: 64,
            },
            x86_hint: None,
        }],
        terminator: Terminator::Trap {
            kind: TrapKind::Halt,
        },
        exec_count: 0,
    };
    interp.execute_block(&mut ctx, &mut memory, &block);
    // 1 + (3<<2) = 13 = 0x000D per halfword.
    match &ctx.arch_regs {
        ArchRegState::Hexagon(hex) => assert_eq!(hex.get_v(1), [0x000D_000D_000D_000Du64; 16]),
        _ => panic!("not hexagon"),
    }
}
#[test]
fn test_vpairpairreducemul() {
    // vmpabusv: lo.h[i] = uu0.ub[2i]*vv0.b[2i] + uu1.ub[2i]*vv1.b[2i].
    // uu0=2, uu1=3, vv0=4, vv1=1 -> 2*4 + 3*1 = 11 per halfword.
    let mut ctx = SmirContext::new_hexagon();
    let mut memory = FlatMemory::new(0x1000);
    let interp = SmirInterpreter::new();
    if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
        hex.set_v(0, [0x0202_0202_0202_0202u64; 16]);
        hex.set_v(1, [0x0303_0303_0303_0303u64; 16]);
        hex.set_v(2, [0x0404_0404_0404_0404u64; 16]);
        hex.set_v(3, [0x0101_0101_0101_0101u64; 16]);
    }
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let block = SmirBlock {
        id: BlockId(0),
        guest_pc: 0x1000,
        phis: vec![],
        ops: vec![SmirOp {
            id: OpId(0),
            guest_pc: 0x1000,
            kind: OpKind::VPairPairReduceMul {
                dst_lo: mkv(4),
                dst_hi: mkv(5),
                src_lo: mkv(0),
                src_hi: mkv(1),
                src2_lo: mkv(2),
                src2_hi: mkv(3),
                narrow_elem: VecElementType::I8,
                out_elem: VecElementType::I16,
                signed1: false,
                signed2: true,
            },
            x86_hint: None,
        }],
        terminator: Terminator::Trap {
            kind: TrapKind::Halt,
        },
        exec_count: 0,
    };
    interp.execute_block(&mut ctx, &mut memory, &block);
    if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
        assert_eq!(hex.get_v(4), [0x000B_000B_000B_000Bu64; 16]); // 11
        assert_eq!(hex.get_v(5), [0x000B_000B_000B_000Bu64; 16]);
    }
}
#[test]
fn test_vpairreducemul() {
    // vmpabus: lo.h[i] = uu0.ub[2i]*Rt.sb[0] + uu1.ub[2i]*Rt.sb[1];
    //          hi.h[i] = uu0.ub[2i+1]*Rt.sb[2] + uu1.ub[2i+1]*Rt.sb[3].
    // uu0=2, uu1=3, Rt bytes all 1 -> lo=hi= 2*1+3*1 = 5 per halfword.
    let mut ctx = SmirContext::new_hexagon();
    let mut memory = FlatMemory::new(0x1000);
    let interp = SmirInterpreter::new();
    if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
        hex.set_v(0, [0x0202_0202_0202_0202u64; 16]); // uu0 = src_lo
        hex.set_v(1, [0x0303_0303_0303_0303u64; 16]); // uu1 = src_hi
        hex.set_v(2, [0x0101_0101_0101_0101u64; 16]); // Rt broadcast (bytes all 1)
    }
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let block = SmirBlock {
        id: BlockId(0),
        guest_pc: 0x1000,
        phis: vec![],
        ops: vec![SmirOp {
            id: OpId(0),
            guest_pc: 0x1000,
            kind: OpKind::VPairReduceMul {
                dst_lo: mkv(3),
                dst_hi: mkv(4),
                src_lo: mkv(0),
                src_hi: mkv(1),
                src2: mkv(2),
                pair_elem: VecElementType::I8,
                rt_elem: VecElementType::I8,
                out_elem: VecElementType::I16,
                signed1: false,
                signed2: true,
                acc: false,
            },
            x86_hint: None,
        }],
        terminator: Terminator::Trap {
            kind: TrapKind::Halt,
        },
        exec_count: 0,
    };
    interp.execute_block(&mut ctx, &mut memory, &block);
    if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
        assert_eq!(hex.get_v(3), [0x0005_0005_0005_0005u64; 16]); // lo = 5
        assert_eq!(hex.get_v(4), [0x0005_0005_0005_0005u64; 16]); // hi = 5
    }
}
#[test]
fn test_vslidereducemul() {
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let run = |v0: [u64; 16], v1: [u64; 16], rt: [u64; 16], op: OpKind| -> ([u64; 16], [u64; 16]) {
        let mut ctx = SmirContext::new_hexagon();
        let mut memory = FlatMemory::new(0x1000);
        let interp = SmirInterpreter::new();
        if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
            hex.set_v(0, v0);
            hex.set_v(1, v1);
            hex.set_v(2, rt); // I32-broadcast of Rt
        }
        let block = SmirBlock {
            id: BlockId(0),
            guest_pc: 0x1000,
            phis: vec![],
            ops: vec![SmirOp {
                id: OpId(0),
                guest_pc: 0x1000,
                kind: op,
                x86_hint: None,
            }],
            terminator: Terminator::Trap {
                kind: TrapKind::Halt,
            },
            exec_count: 0,
        };
        interp.execute_block(&mut ctx, &mut memory, &block);
        if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
            (hex.get_v(3), hex.get_v(4))
        } else {
            unreachable!()
        }
    };
    // mode 0 (vdmpyhb_dv): v0.h=2, v1.h=4, Rt bytes=1 (so all taps=1).
    //   o0 = v0.h[2i]*1 + v0.h[2i+1]*1 = 2+2 = 4
    //   o1 = v0.h[2i+1]*1 + v1.h[2i]*1 = 2+4 = 6
    let v0 = [0x0002_0002_0002_0002u64; 16];
    let v1 = [0x0004_0004_0004_0004u64; 16];
    let rt = [0x0101_0101_0101_0101u64; 16];
    let (lo, hi) = run(
        v0,
        v1,
        rt,
        OpKind::VSlideReduceMul {
            dst_lo: mkv(3),
            dst_hi: mkv(4),
            src_lo: mkv(0),
            src_hi: mkv(1),
            src2: mkv(2),
            src_elem: VecElementType::I16,
            rt_elem: VecElementType::I8,
            out_elem: VecElementType::I32,
            mode: 0,
            signed1: true,
            signed2: true,
            sat: false,
            set_ovf: false,
            acc: false,
        },
    );
    assert_eq!(lo, [0x0000_0004_0000_0004u64; 16]);
    assert_eq!(hi, [0x0000_0006_0000_0006u64; 16]);

    // mode 1 (vtmpyhb): adds a free addend tap.
    //   o0 = v0.h[2i]*1 + v0.h[2i+1]*1 + v1.h[2i]   = 2+2+4 = 8
    //   o1 = v0.h[2i+1]*1 + v1.h[2i]*1 + v1.h[2i+1] = 2+4+4 = 10
    let (lo, hi) = run(
        v0,
        v1,
        rt,
        OpKind::VSlideReduceMul {
            dst_lo: mkv(3),
            dst_hi: mkv(4),
            src_lo: mkv(0),
            src_hi: mkv(1),
            src2: mkv(2),
            src_elem: VecElementType::I16,
            rt_elem: VecElementType::I8,
            out_elem: VecElementType::I32,
            mode: 1,
            signed1: true,
            signed2: true,
            sat: false,
            set_ovf: false,
            acc: false,
        },
    );
    assert_eq!(lo, [0x0000_0008_0000_0008u64; 16]);
    assert_eq!(hi, [0x0000_000A_0000_000Au64; 16]);

    // mode 2 (vdmpyhisat): pair -> single, o[i] = v0.h[2i+1]*Rt.h0 + v1.h[2i]*Rt.h1.
    // Rt.h0 = Rt.h1 = 1 (rt bytes all 1 -> halfword = 0x0101 = 257). Use Rt=1 per half.
    let rt2 = [0x0001_0001_0001_0001u64; 16];
    let (lo, _hi) = run(
        v0,
        v1,
        rt2,
        OpKind::VSlideReduceMul {
            dst_lo: mkv(3),
            dst_hi: mkv(3),
            src_lo: mkv(0),
            src_hi: mkv(1),
            src2: mkv(2),
            src_elem: VecElementType::I16,
            rt_elem: VecElementType::I16,
            out_elem: VecElementType::I32,
            mode: 2,
            signed1: true,
            signed2: true,
            sat: true,
            set_ovf: true,
            acc: false,
        },
    );
    // o = v0.h[2i+1]*1 + v1.h[2i]*1 = 2 + 4 = 6.
    assert_eq!(lo, [0x0000_0006_0000_0006u64; 16]);
}
#[test]
fn test_vrotreducemulpair() {
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let run = |v0: [u64; 16], v1: [u64; 16], rt: [u64; 16], op: OpKind| -> ([u64; 16], [u64; 16]) {
        let mut ctx = SmirContext::new_hexagon();
        let mut memory = FlatMemory::new(0x1000);
        let interp = SmirInterpreter::new();
        if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
            hex.set_v(0, v0); // src_lo
            hex.set_v(1, v1); // src_hi
            hex.set_v(2, rt); // I32-broadcast of Rt
        }
        let block = SmirBlock {
            id: BlockId(0),
            guest_pc: 0x1000,
            phis: vec![],
            ops: vec![SmirOp {
                id: OpId(0),
                guest_pc: 0x1000,
                kind: op,
                x86_hint: None,
            }],
            terminator: Terminator::Trap {
                kind: TrapKind::Halt,
            },
            exec_count: 0,
        };
        interp.execute_block(&mut ctx, &mut memory, &block);
        if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
            (hex.get_v(3), hex.get_v(4))
        } else {
            unreachable!()
        }
    };
    // ---- mode 0, imm=0, product (vrmpyubi): all Vuu bytes=2, Rt bytes=1.
    //   o0 = sel.b[0]*1 + v0.b[1]*1 + v0.b[2]*1 + v0.b[3]*1 = 4*2 = 8
    //   o1 = v1.b[0]*1 + v1.b[1]*1 + sel.b[2]*1 + v0.b[3]*1 = 4*2 = 8
    let v0 = [0x0202_0202_0202_0202u64; 16];
    let v1 = [0x0303_0303_0303_0303u64; 16];
    let rt = [0x0101_0101_0101_0101u64; 16];
    let (lo, hi) = run(
        v0,
        v1,
        rt,
        OpKind::VRotReduceMulPair {
            dst_lo: mkv(3),
            dst_hi: mkv(4),
            src_lo: mkv(0),
            src_hi: mkv(1),
            src2: mkv(2),
            src_elem: VecElementType::I8,
            rt_elem: VecElementType::I8,
            out_elem: VecElementType::I32,
            imm: 0,
            mode: 0,
            signed1: false,
            signed2: false,
            acc: false,
            abs_diff: false,
        },
    );
    // o0: all taps from v0 (sel=v0 since imm=0): 2*1*4 = 8.
    assert_eq!(lo, [0x0000_0008u64 | (0x0000_0008u64 << 32); 16]);
    // o1: v1 taps (3) at bytes 0,1; sel(v0)=2 at byte2; v0=2 at byte3:
    //   3+3+2+2 = 10.
    assert_eq!(hi, [0x0000_000Au64 | (0x0000_000Au64 << 32); 16]);

    // ---- mode 0, imm=1 (vrmpyubi #1): sel = v1; Rt rotate by -1.
    //   o0 = sel.b[0]*rb0 + v0.b[1]*rb1 + v0.b[2]*rb2 + v0.b[3]*rb3
    //   with rb(n)=Rt[(n-1)&3]; all Rt bytes are 1 so rb=1 everywhere.
    //   o0 = v1*1 + v0*1 + v0*1 + v0*1 = 3+2+2+2 = 9
    //   o1 = v1.b[0]*rb2 + v1.b[1]*rb3 + sel.b[2]*rb0 + v0.b[3]*rb1
    //      = 3 + 3 + 3 + 2 = 11
    let (lo, hi) = run(
        v0,
        v1,
        rt,
        OpKind::VRotReduceMulPair {
            dst_lo: mkv(3),
            dst_hi: mkv(4),
            src_lo: mkv(0),
            src_hi: mkv(1),
            src2: mkv(2),
            src_elem: VecElementType::I8,
            rt_elem: VecElementType::I8,
            out_elem: VecElementType::I32,
            imm: 1,
            mode: 0,
            signed1: false,
            signed2: false,
            acc: false,
            abs_diff: false,
        },
    );
    assert_eq!(lo, [0x0000_0009u64 | (0x0000_0009u64 << 32); 16]);
    assert_eq!(hi, [0x0000_000Bu64 | (0x0000_000Bu64 << 32); 16]);

    // ---- mode 0, imm=0, abs_diff (vrsadubi): |Vuu.ub - Rt.ub|.
    //   o0 = |sel-1| + |v0-1| + |v0-1| + |v0-1| = 4*|2-1| = 4
    //   o1 = |v1-1|*2 + |sel-1| + |v0-1| = 2*2 + 1 + 1 = 6
    let (lo, hi) = run(
        v0,
        v1,
        rt,
        OpKind::VRotReduceMulPair {
            dst_lo: mkv(3),
            dst_hi: mkv(4),
            src_lo: mkv(0),
            src_hi: mkv(1),
            src2: mkv(2),
            src_elem: VecElementType::I8,
            rt_elem: VecElementType::I8,
            out_elem: VecElementType::I32,
            imm: 0,
            mode: 0,
            signed1: false,
            signed2: false,
            acc: false,
            abs_diff: true,
        },
    );
    assert_eq!(lo, [0x0000_0004u64 | (0x0000_0004u64 << 32); 16]);
    assert_eq!(hi, [0x0000_0006u64 | (0x0000_0006u64 << 32); 16]);

    // ---- mode 1, abs_diff (vdsaduh): unsigned halfwords.
    //   r0 = r1 = 1 (Rt.uh). v0.uh = 4, v1.uh = 6.
    //   o0 = |v0.uh[2i]-1| + |v0.uh[2i+1]-1| = 3 + 3 = 6
    //   o1 = |v0.uh[2i+1]-1| + |v1.uh[2i]-1| = 3 + 5 = 8
    let v0h = [0x0004_0004_0004_0004u64; 16];
    let v1h = [0x0006_0006_0006_0006u64; 16];
    let rth = [0x0001_0001_0001_0001u64; 16];
    let (lo, hi) = run(
        v0h,
        v1h,
        rth,
        OpKind::VRotReduceMulPair {
            dst_lo: mkv(3),
            dst_hi: mkv(4),
            src_lo: mkv(0),
            src_hi: mkv(1),
            src2: mkv(2),
            src_elem: VecElementType::I16,
            rt_elem: VecElementType::I16,
            out_elem: VecElementType::I32,
            imm: 0,
            mode: 1,
            signed1: false,
            signed2: false,
            acc: false,
            abs_diff: true,
        },
    );
    assert_eq!(lo, [0x0000_0006u64 | (0x0000_0006u64 << 32); 16]);
    assert_eq!(hi, [0x0000_0008u64 | (0x0000_0008u64 << 32); 16]);
}
#[test]
fn test_vmulsublane() {
    // vmpyiewuh-like: Vu.w[i] * Vv.uh[2i] (even halfword), low 32. V0 word=3, V1 even-half=5.
    // V1 word = 0x0007_0005 (uh[2i]=5 even, uh[2i+1]=7 odd) -> even pick 5 -> 3*5=15.
    let v0 = [0x0000_0003_0000_0003u64; 16];
    let v1 = [0x0007_0005_0007_0005u64; 16];
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let even = run_vec2(
        v0,
        v1,
        OpKind::VMulSubLane {
            dst: mkv(2),
            src1: mkv(0),
            src2: mkv(1),
            out_elem: VecElementType::I32,
            sub_elem: VecElementType::I16,
            odd: false,
            signed1: true,
            signed2: false,
            acc: false,
        },
    );
    assert_eq!(even, [0x0000_000F_0000_000Fu64; 16]); // 3*5 = 15
    // odd pick: 3 * 7 = 21 = 0x15.
    let odd = run_vec2(
        v0,
        v1,
        OpKind::VMulSubLane {
            dst: mkv(2),
            src1: mkv(0),
            src2: mkv(1),
            out_elem: VecElementType::I32,
            sub_elem: VecElementType::I16,
            odd: true,
            signed1: true,
            signed2: false,
            acc: false,
        },
    );
    assert_eq!(odd, [0x0000_0015_0000_0015u64; 16]); // 3*7 = 21
}
#[test]
fn test_vmulsublanefrac() {
    // vmpyewuh: (Vu.w * Vv.uh[even]) >> 16. Vu.w=0x00100000, Vv.uh[even]=4 -> *4=0x400000 >>16 = 0x40.
    let v0 = [0x0010_0000_0010_0000u64; 16];
    let v1 = [0x0007_0004_0007_0004u64; 16]; // even hw = 0x0004, odd = 0x0007
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let out = run_vec2(
        v0,
        v1,
        OpKind::VMulSubLaneFrac {
            dst: mkv(2),
            src1: mkv(0),
            src2: mkv(1),
            out_elem: VecElementType::I32,
            sub_elem: VecElementType::I16,
            odd: false,
            signed1: true,
            signed2: false,
            shl1: false,
            rnd: false,
            shift: 16,
            sat: false,
            acc: false,
            rnd2: false,
        },
    );
    assert_eq!(out, [0x0000_0040_0000_0040u64; 16]);
}
#[test]
fn test_vmulsublanesh() {
    // vmpyieoh: Vd.w[i] = (Vu.h[even=2i] * Vv.h[odd=2i+1]) << 16, low 32 bits.
    // V0 word = 0x0007_0003 (h[2i]=3, h[2i+1]=7) -> even half of Vu = 3.
    // V1 word = 0x0005_0009 (h[2i]=9, h[2i+1]=5) -> odd  half of Vv = 5.
    // 3 * 5 = 15; 15 << 16 = 0x000F_0000.
    let v0 = [0x0007_0003_0007_0003u64; 16];
    let v1 = [0x0005_0009_0005_0009u64; 16];
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let out = run_vec2(
        v0,
        v1,
        OpKind::VMulSubLaneSh {
            dst: mkv(2),
            src1: mkv(0),
            src2: mkv(1),
            out_elem: VecElementType::I32,
            sub_elem: VecElementType::I16,
            odd1: false,
            odd2: true,
            signed1: true,
            signed2: true,
            shl: 16,
        },
    );
    assert_eq!(out, [0x000F_0000_000F_0000u64; 16]);

    // Signed: Vu even half = -1 (0xFFFF), Vv odd half = 2 -> -2 << 16 = 0xFFFE_0000.
    let v0n = [0x0000_FFFF_0000_FFFFu64; 16];
    let v1n = [0x0002_0000_0002_0000u64; 16];
    let out2 = run_vec2(
        v0n,
        v1n,
        OpKind::VMulSubLaneSh {
            dst: mkv(2),
            src1: mkv(0),
            src2: mkv(1),
            out_elem: VecElementType::I32,
            sub_elem: VecElementType::I16,
            odd1: false,
            odd2: true,
            signed1: true,
            signed2: true,
            shl: 16,
        },
    );
    assert_eq!(out2, [0xFFFE_0000_FFFE_0000u64; 16]);
}
#[test]
fn test_vmulword64pair() {
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    // Helper: write src1=V0, src2=V1, dst pair seed = V3/V4; run; return (V3,V4).
    let run = |v0: [u64; 16],
               v1: [u64; 16],
               seed_lo: [u64; 16],
               seed_hi: [u64; 16],
               op: OpKind|
     -> ([u64; 16], [u64; 16]) {
        let mut ctx = SmirContext::new_hexagon();
        let mut memory = FlatMemory::new(0x1000);
        let interp = SmirInterpreter::new();
        if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
            hex.set_v(0, v0);
            hex.set_v(1, v1);
            hex.set_v(3, seed_lo);
            hex.set_v(4, seed_hi);
        }
        let block = SmirBlock {
            id: BlockId(0),
            guest_pc: 0x1000,
            phis: vec![],
            ops: vec![SmirOp {
                id: OpId(0),
                guest_pc: 0x1000,
                kind: op,
                x86_hint: None,
            }],
            terminator: Terminator::Trap {
                kind: TrapKind::Halt,
            },
            exec_count: 0,
        };
        interp.execute_block(&mut ctx, &mut memory, &block);
        match &ctx.arch_regs {
            ArchRegState::Hexagon(hex) => (hex.get_v(3), hex.get_v(4)),
            _ => panic!("not hexagon"),
        }
    };
    // mode 0 (vmpyewuh_64): Vu.w = 0x0001_0000 (65536), Vv.uh0 = 4.
    //   prod = 65536 * 4 = 262144 = 0x4_0000. hi = prod>>16 = 4; lo = (prod<<16) = 0x0000_0000 (truncated u32).
    let v0 = [0x0001_0000_0001_0000u64; 16];
    let v1 = [0x0000_0004_0000_0004u64; 16]; // uh0 (low half) = 4
    let z = [0u64; 16];
    let (lo, hi) = run(
        v0,
        v1,
        z,
        z,
        OpKind::VMulWord64Pair {
            dst_lo: mkv(3),
            dst_hi: mkv(4),
            src1: mkv(0),
            src2: mkv(1),
            mode: 0,
        },
    );
    assert_eq!(hi, [0x0000_0004_0000_0004u64; 16]);
    assert_eq!(lo, [0x0000_0000_0000_0000u64; 16]);

    // mode 1 (vmpyowh_64_acc): Vu.w = 2, Vv.h1 = 3 (high half), seed_hi.w = 5, seed_lo.w = 0xAAAA_BBBB.
    //   prod = 2*3 + 5 = 11 = 0xB. hi = 0xB>>16 = 0. lo = (0xB & 0xffff)<<16 | (0xAAAA_BBBB>>16 & 0xffff)
    //        = 0x000B_0000 | 0x0000_AAAA = 0x000B_AAAA.
    let v0b = [0x0000_0002_0000_0002u64; 16];
    let v1b = [0x0003_0000_0003_0000u64; 16]; // h1 (high half) = 3
    let slo = [0xAAAA_BBBB_AAAA_BBBBu64; 16];
    let shi = [0x0000_0005_0000_0005u64; 16];
    let (lo1, hi1) = run(
        v0b,
        v1b,
        slo,
        shi,
        OpKind::VMulWord64Pair {
            dst_lo: mkv(3),
            dst_hi: mkv(4),
            src1: mkv(0),
            src2: mkv(1),
            mode: 1,
        },
    );
    assert_eq!(hi1, [0x0000_0000_0000_0000u64; 16]);
    assert_eq!(lo1, [0x000B_AAAA_000B_AAAAu64; 16]);
}
#[test]
fn test_vmulevenwiden() {
    // vmpyuhe: out.uw[i] = Vu.uh[2i] * Vv.uh[2i]. V0 even halfwords = 3, V1 even = 5 -> 15.
    // V0 word = 0x0007_0003 (uh[2i]=3, uh[2i+1]=7); V1 word = 0x0009_0005 (uh[2i]=5).
    let v0 = [0x0007_0003_0007_0003u64; 16];
    let v1 = [0x0009_0005_0009_0005u64; 16];
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let out = run_vec2(
        v0,
        v1,
        OpKind::VMulEvenWiden {
            dst: mkv(2),
            src1: mkv(0),
            src2: mkv(1),
            src_elem: VecElementType::I16,
            signed1: false,
            signed2: false,
            acc: false,
        },
    );
    // each word = even_uh(3) * even_uh(5) = 15 = 0x0000000F.
    assert_eq!(out, [0x0000_000F_0000_000Fu64; 16]);
}
#[test]
fn test_vpack_even_byte() {
    // vpackeb: out.b[i] = V1(=Vv).b[2i] (low half), out.b[i+64] = V0(=Vu).b[2i] (high half).
    // V0 halfwords = 0xAA11 (byte0=0x11), V1 halfwords = 0xBB22 (byte0=0x22).
    // even byte of every half: V1 -> 0x22, V0 -> 0x11.
    let v0 = [0xAA11_AA11_AA11_AA11u64; 16];
    let v1 = [0xBB22_BB22_BB22_BB22u64; 16];
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let out = run_vec2(
        v0,
        v1,
        OpKind::VPack {
            dst: mkv(2),
            src1: mkv(0),
            src2: mkv(1),
            elem: VecElementType::I8,
            odd: false,
        },
    );
    // low 64 bytes (lanes 0..7 u64) = 0x22 everywhere; high 64 bytes = 0x11.
    assert_eq!(out[0], 0x2222_2222_2222_2222u64);
    assert_eq!(out[7], 0x2222_2222_2222_2222u64);
    assert_eq!(out[8], 0x1111_1111_1111_1111u64);
    assert_eq!(out[15], 0x1111_1111_1111_1111u64);
}
#[test]
fn test_vpacksat_hub() {
    // vpackhub_sat: saturate signed halfword -> unsigned byte [0,255].
    // V1 halfword = 0x0140 (320 -> clamps to 255=0xFF); V0 halfword = 0xFF00 (-256 -> 0).
    let v0 = [0xFF00_FF00_FF00_FF00u64; 16];
    let v1 = [0x0140_0140_0140_0140u64; 16];
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let out = run_vec2(
        v0,
        v1,
        OpKind::VPackSat {
            dst: mkv(2),
            src1: mkv(0),
            src2: mkv(1),
            src_elem: VecElementType::I16,
            to_unsigned: true,
            src_lanes: 64,
            block_lanes: 64,
        },
    );
    // low half = sat(V1 halfwords) = 0xFF; high half = sat(V0 halfwords) = 0x00.
    assert_eq!(out[0], 0xFFFF_FFFF_FFFF_FFFFu64);
    assert_eq!(out[7], 0xFFFF_FFFF_FFFF_FFFFu64);
    assert_eq!(out[8], 0x0000_0000_0000_0000u64);
    assert_eq!(out[15], 0x0000_0000_0000_0000u64);
}
#[test]
fn test_vcmptoq_byte_eq() {
    let mut ctx = SmirContext::new_hexagon();
    let mut memory = FlatMemory::new(0x1000);
    let interp = SmirInterpreter::new();
    // V0 byte0 = 0x01, rest 0; V1 all 0. veqb -> byte0 differs (Q bit0=0), all others equal (1).
    let mut v0 = [0u64; 16];
    v0[0] = 0x01;
    if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
        hex.set_v(0, v0);
        hex.set_v(1, [0u64; 16]);
    }
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let block = SmirBlock {
        id: BlockId(0),
        guest_pc: 0x1000,
        phis: vec![],
        ops: vec![SmirOp {
            id: OpId(0),
            guest_pc: 0x1000,
            kind: OpKind::VCmpToQ {
                dst: VReg::Arch(ArchReg::Hexagon(HexagonReg::Q(0))),
                src1: mkv(0),
                src2: mkv(1),
                cond: VecCmpCond::Eq,
                elem: VecElementType::I8,
                lanes: 128,
                accumulate: None,
            },
            x86_hint: None,
        }],
        terminator: Terminator::Trap {
            kind: TrapKind::Halt,
        },
        exec_count: 0,
    };
    interp.execute_block(&mut ctx, &mut memory, &block);
    if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
        let q = hex.get_q(0);
        assert_eq!(q[0], 0xFFFF_FFFF_FFFF_FFFE); // bit0 (byte0) clear, rest set
        assert_eq!(q[1], 0xFFFF_FFFF_FFFF_FFFF); // bytes 64-127 all equal
    }
}
#[test]
fn test_vqfromvandr() {
    // vandvrt: Qd.bit[i] = (V0.byte[i] & V1.byte[i]) != 0.
    // V0 byte0 = 0x01, rest 0; V1 all 0xFF -> only bit0 set.
    let mut v0 = [0u64; 16];
    v0[0] = 0x01;
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let mut ctx = SmirContext::new_hexagon();
    let mut memory = FlatMemory::new(0x1000);
    let interp = SmirInterpreter::new();
    if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
        hex.set_v(0, v0);
        hex.set_v(1, [0xFFFF_FFFF_FFFF_FFFFu64; 16]);
    }
    let block = SmirBlock {
        id: BlockId(0),
        guest_pc: 0x1000,
        phis: vec![],
        ops: vec![SmirOp {
            id: OpId(0),
            guest_pc: 0x1000,
            kind: OpKind::VQFromVAndR {
                dst: VReg::Arch(ArchReg::Hexagon(HexagonReg::Q(0))),
                src1: mkv(0),
                src2: mkv(1),
                oracc: false,
            },
            x86_hint: None,
        }],
        terminator: Terminator::Trap {
            kind: TrapKind::Halt,
        },
        exec_count: 0,
    };
    interp.execute_block(&mut ctx, &mut memory, &block);
    if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
        assert_eq!(hex.get_q(0)[0], 0x1); // only byte 0 -> bit 0
        assert_eq!(hex.get_q(0)[1], 0);
    }
}
#[test]
fn test_vshiftv_halfword() {
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let sv = |kind| OpKind::VShiftV {
        dst: mkv(2),
        src: mkv(0),
        amount: mkv(1),
        elem: VecElementType::I16,
        lanes: 64,
        kind,
    };
    // vasrhv, +2: 0x0100 >> 2 = 0x0040.
    let out = run_vec2(
        [0x0100_0100_0100_0100u64; 16],
        [0x0002_0002_0002_0002u64; 16],
        sv(VShiftVKind::AshiftR),
    );
    assert_eq!(out, [0x0040_0040_0040_0040u64; 16]);
    // vasrhv, amt=30 -> sxtn(30,5) = -2 -> arithmetic LEFT by 2: 0x0100 << 2 = 0x0400.
    let out2 = run_vec2(
        [0x0100_0100_0100_0100u64; 16],
        [0x001E_001E_001E_001Eu64; 16],
        sv(VShiftVKind::AshiftR),
    );
    assert_eq!(out2, [0x0400_0400_0400_0400u64; 16]);
    // vlsrhv, +2: logical right of 0x8000 = 0x2000 (no sign fill).
    let out3 = run_vec2(
        [0x8000_8000_8000_8000u64; 16],
        [0x0002_0002_0002_0002u64; 16],
        sv(VShiftVKind::LshiftR),
    );
    assert_eq!(out3, [0x2000_2000_2000_2000u64; 16]);
}
#[test]
fn test_vlut_byte() {
    // vlutvvb, sel=0 (matchval=0, oh=0): idx=1 (<32, matches group 0) -> out.b[i] = table.b[1*2+0]=table.b[2].
    // Vu all bytes = 1; Vv byte[2] = 0xAB -> out all bytes = 0xAB.
    let v0 = [0x0101_0101_0101_0101u64; 16]; // Vu: idx=1
    let mut v1 = [0u64; 16];
    v1[0] = 0x0000_0000_00AB_0000; // byte 2 = 0xAB
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let out = run_vec2(
        v0,
        v1,
        OpKind::VLut {
            dst: mkv(2),
            src_idx: mkv(0),
            table: mkv(1),
            sel: SrcOperand::Imm(0),
            nomatch: false,
            oracc: false,
        },
    );
    assert_eq!(out, [0xABAB_ABAB_ABAB_ABABu64; 16]);
    // out-of-group idx (>=32) with matchval 0 -> 0.
    let out2 = run_vec2(
        [0x4040_4040_4040_4040u64; 16],
        v1,
        OpKind::VLut {
            dst: mkv(2),
            src_idx: mkv(0),
            table: mkv(1),
            sel: SrcOperand::Imm(0),
            nomatch: false,
            oracc: false,
        },
    );
    assert_eq!(out2, [0u64; 16]); // idx=0x40 -> (0x40 & 0xe0)=0x40 != 0 -> no match -> 0
}
#[test]
fn test_vdealb4w() {
    // Vu words = 0x04030201 (byte0=1, byte2=3); Vv words = 0x08070605 (byte0=5, byte2=7).
    // out: bytes 0-31 = Vv.b0=5, 32-63 = Vv.b2=7, 64-95 = Vu.b0=1, 96-127 = Vu.b2=3.
    let v0 = [0x0403_0201_0403_0201u64; 16]; // Vu
    let v1 = [0x0807_0605_0807_0605u64; 16]; // Vv
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let out = run_vec2(
        v0,
        v1,
        OpKind::VDealB4W {
            dst: mkv(2),
            src1: mkv(0),
            src2: mkv(1),
        },
    );
    assert_eq!(out[0], 0x0505_0505_0505_0505u64); // bytes 0-7 = Vv.b0
    assert_eq!(out[4], 0x0707_0707_0707_0707u64); // bytes 32-39 = Vv.b2
    assert_eq!(out[8], 0x0101_0101_0101_0101u64); // bytes 64-71 = Vu.b0
    assert_eq!(out[12], 0x0303_0303_0303_0303u64); // bytes 96-103 = Vu.b2
}
#[test]
fn test_vcarry_addcarryo() {
    // carryo: V0.w,Q3 = vadd(V1.w,V2.w):carry (cin=0). Lane0: 0xFFFFFFFF +
    // 0x00000001 = 0 with carry-out -> all 4 Q bits of group 0 set.
    let mut v1 = [0u64; 16];
    v1[0] = 0x0000_0001_FFFF_FFFF; // word0=0xFFFFFFFF, word1=1
    let mut v2 = [0u64; 16];
    v2[0] = 0x0000_0000_0000_0001; // word0=1, word1=0
    let mut ctx = SmirContext::new_hexagon();
    let mut memory = FlatMemory::new(0x1000);
    let interp = SmirInterpreter::new();
    if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
        hex.set_v(1, v1);
        hex.set_v(2, v2);
    }
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let block = SmirBlock {
        id: BlockId(0),
        guest_pc: 0x1000,
        phis: vec![],
        ops: vec![SmirOp {
            id: OpId(0),
            guest_pc: 0x1000,
            kind: OpKind::VCarry {
                dst: mkv(0),
                src1: mkv(1),
                src2: mkv(2),
                q_inout: VReg::Arch(ArchReg::Hexagon(HexagonReg::Q(3))),
                sub: false,
                has_cin: false,
                cin0: false,
                has_cout: true,
                sat: false,
            },
            x86_hint: None,
        }],
        terminator: Terminator::Trap {
            kind: TrapKind::Halt,
        },
        exec_count: 0,
    };
    interp.execute_block(&mut ctx, &mut memory, &block);
    if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
        let v = hex.get_v(0);
        assert_eq!(v[0], 0x0000_0001_0000_0000); // word0=0(carry), word1=1+0=1
        let q = hex.get_q(3);
        assert_eq!(q[0] & 0xff, 0x0f); // group0 all set (carry), group1 clear
    }
}
#[test]
fn test_vswap_pair() {
    // Vdd = vswap(Q0, V0, V1): byte0 Q-set -> lo=Vu(V0), hi=Vv(V1);
    // byte1 Q-clear -> lo=Vv(V1), hi=Vu(V0).
    let mut v0 = [0u64; 16];
    v0[0] = 0x0000_0000_0000_1110; // byte0=0x10, byte1=0x11 (Vu)
    let mut v1 = [0u64; 16];
    v1[0] = 0x0000_0000_0000_2120; // byte0=0x20, byte1=0x21 (Vv)
    let mut q = [0u64; 16];
    q[0] = 0b01; // byte0 Q-set
    let mut ctx = SmirContext::new_hexagon();
    let mut memory = FlatMemory::new(0x1000);
    let interp = SmirInterpreter::new();
    if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
        hex.set_v(0, v0);
        hex.set_v(1, v1);
        hex.set_q(0, q);
    }
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let block = SmirBlock {
        id: BlockId(0),
        guest_pc: 0x1000,
        phis: vec![],
        ops: vec![SmirOp {
            id: OpId(0),
            guest_pc: 0x1000,
            kind: OpKind::VSwap {
                dst_lo: mkv(2),
                dst_hi: mkv(3),
                mask_q: VReg::Arch(ArchReg::Hexagon(HexagonReg::Q(0))),
                src1: mkv(0),
                src2: mkv(1),
            },
            x86_hint: None,
        }],
        terminator: Terminator::Trap {
            kind: TrapKind::Halt,
        },
        exec_count: 0,
    };
    interp.execute_block(&mut ctx, &mut memory, &block);
    if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
        // lo: byte0 = Vu(0x10), byte1 = Vv(0x21)
        assert_eq!(hex.get_v(2)[0] & 0xffff, 0x2110);
        // hi: byte0 = Vv(0x20), byte1 = Vu(0x11)
        assert_eq!(hex.get_v(3)[0] & 0xffff, 0x1120);
    }
}
#[test]
fn test_vcondmove_cancel() {
    // if (P0) V0=V1. P0=false -> V0 keeps its prior value (no write).
    let v_old = [0x1111_1111_1111_1111u64; 16];
    let v_new = [0x2222_2222_2222_2222u64; 16];
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let run = |pval: u64, negate: bool| -> [u64; 16] {
        let mut ctx = SmirContext::new_hexagon();
        let mut memory = FlatMemory::new(0x1000);
        let interp = SmirInterpreter::new();
        ctx.write_arch_reg(ArchReg::Hexagon(HexagonReg::P(0)), pval);
        if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
            hex.set_v(0, v_old);
            hex.set_v(1, v_new);
        }
        let block = SmirBlock {
            id: BlockId(0),
            guest_pc: 0x1000,
            phis: vec![],
            ops: vec![SmirOp {
                id: OpId(0),
                guest_pc: 0x1000,
                kind: OpKind::VCondMove {
                    dst_lo: mkv(0),
                    dst_hi: None,
                    src_lo: mkv(1),
                    src_hi: mkv(1),
                    pred: VReg::Arch(ArchReg::Hexagon(HexagonReg::P(0))),
                    negate,
                },
                x86_hint: None,
            }],
            terminator: Terminator::Trap {
                kind: TrapKind::Halt,
            },
            exec_count: 0,
        };
        interp.execute_block(&mut ctx, &mut memory, &block);
        match &ctx.arch_regs {
            ArchRegState::Hexagon(hex) => hex.get_v(0),
            _ => panic!(),
        }
    };
    assert_eq!(run(1, false), v_new); // P0 true -> move
    assert_eq!(run(0, false), v_old); // P0 false -> cancel
    assert_eq!(run(0, true), v_new); // !P0 (P0 false) -> move
    assert_eq!(run(1, true), v_old); // !P0 (P0 true) -> cancel
}
#[test]
fn test_vprefixqb() {
    // V0.b = prefixsum(Q0): byte i = count of set Q bits in bytes 0..=i.
    // Q0 bits: byte0 set, byte2 set -> prefix b0=1,b1=1,b2=2,b3=2,...
    let mut q = [0u64; 16];
    q[0] = 0b0101; // bits 0 and 2 set
    let mut ctx = SmirContext::new_hexagon();
    let mut memory = FlatMemory::new(0x1000);
    let interp = SmirInterpreter::new();
    if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
        hex.set_q(0, q);
    }
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let block = SmirBlock {
        id: BlockId(0),
        guest_pc: 0x1000,
        phis: vec![],
        ops: vec![SmirOp {
            id: OpId(0),
            guest_pc: 0x1000,
            kind: OpKind::VPrefixSumQ {
                dst: mkv(0),
                mask_q: VReg::Arch(ArchReg::Hexagon(HexagonReg::Q(0))),
                elem: VecElementType::I8,
                lanes: 128,
            },
            x86_hint: None,
        }],
        terminator: Terminator::Trap {
            kind: TrapKind::Halt,
        },
        exec_count: 0,
    };
    interp.execute_block(&mut ctx, &mut memory, &block);
    if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
        let v = hex.get_v(0);
        // bytes: b0=1, b1=1, b2=2, b3=2 -> word0 low = 0x02020101
        assert_eq!(v[0] & 0xffff_ffff, 0x0202_0101);
    }
}
#[test]
fn test_vrotr() {
    // Vd.uw[i] = rotate_right(Vu.uw[i], amt&0x1f). Vu word = 0x0000_0001,
    // amt = 4 -> rotate_right(1,4) = 0x1000_0000.
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let out = run_vec2(
        [0x0000_0001_0000_0001u64; 16],
        [0x0000_0004_0000_0004u64; 16],
        OpKind::VRotr {
            dst: mkv(2),
            src: mkv(0),
            amount: mkv(1),
        },
    );
    assert_eq!(out, [0x1000_0000_1000_0000u64; 16]);
}
#[test]
fn test_vaddsub_mixed_sat() {
    // vaddububb_sat: ub + b:sat. 0xFF + (+1) -> saturate to 0xFF.
    // 0x01 + (-2 = 0xFE) -> -1 -> saturate to 0. Use byte pattern u=0xFF01..,
    // v=0x01FE.. -> bytes alternate.
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let out = run_vec2(
        [0x0000_0000_0000_01FFu64; 16],
        [0x0000_0000_0000_FE01u64; 16],
        OpKind::VAddSubMixedSat {
            dst: mkv(2),
            src1: mkv(0),
            src2: mkv(1),
            sub: false,
        },
    );
    // byte0: 0xFF + 1 = 256 -> 255 (0xFF); byte1: 0x01 + (-2) = -1 -> 0.
    assert_eq!(out[0] & 0xffff, 0x00FF);
}
#[test]
fn test_vsetq() {
    // vsetq(5): low 5 bits set -> 0x1F.
    let mkv = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::V(n)));
    let mkq = |n| VReg::Arch(ArchReg::Hexagon(HexagonReg::Q(n)));
    let mut ctx = SmirContext::new_hexagon();
    let mut memory = FlatMemory::new(0x1000);
    let interp = SmirInterpreter::new();
    ctx.write_arch_reg(ArchReg::Hexagon(HexagonReg::R(5)), 5);
    let block = SmirBlock {
        id: BlockId(0),
        guest_pc: 0x1000,
        phis: vec![],
        ops: vec![SmirOp {
            id: OpId(0),
            guest_pc: 0x1000,
            kind: OpKind::VSetPredQ {
                dst: mkq(0),
                scalar: VReg::Arch(ArchReg::Hexagon(HexagonReg::R(5))),
                v2: false,
            },
            x86_hint: None,
        }],
        terminator: Terminator::Trap {
            kind: TrapKind::Halt,
        },
        exec_count: 0,
    };
    interp.execute_block(&mut ctx, &mut memory, &block);
    let _ = mkv;
    if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
        assert_eq!(hex.get_q(0)[0], 0x1F);
    }
}
#[test]
fn test_vhist() {
    // vhist over the WHOLE V file: input = 128 bytes all = 10. For each of the
    // 8 lanes and each of its 16 bytes, value=10 -> regno=10>>3=1, element=
    // 10&7=2, idx=8*lane+2; V1.uh[idx] += 1. So V1.uh[8*lane+2] = 16 for each
    // lane (16 identical bytes per lane), all other uh = 0, and V0/V2.. stay 0.
    let interp = SmirInterpreter::new();
    let mut ctx = SmirContext::new_hexagon();
    // Seed the whole V file to zero so the bins start clean.
    if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
        for n in 0..32u8 {
            hex.set_v(n, [0u64; 16]);
        }
    }
    ctx.write_arch_reg(ArchReg::Hexagon(HexagonReg::R(0)), 0x200);
    let mut memory = FlatMemory::new(0x1000);
    memory.load(0x200, &[10u8; 128]);

    let block = SmirBlock {
        id: BlockId(0),
        guest_pc: 0x1000,
        phis: vec![],
        ops: vec![SmirOp {
            id: OpId(0),
            guest_pc: 0x1000,
            kind: OpKind::VHist {
                input: Address::BaseOffset {
                    base: VReg::Arch(ArchReg::Hexagon(HexagonReg::R(0))),
                    offset: 0,
                    disp_size: DispSize::Auto,
                },
                aligned: true,
                mask_q: VReg::Arch(ArchReg::Hexagon(HexagonReg::Q(0))),
                use_q: false,
                imm_match: None,
                sat: false,
                kind: 0, // vhist
            },
            x86_hint: None,
        }],
        terminator: Terminator::Trap {
            kind: TrapKind::Halt,
        },
        exec_count: 0,
    };
    interp.execute_block(&mut ctx, &mut memory, &block);

    if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
        let v1 = hex.get_v(1);
        // Helper: read uh[i] from V1's 16 u64 lanes (little-endian, 2 bytes).
        let uh = |i: usize| -> u32 {
            let byte = i * 2;
            ((v1[byte / 8] >> ((byte % 8) * 8)) & 0xffff) as u32
        };
        // V1.uh[8*lane+2] = 16 for every lane; the +0 slots stay 0.
        for lane in 0..8usize {
            assert_eq!(uh(8 * lane + 2), 16, "V1.uh[{}]", 8 * lane + 2);
            assert_eq!(uh(8 * lane), 0, "V1.uh[{}]", 8 * lane);
        }
        // V0 and V2 are untouched bin registers -> all zero.
        assert_eq!(hex.get_v(0), [0u64; 16]);
        assert_eq!(hex.get_v(2), [0u64; 16]);
    }
}
#[test]
fn test_vwhist256_sat() {
    // vwhist256:sat over the whole V file. Input = 64 halfwords, each
    // bucket=0x08, weight=0xFF. bucket>>3 = 1 -> vindex=1; bucket&7=0 ->
    // elindex = (i & !7). Seed V1.uh[*] high so the unsigned weight add
    // saturates to 0xffff instead of wrapping.
    let interp = SmirInterpreter::new();
    let mut ctx = SmirContext::new_hexagon();
    if let ArchRegState::Hexagon(hex) = &mut ctx.arch_regs {
        for n in 0..32u8 {
            hex.set_v(n, [0u64; 16]);
        }
        // Set every halfword of V1 to 0xFF00 so +0xFF saturates at 0xFFFF.
        hex.set_v(1, [0xFF00_FF00_FF00_FF00u64; 16]);
    }
    ctx.write_arch_reg(ArchReg::Hexagon(HexagonReg::R(0)), 0x200);
    let mut memory = FlatMemory::new(0x1000);
    let mut input = [0u8; 128];
    for i in 0..64usize {
        input[2 * i] = 0x08; // bucket
        input[2 * i + 1] = 0xFF; // weight
    }
    memory.load(0x200, &input);

    let block = SmirBlock {
        id: BlockId(0),
        guest_pc: 0x1000,
        phis: vec![],
        ops: vec![SmirOp {
            id: OpId(0),
            guest_pc: 0x1000,
            kind: OpKind::VHist {
                input: Address::BaseOffset {
                    base: VReg::Arch(ArchReg::Hexagon(HexagonReg::R(0))),
                    offset: 0,
                    disp_size: DispSize::Auto,
                },
                aligned: true,
                mask_q: VReg::Arch(ArchReg::Hexagon(HexagonReg::Q(0))),
                use_q: false,
                imm_match: None,
                sat: true,
                kind: 2, // vwhist256
            },
            x86_hint: None,
        }],
        terminator: Terminator::Trap {
            kind: TrapKind::Halt,
        },
        exec_count: 0,
    };
    interp.execute_block(&mut ctx, &mut memory, &block);

    if let ArchRegState::Hexagon(hex) = &ctx.arch_regs {
        let v1 = hex.get_v(1);
        let uh = |i: usize| -> u32 {
            let byte = i * 2;
            ((v1[byte / 8] >> ((byte % 8) * 8)) & 0xffff) as u32
        };
        // elindex = i & !7 for i in 0..64 -> the touched bins are
        // {0,8,16,...,56}; each is hit 8 times (i in [base, base+7]).
        // 0xFF00 + 0xFF would be 0xFFFF; further adds saturate at 0xFFFF.
        for base in (0..64).step_by(8) {
            assert_eq!(uh(base), 0xFFFF, "V1.uh[{base}] saturated");
        }
        // A bin that is never selected keeps its seed 0xFF00.
        assert_eq!(uh(1), 0xFF00);
    }
}
#[test]
fn test_bidir_shift_bit_exact() {
    // Exercise every count in [-64, 63] for several source patterns and
    // all four kinds, vs the verbatim sem reference. The interp masks the
    // 32-bit result; ref_bidir32 returns u32 so compare the low 32 bits.
    let srcs32: [u32; 6] = [
        0x0000_0001,
        0x8000_0000,
        0x4000_0000,
        0xffff_ffff,
        0x1234_5678,
        0xdead_beef,
    ];
    for &src in &srcs32 {
        for shamt in -64i32..=63 {
            // Encode shamt into the low 7 bits of Rt; the upper bits of Rt
            // must be ignored (sxtn7 only looks at bits 6:0).
            let rt = ((shamt as u32) & 0x7f) | 0x5a5a_5a00;
            for kind in 0u8..=3 {
                let got = run_bidir(src as u64, rt, kind, OpWidth::W32) as u32;
                let want = ref_bidir32(src, shamt, kind);
                assert_eq!(
                    got, want,
                    "W32 src={src:#x} shamt={shamt} kind={kind}: got {got:#x} want {want:#x}"
                );
            }
        }
    }

    let srcs64: [u64; 5] = [
        0x0000_0000_0000_0001,
        0x8000_0000_0000_0000,
        0xffff_ffff_ffff_ffff,
        0x0123_4567_89ab_cdef,
        0xdead_beef_cafe_babe,
    ];
    for &src in &srcs64 {
        for shamt in -64i32..=63 {
            let rt = (shamt as u32) & 0x7f;
            for kind in 0u8..=3 {
                let got = run_bidir(src, rt, kind, OpWidth::W64);
                let want = ref_bidir64(src, shamt, kind);
                assert_eq!(
                    got, want,
                    "W64 src={src:#x} shamt={shamt} kind={kind}: got {got:#x} want {want:#x}"
                );
            }
        }
    }

    // Immediate-source form (S4_lsli pattern): logical-left bidir of a const.
    assert_eq!(run_bidir(1, 4, 2, OpWidth::W32), 16);
    assert_eq!(run_bidir(1, (-1i32 as u32) & 0x7f, 2, OpWidth::W32), 0);
}
#[test]
fn test_sat_n_clamp_and_ovf() {
    // ---- signed 32-bit (A2_sat/addsat/...): clamp to [i32::MIN, i32::MAX] ----
    // in range -> no clamp, no OVF.
    assert_eq!(run_sat_n(0x1234, 32, true, true), (0x1234, false));
    assert_eq!(run_sat_n(-5, 32, true, true), (0xFFFF_FFFB, false));
    // clamp high -> i32::MAX, OVF set.
    assert_eq!(run_sat_n(0x8000_0000, 32, true, true), (0x7FFF_FFFF, true));
    // clamp low -> i32::MIN, OVF set.
    assert_eq!(
        run_sat_n(-(1i64 << 31) - 1, 32, true, true),
        (0x8000_0000, true)
    );
    // boundary values exactly representable -> no clamp.
    assert_eq!(
        run_sat_n(i32::MAX as i64, 32, true, true),
        (0x7FFF_FFFF, false)
    );
    assert_eq!(
        run_sat_n(i32::MIN as i64, 32, true, true),
        (0x8000_0000, false)
    );

    // ---- signed 8-bit (A2_satb): clamp to [-128, 127] ----
    assert_eq!(run_sat_n(100, 8, true, true), (100, false));
    assert_eq!(run_sat_n(200, 8, true, true), (127, true)); // clamp high
    assert_eq!(run_sat_n(-200, 8, true, true), (0xFFFF_FF80, true)); // clamp low -> -128 low bits
    assert_eq!(run_sat_n(-1, 8, true, true), (0xFFFF_FFFF, false)); // -1 fits, sign-extended

    // ---- signed 16-bit (A2_sath) ----
    assert_eq!(run_sat_n(0x4000, 16, true, true), (0x4000, false));
    assert_eq!(run_sat_n(0x8000, 16, true, true), (0x7FFF, true)); // clamp high
    assert_eq!(run_sat_n(-0x8001, 16, true, true), (0xFFFF_8000, true)); // clamp low

    // ---- unsigned 8-bit (A2_satub): clamp to [0, 255] ----
    assert_eq!(run_sat_n(200, 8, false, true), (200, false));
    assert_eq!(run_sat_n(300, 8, false, true), (255, true)); // clamp high
    assert_eq!(run_sat_n(-1, 8, false, true), (0, true)); // negative clamps to 0, OVF

    // ---- unsigned 16-bit (A2_satuh) ----
    assert_eq!(run_sat_n(0x1234, 16, false, true), (0x1234, false));
    assert_eq!(run_sat_n(0x1_0000, 16, false, true), (0xFFFF, true)); // clamp high
    assert_eq!(run_sat_n(-5, 16, false, true), (0, true)); // negative -> 0, OVF

    // ---- set_ovf = false: value still clamps, but USR:OVF is NOT set ----
    assert_eq!(
        run_sat_n(0x8000_0000, 32, true, false),
        (0x7FFF_FFFF, false)
    );
    assert_eq!(run_sat_n(-1, 8, false, false), (0, false));
}
// Regression for issue #108: SatN's USR:OVF sticky update is a side effect that
// dests() does not report, so DCE used to drop a saturating op whose data
// result was dead — silently losing the OVF flag. Here the clamp of 0x8000 to
// signed 16 bits overflows and must OR USR:OVF, but its result is written to a
// virtual temp that is never read. After running the FULL optimizer the op (and
// its OVF side effect) must survive end-to-end through the interpreter.
#[test]
fn issue_108_optimized_satn_keeps_usr_ovf_when_result_dead() {
    use crate::smir::optimize::{OptLevel, optimize_function};

    let mut ctx = SmirContext::new_hexagon();
    let mut memory = FlatMemory::new(0x1000);
    ctx.write_arch_reg(ArchReg::Hexagon(HexagonReg::Usr), 0);

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    let tmp = builder.alloc_vreg();
    let dead = builder.alloc_vreg();
    builder.push_op(
        0x1000,
        OpKind::Mov {
            dst: tmp,
            src: SrcOperand::Imm(0x8000),
            width: OpWidth::W64,
        },
    );
    builder.push_op(
        0x1004,
        OpKind::SatN {
            dst: dead,
            src: SrcOperand::Reg(tmp),
            sat_bits: 16,
            signed: true,
            set_ovf: true,
            width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut func = builder.finish();
    optimize_function(&mut func, OptLevel::O2);

    let interp = SmirInterpreter::new();
    interp.execute_block(&mut ctx, &mut memory, &func.blocks[0]);

    assert_eq!(
        ctx.read_arch_reg(ArchReg::Hexagon(HexagonReg::Usr)) & 1,
        1,
        "optimized SatN must still set USR:OVF even though its data result is dead",
    );
}
#[test]
fn clmul_64x64_produces_exact_128_bit_polynomial_product() {
    assert_eq!(run_clmul64(1, u64::MAX, false, (0, 0)), (u64::MAX, 0));
    assert_eq!(
        run_clmul64(0xFEDC_BA98_7654_3210, 0x0123_4567_89AB_CDEF, false, (0, 0),),
        (0x40A0_7898_28C8_10F0, 0x00E0_38D8_6888_50B0)
    );
    assert_eq!(
        run_clmul64(u64::MAX, u64::MAX, false, (0, 0)),
        (0x5555_5555_5555_5555, 0x5555_5555_5555_5555)
    );
    assert_eq!(
        run_clmul64(
            0xFEDC_BA98_7654_3210,
            0x0123_4567_89AB_CDEF,
            true,
            (u64::MAX, 0xA5A5_A5A5_A5A5_A5A5),
        ),
        (0xBF5F_8767_D737_EF0F, 0xA545_9D7D_CD2D_F515)
    );
}
#[test]
fn crc32c_primitive_matches_castagnoli_known_answers_and_widths() {
    assert_eq!(run_crc32c(0, 0, OpWidth::W8), 0);
    assert_eq!(run_crc32c(u64::MAX, 0x31, OpWidth::W8), 0x6F0A_661C);
    assert_eq!(
        run_crc32c(u64::MAX, 0x3837_3635_3433_3231, OpWidth::W64),
        0x9F78_7F65
    );
    assert_eq!(run_crc32c(0x9F78_7F65, 0x39, OpWidth::W8), 0x1CF9_6D7C);
    // Complementing the raw state yields the standard CRC-32C check value
    // E3069283 for ASCII "123456789".
    assert_eq!(
        !run_crc32c(0x9F78_7F65, 0x39, OpWidth::W8) as u32,
        0xE306_9283
    );
    assert_eq!(run_crc32c(0x1234_5678, 0xABCD, OpWidth::W16), 0xAAE3_2043);
    assert_eq!(
        run_crc32c(0x89AB_CDEF, 0x0123_4567, OpWidth::W32),
        0x796A_B9A9
    );
    assert_eq!(
        run_crc32c(0xFFFF_FFFF_DEAD_BEEF, 0x0123_4567_89AB_CDEF, OpWidth::W64),
        0x3AB0_1437
    );
}
#[test]
fn test_clmul_pmpyw_and_vpmpyh() {
    // pmpyw: carry-less 32x32 -> 64; 1 * x = x (identity), no high bits.
    assert_eq!(
        run_clmul(1, 0x1234_5678, 32, 1, false, (0, 0)),
        (0x1234_5678, 0)
    );
    // x<<1 via b=2: shift, still carry-less.
    assert_eq!(
        run_clmul(0x1234_5678, 2, 32, 1, false, (0, 0)),
        (0x2468_ACF0, 0)
    );
    // High word appears when products exceed 32 bits.
    // 0x80000000 * 0x80000000 carry-less = bit62 set -> hi = 0x40000000.
    assert_eq!(
        run_clmul(0x8000_0000, 0x8000_0000, 32, 1, false, (0, 0)),
        (0, 0x4000_0000)
    );
    // _acc XORs into the existing pair.
    let base = run_clmul(0x1234_5678, 2, 32, 1, false, (0, 0));
    assert_eq!(
        run_clmul(0x1234_5678, 2, 32, 1, true, (0xAAAA_AAAA, 0x5555_5555)),
        (base.0 ^ 0xAAAA_AAAA, base.1 ^ 0x5555_5555)
    );

    // vpmpyh: two independent 16x16 carry-less products, interleaved.
    // lane0: 0xffff * 0x0002 ; lane1: 0x0001 * 0x0003.
    // Build inputs: a.h0=0xffff,a.h1=0x0001 ; b.h0=0x0002,b.h1=0x0003.
    let a = 0x0001_ffffu32;
    let b = 0x0003_0002u32;
    // lane0 = clmul(0xffff,2,16) = 0x1_fffe (carry-less: x<<1).
    // lane1 = clmul(1,3,16) = 0x0003.
    // dst.h0 = lane0.lo = 0xfffe, dst.h1 = lane1.lo = 0x0003.
    // hi.h0  = lane0.hi = 0x0001, hi.h1  = lane1.hi = 0x0000.
    assert_eq!(
        run_clmul(a, b, 16, 2, false, (0, 0)),
        (0x0003_fffe, 0x0000_0001)
    );
}
#[test]
fn test_cmpy_w128_sat_worst_case() {
    // Worst case: all words = 0x80000000 (= i32::MIN). cmpyrw is SUB (add=false,
    // w=0,0,1,1): term0 = w0*w1, term1 = w2*w3, acc = term0 - term1 = 0.
    // For the saturation extreme use the ADD form (cmpyrwc, add=true):
    //   acc = (MIN*MIN) + (MIN*MIN) = 2 * 2^62 = 2^63; >>31 = 2^32 = 0x1_0000_0000
    //   -> sat to i32::MAX with OVF.
    let min = 0x8000_0000u32 as i32; // -2^31
    assert_eq!(min as i64 * min as i64, 1i64 << 62);
    let rss = 0x8000_0000_8000_0000u64; // both words = i32::MIN
    let rtt = 0x8000_0000_8000_0000u64;
    assert_eq!(
        run_wcmpy(rss, rtt, (0, 0, 1, 1), true, false),
        (0x7FFF_FFFF, true)
    );

    // Real part (cmpyrw): SUB of identical terms -> 0, no saturation.
    assert_eq!(run_wcmpy(rss, rtt, (0, 0, 1, 1), false, false), (0, false));

    // Small in-range value: Rss.w = (3, 0), Rtt.w = (5, 0); cmpyiw = ADD,
    // w=0,1,1,0: term0 = w0*w1 = 3*0 = 0; term1 = w2*w3 = 0*5 = 0 -> 0.
    let rss2 = 0x0000_0000_0000_0003u64; // w0=3, w1(=w of rss[1])=0
    let rtt2 = 0x0000_0000_0000_0005u64;
    assert_eq!(run_wcmpy(rss2, rtt2, (0, 1, 1, 0), true, false), (0, false));

    // :rnd adds 0x40000000 before the >>31. Pick acc=0 so result = 0x40000000>>31 = 0.
    assert_eq!(run_wcmpy(0, 0, (0, 0, 1, 1), true, true), (0, false));
}
#[test]
fn test_sat_orig_shl_sweep_and_special() {
    let srcs: [u32; 7] = [
        0x0000_0001,
        0x7fff_ffff,
        0x8000_0000,
        0xffff_ffff,
        0x4000_0000,
        0x0000_0000,
        0x1234_5678,
    ];
    for &src in &srcs {
        for sh in -40i32..=40 {
            for &right in &[false, true] {
                let got = run_sat_orig_shl(src, sh, right);
                // sh in [-40,40] round-trips through sxtn7 unchanged.
                let want = ref_sat_orig_shl(src, sh, right);
                assert_eq!(
                    got, want,
                    "src={src:#x} sh={sh} right={right}: got {got:?} want {want:?}"
                );
            }
        }
    }
    // Special case: orig>0 && shifted==0 -> INT_MAX + OVF.
    // asl with a positive small value shifted left by 32 (sh masked to 0..63):
    // sh=32 -> orig<<32 truncated... but a is i64 so orig<<32 != 0; instead use
    // the documented case: positive orig, shift result 0 only via amount that
    // produces a==0 — i.e. a left shift of a positive value can't be 0 unless
    // orig is 0. The INT_MAX-from-0 path triggers for asr with negative count
    // where (orig << k) overflows i64 to exactly 0 is impossible; the realistic
    // trigger is the sign-flip path, swept above. Confirm a sign-flip directly:
    // 0x4000_0000 (positive) << 1 = 0x8000_0000 -> sign flips negative -> sat to
    // INT_MAX + OVF.
    assert_eq!(run_sat_orig_shl(0x4000_0000, 1, false), (0x7FFF_FFFF, true));
    // Negative value left-shifted past sign: 0x8000_0000 (INT_MIN) << 1 overflows
    // to 0 in 32 bits but i64 keeps -2^32 (negative) -> stays negative, sat to
    // INT_MIN + OVF.
    assert_eq!(run_sat_orig_shl(0x8000_0000, 1, false), (0x8000_0000, true));
}
#[test]
fn issue_21_cmpxchg32_match_preserves_rax_high() {
    // EAX(5) == ECX(5) → match: ECX takes EDX (zero-extended), EAX UNCHANGED.
    let (rax, rcx) = run_cmpxchg32(
        0xDEAD_BEEF_0000_0005,
        0xAAAA_0000_0000_0005,
        0xBBBB_0000_0000_0099,
    );
    assert_eq!(
        rax, 0xDEAD_BEEF_0000_0005,
        "RAX upper 32 bits must be preserved on a successful CMPXCHG",
    );
    assert_eq!(
        rcx, 0x0000_0000_0000_0099,
        "ECX takes the source on a match"
    );
}
#[test]
fn issue_21_cmpxchg32_mismatch_preserves_dst_high() {
    // EAX(5) != ECX(7) → mismatch: EAX takes ECX (zero-extended), ECX UNCHANGED.
    let (rax, rcx) = run_cmpxchg32(0x1111_0000_0000_0005, 0xDEAD_BEEF_0000_0007, 0);
    assert_eq!(
        rcx, 0xDEAD_BEEF_0000_0007,
        "destination upper 32 bits must be preserved on a failed CMPXCHG",
    );
    assert_eq!(
        rax, 0x0000_0000_0000_0007,
        "EAX takes the old destination on a mismatch",
    );
}
#[test]
fn cas_pair_failure_has_no_writeback_and_success_is_fault_precise() {
    let addr = VReg::Arch(ArchReg::RiscV(RiscVReg::X(10)));
    let lo = VReg::Arch(ArchReg::RiscV(RiscVReg::X(6)));
    let hi = VReg::Arch(ArchReg::RiscV(RiscVReg::X(7)));
    let new_lo = VReg::Arch(ArchReg::RiscV(RiscVReg::X(8)));
    let new_hi = VReg::Arch(ArchReg::RiscV(RiscVReg::X(9)));
    let success = VReg::Arch(ArchReg::RiscV(RiscVReg::X(11)));
    let old = [0x0123_4567_89ab_cdefu64, 0xfedc_ba98_7654_3210u64];
    let replacement = [0x1111_2222_3333_4444u64, 0x5555_6666_7777_8888u64];
    let mut inner = FlatMemory::new(0x100);
    inner.write(0x20, &old[0].to_le_bytes()).unwrap();
    inner.write(0x28, &old[1].to_le_bytes()).unwrap();
    let mut memory = StoreFaultMemory {
        inner,
        stores_before_fault: 0,
    };
    let mut ctx = SmirContext::new_riscv();
    ctx.write_vreg(addr, 0x20);
    ctx.write_vreg(lo, !old[0]);
    ctx.write_vreg(hi, old[1]);
    ctx.write_vreg(new_lo, replacement[0]);
    ctx.write_vreg(new_hi, replacement[1]);
    ctx.write_vreg(success, u64::MAX);
    let op = SmirOp::new(
        OpId(0),
        0x1000,
        OpKind::CasPair {
            dst_lo: lo,
            dst_hi: hi,
            success,
            addr: Address::Direct(addr),
            expected_lo: lo,
            expected_hi: hi,
            new_lo,
            new_hi,
            order: MemoryOrder::SeqCst,
            failure_order: MemoryOrder::Acquire,
        },
    );

    SmirInterpreter::new()
        .execute_op(&mut ctx, &mut memory, &op)
        .expect("comparison failure must not attempt a write");
    assert_eq!([ctx.read_vreg(lo), ctx.read_vreg(hi)], old);
    assert_eq!(ctx.read_vreg(success), 0);

    ctx.write_vreg(success, u64::MAX);
    let error = SmirInterpreter::new()
        .execute_op(&mut ctx, &mut memory, &op)
        .expect_err("successful comparison must surface the store fault");
    assert!(matches!(error, MemoryError::PageFault { write: true, .. }));
    assert_eq!([ctx.read_vreg(lo), ctx.read_vreg(hi)], old);
    assert_eq!(ctx.read_vreg(success), u64::MAX);
    let mut bytes = [0u8; 16];
    memory.inner.read(0x20, &mut bytes).unwrap();
    assert_eq!(bytes[..8], old[0].to_le_bytes());
    assert_eq!(bytes[8..], old[1].to_le_bytes());
}
#[test]
fn lifted_vtestps_vtestpd_execute_sign_only_truth_table_and_faults() {
    fn vector(signs: u8, elem_bytes: usize, lanes: usize) -> VecValue {
        let mut raw = [0u8; 128];
        for lane in 0..lanes {
            let chunk = &mut raw[lane * elem_bytes..(lane + 1) * elem_bytes];
            chunk.fill(0x7F);
            if signs & (1 << lane) != 0 {
                chunk[elem_bytes - 1] |= 0x80;
            }
        }
        let mut value = [0u64; 16];
        for (word, chunk) in value.iter_mut().zip(raw.chunks_exact(8)) {
            *word = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        value
    }

    fn expected(before: u64, first: u8, second: u8, lanes: usize) -> u64 {
        let lanes_mask = ((1u16 << lanes) - 1) as u8;
        let zf = first & second & lanes_mask == 0;
        let cf = (!first) & second & lanes_mask == 0;
        (before & !0x8D5) | u64::from(cf) | (u64::from(zf) << 6)
    }

    let flags_before = 0xCD7;
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x80);
    for (first, second) in [(0, 0), (0xF, 0xF), (0, 0xF), (0x5, 0xF)] {
        let first_state = vector(first, 4, 4);
        let second_state = vector(second, 4, 4);
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[2] = first_state;
            x86.xmm[1] = second_state;
        }
        ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
        ctx.flags.lazy = None;
        assert!(matches!(
            execute_lifted_x86(&[0xC4, 0xE2, 0x79, 0x0E, 0xD1], &mut ctx, &mut memory,),
            BlockResult::Exit(ExitReason::Halt)
        ));
        ctx.flags.materialize_all();
        assert_eq!(
            ctx.flags.materialized.to_rflags(),
            expected(flags_before, first, second, 4),
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(x86.xmm[2], first_state);
            assert_eq!(x86.xmm[1], second_state);
        }
    }

    let first = vector(0x5, 8, 4);
    let second = vector(0xC, 8, 4);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[10] = first;
        x86.xmm[9] = second;
    }
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;
    execute_lifted_x86(&[0xC4, 0x42, 0x7D, 0x0F, 0xD1], &mut ctx, &mut memory);
    ctx.flags.materialize_all();
    assert_eq!(
        ctx.flags.materialized.to_rflags(),
        expected(flags_before, 0x5, 0xC, 4),
    );

    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let memory_source = vector(0xA5, 4, 8);
    let memory_bytes = memory_source
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .take(32)
        .collect::<Vec<_>>();
    memory.write(0x21, &memory_bytes).unwrap();
    ctx.write_vreg(rax, 1);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = vector(0x3C, 4, 8);
    }
    assert!(matches!(
        execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x0E, 0x58, 0x20], &mut ctx, &mut memory,),
        BlockResult::Exit(ExitReason::Halt)
    ));

    ctx.write_vreg(rax, 0x70);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;
    let fault = execute_lifted_x86(&[0xC4, 0xE2, 0x7D, 0x0E, 0x58, 0x20], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_phminposuw_executes_unsigned_ties_aliases_alignment_and_faults() {
    fn packed_words(values: &[u16], fill: u64) -> VecValue {
        let mut out = [fill; 16];
        out[0] = 0;
        out[1] = 0;
        for (lane, value) in values.iter().copied().enumerate() {
            out[lane / 4] |= u64::from(value) << ((lane % 4) * 16);
        }
        out
    }
    fn expected(values: &[u16; 8]) -> u64 {
        let (index, minimum) = values
            .iter()
            .copied()
            .enumerate()
            .min_by_key(|(_, value)| *value)
            .unwrap();
        u64::from(minimum) | ((index as u64) << 16)
    }

    let ties: [u16; 8] = [500, 0x8000, 7, 7, 0xFFFF, 7, 8, 7];
    let lane7: [u16; 8] = [0xFFFF, 900, 800, 700, 600, 500, 400, 0];
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let flags_before = 0xCD7;
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    let tie_bytes = ties
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
    let lane7_bytes = lane7
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect::<Vec<_>>();
    memory.write(0x100, &tie_bytes).unwrap();
    memory.write(0x121, &lane7_bytes).unwrap();
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    // Equal minima retain the first index, unsigned 0x8000/0xFFFF remain
    // greater than 7, and legacy form preserves state above bit 127.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = [upper; 16];
        x86.xmm[2] = packed_words(&ties, 0);
    }
    execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x41, 0xCA], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1][0], expected(&ties));
        assert_eq!(x86.xmm[1][1], 0);
        assert!(x86.xmm[1][2..].iter().all(|word| *word == upper));
    }

    // Source/destination aliasing must capture every input word before the
    // architectural write, including a minimum in the final lane.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = packed_words(&lane7, upper);
    }
    execute_lifted_x86(&[0x66, 0x0F, 0x38, 0x41, 0xC0], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0][0], expected(&lane7));
        assert_eq!(x86.xmm[0][1], 0);
        assert!(x86.xmm[0][2..].iter().all(|word| *word == upper));
    }

    // VEX high-register form zeros all state above bit 127.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[9] = sentinel;
        x86.xmm[10] = packed_words(&ties, 0);
    }
    execute_lifted_x86(&[0xC4, 0x42, 0x79, 0x41, 0xCA], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[9][0], expected(&ties));
        assert!(x86.xmm[9][1..].iter().all(|word| *word == 0));
    }

    // The VEX memory form explicitly accepts an unaligned m128.
    ctx.write_vreg(rax, 0x121);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[9] = sentinel;
    }
    execute_lifted_x86(&[0xC4, 0x62, 0x79, 0x41, 0x48, 0x00], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[9][0], expected(&lane7));
        assert!(x86.xmm[9][1..].iter().all(|word| *word == 0));
    }

    // The same misalignment is a legacy #GP(0), preceding both the load and
    // any destination or flag modification.
    ctx.write_vreg(rax, 0x121);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[9] = sentinel;
    }
    let misaligned =
        execute_lifted_x86(&[0x66, 0x44, 0x0F, 0x38, 0x41, 0x08], &mut ctx, &mut memory);
    assert!(matches!(
        misaligned,
        BlockResult::Exit(ExitReason::GeneralProtection { error_code: 0, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[9], sentinel);
    }

    // An unaligned-capable VEX access still faults atomically when the full
    // 16-byte memory operand is unavailable.
    ctx.write_vreg(rax, 0x3F8);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[9] = sentinel;
    }
    let fault = execute_lifted_x86(&[0xC4, 0x62, 0x79, 0x41, 0x08], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[9], sentinel);
    }

    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_0f3a_inserts_execute_merges_masks_aliases_tuples_faults_and_flags() {
    fn vector(bytes: &[u8], fill: u64) -> VecValue {
        let mut out = [fill; 16];
        for (word, chunk) in bytes.chunks_exact(8).enumerate() {
            out[word] = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        out
    }
    fn bytes(value: &VecValue, len: usize) -> Vec<u8> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(len)
            .collect()
    }

    let merge = (0..16)
        .map(|lane| (lane * 17 + 3) as u8)
        .collect::<Vec<_>>();
    let second = (0..16)
        .map(|lane| (0xF1u16.wrapping_sub((lane * 11) as u16)) as u8)
        .collect::<Vec<_>>();
    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    let upper = 0xA5A5_A5A5_A5A5_A5A5;
    let flags_before = 0xCD7;
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let r8 = VReg::Arch(ArchReg::X86(X86Reg::R8));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x400);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;

    // Legacy PINSRB reads the low byte of r32, masks the immediate to four
    // bits, replaces only lane 15, and preserves state above bit 127.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[9] = vector(&merge, upper);
    }
    ctx.write_vreg(r8, 0xDEAD_BEEF_0123_45E7);
    execute_lifted_x86(
        &[0x66, 0x45, 0x0F, 0x3A, 0x20, 0xC8, 0x1F],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let mut expected = merge.clone();
        expected[15] = 0xE7;
        assert_eq!(bytes(&x86.xmm[9], 16), expected);
        assert!(x86.xmm[9][2..].iter().all(|word| *word == upper));
    }

    // The older map-0F PINSRW form has reversed ModR/M operand direction,
    // masks its selector to three bits, and has the same legacy upper rule.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[9] = vector(&merge, upper);
    }
    ctx.write_vreg(r8, 0xDEAD_BEEF_0123_A1B2);
    execute_lifted_x86(&[0x66, 0x45, 0x0F, 0xC4, 0xC8, 0x0F], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let mut expected = merge.clone();
        expected[14..16].copy_from_slice(&0xA1B2u16.to_le_bytes());
        assert_eq!(bytes(&x86.xmm[9], 16), expected);
        assert!(x86.xmm[9][2..].iter().all(|word| *word == upper));
    }

    // EVEX map-0F PINSRW uses Tuple1 Scalar disp8*2, high vector registers,
    // and clears all state above bit 127.
    memory.write(0x192, &0xC3D4u16.to_le_bytes()).unwrap();
    ctx.write_vreg(rax, 0x180);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[17] = sentinel;
        x86.xmm[18] = vector(&second, upper);
    }
    execute_lifted_x86(
        &[0x62, 0xE1, 0x6D, 0x00, 0xC4, 0x48, 0x09, 0x0F],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let mut expected = second.clone();
        expected[14..16].copy_from_slice(&0xC3D4u16.to_le_bytes());
        assert_eq!(bytes(&x86.xmm[17], 16), expected);
        assert!(x86.xmm[17][2..].iter().all(|word| *word == 0));
    }

    // VEX memory insertion is unaligned-capable, reads exactly four bytes,
    // merges from xmm10, and zeros all state above bit 127.
    let dword = 0x8877_6655u32;
    memory.write(0x115, &dword.to_le_bytes()).unwrap();
    ctx.write_vreg(rax, 0x101);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[9] = sentinel;
        x86.xmm[10] = vector(&merge, upper);
    }
    execute_lifted_x86(
        &[0xC4, 0x63, 0x29, 0x22, 0x48, 0x14, 0x07],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let mut expected = merge.clone();
        expected[12..16].copy_from_slice(&dword.to_le_bytes());
        assert_eq!(bytes(&x86.xmm[9], 16), expected);
        assert!(x86.xmm[9][2..].iter().all(|word| *word == 0));
    }

    // EVEX high-register qword insertion merges xmm18 into xmm17 and reads
    // the old GPR value before zeroing the destination's upper state.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[17] = sentinel;
        x86.xmm[18] = vector(&merge, upper);
    }
    ctx.write_vreg(r8, 0x0123_4567_89AB_CDEF);
    execute_lifted_x86(
        &[0x62, 0xC3, 0xED, 0x00, 0x22, 0xC8, 0x01],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let mut expected = merge.clone();
        expected[8..16].copy_from_slice(&0x0123_4567_89AB_CDEFu64.to_le_bytes());
        assert_eq!(bytes(&x86.xmm[17], 16), expected);
        assert!(x86.xmm[17][2..].iter().all(|word| *word == 0));
    }

    // Full self-aliasing must snapshot the selected source lane and all
    // merge lanes before the destination write. ZMask then clears lanes 1/3.
    for insn in [
        &[0x66, 0x45, 0x0F, 0x3A, 0x21, 0xC9, 0x6A][..],
        &[0xC4, 0x43, 0x31, 0x21, 0xC9, 0x6A][..],
    ] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[9] = vector(&merge, upper);
        }
        execute_lifted_x86(insn, &mut ctx, &mut memory);
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            let mut expected = vec![0u8; 16];
            expected[0..4].copy_from_slice(&merge[0..4]);
            expected[8..12].copy_from_slice(&merge[4..8]);
            assert_eq!(bytes(&x86.xmm[9], 16), expected);
            let expected_upper = if insn[0] == 0x66 { upper } else { 0 };
            assert!(x86.xmm[9][2..].iter().all(|word| *word == expected_upper));
        }
    }

    // Memory INSERTPS ignores Count_S=3. EVEX Tuple1 Scalar disp8 scales
    // by 4, so disp8=5 reads base+20 and inserts that dword into lane 2.
    let inserted = 0xA1B2_C3D4u32;
    memory.write(0x194, &inserted.to_le_bytes()).unwrap();
    ctx.write_vreg(rax, 0x180);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[17] = sentinel;
        x86.xmm[18] = vector(&second, upper);
    }
    execute_lifted_x86(
        &[0x62, 0xE3, 0x6D, 0x00, 0x21, 0x48, 0x05, 0xE0],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        let mut expected = second.clone();
        expected[8..12].copy_from_slice(&inserted.to_le_bytes());
        assert_eq!(bytes(&x86.xmm[17], 16), expected);
        assert!(x86.xmm[17][2..].iter().all(|word| *word == 0));
    }

    // A scalar source load fault precedes every architectural destination
    // write, including VEX/EVEX upper-state clearing.
    ctx.write_vreg(rax, 0x3FE);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[9] = sentinel;
        x86.xmm[10] = vector(&merge, upper);
    }
    let fault = execute_lifted_x86(&[0xC4, 0x63, 0x29, 0x22, 0x08, 0x03], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[9], sentinel);
    }

    ctx.write_vreg(rax, 0x3FF);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[17] = sentinel;
        x86.xmm[18] = vector(&second, upper);
    }
    let fault = execute_lifted_x86(
        &[0x62, 0xC1, 0x6D, 0x00, 0xC4, 0x08, 0x07],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[17], sentinel);
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn x86_dot_product_softfloat_core_matches_ieee_nearest_and_directed_edges() {
    let mut state = 0xD1B5_4A32_D192_ED03u64;
    let mut next = || {
        state = state
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0xBF58_476D_1CE4_E5B9);
        state
    };

    // For finite normal inputs the host's individual IEEE operation is an
    // independent nearest-even oracle. Products/additions are never fused.
    for _ in 0..20_000 {
        let a = next() as u32;
        let b = next() as u32;
        let a_exp = a & 0x7F80_0000;
        let b_exp = b & 0x7F80_0000;
        if a_exp == 0 || a_exp == 0x7F80_0000 || b_exp == 0 || b_exp == 0x7F80_0000 {
            continue;
        }
        let multiply = SmirInterpreter::x86_simd_fp_mul(
            u64::from(a),
            u64::from(b),
            X86_SIMD_F32,
            FpRoundMode::RoundNearest,
            0x1F80,
        );
        let add = SmirInterpreter::x86_simd_fp_add(
            u64::from(a),
            u64::from(b),
            X86_SIMD_F32,
            FpRoundMode::RoundNearest,
            0x1F80,
        );
        assert_eq!(
            multiply.bits as u32,
            (f32::from_bits(a) * f32::from_bits(b)).to_bits()
        );
        assert_eq!(
            add.bits as u32,
            (f32::from_bits(a) + f32::from_bits(b)).to_bits()
        );
    }

    for _ in 0..20_000 {
        let a = next();
        let b = next();
        let a_exp = a & 0x7FF0_0000_0000_0000;
        let b_exp = b & 0x7FF0_0000_0000_0000;
        if a_exp == 0
            || a_exp == 0x7FF0_0000_0000_0000
            || b_exp == 0
            || b_exp == 0x7FF0_0000_0000_0000
        {
            continue;
        }
        let multiply =
            SmirInterpreter::x86_simd_fp_mul(a, b, X86_SIMD_F64, FpRoundMode::RoundNearest, 0x1F80);
        let add =
            SmirInterpreter::x86_simd_fp_add(a, b, X86_SIMD_F64, FpRoundMode::RoundNearest, 0x1F80);
        assert_eq!(
            multiply.bits,
            (f64::from_bits(a) * f64::from_bits(b)).to_bits()
        );
        assert_eq!(add.bits, (f64::from_bits(a) + f64::from_bits(b)).to_bits());
    }

    for (format, one, half_ulp, next_up) in [
        (
            X86_SIMD_F32,
            u64::from(0x3F80_0000u32),
            u64::from(0x3380_0000u32),
            u64::from(0x3F80_0001u32),
        ),
        (
            X86_SIMD_F64,
            0x3FF0_0000_0000_0000,
            0x3CA0_0000_0000_0000,
            0x3FF0_0000_0000_0001,
        ),
    ] {
        for (mode, expected) in [
            (FpRoundMode::RoundNearest, one),
            (FpRoundMode::RoundTowardZero, one),
            (FpRoundMode::RoundDown, one),
            (FpRoundMode::RoundUp, next_up),
        ] {
            let result = SmirInterpreter::x86_simd_fp_add(one, half_ulp, format, mode, 0x1F80);
            assert_eq!(result.bits, expected);
            assert_ne!(result.status & (1 << 5), 0);
        }
    }
}
#[test]
fn lifted_vzero_execute_maxvl_register_limits_and_preserve_flags() {
    fn seeded(register: u64) -> VecValue {
        let mut value = [0; 16];
        for (lane, word) in value.iter_mut().enumerate() {
            *word = (register << 56) | lane as u64 + 1;
        }
        value
    }

    let flags_before = 0xCD7;
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(1);
    ctx.flags.materialized = MaterializedFlags::from_rflags(flags_before);
    ctx.flags.lazy = None;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        for index in 0..32 {
            x86.xmm[index] = seeded(index as u64);
        }
    }

    assert!(matches!(
        execute_lifted_x86(&[0xC5, 0xF8, 0x77], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for index in 0..16 {
            assert_eq!(&x86.xmm[index][..2], &seeded(index as u64)[..2]);
            assert_eq!(&x86.xmm[index][2..], &[0; 14]);
        }
        for index in 16..32 {
            assert_eq!(x86.xmm[index], seeded(index as u64));
        }
    }

    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        for index in 0..32 {
            x86.xmm[index] = seeded(index as u64 + 32);
        }
    }
    assert!(matches!(
        execute_lifted_x86(&[0xC5, 0xFC, 0x77], &mut ctx, &mut memory),
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        for index in 0..16 {
            assert_eq!(x86.xmm[index], [0; 16]);
        }
        for index in 16..32 {
            assert_eq!(x86.xmm[index], seeded(index as u64 + 32));
        }
    }
    ctx.flags.materialize_all();
    assert_eq!(ctx.flags.materialized.to_rflags(), flags_before);
}
#[test]
fn lifted_get_exponent_executes_exact_specials_daz_masks_sae_and_faults() {
    fn vector_u32(values: &[u32], fill: u64) -> VecValue {
        let mut bytes = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        bytes.resize(bytes.len().next_multiple_of(8), 0);
        let mut result = [fill; 16];
        for (index, chunk) in bytes.chunks_exact(8).enumerate() {
            result[index] = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        result
    }
    fn lanes_u32(value: &VecValue, count: usize) -> Vec<u32> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(count * 4)
            .collect::<Vec<_>>()
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect()
    }
    fn vector_u16(values: &[u16], fill: u64) -> VecValue {
        let mut bytes = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        bytes.resize(bytes.len().next_multiple_of(8), 0);
        let mut result = [fill; 16];
        for (index, chunk) in bytes.chunks_exact(8).enumerate() {
            result[index] = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        result
    }
    fn lanes_u16(value: &VecValue, count: usize) -> Vec<u16> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(count * 2)
            .collect::<Vec<_>>()
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes(chunk.try_into().unwrap()))
            .collect()
    }

    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x200);

    // Normal, zero, and denormal FP32 lanes use floor(log2(abs(x))). A
    // preserved denormal records DE and every integer result is exact.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.xmm[3] = vector_u32(
            &[1.0f32.to_bits(), 3.0f32.to_bits(), (-0.0f32).to_bits(), 1],
            0,
        );
        x86.mxcsr = 0x1F80;
    }
    let packed = execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x08, 0x42, 0xCB], &mut ctx, &mut memory);
    assert!(matches!(packed, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            lanes_u32(&x86.xmm[1], 4),
            [
                0.0f32.to_bits(),
                1.0f32.to_bits(),
                f32::NEG_INFINITY.to_bits(),
                (-149.0f32).to_bits(),
            ]
        );
        assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
        assert_eq!(x86.mxcsr & 0x3F, 1 << 1);
    }

    // FP32/FP64 DAZ converts a denormal to signed zero before GETEXP,
    // whereas AVX512-FP16 ignores DAZ and still reports DE.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = vector_u32(&[1, 0, 0, 0], 0);
        x86.mxcsr = 0x1F80 | (1 << 6);
    }
    execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x08, 0x42, 0xCB], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(lanes_u32(&x86.xmm[1], 1), [f32::NEG_INFINITY.to_bits()]);
        assert_eq!(x86.mxcsr & 0x3F, 0);
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = vector_u16(&[1, 0, 0, 0, 0, 0, 0, 0], 0);
        x86.mxcsr = 0x1F80 | (1 << 6);
    }
    execute_lifted_x86(&[0x62, 0xF6, 0x7D, 0x08, 0x42, 0xCB], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(lanes_u16(&x86.xmm[1], 1), [0xCE00]); // -24.0h
        assert_ne!(x86.mxcsr & (1 << 1), 0);
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        x86.mxcsr = 0x1F80;
    }
    execute_lifted_x86(&[0x62, 0xF2, 0xFD, 0x08, 0x42, 0xCB], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1][0], (-1074.0f64).to_bits());
        assert_ne!(x86.mxcsr & (1 << 1), 0);
    }

    // Scalar writemasking affects only the low element. Upper XMM lanes
    // always come from EVEX.vvvv, and state above bit 127 is cleared.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = vector_u32(&[7.0f32.to_bits(); 4], sentinel[0]);
        x86.xmm[2] = vector_u32(
            &[
                99.0f32.to_bits(),
                11.0f32.to_bits(),
                12.0f32.to_bits(),
                13.0f32.to_bits(),
            ],
            sentinel[0],
        );
        x86.xmm[3] = vector_u32(&[8.0f32.to_bits(), 0, 0, 0], 0);
        x86.k[2] = 0;
        x86.mxcsr = 0x1F80;
    }
    execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x0A, 0x43, 0xCB], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            lanes_u32(&x86.xmm[1], 4),
            [7.0f32, 11.0, 12.0, 13.0].map(f32::to_bits)
        );
        assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
    }
    execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x8A, 0x43, 0xCB], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(lanes_u32(&x86.xmm[1], 1), [0]);
    }

    // SNaN quieting preserves sign and payload. Unmasked IE commits MXCSR
    // but traps atomically; SAE suppresses both status and the trap.
    let snan = 0xFF80_1234u32;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.xmm[3] = vector_u32(&[snan, 0, 0, 0], 0);
        x86.mxcsr = 0x1F80 & !(1 << 7);
    }
    let invalid = execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x08, 0x42, 0xCB], &mut ctx, &mut memory);
    assert!(matches!(
        invalid,
        BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], sentinel);
        assert_ne!(x86.mxcsr & 1, 0);
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.xmm[3] = vector_u32(&[snan; 16], 0);
        x86.mxcsr = 0x1F80 & !(1 << 7);
    }
    let sae = execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x18, 0x42, 0xCB], &mut ctx, &mut memory);
    assert!(matches!(sae, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(lanes_u32(&x86.xmm[1], 1), [snan | 0x0040_0000]);
        assert_eq!(x86.mxcsr & 0x3F, 0);
    }

    // An inactive lane neither examines its SNaN nor accesses memory.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.xmm[3] = vector_u32(&[snan; 4], 0);
        x86.k[2] = 0;
        x86.mxcsr = 0x1F80;
    }
    execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x0A, 0x42, 0xCB], &mut ctx, &mut memory);
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[1][..2], &sentinel[..2]);
        assert_eq!(x86.mxcsr & 0x3F, 0);
    }

    ctx.write_vreg(rax, 0x300);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.xmm[2] = vector_u32(
            &[
                0.0f32.to_bits(),
                11.0f32.to_bits(),
                12.0f32.to_bits(),
                13.0f32.to_bits(),
            ],
            0,
        );
        x86.k[2] = 0;
    }
    let suppressed_fault =
        execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x0A, 0x43, 0x08], &mut ctx, &mut memory);
    assert!(matches!(
        suppressed_fault,
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.k[2] = 1;
    }
    let fault = execute_lifted_x86(&[0x62, 0xF2, 0x6D, 0x0A, 0x43, 0x08], &mut ctx, &mut memory);
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], sentinel);
    }

    // Packed broadcasts aggregate only the mask bits corresponding to
    // encoded lanes: no applicable lane performs no read, while any
    // applicable active lane performs the single scalar memory access.
    let mut broadcast_preserved = sentinel;
    broadcast_preserved[8..].fill(0);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
        x86.k[2] = 1 << 63;
    }
    let suppressed_broadcast =
        execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x5A, 0x42, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        suppressed_broadcast,
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], broadcast_preserved);
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.k[2] = 1;
    }
    let broadcast_fault =
        execute_lifted_x86(&[0x62, 0xF2, 0x7D, 0x5A, 0x42, 0x00], &mut ctx, &mut memory);
    assert!(matches!(
        broadcast_fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], broadcast_preserved);
    }
}
#[test]
fn lifted_get_mantissa_executes_controls_specials_daz_masks_sae_and_faults() {
    fn vector_u32(values: &[u32], fill: u64) -> VecValue {
        let mut bytes = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        bytes.resize(bytes.len().next_multiple_of(8), 0);
        let mut result = [fill; 16];
        for (index, chunk) in bytes.chunks_exact(8).enumerate() {
            result[index] = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        result
    }
    fn lanes_u32(value: &VecValue, count: usize) -> Vec<u32> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(count * 4)
            .collect::<Vec<_>>()
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect()
    }
    fn vector_u16(values: &[u16], fill: u64) -> VecValue {
        let mut bytes = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        bytes.resize(bytes.len().next_multiple_of(8), 0);
        let mut result = [fill; 16];
        for (index, chunk) in bytes.chunks_exact(8).enumerate() {
            result[index] = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        result
    }
    fn lanes_u16(value: &VecValue, count: usize) -> Vec<u16> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(count * 2)
            .collect::<Vec<_>>()
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes(chunk.try_into().unwrap()))
            .collect()
    }

    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x200);

    // The four normalization intervals are exact exponent-field rewrites.
    // Reserved high immediate bits are encoded and ignored semantically.
    for (imm, expected) in [
        (0x00, 1.5f32),
        (0x01, 0.75),
        (0x02, 0.75),
        (0x03, 0.75),
        (0xF3, 0.75),
    ] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[3] = vector_u32(&[3.0f32.to_bits(); 4], 0);
            x86.mxcsr = 0x1F80;
        }
        let result = execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x08, 0x26, 0xCB, imm],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(lanes_u32(&x86.xmm[1], 4), [expected.to_bits(); 4]);
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
            assert_eq!(x86.mxcsr & 0x3F, 0);
        }
    }

    // SC=00 preserves a negative sign, SC=01 forces positive, and SC=1x
    // rejects negative nonzero inputs with canonical indefinite and IE.
    for (imm, expected, status) in [
        (0x00, (-1.5f32).to_bits(), 0),
        (0x04, 1.5f32.to_bits(), 0),
        (0x08, 0xFFC0_0000, 1),
        (0x0C, 0xFFC0_0000, 1),
    ] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3] = vector_u32(&[(-3.0f32).to_bits(); 4], 0);
            x86.mxcsr = 0x1F80;
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x08, 0x26, 0xCB, imm],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(lanes_u32(&x86.xmm[1], 1), [expected]);
            assert_eq!(x86.mxcsr & 1, status);
        }
    }

    // Special values ignore interval control. NaN payload/sign survive
    // quieting; negative zero and infinity still obey SC[0].
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = vector_u32(
            &[
                0.0f32.to_bits(),
                (-0.0f32).to_bits(),
                f32::INFINITY.to_bits(),
                f32::NEG_INFINITY.to_bits(),
            ],
            0,
        );
        x86.mxcsr = 0x1F80;
    }
    execute_lifted_x86(
        &[0x62, 0xF3, 0x7D, 0x08, 0x26, 0xCB, 0x03],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            lanes_u32(&x86.xmm[1], 4),
            [1.0f32, -1.0, 1.0, -1.0].map(f32::to_bits)
        );
    }
    let qnan = 0xFFC0_1234u32;
    let snan = 0xFF80_5678u32;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = vector_u32(&[qnan, snan, qnan, snan], 0);
        x86.mxcsr = 0x1F80;
    }
    execute_lifted_x86(
        &[0x62, 0xF3, 0x7D, 0x08, 0x26, 0xCB, 0x08],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            lanes_u32(&x86.xmm[1], 4),
            [qnan, snan | 0x0040_0000, qnan, snan | 0x0040_0000]
        );
        assert_ne!(x86.mxcsr & 1, 0);
    }

    // FP32 DAZ converts a denormal to signed zero. FP16 ignores DAZ and
    // reports DE; interval 01 exposes the source exponent parity.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = vector_u32(&[1, 0, 0, 0], 0);
        x86.mxcsr = 0x1F80;
    }
    execute_lifted_x86(
        &[0x62, 0xF3, 0x7D, 0x08, 0x26, 0xCB, 0x01],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(lanes_u32(&x86.xmm[1], 1), [0.5f32.to_bits()]);
        assert_ne!(x86.mxcsr & (1 << 1), 0);
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = vector_u32(&[0x8000_0001, 0, 0, 0], 0);
        x86.mxcsr = 0x1F80 | (1 << 6);
    }
    execute_lifted_x86(
        &[0x62, 0xF3, 0x7D, 0x08, 0x26, 0xCB, 0x08],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(lanes_u32(&x86.xmm[1], 1), [(-1.0f32).to_bits()]);
        assert_eq!(x86.mxcsr & 0x3F, 0);
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = vector_u16(&[1; 8], 0);
        x86.mxcsr = 0x1F80 | (1 << 6);
    }
    execute_lifted_x86(
        &[0x62, 0xF3, 0x7C, 0x08, 0x26, 0xCB, 0x01],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(lanes_u16(&x86.xmm[1], 1), [0x3C00]); // +1.0h
        assert_ne!(x86.mxcsr & (1 << 1), 0);
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = [
            1,
            3.0f64.to_bits(),
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        x86.mxcsr = 0x1F80;
    }
    execute_lifted_x86(
        &[0x62, 0xF3, 0xFD, 0x08, 0x26, 0xCB, 0x01],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1][0], 1.0f64.to_bits()); // exponent -1074 is even
        assert_eq!(x86.xmm[1][1], 0.75f64.to_bits());
        assert_ne!(x86.mxcsr & (1 << 1), 0);
    }

    // An inactive lane neither classifies its SNaN nor updates MXCSR.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.xmm[3] = vector_u32(&[snan; 4], 0);
        x86.k[2] = 0;
        x86.mxcsr = 0x1F80 & !(1 << 7);
    }
    let masked_snan = execute_lifted_x86(
        &[0x62, 0xF3, 0x7D, 0x0A, 0x26, 0xCB, 0x08],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(masked_snan, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(&x86.xmm[1][..2], &sentinel[..2]);
        assert_eq!(x86.mxcsr & 1, 0);
    }

    // Unmasked IE/DE trap atomically. SAE suppresses both status and trap
    // while retaining the architecturally selected indefinite result.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.xmm[3] = vector_u32(&[(-3.0f32).to_bits(); 4], 0);
        x86.mxcsr = 0x1F80 & !(1 << 7);
    }
    let invalid = execute_lifted_x86(
        &[0x62, 0xF3, 0x7D, 0x08, 0x26, 0xCB, 0x08],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(
        invalid,
        BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], sentinel);
        assert_ne!(x86.mxcsr & 1, 0);
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.xmm[3] = vector_u32(&[1; 4], 0);
        x86.mxcsr = 0x1F80 & !(1 << 8);
    }
    let denormal = execute_lifted_x86(
        &[0x62, 0xF3, 0x7D, 0x08, 0x26, 0xCB, 0x00],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(
        denormal,
        BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], sentinel);
        assert_ne!(x86.mxcsr & (1 << 1), 0);
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.xmm[3] = vector_u32(&[(-3.0f32).to_bits(); 16], 0);
        x86.mxcsr = 0x1F80 & !(1 << 7);
    }
    let sae = execute_lifted_x86(
        &[0x62, 0xF3, 0x7D, 0x18, 0x26, 0xCB, 0x08],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(sae, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(lanes_u32(&x86.xmm[1], 1), [0xFFC0_0000]);
        assert_eq!(x86.mxcsr & 0x3F, 0);
    }

    // Scalar writemasking applies only to the low element; upper XMM bits
    // come from EVEX.vvvv and state above bit 127 is cleared.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = vector_u32(&[7.0f32.to_bits(); 4], sentinel[0]);
        x86.xmm[2] = vector_u32(
            &[
                99.0f32.to_bits(),
                11.0f32.to_bits(),
                12.0f32.to_bits(),
                13.0f32.to_bits(),
            ],
            sentinel[0],
        );
        x86.xmm[3] = vector_u32(&[3.0f32.to_bits(), 0, 0, 0], 0);
        x86.k[2] = 0;
        x86.mxcsr = 0x1F80;
    }
    execute_lifted_x86(
        &[0x62, 0xF3, 0x6D, 0x0A, 0x27, 0xCB, 0x03],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            lanes_u32(&x86.xmm[1], 4),
            [7.0f32, 11.0, 12.0, 13.0].map(f32::to_bits)
        );
        assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
    }
    execute_lifted_x86(
        &[0x62, 0xF3, 0x6D, 0x8A, 0x27, 0xCB, 0x03],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(lanes_u32(&x86.xmm[1], 1), [0]);
    }

    // Inactive masks suppress source exceptions and invalid memory. An
    // applicable active bit exposes the scalar memory fault atomically.
    ctx.write_vreg(rax, 0x300);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.k[2] = 0;
        x86.mxcsr = 0x1F80;
    }
    let suppressed_fault = execute_lifted_x86(
        &[0x62, 0xF3, 0x6D, 0x0A, 0x27, 0x08, 0x03],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(
        suppressed_fault,
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.k[2] = 1;
    }
    let fault = execute_lifted_x86(
        &[0x62, 0xF3, 0x6D, 0x0A, 0x27, 0x08, 0x03],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], sentinel);
    }

    let mut packed_preserved = sentinel;
    packed_preserved[8..].fill(0);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[0] = sentinel;
        x86.k[2] = 1 << 63;
    }
    let suppressed_broadcast = execute_lifted_x86(
        &[0x62, 0xF3, 0x7D, 0x5A, 0x26, 0x00, 0x03],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(
        suppressed_broadcast,
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[0], packed_preserved);
    }
}
#[test]
fn lifted_reduce_executes_remainders_special_cases_exceptions_masks_and_faults() {
    fn vector_u32(values: &[u32], fill: u64) -> VecValue {
        let mut bytes = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        bytes.resize(bytes.len().next_multiple_of(8), 0);
        let mut result = [fill; 16];
        for (index, chunk) in bytes.chunks_exact(8).enumerate() {
            result[index] = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        result
    }
    fn lanes_u32(value: &VecValue, count: usize) -> Vec<u32> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(count * 4)
            .collect::<Vec<_>>()
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect()
    }
    fn vector_u16(values: &[u16], fill: u64) -> VecValue {
        let mut bytes = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        bytes.resize(bytes.len().next_multiple_of(8), 0);
        let mut result = [fill; 16];
        for (index, chunk) in bytes.chunks_exact(8).enumerate() {
            result[index] = u64::from_le_bytes(chunk.try_into().unwrap());
        }
        result
    }
    fn lanes_u16(value: &VecValue, count: usize) -> Vec<u16> {
        value
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .take(count * 2)
            .collect::<Vec<_>>()
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes(chunk.try_into().unwrap()))
            .collect()
    }

    const IE: u32 = 1;
    const UE: u32 = 1 << 4;
    const PE: u32 = 1 << 5;
    const DAZ: u32 = 1 << 6;
    const IM: u32 = 1 << 7;
    const PM: u32 = 1 << 12;
    const FTZ: u32 = 1 << 15;

    let sentinel = [0xCCCC_CCCC_CCCC_CCCCu64; 16];
    let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
    let mut ctx = SmirContext::new_x86_64();
    let mut memory = FlatMemory::new(0x200);

    // M=0 selects the integer grid. REDUCE returns source minus the grid
    // point selected by each immediate rounding mode.
    let source = [1.5f32, 2.5, -1.5, -2.5].map(f32::to_bits);
    for (imm, expected) in [
        (0x00, [-0.5f32, 0.5, 0.5, -0.5]),
        (0x01, [0.5f32, 0.5, 0.5, 0.5]),
        (0x02, [-0.5f32, -0.5, -0.5, -0.5]),
        (0x03, [0.5f32, 0.5, -0.5, -0.5]),
    ] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[1] = sentinel;
            x86.xmm[3] = vector_u32(&source, 0);
            x86.mxcsr = 0x1F80;
        }
        let result = execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x08, 0x56, 0xCB, imm],
            &mut ctx,
            &mut memory,
        );
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(lanes_u32(&x86.xmm[1], 4), expected.map(f32::to_bits));
            assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
            assert_eq!(x86.mxcsr & PE, 0);
        }
    }

    // imm[2] selects MXCSR.RC. Here round-up maps 1.25 to 2.0, leaving
    // -0.75. The internal round-to-grid step does not itself report precision.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.xmm[3] = vector_u32(&[1.25f32.to_bits(); 4], 0);
        x86.mxcsr = (0x1F80 & !(3 << 13)) | (2 << 13);
    }
    execute_lifted_x86(
        &[0x62, 0xF3, 0x7D, 0x08, 0x56, 0xCB, 0x04],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(lanes_u32(&x86.xmm[1], 4), [(-0.75f32).to_bits(); 4]);
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.xmm[3] = vector_u32(&[1; 4], 0);
        x86.mxcsr = (0x1F80 | FTZ) & !PM;
    }
    let precision = execute_lifted_x86(
        &[0x62, 0xF3, 0x7D, 0x08, 0x56, 0xCB, 0x00],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(
        precision,
        BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], sentinel);
        assert_ne!(x86.mxcsr & PE, 0);
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.mxcsr = (0x1F80 | FTZ) & !PM;
    }
    let precision_suppressed = execute_lifted_x86(
        &[0x62, 0xF3, 0x7D, 0x08, 0x56, 0xCB, 0x08],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(
        precision_suppressed,
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(lanes_u32(&x86.xmm[1], 4), [0; 4]);
        assert_eq!(x86.mxcsr & PE, 0);
    }

    // Infinities reduce to +0. Exact zero signs are determined by RC:
    // round-down produces -0 and every other mode produces +0.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = vector_u32(
            &[
                0.0f32.to_bits(),
                (-0.0f32).to_bits(),
                f32::INFINITY.to_bits(),
                f32::NEG_INFINITY.to_bits(),
            ],
            0,
        );
        x86.mxcsr = 0x1F80;
    }
    execute_lifted_x86(
        &[0x62, 0xF3, 0x7D, 0x08, 0x56, 0xCB, 0x01],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(lanes_u32(&x86.xmm[1], 4), [0x8000_0000, 0x8000_0000, 0, 0]);
    }

    // QNaN payloads survive; SNaNs quiet and raise IE unless SAE applies.
    let qnan = 0xFFC0_1234u32;
    let snan = 0xFF80_5678u32;
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.xmm[3] = vector_u32(&[qnan, snan, qnan, snan], 0);
        x86.mxcsr = 0x1F80 & !IM;
    }
    let invalid = execute_lifted_x86(
        &[0x62, 0xF3, 0x7D, 0x08, 0x56, 0xCB, 0x00],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(
        invalid,
        BlockResult::Exit(ExitReason::SimdFloatingPoint { .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], sentinel);
        assert_ne!(x86.mxcsr & IE, 0);
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.xmm[3] = vector_u32(&[snan; 16], 0);
        x86.mxcsr = 0x1F80 & !IM;
    }
    let sae = execute_lifted_x86(
        &[0x62, 0xF3, 0x7D, 0x18, 0x56, 0xCB, 0x00],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(sae, BlockResult::Exit(ExitReason::Halt)));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(lanes_u32(&x86.xmm[1], 1), [snan | 0x0040_0000]);
        assert_eq!(x86.mxcsr & 0x3F, 0);
    }

    // FP32 DAZ consumes denormals as zero. An exact tiny remainder does not
    // report precision unless FTZ flushes it. FP16 ignores DAZ and FTZ.
    for (mxcsr, expected) in [(0x1F80, 1u32), (0x1F80 | DAZ, 0u32)] {
        if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
            x86.xmm[3] = vector_u32(&[1; 4], 0);
            x86.mxcsr = mxcsr;
        }
        execute_lifted_x86(
            &[0x62, 0xF3, 0x7D, 0x08, 0x56, 0xCB, 0x00],
            &mut ctx,
            &mut memory,
        );
        if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
            assert_eq!(lanes_u32(&x86.xmm[1], 4), [expected; 4]);
            assert_eq!(x86.mxcsr & (UE | PE), 0);
        }
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = vector_u32(&[1; 4], 0);
        x86.mxcsr = 0x1F80 | FTZ;
    }
    execute_lifted_x86(
        &[0x62, 0xF3, 0x7D, 0x08, 0x56, 0xCB, 0x08],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(lanes_u32(&x86.xmm[1], 4), [0; 4]);
        assert_eq!(x86.mxcsr & (UE | PE), 0);
    }
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[3] = vector_u16(&[1; 8], 0);
        x86.mxcsr = 0x1F80 | DAZ | FTZ;
    }
    execute_lifted_x86(
        &[0x62, 0xF3, 0x7C, 0x08, 0x56, 0xCB, 0xF0],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(lanes_u16(&x86.xmm[1], 8), [1; 8]);
        assert_eq!(x86.mxcsr & (UE | PE), 0);
    }

    // Scalar writemasking changes only the low element, sources upper XMM
    // bits from vvvv, and suppresses inactive source exceptions/faults.
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = vector_u32(&[7.0f32.to_bits(); 4], sentinel[0]);
        x86.xmm[2] = vector_u32(
            &[
                99.0f32.to_bits(),
                11.0f32.to_bits(),
                12.0f32.to_bits(),
                13.0f32.to_bits(),
            ],
            sentinel[0],
        );
        x86.xmm[3] = vector_u32(&[snan, 0, 0, 0], 0);
        x86.k[2] = 0;
        x86.mxcsr = 0x1F80 & !IM;
    }
    execute_lifted_x86(
        &[0x62, 0xF3, 0x6D, 0x0A, 0x57, 0xCB, 0x00],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(
            lanes_u32(&x86.xmm[1], 4),
            [7.0f32, 11.0, 12.0, 13.0].map(f32::to_bits)
        );
        assert!(x86.xmm[1][2..].iter().all(|word| *word == 0));
        assert_eq!(x86.mxcsr & IE, 0);
    }
    execute_lifted_x86(
        &[0x62, 0xF3, 0x6D, 0x8A, 0x57, 0xCB, 0x00],
        &mut ctx,
        &mut memory,
    );
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(lanes_u32(&x86.xmm[1], 1), [0]);
    }

    ctx.write_vreg(rax, 0x300);
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.k[2] = 0;
        x86.mxcsr = 0x1F80;
    }
    let suppressed_fault = execute_lifted_x86(
        &[0x62, 0xF3, 0x6D, 0x0A, 0x57, 0x08, 0x00],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(
        suppressed_fault,
        BlockResult::Exit(ExitReason::Halt)
    ));
    if let ArchRegState::X86_64(x86) = &mut ctx.arch_regs {
        x86.xmm[1] = sentinel;
        x86.k[2] = 1;
    }
    let fault = execute_lifted_x86(
        &[0x62, 0xF3, 0x6D, 0x0A, 0x57, 0x08, 0x00],
        &mut ctx,
        &mut memory,
    );
    assert!(matches!(
        fault,
        BlockResult::Exit(ExitReason::MemoryFault { write: false, .. })
    ));
    if let ArchRegState::X86_64(x86) = &ctx.arch_regs {
        assert_eq!(x86.xmm[1], sentinel);
    }
}
#[test]
fn x86_range_exact_semantics_cover_controls_nans_daz_and_ties() {
    let first = u64::from((-2.0f32).to_bits());
    let second = u64::from(3.0f32.to_bits());
    for sign_control in 0..4u8 {
        for compare in 0..4u8 {
            let imm = compare | (sign_control << 2);
            let selected = match compare {
                0 | 2 => first,
                _ => second,
            };
            let expected_sign = match sign_control {
                0 => first & 0x8000_0000,
                1 => selected & 0x8000_0000,
                2 => 0,
                _ => 0x8000_0000,
            };
            let result = SmirInterpreter::x86_simd_range(first, second, X86_SIMD_F32, 0x1F80, imm);
            assert_eq!(
                result.bits,
                expected_sign | (selected & 0x7FFF_FFFF),
                "imm={imm:#04x}"
            );
            assert_eq!(result.status, 0);
        }
    }

    let positive_zero = u64::from(0.0f32.to_bits());
    let negative_zero = u64::from((-0.0f32).to_bits());
    for (a, b) in [
        (positive_zero, negative_zero),
        (negative_zero, positive_zero),
    ] {
        for compare in 0..4u8 {
            let result = SmirInterpreter::x86_simd_range(a, b, X86_SIMD_F32, 0x1F80, compare | 4);
            assert_eq!(
                result.bits,
                if compare & 1 == 0 {
                    negative_zero
                } else {
                    positive_zero
                },
                "opposite-zero compare={compare}, a={a:#x}, b={b:#x}"
            );
        }
    }

    let positive_two = u64::from(2.0f32.to_bits());
    let negative_two = u64::from((-2.0f32).to_bits());
    for (a, b) in [(positive_two, negative_two), (negative_two, positive_two)] {
        assert_eq!(
            SmirInterpreter::x86_simd_range(a, b, X86_SIMD_F32, 0x1F80, 6).bits,
            negative_two
        );
        assert_eq!(
            SmirInterpreter::x86_simd_range(a, b, X86_SIMD_F32, 0x1F80, 7).bits,
            positive_two
        );
    }

    let first_snan = 0x7F80_1234u64;
    let second_snan = 0xFF80_5678u64;
    let first_qnan = 0xFFC1_2345u64;
    let second_qnan = 0x7FC5_6789u64;
    let both_snan =
        SmirInterpreter::x86_simd_range(first_snan, second_snan, X86_SIMD_F32, 0x1F80, 0x0C);
    assert_eq!(both_snan.bits, first_snan | 0x0040_0000);
    assert_eq!(both_snan.status, 1);
    let second_snan_result =
        SmirInterpreter::x86_simd_range(first_qnan, second_snan, X86_SIMD_F32, 0x1F80, 0);
    assert_eq!(second_snan_result.bits, second_snan | 0x0040_0000);
    assert_eq!(second_snan_result.status, 1);
    assert_eq!(
        SmirInterpreter::x86_simd_range(first_qnan, second_qnan, X86_SIMD_F32, 0x1F80, 8,).bits,
        first_qnan & 0x7FFF_FFFF
    );
    let negative_three = u64::from((-3.0f32).to_bits());
    assert_eq!(
        SmirInterpreter::x86_simd_range(first_qnan, negative_three, X86_SIMD_F32, 0x1F80, 4,).bits,
        negative_three
    );
    assert_eq!(
        SmirInterpreter::x86_simd_range(negative_three, second_qnan, X86_SIMD_F32, 0x1F80, 8,).bits,
        negative_three & 0x7FFF_FFFF
    );

    let denormal = 1u64;
    let one = u64::from(1.0f32.to_bits());
    for (a, b) in [(denormal, one), (one, denormal)] {
        let result = SmirInterpreter::x86_simd_range(a, b, X86_SIMD_F32, 0x1F80, 4);
        assert_eq!(result.bits, denormal);
        assert_eq!(result.status, 1 << 1);
    }
    let daz = SmirInterpreter::x86_simd_range(denormal, one, X86_SIMD_F32, 0x1FC0, 4);
    assert_eq!(daz.bits, positive_zero);
    assert_eq!(daz.status, 0);
    for (a, b) in [(denormal, second_qnan), (first_qnan, denormal)] {
        let result = SmirInterpreter::x86_simd_range(a, b, X86_SIMD_F32, 0x1F80, 4);
        assert_eq!(result.bits, denormal);
        assert_eq!(result.status, 0, "qNaN counterpart suppresses DE");
    }
}
