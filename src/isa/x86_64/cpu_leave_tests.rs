//! Direct x86 `LEAVE` width, prefix, and precise fault-frontier coverage.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const CR0_PE: u64 = 1;
const CR0_AM: u64 = 1 << 18;
const EFER_LMA: u64 = 1 << 10;

fn memory_with_ranges(code: &[u8], ranges: &[(GuestAddress, usize)]) -> Arc<GuestMemoryMmap> {
    let memory = Arc::new(GuestMemoryMmap::<()>::from_ranges(ranges).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    memory_with_ranges(code, &[(GuestAddress(0), 0x1_0000)])
}

fn long_mode_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.cr0 = CR0_PE;
    vcpu.sregs.efer = EFER_LMA;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
    vcpu.regs.rip = 0;
    vcpu.regs.rax = 0x0123_4567_89AB_CDEF;
    vcpu.regs.rcx = 0x1111_2222_3333_4444;
    vcpu.regs.rdx = 0x5555_6666_7777_8888;
    vcpu.regs.rbx = 0xBBBB_BBBB_BBBB_BBBB;
    vcpu.regs.rsp = 0x9000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rsi = 0x9999_AAAA_BBBB_CCCC;
    vcpu.regs.rdi = 0xDDDD_EEEE_FFFF_0000;
    vcpu.regs.r8 = 0x0808_0808_0808_0808;
    vcpu.regs.r15 = 0x1515_1515_1515_1515;
    vcpu.regs.r31 = 0x3131_3131_3131_3131;
    vcpu.regs.rflags = 0x2 | 0x08D5 | flags::bits::DF;
    vcpu
}

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

#[test]
fn direct_leave_obeys_long_mode_width_prefix_order_and_preserves_state() {
    for (name, instruction, width) in [
        ("default W64", &[0xC9][..], 8_u8),
        ("66 selects W16", &[0x66, 0xC9], 2),
        ("66 then REX.W selects W64", &[0x66, 0x48, 0xC9], 8),
        ("REX.W then 66 selects W16", &[0x48, 0x66, 0xC9], 2),
        ("67 does not narrow the stack", &[0x67, 0xC9], 8),
    ] {
        let memory = memory_with_code(instruction);
        let saved_rbp = 0xA1B2_C3D4_E5F6_BEEF_u64;
        memory
            .write_slice(
                &saved_rbp.to_le_bytes()[..usize::from(width)],
                GuestAddress(0x7000),
            )
            .unwrap();
        let mut vcpu = long_mode_vcpu(memory);
        let before = vcpu.regs.clone();

        assert!(
            vcpu.step()
                .unwrap_or_else(|error| panic!("{name}: {error:#}"))
                .is_none()
        );
        assert_eq!(vcpu.regs.rip, instruction.len() as u64, "{name}: RIP");
        assert_eq!(vcpu.regs.rsp, 0x7000 + u64::from(width), "{name}: RSP");
        assert_eq!(
            vcpu.regs.rbp,
            if width == 2 {
                (before.rbp & !0xFFFF) | (saved_rbp & 0xFFFF)
            } else {
                saved_rbp
            },
            "{name}: RBP"
        );
        for index in (0..32).filter(|index| !matches!(index, 4 | 5)) {
            assert_eq!(
                gprs(&vcpu.regs)[index],
                gprs(&before)[index],
                "{name}: GPR {index}"
            );
        }
        assert_eq!(vcpu.regs.rflags, before.rflags, "{name}: RFLAGS");
    }
}

#[test]
fn direct_leave16_uses_full_rbp_address_and_merges_only_bp() {
    const FRAME: u64 = 0x0000_0001_0000_7000;
    let memory = memory_with_ranges(
        &[0x66, 0xC9],
        &[(GuestAddress(0), 0x1000), (GuestAddress(FRAME), 0x1000)],
    );
    memory.write_obj(0xBEEF_u16, GuestAddress(FRAME)).unwrap();
    let mut vcpu = long_mode_vcpu(memory);
    vcpu.regs.rbp = FRAME;

    assert!(vcpu.step().expect("direct W16 LEAVE").is_none());
    assert_eq!(vcpu.regs.rsp, FRAME + 2);
    assert_eq!(vcpu.regs.rbp, 0x0000_0001_0000_BEEF);
}

#[test]
fn direct_leave_pop_fault_is_noncommitting() {
    let memory = memory_with_ranges(
        &[0xC9],
        &[(GuestAddress(0), 0x100), (GuestAddress(0x700), 0x100)],
    );
    let mut vcpu = long_mode_vcpu(memory);
    vcpu.regs.rsp = 0x800;
    vcpu.regs.rbp = 0x600;
    let before = vcpu.regs.clone();

    assert!(vcpu.step().is_err(), "unmapped saved RBP must fault");
    assert_eq!(vcpu.regs.rsp, before.rsp);
    assert_eq!(vcpu.regs.rbp, before.rbp);
    assert_eq!(vcpu.regs.rip, before.rip);
    assert_eq!(vcpu.regs.rflags, before.rflags);
}

#[test]
fn direct_leave_ss_and_ac_faults_are_noncommitting() {
    for (name, frame, configure, vector) in [
        (
            "noncanonical stack address",
            0x0000_8000_0000_0000,
            (|_: &mut X86_64Vcpu| {}) as fn(&mut X86_64Vcpu),
            12,
        ),
        (
            "stack access crosses the canonical boundary",
            0x0000_7FFF_FFFF_FFFC,
            |_: &mut X86_64Vcpu| {},
            12,
        ),
        (
            "unaligned user stack",
            0x7001,
            |vcpu: &mut X86_64Vcpu| {
                vcpu.sregs.cr0 |= CR0_AM;
                vcpu.sregs.cs.selector = 3;
                vcpu.regs.rflags |= flags::bits::AC;
            },
            17,
        ),
    ] {
        let mut vcpu = long_mode_vcpu(memory_with_code(&[0xC9]));
        vcpu.regs.rbp = frame;
        configure(&mut vcpu);
        let before = vcpu.regs.clone();

        let error = format!("{:#}", vcpu.step().expect_err("LEAVE must fault"));
        assert!(
            error.contains(&format!("IDT entry {vector} not present")),
            "{name}: {error}"
        );
        assert_eq!(vcpu.regs.rsp, before.rsp, "{name}: unchanged RSP");
        assert_eq!(vcpu.regs.rbp, frame, "{name}: unchanged RBP");
        assert_eq!(vcpu.regs.rip, before.rip, "{name}: fault PC");
    }
}

#[test]
fn direct_rex2_leave_checks_apx_before_any_stack_commit() {
    let instruction = [0xD5, 0x00, 0xC9];
    let memory = memory_with_code(&instruction);
    memory
        .write_obj(0xCAFE_BABE_0123_4567_u64, GuestAddress(0x7000))
        .unwrap();
    let mut disabled = long_mode_vcpu(memory.clone());
    let before = disabled.regs.clone();

    let error = format!(
        "{:#}",
        disabled.step().expect_err("APX-disabled REX2 LEAVE")
    );
    assert!(error.contains("IDT entry 6 not present"), "{error}");
    assert_eq!(gprs(&disabled.regs), gprs(&before));
    assert_eq!(disabled.regs.rip, before.rip);

    let mut enabled = long_mode_vcpu(memory);
    enabled.set_apx_enabled(true);
    assert!(enabled.step().expect("APX-enabled REX2 LEAVE").is_none());
    assert_eq!(enabled.regs.rsp, 0x7008);
    assert_eq!(enabled.regs.rbp, 0xCAFE_BABE_0123_4567);
    assert_eq!(enabled.regs.rip, instruction.len() as u64);
}
