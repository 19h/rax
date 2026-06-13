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

use rax::smir::ir::{FunctionBuilder, Terminator};
use rax::smir::lift::aarch64::Aarch64Lifter;
use rax::smir::lift::{LiftContext, SmirLifter};
use rax::smir::lower::aarch64::Aarch64Lowerer;
use rax::smir::lower::runtime::{Aarch64GuestRegs, ExecMem};
use rax::smir::lower::SmirLowerer;
use rax::smir::types::{FunctionId, SourceArch};

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
    mem.run_aarch64_identity(result.entry_offset, regs);
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
