# eBPF & Solana SBPF Specification Provenance

This directory holds authoritative primary specifications and canonical reference implementation sources for the BPF Instruction Set Architecture (eBPF) and Solana Bytecode Format (SBPF / SBF) used by `rax`.

## Retrieved Documents

| Local Path | Document Title | Organization / Author | Revision / Version | Retrieval Date | License / Redistribution Terms | SHA-256 |
|---|---|---|---|---|---|---|
| `docs/specifications/ebpf/rfc9669.txt` | RFC 9669: BPF Instruction Set Architecture (ISA) | IETF (D. Thaler, Ed.) | RFC 9669 (October 2024, Standards Track) | 2026-10-15 | IETF Trust Legal Provisions (BCP 78) | `54f2d19798d73937befd63ebf7fa65f1a2f1d4a5be29404fb641ca73bb8a1469` |
| `docs/specifications/ebpf/instruction-set.rst` | eBPF Instruction Set Specification, v1.0 | Linux Kernel Organization | `torvalds/linux` `master` (`Documentation/bpf/standardization/instruction-set.rst`) | 2026-10-15 | GPL-2.0 / Linux Kernel Documentation License | `b54e055155ef0daed8351b15d121d61805ddf973f9fe7b2ea6adda32c9d5f1fe` |
| `docs/specifications/ebpf/sbpf/ebpf.rs` | Solana SBPF Opcode and Instruction Definitions | Anza / Solana Labs (`anza-xyz/sbpf`) | `main` (`src/ebpf.rs`) | 2026-10-15 | Apache-2.0 | `53e2cce4ae04bc7be99740dd86b6cc0d88a4e6049a976d2129647d0a76bc6e46` |
| `docs/specifications/ebpf/sbpf/interpreter.rs` | Solana SBPF Canonical Reference Interpreter | Anza / Solana Labs (`anza-xyz/sbpf`) | `main` (`src/interpreter.rs`) | 2026-10-15 | Apache-2.0 | `d9841a9f0b1d69257d77e704f4e0f03d91b2e85be9c734195246d97befc82e6b` |
| `docs/specifications/ebpf/sbpf/program.rs` | Solana SBPF Version Matrix (`SBPFVersion` V0–V4) | Anza / Solana Labs (`anza-xyz/sbpf`) | `main` (`src/program.rs`) | 2026-10-15 | Apache-2.0 | `52ad90f144d201b147ba39d16437e1fe8c04fa7523d7347473616d82f3b56d59` |
| `docs/specifications/ebpf/sbpf/verifier.rs` | Solana SBPF Static Verifier Rules | Anza / Solana Labs (`anza-xyz/sbpf`) | `main` (`src/verifier.rs`) | 2026-10-15 | Apache-2.0 | `b769de34006fc31ec36a41efee8dccf7283237ec762a1d2fc89a21cb69ed8196` |

## Upstream URLs

- `rfc9669.txt`: `https://www.rfc-editor.org/rfc/rfc9669.txt`
- `instruction-set.rst`: `https://raw.githubusercontent.com/torvalds/linux/master/Documentation/bpf/standardization/instruction-set.rst`
- `sbpf/ebpf.rs`: `https://raw.githubusercontent.com/anza-xyz/sbpf/main/src/ebpf.rs`
- `sbpf/interpreter.rs`: `https://raw.githubusercontent.com/anza-xyz/sbpf/main/src/interpreter.rs`
- `sbpf/program.rs`: `https://raw.githubusercontent.com/anza-xyz/sbpf/main/src/program.rs`
- `sbpf/verifier.rs`: `https://raw.githubusercontent.com/anza-xyz/sbpf/main/src/verifier.rs`

## Additional pinned sources for the semantic review

Anza SBPF commit `3e1510b1eac344af15c99dcdb4c3b3e6dc2b3d7b`, retrieved 2026-10-15, Apache-2.0.

| Local path | Purpose | Source | SHA-256 |
|---|---|---|---|
| `sbpf/memory_region.rs` | Virtual stack gaps and region translation | https://github.com/anza-xyz/sbpf/blob/3e1510b1eac344af15c99dcdb4c3b3e6dc2b3d7b/src/memory_region.rs | `71ae0bc37229f8cb7123da8c746fb3c80ac3fe5b4fc563205dc4c5bee8e18c66` |
| `sbpf/vm.rs` | Version-specific initial frame pointer | https://github.com/anza-xyz/sbpf/blob/3e1510b1eac344af15c99dcdb4c3b3e6dc2b3d7b/src/vm.rs | `87c54a63bb655411682afd841c5689b194d4511402237fb91528bb611a818bd7` |
| `sbpf/elf.rs` | Executable text and rodata addresses | https://github.com/anza-xyz/sbpf/blob/3e1510b1eac344af15c99dcdb4c3b3e6dc2b3d7b/src/elf.rs | `017c0e9b1fb9303b56114625101c1500bd223c588a7c9972a5a91633e5b9233c` |

## Compiled upstream reference archive

`sbpf/upstream-3e1510b1.tar.gz` is the unmodified Anza SBPF source archive
(version 0.25.0) at commit `3e1510b1eac344af15c99dcdb4c3b3e6dc2b3d7b`,
retrieved 2026-10-15 from
https://codeload.github.com/anza-xyz/sbpf/tar.gz/3e1510b1eac344af15c99dcdb4c3b3e6dc2b3d7b.
SHA-256: `51b3da377e76a9ae3ee1852a089d81ccf904b686cb90a517f03a1766e0eec127`.
The 242 KiB archive supplies the complete interpreter/verifier and required source
modules for the independent execution tool; upstream Apache-2.0 license files are
retained inside it. The four original bundled `ebpf.rs`, `program.rs`,
`interpreter.rs`, and `verifier.rs` were verified identical to this commit.

## External oracles for the conformance campaign (2026-10-15/16)

### eBPF Foundation `bpf_conformance`

`bpf_conformance/bpf_conformance-698e3795.tar.gz` is the unmodified source
archive of Alan Jowett's `bpf_conformance` (the eBPF Foundation conformance
suite, which treats the Linux kernel as the authoritative implementation) at
commit `698e379525f44cf422bc1dbd43efe8495a76ab87`, retrieved 2026-10-15 from
https://codeload.github.com/Alan-Jowett/bpf_conformance/tar.gz/698e379525f44cf422bc1dbd43efe8495a76ab87.
SHA-256: `4e09b533c6713b39f5833500dcfbf56df3e18ce9403ee8d879e1d9004c17090e`
(117 KiB). The project is MIT-licensed (`LICENSE`); test files carry SPDX
`MIT` (Microsoft) or `Apache-2.0` (Big Switch Networks) headers, and the Apache
License 2.0 text is included in `LICENSE`. It supplies the 313 test programs,
the raw negative programs, the runner, the plugin protocol, and the assembler
that produced `tests/generated/ebpf/bpf_conformance.txt`.

Its runner requires the ELFIO submodule, which is not stored here:
serge1/ELFIO commit `660e90514a2e80fba1c55f2cee07bbeda3d1c601` (the submodule
pin of the commit above), from
https://codeload.github.com/serge1/ELFIO/tar.gz/660e90514a2e80fba1c55f2cee07bbeda3d1c601,
SHA-256 `418315e42d2c4e6745b66b672e05ca9e4d29a57d7d52404b2dda855155136084`
(6.6 MiB, MIT). `tools/ebpf-conformance/run.py` downloads and verifies it.

### Linux kernels

| Byte order | Kernel | Execution | Provenance |
|---|---|---|---|
| Little | Linux `7.0.14-orbstack-00380-ga7e0a2dc9535` aarch64 (OrbStack 29.4.0 Docker engine) | arm64 BPF JIT; `CONFIG_BPF_JIT_ALWAYS_ON=y` (read from `/proc/config.gz`) | Host-provided; the freestanding harness runs in `alpine@sha256:294b683cb724975bec92580e1e685676bd4b50bda910ddb8c51d4cabeaec77e6`, which only supplies a filesystem |
| Big | Debian 13 `linux-image-6.12.107+deb13-s390x` `6.12.107-1` | Generic interpreter: the package config sets `CONFIG_BPF_SYSCALL=y` and no `CONFIG_BPF_JIT` | http://deb.debian.org/debian/pool/main/l/linux/linux-image-6.12.107+deb13-s390x_6.12.107-1_s390x.deb, SHA-256 `72f968758d4ffbb07ad0887d968beec650f961fdc9d3fabcf87a678626ddfd26` (equal to the trixie `Packages` index entry), retrieved 2026-10-15; extracted `boot/vmlinuz-6.12.107+deb13-s390x` SHA-256 `354ea1113285149ad81e0487a34867fdfa827b02b21be8153f576ee7cbba71be`; GPL-2.0 |

The 52 MiB package and the kernel are not stored: `run.py` downloads and
verifies them. QEMU 11.1.1 `qemu-system-s390x` (Homebrew) boots the kernel
under TCG (`-M s390-ccw-virtio -cpu max`) with the harness as PID 1.
