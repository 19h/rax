//! Regression tests for issue #29: guest-controlled opmask register encodings
//! must not index past the architectural opmask register file (k0-k7).
//!
//! VEX-encoded opmask instructions select a source opmask register either via
//! the 4-bit `VEX.vvvv` field (range 0..=15) or, for KMOV stores, via a
//! `REX.R`/`VEX.R`-extended `ModRM.reg` field. Only k0-k7 exist, so any selector
//! >= 8 previously indexed `regs.k` (`[u64; 8]`) out of bounds. Under the release
//! profile's `panic = "abort"` that turned a single guest instruction into a host
//! process abort/DoS. The fix raises #UD (invalid-opcode, vector 6) for an
//! out-of-range opmask selector instead of panicking.
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
