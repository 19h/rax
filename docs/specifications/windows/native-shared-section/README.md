# Selected installed Windows data section

The Windows native-library profile captures `\Windows\SharedSection` as a
bounded immutable input. Guest `NtOpenSection` handles retain typed data-section
identity and captured grants. `NtMapViewOfSection` copies committed bytes into
guest frames, preserves reserved runs, and owns the view independently of handles.
Data bytes remain opaque, including server-address words. Native CSR code owns
its server/client address-delta interpretation.

The acquisition opens a fixed host object, requests a read-only current-process
view with zero new commitment, reads bounded committed regions, compares two
complete captures, and closes every host handle/view before guest use. At most
three capture pairs are attempted. Acquisition rejects inconsistent geometry,
changed grants, unsupported protections, or unstable bytes. Guest pointers,
requested names and requested rights never enter these host calls.

Selection bounds are 64 MiB virtual extent, 16 MiB committed snapshot storage,
and 16,384 page-aligned region runs. Regions cover the exact extent without gaps
or overlaps. Validation is O(R) for R runs; acquisition is O(B + R) for B copied
bytes. Mapping costs O(B + R log P) for P guest page-state boundaries, with no
frames for reserved runs. Copy-on-write frames remain private to each view.

## Native controls

The actual host is Windows 11 ARM64 kernel 10.0.29683.1000 under the captured
SYSTEM subject. x64 and x86 producers execute through this ARM64 Windows host;
physical x64-kernel behavior is unknown. The independent post-run inspector
binds actual producer source, PE machine/LAA, executable bytes/hash, raw stdout,
unchanged installed DLL identities, and resource-release outcomes.

| Direct control partition | Cases/profile | Profiles | Owned mapped views |
| --- | ---: | ---: | ---: |
| Open and granted access | 32 | 4 | 16 |
| View geometry and map protection | 40 | 4 | 80 |
| Later protection changes | 22 | 4 | 88 |
| Every allocation-flag bit and combinations | 39 | 4 | 24 |
| Total | 133 | 4 | 208 |

The 532 direct controls close 36 section handles and unmap all 208 views. A
separate original-entry/true-return capture preserves all native arguments and
status; its native child exits with code 41. That capture observes the startup
allocation flags `0x00500000` after truncating the ABI's ULONG from its raw
64-bit argument slot. All 17 actual producers and their bindings are checked by
`python3 docs/specifications/windows/native-shared-section/check.py`.

Observed access and mapping behavior:

| Input/property | Observed result |
| --- | --- |
| `SECTION_MAP_READ` (`4`) | Granted |
| `READ_CONTROL` (`0x20000`) | Granted |
| `MAXIMUM_ALLOWED` (`0x02000000`) | Grant `0x20004` |
| Query/read combined (`5`), write or execute | `STATUS_ACCESS_DENIED` |
| `ACCESS_SYSTEM_SECURITY` | `STATUS_PRIVILEGE_NOT_HELD` for this subject |
| Generic access requests | Denied for this subject |
| Full default view | 1,048,576 bytes, `MEM_MAPPED` |
| Committed prefix | 20,480 bytes, `PAGE_READONLY` |
| Reserved tail | 1,028,096 bytes, protection zero |
| Requested nonzero size | Rounded upward to 4,096 bytes |
| Nonzero section offset | 65,536-byte alignment required |
| Accepted allocation-bit mask | `0x00500001`; `0x00100000` chooses top placement |
| Read-only, no-access or copy-on-write map | Accepted with read-map rights |
| Later read-only/copy-on-write/no-access/guard change | Accepted |
| Later shared-write/execute upgrade | `STATUS_SECTION_PROTECTION`, old output `PAGE_NOACCESS` |
| Later cache modifiers | `STATUS_INVALID_PARAMETER_4`, old output `PAGE_NOACCESS` |

The VM preserves committed/reserved boundaries, allocation protection, opaque
bytes, view lifetime after handle close, partial reserved views, private writes,
and protection limits. The original NT service transport retains its measured
native/WoW64 pointer capture and late-copy ordering. Portable tests replay all
532 controls. Installed ARM64 and x86 export-leaf tests separately exercise open,
map, protect, close and unmap, including the actual startup allocation flags.

## Scope and assumptions

| ID | Assumption/basis | Stress test or falsification probe | Status |
| --- | --- | --- | --- |
| S1 | Startup selects `\Windows\SharedSection`; actual guest name and original native capture agree | Repeat ordinary observer/name capture on another program/session | Confirmed on the recorded host |
| S2 | Host acquisition uses owned read-only resources | Native status, protections, unmap/close counts and changed grants | Confirmed |
| S3 | Captured bytes remain opaque; the native client computes address deltas | Original post-map code and alternate native/client bases | Revised from an initial relocation hypothesis |
| S4 | A bounded coherent snapshot can be acquired | Three pairs of full region/byte captures; changed data refuses selection | Confirmed by current installed-leaf execution |
| S5 | The recorded allocation-bit policy applies to the selected data object | All 32 one-hot bits, startup combination and invalid combinations across four profiles | Confirmed within the recorded kernel/profile |

High-impact remaining frontiers: guest requests that would commit new pages
beyond the captured snapshot remain explicit unsupported operations; section
query classes and CSR server/ALPC protocol completion are not implemented by this
change. A native-section snapshot does not establish full Windows process support.

The [unchanged 11,178-byte, 3,000,000-turn observer](ordinary-observer.rs.txt) passes the previous section
open at turn 2,114,258 and stops at `NtCreateSection` service `0x4A`, PC
`0x1800014E0`, at turn 2,115,129: 2,115,129 - 2,114,258 = 871 scheduler calls.
Three ordinary CRT programs reach that same service; installed `whoami.exe`
retains its `NtTraceEvent` frontier. All four report reason 5/internal failure.
Their default exit-code field is not evidence of a guest exit. The original
native capture's exit 41 is host-control evidence. Full ordinary Windows guest
completion remains unproved.

Source, host-kernel controls, portable replay, installed-leaf execution, scoped
Assist embeddings and ordinary process outcomes are separate evidence layers.
The frozen Unix/Windows owning roots are e2fc5b48/f386ed2; nine current C++
consumers are compared by hash. Whole current-root plugin/GUI/package validation
is outside these scoped embedding results. Exact commands, source revisions,
archives, baseline failures and later upstream integration are recorded in the
validation artifacts accompanying this corpus.

General API contracts: [Microsoft section/view lifetime](https://learn.microsoft.com/en-us/windows-hardware/drivers/kernel/managing-memory-sections)
and [Nt/ZwMapViewOfSection](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/wdm/nf-wdm-zwmapviewofsection).
Undocumented build-specific statuses, grants and flags above derive from the
included native controls rather than generic API documentation.

## Recorded validation on the feature source

These results use base `7951b3b29` plus the recorded feature source. Subsequent
upstream integration is tracked separately; these counts do not describe an
unbuilt later source revision.

| Host | Library passed/failed/ignored | C API passed | All targets |
| --- | --- | ---: | --- |
| macOS ARM64 | 7,797 / 0 / 2 | 168 | check passed |
| Linux amd64 container | 7,791 / 0 / 2 | 168 | check passed |
| Windows ARM64 | 7,216 / 5 / 2 | 168 | check passed |

Windows retains four BZHI carry-lowering and one FP16 lowering baseline failures.
New snapshot tests pass, including 532 portable native-control replays and the
actual ARM64/x86 export leaves. Unix Windows-model integration targets pass 544
checks; Windows selects zero in that target and runs four memory-target checks.
The scoped macOS embedding passes seven CTests and both protected-text scans;
Linux and Windows pass five rebuilt C++ consumers each.

The complete Linux integration target passes 42 tests with one ignored when
serialized on either Unix host. Initial parallel runs fail in different
`uringio` cases while using fixed `/tmp/uring-*` names; original failures are
retained. The macOS Darwin target passes 116 tests and fails `reclaim_arm64`:
both control programs report pressure level 2, but native sampling is about
1 s and the model reports about 10 s. This independent mismatch remains a full
goal requirement. Linux's Darwin target passes 117 checks but does not provide
native Darwin kernel evidence. See [validation records](validation-summary.json),
[assumptions](assumption-register.json) and [quality gates](quality-gates.json).
