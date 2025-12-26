//! Intel 8253/8254 Programmable Interval Timer (PIT) emulation.
//!
//! The PIT provides three independent 16-bit counters (channels 0-2):
//! - Channel 0: System timer, generates IRQ 0
//! - Channel 1: Originally DRAM refresh (not used in modern systems)
//! - Channel 2: PC speaker
//!
//! I/O ports:
//! - 0x40: Channel 0 data
//! - 0x41: Channel 1 data
//! - 0x42: Channel 2 data
//! - 0x43: Mode/Command register

use crate::timing;

use super::bus::IoDevice;

/// PIT oscillator frequency (1.193182 MHz)
const PIT_FREQUENCY: u64 = timing::PIT_FREQUENCY_HZ;

/// Default reload value for ~100 Hz (10ms period)
const DEFAULT_RELOAD: u16 = 11932;

#[derive(Clone, Copy, Debug, PartialEq)]
enum AccessMode {
    LatchCount,
    LowByteOnly,
    HighByteOnly,
    LowHighByte,
}

#[derive(Clone, Copy, Debug)]
enum OperatingMode {
    InterruptOnTerminalCount,  // Mode 0
    HardwareRetriggerableOneShot, // Mode 1
    RateGenerator,             // Mode 2
    SquareWaveGenerator,       // Mode 3
    SoftwareTriggeredStrobe,   // Mode 4
    HardwareTriggeredStrobe,   // Mode 5
}

#[derive(Clone)]
struct Channel {
    reload_value: u16,
    count: u16,
    access_mode: AccessMode,
    operating_mode: OperatingMode,
    read_latch: Option<u16>,
    write_latch: Option<u8>,
    gate: bool,
    output: bool,
}

impl Default for Channel {
    fn default() -> Self {
        Channel {
            reload_value: DEFAULT_RELOAD,
            count: DEFAULT_RELOAD,
            access_mode: AccessMode::LowHighByte,
            operating_mode: OperatingMode::RateGenerator,
            read_latch: None,
            write_latch: None,
            gate: true,
            output: false,
        }
    }
}

pub struct Pit {
    channels: [Channel; 3],
    /// Instruction count at start
    start_insn: u64,
    /// Instruction count at last tick
    last_tick_insn: u64,
    irq_pending: bool,
    tick_count: u64,
}

impl Pit {
    pub fn new() -> Self {
        let now = timing::current();
        Pit {
            channels: [Channel::default(), Channel::default(), Channel::default()],
            start_insn: now,
            last_tick_insn: now,
            irq_pending: false,
            tick_count: 0,
        }
    }

    /// Check if a timer interrupt is pending
    pub fn has_pending_interrupt(&self) -> bool {
        self.irq_pending
    }

    /// Clear the pending interrupt
    pub fn clear_interrupt(&mut self) {
        self.irq_pending = false;
    }

    /// Tick the timer - should be called periodically
    /// Returns true if an interrupt should be generated
    pub fn tick(&mut self) -> bool {
        let now = timing::current();
        let elapsed_insn = now.saturating_sub(self.last_tick_insn);

        // Convert instruction count to PIT ticks
        // At 3 GHz CPU, PIT runs at 1.193182 MHz
        // pit_ticks = elapsed_insn * PIT_FREQUENCY / CPU_FREQUENCY
        //           = elapsed_insn * 1193182 / 3000000000
        //           ≈ elapsed_insn / 2514
        // To avoid division by zero and get more precision:
        let pit_ticks = (elapsed_insn * PIT_FREQUENCY) / timing::CPU_FREQUENCY_HZ;

        if pit_ticks == 0 {
            return false;
        }

        self.last_tick_insn = now;

        // Update channel 0 (system timer)
        let ch0 = &mut self.channels[0];
        if ch0.gate {
            let reload = if ch0.reload_value == 0 { 0x10000u32 } else { ch0.reload_value as u32 };

            match ch0.operating_mode {
                OperatingMode::RateGenerator | OperatingMode::SquareWaveGenerator => {
                    // Count down and reload
                    let ticks = pit_ticks as u32;
                    if ticks >= ch0.count as u32 {
                        // Timer expired - generate interrupt
                        let remaining = ticks - ch0.count as u32;
                        ch0.count = (reload - (remaining % reload)) as u16;
                        self.irq_pending = true;
                        self.tick_count += 1;
                        return true;
                    } else {
                        ch0.count = ch0.count.wrapping_sub(pit_ticks as u16);
                    }
                }
                _ => {
                    // Other modes - simplified handling
                    ch0.count = ch0.count.wrapping_sub(pit_ticks as u16);
                    if ch0.count == 0 {
                        self.irq_pending = true;
                        self.tick_count += 1;
                        ch0.count = ch0.reload_value;
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Calculate the current count for a channel based on instruction count
    fn current_count(&self, channel: usize) -> u16 {
        let ch = &self.channels[channel];
        let now = timing::current();
        let elapsed_insn = now.saturating_sub(self.last_tick_insn);
        let pit_ticks = (elapsed_insn * PIT_FREQUENCY) / timing::CPU_FREQUENCY_HZ;

        if pit_ticks == 0 {
            return ch.count;
        }

        let reload = if ch.reload_value == 0 { 0x10000u32 } else { ch.reload_value as u32 };

        match ch.operating_mode {
            OperatingMode::RateGenerator | OperatingMode::SquareWaveGenerator => {
                // Count down continuously with wrap-around at reload value
                let total_ticks = pit_ticks as u32;
                if total_ticks >= ch.count as u32 {
                    let remaining = (total_ticks - ch.count as u32) % reload;
                    (reload - remaining) as u16
                } else {
                    ch.count.wrapping_sub(pit_ticks as u16)
                }
            }
            _ => {
                // Simple countdown
                ch.count.saturating_sub(pit_ticks as u16)
            }
        }
    }

    fn read_channel(&mut self, channel: usize) -> u8 {
        // Check if we have a latched value first
        if let Some(latch) = self.channels[channel].read_latch {
            let ch = &mut self.channels[channel];
            match ch.access_mode {
                AccessMode::LowByteOnly | AccessMode::LowHighByte => {
                    if ch.access_mode == AccessMode::LowHighByte {
                        // Return low byte, keep high byte for next read
                        ch.read_latch = Some(latch >> 8);
                    } else {
                        ch.read_latch = None;
                    }
                    (latch & 0xFF) as u8
                }
                AccessMode::HighByteOnly => {
                    ch.read_latch = None;
                    (latch >> 8) as u8
                }
                AccessMode::LatchCount => {
                    ch.read_latch = None;
                    (latch & 0xFF) as u8
                }
            }
        } else {
            // Calculate current count based on wall-clock time
            let count = self.current_count(channel);
            let ch = &mut self.channels[channel];
            match ch.access_mode {
                AccessMode::LowByteOnly => (count & 0xFF) as u8,
                AccessMode::HighByteOnly => (count >> 8) as u8,
                AccessMode::LowHighByte => {
                    // Latch the value and return low byte
                    ch.read_latch = Some(count >> 8);
                    (count & 0xFF) as u8
                }
                AccessMode::LatchCount => (count & 0xFF) as u8,
            }
        }
    }

    fn write_channel(&mut self, channel: usize, value: u8) {
        let ch = &mut self.channels[channel];

        match ch.access_mode {
            AccessMode::LowByteOnly => {
                ch.reload_value = (ch.reload_value & 0xFF00) | (value as u16);
                ch.count = ch.reload_value;
            }
            AccessMode::HighByteOnly => {
                ch.reload_value = (ch.reload_value & 0x00FF) | ((value as u16) << 8);
                ch.count = ch.reload_value;
            }
            AccessMode::LowHighByte => {
                if let Some(low) = ch.write_latch {
                    // Second byte (high)
                    ch.reload_value = (low as u16) | ((value as u16) << 8);
                    ch.count = ch.reload_value;
                    ch.write_latch = None;
                } else {
                    // First byte (low)
                    ch.write_latch = Some(value);
                }
            }
            AccessMode::LatchCount => {
                // Latch mode doesn't accept writes
            }
        }
    }

    fn write_command(&mut self, value: u8) {
        let channel = ((value >> 6) & 0x03) as usize;
        let access = (value >> 4) & 0x03;
        let mode = (value >> 1) & 0x07;
        let bcd = value & 0x01;

        // BCD mode not supported, just ignore
        let _ = bcd;

        if channel == 3 {
            // Read-back command (8254 only) - simplified
            return;
        }

        let ch = &mut self.channels[channel];

        ch.access_mode = match access {
            0 => {
                // Latch count value
                ch.read_latch = Some(ch.count);
                return;
            }
            1 => AccessMode::LowByteOnly,
            2 => AccessMode::HighByteOnly,
            3 => AccessMode::LowHighByte,
            _ => unreachable!(),
        };

        ch.operating_mode = match mode {
            0 => OperatingMode::InterruptOnTerminalCount,
            1 => OperatingMode::HardwareRetriggerableOneShot,
            2 | 6 => OperatingMode::RateGenerator,
            3 | 7 => OperatingMode::SquareWaveGenerator,
            4 => OperatingMode::SoftwareTriggeredStrobe,
            5 => OperatingMode::HardwareTriggeredStrobe,
            _ => unreachable!(),
        };

        ch.write_latch = None;
    }
}

impl IoDevice for Pit {
    fn read(&mut self, port: u16) -> u8 {
        match port {
            0x40 => self.read_channel(0),
            0x41 => self.read_channel(1),
            0x42 => self.read_channel(2),
            0x43 => 0xFF, // Command register is write-only
            _ => 0xFF,
        }
    }

    fn write(&mut self, port: u16, value: u8) {
        match port {
            0x40 => self.write_channel(0, value),
            0x41 => self.write_channel(1, value),
            0x42 => self.write_channel(2, value),
            0x43 => self.write_command(value),
            _ => {}
        }
    }
}
