//! Attributing KVM shutdowns to host CPU capability.
//!
//! KVM executes the guest natively, so an instruction the host CPU does not
//! implement raises #UD and, with no IDT in the guest, ends in a triple-fault
//! shutdown that says nothing about the interpreter (KVM resets the vCPU on
//! shutdown, so the faulting RIP is not recoverable from KVM). `unsupported`
//! takes the instruction addresses the interpreter executed for the same case
//! and reports the case as skipped only when one of those instructions needs
//! a feature the host CPU does not enumerate, or Intel-defined behavior that
//! a non-Intel host implements differently (rax models the Intel SDM). Every
//! other shutdown remains a failure.

/// Marks a backend error string as "this host cannot run the case".
pub(super) const HOST_UNSUPPORTED: &str = "host cannot run this case: ";

/// The CPUID bits the attribution needs, from KVM's supported CPUID.
struct HostCpu {
    intel: bool,
    /// CPUID.(EAX=7,ECX=0): EBX, ECX, EDX.
    leaf7: [u32; 3],
    /// CPUID.(EAX=7,ECX=1): EAX.
    leaf7_1_eax: u32,
}

impl HostCpu {
    /// The host's own CPUID; KVM's guest CPUID enumerates no feature the
    /// host lacks.
    #[cfg(target_arch = "x86_64")]
    #[allow(unused_unsafe)] // `__cpuid_count` is safe on newer toolchains.
    fn new() -> Self {
        use core::arch::x86_64::__cpuid_count;
        // SAFETY: CPUID is available on every x86-64 processor.
        let leaf0 = unsafe { __cpuid_count(0, 0) };
        // SAFETY: as above; leaf 7 is read only when leaf 0 enumerates it.
        let (leaf7, leaf7_1) = unsafe {
            if leaf0.eax >= 7 {
                (__cpuid_count(7, 0), __cpuid_count(7, 1))
            } else {
                (__cpuid_count(0, 0), __cpuid_count(0, 0))
            }
        };
        HostCpu {
            // "GenuineIntel" in EBX, EDX, ECX.
            intel: (leaf0.ebx, leaf0.edx, leaf0.ecx) == (0x756E_6547, 0x4965_6E69, 0x6C65_746E),
            leaf7: if leaf0.eax >= 7 {
                [leaf7.ebx, leaf7.ecx, leaf7.edx]
            } else {
                [0; 3]
            },
            leaf7_1_eax: if leaf0.eax >= 7 { leaf7_1.eax } else { 0 },
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    fn new() -> Self {
        HostCpu {
            intel: false,
            leaf7: [0; 3],
            leaf7_1_eax: 0,
        }
    }

    fn ebx(&self, bit: u32) -> bool {
        self.leaf7[0] >> bit & 1 != 0
    }

    fn ecx(&self, bit: u32) -> bool {
        self.leaf7[1] >> bit & 1 != 0
    }

    fn edx(&self, bit: u32) -> bool {
        self.leaf7[2] >> bit & 1 != 0
    }

    /// The AVX-512 subsets of Ice Lake / Zen 4 class hosts: CD, DQ, BW, VL,
    /// IFMA, VBMI, VBMI2, VNNI, BITALG, VPOPCNTDQ, GFNI, VAES, VPCLMULQDQ.
    fn common_avx512(&self) -> bool {
        [17, 21, 28, 30, 31].into_iter().all(|bit| self.ebx(bit))
            && [1, 6, 8, 9, 10, 11, 12, 14]
                .into_iter()
                .all(|bit| self.ecx(bit))
    }

    /// What an EVEX instruction in `map` with `pp` and `opcode` needs that
    /// this host lacks.
    fn evex_gap(&self, map: u8, pp: u8, opcode: u8, modrm: Option<u8>) -> Option<&'static str> {
        if !self.ebx(16) {
            return Some("AVX-512F");
        }
        if matches!(map, 5 | 6) && !self.edx(23) {
            return Some("AVX512-FP16");
        }
        let reg = modrm.map(|m| m >> 3 & 7);
        let rare = match (map, pp, opcode) {
            (2, 3, 0x68) => Some(("AVX512_VP2INTERSECT", self.edx(8))),
            (2, 3, 0x9A | 0x9B | 0xAA | 0xAB) => Some(("AVX512_4FMAPS", self.edx(3))),
            (2, 3, 0x52 | 0x53) => Some(("AVX512_4VNNIW", self.edx(2))),
            (2, 1, 0xC8 | 0xCA..=0xCD) => Some(("AVX512ER", self.ebx(27))),
            (2, 1, 0xC6 | 0xC7) if matches!(reg, Some(1 | 2 | 5 | 6)) => {
                Some(("AVX512PF", self.ebx(26)))
            }
            (2, 2 | 3, 0x72) | (2, 2, 0x52) => {
                Some(("AVX512_BF16", self.leaf7_1_eax >> 5 & 1 != 0))
            }
            _ => None,
        };
        match rare {
            Some((name, false)) => Some(name),
            Some((_, true)) => None,
            None if !self.common_avx512() => Some("an AVX-512 subset beyond AVX-512F"),
            None => None,
        }
    }
}

/// Why the host cannot run a case whose interpreter run executed the
/// instructions at `executed` in `code` (loaded at `code_addr`): the first
/// such instruction that needs a feature the host lacks.
pub(super) fn unsupported(code: &[u8], code_addr: u64, executed: &[u64]) -> Option<String> {
    let host = HostCpu::new();
    executed
        .iter()
        .find_map(|&rip| instruction_gap(&host, code, code_addr, rip))
}

fn instruction_gap(host: &HostCpu, code: &[u8], code_addr: u64, rip: u64) -> Option<String> {
    let insn = code.get(usize::try_from(rip.checked_sub(code_addr)?).ok()?..)?;
    let mut i = 0;
    let mut last_legacy = None;
    while let Some(&b) = insn.get(i) {
        if !matches!(
            b,
            0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x66 | 0x67 | 0xF0 | 0xF2 | 0xF3
        ) {
            break;
        }
        if matches!(b, 0x66 | 0xF2 | 0xF3) {
            last_legacy = Some(b);
        }
        i += 1;
    }
    let mut rex_w = false;
    if let Some(&rex @ 0x40..=0x4F) = insn.get(i) {
        rex_w = rex & 8 != 0;
        i += 1;
    }
    let rest = &insn[i..];
    let modrm_reg = |m: u8| m >> 3 & 7;
    let gap: Option<&'static str> = match rest {
        [0x62, p0, p1, _, opcode, tail @ ..] => {
            host.evex_gap(p0 & 7, p1 & 3, *opcode, tail.first().copied())
        }
        [0x0F, 0x01, 0xE8, ..] if !host.edx(14) => Some("SERIALIZE"),
        [0x0F, 0x01, 0xEE | 0xEF, ..] if !host.ecx(3) => Some("PKU (RDPKRU/WRPKRU)"),
        [0x0F, 0xAE, m, ..]
            if m >> 6 == 3 && modrm_reg(*m) == 6 && last_legacy.is_some() && !host.ecx(5) =>
        {
            Some("WAITPKG (UMONITOR/UMWAIT/TPAUSE)")
        }
        // Intel-defined behavior that other vendors implement differently:
        // RDPMC against the Intel architectural PMU, SYSENTER/SYSEXIT in
        // long mode, and far CALL/JMP through an m16:64 pointer (REX.W).
        [0x0F, 0x33, ..] if !host.intel => Some("an Intel PMU (RDPMC)"),
        [0x0F, 0x34 | 0x35, ..] if !host.intel => Some("Intel SYSENTER/SYSEXIT in 64-bit mode"),
        [0xFF, m, ..] if rex_w && m >> 6 != 3 && matches!(modrm_reg(*m), 3 | 5) && !host.intel => {
            Some("Intel far CALL/JMP m16:64")
        }
        _ => None,
    };
    gap.map(|what| {
        format!(
            "{HOST_UNSUPPORTED}the instruction at {rip:#x} ({:02x?}) needs {what}",
            &insn[..insn.len().min(16)]
        )
    })
}

/// Whether the host (and so the KVM guest) is an Intel CPU, for cases that
/// compare behavior the Intel SDM defines and other vendors implement
/// differently. Prints a skip line naming `case` otherwise.
pub(super) fn intel_host(case: &str) -> bool {
    let intel = HostCpu::new().intel;
    if !intel {
        eprintln!("[skip] {case}: Intel-specific behavior; the host CPU is not Intel");
    }
    intel
}
