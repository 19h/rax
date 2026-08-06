use super::*;

fn execute(
    function: &SmirFunction,
    initial: &GuestRegs,
    memory: &mut FlatMemory,
) -> (BlockResult, GuestRegs) {
    let mut context = interpreter_context(initial);
    let result = SmirInterpreter::new().execute_block(&mut context, memory, &function.blocks[0]);
    (result, interpreter_registers(&context, initial))
}

fn independent_success_oracle(
    initial: &GuestRegs,
    before: [u8; 24],
    case: IntegerCase,
) -> (GuestRegs, [u8; 24]) {
    let mut registers = *initial;
    let mut memory = before;
    let width = case.selector.memory_width().bytes() as usize;
    match case.selector.kind() {
        X86EvexScalarMoveMemoryKind::Load => {
            let mut scalar = [0u8; 8];
            scalar[..width].copy_from_slice(&before[8..8 + width]);
            registers.zmm[usize::from(case.vector)] = [0; 8];
            registers.zmm[usize::from(case.vector)][0] = u64::from_le_bytes(scalar);
        }
        X86EvexScalarMoveMemoryKind::Store => {
            let scalar = initial.zmm[usize::from(case.vector)][0].to_le_bytes();
            memory[8..8 + width].copy_from_slice(&scalar[..width]);
        }
    }
    (registers, memory)
}

#[test]
fn all_integer_selectors_have_o0_o1_o2_bit_exact_state_and_memory_semantics() {
    let vectors = [0u8, 8, 16, 31];
    let mut executions = 0usize;
    for (selector_ordinal, selector) in IntegerSelector::ALL.into_iter().enumerate() {
        for vector in vectors {
            let case = IntegerCase {
                selector,
                vector,
                base: 2,
            };
            let seed = selector_ordinal * vectors.len() + usize::from(vector);
            let initial = full_registers(case, seed);
            let before = std::array::from_fn(|index| {
                0xA5u8
                    ^ (index as u8).wrapping_mul(0x17)
                    ^ (seed as u8).rotate_left((index & 7) as u32)
            });
            let expected = independent_success_oracle(&initial, before, case);
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                let mut memory = FlatMemory::new(0x3000);
                memory.load(MEMORY_ADDRESS as usize - 8, &before);
                let (result, actual_registers) = execute(&function, &initial, &mut memory);
                assert!(
                    matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
                    "{level:?} {case:?}: {result:?}"
                );
                let mut actual_memory = [0u8; 24];
                memory.read(MEMORY_ADDRESS - 8, &mut actual_memory).unwrap();
                assert_eq!(actual_registers, expected.0, "{level:?} {case:?}");
                assert_eq!(actual_memory, expected.1, "{level:?} {case:?}");
                executions += 1;
            }
        }
    }
    assert_eq!(executions, 10 * 4 * LEVELS.len());
}

#[test]
fn integer_alias_pairs_are_semantically_identical_at_every_optimization_level() {
    let alias_pairs = [
        (IntegerSelector::QLoad6e, IntegerSelector::QLoad7e),
        (IntegerSelector::QStore7e, IntegerSelector::QStoreD6),
        (IntegerSelector::W0Load, IntegerSelector::W1Load),
        (IntegerSelector::W0Store, IntegerSelector::W1Store),
    ];
    let before: [u8; 24] = std::array::from_fn(|index| 0xE7u8 ^ (index as u8).wrapping_mul(0x31));
    let mut comparisons = 0usize;
    for (left_selector, right_selector) in alias_pairs {
        let left_case = IntegerCase {
            selector: left_selector,
            vector: 25,
            base: 2,
        };
        let right_case = IntegerCase {
            selector: right_selector,
            ..left_case
        };
        let initial = full_registers(left_case, comparisons ^ 0x55);
        for level in LEVELS {
            let mut left_memory = FlatMemory::new(0x3000);
            left_memory.load(MEMORY_ADDRESS as usize - 8, &before);
            let left = execute(
                &optimize(lift_case(left_case), level),
                &initial,
                &mut left_memory,
            );
            let mut right_memory = FlatMemory::new(0x3000);
            right_memory.load(MEMORY_ADDRESS as usize - 8, &before);
            let right = execute(
                &optimize(lift_case(right_case), level),
                &initial,
                &mut right_memory,
            );
            let mut left_bytes = [0; 24];
            let mut right_bytes = [0; 24];
            left_memory
                .read(MEMORY_ADDRESS - 8, &mut left_bytes)
                .unwrap();
            right_memory
                .read(MEMORY_ADDRESS - 8, &mut right_bytes)
                .unwrap();
            assert!(matches!(
                left.0,
                BlockResult::Exit(ExitReason::Return { .. })
            ));
            assert!(matches!(
                right.0,
                BlockResult::Exit(ExitReason::Return { .. })
            ));
            assert_eq!(
                left.1, right.1,
                "{level:?}: {left_case:?} vs {right_case:?}"
            );
            assert_eq!(
                left_bytes, right_bytes,
                "{level:?}: {left_case:?} vs {right_case:?}"
            );
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 4 * LEVELS.len());
}

#[test]
fn every_integer_width_faults_at_width_minus_one_without_architectural_commit() {
    let mut faults = 0usize;
    for (ordinal, selector) in IntegerSelector::ALL.into_iter().enumerate() {
        let case = IntegerCase {
            selector,
            vector: [0, 17, 31][ordinal % 3],
            base: 2,
        };
        let initial = full_registers(case, ordinal ^ 0xAA);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let size = MEMORY_ADDRESS as usize + selector.memory_width().bytes() as usize - 1;
            let mut boundary = FlatMemory::new(size);
            let (result, registers) = execute(&function, &initial, &mut boundary);
            assert!(matches!(
                result,
                BlockResult::Exit(ExitReason::MemoryFault { write, .. })
                    if write == (selector.kind() == X86EvexScalarMoveMemoryKind::Store)
            ));
            assert_eq!(registers, initial, "{level:?} {case:?}");
            faults += 1;
        }
    }
    assert_eq!(faults, 10 * LEVELS.len());
}

#[test]
fn apx_address_guard_precedes_integer_load_and_store_and_exits_before_memory() {
    let sentinel = [0xA5u8; 16];
    for selector in [
        IntegerSelector::DLoad,
        IntegerSelector::DStore,
        IntegerSelector::QStoreD6,
        IntegerSelector::W1Store,
    ] {
        let bytes = super::classification::apx_sib_bytes(selector);
        let case = IntegerCase {
            selector,
            vector: 17,
            base: 0,
        };
        for level in LEVELS {
            let function = optimize(lift_bytes(&bytes), level);
            assert_exact_graph(&function, case);
            assert!(matches!(
                function.blocks[0].ops[0].kind,
                OpKind::X86RequireApx
            ));

            let initial = full_registers(case, usize::from(selector.opcode()));
            let mut context = interpreter_context(&initial);
            let ArchRegState::X86_64(x86) = &mut context.arch_regs else {
                unreachable!()
            };
            x86.apx_enabled = false;
            let mut memory = FlatMemory::new(0x3000);
            memory.load(MEMORY_ADDRESS as usize, &sentinel);
            let result = SmirInterpreter::new().execute_block(
                &mut context,
                &mut memory,
                &function.blocks[0],
            );
            assert!(matches!(
                result,
                BlockResult::Exit(ExitReason::Undefined { addr: PC, .. })
            ));
            assert_eq!(interpreter_registers(&context, &initial), initial);
            let mut after = [0u8; 16];
            memory.read(MEMORY_ADDRESS, &mut after).unwrap();
            assert_eq!(after, sentinel, "{level:?} {case:?}");
        }
    }
}
