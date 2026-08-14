//! Exhaustive decode, admission, lowering, and native-state coverage for
//! register-form `RDRAND`/`RDSEED`.
//!
//! Intel SDM Order No. 325383-092US (June 2026), Vol. 2B defines the three
//! operand widths, zero-on-failure behavior, and exact status flags. Intel APX
//! Architecture Specification 355828-007US, Table 3.10 explicitly permits
//! EGPR destinations for both instructions.

use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint};
use crate::smir::ir::types::{
    ArchReg, BlockId, FunctionId, OpId, OpWidth, VReg, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    is_native_clobber_safe, x86_native_scalar_features_supported_excluding,
};
use crate::smir::lower::x86_64::{
    X86_64Lowerer, x86_random_shape_valid, x86_state_random_candidate, x86_state_random_valid,
};

const PC: u64 = 0x524E_4421;
const STATUS: u64 = 0x08D5;
const NON_STATUS: u64 = 0x2 | (1 << 9) | (1 << 10) | (1 << 21);
const LEVELS: [crate::smir::optimize::OptLevel; 3] = [
    crate::smir::optimize::OptLevel::O0,
    crate::smir::optimize::OptLevel::O1,
    crate::smir::optimize::OptLevel::O2,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Prefix {
    Legacy(Option<u8>),
    Rex2(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EncodingCase {
    seed: bool,
    operand_override: bool,
    prefix: Prefix,
    rm: u8,
}

impl EncodingCase {
    fn bytes(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(6);
        if self.operand_override {
            bytes.push(0x66);
        }
        match self.prefix {
            Prefix::Legacy(rex) => {
                bytes.extend(rex);
                bytes.extend([0x0F, 0xC7]);
            }
            Prefix::Rex2(payload) => bytes.extend([0xD5, payload, 0xC7]),
        }
        bytes.push(0xC0 | (if self.seed { 7 } else { 6 }) << 3 | self.rm);
        bytes
    }

    fn destination(self) -> u8 {
        match self.prefix {
            Prefix::Legacy(rex) => self.rm | (rex.unwrap_or(0) & 1) << 3,
            Prefix::Rex2(payload) => self.rm | (payload & 1) << 3 | payload & 0x10,
        }
    }

    fn width(self) -> OpWidth {
        let wide = match self.prefix {
            Prefix::Legacy(rex) => rex.is_some_and(|rex| rex & 8 != 0),
            Prefix::Rex2(payload) => payload & 8 != 0,
        };
        if wide {
            OpWidth::W64
        } else if self.operand_override {
            OpWidth::W16
        } else {
            OpWidth::W32
        }
    }
}

fn exhaustive_cases() -> Vec<EncodingCase> {
    let mut cases = Vec::with_capacity(4_640);
    for seed in [false, true] {
        for operand_override in [false, true] {
            for rex in [None].into_iter().chain((0x40..=0x4F).map(Some)) {
                for rm in 0u8..8 {
                    cases.push(EncodingCase {
                        seed,
                        operand_override,
                        prefix: Prefix::Legacy(rex),
                        rm,
                    });
                }
            }
            for payload in 0x80u8..=0xFF {
                for rm in 0u8..8 {
                    cases.push(EncodingCase {
                        seed,
                        operand_override,
                        prefix: Prefix::Rex2(payload),
                        rm,
                    });
                }
            }
        }
    }
    assert_eq!(cases.len(), 4_640);
    cases
}

fn gpr(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::gpr(index)))
}

fn state_backed(index: u8) -> bool {
    index >= 16 || matches!(index, 4 | 5)
}

fn function(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
        .x86_instruction_bytes
        .insert((BlockId(0), PC), X86InstructionBytes::new(bytes).unwrap());
    function
}

fn random_op(function: &SmirFunction) -> &SmirOp {
    let mut matches = function.blocks[0]
        .ops
        .iter()
        .filter(|op| matches!(op.kind, OpKind::X86Random { .. }));
    let op = matches.next().expect("one random-source operation");
    assert!(
        matches.next().is_none(),
        "duplicate random-source operation"
    );
    op
}

fn native_encoding(width: OpWidth, seed: bool, destination: u8) -> Vec<u8> {
    let mut expected = Vec::with_capacity(5);
    if width == OpWidth::W16 {
        expected.push(0x66);
    }
    if width == OpWidth::W64 || destination >= 8 {
        expected.push(0x40 | u8::from(width == OpWidth::W64) * 8 | u8::from(destination >= 8));
    }
    expected.extend([
        0x0F,
        0xC7,
        0xC0 | (if seed { 7 } else { 6 }) << 3 | destination & 7,
    ]);
    expected
}

#[test]
fn all_4640_legacy_and_rex2_register_images_lift_admit_and_lower_at_every_level() {
    let mut legacy_images = 0usize;
    let mut rex2_images = 0usize;
    let mut state_images = 0usize;
    let mut lowered = 0usize;

    for case in exhaustive_cases() {
        let bytes = case.bytes();
        let destination = case.destination();
        let width = case.width();
        let state = state_backed(destination);
        let function = function(&bytes);
        let op = random_op(&function);
        match &op.kind {
            OpKind::X86Random {
                dst,
                width: actual_width,
                seed,
            } => {
                assert_eq!(*dst, gpr(destination), "{case:?} {bytes:02X?}");
                assert_eq!(*actual_width, width, "{case:?} {bytes:02X?}");
                assert_eq!(*seed, case.seed, "{case:?} {bytes:02X?}");
            }
            _ => unreachable!("random_op returned a non-random operation"),
        }
        assert_eq!(op.x86_hint, None, "{case:?} {bytes:02X?}");
        assert!(x86_random_shape_valid(op), "{case:?} {bytes:02X?}");
        assert_eq!(x86_state_random_candidate(op), state, "{case:?}");
        assert_eq!(x86_state_random_valid(op), state, "{case:?}");
        assert!(is_native_clobber_safe(&function), "{case:?} {bytes:02X?}");

        match case.prefix {
            Prefix::Legacy(_) => legacy_images += 1,
            Prefix::Rex2(_) => {
                rex2_images += 1;
                assert!(
                    matches!(function.blocks[0].ops[0].kind, OpKind::X86RequireApx),
                    "REX2 must retain its dynamic APX guard: {case:?} {bytes:02X?}"
                );
            }
        }
        state_images += usize::from(state);

        for level in LEVELS {
            let mut optimized = function.clone();
            crate::smir::optimize::optimize_function(&mut optimized, level);
            assert!(is_native_clobber_safe(&optimized), "{level:?} {case:?}");
            let mut lowerer = X86_64Lowerer::new();
            lowerer.set_jit_fault_deopt_guards(true);
            lowerer
                .lower_function(&optimized)
                .unwrap_or_else(|error| panic!("{level:?} {case:?} {bytes:02X?}: {error:?}"));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{level:?} {case:?} {bytes:02X?}: {error:?}"));
            let host_destination = if state { 2 } else { destination };
            let expected = native_encoding(width, case.seed, host_destination);
            assert!(
                code.windows(expected.len())
                    .any(|window| window == expected),
                "{level:?} {case:?}: missing {expected:02X?} in {code:02X?}"
            );
            lowered += 1;
        }
    }

    assert_eq!(legacy_images, 544);
    assert_eq!(rex2_images, 4_096);
    assert_eq!(state_images, 2_376);
    assert_eq!(lowered, 4_640 * LEVELS.len());
}

fn single_op_function(op: SmirOp) -> SmirFunction {
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops.push(op);
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
}

#[test]
fn random_validator_exhausts_register_width_source_products_and_fails_closed() {
    let mut valid = 0usize;
    let mut state = 0usize;
    for seed in [false, true] {
        for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
            for destination in 0u8..32 {
                let op = SmirOp::new(
                    OpId(0),
                    PC,
                    OpKind::X86Random {
                        dst: gpr(destination),
                        width,
                        seed,
                    },
                );
                assert!(x86_random_shape_valid(&op));
                assert_eq!(x86_state_random_candidate(&op), state_backed(destination));
                assert_eq!(x86_state_random_valid(&op), state_backed(destination));
                assert!(is_native_clobber_safe(&single_op_function(op)));
                valid += 1;
                state += usize::from(state_backed(destination));
            }
        }
    }
    assert_eq!(valid, 192);
    assert_eq!(state, 108);

    let malformed = [
        SmirOp::new(
            OpId(0),
            PC,
            OpKind::X86Random {
                dst: VReg::Virtual(VirtualId(7)),
                width: OpWidth::W64,
                seed: false,
            },
        ),
        SmirOp::new(
            OpId(0),
            PC,
            OpKind::X86Random {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                width: OpWidth::W64,
                seed: false,
            },
        ),
        SmirOp::new(
            OpId(0),
            PC,
            OpKind::X86Random {
                dst: gpr(4),
                width: OpWidth::W8,
                seed: false,
            },
        ),
        SmirOp::new(
            OpId(0),
            PC,
            OpKind::X86Random {
                dst: gpr(16),
                width: OpWidth::W128,
                seed: true,
            },
        ),
    ];
    for op in malformed {
        assert!(!x86_random_shape_valid(&op), "{op:?}");
        assert!(!is_native_clobber_safe(&single_op_function(op)));
    }

    for destination in [4u8, 9, 16] {
        let mut op = SmirOp::new(
            OpId(0),
            PC,
            OpKind::X86Random {
                dst: gpr(destination),
                width: OpWidth::W64,
                seed: false,
            },
        );
        op.x86_hint = Some(X86OpHint::RexByteReg);
        assert!(!x86_random_shape_valid(&op));
        assert!(!is_native_clobber_safe(&single_op_function(op)));
    }
}

#[test]
fn random_feature_gate_tracks_the_exact_host_source() {
    for seed in [false, true] {
        let op = SmirOp::new(
            OpId(0),
            PC,
            OpKind::X86Random {
                dst: gpr(4),
                width: OpWidth::W64,
                seed,
            },
        );
        let function = single_op_function(op);
        #[cfg(target_arch = "x86_64")]
        assert_eq!(
            x86_native_scalar_features_supported_excluding(
                &function,
                &std::collections::HashMap::new()
            ),
            if seed {
                std::is_x86_feature_detected!("rdseed")
            } else {
                std::is_x86_feature_detected!("rdrand")
            }
        );
        #[cfg(not(target_arch = "x86_64"))]
        assert!(!x86_native_scalar_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ));
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_state_bridge_preserves_all_registers_widths_and_exact_status_flags() {
    use crate::smir::ir::FunctionBuilder;
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let supported = [
        std::is_x86_feature_detected!("rdrand"),
        std::is_x86_feature_detected!("rdseed"),
    ];
    let mut executed = 0usize;
    let mut observed_failures = 0usize;
    for (seed, supported) in [false, true].into_iter().zip(supported) {
        if !supported {
            continue;
        }
        for destination in [4u8, 5].into_iter().chain(16..=31) {
            for width in [OpWidth::W16, OpWidth::W32, OpWidth::W64] {
                for level in LEVELS {
                    let mut builder = FunctionBuilder::new(FunctionId(0), PC);
                    builder.push_op(
                        PC,
                        OpKind::X86Random {
                            dst: gpr(destination),
                            width,
                            seed,
                        },
                    );
                    builder.set_terminator(Terminator::Return { values: Vec::new() });
                    let mut function = builder.finish();
                    crate::smir::optimize::optimize_function(&mut function, level);

                    let mut lowerer = X86_64Lowerer::new();
                    let lowered = lowerer.lower_function(&function).unwrap_or_else(|error| {
                        panic!("{level:?} seed={seed} GPR{destination} {width:?}: {error:?}")
                    });
                    let code = lowerer.finalize().expect("finalize random state bridge");
                    let exec = ExecMem::new(&code).expect("map random state bridge");

                    let mut regs = GuestRegs::default();
                    regs.gpr = std::array::from_fn(|index| {
                        0xA5A5_5A5A_C3C3_3C3Cu64.rotate_left((index * 7) as u32)
                            ^ (executed as u64).wrapping_mul(0x0102_0408_1020_4081)
                    });
                    regs.rflags = NON_STATUS | STATUS;
                    regs.ac_flag = 1;
                    regs.exit_pc = 0xDEAD_BEEF_CAFE_BABE;
                    let before = regs.gpr;
                    exec.run(lowered.entry_offset, &mut regs);

                    let success = regs.rflags & 1 != 0;
                    observed_failures += usize::from(!success);
                    assert_eq!(regs.rflags & STATUS, u64::from(success));
                    assert_eq!(regs.rflags & !STATUS, NON_STATUS);
                    assert_eq!(regs.ac_flag, 1);
                    assert_eq!(regs.exit_pc, 0xDEAD_BEEF_CAFE_BABE);
                    for index in 0usize..32 {
                        if index != usize::from(destination) {
                            assert_eq!(
                                regs.gpr[index], before[index],
                                "{level:?} seed={seed} GPR{destination} {width:?}: GPR{index}"
                            );
                        }
                    }
                    let actual = regs.gpr[usize::from(destination)];
                    match width {
                        OpWidth::W16 => {
                            assert_eq!(actual >> 16, before[usize::from(destination)] >> 16);
                            if !success {
                                assert_eq!(actual & 0xFFFF, 0);
                            }
                        }
                        OpWidth::W32 => {
                            assert_eq!(actual >> 32, 0);
                            if !success {
                                assert_eq!(actual, 0);
                            }
                        }
                        OpWidth::W64 => {
                            if !success {
                                assert_eq!(actual, 0);
                            }
                        }
                        _ => unreachable!(),
                    }
                    executed += 1;
                }
            }
        }
    }
    assert_eq!(
        executed,
        supported.into_iter().filter(|supported| *supported).count() * 18 * 3 * LEVELS.len()
    );
    eprintln!("native random state bridge: {executed} executions, {observed_failures} CF=0");
}

#[cfg(target_arch = "x86_64")]
#[test]
fn rex2_random_apx_guard_is_dynamic_precise_and_noncommitting() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    // REX2.M=1, W=1, B4=1: RDRAND R16.
    let function = function(&[0xD5, 0x98, 0xC7, 0xF0]);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(true);
    let lowered = lowerer
        .lower_function(&function)
        .expect("lower APX-guarded RDRAND R16");
    let code = lowerer.finalize().expect("finalize APX-guarded RDRAND");
    let exec = ExecMem::new(&code).expect("map APX-guarded RDRAND");

    let execute = |enabled: bool| {
        let mut regs = GuestRegs::default();
        regs.gpr = std::array::from_fn(|index| 0xF000_0000_0000_0000 | index as u64);
        regs.rflags = NON_STATUS | STATUS;
        regs.ac_flag = 1;
        regs.apx_enabled = u64::from(enabled);
        regs.exit_pc = 0xDEAD_BEEF_CAFE_BABE;
        exec.run(lowered.entry_offset, &mut regs);
        regs
    };

    let disabled = execute(false);
    assert_eq!(disabled.exit_pc, PC);
    assert_eq!(
        disabled.gpr,
        std::array::from_fn(|index| 0xF000_0000_0000_0000 | index as u64)
    );
    assert_eq!(disabled.rflags, NON_STATUS | STATUS);
    assert_eq!(disabled.ac_flag, 1);

    if std::is_x86_feature_detected!("rdrand") {
        let enabled = execute(true);
        assert_eq!(enabled.exit_pc, 0xDEAD_BEEF_CAFE_BABE);
        assert_eq!(enabled.rflags & STATUS, u64::from(enabled.rflags & 1 != 0));
        assert_eq!(enabled.rflags & !STATUS, NON_STATUS);
        assert_eq!(enabled.ac_flag, 1);
        for index in (0usize..32).filter(|index| *index != 16) {
            assert_eq!(enabled.gpr[index], 0xF000_0000_0000_0000 | index as u64);
        }
        if enabled.rflags & 1 == 0 {
            assert_eq!(enabled.gpr[16], 0);
        }
    }
}
