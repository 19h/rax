//! Exact common semantics for SSE4.2 `PCMPxSTRx` instructions.

/// Architectural data result and status flags produced by one packed-string
/// comparison. AF and PF are always cleared by the instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PackedStringResult {
    pub value: u128,
    pub cf: bool,
    pub zf: bool,
    pub sf: bool,
    pub of: bool,
}

/// Return the implicit string length selected by `imm8[0]`.
pub(crate) fn find_null_terminator(lo: u64, hi: u64, imm8: u8) -> i64 {
    let is_word = (imm8 & 0x01) != 0;
    let element_bits = if is_word { 16 } else { 8 };
    let elements_per_qword = 64 / element_bits;
    let element_mask = if is_word { 0xFFFF } else { 0xFF };

    for (qword_index, qword) in [lo, hi].into_iter().enumerate() {
        for element_index in 0..elements_per_qword {
            if ((qword >> (element_index * element_bits)) & element_mask) == 0 {
                return (qword_index * elements_per_qword + element_index) as i64;
            }
        }
    }

    (elements_per_qword * 2) as i64
}

/// Evaluate the exact Intel `PCMPxSTRx` comparison and output selection.
///
/// Explicit lengths are signed architectural values. Their unsigned absolute
/// magnitude is saturated to the selected element count (16 bytes or 8 words).
/// Implicit callers pass the already bounded result of
/// [`find_null_terminator`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn evaluate(
    first_lo: u64,
    first_hi: u64,
    second_lo: u64,
    second_hi: u64,
    len1: i64,
    len2: i64,
    imm8: u8,
    return_mask: bool,
) -> PackedStringResult {
    let is_word = (imm8 & 0x01) != 0;
    let is_signed = (imm8 & 0x02) != 0;
    let aggregation = (imm8 >> 2) & 0x03;
    let polarity = (imm8 >> 4) & 0x03;
    let output_select = (imm8 >> 6) & 0x01;
    let num_elements = if is_word { 8usize } else { 16usize };
    let valid1 = len1.unsigned_abs().min(num_elements as u64) as usize;
    let valid2 = len2.unsigned_abs().min(num_elements as u64) as usize;

    let get_element = |lo: u64, hi: u64, index: usize| -> i32 {
        if is_word {
            let value = if index < 4 {
                ((lo >> (index * 16)) & 0xFFFF) as u16
            } else {
                ((hi >> ((index - 4) * 16)) & 0xFFFF) as u16
            };
            if is_signed {
                value as i16 as i32
            } else {
                value as i32
            }
        } else {
            let value = if index < 8 {
                ((lo >> (index * 8)) & 0xFF) as u8
            } else {
                ((hi >> ((index - 8) * 8)) & 0xFF) as u8
            };
            if is_signed {
                value as i8 as i32
            } else {
                value as i32
            }
        }
    };

    let mut int_res1 = 0u16;
    match aggregation {
        // Equal any: each valid element of the second operand is compared with
        // every valid element of the first operand.
        0 => {
            for j in 0..valid2 {
                let second = get_element(second_lo, second_hi, j);
                for i in 0..valid1 {
                    if get_element(first_lo, first_hi, i) == second {
                        int_res1 |= 1 << j;
                        break;
                    }
                }
            }
        }
        // Ranges: consecutive valid first-operand element pairs are inclusive
        // lower/upper bounds for each valid second-operand element.
        1 => {
            for j in 0..valid2 {
                let second = get_element(second_lo, second_hi, j);
                let mut i = 0;
                while i + 1 < valid1 {
                    let low = get_element(first_lo, first_hi, i);
                    let high = get_element(first_lo, first_hi, i + 1);
                    if second >= low && second <= high {
                        int_res1 |= 1 << j;
                        break;
                    }
                    i += 2;
                }
            }
        }
        // Equal each, including the architecturally specified invalid-element
        // override: both invalid is true; exactly one invalid is false.
        2 => {
            for i in 0..num_elements {
                let first_valid = i < valid1;
                let second_valid = i < valid2;
                let bit = if first_valid && second_valid {
                    get_element(first_lo, first_hi, i) == get_element(second_lo, second_hi, i)
                } else {
                    !first_valid && !second_valid
                };
                int_res1 |= u16::from(bit) << i;
            }
        }
        // Equal ordered. A first-operand element beyond its explicit/implicit
        // length, or a comparison beyond the 128-bit window, imposes no further
        // constraint. A second-operand element beyond its string length but
        // still inside the vector forces the candidate false.
        3 => {
            for j in 0..num_elements {
                let mut matched = true;
                for i in 0..num_elements {
                    if i + j >= num_elements || i >= valid1 {
                        break;
                    }
                    if i + j >= valid2
                        || get_element(first_lo, first_hi, i)
                            != get_element(second_lo, second_hi, i + j)
                    {
                        matched = false;
                        break;
                    }
                }
                if matched {
                    int_res1 |= 1 << j;
                }
            }
        }
        _ => unreachable!(),
    }

    let element_mask = if num_elements == 16 {
        u16::MAX
    } else {
        (1u16 << num_elements) - 1
    };
    let valid2_mask = if valid2 == 16 {
        u16::MAX
    } else if valid2 == 0 {
        0
    } else {
        (1u16 << valid2) - 1
    };
    let int_res2 = match polarity {
        0 => int_res1,
        1 => !int_res1 & element_mask,
        2 => int_res1,
        3 => (int_res1 ^ valid2_mask) & element_mask,
        _ => unreachable!(),
    };

    let value = if return_mask {
        if output_select == 0 {
            u128::from(int_res2)
        } else {
            let mut expanded = 0u128;
            let element_bits = if is_word { 16 } else { 8 };
            let all_ones = if is_word { 0xFFFFu128 } else { 0xFFu128 };
            for i in 0..num_elements {
                if int_res2 & (1 << i) != 0 {
                    expanded |= all_ones << (i * element_bits);
                }
            }
            expanded
        }
    } else if output_select == 0 {
        int_res2.trailing_zeros().min(num_elements as u32) as u128
    } else {
        let index = if int_res2 == 0 {
            num_elements
        } else {
            (u16::BITS - 1 - int_res2.leading_zeros()) as usize
        };
        index as u128
    };

    PackedStringResult {
        value,
        cf: int_res2 != 0,
        zf: valid2 < num_elements,
        sf: valid1 < num_elements,
        of: int_res2 & 1 != 0,
    }
}
