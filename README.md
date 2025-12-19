# RAX

A minimal x86_64 hypervisor/emulator in Rust.

## Overview

RAX can boot a Linux kernel two ways:

1. **KVM mode** (Linux only): Uses hardware virtualization. The kernel runs at native speed, trapping to RAX only for I/O.

2. **Emulator mode** (any platform): Software interprets every x86 instruction. Slow, but portable and debuggable.

Both modes share the same device emulation (serial port), memory layout, and boot protocol implementation.

## How it works

### Boot process

1. Load the bzImage kernel at physical address 0x100000
2. Load initrd (if provided) at 0x4000000
3. Set up initial page tables mapping the first 1GB identity-mapped
4. Configure a minimal GDT with 64-bit code/data segments
5. Set up the CPU in 64-bit long mode (CR0.PG, CR4.PAE, EFER.LME)
6. Jump to the kernel's 64-bit entry point

### KVM backend

Wraps the Linux KVM API:
- `KVM_CREATE_VM` to create a VM
- `KVM_SET_USER_MEMORY_REGION` to map guest physical memory
- `KVM_CREATE_VCPU` and `KVM_RUN` to execute

When the guest does I/O (like writing to the serial port), KVM exits back to RAX which handles it.

### Software emulator

A fetch-decode-execute loop:

```
loop {
    bytes = read_memory(RIP)
    insn = decode(bytes)      // parse prefixes, opcode, ModR/M, SIB
    execute(insn)             // update registers/memory, advance RIP
}
```

The decoder handles x86 prefixes (REX, operand size, address size, REP) and addressing modes (register, memory, RIP-relative, SIB).

Instructions are organized by category:
- `insn/arith.rs` - ADD, SUB, CMP, INC, DEC
- `insn/control.rs` - JMP, CALL, RET, Jcc
- `insn/data.rs` - MOV, LEA, PUSH, POP
- `insn/logic.rs` - AND, OR, XOR, TEST
- `insn/string.rs` - REP MOVS, REP STOS
- etc.

The MMU translates virtual addresses by walking the 4-level page table (PML4 → PDPT → PD → PT).

## Build & Run

```bash
cargo build --release

# KVM
./target/release/rax --kernel bzImage --initrd initrd.img

# Emulator
./target/release/rax --backend emulator --kernel bzImage --initrd initrd.img

# Cross-platform build (no KVM)
cargo build --release --no-default-features
```

## Status

- KVM: Boots Linux fully
- Emulator: Gets into early kernel init. Needs kernel-space page table mappings to go further.

## Missing

- SMP (only one vCPU runs)
- Interrupts/APIC
- Storage, network, graphics
- Most privileged instructions
