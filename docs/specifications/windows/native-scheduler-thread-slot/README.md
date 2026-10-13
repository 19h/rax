# Native thread scheduler shared-data slot (2026-10-13)

This increment implements `NtSetInformationThread` class57
`ThreadSchedulerSharedDataSlot` on the guest process model. It uses the existing
class112 `SchedulerSharedData` owner; guest requests never call host NT.
The native evidence is Windows kernel10.0.29683.1000 ARM64 under the recorded
SYSTEM principal, with ARM64, x64 compatibility, x86 and x86 LAA callers.
Physical x64-kernel and other kernel/DLL builds remain unknown.

| ABI | Exact size (B) | Action offset (B) | Owner offset (B) | Slot offset (B) |
| --- | --- | --- | --- | --- |
| Native64 | 24 | 0 | 8 | 16 |
| WoW64 | 12 | 0 | 4 | 8 |

Native64 probes the entire nonzero supplied writable extent, with ULONG4B
alignment and numeric user-range validation, before the exact-length test.
WoW64 requires length12B first, reads its converted record, runs the kernel
operation, then copies all12B back even on a returned kernel failure. A caller
copy fault can override status after a successful assignment/release. Reached
guards consume once, including read-only guards. Native padding and the WoW
outside-record tail are deliberately initialized and captured.

A valid Thread must grant `THREAD_SET_INFORMATION`0x20. Assignment0 requires a
`SchedulerSharedData` owner granting0x1, then refuses an already bound/currently
noncurrent target with `STATUS_NOT_SUPPORTED`. Missing/wrong-type owners are
checked before that refusal. Free1 ignores its owner/slot inputs, clears the
current binding and succeeds even if already unbound. Other actions return the
captured `STATUS_NOT_SUPPORTED`; no successful placeholder exists.

An owner lazily reserves8192B `MEM_MAPPED`/allocation protectionRW4 and initially
commits4096B. Slots are64B, allocated lowest-free across128 positions; accessing
the second64 slots commits the second page. The129th active assignment returns
`STATUS_INSUFFICIENT_RESOURCES`0xC000009A without publishing a binding. Free
preserves slot bytes; assignment clears64B. Handles and each thread binding
independently retain the owner. Final owner-reference release retires the whole
view. Thread exit and complete/partially consumed object-table drains reclaim
views. Allocation-token identity prevents retiring or clearing an unrelated
allocation that reused the address. Explicit native section unmap leaves the
owner's cached returned address intact; reassignment can return that unmapped
address. Kernel counter/timing updates beyond the captured profile are unknown.

The PHNT `ntpsapi.h`/`ntpebteb.h` snapshots and MIT notice are retained in
`references/`, with URLs, retrieval dates and SHA-256. The query-only comment
conflicts with native setter success; the captured executable behavior owns
this setter contract. The getter is a native producer oracle, not a new guest
API in this increment. TEB offsets are not inferred from secondary layouts;
installed NTDLL performs its own reached TEB store.

There are26 independently identified native source/executable/stdout producers:
4 mature byte/probe profiles, 1 first-entry capture, 1 cold controlled sequence,
4 lifetime/three-thread profiles, 4 66-worker pools, 4 130-worker capacity runs,
and two sets of4 remote/unmap boundary profiles. Mature replay partitions total
39488 rows (4*9872). Each profile contains3360 action/identity/binding rows,
4704 length/buffer/action/owner rows,1568 invalid-thread/probe-order rows and240
explicit padding/tail controls. Exact native handles and caller bytes are kept.

Fresh ARM64 first-entry capture has one excluded warmup plus two complete
owned parent/child cases. It stops at original NTDLL SVC13 and records the true
LR; parent duplication verifies SchedulerSharedData/grant1 and closes the
inspection alias. Child exit77 is a capture stop, not ordinary guest exit.
The separate cold sequence re-enters only the owned debugger child after its
unchanged first call: 8 calls per case,24 whole4096B page snapshots. Each
resource/pool/capacity/boundary profile runs one excluded warmup and two complete
rounds, with exact input-handle restoration and joined owned workers.

The initial mature source had uncontrolled C++ padding and is excluded from
positive replay. Initial first-entry and resource/capacity probes failed their
own incorrect assumptions; their diagnostic sources/logs remain task-cache
records, and the failed first-entry source/log is retained under
`diagnostic-initial-fresh/`. No failed helper is counted as positive evidence.
The complete text corpus is roughly33MB, justified by exact independent caller
bytes, partition coverage and whole-page/lifetime records. Native DLLs,
executables and archive binaries are excluded; their actual identities remain.

Run `python3 -B check-evidence.py` and `python3 -B check-negative-evidence.py`.
The independent checker reconstructs partitions, status order, all captured
caller bytes, guard effects and binding effects; it checks native source/PE/LAA,
producer exits, fresh original entry/true-LR, owner grant/type and lifetime/pool
controls. Controlled negatives recompute corpus and stdout checksums, so
checksum consistency cannot substitute for semantic/provenance validation.

`assumption-register.json`, `change-surface-map.json`, `bounded-findings.json`
and the final validation record distinguish source, portable tests, actual
native leaves, owning embeddings and ordinary execution. No full-process
completion is claimed from controlled leaves, producer exits or reason5 with
exit_code0. Publication uses exact owned paths through red, RAX origin/master,
then the root gitlink and two engineering records through both root remotes.

Final gates: macOS7741/0/2, Linux7735/0/2 and Windows7153/5/2
(passed/failed/ignored), with the same five Windows BZHI/FP16 baseline failures.
Focused4/4/5, CAPI168 on each host, all-targets and scoped owning embeddings pass.
The unchanged production observer advances115067 to135983 calls (+20916),
then reaches NtOpenThreadToken0x24; the three ordinary CRT programs stop there,
while whoami remains at NtTraceEvent0x5E. No ordinary guest exits are recorded.
The independent checker rejects25 controlled evidence falsifications.
All exact commands/counts/current archive hashes and proof limits are retained
in [validation-summary.json](validation-summary.json).
