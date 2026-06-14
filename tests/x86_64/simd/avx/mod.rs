mod vaddps_vaddpd;
mod vandnps_vandnpd;
mod vandps_vandpd;
mod vblendps_vblendpd;
mod vbroadcastss_vbroadcastsd;
mod vcmpps_vcmppd;
mod vcvtdq2pd_vcvtpd2dq;
mod vcvtdq2ps_vcvtps2dq;
mod vcvtps2pd_vcvtpd2ps;
mod vcvtsi2ss_vcvtsi2sd;
mod vcvtss2sd_vcvtsd2ss;
mod vcvtss2si_vcvtsd2si;
mod vcvttps2dq_vcvttpd2dq;
mod vcvttss2si_vcvttsd2si;
mod vdivps_vdivpd;
mod vdpps;
mod vextractf128;
mod vextractf128_vinsertf128;
mod vhaddps_vhaddpd;
mod vhsubps_vhsubpd;
mod vinsertf128;
mod vldmxcsr_vstmxcsr;
mod vmaskmovps_vmaskmovpd;
mod vmaxps_vmaxpd;
mod vminps_vminpd;
mod vmovaps_vmovapd;
mod vmovdqa_vmovdqu;
mod vmovmskps_vmovmskpd;
mod vmovups_vmovupd;
mod vmulps_vmulpd;
mod vorps_vorpd;
mod vperm2f128;
mod vptest_vpxor;
mod vrcpps;
mod vrsqrtps;
mod vshufps_vshufpd;
mod vsqrtps_vsqrtpd;
mod vsubps_vsubpd;
mod vtestps_vtestpd;
mod vunpckhps_vunpckhpd;
mod vunpcklps_vunpcklpd;
mod vxorps_vxorpd;
mod vzeroupper_vzeroall;

// AVX Permute Instructions
mod vpermilpd;
mod vpermilps;

// AVX Blend Instructions
mod vblendvpd;
mod vblendvps;

// AVX Dot Product
mod vdppd;

// AVX Scalar Arithmetic
mod vaddss_vaddsd;
mod vdivss_vdivsd;
mod vmulss_vmulsd;
mod vsubss_vsubsd;

// AVX Add/Sub Packed
mod vaddsubps_vaddsubpd;

// AVX Rounding
mod vroundpd;
mod vroundps;
mod vroundsd;
mod vroundss;

// AVX Comparison
mod vcomisd;
mod vcomiss;
mod vucomisd;
mod vucomiss;

// AVX Scalar Move
mod vmovsd;
mod vmovss;

// AVX Move Variants
mod vmovhlps;
mod vmovhpd;
mod vmovhps;
mod vmovlhps;
mod vmovlpd;
mod vmovlps;

// AVX Duplicate Move
mod vmovddup;
mod vmovshdup;
mod vmovsldup;

// AVX Non-Temporal Move
mod vmovntdq;
mod vmovntdqa;
mod vmovntpd;
mod vmovntps;

// AVX FMA (Fused Multiply-Add) Instructions
mod vfmadd132pd;
mod vfmadd132ps;
mod vfmadd213pd;
mod vfmadd213ps;
mod vfmadd231pd;
mod vfmadd231ps;
mod vfmsub132pd;
mod vfmsub132ps;
mod vfmsub213pd;
mod vfmsub213ps;
mod vfmsub231pd;
mod vfmsub231ps;
mod vfnmadd132pd;
mod vfnmadd132ps;
mod vfnmadd213pd;
mod vfnmadd213ps;
mod vfnmadd231pd;
mod vfnmadd231ps;
mod vfnmsub132pd;
mod vfnmsub132ps;
mod vfnmsub213pd;
mod vfnmsub213ps;
mod vfnmsub231pd;
mod vfnmsub231ps;
