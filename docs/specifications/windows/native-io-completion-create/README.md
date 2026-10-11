# Native NtCreateIoCompletion construction

The constructor now creates a real process-local reference-counted IoCompletion
object. The installed-loader request at scheduler call42051 is
[out=0x11032C0,access=0x1F0003,ObjectAttributes=NULL,concurrency=4]. This group
covers construction and close. Full packet posting/removal, file association,
APCs and runnable-thread accounting remain requirements of the overall goal.

The four argument types are PHANDLE, ACCESS_MASK, POBJECT_ATTRIBUTES, ULONG.
Pointer publication is4 B WoW64 or8 B native64; OBJECT_ATTRIBUTES is24/48 B.
Native output probing precedes attribute capture; WoW64 captures/converts the
attributes first. Native attributes require8-byte alignment, WoW64 accepts byte
alignment, and PHANDLE accepts byte alignment. Wrong attribute Length fails
before WoW64 output probing and leaves its output unchanged. Kernel validation
failures after that probe copy a zero WoW64 HANDLE; native64 leaves its output
unchanged. Attribute capture precedes aliasing publication. Guards are consumed
once and only in the observed probe order. Final close restores object/handle
counts; failed construction publishes no leaked resources.

Completion-port attribute mask0x1EE2 excludes OBJ_PERMANENT0x10, while the existing
event mask0x1EF2 accepts it. Nonzero RootDirectory with no ObjectName returns
STATUS_OBJECT_NAME_INVALID; invalid flags take precedence. Unsigned concurrency
0,1,2,4,8,16,0x7FFFFFFF,0x80000000,0xFFFFFFFF succeeds in every native producer.
The model stores all nonzero ULONG values and projects zero to its one guest CPU.
This projection does not constitute proof that dequeue enforces concurrency.

| DesiredAccess | GrantedAccess |
|---|---|
|0|0|
|1|1|
|2|2|
|3|3|
|4|0|
|0x000F0000|0x000F0000|
|0x00100000|0x00100000|
|0x001F0003 or0x001F0007|0x001F0003|
|0x02000000|0x001F0003|
|0x80000000|0x00020001|
|0x40000000|0x00020002|
|0x20000000|0x00120000|
|0x10000000|0x001F0003|

The SYSTEM oracle's ACCESS_SYSTEM_SECURITY grants do not authorize a guest
privilege. Non-null namespace/security/QoS attributes and that privilege stop
explicitly. Four303-row constructor captures plus four21-row RootDirectory
captures contain1296 observations. The portable replay checks1184 statuses,
exact bytes except newly allocated handle identity, publication extents,
aliases, guard state, grants and lifetime. The112 excluded observations remain
original retained data for explicitly unimplemented nested policy/privilege.

A behavior-preserving extraction shares capture, mapping and fallible handle
publication with native event creation; event-specific type ordering/flags and
unsupported diagnostic text remain in the event constructor. Constructor work
uses O(1) scratch; lowest-free handle selection is O(H) for H open handles.
No host completion port is created by a guest request. No public C API, ABI,
schema, feature default, dependency pin or package layout is changed.

|Assumption|Basis|Dependent result|Stress/probe|Status|
|---|---|---|---|---|
|I1 unnamed startup request|unchanged published NUMA observer|constructor frontier|rebuild same observer with fresh owning core|confirmed before/after identical observer|
|I2 existing object lifetime owns close|objects.rs and1184 replays|real object publication|failed output/flags/guards and final close counts|confirmed for measured paths|
|I3 events share capture/access but flags differ|four native profiles and event regressions|object_create.rs extraction|port OBJ_PERMANENT/root/alias/guard matrix and event tests|confirmed|
|I4 zero requests default CPU count|Microsoft contract, guest PEB/topology1|stored concurrency1|future dequeue/concurrency oracle can falsify|retained, not scheduling proof|

All five portable constructor tests and seven event regressions pass on macOS
and Linux. Windows passes six constructor tests including installed ARM64/WoW64
NtCreateIoCompletion/NtClose leaves with actual x86RET16 cleanup. Full runs:
macOS7613 passed/0 failed/2 ignored; Linux7606/1/2; Windows7008/5/2. Linux's
multishot-timeout expected one expiry and observed two; its isolated check passes,
and cause remains unknown. Windows retains the four BZHI and one FP16 lowering
failures. A should-panic Darwin wait test is not an additional failure.

macOS/Linux C API168 checks, all-target builds and integrations pass. Windows
C API168 unit checks passed but the initial doctest phase collided with a concurrently
started clean. That clean also hit an open executable. The owning build and
remaining Windows gates use separate fresh target directories. The isolated
C API run passes all168 unit checks and its zero-test doctest selection.
All-target build and four Windows memory integration tests also pass. The accidental overlap is recorded, not a green C API gate.

Five owning C++ consumers pass on Linux; current-root macOS CLI/MCP build,
protected-text scans and all seven selected CTests pass. Root source319b66 is
isolated in a persistent worktree, preserving concurrent work in earlier
checkout. Windows verifies3624 matching source files plus a separately hashed
offline dependency-source adapter; two prior formatting differences are corrected
only in the task-local Windows source copy. This is host/container proof rather
than package, live IDA/Qt or pure physical x86-64 Windows/Linux proof.

High/full-goal blockers: native completion packet/wake/association scheduling,
named NT namespace/security/QoS/privileges, remaining services and successful
applications. All constructor-group runtime acceptance gates pass; broader full-run failures
and full-goal blockers remain explicitly recorded. Archive identity and ordinary
observer advancement are verified.
Medium/nonblocking: known full-run timing/lowering failures and pinned vm-memory's
future-incompatible stdcall diagnostic on ARM64 Windows. No adjacent fix is
included merely because it was discovered. Low: none.

Planes: NT export dispatch, guest object state and shared event construction are
affected. Existing CPU, HLE transport, memory probes and close/wait consumers are
reused. C API/process-tool/SDK consumers compile the same core without a schema
change. UI, IDA main-thread dispatch, conversation lanes, mesh/MCP protocol,
crypto, persistent state and updater semantics do not consume these private NT
constructor fields; packaging requires no new file or dependency. The fresh owning builds compile the shared archive and process consumers.
Plugin linking, live product/package proof and broader native-platform
expansion remain distinct requirements of the full goal.

Primary contracts are in primary-contract.md, pinned PHNT header/license and
original native producer/capture files. No proprietary DLL/PDB/executable is
redistributed. The exact evidence manifest, archive/source identities, native observer pair
and QG1-QG7 audit bind this group to its actual proof scope.

Fresh native owning proof: Assist archive1,112,440,112 B/4673 members, SHA-256
8878b16a47f6e38b63364863aec7672793f554e3f65b39b86ec0f1cb6ffe63b1; selected
empty-feature core725,713,258 B/260 members, SHA-256
5efa936bf72ef30dc1c280d18808852eff8a31dcaadd4ff6d78b84083f70eea3. Complete
member walks,18 owned/preserved source hashes and two actual helper hashes pass.
The10,919 B ordinary observer is identical before/after (SHA-256
75d006334ffed51536d5d5b1c314c6b21b1e2e4f873add8db1b4e519874635f0).
At42051, NtCreateIoCompletion0xB2 at0x180001B60 changes from internal unsupported
to NTSTATUS0. The next stop is NtCreateWorkerFactory0xD5 at0x180001D90 at42076.
42076-42051=25 additional scheduler calls, not instructions. The four owning
ordinary programs all stop there with C API reason5/exit_code0, an internal
failure rather than guest completion. Native worker-factory/threadpool behavior
therefore becomes the next full-goal frontier.

QG1-QG7: no normative content; I1-I4 register with concrete falsification probes;
constructor acceptance covers all measured common cases and explicit exclusions;
4/8 B pointers,24/48 B attributes, ULONG32 bits and25 scheduler-call difference
are reproducible; no unsupported scheduling/security/application success claim;
primary/capture/producer/source/archive/gate provenance is retained; broader
high/medium boundaries remain listed. Publication does not mark the full goal
complete. Positive validator and four negative cases pass; optimized validation
is rejected and no manifest omission is accepted.
