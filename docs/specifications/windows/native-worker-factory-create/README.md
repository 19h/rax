# Real NT worker-factory construction

NtCreateWorkerFactory now creates a real process-local guest TpWorkerFactory
object with its completion-port/process dependencies, raw maximum, entry,
parameter and stack configuration. It does not forward a host factory, process
handle or worker thread. This constructor group advances installed-library
startup; successful full applications and worker activation remain outstanding.
The bounded assumptions W1-W5 are reconciled in [task-contract.md](task-contract.md).

## Measured contract

The ten arguments are PHANDLE, ACCESS_MASK, PCOBJECT_ATTRIBUTES, completion-port
handle, process handle, entry pointer, parameter pointer, ULONG maximum,
SIZE_T reserve and SIZE_T commit. The fresh ARM64 startup request uses maximum512,
reserve1048576 B and commit4096 B; arguments9/10 are read from SP/SP+8, not X8/X9.
Portable tests cover all three guest ABIs, and installed ARM64/WoW64 leaf tests
execute real NtCreateWorkerFactory/NtClose leaves. x86 RET40 returns SP+44 B
including the return address; x64 returns SP+8 B; ARM64 leaves SP unchanged.

Native64 ordering is output byte-range probe, port type/access, process type/access,
aligned48 B attribute capture, flag/root/policy checks, then real publication.
WoW64 converts unaligned24 B attributes before its4 B output probe. Bad attribute
Length leaves output untouched; converted/probed kernel failures publish zero
on WoW64. Aliases, page crossings, one-shot guards and partial failure bytes are
checked against original captures. The factory flag mask0x1EF2 admits OBJ_PERMANENT,
unlike the previously measured completion-port mask0x1EE2. RootDirectory with
no name fails STATUS_OBJECT_NAME_INVALID, after invalid-flag precedence.

Completion-port access bit0x2 is mandatory. A real process handle requires
PROCESS_CREATE_THREAD0x2 | PROCESS_VM_OPERATION0x8 | PROCESS_VM_WRITE0x20 =0x2A.
The current-process pseudo handle is accepted; wrong types and denied rights
retain measured NTSTATUS ordering. Cross-process targets, nested name/security/QoS
and ACCESS_SYSTEM_SECURITY authority remain explicit unsupported boundaries.

| Requested access | Granted access |
|---|---|
| Valid specific/standard rights | requested &0xF00FF |
| GENERIC_READ0x80000000 |0x20008 |
| GENERIC_WRITE0x40000000 |0x20004 |
| GENERIC_EXECUTE0x20000000 |0x20003 |
| GENERIC_ALL0x10000000 or MAXIMUM_ALLOWED0x02000000 |0xF00FF |
| SYNCHRONIZE0x100000 | stripped |
| ACCESS_SYSTEM_SECURITY0x01000000 | explicit unsupported privilege boundary |

All ULONG maxima, including0 and0xFFFFFFFF, are retained exactly. Entry/parameter
and nonzero stack sizes are stored without probing or rounding. Zero reserve
and commit default independently to65536 B and4096 B. Corrected actual producer
PE headers verify defaults remain identical for1048576 B/4096 B and2097152 B/32768 B
executable stacks. Initial basic queries show minimum/pending/waiting/total/release
counts0 and MayCreate1; this is constructor state, not activation proof.

Objects::try_create validates dependency types and both reference capacities
before pinning at most two references; publication failure and final close
release them. Dependencies are a CompletionPort and optional Process, so release
recursion has depth1 and cannot form a factory cycle. Guest tests close dependency
handles before factory duplication/final close, and test count/identifier overflow.
Native five-phase queries show the process object has one extra internal handle
through duplication and first close, released on final close. Kernel pointer
counts have query-sensitive bias and are not an exact port-reference oracle.

```text
native process handles: N -> N+1 -> N+1 -> N+1 -> N
factory phase:      before created duplicated first-close final-close
```

Fixed constructor capture/state storage is O(1) time/space; private object-table
operations retain their existing map complexity and page probes retain the
existing mapped-span traversal. No public C ABI, schema or feature default changes.

## Independent evidence and selected gates

| Producer family | Profiles | Rows/profile | Original rows |
|---|---|---:|---:|
| Initial128 B query attempt | ARM64, x64 compatibility, x86, x86 LAA |410 |1640 |
| Corrected basic query | same four |439 |1756 |
| Handle/root/flag ordering | same four |56 |224 |
| Changed executable stack | ARM64, x86 |1 |2 |
| Total | | |3622 |

Basic/order replay partitions1980 rows into1868 exact replays and112 explicit
nested-policy/privilege exclusions. Every original capture has zero external
SEH and equal caller handle counts before/after. Handles are normalized only at
known successful publication fields; statuses, other bytes, grants, inheritance,
raw fields/defaults and object/handle counts remain exact. Native basic query is
an oracle; guest NtQueryInformationWorkerFactory is not implemented by this group.

| Gate | macOS ARM64 | Linux x86-64 container on ARM64 | Windows ARM64 |
|---|---|---|---|
| Constructor filter |5 pass |5 pass |6 pass, including installed leaves |
| Completion-port regression |5 pass |5 pass |6 pass |
| Event regression |7 pass |7 pass |12 pass |
| Full library: pass/fail/ignored |7618/0/2 |7611/1/2 |7014/5/2 |
| C API |168 pass,0 doctests |168 pass,0 doctests |168 pass,0 doctests |
| All-target build | completion | completion | completion |
| Affected integration |544 pass, memory0 cfg-selected |544 pass, memory0 cfg-selected |memory4 pass |
| Five owning C++ consumers | pass | pass | pass |

The single Linux full-run failure is the existing multishot-timeout test
(two expiry records versus one); its exact isolated rerun passes, cause unknown.
Windows retains four BZHI flag-lowering failures and one FP16 runtime-lowering
failure, named in the curated full log. Full-suite failures are not reported as
green gates. Earlier interrupted full runs retain unknown terminal status in
interrupted-gates.json; resumed commands start at the remaining full/CAPI/build/
integration gates. Windows's original unified session is unavailable; terminal
Cargo build/test markers and no remaining native Cargo/rustc/test process confirm
completion. Lost process exit codes remain null rather than invented.

Fresh owning consumers use rootf4ef0960 and the current worker-factory source.
The later unrelated root19a3cf80 staged-knowledge-tool change is preserved by
fast-forward and is outside this source/consumer proof. macOS CLI/MCP builds,
seven selected CTests and both protected-text scans pass. Linux uses byte-identical
previous generated static-text/Monocypher/JSON inputs; this is not a fresh package
generation claim. No current package, live IDA or Qt-harness execution is claimed.

Fresh native static-CRT Assist/core archives retain exact sizes1111407934 B/
724701192 B and full4673/260 member walks.22 compiled-source and2 consumer-helper
identities agree with the live owning sources; core feature selection is empty.
The native3675-file source-overlay check has56 patches and one explicit offline
Cargo-source adapter, with no changed ARM64 code-generation flags. Later evidence/
documentation edits are not represented as the original compile-time overlay.

The identical11177 B observer (SHA-256 in ordinary-observer-pair.json) gets
NTSTATUS0 at42076 for NtCreateWorkerFactory0xD5/PC0x180001D90 and stops at42165
for NtCreateTimer20xCC/PC0x180001D00.42165-42076=89 scheduler calls, not instructions.
All four ordinary probes still report reason5/exit_code0/internal failure;
none is reported as successful guest termination.

## Plane and scope audit

| Plane | Result and evidence |
|---|---|
| NT dispatch/transport | New ten-argument export; existing native/WoW64 service transports exercised |
| Private object state/lifetime/wait exhaustiveness | Real factory variant, dependency pins/releases, nonsignaled exhaustive match |
| ISA/decode/direct execution/CPU state | Unaffected: no instruction/register contract or decoder change |
| SMIR lift/IR/interpreter/optimizer/lowering/JIT/backend | Unaffected: private Windows HLE fields; no operation or native emission change |
| C API/Assist process consumers | Same compiled core through existing opaque process contract; five consumers per host |
| Plugin/UI/main-thread/lifecycle/conversation/prompt | Unaffected: no consumer of private factory fields; no root product-source edit |
| Tool schema/permissions/mesh/MCP/transport/crypto/persistence | Unaffected: private emulator state, no wire/schema/capability change |
| SDK/ABI/optional engines | No public layout or dependency pin besides the required RAX gitlink; drift/link consumers pass |
| Update/release/package | No new runtime file/dependency/layout/default; native package execution remains unproved |
| Windows/macOS/Linux targets | Shared source compiles/runs in each owning core/archive consumer; physical x64 Windows/Linux proof remains distinct |
| Tests/docs | Red constructor baseline, modeled/native leaves, replay/lifetime/capacity, strict evidence manifest |

High remaining items: timer services, worker activation/packet/wait/APC scheduling,
namespace/security/cross-process policy and successful full applications block
the full goal. Medium: full-run failures of unknown cause, physical pure x64
Windows/Linux proof, and pre-existing vm-memory ARM64 stdcall warning. Low: none
discovered. Generic object waits are not established as native worker-factory wait
semantics by this constructor group. No adjacent non-blocking change is included.

QG1-QG7 cover this bounded constructor group: no normative content; W1-W5 register
with falsification probes; all measured constructor acceptance paths and explicit
exclusions covered; exact byte widths/masks/cleanup/delta; known failures and metadata
correction retained; primary/capture/producer/source/archive/gate identities bound;
scope and platform limits recorded. Full-userland acceptance remains incomplete.
The positive checker and four negative validation probes are recorded separately.
