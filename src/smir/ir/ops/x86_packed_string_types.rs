//! Selector types for exact x86 packed-string comparison operations.

/// Architectural legacy SSE4.2 packed-string comparison/output form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum X86PackedStringKind {
    ExplicitMask,
    ExplicitIndex,
    ImplicitMask,
    ImplicitIndex,
}

impl X86PackedStringKind {
    pub const fn is_explicit(self) -> bool {
        matches!(self, Self::ExplicitMask | Self::ExplicitIndex)
    }

    pub const fn returns_mask(self) -> bool {
        matches!(self, Self::ExplicitMask | Self::ImplicitMask)
    }
}
