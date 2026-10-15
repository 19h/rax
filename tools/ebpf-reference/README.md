# Pinned Anza SBPF execution oracle

Run from the repository root:

```sh
python3 tools/ebpf-reference/run.py
```

The runner verifies and extracts the upstream source archive under `docs/`,
then runs this independent Cargo package with its committed lockfile. It uses
stable Rust, an isolated target directory, the engine's existing dependency
patches, and Anza SBPF commit `3e1510b1eac344af15c99dcdb4c3b3e6dc2b3d7b`
(version 0.25.0). Cargo may retrieve lockfile-pinned build dependencies on first
use. Modified or unexpected files in the extracted upstream cache are rejected.

This package is outside the engine workspace and must be invoked separately.
It does not change the engine's feature defaults or dependency lockfile.
Upstream execution is interpreted, with its tracer enabled; native SBPF JIT
execution is not exercised.

The four tests execute 30 configurations:

| Configurations | Verification |
|---|---|
| 10 | Nested immediate/register calls, V0-V4 frame policy, eleven GPRs; direct CPU and SMIR O0/O1/O2 |
| 15 | Versioned rodata addresses, read-only stores, and stack-gap accesses |
| 2 | V3/V4 reject ordinary data reads through the bytecode address |
| 3 | V0 access starting near a frame end, starting in a gap, and exceeding backing bounds |

The VM's register buffer does not contain final interpreted GPRs. The adapter
uses the trace before successful root EXIT, checks the returned r0 and zero call
depth, and compares those eleven GPRs. Fault probes compare success/trap behavior.
The source archive, checksum, retrieval date, and license provenance are recorded
in [PROVENANCE.md](../../docs/specifications/ebpf/PROVENANCE.md).
