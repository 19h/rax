# RAX

A minimal x86_64 hypervisor and emulator written in Rust. It boots Linux.

## Why?

If you've ever wondered what happens between pressing Enter on `./linux` and seeing a shell, this is a good place to find out. RAX implements just enough virtualization machinery to boot a real Linux kernel - nothing more, nothing less.

There are two backends:

- **KVM mode** (Linux): Uses hardware virtualization. The kernel runs at near-native speed, trapping to userspace only for I/O. This is how production hypervisors work.

- **Emulator mode** (any platform): A software CPU that interprets x86 instructions one at a time. Painfully slow, but you can trace every instruction the kernel executes. Great for understanding what's actually happening.

Both backends share the same device emulation and boot protocol code. Swap backends with a flag and compare behavior.

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

# Emulator (slow, but works anywhere)
./target/release/rax --backend emulator --kernel bzImage --initrd initrd.img

# Verbose logging
RUST_LOG=debug ./target/release/rax --kernel bzImage
```

The `bzImage` is a compressed Linux kernel. Most distros can build one, or grab a prebuilt from kernel.org. The initrd is optional but you'll need one to get a shell.

## How it works

### Boot sequence

RAX implements the Linux x86 boot protocol:

1. Load the bzImage at physical address `0x100000` (the standard load address)
2. Load initrd (if any) high in memory at `0x4000000`
3. Set up initial page tables - identity-mapped first 1GB, plus kernel virtual address mappings
4. Configure a minimal GDT with 64-bit code/data segments
5. Put the CPU in 64-bit long mode (CR0.PG=1, CR4.PAE=1, EFER.LME=1)
6. Jump to the kernel's 64-bit entry point

From there, the kernel takes over. It sets up its own page tables, initializes drivers, mounts the initrd, and eventually launches init.

### The KVM backend

KVM does the heavy lifting. RAX just:

- Creates a VM with `KVM_CREATE_VM`
- Maps guest memory with `KVM_SET_USER_MEMORY_REGION`
- Creates a vCPU and runs it with `KVM_RUN`

The kernel runs directly on hardware. When it does I/O (like writing to the serial port), KVM exits back to RAX, which emulates the device and returns.

### The software emulator

This is where things get interesting. The emulator is a straightforward fetch-decode-execute loop:

```
loop {
    bytes = read_memory(RIP);
    insn = decode(bytes);      // parse prefixes, opcode, ModR/M, SIB, immediates
    execute(insn);             // update registers/memory/flags, advance RIP
}
```

The decoder handles the full x86 encoding complexity:
- REX prefixes (for 64-bit operands and extended registers)
- Legacy prefixes (operand size, address size, REP, segment overrides)
- ModR/M and SIB addressing modes
- RIP-relative addressing

Instructions are organized by category:

| File | Instructions |
|------|--------------|
| `insn/arith.rs` | ADD, SUB, CMP, INC, DEC, NEG, IMUL |
| `insn/control.rs` | JMP, CALL, RET, Jcc (all conditions) |
| `insn/data.rs` | MOV, LEA, MOVZX, MOVSX, PUSH, POP, XCHG |
| `insn/logic.rs` | AND, OR, XOR, TEST, NOT |
| `insn/shift.rs` | SHL, SHR, SAR, ROL, ROR |
| `insn/string.rs` | REP MOVSB/W/D/Q, REP STOSB/W/D/Q, LODS, SCAS, CMPS |
| `insn/bit.rs` | BT, BTS, BTR, BTC, BSF, BSR |
| `insn/system.rs` | CPUID, RDMSR, WRMSR, MOV CR, LGDT, LIDT |
| `insn/io.rs` | IN, OUT |

The MMU handles virtual-to-physical translation by walking the 4-level page table (PML4 → PDPT → PD → PT). It supports 4KB pages, 2MB huge pages, and 1GB huge pages.

### Devices

Minimal device emulation to get Linux booting:

- **Serial (16550)**: Console I/O at port 0x3F8
- **RTC stub**: Enough to stop the kernel from complaining
- **PCI stub**: Responds to config space probes

## Code structure

```
src/
├── main.rs          # CLI and entry point
├── vmm.rs           # VM manager - ties everything together
├── config.rs        # Configuration parsing
├── memory.rs        # Guest memory allocation
├── cpu/             # CPU state types and VCpu trait
├── arch/
│   └── x86_64.rs    # Boot protocol, GDT, page tables
├── backend/
│   ├── kvm/         # KVM wrapper
│   └── emulator/
│       └── x86_64/
│           ├── cpu.rs      # Emulator core
│           ├── decoder.rs  # Instruction decoder
│           ├── mmu.rs      # Page table walking
│           ├── flags.rs    # RFLAGS computation
│           └── insn/       # Instruction implementations
└── devices/         # Serial, PCI, RTC stubs
```

## Current status

| Backend | Status |
|---------|--------|
| KVM | Boots Linux fully, interactive shell works |
| Emulator | Gets into early kernel init. Working on kernel-space page table mappings. |

## What's missing

Things you'd need for a real hypervisor:

- **SMP**: Only one vCPU runs
- **Interrupts/APIC**: No interrupt controller emulation
- **Disk/Network/Graphics**: No storage, networking, or display
- **Many privileged instructions**: Just enough for Linux to boot

## See also

- [kvm-ioctls](https://github.com/rust-vmm/kvm-ioctls) - The KVM bindings we use
- [linux-loader](https://github.com/rust-vmm/linux-loader) - bzImage loading
- [Intel SDM](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html) - The x86 bible

## License

MIT
