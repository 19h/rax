//! misc part 2 tests

use super::*;
use crate::smir::optimize::tests::*;
use crate::smir::optimize::*;

#[test]
fn o2_preserves_repeated_observable_loads_without_explicit_proof() {
    use crate::smir::ir::FunctionBuilder;

    let base = VReg::Arch(ArchReg::Arm(ArmReg::X(0)));
    let dst1 = VReg::Arch(ArchReg::Arm(ArmReg::X(1)));
    let dst2 = VReg::Arch(ArchReg::Arm(ArmReg::X(2)));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for (pc, dst) in [(0x1000, dst1), (0x1004, dst2)] {
        builder.push_op(
            pc,
            OpKind::Load {
                dst,
                addr: Address::Direct(base),
                width: MemWidth::B4,
                sign: SignExtend::Zero,
            },
        );
    }
    builder.set_terminator(Terminator::Trap {
        kind: crate::smir::ir::TrapKind::Halt,
    });
    let mut func = builder.finish();

    let stats = optimize_function(&mut func, OptLevel::O2);
    assert_eq!(stats.redundant_loads_eliminated, 0);
    assert_eq!(
        func.blocks[0]
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::Load { .. }))
            .count(),
        2,
        "each faulting/MMIO-capable read remains observable",
    );
}
#[test]
fn proven_load_forwarding_keys_signedness_and_width() {
    use crate::smir::ir::FunctionBuilder;

    let base = VReg::Arch(ArchReg::Arm(ArmReg::X(0)));
    let zero1 = VReg::Arch(ArchReg::Arm(ArmReg::X(1)));
    let signed = VReg::Arch(ArchReg::Arm(ArmReg::X(2)));
    let zero2 = VReg::Arch(ArchReg::Arm(ArmReg::X(3)));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for (pc, dst, sign) in [
        (0x1000, zero1, SignExtend::Zero),
        (0x1004, signed, SignExtend::Sign),
        (0x1008, zero2, SignExtend::Zero),
    ] {
        builder.push_op(
            pc,
            OpKind::Load {
                dst,
                addr: Address::BaseOffset {
                    base,
                    offset: 4,
                    disp_size: crate::smir::ir::types::DispSize::Auto,
                },
                width: MemWidth::B4,
                sign,
            },
        );
    }
    builder.set_terminator(Terminator::Trap {
        kind: crate::smir::ir::TrapKind::Halt,
    });
    let mut func = builder.finish();
    func.attrs.allow_redundant_load_elimination = true;

    assert_eq!(redundant_load_elimination(&mut func), 1);
    assert!(matches!(
        func.blocks[0].ops[0].kind,
        OpKind::Load {
            sign: SignExtend::Zero,
            ..
        }
    ));
    assert!(matches!(
        func.blocks[0].ops[1].kind,
        OpKind::Load {
            sign: SignExtend::Sign,
            ..
        }
    ));
    assert!(matches!(
        func.blocks[0].ops[2].kind,
        OpKind::Mov {
            dst,
            src: SrcOperand::Reg(src),
            width: OpWidth::W32,
        } if dst == zero2 && src == zero1
    ));
}
#[test]
fn block_merging_migrates_x86_instruction_provenance() {
    use crate::smir::ir::{X86InstructionBytes, x86_evex_fp_replay_spans};

    let entry = BlockId(0);
    let successor = BlockId(1);
    let mut first = SmirBlock::new(entry, 0x1000);
    first.push_op(SmirOp::new(OpId(0), 0x1000, OpKind::Nop));
    first.set_terminator(Terminator::Branch { target: successor });
    let mut second = SmirBlock::new(successor, 0x1001);
    second.push_op(SmirOp::new(OpId(0), 0x1001, OpKind::Nop));
    second.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), entry, 0x1000);
    function.add_block(first);
    function.add_block(second);
    let instruction = X86InstructionBytes::new(&[0x62, 0xF1, 0x6C, 0x48, 0x58, 0xCB]).unwrap();
    function
        .x86_instruction_bytes
        .insert((successor, 0x1001), instruction);

    assert_eq!(block_merging(&mut function), 1);
    assert_eq!(function.blocks.len(), 1);
    assert_eq!(
        function.x86_instruction_bytes.get(&(entry, 0x1001)),
        Some(&instruction)
    );
    assert!(
        !function
            .x86_instruction_bytes
            .contains_key(&(successor, 0x1001))
    );
    let spans = x86_evex_fp_replay_spans(&function.blocks[0], &function.x86_instruction_bytes);
    assert_eq!(spans.get(&1).map(|span| span.end), Some(2));
}

#[test]
fn block_merging_contracts_an_entire_linear_chain_without_dangling_targets() {
    let entry = BlockId(0);
    let middle = BlockId(1);
    let tail = BlockId(2);
    let mut first = SmirBlock::new(entry, 0x1000);
    first.push_op(SmirOp::new(OpId(0), 0x1000, OpKind::Nop));
    first.set_terminator(Terminator::Branch { target: middle });
    let mut second = SmirBlock::new(middle, 0x1001);
    second.push_op(SmirOp::new(OpId(1), 0x1001, OpKind::Nop));
    second.set_terminator(Terminator::Branch { target: tail });
    let mut third = SmirBlock::new(tail, 0x1002);
    third.push_op(SmirOp::new(OpId(2), 0x1002, OpKind::Nop));
    third.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), entry, 0x1000);
    function.add_block(first);
    function.add_block(second);
    function.add_block(third);

    assert_eq!(block_merging(&mut function), 2);
    assert_eq!(function.blocks.len(), 1);
    assert_eq!(function.blocks[0].id, entry);
    assert_eq!(function.blocks[0].ops.len(), 3);
    assert!(matches!(
        function.blocks[0].terminator,
        Terminator::Return { ref values } if values.is_empty()
    ));
}
