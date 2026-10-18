# Selected base named-object directories

The installed Windows runtime selects `\BaseNamedObjects` together with its
existing NT namespace roots. For a nonzero captured primary-token session, it
also selects `\Sessions`, `\Sessions\<session>` and that session's
`BaseNamedObjects` directory. Selection uses the token session captured by the
existing token-query adapter. The independent process session used for the PEB
remains unchanged.

Selected descendants are attached from deepest to shallowest. A child requires
a separately captured parent and a matching directory entry. Absolute and
root-relative opens therefore refer to the same directory object. Missing
ancestors, mismatched names/types, duplicate selections and another session's
paths are rejected. Native opens acquire only private capture handles; guest
opens use the retained objects without forwarding a request to the host.

Only the three existing KnownDLL directory families enter the installed image
section capture path. A section listed in `BaseNamedObjects` does not become an
executable image. Uncaptured child directories and other object resources retain
an explicit unavailable result.

## Capture and case policy

Native directory enumeration is a live traversal. In the retained measurements,
individual successful enumerations sometimes returned duplicate names, and
successive traversals sometimes differed. Complete capture accepts a pair only when the sorted names and types agree
and the model's existing NT case-folded name keys are unique. It attempts at
most three pairs. A KnownDLL image directory must satisfy that rule. If a
non-image directory is still changing, capture retains only unique name/type
pairs present in both final enumerations and marks that directory incomplete.
An absent name in an incomplete directory returns an explicit unselected
result, never a fabricated NAME_NOT_FOUND result. Selected ancestor entries
must still be present. This is an immutable guest copy; it does not establish
a transactional host snapshot or follow later host namespace changes.

The first full Windows run exposed four unrelated installed-leaf tests failing
at runtime selection because all three namespace pairs changed. A controlled
producer creating and closing only its own named events reproduced capture
failure in all 16 old-runtime attempts; all 1008 event handles were closed.
The revised model adds explicit incomplete-directory state rather than raising
the retry count or hiding a missing member.

Case-policy sampling considers at most 16 names with ASCII letters. Each
candidate is queried with its original spelling, an altered spelling, and
explicit case-insensitive lookup. Only a successful or type-mismatch reference
with a matching explicit result establishes that the candidate still exists.
A missing, inaccessible or inconsistent candidate supplies no case-policy
result. Exhausting the candidates fails explicitly.

The existing 8192-entry and 16 MiB retained-name/type/target limits remain. The
selected tree has at most nine directories. Tree attachment is O(D log D) for
D <= 9; entry admission is O((E + U) log(E + 1)) for E entries and U UTF-16 units,
including key comparisons. Consecutive-list checking has the same upper bound,
including folded-name uniqueness.
Native enumeration and bounded case probes remain acquisition work only.

## Native evidence

All producers ran on one Windows ARM64 kernel reporting `10.0.29683`. The four
caller profiles were ARM64, x64 compatibility, x86 and x86 with LAA. This does
not establish behavior on a physical x64 kernel.

| Measurement | Retained extent |
| --- | --- |
| Current-session access survey | 4 profiles x 6 paths x 4 flags x 16 access masks = 1536 opens |
| Explicit session-1 access survey | 4 profiles x 6 paths x 4 flags x 16 access masks = 1536 opens |
| Consecutive directory traversals | 4 profiles x 6 passes = 24 enumerations, 17436 entry rows |
| Snapshot pairs | 10 matching pairs and 2 rejected pairs |
| Direct service replay | 256 global-path rows plus 1024 selected session-1/ancestor rows |

The native principal's token session and process session were both zero, and
its private-namespace flag was zero. The session-1 survey opens a directory
observed under `\Sessions` using that same principal; it does not impersonate
an interactive session-1 user. Every producer checks its source, PE machine,
LAA bit and executable identity, and balances its private native handles.

The first coherence producer contained an invalid C++ backslash escape. It
failed before enumeration and is retained under `development/`; its result is
not evidence of a namespace failure. The corrected version uses the proper NT
path, makes that compiler warning fatal, and checks unique names independently
of their types.

`check-native.py` validates raw matrices, source/PE identity, native grants,
private-handle balance, live duplicates, pair decisions and the baseline
application traces. `stress-native.py` rejects altered claims after rebinding
file digests. The raw proof inputs are listed in `native-inputs.json`.

## Baseline and validation state

Two diagnostic observers independently identify the same former startup stop:

| Program | Scheduler turn | Requested path | Access | Attributes |
| --- | --- | --- | --- | --- |
| `smoke.exe` | 2149956 | `\BaseNamedObjects` | `0x0000000F` | `OBJ_CASE_INSENSITIVE`, root handle zero |
| `whoami.exe` | 2152305 | `\BaseNamedObjects` | `0x0000000F` | `OBJ_CASE_INSENSITIVE`, root handle zero |

The detailed observer retains the preceding instruction history. The second
observer runs the first 2100000 scheduler turns as one bounded call, verifies
`BudgetExhausted`, and then inspects individual turns. Both preserve the
production startup path and end at the former uncaptured directory. Neither
baseline trace is a successful guest exit.

## Final validation and production startup

| Host | Full library passed/failed/ignored | Focus | C API | Windows integration |
| --- | --- | --- | --- | --- |
| macOS ARM64 | 7921/0/2 | 12 | 168 | 544; native-memory suite excluded |
| Linux x86-64 container | 7915/0/2 | 12 | 168 | 544; native-memory suite excluded |
| Windows ARM64 | 7358/5/2 | 13 | 168 | Four native-memory cases; Unix corpus excluded |

The Windows all-target check first stopped with insufficient disk space. Only
the stopped build's incremental cache was removed; its archives and completed
tests were retained. The same check passed with incremental caching disabled,
and all 5728 source identities were checked before and after recovery.
All-target checks pass on all three hosts. Windows retains the five existing
lowerer failures; the two existing ignored microkernel tests are reported
separately. The focused Windows run includes an installed NTDLL entry compared
with an independent native open of the current token's selected directory.
The final source matches 5728 selected inputs rooted at RAX
`be05c4b474dbf76be910e0f50dbc4be3262c16ce`. A second source revision addresses
four installed-runtime capture failures from the first full Windows run.
The original failure logs and source files are retained. Controlled native event
churn rejected all 16 captures before the revision and completed all 16 after it;
each run closed every event handle it created. This probes capture, not guest exit.

Retained Assist checks pass seven CTests plus two executable scans on macOS,
and five C++ consumers each on Linux and Windows. Two complete actual
Cargo-selected archives per host have empty core features and bind 419 selected
source/fixture inputs. Frozen Assist Rust carriers remain ABI 184. The compared
root used ABI 207; the upstream snapshot
`4b0cfd1c1ed117850a92b60985227d2c7a852864` uses ABI 241.
Nine selected C++ files match. These retained-carrier results do not establish a
whole-current-root GUI/plugin/package build.

Actual unchanged-observer outcomes:

- `terminal turn=2149970 Complete(Internal("native NTDLL service NtQueryInformationToken: Unimplemented(\"NtQueryInformationToken: NtQueryInformationToken class 44\")"))`
- `terminal turn=2152319 Complete(Internal("native NTDLL service NtQueryInformationToken: Unimplemented(\"NtQueryInformationToken: NtQueryInformationToken class 44\")"))`

Both ordinary programs open the captured BaseNamedObjects directory successfully and stop at the following unimplemented NtQueryInformationToken class44 call. Neither program reaches a successful guest exit.

The absent helper is excluded from the execution count. `check-validation.py`
binds source identities, exact test selection/counts, existing failures/ignores,
actual selected artifacts and ordinary outcomes. Adversarial native and final
validation checks reject altered claims after rebinding file digests. Native
proof has explicit `-text` attributes; six representative file types preserve
identical Git blobs with `core.autocrlf=true`.

Assumptions, execution-plane ownership and bounded findings are recorded in
`assumption-register.json`, `change-surface-map.json` and `bounded-scope.json`.
