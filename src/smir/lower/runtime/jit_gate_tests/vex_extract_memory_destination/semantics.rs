//! Native success/fault differential coverage for VEX extraction to memory.

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
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_YMM16};

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct StoreMemoryContext {
    ok: u64,
    calls: u64,
    commits: u64,
    last_addr: u64,
    last_index: u32,
    last_size: u32,
    observed: [u8; 16],
    committed: [u8; 16],
}

#[cfg(target_arch = "x86_64")]
extern "C" fn vector_store_helper(state: *mut GuestRegs, addr: u64, source: u32, size: u32) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut StoreMemoryContext) };
    context.calls += 1;
    context.last_addr = addr;
    context.last_index = source;
    context.last_size = size;
    for (destination, word) in context.observed.chunks_mut(8).zip(state.vector_scratch) {
        destination.copy_from_slice(&word.to_le_bytes()[..destination.len()]);
    }
    if context.ok == 0
        || source != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 1 | 2 | 4 | 8 | 16)
    {
        return 0;
    }
    context.commits += 1;
    context.committed = context.observed;
    1
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: ExtractCase, seed: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1800u64
                .wrapping_add((index as u64) * 0x111)
                .wrapping_add((seed as u64) * 0x20)
        }),
        rflags: 0x2 | (((seed as u64).wrapping_mul(0x195)) & 0x8D5),
        exit_pc: 0xDEAD_BEEF_CAFE_BABE,
        k: std::array::from_fn(|index| 0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 9) as u32)),
        vector_active: X86_VECTOR_STATE_YMM16,
        mxcsr: 0x1F80 | ((seed as u32) & 0x3F),
        vector_scratch: std::array::from_fn(|index| {
            0xCCDD_EEFF_0011_2233u64 ^ (index as u64).wrapping_mul(0x1111_1111_1111_1111)
        }),
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0x0123_4567_89AB_CDEFu64.rotate_left((index * 13 + word * 7) as u32)
                ^ (index as u64).wrapping_mul(0x8040_2010_0804_0201)
                ^ (seed as u64).wrapping_mul(0x1020_4081_0204_0810)
                ^ (word as u64).wrapping_mul(0x1111_2222_4444_8888)
        });
    }
    registers.gpr[usize::from(case.base)] = 0x3000 + ((seed & 0x1F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn effective_address(registers: &GuestRegs, case: ExtractCase) -> u64 {
    registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64)
}

#[cfg(target_arch = "x86_64")]
fn interpreted_store_value(
    function: &SmirFunction,
    initial: &GuestRegs,
    address: u64,
    case: ExtractCase,
) -> Vec<u8> {
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

    let width = case.kind.memory_width() as usize;
    let mut memory = FlatMemory::new(0x10000);
    let sentinel = [0xA5; 32];
    memory.load(address as usize - 8, &sentinel);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
        "{case:?}: {result:?}"
    );

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr, initial.gpr, "{case:?}: GPRs");
    for (index, value) in initial.zmm.iter().enumerate() {
        assert_eq!(&x86.xmm[index][..8], value, "{case:?}: ZMM{index}");
    }
    assert_eq!(x86.k, initial.k, "{case:?}: masks");
    assert_eq!(x86.rflags, initial.rflags, "{case:?}: RFLAGS");
    assert_eq!(x86.mxcsr, initial.mxcsr, "{case:?}: MXCSR");

    let mut actual = [0u8; 32];
    memory.read(address - 8, &mut actual).unwrap();
    assert_eq!(actual[..8], sentinel[..8], "{case:?}: leading bytes");
    assert_eq!(
        actual[8 + width..],
        sentinel[8 + width..],
        "{case:?}: trailing bytes"
    );
    actual[8..8 + width].to_vec()
}

#[cfg(target_arch = "x86_64")]
fn scratch_after_store(initial: [u64; 8], value: &[u8]) -> [u64; 8] {
    let mut bytes = [0u8; 64];
    for (chunk, word) in bytes.chunks_mut(8).zip(initial) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes[..value.len()].copy_from_slice(value);
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

#[cfg(target_arch = "x86_64")]
fn assert_helper_observation(
    context: &StoreMemoryContext,
    address: u64,
    expected: &[u8],
    commits: u64,
    case: ExtractCase,
    label: &str,
) {
    assert_eq!(context.calls, 1, "{label} {case:?}");
    assert_eq!(context.last_addr, address, "{label} {case:?}");
    assert_eq!(
        context.last_index,
        crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
        "{label} {case:?}"
    );
    assert_eq!(
        context.last_size,
        case.kind.memory_width(),
        "{label} {case:?}"
    );
    assert_eq!(
        &context.observed[..expected.len()],
        expected,
        "{label} {case:?}"
    );
    assert_eq!(context.commits, commits, "{label} {case:?}");
    if commits != 0 {
        assert_eq!(
            &context.committed[..expected.len()],
            expected,
            "{label} {case:?}"
        );
    }
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: OptLevel,
    instruction: ExtractCase,
    seed: usize,
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    let bases = [0u8, 2, 4, 5, 11, 12, 13, 15, 1, 3, 6, 7, 8, 9, 10, 14];
    let avx2 = std::is_x86_feature_detected!("avx2");
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for level in [OptLevel::O0, OptLevel::O2] {
        for kind in ExtractKind::ALL {
            if kind.needs_avx2() && !avx2 {
                continue;
            }
            for source in 0..16u8 {
                cases.push(NativeCase {
                    level,
                    instruction: ExtractCase {
                        kind,
                        source,
                        base: bases[ordinal & 15],
                        immediate: ((ordinal * 37) as u8) ^ 0xA5,
                    },
                    seed: ordinal,
                });
                ordinal += 1;
            }
        }
    }
    let kinds = if avx2 { 10 } else { 9 };
    assert_eq!(cases.len(), 2 * kinds * 16);
    assert!(cases.iter().any(|case| case.instruction.source >= 8));
    assert!(cases.iter().any(|case| case.instruction.base == 4));
    assert!(cases.iter().any(|case| case.instruction.base == 5));
    cases
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_EXTRACT_MEMORY_CHILD_RANGE";

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

        let mut context = StoreMemoryContext {
            ok: 1,
            calls: 0,
            commits: 0,
            last_addr: 0,
            last_index: 0,
            last_size: 0,
            observed: [0; 16],
            committed: [0; 16],
        };
        let mut registers = full_guest_regs(case, native_case.seed);
        let address = effective_address(&registers, case);
        registers.ctx = (&mut context as *mut StoreMemoryContext) as u64;
        registers.vec_store_fn = vector_store_helper as usize as u64;
        let initial = registers;
        let stored = interpreted_store_value(&function, &initial, address, case);
        let mut expected = initial;
        expected.vector_scratch = scratch_after_store(initial.vector_scratch, &stored);

        exec.run(entry, &mut registers);
        expected.host_mxcsr = registers.host_mxcsr;
        assert_eq!(
            registers, expected,
            "{:?} {case:?}: success",
            native_case.level
        );
        assert_helper_observation(&context, address, &stored, 1, case, "success");
        successes += 1;

        let mut context = StoreMemoryContext {
            ok: 0,
            calls: 0,
            commits: 0,
            last_addr: 0,
            last_index: 0,
            last_size: 0,
            observed: [0; 16],
            committed: [0; 16],
        };
        let mut registers = full_guest_regs(case, native_case.seed ^ 0x55);
        let address = effective_address(&registers, case);
        registers.ctx = (&mut context as *mut StoreMemoryContext) as u64;
        registers.vec_store_fn = vector_store_helper as usize as u64;
        let initial = registers;
        let stored = interpreted_store_value(&function, &initial, address, case);
        let mut expected = initial;
        expected.vector_scratch = scratch_after_store(initial.vector_scratch, &stored);
        expected.exit_pc = PC;

        exec.run(entry, &mut registers);
        expected.host_mxcsr = registers.host_mxcsr;
        assert_eq!(
            registers, expected,
            "{:?} {case:?}: fault",
            native_case.level
        );
        assert_helper_observation(&context, address, &stored, 0, case, "fault");
        faults += 1;
    }
    assert_eq!(successes, executions);
    assert_eq!(faults, executions);
    eprintln!(
        "executed {successes} successful and {faults} faulting native VEX extract-memory cases"
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
        .expect("run isolated native VEX extract-memory differential")
}

#[cfg(target_arch = "x86_64")]
fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    if let Some(range) = child_range() {
        execute_native_case_range(&cases, range);
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
        "isolated native VEX extract-memory failure at case {start}/{}: \
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
fn native_extracts_match_o0_o2_interpreter_and_fault_without_memory_commit() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX extract-memory differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_extract_memory_destination::semantics::\
         native_extracts_match_o0_o2_interpreter_and_fault_without_memory_commit",
    );
}
