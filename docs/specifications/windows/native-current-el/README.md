# Windows ARM64 CurrentEL compatibility

The Windows process scheduler recognizes an otherwise UNDEFINED ARM64
`MRS Xt, CurrentEL` (`0xD5384240 | Rt`) and resumes at the next instruction.
This models the observed Windows kernel response at EL0. The architectural
CPU and the Linux/macOS process personalities retain their existing EL0
system-register access checks.

The independent native v3 producer runs 36 leaf-code cases on Windows ARM64
10.0.29683. Each case allocates a 4096-byte page, writes its instructions,
changes the page to executable/readable, flushes the instruction cache, and
executes under a native SEH filter. All 36 allocations are released. Code bytes
remain identical around execution; source, executable, and eight system DLL
identities remain identical around the complete run.

| Instruction/destination | Observed Windows result |
|---|---|
| `MRS X0-X17, CurrentEL` | destination becomes zero |
| `MRS X18, CurrentEL` | reserved TEB register survives exception return |
| `MRS X19-X30, CurrentEL` | destination becomes zero |
| `MRS XZR, CurrentEL` | result discarded |
| `MRS X0, SCTLR_EL1` | `STATUS_ILLEGAL_INSTRUCTION` (`0xC000001D`) |
| `MRS X0, TTBR0_EL1` | `STATUS_ILLEGAL_INSTRUCTION` |
| `MRS X0, SPSel` | `STATUS_ILLEGAL_INSTRUCTION` |
| `MRS X0, SP_EL0` | `STATUS_ILLEGAL_INSTRUCTION` |

The v1 intrinsic probe establishes a successful read independently of the
emulator. V2 recorded the complete matrix but rejected its own incorrect
zero-only expectation for X18; its driver stopped before final identity checks.
V3 corrects that assertion against `NtCurrentTeb()` and completes all checks.
V2 is retained verbatim and supplies no completed final-identity claim.

The scheduler checks the guest architecture, CPU architecture, current/aligned
PC, checked four-byte PC increment, executable fetch, and complete instruction
encoding before changing any register. It preserves X18, treats Rt31 as XZR,
and changes only an ordinary destination and PC. This compatibility path is
O(1) time and O(1) space. Other privileged instructions retain their prior
exception path. Tests cover all 32 destinations, every other general register,
NZCV, SP, code bytes, missing/guarded/non-executable pages, stale/misaligned PCs,
x86/x64 rejection, and scheduler-turn/resumption behavior.

The actual retained Assist core previously stopped in the newly mapped MSVCRT
image at PC `0x0376CF90`, instruction `0xD5384250` (`MRS X16, CurrentEL`). Its
original exception was `STATUS_ILLEGAL_INSTRUCTION`. A subsequent unwind walk
cycled at `0x037349F8`, because that view was outside the initial module registry.
The CurrentEL change addresses the measured original instruction boundary;
complete image-view unwind registration is a separate scope item. Full same-host
process-userland completion remains an active objective.

The native kernel result is direct execution evidence. The general architectural
separation follows Arm's
[AArch64 Exception Model, section 2.2.2](https://developer.arm.com/documentation/102412/0103)
and the vendored [vendored Arm guide](../../arm/learn_the_architecture_-_aarch64_exception_model_102412_0103_01_en.pdf). Microsoft's
[ARM64 ABI register table](https://learn.microsoft.com/en-us/cpp/build/arm64-windows-abi-conventions)
defines X18 as the reserved TEB register. Neither reference establishes this
particular Windows CurrentEL response; the native producer does.

The precise native handler was not traced. Kernel/OS-personality placement in
the emulator models the observed user result; it does not establish which
native Windows or virtual-machine component handles the trapped instruction.
The observed X18 context is the ABI-valid TEB value. Behavior for a deliberately
corrupted reserved platform register is not claimed.

Both actual owning ordinary routes now pass the original instruction and reach
`NtQueryVolumeInformationFile` (service `0x49`, instruction address
`0x7FFF38A414D0`). The Rust observer reports terminal index 2,550; the C API
reports 2,551 turns started. Both still run smoke to exit status zero (indices
974 and 975 turns, respectively). For whoami, C API reason 5 with exit-code
field zero is an emulator failure, not guest exit. Diagnostic capture identifies
`FileFsDeviceInformation` (class 4), length 8 B, on the guest console input
handle. The service remains outside this increment.

## Validation

| Host/configuration | Full library | C API | Integration |
|---|---|---|---|
| macOS ARM64, x86_64-suite + smir-jit | 10,321 pass; 2 existing ignores | 183 pass | 544 pass |
| Linux AMD64 container, no default features | 8,134 pass; 2 existing ignores | 183 pass | 544 pass |
| Windows ARM64, no default features | 7,603 pass; exactly 5 existing lowerer failures; 2 existing ignores | 183 pass | 4 pass |

All three all-target configurations, Linux default-feature Clippy, and Windows
ARM64/AMD64 all-target cross-checks pass. The Linux AMD64 container runs under
the ARM64 host's virtualization/translation environment; it is not an independent
physical AMD64 oracle. Each host runs the five new focused tests. The executed
native unit image and complete frozen compilation graph are recorded separately
from the actual owning carrier and callers.

Retained Assist ABI184 consumers pass on all three hosts. MacOS runs seven
CTests and verifies eight selected-carrier link rules, including `assist-mcp`
and `assist-cli`; Linux and Windows compile/run five C++ consumers each. Archive
pairs are selected from actual Cargo artifacts, walked completely, and unchanged
around consumers. Linux's first consumer launcher failed before compilation
because its helper was absent. The original failed record remains intact; its
successful initial stages and the three completed retry stages form the complete
evidence. It is not recast as an originally successful driver.

The final native audit rehashes the ten-DLL union used by the native producer
and ordinary callers, both live core/carrier and both executed callers, four
separate retained current artifacts, the native producer source/executable,
and the executed native unit image. The earlier native audit separately verifies
four retained artifacts of the preceding revision. Native-producer and ordinary
observer DLL sets differ by MSVCRT versus version.dll; their union is explicit.

`check_current_el_evidence.py --source <rax-root>` validates native observations,
source binding and the unchanged 2,986-byte red regression prefix. Its Windows
`--live-native` mode rehashes fourteen original native files. Four semantic
oracle corruptions are rejected after repairing transitive log/receipt hashes,
with four fresh positive restorations. `check_current_el_gates.py` verifies the
full source/consumer gates and the new ordinary boundary. It deliberately reports
whole-root/CI success as false. The retained ABI184 proof does not establish
current Assist Rust ABI274 GUI/package execution. RAX C ABI remains 1.14.0.

Captured native CRLF, progress-line whitespace and original receipt hashes are
preserved verbatim. Six public compiler projections retain original byte
identities while withholding private Assist diagnostics. The full same-host
objective remains incomplete, including the next console volume query, image-view
unwind registration, and previously recorded image/runtime policy boundaries.
