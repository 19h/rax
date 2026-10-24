# Selected native connection-state query

`NtUserRemoteConnectState` is acquired through the selected installed Win32u
module on the owned native GUI helper. Its fixed no-argument query is captured
before and after the bounded GUI metadata acquisition. A changed result retries;
guest arguments and pointers never enter the host call. The captured scalar is
part of the immutable selected input view, rather than a live host-session poll.

The fixed native cohort comprises 4 executable views * 2 threads * 2 stages *
32 samples = 512 queries. Every recorded value is 0, and every query preserves
last-error `0x12345678`. ARM64, x64, x86 and large-address-aware x86 views share
one Windows ARM64 10.0.29683 kernel; they are not independent kernel profiles.
Selected kernel disk-image code returns a `ULONG` in 0..4. Nonzero states have
static and synthetic coverage, not independent native session-state controls.
The pointer-free model rejects a selected scalar outside that recorded domain.

A separate private-process control invokes the query at the first Init2 stub,
before the initial native GUI conversion. It observes callback 84 with zero
input bytes. The cold query then returns the scalar. Controlled callback failure
returns zero and clears the native thread conversion marker. Its invalid return
buffer and maximal length are not dereferenced by this measured continuation.

| Cold control | Callback status | Query result | Conversion marker afterward | Native probe exit |
| --- | --- | ---: | --- | ---: |
| Actual USER32 callback | `0x00000000` | 0 | Present | 0 |
| Controlled success | `0x00000000` | 0 | Present | 0 |
| Controlled success and invalid output | `0x00000000` | 0 | Present | 0 |
| Controlled failure | `0xC0000001` | 0 | Absent | 5 |
| Controlled warning and invalid output | `0x80000001` | 0 | Absent | 5 |

The negative probe exits reflect the deliberately failed USER32 load. Producer
`finished` means that recording completed; it does not mean all native exits
were zero. The shared conversion continuation retains a per-service successful
return value so a query does not inherit Init2's opaque process value. Nested
queries preserve the outer continuation. Guest CPU tests exercise both ARM64
and x64 service/dispatcher/callback-return instructions and synthetic states
0..4; they do not establish independent x64-kernel behavior.

`observations.json` retains scalar projections and original private log hashes.
Raw stack, TEB, desktop and GDI buffers remain private. Original receipts bind
native executable/source/runtime identities. `kernel-inputs.json` distinguishes
RSDS/DBI age 1 from PDB Info age 3 and identifies the selected static code.
`inputs.json` binds every projected producer and explanatory file. The positive
invalid-output cohort reuses the exact verified cold-control executable and
source; its receipt binds the original producer receipt. Raw memory logs remain
private. Integrity controls are recorded separately from native behavior.

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| RCS-A1 | Selected session state is coherent with the acquired GUI inputs | Repeated fixed queries; helper before/after comparison | Immutable query scalar | Thread/stage repetitions and changing capture | Inconsistent native values or required live-state transition | Retained; snapshot scope only |
| RCS-A2 | Cold query conversion resumes with the query result | Native callback 84 and controlled failure observations | Per-service continuation value | CPU replay, nested query, nonzero synthetic values, failed callbacks | Wrong restored return/context or changed last-error | Confirmed for recorded controls |

| Impact | Remaining boundary | Effect |
| --- | --- | --- |
| High | Whole USER/GDI and same-host process behavior | This selected query does not complete the larger objective |
| High | WoW64 paired GUI contexts | Narrow matching native 64-bit GUI acquisition remains separate from full x86 conversion |
| Medium | Independent x64 kernel and nonzero native session states | Limits native portability and state-transition claims |

Ordinary candidate execution and owning-carrier evidence are recorded separately.
C API reason 5 is `RAX_PROCESS_FAILED`; Rust `Internal(...)` is a diagnostic
variant. Only `RAX_PROCESS_EXITED` (4) has a valid guest exit-code field.
