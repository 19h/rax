//! Unified vCPU exit reasons.

/// vCPU exit reasons (backend-agnostic).
#[derive(Debug, Clone)]
pub enum VcpuExit {
    /// CPU halted (HLT instruction).
    Hlt,

    /// I/O port read.
    IoIn {
        /// Port number.
        port: u16,
        /// Number of bytes to read.
        size: u8,
    },

    /// I/O port write.
    IoOut {
        /// Port number.
        port: u16,
        /// Data written.
        data: Vec<u8>,
    },

    /// Memory-mapped I/O read.
    MmioRead {
        /// Physical address.
        addr: u64,
        /// Number of bytes to read.
        size: u8,
    },

    /// Memory-mapped I/O write.
    MmioWrite {
        /// Physical address.
        addr: u64,
        /// Data written.
        data: Vec<u8>,
    },

    /// VM shutdown requested.
    Shutdown,

    /// System event (KVM-specific, but useful for compatibility).
    SystemEvent {
        /// Event type.
        type_: u32,
        /// Event flags.
        flags: u64,
    },

    /// vCPU entry failed.
    FailEntry {
        /// Hardware entry failure reason.
        reason: u64,
    },

    /// Internal error.
    InternalError,

    /// Unknown or unhandled exit.
    Unknown(String),
}
