# Verified selected-directory aliases

The Windows CI trace at pipeline 71518/job 424894 identifies the rejected entry
`\Sessions\34651\BaseNamedObjects` as SymbolicLink, with the expected name and
no attached child. Its actual target is unknown in that trace. The constructor
previously required every selected child to have Directory type.

Selection now admits one narrow profile: the current nonzero-session
BaseNamedObjects link may resolve to the independently selected fixed
`\BaseNamedObjects` object. Native acquisition retains at most nine fixed
directory handles until NtCompareObjects proves the original pair identical.
A refreshed alias open must retain that identity/grant, and captured link
text/access must remain unchanged. No arbitrary captured target is opened.
The identity export is resolved only when this alias profile is encountered;
missing or contradictory evidence produces an explicit failure.

The immutable namespace retains SymbolicLink enumeration and link-query
metadata. A separate resolved-directory reference reuses the selected target
Arc, preserving directory identity and Win32 named-object ownership. The
ordinary type/name/attachment rejection remains in place without identity
evidence. DONT_REPARSE, type checks, partial-directory uncertainty, name grammar
and the 63-reparse limit retain their existing ordering. Handles close before
the namespace is published, including error cleanup.

This implementation does not establish that the observed CI link has the fixed
target until that lane executes it. A different target fails with the captured
target and comparison status. No blanket SymbolicLink-to-Directory conversion
or name-case relaxation is applied.

The independent C++ source creates only private temporary NT objects and opens
fixed roots query-only. Its ARM64, x64, x86 and x86-LAA captures establish:

| Native observation | Result |
|---|---|
| Child versus followed directory link | NtCompareObjects = 0x00000000 |
| Parent versus followed directory link | NtCompareObjects = 0xC00001AC |
| Parent enumeration | Link retains SymbolicLink type |
| Canonical child versus followed name | Equal |
| Link object name versus child name | Distinct |
| OBJ_DONT_REPARSE directory open | 0xC000050B; no opened handle |
| Handle count before/after | Equal in all four profiles |

All profiles use one Windows 10.0.29683 ARM64 kernel. The x64 executable view
does not establish an independent physical x64 kernel result. Probe sources,
PE identities and six runtime modules are bound before and after execution.

The first compiled regression fails under the old rejection. Its exact two
source files are reconstructed and match the recorded input hashes. Three
portable tests cover canonical identity, absent/unproven/wrong/duplicate pairs,
rights/case disagreement, missing metadata, partial enumeration, malformed
suffixes and reparse boundaries. A Windows acquisition test creates private
temporary parent/child/link objects, executes the actual adapter's positive
comparison and distinct-object rejection, and closes its handles.

| Complete library binary | Passed | Failed | Ignored |
|---|---:|---:|---:|
| macOS | 8102 | 0 | 2 |
| Linux | 8096 | 0 | 2 |
| Windows | 7564 | 5 established lowerer failures | 2 |

Each host also runs 183 C API tests, selected integration binaries (544 cases
on macOS/Linux, 4 on Windows) and all-target checks. ARM64/x64 MSVC target
checks from macOS compile the Windows-only adapter/test as additional evidence;
they do not replace native Windows execution. The failed unconfigured cross
check is retained separately from the cached-SDK success.

Retained Assist ABI 184 consumers bind 3,826 RAX source inputs, empty core
features and actual structural archive walks before/after linking. Seven
macOS CTests and six link commands, plus five C++ consumers each on Linux and
Windows, pass. Actual rebuilt owning-core and C++/CAPI observers run ARM64 smoke
to exit 0; installed whoami retains the NtQueryAttributesFile 0x3d frontier.
Native controls, sources, modules, programs and artifacts are stable. The four
executed artifacts have independent retained copies with exact hashes.

The original frozen graph contains 25,085 paths / 1,835,122,521 bytes. Seven
owned code/test/engineering paths retain those exact bytes through linear
integration of the published evidence-correction parent. Historical diagnostic
data in the graph has explicit public projections; original identities remain
recorded. The two new private compiler-diagnostic records are omitted before
first publication and bound through the shared projection table. No private
implementation snippets are required to reproduce the public checks.

Run `python3 -B check_validation.py --host macos --source /path/to/rax`, or the
corresponding Linux/Windows value. Assumptions, source review, raw observations,
commands, rejected setup attempts and projection bindings are under
`validation/`. Source verification costs O(B) time for B bytes with bounded
streaming buffers; namespace construction remains O(E log E + U) for E entries
and U UTF-16 units, with one bounded alias attachment and no reference cycle.

C ABI 1.14.0, public layouts, defaults, dependencies, guest permission policy
and persisted formats are unchanged. The broader process-emulation goal,
actual CI alias target and current-root ABI 272 GUI/plugin/package execution
remain outside the completed retained-consumer claim.
