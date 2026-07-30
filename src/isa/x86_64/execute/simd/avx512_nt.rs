//! EVEX non-temporal vector loads and stores.

use crate::error::{Error, Result};
use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};
use crate::vm::vcpu::VcpuExit;

use super::avx512::{
    evex_scaled_disp8_addr, load_mem_bytes, read_reg_bytes, store_mem_bytes, vl_bytes_of,
    write_vec_vl,
};

/// EVEX VMOVNTDQA: non-temporal aligned vector load. Cache hints are not
/// architectural in the emulator, so this is modeled as a memory vector load.
pub fn evex_nt_load(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let evex = ctx.evex.ok_or_else(|| {
        Error::Emulator("EVEX non-temporal load requires EVEX prefix".to_string())
    })?;

    if evex.aaa != 0 || evex.z || evex.broadcast || evex.vvvv != 0xF {
        return vcpu.inject_undefined_instruction();
    }

    let modrm_start = ctx.cursor;
    let (reg, _rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    if !is_memory {
        return vcpu.inject_undefined_instruction();
    }

    let dest = (reg & 0x07) | if evex.r { 0 } else { 8 } | if evex.r_prime { 0 } else { 16 };
    let vl_bytes = vl_bytes_of(evex.ll);
    let addr = evex_scaled_disp8_addr(ctx, modrm_start, addr, vl_bytes);
    if addr & (vl_bytes as u64 - 1) != 0 {
        vcpu.inject_exception(13, Some(0))?;
        return Ok(None);
    }
    let data = load_mem_bytes(vcpu, addr, 8, vl_bytes / 8)?;
    write_vec_vl(vcpu, dest, vl_bytes, &data);
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// EVEX VMOVNTPS/PD/DQ: non-temporal vector store. Cache hints are not
/// architectural in the emulator, so this is modeled as a memory vector store.
pub fn evex_nt_store(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    let evex = ctx.evex.ok_or_else(|| {
        Error::Emulator("EVEX non-temporal store requires EVEX prefix".to_string())
    })?;

    if evex.aaa != 0 || evex.z || evex.broadcast || evex.vvvv != 0xF {
        return vcpu.inject_undefined_instruction();
    }

    let modrm_start = ctx.cursor;
    let (reg, _rm, is_memory, addr, _) = vcpu.decode_modrm(ctx)?;
    if !is_memory {
        return vcpu.inject_undefined_instruction();
    }

    let src = (reg & 0x07) | if evex.r { 0 } else { 8 } | if evex.r_prime { 0 } else { 16 };
    let vl_bytes = vl_bytes_of(evex.ll);
    let addr = evex_scaled_disp8_addr(ctx, modrm_start, addr, vl_bytes);
    let data = read_reg_bytes(vcpu, src, vl_bytes);
    store_mem_bytes(vcpu, addr, 8, vl_bytes / 8, &data)?;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}
