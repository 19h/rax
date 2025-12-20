#![cfg(feature = "x86_64-suite")]

// Aggregated test modules for the nested x86_64 instruction suites.

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
