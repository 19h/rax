//! Apple Hypervisor.framework ARM64 backend implementation.
//!
//! This backend uses Apple's Hypervisor.framework to provide hardware-accelerated
//! virtualization for AArch64 guests on Apple Silicon Macs.
//!
//! Note: This is only compiled on aarch64 macOS targets.

#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use std::any::Any;
use std::ptr;
use std::sync::{Arc, Mutex};

use tracing::{debug, info, warn};
use vm_memory::{Address, GuestAddress, GuestMemory, GuestMemoryMmap, GuestMemoryRegion};

use crate::cpu::{
    Aarch64CpuState, Aarch64Registers, Aarch64SystemRegisters, CpuState, VCpu, VcpuExit,
};
use crate::error::{Error, Result};
use crate::memory::GuestMemoryWrapper;

use super::arm64_bindings::*;
use super::{Backend, Vm};

/// ARM64 HVF backend.
pub struct HvfArm64Backend;

impl HvfArm64Backend {
    pub fn new() -> Result<Self> {
        // Check if ARM64 HVF is available
        if let Err(msg) = hv_arm64_check_available() {
            return Err(Error::InvalidConfig(msg.to_string()));
        }

        info!("ARM64 HVF backend initialized for Apple Silicon");
        Ok(HvfArm64Backend)
    }
}

impl Backend for HvfArm64Backend {
    fn name(&self) -> &'static str {
        "hvf-arm64"
    }

    fn create_vm(&self) -> Result<Box<dyn Vm>> {
        // Create VM (NULL config for default settings)
        let ret = unsafe { hv_vm_create(ptr::null()) };
        if ret != HV_SUCCESS {
            return Err(Error::Emulator(format!(
                "Failed to create ARM64 VM: {}",
                hv_error_string(ret)
            )));
        }

        info!("Created ARM64 HVF VM");

        Ok(Box::new(HvfArm64Vm {
            pending_irq: Mutex::new(false),
            pending_fiq: Mutex::new(false),
            memory_mapped: Mutex::new(false),
        }))
    }
}

/// ARM64 HVF VM instance.
pub struct HvfArm64Vm {
    /// Pending IRQ
    pending_irq: Mutex<bool>,
    /// Pending FIQ
    pending_fiq: Mutex<bool>,
    /// Whether memory has been mapped
    memory_mapped: Mutex<bool>,
}

impl HvfArm64Vm {
    /// Register guest memory with the VM.
    pub fn register_memory(&self, mem: &GuestMemoryWrapper) -> Result<()> {
        let mut mapped = self.memory_mapped.lock().unwrap();
        if *mapped {
            return Ok(());
        }

        for region in mem.memory().iter() {
            let guest_addr = region.start_addr().0;
            let size = region.len() as usize;
            let host_addr = region.as_ptr() as *mut std::ffi::c_void;

            debug!(
                guest_addr = format!("{:#x}", guest_addr),
                size = size,
                "Mapping guest memory region"
            );

            let flags = HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC;
            let ret = unsafe { hv_vm_map(host_addr, guest_addr, size, flags) };
            if ret != HV_SUCCESS {
                return Err(Error::Emulator(format!(
                    "Failed to map memory at {:#x}: {}",
                    guest_addr,
                    hv_error_string(ret)
                )));
            }
        }

        *mapped = true;
        info!("Mapped guest memory to ARM64 HVF VM");
        Ok(())
    }
}

impl Vm for HvfArm64Vm {
    fn create_vcpu(&self, id: u32, mem: Arc<GuestMemoryMmap>) -> Result<Box<dyn VCpu>> {
        let vcpu = HvfArm64Vcpu::new(id, mem)?;
        Ok(Box::new(vcpu))
    }

    fn set_irq_line(&self, irq: u32, level: bool) -> Result<()> {
        // For ARM64, we track IRQ/FIQ state and inject via vCPU
        // IRQ 0 = IRQ line, IRQ 1 = FIQ line (simplified model)
        match irq {
            0 => {
                let mut pending = self.pending_irq.lock().unwrap();
                *pending = level;
            }
            1 => {
                let mut pending = self.pending_fiq.lock().unwrap();
                *pending = level;
            }
            _ => {
                // For GIC integration, we'd route through a proper interrupt controller
                debug!(irq, level, "IRQ line change (GIC integration needed)");
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Drop for HvfArm64Vm {
    fn drop(&mut self) {
        let ret = unsafe { hv_vm_destroy() };
        if ret != HV_SUCCESS {
            warn!("Failed to destroy ARM64 VM: {}", hv_error_string(ret));
        }
    }
}

/// ARM64 HVF vCPU.
pub struct HvfArm64Vcpu {
    /// vCPU handle
    vcpu: hv_vcpu_t,
    /// Exit information pointer
    exit: hv_vcpu_exit_t,
    /// Our vCPU ID (index)
    id: u32,
    /// Guest memory reference
    mem: Arc<GuestMemoryMmap>,
    /// Whether the vCPU is halted (WFI)
    halted: bool,
    /// Pending IRQ to inject
    pending_irq: bool,
    /// Pending FIQ to inject
    pending_fiq: bool,
}

// Safety: HvfArm64Vcpu is Send because the vCPU handle is thread-local,
// and we synchronize access appropriately.
unsafe impl Send for HvfArm64Vcpu {}

impl HvfArm64Vcpu {
    fn new(id: u32, mem: Arc<GuestMemoryMmap>) -> Result<Self> {
        let mut vcpu: hv_vcpu_t = ptr::null_mut();
        let mut exit: hv_vcpu_exit_t = ptr::null_mut();

        let ret = unsafe { hv_vcpu_create(&mut vcpu, &mut exit, ptr::null()) };
        if ret != HV_SUCCESS {
            return Err(Error::Emulator(format!(
                "Failed to create ARM64 vCPU {}: {}",
                id,
                hv_error_string(ret)
            )));
        }

        debug!(id, vcpu = ?vcpu, "Created ARM64 HVF vCPU");

        Ok(HvfArm64Vcpu {
            vcpu,
            exit,
            id,
            mem,
            halted: false,
            pending_irq: false,
            pending_fiq: false,
        })
    }

    /// Read a general-purpose register
    fn read_gpr(&self, reg: hv_reg_t) -> Result<u64> {
        let mut value: u64 = 0;
        let ret = unsafe { hv_vcpu_get_reg(self.vcpu, reg, &mut value) };
        if ret != HV_SUCCESS {
            return Err(Error::Emulator(format!(
                "Failed to read register {}: {}",
                reg,
                hv_error_string(ret)
            )));
        }
        Ok(value)
    }

    /// Write a general-purpose register
    fn write_gpr(&self, reg: hv_reg_t, value: u64) -> Result<()> {
        let ret = unsafe { hv_vcpu_set_reg(self.vcpu, reg, value) };
        if ret != HV_SUCCESS {
            return Err(Error::Emulator(format!(
                "Failed to write register {}: {}",
                reg,
                hv_error_string(ret)
            )));
        }
        Ok(())
    }

    /// Read a system register
    fn read_sys_reg(&self, reg: hv_sys_reg_t) -> Result<u64> {
        let mut value: u64 = 0;
        let ret = unsafe { hv_vcpu_get_sys_reg(self.vcpu, reg, &mut value) };
        if ret != HV_SUCCESS {
            return Err(Error::Emulator(format!(
                "Failed to read system register {:#x}: {}",
                reg,
                hv_error_string(ret)
            )));
        }
        Ok(value)
    }

    /// Write a system register
    fn write_sys_reg(&self, reg: hv_sys_reg_t, value: u64) -> Result<()> {
        let ret = unsafe { hv_vcpu_set_sys_reg(self.vcpu, reg, value) };
        if ret != HV_SUCCESS {
            return Err(Error::Emulator(format!(
                "Failed to write system register {:#x}: {}",
                reg,
                hv_error_string(ret)
            )));
        }
        Ok(())
    }

    /// Read a SIMD/FP register
    fn read_simd_reg(&self, reg: hv_simd_fp_reg_t) -> Result<[u64; 2]> {
        let mut value = hv_simd_fp_uchar16_t::default();
        let ret = unsafe { hv_vcpu_get_simd_fp_reg(self.vcpu, reg, &mut value) };
        if ret != HV_SUCCESS {
            return Err(Error::Emulator(format!(
                "Failed to read SIMD register {}: {}",
                reg,
                hv_error_string(ret)
            )));
        }
        Ok(value.as_u64_pair())
    }

    /// Write a SIMD/FP register
    fn write_simd_reg(&self, reg: hv_simd_fp_reg_t, value: [u64; 2]) -> Result<()> {
        let value = hv_simd_fp_uchar16_t::from_u64_pair(value);
        let ret = unsafe { hv_vcpu_set_simd_fp_reg(self.vcpu, reg, value) };
        if ret != HV_SUCCESS {
            return Err(Error::Emulator(format!(
                "Failed to write SIMD register {}: {}",
                reg,
                hv_error_string(ret)
            )));
        }
        Ok(())
    }

    /// Advance PC past the current instruction
    fn advance_pc(&self, insn_len: u64) -> Result<()> {
        let pc = self.read_gpr(HV_REG_PC)?;
        self.write_gpr(HV_REG_PC, pc + insn_len)
    }

    /// Try to inject pending interrupts
    fn try_inject_interrupts(&mut self) -> Result<()> {
        if self.pending_irq {
            let ret =
                unsafe { hv_vcpu_set_pending_interrupt(self.vcpu, HV_INTERRUPT_TYPE_IRQ, true) };
            if ret == HV_SUCCESS {
                self.pending_irq = false;
                self.halted = false;
            }
        }

        if self.pending_fiq {
            let ret =
                unsafe { hv_vcpu_set_pending_interrupt(self.vcpu, HV_INTERRUPT_TYPE_FIQ, true) };
            if ret == HV_SUCCESS {
                self.pending_fiq = false;
                self.halted = false;
            }
        }

        Ok(())
    }

    /// Handle a data abort exception
    fn handle_data_abort(&self, exit_info: &hv_vcpu_exit) -> Result<VcpuExit> {
        let exc = &exit_info.exception;
        let addr = exc.physical_address;
        let is_write = exc.is_write();
        let size = 1 << exc.access_size(); // Convert to byte count

        if is_write {
            // For writes, we need to read the value from the source register
            let rt = exc.srt();
            let data = if rt < 31 {
                self.read_gpr(rt)?
            } else {
                0 // XZR
            };

            let data_bytes = match size {
                1 => vec![data as u8],
                2 => (data as u16).to_le_bytes().to_vec(),
                4 => (data as u32).to_le_bytes().to_vec(),
                8 => data.to_le_bytes().to_vec(),
                _ => vec![data as u8],
            };

            Ok(VcpuExit::MmioWrite {
                addr,
                data: data_bytes,
            })
        } else {
            Ok(VcpuExit::MmioRead {
                addr,
                size: size as u8,
            })
        }
    }
}

impl VCpu for HvfArm64Vcpu {
    fn run(&mut self) -> Result<VcpuExit> {
        // Try to inject any pending interrupts
        self.try_inject_interrupts()?;

        // If halted and no interrupt pending, return Hlt
        if self.halted && !self.pending_irq && !self.pending_fiq {
            return Ok(VcpuExit::Hlt);
        }

        // Run the vCPU
        let ret = unsafe { hv_vcpu_run(self.vcpu) };
        if ret != HV_SUCCESS {
            return Err(Error::Emulator(format!(
                "hv_vcpu_run failed: {}",
                hv_error_string(ret)
            )));
        }

        // Get exit information
        let exit_info = unsafe { &*self.exit };

        match exit_info.reason {
            hv_exit_reason_t::HV_EXIT_REASON_CANCELED => {
                // vCPU was canceled (e.g., by hv_vcpu_force_exit)
                Ok(VcpuExit::Shutdown)
            }

            hv_exit_reason_t::HV_EXIT_REASON_EXCEPTION => {
                let exc = &exit_info.exception;
                let ec = exc.exception_class();

                match ec {
                    EC_WFI_WFE => {
                        // WFI/WFE - halt until interrupt
                        self.halted = true;
                        self.advance_pc(exc.instruction_length() as u64)?;
                        Ok(VcpuExit::Hlt)
                    }

                    EC_DATA_ABORT_LOWER | EC_DATA_ABORT_CURR => {
                        // Data abort - MMIO access
                        self.handle_data_abort(exit_info)
                    }

                    EC_INST_ABORT_LOWER | EC_INST_ABORT_CURR => {
                        // Instruction abort
                        let addr = exc.physical_address;
                        Ok(VcpuExit::Unknown(format!(
                            "Instruction abort at {:#x}",
                            addr
                        )))
                    }

                    EC_HVC64 => {
                        // Hypervisor call - could be used for paravirtualization
                        let imm = exc.iss() & 0xFFFF;
                        debug!(imm, "HVC call");
                        self.advance_pc(4)?;
                        self.run()
                    }

                    EC_SMC64 => {
                        // Secure Monitor Call - we don't support secure world
                        debug!("SMC call (not supported)");
                        self.advance_pc(4)?;
                        self.run()
                    }

                    EC_MSR_MRS => {
                        // System register access trap
                        // For now, skip and continue
                        debug!(
                            syndrome = format!("{:#x}", exc.syndrome),
                            "Trapped system register access"
                        );
                        self.advance_pc(4)?;
                        self.run()
                    }

                    EC_SVC64 => {
                        // Supervisor call - let the guest handle it
                        Ok(VcpuExit::Exception(ec as u8))
                    }

                    _ => {
                        let pc = self.read_gpr(HV_REG_PC).unwrap_or(0);
                        Ok(VcpuExit::Unknown(format!(
                            "Exception class {:#x} at PC {:#x}, syndrome {:#x}",
                            ec, pc, exc.syndrome
                        )))
                    }
                }
            }

            hv_exit_reason_t::HV_EXIT_REASON_VTIMER_ACTIVATED => {
                // Virtual timer fired
                debug!("VTimer activated");
                // Signal timer interrupt and continue
                self.pending_irq = true;
                self.run()
            }

            hv_exit_reason_t::HV_EXIT_REASON_UNKNOWN => {
                let pc = self.read_gpr(HV_REG_PC).unwrap_or(0);
                Ok(VcpuExit::Unknown(format!("Unknown exit at PC {:#x}", pc)))
            }
        }
    }

    fn get_state(&self) -> Result<CpuState> {
        let mut regs = Aarch64Registers::default();

        // Read X0-X30
        for i in 0..31 {
            regs.x[i] = self.read_gpr(i as u32)?;
        }

        // Read PC, SP, PSTATE
        regs.pc = self.read_gpr(HV_REG_PC)?;
        regs.sp = self.read_sys_reg(HV_SYS_REG_SP_EL0)?;
        regs.pstate = self.read_gpr(HV_REG_CPSR)?;

        // Read FPCR/FPSR
        regs.fpcr = self.read_gpr(HV_REG_FPCR)? as u32;
        regs.fpsr = self.read_gpr(HV_REG_FPSR)? as u32;

        // Read V0-V31
        for i in 0..32 {
            regs.v[i] = self.read_simd_reg(i as u32)?;
        }

        // Read system registers
        let mut sregs = Aarch64SystemRegisters::default();
        sregs.sctlr_el1 = self.read_sys_reg(HV_SYS_REG_SCTLR_EL1)?;
        sregs.tcr_el1 = self.read_sys_reg(HV_SYS_REG_TCR_EL1)?;
        sregs.ttbr0_el1 = self.read_sys_reg(HV_SYS_REG_TTBR0_EL1)?;
        sregs.ttbr1_el1 = self.read_sys_reg(HV_SYS_REG_TTBR1_EL1)?;
        sregs.mair_el1 = self.read_sys_reg(HV_SYS_REG_MAIR_EL1)?;
        sregs.vbar_el1 = self.read_sys_reg(HV_SYS_REG_VBAR_EL1)?;
        sregs.esr_el1 = self.read_sys_reg(HV_SYS_REG_ESR_EL1)?;
        sregs.far_el1 = self.read_sys_reg(HV_SYS_REG_FAR_EL1)?;
        sregs.elr_el1 = self.read_sys_reg(HV_SYS_REG_ELR_EL1)?;
        sregs.spsr_el1 = self.read_sys_reg(HV_SYS_REG_SPSR_EL1)?;
        sregs.sp_el0 = self.read_sys_reg(HV_SYS_REG_SP_EL0)?;
        sregs.sp_el1 = self.read_sys_reg(HV_SYS_REG_SP_EL1)?;
        sregs.tpidr_el0 = self.read_sys_reg(HV_SYS_REG_TPIDR_EL0)?;
        sregs.tpidr_el1 = self.read_sys_reg(HV_SYS_REG_TPIDR_EL1)?;
        sregs.tpidrro_el0 = self.read_sys_reg(HV_SYS_REG_TPIDRRO_EL0)?;

        Ok(CpuState::Aarch64(Aarch64CpuState { regs, sregs }))
    }

    fn set_state(&mut self, state: &CpuState) -> Result<()> {
        let state = match state {
            CpuState::Aarch64(state) => state,
            _ => {
                return Err(Error::InvalidConfig(
                    "ARM64 HVF backend requires aarch64 state".to_string(),
                ))
            }
        };

        let regs = &state.regs;
        let sregs = &state.sregs;

        // Write X0-X30
        for i in 0..31 {
            self.write_gpr(i as u32, regs.x[i])?;
        }

        // Write PC, SP, PSTATE
        self.write_gpr(HV_REG_PC, regs.pc)?;
        self.write_sys_reg(HV_SYS_REG_SP_EL0, regs.sp)?;
        self.write_gpr(HV_REG_CPSR, regs.pstate)?;

        // Write FPCR/FPSR
        self.write_gpr(HV_REG_FPCR, regs.fpcr as u64)?;
        self.write_gpr(HV_REG_FPSR, regs.fpsr as u64)?;

        // Write V0-V31
        for i in 0..32 {
            self.write_simd_reg(i as u32, regs.v[i])?;
        }

        // Write system registers
        self.write_sys_reg(HV_SYS_REG_SCTLR_EL1, sregs.sctlr_el1)?;
        self.write_sys_reg(HV_SYS_REG_TCR_EL1, sregs.tcr_el1)?;
        self.write_sys_reg(HV_SYS_REG_TTBR0_EL1, sregs.ttbr0_el1)?;
        self.write_sys_reg(HV_SYS_REG_TTBR1_EL1, sregs.ttbr1_el1)?;
        self.write_sys_reg(HV_SYS_REG_MAIR_EL1, sregs.mair_el1)?;
        self.write_sys_reg(HV_SYS_REG_VBAR_EL1, sregs.vbar_el1)?;
        self.write_sys_reg(HV_SYS_REG_ELR_EL1, sregs.elr_el1)?;
        self.write_sys_reg(HV_SYS_REG_SPSR_EL1, sregs.spsr_el1)?;
        self.write_sys_reg(HV_SYS_REG_SP_EL0, sregs.sp_el0)?;
        self.write_sys_reg(HV_SYS_REG_SP_EL1, sregs.sp_el1)?;
        self.write_sys_reg(HV_SYS_REG_TPIDR_EL0, sregs.tpidr_el0)?;
        self.write_sys_reg(HV_SYS_REG_TPIDR_EL1, sregs.tpidr_el1)?;
        self.write_sys_reg(HV_SYS_REG_TPIDRRO_EL0, sregs.tpidrro_el0)?;

        Ok(())
    }

    fn complete_io_in(&mut self, _data: &[u8]) {
        // ARM doesn't have port I/O
    }

    fn inject_interrupt(&mut self, vector: u8) -> Result<bool> {
        // For ARM64, we use IRQ/FIQ
        // Vector 0 = IRQ, Vector 1 = FIQ
        match vector {
            0 => {
                self.pending_irq = true;
                self.halted = false;
            }
            1 => {
                self.pending_fiq = true;
                self.halted = false;
            }
            _ => {
                // For proper GIC integration, we'd handle multiple interrupt sources
                self.pending_irq = true;
                self.halted = false;
            }
        }
        Ok(true)
    }

    fn can_inject_interrupt(&self) -> bool {
        // Check PSTATE.I/F (interrupt masks)
        let pstate = self.read_gpr(HV_REG_CPSR).unwrap_or(0);
        let irq_masked = (pstate & (1 << 7)) != 0; // PSTATE.I
        let fiq_masked = (pstate & (1 << 6)) != 0; // PSTATE.F

        !irq_masked || !fiq_masked
    }

    fn inject_nmi(&mut self) -> Result<bool> {
        // ARM doesn't have NMI per se, use FIQ
        self.pending_fiq = true;
        self.halted = false;
        Ok(true)
    }

    #[cfg(feature = "debug")]
    fn set_single_step(&mut self, _enabled: bool) {
        // ARM single-step would need debug register setup
        // Not implemented yet
    }

    #[cfg(feature = "debug")]
    fn is_single_step(&self) -> bool {
        false
    }

    #[cfg(feature = "debug")]
    fn invalidate_code_cache(&mut self, _addr: u64) {
        // HVF doesn't have a software decode cache
    }

    fn id(&self) -> u32 {
        self.id
    }

    fn instruction_count(&self) -> u64 {
        // HVF doesn't provide instruction count
        0
    }
}

impl Drop for HvfArm64Vcpu {
    fn drop(&mut self) {
        let ret = unsafe { hv_vcpu_destroy(self.vcpu) };
        if ret != HV_SUCCESS {
            warn!(
                "Failed to destroy ARM64 vCPU {}: {}",
                self.id,
                hv_error_string(ret)
            );
        }
    }
}
