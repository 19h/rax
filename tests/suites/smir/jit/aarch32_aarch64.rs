//! End-to-end A32 instruction → SMIR → native AArch64 regressions.
#![cfg(all(feature = "smir-jit", target_arch = "aarch64"))]

use rax::isa::arm::aarch32::cpu::{ArmMemory, Armv7Cpu, FlatMemory, MemoryError, Psr};
use rax::isa::arm::aarch32::instructions::{ExecResult, Executor};
use rax::isa::arm::decoder::Aarch32Decoder;
use rax::smir::ir::types::{FunctionId, OpWidth, SourceArch};
use rax::smir::ir::{FunctionBuilder, SmirFunction, Terminator};
use rax::smir::lift::aarch32::Aarch32Lifter;
use rax::smir::lift::{LiftContext, SmirLifter};
use rax::smir::lower::SmirLowerer;
use rax::smir::lower::aarch64::Aarch64Lowerer;
use rax::smir::lower::runtime::{
    Aarch32GuestRegs, Aarch32MemHelpers, ExecMem, is_aarch32_aarch64_native_clobber_safe_excluding,
    is_aarch32_aarch64_native_clobber_safe_excluding_with_mem,
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

fn lift_program(program: &[u32]) -> SmirFunction {
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
    builder.finish()
}

fn lower(program: &[u32]) -> (ExecMem, usize) {
    let function = lift_program(program);
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

fn lower_with_mem(program: &[u32]) -> (ExecMem, usize) {
    let function = lift_program(program);
    assert!(
        is_aarch32_aarch64_native_clobber_safe_excluding_with_mem(
            &function,
            &std::collections::HashMap::new(),
            true,
        ),
        "complete A32 memory program must satisfy the production native gate"
    );

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_mem_helper_addr_width(OpWidth::W32);
    let lowered = lowerer.lower_function(&function).expect("AArch64 lower");
    let code = lowerer.finalize().expect("AArch64 finalize");
    (
        ExecMem::new(&code).expect("map A32 native memory region"),
        lowered.entry_offset,
    )
}

const TEST_MEM_SIZE: usize = 256;

#[repr(C)]
#[derive(Clone, Debug, PartialEq, Eq)]
struct TestMemory {
    data: [u8; TEST_MEM_SIZE],
    fault_addr: u32,
    fault_enabled: u64,
    last_helper_addr: u64,
    helper_loads: u64,
    helper_stores: u64,
}

impl TestMemory {
    fn patterned() -> Self {
        let mut data = [0u8; TEST_MEM_SIZE];
        for (index, byte) in data.iter_mut().enumerate() {
            *byte = (index as u8) ^ 0x80;
        }
        Self {
            data,
            fault_addr: 0,
            fault_enabled: 0,
            last_helper_addr: u64::MAX,
            helper_loads: 0,
            helper_stores: 0,
        }
    }

    fn check(&self, addr: u32) -> Result<(), MemoryError> {
        if self.fault_enabled != 0 && addr == self.fault_addr {
            Err(MemoryError::BusError(addr))
        } else {
            Ok(())
        }
    }

    fn byte(&self, addr: u32) -> u8 {
        self.data[addr as usize & (TEST_MEM_SIZE - 1)]
    }

    fn set_byte(&mut self, addr: u32, value: u8) {
        self.data[addr as usize & (TEST_MEM_SIZE - 1)] = value;
    }
}

impl ArmMemory for TestMemory {
    fn read_word(&self, addr: u32) -> Result<u32, MemoryError> {
        self.check(addr)?;
        Ok(u32::from_le_bytes([
            self.byte(addr),
            self.byte(addr.wrapping_add(1)),
            self.byte(addr.wrapping_add(2)),
            self.byte(addr.wrapping_add(3)),
        ]))
    }

    fn write_word(&mut self, addr: u32, value: u32) -> Result<(), MemoryError> {
        self.check(addr)?;
        for (offset, byte) in value.to_le_bytes().into_iter().enumerate() {
            self.set_byte(addr.wrapping_add(offset as u32), byte);
        }
        Ok(())
    }

    fn read_halfword(&self, addr: u32) -> Result<u16, MemoryError> {
        self.check(addr)?;
        Ok(u16::from_le_bytes([
            self.byte(addr),
            self.byte(addr.wrapping_add(1)),
        ]))
    }

    fn write_halfword(&mut self, addr: u32, value: u16) -> Result<(), MemoryError> {
        self.check(addr)?;
        for (offset, byte) in value.to_le_bytes().into_iter().enumerate() {
            self.set_byte(addr.wrapping_add(offset as u32), byte);
        }
        Ok(())
    }

    fn read_byte(&self, addr: u32) -> Result<u8, MemoryError> {
        self.check(addr)?;
        Ok(self.byte(addr))
    }

    fn write_byte(&mut self, addr: u32, value: u8) -> Result<(), MemoryError> {
        self.check(addr)?;
        self.set_byte(addr, value);
        Ok(())
    }
}

/// AAPCS64 two-register result: x0=value, x1=success.
#[repr(C)]
struct LoadRet {
    value: u64,
    ok: u64,
}

extern "C" fn test_load(ctx: *mut TestMemory, addr: u64, size: u32, signed: u32) -> LoadRet {
    let memory = unsafe { &mut *ctx };
    memory.last_helper_addr = addr;
    memory.helper_loads += 1;
    let Ok(addr) = u32::try_from(addr) else {
        return LoadRet { value: 0, ok: 0 };
    };
    let value = match size {
        1 => memory.read_byte(addr).map(|value| {
            if signed != 0 {
                value as i8 as i64 as u64
            } else {
                u64::from(value)
            }
        }),
        2 => memory.read_halfword(addr).map(|value| {
            if signed != 0 {
                value as i16 as i64 as u64
            } else {
                u64::from(value)
            }
        }),
        4 => memory.read_word(addr).map(u64::from),
        _ => Err(MemoryError::BusError(addr)),
    };
    match value {
        Ok(value) => LoadRet { value, ok: 1 },
        Err(_) => LoadRet { value: 0, ok: 0 },
    }
}

extern "C" fn test_store(ctx: *mut TestMemory, addr: u64, value: u64, size: u32) -> u64 {
    let memory = unsafe { &mut *ctx };
    memory.last_helper_addr = addr;
    memory.helper_stores += 1;
    let Ok(addr) = u32::try_from(addr) else {
        return 0;
    };
    let result = match size {
        1 => memory.write_byte(addr, value as u8),
        2 => memory.write_halfword(addr, value as u16),
        4 => memory.write_word(addr, value as u32),
        _ => Err(MemoryError::BusError(addr)),
    };
    u64::from(result.is_ok())
}

fn reference_memory(
    program: &[u32],
    initial: Aarch32GuestRegs,
    mut memory: TestMemory,
) -> (Aarch32GuestRegs, TestMemory, Option<usize>) {
    let mut cpu = Armv7Cpu::new();
    cpu.regs = initial.r;
    cpu.cpsr = Psr::from_u32(initial.cpsr);
    let mut fault_index = None;
    {
        let mut executor = Executor::new(&mut cpu, &mut memory);
        for (index, &raw) in program.iter().enumerate() {
            let insn = Aarch32Decoder::decode(raw).expect("reference memory decode");
            match executor.execute(&insn) {
                ExecResult::Continue => {}
                ExecResult::MemoryFault(_) => {
                    fault_index = Some(index);
                    break;
                }
                other => panic!("reference memory execution failed for {raw:#010x}: {other:?}"),
            }
        }
    }
    (
        Aarch32GuestRegs {
            r: cpu.regs,
            cpsr: cpu.cpsr.to_u32(),
        },
        memory,
        fault_index,
    )
}

fn run_memory_native(
    program: &[u32],
    initial: Aarch32GuestRegs,
    mut memory: TestMemory,
) -> (Aarch32GuestRegs, TestMemory, u64) {
    let (exec, entry) = lower_with_mem(program);
    let mut regs = initial;
    let helpers = Aarch32MemHelpers {
        ctx: (&mut memory as *mut TestMemory) as u64,
        load_fn: test_load as usize as u64,
        store_fn: test_store as usize as u64,
    };
    let exit_pc = exec.run_aarch32_identity_with_mem(entry, &mut regs, helpers);
    (regs, memory, exit_pc)
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

#[test]
fn a32_memory_width_sign_store_and_r13_base_match_interpreter() {
    let program = [
        0xe591_0004, // ldr   r0,[r1,#4]
        0xe5d3_2002, // ldrb  r2,[r3,#2]
        0xe1d5_40b2, // ldrh  r4,[r5,#2]
        0xe1d7_60d1, // ldrsb r6,[r7,#1]
        0xe1d9_80f2, // ldrsh r8,[r9,#2]
        0xe59d_c004, // ldr   r12,[r13,#4]
        0xe58b_0004, // str   r0,[r11,#4]
        0xe5cb_6008, // strb  r6,[r11,#8]
        0xe1cb_80ba, // strh  r8,[r11,#10]
    ];
    let mut initial = initial_state();
    initial.r[1] = 0x10;
    initial.r[3] = 0x20;
    initial.r[5] = 0x30;
    initial.r[7] = 0x40;
    initial.r[9] = 0x50;
    initial.r[11] = 0x60;
    initial.r[13] = 0x70;
    let memory = TestMemory::patterned();

    let (expected_regs, expected_mem, fault) = reference_memory(&program, initial, memory.clone());
    assert_eq!(fault, None);
    let (actual_regs, actual_mem, exit_pc) = run_memory_native(&program, initial, memory);

    assert_eq!(actual_regs, expected_regs);
    assert_eq!(actual_mem.data, expected_mem.data);
    assert_eq!(exit_pc, 0, "self-contained region returned normally");
    assert_eq!(actual_mem.helper_loads, 6);
    assert_eq!(actual_mem.helper_stores, 3);
    assert_eq!(actual_regs.r[6], 0xffff_ffc1, "LDRSB sign extension");
    assert_eq!(actual_regs.r[8], 0xffff_d3d2, "LDRSH sign extension");
    assert_eq!(
        actual_regs.r[12], 0xf7f6_f5f4,
        "r13 is an ordinary A32 base"
    );
}

#[test]
fn a32_immediate_register_scaled_and_postindex_writeback_match_interpreter() {
    let program = [
        0xe5b1_0004, // ldr r0,[r1,#4]!
        0xe482_0004, // str r0,[r2],#4
        0xe611_3004, // ldr r3,[r1],-r4
        0xe795_6107, // ldr r6,[r5,r7,lsl #2]
    ];
    let mut initial = initial_state();
    initial.r[1] = 0x20;
    initial.r[2] = 0x60;
    initial.r[4] = 4;
    initial.r[5] = 0x10;
    initial.r[7] = 2;
    let memory = TestMemory::patterned();

    let (expected_regs, expected_mem, fault) = reference_memory(&program, initial, memory.clone());
    assert_eq!(fault, None);
    let (actual_regs, actual_mem, exit_pc) = run_memory_native(&program, initial, memory);

    assert_eq!(actual_regs, expected_regs);
    assert_eq!(actual_mem.data, expected_mem.data);
    assert_eq!(exit_pc, 0);
    assert_eq!(actual_regs.r[1], 0x20, "pre-add then post-sub writeback");
    assert_eq!(actual_regs.r[2], 0x64, "store post-index writeback");
}

#[test]
fn a32_helper_fault_is_precise_and_writeback_atomic_for_load_and_store() {
    for (program, label) in [
        ([0xe5b1_0004], "load"),  // ldr r0,[r1,#4]!
        ([0xe5a1_0004], "store"), // str r0,[r1,#4]!
    ] {
        let mut initial = initial_state();
        initial.r[0] = 0x1122_3344;
        initial.r[1] = 0x20;
        let mut memory = TestMemory::patterned();
        memory.fault_enabled = 1;
        memory.fault_addr = 0x24;

        let (expected_regs, expected_mem, fault) =
            reference_memory(&program, initial, memory.clone());
        assert_eq!(fault, Some(0), "{label} reference fault");
        let (actual_regs, actual_mem, exit_pc) = run_memory_native(&program, initial, memory);

        assert_eq!(actual_regs, expected_regs, "{label} architectural state");
        assert_eq!(actual_mem.data, expected_mem.data, "{label} memory state");
        assert_eq!(actual_regs.r[0], initial.r[0], "{label} destination/source");
        assert_eq!(actual_regs.r[1], initial.r[1], "{label} writeback");
        assert_eq!(exit_pc, 0x8000, "{label} faulting guest PC");
    }
}

#[test]
fn a32_helper_effective_addresses_wrap_modulo_2_pow_32() {
    for (program, configure, label) in [
        ([0xe591_0008], (0xffff_fffcu32, 0u32), "immediate"), // ldr r0,[r1,#8]
        ([0xe791_0102], (0xffff_fffcu32, 2u32), "scaled-register"), // ldr r0,[r1,r2,lsl #2]
    ] {
        let mut initial = initial_state();
        initial.r[1] = configure.0;
        initial.r[2] = configure.1;
        let memory = TestMemory::patterned();

        let (expected_regs, _, fault) = reference_memory(&program, initial, memory.clone());
        assert_eq!(fault, None, "{label}");
        let (actual_regs, actual_mem, exit_pc) = run_memory_native(&program, initial, memory);

        assert_eq!(actual_regs, expected_regs, "{label} value");
        assert_eq!(actual_mem.last_helper_addr, 4, "{label} helper address");
        assert_eq!(exit_pc, 0, "{label} normal return");
    }
}

#[test]
fn a32_signed_load_result_is_canonical_before_direct_address_reuse() {
    let program = [
        0xe1d7_10d1, // ldrsb r1,[r7,#1] => 0xffff_ffc1
        0xe591_0000, // ldr   r0,[r1]
    ];
    let mut initial = initial_state();
    initial.r[7] = 0x40;
    let memory = TestMemory::patterned();

    let (expected_regs, _, fault) = reference_memory(&program, initial, memory.clone());
    assert_eq!(fault, None);
    let (actual_regs, actual_mem, exit_pc) = run_memory_native(&program, initial, memory);

    assert_eq!(actual_regs, expected_regs);
    assert_eq!(actual_regs.r[1], 0xffff_ffc1);
    assert_eq!(actual_mem.last_helper_addr, 0xffff_ffc1);
    assert_eq!(exit_pc, 0);
}
