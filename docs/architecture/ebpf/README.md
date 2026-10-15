[← Documentation home](../../../README.md)

# eBPF and Solana SBPF architecture

The eBPF path in `rax` implements instruction decoding, canonical disassembly, direct software interpretation (`EbpfCpu`), segmented virtual-memory mapping (`SbpfMemoryMapping`), `VCpu` emulation (`EbpfVcpu`), SMIR lifting (`EbpfLifter`), and stateless oracle analysis across both **IETF RFC 9669 / Linux kernel eBPF (v1–v4)** and **Solana Bytecode Format (`SBPF` V0–V4)**.

## Supported dialects and byte order

`src/isa/ebpf/` and `src/smir/lift/ebpf/` distinguish two primary dialect families:

- **`EbpfDialect::Rfc9669`** — IETF RFC 9669 / Linux kernel v1–v4 (`Endian::Little` and `Endian::Big`), including:
  - 64-bit (`ALU64`) and 32-bit (`ALU32`) arithmetic/logic, `MOVSX` (8/16/32-bit sign-extending moves), `SDIV`/`SMOD` (`offset == 1`), and `BPF_END` (`TO_LE`, `TO_BE`, `BSWAP16/32/64`);
  - 64-bit (`JMP`) and 32-bit (`JMP32`) conditional branches, `JA` (`0x05`), `JA32` (`0x06`), helper calls (`src_reg == 0`), and BPF-to-BPF pseudo calls (`src_reg == 1`);
  - `LDDW` (`0x18`) 128-bit wide constant loads; subtypes 1-6 decode for analysis but execution and lifting explicitly reject them without a platform resolver;
  - unsigned (`LDX_MEM`) and sign-extending (`LDX_MEMSX`) loads, `ST_MEM`, `STX_MEM`, and `BPF_ATOMIC` (`ADD`, `OR`, `AND`, `XOR` with/without `FETCH`, `XCHG`, and `CMPXCHG`).
- **`EbpfDialect::Solana(SbpfVersion::V0..=V4)`** — Solana Bytecode Format across `V0`, `V1`, `V2`, `V3`, and `V4`:
  - **`V0` / `V1`**: 32-bit `ALU32` operations sign-extend their 32-bit result to 64 bits except `MOV32_IMM`, `MOV32_REG`, `LMUL32`, `UDIV32`, and `UREM32`; `R10` is read-only in V0, which supports 4 KiB stack gaps. V1 uses manual frame adjustment via `ADD64_IMM r10, imm`.
  - **`V2`**: Enables `PQR` multiply/divide/remainder instructions (`LMUL32/64`, `UHMUL64`, `SHMUL64`, `UDIV32/64`, `UREM32/64`, `SDIV32/64`, `SREM32/64`), swaps `SUB32_IMM`/`SUB64_IMM` operand order to `dst = imm - dst`, zero-extends `ADD32`/`SUB32`/`MUL32`, replaces `LDDW` with `HOR64_IMM` (`0xF7`), relocates memory load/store opcodes (`0xX2`/`0xX3`/`0xX7`/`0xX8`/`0xXC`/`0xXD`), and permits dynamic stack-frame adjustments via `ADD64_IMM r10, imm`.
  - **`V3` / `V4`**: Enables `JMP32` (`0xX6`) and disables legacy PQR/Bswap opcodes superseded in later `SBPF` revisions.

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
| Program text | `0x1_0000_0000` | Read-only |
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
| `ebpf_smir_lift` | Direct `EbpfCpu` vs `EbpfLifter` + `SmirInterpreter` at `OptLevel::O0` and `OptLevel::O2` across RFC 9669 (LE/BE) and Solana `SBPF` `V0`–`V4` |
| `ebpf_diff` | 4-way differential parity (`RefEvaluator` vs `EbpfCpu` vs `SMIR O0` vs `SMIR O2`) over exhaustive single-instruction matrices, randomized multi-instruction stress programs, and `llvm-mc -triple=bpfel` / `bpfeb` encoding/disassembly comparisons |
| `isa_oracle` | Public JSON decode, SMIR lift, and seeded concrete execution side effects |

Primary specifications and reference sources are stored under [`docs/specifications/ebpf/`](../../specifications/ebpf/PROVENANCE.md).

RFC decoding rejects nonzero unused fields. SBPF decoding preserves the reference
interpreter's ignored-field behavior. Calls preserve r6-r9 and r10; V0 advances r10
by one frame plus an optional gap, V1/V2 leave adjustment to the program, and V3/V4
advance by one contiguous frame. Manual-frame initial r10 defaults to the top of
`stack_frame_size * max_call_depth` backing bytes and may be set explicitly when
the embedding supplies a different stack size.

See [the semantic verification record](verification-2026-10-15.md) for measured
coverage and limitations.

`SbpfMemoryMapping::new_solana_with_config` accepts an explicit rodata address
and an `EbpfConfig`, constructs V0 gaps when enabled, and rejects inconsistent
text/rodata overlap. `new_solana_default` is the V3/V4 convenience layout with
rodata at address zero. `with_regions` remains the raw embedding-controlled map.
For a V0 gapped stack with 4096-byte frames, backing frame i starts at
`MM_STACK_START + i * 8192`; the following 4096 virtual bytes are inaccessible.
