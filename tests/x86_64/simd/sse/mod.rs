mod movaps;
mod movups;
mod movapd;
mod movupd;
mod movdqa;
mod movdqu;
mod addps_addpd;
mod addss_addsd;
mod subps_subpd;
mod subss_subsd;
mod mulps_mulpd;
mod mulss_mulsd;
mod divps_divpd;
mod divss_divsd;
mod cmpps;
mod cmppd;
mod andps_andpd;
mod orps_orpd;
mod xorps_xorpd;
mod andnps_andnpd;

// SSE Mathematical Functions
mod sqrtps_sqrtpd;
mod sqrtss_sqrtsd;
mod maxps_maxpd;
mod minps_minpd;
mod maxss_maxsd;
mod minss_minsd;
mod rcpps;
mod rsqrtps;

// SSE Conversion Instructions
mod cvtps2pd;
mod cvtpd2ps;
mod cvtss2sd;
mod cvtsd2ss;
mod cvtsi2ss;
mod cvtsi2sd;
mod cvtss2si;
mod cvtsd2si;
mod cvtdq2ps_cvtps2dq;
mod cvtdq2pd_cvtpd2dq;
mod cvttps2dq_cvttpd2dq;

// SSE Comparison and Sign Mask Instructions
mod comiss_comisd;
mod ucomiss_ucomisd;
mod movmskps_movmskpd;

// SSE Scalar Comparison Instructions (Extended)
mod cmpss;
mod cmpsd;

// SSE Control/Status Register Instructions
mod ldmxcsr_stmxcsr;

// SSE Data Movement Instructions
mod movhlps_movlhps;
mod movd_movq;

// SSE Shuffle and Unpack Instructions
mod shufps;
mod shufpd;
mod unpcklps;
mod unpckhps;
mod unpcklpd;
mod unpckhpd;

// SSE2 Packed Integer Arithmetic Instructions
mod paddb_paddw_paddd_paddq;
mod psubb_psubw_psubd_psubq;
mod paddsb_paddsw;
mod paddusb_paddusw;
mod psubsb_psubsw;
mod psubusb_psubusw;
mod pavgb_pavgw;
mod psadbw;

// SSE2 Packed Integer Comparison Instructions
mod pcmpeqb_pcmpeqw_pcmpeqd;
mod pcmpgtb_pcmpgtw_pcmpgtd;

// SSE2 Packed Integer Logical Instructions
mod pand_por_pxor_pandn;

// SSE2 Packed Shift Instructions
mod psllw_pslld_psllq;
mod psrlw_psrld_psrlq;
mod psraw_psrad;
mod pslldq_psrldq;

// SSE2/SSSE3 Packed Shuffle Instructions
mod pshufd;
mod pshufb;
mod pshufhw;
mod pshuflw;

// SSE2/SSE4.1 Packed Integer Min/Max Instructions
mod pminsb_pminsw_pminsd;
mod pminub_pminuw_pminud;
mod pminub_pminuw_extended;
mod pmaxsb_pmaxsw_pmaxsd;
mod pmaxub_pmaxuw_pmaxud;
mod pmaxub_pmaxuw_extended;

// SSE2/SSE4.1 Packed Multiply Instructions
mod pmulhuw;
mod pmuludq;
mod pmuldq;

// SSSE3 Instructions
mod pabsb_pabsw_pabsd;
mod phaddw_phaddd;
mod phsubw_phsubd;
mod phaddsw_phsubsw;
mod pmaddubsw;
mod pmulhrsw;
mod psignb_psignw_psignd;
mod palignr;

// SSE2/SSE4.1 Packed Multiply and Add Instructions
mod pmaddwd;

// SSE4.1 Instructions
mod pblendw;
mod pblendvb;
mod blendps_blendpd;
mod blendvps_blendvpd;
mod roundps_roundpd;
mod roundss_roundsd;
mod phminposuw;
mod pcmpeqq;
mod dpps;
mod dppd;
mod mpsadbw;
mod pmulld;
mod ptest;
mod pextrb_pextrd_pextrq;
mod pextrw;
mod pinsrb_pinsrd_pinsrq;
mod pinsrw;
mod extractps;
mod insertps;

// SSE Special Instructions
mod rcpss;
mod rsqrtss;
mod maskmovdqu;
mod movntdq;
mod movnti;
mod lfence_mfence_sfence;

// SSE3 Move and Duplicate Instructions
mod movshdup_movsldup;
mod movddup;
mod lddqu;

// SSE4.1 Non-Temporal Load
mod movntdqa;

// SSE3 Horizontal Arithmetic Instructions
mod addsubps_addsubpd;
mod haddps_haddpd;
mod hsubps_hsubpd;
