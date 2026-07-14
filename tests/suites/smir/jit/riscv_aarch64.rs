//! End-to-end RISC-V machine code -> SMIR -> AArch64 native execution tests.
#![cfg(all(feature = "smir-jit", target_arch = "aarch64"))]

use rax::isa::riscv::{FlatMemory, Isa, RiscVConfig, RiscVCpu, RiscVExit, Trap, Xlen};
use rax::smir::optimize::OptLevel;

const CODE: u64 = 0x1000;
const DATA: u64 = 0x2000;
const MEMORY_LEN: usize = 0x4000;

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
    let imm = imm as u32 & 0xfff;
    ((imm >> 5) << 25)
        | (u32::from(rs2) << 20)
        | (u32::from(rs1) << 15)
        | (funct3 << 12)
        | ((imm & 0x1f) << 7)
        | 0x23
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

fn amo_type(funct5: u32, rs2: u8, rs1: u8, funct3: u32, rd: u8) -> u32 {
    (funct5 << 27)
        | (u32::from(rs2) << 20)
        | (u32::from(rs1) << 15)
        | (funct3 << 12)
        | (u32::from(rd) << 7)
        | 0x2f
}

fn make_cpu(config: RiscVConfig) -> RiscVCpu {
    RiscVCpu::new(config, Box::new(FlatMemory::new(0, MEMORY_LEN)))
}

fn install(cpu: &mut RiscVCpu, instructions: &[u32]) {
    let bytes = instructions
        .iter()
        .flat_map(|instruction| instruction.to_le_bytes())
        .collect::<Vec<_>>();
    cpu.write_memory(CODE, &bytes).expect("write RISC-V code");
    cpu.set_pc(CODE);
}

fn assert_equivalent(actual: &RiscVCpu, expected: &RiscVCpu) {
    for register in 0..32u8 {
        assert_eq!(actual.x(register), expected.x(register), "x{register}");
        assert_eq!(actual.f(register), expected.f(register), "f{register}");
    }
    assert_eq!(actual.pc(), expected.pc(), "PC");
    assert_eq!(actual.fcsr(), expected.fcsr(), "FCSR");
    assert_eq!(actual.instret(), expected.instret(), "instret");
    for csr in [0xc00, 0x341, 0x342, 0x343] {
        assert_eq!(actual.csr_read(csr), expected.csr_read(csr), "CSR {csr:#x}");
    }
    let mut actual_memory = vec![0; MEMORY_LEN];
    let mut expected_memory = vec![0; MEMORY_LEN];
    actual
        .read_memory(0, &mut actual_memory)
        .expect("read JIT memory");
    expected
        .read_memory(0, &mut expected_memory)
        .expect("read interpreter memory");
    assert_eq!(actual_memory, expected_memory, "guest memory");
}

#[test]
fn production_scalar_i_m_zbb_and_control_flow_match_interpreter() {
    let instructions = [
        i_type(17, 1, 0, 5, 0x13),     // addi x5,x1,17
        r_type(0x20, 2, 5, 0, 6),      // sub x6,x5,x2
        r_type(0x00, 3, 6, 1, 7),      // sll x7,x6,x3
        r_type(0x20, 3, 7, 5, 8),      // sra x8,x7,x3
        r_type(0x01, 2, 1, 0, 9),      // mul x9,x1,x2
        r_type(0x01, 2, 1, 1, 10),     // mulh x10,x1,x2
        r_type(0x01, 2, 1, 4, 11),     // div x11,x1,x2
        r_type(0x01, 2, 1, 6, 12),     // rem x12,x1,x2
        i_type(0x600, 1, 1, 13, 0x13), // clz x13,x1
        i_type(0x601, 1, 1, 14, 0x13), // ctz x14,x1
        i_type(0x602, 1, 1, 15, 0x13), // cpop x15,x1
        b_type(8, 2, 1, 0),            // beq x1,x2,+8 (not taken)
        b_type(8, 2, 1, 1),            // bne x1,x2,+8 (taken)
    ];
    let mut expected = make_cpu(RiscVConfig::rv64gc());
    let mut actual = make_cpu(RiscVConfig::rv64gc());
    for cpu in [&mut expected, &mut actual] {
        install(cpu, &instructions);
        cpu.set_x(1, 0xf123_4567_89ab_cdf0);
        cpu.set_x(2, 0x1234_5678);
        cpu.set_x(3, 11);
    }

    for instruction in instructions {
        assert_eq!(expected.step(), RiscVExit::Continue, "{instruction:08x}");
        assert_eq!(
            actual.step_jit(OptLevel::O2),
            RiscVExit::Continue,
            "{instruction:08x}"
        );
        assert_equivalent(&actual, &expected);
        // Branches alter PC; restore the next corpus address so both branch
        // outcomes are verified as independent single-instruction blocks.
        let next = CODE
            + 4 * (instructions
                .iter()
                .position(|candidate| *candidate == instruction)
                .unwrap() as u64
                + 1);
        expected.set_pc(next);
        actual.set_pc(next);
    }

    let stats = actual.jit_stats();
    assert_eq!(stats.native_executions, instructions.len() as u64);
    assert_eq!(stats.interpreter_fallbacks, 0);
}

#[test]
fn production_scalar_memory_and_faults_are_precise() {
    let instructions = [
        i_type(0, 1, 0b010, 5, 0x03), // lw x5,0(x1)
        s_type(8, 2, 1, 0b011),       // sd x2,8(x1)
    ];
    let mut expected = make_cpu(RiscVConfig::rv64gc());
    let mut actual = make_cpu(RiscVConfig::rv64gc());
    for cpu in [&mut expected, &mut actual] {
        install(cpu, &instructions);
        cpu.set_x(1, DATA);
        cpu.set_x(2, 0x1122_3344_5566_7788);
        cpu.write_memory(DATA, &0x8000_0001u32.to_le_bytes())
            .expect("seed load word");
    }
    for instruction in instructions {
        assert_eq!(expected.step(), RiscVExit::Continue, "{instruction:08x}");
        assert_eq!(actual.step_jit(OptLevel::O0), RiscVExit::Continue);
        assert_equivalent(&actual, &expected);
    }
    assert_eq!(actual.jit_stats().native_executions, 2);

    let faulting = i_type(0, 1, 0b011, 5, 0x03); // ld x5,0(x1)
    let mut expected = make_cpu(RiscVConfig::rv64gc());
    let mut actual = make_cpu(RiscVConfig::rv64gc());
    for cpu in [&mut expected, &mut actual] {
        install(cpu, &[faulting]);
        cpu.set_x(1, MEMORY_LEN as u64 - 4);
        cpu.set_x(5, 0xfeed_face_cafe_beef);
    }
    let expected_exit = expected.step();
    let actual_exit = actual.step_jit(OptLevel::O2);
    assert_eq!(
        actual_exit,
        RiscVExit::Trap(Trap {
            cause: 5,
            tval: MEMORY_LEN as u64 - 4,
        })
    );
    assert_eq!(actual_exit, expected_exit);
    assert_equivalent(&actual, &expected);
    assert_eq!(actual.jit_stats().native_executions, 1);
    assert_eq!(actual.jit_stats().interpreter_fallbacks, 0);
}

#[test]
fn production_integer_crypto_and_scalar_fp_helpers_are_native() {
    let instructions = [
        r_type(0x05, 2, 1, 1, 5),              // clmul x5,x1,x2
        r_type_opcode(0x00, 2, 1, 0, 5, 0x53), // fadd.s f5,f1,f2
        r_type_opcode(0x50, 2, 1, 2, 6, 0x53), // feq.s x6,f1,f2
    ];
    let mut expected = make_cpu(RiscVConfig::rv64gc());
    let mut actual = make_cpu(RiscVConfig::rv64gc());
    for cpu in [&mut expected, &mut actual] {
        install(cpu, &instructions);
        cpu.set_x(1, 0x0123_4567_89ab_cdef);
        cpu.set_x(2, 0xfedc_ba98_7654_3210);
        cpu.set_f(1, 0xffff_ffff_3fc0_0000); // 1.5f32
        cpu.set_f(2, 0xffff_ffff_4020_0000); // 2.5f32
    }
    for instruction in instructions {
        assert_eq!(expected.step(), RiscVExit::Continue, "{instruction:08x}");
        assert_eq!(actual.step_jit(OptLevel::O2), RiscVExit::Continue);
        assert_equivalent(&actual, &expected);
    }
    assert_eq!(
        actual.jit_stats().native_executions,
        instructions.len() as u64
    );
    assert_eq!(actual.jit_stats().interpreter_fallbacks, 0);
}

#[test]
fn production_rv32_wraps_addresses_and_scalar_atomics_are_native() {
    let mut expected = make_cpu(RiscVConfig::rv32(Isa::rv64gc()));
    let mut actual = make_cpu(RiscVConfig::rv32(Isa::rv64gc()));
    let load = i_type(1, 1, 0b100, 5, 0x03); // lbu x5,1(x1)
    for cpu in [&mut expected, &mut actual] {
        install(cpu, &[load]);
        cpu.set_x(1, 0xffff_ffff);
        cpu.write_memory(0, &[0xab]).expect("seed wrapped byte");
    }
    assert_eq!(expected.step(), RiscVExit::Continue);
    assert_eq!(actual.step_jit(OptLevel::O2), RiscVExit::Continue);
    assert_equivalent(&actual, &expected);
    assert_eq!(actual.x(5), 0xab);
    assert_eq!(actual.jit_stats().native_executions, 1);

    let atomics = [
        amo_type(0b00010, 0, 1, 0b010, 3), // lr.w x3,(x1)
        amo_type(0b00011, 2, 1, 0b010, 4), // sc.w x4,x2,(x1)
        amo_type(0b00000, 2, 1, 0b010, 5), // amoadd.w x5,x2,(x1)
        amo_type(0b00101, 8, 1, 0b010, 6), // amocas.w x6,x8,(x1)
    ];
    let mut expected = make_cpu(RiscVConfig::rv64gc());
    let mut actual = make_cpu(RiscVConfig::rv64gc());
    for cpu in [&mut expected, &mut actual] {
        install(cpu, &atomics);
        cpu.set_x(1, DATA);
        cpu.set_x(2, 7);
        cpu.set_x(6, 14);
        cpu.set_x(8, 21);
        cpu.write_memory(DATA, &5u32.to_le_bytes())
            .expect("seed atomic word");
    }
    for instruction in atomics {
        assert_eq!(expected.step(), RiscVExit::Continue, "{instruction:08x}");
        assert_eq!(actual.step_jit(OptLevel::O2), RiscVExit::Continue);
        assert_equivalent(&actual, &expected);
    }
    assert_eq!(actual.jit_stats().native_executions, atomics.len() as u64);
    assert_eq!(actual.jit_stats().interpreter_fallbacks, 0);

    let pair_cas = amo_type(0b00101, 8, 10, 0b100, 6); // amocas.q x6,x8,(x10)
    let old = [0x0123_4567_89ab_cdefu64, 0xfedc_ba98_7654_3210u64];
    let new = [0x1111_2222_3333_4444u64, 0x5555_6666_7777_8888u64];
    let mut expected = make_cpu(RiscVConfig::rv64gc());
    let mut actual = make_cpu(RiscVConfig::rv64gc());
    for cpu in [&mut expected, &mut actual] {
        install(cpu, &[pair_cas]);
        cpu.set_x(10, DATA);
        cpu.set_x(6, old[0]);
        cpu.set_x(7, old[1]);
        cpu.set_x(8, new[0]);
        cpu.set_x(9, new[1]);
        cpu.write_memory(DATA, &old[0].to_le_bytes())
            .expect("seed pair-CAS low word");
        cpu.write_memory(DATA + 8, &old[1].to_le_bytes())
            .expect("seed pair-CAS high word");
    }
    assert_eq!(expected.step(), RiscVExit::Continue);
    assert_eq!(actual.step_jit(OptLevel::O2), RiscVExit::Continue);
    assert_equivalent(&actual, &expected);
    assert_eq!(actual.jit_stats().native_executions, 1);
    assert_eq!(actual.jit_stats().interpreter_fallbacks, 0);
}

#[test]
fn production_cache_reuses_native_aarch64_blocks() {
    let mut cpu = make_cpu(RiscVConfig {
        xlen: Xlen::Rv64,
        isa: Isa::rv64gc(),
    });
    let add = i_type(5, 5, 0, 5, 0x13);
    install(&mut cpu, &[add]);
    cpu.set_x(5, 10);
    for expected in [15, 20] {
        cpu.set_pc(CODE);
        assert_eq!(cpu.step_jit(OptLevel::O2), RiscVExit::Continue);
        assert_eq!(cpu.x(5), expected);
    }
    let stats = cpu.jit_stats();
    assert_eq!(stats.cache_entries, 1);
    assert_eq!(stats.cache_misses, 1);
    assert_eq!(stats.cache_hits, 1);
    assert_eq!(stats.native_executions, 2);
    assert_eq!(stats.interpreter_fallbacks, 0);
}
