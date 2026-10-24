# Selected initial desktop and thread query

`NtUserGetThreadDesktop` uses a guest-owned desktop object and a borrowed
process handle. The selected native GUI helper supplies only immutable UTF-16
name, granted-access and handle-flag metadata. Its borrowed native handle is
neither closed nor imported into guest state. Acquisition checks that the
selected handle and metadata agree before and after copying the GUI views.
The native object-information buffer is exactly 56 bytes. An earlier 128-byte
control returned `0xC0000004`; its untouched output was excluded from metadata
claims.

The process allocates the guest desktop handle before guest execution. Queries
reuse that handle. Desktop attachment prevents closing the assigned handle
independently of `HANDLE_FLAG_PROTECT_FROM_CLOSE`; duplicates refer to the same
object and have independent lifetimes. The current model has one initial
desktop. It does not implement arbitrary desktop creation or reassignment.

| Raw or wrapper operation | Recorded native result |
| --- | --- |
| Current thread and its low-two-bit CID aliases | Same borrowed desktop handle |
| Live worker and retained ended worker | Same desktop |
| Recorded invalid IDs and process ID | Zero; LastError and LastNtStatus unchanged |
| Assigned `CloseDesktop` | False; error `0x000000AA` (`ERROR_BUSY`) |
| Assigned `NtClose` | `0xC0000235` (`STATUS_HANDLE_NOT_CLOSABLE`); flags remain zero |
| Duplicate with `DUPLICATE_SAME_ACCESS` | Distinct handle to the same desktop; duplicate can be closed |
| Duplicate with `DUPLICATE_CLOSE_SOURCE` | Success with a new handle; assigned original remains valid |

The v3 cohort contains 261 query rows per executable profile, or
4 * 261 = 1,044 rows. It covers ARM64, x64, x86 and large-address-aware x86
executables on one Windows ARM64 10.0.29683 kernel. Native identity is SYSTEM.
The captured desktop is `Default`, with granted access `0x000F00CF` and flags
zero. Production acquisition records the selected metadata rather than fixing
those values in shared policy. Independent native x64-kernel observations are
unknown.

An ended thread remains queryable while its thread object is retained. Closing
the last observed user handle does not prove immediate native CID retirement:
one earlier x86 control still returned a desktop afterward. Deferred object
references and TID reuse are unresolved explanations. The guest follows its
explicit retained thread-object lifetime; it does not claim native retirement
timing.

The v4 cohort adds eight worker self-queries per profile. Native ARM64 and x64
workers change the conversion marker from absent to present. The x86 TEB32
marker stays absent. That observation does not establish the corresponding
native64 WoW64 context. The existing matching native 64-bit GUI input contract
remains explicit; paired WoW64 conversion is incomplete.

| Cold ARM64 control | Callback status | Query return | Conversion marker after | Native probe exit |
| --- | --- | --- | --- | --- |
| Actual USER32 callback | `0x00000000` | Nonzero borrowed handle | Present | 0 |
| Controlled success | `0x00000000` | Nonzero borrowed handle | Present | 0 |
| Controlled failure | `0xC0000001` | Zero | Absent | 5 |
| Controlled success, output pointer 1 and length `UINT32_MAX` | `0x00000000` | Nonzero borrowed handle | Present | 0 |
| Controlled warning, same invalid output | `0x80000001` | Zero | Absent | 5 |

Each cold control starts without the conversion marker, invokes callback 84
with zero input bytes and preserves LastError `0x12345678`. Exit 5 belongs to
the deliberately failed native observer process; it is distinct from C API
reason 5, `RAX_PROCESS_FAILED`. The measured continuation does not dereference
the callback output buffer. The guest saves a per-service successful value and
the interrupted register context through `NtCallbackReturn`.

CID lookup costs O(N) time for N retained guest objects and O(1) additional
space. Assigned-handle lookup and attachment checks use the existing object and
handle maps. No guest CID, handle, pointer or path enters a native host query.

`observations.json` publishes scalar and relative projections. Source and
receipt identities bind the private original logs. Raw handle values, native
thread IDs, stacks, TEB, desktop and GDI memory remain private. Projection
controls accept three unchanged datasets and reject 36 altered cases across
normal, `-O` and `-OO` execution. These are integrity controls; they do not add
an independent kernel profile or establish ordinary guest completion.

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
| --- | --- | --- | --- | --- | --- | --- |
| TD-A1 | Helper metadata describes the selected initial desktop | Native object queries and repeated capture | Guest-owned descriptor and initial handle | Name/access/flag changes; stale descriptor identity | Different before/after capture or ordinary DLL rejection | Retained for selected context |
| TD-A2 | CID low bits alias and ended retained objects remain queryable | Native v3 controls | Guest CID lookup | Invalid IDs, retained/released workers, TID reuse | Differing aliases or loss while object retained | Confirmed for recorded cohort; immediate retirement unknown |
| TD-A3 | Cold query requires conversion and a distinct successful handle result | Native cold controls | Shared callback continuation | Success, failure, warning, invalid output, nested calls | Wrong restored value/context or error state | Confirmed for recorded ARM64 controls |

| Impact | Remaining boundary | Effect |
| --- | --- | --- |
| High | Whole USER/GDI process behavior and ordinary callback completion | This selected query alone does not complete the process objective |
| High | Paired WoW64 GUI state and desktop creation/reassignment | Outside the implemented initial native64 desktop contract |
| Medium | Independent x64 kernel and exact native CID retirement timing | Limits portability and lifecycle claims |
