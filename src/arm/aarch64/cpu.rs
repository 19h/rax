//! AArch64 CPU Implementation
//!
//! This module implements a complete AArch64 CPU emulator supporting:
//! - All exception levels (EL0-EL3)
//! - Full system register set
//! - MMU with page table walks
//! - GIC interrupt controller
//! - All ARMv8/v9 instruction categories

use std::collections::HashSet;
use std::fmt::Debug;

use super::exceptions::{
    build_spsr, exception_target_el, parse_spsr, vector_offset, ExceptionType, SyndromeRegister,
};
use super::gic::{Gic, GicConfig};
use super::mmu::{Mmu, MmuConfig, TranslationFault, TranslationGranule};
use super::sysregs::SystemRegisters;
use super::{sctlr, NUM_ELS, NUM_GPRS, NUM_SIMD_REGS};

use crate::arm::cpu_trait::{
    ArmCpu, ArmError, ArmException, ArmProfile, ArmVersion, CpuExit, MemoryFaultInfo,
    MemoryFaultType, ProcessorState, WatchpointKind,
};
use crate::arm::features::ArmFeatures;
use crate::arm::memory::ArmMemory;
use crate::arm::sysreg::Aarch64SysRegEncoding;

// =============================================================================
// CPU Configuration
// =============================================================================

/// AArch64 CPU configuration.
#[derive(Clone, Debug)]
pub struct AArch64Config {
    /// Architecture version.
    pub version: ArmVersion,
    /// Enabled features.
    pub features: ArmFeatures,
    /// Initial exception level (1, 2, or 3).
    pub initial_el: u8,
    /// GIC configuration.
    pub gic_config: Option<GicConfig>,
    /// Number of breakpoint registers.
    pub num_breakpoints: u8,
    /// Number of watchpoint registers.
    pub num_watchpoints: u8,
}

impl Default for AArch64Config {
    fn default() -> Self {
        Self {
            version: ArmVersion::V8_0A,
            features: ArmFeatures::armv8_0_base(),
            initial_el: 1,
            gic_config: Some(GicConfig::default()),
            num_breakpoints: 6,
            num_watchpoints: 4,
        }
    }
}

impl AArch64Config {
    /// Create configuration for ARMv8.0-A.
    pub fn v8_0() -> Self {
        Self {
            version: ArmVersion::V8_0A,
            features: ArmFeatures::armv8_0_base(),
            ..Default::default()
        }
    }

    /// Create configuration for ARMv8.1-A.
    pub fn v8_1() -> Self {
        Self {
            version: ArmVersion::V8_1A,
            features: ArmFeatures::armv8_1_base(),
            ..Default::default()
        }
    }

    /// Create configuration for ARMv8.2-A.
    pub fn v8_2() -> Self {
        Self {
            version: ArmVersion::V8_2A,
            features: ArmFeatures::armv8_2_base(),
            ..Default::default()
        }
    }

    /// Create configuration for ARMv9.0-A.
    pub fn v9_0() -> Self {
        Self {
            version: ArmVersion::V9_0A,
            features: ArmFeatures::armv9_0_base(),
            ..Default::default()
        }
    }
}

// =============================================================================
// AArch64 CPU
// =============================================================================

/// AArch64 CPU emulator.
pub struct AArch64Cpu {
    // Note: Debug derived manually below due to Box<dyn ArmMemory>
    // =========================================================================
    // General Purpose Registers
    // =========================================================================
    /// X0-X30 (64-bit general purpose registers).
    x: [u64; NUM_GPRS],

    /// Stack pointers for each EL.
    sp_el: [u64; NUM_ELS],

    /// Program Counter.
    pc: u64,

    // =========================================================================
    // Processor State (PSTATE)
    // =========================================================================
    /// NZCV condition flags.
    nzcv: u8,

    /// DAIF interrupt masks (D, A, I, F).
    daif: u8,

    /// Current exception level (0-3).
    current_el: u8,

    /// SP selection (false = SP_EL0, true = SP_ELx).
    sp_sel: bool,

    /// PAN (Privileged Access Never).
    pan: bool,

    /// UAO (User Access Override).
    uao: bool,

    /// DIT (Data Independent Timing).
    dit: bool,

    /// SSBS (Speculative Store Bypass Safe).
    ssbs: bool,

    /// TCO (Tag Check Override).
    tco: bool,

    /// BTYPE (Branch Type for BTI).
    btype: u8,

    /// IL (Illegal execution state).
    il: bool,

    /// SS (Software Step).
    ss: bool,

    // =========================================================================
    // SIMD/FP Registers
    // =========================================================================
    /// V0-V31 (128-bit SIMD/FP registers).
    v: [u128; NUM_SIMD_REGS],

    /// Floating-point Control Register.
    fpcr: u32,

    /// Floating-point Status Register.
    fpsr: u32,

    // =========================================================================
    // SVE (Scalable Vector Extension)
    // =========================================================================
    /// SVE Vector Length in bits (must be multiple of 128, min 128, max 2048).
    /// For simplicity, we use VL=128 which makes Z registers equivalent to V registers.
    sve_vl: u16,

    /// SVE Predicate registers P0-P15.
    /// Each bit corresponds to one byte of the vector (VL/8 bits per predicate).
    /// For VL=128: 16 bits, VL=256: 32 bits, etc.
    /// We use u32 to support up to VL=256.
    sve_p: [u32; 16],

    /// First-fault register (FFR) - special predicate for first-fault loads.
    sve_ffr: u32,

    // =========================================================================
    // System Registers
    // =========================================================================
    /// All system registers.
    sysregs: SystemRegisters,

    // =========================================================================
    // MMU
    // =========================================================================
    /// Memory Management Unit.
    mmu: Mmu,

    // =========================================================================
    // GIC
    // =========================================================================
    /// Generic Interrupt Controller.
    gic: Option<Gic>,

    // =========================================================================
    // Memory
    // =========================================================================
    /// Physical memory.
    memory: Box<dyn ArmMemory>,

    // =========================================================================
    // Execution State
    // =========================================================================
    /// Instruction count.
    insn_count: u64,

    /// Cycle count.
    cycle_count: u64,

    /// CPU halted.
    halted: bool,

    /// Waiting for interrupt.
    wfi: bool,

    /// Waiting for event.
    wfe: bool,

    /// Event signaled.
    event_register: bool,

    /// Pending exceptions.
    pending_exceptions: Vec<ArmException>,

    // =========================================================================
    // Debug
    // =========================================================================
    /// Breakpoints (PC addresses).
    breakpoints: HashSet<u64>,

    /// Watchpoints (address, size, kind).
    watchpoints: Vec<(u64, usize, WatchpointKind)>,

    // =========================================================================
    // Configuration
    // =========================================================================
    /// CPU configuration.
    config: AArch64Config,
}

impl std::fmt::Debug for AArch64Cpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AArch64Cpu")
            .field("pc", &format_args!("0x{:016x}", self.pc))
            .field("current_el", &self.current_el)
            .field("sp_sel", &self.sp_sel)
            .field("nzcv", &format_args!("{:04b}", self.nzcv))
            .field("daif", &format_args!("{:04b}", self.daif))
            .field("insn_count", &self.insn_count)
            .field("halted", &self.halted)
            .finish_non_exhaustive()
    }
}

impl AArch64Cpu {
    /// Create a new AArch64 CPU.
    pub fn new(config: AArch64Config, memory: Box<dyn ArmMemory>) -> Self {
        let gic = config.gic_config.as_ref().map(|gc| Gic::new(gc.clone()));

        Self {
            x: [0; NUM_GPRS],
            sp_el: [0; NUM_ELS],
            pc: 0,

            nzcv: 0,
            daif: 0xF, // All exceptions masked on reset
            current_el: config.initial_el,
            sp_sel: true, // Use SP_ELx on reset
            pan: false,
            uao: false,
            dit: false,
            ssbs: false,
            tco: false,
            btype: 0,
            il: false,
            ss: false,

            v: [0; NUM_SIMD_REGS],
            fpcr: 0,
            fpsr: 0,

            // SVE: Default VL=128 bits (16 bytes)
            sve_vl: 128,
            sve_p: [0; 16],
            sve_ffr: 0,

            sysregs: SystemRegisters::new(),
            mmu: Mmu::new(),
            gic,
            memory,

            insn_count: 0,
            cycle_count: 0,
            halted: false,
            wfi: false,
            wfe: false,
            event_register: false,
            pending_exceptions: Vec::new(),

            breakpoints: HashSet::new(),
            watchpoints: Vec::new(),

            config,
        }
    }

    // =========================================================================
    // Register Access
    // =========================================================================

    /// Get X register (X0-X30, or XZR if reg == 31).
    pub fn get_x(&self, reg: u8) -> u64 {
        if reg < 31 {
            self.x[reg as usize]
        } else {
            0 // XZR
        }
    }

    /// Set X register (X0-X30, write to XZR is ignored).
    pub fn set_x(&mut self, reg: u8, value: u64) {
        if reg < 31 {
            self.x[reg as usize] = value;
        }
    }

    /// Get W register (lower 32 bits of X).
    pub fn get_w(&self, reg: u8) -> u32 {
        self.get_x(reg) as u32
    }

    /// Set W register (zero-extends to X).
    pub fn set_w(&mut self, reg: u8, value: u32) {
        self.set_x(reg, value as u64);
    }

    /// Get current stack pointer.
    pub fn current_sp(&self) -> u64 {
        if self.sp_sel || self.current_el == 0 {
            if self.current_el == 0 {
                self.sp_el[0]
            } else {
                self.sp_el[self.current_el as usize]
            }
        } else {
            self.sp_el[0]
        }
    }

    /// Set current stack pointer.
    pub fn set_current_sp(&mut self, value: u64) {
        if self.sp_sel || self.current_el == 0 {
            if self.current_el == 0 {
                self.sp_el[0] = value;
            } else {
                self.sp_el[self.current_el as usize] = value;
            }
        } else {
            self.sp_el[0] = value;
        }
    }

    // =========================================================================
    // Flag Access
    // =========================================================================

    /// Get N flag.
    pub fn get_n(&self) -> bool {
        (self.nzcv >> 3) & 1 != 0
    }

    /// Get Z flag.
    pub fn get_z(&self) -> bool {
        (self.nzcv >> 2) & 1 != 0
    }

    /// Get C flag.
    pub fn get_c(&self) -> bool {
        (self.nzcv >> 1) & 1 != 0
    }

    /// Get V flag.
    pub fn get_v(&self) -> bool {
        self.nzcv & 1 != 0
    }

    /// Set N flag.
    pub fn set_n(&mut self, v: bool) {
        if v {
            self.nzcv |= 0x8;
        } else {
            self.nzcv &= !0x8;
        }
    }

    /// Set Z flag.
    pub fn set_z(&mut self, v: bool) {
        if v {
            self.nzcv |= 0x4;
        } else {
            self.nzcv &= !0x4;
        }
    }

    /// Set C flag.
    pub fn set_c(&mut self, v: bool) {
        if v {
            self.nzcv |= 0x2;
        } else {
            self.nzcv &= !0x2;
        }
    }

    /// Set V flag.
    pub fn set_v(&mut self, v: bool) {
        if v {
            self.nzcv |= 0x1;
        } else {
            self.nzcv &= !0x1;
        }
    }

    /// Set all NZCV flags.
    pub fn set_nzcv(&mut self, n: bool, z: bool, c: bool, v: bool) {
        self.nzcv = ((n as u8) << 3) | ((z as u8) << 2) | ((c as u8) << 1) | (v as u8);
    }

    /// Update N and Z flags based on result.
    pub fn update_nz_64(&mut self, result: u64) {
        self.set_n((result as i64) < 0);
        self.set_z(result == 0);
    }

    /// Update N and Z flags based on 32-bit result.
    pub fn update_nz_32(&mut self, result: u32) {
        self.set_n((result as i32) < 0);
        self.set_z(result == 0);
    }

    // =========================================================================
    // Condition Evaluation
    // =========================================================================

    /// Evaluate condition code.
    pub fn condition_holds(&self, cond: u8) -> bool {
        let result = match cond >> 1 {
            0b000 => self.get_z(),                                  // EQ/NE
            0b001 => self.get_c(),                                  // CS/CC
            0b010 => self.get_n(),                                  // MI/PL
            0b011 => self.get_v(),                                  // VS/VC
            0b100 => self.get_c() && !self.get_z(),                 // HI/LS
            0b101 => self.get_n() == self.get_v(),                  // GE/LT
            0b110 => self.get_n() == self.get_v() && !self.get_z(), // GT/LE
            0b111 => true,                                          // AL
            _ => unreachable!(),
        };

        if cond & 1 != 0 && cond != 0xF {
            !result
        } else {
            result
        }
    }

    // =========================================================================
    // Memory Access
    // =========================================================================

    /// Read byte from memory (with MMU translation).
    pub fn mem_read_u8(&self, va: u64) -> Result<u8, ArmError> {
        let pa = self.translate_address(va, false, false)?;
        self.memory.read_u8(pa).map_err(|e| e.into())
    }

    /// Read halfword from memory.
    pub fn mem_read_u16(&self, va: u64) -> Result<u16, ArmError> {
        let pa = self.translate_address(va, false, false)?;
        self.memory.read_u16(pa).map_err(|e| e.into())
    }

    /// Read word from memory.
    pub fn mem_read_u32(&self, va: u64) -> Result<u32, ArmError> {
        let pa = self.translate_address(va, false, false)?;
        self.memory.read_u32(pa).map_err(|e| e.into())
    }

    /// Read doubleword from memory.
    pub fn mem_read_u64(&self, va: u64) -> Result<u64, ArmError> {
        let pa = self.translate_address(va, false, false)?;
        self.memory.read_u64(pa).map_err(|e| e.into())
    }

    /// Write byte to memory.
    pub fn mem_write_u8(&mut self, va: u64, value: u8) -> Result<(), ArmError> {
        let pa = self.translate_address(va, true, false)?;
        self.memory.write_u8(pa, value).map_err(|e| e.into())
    }

    /// Write halfword to memory.
    pub fn mem_write_u16(&mut self, va: u64, value: u16) -> Result<(), ArmError> {
        let pa = self.translate_address(va, true, false)?;
        self.memory.write_u16(pa, value).map_err(|e| e.into())
    }

    /// Write word to memory.
    pub fn mem_write_u32(&mut self, va: u64, value: u32) -> Result<(), ArmError> {
        let pa = self.translate_address(va, true, false)?;
        self.memory.write_u32(pa, value).map_err(|e| e.into())
    }

    /// Write doubleword to memory.
    pub fn mem_write_u64(&mut self, va: u64, value: u64) -> Result<(), ArmError> {
        let pa = self.translate_address(va, true, false)?;
        self.memory.write_u64(pa, value).map_err(|e| e.into())
    }

    /// Translate virtual address to physical address.
    fn translate_address(
        &self,
        va: u64,
        is_write: bool,
        is_execute: bool,
    ) -> Result<u64, ArmError> {
        // Check alignment for execute
        if is_execute && (va & 3) != 0 {
            return Err(ArmError::MemoryError(MemoryFaultInfo {
                address: va,
                access: if is_write {
                    crate::arm::cpu_trait::AccessType::Write
                } else if is_execute {
                    crate::arm::cpu_trait::AccessType::InstructionFetch
                } else {
                    crate::arm::cpu_trait::AccessType::Read
                },
                fault_type: MemoryFaultType::Alignment,
                stage2: false,
            }));
        }

        // Use MMU if enabled
        let privileged = self.current_el > 0;
        match self.mmu.translate(
            va,
            self.memory.as_ref(),
            is_write,
            is_execute,
            privileged,
            self.current_el,
        ) {
            Ok(desc) => Ok(desc.pa),
            Err(fault) => Err(self.translation_fault_to_error(fault, is_write)),
        }
    }

    /// Convert translation fault to ArmError.
    fn translation_fault_to_error(&self, fault: TranslationFault, is_write: bool) -> ArmError {
        use super::mmu::TranslationFaultType;

        let fault_type = match fault.fault_type {
            TranslationFaultType::Translation => MemoryFaultType::Translation,
            TranslationFaultType::Permission => MemoryFaultType::Permission,
            TranslationFaultType::Alignment => MemoryFaultType::Alignment,
            TranslationFaultType::AccessFlag => MemoryFaultType::AccessFlag,
            TranslationFaultType::AddressSize => MemoryFaultType::AddressSize,
            TranslationFaultType::ExternalAbort => MemoryFaultType::External,
        };

        ArmError::MemoryError(MemoryFaultInfo {
            address: fault.va,
            access: if is_write {
                crate::arm::cpu_trait::AccessType::Write
            } else {
                crate::arm::cpu_trait::AccessType::Read
            },
            fault_type,
            stage2: fault.stage2,
        })
    }

    // =========================================================================
    // Instruction Fetch and Execution
    // =========================================================================

    /// Fetch instruction at PC.
    fn fetch_instruction(&self) -> Result<u32, ArmError> {
        let pa = self.translate_address(self.pc, false, true)?;
        self.memory.read_u32(pa).map_err(|e| e.into())
    }

    /// Execute one instruction.
    fn execute_instruction(&mut self) -> Result<CpuExit, ArmError> {
        // Fetch instruction
        let insn = self.fetch_instruction()?;

        // Check breakpoint
        if self.breakpoints.contains(&self.pc) {
            return Ok(CpuExit::Breakpoint(self.pc as u32));
        }

        // Save PC and advance
        let old_pc = self.pc;
        self.pc = self.pc.wrapping_add(4);

        // Clear BTYPE (set by branches)
        let old_btype = self.btype;
        self.btype = 0;

        // Execute
        let result = self.decode_and_execute(insn);

        match result {
            Ok(exit) => {
                self.insn_count += 1;
                self.cycle_count += 1;
                Ok(exit)
            }
            Err(e) => {
                // Restore PC on error
                self.pc = old_pc;
                self.btype = old_btype;
                Err(e)
            }
        }
    }

    /// Decode and execute an instruction.
    fn decode_and_execute(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        // Top-level decode by bits [28:25]
        let op0 = (insn >> 25) & 0xF;

        match op0 {
            // Reserved
            0b0000 => Err(ArmError::UndefinedInstruction(insn)),

            // Unallocated
            0b0001 | 0b0011 => Err(ArmError::UndefinedInstruction(insn)),

            // SVE (Scalable Vector Extension)
            0b0010 => self.exec_sve(insn),

            // Data Processing - Immediate
            0b1000 | 0b1001 => self.exec_dp_imm(insn),

            // Branches, Exception Generating, System
            0b1010 | 0b1011 => self.exec_branch_system(insn),

            // Loads and Stores
            0b0100 | 0b0110 | 0b1100 | 0b1110 => self.exec_load_store(insn),

            // Data Processing - Register
            0b0101 | 0b1101 => self.exec_dp_reg(insn),

            // Data Processing - SIMD and FP
            0b0111 | 0b1111 => self.exec_simd_fp(insn),

            _ => Err(ArmError::UndefinedInstruction(insn)),
        }
    }

    // =========================================================================
    // Exception Handling
    // =========================================================================

    /// Take an exception.
    fn take_exception(
        &mut self,
        target_el: u8,
        exc_type: ExceptionType,
        syndrome: SyndromeRegister,
    ) -> Result<(), ArmError> {
        // Build SPSR from current state
        let saved_spsr = build_spsr(
            self.nzcv,
            self.daif,
            self.current_el,
            self.sp_sel,
            self.ssbs,
            self.pan,
            self.uao,
            self.dit,
            self.tco,
            self.btype,
            self.il,
            self.ss,
        );

        // Save state to target EL
        self.sysregs.bank_mut(target_el).spsr = saved_spsr;
        self.sysregs.bank_mut(target_el).elr = self.pc;
        self.sysregs.bank_mut(target_el).esr = syndrome.value;

        // Calculate vector offset
        let offset = vector_offset(
            exc_type,
            self.current_el,
            target_el,
            true, // from AArch64
            self.sp_sel,
        );

        // Get VBAR
        let vbar = self.sysregs.bank(target_el).vbar;

        // Switch to target EL
        self.current_el = target_el;
        self.sp_sel = true; // Use SP_ELx
        self.daif = 0xF; // Mask all interrupts

        // Clear single-step
        self.ss = false;

        // Clear IL
        self.il = false;

        // Set BTYPE to 0
        self.btype = 0;

        // Branch to handler
        self.pc = vbar.wrapping_add(offset);

        Ok(())
    }

    /// Return from exception (ERET).
    fn exception_return(&mut self) -> Result<CpuExit, ArmError> {
        // Get saved state from current EL
        let spsr = self.sysregs.bank(self.current_el).spsr;
        let elr = self.sysregs.bank(self.current_el).elr;

        // Parse SPSR
        let (nzcv, daif, target_el, sp_sel, ssbs, pan, uao, dit, tco, btype, il, ss) =
            parse_spsr(spsr);

        // Check if return is valid
        if target_el > self.current_el {
            // Cannot return to higher EL
            return Err(ArmError::Internal("ERET to higher EL".to_string()));
        }

        // Restore state
        self.nzcv = nzcv;
        self.daif = daif;
        self.current_el = target_el;
        self.sp_sel = sp_sel;
        self.ssbs = ssbs;
        self.pan = pan;
        self.uao = uao;
        self.dit = dit;
        self.tco = tco;
        self.btype = btype;
        self.il = il;
        self.ss = ss;

        // Set PC
        self.pc = elr;

        Ok(CpuExit::Continue)
    }

    /// Check for pending interrupts.
    fn check_pending_interrupts(&mut self) -> Result<Option<CpuExit>, ArmError> {
        // Check GIC for pending interrupt
        if let Some(ref gic) = self.gic {
            let cpu_id = 0; // Assume single core for now

            if gic.pending_interrupt(cpu_id) {
                // Check if IRQ is masked
                let irq_masked = (self.daif & 0x2) != 0;

                if !irq_masked {
                    // Determine target EL
                    let target_el = exception_target_el(
                        ExceptionType::Irq,
                        self.current_el,
                        self.sysregs.hcr_el2,
                        self.sysregs.scr_el3,
                    );

                    return Ok(Some(CpuExit::InterruptPending));
                }
            }
        }

        // Check timer interrupts
        if self.sysregs.cntp_interrupt_pending() {
            let irq_masked = (self.daif & 0x2) != 0;
            if !irq_masked {
                return Ok(Some(CpuExit::InterruptPending));
            }
        }

        Ok(None)
    }

    // =========================================================================
    // System Register Access
    // =========================================================================

    /// Read system register.
    fn read_sysreg(&self, encoding: Aarch64SysRegEncoding) -> Result<u64, ArmError> {
        // Handle special cases first
        match (
            encoding.op0,
            encoding.op1,
            encoding.crn,
            encoding.crm,
            encoding.op2,
        ) {
            // NZCV
            (3, 3, 4, 2, 0) => {
                return Ok((self.nzcv as u64) << 28);
            }
            // DAIF
            (3, 3, 4, 2, 1) => {
                return Ok((self.daif as u64) << 6);
            }
            // CurrentEL
            (3, 0, 4, 2, 2) => {
                return Ok((self.current_el as u64) << 2);
            }
            // SPSel
            (3, 0, 4, 2, 0) => {
                return Ok(self.sp_sel as u64);
            }
            // SP_EL0
            (3, 0, 4, 1, 0) => {
                return Ok(self.sp_el[0]);
            }
            // SP_EL1
            (3, 4, 4, 1, 0) => {
                return Ok(self.sp_el[1]);
            }
            // SP_EL2
            (3, 6, 4, 1, 0) => {
                return Ok(self.sp_el[2]);
            }
            // FPCR
            (3, 3, 4, 4, 0) => {
                return Ok(self.fpcr as u64);
            }
            // FPSR
            (3, 3, 4, 4, 1) => {
                return Ok(self.fpsr as u64);
            }
            _ => {}
        }

        // Read from sysregs
        self.sysregs
            .read(encoding, self.current_el)
            .ok_or_else(|| ArmError::Unimplemented(format!("System register {}", encoding)))
    }

    /// Write system register.
    fn write_sysreg(
        &mut self,
        encoding: Aarch64SysRegEncoding,
        value: u64,
    ) -> Result<(), ArmError> {
        // Handle special cases first
        match (
            encoding.op0,
            encoding.op1,
            encoding.crn,
            encoding.crm,
            encoding.op2,
        ) {
            // NZCV
            (3, 3, 4, 2, 0) => {
                self.nzcv = ((value >> 28) & 0xF) as u8;
                return Ok(());
            }
            // DAIF
            (3, 3, 4, 2, 1) => {
                self.daif = ((value >> 6) & 0xF) as u8;
                return Ok(());
            }
            // SPSel
            (3, 0, 4, 2, 0) => {
                self.sp_sel = (value & 1) != 0;
                return Ok(());
            }
            // SP_EL0
            (3, 0, 4, 1, 0) => {
                self.sp_el[0] = value;
                return Ok(());
            }
            // SP_EL1
            (3, 4, 4, 1, 0) => {
                self.sp_el[1] = value;
                return Ok(());
            }
            // SP_EL2
            (3, 6, 4, 1, 0) => {
                self.sp_el[2] = value;
                return Ok(());
            }
            // FPCR
            (3, 3, 4, 4, 0) => {
                self.fpcr = value as u32;
                return Ok(());
            }
            // FPSR
            (3, 3, 4, 4, 1) => {
                self.fpsr = value as u32;
                return Ok(());
            }
            // SCTLR_ELx - update MMU config
            (3, 0, 1, 0, 0) | (3, 4, 1, 0, 0) | (3, 6, 1, 0, 0) => {
                let el = encoding.op1 / 2; // 0->EL1, 4->EL2, 6->EL3
                let el = if el == 0 { 1 } else { el };
                self.sysregs.bank_mut(el).sctlr = value;
                self.update_mmu_config();
                return Ok(());
            }
            // TCR_ELx - update MMU config
            (3, 0, 2, 0, 2) | (3, 4, 2, 0, 2) | (3, 6, 2, 0, 2) => {
                let el = encoding.op1 / 2;
                let el = if el == 0 { 1 } else { el };
                self.sysregs.bank_mut(el).tcr = value;
                self.update_mmu_config();
                return Ok(());
            }
            // TTBR0_ELx - update MMU config
            (3, 0, 2, 0, 0) | (3, 4, 2, 0, 0) | (3, 6, 2, 0, 0) => {
                let el = encoding.op1 / 2;
                let el = if el == 0 { 1 } else { el };
                self.sysregs.bank_mut(el).ttbr0 = value;
                self.update_mmu_config();
                return Ok(());
            }
            // TTBR1_EL1
            (3, 0, 2, 0, 1) => {
                self.sysregs.el1.ttbr1 = value;
                self.update_mmu_config();
                return Ok(());
            }
            // MAIR_ELx
            (3, 0, 10, 2, 0) | (3, 4, 10, 2, 0) | (3, 6, 10, 2, 0) => {
                let el = encoding.op1 / 2;
                let el = if el == 0 { 1 } else { el };
                self.sysregs.bank_mut(el).mair = value;
                self.update_mmu_config();
                return Ok(());
            }
            _ => {}
        }

        // Write to sysregs
        if self.sysregs.write(encoding, value, self.current_el) {
            Ok(())
        } else {
            Err(ArmError::Unimplemented(format!(
                "System register write {}",
                encoding
            )))
        }
    }

    /// Update MMU configuration from system registers.
    fn update_mmu_config(&mut self) {
        let sctlr = self.sysregs.bank(self.current_el).sctlr;
        let tcr = self.sysregs.bank(self.current_el).tcr;
        let ttbr0 = self.sysregs.bank(self.current_el).ttbr0;
        let ttbr1 = if self.current_el == 1 {
            self.sysregs.el1.ttbr1
        } else {
            0
        };
        let mair = self.sysregs.bank(self.current_el).mair;

        let enabled = (sctlr & sctlr::M) != 0;
        let wxn = (sctlr & sctlr::WXN) != 0;

        let t0sz = (tcr & 0x3F) as u8;
        let t1sz = ((tcr >> 16) & 0x3F) as u8;
        let tg0 = ((tcr >> 14) & 0x3) as u8;
        let tg1 = ((tcr >> 30) & 0x3) as u8;

        let granule0 = TranslationGranule::from_tg0(tg0).unwrap_or(TranslationGranule::Granule4KB);
        let granule1 = TranslationGranule::from_tg1(tg1).unwrap_or(TranslationGranule::Granule4KB);

        self.mmu.set_config(MmuConfig {
            enabled,
            pa_size: 48,
            t0sz,
            t1sz,
            tg0: granule0,
            tg1: granule1,
            ttbr0,
            ttbr1,
            mair,
            wxn,
        });
    }

    // =========================================================================
    // Instruction Execution Stubs
    // =========================================================================

    /// Execute data processing (immediate) instruction.
    fn exec_dp_imm(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let op0 = (insn >> 23) & 0x7;

        match op0 {
            0b000 | 0b001 => self.exec_pc_rel(insn),
            0b010 => self.exec_add_sub_imm(insn),
            0b011 => self.exec_add_sub_imm_tags(insn),
            0b100 => self.exec_logical_imm(insn),
            0b101 => self.exec_move_wide(insn),
            0b110 => self.exec_bitfield(insn),
            0b111 => self.exec_extract(insn),
            _ => Err(ArmError::UndefinedInstruction(insn)),
        }
    }

    /// Execute branch and system instruction.
    fn exec_branch_system(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        // Use bits [31:24] for primary decode
        let bits_31_24 = (insn >> 24) & 0xFF;

        // B.cond: bits[31:24] = 01010100 (0x54)
        if bits_31_24 == 0x54 {
            return self.exec_b_cond(insn);
        }

        // B, BL: bits[31:26] = 00010x or 10010x -> bits[31:24] starts with 000101 or 100101
        // Actually B: 000101, BL: 100101, so check bits[30:26] = 00101
        let bits_30_26 = (insn >> 26) & 0x1F;
        if bits_30_26 == 0b00101 {
            return self.exec_b_bl(insn);
        }

        // CBZ/CBNZ: bits[31:24] = x0110100 or x0110101 -> 0x34/0x35 or 0xB4/0xB5
        if bits_31_24 == 0x34 || bits_31_24 == 0x35 || bits_31_24 == 0xB4 || bits_31_24 == 0xB5 {
            return self.exec_cbz_cbnz(insn);
        }

        // TBZ/TBNZ: bits[31:24] = x0110110 or x0110111 -> 0x36/0x37 or 0xB6/0xB7
        if bits_31_24 == 0x36 || bits_31_24 == 0x37 || bits_31_24 == 0xB6 || bits_31_24 == 0xB7 {
            return self.exec_tbz_tbnz(insn);
        }

        // Exception generation: bits[31:24] = 0xD4
        if bits_31_24 == 0xD4 {
            return self.exec_exception_system(insn);
        }

        // System instructions: bits[31:22] = 1101010100 -> bits[31:24] = 0xD5 and bits[23:22] = 00
        if bits_31_24 == 0xD5 {
            let bits_23_22 = (insn >> 22) & 0x3;
            if bits_23_22 == 0 {
                return self.exec_exception_system(insn);
            }
        }

        // Unconditional branch (register): bits[31:25] = 1101011 -> bits[31:24] = 0xD6
        if bits_31_24 == 0xD6 {
            return self.exec_br_reg(insn);
        }

        Err(ArmError::UndefinedInstruction(insn))
    }

    /// Execute load/store instruction.
    fn exec_load_store(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let op0 = (insn >> 28) & 0xF;
        let op1 = (insn >> 26) & 0x1;
        let bits_29_27 = (insn >> 27) & 0x7;
        let bit_24 = (insn >> 24) & 0x1;

        // Load/store exclusive: bits[29:27] = 00x, bit[24] = 0
        if bits_29_27 & 0b110 == 0b000 && bit_24 == 0 && op1 == 0 {
            return self.exec_ldst_exclusive(insn);
        }

        // Load register (literal): bits[29:27] = 01x, bit[26] = 0
        if bits_29_27 & 0b110 == 0b010 && op1 == 0 {
            return self.exec_ldr_literal(insn);
        }

        // Load/store pair: bits[29:27] = 10x (post-index, offset, pre-index)
        // bit[28] = 0 distinguishes pair from single register
        if bits_29_27 & 0b110 == 0b100 {
            return self.exec_ldst_pair(insn);
        }

        // Load/store single register: bits[29:27] = 11x
        if bits_29_27 & 0b110 == 0b110 {
            return self.exec_ldst_reg(insn);
        }

        // Fallback to single register for any remaining cases
        self.exec_ldst_reg(insn)
    }

    /// Execute data processing (register) instruction.
    fn exec_dp_reg(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let op1 = (insn >> 28) & 0x1;
        let op2 = (insn >> 21) & 0xF;
        let _op3 = (insn >> 10) & 0x3F;

        if op1 == 0 {
            if (op2 & 0b1000) == 0 {
                // Logical (shifted register)
                return self.exec_logical_shifted(insn);
            } else {
                // Add/sub (shifted/extended register)
                return self.exec_add_sub_shifted_ext(insn);
            }
        } else {
            // op1 = 1
            match op2 {
                0b0000 => {
                    // Add/sub with carry
                    return self.exec_adc_sbc(insn);
                }
                0b0010 => {
                    // Conditional compare (register)
                    return self.exec_ccmp_ccmn(insn);
                }
                0b0100 => {
                    // Conditional select
                    return self.exec_csel(insn);
                }
                0b0110 => {
                    // Data processing (2 source)
                    return self.exec_dp_2src(insn);
                }
                _ if (op2 & 0b1000) != 0 => {
                    // Data processing (3 source)
                    return self.exec_dp_3src(insn);
                }
                _ => {}
            }
        }

        Err(ArmError::UndefinedInstruction(insn))
    }

    /// Execute SIMD/FP instruction.
    fn exec_simd_fp(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        // Check if FP/SIMD is enabled
        let cpacr = self.sysregs.el1.cpacr;
        let fpen = (cpacr >> 20) & 0x3;

        if self.current_el == 0 && fpen != 0x3 {
            // FP/SIMD trapped at EL0
            return Ok(CpuExit::Undefined(insn));
        }
        if self.current_el == 1 && fpen == 0x0 {
            // FP/SIMD trapped at EL1
            return Ok(CpuExit::Undefined(insn));
        }

        // Decode SIMD/FP instruction groups
        // Bits [28:25] = 0111 or 1111 for SIMD/FP
        // Bits [31:30] and [24:21] determine the specific group

        let op0 = (insn >> 28) & 0xF;
        let op1 = (insn >> 23) & 0x3;
        let op2 = (insn >> 19) & 0xF;
        let op3 = (insn >> 10) & 0x1FF;

        // Scalar FP data processing (two source)
        // bits[31:24] = 0001_1110
        // bits[23:22] = type (size)
        // bit[21] = 1
        // bits[15:12] = opcode
        // bits[11:10] = 10
        if (insn >> 24) & 0xFF == 0b00011110 && (insn >> 21) & 1 == 1 && (insn >> 10) & 0x3 == 0b10
        {
            let fp_type = (insn >> 22) & 0x3;
            let opcode = (insn >> 12) & 0xF;
            let rm = ((insn >> 16) & 0x1F) as u8;
            let rn = ((insn >> 5) & 0x1F) as u8;
            let rd = (insn & 0x1F) as u8;

            // Determine precision
            match fp_type {
                0b00 => {
                    // Single precision (32-bit)
                    let op1 = f32::from_bits(self.v[rn as usize] as u32);
                    let op2 = f32::from_bits(self.v[rm as usize] as u32);

                    let result = match opcode {
                        0b0000 => op1 * op2,                   // FMUL
                        0b0001 => op1 / op2,                   // FDIV
                        0b0010 => op1 + op2,                   // FADD
                        0b0011 => op1 - op2,                   // FSUB
                        0b0100 => op1.max(op2),                // FMAX
                        0b0101 => op1.min(op2),                // FMIN
                        0b0110 => self.fp_maxnm_f32(op1, op2), // FMAXNM
                        0b0111 => self.fp_minnm_f32(op1, op2), // FMINNM
                        0b1000 => self.fp_nmul_f32(op1, op2),  // FNMUL
                        _ => return Err(ArmError::Unimplemented(format!("FP opcode {}", opcode))),
                    };

                    self.v[rd as usize] = result.to_bits() as u128;
                }
                0b01 => {
                    // Double precision (64-bit)
                    let op1 = f64::from_bits(self.v[rn as usize] as u64);
                    let op2 = f64::from_bits(self.v[rm as usize] as u64);

                    let result = match opcode {
                        0b0000 => op1 * op2,                   // FMUL
                        0b0001 => op1 / op2,                   // FDIV
                        0b0010 => op1 + op2,                   // FADD
                        0b0011 => op1 - op2,                   // FSUB
                        0b0100 => op1.max(op2),                // FMAX
                        0b0101 => op1.min(op2),                // FMIN
                        0b0110 => self.fp_maxnm_f64(op1, op2), // FMAXNM
                        0b0111 => self.fp_minnm_f64(op1, op2), // FMINNM
                        0b1000 => self.fp_nmul_f64(op1, op2),  // FNMUL
                        _ => return Err(ArmError::Unimplemented(format!("FP opcode {}", opcode))),
                    };

                    self.v[rd as usize] = result.to_bits() as u128;
                }
                _ => return Err(ArmError::Unimplemented("FP16/reserved".to_string())),
            }
            return Ok(CpuExit::Continue);
        }

        // Scalar FP data processing (one source)
        // bits[31:24] = 0001_1110
        // bits[23:22] = type (size)
        // bit[21] = 1
        // bits[20:15] = 10_0000
        // bits[14:10] = opcode
        if (insn >> 24) & 0xFF == 0b00011110
            && (insn >> 21) & 1 == 1
            && (insn >> 15) & 0x3F == 0b100000
        {
            let fp_type = (insn >> 22) & 0x3;
            let opcode = (insn >> 10) & 0x1F;
            let rn = ((insn >> 5) & 0x1F) as u8;
            let rd = (insn & 0x1F) as u8;

            match fp_type {
                0b00 => {
                    // Single precision
                    let op = f32::from_bits(self.v[rn as usize] as u32);
                    let result = match opcode {
                        0b00000 => op,                                                    // FMOV
                        0b00001 => op.abs(),                                              // FABS
                        0b00010 => -op,                                                   // FNEG
                        0b00011 => op.sqrt(),                                             // FSQRT
                        0b01000 => op.round(), // FRINTN (round to nearest, ties to even)
                        0b01001 => op.trunc() + if op.fract() > 0.0 { 1.0 } else { 0.0 }, // FRINTP
                        0b01010 => op.trunc() - if op.fract() < 0.0 { 1.0 } else { 0.0 }, // FRINTM
                        0b01011 => op.trunc(), // FRINTZ
                        _ => {
                            return Err(ArmError::Unimplemented(format!(
                                "FP unary opcode {}",
                                opcode
                            )))
                        }
                    };
                    self.v[rd as usize] = result.to_bits() as u128;
                }
                0b01 => {
                    // Double precision
                    let op = f64::from_bits(self.v[rn as usize] as u64);
                    let result = match opcode {
                        0b00000 => op,                                                    // FMOV
                        0b00001 => op.abs(),                                              // FABS
                        0b00010 => -op,                                                   // FNEG
                        0b00011 => op.sqrt(),                                             // FSQRT
                        0b01000 => op.round(),                                            // FRINTN
                        0b01001 => op.trunc() + if op.fract() > 0.0 { 1.0 } else { 0.0 }, // FRINTP
                        0b01010 => op.trunc() - if op.fract() < 0.0 { 1.0 } else { 0.0 }, // FRINTM
                        0b01011 => op.trunc(),                                            // FRINTZ
                        _ => {
                            return Err(ArmError::Unimplemented(format!(
                                "FP unary opcode {}",
                                opcode
                            )))
                        }
                    };
                    self.v[rd as usize] = result.to_bits() as u128;
                }
                _ => return Err(ArmError::Unimplemented("FP16/reserved".to_string())),
            }
            return Ok(CpuExit::Continue);
        }

        // FP compare
        // bits[31:24] = 0001_1110
        // bits[23:22] = type
        // bit[21] = 1
        // bits[15:14] = 00
        // bits[13:10] = 1000
        // bits[4:3] = opc
        // bits[2:0] = 0xx
        if (insn >> 24) & 0xFF == 0b00011110
            && (insn >> 21) & 1 == 1
            && (insn >> 14) & 0x3 == 0
            && (insn >> 10) & 0xF == 0b1000
            && (insn >> 3) & 0x3 != 0b11
        {
            let fp_type = (insn >> 22) & 0x3;
            let rm = ((insn >> 16) & 0x1F) as u8;
            let rn = ((insn >> 5) & 0x1F) as u8;
            let opc = (insn >> 3) & 0x3;
            let cmp_with_zero = (insn & 0x8) != 0;

            match fp_type {
                0b00 => {
                    // Single precision
                    let op1 = f32::from_bits(self.v[rn as usize] as u32);
                    let op2 = if cmp_with_zero {
                        0.0f32
                    } else {
                        f32::from_bits(self.v[rm as usize] as u32)
                    };

                    let (n, z, c, v) = if op1.is_nan() || op2.is_nan() {
                        // Unordered
                        if opc & 1 != 0 {
                            // FCMPE - signal exception
                            // For now, just set flags
                        }
                        (false, false, true, true)
                    } else if op1 == op2 {
                        (false, true, true, false)
                    } else if op1 < op2 {
                        (true, false, false, false)
                    } else {
                        (false, false, true, false)
                    };

                    self.set_n(n);
                    self.set_z(z);
                    self.set_c(c);
                    self.set_v(v);
                }
                0b01 => {
                    // Double precision
                    let op1 = f64::from_bits(self.v[rn as usize] as u64);
                    let op2 = if cmp_with_zero {
                        0.0f64
                    } else {
                        f64::from_bits(self.v[rm as usize] as u64)
                    };

                    let (n, z, c, v) = if op1.is_nan() || op2.is_nan() {
                        (false, false, true, true)
                    } else if op1 == op2 {
                        (false, true, true, false)
                    } else if op1 < op2 {
                        (true, false, false, false)
                    } else {
                        (false, false, true, false)
                    };

                    self.set_n(n);
                    self.set_z(z);
                    self.set_c(c);
                    self.set_v(v);
                }
                _ => return Err(ArmError::Unimplemented("FP16/reserved compare".to_string())),
            }
            return Ok(CpuExit::Continue);
        }

        // FMOV (general) - move between FP and general registers
        // bits[31] = sf
        // bits[30:24] = 0011110
        // bits[23:22] = type
        // bit[21] = 1
        // bits[20:19] = rmode
        // bits[18:16] = opcode
        // bits[15:10] = 000000
        if (insn >> 24) & 0x7F == 0b0011110 && (insn >> 21) & 1 == 1 && (insn >> 10) & 0x3F == 0 {
            let sf = (insn >> 31) & 1;
            let fp_type = (insn >> 22) & 0x3;
            let rmode = (insn >> 19) & 0x3;
            let opcode = (insn >> 16) & 0x7;
            let rn = ((insn >> 5) & 0x1F) as u8;
            let rd = (insn & 0x1F) as u8;

            match (sf, fp_type, rmode, opcode) {
                // FMOV Wd, Sn
                (0, 0b00, 0b00, 0b110) => {
                    let val = self.v[rn as usize] as u32;
                    self.set_w(rd, val);
                }
                // FMOV Sd, Wn
                (0, 0b00, 0b00, 0b111) => {
                    let val = self.get_w(rn);
                    self.v[rd as usize] = val as u128;
                }
                // FMOV Xd, Dn
                (1, 0b01, 0b00, 0b110) => {
                    let val = self.v[rn as usize] as u64;
                    self.set_x(rd, val);
                }
                // FMOV Dn, Xn
                (1, 0b01, 0b00, 0b111) => {
                    let val = self.get_x(rn);
                    self.v[rd as usize] = val as u128;
                }
                // FMOV Xd, Vn.D[1]
                (1, 0b10, 0b01, 0b110) => {
                    let val = (self.v[rn as usize] >> 64) as u64;
                    self.set_x(rd, val);
                }
                // FMOV Vd.D[1], Xn
                (1, 0b10, 0b01, 0b111) => {
                    let val = self.get_x(rn);
                    let lower = self.v[rd as usize] as u64;
                    self.v[rd as usize] = ((val as u128) << 64) | (lower as u128);
                }
                _ => {
                    return Err(ArmError::Unimplemented(format!(
                        "FMOV general variant sf={} type={} rmode={} op={}",
                        sf, fp_type, rmode, opcode
                    )));
                }
            }
            return Ok(CpuExit::Continue);
        }

        // FCVT - floating-point convert precision
        // bits[31:24] = 0001_1110
        // bits[23:22] = type (source)
        // bit[21] = 1
        // bits[20:17] = 0001
        // bits[16:15] = opc (dest)
        // bits[14:10] = 10000
        if (insn >> 24) & 0xFF == 0b00011110
            && (insn >> 21) & 1 == 1
            && (insn >> 17) & 0xF == 0b0001
            && (insn >> 10) & 0x1F == 0b10000
        {
            let src_type = (insn >> 22) & 0x3;
            let dst_type = (insn >> 15) & 0x3;
            let rn = ((insn >> 5) & 0x1F) as u8;
            let rd = (insn & 0x1F) as u8;

            match (src_type, dst_type) {
                // FCVT Dd, Sn (single to double)
                (0b00, 0b01) => {
                    let val = f32::from_bits(self.v[rn as usize] as u32);
                    let result = val as f64;
                    self.v[rd as usize] = result.to_bits() as u128;
                }
                // FCVT Sd, Dn (double to single)
                (0b01, 0b00) => {
                    let val = f64::from_bits(self.v[rn as usize] as u64);
                    let result = val as f32;
                    self.v[rd as usize] = result.to_bits() as u128;
                }
                _ => {
                    return Err(ArmError::Unimplemented(format!(
                        "FCVT variant src={} dst={}",
                        src_type, dst_type
                    )));
                }
            }
            return Ok(CpuExit::Continue);
        }

        // SCVTF/UCVTF - Signed/Unsigned integer to floating-point
        // Encoding: 0_sf_0_11110_type_1_00_opcode_000000_Rn_Rd
        // opcode: 010=SCVTF, 011=UCVTF
        if (insn >> 24) & 0x7F == 0b0011110
            && (insn >> 21) & 1 == 1
            && (insn >> 17) & 0x3 == 0b00
            && (insn >> 10) & 0x3F == 0
        {
            let sf = (insn >> 31) & 1;
            let fp_type = (insn >> 22) & 0x3;
            let opcode = (insn >> 16) & 0x7;
            let rn = ((insn >> 5) & 0x1F) as u8;
            let rd = (insn & 0x1F) as u8;

            let is_double = fp_type == 0b01;
            let is_signed = (opcode & 1) == 0;

            let int_val = if sf == 1 {
                self.get_x(rn)
            } else {
                self.get_w(rn) as u64
            };

            if is_double {
                let result = if is_signed {
                    if sf == 1 {
                        (int_val as i64) as f64
                    } else {
                        (int_val as i32) as f64
                    }
                } else {
                    if sf == 1 {
                        int_val as f64
                    } else {
                        (int_val as u32) as f64
                    }
                };
                self.v[rd as usize] = result.to_bits() as u128;
            } else {
                let result = if is_signed {
                    if sf == 1 {
                        (int_val as i64) as f32
                    } else {
                        (int_val as i32) as f32
                    }
                } else {
                    if sf == 1 {
                        int_val as f32
                    } else {
                        (int_val as u32) as f32
                    }
                };
                self.v[rd as usize] = result.to_bits() as u128;
            }

            return Ok(CpuExit::Continue);
        }

        // FCVTZS/FCVTZU - Floating-point to signed/unsigned integer with round toward zero
        // Encoding: 0_sf_0_11110_type_1_11_opcode_000000_Rn_Rd
        // opcode: 000=FCVTNS, 001=FCVTNU, 010=SCVTF, 011=UCVTF,
        //         100=FCVTAS, 101=FCVTAU, 110=FMOV, 111=FMOV
        //         type=0x: 000=FCVTMS/FCVTMU, 001=FCVTZS/FCVTZU, 010=FCVTPS/FCVTPU
        if (insn >> 24) & 0x7F == 0b0011110
            && (insn >> 21) & 1 == 1
            && (insn >> 17) & 0x3 == 0b11
            && (insn >> 10) & 0x3F == 0
        {
            let sf = (insn >> 31) & 1;
            let fp_type = (insn >> 22) & 0x3;
            let opcode = (insn >> 16) & 0x7;
            let rn = ((insn >> 5) & 0x1F) as u8;
            let rd = (insn & 0x1F) as u8;

            let is_double = fp_type == 0b01;
            let is_signed = (opcode & 1) == 0;

            // Get the floating point value
            let (result_signed, result_unsigned): (i64, u64) = if is_double {
                let fp_val = f64::from_bits(self.v[rn as usize] as u64);
                let truncated = fp_val.trunc();

                if is_signed {
                    let result = if sf == 1 {
                        truncated as i64
                    } else {
                        (truncated as i32) as i64
                    };
                    (result, result as u64)
                } else {
                    let result = if sf == 1 {
                        truncated as u64
                    } else {
                        (truncated as u32) as u64
                    };
                    (result as i64, result)
                }
            } else {
                let fp_val = f32::from_bits(self.v[rn as usize] as u32);
                let truncated = fp_val.trunc();

                if is_signed {
                    let result = if sf == 1 {
                        truncated as i64
                    } else {
                        (truncated as i32) as i64
                    };
                    (result, result as u64)
                } else {
                    let result = if sf == 1 {
                        truncated as u64
                    } else {
                        (truncated as u32) as u64
                    };
                    (result as i64, result)
                }
            };

            if sf == 1 {
                self.set_x(
                    rd,
                    if is_signed {
                        result_signed as u64
                    } else {
                        result_unsigned
                    },
                );
            } else {
                self.set_w(
                    rd,
                    if is_signed {
                        result_signed as u32
                    } else {
                        result_unsigned as u32
                    },
                );
            }

            return Ok(CpuExit::Continue);
        }

        // Advanced SIMD three-same
        // Encoding: 0_Q_U_01110_size_1_Rm_opcode_1_Rn_Rd
        if (insn >> 24) & 0x1F == 0b01110 && (insn >> 21) & 1 == 1 && (insn >> 10) & 1 == 1 {
            return self.exec_simd_three_same(insn);
        }

        // Advanced SIMD two-reg misc
        // Encoding: 0_Q_U_01110_size_10000_opcode_10_Rn_Rd
        if (insn >> 24) & 0x1F == 0b01110
            && (insn >> 17) & 0x1F == 0b10000
            && (insn >> 10) & 0x3 == 0b10
        {
            return self.exec_simd_two_reg(insn);
        }

        // If we get here, it's an unimplemented SIMD/FP instruction
        Err(ArmError::Unimplemented(format!(
            "SIMD/FP insn 0x{:08x}",
            insn
        )))
    }

    /// Execute SIMD three-same register instructions.
    fn exec_simd_three_same(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let q = (insn >> 30) & 1;
        let u = (insn >> 29) & 1;
        let size = (insn >> 22) & 0x3;
        let opcode = (insn >> 11) & 0x1F;
        let rm = ((insn >> 16) & 0x1F) as usize;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;

        let esize = 1usize << size;
        let datasize = if q == 1 { 16 } else { 8 };
        let elements = datasize / esize;

        let src1 = self.v[rn].to_le_bytes();
        let src2 = self.v[rm].to_le_bytes();
        let mut dst = [0u8; 16];

        for e in 0..elements {
            let offset = e * esize;

            match esize {
                1 => {
                    let a = src1[offset];
                    let b = src2[offset];
                    dst[offset] = match (u, opcode) {
                        (0, 0b10000) => a.wrapping_add(b), // ADD
                        (1, 0b10000) => a.wrapping_sub(b), // SUB
                        (0, 0b00011) => a & b,             // AND
                        (1, 0b00011) => a ^ b,             // EOR
                        _ => a,
                    };
                }
                2 => {
                    let a = u16::from_le_bytes([src1[offset], src1[offset + 1]]);
                    let b = u16::from_le_bytes([src2[offset], src2[offset + 1]]);
                    let result = match (u, opcode) {
                        (0, 0b10000) => a.wrapping_add(b),
                        (1, 0b10000) => a.wrapping_sub(b),
                        (0, 0b10011) => a.wrapping_mul(b), // MUL
                        (0, 0b00011) => a & b,
                        (1, 0b00011) => a ^ b,
                        _ => a,
                    };
                    let bytes = result.to_le_bytes();
                    dst[offset..offset + 2].copy_from_slice(&bytes);
                }
                4 => {
                    // Could be integer or FP depending on opcode
                    if opcode >= 0b11000 {
                        // FP operations
                        let a = f32::from_le_bytes([
                            src1[offset],
                            src1[offset + 1],
                            src1[offset + 2],
                            src1[offset + 3],
                        ]);
                        let b = f32::from_le_bytes([
                            src2[offset],
                            src2[offset + 1],
                            src2[offset + 2],
                            src2[offset + 3],
                        ]);
                        let result = match (u, opcode) {
                            (0, 0b11010) => a + b,    // FADD
                            (1, 0b11010) => a - b,    // FSUB
                            (0, 0b11011) => a * b,    // FMUL
                            (1, 0b11111) => a / b,    // FDIV
                            (0, 0b11110) => a.max(b), // FMAX
                            (1, 0b11110) => a.min(b), // FMIN
                            _ => a,
                        };
                        let bytes = result.to_le_bytes();
                        dst[offset..offset + 4].copy_from_slice(&bytes);
                    } else {
                        // Integer operations
                        let a = u32::from_le_bytes([
                            src1[offset],
                            src1[offset + 1],
                            src1[offset + 2],
                            src1[offset + 3],
                        ]);
                        let b = u32::from_le_bytes([
                            src2[offset],
                            src2[offset + 1],
                            src2[offset + 2],
                            src2[offset + 3],
                        ]);
                        let result = match (u, opcode) {
                            (0, 0b10000) => a.wrapping_add(b),
                            (1, 0b10000) => a.wrapping_sub(b),
                            (0, 0b10011) => a.wrapping_mul(b),
                            (0, 0b00011) => a & b,
                            (1, 0b00011) => a ^ b,
                            _ => a,
                        };
                        let bytes = result.to_le_bytes();
                        dst[offset..offset + 4].copy_from_slice(&bytes);
                    }
                }
                8 => {
                    if opcode >= 0b11000 {
                        // FP double
                        let a = f64::from_le_bytes([
                            src1[offset],
                            src1[offset + 1],
                            src1[offset + 2],
                            src1[offset + 3],
                            src1[offset + 4],
                            src1[offset + 5],
                            src1[offset + 6],
                            src1[offset + 7],
                        ]);
                        let b = f64::from_le_bytes([
                            src2[offset],
                            src2[offset + 1],
                            src2[offset + 2],
                            src2[offset + 3],
                            src2[offset + 4],
                            src2[offset + 5],
                            src2[offset + 6],
                            src2[offset + 7],
                        ]);
                        let result = match (u, opcode) {
                            (0, 0b11010) => a + b,
                            (1, 0b11010) => a - b,
                            (0, 0b11011) => a * b,
                            (1, 0b11111) => a / b,
                            (0, 0b11110) => a.max(b),
                            (1, 0b11110) => a.min(b),
                            _ => a,
                        };
                        let bytes = result.to_le_bytes();
                        dst[offset..offset + 8].copy_from_slice(&bytes);
                    } else {
                        // Integer 64-bit
                        let a = u64::from_le_bytes([
                            src1[offset],
                            src1[offset + 1],
                            src1[offset + 2],
                            src1[offset + 3],
                            src1[offset + 4],
                            src1[offset + 5],
                            src1[offset + 6],
                            src1[offset + 7],
                        ]);
                        let b = u64::from_le_bytes([
                            src2[offset],
                            src2[offset + 1],
                            src2[offset + 2],
                            src2[offset + 3],
                            src2[offset + 4],
                            src2[offset + 5],
                            src2[offset + 6],
                            src2[offset + 7],
                        ]);
                        let result = match (u, opcode) {
                            (0, 0b10000) => a.wrapping_add(b),
                            (1, 0b10000) => a.wrapping_sub(b),
                            (0, 0b00011) => a & b,
                            (1, 0b00011) => a ^ b,
                            _ => a,
                        };
                        let bytes = result.to_le_bytes();
                        dst[offset..offset + 8].copy_from_slice(&bytes);
                    }
                }
                _ => {}
            }
        }

        self.v[rd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }

    /// Execute SIMD two-register miscellaneous instructions.
    fn exec_simd_two_reg(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let q = (insn >> 30) & 1;
        let u = (insn >> 29) & 1;
        let size = (insn >> 22) & 0x3;
        let opcode = (insn >> 12) & 0x1F;
        let rn = ((insn >> 5) & 0x1F) as usize;
        let rd = (insn & 0x1F) as usize;

        let esize = 1usize << size;
        let datasize = if q == 1 { 16 } else { 8 };
        let elements = datasize / esize;

        let src = self.v[rn].to_le_bytes();
        let mut dst = [0u8; 16];

        for e in 0..elements {
            let offset = e * esize;

            match esize {
                1 => {
                    let a = src[offset];
                    dst[offset] = match (u, opcode) {
                        (1, 0b00101) => !a, // NOT
                        (0, 0b01011) => {
                            if (a as i8) < 0 {
                                a.wrapping_neg()
                            } else {
                                a
                            }
                        } // ABS
                        (1, 0b01011) => a.wrapping_neg(), // NEG
                        _ => a,
                    };
                }
                2 => {
                    let a = i16::from_le_bytes([src[offset], src[offset + 1]]);
                    let result = match (u, opcode) {
                        (0, 0b01011) => a.abs() as u16,
                        (1, 0b01011) => a.wrapping_neg() as u16,
                        _ => a as u16,
                    };
                    let bytes = result.to_le_bytes();
                    dst[offset..offset + 2].copy_from_slice(&bytes);
                }
                4 => {
                    if opcode >= 0b01100 && opcode <= 0b11111 {
                        // FP unary
                        let a = f32::from_le_bytes([
                            src[offset],
                            src[offset + 1],
                            src[offset + 2],
                            src[offset + 3],
                        ]);
                        let result = match (u, opcode) {
                            (0, 0b01111) => a.abs(),   // FABS
                            (1, 0b01111) => -a,        // FNEG
                            (1, 0b10111) => a.sqrt(),  // FSQRT
                            (0, 0b11000) => a.round(), // FRINTN
                            (1, 0b11000) => a.ceil(),  // FRINTP
                            (0, 0b11001) => a.floor(), // FRINTM
                            (1, 0b11001) => a.trunc(), // FRINTZ
                            _ => a,
                        };
                        let bytes = result.to_le_bytes();
                        dst[offset..offset + 4].copy_from_slice(&bytes);
                    } else {
                        // Integer
                        let a = i32::from_le_bytes([
                            src[offset],
                            src[offset + 1],
                            src[offset + 2],
                            src[offset + 3],
                        ]);
                        let result = match (u, opcode) {
                            (0, 0b01011) => a.abs() as u32,
                            (1, 0b01011) => a.wrapping_neg() as u32,
                            (1, 0b00101) => !(a as u32),
                            _ => a as u32,
                        };
                        let bytes = result.to_le_bytes();
                        dst[offset..offset + 4].copy_from_slice(&bytes);
                    }
                }
                8 => {
                    if opcode >= 0b01100 {
                        // FP double
                        let a = f64::from_le_bytes([
                            src[offset],
                            src[offset + 1],
                            src[offset + 2],
                            src[offset + 3],
                            src[offset + 4],
                            src[offset + 5],
                            src[offset + 6],
                            src[offset + 7],
                        ]);
                        let result = match (u, opcode) {
                            (0, 0b01111) => a.abs(),
                            (1, 0b01111) => -a,
                            (1, 0b10111) => a.sqrt(),
                            (0, 0b11000) => a.round(),
                            (1, 0b11000) => a.ceil(),
                            (0, 0b11001) => a.floor(),
                            (1, 0b11001) => a.trunc(),
                            _ => a,
                        };
                        let bytes = result.to_le_bytes();
                        dst[offset..offset + 8].copy_from_slice(&bytes);
                    } else {
                        let a = i64::from_le_bytes([
                            src[offset],
                            src[offset + 1],
                            src[offset + 2],
                            src[offset + 3],
                            src[offset + 4],
                            src[offset + 5],
                            src[offset + 6],
                            src[offset + 7],
                        ]);
                        let result = match (u, opcode) {
                            (0, 0b01011) => a.abs() as u64,
                            (1, 0b01011) => a.wrapping_neg() as u64,
                            _ => a as u64,
                        };
                        let bytes = result.to_le_bytes();
                        dst[offset..offset + 8].copy_from_slice(&bytes);
                    }
                }
                _ => {}
            }
        }

        self.v[rd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }

    // FP helper functions
    fn fp_maxnm_f32(&self, a: f32, b: f32) -> f32 {
        if a.is_nan() {
            b
        } else if b.is_nan() {
            a
        } else {
            a.max(b)
        }
    }

    fn fp_minnm_f32(&self, a: f32, b: f32) -> f32 {
        if a.is_nan() {
            b
        } else if b.is_nan() {
            a
        } else {
            a.min(b)
        }
    }

    fn fp_nmul_f32(&self, a: f32, b: f32) -> f32 {
        -(a * b)
    }

    fn fp_maxnm_f64(&self, a: f64, b: f64) -> f64 {
        if a.is_nan() {
            b
        } else if b.is_nan() {
            a
        } else {
            a.max(b)
        }
    }

    fn fp_minnm_f64(&self, a: f64, b: f64) -> f64 {
        if a.is_nan() {
            b
        } else if b.is_nan() {
            a
        } else {
            a.min(b)
        }
    }

    fn fp_nmul_f64(&self, a: f64, b: f64) -> f64 {
        -(a * b)
    }

    // =========================================================================
    // SVE (Scalable Vector Extension) Execution
    // =========================================================================

    /// Execute SVE instruction.
    fn exec_sve(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        // Check if SVE is enabled (CPACR_EL1.ZEN)
        let cpacr = self.sysregs.el1.cpacr;
        let zen = (cpacr >> 16) & 0x3;

        if self.current_el == 0 && zen != 0x3 {
            return Ok(CpuExit::Undefined(insn));
        }
        if self.current_el == 1 && zen == 0x0 {
            return Ok(CpuExit::Undefined(insn));
        }

        // Extract primary classification bits
        let op0 = (insn >> 29) & 0x7;
        let op1 = (insn >> 23) & 0x3;
        let op2 = (insn >> 17) & 0x1F;
        let op3 = (insn >> 10) & 0x3F;

        // Common register fields
        let zd = (insn & 0x1F) as usize;
        let zn = ((insn >> 5) & 0x1F) as usize;
        let zm = ((insn >> 16) & 0x1F) as usize;
        let pg = ((insn >> 10) & 0x7) as usize;
        let size = (insn >> 22) & 0x3;

        // Element size in bytes
        let esize = 1usize << size; // 1, 2, 4, or 8 bytes

        match op0 {
            // Integer predicated binary operations
            0b000 if (op1 & 0x2) == 0 && (op2 & 0x10) == 0 => {
                self.exec_sve_int_pred(insn, zd, zn, zm, pg, esize)
            }

            // Unpredicated arithmetic
            0b000 if op1 == 0b01 => self.exec_sve_int_unpred(insn, zd, zn, zm, esize),

            // Predicate operations (WHILE, PTRUE, etc.)
            0b001 => self.exec_sve_pred_ops(insn),

            // DUP/MOV/INDEX
            0b000 if op1 == 0b10 || op1 == 0b11 => self.exec_sve_permute(insn, zd, zn, zm, esize),

            // FP predicated operations
            0b011 => self.exec_sve_fp_pred(insn, zd, zn, zm, pg, esize),

            // Load/Store
            0b100 | 0b101 | 0b110 | 0b111 => self.exec_sve_ldst(insn),

            _ => Err(ArmError::Unimplemented(format!(
                "SVE op0={:03b} op1={:02b}",
                op0, op1
            ))),
        }
    }

    /// Execute SVE integer predicated operations.
    fn exec_sve_int_pred(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        _zm: usize,
        pg: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        let opc = (insn >> 16) & 0x7;
        let pred = self.sve_p[pg];

        // Number of elements in VL=128
        let elements = 16 / esize;

        // Get source and destination as byte arrays
        let src = self.v[zn].to_le_bytes();
        let zm_idx = ((insn >> 16) & 0x1F) as usize;
        let src2 = self.v[zm_idx].to_le_bytes();
        let mut dst = self.v[zd].to_le_bytes();

        for e in 0..elements {
            // Check predicate bit for this element
            let pred_bit = (pred >> e) & 1;
            if pred_bit == 0 {
                continue; // Skip inactive elements
            }

            let offset = e * esize;
            match esize {
                1 => {
                    let a = src[offset];
                    let b = src2[offset];
                    let result = match opc {
                        0b000 => a.wrapping_add(b),
                        0b001 => a.wrapping_sub(b),
                        0b011 => b.wrapping_sub(a), // SUBR
                        _ => a,
                    };
                    dst[offset] = result;
                }
                2 => {
                    let a = u16::from_le_bytes([src[offset], src[offset + 1]]);
                    let b = u16::from_le_bytes([src2[offset], src2[offset + 1]]);
                    let result = match opc {
                        0b000 => a.wrapping_add(b),
                        0b001 => a.wrapping_sub(b),
                        0b011 => b.wrapping_sub(a),
                        _ => a,
                    };
                    let bytes = result.to_le_bytes();
                    dst[offset] = bytes[0];
                    dst[offset + 1] = bytes[1];
                }
                4 => {
                    let a = u32::from_le_bytes([
                        src[offset],
                        src[offset + 1],
                        src[offset + 2],
                        src[offset + 3],
                    ]);
                    let b = u32::from_le_bytes([
                        src2[offset],
                        src2[offset + 1],
                        src2[offset + 2],
                        src2[offset + 3],
                    ]);
                    let result = match opc {
                        0b000 => a.wrapping_add(b),
                        0b001 => a.wrapping_sub(b),
                        0b011 => b.wrapping_sub(a),
                        _ => a,
                    };
                    let bytes = result.to_le_bytes();
                    dst[offset..offset + 4].copy_from_slice(&bytes);
                }
                8 => {
                    let a = u64::from_le_bytes([
                        src[offset],
                        src[offset + 1],
                        src[offset + 2],
                        src[offset + 3],
                        src[offset + 4],
                        src[offset + 5],
                        src[offset + 6],
                        src[offset + 7],
                    ]);
                    let b = u64::from_le_bytes([
                        src2[offset],
                        src2[offset + 1],
                        src2[offset + 2],
                        src2[offset + 3],
                        src2[offset + 4],
                        src2[offset + 5],
                        src2[offset + 6],
                        src2[offset + 7],
                    ]);
                    let result = match opc {
                        0b000 => a.wrapping_add(b),
                        0b001 => a.wrapping_sub(b),
                        0b011 => b.wrapping_sub(a),
                        _ => a,
                    };
                    let bytes = result.to_le_bytes();
                    dst[offset..offset + 8].copy_from_slice(&bytes);
                }
                _ => {}
            }
        }

        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }

    /// Execute SVE integer unpredicated operations.
    fn exec_sve_int_unpred(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        zm: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        let opc = (insn >> 10) & 0x7;
        let elements = 16 / esize;

        let src = self.v[zn].to_le_bytes();
        let src2 = self.v[zm].to_le_bytes();
        let mut dst = [0u8; 16];

        for e in 0..elements {
            let offset = e * esize;
            match esize {
                1 => {
                    let a = src[offset];
                    let b = src2[offset];
                    dst[offset] = match opc {
                        0b000 => a.wrapping_add(b),
                        0b001 => a.wrapping_sub(b),
                        _ => a,
                    };
                }
                2 => {
                    let a = u16::from_le_bytes([src[offset], src[offset + 1]]);
                    let b = u16::from_le_bytes([src2[offset], src2[offset + 1]]);
                    let result = match opc {
                        0b000 => a.wrapping_add(b),
                        0b001 => a.wrapping_sub(b),
                        _ => a,
                    };
                    let bytes = result.to_le_bytes();
                    dst[offset] = bytes[0];
                    dst[offset + 1] = bytes[1];
                }
                4 => {
                    let a = u32::from_le_bytes([
                        src[offset],
                        src[offset + 1],
                        src[offset + 2],
                        src[offset + 3],
                    ]);
                    let b = u32::from_le_bytes([
                        src2[offset],
                        src2[offset + 1],
                        src2[offset + 2],
                        src2[offset + 3],
                    ]);
                    let result = match opc {
                        0b000 => a.wrapping_add(b),
                        0b001 => a.wrapping_sub(b),
                        _ => a,
                    };
                    let bytes = result.to_le_bytes();
                    dst[offset..offset + 4].copy_from_slice(&bytes);
                }
                8 => {
                    let a = u64::from_le_bytes([
                        src[offset],
                        src[offset + 1],
                        src[offset + 2],
                        src[offset + 3],
                        src[offset + 4],
                        src[offset + 5],
                        src[offset + 6],
                        src[offset + 7],
                    ]);
                    let b = u64::from_le_bytes([
                        src2[offset],
                        src2[offset + 1],
                        src2[offset + 2],
                        src2[offset + 3],
                        src2[offset + 4],
                        src2[offset + 5],
                        src2[offset + 6],
                        src2[offset + 7],
                    ]);
                    let result = match opc {
                        0b000 => a.wrapping_add(b),
                        0b001 => a.wrapping_sub(b),
                        _ => a,
                    };
                    let bytes = result.to_le_bytes();
                    dst[offset..offset + 8].copy_from_slice(&bytes);
                }
                _ => {}
            }
        }

        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }

    /// Execute SVE predicate operations (PTRUE, PFALSE, WHILE, etc.).
    fn exec_sve_pred_ops(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let op1 = (insn >> 23) & 0x3;
        let op3 = (insn >> 10) & 0x3F;

        match op1 {
            // WHILE instructions
            0b00 => {
                let lt = (insn >> 4) & 1;
                let sf = (insn >> 12) & 1;
                let pd = (insn & 0xF) as usize;
                let rn = ((insn >> 5) & 0x1F) as u8;
                let rm = ((insn >> 16) & 0x1F) as u8;
                let size = (insn >> 22) & 0x3;

                let op1_val = if sf == 1 {
                    self.get_x(rn) as i64
                } else {
                    self.get_w(rn) as i32 as i64
                };
                let op2_val = if sf == 1 {
                    self.get_x(rm) as i64
                } else {
                    self.get_w(rm) as i32 as i64
                };

                let esize = 1usize << size;
                let elements = 16 / esize;
                let mut pred = 0u32;

                for e in 0..elements {
                    let idx = op1_val.wrapping_add(e as i64);
                    let active = if lt == 1 {
                        idx < op2_val // WHILELT
                    } else {
                        idx <= op2_val // WHILELE
                    };
                    if active {
                        pred |= 1 << e;
                    }
                }

                self.sve_p[pd] = pred;

                // Update condition flags based on predicate result
                let all_false = pred == 0;
                let last = ((pred >> (elements - 1)) & 1) != 0;
                self.set_n(!all_false && !last);
                self.set_z(all_false);
                self.set_c(!all_false);
                self.set_v(false);

                Ok(CpuExit::Continue)
            }

            // PTRUE/PFALSE
            0b01 if (op3 & 0x30) == 0x10 => {
                let s = (insn >> 16) & 1;
                let pd = (insn & 0xF) as usize;
                let size = (insn >> 22) & 0x3;
                let pattern = (insn >> 5) & 0x1F;

                let esize = 1usize << size;
                let elements = 16 / esize;

                if s == 1 {
                    // PFALSE
                    self.sve_p[pd] = 0;
                } else {
                    // PTRUE - set active elements based on pattern
                    let active_count = match pattern {
                        0b00000 => 1, // POW2
                        0b00001..=0b00111 => (pattern as usize).min(elements),
                        0b11101 => elements / 4,
                        0b11110 => elements / 2,
                        0b11111 => elements, // ALL
                        _ => elements,
                    };
                    let mut pred = 0u32;
                    for e in 0..active_count.min(elements) {
                        pred |= 1 << e;
                    }
                    self.sve_p[pd] = pred;
                }

                Ok(CpuExit::Continue)
            }

            _ => Err(ArmError::Unimplemented(format!(
                "SVE predicate op1={:02b} op3={:06b}",
                op1, op3
            ))),
        }
    }

    /// Execute SVE permute operations (DUP, INDEX, REV, etc.).
    fn exec_sve_permute(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        zm: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        let op1 = (insn >> 23) & 0x3;
        let op3 = (insn >> 10) & 0x3F;

        match op1 {
            // INDEX
            0b11 if (insn >> 17) & 0xF == 0 => {
                let rn = ((insn >> 5) & 0x1F) as u8;
                let rm = ((insn >> 16) & 0x1F) as u8;
                let elements = 16 / esize;

                let start = self.get_x(rn) as i64;
                let incr = self.get_x(rm) as i64;

                let mut dst = [0u8; 16];
                for e in 0..elements {
                    let val = start.wrapping_add((e as i64).wrapping_mul(incr));
                    let offset = e * esize;
                    match esize {
                        1 => dst[offset] = val as u8,
                        2 => {
                            let bytes = (val as u16).to_le_bytes();
                            dst[offset..offset + 2].copy_from_slice(&bytes);
                        }
                        4 => {
                            let bytes = (val as u32).to_le_bytes();
                            dst[offset..offset + 4].copy_from_slice(&bytes);
                        }
                        8 => {
                            let bytes = (val as u64).to_le_bytes();
                            dst[offset..offset + 8].copy_from_slice(&bytes);
                        }
                        _ => {}
                    }
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // DUP (scalar)
            0b10 if (op3 & 0x3E) == 0x20 => {
                let rn = ((insn >> 5) & 0x1F) as u8;
                let val = self.get_x(rn);
                let elements = 16 / esize;

                let mut dst = [0u8; 16];
                for e in 0..elements {
                    let offset = e * esize;
                    match esize {
                        1 => dst[offset] = val as u8,
                        2 => {
                            let bytes = (val as u16).to_le_bytes();
                            dst[offset..offset + 2].copy_from_slice(&bytes);
                        }
                        4 => {
                            let bytes = (val as u32).to_le_bytes();
                            dst[offset..offset + 4].copy_from_slice(&bytes);
                        }
                        8 => {
                            let bytes = val.to_le_bytes();
                            dst[offset..offset + 8].copy_from_slice(&bytes);
                        }
                        _ => {}
                    }
                }
                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // ZIP/UZP/TRN
            0b10 if (op3 & 0x30) == 0x00 => {
                let opc = (insn >> 10) & 0x7;
                let elements = 16 / esize;
                let src1 = self.v[zn].to_le_bytes();
                let src2 = self.v[zm].to_le_bytes();
                let mut dst = [0u8; 16];

                match opc {
                    // ZIP1 - interleave lower halves
                    0b000 => {
                        let half = elements / 2;
                        for e in 0..half {
                            for b in 0..esize {
                                dst[e * 2 * esize + b] = src1[e * esize + b];
                                dst[(e * 2 + 1) * esize + b] = src2[e * esize + b];
                            }
                        }
                    }
                    // ZIP2 - interleave upper halves
                    0b001 => {
                        let half = elements / 2;
                        for e in 0..half {
                            let src_off = (half + e) * esize;
                            for b in 0..esize {
                                dst[e * 2 * esize + b] = src1[src_off + b];
                                dst[(e * 2 + 1) * esize + b] = src2[src_off + b];
                            }
                        }
                    }
                    // UZP1 - even elements
                    0b010 => {
                        let half = elements / 2;
                        for e in 0..half {
                            for b in 0..esize {
                                dst[e * esize + b] = src1[e * 2 * esize + b];
                                dst[(half + e) * esize + b] = src2[e * 2 * esize + b];
                            }
                        }
                    }
                    // UZP2 - odd elements
                    0b011 => {
                        let half = elements / 2;
                        for e in 0..half {
                            for b in 0..esize {
                                dst[e * esize + b] = src1[(e * 2 + 1) * esize + b];
                                dst[(half + e) * esize + b] = src2[(e * 2 + 1) * esize + b];
                            }
                        }
                    }
                    // TRN1 - transpose even elements
                    0b100 => {
                        for e in 0..elements / 2 {
                            for b in 0..esize {
                                dst[e * 2 * esize + b] = src1[e * 2 * esize + b];
                                dst[(e * 2 + 1) * esize + b] = src2[e * 2 * esize + b];
                            }
                        }
                    }
                    // TRN2 - transpose odd elements
                    0b101 => {
                        for e in 0..elements / 2 {
                            for b in 0..esize {
                                dst[e * 2 * esize + b] = src1[(e * 2 + 1) * esize + b];
                                dst[(e * 2 + 1) * esize + b] = src2[(e * 2 + 1) * esize + b];
                            }
                        }
                    }
                    _ => {
                        return Err(ArmError::Unimplemented(format!(
                            "SVE ZIP/UZP/TRN opc={}",
                            opc
                        )))
                    }
                }

                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // REV
            0b10 if (op3 & 0x38) == 0x18 => {
                let elements = 16 / esize;
                let src = self.v[zn].to_le_bytes();
                let mut dst = [0u8; 16];

                for e in 0..elements {
                    let src_e = elements - 1 - e;
                    for b in 0..esize {
                        dst[e * esize + b] = src[src_e * esize + b];
                    }
                }

                self.v[zd] = u128::from_le_bytes(dst);
                Ok(CpuExit::Continue)
            }

            // RDVL - read vector length
            0b11 if (insn >> 17) & 0x1F == 0x1F && (op3 & 0x3E) == 0x10 => {
                let rd = (insn & 0x1F) as u8;
                let imm6 = ((insn >> 5) & 0x3F) as i64;
                let imm = if imm6 & 0x20 != 0 { imm6 | !0x3F } else { imm6 };
                // VL in bytes
                let vl_bytes = (self.sve_vl / 8) as i64;
                let result = (vl_bytes * imm) as u64;
                self.set_x(rd, result);
                Ok(CpuExit::Continue)
            }

            // CNTx - count elements
            0b11 if (insn >> 17) & 0x18 == 0x10 => {
                let rd = (insn & 0x1F) as u8;
                let opc = (insn >> 16) & 0x7;
                let pattern = (insn >> 5) & 0x1F;
                let imm4 = ((insn >> 16) & 0xF) as u64;

                let esize_bits = match opc {
                    0b000 => 8,  // CNTB
                    0b001 => 16, // CNTH
                    0b010 => 32, // CNTW
                    0b011 => 64, // CNTD
                    _ => 8,
                };

                let elements = (self.sve_vl as u64) / esize_bits;
                let count = match pattern {
                    0b11111 => elements, // ALL
                    _ => elements,
                };

                self.set_x(rd, count * imm4.max(1));
                Ok(CpuExit::Continue)
            }

            _ => Err(ArmError::Unimplemented(format!(
                "SVE permute op1={:02b} op3={:06b}",
                op1, op3
            ))),
        }
    }

    /// Execute SVE FP predicated operations.
    fn exec_sve_fp_pred(
        &mut self,
        insn: u32,
        zd: usize,
        zn: usize,
        zm: usize,
        pg: usize,
        esize: usize,
    ) -> Result<CpuExit, ArmError> {
        let opc = (insn >> 16) & 0xF;
        let pred = self.sve_p[pg];
        let elements = 16 / esize;

        let src = self.v[zn].to_le_bytes();
        let src2 = self.v[zm].to_le_bytes();
        let mut dst = self.v[zd].to_le_bytes();

        for e in 0..elements {
            if (pred >> e) & 1 == 0 {
                continue;
            }

            let offset = e * esize;
            match esize {
                4 => {
                    // Single precision
                    let a = f32::from_le_bytes([
                        src[offset],
                        src[offset + 1],
                        src[offset + 2],
                        src[offset + 3],
                    ]);
                    let b = f32::from_le_bytes([
                        src2[offset],
                        src2[offset + 1],
                        src2[offset + 2],
                        src2[offset + 3],
                    ]);

                    let result = match opc {
                        0b0000 => a + b,    // FADD
                        0b0001 => a - b,    // FSUB
                        0b0010 => a * b,    // FMUL
                        0b0011 => a / b,    // FDIV
                        0b0100 => a.min(b), // FMIN
                        0b0101 => a.max(b), // FMAX
                        0b1000 => a.abs(),  // FABS (unary)
                        0b1001 => -a,       // FNEG (unary)
                        0b1010 => a.sqrt(), // FSQRT (unary)
                        _ => a,
                    };

                    let bytes = result.to_le_bytes();
                    dst[offset..offset + 4].copy_from_slice(&bytes);
                }
                8 => {
                    // Double precision
                    let a = f64::from_le_bytes([
                        src[offset],
                        src[offset + 1],
                        src[offset + 2],
                        src[offset + 3],
                        src[offset + 4],
                        src[offset + 5],
                        src[offset + 6],
                        src[offset + 7],
                    ]);
                    let b = f64::from_le_bytes([
                        src2[offset],
                        src2[offset + 1],
                        src2[offset + 2],
                        src2[offset + 3],
                        src2[offset + 4],
                        src2[offset + 5],
                        src2[offset + 6],
                        src2[offset + 7],
                    ]);

                    let result = match opc {
                        0b0000 => a + b,
                        0b0001 => a - b,
                        0b0010 => a * b,
                        0b0011 => a / b,
                        0b0100 => a.min(b),
                        0b0101 => a.max(b),
                        0b1000 => a.abs(),
                        0b1001 => -a,
                        0b1010 => a.sqrt(),
                        _ => a,
                    };

                    let bytes = result.to_le_bytes();
                    dst[offset..offset + 8].copy_from_slice(&bytes);
                }
                _ => {}
            }
        }

        self.v[zd] = u128::from_le_bytes(dst);
        Ok(CpuExit::Continue)
    }

    /// Execute SVE load/store instructions.
    fn exec_sve_ldst(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let op0 = (insn >> 29) & 0x7;
        let msz = (insn >> 23) & 0x3;
        let is_load = (op0 & 0x1) == 0;

        let zt = (insn & 0x1F) as usize;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let pg = ((insn >> 10) & 0x7) as usize;

        let esize = 1usize << msz;
        let elements = 16 / esize;
        let pred = self.sve_p[pg];

        // Get base address
        let base = if rn == 31 {
            self.current_sp()
        } else {
            self.get_x(rn)
        };

        // Get immediate offset (scaled by VL)
        let imm4 = ((insn >> 16) & 0xF) as i64;
        let offset = if imm4 & 0x8 != 0 {
            (imm4 | !0xF) * 16 // Sign extend and scale by VL bytes
        } else {
            imm4 * 16
        };

        let address = (base as i64 + offset) as u64;

        if is_load {
            let mut dst = [0u8; 16];
            for e in 0..elements {
                if (pred >> e) & 1 == 0 {
                    continue; // Skip inactive elements (zeroing)
                }

                let elem_addr = address + (e * esize) as u64;
                let pa = self.translate_address(elem_addr, false, false)?;

                match esize {
                    1 => {
                        dst[e] = self.memory.read_u8(pa)?;
                    }
                    2 => {
                        let val = self.memory.read_u16(pa)?;
                        let bytes = val.to_le_bytes();
                        dst[e * 2..e * 2 + 2].copy_from_slice(&bytes);
                    }
                    4 => {
                        let val = self.memory.read_u32(pa)?;
                        let bytes = val.to_le_bytes();
                        dst[e * 4..e * 4 + 4].copy_from_slice(&bytes);
                    }
                    8 => {
                        let val = self.memory.read_u64(pa)?;
                        let bytes = val.to_le_bytes();
                        dst[e * 8..e * 8 + 8].copy_from_slice(&bytes);
                    }
                    _ => {}
                }
            }
            self.v[zt] = u128::from_le_bytes(dst);
        } else {
            let src = self.v[zt].to_le_bytes();
            for e in 0..elements {
                if (pred >> e) & 1 == 0 {
                    continue; // Skip inactive elements
                }

                let elem_addr = address + (e * esize) as u64;
                let pa = self.translate_address(elem_addr, true, false)?;

                match esize {
                    1 => {
                        self.memory.write_u8(pa, src[e])?;
                    }
                    2 => {
                        let val = u16::from_le_bytes([src[e * 2], src[e * 2 + 1]]);
                        self.memory.write_u16(pa, val)?;
                    }
                    4 => {
                        let val = u32::from_le_bytes([
                            src[e * 4],
                            src[e * 4 + 1],
                            src[e * 4 + 2],
                            src[e * 4 + 3],
                        ]);
                        self.memory.write_u32(pa, val)?;
                    }
                    8 => {
                        let val = u64::from_le_bytes([
                            src[e * 8],
                            src[e * 8 + 1],
                            src[e * 8 + 2],
                            src[e * 8 + 3],
                            src[e * 8 + 4],
                            src[e * 8 + 5],
                            src[e * 8 + 6],
                            src[e * 8 + 7],
                        ]);
                        self.memory.write_u64(pa, val)?;
                    }
                    _ => {}
                }
            }
        }

        Ok(CpuExit::Continue)
    }

    // =========================================================================
    // Instruction Implementations (stubs - to be filled in)
    // =========================================================================

    fn exec_pc_rel(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let op = (insn >> 31) & 1;
        let rd = (insn & 0x1F) as u8;
        let immhi = ((insn >> 5) & 0x7FFFF) as i64;
        let immlo = ((insn >> 29) & 0x3) as i64;
        let imm = (immhi << 2) | immlo;
        let imm = (imm << 43) >> 43; // Sign extend from 21 bits

        // PC was already incremented, use the address of this instruction
        let current_pc = self.pc.wrapping_sub(4);

        let result = if op == 0 {
            // ADR
            (current_pc as i64).wrapping_add(imm) as u64
        } else {
            // ADRP
            let base = current_pc & !0xFFF;
            (base as i64).wrapping_add(imm << 12) as u64
        };

        self.set_x(rd, result);
        Ok(CpuExit::Continue)
    }

    fn exec_add_sub_imm(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let sf = (insn >> 31) & 1;
        let op = (insn >> 30) & 1; // 0=ADD, 1=SUB
        let s = (insn >> 29) & 1; // Set flags
        let sh = (insn >> 22) & 1; // Shift imm by 12
        let imm12 = ((insn >> 10) & 0xFFF) as u64;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let rd = (insn & 0x1F) as u8;

        let imm = if sh != 0 { imm12 << 12 } else { imm12 };

        if sf != 0 {
            // 64-bit
            let rn_val = if rn == 31 {
                self.current_sp()
            } else {
                self.get_x(rn)
            };

            let (result, carry, overflow) = if op == 0 {
                let (r, c) = rn_val.overflowing_add(imm);
                let v = (!(rn_val ^ imm) & (rn_val ^ r)) >> 63 != 0;
                (r, c, v)
            } else {
                let (r, c) = rn_val.overflowing_sub(imm);
                let v = ((rn_val ^ imm) & (rn_val ^ r)) >> 63 != 0;
                (r, !c, v)
            };

            if s != 0 {
                self.update_nz_64(result);
                self.set_c(carry);
                self.set_v(overflow);
            }

            if rd == 31 {
                if s == 0 {
                    self.set_current_sp(result);
                }
            } else {
                self.set_x(rd, result);
            }
        } else {
            // 32-bit
            let rn_val = if rn == 31 {
                self.current_sp() as u32
            } else {
                self.get_w(rn)
            };
            let imm = imm as u32;

            let (result, carry, overflow) = if op == 0 {
                let (r, c) = rn_val.overflowing_add(imm);
                let v = (!(rn_val ^ imm) & (rn_val ^ r)) >> 31 != 0;
                (r, c, v)
            } else {
                let (r, c) = rn_val.overflowing_sub(imm);
                let v = ((rn_val ^ imm) & (rn_val ^ r)) >> 31 != 0;
                (r, !c, v)
            };

            if s != 0 {
                self.update_nz_32(result);
                self.set_c(carry);
                self.set_v(overflow);
            }

            if rd == 31 {
                if s == 0 {
                    self.set_current_sp(result as u64);
                }
            } else {
                self.set_w(rd, result);
            }
        }

        Ok(CpuExit::Continue)
    }

    /// Execute Add/Sub Immediate with Tags (ADDG/SUBG - MTE instructions).
    ///
    /// Encoding:
    /// 31:31 sf (must be 1 for 64-bit)
    /// 30:30 op (0=ADD, 1=SUB)
    /// 29:29 S (must be 0)
    /// 28:23 100011
    /// 22:22 o2 (must be 0)
    /// 21:16 uimm6 (offset in 16-byte granules)
    /// 15:14 op3
    /// 13:10 uimm4 (tag offset)
    /// 9:5   Xn (source register)
    /// 4:0   Xd (destination register)
    fn exec_add_sub_imm_tags(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let op = (insn >> 30) & 1; // 0=ADDG, 1=SUBG
        let uimm6 = ((insn >> 16) & 0x3F) as u64;
        let uimm4 = ((insn >> 10) & 0xF) as u8;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let rd = (insn & 0x1F) as u8;

        // TAG_GRANULE is 16 bytes (LOG2_TAG_GRANULE = 4)
        const TAG_GRANULE: u64 = 16;
        let offset = uimm6 * TAG_GRANULE;

        // Get source operand
        let operand1 = if rn == 31 {
            self.current_sp()
        } else {
            self.get_x(rn)
        };

        // Extract the current allocation tag from address bits [59:56]
        let start_tag = ((operand1 >> 56) & 0xF) as u8;

        // Compute new tag (simplified - in full MTE, this uses GCR_EL1.Exclude)
        // The tag is modified by uimm4, wrapping at 16
        let rtag = if self.config.features.has_mte() {
            // MTE enabled - compute new tag
            (start_tag.wrapping_add(uimm4)) & 0xF
        } else {
            // MTE disabled - tag is 0
            0
        };

        // Compute result address
        let result = if op == 0 {
            // ADDG
            operand1.wrapping_add(offset)
        } else {
            // SUBG
            operand1.wrapping_sub(offset)
        };

        // Insert the new allocation tag into the result address
        // Tags are stored in bits [59:56] (top byte, lower nibble)
        let result = (result & !0x0F00_0000_0000_0000u64) | ((rtag as u64) << 56);

        // Write result
        if rd == 31 {
            self.set_current_sp(result);
        } else {
            self.set_x(rd, result);
        }

        Ok(CpuExit::Continue)
    }

    fn exec_logical_imm(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let sf = (insn >> 31) & 1;
        let opc = (insn >> 29) & 0x3;
        let n = (insn >> 22) & 1;
        let immr = ((insn >> 16) & 0x3F) as u32;
        let imms = ((insn >> 10) & 0x3F) as u32;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let rd = (insn & 0x1F) as u8;

        // Decode bitmask immediate
        let imm = decode_bitmask(n != 0, imms, immr, sf != 0)?;

        if sf != 0 {
            // 64-bit
            let rn_val = self.get_x(rn);

            let result = match opc {
                0b00 => rn_val & imm, // AND
                0b01 => rn_val | imm, // ORR
                0b10 => rn_val ^ imm, // EOR
                0b11 => rn_val & imm, // ANDS
                _ => unreachable!(),
            };

            if opc == 0b11 {
                self.update_nz_64(result);
                self.set_c(false);
                self.set_v(false);
            }

            if rd == 31 && opc != 0b11 {
                self.set_current_sp(result);
            } else {
                self.set_x(rd, result);
            }
        } else {
            // 32-bit
            let rn_val = self.get_w(rn);
            let imm = imm as u32;

            let result = match opc {
                0b00 => rn_val & imm,
                0b01 => rn_val | imm,
                0b10 => rn_val ^ imm,
                0b11 => rn_val & imm,
                _ => unreachable!(),
            };

            if opc == 0b11 {
                self.update_nz_32(result);
                self.set_c(false);
                self.set_v(false);
            }

            if rd == 31 && opc != 0b11 {
                self.set_current_sp(result as u64);
            } else {
                self.set_w(rd, result);
            }
        }

        Ok(CpuExit::Continue)
    }

    fn exec_move_wide(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let sf = (insn >> 31) & 1;
        let opc = (insn >> 29) & 0x3;
        let hw = ((insn >> 21) & 0x3) as u32;
        let imm16 = ((insn >> 5) & 0xFFFF) as u64;
        let rd = (insn & 0x1F) as u8;

        let shift = hw * 16;

        let result = match opc {
            0b00 => {
                // MOVN
                let val = imm16 << shift;
                if sf != 0 {
                    !val
                } else {
                    (!val) & 0xFFFF_FFFF
                }
            }
            0b10 => {
                // MOVZ
                imm16 << shift
            }
            0b11 => {
                // MOVK
                let old = if sf != 0 {
                    self.get_x(rd)
                } else {
                    self.get_w(rd) as u64
                };
                let mask = !(0xFFFFu64 << shift);
                (old & mask) | (imm16 << shift)
            }
            _ => return Err(ArmError::UndefinedInstruction(insn)),
        };

        if sf != 0 {
            self.set_x(rd, result);
        } else {
            self.set_w(rd, result as u32);
        }

        Ok(CpuExit::Continue)
    }

    fn exec_bitfield(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let sf = (insn >> 31) & 1;
        let opc = (insn >> 29) & 0x3;
        let n = (insn >> 22) & 1;
        let immr = ((insn >> 16) & 0x3F) as u32;
        let imms = ((insn >> 10) & 0x3F) as u32;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let rd = (insn & 0x1F) as u8;

        let datasize = if sf != 0 { 64u32 } else { 32 };

        // Decode wmask and tmask
        let (wmask, tmask) = decode_bitmasks(n != 0, imms, immr, false, datasize)?;

        let src = if sf != 0 {
            self.get_x(rn)
        } else {
            self.get_w(rn) as u64
        };

        let dst = if sf != 0 {
            self.get_x(rd)
        } else {
            self.get_w(rd) as u64
        };

        // Rotate right
        let bot = if immr == 0 {
            src
        } else {
            (src >> immr) | (src << (datasize - immr))
        };

        let result = match opc {
            0b00 => {
                // SBFM
                // Sign-extend based on imms
                let top = if (src >> imms) & 1 != 0 { !0u64 } else { 0u64 };
                (top & !tmask) | (bot & wmask)
            }
            0b01 => {
                // BFM
                (dst & !tmask) | (bot & wmask & tmask)
            }
            0b10 => {
                // UBFM
                bot & wmask
            }
            _ => return Err(ArmError::UndefinedInstruction(insn)),
        };

        if sf != 0 {
            self.set_x(rd, result);
        } else {
            self.set_w(rd, result as u32);
        }

        Ok(CpuExit::Continue)
    }

    fn exec_extract(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let sf = (insn >> 31) & 1;
        let n = (insn >> 22) & 1;
        let rm = ((insn >> 16) & 0x1F) as u8;
        let imms = ((insn >> 10) & 0x3F) as u32;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let rd = (insn & 0x1F) as u8;

        let datasize = if sf != 0 { 64u32 } else { 32 };
        let lsb = imms;

        let operand1 = if sf != 0 {
            self.get_x(rn)
        } else {
            self.get_w(rn) as u64
        };

        let operand2 = if sf != 0 {
            self.get_x(rm)
        } else {
            self.get_w(rm) as u64
        };

        let result = if lsb == 0 {
            operand2
        } else {
            (operand1 << (datasize - lsb)) | (operand2 >> lsb)
        };

        if sf != 0 {
            self.set_x(rd, result);
        } else {
            self.set_w(rd, result as u32);
        }

        Ok(CpuExit::Continue)
    }

    fn exec_b_cond(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let imm19 = ((insn >> 5) & 0x7FFFF) as i64;
        let cond = (insn & 0xF) as u8;

        let offset = ((imm19 << 45) >> 43) as i64; // Sign extend and multiply by 4

        if self.condition_holds(cond) {
            self.pc = ((self.pc as i64).wrapping_sub(4).wrapping_add(offset)) as u64;
        }

        Ok(CpuExit::Continue)
    }

    fn exec_exception_system(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        // Check bits [31:24] to distinguish exception generation from system instructions
        let bits_31_24 = (insn >> 24) & 0xFF;

        if bits_31_24 == 0xD4 {
            // Exception generation (SVC, HVC, SMC, BRK, HLT)
            let opc = (insn >> 21) & 0x7;
            let imm16 = ((insn >> 5) & 0xFFFF) as u16;

            return match opc {
                0b000 => {
                    // SVC
                    Ok(CpuExit::Svc(imm16 as u32))
                }
                0b001 => {
                    // HVC
                    Ok(CpuExit::Hvc(imm16))
                }
                0b010 => {
                    // SMC
                    Ok(CpuExit::Smc(imm16))
                }
                0b011 => {
                    // BRK
                    Ok(CpuExit::Breakpoint(imm16 as u32))
                }
                0b100 => {
                    // HLT
                    self.halted = true;
                    Ok(CpuExit::Halt)
                }
                _ => Err(ArmError::UndefinedInstruction(insn)),
            };
        }

        // bits [31:22] = 0x354 (1101_0101_00) = system instructions
        // This covers hints, barriers, MSR, MRS, etc.
        let l = (insn >> 21) & 1;
        let op0 = (insn >> 19) & 0x3;

        if l == 0 && op0 == 0 {
            // System instructions with L=0, op0=00 (hints, barriers)
            return self.exec_system(insn);
        }

        // MSR/MRS (system register access)
        // L=0: MSR (write), L=1: MRS (read)
        // op0 = 01, 10, or 11 for different register categories
        if op0 != 0 || l == 1 {
            return self.exec_msr_mrs(insn);
        }

        Err(ArmError::UndefinedInstruction(insn))
    }

    fn exec_system(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let crn = ((insn >> 12) & 0xF) as u8;
        let op1 = ((insn >> 16) & 0x7) as u8;
        let op2 = ((insn >> 5) & 0x7) as u8;

        if crn == 2 && op1 == 3 {
            // Hints
            match op2 {
                0b000 => Ok(CpuExit::Continue), // NOP
                0b001 => Ok(CpuExit::Continue), // YIELD
                0b010 => {
                    // WFE
                    if self.event_register {
                        self.event_register = false;
                        Ok(CpuExit::Continue)
                    } else {
                        self.wfe = true;
                        Ok(CpuExit::Wfe)
                    }
                }
                0b011 => {
                    // WFI
                    self.wfi = true;
                    Ok(CpuExit::Wfi)
                }
                0b100 => {
                    // SEV
                    self.event_register = true;
                    Ok(CpuExit::Continue)
                }
                0b101 => {
                    // SEVL
                    self.event_register = true;
                    Ok(CpuExit::Continue)
                }
                _ => Ok(CpuExit::Continue),
            }
        } else if crn == 3 {
            // Barriers
            match op2 {
                0b010 => Ok(CpuExit::Continue), // CLREX
                0b100 => Ok(CpuExit::Continue), // DSB
                0b101 => Ok(CpuExit::Continue), // DMB
                0b110 => Ok(CpuExit::Continue), // ISB
                _ => Ok(CpuExit::Continue),
            }
        } else {
            // Other system instructions (cache maintenance, etc.)
            Ok(CpuExit::Continue)
        }
    }

    fn exec_msr_mrs(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let l = (insn >> 21) & 1; // 0 = MSR, 1 = MRS
        let o0 = ((insn >> 19) & 0x1) as u8 + 2;
        let op1 = ((insn >> 16) & 0x7) as u8;
        let crn = ((insn >> 12) & 0xF) as u8;
        let crm = ((insn >> 8) & 0xF) as u8;
        let op2 = ((insn >> 5) & 0x7) as u8;
        let rt = (insn & 0x1F) as u8;

        let encoding = Aarch64SysRegEncoding::new(o0, op1, crn, crm, op2);

        if l != 0 {
            // MRS
            let value = self.read_sysreg(encoding)?;
            self.set_x(rt, value);
        } else {
            // MSR
            let value = self.get_x(rt);
            self.write_sysreg(encoding, value)?;
        }

        Ok(CpuExit::Continue)
    }

    fn exec_br_reg(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let opc = (insn >> 21) & 0xF;
        let op2 = (insn >> 16) & 0x1F;
        let op3 = (insn >> 10) & 0x3F;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let op4 = insn & 0x1F;

        if op2 != 0x1F || op3 != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }

        let target = self.get_x(rn);

        match (opc, op4) {
            (0b0000, 0) => {
                // BR
                self.pc = target;
                self.btype = 0b01;
            }
            (0b0001, 0) => {
                // BLR
                self.set_x(30, self.pc);
                self.pc = target;
                self.btype = 0b10;
            }
            (0b0010, 0) => {
                // RET
                let lr = if rn == 31 { 30 } else { rn };
                self.pc = self.get_x(lr);
            }
            (0b0100, 0) => {
                // ERET
                return self.exception_return();
            }
            (0b0101, 0) => {
                // DRPS
                return Err(ArmError::Unimplemented("DRPS".to_string()));
            }
            _ => return Err(ArmError::UndefinedInstruction(insn)),
        }

        Ok(CpuExit::Continue)
    }

    fn exec_cbz_cbnz(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let sf = (insn >> 31) & 1;
        let op = (insn >> 24) & 1; // 0=CBZ, 1=CBNZ
        let imm19 = ((insn >> 5) & 0x7FFFF) as i64;
        let rt = (insn & 0x1F) as u8;

        let offset = ((imm19 << 45) >> 43) as i64;
        let operand = if sf != 0 {
            self.get_x(rt)
        } else {
            self.get_w(rt) as u64
        };

        let take_branch = if op == 0 { operand == 0 } else { operand != 0 };

        if take_branch {
            self.pc = ((self.pc as i64).wrapping_sub(4).wrapping_add(offset)) as u64;
        }

        Ok(CpuExit::Continue)
    }

    fn exec_tbz_tbnz(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let b5 = (insn >> 31) & 1;
        let op = (insn >> 24) & 1; // 0=TBZ, 1=TBNZ
        let b40 = ((insn >> 19) & 0x1F) as u32;
        let imm14 = ((insn >> 5) & 0x3FFF) as i64;
        let rt = (insn & 0x1F) as u8;

        let bit_pos = (b5 << 5) | b40;
        let offset = ((imm14 << 50) >> 48) as i64;
        let operand = self.get_x(rt);
        let bit_set = (operand >> bit_pos) & 1 != 0;

        let take_branch = if op == 0 { !bit_set } else { bit_set };

        if take_branch {
            self.pc = ((self.pc as i64).wrapping_sub(4).wrapping_add(offset)) as u64;
        }

        Ok(CpuExit::Continue)
    }

    fn exec_b_bl(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let op = (insn >> 31) & 1; // 0=B, 1=BL
        let imm26 = (insn & 0x03FF_FFFF) as i64;

        let offset = ((imm26 << 38) >> 36) as i64; // Sign extend and multiply by 4

        if op != 0 {
            // BL - save return address
            self.set_x(30, self.pc);
            self.btype = 0b10;
        }

        self.pc = ((self.pc as i64).wrapping_sub(4).wrapping_add(offset)) as u64;

        Ok(CpuExit::Continue)
    }

    // Load/Store implementations
    /// Execute Load/Store Exclusive instructions (LDXR, STXR, LDAXR, STLXR, etc.)
    ///
    /// Encoding (from ASL):
    /// 31:30 size (00=8-bit, 01=16-bit, 10=32-bit, 11=64-bit)
    /// 29:24 001000
    /// 23:23 o2 (pair indicator)
    /// 22:22 L (0=store, 1=load)
    /// 21:21 o1
    /// 20:16 Rs (status register for store)
    /// 15:15 o0 (1=acquire/release semantics)
    /// 14:10 Rt2 (for pair)
    /// 9:5   Rn (base register)
    /// 4:0   Rt (data register)
    fn exec_ldst_exclusive(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let size = (insn >> 30) & 0x3;
        let o2 = (insn >> 23) & 0x1; // 1 = pair
        let l = (insn >> 22) & 0x1; // 1 = load, 0 = store
        let o1 = (insn >> 21) & 0x1;
        let rs = ((insn >> 16) & 0x1F) as u8;
        let o0 = (insn >> 15) & 0x1; // 1 = acquire/release
        let rt2 = ((insn >> 10) & 0x1F) as u8;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let rt = (insn & 0x1F) as u8;

        let is_pair = o2 == 1;
        let is_load = l == 1;
        let is_ordered = o0 == 1; // acquire/release semantics (LDAXR/STLXR)

        // Element size in bytes
        let elsize = 1usize << size; // 1, 2, 4, or 8 bytes
        let datasize = if is_pair { elsize * 2 } else { elsize };

        // Get address from base register
        let address = if rn == 31 {
            // SP - check alignment
            let sp = self.current_sp();
            // SP must be aligned to 16 bytes for stack access
            if sp & 0xF != 0 {
                return Err(ArmError::MemoryError(MemoryFaultInfo {
                    address: sp,
                    access: crate::arm::cpu_trait::AccessType::Read,
                    fault_type: MemoryFaultType::Alignment,
                    stage2: false,
                }));
            }
            sp
        } else {
            self.get_x(rn)
        };

        // Translate address (for physical memory access)
        let pa = self.translate_address(address, !is_load, false)?;

        if is_load {
            // Load exclusive: LDXR, LDAXR, LDXP, LDAXP

            // Set exclusive monitors for this address range
            self.memory.mark_exclusive(pa, datasize as u8);

            if is_pair {
                // Load pair (LDXP, LDAXP)
                if elsize == 4 {
                    // 32-bit pair - atomic 64-bit load
                    let data = self.memory.read_u64(pa)?;
                    // Little-endian: lower register gets lower bits
                    self.set_w(rt, data as u32);
                    self.set_w(rt2, (data >> 32) as u32);
                } else {
                    // 64-bit pair - two 64-bit loads (128-bit aligned)
                    if pa & 0xF != 0 {
                        return Err(ArmError::MemoryError(MemoryFaultInfo {
                            address,
                            access: crate::arm::cpu_trait::AccessType::Read,
                            fault_type: MemoryFaultType::Alignment,
                            stage2: false,
                        }));
                    }
                    let val1 = self.memory.read_u64(pa)?;
                    let val2 = self.memory.read_u64(pa + 8)?;
                    self.set_x(rt, val1);
                    self.set_x(rt2, val2);
                }
            } else {
                // Single register load (LDXR, LDAXR, LDXRB, LDXRH)
                match elsize {
                    1 => {
                        let val = self.memory.read_u8(pa)?;
                        self.set_w(rt, val as u32);
                    }
                    2 => {
                        let val = self.memory.read_u16(pa)?;
                        self.set_w(rt, val as u32);
                    }
                    4 => {
                        let val = self.memory.read_u32(pa)?;
                        self.set_w(rt, val);
                    }
                    8 => {
                        let val = self.memory.read_u64(pa)?;
                        self.set_x(rt, val);
                    }
                    _ => unreachable!(),
                }
            }

            // Memory barrier for acquire semantics
            if is_ordered {
                // LDAXR has acquire semantics - barrier is implicit
                // In our single-threaded emulator, this is a no-op
            }
        } else {
            // Store exclusive: STXR, STLXR, STXP, STLXP

            // Memory barrier for release semantics
            if is_ordered {
                // STLXR has release semantics - barrier is implicit
                // In our single-threaded emulator, this is a no-op
            }

            // Check if exclusive monitors pass
            let exclusive_held = self.memory.check_exclusive(pa, datasize as u8);

            if exclusive_held {
                // Exclusive access succeeded - perform the store
                if is_pair {
                    if elsize == 4 {
                        // 32-bit pair - atomic 64-bit store
                        let val1 = self.get_w(rt) as u64;
                        let val2 = self.get_w(rt2) as u64;
                        let data = val1 | (val2 << 32);
                        self.memory.write_u64(pa, data)?;
                    } else {
                        // 64-bit pair
                        if pa & 0xF != 0 {
                            return Err(ArmError::MemoryError(MemoryFaultInfo {
                                address,
                                access: crate::arm::cpu_trait::AccessType::Write,
                                fault_type: MemoryFaultType::Alignment,
                                stage2: false,
                            }));
                        }
                        let val1 = self.get_x(rt);
                        let val2 = self.get_x(rt2);
                        self.memory.write_u64(pa, val1)?;
                        self.memory.write_u64(pa + 8, val2)?;
                    }
                } else {
                    // Single register store
                    match elsize {
                        1 => {
                            let val = self.get_w(rt) as u8;
                            self.memory.write_u8(pa, val)?;
                        }
                        2 => {
                            let val = self.get_w(rt) as u16;
                            self.memory.write_u16(pa, val)?;
                        }
                        4 => {
                            let val = self.get_w(rt);
                            self.memory.write_u32(pa, val)?;
                        }
                        8 => {
                            let val = self.get_x(rt);
                            self.memory.write_u64(pa, val)?;
                        }
                        _ => unreachable!(),
                    }
                }

                // Store succeeded - write 0 to status register
                self.set_w(rs, 0);
            } else {
                // Exclusive access failed - write 1 to status register
                self.set_w(rs, 1);
            }
        }

        Ok(CpuExit::Continue)
    }

    fn exec_ldr_literal(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let opc = (insn >> 30) & 0x3;
        let v = (insn >> 26) & 1;
        let imm19 = ((insn >> 5) & 0x7FFFF) as i64;
        let rt = (insn & 0x1F) as u8;

        let offset = ((imm19 << 45) >> 43) as i64;
        let address = ((self.pc as i64).wrapping_sub(4).wrapping_add(offset)) as u64;

        if v != 0 {
            return Err(ArmError::Unimplemented("LDR (literal) SIMD".to_string()));
        }

        match opc {
            0b00 => {
                // LDR (32-bit)
                let value = self.mem_read_u32(address)?;
                self.set_w(rt, value);
            }
            0b01 => {
                // LDR (64-bit)
                let value = self.mem_read_u64(address)?;
                self.set_x(rt, value);
            }
            0b10 => {
                // LDRSW
                let value = self.mem_read_u32(address)? as i32 as i64 as u64;
                self.set_x(rt, value);
            }
            0b11 => {
                // PRFM - prefetch, NOP
                return Ok(CpuExit::Continue);
            }
            _ => unreachable!(),
        }

        Ok(CpuExit::Continue)
    }

    fn exec_ldst_pair(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let opc = (insn >> 30) & 0x3;
        let v = (insn >> 26) & 1;
        let mode = (insn >> 23) & 0x3; // 01=post, 11=pre, 10=signed
        let l = (insn >> 22) & 1; // 0=store, 1=load
        let imm7 = ((insn >> 15) & 0x7F) as i32;
        let rt2 = ((insn >> 10) & 0x1F) as u8;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let rt = (insn & 0x1F) as u8;

        if v != 0 {
            return Err(ArmError::Unimplemented("LDP/STP SIMD".to_string()));
        }

        let (scale, signed) = match opc {
            0b00 => (4, false), // 32-bit
            0b01 => (8, false), // 64-bit (LDPSW when l=1)
            0b10 => (8, false), // 64-bit
            0b11 => return Err(ArmError::UndefinedInstruction(insn)),
            _ => unreachable!(),
        };

        let offset = ((imm7 << 25) >> 25) as i64 * scale;
        let wback = mode == 0b01 || mode == 0b11;
        let postindex = mode == 0b01;

        let base = if rn == 31 {
            self.current_sp()
        } else {
            self.get_x(rn)
        };

        let address = if postindex {
            base
        } else {
            (base as i64).wrapping_add(offset) as u64
        };

        if l != 0 {
            // Load
            if scale == 4 {
                let val1 = self.mem_read_u32(address)?;
                let val2 = self.mem_read_u32(address.wrapping_add(4))?;

                if opc == 0b01 {
                    // LDPSW - sign extend
                    self.set_x(rt, val1 as i32 as i64 as u64);
                    self.set_x(rt2, val2 as i32 as i64 as u64);
                } else {
                    self.set_w(rt, val1);
                    self.set_w(rt2, val2);
                }
            } else {
                let val1 = self.mem_read_u64(address)?;
                let val2 = self.mem_read_u64(address.wrapping_add(8))?;
                self.set_x(rt, val1);
                self.set_x(rt2, val2);
            }
        } else {
            // Store
            if scale == 4 {
                let val1 = self.get_w(rt);
                let val2 = self.get_w(rt2);
                self.mem_write_u32(address, val1)?;
                self.mem_write_u32(address.wrapping_add(4), val2)?;
            } else {
                let val1 = self.get_x(rt);
                let val2 = self.get_x(rt2);
                self.mem_write_u64(address, val1)?;
                self.mem_write_u64(address.wrapping_add(8), val2)?;
            }
        }

        if wback {
            let new_base = if postindex {
                (base as i64).wrapping_add(offset) as u64
            } else {
                (base as i64).wrapping_add(offset) as u64
            };

            if rn == 31 {
                self.set_current_sp(new_base);
            } else {
                self.set_x(rn, new_base);
            }
        }

        Ok(CpuExit::Continue)
    }

    fn exec_ldst_reg(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let size = (insn >> 30) & 0x3;
        let v = (insn >> 26) & 1;
        let opc = (insn >> 22) & 0x3;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let rt = (insn & 0x1F) as u8;

        if v != 0 {
            return Err(ArmError::Unimplemented("LDR/STR SIMD".to_string()));
        }

        // Determine addressing mode
        let (address, wback, wback_value) = self.decode_address(insn, rn)?;

        let is_load = (opc & 1) != 0 || opc == 0b10;
        let is_signed = opc >= 0b10;

        if is_load {
            let value = match size {
                0b00 => {
                    let v = self.mem_read_u8(address)?;
                    if is_signed && opc == 0b11 {
                        v as i8 as i64 as u64
                    } else if is_signed {
                        v as i8 as i32 as u64
                    } else {
                        v as u64
                    }
                }
                0b01 => {
                    let v = self.mem_read_u16(address)?;
                    if is_signed && opc == 0b11 {
                        v as i16 as i64 as u64
                    } else if is_signed {
                        v as i16 as i32 as u64
                    } else {
                        v as u64
                    }
                }
                0b10 => {
                    let v = self.mem_read_u32(address)?;
                    if is_signed {
                        v as i32 as i64 as u64
                    } else {
                        v as u64
                    }
                }
                0b11 => self.mem_read_u64(address)?,
                _ => unreachable!(),
            };

            if size == 0b11 || (is_signed && opc == 0b10) {
                self.set_x(rt, value);
            } else {
                self.set_w(rt, value as u32);
            }
        } else {
            // Store
            match size {
                0b00 => self.mem_write_u8(address, self.get_w(rt) as u8)?,
                0b01 => self.mem_write_u16(address, self.get_w(rt) as u16)?,
                0b10 => self.mem_write_u32(address, self.get_w(rt))?,
                0b11 => self.mem_write_u64(address, self.get_x(rt))?,
                _ => unreachable!(),
            }
        }

        // Writeback
        if wback {
            if rn == 31 {
                self.set_current_sp(wback_value);
            } else {
                self.set_x(rn, wback_value);
            }
        }

        Ok(CpuExit::Continue)
    }

    /// Decode addressing mode for load/store.
    fn decode_address(&self, insn: u32, rn: u8) -> Result<(u64, bool, u64), ArmError> {
        let base = if rn == 31 {
            self.current_sp()
        } else {
            self.get_x(rn)
        };

        // Check for unsigned offset (bit 24 = 1, bit 21 = 0)
        if (insn >> 24) & 1 != 0 && (insn >> 21) & 1 == 0 {
            // Unsigned offset
            let size = (insn >> 30) & 0x3;
            let imm12 = ((insn >> 10) & 0xFFF) as u64;
            let offset = imm12 << size;
            return Ok((base.wrapping_add(offset), false, 0));
        }

        // Check addressing mode
        let op4 = (insn >> 10) & 0x3;

        match op4 {
            0b00 => {
                // Unscaled immediate
                let imm9 = ((insn >> 12) & 0x1FF) as i32;
                let offset = ((imm9 << 23) >> 23) as i64;
                Ok(((base as i64).wrapping_add(offset) as u64, false, 0))
            }
            0b01 => {
                // Immediate post-indexed
                let imm9 = ((insn >> 12) & 0x1FF) as i32;
                let offset = ((imm9 << 23) >> 23) as i64;
                Ok((base, true, (base as i64).wrapping_add(offset) as u64))
            }
            0b10 => {
                // Register offset
                let rm = ((insn >> 16) & 0x1F) as u8;
                let option = ((insn >> 13) & 0x7) as u8;
                let s = ((insn >> 12) & 1) != 0;
                let size = (insn >> 30) & 0x3;

                let offset = self.extend_reg(rm, option, if s { size } else { 0 })?;
                Ok((base.wrapping_add(offset), false, 0))
            }
            0b11 => {
                // Immediate pre-indexed
                let imm9 = ((insn >> 12) & 0x1FF) as i32;
                let offset = ((imm9 << 23) >> 23) as i64;
                let addr = (base as i64).wrapping_add(offset) as u64;
                Ok((addr, true, addr))
            }
            _ => unreachable!(),
        }
    }

    /// Extend register with optional shift.
    fn extend_reg(&self, rm: u8, option: u8, shift: u32) -> Result<u64, ArmError> {
        let val = self.get_x(rm);

        let extended = match option {
            0b000 => (val as u8) as u64,                // UXTB
            0b001 => (val as u16) as u64,               // UXTH
            0b010 => (val as u32) as u64,               // UXTW
            0b011 => val,                               // UXTX
            0b100 => (val as u8 as i8 as i64) as u64,   // SXTB
            0b101 => (val as u16 as i16 as i64) as u64, // SXTH
            0b110 => (val as u32 as i32 as i64) as u64, // SXTW
            0b111 => val,                               // SXTX
            _ => return Err(ArmError::UndefinedInstruction(0)),
        };

        Ok(extended << shift)
    }

    // Data processing (register) implementations
    fn exec_logical_shifted(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let sf = (insn >> 31) & 1;
        let opc = (insn >> 29) & 0x3;
        let shift = ((insn >> 22) & 0x3) as u32;
        let n = (insn >> 21) & 1;
        let rm = ((insn >> 16) & 0x1F) as u8;
        let imm6 = ((insn >> 10) & 0x3F) as u32;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let rd = (insn & 0x1F) as u8;

        let datasize = if sf != 0 { 64 } else { 32 };

        let operand1 = if sf != 0 {
            self.get_x(rn)
        } else {
            self.get_w(rn) as u64
        };

        let mut operand2 = if sf != 0 {
            self.get_x(rm)
        } else {
            self.get_w(rm) as u64
        };

        // Apply shift
        operand2 = match shift {
            0b00 => operand2 << imm6,                   // LSL
            0b01 => operand2 >> imm6,                   // LSR
            0b10 => ((operand2 as i64) >> imm6) as u64, // ASR
            0b11 => operand2.rotate_right(imm6),        // ROR
            _ => unreachable!(),
        };

        if sf == 0 {
            operand2 &= 0xFFFF_FFFF;
        }

        // Invert if N bit set
        if n != 0 {
            operand2 = !operand2;
            if sf == 0 {
                operand2 &= 0xFFFF_FFFF;
            }
        }

        let result = match opc {
            0b00 => operand1 & operand2, // AND / BIC
            0b01 => operand1 | operand2, // ORR / ORN
            0b10 => operand1 ^ operand2, // EOR / EON
            0b11 => operand1 & operand2, // ANDS / BICS
            _ => unreachable!(),
        };

        if opc == 0b11 {
            if sf != 0 {
                self.update_nz_64(result);
            } else {
                self.update_nz_32(result as u32);
            }
            self.set_c(false);
            self.set_v(false);
        }

        if sf != 0 {
            self.set_x(rd, result);
        } else {
            self.set_w(rd, result as u32);
        }

        Ok(CpuExit::Continue)
    }

    fn exec_add_sub_shifted_ext(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let sf = (insn >> 31) & 1;
        let op = (insn >> 30) & 1;
        let s = (insn >> 29) & 1;
        let extended = (insn >> 21) & 1; // bit 21 distinguishes shifted (0) from extended (1)
        let rm = ((insn >> 16) & 0x1F) as u8;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let rd = (insn & 0x1F) as u8;

        if extended == 0 {
            // Shifted register
            let shift = ((insn >> 22) & 0x3) as u32;
            let imm6 = ((insn >> 10) & 0x3F) as u32;

            let operand1 = if sf != 0 {
                self.get_x(rn)
            } else {
                self.get_w(rn) as u64
            };

            let mut operand2 = if sf != 0 {
                self.get_x(rm)
            } else {
                self.get_w(rm) as u64
            };

            operand2 = match shift {
                0b00 => operand2 << imm6,
                0b01 => operand2 >> imm6,
                0b10 => ((operand2 as i64) >> imm6) as u64,
                _ => return Err(ArmError::UndefinedInstruction(insn)),
            };

            if sf == 0 {
                operand2 &= 0xFFFF_FFFF;
            }

            let (result, carry, overflow) = if op == 0 {
                // ADD
                if sf != 0 {
                    let (r, c) = operand1.overflowing_add(operand2);
                    let v = (!(operand1 ^ operand2) & (operand1 ^ r)) >> 63 != 0;
                    (r, c, v)
                } else {
                    let o1 = operand1 as u32;
                    let o2 = operand2 as u32;
                    let (r, c) = o1.overflowing_add(o2);
                    let v = (!(o1 ^ o2) & (o1 ^ r)) >> 31 != 0;
                    (r as u64, c, v)
                }
            } else {
                // SUB
                if sf != 0 {
                    let (r, c) = operand1.overflowing_sub(operand2);
                    let v = ((operand1 ^ operand2) & (operand1 ^ r)) >> 63 != 0;
                    (r, !c, v)
                } else {
                    let o1 = operand1 as u32;
                    let o2 = operand2 as u32;
                    let (r, c) = o1.overflowing_sub(o2);
                    let v = ((o1 ^ o2) & (o1 ^ r)) >> 31 != 0;
                    (r as u64, !c, v)
                }
            };

            if s != 0 {
                if sf != 0 {
                    self.update_nz_64(result);
                } else {
                    self.update_nz_32(result as u32);
                }
                self.set_c(carry);
                self.set_v(overflow);
            }

            if rd == 31 && s == 0 {
                self.set_current_sp(result);
            } else if sf != 0 {
                self.set_x(rd, result);
            } else {
                self.set_w(rd, result as u32);
            }
        } else {
            // Extended register
            let option = ((insn >> 13) & 0x7) as u8;
            let imm3 = ((insn >> 10) & 0x7) as u32;

            let operand1 = if rn == 31 {
                self.current_sp()
            } else {
                self.get_x(rn)
            };

            let operand2 = self.extend_reg(rm, option, imm3)?;

            let (result, carry, overflow) = if op == 0 {
                let (r, c) = operand1.overflowing_add(operand2);
                let v = (!(operand1 ^ operand2) & (operand1 ^ r)) >> 63 != 0;
                (r, c, v)
            } else {
                let (r, c) = operand1.overflowing_sub(operand2);
                let v = ((operand1 ^ operand2) & (operand1 ^ r)) >> 63 != 0;
                (r, !c, v)
            };

            if s != 0 {
                if sf != 0 {
                    self.update_nz_64(result);
                } else {
                    self.update_nz_32(result as u32);
                }
                self.set_c(carry);
                self.set_v(overflow);
            }

            if rd == 31 && s == 0 {
                self.set_current_sp(result);
            } else if sf != 0 {
                self.set_x(rd, result);
            } else {
                self.set_w(rd, result as u32);
            }
        }

        Ok(CpuExit::Continue)
    }

    fn exec_adc_sbc(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let sf = (insn >> 31) & 1;
        let op = (insn >> 30) & 1;
        let s = (insn >> 29) & 1;
        let rm = ((insn >> 16) & 0x1F) as u8;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let rd = (insn & 0x1F) as u8;

        let c_in = if self.get_c() { 1u64 } else { 0 };

        if sf != 0 {
            let operand1 = self.get_x(rn);
            let operand2 = self.get_x(rm);

            let (result, carry, overflow) = if op == 0 {
                // ADC
                let (r1, c1) = operand1.overflowing_add(operand2);
                let (r, c2) = r1.overflowing_add(c_in);
                let v = (!(operand1 ^ operand2) & (operand1 ^ r)) >> 63 != 0;
                (r, c1 || c2, v)
            } else {
                // SBC
                let not_c = if self.get_c() { 0u64 } else { 1 };
                let (r1, c1) = operand1.overflowing_sub(operand2);
                let (r, c2) = r1.overflowing_sub(not_c);
                let v = ((operand1 ^ operand2) & (operand1 ^ r)) >> 63 != 0;
                (r, !(c1 || c2), v)
            };

            if s != 0 {
                self.update_nz_64(result);
                self.set_c(carry);
                self.set_v(overflow);
            }

            self.set_x(rd, result);
        } else {
            let operand1 = self.get_w(rn);
            let operand2 = self.get_w(rm);
            let c_in = c_in as u32;

            let (result, carry, overflow) = if op == 0 {
                let (r1, c1) = operand1.overflowing_add(operand2);
                let (r, c2) = r1.overflowing_add(c_in);
                let v = (!(operand1 ^ operand2) & (operand1 ^ r)) >> 31 != 0;
                (r, c1 || c2, v)
            } else {
                let not_c = if self.get_c() { 0u32 } else { 1 };
                let (r1, c1) = operand1.overflowing_sub(operand2);
                let (r, c2) = r1.overflowing_sub(not_c);
                let v = ((operand1 ^ operand2) & (operand1 ^ r)) >> 31 != 0;
                (r, !(c1 || c2), v)
            };

            if s != 0 {
                self.update_nz_32(result);
                self.set_c(carry);
                self.set_v(overflow);
            }

            self.set_w(rd, result);
        }

        Ok(CpuExit::Continue)
    }

    fn exec_ccmp_ccmn(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let sf = (insn >> 31) & 1;
        let op = (insn >> 30) & 1; // 0=CCMN, 1=CCMP
        let imm_or_reg = (insn >> 11) & 1;
        let rm_imm5 = ((insn >> 16) & 0x1F) as u8;
        let cond = ((insn >> 12) & 0xF) as u8;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let nzcv = (insn & 0xF) as u8;

        if self.condition_holds(cond) {
            let operand2 = if imm_or_reg != 0 {
                rm_imm5 as u64
            } else {
                if sf != 0 {
                    self.get_x(rm_imm5)
                } else {
                    self.get_w(rm_imm5) as u64
                }
            };

            if sf != 0 {
                let operand1 = self.get_x(rn);
                let (result, carry, overflow) = if op == 0 {
                    // CCMN (add)
                    let (r, c) = operand1.overflowing_add(operand2);
                    let v = (!(operand1 ^ operand2) & (operand1 ^ r)) >> 63 != 0;
                    (r, c, v)
                } else {
                    // CCMP (sub)
                    let (r, c) = operand1.overflowing_sub(operand2);
                    let v = ((operand1 ^ operand2) & (operand1 ^ r)) >> 63 != 0;
                    (r, !c, v)
                };
                self.update_nz_64(result);
                self.set_c(carry);
                self.set_v(overflow);
            } else {
                let operand1 = self.get_w(rn);
                let operand2 = operand2 as u32;
                let (result, carry, overflow) = if op == 0 {
                    let (r, c) = operand1.overflowing_add(operand2);
                    let v = (!(operand1 ^ operand2) & (operand1 ^ r)) >> 31 != 0;
                    (r, c, v)
                } else {
                    let (r, c) = operand1.overflowing_sub(operand2);
                    let v = ((operand1 ^ operand2) & (operand1 ^ r)) >> 31 != 0;
                    (r, !c, v)
                };
                self.update_nz_32(result);
                self.set_c(carry);
                self.set_v(overflow);
            }
        } else {
            self.nzcv = nzcv;
        }

        Ok(CpuExit::Continue)
    }

    fn exec_csel(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let sf = (insn >> 31) & 1;
        let op = (insn >> 30) & 1;
        let rm = ((insn >> 16) & 0x1F) as u8;
        let cond = ((insn >> 12) & 0xF) as u8;
        let op2 = (insn >> 10) & 0x3;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let rd = (insn & 0x1F) as u8;

        let cond_met = self.condition_holds(cond);

        if sf != 0 {
            let operand1 = self.get_x(rn);
            let operand2 = self.get_x(rm);

            let result = if cond_met {
                operand1
            } else {
                match (op, op2) {
                    (0, 0) => operand2,                 // CSEL
                    (0, 1) => operand2.wrapping_add(1), // CSINC
                    (1, 0) => !operand2,                // CSINV
                    (1, 1) => operand2.wrapping_neg(),  // CSNEG
                    _ => unreachable!(),
                }
            };

            self.set_x(rd, result);
        } else {
            let operand1 = self.get_w(rn);
            let operand2 = self.get_w(rm);

            let result = if cond_met {
                operand1
            } else {
                match (op, op2) {
                    (0, 0) => operand2,
                    (0, 1) => operand2.wrapping_add(1),
                    (1, 0) => !operand2,
                    (1, 1) => operand2.wrapping_neg(),
                    _ => unreachable!(),
                }
            };

            self.set_w(rd, result);
        }

        Ok(CpuExit::Continue)
    }

    fn exec_dp_2src(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let sf = (insn >> 31) & 1;
        let s = (insn >> 29) & 1;
        let rm = ((insn >> 16) & 0x1F) as u8;
        let opcode = (insn >> 10) & 0x3F;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let rd = (insn & 0x1F) as u8;

        if s != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }

        if sf != 0 {
            let operand1 = self.get_x(rn);
            let operand2 = self.get_x(rm);

            let result = match opcode {
                0b000010 => {
                    // UDIV
                    if operand2 == 0 {
                        0
                    } else {
                        operand1 / operand2
                    }
                }
                0b000011 => {
                    // SDIV
                    if operand2 == 0 {
                        0
                    } else {
                        (operand1 as i64).wrapping_div(operand2 as i64) as u64
                    }
                }
                0b001000 => {
                    // LSLV
                    let shift = (operand2 & 0x3F) as u32;
                    operand1 << shift
                }
                0b001001 => {
                    // LSRV
                    let shift = (operand2 & 0x3F) as u32;
                    operand1 >> shift
                }
                0b001010 => {
                    // ASRV
                    let shift = (operand2 & 0x3F) as u32;
                    ((operand1 as i64) >> shift) as u64
                }
                0b001011 => {
                    // RORV
                    let shift = (operand2 & 0x3F) as u32;
                    operand1.rotate_right(shift)
                }
                0b010000 => {
                    // CRC32B
                    crc32(operand1, operand2 as u8 as u64, 8)
                }
                0b010001 => {
                    // CRC32H
                    crc32(operand1, operand2 as u16 as u64, 16)
                }
                0b010010 => {
                    // CRC32W
                    crc32(operand1, operand2 as u32 as u64, 32)
                }
                0b010011 => {
                    // CRC32X
                    crc32(operand1, operand2, 64)
                }
                0b010100 => {
                    // CRC32CB
                    crc32c(operand1, operand2 as u8 as u64, 8)
                }
                0b010101 => {
                    // CRC32CH
                    crc32c(operand1, operand2 as u16 as u64, 16)
                }
                0b010110 => {
                    // CRC32CW
                    crc32c(operand1, operand2 as u32 as u64, 32)
                }
                0b010111 => {
                    // CRC32CX
                    crc32c(operand1, operand2, 64)
                }
                _ => return Err(ArmError::UndefinedInstruction(insn)),
            };

            self.set_x(rd, result);
        } else {
            let operand1 = self.get_w(rn);
            let operand2 = self.get_w(rm);

            let result = match opcode {
                0b000010 => {
                    // UDIV
                    if operand2 == 0 {
                        0
                    } else {
                        operand1 / operand2
                    }
                }
                0b000011 => {
                    // SDIV
                    if operand2 == 0 {
                        0
                    } else {
                        (operand1 as i32).wrapping_div(operand2 as i32) as u32
                    }
                }
                0b001000 => {
                    // LSLV
                    let shift = (operand2 & 0x1F) as u32;
                    operand1 << shift
                }
                0b001001 => {
                    // LSRV
                    let shift = (operand2 & 0x1F) as u32;
                    operand1 >> shift
                }
                0b001010 => {
                    // ASRV
                    let shift = (operand2 & 0x1F) as u32;
                    ((operand1 as i32) >> shift) as u32
                }
                0b001011 => {
                    // RORV
                    let shift = (operand2 & 0x1F) as u32;
                    operand1.rotate_right(shift)
                }
                0b010000 => {
                    // CRC32B
                    crc32(operand1 as u64, operand2 as u8 as u64, 8) as u32
                }
                0b010001 => {
                    // CRC32H
                    crc32(operand1 as u64, operand2 as u16 as u64, 16) as u32
                }
                0b010010 => {
                    // CRC32W
                    crc32(operand1 as u64, operand2 as u64, 32) as u32
                }
                0b010100 => {
                    // CRC32CB
                    crc32c(operand1 as u64, operand2 as u8 as u64, 8) as u32
                }
                0b010101 => {
                    // CRC32CH
                    crc32c(operand1 as u64, operand2 as u16 as u64, 16) as u32
                }
                0b010110 => {
                    // CRC32CW
                    crc32c(operand1 as u64, operand2 as u64, 32) as u32
                }
                _ => return Err(ArmError::UndefinedInstruction(insn)),
            };

            self.set_w(rd, result);
        }

        Ok(CpuExit::Continue)
    }

    fn exec_dp_3src(&mut self, insn: u32) -> Result<CpuExit, ArmError> {
        let sf = (insn >> 31) & 1;
        let op54 = (insn >> 29) & 0x3;
        let op31 = (insn >> 21) & 0x7;
        let rm = ((insn >> 16) & 0x1F) as u8;
        let o0 = (insn >> 15) & 1;
        let ra = ((insn >> 10) & 0x1F) as u8;
        let rn = ((insn >> 5) & 0x1F) as u8;
        let rd = (insn & 0x1F) as u8;

        if op54 != 0 {
            return Err(ArmError::UndefinedInstruction(insn));
        }

        if sf != 0 {
            // 64-bit
            let operand1 = self.get_x(rn);
            let operand2 = self.get_x(rm);
            let addend = self.get_x(ra);

            let result = match (op31, o0) {
                (0b000, 0) => {
                    // MADD
                    addend.wrapping_add(operand1.wrapping_mul(operand2))
                }
                (0b000, 1) => {
                    // MSUB
                    addend.wrapping_sub(operand1.wrapping_mul(operand2))
                }
                (0b001, 0) => {
                    // SMADDL
                    let p = (operand1 as i32 as i64).wrapping_mul(operand2 as i32 as i64);
                    (addend as i64).wrapping_add(p) as u64
                }
                (0b001, 1) => {
                    // SMSUBL
                    let p = (operand1 as i32 as i64).wrapping_mul(operand2 as i32 as i64);
                    (addend as i64).wrapping_sub(p) as u64
                }
                (0b010, 0) => {
                    // SMULH
                    let a = operand1 as i64 as i128;
                    let b = operand2 as i64 as i128;
                    ((a * b) >> 64) as u64
                }
                (0b101, 0) => {
                    // UMADDL
                    let p = (operand1 as u32 as u64).wrapping_mul(operand2 as u32 as u64);
                    addend.wrapping_add(p)
                }
                (0b101, 1) => {
                    // UMSUBL
                    let p = (operand1 as u32 as u64).wrapping_mul(operand2 as u32 as u64);
                    addend.wrapping_sub(p)
                }
                (0b110, 0) => {
                    // UMULH
                    let a = operand1 as u128;
                    let b = operand2 as u128;
                    ((a * b) >> 64) as u64
                }
                _ => return Err(ArmError::UndefinedInstruction(insn)),
            };

            self.set_x(rd, result);
        } else {
            // 32-bit
            let operand1 = self.get_w(rn);
            let operand2 = self.get_w(rm);
            let addend = self.get_w(ra);

            let result = match (op31, o0) {
                (0b000, 0) => {
                    // MADD
                    addend.wrapping_add(operand1.wrapping_mul(operand2))
                }
                (0b000, 1) => {
                    // MSUB
                    addend.wrapping_sub(operand1.wrapping_mul(operand2))
                }
                _ => return Err(ArmError::UndefinedInstruction(insn)),
            };

            self.set_w(rd, result);
        }

        Ok(CpuExit::Continue)
    }
}

// =============================================================================
// ArmCpu Trait Implementation
// =============================================================================

impl ArmCpu for AArch64Cpu {
    fn step(&mut self) -> Result<CpuExit, ArmError> {
        if self.halted {
            return Ok(CpuExit::Halt);
        }

        // Check for WFI/WFE completion
        if self.wfi {
            if let Some(_) = self.check_pending_interrupts()? {
                self.wfi = false;
            } else {
                return Ok(CpuExit::Wfi);
            }
        }

        if self.wfe {
            if self.event_register {
                self.event_register = false;
                self.wfe = false;
            } else {
                return Ok(CpuExit::Wfe);
            }
        }

        // Check for pending interrupts
        if let Some(exit) = self.check_pending_interrupts()? {
            return Ok(exit);
        }

        // Execute one instruction
        self.execute_instruction()
    }

    fn reset(&mut self) {
        // Reset all registers
        self.x = [0; NUM_GPRS];
        self.sp_el = [0; NUM_ELS];
        self.pc = 0;

        self.nzcv = 0;
        self.daif = 0xF; // All exceptions masked
        self.current_el = self.config.initial_el;
        self.sp_sel = true;
        self.pan = false;
        self.uao = false;
        self.dit = false;
        self.ssbs = false;
        self.tco = false;
        self.btype = 0;
        self.il = false;
        self.ss = false;

        self.v = [0; NUM_SIMD_REGS];
        self.fpcr = 0;
        self.fpsr = 0;

        self.sysregs.reset();
        self.mmu = Mmu::new();
        if let Some(ref mut gic) = self.gic {
            gic.reset();
        }

        self.insn_count = 0;
        self.cycle_count = 0;
        self.halted = false;
        self.wfi = false;
        self.wfe = false;
        self.event_register = false;
        self.pending_exceptions.clear();
        self.breakpoints.clear();
        self.watchpoints.clear();
    }

    fn get_gpr(&self, reg: u8) -> u64 {
        self.get_x(reg)
    }

    fn set_gpr(&mut self, reg: u8, value: u64) {
        self.set_x(reg, value);
    }

    fn get_pc(&self) -> u64 {
        self.pc
    }

    fn set_pc(&mut self, value: u64) {
        self.pc = value;
    }

    fn get_sp(&self) -> u64 {
        self.current_sp()
    }

    fn set_sp(&mut self, value: u64) {
        self.set_current_sp(value);
    }

    fn get_lr(&self) -> u64 {
        self.get_x(30) // X30 is the link register in AArch64
    }

    fn set_lr(&mut self, value: u64) {
        self.set_x(30, value);
    }

    fn get_pstate(&self) -> ProcessorState {
        ProcessorState {
            n: self.get_n(),
            z: self.get_z(),
            c: self.get_c(),
            v: self.get_v(),
            q: false,
            ge: 0,
            el: self.current_el,
            sp_sel: self.sp_sel,
            t: false, // Not applicable to AArch64
            i: (self.daif & 0x2) != 0,
            f: (self.daif & 0x1) != 0,
            a: (self.daif & 0x4) != 0,
            d: (self.daif & 0x8) != 0,
            e: false, // Little endian
            it_state: 0,
            mode: 0,
        }
    }

    fn set_pstate(&mut self, state: ProcessorState) {
        self.set_nzcv(state.n, state.z, state.c, state.v);
        self.current_el = state.el;
        self.sp_sel = state.sp_sel;
        self.daif = ((state.d as u8) << 3)
            | ((state.a as u8) << 2)
            | ((state.i as u8) << 1)
            | (state.f as u8);
    }

    fn is_privileged(&self) -> bool {
        self.current_el > 0
    }

    fn is_secure(&self) -> bool {
        // Check SCR_EL3.NS bit
        (self.sysregs.scr_el3 & 1) == 0
    }

    fn current_el(&self) -> u8 {
        self.current_el
    }

    fn read_memory(&self, addr: u64, size: usize) -> Result<Vec<u8>, ArmError> {
        let mut data = vec![0u8; size];
        for i in 0..size {
            data[i] = self.mem_read_u8(addr + i as u64)?;
        }
        Ok(data)
    }

    fn write_memory(&mut self, addr: u64, data: &[u8]) -> Result<(), ArmError> {
        for (i, &byte) in data.iter().enumerate() {
            self.mem_write_u8(addr + i as u64, byte)?;
        }
        Ok(())
    }

    fn arch_version(&self) -> ArmVersion {
        self.config.version
    }

    fn profile(&self) -> ArmProfile {
        ArmProfile::A
    }

    fn features(&self) -> ArmFeatures {
        self.config.features
    }

    fn pending_exceptions(&self) -> Vec<ArmException> {
        self.pending_exceptions.clone()
    }

    fn inject_exception(&mut self, exception: ArmException) -> Result<(), ArmError> {
        self.pending_exceptions.push(exception);
        Ok(())
    }

    fn set_breakpoint(&mut self, addr: u64) -> Result<(), ArmError> {
        self.breakpoints.insert(addr);
        Ok(())
    }

    fn clear_breakpoint(&mut self, addr: u64) -> Result<(), ArmError> {
        self.breakpoints.remove(&addr);
        Ok(())
    }

    fn set_watchpoint(
        &mut self,
        addr: u64,
        size: usize,
        kind: WatchpointKind,
    ) -> Result<(), ArmError> {
        // Check if watchpoint already exists
        if !self
            .watchpoints
            .iter()
            .any(|(a, s, k)| *a == addr && *s == size && *k == kind)
        {
            self.watchpoints.push((addr, size, kind));
        }
        Ok(())
    }

    fn clear_watchpoint(&mut self, addr: u64) -> Result<(), ArmError> {
        self.watchpoints.retain(|(a, _, _)| *a != addr);
        Ok(())
    }

    fn instruction_count(&self) -> u64 {
        self.insn_count
    }

    fn cycle_count(&self) -> Option<u64> {
        Some(self.cycle_count)
    }

    fn has_fpu(&self) -> bool {
        true // AArch64 always has FP
    }

    fn get_simd_reg(&self, reg: u8) -> Option<(u64, u64)> {
        if reg < 32 {
            let val = self.v[reg as usize];
            Some((val as u64, (val >> 64) as u64))
        } else {
            None
        }
    }

    fn set_simd_reg(&mut self, reg: u8, low: u64, high: u64) -> Result<(), ArmError> {
        if reg < 32 {
            self.v[reg as usize] = (high as u128) << 64 | (low as u128);
            Ok(())
        } else {
            Err(ArmError::InvalidRegister(reg))
        }
    }

    fn get_fpcr(&self) -> Option<u32> {
        Some(self.fpcr)
    }

    fn set_fpcr(&mut self, value: u32) -> Result<(), ArmError> {
        self.fpcr = value;
        Ok(())
    }

    fn get_fpsr(&self) -> Option<u32> {
        Some(self.fpsr)
    }

    fn set_fpsr(&mut self, value: u32) -> Result<(), ArmError> {
        self.fpsr = value;
        Ok(())
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Decode bitmask immediate for logical instructions.
fn decode_bitmask(n: bool, imms: u32, immr: u32, is_64bit: bool) -> Result<u64, ArmError> {
    // For 64-bit (sf=1): N must be 1
    // For 32-bit (sf=0): N must be 0, and highest set bit in ~imms[5:0] determines element size
    let len = if n {
        6 // 64-bit elements
    } else {
        // Find highest set bit position in ~imms[5:0] (6-bit value)
        let not_imms = !imms & 0x3F;
        if not_imms == 0 {
            return Err(ArmError::UndefinedInstruction(0));
        }
        // Calculate: 5 - leading_zeros_in_6_bits
        // For a 6-bit value, we need to find the position of highest set bit (0-5)
        // leading_zeros for u32 counts from bit 31, so we adjust:
        // Position in 6-bit = 5 - (leading_zeros_of_u32 - 26)
        //                   = 5 - leading_zeros + 26 = 31 - leading_zeros
        // But we want len = position + 1
        let pos = 31 - not_imms.leading_zeros();
        if pos > 5 {
            return Err(ArmError::UndefinedInstruction(0));
        }
        pos + 1
    };

    if len < 1 || len > 6 {
        return Err(ArmError::UndefinedInstruction(0));
    }

    let levels = (1u32 << len) - 1;
    let s = imms & levels;
    let r = immr & levels;
    let esize = 1u64 << len;

    if s == levels {
        return Err(ArmError::UndefinedInstruction(0));
    }

    // Create the pattern - a run of (s+1) ones
    let welem = if s + 1 >= 64 {
        u64::MAX
    } else {
        (1u64 << (s + 1)) - 1
    };

    // Create mask for element size
    let esize_mask = if esize >= 64 {
        u64::MAX
    } else {
        (1u64 << esize) - 1
    };

    // Rotate right by r
    let rotated = if r == 0 {
        welem
    } else {
        ((welem >> r) | (welem << (esize as u32 - r))) & esize_mask
    };

    // Replicate to fill the register
    let mut result = 0u64;
    let replications = 64 / esize;
    for i in 0..replications {
        result |= rotated << (i * esize);
    }

    if !is_64bit {
        result &= 0xFFFF_FFFF;
    }

    Ok(result)
}

/// Decode bitmasks for bitfield instructions.
fn decode_bitmasks(
    n: bool,
    imms: u32,
    immr: u32,
    _immediate: bool,
    datasize: u32,
) -> Result<(u64, u64), ArmError> {
    // len = HighestSetBit(immN:NOT(imms<5:0>))
    // For N=1: the 7-bit value is 1:xxxxxx, so highest bit is at position 6 -> len=6
    // For N=0: the 7-bit value is 0:NOT(imms), we find highest bit of NOT(imms)
    let len = if n {
        6
    } else {
        let not_imms = !imms & 0x3F;
        if not_imms == 0 {
            // All bits of imms are 1, which is reserved
            return Err(ArmError::UndefinedInstruction(0));
        }
        // Find position of highest set bit in not_imms (0-5)
        // leading_zeros for u32 counts from bit 31, so position = 31 - leading_zeros
        // But not_imms is only 6 bits, so we need: 5 - (not_imms as u8).leading_zeros() after masking
        // Actually simpler: 31 - not_imms.leading_zeros() gives us the position in the u32
        let pos = 31 - not_imms.leading_zeros();
        if pos > 5 {
            return Err(ArmError::UndefinedInstruction(0));
        }
        pos // len = position of highest set bit (not pos + 1!)
    };

    if len < 1 || len > 6 || (1 << len) > datasize {
        return Err(ArmError::UndefinedInstruction(0));
    }

    let levels = (1u32 << len) - 1;
    let s = imms & levels;
    let r = immr & levels;
    let diff = ((s as i32).wrapping_sub(r as i32)) as u32;
    let esize = 1u64 << len;

    // Create element masks, handling potential overflow
    let welem = if s + 1 >= 64 {
        u64::MAX
    } else {
        (1u64 << (s + 1)) - 1
    };

    let telem_bits = (diff & levels) + 1;
    let telem = if telem_bits >= 64 {
        u64::MAX
    } else {
        (1u64 << telem_bits) - 1
    };

    let esize_mask = if esize >= 64 {
        u64::MAX
    } else {
        (1u64 << esize) - 1
    };

    // Rotate welem right by R within element size
    let wmask_elem = if r == 0 {
        welem
    } else {
        ((welem >> r) | (welem << (esize as u32 - r))) & esize_mask
    };

    // Replicate
    let mut wmask = 0u64;
    let mut tmask = 0u64;
    let replications = 64 / esize;
    for i in 0..replications {
        wmask |= wmask_elem << (i * esize);
        tmask |= (telem & esize_mask) << (i * esize);
    }

    if datasize == 32 {
        wmask &= 0xFFFF_FFFF;
        tmask &= 0xFFFF_FFFF;
    }

    Ok((wmask, tmask))
}

/// CRC32 calculation (ISO 3309 polynomial).
fn crc32(crc: u64, data: u64, size: u32) -> u64 {
    const POLY: u32 = 0xEDB8_8320;
    let mut crc = crc as u32;
    let bytes = size / 8;

    for i in 0..bytes {
        let byte = ((data >> (i * 8)) & 0xFF) as u8;
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ POLY
            } else {
                crc >> 1
            };
        }
    }

    crc as u64
}

/// CRC32C calculation (Castagnoli polynomial).
fn crc32c(crc: u64, data: u64, size: u32) -> u64 {
    const POLY: u32 = 0x82F6_3B78;
    let mut crc = crc as u32;
    let bytes = size / 8;

    for i in 0..bytes {
        let byte = ((data >> (i * 8)) & 0xFF) as u8;
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ POLY
            } else {
                crc >> 1
            };
        }
    }

    crc as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arm::memory::FlatMemory;

    fn create_test_cpu() -> AArch64Cpu {
        let memory = FlatMemory::new(0, 0x1000_0000);
        AArch64Cpu::new(AArch64Config::default(), Box::new(memory))
    }

    #[test]
    fn test_cpu_creation() {
        let cpu = create_test_cpu();
        assert_eq!(cpu.get_pc(), 0);
        assert_eq!(cpu.current_el(), 1);
    }

    #[test]
    fn test_register_access() {
        let mut cpu = create_test_cpu();

        cpu.set_x(0, 0x1234_5678_9ABC_DEF0);
        assert_eq!(cpu.get_x(0), 0x1234_5678_9ABC_DEF0);

        cpu.set_w(1, 0xDEAD_BEEF);
        assert_eq!(cpu.get_w(1), 0xDEAD_BEEF);
        assert_eq!(cpu.get_x(1), 0xDEAD_BEEF); // Zero-extended

        // XZR always reads 0
        assert_eq!(cpu.get_x(31), 0);
        cpu.set_x(31, 0xFFFF); // Write to XZR is ignored
        assert_eq!(cpu.get_x(31), 0);
    }

    #[test]
    fn test_condition_flags() {
        let mut cpu = create_test_cpu();

        cpu.set_nzcv(true, false, true, false);
        assert!(cpu.get_n());
        assert!(!cpu.get_z());
        assert!(cpu.get_c());
        assert!(!cpu.get_v());

        cpu.update_nz_64(0);
        assert!(!cpu.get_n());
        assert!(cpu.get_z());

        cpu.update_nz_64(0x8000_0000_0000_0000);
        assert!(cpu.get_n());
        assert!(!cpu.get_z());
    }

    #[test]
    fn test_condition_evaluation() {
        let mut cpu = create_test_cpu();

        // Test EQ (Z=1)
        cpu.set_z(true);
        assert!(cpu.condition_holds(0b0000)); // EQ
        assert!(!cpu.condition_holds(0b0001)); // NE

        // Test CS (C=1)
        cpu.set_c(true);
        assert!(cpu.condition_holds(0b0010)); // CS
        assert!(!cpu.condition_holds(0b0011)); // CC

        // Test AL (always)
        assert!(cpu.condition_holds(0b1110)); // AL
    }

    #[test]
    fn test_stack_pointer() {
        let mut cpu = create_test_cpu();

        cpu.set_current_sp(0x8000_0000);
        assert_eq!(cpu.current_sp(), 0x8000_0000);
    }

    #[test]
    fn test_bitmask_decode() {
        // Test 64-bit mode with N=1 (64-bit elements)
        // imms=0 means a single 1 bit, immr=0 means no rotation
        let mask = decode_bitmask(true, 0, 0, true).unwrap();
        assert_eq!(mask, 0x0000_0000_0000_0001);

        // imms=62 means 63 ones (all except MSB), immr=0
        let mask = decode_bitmask(true, 62, 0, true).unwrap();
        assert_eq!(mask, 0x7FFF_FFFF_FFFF_FFFF);

        // Test N=0 (smaller element sizes)
        // ~imms[5:0] = 0x20 = 0b100000, highest bit at position 5, so len=6 (invalid for N=0)
        // Let's use imms=0b011111, so ~imms[5:0]=0b100000, but that's still len=6

        // For smaller elements, use imms where the high bits indicate element size
        // imms=0b111100, ~imms[5:0]=0b000011, highest bit at position 1, len=2 (4-bit elements)
        // s = imms & 0b11 = 0b00 = 0, so element = 0b1 (single 1)
        let mask = decode_bitmask(false, 0b111100, 0, true).unwrap();
        // 4-bit element replicated: 0x1111111111111111
        assert_eq!(mask, 0x1111_1111_1111_1111);

        // 32-bit mode should mask result
        let mask = decode_bitmask(false, 0b111100, 0, false).unwrap();
        assert_eq!(mask, 0x0000_0000_1111_1111);
    }

    #[test]
    fn test_crc32() {
        // Test basic CRC32 functionality
        let crc = crc32(0, 0x12, 8);
        assert_ne!(crc, 0);

        let crc = crc32c(0, 0x12, 8);
        assert_ne!(crc, 0);
    }

    #[test]
    fn test_arm_cpu_trait() {
        let mut cpu = create_test_cpu();

        assert_eq!(cpu.arch_version(), ArmVersion::V8_0A);
        assert_eq!(cpu.profile(), ArmProfile::A);
        assert!(cpu.is_privileged()); // EL1 is privileged

        cpu.reset();
        assert_eq!(cpu.get_pc(), 0);

        // Test PSTATE
        let pstate = cpu.get_pstate();
        assert_eq!(pstate.el, 1);

        // Test register access via trait
        cpu.set_gpr(5, 0xDEAD_BEEF);
        assert_eq!(cpu.get_gpr(5), 0xDEAD_BEEF);

        // Test LR
        cpu.set_lr(0x1234);
        assert_eq!(cpu.get_lr(), 0x1234);
    }

    #[test]
    fn test_breakpoint() {
        let mut cpu = create_test_cpu();

        assert!(cpu.set_breakpoint(0x1000).is_ok());
        // set_breakpoint always succeeds (idempotent)
        assert!(cpu.set_breakpoint(0x1000).is_ok());

        assert!(cpu.clear_breakpoint(0x1000).is_ok());
        // clear_breakpoint is also idempotent
        assert!(cpu.clear_breakpoint(0x1000).is_ok());
    }

    // =========================================================================
    // Instruction Execution Tests
    // =========================================================================

    /// Helper to create a CPU and write an instruction at PC
    fn create_cpu_with_insn(insn: u32) -> AArch64Cpu {
        let mut cpu = create_test_cpu();
        cpu.write_memory(0, &insn.to_le_bytes()).unwrap();
        cpu
    }

    /// Helper to write instruction at specific address
    fn write_insn(cpu: &mut AArch64Cpu, addr: u64, insn: u32) {
        cpu.write_memory(addr, &insn.to_le_bytes()).unwrap();
    }

    // -------------------------------------------------------------------------
    // Data Processing Immediate - PC-relative addressing
    // -------------------------------------------------------------------------

    #[test]
    fn test_adr() {
        // ADR X0, #0x100 (PC + 0x100)
        // ADR: [0 immlo[1:0] 10000 immhi[18:0] Rd[4:0]]
        // PC=0, imm=0x100 -> immhi=0x40, immlo=0
        let insn = 0x10000800; // ADR X0, #0x100
        let mut cpu = create_cpu_with_insn(insn);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x100);
        assert_eq!(cpu.get_pc(), 4);
    }

    #[test]
    fn test_adrp() {
        // ADRP X1, #0x1000 (page-aligned, PC + 0x1000)
        // ADRP: [1 immlo[1:0] 10000 immhi[18:0] Rd[4:0]]
        let insn = 0x90000001; // ADRP X1, #0 (current page)
        let mut cpu = create_cpu_with_insn(insn);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(1), 0); // Page of PC=0
        assert_eq!(cpu.get_pc(), 4);
    }

    // -------------------------------------------------------------------------
    // Data Processing Immediate - Add/Subtract
    // -------------------------------------------------------------------------

    #[test]
    fn test_add_imm_64() {
        // ADD X0, X1, #0x123
        // sf=1, op=0, S=0, shift=0, imm12=0x123, Rn=1, Rd=0
        // [1 0 0 10001 00 imm12 Rn Rd]
        let insn = 0x91048C20; // ADD X0, X1, #0x123
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1000);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x1123);
    }

    #[test]
    fn test_add_imm_32() {
        // ADD W0, W1, #0x50
        // sf=0
        let insn = 0x11014020; // ADD W0, W1, #0x50
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xFFFF_FFFF_0000_0100);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x150); // 32-bit result, zero-extended
    }

    #[test]
    fn test_adds_imm_sets_flags() {
        // ADDS X0, X1, #1 (result will be 0, sets Z flag)
        let insn = 0xB1000420; // ADDS X0, X1, #1
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xFFFF_FFFF_FFFF_FFFF);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0);
        assert!(cpu.get_z()); // Zero flag
        assert!(cpu.get_c()); // Carry flag (overflow from addition)
    }

    #[test]
    fn test_sub_imm() {
        // SUB X0, X1, #0x100
        let insn = 0xD1040020; // SUB X0, X1, #0x100
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x500);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x400);
    }

    #[test]
    fn test_subs_imm_negative() {
        // SUBS X0, X1, #0x100 (result negative)
        let insn = 0xF1040020; // SUBS X0, X1, #0x100
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x50);
        cpu.step().unwrap();
        assert!(cpu.get_n()); // Negative
        assert!(!cpu.get_c()); // No borrow = C clear
    }

    #[test]
    fn test_add_imm_shifted() {
        // ADD X0, X1, #0x1, LSL #12
        // shift=1 means LSL #12
        let insn = 0x91400420; // ADD X0, X1, #1, LSL #12
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1000);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x2000);
    }

    // -------------------------------------------------------------------------
    // Data Processing Immediate - Logical
    // -------------------------------------------------------------------------

    #[test]
    fn test_and_imm() {
        // AND X0, X1, #0xFF (bitmask for low 8 bits)
        // For AND imm, the immediate is encoded as bitmask
        // N=1, immr=0, imms=7 gives 0xFF mask for 64-bit
        let insn = 0x92401C20; // AND X0, X1, #0xFF
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1234_5678);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x78);
    }

    #[test]
    fn test_orr_imm() {
        // ORR X0, X1, #0x1
        // N=1, immr=0, imms=0 -> single bit pattern
        // sf=1, opc=01, 100100, N=1, immr=000000, imms=000000, Rn=1, Rd=0
        // = 1 01 100100 1 000000 000000 00001 00000
        // = 0xB2400020
        let insn = 0xB2400020; // ORR X0, X1, #0x1
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1234_5678);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x1234_5679); // 0x1234_5678 | 0x1 = 0x1234_5679
    }

    #[test]
    fn test_eor_imm() {
        // EOR X0, X1, #1
        let insn = 0xD2400020; // EOR X0, X1, #1
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xAAAA);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xAAAB);
    }

    #[test]
    fn test_ands_imm() {
        // ANDS X0, X1, #0xFF (sets flags)
        let insn = 0xF2401C20; // ANDS X0, X1, #0xFF
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0);
        assert!(cpu.get_z()); // Zero flag set
    }

    // -------------------------------------------------------------------------
    // Data Processing Immediate - Move Wide
    // -------------------------------------------------------------------------

    #[test]
    fn test_movz() {
        // MOVZ X0, #0x1234
        let insn = 0xD2824680; // MOVZ X0, #0x1234
        let mut cpu = create_cpu_with_insn(insn);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x1234);
    }

    #[test]
    fn test_movz_shifted() {
        // MOVZ X0, #0xABCD, LSL #16 (hw=01)
        // Encoding: 1 10 100101 01 imm16 Rd = 0xD2B579A0
        let insn = 0xD2B579A0; // MOVZ X0, #0xABCD, LSL #16
        let mut cpu = create_cpu_with_insn(insn);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xABCD_0000);
    }

    #[test]
    fn test_movn() {
        // MOVN X0, #0 (result is ~0 = 0xFFFF_FFFF_FFFF_FFFF)
        let insn = 0x92800000; // MOVN X0, #0
        let mut cpu = create_cpu_with_insn(insn);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xFFFF_FFFF_FFFF_FFFF);
    }

    #[test]
    fn test_movk() {
        // MOVK X0, #0x5678, LSL #16 (keep other bits)
        let insn = 0xF2AACF00; // MOVK X0, #0x5678, LSL #16
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0x0000_0000_0000_1234);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x0000_0000_5678_1234);
    }

    // -------------------------------------------------------------------------
    // Data Processing Immediate - Bitfield
    // -------------------------------------------------------------------------

    #[test]
    fn test_ubfm_lsr() {
        // UBFM can implement LSR: LSR X0, X1, #4 = UBFM X0, X1, #4, #63
        let insn = 0xD344FC20; // UBFM X0, X1, #4, #63 (LSR #4)
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xF0);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x0F);
    }

    #[test]
    fn test_ubfm_uxtb() {
        // UXTB W0, W1 = UBFM W0, W1, #0, #7
        let insn = 0x53001C20; // UBFM W0, W1, #0, #7
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xFFFF_1234);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x34);
    }

    #[test]
    fn test_sbfm_asr() {
        // SBFM can implement ASR: ASR X0, X1, #4 = SBFM X0, X1, #4, #63
        let insn = 0x9344FC20; // SBFM X0, X1, #4, #63 (ASR #4)
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x8000_0000_0000_00F0u64);
        cpu.step().unwrap();
        // Sign-extended shift right
        assert_eq!(cpu.get_x(0), 0xF800_0000_0000_000F);
    }

    #[test]
    fn test_sbfm_sxtb() {
        // SXTB W0, W1 = SBFM W0, W1, #0, #7
        let insn = 0x13001C20; // SBFM W0, W1, #0, #7
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x80); // Negative byte
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xFFFF_FF80); // Sign-extended to 32-bit
    }

    #[test]
    fn test_bfm() {
        // BFM X0, X1, #4, #7 - insert bits
        let insn = 0xB3041C20; // BFM X0, X1, #4, #7
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0xFFFF_FFFF_FFFF_0000);
        cpu.set_x(1, 0x00AB);
        cpu.step().unwrap();
        // Bits 7:4 of X1 (0xA) inserted at appropriate position
        let result = cpu.get_x(0);
        // BFM behavior depends on the exact encoding
        assert_ne!(result, 0xFFFF_FFFF_FFFF_0000); // Changed
    }

    // -------------------------------------------------------------------------
    // Data Processing Immediate - Extract
    // -------------------------------------------------------------------------

    #[test]
    fn test_extr() {
        // EXTR X0, X1, X2, #8 - extract bits from concatenation
        // result = (X1 << (64-8)) | (X2 >> 8)
        let insn = 0x93C22020; // EXTR X0, X1, X2, #8
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x0000_0000_0000_00FF);
        cpu.set_x(2, 0xFF00_0000_0000_0000);
        cpu.step().unwrap();
        // (0xFF << 56) | (0xFF00... >> 8) = 0xFF00... | 0x00FF... = 0xFFFF...
        assert_eq!(cpu.get_x(0), 0xFFFF_0000_0000_0000);
    }

    #[test]
    fn test_ror_via_extr() {
        // ROR X0, X1, #4 = EXTR X0, X1, X1, #4
        let insn = 0x93C11020; // EXTR X0, X1, X1, #4
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xF);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xF000_0000_0000_0000);
    }

    // -------------------------------------------------------------------------
    // Branch Instructions - Conditional
    // -------------------------------------------------------------------------

    #[test]
    fn test_b_cond_taken() {
        // B.EQ #0x100 (taken when Z=1)
        let insn = 0x54000800; // B.EQ #0x100
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_z(true);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x100);
    }

    #[test]
    fn test_b_cond_not_taken() {
        // B.EQ #0x100 (not taken when Z=0)
        let insn = 0x54000800; // B.EQ #0x100
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_z(false);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 4); // Falls through
    }

    #[test]
    fn test_b_ne() {
        // B.NE #0x20
        let insn = 0x54000101; // B.NE #0x20
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_z(false);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x20);
    }

    // -------------------------------------------------------------------------
    // Branch Instructions - Unconditional
    // -------------------------------------------------------------------------

    #[test]
    fn test_b() {
        // B #0x1000
        let insn = 0x14000400; // B #0x1000
        let mut cpu = create_cpu_with_insn(insn);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x1000);
    }

    #[test]
    fn test_b_negative() {
        // B #-0x100 (backward branch)
        // imm26 = -0x40 (in instruction words) = 0x3FFFFC0
        let insn = 0x17FFFFC0; // B #-0x100
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_pc(0x1000);
        write_insn(&mut cpu, 0x1000, insn);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0xF00);
    }

    #[test]
    fn test_bl() {
        // BL #0x100 (saves return address in X30)
        let insn = 0x94000040; // BL #0x100
        let mut cpu = create_cpu_with_insn(insn);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x100);
        assert_eq!(cpu.get_x(30), 4); // Return address
    }

    // -------------------------------------------------------------------------
    // Branch Instructions - Compare and Branch
    // -------------------------------------------------------------------------

    #[test]
    fn test_cbz_taken() {
        // CBZ X0, #0x100
        let insn = 0xB4000800; // CBZ X0, #0x100
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x100);
    }

    #[test]
    fn test_cbz_not_taken() {
        // CBZ X0, #0x100
        let insn = 0xB4000800; // CBZ X0, #0x100
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 1);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 4);
    }

    #[test]
    fn test_cbnz_taken() {
        // CBNZ X1, #0x80
        let insn = 0xB5000401; // CBNZ X1, #0x80
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1234);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x80);
    }

    #[test]
    fn test_cbz_32bit() {
        // CBZ W0, #0x20
        let insn = 0x34000100; // CBZ W0, #0x20
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0xFFFF_FFFF_0000_0000); // Upper bits set but W0 is 0
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x20);
    }

    // -------------------------------------------------------------------------
    // Branch Instructions - Test and Branch
    // -------------------------------------------------------------------------

    #[test]
    fn test_tbz_taken() {
        // TBZ X0, #0, #0x40 (branch if bit 0 is 0)
        let insn = 0x36000200; // TBZ X0, #0, #0x40
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0xFFFE); // Bit 0 is 0
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x40);
    }

    #[test]
    fn test_tbz_not_taken() {
        // TBZ X0, #0, #0x40
        let insn = 0x36000200; // TBZ X0, #0, #0x40
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0xFFFF); // Bit 0 is 1
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 4);
    }

    #[test]
    fn test_tbnz_taken() {
        // TBNZ X0, #4, #0x80 (branch if bit 4 is 1)
        let insn = 0x37200400; // TBNZ X0, #4, #0x80
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0x10); // Bit 4 is 1
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x80);
    }

    #[test]
    fn test_tbz_high_bit() {
        // TBZ X0, #63, #0x20 (test highest bit)
        let insn = 0xB6F80100; // TBZ X0, #63, #0x20
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0x7FFF_FFFF_FFFF_FFFF); // Bit 63 is 0
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x20);
    }

    // -------------------------------------------------------------------------
    // Branch Instructions - Register
    // -------------------------------------------------------------------------

    #[test]
    fn test_br() {
        // BR X1
        let insn = 0xD61F0020; // BR X1
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x2000);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x2000);
    }

    #[test]
    fn test_blr() {
        // BLR X5
        let insn = 0xD63F00A0; // BLR X5
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(5, 0x4000);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x4000);
        assert_eq!(cpu.get_x(30), 4); // Return address
    }

    #[test]
    fn test_ret() {
        // RET (uses X30 by default)
        let insn = 0xD65F03C0; // RET
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(30, 0x8000);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x8000);
    }

    #[test]
    fn test_ret_xn() {
        // RET X5
        let insn = 0xD65F00A0; // RET X5
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(5, 0x3000);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x3000);
    }

    // -------------------------------------------------------------------------
    // Load/Store Instructions - LDR Literal
    // -------------------------------------------------------------------------

    #[test]
    fn test_ldr_literal_64() {
        // LDR X0, #0x100 (load from PC+0x100)
        let insn = 0x58000800; // LDR X0, #0x100
        let mut cpu = create_cpu_with_insn(insn);
        // Write test value at offset 0x100
        cpu.write_memory(0x100, &0xDEAD_BEEF_CAFE_BABEu64.to_le_bytes())
            .unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xDEAD_BEEF_CAFE_BABE);
    }

    #[test]
    fn test_ldr_literal_32() {
        // LDR W0, #0x80
        let insn = 0x18000400; // LDR W0, #0x80
        let mut cpu = create_cpu_with_insn(insn);
        cpu.write_memory(0x80, &0x1234_5678u32.to_le_bytes())
            .unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x1234_5678);
    }

    #[test]
    fn test_ldrsw_literal() {
        // LDRSW X0, #0x40 (sign-extended 32-bit load)
        let insn = 0x98000200; // LDRSW X0, #0x40
        let mut cpu = create_cpu_with_insn(insn);
        cpu.write_memory(0x40, &0x8000_0001u32.to_le_bytes())
            .unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xFFFF_FFFF_8000_0001); // Sign-extended
    }

    // -------------------------------------------------------------------------
    // Load/Store Instructions - Load/Store Pair
    // -------------------------------------------------------------------------

    #[test]
    fn test_stp_64() {
        // STP X0, X1, [X2]
        let insn = 0xA9000440; // STP X0, X1, [X2]
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0x1111_1111_1111_1111);
        cpu.set_x(1, 0x2222_2222_2222_2222);
        cpu.set_x(2, 0x1000);
        cpu.step().unwrap();

        let data = cpu.read_memory(0x1000, 8).unwrap();
        assert_eq!(
            u64::from_le_bytes(data[..8].try_into().unwrap()),
            0x1111_1111_1111_1111
        );

        let data = cpu.read_memory(0x1008, 8).unwrap();
        assert_eq!(
            u64::from_le_bytes(data[..8].try_into().unwrap()),
            0x2222_2222_2222_2222
        );
    }

    #[test]
    fn test_ldp_64() {
        // LDP X0, X1, [X2]
        let insn = 0xA9400440; // LDP X0, X1, [X2]
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(2, 0x1000);
        cpu.write_memory(0x1000, &0xAAAA_BBBB_CCCC_DDDDu64.to_le_bytes())
            .unwrap();
        cpu.write_memory(0x1008, &0x1234_5678_9ABC_DEF0u64.to_le_bytes())
            .unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xAAAA_BBBB_CCCC_DDDD);
        assert_eq!(cpu.get_x(1), 0x1234_5678_9ABC_DEF0);
    }

    #[test]
    fn test_ldp_post_index() {
        // LDP X0, X1, [X2], #16
        let insn = 0xA8C10440; // LDP X0, X1, [X2], #16
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(2, 0x1000);
        cpu.write_memory(0x1000, &1u64.to_le_bytes()).unwrap();
        cpu.write_memory(0x1008, &2u64.to_le_bytes()).unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 1);
        assert_eq!(cpu.get_x(1), 2);
        assert_eq!(cpu.get_x(2), 0x1010); // Post-indexed
    }

    #[test]
    fn test_stp_pre_index() {
        // STP X0, X1, [X2, #-16]!
        let insn = 0xA9BF0440; // STP X0, X1, [X2, #-16]!
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0x1111);
        cpu.set_x(1, 0x2222);
        cpu.set_x(2, 0x1010);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(2), 0x1000); // Pre-indexed
    }

    #[test]
    fn test_ldp_32() {
        // LDP W0, W1, [X2]
        let insn = 0x29400440; // LDP W0, W1, [X2]
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(2, 0x1000);
        cpu.write_memory(0x1000, &0xDEAD_BEEFu32.to_le_bytes())
            .unwrap();
        cpu.write_memory(0x1004, &0xCAFE_BABEu32.to_le_bytes())
            .unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xDEAD_BEEF);
        assert_eq!(cpu.get_x(1), 0xCAFE_BABE);
    }

    // -------------------------------------------------------------------------
    // Load/Store Instructions - Register Offset
    // -------------------------------------------------------------------------

    #[test]
    fn test_str_imm() {
        // STR X0, [X1, #8]
        let insn = 0xF9000420; // STR X0, [X1, #8]
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0xDEAD_BEEF_1234_5678);
        cpu.set_x(1, 0x1000);
        cpu.step().unwrap();

        let data = cpu.read_memory(0x1008, 8).unwrap();
        assert_eq!(
            u64::from_le_bytes(data[..8].try_into().unwrap()),
            0xDEAD_BEEF_1234_5678
        );
    }

    #[test]
    fn test_ldr_imm() {
        // LDR X0, [X1, #16]
        let insn = 0xF9400820; // LDR X0, [X1, #16]
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1000);
        cpu.write_memory(0x1010, &0xCAFE_BABE_DEAD_BEEFu64.to_le_bytes())
            .unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xCAFE_BABE_DEAD_BEEF);
    }

    #[test]
    fn test_strb() {
        // STRB W0, [X1]
        let insn = 0x39000020; // STRB W0, [X1]
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0x1234_5678);
        cpu.set_x(1, 0x1000);
        cpu.step().unwrap();

        let data = cpu.read_memory(0x1000, 1).unwrap();
        assert_eq!(data[0], 0x78);
    }

    #[test]
    fn test_ldrb() {
        // LDRB W0, [X1]
        let insn = 0x39400020; // LDRB W0, [X1]
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1000);
        cpu.write_memory(0x1000, &[0xAB]).unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xAB);
    }

    #[test]
    fn test_ldrsb() {
        // LDRSB X0, [X1] (sign-extend byte to 64-bit)
        let insn = 0x39800020; // LDRSB X0, [X1]
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1000);
        cpu.write_memory(0x1000, &[0x80]).unwrap(); // Negative byte
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xFFFF_FFFF_FFFF_FF80);
    }

    #[test]
    fn test_strh() {
        // STRH W0, [X1]
        let insn = 0x79000020; // STRH W0, [X1]
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0x1234_5678);
        cpu.set_x(1, 0x1000);
        cpu.step().unwrap();

        let data = cpu.read_memory(0x1000, 2).unwrap();
        assert_eq!(u16::from_le_bytes(data[..2].try_into().unwrap()), 0x5678);
    }

    #[test]
    fn test_ldrh() {
        // LDRH W0, [X1]
        let insn = 0x79400020; // LDRH W0, [X1]
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1000);
        cpu.write_memory(0x1000, &0xABCDu16.to_le_bytes()).unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xABCD);
    }

    #[test]
    fn test_ldrsh() {
        // LDRSH X0, [X1] (sign-extend halfword to 64-bit)
        let insn = 0x79800020; // LDRSH X0, [X1]
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1000);
        cpu.write_memory(0x1000, &0x8001u16.to_le_bytes()).unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xFFFF_FFFF_FFFF_8001);
    }

    #[test]
    fn test_str_32() {
        // STR W0, [X1]
        let insn = 0xB9000020; // STR W0, [X1]
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0xDEAD_BEEF);
        cpu.set_x(1, 0x1000);
        cpu.step().unwrap();

        let data = cpu.read_memory(0x1000, 4).unwrap();
        assert_eq!(
            u32::from_le_bytes(data[..4].try_into().unwrap()),
            0xDEAD_BEEF
        );
    }

    #[test]
    fn test_ldr_32() {
        // LDR W0, [X1]
        let insn = 0xB9400020; // LDR W0, [X1]
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1000);
        cpu.write_memory(0x1000, &0x1234_5678u32.to_le_bytes())
            .unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x1234_5678);
    }

    #[test]
    fn test_ldrsw() {
        // LDRSW X0, [X1] (sign-extend word to 64-bit)
        let insn = 0xB9800020; // LDRSW X0, [X1]
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1000);
        cpu.write_memory(0x1000, &0x8000_0001u32.to_le_bytes())
            .unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xFFFF_FFFF_8000_0001);
    }

    #[test]
    fn test_ldr_post_index() {
        // LDR X0, [X1], #8
        let insn = 0xF8408420; // LDR X0, [X1], #8
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1000);
        cpu.write_memory(0x1000, &0x1234u64.to_le_bytes()).unwrap();
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x1234);
        assert_eq!(cpu.get_x(1), 0x1008); // Post-indexed
    }

    #[test]
    fn test_str_pre_index() {
        // STR X0, [X1, #8]!
        let insn = 0xF8008C20; // STR X0, [X1, #8]!
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 0x5678);
        cpu.set_x(1, 0x1000);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(1), 0x1008); // Pre-indexed

        let data = cpu.read_memory(0x1008, 8).unwrap();
        assert_eq!(u64::from_le_bytes(data[..8].try_into().unwrap()), 0x5678);
    }

    // -------------------------------------------------------------------------
    // Data Processing Register - Logical Shifted Register
    // -------------------------------------------------------------------------

    #[test]
    fn test_and_shifted() {
        // AND X0, X1, X2
        let insn = 0x8A020020; // AND X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xFF00_FF00);
        cpu.set_x(2, 0x0FF0_0FF0);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x0F00_0F00);
    }

    #[test]
    fn test_and_lsl() {
        // AND X0, X1, X2, LSL #4
        let insn = 0x8A021020; // AND X0, X1, X2, LSL #4
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xFFFF);
        cpu.set_x(2, 0x00FF);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x0FF0);
    }

    #[test]
    fn test_orr_reg() {
        // ORR X0, X1, X2
        let insn = 0xAA020020; // ORR X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xF0F0);
        cpu.set_x(2, 0x0F0F);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xFFFF);
    }

    #[test]
    fn test_eor_reg() {
        // EOR X0, X1, X2
        let insn = 0xCA020020; // EOR X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xFFFF);
        cpu.set_x(2, 0x0F0F);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xF0F0);
    }

    #[test]
    fn test_bic() {
        // BIC X0, X1, X2 (bit clear: X1 AND NOT X2)
        let insn = 0x8A220020; // BIC X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xFFFF);
        cpu.set_x(2, 0x00FF);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xFF00);
    }

    #[test]
    fn test_orn() {
        // ORN X0, X1, X2 (X1 OR NOT X2)
        let insn = 0xAA220020; // ORN X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0);
        cpu.set_x(2, 0xFF);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), !0xFFu64);
    }

    #[test]
    fn test_eon() {
        // EON X0, X1, X2 (X1 XOR NOT X2)
        let insn = 0xCA220020; // EON X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0);
        cpu.set_x(2, 0);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), !0u64);
    }

    #[test]
    fn test_ands_reg() {
        // ANDS X0, X1, X2 (sets flags)
        let insn = 0xEA020020; // ANDS X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1000);
        cpu.set_x(2, 0x0001);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0);
        assert!(cpu.get_z()); // Result is zero
    }

    #[test]
    fn test_tst() {
        // TST X1, X2 (ANDS XZR, X1, X2)
        let insn = 0xEA02003F; // TST X1, X2 (Rd=XZR)
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x8000_0000_0000_0000);
        cpu.set_x(2, 0x8000_0000_0000_0000);
        cpu.step().unwrap();
        assert!(cpu.get_n()); // Negative (bit 63 set)
        assert!(!cpu.get_z()); // Not zero
    }

    #[test]
    fn test_mov_reg() {
        // MOV X0, X1 (alias for ORR X0, XZR, X1)
        let insn = 0xAA0103E0; // MOV X0, X1
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xDEAD_BEEF);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xDEAD_BEEF);
    }

    #[test]
    fn test_mvn() {
        // MVN X0, X1 (alias for ORN X0, XZR, X1)
        let insn = 0xAA2103E0; // MVN X0, X1
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), !0u64);
    }

    // -------------------------------------------------------------------------
    // Data Processing Register - Add/Subtract Shifted/Extended
    // -------------------------------------------------------------------------

    #[test]
    fn test_add_shifted() {
        // ADD X0, X1, X2
        let insn = 0x8B020020; // ADD X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 100);
        cpu.set_x(2, 200);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 300);
    }

    #[test]
    fn test_add_lsl() {
        // ADD X0, X1, X2, LSL #2
        let insn = 0x8B020820; // ADD X0, X1, X2, LSL #2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 100);
        cpu.set_x(2, 25);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 200);
    }

    #[test]
    fn test_sub_shifted() {
        // SUB X0, X1, X2
        let insn = 0xCB020020; // SUB X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 500);
        cpu.set_x(2, 200);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 300);
    }

    #[test]
    fn test_adds_shifted() {
        // ADDS X0, X1, X2 (sets flags)
        let insn = 0xAB020020; // ADDS X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xFFFF_FFFF_FFFF_FFFF);
        cpu.set_x(2, 1);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0);
        assert!(cpu.get_z()); // Zero
        assert!(cpu.get_c()); // Carry
    }

    #[test]
    fn test_subs_shifted() {
        // SUBS X0, X1, X2 (CMP alias when Rd=XZR)
        let insn = 0xEB020020; // SUBS X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 100);
        cpu.set_x(2, 100);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0);
        assert!(cpu.get_z());
        assert!(cpu.get_c()); // No borrow = C set
    }

    #[test]
    fn test_cmp() {
        // CMP X1, X2 (SUBS XZR, X1, X2)
        let insn = 0xEB02003F; // CMP X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 50);
        cpu.set_x(2, 100);
        cpu.step().unwrap();
        assert!(cpu.get_n()); // Negative
        assert!(!cpu.get_c()); // Borrow = C clear
    }

    #[test]
    fn test_cmn() {
        // CMN X1, X2 (ADDS XZR, X1, X2)
        let insn = 0xAB02003F; // CMN X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xFFFF_FFFF_FFFF_FFFF);
        cpu.set_x(2, 1);
        cpu.step().unwrap();
        assert!(cpu.get_z()); // Result is zero
        assert!(cpu.get_c()); // Carry out
    }

    #[test]
    fn test_neg() {
        // NEG X0, X1 (SUB X0, XZR, X1)
        let insn = 0xCB0103E0; // NEG X0, X1
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 1);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xFFFF_FFFF_FFFF_FFFF);
    }

    #[test]
    fn test_add_extended() {
        // ADD X0, X1, W2, UXTW (zero-extend W2 to 64-bit)
        let insn = 0x8B224020; // ADD X0, X1, W2, UXTW
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1000_0000_0000_0000);
        cpu.set_x(2, 0xFFFF_FFFF_0000_0100);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x1000_0000_0000_0100);
    }

    #[test]
    fn test_add_extended_sxtw() {
        // ADD X0, X1, W2, SXTW (sign-extend W2 to 64-bit)
        let insn = 0x8B22C020; // ADD X0, X1, W2, SXTW
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0);
        cpu.set_x(2, 0x8000_0000); // Negative when sign-extended
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xFFFF_FFFF_8000_0000);
    }

    // -------------------------------------------------------------------------
    // Data Processing Register - ADC/SBC
    // -------------------------------------------------------------------------

    #[test]
    fn test_adc() {
        // ADC X0, X1, X2 (add with carry)
        let insn = 0x9A020020; // ADC X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 100);
        cpu.set_x(2, 200);
        cpu.set_c(true);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 301); // 100 + 200 + 1
    }

    #[test]
    fn test_adc_no_carry() {
        // ADC X0, X1, X2 (no carry in)
        let insn = 0x9A020020; // ADC X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 100);
        cpu.set_x(2, 200);
        cpu.set_c(false);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 300);
    }

    #[test]
    fn test_adcs() {
        // ADCS X0, X1, X2 (sets flags)
        let insn = 0xBA020020; // ADCS X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xFFFF_FFFF_FFFF_FFFF);
        cpu.set_x(2, 0);
        cpu.set_c(true);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0);
        assert!(cpu.get_z());
        assert!(cpu.get_c()); // Overflow
    }

    #[test]
    fn test_sbc() {
        // SBC X0, X1, X2 (subtract with carry/borrow)
        let insn = 0xDA020020; // SBC X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 500);
        cpu.set_x(2, 200);
        cpu.set_c(true); // No borrow
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 300);
    }

    #[test]
    fn test_sbc_borrow() {
        // SBC X0, X1, X2 (with borrow)
        let insn = 0xDA020020; // SBC X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 500);
        cpu.set_x(2, 200);
        cpu.set_c(false); // Borrow
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 299);
    }

    #[test]
    fn test_sbcs() {
        // SBCS X0, X1, X2 (sets flags)
        let insn = 0xFA020020; // SBCS X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 100);
        cpu.set_x(2, 100);
        cpu.set_c(true);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0);
        assert!(cpu.get_z());
    }

    #[test]
    fn test_ngc() {
        // NGC X0, X1 (SBC X0, XZR, X1)
        let insn = 0xDA0103E0; // NGC X0, X1
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0);
        cpu.set_c(true);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0);
    }

    // -------------------------------------------------------------------------
    // Data Processing Register - Conditional Compare
    // -------------------------------------------------------------------------

    #[test]
    fn test_ccmp_true() {
        // CCMP X1, X2, #0, EQ (compare if Z=1)
        // Encoding: sf=1 11 11010010 Rm cond 00 Rn 0 nzcv
        // = 111 11010010 00010 0000 00 00001 0 0000
        // = 0xFA420020
        let insn = 0xFA420020; // CCMP X1, X2, #0, EQ
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 100);
        cpu.set_x(2, 100);
        cpu.set_z(true); // Condition true (EQ)
        cpu.step().unwrap();
        assert!(cpu.get_z()); // Result of comparison (100-100=0)
        assert!(cpu.get_c()); // No borrow
    }

    #[test]
    fn test_ccmp_false() {
        // CCMP X1, X2, #0b0100, EQ (use nzcv if Z=0)
        // Encoding: 111 11010010 00010 0000 00 00001 0 0100
        // = 0xFA420024
        let insn = 0xFA420024; // CCMP X1, X2, #4, EQ (nzcv=0100)
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_z(false); // Condition false
        cpu.step().unwrap();
        assert!(cpu.get_z()); // nzcv bit 2 = Z
        assert!(!cpu.get_c()); // nzcv bit 1 = C (clear)
    }

    #[test]
    fn test_ccmn() {
        // CCMN X1, X2, #0, NE (add comparison if Z=0)
        // Encoding: sf=1 01 11010010 Rm cond 00 Rn 0 nzcv (note: op=0 for CCMN)
        // = 101 11010010 00010 0001 00 00001 0 0000
        // = 0xBA421020
        let insn = 0xBA421020; // CCMN X1, X2, #0, NE
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xFFFF_FFFF_FFFF_FFFF);
        cpu.set_x(2, 1);
        cpu.set_z(false); // NE is true
        cpu.step().unwrap();
        assert!(cpu.get_z()); // Result is zero
        assert!(cpu.get_c()); // Carry out
    }

    // -------------------------------------------------------------------------
    // Data Processing Register - Conditional Select
    // -------------------------------------------------------------------------

    #[test]
    fn test_csel_true() {
        // CSEL X0, X1, X2, EQ (select X1 if Z=1)
        let insn = 0x9A820020; // CSEL X0, X1, X2, EQ
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1111);
        cpu.set_x(2, 0x2222);
        cpu.set_z(true);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x1111);
    }

    #[test]
    fn test_csel_false() {
        // CSEL X0, X1, X2, EQ (select X2 if Z=0)
        let insn = 0x9A820020; // CSEL X0, X1, X2, EQ
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1111);
        cpu.set_x(2, 0x2222);
        cpu.set_z(false);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x2222);
    }

    #[test]
    fn test_csinc_true() {
        // CSINC X0, X1, X2, NE (select X1 if Z=0)
        let insn = 0x9A821420; // CSINC X0, X1, X2, NE
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 100);
        cpu.set_x(2, 200);
        cpu.set_z(false); // NE is true
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 100);
    }

    #[test]
    fn test_csinc_false() {
        // CSINC X0, X1, X2, NE (select X2+1 if Z=1)
        let insn = 0x9A821420; // CSINC X0, X1, X2, NE
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 100);
        cpu.set_x(2, 200);
        cpu.set_z(true); // NE is false
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 201);
    }

    #[test]
    fn test_csinv() {
        // CSINV X0, X1, X2, EQ (select X1 if Z=1, else ~X2)
        let insn = 0xDA820020; // CSINV X0, X1, X2, EQ
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1111);
        cpu.set_x(2, 0);
        cpu.set_z(false);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), !0u64);
    }

    #[test]
    fn test_csneg() {
        // CSNEG X0, X1, X2, EQ (select X1 if Z=1, else -X2)
        let insn = 0xDA820420; // CSNEG X0, X1, X2, EQ
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0);
        cpu.set_x(2, 5);
        cpu.set_z(false);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xFFFF_FFFF_FFFF_FFFB); // -5
    }

    #[test]
    fn test_cinc() {
        // CINC X0, X1, NE = CSINC X0, X1, X1, EQ
        // If EQ is true: X0 = X1
        // If EQ is false (NE is true): X0 = X1 + 1
        let insn = 0x9A810420; // CINC X0, X1, NE
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 100);
        cpu.set_z(false); // EQ is false, so NE is true -> X0 = X1 + 1
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 101);
    }

    #[test]
    fn test_cset() {
        // CSET X0, EQ (CSINC X0, XZR, XZR, NE)
        let insn = 0x9A9F17E0; // CSET X0, EQ
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_z(true); // EQ is true
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 1);
    }

    #[test]
    fn test_csetm() {
        // CSETM X0, EQ = CSINV X0, XZR, XZR, NE
        // If NE (Z=0): X0 = XZR = 0
        // If EQ (Z=1): X0 = NOT(XZR) = !0
        // Encoding: sf=1 op=1 S=0 11010100 Rm=11111 cond=0001(NE) op2=00 Rn=11111 Rd=00000
        // = 110 11010100 11111 0001 00 11111 00000 = 0xDA9F13E0
        let insn = 0xDA9F13E0; // CSETM X0, EQ (encoded as CSINV X0, XZR, XZR, NE)
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_z(true); // EQ is true, so NE is false -> X0 = !0
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), !0u64);
    }

    // -------------------------------------------------------------------------
    // Data Processing Register - 2-source
    // -------------------------------------------------------------------------

    #[test]
    fn test_udiv() {
        // UDIV X0, X1, X2
        let insn = 0x9AC20820; // UDIV X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 100);
        cpu.set_x(2, 7);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 14);
    }

    #[test]
    fn test_udiv_by_zero() {
        // UDIV X0, X1, X2 (divide by zero returns 0)
        let insn = 0x9AC20820; // UDIV X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 100);
        cpu.set_x(2, 0);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0);
    }

    #[test]
    fn test_sdiv() {
        // SDIV X0, X1, X2
        let insn = 0x9AC20C20; // SDIV X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, (-100i64) as u64);
        cpu.set_x(2, 7);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0) as i64, -14);
    }

    #[test]
    fn test_sdiv_by_zero() {
        // SDIV X0, X1, X2 (divide by zero returns 0)
        let insn = 0x9AC20C20; // SDIV X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, (-100i64) as u64);
        cpu.set_x(2, 0);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0);
    }

    #[test]
    fn test_lslv() {
        // LSLV X0, X1, X2 (logical shift left variable)
        let insn = 0x9AC22020; // LSLV X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xFF);
        cpu.set_x(2, 4);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xFF0);
    }

    #[test]
    fn test_lsrv() {
        // LSRV X0, X1, X2 (logical shift right variable)
        let insn = 0x9AC22420; // LSRV X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xFF0);
        cpu.set_x(2, 4);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xFF);
    }

    #[test]
    fn test_asrv() {
        // ASRV X0, X1, X2 (arithmetic shift right variable)
        let insn = 0x9AC22820; // ASRV X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x8000_0000_0000_0000);
        cpu.set_x(2, 4);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xF800_0000_0000_0000);
    }

    #[test]
    fn test_rorv() {
        // RORV X0, X1, X2 (rotate right variable)
        let insn = 0x9AC22C20; // RORV X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xF);
        cpu.set_x(2, 4);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0xF000_0000_0000_0000);
    }

    // -------------------------------------------------------------------------
    // Data Processing Register - 3-source
    // -------------------------------------------------------------------------

    #[test]
    fn test_madd() {
        // MADD X0, X1, X2, X3 (X0 = X1*X2 + X3)
        let insn = 0x9B020C20; // MADD X0, X1, X2, X3
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 10);
        cpu.set_x(2, 20);
        cpu.set_x(3, 5);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 205);
    }

    #[test]
    fn test_mul() {
        // MUL X0, X1, X2 (MADD X0, X1, X2, XZR)
        let insn = 0x9B027C20; // MUL X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 100);
        cpu.set_x(2, 200);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 20000);
    }

    #[test]
    fn test_msub() {
        // MSUB X0, X1, X2, X3 (X0 = X3 - X1*X2)
        let insn = 0x9B028C20; // MSUB X0, X1, X2, X3
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 10);
        cpu.set_x(2, 20);
        cpu.set_x(3, 500);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 300);
    }

    #[test]
    fn test_mneg() {
        // MNEG X0, X1, X2 (MSUB X0, X1, X2, XZR)
        let insn = 0x9B02FC20; // MNEG X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 10);
        cpu.set_x(2, 20);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0) as i64, -200);
    }

    #[test]
    fn test_smaddl() {
        // SMADDL X0, W1, W2, X3 (signed widening multiply-add)
        let insn = 0x9B220C20; // SMADDL X0, W1, W2, X3
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xFFFF_FFFF); // -1 as W
        cpu.set_x(2, 10);
        cpu.set_x(3, 100);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0) as i64, 90); // 100 + (-1 * 10)
    }

    #[test]
    fn test_smull() {
        // SMULL X0, W1, W2 (SMADDL X0, W1, W2, XZR)
        let insn = 0x9B227C20; // SMULL X0, W1, W2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xFFFF_FFFF); // -1 as W
        cpu.set_x(2, 100);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0) as i64, -100);
    }

    #[test]
    fn test_umaddl() {
        // UMADDL X0, W1, W2, X3 (unsigned widening multiply-add)
        let insn = 0x9BA20C20; // UMADDL X0, W1, W2, X3
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xFFFF_FFFF); // Max u32
        cpu.set_x(2, 2);
        cpu.set_x(3, 1);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x1_FFFF_FFFF); // 2 * 0xFFFF_FFFF + 1
    }

    #[test]
    fn test_umull() {
        // UMULL X0, W1, W2 (UMADDL X0, W1, W2, XZR)
        let insn = 0x9BA27C20; // UMULL X0, W1, W2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x1_0000);
        cpu.set_x(2, 0x1_0000);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0x1_0000_0000);
    }

    #[test]
    fn test_smulh() {
        // SMULH X0, X1, X2 (signed high multiply)
        let insn = 0x9B427C20; // SMULH X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x8000_0000_0000_0000); // Large negative
        cpu.set_x(2, 2);
        cpu.step().unwrap();
        // Result is high 64 bits of signed 128-bit product
        assert_eq!(cpu.get_x(0), 0xFFFF_FFFF_FFFF_FFFF);
    }

    #[test]
    fn test_umulh() {
        // UMULH X0, X1, X2 (unsigned high multiply)
        let insn = 0x9BC27C20; // UMULH X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x8000_0000_0000_0000);
        cpu.set_x(2, 2);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 1);
    }

    // -------------------------------------------------------------------------
    // System Instructions
    // -------------------------------------------------------------------------

    #[test]
    fn test_nop() {
        // NOP
        let insn = 0xD503201F; // NOP
        let mut cpu = create_cpu_with_insn(insn);
        let old_pc = cpu.get_pc();
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), old_pc + 4);
    }

    #[test]
    fn test_dmb() {
        // DMB SY
        let insn = 0xD5033FBF; // DMB SY
        let mut cpu = create_cpu_with_insn(insn);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 4);
    }

    #[test]
    fn test_dsb() {
        // DSB SY
        let insn = 0xD5033F9F; // DSB SY
        let mut cpu = create_cpu_with_insn(insn);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 4);
    }

    #[test]
    fn test_isb() {
        // ISB
        let insn = 0xD5033FDF; // ISB
        let mut cpu = create_cpu_with_insn(insn);
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 4);
    }

    // -------------------------------------------------------------------------
    // Multi-instruction sequences
    // -------------------------------------------------------------------------

    #[test]
    fn test_simple_program() {
        // Simple program: MOV X0, #1; ADD X0, X0, #1; ADD X0, X0, #1
        let mut cpu = create_test_cpu();
        write_insn(&mut cpu, 0, 0xD2800020); // MOV X0, #1
        write_insn(&mut cpu, 4, 0x91000400); // ADD X0, X0, #1
        write_insn(&mut cpu, 8, 0x91000400); // ADD X0, X0, #1

        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 1);

        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 2);

        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 3);
    }

    #[test]
    fn test_loop() {
        // Simple countdown loop
        // 0x0000: MOV X0, #5
        // 0x0004: SUBS X0, X0, #1
        // 0x0008: B.NE #-4
        let mut cpu = create_test_cpu();
        write_insn(&mut cpu, 0, 0xD28000A0); // MOV X0, #5
        write_insn(&mut cpu, 4, 0xF1000400); // SUBS X0, X0, #1
        write_insn(&mut cpu, 8, 0x54FFFFE1); // B.NE #-4

        // Execute MOV
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 5);

        // Execute loop 5 times
        for expected in (0..5).rev() {
            cpu.step().unwrap(); // SUBS
            assert_eq!(cpu.get_x(0), expected);
            cpu.step().unwrap(); // B.NE or fall through
        }

        // After loop, PC should be at 0x0C (fell through)
        assert_eq!(cpu.get_pc(), 0x0C);
    }

    #[test]
    fn test_function_call() {
        // Test function call and return
        // 0x0000: MOV X0, #42
        // 0x0004: BL #0x100
        // 0x0008: ADD X0, X0, #1  (after return)
        // ...
        // 0x0104: ADD X0, X0, X0
        // 0x0108: RET
        let mut cpu = create_test_cpu();
        write_insn(&mut cpu, 0x0000, 0xD2800540); // MOV X0, #42
        write_insn(&mut cpu, 0x0004, 0x94000040); // BL #0x100
        write_insn(&mut cpu, 0x0008, 0x91000400); // ADD X0, X0, #1

        write_insn(&mut cpu, 0x0104, 0x8B000000); // ADD X0, X0, X0
        write_insn(&mut cpu, 0x0108, 0xD65F03C0); // RET

        // MOV X0, #42
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 42);

        // BL #0x100
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 0x104);
        assert_eq!(cpu.get_x(30), 8); // Return address

        // ADD X0, X0, X0
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 84);

        // RET
        cpu.step().unwrap();
        assert_eq!(cpu.get_pc(), 8);

        // ADD X0, X0, #1
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 85);
    }

    #[test]
    fn test_memory_operations() {
        // Test store and load sequence
        // MOV X0, #0xABCD
        // MOV X1, #0x1000
        // STR X0, [X1]
        // MOV X0, #0
        // LDR X2, [X1]
        let mut cpu = create_test_cpu();
        write_insn(&mut cpu, 0x00, 0xD29579A0); // MOV X0, #0xABCD (imm16=0xABCD, hw=0)
        write_insn(&mut cpu, 0x04, 0xD2820001); // MOV X1, #0x1000
        write_insn(&mut cpu, 0x08, 0xF9000020); // STR X0, [X1]
        write_insn(&mut cpu, 0x0C, 0xD2800000); // MOV X0, #0
        write_insn(&mut cpu, 0x10, 0xF9400022); // LDR X2, [X1]

        for _ in 0..5 {
            cpu.step().unwrap();
        }

        assert_eq!(cpu.get_x(0), 0);
        assert_eq!(cpu.get_x(2), 0xABCD);
    }

    // -------------------------------------------------------------------------
    // Edge cases and special values
    // -------------------------------------------------------------------------

    #[test]
    fn test_max_values() {
        // ADD with maximum 64-bit value
        let insn = 0x91000400; // ADD X0, X0, #1
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, u64::MAX);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 0); // Wraps around
    }

    #[test]
    fn test_signed_overflow() {
        // ADDS with signed overflow
        let insn = 0xAB020020; // ADDS X0, X1, X2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0x7FFF_FFFF_FFFF_FFFF); // Max positive
        cpu.set_x(2, 1);
        cpu.step().unwrap();
        assert!(cpu.get_v()); // Overflow flag set
        assert!(cpu.get_n()); // Result is negative
    }

    #[test]
    fn test_zero_register_as_source() {
        // ADD X0, XZR, #100 (XZR as source)
        // imm12 = 100 = 0x64, Rn = 31 (XZR), Rd = 0
        let insn = 0x910193E0; // ADD X0, XZR, #100
        let mut cpu = create_cpu_with_insn(insn);
        cpu.step().unwrap();
        assert_eq!(cpu.get_x(0), 100);
    }

    #[test]
    fn test_zero_register_as_dest() {
        // ADD XZR, X1, #100 (XZR as destination, discards result)
        let insn = 0x9119003F; // ADD XZR, X0, #100
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(0, 50);
        cpu.step().unwrap();
        // Result discarded, XZR still reads 0
        assert_eq!(cpu.get_x(31), 0);
    }

    #[test]
    fn test_32bit_operations() {
        // 32-bit operations should zero-extend
        let insn = 0x0B020020; // ADD W0, W1, W2
        let mut cpu = create_cpu_with_insn(insn);
        cpu.set_x(1, 0xFFFF_FFFF_0000_0001);
        cpu.set_x(2, 0xFFFF_FFFF_0000_0001);
        cpu.step().unwrap();
        // Result is 32-bit, zero-extended to 64
        assert_eq!(cpu.get_x(0), 2);
    }
}
