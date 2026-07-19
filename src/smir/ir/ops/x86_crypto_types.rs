//! Exact x86 cryptographic operation selectors used by SMIR.

/// Legacy SHA-NI operation over four packed 32-bit words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum X86Sha32Op {
    Sha1Nexte,
    Sha1Msg1,
    Sha1Msg2,
    Sha1Rounds4,
    Sha256Msg1,
    Sha256Msg2,
    Sha256Rounds2,
}
