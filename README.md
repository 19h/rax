# RAX

A comprehensive x86_64 hypervisor and emulator written in Rust. It boots Linux.

## Why?

If you've ever wondered what happens between pressing Enter on `./linux` and seeing a shell, this is a good place to find out. RAX implements virtualization machinery to boot a real Linux kernel, with a software emulator that covers nearly the entire x86_64 instruction set including AVX-512.

There are two backends:

- **KVM mode** (Linux): Uses hardware virtualization. The kernel runs at near-native speed, trapping to userspace only for I/O.

- **Emulator mode** (any platform): A software CPU that interprets x86 instructions one by one. Slow, but you can trace every instruction the kernel executes.

Both backends share the same device emulation, memory management, and boot protocol code.

## Quick Start

```bash
# Build (Linux with KVM)
cargo build --release

# Build (cross-platform, emulator only)
cargo build --release --no-default-features

# Run with KVM (fast)
./target/release/rax --kernel bzImage --initrd initrd.img

# Run with emulator (works anywhere)
./target/release/rax --backend emulator --kernel bzImage --initrd initrd.img

# Verbose logging
RUST_LOG=debug ./target/release/rax --kernel bzImage
```

## How It Works

### Boot Sequence

RAX implements the Linux x86 boot protocol:

1. Load the kernel (ELF or bzImage) at physical address `0x1000000` (16MB)
2. Load initrd high in memory at `0x4000000`
3. Set up initial page tables:
   - Identity-mapped first 8GB using 1GB huge pages
   - Kernel virtual addresses at `0xFFFFFFFF80000000`
   - Direct physical map at `0xFFFF888000000000`
4. Configure a minimal GDT with 64-bit code/data segments
5. Enter 64-bit long mode (CR0.PG=1, CR4.PAE=1, EFER.LME=1)
6. Jump to the kernel's 64-bit entry point

### The Software Emulator

The emulator implements a complete x86_64 instruction decoder and executor:

```
loop {
    bytes = read_memory(RIP);
    insn = decode(bytes);      // prefixes, opcode, ModR/M, SIB, immediates
    execute(insn);             // update registers/memory/flags
}
```

**Instruction coverage:**

| Category | Examples |
|----------|----------|
| Integer | ADD, SUB, MUL, DIV, INC, DEC, CMP, NEG, ADC, SBB |
| Logic | AND, OR, XOR, TEST, NOT |
| Shifts | SHL, SHR, SAR, ROL, ROR, RCL, RCR, SHLD, SHRD |
| Control | JMP, CALL, RET, Jcc, LOOP, CMOVcc, SETcc |
| Data | MOV, LEA, MOVZX, MOVSX, PUSH, POP, XCHG, CMPXCHG |
| String | REP MOVSB/W/D/Q, STOSB, LODSB, SCASB, CMPSB |
| Bit | BT, BTS, BTR, BTC, BSF, BSR, POPCNT, LZCNT |
| x87 FPU | FLD, FST, FADD, FSUB, FMUL, FDIV, FCOM, ... |
| SSE/AVX | MOVAPS, ADDPS, MULPS, CMPPS, CVTSI2SS, ... |
| AVX shifts | VPSLLW/D/Q, VPSRLW/D/Q, VPSRAW/D, VPSLLDQ |
| AVX permute | VINSERTF128, VEXTRACTF128, VPERM2F128 |
| AVX-512 | VMOVDQU32/64, VPADDD, VPORD, masked operations |
| FMA | VFMADD/SUB/NMADD/NSUB 132/213/231 variants |
| BMI1/BMI2 | ANDN, BLSI, BLSR, BZHI, PEXT, PDEP, MULX |
| AES/SHA | AESENC, AESDEC, SHA1/256 rounds |
| System | CPUID, RDMSR, WRMSR, MOV CR, LGDT, LIDT |

The emulator handles the full x86 encoding complexity: REX prefixes, legacy prefixes (operand size, address size, REP, segments), ModR/M, SIB, VEX2/VEX3, EVEX, and RIP-relative addressing.

### Devices

Minimal device emulation for Linux boot:

- **Serial (16550)**: Console I/O at port `0x3F8`
- **PIT (8254)**: System timer at ports `0x40-0x43`
- **PIC (8259)**: Interrupt controller at `0x20-0x21`, `0xA0-0xA1`
- **LAPIC**: Local APIC with timer at `0xFEE00000`
- **RTC/PCI**: Stubs for kernel probing
- **Debug**: Bochs-style output at port `0xE9`

### Performance Optimizations

The emulator includes several optimizations:

- **Lazy Flag Computation**: Arithmetic flags are computed on-demand, not after every instruction
- **Decode Cache**: 4096-entry cache avoids re-decoding hot instruction sequences
- **TLB**: 256-entry translation lookaside buffer for address translation
- **Wall-Clock Timing**: TSC based on real time for accurate timer behavior

## Code Structure

```
src/
├── main.rs              # CLI entry point
├── vmm.rs               # VM manager and run loop
├── memory.rs            # Guest memory allocation
├── timing.rs            # Wall-clock TSC emulation
├── cpu/                 # CPU state, VCpu trait, exit reasons
├── arch/x86_64.rs       # Boot protocol, GDT, page tables
├── backend/
│   ├── kvm/             # KVM wrapper (Linux only)
│   └── emulator/x86_64/
│       ├── cpu.rs       # Emulator core (1,600 lines)
│       ├── decoder.rs   # Instruction decoder
│       ├── mmu.rs       # Page table walking, TLB
│       ├── flags.rs     # RFLAGS computation
│       ├── dispatch/    # Opcode dispatch tables
│       │   ├── legacy.rs    # Single-byte opcodes
│       │   ├── twobyte/     # 0F-prefixed opcodes
│       │   └── vex/         # AVX opcodes
│       └── insn/        # 11 instruction categories, 87 files
│           ├── arith/   # ADD, SUB, MUL, DIV, ...
│           ├── logic/   # AND, OR, XOR, ...
│           ├── data/    # MOV, PUSH, POP, ...
│           ├── simd/    # SSE/AVX operations
│           ├── control/ # JMP, CALL, RET, ...
│           ├── shift/   # SHL, SHR, ROL, ...
│           ├── bit/     # BT, BSF, BSR, ...
│           ├── string/  # MOVS, STOS, ...
│           ├── fpu/     # x87 FPU (D8-DF)
│           ├── system/  # CPUID, MSR, CR/DR
│           └── io/      # IN, OUT
└── devices/             # Serial, PIT, PIC, LAPIC
```

## Test Suite

```bash
cargo test --features x86_64-suite
```

- **27,168 tests** covering individual instruction behavior
- **100% passing**

## Configuration

### CLI Options

```
--kernel <path>           Kernel image (required)
--initrd <path>           Initial ramdisk
--backend <kvm|emulator>  Virtualization backend
--memory <size>           Guest memory (e.g., "512M", "2G")
--cmdline <string>        Kernel command line
--trace <file>            Enable instruction tracing
--gdb <port>              Enable GDB server
--config <file>           Load TOML config
```

### TOML Config

```toml
backend = "emulator"
memory = "512M"
kernel = "/path/to/bzImage"
initrd = "/path/to/initrd.img"
cmdline = "console=ttyS0 earlyprintk=serial"
```

## Features

| Feature | Description |
|---------|-------------|
| `kvm` (default) | KVM backend (Linux only) |
| `trace` | SDE-compatible instruction tracing |
| `debug` | GDB remote serial protocol server |
| `x86_64-suite` | Enable comprehensive instruction tests |

## Status

| Backend | Status |
|---------|--------|
| KVM | Boots Linux, interactive shell works |
| Emulator | Early kernel init (investigating fixmap issue) |

| Feature | Status |
|---------|--------|
| Legacy x86 | Complete |
| x87 FPU | Complete |
| SSE/SSE2/SSE3/SSSE3/SSE4 | Complete |
| AVX/AVX2 | Complete |
| BMI1/BMI2 | Complete |
| FMA | Complete |
| AVX-512 | Complete (F, VL, BW, DQ, CD) |
| AES/SHA | Complete |
| AVX10 | Not implemented |

## What's Missing

For a production hypervisor you'd need:

- **SMP**: Only one vCPU currently executes
- **Full APIC**: Basic interrupt routing only
- **Disk/Network/Graphics**: No storage, networking, or display
- **AVX10**: Not implemented

## Microkernel Test Harness

A bare-metal test kernel lives in `/microkernel/` for validation:

```bash
cd microkernel
make baremetal    # Build bare-metal ELF
make test-rax     # Boot in emulator
make test-sde     # Run under Intel SDE
```

Features N-body physics simulation and comprehensive instruction tests.

## See Also

- [kvm-ioctls](https://github.com/rust-vmm/kvm-ioctls) - KVM bindings
- [linux-loader](https://github.com/rust-vmm/linux-loader) - bzImage loading
- [Intel SDM](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html) - x86 reference

## License

MIT
