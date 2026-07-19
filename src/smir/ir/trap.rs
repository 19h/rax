//! Architectural trap terminators.

/// Trap kinds represented as terminal SMIR control flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapKind {
    /// Debug breakpoint.
    Breakpoint,
    /// Undefined instruction.
    Undefined,
    /// Division by zero.
    DivideByZero,
    /// Integer overflow.
    Overflow,
    /// Bounds-check failure.
    Bounds,
    /// Invalid opcode.
    InvalidOpcode,
    /// System call.
    SystemCall,
    /// Halt and wait for interrupt.
    Halt,
    /// x86 general-protection exception with architectural error code zero.
    GeneralProtection,
}
