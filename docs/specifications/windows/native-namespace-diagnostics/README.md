# Selected NT directory identity diagnostics

Native x64 Assist CI stops before guest execution with `selected NT directory
parent type/name changed`. That rejection covers three distinct conditions:
entry type, exact captured name, and an existing child attachment. The prior
message did not distinguish them.

The diagnostic now includes escaped selected/parent paths, selected/captured
names, captured type and attachment state. The validation predicate, fixed
selection, native capture operations, permissions and guest lookup policy are
unchanged. Formatting runs only after rejection and costs O(U) time/space for
U bounded UTF-16 units. It does not turn an invalid capture into a directory.
C ABI 1.14.0, status classification, defaults and dependencies remain unchanged.

The compiled regression first fails with the old generic diagnostic and then
passes with the contextual diagnostic. It covers a symbolic-link parent entry
and a case-different captured name. The same library test is built/run on
macOS, Linux and Windows; complete library/C API/integration/build selections
and retained Assist ABI 184 consumers are recorded under `validation/`.

The independent native probe opens fixed roots query-only and creates its own
temporary parent/child directory objects. Four ABI profiles verify that an
alternate-case open with OBJ_CASE_INSENSITIVE returns the original canonical
child identity and that all temporary handles close. This one build 29683 ARM64
kernel does not establish behavior on the x64 CI runner. Its fixed-root entries have Directory
type and the expected names. The actual x64 CI condition remains unknown until
the contextual error is captured on that runner.

Assumptions, stress/falsification probes, exact source identities, raw producer
outputs, selected test results and bounded findings accompany the ledger. Raw
bytes are protected from Git text conversion. A pre-Cargo Docker mount failure
is retained separately; it is not a runtime or test failure.

The larger goal remains active. `whoami` still reaches the unimplemented
NtQueryVirtualMemory class 3; these diagnostics do not implement that query or
establish complete Windows process support. Current-root GUI/plugin/packages
remain separate from the retained ABI 184 consumer boundary.
