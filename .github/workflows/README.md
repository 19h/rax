# rax CI

A layered validation matrix that spreads rax's suite across as many
GitHub-hosted OS × CPU-architecture combinations as the platform offers, plus a
build sweep over many more ISAs via cross-compilation.

## Workflows

| Workflow | Trigger | What it does |
|---|---|---|
| [`ci.yml`](ci.yml) | push, PR | Fast gate. `rustfmt` + `clippy`, then **build all targets** and run a **core test slice** on every native platform. |
| [`full-suite.yml`](full-suite.yml) | nightly, dispatch | The **entire ~124k-test suite**, sharded by test binary across parallel jobs, on every unix native platform. |
| [`cross.yml`](cross.yml) | push, PR, nightly | **Cross-compile** the core to many CPU architectures (build-only) to guard portability. |
| [`differential.yml`](differential.yml) | nightly, dispatch | Installs the **QEMU/llvm-mc/clang oracles** so the differential harnesses actually diff (they skip otherwise). One job per guest arch. |
| [`kvm.yml`](kvm.yml) | push (kvm paths), nightly | Enables `/dev/kvm` and exercises the **KVM backend** + a release build with default features. |
| [`sanitizers.yml`](sanitizers.yml) | nightly, dispatch | **ASan/UBSan** on a core slice + a **stable/beta/nightly** toolchain sweep. |

## Platform coverage

Native run/build (GA runners, pinned — `macos-latest`/`windows-latest` are mid-migration in 2026):

| OS | x64 | arm64 |
|---|---|---|
| Linux | `ubuntu-24.04` | `ubuntu-24.04-arm` |
| Windows | `windows-2025` | `windows-11-arm` |
| macOS | `macos-15-intel` | `macos-15` |

Cross-compiled (build-only): aarch64 (gnu/musl), armv7, armv6 (`arm`), riscv64gc,
ppc64, ppc64le, s390x, i686, x86_64-musl, x86_64-pc-windows-gnu, and best-effort
tier-3 (loongarch64, powerpc, sparc64, mips64/mips64el/mipsel, wasm32).

## Design notes

- **Feature gating is per-platform and deliberate.** `kvm` is Linux/x86-only and
  needs `/dev/kvm`; `smir-jit` pulls a `libc`-backed W^X runtime so it is enabled
  on Linux/macOS but **not Windows**; `x86_64-suite` is always on so the gated
  x86_64 test binaries compile. The portable lanes use
  `--no-default-features --features x86_64-suite[,smir-jit]`.
- **Why `cargo test`, not `cargo nextest`.** nextest runs each test in its own
  process; at ~124k tiny tests the spawn overhead dominates. Plain `cargo test`
  runs tests as in-process threads, so we shard coarsely by selecting subsets of
  the ~34 integration-test binaries with repeated `--test` flags. The genuinely
  heavy *compiled* test binaries — `arm_diff` (~1 MB), `differential`, `diff_fuzz`,
  the EVEX diff — live in the `differential.yml` / `kvm.yml` lanes. Note that
  `neon_gen.rs` / `sve2_gen.rs` / `arm32_gen.rs` are **data tables** (`pub static
  ..._SWEEP`) `include!`d by `arm_diff*.rs`, not standalone tests, so they are
  never selected with `--test`.
- **Test builds stay on the dev profile.** The release profile's `lto = "fat"` /
  `codegen-units = 1` would blow compile time and RAM on the giant generated
  tables. The shared setup also drops debuginfo (`CARGO_PROFILE_*_DEBUG=0`).
- **Oracles skip gracefully.** The differential harnesses probe for
  `qemu-<arch>`, `llvm-mc`, `clang`/`cc`, etc. and skip when absent — that keeps
  `ci.yml` green without them; `differential.yml` installs them so the diffs run.
- **Shared setup** lives in [`../actions/setup-rust`](../actions/setup-rust):
  toolchain install + `Swatinem/rust-cache` + CI build defaults.
