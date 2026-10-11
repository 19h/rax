//! Width transitions must execute inside production native regions.

use crate::isa::arm::aarch64::cpu::{AArch64Config, AArch64Cpu};
use crate::isa::arm::common::cpu::{ArmCpu, CpuExit};
use crate::isa::arm::common::memory::FlatMemory;

const ENTRY: u64 = 0x1000;
const EXIT_PC: u64 = ENTRY + 40;

fn resize_cpu(pattern: u64, fpsr: u32) -> AArch64Cpu {
    let program = [
        0x0e22_0020, // SADDL V0.8H, V1.8B, V2.8B
        0x6e62_8020, // UMLAL2 V0.4S, V1.8H, V2.8H
        0x0e21_4823, // SQXTN V3.8B, V1.8H
        0x4e21_2823, // XTN2 V3.16B, V1.8H
        0x6f08_a464, // USHLL2 V4.8H, V3.16B, #0
        0x2f0f_9c80, // UQRSHRN V0.8B, V4.8H, #1
        0x6e21_3865, // SHLL2 V5.8H, V3.16B, #8
        0x4e20_68a6, // SADALP V6.8H, V5.16B
        0xf100_0400, // SUBS X0, X0, #1
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
fn dependent_resize_loop_preserves_complete_registers_and_sticky_qc() {
    for pattern in [0, 1, u64::MAX, 0x8000_7fff_ff00_00ff, 0x7fff_ffff_8000_0000] {
        for fpsr in [0x0090, 0x0800_0090] {
            let mut direct = resize_cpu(pattern, fpsr);
            direct.set_jit_enabled(false);
            for _ in 0..70 {
                assert!(matches!(direct.step_system().unwrap(), CpuExit::Continue));
            }
            assert_eq!(direct.get_pc(), EXIT_PC);

            let mut native = resize_cpu(pattern, fpsr);
            let region = native.jit_compile_region().unwrap_or_else(|| {
                panic!("resize loop failed native admission, pattern={pattern:#018x}");
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
