//! Regression tests for issue #52: a legacy REX / REX2 prefix preceding an EVEX
//! prefix must not be able to abort the host emulator.
//!
//! The prefix scanner records a legacy REX (`0x40`–`0x4F`) or REX2 (`0xD5`) byte
//! in `ctx.rex`/`ctx.rex2` and keeps scanning, so `41 62 …` (REX.B then EVEX)
//! reaches the EVEX decoder with the stray REX still set. `decode_modrm` then ORs
//! `any_rex_b()` into the r/m field, and `evex_rm_vec_reg` adds EVEX.B/EVEX.X on
//! top — pushing the vector-register index to 32 and indexing the 16-entry
//! `regs.zmm_ext` array out of bounds. Under the release profile's
//! `panic = "abort"` that turned a single guest instruction into a host process
//! abort/DoS.
//!
//! A REX/REX2 prefix preceding a VEX/EVEX prefix is an illegal encoding (#UD per
//! the Intel SDM — EVEX supplies its own R/X/B/W bits). The fix rejects it with
//! #UD before decoding the EVEX payload, which closes every stray-REX leak path
//! (both the r/m and the reg/destination index). A defensive `rm & 0x7` mask in
//! `evex_rm_vec_reg` provides belt-and-suspenders bounding.
//!
//! Each abort-prevention test single-steps the malicious encoding and asserts the
//! emulator does not panic, returns gracefully, and delivers a guest #UD (control
//! transfers to the harness's IDT vector-6 handler). The boundary test confirms a
//! valid EVEX move from an extended register (zmm16) still executes correctly.

use crate::common::*;

/// Single-step `code` (placed at `CODE_ADDR`) once and assert the emulator
/// delivered a #UD to the guest rather than aborting the host. The harness
/// installs a full IDT, so #UD (vector 6) vectors to `INT_HANDLER_ADDR`.
fn assert_delivers_ud(code: &[u8]) {
    let (mut vcpu, _mem) = setup_vm(code, None);
    let exit = vcpu
        .step()
        .expect("illegal REX-before-EVEX encoding must not produce a fatal emulator error");
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
fn rex_b_before_evex_raises_ud_not_host_abort() {
    // REX.B (0x41) + EVEX VMOVAPS zmm0, zmm0 (reg-reg):
    //   41          : REX.B  -> any_rex_b() = 8
    //   62 91 7C 08 : EVEX prefix (P0=0x91: R=1,X=0,B=0,R'=1,mm=1; P1=0x7C: W0,vvvv=1111,pp=0; P2=0x08)
    //   28          : VMOVAPS opcode (load form, pp=0 -> execute_evex_mov_load)
    //   C0          : ModRM mod=11 reg=000 rm=000
    // Pre-fix: rm = 0 | 8 = 8, evex_rm_vec_reg adds 8 + 16 = 32 -> regs.zmm_ext[16]
    // out of bounds -> host abort. Post-fix: #UD before decoding the payload.
    assert_delivers_ud(&[0x41, 0x62, 0x91, 0x7C, 0x08, 0x28, 0xC0, 0xF4]);
}

#[test]
fn rex2_before_evex_raises_ud_not_host_abort() {
    // REX2 (0xD5) with B4 set + the same EVEX VMOVAPS reg-reg:
    //   D5 01       : REX2, payload bit0 (B4) set -> rex2_b() = 8
    //   62 91 7C 08 : EVEX prefix
    //   28 C0       : VMOVAPS reg-reg
    // Same out-of-bounds class via the REX2 path; must be #UD, not a panic.
    assert_delivers_ud(&[0xD5, 0x01, 0x62, 0x91, 0x7C, 0x08, 0x28, 0xC0, 0xF4]);
}

#[test]
fn valid_evex_vmovaps_from_zmm16_still_executes() {
    // VMOVAPS zmm1, zmm16 (no REX) — a valid EVEX reg-reg move whose source index
    // is an extended register (16). This exercises evex_rm_vec_reg's extension
    // path and confirms the defensive rm&0x7 mask does not corrupt valid moves and
    // that no spurious #UD is raised.
    //   62 B1 7C 48 28 C8
    //   P0=0xB1: R=1,X=0,B=1,R'=1,mm=1 -> dst reg=1, src rm=0 extended to 16
    let mut zmm_ext = [[0u64; 8]; 16];
    zmm_ext[0] = [0xAAAA, 0xBBBB, 0, 0, 0, 0, 0, 0]; // zmm16 low 128 bits
    let regs = Registers {
        zmm_ext,
        ..Registers::default()
    };
    let (mut vcpu, _mem) = setup_vm(&[0x62, 0xB1, 0x7C, 0x48, 0x28, 0xC8, 0xF4], Some(regs));
    let exit = vcpu
        .step()
        .expect("valid EVEX VMOVAPS must execute without error");
    assert!(
        exit.is_none(),
        "valid EVEX move must not produce a VM exit, got {exit:?}",
    );
    let regs = vcpu.get_regs().unwrap();
    assert_ne!(
        regs.rip, INT_HANDLER_ADDR,
        "a valid EVEX encoding must not raise #UD",
    );
    assert_eq!(
        regs.rip,
        CODE_ADDR + 6,
        "RIP should advance past the 6-byte instruction",
    );
    assert_eq!(
        regs.xmm[1],
        [0xAAAA, 0xBBBB],
        "VMOVAPS zmm1, zmm16 must copy zmm16's low 128 bits",
    );
}
