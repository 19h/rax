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

/// Implicit system-segment selector exposed by SLDT or STR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86SystemSelector {
    Ldtr,
    Tr,
}

/// Architecturally distinct SLDT/STR destinations. Register forms write the
/// selected 16-, 32-, or 64-bit GPR width; memory forms always store exactly
/// the 16-bit selector independently of the encoded operand size.
#[derive(Clone, Debug)]
pub enum X86SystemSelectorTarget {
    Register { dst: VReg, width: OpWidth },
    Memory { addr: Address },
}

/// SLDT/STR read an implicit descriptor-register selector after protected-mode,
/// APX, and UMIP validation. A REX2 encoding requires the dynamic APX profile
/// even when it addresses only legacy GPRs.
#[derive(Clone, Debug)]
pub struct X86SystemSelectorStoreOp {
    pub selector: X86SystemSelector,
    pub target: X86SystemSelectorTarget,
    pub requires_apx: bool,
}

/// Architecturally fixed 16-bit source of LLDT/LTR. Operand-size prefixes do
/// not alter either register reads or memory-transfer width.
#[derive(Clone, Debug)]
pub enum X86SystemSelectorSource {
    Register { src: VReg },
    Memory { addr: Address },
}

/// Load one system-segment selector and its hidden descriptor cache. LTR also
/// performs the implicit available-to-busy GDT descriptor transition before
/// task-register commit. Successful execution serializes and hands off at
/// `next_pc`.
#[derive(Clone, Debug)]
pub struct X86SystemSelectorLoadOp {
    pub selector: X86SystemSelector,
    pub source: X86SystemSelectorSource,
    pub requires_apx: bool,
    pub next_pc: u64,
}

/// Indirect far JMP (`FF /5`) through a memory far pointer. The strict x86-64
/// lifter records the encoded 16-, 32-, or 64-bit offset width and produces the
/// target in architectural RIP. Descriptor-table reads, optional call-gate
/// indirection, the implicit code-descriptor accessed-bit write, and CS:RIP
/// commit are one fault-precise operation.
#[derive(Clone, Debug)]
pub struct X86FarJumpOp {
    pub addr: Address,
    pub target: VReg,
    pub offset_width: OpWidth,
    pub requires_apx: bool,
    /// Select #SS(0), rather than #GP(0), when the far-pointer linear address
    /// is noncanonical because the effective address uses SS.
    pub stack_segment: bool,
    /// Exact end of the source instruction, retained for native shape checks.
    pub next_pc: u64,
}

/// Indirect far CALL (`FF /3`) through a memory far pointer. Direct code
/// targets push a width-selected CS:return-IP frame; IA-32e call gates use
/// fixed 64-bit entries and may select a more-privileged TSS stack. Pointer,
/// descriptor, TSS, stack, accessed-bit, and CS:RIP:RSP[:SS] effects form one
/// fault-precise operation.
#[derive(Clone, Debug)]
pub struct X86FarCallOp {
    pub addr: Address,
    pub target: VReg,
    pub offset_width: OpWidth,
    pub requires_apx: bool,
    /// Select #SS(0), rather than #GP(0), for a noncanonical far-pointer range
    /// whose default segment is SS.
    pub stack_segment: bool,
    /// Architectural return address and exact source-instruction end.
    pub next_pc: u64,
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

/// Descriptor-table register selected by SGDT/SIDT/LGDT/LIDT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86DescriptorTable {
    Gdt,
    Idt,
}

/// SGDT/SIDT store the selected implicit descriptor-table register through a
/// memory-only operand. `SourceArch::X86_64` fixes the payload at 10 bytes:
/// the 16-bit limit followed by the 64-bit base. A REX2 encoding requires the
/// dynamic APX profile even when every address component is a legacy GPR.
#[derive(Clone, Debug)]
pub struct X86DescriptorTableStoreOp {
    pub addr: Address,
    pub table: X86DescriptorTable,
    pub requires_apx: bool,
}

/// LGDT/LIDT load the selected implicit descriptor-table register from one
/// memory-only operand. `SourceArch::X86_64` fixes the payload at 10 bytes:
/// the 16-bit limit followed by the complete 64-bit base. A successful native
/// execution serializes and hands off at `next_pc`; every fault restarts at the
/// original instruction without committing either field.
#[derive(Clone, Debug)]
pub struct X86DescriptorTableLoadOp {
    pub addr: Address,
    pub table: X86DescriptorTable,
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
