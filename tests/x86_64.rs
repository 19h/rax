#![cfg(feature = "x86_64-suite")]

// Aggregated test modules for the nested x86_64 instruction suites.

// Common utilities
#[path = "x86_64/common/mod.rs"]
mod common;

// Arithmetic
#[path = "x86_64/arithmetic/integer_addition_carry/add.rs"]
mod arithmetic_integer_addition_carry_add;
#[path = "x86_64/arithmetic/integer_addition_carry/adc.rs"]
mod arithmetic_integer_addition_carry_adc;
#[path = "x86_64/arithmetic/comparison/cmp.rs"]
mod arithmetic_comparison_cmp;
#[path = "x86_64/arithmetic/integer_division/div.rs"]
mod arithmetic_integer_division_div;
#[path = "x86_64/arithmetic/integer_division/idiv.rs"]
mod arithmetic_integer_division_idiv;
#[path = "x86_64/arithmetic/integer_multiplication/imul.rs"]
mod arithmetic_integer_multiplication_imul;
#[path = "x86_64/arithmetic/integer_multiplication/mul.rs"]
mod arithmetic_integer_multiplication_mul;
#[path = "x86_64/arithmetic/integer_subtraction_base/sub.rs"]
mod arithmetic_integer_subtraction_base_sub;
#[path = "x86_64/arithmetic/integer_subtraction/dec.rs"]
mod arithmetic_integer_subtraction_dec;
#[path = "x86_64/arithmetic/integer_subtraction/inc.rs"]
mod arithmetic_integer_subtraction_inc;
#[path = "x86_64/arithmetic/integer_subtraction/neg.rs"]
mod arithmetic_integer_subtraction_neg;
#[path = "x86_64/arithmetic/integer_subtraction/sbb.rs"]
mod arithmetic_integer_subtraction_sbb;

// Comprehensive arithmetic tests
#[path = "x86_64/arithmetic/neg.rs"]
mod arithmetic_neg;
#[path = "x86_64/arithmetic/inc_dec.rs"]
mod arithmetic_inc_dec;
#[path = "x86_64/arithmetic/imul.rs"]
mod arithmetic_imul;
#[path = "x86_64/arithmetic/mul.rs"]
mod arithmetic_mul;
#[path = "x86_64/arithmetic/div.rs"]
mod arithmetic_div;
#[path = "x86_64/arithmetic/idiv.rs"]
mod arithmetic_idiv;

// Logic and bit manipulation
#[path = "x86_64/logic_and_bit_manipulation/basic_logic/and.rs"]
mod logic_basic_and;
#[path = "x86_64/logic_and_bit_manipulation/basic_logic/not.rs"]
mod logic_basic_not;
#[path = "x86_64/logic_and_bit_manipulation/basic_logic/or.rs"]
mod logic_basic_or;
#[path = "x86_64/logic_and_bit_manipulation/basic_logic/test.rs"]
mod logic_basic_test;
#[path = "x86_64/logic_and_bit_manipulation/basic_logic_xor/xor.rs"]
mod logic_basic_xor;
#[path = "x86_64/logic_and_bit_manipulation/bit_testing/bt.rs"]
mod logic_bit_testing_bt;
#[path = "x86_64/logic_and_bit_manipulation/bit_testing/btc.rs"]
mod logic_bit_testing_btc;
#[path = "x86_64/logic_and_bit_manipulation/bit_testing/btr.rs"]
mod logic_bit_testing_btr;
#[path = "x86_64/logic_and_bit_manipulation/bit_testing/bts.rs"]
mod logic_bit_testing_bts;
// New bit test module with comprehensive coverage
#[path = "x86_64/bit/bt.rs"]
mod bit_test_bt;
#[path = "x86_64/bit/btc.rs"]
mod bit_test_btc;
#[path = "x86_64/bit/btr.rs"]
mod bit_test_btr;
#[path = "x86_64/bit/bts.rs"]
mod bit_test_bts;
#[path = "x86_64/logic_and_bit_manipulation/bit_scanning/bsf.rs"]
mod logic_bit_scanning_bsf;
#[path = "x86_64/logic_and_bit_manipulation/bit_scanning/bsr.rs"]
mod logic_bit_scanning_bsr;
#[path = "x86_64/logic_and_bit_manipulation/rotates_basic/rcl.rs"]
mod logic_rotates_basic_rcl;
#[path = "x86_64/logic_and_bit_manipulation/rotates_basic/rcr.rs"]
mod logic_rotates_basic_rcr;
#[path = "x86_64/logic_and_bit_manipulation/rotates_basic/rol.rs"]
mod logic_rotates_basic_rol;
#[path = "x86_64/logic_and_bit_manipulation/rotates_basic/ror.rs"]
mod logic_rotates_basic_ror;
#[path = "x86_64/logic_and_bit_manipulation/rotates_advanced/rorx.rs"]
mod logic_rotates_advanced_rorx;
#[path = "x86_64/logic_and_bit_manipulation/shifts_arithmetic/sar.rs"]
mod logic_shifts_arithmetic_sar;
#[path = "x86_64/logic_and_bit_manipulation/shifts_double_precision/shld.rs"]
mod logic_shifts_double_precision_shld;
#[path = "x86_64/logic_and_bit_manipulation/shifts_double_precision/shrd.rs"]
mod logic_shifts_double_precision_shrd;
#[path = "x86_64/logic_and_bit_manipulation/shifts_logical/shl.rs"]
mod logic_shifts_logical_shl;
#[path = "x86_64/logic_and_bit_manipulation/shifts_logical/shr.rs"]
mod logic_shifts_logical_shr;
#[path = "x86_64/logic_and_bit_manipulation/shifts_variable/shlx.rs"]
mod logic_shifts_variable_shlx;
#[path = "x86_64/logic_and_bit_manipulation/shifts_variable/shrx.rs"]
mod logic_shifts_variable_shrx;
#[path = "x86_64/logic_and_bit_manipulation/shifts_variable/sarx.rs"]
mod logic_shifts_variable_sarx;

// BMI (Bit Manipulation Instructions)
#[path = "x86_64/bmi/blsi.rs"]
mod bmi_blsi;
#[path = "x86_64/bmi/blsmsk.rs"]
mod bmi_blsmsk;
#[path = "x86_64/bmi/blsr.rs"]
mod bmi_blsr;
#[path = "x86_64/bmi/andn.rs"]
mod bmi_andn;
#[path = "x86_64/bmi/bextr.rs"]
mod bmi_bextr;
#[path = "x86_64/bmi/mulx.rs"]
mod bmi_mulx;
#[path = "x86_64/bmi/popcnt.rs"]
mod bmi_popcnt;
#[path = "x86_64/bmi/sarx_shlx_shrx.rs"]
mod bmi_sarx_shlx_shrx;
#[path = "x86_64/bmi/sarx_shlx_shrx_extended.rs"]
mod bmi_sarx_shlx_shrx_extended;
#[path = "x86_64/bmi/rorx.rs"]
mod bmi_rorx;
#[path = "x86_64/bmi/bzhi_extended.rs"]
mod bmi_bzhi_extended;

// Data Transfer
#[path = "x86_64/data_transfer/bswap.rs"]
mod data_transfer_bswap;
#[path = "x86_64/data_transfer/cmov.rs"]
mod data_transfer_cmov;
#[path = "x86_64/data_transfer/setcc.rs"]
mod data_transfer_setcc;

// Data Conversion
#[path = "x86_64/conversion/movsx.rs"]
mod conversion_movsx;
#[path = "x86_64/conversion/movsxd.rs"]
mod conversion_movsxd;
#[path = "x86_64/conversion/movzx.rs"]
mod conversion_movzx;
#[path = "x86_64/conversion/cbw_cwde_cdqe.rs"]
mod conversion_cbw_cwde_cdqe;
#[path = "x86_64/conversion/cwd_cdq_cqo.rs"]
mod conversion_cwd_cdq_cqo;
#[path = "x86_64/conversion/cvtpi2ps_cvtps2pi.rs"]
mod conversion_cvtpi2ps_cvtps2pi;
#[path = "x86_64/conversion/cvtpi2pd_cvtpd2pi.rs"]
mod conversion_cvtpi2pd_cvtpd2pi;
#[path = "x86_64/conversion/cvttps2pi_cvttpd2pi.rs"]
mod conversion_cvttps2pi_cvttpd2pi;
#[path = "x86_64/conversion/cvtss2si_cvtsd2si_extended.rs"]
mod conversion_cvtss2si_cvtsd2si_extended;

// Synchronization primitives
#[path = "x86_64/sync/cmpxchg.rs"]
mod sync_cmpxchg;

// Flag Manipulation
#[path = "x86_64/flags/clc_stc_cmc.rs"]
mod flags_clc_stc_cmc;
#[path = "x86_64/flags/cld_std.rs"]
mod flags_cld_std;
#[path = "x86_64/flags/lahf_sahf.rs"]
mod flags_lahf_sahf;
#[path = "x86_64/flags/pushf_popf.rs"]
mod flags_pushf_popf;

// Memory operations
#[path = "x86_64/memory/enter_leave.rs"]
mod memory_enter_leave;
#[path = "x86_64/memory/bound.rs"]
mod memory_bound;

// System instructions
#[path = "x86_64/system/cpuid.rs"]
mod system_cpuid;
#[path = "x86_64/system/lgdt_lidt.rs"]
mod system_lgdt_lidt;
#[path = "x86_64/system/sgdt_sidt.rs"]
mod system_sgdt_sidt;
#[path = "x86_64/system/lldt.rs"]
mod system_lldt;
#[path = "x86_64/system/sldt.rs"]
mod system_sldt;
#[path = "x86_64/system/ltr.rs"]
mod system_ltr;
#[path = "x86_64/system/str.rs"]
mod system_str;
#[path = "x86_64/system/verr_verw.rs"]
mod system_verr_verw;
#[path = "x86_64/system/lar.rs"]
mod system_lar;
#[path = "x86_64/system/lsl.rs"]
mod system_lsl;

// Stack operations
#[path = "x86_64/stack_operations/pusha_popa.rs"]
mod stack_operations_pusha_popa;

// Miscellaneous
#[path = "x86_64/misc/nop.rs"]
mod misc_nop;
#[path = "x86_64/misc/cpuid_extended.rs"]
mod misc_cpuid_extended;
#[path = "x86_64/misc/xgetbv_xsetbv.rs"]
mod misc_xgetbv_xsetbv;
#[path = "x86_64/misc/movbe_extended.rs"]
mod misc_movbe_extended;
#[path = "x86_64/misc/xsave_xrstor.rs"]
mod misc_xsave_xrstor;

// x87 FPU State Management and Status Instructions
#[path = "x86_64/fpu/fstsw_fnstsw.rs"]
mod fpu_fstsw_fnstsw;
#[path = "x86_64/fpu/fclex_fnclex.rs"]
mod fpu_fclex_fnclex;
#[path = "x86_64/fpu/finit_fninit.rs"]
mod fpu_finit_fninit;
#[path = "x86_64/fpu/fxsave_fxrstor.rs"]
mod fpu_fxsave_fxrstor;
#[path = "x86_64/fpu/fldenv_fstenv.rs"]
mod fpu_fldenv_fstenv;
#[path = "x86_64/fpu/fsave_frstor.rs"]
mod fpu_fsave_frstor;

// SSE/MMX Data Movement Between Integer and Floating-Point
#[path = "x86_64/simd/sse/movd_movq.rs"]
mod sse_movd_movq;
#[path = "x86_64/simd/sse/cvtdq2ps_cvtps2dq.rs"]
mod sse_cvtdq2ps_cvtps2dq;
#[path = "x86_64/simd/sse/cvtdq2pd_cvtpd2dq.rs"]
mod sse_cvtdq2pd_cvtpd2dq;
#[path = "x86_64/simd/sse/cvttps2dq_cvttpd2dq.rs"]
mod sse_cvttps2dq_cvttpd2dq;
