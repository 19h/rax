//! SMIR call targets and runtime-call identifiers.

use super::types::{Address, FunctionId, GuestAddr, VReg};

/// Call target.
#[derive(Clone, Debug)]
pub enum CallTarget {
    /// Direct call to known function.
    Direct(FunctionId),
    /// Direct call to guest address.
    GuestAddr(GuestAddr),
    /// Direct AArch32 interworking call. `addr` is the architectural target PC
    /// (with no state tag in bit 0), while `thumb` is the execution state the
    /// dispatcher must install before resuming the guest.
    GuestAddrInterworking { addr: GuestAddr, thumb: bool },
    /// Indirect call through register.
    Indirect(VReg),
    /// AArch32 register interworking call. Bit 0 of the W32 target selects the
    /// execution state and is cleared from the architectural target PC.
    IndirectInterworking(VReg),
    /// Indirect call through memory using the address's ordinary width rules.
    IndirectMem(Address),
    /// x86-64 `r/m64` indirect call with a 32-bit effective-address size.
    /// Base, index, scale, and displacement are evaluated modulo 2^32 and
    /// zero-extended before an optional FS/GS segment base is added.
    X86IndirectMemAddr32(Address),
    /// External runtime function.
    Runtime(RuntimeFunc),
}

impl CallTarget {
    /// Stable diagnostic name for this target representation.
    pub(crate) fn kind_name(&self) -> &'static str {
        match self {
            Self::Direct(_) => "DirectFn",
            Self::GuestAddr(_) => "GuestAddr",
            Self::GuestAddrInterworking { .. } => "GuestAddrInterworking",
            Self::Indirect(_) => "IndirectReg",
            Self::IndirectInterworking(_) => "IndirectInterworking",
            Self::IndirectMem(_) => "IndirectMem",
            Self::X86IndirectMemAddr32(_) => "X86IndirectMemAddr32",
            Self::Runtime(_) => "Runtime",
        }
    }

    /// Registers read while resolving this target.
    pub(crate) fn regs(&self) -> Vec<VReg> {
        match self {
            Self::Indirect(reg) | Self::IndirectInterworking(reg) => vec![*reg],
            Self::IndirectMem(addr) | Self::X86IndirectMemAddr32(addr) => addr.regs(),
            Self::Direct(_)
            | Self::GuestAddr(_)
            | Self::GuestAddrInterworking { .. }
            | Self::Runtime(_) => Vec::new(),
        }
    }
}

/// Runtime helper functions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeFunc {
    /// System call handler.
    Syscall,
    /// Page fault handler.
    PageFault,
    /// FP exception handler.
    FpException,
    /// Undefined instruction handler.
    Undefined,
    /// Debug breakpoint.
    Breakpoint,
    /// Memory barrier (fence).
    MemoryBarrier,
    /// CPUID (x86).
    Cpuid,
    /// Read timestamp counter.
    Rdtsc,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smir::ir::types::{ArchReg, DispSize, X86Reg};

    #[test]
    fn addr32_memory_target_reports_every_address_register() {
        let base = VReg::Arch(ArchReg::X86(X86Reg::R31));
        let index = VReg::Arch(ArchReg::X86(X86Reg::R16));
        let target = CallTarget::X86IndirectMemAddr32(Address::BaseIndexScale {
            base: Some(base),
            index,
            scale: 8,
            disp: -1,
            disp_size: DispSize::Disp32,
        });
        assert_eq!(target.regs(), vec![index, base]);
    }
}
