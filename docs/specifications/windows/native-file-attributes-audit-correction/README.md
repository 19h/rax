# Native Cargo ledger integrity correction

The first Windows corruption replay at ac3b2031 accepted a modified original
Cargo-configuration hash. The source verifier checked its separately pinned
374-byte Windows override but did not also pin the original 552-byte ledger
entry. The Windows stress runner reported `accepted semantic corruption:
frozen-source-substituted`; no product/query execution failure is inferred.

The verifier now pins both original and native configuration identities. The
original compiler graph, all product source, native observations and executed
owning artifacts remain unchanged. macOS/Linux initially rejected the corruption
because their normal source-byte check used the original entry. The failed
Windows trial remains explicit. Subsequent corrected-checker platform runs are
recorded separately; the previous portable evidence-audit passes did not execute
the corruption runner.

Assumption A1: the only intended source substitution is the known native Cargo
configuration. Basis: frozen source/frame and native gate configuration digest.
Stress test: corrupt the original ledger hash while retaining the valid native
override. Falsification: any host accepts the rebound corruption. Status: revised
verification rule confirmed by two corrected corruption rounds per host.

This is a high-impact evidence-integrity correction. It does not establish full
process completion; NtOpenFile remains the measured ordinary-program frontier.
The original frame is 1835033720 bytes; replacing 552 by 374 gives 1835033542
native Windows bytes. No ABI, default, dependency or permission policy changes.

The corrected corruption runner passes twice on each of macOS, Linux and native
Windows: 13 corruptions and 13 restored positive corpora per run, totaling
78 semantic rejections and 78 positive restorations. Two additional portable
evidence-audit rounds also pass on each host against the identical corrected
main envelope. Windows additionally hashes the four retained owning artifacts.

Pipeline71768 on Assist5b9fd158 separately reports a macOS AMTP socket-ownership
test failure: F_GETFD returns1 after free instead of-1. Descriptor-number reuse
is a consistent inference, not an established cause or repaired behavior. Its
AMTP source is unchanged here; whole-root CI success remains unclaimed.

QG1-QG7 pass for this bounded integrity correction: no normative judgment, one
reconciled assumption, direct three-platform controls, exact byte accounting,
explicit failed trial and current CI boundary, hash-bound evidence, and no
adjacent product mutation. The full process objective remains active.
