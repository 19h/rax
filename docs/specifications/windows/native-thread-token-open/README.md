# Native thread-token opening without impersonation (2026-10-13)

This group implements `NtOpenThreadToken` in the current guest personality,
whose threads have no impersonation token attachments. It adds the measured
`STATUS_NO_TOKEN`0xC000007C / `ERROR_NO_TOKEN`1008 translation. It does not
create token objects, implement positive token/DACL behavior or call host NT.
Token attachment and impersonation-producing guest APIs remain explicit
unsupported boundaries; class5 `NtSetInformationThread` refusal is tested.
Native positive-token captures are retained as falsification controls for a
universal NO_TOKEN implementation.

| Input | Kernel absence result |
| --- | --- |
| Current-thread pseudo -2 | STATUS_NO_TOKEN0xC000007C |
| Missing/invalid handle; negative pseudo -3 through -7 | STATUS_INVALID_HANDLE0xC0000008 |
| Process pseudo -1, Event or Process object | STATUS_OBJECT_TYPE_MISMATCH0xC0000024 |
| Thread granting neither0x40 nor0x800 | STATUS_ACCESS_DENIED0xC0000022 |
| Current/local/ended Thread granting either query bit | STATUS_NO_TOKEN0xC000007C |

Native64 (ARM64 and x64 compatibility) failure paths never probe or modify the
output pointer. WoW64 skips a null output pointer; a nonnull output receives
zero4B even on kernel failure. Caller copy faults override the kernel result.
Writable guards consume once; readonly guards deny with ACCESS_VIOLATION and
remain armed. Four-byte cross-page faults preserve all caller bytes without a
partial store. The common write/guard helper is the existing constructor policy
extracted unchanged; the scheduler slot setter keeps its different captured
regular guard policy. DesiredAccess and OpenAsSelf are still decoded, but no
access-token ACL decision is reached when the target has no token.

The capture platform is ARM64 Windows10.0.29683.1000 under the recorded SYSTEM
principal. Four caller profiles are ARM64, x64 compatibility, x86 and x86 LAA.
Physical x64-kernel and other kernel/DLL behavior are unknown. Actual source,
executable PE machine/LAA/size/hash, stdout identity and installed DLL identity
are bound by `native-thread-token-producer-identities.json` for13 producers.
No DLL, EXE or archive binary is committed.

Four mature producers have6588 rows each:5760 state/thread/access/OpenAsSelf
rows,540 state/access/OpenAsSelf/buffer rows and288 invalid-target/probe rows.
Thus4*6588=26352 native mature observations. The state0 absent-token partition
has1548 rows/profile;4*1548=6192 portable replays. States1..4 retain anonymous,
identification, impersonation and delegation controls, including actual native
Token handle births/grants/types and close/revert checks. Canonical probes
validate exact Token namespace plus16 owned input IDs, close14 caller-owned
inputs and join owned workers. Global handle counts/exact-set observations are
kept separately; global snapshot atomicity is not assumed.

Four additional pseudo producers have40 rows each:2 token states *2 width
modes *5 negative selectors *2 OpenAsSelf values. The80 absence rows are
replayed; the actual installed RTL mapping is1008 in every profile. Mature
extra selectors were zero-extended32 on Native64; separate pseudo controls
cover full-width signed -3..-7 as well. Four boundary producers have24 rows
each:2 target identities *3 cross-page offsets *4 page protections. All96 rows
are replayed, including writable guard consumption and readonly guard retention.

Fresh ARM64 capture records original installed NTDLL SVC36 (`0x24`), entry
0x7FF8DE2E1260, debug entry PC=entry+4B and actual LR0x7FF8DE3ACECC. Return debug
PC=LR+4B. First arguments are[-2,0x2001C,0,output-stack-pointer]; result is
C000007C and all8 output bytes are unchanged. There is one excluded warmup and
two complete cases with restored exact parent handle sets. Child exit77 is a
debugger capture stop and supplies no ordinary guest-exit proof.

PHNT `ntseapi.h`, Microsoft primary API documents and licenses are retained
with URLs/retrieval dates/checksums in `references/`. Microsoft documents
THREAD_QUERY_INFORMATION; the installed captures also accept QUERY_LIMITED
0x800. The observed executable owns this build-specific access contract.
Existing mingw-w64 14 snapshots provide the two status/error constants; all
four native RTL calls independently verify their translation.

The initial producer overran its own top-pointer inspection arithmetic; v2/v3
incorrectly required an atomic global process-handle set. They are excluded
cache-only diagnostics. Fresh metadata initially had a stale source-byte count;
the canonical producer was corrected and rerun with the actual9502B source.
None of these failed/superseded helpers is counted as positive evidence.

The guest algorithm decodes four ABI arguments, validates thread identity/type
and either query bit, returns the absence status, then performs the WoW64 copy
if nonnull. Handle-table lookup is O(log H), where H is the handle count; the
object lookup has expected O(1) cost. The output payload is exactly4B, touches
at most2 pages and requires O(1) additional space. Fault/guard translation uses
the existing virtual-memory query/guard abstraction. No new scan/allocation or
host service occurs. Replays additionally assert no guest object birth and
correct return PC/SP, including installed WoW64 RET16.

Run `python3 -B check-evidence.py` and `python3 -B check-negative-evidence.py`.
The independent checker reconstructs input partitions, statuses, bytes and guard
effects; it verifies actual native source/PE/LAA/stdout, original fresh entry/LR,
positive Token controls, cleanup and RTL translation. All33 controlled
falsifications refresh corpus/stdout checksums and must still fail their named
semantic/provenance assertion. Assumptions, both complete plane maps and bounded
findings are separate JSON records. Final source/gate/archive/ordinary proof
must remain separate: controlled producer success and reason5/exit_code0 do
not establish an ordinary guest exit. The overall same-host process goal stays
active until full native-library execution succeeds on all three platforms.

Final gates: macOS7745/0/2, Linux7739/0/2 and Windows7158/5/2
(passed/failed/ignored), with the exact five preceding BZHI/FP16 failures.
Focused4/4/5, CAPI168 each, all-targets and scoped owning embeddings pass.
The unchanged11177B production observer advances135983->145453
(+9470 calls), then stops at native NTDLL service NtOpenSection (0x37) at 0x1800013b0 is not implemented.
Three ordinary CRT programs share that frontier; whoami retains NtTraceEvent0x5E.
All four report reason5/Internal,0 proven guest exits. Exact commands/counts,
archive identities and exclusions are in [validation-summary.json](validation-summary.json).
