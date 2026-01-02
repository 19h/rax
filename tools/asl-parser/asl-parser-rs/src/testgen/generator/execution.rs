//! Execution test case generation.
//!
//! Generates tests for:
//! - Register operations (read/write correctness)
//! - Flag computations (N, Z, C, V)
//! - Memory operations (load/store)
//! - Exception paths

use std::collections::HashMap;

use crate::syntax::Instruction;
use crate::testgen::generator::boundary::{self, FlagTestValue};
use crate::testgen::oracle::eval::{
    identify_a32_pattern, identify_a64_pattern, identify_thumb_pattern, BitOpType, BitfieldOp,
    CondSelectOp, InstructionPattern, LoadStoreAddressing, LogicalOp, MoveOp, ShiftType,
    TestEvaluator,
};
use crate::testgen::types::*;

/// Generate register operation tests
pub fn generate_register_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    semantics: &SemanticAnalysis,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();

    // For each register write, generate tests to verify it
    for (write_idx, write) in semantics.register_writes.iter().enumerate() {
        // Generate a test that verifies this register is written
        let mut initial_state = ProcessorState::default();
        let mut expected_state = ProcessorState::default();

        // Set up initial register values
        for read in &semantics.register_reads {
            match &read.register {
                RegisterId::GpFromField(field) => {
                    // Set initial value for source register
                    initial_state.gp_regs.insert(0, 0x1234_5678_9ABC_DEF0);
                }
                RegisterId::Sp => {
                    initial_state.sp = Some(0x0000_8000_0000_0000);
                }
                _ => {}
            }
        }

        // Build a basic encoding
        let mut field_values = HashMap::new();
        for f in &enc_analysis.fields {
            field_values.insert(f.name.clone(), 0);
        }

        let encoding = build_encoding_value(
            &enc_analysis.opcode_pattern,
            &enc_analysis.fields,
            &field_values,
        );

        let provenance = Provenance::new(
            instr.name.as_str(),
            &enc_analysis.name,
            format!("{:?} write", write.register),
            Requirement::RegisterWrite {
                reg_type: match write.width {
                    64 => RegType::Gp64,
                    32 => RegType::Gp32,
                    128 => RegType::Simd128,
                    _ => RegType::Gp64,
                },
                dest_field: match &write.register {
                    RegisterId::GpFromField(f) => f.clone(),
                    _ => "unknown".to_string(),
                },
            },
            format!("verify register write to {:?}", write.register),
        );

        tests.push(ExecutionTest {
            id: format!("{}_reg_write_{}", enc_analysis.name, write_idx),
            provenance,
            description: format!(
                "Test {} register write: {:?}",
                enc_analysis.name, write.register
            ),
            encoding_name: enc_analysis.name.clone(),
            iset: enc_analysis.iset,
            encoding,
            encoding_width: enc_analysis.opcode_pattern.width,
            field_values,
            initial_state,
            expected_state,
            expected_memory: vec![],
            assertions: vec![],
            path_id: None,
            category: ExecutionTestCategory::Normal,
        });
    }

    // Generate special register tests (ZR, SP)
    for field in &enc_analysis.fields {
        match &field.semantics {
            FieldSemantics::GpRegister {
                allows_zr,
                allows_sp,
                ..
            } => {
                if *allows_zr && field.width == 5 {
                    // Test ZR behavior (reads as 0, writes discarded)
                    let mut field_values = HashMap::new();
                    for f in &enc_analysis.fields {
                        if f.name == field.name {
                            field_values.insert(f.name.clone(), 31); // ZR
                        } else {
                            field_values.insert(f.name.clone(), 0);
                        }
                    }

                    let encoding = build_encoding_value(
                        &enc_analysis.opcode_pattern,
                        &enc_analysis.fields,
                        &field_values,
                    );

                    let provenance = Provenance::new(
                        instr.name.as_str(),
                        &enc_analysis.name,
                        format!("{} = 31 (ZR)", field.name),
                        Requirement::RegisterSpecial {
                            reg: SpecialReg::Zr,
                            behavior: "reads as 0, writes discarded".to_string(),
                        },
                        format!("zero register ({} = 31)", field.name),
                    );

                    tests.push(ExecutionTest {
                        id: format!("{}_zr_{}", enc_analysis.name, field.name),
                        provenance,
                        description: format!(
                            "Test {} with {} = ZR (31)",
                            enc_analysis.name, field.name
                        ),
                        encoding_name: enc_analysis.name.clone(),
                        iset: enc_analysis.iset,
                        encoding,
                        encoding_width: enc_analysis.opcode_pattern.width,
                        field_values,
                        initial_state: ProcessorState::default(),
                        expected_state: ProcessorState::default(),
                        expected_memory: vec![],
                        assertions: vec![TestAssertion {
                            check: AssertionCheck::GpReg(31),
                            expected: AssertionValue::Zero,
                            message: "XZR should always be 0".to_string(),
                        }],
                        path_id: None,
                        category: ExecutionTestCategory::Normal,
                    });
                }

                if *allows_sp && field.width == 5 {
                    // Test SP behavior (alignment check, etc.)
                    let mut field_values = HashMap::new();
                    for f in &enc_analysis.fields {
                        if f.name == field.name {
                            field_values.insert(f.name.clone(), 31); // SP
                        } else {
                            field_values.insert(f.name.clone(), 0);
                        }
                    }

                    let encoding = build_encoding_value(
                        &enc_analysis.opcode_pattern,
                        &enc_analysis.fields,
                        &field_values,
                    );

                    let mut initial_state = ProcessorState::default();
                    initial_state.sp = Some(0x0000_8000_0000_0000); // Aligned

                    let provenance = Provenance::new(
                        instr.name.as_str(),
                        &enc_analysis.name,
                        format!("{} = 31 (SP)", field.name),
                        Requirement::RegisterSpecial {
                            reg: SpecialReg::Sp,
                            behavior: "stack pointer with alignment requirements".to_string(),
                        },
                        format!("stack pointer ({} = 31)", field.name),
                    );

                    tests.push(ExecutionTest {
                        id: format!("{}_sp_{}", enc_analysis.name, field.name),
                        provenance,
                        description: format!(
                            "Test {} with {} = SP (31)",
                            enc_analysis.name, field.name
                        ),
                        encoding_name: enc_analysis.name.clone(),
                        iset: enc_analysis.iset,
                        encoding,
                        encoding_width: enc_analysis.opcode_pattern.width,
                        field_values,
                        initial_state,
                        expected_state: ProcessorState::default(),
                        expected_memory: vec![],
                        assertions: vec![],
                        path_id: None,
                        category: ExecutionTestCategory::Normal,
                    });
                }
            }
            _ => {}
        }
    }

    tests
}

/// Generate flag computation tests
pub fn generate_flag_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    semantics: &SemanticAnalysis,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();

    // Only generate flag tests for instructions that set flags
    if semantics.flag_effects.is_empty() {
        return tests;
    }

    let flag_values = boundary::generate_flag_test_values();

    for (idx, fv) in flag_values.iter().enumerate() {
        let mut initial_state = ProcessorState::default();
        initial_state.gp_regs.insert(1, fv.operand1); // Source operand 1
        initial_state.gp_regs.insert(2, fv.operand2); // Source operand 2

        let mut expected_state = ProcessorState::default();
        expected_state.nzcv = Some(fv.expected_flags.to_nzcv());

        // Build encoding with typical field values
        let mut field_values = HashMap::new();
        for f in &enc_analysis.fields {
            match f.name.as_str() {
                "Rd" | "Wd" => field_values.insert(f.name.clone(), 0), // Dest = X0
                "Rn" | "Wn" => field_values.insert(f.name.clone(), 1), // Src1 = X1
                "Rm" | "Wm" => field_values.insert(f.name.clone(), 2), // Src2 = X2
                "S" => field_values.insert(f.name.clone(), 1), // Set S bit for flag-setting variant
                _ => field_values.insert(f.name.clone(), 0),
            };
        }

        let encoding = build_encoding_value(
            &enc_analysis.opcode_pattern,
            &enc_analysis.fields,
            &field_values,
        );

        let provenance = Provenance::new(
            instr.name.as_str(),
            &enc_analysis.name,
            "if setflags then PSTATE.<N,Z,C,V> = nzcv",
            Requirement::FlagComputation {
                flag: ProcessorFlag::N, // Will test all
                scenario: fv.scenario.clone(),
            },
            fv.note.clone(),
        );

        let mut assertions = Vec::new();
        for effect in &semantics.flag_effects {
            let expected = match effect.flag {
                ProcessorFlag::N => fv.expected_flags.n,
                ProcessorFlag::Z => fv.expected_flags.z,
                ProcessorFlag::C => fv.expected_flags.c,
                ProcessorFlag::V => fv.expected_flags.v,
            };
            assertions.push(TestAssertion {
                check: AssertionCheck::Flag(effect.flag),
                expected: AssertionValue::Bool(expected),
                message: format!("{:?} should be {}", effect.flag, expected),
            });
        }

        tests.push(ExecutionTest {
            id: format!("{}_flags_{:?}_{}", enc_analysis.name, fv.scenario, idx),
            provenance,
            description: format!(
                "Test {} flag computation: {:?}",
                enc_analysis.name, fv.scenario
            ),
            encoding_name: enc_analysis.name.clone(),
            iset: enc_analysis.iset,
            encoding,
            encoding_width: enc_analysis.opcode_pattern.width,
            field_values,
            initial_state,
            expected_state,
            expected_memory: vec![],
            assertions,
            path_id: None,
            category: ExecutionTestCategory::Flags,
        });
    }

    tests
}

/// Generate memory operation tests
pub fn generate_memory_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    semantics: &SemanticAnalysis,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();

    // Generate load tests
    for (idx, read) in semantics.memory_reads.iter().enumerate() {
        let mut initial_state = ProcessorState::default();

        // Set up base register with address
        let test_addr = 0x0000_1000_0000_0000u64;
        initial_state.gp_regs.insert(1, test_addr); // Base register

        // Set up memory with test data
        let test_data = match read.size {
            1 => vec![0xAB],
            2 => vec![0xCD, 0xAB],
            4 => vec![0xEF, 0xCD, 0xAB, 0x12],
            8 => vec![0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF],
            _ => vec![0xAB; read.size as usize],
        };
        initial_state.memory.insert(test_addr, test_data);

        let mut field_values = HashMap::new();
        for f in &enc_analysis.fields {
            field_values.insert(f.name.clone(), if f.name == "Rn" { 1 } else { 0 });
        }

        let encoding = build_encoding_value(
            &enc_analysis.opcode_pattern,
            &enc_analysis.fields,
            &field_values,
        );

        let provenance = Provenance::new(
            instr.name.as_str(),
            &enc_analysis.name,
            format!("Mem[address, {}]", read.size),
            Requirement::MemoryAccess {
                op: MemOp::Load,
                size_bits: read.size * 8,
                addressing: format!("{:?}", read.addressing),
            },
            format!("{}-byte load", read.size),
        );

        tests.push(ExecutionTest {
            id: format!("{}_load_{}", enc_analysis.name, idx),
            provenance,
            description: format!(
                "Test {} memory load: {} bytes",
                enc_analysis.name, read.size
            ),
            encoding_name: enc_analysis.name.clone(),
            iset: enc_analysis.iset,
            encoding,
            encoding_width: enc_analysis.opcode_pattern.width,
            field_values,
            initial_state,
            expected_state: ProcessorState::default(),
            expected_memory: vec![],
            assertions: vec![],
            path_id: None,
            category: ExecutionTestCategory::Memory,
        });
    }

    // Generate store tests
    for (idx, write) in semantics.memory_writes.iter().enumerate() {
        let mut initial_state = ProcessorState::default();

        // Set up base register with address
        let test_addr = 0x0000_1000_0000_0000u64;
        initial_state.gp_regs.insert(1, test_addr); // Base register
        initial_state.gp_regs.insert(0, 0xDEAD_BEEF_CAFE_BABEu64); // Data to store

        let mut field_values = HashMap::new();
        for f in &enc_analysis.fields {
            match f.name.as_str() {
                "Rn" => field_values.insert(f.name.clone(), 1),
                "Rt" => field_values.insert(f.name.clone(), 0),
                _ => field_values.insert(f.name.clone(), 0),
            };
        }

        let encoding = build_encoding_value(
            &enc_analysis.opcode_pattern,
            &enc_analysis.fields,
            &field_values,
        );

        let expected_data = match write.size {
            1 => vec![0xBE],
            2 => vec![0xBE, 0xBA],
            4 => vec![0xBE, 0xBA, 0xFE, 0xCA],
            8 => vec![0xBE, 0xBA, 0xFE, 0xCA, 0xEF, 0xBE, 0xAD, 0xDE],
            _ => vec![0; write.size as usize],
        };

        let provenance = Provenance::new(
            instr.name.as_str(),
            &enc_analysis.name,
            format!("Mem[address, {}] = data", write.size),
            Requirement::MemoryAccess {
                op: MemOp::Store,
                size_bits: write.size * 8,
                addressing: format!("{:?}", write.addressing),
            },
            format!("{}-byte store", write.size),
        );

        tests.push(ExecutionTest {
            id: format!("{}_store_{}", enc_analysis.name, idx),
            provenance,
            description: format!(
                "Test {} memory store: {} bytes",
                enc_analysis.name, write.size
            ),
            encoding_name: enc_analysis.name.clone(),
            iset: enc_analysis.iset,
            encoding,
            encoding_width: enc_analysis.opcode_pattern.width,
            field_values,
            initial_state,
            expected_state: ProcessorState::default(),
            expected_memory: vec![MemoryChange {
                address: test_addr,
                value: expected_data,
                size: write.size,
            }],
            assertions: vec![],
            path_id: None,
            category: ExecutionTestCategory::Memory,
        });
    }

    tests
}

/// Generate exception path tests
pub fn generate_exception_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    exceptions: &[ExceptionCondition],
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();

    for (idx, exc) in exceptions.iter().enumerate() {
        let mut field_values = HashMap::new();
        for f in &enc_analysis.fields {
            field_values.insert(f.name.clone(), 0);
        }

        let encoding = build_encoding_value(
            &enc_analysis.opcode_pattern,
            &enc_analysis.fields,
            &field_values,
        );

        let provenance = Provenance::new(
            instr.name.as_str(),
            &enc_analysis.name,
            &exc.trigger,
            Requirement::UndefinedEncoding {
                condition: exc.trigger.clone(),
            },
            format!("triggers {:?}", exc.exception_type),
        );

        tests.push(ExecutionTest {
            id: format!("{}_exception_{}", enc_analysis.name, idx),
            provenance,
            description: format!(
                "Test {} exception: {:?}",
                enc_analysis.name, exc.exception_type
            ),
            encoding_name: enc_analysis.name.clone(),
            iset: enc_analysis.iset,
            encoding,
            encoding_width: enc_analysis.opcode_pattern.width,
            field_values,
            initial_state: ProcessorState::default(),
            expected_state: ProcessorState::default(),
            expected_memory: vec![],
            assertions: vec![],
            path_id: None,
            category: ExecutionTestCategory::Exception,
        });
    }

    tests
}

/// Build an encoding value from opcode pattern, fields, and field values
fn build_encoding_value(
    opcode_pattern: &OpcodePattern,
    fields: &[FieldAnalysis],
    field_values: &HashMap<String, u64>,
) -> u64 {
    // Start with the fixed bits from the opcode pattern
    let mut encoding = opcode_pattern.fixed_value;

    // Apply field values
    for field in fields {
        if let Some(&value) = field_values.get(&field.name) {
            let mask = ((1u64 << field.width) - 1) << field.start;
            encoding = (encoding & !mask) | ((value << field.start) & mask);
        }
    }

    encoding
}

// ============================================================================
// Oracle-based Test Generation (with expected values)
// ============================================================================

/// Generate tests with expected values computed by the oracle.
///
/// This produces tests that verify actual register/flag values after execution,
/// not just that the instruction doesn't fault.
pub fn generate_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    semantics: &SemanticAnalysis,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();
    let evaluator = TestEvaluator::new();

    // Determine instruction pattern based on instruction set
    let pattern = match enc_analysis.iset {
        crate::syntax::InstructionSet::A64 => {
            identify_a64_pattern(enc_analysis.opcode_pattern.fixed_value)
        }
        crate::syntax::InstructionSet::A32 => {
            identify_a32_pattern(enc_analysis.opcode_pattern.fixed_value)
        }
        crate::syntax::InstructionSet::T32 => {
            identify_thumb_pattern(enc_analysis.opcode_pattern.fixed_value, true)
        }
        crate::syntax::InstructionSet::T16 => {
            identify_thumb_pattern(enc_analysis.opcode_pattern.fixed_value, false)
        }
    };

    match pattern {
        InstructionPattern::AddSubImmediate { is_sub, set_flags } => {
            tests.extend(generate_add_sub_imm_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                is_sub,
                set_flags,
            ));
        }
        InstructionPattern::LogicalImmediate { op } => {
            tests.extend(generate_logical_imm_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                op,
            ));
        }
        InstructionPattern::LogicalShiftedReg { op } => {
            tests.extend(generate_logical_shifted_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                op,
            ));
        }
        InstructionPattern::AddSubShiftedReg { is_sub, set_flags } => {
            tests.extend(generate_add_sub_shifted_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                is_sub,
                set_flags,
            ));
        }
        InstructionPattern::MoveImmediate { op } => {
            tests.extend(generate_move_imm_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                op,
            ));
        }
        InstructionPattern::ShiftVariable { shift_type } => {
            tests.extend(generate_shift_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                shift_type,
            ));
        }
        InstructionPattern::Multiply { accumulate, negate } => {
            tests.extend(generate_multiply_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                accumulate,
                negate,
            ));
        }
        InstructionPattern::MultiplyLong {
            signed,
            accumulate,
            negate,
        } => {
            tests.extend(generate_multiply_long_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                signed,
                accumulate,
                negate,
            ));
        }
        InstructionPattern::MultiplyHigh { signed } => {
            tests.extend(generate_multiply_high_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                signed,
            ));
        }
        InstructionPattern::Divide { signed } => {
            tests.extend(generate_divide_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                signed,
            ));
        }
        InstructionPattern::Bitfield { op } => {
            tests.extend(generate_bitfield_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                op,
            ));
        }
        InstructionPattern::Extract => {
            tests.extend(generate_extract_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
            ));
        }
        InstructionPattern::BitOp { op } => {
            tests.extend(generate_bitop_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                op,
            ));
        }
        InstructionPattern::ConditionalSelect { op } => {
            tests.extend(generate_cond_select_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                op,
            ));
        }
        InstructionPattern::Load {
            size,
            signed,
            addressing,
        } => {
            tests.extend(generate_load_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                size,
                signed,
                addressing,
            ));
        }
        InstructionPattern::Store { size, addressing } => {
            tests.extend(generate_store_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                size,
                addressing,
            ));
        }
        InstructionPattern::Branch { link } => {
            tests.extend(generate_branch_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                link,
            ));
        }
        InstructionPattern::CompareBranch { nonzero } => {
            tests.extend(generate_compare_branch_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                nonzero,
            ));
        }
        _ => {
            // Unknown pattern - skip oracle tests for A64
        }
    }

    // For A32/T32/T16, also try generic pattern-based generation
    match enc_analysis.iset {
        crate::syntax::InstructionSet::A32 => {
            tests.extend(generate_a32_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                &pattern,
            ));
        }
        crate::syntax::InstructionSet::T32 => {
            tests.extend(generate_t32_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                &pattern,
            ));
        }
        crate::syntax::InstructionSet::T16 => {
            tests.extend(generate_t16_oracle_tests(
                instr,
                enc_analysis,
                &evaluator,
                &pattern,
            ));
        }
        _ => {}
    }

    tests
}

/// Generate oracle tests for ADD/SUB immediate instructions
fn generate_add_sub_imm_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    evaluator: &TestEvaluator,
    is_sub: bool,
    set_flags: bool,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();
    let op_name = match (is_sub, set_flags) {
        (false, false) => "ADD",
        (false, true) => "ADDS",
        (true, false) => "SUB",
        (true, true) => "SUBS",
    };

    // Test cases: (operand1, imm12, shift, description)
    let test_cases: Vec<(u64, u64, u64, &str)> = vec![
        // Basic arithmetic
        (100, 10, 0, "simple addition/subtraction"),
        (0, 0, 0, "zero operands"),
        (1, 1, 0, "small values"),
        // Boundary values
        (0, 0xFFF, 0, "max imm12 unshifted"),
        (0, 0xFFF, 1, "max imm12 shifted"),
        (0xFFFF_FFFF_FFFF_FFFF, 1, 0, "max u64 operand"),
        // Flag-triggering cases
        (0, 1, 0, "zero result (for sub 1-1)"),
        (
            0x7FFF_FFFF_FFFF_FFFF,
            1,
            0,
            "signed overflow boundary 64-bit",
        ),
        (0x7FFF_FFFF, 1, 0, "signed overflow boundary 32-bit"),
        (0xFFFF_FFFF_FFFF_FFFF, 1, 0, "unsigned overflow 64-bit"),
        (0xFFFF_FFFF, 1, 0, "unsigned overflow 32-bit"),
    ];

    for (test_idx, (operand1, imm12, sh, description)) in test_cases.iter().enumerate() {
        // Test both 32-bit and 64-bit variants
        for sf in [0u64, 1u64] {
            let width = if sf == 1 { 64 } else { 32 };

            // Build encoding
            let encoding = enc_analysis.opcode_pattern.fixed_value
                | (sf << 31)
                | ((is_sub as u64) << 30)
                | ((set_flags as u64) << 29)
                | (*sh << 22)
                | ((*imm12 & 0xFFF) << 10)
                | (1 << 5)  // Rn = X1
                | 0; // Rd = X0

            // Set up initial state
            let mut initial_state = ProcessorState::default();
            let masked_operand = if width == 32 {
                *operand1 & 0xFFFF_FFFF
            } else {
                *operand1
            };
            initial_state.gp_regs.insert(1, masked_operand);
            initial_state.sp = Some(0x0000_8000_0000_0000); // Aligned SP

            // Compute expected values using the evaluator
            let (expected_state, assertions) =
                evaluator.compute_add_sub_imm_expected(encoding, &initial_state);

            let size_suffix = if sf == 1 { "64" } else { "32" };
            let provenance = Provenance::new(
                instr.name.as_str(),
                &enc_analysis.name,
                format!(
                    "{} X{}, X{}, #{}{}",
                    op_name,
                    0,
                    1,
                    imm12,
                    if *sh == 1 { ", LSL #12" } else { "" }
                ),
                if set_flags {
                    Requirement::FlagComputation {
                        flag: ProcessorFlag::N,
                        scenario: FlagScenario::NonZeroResult,
                    }
                } else {
                    Requirement::RegisterWrite {
                        reg_type: if sf == 1 {
                            RegType::Gp64
                        } else {
                            RegType::Gp32
                        },
                        dest_field: "Rd".to_string(),
                    }
                },
                format!("{} ({})", description, size_suffix),
            );

            let mut field_values = HashMap::new();
            field_values.insert("sf".to_string(), sf);
            field_values.insert("op".to_string(), is_sub as u64);
            field_values.insert("S".to_string(), set_flags as u64);
            field_values.insert("sh".to_string(), *sh);
            field_values.insert("imm12".to_string(), *imm12);
            field_values.insert("Rn".to_string(), 1);
            field_values.insert("Rd".to_string(), 0);

            tests.push(ExecutionTest {
                id: format!(
                    "{}_{}_oracle_{}_{}",
                    enc_analysis.name,
                    op_name.to_lowercase(),
                    size_suffix,
                    test_idx
                ),
                provenance,
                description: format!(
                    "Test {} {}-bit: {} (with oracle verification)",
                    op_name, width, description
                ),
                encoding_name: enc_analysis.name.clone(),
                iset: enc_analysis.iset,
                encoding,
                encoding_width: enc_analysis.opcode_pattern.width,
                field_values,
                initial_state,
                expected_state,
                expected_memory: vec![],
                assertions,
                path_id: None,
                category: if set_flags {
                    ExecutionTestCategory::Flags
                } else {
                    ExecutionTestCategory::Normal
                },
            });
        }
    }

    // Generate ZR destination tests
    for sf in [0u64, 1u64] {
        let width = if sf == 1 { 64 } else { 32 };

        // Test Rd = 31 (ZR for ADDS/SUBS, SP for ADD/SUB)
        let encoding = enc_analysis.opcode_pattern.fixed_value
            | (sf << 31)
            | ((is_sub as u64) << 30)
            | ((set_flags as u64) << 29)
            | (0 << 22)  // sh = 0
            | (10 << 10) // imm12 = 10
            | (1 << 5)   // Rn = X1
            | 31; // Rd = 31

        let mut initial_state = ProcessorState::default();
        initial_state.gp_regs.insert(1, 100);
        initial_state.sp = Some(0x0000_8000_0000_0000);

        let (expected_state, assertions) =
            evaluator.compute_add_sub_imm_expected(encoding, &initial_state);

        let size_suffix = if sf == 1 { "64" } else { "32" };
        let dest_name = if set_flags { "ZR" } else { "SP" };

        let provenance = Provenance::new(
            instr.name.as_str(),
            &enc_analysis.name,
            format!("{} {}, X1, #10", op_name, dest_name),
            Requirement::RegisterSpecial {
                reg: if set_flags {
                    SpecialReg::Zr
                } else {
                    SpecialReg::Sp
                },
                behavior: if set_flags {
                    "result discarded, flags set".to_string()
                } else {
                    "writes to stack pointer".to_string()
                },
            },
            format!("{} destination ({})", dest_name, size_suffix),
        );

        let mut field_values = HashMap::new();
        field_values.insert("sf".to_string(), sf);
        field_values.insert("op".to_string(), is_sub as u64);
        field_values.insert("S".to_string(), set_flags as u64);
        field_values.insert("sh".to_string(), 0);
        field_values.insert("imm12".to_string(), 10);
        field_values.insert("Rn".to_string(), 1);
        field_values.insert("Rd".to_string(), 31);

        tests.push(ExecutionTest {
            id: format!(
                "{}_{}_oracle_{}_rd31_{}",
                enc_analysis.name,
                op_name.to_lowercase(),
                size_suffix,
                dest_name.to_lowercase()
            ),
            provenance,
            description: format!("Test {} {}-bit with Rd=31 ({})", op_name, width, dest_name),
            encoding_name: enc_analysis.name.clone(),
            iset: enc_analysis.iset,
            encoding,
            encoding_width: enc_analysis.opcode_pattern.width,
            field_values,
            initial_state,
            expected_state,
            expected_memory: vec![],
            assertions,
            path_id: None,
            category: ExecutionTestCategory::Normal,
        });
    }

    tests
}

/// Generate oracle tests for logical immediate instructions (AND, ORR, EOR, ANDS)
fn generate_logical_imm_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    _evaluator: &TestEvaluator,
    op: LogicalOp,
) -> Vec<ExecutionTest> {
    use crate::testgen::oracle::Oracle;

    let mut tests = Vec::new();
    let oracle = Oracle::new();

    let op_name = match op {
        LogicalOp::And => "AND",
        LogicalOp::Or => "ORR",
        LogicalOp::Xor => "EOR",
        LogicalOp::Ands => "ANDS",
    };
    let set_flags = matches!(op, LogicalOp::Ands);

    // opc encoding for logical immediate
    let opc: u64 = match op {
        LogicalOp::And => 0b00,
        LogicalOp::Or => 0b01,
        LogicalOp::Xor => 0b10,
        LogicalOp::Ands => 0b11,
    };

    // Test cases for 64-bit: (N, imms, immr, operand1, description)
    // These are valid bitmask encodings that produce useful patterns
    let test_cases_64: Vec<(u32, u32, u32, u64, &str)> = vec![
        // N=1 (64-bit element size)
        (1, 0b000111, 0, 0xFFFFFFFFFFFFFFFF, "mask lower 8 bits"), // mask = 0xFF
        (1, 0b001111, 0, 0xFFFFFFFFFFFFFFFF, "mask lower 16 bits"), // mask = 0xFFFF
        (1, 0b011111, 0, 0xFFFFFFFFFFFFFFFF, "mask lower 32 bits"), // mask = 0xFFFFFFFF
        (1, 0b000000, 0, 0xDEADBEEFCAFEBABE, "single bit mask"),   // mask = 0x1
        (1, 0b111110, 0, 0xAAAAAAAAAAAAAAAA, "all but MSB"),       // mask = 0x7FFF...
    ];

    // Test cases for 32-bit: (N, imms, immr, operand1, description)
    let test_cases_32: Vec<(u32, u32, u32, u64, &str)> = vec![
        (0, 0b000111, 0, 0xFFFFFFFF, "mask lower 8 bits"),
        (0, 0b001111, 0, 0xFFFFFFFF, "mask lower 16 bits"),
        (0, 0b000000, 0, 0xDEADBEEF, "single bit mask"),
    ];

    // Generate 64-bit tests
    for (test_idx, (n, imms, immr, operand1, description)) in test_cases_64.iter().enumerate() {
        let mask = match oracle.decode_bitmasks(*n, *imms, *immr, true) {
            Some(m) => m,
            None => continue,
        };

        let result = match op {
            LogicalOp::And | LogicalOp::Ands => operand1 & mask,
            LogicalOp::Or => operand1 | mask,
            LogicalOp::Xor => operand1 ^ mask,
        };

        // Encoding: sf(1) opc(2) 100100 N(1) immr(6) imms(6) Rn(5) Rd(5)
        let encoding: u64 = (1u64 << 31)
            | (opc << 29)
            | (0b100100u64 << 23)
            | ((*n as u64) << 22)
            | ((*immr as u64) << 16)
            | ((*imms as u64) << 10)
            | (1u64 << 5)  // Rn = X1
            | 0; // Rd = X0

        let mut initial_state = ProcessorState::default();
        initial_state.gp_regs.insert(1, *operand1);

        let mut expected_state = ProcessorState::default();
        expected_state.gp_regs.insert(0, result);

        let mut assertions = vec![TestAssertion {
            check: AssertionCheck::GpReg(0),
            expected: AssertionValue::U64(result),
            message: format!("X0 should be 0x{:016X}", result),
        }];

        if set_flags {
            assertions.push(TestAssertion {
                check: AssertionCheck::Flag(ProcessorFlag::N),
                expected: AssertionValue::Bool((result >> 63) != 0),
                message: format!("N flag"),
            });
            assertions.push(TestAssertion {
                check: AssertionCheck::Flag(ProcessorFlag::Z),
                expected: AssertionValue::Bool(result == 0),
                message: format!("Z flag"),
            });
        }

        let provenance = Provenance::new(
            instr.name.as_str(),
            &enc_analysis.name,
            format!("{} X0, X1, #0x{:X}", op_name, mask),
            Requirement::RegisterWrite {
                reg_type: RegType::Gp64,
                dest_field: "Rd".to_string(),
            },
            format!("{} (64)", description),
        );

        let mut field_values = HashMap::new();
        field_values.insert("sf".to_string(), 1);
        field_values.insert("N".to_string(), *n as u64);
        field_values.insert("imms".to_string(), *imms as u64);
        field_values.insert("immr".to_string(), *immr as u64);
        field_values.insert("Rn".to_string(), 1);
        field_values.insert("Rd".to_string(), 0);

        tests.push(ExecutionTest {
            id: format!(
                "{}_{}_oracle_64_{}",
                enc_analysis.name,
                op_name.to_lowercase(),
                test_idx
            ),
            provenance,
            description: format!("Test {} 64-bit: {} (oracle)", op_name, description),
            encoding_name: enc_analysis.name.clone(),
            iset: enc_analysis.iset,
            encoding,
            encoding_width: enc_analysis.opcode_pattern.width,
            field_values,
            initial_state,
            expected_state,
            expected_memory: vec![],
            assertions,
            path_id: None,
            category: if set_flags {
                ExecutionTestCategory::Flags
            } else {
                ExecutionTestCategory::Normal
            },
        });
    }

    // Generate 32-bit tests
    for (test_idx, (n, imms, immr, operand1, description)) in test_cases_32.iter().enumerate() {
        let mask = match oracle.decode_bitmasks(*n, *imms, *immr, false) {
            Some(m) => m,
            None => continue,
        };

        let operand1_32 = *operand1 & 0xFFFFFFFF;
        let result = match op {
            LogicalOp::And | LogicalOp::Ands => operand1_32 & mask,
            LogicalOp::Or => operand1_32 | mask,
            LogicalOp::Xor => operand1_32 ^ mask,
        } & 0xFFFFFFFF;

        // Encoding: sf(0) opc(2) 100100 N(1) immr(6) imms(6) Rn(5) Rd(5)
        let encoding: u64 = (0u64 << 31)  // sf = 0 (32-bit)
            | (opc << 29)
            | (0b100100u64 << 23)
            | ((*n as u64) << 22)
            | ((*immr as u64) << 16)
            | ((*imms as u64) << 10)
            | (1u64 << 5)
            | 0;

        let mut initial_state = ProcessorState::default();
        initial_state.gp_regs.insert(1, operand1_32);

        let mut expected_state = ProcessorState::default();
        expected_state.gp_regs.insert(0, result);

        let mut assertions = vec![TestAssertion {
            check: AssertionCheck::GpReg32(0),
            expected: AssertionValue::U32(result as u32),
            message: format!("W0 should be 0x{:08X}", result as u32),
        }];

        if set_flags {
            assertions.push(TestAssertion {
                check: AssertionCheck::Flag(ProcessorFlag::N),
                expected: AssertionValue::Bool((result >> 31) != 0),
                message: format!("N flag"),
            });
            assertions.push(TestAssertion {
                check: AssertionCheck::Flag(ProcessorFlag::Z),
                expected: AssertionValue::Bool(result == 0),
                message: format!("Z flag"),
            });
        }

        let provenance = Provenance::new(
            instr.name.as_str(),
            &enc_analysis.name,
            format!("{} W0, W1, #0x{:X}", op_name, mask),
            Requirement::RegisterWrite {
                reg_type: RegType::Gp32,
                dest_field: "Rd".to_string(),
            },
            format!("{} (32)", description),
        );

        let mut field_values = HashMap::new();
        field_values.insert("sf".to_string(), 0);
        field_values.insert("N".to_string(), *n as u64);
        field_values.insert("imms".to_string(), *imms as u64);
        field_values.insert("immr".to_string(), *immr as u64);
        field_values.insert("Rn".to_string(), 1);
        field_values.insert("Rd".to_string(), 0);

        tests.push(ExecutionTest {
            id: format!(
                "{}_{}_oracle_32_{}",
                enc_analysis.name,
                op_name.to_lowercase(),
                test_idx
            ),
            provenance,
            description: format!("Test {} 32-bit: {} (oracle)", op_name, description),
            encoding_name: enc_analysis.name.clone(),
            iset: enc_analysis.iset,
            encoding,
            encoding_width: enc_analysis.opcode_pattern.width,
            field_values,
            initial_state,
            expected_state,
            expected_memory: vec![],
            assertions,
            path_id: None,
            category: if set_flags {
                ExecutionTestCategory::Flags
            } else {
                ExecutionTestCategory::Normal
            },
        });
    }

    tests
}

/// Generate oracle tests for logical shifted register instructions
fn generate_logical_shifted_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    _evaluator: &TestEvaluator,
    op: LogicalOp,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();
    let op_name = match op {
        LogicalOp::And => "AND",
        LogicalOp::Or => "ORR",
        LogicalOp::Xor => "EOR",
        LogicalOp::Ands => "ANDS",
    };
    let set_flags = matches!(op, LogicalOp::Ands);

    // Test cases: (op1, op2, shift_amount, shift_type, description)
    let test_cases: Vec<(u64, u64, u32, u32, &str)> = vec![
        (0xFFFF_FFFF, 0x0000_00FF, 0, 0, "no shift"),
        (0xFFFF_FFFF, 0x0000_0001, 8, 0, "LSL #8"),
        (0xFFFF_FFFF, 0xFF00_0000, 8, 1, "LSR #8"),
        (0x8000_0000, 0x8000_0000, 4, 2, "ASR #4 negative"),
        (0x1234_5678, 0xABCD_EF01, 4, 3, "ROR #4"),
    ];

    for (test_idx, (op1, op2, shift_amt, shift_type, description)) in test_cases.iter().enumerate()
    {
        for sf in [0u64, 1u64] {
            let width = if sf == 1 { 64 } else { 32 };
            let masked_op1 = if width == 32 {
                *op1 & 0xFFFF_FFFF
            } else {
                *op1
            };
            let masked_op2 = if width == 32 {
                *op2 & 0xFFFF_FFFF
            } else {
                *op2
            };

            // Apply shift to op2
            let shifted_op2 = match shift_type {
                0 => masked_op2.wrapping_shl(*shift_amt), // LSL
                1 => masked_op2.wrapping_shr(*shift_amt), // LSR
                2 => {
                    // ASR: need proper sign extension for 32-bit
                    if width == 32 {
                        // Sign extend 32-bit to 64-bit, then shift
                        let signed = (masked_op2 as i32) as i64;
                        (signed >> *shift_amt) as u64
                    } else {
                        ((masked_op2 as i64) >> *shift_amt) as u64
                    }
                }
                3 => {
                    // ROR: rotate within the width
                    if width == 32 {
                        let v = masked_op2 as u32;
                        v.rotate_right(*shift_amt) as u64
                    } else {
                        masked_op2.rotate_right(*shift_amt)
                    }
                }
                _ => masked_op2,
            };
            let shifted_masked = if width == 32 {
                shifted_op2 & 0xFFFF_FFFF
            } else {
                shifted_op2
            };

            // Compute result
            let result = match op {
                LogicalOp::And | LogicalOp::Ands => masked_op1 & shifted_masked,
                LogicalOp::Or => masked_op1 | shifted_masked,
                LogicalOp::Xor => masked_op1 ^ shifted_masked,
            };
            let final_result = if width == 32 {
                result & 0xFFFF_FFFF
            } else {
                result
            };

            let encoding = enc_analysis.opcode_pattern.fixed_value
                | (sf << 31)
                | ((*shift_type as u64) << 22)
                | (2 << 16) // Rm = X2
                | ((*shift_amt as u64) << 10)
                | (1 << 5)  // Rn = X1
                | 0; // Rd = X0

            let mut initial_state = ProcessorState::default();
            initial_state.gp_regs.insert(1, masked_op1);
            initial_state.gp_regs.insert(2, masked_op2);

            let mut expected_state = ProcessorState::default();
            expected_state.gp_regs.insert(0, final_result);

            let mut assertions = Vec::new();
            if width == 32 {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg32(0),
                    expected: AssertionValue::U32(final_result as u32),
                    message: format!("W0 should be 0x{:08X}", final_result as u32),
                });
            } else {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg(0),
                    expected: AssertionValue::U64(final_result),
                    message: format!("X0 should be 0x{:016X}", final_result),
                });
            }

            if set_flags {
                let n = (final_result >> (width - 1)) & 1 != 0;
                let z = final_result == 0;
                assertions.push(TestAssertion {
                    check: AssertionCheck::Flag(ProcessorFlag::N),
                    expected: AssertionValue::Bool(n),
                    message: format!("N flag should be {}", n),
                });
                assertions.push(TestAssertion {
                    check: AssertionCheck::Flag(ProcessorFlag::Z),
                    expected: AssertionValue::Bool(z),
                    message: format!("Z flag should be {}", z),
                });
            }

            let size_suffix = if sf == 1 { "64" } else { "32" };
            let provenance = Provenance::new(
                instr.name.as_str(),
                &enc_analysis.name,
                format!("{} X0, X1, X2, shift #{}", op_name, shift_amt),
                Requirement::RegisterWrite {
                    reg_type: if sf == 1 {
                        RegType::Gp64
                    } else {
                        RegType::Gp32
                    },
                    dest_field: "Rd".to_string(),
                },
                format!("{} ({})", description, size_suffix),
            );

            let mut field_values = HashMap::new();
            field_values.insert("sf".to_string(), sf);
            field_values.insert("shift".to_string(), *shift_type as u64);
            field_values.insert("imm6".to_string(), *shift_amt as u64);
            field_values.insert("Rm".to_string(), 2);
            field_values.insert("Rn".to_string(), 1);
            field_values.insert("Rd".to_string(), 0);

            tests.push(ExecutionTest {
                id: format!(
                    "{}_{}_shifted_oracle_{}_{}",
                    enc_analysis.name,
                    op_name.to_lowercase(),
                    size_suffix,
                    test_idx
                ),
                provenance,
                description: format!(
                    "Test {} shifted {}-bit: {} (oracle)",
                    op_name, width, description
                ),
                encoding_name: enc_analysis.name.clone(),
                iset: enc_analysis.iset,
                encoding,
                encoding_width: enc_analysis.opcode_pattern.width,
                field_values,
                initial_state,
                expected_state,
                expected_memory: vec![],
                assertions,
                path_id: None,
                category: ExecutionTestCategory::Normal,
            });
        }
    }

    tests
}

/// Generate oracle tests for ADD/SUB shifted register instructions
fn generate_add_sub_shifted_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    evaluator: &TestEvaluator,
    is_sub: bool,
    set_flags: bool,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();
    let op_name = match (is_sub, set_flags) {
        (false, false) => "ADD",
        (false, true) => "ADDS",
        (true, false) => "SUB",
        (true, true) => "SUBS",
    };

    // Test cases: (op1, op2, shift_amt, shift_type, description)
    let test_cases: Vec<(u64, u64, u32, u32, &str)> = vec![
        (100, 10, 0, 0, "no shift"),
        (100, 1, 3, 0, "LSL #3 (multiply by 8)"),
        (
            0x8000_0000_0000_0000,
            0x8000_0000_0000_0000,
            0,
            0,
            "overflow test",
        ),
        (0, 0xFFFF_FFFF_FFFF_FFFF, 0, 0, "subtract from zero"),
    ];

    for (test_idx, (op1, op2, shift_amt, shift_type, description)) in test_cases.iter().enumerate()
    {
        for sf in [0u64, 1u64] {
            let width = if sf == 1 { 64 } else { 32 };
            let masked_op1 = if width == 32 {
                *op1 & 0xFFFF_FFFF
            } else {
                *op1
            };
            let masked_op2 = if width == 32 {
                *op2 & 0xFFFF_FFFF
            } else {
                *op2
            };

            // Apply shift to op2
            let shifted_op2 = match shift_type {
                0 => masked_op2.wrapping_shl(*shift_amt), // LSL
                1 => masked_op2.wrapping_shr(*shift_amt), // LSR
                2 => {
                    // ASR: need proper sign extension for 32-bit
                    if width == 32 {
                        let signed = (masked_op2 as i32) as i64;
                        (signed >> *shift_amt) as u64
                    } else {
                        ((masked_op2 as i64) >> *shift_amt) as u64
                    }
                }
                _ => masked_op2,
            };
            let shifted_masked = if width == 32 {
                shifted_op2 & 0xFFFF_FFFF
            } else {
                shifted_op2
            };

            // Compute result and flags
            let result = if is_sub {
                evaluator.eval_sub(masked_op1, shifted_masked, width as u32, set_flags)
            } else {
                evaluator.eval_add(masked_op1, shifted_masked, width as u32, set_flags)
            };

            let encoding = enc_analysis.opcode_pattern.fixed_value
                | (sf << 31)
                | ((is_sub as u64) << 30)
                | ((set_flags as u64) << 29)
                | ((*shift_type as u64) << 22)
                | (2 << 16) // Rm = X2
                | ((*shift_amt as u64) << 10)
                | (1 << 5)  // Rn = X1
                | 0; // Rd = X0

            let mut initial_state = ProcessorState::default();
            initial_state.gp_regs.insert(1, masked_op1);
            initial_state.gp_regs.insert(2, masked_op2);

            let mut expected_state = ProcessorState::default();
            let final_result = if width == 32 {
                result.dest_value & 0xFFFF_FFFF
            } else {
                result.dest_value
            };
            expected_state.gp_regs.insert(0, final_result);

            let mut assertions = Vec::new();
            if width == 32 {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg32(0),
                    expected: AssertionValue::U32(final_result as u32),
                    message: format!("W0 should be 0x{:08X}", final_result as u32),
                });
            } else {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg(0),
                    expected: AssertionValue::U64(final_result),
                    message: format!("X0 should be 0x{:016X}", final_result),
                });
            }

            if let Some(nzcv) = result.nzcv {
                assertions.push(TestAssertion {
                    check: AssertionCheck::Flag(ProcessorFlag::N),
                    expected: AssertionValue::Bool(nzcv.n),
                    message: format!("N flag should be {}", nzcv.n),
                });
                assertions.push(TestAssertion {
                    check: AssertionCheck::Flag(ProcessorFlag::Z),
                    expected: AssertionValue::Bool(nzcv.z),
                    message: format!("Z flag should be {}", nzcv.z),
                });
                assertions.push(TestAssertion {
                    check: AssertionCheck::Flag(ProcessorFlag::C),
                    expected: AssertionValue::Bool(nzcv.c),
                    message: format!("C flag should be {}", nzcv.c),
                });
                assertions.push(TestAssertion {
                    check: AssertionCheck::Flag(ProcessorFlag::V),
                    expected: AssertionValue::Bool(nzcv.v),
                    message: format!("V flag should be {}", nzcv.v),
                });
            }

            let size_suffix = if sf == 1 { "64" } else { "32" };
            let provenance = Provenance::new(
                instr.name.as_str(),
                &enc_analysis.name,
                format!("{} X0, X1, X2, shift #{}", op_name, shift_amt),
                Requirement::RegisterWrite {
                    reg_type: if sf == 1 {
                        RegType::Gp64
                    } else {
                        RegType::Gp32
                    },
                    dest_field: "Rd".to_string(),
                },
                format!("{} ({})", description, size_suffix),
            );

            let mut field_values = HashMap::new();
            field_values.insert("sf".to_string(), sf);
            field_values.insert("op".to_string(), is_sub as u64);
            field_values.insert("S".to_string(), set_flags as u64);
            field_values.insert("shift".to_string(), *shift_type as u64);
            field_values.insert("imm6".to_string(), *shift_amt as u64);
            field_values.insert("Rm".to_string(), 2);
            field_values.insert("Rn".to_string(), 1);
            field_values.insert("Rd".to_string(), 0);

            tests.push(ExecutionTest {
                id: format!(
                    "{}_{}_shifted_oracle_{}_{}",
                    enc_analysis.name,
                    op_name.to_lowercase(),
                    size_suffix,
                    test_idx
                ),
                provenance,
                description: format!(
                    "Test {} shifted {}-bit: {} (oracle)",
                    op_name, width, description
                ),
                encoding_name: enc_analysis.name.clone(),
                iset: enc_analysis.iset,
                encoding,
                encoding_width: enc_analysis.opcode_pattern.width,
                field_values,
                initial_state,
                expected_state,
                expected_memory: vec![],
                assertions,
                path_id: None,
                category: if set_flags {
                    ExecutionTestCategory::Flags
                } else {
                    ExecutionTestCategory::Normal
                },
            });
        }
    }

    tests
}

/// Generate oracle tests for move immediate instructions (MOVZ, MOVN, MOVK)
fn generate_move_imm_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    _evaluator: &TestEvaluator,
    op: MoveOp,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();
    let op_name = match op {
        MoveOp::Movz => "MOVZ",
        MoveOp::Movn => "MOVN",
        MoveOp::Movk => "MOVK",
    };

    // Test cases: (imm16, hw (shift = hw*16), initial_rd (for MOVK), description)
    let test_cases: Vec<(u64, u64, u64, &str)> = vec![
        (0x1234, 0, 0, "lower 16 bits"),
        (0xABCD, 1, 0, "shifted 16 bits"),
        (0xFFFF, 0, 0, "max imm16"),
        (0x0000, 0, 0, "zero imm16"),
        (0x5678, 2, 0, "shifted 32 bits"),
        (0xDEAD, 3, 0, "shifted 48 bits"),
    ];

    for (test_idx, (imm16, hw, initial_rd, description)) in test_cases.iter().enumerate() {
        for sf in [0u64, 1u64] {
            // Skip 64-bit shifts for 32-bit ops
            if sf == 0 && *hw >= 2 {
                continue;
            }

            let width = if sf == 1 { 64 } else { 32 };
            let shift = hw * 16;

            // Compute expected result
            let result = match op {
                MoveOp::Movz => *imm16 << shift,
                MoveOp::Movn => {
                    let val = *imm16 << shift;
                    if width == 32 {
                        (!val) & 0xFFFF_FFFF
                    } else {
                        !val
                    }
                }
                MoveOp::Movk => {
                    let mask = 0xFFFFu64 << shift;
                    (*initial_rd & !mask) | (*imm16 << shift)
                }
            };
            let final_result = if width == 32 {
                result & 0xFFFF_FFFF
            } else {
                result
            };

            let encoding = enc_analysis.opcode_pattern.fixed_value
                | (sf << 31)
                | (*hw << 21)
                | (*imm16 << 5)
                | 0; // Rd = X0

            let mut initial_state = ProcessorState::default();
            if matches!(op, MoveOp::Movk) {
                initial_state.gp_regs.insert(0, *initial_rd);
            }

            let mut expected_state = ProcessorState::default();
            expected_state.gp_regs.insert(0, final_result);

            let mut assertions = Vec::new();
            if width == 32 {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg32(0),
                    expected: AssertionValue::U32(final_result as u32),
                    message: format!("W0 should be 0x{:08X}", final_result as u32),
                });
            } else {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg(0),
                    expected: AssertionValue::U64(final_result),
                    message: format!("X0 should be 0x{:016X}", final_result),
                });
            }

            let size_suffix = if sf == 1 { "64" } else { "32" };
            let provenance = Provenance::new(
                instr.name.as_str(),
                &enc_analysis.name,
                format!("{} X0, #0x{:04X}, LSL #{}", op_name, imm16, shift),
                Requirement::RegisterWrite {
                    reg_type: if sf == 1 {
                        RegType::Gp64
                    } else {
                        RegType::Gp32
                    },
                    dest_field: "Rd".to_string(),
                },
                format!("{} ({})", description, size_suffix),
            );

            let mut field_values = HashMap::new();
            field_values.insert("sf".to_string(), sf);
            field_values.insert("hw".to_string(), *hw);
            field_values.insert("imm16".to_string(), *imm16);
            field_values.insert("Rd".to_string(), 0);

            tests.push(ExecutionTest {
                id: format!(
                    "{}_{}_oracle_{}_{}",
                    enc_analysis.name,
                    op_name.to_lowercase(),
                    size_suffix,
                    test_idx
                ),
                provenance,
                description: format!("Test {} {}-bit: {} (oracle)", op_name, width, description),
                encoding_name: enc_analysis.name.clone(),
                iset: enc_analysis.iset,
                encoding,
                encoding_width: enc_analysis.opcode_pattern.width,
                field_values,
                initial_state,
                expected_state,
                expected_memory: vec![],
                assertions,
                path_id: None,
                category: ExecutionTestCategory::Normal,
            });
        }
    }

    tests
}

/// Generate oracle tests for variable shift instructions (LSLV, LSRV, ASRV, RORV)
fn generate_shift_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    evaluator: &TestEvaluator,
    shift_type: ShiftType,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();
    let op_name = match shift_type {
        ShiftType::Lsl => "LSLV",
        ShiftType::Lsr => "LSRV",
        ShiftType::Asr => "ASRV",
        ShiftType::Ror => "RORV",
    };

    // Test cases: (value, shift_amount, description)
    let test_cases: Vec<(u64, u64, &str)> = vec![
        (0x1234_5678, 0, "shift by 0"),
        (0x1234_5678, 4, "shift by 4"),
        (0x1234_5678, 8, "shift by 8"),
        (0x8000_0000_0000_0000, 1, "MSB set, shift 1"),
        (0x0000_0001, 63, "LSB set, max shift"),
        (0xFFFF_FFFF_FFFF_FFFF, 32, "all ones, shift 32"),
    ];

    for (test_idx, (value, shift_amt, description)) in test_cases.iter().enumerate() {
        for sf in [0u64, 1u64] {
            let width = if sf == 1 { 64 } else { 32 };
            let mask = if width == 32 { 0xFFFF_FFFF } else { u64::MAX };
            let masked_value = *value & mask;
            let effective_shift = (*shift_amt as u32) % width;

            // Compute result
            let result = match shift_type {
                ShiftType::Lsl => {
                    evaluator
                        .eval_lsl(masked_value, effective_shift, width as u32)
                        .dest_value
                }
                ShiftType::Lsr => {
                    evaluator
                        .eval_lsr(masked_value, effective_shift, width as u32)
                        .dest_value
                }
                ShiftType::Asr => {
                    evaluator
                        .eval_asr(masked_value, effective_shift, width as u32)
                        .dest_value
                }
                ShiftType::Ror => {
                    evaluator
                        .eval_ror(masked_value, effective_shift, width as u32)
                        .dest_value
                }
            };

            let encoding = enc_analysis.opcode_pattern.fixed_value
                | (sf << 31)
                | (2 << 16) // Rm = X2
                | (1 << 5)  // Rn = X1
                | 0; // Rd = X0

            let mut initial_state = ProcessorState::default();
            initial_state.gp_regs.insert(1, masked_value);
            initial_state.gp_regs.insert(2, *shift_amt);

            let mut expected_state = ProcessorState::default();
            expected_state.gp_regs.insert(0, result);

            let mut assertions = Vec::new();
            if width == 32 {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg32(0),
                    expected: AssertionValue::U32(result as u32),
                    message: format!("W0 should be 0x{:08X}", result as u32),
                });
            } else {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg(0),
                    expected: AssertionValue::U64(result),
                    message: format!("X0 should be 0x{:016X}", result),
                });
            }

            let size_suffix = if sf == 1 { "64" } else { "32" };
            let provenance = Provenance::new(
                instr.name.as_str(),
                &enc_analysis.name,
                format!("{} X0, X1, X2", op_name),
                Requirement::RegisterWrite {
                    reg_type: if sf == 1 {
                        RegType::Gp64
                    } else {
                        RegType::Gp32
                    },
                    dest_field: "Rd".to_string(),
                },
                format!("{} ({})", description, size_suffix),
            );

            let mut field_values = HashMap::new();
            field_values.insert("sf".to_string(), sf);
            field_values.insert("Rm".to_string(), 2);
            field_values.insert("Rn".to_string(), 1);
            field_values.insert("Rd".to_string(), 0);

            tests.push(ExecutionTest {
                id: format!(
                    "{}_{}_oracle_{}_{}",
                    enc_analysis.name,
                    op_name.to_lowercase(),
                    size_suffix,
                    test_idx
                ),
                provenance,
                description: format!("Test {} {}-bit: {} (oracle)", op_name, width, description),
                encoding_name: enc_analysis.name.clone(),
                iset: enc_analysis.iset,
                encoding,
                encoding_width: enc_analysis.opcode_pattern.width,
                field_values,
                initial_state,
                expected_state,
                expected_memory: vec![],
                assertions,
                path_id: None,
                category: ExecutionTestCategory::Normal,
            });
        }
    }

    tests
}

/// Generate oracle tests for multiply instructions (MUL, MADD, MSUB)
/// NOTE: For QEMU validation, we only generate MUL tests (Ra=XZR) since the
/// test harness only sets X1/X2. MADD/MSUB with non-zero Ra would need X3 set.
fn generate_multiply_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    evaluator: &TestEvaluator,
    _accumulate: bool,
    _negate: bool,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();

    // Only generate MUL tests (MADD with Ra=XZR) since test harness only sets X1/X2
    let op_name = "MUL";

    // Test cases: (op1, op2, description)
    let test_cases: Vec<(u64, u64, &str)> = vec![
        (2, 3, "simple multiply"),
        (0, 100, "multiply by zero"),
        (1, 1, "multiply by one"),
        (0xFFFF, 0xFFFF, "16-bit max * 16-bit max"),
        (0x1234_5678, 2, "shift-like multiply"),
        (100, 200, "larger values"),
        (0xFFFF_FFFF, 0xFFFF_FFFF, "32-bit overflow"),
        (7, 11, "prime numbers"),
    ];

    for (test_idx, (op1, op2, description)) in test_cases.iter().enumerate() {
        for sf in [0u64, 1u64] {
            let width = if sf == 1 { 64 } else { 32 };
            let mask = if width == 32 { 0xFFFF_FFFF } else { u64::MAX };

            let masked_op1 = *op1 & mask;
            let masked_op2 = *op2 & mask;

            // MUL: Rd = Rn * Rm (Ra = XZR)
            let result = evaluator.eval_mul(masked_op1, masked_op2, width as u32);

            // A64 encoding for MUL (MADD with Ra=XZR): sf 00 11011 000 Rm(5) 0 11111 Rn(5) Rd(5)
            let encoding = (sf << 31)
                | (0b0011011u64 << 24)
                | (0b000u64 << 21)
                | (2 << 16)  // Rm = X2
                | (0 << 15)  // o0 = 0 (MADD)
                | (31 << 10) // Ra = XZR
                | (1 << 5)   // Rn = X1
                | 0; // Rd = X0

            let mut initial_state = ProcessorState::default();
            initial_state.gp_regs.insert(1, masked_op1);
            initial_state.gp_regs.insert(2, masked_op2);

            let mut expected_state = ProcessorState::default();
            expected_state.gp_regs.insert(0, result.dest_value);

            let mut assertions = Vec::new();
            if width == 32 {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg32(0),
                    expected: AssertionValue::U32(result.dest_value as u32),
                    message: format!("W0 should be 0x{:08X}", result.dest_value as u32),
                });
            } else {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg(0),
                    expected: AssertionValue::U64(result.dest_value),
                    message: format!("X0 should be 0x{:016X}", result.dest_value),
                });
            }

            let size_suffix = if sf == 1 { "64" } else { "32" };
            let provenance = Provenance::new(
                instr.name.as_str(),
                &enc_analysis.name,
                format!("{} X0, X1, X2", op_name),
                Requirement::RegisterWrite {
                    reg_type: if sf == 1 {
                        RegType::Gp64
                    } else {
                        RegType::Gp32
                    },
                    dest_field: "Rd".to_string(),
                },
                format!("{} ({})", description, size_suffix),
            );

            let mut field_values = HashMap::new();
            field_values.insert("sf".to_string(), sf);
            field_values.insert("Rm".to_string(), 2);
            field_values.insert("Ra".to_string(), 31);
            field_values.insert("Rn".to_string(), 1);
            field_values.insert("Rd".to_string(), 0);

            tests.push(ExecutionTest {
                id: format!(
                    "{}_{}_oracle_{}_{}",
                    enc_analysis.name,
                    op_name.to_lowercase(),
                    size_suffix,
                    test_idx
                ),
                provenance,
                description: format!("Test {} {}-bit: {} (oracle)", op_name, width, description),
                encoding_name: enc_analysis.name.clone(),
                iset: enc_analysis.iset,
                encoding,
                encoding_width: enc_analysis.opcode_pattern.width,
                field_values,
                initial_state,
                expected_state,
                expected_memory: vec![],
                assertions,
                path_id: None,
                category: ExecutionTestCategory::Normal,
            });
        }
    }

    tests
}

/// Generate oracle tests for multiply long instructions (SMULL, UMULL)
/// NOTE: Only generates SMULL/UMULL (Ra=XZR) since test harness only sets X1/X2
fn generate_multiply_long_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    evaluator: &TestEvaluator,
    signed: bool,
    _accumulate: bool,
    _negate: bool,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();
    let op_name = if signed { "SMULL" } else { "UMULL" };

    // Test cases: (op1_32, op2_32, description)
    let test_cases: Vec<(u64, u64, &str)> = vec![
        (2, 3, "simple multiply"),
        (0xFFFF_FFFF, 2, "max 32-bit * 2"),
        (0x7FFF_FFFF, 0x7FFF_FFFF, "large positive * large positive"),
        (0xFFFF_FFFF, 0xFFFF_FFFF, "max unsigned * max unsigned"),
        (100, 200, "medium values"),
        (0x1234, 0x5678, "16-bit values"),
    ];

    for (test_idx, (op1, op2, description)) in test_cases.iter().enumerate() {
        let masked_op1 = *op1 & 0xFFFF_FFFF;
        let masked_op2 = *op2 & 0xFFFF_FFFF;

        // Compute expected result
        let result = if signed {
            evaluator.eval_smull(masked_op1, masked_op2).dest_value
        } else {
            evaluator.eval_umull(masked_op1, masked_op2).dest_value
        };

        // A64 encoding: 1 00 11011 U01 Rm(5) 0 11111 Rn(5) Rd(5)
        // U=0 for SMULL, U=1 for UMULL; Ra=XZR (31)
        let u = if signed { 0u64 } else { 1u64 };
        let encoding = (1u64 << 31) // sf=1 (always 64-bit result)
            | (0b0011011u64 << 24)
            | (u << 23)
            | (0b01u64 << 21)
            | (2 << 16)  // Rm = W2
            | (0 << 15)  // o0 = 0 (MADDL variant)
            | (31 << 10) // Ra = XZR
            | (1 << 5)   // Rn = W1
            | 0; // Rd = X0

        let mut initial_state = ProcessorState::default();
        initial_state.gp_regs.insert(1, masked_op1);
        initial_state.gp_regs.insert(2, masked_op2);

        let mut expected_state = ProcessorState::default();
        expected_state.gp_regs.insert(0, result);

        let assertions = vec![TestAssertion {
            check: AssertionCheck::GpReg(0),
            expected: AssertionValue::U64(result),
            message: format!("X0 should be 0x{:016X}", result),
        }];

        let provenance = Provenance::new(
            instr.name.as_str(),
            &enc_analysis.name,
            format!("{} X0, W1, W2", op_name),
            Requirement::RegisterWrite {
                reg_type: RegType::Gp64,
                dest_field: "Rd".to_string(),
            },
            description.to_string(),
        );

        let mut field_values = HashMap::new();
        field_values.insert("sf".to_string(), 1);
        field_values.insert("Rm".to_string(), 2);
        field_values.insert("Ra".to_string(), 31);
        field_values.insert("Rn".to_string(), 1);
        field_values.insert("Rd".to_string(), 0);

        tests.push(ExecutionTest {
            id: format!(
                "{}_{}_oracle_{}",
                enc_analysis.name,
                op_name.to_lowercase(),
                test_idx
            ),
            provenance,
            description: format!("Test {}: {} (oracle)", op_name, description),
            encoding_name: enc_analysis.name.clone(),
            iset: enc_analysis.iset,
            encoding,
            encoding_width: enc_analysis.opcode_pattern.width,
            field_values,
            initial_state,
            expected_state,
            expected_memory: vec![],
            assertions,
            path_id: None,
            category: ExecutionTestCategory::Normal,
        });
    }

    tests
}

/// Generate oracle tests for multiply high instructions (SMULH, UMULH)
fn generate_multiply_high_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    evaluator: &TestEvaluator,
    signed: bool,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();
    let op_name = if signed { "SMULH" } else { "UMULH" };

    // Test cases: (op1, op2, description)
    let test_cases: Vec<(u64, u64, &str)> = vec![
        (2, 3, "small values - high bits zero"),
        (
            0x8000_0000_0000_0000,
            2,
            "large value * 2 - produces high bits",
        ),
        (
            0xFFFF_FFFF_FFFF_FFFF,
            0xFFFF_FFFF_FFFF_FFFF,
            "max * max unsigned",
        ),
        (
            0x7FFF_FFFF_FFFF_FFFF,
            0x7FFF_FFFF_FFFF_FFFF,
            "max positive * max positive",
        ),
        (0x1_0000_0000, 0x1_0000_0000, "2^32 * 2^32"),
    ];

    for (test_idx, (op1, op2, description)) in test_cases.iter().enumerate() {
        let result = if signed {
            evaluator.eval_smulh(*op1, *op2).dest_value
        } else {
            evaluator.eval_umulh(*op1, *op2).dest_value
        };

        // A64 encoding: 1 00 11011 U10 Rm(5) 0 11111 Rn(5) Rd(5)
        let u = if signed { 0u64 } else { 1u64 };
        let encoding = (1u64 << 31)
            | (0b0011011u64 << 24)
            | (u << 23)
            | (0b10u64 << 21)
            | (2 << 16) // Rm = X2
            | (0 << 15)
            | (31 << 10) // Ra = XZR (ignored)
            | (1 << 5)   // Rn = X1
            | 0; // Rd = X0

        let mut initial_state = ProcessorState::default();
        initial_state.gp_regs.insert(1, *op1);
        initial_state.gp_regs.insert(2, *op2);

        let mut expected_state = ProcessorState::default();
        expected_state.gp_regs.insert(0, result);

        let assertions = vec![TestAssertion {
            check: AssertionCheck::GpReg(0),
            expected: AssertionValue::U64(result),
            message: format!("X0 should be 0x{:016X}", result),
        }];

        let provenance = Provenance::new(
            instr.name.as_str(),
            &enc_analysis.name,
            format!("{} X0, X1, X2", op_name),
            Requirement::RegisterWrite {
                reg_type: RegType::Gp64,
                dest_field: "Rd".to_string(),
            },
            description.to_string(),
        );

        let mut field_values = HashMap::new();
        field_values.insert("sf".to_string(), 1);
        field_values.insert("Rm".to_string(), 2);
        field_values.insert("Rn".to_string(), 1);
        field_values.insert("Rd".to_string(), 0);

        tests.push(ExecutionTest {
            id: format!(
                "{}_{}_oracle_{}",
                enc_analysis.name,
                op_name.to_lowercase(),
                test_idx
            ),
            provenance,
            description: format!("Test {}: {} (oracle)", op_name, description),
            encoding_name: enc_analysis.name.clone(),
            iset: enc_analysis.iset,
            encoding,
            encoding_width: enc_analysis.opcode_pattern.width,
            field_values,
            initial_state,
            expected_state,
            expected_memory: vec![],
            assertions,
            path_id: None,
            category: ExecutionTestCategory::Normal,
        });
    }

    tests
}

/// Generate oracle tests for divide instructions (SDIV, UDIV)
fn generate_divide_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    evaluator: &TestEvaluator,
    signed: bool,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();
    let op_name = if signed { "SDIV" } else { "UDIV" };

    // Test cases: (dividend, divisor, description)
    let test_cases: Vec<(u64, u64, &str)> = vec![
        (100, 10, "simple division"),
        (100, 3, "division with remainder"),
        (0, 10, "zero dividend"),
        (10, 0, "divide by zero - result is 0"),
        (0xFFFF_FFFF_FFFF_FFFF, 2, "max value / 2"),
        (0x8000_0000_0000_0000, 2, "MSB set / 2"),
        (7, 7, "self-division"),
        (1, 1, "one / one"),
    ];

    for (test_idx, (dividend, divisor, description)) in test_cases.iter().enumerate() {
        for sf in [0u64, 1u64] {
            let width = if sf == 1 { 64 } else { 32 };
            let mask = if width == 32 { 0xFFFF_FFFF } else { u64::MAX };

            let masked_dividend = *dividend & mask;
            let masked_divisor = *divisor & mask;

            let result = if signed {
                evaluator.eval_sdiv(masked_dividend, masked_divisor, width as u32)
            } else {
                evaluator.eval_udiv(masked_dividend, masked_divisor, width as u32)
            };

            // A64 encoding: sf 0 0 11010110 Rm(5) 00001 x Rn(5) Rd(5)
            // x=0 for UDIV, x=1 for SDIV
            let x = if signed { 1u64 } else { 0u64 };
            let encoding = (sf << 31)
                | (0b0011010110u64 << 21)
                | (2 << 16) // Rm = X2
                | (0b00001u64 << 11)
                | (x << 10)
                | (1 << 5) // Rn = X1
                | 0; // Rd = X0

            let mut initial_state = ProcessorState::default();
            initial_state.gp_regs.insert(1, masked_dividend);
            initial_state.gp_regs.insert(2, masked_divisor);

            let mut expected_state = ProcessorState::default();
            expected_state.gp_regs.insert(0, result.dest_value);

            let mut assertions = Vec::new();
            if width == 32 {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg32(0),
                    expected: AssertionValue::U32(result.dest_value as u32),
                    message: format!("W0 should be 0x{:08X}", result.dest_value as u32),
                });
            } else {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg(0),
                    expected: AssertionValue::U64(result.dest_value),
                    message: format!("X0 should be 0x{:016X}", result.dest_value),
                });
            }

            let size_suffix = if sf == 1 { "64" } else { "32" };
            let provenance = Provenance::new(
                instr.name.as_str(),
                &enc_analysis.name,
                format!("{} X0, X1, X2", op_name),
                Requirement::RegisterWrite {
                    reg_type: if sf == 1 {
                        RegType::Gp64
                    } else {
                        RegType::Gp32
                    },
                    dest_field: "Rd".to_string(),
                },
                format!("{} ({})", description, size_suffix),
            );

            let mut field_values = HashMap::new();
            field_values.insert("sf".to_string(), sf);
            field_values.insert("Rm".to_string(), 2);
            field_values.insert("Rn".to_string(), 1);
            field_values.insert("Rd".to_string(), 0);

            tests.push(ExecutionTest {
                id: format!(
                    "{}_{}_oracle_{}_{}",
                    enc_analysis.name,
                    op_name.to_lowercase(),
                    size_suffix,
                    test_idx
                ),
                provenance,
                description: format!("Test {} {}-bit: {} (oracle)", op_name, width, description),
                encoding_name: enc_analysis.name.clone(),
                iset: enc_analysis.iset,
                encoding,
                encoding_width: enc_analysis.opcode_pattern.width,
                field_values,
                initial_state,
                expected_state,
                expected_memory: vec![],
                assertions,
                path_id: None,
                category: ExecutionTestCategory::Normal,
            });
        }
    }

    tests
}

/// Generate oracle tests for bitfield instructions (SBFM, UBFM, BFM)
fn generate_bitfield_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    evaluator: &TestEvaluator,
    op: BitfieldOp,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();
    let op_name = match op {
        BitfieldOp::Sbfm => "SBFM",
        BitfieldOp::Ubfm => "UBFM",
        BitfieldOp::Bfm => "BFM",
    };

    // Test cases: (value, immr, imms, dest_value (for BFM), description)
    // These cover common alias cases:
    // - LSL: UBFM with imms = width - 1 - shift, immr = width - shift
    // - LSR: UBFM with imms = width - 1, immr = shift
    // - ASR: SBFM with imms = width - 1, immr = shift
    // - SXTB/SXTH/SXTW: SBFM with immr = 0
    // - UXTB/UXTH: UBFM with immr = 0
    let test_cases: Vec<(u64, u32, u32, u64, &str)> = vec![
        (0xFF, 0, 7, 0, "extract byte (UXTB/SXTB)"),
        (0x80, 0, 7, 0, "extract signed byte"),
        (0xFFFF, 0, 15, 0, "extract halfword (UXTH/SXTH)"),
        (0x8000, 0, 15, 0, "extract signed halfword"),
        (0xFFFF_FFFF, 0, 31, 0, "extract word (32-bit extract)"),
        (0x1234_5678, 8, 15, 0, "extract bits [15:8]"),
        (0xDEAD_BEEF, 4, 7, 0, "extract nibble"),
        (0xCAFE, 60, 3, 0, "insert at position 60 (UBFIZ)"),
        (
            0xAAAA_BBBB_CCCC_DDDD,
            16,
            31,
            0x1111_2222_3333_4444,
            "BFM insert",
        ),
    ];

    for (test_idx, (value, immr, imms, dest_init, description)) in test_cases.iter().enumerate() {
        for sf in [0u64, 1u64] {
            let width = if sf == 1 { 64 } else { 32 };

            // Skip 64-bit specific tests for 32-bit mode
            if sf == 0 && (*immr >= 32 || *imms >= 32) {
                continue;
            }

            let mask = if width == 32 { 0xFFFF_FFFF } else { u64::MAX };
            let masked_value = *value & mask;
            let masked_dest = *dest_init & mask;

            let result = match op {
                BitfieldOp::Sbfm => evaluator.eval_sbfm(masked_value, *immr, *imms, width as u32),
                BitfieldOp::Ubfm => evaluator.eval_ubfm(masked_value, *immr, *imms, width as u32),
                BitfieldOp::Bfm => {
                    evaluator.eval_bfm(masked_dest, masked_value, *immr, *imms, width as u32)
                }
            };

            // A64 encoding: sf opc(2) 100110 N immr(6) imms(6) Rn(5) Rd(5)
            // opc: 00=SBFM, 01=BFM, 10=UBFM
            let opc: u64 = match op {
                BitfieldOp::Sbfm => 0b00,
                BitfieldOp::Bfm => 0b01,
                BitfieldOp::Ubfm => 0b10,
            };
            let n = sf; // N must equal sf
            let encoding = (sf << 31)
                | (opc << 29)
                | (0b100110u64 << 23)
                | (n << 22)
                | ((*immr as u64) << 16)
                | ((*imms as u64) << 10)
                | (1 << 5) // Rn = X1
                | 0; // Rd = X0

            let mut initial_state = ProcessorState::default();
            initial_state.gp_regs.insert(1, masked_value);
            if matches!(op, BitfieldOp::Bfm) {
                initial_state.gp_regs.insert(0, masked_dest);
            }

            let mut expected_state = ProcessorState::default();
            expected_state.gp_regs.insert(0, result.dest_value);

            let mut assertions = Vec::new();
            if width == 32 {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg32(0),
                    expected: AssertionValue::U32(result.dest_value as u32),
                    message: format!("W0 should be 0x{:08X}", result.dest_value as u32),
                });
            } else {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg(0),
                    expected: AssertionValue::U64(result.dest_value),
                    message: format!("X0 should be 0x{:016X}", result.dest_value),
                });
            }

            let size_suffix = if sf == 1 { "64" } else { "32" };
            let provenance = Provenance::new(
                instr.name.as_str(),
                &enc_analysis.name,
                format!("{} X0, X1, #{}, #{}", op_name, immr, imms),
                Requirement::RegisterWrite {
                    reg_type: if sf == 1 {
                        RegType::Gp64
                    } else {
                        RegType::Gp32
                    },
                    dest_field: "Rd".to_string(),
                },
                format!("{} ({})", description, size_suffix),
            );

            let mut field_values = HashMap::new();
            field_values.insert("sf".to_string(), sf);
            field_values.insert("N".to_string(), n);
            field_values.insert("immr".to_string(), *immr as u64);
            field_values.insert("imms".to_string(), *imms as u64);
            field_values.insert("Rn".to_string(), 1);
            field_values.insert("Rd".to_string(), 0);

            tests.push(ExecutionTest {
                id: format!(
                    "{}_{}_oracle_{}_{}",
                    enc_analysis.name,
                    op_name.to_lowercase(),
                    size_suffix,
                    test_idx
                ),
                provenance,
                description: format!("Test {} {}-bit: {} (oracle)", op_name, width, description),
                encoding_name: enc_analysis.name.clone(),
                iset: enc_analysis.iset,
                encoding,
                encoding_width: enc_analysis.opcode_pattern.width,
                field_values,
                initial_state,
                expected_state,
                expected_memory: vec![],
                assertions,
                path_id: None,
                category: ExecutionTestCategory::Normal,
            });
        }
    }

    tests
}

/// Generate oracle tests for EXTR instruction
fn generate_extract_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    evaluator: &TestEvaluator,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();

    // Test cases: (rn, rm, lsb, description)
    let test_cases: Vec<(u64, u64, u32, &str)> = vec![
        (0xDEAD_BEEF, 0xCAFE_BABE, 0, "extract at 0"),
        (0xDEAD_BEEF, 0xCAFE_BABE, 16, "extract at 16"),
        (0xDEAD_BEEF, 0xCAFE_BABE, 8, "extract at 8"),
        (
            0x1234_5678_9ABC_DEF0,
            0xFEDC_BA98_7654_3210,
            32,
            "extract at 32 (64-bit)",
        ),
        (0xAAAA_AAAA, 0x5555_5555, 4, "alternating bits"),
    ];

    for (test_idx, (rn, rm, lsb, description)) in test_cases.iter().enumerate() {
        for sf in [0u64, 1u64] {
            let width = if sf == 1 { 64 } else { 32 };

            // Skip large LSB for 32-bit
            if sf == 0 && *lsb >= 32 {
                continue;
            }

            let mask = if width == 32 { 0xFFFF_FFFF } else { u64::MAX };
            let masked_rn = *rn & mask;
            let masked_rm = *rm & mask;

            let result = evaluator.eval_extr(masked_rn, masked_rm, *lsb, width as u32);

            // A64 encoding: sf 00 100111 N0 Rm(5) imms(6) Rn(5) Rd(5)
            let n = sf;
            let encoding = (sf << 31)
                | (0b00100111u64 << 23)
                | (n << 22)
                | (0 << 21)
                | (2 << 16) // Rm = X2
                | ((*lsb as u64) << 10)
                | (1 << 5) // Rn = X1
                | 0; // Rd = X0

            let mut initial_state = ProcessorState::default();
            initial_state.gp_regs.insert(1, masked_rn);
            initial_state.gp_regs.insert(2, masked_rm);

            let mut expected_state = ProcessorState::default();
            expected_state.gp_regs.insert(0, result.dest_value);

            let mut assertions = Vec::new();
            if width == 32 {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg32(0),
                    expected: AssertionValue::U32(result.dest_value as u32),
                    message: format!("W0 should be 0x{:08X}", result.dest_value as u32),
                });
            } else {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg(0),
                    expected: AssertionValue::U64(result.dest_value),
                    message: format!("X0 should be 0x{:016X}", result.dest_value),
                });
            }

            let size_suffix = if sf == 1 { "64" } else { "32" };
            let provenance = Provenance::new(
                instr.name.as_str(),
                &enc_analysis.name,
                format!("EXTR X0, X1, X2, #{}", lsb),
                Requirement::RegisterWrite {
                    reg_type: if sf == 1 {
                        RegType::Gp64
                    } else {
                        RegType::Gp32
                    },
                    dest_field: "Rd".to_string(),
                },
                format!("{} ({})", description, size_suffix),
            );

            let mut field_values = HashMap::new();
            field_values.insert("sf".to_string(), sf);
            field_values.insert("N".to_string(), n);
            field_values.insert("Rm".to_string(), 2);
            field_values.insert("imms".to_string(), *lsb as u64);
            field_values.insert("Rn".to_string(), 1);
            field_values.insert("Rd".to_string(), 0);

            tests.push(ExecutionTest {
                id: format!(
                    "{}_extr_oracle_{}_{}",
                    enc_analysis.name, size_suffix, test_idx
                ),
                provenance,
                description: format!("Test EXTR {}-bit: {} (oracle)", width, description),
                encoding_name: enc_analysis.name.clone(),
                iset: enc_analysis.iset,
                encoding,
                encoding_width: enc_analysis.opcode_pattern.width,
                field_values,
                initial_state,
                expected_state,
                expected_memory: vec![],
                assertions,
                path_id: None,
                category: ExecutionTestCategory::Normal,
            });
        }
    }

    tests
}

/// Generate oracle tests for bit operations (CLZ, CLS, RBIT, REV, REV16, REV32)
fn generate_bitop_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    evaluator: &TestEvaluator,
    op: BitOpType,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();
    let op_name = match op {
        BitOpType::Clz => "CLZ",
        BitOpType::Cls => "CLS",
        BitOpType::Rbit => "RBIT",
        BitOpType::Rev => "REV",
        BitOpType::Rev16 => "REV16",
        BitOpType::Rev32 => "REV32",
    };

    // Test cases: (value, description)
    let test_cases: Vec<(u64, &str)> = vec![
        (0, "zero value"),
        (1, "single bit set (LSB)"),
        (0x8000_0000_0000_0000, "single bit set (MSB 64)"),
        (0x8000_0000, "single bit set (MSB 32)"),
        (0xFFFF_FFFF_FFFF_FFFF, "all ones"),
        (0x1234_5678_9ABC_DEF0, "mixed pattern"),
        (0x0F0F_0F0F_0F0F_0F0F, "alternating nibbles"),
        (0xDEAD_BEEF_CAFE_BABE, "magic values"),
    ];

    for (test_idx, (value, description)) in test_cases.iter().enumerate() {
        // REV32 is only for 64-bit
        let sf_values = if matches!(op, BitOpType::Rev32) {
            vec![1u64]
        } else {
            vec![0u64, 1u64]
        };

        for sf in sf_values {
            let width = if sf == 1 { 64 } else { 32 };
            let mask = if width == 32 { 0xFFFF_FFFF } else { u64::MAX };
            let masked_value = *value & mask;

            let result = match op {
                BitOpType::Clz => evaluator.eval_clz(masked_value, width as u32),
                BitOpType::Cls => evaluator.eval_cls(masked_value, width as u32),
                BitOpType::Rbit => evaluator.eval_rbit(masked_value, width as u32),
                BitOpType::Rev => evaluator.eval_rev(masked_value, width as u32),
                BitOpType::Rev16 => evaluator.eval_rev16(masked_value, width as u32),
                BitOpType::Rev32 => evaluator.eval_rev32(masked_value),
            };

            // A64 encoding: sf 1 0 11010110 00000 0000xx Rn(5) Rd(5)
            // opcode2: 000000=RBIT, 000001=REV16, 000010=REV32/REV(32), 000011=REV(64)
            //          000100=CLZ, 000101=CLS
            let opcode2: u64 = match op {
                BitOpType::Rbit => 0b000000,
                BitOpType::Rev16 => 0b000001,
                BitOpType::Rev32 => 0b000010,
                BitOpType::Rev => {
                    if sf == 1 {
                        0b000011
                    } else {
                        0b000010
                    }
                }
                BitOpType::Clz => 0b000100,
                BitOpType::Cls => 0b000101,
            };

            let encoding = (sf << 31)
                | (0b10u64 << 29)
                | (0b11010110u64 << 21)
                | (0b00000u64 << 16)
                | (opcode2 << 10)
                | (1 << 5) // Rn = X1
                | 0; // Rd = X0

            let mut initial_state = ProcessorState::default();
            initial_state.gp_regs.insert(1, masked_value);

            let mut expected_state = ProcessorState::default();
            expected_state.gp_regs.insert(0, result.dest_value);

            let mut assertions = Vec::new();
            if width == 32 {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg32(0),
                    expected: AssertionValue::U32(result.dest_value as u32),
                    message: format!("W0 should be 0x{:08X}", result.dest_value as u32),
                });
            } else {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg(0),
                    expected: AssertionValue::U64(result.dest_value),
                    message: format!("X0 should be 0x{:016X}", result.dest_value),
                });
            }

            let size_suffix = if sf == 1 { "64" } else { "32" };
            let provenance = Provenance::new(
                instr.name.as_str(),
                &enc_analysis.name,
                format!("{} X0, X1", op_name),
                Requirement::RegisterWrite {
                    reg_type: if sf == 1 {
                        RegType::Gp64
                    } else {
                        RegType::Gp32
                    },
                    dest_field: "Rd".to_string(),
                },
                format!("{} ({})", description, size_suffix),
            );

            let mut field_values = HashMap::new();
            field_values.insert("sf".to_string(), sf);
            field_values.insert("Rn".to_string(), 1);
            field_values.insert("Rd".to_string(), 0);

            tests.push(ExecutionTest {
                id: format!(
                    "{}_{}_oracle_{}_{}",
                    enc_analysis.name,
                    op_name.to_lowercase(),
                    size_suffix,
                    test_idx
                ),
                provenance,
                description: format!("Test {} {}-bit: {} (oracle)", op_name, width, description),
                encoding_name: enc_analysis.name.clone(),
                iset: enc_analysis.iset,
                encoding,
                encoding_width: enc_analysis.opcode_pattern.width,
                field_values,
                initial_state,
                expected_state,
                expected_memory: vec![],
                assertions,
                path_id: None,
                category: ExecutionTestCategory::Normal,
            });
        }
    }

    tests
}

/// Generate oracle tests for conditional select (CSEL, CSINC, CSINV, CSNEG)
/// NOTE: QEMU initializes with Z=1, so we use condition EQ (0000)
/// which is true when Z=1. This means we always select Rn path.
fn generate_cond_select_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    evaluator: &TestEvaluator,
    op: CondSelectOp,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();
    let op_name = match op {
        CondSelectOp::Csel => "CSEL",
        CondSelectOp::Csinc => "CSINC",
        CondSelectOp::Csinv => "CSINV",
        CondSelectOp::Csneg => "CSNEG",
    };

    // Test cases: (rn, rm, description)
    // Using condition EQ (Z=1), which is true with QEMU's default flags (Z=1)
    // So result is always Rn (for CSEL) or Rn (not Rm+1/~Rm/-Rm for others)
    let test_cases: Vec<(u64, u64, &str)> = vec![
        (100, 200, "different values"),
        (0, 0, "zero values"),
        (0xFFFF_FFFF_FFFF_FFFF, 0, "max Rn, zero Rm"),
        (0, 0xFFFF_FFFF_FFFF_FFFF, "zero Rn, max Rm"),
        (0x1234_5678, 0xABCD_EF01, "random pattern"),
        (1, 1, "both one"),
    ];

    for (test_idx, (rn, rm, description)) in test_cases.iter().enumerate() {
        for sf in [0u64, 1u64] {
            let width = if sf == 1 { 64 } else { 32 };
            let mask = if width == 32 { 0xFFFF_FFFF } else { u64::MAX };

            let masked_rn = *rn & mask;
            let masked_rm = *rm & mask;

            // EQ condition is true when Z=1 (QEMU default), so we always take the Rn path
            let cond_true = true;
            let result = match op {
                CondSelectOp::Csel => {
                    evaluator.eval_csel(masked_rn, masked_rm, cond_true, width as u32)
                }
                CondSelectOp::Csinc => {
                    evaluator.eval_csinc(masked_rn, masked_rm, cond_true, width as u32)
                }
                CondSelectOp::Csinv => {
                    evaluator.eval_csinv(masked_rn, masked_rm, cond_true, width as u32)
                }
                CondSelectOp::Csneg => {
                    evaluator.eval_csneg(masked_rn, masked_rm, cond_true, width as u32)
                }
            };

            // A64 encoding: sf op(1) 0 11010100 Rm(5) cond(4) op2(2) Rn(5) Rd(5)
            let (op_bit, op2): (u64, u64) = match op {
                CondSelectOp::Csel => (0, 0b00),
                CondSelectOp::Csinc => (0, 0b01),
                CondSelectOp::Csinv => (1, 0b00),
                CondSelectOp::Csneg => (1, 0b01),
            };

            // Use condition EQ (0000) which is true when Z=1 (QEMU default)
            let cond: u64 = 0b0000; // EQ

            let encoding = (sf << 31)
                | (op_bit << 30)
                | (0b011010100u64 << 21)
                | (2 << 16) // Rm = X2
                | (cond << 12)
                | (op2 << 10)
                | (1 << 5) // Rn = X1
                | 0; // Rd = X0

            let mut initial_state = ProcessorState::default();
            initial_state.gp_regs.insert(1, masked_rn);
            initial_state.gp_regs.insert(2, masked_rm);

            let mut expected_state = ProcessorState::default();
            expected_state.gp_regs.insert(0, result.dest_value);

            let mut assertions = Vec::new();
            if width == 32 {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg32(0),
                    expected: AssertionValue::U32(result.dest_value as u32),
                    message: format!("W0 should be 0x{:08X}", result.dest_value as u32),
                });
            } else {
                assertions.push(TestAssertion {
                    check: AssertionCheck::GpReg(0),
                    expected: AssertionValue::U64(result.dest_value),
                    message: format!("X0 should be 0x{:016X}", result.dest_value),
                });
            }

            let size_suffix = if sf == 1 { "64" } else { "32" };
            let provenance = Provenance::new(
                instr.name.as_str(),
                &enc_analysis.name,
                format!("{} X0, X1, X2, EQ", op_name),
                Requirement::RegisterWrite {
                    reg_type: if sf == 1 {
                        RegType::Gp64
                    } else {
                        RegType::Gp32
                    },
                    dest_field: "Rd".to_string(),
                },
                format!("{} ({})", description, size_suffix),
            );

            let mut field_values = HashMap::new();
            field_values.insert("sf".to_string(), sf);
            field_values.insert("Rm".to_string(), 2);
            field_values.insert("cond".to_string(), cond);
            field_values.insert("Rn".to_string(), 1);
            field_values.insert("Rd".to_string(), 0);

            tests.push(ExecutionTest {
                id: format!(
                    "{}_{}_oracle_{}_{}",
                    enc_analysis.name,
                    op_name.to_lowercase(),
                    size_suffix,
                    test_idx
                ),
                provenance,
                description: format!("Test {} {}-bit: {} (oracle)", op_name, width, description),
                encoding_name: enc_analysis.name.clone(),
                iset: enc_analysis.iset,
                encoding,
                encoding_width: enc_analysis.opcode_pattern.width,
                field_values,
                initial_state,
                expected_state,
                expected_memory: vec![],
                assertions,
                path_id: None,
                category: ExecutionTestCategory::Normal,
            });
        }
    }

    tests
}

// ============================================================================
// Load/Store Oracle Test Generation
// ============================================================================

/// Generate oracle tests for LDR unsigned immediate
fn generate_load_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    evaluator: &TestEvaluator,
    size: u8,
    signed: bool,
    addressing: LoadStoreAddressing,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();

    // Only handle unsigned immediate for now
    if !matches!(addressing, LoadStoreAddressing::UnsignedImm) {
        return tests;
    }

    let size_name = match size {
        0 => "B",
        1 => "H",
        2 => "",
        3 => "",
        _ => return tests,
    };
    let sign_name = if signed { "S" } else { "" };
    let op_name = format!("LDR{}{}", sign_name, size_name);

    // Test cases: (base_addr, offset_imm12, memory_value, description)
    // Use offset 0 to simplify - base_addr directly points to data
    let test_cases: Vec<(u64, u64, u64, &str)> = vec![
        (0x1000, 0, 0, "zero value"),
        (0x1000, 0, 0xFF, "max byte"),
        (0x1000, 0, 0xFFFF, "max halfword"),
        (0x1000, 0, 0xFFFF_FFFF, "max word"),
        (0x1000, 0, 0x1234_5678_9ABC_DEF0, "large value"),
        (0x1000, 0, 0x80, "sign bit (byte)"),
        (0x1000, 0, 0x8000, "sign bit (halfword)"),
        (0x1000, 0, 0x8000_0000, "sign bit (word)"),
    ];

    for (test_idx, (base, imm12, mem_val, description)) in test_cases.iter().enumerate() {
        // For 64-bit loads (size=3) or signed extension, use 64-bit
        let use_64bit = size == 3 || signed;

        // Build encoding: size[31:30] | 111 | 0 | 01 | opc[23:22] | imm12[21:10] | Rn[9:5] | Rt[4:0]
        let opc = if signed {
            if size == 3 {
                0b10
            } else {
                0b10
            }
        } else {
            0b01
        };

        let encoding = ((size as u64) << 30)
            | (0b111u64 << 27)
            | (0b0u64 << 26)
            | (0b01u64 << 24)
            | ((opc as u64) << 22)
            | ((*imm12 & 0xFFF) << 10)
            | (1 << 5)  // Rn = X1 (base register)
            | 0; // Rt = X0 (destination)

        let mut initial_state = ProcessorState::default();
        initial_state.gp_regs.insert(1, *base);
        // Store memory value
        let scale = 1u64 << size;
        let address = base + imm12 * scale;
        initial_state
            .memory
            .insert(address, mem_val.to_le_bytes()[..scale as usize].to_vec());

        let (expected_state, assertions) =
            evaluator.compute_ldr_unsigned_imm_expected(encoding, &initial_state, *mem_val);

        let provenance = Provenance::new(
            instr.name.as_str(),
            &enc_analysis.name,
            format!("{} X0, [X1, #{}]", op_name, imm12 * scale),
            Requirement::RegisterWrite {
                reg_type: if use_64bit {
                    RegType::Gp64
                } else {
                    RegType::Gp32
                },
                dest_field: "Rt".to_string(),
            },
            description.to_string(),
        );

        tests.push(ExecutionTest {
            id: format!("{}_ldr_oracle_{}", enc_analysis.name, test_idx),
            provenance,
            description: format!("Test {}: {} (oracle)", op_name, description),
            encoding_name: enc_analysis.name.clone(),
            iset: enc_analysis.iset,
            encoding,
            encoding_width: 32,
            field_values: HashMap::new(),
            initial_state,
            expected_state,
            expected_memory: vec![],
            assertions,
            path_id: None,
            category: ExecutionTestCategory::Normal,
        });
    }

    tests
}

/// Generate oracle tests for STR unsigned immediate
fn generate_store_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    evaluator: &TestEvaluator,
    size: u8,
    addressing: LoadStoreAddressing,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();

    // Only handle unsigned immediate for now
    if !matches!(addressing, LoadStoreAddressing::UnsignedImm) {
        return tests;
    }

    let size_name = match size {
        0 => "B",
        1 => "H",
        2 => "",
        3 => "",
        _ => return tests,
    };
    let op_name = format!("STR{}", size_name);

    // Test cases: (base_addr, offset, value_to_store, description)
    let test_cases: Vec<(u64, u64, u64, &str)> = vec![
        (0x1000, 0, 0, "zero value"),
        (0x1000, 0, 0xFF, "byte value"),
        (0x1000, 0, 0x1234, "halfword value"),
        (0x1000, 0, 0x1234_5678, "word value"),
        (0x1000, 0, 0x1234_5678_9ABC_DEF0, "doubleword value"),
    ];

    for (test_idx, (base, imm12, value, description)) in test_cases.iter().enumerate() {
        // Build encoding: size[31:30] | 111 | 0 | 01 | 00 | imm12[21:10] | Rn[9:5] | Rt[4:0]
        let encoding = ((size as u64) << 30)
            | (0b111u64 << 27)
            | (0b0u64 << 26)
            | (0b01u64 << 24)
            | (0b00u64 << 22)  // STR
            | ((*imm12 & 0xFFF) << 10)
            | (1 << 5)  // Rn = X1 (base register)
            | 0; // Rt = X0 (source value)

        let mut initial_state = ProcessorState::default();
        initial_state.gp_regs.insert(1, *base); // Base address
        initial_state.gp_regs.insert(0, *value); // Value to store

        let (expected_state, assertions, address, store_value) =
            evaluator.compute_str_unsigned_imm_expected(encoding, &initial_state);

        let scale = 1u64 << size;
        let provenance = Provenance::new(
            instr.name.as_str(),
            &enc_analysis.name,
            format!("{} X0, [X1, #{}]", op_name, imm12 * scale),
            Requirement::MemoryAccess {
                op: MemOp::Store,
                size_bits: (scale * 8) as u32,
                addressing: "immediate".to_string(),
            },
            description.to_string(),
        );

        tests.push(ExecutionTest {
            id: format!("{}_str_oracle_{}", enc_analysis.name, test_idx),
            provenance,
            description: format!("Test {}: {} (oracle)", op_name, description),
            encoding_name: enc_analysis.name.clone(),
            iset: enc_analysis.iset,
            encoding,
            encoding_width: 32,
            field_values: HashMap::new(),
            initial_state,
            expected_state,
            expected_memory: vec![MemoryChange {
                address,
                value: store_value.to_le_bytes()[..scale as usize].to_vec(),
                size: scale as u32,
            }],
            assertions,
            path_id: None,
            category: ExecutionTestCategory::Normal,
        });
    }

    tests
}

// ============================================================================
// Branch Oracle Test Generation
// ============================================================================

/// Generate oracle tests for B/BL
fn generate_branch_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    evaluator: &TestEvaluator,
    link: bool,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();
    let op_name = if link { "BL" } else { "B" };

    // Test cases: (offset_in_instructions, description)
    let test_cases: Vec<(i64, &str)> = vec![
        (1, "forward 1"),
        (4, "forward 4"),
        (-1, "backward 1"),
        (-4, "backward 4"),
        (0x1000, "large forward"),
        (-0x1000, "large backward"),
    ];

    let pc = 0x1000u64; // Starting PC

    for (test_idx, (offset, description)) in test_cases.iter().enumerate() {
        // Encode offset as signed 26-bit
        let imm26 = (*offset as u64) & 0x3FF_FFFF;
        let encoding = if link { 0b1u64 << 31 } else { 0u64 } | (0b00101u64 << 26) | imm26;

        let target = evaluator.compute_branch_target(encoding, pc);

        let mut initial_state = ProcessorState::default();
        initial_state.pc = Some(pc);

        let mut expected_state = initial_state.clone();
        expected_state.pc = Some(target);
        if link {
            expected_state.gp_regs.insert(30, pc + 4); // X30 = return address
        }

        let mut assertions = vec![TestAssertion {
            check: AssertionCheck::Pc,
            expected: AssertionValue::U64(target),
            message: format!("PC should be 0x{:X}", target),
        }];
        if link {
            assertions.push(TestAssertion {
                check: AssertionCheck::GpReg(30),
                expected: AssertionValue::U64(pc + 4),
                message: format!("X30 should be 0x{:X}", pc + 4),
            });
        }

        let provenance = Provenance::new(
            instr.name.as_str(),
            &enc_analysis.name,
            format!("{} 0x{:X}", op_name, target),
            Requirement::RegisterWrite {
                reg_type: RegType::Gp64,
                dest_field: "PC".to_string(),
            },
            description.to_string(),
        );

        tests.push(ExecutionTest {
            id: format!("{}_branch_oracle_{}", enc_analysis.name, test_idx),
            provenance,
            description: format!("Test {}: {} (oracle)", op_name, description),
            encoding_name: enc_analysis.name.clone(),
            iset: enc_analysis.iset,
            encoding,
            encoding_width: 32,
            field_values: HashMap::new(),
            initial_state,
            expected_state,
            expected_memory: vec![],
            assertions,
            path_id: None,
            category: ExecutionTestCategory::Normal,
        });
    }

    tests
}

/// Generate oracle tests for CBZ/CBNZ
fn generate_compare_branch_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    evaluator: &TestEvaluator,
    nonzero: bool,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();
    let op_name = if nonzero { "CBNZ" } else { "CBZ" };

    // Test cases: (reg_value, offset, sf, description)
    let test_cases: Vec<(u64, i64, u64, &str)> = vec![
        (0, 4, 1, "zero reg, forward"),
        (1, 4, 1, "nonzero reg, forward"),
        (0, -4, 1, "zero reg, backward"),
        (0xFFFF_FFFF_FFFF_FFFF, 4, 1, "all ones 64-bit"),
        (0, 4, 0, "zero reg 32-bit"),
        (1, 4, 0, "nonzero reg 32-bit"),
    ];

    let pc = 0x1000u64;

    for (test_idx, (reg_val, offset, sf, description)) in test_cases.iter().enumerate() {
        let imm19 = (*offset as u64) & 0x7_FFFF;
        let encoding =
            (*sf << 31) | (0b0110101u64 << 24) | ((nonzero as u64) << 24) | (imm19 << 5) | 0; // Rt = X0

        let target = evaluator.compute_compare_branch_target(encoding, pc, *reg_val, nonzero);

        let mut initial_state = ProcessorState::default();
        initial_state.pc = Some(pc);
        initial_state.gp_regs.insert(0, *reg_val);

        let mut expected_state = initial_state.clone();
        expected_state.pc = Some(target);

        let assertions = vec![TestAssertion {
            check: AssertionCheck::Pc,
            expected: AssertionValue::U64(target),
            message: format!("PC should be 0x{:X}", target),
        }];

        let provenance = Provenance::new(
            instr.name.as_str(),
            &enc_analysis.name,
            format!("{} X0, 0x{:X}", op_name, target),
            Requirement::RegisterWrite {
                reg_type: RegType::Gp64,
                dest_field: "PC".to_string(),
            },
            description.to_string(),
        );

        tests.push(ExecutionTest {
            id: format!("{}_cbranch_oracle_{}", enc_analysis.name, test_idx),
            provenance,
            description: format!("Test {}: {} (oracle)", op_name, description),
            encoding_name: enc_analysis.name.clone(),
            iset: enc_analysis.iset,
            encoding,
            encoding_width: 32,
            field_values: HashMap::new(),
            initial_state,
            expected_state,
            expected_memory: vec![],
            assertions,
            path_id: None,
            category: ExecutionTestCategory::Normal,
        });
    }

    tests
}

// ============================================================================
// A32 Oracle Test Generation
// ============================================================================

/// Generate oracle tests for A32 instructions
/// Uses proper A32 encoding layouts and evaluation
fn generate_a32_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    evaluator: &TestEvaluator,
    pattern: &InstructionPattern,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();

    // A32 test values for registers: (rn_val, rm_val, description)
    let reg_test_cases: Vec<(u32, u32, &str)> = vec![
        (100, 50, "simple values"),
        (0, 0, "zero values"),
        (0xFFFF_FFFF, 1, "max value"),
        (0x8000_0000, 0x8000_0000, "MSB set"),
        (0x1234_5678, 0x9ABC_DEF0, "mixed pattern"),
    ];

    // A32 immediate values: (imm8, rotate, description)
    let imm_test_cases: Vec<(u32, u32, &str)> = vec![
        (10, 0, "small immediate"),
        (0xFF, 0, "max imm8"),
        (0x80, 1, "rotated by 2"),
        (0x0F, 4, "rotated by 8"),
        (0, 0, "zero immediate"),
    ];

    match pattern {
        InstructionPattern::AddSubImmediate { is_sub, .. } => {
            // A32 ADD/SUB immediate encoding:
            // cond[31:28] | 00 | 1 | opcode[24:21] | S[20] | Rn[19:16] | Rd[15:12] | imm12[11:0]
            let opcode = if *is_sub { 0b0010u32 } else { 0b0100u32 };
            let base_encoding = enc_analysis.opcode_pattern.fixed_value as u32;

            for (test_idx, (imm8, rotate, description)) in imm_test_cases.iter().enumerate() {
                for rn_val in [0u32, 100, 0x7FFF_FFFF, 0x8000_0000, 0xFFFF_FFFF] {
                    let imm12 = (*rotate << 8) | *imm8;
                    // Build proper A32 encoding with Rd=0, Rn=1
                    let encoding = (base_encoding & 0xF000_0000) // Keep condition
                        | (0b001 << 25)      // Data processing immediate
                        | (opcode << 21)     // ADD or SUB
                        | (1 << 16)          // Rn = R1
                        | (0 << 12)          // Rd = R0
                        | imm12;

                    let mut initial_state = ProcessorState::default();
                    initial_state.gp_regs.insert(1, rn_val as u64);

                    let (expected_state, assertions) =
                        evaluator.compute_a32_add_sub_imm_expected(encoding as u64, &initial_state);

                    let op_name = if *is_sub { "SUB" } else { "ADD" };
                    let expanded_imm = evaluator.expand_a32_imm12(imm12);
                    let provenance = Provenance::new(
                        instr.name.as_str(),
                        &enc_analysis.name,
                        format!("{} R0, R1, #{}", op_name, expanded_imm),
                        Requirement::RegisterWrite {
                            reg_type: RegType::Gp32,
                            dest_field: "Rd".to_string(),
                        },
                        format!("{} (Rn=0x{:08X})", description, rn_val),
                    );

                    tests.push(ExecutionTest {
                        id: format!(
                            "{}_a32_add_sub_imm_{}_{:x}",
                            enc_analysis.name, test_idx, rn_val
                        ),
                        provenance,
                        description: format!("Test A32 {}: {} (oracle)", op_name, description),
                        encoding_name: enc_analysis.name.clone(),
                        iset: enc_analysis.iset,
                        encoding: encoding as u64,
                        encoding_width: 32,
                        field_values: HashMap::new(),
                        initial_state,
                        expected_state,
                        expected_memory: vec![],
                        assertions,
                        path_id: None,
                        category: ExecutionTestCategory::Normal,
                    });
                }
            }
        }
        InstructionPattern::AddSubShiftedReg { is_sub, .. } => {
            // A32 ADD/SUB register encoding:
            // cond[31:28] | 00 | 0 | opcode[24:21] | S[20] | Rn[19:16] | Rd[15:12] | shift_imm[11:7] | type[6:5] | 0 | Rm[3:0]
            let opcode = if *is_sub { 0b0010u32 } else { 0b0100u32 };
            let base_encoding = enc_analysis.opcode_pattern.fixed_value as u32;

            for (test_idx, (rn_val, rm_val, description)) in reg_test_cases.iter().enumerate() {
                // Build proper A32 encoding with Rd=0, Rn=1, Rm=2
                let encoding = (base_encoding & 0xF000_0000) // Keep condition
                    | (0b000 << 25)      // Data processing register
                    | (opcode << 21)     // ADD or SUB
                    | (1 << 16)          // Rn = R1
                    | (0 << 12)          // Rd = R0
                    | (0 << 7)           // shift_imm = 0
                    | (0b00 << 5)        // LSL
                    | 2; // Rm = R2

                let mut initial_state = ProcessorState::default();
                initial_state.gp_regs.insert(1, *rn_val as u64);
                initial_state.gp_regs.insert(2, *rm_val as u64);

                let (expected_state, assertions) =
                    evaluator.compute_a32_add_sub_reg_expected(encoding as u64, &initial_state);

                let op_name = if *is_sub { "SUB" } else { "ADD" };
                let provenance = Provenance::new(
                    instr.name.as_str(),
                    &enc_analysis.name,
                    format!("{} R0, R1, R2", op_name),
                    Requirement::RegisterWrite {
                        reg_type: RegType::Gp32,
                        dest_field: "Rd".to_string(),
                    },
                    description.to_string(),
                );

                tests.push(ExecutionTest {
                    id: format!("{}_a32_add_sub_reg_{}", enc_analysis.name, test_idx),
                    provenance,
                    description: format!("Test A32 {}: {} (oracle)", op_name, description),
                    encoding_name: enc_analysis.name.clone(),
                    iset: enc_analysis.iset,
                    encoding: encoding as u64,
                    encoding_width: 32,
                    field_values: HashMap::new(),
                    initial_state,
                    expected_state,
                    expected_memory: vec![],
                    assertions,
                    path_id: None,
                    category: ExecutionTestCategory::Normal,
                });
            }
        }
        InstructionPattern::LogicalImmediate { op } => {
            // A32 logical immediate encoding
            let opcode = match op {
                LogicalOp::And | LogicalOp::Ands => 0b0000u32,
                LogicalOp::Xor => 0b0001u32,
                LogicalOp::Or => 0b1100u32,
            };
            let base_encoding = enc_analysis.opcode_pattern.fixed_value as u32;

            for (test_idx, (imm8, rotate, description)) in imm_test_cases.iter().enumerate() {
                for rn_val in [0u32, 0xFF, 0xAAAA_AAAA, 0x5555_5555, 0xFFFF_FFFF] {
                    let imm12 = (*rotate << 8) | *imm8;
                    let encoding = (base_encoding & 0xF000_0000)
                        | (0b001 << 25)
                        | (opcode << 21)
                        | (1 << 16)
                        | (0 << 12)
                        | imm12;

                    let mut initial_state = ProcessorState::default();
                    initial_state.gp_regs.insert(1, rn_val as u64);

                    let (expected_state, assertions) =
                        evaluator.compute_a32_logical_imm_expected(encoding as u64, &initial_state);

                    let op_name = match op {
                        LogicalOp::And | LogicalOp::Ands => "AND",
                        LogicalOp::Or => "ORR",
                        LogicalOp::Xor => "EOR",
                    };
                    let expanded_imm = evaluator.expand_a32_imm12(imm12);
                    let provenance = Provenance::new(
                        instr.name.as_str(),
                        &enc_analysis.name,
                        format!("{} R0, R1, #{}", op_name, expanded_imm),
                        Requirement::RegisterWrite {
                            reg_type: RegType::Gp32,
                            dest_field: "Rd".to_string(),
                        },
                        format!("{} (Rn=0x{:08X})", description, rn_val),
                    );

                    tests.push(ExecutionTest {
                        id: format!(
                            "{}_a32_logical_imm_{}_{:x}",
                            enc_analysis.name, test_idx, rn_val
                        ),
                        provenance,
                        description: format!("Test A32 {}: {} (oracle)", op_name, description),
                        encoding_name: enc_analysis.name.clone(),
                        iset: enc_analysis.iset,
                        encoding: encoding as u64,
                        encoding_width: 32,
                        field_values: HashMap::new(),
                        initial_state,
                        expected_state,
                        expected_memory: vec![],
                        assertions,
                        path_id: None,
                        category: ExecutionTestCategory::Normal,
                    });
                }
            }
        }
        InstructionPattern::MoveImmediate { op } => {
            // A32 MOV immediate
            let opcode = match op {
                MoveOp::Movz => 0b1101u32, // MOV
                MoveOp::Movn => 0b1111u32, // MVN
                MoveOp::Movk => 0b1101u32, // Treat as MOV
            };
            let base_encoding = enc_analysis.opcode_pattern.fixed_value as u32;

            for (test_idx, (imm8, rotate, description)) in imm_test_cases.iter().enumerate() {
                let imm12 = (*rotate << 8) | *imm8;
                let encoding = (base_encoding & 0xF000_0000)
                    | (0b001 << 25)
                    | (opcode << 21)
                    | (0 << 12) // Rd = R0
                    | imm12;

                let initial_state = ProcessorState::default();
                let (expected_state, assertions) =
                    evaluator.compute_a32_mov_imm_expected(encoding as u64, &initial_state);

                let op_name = match op {
                    MoveOp::Movz | MoveOp::Movk => "MOV",
                    MoveOp::Movn => "MVN",
                };
                let expanded_imm = evaluator.expand_a32_imm12(imm12);
                let provenance = Provenance::new(
                    instr.name.as_str(),
                    &enc_analysis.name,
                    format!("{} R0, #{}", op_name, expanded_imm),
                    Requirement::RegisterWrite {
                        reg_type: RegType::Gp32,
                        dest_field: "Rd".to_string(),
                    },
                    description.to_string(),
                );

                tests.push(ExecutionTest {
                    id: format!("{}_a32_mov_imm_{}", enc_analysis.name, test_idx),
                    provenance,
                    description: format!("Test A32 {}: {} (oracle)", op_name, description),
                    encoding_name: enc_analysis.name.clone(),
                    iset: enc_analysis.iset,
                    encoding: encoding as u64,
                    encoding_width: 32,
                    field_values: HashMap::new(),
                    initial_state,
                    expected_state,
                    expected_memory: vec![],
                    assertions,
                    path_id: None,
                    category: ExecutionTestCategory::Normal,
                });
            }
        }
        InstructionPattern::Multiply { accumulate, .. } => {
            // A32 MUL/MLA encoding:
            // cond[31:28] | 0000 | 00A | S | Rd[19:16] | Ra[15:12] | Rm[11:8] | 1001 | Rn[3:0]
            let base_encoding = enc_analysis.opcode_pattern.fixed_value as u32;

            for (test_idx, (rn_val, rm_val, description)) in reg_test_cases.iter().enumerate() {
                let encoding = (base_encoding & 0xF000_0000)
                    | (0b0000_000 << 21)
                    | ((*accumulate as u32) << 21)
                    | (0 << 16)          // Rd = R0
                    | (0xF << 12)        // Ra = R15 (unused for MUL)
                    | (2 << 8)           // Rm = R2
                    | (0b1001 << 4)
                    | 1; // Rn = R1

                let mut initial_state = ProcessorState::default();
                initial_state.gp_regs.insert(1, *rn_val as u64);
                initial_state.gp_regs.insert(2, *rm_val as u64);

                let (expected_state, assertions) =
                    evaluator.compute_a32_multiply_expected(encoding as u64, &initial_state);

                let op_name = if *accumulate { "MLA" } else { "MUL" };
                let provenance = Provenance::new(
                    instr.name.as_str(),
                    &enc_analysis.name,
                    format!("{} R0, R1, R2", op_name),
                    Requirement::RegisterWrite {
                        reg_type: RegType::Gp32,
                        dest_field: "Rd".to_string(),
                    },
                    description.to_string(),
                );

                tests.push(ExecutionTest {
                    id: format!("{}_a32_mul_{}", enc_analysis.name, test_idx),
                    provenance,
                    description: format!("Test A32 {}: {} (oracle)", op_name, description),
                    encoding_name: enc_analysis.name.clone(),
                    iset: enc_analysis.iset,
                    encoding: encoding as u64,
                    encoding_width: 32,
                    field_values: HashMap::new(),
                    initial_state,
                    expected_state,
                    expected_memory: vec![],
                    assertions,
                    path_id: None,
                    category: ExecutionTestCategory::Normal,
                });
            }
        }
        InstructionPattern::LogicalShiftedReg { op } => {
            // Similar to AddSubShiftedReg but for logical ops
            let opcode = match op {
                LogicalOp::And | LogicalOp::Ands => 0b0000u32,
                LogicalOp::Xor => 0b0001u32,
                LogicalOp::Or => 0b1100u32,
            };
            let base_encoding = enc_analysis.opcode_pattern.fixed_value as u32;

            for (test_idx, (rn_val, rm_val, description)) in reg_test_cases.iter().enumerate() {
                let encoding = (base_encoding & 0xF000_0000)
                    | (0b000 << 25)
                    | (opcode << 21)
                    | (1 << 16)
                    | (0 << 12)
                    | (0 << 7)
                    | (0b00 << 5)
                    | 2;

                let mut initial_state = ProcessorState::default();
                initial_state.gp_regs.insert(1, *rn_val as u64);
                initial_state.gp_regs.insert(2, *rm_val as u64);

                // Compute expected using the logical immediate method (simplified)
                let rn = *rn_val;
                let rm = *rm_val;
                let result = match op {
                    LogicalOp::And | LogicalOp::Ands => rn & rm,
                    LogicalOp::Or => rn | rm,
                    LogicalOp::Xor => rn ^ rm,
                };

                let mut expected_state = initial_state.clone();
                expected_state.gp_regs.insert(0, result as u64);

                let assertions = vec![TestAssertion {
                    check: AssertionCheck::GpReg32(0),
                    expected: AssertionValue::U32(result),
                    message: format!("R0 should be 0x{:08X}", result),
                }];

                let op_name = match op {
                    LogicalOp::And | LogicalOp::Ands => "AND",
                    LogicalOp::Or => "ORR",
                    LogicalOp::Xor => "EOR",
                };
                let provenance = Provenance::new(
                    instr.name.as_str(),
                    &enc_analysis.name,
                    format!("{} R0, R1, R2", op_name),
                    Requirement::RegisterWrite {
                        reg_type: RegType::Gp32,
                        dest_field: "Rd".to_string(),
                    },
                    description.to_string(),
                );

                tests.push(ExecutionTest {
                    id: format!("{}_a32_logical_reg_{}", enc_analysis.name, test_idx),
                    provenance,
                    description: format!("Test A32 {}: {} (oracle)", op_name, description),
                    encoding_name: enc_analysis.name.clone(),
                    iset: enc_analysis.iset,
                    encoding: encoding as u64,
                    encoding_width: 32,
                    field_values: HashMap::new(),
                    initial_state,
                    expected_state,
                    expected_memory: vec![],
                    assertions,
                    path_id: None,
                    category: ExecutionTestCategory::Normal,
                });
            }
        }
        _ => {
            // Unknown pattern - skip oracle tests for A32
        }
    }

    tests
}

/// Build A32 encoding with specified register fields
fn build_a32_encoding(enc_analysis: &EncodingAnalysis, rd: u64, rn: u64, rm: Option<u64>) -> u64 {
    let mut encoding = enc_analysis.opcode_pattern.fixed_value;

    // A32 register field positions vary by instruction type
    // Common positions: Rd[15:12], Rn[19:16], Rm[3:0], Rs[11:8]
    for field in &enc_analysis.fields {
        match field.name.as_str() {
            "Rd" => {
                let mask = ((1u64 << field.width) - 1) << field.start;
                encoding = (encoding & !mask) | ((rd << field.start) & mask);
            }
            "Rn" => {
                let mask = ((1u64 << field.width) - 1) << field.start;
                encoding = (encoding & !mask) | ((rn << field.start) & mask);
            }
            "Rm" => {
                if let Some(rm_val) = rm {
                    let mask = ((1u64 << field.width) - 1) << field.start;
                    encoding = (encoding & !mask) | ((rm_val << field.start) & mask);
                }
            }
            _ => {}
        }
    }

    encoding
}

// ============================================================================
// T32 Oracle Test Generation
// ============================================================================

/// Generate oracle tests for T32 (32-bit Thumb) instructions
fn generate_t32_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    evaluator: &TestEvaluator,
    pattern: &InstructionPattern,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();
    let width = 32u32;

    let test_cases: Vec<(u64, u64, &str)> = vec![
        (100, 50, "simple values"),
        (0, 0, "zero values"),
        (0xFFFF_FFFF, 1, "max value"),
        (0x1234_5678, 0xABCD_EF01, "mixed pattern"),
    ];

    match pattern {
        InstructionPattern::AddSubImmediate { is_sub, set_flags } => {
            for (test_idx, (op1, op2, description)) in test_cases.iter().enumerate() {
                let masked_op1 = *op1 & 0xFFFF_FFFF;
                let masked_op2 = *op2 & 0xFFFF_FFFF;

                let result = if *is_sub {
                    evaluator.eval_sub(masked_op1, masked_op2, width, *set_flags)
                } else {
                    evaluator.eval_add(masked_op1, masked_op2, width, *set_flags)
                };

                let encoding = build_thumb_encoding(enc_analysis, 0, 1, Some(2));

                let mut initial_state = ProcessorState::default();
                initial_state.gp_regs.insert(1, masked_op1);
                initial_state.gp_regs.insert(2, masked_op2);

                let mut expected_state = ProcessorState::default();
                expected_state.gp_regs.insert(0, result.dest_value);

                let assertions = vec![TestAssertion {
                    check: AssertionCheck::GpReg32(0),
                    expected: AssertionValue::U32(result.dest_value as u32),
                    message: format!("R0 should be 0x{:08X}", result.dest_value as u32),
                }];

                let op_name = if *is_sub { "SUB" } else { "ADD" };
                let provenance = Provenance::new(
                    instr.name.as_str(),
                    &enc_analysis.name,
                    format!("{}.W R0, R1, R2", op_name),
                    Requirement::RegisterWrite {
                        reg_type: RegType::Gp32,
                        dest_field: "Rd".to_string(),
                    },
                    description.to_string(),
                );

                tests.push(ExecutionTest {
                    id: format!("{}_t32_oracle_{}", enc_analysis.name, test_idx),
                    provenance,
                    description: format!("Test T32 {}: {} (oracle)", op_name, description),
                    encoding_name: enc_analysis.name.clone(),
                    iset: enc_analysis.iset,
                    encoding,
                    encoding_width: 32,
                    field_values: HashMap::new(),
                    initial_state,
                    expected_state,
                    expected_memory: vec![],
                    assertions,
                    path_id: None,
                    category: ExecutionTestCategory::Normal,
                });
            }
        }
        InstructionPattern::Multiply { .. } => {
            for (test_idx, (op1, op2, description)) in test_cases.iter().enumerate() {
                let masked_op1 = *op1 & 0xFFFF_FFFF;
                let masked_op2 = *op2 & 0xFFFF_FFFF;

                let result = evaluator.eval_mul(masked_op1, masked_op2, width);

                let encoding = build_thumb_encoding(enc_analysis, 0, 1, Some(2));

                let mut initial_state = ProcessorState::default();
                initial_state.gp_regs.insert(1, masked_op1);
                initial_state.gp_regs.insert(2, masked_op2);

                let mut expected_state = ProcessorState::default();
                expected_state.gp_regs.insert(0, result.dest_value);

                let assertions = vec![TestAssertion {
                    check: AssertionCheck::GpReg32(0),
                    expected: AssertionValue::U32(result.dest_value as u32),
                    message: format!("R0 should be 0x{:08X}", result.dest_value as u32),
                }];

                let provenance = Provenance::new(
                    instr.name.as_str(),
                    &enc_analysis.name,
                    "MUL R0, R1, R2",
                    Requirement::RegisterWrite {
                        reg_type: RegType::Gp32,
                        dest_field: "Rd".to_string(),
                    },
                    description.to_string(),
                );

                tests.push(ExecutionTest {
                    id: format!("{}_t32_oracle_{}", enc_analysis.name, test_idx),
                    provenance,
                    description: format!("Test T32 MUL: {} (oracle)", description),
                    encoding_name: enc_analysis.name.clone(),
                    iset: enc_analysis.iset,
                    encoding,
                    encoding_width: 32,
                    field_values: HashMap::new(),
                    initial_state,
                    expected_state,
                    expected_memory: vec![],
                    assertions,
                    path_id: None,
                    category: ExecutionTestCategory::Normal,
                });
            }
        }
        InstructionPattern::Divide { signed } => {
            let div_cases: Vec<(u64, u64, &str)> = vec![
                (100, 10, "exact division"),
                (100, 3, "with remainder"),
                (0, 10, "zero dividend"),
                (10, 0, "divide by zero"),
            ];

            for (test_idx, (dividend, divisor, description)) in div_cases.iter().enumerate() {
                let result = if *signed {
                    evaluator.eval_sdiv(*dividend, *divisor, width)
                } else {
                    evaluator.eval_udiv(*dividend, *divisor, width)
                };

                let encoding = build_thumb_encoding(enc_analysis, 0, 1, Some(2));

                let mut initial_state = ProcessorState::default();
                initial_state.gp_regs.insert(1, *dividend);
                initial_state.gp_regs.insert(2, *divisor);

                let mut expected_state = ProcessorState::default();
                expected_state.gp_regs.insert(0, result.dest_value);

                let assertions = vec![TestAssertion {
                    check: AssertionCheck::GpReg32(0),
                    expected: AssertionValue::U32(result.dest_value as u32),
                    message: format!("R0 should be 0x{:08X}", result.dest_value as u32),
                }];

                let op_name = if *signed { "SDIV" } else { "UDIV" };
                let provenance = Provenance::new(
                    instr.name.as_str(),
                    &enc_analysis.name,
                    format!("{} R0, R1, R2", op_name),
                    Requirement::RegisterWrite {
                        reg_type: RegType::Gp32,
                        dest_field: "Rd".to_string(),
                    },
                    description.to_string(),
                );

                tests.push(ExecutionTest {
                    id: format!("{}_t32_oracle_{}", enc_analysis.name, test_idx),
                    provenance,
                    description: format!("Test T32 {}: {} (oracle)", op_name, description),
                    encoding_name: enc_analysis.name.clone(),
                    iset: enc_analysis.iset,
                    encoding,
                    encoding_width: 32,
                    field_values: HashMap::new(),
                    initial_state,
                    expected_state,
                    expected_memory: vec![],
                    assertions,
                    path_id: None,
                    category: ExecutionTestCategory::Normal,
                });
            }
        }
        _ => {}
    }

    tests
}

/// Build Thumb encoding with specified register fields
fn build_thumb_encoding(enc_analysis: &EncodingAnalysis, rd: u64, rn: u64, rm: Option<u64>) -> u64 {
    let mut encoding = enc_analysis.opcode_pattern.fixed_value;

    // T32 register field positions vary
    for field in &enc_analysis.fields {
        match field.name.as_str() {
            "Rd" => {
                let mask = ((1u64 << field.width) - 1) << field.start;
                encoding = (encoding & !mask) | ((rd << field.start) & mask);
            }
            "Rn" => {
                let mask = ((1u64 << field.width) - 1) << field.start;
                encoding = (encoding & !mask) | ((rn << field.start) & mask);
            }
            "Rm" => {
                if let Some(rm_val) = rm {
                    let mask = ((1u64 << field.width) - 1) << field.start;
                    encoding = (encoding & !mask) | ((rm_val << field.start) & mask);
                }
            }
            _ => {}
        }
    }

    encoding
}

// ============================================================================
// T16 Oracle Test Generation
// ============================================================================

/// Generate oracle tests for T16 (16-bit Thumb) instructions
fn generate_t16_oracle_tests(
    instr: &Instruction,
    enc_analysis: &EncodingAnalysis,
    evaluator: &TestEvaluator,
    pattern: &InstructionPattern,
) -> Vec<ExecutionTest> {
    let mut tests = Vec::new();
    let width = 32u32;

    // T16 uses low registers (R0-R7) typically
    let test_cases: Vec<(u64, u64, &str)> = vec![
        (10, 5, "simple values"),
        (0, 0, "zero values"),
        (255, 1, "byte-sized values"),
        (100, 50, "medium values"),
    ];

    match pattern {
        InstructionPattern::AddSubImmediate { is_sub, set_flags } => {
            for (test_idx, (op1, op2, description)) in test_cases.iter().enumerate() {
                let result = if *is_sub {
                    evaluator.eval_sub(*op1, *op2, width, *set_flags)
                } else {
                    evaluator.eval_add(*op1, *op2, width, *set_flags)
                };

                // T16 typically uses R0-R7, with Rd in bits [2:0], Rn in bits [5:3]
                let encoding = build_t16_encoding(enc_analysis, 0, 1, Some(2));

                let mut initial_state = ProcessorState::default();
                initial_state.gp_regs.insert(1, *op1);
                initial_state.gp_regs.insert(2, *op2);

                let mut expected_state = ProcessorState::default();
                expected_state.gp_regs.insert(0, result.dest_value);

                let mut assertions = vec![TestAssertion {
                    check: AssertionCheck::GpReg32(0),
                    expected: AssertionValue::U32(result.dest_value as u32),
                    message: format!("R0 should be 0x{:08X}", result.dest_value as u32),
                }];

                if *set_flags {
                    if let Some(nzcv) = result.nzcv {
                        assertions.push(TestAssertion {
                            check: AssertionCheck::Flag(ProcessorFlag::Z),
                            expected: AssertionValue::Bool(nzcv.z),
                            message: "Z flag".to_string(),
                        });
                    }
                }

                let op_name = if *is_sub { "SUBS" } else { "ADDS" };
                let provenance = Provenance::new(
                    instr.name.as_str(),
                    &enc_analysis.name,
                    format!("{} R0, R1, R2", op_name),
                    Requirement::RegisterWrite {
                        reg_type: RegType::Gp32,
                        dest_field: "Rd".to_string(),
                    },
                    description.to_string(),
                );

                tests.push(ExecutionTest {
                    id: format!("{}_t16_oracle_{}", enc_analysis.name, test_idx),
                    provenance,
                    description: format!("Test T16 {}: {} (oracle)", op_name, description),
                    encoding_name: enc_analysis.name.clone(),
                    iset: enc_analysis.iset,
                    encoding,
                    encoding_width: 16,
                    field_values: HashMap::new(),
                    initial_state,
                    expected_state,
                    expected_memory: vec![],
                    assertions,
                    path_id: None,
                    category: ExecutionTestCategory::Normal,
                });
            }
        }
        InstructionPattern::LogicalShiftedReg { op } => {
            for (test_idx, (op1, op2, description)) in test_cases.iter().enumerate() {
                let set_flags = matches!(op, LogicalOp::Ands);
                let result = match op {
                    LogicalOp::And | LogicalOp::Ands => {
                        evaluator.eval_and(*op1, *op2, width, set_flags)
                    }
                    LogicalOp::Or => evaluator.eval_or(*op1, *op2, width),
                    LogicalOp::Xor => evaluator.eval_xor(*op1, *op2, width),
                };

                let encoding = build_t16_encoding(enc_analysis, 0, 1, Some(2));

                let mut initial_state = ProcessorState::default();
                initial_state.gp_regs.insert(1, *op1);
                initial_state.gp_regs.insert(2, *op2);

                let mut expected_state = ProcessorState::default();
                expected_state.gp_regs.insert(0, result.dest_value);

                let assertions = vec![TestAssertion {
                    check: AssertionCheck::GpReg32(0),
                    expected: AssertionValue::U32(result.dest_value as u32),
                    message: format!("R0 should be 0x{:08X}", result.dest_value as u32),
                }];

                let op_name = match op {
                    LogicalOp::And | LogicalOp::Ands => "ANDS",
                    LogicalOp::Or => "ORRS",
                    LogicalOp::Xor => "EORS",
                };

                let provenance = Provenance::new(
                    instr.name.as_str(),
                    &enc_analysis.name,
                    format!("{} R0, R1", op_name),
                    Requirement::RegisterWrite {
                        reg_type: RegType::Gp32,
                        dest_field: "Rd".to_string(),
                    },
                    description.to_string(),
                );

                tests.push(ExecutionTest {
                    id: format!("{}_t16_oracle_{}", enc_analysis.name, test_idx),
                    provenance,
                    description: format!("Test T16 {}: {} (oracle)", op_name, description),
                    encoding_name: enc_analysis.name.clone(),
                    iset: enc_analysis.iset,
                    encoding,
                    encoding_width: 16,
                    field_values: HashMap::new(),
                    initial_state,
                    expected_state,
                    expected_memory: vec![],
                    assertions,
                    path_id: None,
                    category: ExecutionTestCategory::Normal,
                });
            }
        }
        InstructionPattern::ShiftVariable { shift_type } => {
            let shift_cases: Vec<(u64, u64, &str)> = vec![
                (0xFF, 0, "no shift"),
                (0xFF, 4, "shift by 4"),
                (0x8000_0000, 1, "MSB set, shift 1"),
                (1, 31, "shift to MSB"),
            ];

            for (test_idx, (value, shift_amt, description)) in shift_cases.iter().enumerate() {
                let result = match shift_type {
                    ShiftType::Lsl => evaluator.eval_lsl(*value, *shift_amt as u32, width),
                    ShiftType::Lsr => evaluator.eval_lsr(*value, *shift_amt as u32, width),
                    ShiftType::Asr => evaluator.eval_asr(*value, *shift_amt as u32, width),
                    ShiftType::Ror => evaluator.eval_ror(*value, *shift_amt as u32, width),
                };

                let encoding = build_t16_encoding(enc_analysis, 0, 1, Some(2));

                let mut initial_state = ProcessorState::default();
                initial_state.gp_regs.insert(1, *value);
                initial_state.gp_regs.insert(2, *shift_amt);

                let mut expected_state = ProcessorState::default();
                expected_state.gp_regs.insert(0, result.dest_value);

                let assertions = vec![TestAssertion {
                    check: AssertionCheck::GpReg32(0),
                    expected: AssertionValue::U32(result.dest_value as u32),
                    message: format!("R0 should be 0x{:08X}", result.dest_value as u32),
                }];

                let op_name = match shift_type {
                    ShiftType::Lsl => "LSLS",
                    ShiftType::Lsr => "LSRS",
                    ShiftType::Asr => "ASRS",
                    ShiftType::Ror => "RORS",
                };

                let provenance = Provenance::new(
                    instr.name.as_str(),
                    &enc_analysis.name,
                    format!("{} R0, R1, R2", op_name),
                    Requirement::RegisterWrite {
                        reg_type: RegType::Gp32,
                        dest_field: "Rd".to_string(),
                    },
                    description.to_string(),
                );

                tests.push(ExecutionTest {
                    id: format!("{}_t16_oracle_{}", enc_analysis.name, test_idx),
                    provenance,
                    description: format!("Test T16 {}: {} (oracle)", op_name, description),
                    encoding_name: enc_analysis.name.clone(),
                    iset: enc_analysis.iset,
                    encoding,
                    encoding_width: 16,
                    field_values: HashMap::new(),
                    initial_state,
                    expected_state,
                    expected_memory: vec![],
                    assertions,
                    path_id: None,
                    category: ExecutionTestCategory::Normal,
                });
            }
        }
        InstructionPattern::Multiply { .. } => {
            for (test_idx, (op1, op2, description)) in test_cases.iter().enumerate() {
                let result = evaluator.eval_mul(*op1, *op2, width);

                let encoding = build_t16_encoding(enc_analysis, 0, 1, Some(2));

                let mut initial_state = ProcessorState::default();
                initial_state.gp_regs.insert(1, *op1);
                initial_state.gp_regs.insert(2, *op2);

                let mut expected_state = ProcessorState::default();
                expected_state.gp_regs.insert(0, result.dest_value);

                let assertions = vec![TestAssertion {
                    check: AssertionCheck::GpReg32(0),
                    expected: AssertionValue::U32(result.dest_value as u32),
                    message: format!("R0 should be 0x{:08X}", result.dest_value as u32),
                }];

                let provenance = Provenance::new(
                    instr.name.as_str(),
                    &enc_analysis.name,
                    "MULS R0, R1, R2",
                    Requirement::RegisterWrite {
                        reg_type: RegType::Gp32,
                        dest_field: "Rd".to_string(),
                    },
                    description.to_string(),
                );

                tests.push(ExecutionTest {
                    id: format!("{}_t16_oracle_{}", enc_analysis.name, test_idx),
                    provenance,
                    description: format!("Test T16 MUL: {} (oracle)", description),
                    encoding_name: enc_analysis.name.clone(),
                    iset: enc_analysis.iset,
                    encoding,
                    encoding_width: 16,
                    field_values: HashMap::new(),
                    initial_state,
                    expected_state,
                    expected_memory: vec![],
                    assertions,
                    path_id: None,
                    category: ExecutionTestCategory::Normal,
                });
            }
        }
        _ => {}
    }

    tests
}

/// Build T16 encoding with specified register fields
fn build_t16_encoding(enc_analysis: &EncodingAnalysis, rd: u64, rn: u64, rm: Option<u64>) -> u64 {
    let mut encoding = enc_analysis.opcode_pattern.fixed_value;

    // T16 register field positions - typically 3-bit fields for low registers
    for field in &enc_analysis.fields {
        match field.name.as_str() {
            "Rd" | "Rdn" => {
                let mask = ((1u64 << field.width) - 1) << field.start;
                encoding = (encoding & !mask) | ((rd << field.start) & mask);
            }
            "Rn" => {
                let mask = ((1u64 << field.width) - 1) << field.start;
                encoding = (encoding & !mask) | ((rn << field.start) & mask);
            }
            "Rm" => {
                if let Some(rm_val) = rm {
                    let mask = ((1u64 << field.width) - 1) << field.start;
                    encoding = (encoding & !mask) | ((rm_val << field.start) & mask);
                }
            }
            _ => {}
        }
    }

    encoding
}
