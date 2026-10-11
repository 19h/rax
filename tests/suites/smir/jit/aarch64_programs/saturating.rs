//! Saturating integer limits, signed shift bytes, indexed lanes, and sticky QC.

use super::{ProgramState, assert_program_parity, assert_program_parity_for_states, initial_state};

fn element_mask(bits: u32) -> u64 {
    match bits {
        64 => u64::MAX,
        _ => (1u64 << bits) - 1,
    }
}

fn repeated_lanes(bits: u32, value: u64) -> [u64; 2] {
    let mut half = 0;
    for lane in 0..(64 / bits) {
        half |= (value & element_mask(bits)) << (lane * bits);
    }
    [half, half]
}

fn boundary_values(bits: u32) -> [u64; 8] {
    let sign = 1u64 << (bits - 1);
    let mask = element_mask(bits);
    [0, 1, 2, sign - 1, sign, sign + 1, mask - 1, mask]
}

fn arithmetic_states(bits: u32) -> Vec<ProgramState> {
    let mut states = Vec::new();
    for left in boundary_values(bits) {
        for right in boundary_values(bits) {
            for fpsr in [0x0090, 0x0800_0090] {
                let mut state = initial_state(left);
                state.v[0] = repeated_lanes(bits, right);
                state.v[1] = repeated_lanes(bits, left);
                state.v[2] = repeated_lanes(bits, right);
                // Different upper lanes expose incorrect vector-width selection.
                state.v[1][1] = !state.v[1][1];
                state.v[2][1] = !state.v[2][1];
                state.fpsr = fpsr;
                states.push(state);
            }
        }
    }
    states
}

fn multiply_states(bits: u32) -> Vec<ProgramState> {
    let mut states = arithmetic_states(bits);
    let half_product = 1u64 << (bits - 2);
    let mask = element_mask(bits);
    // Doubling (+/-1) * (+/-2^(E-2)) produces exactly +/-1/2 after
    // scaling by 2^E. Adjacent values test both sides of the rounding tie.
    for left in [1, mask] {
        for magnitude in [half_product - 1, half_product, half_product + 1] {
            for right in [magnitude, 0u64.wrapping_sub(magnitude) & mask] {
                for fpsr in [0x0090, 0x0800_0090] {
                    let mut state = initial_state(left);
                    state.v[1] = repeated_lanes(bits, left);
                    state.v[2] = repeated_lanes(bits, right);
                    state.fpsr = fpsr;
                    states.push(state);
                }
            }
        }
    }
    states
}

fn forms(size: u32) -> Vec<(u32, bool)> {
    match size {
        3 => vec![(1, false), (1, true)],
        _ => vec![(0, false), (1, false), (1, true)],
    }
}

fn three_same(
    q: u32,
    scalar: bool,
    u: u32,
    size: u32,
    opcode: u32,
    rd: u32,
    rn: u32,
    rm: u32,
) -> u32 {
    0x0e20_0400
        | (q << 30)
        | (u << 29)
        | (u32::from(scalar) << 28)
        | (size << 22)
        | (rm << 16)
        | (opcode << 11)
        | (rn << 5)
        | rd
}

fn two_misc(q: u32, scalar: bool, u: u32, size: u32, opcode: u32, rd: u32, rn: u32) -> u32 {
    0x0e20_0800
        | (q << 30)
        | (u << 29)
        | (u32::from(scalar) << 28)
        | (size << 22)
        | (opcode << 12)
        | (rn << 5)
        | rd
}

fn shift_immediate(
    q: u32,
    scalar: bool,
    u: u32,
    bits: u32,
    amount: u32,
    opcode: u32,
    rd: u32,
    rn: u32,
) -> u32 {
    0x0f00_0400
        | (q << 30)
        | (u << 29)
        | (u32::from(scalar) << 28)
        | ((bits + amount) << 16)
        | (opcode << 11)
        | (rn << 5)
        | rd
}

fn multiply_indexed(
    q: u32,
    scalar: bool,
    size: u32,
    rounding: bool,
    lane: u32,
    rd: u32,
    rn: u32,
    rm: u32,
) -> u32 {
    let (rm_field, index_fields) = match size {
        1 => (
            rm,
            ((lane & 1) << 20) | (((lane >> 1) & 1) << 21) | ((lane >> 2) << 11),
        ),
        _ => (rm, ((lane & 1) << 21) | ((lane >> 1) << 11)),
    };
    0x0f00_0000
        | (q << 30)
        | (u32::from(scalar) << 28)
        | (size << 22)
        | (rm_field << 16)
        | ((12 + u32::from(rounding)) << 12)
        | index_fields
        | (rn << 5)
        | rd
}

#[test]
fn saturating_add_subtract_cover_all_sizes_forms_and_operand_aliases() {
    for size in 0..=3 {
        let states = arithmetic_states(8 << size);
        for (q, scalar) in forms(size) {
            for u in 0..=1 {
                for opcode in [1, 5] {
                    for (rd, rn, rm) in [(0, 1, 2), (1, 1, 2), (2, 1, 2), (0, 1, 1), (1, 1, 1)] {
                        assert_program_parity_for_states(
                            &[three_same(q, scalar, u, size, opcode, rd, rn, rm)],
                            &states,
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn mixed_saturating_accumulates_cover_both_signedness_directions() {
    for size in 0..=3 {
        let states = arithmetic_states(8 << size);
        for (q, scalar) in forms(size) {
            for u in 0..=1 {
                for (rd, rn) in [(0, 1), (1, 1), (31, 1), (0, 31)] {
                    assert_program_parity_for_states(
                        &[two_misc(q, scalar, u, size, 3, rd, rn)],
                        &states,
                    );
                }
            }
        }
    }
}

#[test]
fn saturating_absolute_and_negation_clear_unused_bits_and_retain_qc() {
    for size in 0..=3 {
        let states = arithmetic_states(8 << size);
        for (q, scalar) in forms(size) {
            for u in 0..=1 {
                for (rd, rn) in [(0, 1), (1, 1), (31, 1), (0, 31)] {
                    assert_program_parity_for_states(
                        &[two_misc(q, scalar, u, size, 7, rd, rn)],
                        &states,
                    );
                }
            }
        }
    }
}

#[test]
fn saturating_register_shifts_cover_every_signed_low_byte_and_rounding() {
    for size in 0..=3 {
        let bits = 8 << size;
        let mut states = Vec::new();
        for value in boundary_values(bits) {
            for shift in i8::MIN..=i8::MAX {
                for fpsr in [0x0090, 0x0800_0090] {
                    let mut state = initial_state(value);
                    state.v[1] = repeated_lanes(bits, value);
                    let shift_byte = u64::from(shift as u8);
                    let shift_lane = (element_mask(bits) & !0xff) | shift_byte;
                    state.v[2] = repeated_lanes(bits, shift_lane);
                    state.fpsr = fpsr;
                    states.push(state);
                }
            }
        }
        for (q, scalar) in forms(size) {
            for u in 0..=1 {
                for opcode in [9, 11] {
                    for (rd, rn, rm) in [(0, 1, 2), (1, 1, 2), (2, 1, 2), (0, 1, 1)] {
                        assert_program_parity_for_states(
                            &[three_same(q, scalar, u, size, opcode, rd, rn, rm)],
                            &states,
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn saturating_immediate_shifts_cover_every_amount_and_signed_to_unsigned() {
    for size in 0..=3 {
        let bits = 8 << size;
        let states = arithmetic_states(bits);
        for (q, scalar) in forms(size) {
            for (u, opcode) in [(0, 14), (1, 14), (1, 12)] {
                for amount in 0..bits {
                    for (rd, rn) in [(0, 1), (1, 1), (31, 1)] {
                        assert_program_parity_for_states(
                            &[shift_immediate(q, scalar, u, bits, amount, opcode, rd, rn)],
                            &states,
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn doubling_multiply_high_covers_minimum_product_rounding_and_indexed_lanes() {
    for size in 1..=2 {
        let bits = 8 << size;
        let states = multiply_states(bits);
        for (q, scalar) in forms(size) {
            for rounding in [false, true] {
                for (rd, rn, rm) in [(0, 1, 2), (1, 1, 2), (2, 1, 2), (0, 1, 1)] {
                    assert_program_parity_for_states(
                        &[three_same(
                            q,
                            scalar,
                            u32::from(rounding),
                            size,
                            22,
                            rd,
                            rn,
                            rm,
                        )],
                        &states,
                    );
                }
                for lane in 0..(128 / bits) {
                    let mut indexed_states = states.clone();
                    // Each selected source lane is independently varied; other
                    // lanes carry a different value to expose index mistakes.
                    for state in &mut indexed_states {
                        let selected = state.v[2][0] & element_mask(bits);
                        state.v[2] = repeated_lanes(bits, !selected);
                        let offset = lane * bits;
                        let mask = u128::from(element_mask(bits)) << offset;
                        let original =
                            u128::from(state.v[2][0]) | (u128::from(state.v[2][1]) << 64);
                        let value = (original & !mask) | (u128::from(selected) << offset);
                        state.v[2] = [value as u64, (value >> 64) as u64];
                    }
                    for (rd, rn, rm) in [(0, 1, 2), (1, 1, 2), (2, 1, 2), (0, 1, 1)] {
                        assert_program_parity_for_states(
                            &[multiply_indexed(
                                q, scalar, size, rounding, lane, rd, rn, rm,
                            )],
                            &indexed_states,
                        );
                    }
                }
                for rm in [15, 31] {
                    if size == 2 || rm == 15 {
                        assert_program_parity_for_states(
                            &[multiply_indexed(q, scalar, size, rounding, 0, 0, 1, rm)],
                            &states,
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn dependent_saturation_program_carries_qc_across_arithmetic_shifts_and_scalars() {
    let program = [
        three_same(1, false, 0, 0, 1, 0, 1, 2),
        two_misc(1, false, 0, 0, 3, 0, 2),
        two_misc(1, false, 1, 1, 7, 3, 0),
        shift_immediate(1, false, 1, 16, 5, 12, 4, 3),
        three_same(1, false, 1, 1, 11, 5, 4, 2),
        multiply_indexed(1, false, 1, true, 7, 6, 5, 2),
        three_same(1, true, 0, 2, 22, 7, 6, 1),
        three_same(1, false, 1, 3, 5, 0, 0, 7),
    ];
    assert_program_parity(&program);
    assert_program_parity_for_states(&program, &arithmetic_states(16));
}
