//! Arm EABI helpers required by the custom ARMv6 bare-metal target.

/// Read a little-endian 32-bit value from an arbitrarily aligned address.
///
/// The 2025Q4 Arm Run-time ABI, "Unaligned memory access", requires
/// `__aeabi_uread4` to accept arbitrary byte alignment and permits it to
/// clobber `r0-r3`, `ip`, `lr`, and `CPSR`. The byte loads avoid recursively
/// lowering this helper to the same unaligned word-load symbol on ARMv6.
///
/// # Safety
///
/// The caller must provide a pointer to a readable four-byte range. This naked
/// implementation performs exactly four one-byte loads, which require only
/// byte alignment and do not create references. It follows the target's AAPCS
/// C ABI: the pointer arrives in `r0`, the value returns in `r0`, and only the
/// caller-saved `r1` and `r2` registers plus condition flags are clobbered. It
/// cannot unwind because it contains no call or stack operation.
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub(crate) unsafe extern "C" fn __aeabi_uread4(_address: *const u8) -> u32 {
    core::arch::naked_asm!(
        "ldrb r1, [r0]",
        "ldrb r2, [r0, #1]",
        "orr r1, r1, r2, lsl #8",
        "ldrb r2, [r0, #2]",
        "orr r1, r1, r2, lsl #16",
        "ldrb r2, [r0, #3]",
        "orr r0, r1, r2, lsl #24",
        "bx lr",
    )
}
