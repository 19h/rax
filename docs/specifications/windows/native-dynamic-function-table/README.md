# Native process dynamic function tables

`NtSetInformationProcess(ProcessDynamicFunctionTableInformation)` now maintains
process-owned registration ranges. It uses checked guest memory access and
never passes a guest pointer to the host kernel. Ordinary Windows application
completion remains unproven; the final continuation evidence is recorded below.

The native controls cover ARM64, x64 compatibility, x86 and x86-LAA callers on
Windows 10.0.29683.1000. The x64 producer is not a physical x64-kernel test.
The selected WoW64 wrapper returns success without inspecting the process,
length or pointed memory; it creates no registration. Native 64-bit callers
require the exact current-process pseudohandle `0xffffffffffffffff`. A real
handle to the same process and zero-extended `0xffffffff` are rejected before
length or memory validation.

| Native 64-bit input | Required layout or access |
| --- | --- |
| Outer information | Exactly 16 bytes; address aligned to 8 bytes; entire record read |
| Offset 0 | 8-byte `DynamicFunctionTable` guest pointer |
| Offset 8 | 1-byte `Remove`; any nonzero value removes |
| Inner record | Address aligned to 4 bytes |
| Inner offset 48 | 8-byte `BaseAddress`, read first |
| Inner offset 40 | 8-byte `MaximumAddress`, read second |
| Other inner bytes | Not accessed by this service |

The ordered field reads matter when an unused prefix is inaccessible or each
field touches a different guarded page. Native read guards are consumed once;
a preceding alignment or process-handle rejection does not consume them.
The regression compares status, caller bytes, page protection and last error.

Addition computes `MaximumAddress - BaseAddress` without wrapping and requires
an extent in `[0, 0xffffffff]` bytes. Removal computes the subtraction modulo
`2^64` and accepts extents in `[0, 0xfffffffe]` bytes. Thus a maximum-width
registration can succeed while its removal returns `STATUS_INTEGER_OVERFLOW`.
The two operations intentionally have different bounds.

Registrations retain insertion order. Removal selects the first equal base,
then requires an equal extent. It does not search past an earlier unequal
extent. Empty duplicates are allowed. A nonempty candidate can follow empty
registrations at the same base, but an empty candidate at the start or inside
an existing nonempty interval is rejected. Adjacent nonempty intervals are
accepted; overlapping intervals return `STATUS_CONFLICTING_ADDRESSES`.
Descriptor memory and covered memory can be freed without removing a range.
The registry is shared by all threads in the emulated process.

Each operation takes `O(N)` time; the ledger uses `O(N)` host memory. The
emulator limits live registrations to 65,536, returning
`STATUS_INSUFFICIENT_RESOURCES` without changing the ledger at that bound.
This is an explicit emulator policy, not a measured Windows quota.

This kernel registration ledger is separate from the user-mode fixed arrays
used by `RtlAddFunctionTable`. A diagnostic capture of ordinary startup contains
a type-3 table with one zero entry. Registering that range does not prove that
it is a valid fixed unwind array. General exception recovery is not established
by this change.

The [pinned PHNT declarations](primary/sources.json) identify class 53, its
pointer/BOOLEAN packet and the dynamic-table fields. Microsoft's retained
MS-ERREF page identifies the status values. Native observations own pointer
ordering, range admission and lifetime behavior; those details are not inferred
from the declarations. The prior startup diagnostic has a separate
[identity manifest](diagnostic-inputs.json).

| Native control family | Child executions across four profiles |
| --- | ---: |
| Initial input combinations | 36 |
| Duplicate/value lifetime | 4 |
| Key-byte perturbations | 4 |
| Pointer, length and guard boundaries | 4 |
| Original sequential range diagnostic | 4 |
| Isolated ranges | 204 |
| Overlap | 224 |
| Extent overflow | 60 |
| Descriptor/range/thread lifetime | 4 |
| Empty/nonempty overlap | 36 |
| Ordered duplicate sequences | 104 |
| Status conversions | 4 |
| Pseudohandle boundaries | 24 |
| Total | 712 |

The 52 producer identities bind source, post-run PE machine/LAA/hash, exit
status and raw stdout. The original sequential range diagnostic is retained
with `contract:false`: its maximum-width range survives a failed removal and
contaminates later cases. Only fresh-process range controls define those
semantics. The clean controls contain 14,864 recorded service/status-conversion
calls; the diagnostic adds 612 excluded calls.

`check-native.py` verifies raw-to-corpus correspondence, complete axes, ordered
fault semantics and identities. `stress-native.py` rejects eight altered
proof sets after rebinding both the corpus and digests. Data-driven Rust tests
replay 14,808 recorded calls/conversions without normalizing status or protection.
The 56 descriptor/range/thread lifetime calls are separate native controls,
corroborated by the dedicated shared-state/free-lifetime regression; they are
not claimed as an exact row replay.
The Windows test also executes the installed native-host and x86 service
leaves, including x86 `RET 16` cleanup.


## Validation and remaining application boundary

| Host | Full library passed/failed/ignored | Dynamic-table focus | C API | Windows integration |
| --- | --- | --- | --- | --- |
| macOS ARM64 | 7891/0/2 | 11 | 168 | 544; native-memory adapter excluded |
| Linux x86-64 container | 7885/0/2 | 11 | 168 | 544; native-memory adapter excluded |
| Windows ARM64 | 7324/5/2 | 12 | 168 | 4 native-memory cases; Unix corpus excluded |

All-target checks pass on all three hosts. The five Windows failures are the
unchanged four BZHI and one FP16 lowerer tests. The two existing ignored
microkernel tests remain ignored. The prior TLS fixture correction is inherited
and the full Windows suite contains no TLS failure. No filtered full-suite test
or Windows execution of the Unix integration corpus is claimed.

The final three-host gates bind 5,566 selected source/fixture inputs with no
post-build Rust source change. The first epoch used a wrong memory-error wrapper
and did not compile. The second epoch read the complete 56-byte inner prefix
and failed the native boundary replay. The final epoch reads only BaseAddress,
then MaximumAddress; the failed epochs and correction records remain visible.

Scoped Assist checks pass seven CTests and two protected-command scans on macOS,
and five C++ consumers on Linux and Windows. Every host has two complete archive
walks of the actual Cargo-selected empty-feature core and static carrier,
bound to 361 selected source/fixture inputs. The retained Assist carrier has
Rust ABI minor 184; the paired current root has minor 196. All nine selected C++
consumer sources match. This does not establish a whole-current-root GUI/plugin
or package build.

The unchanged ordinary observer passes class 53 and stops at the next service,
`NtOpenProcessToken` (0x133), at smoke turn 2,149,659 and whoami turn 2,152,008.
Both outcomes are `Complete(Internal(...))`; controller exit 0 is not application
success. A separately instrumented progress observer reproduces the smoke
terminal turn and shows intervening user-mode computation. The absent third
helper is excluded from the execution count. No successful ordinary Windows
application completion or general SEH recovery is established.

The validation checker also rejects six altered proof sets after digest
rebinding: changed native failure, removed ignore, invented Windows integration
execution, source drift, enabled core defaults and invented application success.
