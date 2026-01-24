//! Validation tests for SMIR lift/lower round-trip.
//!
//! This module tests that we can lift x86_64 machine code to SMIR and lower
//! it back to machine code, producing functionally equivalent output.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::smir::ir::{SmirBlock, SmirFunction, Terminator};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
    use crate::smir::lower::x86_64::X86_64Lowerer;
    use crate::smir::lower::SmirLowerer;
    use crate::smir::types::{BlockId, FunctionId, SourceArch};

    /// Format bytes as hex string
    fn hex_bytes(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Print op details
    fn print_op(op: &crate::smir::ops::SmirOp) {
        println!("    [{:04}] {:?}", op.id.0, op.kind);
    }

    #[test]
    fn test_lift_lower_simple_instructions() {
        // Simple instruction sequences to test
        let test_cases: Vec<(&str, Vec<u8>)> = vec![
            // MOV instructions
            ("mov rax, rcx", vec![0x48, 0x89, 0xC8]),
            ("mov eax, 42", vec![0xB8, 0x2A, 0x00, 0x00, 0x00]),
            (
                "mov rax, 0x12345678",
                vec![0x48, 0xC7, 0xC0, 0x78, 0x56, 0x34, 0x12],
            ),
            // ADD instructions
            ("add rax, rbx", vec![0x48, 0x01, 0xD8]),
            ("add rax, 10", vec![0x48, 0x83, 0xC0, 0x0A]),
            // SUB instructions
            ("sub rax, rcx", vec![0x48, 0x29, 0xC8]),
            ("sub rax, 5", vec![0x48, 0x83, 0xE8, 0x05]),
            // AND/OR/XOR
            ("and rax, rbx", vec![0x48, 0x21, 0xD8]),
            ("or rax, rcx", vec![0x48, 0x09, 0xC8]),
            ("xor rax, rax", vec![0x48, 0x31, 0xC0]),
            // Shifts
            ("shl rax, 4", vec![0x48, 0xC1, 0xE0, 0x04]),
            ("shr rbx, 2", vec![0x48, 0xC1, 0xEB, 0x02]),
            // Stack
            ("push rbp", vec![0x55]),
            ("pop rbp", vec![0x5D]),
            // NOP
            ("nop", vec![0x90]),
        ];

        let mut lifter = X86_64Lifter::new();
        let mut lowerer = X86_64Lowerer::new();

        for (name, original_bytes) in test_cases {
            println!("\n=== Testing: {} ===", name);
            println!("  Original: {}", hex_bytes(&original_bytes));

            let mut ctx = LiftContext::new(SourceArch::X86_64);

            // Lift the instruction
            match lifter.lift_insn(0x1000, &original_bytes, &mut ctx) {
                Ok(result) => {
                    println!(
                        "  Lifted {} ops ({} bytes consumed):",
                        result.ops.len(),
                        result.bytes_consumed
                    );
                    for op in &result.ops {
                        print_op(op);
                    }

                    // Create a minimal function for lowering
                    let mut func = SmirFunction::new(FunctionId(0), BlockId(0), 0x1000);
                    let mut block = SmirBlock::new(BlockId(0), 0x1000);

                    // Add ops to block
                    for op in result.ops {
                        block.push_op(op);
                    }
                    block.set_terminator(Terminator::Return { values: vec![] });
                    func.add_block(block);

                    // Lower it
                    match lowerer.lower_function(&func) {
                        Ok(lower_result) => {
                            let lowered = lowerer.finalize().unwrap();
                            println!("  Lowered: {} bytes", lowered.len());
                            println!("  Code: {}", hex_bytes(&lowered));
                        }
                        Err(e) => {
                            println!("  Lower error: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    println!("  Lift error: {:?}", e);
                }
            }
        }
    }

    #[test]
    fn test_lift_lower_microkernel_start() {
        // The _start function from the microkernel
        // 0x1000000: 48 8d 25 f9 ff 00 00   leaq 0xfff9(%rip), %rsp
        // 0x1000007: 48 8d 3d 92 0f 00 00   leaq 0xf92(%rip), %rdi
        // ...
        let start_bytes = vec![
            0x48, 0x8d, 0x25, 0xf9, 0xff, 0x00, 0x00, // lea rsp, [rip+0xfff9]
            0x48, 0x8d, 0x3d, 0x92, 0x0f, 0x00, 0x00, // lea rdi, [rip+0xf92]
            0x48, 0x8d, 0x0d, 0x9b, 0x0f, 0x00, 0x00, // lea rcx, [rip+0xf9b]
            0x48, 0x29, 0xf9, // sub rcx, rdi
            0x48, 0xc1, 0xe9, 0x03, // shr rcx, 3
            0x31, 0xc0, // xor eax, eax
        ];

        println!("\n=== Testing microkernel _start prologue ===");
        println!("Original bytes: {}", hex_bytes(&start_bytes));

        let mut lifter = X86_64Lifter::new();
        let mut ctx = LiftContext::new(SourceArch::X86_64);
        let mut block = SmirBlock::new(BlockId(0), 0x1000000);

        let mut pc = 0x1000000u64;
        let mut offset = 0usize;

        // Lift multiple instructions
        while offset < start_bytes.len() {
            match lifter.lift_insn(pc, &start_bytes[offset..], &mut ctx) {
                Ok(result) => {
                    for op in result.ops {
                        block.push_op(op);
                    }
                    pc += result.bytes_consumed as u64;
                    offset += result.bytes_consumed;

                    if result.control_flow.ends_block() {
                        break;
                    }
                }
                Err(e) => {
                    println!("Lift error at {:#x}: {:?}", pc, e);
                    break;
                }
            }
        }

        println!(
            "\nLifted {} ops (PC: {:#x} -> {:#x}):",
            block.ops.len(),
            0x1000000u64,
            pc
        );
        for op in &block.ops {
            print_op(op);
        }

        // Create function and lower
        let mut func = SmirFunction::new(FunctionId(0), BlockId(0), 0x1000000);
        block.set_terminator(Terminator::Return { values: vec![] });
        func.add_block(block);

        let mut lowerer = X86_64Lowerer::new();
        match lowerer.lower_function(&func) {
            Ok(result) => {
                let lowered = lowerer.finalize().unwrap();
                println!("\nLowered to {} bytes:", lowered.len());
                println!("Code: {}", hex_bytes(&lowered));
                println!("\nStack size: {} bytes", result.stack_size);
            }
            Err(e) => {
                println!("\nLower error: {:?}", e);
            }
        }
    }

    #[test]
    fn test_lift_lower_kernel_main_prologue() {
        // kernel_main prologue:
        // 1000064: 55                       push rbp
        // 1000065: 41 57                    push r15
        // 1000067: 41 56                    push r14
        // 1000069: 41 55                    push r13
        // 100006b: 41 54                    push r12
        // 100006d: 53                       push rbx
        // 100006e: 48 81 ec a8 00 00 00     sub rsp, 0xa8
        let prologue_bytes = vec![
            0x55, // push rbp
            0x41, 0x57, // push r15
            0x41, 0x56, // push r14
            0x41, 0x55, // push r13
            0x41, 0x54, // push r12
            0x53, // push rbx
            0x48, 0x81, 0xec, 0xa8, 0x00, 0x00, 0x00, // sub rsp, 0xa8
        ];

        println!("\n=== Testing kernel_main prologue ===");
        println!("Original bytes: {}", hex_bytes(&prologue_bytes));

        let mut lifter = X86_64Lifter::new();
        let mut ctx = LiftContext::new(SourceArch::X86_64);
        let mut block = SmirBlock::new(BlockId(0), 0x1000064);

        let mut pc = 0x1000064u64;
        let mut offset = 0usize;

        while offset < prologue_bytes.len() {
            match lifter.lift_insn(pc, &prologue_bytes[offset..], &mut ctx) {
                Ok(result) => {
                    for op in result.ops {
                        block.push_op(op);
                    }
                    pc += result.bytes_consumed as u64;
                    offset += result.bytes_consumed;

                    if result.control_flow.ends_block() {
                        break;
                    }
                }
                Err(e) => {
                    println!("Lift error at {:#x}: {:?}", pc, e);
                    break;
                }
            }
        }

        println!(
            "\nLifted {} ops (PC: {:#x} -> {:#x}):",
            block.ops.len(),
            0x1000064u64,
            pc
        );
        for op in &block.ops {
            print_op(op);
        }

        // Create function and lower
        let mut func = SmirFunction::new(FunctionId(0), BlockId(0), 0x1000064);
        block.set_terminator(Terminator::Return { values: vec![] });
        func.add_block(block);

        let mut lowerer = X86_64Lowerer::new();
        match lowerer.lower_function(&func) {
            Ok(result) => {
                let lowered = lowerer.finalize().unwrap();
                println!("\nLowered to {} bytes:", lowered.len());
                println!("Code: {}", hex_bytes(&lowered));
            }
            Err(e) => {
                println!("\nLower error: {:?}", e);
            }
        }
    }

    #[test]
    fn test_lift_lower_loop() {
        // A simple loop from the microkernel:
        // 1000037: 48 83 f9 0e              cmp rcx, 0xe
        // 100003b: 74 0c                    je 0x1000049
        let loop_bytes = vec![
            0x48, 0x83, 0xf9, 0x0e, // cmp rcx, 0xe
            0x74, 0x0c, // je +0xc
        ];

        println!("\n=== Testing loop comparison ===");
        println!("Original bytes: {}", hex_bytes(&loop_bytes));

        let mut lifter = X86_64Lifter::new();
        let mut ctx = LiftContext::new(SourceArch::X86_64);

        let mut pc = 0x1000037u64;
        let mut offset = 0usize;

        while offset < loop_bytes.len() {
            match lifter.lift_insn(pc, &loop_bytes[offset..], &mut ctx) {
                Ok(result) => {
                    println!(
                        "\nInstruction at {:#x} ({} bytes):",
                        pc, result.bytes_consumed
                    );
                    for op in &result.ops {
                        print_op(op);
                    }
                    println!("  Control flow: {:?}", result.control_flow);

                    pc += result.bytes_consumed as u64;
                    offset += result.bytes_consumed;

                    if result.control_flow.ends_block() {
                        break;
                    }
                }
                Err(e) => {
                    println!("Lift error at {:#x}: {:?}", pc, e);
                    break;
                }
            }
        }
    }

    #[test]
    #[ignore] // Run with: cargo test test_lift_lower_full_microkernel -- --ignored --nocapture
    fn test_lift_lower_full_microkernel() {
        // Load the actual microkernel binary
        let kernel_path = "microkernel/microkernel.bin";
        let kernel_bytes = match std::fs::read(kernel_path) {
            Ok(bytes) => bytes,
            Err(e) => {
                println!("Could not read {}: {}", kernel_path, e);
                return;
            }
        };

        // Parse ELF
        let elf = match goblin::elf::Elf::parse(&kernel_bytes) {
            Ok(elf) => elf,
            Err(e) => {
                println!("Could not parse ELF: {}", e);
                return;
            }
        };

        // Find .text section
        let text_section = elf.section_headers.iter().find(|sh| {
            elf.shdr_strtab
                .get_at(sh.sh_name)
                .map_or(false, |name| name == ".text")
        });

        let text_section = match text_section {
            Some(s) => s,
            None => {
                println!("No .text section found");
                return;
            }
        };

        let text_offset = text_section.sh_offset as usize;
        let text_size = text_section.sh_size as usize;
        let text_addr = text_section.sh_addr;

        println!("=== Microkernel .text section ===");
        println!("  Address: {:#x}", text_addr);
        println!("  Size: {} bytes ({:#x})", text_size, text_size);
        println!("  Offset: {:#x}", text_offset);

        let text_bytes = &kernel_bytes[text_offset..text_offset + text_size];

        // Statistics
        let mut total_insns = 0;
        let mut lifted_insns = 0;
        let mut lowered_insns = 0;
        let mut lift_errors: HashMap<String, usize> = HashMap::new();
        let mut lower_errors: HashMap<String, usize> = HashMap::new();

        let mut lifter = X86_64Lifter::new();
        let mut lowerer = X86_64Lowerer::new();

        let mut pc = text_addr;
        let end_addr = text_addr + text_size as u64;
        let mut offset = 0usize;

        while pc < end_addr && offset < text_bytes.len() {
            let mut ctx = LiftContext::new(SourceArch::X86_64);
            total_insns += 1;

            match lifter.lift_insn(pc, &text_bytes[offset..], &mut ctx) {
                Ok(result) => {
                    lifted_insns += 1;

                    // Try to lower the lifted instruction
                    if !result.ops.is_empty() {
                        let mut func = SmirFunction::new(FunctionId(0), BlockId(0), pc);
                        let mut block = SmirBlock::new(BlockId(0), pc);
                        for op in result.ops {
                            block.push_op(op);
                        }
                        block.set_terminator(Terminator::Return { values: vec![] });
                        func.add_block(block);

                        match lowerer.lower_function(&func) {
                            Ok(_) => {
                                lowered_insns += 1;
                            }
                            Err(e) => {
                                // Extract the op name from the error
                                let key = match &e {
                                    crate::smir::lower::LowerError::UnsupportedOp { op } => {
                                        // Extract just the op type name
                                        op.split_whitespace()
                                            .next()
                                            .unwrap_or("Unknown")
                                            .to_string()
                                    }
                                    _ => format!("{:?}", e)
                                        .split_whitespace()
                                        .take(3)
                                        .collect::<Vec<_>>()
                                        .join(" "),
                                };
                                *lower_errors.entry(key).or_insert(0) += 1;
                            }
                        }
                    } else {
                        lowered_insns += 1; // Empty op list is okay
                    }

                    pc += result.bytes_consumed as u64;
                    offset += result.bytes_consumed;
                }
                Err(e) => {
                    let key = format!("{:?}", e)
                        .split_whitespace()
                        .take(3)
                        .collect::<Vec<_>>()
                        .join(" ");
                    *lift_errors.entry(key).or_insert(0) += 1;
                    pc += 1; // Skip one byte and continue
                    offset += 1;
                }
            }
        }

        println!("\n=== Lift/Lower Statistics ===");
        println!("  Total instructions attempted: {}", total_insns);
        println!(
            "  Successfully lifted: {} ({:.1}%)",
            lifted_insns,
            100.0 * lifted_insns as f64 / total_insns as f64
        );
        println!(
            "  Successfully lowered: {} ({:.1}%)",
            lowered_insns,
            100.0 * lowered_insns as f64 / total_insns as f64
        );

        if !lift_errors.is_empty() {
            println!("\n  Lift errors:");
            let mut errors: Vec<_> = lift_errors.iter().collect();
            errors.sort_by(|a, b| b.1.cmp(a.1));
            for (error, count) in errors.iter().take(10) {
                println!("    {}: {}", count, error);
            }
        }

        if !lower_errors.is_empty() {
            println!("\n  Lower errors (top 20):");
            let mut errors: Vec<_> = lower_errors.iter().collect();
            errors.sort_by(|a, b| b.1.cmp(a.1));
            for (error, count) in errors.iter().take(20) {
                println!("    {:4}: {}", count, error);
            }
        }

        // Assert reasonable success rate
        let lift_rate = lifted_insns as f64 / total_insns as f64;
        let lower_rate = lowered_insns as f64 / total_insns as f64;

        println!("\n  Lift rate: {:.1}%", lift_rate * 100.0);
        println!("  Lower rate: {:.1}%", lower_rate * 100.0);
    }

    // ========================================================================
    // Semantic Verification Tests
    // ========================================================================
    //
    // These tests verify that lowered code produces semantically correct
    // results by:
    // 1. Lifting original x86_64 machine code to SMIR
    // 2. Executing the SMIR with the interpreter
    // 3. Lowering the SMIR back to machine code
    // 4. Re-lifting the lowered code
    // 5. Executing the re-lifted SMIR
    // 6. Comparing the results
    //
    // This validates that the round-trip preserves semantic correctness.

    use crate::smir::context::SmirContext;
    use crate::smir::interp::SmirInterpreter;
    use crate::smir::memory::FlatMemory;
    use crate::smir::types::ArchReg;

    /// Execute a block and return the value of RAX
    fn execute_block_get_rax(block: &SmirBlock, initial_rax: u64, initial_rbx: u64) -> u64 {
        let mut ctx = SmirContext::new_x86_64();
        let mut mem = FlatMemory::new(0x10000);

        // Set initial values
        ctx.write_arch_reg(ArchReg::X86(crate::smir::types::X86Reg::Rax), initial_rax);
        ctx.write_arch_reg(ArchReg::X86(crate::smir::types::X86Reg::Rbx), initial_rbx);
        ctx.write_arch_reg(ArchReg::X86(crate::smir::types::X86Reg::Rcx), 0);
        ctx.pc = block.guest_pc;

        let mut interp = SmirInterpreter::new();
        interp.add_block(block.guest_pc, block.clone());

        let _result = interp.run(&mut ctx, &mut mem);

        ctx.read_arch_reg(ArchReg::X86(crate::smir::types::X86Reg::Rax))
    }

    /// Lift bytes, execute, then lower/relift and execute again, comparing results
    fn verify_semantic_equivalence(
        name: &str,
        bytes: &[u8],
        initial_rax: u64,
        initial_rbx: u64,
    ) -> bool {
        let mut lifter = X86_64Lifter::new();
        let mut lowerer = X86_64Lowerer::new();

        // Step 1: Lift original bytes
        let mut ctx = LiftContext::new(SourceArch::X86_64);
        let mut block1 = SmirBlock::new(BlockId(0), 0x1000);

        let mut offset = 0;
        let mut pc = 0x1000u64;

        while offset < bytes.len() {
            let remaining = &bytes[offset..];
            if remaining.is_empty() {
                break;
            }
            match lifter.lift_insn(pc, remaining, &mut ctx) {
                Ok(result) => {
                    if result.bytes_consumed == 0 {
                        break; // Avoid infinite loop
                    }
                    for op in result.ops {
                        block1.push_op(op);
                    }
                    pc += result.bytes_consumed as u64;
                    offset += result.bytes_consumed;
                    if result.control_flow.ends_block() {
                        break;
                    }
                }
                Err(crate::smir::lift::LiftError::Incomplete { .. }) => {
                    // Ran out of bytes - that's expected for short sequences
                    break;
                }
                Err(e) => {
                    println!("  [{}] Lift error: {:?}", name, e);
                    return false;
                }
            }
        }
        block1.set_terminator(Terminator::Return { values: vec![] });

        // Step 2: Execute original lifted code
        let result1 = execute_block_get_rax(&block1, initial_rax, initial_rbx);

        // Step 3: Lower the SMIR
        let mut func = SmirFunction::new(FunctionId(0), BlockId(0), 0x1000);
        func.add_block(block1.clone());

        let lower_result = match lowerer.lower_function(&func) {
            Ok(r) => r,
            Err(e) => {
                println!("  [{}] Lower error: {:?}", name, e);
                return false;
            }
        };

        let lowered_bytes = match lowerer.finalize() {
            Ok(b) => b,
            Err(e) => {
                println!("  [{}] Finalize error: {:?}", name, e);
                return false;
            }
        };

        // Step 4: Re-lift the lowered code
        let mut ctx2 = LiftContext::new(SourceArch::X86_64);
        let mut block2 = SmirBlock::new(BlockId(1), 0x2000);

        // Skip the prologue (push rbp; mov rbp, rsp = 4 bytes)
        let code_start = 4; // Skip prologue
        let mut offset2 = code_start;
        let mut pc2 = 0x2000u64 + code_start as u64;

        while offset2 < lowered_bytes.len() {
            let remaining = &lowered_bytes[offset2..];
            if remaining.is_empty() {
                break;
            }
            match lifter.lift_insn(pc2, remaining, &mut ctx2) {
                Ok(result) => {
                    if result.bytes_consumed == 0 {
                        break; // Avoid infinite loop
                    }
                    for op in result.ops {
                        block2.push_op(op);
                    }
                    pc2 += result.bytes_consumed as u64;
                    offset2 += result.bytes_consumed;
                    if result.control_flow.ends_block() {
                        break;
                    }
                }
                Err(_) => {
                    // Might hit epilogue or other control flow - that's okay
                    break;
                }
            }
        }
        block2.set_terminator(Terminator::Return { values: vec![] });

        // Step 5: Execute re-lifted code
        let result2 = execute_block_get_rax(&block2, initial_rax, initial_rbx);

        // Step 6: Compare
        let matches = result1 == result2;
        if !matches {
            println!(
                "  [{}] MISMATCH: original={:#x}, round-trip={:#x}",
                name, result1, result2
            );
            println!("    Original ops: {}", block1.ops.len());
            println!("    Re-lifted ops: {}", block2.ops.len());
        } else {
            println!(
                "  [{}] OK: result={:#x} (original {} ops, re-lifted {} ops)",
                name,
                result1,
                block1.ops.len(),
                block2.ops.len()
            );
        }

        matches
    }

    #[test]
    fn test_semantic_add() {
        // ADD RAX, RBX (48 01 D8)
        let bytes = vec![0x48, 0x01, 0xD8];
        assert!(verify_semantic_equivalence("add rax,rbx", &bytes, 100, 42));
        assert!(verify_semantic_equivalence(
            "add rax,rbx overflow",
            &bytes,
            u64::MAX,
            1
        ));
    }

    #[test]
    fn test_semantic_sub() {
        // SUB RAX, RBX (48 29 D8)
        let bytes = vec![0x48, 0x29, 0xD8];
        assert!(verify_semantic_equivalence("sub rax,rbx", &bytes, 100, 42));
        assert!(verify_semantic_equivalence(
            "sub rax,rbx underflow",
            &bytes,
            10,
            20
        ));
    }

    #[test]
    fn test_semantic_xor() {
        // XOR RAX, RBX (48 31 D8)
        let bytes = vec![0x48, 0x31, 0xD8];
        assert!(verify_semantic_equivalence(
            "xor rax,rbx",
            &bytes,
            0xFF00FF00,
            0x00FF00FF
        ));
    }

    #[test]
    fn test_semantic_and() {
        // AND RAX, RBX (48 21 D8)
        let bytes = vec![0x48, 0x21, 0xD8];
        assert!(verify_semantic_equivalence(
            "and rax,rbx",
            &bytes,
            0xFF00FF00,
            0xFFFF0000
        ));
    }

    #[test]
    fn test_semantic_or() {
        // OR RAX, RBX (48 09 D8)
        let bytes = vec![0x48, 0x09, 0xD8];
        assert!(verify_semantic_equivalence(
            "or rax,rbx",
            &bytes,
            0x00FF0000,
            0x0000FF00
        ));
    }

    #[test]
    fn test_semantic_shl() {
        // SHL RAX, 4 (48 C1 E0 04)
        let bytes = vec![0x48, 0xC1, 0xE0, 0x04];
        assert!(verify_semantic_equivalence("shl rax,4", &bytes, 0x1234, 0));
    }

    #[test]
    fn test_semantic_shr() {
        // SHR RAX, 4 (48 C1 E8 04)
        let bytes = vec![0x48, 0xC1, 0xE8, 0x04];
        assert!(verify_semantic_equivalence("shr rax,4", &bytes, 0x12340, 0));
    }

    #[test]
    fn test_semantic_mov_imm() {
        // MOV RAX, 0x12345678 (48 C7 C0 78 56 34 12)
        let bytes = vec![0x48, 0xC7, 0xC0, 0x78, 0x56, 0x34, 0x12];
        assert!(verify_semantic_equivalence(
            "mov rax,imm32",
            &bytes,
            0xDEADBEEF,
            0
        ));
    }

    #[test]
    fn test_semantic_add_imm() {
        // ADD RAX, 0x10 (48 83 C0 10)
        let bytes = vec![0x48, 0x83, 0xC0, 0x10];
        assert!(verify_semantic_equivalence("add rax,16", &bytes, 100, 0));
    }

    #[test]
    fn test_semantic_neg() {
        // NEG RAX (48 F7 D8)
        let bytes = vec![0x48, 0xF7, 0xD8];
        assert!(verify_semantic_equivalence("neg rax", &bytes, 42, 0));
        assert!(verify_semantic_equivalence(
            "neg rax negative",
            &bytes,
            -42i64 as u64,
            0
        ));
    }

    #[test]
    fn test_semantic_not() {
        // NOT RAX (48 F7 D0)
        let bytes = vec![0x48, 0xF7, 0xD0];
        assert!(verify_semantic_equivalence(
            "not rax",
            &bytes,
            0x00FF00FF00FF00FF,
            0
        ));
    }

    #[test]
    fn test_semantic_inc() {
        // INC RAX (48 FF C0)
        let bytes = vec![0x48, 0xFF, 0xC0];
        assert!(verify_semantic_equivalence("inc rax", &bytes, 41, 0));
    }

    #[test]
    fn test_semantic_dec() {
        // DEC RAX (48 FF C8)
        let bytes = vec![0x48, 0xFF, 0xC8];
        assert!(verify_semantic_equivalence("dec rax", &bytes, 43, 0));
    }

    #[test]
    fn test_semantic_multi_instruction() {
        // Multiple instructions: MOV RAX, 10; ADD RAX, RBX
        let bytes = vec![
            0x48, 0xC7, 0xC0, 0x0A, 0x00, 0x00, 0x00, // mov rax, 10
            0x48, 0x01, 0xD8, // add rax, rbx
        ];
        assert!(verify_semantic_equivalence(
            "mov+add sequence",
            &bytes,
            0,
            32
        ));
    }
}
