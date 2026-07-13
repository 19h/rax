//! End-to-end A32 instruction → SMIR → native AArch64 regressions.
#![cfg(all(feature = "smir-jit", target_arch = "aarch64"))]

use rax::isa::arm::aarch32::cpu::{Armv7Cpu, FlatMemory, Psr};
use rax::isa::arm::aarch32::instructions::{ExecResult, Executor};
use rax::isa::arm::decoder::Aarch32Decoder;
use rax::smir::ir::types::{FunctionId, SourceArch};
use rax::smir::ir::{FunctionBuilder, Terminator};
use rax::smir::lift::aarch32::Aarch32Lifter;
use rax::smir::lift::{LiftContext, SmirLifter};
use rax::smir::lower::SmirLowerer;
use rax::smir::lower::aarch64::Aarch64Lowerer;
use rax::smir::lower::runtime::{
    Aarch32GuestRegs, ExecMem, is_aarch32_aarch64_native_clobber_safe_excluding,
};

const PROGRAM: [u32; 29] = [
    0xe081_0002, // add   r0,r1,r2
    0xe054_3385, // subs  r3,r4,r5,lsl #7
    0xe2a7_60ff, // adc   r6,r7,#255
    0xe0d9_800a, // sbcs  r8,r9,r10
    0xe26c_b007, // rsb   r11,r12,#7
    0xe002_14e3, // and   r1,r2,r3,ror #9
    0xe385_4102, // orr   r4,r5,#0x80000000
    0xe027_6008, // eor   r6,r7,r8
    0xe1ca_900b, // bic   r9,r10,r11
    0xe1a0_0241, // mov   r0,r1,asr #4
    0xe1e0_2003, // mvn   r2,r3
    0xe004_0695, // mul   r4,r5,r6
    0xe027_a998, // mla   r7,r8,r9,r10
    0xe06b_109c, // mls   r11,r12,r0,r1
    0xe081_0392, // umull r0,r1,r2,r3
    0xe0c5_4796, // smull r4,r5,r6,r7
    0xe16f_2f13, // clz   r2,r3
    0xe6bf_4f35, // rev   r4,r5
    0xe6ff_8f39, // rbit  r8,r9
    0xe7cb_021f, // bfc   r0,#4,#8
    0xe7cf_1412, // bfi   r1,r2,#8,#8
    0xe7e6_3654, // ubfx  r3,r4,#12,#7
    0xe7a7_5856, // sbfx  r5,r6,#16,#8
    0xe730_fa11, // udiv  r0,r1,r10
    0xe713_fb14, // sdiv  r3,r4,r11
    0xe30b_aeef, // movw  r10,#0xbeef
    0xe34c_aafe, // movt  r10,#0xcafe
    0xe15a_000b, // cmp   r10,r11
    0xe37c_0001, // cmn   r12,#1
];

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
        // Seed every non-NZCV category as a preservation canary: Q, GE,
        // endianness, interrupt masks, state, and Supervisor mode.
        cpsr: 0xa80f_03d3,
    }
}

fn reference(program: &[u32], initial: Aarch32GuestRegs) -> Aarch32GuestRegs {
    let mut cpu = Armv7Cpu::new();
    cpu.regs = initial.r;
    cpu.cpsr = Psr::from_u32(initial.cpsr);
    let mut memory = FlatMemory::new(0x10_000, 0);
    let mut executor = Executor::new(&mut cpu, &mut memory);
    for &raw in program {
        let insn = Aarch32Decoder::decode(raw).expect("reference decode");
        assert!(
            matches!(executor.execute(&insn), ExecResult::Continue),
            "reference execution failed for {raw:#010x}"
        );
    }
    Aarch32GuestRegs {
        r: executor.cpu.regs,
        cpsr: executor.cpu.cpsr.to_u32(),
    }
}

fn lower(program: &[u32]) -> (ExecMem, usize) {
    let mut lifter = Aarch32Lifter::new();
    let mut context = LiftContext::new(SourceArch::Aarch32);
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x8000);
    for (index, &raw) in program.iter().enumerate() {
        let pc = 0x8000 + index as u64 * 4;
        let result = lifter
            .lift_insn(pc, &raw.to_le_bytes(), &mut context)
            .unwrap_or_else(|error| panic!("lift {raw:#010x}: {error}"));
        assert!(
            !result.control_flow.ends_block(),
            "test instruction unexpectedly terminates its block"
        );
        for op in result.ops {
            builder.push_op(pc, op.kind);
        }
    }
    builder.set_terminator(Terminator::Return { values: Vec::new() });
    let function = builder.finish();
    assert!(
        is_aarch32_aarch64_native_clobber_safe_excluding(
            &function,
            &std::collections::HashMap::new(),
        ),
        "complete A32 scalar program must satisfy the production native gate"
    );

    let mut lowerer = Aarch64Lowerer::new();
    let lowered = lowerer.lower_function(&function).expect("AArch64 lower");
    let code = lowerer.finalize().expect("AArch64 finalize");
    (
        ExecMem::new(&code).expect("map A32 native region"),
        lowered.entry_offset,
    )
}

#[test]
fn a32_scalar_integer_program_executes_natively_with_interpreter_parity() {
    let initial = initial_state();
    let expected = reference(&PROGRAM, initial);
    let (exec, entry) = lower(&PROGRAM);
    let mut actual = initial;
    exec.run_aarch32_identity(entry, &mut actual);

    assert_eq!(actual.r, expected.r, "complete A32 GPR file");
    assert_eq!(actual.cpsr, expected.cpsr, "complete A32 CPSR");
    assert_eq!(
        actual.r[15], initial.r[15],
        "PC snapshot is not a native operand"
    );
    assert_eq!(
        actual.cpsr & 0x0fff_ffff,
        initial.cpsr & 0x0fff_ffff,
        "non-NZCV CPSR fields remain stable"
    );
}

#[test]
fn a32_identity_bridge_narrows_every_result_and_preserves_unmapped_cpsr() {
    let program = [
        0xe3a0_0000, // mov r0,#0
        0xe280_0001, // add r0,r0,#1
        0xe290_1000, // adds r1,r0,#0
    ];
    let initial = initial_state();
    let expected = reference(&program, initial);
    let (exec, entry) = lower(&program);
    let mut actual = initial;
    exec.run_aarch32_identity(entry, &mut actual);

    assert_eq!(actual, expected);
    assert_eq!(actual.r[0], 1);
    assert_eq!(actual.r[1], 1);
    assert_eq!(actual.cpsr & 0x0fff_ffff, initial.cpsr & 0x0fff_ffff);
}

#[test]
fn a32_bitfield_full_width_and_destructive_aliases_match_interpreter() {
    let program = [
        0xe7cf_1411, // bfi  r1,r1,#8,#8 (source aliases destination)
        0xe7df_701f, // bfc  r7,#0,#32
        0xe7ff_3054, // ubfx r3,r4,#0,#32
        0xe7bf_5056, // sbfx r5,r6,#0,#32
    ];
    let initial = initial_state();
    let expected = reference(&program, initial);
    let (exec, entry) = lower(&program);
    let mut actual = initial;
    exec.run_aarch32_identity(entry, &mut actual);

    assert_eq!(actual, expected);
    assert_eq!(actual.r[7], 0, "full-width BFC clears all 32 bits");
    assert_eq!(actual.r[3], initial.r[4], "full-width UBFX is exact");
    assert_eq!(actual.r[5], initial.r[6], "full-width SBFX is exact");
}

#[test]
fn a32_division_zero_and_signed_overflow_match_interpreter() {
    let zero_divisors = [
        0xe3a0_a000, // mov  r10,#0
        0xe730_fa11, // udiv r0,r1,r10
        0xe713_fb14, // sdiv r3,r4,r11 (nonzero control)
        0xe713_fa14, // sdiv r3,r4,r10
    ];
    let initial = initial_state();
    let expected = reference(&zero_divisors, initial);
    let (exec, entry) = lower(&zero_divisors);
    let mut actual = initial;
    exec.run_aarch32_identity(entry, &mut actual);
    assert_eq!(actual, expected);
    assert_eq!(actual.r[0], 0, "UDIV by zero returns zero");
    assert_eq!(actual.r[3], 0, "SDIV by zero returns zero");

    let signed_overflow = [
        0xe300_4000, // movw r4,#0
        0xe348_4000, // movt r4,#0x8000
        0xe30f_afff, // movw r10,#0xffff
        0xe34f_afff, // movt r10,#0xffff
        0xe713_fa14, // sdiv r3,r4,r10
    ];
    let expected = reference(&signed_overflow, initial);
    let (exec, entry) = lower(&signed_overflow);
    let mut actual = initial;
    exec.run_aarch32_identity(entry, &mut actual);
    assert_eq!(actual, expected);
    assert_eq!(actual.r[3], i32::MIN as u32);
}
