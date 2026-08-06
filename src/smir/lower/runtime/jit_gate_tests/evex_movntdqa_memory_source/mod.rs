//! Exact helper-backed EVEX `VMOVNTDQA` memory-source coverage.

use std::collections::HashMap;

use super::*;
use crate::smir::ir::ops::{OpKind, X86OpHint, X86VecAlign};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, SourceArch, VReg, VecWidth, X86Reg,
};
use crate::smir::ir::{
    SmirBlock, SmirFunction, Terminator, X86EvexMovntdqaMemoryEncoding, X86InstructionBytes,
};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{ControlFlow, LiftContext, SmirLifter};
use crate::smir::lower::runtime::{
    X86JitEvexMovntdqaMemorySequence, is_native_clobber_safe_excluding,
    is_x86_aarch64_native_clobber_safe_excluding, uses_x86_native_vectors_excluding,
    x86_jit_evex_movntdqa_memory_sequence, x86_native_replay_feature_requirements,
    x86_native_vector_uses_avx_ymm16_only_excluding, x86_native_vector_uses_k16_opmasks_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::lower::{LowerError, SmirLowerer};
use crate::smir::optimize::OptLevel;

mod classification;
#[cfg(target_arch = "x86_64")]
mod native;

const PC: u64 = 0x2A_E1_F0;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MovntdqaMemoryCase {
    destination: u8,
    width: VecWidth,
    base: u8,
}

impl MovntdqaMemoryCase {
    const fn ll(self) -> u8 {
        match self.width {
            VecWidth::V128 => 0,
            VecWidth::V256 => 1,
            VecWidth::V512 => 2,
            _ => unreachable!(),
        }
    }

    fn bytes(self) -> Vec<u8> {
        assert!(self.destination < 32 && self.base < 32);
        let mut bytes = vec![
            0x62,
            0x42 | (u8::from(self.destination & 8 == 0) << 7)
                | (u8::from(self.base & 8 == 0) << 5)
                | (u8::from(self.destination < 16) << 4)
                | (u8::from(self.base >= 16) << 3),
            0x7D,
            (self.ll() << 5) | 0x08,
            0x2A,
            0x40 | ((self.destination & 7) << 3) | (self.base & 7),
        ];
        if self.base & 7 == 4 {
            bytes.push(0x24);
        }
        bytes.push(1);
        bytes
    }

    const fn expected_encoding(self) -> X86EvexMovntdqaMemoryEncoding {
        X86EvexMovntdqaMemoryEncoding {
            destination: self.destination,
            width: self.width,
            needs_avx512vl: !matches!(self.width, VecWidth::V512),
        }
    }
}

fn x86(reg: X86Reg) -> VReg {
    VReg::Arch(ArchReg::X86(reg))
}

fn destination(case: MovntdqaMemoryCase) -> VReg {
    x86(match case.width {
        VecWidth::V128 => X86Reg::Xmm(case.destination),
        VecWidth::V256 => X86Reg::Ymm(case.destination),
        VecWidth::V512 => X86Reg::Zmm(case.destination),
        _ => unreachable!(),
    })
}

fn expected_address(case: MovntdqaMemoryCase) -> Address {
    Address::BaseOffset {
        base: x86(X86Reg::gpr(case.base)),
        offset: i64::from(case.width.bytes()),
        disp_size: DispSize::Disp8,
    }
}

fn function_from_bytes(bytes: &[u8], label: impl std::fmt::Debug) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{label:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{label:?} {bytes:02X?}");
    assert!(matches!(result.control_flow, ControlFlow::Fallthrough));

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("EVEX VMOVNTDQA provenance"),
    );
    function
}

fn lift_case(case: MovntdqaMemoryCase) -> SmirFunction {
    let function = function_from_bytes(&case.bytes(), case);
    assert_exact_lift_and_sequence(&function, case);
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn instruction_index(function: &SmirFunction) -> usize {
    usize::from(
        function.blocks[0]
            .ops
            .first()
            .is_some_and(|op| matches!(op.kind, OpKind::X86RequireApx)),
    )
}

fn virtual_counts(function: &SmirFunction) -> (HashMap<VReg, usize>, HashMap<VReg, usize>) {
    let mut definitions = HashMap::new();
    let mut uses = HashMap::new();
    for op in &function.blocks[0].ops {
        for register in op.kind.dests() {
            if matches!(register, VReg::Virtual(_)) {
                *definitions.entry(register).or_insert(0) += 1;
            }
        }
        for register in op.kind.source_vregs() {
            if matches!(register, VReg::Virtual(_)) {
                *uses.entry(register).or_insert(0) += 1;
            }
        }
    }
    (definitions, uses)
}

fn sequence(function: &SmirFunction, allow_mem: bool) -> Option<X86JitEvexMovntdqaMemorySequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_movntdqa_memory_sequence(
        &function.blocks[0],
        instruction_index(function),
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn assert_exact_lift_and_sequence(function: &SmirFunction, case: MovntdqaMemoryCase) {
    let index = instruction_index(function);
    assert_eq!(index, usize::from(case.base >= 16), "{case:?}");
    let [guard, load, write] = &function.blocks[0].ops[index..] else {
        panic!(
            "{case:?}: unexpected operations: {:#?}",
            function.blocks[0].ops
        )
    };
    assert!(guard.x86_hint.is_none(), "{case:?}");
    assert!(
        matches!(
            &guard.kind,
            OpKind::X86CheckAlignment { addr, alignment }
                if *addr == expected_address(case)
                    && u32::from(*alignment) == case.width.bytes()
        ),
        "{case:?}: {:?}",
        guard.kind
    );
    let temporary = match &load.kind {
        OpKind::VLoad {
            dst: temporary @ VReg::Virtual(_),
            addr,
            width,
        } if *addr == expected_address(case) && *width == case.width => *temporary,
        other => panic!("{case:?}: expected VLoad, got {other:?}"),
    };
    assert_eq!(
        load.x86_hint,
        Some(X86OpHint::VecAlign(X86VecAlign::Aligned)),
        "{case:?}"
    );
    assert!(
        matches!(
            write.kind,
            OpKind::VMov { dst, src, width }
                if dst == destination(case) && src == temporary && width == case.width
        ),
        "{case:?}: {:?}",
        write.kind
    );
    assert_eq!(
        sequence(function, true),
        Some(X86JitEvexMovntdqaMemorySequence {
            consumed: 3,
            encoding: case.expected_encoding(),
        }),
        "{case:?}"
    );
    assert_eq!(sequence(function, false), None, "{case:?}");
}

fn assert_feature_contract(function: &SmirFunction, case: MovntdqaMemoryCase) {
    let excluded = HashMap::new();
    assert!(is_native_clobber_safe_excluding(function, &excluded, true));
    assert!(!is_native_clobber_safe_excluding(
        function, &excluded, false
    ));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        function, &excluded
    ));
    assert!(uses_x86_native_vectors_excluding(function, &excluded));
    assert!(!x86_native_vector_uses_avx_ymm16_only_excluding(
        function, &excluded
    ));
    assert!(x86_native_vector_uses_k16_opmasks_excluding(
        function, &excluded
    ));

    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any);
    assert!(!requirements.all_spans_support_avx_ymm16);
    assert!(requirements.needs_avx);
    assert_eq!(requirements.needs_avx512vl, case.width != VecWidth::V512);
    assert!(requirements.has_k16_opmask_span);
    assert!(!requirements.needs_avx2);
    assert!(!requirements.needs_avx512bw);
    assert!(!requirements.needs_avx512dq);
    assert!(!requirements.needs_avx512fp16);
}

fn lower(function: &SmirFunction, case: MovntdqaMemoryCase) -> (Vec<u8>, usize) {
    assert_feature_contract(function, case);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_native_vector_state_active(true);
    lowerer.set_narrow_vector_opmask_helpers(true);
    lowerer.set_avx_ymm16_vector_state(false);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: EVEX VMOVNTDQA lowering failed: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    (
        lowerer.finalize().expect("finalize EVEX VMOVNTDQA"),
        result.entry_offset,
    )
}

fn all_cases() -> Vec<MovntdqaMemoryCase> {
    let mut cases = Vec::new();
    for destination in 0..32 {
        for width in [VecWidth::V128, VecWidth::V256, VecWidth::V512] {
            cases.push(MovntdqaMemoryCase {
                destination,
                width,
                base: 2,
            });
        }
    }
    cases
}
