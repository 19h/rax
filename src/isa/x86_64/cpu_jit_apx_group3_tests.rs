//! Native-region regressions for Intel APX implicit Group 3 arithmetic.

use super::*;
use crate::vm::vcpu::VCpu;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const DATA_ADDRESS: u64 = 0x2000;
const STATUS_MASK: u64 = 0x08D5;
const INITIAL_RFLAGS: u64 = 0x2 | STATUS_MASK | flags::bits::DF;
const INITIAL_RAX: u64 = 0xA1B2_C3D4_E5F6_0718;
const INITIAL_RDX: u64 = 0x192A_3B4C_5D6E_7F80;

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cr0 = 0x0005_0033;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x9000;
    vcpu.regs.rflags = INITIAL_RFLAGS;
    vcpu.set_apx_enabled(true);
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(false);
    vcpu
}

fn mask(bytes: u8) -> u64 {
    if bytes == 8 {
        u64::MAX
    } else {
        (1_u64 << (u32::from(bytes) * 8)) - 1
    }
}

fn merge_gpr(old: u64, value: u64, bytes: u8) -> u64 {
    match bytes {
        1 | 2 => (old & !mask(bytes)) | (value & mask(bytes)),
        4 => value as u32 as u64,
        8 => value,
        _ => unreachable!(),
    }
}

fn apx_group3_encoding(bytes: u8, group: u8, nf: bool, memory: bool) -> Vec<u8> {
    let (opcode, p1) = match bytes {
        1 => (0xF6, 0x7C),
        2 => (0xF7, 0x7D),
        4 => (0xF7, 0x7C),
        8 => (0xF7, 0xFC),
        _ => unreachable!(),
    };
    let p2 = 0x08 | if nf { 0x04 } else { 0 };
    let modrm = (group << 3) | if memory { 3 } else { 0xC1 };
    vec![0x62, 0xF4, p1, p2, opcode, modrm]
}

fn set_source(vcpu: &mut X86_64Vcpu, memory: &GuestMemoryMmap, value: u64, memory_form: bool) {
    if memory_form {
        vcpu.regs.rbx = DATA_ADDRESS;
        memory
            .write_slice(&value.to_le_bytes(), GuestAddress(DATA_ADDRESS))
            .unwrap();
    } else {
        vcpu.regs.rcx = value;
    }
}

fn setup_success(
    vcpu: &mut X86_64Vcpu,
    memory: &GuestMemoryMmap,
    group: u8,
    bytes: u8,
    memory_form: bool,
) {
    let bits = u32::from(bytes) * 8;
    let width_mask = mask(bytes);
    vcpu.regs.rax = INITIAL_RAX;
    vcpu.regs.rdx = INITIAL_RDX;

    let source = match group {
        4 => {
            vcpu.regs.rax = merge_gpr(INITIAL_RAX, 1_u64 << (bits - 1), bytes);
            2
        }
        5 => {
            vcpu.regs.rax = merge_gpr(INITIAL_RAX, (1_u64 << (bits - 1)) - 1, bytes);
            2
        }
        6 => {
            if bytes == 1 {
                vcpu.regs.rax = merge_gpr(INITIAL_RAX, 263, 2);
            } else {
                vcpu.regs.rax = merge_gpr(INITIAL_RAX, 5, bytes);
                vcpu.regs.rdx = merge_gpr(INITIAL_RDX, 1, bytes);
            }
            10
        }
        7 => {
            if bytes == 1 {
                vcpu.regs.rax = merge_gpr(INITIAL_RAX, (-100_i16) as u16 as u64, 2);
            } else {
                vcpu.regs.rax = merge_gpr(INITIAL_RAX, (-100_i64) as u64, bytes);
                vcpu.regs.rdx = merge_gpr(INITIAL_RDX, width_mask, bytes);
            }
            7
        }
        _ => unreachable!(),
    };
    set_source(vcpu, memory, source, memory_form);
}

fn register_image_without_rflags(vcpu: &X86_64Vcpu) -> serde_json::Value {
    let mut image = serde_json::to_value(vcpu.get_regs().expect("materialize x86 registers"))
        .expect("serialize x86 register image");
    image
        .as_object_mut()
        .expect("x86 register image object")
        .remove("rflags");
    image
}

#[test]
fn native_apx_group3_implicit_matches_direct_for_all_widths_sources_and_nf_states() {
    const HLT_PC: u64 = 8;

    for group in 4..=7 {
        for bytes in [1, 2, 4, 8] {
            for nf in [false, true] {
                for memory_form in [false, true] {
                    let instruction = apx_group3_encoding(bytes, group, nf, memory_form);
                    let mut code = instruction.clone();
                    code.extend_from_slice(&[0xEB, 0x00, 0xF4]);
                    let memory = memory_with_code(&code);
                    let mut direct = test_vcpu(memory.clone());
                    let mut native = test_vcpu(memory.clone());
                    setup_success(&mut direct, &memory, group, bytes, memory_form);
                    setup_success(&mut native, &memory, group, bytes, memory_form);

                    assert!(
                        direct
                            .step()
                            .unwrap_or_else(|error| panic!(
                                "direct /{group} width={} NF={nf} memory={memory_form}: {error:#}",
                                bytes * 8
                            ))
                            .is_none()
                    );
                    assert!(direct.step().expect("direct jump to HLT").is_none());
                    assert_eq!(direct.regs.rip, HLT_PC);

                    let region = native
                        .jit_compile_region()
                        .unwrap_or_else(|error| panic!(
                            "compile /{group} width={} NF={nf} memory={memory_form}: {error:#}",
                            bytes * 8
                        ))
                        .unwrap_or_else(|| panic!(
                            "/{group} width={} NF={nf} memory={memory_form} must enter the native tier",
                            bytes * 8
                        ));
                    native.jit_run_region_native(&region);

                    assert_eq!(
                        register_image_without_rflags(&native),
                        register_image_without_rflags(&direct),
                        "/{group} width={} NF={nf} memory={memory_form} GPR state",
                        bytes * 8
                    );
                    let direct_flags = direct.get_regs().unwrap().rflags;
                    let native_flags = native.get_regs().unwrap().rflags;
                    if nf {
                        assert_eq!(
                            native_flags,
                            direct_flags,
                            "/{group} width={} NF flag image",
                            bytes * 8
                        );
                    } else if matches!(group, 4 | 5) {
                        assert_eq!(
                            native_flags & (flags::bits::CF | flags::bits::OF),
                            direct_flags & (flags::bits::CF | flags::bits::OF),
                            "/{group} width={} defined CF/OF",
                            bytes * 8
                        );
                    }
                    assert_eq!(native.regs.rip, HLT_PC, "native frontier");
                }
            }
        }
    }
}

fn setup_fault(
    vcpu: &mut X86_64Vcpu,
    memory: &GuestMemoryMmap,
    group: u8,
    bytes: u8,
    zero_divisor: bool,
    memory_form: bool,
) {
    let bits = u32::from(bytes) * 8;
    vcpu.regs.rax = INITIAL_RAX;
    vcpu.regs.rdx = INITIAL_RDX;

    let divisor = if zero_divisor {
        vcpu.regs.rax = 0x1234;
        vcpu.regs.rdx = 0;
        0
    } else if group == 6 {
        if bytes == 1 {
            vcpu.regs.rax = merge_gpr(INITIAL_RAX, 0x0100, 2);
        } else {
            vcpu.regs.rax = merge_gpr(INITIAL_RAX, 0, bytes);
            vcpu.regs.rdx = merge_gpr(INITIAL_RDX, 1, bytes);
        }
        1
    } else {
        if bytes == 1 {
            vcpu.regs.rax = merge_gpr(INITIAL_RAX, 0x8000, 2);
        } else {
            vcpu.regs.rax = merge_gpr(INITIAL_RAX, 0, bytes);
            vcpu.regs.rdx = merge_gpr(INITIAL_RDX, 1_u64 << (bits - 1), bytes);
        }
        mask(bytes)
    };
    set_source(vcpu, memory, divisor, memory_form);
}

#[test]
fn native_apx_group3_divide_guards_deopt_at_exact_noncommitting_frontier() {
    for group in [6, 7] {
        for bytes in [1, 2, 4, 8] {
            for nf in [false, true] {
                for zero_divisor in [false, true] {
                    for memory_form in [false, true] {
                        let mut code = apx_group3_encoding(bytes, group, nf, memory_form);
                        code.push(0xF4);
                        let memory = memory_with_code(&code);
                        let mut native = test_vcpu(memory.clone());
                        setup_fault(
                            &mut native,
                            &memory,
                            group,
                            bytes,
                            zero_divisor,
                            memory_form,
                        );
                        let before = serde_json::to_value(
                            native.get_regs().expect("materialize entry registers"),
                        )
                        .expect("serialize entry registers");

                        let region = native
                            .jit_compile_region()
                            .unwrap_or_else(|error| panic!(
                                "compile /{group} width={} NF={nf} zero={zero_divisor} memory={memory_form}: {error:#}",
                                bytes * 8
                            ))
                            .unwrap_or_else(|| panic!(
                                "/{group} width={} NF={nf} zero={zero_divisor} memory={memory_form} must compile guarded",
                                bytes * 8
                            ));
                        native.jit_run_region_native(&region);

                        assert_eq!(
                            native.regs.rip,
                            0,
                            "/{group} width={} NF={nf} zero={zero_divisor} memory={memory_form} frontier",
                            bytes * 8
                        );
                        assert_eq!(
                            serde_json::to_value(
                                native.get_regs().expect("materialize deopt registers")
                            )
                            .expect("serialize deopt registers"),
                            before,
                            "/{group} width={} NF={nf} zero={zero_divisor} memory={memory_form} commit",
                            bytes * 8
                        );
                    }
                }
            }
        }
    }
}
