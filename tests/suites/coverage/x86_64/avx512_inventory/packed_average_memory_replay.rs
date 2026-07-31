//! Generated-census coverage for helper-backed EVEX VPAVGB/W memory sources.

use super::*;

fn function(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(0x1000, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");

    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: vec![] });
    let mut function = SmirFunction::new(FunctionId(0), block.id, 0x1000);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), 0x1000),
        X86InstructionBytes::new(bytes).expect("architectural x86 instruction length"),
    );
    optimize_function(&mut function, OptLevel::O2);
    function
}

#[test]
fn memory_evex_packed_average_replay_closes_108_runtime_gate_gaps() {
    let mut lowered = 0usize;
    for opcode in [0xE0, 0xE3] {
        for w in [false, true] {
            for vvvv in [0u8, 1, 15] {
                for ll in 0u8..=2 {
                    for (mask, zeroing) in [(0u8, false), (1, false), (1, true)] {
                        let p0 = 0xF1;
                        let p1 = (u8::from(w) << 7) | (((!vvvv) & 0x0F) << 3) | 0x05;
                        let p2 = (u8::from(zeroing) << 7) | (ll << 5) | 0x08 | mask;
                        let bytes = [0x62, p0, p1, p2, opcode, 0x02];
                        let function = function(&bytes);

                        assert!(
                            is_native_clobber_safe_excluding(
                                &function,
                                &std::collections::HashMap::new(),
                                true,
                            ),
                            "{bytes:02X?}: optimized memory form was not admitted"
                        );

                        let mut lowerer = X86_64Lowerer::new();
                        lowerer.set_mem_helpers(true);
                        lowerer.set_preserve_vector_mem_helpers(true);
                        lowerer.set_jit_fault_deopt_guards(true);
                        lowerer
                            .lower_function(&function)
                            .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
                        let code = lowerer
                            .finalize()
                            .expect("finalize helper-backed packed average");

                        let expected = if mask == 0 {
                            let scratch = (0..16u8)
                                .find(|candidate| *candidate != 0 && *candidate != vvvv)
                                .expect("two operands leave a low vector scratch");
                            vec![
                                0x62,
                                (p0 & 0x97) | 0x40 | if scratch & 8 == 0 { 0x20 } else { 0 },
                                p1 | 0x04,
                                p2,
                                opcode,
                                0xC0 | (scratch & 7),
                            ]
                        } else {
                            vec![0x62, (p0 & 0x97) | 0x60, p1 | 0x04, p2, opcode, 0x04, 0x24]
                        };
                        assert!(
                            code.windows(expected.len())
                                .any(|window| window == expected),
                            "{bytes:02X?}: missing replay {expected:02X?}"
                        );
                        lowered += 1;
                    }
                }
            }
        }
    }
    assert_eq!(lowered, 108);
}
