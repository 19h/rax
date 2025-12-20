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

// SSE Comparison and Sign Mask Instructions
mod comiss_comisd;
mod ucomiss_ucomisd;
mod movmskps_movmskpd;

// SSE Control/Status Register Instructions
mod ldmxcsr_stmxcsr;

// SSE Data Movement Instructions
mod movhlps_movlhps;

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

// SSE2 Packed Integer Comparison Instructions
mod pcmpeqb_pcmpeqw_pcmpeqd;
mod pcmpgtb_pcmpgtw_pcmpgtd;

// SSE2 Packed Integer Logical Instructions
mod pand_por_pxor_pandn;
