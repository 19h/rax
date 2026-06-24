# rax — C/C++ API for the RAX emulation engine

`librax` is the embeddable C (and C++) face of the RAX CPU emulator. It is built
for **arbitrary emulation**: open an engine for a CPU architecture, map guest
memory at any address, load code and data, read/write the full register file,
then run, single‑step, or step a bounded number of instructions with complete
control over stop conditions and a rich set of execution hooks.

- **Stable ABI.** A single, hand‑authored header (`include/rax.h`) is the source
  of truth. Status/enum values are frozen; structs are versioned or reserved for
  forward‑compatible extension.
- **Idiomatic C++.** `include/rax.hpp` is a header‑only C++17 RAII wrapper with
  typed register access, `std::function` (lambda) hooks, and exceptions.
- **Embeddable.** No global state, no hidden threads, no required runtime files.
  The library validates every argument and can never let a Rust panic cross the
  FFI boundary.
- **Cross‑platform.** Uses the portable software emulator backend, so the same
  code runs identically on Linux, macOS (Intel and Apple Silicon), etc.

## Quick start (C)

```c
#include <rax.h>
#include <stdio.h>

int main(void) {
    rax_engine *e;
    rax_engine_open(RAX_ARCH_X86, RAX_MODE_64, &e);

    /* mov rax,0x1337 ; mov rcx,1 ; add rax,rcx ; hlt */
    unsigned char code[] = {0x48,0xC7,0xC0,0x37,0x13,0,0, 0x48,0xC7,0xC1,1,0,0,0,
                            0x48,0x01,0xC8, 0xF4};
    rax_mem_write(e, 0x1000, code, sizeof code);
    rax_reg_write_u64(e, RAX_X86_REG_RSP, 0x8000);

    rax_emu_start(e, 0x1000, RAX_NO_ADDR, /*timeout_us*/0, /*count*/0);

    uint64_t rax;
    rax_reg_read_u64(e, RAX_X86_REG_RAX, &rax);
    printf("RAX = 0x%llx\n", (unsigned long long)rax);   /* 0x1338 */

    rax_engine_close(e);
}
```

## Quick start (C++)

```cpp
#include <rax.hpp>

rax::Engine e(rax::Arch::X86, RAX_MODE_64);
e.memWrite(0x1000, code);                 // std::vector<uint8_t>
e.hookCode([](rax::Engine&, uint64_t pc, uint32_t) {
    printf("exec 0x%llx\n", (unsigned long long)pc);
});
e.setReg(RAX_X86_REG_RSP, uint64_t(0x8000));
e.start(0x1000);                          // throws rax::Error on a fault
uint64_t result = e.regU64(RAX_X86_REG_RAX);
```

## Building

The library is produced by Cargo from the `rax-capi` crate; the artifacts are
`librax.a`, `librax.so` / `librax.dylib`, in `target/{debug,release}/`.

```sh
cargo build -p rax-capi --release      # or: make -C capi release
```

Link against the dynamic library (recommended — it embeds its system deps):

```sh
cc  -I capi/include app.c   -L target/release -lrax -o app           # C
c++ -std=c++17 -I capi/include app.cpp -L target/release -lrax -o app # C++
```

Linking the **static** `librax.a` additionally requires the platform's system
libraries (handled automatically by the provided CMake/pkg-config):

- macOS: `-framework CoreFoundation -framework Security -framework SystemConfiguration -liconv -lobjc -lpthread`
- Linux: `-lpthread -ldl -lm -lrt -lutil`

### CMake

```sh
cmake -S capi -B build -DCMAKE_INSTALL_PREFIX=/usr/local
cmake --build build && cmake --install build
```

Downstream:

```cmake
find_package(rax REQUIRED)
target_link_libraries(myapp PRIVATE rax::rax)          # dynamic
# or                                  rax::rax_static  # static + system deps
```

### pkg-config / Makefile

```sh
make -C capi install PREFIX=/usr/local
cc app.c $(pkg-config --cflags --libs rax) -o app
```

`make -C capi test` builds the library and compiles+runs every example.

## Optional Cargo features

The `rax-capi` crate forwards optional engine capabilities:

| feature | effect |
|---------|--------|
| `jit`   | enables the SMIR native hot‑block JIT (x86‑64 host) |
| `kvm`   | KVM backend (x86‑64 Linux; not exposed via the C backend selector yet) |
| `hvf`   | Hypervisor.framework backend (macOS) |
| `trace` | verbose instruction tracing |

```sh
cargo build -p rax-capi --release --features jit
```

## API overview

| Area | Functions |
|------|-----------|
| Library | `rax_version`, `rax_version_string`, `rax_strerror` |
| Lifecycle | `rax_engine_open`, `rax_engine_open_config`, `rax_engine_close`, `rax_engine_reset` |
| Queries | `rax_engine_arch`, `rax_engine_mode`, `rax_engine_supports_stepping`, `rax_engine_errmsg` |
| Memory map | `rax_mem_map`, `rax_mem_unmap`, `rax_mem_protect`, `rax_mem_regions` |
| Memory access | `rax_mem_read`/`write`, `rax_mem_read_virt`/`write_virt`, `rax_mem_translate` |
| Registers | `rax_reg_size`, `rax_reg_read`/`write`, `rax_reg_read_u64`/`write_u64` |
| Execution | `rax_emu_start`, `rax_emu_step`, `rax_emu_stop`, `rax_emu_last_exit`, `rax_emu_icount` |
| Interrupts | `rax_interrupt`, `rax_nmi`, `rax_can_interrupt` |
| Hooks | `rax_hook_add_code`/`block`/`intr`/`io_in`/`io_out`/`mmio_read`/`mmio_write`/`invalid`, `rax_hook_del` |
| Context | `rax_context_save`, `rax_context_restore` |

### Memory model

Memory is a set of non‑overlapping, page‑aligned regions backed by demand‑paged
anonymous mappings; you may map regions at **any 64‑bit address**. The opener
pre‑maps one default region so the simplest programs "just work"; you can unmap
or remap it (at least one region must always remain mapped). Host accesses
(`rax_mem_read`/`write`) succeed for any mapped range regardless of permissions;
virtual accesses translate through the guest's current paging state.

### Registers

Each architecture has its own register‑id space. The header exposes both
**family macros** (e.g. `RAX_X86_GPR64(i)`, `RAX_ARM64_X(i)`, `RAX_X86_ZMM(i)`)
that give complete coverage, and **named aliases** (`RAX_X86_REG_RAX`, …) that
evaluate to the same numbers. Values are little‑endian, sized to the register's
natural width (`rax_reg_size`); vector registers are raw byte arrays. x86
sub‑register writes follow architectural semantics (writing `EAX` zero‑extends
into `RAX`; `AX`/`AL`/`AH` preserve the rest).

### Execution & stop reasons

`rax_emu_start(begin, until, timeout_us, count)` runs until the first stop
condition; `rax_emu_last_exit` reports why (`rax_exit.reason` is one of the
`RAX_STOP_*` values: count/until/timeout/stopped/hlt/io/mmio/exception/…). The
call returns `RAX_OK` for any clean stop and an error status only for an
unrecoverable fault.

### Hooks

Code and block hooks fire per instruction / per basic‑block entry and require a
stepping‑capable backend. Interrupt, port‑I/O, MMIO, and invalid‑instruction
hooks service the corresponding exits and let execution continue (e.g. an
`io_in` hook supplies the value the guest reads). Callbacks receive the engine
handle and may freely re‑enter the API, including `rax_emu_stop`.

## Architecture capability matrix

| Architecture | `RAX_ARCH_*` | registers / memory / run | single‑step + code/block hooks |
|--------------|--------------|--------------------------|--------------------------------|
| x86 / x86‑64 | `X86`        | ✅                       | ✅                              |
| AArch64      | `ARM64`      | ✅                       | ✅                              |
| RISC‑V (RV64)| `RISCV64`    | ✅                       | ✅                              |
| AArch32 / ARMv7 | `ARM`     | ✅                       | run‑to‑exit                    |
| Cortex‑M     | `CORTEXM`    | ✅                       | run‑to‑exit                    |
| Hexagon      | `HEXAGON`    | ✅                       | run‑to‑exit                    |

All architectures support the full register, memory, run, reset, and context
API. Instruction‑granular control (`count`/`until`, code/block hooks, and
`rax_emu_step`) is available on every backend that advertises
`rax_engine_supports_stepping` — x86‑64, AArch64, and RISC‑V today; the
remaining architectures run to the next exit. Query at runtime rather than
assuming.

## Threading & safety

An `rax_engine` handle is **not** thread‑safe: drive a single handle from one
thread at a time. Distinct handles are independent and may run concurrently on
different threads. The library never takes ownership of caller buffers; all data
is copied. Every entry point validates its arguments, and a panic in engine code
is contained and reported as `RAX_ERR_INTERNAL` rather than crossing the FFI
boundary.

## Examples

See `examples/`:

- `x86_64_basic.c` — minimal open/load/run/read.
- `x86_64_hooks.c` — code + block hooks and stopping from a hook.
- `x86_64_step.c` — single‑stepping instruction by instruction.
- `x86_64_io.c` — servicing guest port I/O with hooks.
- `mem_and_context.c` — sparse mapping, region enumeration, snapshots.
- `cpp_engine.cpp` — the C++ wrapper with a lambda hook and a context round‑trip.

## License

MIT (matching the RAX engine).
