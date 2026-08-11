//! RAX multi-architecture bare-metal microkernel test suite.
//!
//! One `no_std`/`no_main` kernel that builds for three bare-metal targets —
//! x86_64, AArch64 and ARMv6 (ARM1176/S3C64xx) — plus an x86_64 "usermode"
//! build for Intel SDE. It boots under the rax emulator, runs a large
//! self-checking test surface (architecture-neutral integer/fixed-point work
//! plus per-ISA instruction coverage), prints a machine-readable result
//! sentinel, and powers the machine off.
//!
//! Build (see ./build.sh and the top-level Makefile):
//!   cargo +nightly build --release --target x86_64-unknown-none
//!   cargo +nightly build --release --target aarch64-unknown-none-softfloat
//!   cargo +nightly build --release --target ./armv6-rax-none-eabi.json \
//!       -Z unstable-options -Z json-target-spec

#![cfg_attr(not(feature = "usermode"), no_std)]
#![cfg_attr(not(feature = "usermode"), no_main)]

#[macro_use]
mod serial;
mod arch;
#[cfg(all(not(feature = "usermode"), target_arch = "arm"))]
mod arm_eabi;
mod fixed;
mod harness;
mod mem;
mod nbody;
mod tests_common;

use harness::Harness;

/// Common entry point. The per-arch `_start` shim sets up a stack and branches
/// here (on usermode the platform's `main` calls it). Runs the suite and powers
/// off — exit status is conveyed to the host via the printed result sentinel.
pub fn kmain() -> ! {
    arch::early_init();
    unsafe {
        mem::allocator().init();
    }

    let mut h = Harness::new(arch::ARCH_NAME);
    h.banner();
    tests_common::run(&mut h);
    arch::isa_tests(&mut h);
    let _passed = h.finish();

    arch::poweroff();
}

#[cfg(feature = "usermode")]
fn main() {
    kmain();
}

#[cfg(not(feature = "usermode"))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    serial::on_panic(info)
}
