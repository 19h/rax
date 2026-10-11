//! Compare aliases must enter native regions and preserve branch decisions.

use crate::isa::arm::aarch64::cpu::{AArch64Config, AArch64Cpu};
use crate::isa::arm::common::cpu::{ArmCpu, CpuExit};
use crate::isa::arm::common::memory::FlatMemory;

const ENTRY: u64 = 0x1000;
const EXIT_PC: u64 = ENTRY + 20;

fn comparison_cpu(comparison: u32, condition: u32, left: u64, right: u64) -> AArch64Cpu {
    let program = [
        comparison,
        0x5400_0060 | condition, // B.cond ENTRY + 16
        0x9100_0442,             // ADD X2, X2, #1
        0x1400_0002,             // B ENTRY + 20
        0xd100_0442,             // SUB X2, X2, #1
        0xd420_0000,             // BRK: exact interpreter frontier
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
    cpu.set_x(0, left);
    cpu.set_x(1, right);
    cpu.set_x(2, 0xfeed_dead_0123_4567);
    cpu.set_nzcv_bits(0b1111);
    cpu
}

#[test]
fn compare_alias_regions_preserve_all_conditional_branch_decisions() {
    for sf in 0..=1 {
        let bits = 32 << sf;
        let sign = 1u64 << (bits - 1);
        let mask = u64::MAX >> (64 - bits);
        let values = [0, 1, sign - 1, sign, sign + 1, mask - 1, mask];
        for base in [0x6b01_001f, 0x2b01_001f, 0x6a01_001f] {
            let comparison = base | (sf << 31);
            for condition in 0..=13 {
                let mut compiler = comparison_cpu(comparison, condition, 0, 0);
                let region = compiler.jit_compile_region().unwrap_or_else(|| {
                    panic!("comparison {comparison:#010x}, condition={condition}")
                });
                for left in values {
                    for right in values {
                        let mut direct = comparison_cpu(comparison, condition, left, right);
                        direct.set_jit_enabled(false);
                        for _ in 0..2 {
                            assert!(matches!(direct.step_system().unwrap(), CpuExit::Continue));
                        }
                        let native_frontier = match direct.get_pc() {
                            pc if pc == ENTRY + 16 => ENTRY + 16,
                            pc if pc == ENTRY + 8 => ENTRY + 8,
                            pc => panic!("unexpected branch target {pc:#x}"),
                        };
                        for _ in 0..3 {
                            if direct.get_pc() == EXIT_PC {
                                break;
                            }
                            assert!(matches!(direct.step_system().unwrap(), CpuExit::Continue));
                        }
                        assert_eq!(direct.get_pc(), EXIT_PC);

                        let mut native = comparison_cpu(comparison, condition, left, right);
                        assert!(native.jit_run_region(&region));
                        assert_eq!(native.get_pc(), native_frontier);
                        assert_eq!(native.nzcv_bits(), direct.nzcv_bits());
                        // CFG merging makes both scalar tails part of a block
                        // ending in BRK. Each tail resumes at its block entry.
                        native.set_jit_enabled(false);
                        for _ in 0..3 {
                            if native.get_pc() == EXIT_PC {
                                break;
                            }
                            assert!(matches!(native.step_system().unwrap(), CpuExit::Continue));
                        }
                        assert_eq!(native.get_pc(), EXIT_PC);
                        for register in 0..31 {
                            assert_eq!(
                                native.get_x(register),
                                direct.get_x(register),
                                "{comparison:#010x}, cond={condition}, left={left:#x}, right={right:#x}, X{register}"
                            );
                        }
                        assert_eq!(native.nzcv_bits(), direct.nzcv_bits());
                    }
                }
            }
        }
    }
}

#[test]
fn cmp_controlled_vector_loop_enters_native_execution() {
    let program = [
        0x9100_0400u32, // ADD X0, X0, #1
        0x4e22_8420,    // ADD V0.16B, V1.16B, V2.16B
        0xf100_1c1f,    // CMP X0, #7
        0x54ff_ffa3,    // B.LO ENTRY
        0xd420_0000,    // BRK frontier
    ];
    let bytes: Vec<_> = program.into_iter().flat_map(u32::to_le_bytes).collect();
    let mut cpu = comparison_cpu(program[2], 3, 0, 0);
    cpu.write_memory(ENTRY, &bytes).unwrap();
    cpu.sysregs.el1.cpacr |= 3 << 20;
    cpu.set_simd(0, 0xfeed_dead_cafe_beef);
    cpu.set_simd(1, u128::MAX);
    cpu.set_simd(2, 0x0101_0101_0101_0101_0101_0101_0101_0101);
    let initial_x3 = cpu.get_x(3);

    let region = cpu
        .jit_compile_region()
        .expect("CMP-controlled vector loop");
    assert!(region.uses_fp);
    assert!(cpu.jit_run_region(&region));
    assert_eq!(cpu.get_pc(), ENTRY + 16);
    assert_eq!(cpu.get_x(0), 7);
    assert_eq!(cpu.get_x(3), initial_x3);
    assert_eq!(cpu.get_simd(0), 0);
    assert_eq!(cpu.nzcv_bits(), 0b0110);
}
