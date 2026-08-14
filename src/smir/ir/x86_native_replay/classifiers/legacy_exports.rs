//! Re-exports for exact legacy instruction replay classifiers.

pub(crate) use super::legacy_aes::{X86LegacyAesReplay, x86_legacy_aes_shape_virtual_requirements};
pub(crate) use super::legacy_alignr::{
    X86LegacyAlignrReplay, x86_legacy_alignr_shape_virtual_requirements,
};
pub(crate) use super::legacy_blend::{
    X86LegacyBlendReplay, x86_legacy_blend_shape_virtual_requirements,
};
pub(crate) use super::legacy_dot_product::{
    X86LegacyDotProductReplay, x86_legacy_dot_product_shape_virtual_requirements,
};
pub(crate) use super::legacy_gfni::{
    X86LegacyGfniReplay, x86_legacy_gfni_shape_virtual_requirements,
};
pub(crate) use super::legacy_high_byte::{
    X86LegacyHighByteCrc32Replay, X86LegacyHighByteGroup2Kind, X86LegacyHighByteGroup2Replay,
    X86LegacyHighByteMultiplyKind, X86LegacyHighByteMultiplyReplay, X86LegacyHighByteSetccReplay,
    x86_legacy_high_byte_crc32_shape_temporary, x86_legacy_high_byte_group3_test_shape_temporary,
    x86_legacy_high_byte_multiply_shape_temporary,
    x86_legacy_high_byte_setcc_shape_virtual_requirements,
};
pub(crate) use super::legacy_lane_shuffle::{
    X86LegacyLaneShuffleKind, X86LegacyLaneShuffleReplay,
    x86_legacy_lane_shuffle_shape_virtual_requirements,
};
pub(crate) use super::legacy_packed_fp_convert::{
    X86LegacyPackedFpConvertKind, X86LegacyPackedFpConvertReplay,
    x86_legacy_packed_fp_convert_shape_matches,
};
pub(crate) use super::legacy_packed_shift::{
    X86LegacyPackedShiftCount, X86LegacyPackedShiftReplay,
    x86_legacy_packed_shift_shape_virtual_requirements,
};
pub(crate) use super::legacy_pclmulqdq::{
    X86LegacyPclmulqdqReplay, x86_legacy_pclmulqdq_shape_virtual_requirements,
};
pub(crate) use super::legacy_ptest::{
    X86LegacyPtestReplay, x86_legacy_ptest_shape_virtual_requirements,
};
pub(crate) use super::legacy_scalar_extract::{
    X86LegacyScalarExtractKind, X86LegacyScalarExtractReplay,
    x86_legacy_scalar_extract_shape_virtual_requirements,
};
pub(crate) use super::legacy_scalar_fp_convert::{
    X86LegacyScalarFpConvertKind, X86LegacyScalarFpConvertReplay,
    x86_legacy_scalar_fp_convert_shape_matches,
};
pub(crate) use super::legacy_scalar_insert::{
    X86LegacyScalarInsertKind, X86LegacyScalarInsertReplay,
    x86_legacy_scalar_insert_shape_virtual_requirements,
};
pub(crate) use super::legacy_scalar_xmm_movq::{
    X86LegacyScalarXmmMovqReplay, x86_legacy_scalar_xmm_movq_shape_virtual_requirements,
};
pub(crate) use super::legacy_sha::{X86LegacyShaReplay, x86_legacy_sha_shape_virtual_requirements};
pub(crate) use super::legacy_widening_dword_multiply::{
    X86LegacyWideningDwordMultiplyReplay,
    x86_legacy_widening_dword_multiply_shape_virtual_requirements,
};
pub(crate) use super::packed_extend::{
    X86LegacyPackedExtendReplay, x86_legacy_packed_extend_shape_virtual_requirements,
};
pub(crate) use super::scalar_lane_transfer::{
    X86LegacyInsertpsReplay, x86_legacy_insertps_shape_virtual_requirements,
};
