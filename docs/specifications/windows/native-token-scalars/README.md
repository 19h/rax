# Captured native scalar token-query contracts

The guest `NtQueryInformationToken` handler now uses captured primary-token
scalar values and native invalid-class capabilities. Width, length publication,
WoW64 rejection and token-rights ordering remain separate measured contracts.
No guest query calls a host token API. Native startup queries its private token
handle, checks exact returned sizes and retains the complete result map in its
repeated immutable snapshot comparison.

Known DWORD values are preserved without an invented flag or enumeration mask.
The byte-valued class21 retains its u8 width. Classes45/46 have no invented
success layout. Missing capture and unmodeled impersonation properties refuse
explicitly; existing class12/class29 and independent PEB process-session behavior
remain intact. Native output storage uses u32 to guarantee four-byte alignment,
including when only one byte is returned.

The scalar map is bounded to 15 selectors; lookup is O(log C) for C <= 15. A query
performs O(P) page probes for P visited pages with constant auxiliary storage;
large caller lengths do not allocate proportional output storage.

The final native corpus contains 15 selectors, four caller profiles and 1830
rows per selector/profile: `15 * 4 * 1830 = 109800` matrix queries. Each matrix
also has an independent reference query, for 60 references. Four binaries each
execute 15 matrices. ARM64, x64 compatibility, x86 and x86-LAA all run on one
ARM64 Windows 10.0.29683 kernel. This does not measure a physical x64 kernel.

The 1830-row axes are `10 * (11 + 43 * 4)`: ten handle roles, eleven baseline
lengths, and 43 pointer/protection roles at four lengths. Raw transcripts,
post-run source/PE identities and six deduplicated row groups are retained.
Deduplication only shares byte-identical measured rows; each selector/profile
still binds its complete raw transcript.

| Class | Native selector | Successful value shape | Distinct behavior |
| --- | --- | --- | --- |
| 15 | TokenSandBoxInert | DWORD | Minimum length 4 |
| 18 | TokenElevationType | DWORD enumeration | Minimum length 4 |
| 20 | TokenElevation | Four-byte structure | Exact length 4; required length precedes handle validation |
| 21 | TokenHasRestrictions | One-byte Boolean | Minimum length 1; caller alignment remains 4 |
| 23 | TokenVirtualizationAllowed | DWORD Boolean | Minimum length 4 |
| 24 | TokenVirtualizationEnabled | DWORD Boolean | Minimum length 4 |
| 26 | TokenUIAccess | DWORD Boolean | Minimum length 4 |
| 27 | TokenMandatoryPolicy | DWORD flags | Minimum length 4 |
| 32 | TokenAppContainerNumber | DWORD | Minimum length 4 |
| 40 | TokenIsRestricted | DWORD Boolean on native64 | WoW64 rejects before caller probes |
| 42 | TokenPrivateNameSpace | DWORD Boolean | Minimum length 4 |
| 45 | TokenChildProcessFlags | Query unavailable here | Native64 rejects after probes; WoW64 rejects before probes |
| 46 | TokenIsLessPrivilegedAppContainer | Query unavailable here | Rejects after probes on every measured profile |
| 47 | TokenIsSandboxed | DWORD Boolean | Minimum length 4 |
| 48 | TokenIsAppSilo | DWORD Boolean | Minimum length 4 |

The observed dispatch order is:

```text
WoW64 class40/class45 ----------------------> invalid class
Other queries -> caller probes
              -> captured invalid class ---> invalid class
              -> class20: publish4 / require exact4
              -> token kind, rights and captured value
              -> other classes: publish size / reject short buffer
              -> write one byte or four bytes
```

Every measured unavailable query returns `STATUS_INVALID_INFO_CLASS`
(`0xC0000003`) at its specified phase. Post-probe rejection takes precedence over
handle/type/access validation but follows alignment, unmapped-memory and guard
failures. Pre-probe WoW64 rejection leaves those caller conditions untouched.

All other successful selectors use the common probe/handle ordering established
by classes 12 and 29. Class 20 differs: it publishes required length 4 after both
probes, even for an invalid token, then rejects any length other than4 with
`STATUS_INFO_LENGTH_MISMATCH` (`0xC0000004`). Class 21 publishes length 1 and writes
one byte after token validation, preserving adjacent bytes. Output/ReturnLength
aliasing, partial mappings, guards and rights remain represented by native rows.

The Microsoft enumeration documents the Win32 `GetTokenInformation` interface.
Its class 21 DWORD description is not a specification of the distinct native
`NtQueryInformationToken` byte layout measured here. PHNT supplies native names
and annotations, while the retained native programs establish this kernel's
widths, caller ordering and failure phases.

A separate set of 24 private-token cases records 72 queries and 24 setters for
class 42. Zero remains zero; tested nonzero DWORD inputs become one, including
high-bit values. Fresh primary-token duplicates are modified under a private
thread token carrying an already-held privilege. The process primary token is
never the setter target; the child thread reverts before completion. These
controls are separate from the 109800-row query replay. Their copied controller
scope text mentions session IDs; its retained compiled source and actual calls
use class 42, the private-namespace flag.

The first survey assumed a four-byte reference. Of 52 selector/profile attempts,
38 produced complete matrices and14 stopped at that reference check. The final
survey records status and width without imposing that assumption, and adds
classes 18 and 27. Both source versions and the incomplete attempts are retained;
they do not increase the final replay count.

`check-native.py` binds the final matrices, references, profiles and private
controls. `stress-native.py` rejects six altered proof sets after rebinding
file/group hashes. An initial checker omitted the primary-token counterpart of
one elevation-length assertion; the adversarial test detected it and the
checker was corrected.


## Validation and application boundary

| Host | Full library passed/failed/ignored | Token focus | C API | Windows integration |
| --- | --- | --- | --- | --- |
| macOS ARM64 | 7909/0/2 | 29 | 168 | 544; native-memory adapter excluded |
| Linux x86-64 container | 7903/0/2 | 29 | 168 | 544; native-memory adapter excluded |
| Windows ARM64 | 7345/5/2 | 34 | 168 | Four native-memory cases; Unix corpus excluded |

Both focused source epochs passed. Source review then replaced byte-array native
buffers with explicitly aligned u32 storage in the capture adapter and independent
native test oracle. The final epoch changes exactly those two source files;
its complete selected core/workspace identity contains 5718 inputs. The portable
replay executes all 109800 matrix rows; native installed-leaf checks use the native
host and x86/x86-LAA profiles and compare capture with an independent host query.
Only successful value bytes adapt to the actual host's scalar values; the measured
error, width, alias and probe contracts remain asserted.

All-target checks pass on all three hosts. The Windows full suite retains the
four existing BZHI and one FP16 lowerer failures; two existing ignored microkernel
tests remain explicit. Scoped Assist checks pass seven CTests plus two executable
scans on macOS and five C++ consumers each on Linux and Windows. Complete member
walks bind two actual Cargo-selected archives per host, empty core features and
409 selected source/fixture inputs.

Frozen Assist Rust carriers remain ABI 184. The compared root used ABI 203; the
preserved upstream parent `4783b80f13c6430f5a873cd1a6959413983b1c39` uses ABI207.
Nine selected C++ source files still match. Its newer Rust code and whole-current-
root GUI/plugin/package builds are outside these retained carrier results.

Exact unchanged-observer outcomes:

- `terminal turn=2149956 Complete(Internal("native NTDLL service NtOpenDirectoryObject: Unimplemented(\"NtOpenDirectoryObject: NT directory outside selected installed namespace\")"))`
- `terminal turn=2152305 Complete(Internal("native NTDLL service NtOpenDirectoryObject: Unimplemented(\"NtOpenDirectoryObject: NT directory outside selected installed namespace\")"))`

Both executed ordinary programs pass token classes 12, 29 and 42, then stop at an uncaptured NtOpenDirectoryObject namespace: smoke at turn 2149956 and whoami at turn 2152305. Neither terminal is a successful guest exit; the broader process-userland goal remains incomplete.

The absent helper remains excluded from execution counts. `check-validation.py`
binds source epochs, actual selected artifacts, full/filtered counts, known
failures/ignores and exact ordinary outcomes. Native and validation adversarial
checks reject altered claims after digest rebinding. Native proof bytes have
explicit `-text` attributes and six representative file types retain identical
Git blobs with `core.autocrlf=true`.
