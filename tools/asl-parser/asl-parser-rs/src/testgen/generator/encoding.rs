//! Encoding test case generation.
//!
//! Generates tests for:
//! - Field boundary values
//! - Field combinations
//! - Special values (ZR, SP, etc.)
//! - Guard conditions
//! - Invalid encodings (UNDEFINED)
//! - Unpredictable bit patterns

use std::collections::HashMap;

use crate::syntax::Instruction;
use crate::testgen::analysis::encoding as enc_utils;
use crate::testgen::generator::boundary::{self, BoundaryValue};
use crate::testgen::types::*;
use crate::testgen::TestGenConfig;

/// Generate field boundary tests
pub fn generate_field_boundary_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
) -> Vec<EncodingTest> {
    let mut tests = Vec::new();
    let mut test_counter = 0;

    // If no fields, generate a basic test for the fixed encoding
    if enc_analysis.fields.is_empty() {
        let encoding = enc_analysis.opcode_pattern.fixed_value;
        let provenance = Provenance::new(
            instr.name.as_str(),
            &enc_analysis.name,
            "fixed encoding (no variable fields)",
            Requirement::BasicEncoding,
            "instruction with no variable fields".to_string(),
        );

        tests.push(EncodingTest {
            id: format!("{}_basic_encoding", enc_analysis.name),
            provenance,
            description: format!("Test {} basic encoding", enc_analysis.name),
            encoding_name: enc_analysis.name.clone(),
            iset: enc_analysis.iset,
            encoding,
            encoding_width: enc_analysis.opcode_pattern.width,
            field_values: HashMap::new(),
            expected_fields: HashMap::new(),
            expected_decode: HashMap::new(),
            test_type: EncodingTestType::Valid,
            expected_result: ExpectedResult::Pass,
        });

        return tests;
    }

    for field in &enc_analysis.fields {
        let boundary_values = boundary::generate_boundary_values(field);

        for bv in boundary_values {
            test_counter += 1;

            // Build encoding with this field value, others at 0
            let mut field_values = HashMap::new();
            for f in &enc_analysis.fields {
                if f.name == field.name {
                    field_values.insert(f.name.clone(), bv.value);
                } else {
                    field_values.insert(f.name.clone(), 0);
                }
            }

            let encoding = build_encoding_from_fields(
                &enc_analysis.opcode_pattern,
                &enc_analysis.fields,
                &field_values,
            );

            let provenance = Provenance::new(
                instr.name.as_str(),
                &enc_analysis.name,
                format!("field {} {} +: {}", field.name, field.start, field.width),
                Requirement::FieldBoundary {
                    field: field.name.clone(),
                    value: bv.value,
                    boundary: bv.boundary_type.clone(),
                },
                bv.provenance_note.clone(),
            );

            tests.push(EncodingTest {
                id: format!(
                    "{}_field_{}_{}_{}_{:x}",
                    enc_analysis.name,
                    field.name,
                    bv.value,
                    bv.boundary_type.short_name(),
                    enc_analysis.opcode_pattern.fixed_value & 0xFFFF
                ),
                provenance,
                description: format!(
                    "Test {} field {} = {} ({:?})",
                    enc_analysis.name, field.name, bv.value, bv.boundary_type
                ),
                encoding_name: enc_analysis.name.clone(),
                iset: enc_analysis.iset,
                encoding,
                encoding_width: enc_analysis.opcode_pattern.width,
                field_values: field_values.clone(),
                expected_fields: field_values,
                expected_decode: HashMap::new(),
                test_type: EncodingTestType::Boundary {
                    field: field.name.clone(),
                    boundary: bv.boundary_type,
                },
                expected_result: ExpectedResult::Pass,
            });
        }
    }

    tests
}

/// Generate field combination tests (pairwise coverage)
pub fn generate_field_combination_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    config: &TestGenConfig,
) -> Vec<EncodingTest> {
    let mut tests = Vec::new();

    let combinations =
        boundary::generate_field_combinations(&enc_analysis.fields, config.max_encoding_tests);

    for (combo_idx, combo) in combinations.iter().enumerate() {
        let mut field_values = HashMap::new();
        let mut provenance_notes = Vec::new();

        for (name, value, note) in combo {
            field_values.insert(name.clone(), *value);
            if !note.contains("neutral") {
                provenance_notes.push(format!("{}={} ({})", name, value, note));
            }
        }

        let encoding = build_encoding_from_fields(
            &enc_analysis.opcode_pattern,
            &enc_analysis.fields,
            &field_values,
        );

        let provenance = Provenance::new(
            instr.name.as_str(),
            &enc_analysis.name,
            format!("field combination {}", combo_idx),
            Requirement::FieldExtraction {
                field: "combination".to_string(),
                bit_start: 0,
                bit_width: enc_analysis.opcode_pattern.width,
            },
            provenance_notes.join(", "),
        );

        tests.push(EncodingTest {
            id: format!(
                "{}_combo_{}_{:x}",
                enc_analysis.name,
                combo_idx,
                enc_analysis.opcode_pattern.fixed_value & 0xFFFF
            ),
            provenance,
            description: format!(
                "Test {} field combination: {}",
                enc_analysis.name,
                combo
                    .iter()
                    .map(|(n, v, _)| format!("{}={}", n, v))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            encoding_name: enc_analysis.name.clone(),
            iset: enc_analysis.iset,
            encoding,
            encoding_width: enc_analysis.opcode_pattern.width,
            field_values: field_values.clone(),
            expected_fields: field_values,
            expected_decode: HashMap::new(),
            test_type: EncodingTestType::Valid,
            expected_result: ExpectedResult::Pass,
        });
    }

    tests
}

/// Generate special value tests (ZR, SP, etc.)
pub fn generate_special_value_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
) -> Vec<EncodingTest> {
    let mut tests = Vec::new();

    for field in &enc_analysis.fields {
        for special in &field.special_values {
            if !special.generates_different_path {
                continue;
            }

            let mut field_values = HashMap::new();
            for f in &enc_analysis.fields {
                if f.name == field.name {
                    field_values.insert(f.name.clone(), special.value);
                } else {
                    field_values.insert(f.name.clone(), 0);
                }
            }

            let encoding = build_encoding_from_fields(
                &enc_analysis.opcode_pattern,
                &enc_analysis.fields,
                &field_values,
            );

            let provenance = Provenance::new(
                instr.name.as_str(),
                &enc_analysis.name,
                format!(
                    "field {} = {} ({})",
                    field.name, special.value, special.meaning
                ),
                Requirement::FieldSpecial {
                    field: field.name.clone(),
                    value: special.value,
                    meaning: special.meaning.clone(),
                },
                special.meaning.clone(),
            );

            tests.push(EncodingTest {
                id: format!(
                    "{}_special_{}_{}_{}_{}",
                    enc_analysis.name,
                    field.name,
                    special.value,
                    special
                        .meaning
                        .replace(' ', "_")
                        .replace('/', "_")
                        .replace('(', "")
                        .replace(')', "")
                        .replace('-', "_"),
                    enc_analysis.opcode_pattern.fixed_value & 0xFFFF
                ),
                provenance,
                description: format!(
                    "Test {} special value {} = {} ({})",
                    enc_analysis.name, field.name, special.value, special.meaning
                ),
                encoding_name: enc_analysis.name.clone(),
                iset: enc_analysis.iset,
                encoding,
                encoding_width: enc_analysis.opcode_pattern.width,
                field_values: field_values.clone(),
                expected_fields: field_values,
                expected_decode: HashMap::new(),
                test_type: EncodingTestType::Special {
                    field: field.name.clone(),
                    meaning: special.meaning.clone(),
                },
                expected_result: ExpectedResult::Pass,
            });
        }
    }

    tests
}

/// Generate guard condition tests
pub fn generate_guard_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
) -> Vec<EncodingTest> {
    let mut tests = Vec::new();

    if let Some(guard) = &enc_analysis.guard {
        // Generate a test that satisfies the guard (should pass)
        // and a test that violates the guard (should be UNDEFINED)

        // For complex guards, we analyze the constraints
        for (idx, constraint) in guard.constraints.iter().enumerate() {
            match constraint {
                GuardConstraint::FieldEquals { field, value } => {
                    // Test with correct value (pass)
                    let mut field_values = HashMap::new();
                    for f in &enc_analysis.fields {
                        if f.name == *field {
                            field_values.insert(f.name.clone(), *value);
                        } else {
                            field_values.insert(f.name.clone(), 0);
                        }
                    }

                    let encoding = build_encoding_from_fields(
                        &enc_analysis.opcode_pattern,
                        &enc_analysis.fields,
                        &field_values,
                    );

                    let provenance = Provenance::new(
                        instr.name.as_str(),
                        &enc_analysis.name,
                        format!("guard: {} == {}", field, value),
                        Requirement::GuardCondition {
                            condition: format!("{} == {}", field, value),
                            triggers_undefined: false,
                        },
                        format!("guard satisfied: {} = {}", field, value),
                    );

                    tests.push(EncodingTest {
                        id: format!(
                            "{}_guard_{}_pass_{:x}",
                            enc_analysis.name,
                            idx,
                            enc_analysis.opcode_pattern.fixed_value & 0xFFFF
                        ),
                        provenance,
                        description: format!(
                            "Test {} guard: {} = {} (pass)",
                            enc_analysis.name, field, value
                        ),
                        encoding_name: enc_analysis.name.clone(),
                        iset: enc_analysis.iset,
                        encoding,
                        encoding_width: enc_analysis.opcode_pattern.width,
                        field_values: field_values.clone(),
                        expected_fields: field_values,
                        expected_decode: HashMap::new(),
                        test_type: EncodingTestType::Guard,
                        expected_result: ExpectedResult::Pass,
                    });

                    // Test with incorrect value (UNDEFINED)
                    let wrong_value = if *value == 0 { 1 } else { 0 };
                    let mut wrong_field_values = HashMap::new();
                    for f in &enc_analysis.fields {
                        if f.name == *field {
                            wrong_field_values.insert(f.name.clone(), wrong_value);
                        } else {
                            wrong_field_values.insert(f.name.clone(), 0);
                        }
                    }

                    let wrong_encoding = build_encoding_from_fields(
                        &enc_analysis.opcode_pattern,
                        &enc_analysis.fields,
                        &wrong_field_values,
                    );

                    let provenance = Provenance::new(
                        instr.name.as_str(),
                        &enc_analysis.name,
                        format!("guard: {} == {} violated", field, value),
                        Requirement::GuardCondition {
                            condition: format!("{} == {}", field, value),
                            triggers_undefined: true,
                        },
                        format!(
                            "guard violated: {} = {} (expected {})",
                            field, wrong_value, value
                        ),
                    );

                    tests.push(EncodingTest {
                        id: format!(
                            "{}_guard_{}_fail_{:x}",
                            enc_analysis.name,
                            idx,
                            enc_analysis.opcode_pattern.fixed_value & 0xFFFF
                        ),
                        provenance,
                        description: format!(
                            "Test {} guard violation: {} = {} (expected {})",
                            enc_analysis.name, field, wrong_value, value
                        ),
                        encoding_name: enc_analysis.name.clone(),
                        iset: enc_analysis.iset,
                        encoding: wrong_encoding,
                        encoding_width: enc_analysis.opcode_pattern.width,
                        field_values: wrong_field_values.clone(),
                        expected_fields: wrong_field_values,
                        expected_decode: HashMap::new(),
                        test_type: EncodingTestType::Invalid,
                        expected_result: ExpectedResult::Undefined,
                    });
                }
                _ => {
                    // Complex constraints - generate a note but skip for now
                }
            }
        }
    }

    tests
}

/// Generate invalid encoding tests (UNDEFINED conditions)
pub fn generate_invalid_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
) -> Vec<EncodingTest> {
    let mut tests = Vec::new();

    for (idx, invalid) in enc_analysis.invalid_conditions.iter().enumerate() {
        // Try to generate an encoding that triggers this invalid condition
        // This requires parsing the condition to determine field values

        let mut field_values = HashMap::new();
        for f in &enc_analysis.fields {
            // Check if this field is mentioned in the constraints
            let value = invalid
                .constraints
                .iter()
                .find_map(|c| match c {
                    FieldConstraint::Equals(v) => Some(*v),
                    _ => None,
                })
                .unwrap_or(0);
            field_values.insert(f.name.clone(), value);
        }

        let encoding = build_encoding_from_fields(
            &enc_analysis.opcode_pattern,
            &enc_analysis.fields,
            &field_values,
        );

        let provenance = Provenance::new(
            instr.name.as_str(),
            &enc_analysis.name,
            &invalid.description,
            Requirement::UndefinedEncoding {
                condition: invalid.description.clone(),
            },
            format!("triggers {:?}", invalid.result),
        );

        let expected = match invalid.result {
            InvalidResult::Undefined => ExpectedResult::Undefined,
            InvalidResult::Unpredictable => ExpectedResult::Unpredictable,
            InvalidResult::Unallocated => ExpectedResult::Unallocated,
            InvalidResult::ConstraintViolation => ExpectedResult::Undefined,
        };

        tests.push(EncodingTest {
            id: format!(
                "{}_invalid_{}_{:x}",
                enc_analysis.name,
                idx,
                enc_analysis.opcode_pattern.fixed_value & 0xFFFF
            ),
            provenance,
            description: format!(
                "Test {} invalid encoding: {}",
                enc_analysis.name, invalid.description
            ),
            encoding_name: enc_analysis.name.clone(),
            iset: enc_analysis.iset,
            encoding,
            encoding_width: enc_analysis.opcode_pattern.width,
            field_values: field_values.clone(),
            expected_fields: field_values,
            expected_decode: HashMap::new(),
            test_type: EncodingTestType::Invalid,
            expected_result: expected,
        });
    }

    tests
}

/// Generate unpredictable bit pattern tests
pub fn generate_unpredictable_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
) -> Vec<EncodingTest> {
    let mut tests = Vec::new();

    for unpred in &enc_analysis.unpredictable_bits {
        // Generate encoding with the "wrong" bit value
        let wrong_value = !unpred.expected;

        let mut field_values = HashMap::new();
        for f in &enc_analysis.fields {
            field_values.insert(f.name.clone(), 0);
        }

        let mut encoding = build_encoding_from_fields(
            &enc_analysis.opcode_pattern,
            &enc_analysis.fields,
            &field_values,
        );

        // Set the unpredictable bit to the wrong value
        if wrong_value {
            encoding |= 1 << unpred.index;
        } else {
            encoding &= !(1 << unpred.index);
        }

        let provenance = Provenance::new(
            instr.name.as_str(),
            &enc_analysis.name,
            format!(
                "unpredictable_unless {} == '{}'",
                unpred.index,
                if unpred.expected { "1" } else { "0" }
            ),
            Requirement::UnpredictableBit {
                index: unpred.index,
                expected: unpred.expected,
            },
            format!(
                "bit {} is {} (expected {})",
                unpred.index,
                if wrong_value { "1" } else { "0" },
                if unpred.expected { "1" } else { "0" }
            ),
        );

        tests.push(EncodingTest {
            id: format!(
                "{}_unpredictable_bit_{}_{:x}",
                enc_analysis.name,
                unpred.index,
                enc_analysis.opcode_pattern.fixed_value & 0xFFFF
            ),
            provenance,
            description: format!(
                "Test {} unpredictable: bit {} should be {}",
                enc_analysis.name,
                unpred.index,
                if unpred.expected { "1" } else { "0" }
            ),
            encoding_name: enc_analysis.name.clone(),
            iset: enc_analysis.iset,
            encoding,
            encoding_width: enc_analysis.opcode_pattern.width,
            field_values,
            expected_fields: HashMap::new(),
            expected_decode: HashMap::new(),
            test_type: EncodingTestType::Unpredictable,
            expected_result: ExpectedResult::Unpredictable,
        });
    }

    tests
}

/// Build an encoding value from opcode pattern and field values
fn build_encoding_from_fields(
    opcode_pattern: &OpcodePattern,
    fields: &[FieldAnalysis],
    field_values: &HashMap<String, u64>,
) -> u64 {
    let mut encoding = opcode_pattern.fixed_value;

    for field in fields {
        if let Some(&value) = field_values.get(&field.name) {
            let mask = ((1u64 << field.width) - 1) << field.start;
            encoding = (encoding & !mask) | ((value << field.start) & mask);
        }
    }

    encoding
}
