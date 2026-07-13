//! End-to-end RISC-V machine code -> SMIR -> x86-64 native execution tests.
#![cfg(all(feature = "smir-jit", target_arch = "x86_64"))]

use std::collections::HashMap;

use rax::isa::riscv::{FlatMemory as RvMemory, RiscVConfig, RiscVCpu, RiscVExit};
use rax::smir::RiscVLifter;
use rax::smir::ir::types::{BlockId, FunctionId, OpId, SourceArch};
use rax::smir::ir::{CallingConv, SmirBlock, SmirFunction, Terminator};
use rax::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use rax::smir::lower::SmirLowerer;
use rax::smir::lower::cross::riscv_guest_to_x86_64_host::RiscVX86_64Lowerer;
use rax::smir::lower::runtime::{
    ExecMem, RiscVAtomicCasResult, RiscVAtomicOpCode, RiscVGuestRegs, RiscVMemoryOrderCode,
};
use rax::smir::optimize::{OptLevel, optimize_function};

const CODE: u64 = 0x1000;
const DATA: u64 = 0x2000;
const MEMORY_LEN: usize = 0x4000;

#[repr(C)]
struct TestMemory {
    bytes: [u8; MEMORY_LEN],
    reservation_addr: u64,
    reservation_size: u64,
    reservation_valid: u64,
    last_atomic_order: u64,
}

impl TestMemory {
    fn new(bytes: [u8; MEMORY_LEN]) -> Self {
        Self {
            bytes,
            reservation_addr: 0,
            reservation_size: 0,
            reservation_valid: 0,
            last_atomic_order: u64::MAX,
        }
    }

    fn read_value(&self, addr: u64, size: u64) -> u64 {
        let addr = addr as usize;
        let size = size as usize;
        assert!(addr.checked_add(size).is_some_and(|end| end <= MEMORY_LEN));
        let mut value = 0u64;
        for index in 0..size {
            value |= u64::from(self.bytes[addr + index]) << (index * 8);
        }
        value
    }

    fn write_value(&mut self, addr: u64, value: u64, size: u64) {
        let host_addr = addr as usize;
        let host_size = size as usize;
        assert!(
            host_addr
                .checked_add(host_size)
                .is_some_and(|host_end| host_end <= MEMORY_LEN)
        );
        for index in 0..host_size {
            self.bytes[host_addr + index] = (value >> (index * 8)) as u8;
        }
    }
}

unsafe extern "sysv64" fn load(ctx: u64, addr: u64, size: u64, signed: u64) -> u64 {
    let memory = unsafe { &*(ctx as *const TestMemory) };
    let mut value = memory.read_value(addr, size);
    if signed != 0 && size < 8 {
        let bits = size as usize * 8;
        let sign_bit = 1u64 << (bits - 1);
        if value & sign_bit != 0 {
            value |= u64::MAX << bits;
        }
    }
    value
}

unsafe extern "sysv64" fn store(ctx: u64, addr: u64, value: u64, size: u64) -> u64 {
    let memory = unsafe { &mut *(ctx as *mut TestMemory) };
    memory.write_value(addr, value, size);
    1
}

unsafe extern "sysv64" fn atomic_rmw(
    ctx: u64,
    addr: u64,
    operand: u64,
    size: u64,
    op: u64,
    order: u64,
) -> u64 {
    let memory = unsafe { &mut *(ctx as *mut TestMemory) };
    assert!(order <= RiscVMemoryOrderCode::SeqCst as u64);
    memory.last_atomic_order = order;
    let bits = size * 8;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let old = memory.read_value(addr, size) & mask;
    let operand = operand & mask;
    let signed = |value: u64| -> i64 {
        if bits == 64 {
            value as i64
        } else {
            ((value << (64 - bits)) as i64) >> (64 - bits)
        }
    };
    let new = match op {
        value if value == RiscVAtomicOpCode::Add as u64 => old.wrapping_add(operand),
        value if value == RiscVAtomicOpCode::Sub as u64 => old.wrapping_sub(operand),
        value if value == RiscVAtomicOpCode::Neg as u64 => 0u64.wrapping_sub(old),
        value if value == RiscVAtomicOpCode::And as u64 => old & operand,
        value if value == RiscVAtomicOpCode::Or as u64 => old | operand,
        value if value == RiscVAtomicOpCode::Xor as u64 => old ^ operand,
        value if value == RiscVAtomicOpCode::Nand as u64 => !(old & operand),
        value if value == RiscVAtomicOpCode::Max as u64 => {
            std::cmp::max(signed(old), signed(operand)) as u64
        }
        value if value == RiscVAtomicOpCode::Min as u64 => {
            std::cmp::min(signed(old), signed(operand)) as u64
        }
        value if value == RiscVAtomicOpCode::Umax as u64 => std::cmp::max(old, operand),
        value if value == RiscVAtomicOpCode::Umin as u64 => std::cmp::min(old, operand),
        value if value == RiscVAtomicOpCode::Swap as u64 => operand,
        _ => panic!("invalid atomic RMW operation code {op}"),
    } & mask;
    memory.write_value(addr, new, size);
    old
}

unsafe extern "sysv64" fn compare_and_swap(
    ctx: u64,
    addr: u64,
    expected: u64,
    new_value: u64,
    size: u64,
    order: u64,
) -> RiscVAtomicCasResult {
    let memory = unsafe { &mut *(ctx as *mut TestMemory) };
    assert!(order <= RiscVMemoryOrderCode::SeqCst as u64);
    memory.last_atomic_order = order;
    let bits = size * 8;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let old = memory.read_value(addr, size) & mask;
    let success = old == expected & mask;
    if success {
        memory.write_value(addr, new_value & mask, size);
    }
    RiscVAtomicCasResult {
        old,
        success: u64::from(success),
    }
}

unsafe extern "sysv64" fn load_exclusive(ctx: u64, addr: u64, size: u64) -> u64 {
    let memory = unsafe { &mut *(ctx as *mut TestMemory) };
    let value = memory.read_value(addr, size);
    memory.reservation_addr = addr;
    memory.reservation_size = size;
    memory.reservation_valid = 1;
    value
}

unsafe extern "sysv64" fn store_exclusive(ctx: u64, addr: u64, value: u64, size: u64) -> u64 {
    let memory = unsafe { &mut *(ctx as *mut TestMemory) };
    let success = memory.reservation_valid != 0
        && memory.reservation_addr == addr
        && memory.reservation_size == size;
    if success {
        memory.write_value(addr, value, size);
    }
    memory.reservation_valid = 0;
    u64::from(success)
}

unsafe extern "sysv64" fn clear_exclusive(ctx: u64) {
    let memory = unsafe { &mut *(ctx as *mut TestMemory) };
    memory.reservation_valid = 0;
}

fn jit_state(
    memory: &mut TestMemory,
    x: [u64; 32],
    f: [u64; 32],
    fcsr: u32,
    pc: u64,
) -> RiscVGuestRegs {
    RiscVGuestRegs {
        x,
        f,
        fcsr: u64::from(fcsr),
        pc,
        ctx: (memory as *mut TestMemory) as u64,
        load_fn: load as *const () as usize as u64,
        store_fn: store as *const () as usize as u64,
        atomic_rmw_fn: atomic_rmw as *const () as usize as u64,
        cas_fn: compare_and_swap as *const () as usize as u64,
        load_exclusive_fn: load_exclusive as *const () as usize as u64,
        store_exclusive_fn: store_exclusive as *const () as usize as u64,
        clear_exclusive_fn: clear_exclusive as *const () as usize as u64,
        ..Default::default()
    }
}

fn r_type(funct7: u32, rs2: u8, rs1: u8, funct3: u32, rd: u8) -> u32 {
    r_type_opcode(funct7, rs2, rs1, funct3, rd, 0x33)
}

fn r_type_opcode(funct7: u32, rs2: u8, rs1: u8, funct3: u32, rd: u8, opcode: u32) -> u32 {
    (funct7 << 25)
        | (u32::from(rs2) << 20)
        | (u32::from(rs1) << 15)
        | (funct3 << 12)
        | (u32::from(rd) << 7)
        | opcode
}

fn i_type(imm: i32, rs1: u8, funct3: u32, rd: u8, opcode: u32) -> u32 {
    ((imm as u32 & 0xfff) << 20)
        | (u32::from(rs1) << 15)
        | (funct3 << 12)
        | (u32::from(rd) << 7)
        | opcode
}

fn s_type(imm: i32, rs2: u8, rs1: u8, funct3: u32) -> u32 {
    s_type_opcode(imm, rs2, rs1, funct3, 0x23)
}

fn s_type_opcode(imm: i32, rs2: u8, rs1: u8, funct3: u32, opcode: u32) -> u32 {
    let imm = imm as u32 & 0xfff;
    ((imm >> 5) << 25)
        | (u32::from(rs2) << 20)
        | (u32::from(rs1) << 15)
        | (funct3 << 12)
        | ((imm & 0x1f) << 7)
        | opcode
}

fn b_type(offset: i32, rs2: u8, rs1: u8, funct3: u32) -> u32 {
    let imm = offset as u32 & 0x1fff;
    (((imm >> 12) & 1) << 31)
        | (((imm >> 5) & 0x3f) << 25)
        | (u32::from(rs2) << 20)
        | (u32::from(rs1) << 15)
        | (funct3 << 12)
        | (((imm >> 1) & 0xf) << 8)
        | (((imm >> 11) & 1) << 7)
        | 0x63
}

fn j_type(offset: i32, rd: u8) -> u32 {
    let imm = offset as u32 & 0x1f_ffff;
    (((imm >> 20) & 1) << 31)
        | (((imm >> 1) & 0x3ff) << 21)
        | (((imm >> 11) & 1) << 20)
        | (((imm >> 12) & 0xff) << 12)
        | (u32::from(rd) << 7)
        | 0x6f
}

fn amo_type(funct5: u32, aq: bool, rl: bool, rs2: u8, rs1: u8, funct3: u32, rd: u8) -> u32 {
    (funct5 << 27)
        | (u32::from(aq) << 26)
        | (u32::from(rl) << 25)
        | (u32::from(rs2) << 20)
        | (u32::from(rs1) << 15)
        | (funct3 << 12)
        | (u32::from(rd) << 7)
        | 0x2f
}

fn function_for_lift(
    control: ControlFlow,
    ops: Vec<rax::smir::ir::ops::SmirOp>,
    instruction_len: usize,
) -> (SmirFunction, HashMap<BlockId, u64>) {
    function_for_lift_at(CODE, control, ops, instruction_len)
}

fn function_for_lift_at(
    pc: u64,
    control: ControlFlow,
    mut ops: Vec<rax::smir::ir::ops::SmirOp>,
    instruction_len: usize,
) -> (SmirFunction, HashMap<BlockId, u64>) {
    for (index, op) in ops.iter_mut().enumerate() {
        op.id = OpId(index as u16);
    }
    let entry = BlockId(0);
    let mut return_pcs = HashMap::new();
    let mut blocks = Vec::new();
    let terminator = match control {
        ControlFlow::Fallthrough | ControlFlow::NextInsn => {
            return_pcs.insert(entry, pc + instruction_len as u64);
            Terminator::Return { values: vec![] }
        }
        ControlFlow::Branch { target } | ControlFlow::DirectBranch(target) => {
            return_pcs.insert(entry, target);
            Terminator::Return { values: vec![] }
        }
        ControlFlow::CondBranchReg {
            cond,
            taken,
            not_taken,
        } => {
            let taken_id = BlockId(1);
            let not_taken_id = BlockId(2);
            return_pcs.insert(taken_id, taken);
            return_pcs.insert(not_taken_id, not_taken);
            blocks.push(SmirBlock {
                id: taken_id,
                guest_pc: taken,
                phis: vec![],
                ops: vec![],
                terminator: Terminator::Return { values: vec![] },
                exec_count: 0,
            });
            blocks.push(SmirBlock {
                id: not_taken_id,
                guest_pc: not_taken,
                phis: vec![],
                ops: vec![],
                terminator: Terminator::Return { values: vec![] },
                exec_count: 0,
            });
            Terminator::CondBranch {
                cond,
                true_target: taken_id,
                false_target: not_taken_id,
            }
        }
        ControlFlow::IndirectBranch { target } => Terminator::IndirectBranch {
            target,
            possible_targets: vec![],
        },
        other => panic!("control flow is outside this scalar JIT test: {other:?}"),
    };
    blocks.insert(
        0,
        SmirBlock {
            id: entry,
            guest_pc: pc,
            phis: vec![],
            ops,
            terminator,
            exec_count: 0,
        },
    );

    let mut function = SmirFunction::new(FunctionId(0), entry, pc);
    function.blocks = blocks;
    function.guest_range = (pc, pc + instruction_len as u64);
    function.calling_convention = CallingConv::RiscVStd;
    (function, return_pcs)
}

fn run_case(bytes: &[u8], initial_x: [u64; 32], initial_memory: [u8; MEMORY_LEN]) {
    run_case_with_fp(bytes, initial_x, [0; 32], 0, initial_memory);
}

fn run_case_with_fp(
    bytes: &[u8],
    initial_x: [u64; 32],
    initial_f: [u64; 32],
    initial_fcsr: u32,
    initial_memory: [u8; MEMORY_LEN],
) {
    let mut reference_memory = RvMemory::new(0, MEMORY_LEN);
    rax::isa::riscv::Memory::write(&mut reference_memory, 0, &initial_memory)
        .expect("seed reference memory");
    let mut cpu = RiscVCpu::new(RiscVConfig::rv64gc(), Box::new(reference_memory));
    for register in 1..32u8 {
        cpu.set_x(register, initial_x[register as usize]);
    }
    for register in 0..32u8 {
        cpu.set_f(register, initial_f[register as usize]);
    }
    cpu.set_fcsr(initial_fcsr);
    cpu.write_memory(CODE, bytes).expect("write reference code");
    cpu.set_pc(CODE);
    assert_eq!(
        cpu.step(),
        RiscVExit::Continue,
        "reference instruction trapped"
    );

    let expected_x: [u64; 32] = std::array::from_fn(|index| cpu.x(index as u8));
    let expected_f: [u64; 32] = std::array::from_fn(|index| cpu.f(index as u8));
    let expected_fcsr = cpu.fcsr();
    let expected_pc = cpu.pc();
    let mut expected_data = [0u8; 64];
    cpu.read_memory(DATA, &mut expected_data)
        .expect("read reference data");

    let mut lifter = RiscVLifter::rv64gc();
    let mut context = LiftContext::new(SourceArch::RiscV64);
    let lifted = lifter
        .lift_insn(CODE, bytes, &mut context)
        .expect("lift RISC-V instruction");
    let (function, return_pcs) =
        function_for_lift(lifted.control_flow, lifted.ops, lifted.bytes_consumed);
    for level in [OptLevel::O0, OptLevel::O2] {
        let mut optimized = function.clone();
        optimize_function(&mut optimized, level);
        let mut lowerer = RiscVX86_64Lowerer::new();
        lowerer.set_return_pcs(return_pcs.clone());
        let lowered = lowerer
            .lower_function(&optimized)
            .unwrap_or_else(|error| panic!("lower RISC-V SMIR at {level:?}: {error:?}"));
        let code = lowerer.finalize().expect("finalize native code");
        let executable = ExecMem::new(&code).expect("map native code");

        let mut test_memory = TestMemory::new(initial_memory);
        let mut state = jit_state(&mut test_memory, initial_x, initial_f, initial_fcsr, CODE);
        executable.run_riscv(lowered.entry_offset, &mut state);

        assert_eq!(
            state.x, expected_x,
            "integer-register divergence at {level:?} for {bytes:02x?}"
        );
        assert_eq!(
            state.f, expected_f,
            "floating-register divergence at {level:?} for {bytes:02x?}"
        );
        assert_eq!(
            state.fcsr,
            u64::from(expected_fcsr),
            "FCSR divergence at {level:?} for {bytes:02x?}"
        );
        assert_eq!(
            state.pc, expected_pc,
            "PC divergence at {level:?} for {bytes:02x?}"
        );
        assert_eq!(
            state.exit_reason, 0,
            "unexpected JIT exit at {level:?} for {bytes:02x?}"
        );
        assert_eq!(
            &test_memory.bytes[DATA as usize..DATA as usize + 64],
            &expected_data,
            "memory divergence at {level:?} for {bytes:02x?}"
        );
    }
}

fn run_atomic_sequence(
    instructions: &[u32],
    initial_x: [u64; 32],
    initial_memory: [u8; MEMORY_LEN],
    expected_order: Option<RiscVMemoryOrderCode>,
) {
    let mut reference_memory = RvMemory::new(0, MEMORY_LEN);
    rax::isa::riscv::Memory::write(&mut reference_memory, 0, &initial_memory)
        .expect("seed reference memory");
    let mut cpu = RiscVCpu::new(RiscVConfig::rv64gc(), Box::new(reference_memory));
    for register in 1..32u8 {
        cpu.set_x(register, initial_x[register as usize]);
    }
    let code = instructions
        .iter()
        .flat_map(|instruction| instruction.to_le_bytes())
        .collect::<Vec<_>>();
    cpu.write_memory(CODE, &code).expect("write reference code");
    cpu.set_pc(CODE);
    for instruction in instructions {
        assert_eq!(
            cpu.step(),
            RiscVExit::Continue,
            "reference atomic instruction {instruction:08x} trapped"
        );
    }

    let expected_x: [u64; 32] = std::array::from_fn(|index| cpu.x(index as u8));
    let expected_pc = cpu.pc();
    let mut expected_data = [0u8; 64];
    cpu.read_memory(DATA, &mut expected_data)
        .expect("read reference atomic data");

    for level in [OptLevel::O0, OptLevel::O2] {
        let mut test_memory = TestMemory::new(initial_memory);
        let mut state = jit_state(&mut test_memory, initial_x, [0; 32], 0, CODE);
        let mut lifter = RiscVLifter::rv64gc();

        for (index, instruction) in instructions.iter().enumerate() {
            let pc = CODE + index as u64 * 4;
            assert_eq!(state.pc, pc, "unexpected dispatcher PC before atomic lift");
            let mut context = LiftContext::new(SourceArch::RiscV64);
            let lifted = lifter
                .lift_insn(pc, &instruction.to_le_bytes(), &mut context)
                .unwrap_or_else(|error| panic!("lift atomic {instruction:08x}: {error:?}"));
            let (mut function, return_pcs) =
                function_for_lift_at(pc, lifted.control_flow, lifted.ops, lifted.bytes_consumed);
            optimize_function(&mut function, level);
            let mut lowerer = RiscVX86_64Lowerer::new();
            lowerer.set_return_pcs(return_pcs);
            let lowered = lowerer.lower_function(&function).unwrap_or_else(|error| {
                panic!("lower atomic {instruction:08x} at {level:?}: {error:?}")
            });
            let code = lowerer.finalize().expect("finalize atomic native code");
            let executable = ExecMem::new(&code).expect("map atomic native code");
            executable.run_riscv(lowered.entry_offset, &mut state);
        }

        assert_eq!(
            state.x, expected_x,
            "atomic register divergence at {level:?} for {instructions:08x?}"
        );
        assert_eq!(
            state.pc, expected_pc,
            "atomic PC divergence at {level:?} for {instructions:08x?}"
        );
        assert_eq!(state.exit_reason, 0, "unexpected atomic JIT exit");
        assert_eq!(
            &test_memory.bytes[DATA as usize..DATA as usize + 64],
            &expected_data,
            "atomic memory divergence at {level:?} for {instructions:08x?}"
        );
        if let Some(order) = expected_order {
            assert_eq!(
                test_memory.last_atomic_order, order as u64,
                "atomic ordering code divergence at {level:?}"
            );
        }
        assert_eq!(
            test_memory.reservation_valid, 0,
            "reservation leaked after atomic sequence at {level:?}"
        );
    }
}

fn fp_type(funct7: u32, rs2: u8, rs1: u8, funct3: u32, rd: u8) -> u32 {
    (funct7 << 25)
        | (u32::from(rs2) << 20)
        | (u32::from(rs1) << 15)
        | (funct3 << 12)
        | (u32::from(rd) << 7)
        | 0x53
}

#[test]
fn lifted_rv64_integer_alu_and_m_extension_execute_natively() {
    let mut x = [0u64; 32];
    x[1] = 0x8123_4567_89ab_cdef;
    x[2] = 0xfedc_ba98_7654_3211;
    x[3] = 17;
    let memory = [0u8; MEMORY_LEN];

    let instructions = [
        i_type(-37, 1, 0, 5, 0x13),
        r_type(0x00, 2, 1, 0, 6),
        r_type(0x20, 2, 1, 0, 7),
        r_type(0x00, 3, 1, 1, 8),
        r_type(0x00, 3, 1, 5, 9),
        r_type(0x20, 3, 1, 5, 9),
        r_type(0x00, 2, 1, 2, 10),
        r_type(0x00, 2, 1, 3, 11),
        r_type(0x00, 2, 1, 4, 16),
        r_type(0x00, 2, 1, 6, 17),
        r_type(0x00, 2, 1, 7, 18),
        r_type(0x01, 2, 1, 0, 12),
        r_type(0x01, 2, 1, 1, 13),
        r_type(0x01, 2, 1, 2, 19),
        r_type(0x01, 2, 1, 3, 20),
        r_type(0x01, 3, 1, 4, 14),
        r_type(0x01, 3, 1, 5, 21),
        r_type(0x01, 3, 1, 6, 15),
        r_type(0x01, 3, 1, 7, 22),
        r_type(0x30, 3, 1, 1, 23),     // rol
        r_type(0x30, 3, 1, 5, 24),     // ror
        i_type(0x600, 1, 1, 25, 0x13), // clz
        i_type(0x601, 1, 1, 26, 0x13), // ctz
        i_type(0x602, 1, 1, 27, 0x13), // cpop
        r_type_opcode(0x00, 2, 1, 0, 28, 0x3b),
        r_type_opcode(0x01, 3, 1, 4, 29, 0x3b),
    ];
    for instruction in instructions {
        run_case(&instruction.to_le_bytes(), x, memory);
    }

    for value in [0, u64::MAX, 0x0102_0408_1020_4080] {
        let mut counts = x;
        counts[0] = 0xfeed_face_dead_beef;
        counts[1] = value;
        for instruction in [
            i_type(0x600, 1, 1, 25, 0x13),          // clz
            i_type(0x601, 1, 1, 26, 0x13),          // ctz
            i_type(0x602, 1, 1, 27, 0x13),          // cpop
            i_type(0x602, 1, 1, 27, 0x1b),          // cpopw
            r_type_opcode(0x34, 7, 1, 5, 28, 0x13), // brev8
        ] {
            run_case(&instruction.to_le_bytes(), counts, memory);
        }
    }

    // Division edge cases exercise the lifter's RISC-V totalization sequences
    // and prove the native lowerer never exposes host #DE.
    let mut edge = x;
    edge[1] = i64::MIN as u64;
    edge[3] = u64::MAX;
    for instruction in [
        r_type(0x01, 0, 1, 4, 5),
        r_type(0x01, 0, 1, 6, 6),
        r_type(0x01, 3, 1, 4, 7),
        r_type(0x01, 3, 1, 6, 8),
    ] {
        run_case(&instruction.to_le_bytes(), edge, memory);
    }
}

#[test]
fn lifted_rv64_load_store_and_control_flow_execute_natively() {
    let mut x = [0u64; 32];
    x[1] = DATA;
    x[2] = 0x8877_6655_4433_2211;
    x[3] = x[2];
    let mut memory = [0u8; MEMORY_LEN];
    memory[DATA as usize..DATA as usize + 8]
        .copy_from_slice(&0x8000_0001_7654_3210u64.to_le_bytes());

    for instruction in [
        i_type(0, 1, 0, 5, 0x03),
        i_type(0, 1, 1, 5, 0x03),
        i_type(0, 1, 2, 5, 0x03),
        i_type(0, 1, 3, 5, 0x03),
        i_type(0, 1, 4, 5, 0x03),
        i_type(0, 1, 5, 5, 0x03),
        i_type(0, 1, 6, 5, 0x03),
        s_type(16, 2, 1, 0),
        s_type(16, 2, 1, 1),
        s_type(16, 2, 1, 2),
        s_type(16, 2, 1, 3),
        b_type(8, 3, 2, 0),
        b_type(8, 1, 2, 0),
        b_type(8, 1, 2, 1),
        b_type(8, 3, 2, 1),
        j_type(12, 5),
        i_type(4, 2, 0, 5, 0x67),
    ] {
        run_case(&instruction.to_le_bytes(), x, memory);
    }
}

#[test]
fn compressed_fallthrough_uses_exact_two_byte_resume_pc() {
    // c.addi x5,1: quadrant 1, funct3=000, rd=x5, imm=1.
    let instruction = ((5u16) << 7) | (1 << 2) | 0b01;
    let mut x = [0u64; 32];
    x[5] = u64::MAX;
    run_case(&instruction.to_le_bytes(), x, [0u8; MEMORY_LEN]);
}

#[test]
fn lifted_fp_bit_operations_and_fcsr_access_execute_natively() {
    let mut x = [0u64; 32];
    x[1] = 0x0123_4567_89ab_cdef;
    let mut f = [0u64; 32];
    f[1] = 0x8000_0000_0000_0001;
    f[2] = 0x7ff8_1234_5678_9abc;
    let memory = [0u8; MEMORY_LEN];

    for instruction in [
        fp_type(0x71, 0, 1, 0, 5),    // fmv.x.d x5,f1
        fp_type(0x79, 0, 1, 0, 5),    // fmv.d.x f5,x1
        fp_type(0x11, 2, 1, 0, 5),    // fsgnj.d f5,f1,f2
        fp_type(0x11, 2, 1, 1, 5),    // fsgnjn.d f5,f1,f2
        fp_type(0x11, 2, 1, 2, 5),    // fsgnjx.d f5,f1,f2
        fp_type(0x71, 0, 1, 1, 5),    // fclass.d x5,f1
        fp_type(0x10, 2, 1, 0, 5),    // fsgnj.s f5,f1,f2
        fp_type(0x70, 0, 1, 1, 5),    // fclass.s x5,f1
        i_type(0x003, 1, 1, 5, 0x73), // csrrw x5,fcsr,x1
    ] {
        run_case_with_fp(&instruction.to_le_bytes(), x, f, 0x61, memory);
    }

    let mut memory_x = x;
    memory_x[1] = DATA;
    let mut fp_memory = memory;
    fp_memory[DATA as usize..DATA as usize + 8]
        .copy_from_slice(&0x8000_0001_7654_3210u64.to_le_bytes());
    for instruction in [
        i_type(0, 1, 2, 5, 0x07),         // flw f5,0(x1)
        i_type(0, 1, 3, 5, 0x07),         // fld f5,0(x1)
        s_type_opcode(16, 2, 1, 2, 0x27), // fsw f2,16(x1)
        s_type_opcode(16, 2, 1, 3, 0x27), // fsd f2,16(x1)
    ] {
        run_case_with_fp(&instruction.to_le_bytes(), memory_x, f, 0x61, fp_memory);
    }
}

#[test]
fn lifted_rv64_atomics_execute_through_helper_abi() {
    let mut x = [0u64; 32];
    x[1] = DATA;
    x[2] = 0x7000_0003_7fff_fffd;
    x[3] = 0x1122_3344_5566_7788;
    x[4] = DATA + 8;
    let mut memory = [0u8; MEMORY_LEN];
    memory[DATA as usize..DATA as usize + 8]
        .copy_from_slice(&0x8000_0005_8000_0005u64.to_le_bytes());
    memory[DATA as usize + 8..DATA as usize + 16]
        .copy_from_slice(&0x0102_0304_0506_0708u64.to_le_bytes());

    let operations = [
        0b00001, // amoswap
        0b00000, // amoadd
        0b00100, // amoxor
        0b01100, // amoand
        0b01000, // amoor
        0b10000, // amomin
        0b10100, // amomax
        0b11000, // amominu
        0b11100, // amomaxu
    ];
    let orders = [
        (false, false, RiscVMemoryOrderCode::Relaxed),
        (true, false, RiscVMemoryOrderCode::Acquire),
        (false, true, RiscVMemoryOrderCode::Release),
        (true, true, RiscVMemoryOrderCode::AcqRel),
    ];
    for (index, funct5) in operations.into_iter().enumerate() {
        let (aq, rl, order) = orders[index % orders.len()];
        for funct3 in [0b010, 0b011] {
            let instruction = amo_type(funct5, aq, rl, 2, 1, funct3, 5);
            run_atomic_sequence(&[instruction], x, memory, Some(order));
        }
    }

    // AMOCAS.D success and AMOCAS.W failure exercise both SysV return
    // registers (`old`, `success`) and word-result sign extension.
    let mut cas_success = x;
    cas_success[5] = 0x8000_0005_8000_0005;
    run_atomic_sequence(
        &[amo_type(0b00101, true, true, 2, 1, 0b011, 5)],
        cas_success,
        memory,
        Some(RiscVMemoryOrderCode::AcqRel),
    );
    let mut cas_failure = x;
    cas_failure[5] = 0x1234;
    run_atomic_sequence(
        &[amo_type(0b00101, true, false, 2, 1, 0b010, 5)],
        cas_failure,
        memory,
        Some(RiscVMemoryOrderCode::Acquire),
    );

    // LR/SC success for both widths, missing/different reservations, and the
    // reference CPU's same-hart ordinary-store reservation behavior.
    for funct3 in [0b010, 0b011] {
        run_atomic_sequence(
            &[
                amo_type(0b00010, true, false, 0, 1, funct3, 5),
                amo_type(0b00011, false, true, 2, 1, funct3, 6),
            ],
            x,
            memory,
            None,
        );
    }
    run_atomic_sequence(
        &[amo_type(0b00011, false, false, 2, 1, 0b011, 6)],
        x,
        memory,
        None,
    );
    run_atomic_sequence(
        &[
            amo_type(0b00010, false, false, 0, 1, 0b011, 5),
            s_type(0, 3, 1, 0b011),
            amo_type(0b00011, false, false, 2, 1, 0b011, 6),
        ],
        x,
        memory,
        None,
    );
    run_atomic_sequence(
        &[
            amo_type(0b00010, false, false, 0, 1, 0b011, 5),
            amo_type(0b00011, false, false, 2, 4, 0b011, 6),
        ],
        x,
        memory,
        None,
    );
    run_atomic_sequence(
        &[
            amo_type(0b00010, false, false, 0, 1, 0b011, 5),
            amo_type(0b00010, false, false, 0, 4, 0b011, 7),
            amo_type(0b00011, false, false, 2, 1, 0b011, 6),
        ],
        x,
        memory,
        None,
    );
}

#[test]
fn runtime_layout_matches_codegen_offsets() {
    assert_eq!(RiscVGuestRegs::X_OFFSET, 0);
    assert_eq!(RiscVGuestRegs::F_OFFSET, 32 * 8);
    assert_eq!(RiscVGuestRegs::PC_OFFSET, 64 * 8);
    assert_eq!(RiscVGuestRegs::FCSR_OFFSET, 65 * 8);
    assert_eq!(RiscVGuestRegs::EXIT_REASON_OFFSET, 66 * 8);
    assert_eq!(RiscVGuestRegs::CTX_OFFSET, 67 * 8);
    assert_eq!(RiscVGuestRegs::LOAD_FN_OFFSET, 68 * 8);
    assert_eq!(RiscVGuestRegs::STORE_FN_OFFSET, 69 * 8);
    assert_eq!(RiscVGuestRegs::ATOMIC_RMW_FN_OFFSET, 70 * 8);
    assert_eq!(RiscVGuestRegs::CAS_FN_OFFSET, 71 * 8);
    assert_eq!(RiscVGuestRegs::LOAD_EXCLUSIVE_FN_OFFSET, 72 * 8);
    assert_eq!(RiscVGuestRegs::STORE_EXCLUSIVE_FN_OFFSET, 73 * 8);
    assert_eq!(RiscVGuestRegs::CLEAR_EXCLUSIVE_FN_OFFSET, 74 * 8);
    assert_eq!(std::mem::size_of::<RiscVGuestRegs>(), 75 * 8);
}
