//! CPU abstraction layer.
//!
//! This module provides backend-agnostic types and traits for CPU emulation.

pub mod exit;
pub mod state;

pub use exit::VcpuExit;
pub use state::{
    CpuState, DescriptorTable, HexagonCpuState, HexagonRegisters, Registers, Segment,
    SystemRegisters, X86_64CpuState,
};

use crate::error::{Error, Result};

/// Abstract vCPU interface.
///
/// This trait is implemented by both KVM and emulator backends.
pub trait VCpu: Send {
    /// Run the vCPU until an exit condition.
    fn run(&mut self) -> Result<VcpuExit>;

    /// Get general-purpose registers (x86_64 only).
    fn get_regs(&self) -> Result<Registers> {
        match self.get_state()? {
            CpuState::X86_64(state) => Ok(state.regs),
            _ => Err(Error::InvalidConfig(
                "register access is only supported for x86_64".to_string(),
            )),
        }
    }

    /// Set general-purpose registers (x86_64 only).
    fn set_regs(&mut self, regs: &Registers) -> Result<()> {
        match self.get_state()? {
            CpuState::X86_64(state) => self.set_state(&CpuState::x86_64(regs.clone(), state.sregs)),
            _ => Err(Error::InvalidConfig(
                "register access is only supported for x86_64".to_string(),
            )),
        }
    }

    /// Get system registers (x86_64 only).
    fn get_sregs(&self) -> Result<SystemRegisters> {
        match self.get_state()? {
            CpuState::X86_64(state) => Ok(state.sregs),
            _ => Err(Error::InvalidConfig(
                "system register access is only supported for x86_64".to_string(),
            )),
        }
    }

    /// Set system registers (x86_64 only).
    fn set_sregs(&mut self, sregs: &SystemRegisters) -> Result<()> {
        match self.get_state()? {
            CpuState::X86_64(state) => self.set_state(&CpuState::x86_64(state.regs, sregs.clone())),
            _ => Err(Error::InvalidConfig(
                "system register access is only supported for x86_64".to_string(),
            )),
        }
    }

    /// Get complete CPU state.
    fn get_state(&self) -> Result<CpuState>;

    /// Set complete CPU state.
    fn set_state(&mut self, state: &CpuState) -> Result<()>;

    /// Complete an I/O in operation by providing the data read from the device.
    fn complete_io_in(&mut self, data: &[u8]);

    /// Inject an external interrupt (hardware IRQ).
    /// Returns Ok(true) if the interrupt was injected, Ok(false) if interrupts are disabled.
    fn inject_interrupt(&mut self, vector: u8) -> Result<bool> {
        // Default implementation does nothing
        let _ = vector;
        Ok(false)
    }

    /// Check if interrupts are enabled and can be injected.
    fn can_inject_interrupt(&self) -> bool {
        false
    }

    /// Enable or disable single-step mode for debugging.
    #[cfg(feature = "debug")]
    fn set_single_step(&mut self, enabled: bool) {
        let _ = enabled;
    }

    /// Check if single-step mode is enabled.
    #[cfg(feature = "debug")]
    fn is_single_step(&self) -> bool {
        false
    }

    /// Invalidate any cached instruction decodes for the given address.
    /// Called when modifying code memory (e.g., for software breakpoints).
    #[cfg(feature = "debug")]
    fn invalidate_code_cache(&mut self, addr: u64) {
        let _ = addr;
    }

    /// Get vCPU ID.
    fn id(&self) -> u32;
}
