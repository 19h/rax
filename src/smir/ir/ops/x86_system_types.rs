//! Structured x86 system-operation payloads.

use crate::smir::ir::types::{Address, OpWidth, VReg};

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

/// x86 RDPMC read under the deterministic legacy-PMU profile. `selector` is
/// ECX; the operation validates privilege and selector state before committing
/// zero-extended EDX:EAX destinations.
#[derive(Clone, Debug)]
pub struct X86ReadPmcOp {
    pub dst_lo: VReg,
    pub dst_hi: VReg,
    pub selector: VReg,
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

/// Architecturally distinct SMSW destinations. Register forms write the
/// selected 16-, 32-, or 64-bit GPR width; memory forms always store exactly
/// CR0[15:0] as a 2-byte quantity independently of the encoded operand size.
#[derive(Clone, Debug)]
pub enum X86SmswTarget {
    Register { dst: VReg, width: OpWidth },
    Memory { addr: Address },
}

/// SMSW reads implicit CR0 state after dynamic APX and UMIP checks. A REX2
/// encoding sets `requires_apx` even when it addresses a legacy GPR, because
/// the prefix itself is unavailable when the guest APX profile is disabled.
#[derive(Clone, Debug)]
pub struct X86SmswOp {
    pub target: X86SmswTarget,
    pub requires_apx: bool,
}

/// Architecturally distinct LMSW sources. Both forms read exactly 16 bits;
/// operand-size prefixes never change the source width.
#[derive(Clone, Debug)]
pub enum X86LmswSource {
    Register { src: VReg },
    Memory { addr: Address },
}

/// LMSW reads its source only after dynamic APX and CPL validation, updates
/// CR0[3:0] without clearing an already-set CR0.PE, serializes execution, and
/// hands native execution off at the exact next instruction.
#[derive(Clone, Debug)]
pub struct X86LmswOp {
    pub source: X86LmswSource,
    pub requires_apx: bool,
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
