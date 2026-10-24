//! CPACR.cp10 denies before effects, permits PL1/full access, and follows predicates.
use rax::isa::arm::aarch32::cp15::{Cp15State, Cpacr};
use rax::isa::arm::aarch32::cpu::{ArmMemory, MemoryError, ProcessorMode};
use rax::isa::arm::aarch32::vfp::Fpscr;
use rax::isa::arm::decoder::{Aarch32Decoder, DecodedInsn, ThumbDecoder};
use rax::isa::arm::execution::FlatMemory;
use rax::isa::arm::{Armv7Cpu, ExceptionType, ExecResult, Executor};
include!("fp_access_words.rs");

struct NoMemoryAccess;
impl ArmMemory for NoMemoryAccess {
    fn read_word(&self, _: u32) -> Result<u32, MemoryError> {
        panic!("denied FP memory read")
    }
    fn write_word(&mut self, _: u32, _: u32) -> Result<(), MemoryError> {
        panic!("denied FP memory write")
    }
    fn read_halfword(&self, _: u32) -> Result<u16, MemoryError> {
        panic!("denied FP memory read")
    }
    fn write_halfword(&mut self, _: u32, _: u16) -> Result<(), MemoryError> {
        panic!("denied FP memory write")
    }
    fn read_byte(&self, _: u32) -> Result<u8, MemoryError> {
        panic!("denied FP memory read")
    }
    fn write_byte(&mut self, _: u32, _: u8) -> Result<(), MemoryError> {
        panic!("denied FP memory write")
    }
}
fn cpu(thumb: bool, user: bool, cp10: u32, cp11: u32) -> Armv7Cpu {
    let mut cpu = Armv7Cpu::new();
    cpu.cpsr.t = thumb;
    if user {
        cpu.cpsr.mode = ProcessorMode::User as u8;
    }
    cpu.cp15.cpacr = Cpacr::from_bits(cp10 << 20 | cp11 << 22);
    cpu.regs[0] = 0x4000_0000;
    cpu.regs[1] = 0x100;
    cpu.regs[13] = 0x800;
    cpu.vfp.dregs.fill(0x3ff0_0000_3f80_0000);
    cpu.vfp.fpscr = Fpscr::from_bits(0xa8c8_009f);
    cpu
}
fn decode(raw: u32, thumb: bool) -> DecodedInsn {
    if thumb {
        ThumbDecoder::decode_32bit(raw)
    } else {
        Aarch32Decoder::decode(raw)
    }
    .unwrap()
}
fn denied(cpu: &mut Armv7Cpu, insn: &DecodedInsn) {
    let regs = cpu.regs;
    let cpsr = cpu.cpsr.to_u32();
    let dregs = cpu.vfp.dregs;
    let fpscr = cpu.vfp.fpscr.bits();
    let fpexc = cpu.vfp.fpexc;
    let cpacr = cpu.cp15.cpacr.bits();
    let result = Executor::new(cpu, &mut NoMemoryAccess).execute(insn);
    assert!(
        matches!(
            result,
            ExecResult::Undefined | ExecResult::Exception(ExceptionType::UndefinedInstruction)
        ),
        "raw={:08x} result={result:?}",
        insn.raw
    );
    assert_eq!(cpu.regs, regs);
    assert_eq!(cpu.cpsr.to_u32(), cpsr);
    assert_eq!(cpu.vfp.dregs, dregs);
    assert_eq!(cpu.vfp.fpscr.bits(), fpscr);
    assert_eq!(cpu.vfp.fpexc, fpexc);
    assert_eq!(cpu.cp15.cpacr.bits(), cpacr);
}
#[test]
fn denied_and_reserved_cp10_permissions_trap_every_fp_route_before_any_effect() {
    let mut count = 0;
    for words in FP_WORDS {
        for thumb in [false, true] {
            for user in [false, true] {
                for cp10 in 0..4 {
                    if cp10 == 3 || (cp10 == 1 && !user) {
                        continue;
                    }
                    for cp11 in 0..4 {
                        let mut cpu = cpu(thumb, user, cp10, cp11);
                        denied(&mut cpu, &decode(words[usize::from(thumb)], thumb));
                        count += 1;
                    }
                }
            }
        }
    }
    assert_eq!(count, 9800);
}
#[test]
fn permitted_cp10_permissions_keep_every_fp_route_available_independently_of_cp11() {
    let mut count = 0;
    let mut memory = FlatMemory::new(4096, 0);
    for words in FP_WORDS {
        for thumb in [false, true] {
            for user in [false, true] {
                for cp10 in [1, 3] {
                    if cp10 == 1 && user {
                        continue;
                    }
                    for cp11 in 0..4 {
                        let mut cpu = cpu(thumb, user, cp10, cp11);
                        let insn = decode(words[usize::from(thumb)], thumb);
                        let result = Executor::new(&mut cpu, &mut memory).execute(&insn);
                        assert!(
                            matches!(result, ExecResult::Continue),
                            "raw={:08x} Thumb={thumb} User={user} cp10={cp10} cp11={cp11}: {result:?}",
                            insn.raw
                        );
                        count += 1;
                    }
                }
            }
        }
    }
    assert_eq!(count, 5880);
}
#[test]
fn failed_arm_and_thumb_predicates_skip_denied_fp_access_checks() {
    for thumb in [false, true] {
        let mut cpu = cpu(thumb, true, 0, 0);
        cpu.cpsr.z = false;
        if thumb {
            cpu.cpsr.set_it_state(0, 8);
        }
        let insn = decode(if thumb { 0xee300a20 } else { 0x0e300a20 }, thumb);
        let regs = cpu.regs;
        let cpsr = cpu.cpsr.to_u32();
        let dregs = cpu.vfp.dregs;
        let status = cpu.vfp.fpscr.bits();
        assert!(matches!(
            Executor::new(&mut cpu, &mut NoMemoryAccess).execute(&insn),
            ExecResult::Continue
        ));
        assert_eq!(cpu.regs, regs);
        assert_eq!(cpu.cpsr.to_u32(), cpsr);
        assert_eq!(cpu.vfp.dregs, dregs);
        assert_eq!(cpu.vfp.fpscr.bits(), status);
    }
}
#[test]
fn software_cpu_default_matches_enabled_fp_while_standalone_cp15_keeps_reset_permissions() {
    let cpu = Armv7Cpu::new();
    assert!(cpu.vfp.is_enabled());
    assert_eq!(cpu.cp15.cpacr.cp_access(10), 3);
    assert_eq!(cpu.cp15.cpacr.cp_access(11), 3);
    assert_eq!(Cp15State::new().cpacr.bits(), 0);
}
#[test]
fn fp_system_register_permissions_distinguish_fpscr_and_enable_independent_views() {
    let mut memory = NoMemoryAccess;
    let mut count = 0;
    for thumb in [false, true] {
        for user in [false, true] {
            for enabled in [false, true] {
                for cp10 in 0..4 {
                    for cp11 in 0..4 {
                        for (raw, user_allowed, needs_enable) in [
                            (0xeef10a10, true, true),
                            (0xeee10a10, true, true),
                            (0xeef80a10, false, false),
                            (0xeee80a10, false, false),
                            (0xeef60a10, false, false),
                        ] {
                            let mut cpu = cpu(thumb, user, cp10, cp11);
                            cpu.vfp.fpexc = if enabled { 0x40000700 } else { 0x700 };
                            let insn = decode(raw, thumb);
                            let permitted = (cp10 == 3 || (cp10 == 1 && !user))
                                && (!user || user_allowed)
                                && (!needs_enable || enabled);
                            if permitted {
                                let result = Executor::new(&mut cpu, &mut memory).execute(&insn);
                                assert!(
                                    matches!(result, ExecResult::Continue),
                                    "raw={raw:08x}: {result:?}"
                                );
                            } else {
                                denied(&mut cpu, &insn);
                            }
                            count += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(count, 640);
}
#[test]
fn ordinary_integer_execution_does_not_require_fp_access() {
    let mut cpu = cpu(false, true, 0, 0);
    cpu.regs[0] = 17;
    let insn = Aarch32Decoder::decode(0xe2800001).unwrap();
    assert!(matches!(
        Executor::new(&mut cpu, &mut NoMemoryAccess).execute(&insn),
        ExecResult::Continue
    ));
    assert_eq!(cpu.regs[0], 18);
}

#[test]
fn every_fp_pair_remains_explicitly_unsupported_without_smir_control_state() {
    use rax::smir::ir::types::SourceArch;
    use rax::smir::lift::aarch32::Aarch32Lifter;
    use rax::smir::lift::thumb::ThumbLifter;
    use rax::smir::lift::{LiftContext, LiftError, SmirLifter};
    let mut count = 0;
    for words in FP_WORDS {
        for thumb in [false, true] {
            let raw = words[usize::from(thumb)];
            let bytes = if thumb {
                let hi = ((raw >> 16) as u16).to_le_bytes();
                let lo = (raw as u16).to_le_bytes();
                [hi[0], hi[1], lo[0], lo[1]]
            } else {
                raw.to_le_bytes()
            };
            let mut context = LiftContext::new(if thumb {
                SourceArch::Thumb
            } else {
                SourceArch::Aarch32
            });
            let mut lifter: Box<dyn SmirLifter> = if thumb {
                Box::new(ThumbLifter::new())
            } else {
                Box::new(Aarch32Lifter::new())
            };
            assert!(
                matches!(
                    lifter.lift_insn(0x1000, &bytes, &mut context),
                    Err(LiftError::Unsupported { addr: 0x1000, .. })
                ),
                "raw={raw:08x} Thumb={thumb}"
            );
            count += 1;
        }
    }
    assert_eq!(count, 490);
}
