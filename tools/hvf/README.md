# Hypervisor.framework SDK contract

Run the compiler-backed check on macOS with Xcode command-line tools, Python 3,
and stable Rust available:

```sh
python3 tools/hvf/check_sdk_abi.py --work-dir ./target/hvf-sdk-abi
cargo +stable test --test hvf_sdk_abi -- --nocapture
```

The probe extracts the register enum, relevant type aliases, constants and every
Intel foreign function declaration from the actual bindings source. A standalone
Rust program reports values, widths and alignments; Clang checks those results
and function pointer types against the installed Intel SDK headers. A second
Clang invocation checks the shared error and mapping constants against the Arm64
SDK headers. The work directory retains commands, compiler versions, header and
source hashes, generated probes and diagnostics. A missing tool or compiler
failure fails the check. The Cargo target is compiled only on macOS.

The probe performs source and compiler ABI checks. Native Intel VM creation,
VM-exit behavior, guest execution, VMCS field encodings and Arm64 function
signatures require separate validation. Rosetta can run the x86_64 test process
on Apple Silicon; it does not establish native Intel hypervisor execution.

The SDK defines Intel vCPU identifiers as 32-bit unsigned integers. Register IDs
include separate IDT, GDT, LDT and TSS fields before CR0, so architectural register
numbers cannot substitute for SDK identifiers. VMX capabilities use
`hv_vmx_read_capability`. The Intel and Arm64 APIs have different function
signatures and are compiled under their respective architecture gates.
