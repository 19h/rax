# Native process-token opening

`NtOpenProcessToken` opens a guest-owned handle to one process-owned primary
token. Repeated opens and duplicates share its object identity while retaining
independent granted access. Closing the last caller handle does not destroy
the primary token; process teardown releases it. No guest pointer or token
request is forwarded to a host kernel.

The native startup adapter captures the current process token's maximum grant
and the result of an `ACCESS_SYSTEM_SECURITY` open. It closes both private host
handles and retains only validated access metadata. The existing subject and
the access profile must match across native WNF state acquisition. A missing
profile or subject causes an explicit unsupported result, without creating an
object. Token mutation, impersonation and other-process token acquisition are
outside this contract.

| Input or access | Behavior |
| --- | --- |
| Exact architecture-width current-process pseudohandle | Uses the selected primary token |
| Real current-process handle | Requires either query or query-limited rights (`0x1400` combined) |
| Current-thread pseudohandle or other object type | `STATUS_OBJECT_TYPE_MISMATCH` |
| Invalid process handle | `STATUS_INVALID_HANDLE` |
| `GENERIC_READ` | Maps to `0x0002001a` |
| `GENERIC_WRITE` | Maps to `0x000201e0` |
| `GENERIC_EXECUTE` | Maps to `0x00020005` |
| `GENERIC_ALL` | Maps to `0x000f01ff` |
| `MAXIMUM_ALLOWED` | Uses the captured maximum grant |
| Zero requested rights | `STATUS_ACCESS_DENIED` |

The measured generic mappings differ from the `TOKEN_READ` and `TOKEN_WRITE`
convenience macros. Illegal bits are rejected before the captured security
privilege status; legal but ungranted rights are rejected afterward. Access
calculation uses `O(1)` time and storage. The existing handle allocator scans
`O(H)` entries for `H` live handles; object-map access adds `O(log N)` for `N`
live objects. The token object uses constant additional storage and each caller
handle uses one table entry. Modeled allocation failure returns
`STATUS_INSUFFICIENT_RESOURCES`; native resource-exhaustion status and quotas
were not measured by these controls.

Native 64-bit failure leaves the caller output unchanged. WoW64 failure copies
a zero handle to a non-NULL output; a failing copy supersedes the original
status. A NULL WoW64 output skips that failure copy. Successful calls probe the
entire architecture-width output before publishing a handle. Caller-byte,
page-guard and no-birth results follow the retained native boundary records.
The selected x86 NTDLL service leaf uses `RET 12`.

The [pinned primary sources](primary/sources.json) describe the interface and
access masks. The DDI page names `PROCESS_QUERY_INFORMATION`; the user API page
and current native controls also admit `PROCESS_QUERY_LIMITED_INFORMATION`.
The measured current behavior owns this implementation. Pointer fault order,
generic mappings, handle aliases and lifetime come from native observations,
not from an inference that the API names specify those details.

| Native control family | Operations across four caller profiles |
| --- | ---: |
| Initial process/access/output combinations | 4096 |
| Output boundaries and ten valid protection modes | 8640 |
| Access-mask combinations and grant precedence | 52860 |
| Separate lifetime, duplicate and alias operations | 106 |
| Total | 65702 |

All sixteen producer identities bind source, post-run PE machine/LAA/hash,
exit status and raw output on one ARM64 Windows 10.0.29683.1000 system. x64 is a
compatibility caller, not a physical x64-kernel measurement. Rust regressions
replay the first 65,596 operations, comparing status, grant, caller bytes,
guards, handle creation and last error. Only successful opaque handle bytes
are normalized. The 106 lifetime operations are separate native controls,
corroborated by representative object/duplicate/close tests; they are not an
exact row replay.

The first boundary probe used an invalid `PAGE_NOACCESS | PAGE_GUARD`
combination. Its first two versions also had a `printf` argument mismatch.
Those outputs are excluded; boundary-v3 uses valid protection combinations
and compiler format warnings as errors. `check-native.py` verifies admitted
source, artifact and transcript identities. `stress-native.py` rejects eight
altered proof sets even after the attacker rebinds their digests.

## Final validation and application boundary

| Host | Full library passed/failed/ignored | Token focus | C API | Windows integration |
| --- | --- | --- | --- | --- |
| macOS ARM64 | 7903/0/2 | 12 | 168 | 544; native-memory adapter excluded |
| Linux x86-64 container | 7897/0/2 | 12 | 168 | 544; native-memory adapter excluded |
| Windows ARM64 | 7337/5/2 | 13 | 168 | 4 native-memory cases; Unix corpus excluded |

All-target checks pass on all three hosts. The five Windows failures are the
existing four BZHI and one FP16 lowerer cases; the two existing ignored
microkernel cases remain visible. The Windows focus includes real installed
native-host and x86 NTDLL leaves. The first Windows preparation attempt stopped
before Cargo because the copied source lacked 71 older diagnostic files. The
retry verified all 5677 selected source/fixture inputs. No Rust source changed
after the focused suite.

Scoped Assist checks pass seven CTests and two executable scans on macOS and
five C++ consumers each on Linux and Windows. Each host has complete walks of
two actual Cargo-selected core/carrier archives, with empty core features and
401 selected source/fixture inputs. The retained Assist Rust carrier is ABI
minor 184. The compared root at test preparation was minor 196; the later
upstream parent `a5d764b5235dc634f5dcbc2d92e5f33b839752e8` is minor 199. All nine selected
C++ consumer files match both roots. The upstream Rust changes and whole
current-root GUI/plugin/package build are outside this recorded validation.

The unchanged production observer produced these exact ordinary outcomes:

- `terminal turn=2149788 Complete(Internal("native NTDLL service NtQueryInformationToken: Unimplemented(\"NtQueryInformationToken: NtQueryInformationToken class 12\")"))`
- `terminal turn=2152137 Complete(Internal("native NTDLL service NtQueryInformationToken: Unimplemented(\"NtQueryInformationToken: NtQueryInformationToken class 12\")"))`

A controller exit of zero does not establish a guest exit. The absent third
helper is excluded from the execution count. Full process emulation remains
incomplete. `check-validation.py` binds these outcomes and rejects invented
success, changed failures, removed ignores, source drift, invented integration
coverage and default-feature drift after evidence digest rebinding.
