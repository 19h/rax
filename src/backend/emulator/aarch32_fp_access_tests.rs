//! Architectural Undefined entry through the actual vCPU fetch/step path.
use super::*;
use crate::isa::arm::cp15::{Cpacr, Sctlr};

fn fp_vcpu(thumb: bool) -> Aarch32Vcpu {
    let raw = 0xee30_0a20u32;
    if thumb {
        let hi = ((raw >> 16) as u16).to_le_bytes();
        let lo = (raw as u16).to_le_bytes();
        thumb_vcpu(&[hi[0], hi[1], lo[0], lo[1]]).0
    } else {
        new_vcpu(&[raw]).0
    }
}

#[test]
fn fp_permission_and_enable_faults_enter_guest_undefined_without_retirement() {
    let mut count = 0;
    for thumb in [false, true] {
        for user in [false, true] {
            for permission in [0, 1, 3] {
                for enabled in [false, true] {
                    let mut vcpu = fp_vcpu(thumb);
                    vcpu.cpu.cpsr.mode = if user { USER_MODE } else { SVC_MODE } as u8;
                    vcpu.cpu.cp15.cpacr = Cpacr::from_bits(permission << 20 | permission << 22);
                    vcpu.cpu.vfp.fpexc = if enabled { 0x4000_0700 } else { 0x700 };
                    let old = vcpu.cpu.cpsr.to_u32();
                    assert!(vcpu.step_insn().unwrap().is_none());
                    let allowed = enabled && (permission == 3 || (permission == 1 && !user));
                    if allowed {
                        assert_eq!(vcpu.current_pc(), CODE + 4);
                        assert_eq!(vcpu.instruction_count(), 1);
                    } else {
                        assert_eq!(vcpu.current_pc(), 4);
                        assert_eq!(vcpu.cpu.cpsr.mode, ProcessorMode::Undefined as u8);
                        assert_eq!(vcpu.cpu.regs[14], CODE as u32 + if thumb { 2 } else { 4 });
                        assert_eq!(vcpu.cpu.spsr_und.to_u32(), old);
                        assert_eq!(vcpu.instruction_count(), 0);
                    }
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 24);
}

#[test]
fn fp_undefined_frame_saves_old_state_and_applies_handler_te_ee_and_it_reset() {
    let mut count = 0;
    for thumb in [false, true] {
        for it in [false, true] {
            if it && !thumb {
                continue;
            }
            for user in [false, true] {
                for high in [false, true] {
                    for handler_thumb in [false, true] {
                        for handler_big in [false, true] {
                            let mut vcpu = fp_vcpu(thumb);
                            vcpu.cpu.cpsr.mode = if user { USER_MODE } else { SVC_MODE } as u8;
                            vcpu.cpu.cpsr.n = true;
                            vcpu.cpu.cpsr.z = true;
                            vcpu.cpu.cpsr.c = false;
                            vcpu.cpu.cpsr.v = true;
                            vcpu.cpu.cpsr.q = true;
                            vcpu.cpu.cpsr.e = !handler_big;
                            if it {
                                vcpu.cpu.cpsr.set_it_state(0, 8);
                            }
                            vcpu.cpu.cp15.cpacr = Cpacr::from_bits(0);
                            vcpu.cpu.cp15.sctlr = Sctlr::from_bits(
                                u32::from(high) << 13
                                    | u32::from(handler_thumb) << 30
                                    | u32::from(handler_big) << 25,
                            );
                            vcpu.cpu.regs[14] = 0x1234_5678;
                            vcpu.cpu.vfp.dregs.fill(0x7ff1_2345_89ab_cdef);
                            let regs = vcpu.cpu.regs;
                            let old = vcpu.cpu.cpsr.to_u32();
                            let dregs = vcpu.cpu.vfp.dregs;
                            let fpscr = vcpu.cpu.vfp.fpscr.bits();
                            assert!(vcpu.step_insn().unwrap().is_none());
                            assert_eq!(vcpu.current_pc(), if high { 0xffff_0004 } else { 4 });
                            assert_eq!(vcpu.cpu.cpsr.mode, ProcessorMode::Undefined as u8);
                            assert_eq!(vcpu.cpu.regs[14], CODE as u32 + if thumb { 2 } else { 4 });
                            assert_eq!(vcpu.cpu.spsr_und.to_u32(), old);
                            assert_eq!(vcpu.cpu.cpsr.it_state, 0);
                            assert_eq!(vcpu.cpu.cpsr.t, handler_thumb);
                            assert_eq!(vcpu.cpu.cpsr.e, handler_big);
                            assert!(vcpu.cpu.cpsr.i);
                            assert_eq!(vcpu.cpu.regs[..13], regs[..13]);
                            assert_eq!(
                                if user {
                                    vcpu.cpu.regs_usr[1]
                                } else {
                                    vcpu.cpu.regs_svc[1]
                                },
                                0x1234_5678
                            );
                            assert_eq!(vcpu.cpu.vfp.dregs, dregs);
                            assert_eq!(vcpu.cpu.vfp.fpscr.bits(), fpscr);
                            assert_eq!(vcpu.instruction_count(), 0);
                            count += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(count, 48);
}

#[test]
fn unsupported_decoded_instruction_retains_host_error_and_no_retirement() {
    // Reserved VSEL size0 remains unsupported by the decoder/executor API.
    let (mut vcpu, _) = new_vcpu(&[0xfe00_0800]);
    assert!(matches!(
        vcpu.step_insn(),
        Err(Error::InvalidInstruction { pc: CODE, .. })
    ));
    assert_eq!(vcpu.current_pc(), CODE);
    assert_eq!(vcpu.instruction_count(), 0);
}
