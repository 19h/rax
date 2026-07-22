//! Direct-execution regressions for Intel APX NF exclusions.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::isa::x86_64::flags;
use crate::vm::vcpu::VCpu;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const INVALID_DATA: u64 = 0x2_0000;
const SENTINEL_ADDR: u64 = 0x3000;
const SENTINEL: u64 = 0x0123_4567_89AB_CDEF;

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
        .write_slice(&SENTINEL.to_le_bytes(), GuestAddress(SENTINEL_ADDR))
        .unwrap();
    memory
}

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cr0 = 0x0005_0033;
    vcpu.regs.rip = 0;
    vcpu.regs.rax = INVALID_DATA;
    vcpu.regs.rbx = 0x1111_2222_3333_4444;
    vcpu.regs.rcx = 0x5555_6666_7777_8888;
    vcpu.regs.rdx = 0x9999_AAAA_BBBB_CCCC;
    vcpu.regs.rsp = 0x9000;
    vcpu.regs.r8 = 0xDDDD_EEEE_FFFF_0000;
    vcpu.regs.rflags = 0x2 | 0x08D5 | flags::bits::DF;
    vcpu.set_apx_enabled(true);
    vcpu
}

fn register_image(vcpu: &X86_64Vcpu) -> serde_json::Value {
    serde_json::to_value(vcpu.get_regs().expect("read materialized x86 registers"))
        .expect("serialize x86 register image")
}

fn system_image(vcpu: &X86_64Vcpu) -> serde_json::Value {
    serde_json::to_value(vcpu.get_sregs().expect("read x86 system registers"))
        .expect("serialize x86 system-register image")
}

fn apx_prefix(nd: bool, w: bool, pp: u8, nf: bool) -> [u8; 4] {
    let p1 = (if nd { 0x3C } else { 0x7C }) | (if w { 0x80 } else { 0 }) | pp;
    let p2 = 0x08 | if nd { 0x10 } else { 0 } | if nf { 0x04 } else { 0 };
    [0x62, 0xF4, p1, p2]
}

fn assert_precise_ud(code: &[u8], name: &str) {
    let memory = memory_with_code(code);
    let mut vcpu = test_vcpu(memory.clone());
    let before = register_image(&vcpu);
    let before_system = system_image(&vcpu);

    let error = format!("{:#}", vcpu.step().expect_err(name));
    assert!(
        error.contains("IDT entry 6 not present"),
        "{name}: expected #UD before any apparent operand fault, got {error}"
    );
    assert_eq!(vcpu.regs.rip, 0, "{name}: precise faulting RIP");
    assert_eq!(register_image(&vcpu), before, "{name}: state commit");
    assert_eq!(
        system_image(&vcpu),
        before_system,
        "{name}: system-state commit"
    );

    let mut sentinel = [0_u8; 8];
    memory
        .read_slice(&mut sentinel, GuestAddress(SENTINEL_ADDR))
        .unwrap();
    assert_eq!(u64::from_le_bytes(sentinel), SENTINEL, "{name}: memory");
}

#[test]
fn every_apx_nf_adc_sbb_register_opcode_is_precise_ud_before_modrm() {
    // Intel APX Architecture Specification revision 7.0 lists ADC and SBB as
    // NDD-capable but not NF-capable. Opcode 10-13 and 18-1B identify the
    // instruction before ModR/M. Cover both ND states, both W states, every
    // architecturally applicable pp class, and register/memory-looking ModR/M.
    for nd in [false, true] {
        for w in [false, true] {
            for opcode in [0x10, 0x11, 0x12, 0x13, 0x18, 0x19, 0x1A, 0x1B] {
                let valid_pp = if opcode & 1 == 0 {
                    &[0][..]
                } else {
                    &[0, 1][..]
                };
                for &pp in valid_pp {
                    for modrm in [0x00, 0xC0] {
                        let mut code = apx_prefix(nd, w, pp, true).to_vec();
                        code.extend_from_slice(&[opcode, modrm]);
                        assert_precise_ud(
                            &code,
                            &format!(
                                "NF ADC/SBB opcode={opcode:02X} ND={nd} W={w} pp={pp} ModRM={modrm:02X}"
                            ),
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn every_apx_nf_group1_adc_sbb_form_is_precise_ud_at_modrm() {
    // ModR/M.reg distinguishes ADC (/2) and SBB (/3). The apparent operand
    // cells include register, base, SIB, disp8, and disp32 forms. No SIB,
    // displacement, or immediate bytes are supplied; mapped zero-fill would
    // let an over-decoder continue, so the #UD vector and unchanged state prove
    // that rejection occurred before those fields or the data operand mattered.
    for nd in [false, true] {
        for w in [false, true] {
            for opcode in [0x80, 0x81, 0x83] {
                let valid_pp = if opcode == 0x80 {
                    &[0][..]
                } else {
                    &[0, 1][..]
                };
                for &pp in valid_pp {
                    for group in [2, 3] {
                        for (mode, rm) in [(0, 0), (0, 4), (1, 0), (2, 0), (3, 0)] {
                            let modrm = (mode << 6) | (group << 3) | rm;
                            let mut code = apx_prefix(nd, w, pp, true).to_vec();
                            code.extend_from_slice(&[opcode, modrm]);
                            assert_precise_ud(
                                &code,
                                &format!(
                                    "NF Group1 opcode={opcode:02X} /{group} ND={nd} W={w} pp={pp} ModRM={modrm:02X}"
                                ),
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn every_apx_nf_group2_rcl_rcr_form_is_precise_ud_at_modrm() {
    // RCL and RCR are /2 and /3 in every immediate, implicit-one, and CL
    // Group 2 opcode class. Intel APX revision 7.0 specifies NF=0 for both.
    for nd in [false, true] {
        for w in [false, true] {
            for opcode in [0xC0, 0xC1, 0xD0, 0xD1, 0xD2, 0xD3] {
                let valid_pp = if opcode & 1 == 0 {
                    &[0][..]
                } else {
                    &[0, 1][..]
                };
                for &pp in valid_pp {
                    for group in [2, 3] {
                        for (mode, rm) in [(0, 0), (0, 4), (1, 0), (2, 0), (3, 0)] {
                            let modrm = (mode << 6) | (group << 3) | rm;
                            let mut code = apx_prefix(nd, w, pp, true).to_vec();
                            code.extend_from_slice(&[opcode, modrm]);
                            assert_precise_ud(
                                &code,
                                &format!(
                                    "NF Group2 opcode={opcode:02X} /{group} ND={nd} W={w} pp={pp} ModRM={modrm:02X}"
                                ),
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn every_apx_nf_group3_not_form_is_precise_ud_at_modrm() {
    // NOT is /2 in the byte and scalable Group 3 opcode classes. It supports
    // ND but not NF; classify it from ModR/M before any apparent operand.
    for nd in [false, true] {
        for w in [false, true] {
            for opcode in [0xF6, 0xF7] {
                let valid_pp = if opcode == 0xF6 {
                    &[0][..]
                } else {
                    &[0, 1][..]
                };
                for &pp in valid_pp {
                    for (mode, rm) in [(0, 0), (0, 4), (1, 0), (2, 0), (3, 0)] {
                        let modrm = (mode << 6) | (2 << 3) | rm;
                        let mut code = apx_prefix(nd, w, pp, true).to_vec();
                        code.extend_from_slice(&[opcode, modrm]);
                        assert_precise_ud(
                            &code,
                            &format!(
                                "NF Group3 NOT opcode={opcode:02X} ND={nd} W={w} pp={pp} ModRM={modrm:02X}"
                            ),
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn apx_nf_exclusion_is_narrow_for_legal_neighbors_and_required_nf0_forms() {
    for nd in [false, true] {
        for instruction in [
            &[0x11, 0xD8][..],       // ADC r/m64,r64
            &[0x19, 0xD8][..],       // SBB r/m64,r64
            &[0x83, 0xD0, 0x01][..], // ADC r/m64,imm8
            &[0x83, 0xD8, 0x01][..], // SBB r/m64,imm8
            &[0xD1, 0xD0][..],       // RCL r/m64,1
            &[0xD1, 0xD8][..],       // RCR r/m64,1
            &[0xF7, 0xD0][..],       // NOT r/m64
        ] {
            let mut code = apx_prefix(nd, true, 0, false).to_vec();
            code.extend_from_slice(instruction);
            let mut vcpu = test_vcpu(memory_with_code(&code));
            vcpu.regs.rax = 5;
            vcpu.regs.rbx = 2;
            assert!(
                vcpu.step()
                    .unwrap_or_else(|error| panic!("legal NF=0 form {code:02X?}: {error:#}"))
                    .is_none()
            );
            assert_eq!(vcpu.regs.rip, code.len() as u64, "{code:02X?}");
        }

        for instruction in [
            &[0x01, 0xD8][..],       // ADD r/m64,r64
            &[0x83, 0xC0, 0x01][..], // ADD r/m64,imm8
            &[0xD1, 0xC0][..],       // ROL r/m64,1
            &[0xD1, 0xC8][..],       // ROR r/m64,1
            &[0xD1, 0xE0][..],       // SHL r/m64,1
            &[0xD1, 0xE8][..],       // SHR r/m64,1
            &[0xD1, 0xF0][..],       // SAL/SHL r/m64,1
            &[0xD1, 0xF8][..],       // SAR r/m64,1
            &[0xF7, 0xD8][..],       // NEG r/m64
        ] {
            let mut code = apx_prefix(nd, true, 0, true).to_vec();
            code.extend_from_slice(instruction);
            let mut vcpu = test_vcpu(memory_with_code(&code));
            vcpu.regs.rax = 0x8000_0000_0000_0001;
            vcpu.regs.rbx = 2;
            let flags = vcpu.regs.rflags;
            assert!(
                vcpu.step()
                    .unwrap_or_else(|error| panic!("legal NF neighbor {code:02X?}: {error:#}"))
                    .is_none()
            );
            assert_eq!(vcpu.regs.rip, code.len() as u64, "{code:02X?}");
            assert_eq!(vcpu.regs.rflags, flags, "{code:02X?}: RFLAGS");
        }
    }
}
