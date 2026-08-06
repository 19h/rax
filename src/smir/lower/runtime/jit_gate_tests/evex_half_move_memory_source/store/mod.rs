//! Exact helper-backed EVEX.128 high/low 64-bit lane store coverage.

use super::*;
use crate::smir::ir::X86EvexHalfMoveStoreEncoding;
use crate::smir::lower::X86_GUEST_STORE_FN_OFFSET;
use crate::smir::lower::runtime::{
    X86JitEvexHalfMoveStoreSequence, x86_jit_evex_half_move_store_sequence,
};

mod classification;
#[cfg(target_arch = "x86_64")]
mod native;
mod semantics;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HalfMoveStoreCase {
    lane: MemoryLane,
    format: MoveFormat,
    source: u8,
}

impl HalfMoveStoreCase {
    const fn opcode(self) -> u8 {
        match self.lane {
            MemoryLane::Low => 0x13,
            MemoryLane::High => 0x17,
        }
    }

    fn bytes(self) -> [u8; 6] {
        assert!(self.source < 32);
        [
            0x62,
            (u8::from(self.source & 8 == 0) << 7)
                | 0x40
                | 0x20
                | (u8::from(self.source & 16 == 0) << 4)
                | 1,
            (u8::from(self.format.w()) << 7) | 0x7C | self.format.pp(),
            0x08,
            self.opcode(),
            ((self.source & 7) << 3) | 2,
        ]
    }

    fn stack_instruction(self) -> X86InstructionBytes {
        let bytes = self.bytes();
        X86InstructionBytes::new(&[
            0x62,
            (bytes[1] & 0x97) | 0x60,
            bytes[2] | 0x04,
            bytes[3],
            bytes[4],
            ((self.source & 7) << 3) | 4,
            0x24,
        ])
        .unwrap()
    }

    fn expected_encoding(self) -> X86EvexHalfMoveStoreEncoding {
        X86EvexHalfMoveStoreEncoding {
            source: self.source,
            memory_lane: self.lane.index(),
            w: self.format.w(),
            pp: self.format.pp(),
            opcode: self.opcode(),
            stack_instruction: self.stack_instruction(),
        }
    }
}

fn lift_store_case(case: HalfMoveStoreCase) -> SmirFunction {
    function_from_bytes(&case.bytes())
}

fn store_sequence(
    function: &SmirFunction,
    allow_mem: bool,
) -> Option<X86JitEvexHalfMoveStoreSequence> {
    let index = usize::from(
        function.blocks[0]
            .ops
            .first()
            .is_some_and(|op| matches!(op.kind, OpKind::X86RequireApx)),
    );
    let (definitions, uses) = virtual_counts(function);
    x86_jit_evex_half_move_store_sequence(
        &function.blocks[0],
        index,
        allow_mem,
        &function.x86_instruction_bytes,
        &definitions,
        &uses,
    )
}

fn assert_exact_store_graph(function: &SmirFunction, case: HalfMoveStoreCase) {
    let index = usize::from(matches!(
        function.blocks[0].ops.first().map(|op| &op.kind),
        Some(OpKind::X86RequireApx)
    ));
    let ops = &function.blocks[0].ops[index..];
    assert_eq!(ops.len(), 2, "{case:?}: {ops:#?}");
    let extracted = match ops[0].kind {
        OpKind::VExtractLane {
            dst,
            vec,
            lane,
            elem: VecElementType::I64,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(vec, xmm(case.source), "{case:?}");
            assert_eq!(lane, case.lane.index(), "{case:?}");
            dst
        }
        ref other => panic!("{case:?}: source extraction: {other:?}"),
    };
    assert!(matches!(
        &ops[1].kind,
        OpKind::Store {
            src,
            width: MemWidth::B8,
            ..
        } if *src == extracted
    ));
    assert!(
        ops.iter()
            .all(|op| op.guest_pc == PC && op.x86_hint.is_none())
    );
    assert_eq!(
        store_sequence(function, true),
        Some(X86JitEvexHalfMoveStoreSequence {
            consumed: 2,
            address_offset: 1,
            encoding: case.expected_encoding(),
        }),
        "{case:?}"
    );
    assert_eq!(store_sequence(function, false), None, "{case:?}");
}

fn lower_store(function: &SmirFunction, case: HalfMoveStoreCase) -> (Vec<u8>, usize) {
    assert_exact_store_graph(function, case);
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
    assert!(!x86_native_vector_uses_k16_opmasks_excluding(
        function, &excluded
    ));
    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any && requirements.needs_avx, "{case:?}");
    assert!(requirements.needs_avx512bw, "{case:?}");
    assert!(!requirements.needs_avx512vl, "{case:?}");
    assert!(!requirements.needs_avx512dq, "{case:?}");
    assert!(!requirements.has_k16_opmask_span, "{case:?}");
    assert!(!requirements.all_spans_support_avx_ymm16, "{case:?}");

    let mut lowerer = configured_lowerer(false);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("{case:?}: EVEX half-move store lowering: {error:?}"));
    assert!(result.relocations.is_empty(), "{case:?}");
    let code = lowerer
        .finalize()
        .expect("finalize EVEX half-move store replay");
    let stack = case.stack_instruction();
    assert!(
        code.windows(stack.as_slice().len())
            .any(|window| window == stack.as_slice()),
        "{case:?}: missing stack replay {:02X?}",
        stack.as_slice()
    );
    assert!(
        code.windows(4)
            .any(|window| window == (X86_GUEST_STORE_FN_OFFSET as u32).to_le_bytes()),
        "{case:?}: scalar-store helper offset absent"
    );
    (code, result.entry_offset)
}

fn representative_store_cases() -> Vec<HalfMoveStoreCase> {
    let mut cases = Vec::with_capacity(24);
    for lane in MemoryLane::ALL {
        for format in MoveFormat::ALL {
            for source in [0, 1, 15, 16, 17, 31] {
                cases.push(HalfMoveStoreCase {
                    lane,
                    format,
                    source,
                });
            }
        }
    }
    cases
}

#[test]
fn all_128_source_format_lane_cells_admit_and_lower_at_every_optimizer_level() {
    let mut lowerings = 0usize;
    for lane in MemoryLane::ALL {
        for format in MoveFormat::ALL {
            for source in 0..32u8 {
                let case = HalfMoveStoreCase {
                    lane,
                    format,
                    source,
                };
                assert_eq!(
                    X86InstructionBytes::new(&case.bytes())
                        .unwrap()
                        .evex_half_move_store_encoding(),
                    Some(case.expected_encoding()),
                    "{case:?}"
                );
                for level in LEVELS {
                    lower_store(&optimize(lift_store_case(case), level), case);
                    lowerings += 1;
                }
            }
        }
    }
    assert_eq!(lowerings, 2 * 2 * 32 * LEVELS.len());
}

#[test]
fn store_full_vector_bridge_rejects_avx_only_state_marshalling() {
    let case = representative_store_cases()[0];
    let function = optimize(lift_store_case(case), OptLevel::O2);
    assert!(store_sequence(&function, true).is_some());
    let mut lowerer = configured_lowerer(true);
    assert!(matches!(
        lowerer.lower_function(&function),
        Err(LowerError::InvalidOperand { .. })
    ));
}
