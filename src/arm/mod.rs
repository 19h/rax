//! ARM architecture definitions and ISA support.
//!
//! This module provides comprehensive ARM architecture definitions including:
//! - Hierarchical ISA selection (Profile -> Version -> Extensions)
//! - Execution state management (ARM, Thumb, AArch32, AArch64)
//! - System register encoding/decoding for CP15 and AArch64
//! - Feature flags for optional extensions
//!
//! # Architecture Hierarchy
//!
//! ARM processors are organized by profile:
//! - **A-profile**: Application processors (Cortex-A, mobile/server)
//! - **R-profile**: Real-time processors (Cortex-R, automotive/industrial)
//! - **M-profile**: Microcontroller processors (Cortex-M, embedded)
//!
//! Each profile has multiple architecture versions (v6, v7, v8, v9) with
//! different mandatory and optional features.

pub mod features;
pub mod isa;
pub mod state;
pub mod sysreg;

pub use features::*;
pub use isa::*;
pub use state::*;
pub use sysreg::{Aarch64SysReg, Cp15Encoding, Aarch64SysRegEncoding};
