//! AMD XOP operation types.

/// AMD XOP packed-vector bit operation. A signed 8-bit count selects direction:
/// positive values operate left and negative values operate right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum X86XopPackedBitKind {
    /// Circular rotate in either direction.
    Rotate,
    /// Logical shift in either direction.
    LogicalShift,
    /// Left shift or sign-preserving arithmetic right shift.
    ArithmeticShift,
}
