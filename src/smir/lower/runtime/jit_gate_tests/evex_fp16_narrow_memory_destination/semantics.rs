//! Native success/fault differential coverage for EVEX `VCVTPS2PH` stores.

use super::*;

#[cfg(target_arch = "x86_64")]
use crate::smir::interpret::{BlockResult, SmirInterpreter};
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::flags::MaterializedFlags;
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
#[cfg(target_arch = "x86_64")]
use crate::smir::lower::runtime::{
    ExecMem, GuestRegs, X86_VECTOR_STATE_K16, x86_native_vector_features_supported_excluding,
};

#[cfg(target_arch = "x86_64")]
const F32_PATTERNS: [u32; 16] = [
    0x0000_0000, // +0
    0x8000_0000, // -0
    0x3F80_0000, // +1
    0xC000_0000, // -2
    0x3880_0000, // minimum normal binary16
    0x3380_0000, // minimum subnormal binary16
    0x3300_0000, // half minimum subnormal binary16
    0x477F_E000, // maximum finite binary16
    0x477F_F000, // midpoint to +infinity under round-to-nearest
    0x7F80_0000, // +infinity
    0xFF80_0000, // -infinity
    0x7FC1_2345, // positive quiet NaN with payload
    0xFFC1_2345, // negative quiet NaN with payload
    0x7F81_2345, // signaling NaN with payload
    0x0000_0001, // positive binary32 subnormal
    0x8000_0001, // negative binary32 subnormal
];

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct StoreMemoryContext {
    ok: u64,
    calls: u64,
    successful_calls: u64,
    active_writes: u64,
    expected_scratch: u32,
    expected_tag: u32,
    expected_lanes: u32,
    last_addr: u64,
    last_tag: u32,
    last_size: u32,
    observed_mxcsr: u32,
    observed_payload: [u8; 32],
    committed: [u8; 32],
    observed_scratch_register: [u64; 8],
}

#[cfg(target_arch = "x86_64")]
extern "C" fn vector_store_helper(state: *mut GuestRegs, addr: u64, tag: u32, size: u32) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut StoreMemoryContext) };
    context.calls += 1;
    context.last_addr = addr;
    context.last_tag = tag;
    context.last_size = size;
    context.observed_mxcsr = state.mxcsr;
    context.observed_scratch_register = state.zmm[context.expected_scratch as usize];
    for (destination, word) in context
        .observed_payload
        .chunks_mut(8)
        .zip(state.vector_scratch)
    {
        destination.copy_from_slice(&word.to_le_bytes()[..destination.len()]);
    }
    if context.ok == 0
        || tag != context.expected_tag
        || size != context.expected_lanes * 2
        || !matches!(size, 8 | 16 | 32)
    {
        return 0;
    }

    let mask_index = tag - X86_JIT_VECTOR_MASKED_WORD_SCRATCH_BASE;
    let active_mask = if mask_index == 0 {
        (1u64 << context.expected_lanes) - 1
    } else {
        state.k[mask_index as usize] & ((1u64 << context.expected_lanes) - 1)
    };
    for lane in 0..context.expected_lanes {
        if active_mask & (1u64 << lane) == 0 {
            continue;
        }
        let offset = lane as usize * 2;
        context.committed[offset..offset + 2]
            .copy_from_slice(&context.observed_payload[offset..offset + 2]);
        context.active_writes += 1;
    }
    context.successful_calls += 1;
    1
}

#[cfg(target_arch = "x86_64")]
fn patterned_vector(register: usize, seed: usize) -> [u64; 8] {
    std::array::from_fn(|word| {
        0x0123_4567_89AB_CDEFu64.rotate_left((register * 11 + word * 7) as u32)
            ^ (register as u64).wrapping_mul(0x8040_2010_0804_0201)
            ^ (seed as u64).wrapping_mul(0x1020_4081_0204_0810)
            ^ (word as u64).wrapping_mul(0x1111_2222_4444_8888)
    })
}

#[cfg(target_arch = "x86_64")]
fn mask_pattern(lanes: u8, seed: usize) -> u64 {
    let full = (1u64 << lanes) - 1;
    match seed & 3 {
        0 => 0,
        1 => 1 | (1u64 << (lanes - 1)),
        2 => 0xAAAA_AAAA_AAAA_AAAA & full,
        _ => full,
    }
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: NarrowCase, seed: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1800u64
                .wrapping_add((index as u64) * 0x111)
                .wrapping_add((seed as u64) * 0x20)
        }),
        rflags: 0x2 | (((seed as u64).wrapping_mul(0x195)) & 0x8D5),
        exit_pc: 0xDEAD_BEEF_CAFE_BABE,
        k: std::array::from_fn(|index| 0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 9) as u32)),
        vector_active: X86_VECTOR_STATE_K16,
        // All six SIMD exception masks stay set. Vary status, RC, DAZ, and FTZ.
        mxcsr: 0x1F80
            | ((seed as u32).rotate_left(3) & 0x3F)
            | (((seed as u32 >> 1) & 3) << 13)
            | (u32::from(seed & 1 != 0) << 6)
            | (u32::from(seed & 2 != 0) << 15),
        vector_scratch: std::array::from_fn(|index| {
            0xCCDD_EEFF_0011_2233u64 ^ (index as u64).wrapping_mul(0x1111_1111_1111_1111)
        }),
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = patterned_vector(index, seed);
    }
    if let Some(mask) = case.writemask {
        registers.k[usize::from(mask)] = mask_pattern(case.lanes(), seed);
    }

    let mut source_bytes = [0u8; 64];
    for (chunk, word) in source_bytes
        .chunks_mut(8)
        .zip(registers.zmm[usize::from(case.source)])
    {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    for lane in 0..usize::from(case.lanes()) {
        let bits = F32_PATTERNS[(lane + seed * 5) % F32_PATTERNS.len()];
        source_bytes[lane * 4..lane * 4 + 4].copy_from_slice(&bits.to_le_bytes());
    }
    registers.zmm[usize::from(case.source)] = std::array::from_fn(|word| {
        u64::from_le_bytes(source_bytes[word * 8..word * 8 + 8].try_into().unwrap())
    });
    registers.gpr[usize::from(case.base)] = 0x3000 + ((seed & 0x1F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn effective_address(registers: &GuestRegs, case: NarrowCase) -> u64 {
    registers.gpr[usize::from(case.base)].wrapping_add(u64::from(case.memory_size()))
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug, PartialEq, Eq)]
struct Interpreted {
    payload: Vec<u8>,
    mxcsr: u32,
}

#[cfg(target_arch = "x86_64")]
fn interpret(
    function: &SmirFunction,
    initial: &GuestRegs,
    address: u64,
    case: NarrowCase,
) -> Interpreted {
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

    let width = case.memory_size() as usize;
    let mut memory = FlatMemory::new(0x10000);
    memory.load(address as usize - 8, &[0xA5; 48]);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
        "{case:?}: {result:?}"
    );

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr, initial.gpr, "{case:?}: interpreter GPRs");
    assert_eq!(x86.k, initial.k, "{case:?}: interpreter masks");
    assert_eq!(x86.rflags, initial.rflags, "{case:?}: interpreter RFLAGS");
    for (index, value) in initial.zmm.iter().enumerate() {
        assert_eq!(&x86.xmm[index][..8], value, "{case:?}: ZMM{index}");
    }

    let mut payload = vec![0u8; width];
    memory.read(address, &mut payload).unwrap();
    Interpreted {
        payload,
        mxcsr: x86.mxcsr,
    }
}

#[cfg(target_arch = "x86_64")]
fn assert_architectural_state(
    actual: &GuestRegs,
    initial: &GuestRegs,
    expected_mxcsr: u32,
    expected_exit_pc: u64,
    case: NarrowCase,
    label: &str,
) {
    assert_eq!(actual.gpr, initial.gpr, "{label} {case:?}: GPRs");
    assert_eq!(actual.zmm, initial.zmm, "{label} {case:?}: vectors");
    assert_eq!(actual.k, initial.k, "{label} {case:?}: masks");
    assert_eq!(actual.rflags, initial.rflags, "{label} {case:?}: RFLAGS");
    assert_eq!(actual.mxcsr, expected_mxcsr, "{label} {case:?}: MXCSR");
    assert_eq!(actual.exit_pc, expected_exit_pc, "{label} {case:?}: PC");
}

#[cfg(target_arch = "x86_64")]
fn assert_helper_observation(
    context: &StoreMemoryContext,
    initial: &GuestRegs,
    expected: &Interpreted,
    address: u64,
    successful_calls: u64,
    case: NarrowCase,
    label: &str,
) {
    assert_eq!(context.calls, 1, "{label} {case:?}: helper calls");
    assert_eq!(context.last_addr, address, "{label} {case:?}: address");
    assert_eq!(
        context.last_tag, context.expected_tag,
        "{label} {case:?}: tag"
    );
    assert_eq!(
        context.last_size,
        case.memory_size(),
        "{label} {case:?}: width"
    );
    assert_eq!(
        context.observed_mxcsr, initial.mxcsr,
        "{label} {case:?}: helper must observe pre-conversion MXCSR"
    );
    assert_eq!(
        context.observed_scratch_register,
        initial.zmm[usize::from(case.scratch())],
        "{label} {case:?}: borrowed vector restored"
    );
    let mask = case.writemask.map_or((1u64 << case.lanes()) - 1, |mask| {
        initial.k[usize::from(mask)] & ((1u64 << case.lanes()) - 1)
    });
    for lane in 0..case.lanes() {
        if mask & (1u64 << lane) == 0 {
            continue;
        }
        let offset = usize::from(lane) * 2;
        assert_eq!(
            &context.observed_payload[offset..offset + 2],
            &expected.payload[offset..offset + 2],
            "{label} {case:?}: active lane {lane}"
        );
    }
    assert_eq!(
        context.successful_calls, successful_calls,
        "{label} {case:?}: successful calls"
    );
    if successful_calls != 0 {
        assert_eq!(
            &context.committed[..expected.payload.len()],
            expected.payload,
            "{label} {case:?}: committed memory"
        );
        assert_eq!(context.active_writes, u64::from(mask.count_ones()));
    } else {
        assert_eq!(context.committed, [0xA5; 32]);
        assert_eq!(context.active_writes, 0);
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: OptLevel,
    instruction: NarrowCase,
    seed: usize,
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for level in [OptLevel::O0, OptLevel::O2] {
        for ll in 0..=2 {
            for writemask in [None, Some([3, 5, 7][usize::from(ll)])] {
                for source in 0..32u8 {
                    cases.push(NativeCase {
                        level,
                        instruction: NarrowCase {
                            ll,
                            source,
                            base: 2,
                            immediate: (ordinal & 7) as u8,
                            writemask,
                        },
                        seed: ordinal,
                    });
                    ordinal += 1;
                }
            }
        }
    }
    assert_eq!(cases.len(), 2 * 3 * 2 * 32);
    assert!(cases.iter().any(|case| case.instruction.source >= 16));
    assert!(
        cases
            .iter()
            .any(|case| case.instruction.writemask.is_some())
    );
    cases
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_EVEX_FP16_NARROW_MEMORY_CHILD_RANGE";

#[cfg(target_arch = "x86_64")]
fn child_range() -> Option<std::ops::Range<usize>> {
    let value = std::env::var(CHILD_RANGE_ENV).ok()?;
    let (start, end) = value
        .split_once(':')
        .unwrap_or_else(|| panic!("invalid {CHILD_RANGE_ENV}: {value}"));
    Some(
        start
            .parse::<usize>()
            .unwrap_or_else(|_| panic!("invalid {CHILD_RANGE_ENV} start: {value}"))
            ..end
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("invalid {CHILD_RANGE_ENV} end: {value}")),
    )
}

#[cfg(target_arch = "x86_64")]
fn execute_native_case_range(cases: &[NativeCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    let executions = range.len();
    let mut successes = 0usize;
    let mut faults = 0usize;

    for native_case in &cases[range] {
        let case = native_case.instruction;
        let function = optimize(lift_case(case), native_case.level);
        let (code, entry) = lower(&function, case);
        let exec = ExecMem::new(&code)
            .unwrap_or_else(|error| panic!("{:?} {case:?}: {error:?}", native_case.level));
        let expected_tag =
            X86_JIT_VECTOR_MASKED_WORD_SCRATCH_BASE + u32::from(case.writemask.unwrap_or(0));

        let mut context = StoreMemoryContext {
            ok: 1,
            calls: 0,
            successful_calls: 0,
            active_writes: 0,
            expected_scratch: u32::from(case.scratch()),
            expected_tag,
            expected_lanes: u32::from(case.lanes()),
            last_addr: 0,
            last_tag: 0,
            last_size: 0,
            observed_mxcsr: 0,
            observed_payload: [0; 32],
            committed: [0xA5; 32],
            observed_scratch_register: [0; 8],
        };
        let mut registers = full_guest_regs(case, native_case.seed);
        let address = effective_address(&registers, case);
        registers.ctx = (&mut context as *mut StoreMemoryContext) as u64;
        registers.vec_store_fn = vector_store_helper as usize as u64;
        let initial = registers;
        let expected = interpret(&function, &initial, address, case);

        exec.run(entry, &mut registers);
        assert_architectural_state(
            &registers,
            &initial,
            expected.mxcsr,
            initial.exit_pc,
            case,
            "success",
        );
        assert_helper_observation(&context, &initial, &expected, address, 1, case, "success");
        successes += 1;

        let mut context = StoreMemoryContext {
            ok: 0,
            calls: 0,
            successful_calls: 0,
            active_writes: 0,
            expected_scratch: u32::from(case.scratch()),
            expected_tag,
            expected_lanes: u32::from(case.lanes()),
            last_addr: 0,
            last_tag: 0,
            last_size: 0,
            observed_mxcsr: 0,
            observed_payload: [0; 32],
            committed: [0xA5; 32],
            observed_scratch_register: [0; 8],
        };
        let mut registers = full_guest_regs(case, native_case.seed ^ 0x55);
        let address = effective_address(&registers, case);
        registers.ctx = (&mut context as *mut StoreMemoryContext) as u64;
        registers.vec_store_fn = vector_store_helper as usize as u64;
        let initial = registers;
        let expected = interpret(&function, &initial, address, case);

        exec.run(entry, &mut registers);
        assert_architectural_state(&registers, &initial, initial.mxcsr, PC, case, "fault");
        assert_helper_observation(&context, &initial, &expected, address, 0, case, "fault");
        faults += 1;
    }

    assert_eq!(successes, executions);
    assert_eq!(faults, executions);
    eprintln!(
        "executed {successes} successful and {faults} faulting native EVEX VCVTPS2PH memory cases"
    );
}

#[cfg(target_arch = "x86_64")]
fn run_child_range(test_name: &str, range: std::ops::Range<usize>) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("current unit-test executable"))
        .arg(test_name)
        .arg("--exact")
        .arg("--nocapture")
        .env(CHILD_RANGE_ENV, format!("{}:{}", range.start, range.end))
        .output()
        .expect("run isolated native EVEX VCVTPS2PH differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    if let Some(range) = child_range() {
        execute_native_case_range(&cases, range);
        return;
    }

    // The matrix includes 128-/256-bit forms, so its common prerequisite is
    // AVX512VL in addition to the full-width AVX512F bridge and semantic guard.
    let probe = optimize(lift_case(cases[0].instruction), OptLevel::O0);
    if !x86_native_vector_features_supported_excluding(&probe, &HashMap::new()) {
        eprintln!(
            "skipping native EVEX VCVTPS2PH memory differential: production AVX512F/VL/host-semantic gate rejected the host"
        );
        return;
    }

    let whole = run_child_range(test_name, 0..cases.len());
    if whole.status.success() {
        eprint!("{}", String::from_utf8_lossy(&whole.stderr));
        return;
    }

    let mut start = 0usize;
    let mut end = cases.len();
    while end - start > 1 {
        let middle = start + (end - start) / 2;
        if run_child_range(test_name, start..middle).status.success() {
            start = middle;
        } else {
            end = middle;
        }
    }
    let singleton = run_child_range(test_name, start..end);
    let case = cases[start];
    let bytes = case.instruction.bytes();
    panic!(
        "isolated native EVEX VCVTPS2PH memory failure at case {start}/{}: \
         {case:?} {bytes:02X?}; whole status {}; singleton status {}; \
         singleton stdout: {}; singleton stderr: {}",
        cases.len(),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_masked_stores_match_interpreter_and_fault_before_mxcsr_or_memory_commit() {
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::evex_fp16_narrow_memory_destination::semantics::\
         native_masked_stores_match_interpreter_and_fault_before_mxcsr_or_memory_commit",
    );
}
