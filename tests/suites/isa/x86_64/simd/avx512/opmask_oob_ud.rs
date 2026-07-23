//! Regression tests for issue #29: guest-controlled opmask register encodings
//! must not index past the architectural opmask register file (k0-k7).
//!
//! VEX-encoded opmask instructions select registers through `VEX.vvvv` and
//! ModR/M fields. Only k0-k7 exist, so K-register extension bits are reserved;
//! unary/move/test forms additionally reserve `VEX.vvvv`. Out-of-range selectors
//! previously could index `regs.k` (`[u64; 8]`) out of bounds, while other
//! reserved forms silently aliased k0-k7. The decoder must raise #UD
//! (invalid-opcode, vector 6) for every such encoding.
//!
//! Affected handlers hardened by the fix:
//!   * `execute_kunpck`      — KUNPCKBW/WD/DQ (the encoding from the issue)
//!   * `execute_kmask_binop` — KAND/KANDN/KOR/KXNOR/KXOR/KADD
//!   * `execute_kmov_store`  — KMOVB/W/D/Q m, k (REX.R-extended ModRM.reg)
//!
//! Each abort-prevention test single-steps the malicious encoding and asserts
//! that the emulator does not panic, returns gracefully, and delivers a guest
//! #UD (control transfers to the harness's IDT vector-6 handler). The boundary
//! test confirms the highest valid selector (k7) is still accepted.
//!
//! Reference: Intel SDM Vol. 2 — opmask registers are k0-k7 only.

use crate::common::*;

/// Single-step `code` (placed at `CODE_ADDR`) once and assert the emulator
/// delivered a #UD to the guest rather than aborting the host. The test harness
/// installs a full IDT, so #UD (vector 6) vectors to `INT_HANDLER_ADDR`.
fn assert_delivers_ud(code: &[u8]) {
    let (mut vcpu, _mem) = setup_vm(code, None);
    // Must return Ok (graceful) and, crucially, must not panic/abort the host.
    let exit = vcpu
        .step()
        .expect("malicious opmask encoding must not produce a fatal emulator error");
    assert!(
        exit.is_none(),
        "expected #UD injection with no VM exit, got {exit:?}",
    );
    let regs = vcpu.get_regs().unwrap();
    assert_eq!(
        regs.rip, INT_HANDLER_ADDR,
        "guest #UD handler must run: RIP should be at the IDT vector-6 handler",
    );
}

#[test]
fn kunpckbw_vvvv_high_bit_raises_ud_not_host_abort() {
    // KUNPCKBW with VEX.vvvv decoding to 8 (the exact encoding from issue #29):
    //   C5 : 2-byte VEX prefix
    //   BD : R=1, vvvv field 0111b -> decoded 8 (k8, nonexistent), L=1, pp=01 (66)
    //   4B : KUNPCK opcode
    //   C0 : ModRM mod=11 reg=000 (k0 dst) rm=000 (k0 src2)
    // Pre-fix, vvvv=8 indexed regs.k[8] (out of bounds). Post-fix it is #UD.
    assert_delivers_ud(&[0xC5, 0xBD, 0x4B, 0xC0, 0xF4]);
}

#[test]
fn kandw_vvvv_high_bit_raises_ud_not_host_abort() {
    // KANDW with VEX.vvvv decoding to 8:
    //   C5 : 2-byte VEX prefix
    //   BC : R=1, vvvv field 0111b -> decoded 8, L=1, pp=00 (no prefix), W0 => 16-bit
    //   41 : KAND opcode
    //   C0 : ModRM mod=11 reg=000 (k0 dst) rm=000 (k0 src2)
    // Same out-of-bounds source-index class as KUNPCK; must be #UD, not a panic.
    assert_delivers_ud(&[0xC5, 0xBC, 0x41, 0xC0, 0xF4]);
}

#[test]
fn kmovw_store_rex_r_extended_reg_raises_ud_not_host_abort() {
    // KMOVW m16, k1 with VEX.R extending ModRM.reg to 8 (k8, nonexistent):
    //   C5 : 2-byte VEX prefix
    //   78 : R=0 (=> REX.R, reg += 8), vvvv field 1111b (unused), L=0, pp=00, W0
    //   91 : KMOV store opcode
    //   00 : ModRM mod=00 reg=000 rm=000 -> [rax] (memory); reg extended to 8
    // Pre-fix, the extended reg indexed regs.k[8] (out of bounds). Post-fix #UD.
    assert_delivers_ud(&[0xC5, 0x78, 0x91, 0x00, 0xF4]);
}

#[test]
fn reserved_k_register_extension_bits_raise_ud_for_every_opmask_family() {
    for (_label, code) in [
        ("KMOV load VEX.R", &[0xC5, 0x78, 0x90, 0xC0, 0xF4][..]),
        ("KMOV load VEX.B", &[0xC4, 0xC1, 0x78, 0x90, 0xC0, 0xF4]),
        ("KMOV GPR-to-K VEX.R", &[0xC5, 0x78, 0x92, 0xC0, 0xF4]),
        ("KMOV K-to-GPR VEX.B", &[0xC4, 0xC1, 0x78, 0x93, 0xC0, 0xF4]),
        ("KNOT VEX.R", &[0xC5, 0x78, 0x44, 0xC0, 0xF4]),
        ("KNOT VEX.B", &[0xC4, 0xC1, 0x78, 0x44, 0xC0, 0xF4]),
        ("KAND VEX.R", &[0xC5, 0x7C, 0x41, 0xC0, 0xF4]),
        ("KAND VEX.B", &[0xC4, 0xC1, 0x7C, 0x41, 0xC0, 0xF4]),
        ("KTEST VEX.R", &[0xC5, 0x78, 0x99, 0xC0, 0xF4]),
        ("KORTEST VEX.B", &[0xC4, 0xC1, 0x78, 0x98, 0xC0, 0xF4]),
        ("KUNPCK VEX.R", &[0xC5, 0x7D, 0x4B, 0xC0, 0xF4]),
        ("KUNPCK VEX.B", &[0xC4, 0xC1, 0x7D, 0x4B, 0xC0, 0xF4]),
        ("KSHIFT VEX.R", &[0xC4, 0x63, 0xF9, 0x32, 0xC0, 0x01, 0xF4]),
        ("KSHIFT VEX.B", &[0xC4, 0xC3, 0xF9, 0x32, 0xC0, 0x01, 0xF4]),
    ] {
        assert_delivers_ud(code);
    }
}

#[test]
fn reserved_vvvv_raises_ud_for_unary_move_and_test_opmask_forms() {
    for code in [
        &[0xC5, 0xF0, 0x90, 0xC0, 0xF4][..],         // KMOVW k0,k0
        &[0xC5, 0xF0, 0x91, 0x00, 0xF4],             // KMOVW [rax],k0
        &[0xC5, 0xF0, 0x92, 0xC0, 0xF4],             // KMOVW k0,eax
        &[0xC5, 0xF0, 0x93, 0xC0, 0xF4],             // KMOVW eax,k0
        &[0xC5, 0xF0, 0x44, 0xC0, 0xF4],             // KNOTW k0,k0
        &[0xC5, 0xF0, 0x99, 0xC0, 0xF4],             // KTESTW k0,k0
        &[0xC5, 0xF0, 0x98, 0xC0, 0xF4],             // KORTESTW k0,k0
        &[0xC4, 0xE3, 0xF1, 0x32, 0xC0, 0x01, 0xF4], // KSHIFTLW k0,k0,1
    ] {
        assert_delivers_ud(code);
    }
}

#[test]
fn noncanonical_kmov_prefixes_and_opcode91_register_form_raise_ud() {
    for code in [
        &[0xC5, 0xFB, 0x90, 0xC0, 0xF4][..],   // F2/W0 opcode-90 alias
        &[0xC4, 0xE1, 0xFB, 0x90, 0xC0, 0xF4], // F2/W1 opcode-90 alias
        &[0xC5, 0xFB, 0x91, 0x00, 0xF4],       // F2/W0 opcode-91 alias
        &[0xC4, 0xE1, 0xFB, 0x91, 0x00, 0xF4], // F2/W1 opcode-91 alias
        &[0xC5, 0xF8, 0x91, 0xC0, 0xF4],       // opcode 91 is MR-only
    ] {
        assert_delivers_ud(code);
    }
}

#[test]
fn kunpckbw_max_valid_vvvv_k7_still_executes() {
    // Boundary check: VEX.vvvv = 7 (k7, the highest valid opmask register) must
    // NOT be rejected by the new guard — it must execute normally.
    //   C5 C5 4B C0 : KUNPCKBW k0, k7, k0 (vvvv field 1000b -> decoded 7)
    // KUNPCKBW computes k0 = (k7[7:0] << 8) | k0[7:0].
    let regs = Registers {
        k: [0x12, 0, 0, 0, 0, 0, 0, 0xAB],
        ..Registers::default()
    };
    let (mut vcpu, _mem) = setup_vm(&[0xC5, 0xC5, 0x4B, 0xC0, 0xF4], Some(regs));
    let exit = vcpu
        .step()
        .expect("valid KUNPCKBW must execute without error");
    assert!(
        exit.is_none(),
        "valid KUNPCKBW must not produce a VM exit, got {exit:?}",
    );
    let regs = vcpu.get_regs().unwrap();
    assert_ne!(
        regs.rip, INT_HANDLER_ADDR,
        "a valid vvvv=7 encoding must not raise #UD",
    );
    assert_eq!(
        regs.rip,
        CODE_ADDR + 4,
        "RIP should advance past the 4-byte instruction",
    );
    assert_eq!(
        regs.k[0], 0xAB12,
        "KUNPCKBW k0,k7,k0 = (k7 << 8) | k0 = 0xAB12",
    );
}
