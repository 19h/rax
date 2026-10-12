# Installed Timer2 construction (2026-10-12)

NtCreateTimer2 constructs a real, dormant process-local IRTimer through the
installed NTDLL service path. The private variant retains validated raw
attributes and an optional captured ULONG IR ID, distinct from legacy Timer.
No host timer is acquired, armed or forwarded. Existing fallible object/handle
publication and final-close release own the lifecycle. Shared pure HLE code
compiles on Windows, macOS and Linux; native leaves additionally exercise the
installed ARM64 and WoW64 transports.

## Independent native contract

The original create/order/reduced/alias programs retain835/842/267/154 rows for
each ARM64/x64/x86/x86-laa profile:
4*(835+842+267+154)=8392 observations. Native handles before/after are identical
for every row; no external SEH occurred. Replay checks8288 rows exactly and
excludes104 ACCESS_SYSTEM_SECURITY privilege-dependent outcomes explicitly.
Expected statuses/bytes are original captures, independent of emulator output.
All16 actual producer binaries/source identities and the selected installed
ntdll identity are retained. The kernel is ARM64 Windows11 build29683.1000;
x64/x86 compatibility processes are not physical x64 Windows proof.

| Condition, in validation order | Constructor result |
|---|---|
| Attributes outside0x8000000E, or IR2 combined with NO_WAKE8 | STATUS_INVALID_PARAMETER_4,0xC00000F2 |
| Any non-null ObjectAttributes value | STATUS_INVALID_PARAMETER_3,0xC00000F1; no OA memory capture |
| Non-IR attributes with non-null TimerId | STATUS_INVALID_PARAMETER_2,0xC00000F0; no ID memory capture |
| IR ID0, NULL, or >=20 | STATUS_INVALID_PARAMETER,0xC000000D |
| IR IDs2,3,4,6,7,17 | STATUS_ACCESS_DENIED,0xC0000022 |
| IR IDs1,5,8..16,18,19 | Accepted measured UserMode role profile |
| Successful construction | IRTimer, initially unsignaled; ordinary close releases it |

ID outcomes persist with all producer-process privileges disabled; another
caller SID/build may falsify them. No SYSTEM identity, token authority or
cross-process permission is inferred. Native NtQueryTimer returns
STATUS_OBJECT_TYPE_MISMATCH0xC0000024 for every successful new handle, including
all-access grants. Six reduced-token type/wait probes per profile report
IRTimer and zero-time WAIT_TIMEOUT0x102. The reduced producer's wait-error field
has unspecified printf argument evaluation order and is diagnostic only.

Native64 probes the unaligned PHANDLE extent before reading optional unaligned
ULONG ID. WoW64 captures the IR ID before its temporary-handle copy when the
output is non-null; null output fails before ID capture on both paths. Parameter
checks precede these probes. WoW validation/ID failures copy zero to non-null
output; that copy's access/guard fault can replace the earlier NTSTATUS. A
publication-phase guard fault performs one store attempt, preserves sentinel
bytes and rolls back resources; it does not retry after consuming the guard.
Dual-guard repeat probes distinguish these paths. ID capture precedes aliased
HANDLE stores; all partial aliases, guard transitions and accessible suffix
bytes remain checked. Relocation applies only to exact known native HANDLE
spans and one initialized OA nested pointer slot, with native shape/value
assertions before replacement. No byte range is discarded to make replay pass.

| Desired access | Guest/native grant |
|---|---|
| Specific supported rights | mask0x001F0003 |
| GENERIC_READ0x80000000 |0x00020001 |
| GENERIC_WRITE0x40000000 |0x00020002 |
| GENERIC_EXECUTE0x20000000 |0x00120000 |
| GENERIC_ALL0x10000000 / MAXIMUM_ALLOWED0x02000000 |0x001F0003 |
| ACCESS_SYSTEM_SECURITY0x01000000 | Explicit unsupported guest privilege policy |

Transport checks preserve x64 fifth argument at SP+40 B=8 B return+32 B shadow
space; installed x86 leaf RET20 releases5*4 B arguments, total return24 B;
ARM64 uses X0..X4. Scalar fields are ULONG/ACCESS_MASK32 bits; output uses guest
HANDLE width4 B or8 B. Fixed constructor work is O(1), except existing handle
allocation O(H) time/O(1) extra space for H open handles. Capture/replay memory
is bounded by fixture rows and mapped guest pages; no host timer is scheduled.

## Validation and limits

Portable constructor, exact replay, duplicate/final-close dormant waits and
explicit privilege refusal tests pass4/4 on macOS and Linux. Native constructor/leaf tests pass5/5. The remaining completed gates and
limits are recorded below. The separate red baseline
fails on the original missing service; an earlier test-source compile typo is
retained separately and is not semantic red proof.

macOS full7622/0/2, Linux7613/3/2 and Windows7019/5/2
pass/fail/ignored are retained. Linux
multishot expiry, timeout update and restart-sleep timing assertions fail in the
combined full run and pass individually; root cause unknown. Windows retains the same four BZHI flag-lowering and
one FP16 runtime-lowering failures as the published worker-factory group.
No failed full run is represented as green. Fresh root6e49c788 consumers include the prior
knowledge helper changes and preserve the release-note-only fast-forward.
Fresh macOS CLI/MCP, seven selected CTests and both protected-text scans
pass; all five owning C++ consumers pass on macOS, Linux and Windows. Linux reuses byte-identical prior generated static-text,
Monocypher and JSON inputs, so this is not fresh package generation proof.

The published worker-factory post-change observer at42165 is the explicit
baseline. Before/after observer bytes are identical. Native production returns NTSTATUS0 at42165 for NtCreateTimer2 service0xCC
at PC0x180001D00. It stops at42172 for NtCreateWaitCompletionPacket service0xD2
at PC0x180001D60, with arguments[0x1103318,1,0].
42172-42165=7 scheduler calls, not instructions. All four ordinary programs
(smoke, MSVCRT, UCRT and installed whoami) now reach that explicit internal
failure: reason5/exit_code0 is not guest exit.

The current native static-CRT Assist/core archives are1112022088 B/725328222 B
with4673/260 complete member walks. Their27 source identities,2 helper identities
and empty core feature selection are verified. The resolved native source
snapshot checks16166 files and12498 patches; the original per-file transfer was
interrupted before Cargo. Its bundled first check rejected a duplicate offline
Cargo adapter; the exact previous374 B adapter was restored and the resolved
check passed. Original failed checks and retry provenance remain separate from
syscall measurements. Later evidence/doc updates are not represented as original
compile-time source-overlay bytes.

## Assumptions and change surfaces

The reconciled T1-T4 Assumption Register is in task-contract.md. T3 remains a
selected-build/caller limitation, with concrete alternate-token/build probes.
Private NT exports/status constants/object/wait integration and service tests
consume the change. Plugin/idalib/SDK/terminal consumers receive it through the
existing C API/archive, whose layout and feature selection remain unchanged.
ISA/decode/SMIR/interpreter/lowering/JIT are unchanged. Root UI/main-thread,
lanes/context/prompt, tool schemas, permissions, mesh/crypto, persistence,
optional engines, release and package layout have no changed contract.

High remaining full-goal requirements: arming/cancellation/expiration/APC,
worker activation/packet/wait scheduling, security/namespace/cross-process
policy and successful complete ordinary applications. Medium limits: known
full-run failures, physical x64 Windows and alternate caller/build evidence,
current package/live IDA/Qt execution. They remain recorded task boundaries;
constructor success does not discharge them. Low out-of-scope items: None.

The native vm-memory build retains its existing unsupported_calling_conventions
warning for extern stdcall on ARM64. It compiles with pinned Rust1.95.0 and its
actual mapping tests run; future compiler rejection is a medium dependency risk,
not a timer constructor exception or a dependency-pin change.

Strict evidence validation rejects incomplete manifests, changed raw bytes,
rehashed impossible statuses, empty filtered test records and optimized Python.
Its complete gate/result file set and27 live source identities must agree.
Original native logs, build logs, drivers, primary header/license and failure
records remain separate from generated replay expectations and updated docs.

| Gate | macOS ARM64 | Linux amd64 under host ARM translation | Windows ARM64 |
|---|---|---|---|
| Constructor/replay/lifetime/refusal |4 pass |4 pass |5 pass, including installed leaf |
| Worker factory |5 pass |5 pass |6 pass |
| Event regression |7 pass |7 pass |12 pass |
| Completion port |5 pass |5 pass |6 pass |
| Full pass/fail/ignored |7622/0/2 |7613/3/2 |7019/5/2 |
| C API / doctests |168/0 pass |168/0 pass |168/0 pass |
| All-target build |complete |complete |complete |
| Integration |544 pass; memory0 cfg-selected |544 pass; memory0 cfg-selected |memory4 pass |
| Five owning C++ consumers |pass |pass |pass |

The strict independent evidence validator and its positive/five negative
checks complete the constructor group. All required selected gates above run;
failed full-suite rows and platform/build/caller limitations remain explicit.
