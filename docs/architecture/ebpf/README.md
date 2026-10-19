[← Documentation home](../../../README.md)

# eBPF and Solana SBPF architecture

The eBPF path in `rax` implements instruction decoding, canonical disassembly, direct software interpretation (`EbpfCpu`), segmented virtual-memory mapping (`SbpfMemoryMapping`), `VCpu` emulation (`EbpfVcpu`), SMIR lifting (`EbpfLifter`), and stateless oracle analysis across both **IETF RFC 9669 / Linux kernel eBPF (v1–v4)** and **Solana Bytecode Format (`SBPF` V0–V4)**.

## Supported dialects and byte order

`src/isa/ebpf/` and `src/smir/lift/ebpf/` distinguish two primary dialect families:

- **`EbpfDialect::Rfc9669`** — IETF RFC 9669 / Linux kernel v1–v4 (`Endian::Little` and `Endian::Big`), including:
  - 64-bit (`ALU64`) and 32-bit (`ALU32`) arithmetic/logic, `MOVSX` (8/16/32-bit sign-extending moves), `SDIV`/`SMOD` (`offset == 1`), and `BPF_END` (`TO_LE`, `TO_BE`, `BSWAP16/32/64`);
  - 64-bit (`JMP`) and 32-bit (`JMP32`) conditional branches, `JA` (`0x05`), `JA32` (`0x06`), static helper calls (`src_reg == 0`), BPF-to-BPF pseudo calls (`src_reg == 1`), and helper calls by BTF ID (`src_reg == 2`), which run a handler registered with `EbpfCpu::register_btf_helper` or exit to the host (`EbpfExit::BtfHelper`; in SMIR an `ExitReason::Syscall` whose key `EbpfHelperKey::from_smir` decodes as `Btf`) in a namespace separate from static IDs;
  - `LDDW` (`0x18`) 128-bit wide loads with every RFC 9669 Table 12 subtype: the constant (0x0), `code_addr` (0x4, the instruction `imm + 1` slots after the LDDW, as the Linux verifier resolves it), and the platform objects bound in `EbpfConfig::platform` / `EbpfLifter::platform` (`EbpfPlatform`: `map_by_fd` 0x1, `map_val` + signed `next_imm` 0x2, `var_addr` 0x3, `map_by_idx` 0x5, 0x6); an unbound object traps with `UnsupportedPseudoLoad` before any state change;
  - unsigned (`LDX_MEM`) and sign-extending (`LDX_MEMSX`) loads, `ST_MEM`, `STX_MEM`, and `BPF_ATOMIC` (`ADD`, `OR`, `AND`, `XOR` with/without `FETCH`, `XCHG`, and `CMPXCHG`).
- **`EbpfDialect::Solana(SbpfVersion::V0..=V4)`** — Solana Bytecode Format across `V0`, `V1`, `V2`, `V3`, and `V4`:
  - **`V0` / `V1`**: `ADD32`, `SUB32`, and `MUL32` sign-extend their 32-bit result to 64 bits; every other 32-bit result (`MOV32`, bitwise, shift, `NEG32`, `DIV32`, `MOD32`) is zero-extended. `R10` is read-only in V0, which supports 4 KiB stack gaps. V1 uses manual frame adjustment via `ADD64_IMM r10, imm`.
  - **`V2`**: Enables `PQR` multiply/divide/remainder instructions (`LMUL32/64`, `UHMUL64`, `SHMUL64`, `UDIV32/64`, `UREM32/64`, `SDIV32/64`, `SREM32/64`) in place of `MUL`/`DIV`/`MOD`, swaps `SUB32_IMM`/`SUB64_IMM` operand order to `dst = imm - dst`, zero-extends `ADD32`/`SUB32`, sign-extends `MOV32_REG`, removes `NEG` and `LE`, replaces `LDDW` with `HOR64_IMM` (`0xF7`), relocates memory loads to `0xXC`, immediate stores to `0xX7`, and register stores to `0xXF`, and permits dynamic stack-frame adjustments via `ADD64_IMM r10, imm`.
  - **`V3` / `V4`**: Restore the V0 arithmetic, `LDDW`, `NEG`, `LE`, and memory classes; enable `JMP32` (`0xX6`), static syscalls (`CALL_IMM` `src` 0 vs PC-relative `src` 1), `CALLX` through `dst`, and segment-zero rodata.

## Architectural state and memory model

The exported state (`EbpfRegisters` / `EbpfCpuState`) consists of:

- `R0`–`R9` general-purpose 64-bit registers (`R0` return value, `R1`–`R5` call arguments, `R6`–`R9` callee-saved);
- `R10` frame/stack pointer (read-only in RFC 9669 and `SBPF` `V0`/`V3`/`V4`; adjustable by `ADD64_IMM` only in `SBPF` `V1`/`V2`);
- `PC` byte address (8 bytes per slot; `LDDW` occupies 2 slots);
- bounded internal call stack for BPF-to-BPF / SBPF internal calls;
- compute-unit / instruction-meter budget.

For Solana workloads, `SbpfMemoryMapping` (`src/isa/ebpf/memory.rs`) models the 4 GiB segmented virtual address layout:

| Segment | Base address | Access |
|---|---|---|
| Program text | `0x1_0000_0000` | Executable fetch; ordinary data reads require a data-region mapping |
| RODATA | Executable address in segment 1 (V0-V2), segment 0 (V3/V4) | Read-only |
| Stack | `0x2_0000_0000` | Read-write (`V0` optional 4 KiB frame gaps; `V1`-`V4` contiguous) |
| Heap | `0x3_0000_0000` | Read-write |
| Input / Accounts | `0x4_0000_0000` | Read-write or read-only per region |

## Stateless oracle (`rax_isa_oracle`)

The static oracle binary and library (`src/oracle/`) accept `--arch ebpf` or `--arch sbpf` with mode flags:

- Endianness: `le` (default), `be`
- Dialect: `rfc9669` (default for `ebpf`), `sbpf-v0`, `sbpf-v1`, `sbpf-v2`, `sbpf-v3` (default for `sbpf`), `sbpf-v4`

```sh
cargo run --bin rax_isa_oracle -- --arch sbpf --mode le,sbpf-v2 --seeded-effects 36120000000000009500000000000000
```

## Verification targets

| Cargo target | Scope |
|---|---|
| `ebpf_smir_lift` | Direct `EbpfCpu` vs `EbpfLifter` + `SmirInterpreter` at O0/O1/O2 across RFC 9669 (LE/BE) and Solana `SBPF` `V0`–`V4`, including precise trap and meter parity |
| `ebpf_diff` | Replays of the checked-in oracle corpora on every plane: the eBPF Foundation `bpf_conformance` suite (312 tests, 45 negatives), the Linux kernel (147,698 cases each on the arm64 JIT and the big-endian s390x interpreter), and the pinned Anza `solana-sbpf` interpreter (V0–V4); plus a specification-derived evaluator and `llvm-mc` encodings |
| `isa_oracle` | Public JSON decode, SMIR lift, and seeded concrete execution side effects |

The live oracles run outside the engine workspace:
`python3 tools/ebpf-conformance/run.py` (conformance suite and both kernels; see
its [README](../../../tools/ebpf-conformance/README.md)) and
`python3 tools/ebpf-reference/run.py` (Anza interpreter and verifier). With
`--export` / `RAX_SBPF_CORPUS_OUT=1` they regenerate `tests/generated/ebpf/`.
`python3 tools/ebpf-formal/run.py` runs the Kani proofs, over all field,
register, and memory values: RFC 9669 admission and direct execution of every
opcode against an independent specification, and SBPF admission of every
opcode against the pinned Anza verifier. The SMIR and SBPF-execution harnesses
are defined but not discharged (see its
[README](../../../tools/ebpf-formal/README.md)).

## Conformance claims

RFC 9669 conformance groups `base32`, `base64`, `atomic32`, `atomic64`,
`divmul32`, and `divmul64` are implemented in both byte orders. The deprecated
legacy `packet` group (`BPF_ABS`/`BPF_IND`) and the non-RFC uBPF `callx` group
are rejected (RFC 9669 section 2.4 makes every group but `base32` optional;
section 5.5 defines no semantics for `packet`). The RFC's platform objects
are bound by the embedder: maps and platform variables for LDDW subtypes
(`EbpfPlatform`), and helper functions, by static ID or BTF ID, as handlers
or host exits; an unbound object traps precisely. `r10` is the read-only
frame pointer, a platform contract (RFC 9669 section 6 puts verification out
of scope). Solana admission is `verify_sbpf`, equal to the pinned
`RequisiteVerifier`; syscalls charge compute units through the host exit
(`insn_count`), as the `metered-syscall` corpus checks against Anza. ELF
loading and relocation are not part of the engine.
See [the conformance record](verification-2026-10-16.md).

Primary specifications and reference sources are stored under [`docs/specifications/ebpf/`](../../specifications/ebpf/PROVENANCE.md).

RFC decoding rejects nonzero unused fields. SBPF decoding preserves the reference
interpreter's ignored-field behavior. Calls preserve r6-r9 and r10; V0 advances r10
by one frame plus an optional gap, V1/V2 leave adjustment to the program, and V3/V4
advance by one contiguous frame. Manual-frame initial r10 defaults to the top of
`stack_frame_size * max_call_depth` backing bytes and may be set explicitly when
the embedding supplies a different stack size.

See [the semantic verification record](verification-2026-10-15.md) and
[the conformance record](verification-2026-10-16.md) for measured coverage and
limitations.

`SbpfMemoryMapping::new_solana_with_config` accepts an explicit rodata address
and an `EbpfConfig`, constructs V0 gaps when enabled, and rejects inconsistent
text/rodata overlap. `new_solana_default` is the V3/V4 convenience layout with
rodata at address zero. `with_regions` remains the raw embedding-controlled map.
For a V0 gapped stack with 4096-byte frames, backing frame i starts at
`MM_STACK_START + i * 8192`; accesses starting in the following 4096 virtual bytes fault. The reference
translation permits an access starting in a mapped frame to extend into the
next contiguous backing bytes, even across its virtual gap.

SMIR execution carries an observable `EbpfInstruction` boundary for every guest
instruction, including branches and EXIT. Optimization retains these boundaries;
instruction limits, fault PCs, and resumed execution use guest instructions rather
than IR operation counts. `SmirContext::new_ebpf_with_config` selects nondefault
stack/depth/text policies. `SmirInterpreter::add_function` installs its blocks.
`EbpfLifter::register_function` supplies legacy SBPF keys and possible CALLX
entries; unknown indirect targets remain explicit BlockNotFound frontiers until
the embedding registers/lifts their code (reusing the same `LiftContext`, so block
identifiers stay unique). `EbpfLifter::lift_program` discovers the reachable program as distinct functions.
A reachable instruction that cannot be lifted (unreadable slot, malformed encoding,
unbound pseudo-load) becomes a guest marker and a `TrapKind::Ebpf` terminator that
raises the direct CPU's exact trap only if execution reaches it.
`lift_function` follows intraprocedural successors; each `guest_range` describes
that function's minimum-to-maximum decoded byte extent and excludes its callees.
It does not assert that every byte inside a sparse range is an instruction.

`EbpfMemory::atomic` is the indivisible transaction contract; byte-only custom
backends reject it by default. Private FlatMemory and segmented mappings execute
under exclusive mutable ownership. Shared GuestMemoryMmap transactions use checked
32/64-bit host atomics and serialize overlapping mixed-width accesses. Host atomic
users outside this interface must use a consistent width for an object. SMIR guest
boundary metadata selects atomic byte order; big-endian arithmetic RMW uses a CAS
loop, and compare-exchange converts both operands and the returned previous value.
Failed comparisons still require writable memory. Shared host atomics require
natural host alignment and reject unsupported/misaligned accesses before updates.

For legacy keys present in both loader and internal registries, call
`EbpfLifter::register_syscall` as well as `register_function`. Execution exits for
the host syscall, then resumes the deferred internal call without charging a
second guest instruction. The direct CPU uses the same external-then-internal
resolution order. Single stepping and breakpoints stop at guest boundaries;
SBPF division exits retain DivideByZero/DivideOverflow and the precise fault PC.

SBPF executable text is separate from guest data permissions. In V3/V4 it is
fetched at `MM_BYTECODE_START` without exposing a readable data alias there;
rodata remains in segment zero. `MemoryReader` on an executable mapping reads
code, while `EbpfMemory`/`SmirMemory` loads read the data map. Raw `with_regions`
mappings retain an embedding-controlled unified address space.

For independent execution against the pinned Anza interpreter, run
`python3 tools/ebpf-reference/run.py`; see its [README](../../../tools/ebpf-reference/README.md).
