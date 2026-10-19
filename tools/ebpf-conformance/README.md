# RFC 9669 external oracles

Run from the repository root:

```sh
python3 tools/ebpf-conformance/run.py            # every oracle
python3 tools/ebpf-conformance/run.py conformance
python3 tools/ebpf-conformance/run.py kernel-le  # --scale N (default 400)
python3 tools/ebpf-conformance/run.py kernel-be
python3 tools/ebpf-conformance/run.py admission-le   # and admission-be
python3 tools/ebpf-conformance/run.py all --export   # rewrite tests/generated/ebpf/
```

Every oracle executes identical program bytes on the direct `EbpfCpu` and on
SMIR at O0, O1, and O2. This package is outside the engine workspace and has
its own lockfile. The flat execution layout, engine adapters, and kernel
campaign generator live in `tests/support/ebpf_rfc_oracle.rs`, which the
engine's `ebpf_diff` target shares to replay the checked-in corpora without
these tools.

| Oracle | Implementation | Evidence |
|---|---|---|
| `conformance` | eBPF Foundation `bpf_conformance` runner (pinned archive under `docs/specifications/ebpf/bpf_conformance/`) with `rax-bpf-conformance-plugin --engine <plane>`, `--cpu_version v4`, default groups | `Passed 312 out of 312 tests` per plane; the 45 raw reserved-field negatives (which the runner skips because their error text is empty) must be rejected; the assembled vectors must equal `tests/generated/ebpf/bpf_conformance.txt` |
| `kernel-le` | Linux arm64 BPF JIT, privileged Docker container (OrbStack Linux 7.0.14), `BPF_PROG_TYPE_SYSCALL` and `BPF_PROG_TEST_RUN` | 154 instruction forms; 147,698 cases at scale 0 and 190,738 at scale 400; complete context buffers and return values per program |
| `kernel-be` | Debian 13 s390x Linux 6.12.107 booted under QEMU; no BPF JIT, so the generic kernel interpreter executes | The same 147,698 cases in big-endian encoding |
| `admission-le`, `admission-be` | The verifiers of both kernels in load-only mode | 1,980,208 encodings: every opcode x register, offset, and immediate partitions (and LDDW second slots), plus every opcode x all 16 x 16 register fields; each verdict is classified as accepted, syntactic, or semantic and compared with `decode_insn`; every disagreement must have a documented reason |

Requirements: stable Rust, CMake, a C++20 compiler and Boost
(`program_options`, `filesystem`) for the runner; `clang` with the AArch64 and
SystemZ targets and `ld.lld` for the freestanding harness; Docker with a
privileged arm64 Linux engine; `qemu-system-s390x`. The ELFIO submodule and
the Debian kernel package are downloaded on first use and verified against
the SHA-256 values in `run.py` and
[PROVENANCE.md](../../docs/specifications/ebpf/PROVENANCE.md).

## Kernel campaign

`kernel/harness.c` is a freestanding loader with no libc. In a container it
reads `/work/input.bin`. As PID 1 of an initramfs it reads `/input.bin`,
writes its results to a virtio-serial port, and powers off. Each program
batches up to about 200 cases of one form. A case loads `r0..r9` and an
optional 64-byte stack window from the context, executes the instruction under
test, and stores the observable state back. Branch cases record their outcome
on both paths before a join at which every register is dead, so the verifier
prunes them. Calls cover BPF-to-BPF frames, nested calls, and stores through a
caller's stack pointer. Atomics cover both widths and all ten operations on
the stack.

The kernel verifier admits only safe programs. The campaign therefore leaves
out encodings the verifier rejects by policy rather than by ISA rule: division
by an immediate zero, immediate shift counts outside the operand width,
unaligned stack accesses, and reads of `r1..r5` after a call. RFC 9669 defines
those encodings; the engine suites and `bpf_conformance` cover them.

`rax-ebpf-kernel-campaign compare` fails on any byte of difference and names
the case, field, and operands; `export` additionally writes the per-program
digest corpora that `ebpf_diff` replays.

## Admission campaign

`rax-ebpf-kernel-admission` places each candidate after a prologue that
initializes `r0..r9` and eight stack slots, and the harness only loads it. A
verifier rejection is syntactic when its message names an encoding error
(unknown opcode, reserved fields, invalid register, immediate, or atomic
operation); any other rejection is a program-safety verdict that a decoder
does not make. Every disagreement must be one of three classes:

- RFC 9669 defines the result, but the verifier refuses the encoding by
  policy: an immediate zero divisor, or an out-of-range immediate shift count.
- The encoding names a platform object the load did not bind: `map_by_idx`
  (LDDW subtypes 0x5 and 0x6) without a program map set. The engine admits
  the encoding and traps with `UnsupportedPseudoLoad` if it executes unbound.
- A Linux extension postdates RFC 9669: `may_goto`, or load-acquire and
  store-release atomics.

`export` records the classified table, every disagreement, and a digest of
the decoder verdicts, which `ebpf_diff` pins.
