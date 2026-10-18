# Captured TokenSessionId and process-session identity

`NtQueryInformationToken` class 12 reads the selected primary token's session
identifier as an unsigned 32-bit value. Class 29 retains its AppContainer
behavior. Both classes use the measured caller-probe, handle/access and DWORD
publication order. No host token operation occurs during a guest query.

The native startup adapter reads `TokenSessionId` through its private query
handle, requires exactly four returned bytes, and includes the value in repeated
subject capture. The outer native selection compares subject, access profile
and session value across WNF acquisition. Host handles and pointers do not
enter guest state. A missing subject or session value causes an explicit
unsupported result before publishing a value.

The token field and the process session have separate owners. The existing
`ProcessIdToSessionId` capture in WNF metadata initializes `PEB.SessionId` at
the architecture-specific offset. An uncaptured built-in profile retains its
existing zero process-session initialization. A token query uses its own
captured field; it does not substitute the PEB or process-session value.
Private-token controls demonstrate values across the complete DWORD width.

```text
ProcessIdToSessionId     -> Queries.session                -> PEB.SessionId
GetTokenInformation(12)  -> Catalog.primary_token_session_id -> token query(12)
```

| Value or capability | Behavior |
| --- | --- |
| Current-process/effective token pseudo-handle | Uses the captured primary-token property |
| Current-thread token pseudo-handle | `STATUS_NO_TOKEN` in the existing no-impersonation model |
| Guest primary-token handle | Requires `TOKEN_QUERY` (`0x0008`) |
| Impersonation-token session field | Explicitly unsupported because that metadata is absent |
| Impersonation-token AppContainer flag | Continues to use its own captured subject |
| Missing primary subject/session metadata | Explicitly unsupported; no invented zero |
| Other information classes | Retain their existing unsupported result |
| Guest token/session mutation | No setter is added |

For a nonempty output, the complete supplied span is probed with four-byte
alignment before token validation. The four-byte `ReturnLength` is mandatory
and accepts unaligned stores. Valid queries publish required length four before
the short-buffer result, then publish the DWORD on success. Overlapping output
therefore has the final write. Existing guard, failure and byte-preservation
semantics remain shared with class 29.

The query uses constant auxiliary storage and `O(P)` page probes for `P` visited
pages, plus the existing object/handle map lookup. It does not allocate an
output buffer proportional to the requested length. PEB initialization is a
single checked four-byte write.

Microsoft's [token information contract](primary/sources.json) identifies the
DWORD token field and setter privilege requirement. The process-session API
identifies the session associated with a process. The retained PHNT snapshot
provides the class declaration. Native observations establish private probe
ordering and failure precedence; those details are not inferred from API prose.

| Native controls | Recorded operations |
| --- | ---: |
| Four caller profiles, ten handle roles, lengths and 43 pointer/protection roles | 7,320 queries |
| Six values on private tokens in each profile | 24 value cases |
| Queries within the private-token controls | 72 queries |
| Private-token setters within those controls | 24 setters |

The caller matrix has `10 * (11 + 43 * 4) = 1830` queries per profile,
or `4 * 1830 = 7320` queries. Private controls have `4 * 6 = 24` value cases,
with three queries and one setter per case: 72 queries and 24 setters.

The private values are `0`, `1`, `7`, `0x12345678`, `0x80000000` and
`0xffffffff`. A private authority token enables an already-present privilege
only for its child thread. Each control modifies a fresh duplicate token,
queries before/after and through another duplicate, and reverts the child
thread before completion. The process primary token is not the setter target.
These controls corroborate scalar values and duplicate lifetime; they are
separate from the 7,320-row caller-boundary replay.

Source, post-run PE machine/LAA/hash and raw output identify all producers.
All four profiles ran on one ARM64 Windows 10.0.29683 kernel; x64 is a
compatibility caller, not a physical x64-kernel measurement. No native
executable or library is committed. `check-native.py` verifies both matrices;
`stress-native.py` rejects eleven altered proof sets after digest rebinding.

The existing CSR capture remains limited to its measured session-0 profile.
Initializing a different process session does not permit borrowing session-0
CSR backing. General other-session CSR completion remains outside this change.

## Final validation and application boundary

| Host | Full library passed/failed/ignored | Token focus | C API | Windows integration |
| --- | --- | --- | --- | --- |
| macOS ARM64 | 7906/0/2 | 26 | 168 | 544; native-memory adapter excluded |
| Linux x86-64 container | 7900/0/2 | 26 | 168 | 544; native-memory adapter excluded |
| Windows ARM64 | 7341/5/2 | 30 | 168 | 4 native-memory cases; Unix corpus excluded |

The focused suites include the complete class-12 caller matrix, nonzero values,
missing capture, impersonation boundaries, PEB separation and existing token
behavior. The native Windows focus includes actual installed native-host and
x86 NTDLL leaves. All-target checks pass on all three hosts. The five Windows
failures remain the four existing BZHI and one FP16 lowerer cases. The two
existing ignored microkernel cases are retained by identity.

Scoped Assist checks pass seven CTests and two executable scans on macOS and
five C++ consumers each on Linux and Windows. Complete member walks bind two
actual Cargo-selected archives per host, empty RAX core features and 404
selected source/fixture inputs. The final core/workspace verification binds
5713 inputs. The retained Assist Rust carriers use ABI minor 184; the compared
root d9bb4a8ac21f5b5a75f75846318dd09fd6c5e3f5 uses minor 200. All nine selected
C++ consumer source files match. A whole-current-root GUI/plugin/package build
is outside this validation.

The first Unix compile failed because the new test fixture supplied `Subject`
instead of `Arc<Subject>`. The sole Rust correction wrapped that test value in
`Arc::new`; production code did not change between epochs. Windows stopped
before Rust compilation: the first selected-source transfer omitted the C API
workspace manifest, and the next transfer omitted `Cargo.lock`. A complete
previously validated native workspace plus the corrected overlay then passed
5713 source identity checks. Logs and source manifests preserve all three
preparation states; no native preparation failure is counted as a test run.

The unchanged production observer now returns successfully from the previous
class-12 frontier in both ordinary programs. Its exact terminal outcomes are:

- `terminal turn=2149815 Complete(Internal("native NTDLL service NtQueryInformationToken: Unimplemented(\"NtQueryInformationToken: NtQueryInformationToken class 42\")"))`
- `terminal turn=2152164 Complete(Internal("native NTDLL service NtQueryInformationToken: Unimplemented(\"NtQueryInformationToken: NtQueryInformationToken class 42\")"))`

A controller exit of zero does not establish a guest exit. The absent third
helper is excluded from the execution count. Full ordinary Windows application
completion remains unproven. The next measured boundary is retained in
`validation/ordinary-analysis.json`; no unsupported path is mapped to success.

`check-validation.py` binds source bytes, explicit selection, exact counts,
known failures/ignores, native producer and observer identities, the actual
archives and the ordinary trace outcomes. Its adversarial checks reject altered
claims after digest rebinding. Native source and proof bytes have explicit
`-text` attributes; six representative file types retain identical Git blobs
with `core.autocrlf=true`. Source identity checks apply to the checked working
bytes, not arbitrary line-ending conversions in unrelated source files.

The later preserved upstream parent `e4f998244e3eec728f87a85482d4aa62566e802b` uses Rust ABI minor 203. Its nine selected C++ sources still match, but its newer Rust code was not part of the retained ABI184 carrier builds.
