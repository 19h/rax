# Selected native WNF state queries

The ordinary ARM64 smoke and whoami continuations pass NtAccessCheck and call
NtQueryWnfStateData for state `0x0280032EA3BC0875`, TypeId
`48f96463-52d6-4e85-9c02-f2c14e17264e`, NULL ExplicitScope and 4096 bytes of
capacity. Its decoded scope is System, not Process. The captured native query
returns change stamp 1 and eight bytes `0100000000000000`; a missing or different
TypeId returns STATUS_INVALID_PARAMETER. This distinction prevents replacing
all queries with an empty or successful result.

## Ownership and acquisition

The adapter selects immutable data using the fixed Notifications registry
catalog before guest execution. Provisioning records contain a self-relative
security descriptor, a maximum-length DWORD and, for typed states, a 16-byte
TypeId. The retained catalog has 1613 entries; 1612 use the four-byte trailer,
and the actual continuation uses the sole 20-byte trailer. Selection uses
validated names and TypeIds, never guest input or a guest pointer.

Each state uses at most three pairs of native observations. A pair must agree
on status, change stamp and data. Each payload is bounded to 65536 bytes, the
catalog to 4096 entries and retained payload to 16777216 bytes. Native denial,
known absence and unavailable acquisition remain distinct. The captured
primary subject must agree before and after selection. Host thread
impersonation must be absent before and after capture. An owned native test
thread verifies rejection under impersonation and restores its original state.

Process-scoped states select native read admission but start empty with change
stamp zero in each new guest process. Host process-private payload and change
stamps are never imported. Other selected scopes use selection-time snapshots.
Later host changes are outside that immutable initial-environment contract;
state creation, update and notification delivery remain separate boundaries.

Guest queries run against owned immutable records. The adapter captures the
name, optional GUID, capacity and output probes before resolving scope, name,
access and TypeId. It preserves byte-aligned inputs, guard order, complete
nonempty output-span checks and exact aliases. On both success and insufficient
capacity, it publishes change stamp before required length. Data follows only
when capacity is sufficient. A supplied capacity causes no proportional host
allocation.

ExplicitScope depends on the decoded namespace: System and Machine require
NULL; Session uses the captured session-ID DWORD; User uses the captured
user SID; Process uses a guest process handle. Queries of uncaptured sessions,
users and other guest processes stop explicitly: a snapshot cannot establish
whether a different namespace is absent or contains data. A valid Process
handle with zero granted access is sufficient, as independently measured.
Unmeasured explicit PhysicalMachine scope also remains unsupported. Guest
handles are never passed to Windows.

For `N` definitions and maximum lengths `M_i`, selection performs at most
`6*N` native observations and `O(N log N + sum(M_i))` local work. Retained
storage is `O(N + D)`, where `D <= 16777216` bytes; temporary capture storage
is bounded by two 65536-byte payloads. Kernel call latency is not given a
wall-clock guarantee. A guest lookup is `O(log N)` plus the existing VM-region
probe cost and `O(D_i)` output copying. No `O(capacity)` host buffer is created.

## Native evidence and limits

Four independently inspected producer ABIs on one ARM64 Windows 10.0.29683
kernel record 7680 logical V3 calls and 6240 pointer/protection calls. Pointer
rows retain the actual temporary-name bytes separately for each producer; no
byte comparison is discarded to normalize opaque names.

The V5 extension adds a real read-denied state. ARM64, x86 and x86-LAA producer
identities independently establish 6480 V5 calls. Its x64 producer executed
2160 calls and cleanup, but Defender quarantined the executable before the
independent PE/hash inspection. That output is retained as corroboration and
is explicitly excluded from the verified V5 producer count. No Defender policy
change or quarantine restoration is part of this source change.

Private typed-state controls add 168 ARM64 calls, eight deleted states, two
freed descriptors and one closed query token. They establish that namespace
resolution precedes access, and access denial precedes TypeId mismatch. Six
separately duplicated process handles establish the zero-rights behavior;
all six handles close. Failed setup attempts and the V4 read-denial label
mistake remain separately recorded in `provenance-notes.md`.

`check.py` verifies raw identities, producer architectures, matrix dimensions,
cleanup, compiled fixtures, captured input and these evidence boundaries.
Guest execution, installed NTDLL leaves and full application completion are
separate validation claims. No ordinary application success is established by
these native probes.

## Assumption register

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| A1 | Shared WNF snapshots represent the selected initial environment. | Same-host immutable runtime selection and stable native observation pairs. | Shared-scope query data. | Changed stamp/data, denied/missing state, oversized payload. | A guest query forwards to the host or an unstable pair is accepted. | Retained; live host mutation equivalence is not claimed. |
| A2 | A new guest process starts with its own empty process-scoped state. | Separate process namespace, zero-stamp native controls. | Process query initialization. | Two guests, overwritten caller output, zero-right process handles. | Native host process-private payload appears in a guest. | Confirmed by capture-isolation and per-process replay; live updates remain unmodeled. |
| A3 | Provisioning trailers have the measured maximum plus optional GUID grammar. | All 1613 retained metadata values and matching captured GUID. | Bounded native acquisition. | Truncation, invalid offsets, invalid SID/ACL extent, duplicate names. | Unknown metadata is accepted as a different known record. | Confirmed for the corpus; unknown formats refuse. |
| A4 | Native query admission uses the same primary subject as guest decisions. | Subject capture before/after; required absent host impersonation. | Permission snapshots. | Owned native test-thread impersonation and restoration. | Capture succeeds under an impersonating host thread. | Confirmed by native capture and owned-thread impersonation/reversion tests. |
| A5 | Measured probe and publication order transfers across the four guest ABIs. | Four native V3/protection matrices; three verified V5 producers. | Guest pointer/alias semantics. | Every retained row plus actual installed ARM64/x86/x86-LAA leaves. | A replay differs in status, bytes, protection or stack cleanup. | Shared replay, installed leaves and final three-host suites are verified. x64 V5 identity remains unverified. |
| A6 | Every noncurrent session is invalid and every other user namespace is absent. | The first scope matrix used unallocated IDs and a nonuser World SID; it did not establish the general rule. | Superseded status-return branches. | Probe-owned states queried through valid session 1 and LocalService SID. | Session 1 returns NAME_NOT_FOUND, not INVALID_PARAMETER; the selected snapshot cannot prove other namespace contents. | Falsified; current code explicitly refuses uncaptured sessions/users. |

## Source and platform surface

Affected planes are fixed native WNF acquisition, immutable query records,
process-owned catalog access, native service registration and replay tests.
Existing subscription and notification-event policy is unchanged. Public C
ABI, features, dependencies, CPU lifting/JIT, filesystem policy, transport,
persistence, UI and packaging layout are unaffected. The Windows adapter adds
no new runtime library dependency beyond already linked Windows system APIs.

The same guest adapter and data model compile on Windows, macOS and Linux.
Host WNF acquisition belongs to Windows; other hosts retain the existing
explicit unavailable result for selecting installed Windows DLLs. Final
three-host core and scoped Assist embedding checks are retained below.

Bounded findings: High, complete ordinary Windows execution, live WNF mutation
and notification delivery remain unproven. Medium, physical x64 kernels,
other Windows builds and noncurrent guest process routing are unmeasured.
Medium, the quarantined V5 x64 executable prevents its independent identity
claim. These limits do not become platform exceptions or successful stubs.

## Retained validation epochs

The initial source did not compile because the metadata tests lacked explicit
imports and the service test module lacked its sibling-directory path. Each
platform's first diagnostic is retained; redundant filtered compile attempts
were stopped using only verified task-owned processes and containers. The
second epoch corrects those test declarations and adds the host-thread guard.
The third adds the native scope fixture and its four-ABI replay without
changing the production implementation. A later private-state probe then
falsified the assumption that every noncurrent session ID is invalid: session
1 returns NAME_NOT_FOUND for the probe-owned name, while unallocated IDs return
INVALID_PARAMETER. The fourth epoch refuses uncaptured session/user queries
instead of inventing either result. All native observations remain unchanged.
The scope test compares 144 admitted native rows per ABI and explicitly checks
24 uncaptured rows plus two new counterexamples per ABI for refusal.

The first Windows scope-extension runner encountered PowerShell's singleton
array flattening after applying and verifying the source. Its failure record
is retained. The corrected runner uses the same verified source and passes
all seven native service tests, including installed leaf execution.

The cross-namespace counterexample creates only two temporary probe-owned
names. It seeds the current session/user instances, attempts the other
namespace, and deletes both names and its descriptor. No existing state is
modified. Its source and complete cleanup record are retained under `controls/`.

## Final validation and ordinary continuation

| Host | Full library passed/failed/ignored | C API | Scoped owning gates |
| --- | --- | --- | --- |
| macOS ARM64 | 7858/0/2 | 168 | 6, including 7 CTests and 2 protected-text scans |
| Linux x86-64 container | 7852/0/2 | 168 | 4, including 5 C++ consumers |
| Windows ARM64 | 7287/5/2 | 168 | Source verification, shipping archive and 5 C++ consumers |

All library runs are unfiltered. The Windows failures are the same four BZHI
and one FP16 lowerer cases checked by exact name in `validation-check.py`.
Both ignored cases are pre-existing microkernel tests. All-target checks pass
on all three hosts; Unix Windows integration runs 544 cases and native memory
integration runs 4. Query focus comprises 4 metadata and 6 service tests on each Unix host,
and 6 metadata and 7 service tests on Windows, including owned impersonation
and installed-leaf cases.

The scoped Assist carriers each bind 327 selected inputs, empty core features
and two structurally complete archives. Their frozen Assist public ABI minor
184 is distinct from current minor 186; RAX ABI remains 1.11.0. These results
do not establish whole-current-root, plugin, GUI or package validation. Five
altered/incomplete proof sets are rejected by the retained stress checker.

The unchanged ordinary observer now stops at `NtSubscribeWnfStateChange`
service `0x1cd`, PC `0x180002d10`: smoke at turn 2141884 and whoami at 2144246.
Each advances 2202 turns from its prior query boundary. The controller exits
successfully after recording an unsupported guest result; neither application
exits successfully. The absent third fixture is explicitly excluded.
