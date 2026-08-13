//! Re-exports for exact legacy instruction replay classifiers.

pub(crate) use super::legacy_aes::{X86LegacyAesReplay, x86_legacy_aes_shape_virtual_requirements};
pub(crate) use super::legacy_blend::{
    X86LegacyBlendReplay, x86_legacy_blend_shape_virtual_requirements,
};
pub(crate) use super::legacy_dot_product::{
    X86LegacyDotProductReplay, x86_legacy_dot_product_shape_virtual_requirements,
};
pub(crate) use super::legacy_high_byte::{
    X86LegacyHighByteCrc32Replay, X86LegacyHighByteGroup2Kind, X86LegacyHighByteGroup2Replay,
    X86LegacyHighByteMultiplyKind, X86LegacyHighByteMultiplyReplay, X86LegacyHighByteSetccReplay,
    x86_legacy_high_byte_crc32_shape_temporary, x86_legacy_high_byte_multiply_shape_temporary,
    x86_legacy_high_byte_setcc_shape_virtual_requirements,
};
pub(crate) use super::legacy_packed_fp_convert::{
    X86LegacyPackedFpConvertKind, X86LegacyPackedFpConvertReplay,
    x86_legacy_packed_fp_convert_shape_matches,
};
pub(crate) use super::legacy_pclmulqdq::{
    X86LegacyPclmulqdqReplay, x86_legacy_pclmulqdq_shape_virtual_requirements,
};
pub(crate) use super::legacy_ptest::{
    X86LegacyPtestReplay, x86_legacy_ptest_shape_virtual_requirements,
};
pub(crate) use super::legacy_scalar_fp_convert::{
    X86LegacyScalarFpConvertKind, X86LegacyScalarFpConvertReplay,
    x86_legacy_scalar_fp_convert_shape_matches,
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
