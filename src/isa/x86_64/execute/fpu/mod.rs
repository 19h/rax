//! x87 FPU instruction implementations.

mod escape_d8;
mod escape_d9;
mod escape_da;
mod escape_db;
mod escape_dc;
mod escape_dd;
mod escape_de;
mod escape_df;
pub mod helpers;

// Re-export escape functions
pub use escape_d8::escape_d8;
pub use escape_d9::escape_d9;
pub use escape_da::escape_da;
pub use escape_db::escape_db;
pub use escape_dc::escape_dc;
pub use escape_dd::escape_dd;
pub use escape_de::escape_de;
pub use escape_df::escape_df;

// Re-export public helper functions
pub use helpers::{f64_to_f80_pub, f80_to_f64_pub};

const CR0_EM: u64 = 1 << 2;
const CR0_NE: u64 = 1 << 5;
const CR0_TS: u64 = 1 << 3;
const FSW_ES: u16 = 1 << 7;

/// Deliver x87 device-not-available before a decoded instruction observes or
/// commits architectural state. Encoding and LOCK/REX2 validity are resolved
/// by the caller and common decoder first, preserving #UD priority.
fn require_x87_available(
    vcpu: &mut crate::isa::x86_64::cpu::X86_64Vcpu,
) -> crate::error::Result<bool> {
    if vcpu.sregs.cr0 & (CR0_EM | CR0_TS) != 0 {
        vcpu.inject_exception(7, None)?;
        return Ok(false);
    }
    Ok(true)
}

/// Deliver the two pre-execution faults shared by waiting x87 operations.
/// Device-not-available has priority over a pending floating-point error.
fn require_waiting_x87_available(
    vcpu: &mut crate::isa::x86_64::cpu::X86_64Vcpu,
) -> crate::error::Result<bool> {
    if !require_x87_available(vcpu)? {
        return Ok(false);
    }
    if vcpu.sregs.cr0 & CR0_NE != 0 && vcpu.fpu.status_word & FSW_ES != 0 {
        vcpu.inject_exception(16, None)?;
        return Ok(false);
    }
    Ok(true)
}

/// Record the deterministic profile's successful x87 non-control instruction
/// provenance. Faulting waiting instructions call this only after all guards.
fn record_x87_data_op(vcpu: &mut crate::isa::x86_64::cpu::X86_64Vcpu, fop: u16) {
    vcpu.fpu.instr_ptr = vcpu.regs.rip;
    vcpu.fpu.last_opcode = fop & 0x07FF;
}
