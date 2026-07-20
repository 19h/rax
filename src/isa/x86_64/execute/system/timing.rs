//! Timing instructions: RDTSC, RDTSCP, RDPMC.

use crate::error::Result;
use crate::vm::vcpu::VcpuExit;

use crate::isa::x86_64::cpu::{InsnContext, X86_64Vcpu};

const CR0_PE: u64 = 1;
const CR4_TSD: u64 = 1 << 2;
const CR4_PCE: u64 = 1 << 8;
const RFLAGS_VM: u64 = 1 << 17;

/// Deterministic legacy-PMU profile used while CPUID.0AH:EAX[7:0] is zero.
/// Intel defines 40-bit general-purpose counters for that profile and uses
/// ECX[30:0] as the model-specific counter index. RAX exposes eight such
/// counters, all backed by the unadjusted guest reference clock.
pub(crate) const X86_LEGACY_PMC_COUNT: u32 = 8;
pub(crate) const X86_LEGACY_PMC_WIDTH: u32 = 40;
const X86_LEGACY_PMC_MASK: u64 = (1_u64 << X86_LEGACY_PMC_WIDTH) - 1;
const X86_LEGACY_PMC_FAST: u32 = 1 << 31;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct X86PmcState {
    pub cr0: u64,
    pub cr4: u64,
    /// Effective CPL; virtual-8086 execution is represented as CPL3.
    pub cpl: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum X86PmcFault {
    GeneralProtection,
}

/// Evaluate one RDPMC against the deterministic legacy-PMU profile.
///
/// `counter_base` is an unadjusted guest reference-clock count. The operation
/// is pure so direct execution, canonical SMIR interpretation, and the native
/// helper share selector, privilege, width, and fast-read semantics.
pub(crate) fn read_x86_pmc(
    selector: u32,
    state: X86PmcState,
    counter_base: u64,
) -> core::result::Result<u64, X86PmcFault> {
    let permitted = state.cr0 & CR0_PE == 0 || state.cr4 & CR4_PCE != 0 || state.cpl == 0;
    let counter_index = selector & !X86_LEGACY_PMC_FAST;
    if !permitted || counter_index >= X86_LEGACY_PMC_COUNT {
        return Err(X86PmcFault::GeneralProtection);
    }

    let value = counter_base & X86_LEGACY_PMC_MASK;
    if selector & X86_LEGACY_PMC_FAST != 0 {
        Ok(value & u64::from(u32::MAX))
    } else {
        Ok(value)
    }
}

/// RDTSC/RDTSCP are available when protected mode is disabled, CR4.TSD is
/// clear, or execution is at CPL0. Virtual-8086 mode has effective CPL3.
fn timestamp_read_allowed(vcpu: &X86_64Vcpu) -> bool {
    vcpu.sregs.cr0 & CR0_PE == 0
        || vcpu.sregs.cr4 & CR4_TSD == 0
        || (vcpu.regs.rflags & RFLAGS_VM == 0 && vcpu.sregs.cs.selector & 3 == 0)
}

fn check_timestamp_read(vcpu: &mut X86_64Vcpu) -> Result<bool> {
    if timestamp_read_allowed(vcpu) {
        Ok(true)
    } else {
        vcpu.inject_exception(13, Some(0))?;
        Ok(false)
    }
}

/// RDTSC - Read Time-Stamp Counter (0x0F 0x31)
/// Reads 64-bit TSC into EDX:EAX. Upper 32 bits of RAX and RDX are cleared.
pub fn rdtsc(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    if !check_timestamp_read(vcpu)? {
        return Ok(None);
    }
    let tsc = vcpu.tsc();

    // EDX:EAX = TSC, upper 32 bits of RAX and RDX are cleared
    vcpu.regs.rax = tsc & 0xFFFF_FFFF;
    vcpu.regs.rdx = (tsc >> 32) & 0xFFFF_FFFF;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// RDTSCP - Read Time-Stamp Counter and Processor ID (0x0F 0x01 0xF9)
/// Reads 64-bit TSC into EDX:EAX, and IA32_TSC_AUX[31:0] into ECX.
/// Upper 32 bits of RAX, RDX, and RCX are cleared.
pub fn rdtscp(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    if !check_timestamp_read(vcpu)? {
        return Ok(None);
    }
    let tsc = vcpu.tsc();
    // EDX:EAX = TSC, upper 32 bits cleared
    vcpu.regs.rax = tsc & 0xFFFF_FFFF;
    vcpu.regs.rdx = (tsc >> 32) & 0xFFFF_FFFF;
    // ECX = IA32_TSC_AUX[31:0], upper 32 bits cleared
    vcpu.regs.rcx = vcpu.tsc_aux as u64;
    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

/// RDPMC - Read Performance Monitoring Counter (0x0F 0x33)
/// Reads the performance counter specified by ECX into EDX:EAX.
/// CPUID.0AH reports architectural-performance-monitoring version zero, so
/// ECX[30:0] selects one of eight model-specific 40-bit PMCs and ECX[31]
/// selects fast-read mode (low 32 bits in EAX, EDX cleared).
/// Upper 32 bits of RAX and RDX are cleared.
pub fn rdpmc(vcpu: &mut X86_64Vcpu, ctx: &mut InsnContext) -> Result<Option<VcpuExit>> {
    // APX REX2 defines no RDPMC encoding. Decode-time #UD takes precedence
    // over dynamic privilege and selector faults.
    if ctx.rex2.is_some() {
        return vcpu.inject_undefined_instruction();
    }

    let effective_cpl = if vcpu.regs.rflags & RFLAGS_VM != 0 {
        3
    } else {
        (vcpu.sregs.cs.selector & 3) as u8
    };
    // IA32_TSC_ADJUST changes only the timestamp-counter domain, not PMCs.
    let counter_base = vcpu.tsc().wrapping_sub(vcpu.tsc_adjust);
    let pmc_value = match read_x86_pmc(
        vcpu.regs.rcx as u32,
        X86PmcState {
            cr0: vcpu.sregs.cr0,
            cr4: vcpu.sregs.cr4,
            cpl: effective_cpl,
        },
        counter_base,
    ) {
        Ok(value) => value,
        Err(X86PmcFault::GeneralProtection) => {
            vcpu.inject_exception(13, Some(0))?;
            return Ok(None);
        }
    };

    vcpu.regs.rax = u64::from(pmc_value as u32);
    vcpu.regs.rdx = u64::from((pmc_value >> 32) as u32);

    vcpu.regs.rip += ctx.cursor as u64;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(cr0: u64, cr4: u64, cpl: u8) -> X86PmcState {
        X86PmcState { cr0, cr4, cpl }
    }

    #[test]
    fn legacy_pmc_selector_and_width_boundaries_are_exact() {
        let counter = 0xABCD_EF12_3456_7890;
        for selector in 0..X86_LEGACY_PMC_COUNT {
            assert_eq!(
                read_x86_pmc(selector, state(CR0_PE, 0, 0), counter),
                Ok(counter & X86_LEGACY_PMC_MASK)
            );
            assert_eq!(
                read_x86_pmc(selector | X86_LEGACY_PMC_FAST, state(CR0_PE, 0, 0), counter),
                Ok(counter & u64::from(u32::MAX))
            );
        }
        for selector in [8, 1 << 29, 1 << 30, u32::MAX] {
            assert_eq!(
                read_x86_pmc(selector, state(CR0_PE, 0, 0), counter),
                Err(X86PmcFault::GeneralProtection),
                "selector={selector:#010x}"
            );
        }
    }

    #[test]
    fn legacy_pmc_privilege_gate_models_pce_and_real_mode() {
        assert_eq!(
            read_x86_pmc(0, state(CR0_PE, 0, 3), 1),
            Err(X86PmcFault::GeneralProtection)
        );
        for allowed in [
            state(CR0_PE, 0, 0),
            state(CR0_PE, CR4_PCE, 3),
            state(0, 0, 3),
        ] {
            assert_eq!(read_x86_pmc(0, allowed, 1), Ok(1));
        }
    }
}
