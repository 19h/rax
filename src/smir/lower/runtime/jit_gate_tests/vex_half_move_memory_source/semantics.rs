//! Native/interpreter differential and independent Intel-contract checks.

use super::*;

#[cfg(target_arch = "x86_64")]
use crate::smir::interpret::{BlockResult, SmirInterpreter};
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::flags::MaterializedFlags;
#[cfg(target_arch = "x86_64")]
use crate::smir::ir::memory::FlatMemory;
#[cfg(target_arch = "x86_64")]
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_YMM16};

#[cfg(target_arch = "x86_64")]
const DIFFERENTIAL_LEVELS: [OptLevel; 2] = [OptLevel::O0, OptLevel::O2];

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct VectorMemoryContext {
    value: u64,
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_index: u32,
    last_size: u32,
    last_zero_upper: u32,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn vector_load_helper(
    state: *mut GuestRegs,
    addr: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut VectorMemoryContext) };
    context.calls += 1;
    context.last_addr = addr;
    context.last_index = destination;
    context.last_size = size;
    context.last_zero_upper = zero_upper;
    if context.ok == 0
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || size != 8
        || zero_upper == 0
    {
        return 0;
    }

    state.vector_scratch = [0; 8];
    state.vector_scratch[0] = context.value;
    1
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: HalfMoveCase, seed: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((seed as u64) * 0x20)
        }),
        rflags: 0x2 | (((seed as u64).wrapping_mul(0x145)) & 0x8D5),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_YMM16,
        mxcsr: 0x1F80 | ((seed as u32) & 0x3F),
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 11 + word * 5) as u32)
                ^ (index as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (seed as u64).wrapping_mul(0x0102_0408_1020_4081)
                ^ (word as u64).wrapping_mul(0x8040_2010_0804_0201)
        });
    }
    registers.gpr[usize::from(case.base)] = 0x2000 + ((seed & 0x1F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn effective_address(registers: &GuestRegs, case: HalfMoveCase) -> u64 {
    registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64)
}

#[cfg(target_arch = "x86_64")]
fn interpreted_expected(
    function: &SmirFunction,
    initial: &GuestRegs,
    source: u64,
    address: u64,
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
    memory.load(address as usize, &source.to_le_bytes());
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
    for (index, value) in expected.zmm.iter_mut().enumerate() {
        value.copy_from_slice(&x86.xmm[index][..8]);
    }
    expected.k = x86.k;
    expected.rflags = x86.rflags;
    expected.mxcsr = x86.mxcsr;
    expected.vector_scratch = [0; 8];
    expected.vector_scratch[0] = source;
    expected
}

#[cfg(target_arch = "x86_64")]
fn assert_helper_observation(
    context: &VectorMemoryContext,
    address: u64,
    case: HalfMoveCase,
    label: &str,
) {
    assert_eq!(context.calls, 1, "{label} {case:?}");
    assert_eq!(context.last_addr, address, "{label} {case:?}");
    assert_eq!(
        context.last_index,
        crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
        "{label} {case:?}"
    );
    assert_eq!(context.last_size, 8, "{label} {case:?}");
    assert_eq!(context.last_zero_upper, 1, "{label} {case:?}");
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
struct NativeCase {
    level: OptLevel,
    instruction: HalfMoveCase,
    source: u64,
    seed: usize,
}

#[cfg(target_arch = "x86_64")]
fn native_cases() -> Vec<NativeCase> {
    let c5_bases = [0u8, 2, 4, 5, 7, 1, 3, 6];
    let c4_bases = [11u8, 12, 4, 5, 14, 0, 2, 15];
    let samples = [
        0u64,
        u64::MAX,
        0x8000_0000_0000_0000,
        0x0000_0000_0000_0001,
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
        0x7FF8_1234_5678_9ABC,
        0x000F_FFFF_FFFF_FFFF,
    ];
    let mut cases = Vec::new();
    let mut ordinal = 0usize;
    for level in DIFFERENTIAL_LEVELS {
        for lane in MemoryLane::ALL {
            for format in MoveFormat::ALL {
                for form in VexForm::ALL {
                    for scanner_destination in 0..8u8 {
                        for source1 in 0..16u8 {
                            let destination = scanner_destination
                                + if (usize::from(source1)
                                    + usize::from(scanner_destination)
                                    + ordinal)
                                    & 1
                                    == 0
                                {
                                    0
                                } else {
                                    8
                                };
                            let base = if form == VexForm::C5 {
                                c5_bases[usize::from(source1) & 7]
                            } else {
                                c4_bases[usize::from(source1) & 7]
                            };
                            cases.push(NativeCase {
                                level,
                                instruction: HalfMoveCase {
                                    lane,
                                    format,
                                    form,
                                    destination,
                                    source1,
                                    base,
                                },
                                source: samples[(ordinal + usize::from(source1)) & 7]
                                    ^ (ordinal as u64).wrapping_mul(0x0101_0101_0101_0101),
                                seed: ordinal,
                            });
                            ordinal += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases.len(), 2 * 1_536);
    assert!(cases.iter().any(|case| case.instruction.destination >= 8));
    assert!(
        cases
            .iter()
            .any(|case| case.instruction.destination == case.instruction.source1)
    );
    assert!(cases.iter().any(|case| case.instruction.base == 4));
    assert!(cases.iter().any(|case| case.instruction.base == 5));
    cases
}

#[cfg(target_arch = "x86_64")]
const CHILD_RANGE_ENV: &str = "RAX_VEX_HALF_MOVE_MEMORY_CHILD_RANGE";

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

        let mut context = VectorMemoryContext {
            value: native_case.source,
            ok: 1,
            calls: 0,
            last_addr: 0,
            last_index: 0,
            last_size: 0,
            last_zero_upper: 0,
        };
        let mut registers = full_guest_regs(case, native_case.seed);
        let address = effective_address(&registers, case);
        registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
        registers.vec_load_fn = vector_load_helper as usize as u64;
        let initial = registers;
        let mut expected = interpreted_expected(&function, &initial, native_case.source, address);

        exec.run(entry, &mut registers);
        expected.host_mxcsr = registers.host_mxcsr;
        assert_eq!(
            registers, expected,
            "{:?} {case:?}: success",
            native_case.level
        );
        assert_helper_observation(&context, address, case, "success");
        successes += 1;

        let mut context = VectorMemoryContext {
            value: native_case.source ^ u64::MAX,
            ok: 0,
            calls: 0,
            last_addr: 0,
            last_index: 0,
            last_size: 0,
            last_zero_upper: 0,
        };
        let mut registers = full_guest_regs(case, native_case.seed ^ 0x55);
        let address = effective_address(&registers, case);
        registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
        registers.vec_load_fn = vector_load_helper as usize as u64;
        let mut expected = registers;
        expected.exit_pc = PC;

        exec.run(entry, &mut registers);
        expected.host_mxcsr = registers.host_mxcsr;
        assert_eq!(
            registers, expected,
            "{:?} {case:?}: fault",
            native_case.level
        );
        assert_helper_observation(&context, address, case, "fault");
        faults += 1;
    }
    assert_eq!(successes, executions);
    assert_eq!(faults, executions);
    eprintln!(
        "executed {successes} successful and {faults} faulting native VEX half-move memory cases"
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
        .expect("run isolated native VEX half-move memory differential")
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
        "isolated native VEX half-move memory failure at case {start}/{}: \
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
fn native_half_moves_match_o0_o2_interpreter_and_fault_without_commit() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX half-move memory differential: host lacks AVX");
        return;
    }
    run_isolated_native_differential(
        "smir::lower::runtime::jit_gate_tests::vex_half_move_memory_source::semantics::\
         native_half_moves_match_o0_o2_interpreter_and_fault_without_commit",
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn interpreter_matches_intel_lane_merge_zero_upper_alias_and_no_status_contracts() {
    let source = 0x0123_4567_89AB_CDEFu64;
    for lane in MemoryLane::ALL {
        let mut format_results = Vec::new();
        for format in MoveFormat::ALL {
            let case = HalfMoveCase {
                lane,
                format,
                form: VexForm::C4W1,
                destination: 10,
                source1: 10,
                base: 11,
            };
            let function = optimize(lift_case(case), OptLevel::O2);
            let initial = full_guest_regs(case, usize::from(lane.index()));
            let address = effective_address(&initial, case);
            let actual = interpreted_expected(&function, &initial, source, address);
            let destination = actual.zmm[usize::from(case.destination)];
            let original_merge = initial.zmm[usize::from(case.source1)];

            assert_eq!(destination[usize::from(lane.index())], source, "{case:?}");
            assert_eq!(
                destination[usize::from(1 - lane.index())],
                original_merge[usize::from(1 - lane.index())],
                "{case:?}"
            );
            assert_eq!(destination[2..], [0; 6], "{case:?}");
            assert_eq!(actual.gpr, initial.gpr, "{case:?}");
            assert_eq!(actual.rflags, initial.rflags, "{case:?}");
            assert_eq!(actual.k, initial.k, "{case:?}");
            assert_eq!(actual.mxcsr, initial.mxcsr, "{case:?}");
            format_results.push(destination);
        }
        assert_eq!(
            format_results[0], format_results[1],
            "PS/PD names must be bit-identical for {lane:?}"
        );
    }
}
