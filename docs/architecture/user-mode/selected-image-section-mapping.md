# Selected installed image-section mapping

Implementation and final owning validation completed on 2026-10-14. Full same-host ordinary Windows completion remains
unproved. The unchanged observer advances161784 ->168227 turns (+6443) and
stops at NtQueryPerformanceCounter; all four ordinary runs remain Internal.
Captures and primary sources
belong to [native-section-map](../../specifications/windows/native-section-map/).

The later [upstream integration audit](../../specifications/windows/native-section-map/upstream-integration/README.md)
records combined validation after the independent AArch64 expansion.

## Contract and ownership

NtMapViewOfSection is appended after the existing 41 native exports. Its ten
Stdcall arguments are Section/process handles, base pointer, ZeroBits, commit
size, optional signed 64-bit section offset, view-size pointer, 32-bit inheritance,
allocation flags and page protection. ARM64 transports arguments 0-7 in X0-X7
and arguments 8-9 as DWORD values in 8-byte stack slots. x86 installed leaves
remove 40 argument bytes plus the return address.

Selected ImageSection objects pin the captured native Section handle.
Directory/link handles still close after acquisition. The pinned handle remains
private to the host adapter. Guest handles, process values, addresses, offsets,
rights and allocation flags never enter the host kernel. Each template
acquisition creates and closes an independent host current-process view.

    fixed installed Section -> pinned host identity -> immutable template
                                           |
    guest virtual handle -> shared handler -> guest image allocation
                                           -> private guest pages
                                           -> retained Section owner

The adapter bounds one template to 64 MiB, 256 region runs and a 1 MiB relocation
directory. It captures committed bytes and original page policy, retaining
reserved gaps. Temporary read-protection changes affect only the adapter's own
view and are restored before unmapping. No mapped host instruction executes.
Acquisition validates section size/attributes before and after copying.

The kernel's ARM64X projection is architecture-dependent. The native template
and an AMD64 projection have separate caches. For an AMD64 guest with an ARM64
native template, the adapter dynamically resolves NtMapViewOfSectionEx and
requests MemExtendedParameterImageMachine (6), value 0x8664, using initialized
host-owned buffers. Missing support or acquisition failure is explicit.
Complete projected kernel32 (1,511,424 bytes) and NTDLL (4,665,344 bytes) SHA-256
values match actual x64 raw views. Changing only PE Machine would not establish
correct imports, exports or bytes.

## Shared guest semantics

The shared handler validates pointer probes, process/section identity and access,
inheritance, allocation flags, protection and view geometry. Native/WoW64
argument and copy-back ordering differ. WoW64 can map successfully and then
fail a read-only base/size caller store; that view remains owned. Native 64-bit
callers probe both writable destinations before mapping. Guard visitation is
recorded separately from writable-access failure.
WoW64 also copies captured base/size back after a native-call failure; a caller
store fault can replace a handle/access/geometry status. The measured x64-on-ARM64
capture visits base/size read guards even when invalid PAGE remains the final
status. Native ARM64 does not visit them in that path.

For ordinary pages the returned base is allocation base. Section offset must be
a multiple of 65,536 bytes. View size zero means the remaining section extent;
nonzero size rounds to 4,096 bytes without exceeding that extent. The preferred
candidate is source preferred base plus section offset. Nonpreferred views use
a free guest interval or return an exact geometry error.
Automatic low placement retains a candidate across unmap and reuses it when
available. A stricter ZeroBits ceiling can reject that candidate even with other
free low intervals. Live collisions choose successive aligned free addresses;
unmapping restores reuse of the original candidate. Explicit and free preferred
bases bypass the cached alternative. This is guest state, not a host address
forwarded from a guest mapping request.

MEM_DIFFERENT_IMAGE_BASE_OK (0x00800000) rebases the complete immutable template
before taking the requested slice, including PE ImageBase and supported base
relocation words. Other admitted flags retain source-relative image bytes.
Imports, TLS, module registration and PEB publication remain loader operations.

MEM_LARGE_PAGES is opportunistic in the native controls. Whole images with an
unspecified base can use a 2,097,152-byte-aligned carrier, with a distinct image
address inside it. Explicit placement and nonzero offsets use ordinary image
policy. Guest policy chooses a deterministic carrier for the admitted
whole-image path; native address randomization is not reproduced.

| Input protection | Required Section mapping access |
|---|---|
| PAGE_NOACCESS, PAGE_READONLY, PAGE_WRITECOPY | SECTION_MAP_READ (0x4) |
| PAGE_READWRITE | SECTION_MAP_WRITE (0x2) |
| PAGE_EXECUTE | SECTION_MAP_EXECUTE (0x8) |
| PAGE_EXECUTE_READ, PAGE_EXECUTE_WRITECOPY | 0xC |
| PAGE_EXECUTE_READWRITE | 0xA |

SEC_IMAGE page policy comes from the template, independently of valid caller
protection after the mapping-access check.

| Observed outcome | NTSTATUS | RTL Win32 translation |
|---|---|---|
| Nonpreferred image, source-relative bytes | 0x40000003 | 700 |
| Image rebased by allowed-different-base flag | 0x40000036 | 807 |
| Invalid view extent | 0xC000001F | 5 |
| Allocation/section-offset misalignment | 0xC0000220 | 1132 |

The installed SDK header defines these statuses. Independently executed RTL
controls supply their translations. No physical x64 Windows kernel was measured.

NtUnmapViewOfSection accepts an address inside an owned allocation and releases
its pages and Section reference. Closing a guest Section handle does not release
an existing view. Private release/decommit returns 0xC000001B; private recommit
returns 0xC0000021. Multiple views have independent writable guest frames and
share immutable source identity.

MemoryImageInformation reports logical image base, full Section extent and
partial-view bit. Guest signing stays UNCHECKED. Host signing/CFG/SCP extension
claims are not inherited. Native extension pages outside the Section extent are
excluded from the template and are not claimed as implemented.

## Evidence and limits

Native controls ran on Windows 10.0.29683.1000, native ARM64, SYSTEM subject.
ARM64, x64, x86 and x86 LAA producers share that kernel. Actual source,
executable and log identities are retained with library hashes. The x64/x86
profiles establish compatibility-process behavior on ARM64.

The access matrix contains 914 cases per profile, with 145 additional
low-address cases per profile. Geometry, query, large-page, collision and
original-entry startup controls are separate observations. The 4,236-case
portable replay uses synthetic PE bytes with captured section geometry/machine;
it is not a byte-exact substitute for the installed-template leaf test.
ASLR-dependent scratch/free/upper-bound placements are explicitly normalized.
Captured low-address layouts supply occupied intervals. A prior successful
ZeroBits=1 call supplies the captured randomized candidate independently of a
later tighter-ceiling success/failure expectation. The LAA fixture sets the
executable's COFF bit before process creation.
Failed collectors remain in the task cache and are excluded from completed
native evidence.

Portable regressions cover ownership, independent writes, partial metadata,
full-template rebasing, malformed geometry, mapping access, exact argument
transport and WoW64 late-store preservation. The installed Windows leaf test
executes actual ARM64/WoW64 NTDLL entries and compares template headers.
The native evidence verifier binds 4,632 matrix observations, nine measured
original-entry cases and 6,176,768 bytes of whole-image projection equivalence;
nine controlled corruption/optimization cases are rejected. Final platform counts and same-source owning archive identities are in
[validation-summary.json](../../specifications/windows/native-section-map/validation-summary.json).
The final Rust installed leaf exercises the AMD64 cache separately from ARM64.
Native captures and scoped owning builds do not establish whole-current-root
plugin, GUI, package or arbitrary mixed-machine execution support.

Mapping uses O(B + R log P) time and O(B + R) guest storage for B mapped bytes,
R source region runs and P page-policy runs, plus address-space mapping costs.
Template validation/copy is O(B + R). A relocation-requested partial view
temporarily copies the full template before slicing. The 64 MiB bound applies
per template, not to process-wide aggregate cached bytes.

Material assumptions, probes and statuses are in
[the register](../../specifications/windows/native-section-map/assumption-register.json).
Execution-plane classification is in
[the surface map](../../specifications/windows/native-section-map/change-surface-map.json).

| Impact | Bounded finding | Blocks completion |
|---|---|---|
| High | AMD64 mapping projection is proved; mixed ARM64EC/x64 execution is a separate loader/CPU surface | Yes, for arbitrary x64-on-ARM64 process completion |
| High | Ordinary Windows CRT/Win32 programs have no demonstrated guest exit | Yes, for the overall goal |
| Medium | Native large-page placement/fallback depends on process/kernel state | Exact addresses excluded; semantic outcome needs coverage |
| Medium | Host CFG/SCP extension/trust is not guest execution evidence | Explicit limit outside selected template semantics |
| Medium | Aggregate cached bytes can exceed the per-template limit | No 64 MiB aggregate claim; guest arena still bounds mapped backing |

Primary sources under references/ retain URLs, retrieval dates, hashes and
redistribution notices. Source declarations, native controls and executable
tests own distinct parts of the evidence.
