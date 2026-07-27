//! Scalar JIT memory-load helper and speculative-access boundary.

use super::X86_64Vcpu;

/// Result of a JIT memory load: `value` in RAX, `ok` in RDX (SysV two-eightbyte
/// integer struct return). `ok == 0` signals a fault, MMIO, or unmapped access;
/// the native region then leaves before committing and direct execution owns
/// the single architecturally observable access.
#[repr(C)]
pub(super) struct JitLoadRet {
    value: u64,
    ok: u64,
}

/// Translate and read one ordinary-RAM scalar operand, sign- or zero-extending
/// it to 64 bits. The non-mutating RAM preflight is required because a caller
/// can discover a semantic fault only after seeing the value and deoptimize;
/// speculative MMIO must never be repeated by direct replay.
pub(super) unsafe extern "C" fn rax_jit_mem_load(
    ctx: *mut X86_64Vcpu,
    addr: u64,
    size: u32,
    signed: u32,
) -> JitLoadRet {
    let vcpu = unsafe { &mut *ctx };
    if !matches!(size, 1 | 2 | 4 | 8)
        || !vcpu
            .mmu
            .read_range_is_plain_ram(addr, size as usize, &vcpu.sregs)
    {
        return JitLoadRet { value: 0, ok: 0 };
    }

    match vcpu.read_mem(addr, size as u8) {
        Ok(val) => {
            let value = if signed != 0 {
                match size {
                    1 => val as u8 as i8 as i64 as u64,
                    2 => val as u16 as i16 as i64 as u64,
                    4 => val as u32 as i32 as i64 as u64,
                    _ => val,
                }
            } else {
                val
            };
            JitLoadRet { value, ok: 1 }
        }
        Err(_) => JitLoadRet { value: 0, ok: 0 },
    }
}
