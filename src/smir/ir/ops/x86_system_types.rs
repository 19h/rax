//! Structured x86 system-operation payloads.

use crate::smir::ir::types::{Address, VReg};

/// Architecturally readable x86 control registers accepted by `MOV r64, CRn`
/// in 64-bit mode. Reserved control-register numbers are represented as an
/// explicit invalid-opcode trap by the lifter and never reach this enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86ControlReg {
    Cr0,
    Cr2,
    Cr3,
    Cr4,
    Cr8,
}

/// Encoded debug-register selector accepted by `MOV r64, DRn` and
/// `MOV DRn, r64`. DR4 and DR5 remain explicit because their CR4.DE-dependent
/// invalidity and DR6/DR7 alias behavior are architectural runtime state, not
/// static decode properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86DebugReg {
    Dr0,
    Dr1,
    Dr2,
    Dr3,
    Dr4,
    Dr5,
    Dr6,
    Dr7,
}

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

/// RDMSR/WRMSR implicit-register operation. `write == false` reads the MSR
/// selected by ECX into zero-extended EDX:EAX. `write == true` writes the low
/// 32-bit EDX:EAX pair, preserves all three GPRs, and terminates native
/// execution at the exact `next_pc` after a successful state transition.
#[derive(Clone, Debug)]
pub struct X86MsrOp {
    pub eax: VReg,
    pub ecx: VReg,
    pub edx: VReg,
    pub write: bool,
    pub next_pc: u64,
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
