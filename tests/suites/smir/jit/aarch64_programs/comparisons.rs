//! Flag-only compare aliases, operand transformations, and native admission.

use rax::smir::ir::flags::FlagUpdate;
use rax::smir::ir::ops::OpKind;
use rax::smir::ir::types::{ArchReg, ArmReg, OpWidth, ShiftOp, SrcOperand, VReg};

#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
use rax::smir::ir::ops::SmirOp;
#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
use rax::smir::ir::types::OpId;

use super::{
    CODE_ADDR, ProgramState, assert_program_parity_for_states, execute_direct, execute_smir,
    initial_state, lift_program,
};

fn compare_states(bits: u32) -> Vec<ProgramState> {
    let sign = 1u64 << (bits - 1);
    let mask = u64::MAX >> (64 - bits);
    let values = [0, 1, 2, sign - 1, sign, sign + 1, mask - 1, mask];
    let mut states = Vec::new();
    for left in values {
        for right in values {
            let mut state = initial_state(left);
            // W-register reads must ignore the deliberately nonzero upper half.
            state.x[0] = left | (0xa55a_ffff_0000_0000 & !mask);
            state.x[1] = right | (0x5aa5_ffff_0000_0000 & !mask);
            state.x[29] = state.x[0];
            states.push(state);
        }
    }
    states
}

fn encode_compare_immediate(sf: u32, subtract: u32, sh: u32, immediate: u32, rn: u32) -> u32 {
    0x3100_001f | (sf << 31) | (subtract << 30) | (sh << 22) | (immediate << 10) | (rn << 5)
}

fn encode_compare_shifted(
    sf: u32,
    subtract: u32,
    shift: u32,
    amount: u32,
    rn: u32,
    rm: u32,
) -> u32 {
    0x2b00_001f
        | (sf << 31)
        | (subtract << 30)
        | (shift << 22)
        | (rm << 16)
        | (amount << 10)
        | (rn << 5)
}

fn encode_compare_extended(sf: u32, subtract: u32, option: u32, amount: u32, rn: u32) -> u32 {
    0x2b21_001f | (sf << 31) | (subtract << 30) | (option << 13) | (amount << 10) | (rn << 5)
}

fn encode_test_shifted(sf: u32, shift: u32, amount: u32, rn: u32, rm: u32) -> u32 {
    0x6a00_001f | (sf << 31) | (shift << 22) | (rm << 16) | (amount << 10) | (rn << 5)
}

#[test]
fn compare_immediates_cover_carry_overflow_and_shifted_boundaries() {
    for sf in 0..=1 {
        let states = compare_states(32 << sf);
        for subtract in 0..=1 {
            for sh in 0..=1 {
                for immediate in [0, 1, 2, 0x7ff, 0xfff] {
                    for rn in [0, 29] {
                        let instruction = encode_compare_immediate(sf, subtract, sh, immediate, rn);
                        assert_program_parity_for_states(&[instruction], &states);
                    }
                }
            }
        }
    }
}

#[test]
fn compare_shifted_registers_cover_every_amount_and_zero_register_alias() {
    for sf in 0..=1 {
        let bits = 32 << sf;
        let states = compare_states(bits);
        for subtract in 0..=1 {
            for shift in 0..=2 {
                for amount in 0..bits {
                    for (rn, rm) in [(0, 1), (0, 0), (31, 1), (0, 31), (31, 31)] {
                        let instruction =
                            encode_compare_shifted(sf, subtract, shift, amount, rn, rm);
                        assert_program_parity_for_states(&[instruction], &states);
                    }
                }
            }
        }
    }
}

#[test]
fn compare_extended_registers_cover_every_extension_and_legal_shift() {
    for sf in 0..=1 {
        let states = compare_states(32 << sf);
        for subtract in 0..=1 {
            for option in 0..=7 {
                for amount in 0..=4 {
                    for rn in [0, 29] {
                        let instruction = encode_compare_extended(sf, subtract, option, amount, rn);
                        assert_program_parity_for_states(&[instruction], &states);
                    }
                }
            }
        }
    }
}

#[test]
fn test_shifted_registers_cover_every_amount_and_clear_carry_overflow() {
    for sf in 0..=1 {
        let bits = 32 << sf;
        let states = compare_states(bits);
        for shift in 0..=3 {
            for amount in 0..bits {
                for (rn, rm) in [(0, 1), (0, 0), (31, 1), (0, 31), (31, 31)] {
                    let instruction = encode_test_shifted(sf, shift, amount, rn, rm);
                    assert_program_parity_for_states(&[instruction], &states);
                }
            }
        }
    }
}

#[test]
fn test_immediate_masks_cover_element_width_rotation_and_one_counts() {
    for sf in 0..=1 {
        let states = compare_states(32 << sf);
        for size in 1..=(5 + sf) {
            let element_bits = 1u32 << size;
            let n = u32::from(size == 6);
            // DecodeBitMasks encodes the element width with the leading bits
            // of imms; an all-ones element is reserved for logical immediates.
            let width_prefix = (!(element_bits * 2 - 1)) & 0x3f;
            for ones in [1, element_bits / 2, element_bits - 1] {
                for rotation in [0, 1, element_bits - 1] {
                    let imms = width_prefix | (ones - 1);
                    for rn in [0, 31] {
                        let instruction = 0x7200_001f
                            | (sf << 31)
                            | (n << 22)
                            | (rotation << 16)
                            | (imms << 10)
                            | (rn << 5);
                        assert_program_parity_for_states(&[instruction], &states);
                    }
                }
            }
        }
    }
}

#[test]
fn compare_aliases_have_no_register_destination() {
    let cases = [
        (encode_compare_immediate(1, 1, 0, 1, 0), "cmp"),
        (encode_compare_immediate(1, 0, 0, 1, 0), "cmn"),
        (encode_test_shifted(1, 0, 0, 0, 1), "test"),
    ];
    for (instruction, mnemonic) in cases {
        let function = lift_program(&[instruction]);
        assert_eq!(function.blocks[0].ops.len(), 1, "{mnemonic}");
        let kind = &function.blocks[0].ops[0].kind;
        assert!(kind.dests().is_empty(), "{mnemonic}: {kind:?}");
        match (mnemonic, kind) {
            ("cmp", OpKind::Cmp { .. })
            | ("cmn", OpKind::Cmn { .. })
            | ("test", OpKind::Test { .. }) => {}
            _ => panic!("unexpected flag-only operation: {kind:?}"),
        }
    }
}

#[test]
fn scalar_alu_metadata_retains_shifted_and_extended_source_registers() {
    use rax::smir::ir::types::ExtendOp;

    let left = VReg::Arch(ArchReg::Arm(ArmReg::X(0)));
    let right = VReg::Arch(ArchReg::Arm(ArmReg::X(28)));
    let sources = [
        SrcOperand::Reg(right),
        SrcOperand::Shifted {
            reg: right,
            shift: ShiftOp::Lsl,
            amount: 1,
        },
        SrcOperand::Extended {
            reg: right,
            extend: ExtendOp::Sxtw,
            shift: 4,
        },
    ];
    for src2 in sources {
        let cases = [
            OpKind::Cmp {
                src1: left,
                src2: src2.clone(),
                width: OpWidth::W64,
            },
            OpKind::Cmn {
                src1: left,
                src2: src2.clone(),
                width: OpWidth::W64,
            },
            OpKind::Test {
                src1: left,
                src2: src2.clone(),
                width: OpWidth::W64,
            },
            OpKind::Add {
                dst: left,
                src1: left,
                src2: src2.clone(),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
            OpKind::Sub {
                dst: left,
                src1: left,
                src2: src2.clone(),
                width: OpWidth::W64,
                flags: FlagUpdate::All,
            },
        ];
        for kind in cases {
            assert_eq!(kind.source_vregs(), vec![left, right], "{kind:?}");
        }
    }
}

#[test]
fn reserved_comparison_operand_encodings_end_at_the_guest_frontier() {
    use rax::smir::ir::types::SourceArch;
    use rax::smir::lift::{LiftContext, SmirLifter, aarch64::Aarch64Lifter};

    let instructions = [
        encode_compare_shifted(1, 0, 3, 0, 0, 1),
        encode_compare_shifted(0, 1, 0, 32, 0, 1),
        encode_compare_extended(1, 0, 2, 5, 0),
        encode_test_shifted(0, 0, 32, 0, 1),
        0x7240_001f, // W-form logical immediate reserves N=1.
        0xf240_fc1f, // Logical immediate reserves an all-ones element.
    ];
    for instruction in instructions {
        let mut lifter = Aarch64Lifter::new();
        let mut context = LiftContext::new(SourceArch::Aarch64);
        if let Ok(result) = lifter.lift_insn(CODE_ADDR, &instruction.to_le_bytes(), &mut context) {
            assert!(result.control_flow.ends_block(), "{instruction:#010x}");
        }
    }
}

#[test]
fn dependent_comparisons_preserve_final_flags_at_every_optimization_level() {
    let program = [
        encode_compare_immediate(1, 1, 1, 0xfff, 0),
        encode_compare_shifted(1, 0, 2, 63, 0, 1),
        encode_test_shifted(1, 3, 31, 0, 1),
        encode_compare_extended(1, 0, 6, 4, 0),
    ];
    assert_program_parity_for_states(&program, &compare_states(64));
    let function = lift_program(&program);
    assert_eq!(
        execute_direct(&program, &initial_state(0)),
        execute_smir(&function, &initial_state(0))
    );
}

#[test]
fn scalar_alu_word_shifts_use_the_low_32_bits_before_transforming() {
    let states = compare_states(32);
    for amount in [0, 1, 16, 31] {
        for shift in 0..=2 {
            for subtract in 0..=1 {
                let comparison = encode_compare_shifted(0, subtract, shift, amount, 0, 1);
                let instruction = (comparison & !0x1f) | 2; // ADDS/SUBS W2, W0, W1
                assert_program_parity_for_states(&[instruction], &states);
            }
        }
        for shift in 0..=3 {
            let test = encode_test_shifted(0, shift, amount, 0, 1);
            let instruction = (test & !0x1f) | 2; // ANDS W2, W0, W1
            assert_program_parity_for_states(&[instruction], &states);
        }
    }
}

#[test]
fn comparison_and_logical_flags_feed_adc_and_sbc_with_architectural_carry() {
    let program = [
        0x9a0b_014a, // ADC X10, X10, X11 consumes the initial PSTATE.C.
        0xeb01_001f, // CMP X0, X1
        0x9a03_0042, // ADC X2, X2, X3
        0xab01_001f, // CMN X0, X1
        0x9a05_0084, // ADC X4, X4, X5
        0xea01_001f, // TST X0, X1 clears PSTATE.C and PSTATE.V.
        0xba07_00c6, // ADCS X6, X6, X7
        0xfa09_0108, // SBCS X8, X8, X9 consumes the ADCS carry.
    ];
    let mut states = Vec::new();
    for state in compare_states(64) {
        for nzcv in 0..=15 {
            let mut initial = state.clone();
            initial.nzcv = nzcv << 28;
            states.push(initial);
        }
    }
    assert_program_parity_for_states(&program, &states);
}

fn assert_context_adc_preserves_nzcv(context: &mut rax::smir::ir::context::SmirContext, nzcv: u32) {
    use rax::smir::interpret::{BlockResult, SmirInterpreter};
    use rax::smir::ir::context::ExitReason;
    use rax::smir::ir::memory::FlatMemory;

    context.write_arch_reg(ArchReg::Arm(ArmReg::X(0)), 2);
    context.write_arch_reg(ArchReg::Arm(ArmReg::X(1)), 3);
    assert_eq!(
        context.read_arch_reg(ArchReg::Arm(ArmReg::Nzcv)),
        u64::from(nzcv)
    );
    assert_eq!(context.flags.materialized.to_nzcv(), nzcv);
    assert!(context.flags.lazy.is_none());

    let function = lift_program(&[0x9a01_0000]); // ADC X0, X0, X1
    let mut memory = FlatMemory::new(1);
    let result = SmirInterpreter::new().execute_block(context, &mut memory, &function.blocks[0]);
    assert!(matches!(
        result,
        BlockResult::Exit(ExitReason::Return { .. })
    ));

    let carry = u64::from((nzcv & (1 << 29)) != 0);
    assert_eq!(context.read_arch_reg(ArchReg::Arm(ArmReg::X(0))), 5 + carry);
    assert_eq!(context.read_arch_reg(ArchReg::Arm(ArmReg::X(1))), 3);
    assert_eq!(
        context.read_arch_reg(ArchReg::Arm(ArmReg::Nzcv)),
        u64::from(nzcv)
    );
    assert_eq!(context.flags.materialized.to_nzcv(), nzcv);
}

#[test]
fn fresh_arm_context_adc_uses_initial_zero_carry() {
    let mut context = rax::smir::ir::context::SmirContext::new_aarch64();
    assert_context_adc_preserves_nzcv(&mut context, 0);
}

#[test]
fn explicit_nzcv_writes_replace_pending_flags_before_adc() {
    for bits in 0..=15 {
        let mut context = rax::smir::ir::context::SmirContext::new_aarch64();
        context.flags.set_lazy_add(u64::MAX, 1, 0, OpWidth::W64);
        let nzcv = bits << 28;
        context.write_arch_reg(ArchReg::Arm(ArmReg::Nzcv), u64::from(nzcv));
        assert_context_adc_preserves_nzcv(&mut context, nzcv);
    }
}

#[test]
fn compare_immediate_and_extended_forms_read_sp_without_writing_it() {
    use rax::isa::arm::{AArch64Config, AArch64Cpu, ArmCpu, CpuExit, FlatMemory};
    use rax::smir::interpret::{BlockResult, SmirInterpreter};
    use rax::smir::ir::context::{ArchRegState, ExitReason, SmirContext};

    for sf in 0..=1 {
        for subtract in 0..=1 {
            for instruction in [
                encode_compare_immediate(sf, subtract, 0, 1, 31),
                encode_compare_immediate(sf, subtract, 1, 0xfff, 31),
                encode_compare_extended(sf, subtract, 6, 4, 31),
            ] {
                let function = lift_program(&[instruction]);
                #[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
                assert!(!super::is_aarch64_native_clobber_safe_excluding(
                    &function,
                    &Default::default(),
                    false,
                ));

                for state in compare_states(32 << sf) {
                    let mut cpu = AArch64Cpu::new(
                        AArch64Config::default(),
                        Box::new(FlatMemory::new(0, super::MEMORY_SIZE)),
                    );
                    #[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
                    cpu.set_jit_enabled(false);
                    cpu.write_memory(CODE_ADDR, &instruction.to_le_bytes())
                        .unwrap();
                    cpu.set_pc(CODE_ADDR);
                    cpu.set_current_sp(state.x[0]);
                    cpu.set_x(1, state.x[1]);
                    assert!(matches!(cpu.step().unwrap(), CpuExit::Continue));

                    let mut context = SmirContext::new_aarch64();
                    let ArchRegState::Aarch64(regs) = &mut context.arch_regs else {
                        panic!("expected AArch64 state");
                    };
                    regs.sp = state.x[0];
                    regs.x[1] = state.x[1];
                    let mut memory = rax::smir::ir::memory::FlatMemory::new(1);
                    let result = SmirInterpreter::new().execute_block(
                        &mut context,
                        &mut memory,
                        &function.blocks[0],
                    );
                    assert!(matches!(
                        result,
                        BlockResult::Exit(ExitReason::Return { .. })
                    ));
                    let ArchRegState::Aarch64(regs) = &context.arch_regs else {
                        panic!("execution changed the register architecture");
                    };
                    assert_eq!(
                        regs.nzcv,
                        u32::from(cpu.nzcv_bits()) << 28,
                        "{instruction:#010x}"
                    );
                    assert_eq!(regs.sp, state.x[0]);
                    assert_eq!(cpu.get_sp(), state.x[0]);
                    assert_eq!(regs.x[1], state.x[1]);
                    assert_eq!(cpu.get_x(1), state.x[1]);
                }
            }
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
#[test]
fn native_cmn_rejects_invalid_widths_registers_shifts_and_provenance() {
    use super::assert_native_rejected;
    use rax::smir::ir::ops::X86OpHint;
    use rax::smir::ir::types::{ExtendOp, X86Reg};

    let left = VReg::Arch(ArchReg::Arm(ArmReg::X(0)));
    let right = VReg::Arch(ArchReg::Arm(ArmReg::X(1)));
    let baseline = lift_program(&[encode_compare_immediate(1, 0, 0, 1, 0)]);
    let kind = &baseline.blocks[0].ops[0].kind;
    assert!(!kind.is_jit_safe());
    assert!(!baseline.blocks[0].ops[0].is_jit_safe());

    let invalid_registers = [
        VReg::virt(0),
        VReg::Arch(ArchReg::Arm(ArmReg::X(31))),
        VReg::Arch(ArchReg::Arm(ArmReg::X(255))),
        VReg::Arch(ArchReg::Arm(ArmReg::V(0))),
        VReg::Arch(ArchReg::Arm(ArmReg::Sp)),
        VReg::Arch(ArchReg::X86(X86Reg::Rax)),
        VReg::Imm(1),
    ];
    let mut invalid = Vec::new();
    for reg in invalid_registers {
        invalid.push(OpKind::Cmn {
            src1: reg,
            src2: SrcOperand::Reg(right),
            width: OpWidth::W64,
        });
        invalid.push(OpKind::Cmn {
            src1: left,
            src2: SrcOperand::Reg(reg),
            width: OpWidth::W64,
        });
    }
    for width in [OpWidth::W8, OpWidth::W16, OpWidth::W128] {
        invalid.push(OpKind::Cmn {
            src1: left,
            src2: SrcOperand::Reg(right),
            width,
        });
    }
    for src2 in [
        SrcOperand::Shifted {
            reg: right,
            shift: ShiftOp::Ror,
            amount: 1,
        },
        SrcOperand::Shifted {
            reg: right,
            shift: ShiftOp::Rrx,
            amount: 1,
        },
        SrcOperand::Shifted {
            reg: right,
            shift: ShiftOp::Lsl,
            amount: 64,
        },
        SrcOperand::Extended {
            reg: right,
            extend: ExtendOp::Sxtw,
            shift: 5,
        },
        SrcOperand::Imm(-1),
        SrcOperand::Imm(0x1001),
        SrcOperand::Imm64(0x1000000),
    ] {
        invalid.push(OpKind::Cmn {
            src1: left,
            src2,
            width: OpWidth::W64,
        });
    }
    for kind in invalid {
        let mut function = baseline.clone();
        function.blocks[0].ops[0] = SmirOp::new(OpId(0), CODE_ADDR, kind);
        assert_native_rejected(&function);
    }
    let mut function = baseline;
    function.blocks[0].ops[0].x86_hint = Some(X86OpHint::ShiftGroup6);
    assert_native_rejected(&function);
}

#[cfg(all(feature = "smir-jit", target_arch = "aarch64"))]
#[test]
fn native_flag_only_operations_reject_reserved_host_register_sources() {
    use super::assert_native_rejected;
    use rax::smir::ir::types::ExtendOp;

    let left = VReg::Arch(ArchReg::Arm(ArmReg::X(0)));
    for index in [18, 28, 30] {
        let right = VReg::Arch(ArchReg::Arm(ArmReg::X(index)));
        for src2 in [
            SrcOperand::Shifted {
                reg: right,
                shift: ShiftOp::Lsl,
                amount: 1,
            },
            SrcOperand::Extended {
                reg: right,
                extend: ExtendOp::Sxtw,
                shift: 4,
            },
        ] {
            for kind in [
                OpKind::Cmp {
                    src1: left,
                    src2: src2.clone(),
                    width: OpWidth::W64,
                },
                OpKind::Cmn {
                    src1: left,
                    src2: src2.clone(),
                    width: OpWidth::W64,
                },
                OpKind::Test {
                    src1: left,
                    src2: src2.clone(),
                    width: OpWidth::W64,
                },
            ] {
                let mut function = lift_program(&[encode_compare_immediate(1, 1, 0, 1, 0)]);
                function.blocks[0].ops[0] = SmirOp::new(OpId(0), CODE_ADDR, kind);
                // The lowerer can encode these sources, but the production
                // trampoline gate must reject their host-owned live values.
                assert!(!super::is_aarch64_native_clobber_safe_excluding(
                    &function,
                    &Default::default(),
                    false,
                ));
                if matches!(function.blocks[0].ops[0].kind, OpKind::Cmn { .. }) {
                    assert_native_rejected(&function);
                }
            }
        }
    }
}
