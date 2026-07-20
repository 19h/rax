//! Direct/native differentials for long-mode indirect far CALL.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const POINTER: u64 = 0x2000;
const GDT: u64 = 0x1000;
const TSS: u64 = 0x1800;

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.cr0 = 0x0005_0033;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs = crate::vm::vcpu::Segment {
        selector: 0,
        type_: 0xB,
        present: true,
        s: true,
        l: true,
        ..crate::vm::vcpu::Segment::default()
    };
    vcpu.sregs.ss = crate::vm::vcpu::Segment {
        selector: 0x20,
        type_: 0x3,
        present: true,
        s: true,
        db: true,
        g: true,
        ..crate::vm::vcpu::Segment::default()
    };
    vcpu.sregs.gdt.base = GDT;
    vcpu.sregs.gdt.limit = 0x7F;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(false);
    vcpu
}

fn code_descriptor(dpl: u8, present: bool, l: bool, db: bool, accessed: bool) -> [u8; 8] {
    let raw = 0xFFFF_u64
        | ((0xA_u64 | u64::from(accessed)) << 40)
        | (1 << 44)
        | (u64::from(dpl & 3) << 45)
        | (u64::from(present) << 47)
        | (u64::from(l) << 53)
        | (u64::from(db) << 54);
    raw.to_le_bytes()
}

fn call_gate(target_selector: u16, target_offset: u64, dpl: u8) -> [u8; 16] {
    let low = (target_offset & 0xFFFF)
        | (u64::from(target_selector) << 16)
        | (0xC << 40)
        | (u64::from(dpl & 3) << 45)
        | (1 << 47)
        | (((target_offset >> 16) & 0xFFFF) << 48);
    let high = (target_offset >> 32) & 0xFFFF_FFFF;
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&low.to_le_bytes());
    bytes[8..].copy_from_slice(&high.to_le_bytes());
    bytes
}

fn install_pointer(memory: &GuestMemoryMmap, offset: u64, selector: u16, width: usize) {
    let mut pointer = offset.to_le_bytes()[..width].to_vec();
    pointer.extend_from_slice(&selector.to_le_bytes());
    memory.write_slice(&pointer, GuestAddress(POINTER)).unwrap();
}

fn install_descriptor(memory: &GuestMemoryMmap, selector: u16, descriptor: &[u8]) {
    memory
        .write_slice(descriptor, GuestAddress(GDT + u64::from(selector >> 3) * 8))
        .unwrap();
}

fn read_u64(memory: &GuestMemoryMmap, address: u64) -> u64 {
    let mut bytes = [0_u8; 8];
    memory
        .read_slice(&mut bytes, GuestAddress(address))
        .unwrap();
    u64::from_le_bytes(bytes)
}

fn segment_fingerprint(
    segment: &crate::vm::vcpu::Segment,
) -> (
    u64,
    u32,
    u16,
    u8,
    bool,
    u8,
    bool,
    bool,
    bool,
    bool,
    bool,
    bool,
) {
    (
        segment.base,
        segment.limit,
        segment.selector,
        segment.type_,
        segment.present,
        segment.dpl,
        segment.db,
        segment.s,
        segment.l,
        segment.g,
        segment.avl,
        segment.unusable,
    )
}

fn exception_without_idt(vcpu: &mut X86_64Vcpu) -> String {
    format!(
        "{:#}",
        vcpu.step()
            .expect_err("exception delivery must fail against the empty test IDT")
    )
}

#[test]
fn jit_far_call_widths_stack_and_apx_egpr_addresses_match_direct() {
    for (name, instruction, width, target, frame_width, source) in [
        (
            "m16:16 RSP",
            &[0x66, 0xFF, 0x1C, 0x24][..],
            2,
            0x5678,
            2_u64,
            4_u8,
        ),
        ("m16:32 RAX", &[0xFF, 0x18], 4, 0x1234_5678, 4, 0),
        (
            "m16:64 R16",
            &[0xD5, 0x18, 0xFF, 0x18],
            8,
            0xFFFF_8000_1234_5678,
            8,
            16,
        ),
    ] {
        let direct_memory = memory_with_code(instruction);
        let native_memory = memory_with_code(instruction);
        let descriptor = code_descriptor(0, true, true, false, false);
        for memory in [&direct_memory, &native_memory] {
            install_pointer(memory, target, 0x10, width);
            install_descriptor(memory, 0x10, &descriptor);
        }
        let mut direct = test_vcpu(direct_memory.clone());
        let mut native = test_vcpu(native_memory.clone());
        for vcpu in [&mut direct, &mut native] {
            vcpu.set_apx_enabled(source == 16);
            match source {
                0 => vcpu.regs.rax = POINTER,
                4 => vcpu.regs.rsp = POINTER,
                16 => vcpu.regs.r16 = POINTER,
                _ => unreachable!(),
            }
            vcpu.regs.rflags &= !flags::bits::AF;
        }

        assert!(direct.step().expect("direct far CALL").is_none(), "{name}");
        let region = native
            .jit_compile_region()
            .expect("compile far-CALL region")
            .unwrap_or_else(|| panic!("{name}: exact far CALL must be native eligible"));
        native.jit_run_region_verified(&region);

        let initial_rsp = if source == 4 { POINTER } else { 0x8000 };
        let expected_rsp = initial_rsp - 2 * frame_width;
        assert_eq!(native.regs.rip, target, "{name}");
        assert_eq!(native.regs.rsp, expected_rsp, "{name}");
        assert_eq!(native.regs.rsp, direct.regs.rsp, "{name}");
        assert_eq!(native.regs.r16, direct.regs.r16, "{name}");
        assert_eq!(
            segment_fingerprint(&native.sregs.cs),
            segment_fingerprint(&direct.sregs.cs),
            "{name}"
        );
        let mask = match frame_width {
            2 => 0xFFFF,
            4 => 0xFFFF_FFFF,
            _ => u64::MAX,
        };
        assert_eq!(
            read_u64(&native_memory, expected_rsp) & mask,
            instruction.len() as u64,
            "{name}: return IP"
        );
        assert_eq!(
            read_u64(&native_memory, expected_rsp + frame_width) & mask,
            0,
            "{name}: return CS"
        );
        let mut descriptor_after = [0_u8; 8];
        native_memory
            .read_slice(&mut descriptor_after, GuestAddress(GDT + 0x10))
            .unwrap();
        assert_eq!(descriptor_after[5] & 1, 1, "{name}");
    }
}

#[test]
fn jit_far_call_same_privilege_gate_ignores_pointer_offset_and_uses_64_bit_frame() {
    let instruction = [0x66, 0xFF, 0x18];
    let target = 0xFFFF_8000_2468_ACE0;
    let gate_selector = 0x18;
    let target_selector = 0x30;
    let direct_memory = memory_with_code(&instruction);
    let native_memory = memory_with_code(&instruction);
    for memory in [&direct_memory, &native_memory] {
        install_pointer(memory, 0xDEAD, gate_selector, 2);
        install_descriptor(
            memory,
            gate_selector,
            &call_gate(target_selector, target, 0),
        );
        install_descriptor(
            memory,
            target_selector,
            &code_descriptor(0, true, true, false, false),
        );
    }
    let mut direct = test_vcpu(direct_memory);
    let mut native = test_vcpu(native_memory.clone());
    direct.regs.rax = POINTER;
    native.regs.rax = POINTER;
    direct.regs.rflags &= !flags::bits::AF;
    native.regs.rflags &= !flags::bits::AF;

    direct.step().unwrap();
    let region = native
        .jit_compile_region()
        .unwrap()
        .expect("call-gate far CALL must be native eligible");
    native.jit_run_region_verified(&region);
    assert_eq!(native.regs.rip, target);
    assert_eq!(native.regs.rsp, 0x7FF0);
    assert_eq!(read_u64(&native_memory, 0x7FF0), 3);
    assert_eq!(read_u64(&native_memory, 0x7FF8), 0);
    assert_eq!(
        segment_fingerprint(&native.sregs.cs),
        segment_fingerprint(&direct.sregs.cs)
    );
}

#[test]
fn jit_far_call_rexw_compatibility_target_loads_eip_with_64_bit_return_frame() {
    let instruction = [0x48, 0xFF, 0x18];
    let pointer_offset = 0xFFFF_FFFF_0000_3456;
    let direct_memory = memory_with_code(&instruction);
    let native_memory = memory_with_code(&instruction);
    let descriptor = code_descriptor(0, true, false, true, false);
    for memory in [&direct_memory, &native_memory] {
        install_pointer(memory, pointer_offset, 0x10, 8);
        install_descriptor(memory, 0x10, &descriptor);
    }
    let mut direct = test_vcpu(direct_memory);
    let mut native = test_vcpu(native_memory.clone());
    for vcpu in [&mut direct, &mut native] {
        vcpu.regs.rax = POINTER;
        vcpu.regs.rflags &= !flags::bits::AF;
    }

    direct.step().expect("direct compatibility transition");
    let region = native
        .jit_compile_region()
        .unwrap()
        .expect("compatibility target remains dynamically native eligible");
    native.jit_run_region_verified(&region);

    assert_eq!(native.regs.rip, 0x3456);
    assert_eq!(native.regs.rsp, 0x7FF0);
    assert!(!native.sregs.cs.l);
    assert!(native.sregs.cs.db);
    assert_eq!(
        segment_fingerprint(&native.sregs.cs),
        segment_fingerprint(&direct.sregs.cs)
    );
    assert_eq!(read_u64(&native_memory, 0x7FF0), instruction.len() as u64);
    assert_eq!(read_u64(&native_memory, 0x7FF8), 0);
}

fn configure_ring3_call_gate(vcpu: &mut X86_64Vcpu) {
    vcpu.sregs.cs.selector = 3;
    vcpu.sregs.cs.dpl = 3;
    vcpu.sregs.ss.selector = 0x23;
    vcpu.sregs.ss.dpl = 3;
    vcpu.sregs.tr = crate::vm::vcpu::Segment {
        base: TSS,
        limit: 0x67,
        selector: 0x28,
        type_: 0xB,
        present: true,
        s: false,
        unusable: false,
        ..crate::vm::vcpu::Segment::default()
    };
}

#[test]
fn native_far_call_privilege_gate_loads_tss_stack_and_commits_full_outer_frame() {
    let instruction = [0x48, 0xFF, 0x18];
    let target = 0xFFFF_8000_1357_9BDF;
    let gate_selector = 0x1B;
    let target_selector = 0x30;
    let direct_memory = memory_with_code(&instruction);
    let native_memory = memory_with_code(&instruction);
    for memory in [&direct_memory, &native_memory] {
        install_pointer(memory, u64::MAX, gate_selector, 8);
        install_descriptor(
            memory,
            gate_selector,
            &call_gate(target_selector, target, 3),
        );
        install_descriptor(
            memory,
            target_selector,
            &code_descriptor(0, true, true, false, true),
        );
        memory
            .write_slice(&0x9000_u64.to_le_bytes(), GuestAddress(TSS + 4))
            .unwrap();
    }
    let mut direct = test_vcpu(direct_memory);
    let mut native = test_vcpu(native_memory.clone());
    for vcpu in [&mut direct, &mut native] {
        configure_ring3_call_gate(vcpu);
        vcpu.regs.rax = POINTER;
        vcpu.regs.rflags &= !flags::bits::AF;
    }

    direct.step().expect("direct privilege far CALL");
    let region = native
        .jit_compile_region()
        .unwrap()
        .expect("privilege far CALL must remain dynamically native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.regs.rip, target);
    assert_eq!(native.regs.rsp, 0x8FE0);
    assert_eq!(native.sregs.cs.selector, target_selector);
    assert_eq!(native.sregs.ss.selector, 0);
    assert_eq!(
        segment_fingerprint(&native.sregs.cs),
        segment_fingerprint(&direct.sregs.cs)
    );
    assert_eq!(
        segment_fingerprint(&native.sregs.ss),
        segment_fingerprint(&direct.sregs.ss)
    );
    assert_eq!(read_u64(&native_memory, 0x8FE0), 3);
    assert_eq!(read_u64(&native_memory, 0x8FE8), 3);
    assert_eq!(read_u64(&native_memory, 0x8FF0), 0x8000);
    assert_eq!(read_u64(&native_memory, 0x8FF8), 0x23);
}

#[test]
fn far_call_fault_deoptimizes_before_direct_np_and_preserves_stack_and_segments() {
    let memory = memory_with_code(&[0x48, 0xFF, 0x18]);
    install_pointer(&memory, 0x1234, 0x10, 8);
    install_descriptor(
        &memory,
        0x10,
        &code_descriptor(0, false, true, false, false),
    );
    let mut vcpu = test_vcpu(memory);
    vcpu.regs.rax = POINTER;
    let before_cs = segment_fingerprint(&vcpu.sregs.cs);
    let before_ss = segment_fingerprint(&vcpu.sregs.ss);
    let before_rsp = vcpu.regs.rsp;
    let region = vcpu
        .jit_compile_region()
        .expect("compile faulting far CALL")
        .expect("dynamic descriptor fault must not block admission");
    vcpu.jit_run_region_native(&region);
    assert_eq!(vcpu.regs.rip, 0);
    assert_eq!(vcpu.regs.rsp, before_rsp);
    assert_eq!(segment_fingerprint(&vcpu.sregs.cs), before_cs);
    assert_eq!(segment_fingerprint(&vcpu.sregs.ss), before_ss);
    let error = exception_without_idt(&mut vcpu);
    assert!(error.contains("IDT entry 11 not present"), "{error}");
}

#[test]
fn native_far_call_replays_stack_fault_before_noncanonical_target_fault() {
    let memory = memory_with_code(&[0x48, 0xFF, 0x18]);
    install_pointer(&memory, 0x0000_8000_0000_1234, 0x10, 8);
    install_descriptor(&memory, 0x10, &code_descriptor(0, true, true, false, false));
    let mut vcpu = test_vcpu(memory.clone());
    vcpu.regs.rax = POINTER;
    vcpu.regs.rsp = 0x0000_8000_0000_0008;
    let before_cs = segment_fingerprint(&vcpu.sregs.cs);
    let region = vcpu
        .jit_compile_region()
        .expect("compile dual-fault far CALL")
        .expect("dynamic stack and target checks remain native eligible");

    vcpu.jit_run_region_native(&region);
    assert_eq!(vcpu.regs.rip, 0);
    assert_eq!(vcpu.regs.rsp, 0x0000_8000_0000_0008);
    assert_eq!(segment_fingerprint(&vcpu.sregs.cs), before_cs);
    let mut descriptor_after = [0_u8; 8];
    memory
        .read_slice(&mut descriptor_after, GuestAddress(GDT + 0x10))
        .unwrap();
    assert_eq!(descriptor_after[5] & 1, 0);

    let error = exception_without_idt(&mut vcpu);
    assert!(error.contains("IDT entry 12 not present"), "{error}");
}

#[test]
fn jit_rejects_far_call_outside_long_code_mode_while_direct_retains_legacy_path() {
    let memory = memory_with_code(&[0xFF, 0x18, 0xF4]);
    install_pointer(&memory, 0x1234, 0x10, 4);
    let mut compatibility = test_vcpu(memory);
    compatibility.sregs.cs.l = false;
    compatibility.sregs.cs.db = true;
    compatibility.regs.rax = POINTER;
    assert!(compatibility.jit_compile_region().unwrap().is_none());
    assert!(compatibility.step().unwrap().is_none());
    assert_eq!(compatibility.regs.rip, 0x1234);
}
