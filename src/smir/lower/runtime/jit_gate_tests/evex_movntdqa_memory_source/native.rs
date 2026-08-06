use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::FlatMemory;
use crate::smir::lower::runtime::{ExecMem, GuestRegs, X86_VECTOR_STATE_K16};

const MEMORY_ADDRESS: u64 = 0x4000;

#[derive(Clone, Debug)]
struct VectorMemoryContext {
    value: [u64; 8],
    ok: bool,
    calls: u64,
    last_address: u64,
    last_index: u32,
    last_size: u32,
    last_zero_upper: u32,
}

extern "C" fn vector_load_helper(
    state: *mut GuestRegs,
    address: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut VectorMemoryContext) };
    context.calls += 1;
    context.last_address = address;
    context.last_index = destination;
    context.last_size = size;
    context.last_zero_upper = zero_upper;
    if !context.ok || destination >= 32 || !matches!(size, 16 | 32 | 64) || zero_upper != 1 {
        return 0;
    }

    let mut value = [0; 8];
    let words = size as usize / 8;
    value[..words].copy_from_slice(&context.value[..words]);
    state.zmm[destination as usize] = value;
    1
}

fn source_vector(case: MovntdqaMemoryCase, seed: usize) -> [u64; 8] {
    std::array::from_fn(|word| {
        0xF0E1_D2C3_B4A5_9687u64
            .rotate_left(((usize::from(case.destination) * 7 + word * 11 + seed) & 63) as u32)
            ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
    })
}

fn full_guest_regs(case: MovntdqaMemoryCase, seed: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1000u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((seed as u64) * 0x10)
        }),
        rflags: 0x2 | (((seed as u64).wrapping_mul(0x145)) & 0x8D5),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        k: std::array::from_fn(|index| 0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_K16,
        mxcsr: 0x1F80 | ((seed as u32) & 0x3F),
        vector_scratch: std::array::from_fn(|index| {
            0xCCDD_EEFF_0011_2233u64 ^ (index as u64).wrapping_mul(0x1111_1111_1111_1111)
        }),
        cr0: 1,
        cr4: 1 << 18,
        xcr0: 0b1110_0110,
        cs_l: 1,
        apx_enabled: 1,
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0x8877_6655_4433_2211u64.rotate_left((index * 13 + word * 3) as u32)
                ^ (index as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x1020_4081_0204_0810)
        });
    }
    registers.gpr[usize::from(case.base)] =
        MEMORY_ADDRESS.wrapping_sub(u64::from(case.width.bytes()));
    registers
}

fn expected_success(
    mut registers: GuestRegs,
    case: MovntdqaMemoryCase,
    source: [u64; 8],
) -> GuestRegs {
    let words = case.width.bytes() as usize / 8;
    registers.zmm[usize::from(case.destination)] = [0; 8];
    registers.zmm[usize::from(case.destination)][..words].copy_from_slice(&source[..words]);
    registers
}

fn words_to_bytes(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn assert_interpreter_matches(
    function: &SmirFunction,
    initial: &GuestRegs,
    expected: &GuestRegs,
    source: [u64; 8],
    case: MovntdqaMemoryCase,
    level: OptLevel,
) {
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
    let bytes = words_to_bytes(source);
    memory.load(
        MEMORY_ADDRESS as usize,
        &bytes[..case.width.bytes() as usize],
    );
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
        "{level:?} {case:?}: {result:?}"
    );

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    assert_eq!(x86.gpr, expected.gpr, "{level:?} {case:?}: GPRs");
    for (index, value) in expected.zmm.iter().enumerate() {
        assert_eq!(
            &x86.xmm[index][..8],
            value,
            "{level:?} {case:?}: ZMM{index}"
        );
    }
    assert_eq!(x86.k, expected.k, "{level:?} {case:?}: opmasks");
    assert_eq!(x86.rflags, expected.rflags, "{level:?} {case:?}: RFLAGS");
    assert_eq!(x86.mxcsr, expected.mxcsr, "{level:?} {case:?}: MXCSR");
}

fn assert_helper_call(context: &VectorMemoryContext, case: MovntdqaMemoryCase) {
    assert_eq!(context.calls, 1, "{case:?}");
    assert_eq!(context.last_address, MEMORY_ADDRESS, "{case:?}");
    assert_eq!(context.last_index, u32::from(case.destination), "{case:?}");
    assert_eq!(context.last_size, case.width.bytes(), "{case:?}");
    assert_eq!(context.last_zero_upper, 1, "{case:?}");
}

fn host_supports(case: MovntdqaMemoryCase) -> bool {
    std::is_x86_feature_detected!("avx512f")
        && (case.width == VecWidth::V512 || std::is_x86_feature_detected!("avx512vl"))
}

#[test]
fn native_vmovntdqa_matches_interpreter_and_is_precise_for_faults_and_alignment() {
    let mut compiled = 0usize;
    for case in all_cases() {
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let _ = lower(&function, case);
            compiled += 1;
        }
    }
    assert_eq!(compiled, 96 * 2);

    if !std::is_x86_feature_detected!("avx512f") {
        eprintln!("skipping native EVEX VMOVNTDQA differential: host lacks AVX-512F");
        return;
    }
    let cases: Vec<_> = all_cases()
        .into_iter()
        .filter(|case| host_supports(*case))
        .collect();
    assert!(!cases.is_empty());

    let expected_runs = cases.len() * 2;
    let mut successes = 0usize;
    let mut helper_faults = 0usize;
    let mut alignment_exits = 0usize;
    for (seed, case) in cases.into_iter().enumerate() {
        let source = source_vector(case, seed);
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_case(case), level);
            let (code, entry) = lower(&function, case);
            let executable =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));

            let mut context = VectorMemoryContext {
                value: source,
                ok: true,
                calls: 0,
                last_address: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, seed);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
            let initial = registers;
            let mut expected = expected_success(registers, case, source);
            executable.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_helper_call(&context, case);
            assert_interpreter_matches(&function, &initial, &expected, source, case, level);
            successes += 1;

            let mut fault_context = VectorMemoryContext {
                value: source,
                ok: false,
                calls: 0,
                last_address: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut fault_registers = full_guest_regs(case, seed ^ 0x55);
            fault_registers.ctx = (&mut fault_context as *mut VectorMemoryContext) as u64;
            fault_registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
            let mut expected_fault = fault_registers;
            expected_fault.exit_pc = PC;
            executable.run(entry, &mut fault_registers);
            expected_fault.host_mxcsr = fault_registers.host_mxcsr;
            assert_eq!(
                fault_registers, expected_fault,
                "{level:?} {case:?}: helper fault"
            );
            assert_helper_call(&fault_context, case);
            helper_faults += 1;

            let mut alignment_context = VectorMemoryContext {
                value: source,
                ok: true,
                calls: 0,
                last_address: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut alignment_registers = full_guest_regs(case, seed ^ 0x2A);
            alignment_registers.gpr[usize::from(case.base)] =
                alignment_registers.gpr[usize::from(case.base)].wrapping_add(1);
            alignment_registers.ctx = (&mut alignment_context as *mut VectorMemoryContext) as u64;
            alignment_registers.vec_load_fn = vector_load_helper as *const () as usize as u64;
            let mut expected_alignment = alignment_registers;
            expected_alignment.exit_pc = PC;
            executable.run(entry, &mut alignment_registers);
            expected_alignment.host_mxcsr = alignment_registers.host_mxcsr;
            assert_eq!(
                alignment_registers, expected_alignment,
                "{level:?} {case:?}: alignment exit"
            );
            assert_eq!(
                alignment_context.calls, 0,
                "{level:?} {case:?}: helper ran before alignment exit"
            );
            alignment_exits += 1;
        }
    }
    assert_eq!(successes, expected_runs);
    assert_eq!(helper_faults, expected_runs);
    assert_eq!(alignment_exits, expected_runs);
}
