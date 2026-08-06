//! Canonical x86 emission tests for fused LAHF/SAHF graphs.

use super::*;
use crate::smir::SourceArch;
use crate::smir::ir::SmirBlock;
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};

fn lift_block(opcode: u8) -> SmirBlock {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(0x1000, &[opcode], &mut context)
        .expect("lift AH/flags instruction");
    assert_eq!(result.bytes_consumed, 1);
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block
}

fn virtual_counts(
    block: &SmirBlock,
) -> (
    std::collections::HashMap<VReg, usize>,
    std::collections::HashMap<VReg, usize>,
) {
    let mut definitions = std::collections::HashMap::new();
    let mut uses = std::collections::HashMap::new();
    for op in &block.ops {
        for reg in op.kind.dests() {
            if matches!(reg, VReg::Virtual(_)) {
                *definitions.entry(reg).or_insert(0) += 1;
            }
        }
        for reg in op.kind.source_vregs() {
            if matches!(reg, VReg::Virtual(_)) {
                *uses.entry(reg).or_insert(0) += 1;
            }
        }
    }
    (definitions, uses)
}

#[test]
fn exact_ah_flag_graphs_emit_only_the_canonical_instruction() {
    for opcode in [0x9E, 0x9F] {
        let block = lift_block(opcode);
        let (definitions, uses) = virtual_counts(&block);
        let mut lowerer = X86_64Lowerer::new();
        assert_eq!(
            lowerer
                .try_lower_jit_ah_flags(&block, 0, &definitions, &uses)
                .expect("lower AH/flags graph"),
            Some(6)
        );
        assert_eq!(lowerer.code.data(), &[opcode]);
    }
}
