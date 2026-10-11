//! A64 AdvSIMD program parity across direct execution, SMIR interpretation,
//! native lowering, and raw EL0 hardware execution.
//!
//! Encodings follow Arm DDI 0602, release 2026-03. The retained instruction
//! descriptions are under `docs/specifications/arm/a64-neon/`. Portable tests
//! compare the direct interpreter with SMIR at O0, O1, and O2. AArch64 hosts
//! with `smir-jit` additionally execute the original and lowered instructions.

use rax::isa::arm::{AArch64Config, AArch64Cpu, ArmCpu, CpuExit, FlatMemory};
use rax::smir::interpret::{BlockResult, SmirInterpreter};
use rax::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
use rax::smir::ir::types::{FunctionId, SourceArch};
use rax::smir::ir::{FunctionBuilder, SmirFunction, Terminator};
use rax::smir::lift::aarch64::Aarch64Lifter;
use rax::smir::lift::{LiftContext, SmirLifter};
use rax::smir::optimize::{OptLevel, optimize_function};

#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
use rax::smir::lower::runtime::{
    Aarch64GuestRegs, ExecMem, is_aarch64_native_clobber_safe_excluding,
};
#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
use rax::smir::lower::{SmirLowerer, aarch64::Aarch64Lowerer};

const CODE_ADDR: u64 = 0x1000;
const MEMORY_SIZE: usize = 0x10000;
const INSTRUCTION_BYTES: u64 = 4;

#[path = "aarch64_programs/comparisons.rs"]
mod comparisons;
#[path = "aarch64_programs/resize.rs"]
mod resize;
#[path = "aarch64_programs/resize_contracts.rs"]
mod resize_contracts;
#[path = "aarch64_programs/saturating.rs"]
mod saturating;
#[path = "aarch64_programs/saturating_contracts.rs"]
mod saturating_contracts;
#[path = "aarch64_programs/scalar_simd.rs"]
mod scalar_simd;
#[path = "aarch64_programs/scalar_simd_contracts.rs"]
mod scalar_simd_contracts;

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProgramState {
    x: [u64; 31],
    v: [[u64; 2]; 32],
    nzcv: u32,
    fpsr: u32,
}

fn initial_state(pattern: u64) -> ProgramState {
    ProgramState {
        x: std::array::from_fn(|index| {
            pattern.rotate_left(index as u32).wrapping_add(index as u64)
        }),
        v: std::array::from_fn(|index| {
            [
                pattern
                    .rotate_left(index as u32)
                    .wrapping_add(index as u64 * 0x0101_0101),
                (!pattern)
                    .rotate_right(index as u32)
                    .wrapping_sub(index as u64),
            ]
        }),
        nzcv: 0xa000_0000,
        // QC and unrelated sticky status bits must survive these operations.
        fpsr: match pattern {
            0 => 0,
            _ => 0x0800_0090,
        },
    }
}

fn program_bytes(program: &[u32]) -> Vec<u8> {
    program.iter().flat_map(|word| word.to_le_bytes()).collect()
}

fn lift_program(program: &[u32]) -> SmirFunction {
    let mut lifter = Aarch64Lifter::new();
    let mut context = LiftContext::new(SourceArch::Aarch64);
    let mut builder = FunctionBuilder::new(FunctionId(0), CODE_ADDR);
    for (index, word) in program.iter().enumerate() {
        let pc = CODE_ADDR + index as u64 * INSTRUCTION_BYTES;
        let result = lifter
            .lift_insn(pc, &word.to_le_bytes(), &mut context)
            .unwrap_or_else(|error| panic!("lift instruction #{index} {word:#010x}: {error}"));
        assert!(
            !result.control_flow.ends_block(),
            "unexpected interpreter frontier at instruction #{index}: {word:#010x}"
        );
        assert!(
            !result.ops.is_empty(),
            "missing semantic effect for instruction #{index}: {word:#010x}"
        );
        for op in result.ops {
            builder.push_op(pc, op.kind);
        }
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.finish()
}

fn execute_direct(program: &[u32], state: &ProgramState) -> ProgramState {
    let mut cpu = AArch64Cpu::new(
        AArch64Config::default(),
        Box::new(FlatMemory::new(0, MEMORY_SIZE)),
    );
    #[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
    cpu.set_jit_enabled(false);

    cpu.write_memory(CODE_ADDR, &program_bytes(program))
        .unwrap();
    cpu.set_pc(CODE_ADDR);
    for (index, value) in state.x.iter().enumerate() {
        cpu.set_x(index as u8, *value);
    }
    for (index, value) in state.v.iter().enumerate() {
        let bits = u128::from(value[0]) | (u128::from(value[1]) << 64);
        cpu.set_simd(index as u8, bits);
    }
    cpu.set_nzcv_bits((state.nzcv >> 28) as u8);
    cpu.set_fpsr(state.fpsr).unwrap();

    for (index, word) in program.iter().enumerate() {
        let exit = cpu
            .step()
            .unwrap_or_else(|error| panic!("direct instruction #{index} {word:#010x}: {error:?}"));
        assert!(
            matches!(exit, CpuExit::Continue),
            "unexpected direct exit at instruction #{index} {word:#010x}: {exit:?}"
        );
    }
    ProgramState {
        x: std::array::from_fn(|index| cpu.get_x(index as u8)),
        v: std::array::from_fn(|index| {
            let bits = cpu.get_simd(index as u8);
            [bits as u64, (bits >> 64) as u64]
        }),
        nzcv: u32::from(cpu.nzcv_bits()) << 28,
        fpsr: cpu.get_fpsr().unwrap(),
    }
}

fn execute_smir(function: &SmirFunction, state: &ProgramState) -> ProgramState {
    let mut context = SmirContext::new_aarch64();
    let ArchRegState::Aarch64(regs) = &mut context.arch_regs else {
        panic!("new_aarch64 created an unexpected register state");
    };
    regs.x = state.x;
    regs.v = state.v;
    regs.nzcv = state.nzcv;
    regs.fpsr = state.fpsr;
    context.flags.materialized = rax::smir::ir::flags::MaterializedFlags::from_nzcv(state.nzcv);

    let mut memory = rax::smir::ir::memory::FlatMemory::new(1);
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut memory, &function.blocks[0]);
    assert!(
        matches!(result, BlockResult::Exit(ExitReason::Return { .. })),
        "unexpected SMIR block exit: {result:?}"
    );

    let ArchRegState::Aarch64(regs) = context.arch_regs else {
        panic!("SMIR execution changed the register-state architecture");
    };
    ProgramState {
        x: regs.x,
        v: regs.v,
        nzcv: regs.nzcv,
        fpsr: regs.fpsr,
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
fn execute_native(code: &[u8], entry_offset: usize, state: &ProgramState) -> ProgramState {
    let mut regs = Aarch64GuestRegs::default();
    regs.x.copy_from_slice(&state.x);
    for (index, value) in state.v.iter().enumerate() {
        regs.v[2 * index..2 * index + 2].copy_from_slice(value);
    }
    regs.nzcv = u64::from(state.nzcv);
    regs.fpsr = u64::from(state.fpsr);

    let executable = ExecMem::new(code).unwrap();
    executable.run_aarch64_identity_fp(entry_offset, &mut regs);
    ProgramState {
        x: regs.x,
        v: std::array::from_fn(|index| [regs.v[2 * index], regs.v[2 * index + 1]]),
        nzcv: regs.nzcv as u32,
        fpsr: regs.fpsr as u32,
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
fn execute_raw_hardware(program: &[u32], state: &ProgramState) -> ProgramState {
    let mut code = program_bytes(program);
    code.extend_from_slice(&0xd65f_03c0u32.to_le_bytes()); // RET X30
    execute_native(&code, 0, state)
}

fn assert_program_parity(program: &[u32]) {
    let patterns = [0, u64::MAX, 0x8080_7f7f_ffff_0001, 0xaaaa_5555_0123_fedc];
    let states = patterns.map(initial_state);
    assert_program_parity_for_states(program, &states);
}

fn assert_program_parity_for_states(program: &[u32], states: &[ProgramState]) {
    let function = lift_program(program);
    let references: Vec<_> = states
        .iter()
        .map(|initial| execute_direct(program, initial))
        .collect();
    #[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
    for (initial, reference) in states.iter().zip(&references) {
        assert_eq!(
            execute_raw_hardware(program, initial),
            *reference,
            "raw hardware versus direct execution: {program:08x?}, initial={initial:?}"
        );
    }

    for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
        let mut optimized = function.clone();
        optimize_function(&mut optimized, level);
        for (initial, reference) in states.iter().zip(&references) {
            assert_eq!(
                execute_smir(&optimized, initial),
                *reference,
                "SMIR {level:?}: {program:08x?}, initial={initial:?}"
            );
        }

        #[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
        let (code, entry_offset) = {
            assert!(is_aarch64_native_clobber_safe_excluding(
                &optimized,
                &Default::default(),
                false,
            ));
            let mut lowerer = Aarch64Lowerer::new();
            let result = lowerer
                .lower_function(&optimized)
                .unwrap_or_else(|error| panic!("lower {program:08x?} at {level:?}: {error:?}"));
            (lowerer.finalize().unwrap(), result.entry_offset)
        };

        #[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
        for (initial, reference) in states.iter().zip(&references) {
            assert_eq!(
                execute_native(&code, entry_offset, initial),
                *reference,
                "native {level:?}: {program:08x?}, initial={initial:?}"
            );
        }
    }
}

fn repeated_lane(bits: u32, value: u64) -> [u64; 2] {
    let mask = (1u128 << bits) - 1;
    let mut vector = 0u128;
    for lane in 0..(128 / bits) {
        vector |= (u128::from(value) & mask) << (lane * bits);
    }
    [vector as u64, (vector >> 64) as u64]
}

#[test]
fn variable_shifts_exhaust_signed_byte_counts_and_ignore_upper_count_bits() {
    for size in 0..=3 {
        let bits = 8 << size;
        for q in 0..=1 {
            if size == 3 && q == 0 {
                continue;
            }
            for u in 0..=1 {
                for opcode in [0b01000, 0b01010] {
                    let states: Vec<_> = (0..=u8::MAX)
                        .flat_map(|count| {
                            [0, u64::MAX, 1u64 << (bits - 1), (1u64 << (bits - 1)) - 1].map(
                                |value| {
                                    let mut state = initial_state(value);
                                    state.v[1] = repeated_lane(bits, value);
                                    let amount = (0xa55a_ffff_a55a_ff00 & !0xff) | u64::from(count);
                                    state.v[2] = repeated_lane(bits, amount);
                                    state
                                },
                            )
                        })
                        .collect();
                    let instruction = encode_three_same(q, u, size, opcode, 0, 1, 2);
                    assert_program_parity_for_states(&[instruction], &states);
                    for (rd, rn, rm) in [(1, 1, 2), (2, 1, 2), (31, 31, 31)] {
                        assert_program_parity(&[encode_three_same(q, u, size, opcode, rd, rn, rm)]);
                    }
                }
            }
        }
    }
}

#[test]
fn rounded_immediate_shifts_cover_every_amount_and_accumulator_alias() {
    for size in 0..=3 {
        for q in 0..=1 {
            if size == 3 && q == 0 {
                continue;
            }
            for u in 0..=1 {
                for opcode in [0b00100, 0b00110] {
                    for amount in 1..=(8 << size) {
                        for (rd, rn) in [(0, 1), (1, 1), (31, 31)] {
                            assert_program_parity(&[encode_shift_immediate(
                                q, u, size, opcode, amount, rd, rn,
                            )]);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn pairwise_integer_arithmetic_preserves_source_order_and_aliases() {
    for opcode in [0b10100, 0b10101, 0b10111] {
        for u in 0..=1 {
            if opcode == 0b10111 && u == 1 {
                continue;
            }
            for q in 0..=1 {
                for size in 0..=3 {
                    if size == 3 && (q == 0 || opcode != 0b10111) {
                        continue;
                    }
                    for (rd, rn, rm) in [(0, 1, 2), (1, 1, 2), (2, 1, 2), (31, 31, 31)] {
                        assert_program_parity(&[encode_three_same(q, u, size, opcode, rd, rn, rm)]);
                    }
                }
            }
        }
    }
}

#[test]
fn bic_vector_logic_matches_hardware_for_both_widths_and_all_aliases() {
    assert_inverted_vector_logic_parity(0b01);
}

#[test]
fn orn_vector_logic_matches_hardware_for_both_widths_and_all_aliases() {
    assert_inverted_vector_logic_parity(0b11);
}

fn assert_inverted_vector_logic_parity(size: u32) {
    for q in 0..=1 {
        for (rd, rn, rm) in [(0, 1, 2), (1, 1, 2), (2, 1, 2), (31, 31, 31)] {
            assert_program_parity(&[encode_three_same(q, 0, size, 0b00011, rd, rn, rm)]);
        }
    }
}

#[test]
fn dependent_shift_and_pairwise_program() {
    let mut program = Vec::new();
    for q in 0..=1 {
        for size in 0..=2 {
            for u in 0..=1 {
                for opcode in [0b01000, 0b01010, 0b10100, 0b10101] {
                    let rd = (program.len() % 16) as u32;
                    program.push(encode_three_same(
                        q,
                        u,
                        size,
                        opcode,
                        rd,
                        (rd + 1) % 16,
                        (rd + 2) % 16,
                    ));
                }
                for opcode in [0b00100, 0b00110] {
                    let rd = (program.len() % 16) as u32;
                    program.push(encode_shift_immediate(
                        q,
                        u,
                        size,
                        opcode,
                        8 << size,
                        rd,
                        (rd + 1) % 16,
                    ));
                }
            }
        }
    }
    assert_program_parity(&program);
}

fn encode_three_same(q: u32, u: u32, size: u32, opcode: u32, rd: u32, rn: u32, rm: u32) -> u32 {
    0x0e20_0400
        | (q << 30)
        | (u << 29)
        | (size << 22)
        | (rm << 16)
        | (opcode << 11)
        | (rn << 5)
        | rd
}

fn encode_copy(q: u32, op: u32, size: u32, lane: u32, imm4: u32, rd: u32, rn: u32) -> u32 {
    let imm5 = (lane << (size + 1)) | (1 << size);
    0x0e00_0400 | (q << 30) | (op << 29) | (imm5 << 16) | (imm4 << 11) | (rn << 5) | rd
}

fn encode_shift_immediate(
    q: u32,
    u: u32,
    size: u32,
    opcode: u32,
    amount: u32,
    rd: u32,
    rn: u32,
) -> u32 {
    let bits = 8 << size;
    let encoded_amount = if opcode == 0b01010 {
        bits + amount
    } else {
        2 * bits - amount
    };
    0x0f00_0400 | (q << 30) | (u << 29) | (encoded_amount << 16) | (opcode << 11) | (rn << 5) | rd
}

#[test]
fn halving_minmax_absolute_difference_and_comparison_matrix() {
    // SHADD/UHADD, SRHADD/URHADD, SHSUB/UHSUB, CMGT/CMHI, CMGE/CMHS,
    // SMAX/UMAX, SMIN/UMIN, SABD/UABD, SABA/UABA; CMTST/CMEQ; MLA/MLS.
    let opcodes = [
        0b00000, 0b00010, 0b00100, 0b00110, 0b00111, 0b01100, 0b01101, 0b01110, 0b01111, 0b10001,
        0b10010,
    ];
    for opcode in opcodes {
        for u in 0..=1 {
            for q in 0..=1 {
                for size in 0..=2 {
                    for (rd, rn, rm) in [(0, 1, 2), (1, 1, 2), (2, 1, 2), (31, 31, 31)] {
                        let instruction = encode_three_same(q, u, size, opcode, rd, rn, rm);
                        assert_program_parity(&[instruction]);
                    }
                }
            }
        }
    }

    // Compare and bit-test instructions additionally permit the .2D form.
    for opcode in [0b00110, 0b00111, 0b10001] {
        for u in 0..=1 {
            assert_program_parity(&[encode_three_same(1, u, 3, opcode, 0, 1, 2)]);
        }
    }
}

#[test]
fn dependent_integer_vector_program() {
    let mut program = Vec::new();
    let opcodes = [
        0b00000, 0b00010, 0b00100, 0b01100, 0b01101, 0b01110, 0b01111, 0b10010, 0b00110, 0b00111,
        0b10001,
    ];
    for q in 0..=1 {
        for size in 0..=2 {
            for u in 0..=1 {
                for opcode in opcodes {
                    let rd = (program.len() % 16) as u32;
                    let rn = (rd + 1) % 16;
                    let rm = (rd + 2) % 16;
                    program.push(encode_three_same(q, u, size, opcode, rd, rn, rm));
                }
            }
        }
    }
    assert_program_parity(&program);
}

#[test]
fn copies_broadcasts_and_lane_boundaries() {
    for size in 0..=3 {
        let last_lane = (16 >> size) - 1;
        for lane in [0, last_lane] {
            // DUP (element), including a source lane above a .8B/.4H/.2S
            // destination's active width.
            for q in 0..=1 {
                if size != 3 || q == 1 {
                    assert_program_parity(&[encode_copy(q, 0, size, lane, 0b0000, 0, 1)]);
                }
            }

            // INS (element), with independent source and destination lanes.
            for (rd, rn) in [(0, 1), (1, 1), (31, 31)] {
                let source_lane = last_lane << size;
                assert_program_parity(&[encode_copy(1, 1, size, lane, source_lane, rd, rn)]);
            }

            // INS (general), UMOV, and the legal SMOV W/X forms.
            assert_program_parity(&[encode_copy(1, 0, size, lane, 0b0011, 0, 1)]);
            let umov_q = u32::from(size == 3);
            assert_program_parity(&[encode_copy(umov_q, 0, size, lane, 0b0111, 0, 1)]);
            if size < 3 {
                assert_program_parity(&[encode_copy(1, 0, size, lane, 0b0101, 0, 1)]);
                if size < 2 {
                    assert_program_parity(&[encode_copy(0, 0, size, lane, 0b0101, 0, 1)]);
                }
            }
        }

        // DUP (general), including WZR/XZR as the scalar source.
        for q in 0..=1 {
            if size != 3 || q == 1 {
                assert_program_parity(&[encode_copy(q, 0, size, 0, 0b0001, 0, 1)]);
                assert_program_parity(&[encode_copy(q, 0, size, 0, 0b0001, 0, 31)]);
            }
        }
    }
}

#[test]
fn immediate_shifts_include_full_element_width() {
    for size in 0..=3 {
        for q in 0..=1 {
            if size == 3 && q == 0 {
                continue;
            }
            let bits = 8 << size;
            for (opcode, u) in [
                (0b00000, 0),
                (0b00000, 1),
                (0b00010, 0),
                (0b00010, 1),
                (0b01010, 0),
            ] {
                let amounts = if opcode == 0b01010 {
                    [0, 1, bits - 1]
                } else {
                    [1, bits - 1, bits]
                };
                for amount in amounts {
                    for rd in [0, 1] {
                        let instruction = encode_shift_immediate(q, u, size, opcode, amount, rd, 1);
                        assert_program_parity(&[instruction]);
                    }
                }
            }
        }
    }
}

#[test]
fn reserved_vector_shapes_reject_before_native_execution() {
    let mut lifter = Aarch64Lifter::new();
    let mut context = LiftContext::new(SourceArch::Aarch64);
    let mut reserved = vec![
        encode_copy(0, 0, 3, 0, 0b0000, 0, 1),
        encode_copy(0, 0, 3, 0, 0b0001, 0, 1),
        encode_copy(0, 0, 0, 0, 0b0011, 0, 1),
        encode_copy(0, 1, 0, 0, 0b0000, 0, 1),
        encode_copy(0, 0, 2, 0, 0b0101, 0, 1),
        encode_copy(1, 0, 3, 0, 0b0101, 0, 1),
        encode_copy(1, 0, 2, 0, 0b0111, 0, 1),
        encode_copy(0, 0, 3, 0, 0b0111, 0, 1),
        0x0e00_0420, // imm5=0
    ];
    for opcode in [
        0b00000, 0b00010, 0b00100, 0b01100, 0b01101, 0b01110, 0b01111, 0b10010,
    ] {
        reserved.push(encode_three_same(1, 0, 3, opcode, 0, 1, 2));
    }
    for opcode in [0b00110, 0b00111, 0b10000, 0b10001] {
        reserved.push(encode_three_same(0, 0, 3, opcode, 0, 1, 2));
    }
    for raw in reserved {
        let result = lifter.lift_insn(CODE_ADDR, &raw.to_le_bytes(), &mut context);
        if let Ok(result) = result {
            assert!(
                result.control_flow.ends_block(),
                "reserved instruction {raw:#010x} admitted: {result:?}"
            );
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
fn assert_native_rejected(function: &SmirFunction) {
    assert!(
        !is_aarch64_native_clobber_safe_excluding(function, &Default::default(), false),
        "production admission gate accepted an unsupported native function: {function:?}"
    );
    assert!(
        Aarch64Lowerer::new().lower_function(function).is_err(),
        "native lowerer accepted an unsupported native function: {function:?}"
    );
}

#[test]
#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
fn native_comparisons_validate_registers_conditions_shapes_and_provenance() {
    use rax::smir::ir::ops::{OpKind, X86OpHint};
    use rax::smir::ir::types::{
        ArchReg, ArmReg, VReg, VecCmpCond, VecElementType, VirtualId, X86Reg,
    };

    let vector = |index| VReg::Arch(ArchReg::Arm(ArmReg::V(index)));
    let function_with = |kind| {
        let mut builder = FunctionBuilder::new(FunctionId(0), CODE_ADDR);
        builder.push_op(CODE_ADDR, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        builder.finish()
    };
    let comparison = |dst, src1, src2, elem, lanes, cond| OpKind::VCmp {
        dst,
        src1,
        src2,
        elem,
        lanes,
        cond,
    };

    let shapes = [
        (VecElementType::I8, 8),
        (VecElementType::I8, 16),
        (VecElementType::I16, 4),
        (VecElementType::I16, 8),
        (VecElementType::I32, 2),
        (VecElementType::I32, 4),
        (VecElementType::I64, 1),
        (VecElementType::I64, 2),
    ];
    let conditions = [
        VecCmpCond::Eq,
        VecCmpCond::Gt,
        VecCmpCond::Ge,
        VecCmpCond::Gtu,
        VecCmpCond::Geu,
        VecCmpCond::Lt,
        VecCmpCond::Le,
        VecCmpCond::Ltu,
        VecCmpCond::Leu,
    ];
    for (elem, lanes) in shapes {
        for cond in conditions {
            let kind = comparison(vector(0), vector(1), vector(31), elem, lanes, cond);
            assert!(!kind.is_jit_safe(), "generic whitelist admitted {kind:?}");
            let function = function_with(kind.clone());
            for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                let mut optimized = function.clone();
                optimize_function(&mut optimized, level);
                assert!(
                    is_aarch64_native_clobber_safe_excluding(
                        &optimized,
                        &Default::default(),
                        false,
                    ),
                    "AArch64 admission rejected {kind:?} at {level:?}"
                );

                let mut lowerer = Aarch64Lowerer::new();
                let result = lowerer.lower_function(&optimized).unwrap_or_else(|error| {
                    panic!("lower comparison {kind:?} at {level:?}: {error:?}")
                });
                let code = lowerer.finalize().unwrap();
                for pattern in [0, u64::MAX, 0x8080_7f7f_ffff_0001, 0xaaaa_5555_0123_fedc] {
                    let initial = initial_state(pattern);
                    let reference = execute_smir(&function, &initial);
                    assert_eq!(
                        execute_smir(&optimized, &initial),
                        reference,
                        "optimized comparison {kind:?} at {level:?}, pattern {pattern:#018x}"
                    );
                    assert_eq!(
                        execute_native(&code, result.entry_offset, &initial),
                        reference,
                        "native comparison {kind:?} at {level:?}, pattern {pattern:#018x}"
                    );
                }
            }
        }
    }

    for invalid in [
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        VReg::Arch(ArchReg::Arm(ArmReg::X(1))),
        vector(32),
        vector(255),
        VReg::Virtual(VirtualId(0)),
        VReg::Imm(1),
    ] {
        for (dst, src1, src2) in [
            (invalid, vector(1), vector(2)),
            (vector(0), invalid, vector(2)),
            (vector(0), vector(1), invalid),
        ] {
            let kind = comparison(dst, src1, src2, VecElementType::I8, 16, VecCmpCond::Eq);
            assert_native_rejected(&function_with(kind));
        }
    }
    for (elem, lanes) in [
        (VecElementType::I8, 0),
        (VecElementType::I8, 15),
        (VecElementType::I8, 32),
        (VecElementType::I64, 3),
        (VecElementType::F32, 4),
        (VecElementType::F64, 2),
    ] {
        let kind = comparison(vector(0), vector(1), vector(2), elem, lanes, VecCmpCond::Eq);
        assert_native_rejected(&function_with(kind));
    }
    for cond in [VecCmpCond::Ne, VecCmpCond::False, VecCmpCond::True] {
        let kind = comparison(
            vector(0),
            vector(1),
            vector(2),
            VecElementType::I8,
            16,
            cond,
        );
        assert_native_rejected(&function_with(kind));
    }

    let kind = comparison(
        vector(0),
        vector(1),
        vector(2),
        VecElementType::I8,
        16,
        VecCmpCond::Eq,
    );
    let mut hinted = function_with(kind);
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::XopVpcom);
    assert_native_rejected(&hinted);
}

#[test]
#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
fn fusion_temporaries_must_not_escape_to_later_operations_or_terminators() {
    use rax::smir::ir::ops::{OpKind, SmirOp};
    use rax::smir::ir::types::{ArchReg, ArmReg, OpId, VReg, VecElementType};

    let function = lift_program(&[encode_copy(1, 0, 0, 3, 0b0000, 0, 1)]);
    let temporary = function.blocks[0].ops[0].kind.dests()[0];
    let mut later_read = function.clone();
    later_read.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        CODE_ADDR + INSTRUCTION_BYTES,
        OpKind::VBroadcast {
            dst: VReg::Arch(ArchReg::Arm(ArmReg::V(3))),
            scalar: temporary,
            elem: VecElementType::I8,
            lanes: 16,
        },
    ));
    assert_native_rejected(&later_read);

    let mut terminator_read = function;
    terminator_read.blocks[0].terminator = Terminator::Return {
        values: vec![temporary],
    };
    assert_native_rejected(&terminator_read);
}

#[test]
#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
fn fusion_temporaries_must_not_escape_to_excluded_frontiers_or_phi_nodes() {
    use rax::smir::ir::types::{BlockId, VReg, VirtualId};
    use rax::smir::ir::{PhiNode, SmirBlock};

    let function = lift_program(&[encode_copy(1, 0, 0, 3, 0b0000, 0, 1)]);
    let temporary = function.blocks[0].ops[0].kind.dests()[0];
    let frontier_id = BlockId(1);
    let frontier_pc = CODE_ADDR + INSTRUCTION_BYTES;
    let mut frontier_read = function.clone();
    let mut frontier = SmirBlock::new(frontier_id, frontier_pc);
    frontier.terminator = Terminator::Return {
        values: vec![temporary],
    };
    frontier_read.add_block(frontier);
    let excluded = std::collections::HashMap::from([(frontier_id, frontier_pc)]);
    assert!(
        !is_aarch64_native_clobber_safe_excluding(&frontier_read, &excluded, false),
        "production admission gate ignored a temporary read at an excluded frontier"
    );
    assert_native_rejected(&frontier_read);

    let mut phi_read = function;
    let mut successor = SmirBlock::new(frontier_id, frontier_pc);
    successor.phis.push(PhiNode {
        dst: VReg::Virtual(VirtualId(100)),
        sources: vec![(phi_read.blocks[0].id, temporary)],
    });
    successor.terminator = Terminator::Return { values: vec![] };
    phi_read.add_block(successor);
    assert_native_rejected(&phi_read);
}

#[test]
#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
fn fusion_must_not_cross_guest_instruction_boundaries() {
    let mut function = lift_program(&[encode_copy(1, 0, 0, 3, 0b0000, 0, 1)]);
    function.blocks[0].ops[1].guest_pc += INSTRUCTION_BYTES;
    assert_native_rejected(&function);
}

#[test]
#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
fn pairwise_and_rounded_shift_fusions_require_private_operands_and_one_guest_pc() {
    use rax::smir::ir::ops::X86OpHint;
    use rax::smir::ir::types::VReg;

    let instructions = [
        encode_three_same(1, 0, 0, 0b10111, 0, 1, 2),
        encode_shift_immediate(1, 0, 0, 0b00100, 8, 0, 1),
        encode_shift_immediate(1, 1, 3, 0b00110, 64, 0, 1),
    ];
    for instruction in instructions {
        let function = lift_program(&[instruction]);
        for index in 0..function.blocks[0].ops.len() {
            let mut different_pc = function.clone();
            different_pc.blocks[0].ops[index].guest_pc += INSTRUCTION_BYTES;
            assert_native_rejected(&different_pc);

            let mut hinted = function.clone();
            hinted.blocks[0].ops[index].x86_hint = Some(X86OpHint::XopVpcom);
            assert_native_rejected(&hinted);

            for temporary in function.blocks[0].ops[index].kind.dests() {
                if matches!(temporary, VReg::Virtual(_)) {
                    let mut escape = function.clone();
                    escape.blocks[0].terminator = Terminator::Return {
                        values: vec![temporary],
                    };
                    assert_native_rejected(&escape);
                }
            }
        }
    }
}

#[test]
fn new_integer_families_reject_reserved_64_bit_arrangements() {
    let mut lifter = Aarch64Lifter::new();
    let mut assert_reserved = |invalid: u32| {
        let mut context = LiftContext::new(SourceArch::Aarch64);
        if let Ok(result) = lifter.lift_insn(CODE_ADDR, &invalid.to_le_bytes(), &mut context) {
            assert!(
                result.control_flow.ends_block(),
                "reserved instruction {invalid:#010x} admitted: {result:?}"
            );
        }
    };
    for opcode in [0b01000, 0b01010, 0b10100, 0b10101, 0b10111] {
        for u in 0..=1 {
            let invalid = encode_three_same(0, u, 3, opcode, 0, 1, 2);
            assert_reserved(invalid);
        }
    }
    for u in 0..=1 {
        for opcode in [0b00100, 0b00110] {
            let invalid = encode_shift_immediate(0, u, 3, opcode, 64, 0, 1);
            assert_reserved(invalid);
        }
    }
}

#[test]
#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
fn native_shift_and_inverted_logic_reject_invalid_operands_and_state_effects() {
    use rax::smir::ir::ops::{OpKind, X86OpHint};
    use rax::smir::ir::types::{ArchReg, ArmReg, VLaneOp, VReg, VecElementType, VirtualId, X86Reg};

    let vector = |index| VReg::Arch(ArchReg::Arm(ArmReg::V(index)));
    let function_with = |kind| {
        let mut builder = FunctionBuilder::new(FunctionId(0), CODE_ADDR);
        builder.push_op(CODE_ADDR, kind);
        builder.set_terminator(Terminator::Return { values: vec![] });
        builder.finish()
    };
    for lane_op in [
        VLaneOp::ShiftSigned8,
        VLaneOp::ShiftSigned8Rnd,
        VLaneOp::AndNot,
        VLaneOp::OrNot,
    ] {
        let lane = |dst, src1, src2, elem, lanes, set_ovf| OpKind::VLane {
            dst,
            src1,
            src2,
            elem,
            lanes,
            op: lane_op,
            signed: true,
            set_ovf,
        };
        let valid = lane(
            vector(0),
            vector(1),
            vector(31),
            VecElementType::I8,
            16,
            false,
        );
        assert!(!valid.is_jit_safe(), "generic whitelist admitted {valid:?}");
        let function = function_with(valid);
        assert!(is_aarch64_native_clobber_safe_excluding(
            &function,
            &Default::default(),
            false,
        ));
        assert!(Aarch64Lowerer::new().lower_function(&function).is_ok());

        for invalid in [
            VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            VReg::Arch(ArchReg::Arm(ArmReg::X(1))),
            vector(32),
            vector(255),
            VReg::Virtual(VirtualId(0)),
            VReg::Imm(0),
        ] {
            for (dst, src1, src2) in [
                (invalid, vector(1), vector(2)),
                (vector(0), invalid, vector(2)),
                (vector(0), vector(1), invalid),
            ] {
                let kind = lane(dst, src1, src2, VecElementType::I8, 16, false);
                assert_native_rejected(&function_with(kind));
            }
        }
        for lanes in [0, 15, 32] {
            let kind = lane(
                vector(0),
                vector(1),
                vector(2),
                VecElementType::I8,
                lanes,
                false,
            );
            assert_native_rejected(&function_with(kind));
        }
        let overflow = lane(
            vector(0),
            vector(1),
            vector(2),
            VecElementType::I8,
            16,
            true,
        );
        assert_native_rejected(&function_with(overflow));
        let mut hinted = function;
        hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::XopVpcom);
        assert_native_rejected(&hinted);

        if matches!(lane_op, VLaneOp::ShiftSigned8 | VLaneOp::ShiftSigned8Rnd) {
            for (elem, lanes) in [
                (VecElementType::I64, 3),
                (VecElementType::F32, 4),
                (VecElementType::F64, 2),
            ] {
                let kind = lane(vector(0), vector(1), vector(2), elem, lanes, false);
                assert_native_rejected(&function_with(kind));
            }
        }
    }
}
