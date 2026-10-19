# SDE startup regression, 2026-10-19

The reported `rax-user` command is exercised against Intel SDE 10.8.0 from
`sde-external-10.8.0-2026-03-15-lin`, on macOS 27.2 / AArch64. Baseline HEAD is
`51aa9b11fbe4181b63f19e704c343c531bfb1d16`, on `master`, with unrelated existing
worktree changes. The earlier instruction fix, commit `b7dbbfa16`, exists on
`fix/x86-rip-relative-imm` and was absent from this checkout and its installed
binary. This change ports the addressing fix without merging the other branch.

## Observed failure and resulting execution

The installed baseline reproduces SIGSEGV at `0x4d208000`. Pin executes these
eleven instruction bytes at `0x48be36`:

```text
66 48 0f 3a 22 05 57 71 0b 00 01   PINSRQ xmm0,[rip+0xb7157],1
```

The address calculation is exact integer arithmetic in bytes:

```text
next RIP       = 0x48be36 + 11 bytes           = 0x48be41
operand        = 0x48be41 + 0xb7157 bytes     = 0x542f98
old operand    = operand - 1 byte            = 0x542f97
corrupt pointer = 0x4d2080 << 8 bits          = 0x4d208000
```

The legacy `0F` and `0F3A` dispatchers now set the trailing-immediate length
before decoding a ModR/M address. The full instruction length is the base,
including with address-size override. Existing VEX/EVEX accounting is unchanged.

Removing that failure exposed the remaining startup dependencies in order:

```text
launcher -> fork -> intermediate -> fork -> Pin server
    |                  exits                  |
    +<--- direct ancestor tracing link -------+
    |                                         |
    +---- current guest procfs queries --------+
    +---- reopen server memfd via SCM_RIGHTS --+
    |
    +-> injected Pin runtime -> inner SDE argument check -> exit 1
```

The supplied command runs an inner `sde64` without an application argument. It
now reaches the inner application's diagnostic:

```text
SDE ERROR: Need at least "-- application" name. Use -help for help.
```

Direct execution of that inner command produces the same diagnostic and status
1. Running SDE with the Linux `hello` fixture instead produces the fixture's
three expected output lines and its exit status 41, matching direct execution.

## Runtime changes and boundaries

- Fork establishes separate ancestor/descendant socket pairs before creating
  the host child. `SCM_RIGHTS` distributes the correct ends. The intermediate
  process can exit without breaking subsequent attach, stops, requests, or
  replies. At depth d, preparation takes O(d) time and descriptors.
- `PR_SET_PTRACER` validates task IDs and the zero/-1 forms. RAX already reports
  Yama `ptrace_scope=0`; this setting is preserved. Actual attach still checks
  credentials and dumpability.
- A stopped tracee reports its executable with its stop. Read-only `maps`,
  `auxv`, `cmdline`, `environ`, `stat`, `status`, and `comm` files are queried
  from current guest state at an owning process's scheduling boundary.
  Pending opens retain their pathname and flags. Procfs replies have a separate
  namespace and requesting-thread identity, and abandoned replies are discarded.
- Opening a linked process's memfd descriptor transfers the host backing and
  validated inode metadata. The supported seal set is immutable
  (`F_SEAL_SEAL`); mutable seal sets and other inode kinds are deliberately
  refused with `EOPNOTSUPP`. The independent reopened position uses positional
  I/O, shared atomic storage, a local mutex, and a private POSIX record lock.
  Guest dup/fork share that position, and the kernel releases the record lock
  on owner exit. Mappings continue to share the original backing.

## Change-surface map

| Plane | Disposition |
|---|---|
| Direct decode | Affected: legacy immediate length before ModR/M addressing |
| Direct execute | Existing SIMD effects retained; their memory address is corrected |
| CPU state | No register layout, serialization, or exception contract changes |
| Memory/MMU | Linux descriptor/backing transport affected; address-space translation and permission algorithms retained |
| SMIR lift | Existing complete-length address calculation retained; parity checked |
| SMIR IR/interpreter | No operation changes; used as a separate execution comparison |
| Optimizer | Unchanged; no new operation or optimization |
| Native lowering/JIT runtime | Unchanged; no new admission or lowering |
| Emulator/KVM/HVF adapters | Unchanged; no backend state or exit changes |
| Machine/devices | Unchanged; process-level behavior only |
| Oracle/analysis | Unchanged; independent SMIR decoding already counted the immediate |
| C ABI | No ABI layout, identifier, or entry-point changes |
| Tests/docs | New reachable library regression modules and reproducible native probes |

## Assumption register

| ID | Assumption | Basis | Dependent result | Stress test | Falsification probe | Status |
|---|---|---|---|---|---|---|
| A1 | The available SDE package reproduces the reported workload | Exact path and identical fault address | Attribution to the user's command | Installed binary and minimal independent ELF | Package hashes or baseline fault differ | Confirmed locally |
| A2 | Both cold and cached legacy dispatch use the same length rule | Resolver source and repeated instruction execution | Complete addressing fix | Positive/negative displacement, address-size override, loads/stores, indirect call | Regressions below; old minimal ELF fails | Confirmed |
| A3 | Copied remote seals cannot subsequently change | Export requires `F_SEAL_SEAL` | Correct remote inode checks without a shared mutable seal word | `MFD_ALLOW_SEALING` descriptor rejected | Mutable seal descriptor is accepted | Confirmed |
| A4 | Guest fork occurs outside a reopened file's active cursor operation | Serialized process scheduler; position operations finish within file I/O | Local mutex inheritance and shared cursor | Concurrent readers and native/emulated fork-position probe | Probe deadlocks, repeats/skips a byte, or changes owner's position | Confirmed for the exercised runtime path |
| A5 | Exit 1 is the inner SDE's own argument rejection | Direct inner execution has identical output/status | Interpretation of the supplied command's final result | SDE running `hello` succeeds with expected status/output | Direct execution or instrumented fixture differs | Confirmed |

## Reproduction and independent evidence

Package SHA-256 identities:

| File | SHA-256 |
|---|---|
| `sde64` | `39c8c54e005ea955f6ed0051e5b625fa0746ce28b2c41c73d067244c3ab02ad9` |
| `intel64/pinrt/pind` | `3a5c0b69801766a502e3ab5e0a5f705e69c322754b3d3c9e4949995f66bf8dcf` |
| Installed baseline `rax-user` | `e1b6de81f6f74d3d02a5f27629edf7b3dbd713fb1749d3cf52402b35dfd18f54` |

Create a fresh minimal regression file:

```sh
python3 docs/development/rax-user-sde-repro/generate_pin_pointer.py /tmp/pin-pointer.elf
rax-user /tmp/pin-pointer.elf
```

The baseline faults at `0x40110000`, status 139. The fixed binary exits 0.
The four instruction regressions additionally exercise 96 direct loads, 48 SMIR
comparisons, eight stores, and two Pin-style pointer-call runs.

`ptracefamily.c` independently checks orphaned-grandchild attach, stopped memory
reads, executable access, maps containing a known fixed mapping, and detach.
`remotememfd.c` checks independent reopen offsets, a fork-shared reopened offset,
shared mapping contents, unchanged owner offset, and backing lifetime after the
owner exits. Both return 0 with identical output on native Linux AArch64 and
under `rax-user` on macOS. The native oracle is Linux
`7.2.9-orbstack-00391-g2b298d05fbd4`, AArch64, with UID/GID 1000 and all
capabilities dropped; it is distinct from the Linux 6.19 specification revision.

For example, build/run either probe on the native oracle, then run its static
AArch64 ELF using `rax-user`:

```sh
mkdir -p /tmp/rax-sde-oracle
docker run --rm --user "$(id -u):$(id -g)" --cap-drop=ALL --security-opt seccomp=unconfined \
  -v "$PWD/docs/development/rax-user-sde-repro:/src:ro" \
  -v /tmp/rax-sde-oracle:/out --workdir /src rust:1-trixie \
  sh -c 'gcc -O2 -static ptracefamily.c -o /out/ptracefamily && /out/ptracefamily'
rax-user /tmp/rax-sde-oracle/ptracefamily
```

## Owned files and installed release

The task owns the following source paths; pre-existing changes elsewhere are
excluded:

```text
src/isa/x86_64/
  decode/dispatch/mod.rs
  decode/dispatch/twobyte/dispatch/{primary,escape_3a}.rs
  user_mode_tests.rs
  user_mode_tests/rip_relative.rs
src/user/linux/
  process.rs, procfs.rs, wait.rs
  fs/{mod,fd,reopen}.rs
  ptrace/{mod,family,procfs,tracee}.rs
  syscall/{child,fcntl,path,process,ptrace}.rs
  tests/{mod,ptrace,ptrace_fork,ptrace_jobctl}.rs
docs/
  development/rax-user-sde-repro/{README.md,generate_pin_pointer.py,ptracefamily.c,remotememfd.c}
  reference/status-and-limitations.md
  specifications/linux/kernel-6.19/security/yama/{SOURCE.md,yama_lsm.c}
```

The final executable was built with normal release settings, including the
repository's release LTO configuration:

```sh
cargo +stable build --locked --release --bin rax-user --target-dir /tmp/rax-sde-fixed-target
```

It is installed at `/Users/int/.cargo/bin/rax-user` and
`/Users/int/dev/rax/target/release/rax-user`; both SHA-256 identities are
`8170ad1204a55bf1f08179d8ac2a7261fadd287d332c06d8aacd3fb687bc88ca`.
The executable present immediately before final installation is preserved at
`/tmp/rax-sde-oracle/rax-user-before-final-install-f778b902426fd1e268c351ecb43cabc9244d2b396ae6a7009ae3306adec16dd9`.
The minimal pointer ELF and both standalone probes pass using this release.
The installed release's SDE/hello execution returns 41, with byte-identical
stdout to direct hello execution and empty stderr.
The supplied SDE-on-SDE command was also rerun through the installed binary;
its only diagnostic is the inner SDE argument error above, with exit status 1.

```sh
rax-user --sysroot ~/dev/rax/sysroot /Users/int/Downloads/sde-external-10.8.0-2026-03-15-lin/sde64 -- /Users/int/Downloads/sde-external-10.8.0-2026-03-15-lin/sde64
rax-user --sysroot ~/dev/rax/sysroot /Users/int/Downloads/sde-external-10.8.0-2026-03-15-lin/sde64 -- /Users/int/dev/rax/tests/fixtures/user/linux/bin/x86_64/hello
```

## Primary provenance

- Intel 64 and IA-32 Architectures Software Developer's Manual, December 2024,
  Vol. 2A, section 2.2.1.6 and the PINSRQ encoding/operation, retained in
  [the combined SDM](../../specifications/x86_64/325462-sdm-vol-1-2abcd-3abcd-4-1.pdf).
- Linux 6.19 `security/yama/yama_lsm.c`, `yama_task_prctl`, retained with
  [retrieval provenance](../../specifications/linux/kernel-6.19/security/yama/SOURCE.md).
- Linux 6.19 `kernel/ptrace.c`, `fs/proc/base.c`, `fs/proc/fd.c`,
  `fs/proc/task_mmu.c`, and `mm/memfd.c`, retained under
  [the kernel reference tree](../../specifications/linux/kernel-6.19/).

## Bounded findings

| Impact | Finding | Blocks this workload |
|---|---|---|
| High | The original fix was on another branch, and three successive Linux runtime transport gaps prevented Pin startup; addressed here | Resolved |
| Medium | Mutable remote seal sets and other inode kinds on the linked-process reopen transport return `EOPNOTSUPP`; unrelated processes have no emulated tracing transport | No; SDE uses linked processes and immutable-seal memfds |
| Medium | General concurrent append behavior across independent descriptions is not established by these probes; the existing host-file path also seeks before writing | No; these channels use shared mappings |
| Low | Native/oracle binaries and proprietary SDE files are external inputs, not added to the repository | No |

## Final gates

The complete macOS AArch64 run uses stable Rust 1.98.1 and
`--locked --no-default-features --features x86_64-suite,smir-jit`:

| Gate | Result |
|---|---|
| Complete library binary | 9,992 passed; 2 ignored |
| Complete `user_linux` binary | 42 passed; 1 ignored |
| Complete `x86_64` binary | 28,541 passed; 3 ignored |
| All-target Clippy | Passed |
| Workspace formatting and diff whitespace | Passed |
| Native Linux AArch64 reopened-file tests | 2 passed; nonzero execution, no skips |
| Native Linux and emulated standalone C probes | Both programs passed on both paths |
| Original command, installed normal-release binary | Exit 1, inner SDE argument diagnostic, no SIGSEGV |
| SDE running `hello`, installed normal-release binary | Exit 41; stdout exactly equals direct execution; stderr empty |

Rust test-function counts total 38,575 passes and six explicitly ignored tests.
They do not establish physical x86 execution or KVM coverage on this AArch64
host. The six ignored tests are two full-microkernel lowering/roundtrip tests,
one live Docker fixture oracle requiring binfmt support for all three 64-bit
ISAs, and three stack-fault tests requiring IST setup. They retain their
repository skip conditions; the separate native C probes and Linux cursor
tests above did run. The native Linux run establishes the cursor's real Linux
compilation and execution, separately from the emulated C-probe comparisons. A direct local
cross-check first stopped at the missing `aarch64-linux-gnu-gcc`; the native
container build and tests then completed successfully.

Validation runs use the combined dirty tree; the pre-existing eBPF, SMIR,
workflow, README, and unrelated untracked changes are preserved. No commit,
staging, push, dependency change, or feature-default change is part of this fix.
