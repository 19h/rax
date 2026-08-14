//! Exhaustive admission and lowering coverage for register-form, same-width
//! `MOVSX`/`MOVZX` word operations. Intel SDM Order No. 325383-092US
//! (June 2026), Vol. 2B defines both operations as extension plus no flag
//! changes; when source and destination are both 16 bits, both reduce to the
//! same partial-register copy.

use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint};
use crate::smir::ir::types::{
    ArchReg, BlockId, FunctionId, OpId, OpWidth, VReg, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::is_native_clobber_safe;
use crate::smir::lower::x86_64::{
    X86_64Lowerer, x86_state_backed_gpr_extend_candidate, x86_state_backed_gpr_extend_valid,
};

const PC: u64 = 0x4D4F_5658;
const LEVELS: [crate::smir::optimize::OptLevel; 3] = [
    crate::smir::optimize::OptLevel::O0,
    crate::smir::optimize::OptLevel::O1,
    crate::smir::optimize::OptLevel::O2,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Extension {
    Sign,
    Zero,
}

impl Extension {
    const ALL: [Self; 2] = [Self::Sign, Self::Zero];

    fn opcode(self) -> u8 {
        match self {
            Self::Sign => 0xBF,
            Self::Zero => 0xB7,
        }
    }

    fn op(self, dst: VReg, src: VReg) -> OpKind {
        match self {
            Self::Sign => OpKind::SignExtend {
                dst,
                src,
                from_width: OpWidth::W16,
                to_width: OpWidth::W16,
            },
            Self::Zero => OpKind::ZeroExtend {
                dst,
                src,
                from_width: OpWidth::W16,
                to_width: OpWidth::W16,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Prefix {
    Legacy(Option<u8>),
    Rex2(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EncodingCase {
    extension: Extension,
    prefix: Prefix,
    modrm: u8,
}

impl EncodingCase {
    fn bytes(self) -> Vec<u8> {
        let mut bytes = vec![0x66];
        match self.prefix {
            Prefix::Legacy(rex) => {
                bytes.extend(rex);
                bytes.extend([0x0F, self.extension.opcode(), self.modrm]);
            }
            Prefix::Rex2(payload) => {
                bytes.extend([0xD5, payload, self.extension.opcode(), self.modrm]);
            }
        }
        bytes
    }

    fn registers(self) -> (u8, u8) {
        let reg = (self.modrm >> 3) & 7;
        let rm = self.modrm & 7;
        match self.prefix {
            Prefix::Legacy(rex) => {
                let rex = rex.unwrap_or(0);
                (reg | ((rex & 0x04) << 1), rm | ((rex & 0x01) << 3))
            }
            Prefix::Rex2(payload) => (
                reg | ((payload & 0x04) << 1) | ((payload & 0x40) >> 2),
                rm | ((payload & 0x01) << 3) | (payload & 0x10),
            ),
        }
    }
}

fn state_backed(index: u8) -> bool {
    index >= 16 || matches!(index, 4 | 5)
}

fn exhaustive_cases() -> Vec<EncodingCase> {
    let mut cases = Vec::with_capacity(9_344);
    for extension in Extension::ALL {
        for rex in [None].into_iter().chain((0x40..=0x47).map(Some)) {
            for modrm in 0xC0..=0xFF {
                cases.push(EncodingCase {
                    extension,
                    prefix: Prefix::Legacy(rex),
                    modrm,
                });
            }
        }
        for payload in (0x80..=0xFF).filter(|payload| payload & 0x08 == 0) {
            for modrm in 0xC0..=0xFF {
                cases.push(EncodingCase {
                    extension,
                    prefix: Prefix::Rex2(payload),
                    modrm,
                });
            }
        }
    }
    assert_eq!(cases.len(), 9_344);
    cases
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

fn extension_op(function: &SmirFunction) -> &SmirOp {
    let mut matches = function.blocks[0].ops.iter().filter(|op| {
        matches!(
            op.kind,
            OpKind::SignExtend {
                from_width: OpWidth::W16,
                to_width: OpWidth::W16,
                ..
            } | OpKind::ZeroExtend {
                from_width: OpWidth::W16,
                to_width: OpWidth::W16,
                ..
            }
        )
    });
    let op = matches.next().expect("one same-width MOVX operation");
    assert!(
        matches.next().is_none(),
        "duplicate same-width MOVX operation"
    );
    op
}

fn assert_extension_op(op: &SmirOp, case: EncodingCase, dst: u8, src: u8, bytes: &[u8]) {
    let operands = match (&op.kind, case.extension) {
        (
            OpKind::SignExtend {
                dst,
                src,
                from_width: OpWidth::W16,
                to_width: OpWidth::W16,
            },
            Extension::Sign,
        )
        | (
            OpKind::ZeroExtend {
                dst,
                src,
                from_width: OpWidth::W16,
                to_width: OpWidth::W16,
            },
            Extension::Zero,
        ) => (*dst, *src),
        _ => panic!("unexpected graph for {case:?} {bytes:02X?}: {op:?}"),
    };
    assert_eq!(operands, (gpr(dst), gpr(src)), "{case:?} {bytes:02X?}");
}

fn gpr(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::gpr(index)))
}

#[test]
fn all_9344_legacy_and_rex2_register_images_lift_admit_and_lower_at_every_level() {
    let mut legacy_images = 0usize;
    let mut rex2_images = 0usize;
    let mut state_backed_images = 0usize;
    let mut lowered = 0usize;

    for case in exhaustive_cases() {
        let bytes = case.bytes();
        let (dst, src) = case.registers();
        let function = function(&bytes);
        let op = extension_op(&function);
        assert_extension_op(op, case, dst, src, &bytes);
        assert_eq!(op.x86_hint, None, "{case:?} {bytes:02X?}");

        let expected_state_backed = state_backed(dst) || state_backed(src);
        assert_eq!(
            x86_state_backed_gpr_extend_candidate(op),
            expected_state_backed,
            "candidate {case:?} {bytes:02X?}"
        );
        assert_eq!(
            x86_state_backed_gpr_extend_valid(op),
            expected_state_backed,
            "valid {case:?} {bytes:02X?}"
        );
        assert!(is_native_clobber_safe(&function), "{case:?} {bytes:02X?}");

        match case.prefix {
            Prefix::Legacy(_) => legacy_images += 1,
            Prefix::Rex2(_) => rex2_images += 1,
        }
        state_backed_images += usize::from(expected_state_backed);

        for level in LEVELS {
            let mut optimized = function.clone();
            crate::smir::optimize::optimize_function(&mut optimized, level);
            assert!(
                is_native_clobber_safe(&optimized),
                "{level:?} {case:?} {bytes:02X?}"
            );
            let mut lowerer = X86_64Lowerer::new();
            lowerer.set_jit_fault_deopt_guards(true);
            lowerer
                .lower_function(&optimized)
                .unwrap_or_else(|error| panic!("{level:?} {case:?} {bytes:02X?}: {error:?}"));
            let code = lowerer
                .finalize()
                .unwrap_or_else(|error| panic!("{level:?} {case:?} {bytes:02X?}: {error:?}"));
            if expected_state_backed && matches!(level, crate::smir::optimize::OptLevel::O0) {
                assert!(
                    code.windows(2).any(|window| window == [0x66, 0x8B]),
                    "{level:?} {case:?}: documented word load absent from {code:02X?}"
                );
                assert!(
                    !code
                        .windows(3)
                        .any(|window| window == [0x66, 0x0F, case.extension.opcode()]),
                    "{level:?} {case:?}: raw same-width MOVX leaked into {code:02X?}"
                );
            }
            lowered += 1;
        }
    }

    assert_eq!(legacy_images, 1_152);
    assert_eq!(rex2_images, 8_192);
    assert_eq!(state_backed_images, 6_920);
    assert_eq!(lowered, 9_344 * LEVELS.len());
}

#[test]
fn same_width_validator_exhausts_register_products_and_fails_closed() {
    let mut admitted = 0usize;
    for extension in Extension::ALL {
        for dst in 0u8..32 {
            for src in 0u8..32 {
                let op = SmirOp::new(OpId(0), PC, extension.op(gpr(dst), gpr(src)));
                let expected = state_backed(dst) || state_backed(src);
                assert_eq!(x86_state_backed_gpr_extend_candidate(&op), expected);
                assert_eq!(x86_state_backed_gpr_extend_valid(&op), expected);
                admitted += usize::from(expected);
            }
        }
    }
    assert_eq!(admitted, 1_656);

    let invalid = [
        SmirOp::new(
            OpId(0),
            PC,
            OpKind::SignExtend {
                dst: gpr(5),
                src: gpr(3),
                from_width: OpWidth::W32,
                to_width: OpWidth::W32,
            },
        ),
        SmirOp::new(
            OpId(0),
            PC,
            OpKind::ZeroExtend {
                dst: gpr(4),
                src: VReg::Virtual(VirtualId(7)),
                from_width: OpWidth::W16,
                to_width: OpWidth::W16,
            },
        ),
        SmirOp::new(
            OpId(0),
            PC,
            OpKind::SignExtend {
                dst: gpr(4),
                src: gpr(5),
                from_width: OpWidth::W16,
                to_width: OpWidth::W8,
            },
        ),
    ];
    for op in invalid {
        assert!(!x86_state_backed_gpr_extend_valid(&op), "{op:?}");
        assert!(
            !is_native_clobber_safe(&single_op_function(op.clone())),
            "{op:?}"
        );
    }

    for hint in [
        X86OpHint::RexByteReg,
        X86OpHint::LegacyHighByteReg,
        X86OpHint::Mulx,
    ] {
        let mut op = SmirOp::new(OpId(0), PC, Extension::Zero.op(gpr(4), gpr(3)));
        op.x86_hint = Some(hint);
        assert!(!x86_state_backed_gpr_extend_valid(&op), "{hint:?}");
        assert!(
            !is_native_clobber_safe(&single_op_function(op.clone())),
            "{hint:?}"
        );
    }
}

fn single_op_function(op: SmirOp) -> SmirFunction {
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops.push(op);
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_state_bridge_matches_word_copy_for_all_1656_state_backed_products() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let mut executed = 0usize;
    for extension in Extension::ALL {
        for dst in 0u8..32 {
            for src in 0u8..32 {
                if !(state_backed(dst) || state_backed(src)) {
                    continue;
                }
                for level in LEVELS {
                    let mut builder = crate::smir::ir::FunctionBuilder::new(FunctionId(0), PC);
                    builder.push_op(PC, extension.op(gpr(dst), gpr(src)));
                    builder.set_terminator(Terminator::Return { values: Vec::new() });
                    let mut function = builder.finish();
                    crate::smir::optimize::optimize_function(&mut function, level);

                    let mut lowerer = X86_64Lowerer::new();
                    let lowered = lowerer.lower_function(&function).unwrap_or_else(|error| {
                        panic!("{level:?} {extension:?} dst={dst} src={src}: {error:?}")
                    });
                    let code = lowerer.finalize().unwrap_or_else(|error| {
                        panic!("{level:?} {extension:?} dst={dst} src={src}: {error:?}")
                    });
                    assert!(
                        !code
                            .windows(3)
                            .any(|window| window == [0x66, 0x0F, extension.opcode()]),
                        "{level:?} {extension:?} dst={dst} src={src}: {code:02X?}"
                    );

                    let mut regs = GuestRegs::default();
                    regs.gpr = std::array::from_fn(|index| {
                        0x89AB_CDEF_0123_4567u64.rotate_left((index * 11) as u32)
                            ^ (executed as u64).wrapping_mul(0x0102_0408_1020_4081)
                    });
                    regs.rflags = 0x2 | 0x8D5;
                    let mut expected = regs.gpr;
                    expected[usize::from(dst)] = (expected[usize::from(dst)] & !0xFFFF)
                        | (regs.gpr[usize::from(src)] & 0xFFFF);

                    let exec = ExecMem::new(&code).expect("map same-width MOVX state bridge");
                    exec.run(lowered.entry_offset, &mut regs);
                    assert_eq!(
                        regs.gpr, expected,
                        "{level:?} {extension:?} dst={dst} src={src}"
                    );
                    assert_eq!(regs.rflags & 0x8D5, 0x8D5);
                    executed += 1;
                }
            }
        }
    }
    assert_eq!(executed, 1_656 * LEVELS.len());
}
