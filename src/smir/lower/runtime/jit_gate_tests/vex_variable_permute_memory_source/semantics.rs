//! Independent variable-permute model and native helper-boundary differential.

use super::*;

fn element_bits(vector: &[u64; 8], width: usize, lane: usize) -> u64 {
    let bit = lane * width;
    let mask = if width == 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    (vector[bit / 64] >> (bit % 64)) & mask
}

fn set_element_bits(vector: &mut [u64; 8], width: usize, lane: usize, value: u64) {
    let bit = lane * width;
    let mask = if width == 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    let shift = bit % 64;
    vector[bit / 64] = (vector[bit / 64] & !(mask << shift)) | ((value & mask) << shift);
}

fn memory_vector(case: VariablePermuteMemoryCase, ordinal: usize) -> [u64; 8] {
    let mut value = std::array::from_fn(|word| {
        0xA5C3_6996_F00F_5AA5u64.rotate_left(((ordinal * 7 + word * 13) & 63) as u32)
            ^ (ordinal as u64).wrapping_mul(0x0102_0408_1020_4081)
            ^ (word as u64).wrapping_mul(0x1111_2222_4444_8888)
    });
    if case.operation.is_permil() {
        let elem_bits = if case.operation == VariablePermute::PermilPd {
            64
        } else {
            32
        };
        let lanes = case.width.bytes() as usize * 8 / elem_bits;
        for lane in 0..lanes {
            let selector = match case.operation {
                VariablePermute::PermilPs => (lane + ordinal) & 3,
                VariablePermute::PermilPd => ((lane + ordinal) & 1) << 1,
                _ => unreachable!(),
            };
            let selector_mask = if elem_bits == 64 { 0x2 } else { 0x3 };
            let noise = 0xA5A5_0000_5A5A_0000u64.rotate_left((lane * 9) as u32);
            set_element_bits(
                &mut value,
                elem_bits,
                lane,
                (noise & !selector_mask) | selector as u64,
            );
        }
    }
    value
}

/// Apply the Intel variable-permute selection equations to raw element bits.
/// In-lane forms read controls from memory and data from VEX.vvvv; full-width
/// forms read indices from VEX.vvvv and the data table from memory.
fn architectural_destination(
    case: VariablePermuteMemoryCase,
    source1: [u64; 8],
    memory: [u64; 8],
) -> [u64; 8] {
    let (elem_bits, table, controls) = match case.operation {
        VariablePermute::PermilPs => (32, &source1, &memory),
        VariablePermute::PermilPd => (64, &source1, &memory),
        VariablePermute::PermPs | VariablePermute::PermD => (32, &memory, &source1),
    };
    let lanes = case.width.bytes() as usize * 8 / elem_bits;
    let mut destination = [0u64; 8];
    for lane in 0..lanes {
        let control = element_bits(controls, elem_bits, lane);
        let source_lane = match case.operation {
            VariablePermute::PermilPs => (lane / 4) * 4 + (control as usize & 3),
            VariablePermute::PermilPd => (lane / 2) * 2 + ((control as usize >> 1) & 1),
            VariablePermute::PermPs | VariablePermute::PermD => control as usize & 7,
        };
        set_element_bits(
            &mut destination,
            elem_bits,
            lane,
            element_bits(table, elem_bits, source_lane),
        );
    }
    destination
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Debug)]
struct VectorMemoryContext {
    value: [u64; 8],
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
        || !matches!(size, 16 | 32)
    {
        return 0;
    }
    let mut value = if zero_upper != 0 {
        [0; 8]
    } else {
        state.vector_scratch
    };
    value[..(size / 8) as usize].copy_from_slice(&context.value[..(size / 8) as usize]);
    state.vector_scratch = value;
    1
}

#[cfg(target_arch = "x86_64")]
fn full_guest_regs(case: VariablePermuteMemoryCase, ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: std::array::from_fn(|index| {
            0x1003u64
                .wrapping_add((index as u64) * 0x101)
                .wrapping_add((ordinal as u64) * 0x10)
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        k: std::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left((index * 7) as u32)),
        vector_active: X86_VECTOR_STATE_YMM16,
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F),
        vector_scratch: [0xCCDD_EEFF_0011_2233; 8],
        ..GuestRegs::default()
    };
    for (index, value) in registers.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687u64.rotate_left((index * 11 + word * 5) as u32)
                ^ (index as u64).wrapping_mul(0x1111_2222_3333_4444)
                ^ (word as u64).wrapping_mul(0x0102_0408_1020_4081)
                ^ (ordinal as u64).wrapping_mul(0x0F1E_2D3C_4B5A_6978)
        });
    }
    if !case.operation.is_permil() {
        let controls = &mut registers.zmm[usize::from(case.source1)];
        for lane in 0..8 {
            let selector = (7usize.wrapping_sub(lane).wrapping_add(ordinal)) & 7;
            let noise = 0x5A5A_0000_A5A5_0000u64.rotate_left((lane * 7) as u32);
            set_element_bits(controls, 32, lane, (noise & !7) | selector as u64);
        }
    }
    registers.gpr[usize::from(case.base)] = 0x2003 + ((ordinal & 0x0F) as u64) * 0x40;
    registers
}

#[cfg(target_arch = "x86_64")]
fn expected_success(
    mut registers: GuestRegs,
    case: VariablePermuteMemoryCase,
    memory: [u64; 8],
) -> GuestRegs {
    let source1 = registers.zmm[usize::from(case.source1)];
    registers.zmm[usize::from(case.destination)] = architectural_destination(case, source1, memory);
    let words = (case.width.bytes() / 8) as usize;
    registers.vector_scratch =
        std::array::from_fn(|word| if word < words { memory[word] } else { 0 });
    registers
}

#[cfg(target_arch = "x86_64")]
fn assert_interpreter_matches(
    function: &SmirFunction,
    initial: &GuestRegs,
    expected: &GuestRegs,
    memory_value: [u64; 8],
    address: u64,
    case: VariablePermuteMemoryCase,
    level: OptLevel,
) {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;

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
    let memory_bytes = memory_value
        .iter()
        .flat_map(|word| word.to_le_bytes())
        .collect::<Vec<_>>();
    memory.load(
        address as usize,
        &memory_bytes[..case.width.bytes() as usize],
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
    assert_eq!(x86.k, expected.k, "{level:?} {case:?}: masks");
    assert_eq!(x86.rflags, expected.rflags, "{level:?} {case:?}: RFLAGS");
    assert_eq!(x86.mxcsr, expected.mxcsr, "{level:?} {case:?}: MXCSR");
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_all_register_aliases_match_intel_model_interpreter_and_precise_faults() {
    use crate::smir::lower::runtime::ExecMem;

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping native VEX variable-permute memory differential: host lacks AVX");
        return;
    }

    let avx2 = std::is_x86_feature_detected!("avx2");
    let cases = cases()
        .into_iter()
        .filter(|case| avx2 || !case.operation.needs_avx2())
        .collect::<Vec<_>>();
    let expected_executions = cases.len() * DIFFERENTIAL_LEVELS.len();
    let mut successes = 0usize;
    let mut faults = 0usize;
    for (ordinal, case) in cases.into_iter().enumerate() {
        for level in DIFFERENTIAL_LEVELS {
            let function = optimize(lift_case(case), level);
            let (code, entry, _) = lower(&function, case);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));
            let memory_value = memory_vector(case, ordinal);

            let mut context = VectorMemoryContext {
                value: memory_value,
                ok: 1,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, ordinal);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let initial = registers;
            let mut expected = expected_success(registers, case, memory_value);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: success");
            assert_eq!(context.calls, 1, "{level:?} {case:?}");
            assert_eq!(context.last_addr, address, "{level:?} {case:?}");
            assert_eq!(
                context.last_index,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                "{level:?} {case:?}"
            );
            assert_eq!(context.last_size, case.width.bytes(), "{level:?} {case:?}");
            assert_eq!(context.last_zero_upper, 1, "{level:?} {case:?}");
            assert_interpreter_matches(
                &function,
                &initial,
                &expected,
                memory_value,
                address,
                case,
                level,
            );
            successes += 1;

            let fault_ordinal = ordinal ^ 0x155;
            let memory_value = memory_vector(case, fault_ordinal);
            let mut context = VectorMemoryContext {
                value: memory_value,
                ok: 0,
                calls: 0,
                last_addr: 0,
                last_index: 0,
                last_size: 0,
                last_zero_upper: 0,
            };
            let mut registers = full_guest_regs(case, fault_ordinal);
            let address = registers.gpr[usize::from(case.base)].wrapping_add(DISP as u64);
            registers.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            registers.vec_load_fn = vector_load_helper as usize as u64;
            let mut expected = registers;
            expected.exit_pc = PC;

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{level:?} {case:?}: fault");
            assert_eq!(context.calls, 1, "fault {level:?} {case:?}");
            assert_eq!(context.last_addr, address, "fault {level:?} {case:?}");
            assert_eq!(
                context.last_index,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX,
                "fault {level:?} {case:?}"
            );
            assert_eq!(
                context.last_size,
                case.width.bytes(),
                "fault {level:?} {case:?}"
            );
            assert_eq!(context.last_zero_upper, 1, "fault {level:?} {case:?}");
            faults += 1;
        }
    }

    assert!(expected_executions > 0);
    assert_eq!(successes, expected_executions);
    assert_eq!(faults, expected_executions);
    eprintln!(
        "executed {successes} successful and {faults} faulting native VEX variable-permute memory cases"
    );
}
