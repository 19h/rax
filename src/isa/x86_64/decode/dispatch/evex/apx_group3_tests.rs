//! Direct-execution coverage for Intel APX implicit Group 3 arithmetic.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::isa::x86_64::flags;
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
    vcpu.sregs.idt.base = 0x8000;
    vcpu.sregs.idt.limit = 0x0FFF;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x9000;
    vcpu.regs.rflags = INITIAL_RFLAGS;
    vcpu.set_apx_enabled(true);
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

fn signed(value: u64, bytes: u8) -> i128 {
    let shift = 128 - u32::from(bytes) * 8;
    ((value as i128) << shift) >> shift
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

#[test]
fn direct_apx_group3_mul_imul_cover_widths_sources_overflow_and_nf() {
    for group in [4, 5] {
        for bytes in [1, 2, 4, 8] {
            let bits = u32::from(bytes) * 8;
            let width_mask = mask(bytes);
            for overflow_case in [false, true] {
                let (left, right) = match (group, overflow_case) {
                    (4, false) => (3, 2),
                    (4, true) => (1_u64 << (bits - 1), 2),
                    (5, false) => (width_mask.wrapping_sub(1), 3), // -2 * 3
                    (5, true) => ((1_u64 << (bits - 1)) - 1, 2),
                    _ => unreachable!(),
                };

                for nf in [false, true] {
                    for memory_form in [false, true] {
                        let code = apx_group3_encoding(bytes, group, nf, memory_form);
                        let memory = memory_with_code(&code);
                        let mut vcpu = test_vcpu(memory.clone());
                        vcpu.regs.rax = merge_gpr(INITIAL_RAX, left, bytes);
                        vcpu.regs.rdx = INITIAL_RDX;
                        set_source(&mut vcpu, &memory, right, memory_form);

                        assert!(
                            vcpu.step()
                                .unwrap_or_else(|error| panic!(
                                    "group=/{group} width={} NF={nf} memory={memory_form}: {error:#}",
                                    bits
                                ))
                                .is_none()
                        );

                        let (product, expected_overflow) = if group == 4 {
                            let product =
                                u128::from(left & width_mask) * u128::from(right & width_mask);
                            (product, (product >> bits) != 0)
                        } else {
                            let product = signed(left, bytes) * signed(right, bytes);
                            let min = -(1_i128 << (bits - 1));
                            let max = (1_i128 << (bits - 1)) - 1;
                            (product as u128, product < min || product > max)
                        };
                        let low = product as u64 & width_mask;
                        let high = (product >> bits) as u64 & width_mask;
                        let (expected_rax, expected_rdx) = if bytes == 1 {
                            (
                                merge_gpr(INITIAL_RAX, product as u64 & 0xFFFF, 2),
                                INITIAL_RDX,
                            )
                        } else {
                            (
                                merge_gpr(INITIAL_RAX, low, bytes),
                                merge_gpr(INITIAL_RDX, high, bytes),
                            )
                        };
                        assert_eq!(vcpu.regs.rax, expected_rax, "low product");
                        assert_eq!(vcpu.regs.rdx, expected_rdx, "high product");
                        assert_eq!(vcpu.regs.rip, code.len() as u64);

                        if nf {
                            assert_eq!(vcpu.regs.rflags, INITIAL_RFLAGS, "NF flag image");
                        } else {
                            let expected = if expected_overflow {
                                flags::bits::CF | flags::bits::OF
                            } else {
                                0
                            };
                            assert_eq!(
                                vcpu.regs.rflags & (flags::bits::CF | flags::bits::OF),
                                expected,
                                "defined CF/OF group=/{group} width={bits}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn direct_apx_group3_div_idiv_cover_double_width_dividend_sources_and_nf() {
    for group in [6, 7] {
        for bytes in [1, 2, 4, 8] {
            let bits = u32::from(bytes) * 8;
            let width_mask = mask(bytes);
            for nf in [false, true] {
                for memory_form in [false, true] {
                    let code = apx_group3_encoding(bytes, group, nf, memory_form);
                    let memory = memory_with_code(&code);
                    let mut vcpu = test_vcpu(memory.clone());
                    vcpu.regs.rax = INITIAL_RAX;
                    vcpu.regs.rdx = INITIAL_RDX;

                    let (low, high, divisor, quotient, remainder) = if group == 6 {
                        let dividend = if bytes == 1 {
                            263_u128
                        } else {
                            (1_u128 << bits) | 5
                        };
                        let divisor = 10_u64;
                        (
                            dividend as u64 & width_mask,
                            (dividend >> bits) as u64 & width_mask,
                            divisor,
                            (dividend / u128::from(divisor)) as u64,
                            (dividend % u128::from(divisor)) as u64,
                        )
                    } else {
                        let dividend = -100_i128;
                        let encoded = dividend as u128;
                        let divisor = 7_u64;
                        (
                            encoded as u64 & width_mask,
                            (encoded >> bits) as u64 & width_mask,
                            divisor,
                            (dividend / divisor as i128) as u64 & width_mask,
                            (dividend % divisor as i128) as u64 & width_mask,
                        )
                    };

                    if bytes == 1 {
                        vcpu.regs.rax = merge_gpr(INITIAL_RAX, (high << 8) | low, 2);
                    } else {
                        vcpu.regs.rax = merge_gpr(INITIAL_RAX, low, bytes);
                        vcpu.regs.rdx = merge_gpr(INITIAL_RDX, high, bytes);
                    }
                    set_source(&mut vcpu, &memory, divisor, memory_form);

                    assert!(
                        vcpu.step()
                            .unwrap_or_else(|error| panic!(
                                "group=/{group} width={bits} NF={nf} memory={memory_form}: {error:#}"
                            ))
                            .is_none()
                    );

                    let (expected_rax, expected_rdx) = if bytes == 1 {
                        (
                            merge_gpr(
                                INITIAL_RAX,
                                ((remainder & 0xFF) << 8) | (quotient & 0xFF),
                                2,
                            ),
                            INITIAL_RDX,
                        )
                    } else {
                        (
                            merge_gpr(INITIAL_RAX, quotient, bytes),
                            merge_gpr(INITIAL_RDX, remainder, bytes),
                        )
                    };
                    assert_eq!(vcpu.regs.rax, expected_rax, "quotient");
                    assert_eq!(vcpu.regs.rdx, expected_rdx, "remainder");
                    assert_eq!(vcpu.regs.rip, code.len() as u64);
                    if nf {
                        assert_eq!(vcpu.regs.rflags, INITIAL_RFLAGS, "NF flag image");
                    }
                }
            }
        }
    }
}

fn register_image(vcpu: &X86_64Vcpu) -> serde_json::Value {
    serde_json::to_value(vcpu.get_regs().expect("read materialized x86 registers"))
        .expect("serialize x86 register image")
}

fn system_image(vcpu: &X86_64Vcpu) -> serde_json::Value {
    serde_json::to_value(vcpu.get_sregs().expect("read x86 system registers"))
        .expect("serialize x86 system-register image")
}

#[test]
fn direct_apx_group3_divide_errors_are_precise_and_noncommitting() {
    for group in [6, 7] {
        for bytes in [1, 2, 4, 8] {
            let bits = u32::from(bytes) * 8;
            let width_mask = mask(bytes);
            for nf in [false, true] {
                for zero_divisor in [false, true] {
                    let code = apx_group3_encoding(bytes, group, nf, false);
                    let memory = memory_with_code(&code);
                    let mut vcpu = test_vcpu(memory);

                    if zero_divisor {
                        vcpu.regs.rax = 0x1234;
                        vcpu.regs.rdx = 0;
                        vcpu.regs.rcx = 0;
                    } else if group == 6 {
                        if bytes == 1 {
                            vcpu.regs.rax = merge_gpr(INITIAL_RAX, 0x0100, 2);
                        } else {
                            vcpu.regs.rax = merge_gpr(INITIAL_RAX, 0, bytes);
                            vcpu.regs.rdx = merge_gpr(INITIAL_RDX, 1, bytes);
                        }
                        vcpu.regs.rcx = 1;
                    } else {
                        if bytes == 1 {
                            vcpu.regs.rax = merge_gpr(INITIAL_RAX, 0x8000, 2);
                        } else {
                            vcpu.regs.rax = merge_gpr(INITIAL_RAX, 0, bytes);
                            vcpu.regs.rdx = merge_gpr(INITIAL_RDX, 1_u64 << (bits - 1), bytes);
                        }
                        vcpu.regs.rcx = width_mask;
                    }

                    let before = register_image(&vcpu);
                    let before_system = system_image(&vcpu);
                    let error = match vcpu.step() {
                        Err(error) => format!("{error:#}"),
                        Ok(exit) => panic!(
                            "group=/{group} width={bits} NF={nf} zero={zero_divisor}: expected #DE, got {exit:?}; RAX={:#x} RDX={:#x} RCX={:#x} RIP={:#x}",
                            vcpu.regs.rax, vcpu.regs.rdx, vcpu.regs.rcx, vcpu.regs.rip,
                        ),
                    };
                    assert!(
                        error.contains("IDT entry 0 not present"),
                        "group=/{group} width={bits} NF={nf} zero={zero_divisor}: {error}"
                    );
                    assert_eq!(register_image(&vcpu), before, "register commit");
                    assert_eq!(system_image(&vcpu), before_system, "system-state commit");
                    assert_eq!(vcpu.regs.rip, 0, "faulting RIP");
                }
            }
        }
    }
}
