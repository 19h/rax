//! Optimizer contracts for AVX10.2 saturating-conversion effects.

use super::*;

fn conversion(
    mask: Option<VReg>,
    zeroing: bool,
    suppress_exceptions: bool,
    width: VecWidth,
) -> OpKind {
    OpKind::VCvtFpToIntSat {
        dst: VReg::virt(2),
        src: VReg::virt(0),
        mask,
        fp_elem: VecElementType::F32,
        int_elem: VecElementType::I8,
        width,
        signed: true,
        truncate: true,
        round: FpRoundMode::RoundTowardZero,
        zeroing,
        suppress_exceptions,
    }
}

fn rounded_conversion(round: FpRoundMode, suppress_exceptions: bool, width: VecWidth) -> OpKind {
    OpKind::VCvtFpToIntSat {
        dst: VReg::virt(2),
        src: VReg::virt(0),
        mask: None,
        fp_elem: VecElementType::F32,
        int_elem: VecElementType::I8,
        width,
        signed: false,
        truncate: false,
        round,
        zeroing: false,
        suppress_exceptions,
    }
}

#[test]
fn saturating_conversion_metadata_tracks_merge_mask_and_mxcsr_effects() {
    let merging = conversion(Some(VReg::virt(1)), false, false, VecWidth::V128);
    assert_eq!(merging.dests(), vec![VReg::virt(2)]);
    assert_eq!(
        merging.source_vregs(),
        vec![VReg::virt(0), VReg::virt(1), VReg::virt(2)]
    );
    assert!(merging.has_side_effects());
    assert!(!merging.is_jit_safe());
    assert!(!make_op(0, merging).is_jit_safe());

    let zeroing = conversion(Some(VReg::virt(1)), true, false, VecWidth::V128);
    assert_eq!(zeroing.source_vregs(), vec![VReg::virt(0), VReg::virt(1)]);
    assert!(zeroing.has_side_effects());

    let sae = conversion(Some(VReg::virt(1)), false, true, VecWidth::V512);
    assert!(!sae.has_side_effects());
    assert!(!sae.is_jit_safe());

    let malformed = conversion(None, true, true, VecWidth::V128);
    assert!(malformed.has_side_effects());

    let mxcsr_rounded = rounded_conversion(FpRoundMode::Dynamic, false, VecWidth::V128);
    assert!(mxcsr_rounded.has_side_effects());

    let embedded = rounded_conversion(FpRoundMode::RoundDown, true, VecWidth::V512);
    assert!(!embedded.has_side_effects());

    for malformed in [
        rounded_conversion(FpRoundMode::Dynamic, true, VecWidth::V512),
        rounded_conversion(FpRoundMode::RoundUp, false, VecWidth::V512),
        rounded_conversion(FpRoundMode::RoundNearest, true, VecWidth::V128),
        rounded_conversion(FpRoundMode::RoundNearestTiesAway, true, VecWidth::V512),
    ] {
        assert!(malformed.has_side_effects());
    }
}

#[test]
fn dce_preserves_status_and_malformed_boundaries_but_removes_dead_sae_result() {
    for operation in [
        conversion(None, false, false, VecWidth::V128),
        conversion(None, true, true, VecWidth::V128),
    ] {
        let mut block = SmirBlock::new(BlockId(0), 0x1000);
        block.push_op(make_op(0, operation));
        block.set_terminator(Terminator::Return { values: vec![] });
        assert_eq!(dead_code_elimination(&mut block), 0);
        assert_eq!(block.ops.len(), 1);
    }

    let mut sae = SmirBlock::new(BlockId(0), 0x1000);
    sae.push_op(make_op(0, conversion(None, false, true, VecWidth::V512)));
    sae.set_terminator(Terminator::Return { values: vec![] });
    assert_eq!(dead_code_elimination(&mut sae), 1);
    assert!(sae.ops.is_empty());

    let mut embedded = SmirBlock::new(BlockId(0), 0x1000);
    embedded.push_op(make_op(
        0,
        rounded_conversion(FpRoundMode::RoundDown, true, VecWidth::V512),
    ));
    embedded.set_terminator(Terminator::Return { values: vec![] });
    assert_eq!(dead_code_elimination(&mut embedded), 1);
    assert!(embedded.ops.is_empty());
}
