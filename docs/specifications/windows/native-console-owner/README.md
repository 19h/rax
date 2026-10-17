# Detached stream startup and process-owner information

This engineering corpus records Windows 10.0.29683.1000 observations on an
ARM64 kernel. ARM64 and x64 producers use 8-byte pointers; x86 and x86-LAA
producers use 4-byte pointers through the installed WoW64 substrate. These
profiles do not establish behavior on an x86-64 kernel or a native x86 kernel.

The canonical `native-console-owner.cpp` explicitly calls process-information
class 49 in 136 isolated, detached children per profile. All 544 children exit
with the fixture's code 77. The primary calls contain 130 successes and 414
measured status failures, with no escaping structured exception. Another 136
repeat setters and 1,224 state-helper queries are retained separately. Expected
NT status failures are probe results, not failed producer executions.

`native-console-owner-identities.json` comes from an independent native Python
inspection of the actual compiled sources, PE architecture/LAA flags,
executables, stdout files, snapshots and cleanup. It verifies 16 producers:
four canonical matrices and twelve suspended-parameter probes. Executable bytes
are not stored in this corpus. Reported NTDLL filesystem identities and loaded
leaf bytes are separate evidence; an ARM64X filesystem image is not asserted
to equal its process-specific loaded representation.

Run `python3 docs/specifications/windows/native-console-owner/check.py` from
the RAX root to check the retained identities, primary-source bytes and all
native rows. This portable check does not rerun native producers. Cross-platform
compilation, semantic replay, ordinary startup and owning-product evidence have
separate records under `validation/` when those gates finish.

## Startup contract

The pre-change ordinary ARM64 smoke trace requests
`NtCreateFile("\Device\ConDrv\Server")`. Its object attributes contain 0x42;
the name is 42 bytes of UTF-16LE. This is a console-server request, not evidence
for implementing a general disk-file constructor or returning a fabricated
console handle.

The existing `Console::Host` and `Console::Captured` routes provide streams;
they have no native guest console session. Initial process parameters now
describe a detached process with explicit standard handles:

| Field | x86 | x64/ARM64 |
|---|---|---|
| `ConsoleHandle` | `0xFFFFFFFF` | `0xFFFFFFFFFFFFFFFF` |
| `WindowFlags` | `0x100` (`STARTF_USESTDHANDLES`) | same |
| standard handles | existing guest-owned stream objects | same |

Three suspended-child probe families distinguish plain detached startup
(`WindowFlags = 0`) from detached startup with supplied redirected handles
(`WindowFlags = 0x100`). The raw parameter structures are 708 bytes on x86 and
1,096 bytes on native64. The x86-LAA snapshot controller launches the same
non-LAA x86 smoke child; it does not establish LAA child address geometry.

The controlled traces change private guest parameters only. They demonstrate
that `ConsoleHandle = -1` reaches the subsequent class49 setter with an 8-byte
input containing `1`. They are distinct from an unmodified production startup
run and do not count as successful ordinary guest exits.

## Class49 contract

The pinned PHNT header calls this class `ProcessConsoleHostProcess`, with a
query/set `ULONG_PTR`. Exact status, ordering and tag behavior below come from
the native fixtures, rather than that annotation.

| Operation | Measured ordering and state |
|---|---|
| native64 set | for nonzero length: 8-byte alignment, then upper user-span check; exact length 8; require self pseudo-handle `-1`; read 8 bytes; require `(owner & 3) == 1`; store the complete value |
| WoW64 set | require length >= 4; capture a non-NULL, possibly unaligned 4-byte input before handle validation; NULL passes through; require self pseudo-handle; read NULL only after that check; require tag `01`; ignore excess caller length |
| native64 query | for nonzero length: 4-byte output alignment and upper-span check; probe optional 4-byte `ReturnLength`; require length 8; validate process/type/query rights; probe 8 output bytes; write owner; write return length 8 |
| WoW64 query | return `STATUS_INVALID_INFO_CLASS` before either destination is visited |

The setter rejects even an actual self handle with `PROCESS_ALL_ACCESS`.
Native64 query accepts a self process handle with `PROCESS_QUERY_INFORMATION`
or `PROCESS_QUERY_LIMITED_INFORMATION`; event/thread objects produce a type
mismatch and missing handles produce `STATUS_INVALID_HANDLE`. Queries for an
unmodeled peer remain explicitly unsupported.

The setter accepts full-width values whose low bits are `01`, including values
that are not current process identifiers. No host process lookup, host handle
operation or host kernel request is performed for guest input. Query state is
guest-owned and process-local. Native64 suspended-child probes measure initial
owner `creator PID | 2`. The standalone guest root has no modeled parent, so its
initial value is `0 | 2`; native detached initialization subsequently sets `1`.
This zero-parent choice is an explicit model boundary, not a claim that the
native creator has PID 0.

Guard ordering is observable. A native64 query's `READONLY|GUARD` output fails
write validation without consuming the guard; `READONLY|GUARD ReturnLength`
instead consumes the guard and returns `STATUS_GUARD_PAGE_VIOLATION`. Aliased
output/return-length stores occur in owner-then-length order. Bytewise native
readback retains readable prefixes at failed page crossings; inaccessible
cells are `??` and are not asserted to have a particular byte value.

Service work uses constant-sized transfers: 4 or 8 input bytes, at most 8 output
bytes plus a 4-byte return length. It allocates no guest objects or host buffers
based on supplied length. Complexity is O(1) fixed-size work plus existing
guest handle and virtual-memory map lookups; auxiliary storage is O(1).

## Validation boundaries and sources

`initial-scope.json` records ownership, assumptions, both repository
change-surface maps and impact-labeled findings. The first source epoch and
its failing synthetic WoW64 stack assertion are retained separately. That
transition pops its own 4-byte call return address before the installed
NTDLL's `RET imm16`; installed-leaf tests independently check the complete
20-byte setter and 24-byte query x86 stack changes.

The canonical replay uses four native profiles, including x86-LAA producer
evidence. Its fixture pointers and handles are relocated into guest-owned
storage. Native statuses, readable bytes, guard transitions and native64 owner
values come from raw captured output. WoW64 cannot query this class, so its
setter's private stored value is a model-state assertion rather than an
independent WoW64 query result.

The Microsoft process-creation and `STARTUPINFOW` references, pinned PHNT
headers and license are archived byte-for-byte with checksums in
`primary-sources.json`. They establish creation flags and declarations. The
native oracle establishes this selected build's private implementation.

Full installed Windows userland completion remains an acceptance criterion
of the overarching task. Neither this corpus nor a controller exit code 0
establishes that an ordinary guest reached its entry point or exited.
