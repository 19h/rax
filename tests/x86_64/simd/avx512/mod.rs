mod evex_rm_reg_ext;
mod kadd_mask;
mod kand_kor_kxor;
mod kandn_knot_mask;
mod kmov;
mod ktest_kunpck_kshift;
mod vaddps_zmm;
mod vcomish_vucomish;
mod vdivps_zmm;
mod vmovaps_zmm;
mod vmovups_zmm;
mod vmulps_zmm;
mod vsubps_zmm;

// AVX-512 FP16 Instructions
mod vaddph_vsubph_vmulph_vdivph;

// AVX-512 Compress/Expand Instructions
mod vcompress_vexpand;

// AVX-512 Bit Manipulation Instructions
mod valign_vprol_vpror_vpternlog;

// AVX-512 Specialized Instructions
mod vdbpsadbw_vplzcnt_vpshld;
