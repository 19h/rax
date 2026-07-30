//! Atomic instructions: XADD, CMPXCHG.
//!
//! LOCK semantics: these read-modify-write forms may carry a LOCK prefix
//! (0xF0). On a real CPU LOCK guarantees the RMW is atomic w.r.t. other cores;
//! rax is a single-vCPU interpreter, so each instruction runs to completion
//! without interleaving and the RMW is already atomic regardless of LOCK.
//! The only architectural behaviour LOCK adds here is a decode-time legality
//! check (`X86_64Vcpu::enforce_lock_prefix`): a LOCK on a register-destination
//! XADD/CMPXCHG (ModR/M mod == 3) raises #UD before these handlers run, so the
//! register branches below are never reached with a LOCK prefix present.

use crate::error::Result;
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::isa::x86_64::execute::system::is_canonical_48;
use crate::isa::x86_64::flags;

#[inline]
fn alignment_check_enabled(vcpu: &X86_64Vcpu) -> bool {
    const CR0_AM: u64 = 1 << 18;
    vcpu.sregs.cr0 & CR0_AM != 0
        && vcpu.regs.rflags & flags::bits::AC != 0
        && vcpu.sregs.cs.selector & 3 == 3
}

/// Original-VEX and APX-promoted-EVEX
/// `CMPccXADD r32/r64, r32/r64, m32/m64`.
///
/// The emulator's single-vCPU execution model makes the staged read/write one
/// non-interleaved instruction transaction. Both the successful-add and false
/// condition paths perform the architecturally required write. Architectural
/// registers, flags, and RIP commit only after that write succeeds.
pub(in crate::isa::x86_64) fn cmpccxadd(
    vcpu: &mut X86_64Vcpu,
    ctx: &mut InsnContext,
    add_register: u8,
    condition_code: u8,
) -> Result<Option<VcpuExit>> {
    if !vcpu.sregs.cs.l {
        return vcpu.inject_undefined_instruction();
    }
    let apx = ctx.is_apx();

    let modrm_start = ctx.cursor;
    let modrm = ctx.consume_u8()?;
    if modrm >> 6 == 3 {
        return vcpu.inject_undefined_instruction();
    }
    let cmp_register = ((modrm >> 3) & 7)
        | if apx {
            ctx.evex_dest_reg()
        } else {
            ctx.any_rex_r()
        };
    let (address, extra, stack_segment) =
        vcpu.decode_modrm_addr_with_stack_segment(ctx, modrm_start)?;
    ctx.cursor = modrm_start + 1 + extra;

    let size = ctx.op_size;
    debug_assert!(matches!(size, 4 | 8));
    let canonical_range = address
        .checked_add(u64::from(size - 1))
        .is_some_and(|last| is_canonical_48(address) && is_canonical_48(last));
    if !canonical_range {
        vcpu.inject_exception(if stack_segment { 12 } else { 13 }, Some(0))?;
        return Ok(None);
    }
    if address & u64::from(size - 1) != 0 {
        if apx {
            vcpu.inject_exception(13, Some(0))?;
            return Ok(None);
        }
        if alignment_check_enabled(vcpu) {
            vcpu.inject_exception(17, Some(0))?;
            return Ok(None);
        }
    }

    // Snapshot both register operands before the destination write; either may
    // also participate in effective-address calculation or alias the other.
    let cmp = vcpu.get_reg(cmp_register, size);
    let add = vcpu.get_reg(add_register, size);
    let old = vcpu.read_mem(address, size)?;
    let mask = if size == 4 {
        u64::from(u32::MAX)
    } else {
        u64::MAX
    };
    let old = old & mask;
    let cmp = cmp & mask;
    let add = add & mask;
    let mut candidate_rflags = vcpu.regs.rflags;
    flags::update_flags_sub(
        &mut candidate_rflags,
        old,
        cmp,
        old.wrapping_sub(cmp) & mask,
        size,
    );
    let new = if flags::condition_holds(candidate_rflags, condition_code) {
        old.wrapping_add(add) & mask
    } else {
        old
    };

    // A false condition is still a locked write-back of the original value.
    vcpu.write_mem(address, new, size)?;
    vcpu.set_reg(cmp_register, old, size);
    vcpu.regs.rflags = candidate_rflags;
    vcpu.clear_lazy_flags();
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// XADD r/m8, r8 (0x0F 0xC0) - Exchange and Add
pub fn xadd_rm8_r8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let has_rex = ctx.rex.is_some();
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg8(reg, has_rex) as u8;

    if is_memory {
        let dst = vcpu.mmu.read_u8(addr, &vcpu.sregs)?;
        let sum = dst.wrapping_add(src);
        // DEST = DEST + SRC, SRC = old DEST
        vcpu.mmu.write_u8(addr, sum, &vcpu.sregs)?;
        vcpu.set_reg8(reg, dst as u64, has_rex);
        flags::update_flags_add(&mut vcpu.regs.rflags, dst as u64, src as u64, sum as u64, 1);
    } else {
        let dst = vcpu.get_reg8(rm, has_rex) as u8;
        let sum = dst.wrapping_add(src);
        // XADD: TEMP = SRC + DEST; SRC = DEST; DEST = TEMP. When reg == rm refer
        // to the same byte register, writing SRC = old DEST after DEST = sum
        // would clobber the sum back to the old value, so only the DEST = sum
        // write happens (matches hardware: the register ends up 2*old). The
        // AH/AL-style alias (different byte positions of the same GPR) is fine
        // since the two set_reg8 writes touch disjoint byte lanes.
        if reg == rm {
            vcpu.set_reg8(rm, sum as u64, has_rex);
        } else {
            vcpu.set_reg8(rm, sum as u64, has_rex);
            vcpu.set_reg8(reg, dst as u64, has_rex);
        }
        flags::update_flags_add(&mut vcpu.regs.rflags, dst as u64, src as u64, sum as u64, 1);
    }
    vcpu.clear_lazy_flags();
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// XADD r/m, r (0x0F 0xC1) - Exchange and Add
pub fn xadd_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, op_size);

    if is_memory {
        let dst = vcpu.read_mem(addr, op_size)?;
        let sum = dst.wrapping_add(src);
        // DEST = DEST + SRC, SRC = old DEST
        vcpu.write_mem(addr, sum, op_size)?;
        vcpu.set_reg(reg, dst, op_size);
        flags::update_flags_add(&mut vcpu.regs.rflags, dst, src, sum, op_size);
    } else {
        let dst = vcpu.get_reg(rm, op_size);
        let sum = dst.wrapping_add(src);
        // XADD: TEMP = SRC + DEST; SRC = DEST; DEST = TEMP
        // When reg == rm (same register), both SRC and DEST refer to the same register
        // so the result is just DEST = DEST + SRC = 2 * reg (SRC = DEST is a no-op)
        if reg == rm {
            vcpu.set_reg(rm, sum, op_size);
        } else {
            vcpu.set_reg(rm, sum, op_size);
            vcpu.set_reg(reg, dst, op_size);
        }
        flags::update_flags_add(&mut vcpu.regs.rflags, dst, src, sum, op_size);
    }
    vcpu.clear_lazy_flags();
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CMPXCHG r/m8, r8 (0x0F 0xB0) - Compare and Exchange
pub fn cmpxchg_rm8_r8(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let has_rex = ctx.rex.is_some();
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg8(reg, has_rex) as u8;
    let al = (vcpu.regs.rax & 0xFF) as u8;

    let dst = if is_memory {
        vcpu.mmu.read_u8(addr, &vcpu.sregs)?
    } else {
        vcpu.get_reg8(rm, has_rex) as u8
    };

    // Compare AL with destination
    let cmp_result = al.wrapping_sub(dst);
    flags::update_flags_sub(
        &mut vcpu.regs.rflags,
        al as u64,
        dst as u64,
        cmp_result as u64,
        1,
    );
    vcpu.clear_lazy_flags();

    if al == dst {
        // ZF is set, store source into destination
        if is_memory {
            vcpu.mmu.write_u8(addr, src, &vcpu.sregs)?;
        } else {
            vcpu.set_reg8(rm, src as u64, has_rex);
        }
    } else {
        // ZF is clear, load destination into AL
        vcpu.regs.rax = (vcpu.regs.rax & !0xFF) | (dst as u64);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// CMPXCHG r/m, r (0x0F 0xB1) - Compare and Exchange
pub fn cmpxchg_rm_r(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let op_size = ctx.op_size;
    let (reg, rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    let src = vcpu.get_reg(reg, op_size);
    let rax = vcpu.get_reg(0, op_size);

    let dst = if is_memory {
        vcpu.read_mem(addr, op_size)?
    } else {
        vcpu.get_reg(rm, op_size)
    };

    // Compare rAX with destination
    let cmp_result = rax.wrapping_sub(dst);
    flags::update_flags_sub(&mut vcpu.regs.rflags, rax, dst, cmp_result, op_size);
    vcpu.clear_lazy_flags();

    if rax == dst {
        // ZF is set, store source into destination
        if is_memory {
            vcpu.write_mem(addr, src, op_size)?;
        } else {
            vcpu.set_reg(rm, src, op_size);
        }
    } else {
        // ZF is clear, load destination into rAX
        vcpu.set_reg(0, dst, op_size);
    }
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
