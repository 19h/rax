//! Strict lifting, interpretation, optimization, and admission coverage for
//! legacy SSE4.2 packed-string comparisons.

use super::*;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext, VecValue};
use crate::smir::ir::flags::MaterializedFlags;
use crate::smir::ir::memory::{FlatMemory, SmirMemory};
use crate::smir::ir::ops::X86PackedStringKind;

fn exact_compare(result: &LiftResult) -> &SmirOp {
    result
        .ops
        .iter()
        .find(|op| matches!(op.kind, OpKind::X86PackedStringCompare { .. }))
        .expect("one exact packed-string comparison")
}

fn block_for(bytes: &[u8]) -> SmirBlock {
    let result = lift_single(bytes).unwrap();
    let mut block = SmirBlock::new(BlockId(0), 0x1000);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    block
}

fn set_xmm(context: &mut SmirContext, index: usize, value: VecValue) {
    match &mut context.arch_regs {
        ArchRegState::X86_64(state) => state.xmm[index] = value,
        _ => unreachable!(),
    }
}

fn get_xmm(context: &SmirContext, index: usize) -> VecValue {
    match &context.arch_regs {
        ArchRegState::X86_64(state) => state.xmm[index],
        _ => unreachable!(),
    }
}

fn xmm_bytes(bytes: &[u8]) -> VecValue {
    assert!(bytes.len() <= 16);
    let mut raw = [0u8; 16];
    raw[..bytes.len()].copy_from_slice(bytes);
    let mut value = [0u64; 16];
    value[0] = u64::from_le_bytes(raw[..8].try_into().unwrap());
    value[1] = u64::from_le_bytes(raw[8..].try_into().unwrap());
    value
}

fn execute(bytes: &[u8], context: &mut SmirContext, memory: &mut FlatMemory) {
    assert!(matches!(
        SmirInterpreter::new().execute_block(context, memory, &block_for(bytes)),
        BlockResult::Exit(ExitReason::Halt)
    ));
}

#[test]
fn legacy_pcmpxstrx_strictly_lifts_all_four_forms_and_dependencies() {
    for (opcode, kind) in [
        (0x60, X86PackedStringKind::ExplicitMask),
        (0x61, X86PackedStringKind::ExplicitIndex),
        (0x62, X86PackedStringKind::ImplicitMask),
        (0x63, X86PackedStringKind::ImplicitIndex),
    ] {
        let bytes = [0x66, 0x0F, 0x3A, opcode, 0xD1, 0xFD];
        let result = lift_single(&bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "opcode {opcode:02X}");
        assert!(matches!(result.control_flow, ControlFlow::Fallthrough));
        let op = exact_compare(&result);
        match &op.kind {
            OpKind::X86PackedStringCompare {
                dst,
                src1,
                src2,
                len1,
                len2,
                length_width,
                kind: got_kind,
                imm,
            } => {
                assert_eq!(*got_kind, kind);
                assert_eq!(*src1, VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))));
                assert_eq!(*src2, VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))));
                assert_eq!(*imm, 0xFD);
                if kind.is_explicit() {
                    assert_eq!(*len1, Some(x86_gpr(0)));
                    assert_eq!(*len2, Some(x86_gpr(2)));
                    assert_eq!(*length_width, OpWidth::W32);
                } else {
                    assert_eq!(*len1, None);
                    assert_eq!(*len2, None);
                    assert_eq!(*length_width, OpWidth::W32);
                }
                assert_eq!(
                    *dst,
                    if kind.returns_mask() {
                        VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)))
                    } else {
                        x86_gpr(1)
                    }
                );
            }
            _ => unreachable!(),
        }
        let mut expected_sources = vec![
            VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        ];
        if kind.is_explicit() {
            expected_sources.extend([x86_gpr(0), x86_gpr(2)]);
        }
        if kind.returns_mask() {
            expected_sources.push(VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))));
        }
        assert_eq!(op.kind.source_vregs(), expected_sources);
        assert_eq!(op.kind.dests().len(), 1);
        assert_eq!(op.kind.flags_written(), FlagSet::ALL_X86);
        assert!(op.kind.flags_read().is_empty());
        assert!(!op.kind.is_jit_safe());
        assert!(!op.is_jit_safe());
        #[cfg(feature = "smir-jit")]
        assert!(!crate::smir::lower::runtime::is_x86_native_vector_op(
            &op.kind
        ));
    }
}

#[test]
fn legacy_pcmpestri_rex_w_selects_high_xmm_registers_and_64_bit_lengths() {
    let result = lift_single(&[0x66, 0x4D, 0x0F, 0x3A, 0x61, 0xD1, 0x00]).unwrap();
    assert_eq!(result.bytes_consumed, 7);
    assert!(matches!(
        exact_compare(&result).kind,
        OpKind::X86PackedStringCompare {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(10))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
            len1: Some(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
            len2: Some(VReg::Arch(ArchReg::X86(X86Reg::Rdx))),
            length_width: OpWidth::W64,
            kind: X86PackedStringKind::ExplicitIndex,
            ..
        }
    ));

    // A REX prefix before the mandatory 66 prefix is invalidated by the later
    // legacy prefix and therefore neither extends XMM registers nor lengths.
    let invalidated = lift_single(&[0x4D, 0x66, 0x0F, 0x3A, 0x61, 0xD1, 0x00]).unwrap();
    assert!(matches!(
        exact_compare(&invalidated).kind,
        OpKind::X86PackedStringCompare {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            length_width: OpWidth::W32,
            ..
        }
    ));
}

#[test]
fn legacy_pcmpxstrx_tracks_rip_addr32_segment_and_unaligned_memory_contracts() {
    let rip = lift_single(&[0x66, 0x0F, 0x3A, 0x63, 0x05, 0x20, 0x00, 0x00, 0x00, 0x3A]).unwrap();
    assert_eq!(rip.bytes_consumed, 10);
    assert!(rip.ops.iter().any(|op| matches!(
        &op.kind,
        OpKind::VLoad {
            addr: Address::PcRel {
                offset: 0x20,
                base: Some(0x100A),
                ..
            },
            width: VecWidth::V128,
            ..
        }
    )));
    assert!(
        !rip.ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );

    let addr32 = lift_single(&[0x64, 0x67, 0x66, 0x0F, 0x3A, 0x62, 0x04, 0x88, 0x40]).unwrap();
    assert_eq!(addr32.bytes_consumed, 9);
    assert!(addr32.ops.iter().any(|op| matches!(
        &op.kind,
        OpKind::VLoad {
            addr: Address::X86Addr32(inner),
            width: VecWidth::V128,
            ..
        } if matches!(
            inner.as_ref(),
            Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                base: Some(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
                index: Some(VReg::Arch(ArchReg::X86(X86Reg::Rcx))),
                scale: 4,
                disp: 0,
            }
        )
    )));
}

#[test]
fn legacy_pcmpxstrx_rejects_invalid_and_incomplete_encodings() {
    for bytes in [
        &[0x0F, 0x3A, 0x60, 0xD1, 0x00][..],
        &[0xF2, 0x66, 0x0F, 0x3A, 0x61, 0xD1, 0x00][..],
        &[0x66, 0xF2, 0x0F, 0x3A, 0x61, 0xD1, 0x00][..],
        &[0xF3, 0x66, 0x0F, 0x3A, 0x62, 0xD1, 0x00][..],
        &[0x66, 0xF3, 0x0F, 0x3A, 0x62, 0xD1, 0x00][..],
        &[0xF0, 0x66, 0x0F, 0x3A, 0x63, 0xD1, 0x00][..],
        &[0x66, 0xD5, 0x00, 0x0F, 0x3A, 0x63, 0xD1, 0x00][..],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::InvalidEncoding { .. })),
            "invalid packed-string encoding accepted: {bytes:02X?}",
        );
    }
    for bytes in [
        &[0x66, 0x0F, 0x3A, 0x60][..],
        &[0x66, 0x0F, 0x3A, 0x60, 0xD1][..],
        &[0x66, 0x0F, 0x3A, 0x60, 0x84, 0x88, 0x00, 0x00][..],
    ] {
        assert!(
            matches!(lift_single(bytes), Err(LiftError::Incomplete { .. })),
            "truncated packed-string encoding did not report Incomplete: {bytes:02X?}",
        );
    }
}

#[test]
fn legacy_pcmpxstrx_family_stays_in_one_strict_block_before_hlt_frontier() {
    let memory = TestMemory::new(
        0x2000,
        vec![
            0x66, 0x0F, 0x3A, 0x60, 0xD1, 0x00, // PCMPESTRM
            0x66, 0x0F, 0x3A, 0x61, 0xD1, 0x00, // PCMPESTRI
            0x66, 0x0F, 0x3A, 0x62, 0xD1, 0x00, // PCMPISTRM
            0x66, 0x0F, 0x3A, 0x63, 0xD1, 0x00, // PCMPISTRI
            0xF4,
        ],
    );
    let mut lifter = X86_64Lifter::strict();
    lifter.set_interpreter_frontiers(true);
    let mut context = LiftContext::new(SourceArch::X86_64);
    let function = lifter.lift_function(0x2000, &memory, &mut context).unwrap();
    let entry = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x2000)
        .unwrap();
    let frontier = function
        .blocks
        .iter()
        .find(|block| block.guest_pc == 0x2018)
        .expect("exact HLT interpreter frontier");
    assert_eq!(
        entry
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::X86PackedStringCompare { .. }))
            .count(),
        4
    );
    assert!(matches!(
        entry.terminator,
        Terminator::Branch { target } if target == frontier.id
    ));
    assert!(frontier.ops.is_empty());
    assert!(matches!(frontier.terminator, Terminator::Return { .. }));
}

#[test]
fn packed_string_interpreter_matches_hard_coded_byte_mask_index_and_flags() {
    let mut mask_context = SmirContext::new_x86_64();
    set_xmm(&mut mask_context, 2, xmm_bytes(b"abc"));
    set_xmm(&mut mask_context, 1, xmm_bytes(b"xbycz"));
    let mut old_xmm0 = [0u64; 16];
    for (index, lane) in old_xmm0.iter_mut().enumerate() {
        *lane = 0xA5A5_0000_0000_0000 | index as u64;
    }
    set_xmm(&mut mask_context, 0, old_xmm0);
    mask_context.write_vreg(x86_gpr(0), 3);
    mask_context.write_vreg(x86_gpr(2), 5);
    mask_context.flags.materialized = MaterializedFlags {
        df: true,
        pf: true,
        af: true,
        ..MaterializedFlags::default()
    };
    mask_context
        .flags
        .set_lazy_add(u64::MAX, 1, 0, OpWidth::W64);
    execute(
        &[0x66, 0x0F, 0x3A, 0x60, 0xD1, 0x00],
        &mut mask_context,
        &mut FlatMemory::new(1),
    );
    let mask = get_xmm(&mask_context, 0);
    assert_eq!(mask[0], 0x0A);
    assert_eq!(mask[1], 0);
    assert_eq!(&mask[2..], &old_xmm0[2..]);
    assert!(mask_context.flags.lazy.is_none());
    assert_eq!(
        mask_context.flags.materialized.to_rflags(),
        MaterializedFlags {
            cf: true,
            zf: true,
            sf: true,
            of: false,
            pf: false,
            af: false,
            df: true,
            ac: false,
        }
        .to_rflags()
    );

    let mut index_context = SmirContext::new_x86_64();
    set_xmm(&mut index_context, 2, xmm_bytes(b"abc"));
    set_xmm(&mut index_context, 1, xmm_bytes(b"xbycz"));
    index_context.write_vreg(x86_gpr(0), 3);
    index_context.write_vreg(x86_gpr(2), 5);
    index_context.write_vreg(x86_gpr(1), u64::MAX);
    execute(
        &[0x66, 0x0F, 0x3A, 0x61, 0xD1, 0x00],
        &mut index_context,
        &mut FlatMemory::new(1),
    );
    assert_eq!(index_context.read_vreg(x86_gpr(1)), 1);

    let mut expanded_context = SmirContext::new_x86_64();
    set_xmm(&mut expanded_context, 2, xmm_bytes(b"abc"));
    set_xmm(&mut expanded_context, 1, xmm_bytes(b"xbycz"));
    expanded_context.write_vreg(x86_gpr(0), 3);
    expanded_context.write_vreg(x86_gpr(2), 5);
    execute(
        &[0x66, 0x0F, 0x3A, 0x60, 0xD1, 0x40],
        &mut expanded_context,
        &mut FlatMemory::new(1),
    );
    assert_eq!(get_xmm(&expanded_context, 0)[0], 0x0000_0000_FF00_FF00);
}

#[test]
fn packed_string_interpreter_handles_implicit_strlen_signed_words_and_rex_w_lengths() {
    let mut strlen_context = SmirContext::new_x86_64();
    set_xmm(&mut strlen_context, 2, xmm_bytes(b"abc\0suffix"));
    execute(
        &[0x66, 0x0F, 0x3A, 0x63, 0xD2, 0x3A],
        &mut strlen_context,
        &mut FlatMemory::new(1),
    );
    assert_eq!(strlen_context.read_vreg(x86_gpr(1)), 3);

    let word_ranges = crate::isa::x86_64::execute::simd::pcmpxstrx::evaluate(
        0x0000_0000_0002_FFFE,
        0,
        0x0002_0000_FFFE_FFFD,
        0x0000_0000_0000_0003,
        2,
        5,
        0x07,
        true,
    );
    assert_eq!(word_ranges.value, 0x0E);
    assert!(word_ranges.cf && word_ranges.zf && word_ranges.sf);
    assert!(!word_ranges.of);

    for first_length in [0x0000_0001_0000_0000, i64::MIN as u64] {
        for (rex, expected) in [(&[][..], 16), (&[0x48][..], 0)] {
            let mut code = vec![0x66];
            code.extend_from_slice(rex);
            code.extend_from_slice(&[0x0F, 0x3A, 0x61, 0xD1, 0x00]);
            let mut context = SmirContext::new_x86_64();
            set_xmm(&mut context, 2, xmm_bytes(b"A"));
            set_xmm(&mut context, 1, xmm_bytes(b"A"));
            context.write_vreg(x86_gpr(0), first_length);
            context.write_vreg(x86_gpr(2), 1);
            execute(&code, &mut context, &mut FlatMemory::new(1));
            assert_eq!(context.read_vreg(x86_gpr(1)), expected);
        }
    }
}

#[test]
fn packed_string_memory_source_accepts_unaligned_m128_without_general_protection() {
    let mut context = SmirContext::new_x86_64();
    set_xmm(&mut context, 1, xmm_bytes(b"abc"));
    context.write_vreg(x86_gpr(0), 0x3001);
    let mut memory = FlatMemory::with_base(0x3000, 0x100);
    let mut source = [0u8; 16];
    source[..6].copy_from_slice(b"xbycz\0");
    memory.write(0x3001, &source).unwrap();
    execute(
        &[0x66, 0x0F, 0x3A, 0x63, 0x08, 0x00],
        &mut context,
        &mut memory,
    );
    assert_eq!(context.read_vreg(x86_gpr(1)), 1);
}

#[test]
fn packed_string_copy_propagation_o2_and_native_gate_remain_exact() {
    let mut copies = SmirBlock::new(BlockId(0), 0x1000);
    for (dst, src) in [
        (VReg::virt(4), VReg::virt(0)),
        (VReg::virt(5), VReg::virt(1)),
        (VReg::virt(6), VReg::virt(2)),
        (VReg::virt(7), VReg::virt(3)),
    ] {
        copies.push_op(SmirOp::new(
            OpId(copies.ops.len() as u16),
            0x1000,
            OpKind::Mov {
                dst,
                src: SrcOperand::Reg(src),
                width: OpWidth::W64,
            },
        ));
    }
    copies.push_op(SmirOp::new(
        OpId(4),
        0x1000,
        OpKind::X86PackedStringCompare {
            dst: VReg::virt(8),
            src1: VReg::virt(4),
            src2: VReg::virt(5),
            len1: Some(VReg::virt(6)),
            len2: Some(VReg::virt(7)),
            length_width: OpWidth::W64,
            kind: X86PackedStringKind::ExplicitIndex,
            imm: 0,
        },
    ));
    assert_eq!(crate::smir::optimize::copy_propagation(&mut copies), 4);
    assert!(matches!(
        copies.ops[4].kind,
        OpKind::X86PackedStringCompare {
            src1: VReg::Virtual(VirtualId(0)),
            src2: VReg::Virtual(VirtualId(1)),
            len1: Some(VReg::Virtual(VirtualId(2))),
            len2: Some(VReg::Virtual(VirtualId(3))),
            ..
        }
    ));

    let original = block_for(&[0x66, 0x0F, 0x3A, 0x61, 0xD1, 0x00]);
    let mut function = SmirFunction::new(FunctionId(0), original.id, 0x1000);
    function.add_block(original.clone());
    crate::smir::optimize::optimize_function(&mut function, crate::smir::optimize::OptLevel::O2);
    let optimized = function.entry_block().unwrap();
    let optimized_compare = optimized
        .ops
        .iter()
        .find(|op| matches!(op.kind, OpKind::X86PackedStringCompare { .. }))
        .expect("O2 must retain architectural result and flags");

    let seed = |context: &mut SmirContext| {
        set_xmm(context, 2, xmm_bytes(b"abc"));
        set_xmm(context, 1, xmm_bytes(b"xbycz"));
        context.write_vreg(x86_gpr(0), 3);
        context.write_vreg(x86_gpr(2), 5);
    };
    let mut original_context = SmirContext::new_x86_64();
    let mut optimized_context = SmirContext::new_x86_64();
    seed(&mut original_context);
    seed(&mut optimized_context);
    assert!(matches!(
        SmirInterpreter::new().execute_block(
            &mut original_context,
            &mut FlatMemory::new(1),
            &original,
        ),
        BlockResult::Exit(ExitReason::Halt)
    ));
    assert!(matches!(
        SmirInterpreter::new().execute_block(
            &mut optimized_context,
            &mut FlatMemory::new(1),
            optimized,
        ),
        BlockResult::Exit(ExitReason::Halt)
    ));
    assert_eq!(optimized_context.read_vreg(x86_gpr(1)), 1);
    assert_eq!(
        optimized_context.read_vreg(x86_gpr(1)),
        original_context.read_vreg(x86_gpr(1))
    );
    assert_eq!(
        optimized_context.flags.materialized.to_rflags(),
        original_context.flags.materialized.to_rflags()
    );
    assert!(!optimized_compare.kind.is_jit_safe());
    #[cfg(feature = "smir-jit")]
    assert!(!crate::smir::lower::runtime::is_x86_native_vector_op(
        &optimized_compare.kind
    ));
}

#[test]
fn malformed_packed_string_ir_exits_undefined_without_partial_writeback() {
    for op in [
        OpKind::X86PackedStringCompare {
            dst: x86_gpr(1),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            len1: None,
            len2: None,
            length_width: OpWidth::W32,
            kind: X86PackedStringKind::ExplicitIndex,
            imm: 0,
        },
        OpKind::X86PackedStringCompare {
            dst: x86_gpr(1),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            len1: Some(x86_gpr(0)),
            len2: Some(x86_gpr(2)),
            length_width: OpWidth::W64,
            kind: X86PackedStringKind::ImplicitIndex,
            imm: 0,
        },
    ] {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(SmirOp::new(OpId(0), 0x1000, op));
        block.set_terminator(Terminator::Trap {
            kind: TrapKind::Halt,
        });
        let mut context = SmirContext::new_x86_64();
        context.write_vreg(x86_gpr(1), 0xDEAD_BEEF);
        assert!(matches!(
            SmirInterpreter::new().execute_block(&mut context, &mut FlatMemory::new(1), &block,),
            BlockResult::Exit(ExitReason::Undefined {
                addr: 0x1000,
                opcode: 0,
            })
        ));
        assert_eq!(context.read_vreg(x86_gpr(1)), 0xDEAD_BEEF);
    }
}
