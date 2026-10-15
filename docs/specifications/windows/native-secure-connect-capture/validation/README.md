# Completed secure-port capture validation

The corrected epoch 2 source passes the measured capture contract on all three
hosts with Rust 1.95.0. This group implements failure capture and selected
namespace lookup. Successful CSR connection and ordinary Windows program
completion remain requirements of the active process-userland objective.

| Host | Library passed/failed/ignored | C API passed | Focused passed |
| --- | --- | --- | --- |
| macOS arm64 | 7814/0/2 | 168 | 6 |
| Linux x86-64 container | 7808/0/2 | 168 | 6 |
| Windows ARM64 | 7235/5/2 | 168 | 7 |

All three all-target checks pass. Both Unix hosts execute 544 Windows
integration cases; Windows executes four native memory integration cases.
The two ignored tests are the pre-existing full-microkernel lowering and
exact-byte microkernel roundtrip cases. Windows retains exactly the preceding
four BZHI and one vector FP16 lowerer failures; names and raw output are retained.
These results do not claim those failures were fixed.

The native corpus independently verifies eight actual producers, 396 failure
observations, zero caller exceptions and twelve explicit closes. Its installed
NTDLL status control measures C0000078 -> Win32 error 1337. The Windows focused
suite executes the original nine-argument installed ARM64/x86 leaves, including
x86 RET36 and the ninth argument. Compatibility ABIs execute on an ARM64 kernel;
physical x64 kernels and other builds remain unverified.

Scoped owning builds pass on all three hosts. macOS links CLI/MCP, passes all
seven CTests and both protected-text scans. Linux and Windows compile and run
five C++ consumers each. Complete static/rlib archive walks bind 274 selected
RAX inputs per host with empty core features. These builds use frozen older
Assist exports. Nine current C++ input files match those exports; this does not
establish whole-current-root, plugin, GUI or package validation.

Initial failures remain visible. Epoch 1 macOS nativeTrue exceeded its unchanged
30 s budget twice. A separate same-process diagnostic continuation exited zero
at 42.245449 s; its original assertion still failed. Epoch 2 passes the canonical
seven CTests. Its initial full library run has one multishot timer mismatch;
three exact unchanged reruns and the complete same-binary rerun pass. Its initial
C API run times out during native printf startup; the canonical 168-test rerun
passes with unchanged code, limits and assertions. Causes remain unknown;
host contention is an inference, not established causation. The earlier nightly
Darwin workloop timeout also remains separate from this pinned validation.

Windows preflight preserves the native offline Cargo adapter. The exact current
316 scoped inputs were freshly verified, including the 274 selected embedding
inputs and four previously omitted SBPF references. The redundant native full
8578-file scan was interrupted before tests and is not claimed complete.
The common manifest records 8578 inputs plus four scoped identities. First-pass
CTest/diagnostic/archive logs are retained, while two first-pass build/shipping
text logs were overwritten before epoch 2; their raw text is not claimed retained.

Run `python3 docs/specifications/windows/native-secure-connect-capture/validation/check.py`
for portable verification of collected hashes, compiled input snapshots, actual
counts and retained failures. The collected drivers and cache checker preserve
commands as executed and require their original host/container/VM paths; they
are provenance rather than relocatable build entry points. Raw CRLF output is
preserved. `post-run-documentation.json` records documentation reconciliation
after completed tests; no Rust execution input changed.

Diagnostic executable and object bytes remain in the local task cache. Their exact hashes
and retained paths are recorded in `external-diagnostic-artifacts.json`; source,
commands and raw logs are included here. The first unpublished commit containing
the binaries was rejected by GitHub and is not part of published history.
