//! End-to-end RISC-V machine code -> SMIR -> x86-64 native execution tests.
#![cfg(all(feature = "smir-jit", target_arch = "x86_64"))]

use std::collections::HashMap;

use rax::isa::riscv::{
    FlatMemory as RvMemory, Isa as RvIsa, Op as RvOp, RiscVConfig, RiscVCpu, RiscVExit,
};
use rax::smir::ir::types::{ArchReg, BlockId, FunctionId, OpId, RiscVReg, SourceArch, VReg};
use rax::smir::ir::{CallingConv, SmirBlock, SmirFunction, Terminator};
use rax::smir::lift::riscv::RiscVExtensions;
use rax::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use rax::smir::lower::SmirLowerer;
use rax::smir::lower::cross::riscv_guest_to_x86_64_host::RiscVX86_64Lowerer;
use rax::smir::lower::runtime::{
    ExecMem, RISCV_FP_RESULT_INVALID, RiscVAtomicCasResult, RiscVAtomicOpCode, RiscVFpOpCode,
    RiscVFpResult, RiscVGuestRegs, RiscVIntCryptoOpCode, RiscVMemoryOrderCode,
};
use rax::smir::optimize::{OptLevel, optimize_function};
use rax::smir::{OpKind, RiscVLifter, SmirOp};

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

unsafe extern "sysv64" fn int_crypto(
    op_code: u64,
    src1: u64,
    src2: u64,
    imm: u64,
    xlen: u64,
) -> u64 {
    let op = match op_code {
        value if value == RiscVIntCryptoOpCode::Clmul as u64 => RvOp::Clmul,
        value if value == RiscVIntCryptoOpCode::Clmulh as u64 => RvOp::Clmulh,
        value if value == RiscVIntCryptoOpCode::Clmulr as u64 => RvOp::Clmulr,
        value if value == RiscVIntCryptoOpCode::Xperm4 as u64 => RvOp::Xperm4,
        value if value == RiscVIntCryptoOpCode::Xperm8 as u64 => RvOp::Xperm8,
        value if value == RiscVIntCryptoOpCode::Sha512Sig0l as u64 => RvOp::Sha512Sig0l,
        value if value == RiscVIntCryptoOpCode::Sha512Sig0h as u64 => RvOp::Sha512Sig0h,
        value if value == RiscVIntCryptoOpCode::Sha512Sig1l as u64 => RvOp::Sha512Sig1l,
        value if value == RiscVIntCryptoOpCode::Sha512Sig1h as u64 => RvOp::Sha512Sig1h,
        value if value == RiscVIntCryptoOpCode::Sha512Sum0r as u64 => RvOp::Sha512Sum0r,
        value if value == RiscVIntCryptoOpCode::Sha512Sum1r as u64 => RvOp::Sha512Sum1r,
        value if value == RiscVIntCryptoOpCode::Sm4ed as u64 => RvOp::Sm4ed,
        value if value == RiscVIntCryptoOpCode::Sm4ks as u64 => RvOp::Sm4ks,
        value if value == RiscVIntCryptoOpCode::Aes32esi as u64 => RvOp::Aes32esi,
        value if value == RiscVIntCryptoOpCode::Aes32esmi as u64 => RvOp::Aes32esmi,
        value if value == RiscVIntCryptoOpCode::Aes32dsi as u64 => RvOp::Aes32dsi,
        value if value == RiscVIntCryptoOpCode::Aes32dsmi as u64 => RvOp::Aes32dsmi,
        value if value == RiscVIntCryptoOpCode::Aes64es as u64 => RvOp::Aes64es,
        value if value == RiscVIntCryptoOpCode::Aes64esm as u64 => RvOp::Aes64esm,
        value if value == RiscVIntCryptoOpCode::Aes64ds as u64 => RvOp::Aes64ds,
        value if value == RiscVIntCryptoOpCode::Aes64dsm as u64 => RvOp::Aes64dsm,
        value if value == RiscVIntCryptoOpCode::Aes64im as u64 => RvOp::Aes64im,
        value if value == RiscVIntCryptoOpCode::Aes64ks1i as u64 => RvOp::Aes64ks1i,
        value if value == RiscVIntCryptoOpCode::Aes64ks2 as u64 => RvOp::Aes64ks2,
        _ => panic!("invalid integer-crypto operation code {op_code}"),
    };
    assert!(matches!(xlen, 32 | 64));
    rax::isa::riscv::crypto::eval_int_crypto(op, src1, src2, imm as u8, xlen as u32)
        .expect("helper operation must be in eval_int_crypto")
}

unsafe extern "sysv64" fn scalar_fp(
    op_code: u64,
    rm_field: u64,
    fcsr: u64,
    a: u64,
    b: u64,
    c: u64,
) -> RiscVFpResult {
    let Some(op_code) = RiscVFpOpCode::from_code(op_code) else {
        return RiscVFpResult {
            value: 0,
            fcsr_status: RISCV_FP_RESULT_INVALID,
        };
    };
    let Some((value, new_fcsr)) = rax::isa::riscv::float::eval_scalar_fp(
        op_code.into_op(),
        rm_field as u8,
        fcsr as u32,
        a,
        b,
        c,
    ) else {
        return RiscVFpResult {
            value: 0,
            fcsr_status: RISCV_FP_RESULT_INVALID,
        };
    };
    RiscVFpResult {
        value,
        fcsr_status: u64::from(new_fcsr),
    }
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
        int_crypto_fn: int_crypto as *const () as usize as u64,
        fp_fn: scalar_fp as *const () as usize as u64,
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

fn r4_type(fmt: u32, rs3: u8, rs2: u8, rs1: u8, rm: u32, rd: u8, opcode: u32) -> u32 {
    (u32::from(rs3) << 27)
        | (fmt << 25)
        | (u32::from(rs2) << 20)
        | (u32::from(rs1) << 15)
        | (rm << 12)
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

fn run_case_rv32(bytes: &[u8], initial_x: [u64; 32], initial_memory: [u8; MEMORY_LEN]) {
    run_case_for_xlen(bytes, initial_x, [0; 32], 0, initial_memory, true);
}

fn run_case_rv32_with_fp(
    bytes: &[u8],
    initial_x: [u64; 32],
    initial_f: [u64; 32],
    initial_fcsr: u32,
    initial_memory: [u8; MEMORY_LEN],
) {
    run_case_for_xlen(
        bytes,
        initial_x,
        initial_f,
        initial_fcsr,
        initial_memory,
        true,
    );
}

fn run_case_with_fp(
    bytes: &[u8],
    initial_x: [u64; 32],
    initial_f: [u64; 32],
    initial_fcsr: u32,
    initial_memory: [u8; MEMORY_LEN],
) {
    run_case_for_xlen(
        bytes,
        initial_x,
        initial_f,
        initial_fcsr,
        initial_memory,
        false,
    );
}

fn run_case_for_xlen(
    bytes: &[u8],
    initial_x: [u64; 32],
    initial_f: [u64; 32],
    initial_fcsr: u32,
    initial_memory: [u8; MEMORY_LEN],
    rv32: bool,
) {
    let initial_x = if rv32 {
        std::array::from_fn(|index| initial_x[index] & 0xffff_ffff)
    } else {
        initial_x
    };
    let mut reference_memory = RvMemory::new(0, MEMORY_LEN);
    rax::isa::riscv::Memory::write(&mut reference_memory, 0, &initial_memory)
        .expect("seed reference memory");
    let config = if rv32 {
        RiscVConfig::rv32(RvIsa::rv64gc())
    } else {
        RiscVConfig::rv64gc()
    };
    let mut cpu = RiscVCpu::new(config, Box::new(reference_memory));
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

    let mut lifter = if rv32 {
        RiscVLifter::new_rv32(RiscVExtensions::rv64gc())
    } else {
        RiscVLifter::rv64gc()
    };
    let source_arch = if rv32 {
        SourceArch::RiscV32
    } else {
        SourceArch::RiscV64
    };
    let mut context = LiftContext::new(source_arch);
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

fn run_opaque_fp_case(op: RvOp) {
    use rax::isa::riscv::float::{fp_uses_int_src1, fp_writes_int_dst};

    let writes_int = fp_writes_int_dst(op);
    let dst = if writes_int {
        VReg::Arch(ArchReg::RiscV(RiscVReg::X(5)))
    } else {
        VReg::Arch(ArchReg::RiscV(RiscVReg::F(5)))
    };
    let src1 = if fp_uses_int_src1(op) {
        VReg::Arch(ArchReg::RiscV(RiscVReg::X(1)))
    } else {
        VReg::Arch(ArchReg::RiscV(RiscVReg::F(1)))
    };
    let fcsr_reg = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0x003)));
    let opaque = SmirOp::new(
        OpId(0),
        CODE,
        OpKind::RvFp {
            dst,
            fcsr_dst: fcsr_reg,
            src1,
            src2: VReg::Arch(ArchReg::RiscV(RiscVReg::F(2))),
            src3: VReg::Arch(ArchReg::RiscV(RiscVReg::F(3))),
            fcsr_src: fcsr_reg,
            op,
            rm_field: 0,
            xlen: 64,
        },
    );
    let (function, return_pcs) = function_for_lift(ControlFlow::NextInsn, vec![opaque], 4);

    let mut x = [0u64; 32];
    x[1] = 0x8000_0001_0000_0003;
    x[5] = 0x5555_5555_5555_5555;
    let mut f = [0u64; 32];
    f[1] = 0xffff_ffff_3fc0_0000;
    f[2] = 0xffff_ffff_4020_0000;
    f[3] = 0xffff_ffff_bf00_0000;
    f[5] = 0x5555_5555_5555_5555;
    let fcsr = 0x10;
    let a = if fp_uses_int_src1(op) { x[1] } else { f[1] };
    let (expected_value, expected_fcsr) =
        rax::isa::riscv::float::eval_scalar_fp(op, 0, fcsr, a, f[2], f[3])
            .expect("ABI-listed FP operation must evaluate with RNE");

    for level in [OptLevel::O0, OptLevel::O2] {
        let mut optimized = function.clone();
        optimize_function(&mut optimized, level);
        let mut lowerer = RiscVX86_64Lowerer::new();
        lowerer.set_return_pcs(return_pcs.clone());
        let lowered = lowerer
            .lower_function(&optimized)
            .unwrap_or_else(|error| panic!("lower opaque {op:?} at {level:?}: {error:?}"));
        let code = lowerer.finalize().expect("finalize opaque FP code");
        let executable = ExecMem::new(&code).expect("map opaque FP code");
        let mut memory = TestMemory::new([0; MEMORY_LEN]);
        let mut state = jit_state(&mut memory, x, f, fcsr, CODE);
        executable.run_riscv(lowered.entry_offset, &mut state);

        if writes_int {
            assert_eq!(state.x[5], expected_value, "{op:?} result at {level:?}");
            assert_eq!(state.f[5], f[5], "{op:?} wrote the wrong register file");
        } else {
            assert_eq!(state.f[5], expected_value, "{op:?} result at {level:?}");
            assert_eq!(state.x[5], x[5], "{op:?} wrote the wrong register file");
        }
        assert_eq!(state.fcsr, u64::from(expected_fcsr), "{op:?} FCSR");
        assert_eq!(state.pc, CODE + 4, "{op:?} resume PC");
        assert_eq!(state.exit_reason, 0, "{op:?} exit classification");
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
fn lifted_rv64_scalar_fp_executes_through_helper_abi() {
    let memory = [0u8; MEMORY_LEN];

    let mut f32_regs = [0u64; 32];
    f32_regs[1] = 0xffff_ffff_3fc0_0000; // 1.5
    f32_regs[2] = 0xffff_ffff_4020_0000; // 2.5
    for instruction in [
        fp_type(0x00, 2, 1, 0, 5), // fadd.s f5,f1,f2
        fp_type(0x00, 2, 1, 0, 1), // fadd.s f1,f1,f2 (source/destination alias)
        fp_type(0x50, 2, 1, 2, 5), // feq.s x5,f1,f2
    ] {
        run_case_with_fp(&instruction.to_le_bytes(), [0; 32], f32_regs, 0, memory);
    }

    let mut f64_regs = [0u64; 32];
    f64_regs[1] = 1.0f64.to_bits();
    f64_regs[2] = 10.0f64.to_bits();
    f64_regs[3] = (-0.5f64).to_bits();
    for (instruction, fcsr) in [
        (fp_type(0x0d, 2, 1, 7, 5), 2 << 5),  // fdiv.d f5,f1,f2,dyn (RDN)
        (fp_type(0x15, 2, 1, 0, 5), 0),       // fmin.d f5,f1,f2
        (r4_type(1, 3, 2, 1, 0, 5, 0x43), 0), // fmadd.d f5,f1,f2,f3
        (fp_type(0x61, 0, 1, 1, 5), 0),       // fcvt.w.d x5,f1,rtz
    ] {
        run_case_with_fp(&instruction.to_le_bytes(), [0; 32], f64_regs, fcsr, memory);
    }

    let mut x = [0u64; 32];
    x[1] = 0x0020_0000_0000_0001;
    run_case_with_fp(
        &fp_type(0x69, 2, 1, 0, 5).to_le_bytes(), // fcvt.d.l f5,x1
        x,
        [0; 32],
        0,
        memory,
    );

    let mut discard_f = [0u64; 32];
    discard_f[1] = 1.5f64.to_bits();
    run_case_with_fp(
        &fp_type(0x61, 0, 1, 1, 0).to_le_bytes(), // fcvt.w.d x0,f1,rtz
        [0; 32],
        discard_f,
        0,
        memory,
    );

    let mut rv32_f = [0u64; 32];
    rv32_f[1] = 0xffff_ffff_bfc0_0000; // -1.5f
    run_case_rv32_with_fp(
        &fp_type(0x60, 0, 1, 1, 5).to_le_bytes(), // fcvt.w.s x5,f1,rtz
        [0; 32],
        rv32_f,
        0,
        memory,
    );
    rv32_f[1] = 0xffff_ffff_7fc0_0000; // canonical qNaN
    run_case_rv32_with_fp(
        &fp_type(0x60, 1, 1, 0, 5).to_le_bytes(), // fcvt.wu.s x5,f1,rne
        [0; 32],
        rv32_f,
        0,
        memory,
    );
}

#[test]
fn scalar_fp_helper_abi_covers_every_operation_code() {
    for code in 0..=90u64 {
        let abi_op = RiscVFpOpCode::from_code(code).expect("dense FP ABI code");
        let op = abi_op.into_op();
        assert_eq!(RiscVFpOpCode::from_op(op), Some(abi_op));
        assert_eq!(abi_op as u64, code);
        run_opaque_fp_case(op);
    }
    assert_eq!(RiscVFpOpCode::from_code(91), None);
    assert_eq!(RiscVFpOpCode::from_code(u64::MAX), None);
    assert_eq!(RiscVFpOpCode::from_op(RvOp::Add), None);
}

#[test]
fn scalar_fp_invalid_rounding_traps_without_architectural_writes() {
    for (rm_field, fcsr) in [(5, 0), (6, 0), (7, 5 << 5), (7, 6 << 5), (7, 7 << 5)] {
        let fcsr_reg = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0x003)));
        let opaque = SmirOp::new(
            OpId(0),
            CODE,
            OpKind::RvFp {
                dst: VReg::Arch(ArchReg::RiscV(RiscVReg::F(5))),
                fcsr_dst: fcsr_reg,
                src1: VReg::Arch(ArchReg::RiscV(RiscVReg::F(1))),
                src2: VReg::Arch(ArchReg::RiscV(RiscVReg::F(2))),
                src3: VReg::Arch(ArchReg::RiscV(RiscVReg::F(3))),
                fcsr_src: fcsr_reg,
                op: RvOp::FaddS,
                rm_field,
                xlen: 64,
            },
        );
        let (function, return_pcs) = function_for_lift(ControlFlow::NextInsn, vec![opaque], 4);
        for level in [OptLevel::O0, OptLevel::O2] {
            let mut optimized = function.clone();
            optimize_function(&mut optimized, level);
            let mut lowerer = RiscVX86_64Lowerer::new();
            lowerer.set_return_pcs(return_pcs.clone());
            let lowered = lowerer
                .lower_function(&optimized)
                .expect("lower invalid-rounding trap path");
            let code = lowerer.finalize().expect("finalize invalid-rounding path");
            let executable = ExecMem::new(&code).expect("map invalid-rounding path");
            let mut memory = TestMemory::new([0; MEMORY_LEN]);
            let mut f = [0u64; 32];
            f[1] = 0xffff_ffff_3f80_0000;
            f[2] = 0xffff_ffff_4000_0000;
            f[5] = 0x5555_5555_5555_5555;
            let mut state = jit_state(&mut memory, [0; 32], f, fcsr, CODE);
            executable.run_riscv(lowered.entry_offset, &mut state);

            assert_eq!(state.f[5], f[5], "rm={rm_field}, fcsr={fcsr:#x}");
            assert_eq!(state.fcsr, u64::from(fcsr));
            assert_eq!(state.pc, CODE);
            assert_eq!(state.exit_reason, 1);
        }
    }
}

#[test]
fn scalar_fp_lowering_rejects_malformed_opaque_operations() {
    let fcsr = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0x003)));
    for (op, rm_field, xlen, expected) in [
        (RvOp::Add, 0, 64, "scalar-FP helper operation Add"),
        (RvOp::FaddS, 0, 16, "scalar-FP XLEN 16"),
        (RvOp::FaddS, 8, 64, "scalar-FP rounding field 8"),
        (
            RvOp::FcvtLD,
            0,
            32,
            "RV64-only operation FcvtLD with XLEN 32",
        ),
    ] {
        let opaque = SmirOp::new(
            OpId(0),
            CODE,
            OpKind::RvFp {
                dst: VReg::Arch(ArchReg::RiscV(RiscVReg::F(5))),
                fcsr_dst: fcsr,
                src1: VReg::Imm(1),
                src2: VReg::Imm(2),
                src3: VReg::Imm(3),
                fcsr_src: fcsr,
                op,
                rm_field,
                xlen,
            },
        );
        let (function, return_pcs) = function_for_lift(ControlFlow::NextInsn, vec![opaque], 4);
        let mut lowerer = RiscVX86_64Lowerer::new();
        lowerer.set_return_pcs(return_pcs);
        let error = lowerer
            .lower_function(&function)
            .expect_err("malformed RvFp operation must fail closed");
        assert!(
            format!("{error:?}").contains(expected),
            "unexpected malformed-op error: {error:?}"
        );
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
fn lifted_rv64_integer_crypto_executes_through_helper_abi() {
    let mut x = [0u64; 32];
    x[1] = 0x0123_4567_89ab_cdef;
    x[2] = 0xfedc_ba98_7654_3210;
    let memory = [0u8; MEMORY_LEN];

    for instruction in [
        r_type(0x05, 2, 1, 1, 5),         // clmul
        r_type(0x05, 2, 1, 2, 5),         // clmulr
        r_type(0x05, 2, 1, 3, 5),         // clmulh
        r_type(0x14, 2, 1, 2, 5),         // xperm4
        r_type(0x14, 2, 1, 4, 5),         // xperm8
        r_type(0x19, 2, 1, 0, 5),         // aes64es
        r_type(0x1b, 2, 1, 0, 5),         // aes64esm
        r_type(0x1d, 2, 1, 0, 5),         // aes64ds
        r_type(0x1f, 2, 1, 0, 5),         // aes64dsm
        r_type(0x3f, 2, 1, 0, 5),         // aes64ks2
        i_type(0x18 << 5, 1, 1, 5, 0x13), // aes64im
    ] {
        run_case(&instruction.to_le_bytes(), x, memory);
    }

    for bs in 0..4u32 {
        for instruction in [
            r_type((bs << 5) | 0x18, 2, 1, 0, 5), // sm4ed
            r_type((bs << 5) | 0x1a, 2, 1, 0, 5), // sm4ks
        ] {
            run_case(&instruction.to_le_bytes(), x, memory);
        }
    }
    for round in 0..=0xau32 {
        let instruction = i_type((0x18 << 5) | 0x10 | round as i32, 1, 1, 5, 0x13);
        run_case(&instruction.to_le_bytes(), x, memory);
    }
}

#[test]
fn lifted_rv32_integer_crypto_executes_through_helper_abi() {
    let mut x = [0u64; 32];
    x[1] = 0x89ab_cdef;
    x[2] = 0x7654_3210;
    let memory = [0u8; MEMORY_LEN];

    for instruction in [
        r_type(0x05, 2, 1, 1, 5), // clmul
        r_type(0x05, 2, 1, 2, 5), // clmulr
        r_type(0x05, 2, 1, 3, 5), // clmulh
        r_type(0x14, 2, 1, 2, 5), // xperm4
        r_type(0x14, 2, 1, 4, 5), // xperm8
        r_type(0x28, 2, 1, 0, 5), // sha512sum0r
        r_type(0x29, 2, 1, 0, 5), // sha512sum1r
        r_type(0x2a, 2, 1, 0, 5), // sha512sig0l
        r_type(0x2b, 2, 1, 0, 5), // sha512sig1l
        r_type(0x2e, 2, 1, 0, 5), // sha512sig0h
        r_type(0x2f, 2, 1, 0, 5), // sha512sig1h
    ] {
        run_case_rv32(&instruction.to_le_bytes(), x, memory);
    }

    for bs in 0..4u32 {
        for low_funct7 in [0x11, 0x13, 0x15, 0x17] {
            let instruction = r_type((bs << 5) | low_funct7, 2, 1, 0, 5);
            run_case_rv32(&instruction.to_le_bytes(), x, memory);
        }
        for instruction in [
            r_type((bs << 5) | 0x18, 2, 1, 0, 5), // sm4ed
            r_type((bs << 5) | 0x1a, 2, 1, 0, 5), // sm4ks
        ] {
            run_case_rv32(&instruction.to_le_bytes(), x, memory);
        }
    }
}

#[test]
fn integer_crypto_lowering_rejects_malformed_opaque_ops() {
    for (op, xlen, expected) in [
        (RvOp::Add, 64, "helper operation Add"),
        (RvOp::Clmul, 16, "XLEN 16"),
    ] {
        let opaque = SmirOp::new(
            OpId(0),
            CODE,
            OpKind::RvIntCrypto {
                dst: VReg::Arch(ArchReg::RiscV(RiscVReg::X(5))),
                src1: VReg::Imm(1),
                src2: VReg::Imm(2),
                op,
                imm: 0,
                xlen,
            },
        );
        let (function, return_pcs) = function_for_lift(ControlFlow::NextInsn, vec![opaque], 4);
        let mut lowerer = RiscVX86_64Lowerer::new();
        lowerer.set_return_pcs(return_pcs);
        let error = lowerer
            .lower_function(&function)
            .expect_err("malformed opaque crypto op must fail closed");
        assert!(
            format!("{error:?}").contains(expected),
            "unexpected malformed-op error: {error:?}"
        );
    }
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
    assert_eq!(RiscVGuestRegs::INT_CRYPTO_FN_OFFSET, 75 * 8);
    assert_eq!(RiscVGuestRegs::FP_FN_OFFSET, 76 * 8);
    assert_eq!(std::mem::size_of::<RiscVGuestRegs>(), 77 * 8);
    assert_eq!(std::mem::size_of::<RiscVFpResult>(), 2 * 8);
}
