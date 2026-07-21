//! Strict lift, metadata, optimizer, and interpreter coverage for RDMSR/WRMSR.

use super::*;
use crate::isa::x86_64::execute::system::{
    IA32_EFER, IA32_LSTAR, IA32_STAR, IA32_SYSENTER_CS, IA32_TSC, IA32_TSC_ADJUST,
};
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::ops::X86MsrOp;
use crate::smir::optimize::{OptLevel, optimize_function};

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn msr_kind(write: bool, next_pc: u64) -> OpKind {
    OpKind::X86Msr(X86MsrOp {
        eax: x86(X86Reg::Rax),
        ecx: x86(X86Reg::Rcx),
        edx: x86(X86Reg::Rdx),
        write,
        next_pc,
    })
}

fn exact_msr(result: &LiftResult) -> &X86MsrOp {
    assert_eq!(result.ops.len(), 1);
    match &result.ops[0].kind {
        OpKind::X86Msr(msr) => msr,
        other => panic!("expected one exact MSR op, got {other:?}"),
    }
}

fn execute_msr(
    write: bool,
    index: u64,
    value: u64,
    configure: impl FnOnce(&mut SmirContext),
) -> (BlockResult, SmirContext) {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, msr_kind(write, 0x1002));
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let function = builder.finish();
    let mut context = SmirContext::new_x86_64();
    configure(&mut context);
    context.write_vreg(x86(X86Reg::Rcx), index);
    context.write_vreg(
        x86(X86Reg::Rax),
        0xA5A5_A5A5_0000_0000 | (value & u64::from(u32::MAX)),
    );
    context.write_vreg(x86(X86Reg::Rdx), 0x5A5A_5A5A_0000_0000 | (value >> 32));
    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        function.entry_block().unwrap(),
    );
    (result, context)
}

#[test]
fn rdmsr_wrmsr_strictly_lift_fixed_registers_and_exact_frontiers() {
    for (bytes, write) in [
        (&[0x0F, 0x32][..], false),
        (&[0x0F, 0x30], true),
        (&[0x66, 0x0F, 0x32], false),
        (&[0xF3, 0x48, 0x0F, 0x30], true),
        (&[0x64, 0x67, 0x0F, 0x32], false),
    ] {
        let result = lift_single(bytes).expect("strict MSR lift");
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        assert!(matches!(
            exact_msr(&result),
            X86MsrOp {
                eax: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                ecx: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
                edx: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
                write: got_write,
                next_pc,
            } if *got_write == write && *next_pc == 0x1000 + bytes.len() as u64
        ));
    }
}

#[test]
fn rdmsr_wrmsr_reject_lock_and_rex2_but_ignore_other_legacy_prefixes() {
    for prefix in [
        0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, // segment overrides
        0x66, 0x67, // operand/address size
        0x40, 0x48, // ordinary REX and REX.W
        0xF2, 0xF3, // repeat prefixes
    ] {
        for opcode in [0x30, 0x32] {
            let bytes = [prefix, 0x0F, opcode];
            let result = lift_single(&bytes).expect("architecturally ignored MSR prefix");
            assert_eq!(result.bytes_consumed, 3, "{bytes:02X?}");
            assert_eq!(exact_msr(&result).write, opcode == 0x30);
        }
    }

    for bytes in [&[0xF0, 0x0F, 0x30][..], &[0xF0, 0x0F, 0x32]] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "{bytes:02X?}"
        );
    }

    for bytes in [&[0xD5, 0x80, 0x30][..], &[0xD5, 0x80, 0x32]] {
        let result = lift_single(bytes).expect("REX2 compressed map 1 row 3 is an explicit #UD");
        assert_invalid_opcode_trap(&result, 3);
    }
}

#[test]
fn msr_metadata_distinguishes_read_destinations_from_write_sources() {
    let read = msr_kind(false, 0x1002);
    assert_eq!(read.source_vregs(), vec![x86(X86Reg::Rcx)]);
    assert_eq!(read.dests(), vec![x86(X86Reg::Rax), x86(X86Reg::Rdx)]);

    let write = msr_kind(true, 0x1002);
    assert_eq!(
        write.source_vregs(),
        vec![x86(X86Reg::Rcx), x86(X86Reg::Rax), x86(X86Reg::Rdx)]
    );
    assert!(write.dests().is_empty());

    for kind in [read, write] {
        assert!(kind.flags_read().is_empty());
        assert!(kind.flags_written().is_empty());
        assert!(kind.has_side_effects());
        assert!(!kind.reads_memory());
        assert!(!kind.writes_memory());
        assert!(kind.is_jit_safe());
        assert!(SmirOp::new(OpId(0), 0x1000, kind).is_jit_safe());
    }
}

#[test]
fn msr_interpreter_roundtrips_state_and_zero_extends_rdmsr_outputs() {
    let value = 0xCAFE_BABE_DEAD_BEEF;
    let (write, context) = execute_msr(true, u64::from(IA32_STAR), value, |context| {
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr0 = 1;
        x86.cpl = 0;
    });
    assert!(matches!(write, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.read_vreg(x86(X86Reg::Rax)), 0xA5A5_A5A5_DEAD_BEEF);
    assert_eq!(context.read_vreg(x86(X86Reg::Rdx)), 0x5A5A_5A5A_CAFE_BABE);
    let ArchRegState::X86_64(x86_state) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86_state.star, value);

    let flags = MaterializedFlags {
        cf: true,
        zf: false,
        sf: true,
        of: true,
        pf: false,
        af: true,
        df: true,
        ac: true,
    };
    let (read, context) = execute_msr(false, u64::from(IA32_STAR), 0, |context| {
        context.flags.materialized = flags;
        let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
            unreachable!()
        };
        x86.cr0 = 1;
        x86.cpl = 0;
        x86.star = value;
    });
    assert!(matches!(read, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.read_vreg(x86(X86Reg::Rax)), 0xDEAD_BEEF);
    assert_eq!(context.read_vreg(x86(X86Reg::Rdx)), 0xCAFE_BABE);
    assert_eq!(context.read_vreg(x86(X86Reg::Rcx)), u64::from(IA32_STAR));
    assert_eq!(context.flags.materialized.to_rflags(), flags.to_rflags());
    assert!(context.flags.lazy.is_none());
}

#[test]
fn msr_interpreter_models_tsc_and_tsc_adjust_in_one_clock_domain() {
    let desired_tsc = 0x1234_5678_9ABC_DEF0;
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, msr_kind(true, 0x1002));
    builder.push_op(0x1002, msr_kind(false, 0x1004));
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let function = builder.finish();
    let mut context = SmirContext::new_x86_64();
    context.cycle_count = 100;
    context.write_vreg(x86(X86Reg::Rcx), u64::from(IA32_TSC));
    context.write_vreg(x86(X86Reg::Rax), desired_tsc & u64::from(u32::MAX));
    context.write_vreg(x86(X86Reg::Rdx), desired_tsc >> 32);
    let ArchRegState::X86_64(x86_state) = &mut context.arch_regs else {
        unreachable!()
    };
    x86_state.cr0 = 1;
    x86_state.tsc_adjust = 20;

    let result = SmirInterpreter::new().execute_block(
        &mut context,
        &mut FlatMemory::new(1),
        function.entry_block().unwrap(),
    );
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    assert_eq!(context.read_vreg(x86(X86Reg::Rax)), 0x9ABC_DEF0);
    assert_eq!(context.read_vreg(x86(X86Reg::Rdx)), 0x1234_5678);
    let ArchRegState::X86_64(x86_state) = context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86_state.tsc_adjust, desired_tsc.wrapping_sub(100));
}

#[test]
fn msr_interpreter_faults_are_precise_and_noncommitting() {
    for (name, write, index, value, configure) in [
        (
            "protected CPL3",
            true,
            u64::from(IA32_STAR),
            0x1111,
            (|context: &mut SmirContext| {
                let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                    unreachable!()
                };
                x86.cr0 = 1;
                x86.cpl = 3;
                x86.star = 0x2222;
            }) as fn(&mut SmirContext),
        ),
        (
            "unknown read",
            false,
            0xDEAD_BEEF,
            0,
            (|context: &mut SmirContext| {
                let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                    unreachable!()
                };
                x86.cr0 = 1;
                x86.star = 0x2222;
            }) as fn(&mut SmirContext),
        ),
        (
            "unknown write",
            true,
            0xDEAD_BEEF,
            0x3333,
            (|context: &mut SmirContext| {
                let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                    unreachable!()
                };
                x86.cr0 = 1;
                x86.star = 0x2222;
            }) as fn(&mut SmirContext),
        ),
        (
            "noncanonical LSTAR",
            true,
            u64::from(IA32_LSTAR),
            0x0000_8000_0000_0000,
            (|context: &mut SmirContext| {
                let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                    unreachable!()
                };
                x86.cr0 = 1;
                x86.star = 0x2222;
                x86.lstar = 0x4444;
            }) as fn(&mut SmirContext),
        ),
        (
            "reserved EFER bit",
            true,
            u64::from(IA32_EFER),
            1 << 12,
            (|context: &mut SmirContext| {
                let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                    unreachable!()
                };
                x86.cr0 = 1;
                x86.star = 0x2222;
                x86.efer = 1 << 8;
            }) as fn(&mut SmirContext),
        ),
        (
            "reserved SYSENTER_CS high bits",
            true,
            u64::from(IA32_SYSENTER_CS),
            1 << 32,
            (|context: &mut SmirContext| {
                let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                    unreachable!()
                };
                x86.cr0 = 1;
                x86.star = 0x2222;
                x86.sysenter_cs = 8;
            }) as fn(&mut SmirContext),
        ),
    ] {
        let (result, context) = execute_msr(write, index, value, configure);
        assert!(
            matches!(
                result,
                BlockResult::Exit(ExitReason::GeneralProtection {
                    addr: 0x1000,
                    error_code: 0
                })
            ),
            "{name}: {result:?}"
        );
        assert_eq!(
            context.read_vreg(x86(X86Reg::Rax)),
            0xA5A5_A5A5_0000_0000 | (value & u64::from(u32::MAX)),
            "{name}: RAX"
        );
        assert_eq!(
            context.read_vreg(x86(X86Reg::Rdx)),
            0x5A5A_5A5A_0000_0000 | (value >> 32),
            "{name}: RDX"
        );
        let ArchRegState::X86_64(x86_state) = context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86_state.star, 0x2222, "{name}: STAR");
        if name == "noncanonical LSTAR" {
            assert_eq!(x86_state.lstar, 0x4444);
        }
        if name == "reserved EFER bit" {
            assert_eq!(x86_state.efer, 1 << 8);
        }
        if name == "reserved SYSENTER_CS high bits" {
            assert_eq!(x86_state.sysenter_cs, 8);
        }
    }
}

#[test]
fn msr_interpreter_real_mode_bypasses_stale_cpl() {
    let (result, context) = execute_msr(
        true,
        u64::from(IA32_TSC_ADJUST),
        0xCAFE_BABE_DEAD_BEEF,
        |context| {
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.cr0 = 0;
            x86.cpl = 3;
        },
    );
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86_state) = context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86_state.tsc_adjust, 0xCAFE_BABE_DEAD_BEEF);
}

#[test]
fn msr_accesses_survive_o2_in_program_order() {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, msr_kind(true, 0x1002));
    builder.push_op(0x1002, msr_kind(false, 0x1004));
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut function = builder.finish();
    optimize_function(&mut function, OptLevel::O2);

    let accesses: Vec<_> = function
        .entry_block()
        .unwrap()
        .ops
        .iter()
        .filter_map(|op| match &op.kind {
            OpKind::X86Msr(msr) => Some((msr.write, msr.next_pc)),
            _ => None,
        })
        .collect();
    assert_eq!(accesses, vec![(true, 0x1002), (false, 0x1004)]);
}
