//! x86 cache hints and privileged maintenance operations.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86CacheControlKind {
    Cldemote,
    Clflush,
    Clflushopt,
    Clwb,
}

/// Operand-free cache maintenance. The profile models no dirty data cache,
/// while preserving privilege faults, ordering and serialization boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86CacheMaintenanceKind {
    Invd,
    Wbinvd,
    Wbnoinvd,
}

impl X86CacheMaintenanceKind {
    pub fn mnemonic(self) -> &'static str {
        match self {
            Self::Invd => "invd",
            Self::Wbinvd => "wbinvd",
            Self::Wbnoinvd => "wbnoinvd",
        }
    }
}
