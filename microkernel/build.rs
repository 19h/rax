//! Select the per-architecture linker script for the bare-metal microkernel.
//!
//! Each supported target loads at a fixed physical address that the rax VMM
//! jumps to (x86_64: 0x100_0000, aarch64: 0x4000_0000, armv6: 0x5000_8000), so
//! the layout has to match the loader exactly — hence one script per arch. A
//! hosted ("usermode") build keeps the platform's default linker.

use std::env;

fn main() {
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if os != "none" {
        // Hosted/usermode build (e.g. x86_64-unknown-linux-gnu for Intel SDE):
        // use the default linker and start files, no custom script.
        return;
    }

    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    let script = match arch.as_str() {
        "x86_64" => "linker/x86_64.ld",
        "aarch64" => "linker/aarch64.ld",
        "arm" => "linker/armv6.ld",
        other => panic!("unsupported bare-metal target arch for microkernel: {other}"),
    };

    println!("cargo:rustc-link-arg=-T{manifest_dir}/{script}");
    if arch == "x86_64" {
        // The x86_64 kernel is linked at a high address but loaded position-
        // independently via RIP-relative access; keep it non-PIE/static.
        println!("cargo:rustc-link-arg=--no-pie");
    }
    println!("cargo:rerun-if-changed={manifest_dir}/{script}");
}
