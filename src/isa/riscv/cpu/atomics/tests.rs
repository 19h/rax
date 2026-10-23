//! RISC-V Privileged Architecture 1.13, Machine Cause table: AMO failures
//! report store/AMO causes, including a failure during the read phase.

use super::*;
use crate::isa::riscv::memory::FlatMemory;

const PC: u64 = 0x100;
const SENTINEL: u64 = 0xfeed_face;

fn instruction(funct5: u32, width: u32, ordering: u32) -> u32 {
    (funct5 << 27) | (ordering << 25) | (2 << 20) | (1 << 15) | (width << 12) | (6 << 7) | 0x2f
}

fn cpu(word: u32, address: u64) -> RiscVCpu {
    let mut cpu = RiscVCpu::new(RiscVConfig::rv64gc(), Box::new(FlatMemory::new(0, 0x1000)));
    cpu.write_memory(PC, &word.to_le_bytes()).unwrap();
    cpu.set_pc(PC);
    cpu.set_x(1, address);
    cpu.set_x(2, 1);
    cpu.set_x(6, SENTINEL);
    cpu.set_x(7, SENTINEL);
    cpu
}

fn assert_fault(cpu: &mut RiscVCpu, expected_cause: u64, address: u64) {
    let RiscVExit::Trap(trap) = cpu.step() else {
        panic!("expected a trap");
    };
    assert_eq!(trap.cause, expected_cause);
    assert_eq!(trap.tval, address);
    assert_eq!(cpu.csr_read(0x342).unwrap(), expected_cause);
    assert_eq!(cpu.csr_read(0x341).unwrap(), PC);
    assert_eq!(cpu.x(6), SENTINEL);
    assert_eq!(cpu.x(7), SENTINEL);
}

#[test]
fn amo_read_failures_are_store_faults() {
    for operation in [0, 1, 4, 8, 12, 16, 20, 24, 28] {
        for width in [2, 3] {
            for ordering in 0..4 {
                let mut cpu = cpu(instruction(operation, width, ordering), 0x2000);
                assert_fault(&mut cpu, cause::STORE_ACCESS_FAULT, 0x2000);
            }
        }
    }
}

#[test]
fn amocas_read_failures_are_store_faults() {
    for width in [2, 3, 4] {
        for ordering in 0..4 {
            let mut cpu = cpu(instruction(5, width, ordering), 0x2000);
            assert_fault(&mut cpu, cause::STORE_ACCESS_FAULT, 0x2000);
        }
    }
}

#[test]
fn atomic_alignment_faults_preserve_destinations() {
    for operation in [0, 1, 4, 5, 8, 12, 16, 20, 24, 28] {
        for width in [2, 3] {
            let mut cpu = cpu(instruction(operation, width, 0), 0x201);
            assert_fault(&mut cpu, cause::STORE_MISALIGNED, 0x201);
        }
    }
    let mut cpu = cpu(instruction(5, 4, 0), 0x208);
    assert_fault(&mut cpu, cause::STORE_MISALIGNED, 0x208);
}

#[test]
fn lr_access_failures_remain_load_faults() {
    for width in [2, 3] {
        // LR requires rs2=x0.
        let word = instruction(2, width, 0) & !(31 << 20);
        let mut cpu = cpu(word, 0x2000);
        assert_fault(&mut cpu, cause::LOAD_ACCESS_FAULT, 0x2000);
    }
}

#[test]
fn failed_sc_access_remains_a_store_fault() {
    for width in [2, 3] {
        let mut cpu = cpu(instruction(3, width, 0), 0x2000);
        assert_fault(&mut cpu, cause::STORE_ACCESS_FAULT, 0x2000);
    }
}

#[test]
fn amoadd_success_preserves_word_sign_extension() {
    for (width, old, expected) in [(2, 0x8000_0000, 0xffff_ffff_8000_0000), (3, 7, 7)] {
        let mut cpu = cpu(instruction(0, width, 0), 0x200);
        cpu.write_memory(0x200, &u64::to_le_bytes(old)).unwrap();
        assert!(matches!(cpu.step(), RiscVExit::Continue));
        assert_eq!(cpu.x(6), expected);
        assert_eq!(cpu.mem_read_u64(0x200).unwrap(), old + 1);
        assert_eq!(cpu.pc(), PC + 4);
    }
}

#[cfg(all(
    feature = "smir-jit",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
#[test]
fn native_atomic_read_failures_are_store_faults() {
    use crate::smir::optimize::OptLevel;

    for (operation, width) in [(0, 2), (0, 3), (5, 2), (5, 3), (5, 4)] {
        for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
            let mut cpu = cpu(instruction(operation, width, 0), 0x2000);
            let RiscVExit::Trap(trap) = cpu.step_jit(level) else {
                panic!("expected an atomic access fault at {level:?}");
            };
            assert_eq!(trap.cause, cause::STORE_ACCESS_FAULT);
            assert_eq!(trap.tval, 0x2000);
            assert_eq!(cpu.csr_read(0x341).unwrap(), PC);
            assert_eq!(cpu.x(6), SENTINEL);
            assert_eq!(cpu.x(7), SENTINEL);
            assert_eq!(cpu.jit_stats().native_executions, 1);
            assert_eq!(cpu.jit_stats().interpreter_fallbacks, 0);
        }
    }
}
