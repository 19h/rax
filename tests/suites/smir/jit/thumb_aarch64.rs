//! End-to-end T16/T32 instruction -> SMIR -> native AArch64 regressions.
#![cfg(all(feature = "smir-jit", target_arch = "aarch64"))]

use std::collections::HashMap;

use rax::isa::arm::ExecutionState;
use rax::isa::arm::aarch32::cpu::{Armv7Cpu, FlatMemory, Psr};
use rax::isa::arm::aarch32::instructions::{ExecResult, Executor};
use rax::isa::arm::decoder::Decoder;
use rax::smir::ir::types::{FunctionId, SourceArch};
use rax::smir::ir::{FunctionBuilder, Terminator};
use rax::smir::lift::thumb::ThumbLifter;
use rax::smir::lift::{LiftContext, SmirLifter};
use rax::smir::lower::SmirLowerer;
use rax::smir::lower::aarch64::Aarch64Lowerer;
use rax::smir::lower::runtime::{
    Aarch32GuestRegs, ExecMem, is_aarch32_aarch64_native_clobber_safe_excluding,
};

#[derive(Clone, Copy)]
struct ThumbInsn {
    bytes: &'static [u8],
    asm: &'static str,
}

const PROGRAM: &[ThumbInsn] = &[
    ThumbInsn {
        bytes: &[0x88, 0x18],
        asm: "adds r0,r1,r2",
    },
    ThumbInsn {
        bytes: &[0x63, 0x1f],
        asm: "subs r3,r4,#5",
    },
    ThumbInsn {
        bytes: &[0x75, 0x41],
        asm: "adcs r5,r6",
    },
    ThumbInsn {
        bytes: &[0x87, 0x41],
        asm: "sbcs r7,r0",
    },
    ThumbInsn {
        bytes: &[0x80, 0x29],
        asm: "cmp r1,#128",
    },
    ThumbInsn {
        bytes: &[0xda, 0x42],
        asm: "cmn r2,r3",
    },
    ThumbInsn {
        bytes: &[0x6c, 0x42],
        asm: "rsbs r4,r5,#0",
    },
    ThumbInsn {
        bytes: &[0xc8, 0x44],
        asm: "add r8,r9",
    },
    ThumbInsn {
        bytes: &[0xda, 0x46],
        asm: "mov r10,r11",
    },
    ThumbInsn {
        bytes: &[0x08, 0xba],
        asm: "rev r0,r1",
    },
    ThumbInsn {
        bytes: &[0x85, 0x44],
        asm: "add sp,r0",
    },
    ThumbInsn {
        bytes: &[0xec, 0x46],
        asm: "mov r12,sp",
    },
    ThumbInsn {
        bytes: &[0x01, 0xeb, 0xc2, 0x00],
        asm: "add.w r0,r1,r2,lsl #3",
    },
    ThumbInsn {
        bytes: &[0xa4, 0xf2, 0x23, 0x13],
        asm: "subw r3,r4,#0x123",
    },
    ThumbInsn {
        bytes: &[0x46, 0xeb, 0x07, 0x05],
        asm: "adc.w r5,r6,r7",
    },
    ThumbInsn {
        bytes: &[0x69, 0xeb, 0x0a, 0x08],
        asm: "sbc.w r8,r9,r10",
    },
    ThumbInsn {
        bytes: &[0x0c, 0xea, 0x70, 0x1b],
        asm: "and.w r11,r12,r0,ror #5",
    },
    ThumbInsn {
        bytes: &[0x42, 0xf0, 0x00, 0x41],
        asm: "orr r1,r2,#0x80000000",
    },
    ThumbInsn {
        bytes: &[0x84, 0xea, 0x05, 0x03],
        asm: "eor.w r3,r4,r5",
    },
    ThumbInsn {
        bytes: &[0x27, 0xea, 0x08, 0x06],
        asm: "bic.w r6,r7,r8",
    },
    ThumbInsn {
        bytes: &[0x6f, 0xea, 0x0a, 0x09],
        asm: "mvn.w r9,r10",
    },
    ThumbInsn {
        bytes: &[0x4f, 0xea, 0xcc, 0x1b],
        asm: "lsl.w r11,r12,#7",
    },
    ThumbInsn {
        bytes: &[0x09, 0xfb, 0x0a, 0xf8],
        asm: "mul r8,r9,r10",
    },
    ThumbInsn {
        bytes: &[0x04, 0xfb, 0x05, 0x63],
        asm: "mla r3,r4,r5,r6",
    },
    ThumbInsn {
        bytes: &[0x08, 0xfb, 0x19, 0xa7],
        asm: "mls r7,r8,r9,r10",
    },
    ThumbInsn {
        bytes: &[0xa2, 0xfb, 0x03, 0x01],
        asm: "umull r0,r1,r2,r3",
    },
    ThumbInsn {
        bytes: &[0x86, 0xfb, 0x07, 0x45],
        asm: "smull r4,r5,r6,r7",
    },
    ThumbInsn {
        bytes: &[0xb9, 0xfb, 0xfa, 0xf8],
        asm: "udiv r8,r9,r10",
    },
    ThumbInsn {
        bytes: &[0x9c, 0xfb, 0xf0, 0xfb],
        asm: "sdiv r11,r12,r0",
    },
    ThumbInsn {
        bytes: &[0xb2, 0xfa, 0x82, 0xf1],
        asm: "clz r1,r2",
    },
    ThumbInsn {
        bytes: &[0x94, 0xfa, 0xa4, 0xf3],
        asm: "rbit r3,r4",
    },
    ThumbInsn {
        bytes: &[0x96, 0xfa, 0x86, 0xf5],
        asm: "rev.w r5,r6",
    },
    ThumbInsn {
        bytes: &[0x4b, 0xf6, 0xef, 0x67],
        asm: "movw r7,#0xbeef",
    },
    ThumbInsn {
        bytes: &[0xcc, 0xf6, 0xfe, 0x27],
        asm: "movt r7,#0xcafe",
    },
    ThumbInsn {
        bytes: &[0x6f, 0xf3, 0x0b, 0x18],
        asm: "bfc r8,#4,#8",
    },
    ThumbInsn {
        bytes: &[0x6a, 0xf3, 0x0f, 0x29],
        asm: "bfi r9,r10,#8,#8",
    },
    ThumbInsn {
        bytes: &[0xcc, 0xf3, 0x06, 0x3b],
        asm: "ubfx r11,r12,#12,#7",
    },
    ThumbInsn {
        bytes: &[0x41, 0xf3, 0x07, 0x40],
        asm: "sbfx r0,r1,#16,#8",
    },
    ThumbInsn {
        bytes: &[0x4f, 0xfa, 0x83, 0xf2],
        asm: "sxtb.w r2,r3",
    },
    ThumbInsn {
        bytes: &[0x0f, 0xfa, 0x85, 0xf4],
        asm: "sxth.w r4,r5",
    },
    ThumbInsn {
        bytes: &[0x5f, 0xfa, 0x87, 0xf6],
        asm: "uxtb.w r6,r7",
    },
    ThumbInsn {
        bytes: &[0x1f, 0xfa, 0x89, 0xf8],
        asm: "uxth.w r8,r9",
    },
];

fn insn(bytes: &'static [u8], asm: &'static str) -> ThumbInsn {
    ThumbInsn { bytes, asm }
}

fn initial_state() -> Aarch32GuestRegs {
    let mut r = [0u32; 16];
    for (index, value) in r.iter_mut().enumerate() {
        *value = 0x1020_3040u32
            .wrapping_mul(index as u32 + 1)
            .rotate_left(index as u32);
    }
    r[15] = 0x8000;
    Aarch32GuestRegs {
        r,
        // NZCV + Q + GE + E/A/I/F + T + Supervisor mode; IT state is zero.
        cpsr: 0xa80f_03f3,
    }
}

fn reference(program: &[ThumbInsn], initial: Aarch32GuestRegs) -> Aarch32GuestRegs {
    let mut cpu = Armv7Cpu::new();
    cpu.regs = initial.r;
    cpu.cpsr = Psr::from_u32(initial.cpsr);
    assert!(cpu.cpsr.t, "reference must execute in Thumb state");
    assert_eq!(cpu.cpsr.it_state, 0, "native subset excludes IT state");
    let mut memory = FlatMemory::new(0x10_000, 0);
    let mut executor = Executor::new(&mut cpu, &mut memory);
    let decoder = Decoder::new(ExecutionState::Thumb);
    for item in program {
        let decoded = decoder
            .decode(item.bytes)
            .unwrap_or_else(|error| panic!("reference decode {}: {error}", item.asm));
        assert_eq!(
            decoded.size as usize,
            item.bytes.len(),
            "{} width",
            item.asm
        );
        assert!(
            matches!(executor.execute(&decoded), ExecResult::Continue),
            "reference execution failed for {}",
            item.asm
        );
    }
    Aarch32GuestRegs {
        r: executor.cpu.regs,
        cpsr: executor.cpu.cpsr.to_u32(),
    }
}

fn lower(program: &[ThumbInsn]) -> (ExecMem, usize) {
    let mut lifter = ThumbLifter::new();
    let mut context = LiftContext::new(SourceArch::Thumb);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x8000);
    let mut pc = 0x8000;
    for item in program {
        let result = lifter
            .lift_insn(pc, item.bytes, &mut context)
            .unwrap_or_else(|error| panic!("lift {}: {error}", item.asm));
        assert_eq!(
            result.bytes_consumed,
            item.bytes.len(),
            "{} width",
            item.asm
        );
        assert!(
            !result.control_flow.ends_block(),
            "{} unexpectedly terminates its block",
            item.asm
        );
        let mut instruction = FunctionBuilder::new(FunctionId(1), pc);
        for op in &result.ops {
            instruction.push_op(pc, op.kind.clone());
        }
        instruction.set_terminator(Terminator::Return { values: Vec::new() });
        assert!(
            is_aarch32_aarch64_native_clobber_safe_excluding(
                &instruction.finish(),
                &HashMap::new(),
            ),
            "{} must satisfy the production native gate: {:?}",
            item.asm,
            result.ops,
        );
        for op in result.ops {
            builder.push_op(pc, op.kind);
        }
        pc += result.bytes_consumed as u64;
    }
    builder.set_terminator(Terminator::Return { values: Vec::new() });
    let function = builder.finish();
    assert!(
        is_aarch32_aarch64_native_clobber_safe_excluding(&function, &HashMap::new()),
        "complete Thumb scalar program must satisfy the production native gate"
    );

    let mut lowerer = Aarch64Lowerer::new();
    let lowered = lowerer.lower_function(&function).expect("AArch64 lower");
    let code = lowerer.finalize().expect("AArch64 finalize");
    (
        ExecMem::new(&code).expect("map Thumb native region"),
        lowered.entry_offset,
    )
}

fn assert_native_parity(program: &[ThumbInsn], initial: Aarch32GuestRegs) -> Aarch32GuestRegs {
    let expected = reference(program, initial);
    let (exec, entry) = lower(program);
    let mut actual = initial;
    exec.run_aarch32_identity(entry, &mut actual);
    assert_eq!(actual.r, expected.r, "complete Thumb GPR file");
    assert_eq!(actual.cpsr, expected.cpsr, "complete Thumb CPSR");
    assert_eq!(actual.r[15], initial.r[15], "PC is not a native operand");
    assert_eq!(
        actual.cpsr & 0x0fff_ffff,
        initial.cpsr & 0x0fff_ffff,
        "Q/IT/GE/E/A/I/F/T/mode fields remain stable"
    );
    actual
}

#[test]
fn mixed_t16_t32_scalar_program_executes_natively_with_interpreter_parity() {
    let initial = initial_state();
    let actual = assert_native_parity(PROGRAM, initial);
    assert_eq!(
        actual.r[12], actual.r[13],
        "r13 is identity-mapped, not host SP"
    );
}

#[test]
fn thumb_bitfield_full_width_and_destructive_aliases_match_interpreter() {
    let program = [
        insn(&[0x61, 0xf3, 0x0f, 0x21], "bfi r1,r1,#8,#8"),
        insn(&[0x6f, 0xf3, 0x1f, 0x07], "bfc r7,#0,#32"),
        insn(&[0xc4, 0xf3, 0x1f, 0x03], "ubfx r3,r4,#0,#32"),
        insn(&[0x46, 0xf3, 0x1f, 0x05], "sbfx r5,r6,#0,#32"),
    ];
    let initial = initial_state();
    let actual = assert_native_parity(&program, initial);
    assert_eq!(actual.r[7], 0, "full-width BFC clears all 32 bits");
    assert_eq!(actual.r[3], initial.r[4], "full-width UBFX is exact");
    assert_eq!(actual.r[5], initial.r[6], "full-width SBFX is exact");
}

#[test]
fn thumb_division_zero_and_signed_overflow_match_interpreter() {
    let zero_divisors = [
        insn(&[0x40, 0xf2, 0x00, 0x0a], "movw r10,#0"),
        insn(&[0xb1, 0xfb, 0xfa, 0xf0], "udiv r0,r1,r10"),
        insn(&[0xb4, 0xfb, 0xfa, 0xf3], "udiv r3,r4,r10"),
        insn(&[0x94, 0xfb, 0xfa, 0xf3], "sdiv r3,r4,r10"),
    ];
    let initial = initial_state();
    let actual = assert_native_parity(&zero_divisors, initial);
    assert_eq!(actual.r[0], 0, "UDIV by zero returns zero");
    assert_eq!(actual.r[3], 0, "SDIV by zero returns zero");

    let signed_overflow = [
        insn(&[0x40, 0xf2, 0x00, 0x04], "movw r4,#0"),
        insn(&[0xc8, 0xf2, 0x00, 0x04], "movt r4,#0x8000"),
        insn(&[0x4f, 0xf6, 0xff, 0x7a], "movw r10,#0xffff"),
        insn(&[0xcf, 0xf6, 0xff, 0x7a], "movt r10,#0xffff"),
        insn(&[0x94, 0xfb, 0xfa, 0xf3], "sdiv r3,r4,r10"),
    ];
    let actual = assert_native_parity(&signed_overflow, initial);
    assert_eq!(actual.r[3], i32::MIN as u32);
}

#[test]
fn thumb_identity_bridge_preserves_thumb_and_non_nzcv_cpsr_state() {
    let program = [
        insn(&[0x88, 0x18], "adds r0,r1,r2"),
        insn(&[0x80, 0x29], "cmp r1,#128"),
    ];
    let initial = initial_state();
    let actual = assert_native_parity(&program, initial);
    assert_ne!(actual.cpsr & 0xf000_0000, initial.cpsr & 0xf000_0000);
    assert_eq!(actual.cpsr & (1 << 5), 1 << 5, "CPSR.T remains set");
}
