//! Selected non-trapping/non-short-vector register profile, Arm100446_0200_01.
//! FPSCR sections4.4 and FPEXC4.8 define the masks independently of arithmetic.
use rax::isa::arm::aarch32::vfp::Fpscr;
use rax::isa::arm::decoder::{Aarch32Decoder, ThumbDecoder};
use rax::isa::arm::execution::FlatMemory;
use rax::isa::arm::{Armv7Cpu, ExecResult, Executor};

fn execute(cpu: &mut Armv7Cpu, memory: &mut FlatMemory, raw: u32) -> ExecResult {
    let insn = if cpu.cpsr.t {
        ThumbDecoder::decode_32bit(raw)
    } else {
        Aarch32Decoder::decode(raw)
    }
    .unwrap();
    Executor::new(cpu, memory).execute(&insn)
}

fn values() -> Vec<u32> {
    let mut values = vec![0, u32::MAX, 0xffc8_009f, 0x4000_0700];
    for bit in 0..32 {
        values.push(1 << bit);
        values.push(!(1 << bit));
    }
    let mut state = 0x43f0_0d98_u32;
    for _ in 0..4096 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        values.push(state);
    }
    assert_eq!(values.len(), 4164);
    values
}

#[test]
fn fpscr_writes_clear_all_reserved_fields_and_preserve_supported_fields() {
    let mut memory = FlatMemory::new(4096, 0);
    for thumb in [false, true] {
        for value in values() {
            let mut cpu = Armv7Cpu::new();
            cpu.cpsr.t = thumb;
            cpu.vfp.dregs.fill(0x7ff1_2345_89ab_cdef);
            cpu.regs[0] = value;
            let cpsr = cpu.cpsr.to_u32();
            let dregs = cpu.vfp.dregs;
            assert!(matches!(
                execute(&mut cpu, &mut memory, 0xeee1_0a10),
                ExecResult::Continue
            ));
            assert_eq!(cpu.vfp.fpscr.bits(), value & 0xffc8_009f);
            assert!(matches!(
                execute(&mut cpu, &mut memory, 0xeef1_1a10),
                ExecResult::Continue
            ));
            assert_eq!(cpu.regs[1], value & 0xffc8_009f);
            assert_eq!(cpu.vfp.dregs, dregs);
            assert_eq!(cpu.cpsr.to_u32(), cpsr);
        }
    }
}

#[test]
fn fpscr_reads_mask_raw_state_without_mutating_it_and_apsr_receives_only_nzcv() {
    let mut memory = FlatMemory::new(4096, 0);
    for thumb in [false, true] {
        for value in values() {
            let mut cpu = Armv7Cpu::new();
            cpu.cpsr.t = thumb;
            cpu.cpsr.q = true;
            cpu.vfp.fpscr = Fpscr::from_bits(value);
            let cpsr = cpu.cpsr.to_u32();
            assert!(matches!(
                execute(&mut cpu, &mut memory, 0xeef1_1a10),
                ExecResult::Continue
            ));
            assert_eq!(cpu.regs[1], value & 0xffc8_009f);
            assert_eq!(cpu.vfp.fpscr.bits(), value);
            assert_eq!(cpu.cpsr.to_u32(), cpsr);
            let regs = cpu.regs;
            assert!(matches!(
                execute(&mut cpu, &mut memory, 0xeef1_fa10),
                ExecResult::Continue
            ));
            assert_eq!(
                cpu.cpsr.to_u32(),
                (cpsr & 0x0fff_ffff) | (value & 0xf000_0000)
            );
            assert_eq!(cpu.regs, regs);
            assert_eq!(cpu.vfp.fpscr.bits(), value);
        }
    }
}

#[test]
fn fpexc_writes_only_change_enable_and_preserve_fixed_profile_bits() {
    let mut memory = FlatMemory::new(4096, 0);
    assert_eq!(Armv7Cpu::new().vfp.fpexc, 0x4000_0700);
    for thumb in [false, true] {
        for value in values() {
            let mut cpu = Armv7Cpu::new();
            cpu.cpsr.t = thumb;
            cpu.regs[0] = value;
            cpu.vfp.fpscr = Fpscr::from_bits(0xaac8_009f);
            cpu.vfp.dregs.fill(0x7ff1_2345_89ab_cdef);
            let cpsr = cpu.cpsr.to_u32();
            let dregs = cpu.vfp.dregs;
            let expected = (value & 0x4000_0000) | 0x700;
            assert!(matches!(
                execute(&mut cpu, &mut memory, 0xeee8_0a10),
                ExecResult::Continue
            ));
            assert_eq!(cpu.vfp.fpexc, expected);
            assert_eq!(cpu.vfp.is_enabled(), value & 0x4000_0000 != 0);
            assert!(matches!(
                execute(&mut cpu, &mut memory, 0xeef8_1a10),
                ExecResult::Continue
            ));
            assert_eq!(cpu.regs[1], expected);
            assert_eq!(cpu.vfp.fpscr.bits(), 0xaac8_009f);
            assert_eq!(cpu.vfp.dregs, dregs);
            assert_eq!(cpu.cpsr.to_u32(), cpsr);
        }
    }
}

#[test]
fn fpexc_architectural_reads_canonicalize_raw_state_without_mutating_it() {
    let mut memory = FlatMemory::new(4096, 0);
    for thumb in [false, true] {
        for value in values() {
            let mut cpu = Armv7Cpu::new();
            cpu.cpsr.t = thumb;
            cpu.vfp.fpexc = value;
            let cpsr = cpu.cpsr.to_u32();
            assert!(matches!(
                execute(&mut cpu, &mut memory, 0xeef8_1a10),
                ExecResult::Continue
            ));
            assert_eq!(cpu.regs[1], (value & 0x4000_0000) | 0x700);
            assert_eq!(cpu.vfp.fpexc, value);
            assert_eq!(cpu.cpsr.to_u32(), cpsr);
        }
    }
}

#[test]
fn fpexc_enable_controls_fpscr_and_arithmetic_after_canonical_writes() {
    let mut memory = FlatMemory::new(4096, 0);
    for thumb in [false, true] {
        let mut cpu = Armv7Cpu::new();
        cpu.cpsr.t = thumb;
        for value in [0xbfff_ffff, u32::MAX, 0, 0x4000_0000] {
            cpu.regs[0] = value;
            assert!(matches!(
                execute(&mut cpu, &mut memory, 0xeee8_0a10),
                ExecResult::Continue
            ));
            let enabled = value & 0x4000_0000 != 0;
            for raw in [0xeef1_1a10, 0xeee1_0a10, 0xee30_0a20] {
                let result = execute(&mut cpu, &mut memory, raw);
                assert!(if enabled {
                    matches!(result, ExecResult::Continue)
                } else {
                    matches!(result, ExecResult::Exception(_))
                });
            }
            assert_eq!(cpu.vfp.fpexc, (value & 0x4000_0000) | 0x700);
        }
    }
}
