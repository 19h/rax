//! Scalar integer AdvSIMD programs must execute in production native regions.

use crate::isa::arm::aarch64::cpu::{AArch64Config, AArch64Cpu};
use crate::isa::arm::common::cpu::{ArmCpu, CpuExit};
use crate::isa::arm::common::memory::FlatMemory;

const ENTRY: u64 = 0x1000;
const EXIT_PC: u64 = ENTRY + 40;

fn scalar_cpu(pattern: u64, fpsr: u32) -> AArch64Cpu {
    let program = [
        0x5ee2_8420, // ADD D0,D1,D2
        0x7ee2_8403, // SUB D3,D0,D2
        0x5ee0_b864, // ABS D4,D3
        0x5ee2_5485, // SRSHL D5,D4,D2
        0x7f40_34a5, // URSRA D5,D5,#64
        0x4e20_9826, // CMEQ V6.16B,V1.16B,#0
        0x5ef1_b8c7, // ADDP D7,V6.2D
        0x7ee0_98e8, // CMLE D8,D7,#0
        0xf100_0400, // SUBS X0,X0,#1
        0x54ff_fee1, // B.NE ENTRY
        0xd420_0000, // BRK: interpreter frontier
    ];
    let bytes: Vec<_> = program.into_iter().flat_map(u32::to_le_bytes).collect();
    let mut cpu = AArch64Cpu::new(
        AArch64Config::default(),
        Box::new(FlatMemory::new(0, 0x10000)),
    );
    cpu.write_memory(ENTRY, &bytes).unwrap();
    cpu.set_pc(ENTRY);
    cpu.current_el = 1;
    cpu.sp_sel = true;
    for index in 0..31 {
        cpu.set_x(index, pattern.rotate_left(u32::from(index)));
    }
    cpu.set_x(0, 7);
    for index in 0..32 {
        let low = pattern.rotate_left(u32::from(index));
        let high = (!pattern).rotate_right(u32::from(index));
        cpu.set_simd(index, u128::from(low) | (u128::from(high) << 64));
    }
    cpu.set_fpsr(fpsr).unwrap();
    cpu.set_nzcv_bits(0b1010);
    cpu
}

#[test]
fn dependent_scalar_integer_loop_preserves_full_state_and_frontier() {
    for pattern in [0, 1, u64::MAX, 0x8000_0000_0000_0000, 0x7fff_ffff_ffff_ffff] {
        for fpsr in [0x0090, 0x0800_0090] {
            let mut direct = scalar_cpu(pattern, fpsr);
            direct.set_jit_enabled(false);
            for _ in 0..70 {
                assert!(matches!(direct.step_system().unwrap(), CpuExit::Continue));
            }
            assert_eq!(direct.get_pc(), EXIT_PC);

            let mut native = scalar_cpu(pattern, fpsr);
            let region = native.jit_compile_region().unwrap_or_else(|| {
                panic!("scalar integer loop failed native admission, pattern={pattern:#018x}");
            });
            assert!(native.jit_run_region(&region), "native region must execute");
            assert_eq!(native.get_pc(), EXIT_PC);
            for index in 0..31 {
                assert_eq!(native.get_x(index), direct.get_x(index), "X{index}");
            }
            for index in 0..32 {
                assert_eq!(native.get_simd(index), direct.get_simd(index), "V{index}");
            }
            assert_eq!(native.get_fpsr().unwrap(), direct.get_fpsr().unwrap());
            assert_eq!(native.nzcv_bits(), direct.nzcv_bits());
            assert_eq!(native.get_sp(), direct.get_sp());
        }
    }
}
