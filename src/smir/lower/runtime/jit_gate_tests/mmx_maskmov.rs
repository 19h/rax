//! Helper-backed MMX MASKMOVQ admission tests.

use super::*;
use crate::smir::ir::ops::X86VecAlign;
use crate::smir::lower::runtime::*;

fn maskmovq_function(
    data_index: u8,
    mask_index: u8,
    segment: Option<X86Reg>,
    address_size_32: bool,
) -> crate::smir::ir::SmirFunction {
    let data = VReg::Arch(ArchReg::X86(X86Reg::Mm(data_index)));
    let mask = VReg::Arch(ArchReg::X86(X86Reg::Mm(mask_index)));
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    let address_base = if address_size_32 {
        let truncated = VReg::Virtual(VirtualId(99));
        builder.push_op(
            0x1000,
            OpKind::And {
                dst: truncated,
                src1: VReg::Arch(ArchReg::X86(X86Reg::Rdi)),
                src2: SrcOperand::Imm(0xFFFF_FFFF),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        truncated
    } else {
        VReg::Arch(ArchReg::X86(X86Reg::Rdi))
    };
    for lane in 0..8u8 {
        let (lane_address_base, disp) = if address_size_32 && lane != 0 {
            let wrapped = VReg::Virtual(VirtualId(100 + u32::from(lane)));
            builder.push_op(
                0x1000,
                OpKind::Add {
                    dst: wrapped,
                    src1: address_base,
                    src2: SrcOperand::Imm(i64::from(lane)),
                    width: OpWidth::W32,
                    flags: FlagUpdate::None,
                },
            );
            (wrapped, 0)
        } else {
            (
                address_base,
                if address_size_32 { 0 } else { i64::from(lane) },
            )
        };
        let mask_byte = VReg::Virtual(VirtualId(u32::from(lane) * 3));
        let active = VReg::Virtual(VirtualId(u32::from(lane) * 3 + 1));
        let data_byte = VReg::Virtual(VirtualId(u32::from(lane) * 3 + 2));
        builder.push_op(
            0x1000,
            OpKind::VExtractLane {
                dst: mask_byte,
                vec: mask,
                lane,
                elem: VecElementType::I8,
                sign: SignExtend::Zero,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::Shr {
                dst: active,
                src: mask_byte,
                amount: SrcOperand::Imm(7),
                width: OpWidth::W64,
                flags: FlagUpdate::None,
            },
        );
        builder.push_op(
            0x1000,
            OpKind::VExtractLane {
                dst: data_byte,
                vec: data,
                lane,
                elem: VecElementType::I8,
                sign: SignExtend::Zero,
            },
        );
        let addr = match segment {
            Some(segment) => Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(segment)),
                base: Some(lane_address_base),
                index: None,
                scale: 1,
                disp,
            },
            None => Address::base_off(lane_address_base, disp),
        };
        builder.push_op(
            0x1000,
            OpKind::PredStore {
                src: SrcOperand::Reg(data_byte),
                cond: active,
                addr,
                width: MemWidth::B1,
            },
        );
    }
    builder.push_op(
        0x1000,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EnterMmx,
            addr: None,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    builder.finish()
}

fn virtual_counts(
    function: &crate::smir::ir::SmirFunction,
) -> (
    std::collections::HashMap<VReg, usize>,
    std::collections::HashMap<VReg, usize>,
) {
    let mut definitions = std::collections::HashMap::new();
    let mut uses = std::collections::HashMap::new();
    for op in &function.blocks[0].ops {
        for reg in op.kind.dests() {
            if matches!(reg, VReg::Virtual(_)) {
                *definitions.entry(reg).or_insert(0) += 1;
            }
        }
        for reg in op.kind.source_vregs() {
            if matches!(reg, VReg::Virtual(_)) {
                *uses.entry(reg).or_insert(0) += 1;
            }
        }
    }
    (definitions, uses)
}

fn sequence(
    function: &crate::smir::ir::SmirFunction,
    allow_mem: bool,
) -> Option<X86MmxMaskmovqSequence> {
    let (definitions, uses) = virtual_counts(function);
    x86_jit_mmx_maskmovq_sequence(&function.blocks[0], 0, allow_mem, &definitions, &uses)
}

fn assert_rejected(function: &crate::smir::ir::SmirFunction) {
    let excluded = std::collections::HashMap::new();
    assert!(
        sequence(function, true).is_none(),
        "{:#?}",
        function.blocks[0].ops
    );
    assert!(!is_native_clobber_safe_excluding(function, &excluded, true));
    assert!(!x86_native_mmx_pairs_valid_excluding(function, &excluded));
}

#[test]
fn x86_mmx_maskmovq_gate_accepts_only_exact_helper_backed_sequence() {
    let excluded = std::collections::HashMap::new();
    for (data_index, mask_index, segment) in [
        (0, 1, None),
        (7, 0, Some(X86Reg::FsBase)),
        (3, 3, Some(X86Reg::GsBase)),
    ] {
        let function = maskmovq_function(data_index, mask_index, segment, false);
        let exact = sequence(&function, true).expect("exact MASKMOVQ sequence");
        assert_eq!(exact.consumed, 33);
        assert_eq!(exact.marker_offset, 32);
        assert_eq!(exact.data_index, data_index);
        assert_eq!(exact.mask_index, mask_index);
        assert!(!exact.address_size_32);
        assert!(is_native_clobber_safe_excluding(&function, &excluded, true));
        assert!(!is_native_clobber_safe_excluding(
            &function, &excluded, false
        ));
        assert!(x86_native_mmx_pairs_valid_excluding(&function, &excluded));
        assert!(uses_x86_native_mmx_excluding(&function, &excluded));
        assert!(!uses_x86_native_vectors_excluding(&function, &excluded));
        assert!(sequence(&function, false).is_none());
    }
}

#[test]
fn x86_mmx_maskmovq_gate_accepts_exact_address_size_override_sequence() {
    let excluded = std::collections::HashMap::new();
    for segment in [None, Some(X86Reg::FsBase), Some(X86Reg::GsBase)] {
        let function = maskmovq_function(7, 2, segment, true);
        let exact = sequence(&function, true).expect("exact addr32 MASKMOVQ sequence");
        assert_eq!(exact.consumed, 41);
        assert_eq!(exact.marker_offset, 40);
        assert_eq!(exact.data_index, 7);
        assert_eq!(exact.mask_index, 2);
        assert!(exact.address_size_32);
        assert!(is_native_clobber_safe_excluding(&function, &excluded, true));
        assert!(!is_native_clobber_safe_excluding(
            &function, &excluded, false
        ));
        assert!(x86_native_mmx_pairs_valid_excluding(&function, &excluded));
        assert!(uses_x86_native_mmx_excluding(&function, &excluded));
        assert!(!uses_x86_native_vectors_excluding(&function, &excluded));
    }
}

#[test]
fn x86_mmx_maskmovq_gate_rejects_malformed_address_size_override_state() {
    let exact = maskmovq_function(7, 2, Some(X86Reg::FsBase), true);
    let mut malformed = Vec::new();

    let mut wrong_mask = exact.clone();
    if let OpKind::And { src2, .. } = &mut wrong_mask.blocks[0].ops[0].kind {
        *src2 = SrcOperand::Imm(0xFFFF_FFFE);
    }
    malformed.push(wrong_mask);

    let mut wrong_width = exact.clone();
    if let OpKind::And { width, .. } = &mut wrong_width.blocks[0].ops[0].kind {
        *width = OpWidth::W32;
    }
    malformed.push(wrong_width);

    let mut flagful = exact.clone();
    if let OpKind::And { flags, .. } = &mut flagful.blocks[0].ops[0].kind {
        *flags = FlagUpdate::All;
    }
    malformed.push(flagful);

    let mut hinted = exact.clone();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(hinted);

    let mut mixed_base = exact.clone();
    if let OpKind::PredStore { addr, .. } = &mut mixed_base.blocks[0].ops[4].kind {
        *addr = Address::SegmentRel {
            segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
            base: Some(VReg::Arch(ArchReg::X86(X86Reg::Rdi))),
            index: None,
            scale: 1,
            disp: 0,
        };
    }
    malformed.push(mixed_base);

    let mut reused_truncated = exact.clone();
    if let OpKind::VExtractLane { vec, .. } = &mut reused_truncated.blocks[0].ops[1].kind {
        *vec = VReg::Virtual(VirtualId(99));
    }
    malformed.push(reused_truncated);

    let mut wrong_lane_wrap = exact.clone();
    if let OpKind::Add { src2, .. } = &mut wrong_lane_wrap.blocks[0].ops[5].kind {
        *src2 = SrcOperand::Imm(2);
    }
    malformed.push(wrong_lane_wrap);

    let mut wrong_wrap_width = exact.clone();
    if let OpKind::Add { width, .. } = &mut wrong_wrap_width.blocks[0].ops[5].kind {
        *width = OpWidth::W64;
    }
    malformed.push(wrong_wrap_width);

    for function in malformed {
        assert_rejected(&function);
    }
}

#[test]
fn x86_mmx_maskmovq_gate_rejects_malformed_lane_state_and_address_shapes() {
    let exact = maskmovq_function(0, 1, None, false);
    let mut malformed = Vec::new();

    let mut wrong_data_class = exact.clone();
    if let OpKind::VExtractLane { vec, .. } = &mut wrong_data_class.blocks[0].ops[2].kind {
        *vec = VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)));
    }
    malformed.push(wrong_data_class);

    let mut inconsistent_mask = exact.clone();
    if let OpKind::VExtractLane { vec, .. } = &mut inconsistent_mask.blocks[0].ops[4].kind {
        *vec = VReg::Arch(ArchReg::X86(X86Reg::Mm(2)));
    }
    malformed.push(inconsistent_mask);

    let mut wrong_lane = exact.clone();
    if let OpKind::VExtractLane { lane, .. } = &mut wrong_lane.blocks[0].ops[8].kind {
        *lane = 7;
    }
    malformed.push(wrong_lane);

    let mut wrong_extract_width = exact.clone();
    if let OpKind::VExtractLane { elem, .. } = &mut wrong_extract_width.blocks[0].ops[0].kind {
        *elem = VecElementType::I16;
    }
    malformed.push(wrong_extract_width);

    let mut wrong_extract_sign = exact.clone();
    if let OpKind::VExtractLane { sign, .. } = &mut wrong_extract_sign.blocks[0].ops[0].kind {
        *sign = SignExtend::Sign;
    }
    malformed.push(wrong_extract_sign);

    let mut wrong_mask_bit = exact.clone();
    if let OpKind::Shr { amount, .. } = &mut wrong_mask_bit.blocks[0].ops[1].kind {
        *amount = SrcOperand::Imm(6);
    }
    malformed.push(wrong_mask_bit);

    let mut flagful_shift = exact.clone();
    if let OpKind::Shr { flags, .. } = &mut flagful_shift.blocks[0].ops[1].kind {
        *flags = FlagUpdate::All;
    }
    malformed.push(flagful_shift);

    let mut wrong_store_source = exact.clone();
    if let OpKind::PredStore { src, .. } = &mut wrong_store_source.blocks[0].ops[3].kind {
        *src = SrcOperand::Reg(VReg::Virtual(VirtualId(0)));
    }
    malformed.push(wrong_store_source);

    let mut wrong_store_width = exact.clone();
    if let OpKind::PredStore { width, .. } = &mut wrong_store_width.blocks[0].ops[3].kind {
        *width = MemWidth::B2;
    }
    malformed.push(wrong_store_width);

    let mut wrong_base = exact.clone();
    if let OpKind::PredStore { addr, .. } = &mut wrong_base.blocks[0].ops[3].kind {
        *addr = Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rbx)));
    }
    malformed.push(wrong_base);

    let mut wrong_offset = exact.clone();
    if let OpKind::PredStore { addr, .. } = &mut wrong_offset.blocks[0].ops[7].kind {
        *addr = Address::base_off(VReg::Arch(ArchReg::X86(X86Reg::Rdi)), 7);
    }
    malformed.push(wrong_offset);

    let mut wrong_disp_size = exact.clone();
    if let OpKind::PredStore {
        addr: Address::BaseOffset { disp_size, .. },
        ..
    } = &mut wrong_disp_size.blocks[0].ops[3].kind
    {
        *disp_size = DispSize::Disp8;
    }
    malformed.push(wrong_disp_size);

    let mut mixed_segment = exact.clone();
    if let OpKind::PredStore { addr, .. } = &mut mixed_segment.blocks[0].ops[7].kind {
        *addr = Address::SegmentRel {
            segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
            base: Some(VReg::Arch(ArchReg::X86(X86Reg::Rdi))),
            index: None,
            scale: 1,
            disp: 1,
        };
    }
    malformed.push(mixed_segment);

    let mut virtual_address = exact.clone();
    for lane in 0..8usize {
        if let OpKind::PredStore { addr, .. } =
            &mut virtual_address.blocks[0].ops[lane * 4 + 3].kind
        {
            *addr = Address::base_off(VReg::Virtual(VirtualId(99)), lane as i64);
        }
    }
    malformed.push(virtual_address);

    let mut hinted = exact.clone();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::VecAlign(X86VecAlign::Unaligned));
    malformed.push(hinted);

    let mut wrong_pc = exact.clone();
    wrong_pc.blocks[0].ops[32].guest_pc = 0x1001;
    malformed.push(wrong_pc);

    let mut marker_before_store = exact.clone();
    marker_before_store.blocks[0].ops.swap(31, 32);
    malformed.push(marker_before_store);

    let mut reused_temporary = exact.clone();
    if let OpKind::VExtractLane { dst, .. } = &mut reused_temporary.blocks[0].ops[4].kind {
        *dst = VReg::Virtual(VirtualId(0));
    }
    malformed.push(reused_temporary);

    for function in malformed {
        assert_rejected(&function);
    }
}
