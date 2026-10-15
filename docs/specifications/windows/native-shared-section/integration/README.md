# Linear upstream integration validation

The SharedSection feature was committed as
`cbcb012f26a4b70b2b8922c9f5c5b0f076a04cb1` on base
`7951b3b29d1f011999b32de797e7d12dee8501b8`, then rebased onto the
published upstream `84e194fff5c256da8cdba45ee1c8c3097dd216fe` as
`aadc57b6014d6cc6acc0dd14679fda8b673bb19e`. All 106 owned file blobs
remained byte-identical. Upstream's 51 changed paths did not overlap them.
Only the unpublished feature commit was replayed; published history was retained.
The parent corpus's earlier results remain historical evidence under their
original source hashes.

Combined library pass/fail/ignored counts are macOS 7798/0/2, Linux 7792/0/2,
and Windows ARM64 7217/5/2. The five Windows failures are the preceding BZHI/FP16
baseline, listed in summary.json. Each host passes 168 C API tests and the
all-targets check. Focused shared-section replay tests pass on all three hosts;
Windows additionally runs the installed ARM64 and x86 NTDLL leaf test. The
Windows integration target's host selection remains visible in its raw log.
Serialized Linux integration passes 42 tests with one deliberately ignored Docker
oracle on each Unix host. Broader Darwin integration was still running when this
record was frozen; the preceding base's reclaim deadline mismatch remains a
separate finding. These records do not claim an entirely passing project suite.

The Windows export was checked against 8475 tracked source and input hashes
before compilation. Unix checks used the same immutable checkout. The 249-unit
owning scope was refreshed after integration and checked against all three
embedding archives, each with empty RAX core features. The nine recorded C++
consumers still match the current root. macOS's seven scoped CTests and both
protected-text scans pass; five rebuilt scoped C++ consumers pass on Linux and
Windows. The owning roots are frozen exports, not proof of the entire current
root, IDA plugin, GUI, or package matrix.

The unchanged observer again reaches NtCreateSection, service 0x4a at
0x1800014e0, turn 2115129. Three ordinary CRT samples share that frontier;
whoami still reaches NtTraceEvent. Ordinary Windows guest exits remain zero.
The full same-host process objective remains active.

Run `python3 check.py` here to verify the retained artifact hashes, counts,
unchanged feature blobs, current scoped sources, and archive records.
