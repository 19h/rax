//! Exact helper-backed VEX/APX `MULX` memory-source coverage.

use super::*;
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint};
use crate::smir::ir::types::{
    Address, ArchReg, BlockId, DispSize, FunctionId, MemWidth, OpId, OpWidth, SignExtend,
    SrcOperand, VReg, VirtualId, X86Reg,
};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::runtime::{
    is_native_clobber_safe_excluding, is_x86_aarch64_native_clobber_safe_excluding,
    x86_native_scalar_features_supported_excluding,
};
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;

const PC: u64 = 0xB240;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncodingKind {
    Vex,
    Apx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MemoryMulxCase {
    encoding: EncodingKind,
    width: OpWidth,
    dst_lo: u8,
    dst_hi: u8,
}

impl MemoryMulxCase {
    /// Encode `MULX dst_hi,dst_lo,[RBX]`. Destination fields are exhaustive;
    /// RBX supplies one stable helper-backed address with no SIB tail.
    fn bytes(self) -> Vec<u8> {
        match self.encoding {
            EncodingKind::Vex => {
                assert!(self.dst_lo < 16 && self.dst_hi < 16);
                let mut p0 = 0xE2; // X'=B'=1, map 0F38.
                if self.dst_hi >= 8 {
                    p0 &= !0x80;
                }
                vec![
                    0xC4,
                    p0,
                    (u8::from(self.width == OpWidth::W64) << 7)
                        | (((!self.dst_lo) & 0x0F) << 3)
                        | 0x03,
                    0xF6,
                    ((self.dst_hi & 7) << 3) | 3,
                ]
            }
            EncodingKind::Apx => {
                assert!(self.dst_lo < 32 && self.dst_hi < 32);
                let mut p0 = 0x02 | 0x40 | 0x20; // map 2, X/B encode legacy RBX.
                if self.dst_hi & 8 == 0 {
                    p0 |= 0x80;
                }
                if self.dst_hi & 16 == 0 {
                    p0 |= 0x10;
                }
                vec![
                    0x62,
                    p0,
                    (u8::from(self.width == OpWidth::W64) << 7)
                        | (((!self.dst_lo) & 0x0F) << 3)
                        | 0x07, // X4=1; mandatory F2.
                    if self.dst_lo < 16 { 0x08 } else { 0x00 },
                    0xF6,
                    ((self.dst_hi & 7) << 3) | 3,
                ]
            }
        }
    }

    fn needs_state_bridge(self) -> bool {
        [self.dst_lo, self.dst_hi]
            .into_iter()
            .any(|index| index >= 16 || matches!(index, 4 | 5))
    }
}

fn x86(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::gpr(index)))
}

fn lift_case(case: MemoryMulxCase) -> SmirFunction {
    let bytes = case.bytes();
    lift_bytes(case, &bytes, &Address::Direct(x86(3)))
}

fn lift_bytes(case: MemoryMulxCase, bytes: &[u8], expected_addr: &Address) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{case:?} {bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{case:?}");
    assert_exact_pair(&result.ops, case, expected_addr);

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
}

fn assert_exact_pair(ops: &[SmirOp], case: MemoryMulxCase, expected_addr: &Address) {
    let ops = match case.encoding {
        EncodingKind::Vex => {
            assert!(
                !matches!(
                    ops.first(),
                    Some(SmirOp {
                        kind: OpKind::X86RequireApx,
                        ..
                    })
                ),
                "{case:?}: VEX form has an APX requirement"
            );
            ops
        }
        EncodingKind::Apx => {
            assert!(
                matches!(
                    ops.first(),
                    Some(SmirOp {
                        kind: OpKind::X86RequireApx,
                        ..
                    })
                ),
                "{case:?}: APX form lacks its dynamic requirement"
            );
            &ops[1..]
        }
    };
    let [load, consumer] = ops else {
        panic!("{case:?}: expected Load + MULX, got {ops:?}")
    };
    let temporary = match &load.kind {
        OpKind::Load {
            dst: temporary @ VReg::Virtual(_),
            addr,
            width,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(addr, expected_addr, "{case:?}");
            assert_eq!(
                *width,
                if case.width == OpWidth::W64 {
                    MemWidth::B8
                } else {
                    MemWidth::B4
                },
                "{case:?}"
            );
            *temporary
        }
        other => panic!("{case:?}: expected exact load, got {other:?}"),
    };
    assert_eq!(consumer.guest_pc, load.guest_pc, "{case:?}");
    assert_eq!(consumer.x86_hint, Some(X86OpHint::Mulx), "{case:?}");
    assert!(matches!(
        &consumer.kind,
        OpKind::MulU {
            dst_lo,
            dst_hi: Some(dst_hi),
            src1,
            src2: SrcOperand::Reg(src2),
            width,
            flags: FlagUpdate::None,
        } if *dst_lo == x86(case.dst_lo)
            && *dst_hi == x86(case.dst_hi)
            && *src1 == x86(2)
            && *src2 == temporary
            && *width == case.width
    ));
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn lower(function: &SmirFunction) -> (Vec<u8>, usize) {
    assert!(is_native_clobber_safe_excluding(
        function,
        &std::collections::HashMap::new(),
        true,
    ));
    assert!(!is_native_clobber_safe_excluding(
        function,
        &std::collections::HashMap::new(),
        false,
    ));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        function,
        &std::collections::HashMap::new(),
    ));

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_jit_fault_deopt_guards(true);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("helper-backed MULX lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer.finalize().expect("finalize helper-backed MULX"),
        result.entry_offset,
    )
}

#[test]
fn all_2560_vex_apx_memory_destination_encodings_are_admitted_and_lowerable() {
    let mut lifted = 0usize;
    let mut lowered = 0usize;
    for (encoding, count) in [(EncodingKind::Vex, 16u8), (EncodingKind::Apx, 32u8)] {
        for width in [OpWidth::W32, OpWidth::W64] {
            for dst_lo in 0..count {
                for dst_hi in 0..count {
                    let case = MemoryMulxCase {
                        encoding,
                        width,
                        dst_lo,
                        dst_hi,
                    };
                    let function = lift_case(case);
                    lifted += 1;
                    for level in LEVELS {
                        let function = optimize(function.clone(), level);
                        assert_exact_pair(&function.blocks[0].ops, case, &Address::Direct(x86(3)));
                        let (code, _) = lower(&function);
                        assert!(!code.is_empty(), "{level:?} {case:?}");
                        lowered += 1;
                    }
                }
            }
        }
    }
    assert_eq!(lifted, 2 * 16 * 16 + 2 * 32 * 32);
    assert_eq!(lowered, lifted * LEVELS.len());
}

fn vex_sib_bytes(case: MemoryMulxCase, base: u8, index: u8, scale: u8) -> Vec<u8> {
    assert_eq!(case.encoding, EncodingKind::Vex);
    assert!(base < 16 && index < 16 && index != 4);
    let mut p0 = 0x02;
    if case.dst_hi & 8 == 0 {
        p0 |= 0x80;
    }
    if index & 8 == 0 {
        p0 |= 0x40;
    }
    if base & 8 == 0 {
        p0 |= 0x20;
    }
    vec![
        0xC4,
        p0,
        (u8::from(case.width == OpWidth::W64) << 7) | (((!case.dst_lo) & 0x0F) << 3) | 0x03,
        0xF6,
        0x40 | ((case.dst_hi & 7) << 3) | 4,
        ((scale.trailing_zeros() as u8) << 6) | ((index & 7) << 3) | (base & 7),
        0x80,
    ]
}

fn apx_sib_bytes(case: MemoryMulxCase, base: u8, index: u8, scale: u8) -> Vec<u8> {
    assert_eq!(case.encoding, EncodingKind::Apx);
    assert!(base < 32 && index < 32 && index != 4);
    let mut p0 = 0x02;
    if case.dst_hi & 8 == 0 {
        p0 |= 0x80;
    }
    if index & 8 == 0 {
        p0 |= 0x40;
    }
    if base & 8 == 0 {
        p0 |= 0x20;
    }
    if case.dst_hi & 16 == 0 {
        p0 |= 0x10;
    }
    if base & 16 != 0 {
        p0 |= 0x08;
    }
    vec![
        0x62,
        p0,
        (u8::from(case.width == OpWidth::W64) << 7)
            | (((!case.dst_lo) & 0x0F) << 3)
            | (u8::from(index < 16) << 2)
            | 0x03,
        if case.dst_lo < 16 { 0x08 } else { 0x00 },
        0xF6,
        0x40 | ((case.dst_hi & 7) << 3) | 4,
        ((scale.trailing_zeros() as u8) << 6) | ((index & 7) << 3) | (base & 7),
        0x80,
    ]
}

#[test]
fn all_9856_vex_apx_base_index_scale_encodings_lift_and_lower_exactly() {
    let mut count = 0usize;
    for width in [OpWidth::W32, OpWidth::W64] {
        let vex = MemoryMulxCase {
            encoding: EncodingKind::Vex,
            width,
            dst_lo: 9,
            dst_hi: 8,
        };
        for base in 0..16 {
            for index in (0..16).filter(|index| *index != 4) {
                for scale in [1, 2, 4, 8] {
                    let expected = Address::BaseIndexScale {
                        base: Some(x86(base)),
                        index: x86(index),
                        scale,
                        disp: -128,
                        disp_size: DispSize::Disp8,
                    };
                    let function =
                        lift_bytes(vex, &vex_sib_bytes(vex, base, index, scale), &expected);
                    lower(&function);
                    count += 1;
                }
            }
        }

        let apx = MemoryMulxCase {
            encoding: EncodingKind::Apx,
            width,
            dst_lo: 19,
            dst_hi: 20,
        };
        for base in 0..32 {
            for index in (0..32).filter(|index| *index != 4) {
                for scale in [1, 2, 4, 8] {
                    let expected = Address::BaseIndexScale {
                        base: Some(x86(base)),
                        index: x86(index),
                        scale,
                        disp: -128,
                        disp_size: DispSize::Disp8,
                    };
                    let function =
                        lift_bytes(apx, &apx_sib_bytes(apx, base, index, scale), &expected);
                    lower(&function);
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 2 * (16 * 15 * 4 + 32 * 31 * 4));
}

#[test]
fn memory_mulx_lifts_rip_addr32_segment_no_base_and_egpr_addresses_exactly() {
    let vex = MemoryMulxCase {
        encoding: EncodingKind::Vex,
        width: OpWidth::W64,
        dst_lo: 1,
        dst_hi: 0,
    };
    let cases = [
        (
            "RIP-relative",
            vex,
            vec![0xC4, 0xE2, 0xF3, 0xF6, 0x05, 0xFC, 0xFF, 0xFF, 0xFF],
            Address::PcRel {
                offset: -4,
                disp_size: DispSize::Disp32,
                base: Some(PC + 9),
            },
        ),
        (
            "addr32",
            vex,
            vec![0x67, 0xC4, 0xE2, 0xF3, 0xF6, 0x44, 0x8B, 0x20],
            Address::X86Addr32(Box::new(Address::BaseIndexScale {
                base: Some(x86(3)),
                index: x86(1),
                scale: 4,
                disp: 0x20,
                disp_size: DispSize::Disp8,
            })),
        ),
        (
            "FS-relative",
            vex,
            vec![0x64, 0xC4, 0xE2, 0xF3, 0xF6, 0x44, 0x8B, 0x20],
            Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                base: Some(x86(3)),
                index: Some(x86(1)),
                scale: 4,
                disp: 0x20,
            },
        ),
        (
            "SIB no base",
            vex,
            vec![0xC4, 0xE2, 0xF3, 0xF6, 0x04, 0x8D, 0x78, 0x56, 0x34, 0x12],
            Address::BaseIndexScale {
                base: None,
                index: x86(1),
                scale: 4,
                disp: 0x1234_5678,
                disp_size: DispSize::Disp32,
            },
        ),
        (
            "APX EGPR SIB",
            MemoryMulxCase {
                encoding: EncodingKind::Apx,
                width: OpWidth::W64,
                dst_lo: 19,
                dst_hi: 20,
            },
            vec![0x62, 0xEA, 0xE3, 0x00, 0xF6, 0x64, 0x91, 0x20],
            Address::BaseIndexScale {
                base: Some(x86(17)),
                index: x86(18),
                scale: 4,
                disp: 0x20,
                disp_size: DispSize::Disp8,
            },
        ),
    ];

    for (name, case, bytes, expected) in cases {
        let function = lift_bytes(case, &bytes, &expected);
        let (code, _) = lower(&function);
        assert!(!code.is_empty(), "{name}");
    }
}

#[test]
fn memory_mulx_emits_independently_decoded_direct_and_state_backed_cores() {
    // LLVM 23 independently decodes these byte sequences as:
    //   C4 E2 F3 F6 3C 24  -> mulx rdi, rcx, qword ptr [rsp]
    //   C4 62 7B F6 04 24  -> mulx r8d, eax, dword ptr [rsp]
    //   C4 C2 F3 F6 F8     -> mulx rdi, rcx, r8
    //   C4 C2 73 F6 F8     -> mulx edi, ecx, r8d
    for (case, expected) in [
        (
            MemoryMulxCase {
                encoding: EncodingKind::Vex,
                width: OpWidth::W64,
                dst_lo: 1,
                dst_hi: 7,
            },
            &[0xC4, 0xE2, 0xF3, 0xF6, 0x3C, 0x24][..],
        ),
        (
            MemoryMulxCase {
                encoding: EncodingKind::Vex,
                width: OpWidth::W32,
                dst_lo: 0,
                dst_hi: 8,
            },
            &[0xC4, 0x62, 0x7B, 0xF6, 0x04, 0x24][..],
        ),
        (
            MemoryMulxCase {
                encoding: EncodingKind::Apx,
                width: OpWidth::W64,
                dst_lo: 16,
                dst_hi: 31,
            },
            &[0xC4, 0xC2, 0xF3, 0xF6, 0xF8][..],
        ),
        (
            MemoryMulxCase {
                encoding: EncodingKind::Apx,
                width: OpWidth::W32,
                dst_lo: 4,
                dst_hi: 5,
            },
            &[0xC4, 0xC2, 0x73, 0xF6, 0xF8][..],
        ),
    ] {
        assert_eq!(
            case.needs_state_bridge(),
            case.dst_lo >= 16
                || case.dst_hi >= 16
                || matches!(case.dst_lo, 4 | 5)
                || matches!(case.dst_hi, 4 | 5),
            "{case:?}"
        );
        let (code, _) = lower(&lift_case(case));
        assert!(
            code.windows(expected.len())
                .any(|window| window == expected),
            "{case:?}: missing {expected:02X?} in {code:02X?}"
        );
    }
}

fn manual_function(
    addr: Address,
    width: OpWidth,
    dst_lo: VReg,
    dst_hi: Option<VReg>,
) -> SmirFunction {
    let temporary = VReg::Virtual(VirtualId(0xB2));
    let mem_width = match width {
        OpWidth::W32 => MemWidth::B4,
        OpWidth::W64 => MemWidth::B8,
        OpWidth::W8 => MemWidth::B1,
        OpWidth::W16 => MemWidth::B2,
        OpWidth::W128 => MemWidth::B16,
    };
    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = vec![
        SmirOp::new(
            OpId(0),
            PC,
            OpKind::Load {
                dst: temporary,
                addr,
                width: mem_width,
                sign: SignExtend::Zero,
            },
        ),
        SmirOp::with_hint(
            OpId(1),
            PC,
            OpKind::MulU {
                dst_lo,
                dst_hi,
                src1: x86(2),
                src2: SrcOperand::Reg(temporary),
                width,
                flags: FlagUpdate::None,
            },
            X86OpHint::Mulx,
        ),
    ];
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function
}

fn assert_address_lowerable(addr: Address, ordinal: usize) {
    let width = if ordinal & 1 == 0 {
        OpWidth::W32
    } else {
        OpWidth::W64
    };
    let function = manual_function(addr, width, x86(16), Some(x86(31)));
    let (code, _) = lower(&function);
    assert!(!code.is_empty(), "address shape {ordinal}");
}

#[test]
fn all_15517_state_backed_address_shapes_reach_exact_memory_mulx_lowering() {
    let displacements = [-128, 0, 127];
    let mut count = 0usize;

    for base in 0..32 {
        assert_address_lowerable(Address::Direct(x86(base)), count);
        count += 1;
        for offset in [
            i64::from(i32::MIN),
            -129,
            -128,
            -1,
            0,
            127,
            128,
            i64::from(i32::MAX),
        ] {
            assert_address_lowerable(
                Address::BaseOffset {
                    base: x86(base),
                    offset,
                    disp_size: DispSize::Auto,
                },
                count,
            );
            count += 1;
        }
    }

    for base in 0..=32 {
        for index in 0..32 {
            for scale in [1, 2, 4, 8] {
                for disp in displacements {
                    assert_address_lowerable(
                        Address::BaseIndexScale {
                            base: (base < 32).then(|| x86(base)),
                            index: x86(index),
                            scale,
                            disp,
                            disp_size: DispSize::Auto,
                        },
                        count,
                    );
                    count += 1;
                }
            }
        }
    }

    for base in 0..32 {
        assert_address_lowerable(
            Address::X86Addr32(Box::new(Address::Direct(x86(base)))),
            count,
        );
        count += 1;
    }
    for base in [None, Some(4), Some(5), Some(16), Some(31)] {
        for index in 0..32 {
            for scale in [1, 2, 4, 8] {
                for disp in displacements {
                    assert_address_lowerable(
                        Address::X86Addr32(Box::new(Address::BaseIndexScale {
                            base: base.map(x86),
                            index: x86(index),
                            scale,
                            disp,
                            disp_size: DispSize::Auto,
                        })),
                        count,
                    );
                    count += 1;
                }
            }
        }
    }

    for segment in [X86Reg::FsBase, X86Reg::GsBase] {
        for base in [None, Some(4), Some(5), Some(16), Some(31)] {
            for index in [None, Some(4), Some(5), Some(16), Some(31)] {
                for scale in [1, 2, 4, 8] {
                    for disp in displacements.map(i64::from) {
                        assert_address_lowerable(
                            Address::SegmentRel {
                                segment: VReg::Arch(ArchReg::X86(segment)),
                                base: base.map(x86),
                                index: index.map(x86),
                                scale,
                                disp,
                            },
                            count,
                        );
                        count += 1;
                    }
                }
            }
        }
    }

    for offset in [i64::from(i32::MIN), i64::from(i32::MAX)] {
        assert_address_lowerable(
            Address::PcRel {
                offset,
                disp_size: DispSize::Disp32,
                base: Some(PC + 7),
            },
            count,
        );
        count += 1;
    }
    for absolute in [0, 0xFFFF_FFFF, u64::MAX] {
        assert_address_lowerable(Address::Absolute(absolute), count);
        count += 1;
    }

    assert_eq!(count, 15_517);
}

#[test]
fn malformed_memory_mulx_pairs_fail_closed_before_lowering() {
    let exact = manual_function(Address::Direct(x86(3)), OpWidth::W64, x86(1), Some(x86(7)));
    let mut malformed = Vec::new();

    let mut case = exact.clone();
    if let OpKind::Load { sign, .. } = &mut case.blocks[0].ops[0].kind {
        *sign = SignExtend::Sign;
    }
    malformed.push(("signed load", case));

    let mut case = exact.clone();
    if let OpKind::Load { width, .. } = &mut case.blocks[0].ops[0].kind {
        *width = MemWidth::B4;
    }
    malformed.push(("load/operation width mismatch", case));

    let mut case = exact.clone();
    case.blocks[0].ops[1].x86_hint = None;
    malformed.push(("missing hint", case));

    let mut case = exact.clone();
    if let OpKind::MulU { flags, .. } = &mut case.blocks[0].ops[1].kind {
        *flags = FlagUpdate::All;
    }
    malformed.push(("flag update", case));

    let mut case = exact.clone();
    if let OpKind::MulU { src1, .. } = &mut case.blocks[0].ops[1].kind {
        *src1 = x86(0);
    }
    malformed.push(("wrong implicit source", case));

    let mut case = exact.clone();
    if let OpKind::MulU { dst_hi, .. } = &mut case.blocks[0].ops[1].kind {
        *dst_hi = None;
    }
    malformed.push(("missing high destination", case));

    let mut case = exact.clone();
    if let OpKind::MulU { dst_lo, .. } = &mut case.blocks[0].ops[1].kind {
        *dst_lo = VReg::Virtual(VirtualId(9));
    }
    malformed.push(("virtual destination", case));

    let mut case = exact.clone();
    if let OpKind::MulU { src2, .. } = &mut case.blocks[0].ops[1].kind {
        *src2 = SrcOperand::Reg(VReg::Virtual(VirtualId(9)));
    }
    malformed.push(("different temporary", case));

    let mut case = exact.clone();
    case.blocks[0].ops[1].guest_pc += 1;
    malformed.push(("different guest PC", case));

    let mut case = exact.clone();
    if let OpKind::Load { addr, .. } = &mut case.blocks[0].ops[0].kind {
        *addr = Address::Direct(VReg::Virtual(VirtualId(7)));
    }
    malformed.push(("virtual address", case));

    let mut case = exact.clone();
    if let OpKind::Load { addr, .. } = &mut case.blocks[0].ops[0].kind {
        *addr = Address::BaseIndexScale {
            base: Some(x86(3)),
            index: x86(1),
            scale: 3,
            disp: 0,
            disp_size: DispSize::Auto,
        };
    }
    malformed.push(("invalid scale", case));

    let mut case = exact.clone();
    if let OpKind::Load { addr, .. } = &mut case.blocks[0].ops[0].kind {
        *addr = Address::GpRel { offset: 0 };
    }
    malformed.push(("non-x86 address", case));

    let mut case = exact.clone();
    if let OpKind::Load { addr, .. } = &mut case.blocks[0].ops[0].kind {
        *addr = Address::X86Addr32(Box::new(Address::X86Addr32(Box::new(Address::Direct(
            x86(3),
        )))));
    }
    malformed.push(("nested addr32", case));

    let mut case = exact.clone();
    case.blocks[0].ops.push(SmirOp::new(
        OpId(2),
        PC,
        OpKind::Mov {
            dst: x86(0),
            src: SrcOperand::Reg(VReg::Virtual(VirtualId(0xB2))),
            width: OpWidth::W64,
        },
    ));
    malformed.push(("extra temporary use", case));

    for (name, function) in malformed {
        assert!(
            !is_native_clobber_safe_excluding(&function, &std::collections::HashMap::new(), true,),
            "{name}: gate admitted malformed pair"
        );
        let mut lowerer = X86_64Lowerer::new();
        lowerer.set_mem_helpers(true);
        assert!(
            lowerer.lower_function(&function).is_err(),
            "{name}: lowerer accepted malformed pair"
        );
    }
}

#[test]
fn memory_mulx_requires_bmi2_only_for_an_executable_native_block() {
    let function = lift_case(MemoryMulxCase {
        encoding: EncodingKind::Apx,
        width: OpWidth::W64,
        dst_lo: 16,
        dst_hi: 31,
    });
    let mut excluded = std::collections::HashMap::new();
    excluded.insert(function.entry, PC);
    assert!(x86_native_scalar_features_supported_excluding(
        &function, &excluded,
    ));

    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_scalar_features_supported_excluding(
            &function,
            &std::collections::HashMap::new(),
        ),
        std::is_x86_feature_detected!("bmi2")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_scalar_features_supported_excluding(
        &function,
        &std::collections::HashMap::new(),
    ));
}

#[cfg(target_arch = "x86_64")]
#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[cfg(target_arch = "x86_64")]
#[derive(Default)]
struct MemoryContext {
    value: u64,
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_size: u64,
    last_signed: u64,
}

#[cfg(target_arch = "x86_64")]
extern "C" fn load_helper(
    context: *mut MemoryContext,
    addr: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    context.calls += 1;
    context.last_addr = addr;
    context.last_size = size;
    context.last_signed = signed;
    LoadResult {
        value: context.value,
        ok: context.ok,
    }
}

#[cfg(target_arch = "x86_64")]
fn expected_product(case: MemoryMulxCase, lhs: u64, rhs: u64) -> (u64, u64) {
    match case.width {
        OpWidth::W32 => {
            let product = u64::from(lhs as u32) * u64::from(rhs as u32);
            (u64::from(product as u32), u64::from((product >> 32) as u32))
        }
        OpWidth::W64 => {
            let product = u128::from(lhs) * u128::from(rhs);
            (product as u64, (product >> 64) as u64)
        }
        _ => unreachable!("MULX supports W32/W64"),
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_memory_mulx_uses_addr32_segment_and_apx_egpr_effective_addresses() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    if !std::is_x86_feature_detected!("bmi2") {
        return;
    }
    let vex = MemoryMulxCase {
        encoding: EncodingKind::Vex,
        width: OpWidth::W64,
        dst_lo: 1,
        dst_hi: 0,
    };
    let apx = MemoryMulxCase {
        encoding: EncodingKind::Apx,
        width: OpWidth::W64,
        dst_lo: 19,
        dst_hi: 20,
    };
    let initial_gprs = || core::array::from_fn(|index| 0xA500_0000_0000_0000u64 | index as u64);
    let mut cases = Vec::new();

    let mut addr32_gprs = initial_gprs();
    addr32_gprs[2] = 0xFEDC_BA98_7654_3210;
    addr32_gprs[3] = 0xAAAA_BBBB_FFFF_FFF0;
    addr32_gprs[1] = 0xCCCC_DDDD_0000_0008;
    cases.push((
        "addr32",
        vex,
        vec![0x67, 0xC4, 0xE2, 0xF3, 0xF6, 0x44, 0x8B, 0x20],
        addr32_gprs,
        0,
        0x30,
    ));

    let mut fs_gprs = initial_gprs();
    fs_gprs[2] = 0xFEDC_BA98_7654_3210;
    fs_gprs[3] = 0x1000;
    fs_gprs[1] = 0x20;
    let fs_base = 0x1234_5678_0000_0000;
    cases.push((
        "FS-relative",
        vex,
        vec![0x64, 0xC4, 0xE2, 0xF3, 0xF6, 0x44, 0x8B, 0x20],
        fs_gprs,
        fs_base,
        fs_base + 0x1000 + 0x20 * 4 + 0x20,
    ));

    let mut apx_gprs = initial_gprs();
    apx_gprs[2] = 0xFEDC_BA98_7654_3210;
    apx_gprs[17] = 0x2000;
    apx_gprs[18] = 0x30;
    cases.push((
        "APX EGPR SIB",
        apx,
        vec![0x62, 0xEA, 0xE3, 0x00, 0xF6, 0x64, 0x91, 0x20],
        apx_gprs,
        0,
        0x2000 + 0x30 * 4 + 0x20,
    ));

    for (name, case, bytes, gprs, fs_base, expected_addr) in cases {
        for level in [OptLevel::O0, OptLevel::O2] {
            let expected_shape = match name {
                "addr32" => Address::X86Addr32(Box::new(Address::BaseIndexScale {
                    base: Some(x86(3)),
                    index: x86(1),
                    scale: 4,
                    disp: 0x20,
                    disp_size: DispSize::Disp8,
                })),
                "FS-relative" => Address::SegmentRel {
                    segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                    base: Some(x86(3)),
                    index: Some(x86(1)),
                    scale: 4,
                    disp: 0x20,
                },
                "APX EGPR SIB" => Address::BaseIndexScale {
                    base: Some(x86(17)),
                    index: x86(18),
                    scale: 4,
                    disp: 0x20,
                    disp_size: DispSize::Disp8,
                },
                _ => unreachable!(),
            };
            let function = optimize(lift_bytes(case, &bytes, &expected_shape), level);
            let (code, entry) = lower(&function);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{name} {level:?}: {error:?}"));
            let rhs = 0x0123_4567_89AB_CDEF;
            let mut context = MemoryContext {
                value: rhs,
                ok: 1,
                ..MemoryContext::default()
            };
            let mut registers = GuestRegs {
                gpr: gprs,
                rflags: 0x8D7,
                exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
                fs_base,
                apx_enabled: 1,
                ..GuestRegs::default()
            };
            registers.ctx = (&mut context as *mut MemoryContext) as u64;
            registers.load_fn = load_helper as usize as u64;
            let mut expected = registers;
            let (low, high) = expected_product(case, gprs[2], rhs);
            expected.gpr[usize::from(case.dst_lo)] = low;
            expected.gpr[usize::from(case.dst_hi)] = high;

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{name} {level:?}");
            assert_eq!(context.calls, 1, "{name} {level:?}");
            assert_eq!(context.last_addr, expected_addr, "{name} {level:?}");
            assert_eq!(context.last_size, 8, "{name} {level:?}");
            assert_eq!(context.last_signed, 0, "{name} {level:?}");
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn native_memory_mulx_is_fault_precise_and_preserves_complete_guest_state() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    if !std::is_x86_feature_detected!("bmi2") {
        return;
    }
    let tuples = [
        (EncodingKind::Vex, 1, 7),
        (EncodingKind::Vex, 0, 8),
        (EncodingKind::Vex, 2, 2),
        (EncodingKind::Vex, 3, 3),
        (EncodingKind::Vex, 4, 5),
        (EncodingKind::Vex, 5, 4),
        (EncodingKind::Apx, 16, 17),
        (EncodingKind::Apx, 31, 31),
        (EncodingKind::Apx, 4, 16),
        (EncodingKind::Apx, 16, 5),
        (EncodingKind::Apx, 2, 31),
    ];
    let values = [
        (0, 0),
        (1, u64::MAX),
        (0xFFFF_FFFF, 0xFFFF_FFFF),
        (0x1_0000_0000, 0x1_0000_0000),
        (0x8000_0000_0000_0000, 2),
        (u64::MAX, u64::MAX),
    ];
    let mut successes = 0usize;
    let mut faults = 0usize;

    for (encoding, dst_lo, dst_hi) in tuples {
        for width in [OpWidth::W32, OpWidth::W64] {
            let case = MemoryMulxCase {
                encoding,
                width,
                dst_lo,
                dst_hi,
            };
            for level in LEVELS {
                let function = optimize(lift_case(case), level);
                let (code, entry) = lower(&function);
                let exec = ExecMem::new(&code)
                    .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));

                for (ordinal, (lhs, rhs)) in values.into_iter().enumerate() {
                    let address = 0x4000_0000_0000_1000u64.wrapping_add(ordinal as u64 * 0x20);
                    let mut initial = GuestRegs {
                        gpr: core::array::from_fn(|index| {
                            0xA500_0000_0000_0000u64.wrapping_add((index as u64) * 0x0101_0101)
                        }),
                        rflags: 0x8D7,
                        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
                        mxcsr: 0x1F80 | ordinal as u32,
                        ac_flag: 1,
                        apx_enabled: 1,
                        k: core::array::from_fn(|index| {
                            0x0102_0304_0506_0708u64.rotate_left(index as u32)
                        }),
                        ..GuestRegs::default()
                    };
                    initial.gpr[2] = lhs;
                    initial.gpr[3] = address;
                    for (index, vector) in initial.zmm.iter_mut().enumerate() {
                        *vector = core::array::from_fn(|lane| {
                            0x1122_3344_5566_7788u64.wrapping_add((index * 8 + lane) as u64)
                        });
                    }

                    let mut context = MemoryContext {
                        value: rhs,
                        ok: 1,
                        ..MemoryContext::default()
                    };
                    let mut registers = initial;
                    registers.ctx = (&mut context as *mut MemoryContext) as u64;
                    registers.load_fn = load_helper as usize as u64;
                    let mut expected = registers;
                    let (low, high) = expected_product(case, lhs, rhs);
                    expected.gpr[usize::from(dst_lo)] = low;
                    expected.gpr[usize::from(dst_hi)] = high;

                    exec.run(entry, &mut registers);
                    expected.host_mxcsr = registers.host_mxcsr;
                    assert_eq!(
                        registers, expected,
                        "{level:?} {case:?} lhs={lhs:#018X} rhs={rhs:#018X}"
                    );
                    assert_eq!(context.calls, 1, "{level:?} {case:?}");
                    assert_eq!(context.last_addr, address, "{level:?} {case:?}");
                    assert_eq!(
                        context.last_size,
                        u64::from(width.bits() / 8),
                        "{level:?} {case:?}"
                    );
                    assert_eq!(context.last_signed, 0, "{level:?} {case:?}");
                    successes += 1;
                }

                let mut context = MemoryContext {
                    value: u64::MAX,
                    ok: 0,
                    ..MemoryContext::default()
                };
                let mut registers = GuestRegs {
                    gpr: core::array::from_fn(|index| {
                        0x5A00_0000_0000_0000u64.wrapping_add(index as u64)
                    }),
                    rflags: 0x8D7,
                    exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
                    ac_flag: 1,
                    apx_enabled: 1,
                    ..GuestRegs::default()
                };
                registers.gpr[2] = 0xFEDC_BA98_7654_3210;
                registers.gpr[3] = 0x1234_5000;
                registers.ctx = (&mut context as *mut MemoryContext) as u64;
                registers.load_fn = load_helper as usize as u64;
                let mut expected = registers;
                expected.exit_pc = PC;

                exec.run(entry, &mut registers);
                expected.host_mxcsr = registers.host_mxcsr;
                assert_eq!(registers, expected, "fault {level:?} {case:?}");
                assert_eq!(context.calls, 1, "fault {level:?} {case:?}");
                assert_eq!(context.last_addr, 0x1234_5000, "fault {level:?} {case:?}");
                faults += 1;
            }
        }
    }

    eprintln!("executed {successes} successful and {faults} faulting native memory MULX cases");
    assert_eq!(successes, tuples.len() * 2 * LEVELS.len() * values.len());
    assert_eq!(faults, tuples.len() * 2 * LEVELS.len());
}
