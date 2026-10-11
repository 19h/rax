//! Scalar integer AdvSIMD boundaries, aliases, counts, and zero comparisons.

use super::{ProgramState, assert_program_parity_for_states, initial_state};

const BOUNDARIES: [u64; 8] = [
    0,
    1,
    2,
    0x7fff_ffff_ffff_ffff,
    0x8000_0000_0000_0000,
    0x8000_0000_0000_0001,
    u64::MAX - 1,
    u64::MAX,
];

fn arithmetic_states() -> Vec<ProgramState> {
    let mut states = Vec::new();
    for left in BOUNDARIES {
        for right in BOUNDARIES {
            for nzcv in [0, 0x5000_0000, 0xa000_0000, 0xf000_0000] {
                let mut state = initial_state(left);
                state.v[0] = [right, !left];
                state.v[1] = [left, right];
                state.v[2] = [right, !right];
                state.v[31] = [right, left];
                state.nzcv = nzcv;
                state.fpsr = if nzcv == 0 { 0x0090 } else { 0x0800_0090 };
                states.push(state);
            }
        }
    }
    states
}

fn scalar_three_same(u: u32, opcode: u32, rd: u32, rn: u32, rm: u32) -> u32 {
    0x5ee0_0400 | (u << 29) | (rm << 16) | (opcode << 11) | (rn << 5) | rd
}

fn two_misc(q: u32, scalar: bool, u: u32, size: u32, opcode: u32, rd: u32, rn: u32) -> u32 {
    0x0e20_0800
        | (q << 30)
        | (u32::from(scalar) << 28)
        | (u << 29)
        | (size << 22)
        | (opcode << 12)
        | (rn << 5)
        | rd
}

fn scalar_shift_immediate(u: u32, opcode: u32, amount: u32, rd: u32, rn: u32) -> u32 {
    let encoded_amount = if opcode == 10 {
        64 + amount
    } else {
        128 - amount
    };
    0x5f00_0400 | (u << 29) | (encoded_amount << 16) | (opcode << 11) | (rn << 5) | rd
}

#[test]
fn scalar_add_subtract_and_register_comparisons_preserve_flags_and_clear_upper_bits() {
    let states = arithmetic_states();
    for opcode in [6, 7, 16, 17] {
        for u in 0..=1 {
            for (rd, rn, rm) in [
                (0, 1, 2),
                (1, 1, 2),
                (2, 1, 2),
                (0, 1, 1),
                (1, 1, 1),
                (31, 1, 2),
                (0, 31, 1),
            ] {
                assert_program_parity_for_states(
                    &[scalar_three_same(u, opcode, rd, rn, rm)],
                    &states,
                );
            }
        }
    }
}

#[test]
fn scalar_register_shifts_use_every_signed_low_byte_and_ignore_other_count_bits() {
    let mut states = Vec::new();
    for value in BOUNDARIES {
        for shift in i8::MIN..=i8::MAX {
            for ignored_bits in [0, 0xffff_ffff_ffff_ff00] {
                let mut state = initial_state(value);
                state.v[1] = [value, !value];
                state.v[2] = [ignored_bits | u64::from(shift as u8), value];
                states.push(state);
            }
        }
    }
    for opcode in [8, 10] {
        for u in 0..=1 {
            for (rd, rn, rm) in [(0, 1, 2), (1, 1, 2), (2, 1, 2), (0, 1, 1), (31, 1, 2)] {
                assert_program_parity_for_states(
                    &[scalar_three_same(u, opcode, rd, rn, rm)],
                    &states,
                );
            }
        }
    }
}

#[test]
fn scalar_absolute_negation_and_pairwise_add_cover_minimum_and_wrapping() {
    let states = arithmetic_states();
    for (rd, rn) in [(0, 1), (1, 1), (31, 1), (0, 31)] {
        for u in 0..=1 {
            assert_program_parity_for_states(&[two_misc(1, true, u, 3, 11, rd, rn)], &states);
        }
        let addp = 0x5ef1_b800 | (rn << 5) | rd;
        assert_program_parity_for_states(&[addp], &states);
    }
}

#[test]
fn comparisons_against_zero_cover_every_scalar_and_vector_integer_arrangement() {
    for size in 0..=3 {
        let bits = 8 << size;
        let mask = if bits == 64 {
            u64::MAX
        } else {
            (1u64 << bits) - 1
        };
        let sign = 1u64 << (bits - 1);
        let mut states = Vec::new();
        for value in [0, 1, 2, sign - 1, sign, sign + 1, mask - 1, mask] {
            for nzcv in 0..16 {
                let mut state = initial_state(value);
                let mut low = 0;
                let mut high = 0;
                for lane in 0..(64 / bits) {
                    // Alternate positive, negative, and zero elements in one vector.
                    let low_value = if lane % 2 == 0 { value } else { 0 };
                    low |= low_value << (lane * bits);
                    high |= (mask ^ value) << (lane * bits);
                }
                state.v[1] = [low, high];
                state.v[31] = [high, low];
                state.nzcv = nzcv << 28;
                states.push(state);
            }
        }
        let forms = if size == 3 {
            vec![(1, false), (1, true)]
        } else {
            vec![(0, false), (1, false)]
        };
        for (q, scalar) in forms {
            for (u, opcode) in [(0, 8), (1, 8), (0, 9), (1, 9), (0, 10)] {
                for (rd, rn) in [(0, 1), (1, 1), (31, 1), (0, 31)] {
                    assert_program_parity_for_states(
                        &[two_misc(q, scalar, u, size, opcode, rd, rn)],
                        &states,
                    );
                }
            }
        }
    }
}

#[test]
fn scalar_immediate_shifts_cover_every_count_rounding_and_accumulator_alias() {
    let states = arithmetic_states();
    for opcode in [0, 2, 4, 6, 10] {
        let amounts = if opcode == 10 { 0..=63 } else { 1..=64 };
        let unsigned_forms = if opcode == 10 { 0..=0 } else { 0..=1 };
        for u in unsigned_forms {
            for amount in amounts.clone() {
                let mut count_states = states.clone();
                if matches!(opcode, 4 | 6) {
                    let half = 1u64 << (amount - 1);
                    for magnitude in [half - 1, half, half + 1] {
                        for value in [magnitude, 0u64.wrapping_sub(magnitude)] {
                            for fpsr in [0x0090, 0x0800_0090] {
                                let mut state = initial_state(value);
                                state.v[1] = [value, !value];
                                state.v[31] = [value, !value];
                                state.fpsr = fpsr;
                                count_states.push(state);
                            }
                        }
                    }
                }
                for (rd, rn) in [(0, 1), (1, 1), (31, 1), (0, 31)] {
                    assert_program_parity_for_states(
                        &[scalar_shift_immediate(u, opcode, amount, rd, rn)],
                        &count_states,
                    );
                }
            }
        }
    }
}

#[test]
fn dependent_scalar_programs_preserve_vector_and_status_state() {
    let program = [
        scalar_three_same(0, 16, 0, 1, 2),
        scalar_three_same(1, 16, 3, 0, 31),
        two_misc(1, true, 0, 3, 11, 3, 3),
        scalar_three_same(0, 10, 5, 3, 2),
        scalar_shift_immediate(1, 6, 64, 5, 5),
        two_misc(1, false, 0, 0, 9, 6, 1),
        0x5ef1_b800 | (6 << 5) | 7,
        scalar_three_same(0, 17, 8, 7, 5),
        two_misc(1, true, 1, 3, 9, 9, 8),
    ];
    assert_program_parity_for_states(&program, &arithmetic_states());
}
