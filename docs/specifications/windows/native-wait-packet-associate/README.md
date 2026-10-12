# Installed NT wait-packet association and cancellation

Measured on 2026-10-12, Windows 11 ARM64 10.0.29683.1000. The native
ARM64, x64 compatibility, x86 and x86 large-address-aware producers execute
installed NTDLL exports. Twenty-four producer executable/source identities and PE
machine/characteristic records are retained. Native principal: SYSTEM,
S-1-5-18. This is an oracle identity, not guest privilege authorization. No
physical Windows x64 execution is claimed.

The native System32 and SysWOW64 NTDLL SHA-256 identities equal the previous
constructor group's identities. The 11177-byte process observer is unchanged
(SHA-256 `d41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e`).

## Implemented contract

`NtAssociateWaitCompletionPacket` takes eight arguments. Validation checks the
packet type/right `0x1`, completion-port type/right `0x2`, target validity and
`SYNCHRONIZE` (`0x00100000`), target dispatcher eligibility, then packet reuse.
Typed-handle type errors precede access errors. A busy packet returns
`STATUS_INVALID_PARAMETER_1`; mutex/packet/port targets return
`STATUS_INVALID_PARAMETER_3`. Events, legacy/IR timers, semaphores, threads,
processes and synchronous file objects participate in the guest model.
Unmeasured dispatcher types stop explicitly rather than fabricate success.

Key/APC contexts and ULONG_PTR information retain the guest pointer width;
NTSTATUS retains all 32 bits. An association owns object references independent
of caller handle values. A consumed automatic event/timer or semaphore signal
queues one completion. Association registration order determines the model's
packet selection and matches the captured event/semaphore FIFO cases.

The optional `AlreadySignaled` destination receives one BOOLEAN byte after
successful association. Its store fault is suppressed without rollback; a guard
is consumed once, with no retry. Null, unmapped, readonly, guarded, unaligned and
last-page-byte destinations are measured. Validation errors leave it untouched.
The packet itself remains notification-signaled throughout association states.

```text
idle --associate(unsignaled target)--> pending --target signal--> queued
idle --associate(signaled target)-----------------------------> queued
pending --cancel or final-handle close------------------------> idle
queued --dequeue, cancel(true), or final-handle close----------> idle
queued --cancel(false) / STATUS_PENDING-----------------------> queued
```

`NtCancelWaitCompletionPacket` checks the same packet type/right. It truncates
its BOOLEAN input to eight bits. Cancellation of a pending packet succeeds;
a queued packet remains in use when removal is false. Any nonzero BOOLEAN
removes it. A never-associated or completed packet returns `STATUS_CANCELLED`.
Closing one duplicate preserves association; final-handle close cancels even
when an ordinary object-wait pin keeps the packet object alive. Process teardown
clears the association ledger and owned object table. Closing the final completion-
port handle discards packets already queued there and clears their associations.
Pending associations retain the port and can deliver after that close.

The pending poll uses O(P + B log P) expected time and O(P) temporary space for
P pending packets and B deliveries. Queued cancellation scans O(Q) completion
records; final-handle detection scans O(H) handles. Capacity is reserved before
consuming a target signal. Queued records hold no owning packet reference, so
packet/port reference cycles do not form.

The public Rust `CompletionPort.queue` field changes from
`VecDeque<(u32,u64,u64)>` to `VecDeque<CompletionPacket>` with `key`, `apc`,
`status`, and `information` fields. `CompletionPacket::new` constructs a payload.
The C ABI remains 1.11.0; Assist's Rust ABI minor remains 184. Dependencies,
default features, toolchains, CRT policy and native-library admission do not change.

## Evidence and validation

| Exact native row replays | Count |
|---|---:|
| Association access/type/output validation | 540 |
| Association/cancel/signal/completion transitions | 60 |
| Cancellation handles/access/BOOLEAN values | 220 |
| Target dispatcher eligibility | 52 |
| Busy-packet validation precedence | 80 |
| Queued BOOLEAN and dependency final-close boundaries | 44 |
| Total | 996 |

Additional state, alias/final-close and FIFO captures support lifecycle tests.
The original `lifetime` producer's 128-byte ObjectBasicInformation query was
rejected. `lifetime-v2` uses the required 56-byte buffer and keeps the alias
through signal/dequeue. Both original captures remain; the rejected queries are
not reference-count proof. Pointer counts vary; the cause and exact kernel reference ownership counts
are unknown. Guest references are
checked through handle-close permutations, wait pins and complete table cleanup.

| Gate | macOS | Linux | Windows |
|---|---:|---:|---:|
| Focused tests, passed | 9 | 9 | 10 |
| Full suite, passed/failed/ignored | 7638/0/2 | 7629/3/2 | 7037/5/2 |
| C API tests, passed | 168 | 168 | 168 |
| Registered Windows guest integrations, passed | 544 | 544 | 4 |
| Owning C++ executables, passed | 5 | 5 | 5 |
| All-target build | pass | pass | pass |

Windows focused tests execute actual ARM64/WoW64 leaves, including x86
RET 32 plus the 4-byte return address for association (36 bytes total), and
RET 8 plus return address for cancellation (12 bytes total).

Linux retains three full-suite io_uring timeout failures; all three unchanged
isolated cases pass. The initial one-failure run is also retained. Windows retains four BZHI and
one FP16 lowering failures. The initial native thread-clock comparison failure
(15.625 ms joining CPU against 62.5 ms worker CPU) is retained; the unchanged isolated clock
case passes. Cause of the timing-result variation is unknown. No assertion or
platform skip was weakened. macOS/Linux C API execution and final Linux integration execution are serial,
as recorded in the commands. The parallel Linux integration runner stalled
after 58 completed cases with host threads blocked; its output and thread
snapshot are retained. The cause is unknown. Linux amd64 execution uses CPU
translation.

The owning macOS CLI/MCP build, seven selected CTests and both protected-text
scans pass. macOS/Linux use the previous frozen root `e2fc5b48`, whose owning
source/config/resource bytes equal root `4efcbc98`; only two engineering records
and the gitlink differ. Windows verifies all 16583 source files, 115 final-attempt patches and
the unchanged 374-byte offline adapter. Its actual static-CRT Assist/core
archives contain 1119581022/732743660 bytes and 4673/260 archive members.
Thirty-seven actual compiled source hashes and two helper hashes bind those
archives; the selected core has no default features.

The process frontier advances from scheduler turn 42185,
`NtAssociateWaitCompletionPacket` service `0x92` at `0x180001960`, to turn 42564,
`NtSetInformationWorkerFactory` service `0x1b1` at `0x180002b50`, with arguments
`[0x20,0x5,0xabe720,0x4]`. The difference is 379 scheduler calls, not instructions.
All four ordinary smoke/MSVCRT/UCRT/installed-whoami programs still report
reason 5, exit field 0 and an internal emulator failure before guest exit.

`NtRemoveIoCompletion`, completion-thread accounting and worker-factory
mutation/activation remain separate unimplemented services. Native removal is
an oracle here; internal dequeue assertions do not establish an exported
removal service. Mixed ordinary-thread/association fairness is not measured.
Microsoft does not promise FIFO thread selection for auto-reset events.

## Provenance and reproduction

The typed prototypes agree with pinned PHNT commit
`53fbbdc5b5d2b08761db1c7b26bfa8c820924356`; its header and MIT license remain in
[the constructor evidence](../native-wait-packet-create/README.md).
Primary API contracts: [association](https://learn.microsoft.com/en-us/windows/win32/devnotes/ntassociatewaitcompletionpacket),
[cancellation](https://learn.microsoft.com/en-us/windows/win32/devnotes/ntcancelwaitcompletionpacket),
[event selection](https://learn.microsoft.com/en-us/windows/win32/sync/event-objects).
Private statuses, rights, fault behavior and lifecycle details above come from
the retained installed-kernel measurements.

Run `python3 check_native_wait_packet_associate.py` from this directory, or
provide an evidence directory followed by the RAX source root. The checker
requires the exact evidence file set, producer/source/log identities, measured
statuses, nonempty filtered tests, original stdout-bound summaries, compiled
source identities, owning archives and retained failure frontiers. It rejects
optimized Python. Commands, adapters, raw output and the Assumption Register /
change-surface map / bounded findings are retained in this directory.
