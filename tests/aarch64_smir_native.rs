//! End-to-end AArch64-on-AArch64 SMIR JIT: lift real AArch64 machine code to
//! SMIR, lower it with the native `Aarch64Lowerer` (identity register map), map
//! it W^X, and execute it on the host through `ExecMem::run_aarch64_identity`.
//!
//! Until now the native AArch64 lowerer was only validated as *bytes* against a
//! QEMU oracle (tests/arm_diff.rs) — never actually executed. These tests run
//! the lowered code on real hardware and check architectural results, proving
//! the lift → lower → W^X-map → run → marshal-back pipeline.
//!
//! Gated to aarch64 hosts with the `smir-jit` feature (the executor only exists
//! there). Register-only blocks for now (the clobber-safe core); memory/FP/
//! native-exit modes land with the lowerer ABI work.
#![cfg(all(feature = "smir-jit", target_arch = "aarch64"))]

use std::collections::HashMap;

use rax::smir::ir::{FunctionBuilder, Terminator};
use rax::smir::lift::aarch64::Aarch64Lifter;
use rax::smir::lift::{LiftContext, SmirLifter};
use rax::smir::lower::SmirLowerer;
use rax::smir::lower::aarch64::Aarch64Lowerer;
use rax::smir::lower::runtime::{Aarch64GuestRegs, ExecMem};
use rax::smir::ops::OpKind;
use rax::smir::types::{
    Address, ArchReg, ArmReg, FunctionId, MemWidth, SignExtend, SourceArch, VReg,
};

use rax::arm::{AArch64Config, AArch64Cpu, ArmCpu, CpuExit, FlatMemory};

fn xr(n: u8) -> VReg {
    VReg::Arch(ArchReg::Arm(ArmReg::X(n)))
}

/// Lift `insns` (consecutive 4-byte AArch64 words) into one straight-line SMIR
/// block, lower it natively, execute it over `regs`, and write results back.
fn jit_run(insns: &[u32], regs: &mut Aarch64GuestRegs) -> Result<(), String> {
    let mut lifter = Aarch64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::Aarch64);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    for (i, &insn) in insns.iter().enumerate() {
        let pc = (i * 4) as u64;
        let lifted = lifter
            .lift_insn(pc, &insn.to_le_bytes(), &mut ctx)
            .map_err(|e| format!("lift #{i} ({insn:#010x}) failed: {e:?}"))?;
        for op in lifted.ops {
            builder.push_op(op.guest_pc, op.kind);
        }
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = Aarch64Lowerer::new();
    let result = lowerer
        .lower_function(&func)
        .map_err(|e| format!("lower failed: {e:?}"))?;
    let code = lowerer
        .finalize()
        .map_err(|e| format!("finalize failed: {e:?}"))?;
    let mem = ExecMem::new(&code).map_err(|e| format!("exec map failed: {e:?}"))?;
    // Pick the FP trampoline iff any op touches a V register.
    let touches_v = |v: &VReg| matches!(v, VReg::Arch(ArchReg::Arm(ArmReg::V(_))));
    let uses_fp = func.blocks.iter().flat_map(|b| &b.ops).any(|op| {
        op.kind.dests().iter().any(touches_v) || op.kind.source_vregs().iter().any(touches_v)
    });
    if uses_fp {
        mem.run_aarch64_identity_fp(result.entry_offset, regs);
    } else {
        mem.run_aarch64_identity(result.entry_offset, regs);
    }
    Ok(())
}

fn run(insns: &[u32], setup: impl FnOnce(&mut Aarch64GuestRegs)) -> Aarch64GuestRegs {
    let mut regs = Aarch64GuestRegs::default();
    setup(&mut regs);
    jit_run(insns, &mut regs).expect("jit_run");
    regs
}

#[test]
fn add_register() {
    // 8b020020  add x0, x1, x2
    let r = run(&[0x8b02_0020], |g| {
        g.x[1] = 40;
        g.x[2] = 2;
    });
    assert_eq!(r.x[0], 42);
}

#[test]
fn sub_register() {
    // cb020020  sub x0, x1, x2
    let r = run(&[0xcb02_0020], |g| {
        g.x[1] = 100;
        g.x[2] = 58;
    });
    assert_eq!(r.x[0], 42);
}

#[test]
fn logical_and_orr() {
    // 8a020020  and x0, x1, x2
    let r = run(&[0x8a02_0020], |g| {
        g.x[1] = 0xff0f;
        g.x[2] = 0x0ff0;
    });
    assert_eq!(r.x[0], 0x0f00);

    // aa020020  orr x0, x1, x2
    let r = run(&[0xaa02_0020], |g| {
        g.x[1] = 0xf0;
        g.x[2] = 0x0f;
    });
    assert_eq!(r.x[0], 0xff);
}

#[test]
fn multi_instruction_block_chains_through_arch_regs() {
    // 8b020023  add x3, x1, x2
    // cb010060  sub x0, x3, x1   => x0 = (x1 + x2) - x1 = x2
    let r = run(&[0x8b02_0023, 0xcb01_0060], |g| {
        g.x[1] = 1000;
        g.x[2] = 42;
    });
    assert_eq!(r.x[3], 1042);
    assert_eq!(r.x[0], 42);
}

#[test]
fn mul() {
    // 9b027c20  mul x0, x1, x2  (madd x0,x1,x2,xzr)
    let r = run(&[0x9b02_7c20], |g| {
        g.x[1] = 6;
        g.x[2] = 7;
    });
    assert_eq!(r.x[0], 42);
}

#[test]
fn flags_subs_then_cset() {
    // eb02_0020  subs x0, x1, x2   (sets NZCV)
    // 9a9f_17e3  cset x3, eq       (x3 = (x1==x2) ? 1 : 0)
    let eq = run(&[0xeb02_0020, 0x9a9f_17e3], |g| {
        g.x[1] = 7;
        g.x[2] = 7;
    });
    assert_eq!(eq.x[0], 0, "7 - 7 == 0");
    assert_eq!(eq.x[3], 1, "Z set => cset eq = 1");

    let ne = run(&[0xeb02_0020, 0x9a9f_17e3], |g| {
        g.x[1] = 9;
        g.x[2] = 7;
    });
    assert_eq!(ne.x[0], 2);
    assert_eq!(ne.x[3], 0, "Z clear => cset eq = 0");
}

#[test]
fn high_callee_saved_regs() {
    // Exercises the trampoline's single-ldr/str marshaling of x19..x29
    // (distinct from the ldp-paired x0..x17 path).
    // 8b150293  add x19, x20, x21
    // aa1303e0  mov x0, x19
    let r = run(&[0x8b15_0293, 0xaa13_03e0], |g| {
        g.x[20] = 300;
        g.x[21] = 33;
    });
    assert_eq!(r.x[19], 333);
    assert_eq!(r.x[0], 333);
}

#[test]
fn movz_builds_constant() {
    // d2824680  movz x0, #0x1234
    let r = run(&[0xd282_4680], |_g| {});
    assert_eq!(r.x[0], 0x1234);
}

// Multi-block region with a native-exit stub: the entry block computes
// `add x0, x1, x2` then unconditionally branches to a frontier block that is
// marked as a native exit. The exit stub must record its resume guest PC into
// Aarch64GuestRegs.pc and return to the trampoline, while the entry block's
// result survives. Proves: native_exits short-circuit in lower_block, the
// intra-region branch landing on the stub, the scratch spill/restore, and the
// PC marshal-back.
#[test]
fn native_exit_stub_records_resume_pc() {
    const RESUME_PC: u64 = 0x4000;

    let mut lifter = Aarch64Lifter::new();
    let mut ctx = LiftContext::new(SourceArch::Aarch64);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0);

    // Frontier/exit block (created first so the entry can branch to it).
    let exit_blk = builder.create_block(RESUME_PC);

    // Entry block: add x0, x1, x2  (8b020020), then Branch -> exit_blk.
    let lifted = lifter
        .lift_insn(0, &0x8b02_0020u32.to_le_bytes(), &mut ctx)
        .expect("lift add");
    for op in lifted.ops {
        builder.push_op(op.guest_pc, op.kind);
    }
    builder.set_terminator(Terminator::Branch { target: exit_blk });

    // Exit block body is irrelevant (replaced by the stub); give it a terminator.
    builder.switch_to_block(exit_blk);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut exits = HashMap::new();
    exits.insert(exit_blk, RESUME_PC);

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.set_native_exits(exits);
    let result = lowerer.lower_function(&func).expect("lower");
    let code = lowerer.finalize().expect("finalize");
    let mem = ExecMem::new(&code).expect("map");

    let mut regs = Aarch64GuestRegs::default();
    regs.x[1] = 40;
    regs.x[2] = 2;
    regs.x[5] = 0xdead_beef; // unrelated live reg must survive
    regs.pc = 0; // proves the stub actually writes it
    mem.run_aarch64_identity(result.entry_offset, &mut regs);

    assert_eq!(regs.x[0], 42, "entry block's add executed");
    assert_eq!(regs.pc, RESUME_PC, "exit stub recorded the resume PC");
    assert_eq!(
        regs.x[5], 0xdead_beef,
        "unrelated reg preserved across the stub"
    );
}

// A conditional branch where BOTH targets are native exits with distinct resume
// PCs: verifies the structural CondBranch->stub handling (no special terminator
// code) and that the taken edge selects the right resume PC.
#[test]
fn native_exit_conditional_selects_resume_pc() {
    const PC_EQ: u64 = 0x1000;
    const PC_NE: u64 = 0x2000;

    let build_and_run = |x1: u64, x2: u64| -> Aarch64GuestRegs {
        let mut lifter = Aarch64Lifter::new();
        let mut ctx = LiftContext::new(SourceArch::Aarch64);
        let mut builder = FunctionBuilder::new(FunctionId(0), 0);
        let blk_eq = builder.create_block(PC_EQ);
        let blk_ne = builder.create_block(PC_NE);

        // Entry: cmp x1, x2 ; b.eq blk_eq ; b blk_ne.
        // Lift `subs xzr, x1, x2` (cmp) = 0xeb02003f to set NZCV, then lift
        // `b.eq #target` to obtain the cond-branch SMIR shape... simpler: build
        // the compare op via lifting cmp, then a manual CondBranch with a folded
        // TestCondition.
        let lifted = lifter
            .lift_insn(0, &0xeb02_003fu32.to_le_bytes(), &mut ctx)
            .expect("lift cmp");
        for op in lifted.ops {
            builder.push_op(op.guest_pc, op.kind);
        }
        // TestCondition feeding the CondBranch (folded into B.eq by the lowerer).
        let cond = ctx.alloc_vreg();
        builder.push_op(
            0,
            rax::smir::ops::OpKind::TestCondition {
                dst: cond,
                cond: rax::smir::types::Condition::Eq,
            },
        );
        builder.set_terminator(Terminator::CondBranch {
            cond,
            true_target: blk_eq,
            false_target: blk_ne,
        });
        builder.switch_to_block(blk_eq);
        builder.set_terminator(Terminator::Return { values: vec![] });
        builder.switch_to_block(blk_ne);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let func = builder.finish();

        let mut exits = HashMap::new();
        exits.insert(blk_eq, PC_EQ);
        exits.insert(blk_ne, PC_NE);

        let mut lowerer = Aarch64Lowerer::new();
        lowerer.set_native_exits(exits);
        let result = lowerer.lower_function(&func).expect("lower");
        let code = lowerer.finalize().expect("finalize");
        let mem = ExecMem::new(&code).expect("map");

        let mut regs = Aarch64GuestRegs::default();
        regs.x[1] = x1;
        regs.x[2] = x2;
        mem.run_aarch64_identity(result.entry_offset, &mut regs);
        regs
    };

    assert_eq!(build_and_run(7, 7).pc, PC_EQ, "x1==x2 takes the eq exit");
    assert_eq!(build_and_run(9, 7).pc, PC_NE, "x1!=x2 takes the ne exit");
}

// ---- Memory-helper call-out tests (mem_helpers mode) ----------------------

/// AAPCS64 16-byte return: value in x0, ok in x1.
#[repr(C)]
struct LoadRet {
    value: u64,
    ok: u64,
}

#[repr(C)]
struct TestMemCtx {
    /// When non-zero, the helpers report a fault (ok = 0).
    fault: u64,
}

extern "C" fn test_load(ctx: *mut TestMemCtx, addr: u64, size: u32, signed: u32) -> LoadRet {
    if unsafe { (*ctx).fault } != 0 {
        return LoadRet { value: 0, ok: 0 };
    }
    let value = unsafe {
        match size {
            1 => {
                let v = *(addr as *const u8);
                if signed != 0 {
                    v as i8 as i64 as u64
                } else {
                    v as u64
                }
            }
            2 => {
                let v = *(addr as *const u16);
                if signed != 0 {
                    v as i16 as i64 as u64
                } else {
                    v as u64
                }
            }
            4 => {
                let v = *(addr as *const u32);
                if signed != 0 {
                    v as i32 as i64 as u64
                } else {
                    v as u64
                }
            }
            _ => *(addr as *const u64),
        }
    };
    LoadRet { value, ok: 1 }
}

extern "C" fn test_store(ctx: *mut TestMemCtx, addr: u64, value: u64, size: u32) -> u64 {
    if unsafe { (*ctx).fault } != 0 {
        return 0;
    }
    unsafe {
        match size {
            1 => *(addr as *mut u8) = value as u8,
            2 => *(addr as *mut u16) = value as u16,
            4 => *(addr as *mut u32) = value as u32,
            _ => *(addr as *mut u64) = value,
        }
    }
    1
}

// Load through the MMU helper, then Store through it: copies src→dst. Proves the
// full call-out path — spill-all to the struct, LR save/restore around `blr`,
// arg marshaling (ctx/addr/size/signed), the (value,ok) return, value delivery
// into the dst slot, and reload preserving unrelated live registers.
#[test]
fn mem_helper_load_store_copies() {
    let src: u64 = 0xCAFE_F00D_1234_5678;
    let mut dst: u64 = 0;
    let mut ctx = TestMemCtx { fault: 0 };

    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    let exit_blk = builder.create_block(0x9000);
    builder.push_op(
        0,
        OpKind::Load {
            dst: xr(0),
            addr: Address::Direct(xr(1)),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
    );
    builder.push_op(
        4,
        OpKind::Store {
            src: xr(0),
            addr: Address::Direct(xr(2)),
            width: MemWidth::B8,
        },
    );
    builder.set_terminator(Terminator::Branch { target: exit_blk });
    builder.switch_to_block(exit_blk);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut exits = HashMap::new();
    exits.insert(exit_blk, 0x9000u64);
    let mut lowerer = Aarch64Lowerer::new();
    lowerer.set_native_exits(exits);
    lowerer.set_mem_helpers(true);
    let result = lowerer.lower_function(&func).expect("lower");
    let code = lowerer.finalize().expect("finalize");
    let mem = ExecMem::new(&code).expect("map");

    let mut regs = Aarch64GuestRegs::default();
    regs.x[1] = &src as *const u64 as u64;
    regs.x[2] = &mut dst as *mut u64 as u64;
    regs.x[7] = 0x7777_7777; // unrelated live reg must survive spill/reload
    regs.ctx = &mut ctx as *mut TestMemCtx as u64;
    regs.load_fn = test_load as usize as u64;
    regs.store_fn = test_store as usize as u64;
    mem.run_aarch64_identity(result.entry_offset, &mut regs);

    assert_eq!(dst, 0xCAFE_F00D_1234_5678, "store landed via helper");
    assert_eq!(
        regs.x[0], 0xCAFE_F00D_1234_5678,
        "loaded value delivered to x0"
    );
    assert_eq!(
        regs.x[7], 0x7777_7777,
        "unrelated reg preserved across spill/reload"
    );
    assert_eq!(regs.pc, 0x9000, "exited at the frontier resume PC");
}

// A faulting load (helper returns ok=0) must record the faulting op's guest PC
// and bail before the store, leaving guest state uncommitted.
#[test]
fn mem_helper_load_fault_records_pc() {
    const LOAD_PC: u64 = 0x40;
    let probe: u64 = 0xdead;
    let mut dst: u64 = 0;
    let mut ctx = TestMemCtx { fault: 1 }; // load will report a fault

    let mut builder = FunctionBuilder::new(FunctionId(0), 0);
    let exit_blk = builder.create_block(0x9000);
    builder.push_op(
        LOAD_PC,
        OpKind::Load {
            dst: xr(0),
            addr: Address::Direct(xr(1)),
            width: MemWidth::B8,
            sign: SignExtend::Zero,
        },
    );
    builder.push_op(
        LOAD_PC + 4,
        OpKind::Store {
            src: xr(0),
            addr: Address::Direct(xr(2)),
            width: MemWidth::B8,
        },
    );
    builder.set_terminator(Terminator::Branch { target: exit_blk });
    builder.switch_to_block(exit_blk);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut exits = HashMap::new();
    exits.insert(exit_blk, 0x9000u64);
    let mut lowerer = Aarch64Lowerer::new();
    lowerer.set_native_exits(exits);
    lowerer.set_mem_helpers(true);
    let result = lowerer.lower_function(&func).expect("lower");
    let code = lowerer.finalize().expect("finalize");
    let mem = ExecMem::new(&code).expect("map");

    let mut regs = Aarch64GuestRegs::default();
    regs.x[0] = 0xABCD; // sentinel: must be untouched on fault
    regs.x[1] = &probe as *const u64 as u64;
    regs.x[2] = &mut dst as *mut u64 as u64;
    regs.ctx = &mut ctx as *mut TestMemCtx as u64;
    regs.load_fn = test_load as usize as u64;
    regs.store_fn = test_store as usize as u64;
    mem.run_aarch64_identity(result.entry_offset, &mut regs);

    assert_eq!(regs.pc, LOAD_PC, "faulting load recorded its own guest PC");
    assert_eq!(regs.x[0], 0xABCD, "dst register uncommitted on fault");
    assert_eq!(dst, 0, "store never executed after the load fault");
}

// ---- End-to-end: JIT tier inside the AArch64 emulator vs the interpreter ----

const PROG_BASE: u64 = 0x1000;
/// Sentinel return address: the loops end in `ret`, so once X30 lands in PC the
/// program is done. It is never executed (the harness stops first).
const DONE_PC: u64 = 0x00DE_AD00;

/// Drive `cpu` from PROG_BASE until PC reaches DONE_PC (the `ret` target) or a
/// step budget is exhausted.
fn drive_to_done(cpu: &mut AArch64Cpu) {
    cpu.set_x(30, DONE_PC); // return address for the terminating `ret`
    cpu.set_pc(PROG_BASE);
    // Runaway cap; large enough for the interpreter to finish the benchmark's
    // multi-million-iteration loop (≈3 steps/iter) without false-tripping.
    for _ in 0..64_000_000u64 {
        if cpu.get_pc() == DONE_PC {
            return;
        }
        match cpu.step_system() {
            Ok(_) => {}
            Err(e) => panic!("cpu error: {e:?}"),
        }
    }
    panic!("program did not reach DONE_PC (pc={:#x})", cpu.get_pc());
}

fn load_prog(cpu: &mut AArch64Cpu, prog: &[u32]) {
    let mut bytes = Vec::with_capacity(prog.len() * 4);
    for &w in prog {
        bytes.extend_from_slice(&w.to_le_bytes());
    }
    cpu.write_memory(PROG_BASE, &bytes).unwrap();
}

fn fresh_cpu() -> AArch64Cpu {
    let mem = FlatMemory::new(0, 0x0100_0000);
    AArch64Cpu::new(AArch64Config::default(), Box::new(mem))
}

// A register-only hot loop: x0 = sum(1..=1000). Runs >64 back-edges so the JIT
// promotes the loop head, compiles the self-looping region (CondBranch back to
// itself, `ret` frontier), and executes the tail natively. The JIT result must
// equal the pure-interpreter result.
#[test]
fn e2e_register_hot_loop_matches_interpreter() {
    // movz x0,#0 ; movz x1,#1000 ; (loop) add x0,x0,x1 ; subs x1,x1,#1 ;
    // b.ne loop ; ret
    let prog: [u32; 6] = [
        0xd280_0000, // movz x0, #0
        0xd280_7d01, // movz x1, #1000
        0x8b01_0000, // add  x0, x0, x1     ; loop head @ PROG_BASE+0x8
        0xf100_0421, // subs x1, x1, #1
        0x54ff_ffc1, // b.ne loop (-8)
        0xd65f_03c0, // ret
    ];

    let mut interp = fresh_cpu();
    interp.set_jit_enabled(false);
    load_prog(&mut interp, &prog);
    drive_to_done(&mut interp);

    let mut jit = fresh_cpu();
    jit.set_jit_enabled(true);
    load_prog(&mut jit, &prog);
    drive_to_done(&mut jit);

    assert_eq!(interp.get_x(0), 500_500, "sum(1..=1000)");
    assert_eq!(interp.get_x(1), 0);
    for i in 0..31u8 {
        assert_eq!(
            jit.get_x(i),
            interp.get_x(i),
            "X{i} diverged: jit={:#x} interp={:#x}",
            jit.get_x(i),
            interp.get_x(i)
        );
    }
}

// A memory-touching hot loop summing an in-guest-memory array through the MMU
// helper path (set_jit_mem(true)). Validates the full memory-helper call-out
// end-to-end inside the emulator, differentially against the interpreter.
#[test]
fn e2e_memory_hot_loop_matches_interpreter() {
    const ARRAY: u64 = 0x4000;
    const N: u64 = 500;

    // x0=sum(0), x1=ptr(ARRAY), x2=count(N):
    //   loop: ldr x3,[x1] ; add x0,x0,x3 ; add x1,x1,#8 ; subs x2,x2,#1 ; b.ne loop ; ret
    let prog: [u32; 9] = [
        0xd280_0000, // movz x0, #0
        0xd288_0001, // movz x1, #0x4000   (ARRAY)
        0xd280_3e82, // movz x2, #500      (N)
        0xf940_0023, // ldr  x3, [x1]      ; loop head @ +0xC
        0x8b03_0000, // add  x0, x0, x3
        0x9100_2021, // add  x1, x1, #8
        0xf100_0442, // subs x2, x2, #1
        0x54ff_ff81, // b.ne loop (-16, back to the ldr at +0xC)
        0xd65f_03c0, // ret
    ];

    // Expected sum of the array values we fill in: a[i] = i*3 + 7.
    let fill = |cpu: &mut AArch64Cpu| {
        for i in 0..N {
            let v: u64 = i.wrapping_mul(3).wrapping_add(7);
            cpu.write_memory(ARRAY + i * 8, &v.to_le_bytes()).unwrap();
        }
    };
    let expected: u64 = (0..N).map(|i| i.wrapping_mul(3).wrapping_add(7)).sum();

    let mut interp = fresh_cpu();
    interp.set_jit_enabled(false);
    load_prog(&mut interp, &prog);
    fill(&mut interp);
    drive_to_done(&mut interp);

    let mut jit = fresh_cpu();
    jit.set_jit_enabled(true);
    jit.set_jit_mem(true);
    load_prog(&mut jit, &prog);
    fill(&mut jit);
    drive_to_done(&mut jit);

    assert_eq!(interp.get_x(0), expected, "interpreter sums the array");
    for i in 0..31u8 {
        assert_eq!(jit.get_x(i), interp.get_x(i), "X{i} diverged (memory JIT)");
    }
}

// Self-modifying code: after a loop is JIT-compiled, a guest store into its code
// page must invalidate the cached region so a re-run reflects the patched
// instruction. The loop body `add x0,x0,#1` is rewritten to `add x0,x0,#2`
// through the guest store path (mem_write_u32, which feeds the SMC journal).
#[test]
fn e2e_smc_invalidates_stale_region() {
    // loop head @ PROG_BASE: add x0,x0,#1 ; subs x1,x1,#1 ; b.ne head ; ret
    let prog: [u32; 4] = [
        0x9100_0400, // add  x0, x0, #1   ; <- patched below
        0xf100_0421, // subs x1, x1, #1
        0x54ff_ffc1, // b.ne head (-8)
        0xd65f_03c0, // ret
    ];

    let mut cpu = fresh_cpu();
    cpu.set_jit_enabled(true);
    load_prog(&mut cpu, &prog);

    // Pass 1: 200 iterations of +1 -> x0 = 200, and the loop head is JIT'd.
    cpu.set_x(0, 0);
    cpu.set_x(1, 200);
    drive_to_done(&mut cpu);
    assert_eq!(cpu.get_x(0), 200, "pass 1: +1 x 200");

    // Patch the loop body to `add x0,x0,#2` via the guest store path. This must
    // mark the cached region stale (it covers PROG_BASE's page).
    cpu.mem_write_u32(PROG_BASE, 0x9100_0800).unwrap();

    // Pass 2: if SMC invalidation works, the re-run uses +2 -> x0 = 400. A stale
    // cached region would still apply +1 and yield 200.
    cpu.set_x(0, 0);
    cpu.set_x(1, 200);
    drive_to_done(&mut cpu);
    assert_eq!(cpu.get_x(0), 400, "pass 2: SMC picked up the +2 patch");
}

// ---- Differential harness: SMIR-lowered native code vs the interpreter -------
//
// For each instruction sequence + input vector, run it two ways and compare the
// low GPRs: (a) through the native lowerer (jit_run / run_aarch64_identity), and
// (b) through the AArch64 interpreter. Sequences the lowerer declines to lower
// are skipped (they safely deopt to the interpreter in the emulator). This is
// the gold-standard correctness check for the lowerer across op classes the
// hand-written tests above don't individually cover.

fn interp_seq(insns: &[u32], xin: &[(u8, u64)]) -> [u64; 9] {
    let mut cpu = fresh_cpu();
    cpu.set_jit_enabled(false);
    let mut bytes = Vec::new();
    for &w in insns {
        bytes.extend_from_slice(&w.to_le_bytes());
    }
    bytes.extend_from_slice(&0xd65f_03c0u32.to_le_bytes()); // ret
    cpu.write_memory(PROG_BASE, &bytes).unwrap();
    for &(r, v) in xin {
        cpu.set_x(r, v);
    }
    cpu.set_x(30, DONE_PC);
    cpu.set_pc(PROG_BASE);
    for _ in 0..1000 {
        if cpu.get_pc() == DONE_PC {
            break;
        }
        cpu.step_system().unwrap();
    }
    let mut out = [0u64; 9];
    for i in 0..9 {
        out[i] = cpu.get_x(i as u8);
    }
    out
}

fn jit_seq(insns: &[u32], xin: &[(u8, u64)]) -> Option<[u64; 9]> {
    let mut regs = Aarch64GuestRegs::default();
    for &(r, v) in xin {
        regs.x[r as usize] = v;
    }
    jit_run(insns, &mut regs).ok()?;
    let mut out = [0u64; 9];
    for i in 0..9 {
        out[i] = regs.x[i];
    }
    Some(out)
}

fn diff_check(label: &str, insns: &[u32], inputs: &[&[(u8, u64)]]) {
    let mut lowered = false;
    for xin in inputs {
        let interp = interp_seq(insns, xin);
        if let Some(jit) = jit_seq(insns, xin) {
            lowered = true;
            assert_eq!(
                jit, interp,
                "{label}: JIT vs interpreter diverged\n  insns={insns:#010x?}\n  in={xin:?}\n  jit={jit:#x?}\n  interp={interp:#x?}"
            );
        }
    }
    // Not a hard failure if the lowerer declines (deopt is correct), but note it.
    if !lowered {
        eprintln!("[diff] {label}: lowerer declined all inputs (deopt path)");
    }
}

#[test]
fn differential_scalar_ops_vs_interpreter() {
    let vecs: &[&[(u8, u64)]] = &[
        &[(1, 0x0000_0000_0000_0003), (2, 0x0000_0000_0000_0004)],
        &[(1, 0xFFFF_FFFF_FFFF_FFFF), (2, 0x0000_0000_0000_0001)],
        &[(1, 0x8000_0000_0000_0000), (2, 0x0000_0000_0000_003F)],
        &[(1, 0x0123_4567_89AB_CDEF), (2, 0x0000_0000_0000_0011)],
    ];

    // Data-processing 2-source: shifts and divides.
    diff_check("lslv", &[0x9ac2_2020], vecs); // lsl x0,x1,x2
    diff_check("lsrv", &[0x9ac2_2420], vecs); // lsr x0,x1,x2
    diff_check("asrv", &[0x9ac2_2820], vecs); // asr x0,x1,x2
    diff_check("rorv", &[0x9ac2_2c20], vecs); // ror x0,x1,x2
    diff_check("udiv", &[0x9ac2_0820], vecs); // udiv x0,x1,x2
    diff_check("sdiv", &[0x9ac2_0c20], vecs); // sdiv x0,x1,x2

    // Data-processing 1-source: bit ops.
    diff_check("clz", &[0xdac0_1020], vecs); // clz  x0,x1
    diff_check("rbit", &[0xdac0_0020], vecs); // rbit x0,x1
    diff_check("rev", &[0xdac0_0c20], vecs); // rev  x0,x1

    // Bitfield extracts.
    diff_check("ubfx", &[0xd344_2c20], vecs); // ubfx x0,x1,#4,#8
    diff_check("sbfx", &[0x9344_2c20], vecs); // sbfx x0,x1,#4,#8
    // Bitfield insert (result depends on the dst's prior value).
    diff_check(
        "bfi",
        &[0xb37c_1c20], // bfi x0,x1,#4,#8
        &[
            &[(0, 0xFFFF_FFFF_FFFF_FFFF), (1, 0x0)],
            &[(0, 0x0), (1, 0xFF)],
            &[(0, 0xAAAA_AAAA_AAAA_AAAA), (1, 0x55)],
        ],
    );

    // Add/sub with carry (carry seeded by a preceding adds).
    // adds x3,x4,x5 ; adc x0,x1,x2
    diff_check(
        "adc",
        &[0xab05_0083, 0x9a02_0020],
        &[
            &[(1, 5), (2, 7), (4, u64::MAX), (5, 1)], // carry set
            &[(1, 5), (2, 7), (4, 1), (5, 1)],        // carry clear
        ],
    );
    // subs x3,x4,x5 ; sbc x0,x1,x2
    diff_check(
        "sbc",
        &[0xeb05_0083, 0xda02_0020],
        &[
            &[(1, 100), (2, 30), (4, 10), (5, 1)], // borrow
            &[(1, 100), (2, 30), (4, 10), (5, 10)],
        ],
    );

    // Conditional select off a compare.
    // cmp x1,x2 (subs xzr,x1,x2) ; csel x0,x3,x4,eq
    diff_check(
        "csel_eq",
        &[0xeb02_003f, 0x9a82_0060],
        &[
            &[(1, 7), (2, 7), (3, 0xAAAA), (4, 0xBBBB)], // eq -> x3
            &[(1, 9), (2, 7), (3, 0xAAAA), (4, 0xBBBB)], // ne -> x4
        ],
    );
    // cmp x1,x2 ; csinc x0,x3,x4,ne   (csinc x0,x3,x4,ne = 0x9a84_1060)
    diff_check(
        "csinc_ne",
        &[0xeb02_003f, 0x9a84_1060],
        &[
            &[(1, 7), (2, 7), (3, 0xAAAA), (4, 0xBBBB)],
            &[(1, 9), (2, 7), (3, 0xAAAA), (4, 0xBBBB)],
        ],
    );
}

// ---- Benchmark: JIT vs interpreter on a hot loop (perf evidence) ------------
//
// Not a pass/fail threshold (host-dependent, CI-flaky); it asserts the JIT
// result equals the interpreter and prints the wall-clock speedup. Run with
//   cargo test --test aarch64_smir_native bench_jit_speedup -- --nocapture
#[test]
fn bench_jit_speedup() {
    use std::time::Instant;

    // A long register-only countdown: add x0,x0,x1 ; subs x1,x1,#1 ; b.ne ; ret
    let prog: [u32; 4] = [0x8b01_0000, 0xf100_0421, 0x54ff_ffc1, 0xd65f_03c0];
    let iters: u64 = 5_000_000;

    let mut interp = fresh_cpu();
    interp.set_jit_enabled(false);
    load_prog(&mut interp, &prog);
    interp.set_x(0, 0);
    interp.set_x(1, iters);
    let t0 = Instant::now();
    drive_to_done(&mut interp);
    let interp_t = t0.elapsed();

    let mut jit = fresh_cpu();
    jit.set_jit_enabled(true);
    load_prog(&mut jit, &prog);
    jit.set_x(0, 0);
    jit.set_x(1, iters);
    let t1 = Instant::now();
    drive_to_done(&mut jit);
    let jit_t = t1.elapsed();

    let expected = iters * (iters + 1) / 2;
    assert_eq!(interp.get_x(0), expected, "interpreter sum");
    assert_eq!(jit.get_x(0), expected, "JIT sum matches");

    eprintln!(
        "[bench] {iters} iters: interp={:?} jit={:?} speedup={:.1}x",
        interp_t,
        jit_t,
        interp_t.as_secs_f64() / jit_t.as_secs_f64().max(1e-9)
    );
}

// ---- Scalar FP through the JIT (lift -> lower -> FP trampoline -> exec) ------
//
// jit_run auto-detects V-register usage and routes through the FP trampoline.
// These validate the full lift->lower->exec chain for the IEEE-exact scalar FP
// ops admitted by the clobber gate. Results are exact (no rounding ambiguity).

fn fp_run(insns: &[u32], setup: impl FnOnce(&mut Aarch64GuestRegs)) -> Aarch64GuestRegs {
    let mut regs = Aarch64GuestRegs::default();
    setup(&mut regs);
    jit_run(insns, &mut regs).expect("fp jit_run");
    regs
}

// Scalar FP through the JIT, now that the decoder's 2-source opcode table is
// fixed (decoder/aarch64.rs decode_scalar_fp_2source): both single- AND
// double-precision FADD/FSUB/FMUL/FDIV (+ FSQRT) lift correctly and run via the
// FP trampoline (V0-V31 + FPCR marshaled). Double-precision was entirely
// mis-decoded before the fix. Results are IEEE-exact, so hand-expected values
// are the oracle. (V_n.f64 lives in regs.v[2*n]; V_n.f32 in its low 32 bits.)
#[test]
fn fp_scalar_arith_lift_lower_exec() {
    // Single-precision fadd s0,s1,s2 (the one encoding that decoded pre-fix too).
    let r = fp_run(&[0x1e22_2820], |g| {
        g.v[2] = (2.5_f32).to_bits() as u64;
        g.v[4] = (4.0_f32).to_bits() as u64;
    });
    assert_eq!(f32::from_bits(r.v[0] as u32), 6.5, "fadd s0,s1,s2");

    // Double-precision: previously broken, now correct.
    let r = fp_run(&[0x1e62_2820], |g| {
        // fadd d0,d1,d2
        g.v[2] = (2.5_f64).to_bits();
        g.v[4] = (4.0_f64).to_bits();
    });
    assert_eq!(f64::from_bits(r.v[0]), 6.5, "fadd d0,d1,d2");

    let r = fp_run(&[0x1e62_3820], |g| {
        // fsub d0,d1,d2
        g.v[2] = (10.0_f64).to_bits();
        g.v[4] = (3.5_f64).to_bits();
    });
    assert_eq!(f64::from_bits(r.v[0]), 6.5, "fsub d0,d1,d2");

    let r = fp_run(&[0x1e62_0820], |g| {
        // fmul d0,d1,d2
        g.v[2] = (2.0_f64).to_bits();
        g.v[4] = (3.25_f64).to_bits();
    });
    assert_eq!(f64::from_bits(r.v[0]), 6.5, "fmul d0,d1,d2");

    let r = fp_run(&[0x1e62_1820], |g| {
        // fdiv d0,d1,d2
        g.v[2] = (13.0_f64).to_bits();
        g.v[4] = (2.0_f64).to_bits();
    });
    assert_eq!(f64::from_bits(r.v[0]), 6.5, "fdiv d0,d1,d2");

    let r = fp_run(&[0x1e61_c020], |g| {
        // fsqrt d0,d1
        g.v[2] = (42.25_f64).to_bits();
    });
    assert_eq!(f64::from_bits(r.v[0]), 6.5, "fsqrt d0,d1");
}

// ---- NEON vector ops through the JIT (lift -> lower -> V-trampoline -> exec) --
#[test]
fn probe_vector_add_4s() {
    // add v0.4s, v1.4s, v2.4s  (0x4ea28420)
    // V_n.4s lanes: [lane0,lane1] in v[2n], [lane2,lane3] in v[2n+1] (each u32).
    let pack = |l0: u32, l1: u32| (l1 as u64) << 32 | l0 as u64;
    let r = fp_run(&[0x4ea2_8420], |g| {
        g.v[2] = pack(1, 2);
        g.v[3] = pack(3, 4); // V1 = [1,2,3,4]
        g.v[4] = pack(10, 20);
        g.v[5] = pack(40 - 10, 40); // V2 = [10,20,30,40]
    });
    assert_eq!(r.v[0], pack(11, 22), "V0 lanes 0,1");
    assert_eq!(r.v[1], pack(33, 44), "V0 lanes 2,3");
}

// End-to-end: a NEON vector hot loop JIT'd inside the emulator vs the
// interpreter. Each iteration accumulates v1 into v0 (per 32-bit lane); after N
// iterations v0.lane == N*v1.lane. Proves the clobber gate now admits vector
// ops, the emulator routes the region through the FP/V trampoline, and the
// 128-bit vector result matches the interpreter.
#[test]
fn e2e_vector_hot_loop_matches_interpreter() {
    // loop: add v0.4s, v0.4s, v1.4s ; subs x0,x0,#1 ; b.ne loop ; ret
    let prog: [u32; 4] = [0x4ea1_8400, 0xf100_0400, 0x54ff_ffc1, 0xd65f_03c0];
    let pack = |l0: u32, l1: u32, l2: u32, l3: u32| -> u128 {
        (l0 as u128) | (l1 as u128) << 32 | (l2 as u128) << 64 | (l3 as u128) << 96
    };
    let v1 = pack(1, 2, 3, 4);
    const N: u64 = 300;

    let mut interp = fresh_cpu();
    interp.set_jit_enabled(false);
    load_prog(&mut interp, &prog);
    interp.set_simd(1, v1);
    interp.set_x(0, N);
    drive_to_done(&mut interp);

    let mut jit = fresh_cpu();
    jit.set_jit_enabled(true);
    load_prog(&mut jit, &prog);
    jit.set_simd(1, v1);
    jit.set_x(0, N);
    drive_to_done(&mut jit);

    let expected = pack(N as u32, 2 * N as u32, 3 * N as u32, 4 * N as u32);
    assert_eq!(
        interp.get_simd(0),
        expected,
        "interpreter accumulates the vector"
    );
    assert_eq!(
        jit.get_simd(0),
        interp.get_simd(0),
        "JIT vector result matches interp"
    );
    assert_eq!(jit.get_x(0), 0);
}

// Vector fused multiply-add (FMLA), newly emitted by the lifter -> VFma ->
// native vector fmla. v0.4s += v1.4s * v2.4s, per f32 lane.
#[test]
fn probe_vector_fmla_4s() {
    let f = |x: f32| x.to_bits() as u64;
    let pack = |a: f32, b: f32| f(a) | f(b) << 32;
    // fmla v0.4s, v1.4s, v2.4s  (0x4e22cc20)
    let r = fp_run(&[0x4e22_cc20], |g| {
        g.v[0] = pack(1.0, 2.0);
        g.v[1] = pack(3.0, 4.0); // v0 acc = [1,2,3,4]
        g.v[2] = pack(2.0, 2.0);
        g.v[3] = pack(2.0, 2.0); // v1 = [2,2,2,2]
        g.v[4] = pack(3.0, 3.0);
        g.v[5] = pack(3.0, 3.0); // v2 = [3,3,3,3]
    });
    // v0.lane += v1.lane*v2.lane = [1+6, 2+6, 3+6, 4+6]
    assert_eq!(r.v[0], pack(7.0, 8.0), "fmla lanes 0,1");
    assert_eq!(r.v[1], pack(9.0, 10.0), "fmla lanes 2,3");
}

// End-to-end FMLA accumulation hot loop (v0 += v1*v2 each iteration) JIT'd in
// the emulator vs the interpreter — the canonical vectorized dot-product kernel.
#[test]
fn e2e_vector_fmla_hot_loop_matches_interpreter() {
    // loop: fmla v0.4s,v1.4s,v2.4s ; subs x0,x0,#1 ; b.ne loop ; ret
    let prog: [u32; 4] = [0x4e22_cc20, 0xf100_0400, 0x54ff_ffc1, 0xd65f_03c0];
    let f = |x: f32| x.to_bits() as u128;
    let splat = |x: f32| f(x) | f(x) << 32 | f(x) << 64 | f(x) << 96;
    const N: u64 = 100;

    let run_one = |jit: bool| -> u128 {
        let mut cpu = fresh_cpu();
        cpu.set_jit_enabled(jit);
        load_prog(&mut cpu, &prog);
        cpu.set_simd(0, 0); // accumulator
        cpu.set_simd(1, splat(2.0));
        cpu.set_simd(2, splat(3.0));
        cpu.set_x(0, N);
        drive_to_done(&mut cpu);
        cpu.get_simd(0)
    };

    let interp = run_one(false);
    let jit = run_one(true);
    assert_eq!(
        interp,
        splat(N as f32 * 6.0),
        "interp: v0 = N*(2*3) per lane"
    );
    assert_eq!(jit, interp, "JIT FMLA loop matches interpreter");
}

// Interpreter must load a 128-bit vector correctly after the C1 decoder fix
// (ldr q0,[x1] now decodes to an FP-register load, not a GPR load).
#[test]
fn interp_vector_ldr_q() {
    const ADDR: u64 = 0x4000;
    let val: u128 = 0x1122_3344_5566_7788_99aa_bbcc_ddee_ff00;
    let mut cpu = fresh_cpu();
    cpu.set_jit_enabled(false);
    // ldr q0, [x1] ; ret
    load_prog(&mut cpu, &[0x3dc0_0020, 0xd65f_03c0]);
    cpu.write_memory(ADDR, &val.to_le_bytes()).unwrap();
    cpu.set_x(1, ADDR);
    cpu.set_x(30, DONE_PC);
    cpu.set_pc(PROG_BASE);
    for _ in 0..100 {
        if cpu.get_pc() == DONE_PC {
            break;
        }
        cpu.step_system().unwrap();
    }
    assert_eq!(
        cpu.get_simd(0),
        val,
        "interpreter loaded the 128-bit vector"
    );
}

// End-to-end vector load/compute/store loop over guest memory, JIT'd vs the
// interpreter. Each iteration: q0 = load array[i]; q0 += v2 (per 32-bit lane);
// store array[i]; advance pointer. Exercises the full vector-memory JIT path
// (C1 decoder fix -> VLoad/VStore lift -> vec mem-helper lowering -> emulator
// 128-bit helpers) plus the FP/V trampoline.
#[test]
fn e2e_vector_loadstore_loop_matches_interpreter() {
    const ADDR: u64 = 0x4000;
    const N: u64 = 200; // > JIT hot threshold so the region actually compiles
    // loop: ldr q0,[x1]; add v0.4s,v0.4s,v2.4s; str q0,[x1]; add x1,x1,#16;
    //       subs x0,x0,#1; b.ne loop; ret
    let prog: [u32; 7] = [
        0x3dc0_0020,
        0x4ea2_8400,
        0x3d80_0020,
        0x9100_4021,
        0xf100_0400,
        0x54ff_ff61,
        0xd65f_03c0,
    ];
    let v2: u128 = 1 | 2u128 << 32 | 3u128 << 64 | 4u128 << 96; // lanes [1,2,3,4]

    let run_one = |jit: bool| -> Vec<u32> {
        let mut cpu = fresh_cpu();
        cpu.set_jit_enabled(jit);
        cpu.set_jit_mem(true); // vector memory needs the helper path
        load_prog(&mut cpu, &prog);
        for i in 0..N {
            for lane in 0..4u64 {
                let v = (10 * i) as u32;
                cpu.write_memory(ADDR + i * 16 + lane * 4, &v.to_le_bytes())
                    .unwrap();
            }
        }
        cpu.set_simd(2, v2);
        cpu.set_x(1, ADDR);
        cpu.set_x(0, N);
        drive_to_done(&mut cpu);
        let mut out = Vec::new();
        for i in 0..N {
            for lane in 0..4u64 {
                out.push(cpu.mem_read_u32(ADDR + i * 16 + lane * 4).unwrap());
            }
        }
        out
    };

    let interp = run_one(false);
    let jit = run_one(true);
    // Sanity: array[i].lane == 10*i + (lane+1).
    for i in 0..N {
        for lane in 0..4u64 {
            assert_eq!(
                interp[(i * 4 + lane) as usize],
                10 * i as u32 + lane as u32 + 1,
                "interp array[{i}].{lane}"
            );
        }
    }
    assert_eq!(
        jit, interp,
        "vector load/store loop: JIT matches interpreter"
    );
}

// Vector FP arithmetic (fadd/fmul v.4s), newly routed from the cleaned-up
// decoder + lifter bit-28 vector/scalar split, lowered to native vector fadd/fmul.
#[test]
fn probe_vector_fp_arith_4s() {
    let f = |x: f32| x.to_bits() as u64;
    let pack = |a: f32, b: f32| f(a) | f(b) << 32;

    // fadd v0.4s, v1.4s, v2.4s (0x4e22d420): [1,2,3,4] + [10,20,30,40]
    let r = fp_run(&[0x4e22_d420], |g| {
        g.v[2] = pack(1.0, 2.0);
        g.v[3] = pack(3.0, 4.0);
        g.v[4] = pack(10.0, 20.0);
        g.v[5] = pack(30.0, 40.0);
    });
    assert_eq!(r.v[0], pack(11.0, 22.0), "fadd v.4s lanes 0,1");
    assert_eq!(r.v[1], pack(33.0, 44.0), "fadd v.4s lanes 2,3");

    // fmul v0.4s, v1.4s, v2.4s (0x6e22dc20): [2,2,2,2] * [3,4,5,6]
    let r = fp_run(&[0x6e22_dc20], |g| {
        g.v[2] = pack(2.0, 2.0);
        g.v[3] = pack(2.0, 2.0);
        g.v[4] = pack(3.0, 4.0);
        g.v[5] = pack(5.0, 6.0);
    });
    assert_eq!(r.v[0], pack(6.0, 8.0), "fmul v.4s lanes 0,1");
    assert_eq!(r.v[1], pack(10.0, 12.0), "fmul v.4s lanes 2,3");
}

// End-to-end vector FP accumulation loop JIT'd in the emulator vs interpreter:
// v0.4s += v1.4s each iteration. Validates vector FP arithmetic through the full
// emulator JIT path (gate admission + FP/V trampoline).
#[test]
fn e2e_vector_fp_hot_loop_matches_interpreter() {
    // loop: fadd v0.4s,v0.4s,v1.4s ; subs x0,x0,#1 ; b.ne loop ; ret
    let prog: [u32; 4] = [0x4e21_d400, 0xf100_0400, 0x54ff_ffc1, 0xd65f_03c0];
    let f = |x: f32| x.to_bits() as u128;
    let v1 = f(1.0) | f(2.0) << 32 | f(3.0) << 64 | f(4.0) << 96;
    const N: u64 = 100;

    let run_one = |jit: bool| -> u128 {
        let mut cpu = fresh_cpu();
        cpu.set_jit_enabled(jit);
        load_prog(&mut cpu, &prog);
        cpu.set_simd(0, 0);
        cpu.set_simd(1, v1);
        cpu.set_x(0, N);
        drive_to_done(&mut cpu);
        cpu.get_simd(0)
    };

    let interp = run_one(false);
    let jit = run_one(true);
    let nf = N as f32;
    let expected = f(nf) | f(2.0 * nf) << 32 | f(3.0 * nf) << 64 | f(4.0 * nf) << 96;
    assert_eq!(interp, expected, "interp: v0 = N*v1 per lane");
    assert_eq!(jit, interp, "JIT vector FP loop matches interpreter");
}

// Vector FP divide / max / min (three-same, .4s) through the lift→lower→exec
// JIT path. These exercise the new OpKind::VDiv (native FDIV) and the FMAX/FMIN
// lifter emission that reuses VMax/VMin (native FMAX/FMIN).
#[test]
fn probe_vector_fp_div_max_min_4s() {
    let f = |x: f32| x.to_bits() as u64;
    let pack = |a: f32, b: f32| f(a) | f(b) << 32;

    // fdiv v0.4s, v1.4s, v2.4s (0x6e22fc20): [12,20,30,42] / [3,4,5,6]
    let r = fp_run(&[0x6e22_fc20], |g| {
        g.v[2] = pack(12.0, 20.0);
        g.v[3] = pack(30.0, 42.0);
        g.v[4] = pack(3.0, 4.0);
        g.v[5] = pack(5.0, 6.0);
    });
    assert_eq!(r.v[0], pack(4.0, 5.0), "fdiv v.4s lanes 0,1");
    assert_eq!(r.v[1], pack(6.0, 7.0), "fdiv v.4s lanes 2,3");

    // fmax v0.4s, v1.4s, v2.4s (0x4e22f420): max([1,9,3,8],[5,2,7,4])
    let r = fp_run(&[0x4e22_f420], |g| {
        g.v[2] = pack(1.0, 9.0);
        g.v[3] = pack(3.0, 8.0);
        g.v[4] = pack(5.0, 2.0);
        g.v[5] = pack(7.0, 4.0);
    });
    assert_eq!(r.v[0], pack(5.0, 9.0), "fmax v.4s lanes 0,1");
    assert_eq!(r.v[1], pack(7.0, 8.0), "fmax v.4s lanes 2,3");

    // fmin v0.4s, v1.4s, v2.4s (0x4ea2f420): min(same inputs)
    let r = fp_run(&[0x4ea2_f420], |g| {
        g.v[2] = pack(1.0, 9.0);
        g.v[3] = pack(3.0, 8.0);
        g.v[4] = pack(5.0, 2.0);
        g.v[5] = pack(7.0, 4.0);
    });
    assert_eq!(r.v[0], pack(1.0, 2.0), "fmin v.4s lanes 0,1");
    assert_eq!(r.v[1], pack(3.0, 4.0), "fmin v.4s lanes 2,3");
}

// End-to-end differential: each new vector-FP op (fdiv/fmax/fmin) run as a hot
// loop through the emulator JIT vs the interpreter. Catches any lifter/lowerer/
// decoder disagreement with the authoritative AArch64 interpreter.
#[test]
fn e2e_vector_fp_div_max_min_matches_interpreter() {
    let f = |x: f32| x.to_bits() as u128;
    let pack = |a: f32, b: f32, c: f32, d: f32| f(a) | f(b) << 32 | f(c) << 64 | f(d) << 96;

    // Each program: <op> v0.4s,v1.4s,v2.4s ; subs x0,x0,#1 ; b.ne -8 ; ret.
    // v0 is recomputed every iteration (idempotent), so the final v0 = op(v1,v2).
    let cases: [(u32, u128, u128, u128); 3] = [
        // fdiv: [60,60,60,60] / [2,3,4,5] = [30,20,15,12]
        (
            0x6e22_fc20,
            pack(60.0, 60.0, 60.0, 60.0),
            pack(2.0, 3.0, 4.0, 5.0),
            pack(30.0, 20.0, 15.0, 12.0),
        ),
        // fmax: max([1,9,3,8],[5,2,7,4]) = [5,9,7,8]
        (
            0x4e22_f420,
            pack(1.0, 9.0, 3.0, 8.0),
            pack(5.0, 2.0, 7.0, 4.0),
            pack(5.0, 9.0, 7.0, 8.0),
        ),
        // fmin: min(same) = [1,2,3,4]
        (
            0x4ea2_f420,
            pack(1.0, 9.0, 3.0, 8.0),
            pack(5.0, 2.0, 7.0, 4.0),
            pack(1.0, 2.0, 3.0, 4.0),
        ),
    ];

    for (op, v1, v2, expected) in cases {
        let prog: [u32; 4] = [op, 0xf100_0400, 0x54ff_ffc1, 0xd65f_03c0];
        let run_one = |jit: bool| -> u128 {
            let mut cpu = fresh_cpu();
            cpu.set_jit_enabled(jit);
            load_prog(&mut cpu, &prog);
            cpu.set_simd(0, 0);
            cpu.set_simd(1, v1);
            cpu.set_simd(2, v2);
            cpu.set_x(0, 100);
            drive_to_done(&mut cpu);
            cpu.get_simd(0)
        };
        let interp = run_one(false);
        let jit = run_one(true);
        assert_eq!(interp, expected, "interp op={:#010x}", op);
        assert_eq!(jit, interp, "JIT matches interp op={:#010x}", op);
    }
}

// Safety regression for the two-register-misc decoder/lifter work. The vector
// forms (FABS/FNEG/FSQRT/NEG/ABS/CLZ/CLS/RBIT/CNT/NOT/REV16/REV32/REV64) all JIT
// now (see the probe_vector_* tests). This guards the converse: the SCALAR FP
// 1-source forms (bit 28 == 1), which share the FABS/FNEG/FSQRT mnemonics with
// the vector forms, must still lift and execute via the scalar path (the bit-28
// discriminator must not misroute them to the vector VUnary path).
#[test]
fn scalar_fp_one_source_still_lifts_after_vector_unary() {
    let f = |x: f32| x.to_bits() as u64;
    // fabs s0, s1 (0x1e20c020): |-3.0| = 3.0
    let mut regs = Aarch64GuestRegs::default();
    regs.v[2] = f(-3.0); // V1.lo = s1
    jit_run(&[0x1e20_c020], &mut regs).expect("scalar fabs must still lift");
    assert_eq!(regs.v[0] as u32, f(3.0) as u32, "scalar fabs s0");

    // fneg s0, s1 (0x1e214020): -(3.0) = -3.0
    let mut regs = Aarch64GuestRegs::default();
    regs.v[2] = f(3.0);
    jit_run(&[0x1e21_4020], &mut regs).expect("scalar fneg must still lift");
    assert_eq!(regs.v[0] as u32, f(-3.0) as u32, "scalar fneg s0");

    // fsqrt s0, s1 (0x1e21c020): sqrt(9.0) = 3.0
    let mut regs = Aarch64GuestRegs::default();
    regs.v[2] = f(9.0);
    jit_run(&[0x1e21_c020], &mut regs).expect("scalar fsqrt must still lift");
    assert_eq!(regs.v[0] as u32, f(3.0) as u32, "scalar fsqrt s0");
}

// Per-lane vector FP unary (FABS/FNEG/FSQRT) via OpKind::VUnary, lift→lower→exec.
#[test]
fn probe_vector_fp_unary_4s() {
    let f = |x: f32| x.to_bits() as u64;
    let pack = |a: f32, b: f32| f(a) | f(b) << 32;

    // fabs v0.4s, v1.4s (0x4ea0f820)
    let r = fp_run(&[0x4ea0_f820], |g| {
        g.v[2] = pack(-1.0, 2.0);
        g.v[3] = pack(-3.0, 4.0);
    });
    assert_eq!(r.v[0], pack(1.0, 2.0), "fabs lanes 0,1");
    assert_eq!(r.v[1], pack(3.0, 4.0), "fabs lanes 2,3");

    // fneg v0.4s, v1.4s (0x6ea0f820)
    let r = fp_run(&[0x6ea0_f820], |g| {
        g.v[2] = pack(1.0, -2.0);
        g.v[3] = pack(3.0, -4.0);
    });
    assert_eq!(r.v[0], pack(-1.0, 2.0), "fneg lanes 0,1");
    assert_eq!(r.v[1], pack(-3.0, 4.0), "fneg lanes 2,3");

    // fsqrt v0.4s, v1.4s (0x6ea1f820)
    let r = fp_run(&[0x6ea1_f820], |g| {
        g.v[2] = pack(1.0, 4.0);
        g.v[3] = pack(9.0, 16.0);
    });
    assert_eq!(r.v[0], pack(1.0, 2.0), "fsqrt lanes 0,1");
    assert_eq!(r.v[1], pack(3.0, 4.0), "fsqrt lanes 2,3");
}

// Per-lane vector integer unary (NEG/ABS) via OpKind::VUnary, lift→lower→exec.
#[test]
fn probe_vector_int_unary_4s() {
    let packi = |a: i32, b: i32| (a as u32 as u64) | ((b as u32 as u64) << 32);

    // neg v0.4s, v1.4s (0x6ea0b820) — I32 lanes
    let r = fp_run(&[0x6ea0_b820], |g| {
        g.v[2] = packi(1, -2);
        g.v[3] = packi(3, -4);
    });
    assert_eq!(r.v[0], packi(-1, 2), "neg lanes 0,1");
    assert_eq!(r.v[1], packi(-3, 4), "neg lanes 2,3");

    // abs v0.4s, v1.4s (0x4ea0b820)
    let r = fp_run(&[0x4ea0_b820], |g| {
        g.v[2] = packi(-5, 6);
        g.v[3] = packi(-7, 8);
    });
    assert_eq!(r.v[0], packi(5, 6), "abs lanes 0,1");
    assert_eq!(r.v[1], packi(7, 8), "abs lanes 2,3");
}

// End-to-end: vector unary ops run as a hot loop through the emulator JIT vs
// the interpreter. Each loop recomputes v0 = unary(v1) (idempotent), so the
// final v0 == unary(v1) and JIT must match the interpreter.
#[test]
fn e2e_vector_unary_hot_loop_matches_interpreter() {
    let f = |x: f32| x.to_bits() as u128;
    let pf = |a: f32, b: f32, c: f32, d: f32| f(a) | f(b) << 32 | f(c) << 64 | f(d) << 96;
    let pi = |a: i32, b: i32, c: i32, d: i32| {
        (a as u32 as u128)
            | (b as u32 as u128) << 32
            | (c as u32 as u128) << 64
            | (d as u32 as u128) << 96
    };
    // (op, v1, expected): <op> v0.4s,v1.4s ; subs x0,x0,#1 ; b.ne -8 ; ret
    let cases: [(u32, u128, u128); 5] = [
        (
            0x4ea0_f820,
            pf(-1.0, 2.0, -3.0, 4.0),
            pf(1.0, 2.0, 3.0, 4.0),
        ), // fabs
        (
            0x6ea0_f820,
            pf(1.0, -2.0, 3.0, -4.0),
            pf(-1.0, 2.0, -3.0, 4.0),
        ), // fneg
        (0x6ea1_f820, pf(1.0, 4.0, 9.0, 16.0), pf(1.0, 2.0, 3.0, 4.0)), // fsqrt
        (0x6ea0_b820, pi(1, -2, 3, -4), pi(-1, 2, -3, 4)),              // neg
        (0x4ea0_b820, pi(-5, 6, -7, 8), pi(5, 6, 7, 8)),                // abs
    ];

    for (op, v1, expected) in cases {
        let prog: [u32; 4] = [op, 0xf100_0400, 0x54ff_ffc1, 0xd65f_03c0];
        let run_one = |jit: bool| -> u128 {
            let mut cpu = fresh_cpu();
            cpu.set_jit_enabled(jit);
            load_prog(&mut cpu, &prog);
            cpu.set_simd(0, 0);
            cpu.set_simd(1, v1);
            cpu.set_x(0, 100);
            drive_to_done(&mut cpu);
            cpu.get_simd(0)
        };
        let interp = run_one(false);
        let jit = run_one(true);
        assert_eq!(interp, expected, "interp op={:#010x}", op);
        assert_eq!(jit, interp, "JIT matches interp op={:#010x}", op);
    }
}

// Per-lane vector bit-manipulation unary ops (CLZ/CLS/RBIT/CNT/NOT) via
// OpKind::VUnary, lift→lower→exec, compared to independent reference closures.
#[test]
fn probe_vector_bitmanip_unary() {
    // Per-byte helpers over a u64 (lane 0 = LSB).
    let per_byte = |x: u64, f: fn(u8) -> u8| -> u64 {
        let mut out = 0u64;
        for i in 0..8 {
            out |= (f(((x >> (i * 8)) & 0xFF) as u8) as u64) << (i * 8);
        }
        out
    };
    let lo: u64 = 0x7F3F_1F0F_0703_0100;
    let hi: u64 = 0xFFAA_5580_C0E0_F0F8;

    // cnt v0.16b, v1.16b (0x4e205820): per-byte popcount.
    let r = fp_run(&[0x4e20_5820], |g| {
        g.v[2] = lo;
        g.v[3] = hi;
    });
    assert_eq!(r.v[0], per_byte(lo, |b| b.count_ones() as u8), "cnt lo");
    assert_eq!(r.v[1], per_byte(hi, |b| b.count_ones() as u8), "cnt hi");

    // not v0.16b, v1.16b (0x6e205820): bitwise NOT.
    let r = fp_run(&[0x6e20_5820], |g| {
        g.v[2] = lo;
        g.v[3] = hi;
    });
    assert_eq!(r.v[0], !lo, "not lo");
    assert_eq!(r.v[1], !hi, "not hi");

    // rbit v0.16b, v1.16b (0x6e605820): per-byte bit reverse.
    let r = fp_run(&[0x6e60_5820], |g| {
        g.v[2] = lo;
        g.v[3] = hi;
    });
    assert_eq!(r.v[0], per_byte(lo, |b| b.reverse_bits()), "rbit lo");
    assert_eq!(r.v[1], per_byte(hi, |b| b.reverse_bits()), "rbit hi");

    // Per-32-bit-lane CLZ/CLS helpers.
    let pack32 = |a: u32, b: u32| (a as u64) | ((b as u64) << 32);
    let cls32 = |x: u32| -> u32 {
        let sign = (x >> 31) & 1;
        let mut c = 0u32;
        for i in (0..31).rev() {
            if (x >> i) & 1 == sign {
                c += 1;
            } else {
                break;
            }
        }
        c
    };

    // clz v0.4s, v1.4s (0x6ea04820).
    let s = [0x0000_0001u32, 0x0000_FFFF, 0x8000_0000, 0x0000_0000];
    let r = fp_run(&[0x6ea0_4820], |g| {
        g.v[2] = pack32(s[0], s[1]);
        g.v[3] = pack32(s[2], s[3]);
    });
    assert_eq!(
        r.v[0],
        pack32(s[0].leading_zeros(), s[1].leading_zeros()),
        "clz 0,1"
    );
    assert_eq!(
        r.v[1],
        pack32(s[2].leading_zeros(), s[3].leading_zeros()),
        "clz 2,3"
    );

    // cls v0.4s, v1.4s (0x4ea04820).
    let s = [0x0000_0001u32, 0xFFFF_FFFF, 0x8000_0000, 0x4000_0000];
    let r = fp_run(&[0x4ea0_4820], |g| {
        g.v[2] = pack32(s[0], s[1]);
        g.v[3] = pack32(s[2], s[3]);
    });
    assert_eq!(r.v[0], pack32(cls32(s[0]), cls32(s[1])), "cls 0,1");
    assert_eq!(r.v[1], pack32(cls32(s[2]), cls32(s[3])), "cls 2,3");
}

// End-to-end: each vector bit-manip op run as a hot loop through the emulator
// JIT vs the interpreter (which decodes them via its own independent path).
#[test]
fn e2e_vector_bitmanip_hot_loop_matches_interpreter() {
    // (op, v1): <op> v0,v1 ; subs x0,x0,#1 ; b.ne -8 ; ret. v0 = op(v1) each iter.
    let v1: u128 = 0x0123_4567_89AB_CDEF_FEDC_BA98_7654_3210;
    let ops: [u32; 5] = [
        0x4e20_5820, // cnt  v0.16b, v1.16b
        0x6e20_5820, // not  v0.16b, v1.16b
        0x6e60_5820, // rbit v0.16b, v1.16b
        0x6ea0_4820, // clz  v0.4s,  v1.4s
        0x4ea0_4820, // cls  v0.4s,  v1.4s
    ];
    for op in ops {
        let prog: [u32; 4] = [op, 0xf100_0400, 0x54ff_ffc1, 0xd65f_03c0];
        let run_one = |jit: bool| -> u128 {
            let mut cpu = fresh_cpu();
            cpu.set_jit_enabled(jit);
            load_prog(&mut cpu, &prog);
            cpu.set_simd(0, 0);
            cpu.set_simd(1, v1);
            cpu.set_x(0, 100);
            drive_to_done(&mut cpu);
            cpu.get_simd(0)
        };
        let interp = run_one(false);
        let jit = run_one(true);
        assert_eq!(jit, interp, "JIT matches interp op={:#010x}", op);
        assert_ne!(interp, 0, "op={:#010x} produced a nonzero result", op);
    }
}

// Vector REV16/REV32/REV64 (reverse elements within 16/32/64-bit containers)
// via OpKind::VUnary, lift→lower→exec, compared to reference closures.
#[test]
fn probe_vector_rev() {
    let lo: u64 = 0x0102_0304_0506_0708;
    let hi: u64 = 0x1112_1314_1516_1718;

    // rev64 v0.16b, v1.16b (0x4e200820): byte-reverse each 64-bit lane.
    let r = fp_run(&[0x4e20_0820], |g| {
        g.v[2] = lo;
        g.v[3] = hi;
    });
    assert_eq!(r.v[0], lo.swap_bytes(), "rev64.16b lo");
    assert_eq!(r.v[1], hi.swap_bytes(), "rev64.16b hi");

    // rev32 v0.16b, v1.16b (0x6e200820): byte-reverse each 32-bit word.
    let rev32 = |x: u64| {
        (x as u32).swap_bytes() as u64 | ((((x >> 32) as u32).swap_bytes() as u64) << 32)
    };
    let r = fp_run(&[0x6e20_0820], |g| {
        g.v[2] = lo;
        g.v[3] = hi;
    });
    assert_eq!(r.v[0], rev32(lo), "rev32.16b lo");
    assert_eq!(r.v[1], rev32(hi), "rev32.16b hi");

    // rev16 v0.16b, v1.16b (0x4e201820): byte-reverse each 16-bit halfword.
    let rev16 = |x: u64| {
        let mut out = 0u64;
        for i in 0..4 {
            let h = ((x >> (i * 16)) & 0xFFFF) as u16;
            out |= (h.swap_bytes() as u64) << (i * 16);
        }
        out
    };
    let r = fp_run(&[0x4e20_1820], |g| {
        g.v[2] = lo;
        g.v[3] = hi;
    });
    assert_eq!(r.v[0], rev16(lo), "rev16.16b lo");
    assert_eq!(r.v[1], rev16(hi), "rev16.16b hi");

    // rev64 v0.4s, v1.4s (0x4ea00820): swap the two 32-bit words in each lane.
    let rev64_w = |x: u64| (x >> 32) | (x << 32);
    let r = fp_run(&[0x4ea0_0820], |g| {
        g.v[2] = lo;
        g.v[3] = hi;
    });
    assert_eq!(r.v[0], rev64_w(lo), "rev64.4s lo");
    assert_eq!(r.v[1], rev64_w(hi), "rev64.4s hi");
}

// End-to-end: vector REV ops run as a hot loop through the emulator JIT vs the
// interpreter (which reverses container elements via its own decode path).
#[test]
fn e2e_vector_rev_hot_loop_matches_interpreter() {
    let v1: u128 = 0x0011_2233_4455_6677_8899_AABB_CCDD_EEFF;
    let ops: [u32; 4] = [
        0x4e20_0820, // rev64 v0.16b, v1.16b
        0x6e20_0820, // rev32 v0.16b, v1.16b
        0x4e20_1820, // rev16 v0.16b, v1.16b
        0x4ea0_0820, // rev64 v0.4s,  v1.4s
    ];
    for op in ops {
        let prog: [u32; 4] = [op, 0xf100_0400, 0x54ff_ffc1, 0xd65f_03c0];
        let run_one = |jit: bool| -> u128 {
            let mut cpu = fresh_cpu();
            cpu.set_jit_enabled(jit);
            load_prog(&mut cpu, &prog);
            cpu.set_simd(0, 0);
            cpu.set_simd(1, v1);
            cpu.set_x(0, 100);
            drive_to_done(&mut cpu);
            cpu.get_simd(0)
        };
        let interp = run_one(false);
        let jit = run_one(true);
        assert_eq!(jit, interp, "JIT matches interp op={:#010x}", op);
        assert_ne!(interp, 0, "op={:#010x} produced a nonzero result", op);
    }
}

// Vector across-lanes integer reductions (ADDV/SMAXV/UMAXV/SMINV/UMINV) via
// OpKind::VReduce, lift→lower→exec. Result is a scalar in lane 0.
#[test]
fn probe_vector_reduce() {
    let pack32 = |a: u32, b: u32| (a as u64) | ((b as u64) << 32);

    // addv s0, v1.4s (0x4eb1b820): sum of 4 i32 lanes = 1+2+3+4 = 10.
    let r = fp_run(&[0x4eb1_b820], |g| {
        g.v[2] = pack32(1, 2);
        g.v[3] = pack32(3, 4);
    });
    assert_eq!(r.v[0], 10, "addv .4s sum");
    assert_eq!(r.v[1], 0, "addv clears upper");

    // smaxv s0, v1.4s (0x4eb0a820): signed max of [-5,3,10,-2] = 10.
    let r = fp_run(&[0x4eb0_a820], |g| {
        g.v[2] = pack32((-5i32) as u32, 3);
        g.v[3] = pack32(10, (-2i32) as u32);
    });
    assert_eq!(r.v[0] as u32, 10, "smaxv .4s");

    // sminv s0, v1.4s (0x4eb1a820): signed min of [-5,3,10,-2] = -5.
    let r = fp_run(&[0x4eb1_a820], |g| {
        g.v[2] = pack32((-5i32) as u32, 3);
        g.v[3] = pack32(10, (-2i32) as u32);
    });
    assert_eq!(r.v[0] as u32, (-5i32) as u32, "sminv .4s");

    // umaxv s0, v1.4s (0x6eb0a820): unsigned max = 0xFFFFFFFF.
    let r = fp_run(&[0x6eb0_a820], |g| {
        g.v[2] = pack32(1, 0xFFFF_FFFF);
        g.v[3] = pack32(3, 4);
    });
    assert_eq!(r.v[0] as u32, 0xFFFF_FFFF, "umaxv .4s");

    // uminv b0, v1.16b (0x6e31a820): unsigned min byte = 0x01.
    let r = fp_run(&[0x6e31_a820], |g| {
        g.v[2] = 0x0807_0605_0403_0201;
        g.v[3] = 0x100F_0E0D_0C0B_0A09;
    });
    assert_eq!(r.v[0], 0x01, "uminv .16b");
}

// End-to-end: vector reductions run as a hot loop through the emulator JIT vs
// the interpreter (which reduces via its own exec_simd_across_lanes path).
#[test]
fn e2e_vector_reduce_hot_loop_matches_interpreter() {
    let p = |a: u32, b: u32, c: u32, d: u32| {
        (a as u128) | (b as u128) << 32 | (c as u128) << 64 | (d as u128) << 96
    };
    let v1 = p(7, 3, 11, 5);
    let ops: [u32; 4] = [
        0x4eb1_b820, // addv  s0, v1.4s  -> 26
        0x4eb0_a820, // smaxv s0, v1.4s  -> 11
        0x4eb1_a820, // sminv s0, v1.4s  -> 3
        0x6eb0_a820, // umaxv s0, v1.4s  -> 11
    ];
    for op in ops {
        let prog: [u32; 4] = [op, 0xf100_0400, 0x54ff_ffc1, 0xd65f_03c0];
        let run_one = |jit: bool| -> u128 {
            let mut cpu = fresh_cpu();
            cpu.set_jit_enabled(jit);
            load_prog(&mut cpu, &prog);
            cpu.set_simd(0, 0);
            cpu.set_simd(1, v1);
            cpu.set_x(0, 100);
            drive_to_done(&mut cpu);
            cpu.get_simd(0)
        };
        let interp = run_one(false);
        let jit = run_one(true);
        assert_eq!(jit, interp, "JIT matches interp op={:#010x}", op);
        assert_ne!(interp, 0, "op={:#010x} produced a nonzero result", op);
    }
}

// Vector FP numeric min/max FMAXNM/FMINNM (three-same) via OpKind::VFMinMaxNm.
// These are NaN-quiet (return the numeric operand), unlike FMAX/FMIN.
#[test]
fn probe_vector_fp_minmax_nm_4s() {
    let f = |x: f32| x.to_bits() as u64;
    let pack = |a: f32, b: f32| f(a) | f(b) << 32;
    let nan = f32::NAN;

    // fmaxnm v0.4s, v1.4s, v2.4s (0x4e22c420). v1=[1,NaN,3,8], v2=[4,2,7,6].
    let r = fp_run(&[0x4e22_c420], |g| {
        g.v[2] = pack(1.0, nan);
        g.v[3] = pack(3.0, 8.0);
        g.v[4] = pack(4.0, 2.0);
        g.v[5] = pack(7.0, 6.0);
    });
    assert_eq!(r.v[0], pack(4.0, 2.0), "fmaxnm 0,1 (max(NaN,2)=2 numeric)");
    assert_eq!(r.v[1], pack(7.0, 8.0), "fmaxnm 2,3");

    // fminnm v0.4s, v1.4s, v2.4s (0x4ea2c420).
    let r = fp_run(&[0x4ea2_c420], |g| {
        g.v[2] = pack(1.0, nan);
        g.v[3] = pack(3.0, 8.0);
        g.v[4] = pack(4.0, 2.0);
        g.v[5] = pack(7.0, 6.0);
    });
    assert_eq!(r.v[0], pack(1.0, 2.0), "fminnm 0,1 (min(NaN,2)=2 numeric)");
    assert_eq!(r.v[1], pack(3.0, 6.0), "fminnm 2,3");
}

// End-to-end: FMAXNM/FMINNM hot loop through the emulator JIT vs interpreter.
#[test]
fn e2e_vector_fp_minmax_nm_matches_interpreter() {
    let f = |x: f32| x.to_bits() as u128;
    let p = |a: f32, b: f32, c: f32, d: f32| f(a) | f(b) << 32 | f(c) << 64 | f(d) << 96;
    let v1 = p(1.0, 5.0, 3.0, 8.0);
    let v2 = p(4.0, 2.0, 7.0, 6.0);
    let cases: [(u32, u128); 2] = [
        (0x4e22_c420, p(4.0, 5.0, 7.0, 8.0)), // fmaxnm
        (0x4ea2_c420, p(1.0, 2.0, 3.0, 6.0)), // fminnm
    ];
    for (op, expected) in cases {
        let prog: [u32; 4] = [op, 0xf100_0400, 0x54ff_ffc1, 0xd65f_03c0];
        let run_one = |jit: bool| -> u128 {
            let mut cpu = fresh_cpu();
            cpu.set_jit_enabled(jit);
            load_prog(&mut cpu, &prog);
            cpu.set_simd(0, 0);
            cpu.set_simd(1, v1);
            cpu.set_simd(2, v2);
            cpu.set_x(0, 100);
            drive_to_done(&mut cpu);
            cpu.get_simd(0)
        };
        let interp = run_one(false);
        let jit = run_one(true);
        assert_eq!(interp, expected, "interp op={:#010x}", op);
        assert_eq!(jit, interp, "JIT matches interp op={:#010x}", op);
    }
}
