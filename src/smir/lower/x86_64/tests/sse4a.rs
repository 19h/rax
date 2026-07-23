//! Fault-precise state-backed native lowering for AMD SSE4A operations.

use super::*;
use crate::isa::x86_64::flags;
use crate::smir::ir::ops::{OpKind, SmirOp, X86OpHint, X86Sse4aBitfieldKind};
use crate::smir::ir::types::{
    Address, ArchReg, FunctionId, MemWidth, OpId, OpWidth, SrcOperand, VReg, VirtualId, X86Reg,
};
use crate::smir::ir::{FunctionBuilder, Terminator};
use crate::smir::lower::{
    X86_GUEST_CPUID_SSE4A_OFFSET, X86_GUEST_CR0_OFFSET, X86_GUEST_CR4_OFFSET,
    X86_GUEST_VEC_STORE_FN_OFFSET, X86_GUEST_ZMM_OFFSET,
};

const CR0_EM: u64 = 1 << 2;
const CR0_TS: u64 = 1 << 3;
const CR4_OSFXSR: u64 = 1 << 9;

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn bitfield(
    dst: u8,
    source: u8,
    kind: X86Sse4aBitfieldKind,
    length: Option<u8>,
    index: Option<u8>,
) -> OpKind {
    OpKind::X86Sse4aBitfield {
        dst: xmm(dst),
        source: xmm(source),
        kind,
        length,
        index,
    }
}

fn movnt(src: VReg, addr: Address, width: MemWidth) -> OpKind {
    OpKind::X86Sse4aMovntStore { src, addr, width }
}

fn lower_ops(ops: Vec<(u64, OpKind)>, fault_guards: bool) -> Result<(Vec<u8>, usize), LowerError> {
    lower_ops_with_memory(ops, fault_guards, false, false)
}

fn lower_ops_with_memory(
    ops: Vec<(u64, OpKind)>,
    fault_guards: bool,
    mem_helpers: bool,
    preserve_vectors: bool,
) -> Result<(Vec<u8>, usize), LowerError> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for (pc, kind) in ops {
        builder.push_op(pc, kind);
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    lowerer.set_mem_helpers(mem_helpers);
    lowerer.set_preserve_vector_mem_helpers(preserve_vectors);
    let lowered = lowerer.lower_function(&builder.finish())?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

#[test]
fn lower_sse4a_movnt_requires_memory_helpers_and_embeds_exact_helper_abi() {
    for (name, source, width, size) in [
        ("MOVNTSS", xmm(1), MemWidth::B4, 4_u32),
        ("MOVNTSD extended XMM", xmm(9), MemWidth::B8, 8_u32),
    ] {
        let kind = movnt(
            source,
            Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
            width,
        );
        assert!(matches!(
            lower_ops(vec![(0x2345, kind.clone())], true),
            Err(LowerError::UnsupportedOp { .. })
        ));

        let (code, _) = lower_ops_with_memory(vec![(0x2345, kind)], true, true, false)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        for (field, value) in [
            (
                "source index",
                u32::from(match source {
                    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index))) => index,
                    _ => unreachable!(),
                }),
            ),
            ("byte size", size),
            ("helper offset", X86_GUEST_VEC_STORE_FN_OFFSET as u32),
            ("deoptimization PC", 0x2345),
        ] {
            assert!(
                code.windows(4).any(|window| window == value.to_le_bytes()),
                "{name}: missing {field} {value:#x}: {code:02X?}"
            );
        }
    }
}

#[test]
fn lower_sse4a_movnt_rejects_every_noncanonical_shape() {
    for (name, kind) in [
        (
            "virtual source",
            movnt(
                VReg::Virtual(VirtualId(0)),
                Address::Absolute(0x2000),
                MemWidth::B4,
            ),
        ),
        (
            "unencodable XMM",
            movnt(xmm(16), Address::Absolute(0x2000), MemWidth::B8),
        ),
        (
            "invalid width",
            movnt(xmm(1), Address::Absolute(0x2000), MemWidth::B2),
        ),
    ] {
        assert!(
            matches!(
                lower_ops_with_memory(vec![(0x1000, kind)], true, true, false),
                Err(LowerError::InvalidOperand { .. })
            ),
            "{name}"
        );
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        movnt(xmm(1), Address::Absolute(0x2000), MemWidth::B4),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!x86_sse4a_movnt_store_shape_valid(&hinted.blocks[0].ops[0]));
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    assert!(matches!(
        lowerer.lower_function(&hinted),
        Err(LowerError::InvalidOperand { .. })
    ));
}

#[test]
fn lower_sse4a_guard_requires_deoptimization_and_embeds_all_dynamic_state() {
    let guard = vec![(0x2345, OpKind::X86RequireSse4a)];
    assert!(matches!(
        lower_ops(guard.clone(), false),
        Err(LowerError::UnsupportedOp { .. })
    ));

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x2345, OpKind::X86RequireSse4a);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(true);
    assert!(matches!(
        lowerer.lower_function(&hinted),
        Err(LowerError::InvalidOperand { .. })
    ));

    let (code, _) = lower_ops(guard, true).expect("lower SSE4A guard");
    for (name, value) in [
        ("CPUID.SSE4A", X86_GUEST_CPUID_SSE4A_OFFSET),
        ("CR0", X86_GUEST_CR0_OFFSET),
        ("CR4", X86_GUEST_CR4_OFFSET),
    ] {
        assert!(
            code.windows(4)
                .any(|window| window == (value as u32).to_le_bytes()),
            "missing {name} displacement: {code:02X?}"
        );
    }
    assert!(
        code.windows(4)
            .any(|window| window == 0x2345_u32.to_le_bytes()),
        "missing precise deoptimization PC: {code:02X?}"
    );
}

#[test]
fn lower_sse4a_bitfield_rejects_every_noncanonical_shape() {
    for (name, kind) in [
        (
            "unpaired controls",
            bitfield(1, 1, X86Sse4aBitfieldKind::Extract, Some(8), None),
        ),
        (
            "out-of-range immediate",
            bitfield(1, 1, X86Sse4aBitfieldKind::Extract, Some(64), Some(0)),
        ),
        (
            "immediate EXTRQ source mismatch",
            bitfield(1, 2, X86Sse4aBitfieldKind::Extract, Some(8), Some(4)),
        ),
        (
            "extended destination",
            bitfield(16, 1, X86Sse4aBitfieldKind::Insert, Some(8), Some(4)),
        ),
        (
            "virtual destination",
            OpKind::X86Sse4aBitfield {
                dst: VReg::Virtual(VirtualId(0)),
                source: xmm(1),
                kind: X86Sse4aBitfieldKind::Insert,
                length: None,
                index: None,
            },
        ),
    ] {
        assert!(
            matches!(
                lower_ops(vec![(0x1000, kind)], true),
                Err(LowerError::InvalidOperand { .. })
            ),
            "{name}"
        );
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        bitfield(1, 2, X86Sse4aBitfieldKind::Insert, None, None),
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(!x86_sse4a_bitfield_shape_valid(&hinted.blocks[0].ops[0]));
    let mut lowerer = X86_64Lowerer::new();
    assert!(matches!(
        lowerer.lower_function(&hinted),
        Err(LowerError::InvalidOperand { .. })
    ));
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn initialized_guest_regs() -> crate::smir::lower::runtime::GuestRegs {
    let mut regs = crate::smir::lower::runtime::GuestRegs::default();
    for (index, value) in regs.gpr.iter_mut().enumerate() {
        *value = 0xA500_0000_0000_0000 | index as u64;
    }
    regs.rflags = 0x2 | 0x08D5 | flags::bits::DF;
    regs.ac_flag = 1;
    regs.exit_pc = 0xDEAD_BEEF_CAFE_BABE;
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_sse4a_guard_is_dynamic_precise_noncommitting_and_flag_neutral() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let ops = vec![
        (0x2345, OpKind::X86RequireSse4a),
        (
            0x2345,
            OpKind::Mov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Rbx)),
                src: SrcOperand::Imm64(0x1357_9BDF_2468_ACE0_u64 as i64),
                width: OpWidth::W64,
            },
        ),
    ];
    let (code, entry) = lower_ops(ops, true).expect("lower SSE4A-guarded sequence");
    let exec = ExecMem::new(&code).expect("map SSE4A-guarded sequence");

    for (name, feature, cr0, cr4, succeeds) in [
        ("enabled", 1, 1, CR4_OSFXSR, true),
        ("feature absent", 0, 1, CR4_OSFXSR, false),
        ("CR0.EM", 1, 1 | CR0_EM, CR4_OSFXSR, false),
        ("CR0.TS", 1, 1 | CR0_TS, CR4_OSFXSR, false),
        ("CR4.OSFXSR absent", 1, 1, 0, false),
    ] {
        let mut regs: GuestRegs = initialized_guest_regs();
        let before_gpr = regs.gpr;
        regs.cpuid_sse4a = feature;
        regs.cr0 = cr0;
        regs.cr4 = cr4;
        exec.run(entry, &mut regs);

        assert_eq!(
            regs.exit_pc,
            if succeeds {
                0xDEAD_BEEF_CAFE_BABE
            } else {
                0x2345
            },
            "{name}"
        );
        for (index, actual) in regs.gpr.iter().enumerate() {
            let expected = if succeeds && index == 3 {
                0x1357_9BDF_2468_ACE0
            } else {
                before_gpr[index]
            };
            assert_eq!(*actual, expected, "{name}: GPR{index}");
        }
        assert_eq!(
            regs.rflags & (0x08D5 | flags::bits::DF),
            0x08D5 | flags::bits::DF,
            "{name}: RFLAGS"
        );
        assert_eq!(regs.ac_flag, 1, "{name}: AC");
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_sse4a_bitfields_update_only_the_low_destination_qword() {
    use crate::smir::lower::runtime::ExecMem;

    struct Case {
        name: &'static str,
        kind: OpKind,
        dst: usize,
        source: usize,
        dst_value: [u64; 8],
        source_value: [u64; 8],
        expected_low: u64,
    }
    let vector = |low, high| [low, high, 3, 4, 5, 6, 7, 8];
    let cases = [
        Case {
            name: "EXTRQ immediate",
            kind: bitfield(1, 1, X86Sse4aBitfieldKind::Extract, Some(8), Some(4)),
            dst: 1,
            source: 1,
            dst_value: vector(0xFEDC_BA98_7654_3210, 0x1112_1314_1516_1718),
            source_value: [0; 8],
            expected_low: 0x21,
        },
        Case {
            name: "EXTRQ register",
            kind: bitfield(2, 3, X86Sse4aBitfieldKind::Extract, None, None),
            dst: 2,
            source: 3,
            dst_value: vector(0xFEDC_BA98_7654_3210, 0x2122_2324_2526_2728),
            source_value: vector(0xFFFF_FFFF_FFFF_100C, 0),
            expected_low: 0x654,
        },
        Case {
            name: "INSERTQ immediate",
            kind: bitfield(4, 5, X86Sse4aBitfieldKind::Insert, Some(8), Some(16)),
            dst: 4,
            source: 5,
            dst_value: vector(0xFFFF_0000_FFFF_0000, 0x4142_4344_4546_4748),
            source_value: vector(0xA5, 0),
            expected_low: 0xFFFF_0000_FFA5_0000,
        },
        Case {
            name: "INSERTQ register",
            kind: bitfield(6, 7, X86Sse4aBitfieldKind::Insert, None, None),
            dst: 6,
            source: 7,
            dst_value: vector(0x0123_4567_89AB_CDEF, 0x6162_6364_6566_6768),
            source_value: vector(0xE7, 0xFFFF_FFFF_FFFF_2008),
            expected_low: 0x0123_45E7_89AB_CDEF,
        },
        Case {
            name: "INSERTQ register alias",
            kind: bitfield(1, 1, X86Sse4aBitfieldKind::Insert, None, None),
            dst: 1,
            source: 1,
            dst_value: vector(0x0123_4567_89AB_CDEF, 0xFFFF_FFFF_FFFF_2008),
            source_value: [0; 8],
            expected_low: 0x0123_45EF_89AB_CDEF,
        },
        Case {
            name: "EXTRQ encoded length zero",
            kind: bitfield(1, 1, X86Sse4aBitfieldKind::Extract, Some(0), Some(0)),
            dst: 1,
            source: 1,
            dst_value: vector(0x8877_6655_4433_2211, 0x7172_7374_7576_7778),
            source_value: [0; 8],
            expected_low: 0x8877_6655_4433_2211,
        },
        Case {
            name: "EXTRQ extended XMM",
            kind: bitfield(9, 10, X86Sse4aBitfieldKind::Extract, None, None),
            dst: 9,
            source: 10,
            dst_value: vector(0x8877_6655_4433_2211, 0x8182_8384_8586_8788),
            source_value: vector((4 << 8) | 8, 0x9192_9394_9596_9798),
            expected_low: 0x21,
        },
    ];

    for case in cases {
        let (code, entry) = lower_ops(vec![(0x1000, case.kind)], true)
            .unwrap_or_else(|error| panic!("{} lowering: {error:?}", case.name));
        let dst_offset = X86_GUEST_ZMM_OFFSET + case.dst as i32 * 64;
        assert!(
            code.windows(4)
                .any(|window| window == (dst_offset as u32).to_le_bytes()),
            "{} missing destination state displacement: {code:02X?}",
            case.name
        );
        let exec = ExecMem::new(&code)
            .unwrap_or_else(|error| panic!("{} executable map: {error:?}", case.name));
        let mut regs = initialized_guest_regs();
        let before_gpr = regs.gpr;
        regs.set_zmm(case.dst, case.dst_value);
        if case.source != case.dst {
            regs.set_zmm(case.source, case.source_value);
        }
        exec.run(entry, &mut regs);

        let mut expected = case.dst_value;
        expected[0] = case.expected_low;
        assert_eq!(
            regs.get_zmm(case.dst),
            expected,
            "{} destination",
            case.name
        );
        if case.source != case.dst {
            assert_eq!(
                regs.get_zmm(case.source),
                case.source_value,
                "{} source",
                case.name
            );
        }
        assert_eq!(regs.gpr, before_gpr, "{} GPR image", case.name);
        assert_eq!(
            regs.rflags & (0x08D5 | flags::bits::DF),
            0x08D5 | flags::bits::DF,
            "{} RFLAGS",
            case.name
        );
        assert_eq!(regs.ac_flag, 1, "{} AC", case.name);
    }
}

#[test]
fn sse4a_shape_validator_accepts_exact_lifted_contract() {
    for kind in [
        bitfield(1, 1, X86Sse4aBitfieldKind::Extract, Some(8), Some(4)),
        bitfield(1, 2, X86Sse4aBitfieldKind::Extract, None, None),
        bitfield(1, 2, X86Sse4aBitfieldKind::Insert, Some(8), Some(4)),
        bitfield(1, 2, X86Sse4aBitfieldKind::Insert, None, None),
    ] {
        let op = SmirOp::new(OpId(0), 0x1000, kind);
        assert!(x86_sse4a_bitfield_shape_valid(&op), "{op:?}");
    }

    for kind in [
        movnt(xmm(0), Address::Absolute(0x2000), MemWidth::B4),
        movnt(xmm(15), Address::Absolute(0x2000), MemWidth::B8),
    ] {
        let op = SmirOp::new(OpId(0), 0x1000, kind);
        assert!(x86_sse4a_movnt_store_shape_valid(&op), "{op:?}");
    }
}
