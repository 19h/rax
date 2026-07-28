//! AMD XOP optimizer metadata and precise-frontier tests.

use super::*;
use crate::smir::ir::ops::X86XopPackedBitKind;

fn packed_bit(dst: VReg, src: VReg, count: SrcOperand) -> OpKind {
    OpKind::X86XopPackedBit {
        dst,
        src,
        count,
        elem: VecElementType::I32,
        kind: X86XopPackedBitKind::LogicalShift,
    }
}

#[test]
fn packed_xop_metadata_tracks_full_definition_and_both_count_shapes() {
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
    let src = VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)));
    let count = VReg::Arch(ArchReg::X86(X86Reg::Xmm(3)));

    let variable = packed_bit(dst, src, SrcOperand::Reg(count));
    assert_eq!(variable.dests(), vec![dst]);
    assert_eq!(variable.source_vregs(), vec![src, count]);
    assert_eq!(op_out_width(&variable), Some(OpWidth::W128));
    assert!(op_fully_defines(&variable));
    assert!(!variable.has_side_effects());
    assert_eq!(variable.flags_read(), FlagSet::EMPTY);
    assert_eq!(variable.flags_written(), FlagSet::EMPTY);

    let immediate = packed_bit(dst, src, SrcOperand::Imm(0x80));
    assert_eq!(immediate.dests(), vec![dst]);
    assert_eq!(immediate.source_vregs(), vec![src]);
    assert_eq!(op_out_width(&immediate), Some(OpWidth::W128));
    assert!(op_fully_defines(&immediate));
}

#[test]
fn xop_deopt_edge_preserves_pre_guard_vector_state_at_o1_and_o2() {
    let dst = VReg::Arch(ArchReg::X86(X86Reg::Xmm(1)));
    let first_src = VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)));
    let second_src = VReg::Arch(ArchReg::X86(X86Reg::Xmm(3)));

    for level in [OptLevel::O1, OptLevel::O2] {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.push_op(0x1000, packed_bit(dst, first_src, SrcOperand::Imm(1)));
        builder.push_op(0x1005, OpKind::X86RequireXop);
        builder.push_op(0x1009, packed_bit(dst, second_src, SrcOperand::Imm(2)));
        builder.set_terminator(Terminator::Return { values: vec![dst] });
        let mut function = builder.finish();

        optimize_function(&mut function, level);

        assert_eq!(
            function.blocks[0]
                .ops
                .iter()
                .filter(|op| matches!(op.kind, OpKind::X86XopPackedBit { .. }))
                .count(),
            2,
            "{level:?}: the disabled XOP guard observes the first vector result",
        );
        assert!(matches!(
            function.blocks[0].ops.get(1).map(|op| &op.kind),
            Some(OpKind::X86RequireXop)
        ));
    }
}
