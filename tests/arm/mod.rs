//! ARM instruction test suite.
//!
//! This module provides comprehensive testing for ARMv7-A instruction execution,
//! organized into categories following the x86_64 test structure pattern.
//!
//! Test categories:
//! - common: Shared utilities and bit operations
//! - arithmetic: ADD, SUB, ADC, SBC, RSB
//! - logical: AND, ORR, EOR, BIC
//! - bitwise: CLZ, REV, RBIT, MOV
//! - branch: B, BL, BX, BLX
//! - load_store: LDR, STR, LDM, STM, PUSH, POP
//! - multiply: MUL, MLA, UMULL, SMULL
//! - system: SVC, MRS, NOP, CPS
//! - execution: Integration tests for instruction execution

pub mod common;
pub mod arithmetic;
pub mod logical;
pub mod bitwise;
pub mod branch;
pub mod load_store;
pub mod multiply;
pub mod system;
pub mod execution;
