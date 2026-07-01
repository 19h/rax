//! TSX instruction behavior without transactional state.

use crate::common::{run_until_hlt, setup_vm, setup_vm_no_idt};
use rax::cpu::VCpu;

fn assert_missing_idt_ud(code: &[u8]) {
    let (mut vcpu, _) = setup_vm_no_idt(code, None);
    let err = run_until_hlt(&mut vcpu).expect_err("instruction should inject #UD");
    assert!(
        err.to_string().contains("IDT entry 6 not present"),
        "expected #UD delivery failure, got {err}"
    );
}

#[test]
fn test_xbegin_forced_abort_jumps_to_fallback() {
    let code = [
        0x48, 0xc7, 0xc3, 0x00, 0x00, 0x00, 0x00, // MOV RBX, 0
        0xc7, 0xf8, 0x0a, 0x00, 0x00, 0x00, // XBEGIN +10
        0x48, 0xc7, 0xc3, 0x01, 0x00, 0x00, 0x00, // MOV RBX, 1 (skipped)
        0x0f, 0x01, 0xd5, // XEND (skipped)
        0x48, 0xc7, 0xc3, 0x02, 0x00, 0x00, 0x00, // MOV RBX, 2 (fallback)
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0, "forced XBEGIN abort should report status 0");
    assert_eq!(regs.rbx, 2, "XBEGIN should jump to fallback");
}

#[test]
fn test_xabort_outside_transaction_is_noop() {
    let code = [
        0x48, 0xc7, 0xc0, 0x34, 0x12, 0x00, 0x00, // MOV RAX, 0x1234
        0xc6, 0xf8, 0x42, // XABORT 0x42
        0x48, 0xc7, 0xc3, 0x78, 0x56, 0x00, 0x00, // MOV RBX, 0x5678
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x1234);
    assert_eq!(regs.rbx, 0x5678);
}

#[test]
fn test_xtest_outside_transaction_sets_zf() {
    let code = [
        0x48, 0xc7, 0xc0, 0x42, 0x00, 0x00, 0x00, // MOV RAX, 0x42
        0x0f, 0x01, 0xd6, // XTEST
        0xf4, // HLT
    ];
    let (mut vcpu, _) = setup_vm(&code, None);

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rax, 0x42);
    assert_ne!(regs.rflags & 0x40, 0, "XTEST should set ZF outside TSX");
}

#[test]
fn test_xend_outside_transaction_raises_gp() {
    let (mut vcpu, _) = setup_vm_no_idt(
        &[
            0x0f, 0x01, 0xd5, // XEND
            0xf4, // HLT
        ],
        None,
    );

    let err = vcpu
        .run()
        .expect_err("XEND outside a transaction should inject #GP");
    assert!(
        err.to_string().contains("IDT entry 13 not present"),
        "expected #GP delivery failure, got {err}"
    );
}

#[test]
fn test_xsusldtrk_unsupported_injects_ud() {
    assert_missing_idt_ud(&[
        0xf2, 0x0f, 0x01, 0xe8, // XSUSLDTRK
        0xf4, // HLT (should not be reached)
    ]);
}

#[test]
fn test_xresldtrk_unsupported_injects_ud() {
    assert_missing_idt_ud(&[
        0xf2, 0x0f, 0x01, 0xe9, // XRESLDTRK
        0xf4, // HLT (should not be reached)
    ]);
}
