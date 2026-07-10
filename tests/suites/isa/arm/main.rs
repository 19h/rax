// ARM ISA integration tests (subset wired into the harness).

// Existing hand-written tests
#[path = "a64/arithmetic/floating_point/half_precision/arithmetic/add/mod.rs"]
mod arm_a64_fp16_add;

// Auto-generated tests from ASL specifications
#[path = "../../../generated/arm/mod.rs"]
mod generated;
