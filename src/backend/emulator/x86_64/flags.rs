//! RFLAGS register helpers.

/// RFLAGS bit positions.
pub mod bits {
    pub const CF: u64 = 1 << 0;  // Carry Flag
    pub const PF: u64 = 1 << 2;  // Parity Flag
    pub const AF: u64 = 1 << 4;  // Auxiliary Carry Flag
    pub const ZF: u64 = 1 << 6;  // Zero Flag
    pub const SF: u64 = 1 << 7;  // Sign Flag
    pub const TF: u64 = 1 << 8;  // Trap Flag
    pub const IF: u64 = 1 << 9;  // Interrupt Enable Flag
    pub const DF: u64 = 1 << 10; // Direction Flag
    pub const OF: u64 = 1 << 11; // Overflow Flag
    pub const IOPL_MASK: u64 = 0x3000; // I/O Privilege Level
    pub const NT: u64 = 1 << 14; // Nested Task
    pub const RF: u64 = 1 << 16; // Resume Flag
    pub const VM: u64 = 1 << 17; // Virtual 8086 Mode
    pub const AC: u64 = 1 << 18; // Alignment Check
    pub const VIF: u64 = 1 << 19; // Virtual Interrupt Flag
    pub const VIP: u64 = 1 << 20; // Virtual Interrupt Pending
    pub const ID: u64 = 1 << 21; // ID Flag
}

/// Compute parity flag for the low 8 bits of a value.
/// PF is set if the number of set bits in the low byte is even.
pub fn compute_pf(value: u64) -> bool {
    (value as u8).count_ones() % 2 == 0
}

/// Compute zero flag.
pub fn compute_zf(value: u64, size: u8) -> bool {
    match size {
        1 => (value as u8) == 0,
        2 => (value as u16) == 0,
        4 => (value as u32) == 0,
        8 => value == 0,
        _ => false,
    }
}

/// Compute sign flag (most significant bit of result).
pub fn compute_sf(value: u64, size: u8) -> bool {
    match size {
        1 => (value as i8) < 0,
        2 => (value as i16) < 0,
        4 => (value as i32) < 0,
        8 => (value as i64) < 0,
        _ => false,
    }
}

/// Update flags after an arithmetic operation.
pub fn update_flags_add(rflags: &mut u64, a: u64, b: u64, result: u64, size: u8) {
    let mask = match size {
        1 => 0xFF,
        2 => 0xFFFF,
        4 => 0xFFFF_FFFF,
        8 => u64::MAX,
        _ => u64::MAX,
    };

    let a = a & mask;
    let b = b & mask;
    let result = result & mask;

    // CF: carry out of the result
    let cf = result < a;

    // ZF: result is zero
    let zf = compute_zf(result, size);

    // SF: sign of result
    let sf = compute_sf(result, size);

    // PF: parity of low byte
    let pf = compute_pf(result);

    // OF: signed overflow
    let sign_bit = match size {
        1 => 0x80,
        2 => 0x8000,
        4 => 0x8000_0000,
        8 => 0x8000_0000_0000_0000,
        _ => 0x8000_0000_0000_0000,
    };
    let of = ((a ^ result) & (b ^ result) & sign_bit) != 0;

    // AF: auxiliary carry (from bit 3 to bit 4)
    let af = ((a ^ b ^ result) & 0x10) != 0;

    // Clear and set flags
    *rflags &= !(bits::CF | bits::ZF | bits::SF | bits::PF | bits::OF | bits::AF);
    if cf { *rflags |= bits::CF; }
    if zf { *rflags |= bits::ZF; }
    if sf { *rflags |= bits::SF; }
    if pf { *rflags |= bits::PF; }
    if of { *rflags |= bits::OF; }
    if af { *rflags |= bits::AF; }
}

/// Update flags after a subtraction operation.
pub fn update_flags_sub(rflags: &mut u64, a: u64, b: u64, result: u64, size: u8) {
    let mask = match size {
        1 => 0xFF,
        2 => 0xFFFF,
        4 => 0xFFFF_FFFF,
        8 => u64::MAX,
        _ => u64::MAX,
    };

    let a = a & mask;
    let b = b & mask;
    let result = result & mask;

    // CF: borrow
    let cf = a < b;

    // ZF: result is zero
    let zf = compute_zf(result, size);

    // SF: sign of result
    let sf = compute_sf(result, size);

    // PF: parity of low byte
    let pf = compute_pf(result);

    // OF: signed overflow for subtraction
    let sign_bit = match size {
        1 => 0x80,
        2 => 0x8000,
        4 => 0x8000_0000,
        8 => 0x8000_0000_0000_0000,
        _ => 0x8000_0000_0000_0000,
    };
    let of = ((a ^ b) & (a ^ result) & sign_bit) != 0;

    // AF: auxiliary carry
    let af = ((a ^ b ^ result) & 0x10) != 0;

    // Clear and set flags
    *rflags &= !(bits::CF | bits::ZF | bits::SF | bits::PF | bits::OF | bits::AF);
    if cf { *rflags |= bits::CF; }
    if zf { *rflags |= bits::ZF; }
    if sf { *rflags |= bits::SF; }
    if pf { *rflags |= bits::PF; }
    if of { *rflags |= bits::OF; }
    if af { *rflags |= bits::AF; }
}

/// Update flags after a logical operation (AND, OR, XOR).
pub fn update_flags_logic(rflags: &mut u64, result: u64, size: u8) {
    // CF and OF are cleared for logical operations
    let zf = compute_zf(result, size);
    let sf = compute_sf(result, size);
    let pf = compute_pf(result);

    *rflags &= !(bits::CF | bits::ZF | bits::SF | bits::PF | bits::OF);
    if zf { *rflags |= bits::ZF; }
    if sf { *rflags |= bits::SF; }
    if pf { *rflags |= bits::PF; }
}
