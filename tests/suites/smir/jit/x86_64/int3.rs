//! Exact native-prefix handoff coverage for guest `INT3` (`CC`).

use super::*;

#[test]
fn jit_commits_supported_prefix_before_int3_without_executing_the_breakpoint() {
    let instruction = [0x66, 0xF3, 0x2E, 0x48, 0xCC];
    let mut code = vec![0xB8, 0x78, 0x56, 0x34, 0x12]; // MOV EAX,0x12345678
    code.extend_from_slice(&instruction);
    code.extend_from_slice(&[0xB9, 0x01, 0x00, 0x00, 0x00, 0xF4]);

    let mut vcpu = make_vcpu_code(&code);
    let mut before = vcpu.get_regs().unwrap();
    before.rcx = 0xDEAD_BEEF_CAFE_BABE;
    before.rflags = 0x2 | 0x8D5;
    vcpu.set_regs(&before).unwrap();

    assert!(
        vcpu.jit_try_block().expect("INT3 prefix region"),
        "the supported prefix before INT3 must enter the native tier"
    );
    let at_frontier = vcpu.get_regs().unwrap();
    assert_eq!(at_frontier.rax, 0x1234_5678);
    assert_eq!(at_frontier.rcx, before.rcx, "following MOV must not retire");
    assert_eq!(at_frontier.rflags, before.rflags);
    assert_eq!(at_frontier.rip, LOAD_ADDR + 5);

    let error = vcpu.step().expect_err("missing #BP gate").to_string();
    assert!(error.contains("IDT entry 3 not present"), "{error}");
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rip, at_frontier.rip, "delivery fault must restore RIP");
    assert_eq!(after.rax, at_frontier.rax);
    assert_eq!(after.rcx, at_frontier.rcx);
    assert_eq!(after.rflags, at_frontier.rflags);
}

#[test]
fn jit_rex2_int3_handoff_preserves_dynamic_apx_exception_priority() {
    for (apx_enabled, expected_vector) in [(false, 6), (true, 3)] {
        let code = [
            0xB8, 0x78, 0x56, 0x34, 0x12, // MOV EAX,0x12345678
            0xD5, 0x7F, 0xCC, // REX2 map-0 INT3
            0xB9, 0x01, 0x00, 0x00, 0x00, // MOV ECX,1
            0xF4,
        ];
        let mut vcpu = make_vcpu_code(&code);
        vcpu.set_apx_enabled(apx_enabled);
        let mut before = vcpu.get_regs().unwrap();
        before.rcx = 0xDEAD_BEEF_CAFE_BABE;
        before.rflags = 0x2 | 0x8D5;
        vcpu.set_regs(&before).unwrap();

        assert!(
            vcpu.jit_try_block().expect("REX2 INT3 prefix region"),
            "the supported prefix must compile independently of runtime APX"
        );
        let at_frontier = vcpu.get_regs().unwrap();
        assert_eq!(at_frontier.rax, 0x1234_5678);
        assert_eq!(at_frontier.rcx, before.rcx);
        assert_eq!(at_frontier.rflags, before.rflags);
        assert_eq!(at_frontier.rip, LOAD_ADDR + 5);

        let error = vcpu
            .step()
            .expect_err("missing exception gate")
            .to_string();
        assert!(
            error.contains(&format!("IDT entry {expected_vector} not present")),
            "APX={apx_enabled}: {error}"
        );
        let after = vcpu.get_regs().unwrap();
        assert_eq!(after.rip, at_frontier.rip);
        assert_eq!(after.rax, at_frontier.rax);
        assert_eq!(after.rcx, at_frontier.rcx);
        assert_eq!(after.rflags, at_frontier.rflags);
    }
}

#[test]
fn jit_rejects_bare_int3_entry_frontiers() {
    let mut vcpu = make_vcpu_code(&[0xCC]);
    let before = vcpu.get_regs().unwrap();
    assert!(
        !vcpu.jit_try_block().expect("bare INT3 frontier"),
        "a terminal entry with no native work must remain direct"
    );
    let after = vcpu.get_regs().unwrap();
    assert_eq!(after.rip, before.rip);
    assert_eq!(after.rax, before.rax);
    assert_eq!(after.rflags, before.rflags);
}
