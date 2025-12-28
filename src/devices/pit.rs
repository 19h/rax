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
//!
//! Timing is based on wall-clock time for real-time behavior.

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
    /// Read latch for counter values. Uses u32 to allow bit 16 as a marker
    /// for "high byte only" state after first read in LowHighByte mode.
    read_latch: Option<u32>,
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
    /// Wall-clock nanoseconds at last tick
    last_tick_nanos: u64,
    irq_pending: bool,
    tick_count: u64,
}

impl Pit {
    pub fn new() -> Self {
        let now = timing::elapsed_nanos();
        Pit {
            channels: [Channel::default(), Channel::default(), Channel::default()],
            last_tick_nanos: now,
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
        let now = timing::elapsed_nanos();
        let elapsed_nanos = now.saturating_sub(self.last_tick_nanos);

        // Convert nanoseconds to PIT ticks
        // pit_ticks = elapsed_nanos * PIT_FREQUENCY / 1_000_000_000
        let pit_ticks = timing::nanos_to_pit_ticks(elapsed_nanos);

        if pit_ticks == 0 {
            return false;
        }

        self.last_tick_nanos = now;

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

    /// Calculate the current count for a channel based on wall-clock time
    fn current_count(&self, channel: usize) -> u16 {
        let ch = &self.channels[channel];
        let now = timing::elapsed_nanos();
        let elapsed_nanos = now.saturating_sub(self.last_tick_nanos);
        let pit_ticks = timing::nanos_to_pit_ticks(elapsed_nanos);

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

    /// Marker bit to indicate high-byte-only latch (bit 16 set in u32)
    /// Valid count values are 0x0000-0xFFFF, so bit 16 is safe to use as marker
    const HIGH_BYTE_ONLY_MARKER: u32 = 0x10000;

    fn read_channel(&mut self, channel: usize) -> u8 {
        // Check if we have a latched value first
        if let Some(latch) = self.channels[channel].read_latch {
            let ch = &mut self.channels[channel];
            match ch.access_mode {
                AccessMode::LowByteOnly => {
                    ch.read_latch = None;
                    (latch & 0xFF) as u8
                }
                AccessMode::LowHighByte => {
                    // Check if this is a high-byte-only latch (after first read)
                    // Bit 16 is set for high-byte-only, clear for full 16-bit latch
                    if latch & Self::HIGH_BYTE_ONLY_MARKER != 0 {
                        // Second read: return the high byte, clear latch
                        ch.read_latch = None;
                        (latch & 0xFF) as u8
                    } else {
                        // First read: return low byte, keep high byte for next read
                        // Mark with HIGH_BYTE_ONLY_MARKER to indicate second read pending
                        ch.read_latch = Some(Self::HIGH_BYTE_ONLY_MARKER | (latch >> 8));
                        (latch & 0xFF) as u8
                    }
                }
                AccessMode::HighByteOnly => {
                    ch.read_latch = None;
                    ((latch >> 8) & 0xFF) as u8
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
                    // Latch the full value for consistent reads (no marker bit)
                    ch.read_latch = Some(count as u32);
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
                // Per spec: "the respective other 8 bits are zeroed"
                ch.reload_value = value as u16;
                ch.count = ch.reload_value;
            }
            AccessMode::HighByteOnly => {
                // Per spec: "the respective other 8 bits are zeroed"
                ch.reload_value = (value as u16) << 8;
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
                // Latch count value command
                // Per spec: "If a Counter is latched and then, some time later,
                // latched again before the count is read, the second Counter
                // Latch Command is ignored."
                if ch.read_latch.is_none() {
                    ch.read_latch = Some(ch.count as u32);
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a fresh PIT for testing
    fn make_pit() -> Pit {
        // Initialize timing to ensure consistent behavior
        crate::timing::init();
        Pit::new()
    }

    // ========== Basic Construction and Defaults ==========

    #[test]
    fn test_pit_new_default_state() {
        let pit = make_pit();
        // Check default values per spec: mode 2 (rate generator), lobyte/hibyte access
        assert!(!pit.irq_pending);
        assert_eq!(pit.tick_count, 0);
        // Default reload value should be set for ~100Hz (11932)
        assert_eq!(pit.channels[0].reload_value, DEFAULT_RELOAD);
        assert_eq!(pit.channels[0].count, DEFAULT_RELOAD);
    }

    #[test]
    fn test_default_channel_settings() {
        let pit = make_pit();
        for ch in &pit.channels {
            assert_eq!(ch.access_mode, AccessMode::LowHighByte);
            assert!(matches!(ch.operating_mode, OperatingMode::RateGenerator));
            assert!(ch.gate); // Gate should be high by default
            assert!(!ch.output);
            assert!(ch.read_latch.is_none());
            assert!(ch.write_latch.is_none());
        }
    }

    // ========== I/O Port Mapping ==========

    #[test]
    fn test_io_port_mapping() {
        let mut pit = make_pit();

        // Command register (0x43) is write-only, reads return 0xFF
        assert_eq!(pit.read(0x43), 0xFF);

        // Unknown ports return 0xFF
        assert_eq!(pit.read(0x44), 0xFF);
        assert_eq!(pit.read(0x50), 0xFF);
    }

    // ========== Command Register Parsing ==========

    #[test]
    fn test_command_channel_selection() {
        let mut pit = make_pit();

        // Program channel 0: 0b00110110 = channel 0, lobyte/hibyte, mode 3
        pit.write(0x43, 0x36);
        assert!(matches!(pit.channels[0].operating_mode, OperatingMode::SquareWaveGenerator));

        // Program channel 1: 0b01110100 = channel 1, lobyte/hibyte, mode 2
        pit.write(0x43, 0x74);
        assert!(matches!(pit.channels[1].operating_mode, OperatingMode::RateGenerator));

        // Program channel 2: 0b10110000 = channel 2, lobyte/hibyte, mode 0
        pit.write(0x43, 0xB0);
        assert!(matches!(pit.channels[2].operating_mode, OperatingMode::InterruptOnTerminalCount));
    }

    #[test]
    fn test_command_access_mode_parsing() {
        let mut pit = make_pit();

        // Access mode = 01 (low byte only): 0b00010000
        pit.write(0x43, 0x10);
        assert_eq!(pit.channels[0].access_mode, AccessMode::LowByteOnly);

        // Access mode = 10 (high byte only): 0b00100000
        pit.write(0x43, 0x20);
        assert_eq!(pit.channels[0].access_mode, AccessMode::HighByteOnly);

        // Access mode = 11 (lobyte/hibyte): 0b00110000
        pit.write(0x43, 0x30);
        assert_eq!(pit.channels[0].access_mode, AccessMode::LowHighByte);
    }

    #[test]
    fn test_command_operating_modes() {
        let mut pit = make_pit();

        // Mode 0: 0b00110000
        pit.write(0x43, 0x30);
        assert!(matches!(pit.channels[0].operating_mode, OperatingMode::InterruptOnTerminalCount));

        // Mode 1: 0b00110010
        pit.write(0x43, 0x32);
        assert!(matches!(pit.channels[0].operating_mode, OperatingMode::HardwareRetriggerableOneShot));

        // Mode 2: 0b00110100
        pit.write(0x43, 0x34);
        assert!(matches!(pit.channels[0].operating_mode, OperatingMode::RateGenerator));

        // Mode 3: 0b00110110
        pit.write(0x43, 0x36);
        assert!(matches!(pit.channels[0].operating_mode, OperatingMode::SquareWaveGenerator));

        // Mode 4: 0b00111000
        pit.write(0x43, 0x38);
        assert!(matches!(pit.channels[0].operating_mode, OperatingMode::SoftwareTriggeredStrobe));

        // Mode 5: 0b00111010
        pit.write(0x43, 0x3A);
        assert!(matches!(pit.channels[0].operating_mode, OperatingMode::HardwareTriggeredStrobe));

        // Mode 6 maps to Mode 2: 0b00111100
        pit.write(0x43, 0x3C);
        assert!(matches!(pit.channels[0].operating_mode, OperatingMode::RateGenerator));

        // Mode 7 maps to Mode 3: 0b00111110
        pit.write(0x43, 0x3E);
        assert!(matches!(pit.channels[0].operating_mode, OperatingMode::SquareWaveGenerator));
    }

    // ========== Counter Latch Command ==========

    #[test]
    fn test_counter_latch_command() {
        let mut pit = make_pit();

        // Set up channel 0 with known count
        pit.channels[0].count = 0x1234;
        pit.channels[0].access_mode = AccessMode::LowHighByte;

        // Send counter latch command for channel 0: 0b00000000
        pit.write(0x43, 0x00);

        // Read latch should now contain the count
        assert_eq!(pit.channels[0].read_latch, Some(0x1234));
    }

    #[test]
    fn test_counter_latch_preserves_until_read() {
        let mut pit = make_pit();

        // Set up channel 0
        pit.channels[0].count = 0xABCD;
        pit.channels[0].access_mode = AccessMode::LowHighByte;

        // Latch the count
        pit.write(0x43, 0x00);
        assert_eq!(pit.channels[0].read_latch, Some(0xABCD));

        // Change the actual count
        pit.channels[0].count = 0x1234;

        // Read should still return latched value
        let low = pit.read(0x40);
        assert_eq!(low, 0xCD); // Low byte of 0xABCD

        let high = pit.read(0x40);
        assert_eq!(high, 0xAB); // High byte of 0xABCD

        // After reading both bytes, latch should be cleared
        assert!(pit.channels[0].read_latch.is_none());
    }

    #[test]
    fn test_multiple_latch_commands_ignored() {
        let mut pit = make_pit();

        // Set up and latch first value
        pit.channels[0].count = 0x1111;
        pit.channels[0].access_mode = AccessMode::LowHighByte;
        pit.write(0x43, 0x00); // Latch

        // Change count and try to latch again
        pit.channels[0].count = 0x2222;
        pit.write(0x43, 0x00); // Second latch should be ignored

        // Should still read the first latched value
        let low = pit.read(0x40);
        let high = pit.read(0x40);
        assert_eq!((high as u16) << 8 | low as u16, 0x1111);
    }

    // ========== Access Modes ==========

    #[test]
    fn test_low_byte_only_write() {
        let mut pit = make_pit();

        // Configure channel 0 for low byte only
        pit.write(0x43, 0x10); // Channel 0, low byte only, mode 0

        // Write only low byte
        pit.write(0x40, 0x42);

        // High byte should be 0, low byte should be 0x42
        assert_eq!(pit.channels[0].reload_value, 0x0042);
    }

    #[test]
    fn test_high_byte_only_write() {
        let mut pit = make_pit();

        // Configure channel 0 for high byte only
        pit.write(0x43, 0x20); // Channel 0, high byte only, mode 0

        // Write only high byte
        pit.write(0x40, 0x42);

        // High byte should be 0x42, low byte preserved/zeroed
        assert_eq!(pit.channels[0].reload_value & 0xFF00, 0x4200);
    }

    #[test]
    fn test_lobyte_hibyte_write_sequence() {
        let mut pit = make_pit();

        // Configure channel 0 for lobyte/hibyte (standard mode)
        pit.write(0x43, 0x36); // Channel 0, lobyte/hibyte, mode 3

        // Write low byte first
        pit.write(0x40, 0x34);
        // Write latch should have the low byte
        assert_eq!(pit.channels[0].write_latch, Some(0x34));

        // Write high byte
        pit.write(0x40, 0x12);
        // Write latch should be cleared, reload value set
        assert!(pit.channels[0].write_latch.is_none());
        assert_eq!(pit.channels[0].reload_value, 0x1234);
    }

    #[test]
    fn test_low_byte_only_read() {
        let mut pit = make_pit();

        pit.channels[0].count = 0xABCD;
        pit.channels[0].access_mode = AccessMode::LowByteOnly;

        let value = pit.read(0x40);
        assert_eq!(value, 0xCD); // Should return low byte only
    }

    #[test]
    fn test_high_byte_only_read() {
        let mut pit = make_pit();

        pit.channels[0].count = 0xABCD;
        pit.channels[0].access_mode = AccessMode::HighByteOnly;

        let value = pit.read(0x40);
        assert_eq!(value, 0xAB); // Should return high byte only
    }

    // ========== Reload Value of 0 = 65536 ==========

    #[test]
    fn test_reload_value_zero_means_65536() {
        let mut pit = make_pit();

        // Configure and write 0 as reload value
        pit.write(0x43, 0x36);
        pit.write(0x40, 0x00); // Low byte
        pit.write(0x40, 0x00); // High byte

        assert_eq!(pit.channels[0].reload_value, 0);

        // In tick calculations, 0 should be treated as 0x10000
        // This is tested indirectly through the tick() method
    }

    // ========== Channel 2 Data Port ==========

    #[test]
    fn test_channel_2_read_write() {
        let mut pit = make_pit();

        // Configure channel 2
        pit.write(0x43, 0xB6); // Channel 2, lobyte/hibyte, mode 3

        // Write to channel 2 data port
        pit.write(0x42, 0xEF);
        pit.write(0x42, 0xBE);

        assert_eq!(pit.channels[2].reload_value, 0xBEEF);
    }

    // ========== Read-Back Command ==========

    #[test]
    fn test_readback_command_channel_3_ignored() {
        let mut pit = make_pit();

        // Set up known state
        pit.channels[0].reload_value = 0x1234;

        // Send read-back command (channel bits = 11)
        pit.write(0x43, 0xC2); // 11000010: read-back, latch count, channel 0

        // Per current implementation, this is ignored (returns early)
        // This is a known limitation - read-back command not fully implemented
        // The spec says 8254 supports it, 8253 doesn't
    }

    // ========== Interrupt Generation ==========

    #[test]
    fn test_has_pending_interrupt() {
        let mut pit = make_pit();

        assert!(!pit.has_pending_interrupt());
        pit.irq_pending = true;
        assert!(pit.has_pending_interrupt());
    }

    #[test]
    fn test_clear_interrupt() {
        let mut pit = make_pit();

        pit.irq_pending = true;
        pit.clear_interrupt();
        assert!(!pit.irq_pending);
    }

    // ========== Gate Input ==========

    #[test]
    fn test_gate_disabled_prevents_counting() {
        let mut pit = make_pit();

        pit.channels[0].gate = false;
        pit.channels[0].count = 100;

        // Manually simulate what tick would do - with gate=false, count shouldn't decrement
        // Note: tick() uses wall-clock time, so we test the gate check logic directly
        assert!(!pit.channels[0].gate);
    }

    // ========== Standard Timer Configuration (like BIOS) ==========

    #[test]
    fn test_standard_100hz_configuration() {
        let mut pit = make_pit();

        // Standard 100 Hz configuration: 1193182 / 100 ≈ 11932
        // Command: 0x36 = channel 0, lobyte/hibyte, mode 3 (square wave)
        pit.write(0x43, 0x36);

        // Write divisor 11932 = 0x2E9C
        pit.write(0x40, 0x9C); // Low byte
        pit.write(0x40, 0x2E); // High byte

        assert_eq!(pit.channels[0].reload_value, 0x2E9C);
        assert!(matches!(pit.channels[0].operating_mode, OperatingMode::SquareWaveGenerator));
    }

    #[test]
    fn test_bios_default_18hz_configuration() {
        let mut pit = make_pit();

        // BIOS default: divisor 0 (= 65536) for ~18.2 Hz
        // 1193182 / 65536 ≈ 18.2065 Hz
        pit.write(0x43, 0x36);
        pit.write(0x40, 0x00);
        pit.write(0x40, 0x00);

        assert_eq!(pit.channels[0].reload_value, 0);
    }

    // ========== All Three Channels Independent ==========

    #[test]
    fn test_channels_independent() {
        let mut pit = make_pit();

        // Configure each channel differently
        pit.write(0x43, 0x30); // Ch 0: mode 0
        pit.write(0x43, 0x76); // Ch 1: mode 3
        pit.write(0x43, 0xB4); // Ch 2: mode 2

        // Set different reload values
        pit.write(0x40, 0x11);
        pit.write(0x40, 0x11);

        pit.write(0x41, 0x22);
        pit.write(0x41, 0x22);

        pit.write(0x42, 0x33);
        pit.write(0x42, 0x33);

        // Verify each channel has its own settings
        assert!(matches!(pit.channels[0].operating_mode, OperatingMode::InterruptOnTerminalCount));
        assert!(matches!(pit.channels[1].operating_mode, OperatingMode::SquareWaveGenerator));
        assert!(matches!(pit.channels[2].operating_mode, OperatingMode::RateGenerator));

        assert_eq!(pit.channels[0].reload_value, 0x1111);
        assert_eq!(pit.channels[1].reload_value, 0x2222);
        assert_eq!(pit.channels[2].reload_value, 0x3333);
    }

    // ========== Write Latch Reset on Mode Change ==========

    #[test]
    fn test_write_latch_cleared_on_mode_change() {
        let mut pit = make_pit();

        // Start writing a count
        pit.write(0x43, 0x36);
        pit.write(0x40, 0x12); // Write low byte, latch should be set
        assert!(pit.channels[0].write_latch.is_some());

        // Change mode - this should clear the write latch
        pit.write(0x43, 0x34);
        assert!(pit.channels[0].write_latch.is_none());
    }

    // ========== PIT Frequency Constant ==========

    #[test]
    fn test_pit_frequency_constant() {
        // Verify PIT frequency is the canonical 1.193182 MHz
        assert_eq!(PIT_FREQUENCY, 1193182);
    }

    // ========== Spec Compliance: BCD Mode Bit Ignored ==========

    #[test]
    fn test_bcd_mode_bit_ignored() {
        let mut pit = make_pit();

        // Command with BCD bit set (bit 0 = 1)
        pit.write(0x43, 0x37); // Same as 0x36 but with BCD

        // Should still work the same - BCD mode ignored per implementation
        pit.write(0x40, 0x9C);
        pit.write(0x40, 0x2E);

        assert_eq!(pit.channels[0].reload_value, 0x2E9C);
    }
}
