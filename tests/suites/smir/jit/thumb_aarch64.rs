//! End-to-end T16/T32 instruction -> SMIR -> native AArch64 regressions.
#![cfg(all(feature = "smir-jit", target_arch = "aarch64"))]

use std::collections::HashMap;

use rax::isa::arm::ExecutionState;
use rax::isa::arm::aarch32::cpu::{ArmMemory, Armv7Cpu, FlatMemory, MemoryError, Psr};
use rax::isa::arm::aarch32::instructions::{ExecResult, Executor};
use rax::isa::arm::decoder::Decoder;
use rax::smir::ir::memory::MemoryError as SmirMemoryError;
use rax::smir::ir::types::{BlockId, FunctionId, OpWidth, SourceArch};
use rax::smir::ir::{FunctionBuilder, SmirBlock, SmirFunction, Terminator};
use rax::smir::lift::thumb::ThumbLifter;
use rax::smir::lift::{LiftContext, MemoryReader, SmirLifter};
use rax::smir::lower::SmirLowerer;
use rax::smir::lower::aarch64::Aarch64Lowerer;
use rax::smir::lower::runtime::{
    Aarch32GuestRegs, Aarch32MemHelpers, ExecMem,
    is_aarch32_aarch64_native_clobber_safe_excluding_with_mem,
};
use rax::smir::optimize::{OptLevel, optimize_function};

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
    lower_configured(program, false)
}

fn lower_with_mem(program: &[ThumbInsn]) -> (ExecMem, usize) {
    lower_configured(program, true)
}

fn lower_configured(program: &[ThumbInsn], allow_mem: bool) -> (ExecMem, usize) {
    lower_configured_at(program, allow_mem, OptLevel::O0)
}

fn lower_with_mem_at(program: &[ThumbInsn], level: OptLevel) -> (ExecMem, usize) {
    lower_configured_at(program, true, level)
}

fn lower_configured_at(
    program: &[ThumbInsn],
    allow_mem: bool,
    level: OptLevel,
) -> (ExecMem, usize) {
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
            is_aarch32_aarch64_native_clobber_safe_excluding_with_mem(
                &instruction.finish(),
                &HashMap::new(),
                allow_mem,
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
    let mut function = builder.finish();
    optimize_function(&mut function, level);
    assert!(
        is_aarch32_aarch64_native_clobber_safe_excluding_with_mem(
            &function,
            &HashMap::new(),
            allow_mem,
        ),
        "complete Thumb scalar program at {level:?} must satisfy the production native gate"
    );

    let mut lowerer = Aarch64Lowerer::new();
    if allow_mem {
        lowerer.set_mem_helpers(true);
        lowerer.set_mem_helper_addr_width(OpWidth::W32);
    }
    let lowered = lowerer.lower_function(&function).expect("AArch64 lower");
    let code = lowerer.finalize().expect("AArch64 finalize");
    (
        ExecMem::new(&code).expect("map Thumb native region"),
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
            *byte = index as u8 ^ 0x80;
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
    program: &[ThumbInsn],
    initial: Aarch32GuestRegs,
    mut memory: TestMemory,
) -> (Aarch32GuestRegs, TestMemory, Option<usize>) {
    let mut cpu = Armv7Cpu::new();
    cpu.regs = initial.r;
    cpu.cpsr = Psr::from_u32(initial.cpsr);
    let decoder = Decoder::new(ExecutionState::Thumb);
    let mut fault_index = None;
    {
        let mut executor = Executor::new(&mut cpu, &mut memory);
        for (index, item) in program.iter().enumerate() {
            let decoded = decoder
                .decode(item.bytes)
                .unwrap_or_else(|error| panic!("reference decode {}: {error}", item.asm));
            match executor.execute(&decoded) {
                ExecResult::Continue => {}
                ExecResult::MemoryFault(_) => {
                    fault_index = Some(index);
                    break;
                }
                other => panic!(
                    "reference memory execution failed for {}: {other:?}",
                    item.asm
                ),
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
    program: &[ThumbInsn],
    initial: Aarch32GuestRegs,
    memory: TestMemory,
) -> (Aarch32GuestRegs, TestMemory, u64) {
    run_memory_native_at(program, initial, memory, OptLevel::O0)
}

fn run_memory_native_at(
    program: &[ThumbInsn],
    initial: Aarch32GuestRegs,
    mut memory: TestMemory,
    level: OptLevel,
) -> (Aarch32GuestRegs, TestMemory, u64) {
    let (exec, entry) = lower_with_mem_at(program, level);
    let mut regs = initial;
    let helpers = Aarch32MemHelpers {
        ctx: (&mut memory as *mut TestMemory) as u64,
        load_fn: test_load as usize as u64,
        store_fn: test_store as usize as u64,
    };
    let exit_pc = exec.run_aarch32_identity_with_mem(entry, &mut regs, helpers);
    (regs, memory, exit_pc)
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

#[test]
fn mixed_t16_t32_scalar_memory_matches_interpreter() {
    let program = [
        insn(&[0x88, 0x56], "ldrsb r0,[r1,r2]"),
        insn(&[0x63, 0x5f], "ldrsh r3,[r4,r5]"),
        insn(&[0x01, 0x9e], "ldr r6,[sp,#4]"),
        insn(&[0xf8, 0x70], "strb r0,[r7,#3]"),
        insn(&[0xbb, 0x80], "strh r3,[r7,#4]"),
        insn(&[0xbe, 0x60], "str r6,[r7,#8]"),
        insn(&[0x59, 0xf8, 0x04, 0x8f], "ldr.w r8,[r9,#4]!"),
        insn(&[0x4a, 0xf8, 0x08, 0x89], "str.w r8,[r10],#-8"),
        insn(&[0x9c, 0xf9, 0x07, 0xb0], "ldrsb.w r11,[r12,#7]"),
        insn(&[0x8d, 0xf8, 0x0c, 0xb0], "strb.w r11,[sp,#12]"),
    ];
    let mut initial = initial_state();
    initial.r[1] = 0x10;
    initial.r[2] = 1;
    initial.r[4] = 0x20;
    initial.r[5] = 2;
    initial.r[7] = 0x40;
    initial.r[9] = 0x50;
    initial.r[10] = 0x70;
    initial.r[12] = 0x60;
    initial.r[13] = 0x30;
    let memory = TestMemory::patterned();

    let (expected_regs, expected_mem, fault) = reference_memory(&program, initial, memory.clone());
    assert_eq!(fault, None);
    let (actual_regs, actual_mem, exit_pc) = run_memory_native(&program, initial, memory);

    assert_eq!(actual_regs, expected_regs);
    assert_eq!(actual_mem.data, expected_mem.data);
    assert_eq!(actual_regs.r[0], 0xffff_ff91, "T16 LDRSB");
    assert_eq!(actual_regs.r[3], 0xffff_a3a2, "T16 LDRSH");
    assert_eq!(actual_regs.r[9], 0x54, "T32 pre-index writeback");
    assert_eq!(actual_regs.r[10], 0x68, "T32 post-index writeback");
    assert_eq!(actual_mem.helper_loads, 5);
    assert_eq!(actual_mem.helper_stores, 5);
    assert_eq!(exit_pc, 0);
}

#[test]
fn thumb_helper_fault_is_precise_and_writeback_atomic() {
    for (program, label) in [
        ([insn(&[0x51, 0xf8, 0x04, 0x0f], "ldr r0,[r1,#4]!")], "load"),
        (
            [insn(&[0x41, 0xf8, 0x04, 0x0f], "str r0,[r1,#4]!")],
            "store",
        ),
    ] {
        let mut initial = initial_state();
        initial.r[0] = 0x1122_3344;
        initial.r[1] = 0x20;
        let mut memory = TestMemory::patterned();
        memory.fault_enabled = 1;
        memory.fault_addr = 0x24;

        let (expected_regs, expected_mem, fault) =
            reference_memory(&program, initial, memory.clone());
        assert_eq!(fault, Some(0), "{label}");
        let (actual_regs, actual_mem, exit_pc) = run_memory_native(&program, initial, memory);

        assert_eq!(actual_regs, expected_regs, "{label} state");
        assert_eq!(actual_mem.data, expected_mem.data, "{label} memory");
        assert_eq!(actual_regs.r[1], initial.r[1], "{label} writeback");
        assert_eq!(exit_pc, 0x8000, "{label} fault PC");
    }
}

#[test]
fn thumb_helper_effective_address_wraps_modulo_2_pow_32() {
    let program = [insn(&[0xd1, 0xf8, 0x08, 0x00], "ldr.w r0,[r1,#8]")];
    let mut initial = initial_state();
    initial.r[1] = 0xffff_fffc;
    let memory = TestMemory::patterned();

    let (expected_regs, _, fault) = reference_memory(&program, initial, memory.clone());
    assert_eq!(fault, None);
    let (actual_regs, actual_mem, exit_pc) = run_memory_native(&program, initial, memory);

    assert_eq!(actual_regs, expected_regs);
    assert_eq!(actual_mem.last_helper_addr, 4);
    assert_eq!(exit_pc, 0);
}

#[test]
fn mixed_t16_t32_multiple_transfers_match_interpreter() {
    let program = [
        insn(&[0x0d, 0xc6], "stmia r6!,{r0,r2,r3}"),
        insn(&[0x07, 0xcf], "ldmia r7!,{r0-r2}"),
        insn(&[0x30, 0xb5], "push {r4,r5,lr}"),
        insn(&[0xbd, 0xe8, 0x30, 0x40], "pop.w {r4,r5,lr}"),
        insn(&[0x2d, 0xe9, 0x00, 0x4f], "push.w {r8-r11,lr}"),
        insn(&[0xbd, 0xe8, 0x00, 0x4f], "pop.w {r8-r11,lr}"),
        insn(&[0x2a, 0xe9, 0x05, 0x01], "stmdb r10!,{r0,r2,r8}"),
        insn(&[0xba, 0xe8, 0x05, 0x01], "ldmia r10!,{r0,r2,r8}"),
    ];
    let mut initial = initial_state();
    initial.r[6] = 0x20;
    initial.r[7] = 0x40;
    initial.r[10] = 0x70;
    initial.r[13] = 0xc0;
    let memory = TestMemory::patterned();
    let (expected_regs, expected_mem, fault) = reference_memory(&program, initial, memory.clone());
    assert_eq!(fault, None);
    for level in [OptLevel::O0, OptLevel::O2] {
        let (actual_regs, actual_mem, exit_pc) =
            run_memory_native_at(&program, initial, memory.clone(), level);
        assert_eq!(actual_regs, expected_regs, "{level:?}");
        assert_eq!(actual_mem.data, expected_mem.data, "{level:?}");
        assert_eq!(actual_mem.helper_stores, 14, "{level:?}");
        assert_eq!(actual_mem.helper_loads, 14, "{level:?}");
        assert_eq!(actual_regs.r[6], 0x2c, "T16 STMIA {level:?}");
        assert_eq!(actual_regs.r[7], 0x4c, "T16 LDMIA {level:?}");
        assert_eq!(actual_regs.r[10], 0x70, "T32 DB/IA {level:?}");
        assert_eq!(actual_regs.r[13], 0xc0, "T16/T32 stack {level:?}");
        assert_eq!(exit_pc, 0, "{level:?}");
    }
}

#[test]
fn thumb_multiple_fault_commit_order_and_writeback_match_interpreter() {
    for (program, base, fault_addr, label) in [
        (
            [insn(&[0x07, 0xc6], "stmia r6!,{r0-r2}")],
            0x20,
            0x24,
            "T16 STMIA",
        ),
        (
            [insn(&[0x07, 0xce], "ldmia r6!,{r0-r2}")],
            0x20,
            0x24,
            "T16 LDMIA",
        ),
    ] {
        let mut initial = initial_state();
        initial.r[6] = base;
        let mut memory = TestMemory::patterned();
        memory.fault_enabled = 1;
        memory.fault_addr = fault_addr;
        let (expected_regs, expected_mem, fault) =
            reference_memory(&program, initial, memory.clone());
        assert_eq!(fault, Some(0), "{label}");
        for level in [OptLevel::O0, OptLevel::O2] {
            let (actual_regs, actual_mem, exit_pc) =
                run_memory_native_at(&program, initial, memory.clone(), level);
            assert_eq!(actual_regs, expected_regs, "{label} {level:?}");
            assert_eq!(actual_mem.data, expected_mem.data, "{label} {level:?}");
            assert_eq!(
                actual_regs.r[6], initial.r[6],
                "{label} {level:?} writeback"
            );
            assert_eq!(exit_pc, 0x8000, "{label} {level:?} fault PC");
            if label.contains("STM") {
                assert_eq!(actual_mem.helper_stores, 2, "{label} {level:?}");
            } else {
                assert_eq!(actual_mem.helper_loads, 2, "{label} {level:?}");
                assert_ne!(
                    actual_regs.r[0], initial.r[0],
                    "{label} {level:?} first load committed"
                );
                assert_eq!(
                    actual_regs.r[1], initial.r[1],
                    "{label} {level:?} faulting load did not commit"
                );
            }
        }
    }
}

#[test]
fn thumb_multiple_transfer_addresses_wrap_modulo_2_pow_32() {
    let program = [insn(&[0x2a, 0xe9, 0x05, 0x00], "stmdb r10!,{r0,r2}")];
    let mut initial = initial_state();
    initial.r[10] = 4;
    let memory = TestMemory::patterned();
    let (expected_regs, expected_mem, fault) = reference_memory(&program, initial, memory.clone());
    assert_eq!(fault, None);
    for level in [OptLevel::O0, OptLevel::O2] {
        let (actual_regs, actual_mem, exit_pc) =
            run_memory_native_at(&program, initial, memory.clone(), level);
        assert_eq!(actual_regs, expected_regs, "{level:?}");
        assert_eq!(actual_mem.data, expected_mem.data, "{level:?}");
        assert_eq!(actual_regs.r[10], 0xffff_fffc, "{level:?}");
        assert_eq!(actual_mem.last_helper_addr, 0, "{level:?}");
        assert_eq!(actual_mem.helper_stores, 2, "{level:?}");
        assert_eq!(exit_pc, 0, "{level:?}");
    }
}

#[test]
fn t32_double_transfers_match_interpreter_at_o0_and_o2() {
    let program = [
        insn(&[0xd8, 0xe9, 0x02, 0x01], "ldrd r0,r1,[r8,#8]"),
        insn(&[0xe9, 0xe9, 0x02, 0x01], "strd r0,r1,[r9,#8]!"),
        insn(&[0xfa, 0xe8, 0x02, 0x23], "ldrd r2,r3,[r10],#8"),
        insn(&[0x4b, 0xe9, 0x02, 0x23], "strd r2,r3,[r11,#-8]"),
    ];
    let mut initial = initial_state();
    initial.r[8] = 0x20;
    initial.r[9] = 0x50;
    initial.r[10] = 0x80;
    initial.r[11] = 0xb0;
    let memory = TestMemory::patterned();
    let (expected_regs, expected_mem, fault) = reference_memory(&program, initial, memory.clone());
    assert_eq!(fault, None);

    for level in [OptLevel::O0, OptLevel::O2] {
        let (actual_regs, actual_mem, exit_pc) =
            run_memory_native_at(&program, initial, memory.clone(), level);
        assert_eq!(actual_regs, expected_regs, "{level:?}");
        assert_eq!(actual_mem.data, expected_mem.data, "{level:?}");
        assert_eq!(actual_mem.helper_loads, 4, "{level:?}");
        assert_eq!(actual_mem.helper_stores, 4, "{level:?}");
        assert_eq!(actual_regs.r[9], 0x58, "pre-index writeback {level:?}");
        assert_eq!(actual_regs.r[10], 0x88, "post-index writeback {level:?}");
        assert_eq!(exit_pc, 0, "{level:?}");
    }
}

#[test]
fn t32_double_transfer_second_fault_preserves_load_pair_and_writeback() {
    for (program, is_load, label) in [
        (
            [insn(&[0xf8, 0xe9, 0x02, 0x01], "ldrd r0,r1,[r8,#8]!")],
            true,
            "LDRD",
        ),
        (
            [insn(&[0xe8, 0xe9, 0x02, 0x01], "strd r0,r1,[r8,#8]!")],
            false,
            "STRD",
        ),
    ] {
        let mut initial = initial_state();
        initial.r[8] = 0x18;
        let mut memory = TestMemory::patterned();
        memory.fault_enabled = 1;
        memory.fault_addr = 0x24;
        let (expected_regs, expected_mem, fault) =
            reference_memory(&program, initial, memory.clone());
        assert_eq!(fault, Some(0), "{label}");

        for level in [OptLevel::O0, OptLevel::O2] {
            let (actual_regs, actual_mem, exit_pc) =
                run_memory_native_at(&program, initial, memory.clone(), level);
            assert_eq!(actual_regs, expected_regs, "{label} {level:?}");
            assert_eq!(actual_mem.data, expected_mem.data, "{label} {level:?}");
            assert_eq!(actual_regs.r[8], 0x18, "writeback {label} {level:?}");
            assert_eq!(exit_pc, 0x8000, "{label} {level:?}");
            if is_load {
                assert_eq!(actual_mem.helper_loads, 2, "{label} {level:?}");
                assert_eq!(actual_regs.r[0], initial.r[0], "dst1 {label} {level:?}");
                assert_eq!(actual_regs.r[1], initial.r[1], "dst2 {label} {level:?}");
            } else {
                assert_eq!(actual_mem.helper_stores, 2, "{label} {level:?}");
            }
        }
    }
}

#[test]
fn t32_double_transfer_second_address_wraps_modulo_2_pow_32() {
    let program = [insn(&[0xd8, 0xe9, 0x00, 0x01], "ldrd r0,r1,[r8]")];
    let mut initial = initial_state();
    initial.r[8] = 0xffff_fffc;
    let memory = TestMemory::patterned();
    let (expected_regs, _, fault) = reference_memory(&program, initial, memory.clone());
    assert_eq!(fault, None);
    for level in [OptLevel::O0, OptLevel::O2] {
        let (actual_regs, actual_mem, exit_pc) =
            run_memory_native_at(&program, initial, memory.clone(), level);
        assert_eq!(actual_regs, expected_regs, "{level:?}");
        assert_eq!(actual_mem.last_helper_addr, 0, "{level:?}");
        assert_eq!(actual_mem.helper_loads, 2, "{level:?}");
        assert_eq!(exit_pc, 0, "{level:?}");
    }
}

struct ThumbCode {
    base: u64,
    bytes: Vec<u8>,
}

impl ThumbCode {
    fn new(base: u64, bytes: &[u8]) -> Self {
        Self {
            base,
            bytes: bytes.to_vec(),
        }
    }
}

impl MemoryReader for ThumbCode {
    fn read(&self, addr: u64, size: usize) -> Result<Vec<u8>, SmirMemoryError> {
        let Some(offset) = addr
            .checked_sub(self.base)
            .and_then(|offset| usize::try_from(offset).ok())
        else {
            return Err(SmirMemoryError::OutOfBounds { addr });
        };
        let Some(end) = offset.checked_add(size) else {
            return Err(SmirMemoryError::OutOfBounds { addr });
        };
        self.bytes
            .get(offset..end)
            .map(<[u8]>::to_vec)
            .ok_or(SmirMemoryError::OutOfBounds { addr })
    }
}

fn thumb_native_cfg(bytes: &[u8], level: OptLevel) -> (ExecMem, usize) {
    const BASE: u64 = 0x8000;

    let memory = ThumbCode::new(BASE, bytes);
    let mut lifter = ThumbLifter::new();
    let mut context = LiftContext::new(SourceArch::Thumb);
    let entry_block = lifter
        .lift_block(BASE, &memory, &mut context)
        .expect("lift Thumb control-flow block");
    let entry = entry_block.id;
    let successors = entry_block.successors();
    let mut function = SmirFunction::new(FunctionId(0), entry, BASE);
    function.add_block(entry_block);
    let mut exits = HashMap::<BlockId, u64>::new();
    for successor in successors {
        if successor == entry || exits.contains_key(&successor) {
            continue;
        }
        let guest_pc = context
            .block_cache
            .iter()
            .find_map(|(&guest_pc, &id)| (id == successor).then_some(guest_pc))
            .expect("successor must have a guest address");
        let mut exit = SmirBlock::new(successor, guest_pc);
        exit.set_terminator(Terminator::Return { values: Vec::new() });
        function.add_block(exit);
        exits.insert(successor, guest_pc);
    }
    optimize_function(&mut function, level);
    assert!(
        is_aarch32_aarch64_native_clobber_safe_excluding_with_mem(&function, &exits, false),
        "Thumb control-flow region at {level:?} must satisfy the production native gate"
    );

    let mut lowerer = Aarch64Lowerer::new();
    lowerer.set_native_exits(exits);
    lowerer.set_guest_call_exits(true);
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower Thumb control-flow region");
    let code = lowerer
        .finalize()
        .expect("finalize Thumb control-flow region");
    (
        ExecMem::new(&code).expect("map Thumb control-flow region"),
        lowered.entry_offset,
    )
}

fn thumb_reference_branch_exit(bytes: &[u8], initial: Aarch32GuestRegs) -> u64 {
    let mut cpu = Armv7Cpu::new();
    cpu.regs = initial.r;
    cpu.cpsr = Psr::from_u32(initial.cpsr);
    let mut memory = FlatMemory::new(0x10_000, 0);
    let mut executor = Executor::new(&mut cpu, &mut memory);
    let decoded = Decoder::new(ExecutionState::Thumb)
        .decode(bytes)
        .expect("reference Thumb branch decode");
    match executor.execute(&decoded) {
        ExecResult::Continue => 0x8000 + u64::from(decoded.size),
        ExecResult::Branch(target) => u64::from(target),
        other => panic!("reference Thumb branch execution failed: {other:?}"),
    }
}

fn thumb_reference_call(bytes: &[u8], initial: Aarch32GuestRegs) -> (u64, Aarch32GuestRegs) {
    let mut cpu = Armv7Cpu::new();
    cpu.regs = initial.r;
    cpu.cpsr = Psr::from_u32(initial.cpsr);
    let mut memory = FlatMemory::new(0x10_000, 0);
    let mut executor = Executor::new(&mut cpu, &mut memory);
    let decoded = Decoder::new(ExecutionState::Thumb)
        .decode(bytes)
        .expect("reference Thumb call decode");
    let exit = match executor.execute(&decoded) {
        ExecResult::Branch(target) => u64::from(target),
        other => panic!("reference Thumb call execution failed: {other:?}"),
    };
    (
        exit,
        Aarch32GuestRegs {
            r: executor.cpu.regs,
            cpsr: executor.cpu.cpsr.to_u32(),
        },
    )
}

fn thumb_reference_loop(bytes: &[u8], initial: Aarch32GuestRegs, exit_pc: u64) -> Aarch32GuestRegs {
    const BASE: u64 = 0x8000;
    const MAX_STEPS: usize = 64;

    let mut cpu = Armv7Cpu::new();
    cpu.regs = initial.r;
    cpu.cpsr = Psr::from_u32(initial.cpsr);
    let mut memory = FlatMemory::new(0x10_000, 0);
    let mut executor = Executor::new(&mut cpu, &mut memory);
    let decoder = Decoder::new(ExecutionState::Thumb);
    let mut pc = BASE;
    for _ in 0..MAX_STEPS {
        if pc == exit_pc {
            executor.cpu.regs[15] = initial.r[15];
            return Aarch32GuestRegs {
                r: executor.cpu.regs,
                cpsr: executor.cpu.cpsr.to_u32(),
            };
        }
        let offset = usize::try_from(pc - BASE).expect("Thumb loop index");
        let encoded = bytes
            .get(offset..)
            .expect("Thumb loop PC must be in region");
        executor.cpu.regs[15] = pc as u32;
        let decoded = decoder
            .decode(encoded)
            .expect("reference Thumb loop decode");
        pc = match executor.execute(&decoded) {
            ExecResult::Continue => pc + u64::from(decoded.size),
            ExecResult::Branch(target) => u64::from(target),
            other => panic!("reference Thumb loop execution failed: {other:?}"),
        };
    }
    panic!("Thumb reference loop exceeded {MAX_STEPS} steps");
}

#[test]
fn t16_all_direct_branch_conditions_match_interpreter_for_all_nzcv_at_o0_and_o2() {
    const NZCV_MASK: u32 = 0xf000_0000;

    for condition in 0_u16..14 {
        let raw = 0xd001 | (condition << 8);
        let bytes = raw.to_le_bytes();
        for level in [OptLevel::O0, OptLevel::O2] {
            let (exec, entry) = thumb_native_cfg(&bytes, level);
            for nzcv in 0_u32..16 {
                let mut initial = initial_state();
                initial.cpsr = (initial.cpsr & !NZCV_MASK) | (nzcv << 28);
                let expected_exit = thumb_reference_branch_exit(&bytes, initial);
                let mut actual = initial;
                let actual_exit = exec.run_aarch32_identity_until_exit(entry, &mut actual);
                assert_eq!(
                    actual_exit, expected_exit,
                    "cond={condition:#x} NZCV={nzcv:#x} {level:?}"
                );
                assert_eq!(
                    actual, initial,
                    "cond={condition:#x} NZCV={nzcv:#x} {level:?}"
                );
            }
        }
    }
}

#[test]
fn t32_conditional_branch_matches_interpreter_for_all_nzcv_at_o0_and_o2() {
    const BNE_W_PLUS_4: [u8; 4] = [0x40, 0xf0, 0x02, 0x80];
    const NZCV_MASK: u32 = 0xf000_0000;

    for level in [OptLevel::O0, OptLevel::O2] {
        let (exec, entry) = thumb_native_cfg(&BNE_W_PLUS_4, level);
        for nzcv in 0_u32..16 {
            let mut initial = initial_state();
            initial.cpsr = (initial.cpsr & !NZCV_MASK) | (nzcv << 28);
            let expected_exit = thumb_reference_branch_exit(&BNE_W_PLUS_4, initial);
            let mut actual = initial;
            let actual_exit = exec.run_aarch32_identity_until_exit(entry, &mut actual);
            assert_eq!(actual_exit, expected_exit, "NZCV={nzcv:#x} {level:?}");
            assert_eq!(actual, initial, "NZCV={nzcv:#x} {level:?}");
        }
    }
}

#[test]
fn t16_cbz_cbnz_match_interpreter_for_all_low_regs_values_nzcv_at_o0_and_o2() {
    const NZCV_MASK: u32 = 0xf000_0000;

    for rn in 0_u16..8 {
        for base in [0xb100_u16, 0xb900_u16] {
            let bytes = (base | (1 << 3) | rn).to_le_bytes(); // target = 0x8006.
            for level in [OptLevel::O0, OptLevel::O2] {
                let (exec, entry) = thumb_native_cfg(&bytes, level);
                for value in [0_u32, 1, u32::MAX] {
                    for nzcv in 0_u32..16 {
                        let mut initial = initial_state();
                        initial.r[rn as usize] = value;
                        initial.cpsr = (initial.cpsr & !NZCV_MASK) | (nzcv << 28);
                        let expected_exit = thumb_reference_branch_exit(&bytes, initial);
                        let mut actual = initial;
                        let actual_exit = exec.run_aarch32_identity_until_exit(entry, &mut actual);
                        assert_eq!(
                            actual_exit,
                            expected_exit,
                            "raw={:#06x} r{rn}={value:#010x} NZCV={nzcv:#x} {level:?}",
                            u16::from_le_bytes(bytes),
                        );
                        assert_eq!(
                            actual,
                            initial,
                            "raw={:#06x} r{rn}={value:#010x} NZCV={nzcv:#x} {level:?}",
                            u16::from_le_bytes(bytes),
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn thumb_direct_bl_frontier_exit_matches_interpreter_for_all_nzcv_at_o0_and_o2() {
    const BL_PLUS_ZERO: [u8; 4] = [0x00, 0xf0, 0x00, 0xf8];
    const NZCV_MASK: u32 = 0xf000_0000;

    for level in [OptLevel::O0, OptLevel::O2] {
        let (exec, entry) = thumb_native_cfg(&BL_PLUS_ZERO, level);
        for nzcv in 0_u32..16 {
            let mut initial = initial_state();
            initial.cpsr = (initial.cpsr & !NZCV_MASK) | (nzcv << 28);
            let (expected_exit, expected) = thumb_reference_call(&BL_PLUS_ZERO, initial);
            let mut actual = initial;
            let actual_exit = exec.run_aarch32_identity_until_exit(entry, &mut actual);
            assert_eq!(actual_exit, expected_exit, "NZCV={nzcv:#x} {level:?}");
            assert_eq!(actual, expected, "NZCV={nzcv:#x} {level:?}");
        }
    }
}

#[test]
fn thumb_countdown_loop_and_frontier_exit_match_interpreter_at_o0_and_o2() {
    const PROGRAM: [u8; 4] = [
        0x40, 0x1e, // subs r0,r0,#1
        0xfd, 0xd1, // bne  0x8000
    ];
    const EXIT_PC: u64 = 0x8004;

    let mut initial = initial_state();
    initial.r[0] = 4;
    let expected = thumb_reference_loop(&PROGRAM, initial, EXIT_PC);
    assert_eq!(expected.r[0], 0);
    for level in [OptLevel::O0, OptLevel::O2] {
        let (exec, entry) = thumb_native_cfg(&PROGRAM, level);
        let mut actual = initial;
        let actual_exit = exec.run_aarch32_identity_until_exit(entry, &mut actual);
        assert_eq!(actual_exit, EXIT_PC, "{level:?}");
        assert_eq!(actual, expected, "{level:?}");
    }
}
