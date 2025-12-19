use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use crate::devices::bus::IoDevice;

const DATA_REG: u16 = 0;
const IER_REG: u16 = 1;
const IIR_REG: u16 = 2;
const LCR_REG: u16 = 3;
const MCR_REG: u16 = 4;
const LSR_REG: u16 = 5;
const MSR_REG: u16 = 6;
const SCR_REG: u16 = 7;

// LSR bits
const LSR_DATA_READY: u8 = 0x01;
const LSR_THRE: u8 = 0x20;
const LSR_TEMT: u8 = 0x40;

// State for filtering cursor position responses (ESC[n;nR) on input
// and cursor position queries (ESC[6n) on output
#[derive(Clone, Copy, PartialEq)]
enum CprFilterState {
    Normal,
    GotEsc,
    GotBracket,
    InNumber,
}

#[derive(Clone, Copy, PartialEq)]
enum CpqFilterState {
    Normal,
    GotEsc,
    GotBracket,
    Got6,
}

pub struct Serial16550 {
    base: u16,
    regs: [u8; 8],
    dlab: bool,
    divisor: u16,
    thre_pending: bool,
    rda_pending: bool,
    input_buffer: VecDeque<u8>,
    input_rx: Option<Receiver<u8>>,
    cpr_state: CprFilterState,
    cpq_state: CpqFilterState,
}

impl Serial16550 {
    pub fn new(base: u16) -> Self {
        Serial16550 {
            base,
            regs: [0u8; 8],
            dlab: false,
            divisor: 0,
            thre_pending: false,
            rda_pending: false,
            input_buffer: VecDeque::new(),
            input_rx: None,
            cpr_state: CprFilterState::Normal,
            cpq_state: CpqFilterState::Normal,
        }
    }

    /// Filter cursor position query (ESC[6n) on output
    /// Returns true if byte should be suppressed
    fn filter_cpq(&mut self, byte: u8) -> bool {
        match (self.cpq_state, byte) {
            (CpqFilterState::Normal, 0x1b) => {
                self.cpq_state = CpqFilterState::GotEsc;
                false // don't suppress yet, might not be CPQ
            }
            (CpqFilterState::GotEsc, b'[') => {
                self.cpq_state = CpqFilterState::GotBracket;
                false
            }
            (CpqFilterState::GotEsc, _) => {
                self.cpq_state = CpqFilterState::Normal;
                false
            }
            (CpqFilterState::GotBracket, b'6') => {
                self.cpq_state = CpqFilterState::Got6;
                false
            }
            (CpqFilterState::GotBracket, _) => {
                self.cpq_state = CpqFilterState::Normal;
                false
            }
            (CpqFilterState::Got6, b'n') => {
                // Complete CPQ sequence - suppress it
                // We already output ESC [ 6, so we can't take them back
                // But we suppress the 'n' to break the sequence
                self.cpq_state = CpqFilterState::Normal;
                true // suppress 'n'
            }
            (CpqFilterState::Got6, _) => {
                self.cpq_state = CpqFilterState::Normal;
                false
            }
            (CpqFilterState::Normal, _) => false,
        }
    }

    /// Enable stdin input handling by spawning a reader thread
    pub fn enable_input(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.input_rx = Some(rx);

        thread::spawn(move || {
            let stdin = io::stdin();
            let mut handle = stdin.lock();
            let mut buf = [0u8; 1];
            loop {
                match handle.read(&mut buf) {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        if tx.send(buf[0]).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    /// Check for new input from stdin and buffer it
    /// Filters out cursor position responses (ESC[n;nR) from terminal
    pub fn poll_input(&mut self) {
        if let Some(ref rx) = self.input_rx {
            loop {
                match rx.try_recv() {
                    Ok(byte) => {
                        // Filter cursor position responses: ESC [ digits ; digits R
                        let filtered = match (self.cpr_state, byte) {
                            (CprFilterState::Normal, 0x1b) => {
                                self.cpr_state = CprFilterState::GotEsc;
                                true // filter ESC
                            }
                            (CprFilterState::GotEsc, b'[') => {
                                self.cpr_state = CprFilterState::GotBracket;
                                true
                            }
                            (CprFilterState::GotEsc, _) => {
                                // Not a CSI, pass through ESC and this byte
                                self.cpr_state = CprFilterState::Normal;
                                self.input_buffer.push_back(0x1b);
                                false
                            }
                            (CprFilterState::GotBracket, b'0'..=b'9') => {
                                self.cpr_state = CprFilterState::InNumber;
                                true
                            }
                            (CprFilterState::GotBracket, _) => {
                                // Not a CPR, pass through
                                self.cpr_state = CprFilterState::Normal;
                                self.input_buffer.push_back(0x1b);
                                self.input_buffer.push_back(b'[');
                                false
                            }
                            (CprFilterState::InNumber, b'0'..=b'9' | b';') => {
                                true // still in CPR sequence
                            }
                            (CprFilterState::InNumber, b'R') => {
                                // End of CPR - discard entire sequence
                                self.cpr_state = CprFilterState::Normal;
                                true
                            }
                            (CprFilterState::InNumber, _) => {
                                // Not a CPR after all, but too late to recover
                                // Just discard and continue
                                self.cpr_state = CprFilterState::Normal;
                                false
                            }
                            (CprFilterState::Normal, _) => false,
                        };

                        if !filtered {
                            self.input_buffer.push_back(byte);
                            // Set RDA interrupt pending if IER allows
                            let ier = self.regs[IER_REG as usize];
                            if (ier & 0x01) != 0 {
                                self.rda_pending = true;
                            }
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => break,
                }
            }
        }
    }

    /// Returns true if an interrupt should be injected
    pub fn has_pending_interrupt(&self) -> bool {
        let ier = self.regs[IER_REG as usize];
        (self.thre_pending && (ier & 0x02) != 0) || (self.rda_pending && (ier & 0x01) != 0)
    }

    /// Returns true if there's input data available
    pub fn has_input(&self) -> bool {
        !self.input_buffer.is_empty()
    }

    fn port_offset(&self, port: u16) -> Option<u16> {
        if port < self.base {
            return None;
        }
        let offset = port - self.base;
        if offset < self.regs.len() as u16 {
            Some(offset)
        } else {
            None
        }
    }

    fn write_reg(&mut self, offset: u16, value: u8) {
        match offset {
            DATA_REG => {
                if self.dlab {
                    self.divisor = (self.divisor & 0xff00) | value as u16;
                } else {
                    // Filter cursor position queries (ESC[6n) to prevent terminal responses
                    if !self.filter_cpq(value) {
                        let _ = io::stdout().write_all(&[value]);
                        let _ = io::stdout().flush();
                    }
                    // Set THRE interrupt pending (transmitter is immediately empty)
                    self.thre_pending = true;
                }
            }
            IER_REG => {
                if self.dlab {
                    self.divisor = (self.divisor & 0x00ff) | ((value as u16) << 8);
                } else {
                    self.regs[IER_REG as usize] = value;
                }
            }
            IIR_REG => {
                // IIR is read-only, writes go to FCR (FIFO control)
                self.regs[IIR_REG as usize] = value;
            }
            LCR_REG => {
                self.regs[LCR_REG as usize] = value;
                self.dlab = (value & 0x80) != 0;
            }
            MCR_REG => {
                self.regs[MCR_REG as usize] = value;
            }
            LSR_REG => {
                self.regs[LSR_REG as usize] = value;
            }
            MSR_REG => {
                self.regs[MSR_REG as usize] = value;
            }
            SCR_REG => {
                self.regs[SCR_REG as usize] = value;
            }
            _ => {}
        }
    }

    fn read_reg(&mut self, offset: u16) -> u8 {
        match offset {
            DATA_REG => {
                if self.dlab {
                    (self.divisor & 0xff) as u8
                } else {
                    // Return buffered input character
                    self.input_buffer.pop_front().unwrap_or(0)
                }
            }
            IER_REG => {
                if self.dlab {
                    ((self.divisor >> 8) & 0xff) as u8
                } else {
                    self.regs[IER_REG as usize]
                }
            }
            IIR_REG => {
                let ier = self.regs[IER_REG as usize];
                // Priority: RDA > THRE
                if self.rda_pending && (ier & 0x01) != 0 {
                    self.rda_pending = false;
                    0x04 // RDA interrupt ID (bits 1-2 = 10, bit 0 = 0 = pending)
                } else if self.thre_pending && (ier & 0x02) != 0 {
                    self.thre_pending = false;
                    0x02 // THRE interrupt ID (bits 1-2 = 01, bit 0 = 0 = pending)
                } else {
                    0x01 // No interrupt pending
                }
            }
            LCR_REG => self.regs[LCR_REG as usize],
            MCR_REG => self.regs[MCR_REG as usize],
            LSR_REG => {
                let mut lsr = LSR_THRE | LSR_TEMT; // Transmitter always empty
                if !self.input_buffer.is_empty() {
                    lsr |= LSR_DATA_READY;
                }
                lsr
            }
            MSR_REG => self.regs[MSR_REG as usize],
            SCR_REG => self.regs[SCR_REG as usize],
            _ => 0,
        }
    }
}

impl IoDevice for Serial16550 {
    fn read(&mut self, port: u16) -> u8 {
        self.port_offset(port)
            .map(|offset| self.read_reg(offset))
            .unwrap_or(0)
    }

    fn write(&mut self, port: u16, value: u8) {
        if let Some(offset) = self.port_offset(port) {
            self.write_reg(offset, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_tracks_dlab_and_divisor() {
        let mut serial = Serial16550::new(0x3f8);
        serial.write(0x3fb, 0x80);
        serial.write(0x3f8, 0x34);
        serial.write(0x3f9, 0x12);
        assert_eq!(serial.divisor, 0x1234);
        serial.write(0x3fb, 0x00);
        assert!(!serial.dlab);
    }
}
