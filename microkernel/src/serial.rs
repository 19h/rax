//! Architecture-neutral serial console.
//!
//! Every architecture provides a single `putc` primitive in [`crate::arch`];
//! this module layers a [`core::fmt::Write`] sink and `print!`/`println!`
//! macros on top so the rest of the kernel can format freely. The host test
//! runner observes these bytes (port 0xE9 on x86_64, the PL011 on AArch64, the
//! Samsung UART on ARMv6) and scans them for the result sentinel.

use core::fmt;

pub struct Serial;

impl Serial {
    #[inline]
    pub fn write_byte(&self, b: u8) {
        crate::arch::putc(b);
    }

    pub fn write_bytes(&self, bytes: &[u8]) {
        for &b in bytes {
            self.write_byte(b);
        }
    }
}

impl fmt::Write for Serial {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_bytes(s.as_bytes());
        Ok(())
    }
}

macro_rules! print {
    ($($arg:tt)*) => {{
        let _ = core::fmt::Write::write_fmt(&mut $crate::serial::Serial, format_args!($($arg)*));
    }};
}

macro_rules! println {
    () => { print!("\n") };
    ($($arg:tt)*) => {{
        print!($($arg)*);
        print!("\n");
    }};
}

/// Best-effort panic reporter (formatting may itself be what panicked, so this
/// stays minimal and never allocates).
#[cfg(not(feature = "usermode"))]
pub fn on_panic(info: &core::panic::PanicInfo) -> ! {
    let s = Serial;
    s.write_bytes(b"\nRAX-MK: KERNEL PANIC");
    if let Some(loc) = info.location() {
        s.write_bytes(b" at ");
        s.write_bytes(loc.file().as_bytes());
    }
    s.write_bytes(b"\nRAX-MK: RESULT FAIL (panic)\n");
    crate::arch::poweroff();
}
