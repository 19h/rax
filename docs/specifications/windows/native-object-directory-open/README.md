# Native NtOpenDirectoryObject and guest directory resources

This increment implements snapshot-backed native directory opens and their
handle lifecycle using installed Windows 10.0.29683.1000 ARM64 behavior.
The unchanged ordinary observer advances from scheduler turn 90,924 to 91,011:
91,011 - 90,924 = 87 scheduler calls. All four ordinary programs now reach
unsupported `NtOpenSymbolicLinkObject` service 0x138 at PC 0x1800023C0 before
guest exit. The earlier NtOpenDirectoryObject boundary is passed.
Full same-host ordinary process execution remains an active incomplete goal.

Native acquisition selects the fixed NT directories `\`, `\KnownDlls` and
`\KnownDlls32`, their entry names/types, query-only KnownDllPath link targets,
maximum grants, ACCESS_SYSTEM_SECURITY status and case policy. Host handles
close before selection returns. Guest names, pointers, handles and requested
access never cross into host NT calls; guest opens allocate independent process
resources with shared immutable Arc identity. The default closed personality
has no installed catalog. NT counted UTF-16 names retain NT grammar and are
separate from native filesystem paths. Unselected directory children, matching
WoW64 system-root redirection and unselected security/QoS contexts stop with
explicit unsupported results.

| Field | Native64 offset/size, B | WoW64 offset/size, B |
|---|---|---|
| OBJECT_ATTRIBUTES.Length | 0/4 | 0/4 |
| RootDirectory | 8/8 | 4/4 |
| ObjectName | 16/8 | 8/4 |
| Attributes | 24/4 | 12/4 |
| SecurityDescriptor | 32/8 | 16/4 |
| SecurityQualityOfService | 40/8 | 20/4 |
| Total OBJECT_ATTRIBUTES | 48 | 24 |
| UNICODE_STRING.Length | 0/2 | 0/2 |
| MaximumLength | 2/2 | 2/2 |
| Buffer | 8/8 | 4/4 |
| Total UNICODE_STRING | 16 | 8 |
| Output HANDLE | 8 | 4 |

Native64 probes the entire output before input capture, requires 8 B attributes
alignment, captures all 48 B, validates flags and then follows subordinate
pointers. WoW64 checks the 24 B Length, copies RootDirectory and ObjectName,
follows the 8 B name descriptor before the remaining attributes fields, then
performs security/QoS conversion and measured name-prefix reads. It calls the
kernel model through a private output slot and copies to the caller afterward.
Caller-copy failure can override an input status or follow successful open;
owned unprotected directory handles are released on that copy fault. Inputs
and names are captured before aliased output stores. Guard consumption follows
the first reached probe. MaximumLength is ignored by the NT open, although
WoW64 copies it during conversion. Buffer alignment precedes odd/oversized
Length checks. Empty counted names do not probe Buffer.

The selected catalog validates entry type, canonical spelling, forced case
policy and symbolic-link targets before reporting missing name, missing
intermediate path, type mismatch or invalid syntax. Unknown captured children
retain an unsupported distinction. OBJ_EXCLUSIVE/OBJ_OPENIF validation follows
successful namespace lookup in the measured profile; unknown flags fail earlier.
OBJ_DONT_REPARSE returns STATUS_REPARSE_POINT_ENCOUNTERED on a reached link.
Generic read/execute map to 0x20003, write to 0x2000C, all to 0xF000F.
MAXIMUM_ALLOWED expands from the captured ceiling; ACCESS_SYSTEM_SECURITY uses
the separately captured current subject status, after DACL ceiling validation.
The observed directory grant excludes SYNCHRONIZE; native wait admission fails.
Query/wait records constrain metadata/access but do not establish guest
NtQueryObject or NtWaitForSingleObject implementations.

Directory aliases retain one object identity after namespace drop. Duplicate
access, SAME_ACCESS, SAME_ATTRIBUTES, inheritance, protection, close-source
priority, lowest-free handle reuse and final drain match the independent kernel
corpus. Protected sources suppress close without preventing duplication.
Native64 duplicate output faults precede the operation; WoW64 faults can follow
it. A WoW64 SAME_ATTRIBUTES copy failure can leave a protected duplicate live
and unreturned, matching the measured native handle delta. Guest objects retain
no native handle or callback. Other object kinds keep their existing policies.

The selector admits at most 8,192 total entries and 16 MiB of captured UTF-16
name/type/target bytes across at most three catalogs, with 32,766 code units per
name/target, 255 units per type and a 65,536-entry upcase table. The native
adapter uses initialized aligned 128 KiB enumeration scratch, bounded strings,
two matching sorted passes and at most three retries. Immutable folded keys,
the shared upcase table and object metadata add storage beyond the raw 16 MiB
payload bound. For E entries and U captured units, construction has an upper
bound O(U log E + E); retained space is O(U + E), plus the fixed upcase table.
Lookup over L units has an upper bound O(L log E) and O(L) temporary storage.
Directory-open name capture is bounded below 65,534 B.

| Native family | Cases/profile | Profiles | Replayed rows |
|---|---:|---:|---:|
| Names, rights, lengths, flags, roots and aliases | 967 | 4 | 3868 |
| Pointer, guard, alignment and page crossing | 5000 | 4 | 20000 |
| UTF-16, typed paths, security, QoS and reparsing | 762 | 4 | 3048 |
| Relative roots, grants and tagged handles | 864 | 4 | 3456 |
| Duplicate rights, inheritance and close | 1200 | 4 | 4800 |
| Duplicate output faults and unreturned handles | 112 | 4 | 448 |
| Isolated subject privilege phases | 72 | 4 | 288 |
| Flag/length/pointer error priority | 420 | 4 | 1680 |
| Protected source and SAME_ATTRIBUTES policy | 1200 | 4 | 4800 |
| Name extents, prefix probes, roots and NULL output | 6656 | 4 | 26624 |
| Protected duplicate copy faults/unreturned handles | 168 | 4 | 672 |
| Total syscall/duplicate rows | 17421 | 4 | 69684 |

Eleven native families over four profiles and one fresh producer give 45 actual
source/PE/stdout identities. Each snapshot contains 151 namespace entries,
separate from the 69,684 syscall/duplicate rows. Profiles are ARM64, x64
compatibility, WoW64 x86 and WoW64 x86 LAA on the same ARM64 kernel. Native probe
principal is NT AUTHORITY\SYSTEM, SID S-1-5-18. The privilege producer modifies
only an owned duplicated impersonation token, restores prior thread-token state
and verifies the primary token remains unchanged. Every completed mature row
restores its owned native handles.

Two complete fresh ARM64 original-entry/true-LR captures observe installed
MEM_IMAGE NTDLL with Length48, flags0x40, RootDirectory0, desired access3 and
ObjectName `\KnownDlls`. Both owned debugger children terminate77 after capture,
with exact native handle-set restoration; this is separate from guest exit.
An excluded debugger warmup changes the host handle count52->57 and has
exact-handle-set0; its detailed allocation ownership is unknown. Fresh
compatibility startup and a physical x64-kernel capture remain unknown.

The four 6,656-case name-extent profiles constrain the loaded WoW64 converter.
For the measured negative-prefix paths, case-insensitive probes read WCHARs to
first mismatch; case-sensitive probes read the first16 B before mismatch,
including tested unaligned buffers. Prefix source is RtlGetNtSystemRoot, here
`C:\WINDOWS`, rather than the current session identifier. Raw installed image
inspection found different alignment-dependent memcmp paths. Complete loaded
ARM64X fixup/target equivalence is unknown; raw-image disassembly alone does not
prove the executed target. Matching system-root redirection remains explicitly
unsupported. Installed DLL binaries stay in the task cache; disassembly text,
image identities and measured native rows are retained here.

Primary public declarations:
[Microsoft NtOpenDirectoryObject](https://learn.microsoft.com/en-us/windows/win32/devnotes/ntopendirectoryobject),
[Microsoft ZwOpenDirectoryObject](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntifs/nf-ntifs-zwopendirectoryobject),
[Microsoft OBJECT_ATTRIBUTES](https://learn.microsoft.com/en-us/windows/win32/api/ntdef/ns-ntdef-_object_attributes).
The unchanged HTML copies and their retrieval/source/hash metadata are under
`references/`. Public declarations establish the three-argument API and field
names; private validation order and loaded WoW64 behavior come from the native
kernel corpus, not from generic Nt/Zw documentation.

| Host | Focused passed | Full passed/failed/ignored | C API | Integration | C++ consumers |
|---|---:|---|---:|---:|---:|
| macOS | 27 | 7713/0/2 | 168 | 544 | 5 |
| Linux | 27 | 7707/0/2 | 168 | 544 | 5 |
| Windows ARM64 | 25 | 7121/5/2 | 168 | 4 | 5 |

Fourteen new portable tests and one Windows installed-leaf test are included in
these focused filters; other selected tests are prior directory regressions.
All 69,684 observed rows replay on every host. The native leaf test executes
installed ARM64 and x86 NTDLL, including x86 RET12/16 B caller-stack cleanup.
Full suites are unfiltered. Windows retains the same four BZHI lowering and one
FP16 runtime failures in untouched source. No assertion or platform selection
is weakened. The zero-test user_windows_memory binary on Unix is distinguished
from Windows's four actual memory integration tests. All-target builds use
empty core features; C API tests run serially.

Mac owning CLI/MCP, seven selected CTests and both protected-text scans pass.
Linux and Windows owning shipping archives and the five C++ consumers pass.
Unix owning root source is frozen at e2fc5b48; native Windows root acquisition
includes f386ed2a. `owning-root-scope.json` records the concurrent root boundary:
the nine process adapter/factory/header/consumer files match the captured current
root, while unrelated Cluster/MCP/public-launcher changes are excluded from
these owning builds. This is compiled current-RAX evidence inside the frozen
owning embedding, not full validation of concurrent root work. Public C API
1.11.0, opaque Assist Rust ABI, layouts, feature defaults, dependency pins and
package layouts are unchanged. Public Rust Object gains Directory. Full
packaging, live IDA and standalone Qt harness runtime checks are not claimed.
The static-CRT Assist archive is 1130815748 B with 4673 completely walked members;
the empty-feature core rlib is 743232692 B with 260 walked members.
Actual source/archive identities bind 79 Rust source units and two unchanged
helpers. Archive and compiled-source SHA-256 values are retained in the owning
identity record.

Run `python3 docs/specifications/windows/native-object-directory-open/check-evidence.py`
from the RAX root. It verifies exact recursive evidence coverage, actual
source/PE/stdout identities, independently reconstructed producer partitions,
same-width semantic agreement, native handle/subject restoration, gate counts,
installed leaf execution, owning archive identities and current source hashes.
Rust replay separately compares the implementation against every native row.
The negative runner rejects six controlled corruptions for their intended
reasons, including rehashed semantic/input rows, forged PE architecture,
compiled-source changes and optimized Python. Negative copies remain in the
task cache; only rejection results/logs are retained here.

| ID | Dependent result | Stress/falsification probe | Status |
|---|---|---|---|
| A1 | Private NT layout and ordering | Different installed build or guard matrix differs | Confirmed on measured build |
| A2 | Immutable selected namespace | Reached uncaptured state affects lookup | Confirmed for selected snapshot/leaf; wider state unsupported |
| A3 | Captured host principal/grants | Reach guest impersonation or changed authorization policy | Retained; token-changing guest services unsupported |
| A4 | Fresh compatibility startup | Fresh x64/WoW64 entry/return differs | Retained; fresh compatibility unknown |
| A5 | Selected NT case policy | Original/inverted/explicit-fold probes disagree | Confirmed on measured host; acquired dynamically |
| A6 | Loaded WoW64 prefix mechanics | Another build/target or matching prefix differs | Confirmed for measured negative prefixes; wider redirection unsupported |

`assumption-register.json` includes basis and concrete probes;
`change-surface-map.json` covers the affected planes and all three platforms.
High remaining work is the next reached native service, uncaptured namespace
state, matching WoW64 redirection, selected security/QoS contexts and complete
ordinary guest exit. Medium limits are fresh compatibility/physical x64,
pre-existing Windows full-suite failures, concurrent root coverage and complete
package/live IDA/Qt runtime proof. No unrelated system is changed by this group.

QG1 needs no normative conclusion. QG2 maintains assumptions and stress probes.
QG3 maps this increment to six acceptance criteria while full-process acceptance
stays open. QG4 uses exact byte/UTF-16 bounds and reproducible partitions/scheduler
arithmetic. QG5 exposes unsupported edges and the loaded/raw-image discrepancy.
QG6 binds primary declarations, native identities and current owning artifacts.
QG7 records bounded impact and exclusions. Positive verification and all six
controlled negative rejections passed before exact-file publication through red.
