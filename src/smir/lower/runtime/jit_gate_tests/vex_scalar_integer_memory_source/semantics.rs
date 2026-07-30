//! Native/interpreter success and fault differential coverage.

#![cfg(target_arch = "x86_64")]

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_YMM16};

#[derive(Clone, Debug)]
struct MemoryContext {
    ok: u64,
    calls: u64,
    transfers: u64,
    last_addr: u64,
    last_index: u32,
    last_size: u32,
    last_zero_upper: u32,
    value: u64,
    observed: u64,
}

extern "C" fn vector_load_helper(
    state: *mut GuestRegs,
    addr: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut MemoryContext) };
    context.calls += 1;
    context.last_addr = addr;
    context.last_index = destination;
    context.last_size = size;
    context.last_zero_upper = zero_upper;
    if context.ok == 0
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 4 | 8)
        || zero_upper != 1
    {
        return 0;
    }
    state.vector_scratch = [0; 8];
    state.vector_scratch[0] = if size == 4 {
        context.value & 0xFFFF_FFFF
    } else {
        context.value
    };
    context.transfers += 1;
    1
}

extern "C" fn vector_store_helper(state: *mut GuestRegs, addr: u64, source: u32, size: u32) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut MemoryContext) };
    context.calls += 1;
    context.last_addr = addr;
    context.last_index = source;
    context.last_size = size;
    context.observed = if size == 4 {
        state.vector_scratch[0] & 0xFFFF_FFFF
    } else {
        state.vector_scratch[0]
    };
    if context.ok == 0
        || source != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || !matches!(size, 4 | 8)
    {
        return 0;
    }
    context.transfers += 1;
    1
}

fn full_guest_regs(case: ScalarIntegerCase, seed: usize) -> GuestRegs {
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

fn effective_address(registers: &GuestRegs, case: ScalarIntegerCase) -> u64 {
    registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64)
}

fn interpreter_context(initial: &GuestRegs) -> SmirContext {
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
    context
}

fn interpreted_load_vectors(
    function: &SmirFunction,
    initial: &GuestRegs,
    address: u64,
    value: u64,
    case: ScalarIntegerCase,
) -> [[u64; 8]; 32] {
    let mut context = interpreter_context(initial);
    let mut memory = FlatMemory::new(0x10000);
    let bytes = value.to_le_bytes();
    memory.load(
        address as usize,
        &bytes[..case.memory_width().bytes() as usize],
    );
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
    assert_eq!(x86.k, initial.k, "{case:?}: masks");
    assert_eq!(x86.rflags, initial.rflags, "{case:?}: RFLAGS");
    assert_eq!(x86.mxcsr, initial.mxcsr, "{case:?}: MXCSR");
    std::array::from_fn(|index| x86.xmm[index][..8].try_into().unwrap())
}

fn interpreted_store_value(
    function: &SmirFunction,
    initial: &GuestRegs,
    address: u64,
    case: ScalarIntegerCase,
) -> u64 {
    let mut context = interpreter_context(initial);
    let mut memory = FlatMemory::new(0x10000);
    let sentinel = [0xA5; 24];
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
    for (index, expected) in initial.zmm.iter().enumerate() {
        assert_eq!(&x86.xmm[index][..8], expected, "{case:?}: ZMM{index}");
    }
    assert_eq!(x86.k, initial.k, "{case:?}: masks");
    assert_eq!(x86.rflags, initial.rflags, "{case:?}: RFLAGS");
    assert_eq!(x86.mxcsr, initial.mxcsr, "{case:?}: MXCSR");

    let mut actual = [0u8; 24];
    memory.read(address - 8, &mut actual).unwrap();
    let size = case.memory_width().bytes() as usize;
    assert_eq!(actual[..8], sentinel[..8], "{case:?}: leading bytes");
    assert_eq!(
        actual[8 + size..],
        sentinel[8 + size..],
        "{case:?}: trailing bytes"
    );
    let mut value = [0u8; 8];
    value[..size].copy_from_slice(&actual[8..8 + size]);
    u64::from_le_bytes(value)
}

fn expected_load_scratch(case: ScalarIntegerCase, value: u64) -> [u64; 8] {
    let mut scratch = [0; 8];
    scratch[0] = if case.memory_width() == MemWidth::B4 {
        value & 0xFFFF_FFFF
    } else {
        value
    };
    scratch
}

fn expected_store_scratch(initial: &GuestRegs, case: ScalarIntegerCase, value: u64) -> [u64; 8] {
    let mut scratch = initial.vector_scratch;
    if case.memory_width() == MemWidth::B4 {
        scratch[0] = (scratch[0] & 0xFFFF_FFFF_0000_0000) | (value & 0xFFFF_FFFF);
    } else {
        scratch[0] = value;
    }
    scratch
}

fn assert_helper_observation(
    context: &MemoryContext,
    address: u64,
    case: ScalarIntegerCase,
    expected: u64,
    transfers: u64,
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
        case.memory_width().bytes(),
        "{label} {case:?}"
    );
    assert_eq!(context.transfers, transfers, "{label} {case:?}");
    match case.alias.kind() {
        X86VexScalarIntegerMemoryKind::Load => {
            assert_eq!(context.last_zero_upper, 1, "{label} {case:?}");
        }
        X86VexScalarIntegerMemoryKind::Store => {
            assert_eq!(context.observed, expected, "{label} {case:?}");
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: OptLevel,
    instruction: ScalarIntegerCase,
    seed: usize,
}

fn native_cases() -> Vec<NativeCase> {
    let c5_bases = [0u8, 2, 4, 5, 7, 1, 3, 6];
    let c4_bases = [11u8, 12, 4, 5, 14, 0, 2, 15];
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for level in [OptLevel::O0, OptLevel::O2] {
        for form in VexForm::ALL {
            for alias in ScalarIntegerAlias::ALL {
                for vector in 0..16u8 {
                    let base = if form == VexForm::C5 {
                        c5_bases[usize::from(vector) & 7]
                    } else {
                        c4_bases[usize::from(vector) & 7]
                    };
                    cases.push(NativeCase {
                        level,
                        instruction: ScalarIntegerCase {
                            alias,
                            form,
                            vector,
                            base,
                        },
                        seed: ordinal,
                    });
                    ordinal += 1;
                }
            }
        }
    }
    assert_eq!(cases.len(), 2 * 3 * 4 * 16);
    assert!(cases.iter().any(|case| case.instruction.vector >= 8));
    assert!(cases.iter().any(|case| case.instruction.base == 4));
    assert!(cases.iter().any(|case| case.instruction.base == 5));
    cases
}

const CHILD_RANGE_ENV: &str = "RAX_VEX_SCALAR_INTEGER_MEMORY_CHILD_RANGE";

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

fn execute_case_range(cases: &[NativeCase], range: std::ops::Range<usize>) {
    assert!(range.start < range.end && range.end <= cases.len());
    let executions = range.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for native_case in &cases[range] {
        let case = native_case.instruction;
        let function = optimize(lift_case(case), native_case.level);
        let (code, entry) = lower_case(&function, case);
        let exec = ExecMem::new(&code)
            .unwrap_or_else(|error| panic!("{:?} {case:?}: {error:?}", native_case.level));
        let value = 0x8877_6655_4433_2211u64
            ^ (native_case.seed as u64).wrapping_mul(0x1020_4081_0204_0810);

        let mut context = MemoryContext {
            ok: 1,
            calls: 0,
            transfers: 0,
            last_addr: 0,
            last_index: 0,
            last_size: 0,
            last_zero_upper: 0,
            value,
            observed: 0,
        };
        let mut registers = full_guest_regs(case, native_case.seed);
        let address = effective_address(&registers, case);
        registers.ctx = (&mut context as *mut MemoryContext) as u64;
        registers.vec_load_fn = vector_load_helper as usize as u64;
        registers.vec_store_fn = vector_store_helper as usize as u64;
        let initial = registers;
        let mut expected = initial;
        let semantic_value = match case.alias.kind() {
            X86VexScalarIntegerMemoryKind::Load => {
                expected.zmm = interpreted_load_vectors(&function, &initial, address, value, case);
                expected.vector_scratch = expected_load_scratch(case, value);
                value
            }
            X86VexScalarIntegerMemoryKind::Store => {
                let stored = interpreted_store_value(&function, &initial, address, case);
                expected.vector_scratch = expected_store_scratch(&initial, case, stored);
                stored
            }
        };

        exec.run(entry, &mut registers);
        expected.host_mxcsr = registers.host_mxcsr;
        assert_eq!(
            registers, expected,
            "{:?} {case:?}: success",
            native_case.level
        );
        assert_helper_observation(&context, address, case, semantic_value, 1, "success");
        successes += 1;

        let mut context = MemoryContext {
            ok: 0,
            calls: 0,
            transfers: 0,
            last_addr: 0,
            last_index: 0,
            last_size: 0,
            last_zero_upper: 0,
            value: value.rotate_left(17),
            observed: 0,
        };
        let mut registers = full_guest_regs(case, native_case.seed ^ 0x55);
        let address = effective_address(&registers, case);
        registers.ctx = (&mut context as *mut MemoryContext) as u64;
        registers.vec_load_fn = vector_load_helper as usize as u64;
        registers.vec_store_fn = vector_store_helper as usize as u64;
        let initial = registers;
        let mut expected = initial;
        let semantic_value = match case.alias.kind() {
            X86VexScalarIntegerMemoryKind::Load => context.value,
            X86VexScalarIntegerMemoryKind::Store => {
                let stored = interpreted_store_value(&function, &initial, address, case);
                expected.vector_scratch = expected_store_scratch(&initial, case, stored);
                stored
            }
        };
        expected.exit_pc = PC;

        exec.run(entry, &mut registers);
        expected.host_mxcsr = registers.host_mxcsr;
        assert_eq!(
            registers, expected,
            "{:?} {case:?}: fault",
            native_case.level
        );
        assert_helper_observation(&context, address, case, semantic_value, 0, "fault");
        faults += 1;
    }
    assert_eq!(successes, executions);
    assert_eq!(faults, executions);
    eprintln!(
        "executed {successes} successful and {faults} faulting native VEX scalar-integer memory cases"
    );
}

fn run_child_range(test_name: &str, range: std::ops::Range<usize>) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().expect("current unit-test executable"))
        .arg(test_name)
        .arg("--exact")
        .arg("--nocapture")
        .env(CHILD_RANGE_ENV, format!("{}:{}", range.start, range.end))
        .output()
        .expect("run isolated native VEX scalar-integer memory differential")
}

fn run_isolated_native_differential(test_name: &str) {
    let cases = native_cases();
    if let Some(range) = child_range() {
        execute_case_range(&cases, range);
        return;
    }

    let whole = run_child_range(test_name, 0..cases.len());
    if whole.status.success() {
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
        "isolated native VEX scalar-integer memory failure at case {start}/{}: \
         {case:?} {bytes:02X?}; whole status {}; singleton status {}; \
         singleton stdout: {}; singleton stderr: {}",
        cases.len(),
        whole.status,
        singleton.status,
        String::from_utf8_lossy(&singleton.stdout),
        String::from_utf8_lossy(&singleton.stderr),
    );
}

#[test]
fn native_aliases_match_o0_o2_interpreter_and_fault_without_guest_commit() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX scalar-integer memory differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_scalar_integer_memory_source::semantics::\
         native_aliases_match_o0_o2_interpreter_and_fault_without_guest_commit",
    );
}
