# Non-PAE paging probe

Python 3.10+, LLVM (`llvm-mc` and `ld.lld`) and `qemu-system-i386` are required.
The guest is a bare-metal Multiboot executable; no guest libc or sysroot is used.

```sh
python3 tools/x86-paging/legacy_oracle.py \
    --llvm-bin /path/to/llvm/bin \
    --work-dir /tmp/rax-legacy-paging-oracle
```

The guest checks 4 KiB/4 MiB mappings, accessed/dirty status, PSE changes, ignored
PSE=0 fields, an absent PTE, supervisor write protection, and paging disable/enable
translation invalidation. It verifies three page-fault deliveries. Every command
must succeed with the expected status (QEMU debug-exit success is33), and the
exact completion marker must be present. A missing tool, timeout or failed check
fails the command. Commands, versions, timings and source/ELF hashes are retained.

This is a bounded independent execution probe. Installed QEMU11.1.2 TCG accepts
some PSE36/PAT fields after their CPUID bits are cleared, and reports reserved-bit
fault code8 (P=0). The guest explicitly records that QEMU-specific error code.
[Intel SDM086](../../docs/specifications/x86_64/325462-sdm-vol-1-2abcd-3abcd-4-1.pdf)
Vol.3A5.3/5.7 requires absent-feature reserved checks and P=1 for reserved faults.
The RAX regression tests use those Intel requirements, including code9 for a
supervisor read, rather than treating the QEMU differences as reference behavior.
The relevant QEMU implementation is in
[`excp_helper.c`](https://github.com/qemu/qemu/blob/master/target/i386/tcg/system/excp_helper.c).
Exact installed-source revision is unknown; source/runtime evidence are distinct.

PAE/PDPTE loading, IA-32e translation, Linux boot, and physical x86 hardware require
separate validation. This probe does not establish full CPU equivalence.
