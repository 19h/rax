//! Shared direct and native-helper support for SGDT/SIDT memory stores.

use super::{Result, X86_64Vcpu};

impl X86_64Vcpu {
    /// Store a descriptor-table register as one MMU transaction. In 64-bit
    /// mode the payload is the fixed 10-byte limit:base form; legacy and
    /// compatibility modes use the 6-byte limit:base form. The logical trace
    /// remains split into scalar widths supported by JIT verification.
    #[inline]
    pub(in crate::isa::x86_64) fn write_descriptor_table_mem(
        &mut self,
        addr: u64,
        limit: u16,
        base: u64,
    ) -> Result<()> {
        let mut payload = [0u8; 10];
        payload[..2].copy_from_slice(&limit.to_le_bytes());
        let (len, base_size, traced_base) = if self.sregs.cs.l {
            payload[2..].copy_from_slice(&base.to_le_bytes());
            (10, 8, base)
        } else {
            let base32 = base as u32;
            payload[2..6].copy_from_slice(&base32.to_le_bytes());
            (6, 4, u64::from(base32))
        };
        let result = self.mmu.write(addr, &payload[..len], &self.sregs);
        #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
        if result.is_ok() {
            self.push_jit_mem_trace((1, addr, 2, u64::from(limit)));
            self.push_jit_mem_trace((1, addr.wrapping_add(2), base_size, traced_base));
        }
        result
    }
}

/// JIT SGDT/SIDT helper. The complete 10-byte long-mode payload is submitted
/// to the MMU in one transaction. Verify-mode undo records retain the existing
/// scalar log ABI as an adjacent 2-byte limit and 8-byte base pair.
#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
pub(super) unsafe extern "C" fn rax_jit_descriptor_table_store(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    addr: u64,
    table: u32,
) -> u64 {
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    let Some(vcpu) = (unsafe { (state.ctx as *mut X86_64Vcpu).as_mut() }) else {
        return 0;
    };
    if !vcpu.sregs.cs.l {
        return 0;
    }
    let Some(last) = addr.checked_add(9) else {
        return 0;
    };
    if vcpu.mmu.is_code_page(addr) || vcpu.mmu.is_code_page(last) {
        return 0;
    }
    let (limit, base) = match table {
        0 => (vcpu.sregs.gdt.limit, vcpu.sregs.gdt.base),
        1 => (vcpu.sregs.idt.limit, vcpu.sregs.idt.base),
        _ => return 0,
    };

    if vcpu.jit_mem_log.is_some() {
        match vcpu.read_bytes(addr, 10) {
            Ok(old) => {
                vcpu.push_jit_mem_log((
                    addr,
                    2,
                    u64::from(u16::from_le_bytes(old[..2].try_into().unwrap())),
                ));
                if vcpu.jit_mem_log.is_some() {
                    vcpu.push_jit_mem_log((
                        addr.wrapping_add(2),
                        8,
                        u64::from_le_bytes(old[2..].try_into().unwrap()),
                    ));
                }
            }
            Err(_) => vcpu.jit_mem_log = None,
        }
    }

    u64::from(vcpu.write_descriptor_table_mem(addr, limit, base).is_ok())
}
