//! Decode-cache handler resolution for the function-pointer dispatch fast path.
//!
//! On a decode-cache MISS the big `execute` opcode match still runs once (to
//! produce the result for the current instruction); alongside it we resolve the
//! instruction's handler to a uniform-signature function pointer and stash it in
//! the [`DecodeCacheEntry`]. On a subsequent HIT the stored pointer is called
//! directly, skipping the `execute` match and the two-byte / escape call chain.
//!
//! The mapping here MIRRORS `dispatch/legacy.rs::execute` exactly (same opcode
//! ranges, same prefix-independent behaviour). Handlers that take an extra
//! argument in `execute` (`opcode`, condition code, or a literal segment index)
//! are wrapped in thin shims that recover that argument from `InsnContext`
//! (`ctx.opcode` is set by `step()` before dispatch).
//!
//! Correctness note: the resolved pointer must be a pure function of the same
//! inputs the cache key already covers (opcode + prefixes + mode). Multi-byte
//! escape opcodes (0x0F, VEX, EVEX, x87) resolve to their top-level dispatcher
//! (`execute_0f`, `execute_vex2/3`, escape_dX), which re-reads the trailing
//! bytes from `ctx` every call exactly as the match did — so they remain correct
//! even though several distinct instructions share one cache slot's handler.

use crate::error::Result;
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{HandlerFn, InsnContext, X86_64Vcpu};
use crate::isa::x86_64::execute;

// ===========================================================================
// Shims: recover the opcode-/cc-/sreg-derived argument from the context.
// These mirror the `self.handler(ctx, arg)` arms in `execute`.
// ===========================================================================

fn sh_jcc_rel8(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    execute::control::jcc_rel8(v, c, c.opcode & 0x0F)
}
fn sh_mov_r8_imm8(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op = c.opcode;
    execute::data::mov_r8_imm8(v, c, op)
}
fn sh_mov_r_imm(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op = c.opcode;
    execute::data::mov_r_imm(v, c, op)
}
fn sh_push_r64(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op = c.opcode;
    execute::data::push_r64(v, c, op)
}
fn sh_pop_r64(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op = c.opcode;
    execute::data::pop_r64(v, c, op)
}
fn sh_xchg_rax_r(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op = c.opcode;
    execute::data::xchg_rax_r(v, c, op)
}
// PUSH/POP segment register: literal segment index per opcode (matches `execute`).
fn sh_push_es(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    execute::data::push_sreg(v, c, 0)
}
fn sh_push_cs(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    execute::data::push_sreg(v, c, 1)
}
fn sh_push_ss(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    execute::data::push_sreg(v, c, 2)
}
fn sh_push_ds(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    execute::data::push_sreg(v, c, 3)
}
fn sh_pop_es(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    execute::data::pop_sreg(v, c, 0)
}
fn sh_pop_ss(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    execute::data::pop_sreg(v, c, 2)
}
fn sh_pop_ds(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    execute::data::pop_sreg(v, c, 3)
}

// Opcodes whose `execute` arm is a small inline body rather than an `execute::`
// call get their own shims so the fast path mirrors them exactly.
fn sh_nop_or_pause(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // 0x90: PAUSE under F3, otherwise NOP.
    if c.rex_b() != 0 {
        execute::data::xchg_rax_r(v, c, 0x90)
    } else if c.rep_prefix == Some(0xF3) {
        execute::system::pause(v, c)
    } else {
        v.regs.rip += c.cursor as u64;
        Ok(None)
    }
}
fn sh_hlt(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    v.regs.rip += c.cursor as u64;
    v.halted = true;
    Ok(Some(VcpuExit::Hlt))
}
fn sh_fwait(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // 0x9B FWAIT/WAIT - NOP in this emulator.
    v.regs.rip += c.cursor as u64;
    Ok(None)
}

// Multi-byte escape dispatchers (re-read trailing bytes from ctx each call).
fn sh_0f(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    v.execute_0f(c)
}
fn sh_les_or_vex3(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    v.execute_les_or_vex3(c)
}
fn sh_lds_or_vex2(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    v.execute_lds_or_vex2(c)
}
fn sh_pop_or_xop(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    v.execute_pop_or_xop(c)
}
fn sh_mov_rax_moffs_or_jmp_abs(
    v: &mut X86_64Vcpu,
    c: &mut InsnContext,
) -> Result<Option<VcpuExit>> {
    if c.has_rex2() {
        execute::control::jmp_abs(v, c)
    } else {
        execute::data::mov_rax_moffs(v, c)
    }
}
fn sh_arpl_or_movsxd(v: &mut X86_64Vcpu, c: &mut InsnContext) -> Result<Option<VcpuExit>> {
    if v.sregs.cs.l {
        execute::data::movsxd(v, c)
    } else {
        execute::data::arpl(v, c)
    }
}

impl X86_64Vcpu {
    /// Resolve a single-byte opcode to its uniform-signature handler.
    ///
    /// MUST stay in lockstep with `dispatch/legacy.rs::execute`. Returns `None`
    /// for opcodes `execute` would treat as unimplemented (the `_ =>` arm); in
    /// that case the fill path stores a fallback that simply re-enters `execute`
    /// (which produces the proper error), so behaviour is identical.
    pub(in crate::isa::x86_64) fn resolve_handler(opcode: u8) -> Option<HandlerFn> {
        let f: HandlerFn = match opcode {
            0x90 => sh_nop_or_pause,
            0xF4 => sh_hlt,
            0x0F => sh_0f,

            // Control flow
            0xEB => execute::control::jmp_rel8,
            0xE9 => execute::control::jmp_rel32,
            0xEA => execute::control::jmp_far_ptr,
            0xE8 => execute::control::call_rel32,
            0x9A => execute::control::call_far_ptr,
            0xC3 => execute::control::ret,
            0xC2 => execute::control::ret_imm16,
            0xCA => execute::control::retf_imm16,
            0xCB => execute::control::retf,
            0xCF => execute::control::iret,
            0x70..=0x7F => sh_jcc_rel8,

            // Legacy LES/LDS in compatibility mode, or VEX prefixes otherwise.
            0xC4 => sh_les_or_vex3,
            0xC5 => sh_lds_or_vex2,

            // I/O
            0xE4 => execute::io::in_al_imm8,
            0xE5 => execute::io::in_ax_imm8,
            0xEC => execute::io::in_al_dx,
            0xED => execute::io::in_ax_dx,
            0xE6 => execute::io::out_imm8_al,
            0xE7 => execute::io::out_imm8_ax,
            0xEE => execute::io::out_dx_al,
            0xEF => execute::io::out_dx_ax,

            // String I/O
            0x6C => execute::io::insb,
            0x6D => execute::io::insw,
            0x6E => execute::io::outsb,
            0x6F => execute::io::outsw,

            // Data movement
            0xB0..=0xB7 => sh_mov_r8_imm8,
            0xB8..=0xBF => sh_mov_r_imm,
            0x88 => execute::data::mov_rm8_r8,
            0x89 => execute::data::mov_rm_r,
            0x8A => execute::data::mov_r8_rm8,
            0x8B => execute::data::mov_r_rm,
            0x8C => execute::data::mov_rm_sreg,
            0x8E => execute::data::mov_sreg_rm,
            0x8D => execute::data::lea,
            0x06 => sh_push_es,
            0x0E => sh_push_cs,
            0x16 => sh_push_ss,
            0x1E => sh_push_ds,
            0x07 => sh_pop_es,
            0x17 => sh_pop_ss,
            0x1F => sh_pop_ds,
            0xA0 => execute::data::mov_al_moffs,
            0xA1 => sh_mov_rax_moffs_or_jmp_abs,
            0xA2 => execute::data::mov_moffs_al,
            0xA3 => execute::data::mov_moffs_rax,
            0xC6 => execute::data::mov_rm8_imm8,
            0xC7 => execute::data::mov_rm_imm,
            0x50..=0x57 => sh_push_r64,
            0x58..=0x5F => sh_pop_r64,
            0x8F => sh_pop_or_xop,
            0x6A => execute::data::push_imm8,
            0x68 => execute::data::push_imm32,
            0x86 => execute::data::xchg_r8_rm8,
            0x87 => execute::data::xchg_r_rm,
            0x91..=0x97 => sh_xchg_rax_r,
            0x63 => sh_arpl_or_movsxd,

            // Arithmetic
            0x00 => execute::arith::add_rm8_r8,
            0x01 => execute::arith::add_rm_r,
            0x02 => execute::arith::add_r8_rm8,
            0x03 => execute::arith::add_r_rm,
            0x04 => execute::arith::add_al_imm8,
            0x05 => execute::arith::add_rax_imm,
            0x10 => execute::arith::adc_rm8_r8,
            0x11 => execute::arith::adc_rm_r,
            0x12 => execute::arith::adc_r8_rm8,
            0x13 => execute::arith::adc_r_rm,
            0x14 => execute::arith::adc_al_imm8,
            0x15 => execute::arith::adc_rax_imm,
            0x18 => execute::arith::sbb_rm8_r8,
            0x19 => execute::arith::sbb_rm_r,
            0x1A => execute::arith::sbb_r8_rm8,
            0x1B => execute::arith::sbb_r_rm,
            0x1C => execute::arith::sbb_al_imm8,
            0x1D => execute::arith::sbb_rax_imm,
            0x27 => execute::arith::daa,
            0x28 => execute::arith::sub_rm8_r8,
            0x29 => execute::arith::sub_rm_r,
            0x2A => execute::arith::sub_r8_rm8,
            0x2B => execute::arith::sub_r_rm,
            0x2C => execute::arith::sub_al_imm8,
            0x2D => execute::arith::sub_rax_imm,
            0x2F => execute::arith::das,
            0x38 => execute::arith::cmp_rm8_r8,
            0x39 => execute::arith::cmp_rm_r,
            0x3A => execute::arith::cmp_r8_rm8,
            0x3B => execute::arith::cmp_r_rm,
            0x3C => execute::arith::cmp_al_imm8,
            0x3D => execute::arith::cmp_rax_imm,
            0x3F => execute::arith::aas,
            0x80 | 0x82 => execute::arith::group1_rm8_imm8,
            0x81 => execute::arith::group1_rm_imm32,
            0x83 => execute::arith::group1_rm_imm8,
            0x69 => execute::arith::imul_r_rm_imm,
            0x6B => execute::arith::imul_r_rm_imm8,
            0x98 => execute::arith::cbw_cwde_cdqe,
            0x99 => execute::arith::cwd_cdq_cqo,

            // Logic
            0x08 => execute::logic::or_rm8_r8,
            0x09 => execute::logic::or_rm_r,
            0x0A => execute::logic::or_r8_rm8,
            0x0B => execute::logic::or_r_rm,
            0x0C => execute::logic::or_al_imm8,
            0x0D => execute::logic::or_rax_imm,
            0x20 => execute::logic::and_rm8_r8,
            0x21 => execute::logic::and_rm_r,
            0x22 => execute::logic::and_r8_rm8,
            0x23 => execute::logic::and_r_rm,
            0x24 => execute::logic::and_al_imm8,
            0x25 => execute::logic::and_rax_imm,
            0x30 => execute::logic::xor_rm8_r8,
            0x31 => execute::logic::xor_rm_r,
            0x32 => execute::logic::xor_r8_rm8,
            0x33 => execute::logic::xor_r_rm,
            0x34 => execute::logic::xor_al_imm8,
            0x35 => execute::logic::xor_rax_imm,
            0x37 => execute::arith::aaa,
            0x84 => execute::logic::test_rm8_r8,
            0x85 => execute::logic::test_rm_r,
            0xA8 => execute::logic::test_al_imm8,
            0xA9 => execute::logic::test_rax_imm,
            0xF6 => execute::logic::group3_rm8,
            0xF7 => execute::logic::group3_rm,

            // Shifts/Rotates
            0xC0 => execute::shift::group2_rm8_imm8,
            0xC1 => execute::shift::group2_rm_imm8,
            0xD0 => execute::shift::group2_rm8_1,
            0xD1 => execute::shift::group2_rm_1,
            0xD2 => execute::shift::group2_rm8_cl,
            0xD3 => execute::shift::group2_rm_cl,

            // BCD Adjust
            0xD4 => execute::arith::aam,
            0xD5 => execute::arith::aad,

            // System/Flags
            0xFA => execute::system::cli,
            0xFB => execute::system::sti,
            0xF8 => execute::system::clc,
            0xF9 => execute::system::stc,
            0xF5 => execute::system::cmc,
            0xFC => execute::system::cld,
            0xFD => execute::system::std,
            0x9C => execute::system::pushf,
            0x9D => execute::system::popf,
            0x9E => execute::system::sahf,
            0x9F => execute::system::lahf,

            // Loop instructions
            0xE0 => execute::control::loopnz,
            0xE1 => execute::control::loopz,
            0xE2 => execute::control::loop_rel8,
            0xE3 => execute::control::jrcxz,

            // Interrupts
            0xCC => execute::control::int3,
            0xCD => execute::control::int_imm8,
            0xCE => execute::control::into,

            // Misc
            0x60 => execute::data::pusha,
            0x61 => execute::data::popa,
            0x62 => execute::data::bound_or_evex,
            0xC8 => execute::data::enter,
            0xC9 => execute::data::leave,
            0xD7 => execute::control::xlat,
            0xFE => execute::control::group4,
            0xFF => execute::control::group5,

            // String operations
            0xA4 => execute::string::movsb,
            0xA5 => execute::string::movs,
            0xAA => execute::string::stosb,
            0xAB => execute::string::stos,
            0xAC => execute::string::lodsb,
            0xAD => execute::string::lods,
            0xAE => execute::string::scasb,
            0xAF => execute::string::scas,
            0xA6 => execute::string::cmpsb,
            0xA7 => execute::string::cmps,

            // FWAIT/WAIT
            0x9B => sh_fwait,

            // x87 FPU escape opcodes
            0xD8 => execute::fpu::escape_d8,
            0xD9 => execute::fpu::escape_d9,
            0xDA => execute::fpu::escape_da,
            0xDB => execute::fpu::escape_db,
            0xDC => execute::fpu::escape_dc,
            0xDD => execute::fpu::escape_dd,
            0xDE => execute::fpu::escape_de,
            0xDF => execute::fpu::escape_df,

            // Unimplemented in `execute` (the `_ =>` arm). Fall back to the match.
            _ => return None,
        };
        Some(f)
    }
}
