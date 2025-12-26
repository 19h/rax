//! Intel 8259 Programmable Interrupt Controller (PIC) emulation.
//!
//! The x86 architecture traditionally uses two cascaded PICs:
//! - Master PIC at I/O ports 0x20-0x21 (handles IRQ 0-7)
//! - Slave PIC at I/O ports 0xA0-0xA1 (handles IRQ 8-15)
//!
//! IRQ 2 on the master is connected to the slave's output.

use super::bus::IoDevice;

#[derive(Clone, Copy, Debug, PartialEq)]
enum InitState {
    Ready,
    WaitingICW2,
    WaitingICW3,
    WaitingICW4,
}

#[derive(Clone)]
struct Pic8259 {
    /// Interrupt Request Register - pending interrupts
    irr: u8,
    /// In-Service Register - currently being serviced
    isr: u8,
    /// Interrupt Mask Register
    imr: u8,
    /// Interrupt vector offset (set by ICW2)
    vector_offset: u8,
    /// Initialization state machine
    init_state: InitState,
    /// ICW1 received
    icw1: u8,
    /// Whether ICW4 is needed
    icw4_needed: bool,
    /// Auto EOI mode
    auto_eoi: bool,
    /// Special fully nested mode
    special_fully_nested: bool,
    /// Read IRR (true) or ISR (false) on next read
    read_irr: bool,
    /// Lowest priority IRQ for rotation
    lowest_priority: u8,
}

impl Pic8259 {
    fn new(vector_offset: u8) -> Self {
        Pic8259 {
            irr: 0,
            isr: 0,
            imr: 0xFF, // All interrupts masked initially
            vector_offset,
            init_state: InitState::Ready,
            icw1: 0,
            icw4_needed: false,
            auto_eoi: false,
            special_fully_nested: false,
            read_irr: true,
            lowest_priority: 7,
        }
    }

    /// Set an IRQ line (edge-triggered)
    fn set_irq(&mut self, irq: u8, level: bool) {
        if irq > 7 {
            return;
        }
        if level {
            self.irr |= 1 << irq;
        }
        // Note: for edge-triggered, we don't clear on level=false
    }

    /// Get the highest priority pending interrupt
    fn get_pending_irq(&self) -> Option<u8> {
        let pending = self.irr & !self.imr;
        if pending == 0 {
            return None;
        }
        // Find highest priority (lowest number) pending interrupt
        for i in 0..8 {
            if pending & (1 << i) != 0 {
                // Don't deliver an interrupt that's already being serviced
                // (waiting for EOI)
                if self.isr & (1 << i) != 0 {
                    continue;
                }
                // Check if a higher priority interrupt is already being serviced
                let higher_priority_mask = (1 << i) - 1;
                if self.isr & higher_priority_mask != 0 {
                    return None;
                }
                return Some(i);
            }
        }
        None
    }

    /// Acknowledge an interrupt - move from IRR to ISR
    fn ack_irq(&mut self, irq: u8) -> u8 {
        self.irr &= !(1 << irq);
        self.isr |= 1 << irq;
        if self.auto_eoi {
            self.isr &= !(1 << irq);
        }
        let vector = self.vector_offset + irq;
        tracing::debug!(
            "PIC ack_irq: irq={}, vector={:#x}, auto_eoi={}, isr={:#x}",
            irq, vector, self.auto_eoi, self.isr
        );
        vector
    }

    /// End of interrupt
    fn eoi(&mut self, specific: Option<u8>) {
        if let Some(irq) = specific {
            self.isr &= !(1 << irq);
        } else {
            // Non-specific EOI - clear highest priority in-service bit
            for i in 0..8 {
                if self.isr & (1 << i) != 0 {
                    self.isr &= !(1 << i);
                    break;
                }
            }
        }
    }

    fn write_command(&mut self, value: u8) {
        if value & 0x10 != 0 {
            // ICW1 - start initialization sequence
            tracing::info!(
                "PIC ICW1: value={:#x}, icw4_needed={}, cascade={}",
                value,
                value & 0x01 != 0,
                value & 0x02 == 0
            );
            self.icw1 = value;
            self.icw4_needed = value & 0x01 != 0;
            self.init_state = InitState::WaitingICW2;
            self.imr = 0;
            self.isr = 0;
            self.irr = 0;
            self.auto_eoi = false;
            self.read_irr = true;
        } else if value & 0x08 != 0 {
            // OCW3
            if value & 0x02 != 0 {
                self.read_irr = value & 0x01 != 0;
            }
        } else {
            // OCW2
            let eoi_type = (value >> 5) & 0x07;
            match eoi_type {
                0b001 => {
                    tracing::debug!("PIC OCW2: non-specific EOI, isr={:#x}", self.isr);
                    self.eoi(None);
                }
                0b011 => {
                    // Specific EOI
                    let irq = value & 0x07;
                    tracing::debug!("PIC OCW2: specific EOI irq={}", irq);
                    self.eoi(Some(irq));
                }
                0b101 => {
                    // Rotate on non-specific EOI
                    self.eoi(None);
                }
                0b111 => {
                    // Rotate on specific EOI
                    let irq = value & 0x07;
                    self.eoi(Some(irq));
                    self.lowest_priority = irq;
                }
                _ => {}
            }
        }
    }

    fn write_data(&mut self, value: u8) {
        match self.init_state {
            InitState::WaitingICW2 => {
                self.vector_offset = value & 0xF8;
                tracing::info!("PIC ICW2: vector_offset set to {:#x}", self.vector_offset);
                if self.icw1 & 0x02 != 0 {
                    // Single mode - skip ICW3
                    self.init_state = if self.icw4_needed {
                        InitState::WaitingICW4
                    } else {
                        InitState::Ready
                    };
                } else {
                    self.init_state = InitState::WaitingICW3;
                }
            }
            InitState::WaitingICW3 => {
                // ICW3 - cascade configuration (ignored for now)
                self.init_state = if self.icw4_needed {
                    InitState::WaitingICW4
                } else {
                    InitState::Ready
                };
            }
            InitState::WaitingICW4 => {
                // ICW4
                self.auto_eoi = value & 0x02 != 0;
                self.special_fully_nested = value & 0x10 != 0;
                self.init_state = InitState::Ready;
            }
            InitState::Ready => {
                // OCW1 - set interrupt mask
                self.imr = value;
            }
        }
    }

    fn read_command(&self) -> u8 {
        if self.read_irr {
            self.irr
        } else {
            self.isr
        }
    }

    fn read_data(&self) -> u8 {
        self.imr
    }
}

pub struct DualPic {
    master: Pic8259,
    slave: Pic8259,
}

impl DualPic {
    pub fn new() -> Self {
        // Use standard x86 protected mode vector offsets:
        // Master PIC: vectors 0x20-0x27 (IRQ 0-7)
        // Slave PIC: vectors 0x28-0x2F (IRQ 8-15)
        // Also unmask IRQ0 (timer) and IRQ2 (cascade) for virtual wire mode
        let mut master = Pic8259::new(0x20);
        master.imr = 0xFB; // Unmask IRQ2 (cascade) - bit 2 = 0
        master.imr = 0x00; // Unmask all for now (will be masked by kernel if needed)
        let mut slave = Pic8259::new(0x28);
        slave.imr = 0x00; // Unmask all
        DualPic { master, slave }
    }

    /// Set an IRQ line (0-15)
    pub fn set_irq(&mut self, irq: u8, level: bool) {
        if irq < 8 {
            self.master.set_irq(irq, level);
        } else if irq < 16 {
            self.slave.set_irq(irq - 8, level);
            // Cascade: slave output connected to master IRQ 2
            if self.slave.get_pending_irq().is_some() {
                self.master.set_irq(2, true);
            }
        }
    }

    /// Get the highest priority pending interrupt vector
    pub fn get_pending_vector(&mut self) -> Option<u8> {
        if let Some(master_irq) = self.master.get_pending_irq() {
            if master_irq == 2 {
                // Cascade - get from slave
                if let Some(slave_irq) = self.slave.get_pending_irq() {
                    let vector = self.slave.ack_irq(slave_irq);
                    self.master.ack_irq(2);
                    return Some(vector);
                }
            }
            let vector = self.master.ack_irq(master_irq);
            return Some(vector);
        }
        None
    }

    /// Check if any interrupt is pending
    pub fn has_pending(&self) -> bool {
        self.master.get_pending_irq().is_some()
    }
}

/// Master PIC I/O device (ports 0x20-0x21)
pub struct MasterPicDevice {
    pic: std::sync::Arc<std::sync::Mutex<DualPic>>,
}

impl MasterPicDevice {
    pub fn new(pic: std::sync::Arc<std::sync::Mutex<DualPic>>) -> Self {
        MasterPicDevice { pic }
    }
}

impl IoDevice for MasterPicDevice {
    fn read(&mut self, port: u16) -> u8 {
        if let Ok(pic) = self.pic.lock() {
            match port {
                0x20 => pic.master.read_command(),
                0x21 => pic.master.read_data(),
                _ => 0xFF,
            }
        } else {
            0xFF
        }
    }

    fn write(&mut self, port: u16, value: u8) {
        if let Ok(mut pic) = self.pic.lock() {
            match port {
                0x20 => pic.master.write_command(value),
                0x21 => pic.master.write_data(value),
                _ => {}
            }
        }
    }
}

/// Slave PIC I/O device (ports 0xA0-0xA1)
pub struct SlavePicDevice {
    pic: std::sync::Arc<std::sync::Mutex<DualPic>>,
}

impl SlavePicDevice {
    pub fn new(pic: std::sync::Arc<std::sync::Mutex<DualPic>>) -> Self {
        SlavePicDevice { pic }
    }
}

impl IoDevice for SlavePicDevice {
    fn read(&mut self, port: u16) -> u8 {
        if let Ok(pic) = self.pic.lock() {
            match port {
                0xA0 => pic.slave.read_command(),
                0xA1 => pic.slave.read_data(),
                _ => 0xFF,
            }
        } else {
            0xFF
        }
    }

    fn write(&mut self, port: u16, value: u8) {
        if let Ok(mut pic) = self.pic.lock() {
            match port {
                0xA0 => pic.slave.write_command(value),
                0xA1 => pic.slave.write_data(value),
                _ => {}
            }
        }
    }
}
