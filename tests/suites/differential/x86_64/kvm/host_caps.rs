//! Attributing KVM shutdowns to host CPU capability.
//!
//! KVM executes the guest natively, so an instruction the host CPU does not
//! implement raises #UD (or #GP) and, with no IDT in the guest, ends in a
//! triple-fault shutdown that says nothing about the interpreter. KVM resets
//! the vCPU on shutdown, so the faulting RIP is not recoverable from KVM;
//! `unsupported` takes the instructions the interpreter executed for the
//! same case, with the registers they consumed, and reports the case as
//! skipped only when one of them needs a feature the host CPU does not
//! enumerate, or Intel-defined behavior that a non-Intel host implements
//! differently (rax models the Intel SDM). Every other shutdown remains a
//! failure.

/// Marks a backend error string as "this host cannot run the case".
pub(super) const HOST_UNSUPPORTED: &str = "host cannot run this case: ";

/// One instruction the interpreter executed: its address and the registers
/// that select its behavior where a feature check depends on them.
#[derive(Clone, Copy)]
pub(super) struct Step {
    pub rip: u64,
    pub rax: u64,
    pub rcx: u64,
    pub rdx: u64,
}

impl Step {
    /// The 64-bit value EDX:EAX.
    fn edx_eax(&self) -> u64 {
        u64::from(self.rdx as u32) << 32 | u64::from(self.rax as u32)
    }
}

/// The CPUID leaves the attribution needs.
struct HostCpu {
    intel: bool,
    /// CPUID.1: ECX.
    leaf1_ecx: u32,
    /// CPUID.(EAX=7,ECX=0): EBX, ECX, EDX.
    leaf7: [u32; 3],
    /// CPUID.(EAX=7,ECX=1): EAX, EDX.
    leaf7_1: [u32; 2],
    /// CPUID.(EAX=0DH,ECX=0): EDX:EAX, the XCR0 bits the host supports.
    xcr0_supported: u64,
    /// CPUID.(EAX=0DH,ECX=1): EAX.
    leaf_d_1_eax: u32,
}

impl HostCpu {
    /// The host's own CPUID; KVM's guest CPUID enumerates no feature the
    /// host lacks.
    #[cfg(target_arch = "x86_64")]
    #[allow(unused_unsafe)] // `__cpuid_count` is safe on newer toolchains.
    fn new() -> Self {
        use core::arch::x86_64::__cpuid_count;
        // SAFETY: CPUID is available on every x86-64 processor; each leaf is
        // read only when leaf 0 enumerates it.
        let leaf = |eax, ecx| unsafe { __cpuid_count(eax, ecx) };
        let leaf0 = leaf(0, 0);
        let max = leaf0.eax;
        let (leaf7, leaf7_1) = if max >= 7 {
            (leaf(7, 0), leaf(7, 1))
        } else {
            (leaf(0, 0), leaf(0, 0))
        };
        let (leaf_d, leaf_d_1) = if max >= 0xD {
            (leaf(0xD, 0), leaf(0xD, 1))
        } else {
            (leaf(0, 0), leaf(0, 0))
        };
        HostCpu {
            // "GenuineIntel" in EBX, EDX, ECX.
            intel: (leaf0.ebx, leaf0.edx, leaf0.ecx) == (0x756E_6547, 0x4965_6E69, 0x6C65_746E),
            leaf1_ecx: leaf(1, 0).ecx,
            leaf7: if max >= 7 {
                [leaf7.ebx, leaf7.ecx, leaf7.edx]
            } else {
                [0; 3]
            },
            leaf7_1: if max >= 7 {
                [leaf7_1.eax, leaf7_1.edx]
            } else {
                [0; 2]
            },
            xcr0_supported: if max >= 0xD {
                u64::from(leaf_d.eax) | u64::from(leaf_d.edx) << 32
            } else {
                0
            },
            leaf_d_1_eax: if max >= 0xD { leaf_d_1.eax } else { 0 },
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    fn new() -> Self {
        HostCpu {
            intel: false,
            leaf1_ecx: 0,
            leaf7: [0; 3],
            leaf7_1: [0; 2],
            xcr0_supported: 0,
            leaf_d_1_eax: 0,
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

    fn leaf7_1_eax(&self, bit: u32) -> bool {
        self.leaf7_1[0] >> bit & 1 != 0
    }

    fn leaf7_1_edx(&self, bit: u32) -> bool {
        self.leaf7_1[1] >> bit & 1 != 0
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
            (2, 2 | 3, 0x72) | (2, 2, 0x52) => Some(("AVX512_BF16", self.leaf7_1_eax(5))),
            _ => None,
        };
        match rare {
            Some((name, false)) => Some(name),
            Some((_, true)) => None,
            None if !self.common_avx512() => Some("an AVX-512 subset beyond AVX-512F"),
            None => None,
        }
    }

    /// What a VEX instruction in `map` with `pp`, `l`, and `opcode` needs
    /// that this host lacks (beyond AVX/AVX2, which every KVM runner has).
    fn vex_gap(&self, map: u8, pp: u8, l: bool, opcode: u8) -> Option<&'static str> {
        let (name, present) = match (map, pp, opcode) {
            // Opmask instructions (KMOV/KAND/KOR/KNOT/KXOR/KADD/KUNPCK/
            // KORTEST/KTEST and KSHIFT).
            (1, _, 0x41 | 0x42 | 0x44..=0x47 | 0x4A | 0x4B | 0x90..=0x93 | 0x98 | 0x99)
            | (3, _, 0x30..=0x33) => ("AVX-512 opmask instructions", self.ebx(16)),
            (2, 1, 0xCF) | (3, 1, 0xCE | 0xCF) => ("GFNI", self.ecx(8)),
            (2, 1, 0x50..=0x53) => ("AVX-VNNI", self.leaf7_1_eax(4)),
            (2, 0 | 2 | 3, 0x50 | 0x51) => ("AVX-VNNI-INT8", self.leaf7_1_edx(4)),
            (2, _, 0xD2 | 0xD3) => ("AVX-VNNI-INT16", self.leaf7_1_edx(10)),
            (2, 1, 0xB4 | 0xB5) => ("AVX-IFMA", self.leaf7_1_eax(23)),
            (2, _, 0xB0 | 0xB1) => ("AVX-NE-CONVERT", self.leaf7_1_edx(5)),
            (2, 3, 0xCB..=0xCD) => ("SHA512", self.leaf7_1_eax(0)),
            (3, 1, 0xDE) | (2, 0, 0xDA) => ("SM3", self.leaf7_1_eax(1)),
            (2, 2 | 3, 0xDA) => ("SM4", self.leaf7_1_eax(2)),
            (2, 1, 0xDC..=0xDF) if l => ("VAES", self.ecx(9)),
            (3, 1, 0x44) if l => ("VPCLMULQDQ", self.ecx(10)),
            _ => return None,
        };
        (!present).then_some(name)
    }
}

/// Why the host cannot run a case whose interpreter run executed `executed`
/// in `code` (loaded at `code_addr`): the first such instruction that needs a
/// feature the host lacks.
pub(super) fn unsupported(code: &[u8], code_addr: u64, executed: &[Step]) -> Option<String> {
    let host = HostCpu::new();
    executed
        .iter()
        .find_map(|step| instruction_gap(&host, code, code_addr, step))
}

fn instruction_gap(host: &HostCpu, code: &[u8], code_addr: u64, step: &Step) -> Option<String> {
    let rip = step.rip;
    let insn = code.get(usize::try_from(rip.checked_sub(code_addr)?).ok()?..)?;
    let mut i = 0;
    let mut mandatory = None;
    while let Some(&b) = insn.get(i) {
        if !matches!(
            b,
            0x26 | 0x2E | 0x36 | 0x3E | 0x64 | 0x65 | 0x66 | 0x67 | 0xF0 | 0xF2 | 0xF3
        ) {
            break;
        }
        if matches!(b, 0x66 | 0xF2 | 0xF3) {
            mandatory = Some(b);
        }
        i += 1;
    }
    let mut rex_w = false;
    if let Some(&rex @ 0x40..=0x4F) = insn.get(i) {
        rex_w = rex & 8 != 0;
        i += 1;
    }
    let rest = &insn[i..];
    let reg = |m: u8| m >> 3 & 7;
    let reg_form = |m: u8| m >> 6 == 3;
    let xsave_feature = |bit: u32| host.leaf_d_1_eax >> bit & 1 != 0;
    let gap: Option<String> = match rest {
        [0x62, p0, p1, _, opcode, tail @ ..] => host
            .evex_gap(p0 & 7, p1 & 3, *opcode, tail.first().copied())
            .map(str::to_string),
        // Two-byte VEX: map 1, R vvvv L pp.
        [0xC5, p, opcode, ..] => host
            .vex_gap(1, p & 3, p & 4 != 0, *opcode)
            .map(str::to_string),
        // Three-byte VEX: R X B mmmmm, W vvvv L pp.
        [0xC4, p0, p1, opcode, ..] => host
            .vex_gap(p0 & 0x1F, p1 & 3, p1 & 4 != 0, *opcode)
            .map(str::to_string),
        // XSETBV of XCR0 components the host does not support is #GP.
        [0x0F, 0x01, 0xD1, ..]
            if step.rcx as u32 == 0 && step.edx_eax() & !host.xcr0_supported != 0 =>
        {
            Some(format!(
                "XCR0 components {:#x} (XSETBV of {:#x})",
                step.edx_eax() & !host.xcr0_supported,
                step.edx_eax()
            ))
        }
        [0x0F, 0x01, 0xD0, ..] if step.rcx as u32 == 1 && !xsave_feature(2) => {
            Some("XGETBV with ECX=1 (XINUSE)".into())
        }
        [0x0F, 0x01, 0xE8, ..] if !host.edx(14) => Some("SERIALIZE".into()),
        [0x0F, 0x01, 0xEE | 0xEF, ..] if !host.ecx(3) => Some("PKU (RDPKRU/WRPKRU)".into()),
        [0x0F, 0xAE, m, ..] if reg_form(*m) && reg(*m) == 6 && mandatory.is_some() => {
            (!host.ecx(5)).then(|| "WAITPKG (UMONITOR/UMWAIT/TPAUSE)".into())
        }
        [0x0F, 0xAE, m, ..] if !reg_form(*m) && reg(*m) == 6 && mandatory.is_none() => {
            (!xsave_feature(0)).then(|| "XSAVEOPT".into())
        }
        [0x0F, 0xC7, m, ..] if !reg_form(*m) && reg(*m) == 4 => {
            (!xsave_feature(1)).then(|| "XSAVEC".into())
        }
        [0x0F, 0xC7, m, ..] if !reg_form(*m) && matches!(reg(*m), 3 | 5) => {
            (!xsave_feature(3)).then(|| "XSAVES/XRSTORS".into())
        }
        [0x0F, 0xC7, m, ..] if reg_form(*m) && reg(*m) == 6 => {
            (host.leaf1_ecx >> 30 & 1 == 0).then(|| "RDRAND".into())
        }
        [0x0F, 0xC7, m, ..] if reg_form(*m) && reg(*m) == 7 && mandatory.is_none() => {
            (!host.ebx(18)).then(|| "RDSEED".into())
        }
        [0x0F, 0x38, 0xF9, ..] if mandatory.is_none() => (!host.ecx(27)).then(|| "MOVDIRI".into()),
        [0x0F, 0x38, 0xF8, ..] if mandatory == Some(0x66) => {
            (!host.ecx(28)).then(|| "MOVDIR64B".into())
        }
        [0x0F, 0x38, 0xF8, ..] if mandatory.is_some() => (!host.ecx(29)).then(|| "ENQCMD".into()),
        [0x0F, 0x38, 0xCF, ..] | [0x0F, 0x3A, 0xCE | 0xCF, ..] if mandatory == Some(0x66) => {
            (!host.ecx(8)).then(|| "GFNI".into())
        }
        [0x0F, 0x38, 0xC8..=0xCD, ..] | [0x0F, 0x3A, 0xCC, ..] if mandatory.is_none() => {
            (!host.ebx(29)).then(|| "SHA".into())
        }
        [0x0F, 0x38, 0xDB..=0xDF, ..] | [0x0F, 0x3A, 0xDF, ..] if mandatory == Some(0x66) => {
            (host.leaf1_ecx >> 25 & 1 == 0).then(|| "AES-NI".into())
        }
        [0x0F, 0x3A, 0x44, ..] if mandatory == Some(0x66) => {
            (host.leaf1_ecx >> 1 & 1 == 0).then(|| "PCLMULQDQ".into())
        }
        // Intel-defined behavior that other vendors implement differently:
        // RDPMC against the Intel architectural PMU, SYSENTER/SYSEXIT in
        // long mode, and far CALL/JMP through an m16:64 pointer (REX.W).
        [0x0F, 0x33, ..] if !host.intel => Some("an Intel PMU (RDPMC)".into()),
        [0x0F, 0x34 | 0x35, ..] if !host.intel => {
            Some("Intel SYSENTER/SYSEXIT in 64-bit mode".into())
        }
        [0xFF, m, ..] if rex_w && !reg_form(*m) && matches!(reg(*m), 3 | 5) && !host.intel => {
            Some("Intel far CALL/JMP m16:64".into())
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
