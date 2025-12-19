//! Software CPU emulator backend.
//!
//! This module provides a software-based x86_64 CPU emulator for cross-platform support.

pub mod x86_64;

use std::any::Any;
use std::sync::Arc;

use vm_memory::GuestMemoryMmap;

use crate::cpu::VCpu;
use crate::error::Result;

use super::{Backend, Vm};

/// Software emulator backend.
pub struct EmulatorBackend;

impl EmulatorBackend {
    pub fn new() -> Self {
        EmulatorBackend
    }
}

impl Backend for EmulatorBackend {
    fn name(&self) -> &'static str {
        "emulator"
    }

    fn create_vm(&self) -> Result<Box<dyn Vm>> {
        Ok(Box::new(EmulatorVm::new()))
    }
}

/// Emulated VM instance.
pub struct EmulatorVm {
    irq_pending: std::sync::Mutex<Vec<u32>>,
}

impl EmulatorVm {
    pub fn new() -> Self {
        EmulatorVm {
            irq_pending: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl Vm for EmulatorVm {
    fn create_vcpu(&self, id: u32, mem: Arc<GuestMemoryMmap>) -> Result<Box<dyn VCpu>> {
        Ok(Box::new(x86_64::X86_64Vcpu::new(id, mem)))
    }

    fn set_irq_line(&self, irq: u32, level: bool) -> Result<()> {
        if level {
            let mut pending = self.irq_pending.lock().unwrap();
            if !pending.contains(&irq) {
                pending.push(irq);
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
