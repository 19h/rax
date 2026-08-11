//! tests::riscv tests

use super::*;
use crate::smir::interpret::*;
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::flags::{FlagSet, FlagUpdate, MaterializedFlags};
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::types::ShiftOp;

#[test]
fn rvfp_invalid_rounding_mode_traps_without_writes() {
    let dst = VReg::Virtual(VirtualId(1));
    let fcsr_dst = VReg::Virtual(VirtualId(2));
    let src1 = VReg::Virtual(VirtualId(3));
    let src2 = VReg::Virtual(VirtualId(4));
    let src3 = VReg::Virtual(VirtualId(5));
    let fcsr_src = VReg::Virtual(VirtualId(6));
    let mut ctx = SmirContext::new_riscv();
    ctx.pc = 0x2000;
    ctx.write_vreg(dst, 0x1111);
    ctx.write_vreg(fcsr_dst, 0x2222);
    ctx.write_vreg(src1, 0xffff_ffff_3f80_0000); // boxed 1.0f
    ctx.write_vreg(src2, 0xffff_ffff_4000_0000); // boxed 2.0f
    ctx.write_vreg(src3, 0);
    ctx.write_vreg(fcsr_src, 0);

    let block = SmirBlock {
        id: BlockId(0),
        guest_pc: 0x2000,
        phis: vec![],
        ops: vec![SmirOp::new(
            OpId(0),
            0x2004,
            OpKind::RvFp {
                dst,
                fcsr_dst,
                src1,
                src2,
                src3,
                fcsr_src,
                op: crate::isa::riscv::Op::FaddS,
                rm_field: 0b101,
                xlen: 64,
            },
        )],
        terminator: Terminator::Trap {
            kind: TrapKind::Halt,
        },
        exec_count: 0,
    };
    let interp = SmirInterpreter::new();
    let mut memory = FlatMemory::new(0x1000);

    let exit = interp.execute_block(&mut ctx, &mut memory, &block);

    assert!(matches!(
        exit,
        BlockResult::Exit(ExitReason::Undefined {
            addr: 0x2004,
            opcode: 0
        })
    ));
    assert_eq!(ctx.read_vreg(dst), 0x1111);
    assert_eq!(ctx.read_vreg(fcsr_dst), 0x2222);
    assert!(ctx.exit_reason.is_none());
}
#[test]
fn rvfp_rv32_integer_destination_is_zero_extended_to_xlen() {
    let dst = VReg::Virtual(VirtualId(1));
    let src1 = VReg::Virtual(VirtualId(2));
    let src2 = VReg::Virtual(VirtualId(3));
    let src3 = VReg::Virtual(VirtualId(4));
    let fcsr = VReg::Virtual(VirtualId(5));
    let mut ctx = SmirContext::new_riscv();
    ctx.write_vreg(src1, 0xffff_ffff_bfc0_0000); // boxed -1.5f
    ctx.write_vreg(src2, 0);
    ctx.write_vreg(src3, 0);
    ctx.write_vreg(fcsr, 0);

    let interp = SmirInterpreter::new();
    let mut memory = FlatMemory::new(0x1000);
    interp
        .execute_op(
            &mut ctx,
            &mut memory,
            &SmirOp::new(
                OpId(0),
                0x1000,
                OpKind::RvFp {
                    dst,
                    fcsr_dst: fcsr,
                    src1,
                    src2,
                    src3,
                    fcsr_src: fcsr,
                    op: crate::isa::riscv::Op::FcvtWS,
                    rm_field: 1,
                    xlen: 32,
                },
            ),
        )
        .unwrap();

    assert_eq!(ctx.read_vreg(dst), 0xffff_ffff);
    assert_eq!(ctx.read_vreg(fcsr) & 1, 1);
}
#[test]
fn rv_vector_load_uses_current_scalar_vreg_address() {
    let current_x10 = VReg::Virtual(VirtualId(7));
    let mut ctx = SmirContext::new_riscv();
    let mut memory = FlatMemory::new(0x1000);
    let stale_addr = 0x100;
    let current_addr = 0x200;
    let stale_lane = 0x1111_2222u32.to_le_bytes();
    let current_lane = 0xAABB_CCDDu32.to_le_bytes();

    ctx.write_arch_reg(ArchReg::RiscV(RiscVReg::X(10)), stale_addr);
    ctx.write_vreg(current_x10, current_addr);
    ctx.write_arch_reg(ArchReg::RiscV(RiscVReg::Csr(0xc20)), 1); // vl
    ctx.write_arch_reg(ArchReg::RiscV(RiscVReg::Csr(0xc21)), 0x10); // e32,m1
    memory.write(stale_addr, &stale_lane).unwrap();
    memory.write(current_addr, &current_lane).unwrap();

    // vle32.v v1,(a0)
    let insn = (1 << 25) | (10 << 15) | (6 << 12) | (1 << 7) | 0x07;
    let state = rv_vector_test_state(current_x10);
    exec_rv_vector(&mut ctx, &mut memory, insn, 64, 0, &state);

    let ArchRegState::RiscV(rv) = &ctx.arch_regs else {
        panic!("expected RISC-V context");
    };
    assert_eq!(&rv.v[1][0..4], &current_lane);
    assert_ne!(&rv.v[1][0..4], &stale_lane);
}

#[test]
fn rv_vector_vfncvt_fp16_to_integer8_commits_values_and_flags() {
    let mut ctx = SmirContext::new_riscv();
    let mut memory = FlatMemory::new(0x1000);
    let x10 = VReg::Arch(ArchReg::RiscV(RiscVReg::X(10)));
    let fcsr = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0x003)));
    let vl = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0xc20)));
    let vtype = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0xc21)));
    ctx.write_vreg(fcsr, 0);
    ctx.write_vreg(vl, 4);
    ctx.write_vreg(vtype, 0); // e8,m1

    let ArchRegState::RiscV(rv) = &mut ctx.arch_regs else {
        panic!("expected RISC-V context");
    };
    rv.v[1] = [0xa5; 16];
    for (lane, bits) in [0x3e00u16, 0xbc00, 0x5c00, 0x7e00].into_iter().enumerate() {
        rv.v[2][lane * 2..lane * 2 + 2].copy_from_slice(&bits.to_le_bytes());
    }

    // vfncvt.xu.f.w v1,v2: {1.5, -1, 256, qNaN} -> {2, 0, 255, 255}.
    let insn = (0b010010 << 26)
        | (1 << 25)
        | (2 << 20)
        | (0b10000 << 15)
        | (0b001 << 12)
        | (1 << 7)
        | 0x57;
    exec_rv_vector(
        &mut ctx,
        &mut memory,
        insn,
        64,
        0x1080,
        &rv_vector_test_state(x10),
    );

    assert!(ctx.exit_reason.is_none());
    let ArchRegState::RiscV(rv) = &ctx.arch_regs else {
        panic!("expected RISC-V context");
    };
    assert_eq!(&rv.v[1][0..4], &[2, 0, u8::MAX, u8::MAX]);
    assert_eq!(
        ctx.read_vreg(fcsr),
        u64::from(crate::isa::riscv::float::fflags::NX | crate::isa::riscv::float::fflags::NV)
    );
    assert_eq!(ctx.read_vreg(vl), 4);
    assert_eq!(ctx.read_vreg(vtype), 0);
}

#[test]
fn rv_vector_reserved_encoding_exits_without_committing_state() {
    let mut ctx = SmirContext::new_riscv();
    let mut memory = FlatMemory::new(0x2000);
    let x10 = VReg::Arch(ArchReg::RiscV(RiscVReg::X(10)));
    let fcsr = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0x003)));
    let vl = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0xc20)));
    let vtype = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0xc21)));
    ctx.write_vreg(fcsr, 7 << 5); // reserved frm=111
    ctx.write_vreg(vl, 0); // must not suppress frm validation
    ctx.write_vreg(vtype, 0x10); // e32,m1

    let before = [0xa5; 16];
    let ArchRegState::RiscV(rv) = &mut ctx.arch_regs else {
        panic!("expected RISC-V context");
    };
    rv.v[1] = before;

    // vfsgnj.vv v1,v2,v3 is exact, but every OPFVV/OPFVF instruction still
    // requires frm to hold an architecturally valid encoding.
    let insn =
        (0b001000 << 26) | (1 << 25) | (2 << 20) | (3 << 15) | (0b001 << 12) | (1 << 7) | 0x57;
    exec_rv_vector(
        &mut ctx,
        &mut memory,
        insn,
        64,
        0x1000,
        &rv_vector_test_state(x10),
    );

    assert!(matches!(
        ctx.exit_reason,
        Some(ExitReason::Undefined {
            addr: 0x1000,
            opcode
        }) if opcode == insn
    ));
    let ArchRegState::RiscV(rv) = &ctx.arch_regs else {
        panic!("expected RISC-V context");
    };
    assert_eq!(rv.v[1], before);
    assert_eq!(ctx.read_vreg(fcsr), 7 << 5);
    assert_eq!(ctx.read_vreg(vl), 0);
    assert_eq!(ctx.read_vreg(vtype), 0x10);
}

#[test]
fn rv_vector_widening_overlap_exits_without_committing_state() {
    let mut ctx = SmirContext::new_riscv();
    let mut memory = FlatMemory::new(0x2000);
    let x10 = VReg::Arch(ArchReg::RiscV(RiscVReg::X(10)));
    let vl = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0xc20)));
    let vtype = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0xc21)));
    ctx.write_vreg(vl, 1);
    ctx.write_vreg(vtype, 0x10); // e32,m1

    let before_v0 = [0x5a; 16];
    let before_v1 = [0xa5; 16];
    let ArchRegState::RiscV(rv) = &mut ctx.arch_regs else {
        panic!("expected RISC-V context");
    };
    rv.v[0] = before_v0;
    rv.v[1] = before_v1;

    // vwadd.vv v0,v0,v2: wide vd={v0,v1}, while narrow vs2=v0 overlaps
    // its low part. The trap must occur before either destination register is
    // committed through the opaque RVV interpreter bridge.
    let insn = (0b110000 << 26) | (1 << 25) | (2 << 15) | (0b010 << 12) | 0x57;
    exec_rv_vector(
        &mut ctx,
        &mut memory,
        insn,
        64,
        0x1100,
        &rv_vector_test_state(x10),
    );

    assert!(matches!(
        ctx.exit_reason,
        Some(ExitReason::Undefined {
            addr: 0x1100,
            opcode
        }) if opcode == insn
    ));
    let ArchRegState::RiscV(rv) = &ctx.arch_regs else {
        panic!("expected RISC-V context");
    };
    assert_eq!(rv.v[0], before_v0);
    assert_eq!(rv.v[1], before_v1);
    assert_eq!(ctx.read_vreg(vl), 1);
    assert_eq!(ctx.read_vreg(vtype), 0x10);
}

#[test]
fn rv_vector_fp_sew8_exits_without_committing_state() {
    let mut ctx = SmirContext::new_riscv();
    let mut memory = FlatMemory::new(0x2000);
    let x10 = VReg::Arch(ArchReg::RiscV(RiscVReg::X(10)));
    let vl = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0xc20)));
    let vtype = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0xc21)));
    ctx.write_vreg(vl, 1);
    ctx.write_vreg(vtype, 0x00); // e8,m1

    let before_v1 = [0x5a; 16];
    let ArchRegState::RiscV(rv) = &mut ctx.arch_regs else {
        panic!("expected RISC-V context");
    };
    rv.v[1] = before_v1;

    // vfadd.vv v1,v2,v3 would consume unsupported FP8 operands. The opaque
    // RVV bridge must reject it at the instruction frontier before committing
    // any destination state.
    let insn = (1 << 25) | (2 << 20) | (3 << 15) | (0b001 << 12) | (1 << 7) | 0x57;
    exec_rv_vector(
        &mut ctx,
        &mut memory,
        insn,
        64,
        0x1200,
        &rv_vector_test_state(x10),
    );

    assert!(matches!(
        ctx.exit_reason,
        Some(ExitReason::Undefined {
            addr: 0x1200,
            opcode
        }) if opcode == insn
    ));
    let ArchRegState::RiscV(rv) = &ctx.arch_regs else {
        panic!("expected RISC-V context");
    };
    assert_eq!(rv.v[1], before_v1);
    assert_eq!(ctx.read_vreg(vl), 1);
    assert_eq!(ctx.read_vreg(vtype), 0x00);
}
