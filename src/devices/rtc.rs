//! CMOS/RTC stub device
//! Ports 0x70-0x71

use super::bus::IoDevice;

/// CMOS/RTC ports
pub const RTC_ADDRESS: u16 = 0x70;
pub const RTC_DATA: u16 = 0x71;

/// Stub RTC device that returns zeros
pub struct RtcStub {
    address: u8,
}

impl RtcStub {
    pub fn new() -> Self {
        RtcStub { address: 0 }
    }
}

impl IoDevice for RtcStub {
    fn read(&mut self, port: u16) -> u8 {
        match port {
            RTC_ADDRESS => self.address,
            RTC_DATA => {
                // Return some reasonable defaults
                match self.address {
                    0x0a => 0x26, // Status register A - update not in progress
                    0x0b => 0x02, // Status register B - 24h mode
                    0x0c => 0x00, // Status register C
                    0x0d => 0x80, // Status register D - RTC valid
                    _ => 0,
                }
            }
            _ => 0,
        }
    }

    fn write(&mut self, port: u16, value: u8) {
        match port {
            RTC_ADDRESS => self.address = value & 0x7f, // Mask NMI disable bit
            RTC_DATA => {
                // Ignore writes
            }
            _ => {}
        }
    }
}
