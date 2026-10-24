//! Alignment is a conditional fault barrier, not a speculative memory access.
use super::*;
use crate::error::MemoryAccessKind;
use crate::smir::ir::FunctionBuilder;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::optimize::{OptLevel, optimize_function};

fn guard(width: MemWidth, address: u64, access: MemoryAccessKind, cond: Option<VReg>) -> OpKind {
    OpKind::CheckMemoryAlignment {
        addr: Address::Absolute(address),
        width,
        access,
        cond,
    }
}

#[test]
fn natural_alignment_guard_covers_all_widths_and_access_intents_without_memory() {
    for width in [
        MemWidth::B1,
        MemWidth::B2,
        MemWidth::B4,
        MemWidth::B8,
        MemWidth::B16,
        MemWidth::B32,
        MemWidth::B64,
    ] {
        let alignment = u64::from(width.bytes());
        for address in [
            0,
            1,
            alignment - 1,
            alignment,
            alignment + 1,
            2 * alignment - 1,
        ] {
            for access in [
                MemoryAccessKind::Read,
                MemoryAccessKind::Write,
                MemoryAccessKind::Fetch,
            ] {
                let mut block = SmirBlock::new(BlockId(0), 0x1000);
                block.ops.push(SmirOp::new(
                    OpId(0),
                    0x1000,
                    guard(width, address, access, None),
                ));
                block.terminator = Terminator::Trap {
                    kind: TrapKind::Halt,
                };
                let result = SmirInterpreter::new().execute_block(
                    &mut SmirContext::new_hexagon(),
                    &mut FlatMemory::new(0),
                    &block,
                );
                if address % alignment == 0 {
                    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
                } else {
                    assert!(
                        matches!(result,BlockResult::Exit(ExitReason::AlignmentFault {pc:0x1000,address:a,alignment:n,access:k}) if a==address && n==width.bytes() && k==access)
                    );
                }
            }
        }
    }
}

#[test]
fn a_clear_predicate_bit_cancels_the_alignment_guard() {
    for value in [0, 2, 0xfe] {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.ops.push(SmirOp::new(
            OpId(0),
            0x1000,
            guard(
                MemWidth::B8,
                3,
                MemoryAccessKind::Write,
                Some(VReg::imm(value)),
            ),
        ));
        block.terminator = Terminator::Trap {
            kind: TrapKind::Halt,
        };
        let result = SmirInterpreter::new().execute_block(
            &mut SmirContext::new_hexagon(),
            &mut FlatMemory::new(0),
            &block,
        );
        assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));
    }
}

#[test]
fn an_alignment_fault_keeps_an_earlier_store_and_prevents_a_later_store_at_every_opt_level() {
    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x0ff8);
        builder.push_op(
            0x0ff8,
            OpKind::Store {
                src: VReg::imm(0x11),
                addr: Address::Absolute(0),
                width: MemWidth::B1,
            },
        );
        builder.push_op(
            0x1000,
            guard(MemWidth::B4, 3, MemoryAccessKind::Write, None),
        );
        builder.push_op(
            0x1004,
            OpKind::Store {
                src: VReg::imm(0x5a),
                addr: Address::Absolute(0),
                width: MemWidth::B1,
            },
        );
        builder.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let mut function = builder.finish();
        optimize_function(&mut function, level);
        let mut memory = FlatMemory::new(1);
        let result = SmirInterpreter::new().execute_block(
            &mut SmirContext::new_hexagon(),
            &mut memory,
            function.entry_block().unwrap(),
        );
        assert!(matches!(
            result,
            BlockResult::Exit(ExitReason::AlignmentFault {
                pc: 0x1000,
                address: 3,
                alignment: 4,
                access: MemoryAccessKind::Write
            })
        ));
        let mut data = [0];
        memory.read(0, &mut data).unwrap();
        assert_eq!(data, [0x11], "{level:?}");
    }
}

#[test]
fn alignment_guard_has_precise_effect_metadata() {
    let operation = guard(MemWidth::B4, 3, MemoryAccessKind::Read, Some(VReg::imm(1)));
    assert!(operation.has_side_effects());
    assert!(!operation.reads_memory());
    assert!(!operation.writes_memory());
    assert!(operation.dests().is_empty());
    assert!(!operation.is_jit_safe());
}

#[cfg(feature = "smir-jit")]
#[test]
fn alignment_guard_fails_native_admission() {
    let operation = guard(MemWidth::B4, 3, MemoryAccessKind::Read, Some(VReg::imm(1)));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, operation);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let function = builder.finish();
    assert!(!crate::smir::lower::runtime::is_native_clobber_safe(
        &function
    ));
    assert!(
        !crate::smir::lower::runtime::is_x86_aarch64_native_clobber_safe_excluding(
            &function,
            &HashMap::new()
        )
    );
    for allow_memory in [false, true] {
        assert!(
            !crate::smir::lower::runtime::is_aarch64_native_clobber_safe_excluding(
                &function,
                &HashMap::new(),
                allow_memory,
            )
        );
    }
}
