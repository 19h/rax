// End-to-end x86-64 JIT coverage modules.

#[path = "ah_flags.rs"]
mod ah_flags;
#[path = "amx_disabled.rs"]
mod amx_disabled;
#[path = "apx_bmi.rs"]
mod apx_bmi;
#[path = "apx_cet.rs"]
mod apx_cet;
#[path = "apx_movrs.rs"]
mod apx_movrs;
#[path = "apx_nf_reserved.rs"]
mod apx_nf_reserved;
#[path = "apx_push2_pop2.rs"]
mod apx_push2_pop2;
#[path = "apx_reserved.rs"]
mod apx_reserved;
#[path = "cmpccxadd.rs"]
mod cmpccxadd;
#[path = "cmpxchg_register.rs"]
mod cmpxchg_register;
#[path = "flag_control.rs"]
mod flag_control;
#[path = "group3_alias.rs"]
mod group3_alias;
#[path = "legacy_0f38_terminal.rs"]
mod legacy_0f38_terminal;
#[path = "legacy_0f3a_reserved.rs"]
mod legacy_0f3a_reserved;
#[path = "legacy_dot_product.rs"]
mod legacy_dot_product;
#[path = "legacy_fp_round.rs"]
mod legacy_fp_round;
#[path = "legacy_high_byte.rs"]
mod legacy_high_byte;
#[path = "legacy_insertps.rs"]
mod legacy_insertps;
#[path = "legacy_alignr.rs"]
mod legacy_alignr;
#[path = "legacy_lane_shuffle.rs"]
mod legacy_lane_shuffle;
#[path = "legacy_pclmulqdq.rs"]
mod legacy_pclmulqdq;
#[path = "legacy_ptest.rs"]
mod legacy_ptest;
#[path = "legacy_packed_extend.rs"]
mod legacy_packed_extend;
#[path = "legacy_packed_shift.rs"]
mod legacy_packed_shift;
#[path = "legacy_packed_fp_convert.rs"]
mod legacy_packed_fp_convert;
#[path = "legacy_scalar_fp_convert.rs"]
mod legacy_scalar_fp_convert;
#[path = "legacy_scalar_extract.rs"]
mod legacy_scalar_extract;
#[path = "legacy_scalar_insert.rs"]
mod legacy_scalar_insert;
#[path = "legacy_widening_dword_multiply.rs"]
mod legacy_widening_dword_multiply;
#[path = "mmx_xmm_transfer.rs"]
mod mmx_xmm_transfer;
#[path = "multiply_register.rs"]
mod multiply_register;
#[path = "ordinary_stack.rs"]
mod ordinary_stack;
#[path = "rdpid.rs"]
mod rdpid;
#[path = "smc.rs"]
mod smc;
#[path = "sse4a_bitfield.rs"]
mod sse4a_bitfield;
#[path = "tbm.rs"]
mod tbm;
#[path = "three_dnow_reserved.rs"]
mod three_dnow_reserved;
#[path = "vector_legacy_prefix_reserved.rs"]
mod vector_legacy_prefix_reserved;
#[path = "vector_prefix_reserved.rs"]
mod vector_prefix_reserved;
#[path = "vex_bmi_reserved.rs"]
mod vex_bmi_reserved;
#[path = "xchg_register.rs"]
mod xchg_register;
