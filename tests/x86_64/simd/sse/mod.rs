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
