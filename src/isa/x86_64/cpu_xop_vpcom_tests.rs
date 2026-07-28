//! CPU-level execution and fault contracts for AMD XOP VPCOM.

use super::xop_tests::{
    CR0_AM, CR0_PE, CR0_TS, CR4_OSXSAVE, DATA, assert_fault_noncommitting, assert_registers_equal,
    memory_with_code, seed_architectural_state, test_vcpu, xop,
};
use super::*;
use crate::isa::x86_64::flags;
use vm_memory::{Bytes, GuestAddress};

const OPCODES: &[(u8, usize, bool)] = &[
    (0xCC, 1, true),
    (0xCD, 2, true),
    (0xCE, 4, true),
    (0xCF, 8, true),
    (0xEC, 1, false),
    (0xED, 2, false),
    (0xEE, 4, false),
    (0xEF, 8, false),
];

fn lane(bytes: &[u8; 16], offset: usize, element_bytes: usize) -> u64 {
    let mut lane = [0_u8; 8];
    lane[..element_bytes].copy_from_slice(&bytes[offset..offset + element_bytes]);
    u64::from_le_bytes(lane)
}

fn signed(value: u64, bits: u32) -> i64 {
    if bits == 64 {
        value as i64
    } else {
        let shift = 64 - bits;
        ((value << shift) as i64) >> shift
    }
}

fn words_to_bytes(words: [u64; 2]) -> [u8; 16] {
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&words[0].to_le_bytes());
    bytes[8..].copy_from_slice(&words[1].to_le_bytes());
    bytes
}

fn bytes_to_words(bytes: [u8; 16]) -> [u64; 2] {
    [
        u64::from_le_bytes(bytes[..8].try_into().unwrap()),
        u64::from_le_bytes(bytes[8..].try_into().unwrap()),
    ]
}

/// AMD APM Vol. 4 predicate-table reference, independent of the executor.
fn reference(
    source1: [u64; 2],
    source2: [u64; 2],
    element_bytes: usize,
    signed_elements: bool,
    immediate: u8,
) -> [u64; 2] {
    let source1 = words_to_bytes(source1);
    let source2 = words_to_bytes(source2);
    let predicate = immediate & 7;
    let bits = (element_bytes * 8) as u32;
    let mut output = [0_u8; 16];
    for offset in (0..16).step_by(element_bytes) {
        let left = lane(&source1, offset, element_bytes);
        let right = lane(&source2, offset, element_bytes);
        let value = match predicate {
            0 if signed_elements => signed(left, bits) < signed(right, bits),
            1 if signed_elements => signed(left, bits) <= signed(right, bits),
            2 if signed_elements => signed(left, bits) > signed(right, bits),
            3 if signed_elements => signed(left, bits) >= signed(right, bits),
            0 => left < right,
            1 => left <= right,
            2 => left > right,
            3 => left >= right,
            4 => left == right,
            5 => left != right,
            6 => false,
            7 => true,
            _ => unreachable!(),
        };
        output[offset..offset + element_bytes].fill(if value { 0xFF } else { 0 });
    }
    bytes_to_words(output)
}

#[test]
fn direct_vpcom_executes_every_element_family_and_immediate_image() {
    let mut code = Vec::new();
    for &(opcode, _, _) in OPCODES {
        for immediate in 0..=u8::MAX {
            // VPCOM* XMM3,XMM2,XMM1,imm8.
            code.extend_from_slice(&xop(8, false, false, 0, 2, opcode, &[0xD9, immediate]));
        }
    }
    let mut vcpu = test_vcpu(memory_with_code(&code), false);
    seed_architectural_state(&mut vcpu);
    let source1 = vcpu.regs.xmm[2];
    let source2 = vcpu.regs.xmm[1];
    let rflags = vcpu.regs.rflags;
    let mxcsr = vcpu.mxcsr;
    let mut instruction = 0_u64;
    for &(opcode, element_bytes, signed_elements) in OPCODES {
        for immediate in 0..=u8::MAX {
            let expected = reference(source1, source2, element_bytes, signed_elements, immediate);
            assert!(
                vcpu.step().expect("direct VPCOM").is_none(),
                "opcode={opcode:#04x}, imm={immediate:#04x}"
            );
            instruction += 1;
            assert_eq!(
                vcpu.regs.xmm[3], expected,
                "opcode={opcode:#04x}, imm={immediate:#04x}"
            );
            assert_eq!(vcpu.regs.ymm_high[3], [0; 2]);
            assert_eq!(vcpu.regs.zmm_high[3], [0; 4]);
            assert_eq!(vcpu.regs.rflags, rflags);
            assert_eq!(vcpu.mxcsr, mxcsr);
            assert_eq!(vcpu.regs.rip, instruction * 6);
        }
    }
}

#[test]
fn direct_vpcom_preserves_all_destination_source_aliases() {
    for &(opcode, element_bytes, signed_elements) in OPCODES {
        for immediate in 0..8 {
            for (destination, source1, source2) in [(2, 2, 1), (1, 2, 1), (3, 3, 3)] {
                let modrm = 0xC0 | (destination << 3) | source2;
                let code = xop(8, false, false, 0, source1, opcode, &[modrm, immediate]);
                let mut vcpu = test_vcpu(memory_with_code(&code), false);
                seed_architectural_state(&mut vcpu);
                let before = vcpu.regs.clone();
                let expected = reference(
                    before.xmm[usize::from(source1)],
                    before.xmm[usize::from(source2)],
                    element_bytes,
                    signed_elements,
                    immediate,
                );
                assert!(vcpu.step().expect("aliased VPCOM").is_none());
                assert_eq!(vcpu.regs.xmm[usize::from(destination)], expected);
                assert_eq!(vcpu.regs.ymm_high[usize::from(destination)], [0; 2]);
                assert_eq!(vcpu.regs.zmm_high[usize::from(destination)], [0; 4]);
            }
        }
    }
}

#[test]
fn direct_vpcom_memory_is_one_aligned_full_width_read_with_exact_rip() {
    let source = [0x807F_FF00_0123_FEDC, 0x8000_7FFF_FFFF_0001];
    let bytes = words_to_bytes(source);
    for &(opcode, element_bytes, signed_elements) in OPCODES {
        // VPCOM* XMM1,XMM2,[RIP+disp32],0xA3.
        let mut code = xop(8, false, false, 0, 2, opcode, &[0x0D]);
        let instruction_len = 10_u64;
        let displacement = DATA.wrapping_sub(instruction_len) as u32;
        code.extend_from_slice(&displacement.to_le_bytes());
        code.push(0xA3);
        let memory = memory_with_code(&code);
        memory.write_slice(&bytes, GuestAddress(DATA)).unwrap();
        let mut vcpu = test_vcpu(memory, false);
        seed_architectural_state(&mut vcpu);
        let expected = reference(
            vcpu.regs.xmm[2],
            source,
            element_bytes,
            signed_elements,
            0xA3,
        );
        assert!(vcpu.step().expect("RIP-relative VPCOM").is_none());
        assert_eq!(vcpu.regs.xmm[1], expected);
        assert_eq!(vcpu.regs.rip, instruction_len);
    }

    let code = xop(8, false, false, 0, 2, 0xCC, &[0x0B, 0xA5]);
    let mut alignment = test_vcpu(memory_with_code(&code), false);
    seed_architectural_state(&mut alignment);
    alignment.regs.rbx = 0x20_001;
    alignment.sregs.cr0 |= CR0_AM;
    alignment.sregs.cs.selector = 3;
    alignment.regs.rflags |= flags::bits::AC;
    assert_fault_noncommitting(&mut alignment, 17, "VPCOM #AC precedes #PF");

    let mut range = test_vcpu(memory_with_code(&code), false);
    seed_architectural_state(&mut range);
    range.regs.rbx = 0x0000_7FFF_FFFF_FFF8;
    assert_fault_noncommitting(&mut range, 13, "VPCOM canonical range crossing");

    let stack_code = xop(8, false, false, 0, 2, 0xCC, &[0x0C, 0x24, 0xA5]);
    let mut stack = test_vcpu(memory_with_code(&stack_code), false);
    seed_architectural_state(&mut stack);
    stack.regs.rsp = 0x0000_8000_0000_0000;
    assert_fault_noncommitting(&mut stack, 12, "VPCOM noncanonical stack address");
}

#[test]
fn direct_vpcom_reserved_and_dynamic_faults_precede_nm_and_memory() {
    let memory_tail = [0x0B, 0xA5];
    let mut forbidden = vec![0x66];
    forbidden.extend_from_slice(&xop(8, false, false, 0, 2, 0xCC, &memory_tail));
    for (name, code) in [
        ("W=1", xop(8, true, false, 0, 2, 0xCC, &memory_tail)),
        ("L=1", xop(8, false, true, 0, 2, 0xCC, &memory_tail)),
        ("pp=01", xop(8, false, false, 1, 2, 0xCC, &memory_tail)),
        ("legacy 66", forbidden),
    ] {
        let mut vcpu = test_vcpu(memory_with_code(&code), false);
        seed_architectural_state(&mut vcpu);
        vcpu.sregs.cr0 |= CR0_TS;
        vcpu.regs.rbx = 0x20_000;
        assert_fault_noncommitting(&mut vcpu, 6, name);
    }

    for case in 0..6 {
        let code = xop(8, false, false, 0, 2, 0xCC, &memory_tail);
        let mut vcpu = test_vcpu(memory_with_code(&code), false);
        seed_architectural_state(&mut vcpu);
        vcpu.regs.rbx = 0x20_000;
        vcpu.sregs.cr0 |= CR0_TS;
        let (vector, name) = match case {
            0 => {
                vcpu.set_xop_enabled(false);
                (6, "CPUID.XOP=0")
            }
            1 => {
                vcpu.sregs.cr4 &= !CR4_OSXSAVE;
                (6, "CR4.OSXSAVE=0")
            }
            2 => {
                vcpu.xcr0 &= !0b100;
                (6, "XCR0.YMM=0")
            }
            3 => {
                vcpu.sregs.cr0 &= !CR0_PE;
                (6, "CR0.PE=0")
            }
            4 => {
                vcpu.regs.rflags |= flags::bits::VM;
                (6, "RFLAGS.VM=1")
            }
            5 => (7, "CR0.TS=1"),
            _ => unreachable!(),
        };
        assert_fault_noncommitting(&mut vcpu, vector, name);
    }
}

#[test]
fn direct_vpcom_compatibility_mode_honors_xop_register_restrictions() {
    let code = xop(8, false, false, 0, 2, 0xCC, &[0xD9, 0xA5]);
    let mut vcpu = test_vcpu(memory_with_code(&code), false);
    seed_architectural_state(&mut vcpu);
    vcpu.sregs.cs.l = false;
    let expected = reference(vcpu.regs.xmm[2], vcpu.regs.xmm[1], 1, true, 0xA5);
    assert!(vcpu.step().expect("compatibility VPCOM").is_none());
    assert_eq!(vcpu.regs.xmm[3], expected);

    for (name, code) in [
        ("R=0", [0x8F, 0x68, 0x68, 0xCC, 0xD9, 0xA5]),
        ("X=0", [0x8F, 0xA8, 0x68, 0xCC, 0xD9, 0xA5]),
        ("vvvv=8", [0x8F, 0xE8, 0xB8, 0xCC, 0xD9, 0xA5]),
        ("W=1", [0x8F, 0xE8, 0xE8, 0xCC, 0xD9, 0xA5]),
    ] {
        let mut vcpu = test_vcpu(memory_with_code(&code), false);
        seed_architectural_state(&mut vcpu);
        vcpu.sregs.cs.l = false;
        assert_fault_noncommitting(&mut vcpu, 6, name);
    }

    // XOP.B is ignored outside 64-bit mode, so encoded B=0 still selects XMM1.
    let b_ignored = [0x8F, 0xC8, 0x68, 0xCC, 0xD9, 0xA5];
    let mut vcpu = test_vcpu(memory_with_code(&b_ignored), false);
    seed_architectural_state(&mut vcpu);
    vcpu.sregs.cs.l = false;
    let expected = reference(vcpu.regs.xmm[2], vcpu.regs.xmm[1], 1, true, 0xA5);
    assert!(vcpu.step().expect("compatibility XOP.B ignored").is_none());
    assert_eq!(vcpu.regs.xmm[3], expected);
}

#[test]
fn direct_vpcom_feature_enabled_memory_fault_is_noncommitting() {
    let code = xop(8, false, false, 0, 2, 0xCC, &[0x0B, 0xA5]);
    let mut vcpu = test_vcpu(memory_with_code(&code), false);
    seed_architectural_state(&mut vcpu);
    vcpu.regs.rbx = 0x20_000;
    let before = vcpu.regs.clone();
    assert!(vcpu.step().is_err(), "enabled VPCOM must reach memory");
    assert_registers_equal(&vcpu.regs, &before, "faulting VPCOM read");
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn run_direct_to(vcpu: &mut X86_64Vcpu, target: u64) {
    for _ in 0..32 {
        if vcpu.regs.rip == target {
            return;
        }
        assert!(
            vcpu.step().expect("direct VPCOM sequence").is_none(),
            "unexpected direct exit at {:#x}",
            vcpu.regs.rip
        );
    }
    panic!("direct VPCOM execution did not reach {target:#x}");
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn jit_vpcom_register_matrix_matches_direct_for_every_family_predicate_and_alias() {
    for &(opcode, _, _) in OPCODES {
        for immediate in 0..8 {
            for (destination, source1, source2) in [(1, 2, 3), (2, 2, 3), (3, 2, 3)] {
                let modrm = 0xC0 | (destination << 3) | source2;
                let mut code = xop(
                    8,
                    false,
                    false,
                    0,
                    source1,
                    opcode,
                    &[modrm, 0xA0 | immediate],
                );
                code.extend_from_slice(&[0xEB, 0x00, 0xF4]);
                let mut direct = test_vcpu(memory_with_code(&code), false);
                let mut native = test_vcpu(memory_with_code(&code), false);
                seed_architectural_state(&mut direct);
                seed_architectural_state(&mut native);
                let frontier = code.len() as u64 - 1;

                run_direct_to(&mut direct, frontier);
                let region = native
                    .jit_compile_region()
                    .unwrap_or_else(|error| {
                        panic!(
                            "opcode={opcode:#04x}, immediate={immediate}, alias=({destination},{source1},{source2}): {error:?}"
                        )
                    })
                    .expect("register VPCOM must be native eligible");
                assert!(!region.uses_vector);
                assert!(region.uses_xmm_state);
                native.jit_run_region_native(&region);

                assert_registers_equal(
                    &native.regs,
                    &direct.regs,
                    &format!(
                        "register opcode={opcode:#04x}, immediate={immediate}, alias=({destination},{source1},{source2})"
                    ),
                );
                assert_eq!(native.mxcsr, direct.mxcsr);
            }
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn jit_vpcom_memory_matrix_matches_direct_and_constant_predicates_still_fault() {
    let memory_value = [0x807F_FF00_0123_FEDC, 0x8000_7FFF_FFFF_0001];
    for &(opcode, _, _) in OPCODES {
        for immediate in 0..8 {
            let mut code = xop(8, false, false, 0, 2, opcode, &[0x0B, 0xF0 | immediate]);
            code.extend_from_slice(&[0xEB, 0x00, 0xF4]);
            let direct_memory = memory_with_code(&code);
            let native_memory = memory_with_code(&code);
            for memory in [&direct_memory, &native_memory] {
                memory
                    .write_slice(&words_to_bytes(memory_value), GuestAddress(DATA))
                    .unwrap();
            }
            let mut direct = test_vcpu(direct_memory, true);
            let mut native = test_vcpu(native_memory, true);
            seed_architectural_state(&mut direct);
            seed_architectural_state(&mut native);
            let frontier = code.len() as u64 - 1;

            run_direct_to(&mut direct, frontier);
            let region = native
                .jit_compile_region()
                .unwrap_or_else(|error| {
                    panic!("opcode={opcode:#04x}, immediate={immediate}: {error:?}")
                })
                .expect("helper-backed VPCOM must be native eligible");
            assert!(!region.uses_vector);
            assert!(region.uses_xmm_state);
            native.jit_run_region_native(&region);

            assert_registers_equal(
                &native.regs,
                &direct.regs,
                &format!("memory opcode={opcode:#04x}, immediate={immediate}"),
            );
            assert_eq!(native.mxcsr, direct.mxcsr);
        }
    }

    for immediate in [6, 7] {
        let mut code = xop(8, false, false, 0, 2, 0xCC, &[0x0B, immediate]);
        code.extend_from_slice(&[0xEB, 0x00, 0xF4]);
        let mut native = test_vcpu(memory_with_code(&code), true);
        seed_architectural_state(&mut native);
        native.regs.rbx = 0x20_000;
        let region = native
            .jit_compile_region()
            .expect("compile constant-predicate memory VPCOM")
            .expect("constant-predicate memory VPCOM must be native eligible");
        let before = native.regs.clone();
        native.jit_run_region_native(&region);
        assert_registers_equal(
            &native.regs,
            &before,
            "faulting constant-predicate native VPCOM",
        );
        assert!(
            native.step().is_err(),
            "predicate {immediate} must replay the faulting memory access"
        );
        assert_registers_equal(
            &native.regs,
            &before,
            "faulting constant-predicate direct replay",
        );
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn jit_vpcom_synchronizes_both_sides_of_a_mixed_physical_vector_region() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping mixed VPCOM synchronization: host lacks AVX");
        return;
    }

    // VRCPPS updates source 1 in physical state, VPCOM reads it through the
    // state image, and the second VRCPPS consumes the reloaded VPCOM result.
    let mut code = vec![0xC5, 0xF8, 0x53, 0xD5];
    code.extend_from_slice(&xop(8, false, false, 0, 2, 0xCC, &[0xCB, 0xA1]));
    code.extend_from_slice(&[0xC5, 0xF8, 0x53, 0xF1, 0xEB, 0x00, 0xF4]);
    let mut direct = test_vcpu(memory_with_code(&code), false);
    let mut native = test_vcpu(memory_with_code(&code), false);
    seed_architectural_state(&mut direct);
    seed_architectural_state(&mut native);
    let frontier = code.len() as u64 - 1;

    run_direct_to(&mut direct, frontier);
    let region = native
        .jit_compile_region()
        .expect("compile mixed physical/state-backed VPCOM region")
        .expect("mixed VPCOM region must be native eligible");
    assert!(region.uses_vector);
    assert!(region.uses_xmm_state);
    assert!(region.avx_ymm16_vector_state);
    native.jit_run_region_native(&region);

    assert_registers_equal(&native.regs, &direct.regs, "mixed VPCOM region");
    assert_eq!(native.mxcsr, direct.mxcsr);
}
