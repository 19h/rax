//! Direct/native differentials for protected IA-32e far RET (`CA`/`CB`).

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const GDT: u64 = 0x1000;
const STACK: u64 = 0x8000;

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
        selector: 0x08,
        type_: 0xB,
        present: true,
        s: true,
        l: true,
        ..crate::vm::vcpu::Segment::default()
    };
    vcpu.sregs.ss = crate::vm::vcpu::Segment {
        selector: 0x10,
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
    vcpu.regs.rsp = STACK;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rflags = 0x2 | 0x08D5 | (1 << 10);
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(false);
    vcpu
}

fn code_descriptor(dpl: u8, present: bool, l: bool, db: bool, accessed: bool) -> [u8; 8] {
    code_descriptor_with_base(0, dpl, present, l, db, accessed)
}

fn code_descriptor_with_base(
    base: u32,
    dpl: u8,
    present: bool,
    l: bool,
    db: bool,
    accessed: bool,
) -> [u8; 8] {
    let raw = 0xFFFF_u64
        | (u64::from(base & 0xFFFF) << 16)
        | (u64::from((base >> 16) & 0xFF) << 32)
        | ((0xA_u64 | u64::from(accessed)) << 40)
        | (1 << 44)
        | (u64::from(dpl & 3) << 45)
        | (u64::from(present) << 47)
        | (u64::from(l) << 53)
        | (u64::from(db) << 54)
        | (u64::from(base >> 24) << 56);
    raw.to_le_bytes()
}

fn stack_descriptor(dpl: u8, present: bool, db: bool, accessed: bool) -> [u8; 8] {
    let raw = 0xFFFF_u64
        | ((0x2_u64 | u64::from(accessed)) << 40)
        | (1 << 44)
        | (u64::from(dpl & 3) << 45)
        | (u64::from(present) << 47)
        | (u64::from(db) << 54);
    raw.to_le_bytes()
}

fn install_descriptor(memory: &GuestMemoryMmap, selector: u16, descriptor: &[u8]) {
    memory
        .write_slice(descriptor, GuestAddress(GDT + u64::from(selector >> 3) * 8))
        .unwrap();
}

fn write_slot(memory: &GuestMemoryMmap, address: u64, width: usize, value: u64) {
    memory
        .write_slice(&value.to_le_bytes()[..width], GuestAddress(address))
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
fn jit_far_return_widths_immediate_and_apx_match_direct() {
    const TARGET_CS_BASE: u64 = 0x000B_0000;

    for (name, instruction, width, pop_bytes, target, apx) in [
        ("W16", &[0x66, 0xCB][..], 2, 0_u16, 0x5678_u64, false),
        (
            "W32 imm16",
            &[0xCA, 0x10, 0x00],
            4,
            0x10,
            0x1234_5678,
            false,
        ),
        (
            "REX2.W",
            &[0xD5, 0x18, 0xCB],
            8,
            0,
            0xFFFF_8000_1234_5678,
            true,
        ),
    ] {
        let direct_memory = memory_with_code(instruction);
        let native_memory = memory_with_code(instruction);
        for memory in [&direct_memory, &native_memory] {
            install_descriptor(
                memory,
                0x18,
                &code_descriptor_with_base(TARGET_CS_BASE as u32, 0, true, true, false, false),
            );
            write_slot(memory, STACK, width, target);
            write_slot(memory, STACK + width as u64, width, 0x18);
        }
        let mut direct = test_vcpu(direct_memory);
        let mut native = test_vcpu(native_memory.clone());
        for vcpu in [&mut direct, &mut native] {
            vcpu.set_apx_enabled(apx);
            vcpu.regs.rflags &= !flags::bits::AF;
        }

        assert!(direct.step().expect("direct far RET").is_none(), "{name}");
        let region = native
            .jit_compile_region()
            .expect("compile far-RET region")
            .unwrap_or_else(|| panic!("{name}: exact far RET must be native eligible"));
        native.jit_run_region_verified(&region);

        assert_eq!(native.regs.rip, target, "{name}");
        assert_eq!(
            native.regs.rsp,
            STACK + 2 * width as u64 + u64::from(pop_bytes),
            "{name}"
        );
        assert_eq!(native.regs.rsp, direct.regs.rsp, "{name}");
        assert_eq!(native.sregs.cs.base, TARGET_CS_BASE, "{name}");
        assert_eq!(
            segment_fingerprint(&native.sregs.cs),
            segment_fingerprint(&direct.sregs.cs),
            "{name}"
        );
        assert_eq!(read_u64(&native_memory, GDT + 0x18) & (1 << 40), 1 << 40);
    }
}

#[test]
fn native_far_return_outer_privilege_commits_stack_and_segment_invalidation() {
    let instruction = [0x48, 0xCA, 0x10, 0x00];
    let target = 0xFFFF_8000_2468_ACE0;
    let direct_memory = memory_with_code(&instruction);
    let native_memory = memory_with_code(&instruction);
    for memory in [&direct_memory, &native_memory] {
        install_descriptor(memory, 0x1B, &code_descriptor(3, true, true, false, false));
        install_descriptor(memory, 0x23, &stack_descriptor(3, true, true, false));
        write_slot(memory, STACK, 8, target);
        write_slot(memory, STACK + 8, 8, 0x1B);
        write_slot(memory, STACK + 32, 8, 0x9000);
        write_slot(memory, STACK + 40, 8, 0x23);
    }
    let mut direct = test_vcpu(direct_memory);
    let mut native = test_vcpu(native_memory.clone());
    for vcpu in [&mut direct, &mut native] {
        vcpu.sregs.ds = crate::vm::vcpu::Segment {
            selector: 0x28,
            type_: 0x3,
            present: true,
            dpl: 0,
            s: true,
            ..crate::vm::vcpu::Segment::default()
        };
        vcpu.sregs.fs = crate::vm::vcpu::Segment {
            base: 0xAAAA_BBBB_CCCC_DDDD,
            selector: 0x30,
            type_: 0xE,
            present: true,
            dpl: 0,
            s: true,
            ..crate::vm::vcpu::Segment::default()
        };
        vcpu.regs.rflags &= !flags::bits::AF;
    }

    direct.step().expect("direct outer far RET");
    let region = native
        .jit_compile_region()
        .unwrap()
        .expect("outer far RET must remain dynamically native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(native.regs.rip, target);
    assert_eq!(native.regs.rsp, 0x9010);
    assert_eq!(native.sregs.cs.selector, 0x1B);
    assert_eq!(native.sregs.ss.selector, 0x23);
    assert_eq!(native.sregs.ds.selector, 0);
    assert!(native.sregs.ds.unusable);
    assert_eq!(native.sregs.fs.selector, 0x30);
    assert_eq!(native.sregs.fs.base, 0xAAAA_BBBB_CCCC_DDDD);
    assert_eq!(
        segment_fingerprint(&native.sregs.cs),
        segment_fingerprint(&direct.sregs.cs)
    );
    assert_eq!(
        segment_fingerprint(&native.sregs.ss),
        segment_fingerprint(&direct.sregs.ss)
    );
    assert_eq!(read_u64(&native_memory, GDT + 0x18) & (1 << 40), 1 << 40);
    assert_eq!(read_u64(&native_memory, GDT + 0x20) & (1 << 40), 1 << 40);
}

#[test]
fn far_return_fault_deoptimizes_before_direct_np_and_preserves_state() {
    let memory = memory_with_code(&[0x48, 0xCB]);
    install_descriptor(
        &memory,
        0x18,
        &code_descriptor(0, false, true, false, false),
    );
    write_slot(&memory, STACK, 8, 0x1234);
    write_slot(&memory, STACK + 8, 8, 0x18);
    let mut vcpu = test_vcpu(memory.clone());
    let before_cs = segment_fingerprint(&vcpu.sregs.cs);
    let before_ss = segment_fingerprint(&vcpu.sregs.ss);
    let before_rsp = vcpu.regs.rsp;
    let region = vcpu
        .jit_compile_region()
        .expect("compile faulting far RET")
        .expect("dynamic descriptor fault must not block admission");
    vcpu.jit_run_region_native(&region);
    assert_eq!(vcpu.regs.rip, 0);
    assert_eq!(vcpu.regs.rsp, before_rsp);
    assert_eq!(segment_fingerprint(&vcpu.sregs.cs), before_cs);
    assert_eq!(segment_fingerprint(&vcpu.sregs.ss), before_ss);
    assert_eq!(read_u64(&memory, GDT + 0x18) & (1 << 40), 0);
    let error = exception_without_idt(&mut vcpu);
    assert!(error.contains("IDT entry 11 not present"), "{error}");
}

#[test]
fn far_return_verifier_deopts_descriptor_writes_but_accepts_preaccessed_outer_return() {
    let target = 0xFFFF_8000_2468_ACE0;
    for accessed in [false, true] {
        let memory = memory_with_code(&[0x48, 0xCB]);
        install_descriptor(
            &memory,
            0x1B,
            &code_descriptor(3, true, true, false, accessed),
        );
        install_descriptor(&memory, 0x23, &stack_descriptor(3, true, true, accessed));
        write_slot(&memory, STACK, 8, target);
        write_slot(&memory, STACK + 8, 8, 0x1B);
        write_slot(&memory, STACK + 16, 8, 0x9000);
        write_slot(&memory, STACK + 24, 8, 0x23);
        let mut vcpu = test_vcpu(memory.clone());
        vcpu.regs.rflags &= !flags::bits::AF;
        let region = vcpu
            .jit_compile_region()
            .unwrap()
            .expect("outer far RET must be admitted independently of descriptors");
        vcpu.jit_run_region_verified(&region);

        if accessed {
            assert_eq!(vcpu.regs.rip, target);
            assert_eq!(vcpu.regs.rsp, 0x9000);
            assert_eq!(vcpu.sregs.cs.selector, 0x1B);
            assert_eq!(vcpu.sregs.ss.selector, 0x23);
        } else {
            assert_eq!(vcpu.regs.rip, 0, "write-bearing verifier run must deopt");
            assert_eq!(vcpu.regs.rsp, STACK);
            assert_eq!(vcpu.sregs.cs.selector, 0x08);
            assert_eq!(vcpu.sregs.ss.selector, 0x10);
            assert_eq!(read_u64(&memory, GDT + 0x18) & (1 << 40), 0);
            assert_eq!(read_u64(&memory, GDT + 0x20) & (1 << 40), 0);
            vcpu.step()
                .expect("direct replay after verifier deoptimization");
            assert_eq!(vcpu.regs.rip, target);
        }
    }
}

#[test]
fn jit_rejects_far_return_outside_long_code_mode_while_direct_retains_legacy_path() {
    let memory = memory_with_code(&[0xCB, 0xF4]);
    write_slot(&memory, STACK, 4, 0x1234);
    write_slot(&memory, STACK + 4, 4, 0x08);
    let mut compatibility = test_vcpu(memory);
    compatibility.sregs.cs.l = false;
    compatibility.sregs.cs.db = true;
    assert!(compatibility.jit_compile_region().unwrap().is_none());
    assert!(compatibility.step().unwrap().is_none());
    assert_eq!(compatibility.regs.rip, 0x1234);
}
