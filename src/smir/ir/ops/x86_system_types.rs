//! Structured x86 system-operation payloads.

use crate::smir::ir::types::{Address, VReg};

/// x86 RDTSC/RDTSCP timestamp read. Both forms write EDX:EAX with 32-bit
/// zero-extending writes. `dst_aux == Some(ECX)` selects RDTSCP: it additionally
/// reads guest IA32_TSC_AUX and has the architectural prior-load ordering
/// guarantee. `None` selects the unordered RDTSC form.
#[derive(Clone, Debug)]
pub struct X86ReadTscOp {
    pub dst_lo: VReg,
    pub dst_hi: VReg,
    pub dst_aux: Option<VReg>,
}

/// MONITOR/MWAIT under the deterministic guest profile. `Some(addr)` is
/// MONITOR: `hint` is EDX, validate CPL/RCX, then perform an ordered faulting
/// byte read from the monitored linear address. `None` is MWAIT: `hint` is
/// EAX, validate CPL/RCX, and return immediately because the emulator does not
/// retain monitor hardware state. Hint values are implementation-dependent
/// and ignored by this profile. CPUID.05H advertises no MWAIT extensions, so
/// RCX must be zero for both forms in 64-bit mode.
#[derive(Clone, Debug)]
pub struct X86MonitorMwaitOp {
    pub rcx: VReg,
    pub hint: VReg,
    pub addr: Option<Address>,
    /// MONITOR used an SS override, selecting #SS(0) rather than #GP(0) for
    /// a noncanonical 64-bit linear address. Always false for MWAIT.
    pub stack_segment: bool,
}
