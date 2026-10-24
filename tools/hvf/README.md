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

## XSETBV and XSAVE enumeration

```sh
cargo +stable test --test hvf_xsetbv
python3 tools/hvf/xsetbv_oracle.py --llvm-bin /path/to/llvm/bin --work-dir ./target/hvf-xsetbv
```

The Intel exit adapter reads the live CR0/CR4, privilege state, EAX/ECX/EDX and
configured XSAVE capabilities. It validates before writing XCR0. Success writes
XCR0 through the SDK register API before advancing RIP. Failure queues a hardware
exception at the faulting instruction; #GP carries error code0, #UD has no error
code. A queued synchronous exception takes priority over a pending IRQ. Host API
errors propagate without manufacturing successful retirement.

| Condition | Result |
| --- | --- |
| XSAVE absent or CR4.OSXSAVE0 | #UD |
| Protected-mode CPL1-3 or virtual8086 | #GP(0) |
| Nonzero ECX, x870, unsupported bit, invalid component dependency | #GP(0) |
| Valid configured XCR0 value1,3,7 | XCR0 write, then RIP advance |

Upper32 bits of RAX/RCX/RDX do not participate in XSETBV. The configured x87/SSE/
AVX profile advertises XCR0 mask7, a576-byte base area, and a256-byte YMM_Hi128
component at offset576. CPUID.1.OSXSAVE follows live CR4.OSXSAVE; CPUID.D.0.EBX is
576 or832 bytes according to enabled AVX state. These layout rules do not claim
support for additional CPUID-configured state components. Dependency checks for
MPX, AVX-512 and AMX do not advertise those extensions.

The independent probe boots a Haswell-noTSX QEMU TCG guest, verifies XCR0 mask7
and the AVX component layout, installs actual #UD/#GP handlers and executes114
instructions. OSXSAVE0/1, three ECX selectors and19 candidate values produce
three successful writes,57 #UD and54 #GP cases. It compares fault vector/error,
XCR0, instruction-boundary behavior, commit order and dynamic CPUID fields with
the actual product decision/commit module. All records and completion markers
must be present. The host-independent Cargo target additionally checks CPL,
virtual8086, ignored high halves, each reserved bit and host API failures.

TCG instruction execution supplies an independent architectural reference for
this listed CPL0 profile. Mock host calls and compiler/link checks do not prove
native Intel VMX calls, VM-entry delivery or every pending-event interleaving.
The original CR-access and MMIO exit defects remain separate. Internal exits
still use the existing recursive resume path; replacing that path and validating
all exit transitions is a separate group.
