//! Fault-precise state-backed native lowering for AMD XOP packed rotate/shift.

use super::*;
use crate::smir::OpId;
use crate::smir::ir::ops::{
    OpKind, SmirOp, X86OpHint, X86SsePrefix, X86VecAlign, X86VecMap, X86XopPackedBitKind,
};
use crate::smir::ir::types::{
    Address, ArchReg, FunctionId, OpWidth, SrcOperand, VReg, VecElementType, VecWidth, VirtualId,
    X86Reg,
};
use crate::smir::ir::{FunctionBuilder, SmirFunction, Terminator};
use crate::smir::lower::x86_64::{
    x86_check_alignment_ac_shape_valid, x86_require_xop_shape_valid, x86_xop_packed_bit_shape_valid,
};
use crate::smir::lower::{
    X86_GUEST_AC_FLAG_OFFSET, X86_GUEST_CPL_OFFSET, X86_GUEST_CPUID_XOP_OFFSET,
    X86_GUEST_CR0_OFFSET, X86_GUEST_CR4_OFFSET, X86_GUEST_CS_L_OFFSET, X86_GUEST_RFLAGS_OFFSET,
    X86_GUEST_VEC_LOAD_FN_OFFSET, X86_GUEST_VECTOR_SCRATCH_OFFSET, X86_GUEST_XCR0_OFFSET,
    X86_GUEST_ZMM_OFFSET,
};

const PC: u64 = 0x2345;
const SENTINEL_PC: u64 = 0xAAAA_BBBB_CCCC_DDDD;
const CR0_PE: u64 = 1;
const CR0_TS: u64 = 1 << 3;
const CR0_AM: u64 = 1 << 18;
const CR4_OSXSAVE: u64 = 1 << 18;
const STATUS_FLAGS: u64 = 1 | (1 << 2) | (1 << 4) | (1 << 6) | (1 << 7) | (1 << 11);

fn xmm(index: u8) -> VReg {
    VReg::Arch(ArchReg::X86(X86Reg::Xmm(index)))
}

fn xop(
    dst: VReg,
    src: VReg,
    count: SrcOperand,
    elem: VecElementType,
    kind: X86XopPackedBitKind,
) -> OpKind {
    OpKind::X86XopPackedBit {
        dst,
        src,
        count,
        elem,
        kind,
    }
}

#[allow(clippy::too_many_arguments)]
fn lower(
    function: &SmirFunction,
    fault_guards: bool,
    mem_helpers: bool,
    preserve_vectors: bool,
    native_vector_state: bool,
    avx_ymm16_state: bool,
) -> Result<(Vec<u8>, usize), LowerError> {
    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_jit_fault_deopt_guards(fault_guards);
    lowerer.set_mem_helpers(mem_helpers);
    lowerer.set_preserve_vector_mem_helpers(preserve_vectors);
    lowerer.set_native_vector_state_active(native_vector_state);
    lowerer.set_avx_ymm16_vector_state(avx_ymm16_state);
    let lowered = lowerer.lower_function(function)?;
    assert!(lowered.relocations.is_empty());
    Ok((lowerer.finalize()?, lowered.entry_offset))
}

fn function_with(ops: Vec<(u64, OpKind)>) -> SmirFunction {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    for (pc, kind) in ops {
        builder.push_op(pc, kind);
    }
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.finish()
}

fn memory_function(memory_is_source: bool) -> SmirFunction {
    let temporary = VReg::Virtual(VirtualId(7));
    let addr = Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rbx)));
    let mut function = function_with(vec![
        (PC, OpKind::X86RequireXop),
        (
            PC,
            OpKind::X86CheckAlignmentAc {
                addr: addr.clone(),
                alignment: 16,
                stack_segment: false,
            },
        ),
        (
            PC,
            OpKind::VLoad {
                dst: temporary,
                addr,
                width: VecWidth::V128,
            },
        ),
        (
            PC,
            if memory_is_source {
                xop(
                    xmm(1),
                    temporary,
                    SrcOperand::Reg(xmm(3)),
                    VecElementType::I8,
                    X86XopPackedBitKind::LogicalShift,
                )
            } else {
                xop(
                    xmm(1),
                    xmm(3),
                    SrcOperand::Reg(temporary),
                    VecElementType::I8,
                    X86XopPackedBitKind::LogicalShift,
                )
            },
        ),
    ]);
    function.blocks[0].ops[2].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Aligned));
    function
}

#[test]
fn guard_requires_deoptimization_and_embeds_every_live_state_field_and_fault_pc() {
    let exact = function_with(vec![(PC, OpKind::X86RequireXop)]);
    assert!(matches!(
        lower(&exact, false, false, false, false, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    let mut hinted = exact.clone();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::RexByteReg);
    assert!(matches!(
        lower(&hinted, true, false, false, false, false),
        Err(LowerError::InvalidOperand { .. })
    ));

    let op = SmirOp::new(OpId(0), PC, OpKind::X86RequireXop);
    assert!(x86_require_xop_shape_valid(&op));
    let (code, _) = lower(&exact, true, false, false, false, false).expect("lower exact XOP guard");
    for (name, offset) in [
        ("CPUID.XOP", X86_GUEST_CPUID_XOP_OFFSET),
        ("CR0", X86_GUEST_CR0_OFFSET),
        ("CR4", X86_GUEST_CR4_OFFSET),
        ("XCR0", X86_GUEST_XCR0_OFFSET),
        ("CS.L", X86_GUEST_CS_L_OFFSET),
        ("RFLAGS", X86_GUEST_RFLAGS_OFFSET),
    ] {
        assert!(
            code.windows(4)
                .any(|window| window == (offset as u32).to_le_bytes()),
            "missing {name} displacement: {code:02X?}"
        );
    }
    assert!(
        code.windows(4)
            .any(|window| window == (PC as u32).to_le_bytes()),
        "missing exact guard deoptimization PC: {code:02X?}"
    );
}

#[test]
fn alignment_guard_requires_precise_deoptimization_and_exact_state_backed_shape() {
    let alignment = OpKind::X86CheckAlignmentAc {
        addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rcx))),
        alignment: 16,
        stack_segment: false,
    };
    let function = function_with(vec![(PC, alignment.clone())]);
    assert!(matches!(
        lower(&function, false, false, false, false, false),
        Err(LowerError::UnsupportedOp { .. })
    ));
    let op = SmirOp::new(OpId(0), PC, alignment);
    assert!(x86_check_alignment_ac_shape_valid(&op));
    let (code, _) = lower(&function, true, false, false, false, false).expect("lower #AC guard");
    for (name, offset) in [
        ("CR0", X86_GUEST_CR0_OFFSET),
        ("CPL", X86_GUEST_CPL_OFFSET),
        ("AC shadow", X86_GUEST_AC_FLAG_OFFSET),
        ("CS.L", X86_GUEST_CS_L_OFFSET),
    ] {
        assert!(
            code.windows(4)
                .any(|window| window == (offset as u32).to_le_bytes()),
            "missing alignment {name} displacement: {code:02X?}"
        );
    }
    assert!(
        code.windows(4)
            .any(|window| window == (PC as u32).to_le_bytes()),
        "missing exact alignment deoptimization PC: {code:02X?}"
    );

    for malformed in [
        OpKind::X86CheckAlignmentAc {
            addr: Address::Absolute(0x2000),
            alignment: 8,
            stack_segment: false,
        },
        OpKind::X86CheckAlignmentAc {
            addr: Address::Direct(VReg::Virtual(VirtualId(0))),
            alignment: 16,
            stack_segment: false,
        },
    ] {
        let function = function_with(vec![(PC, malformed)]);
        assert!(matches!(
            lower(&function, true, false, false, false, false),
            Err(LowerError::InvalidOperand { .. })
                | Err(LowerError::InvalidRegister(_))
                | Err(LowerError::UnsupportedOp { .. })
        ));
    }
}

#[test]
fn register_lowering_accepts_all_semantic_cells_and_rejects_malformed_shapes() {
    for kind in [
        X86XopPackedBitKind::Rotate,
        X86XopPackedBitKind::LogicalShift,
        X86XopPackedBitKind::ArithmeticShift,
    ] {
        for elem in [
            VecElementType::I8,
            VecElementType::I16,
            VecElementType::I32,
            VecElementType::I64,
        ] {
            for count in [
                SrcOperand::Reg(xmm(3)),
                SrcOperand::Imm(0),
                SrcOperand::Imm(255),
            ] {
                let operation = xop(xmm(1), xmm(2), count.clone(), elem, kind);
                let op = SmirOp::new(OpId(0), PC, operation.clone());
                assert!(x86_xop_packed_bit_shape_valid(&op));
                let function = function_with(vec![(PC, operation)]);
                let (code, _) = lower(&function, false, false, false, false, false)
                    .expect("lower state-backed XOP operation");
                assert!(!code.is_empty(), "{kind:?}, {elem:?}, {count:?}");
                for index in [1_u8, 2] {
                    let offset = X86_GUEST_ZMM_OFFSET + i32::from(index) * 64;
                    assert!(
                        code.windows(4)
                            .any(|window| window == (offset as u32).to_le_bytes()),
                        "missing XMM{index} state displacement"
                    );
                }
            }
        }
    }

    for malformed in [
        xop(
            xmm(16),
            xmm(2),
            SrcOperand::Imm(1),
            VecElementType::I8,
            X86XopPackedBitKind::Rotate,
        ),
        xop(
            xmm(1),
            xmm(2),
            SrcOperand::Imm(256),
            VecElementType::I8,
            X86XopPackedBitKind::Rotate,
        ),
        xop(
            xmm(1),
            xmm(2),
            SrcOperand::Reg(VReg::Virtual(VirtualId(0))),
            VecElementType::I8,
            X86XopPackedBitKind::Rotate,
        ),
        xop(
            xmm(1),
            xmm(2),
            SrcOperand::Imm(1),
            VecElementType::F32,
            X86XopPackedBitKind::Rotate,
        ),
    ] {
        let function = function_with(vec![(PC, malformed)]);
        assert!(matches!(
            lower(&function, false, false, false, false, false),
            Err(LowerError::InvalidOperand { .. })
                | Err(LowerError::InvalidRegister(_))
                | Err(LowerError::UnsupportedOp { .. })
        ));
    }
}

#[cfg(feature = "smir-jit")]
#[test]
fn helper_backed_memory_pair_requires_memory_mode_and_preserves_physical_vectors_when_active() {
    for memory_is_source in [false, true] {
        let function = memory_function(memory_is_source);
        assert!(matches!(
            lower(&function, true, false, false, false, false),
            Err(LowerError::UnsupportedOp { .. })
                | Err(LowerError::InvalidOperand { .. })
                | Err(LowerError::InvalidRegister(_))
        ));
        let (code, _) = lower(&function, true, true, false, false, false)
            .expect("lower isolated helper-backed XOP memory pair");
        for (name, offset) in [
            ("vector-load helper", X86_GUEST_VEC_LOAD_FN_OFFSET),
            ("vector scratch", X86_GUEST_VECTOR_SCRATCH_OFFSET),
            ("destination", X86_GUEST_ZMM_OFFSET + 64),
        ] {
            assert!(
                code.windows(4)
                    .any(|window| window == (offset as u32).to_le_bytes()),
                "missing {name} displacement: {code:02X?}"
            );
        }

        assert!(matches!(
            lower(&function, true, true, false, true, true),
            Err(LowerError::UnsupportedOp { .. })
        ));
        lower(&function, true, true, true, true, true)
            .expect("mixed vector XOP helper must preserve physical vector state");
    }
}

#[test]
fn mixed_state_sync_emits_both_ymm16_and_general_vector_boundaries() {
    let function = function_with(vec![(
        PC,
        xop(
            xmm(1),
            xmm(2),
            SrcOperand::Reg(xmm(3)),
            VecElementType::I8,
            X86XopPackedBitKind::LogicalShift,
        ),
    )]);

    for (name, avx_ymm16, prefix) in [
        ("AVX YMM16", true, 0xC5_u8),
        ("general EVEX", false, 0x62_u8),
    ] {
        let (code, _) = lower(&function, false, false, false, true, avx_ymm16)
            .unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert!(
            code.iter().filter(|byte| **byte == prefix).count() >= 3,
            "{name}: missing source/count stores and destination reload: {code:02X?}"
        );
        for index in [1_u8, 2, 3] {
            let offset = X86_GUEST_ZMM_OFFSET + i32::from(index) * 64;
            assert!(
                code.windows(4)
                    .any(|window| window == (offset as u32).to_le_bytes()),
                "{name}: missing XMM{index} synchronization displacement"
            );
        }
    }
}

fn words_to_bytes(words: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0_u8; 64];
    for (chunk, word) in bytes.chunks_exact_mut(8).zip(words) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn bytes_to_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

fn reference(
    source_words: [u64; 8],
    count_words: Option<[u64; 8]>,
    immediate: Option<u8>,
    elem: VecElementType,
    kind: X86XopPackedBitKind,
) -> [u64; 8] {
    let source = words_to_bytes(source_words);
    let counts = count_words.map(words_to_bytes);
    let element_bytes = elem.bytes() as usize;
    let bits = (element_bytes * 8) as u32;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1_u64 << bits) - 1
    };
    let mut output = [0_u8; 64];
    for offset in (0..16).step_by(element_bytes) {
        let mut lane = [0_u8; 8];
        lane[..element_bytes].copy_from_slice(&source[offset..offset + element_bytes]);
        let value = u64::from_le_bytes(lane);
        let signed_count =
            immediate.unwrap_or_else(|| counts.expect("variable count")[offset]) as i8;
        let amount = u32::from(signed_count.unsigned_abs()) & (bits - 1);
        let value = match (kind, signed_count.is_negative()) {
            (X86XopPackedBitKind::Rotate, false) => {
                if bits == 64 {
                    value.rotate_left(amount)
                } else {
                    ((value << amount) | (value >> ((bits - amount) & (bits - 1)))) & mask
                }
            }
            (X86XopPackedBitKind::Rotate, true) => {
                if bits == 64 {
                    value.rotate_right(amount)
                } else {
                    ((value >> amount) | (value << ((bits - amount) & (bits - 1)))) & mask
                }
            }
            (X86XopPackedBitKind::LogicalShift, false)
            | (X86XopPackedBitKind::ArithmeticShift, false) => (value << amount) & mask,
            (X86XopPackedBitKind::LogicalShift, true) => value >> amount,
            (X86XopPackedBitKind::ArithmeticShift, true) => {
                let signed = if bits == 64 {
                    value as i64
                } else {
                    ((value << (64 - bits)) as i64) >> (64 - bits)
                };
                ((signed >> amount) as u64) & mask
            }
        };
        output[offset..offset + element_bytes]
            .copy_from_slice(&value.to_le_bytes()[..element_bytes]);
    }
    bytes_to_words(output)
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
fn initialized_regs() -> crate::smir::lower::runtime::GuestRegs {
    let mut regs = crate::smir::lower::runtime::GuestRegs {
        gpr: std::array::from_fn(|index| 0xA500_0000_0000_0000 | index as u64),
        rflags: 0x2 | STATUS_FLAGS,
        exit_pc: SENTINEL_PC,
        ..crate::smir::lower::runtime::GuestRegs::default()
    };
    for (index, value) in regs.zmm.iter_mut().enumerate() {
        *value = std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687_u64.rotate_left((index * 11 + word * 5) as u32)
        });
    }
    regs
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_register_xop_matches_reference_for_all_kinds_elements_counts_and_aliases() {
    use crate::smir::lower::runtime::ExecMem;

    let source = [
        0x9234_7E81_9ABC_5678,
        0x8123_4567_89AB_CDEF,
        0x1111_2222_3333_4444,
        0x5555_6666_7777_8888,
        0x9999_AAAA_BBBB_CCCC,
        0xDDDD_EEEE_FFFF_0000,
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
    ];
    let counts = [
        0x80F1_0FF9_07FF_0100,
        0xF808_817F_C13F_E11F,
        0xA5A5_A5A5_A5A5_A5A5,
        0x5A5A_5A5A_5A5A_5A5A,
        0,
        0,
        0,
        0,
    ];
    for kind in [
        X86XopPackedBitKind::Rotate,
        X86XopPackedBitKind::LogicalShift,
        X86XopPackedBitKind::ArithmeticShift,
    ] {
        for elem in [
            VecElementType::I8,
            VecElementType::I16,
            VecElementType::I32,
            VecElementType::I64,
        ] {
            for count in [
                SrcOperand::Reg(xmm(3)),
                SrcOperand::Imm(0),
                SrcOperand::Imm(1),
                SrcOperand::Imm(0x80),
                SrcOperand::Imm(0xFF),
            ] {
                let function =
                    function_with(vec![(PC, xop(xmm(1), xmm(2), count.clone(), elem, kind))]);
                let (code, entry) = lower(&function, false, false, false, false, false)
                    .expect("lower native register XOP");
                let exec = ExecMem::new(&code).expect("map native register XOP");
                let mut regs = initialized_regs();
                regs.zmm[2] = source;
                regs.zmm[3] = counts;
                let initial_gpr = regs.gpr;
                let initial_flags = regs.rflags;
                let mut expected_zmm = regs.zmm;
                expected_zmm[1] = match count {
                    SrcOperand::Reg(_) => reference(source, Some(counts), None, elem, kind),
                    SrcOperand::Imm(value) => {
                        reference(source, None, Some(value as u8), elem, kind)
                    }
                    _ => unreachable!(),
                };
                exec.run(entry, &mut regs);
                assert_eq!(regs.zmm, expected_zmm, "{kind:?}, {elem:?}, {count:?}");
                assert_eq!(regs.gpr, initial_gpr, "{kind:?}, {elem:?}, {count:?}");
                assert_eq!(regs.rflags, initial_flags, "{kind:?}, {elem:?}, {count:?}");
                assert_eq!(regs.exit_pc, SENTINEL_PC);
            }
        }
    }

    for (name, dst, src, count) in [
        ("destination-source", 2, 2, 3),
        ("destination-count", 3, 2, 3),
        ("source-count", 1, 2, 2),
        ("all operands", 2, 2, 2),
    ] {
        let function = function_with(vec![(
            PC,
            xop(
                xmm(dst),
                xmm(src),
                SrcOperand::Reg(xmm(count)),
                VecElementType::I32,
                X86XopPackedBitKind::LogicalShift,
            ),
        )]);
        let (code, entry) =
            lower(&function, false, false, false, false, false).expect("lower aliased XOP");
        let exec = ExecMem::new(&code).expect("map aliased XOP");
        let mut regs = initialized_regs();
        regs.zmm[2] = source;
        regs.zmm[3] = counts;
        let source_before = regs.zmm[usize::from(src)];
        let count_before = regs.zmm[usize::from(count)];
        let expected = reference(
            source_before,
            Some(count_before),
            None,
            VecElementType::I32,
            X86XopPackedBitKind::LogicalShift,
        );
        exec.run(entry, &mut regs);
        assert_eq!(regs.zmm[usize::from(dst)], expected, "{name}");
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_xop_guard_is_dynamic_precise_noncommitting_and_flag_neutral() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let function = function_with(vec![
        (PC, OpKind::X86RequireXop),
        (
            PC,
            OpKind::Mov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Rbx)),
                src: SrcOperand::Imm(0x1357_9BDF_2468_ACE0_u64 as i64),
                width: OpWidth::W64,
            },
        ),
    ]);
    let (code, entry) =
        lower(&function, true, false, false, false, false).expect("lower guarded XOP");
    let exec = ExecMem::new(&code).expect("map guarded XOP");

    for failed_condition in 0_u8..=8 {
        let admitted = failed_condition == 8;
        let mut regs = GuestRegs {
            gpr: std::array::from_fn(|index| 0xA500_0000_0000_0000 | index as u64),
            rflags: 0x2 | STATUS_FLAGS,
            exit_pc: SENTINEL_PC,
            cr0: CR0_PE,
            cr4: CR4_OSXSAVE,
            xcr0: 0b110,
            cs_l: 1,
            cpuid_xop: 1,
            ..GuestRegs::default()
        };
        match failed_condition {
            0 => regs.cpuid_xop = 0,
            1 => regs.cr0 &= !CR0_PE,
            2 => regs.cs_l = 0,
            3 => regs.rflags |= crate::isa::x86_64::flags::bits::VM,
            4 => regs.cr4 &= !CR4_OSXSAVE,
            5 => regs.xcr0 &= !(1 << 1),
            6 => regs.xcr0 &= !(1 << 2),
            7 => regs.cr0 |= CR0_TS,
            8 => {}
            _ => unreachable!(),
        }
        let before = regs.gpr;
        let before_flags = regs.rflags;
        exec.run(entry, &mut regs);
        let mut expected = before;
        if admitted {
            expected[3] = 0x1357_9BDF_2468_ACE0;
        }
        assert_eq!(regs.gpr, expected, "failed condition {failed_condition}");
        assert_eq!(
            regs.rflags, before_flags,
            "failed condition {failed_condition}"
        );
        assert_eq!(
            regs.exit_pc,
            if admitted { SENTINEL_PC } else { PC },
            "failed condition {failed_condition}"
        );
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_alignment_guard_is_dynamic_precise_and_noncommitting() {
    use crate::smir::lower::runtime::{ExecMem, GuestRegs};

    let function = function_with(vec![
        (
            PC,
            OpKind::X86CheckAlignmentAc {
                addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rcx))),
                alignment: 16,
                stack_segment: false,
            },
        ),
        (
            PC,
            OpKind::Mov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Rbx)),
                src: SrcOperand::Imm(0x1357_9BDF_2468_ACE0_u64 as i64),
                width: OpWidth::W64,
            },
        ),
    ]);
    let (code, entry) =
        lower(&function, true, false, false, false, false).expect("lower #AC guard");
    let exec = ExecMem::new(&code).expect("map #AC guard");

    for (name, address, long_mode, am, cpl, ac, admitted) in [
        ("aligned", 0x2000, true, true, 3, true, true),
        ("AM disabled", 0x2001, true, false, 3, true, true),
        ("CPL0", 0x2001, true, true, 0, true, true),
        ("AC disabled", 0x2001, true, true, 3, false, true),
        ("enabled #AC", 0x2001, true, true, 3, true, false),
        (
            "noncanonical start",
            0x0000_8000_0000_0000,
            true,
            false,
            0,
            false,
            false,
        ),
        (
            "canonical range crossing",
            0x0000_7FFF_FFFF_FFF8,
            true,
            false,
            0,
            false,
            false,
        ),
        (
            "compatibility skips canonicality",
            0x0000_8000_0000_0000,
            false,
            false,
            0,
            false,
            true,
        ),
    ] {
        let mut regs = GuestRegs {
            gpr: std::array::from_fn(|index| 0xA500_0000_0000_0000 | index as u64),
            rflags: 0x2 | STATUS_FLAGS,
            exit_pc: SENTINEL_PC,
            cr0: CR0_PE | if am { CR0_AM } else { 0 },
            cpl,
            cs_l: u64::from(long_mode),
            ac_flag: u64::from(ac),
            ..GuestRegs::default()
        };
        regs.gpr[1] = address;
        let before = regs.gpr;
        let before_flags = regs.rflags;
        exec.run(entry, &mut regs);
        let mut expected = before;
        if admitted {
            expected[3] = 0x1357_9BDF_2468_ACE0;
        }
        assert_eq!(regs.gpr, expected, "{name}");
        assert_eq!(regs.rflags, before_flags, "{name}");
        assert_eq!(regs.ac_flag, u64::from(ac), "{name}: AC shadow");
        assert_eq!(
            regs.exit_pc,
            if admitted { SENTINEL_PC } else { PC },
            "{name}"
        );
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[derive(Clone, Debug)]
struct VectorMemoryContext {
    value: [u64; 8],
    ok: u64,
    calls: u64,
    address: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
extern "C" fn vector_load_helper(
    state: *mut crate::smir::lower::runtime::GuestRegs,
    addr: u64,
    destination: u32,
    size: u32,
    zero_upper: u32,
) -> u64 {
    let state = unsafe { &mut *state };
    let context = unsafe { &mut *(state.ctx as *mut VectorMemoryContext) };
    context.calls += 1;
    context.address = addr;
    context.destination = destination;
    context.size = size;
    context.zero_upper = zero_upper;
    if context.ok == 0
        || destination != crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
        || size != 16
    {
        return 0;
    }
    state.vector_scratch = [0; 8];
    state.vector_scratch[..2].copy_from_slice(&context.value[..2]);
    1
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn native_memory_source_and_count_match_reference_and_fault_before_commit() {
    use crate::smir::lower::runtime::ExecMem;

    let memory = [
        0x9234_7E81_9ABC_5678,
        0x8123_4567_89AB_CDEF,
        0,
        0,
        0,
        0,
        0,
        0,
    ];
    let register = [
        0x80F1_0FF9_07FF_0100,
        0xF808_817F_C13F_E11F,
        0,
        0,
        0,
        0,
        0,
        0,
    ];
    for memory_is_source in [false, true] {
        let function = memory_function(memory_is_source);
        let (code, entry) =
            lower(&function, true, true, false, false, false).expect("lower native memory XOP");
        let exec = ExecMem::new(&code).expect("map native memory XOP");

        for ok in [1_u64, 0] {
            let mut context = VectorMemoryContext {
                value: memory,
                ok,
                calls: 0,
                address: 0,
                destination: 0,
                size: 0,
                zero_upper: 0,
            };
            let mut regs = initialized_regs();
            regs.gpr[3] = 0x2000;
            regs.zmm[3] = register;
            regs.ctx = (&mut context as *mut VectorMemoryContext) as u64;
            regs.vec_load_fn = vector_load_helper as usize as u64;
            regs.cpuid_xop = 1;
            regs.cr0 = CR0_PE;
            regs.cr4 = CR4_OSXSAVE;
            regs.xcr0 = 0b110;
            regs.cs_l = 1;
            let initial = regs;
            let mut expected = initial;
            if ok != 0 {
                expected.vector_scratch = memory;
                expected.zmm[1] = if memory_is_source {
                    reference(
                        memory,
                        Some(register),
                        None,
                        VecElementType::I8,
                        X86XopPackedBitKind::LogicalShift,
                    )
                } else {
                    reference(
                        register,
                        Some(memory),
                        None,
                        VecElementType::I8,
                        X86XopPackedBitKind::LogicalShift,
                    )
                };
            } else {
                expected.exit_pc = PC;
            }
            exec.run(entry, &mut regs);
            expected.host_mxcsr = regs.host_mxcsr;
            assert_eq!(regs, expected, "memory source={memory_is_source}, ok={ok}");
            assert_eq!(context.calls, 1);
            assert_eq!(context.address, 0x2000);
            assert_eq!(
                context.destination,
                crate::smir::lower::X86_JIT_VECTOR_SCRATCH_INDEX
            );
            assert_eq!(context.size, 16);
            assert_eq!(context.zero_upper, 1);
        }
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn mixed_native_vector_and_state_backed_xop_synchronize_in_both_directions() {
    use crate::smir::lower::runtime::{ExecMem, X86_VECTOR_STATE_YMM16};

    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping XOP mixed-vector synchronization: host lacks AVX");
        return;
    }

    let mut function = function_with(vec![
        (
            PC,
            OpKind::VAdd {
                dst: xmm(2),
                src1: xmm(2),
                src2: xmm(5),
                elem: VecElementType::I8,
                lanes: 16,
            },
        ),
        (
            PC,
            xop(
                xmm(1),
                xmm(2),
                SrcOperand::Reg(xmm(3)),
                VecElementType::I8,
                X86XopPackedBitKind::LogicalShift,
            ),
        ),
        (
            PC,
            OpKind::VAnd {
                dst: xmm(0),
                src1: xmm(1),
                src2: xmm(6),
                width: VecWidth::V128,
            },
        ),
    ]);
    function.blocks[0].ops[0].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::OpSize,
        opcode: 0xFC,
        width: VecWidth::V128,
        w: false,
    });
    function.blocks[0].ops[2].x86_hint = Some(X86OpHint::VexOp {
        map: X86VecMap::Map0F,
        pp: X86SsePrefix::OpSize,
        opcode: 0xDB,
        width: VecWidth::V128,
        w: false,
    });
    let (code, entry) =
        lower(&function, false, false, false, true, true).expect("lower mixed XOP region");
    let exec = ExecMem::new(&code).expect("map mixed XOP region");

    let mut regs = initialized_regs();
    regs.vector_active = X86_VECTOR_STATE_YMM16;
    let source = words_to_bytes(regs.zmm[2]);
    let addend = words_to_bytes(regs.zmm[5]);
    let mut added_bytes = [0_u8; 64];
    for index in 0..16 {
        added_bytes[index] = source[index].wrapping_add(addend[index]);
    }
    let added = bytes_to_words(added_bytes);
    let counts = regs.zmm[3];
    regs.zmm[6] = [u64::MAX; 8];
    let expected_xop = reference(
        added,
        Some(counts),
        None,
        VecElementType::I8,
        X86XopPackedBitKind::LogicalShift,
    );
    let initial_gpr = regs.gpr;
    let initial_flags = regs.rflags;
    exec.run(entry, &mut regs);

    assert_eq!(
        regs.zmm[2][..2],
        added[..2],
        "native producer -> XOP source"
    );
    assert_eq!(regs.zmm[1], expected_xop, "state-backed XOP destination");
    assert_eq!(
        regs.zmm[0][..2],
        expected_xop[..2],
        "XOP destination -> native consumer"
    );
    assert_eq!(regs.gpr, initial_gpr);
    assert_eq!(regs.rflags, initial_flags);
}
