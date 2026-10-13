# Native symbolic-link objects and selected KnownDllPath queries

This increment implements NtOpenSymbolicLinkObject, NtQuerySymbolicLinkObject
and guest handle lifetimes for the immutable selected KnownDllPath links.
Full same-host ordinary process execution remains an active incomplete goal.
The unchanged ordinary observer advances 91,011->97,257:
97,257-91,011=6,246 scheduler calls. All four ordinary programs reach
NtQueryVirtualMemory class 14, service 0x23 at instruction PC 0x180001240
(stopped-thread PC 0x180001244), before guest exit.

Fixed Windows selection opens the captured KnownDllPath names with
MAXIMUM_ALLOWED, records the actual granted mask and independently observes
ACCESS_SYSTEM_SECURITY status. Target capture is bounded and the grant is
rechecked; every native handle closes before selection returns. Guest names,
pointers, handle values and requested access never cross into host NT calls.
The selected target is an immutable snapshot, including if the host target
changes afterward. Other captured link names without selected target/access
metadata remain explicitly unsupported. Default closed personalities have no
installed namespace. Native NT counted UTF-16 paths keep NT grammar and are
separate from native filesystem paths.

| Contract | Native64 | WoW64 x86 |
|---|---|---|
| OBJECT_ATTRIBUTES extent | 48 B | 24 B |
| UNICODE_STRING extent | 16 B | 8 B |
| Open output HANDLE extent | 8 B | 4 B |
| Query descriptor alignment | 2 B | byte aligned |
| Initial query descriptor write probe | first 4 B | none; capture first |
| Query kernel descriptor | caller descriptor | private promoted descriptor |
| Final query descriptor copy | successful Length only, 2 B | entire captured descriptor,8 B |
| Duplicate output fault | before operation | after operation |

Open shares the measured directory attributes/name converter. Native64 probes
its output before capturing aligned attributes. WoW64 captures attributes,
name, flags/security/QoS tails and measured system-root prefixes before a
private output slot and caller copy. Reached faults, guards and aliases retain
the measured order. Empty counted names can reopen a typed SymbolicLink root;
nonempty traversal through that root is a type mismatch. A final symbolic
link accepts OBJ_DONT_REPARSE; intermediate links retain reparse rejection or
explicit unsupported traversal. Missing name, missing intermediate path,
directory/type mismatch and unknown captured target are distinct outcomes.

Link generic read/execute map to 0x20001, write to 0x20000 and all to 0xF0001.
MAXIMUM_ALLOWED expands from the selected maximum grant on open. Zero open
access denies; duplicate zero access is admitted. Duplicate SYNCHRONIZE and
MAXIMUM_ALLOWED are discarded. Requested query rights must be present in the
selected ceiling; system-security status follows the ordinary rights check.
Duplicate SAME_ACCESS/SAME_ATTRIBUTES, inheritance/protection, close-source,
tagged handles, lowest-free reuse and unreturned protected WoW64 copy-fault
handles match the kernel corpus. Unknown high access bits deny before source
close. Unknown low link-specific bits deny after eligible source close; an
independently captured directory control establishes that directory duplicates
filter those low bits instead. Other resource adapters retain their policies.

Query probes the entire declared target capacity, then optional ReturnLength,
before handle/type/query-right validation. Capacity0 touches no target bytes.
The measured buffers and ReturnLength accept byte alignment. Native64 first
requires2 B descriptor alignment and probes its first 4 B for writing, then
captures its fields. WoW64 captures all 8 B without that initial write probe,
uses a private native descriptor, and copies the complete original descriptor
back after the kernel phase. On success only the copied Length changes;
MaximumLength and Buffer are restored even when ReturnLength aliased them.
Kernel faults resolve before the final caller copy. A failed caller copy can
override the kernel status after target or ReturnLength effects, while reached
kernel guard consumption survives that later failure. A cross-page denied
descriptor copy is atomic.

For T captured UTF-16 target units, length=2T B. With non-NULL ReturnLength,
required=2T+2 B and success includes a 2 B NUL; with NULL ReturnLength,
required=2T B and no terminator is written even with extra capacity. A short
buffer leaves target/descriptor unchanged in the kernel phase, and writes
required if ReturnLength is present. Successful target bytes precede native
Length and ReturnLength stores; WoW64 then copies its descriptor. Overlapping
outputs, pointer faults and one-shot guard consumption reproduce native bytes,
status and page state. No guest SEH path is fabricated for these syscall faults.

Guest ownership:

```text
Arc<SymbolicLink>
+-- selected catalog reference
+-- object table[id] reference
    +-- handle A (open)
    +-- handle B (duplicate)
    +-- handle C (empty-name root reopen)
```

Catalog drop preserves live handle metadata; final alias close and process
object-table drain release the resource. No native handle or host callback is
retained. Query does not change the captured target or namespace.

The selected namespace retains its 8,192-entry,16 MiB raw UTF-16 and 32,766-unit
per-name/target bounds. Target length<=65,532 B; optional terminated extent
<=65,534 B, fitting the unsigned16-bit MaximumLength. Additional immutable Arc
metadata/folded keys add storage beyond that raw payload bound. For E entries
and U captured UTF-16 units, construction is O(U log E + E) time and O(U + E)
retained space, plus the fixed 65,536-entry upcase table. Lookup over L units is
O(L log E) time and O(L) temporary space. Query is O(T) time and O(T) output
scratch; handle birth has expected O(C + (H + 1) log(H + 1)) time for object-table
capacity C and H handles. Its scan uses O(1) auxiliary storage; table growth
is retained allocation.

| Native family | Cases/profile | Profiles | Exact replay rows |
|---|---:|---:|---:|
| Open names/rights/lengths/flags/roots/aliases | 1189 | 4 | 4756 |
| Open pointer/guard/alignment/page boundaries | 5000 | 4 | 20000 |
| Flag/length/pointer priority | 420 | 4 | 1680 |
| UTF-16, security, QoS and reparse | 858 | 4 | 3432 |
| Name extents, prefix probes, roots and NULL output | 9152 | 4 | 36608 |
| Typed relative roots, grants and tags | 720 | 4 | 2880 |
| Duplicate rights/options/protection | 2400 | 4 | 9600 |
| Duplicate output faults | 112 | 4 | 448 |
| Protected duplicate copy faults | 168 | 4 | 672 |
| Extended link rights and close-source priority | 896 | 4 | 3584 |
| Independent directory low-rights control | 896 | 4 | 3584 |
| Query lengths/capacity/pointers/guards/aliases | 3798 | 4 | 15192 |
| Query descriptor alignment | 225 | 4 | 900 |
| Query descriptor copy boundaries/atomicity | 768 | 4 | 3072 |
| Query kernel/caller dual faults and guard aliases | 6384 | 4 | 25536 |
| Total | 32986 | 4 | 131944 |

Fifteen mature families over ARM64, x64 compatibility, x86 and x86 LAA,
plus one fresh ARM64 producer, bind 61 actual compiled source/PE/stdout
identities. Actual PE machine/LAA flags and installed System32/SysWOW64 NTDLL
identities are recorded. Every completed mature row restores its owned native
handles and has no caller exception. The native principal is NT AUTHORITY\SYSTEM,
SID S-1-5-18; its captured subject grants do not assign that identity to the guest.

Two complete fresh ARM64 original-entry/true-LR captures observe installed
MEM_IMAGE NTDLL and decode actual SVC service 312 (0x138), rather than assuming
it from an API label. The call uses48 B attributes, flags0x40, desired access1
and relative KnownDllPath below a directory handle. Both owned debugger children
terminate77 after capture and restore the exact native handle set; this is
separate from guest exit. One debugger warmup is excluded from semantic proof.
Fresh compatibility startup, physical x64-kernel proof and behavioral equivalence
on other kernel/native DLL builds remain unknown. Aborted producer diagnostics
stay in the task cache and are excluded from the complete native corpus.

Primary declarations:
[Microsoft NtOpenSymbolicLinkObject](https://learn.microsoft.com/en-us/windows/win32/devnotes/ntopensymboliclinkobject),
[Microsoft NtQuerySymbolicLinkObject](https://learn.microsoft.com/en-us/windows/win32/devnotes/ntquerysymboliclinkobject).
Unchanged HTML copies with retrieval/hash metadata are in references/.
The open page's three-argument syntax names LinkHandle, while its parameter
prose refers to directories; primary-source-conflict.json records the conflict.
Syntax establishes declaration shape. Private validation/order, optional NUL
behavior and loaded WoW64 conversion come from native capture and exact replay.

| Host | Focused passed | Full passed/failed/ignored | C API | Integration | C++ consumers |
|---|---:|---|---:|---:|---:|
| macOS | 17 | 7730/0/2 | 168 | 544 | 5 |
| Linux | 17 | 7724/0/2, unfiltered serial | 168 | 544 | 5 |
| Windows ARM64 VM | 18 | 7139/5/2 | 168 | 4 | 5 |

The Windows five failures match the exact four BZHI/one FP16 baseline names
in untouched source. The earlier Linux parallel run reports 7720/3/2: two timer
assertions and one EINTR readiness failure. All three pass individually; the
prior serial 7723/0/2 and final serial 7724/0/2 runs are retained. The complete
parallel failure cause is unknown. No test assertion or selection is weakened.
The two ignored microkernel checks remain distinguished from executed tests.
Mac owning CLI/MCP, seven selected CTests and both protected-text scans pass.

All 131,944 observations replay in the17 portable/18 Windows focused tests.
The installed Windows leaf resolves actual ARM64/x86 NTDLL and verifies RET 12
with 16 B caller-stack cleanup. All-target builds use empty core features;
C API tests run serially. Unix user_windows_memory binaries contain0 tests;
Windows's four memory integration tests are distinct native execution evidence.

Owning builds use the frozen Unix e2fc5b48 and Windows f386ed2a root embeddings
with current RAX sources. All nine process adapter/factory/header/consumer files
are byte-identical to the captured current root; unrelated concurrent root
deltas remain excluded from those builds. owning-root-scope.json records the
boundary. This does not validate concurrent Cluster/MCP/public-launcher work.
Public C API 1.11.0, opaque Assist Rust ABI, layouts, feature defaults,
dependency pins and package layouts remain unchanged. Public Rust Object gains
SymbolicLink. Full release/package/plugin/GUI validation is not established by
these selected owning archive and consumer gates.

The native shipping archive is 1132995580 B, SHA-256
`8b753dccd7de361f399c65613d630f00765145be93e71b86f4b8d7e45b1988ef`, with 4,673 members.
The actual Cargo-selected empty-feature core is 745318104 B,
SHA-256 `62f8682d1baf626990ca0b592401b4b7aa36a5fccf71b79353f7ca33ab641a39`,
with 260 members. Complete member walks,86 actual Rust source identities and
two actual helper identities pass. The unchanged observer is 11,177 B, SHA-256
`d41c953666769f60c0a4a53b098bd85f8b778133d6839be4c8ea6ca221c1734e`.

Run `python3 docs/specifications/windows/native-symbolic-link-open/check-evidence.py`
and `python3 docs/specifications/windows/native-symbolic-link-open/check-negative-evidence.py`
from the RAX root. The checker independently enumerates the15 family inputs,
requires the61 producer/profile inventory, validates actual source/stdout
bindings and PE ABI flags, compares same-width statuses/grants/guard effects,
checks native owned-handle restoration and validates exact evidence file hashes.
Eleven controlled source/PE/LAA/inventory/DLL/input/row/status/fresh-lifetime
falsifications are rejected even when changed native rows are rehashed.
Assumptions and falsification probes are in assumption-register.json; impact
and blocking boundaries are in bounded-scope.json. Positive and all eleven controlled negative verifications pass before exact-file
publication through red.
