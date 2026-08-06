//! Native x86-64 differential, helper ordering, and precise-fault coverage.

use super::semantics::{
    SemanticOutcome, SemanticState, initial_state, interpret_success, memory_bytes,
};
use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_K16, X86_VECTOR_STATE_K64};

#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[derive(Clone, Debug)]
struct LaneMemoryContext {
    base: u64,
    bytes: [u8; 64],
    lane_bytes: usize,
    fail_address: Option<u64>,
    calls: Vec<(u64, u64, Option<u64>)>,
    commits: Vec<(u64, u64, u64)>,
}

extern "C" fn lane_load_helper(
    context: *mut LaneMemoryContext,
    address: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    context.calls.push((address, size, None));
    if signed != 0 || context.fail_address == Some(address) {
        return LoadResult { value: 0, ok: 0 };
    }
    let offset = usize::try_from(address.wrapping_sub(context.base)).unwrap();
    let width = usize::try_from(size).unwrap();
    assert_eq!(width, context.lane_bytes);
    let mut raw = [0u8; 8];
    raw[..width].copy_from_slice(&context.bytes[offset..offset + width]);
    LoadResult {
        value: u64::from_le_bytes(raw),
        ok: 1,
    }
}

extern "C" fn lane_store_helper(
    context: *mut LaneMemoryContext,
    address: u64,
    value: u64,
    size: u64,
) -> u64 {
    let context = unsafe { &mut *context };
    context.calls.push((address, size, Some(value)));
    if context.fail_address == Some(address) {
        return 0;
    }
    let offset = usize::try_from(address.wrapping_sub(context.base)).unwrap();
    let width = usize::try_from(size).unwrap();
    assert_eq!(width, context.lane_bytes);
    context.bytes[offset..offset + width].copy_from_slice(&value.to_le_bytes()[..width]);
    context.commits.push((address, size, value));
    1
}

fn uses_k16(case: PackedMoveMemoryCase) -> bool {
    !case.spec.needs_avx512bw && case.lanes() <= 16
}

fn guest_regs(case: PackedMoveMemoryCase, initial: &SemanticState) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: initial.gpr,
        rflags: initial.rflags,
        exit_pc: 0xDEAD_BEEF_CAFE_BABE,
        k: initial.masks,
        vector_active: if uses_k16(case) {
            X86_VECTOR_STATE_K16
        } else {
            X86_VECTOR_STATE_K64
        },
        mxcsr: initial.mxcsr,
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        cr0: 1,
        cr4: 1 << 18,
        xcr0: 0b1110_0110,
        cs_l: 1,
        apx_enabled: 1,
        ..GuestRegs::default()
    };
    for (index, vector) in initial.vectors.iter().enumerate() {
        registers.set_zmm(index, vector[..8].try_into().unwrap());
    }
    registers
}

fn expected_guest_regs(initial: GuestRegs, outcome: &SemanticOutcome) -> GuestRegs {
    let mut expected = initial;
    expected.gpr = outcome.state.gpr;
    expected.rflags = outcome.state.rflags;
    expected.k = outcome.state.masks;
    expected.mxcsr = outcome.state.mxcsr;
    for (index, vector) in outcome.state.vectors.iter().enumerate() {
        expected.set_zmm(index, vector[..8].try_into().unwrap());
    }
    expected
}

fn bind_helpers(registers: &mut GuestRegs, context: &mut LaneMemoryContext) {
    registers.ctx = (context as *mut LaneMemoryContext) as u64;
    registers.load_fn = lane_load_helper as *const () as usize as u64;
    registers.store_fn = lane_store_helper as *const () as usize as u64;
}

fn active_addresses(case: PackedMoveMemoryCase, mask: u64, base: u64) -> Vec<u64> {
    (0..case.lanes())
        .filter(|lane| mask & (1u64 << lane) != 0)
        .map(|lane| base + (lane * case.lane_bytes()) as u64)
        .collect()
}

fn assert_calls(
    context: &LaneMemoryContext,
    case: PackedMoveMemoryCase,
    expected_addresses: &[u64],
) {
    assert_eq!(context.calls.len(), expected_addresses.len(), "{case:?}");
    for ((address, size, _), expected) in context.calls.iter().zip(expected_addresses) {
        assert_eq!(address, expected, "{case:?}");
        assert_eq!(*size, case.lane_bytes() as u64, "{case:?}");
    }
}

fn supported(case: PackedMoveMemoryCase) -> bool {
    std::is_x86_feature_detected!("avx512f")
        && (!case.spec.needs_avx512bw || std::is_x86_feature_detected!("avx512bw"))
        && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl"))
}

fn selected_cases() -> [PackedMoveMemoryCase; 12] {
    [
        PackedMoveMemoryCase {
            spec: SPECS[0],
            direction: Direction::Load,
            width: VecWidth::V512,
            vector: 17,
            base: 2,
            mask: 3,
            control: MaskControl::Merge,
        },
        PackedMoveMemoryCase {
            spec: SPECS[1],
            direction: Direction::Load,
            width: VecWidth::V256,
            vector: 25,
            base: 2,
            mask: 5,
            control: MaskControl::Zero,
        },
        PackedMoveMemoryCase {
            spec: SPECS[2],
            direction: Direction::Load,
            width: VecWidth::V128,
            vector: 9,
            base: 2,
            mask: 1,
            control: MaskControl::Merge,
        },
        PackedMoveMemoryCase {
            spec: SPECS[3],
            direction: Direction::Store,
            width: VecWidth::V512,
            vector: 17,
            base: 2,
            mask: 3,
            control: MaskControl::Merge,
        },
        PackedMoveMemoryCase {
            spec: SPECS[4],
            direction: Direction::Load,
            width: VecWidth::V256,
            vector: 25,
            base: 2,
            mask: 5,
            control: MaskControl::Zero,
        },
        PackedMoveMemoryCase {
            spec: SPECS[5],
            direction: Direction::Store,
            width: VecWidth::V512,
            vector: 17,
            base: 2,
            mask: 3,
            control: MaskControl::Merge,
        },
        PackedMoveMemoryCase {
            spec: SPECS[6],
            direction: Direction::Load,
            width: VecWidth::V512,
            vector: 9,
            base: 2,
            mask: 1,
            control: MaskControl::Merge,
        },
        PackedMoveMemoryCase {
            spec: SPECS[7],
            direction: Direction::Store,
            width: VecWidth::V256,
            vector: 25,
            base: 2,
            mask: 5,
            control: MaskControl::Merge,
        },
        PackedMoveMemoryCase {
            spec: SPECS[8],
            direction: Direction::Load,
            width: VecWidth::V128,
            vector: 17,
            base: 2,
            mask: 3,
            control: MaskControl::Zero,
        },
        PackedMoveMemoryCase {
            spec: SPECS[9],
            direction: Direction::Store,
            width: VecWidth::V512,
            vector: 25,
            base: 2,
            mask: 5,
            control: MaskControl::Merge,
        },
        PackedMoveMemoryCase {
            spec: SPECS[0],
            direction: Direction::Store,
            width: VecWidth::V128,
            vector: 9,
            base: 2,
            mask: 1,
            control: MaskControl::Merge,
        },
        PackedMoveMemoryCase {
            spec: SPECS[6],
            direction: Direction::Store,
            width: VecWidth::V128,
            vector: 17,
            base: 2,
            mask: 3,
            control: MaskControl::Merge,
        },
    ]
}

#[test]
fn native_packed_moves_match_interpreter_helper_order_faults_and_suppression() {
    if !std::is_x86_feature_detected!("avx512f") {
        eprintln!("skipping native packed-move memory differential: host lacks AVX-512F");
        return;
    }
    let cases: Vec<_> = selected_cases()
        .into_iter()
        .filter(|case| supported(*case))
        .collect();
    assert!(!cases.is_empty());

    let mut successes = 0usize;
    let mut faults = 0usize;
    let mut suppressions = 0usize;
    let mut alignment_faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        let full_mask = if case.lanes() == 64 {
            u64::MAX
        } else {
            (1u64 << case.lanes()) - 1
        };
        let mut mask = 0xD6A5_3C69_F00F_5AA5u64.rotate_left((ordinal & 63) as u32) & full_mask;
        if mask.count_ones() < 2 {
            mask |= 1 | (1u64 << (case.lanes() - 1));
        }
        let bytes = memory_bytes(ordinal);
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let executable =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let semantic = initial_state(case, ordinal, MEMORY_ADDRESS, mask);
            let expected_outcome = interpret_success(&function, &semantic, &bytes);

            let mut context = LaneMemoryContext {
                base: MEMORY_ADDRESS,
                bytes,
                lane_bytes: case.lane_bytes(),
                fail_address: None,
                calls: Vec::new(),
                commits: Vec::new(),
            };
            let mut registers = guest_regs(case, &semantic);
            bind_helpers(&mut registers, &mut context);
            let initial = registers;
            executable.run(entry, &mut registers);
            let mut expected = expected_guest_regs(initial, &expected_outcome);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success state");
            assert_eq!(context.bytes, expected_outcome.memory, "{level:?} {case:?}");
            let addresses = active_addresses(case, mask, MEMORY_ADDRESS);
            assert_calls(&context, case, &addresses);
            assert_eq!(
                context.commits.len(),
                if case.direction == Direction::Store {
                    addresses.len()
                } else {
                    0
                },
                "{level:?} {case:?}"
            );
            successes += 1;

            let fault_ordinal = addresses.len() / 2;
            let mut context = LaneMemoryContext {
                base: MEMORY_ADDRESS,
                bytes,
                lane_bytes: case.lane_bytes(),
                fail_address: Some(addresses[fault_ordinal]),
                calls: Vec::new(),
                commits: Vec::new(),
            };
            let mut registers = guest_regs(case, &semantic);
            bind_helpers(&mut registers, &mut context);
            let mut expected = registers;
            expected.exit_pc = PC;
            executable.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: fault state");
            assert_calls(&context, case, &addresses[..=fault_ordinal]);
            if case.direction == Direction::Load {
                assert_eq!(context.bytes, bytes, "{level:?} {case:?}");
                assert!(context.commits.is_empty());
            } else {
                let source = semantic.vectors[usize::from(case.vector)];
                let source = source[..8]
                    .iter()
                    .flat_map(|word| word.to_le_bytes())
                    .collect::<Vec<_>>();
                let mut expected_bytes = bytes;
                for address in &addresses[..fault_ordinal] {
                    let lane =
                        usize::try_from((address - MEMORY_ADDRESS) / case.lane_bytes() as u64)
                            .unwrap();
                    let range = lane * case.lane_bytes()..(lane + 1) * case.lane_bytes();
                    expected_bytes[range.clone()].copy_from_slice(&source[range]);
                }
                assert_eq!(context.bytes, expected_bytes, "{level:?} {case:?}");
                assert_eq!(context.commits.len(), fault_ordinal, "{level:?} {case:?}");
            }
            faults += 1;

            let suppressed_semantic = initial_state(case, ordinal ^ 0x55, MEMORY_ADDRESS, 0);
            let expected_outcome = interpret_success(&function, &suppressed_semantic, &bytes);
            let mut context = LaneMemoryContext {
                base: MEMORY_ADDRESS,
                bytes,
                lane_bytes: case.lane_bytes(),
                fail_address: Some(MEMORY_ADDRESS),
                calls: Vec::new(),
                commits: Vec::new(),
            };
            let mut registers = guest_regs(case, &suppressed_semantic);
            bind_helpers(&mut registers, &mut context);
            let initial = registers;
            executable.run(entry, &mut registers);
            let mut expected = expected_guest_regs(initial, &expected_outcome);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: suppression");
            assert_eq!(context.bytes, bytes, "{level:?} {case:?}");
            assert!(context.calls.is_empty(), "{level:?} {case:?}");
            assert!(context.commits.is_empty(), "{level:?} {case:?}");
            suppressions += 1;

            if case.spec.aligned {
                let misaligned = initial_state(case, ordinal ^ 0xAA, MEMORY_ADDRESS + 1, 0);
                let mut context = LaneMemoryContext {
                    base: MEMORY_ADDRESS + 1,
                    bytes,
                    lane_bytes: case.lane_bytes(),
                    fail_address: Some(MEMORY_ADDRESS + 1),
                    calls: Vec::new(),
                    commits: Vec::new(),
                };
                let mut registers = guest_regs(case, &misaligned);
                bind_helpers(&mut registers, &mut context);
                let mut expected = registers;
                expected.exit_pc = PC;
                executable.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected, "{level:?} {case:?}: alignment #GP");
                assert!(context.calls.is_empty(), "{level:?} {case:?}");
                assert_eq!(context.bytes, bytes, "{level:?} {case:?}");
                alignment_faults += 1;
            }
        }
    }
    assert_eq!(successes, faults);
    assert_eq!(successes, suppressions);
    assert!(successes >= 12);
    assert!(alignment_faults >= 4);
}
