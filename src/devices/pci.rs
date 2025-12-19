//! Stub PCI configuration space handler
//! Returns 0xff for all reads (no devices present)

use super::bus::IoDevice;

/// PCI configuration space ports
pub const PCI_CONFIG_ADDRESS: u16 = 0xcf8;
pub const PCI_CONFIG_DATA: u16 = 0xcfc;

/// Stub PCI device that returns no devices present
pub struct PciStub {
    address: u32,
}

impl PciStub {
    pub fn new() -> Self {
        PciStub { address: 0 }
    }
}

impl IoDevice for PciStub {
    fn read(&mut self, port: u16) -> u8 {
        match port {
            PCI_CONFIG_ADDRESS..=0xcfb => {
                let offset = (port - PCI_CONFIG_ADDRESS) as usize;
                ((self.address >> (offset * 8)) & 0xff) as u8
            }
            PCI_CONFIG_DATA..=0xcff => {
                // Return 0xff = no device present
                0xff
            }
            _ => 0xff,
        }
    }

    fn write(&mut self, port: u16, value: u8) {
        match port {
            PCI_CONFIG_ADDRESS..=0xcfb => {
                let offset = (port - PCI_CONFIG_ADDRESS) as usize;
                let mask = !(0xffu32 << (offset * 8));
                self.address = (self.address & mask) | ((value as u32) << (offset * 8));
            }
            PCI_CONFIG_DATA..=0xcff => {
                // Ignore writes to config data
            }
            _ => {}
        }
    }
}
