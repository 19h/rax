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
