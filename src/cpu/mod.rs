//! CPU abstraction layer.
//!
//! This module provides backend-agnostic types and traits for CPU emulation.

pub mod exit;
pub mod state;

pub use exit::VcpuExit;
pub use state::{CpuState, DescriptorTable, Registers, Segment, SystemRegisters};

use crate::error::Result;

/// Abstract vCPU interface.
///
/// This trait is implemented by both KVM and emulator backends.
pub trait VCpu: Send {
    /// Run the vCPU until an exit condition.
    fn run(&mut self) -> Result<VcpuExit>;

    /// Get general-purpose registers.
    fn get_regs(&self) -> Result<Registers>;

    /// Set general-purpose registers.
    fn set_regs(&mut self, regs: &Registers) -> Result<()>;

    /// Get system registers.
    fn get_sregs(&self) -> Result<SystemRegisters>;

    /// Set system registers.
    fn set_sregs(&mut self, sregs: &SystemRegisters) -> Result<()>;

    /// Get complete CPU state.
    fn get_state(&self) -> Result<CpuState> {
        Ok(CpuState {
            regs: self.get_regs()?,
            sregs: self.get_sregs()?,
        })
    }

    /// Set complete CPU state.
    fn set_state(&mut self, state: &CpuState) -> Result<()> {
        self.set_regs(&state.regs)?;
        self.set_sregs(&state.sregs)?;
        Ok(())
    }

    /// Complete an I/O in operation by providing the data read from the device.
    fn complete_io_in(&mut self, data: &[u8]);

    /// Get vCPU ID.
    fn id(&self) -> u32;
}
