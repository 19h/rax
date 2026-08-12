//! Interpreter and optimizer semantics for helper-backed VFIXUPIMM memory.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::lower::runtime::{GuestRegs, X86_VECTOR_STATE_K64};

fn set_lane(words: &mut [u64; 8], elem: VecElementType, lane: usize, value: u64) {
    match elem {
        VecElementType::F32 => {
            let word = lane / 2;
            let shift = (lane % 2) * 32;
            words[word] = (words[word] & !(u64::from(u32::MAX) << shift))
                | (u64::from(value as u32) << shift);
        }
        VecElementType::F64 => words[lane] = value,
        _ => unreachable!("VFIXUPIMM binary32/binary64 element"),
    }
}

fn source_bits(elem: VecElementType, lane: usize) -> u64 {
    const F32: [u32; 12] = [
        0x7FC0_1234,
        0x7F80_1234,
        0x0000_0000,
        0x8000_0000,
        0x3F80_0000,
        0xFF80_0000,
        0x7F80_0000,
        0xC020_0000,
        0x4020_0000,
        0x0000_0001,
        0x8000_0001,
        0xBF80_0000,
    ];
    const F64: [u64; 12] = [
        0x7FF8_0000_0000_1234,
        0x7FF0_0000_0000_1234,
        0x0000_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x3FF0_0000_0000_0000,
        0xFFF0_0000_0000_0000,
        0x7FF0_0000_0000_0000,
        0xC004_0000_0000_0000,
        0x4004_0000_0000_0000,
        0x0000_0000_0000_0001,
        0x8000_0000_0000_0001,
        0xBFF0_0000_0000_0000,
    ];
    match elem {
        VecElementType::F32 => u64::from(F32[lane % F32.len()]),
        VecElementType::F64 => F64[lane % F64.len()],
        _ => unreachable!("VFIXUPIMM binary32/binary64 element"),
    }
}

pub(super) fn initial_registers(case: FixupMemoryCase, ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0xA5A5_0000_0000_0000u64
                ^ ((ordinal as u64) << 12)
                ^ (index as u64 * 0x0101_0101_0101_0101)
        }),
        zmm: std::array::from_fn(|register| {
            std::array::from_fn(|word| {
                0x1111_0000_0000_0000u64 ^ ((register as u64) << 24) ^ (word as u64 * 0x0101_0101)
            })
        }),
        k: std::array::from_fn(|index| {
            if index == 3 {
                0xA5A5_A5A5
            } else if index == 1 {
                0x5555_5555
            } else {
                0xF0F0_0000 ^ index as u64
            }
        }),
        rflags: 0x8D5,
        mxcsr: 0x1F80 | (u32::from(ordinal & 1 != 0) << 6),
        vector_active: X86_VECTOR_STATE_K64,
        ..GuestRegs::default()
    };
    registers.gpr[3] = 0x2000;

    let lanes = if case.scalar() {
        VecWidth::V128.lanes(case.elem)
    } else {
        VecWidth::V512.lanes(case.elem)
    } as usize;
    for lane in 0..lanes {
        set_lane(
            &mut registers.zmm[usize::from(case.source1)],
            case.elem,
            lane,
            source_bits(case.elem, lane + ordinal),
        );
    }
    registers
}

pub(super) fn memory_value(case: FixupMemoryCase, ordinal: usize) -> [u64; 8] {
    let mut value = [0u64; 8];
    let lanes = VecWidth::V512.lanes(case.elem) as usize;
    for lane in 0..lanes {
        let table = if (lane + ordinal) & 1 == 0 {
            0x7654_3210u64
        } else {
            0xFEDC_BA98u64
        } | ((lane as u64) << 32);
        set_lane(&mut value, case.elem, lane, table);
    }
    value
}

pub(super) fn memory_bytes(value: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(value) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

pub(super) fn interpreter_success(
    function: &SmirFunction,
    initial: &GuestRegs,
    memory_value: [u64; 8],
    case: FixupMemoryCase,
) -> GuestRegs {
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = initial.gpr;
        for (index, value) in initial.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = initial.k;
        x86.rflags = initial.rflags;
        x86.mxcsr = initial.mxcsr;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    let mut memory = FlatMemory::new(0x10000);
    let bytes = memory_bytes(memory_value);
    let size = if case.scalar() || case.broadcast() {
        case.memory_width().bytes()
    } else {
        case.width.bytes()
    } as usize;
    memory.load(0x2000, &bytes[..size]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    let mut expected = *initial;
    expected.gpr = x86.gpr;
    for (index, value) in x86.xmm.iter().enumerate() {
        expected.zmm[index].copy_from_slice(&value[..8]);
    }
    expected.k = x86.k;
    expected.rflags = x86.rflags;
    expected.mxcsr = x86.mxcsr;
    expected
}

#[test]
fn fixup_memory_o0_o1_o2_interpretation_is_exactly_equivalent() {
    let cases = all_cases();
    let mut comparisons = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let initial = initial_registers(case, ordinal);
        assert_eq!(
            initial.vector_active, X86_VECTOR_STATE_K64,
            "native vector bridge mode for {case:?}"
        );
        let memory = memory_value(case, ordinal);
        let expected = interpreter_success(&lift_case(case), &initial, memory, case);
        for level in LEVELS {
            let function = optimize(lift_case(case), level);
            let actual = interpreter_success(&function, &initial, memory, case);
            assert_eq!(actual, expected, "{level:?} {case:?}");
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 180 * LEVELS.len());
}

#[test]
fn scalar_register_sae_suppresses_ie_ze_while_memory_dynamic_sets_both_sticky_flags() {
    let base = FixupMemoryCase {
        elem: VecElementType::F32,
        width: VecWidth::V128,
        source1: 1,
        form: SourceForm::Scalar { ll: 3 },
        control: MaskControl::None,
        // ZERO token: bit 0 requests ZE and bit 1 requests IE.
        immediate: 0x03,
    };
    let memory = memory_value(base, 0);
    let mut initial = initial_registers(base, 0);
    initial.mxcsr = 0x1F80;
    set_lane(&mut initial.zmm[usize::from(base.source1)], base.elem, 0, 0);
    let register_table = 2u8;
    initial.zmm[usize::from(register_table)] = memory;
    let dynamic = interpreter_success(&lift_case(base), &initial, memory, base);
    assert_eq!(dynamic.mxcsr & 0x3F, 0x05);

    let register_sae = lift_bytes(&register_encoding(
        base.elem,
        true,
        base.destination(),
        base.source1,
        register_table,
        base.ll(),
        base.mask(),
        base.zeroing(),
        true,
        base.immediate,
    ));
    let suppressed = interpreter_success(&register_sae, &initial, memory, base);
    assert_eq!(suppressed.mxcsr, initial.mxcsr);
    let destination = usize::from(base.destination());
    assert_eq!(suppressed.zmm[destination], dynamic.zmm[destination]);
}

fn execute_with_unmapped_source(case: FixupMemoryCase) -> (BlockResult, SmirContext) {
    let function = optimize(lift_case(case), OptLevel::O2);
    let mut registers = initial_registers(case, 0);
    registers.gpr[3] = 0x20_000;
    registers.k[usize::from(case.mask())] = 0;
    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        x86.gpr = registers.gpr;
        for (index, value) in registers.zmm.iter().enumerate() {
            x86.xmm[index][..8].copy_from_slice(value);
        }
        x86.k = registers.k;
        x86.rflags = registers.rflags;
        x86.mxcsr = registers.mxcsr;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(registers.rflags);
    let mut memory = FlatMemory::new(0x1000);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    (result, context)
}

#[test]
fn masked_fixup_empty_applicable_mask_suppresses_unmapped_scalar_broadcast_and_vector_sources() {
    for case in [
        FixupMemoryCase {
            elem: VecElementType::F32,
            width: VecWidth::V128,
            source1: 1,
            form: SourceForm::Scalar { ll: 3 },
            control: MaskControl::Merge,
            immediate: 0xFF,
        },
        FixupMemoryCase {
            elem: VecElementType::F64,
            width: VecWidth::V512,
            source1: 1,
            form: SourceForm::Broadcast,
            control: MaskControl::Zero,
            immediate: 0xFF,
        },
        FixupMemoryCase {
            elem: VecElementType::F32,
            width: VecWidth::V512,
            source1: 1,
            form: SourceForm::Vector,
            control: MaskControl::Merge,
            immediate: 0xFF,
        },
    ] {
        let (result, context) = execute_with_unmapped_source(case);
        assert!(
            matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
            "{case:?}: {result:?}"
        );
        let ArchRegState::X86_64(x86) = context.arch_regs else {
            unreachable!()
        };
        assert_eq!(x86.mxcsr, initial_registers(case, 0).mxcsr, "{case:?}");
    }
}
