//! Widening, narrowing, rounding, source-half selection, and cumulative QC.

use super::{ProgramState, assert_program_parity, assert_program_parity_for_states, initial_state};

fn element_mask(bits: u32) -> u64 {
    match bits {
        64 => u64::MAX,
        _ => (1u64 << bits) - 1,
    }
}

fn patterned_vector(bits: u32, low: u64, high: u64) -> [u64; 2] {
    let mask = element_mask(bits);
    let mut halves = [0; 2];
    for (half, value) in halves.iter_mut().zip([low, high]) {
        for lane in 0..(64 / bits) {
            *half |= (value & mask) << (lane * bits);
        }
    }
    halves
}

fn boundary_states(bits: u32) -> Vec<ProgramState> {
    let sign = 1u64 << (bits - 1);
    let mask = element_mask(bits);
    let values = [0, 1, 2, sign - 1, sign, sign + 1, mask - 1, mask];
    let mut states = Vec::new();
    for (index, left) in values.into_iter().enumerate() {
        for right in values {
            let mut state = initial_state(left);
            state.v[0] = patterned_vector(bits, values[(index + 3) % values.len()], !right);
            state.v[1] = patterned_vector(bits, left, !left);
            state.v[2] = patterned_vector(bits, right, right.rotate_left(1));
            state.fpsr = 0x0090;
            states.push(state.clone());
            state.fpsr = 0x0800_0090;
            states.push(state);
        }
    }
    states
}

fn encode_three_different(
    q: u32,
    u: u32,
    size: u32,
    opcode: u32,
    rd: u32,
    rn: u32,
    rm: u32,
) -> u32 {
    0x0e20_0000
        | (q << 30)
        | (u << 29)
        | (size << 22)
        | (rm << 16)
        | (opcode << 12)
        | (rn << 5)
        | rd
}

fn encode_misc(q: u32, u: u32, size: u32, opcode: u32, rd: u32, rn: u32) -> u32 {
    0x0e20_0800 | (q << 30) | (u << 29) | (size << 22) | (opcode << 12) | (rn << 5) | rd
}

fn encode_shift(q: u32, u: u32, encoded: u32, opcode: u32, rd: u32, rn: u32) -> u32 {
    0x0f00_0400 | (q << 30) | (u << 29) | (encoded << 16) | (opcode << 11) | (rn << 5) | rd
}

#[test]
fn widening_arithmetic_and_multiply_cover_signedness_source_halves_and_aliases() {
    let opcodes = [0, 1, 2, 3, 5, 7, 8, 10, 12];
    for size in 0..=2 {
        let states = boundary_states(8 << size);
        for q in 0..=1 {
            for u in 0..=1 {
                for opcode in opcodes {
                    for (rd, rn, rm) in [(0, 1, 2), (1, 1, 2), (2, 1, 2), (0, 1, 1)] {
                        let instruction = encode_three_different(q, u, size, opcode, rd, rn, rm);
                        assert_program_parity_for_states(&[instruction], &states);
                    }
                }
            }
        }
    }
}

#[test]
fn doubling_long_multiply_covers_product_and_accumulator_saturation() {
    for size in 1..=2 {
        let bits = 8 << size;
        let mut states = boundary_states(bits);
        let wide_bits = bits * 2;
        let sign = 1u64 << (wide_bits - 1);
        let mask = element_mask(wide_bits);
        for state in boundary_states(bits) {
            for accumulator in [0, 1, sign - 1, sign, sign + 1, mask] {
                let mut initial = state.clone();
                initial.v[0] = patterned_vector(wide_bits, accumulator, !accumulator);
                states.push(initial);
            }
        }
        for opcode in [9, 11, 13] {
            for q in 0..=1 {
                for (rd, rn, rm) in [(0, 1, 2), (1, 1, 2), (2, 1, 2), (0, 1, 1)] {
                    let instruction = encode_three_different(q, 0, size, opcode, rd, rn, rm);
                    assert_program_parity_for_states(&[instruction], &states);
                }
            }
            for (rd, rn, rm) in [(0, 1, 2), (1, 1, 2), (2, 1, 2), (0, 1, 1)] {
                let scalar = encode_three_different(1, 0, size, opcode, rd, rn, rm) | (1 << 28);
                assert_program_parity_for_states(&[scalar], &states);
            }
        }
    }
}

#[test]
fn widening_shifts_cover_every_immediate_and_element_size_shift() {
    for size in 0..=2 {
        let bits = 8 << size;
        let states = boundary_states(bits);
        for q in 0..=1 {
            for u in 0..=1 {
                for amount in 0..bits {
                    for (rd, rn) in [(0, 1), (1, 1)] {
                        let instruction = encode_shift(q, u, bits + amount, 20, rd, rn);
                        assert_program_parity_for_states(&[instruction], &states);
                    }
                }
            }
            for (rd, rn) in [(0, 1), (1, 1)] {
                let instruction = encode_misc(q, 1, size, 19, rd, rn);
                assert_program_parity_for_states(&[instruction], &states);
            }
        }
    }
}

#[test]
fn pairwise_long_adds_cover_adjacent_lanes_accumulators_and_short_vectors() {
    for size in 0..=2 {
        let bits = 8 << size;
        let mut states = boundary_states(bits);
        for state in &mut states {
            let mut vector = 0u128;
            for lane in 0..(128 / bits) {
                let value = match lane % 4 {
                    0 => element_mask(bits),
                    1 => 1,
                    2 => 1u64 << (bits - 1),
                    _ => (1u64 << (bits - 1)) - 1,
                };
                vector |= u128::from(value) << (lane * bits);
            }
            state.v[1] = [vector as u64, (vector >> 64) as u64];
        }
        for q in 0..=1 {
            for u in 0..=1 {
                for opcode in [2, 6] {
                    for (rd, rn) in [(0, 1), (1, 1)] {
                        let instruction = encode_misc(q, u, size, opcode, rd, rn);
                        assert_program_parity_for_states(&[instruction], &states);
                    }
                }
            }
        }
    }
}

#[test]
fn high_half_add_sub_narrowing_covers_wrapping_rounding_and_destination_merge() {
    for size in 0..=2 {
        let states = boundary_states(16 << size);
        for q in 0..=1 {
            for u in 0..=1 {
                for opcode in [4, 6] {
                    for (rd, rn, rm) in [(0, 1, 2), (1, 1, 2), (2, 1, 2), (0, 1, 1)] {
                        let instruction = encode_three_different(q, u, size, opcode, rd, rn, rm);
                        assert_program_parity_for_states(&[instruction], &states);
                    }
                }
            }
        }
    }
}

#[test]
fn extract_narrowing_covers_signed_unsigned_and_signed_to_unsigned_qc() {
    for size in 0..=2 {
        let states = boundary_states(16 << size);
        for (u, opcode) in [(0, 18), (1, 18), (0, 20), (1, 20)] {
            for q in 0..=1 {
                for (rd, rn) in [(0, 1), (1, 1)] {
                    let instruction = encode_misc(q, u, size, opcode, rd, rn);
                    assert_program_parity_for_states(&[instruction], &states);
                }
            }
            if (u, opcode) != (0, 18) {
                for (rd, rn) in [(0, 1), (1, 1)] {
                    let scalar = encode_misc(1, u, size, opcode, rd, rn) | (1 << 28);
                    assert_program_parity_for_states(&[scalar], &states);
                }
            }
        }
    }
}

#[test]
fn narrowing_shifts_cover_every_amount_rounding_boundary_and_scalar_saturation() {
    for size in 0..=2 {
        let bits = 8 << size;
        let mut states = boundary_states(bits * 2);
        for amount in 1..=bits {
            for boundary in [0, 1, (1u64 << (amount - 1)) - 1, 1u64 << (amount - 1)] {
                let mut state = initial_state(boundary);
                state.v[1] = patterned_vector(bits * 2, boundary, !boundary);
                states.push(state);
            }
            for u in 0..=1 {
                for opcode in 16..=19 {
                    for q in 0..=1 {
                        for (rd, rn) in [(0, 1), (1, 1)] {
                            let instruction = encode_shift(q, u, 2 * bits - amount, opcode, rd, rn);
                            assert_program_parity_for_states(&[instruction], &states);
                        }
                    }
                    if u != 0 || opcode >= 18 {
                        for (rd, rn) in [(0, 1), (1, 1)] {
                            let scalar =
                                encode_shift(1, u, 2 * bits - amount, opcode, rd, rn) | (1 << 28);
                            assert_program_parity_for_states(&[scalar], &states);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn dependent_resize_programs_preserve_qc_and_values_across_width_transitions() {
    let program = [
        encode_three_different(0, 0, 0, 0, 0, 1, 2), // SADDL V0.8H
        encode_three_different(1, 1, 0, 8, 0, 1, 2), // UMLAL2 V0.8H
        encode_misc(0, 0, 0, 20, 3, 0),              // SQXTN V3.8B
        encode_misc(1, 0, 0, 18, 3, 0),              // XTN2 V3.16B
        encode_shift(1, 1, 8 + 7, 20, 4, 3),         // USHLL2 V4.8H, #7
        encode_shift(0, 1, 16 - 1, 19, 5, 4),        // UQRSHRN V5.8B, #1
        encode_misc(1, 1, 0, 19, 6, 5),              // SHLL2 V6.8H, #8
        encode_misc(1, 0, 0, 6, 6, 5),               // SADALP V6.8H
    ];
    assert_program_parity(&program);
}
