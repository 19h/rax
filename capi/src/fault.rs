//! Typed execution faults. Querying diagnostics never clears the underlying error.
use crate::engine::{Engine, engine_ref};
use crate::{RaxStatus, guard};
use rax_engine::error::{Error, MemoryAccessKind, MemoryFaultKind};

pub const RAX_FAULT_INFO_VERSION: u32 = 1;
pub const RAX_FAULT_NONE: u32 = 0;
pub const RAX_FAULT_UNMAPPED: u32 = 1;
pub const RAX_FAULT_PERMISSION: u32 = 2;
pub const RAX_FAULT_INVALID_INSTRUCTION: u32 = 3;
pub const RAX_FAULT_OTHER: u32 = 4;
pub const RAX_FAULT_ACCESS_NONE: u32 = 0;
pub const RAX_FAULT_ACCESS_READ: u32 = 1;
pub const RAX_FAULT_ACCESS_WRITE: u32 = 2;
pub const RAX_FAULT_ACCESS_FETCH: u32 = 3;
pub const RAX_FAULT_ADDRESS_VALID: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RaxFaultInfo {
    pub struct_size: u32,
    pub version: u32,
    pub kind: u32,
    pub access: u32,
    pub pc: u64,
    pub address: u64,
    pub size: u32,
    pub flags: u32,
    pub retired_instructions: u64,
}
impl Default for RaxFaultInfo {
    fn default() -> Self {
        Self {
            struct_size: std::mem::size_of::<Self>() as u32,
            version: RAX_FAULT_INFO_VERSION,
            kind: RAX_FAULT_NONE,
            access: RAX_FAULT_ACCESS_NONE,
            pc: 0,
            address: 0,
            size: 0,
            flags: 0,
            retired_instructions: 0,
        }
    }
}
impl RaxFaultInfo {
    pub(crate) fn from_error(pc: u64, error: &Error) -> Self {
        let mut info = Self {
            pc,
            kind: RAX_FAULT_OTHER,
            ..Self::default()
        };
        match error {
            Error::FaultDelivery { fault, .. } => return Self::from_error(pc, fault),
            Error::GuestAccess(fault) => {
                info.kind = match fault.kind {
                    MemoryFaultKind::Unmapped => RAX_FAULT_UNMAPPED,
                    MemoryFaultKind::Permission => RAX_FAULT_PERMISSION,
                    MemoryFaultKind::Other => RAX_FAULT_OTHER,
                };
                info.access = match fault.access {
                    MemoryAccessKind::Read => RAX_FAULT_ACCESS_READ,
                    MemoryAccessKind::Write => RAX_FAULT_ACCESS_WRITE,
                    MemoryAccessKind::Fetch => RAX_FAULT_ACCESS_FETCH,
                };
                info.address = fault.address;
                info.size = fault.size;
                info.flags = RAX_FAULT_ADDRESS_VALID;
            }
            // Guest page-table faults are not missing host backing. An embedder
            // must not map physical memory using this virtual address.
            Error::PageFault { vaddr, error_code } => {
                info.kind = if error_code & 1 != 0 {
                    RAX_FAULT_PERMISSION
                } else {
                    RAX_FAULT_OTHER
                };
                info.address = *vaddr;
                info.flags = RAX_FAULT_ADDRESS_VALID;
                info.access = if error_code & 0x10 != 0 {
                    RAX_FAULT_ACCESS_FETCH
                } else if error_code & 2 != 0 {
                    RAX_FAULT_ACCESS_WRITE
                } else {
                    RAX_FAULT_ACCESS_READ
                };
            }
            Error::AlignmentFault {
                pc,
                address,
                alignment,
                access,
            } => {
                info.pc = *pc;
                info.address = *address;
                info.size = *alignment;
                info.flags = RAX_FAULT_ADDRESS_VALID;
                info.access = match access {
                    MemoryAccessKind::Read => RAX_FAULT_ACCESS_READ,
                    MemoryAccessKind::Write => RAX_FAULT_ACCESS_WRITE,
                    MemoryAccessKind::Fetch => RAX_FAULT_ACCESS_FETCH,
                };
            }
            Error::GeneralProtection { .. } => info.kind = RAX_FAULT_PERMISSION,
            Error::InvalidInstruction { pc, .. } => {
                info.kind = RAX_FAULT_INVALID_INSTRUCTION;
                info.pc = *pc;
            }
            _ => {}
        }
        info
    }
}

/// `out` must point to writable storage whose initialized size/version header
/// describes at least one v1 record. Only the v1 bytes are written; a future
/// caller's tail remains untouched. NULL, short buffers, and unknown versions
/// fail without modifying the caller's output or the stored execution fault.
#[unsafe(no_mangle)]
pub extern "C" fn rax_emu_last_fault(engine: *const Engine, out: *mut RaxFaultInfo) -> RaxStatus {
    guard(|| {
        // SAFETY: the C API requires a valid handle and, when non-NULL, aligned
        // caller-owned storage for the declared record. Validate the header
        // before writing the v1 record; no reference escapes this call.
        let Some(engine) = (unsafe { engine_ref(engine) }) else {
            return RaxStatus::Handle;
        };
        if out.is_null() {
            return RaxStatus::Arg;
        }
        unsafe {
            if (*out).struct_size < std::mem::size_of::<RaxFaultInfo>() as u32 {
                return RaxStatus::Arg;
            }
            if (*out).version != RAX_FAULT_INFO_VERSION {
                return RaxStatus::Unsupported;
            }
            out.write(engine.last_fault);
        }
        RaxStatus::Ok
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hexagon_alignment_faults_reach_the_public_execution_and_diagnostic_apis() {
        use crate::arch::RaxArch;
        use crate::engine::{rax_engine_close, rax_engine_open};
        use crate::mem::{rax_mem_read, rax_mem_write};
        use crate::reg::{rax_reg_read_u64, rax_reg_write_u64};
        use crate::run::{rax_emu_icount, rax_emu_step};

        struct Handle(*mut Engine);
        impl Drop for Handle {
            fn drop(&mut self) {
                rax_engine_close(self.0);
            }
        }

        for (word, pc, address, access) in [
            (0x9181c003_u32, 0x1000, 0x2001, RAX_FAULT_ACCESS_READ),
            (0xa181c200, 0x1000, 0x2001, RAX_FAULT_ACCESS_WRITE),
            (0x9181c003, 0x1001, 0x1001, RAX_FAULT_ACCESS_FETCH),
        ] {
            let mut engine = std::ptr::null_mut();
            assert_eq!(
                rax_engine_open(RaxArch::Hexagon as i32, 0, &mut engine),
                RaxStatus::Ok
            );
            let handle = Handle(engine);
            assert_eq!(
                rax_mem_write(handle.0, pc, word.to_le_bytes().as_ptr(), 4),
                RaxStatus::Ok
            );
            assert_eq!(
                rax_mem_write(handle.0, 0x2000, [0xa5_u8; 8].as_ptr(), 8),
                RaxStatus::Ok
            );
            assert_eq!(rax_reg_write_u64(handle.0, 0x0101, 0x2001), RaxStatus::Ok);
            assert_eq!(rax_reg_write_u64(handle.0, 0x0309, pc), RaxStatus::Ok);
            let mut done = u64::MAX;
            assert_eq!(rax_emu_step(handle.0, 1, &mut done), RaxStatus::Fault);
            assert_eq!(done, 0);
            assert_eq!(rax_emu_icount(handle.0), 0);
            for _ in 0..2 {
                let mut info = RaxFaultInfo::default();
                assert_eq!(rax_emu_last_fault(handle.0, &mut info), RaxStatus::Ok);
                assert_eq!((info.kind, info.access), (RAX_FAULT_OTHER, access));
                assert_eq!((info.pc, info.address), (pc, address));
                assert_eq!((info.size, info.flags), (4, RAX_FAULT_ADDRESS_VALID));
                assert_eq!(info.retired_instructions, 0);
            }
            let mut after_pc = 0;
            assert_eq!(
                rax_reg_read_u64(handle.0, 0x0309, &mut after_pc),
                RaxStatus::Ok
            );
            assert_eq!(after_pc, pc);
            let mut bytes = [0; 8];
            assert_eq!(
                rax_mem_read(handle.0, 0x2000, bytes.as_mut_ptr(), 8),
                RaxStatus::Ok
            );
            assert_eq!(bytes, [0xa5; 8]);
        }
    }

    #[test]
    fn alignment_diagnostics_preserve_v1_layout_and_precise_access_fields() {
        assert_eq!(std::mem::size_of::<RaxFaultInfo>(), 48);
        for (access, expected) in [
            (MemoryAccessKind::Read, RAX_FAULT_ACCESS_READ),
            (MemoryAccessKind::Write, RAX_FAULT_ACCESS_WRITE),
            (MemoryAccessKind::Fetch, RAX_FAULT_ACCESS_FETCH),
        ] {
            let error = Error::AlignmentFault {
                pc: 0x1000,
                address: 0x2001,
                alignment: 8,
                access,
            };
            assert_eq!(
                crate::status::status_from_engine_error(&error),
                RaxStatus::Fault
            );
            let info = RaxFaultInfo::from_error(0xdead, &error);
            assert_eq!(info.version, 1);
            assert_eq!(info.kind, RAX_FAULT_OTHER);
            assert_eq!(info.access, expected);
            assert_eq!(info.pc, 0x1000);
            assert_eq!(info.address, 0x2001);
            assert_eq!(info.size, 8);
            assert_eq!(info.flags, RAX_FAULT_ADDRESS_VALID);
        }
    }
}
