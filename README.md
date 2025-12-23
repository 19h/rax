# RAX

A minimal x86_64 hypervisor and emulator written in Rust. It boots Linux.

## Why?

If you've ever wondered what happens between pressing Enter on `./linux` and seeing a shell, this is a good place to find out. RAX implements just enough virtualization machinery to boot a real Linux kernel.

There are two backends:

- **KVM mode** (Linux): Uses hardware virtualization. The kernel runs at near-native speed, trapping to userspace only for I/O.

- **Emulator mode** (any platform): A software CPU that interprets x86 instructions. Slow, but you can trace every instruction the kernel executes.

Both backends share the same device emulation and boot protocol code.

## Building

```bash
# Full build (KVM + emulator)
cargo build --release

# Cross-platform build (emulator only)
cargo build --release --no-default-features
```

## Running

```bash
# KVM (default, fast)
./target/release/rax --kernel bzImage --initrd initrd.img

# Emulator (slow, works anywhere)
./target/release/rax --backend emulator --kernel bzImage --initrd initrd.img

# Verbose logging
RUST_LOG=debug ./target/release/rax --kernel bzImage
```

## How it works

### Boot sequence

RAX implements the Linux x86 boot protocol:

1. Load bzImage at physical address `0x100000`
2. Load initrd high in memory at `0x4000000`
3. Set up initial page tables (identity-mapped + kernel virtual addresses)
4. Configure a minimal GDT with 64-bit code/data segments
5. Enter 64-bit long mode (CR0.PG=1, CR4.PAE=1, EFER.LME=1)
6. Jump to the kernel's 64-bit entry point

### The software emulator

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
| Integer | ADD, SUB, MUL, DIV, INC, DEC, CMP, NEG |
| Logic | AND, OR, XOR, TEST, NOT |
| Shifts | SHL, SHR, SAR, ROL, ROR, SHLD, SHRD |
| Control | JMP, CALL, RET, Jcc, LOOP, CMOVcc |
| Data | MOV, LEA, MOVZX, MOVSX, PUSH, POP, XCHG |
| String | REP MOVSB/W/D/Q, STOSB, LODSB, SCASB, CMPSB |
| Bit | BT, BTS, BTR, BTC, BSF, BSR, POPCNT |
| x87 FPU | FLD, FST, FADD, FSUB, FMUL, FDIV, FCOM, ... |
| SSE/AVX | MOVAPS, ADDPS, MULPS, CMPPS, CVTSI2SS, ... |
| AVX shifts | VPSLLW/D/Q, VPSRLW/D/Q, VPSRAW/D, VPSLLDQ |
| AVX permute | VINSERTF128, VEXTRACTF128, VPERM2F128 |
| BMI1/BMI2 | ANDN, BLSI, BLSR, BZHI, PEXT, PDEP, MULX |
| System | CPUID, RDMSR, WRMSR, MOV CR, LGDT, LIDT |

The emulator handles the full x86 encoding complexity: REX prefixes, legacy prefixes (operand size, address size, REP, segments), ModR/M, SIB, VEX2/VEX3, and RIP-relative addressing.

### Devices

Minimal device emulation for Linux boot:

- **Serial (16550)**: Console I/O at port 0x3F8
- **RTC stub**: Enough for the kernel
- **PCI stub**: Responds to config space probes

## Code structure

```
src/
├── main.rs              # CLI entry point
├── vmm.rs               # VM manager
├── memory.rs            # Guest memory
├── cpu/                 # CPU state, VCpu trait
├── arch/x86_64.rs       # Boot protocol, GDT, page tables
├── backend/
│   ├── kvm/             # KVM wrapper
│   └── emulator/x86_64/
│       ├── cpu.rs       # Emulator core
│       ├── decoder.rs   # Instruction decoder
│       ├── mmu.rs       # Page table walking
│       ├── dispatch/    # Opcode dispatch
│       │   ├── legacy.rs    # Single-byte opcodes
│       │   ├── twobyte.rs   # 0F-prefixed opcodes
│       │   └── vex.rs       # VEX/AVX opcodes
│       └── insn/        # 11 instruction categories, 77 files
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
│           └── io/      # IN, OUT, INS, OUTS
└── devices/             # Serial, PCI, RTC
```

## Test suite

```bash
cargo test --features x86_64-suite
```

## Status

| Backend | Status |
|---------|--------|
| KVM | Boots Linux, interactive shell works |
| Emulator | Early kernel init, extensive instruction coverage |

| Feature | Status |
|---------|--------|
| Legacy x86 | Complete |
| x87 FPU | Complete |
| SSE | Most operations |
| AVX | Core operations, shifts, permutes |
| BMI1/BMI2 | Complete |
| AVX-512 | Not implemented |

## What's missing

For a production hypervisor you'd need:

- **SMP**: Only one vCPU
- **Interrupts/APIC**: No interrupt controller
- **Disk/Network/Graphics**: No storage, networking, or display
- **AVX-512**: Not implemented

## See also

- [kvm-ioctls](https://github.com/rust-vmm/kvm-ioctls) - KVM bindings
- [linux-loader](https://github.com/rust-vmm/linux-loader) - bzImage loading
- [Intel SDM](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html) - x86 reference

## License

MIT
