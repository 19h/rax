//! Local APIC (Local Advanced Programmable Interrupt Controller) emulation.
//!
//! The x86-64 Local APIC provides:
//! - Per-CPU interrupt handling
//! - Timer functionality (one-shot, periodic, TSC-deadline modes)
//! - Inter-Processor Interrupts (IPI)
//! - Local interrupt sources (LINT0, LINT1, error, thermal, PMC)
//!
//! MMIO region: 0xFEE00000 - 0xFEE00FFF (4KB)

use super::bus::MmioDevice;
use std::time::Instant;

/// LAPIC base address (fixed for x86-64)
pub const LAPIC_BASE: u64 = 0xFEE00000;
/// LAPIC region size
pub const LAPIC_SIZE: u64 = 0x1000;

// Register offsets (byte offsets from LAPIC_BASE)
const LAPIC_ID: u64 = 0x020;
const LAPIC_VERSION: u64 = 0x030;
const LAPIC_TPR: u64 = 0x080; // Task Priority Register
const LAPIC_APR: u64 = 0x090; // Arbitration Priority Register
const LAPIC_PPR: u64 = 0x0A0; // Processor Priority Register
const LAPIC_EOI: u64 = 0x0B0; // End of Interrupt
const LAPIC_RRD: u64 = 0x0C0; // Remote Read Register
const LAPIC_LDR: u64 = 0x0D0; // Logical Destination Register
const LAPIC_DFR: u64 = 0x0E0; // Destination Format Register
const LAPIC_SVR: u64 = 0x0F0; // Spurious Interrupt Vector Register
const LAPIC_ISR_BASE: u64 = 0x100; // In-Service Register (8 x 32-bit)
const LAPIC_TMR_BASE: u64 = 0x180; // Trigger Mode Register (8 x 32-bit)
const LAPIC_IRR_BASE: u64 = 0x200; // Interrupt Request Register (8 x 32-bit)
const LAPIC_ESR: u64 = 0x280; // Error Status Register
const LAPIC_ICR_LOW: u64 = 0x300; // Interrupt Command Register (low)
const LAPIC_ICR_HIGH: u64 = 0x310; // Interrupt Command Register (high)
const LAPIC_LVT_TIMER: u64 = 0x320; // LVT Timer Register
const LAPIC_LVT_THERMAL: u64 = 0x330; // LVT Thermal Sensor Register
const LAPIC_LVT_PMC: u64 = 0x340; // LVT Performance Counter Register
const LAPIC_LVT_LINT0: u64 = 0x350; // LVT LINT0 Register
const LAPIC_LVT_LINT1: u64 = 0x360; // LVT LINT1 Register
const LAPIC_LVT_ERROR: u64 = 0x370; // LVT Error Register
const LAPIC_TIMER_ICR: u64 = 0x380; // Timer Initial Count Register
const LAPIC_TIMER_CCR: u64 = 0x390; // Timer Current Count Register
const LAPIC_TIMER_DCR: u64 = 0x3E0; // Timer Divide Configuration Register

// LVT entry bits
const LVT_MASK: u32 = 1 << 16; // Interrupt masked
const LVT_TIMER_MODE_SHIFT: u32 = 17;
const LVT_TIMER_MODE_MASK: u32 = 0x3 << LVT_TIMER_MODE_SHIFT;

// Timer modes
const TIMER_MODE_ONESHOT: u32 = 0;
const TIMER_MODE_PERIODIC: u32 = 1;
const TIMER_MODE_TSC_DEADLINE: u32 = 2;

// SVR bits
const SVR_APIC_ENABLED: u32 = 1 << 8;

/// LAPIC timer frequency (approximate - 1 GHz bus clock / 16 = ~62.5 MHz base)
/// We'll use a simulated frequency based on real time
const LAPIC_TIMER_FREQ_HZ: u64 = 1_000_000_000; // 1 GHz base frequency

pub struct LocalApic {
    /// LAPIC ID (usually matches CPU ID)
    id: u32,
    /// LAPIC Version register
    version: u32,
    /// Task Priority Register
    tpr: u32,
    /// Logical Destination Register
    ldr: u32,
    /// Destination Format Register
    dfr: u32,
    /// Spurious Interrupt Vector Register
    svr: u32,
    /// In-Service Register (256 bits = 8 x 32-bit words)
    isr: [u32; 8],
    /// Trigger Mode Register (256 bits = 8 x 32-bit words)
    tmr: [u32; 8],
    /// Interrupt Request Register (256 bits = 8 x 32-bit words)
    irr: [u32; 8],
    /// Error Status Register
    esr: u32,
    /// Interrupt Command Register (64-bit)
    icr: u64,
    /// LVT Timer Register
    lvt_timer: u32,
    /// LVT Thermal Sensor Register
    lvt_thermal: u32,
    /// LVT Performance Counter Register
    lvt_pmc: u32,
    /// LVT LINT0 Register
    lvt_lint0: u32,
    /// LVT LINT1 Register
    lvt_lint1: u32,
    /// LVT Error Register
    lvt_error: u32,
    /// Timer Initial Count Register
    timer_initial_count: u32,
    /// Timer Current Count (computed dynamically)
    timer_current_count: u32,
    /// Timer Divide Configuration Register
    timer_divide_config: u32,
    /// Timestamp when timer was started
    timer_start: Option<Instant>,
    /// Pending timer interrupt
    timer_pending: bool,
}

impl LocalApic {
    pub fn new(apic_id: u32) -> Self {
        // Configure for virtual wire mode (what BIOS would normally do):
        // - APIC enabled (SVR bit 8 = 1)
        // - LINT0 configured for ExtInt mode (delivery mode 7 = external interrupt)
        // - LINT1 configured for NMI (delivery mode 4)
        //
        // LINT0 value: delivery_mode=7 (ExtInt), not masked = 0x700
        // LINT1 value: delivery_mode=4 (NMI), not masked = 0x400
        LocalApic {
            id: apic_id << 24, // ID is in bits 24-31
            // Version: bits 0-7 = version (0x14 = modern APIC)
            // bits 16-23 = max LVT entry (6 entries = 0x05)
            version: 0x00050014,
            tpr: 0,
            ldr: 0,
            dfr: 0xFFFFFFFF, // Flat model
            svr: 0x1FF, // APIC enabled (bit 8), spurious vector 0xFF
            isr: [0; 8],
            tmr: [0; 8],
            irr: [0; 8],
            esr: 0,
            icr: 0,
            lvt_timer: LVT_MASK, // Masked initially
            lvt_thermal: LVT_MASK,
            lvt_pmc: LVT_MASK,
            lvt_lint0: 0x700, // ExtInt mode, not masked - virtual wire mode
            lvt_lint1: 0x400, // NMI mode, not masked
            lvt_error: LVT_MASK,
            timer_initial_count: 0,
            timer_current_count: 0,
            timer_divide_config: 0,
            timer_start: None,
            timer_pending: false,
        }
    }

    /// Check if APIC is enabled
    pub fn is_enabled(&self) -> bool {
        (self.svr & SVR_APIC_ENABLED) != 0
    }

    /// Get the timer divisor from the divide configuration register
    fn timer_divisor(&self) -> u32 {
        // DCR bits 0-1 and bit 3 encode the divisor
        // 0b0000 = /2, 0b0001 = /4, 0b0010 = /8, 0b0011 = /16
        // 0b1000 = /32, 0b1001 = /64, 0b1010 = /128, 0b1011 = /1
        let bits = (self.timer_divide_config & 0x3) | ((self.timer_divide_config >> 1) & 0x4);
        match bits {
            0b000 => 2,
            0b001 => 4,
            0b010 => 8,
            0b011 => 16,
            0b100 => 32,
            0b101 => 64,
            0b110 => 128,
            0b111 => 1,
            _ => 1,
        }
    }

    /// Get the timer mode from LVT timer register
    fn timer_mode(&self) -> u32 {
        (self.lvt_timer & LVT_TIMER_MODE_MASK) >> LVT_TIMER_MODE_SHIFT
    }

    /// Check if timer is masked
    fn timer_masked(&self) -> bool {
        (self.lvt_timer & LVT_MASK) != 0
    }

    /// Get the timer vector
    pub fn timer_vector(&self) -> u8 {
        (self.lvt_timer & 0xFF) as u8
    }

    /// Tick the timer and return pending interrupt vector if any
    pub fn tick(&mut self) -> Option<u8> {
        if !self.is_enabled() || self.timer_initial_count == 0 || self.timer_masked() {
            return None;
        }

        let Some(start) = self.timer_start else {
            return None;
        };

        let elapsed = start.elapsed();
        let divisor = self.timer_divisor() as u64;
        let ticks_per_sec = LAPIC_TIMER_FREQ_HZ / divisor;

        // Calculate how many timer ticks have elapsed
        let elapsed_ticks = (elapsed.as_nanos() as u64 * ticks_per_sec) / 1_000_000_000;

        let initial = self.timer_initial_count as u64;
        let mode = self.timer_mode();

        match mode {
            TIMER_MODE_ONESHOT => {
                if elapsed_ticks >= initial {
                    // Timer expired
                    self.timer_current_count = 0;
                    self.timer_start = None; // Stop the timer
                    if !self.timer_pending {
                        self.timer_pending = true;
                        return Some(self.timer_vector());
                    }
                } else {
                    self.timer_current_count = (initial - elapsed_ticks) as u32;
                }
            }
            TIMER_MODE_PERIODIC => {
                // In periodic mode, timer restarts automatically
                let periods = elapsed_ticks / initial;
                let remainder = elapsed_ticks % initial;
                self.timer_current_count = (initial - remainder) as u32;

                // Generate interrupt for each period that passed
                if periods > 0 && !self.timer_pending {
                    self.timer_pending = true;
                    // Reset start time for next period
                    self.timer_start = Some(Instant::now());
                    return Some(self.timer_vector());
                }
            }
            TIMER_MODE_TSC_DEADLINE => {
                // TSC-deadline mode uses MSR, not implemented yet
            }
            _ => {}
        }

        None
    }

    /// Clear pending timer interrupt (called after injection)
    pub fn clear_timer_pending(&mut self) {
        self.timer_pending = false;
    }

    /// Check if there's a pending timer interrupt
    pub fn has_pending_timer(&self) -> bool {
        self.timer_pending
    }

    /// Get pending interrupt vector (highest priority)
    pub fn get_pending_vector(&self) -> Option<u8> {
        // Check IRR for pending interrupts
        for i in (0..8).rev() {
            if self.irr[i] != 0 {
                for bit in (0..32).rev() {
                    if self.irr[i] & (1 << bit) != 0 {
                        return Some((i * 32 + bit) as u8);
                    }
                }
            }
        }
        None
    }

    /// Set an interrupt as pending in IRR
    pub fn set_irr(&mut self, vector: u8) {
        let idx = (vector / 32) as usize;
        let bit = vector % 32;
        self.irr[idx] |= 1 << bit;
    }

    /// Acknowledge interrupt (move from IRR to ISR)
    pub fn ack_interrupt(&mut self, vector: u8) {
        let idx = (vector / 32) as usize;
        let bit = vector % 32;
        self.irr[idx] &= !(1 << bit);
        self.isr[idx] |= 1 << bit;
    }

    /// End of interrupt - clear highest priority ISR bit
    fn eoi(&mut self) {
        // Find highest priority in-service interrupt and clear it
        for i in (0..8).rev() {
            if self.isr[i] != 0 {
                for bit in (0..32).rev() {
                    if self.isr[i] & (1 << bit) != 0 {
                        self.isr[i] &= !(1 << bit);
                        return;
                    }
                }
            }
        }
    }

    fn read_register(&self, offset: u64) -> u32 {
        match offset {
            LAPIC_ID => self.id,
            LAPIC_VERSION => self.version,
            LAPIC_TPR => self.tpr,
            LAPIC_APR => 0, // Arbitration priority (read-only, usually 0)
            LAPIC_PPR => {
                // Processor priority = max(TPR, highest ISR priority)
                let tpr_class = (self.tpr >> 4) & 0xF;
                let mut max_isr = 0u32;
                for i in (0..8).rev() {
                    if self.isr[i] != 0 {
                        for bit in (0..32).rev() {
                            if self.isr[i] & (1 << bit) != 0 {
                                max_isr = ((i * 32 + bit) >> 4) as u32;
                                break;
                            }
                        }
                        break;
                    }
                }
                std::cmp::max(tpr_class, max_isr) << 4
            }
            LAPIC_RRD => 0,
            LAPIC_LDR => self.ldr,
            LAPIC_DFR => self.dfr,
            LAPIC_SVR => self.svr,
            o if o >= LAPIC_ISR_BASE && o < LAPIC_ISR_BASE + 0x80 => {
                let idx = ((o - LAPIC_ISR_BASE) / 0x10) as usize;
                if idx < 8 {
                    self.isr[idx]
                } else {
                    0
                }
            }
            o if o >= LAPIC_TMR_BASE && o < LAPIC_TMR_BASE + 0x80 => {
                let idx = ((o - LAPIC_TMR_BASE) / 0x10) as usize;
                if idx < 8 {
                    self.tmr[idx]
                } else {
                    0
                }
            }
            o if o >= LAPIC_IRR_BASE && o < LAPIC_IRR_BASE + 0x80 => {
                let idx = ((o - LAPIC_IRR_BASE) / 0x10) as usize;
                if idx < 8 {
                    self.irr[idx]
                } else {
                    0
                }
            }
            LAPIC_ESR => self.esr,
            LAPIC_ICR_LOW => self.icr as u32,
            LAPIC_ICR_HIGH => (self.icr >> 32) as u32,
            LAPIC_LVT_TIMER => self.lvt_timer,
            LAPIC_LVT_THERMAL => self.lvt_thermal,
            LAPIC_LVT_PMC => self.lvt_pmc,
            LAPIC_LVT_LINT0 => self.lvt_lint0,
            LAPIC_LVT_LINT1 => self.lvt_lint1,
            LAPIC_LVT_ERROR => self.lvt_error,
            LAPIC_TIMER_ICR => self.timer_initial_count,
            LAPIC_TIMER_CCR => {
                // Current count must be computed dynamically on every read
                // The kernel uses this for timer calibration
                if self.timer_initial_count == 0 {
                    0
                } else if let Some(start) = self.timer_start {
                    let elapsed = start.elapsed();
                    let divisor = self.timer_divisor() as u64;
                    let ticks_per_sec = LAPIC_TIMER_FREQ_HZ / divisor;

                    // Calculate how many timer ticks have elapsed
                    let elapsed_ticks = (elapsed.as_nanos() as u64 * ticks_per_sec) / 1_000_000_000;
                    let initial = self.timer_initial_count as u64;

                    let mode = self.timer_mode();
                    match mode {
                        TIMER_MODE_ONESHOT => {
                            if elapsed_ticks >= initial {
                                0
                            } else {
                                (initial - elapsed_ticks) as u32
                            }
                        }
                        TIMER_MODE_PERIODIC => {
                            // In periodic mode, wrap around
                            let remainder = elapsed_ticks % initial;
                            (initial - remainder) as u32
                        }
                        _ => 0, // TSC-deadline mode returns 0
                    }
                } else {
                    self.timer_current_count
                }
            }
            LAPIC_TIMER_DCR => self.timer_divide_config,
            _ => 0,
        }
    }

    fn write_register(&mut self, offset: u64, value: u32) {
        match offset {
            LAPIC_ID => {
                // ID is in bits 24-31
                self.id = value & 0xFF000000;
            }
            LAPIC_TPR => {
                self.tpr = value & 0xFF;
            }
            LAPIC_EOI => {
                // Any write triggers EOI
                self.eoi();
            }
            LAPIC_LDR => {
                self.ldr = value;
            }
            LAPIC_DFR => {
                self.dfr = value;
            }
            LAPIC_SVR => {
                let was_enabled = self.is_enabled();
                self.svr = value;
                if !was_enabled && self.is_enabled() {
                    // APIC just got enabled
                    tracing::debug!("LAPIC enabled, SVR={:#x}", value);
                }
            }
            LAPIC_ESR => {
                // Writing clears the ESR
                self.esr = 0;
            }
            LAPIC_ICR_LOW => {
                self.icr = (self.icr & 0xFFFFFFFF00000000) | (value as u64);
                // TODO: Handle IPI delivery
            }
            LAPIC_ICR_HIGH => {
                self.icr = (self.icr & 0x00000000FFFFFFFF) | ((value as u64) << 32);
            }
            LAPIC_LVT_TIMER => {
                self.lvt_timer = value;
                tracing::debug!(
                    "LVT Timer set: vector={:#x}, mode={}, masked={}",
                    value & 0xFF,
                    (value >> 17) & 0x3,
                    (value & LVT_MASK) != 0
                );
            }
            LAPIC_LVT_THERMAL => {
                self.lvt_thermal = value;
            }
            LAPIC_LVT_PMC => {
                self.lvt_pmc = value;
            }
            LAPIC_LVT_LINT0 => {
                self.lvt_lint0 = value;
            }
            LAPIC_LVT_LINT1 => {
                self.lvt_lint1 = value;
            }
            LAPIC_LVT_ERROR => {
                self.lvt_error = value;
            }
            LAPIC_TIMER_ICR => {
                self.timer_initial_count = value;
                self.timer_current_count = value;
                if value > 0 {
                    self.timer_start = Some(Instant::now());
                    self.timer_pending = false;
                    tracing::debug!(
                        "LAPIC timer started: initial_count={}, divisor={}, mode={}",
                        value,
                        self.timer_divisor(),
                        self.timer_mode()
                    );
                } else {
                    self.timer_start = None;
                }
            }
            LAPIC_TIMER_DCR => {
                self.timer_divide_config = value & 0xB; // Only bits 0,1,3 are valid
                tracing::debug!("LAPIC timer divisor set to {}", self.timer_divisor());
            }
            _ => {}
        }
    }
}

/// LAPIC MMIO device wrapper
pub struct LapicDevice {
    lapic: std::sync::Arc<std::sync::Mutex<LocalApic>>,
}

impl LapicDevice {
    pub fn new(lapic: std::sync::Arc<std::sync::Mutex<LocalApic>>) -> Self {
        LapicDevice { lapic }
    }
}

impl MmioDevice for LapicDevice {
    fn read(&mut self, addr: u64, data: &mut [u8]) {
        let offset = addr - LAPIC_BASE;
        // LAPIC registers are 32-bit aligned
        let aligned_offset = offset & !0x3;

        if let Ok(lapic) = self.lapic.lock() {
            let value = lapic.read_register(aligned_offset);
            // Handle different read sizes
            match data.len() {
                1 => {
                    let byte_offset = (offset & 0x3) as usize;
                    data[0] = ((value >> (byte_offset * 8)) & 0xFF) as u8;
                }
                2 => {
                    let byte_offset = (offset & 0x2) as usize;
                    let word = ((value >> (byte_offset * 8)) & 0xFFFF) as u16;
                    data[0] = (word & 0xFF) as u8;
                    data[1] = ((word >> 8) & 0xFF) as u8;
                }
                4 => {
                    data[0] = (value & 0xFF) as u8;
                    data[1] = ((value >> 8) & 0xFF) as u8;
                    data[2] = ((value >> 16) & 0xFF) as u8;
                    data[3] = ((value >> 24) & 0xFF) as u8;
                }
                _ => {
                    for byte in data.iter_mut() {
                        *byte = 0;
                    }
                }
            }
        } else {
            for byte in data.iter_mut() {
                *byte = 0xFF;
            }
        }
    }

    fn write(&mut self, addr: u64, data: &[u8]) {
        let offset = addr - LAPIC_BASE;
        let aligned_offset = offset & !0x3;

        // Reconstruct 32-bit value from data
        let value = match data.len() {
            1 => data[0] as u32,
            2 => (data[0] as u32) | ((data[1] as u32) << 8),
            4 => {
                (data[0] as u32)
                    | ((data[1] as u32) << 8)
                    | ((data[2] as u32) << 16)
                    | ((data[3] as u32) << 24)
            }
            _ => return,
        };

        if let Ok(mut lapic) = self.lapic.lock() {
            lapic.write_register(aligned_offset, value);
        }
    }
}
