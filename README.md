<h1 align="center">
    <img width="128" src="./assets/ida.png" alt="rax" /><br>
    rax
</h1>

<h5 align="center">
rax is a CPU emulator that checks its own work. It implements four instruction sets in software<br/>
(x86-64, AArch64, Hexagon, and RISC-V) and validates each instruction against an authoritative<br/>
reference: real silicon (KVM) for x86-64, and QEMU for the rest. The x86-64 core boots Linux, and<br/>
a shared IR (SMIR) JITs hot loops to native code.
</h5>

<div align="center"><code>Rust</code> • <code>x86-64 · AArch64+SVE · Hexagon+HVX · RV64GC</code> • <code>boots Linux</code> • <code>hot-block JIT</code> • <code>oracle-verified</code></div>

<br/>

rax is a research project, not an official Hex-Rays product. It is already useful in practice: point
IDA Pro's GDB debugger at rax's built-in GDB stub (the `--gdb` option) and you can debug a full kernel
running under the emulator.

---

## Quick start

Build it, then run a Linux kernel, on hardware or one instruction at a time:

```bash
cargo build --release

# 1. Boot a Linux kernel on hardware virtualization (Linux + KVM).
./target/release/rax --kernel bzImage --initrd initrd.img

# 2. Boot it on the software CPU instead: slower, but every instruction is observable, and it
#    boots to a BusyBox shell. (Use an ELF vmlinux; bzImage real-mode boot is still in progress.)
./target/release/rax --backend emulator --kernel vmlinux --initrd initrd.cpio

# 3. Trace every instruction the kernel executes, in an SDE-compatible format.
./target/release/rax --backend emulator --kernel vmlinux --trace boot.trace

# 4. Boot AArch64 Linux: the architecture is read from the kernel image, and a DTB with
#    GICv3, PL011, generic timer, and PSCI is generated on the fly. Works on any host.
./target/release/rax --kernel linux-aarch64/Image --initrd initramfs.cpio

# 5. Boot something that isn't Linux: a bootable ISO, through the real-mode mini-BIOS.
#    rax boots TempleOS V5.03 from real mode to its 64-bit HolyC shell (El-Torito + ATAPI CD-ROM).
./target/release/rax --backend emulator --kernel TempleOS.ISO --memory 512M
```

RISC-V and Hexagon run bare-metal programs through the emulator backend (UART and halt):

```bash
./target/release/rax --arch riscv64 --backend emulator --kernel program.elf
```

Every core is checked against a reference oracle. A harness runs an instruction on both rax and the
reference from an identical state, then diffs the result:

```bash
cargo test --release --test differential       # x86-64  vs. KVM (the silicon)
cargo test --release --test arm_diff           # AArch64 vs. native EL0 hardware on aarch64, qemu-aarch64 elsewhere
cargo test --release --test hexagon_hvx_diff   # Hexagon vs. qemu-hexagon
cargo test --release --test riscv_diff         # RV64GC  vs. qemu-riscv64
```

> Note: every oracle harness self-skips cleanly if the cross-compiler, QEMU, or `/dev/kvm` is absent,
> so the suite is green on any host. A harness fails only when rax and the reference genuinely disagree.

---

## Why it exists

If you have ever wondered what actually happens between launching a kernel and seeing a shell, most
tools give you one of two unsatisfying answers. A real hypervisor (QEMU/KVM) runs the kernel so fast
you cannot watch it; the CPU is a black box. A pure emulator (Bochs, Unicorn) lets you watch, but its
instruction coverage trails the hardware by years, and there is no easy way to tell whether what it did
matches what a real chip would have done.

rax is built around the second problem. A software CPU is only useful if its behavior is correct, and
the most direct way to establish that is to compare it, instruction by instruction, against an
authoritative reference:

- **x86-64** is checked against **KVM**, the silicon in your machine. The same machine code runs on the
  interpreter and on hardware from an identical architectural state, and the final state is diffed; the
  chip itself defines the expected result.
- **AArch64** is checked against a native **EL0 hardware oracle** on aarch64 hosts: a small signal-frame
  harness loads a state, runs one instruction directly on the CPU, and reports back. On non-aarch64
  hosts the same harness falls back to **qemu-aarch64** user mode. Hexagon and RISC-V use the same
  user-mode oracle pattern through QEMU.
- **Intel APX** is the exception with no chip to ask: no shipping CPU implements it, and QEMU does not
  emulate it, so neither can be the oracle. Its encodings come from **LLVM**, the only assembler that
  speaks APX; each test pins an instruction to LLVM's exact bytes and checks rax's architectural effect
  against the documented semantics.

That approach keeps rax both legible (open `src/isa/x86_64/execute/arith/add.rs` and read exactly what `ADD` does to the
flags) and trustworthy (a regression suite stands between any change and the behavior it might break).
It is also what makes the JIT possible: a native code generator is only safe if you can prove its
output matches the interpreter, and the same oracle provides that proof.

Correctness is the main reason to build this rather than reach for QEMU, but not the only one. rax is
MIT-licensed, while QEMU and Unicorn (the usual emulation engines) are GPL, which makes them awkward to
embed in a binary-analysis product; a permissively licensed, well-tested emulator can ship alongside or
inside reverse-engineering tools without GPL obligations, which benefits the whole ecosystem. And it is
written in Rust, for memory safety and maintainability, rather than C.

---

## What a run looks like

A throughput benchmark of the x86-64 interpreter's hot path (`examples/bench_loop.rs`, a tight
register-only guest loop) reports sustained MIPS:

```text
$ RUSTFLAGS="-C target-cpu=native" cargo run --release --example bench_loop
[bench] iterations    : 268435456 (0x10000000)
[bench] expected insns: 1342177283
[bench] executed insns: 1342177283
[bench] elapsed       : 9.26 s
[bench] throughput    : ~145 MIPS
```

With the JIT (on by default), that loop is detected as hot, lifted to SMIR, lowered to native host code,
and run directly. On an x86-64 host it runs at roughly 80x the interpreter, bit-identical to
interpreting it. vCPU-level tests assert register-for-register equality on x86-64 and AArch64 hosts;
`RAX_JIT_VERIFY=1` also re-checks every compiled region at runtime on x86-64.

Boot a kernel under the emulator with `--trace` and every retired instruction lands in an
SDE-compatible trace file: the instruction, its register changes, and (where they happen) memory
accesses and XMM updates, diffable against Intel's own Software Development Emulator:

```text
$ ./target/release/rax --backend emulator --kernel vmlinux --trace boot.trace
...
$ head -4 boot.trace
INS 0x00000000010000f0   xor eax, eax                                       | eax=0x00000000
INS 0x00000000010000f2   mov ecx, 0x80000000                                | ecx=0x80000000
INS 0x00000000010000f7   mov cr0, eax                                       | cr0=0x80000011
Write *(UINT64*)0x9000 = 0x000000000000a003
```

> Note: the trace, GDB stub, snapshot facility, and per-mnemonic profiler all hook the interpreter's
> step loop, so they observe the genuine instruction stream rather than a re-derived approximation. The
> KVM backend traps only on I/O, so it is fast but opaque by design.

---

## Four CPUs

x86-64 is the complete VM target: it boots Linux, with the full device platform, boot protocol, tracing,
GDB, snapshots, and the JIT. AArch64 boots Linux too (see "How the machines work"). Hexagon and RISC-V
are bootable emulator backends for bare-metal programs. All four also have SMIR lifters.

| Core | Runs | Coverage | Oracle |
|------|------|----------|--------|
| **x86-64** | boots Linux (KVM/HVF/emulator) + JIT | Legacy → SSE/AVX/AVX2 → AVX-512 → AVX10.1/10.2 → APX; x87; AES/SHA/GFNI; XSAVE | KVM (real hardware) |
| **AArch64 / ARM** | boots Linux (emulator + HVF on Apple Silicon) | A64 base, complete SVE/SVE2/SVE2.1, NEON/VFP, FP16; AArch32/Thumb; Cortex-M (M0-M85) | EL0 hardware / qemu-aarch64 + ASL |
| **Hexagon** | bare-metal (`--arch hexagon`) | V73 scalar + VLIW packets + HVX, every opcode verified | qemu-hexagon |
| **RISC-V** | bare-metal (`--arch riscv64`) | RVA23 scalar set (RV64GC + Zfh/Zicond/Zfa/Zbk\*/Zcb + scalar crypto) + RVV 1.0 | qemu-riscv64 |

### x86-64

The most complete target. The decoder handles the full x86-64 encoding space (REX and
REX2, every legacy prefix including the `0x67` address-size override, ModR/M + SIB, VEX2/VEX3, EVEX
including APX Map 4, RIP-relative), dispatching to per-category implementations under
`src/isa/x86_64/execute/`.

| Category | Coverage |
|----------|----------|
| **Integer / logic / bit** | full ALU, ADCX/ADOX, BT/BTS/BTR/BTC, BSF/BSR, POPCNT/LZCNT/TZCNT; `#DE` on ÷0/overflow |
| **Shifts / strings / BCD** | SHL…RCR, SHLD/SHRD; REP MOVS/STOS/SCAS/CMPS (bulk fast path); DAA…AAD |
| **x87 FPU** | escape codes D8-DF via f64 |
| **SSE → SSE4 / AVX / AVX2** | moves, arithmetic, all compare predicates, shuffle/permute/convert (XMM/YMM) |
| **FMA / BMI1 / BMI2** | VFMADD/SUB/NMADD/NSUB {132,213,231}; ANDN, BZHI, PEXT, PDEP, MULX, … |
| **AVX-512** | F / VL / BW / DQ / CD plus FP16, VBMI, VBMI2, IFMA, VNNI, BITALG, VPOPCNTDQ, BF16, VP2INTERSECT; EVEX forms of VAES / VPCLMULQDQ / GFNI; FP math (VGETEXP/VGETMANT, VRCP/VRSQRT14, VRNDSCALE, VREDUCE, VSCALEF, VRANGE, VFIXUPIMM); MAP5/MAP6 FP16 FMA; VSIB gather/scatter; masked ops, opmask k0-k7 |
| **AVX10.1 / 10.2** | VNNI, IFMA, VPOPCNTDQ, VBMI, BF16; VMPSADBW, VMINMAX, saturating converts |
| **APX** | REX2, EGPRs R16-R31, NDD (3-operand), NF (no-flags), CCMP/CTEST, SETZUcc, PUSH2, JMPABS, MOVBE, MUL/DIV, EVEX Map 4 |
| **Crypto / state / system** | AES, SHA1/256, GFNI (FIPS/SDM known-answer tested); XSAVE/XRSTOR/XCR0; CPUID, MSRs, CR/DR, descriptor-table loads, CPL-checked, canonical-address checks, faults injected (`#UD`/`#GP`) |

### AArch64 / ARM

A broad ISA surface, and a runnable backend that boots Linux.

- **SVE** is complete: no valid SVE encoding is unhandled. The full data-processing set (predicate
  generation/logical, predicated integer and FP ALU, reductions, permutes, CPY/SEL/CMP, shifts) and the
  entire memory subsystem (contiguous, gather/scatter, LD2-4/ST2-4, first-fault + FFR), all at VL=128.
- **SVE2 and SVE2.1** are complete on the register surface, every encoding bit-exact against the
  qemu-aarch64 oracle: long/wide/narrow and saturating integer ops, complex arithmetic, bit-permute,
  pairwise, BF16 (B16B16, dot product), quadword reductions, and PMOV/PSEL/PEXT. An `llvm-mc` sweep
  (`tests/generated/arm/oracle_cases/sve2_sweep.rs`) guards it; only multi-vector memory, SME, and FEAT_LUT remain, beyond reach of a
  register-only oracle.
- **NEON / VFP**: full Advanced SIMD and scalar FP including FP16, bit-exact against the oracle on a
  generated sweep (`tests/generated/arm/oracle_cases/neon_sweep.rs`); full crypto (AES, SHA1/256/512, SHA3, SM3, SM4).
- **Modern A64 extensions**: MTE (memory tagging), PAuth (pointer authentication), FlagM, LRCPC
  (release-consistency atomics), and FP8 (FP8FMA).
- **AArch32 / Thumb / Cortex-M (M0-M85)**: the A32 and Thumb (T16/T32) integer ISA is bit-exact against
  a qemu-arm oracle on a generated encoding sweep (`tests/generated/arm/oracle_cases/aarch32_sweep.rs`,
  run by `tests/suites/differential/arm/aarch32.rs`), with VFP and NEON execution and hardware exception routing.
  Cortex-M adds NVIC/SysTick/SCB/MPU across ARMv6-M to v8.1-M, and an ARMv6 core (CP15 + MMU) drives two
  emulated SoC machines: the S3C64xx, and the S5L8900 (the original iPhone / iPod Touch 1G). The S5L8900
  boots Apple's **iBoot** from real device firmware (bootrom + LLB + NOR) and runs on into early iOS XNU
  kernel bringup; its platform set spans a PL192 VIC pair, SYSIC/GPIO, timer, I2C (PMU, RTC,
  accelerometer), UART, an AES engine, NAND with ECC, PL080 DMA and the Apple Data Mover, SPI (LCD panel
  and multitouch), an LCD controller, USB OTG, and NOR flash (`RAX_MACHINE=s5l8900`).

### Hexagon

A Qualcomm Hexagon (V73) implementation where every instruction opcode is decoded, executed, and
differentially verified against qemu-hexagon at zero divergence: the scalar core, the full HVX vector
set, plus CABAC bin decode, scatter/gather, and FP reciprocal/sqrt seed tables recovered from the
oracle.

- **VLIW packets** carry true parallel-packet semantics: all instructions read the old register file and
  commit atomically at packet end; `.new` value forwarding for scalars and HVX vectors; duplex
  encodings; hardware loops (SA0/LC0, SA1/LC1) with circular and bit-reversed addressing; dual stores.
- **HVX** provides 1024-bit vectors V0-V31 and predicates Q0-Q3: ALU, compare, min/max, the full
  multiply family, permute, shift, round/saturate, LUT, histogram, vector-predicate ops, `vmem`
  loads/stores (`.cur`/`.tmp`, scalar-predicated), and V65 scatter/gather.

### RISC-V

An **RV64GC** core wired into the VMM as a real `--arch riscv64 --backend emulator` target: it loads an
ELF, drives a 16550 UART over MMIO, and halts on `ecall`. Coverage is the entire RVA23 scalar set:
RV64I/M/A/F/D, **Zfh** half-precision, C (compressed), Zicsr/Zifencei, Zba/Zbb/Zbc/Zbs, Zicond, Zfa,
Zbkb/Zbkx/Zcb, the scalar crypto suite (Zknh SHA-256/512, Zksh SM3, Zksed SM4, Zkne/Zknd AES), and the
full **RVV 1.0** vector extension — arithmetic, fixed-point, FP, reductions, permutes, conversions, and
every load/store mode, at VLEN=128. The floating-point core computes the
round-to-nearest result, recovers the exact residual (2Sum / FMA / Newton), and uses it to deliver
correctly-rounded answers in all five rounding modes with all five IEEE flags. It is checked against
qemu-riscv64 by fuzzers that exercise the whole non-control-flow opcode space, and by a dedicated RVV
harness that diffs the full vector register file and `vl`/`vtype` case by case. Its opt-in SMIR
execution tier lowers `step_jit` as one instruction and `run_jit` as cache-keyed straight-line regions
of up to 16 instructions to state-backed native blocks on both x86-64 and AArch64 hosts. Regions stop
at memory, control-flow, fence, and replay-sensitive boundaries; cache identity covers every 16- or
32-bit encoding in the region, and a faulting final memory operation retires only its completed prefix.
Scalar integer, memory, control flow, A/Zacas atomics (including AMOCAS.Q), integer crypto, and scalar
floating point execute natively; unsupported boundaries remain on the interpreter path without
speculative guest-state commits.

---

## Correctness: every architecture has an oracle

Correctness verification is central here: each instruction is not just implemented but checked against
an authoritative reference. A harness builds an initial architectural state, runs one instruction (or a
short sequence) on both rax and the reference from that identical state, then diffs the full register
file (and, for x86, a scratch memory page). Inputs are enumerated over encoding fields and driven with
pseudo-random states, so each test function exercises many cases.

| Harness | rax core | Oracle | Compares |
|---------|----------|--------|----------|
| `tests/suites/differential/x86_64/kvm.rs` | x86-64 | KVM (hardware) | GPRs, RIP, RFLAGS, XMM, memory |
| `tests/suites/differential/x86_64/kvm_avx512.rs` | x86-64: full EVEX state (AVX-512) | KVM (hardware) | 32 ZMM, opmasks k0-k7, RFLAGS, scratch memory |
| `tests/suites/differential/x86_64/qemu_evex.rs` | x86-64: generated EVEX SIMD corpus | `qemu-x86_64` | 32 ZMM, opmasks k0-k7, RFLAGS, scratch memory |
| `tests/suites/differential/x86_64/qemu_apx.rs` | x86-64: generated APX Map 4 corpus | `qemu-x86_64` | GPRs, RFLAGS, memory, stack (self-skips until QEMU speaks APX) |
| `tests/suites/differential/x86_64/qemu_unimplemented.rs` | x86-64: unimplemented-mnemonic manifest | `qemu-x86_64` | rax must reject each case the oracle executes |
| `tests/suites/differential/arm/aarch64.rs` | AArch64: NEON + SVE/SVE2/SVE2.1 | native EL0 hardware on aarch64; `qemu-aarch64` fallback | X0-X30, SP, NZCV, V0-V31, P0-P15 |
| `tests/suites/differential/arm/aarch32.rs` | AArch32: A32 + Thumb T16/T32 | `qemu-arm` | R0-R14, CPSR, FPSCR, D0-D31, scratch |
| `tests/suites/differential/hexagon/*.rs` | Hexagon (scalar / control flow / float / memory / HVX / HVX-memory) | `qemu-hexagon` | GPRs, P3:0, USR, loop regs, V0-V31, Q0-Q3 |
| `tests/suites/differential/riscv/scalar.rs` | RV64GC | `qemu-riscv64` | x1-x31, f0-f31, fcsr, scratch |
| `tests/suites/differential/riscv/vector.rs` | RVV 1.0 (vector data path) | `qemu-riscv64` | v0-v31, vl/vtype, x/f registers, fcsr/vcsr, scratch |
| `tests/suites/differential/x86_64/fuzz.rs` | x86-64 interp (randomized encodings) + SMIR mirror (lift → interp / native) | KVM | GPRs, masked RFLAGS, scratch memory / XMM |
| `tests/suites/smir/lift/riscv.rs` | RISC-V → SMIR lift | rax RISC-V interp | x/f/v/fcsr (incl. RVV, zero divergence) |
| `tests/suites/smir/jit/riscv_{x86_64,aarch64}.rs` | RISC-V → SMIR → native host JIT | rax RISC-V interp | x/f/PC/CSR/memory, native/fallback counters |
| `tests/suites/smir/lift/hexagon.rs` | Hexagon → SMIR lift | rax Hexagon interp | R/P/USR/V/Q (entire ISA: scalar + HVX) |
| `tests/suites/smir/lower/aarch64_native.rs` | AArch64 → SMIR lift + native ARM64 lower | rax AArch64 interp | X0-X30, V0-V31, memory (scalar int/FP + NEON) |
| `tests/suites/smir/jit/x86_64.rs` | x86 vCPU → native x86-64 JIT | rax x86 interpreter | registers, flags, memory, throughput |
| `tests/suites/smir/jit/x86_64_aarch64.rs` | x86 vCPU → native AArch64 JIT | rax x86 interpreter | RAX-R15, RIP, complete RFLAGS, hot-loop promotion/cache |

> Note: the reference harnesses are small C/asm programs (`tools/{arm,riscv,hexagon}-diff/`) that QEMU
> runs as ground truth; for x86 the ground truth is KVM, with `qemu-x86_64` covering the generated
> EVEX corpus where the silicon lacks the extension (the APX harness is staged, but self-skips until
> QEMU gains APX support).

### Generated suites

Much of the per-instruction coverage is generated rather than hand-written. The ARM suite is emitted from ARM's
machine-readable ASL specification by `tools/asl-parser/`; the x86-64 AVX-512 / EVEX suite is generated
the same way from a checked-in Intel Intrinsics Guide corpus, which also tracks which SIMD mnemonics
remain unimplemented (behind `--features x86_64-suite`). The rest covers the oracle and SMIR-lift
harnesses, real-mode/ISO boot, Hexagon bare-metal, RISC-V boot, and FIPS/SDM crypto known-answer tests.

---

## SMIR and the hot-block JIT

**SMIR** (Sigma Machine IR, `src/smir/`; spec under
[`docs/specifications/smir/`](docs/specifications/smir/)) is the shared layer across the four guest
architectures. Each architecture has a *lifter* that translates its instructions into a common typed
operation set; the IR is interpreted directly, optimized, and lowered to native machine code on both
x86-64 and ARM64 hosts.

```text
  ┌─────────────────────────────────────────────────┐
  │   x86-64 · AArch64 · Hexagon · RISC-V · AVX10   │   per-arch lifters
  └────────────────────────┬────────────────────────┘
                           ▼
  ┌─────────────────────────────────────────────────┐
  │                   SMIR Module                   │   SmirFunction → SmirBlock → SmirOp
  └────────────────────────┬────────────────────────┘
         ┌─────────────────┼─────────────────┐
         ▼                 ▼                 ▼
     Interpreter       optimizer        JIT → x86-64 / ARM64
     (lazy flags)      (O0/O1/O2)       (emit · regalloc · W^X)
```

The native JIT is integrated and on by default. The run loop detects hot loops (a back-edge counter
promotes a region once it crosses a threshold), lifts the region to SMIR, runs the O2 optimizer over it,
lowers it to native code, caches the compiled block, and runs it through a W^X `mmap` trampoline. On the
x86-64-host bench loop the lowered body is one native instruction per guest instruction, roughly 80x
the interpreter and bit-identical to it.

Safety comes from a **fail-safe gate**: a region compiles only from operations proven equal to the
reference, and anything else makes it bail back to the interpreter, so native code runs only for regions
known to be correct. On x86-64 hosts the gate covers the scalar integer core (the ALU with its carry
chains through ADC/SBB and ADCX/ADOX, shifts and rotates including RCL/RCR and SHLD/SHRD, multiply
including MULX, mov/extend, CWD/CDQ/CQO, LEA, XCHG, BSWAP, bit scans and counts, the BMI sets, the APX
NDD/NF forms, setcc/cmov, branches), memory, and a whitelisted vector tier — masked ternary logic,
VNNI, IFMA, BF16/FP16 arithmetic, population count, conflict detection, compress/expand, byte permutes,
integer narrowing, packed shifts and rotates, and the AES, SHA-512, SM3, and SM4 vector crypto
families — each op gated on the host CPU's own features and run under the guest's MXCSR. FS/GS
segment-relative loads and stores lower to MMU helper calls that bail cleanly on a page fault or a
write to a code page, and a guest CALL does not end a region: the call site lowers to an interpreter
call-out that runs the callee and resumes the native body, so call-heavy loops stay native between
callees (`RAX_JIT_NO_CALL=1` makes a CALL end the region again; `RAX_JIT_NO_MEM=1` alongside it
restores the register-only JIT). On AArch64 hosts, the x86 vCPU currently admits register-only scalar regions
over RAX-R15. It bridges CF/ZF/SF/OF to PSTATE.NZCV and preserves the remaining RFLAGS bits. It rejects
unrepresentable flag contracts, memory, FP/SIMD, APX extended registers, and virtual temporaries.
Still interpreter-only on both hosts are locked/RMW operations, plus the double-width DIV the IR cannot
yet model.
Self-modifying code evicts compiled blocks via the MMU's dirty-page journal, persistently-ineligible
region heads are memoized so SMC-heavy guests like TempleOS do not thrash the compiler, and a
frontier-less spin loop is refused so native code cannot trap the vCPU.

The lowerer is retargetable. Alongside the x86-64 host backend sits a full AArch64 host backend
(`lower/aarch64/mod.rs`) that emits native ARM64 for the entire SMIR op set, x86-64 guest semantics included
(APX, atomics, REP MOVS, vector/FP), plus an AArch64-guest-to-x86 lowerer: the groundwork for
JIT-compiling a guest across host ISAs (x86 on ARM, ARM on x86). Live native execution now runs on both
host ISAs: x86-64 guests run on x86-64 and AArch64 hosts, while AArch64 guests run on AArch64 hosts.
The production x86-on-ARM path uses a 1:1 RAX-R15 → X0-X15 register map and native edge exits, so the
normal hot-loop policy can auto-promote, cache, and re-enter cross-ISA regions. ARM-on-x86 remains an
emit-and-test path, not yet wired into a run loop.

| Piece | Where | What it does |
|-------|-------|--------------|
| **Lifters** | `lift/` | x86-64; AArch64 (A64 scalar, control flow, scalar FP, and a growing NEON set: FP arithmetic, FMLA/FMLS, unary, reductions, permutes, TBL/TBX); RISC-V; AVX10; and a lift-complete Hexagon (every opcode, scalar and HVX) |
| **Interpreter** | `interpret.rs` | direct execution; lazy flags, block caching |
| **Optimizer** | `optimize.rs` | frontier-aware liveness; dead-flag and dead-code elimination, copy propagation, constant and branch folding (O2) |
| **JIT lowering** | `lower/x86_64/mod.rs`, `lower/aarch64/mod.rs`, `lower/regalloc.rs`, `lower/runtime.rs` | x86-64 and AArch64 host emitters (SMIR to native code, x86 guest semantics included), plus an ARM-guest-to-x86 lowerer; 1:1 register map; W^X exec runtime + entry trampoline on both hosts |
| **JIT integration** | `src/isa/x86_64/cpu.rs` and `src/isa/arm/aarch64/cpu.rs` (on by default) | hot-loop detection, region cache, per-host safety gates, SMC eviction, native edge exits; x86-64-host memory via MMU helpers; x86 vCPU native exec on x86-64 and AArch64 hosts |

The lifters are verified too, not just the JIT's native output:
`tests/suites/smir/lift/riscv.rs` and `tests/suites/smir/lift/hexagon.rs` lift each instruction to
SMIR, interpret it, and diff against that
architecture's own qemu-verified interpreter. RISC-V lifts its entire user-mode RV64GCV (scalar, the
full RVV vector ISA, FP, the Zb*/Zk* bit-manip and crypto extensions, and CSRs) at zero divergence;
Hexagon lifts its entire ISA. AArch64 lifting is verified the same way, though not yet exhaustively:
`tests/suites/smir/lower/aarch64_native.rs` lifts, lowers to native ARM64, and executes scalar-integer, scalar-FP,
and NEON sequences on an AArch64 host.

> Note: the JIT does not change behavior. A kernel boots identically with it on or off, because a region
> it cannot prove correct falls back to the interpreter. On the x86-64 host, `RAX_JIT_VERIFY=1` audits
> that equivalence live; the AArch64 tier has no live verify mode yet and relies on its static safety
> gates plus end-to-end comparisons against the interpreters in
> `tests/suites/smir/lower/aarch64_native.rs` and `tests/suites/smir/jit/x86_64_aarch64.rs`.

---

## How the machines work

### Booting x86-64

The fast path loads a Linux kernel (ELF or bzImage) straight into 64-bit mode: kernel at physical
`0x1000000` (16 MiB), the initrd top-down just below the end of guest RAM (capped by the bzImage
header's `initrd_addr_max`), initial page tables (identity-mapped first 8 GiB via 1 GiB
huge pages, kernel space at `0xFFFFFFFF80000000`, direct map at `0xFFFF888000000000`), a minimal 64-bit
GDT, then `CR0.PG=1` / `CR4.PAE=1` / `EFER.LME=1` and a jump to the entry point.

rax also boots the old way, from a bootable CD. A real-mode mini-BIOS (INT 10h/13h/15h/16h/1Ah, an
El-Torito catalog parser, and an ATAPI CD-ROM model) drops a boot image at `0x7C00` in 16-bit real mode
and lets the guest walk itself up through protected mode into long mode. That path boots **TempleOS
V5.03** from its ISO: real to protected to long to 64-bit kernel init, mounting its RedSea CD as drive
`T:` and running its own HolyC compiler.

### The AArch64 machine

An AArch64 guest needs no flags: rax reads the architecture from the kernel image, generates a DTB (RAM,
GICv3, PL011, armv8 generic timer, PSCI) on the fly, and boots it. On the software emulator that means
full EL0/EL1 system emulation: the stage-1 MMU, exception delivery, a GICv3 distributor +
redistributor + ICC system registers, the generic timer, and a PL011 console. On Apple Silicon the same
guest runs near-native through Hypervisor.framework with the in-kernel GICv3. Both paths share the
generated DTB, PSCI, and PL011 wiring.

### The x86-64 interpreter loop

Fetch / decode / execute, with two notable mechanisms:

```text
loop {
    entry = decode_cache[rip & 0xFFF];          // 4096-entry, RIP-indexed
    insn  = if entry.matches(rip, mode) {
        entry.bytes                              // HIT: skip the memory fetch entirely
    } else {
        decode(fetch(rip))                       // MISS: prefixes, ModR/M, SIB, VEX/EVEX, immediates
    };
    execute(insn);                               // update regs / memory / lazy flags
    if (++insn_count & 1023) == 0 { poll_lapic_and_yield(); }
}
```

| Mechanism | What it does |
|-----------|--------------|
| **Decode cache** | 4096 entries indexed by RIP, keyed on a mode tag (`CR3 \| CS.L \| CS.D`). A hit reuses the cached bytes and skips the guest-memory fetch entirely. Kept coherent by SMC detection on guest writes. |
| **Lazy flags** | arithmetic records its operands and defers RFLAGS materialization until a consumer (a `Jcc`, a `PUSHF`) reads them. Most computed flags are never needed. |
| **Fast paths** | a direct host-pointer path for physical RAM, a fast path for common ModR/M memory operands, and page-at-a-time `REP MOVS`/`STOS`. |
| **Hot-block JIT** | on by default: eligible hot loops promote to native code on x86-64 and AArch64 hosts; memory operations currently require an x86-64 host (see above); `RAX_NO_JIT=1` disables it. |
| **TLB** | 256-entry direct-mapped cache over the 4-level page walk (4 KiB / 2 MiB / 1 GiB pages). |

---

## Devices

The machine boots a full legacy PC platform, always on: a 16550 serial console (interrupt-driven, so
guest console input works), 8254 PIT, 8259 PIC, LAPIC + IOAPIC, RTC/CMOS, the 8237 DMA controller,
i8042 PS/2 keyboard, primary and secondary IDE, a floppy controller, system-control ports, QEMU
`fw_cfg`, and the Bochs-style debug port.

On top of that sits a functional **PCI host bridge** with BAR-mapped routing and an MMU MMIO aperture.
Pass `--pci-devices` and rax attaches real controllers that Linux enumerates and drives:

| Class | Device | Status |
|-------|--------|--------|
| **Network** | Intel e1000 (82540EM) | `eth0` comes up; Microwire EEPROM bit-banged via EECD |
| **Storage** | AHCI, NVMe, IDE | enumerate and bind; AHCI reports SATA link state correctly |
| **Audio / USB** | AC97, UHCI | enumerate as PCI endpoints |

> Note: `--pci-devices` is opt-in, so the default boot stays byte-identical and the interpreter hot path
> is untouched (the MMIO aperture collapses to a no-op when no bridge is set). VGA is the one model still
> on the shelf: its legacy `0xA0000` window shadows guest RAM in the flat-memory model, and it is
> display-only, so it is pointless on a serial VM.

---

## Observability and tooling

Because the interpreter owns the step loop, the introspection tools see the real instruction stream:

| Tool | Flag / feature | What you get |
|------|----------------|--------------|
| **Interactive console** | on by default on a TTY | raw-mode serial console with a qemu-style `Ctrl-A` mux (`Ctrl-A s` checkpoints, `x` quits, `h` helps); termios restored even on panic or signal |
| **Machine checkpoints** | `Ctrl-A s` / `SIGUSR1` / `--snapshot-interval N`; resume with `--checkpoint <f.rxc>` | a self-contained `.rxc` image: embedded config, CPU, zstd-compressed RAM, every device's serialized state, and a timing anchor. `rax --checkpoint m.rxc` brings a live machine back with no `--kernel` or `--config` |
| **Instruction trace** | `--trace <file>` (`--features trace`) | SDE-compatible per-instruction trace, diffable against Intel SDE |
| **GDB stub** | `--gdb <port> --wait-gdb` (`--features debug`) | Remote Serial Protocol server: registers, memory, stepping |
| **Profiler** | `--profile` (`--features profiling`) | per-mnemonic execution counts and a hot-instruction report, optional JSON export |

---

## Usage

```
--kernel <path>            Kernel image: ELF, bzImage, or bootable ISO (required)
--initrd <path>            Initial ramdisk
--arch <x86-64|aarch64|riscv64|hexagon|…>   Target architecture (auto-detected from the kernel; default x86-64)
--backend <kvm|emulator>   Virtualization backend (hvf on macOS)
--memory <size>            Guest memory, e.g. "512M", "2G"
--cmdline <string>         Kernel command line
--vcpus <n>                vCPU count (only vCPU 0 executes)
--dtb <path>               Device tree blob for the ARM DT boot path (armv7a)
--config <file>            Load a TOML config
--trace <file>             Write an SDE-compatible instruction trace   (--features trace)
--gdb <port> [--wait-gdb]  Start a GDB stub, optionally wait for attach (--features debug)
--gdb-trace                Log every GDB RSP packet (with --gdb)
--pci-devices              Attach the optional PCI devices (e1000, AHCI, NVMe, UHCI, AC97)
--snapshot-interval <N>    Checkpoint every N instructions (0 = off)
--snapshot-at <n,n,…>      Checkpoint at specific instruction counts
--snapshot-out <file>      Where Ctrl-A s / SIGUSR1 checkpoints land (default checkpoint.rxc)
--snapshot-dir <dir>       Directory for interval/at checkpoints (default .)
--checkpoint <file.rxc>    Resume a whole machine from a checkpoint (no --kernel/--config needed)
--resume <file>            Legacy restore into a machine rebuilt from --kernel/--config
--profile [--profile-output <json>]   Instruction profiling             (--features profiling)
--profile-interval <N>     Live profiling stats every N instructions (default 10M, 0 = off)
--hexagon-isa / --hexagon-endian / --hexagon-entry / --hexagon-load-addr   Hexagon ISA + bare-metal load controls
```

```toml
# config.toml
backend = "emulator"
memory  = "512M"
kernel  = "/path/to/vmlinux"
initrd  = "/path/to/initrd.cpio"
cmdline = "console=ttyS0 earlyprintk=serial"
```

---

## Building

```bash
# Default (Linux): KVM backend enabled.
cargo build --release

# Cross-platform: software emulator only — no KVM. This also drops the native JIT
# (a default feature); add --features smir-jit on a Unix host to keep it.
cargo build --release --no-default-features

# The native JIT ships on by default (disable at runtime with RAX_NO_JIT=1).
# Build without it: cargo build --release --no-default-features --features kvm

# Fastest local interpreter (uses your host's full ISA).
RUSTFLAGS="-C target-cpu=native" cargo build --release

# Apple Silicon (AArch64 guests near-native on Hypervisor.framework, macOS 15+):
# the binary must carry the hypervisor entitlement after every build.
cargo build --release --features hvf
codesign -s - -f --entitlements rax.entitlements target/release/rax
./target/release/rax --backend hvf --kernel linux-aarch64/Image --initrd initramfs.cpio
```

> Note: `.cargo/config.toml` ships `target-cpu=x86-64-v3` as a portable default. It still lets LLVM emit
> AVX2/BMI2/FMA and autovectorize the scalar SIMD/flag loops while staying runnable on any 2013-or-later
> x86-64 host. The release profile is fat-LTO, one codegen unit, `panic=unwind`, stripped.
> Unwinding is retained deliberately: the stable C ABI catches internal panics and returns
> `RAX_ERR_INTERNAL`, so an aborting release profile would violate that embedding contract.

| Feature | Default | Enables |
|---------|---------|---------|
| `kvm` | ✓ (Linux) | KVM backend (`kvm-bindings` / `kvm-ioctls`) |
| `hvf` | | Hypervisor.framework backend (macOS): x86-64 guests on Intel, AArch64 guests on Apple Silicon |
| `smir-jit` | ✓ | SMIR native hot-block JIT (x86-64 and AArch64 hosts; `RAX_NO_JIT=1` disables at runtime) |
| `trace` | | SDE-compatible instruction tracing |
| `debug` | | GDB Remote Serial Protocol server |
| `profiling` | | per-mnemonic profiler + JSON export |
| `x86_64-suite` | | the generated x86-64 instruction test suite |

---

## The microkernel test harness

`microkernel/` is a freestanding bare-metal kernel that exercises the interpreter end to end without a
full Linux image, built for three architectures (x86-64, AArch64, ARMv6): an N-body physics simulation,
a bump allocator, and broad instruction coverage, with the N-body checksum asserted identical across
all three. The same suite also builds as a hosted x86-64 Linux binary for cross-checking under Intel
SDE.

```bash
cd microkernel
make               # build the three bare-metal kernels (x86_64, aarch64, armv6)
make run           # boot all three under rax; assert RESULT PASS + identical checksums
make test-sde      # run the usermode x86-64 build under Intel SDE
```

---

## Status

| Path | State |
|------|-------|
| **x86-64 (KVM/HVF)** | boots Linux to an interactive shell |
| **x86-64 (software)** | boots Linux to a BusyBox shell; full modern ISA; differential-tested vs. KVM; native JIT on x86-64 and AArch64 hosts |
| **AArch64 / ARM** | boots Linux (HVF near-native on Apple Silicon, or full EL0/EL1 software emulation); AArch64 and AArch32 bit-exact vs. qemu; the ARMv6/S5L8900 machine boots iBoot and runs into early iOS XNU/IOKit bringup |
| **Hexagon** | every opcode (scalar + HVX) verified vs. qemu-hexagon; bootable bare-metal backend |
| **RISC-V** | full RVA23 scalar set + crypto + RVV 1.0; bootable `--arch riscv64` backend; verified vs. qemu-riscv64; opt-in SMIR JIT on x86-64 and AArch64 hosts |
| **SMIR** | JIT on by default, auto-triggered, fail-safe; x86-64 hosts admit integer, memory, and eligible FP/SIMD regions; AArch64 hosts admit register-only scalar x86 regions with an exact RFLAGS↔NZCV bridge; both hosts execute state-backed RISC-V scalar/helper regions; RISC-V (incl. RVV) and Hexagon lifts complete |
| **Platform** | legacy PC devices wired; PCI host bridge + `--pci-devices` (e1000 `eth0`, AHCI/NVMe/UHCI/AC97); interactive console and full `.rxc` machine checkpoint/resume |
| **Legacy boot** | real-mode mini-BIOS + El-Torito CD boot; **TempleOS V5.03** boots real to long mode, mounts its CD, runs its HolyC compiler |

### What's missing

This is not a production hypervisor, and the scope is deliberately bounded:

- **No SMP.** A single vCPU executes.
- **Limited devices.** VGA is not wired (serial console only), and PCI interrupts run in polled mode.
- **AArch32 Linux is unproven.** The A32/Thumb core is oracle-validated and drives real machines (the
  ARMv6 SoCs, the `armv7a` DT boot path, Cortex-M), but no 32-bit target has been taken to a Linux
  shell yet.
- **JIT scope.** On x86-64 hosts it compiles integer, memory, and eligible FP/SIMD hot regions;
  x86-on-AArch64 is currently register-only and scalar. The double-width DIV model and native
  block-to-block chaining are future work.
- **RISC-V is bare-metal only.** The runnable `--arch riscv64` backend executes the RVA23 scalar set
  and full RVV 1.0, but there is no privileged/Sv39 MMU, so it runs programs, not kernels.
- **Software x86-64 boot is narrow.** It reaches a BusyBox shell on a mitigations-off ELF kernel; wider
  configurations (CFI/FineIBT, bzImage real-mode entry) are still being worked through. The KVM path
  boots cleanly throughout.

---

## A note on the name

`rax` is the x86-64 accumulator register, the first register the manuals introduce. The project started
x86-64-centric and the name stuck as it grew three more instruction sets. It is also just the crate
name, so `cargo run` works directly.

---

## See also

- [kvm-ioctls](https://github.com/rust-vmm/kvm-ioctls) / [kvm-bindings](https://github.com/rust-vmm/kvm-bindings): KVM access
- [linux-loader](https://github.com/rust-vmm/linux-loader) / [vm-memory](https://github.com/rust-vmm/vm-memory): boot protocol and guest memory
- [QEMU](https://www.qemu.org/): the user-mode reference oracle for AArch64, Hexagon, and RISC-V
- [Intel SDM](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html) · [Arm ASL](https://developer.arm.com/Architectures/A-Profile%20Architecture) · [RISC-V specs](https://riscv.org/technical/specifications/)
- [`docs/specifications/smir/`](docs/specifications/smir/): the SMIR IR specification

## License

MIT
