//! AVX-512 differential tests: rax software interpreter vs. KVM (the silicon).
//!
//! This is the AVX-512 analogue of `differential.rs`. Where `differential.rs`
//! exercises scalar / SSE behaviour against KVM, this harness drives full
//! 512-bit EVEX state — all 32 ZMM registers and all 8 opmask (k) registers —
//! through identical machine code on the interpreter and on real hardware, then
//! diffs the resulting architectural state.
//!
//! Why this is the strong oracle: the `x86_64_evex_qemu_diff.rs` harness already
//! checks rax against qemu-x86_64, but qemu is itself an emulator. Now that the
//! build host has native AVX-512 (F/BW/CD/DQ/VL plus newer Xeon extensions),
//! the *chip* can be the reference, and silicon is the final word on x86
//! semantics.
//!
//! How state crosses the KVM boundary: rax models ZMM/opmask in its `Registers`
//! struct, so the interpreter side injects/extracts them directly. The KVM side
//! cannot — KVM_SET_REGS only covers GPRs — so it goes through the architectural
//! XSAVE area (`KVM_GET_XSAVE`/`KVM_SET_XSAVE`) plus `KVM_SET_XCRS` to enable the
//! AVX-512 XCR0 components and `KVM_SET_CPUID2` so the guest may use them at all.
//! The component byte offsets inside the XSAVE area are read from the host's own
//! CPUID(0xD) enumeration, so the layout always matches whatever silicon we run
//! on. Both backends then execute the *same* `mov rax, scratch; <op>; hlt`, and
//! the full final state is compared.
//!
//! Robustness:
//!  - Skips cleanly (no failure) when `/dev/kvm`, `KVM_*XSAVE*`, or the host's
//!    AVX-512 feature set is unavailable, so the suite stays green anywhere.
//!  - Only emits cases whose required AVX-512 subset the host actually
//!    implements, and only mnemonics rax claims to implement.
//!  - Bounded execution on both backends; a faulting case is reported, not hung.

#![cfg(all(feature = "kvm", target_os = "linux"))]
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "x86_64/common/mod.rs"]
mod common;

use common::{run_until_hlt, setup_vm, Bytes, GuestAddress, Registers};

// ---------------------------------------------------------------------------
// Wire model (mirrors x86_64_evex_qemu_diff.rs so the two corpora are directly
// comparable in shape: 32 ZMM regs, 8 opmask regs, a 256-byte scratch operand).
// ---------------------------------------------------------------------------

const ZMM_REGS: usize = 32;
const K_REGS: usize = 8;
const SCRATCH_BYTES: usize = 256;

/// Scratch / memory-operand region (64-byte aligned so EVEX aligned-move forms
/// and full-width broadcasts never #GP on the alignment check).
const SCRATCH_ADDR: u64 = 0x4000;

/// Initial RFLAGS (matches the EVEX qemu harness): IF + a spread of status bits
/// so flag-producing ops (vcomiss, ktest, ...) start from a non-trivial state.
const INITIAL_RFLAGS: u64 = 0x8d7;
const RFLAGS_CF: u64 = 0x001;
const RFLAGS_PF: u64 = 0x004;
const RFLAGS_AF: u64 = 0x010;
const RFLAGS_ZF: u64 = 0x040;
const RFLAGS_SF: u64 = 0x080;
const RFLAGS_IF: u64 = 0x200;
const RFLAGS_DF: u64 = 0x400;
const RFLAGS_OF: u64 = 0x800;
const RFLAGS_AC: u64 = 0x40000;
/// Architecturally-defined status bits to compare (the 6 arithmetic flags).
const STATUS_RFLAGS_MASK: u64 =
    RFLAGS_CF | RFLAGS_PF | RFLAGS_AF | RFLAGS_ZF | RFLAGS_SF | RFLAGS_OF;
/// Value seeded into r8 (GPR-source / GPR-dest EVEX and k<->GPR forms read it).
const R8_SEED: u64 = 0x8877_6655_4433_2211;
/// Small bit index for core BT/BTS/BTR/BTC register-indexed memory forms.
const R9_SEED: u64 = 70;
/// Value seeded into rcx; its low bits also drive BMI bit ranges / shift counts.
const RCX_SEED: u64 = 0x1020_3040_5060_0c04;
/// Value seeded into rdx, the implicit BMI2 MULX multiplicand.
const RDX_SEED: u64 = 0x1357_9bdf_2468_ace0;
/// Value seeded into rbx, used as an XLAT table base in core data-move cases.
const RBX_SEED: u64 = SCRATCH_ADDR + 16;
/// Stack/base-pointer seeds for core stack and addressing cases.
const RBP_SEED: u64 = STACK_ADDR + 0x80;
const RSP_SEED: u64 = STACK_ADDR;
const STACK_WINDOW_ADDR: u64 = STACK_ADDR - 64;
const STACK_BYTES: usize = 128;
/// Scratch-local source/destination windows for string instructions.
const STRING_SRC_ADDR: u64 = SCRATCH_ADDR + 128;
const STRING_DST_ADDR: u64 = SCRATCH_ADDR + 32;
const STRING_REP_COUNT: u64 = 4;
const STRING_DF_OFFSET: u64 = 24;

/// One concrete architectural input: register file + scratch memory.
#[derive(Clone)]
struct InCase {
    zmm: [[u64; 8]; ZMM_REGS],
    k: [u64; K_REGS],
    rbx: u64,
    rcx: u64,
    rdx: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64,
    rsp: u64,
    r8: u64,
    r9: u64,
    rflags: u64,
    scratch: [u8; SCRATCH_BYTES],
    stack: [u8; STACK_BYTES],
}

/// One captured architectural output.
#[derive(Clone, PartialEq, Eq)]
struct OutCase {
    zmm: [[u64; 8]; ZMM_REGS],
    k: [u64; K_REGS],
    rax: u64,
    rbx: u64,
    rcx: u64,
    rdx: u64,
    rsi: u64,
    rdi: u64,
    rbp: u64,
    rsp: u64,
    r8: u64,
    r9: u64,
    rflags: u64,
    scratch: [u8; SCRATCH_BYTES],
    stack: [u8; STACK_BYTES],
}

// ---------------------------------------------------------------------------
// Host AVX-512 feature detection. The silicon can only execute what it
// implements; everything else would #UD, so the corpus is gated on this.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Feat {
    /// Core scalar x86-64 instructions that do not require an optional CPUID bit.
    Core,
    /// AVX-512 Foundation (always required as a baseline for EVEX).
    F,
    /// Byte/Word integer ops.
    Bw,
    /// Doubleword/Quadword ops + extra converts.
    Dq,
    /// Conflict-detection (vplzcnt/vpconflict/vpbroadcastm).
    Cd,
    /// 128/256-bit EVEX width variants (orthogonal capability).
    Vl,
    /// Base AVX-512F cases that do not require an optional extension gate.
    Base,
    /// AVX VEX-encoded SIMD instructions.
    Avx,
    /// AVX2 VEX-encoded integer/memory SIMD instructions.
    Avx2,
    /// VEX-encoded FMA fused multiply-add/subtract instructions.
    Fma,
    /// FXSAVE/FXRSTOR and MXCSR state-management instructions.
    Fxsave,
    /// XGETBV/XSETBV and XSAVE/XRSTOR state-management instructions.
    Xsave,
    /// x87 FPU stack, arithmetic, and conversion instructions.
    X87,
    /// Legacy MMX packed-integer instructions.
    Mmx,
    /// ENTER/LEAVE stack-frame setup and teardown.
    StackFrame,
    /// Stack-based and privileged RFLAGS control instructions.
    FlagControl,
    /// CMPXCHG8B doubleword compare-and-exchange.
    Cx8,
    /// CMPXCHG16B quadword compare-and-exchange.
    Cx16,
    /// Load/store fence ordering instructions.
    Fence,
    /// CLFLUSH cache-line invalidation.
    Clflush,
    /// CLFLUSHOPT optimized cache-line invalidation.
    Clflushopt,
    /// CLWB cache-line writeback.
    Clwb,
    /// CLDEMOTE cache-line demotion hint.
    Cldemote,
    /// NOP, PAUSE, and PREFETCHh no-op/hint instructions.
    HintNop,
    /// PREFETCHW write-intent cache hint.
    Prefetchw,
    /// MONITOR address-range monitoring setup instruction.
    Monitor,
    /// FSGSBASE FS/GS base read/write instructions.
    Fsgsbase,
    /// Privileged control-register and machine-status-word instructions.
    ControlReg,
    /// Descriptor-table load/store instructions.
    DescriptorTable,
    /// Descriptor access-rights, limit, and permission-check instructions.
    DescriptorAccess,
    /// Model-specific register read/write instructions.
    Msr,
    /// Debug-register read/write instructions.
    DebugReg,
    /// Port I/O instructions and string I/O forms.
    Io,
    /// Fast system-call transition instructions.
    FastSyscall,
    /// CPUID processor-query instruction.
    Cpuid,
    /// RDPMC performance-monitor counter reads.
    Rdpmc,
    /// Privileged cache invalidation/writeback instructions without CPUID gates.
    CacheInvd,
    /// WBNOINVD cache writeback without invalidation.
    Wbnoinvd,
    /// INVLPG TLB invalidation by linear address.
    Invlpg,
    /// SMAP access-flag control instructions.
    Smap,
    /// Protection-key user access register instructions.
    Pku,
    /// SWAPGS GS-base exchange instruction.
    Swapgs,
    /// SERIALIZE instruction execution barrier.
    Serialize,
    /// WAITPKG user-level monitor/wait instructions.
    Waitpkg,
    /// RDPID processor ID read from IA32_TSC_AUX.
    Rdpid,
    /// RDRAND hardware random-number generation.
    Rdrand,
    /// RDSEED hardware seed generation.
    Rdseed,
    /// RDTSC time-stamp counter read.
    Tsc,
    /// RDTSCP ordered time-stamp counter and IA32_TSC_AUX read.
    Rdtscp,
    /// VEX-encoded AVX VNNI dot-product instructions.
    AvxVnni,
    /// Legacy SSE packed/scalar single-precision SIMD instructions.
    Sse,
    /// Legacy SSE2 packed/scalar double-precision SIMD instructions.
    Sse2,
    /// Legacy SSSE3 byte/word shuffle, horizontal, sign, and abs instructions.
    Ssse3,
    /// Legacy SSE4.1 blend and test instructions.
    Sse41,
    /// Legacy SSE4.2 compare and string instructions.
    Sse42,
    /// AES-NI legacy XMM crypto/key-schedule instructions.
    Aes,
    /// PCLMULQDQ legacy XMM carry-less multiplication.
    Pclmulqdq,
    /// F16C VEX half/single-precision conversion instructions.
    F16c,
    /// SHA-NI XMM crypto/message-schedule instructions.
    Sha,
    /// MOVDIRI direct stores from GPR to memory.
    Movdiri,
    /// MOVDIR64B 64-byte direct stores.
    Movdir64b,
    /// ADX dual-carry arithmetic (ADCX/ADOX).
    Adx,
    /// MOVBE endian-swapping loads/stores.
    Movbe,
    /// SSE4.2 CRC32C accumulator instructions.
    Crc32,
    /// POPCNT scalar population count.
    Popcnt,
    /// BMI1 scalar bit-manipulation instructions.
    Bmi1,
    /// BMI2 scalar bit-manipulation instructions.
    Bmi2,
    /// LZCNT scalar leading-zero count instruction.
    Lzcnt,
    /// AVX-512 Integer FMA (VPMADD52*).
    Ifma,
    /// AVX-512 VNNI dot-product instructions.
    Vnni,
    /// AVX-512 VBMI byte permutes / multishift.
    Vbmi,
    /// AVX-512 VBMI2 byte/word compress/expand and funnel shifts.
    Vbmi2,
    /// AVX-512 BITALG byte/word popcount and bit-shuffle-to-mask.
    Bitalg,
    /// AVX-512 VPOPCNTDQ dword/qword popcount.
    Vpopcntdq,
    /// AVX-512 BF16 dot-product and conversions.
    Bf16,
    /// AVX-512 FP16 packed/scalar half-precision operations.
    Fp16,
    /// GFNI EVEX vector forms.
    Gfni,
    /// VAES EVEX vector AES rounds.
    Vaes,
    /// VPCLMULQDQ EVEX carry-less multiplication.
    Vpclmulqdq,
}

impl Feat {
    fn name(self) -> &'static str {
        match self {
            Feat::Core => "core",
            Feat::F => "avx512f",
            Feat::Bw => "avx512bw",
            Feat::Dq => "avx512dq",
            Feat::Cd => "avx512cd",
            Feat::Vl => "avx512vl",
            Feat::Base => "base",
            Feat::Avx => "avx",
            Feat::Avx2 => "avx2",
            Feat::Fma => "fma",
            Feat::Fxsave => "fxsr",
            Feat::Xsave => "xsave",
            Feat::X87 => "x87",
            Feat::Mmx => "mmx",
            Feat::StackFrame => "stack_frame",
            Feat::FlagControl => "flag_control",
            Feat::Cx8 => "cx8",
            Feat::Cx16 => "cx16",
            Feat::Fence => "fence",
            Feat::Clflush => "clflush",
            Feat::Clflushopt => "clflushopt",
            Feat::Clwb => "clwb",
            Feat::Cldemote => "cldemote",
            Feat::HintNop => "hint_nop",
            Feat::Prefetchw => "prefetchw",
            Feat::Monitor => "monitor",
            Feat::Fsgsbase => "fsgsbase",
            Feat::ControlReg => "control_reg",
            Feat::DescriptorTable => "descriptor_table",
            Feat::DescriptorAccess => "descriptor_access",
            Feat::Msr => "msr",
            Feat::DebugReg => "debug_reg",
            Feat::Io => "io",
            Feat::FastSyscall => "fast_syscall",
            Feat::Cpuid => "cpuid",
            Feat::Rdpmc => "rdpmc",
            Feat::CacheInvd => "cache_invd",
            Feat::Wbnoinvd => "wbnoinvd",
            Feat::Invlpg => "invlpg",
            Feat::Smap => "smap",
            Feat::Pku => "pku",
            Feat::Swapgs => "swapgs",
            Feat::Serialize => "serialize",
            Feat::Waitpkg => "waitpkg",
            Feat::Rdpid => "rdpid",
            Feat::Rdrand => "rdrand",
            Feat::Rdseed => "rdseed",
            Feat::Tsc => "tsc",
            Feat::Rdtscp => "rdtscp",
            Feat::AvxVnni => "avx_vnni",
            Feat::Sse => "sse",
            Feat::Sse2 => "sse2",
            Feat::Ssse3 => "ssse3",
            Feat::Sse41 => "sse4_1",
            Feat::Sse42 => "sse4_2",
            Feat::Aes => "aes",
            Feat::Pclmulqdq => "pclmulqdq",
            Feat::F16c => "f16c",
            Feat::Sha => "sha_ni",
            Feat::Movdiri => "movdiri",
            Feat::Movdir64b => "movdir64b",
            Feat::Adx => "adx",
            Feat::Movbe => "movbe",
            Feat::Crc32 => "sse4_2_crc32",
            Feat::Popcnt => "popcnt",
            Feat::Bmi1 => "bmi1",
            Feat::Bmi2 => "bmi2",
            Feat::Lzcnt => "lzcnt",
            Feat::Ifma => "avx512ifma",
            Feat::Vnni => "avx512_vnni",
            Feat::Vbmi => "avx512vbmi",
            Feat::Vbmi2 => "avx512_vbmi2",
            Feat::Bitalg => "avx512_bitalg",
            Feat::Vpopcntdq => "avx512_vpopcntdq",
            Feat::Bf16 => "avx512_bf16",
            Feat::Fp16 => "avx512_fp16",
            Feat::Gfni => "gfni",
            Feat::Vaes => "vaes",
            Feat::Vpclmulqdq => "vpclmulqdq",
        }
    }

    fn expanded_xeon() -> &'static [Feat] {
        &[
            Feat::Core,
            Feat::Avx,
            Feat::Avx2,
            Feat::Fma,
            Feat::Fxsave,
            Feat::Xsave,
            Feat::X87,
            Feat::Mmx,
            Feat::StackFrame,
            Feat::FlagControl,
            Feat::Cx8,
            Feat::Cx16,
            Feat::Fence,
            Feat::Clflush,
            Feat::Clflushopt,
            Feat::Clwb,
            Feat::Cldemote,
            Feat::HintNop,
            Feat::Prefetchw,
            Feat::Monitor,
            Feat::Fsgsbase,
            Feat::ControlReg,
            Feat::DescriptorTable,
            Feat::DescriptorAccess,
            Feat::Msr,
            Feat::DebugReg,
            Feat::Io,
            Feat::FastSyscall,
            Feat::Cpuid,
            Feat::Rdpmc,
            Feat::CacheInvd,
            Feat::Wbnoinvd,
            Feat::Invlpg,
            Feat::Smap,
            Feat::Pku,
            Feat::Swapgs,
            Feat::Serialize,
            Feat::Waitpkg,
            Feat::Rdpid,
            Feat::Rdrand,
            Feat::Rdseed,
            Feat::Tsc,
            Feat::Rdtscp,
            Feat::AvxVnni,
            Feat::Sse,
            Feat::Sse2,
            Feat::Ssse3,
            Feat::Sse41,
            Feat::Sse42,
            Feat::Aes,
            Feat::Pclmulqdq,
            Feat::F16c,
            Feat::Sha,
            Feat::Movdiri,
            Feat::Movdir64b,
            Feat::Adx,
            Feat::Movbe,
            Feat::Crc32,
            Feat::Popcnt,
            Feat::Bmi1,
            Feat::Bmi2,
            Feat::Lzcnt,
            Feat::Ifma,
            Feat::Vnni,
            Feat::Vbmi,
            Feat::Vbmi2,
            Feat::Bitalg,
            Feat::Vpopcntdq,
            Feat::Bf16,
            Feat::Fp16,
            Feat::Gfni,
            Feat::Vaes,
            Feat::Vpclmulqdq,
        ]
    }
}

struct HostFeatures {
    f: bool,
    bw: bool,
    dq: bool,
    cd: bool,
    vl: bool,
    avx: bool,
    avx2: bool,
    fma: bool,
    fxsave: bool,
    xsave: bool,
    mmx: bool,
    cx8: bool,
    cx16: bool,
    fence: bool,
    clflush: bool,
    clflushopt: bool,
    clwb: bool,
    cldemote: bool,
    prefetchw: bool,
    monitor: bool,
    fsgsbase: bool,
    wbnoinvd: bool,
    smap: bool,
    pku: bool,
    serialize: bool,
    waitpkg: bool,
    rdpid: bool,
    rdrand: bool,
    rdseed: bool,
    tsc: bool,
    rdtscp: bool,
    syscall: bool,
    sep: bool,
    rdpmc: bool,
    avx_vnni: bool,
    sse: bool,
    sse2: bool,
    ssse3: bool,
    sse4_1: bool,
    aes: bool,
    pclmulqdq: bool,
    f16c: bool,
    sha: bool,
    movdiri: bool,
    movdir64b: bool,
    adx: bool,
    movbe: bool,
    sse4_2: bool,
    popcnt: bool,
    bmi1: bool,
    bmi2: bool,
    lzcnt: bool,
    ifma: bool,
    vnni: bool,
    vbmi: bool,
    vbmi2: bool,
    bitalg: bool,
    vpopcntdq: bool,
    bf16: bool,
    fp16: bool,
    gfni: bool,
    vaes: bool,
    vpclmulqdq: bool,
}

impl HostFeatures {
    fn detect() -> Self {
        HostFeatures {
            f: is_x86_feature_detected!("avx512f"),
            bw: is_x86_feature_detected!("avx512bw"),
            dq: is_x86_feature_detected!("avx512dq"),
            cd: is_x86_feature_detected!("avx512cd"),
            vl: is_x86_feature_detected!("avx512vl"),
            avx: is_x86_feature_detected!("avx"),
            avx2: is_x86_feature_detected!("avx2"),
            fma: is_x86_feature_detected!("fma"),
            fxsave: host_cpu_flag("fxsr"),
            xsave: host_cpu_flag("xsave"),
            mmx: host_cpu_flag("mmx"),
            cx8: host_cpu_flag("cx8"),
            cx16: host_cpu_flag("cx16"),
            fence: is_x86_feature_detected!("sse2"),
            clflush: host_cpu_flag("clflush"),
            clflushopt: host_cpu_flag("clflushopt"),
            clwb: host_cpu_flag("clwb"),
            cldemote: host_cpu_flag("cldemote"),
            prefetchw: host_cpu_flag("3dnowprefetch") || host_cpu_flag("prefetchw"),
            monitor: host_cpu_flag("monitor"),
            fsgsbase: host_cpu_flag("fsgsbase"),
            wbnoinvd: host_cpu_flag("wbnoinvd"),
            smap: host_cpu_flag("smap"),
            pku: host_cpu_flag("pku"),
            serialize: host_cpu_flag("serialize"),
            waitpkg: host_cpu_flag("waitpkg"),
            rdpid: host_cpu_flag("rdpid"),
            rdrand: host_cpu_flag("rdrand"),
            rdseed: host_cpu_flag("rdseed"),
            tsc: host_cpu_flag("tsc"),
            rdtscp: host_cpu_flag("rdtscp"),
            syscall: host_cpu_flag("syscall"),
            sep: host_cpu_flag("sep"),
            rdpmc: host_cpu_flag("arch_perfmon") && host_kvm_pmu_enabled(),
            avx_vnni: host_cpu_flag("avx_vnni"),
            sse: is_x86_feature_detected!("sse"),
            sse2: is_x86_feature_detected!("sse2"),
            ssse3: is_x86_feature_detected!("ssse3"),
            sse4_1: is_x86_feature_detected!("sse4.1"),
            aes: host_cpu_flag("aes"),
            pclmulqdq: host_cpu_flag("pclmulqdq"),
            f16c: host_cpu_flag("f16c"),
            sha: host_cpu_flag("sha_ni"),
            movdiri: host_cpu_flag("movdiri"),
            movdir64b: host_cpu_flag("movdir64b"),
            adx: host_cpu_flag("adx"),
            movbe: host_cpu_flag("movbe"),
            sse4_2: is_x86_feature_detected!("sse4.2"),
            popcnt: host_cpu_flag("popcnt"),
            bmi1: host_cpu_flag("bmi1"),
            bmi2: host_cpu_flag("bmi2"),
            lzcnt: host_cpu_flag("lzcnt") || host_cpu_flag("abm"),
            ifma: host_cpu_flag("avx512ifma"),
            vnni: host_cpu_flag("avx512_vnni"),
            vbmi: host_cpu_flag("avx512vbmi"),
            vbmi2: host_cpu_flag("avx512_vbmi2"),
            bitalg: host_cpu_flag("avx512_bitalg"),
            vpopcntdq: host_cpu_flag("avx512_vpopcntdq"),
            bf16: host_cpu_flag("avx512_bf16"),
            fp16: host_cpu_flag("avx512_fp16"),
            gfni: host_cpu_flag("gfni"),
            vaes: host_cpu_flag("vaes"),
            vpclmulqdq: host_cpu_flag("vpclmulqdq"),
        }
    }

    fn supports(&self, feat: Feat) -> bool {
        match feat {
            Feat::Core => true,
            Feat::F | Feat::Base => self.f,
            Feat::Bw => self.bw,
            Feat::Dq => self.dq,
            Feat::Cd => self.cd,
            Feat::Vl => self.vl,
            Feat::Avx => self.avx,
            Feat::Avx2 => self.avx2,
            Feat::Fma => self.fma,
            Feat::Fxsave => self.fxsave,
            Feat::Xsave => self.xsave,
            Feat::X87 => true,
            Feat::Mmx => self.mmx,
            Feat::StackFrame => true,
            Feat::FlagControl => true,
            Feat::Cx8 => self.cx8,
            Feat::Cx16 => self.cx16,
            Feat::Fence => self.fence,
            Feat::Clflush => self.clflush,
            Feat::Clflushopt => self.clflushopt,
            Feat::Clwb => self.clwb,
            Feat::Cldemote => self.cldemote,
            Feat::HintNop => true,
            Feat::Prefetchw => self.prefetchw,
            Feat::Monitor => self.monitor,
            Feat::Fsgsbase => self.fsgsbase,
            Feat::ControlReg => true,
            Feat::DescriptorTable => true,
            Feat::DescriptorAccess => true,
            Feat::Msr => true,
            Feat::DebugReg => true,
            Feat::Io => true,
            Feat::FastSyscall => self.syscall && self.sep,
            Feat::Cpuid => true,
            Feat::Rdpmc => self.rdpmc,
            Feat::CacheInvd => true,
            Feat::Wbnoinvd => self.wbnoinvd,
            Feat::Invlpg => true,
            Feat::Smap => self.smap,
            Feat::Pku => self.pku,
            Feat::Swapgs => true,
            Feat::Serialize => self.serialize,
            Feat::Waitpkg => self.waitpkg,
            Feat::Rdpid => self.rdpid,
            Feat::Rdrand => self.rdrand,
            Feat::Rdseed => self.rdseed,
            Feat::Tsc => self.tsc,
            Feat::Rdtscp => self.rdtscp,
            Feat::AvxVnni => self.avx_vnni,
            Feat::Sse => self.sse,
            Feat::Sse2 => self.sse2,
            Feat::Ssse3 => self.ssse3,
            Feat::Sse41 => self.sse4_1,
            Feat::Sse42 => self.sse4_2,
            Feat::Aes => self.aes,
            Feat::Pclmulqdq => self.pclmulqdq,
            Feat::F16c => self.f16c,
            Feat::Sha => self.sha,
            Feat::Movdiri => self.movdiri,
            Feat::Movdir64b => self.movdir64b,
            Feat::Adx => self.adx,
            Feat::Movbe => self.movbe,
            Feat::Crc32 => self.sse4_2,
            Feat::Popcnt => self.popcnt,
            Feat::Bmi1 => self.bmi1,
            Feat::Bmi2 => self.bmi2,
            Feat::Lzcnt => self.lzcnt,
            Feat::Ifma => self.ifma,
            Feat::Vnni => self.vnni,
            Feat::Vbmi => self.vbmi,
            Feat::Vbmi2 => self.vbmi2,
            Feat::Bitalg => self.bitalg,
            Feat::Vpopcntdq => self.vpopcntdq,
            Feat::Bf16 => self.bf16,
            Feat::Fp16 => self.fp16,
            Feat::Gfni => self.gfni,
            Feat::Vaes => self.vaes,
            Feat::Vpclmulqdq => self.vpclmulqdq,
        }
    }
}

fn host_cpu_flag(flag: &str) -> bool {
    static FLAGS: OnceLock<String> = OnceLock::new();
    let flags = FLAGS.get_or_init(|| {
        std::fs::read_to_string("/proc/cpuinfo")
            .ok()
            .and_then(|text| {
                text.lines()
                    .find(|line| line.starts_with("flags"))
                    .map(str::to_string)
            })
            .unwrap_or_default()
    });
    flags.split_whitespace().any(|word| word == flag)
}

fn host_kvm_pmu_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::fs::read_to_string("/sys/module/kvm/parameters/enable_pmu")
            .map(|text| matches!(text.trim(), "Y" | "y" | "1"))
            .unwrap_or(true)
    })
}

// ---------------------------------------------------------------------------
// XSAVE area layout, taken from the host's own CPUID(0xD) enumeration so the
// offsets always match the silicon KVM is running on. The standard
// (non-compacted) format is fixed for a given CPU; KVM_GET/SET_XSAVE use it.
// ---------------------------------------------------------------------------

/// Byte offset of XMM0 inside the legacy region (architecturally fixed).
const XSAVE_XMM_OFFSET: usize = 160;
/// Byte offset of the XSAVE header's xstate_bv field.
const XSAVE_XSTATE_BV_OFFSET: usize = 512;

/// XCR0 component bits we drive: x87(0) | SSE(1) | AVX(2) | opmask(5) |
/// ZMM_Hi256(6) | Hi16_ZMM(7).
const XCR0_AVX512: u64 = 0b1110_0111;

#[derive(Clone, Copy)]
struct XsaveLayout {
    /// AVX (YMM_Hi128) component, 16 regs x 16 bytes. CPUID(0xD,2).
    avx: usize,
    /// Opmask component, 8 regs x 8 bytes. CPUID(0xD,5).
    opmask: usize,
    /// ZMM_Hi256 component (upper 256 bits of zmm0-15), 16 x 32 bytes. CPUID(0xD,6).
    zmm_hi: usize,
    /// Hi16_ZMM component (full zmm16-31), 16 x 64 bytes. CPUID(0xD,7).
    hi16: usize,
}

impl XsaveLayout {
    fn from_host_cpuid() -> Self {
        use std::arch::x86_64::__cpuid_count;
        // leaf 0xD is always valid on a host advertising XSAVE/AVX-512, which we
        // have already feature-gated on before reaching here.
        XsaveLayout {
            avx: __cpuid_count(0xD, 2).ebx as usize,
            opmask: __cpuid_count(0xD, 5).ebx as usize,
            zmm_hi: __cpuid_count(0xD, 6).ebx as usize,
            hi16: __cpuid_count(0xD, 7).ebx as usize,
        }
    }

    /// Patch the full ZMM/opmask state into a (host-formatted) XSAVE byte area.
    fn store_state(&self, area: &mut [u8], zmm: &[[u64; 8]; ZMM_REGS], k: &[u64; K_REGS]) {
        let put = |area: &mut [u8], off: usize, w: u64| {
            area[off..off + 8].copy_from_slice(&w.to_le_bytes());
        };
        for i in 0..16 {
            // bits 127:0 -> legacy XMM
            put(area, XSAVE_XMM_OFFSET + i * 16, zmm[i][0]);
            put(area, XSAVE_XMM_OFFSET + i * 16 + 8, zmm[i][1]);
            // bits 255:128 -> AVX component
            put(area, self.avx + i * 16, zmm[i][2]);
            put(area, self.avx + i * 16 + 8, zmm[i][3]);
            // bits 511:256 -> ZMM_Hi256 component
            for w in 0..4 {
                put(area, self.zmm_hi + i * 32 + w * 8, zmm[i][4 + w]);
            }
        }
        for i in 0..16 {
            // zmm16..31 full 512 bits -> Hi16_ZMM component
            for w in 0..8 {
                put(area, self.hi16 + i * 64 + w * 8, zmm[16 + i][w]);
            }
        }
        for i in 0..K_REGS {
            put(area, self.opmask + i * 8, k[i]);
        }
        // Mark every component we just wrote as in-use so XRSTOR honours it.
        put(area, XSAVE_XSTATE_BV_OFFSET, XCR0_AVX512);
    }

    /// Read the full ZMM/opmask state back out of a host-formatted XSAVE area.
    fn load_state(&self, area: &[u8]) -> ([[u64; 8]; ZMM_REGS], [u64; K_REGS]) {
        let get = |area: &[u8], off: usize| -> u64 {
            u64::from_le_bytes(area[off..off + 8].try_into().unwrap())
        };
        let mut zmm = [[0u64; 8]; ZMM_REGS];
        let mut k = [0u64; K_REGS];
        for i in 0..16 {
            zmm[i][0] = get(area, XSAVE_XMM_OFFSET + i * 16);
            zmm[i][1] = get(area, XSAVE_XMM_OFFSET + i * 16 + 8);
            zmm[i][2] = get(area, self.avx + i * 16);
            zmm[i][3] = get(area, self.avx + i * 16 + 8);
            for w in 0..4 {
                zmm[i][4 + w] = get(area, self.zmm_hi + i * 32 + w * 8);
            }
        }
        for i in 0..16 {
            for w in 0..8 {
                zmm[16 + i][w] = get(area, self.hi16 + i * 64 + w * 8);
            }
        }
        for i in 0..K_REGS {
            k[i] = get(area, self.opmask + i * 8);
        }
        (zmm, k)
    }
}

// ---------------------------------------------------------------------------
// KVM guest memory layout (identity-mapped: GVA == GPA).
// ---------------------------------------------------------------------------

const MEM_SIZE: usize = 8 * 1024 * 1024;
const PML4_ADDR: u64 = 0x1000;
const PDPTE_ADDR: u64 = 0x2000;
const CODE_ADDR: u64 = 0x10000;
const STACK_ADDR: u64 = 0x20000;

const CR0_PE: u64 = 1 << 0;
const CR0_MP: u64 = 1 << 1;
const CR0_ET: u64 = 1 << 4;
const CR0_NE: u64 = 1 << 5;
const CR0_WP: u64 = 1 << 16;
const CR0_PG: u64 = 1 << 31;
const CR0_VAL: u64 = CR0_PE | CR0_MP | CR0_ET | CR0_NE | CR0_WP | CR0_PG;
const CR4_PAE: u64 = 1 << 5;
const CR4_FSGSBASE: u64 = 1 << 16;
const CR4_OSFXSR: u64 = 1 << 9;
const CR4_OSXMMEXCPT: u64 = 1 << 10;
const CR4_OSXSAVE: u64 = 1 << 18;
const CR4_VAL: u64 = CR4_PAE | CR4_FSGSBASE | CR4_OSFXSR | CR4_OSXMMEXCPT | CR4_OSXSAVE;
const EFER_LME: u64 = 1 << 8;
const EFER_LMA: u64 = 1 << 10;
const EFER_VAL: u64 = EFER_LME | EFER_LMA;

const MAX_ITERS: u64 = 10_000;

// ---------------------------------------------------------------------------
// KVM oracle. One Kvm handle + supported CPUID is shared across all cases; a
// fresh VM/vCPU/memory is built per case so no state can leak between them.
// ---------------------------------------------------------------------------

/// Owns the mmap backing KVM guest memory.
struct KvmMem {
    ptr: *mut u8,
    size: usize,
}

impl KvmMem {
    fn new(size: usize) -> Option<Self> {
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_NORESERVE,
                -1,
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return None;
        }
        Some(KvmMem {
            ptr: ptr as *mut u8,
            size,
        })
    }

    fn write(&self, addr: u64, bytes: &[u8]) {
        assert!(addr as usize + bytes.len() <= self.size);
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.ptr.add(addr as usize), bytes.len());
        }
    }

    fn read(&self, addr: u64, out: &mut [u8]) {
        assert!(addr as usize + out.len() <= self.size);
        unsafe {
            std::ptr::copy_nonoverlapping(self.ptr.add(addr as usize), out.as_mut_ptr(), out.len());
        }
    }
}

impl Drop for KvmMem {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr as *mut libc::c_void, self.size);
        }
    }
}

/// Result of running a case on the silicon.
enum KvmOutcome {
    /// The instruction executed to the trailing HLT; here is the final state.
    Ran(OutCase),
    /// The instruction faulted on the silicon (e.g. #UD): a triple fault took
    /// the guest down. Not comparable; the case is skipped + counted.
    Faulted,
}

/// Lazily-initialised, process-wide KVM context. `None` once we determine KVM
/// is unusable so every subsequent case skips without re-probing.
struct KvmOracle {
    kvm: kvm_ioctls::Kvm,
    supported_cpuid: kvm_bindings::CpuId,
    layout: XsaveLayout,
}

fn install_page_tables(mem: &KvmMem) {
    // PML4[0] -> PDPTE (present + writable + user). The user bit lets
    // SYSRET/SYSEXIT ring-3 trampolines fetch instructions before immediately
    // returning to ring 0 through SYSCALL.
    mem.write(PML4_ADDR, &(PDPTE_ADDR | 0x7).to_le_bytes());
    // PDPTE[i] identity 1GiB huge pages (present + writable + user + PS).
    for i in 0u64..4 {
        let entry: u64 = (i << 30) | 0x87;
        mem.write(PDPTE_ADDR + i * 8, &entry.to_le_bytes());
    }
}

impl KvmOracle {
    /// Build the shared oracle, or `None` if KVM / XSAVE / AVX-512 is unusable.
    fn try_new() -> Option<KvmOracle> {
        use kvm_bindings::KVM_MAX_CPUID_ENTRIES;
        use kvm_ioctls::Kvm;

        let kvm = Kvm::new().ok()?;
        // KVM_GET_SUPPORTED_CPUID reflects host capabilities, so on an AVX-512
        // host it advertises the AVX-512 leaves + XSAVE components the guest
        // needs in order to set XCR0 and execute EVEX without faulting.
        let supported_cpuid = kvm.get_supported_cpuid(KVM_MAX_CPUID_ENTRIES).ok()?;
        let layout = XsaveLayout::from_host_cpuid();
        Some(KvmOracle {
            kvm,
            supported_cpuid,
            layout,
        })
    }

    /// Run `code` from `input`, returning the final architectural state.
    fn run(&self, code: &[u8], input: &InCase) -> Result<KvmOutcome, String> {
        use kvm_bindings::{kvm_segment, kvm_userspace_memory_region};

        let vm = self
            .kvm
            .create_vm()
            .map_err(|e| format!("create_vm: {e:?}"))?;
        let mem = KvmMem::new(MEM_SIZE).ok_or("mmap guest memory failed")?;

        install_page_tables(&mem);
        mem.write(CODE_ADDR, code);
        mem.write(SCRATCH_ADDR, &input.scratch);
        mem.write(STACK_WINDOW_ADDR, &input.stack);

        let region = kvm_userspace_memory_region {
            slot: 0,
            guest_phys_addr: 0,
            memory_size: MEM_SIZE as u64,
            userspace_addr: mem.ptr as u64,
            flags: 0,
        };
        unsafe { vm.set_user_memory_region(region) }.map_err(|e| format!("set_memory: {e:?}"))?;

        let mut vcpu = vm
            .create_vcpu(0)
            .map_err(|e| format!("create_vcpu: {e:?}"))?;

        // Guest CPUID first: XCR0 validation and EVEX legality depend on it.
        vcpu.set_cpuid2(&self.supported_cpuid)
            .map_err(|e| format!("set_cpuid2: {e:?}"))?;

        // --- sregs: long mode, paging, flat 64-bit segments, CR4.OSXSAVE ---
        let mut sregs = vcpu.get_sregs().map_err(|e| format!("get_sregs: {e:?}"))?;
        let flat_code = kvm_segment {
            base: 0,
            limit: 0xFFFFF,
            selector: 0x8,
            type_: 0xB,
            present: 1,
            dpl: 0,
            db: 0,
            s: 1,
            l: 1,
            g: 1,
            avl: 0,
            unusable: 0,
            padding: 0,
        };
        let mut flat_data = flat_code;
        flat_data.selector = 0x10;
        flat_data.type_ = 0x3;
        flat_data.l = 0;
        flat_data.db = 1;
        sregs.cr0 = CR0_VAL;
        sregs.cr3 = PML4_ADDR;
        sregs.cr4 = CR4_VAL;
        sregs.efer = EFER_VAL;
        sregs.cs = flat_code;
        sregs.ds = flat_data;
        sregs.es = flat_data;
        sregs.fs = flat_data;
        sregs.gs = flat_data;
        sregs.ss = flat_data;
        vcpu.set_sregs(&sregs)
            .map_err(|e| format!("set_sregs: {e:?}"))?;

        // --- XCR0: enable the AVX-512 state components ---
        let mut xcrs = kvm_bindings::kvm_xcrs::default();
        xcrs.nr_xcrs = 1;
        xcrs.xcrs[0].xcr = 0;
        xcrs.xcrs[0].value = XCR0_AVX512;
        vcpu.set_xcrs(&xcrs)
            .map_err(|e| format!("set_xcrs: {e:?}"))?;

        // --- ZMM + opmask: inject via the XSAVE area ---
        // Start from a live, valid area (correct MXCSR, xcomp_bv, ...) and patch.
        let mut xsave = vcpu.get_xsave().map_err(|e| format!("get_xsave: {e:?}"))?;
        {
            let area = xsave_bytes_mut(&mut xsave);
            self.layout.store_state(area, &input.zmm, &input.k);
        }
        // SAFETY: `xsave` is a well-formed kvm_xsave whose xstate_bv ⊆ XCR0.
        unsafe { vcpu.set_xsave(&xsave) }.map_err(|e| format!("set_xsave: {e:?}"))?;

        // --- GPRs + RFLAGS ---
        let mut kregs = vcpu.get_regs().map_err(|e| format!("get_regs: {e:?}"))?;
        kregs.rip = CODE_ADDR;
        kregs.rbx = input.rbx;
        kregs.rcx = input.rcx;
        kregs.rdx = input.rdx;
        kregs.rsi = input.rsi;
        kregs.rdi = input.rdi;
        kregs.rbp = input.rbp;
        kregs.rsp = input.rsp;
        kregs.r8 = input.r8;
        kregs.r9 = input.r9;
        kregs.rflags = input.rflags | 0x2;
        vcpu.set_regs(&kregs)
            .map_err(|e| format!("set_regs: {e:?}"))?;

        // --- run, bounded, to the trailing HLT ---
        let mut iters = 0u64;
        loop {
            iters += 1;
            if iters > MAX_ITERS {
                return Err(format!("kvm exceeded {MAX_ITERS} iterations"));
            }
            match vcpu.run().map_err(|e| format!("kvm run: {e:?}"))? {
                kvm_ioctls::VcpuExit::Hlt => break,
                kvm_ioctls::VcpuExit::IoIn(_, data) => data.iter_mut().for_each(|b| *b = 0),
                kvm_ioctls::VcpuExit::IoOut(..) => {}
                // A faulting EVEX op with no usable IDT triple-faults the guest.
                kvm_ioctls::VcpuExit::Shutdown
                | kvm_ioctls::VcpuExit::FailEntry(..)
                | kvm_ioctls::VcpuExit::InternalError => return Ok(KvmOutcome::Faulted),
                other => return Err(format!("kvm abnormal exit: {other:?}")),
            }
        }

        // --- extract final state ---
        let final_regs = vcpu
            .get_regs()
            .map_err(|e| format!("get_regs(final): {e:?}"))?;
        let final_xsave = vcpu
            .get_xsave()
            .map_err(|e| format!("get_xsave(final): {e:?}"))?;
        let (zmm, k) = self.layout.load_state(xsave_bytes(&final_xsave));

        let mut scratch = [0u8; SCRATCH_BYTES];
        mem.read(SCRATCH_ADDR, &mut scratch);
        let mut stack = [0u8; STACK_BYTES];
        mem.read(STACK_WINDOW_ADDR, &mut stack);

        Ok(KvmOutcome::Ran(OutCase {
            zmm,
            k,
            rax: final_regs.rax,
            rbx: final_regs.rbx,
            rcx: final_regs.rcx,
            rdx: final_regs.rdx,
            rsi: final_regs.rsi,
            rdi: final_regs.rdi,
            rbp: final_regs.rbp,
            rsp: final_regs.rsp,
            r8: final_regs.r8,
            r9: final_regs.r9,
            rflags: final_regs.rflags,
            scratch,
            stack,
        }))
    }
}

fn xsave_bytes(x: &kvm_bindings::kvm_xsave) -> &[u8] {
    // SAFETY: kvm_xsave is `region: [u32; 1024]`, a plain 4096-byte POD blob.
    unsafe { std::slice::from_raw_parts(x.region.as_ptr() as *const u8, x.region.len() * 4) }
}

fn xsave_bytes_mut(x: &mut kvm_bindings::kvm_xsave) -> &mut [u8] {
    unsafe { std::slice::from_raw_parts_mut(x.region.as_mut_ptr() as *mut u8, x.region.len() * 4) }
}

// ---------------------------------------------------------------------------
// Interpreter side: inject ZMM/opmask via rax's `Registers`, run the same code.
// ---------------------------------------------------------------------------

fn set_regs_zmm(regs: &mut Registers, index: usize, value: [u64; 8]) {
    if index < 16 {
        regs.xmm[index] = [value[0], value[1]];
        regs.ymm_high[index] = [value[2], value[3]];
        regs.zmm_high[index] = [value[4], value[5], value[6], value[7]];
    } else {
        regs.zmm_ext[index - 16] = value;
    }
}

fn get_regs_zmm(regs: &Registers, index: usize) -> [u64; 8] {
    if index < 16 {
        [
            regs.xmm[index][0],
            regs.xmm[index][1],
            regs.ymm_high[index][0],
            regs.ymm_high[index][1],
            regs.zmm_high[index][0],
            regs.zmm_high[index][1],
            regs.zmm_high[index][2],
            regs.zmm_high[index][3],
        ]
    } else {
        regs.zmm_ext[index - 16]
    }
}

fn run_interp(code: &[u8], input: &InCase) -> Result<OutCase, String> {
    let mut regs = Registers {
        rbx: input.rbx,
        rcx: input.rcx,
        rdx: input.rdx,
        rsi: input.rsi,
        rdi: input.rdi,
        rbp: input.rbp,
        rsp: input.rsp,
        r8: input.r8,
        r9: input.r9,
        rflags: input.rflags,
        ..Registers::default()
    };
    for reg in 0..ZMM_REGS {
        set_regs_zmm(&mut regs, reg, input.zmm[reg]);
    }
    regs.k = input.k;

    let (mut vcpu, mem) = setup_vm(code, Some(regs));
    mem.write_slice(&input.scratch, GuestAddress(SCRATCH_ADDR))
        .map_err(|e| format!("write scratch: {e:?}"))?;
    mem.write_slice(&input.stack, GuestAddress(STACK_WINDOW_ADDR))
        .map_err(|e| format!("write stack: {e:?}"))?;
    let out_regs = run_until_hlt(&mut vcpu).map_err(|e| format!("interp run: {e:?}"))?;

    let mut scratch = [0u8; SCRATCH_BYTES];
    mem.read_slice(&mut scratch, GuestAddress(SCRATCH_ADDR))
        .map_err(|e| format!("read scratch: {e:?}"))?;
    let mut stack = [0u8; STACK_BYTES];
    mem.read_slice(&mut stack, GuestAddress(STACK_WINDOW_ADDR))
        .map_err(|e| format!("read stack: {e:?}"))?;

    let mut zmm = [[0u64; 8]; ZMM_REGS];
    for reg in 0..ZMM_REGS {
        zmm[reg] = get_regs_zmm(&out_regs, reg);
    }
    Ok(OutCase {
        zmm,
        k: out_regs.k,
        rax: out_regs.rax,
        rbx: out_regs.rbx,
        rcx: out_regs.rcx,
        rdx: out_regs.rdx,
        rsi: out_regs.rsi,
        rdi: out_regs.rdi,
        rbp: out_regs.rbp,
        rsp: out_regs.rsp,
        r8: out_regs.r8,
        r9: out_regs.r9,
        rflags: out_regs.rflags,
        scratch,
        stack,
    })
}

// ---------------------------------------------------------------------------
// Code emission: the identical `mov rax, scratch; <op>; hlt` both sides run.
// ---------------------------------------------------------------------------

fn build_code(op: &[u8]) -> Vec<u8> {
    let mut code = Vec::new();
    // mov rax, imm64(SCRATCH_ADDR)
    code.push(0x48);
    code.push(0xb8);
    code.extend_from_slice(&SCRATCH_ADDR.to_le_bytes());
    code.extend_from_slice(op);
    code.push(0xf4); // hlt
    code
}

// ---------------------------------------------------------------------------
// Comparison.
// ---------------------------------------------------------------------------

fn diff(interp: &OutCase, kvm: &OutCase, rflags_mask: u64) -> Vec<String> {
    let mut diffs = Vec::new();
    for i in 0..ZMM_REGS {
        if interp.zmm[i] != kvm.zmm[i] {
            diffs.push(format!(
                "zmm{i}: interp={:016x?} kvm={:016x?}",
                interp.zmm[i], kvm.zmm[i]
            ));
        }
    }
    for i in 0..K_REGS {
        if interp.k[i] != kvm.k[i] {
            diffs.push(format!(
                "k{i}: interp={:#018x} kvm={:#018x}",
                interp.k[i], kvm.k[i]
            ));
        }
    }
    if interp.rax != kvm.rax {
        diffs.push(format!("rax: interp={:#x} kvm={:#x}", interp.rax, kvm.rax));
    }
    if interp.rbx != kvm.rbx {
        diffs.push(format!("rbx: interp={:#x} kvm={:#x}", interp.rbx, kvm.rbx));
    }
    if interp.rcx != kvm.rcx {
        diffs.push(format!("rcx: interp={:#x} kvm={:#x}", interp.rcx, kvm.rcx));
    }
    if interp.rdx != kvm.rdx {
        diffs.push(format!("rdx: interp={:#x} kvm={:#x}", interp.rdx, kvm.rdx));
    }
    if interp.rsi != kvm.rsi {
        diffs.push(format!("rsi: interp={:#x} kvm={:#x}", interp.rsi, kvm.rsi));
    }
    if interp.rdi != kvm.rdi {
        diffs.push(format!("rdi: interp={:#x} kvm={:#x}", interp.rdi, kvm.rdi));
    }
    if interp.rbp != kvm.rbp {
        diffs.push(format!("rbp: interp={:#x} kvm={:#x}", interp.rbp, kvm.rbp));
    }
    if interp.rsp != kvm.rsp {
        diffs.push(format!("rsp: interp={:#x} kvm={:#x}", interp.rsp, kvm.rsp));
    }
    if interp.r8 != kvm.r8 {
        diffs.push(format!("r8: interp={:#x} kvm={:#x}", interp.r8, kvm.r8));
    }
    if interp.r9 != kvm.r9 {
        diffs.push(format!("r9: interp={:#x} kvm={:#x}", interp.r9, kvm.r9));
    }
    let im = interp.rflags & rflags_mask;
    let km = kvm.rflags & rflags_mask;
    if im != km {
        diffs.push(format!(
            "rflags(mask={rflags_mask:#x}): interp={im:#x} kvm={km:#x}"
        ));
    }
    if interp.scratch != kvm.scratch {
        diffs.push(format!(
            "scratch differs:\n    interp={:02x?}\n    kvm   ={:02x?}",
            &interp.scratch[..],
            &kvm.scratch[..]
        ));
    }
    if interp.stack != kvm.stack {
        diffs.push(format!(
            "stack differs:\n    interp={:02x?}\n    kvm   ={:02x?}",
            &interp.stack[..],
            &kvm.stack[..]
        ));
    }
    diffs
}

// ---------------------------------------------------------------------------
// Input construction.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum InputProfile {
    Int,
    F32,
    F64,
    F16,
    /// f32 lanes drawn from a pool of edge values (NaN/Inf/denormal/zeros/signs/
    /// powers of two), so rounding and special-value handling is stressed.
    F32Edge,
    /// f64 analogue.
    F64Edge,
}

/// "Interesting" f32 bit patterns: +0, -0, 1, -1, +Inf, -Inf, qNaN, sNaN,
/// smallest denormal, largest normal, 0.5, 3.5, a value needing rounding, 2^24.
const F32_EDGES: [u32; 16] = [
    0x0000_0000,
    0x8000_0000,
    0x3f80_0000,
    0xbf80_0000,
    0x7f80_0000,
    0xff80_0000,
    0x7fc0_0000,
    0x7f80_0001,
    0x0000_0001,
    0x7f7f_ffff,
    0x3f00_0000,
    0x4060_0000,
    0x3fb5_04f3,
    0x4b80_0000,
    0x0080_0000,
    0xc97a_0000,
];

/// "Interesting" f64 bit patterns mirroring F32_EDGES.
const F64_EDGES: [u64; 8] = [
    0x0000_0000_0000_0000,
    0x8000_0000_0000_0000,
    0x3ff0_0000_0000_0000,
    0xbff0_0000_0000_0000,
    0x7ff0_0000_0000_0000,
    0xfff0_0000_0000_0000,
    0x7ff8_0000_0000_0000,
    0x0000_0000_0000_0001,
];

/// Finite, non-zero half-precision values. Keeping the FP16 corpus away from
/// NaN/Inf/zero denominators makes bit-exact silicon comparison meaningful.
const F16_VALUES: [u16; 16] = [
    0x3c00, 0x4000, 0x4200, 0x4400, 0x3800, 0x3e00, 0xbc00, 0xc000, 0x3555, 0x3a00, 0x4100, 0x4480,
    0x4600, 0x4800, 0x4900, 0x4a00,
];

fn zmm_from_bytes(bytes: [u8; 64]) -> [u64; 8] {
    let mut out = [0u64; 8];
    for (i, chunk) in bytes.chunks_exact(8).enumerate() {
        out[i] = u64::from_le_bytes(chunk.try_into().unwrap());
    }
    out
}

fn int_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = (reg * 37 + i * 29 + 0x83) as u8;
    }
    bytes
}

fn f32_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..16 {
        let value = 1.0 + reg as f32 * 0.125 + lane as f32 * 0.0625;
        bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn f64_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..8 {
        let value = 1.0 + reg as f64 * 0.25 + lane as f64 * 0.125;
        bytes[lane * 8..lane * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn f16_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..32 {
        let value = F16_VALUES[(reg * 7 + lane) % F16_VALUES.len()];
        bytes[lane * 2..lane * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn f32_edge_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..16 {
        let value = F32_EDGES[(reg * 5 + lane) % F32_EDGES.len()];
        bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn f64_edge_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..8 {
        let value = F64_EDGES[(reg * 3 + lane) % F64_EDGES.len()];
        bytes[lane * 8..lane * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn profile_zmm(profile: InputProfile, reg: usize) -> [u8; 64] {
    match profile {
        InputProfile::Int => int_zmm(reg),
        InputProfile::F32 => f32_zmm(reg),
        InputProfile::F64 => f64_zmm(reg),
        InputProfile::F16 => f16_zmm(reg),
        InputProfile::F32Edge => f32_edge_zmm(reg),
        InputProfile::F64Edge => f64_edge_zmm(reg),
    }
}

fn input_for(profile: InputProfile) -> InCase {
    let mut zmm = [[0u64; 8]; ZMM_REGS];
    for reg in 0..ZMM_REGS {
        zmm[reg] = zmm_from_bytes(profile_zmm(profile, reg));
    }
    let mut scratch = [0u8; SCRATCH_BYTES];
    let scratch_profile = profile_zmm(profile, 31);
    for chunk in scratch.chunks_mut(64) {
        let n = chunk.len();
        chunk.copy_from_slice(&scratch_profile[..n]);
    }
    InCase {
        zmm,
        k: [
            u64::MAX,
            0x5555_5555_5555_5555,
            0xAAAA_AAAA_AAAA_AAAA,
            0x0F0F_0F0F_0F0F_0F0F,
            0x00FF_00FF_00FF_00FF,
            u64::MAX,
            u64::MAX,
            u64::MAX,
        ],
        rbx: RBX_SEED,
        rcx: RCX_SEED,
        rdx: RDX_SEED,
        rsi: STRING_SRC_ADDR,
        rdi: STRING_DST_ADDR,
        rbp: RBP_SEED,
        rsp: RSP_SEED,
        r8: R8_SEED,
        r9: R9_SEED,
        rflags: INITIAL_RFLAGS,
        scratch,
        stack: stack_pattern(),
    }
}

fn stack_pattern() -> [u8; STACK_BYTES] {
    let mut stack = [0u8; STACK_BYTES];
    for (i, byte) in stack.iter_mut().enumerate() {
        *byte = (i as u8).wrapping_mul(19).wrapping_add(0xa7);
    }
    stack
}

fn string_scratch() -> [u8; SCRATCH_BYTES] {
    let mut scratch = [0u8; SCRATCH_BYTES];
    for (i, byte) in scratch.iter_mut().enumerate() {
        *byte = (i as u8).wrapping_mul(37).wrapping_add(0x53);
    }
    scratch
}

fn is_string_mnemonic(mnem: &str) -> bool {
    matches!(
        mnem,
        "movsb"
            | "movsw"
            | "movsl"
            | "movsq"
            | "stosb"
            | "stosw"
            | "stosl"
            | "stosq"
            | "lodsb"
            | "lodsw"
            | "lodsl"
            | "lodsq"
            | "scasb"
            | "scasw"
            | "scasl"
            | "scasq"
            | "cmpsb"
            | "cmpsw"
            | "cmpsl"
            | "cmpsq"
            | "insb"
            | "insw"
            | "insl"
            | "outsb"
            | "outsw"
            | "outsl"
            | "rep"
            | "repe"
            | "repne"
            | "addr32"
    )
}

fn input_for_case(case: &Case) -> InCase {
    let mut input = input_for(case.profile);
    if case.label.contains("initial_cf_clear") {
        input.rflags &= !RFLAGS_CF;
    }
    if case.label.contains("initial_df") {
        input.rflags |= RFLAGS_DF;
    }
    if !is_string_mnemonic(asm_mnemonic(&case.asm)) {
        return input;
    }

    input.scratch = string_scratch();
    input.rsi = STRING_SRC_ADDR;
    input.rdi = STRING_DST_ADDR;
    if case
        .asm
        .split_whitespace()
        .any(|token| matches!(token, "rep" | "repe" | "repne"))
    {
        input.rcx = STRING_REP_COUNT;
    }
    if case.label.contains("count_zero") {
        input.rcx = 0;
    }
    if case.label.contains("_df") {
        input.rsi = input.rsi.wrapping_add(STRING_DF_OFFSET);
        input.rdi = input.rdi.wrapping_add(STRING_DF_OFFSET);
        input.rflags |= RFLAGS_DF;
    }
    input
}

// ---------------------------------------------------------------------------
// Corpus.
// ---------------------------------------------------------------------------

struct Case {
    label: String,
    asm: String,
    feat: Feat,
    profile: InputProfile,
}

/// A small, hand-picked starter corpus that exercises every distinct cross-KVM
/// path: integer VVV, FP VVV, immediate, compare-into-mask, convert, mask-op,
/// FMA (dst is also a source), and merge/zero masking. The full generator is
/// layered on in a later step.
fn starter_cases() -> Vec<Case> {
    let c = |label: &str, asm: &str, feat: Feat, profile: InputProfile| Case {
        label: label.to_string(),
        asm: asm.to_string(),
        feat,
        profile,
    };
    vec![
        c(
            "vpaddd_zmm",
            "vpaddd %zmm2, %zmm3, %zmm1",
            Feat::F,
            InputProfile::Int,
        ),
        c(
            "vpaddd_zmm_merge",
            "vpaddd %zmm2, %zmm3, %zmm1 {%k2}",
            Feat::F,
            InputProfile::Int,
        ),
        c(
            "vpaddd_zmm_zero",
            "vpaddd %zmm2, %zmm3, %zmm1 {%k2}{z}",
            Feat::F,
            InputProfile::Int,
        ),
        c(
            "vpaddd_zmm_mem",
            "vpaddd (%rax), %zmm3, %zmm1",
            Feat::F,
            InputProfile::Int,
        ),
        c(
            "vpaddd_zmm_bcst",
            "vpaddd (%rax){1to16}, %zmm3, %zmm1",
            Feat::F,
            InputProfile::Int,
        ),
        c(
            "vpaddd_zmm_high",
            "vpaddd %zmm16, %zmm18, %zmm17",
            Feat::F,
            InputProfile::Int,
        ),
        c(
            "vaddps_zmm",
            "vaddps %zmm2, %zmm3, %zmm1",
            Feat::F,
            InputProfile::F32,
        ),
        c(
            "vaddpd_zmm",
            "vaddpd %zmm2, %zmm3, %zmm1",
            Feat::F,
            InputProfile::F64,
        ),
        c(
            "vmulps_zmm",
            "vmulps %zmm2, %zmm3, %zmm1",
            Feat::F,
            InputProfile::F32,
        ),
        c(
            "vpsrld_imm",
            "vpsrld $3, %zmm3, %zmm1",
            Feat::F,
            InputProfile::Int,
        ),
        c(
            "vpcmpd_eq",
            "vpcmpd $0, %zmm2, %zmm3, %k1",
            Feat::F,
            InputProfile::Int,
        ),
        c(
            "vcvtdq2ps_zmm",
            "vcvtdq2ps %zmm3, %zmm1",
            Feat::F,
            InputProfile::Int,
        ),
        c("kandw_k", "kandw %k2, %k3, %k1", Feat::F, InputProfile::Int),
        c(
            "vfmadd213ps_zmm",
            "vfmadd213ps %zmm2, %zmm3, %zmm1",
            Feat::F,
            InputProfile::F32,
        ),
        c(
            "vpaddb_zmm",
            "vpaddb %zmm2, %zmm3, %zmm1",
            Feat::Bw,
            InputProfile::Int,
        ),
        c(
            "vpaddd_ymm_evex",
            "{evex} vpaddd %ymm2, %ymm3, %ymm1",
            Feat::Vl,
            InputProfile::Int,
        ),
        c(
            "vpaddd_xmm_evex",
            "{evex} vpaddd %xmm2, %xmm3, %xmm1",
            Feat::Vl,
            InputProfile::Int,
        ),
        c(
            "vplzcntd_zmm",
            "vplzcntd %zmm3, %zmm1",
            Feat::Cd,
            InputProfile::Int,
        ),
    ]
}

// ---------------------------------------------------------------------------
// Exhaustive EVEX corpus generator.
//
// A compact table of base instructions is expanded across every EVEX dimension
// where bugs hide: operation width (512 + VL 256/128), write-masking (none /
// merge / zeroing), the r/m operand (register / memory / embedded broadcast),
// and high registers (zmm16-31, which also exercise the Hi16_ZMM XSAVE state).
// All width<512 forms are pinned to EVEX with the `{evex}` pseudo-prefix so we
// test the AVX-512 encodings rather than letting the assembler pick VEX.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Form {
    /// dst, src1(vvvv), src2/mem. (also covers FMA + 3-source vpermt2/vpternlog-
    /// without-imm, since the extra source is the injected dst.)
    Vvv,
    /// dst, src/mem.
    Vv,
    /// dst, src/mem, imm8.
    VvI(u8),
    /// dst, src1(vvvv), src2/mem, imm8.
    VvvI(u8),
    /// kdst, src1, src2/mem  (compare / test into a mask).
    Kvv,
    /// kdst, src1, src2/mem, imm8.
    KvvI(u8),
    /// kdst, src/mem, imm8  (vfpclass).
    KvI(u8),
    /// kdst, ksrc1, ksrc2  (VEX-encoded opmask ALU).
    MaskRRR,
    /// kdst, ksrc, imm8.
    MaskRRI(u8),
    /// kdst, ksrc.
    MaskRR,
}

impl Form {
    /// Does this form take a write-mask {k}/{k}{z}?
    fn maskable(self) -> bool {
        matches!(self, Form::Vvv | Form::Vv | Form::VvI(_) | Form::VvvI(_))
    }
    /// Is the r/m operand a vector (so register/memory/broadcast applies)?
    fn vector_rm(self) -> bool {
        !matches!(self, Form::MaskRRR | Form::MaskRRI(_) | Form::MaskRR)
    }
    /// Short tag that keeps labels unique when one mnemonic appears in two forms
    /// (e.g. variable vs. immediate shift).
    fn tag(self) -> &'static str {
        match self {
            Form::Vvv => "vvv",
            Form::Vv => "vv",
            Form::VvI(_) => "vvi",
            Form::VvvI(_) => "vvvi",
            Form::Kvv => "kvv",
            Form::KvvI(_) => "kvvi",
            Form::KvI(_) => "kvi",
            Form::MaskRRR | Form::MaskRRI(_) | Form::MaskRR => "k",
        }
    }
}

/// Mnemonics whose r/m operand is a scalar shift count in xmm/m128, regardless
/// of the destination width (the "shift whole vector by one count" forms).
fn rm_is_xmm(mnem: &str) -> bool {
    matches!(
        mnem,
        "vpslld"
            | "vpsrld"
            | "vpsrad"
            | "vpsllq"
            | "vpsrlq"
            | "vpsraq"
            | "vpsllw"
            | "vpsrlw"
            | "vpsraw"
    )
}

/// Mnemonics that do not accept a write-mask in the forms we generate.
fn is_nomask(mnem: &str) -> bool {
    matches!(
        mnem,
        "vpsadbw" | "vaesenc" | "vaesenclast" | "vaesdec" | "vaesdeclast" | "vpclmulqdq"
    )
}

struct Base {
    mnem: &'static str,
    feat: Feat,
    form: Form,
    profile: InputProfile,
    /// Broadcast element size in bytes (4=dword, 8=qword); 0 = no broadcast form.
    elem: u8,
    /// Expand the 256-/128-bit VL widths in addition to 512.
    vl: bool,
    /// Also run an FP-edge-value input profile (NaN/Inf/denormal/zeros/signs).
    edge: bool,
}

const fn b(
    mnem: &'static str,
    feat: Feat,
    form: Form,
    profile: InputProfile,
    elem: u8,
    vl: bool,
    edge: bool,
) -> Base {
    Base {
        mnem,
        feat,
        form,
        profile,
        elem,
        vl,
        edge,
    }
}

#[derive(Clone, Copy)]
enum Mask {
    None,
    Merge,
    Zero,
}

#[derive(Clone, Copy)]
enum Rm {
    Reg,
    Mem,
    Bcst,
}

fn vec_name(width: u16, idx: u8) -> String {
    let class = match width {
        512 => "zmm",
        256 => "ymm",
        _ => "xmm",
    };
    format!("%{class}{idx}")
}

fn bcast_token(width: u16, elem: u8) -> String {
    let count = (width as usize / 8) / elem as usize;
    format!("(%rax){{1to{count}}}")
}

fn mask_str(mask: Mask, kreg: u8) -> &'static str {
    // The mask register itself is fixed (k1 has alternating bits so masking is
    // observable); only the merge/zero decoration changes here.
    let _ = kreg;
    match mask {
        Mask::None => "",
        Mask::Merge => " {%k1}",
        Mask::Zero => " {%k1}{z}",
    }
}

/// Build the r/m operand text for a vector form. `rm_width` is the register
/// width of the r/m operand (usually the op width, but 128 for shift counts).
fn rm_text(rm_width: u16, op_width: u16, elem: u8, rm: Rm, reg_idx: u8) -> String {
    match rm {
        Rm::Reg => vec_name(rm_width, reg_idx),
        Rm::Mem => "(%rax)".to_string(),
        Rm::Bcst => bcast_token(op_width, elem),
    }
}

/// Emit the AT&T text for one expanded case (or None if the combination is
/// structurally invalid, e.g. a broadcast on a byte-element op).
fn emit_asm(base: &Base, width: u16, mask: Mask, rm: Rm, high: bool) -> Option<String> {
    if matches!(rm, Rm::Bcst) && base.elem == 0 {
        return None;
    }
    let evex = if width == 512 { "" } else { "{evex} " };
    // Register roles. High variant shifts vector regs into zmm16+.
    let (d, s1, s2) = if high {
        (17u8, 18u8, 16u8)
    } else {
        (1u8, 2u8, 3u8)
    };
    let kd = if high { 6u8 } else { 5u8 };
    let m = mask_str(mask, 1);
    // Only the *variable* shift form (Vvv: shift whole vector by an xmm count)
    // narrows the r/m operand to xmm; the immediate form (VvI) is same-width.
    let rm_width = if matches!(base.form, Form::Vvv) && rm_is_xmm(base.mnem) {
        128
    } else {
        width
    };
    let rmop = |idx: u8| rm_text(rm_width, width, base.elem, rm, idx);

    let asm = match base.form {
        Form::Vvv => format!(
            "{evex}{} {}, {}, {}{m}",
            base.mnem,
            rmop(s2),
            vec_name(width, s1),
            vec_name(width, d)
        ),
        Form::Vv => format!(
            "{evex}{} {}, {}{m}",
            base.mnem,
            rmop(s2),
            vec_name(width, d)
        ),
        Form::VvI(i) => format!(
            "{evex}{} ${i}, {}, {}{m}",
            base.mnem,
            rmop(s2),
            vec_name(width, d)
        ),
        Form::VvvI(i) => format!(
            "{evex}{} ${i}, {}, {}, {}{m}",
            base.mnem,
            rmop(s2),
            vec_name(width, s1),
            vec_name(width, d)
        ),
        Form::Kvv => format!(
            "{evex}{} {}, {}, %k{kd}{m}",
            base.mnem,
            rmop(s2),
            vec_name(width, s1)
        ),
        Form::KvvI(i) => format!(
            "{evex}{} ${i}, {}, {}, %k{kd}{m}",
            base.mnem,
            rmop(s2),
            vec_name(width, s1)
        ),
        Form::KvI(i) => format!("{evex}{} ${i}, {}, %k{kd}{m}", base.mnem, rmop(s2)),
        Form::MaskRRR => format!("{} %k3, %k2, %k{kd}", base.mnem),
        Form::MaskRRI(i) => format!("{} ${i}, %k2, %k{kd}", base.mnem),
        Form::MaskRR => format!("{} %k2, %k{kd}", base.mnem),
    };
    let _ = (d, s1, s2);
    Some(asm)
}

/// Expand one base instruction into all its case variants.
fn expand(base: &Base, out: &mut Vec<Case>) {
    let widths: &[u16] = if base.form.vector_rm() && base.vl {
        &[512, 256, 128]
    } else {
        &[512]
    };
    let masks_for = |w: u16| -> &'static [Mask] {
        if !base.form.maskable() || is_nomask(base.mnem) {
            &[Mask::None]
        } else if w == 512 {
            &[Mask::None, Mask::Merge, Mask::Zero]
        } else {
            &[Mask::None, Mask::Zero]
        }
    };
    let tag = base.form.tag();

    let mut push = |label: String, asm: String, profile: InputProfile| {
        out.push(Case {
            label,
            asm,
            feat: base.feat,
            profile,
        });
    };

    if !base.form.vector_rm() {
        // Opmask ALU: no width / mask / memory expansion.
        if let Some(asm) = emit_asm(base, 512, Mask::None, Rm::Reg, false) {
            push(format!("{}", base.mnem), asm, base.profile);
        }
        return;
    }

    for &w in widths {
        for &mask in masks_for(w) {
            // Register r/m, low regs.
            if let Some(asm) = emit_asm(base, w, mask, Rm::Reg, false) {
                push(
                    format!("{}_{tag}_w{w}_{}_reg", base.mnem, mask_tag(mask)),
                    asm,
                    base.profile,
                );
            }
            // Memory r/m (only no-mask + merge, to bound counts). 512-bit
            // packed vfpclass (KvI) accepts memory only as embedded broadcast,
            // which is emitted below; the VL memory forms are valid.
            let skip_plain_mem = w == 512 && matches!(base.form, Form::KvI(_));
            if matches!(mask, Mask::None | Mask::Merge) {
                if !skip_plain_mem {
                    if let Some(asm) = emit_asm(base, w, mask, Rm::Mem, false) {
                        push(
                            format!("{}_{tag}_w{w}_{}_mem", base.mnem, mask_tag(mask)),
                            asm,
                            base.profile,
                        );
                    }
                }
                // Embedded broadcast (no-mask only).
                if matches!(mask, Mask::None) {
                    if let Some(asm) = emit_asm(base, w, mask, Rm::Bcst, false) {
                        push(format!("{}_{tag}_w{w}_bcst", base.mnem), asm, base.profile);
                    }
                }
            }
        }
    }

    // High-register variant at 512 (exercises zmm16-31 / Hi16_ZMM state).
    if let Some(asm) = emit_asm(base, 512, Mask::None, Rm::Reg, true) {
        push(format!("{}_{tag}_high", base.mnem), asm, base.profile);
    }

    // FP edge-value pass (register form, 512, no mask).
    if base.edge {
        let edge_profile = match base.profile {
            InputProfile::F64 => InputProfile::F64Edge,
            _ => InputProfile::F32Edge,
        };
        if let Some(asm) = emit_asm(base, 512, Mask::None, Rm::Reg, false) {
            push(format!("{}_{tag}_edge", base.mnem), asm, edge_profile);
        }
    }
}

fn mask_tag(mask: Mask) -> &'static str {
    match mask {
        Mask::None => "nomask",
        Mask::Merge => "merge",
        Mask::Zero => "zero",
    }
}

/// The base-instruction table for the host-supported AVX-512 subsets.
fn base_table() -> Vec<Base> {
    use Feat::*;
    use Form::*;
    use InputProfile::*;
    let t = true;
    let f = false;
    vec![
        // ---- packed integer arithmetic (F: d/q, BW: b/w) ----
        b("vpaddd", F, Vvv, Int, 4, t, f),
        b("vpaddq", F, Vvv, Int, 8, t, f),
        b("vpsubd", F, Vvv, Int, 4, t, f),
        b("vpsubq", F, Vvv, Int, 8, t, f),
        b("vpaddb", Bw, Vvv, Int, 0, t, f),
        b("vpaddw", Bw, Vvv, Int, 0, t, f),
        b("vpsubb", Bw, Vvv, Int, 0, t, f),
        b("vpsubw", Bw, Vvv, Int, 0, t, f),
        b("vpaddsb", Bw, Vvv, Int, 0, t, f),
        b("vpaddsw", Bw, Vvv, Int, 0, t, f),
        b("vpaddusb", Bw, Vvv, Int, 0, f, f),
        b("vpaddusw", Bw, Vvv, Int, 0, f, f),
        b("vpsubsb", Bw, Vvv, Int, 0, f, f),
        b("vpsubsw", Bw, Vvv, Int, 0, f, f),
        b("vpsubusb", Bw, Vvv, Int, 0, f, f),
        b("vpsubusw", Bw, Vvv, Int, 0, f, f),
        b("vpmulld", F, Vvv, Int, 4, t, f),
        b("vpmullq", Dq, Vvv, Int, 8, t, f),
        b("vpmullw", Bw, Vvv, Int, 0, t, f),
        b("vpmuldq", F, Vvv, Int, 8, f, f),
        b("vpmuludq", F, Vvv, Int, 8, f, f),
        b("vpmulhw", Bw, Vvv, Int, 0, f, f),
        b("vpmulhuw", Bw, Vvv, Int, 0, f, f),
        b("vpmulhrsw", Bw, Vvv, Int, 0, t, f),
        b("vpmaddubsw", Bw, Vvv, Int, 0, t, f),
        b("vpmaddwd", F, Vvv, Int, 0, t, f),
        b("vpavgb", Bw, Vvv, Int, 0, f, f),
        b("vpavgw", Bw, Vvv, Int, 0, f, f),
        b("vpsadbw", Bw, Vvv, Int, 0, f, f),
        b("vpabsd", F, Vv, Int, 4, t, f),
        b("vpabsq", F, Vv, Int, 8, f, f),
        b("vpabsb", Bw, Vv, Int, 0, f, f),
        b("vpabsw", Bw, Vv, Int, 0, f, f),
        // ---- min/max ----
        b("vpmaxsd", F, Vvv, Int, 4, t, f),
        b("vpmaxud", F, Vvv, Int, 4, f, f),
        b("vpmaxsq", F, Vvv, Int, 8, f, f),
        b("vpmaxuq", F, Vvv, Int, 8, f, f),
        b("vpminsd", F, Vvv, Int, 4, t, f),
        b("vpminud", F, Vvv, Int, 4, f, f),
        b("vpminsq", F, Vvv, Int, 8, f, f),
        b("vpminuq", F, Vvv, Int, 8, f, f),
        b("vpmaxsb", Bw, Vvv, Int, 0, f, f),
        b("vpmaxsw", Bw, Vvv, Int, 0, f, f),
        b("vpmaxub", Bw, Vvv, Int, 0, f, f),
        b("vpmaxuw", Bw, Vvv, Int, 0, f, f),
        b("vpminsb", Bw, Vvv, Int, 0, f, f),
        b("vpminsw", Bw, Vvv, Int, 0, f, f),
        b("vpminub", Bw, Vvv, Int, 0, f, f),
        b("vpminuw", Bw, Vvv, Int, 0, f, f),
        // ---- logical (F) ----
        b("vpandd", F, Vvv, Int, 4, t, f),
        b("vpandq", F, Vvv, Int, 8, f, f),
        b("vpandnd", F, Vvv, Int, 4, f, f),
        b("vpandnq", F, Vvv, Int, 8, f, f),
        b("vpord", F, Vvv, Int, 4, t, f),
        b("vporq", F, Vvv, Int, 8, f, f),
        b("vpxord", F, Vvv, Int, 4, t, f),
        b("vpxorq", F, Vvv, Int, 8, f, f),
        b("vpternlogd", F, VvvI(0xca), Int, 4, t, f),
        b("vpternlogq", F, VvvI(0xca), Int, 8, f, f),
        // ---- newer Xeon integer / crypto / media extensions ----
        b("vpdpbusd", Vnni, Vvv, Int, 4, t, f),
        b("vpdpbusds", Vnni, Vvv, Int, 4, t, f),
        b("vpdpwssd", Vnni, Vvv, Int, 4, t, f),
        b("vpdpwssds", Vnni, Vvv, Int, 4, t, f),
        b("vpmadd52luq", Ifma, Vvv, Int, 8, t, f),
        b("vpmadd52huq", Ifma, Vvv, Int, 8, t, f),
        b("vdbpsadbw", Bw, VvvI(5), Int, 0, t, f),
        b("vpermb", Vbmi, Vvv, Int, 0, t, f),
        b("vpermi2b", Vbmi, Vvv, Int, 0, t, f),
        b("vpermt2b", Vbmi, Vvv, Int, 0, t, f),
        b("vpmultishiftqb", Vbmi, Vvv, Int, 8, t, f),
        b("vpopcntb", Bitalg, Vv, Int, 0, t, f),
        b("vpopcntw", Bitalg, Vv, Int, 0, t, f),
        b("vpshufbitqmb", Bitalg, Kvv, Int, 0, t, f),
        b("vpopcntd", Vpopcntdq, Vv, Int, 4, t, f),
        b("vpopcntq", Vpopcntdq, Vv, Int, 8, t, f),
        b("vdpbf16ps", Bf16, Vvv, F32, 4, t, f),
        b("vgf2p8mulb", Gfni, Vvv, Int, 0, t, f),
        b("vgf2p8affineqb", Gfni, VvvI(0x63), Int, 8, t, f),
        b("vgf2p8affineinvqb", Gfni, VvvI(0x63), Int, 8, t, f),
        b("vaesenc", Vaes, Vvv, Int, 0, t, f),
        b("vaesenclast", Vaes, Vvv, Int, 0, t, f),
        b("vaesdec", Vaes, Vvv, Int, 0, t, f),
        b("vaesdeclast", Vaes, Vvv, Int, 0, t, f),
        b("vpclmulqdq", Vpclmulqdq, VvvI(0x11), Int, 0, t, f),
        // ---- shifts (F: d/q variable+imm, BW: w) ----
        b("vpslld", F, Vvv, Int, 0, f, f),
        b("vpsrld", F, Vvv, Int, 0, f, f),
        b("vpsrad", F, Vvv, Int, 0, f, f),
        b("vpsllq", F, Vvv, Int, 0, f, f),
        b("vpsrlq", F, Vvv, Int, 0, f, f),
        b("vpsraq", F, Vvv, Int, 0, f, f),
        b("vpslld", F, VvI(5), Int, 4, t, f),
        b("vpsrld", F, VvI(5), Int, 4, t, f),
        b("vpsrad", F, VvI(5), Int, 4, f, f),
        b("vpsllq", F, VvI(5), Int, 8, f, f),
        b("vpsrlq", F, VvI(5), Int, 8, f, f),
        b("vpsraq", F, VvI(5), Int, 8, f, f),
        b("vpsllw", Bw, VvI(5), Int, 0, f, f),
        b("vpsrlw", Bw, VvI(5), Int, 0, f, f),
        b("vpsraw", Bw, VvI(5), Int, 0, f, f),
        b("vpsllvd", F, Vvv, Int, 4, t, f),
        b("vpsrlvd", F, Vvv, Int, 4, f, f),
        b("vpsravd", F, Vvv, Int, 4, f, f),
        b("vpsllvq", F, Vvv, Int, 8, f, f),
        b("vpsrlvq", F, Vvv, Int, 8, f, f),
        b("vpsravq", F, Vvv, Int, 8, f, f),
        b("vpsllvw", Bw, Vvv, Int, 0, f, f),
        b("vpsrlvw", Bw, Vvv, Int, 0, f, f),
        b("vpsravw", Bw, Vvv, Int, 0, f, f),
        b("vprold", F, VvI(7), Int, 4, f, f),
        b("vprolq", F, VvI(7), Int, 8, f, f),
        b("vprord", F, VvI(7), Int, 4, f, f),
        b("vprorq", F, VvI(7), Int, 8, f, f),
        b("vprolvd", F, Vvv, Int, 4, f, f),
        b("vprolvq", F, Vvv, Int, 8, f, f),
        b("vprorvd", F, Vvv, Int, 4, f, f),
        b("vprorvq", F, Vvv, Int, 8, f, f),
        // ---- VBMI2 funnel shifts ----
        b("vpshldw", Vbmi2, VvvI(5), Int, 0, t, f),
        b("vpshldd", Vbmi2, VvvI(5), Int, 4, t, f),
        b("vpshldq", Vbmi2, VvvI(5), Int, 8, t, f),
        b("vpshrdw", Vbmi2, VvvI(5), Int, 0, t, f),
        b("vpshrdd", Vbmi2, VvvI(5), Int, 4, t, f),
        b("vpshrdq", Vbmi2, VvvI(5), Int, 8, t, f),
        b("vpshldvw", Vbmi2, Vvv, Int, 0, t, f),
        b("vpshldvd", Vbmi2, Vvv, Int, 4, t, f),
        b("vpshldvq", Vbmi2, Vvv, Int, 8, t, f),
        b("vpshrdvw", Vbmi2, Vvv, Int, 0, t, f),
        b("vpshrdvd", Vbmi2, Vvv, Int, 4, t, f),
        b("vpshrdvq", Vbmi2, Vvv, Int, 8, t, f),
        // ---- compares into mask (F: d/q, BW: b/w) ----
        b("vpcmpeqd", F, Kvv, Int, 4, f, f),
        b("vpcmpgtd", F, Kvv, Int, 4, f, f),
        b("vpcmpeqq", F, Kvv, Int, 8, f, f),
        b("vpcmpgtq", F, Kvv, Int, 8, f, f),
        b("vpcmpeqb", Bw, Kvv, Int, 0, f, f),
        b("vpcmpgtb", Bw, Kvv, Int, 0, f, f),
        b("vpcmpeqw", Bw, Kvv, Int, 0, f, f),
        b("vpcmpgtw", Bw, Kvv, Int, 0, f, f),
        b("vpcmpd", F, KvvI(1), Int, 4, f, f),
        b("vpcmpud", F, KvvI(1), Int, 4, f, f),
        b("vpcmpq", F, KvvI(2), Int, 8, f, f),
        b("vpcmpuq", F, KvvI(2), Int, 8, f, f),
        b("vpcmpb", Bw, KvvI(4), Int, 0, f, f),
        b("vpcmpw", Bw, KvvI(5), Int, 0, f, f),
        b("vpcmpub", Bw, KvvI(0), Int, 0, f, f),
        b("vpcmpuw", Bw, KvvI(6), Int, 0, f, f),
        b("vptestmd", F, Kvv, Int, 4, f, f),
        b("vptestnmd", F, Kvv, Int, 4, f, f),
        b("vptestmq", F, Kvv, Int, 8, f, f),
        b("vptestnmq", F, Kvv, Int, 8, f, f),
        b("vptestmb", Bw, Kvv, Int, 0, f, f),
        b("vptestnmb", Bw, Kvv, Int, 0, f, f),
        b("vptestmw", Bw, Kvv, Int, 0, f, f),
        b("vptestnmw", Bw, Kvv, Int, 0, f, f),
        // ---- blend by mask (F) ----
        b("vpblendmd", F, Vvv, Int, 4, f, f),
        b("vpblendmq", F, Vvv, Int, 8, f, f),
        b("vblendmps", F, Vvv, F32, 4, f, f),
        b("vblendmpd", F, Vvv, F64, 8, f, f),
        // ---- masked moves ----
        b("vmovdqa32", F, Vv, Int, 0, f, f),
        b("vmovdqa64", F, Vv, Int, 0, f, f),
        b("vmovdqu32", F, Vv, Int, 0, f, f),
        b("vmovdqu64", F, Vv, Int, 0, f, f),
        b("vmovdqu8", Bw, Vv, Int, 0, f, f),
        b("vmovdqu16", Bw, Vv, Int, 0, f, f),
        b("vmovaps", F, Vv, F32, 0, f, f),
        b("vmovapd", F, Vv, F64, 0, f, f),
        // ---- permute / shuffle / unpack / align ----
        b("vpermd", F, Vvv, Int, 4, f, f),
        b("vpermq", F, VvI(0x1b), Int, 8, f, f),
        b("vpermw", Bw, Vvv, Int, 0, f, f),
        b("vpermps", F, Vvv, F32, 4, f, f),
        b("vpermpd", F, VvI(0x1b), F64, 8, f, f),
        b("vpermi2w", Bw, Vvv, Int, 0, f, f),
        b("vpermt2w", Bw, Vvv, Int, 0, f, f),
        b("vpermt2d", F, Vvv, Int, 4, f, f),
        b("vpermt2q", F, Vvv, Int, 8, f, f),
        b("vpermi2d", F, Vvv, Int, 4, f, f),
        b("vpermi2q", F, Vvv, Int, 8, f, f),
        b("vpermi2ps", F, Vvv, F32, 4, f, f),
        b("vpermi2pd", F, Vvv, F64, 8, f, f),
        b("vpermt2ps", F, Vvv, F32, 4, f, f),
        b("vpermt2pd", F, Vvv, F64, 8, f, f),
        b("vpermilps", F, VvI(0x1b), F32, 4, f, f),
        b("vpermilpd", F, VvI(0x05), F64, 8, f, f),
        b("vshufps", F, VvvI(0x1b), F32, 4, f, f),
        b("vshufpd", F, VvvI(0x05), F64, 8, f, f),
        b("vshufi32x4", F, VvvI(0x4e), Int, 4, f, f),
        b("vshufi64x2", F, VvvI(0x4e), Int, 8, f, f),
        b("vshuff32x4", F, VvvI(0x4e), F32, 4, f, f),
        b("vshuff64x2", F, VvvI(0x4e), F64, 8, f, f),
        b("valignd", F, VvvI(3), Int, 4, f, f),
        b("valignq", F, VvvI(1), Int, 8, f, f),
        b("vpalignr", Bw, VvvI(5), Int, 0, f, f),
        b("vpshufd", F, VvI(0x1b), Int, 4, f, f),
        b("vpshufhw", Bw, VvI(0x1b), Int, 0, f, f),
        b("vpshuflw", Bw, VvI(0x1b), Int, 0, f, f),
        b("vpshufb", Bw, Vvv, Int, 0, f, f),
        b("vpackssdw", F, Vvv, Int, 4, f, f),
        b("vpackusdw", F, Vvv, Int, 4, f, f),
        b("vpacksswb", Bw, Vvv, Int, 0, f, f),
        b("vpackuswb", Bw, Vvv, Int, 0, f, f),
        b("vpunpckldq", F, Vvv, Int, 4, f, f),
        b("vpunpckhdq", F, Vvv, Int, 4, f, f),
        b("vpunpcklqdq", F, Vvv, Int, 8, f, f),
        b("vpunpckhqdq", F, Vvv, Int, 8, f, f),
        b("vpunpcklbw", Bw, Vvv, Int, 0, f, f),
        b("vpunpckhbw", Bw, Vvv, Int, 0, f, f),
        b("vpunpcklwd", Bw, Vvv, Int, 0, f, f),
        b("vpunpckhwd", Bw, Vvv, Int, 0, f, f),
        b("vunpcklps", F, Vvv, F32, 4, f, f),
        b("vunpckhps", F, Vvv, F32, 4, f, f),
        b("vunpcklpd", F, Vvv, F64, 8, f, f),
        b("vunpckhpd", F, Vvv, F64, 8, f, f),
        // ---- same-width converts (dword<->single, qword<->double) ----
        // (width-changing converts, pmov extend/truncate, and broadcasts have
        //  asymmetric operand widths and live in `irregular_cases()`.)
        b("vcvtdq2ps", F, Vv, Int, 4, f, f),
        b("vcvtudq2ps", F, Vv, Int, 4, f, f),
        b("vcvtps2dq", F, Vv, F32, 4, f, t),
        b("vcvttps2dq", F, Vv, F32, 4, f, t),
        b("vcvtps2udq", F, Vv, F32, 4, f, t),
        b("vcvtqq2pd", Dq, Vv, Int, 8, f, f),
        b("vcvtuqq2pd", Dq, Vv, Int, 8, f, f),
        b("vcvtpd2qq", Dq, Vv, F64, 8, f, t),
        b("vcvttpd2qq", Dq, Vv, F64, 8, f, t),
        // ---- packed FP arithmetic (F) ----
        b("vaddps", F, Vvv, F32, 4, t, t),
        b("vaddpd", F, Vvv, F64, 8, t, t),
        b("vsubps", F, Vvv, F32, 4, f, t),
        b("vsubpd", F, Vvv, F64, 8, f, t),
        b("vmulps", F, Vvv, F32, 4, t, t),
        b("vmulpd", F, Vvv, F64, 8, f, t),
        b("vdivps", F, Vvv, F32, 4, f, t),
        b("vdivpd", F, Vvv, F64, 8, f, t),
        b("vminps", F, Vvv, F32, 4, f, t),
        b("vmaxps", F, Vvv, F32, 4, f, t),
        b("vminpd", F, Vvv, F64, 8, f, t),
        b("vmaxpd", F, Vvv, F64, 8, f, t),
        b("vsqrtps", F, Vv, F32, 4, f, t),
        b("vsqrtpd", F, Vv, F64, 8, f, t),
        b("vaddph", Fp16, Vvv, F16, 2, t, f),
        b("vsubph", Fp16, Vvv, F16, 2, t, f),
        b("vmulph", Fp16, Vvv, F16, 2, t, f),
        b("vdivph", Fp16, Vvv, F16, 2, t, f),
        b("vminph", Fp16, Vvv, F16, 2, t, f),
        b("vmaxph", Fp16, Vvv, F16, 2, t, f),
        b("vsqrtph", Fp16, Vv, F16, 2, t, f),
        b("vcmpph", Fp16, KvvI(4), F16, 2, t, f),
        b("vgetexpph", Fp16, Vv, F16, 2, t, f),
        b("vgetmantph", Fp16, VvI(0), F16, 2, t, f),
        b("vrndscaleph", Fp16, VvI(0), F16, 2, t, f),
        b("vreduceph", Fp16, VvI(0), F16, 2, t, f),
        b("vscalefph", Fp16, Vvv, F16, 2, t, f),
        b("vrcpph", Fp16, Vv, F16, 2, t, f),
        b("vrsqrtph", Fp16, Vv, F16, 2, t, f),
        b("vfmadd132ph", Fp16, Vvv, F16, 2, t, f),
        b("vfmadd213ph", Fp16, Vvv, F16, 2, t, f),
        b("vfmadd231ph", Fp16, Vvv, F16, 2, t, f),
        b("vfmsub132ph", Fp16, Vvv, F16, 2, t, f),
        b("vfmsub213ph", Fp16, Vvv, F16, 2, t, f),
        b("vfmsub231ph", Fp16, Vvv, F16, 2, t, f),
        b("vfnmadd132ph", Fp16, Vvv, F16, 2, t, f),
        b("vfnmadd213ph", Fp16, Vvv, F16, 2, t, f),
        b("vfnmadd231ph", Fp16, Vvv, F16, 2, t, f),
        b("vfnmsub132ph", Fp16, Vvv, F16, 2, t, f),
        b("vfnmsub213ph", Fp16, Vvv, F16, 2, t, f),
        b("vfnmsub231ph", Fp16, Vvv, F16, 2, t, f),
        b("vfmaddsub132ph", Fp16, Vvv, F16, 2, t, f),
        b("vfmaddsub213ph", Fp16, Vvv, F16, 2, t, f),
        b("vfmaddsub231ph", Fp16, Vvv, F16, 2, t, f),
        b("vfmsubadd132ph", Fp16, Vvv, F16, 2, t, f),
        b("vfmsubadd213ph", Fp16, Vvv, F16, 2, t, f),
        b("vfmsubadd231ph", Fp16, Vvv, F16, 2, t, f),
        b("vfmulcph", Fp16, Vvv, F16, 4, t, f),
        b("vfcmulcph", Fp16, Vvv, F16, 4, t, f),
        b("vfmaddcph", Fp16, Vvv, F16, 4, t, f),
        b("vfcmaddcph", Fp16, Vvv, F16, 4, t, f),
        b("vandps", F, Vvv, F32, 4, f, f),
        b("vandpd", F, Vvv, F64, 8, f, f),
        b("vandnps", F, Vvv, F32, 4, f, f),
        b("vandnpd", F, Vvv, F64, 8, f, f),
        b("vorps", F, Vvv, F32, 4, f, f),
        b("vxorps", F, Vvv, F32, 4, f, f),
        b("vorpd", F, Vvv, F64, 8, f, f),
        b("vxorpd", F, Vvv, F64, 8, f, f),
        // ---- FMA (F) ----
        b("vfmadd213ps", F, Vvv, F32, 4, f, t),
        b("vfmadd213pd", F, Vvv, F64, 8, f, t),
        b("vfmadd231ps", F, Vvv, F32, 4, f, f),
        b("vfmsub213ps", F, Vvv, F32, 4, f, t),
        b("vfnmadd213ps", F, Vvv, F32, 4, f, f),
        b("vfmaddsub213ps", F, Vvv, F32, 4, f, f),
        b("vfmsubadd213ps", F, Vvv, F32, 4, f, f),
        // ---- FP range/scale/round/class (F + DQ) ----
        b("vscalefps", F, Vvv, F32, 4, f, t),
        b("vscalefpd", F, Vvv, F64, 8, f, t),
        b("vgetexpps", F, Vv, F32, 4, f, t),
        b("vgetexppd", F, Vv, F64, 8, f, t),
        b("vgetmantps", F, VvI(0x00), F32, 4, f, t),
        b("vrndscaleps", F, VvI(0x03), F32, 4, f, t),
        b("vrndscalepd", F, VvI(0x03), F64, 8, f, t),
        b("vreduceps", Dq, VvI(0x03), F32, 4, f, t),
        b("vrcp14ps", F, Vv, F32, 4, f, t),
        b("vrsqrt14ps", F, Vv, F32, 4, f, t),
        b("vfixupimmps", F, VvvI(0x00), F32, 4, f, f),
        b("vrangeps", Dq, VvvI(0x00), F32, 4, f, t),
        b("vcmpps", F, KvvI(0), F32, 4, f, t),
        b("vcmppd", F, KvvI(0), F64, 8, f, t),
        b("vfpclassps", Dq, KvI(0x03), F32, 4, f, t),
        b("vfpclasspd", Dq, KvI(0x03), F64, 8, f, t),
        // ---- conflict-detection (CD) ----
        b("vplzcntd", Cd, Vv, Int, 4, f, f),
        b("vplzcntq", Cd, Vv, Int, 8, f, f),
        b("vpconflictd", Cd, Vv, Int, 4, f, f),
        b("vpconflictq", Cd, Vv, Int, 8, f, f),
        // ---- opmask ALU (VEX-encoded) ----
        b("kandw", F, MaskRRR, Int, 0, f, f),
        b("kandq", Bw, MaskRRR, Int, 0, f, f),
        b("kandb", Dq, MaskRRR, Int, 0, f, f),
        b("kandd", Dq, MaskRRR, Int, 0, f, f),
        b("kandnw", F, MaskRRR, Int, 0, f, f),
        b("kandnb", Dq, MaskRRR, Int, 0, f, f),
        b("kandnd", Dq, MaskRRR, Int, 0, f, f),
        b("kandnq", Bw, MaskRRR, Int, 0, f, f),
        b("korw", F, MaskRRR, Int, 0, f, f),
        b("korq", Bw, MaskRRR, Int, 0, f, f),
        b("korb", Dq, MaskRRR, Int, 0, f, f),
        b("kord", Dq, MaskRRR, Int, 0, f, f),
        b("kxorw", F, MaskRRR, Int, 0, f, f),
        b("kxorb", Dq, MaskRRR, Int, 0, f, f),
        b("kxord", Dq, MaskRRR, Int, 0, f, f),
        b("kxorq", Bw, MaskRRR, Int, 0, f, f),
        b("kxnorw", F, MaskRRR, Int, 0, f, f),
        b("kxnorb", Dq, MaskRRR, Int, 0, f, f),
        b("kxnord", Dq, MaskRRR, Int, 0, f, f),
        b("kxnorq", Bw, MaskRRR, Int, 0, f, f),
        b("kaddw", Dq, MaskRRR, Int, 0, f, f),
        b("kaddb", Dq, MaskRRR, Int, 0, f, f),
        b("kaddd", Dq, MaskRRR, Int, 0, f, f),
        b("kaddq", Bw, MaskRRR, Int, 0, f, f),
        b("knotw", F, MaskRR, Int, 0, f, f),
        b("knotb", Dq, MaskRR, Int, 0, f, f),
        b("knotd", Dq, MaskRR, Int, 0, f, f),
        b("knotq", Bw, MaskRR, Int, 0, f, f),
        b("ktestw", F, MaskRR, Int, 0, f, f),
        b("ktestb", Dq, MaskRR, Int, 0, f, f),
        b("ktestd", Dq, MaskRR, Int, 0, f, f),
        b("ktestq", Bw, MaskRR, Int, 0, f, f),
        b("kortestw", F, MaskRR, Int, 0, f, f),
        b("kortestb", Dq, MaskRR, Int, 0, f, f),
        b("kortestd", Dq, MaskRR, Int, 0, f, f),
        b("kortestq", Bw, MaskRR, Int, 0, f, f),
        b("kshiftlb", Dq, MaskRRI(3), Int, 0, f, f),
        b("kshiftlw", F, MaskRRI(3), Int, 0, f, f),
        b("kshiftld", Dq, MaskRRI(5), Int, 0, f, f),
        b("kshiftrw", F, MaskRRI(3), Int, 0, f, f),
        b("kshiftrb", Dq, MaskRRI(3), Int, 0, f, f),
        b("kshiftrd", Dq, MaskRRI(5), Int, 0, f, f),
        b("kshiftlq", Bw, MaskRRI(5), Int, 0, f, f),
        b("kshiftrq", Bw, MaskRRI(5), Int, 0, f, f),
        b("kunpckbw", F, MaskRRR, Int, 0, f, f),
        b("kunpckwd", F, MaskRRR, Int, 0, f, f),
        b("kunpckdq", Bw, MaskRRR, Int, 0, f, f),
    ]
}

/// Instructions whose source and destination have *different* widths (the
/// generator assumes a uniform width), written out explicitly with correct
/// operand register classes. Masking is appended where the form allows it.
fn irregular_cases() -> Vec<Case> {
    use Feat::*;
    use InputProfile::*;
    let mut out = Vec::new();

    // (label, no-mask AT&T, feature, profile, maskable)
    let table: &[(&str, &str, Feat, InputProfile, bool)] = &[
        // widening converts (src ymm -> dst zmm)
        ("vcvtps2pd", "vcvtps2pd %ymm3, %zmm1", F, F32, true),
        ("vcvtdq2pd", "vcvtdq2pd %ymm3, %zmm1", F, Int, true),
        ("vcvtudq2pd", "vcvtudq2pd %ymm3, %zmm1", F, Int, true),
        ("vcvtps2qq", "vcvtps2qq %ymm3, %zmm1", Dq, F32, true),
        ("vcvttps2qq", "vcvttps2qq %ymm3, %zmm1", Dq, F32, true),
        ("vcvtps2uqq", "vcvtps2uqq %ymm3, %zmm1", Dq, F32, true),
        // narrowing converts (src zmm -> dst ymm)
        ("vcvtpd2ps", "vcvtpd2ps %zmm3, %ymm1", F, F64, true),
        ("vcvtpd2dq", "vcvtpd2dq %zmm3, %ymm1", F, F64, true),
        ("vcvtpd2udq", "vcvtpd2udq %zmm3, %ymm1", F, F64, true),
        ("vcvttpd2dq", "vcvttpd2dq %zmm3, %ymm1", F, F64, true),
        ("vcvtqq2ps", "vcvtqq2ps %zmm3, %ymm1", Dq, Int, true),
        ("vcvtuqq2ps", "vcvtuqq2ps %zmm3, %ymm1", Dq, Int, true),
        // sign/zero-extend moves (src xmm/ymm -> dst zmm)
        ("vpmovzxbd", "vpmovzxbd %xmm3, %zmm1", F, Int, true),
        ("vpmovzxwd", "vpmovzxwd %ymm3, %zmm1", F, Int, true),
        ("vpmovzxdq", "vpmovzxdq %ymm3, %zmm1", F, Int, true),
        ("vpmovzxbq", "vpmovzxbq %xmm3, %zmm1", F, Int, true),
        ("vpmovzxwq", "vpmovzxwq %xmm3, %zmm1", F, Int, true),
        ("vpmovsxbd", "vpmovsxbd %xmm3, %zmm1", F, Int, true),
        ("vpmovsxwd", "vpmovsxwd %ymm3, %zmm1", F, Int, true),
        ("vpmovsxdq", "vpmovsxdq %ymm3, %zmm1", F, Int, true),
        ("vpmovsxbq", "vpmovsxbq %xmm3, %zmm1", F, Int, true),
        ("vpmovsxwq", "vpmovsxwq %xmm3, %zmm1", F, Int, true),
        ("vpmovzxbw", "vpmovzxbw %ymm3, %zmm1", Bw, Int, true),
        ("vpmovsxbw", "vpmovsxbw %ymm3, %zmm1", Bw, Int, true),
        // truncating moves (src zmm -> dst xmm/ymm)
        ("vpmovdb", "vpmovdb %zmm3, %xmm1", F, Int, true),
        ("vpmovdw", "vpmovdw %zmm3, %ymm1", F, Int, true),
        ("vpmovqd", "vpmovqd %zmm3, %ymm1", F, Int, true),
        ("vpmovqw", "vpmovqw %zmm3, %xmm1", F, Int, true),
        ("vpmovqb", "vpmovqb %zmm3, %xmm1", F, Int, true),
        ("vpmovwb", "vpmovwb %zmm3, %ymm1", Bw, Int, true),
        ("vpmovsdb", "vpmovsdb %zmm3, %xmm1", F, Int, true),
        ("vpmovusdb", "vpmovusdb %zmm3, %xmm1", F, Int, true),
        ("vpmovsdw", "vpmovsdw %zmm3, %ymm1", F, Int, true),
        ("vpmovswb", "vpmovswb %zmm3, %ymm1", F, Int, true),
        ("vpmovsqb", "vpmovsqb %zmm3, %xmm1", F, Int, true),
        ("vpmovsqw", "vpmovsqw %zmm3, %xmm1", F, Int, true),
        ("vpmovsqd", "vpmovsqd %zmm3, %ymm1", F, Int, true),
        ("vpmovuswb", "vpmovuswb %zmm3, %ymm1", F, Int, true),
        ("vpmovusqb", "vpmovusqb %zmm3, %xmm1", F, Int, true),
        ("vpmovusdw", "vpmovusdw %zmm3, %ymm1", F, Int, true),
        ("vpmovusqw", "vpmovusqw %zmm3, %xmm1", F, Int, true),
        ("vpmovusqd", "vpmovusqd %zmm3, %ymm1", F, Int, true),
        // broadcasts from xmm scalar / m128 / m256
        ("vpbroadcastd", "vpbroadcastd %xmm3, %zmm1", F, Int, true),
        ("vpbroadcastq", "vpbroadcastq %xmm3, %zmm1", F, Int, true),
        ("vpbroadcastb", "vpbroadcastb %xmm3, %zmm1", Bw, Int, true),
        ("vpbroadcastw", "vpbroadcastw %xmm3, %zmm1", Bw, Int, true),
        ("vbroadcastss", "vbroadcastss %xmm3, %zmm1", F, F32, true),
        ("vbroadcastsd", "vbroadcastsd %xmm3, %zmm1", F, F64, true),
        // mask/vector transfers and mask broadcasts
        ("vpmovm2b", "vpmovm2b %k2, %zmm1", Bw, Int, false),
        ("vpmovm2w", "vpmovm2w %k2, %zmm1", Bw, Int, false),
        ("vpmovb2m", "vpmovb2m %zmm2, %k5", Bw, Int, false),
        ("vpmovw2m", "vpmovw2m %zmm2, %k5", Bw, Int, false),
        ("vpmovm2d", "vpmovm2d %k2, %zmm1", Dq, Int, false),
        ("vpmovm2q", "vpmovm2q %k2, %zmm1", Dq, Int, false),
        ("vpmovd2m", "vpmovd2m %zmm2, %k5", Dq, Int, false),
        ("vpmovq2m", "vpmovq2m %zmm2, %k5", Dq, Int, false),
        (
            "vpbroadcastmb2q",
            "vpbroadcastmb2q %k2, %zmm1",
            Cd,
            Int,
            false,
        ),
        (
            "vpbroadcastmw2d",
            "vpbroadcastmw2d %k2, %zmm1",
            Cd,
            Int,
            false,
        ),
        (
            "vbroadcasti32x4",
            "vbroadcasti32x4 (%rax), %zmm1",
            F,
            Int,
            true,
        ),
        (
            "vbroadcastf32x4",
            "vbroadcastf32x4 (%rax), %zmm1",
            F,
            F32,
            true,
        ),
        (
            "vbroadcasti64x4",
            "vbroadcasti64x4 (%rax), %zmm1",
            F,
            Int,
            true,
        ),
        (
            "vbroadcastf64x4",
            "vbroadcastf64x4 (%rax), %zmm1",
            F,
            F64,
            true,
        ),
        (
            "vbroadcasti32x8",
            "vbroadcasti32x8 (%rax), %zmm1",
            Dq,
            Int,
            true,
        ),
        (
            "vbroadcastf64x2",
            "vbroadcastf64x2 (%rax), %zmm1",
            Dq,
            F64,
            true,
        ),
        // compress / expand (mask selects packed elements)
        ("vpcompressd", "vpcompressd %zmm2, %zmm1", F, Int, true),
        ("vpcompressq", "vpcompressq %zmm2, %zmm1", F, Int, true),
        ("vpexpandd", "vpexpandd %zmm2, %zmm1", F, Int, true),
        ("vpexpandq", "vpexpandq %zmm2, %zmm1", F, Int, true),
        ("vcompressps", "vcompressps %zmm2, %zmm1", F, F32, true),
        ("vexpandps", "vexpandps %zmm2, %zmm1", F, F32, true),
        ("vcompresspd", "vcompresspd %zmm2, %zmm1", F, F64, true),
        ("vexpandpd", "vexpandpd %zmm2, %zmm1", F, F64, true),
        // byte/word compress-expand (VBMI2)
        ("vpcompressb", "vpcompressb %zmm2, %zmm1", Vbmi2, Int, true),
        ("vpcompressw", "vpcompressw %zmm2, %zmm1", Vbmi2, Int, true),
        ("vpexpandb", "vpexpandb %zmm2, %zmm1", Vbmi2, Int, true),
        ("vpexpandw", "vpexpandw %zmm2, %zmm1", Vbmi2, Int, true),
        // BF16 narrowing conversions.
        (
            "vcvtneps2bf16",
            "vcvtneps2bf16 %zmm3, %ymm1",
            Bf16,
            F32,
            true,
        ),
        (
            "vcvtne2ps2bf16",
            "vcvtne2ps2bf16 %zmm3, %zmm2, %zmm1",
            Bf16,
            F32,
            true,
        ),
        // FP32/FP64 scalar arithmetic/min/max/sqrt and comparisons.
        ("vaddss", "{evex} vaddss %xmm2, %xmm3, %xmm1", F, F32, true),
        ("vsubss", "{evex} vsubss %xmm2, %xmm3, %xmm1", F, F32, true),
        ("vmulss", "{evex} vmulss %xmm2, %xmm3, %xmm1", F, F32, true),
        ("vdivss", "{evex} vdivss %xmm2, %xmm3, %xmm1", F, F32, true),
        ("vminss", "{evex} vminss %xmm2, %xmm3, %xmm1", F, F32, true),
        ("vmaxss", "{evex} vmaxss %xmm2, %xmm3, %xmm1", F, F32, true),
        (
            "vsqrtss",
            "{evex} vsqrtss %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vcmpss",
            "{evex} vcmpss $1, %xmm2, %xmm3, %k5",
            F,
            F32,
            false,
        ),
        (
            "vcmpss_merge",
            "{evex} vcmpss $1, %xmm2, %xmm3, %k5 {%k1}",
            F,
            F32,
            false,
        ),
        (
            "vaddss_mem",
            "{evex} vaddss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vsubss_mem",
            "{evex} vsubss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vmulss_mem",
            "{evex} vmulss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vdivss_mem",
            "{evex} vdivss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vminss_mem",
            "{evex} vminss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vmaxss_mem",
            "{evex} vmaxss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vsqrtss_mem",
            "{evex} vsqrtss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vcmpss_mem",
            "{evex} vcmpss $1, (%rax), %xmm3, %k5",
            F,
            F32,
            false,
        ),
        (
            "vcmpss_mem_merge",
            "{evex} vcmpss $1, (%rax), %xmm3, %k5 {%k1}",
            F,
            F32,
            false,
        ),
        ("vaddsd", "{evex} vaddsd %xmm2, %xmm3, %xmm1", F, F64, true),
        ("vsubsd", "{evex} vsubsd %xmm2, %xmm3, %xmm1", F, F64, true),
        ("vmulsd", "{evex} vmulsd %xmm2, %xmm3, %xmm1", F, F64, true),
        ("vdivsd", "{evex} vdivsd %xmm2, %xmm3, %xmm1", F, F64, true),
        ("vminsd", "{evex} vminsd %xmm2, %xmm3, %xmm1", F, F64, true),
        ("vmaxsd", "{evex} vmaxsd %xmm2, %xmm3, %xmm1", F, F64, true),
        (
            "vsqrtsd",
            "{evex} vsqrtsd %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vcmpsd",
            "{evex} vcmpsd $2, %xmm2, %xmm3, %k5",
            F,
            F64,
            false,
        ),
        (
            "vcmpsd_merge",
            "{evex} vcmpsd $2, %xmm2, %xmm3, %k5 {%k1}",
            F,
            F64,
            false,
        ),
        (
            "vaddsd_mem",
            "{evex} vaddsd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vsubsd_mem",
            "{evex} vsubsd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vmulsd_mem",
            "{evex} vmulsd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vdivsd_mem",
            "{evex} vdivsd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vminsd_mem",
            "{evex} vminsd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vmaxsd_mem",
            "{evex} vmaxsd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vsqrtsd_mem",
            "{evex} vsqrtsd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vcmpsd_mem",
            "{evex} vcmpsd $2, (%rax), %xmm3, %k5",
            F,
            F64,
            false,
        ),
        (
            "vcmpsd_mem_merge",
            "{evex} vcmpsd $2, (%rax), %xmm3, %k5 {%k1}",
            F,
            F64,
            false,
        ),
        // FP32/FP64 scalar fused multiply-add/subtract.
        (
            "vfmadd132ss",
            "{evex} vfmadd132ss %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfmadd213ss",
            "{evex} vfmadd213ss %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfmadd231ss",
            "{evex} vfmadd231ss %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfmsub132ss",
            "{evex} vfmsub132ss %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfmsub213ss",
            "{evex} vfmsub213ss %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfmsub231ss",
            "{evex} vfmsub231ss %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfnmadd132ss",
            "{evex} vfnmadd132ss %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfnmadd213ss",
            "{evex} vfnmadd213ss %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfnmadd231ss",
            "{evex} vfnmadd231ss %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfnmsub132ss",
            "{evex} vfnmsub132ss %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfnmsub213ss",
            "{evex} vfnmsub213ss %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfnmsub231ss",
            "{evex} vfnmsub231ss %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfmadd132ss_mem",
            "{evex} vfmadd132ss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfmadd213ss_mem",
            "{evex} vfmadd213ss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfmadd231ss_mem",
            "{evex} vfmadd231ss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfmsub132ss_mem",
            "{evex} vfmsub132ss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfmsub213ss_mem",
            "{evex} vfmsub213ss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfmsub231ss_mem",
            "{evex} vfmsub231ss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfnmadd132ss_mem",
            "{evex} vfnmadd132ss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfnmadd213ss_mem",
            "{evex} vfnmadd213ss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfnmadd231ss_mem",
            "{evex} vfnmadd231ss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfnmsub132ss_mem",
            "{evex} vfnmsub132ss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfnmsub213ss_mem",
            "{evex} vfnmsub213ss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfnmsub231ss_mem",
            "{evex} vfnmsub231ss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfmadd132sd",
            "{evex} vfmadd132sd %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfmadd213sd",
            "{evex} vfmadd213sd %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfmadd231sd",
            "{evex} vfmadd231sd %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfmsub132sd",
            "{evex} vfmsub132sd %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfmsub213sd",
            "{evex} vfmsub213sd %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfmsub231sd",
            "{evex} vfmsub231sd %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfnmadd132sd",
            "{evex} vfnmadd132sd %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfnmadd213sd",
            "{evex} vfnmadd213sd %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfnmadd231sd",
            "{evex} vfnmadd231sd %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfnmsub132sd",
            "{evex} vfnmsub132sd %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfnmsub213sd",
            "{evex} vfnmsub213sd %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfnmsub231sd",
            "{evex} vfnmsub231sd %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfmadd132sd_mem",
            "{evex} vfmadd132sd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfmadd213sd_mem",
            "{evex} vfmadd213sd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfmadd231sd_mem",
            "{evex} vfmadd231sd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfmsub132sd_mem",
            "{evex} vfmsub132sd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfmsub213sd_mem",
            "{evex} vfmsub213sd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfmsub231sd_mem",
            "{evex} vfmsub231sd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfnmadd132sd_mem",
            "{evex} vfnmadd132sd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfnmadd213sd_mem",
            "{evex} vfnmadd213sd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfnmadd231sd_mem",
            "{evex} vfnmadd231sd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfnmsub132sd_mem",
            "{evex} vfnmsub132sd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfnmsub213sd_mem",
            "{evex} vfnmsub213sd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfnmsub231sd_mem",
            "{evex} vfnmsub231sd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        // FP32/FP64 scalar transforms, reductions, range/fixup, and approximations.
        (
            "vscalefss",
            "{evex} vscalefss %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vscalefss_mem",
            "{evex} vscalefss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vgetexpss",
            "{evex} vgetexpss %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vgetexpss_mem",
            "{evex} vgetexpss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vgetmantss",
            "{evex} vgetmantss $0, %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vgetmantss_mem",
            "{evex} vgetmantss $0, (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vrndscaless",
            "{evex} vrndscaless $0, %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vrndscaless_mem",
            "{evex} vrndscaless $0, (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vreducess",
            "{evex} vreducess $0, %xmm2, %xmm3, %xmm1",
            Dq,
            F32,
            true,
        ),
        (
            "vreducess_mem",
            "{evex} vreducess $0, (%rax), %xmm3, %xmm1",
            Dq,
            F32,
            true,
        ),
        (
            "vrangess",
            "{evex} vrangess $0, %xmm2, %xmm3, %xmm1",
            Dq,
            F32,
            true,
        ),
        (
            "vrangess_mem",
            "{evex} vrangess $0, (%rax), %xmm3, %xmm1",
            Dq,
            F32,
            true,
        ),
        (
            "vfixupimmss",
            "{evex} vfixupimmss $0, %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vfixupimmss_mem",
            "{evex} vfixupimmss $0, (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vrcp14ss",
            "{evex} vrcp14ss %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vrcp14ss_mem",
            "{evex} vrcp14ss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vrsqrt14ss",
            "{evex} vrsqrt14ss %xmm2, %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vrsqrt14ss_mem",
            "{evex} vrsqrt14ss (%rax), %xmm3, %xmm1",
            F,
            F32,
            true,
        ),
        (
            "vscalefsd",
            "{evex} vscalefsd %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vscalefsd_mem",
            "{evex} vscalefsd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vgetexpsd",
            "{evex} vgetexpsd %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vgetexpsd_mem",
            "{evex} vgetexpsd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vgetmantsd",
            "{evex} vgetmantsd $0, %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vgetmantsd_mem",
            "{evex} vgetmantsd $0, (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vrndscalesd",
            "{evex} vrndscalesd $0, %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vrndscalesd_mem",
            "{evex} vrndscalesd $0, (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vreducesd",
            "{evex} vreducesd $0, %xmm2, %xmm3, %xmm1",
            Dq,
            F64,
            true,
        ),
        (
            "vreducesd_mem",
            "{evex} vreducesd $0, (%rax), %xmm3, %xmm1",
            Dq,
            F64,
            true,
        ),
        (
            "vrangesd",
            "{evex} vrangesd $0, %xmm2, %xmm3, %xmm1",
            Dq,
            F64,
            true,
        ),
        (
            "vrangesd_mem",
            "{evex} vrangesd $0, (%rax), %xmm3, %xmm1",
            Dq,
            F64,
            true,
        ),
        (
            "vfixupimmsd",
            "{evex} vfixupimmsd $0, %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vfixupimmsd_mem",
            "{evex} vfixupimmsd $0, (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vrcp14sd",
            "{evex} vrcp14sd %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vrcp14sd_mem",
            "{evex} vrcp14sd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vrsqrt14sd",
            "{evex} vrsqrt14sd %xmm2, %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        (
            "vrsqrt14sd_mem",
            "{evex} vrsqrt14sd (%rax), %xmm3, %xmm1",
            F,
            F64,
            true,
        ),
        // FP32/FP64 scalar class tests and ordered/unordered flag compares.
        ("vcomiss", "{evex} vcomiss %xmm2, %xmm1", F, F32, false),
        ("vucomiss", "{evex} vucomiss %xmm2, %xmm1", F, F32, false),
        ("vcomiss_mem", "{evex} vcomiss (%rax), %xmm1", F, F32, false),
        (
            "vucomiss_mem",
            "{evex} vucomiss (%rax), %xmm1",
            F,
            F32,
            false,
        ),
        ("vcomisd", "{evex} vcomisd %xmm2, %xmm1", F, F64, false),
        ("vucomisd", "{evex} vucomisd %xmm2, %xmm1", F, F64, false),
        ("vcomisd_mem", "{evex} vcomisd (%rax), %xmm1", F, F64, false),
        (
            "vucomisd_mem",
            "{evex} vucomisd (%rax), %xmm1",
            F,
            F64,
            false,
        ),
        (
            "vfpclassss",
            "{evex} vfpclassss $3, %xmm2, %k5",
            Dq,
            F32,
            false,
        ),
        (
            "vfpclassss_merge",
            "{evex} vfpclassss $3, %xmm2, %k5 {%k1}",
            Dq,
            F32,
            false,
        ),
        (
            "vfpclassss_mem",
            "{evex} vfpclassss $3, (%rax), %k5",
            Dq,
            F32,
            false,
        ),
        (
            "vfpclassss_mem_merge",
            "{evex} vfpclassss $3, (%rax), %k5 {%k1}",
            Dq,
            F32,
            false,
        ),
        (
            "vfpclasssd",
            "{evex} vfpclasssd $3, %xmm2, %k5",
            Dq,
            F64,
            false,
        ),
        (
            "vfpclasssd_merge",
            "{evex} vfpclasssd $3, %xmm2, %k5 {%k1}",
            Dq,
            F64,
            false,
        ),
        (
            "vfpclasssd_mem",
            "{evex} vfpclasssd $3, (%rax), %k5",
            Dq,
            F64,
            false,
        ),
        (
            "vfpclasssd_mem_merge",
            "{evex} vfpclasssd $3, (%rax), %k5 {%k1}",
            Dq,
            F64,
            false,
        ),
        // FP16 scalar comparisons update status flags.
        ("vcomish", "vcomish %xmm2, %xmm1", Fp16, F16, false),
        ("vucomish", "vucomish %xmm2, %xmm1", Fp16, F16, false),
        ("vcmpsh", "vcmpsh $4, %xmm2, %xmm3, %k5", Fp16, F16, false),
        (
            "vcmpsh_merge",
            "vcmpsh $4, %xmm2, %xmm3, %k5 {%k1}",
            Fp16,
            F16,
            false,
        ),
        ("vcomish_mem", "vcomish (%rax), %xmm1", Fp16, F16, false),
        ("vucomish_mem", "vucomish (%rax), %xmm1", Fp16, F16, false),
        (
            "vcmpsh_mem",
            "vcmpsh $4, (%rax), %xmm3, %k5",
            Fp16,
            F16,
            false,
        ),
        (
            "vcmpsh_mem_merge",
            "vcmpsh $4, (%rax), %xmm3, %k5 {%k1}",
            Fp16,
            F16,
            false,
        ),
        ("vfpclassph", "vfpclassph $3, %zmm3, %k5", Fp16, F16, false),
        ("vfpclasssh", "vfpclasssh $3, %xmm2, %k5", Fp16, F16, false),
        (
            "vfpclasssh_merge",
            "vfpclasssh $3, %xmm2, %k5 {%k1}",
            Fp16,
            F16,
            false,
        ),
        (
            "vfpclasssh_mem",
            "vfpclasssh $3, (%rax), %k5",
            Fp16,
            F16,
            false,
        ),
        (
            "vfpclasssh_mem_merge",
            "vfpclasssh $3, (%rax), %k5 {%k1}",
            Fp16,
            F16,
            false,
        ),
        // FP16 scalar arithmetic/min/max/sqrt.
        ("vaddsh", "vaddsh %xmm2, %xmm3, %xmm1", Fp16, F16, true),
        ("vsubsh", "vsubsh %xmm2, %xmm3, %xmm1", Fp16, F16, true),
        ("vmulsh", "vmulsh %xmm2, %xmm3, %xmm1", Fp16, F16, true),
        ("vdivsh", "vdivsh %xmm2, %xmm3, %xmm1", Fp16, F16, true),
        ("vminsh", "vminsh %xmm2, %xmm3, %xmm1", Fp16, F16, true),
        ("vmaxsh", "vmaxsh %xmm2, %xmm3, %xmm1", Fp16, F16, true),
        ("vsqrtsh", "vsqrtsh %xmm2, %xmm3, %xmm1", Fp16, F16, true),
        (
            "vscalefsh",
            "vscalefsh %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vgetexpsh",
            "vgetexpsh %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vgetmantsh",
            "vgetmantsh $0, %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vrndscalesh",
            "vrndscalesh $0, %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vreducesh",
            "vreducesh $0, %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        ("vrcpsh", "vrcpsh %xmm2, %xmm3, %xmm1", Fp16, F16, true),
        ("vrsqrtsh", "vrsqrtsh %xmm2, %xmm3, %xmm1", Fp16, F16, true),
        ("vaddsh_mem", "vaddsh (%rax), %xmm3, %xmm1", Fp16, F16, true),
        ("vsubsh_mem", "vsubsh (%rax), %xmm3, %xmm1", Fp16, F16, true),
        ("vmulsh_mem", "vmulsh (%rax), %xmm3, %xmm1", Fp16, F16, true),
        ("vdivsh_mem", "vdivsh (%rax), %xmm3, %xmm1", Fp16, F16, true),
        ("vminsh_mem", "vminsh (%rax), %xmm3, %xmm1", Fp16, F16, true),
        ("vmaxsh_mem", "vmaxsh (%rax), %xmm3, %xmm1", Fp16, F16, true),
        (
            "vsqrtsh_mem",
            "vsqrtsh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vscalefsh_mem",
            "vscalefsh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vgetexpsh_mem",
            "vgetexpsh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vgetmantsh_mem",
            "vgetmantsh $0, (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vrndscalesh_mem",
            "vrndscalesh $0, (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vreducesh_mem",
            "vreducesh $0, (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        ("vrcpsh_mem", "vrcpsh (%rax), %xmm3, %xmm1", Fp16, F16, true),
        (
            "vrsqrtsh_mem",
            "vrsqrtsh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        // FP16 scalar FMA and complex arithmetic.
        (
            "vfmadd132sh",
            "vfmadd132sh %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfmadd213sh",
            "vfmadd213sh %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfmadd231sh",
            "vfmadd231sh %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfmsub132sh",
            "vfmsub132sh %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfmsub213sh",
            "vfmsub213sh %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfmsub231sh",
            "vfmsub231sh %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfnmadd132sh",
            "vfnmadd132sh %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfnmadd213sh",
            "vfnmadd213sh %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfnmadd231sh",
            "vfnmadd231sh %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfnmsub132sh",
            "vfnmsub132sh %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfnmsub213sh",
            "vfnmsub213sh %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfnmsub231sh",
            "vfnmsub231sh %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfmadd132sh_mem",
            "vfmadd132sh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfmadd213sh_mem",
            "vfmadd213sh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfmadd231sh_mem",
            "vfmadd231sh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfmsub132sh_mem",
            "vfmsub132sh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfmsub213sh_mem",
            "vfmsub213sh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfmsub231sh_mem",
            "vfmsub231sh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfnmadd132sh_mem",
            "vfnmadd132sh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfnmadd213sh_mem",
            "vfnmadd213sh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfnmadd231sh_mem",
            "vfnmadd231sh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfnmsub132sh_mem",
            "vfnmsub132sh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfnmsub213sh_mem",
            "vfnmsub213sh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfnmsub231sh_mem",
            "vfnmsub231sh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        ("vfmulcsh", "vfmulcsh %xmm2, %xmm3, %xmm1", Fp16, F16, true),
        (
            "vfcmulcsh",
            "vfcmulcsh %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfmaddcsh",
            "vfmaddcsh %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfcmaddcsh",
            "vfcmaddcsh %xmm2, %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfmulcsh_mem",
            "vfmulcsh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfcmulcsh_mem",
            "vfcmulcsh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfmaddcsh_mem",
            "vfmaddcsh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        (
            "vfcmaddcsh_mem",
            "vfcmaddcsh (%rax), %xmm3, %xmm1",
            Fp16,
            F16,
            true,
        ),
        // FP16 packed conversions that change vector width.
        ("vcvtph2ps", "vcvtph2ps %ymm3, %zmm1", Fp16, F16, true),
        ("vcvtph2psx", "vcvtph2psx %ymm3, %zmm1", Fp16, F16, true),
        ("vcvtph2pd", "vcvtph2pd %xmm3, %zmm1", Fp16, F16, true),
        ("vcvtps2phx", "vcvtps2phx %zmm3, %ymm1", Fp16, F32, true),
        ("vcvtpd2ph", "vcvtpd2ph %zmm3, %xmm1", Fp16, F64, true),
        ("vcvtph2dq", "vcvtph2dq %ymm3, %zmm1", Fp16, F16, true),
        ("vcvttph2dq", "vcvttph2dq %ymm3, %zmm1", Fp16, F16, true),
        ("vcvtph2udq", "vcvtph2udq %ymm3, %zmm1", Fp16, F16, true),
        ("vcvttph2udq", "vcvttph2udq %ymm3, %zmm1", Fp16, F16, true),
        ("vcvtph2qq", "vcvtph2qq %xmm3, %zmm1", Fp16, F16, true),
        ("vcvttph2qq", "vcvttph2qq %xmm3, %zmm1", Fp16, F16, true),
        ("vcvtph2uqq", "vcvtph2uqq %xmm3, %zmm1", Fp16, F16, true),
        ("vcvttph2uqq", "vcvttph2uqq %xmm3, %zmm1", Fp16, F16, true),
        ("vcvtph2w", "vcvtph2w %zmm3, %zmm1", Fp16, F16, true),
        ("vcvttph2w", "vcvttph2w %zmm3, %zmm1", Fp16, F16, true),
        ("vcvtph2uw", "vcvtph2uw %zmm3, %zmm1", Fp16, F16, true),
        ("vcvttph2uw", "vcvttph2uw %zmm3, %zmm1", Fp16, F16, true),
        ("vcvtdq2ph", "vcvtdq2ph %zmm3, %ymm1", Fp16, Int, true),
        ("vcvtqq2ph", "vcvtqq2ph %zmm3, %xmm1", Fp16, Int, true),
        ("vcvtudq2ph", "vcvtudq2ph %zmm3, %ymm1", Fp16, Int, true),
        ("vcvtuqq2ph", "vcvtuqq2ph %zmm3, %xmm1", Fp16, Int, true),
        ("vcvtw2ph", "vcvtw2ph %zmm3, %zmm1", Fp16, Int, true),
        ("vcvtuw2ph", "vcvtuw2ph %zmm3, %zmm1", Fp16, Int, true),
    ];

    for &(label, asm, feat, profile, maskable) in table {
        out.push(Case {
            label: format!("{label}_nomask"),
            asm: asm.to_string(),
            feat,
            profile,
        });
        if maskable {
            out.push(Case {
                label: format!("{label}_merge"),
                asm: format!("{asm} {{%k1}}"),
                feat,
                profile,
            });
            out.push(Case {
                label: format!("{label}_zero"),
                asm: format!("{asm} {{%k1}}{{z}}"),
                feat,
                profile,
            });
        }
    }

    // Narrowing stores write partial vectors to scratch memory. Memory
    // destinations accept merge masks but not EVEX zeroing masks.
    for &(label, asm, feat) in &[
        ("vpmovdb_mem", "vpmovdb %zmm3, (%rax)", F),
        ("vpmovdw_mem", "vpmovdw %zmm3, 32(%rax)", F),
        ("vpmovqd_mem", "vpmovqd %zmm3, 64(%rax)", F),
        ("vpmovqw_mem", "vpmovqw %zmm3, 96(%rax)", F),
        ("vpmovqb_mem", "vpmovqb %zmm3, 112(%rax)", F),
        ("vpmovwb_mem", "vpmovwb %zmm3, 128(%rax)", Bw),
        ("vpmovsdb_mem", "vpmovsdb %zmm3, (%rax)", F),
        ("vpmovsdw_mem", "vpmovsdw %zmm3, 32(%rax)", F),
        ("vpmovsqd_mem", "vpmovsqd %zmm3, 64(%rax)", F),
        ("vpmovsqw_mem", "vpmovsqw %zmm3, 96(%rax)", F),
        ("vpmovsqb_mem", "vpmovsqb %zmm3, 112(%rax)", F),
        ("vpmovswb_mem", "vpmovswb %zmm3, 128(%rax)", F),
        ("vpmovusdb_mem", "vpmovusdb %zmm3, (%rax)", F),
        ("vpmovusdw_mem", "vpmovusdw %zmm3, 32(%rax)", F),
        ("vpmovusqd_mem", "vpmovusqd %zmm3, 64(%rax)", F),
        ("vpmovusqw_mem", "vpmovusqw %zmm3, 96(%rax)", F),
        ("vpmovusqb_mem", "vpmovusqb %zmm3, 112(%rax)", F),
        ("vpmovuswb_mem", "vpmovuswb %zmm3, 128(%rax)", F),
    ] {
        out.push(Case {
            label: format!("{label}_nomask"),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
        out.push(Case {
            label: format!("{label}_merge"),
            asm: format!("{asm} {{%k1}}"),
            feat,
            profile: Int,
        });
    }

    // Extend loads read the compact source from scratch memory and expand into
    // a ZMM destination, including masked merge and zeroing forms.
    for &(label, asm, feat) in &[
        ("vpmovzxbd_memsrc", "vpmovzxbd 16(%rax), %zmm1", F),
        ("vpmovzxwd_memsrc", "vpmovzxwd 32(%rax), %zmm1", F),
        ("vpmovzxdq_memsrc", "vpmovzxdq 64(%rax), %zmm1", F),
        ("vpmovzxbq_memsrc", "vpmovzxbq 8(%rax), %zmm1", F),
        ("vpmovzxwq_memsrc", "vpmovzxwq 16(%rax), %zmm1", F),
        ("vpmovsxbd_memsrc", "vpmovsxbd 16(%rax), %zmm1", F),
        ("vpmovsxwd_memsrc", "vpmovsxwd 32(%rax), %zmm1", F),
        ("vpmovsxdq_memsrc", "vpmovsxdq 64(%rax), %zmm1", F),
        ("vpmovsxbq_memsrc", "vpmovsxbq 8(%rax), %zmm1", F),
        ("vpmovsxwq_memsrc", "vpmovsxwq 16(%rax), %zmm1", F),
        ("vpmovzxbw_memsrc", "vpmovzxbw 32(%rax), %zmm1", Bw),
        ("vpmovsxbw_memsrc", "vpmovsxbw 64(%rax), %zmm1", Bw),
    ] {
        out.push(Case {
            label: format!("{label}_nomask"),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
        out.push(Case {
            label: format!("{label}_merge"),
            asm: format!("{asm} {{%k1}}"),
            feat,
            profile: Int,
        });
        out.push(Case {
            label: format!("{label}_zero"),
            asm: format!("{asm} {{%k1}}{{z}}"),
            feat,
            profile: Int,
        });
    }

    // Selected compare predicates beyond the single baseline immediate in
    // `base_table()`. K-destination compares do not accept {z}, so enumerate
    // no-mask and merge forms explicitly.
    let mut push_compare = |label: String, asm: String, feat: Feat, profile: InputProfile| {
        out.push(Case {
            label,
            asm,
            feat,
            profile,
        });
    };

    for &(mnem, feat) in &[
        ("vpcmpd", F),
        ("vpcmpud", F),
        ("vpcmpq", F),
        ("vpcmpuq", F),
        ("vpcmpb", Bw),
        ("vpcmpub", Bw),
        ("vpcmpw", Bw),
        ("vpcmpuw", Bw),
    ] {
        for pred in 0..=7 {
            push_compare(
                format!("{mnem}_pred{pred}_reg"),
                format!("{mnem} ${pred}, %zmm2, %zmm3, %k5"),
                feat,
                Int,
            );
            push_compare(
                format!("{mnem}_pred{pred}_reg_merge"),
                format!("{mnem} ${pred}, %zmm2, %zmm3, %k5 {{%k1}}"),
                feat,
                Int,
            );
            push_compare(
                format!("{mnem}_pred{pred}_mem"),
                format!("{mnem} ${pred}, (%rax), %zmm3, %k5"),
                feat,
                Int,
            );
            push_compare(
                format!("{mnem}_pred{pred}_mem_merge"),
                format!("{mnem} ${pred}, (%rax), %zmm3, %k5 {{%k1}}"),
                feat,
                Int,
            );
        }
    }

    let fp_predicates = [0u8, 1, 4, 7, 16, 17, 30, 31];
    for &(mnem, feat, profile) in &[
        ("vcmpps", F, F32),
        ("vcmppd", F, F64),
        ("vcmpph", Fp16, F16),
    ] {
        for pred in fp_predicates {
            push_compare(
                format!("{mnem}_pred{pred}_reg"),
                format!("{mnem} ${pred}, %zmm2, %zmm3, %k5"),
                feat,
                profile,
            );
            push_compare(
                format!("{mnem}_pred{pred}_reg_merge"),
                format!("{mnem} ${pred}, %zmm2, %zmm3, %k5 {{%k1}}"),
                feat,
                profile,
            );
            push_compare(
                format!("{mnem}_pred{pred}_mem"),
                format!("{mnem} ${pred}, (%rax), %zmm3, %k5"),
                feat,
                profile,
            );
            push_compare(
                format!("{mnem}_pred{pred}_mem_merge"),
                format!("{mnem} ${pred}, (%rax), %zmm3, %k5 {{%k1}}"),
                feat,
                profile,
            );
        }
    }

    for &(mnem, prefix, feat, profile) in &[
        ("vcmpss", "{evex} ", F, F32),
        ("vcmpsd", "{evex} ", F, F64),
        ("vcmpsh", "", Fp16, F16),
    ] {
        for pred in fp_predicates {
            push_compare(
                format!("{mnem}_pred{pred}_reg"),
                format!("{prefix}{mnem} ${pred}, %xmm2, %xmm3, %k5"),
                feat,
                profile,
            );
            push_compare(
                format!("{mnem}_pred{pred}_reg_merge"),
                format!("{prefix}{mnem} ${pred}, %xmm2, %xmm3, %k5 {{%k1}}"),
                feat,
                profile,
            );
            push_compare(
                format!("{mnem}_pred{pred}_mem"),
                format!("{prefix}{mnem} ${pred}, (%rax), %xmm3, %k5"),
                feat,
                profile,
            );
            push_compare(
                format!("{mnem}_pred{pred}_mem_merge"),
                format!("{prefix}{mnem} ${pred}, (%rax), %xmm3, %k5 {{%k1}}"),
                feat,
                profile,
            );
        }
    }

    // AVX-512VL permute/shuffle/blend forms. These stay explicit because some
    // related mnemonics have only 256-bit VL encodings, not 128-bit forms.
    let mut push_vl_maskable = |label: String, asm: String, profile: InputProfile| {
        out.push(Case {
            label: format!("{label}_nomask"),
            asm: asm.clone(),
            feat: Vl,
            profile,
        });
        out.push(Case {
            label: format!("{label}_merge"),
            asm: format!("{asm} {{%k1}}"),
            feat: Vl,
            profile,
        });
        out.push(Case {
            label: format!("{label}_zero"),
            asm: format!("{asm} {{%k1}}{{z}}"),
            feat: Vl,
            profile,
        });
    };

    for &(mnem, profile) in &[
        ("vpblendmd", Int),
        ("vpblendmq", Int),
        ("vblendmps", F32),
        ("vblendmpd", F64),
    ] {
        for class in ["xmm", "ymm"] {
            push_vl_maskable(
                format!("{mnem}_{class}_reg"),
                format!("{{evex}} {mnem} %{class}2, %{class}3, %{class}1"),
                profile,
            );
            push_vl_maskable(
                format!("{mnem}_{class}_mem"),
                format!("{{evex}} {mnem} (%rax), %{class}3, %{class}1"),
                profile,
            );
        }
    }

    for &(mnem, imm, profile) in &[("vshufps", 0x1b, F32), ("vshufpd", 0x05, F64)] {
        for class in ["xmm", "ymm"] {
            push_vl_maskable(
                format!("{mnem}_{class}_reg"),
                format!("{{evex}} {mnem} ${imm}, %{class}2, %{class}3, %{class}1"),
                profile,
            );
            push_vl_maskable(
                format!("{mnem}_{class}_mem"),
                format!("{{evex}} {mnem} ${imm}, (%rax), %{class}3, %{class}1"),
                profile,
            );
        }
    }

    for &(mnem, imm, profile) in &[("vpermilps", 0x1b, F32), ("vpermilpd", 0x05, F64)] {
        for class in ["xmm", "ymm"] {
            push_vl_maskable(
                format!("{mnem}_imm_{class}_reg"),
                format!("{{evex}} {mnem} ${imm}, %{class}2, %{class}1"),
                profile,
            );
            push_vl_maskable(
                format!("{mnem}_imm_{class}_mem"),
                format!("{{evex}} {mnem} ${imm}, (%rax), %{class}1"),
                profile,
            );
            push_vl_maskable(
                format!("{mnem}_var_{class}_reg"),
                format!("{{evex}} {mnem} %{class}2, %{class}3, %{class}1"),
                profile,
            );
            push_vl_maskable(
                format!("{mnem}_var_{class}_mem"),
                format!("{{evex}} {mnem} (%rax), %{class}3, %{class}1"),
                profile,
            );
        }
    }

    for &(mnem, imm, profile) in &[("valignd", 3, Int), ("valignq", 1, Int)] {
        for class in ["xmm", "ymm"] {
            push_vl_maskable(
                format!("{mnem}_{class}_reg"),
                format!("{{evex}} {mnem} ${imm}, %{class}2, %{class}3, %{class}1"),
                profile,
            );
            push_vl_maskable(
                format!("{mnem}_{class}_mem"),
                format!("{{evex}} {mnem} ${imm}, (%rax), %{class}3, %{class}1"),
                profile,
            );
        }
    }

    for &(mnem, profile) in &[
        ("vpermd", Int),
        ("vpermq", Int),
        ("vpermps", F32),
        ("vpermpd", F64),
    ] {
        push_vl_maskable(
            format!("{mnem}_var_ymm_reg"),
            format!("{{evex}} {mnem} %ymm2, %ymm3, %ymm1"),
            profile,
        );
        push_vl_maskable(
            format!("{mnem}_var_ymm_mem"),
            format!("{{evex}} {mnem} (%rax), %ymm3, %ymm1"),
            profile,
        );
    }
    for &(mnem, imm, profile) in &[("vpermq", 0x1b, Int), ("vpermpd", 0x1b, F64)] {
        push_vl_maskable(
            format!("{mnem}_imm_ymm_reg"),
            format!("{{evex}} {mnem} ${imm}, %ymm2, %ymm1"),
            profile,
        );
        push_vl_maskable(
            format!("{mnem}_imm_ymm_mem"),
            format!("{{evex}} {mnem} ${imm}, (%rax), %ymm1"),
            profile,
        );
    }

    for &(mnem, profile) in &[
        ("vpermi2d", Int),
        ("vpermt2d", Int),
        ("vpermi2q", Int),
        ("vpermt2q", Int),
        ("vpermi2ps", F32),
        ("vpermt2ps", F32),
        ("vpermi2pd", F64),
        ("vpermt2pd", F64),
    ] {
        for class in ["xmm", "ymm"] {
            push_vl_maskable(
                format!("{mnem}_{class}_reg"),
                format!("{{evex}} {mnem} %{class}2, %{class}3, %{class}1"),
                profile,
            );
            push_vl_maskable(
                format!("{mnem}_{class}_mem"),
                format!("{{evex}} {mnem} (%rax), %{class}3, %{class}1"),
                profile,
            );
        }
    }

    // AVX-512 lane extract/insert forms cover register upper-lane zeroing and
    // memory side effects for both 128-bit and 256-bit chunks.
    for &(label, asm, profile) in &[
        ("vextracti32x4_reg", "vextracti32x4 $2, %zmm2, %xmm3", Int),
        ("vextracti32x4_mem", "vextracti32x4 $2, %zmm2, (%rax)", Int),
        ("vextractf32x4_reg", "vextractf32x4 $3, %zmm2, %xmm3", F32),
        (
            "vextractf32x4_mem",
            "vextractf32x4 $3, %zmm2, 16(%rax)",
            F32,
        ),
        (
            "vinserti32x4_reg",
            "vinserti32x4 $2, %xmm3, %zmm2, %zmm1",
            Int,
        ),
        (
            "vinserti32x4_mem",
            "vinserti32x4 $2, 64(%rax), %zmm2, %zmm1",
            Int,
        ),
        (
            "vinsertf32x4_reg",
            "vinsertf32x4 $3, %xmm3, %zmm2, %zmm1",
            F32,
        ),
        (
            "vinsertf32x4_mem",
            "vinsertf32x4 $3, 64(%rax), %zmm2, %zmm1",
            F32,
        ),
        ("vextracti32x8_reg", "vextracti32x8 $1, %zmm2, %ymm3", Int),
        (
            "vextracti32x8_mem",
            "vextracti32x8 $1, %zmm2, 32(%rax)",
            Int,
        ),
        ("vextractf32x8_reg", "vextractf32x8 $1, %zmm2, %ymm3", F32),
        (
            "vextractf32x8_mem",
            "vextractf32x8 $1, %zmm2, 64(%rax)",
            F32,
        ),
        (
            "vinserti32x8_reg",
            "vinserti32x8 $1, %ymm3, %zmm2, %zmm1",
            Int,
        ),
        (
            "vinserti32x8_mem",
            "vinserti32x8 $1, 64(%rax), %zmm2, %zmm1",
            Int,
        ),
        (
            "vinsertf32x8_reg",
            "vinsertf32x8 $1, %ymm3, %zmm2, %zmm1",
            F32,
        ),
        (
            "vinsertf32x8_mem",
            "vinsertf32x8 $1, 64(%rax), %zmm2, %zmm1",
            F32,
        ),
        ("vextracti64x2_reg", "vextracti64x2 $1, %zmm2, %xmm3", Int),
        (
            "vextracti64x2_mem",
            "vextracti64x2 $1, %zmm2, 32(%rax)",
            Int,
        ),
        ("vextractf64x2_reg", "vextractf64x2 $1, %zmm2, %xmm3", F64),
        (
            "vextractf64x2_mem",
            "vextractf64x2 $1, %zmm2, 48(%rax)",
            F64,
        ),
        (
            "vinserti64x2_reg",
            "vinserti64x2 $1, %xmm3, %zmm2, %zmm1",
            Int,
        ),
        (
            "vinserti64x2_mem",
            "vinserti64x2 $1, 64(%rax), %zmm2, %zmm1",
            Int,
        ),
        (
            "vinsertf64x2_reg",
            "vinsertf64x2 $1, %xmm3, %zmm2, %zmm1",
            F64,
        ),
        (
            "vinsertf64x2_mem",
            "vinsertf64x2 $1, 80(%rax), %zmm2, %zmm1",
            F64,
        ),
        ("vextracti64x4_reg", "vextracti64x4 $1, %zmm2, %ymm3", Int),
        (
            "vextracti64x4_mem",
            "vextracti64x4 $1, %zmm2, 64(%rax)",
            Int,
        ),
        ("vextractf64x4_reg", "vextractf64x4 $1, %zmm2, %ymm3", F64),
        (
            "vextractf64x4_mem",
            "vextractf64x4 $1, %zmm2, 96(%rax)",
            F64,
        ),
        (
            "vinserti64x4_reg",
            "vinserti64x4 $1, %ymm3, %zmm2, %zmm1",
            Int,
        ),
        (
            "vinserti64x4_mem",
            "vinserti64x4 $1, 64(%rax), %zmm2, %zmm1",
            Int,
        ),
        (
            "vinsertf64x4_reg",
            "vinsertf64x4 $1, %ymm3, %zmm2, %zmm1",
            F64,
        ),
        (
            "vinsertf64x4_mem",
            "vinsertf64x4 $1, 96(%rax), %zmm2, %zmm1",
            F64,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: F,
            profile,
        });
    }

    // Legacy SSE single-precision forms use destructive two-operand XMM
    // semantics and must preserve all upper AVX/AVX-512 state.
    for &(label, asm, profile) in &[
        ("addps_sse_reg", "addps %xmm2, %xmm1", F32),
        ("addps_sse_mem", "addps (%rax), %xmm1", F32),
        ("addss_sse_reg", "addss %xmm2, %xmm1", F32),
        ("addss_sse_mem", "addss 32(%rax), %xmm1", F32),
        ("subps_sse_reg", "subps %xmm2, %xmm1", F32),
        ("subss_sse_mem", "subss 32(%rax), %xmm1", F32),
        ("mulps_sse_mem", "mulps (%rax), %xmm1", F32),
        ("mulss_sse_reg", "mulss %xmm2, %xmm1", F32),
        ("divps_sse_reg", "divps %xmm2, %xmm1", F32),
        ("divss_sse_mem", "divss 32(%rax), %xmm1", F32),
        ("sqrtps_sse_reg", "sqrtps %xmm3, %xmm1", F32),
        ("sqrtss_sse_mem", "sqrtss 32(%rax), %xmm1", F32),
        ("rsqrtps_sse_reg", "rsqrtps %xmm3, %xmm1", F32),
        ("rsqrtss_sse_mem", "rsqrtss 32(%rax), %xmm1", F32),
        ("rcpps_sse_reg", "rcpps %xmm3, %xmm1", F32),
        ("rcpss_sse_mem", "rcpss 32(%rax), %xmm1", F32),
        ("minps_sse_reg", "minps %xmm2, %xmm1", F32),
        ("minss_sse_mem", "minss 32(%rax), %xmm1", F32),
        ("maxps_sse_reg", "maxps %xmm2, %xmm1", F32),
        ("maxss_sse_mem", "maxss 32(%rax), %xmm1", F32),
        ("unpcklps_sse_reg", "unpcklps %xmm2, %xmm1", F32),
        ("unpckhps_sse_mem", "unpckhps 32(%rax), %xmm1", F32),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Sse,
            profile,
        });
    }

    // Legacy SSE single-precision data movement, logical, compare, shuffle,
    // mask extraction, and scalar integer conversion forms.
    for &(label, asm, profile) in &[
        ("movaps_sse_reg", "movaps %xmm2, %xmm1", F32),
        ("movaps_sse_load", "movaps 16(%rax), %xmm1", F32),
        ("movaps_sse_store", "movaps %xmm1, 16(%rax)", F32),
        ("movups_sse_load_unaligned", "movups 17(%rax), %xmm1", F32),
        ("movups_sse_store_unaligned", "movups %xmm1, 17(%rax)", F32),
        ("movss_sse_reg", "movss %xmm2, %xmm1", F32),
        ("movss_sse_load", "movss 4(%rax), %xmm1", F32),
        ("movss_sse_store", "movss %xmm1, 4(%rax)", F32),
        ("andps_sse_reg", "andps %xmm2, %xmm1", Int),
        ("andnps_sse_mem", "andnps 16(%rax), %xmm1", Int),
        ("orps_sse_reg", "orps %xmm2, %xmm1", Int),
        ("xorps_sse_mem", "xorps 16(%rax), %xmm1", Int),
        ("cmpeqps_sse_reg", "cmpeqps %xmm2, %xmm1", F32),
        ("cmpltps_sse_mem", "cmpltps 16(%rax), %xmm1", F32),
        ("cmpunordps_sse_reg", "cmpunordps %xmm2, %xmm1", F32),
        ("cmpeqss_sse_reg", "cmpeqss %xmm2, %xmm1", F32),
        ("cmpnltss_sse_mem", "cmpnltss 16(%rax), %xmm1", F32),
        ("shufps_sse_reg", "shufps $0x1b, %xmm2, %xmm1", F32),
        ("shufps_sse_mem", "shufps $0xb1, 16(%rax), %xmm1", F32),
        ("movmskps_sse_r8d", "movmskps %xmm1, %r8d", F32),
        ("comiss_sse_reg", "comiss %xmm2, %xmm1", F32),
        ("comiss_sse_mem", "comiss 16(%rax), %xmm1", F32),
        ("ucomiss_sse_reg", "ucomiss %xmm2, %xmm1", F32),
        ("ucomiss_sse_mem", "ucomiss 16(%rax), %xmm1", F32),
        ("cvtsi2ss_sse_r64", "cvtsi2ss %r8, %xmm1", F32),
        ("cvtsi2ss_sse_m32", "cvtsi2ss 16(%rax), %xmm1", F32),
        ("cvtss2si_sse_xmm_r8", "cvtss2si %xmm1, %r8", F32),
        ("cvtss2si_sse_m32_r8", "cvtss2si 16(%rax), %r8", F32),
        ("cvttss2si_sse_xmm_r8", "cvttss2si %xmm1, %r8", F32),
        ("cvttss2si_sse_m32_r8", "cvttss2si 16(%rax), %r8", F32),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Sse,
            profile,
        });
    }

    // Legacy SSE2 double-precision forms cover the operand-size override and
    // F2-prefix variants of the same two-operand XMM execution paths.
    for &(label, asm, profile) in &[
        ("addpd_sse2_reg", "addpd %xmm2, %xmm1", F64),
        ("addpd_sse2_mem", "addpd (%rax), %xmm1", F64),
        ("addsd_sse2_reg", "addsd %xmm2, %xmm1", F64),
        ("addsd_sse2_mem", "addsd 32(%rax), %xmm1", F64),
        ("subpd_sse2_reg", "subpd %xmm2, %xmm1", F64),
        ("subsd_sse2_mem", "subsd 32(%rax), %xmm1", F64),
        ("mulpd_sse2_mem", "mulpd (%rax), %xmm1", F64),
        ("mulsd_sse2_reg", "mulsd %xmm2, %xmm1", F64),
        ("divpd_sse2_reg", "divpd %xmm2, %xmm1", F64),
        ("divsd_sse2_mem", "divsd 32(%rax), %xmm1", F64),
        ("sqrtpd_sse2_reg", "sqrtpd %xmm3, %xmm1", F64),
        ("sqrtsd_sse2_mem", "sqrtsd 32(%rax), %xmm1", F64),
        ("minpd_sse2_reg", "minpd %xmm2, %xmm1", F64),
        ("minsd_sse2_mem", "minsd 32(%rax), %xmm1", F64),
        ("maxpd_sse2_reg", "maxpd %xmm2, %xmm1", F64),
        ("maxsd_sse2_mem", "maxsd 32(%rax), %xmm1", F64),
        ("unpcklpd_sse2_reg", "unpcklpd %xmm2, %xmm1", F64),
        ("unpckhpd_sse2_mem", "unpckhpd 32(%rax), %xmm1", F64),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Sse2,
            profile,
        });
    }

    // Legacy SSE2 double-precision data movement, logical, compare, shuffle,
    // mask extraction, scalar conversion, and packed FP/int conversion forms.
    for &(label, asm, profile) in &[
        ("movapd_sse2_reg", "movapd %xmm2, %xmm1", F64),
        ("movapd_sse2_load", "movapd 16(%rax), %xmm1", F64),
        ("movapd_sse2_store", "movapd %xmm1, 16(%rax)", F64),
        ("movupd_sse2_load_unaligned", "movupd 17(%rax), %xmm1", F64),
        ("movupd_sse2_store_unaligned", "movupd %xmm1, 17(%rax)", F64),
        ("movsd_sse2_reg", "movsd %xmm2, %xmm1", F64),
        ("movsd_sse2_load", "movsd 8(%rax), %xmm1", F64),
        ("movsd_sse2_store", "movsd %xmm1, 8(%rax)", F64),
        ("andpd_sse2_reg", "andpd %xmm2, %xmm1", Int),
        ("andnpd_sse2_mem", "andnpd 16(%rax), %xmm1", Int),
        ("orpd_sse2_reg", "orpd %xmm2, %xmm1", Int),
        ("xorpd_sse2_mem", "xorpd 16(%rax), %xmm1", Int),
        ("cmpeqpd_sse2_reg", "cmpeqpd %xmm2, %xmm1", F64),
        ("cmpltpd_sse2_mem", "cmpltpd 16(%rax), %xmm1", F64),
        ("cmpunordpd_sse2_reg", "cmpunordpd %xmm2, %xmm1", F64),
        ("cmpeqsd_sse2_reg", "cmpeqsd %xmm2, %xmm1", F64),
        ("cmpnltsd_sse2_mem", "cmpnltsd 16(%rax), %xmm1", F64),
        ("shufpd_sse2_reg", "shufpd $0x1, %xmm2, %xmm1", F64),
        ("shufpd_sse2_mem", "shufpd $0x2, 16(%rax), %xmm1", F64),
        ("movmskpd_sse2_r8d", "movmskpd %xmm1, %r8d", F64),
        ("comisd_sse2_reg", "comisd %xmm2, %xmm1", F64),
        ("comisd_sse2_mem", "comisd 16(%rax), %xmm1", F64),
        ("ucomisd_sse2_reg", "ucomisd %xmm2, %xmm1", F64),
        ("ucomisd_sse2_mem", "ucomisd 16(%rax), %xmm1", F64),
        ("cvtsi2sd_sse2_r64", "cvtsi2sd %r8, %xmm1", F64),
        ("cvtsi2sd_sse2_m32", "cvtsi2sd 16(%rax), %xmm1", F64),
        ("cvtsd2si_sse2_xmm_r8", "cvtsd2si %xmm1, %r8", F64),
        ("cvtsd2si_sse2_m64_r8", "cvtsd2si 16(%rax), %r8", F64),
        ("cvttsd2si_sse2_xmm_r8", "cvttsd2si %xmm1, %r8", F64),
        ("cvttsd2si_sse2_m64_r8", "cvttsd2si 16(%rax), %r8", F64),
        ("cvtpd2ps_sse2_reg", "cvtpd2ps %xmm2, %xmm1", F64),
        ("cvtpd2ps_sse2_mem", "cvtpd2ps 16(%rax), %xmm1", F64),
        ("cvtps2pd_sse2_reg", "cvtps2pd %xmm2, %xmm1", F32),
        ("cvtps2pd_sse2_mem", "cvtps2pd 16(%rax), %xmm1", F32),
        ("cvtdq2pd_sse2_reg", "cvtdq2pd %xmm2, %xmm1", Int),
        ("cvtdq2pd_sse2_mem", "cvtdq2pd 16(%rax), %xmm1", Int),
        ("cvtpd2dq_sse2_reg", "cvtpd2dq %xmm2, %xmm1", F64),
        ("cvtpd2dq_sse2_mem", "cvtpd2dq 16(%rax), %xmm1", F64),
        ("cvttpd2dq_sse2_reg", "cvttpd2dq %xmm2, %xmm1", F64),
        ("cvttpd2dq_sse2_mem", "cvttpd2dq 16(%rax), %xmm1", F64),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Sse2,
            profile,
        });
    }

    // Legacy SSE2 packed-integer forms. These cover aligned/unaligned moves,
    // destructive two-operand arithmetic/logical/compare/multiply/pack/unpack,
    // and both immediate and XMM/memory shift-count encodings.
    for &(label, asm) in &[
        ("movdqa_sse2_reg", "movdqa %xmm2, %xmm1"),
        ("movdqa_sse2_load", "movdqa 16(%rax), %xmm1"),
        ("movdqa_sse2_store", "movdqa %xmm1, 16(%rax)"),
        ("movdqu_sse2_load_unaligned", "movdqu 17(%rax), %xmm1"),
        ("movdqu_sse2_store_unaligned", "movdqu %xmm1, 17(%rax)"),
        ("movq_sse2_reg", "movq %xmm2, %xmm1"),
        ("movq_sse2_load", "movq 8(%rax), %xmm1"),
        ("movq_sse2_store", "movq %xmm1, 8(%rax)"),
        ("paddb_sse2_reg", "paddb %xmm2, %xmm1"),
        ("paddw_sse2_mem", "paddw 16(%rax), %xmm1"),
        ("paddd_sse2_reg", "paddd %xmm2, %xmm1"),
        ("paddq_sse2_mem", "paddq 16(%rax), %xmm1"),
        ("paddsb_sse2_reg", "paddsb %xmm2, %xmm1"),
        ("paddsw_sse2_mem", "paddsw 16(%rax), %xmm1"),
        ("paddusb_sse2_reg", "paddusb %xmm2, %xmm1"),
        ("paddusw_sse2_mem", "paddusw 16(%rax), %xmm1"),
        ("psubb_sse2_reg", "psubb %xmm2, %xmm1"),
        ("psubw_sse2_mem", "psubw 16(%rax), %xmm1"),
        ("psubd_sse2_reg", "psubd %xmm2, %xmm1"),
        ("psubq_sse2_mem", "psubq 16(%rax), %xmm1"),
        ("psubsb_sse2_reg", "psubsb %xmm2, %xmm1"),
        ("psubsw_sse2_mem", "psubsw 16(%rax), %xmm1"),
        ("psubusb_sse2_reg", "psubusb %xmm2, %xmm1"),
        ("psubusw_sse2_mem", "psubusw 16(%rax), %xmm1"),
        ("pand_sse2_reg", "pand %xmm2, %xmm1"),
        ("pandn_sse2_mem", "pandn 16(%rax), %xmm1"),
        ("por_sse2_reg", "por %xmm2, %xmm1"),
        ("pxor_sse2_mem", "pxor 16(%rax), %xmm1"),
        ("pcmpeqb_sse2_reg", "pcmpeqb %xmm2, %xmm1"),
        ("pcmpeqw_sse2_mem", "pcmpeqw 16(%rax), %xmm1"),
        ("pcmpeqd_sse2_reg", "pcmpeqd %xmm2, %xmm1"),
        ("pcmpgtb_sse2_reg", "pcmpgtb %xmm2, %xmm1"),
        ("pcmpgtw_sse2_mem", "pcmpgtw 16(%rax), %xmm1"),
        ("pcmpgtd_sse2_reg", "pcmpgtd %xmm2, %xmm1"),
        ("pminub_sse2_reg", "pminub %xmm2, %xmm1"),
        ("pminsw_sse2_mem", "pminsw 16(%rax), %xmm1"),
        ("pmaxub_sse2_reg", "pmaxub %xmm2, %xmm1"),
        ("pmaxsw_sse2_mem", "pmaxsw 16(%rax), %xmm1"),
        ("pmullw_sse2_reg", "pmullw %xmm2, %xmm1"),
        ("pmulhw_sse2_mem", "pmulhw 16(%rax), %xmm1"),
        ("pmulhuw_sse2_reg", "pmulhuw %xmm2, %xmm1"),
        ("pmuludq_sse2_mem", "pmuludq 16(%rax), %xmm1"),
        ("pmaddwd_sse2_reg", "pmaddwd %xmm2, %xmm1"),
        ("punpcklbw_sse2_reg", "punpcklbw %xmm2, %xmm1"),
        ("punpcklwd_sse2_mem", "punpcklwd 16(%rax), %xmm1"),
        ("punpckldq_sse2_reg", "punpckldq %xmm2, %xmm1"),
        ("punpcklqdq_sse2_mem", "punpcklqdq 16(%rax), %xmm1"),
        ("punpckhbw_sse2_reg", "punpckhbw %xmm2, %xmm1"),
        ("punpckhwd_sse2_mem", "punpckhwd 16(%rax), %xmm1"),
        ("punpckhdq_sse2_reg", "punpckhdq %xmm2, %xmm1"),
        ("punpckhqdq_sse2_mem", "punpckhqdq 16(%rax), %xmm1"),
        ("packsswb_sse2_reg", "packsswb %xmm2, %xmm1"),
        ("packssdw_sse2_mem", "packssdw 16(%rax), %xmm1"),
        ("packuswb_sse2_reg", "packuswb %xmm2, %xmm1"),
        ("psllw_sse2_imm", "psllw $3, %xmm1"),
        ("pslld_sse2_mem_count", "pslld 16(%rax), %xmm1"),
        ("psllq_sse2_imm", "psllq $7, %xmm1"),
        ("psrlw_sse2_imm", "psrlw $5, %xmm1"),
        ("psrld_sse2_mem_count", "psrld 16(%rax), %xmm1"),
        ("psrlq_sse2_imm", "psrlq $11, %xmm1"),
        ("psraw_sse2_imm", "psraw $2, %xmm1"),
        ("psrad_sse2_mem_count", "psrad 16(%rax), %xmm1"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Sse2,
            profile: Int,
        });
    }

    // Legacy SSSE3 forms cover 0F38 byte shuffles, horizontal arithmetic,
    // sign/absolute-value operations, and 0F3A PALIGNR immediate handling.
    for &(label, asm) in &[
        ("pshufb_ssse3_reg", "pshufb %xmm2, %xmm1"),
        ("pshufb_ssse3_mem", "pshufb (%rax), %xmm1"),
        ("phaddw_ssse3_reg", "phaddw %xmm2, %xmm1"),
        ("phaddd_ssse3_mem", "phaddd 32(%rax), %xmm1"),
        ("phaddsw_ssse3_reg", "phaddsw %xmm2, %xmm1"),
        ("pmaddubsw_ssse3_reg", "pmaddubsw %xmm2, %xmm1"),
        ("pmaddubsw_ssse3_mem", "pmaddubsw 32(%rax), %xmm1"),
        ("phsubw_ssse3_mem", "phsubw 32(%rax), %xmm1"),
        ("phsubd_ssse3_reg", "phsubd %xmm2, %xmm1"),
        ("phsubsw_ssse3_mem", "phsubsw 32(%rax), %xmm1"),
        ("psignb_ssse3_reg", "psignb %xmm2, %xmm1"),
        ("psignw_ssse3_mem", "psignw 32(%rax), %xmm1"),
        ("psignd_ssse3_reg", "psignd %xmm2, %xmm1"),
        ("pmulhrsw_ssse3_mem", "pmulhrsw 32(%rax), %xmm1"),
        ("pabsb_ssse3_reg", "pabsb %xmm2, %xmm1"),
        ("pabsw_ssse3_mem", "pabsw 32(%rax), %xmm1"),
        ("pabsd_ssse3_reg", "pabsd %xmm2, %xmm1"),
        ("palignr_ssse3_reg_5", "palignr $5, %xmm2, %xmm1"),
        ("palignr_ssse3_mem_17", "palignr $17, 32(%rax), %xmm1"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Ssse3,
            profile: Int,
        });
    }

    // Legacy SSE4.1 blend forms use implicit XMM0 masks; PTEST exercises
    // status-flag writes in the same legacy 0F38 map.
    for &(label, asm, profile) in &[
        ("pblendvb_sse41_reg", "pblendvb %xmm2, %xmm1", Int),
        ("pblendvb_sse41_mem", "pblendvb 32(%rax), %xmm1", Int),
        ("blendvps_sse41_reg", "blendvps %xmm2, %xmm1", F32),
        ("blendvps_sse41_mem", "blendvps 32(%rax), %xmm1", F32),
        ("blendvpd_sse41_reg", "blendvpd %xmm2, %xmm1", F64),
        ("blendvpd_sse41_mem", "blendvpd 32(%rax), %xmm1", F64),
        ("ptest_sse41_reg", "ptest %xmm2, %xmm1", Int),
        ("ptest_sse41_mem", "ptest 32(%rax), %xmm1", Int),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Sse41,
            profile,
        });
    }

    // Legacy SSE4.1 0F38 integer/vector forms cover partial-width sign/zero
    // extends, integer min/max, multiply/compare, pack, and non-temporal load.
    for &(label, asm) in &[
        ("pmovsxbw_sse41_reg", "pmovsxbw %xmm2, %xmm1"),
        ("pmovsxbd_sse41_mem", "pmovsxbd 32(%rax), %xmm1"),
        ("pmovsxbq_sse41_reg", "pmovsxbq %xmm2, %xmm1"),
        ("pmovsxwd_sse41_mem", "pmovsxwd 32(%rax), %xmm1"),
        ("pmovsxwq_sse41_reg", "pmovsxwq %xmm2, %xmm1"),
        ("pmovsxdq_sse41_mem", "pmovsxdq 32(%rax), %xmm1"),
        ("pmovzxbw_sse41_reg", "pmovzxbw %xmm2, %xmm1"),
        ("pmovzxbd_sse41_mem", "pmovzxbd 32(%rax), %xmm1"),
        ("pmovzxbq_sse41_reg", "pmovzxbq %xmm2, %xmm1"),
        ("pmovzxwd_sse41_mem", "pmovzxwd 32(%rax), %xmm1"),
        ("pmovzxwq_sse41_reg", "pmovzxwq %xmm2, %xmm1"),
        ("pmovzxdq_sse41_mem", "pmovzxdq 32(%rax), %xmm1"),
        ("pmuldq_sse41_reg", "pmuldq %xmm2, %xmm1"),
        ("pmuldq_sse41_mem", "pmuldq 32(%rax), %xmm1"),
        ("pcmpeqq_sse41_reg", "pcmpeqq %xmm2, %xmm1"),
        ("movntdqa_sse41_mem", "movntdqa 32(%rax), %xmm1"),
        ("packusdw_sse41_reg", "packusdw %xmm2, %xmm1"),
        ("packusdw_sse41_mem", "packusdw 32(%rax), %xmm1"),
        ("pminsb_sse41_reg", "pminsb %xmm2, %xmm1"),
        ("pminsd_sse41_mem", "pminsd 32(%rax), %xmm1"),
        ("pminuw_sse41_reg", "pminuw %xmm2, %xmm1"),
        ("pminud_sse41_mem", "pminud 32(%rax), %xmm1"),
        ("pmaxsb_sse41_reg", "pmaxsb %xmm2, %xmm1"),
        ("pmaxsd_sse41_mem", "pmaxsd 32(%rax), %xmm1"),
        ("pmaxuw_sse41_reg", "pmaxuw %xmm2, %xmm1"),
        ("pmaxud_sse41_mem", "pmaxud 32(%rax), %xmm1"),
        ("pmulld_sse41_reg", "pmulld %xmm2, %xmm1"),
        ("pmulld_sse41_mem", "pmulld 32(%rax), %xmm1"),
        ("phminposuw_sse41_reg", "phminposuw %xmm2, %xmm1"),
        ("phminposuw_sse41_mem", "phminposuw 32(%rax), %xmm1"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Sse41,
            profile: Int,
        });
    }

    // Legacy SSE4.1 0F3A immediate/control forms cover rounding modes, blends,
    // dot products, byte SAD, extract-to-GPR/memory, and insert-from-GPR/memory.
    for &(label, asm, profile) in &[
        ("roundps_sse41_reg", "roundps $1, %xmm2, %xmm1", F32),
        ("roundps_sse41_mem", "roundps $2, 32(%rax), %xmm1", F32),
        ("roundpd_sse41_reg", "roundpd $2, %xmm2, %xmm1", F64),
        ("roundpd_sse41_mem", "roundpd $1, 32(%rax), %xmm1", F64),
        ("roundss_sse41_reg", "roundss $3, %xmm2, %xmm1", F32),
        ("roundss_sse41_mem", "roundss $1, 32(%rax), %xmm1", F32),
        ("roundsd_sse41_reg", "roundsd $1, %xmm2, %xmm1", F64),
        ("roundsd_sse41_mem", "roundsd $2, 32(%rax), %xmm1", F64),
        ("blendps_sse41_reg", "blendps $0x5a, %xmm2, %xmm1", F32),
        ("blendps_sse41_mem", "blendps $0xa5, 32(%rax), %xmm1", F32),
        ("blendpd_sse41_reg", "blendpd $0x1, %xmm2, %xmm1", F64),
        ("blendpd_sse41_mem", "blendpd $0x2, 32(%rax), %xmm1", F64),
        ("pblendw_sse41_reg", "pblendw $0xa5, %xmm2, %xmm1", Int),
        ("pblendw_sse41_mem", "pblendw $0x5a, 32(%rax), %xmm1", Int),
        ("dpps_sse41_reg", "dpps $0xf1, %xmm2, %xmm1", F32),
        ("dpps_sse41_mem", "dpps $0xff, 32(%rax), %xmm1", F32),
        ("dppd_sse41_reg", "dppd $0x31, %xmm2, %xmm1", F64),
        ("dppd_sse41_mem", "dppd $0x33, 32(%rax), %xmm1", F64),
        ("mpsadbw_sse41_reg", "mpsadbw $5, %xmm2, %xmm1", Int),
        ("mpsadbw_sse41_mem", "mpsadbw $2, 32(%rax), %xmm1", Int),
        ("pextrb_sse41_gpr", "pextrb $10, %xmm1, %r8d", Int),
        ("pextrb_sse41_mem", "pextrb $5, %xmm1, 32(%rax)", Int),
        ("pextrw_sse41_mem", "pextrw $4, %xmm1, 34(%rax)", Int),
        ("pextrd_sse41_gpr", "pextrd $2, %xmm1, %r8d", Int),
        ("pextrd_sse41_mem", "pextrd $1, %xmm1, 40(%rax)", Int),
        ("pextrq_sse41_gpr", "pextrq $1, %xmm1, %r8", Int),
        ("pextrq_sse41_mem", "pextrq $0, %xmm1, 48(%rax)", Int),
        ("extractps_sse41_gpr", "extractps $2, %xmm1, %r8d", F32),
        ("extractps_sse41_mem", "extractps $3, %xmm1, 56(%rax)", F32),
        ("pinsrb_sse41_gpr", "pinsrb $14, %r8d, %xmm1", Int),
        ("pinsrb_sse41_mem", "pinsrb $5, 31(%rax), %xmm1", Int),
        ("pinsrd_sse41_gpr", "pinsrd $2, %r8d, %xmm1", Int),
        ("pinsrd_sse41_mem", "pinsrd $1, 28(%rax), %xmm1", Int),
        ("pinsrq_sse41_gpr", "pinsrq $1, %r8, %xmm1", Int),
        ("pinsrq_sse41_mem", "pinsrq $0, 24(%rax), %xmm1", Int),
        ("insertps_sse41_reg", "insertps $0x2c, %xmm2, %xmm1", F32),
        ("insertps_sse41_mem", "insertps $0x10, 12(%rax), %xmm1", F32),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Sse41,
            profile,
        });
    }

    // Legacy SSE4.2 compare/string forms cover qword compares, XMM0 mask
    // results, RCX index results, and the PCMPxSTRx status flags.
    for &(label, asm) in &[
        ("pcmpgtq_sse42_reg", "pcmpgtq %xmm2, %xmm1"),
        ("pcmpgtq_sse42_mem", "pcmpgtq 32(%rax), %xmm1"),
        ("pcmpistrm_sse42_reg_eqany", "pcmpistrm $0x08, %xmm2, %xmm1"),
        (
            "pcmpistrm_sse42_mem_eqany",
            "pcmpistrm $0x08, 32(%rax), %xmm1",
        ),
        ("pcmpistri_sse42_reg_eqany", "pcmpistri $0x08, %xmm2, %xmm1"),
        (
            "pcmpistri_sse42_mem_eqany",
            "pcmpistri $0x08, 32(%rax), %xmm1",
        ),
        (
            "pcmpistrm_sse42_reg_ranges",
            "pcmpistrm $0x3a, %xmm2, %xmm1",
        ),
        (
            "pcmpistri_sse42_reg_ranges",
            "pcmpistri $0x3a, %xmm2, %xmm1",
        ),
        (
            "pcmpistrm_sse42_reg_eqany_u8",
            "pcmpistrm $0x00, %xmm2, %xmm1",
        ),
        (
            "pcmpistri_sse42_reg_eqany_u8",
            "pcmpistri $0x00, %xmm2, %xmm1",
        ),
        (
            "pcmpistrm_sse42_mem_eqordered_u8",
            "pcmpistrm $0x0c, 32(%rax), %xmm1",
        ),
        (
            "pcmpistri_sse42_mem_eqordered_u8",
            "pcmpistri $0x0c, 32(%rax), %xmm1",
        ),
        (
            "pcmpistrm_sse42_reg_eqany_u16_neg",
            "pcmpistrm $0x11, %xmm2, %xmm1",
        ),
        (
            "pcmpistri_sse42_reg_eqany_u16_neg",
            "pcmpistri $0x11, %xmm2, %xmm1",
        ),
        (
            "pcmpistrm_sse42_reg_eqany_bitmask",
            "pcmpistrm $0x40, %xmm2, %xmm1",
        ),
        (
            "pcmpistri_sse42_reg_eqeach_msb",
            "pcmpistri $0x58, %xmm2, %xmm1",
        ),
        (
            "pcmpestrm_sse42_reg_eqany_u8",
            "pcmpestrm $0x00, %xmm2, %xmm1",
        ),
        (
            "pcmpestri_sse42_reg_eqany_u8",
            "pcmpestri $0x00, %xmm2, %xmm1",
        ),
        (
            "pcmpestrm_sse42_mem_eqordered_u8",
            "pcmpestrm $0x0c, 32(%rax), %xmm1",
        ),
        (
            "pcmpestri_sse42_mem_eqordered_u8",
            "pcmpestri $0x0c, 32(%rax), %xmm1",
        ),
        (
            "pcmpestrm_sse42_reg_eqany_u16_neg",
            "pcmpestrm $0x11, %xmm2, %xmm1",
        ),
        (
            "pcmpestri_sse42_reg_eqany_u16_neg",
            "pcmpestri $0x11, %xmm2, %xmm1",
        ),
        (
            "pcmpestrm_sse42_reg_eqany_bitmask",
            "pcmpestrm $0x40, %xmm2, %xmm1",
        ),
        (
            "pcmpestri_sse42_reg_eqeach_msb",
            "pcmpestri $0x58, %xmm2, %xmm1",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Sse42,
            profile: Int,
        });
    }

    // VEX-encoded AVX VNNI dot products are distinct from the EVEX AVX-512
    // VNNI forms in `base_table()`: XMM/YMM only, no write-mask, and VEX upper
    // zeroing semantics.
    for mnem in ["vpdpbusd", "vpdpbusds", "vpdpwssd", "vpdpwssds"] {
        for class in ["xmm", "ymm"] {
            out.push(Case {
                label: format!("{mnem}_vex_{class}_reg"),
                asm: format!("{{vex}} {mnem} %{class}2, %{class}3, %{class}1"),
                feat: AvxVnni,
                profile: Int,
            });
            out.push(Case {
                label: format!("{mnem}_vex_{class}_mem"),
                asm: format!("{{vex}} {mnem} (%rax), %{class}3, %{class}1"),
                feat: AvxVnni,
                profile: Int,
            });
            out.push(Case {
                label: format!("{mnem}_vex_{class}_high"),
                asm: format!("{{vex}} {mnem} %{class}10, %{class}11, %{class}9"),
                feat: AvxVnni,
                profile: Int,
            });
        }
    }

    // AVX/AVX2 VEX mask extraction, masked memory operations, non-temporal
    // moves, and explicit zeroing forms. These exercise GPR writes, scratch
    // side effects, and VEX upper-zeroing outside the EVEX generator.
    for &(label, asm, feat) in &[
        ("vmovmskps_vex_xmm", "{vex} vmovmskps %xmm3, %r8d", Avx),
        ("vmovmskps_vex_ymm", "{vex} vmovmskps %ymm3, %r8d", Avx),
        ("vmovmskpd_vex_xmm", "{vex} vmovmskpd %xmm3, %r8d", Avx),
        ("vmovmskpd_vex_ymm", "{vex} vmovmskpd %ymm3, %r8d", Avx),
        ("vpmovmskb_vex_xmm", "{vex} vpmovmskb %xmm3, %r8d", Avx2),
        ("vpmovmskb_vex_ymm", "{vex} vpmovmskb %ymm3, %r8d", Avx2),
        ("vzeroupper_vex", "vzeroupper", Avx),
        ("vzeroall_vex", "vzeroall", Avx),
        ("vmovntdqa_vex_xmm", "{vex} vmovntdqa (%rax), %xmm1", Avx2),
        ("vmovntdqa_vex_ymm", "{vex} vmovntdqa 32(%rax), %ymm1", Avx2),
        ("vmovntps_vex_xmm", "{vex} vmovntps %xmm3, (%rax)", Avx),
        ("vmovntps_vex_ymm", "{vex} vmovntps %ymm3, 32(%rax)", Avx),
        ("vmovntpd_vex_xmm", "{vex} vmovntpd %xmm3, 64(%rax)", Avx),
        ("vmovntpd_vex_ymm", "{vex} vmovntpd %ymm3, 96(%rax)", Avx),
        ("vmovntdq_vex_xmm", "{vex} vmovntdq %xmm3, 128(%rax)", Avx2),
        ("vmovntdq_vex_ymm", "{vex} vmovntdq %ymm3, 160(%rax)", Avx2),
        (
            "vmaskmovps_vex_xmm_load",
            "{vex} vmaskmovps (%rax), %xmm2, %xmm1",
            Avx,
        ),
        (
            "vmaskmovps_vex_xmm_store",
            "{vex} vmaskmovps %xmm3, %xmm2, 32(%rax)",
            Avx,
        ),
        (
            "vmaskmovps_vex_ymm_load",
            "{vex} vmaskmovps 64(%rax), %ymm2, %ymm1",
            Avx,
        ),
        (
            "vmaskmovps_vex_ymm_store",
            "{vex} vmaskmovps %ymm3, %ymm2, 96(%rax)",
            Avx,
        ),
        (
            "vmaskmovpd_vex_xmm_load",
            "{vex} vmaskmovpd (%rax), %xmm2, %xmm1",
            Avx,
        ),
        (
            "vmaskmovpd_vex_xmm_store",
            "{vex} vmaskmovpd %xmm3, %xmm2, 32(%rax)",
            Avx,
        ),
        (
            "vmaskmovpd_vex_ymm_load",
            "{vex} vmaskmovpd 64(%rax), %ymm2, %ymm1",
            Avx,
        ),
        (
            "vmaskmovpd_vex_ymm_store",
            "{vex} vmaskmovpd %ymm3, %ymm2, 96(%rax)",
            Avx,
        ),
        (
            "vpmaskmovd_vex_xmm_load",
            "{vex} vpmaskmovd (%rax), %xmm2, %xmm1",
            Avx2,
        ),
        (
            "vpmaskmovd_vex_xmm_store",
            "{vex} vpmaskmovd %xmm3, %xmm2, 32(%rax)",
            Avx2,
        ),
        (
            "vpmaskmovd_vex_ymm_load",
            "{vex} vpmaskmovd 64(%rax), %ymm2, %ymm1",
            Avx2,
        ),
        (
            "vpmaskmovd_vex_ymm_store",
            "{vex} vpmaskmovd %ymm3, %ymm2, 96(%rax)",
            Avx2,
        ),
        (
            "vpmaskmovq_vex_xmm_load",
            "{vex} vpmaskmovq (%rax), %xmm2, %xmm1",
            Avx2,
        ),
        (
            "vpmaskmovq_vex_xmm_store",
            "{vex} vpmaskmovq %xmm3, %xmm2, 32(%rax)",
            Avx2,
        ),
        (
            "vpmaskmovq_vex_ymm_load",
            "{vex} vpmaskmovq 64(%rax), %ymm2, %ymm1",
            Avx2,
        ),
        (
            "vpmaskmovq_vex_ymm_store",
            "{vex} vpmaskmovq %ymm3, %ymm2, 96(%rax)",
            Avx2,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }

    // AVX VEX floating-point and move/conversion coverage. These cases verify
    // three-operand FP arithmetic, scalar merge semantics, VEX upper-zeroing,
    // scratch side effects, FP compares/test flag writes, lane insertion and
    // extraction, broadcasts, and scalar/vector FP-int conversions.
    for &(label, asm, profile) in &[
        ("vmovaps_avx_xmm_reg", "{vex} vmovaps %xmm2, %xmm1", F32),
        ("vmovaps_avx_ymm_load", "{vex} vmovaps 32(%rax), %ymm1", F32),
        (
            "vmovaps_avx_ymm_store",
            "{vex} vmovaps %ymm1, 64(%rax)",
            F32,
        ),
        ("vmovups_avx_xmm_load", "{vex} vmovups 17(%rax), %xmm1", F32),
        (
            "vmovups_avx_ymm_store",
            "{vex} vmovups %ymm1, 96(%rax)",
            F32,
        ),
        ("vmovapd_avx_xmm_reg", "{vex} vmovapd %xmm2, %xmm1", F64),
        ("vmovapd_avx_ymm_load", "{vex} vmovapd 32(%rax), %ymm1", F64),
        (
            "vmovapd_avx_ymm_store",
            "{vex} vmovapd %ymm1, 128(%rax)",
            F64,
        ),
        ("vmovupd_avx_xmm_load", "{vex} vmovupd 17(%rax), %xmm1", F64),
        (
            "vmovupd_avx_ymm_store",
            "{vex} vmovupd %ymm1, 160(%rax)",
            F64,
        ),
        ("vmovss_avx_reg", "{vex} vmovss %xmm2, %xmm3, %xmm1", F32),
        ("vmovss_avx_load", "{vex} vmovss 4(%rax), %xmm1", F32),
        ("vmovss_avx_store", "{vex} vmovss %xmm1, 4(%rax)", F32),
        ("vmovsd_avx_reg", "{vex} vmovsd %xmm2, %xmm3, %xmm1", F64),
        ("vmovsd_avx_load", "{vex} vmovsd 8(%rax), %xmm1", F64),
        ("vmovsd_avx_store", "{vex} vmovsd %xmm1, 8(%rax)", F64),
        (
            "vaddps_avx_ymm_reg",
            "{vex} vaddps %ymm2, %ymm3, %ymm1",
            F32,
        ),
        (
            "vaddps_avx_xmm_mem",
            "{vex} vaddps 32(%rax), %xmm3, %xmm1",
            F32,
        ),
        (
            "vaddpd_avx_xmm_reg",
            "{vex} vaddpd %xmm2, %xmm3, %xmm1",
            F64,
        ),
        (
            "vaddpd_avx_ymm_mem",
            "{vex} vaddpd 32(%rax), %ymm3, %ymm1",
            F64,
        ),
        ("vaddss_avx_reg", "{vex} vaddss %xmm2, %xmm3, %xmm1", F32),
        ("vaddss_avx_mem", "{vex} vaddss 32(%rax), %xmm3, %xmm1", F32),
        ("vaddsd_avx_reg", "{vex} vaddsd %xmm2, %xmm3, %xmm1", F64),
        ("vaddsd_avx_mem", "{vex} vaddsd 32(%rax), %xmm3, %xmm1", F64),
        (
            "vsubps_avx_xmm_reg",
            "{vex} vsubps %xmm2, %xmm3, %xmm1",
            F32,
        ),
        (
            "vsubpd_avx_ymm_mem",
            "{vex} vsubpd 32(%rax), %ymm3, %ymm1",
            F64,
        ),
        (
            "vmulps_avx_ymm_reg",
            "{vex} vmulps %ymm2, %ymm3, %ymm1",
            F32,
        ),
        (
            "vmulpd_avx_xmm_mem",
            "{vex} vmulpd 32(%rax), %xmm3, %xmm1",
            F64,
        ),
        (
            "vdivps_avx_xmm_reg",
            "{vex} vdivps %xmm2, %xmm3, %xmm1",
            F32,
        ),
        (
            "vdivpd_avx_ymm_mem",
            "{vex} vdivpd 32(%rax), %ymm3, %ymm1",
            F64,
        ),
        (
            "vmaxps_avx_ymm_reg",
            "{vex} vmaxps %ymm2, %ymm3, %ymm1",
            F32,
        ),
        (
            "vminpd_avx_xmm_mem",
            "{vex} vminpd 32(%rax), %xmm3, %xmm1",
            F64,
        ),
        ("vmaxss_avx_reg", "{vex} vmaxss %xmm2, %xmm3, %xmm1", F32),
        ("vminsd_avx_mem", "{vex} vminsd 32(%rax), %xmm3, %xmm1", F64),
        ("vsqrtps_avx_ymm_reg", "{vex} vsqrtps %ymm3, %ymm1", F32),
        ("vsqrtpd_avx_xmm_mem", "{vex} vsqrtpd 32(%rax), %xmm1", F64),
        ("vsqrtss_avx_reg", "{vex} vsqrtss %xmm2, %xmm3, %xmm1", F32),
        (
            "vsqrtsd_avx_mem",
            "{vex} vsqrtsd 32(%rax), %xmm3, %xmm1",
            F64,
        ),
        (
            "vandps_avx_xmm_reg",
            "{vex} vandps %xmm2, %xmm3, %xmm1",
            Int,
        ),
        (
            "vandnps_avx_ymm_mem",
            "{vex} vandnps 32(%rax), %ymm3, %ymm1",
            Int,
        ),
        ("vorps_avx_ymm_reg", "{vex} vorps %ymm2, %ymm3, %ymm1", Int),
        (
            "vxorps_avx_xmm_mem",
            "{vex} vxorps 32(%rax), %xmm3, %xmm1",
            Int,
        ),
        (
            "vandpd_avx_xmm_reg",
            "{vex} vandpd %xmm2, %xmm3, %xmm1",
            Int,
        ),
        (
            "vandnpd_avx_ymm_mem",
            "{vex} vandnpd 32(%rax), %ymm3, %ymm1",
            Int,
        ),
        ("vorpd_avx_ymm_reg", "{vex} vorpd %ymm2, %ymm3, %ymm1", Int),
        (
            "vxorpd_avx_xmm_mem",
            "{vex} vxorpd 32(%rax), %xmm3, %xmm1",
            Int,
        ),
        (
            "vcmpeqps_avx_xmm_reg",
            "{vex} vcmpeqps %xmm2, %xmm3, %xmm1",
            F32,
        ),
        (
            "vcmpltps_avx_ymm_mem",
            "{vex} vcmpltps 32(%rax), %ymm3, %ymm1",
            F32,
        ),
        (
            "vcmpunordps_avx_ymm_reg",
            "{vex} vcmpunordps %ymm2, %ymm3, %ymm1",
            F32,
        ),
        (
            "vcmpeqpd_avx_xmm_reg",
            "{vex} vcmpeqpd %xmm2, %xmm3, %xmm1",
            F64,
        ),
        (
            "vcmpnltpd_avx_ymm_mem",
            "{vex} vcmpnltpd 32(%rax), %ymm3, %ymm1",
            F64,
        ),
        (
            "vcmpeqss_avx_reg",
            "{vex} vcmpeqss %xmm2, %xmm3, %xmm1",
            F32,
        ),
        (
            "vcmpunordss_avx_mem",
            "{vex} vcmpunordss 32(%rax), %xmm3, %xmm1",
            F32,
        ),
        (
            "vcmpeqsd_avx_reg",
            "{vex} vcmpeqsd %xmm2, %xmm3, %xmm1",
            F64,
        ),
        (
            "vcmpnltsd_avx_mem",
            "{vex} vcmpnltsd 32(%rax), %xmm3, %xmm1",
            F64,
        ),
        ("vucomiss_avx_reg", "{vex} vucomiss %xmm2, %xmm1", F32),
        ("vcomiss_avx_mem", "{vex} vcomiss 32(%rax), %xmm1", F32),
        ("vucomisd_avx_reg", "{vex} vucomisd %xmm2, %xmm1", F64),
        ("vcomisd_avx_mem", "{vex} vcomisd 32(%rax), %xmm1", F64),
        ("vtestps_avx_xmm_reg", "{vex} vtestps %xmm2, %xmm1", Int),
        ("vtestps_avx_ymm_mem", "{vex} vtestps 32(%rax), %ymm1", Int),
        ("vtestpd_avx_ymm_reg", "{vex} vtestpd %ymm2, %ymm1", Int),
        ("vtestpd_avx_xmm_mem", "{vex} vtestpd 32(%rax), %xmm1", Int),
        (
            "vshufps_avx_xmm_reg",
            "{vex} vshufps $0x1b, %xmm2, %xmm3, %xmm1",
            F32,
        ),
        (
            "vshufps_avx_ymm_mem",
            "{vex} vshufps $0xb1, 32(%rax), %ymm3, %ymm1",
            F32,
        ),
        (
            "vshufpd_avx_xmm_reg",
            "{vex} vshufpd $0x1, %xmm2, %xmm3, %xmm1",
            F64,
        ),
        (
            "vshufpd_avx_ymm_mem",
            "{vex} vshufpd $0x2, 32(%rax), %ymm3, %ymm1",
            F64,
        ),
        (
            "vunpcklps_avx_xmm_reg",
            "{vex} vunpcklps %xmm2, %xmm3, %xmm1",
            F32,
        ),
        (
            "vunpckhps_avx_ymm_mem",
            "{vex} vunpckhps 32(%rax), %ymm3, %ymm1",
            F32,
        ),
        (
            "vunpcklpd_avx_xmm_reg",
            "{vex} vunpcklpd %xmm2, %xmm3, %xmm1",
            F64,
        ),
        (
            "vunpckhpd_avx_ymm_mem",
            "{vex} vunpckhpd 32(%rax), %ymm3, %ymm1",
            F64,
        ),
        (
            "vblendps_avx_xmm_reg",
            "{vex} vblendps $0x5a, %xmm2, %xmm3, %xmm1",
            F32,
        ),
        (
            "vblendps_avx_ymm_mem",
            "{vex} vblendps $0xa5, 32(%rax), %ymm3, %ymm1",
            F32,
        ),
        (
            "vblendpd_avx_xmm_reg",
            "{vex} vblendpd $0x1, %xmm2, %xmm3, %xmm1",
            F64,
        ),
        (
            "vblendpd_avx_ymm_mem",
            "{vex} vblendpd $0x2, 32(%rax), %ymm3, %ymm1",
            F64,
        ),
        (
            "vblendvps_avx_xmm_reg",
            "{vex} vblendvps %xmm4, %xmm2, %xmm3, %xmm1",
            F32,
        ),
        (
            "vblendvpd_avx_ymm_mem",
            "{vex} vblendvpd %ymm4, 32(%rax), %ymm3, %ymm1",
            F64,
        ),
        (
            "vpermilps_avx_xmm_imm",
            "{vex} vpermilps $0x1b, %xmm3, %xmm1",
            F32,
        ),
        (
            "vpermilps_avx_ymm_mem_imm",
            "{vex} vpermilps $0xb1, 32(%rax), %ymm1",
            F32,
        ),
        (
            "vpermilps_avx_xmm_reg",
            "{vex} vpermilps %xmm2, %xmm3, %xmm1",
            F32,
        ),
        (
            "vpermilpd_avx_ymm_imm",
            "{vex} vpermilpd $0x5, %ymm3, %ymm1",
            F64,
        ),
        (
            "vpermilpd_avx_xmm_reg",
            "{vex} vpermilpd %xmm2, %xmm3, %xmm1",
            F64,
        ),
        (
            "vperm2f128_avx_ymm_reg",
            "{vex} vperm2f128 $0x31, %ymm2, %ymm3, %ymm1",
            F32,
        ),
        (
            "vinsertf128_avx_ymm_reg",
            "{vex} vinsertf128 $0x1, %xmm2, %ymm3, %ymm1",
            F32,
        ),
        (
            "vinsertf128_avx_ymm_mem",
            "{vex} vinsertf128 $0x1, 32(%rax), %ymm3, %ymm1",
            F32,
        ),
        (
            "vextractf128_avx_xmm8",
            "{vex} vextractf128 $0x1, %ymm1, %xmm8",
            F32,
        ),
        (
            "vextractf128_avx_mem",
            "{vex} vextractf128 $0x0, %ymm1, 32(%rax)",
            F32,
        ),
        (
            "vbroadcastss_avx_xmm_mem",
            "{vex} vbroadcastss 32(%rax), %xmm1",
            F32,
        ),
        (
            "vbroadcastss_avx_ymm_mem",
            "{vex} vbroadcastss 32(%rax), %ymm1",
            F32,
        ),
        (
            "vbroadcastsd_avx_ymm_mem",
            "{vex} vbroadcastsd 32(%rax), %ymm1",
            F64,
        ),
        (
            "vbroadcastf128_avx_ymm_mem",
            "{vex} vbroadcastf128 32(%rax), %ymm1",
            F32,
        ),
        ("vcvtps2pd_avx_reg", "{vex} vcvtps2pd %xmm3, %ymm1", F32),
        ("vcvtps2pd_avx_mem", "{vex} vcvtps2pd 32(%rax), %ymm1", F32),
        ("vcvtpd2ps_avx_reg", "{vex} vcvtpd2ps %ymm3, %xmm1", F64),
        ("vcvtps2dq_avx_reg", "{vex} vcvtps2dq %ymm3, %ymm1", F32),
        (
            "vcvttps2dq_avx_mem",
            "{vex} vcvttps2dq 32(%rax), %xmm1",
            F32,
        ),
        ("vcvtdq2ps_avx_reg", "{vex} vcvtdq2ps %xmm3, %xmm1", Int),
        ("vcvtpd2dq_avx_reg", "{vex} vcvtpd2dq %ymm3, %xmm1", F64),
        ("vcvtdq2pd_avx_reg", "{vex} vcvtdq2pd %xmm3, %ymm1", Int),
        (
            "vcvtsi2ss_avx_r64",
            "{vex} vcvtsi2ss %r8, %xmm3, %xmm1",
            F32,
        ),
        (
            "vcvtsi2sd_avx_m32",
            "{vex} vcvtsi2sd 32(%rax), %xmm3, %xmm1",
            F64,
        ),
        ("vcvtss2si_avx_xmm_r8", "{vex} vcvtss2si %xmm1, %r8", F32),
        (
            "vcvttss2si_avx_m32_r8",
            "{vex} vcvttss2si 32(%rax), %r8",
            F32,
        ),
        ("vcvtsd2si_avx_xmm_r8", "{vex} vcvtsd2si %xmm1, %r8", F64),
        (
            "vcvttsd2si_avx_m64_r8",
            "{vex} vcvttsd2si 32(%rax), %r8",
            F64,
        ),
        ("vmovsldup_avx_xmm_reg", "{vex} vmovsldup %xmm3, %xmm1", F32),
        (
            "vmovsldup_avx_ymm_mem",
            "{vex} vmovsldup 32(%rax), %ymm1",
            F32,
        ),
        ("vmovshdup_avx_ymm_reg", "{vex} vmovshdup %ymm3, %ymm1", F32),
        (
            "vmovddup_avx_xmm_mem",
            "{vex} vmovddup 32(%rax), %xmm1",
            F64,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Avx,
            profile,
        });
    }

    // AVX2 VEX packed-integer coverage. This spans the destructive-to-3-source
    // transition for arithmetic/logical/compare/minmax/multiply, pack/unpack,
    // immediate/count/variable shifts, SSSE3-style byte/word transforms,
    // 128-bit lane permutes, element permutes, and broadcasts.
    for &(label, asm) in &[
        ("vpaddb_avx2_xmm_reg", "{vex} vpaddb %xmm2, %xmm3, %xmm1"),
        ("vpaddw_avx2_ymm_mem", "{vex} vpaddw 32(%rax), %ymm3, %ymm1"),
        ("vpaddd_avx2_ymm_reg", "{vex} vpaddd %ymm2, %ymm3, %ymm1"),
        ("vpaddq_avx2_xmm_mem", "{vex} vpaddq 32(%rax), %xmm3, %xmm1"),
        ("vpaddsb_avx2_xmm_reg", "{vex} vpaddsb %xmm2, %xmm3, %xmm1"),
        (
            "vpaddsw_avx2_ymm_mem",
            "{vex} vpaddsw 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vpaddusb_avx2_ymm_reg",
            "{vex} vpaddusb %ymm2, %ymm3, %ymm1",
        ),
        (
            "vpaddusw_avx2_xmm_mem",
            "{vex} vpaddusw 32(%rax), %xmm3, %xmm1",
        ),
        ("vpsubb_avx2_xmm_reg", "{vex} vpsubb %xmm2, %xmm3, %xmm1"),
        ("vpsubw_avx2_ymm_mem", "{vex} vpsubw 32(%rax), %ymm3, %ymm1"),
        ("vpsubd_avx2_ymm_reg", "{vex} vpsubd %ymm2, %ymm3, %ymm1"),
        ("vpsubq_avx2_xmm_mem", "{vex} vpsubq 32(%rax), %xmm3, %xmm1"),
        ("vpsubsb_avx2_xmm_reg", "{vex} vpsubsb %xmm2, %xmm3, %xmm1"),
        (
            "vpsubsw_avx2_ymm_mem",
            "{vex} vpsubsw 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vpsubusb_avx2_ymm_reg",
            "{vex} vpsubusb %ymm2, %ymm3, %ymm1",
        ),
        (
            "vpsubusw_avx2_xmm_mem",
            "{vex} vpsubusw 32(%rax), %xmm3, %xmm1",
        ),
        ("vpand_avx2_ymm_reg", "{vex} vpand %ymm2, %ymm3, %ymm1"),
        ("vpandn_avx2_ymm_mem", "{vex} vpandn 32(%rax), %ymm3, %ymm1"),
        ("vpor_avx2_xmm_reg", "{vex} vpor %xmm2, %xmm3, %xmm1"),
        ("vpxor_avx2_xmm_mem", "{vex} vpxor 32(%rax), %xmm3, %xmm1"),
        (
            "vpcmpeqb_avx2_xmm_reg",
            "{vex} vpcmpeqb %xmm2, %xmm3, %xmm1",
        ),
        (
            "vpcmpeqw_avx2_ymm_mem",
            "{vex} vpcmpeqw 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vpcmpeqd_avx2_ymm_reg",
            "{vex} vpcmpeqd %ymm2, %ymm3, %ymm1",
        ),
        (
            "vpcmpeqq_avx2_xmm_mem",
            "{vex} vpcmpeqq 32(%rax), %xmm3, %xmm1",
        ),
        (
            "vpcmpgtb_avx2_xmm_reg",
            "{vex} vpcmpgtb %xmm2, %xmm3, %xmm1",
        ),
        (
            "vpcmpgtw_avx2_ymm_mem",
            "{vex} vpcmpgtw 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vpcmpgtd_avx2_ymm_reg",
            "{vex} vpcmpgtd %ymm2, %ymm3, %ymm1",
        ),
        (
            "vpcmpgtq_avx2_xmm_mem",
            "{vex} vpcmpgtq 32(%rax), %xmm3, %xmm1",
        ),
        ("vpminub_avx2_xmm_reg", "{vex} vpminub %xmm2, %xmm3, %xmm1"),
        (
            "vpminsw_avx2_ymm_mem",
            "{vex} vpminsw 32(%rax), %ymm3, %ymm1",
        ),
        ("vpmaxub_avx2_ymm_reg", "{vex} vpmaxub %ymm2, %ymm3, %ymm1"),
        (
            "vpmaxsw_avx2_xmm_mem",
            "{vex} vpmaxsw 32(%rax), %xmm3, %xmm1",
        ),
        ("vpminsb_avx2_xmm_reg", "{vex} vpminsb %xmm2, %xmm3, %xmm1"),
        (
            "vpminsd_avx2_ymm_mem",
            "{vex} vpminsd 32(%rax), %ymm3, %ymm1",
        ),
        ("vpminuw_avx2_ymm_reg", "{vex} vpminuw %ymm2, %ymm3, %ymm1"),
        (
            "vpminud_avx2_xmm_mem",
            "{vex} vpminud 32(%rax), %xmm3, %xmm1",
        ),
        ("vpmaxsb_avx2_xmm_reg", "{vex} vpmaxsb %xmm2, %xmm3, %xmm1"),
        (
            "vpmaxsd_avx2_ymm_mem",
            "{vex} vpmaxsd 32(%rax), %ymm3, %ymm1",
        ),
        ("vpmaxuw_avx2_ymm_reg", "{vex} vpmaxuw %ymm2, %ymm3, %ymm1"),
        (
            "vpmaxud_avx2_xmm_mem",
            "{vex} vpmaxud 32(%rax), %xmm3, %xmm1",
        ),
        ("vpmullw_avx2_xmm_reg", "{vex} vpmullw %xmm2, %xmm3, %xmm1"),
        (
            "vpmulhw_avx2_ymm_mem",
            "{vex} vpmulhw 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vpmulhuw_avx2_ymm_reg",
            "{vex} vpmulhuw %ymm2, %ymm3, %ymm1",
        ),
        (
            "vpmuludq_avx2_xmm_mem",
            "{vex} vpmuludq 32(%rax), %xmm3, %xmm1",
        ),
        (
            "vpmaddwd_avx2_xmm_reg",
            "{vex} vpmaddwd %xmm2, %xmm3, %xmm1",
        ),
        (
            "vpmaddubsw_avx2_ymm_mem",
            "{vex} vpmaddubsw 32(%rax), %ymm3, %ymm1",
        ),
        ("vpmulld_avx2_ymm_reg", "{vex} vpmulld %ymm2, %ymm3, %ymm1"),
        (
            "vpmuldq_avx2_xmm_mem",
            "{vex} vpmuldq 32(%rax), %xmm3, %xmm1",
        ),
        (
            "vpacksswb_avx2_xmm_reg",
            "{vex} vpacksswb %xmm2, %xmm3, %xmm1",
        ),
        (
            "vpackssdw_avx2_ymm_mem",
            "{vex} vpackssdw 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vpackuswb_avx2_ymm_reg",
            "{vex} vpackuswb %ymm2, %ymm3, %ymm1",
        ),
        (
            "vpackusdw_avx2_xmm_mem",
            "{vex} vpackusdw 32(%rax), %xmm3, %xmm1",
        ),
        (
            "vpunpcklbw_avx2_xmm_reg",
            "{vex} vpunpcklbw %xmm2, %xmm3, %xmm1",
        ),
        (
            "vpunpcklwd_avx2_ymm_mem",
            "{vex} vpunpcklwd 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vpunpckldq_avx2_ymm_reg",
            "{vex} vpunpckldq %ymm2, %ymm3, %ymm1",
        ),
        (
            "vpunpcklqdq_avx2_xmm_mem",
            "{vex} vpunpcklqdq 32(%rax), %xmm3, %xmm1",
        ),
        (
            "vpunpckhbw_avx2_xmm_reg",
            "{vex} vpunpckhbw %xmm2, %xmm3, %xmm1",
        ),
        (
            "vpunpckhwd_avx2_ymm_mem",
            "{vex} vpunpckhwd 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vpunpckhdq_avx2_ymm_reg",
            "{vex} vpunpckhdq %ymm2, %ymm3, %ymm1",
        ),
        (
            "vpunpckhqdq_avx2_xmm_mem",
            "{vex} vpunpckhqdq 32(%rax), %xmm3, %xmm1",
        ),
        ("vpsllw_avx2_xmm_imm", "{vex} vpsllw $3, %xmm3, %xmm1"),
        ("vpslld_avx2_ymm_imm", "{vex} vpslld $7, %ymm3, %ymm1"),
        ("vpsllq_avx2_xmm_imm", "{vex} vpsllq $11, %xmm3, %xmm1"),
        ("vpsrlw_avx2_ymm_imm", "{vex} vpsrlw $5, %ymm3, %ymm1"),
        ("vpsrld_avx2_xmm_imm", "{vex} vpsrld $9, %xmm3, %xmm1"),
        ("vpsrlq_avx2_ymm_imm", "{vex} vpsrlq $13, %ymm3, %ymm1"),
        ("vpsraw_avx2_xmm_imm", "{vex} vpsraw $2, %xmm3, %xmm1"),
        ("vpsrad_avx2_ymm_imm", "{vex} vpsrad $6, %ymm3, %ymm1"),
        (
            "vpslld_avx2_xmm_count",
            "{vex} vpslld 32(%rax), %xmm3, %xmm1",
        ),
        (
            "vpsrlq_avx2_ymm_count",
            "{vex} vpsrlq 32(%rax), %ymm3, %ymm1",
        ),
        ("vpsllvd_avx2_xmm_reg", "{vex} vpsllvd %xmm2, %xmm3, %xmm1"),
        (
            "vpsllvq_avx2_ymm_mem",
            "{vex} vpsllvq 32(%rax), %ymm3, %ymm1",
        ),
        ("vpsrlvd_avx2_ymm_reg", "{vex} vpsrlvd %ymm2, %ymm3, %ymm1"),
        (
            "vpsrlvq_avx2_xmm_mem",
            "{vex} vpsrlvq 32(%rax), %xmm3, %xmm1",
        ),
        ("vpsravd_avx2_ymm_reg", "{vex} vpsravd %ymm2, %ymm3, %ymm1"),
        ("vpshufb_avx2_xmm_reg", "{vex} vpshufb %xmm2, %xmm3, %xmm1"),
        (
            "vpsignb_avx2_ymm_mem",
            "{vex} vpsignb 32(%rax), %ymm3, %ymm1",
        ),
        ("vpsignw_avx2_ymm_reg", "{vex} vpsignw %ymm2, %ymm3, %ymm1"),
        (
            "vpsignd_avx2_xmm_mem",
            "{vex} vpsignd 32(%rax), %xmm3, %xmm1",
        ),
        ("vpabsb_avx2_xmm_reg", "{vex} vpabsb %xmm3, %xmm1"),
        ("vpabsw_avx2_ymm_mem", "{vex} vpabsw 32(%rax), %ymm1"),
        ("vpabsd_avx2_ymm_reg", "{vex} vpabsd %ymm3, %ymm1"),
        (
            "vpalignr_avx2_xmm_reg",
            "{vex} vpalignr $5, %xmm2, %xmm3, %xmm1",
        ),
        (
            "vpalignr_avx2_ymm_mem",
            "{vex} vpalignr $17, 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vperm2i128_avx2_ymm_reg",
            "{vex} vperm2i128 $0x31, %ymm2, %ymm3, %ymm1",
        ),
        ("vpermq_avx2_ymm_imm", "{vex} vpermq $0x1b, %ymm3, %ymm1"),
        ("vpermd_avx2_ymm_reg", "{vex} vpermd %ymm2, %ymm3, %ymm1"),
        ("vpermd_avx2_ymm_mem", "{vex} vpermd 32(%rax), %ymm3, %ymm1"),
        (
            "vpbroadcastb_avx2_xmm_mem",
            "{vex} vpbroadcastb 32(%rax), %xmm1",
        ),
        (
            "vpbroadcastw_avx2_ymm_mem",
            "{vex} vpbroadcastw 32(%rax), %ymm1",
        ),
        (
            "vpbroadcastd_avx2_ymm_reg",
            "{vex} vpbroadcastd %xmm2, %ymm1",
        ),
        (
            "vpbroadcastq_avx2_ymm_mem",
            "{vex} vpbroadcastq 32(%rax), %ymm1",
        ),
        (
            "vbroadcasti128_avx2_ymm_mem",
            "{vex} vbroadcasti128 32(%rax), %ymm1",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Avx2,
            profile: Int,
        });
    }

    // VEX-encoded FMA and AVX floating-point misc forms cover the packed,
    // scalar, horizontal, rounding, and dot-product paths outside EVEX.
    for &(label, asm, feat, profile) in &[
        (
            "vfmadd132ps_vex_xmm_reg",
            "{vex} vfmadd132ps %xmm2, %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfmadd213ps_vex_xmm_mem",
            "{vex} vfmadd213ps 32(%rax), %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfmadd231ps_vex_ymm_reg",
            "{vex} vfmadd231ps %ymm2, %ymm3, %ymm1",
            Fma,
            F32,
        ),
        (
            "vfmadd132pd_vex_ymm_mem",
            "{vex} vfmadd132pd 32(%rax), %ymm3, %ymm1",
            Fma,
            F64,
        ),
        (
            "vfmaddsub132ps_vex_ymm_reg",
            "{vex} vfmaddsub132ps %ymm2, %ymm3, %ymm1",
            Fma,
            F32,
        ),
        (
            "vfmsubadd231pd_vex_ymm_mem",
            "{vex} vfmsubadd231pd 64(%rax), %ymm3, %ymm1",
            Fma,
            F64,
        ),
        (
            "vfmadd132ss_vex_reg",
            "{vex} vfmadd132ss %xmm2, %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfmadd213sd_vex_mem",
            "{vex} vfmadd213sd 32(%rax), %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfmsub231ss_vex_reg",
            "{vex} vfmsub231ss %xmm2, %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfnmadd231sd_vex_mem",
            "{vex} vfnmadd231sd 32(%rax), %xmm3, %xmm1",
            Fma,
            F64,
        ),
        // Broaden FMA3 VEX coverage across operand permutations, add/sub and
        // negated variants, packed/scalar widths, and memory operands.
        (
            "vfmsub132ps_vex_xmm_reg",
            "{vex} vfmsub132ps %xmm2, %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfmsub213ps_vex_ymm_mem",
            "{vex} vfmsub213ps 32(%rax), %ymm3, %ymm1",
            Fma,
            F32,
        ),
        (
            "vfmsub231ps_vex_xmm_reg",
            "{vex} vfmsub231ps %xmm2, %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfnmadd132ps_vex_ymm_reg",
            "{vex} vfnmadd132ps %ymm2, %ymm3, %ymm1",
            Fma,
            F32,
        ),
        (
            "vfnmadd213ps_vex_xmm_mem",
            "{vex} vfnmadd213ps 32(%rax), %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfnmadd231ps_vex_ymm_reg",
            "{vex} vfnmadd231ps %ymm2, %ymm3, %ymm1",
            Fma,
            F32,
        ),
        (
            "vfnmsub132ps_vex_xmm_mem",
            "{vex} vfnmsub132ps 32(%rax), %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfnmsub213ps_vex_ymm_reg",
            "{vex} vfnmsub213ps %ymm2, %ymm3, %ymm1",
            Fma,
            F32,
        ),
        (
            "vfnmsub231ps_vex_xmm_reg",
            "{vex} vfnmsub231ps %xmm2, %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfmaddsub213ps_vex_xmm_mem",
            "{vex} vfmaddsub213ps 32(%rax), %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfmaddsub231ps_vex_ymm_reg",
            "{vex} vfmaddsub231ps %ymm2, %ymm3, %ymm1",
            Fma,
            F32,
        ),
        (
            "vfmsubadd132ps_vex_xmm_reg",
            "{vex} vfmsubadd132ps %xmm2, %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfmsubadd213ps_vex_ymm_mem",
            "{vex} vfmsubadd213ps 32(%rax), %ymm3, %ymm1",
            Fma,
            F32,
        ),
        (
            "vfmadd213pd_vex_xmm_reg",
            "{vex} vfmadd213pd %xmm2, %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfmadd231pd_vex_ymm_mem",
            "{vex} vfmadd231pd 64(%rax), %ymm3, %ymm1",
            Fma,
            F64,
        ),
        (
            "vfmsub132pd_vex_xmm_reg",
            "{vex} vfmsub132pd %xmm2, %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfmsub213pd_vex_ymm_mem",
            "{vex} vfmsub213pd 64(%rax), %ymm3, %ymm1",
            Fma,
            F64,
        ),
        (
            "vfmsub231pd_vex_xmm_reg",
            "{vex} vfmsub231pd %xmm2, %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfnmadd132pd_vex_ymm_mem",
            "{vex} vfnmadd132pd 64(%rax), %ymm3, %ymm1",
            Fma,
            F64,
        ),
        (
            "vfnmadd213pd_vex_xmm_reg",
            "{vex} vfnmadd213pd %xmm2, %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfnmadd231pd_vex_ymm_mem",
            "{vex} vfnmadd231pd 64(%rax), %ymm3, %ymm1",
            Fma,
            F64,
        ),
        (
            "vfnmsub132pd_vex_xmm_reg",
            "{vex} vfnmsub132pd %xmm2, %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfnmsub213pd_vex_ymm_mem",
            "{vex} vfnmsub213pd 64(%rax), %ymm3, %ymm1",
            Fma,
            F64,
        ),
        (
            "vfnmsub231pd_vex_xmm_reg",
            "{vex} vfnmsub231pd %xmm2, %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfmaddsub132pd_vex_xmm_reg",
            "{vex} vfmaddsub132pd %xmm2, %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfmaddsub213pd_vex_ymm_mem",
            "{vex} vfmaddsub213pd 64(%rax), %ymm3, %ymm1",
            Fma,
            F64,
        ),
        (
            "vfmaddsub231pd_vex_xmm_reg",
            "{vex} vfmaddsub231pd %xmm2, %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfmsubadd132pd_vex_ymm_reg",
            "{vex} vfmsubadd132pd %ymm2, %ymm3, %ymm1",
            Fma,
            F64,
        ),
        (
            "vfmsubadd213pd_vex_xmm_mem",
            "{vex} vfmsubadd213pd 64(%rax), %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfmadd213ss_vex_mem",
            "{vex} vfmadd213ss 32(%rax), %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfmadd231ss_vex_reg",
            "{vex} vfmadd231ss %xmm2, %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfmsub132ss_vex_mem",
            "{vex} vfmsub132ss 32(%rax), %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfmsub213ss_vex_reg",
            "{vex} vfmsub213ss %xmm2, %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfnmadd132ss_vex_reg",
            "{vex} vfnmadd132ss %xmm2, %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfnmadd213ss_vex_mem",
            "{vex} vfnmadd213ss 32(%rax), %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfnmadd231ss_vex_reg",
            "{vex} vfnmadd231ss %xmm2, %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfnmsub132ss_vex_mem",
            "{vex} vfnmsub132ss 32(%rax), %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfnmsub213ss_vex_reg",
            "{vex} vfnmsub213ss %xmm2, %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfnmsub231ss_vex_mem",
            "{vex} vfnmsub231ss 32(%rax), %xmm3, %xmm1",
            Fma,
            F32,
        ),
        (
            "vfmadd132sd_vex_reg",
            "{vex} vfmadd132sd %xmm2, %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfmadd231sd_vex_reg",
            "{vex} vfmadd231sd %xmm2, %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfmsub132sd_vex_mem",
            "{vex} vfmsub132sd 32(%rax), %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfmsub213sd_vex_reg",
            "{vex} vfmsub213sd %xmm2, %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfmsub231sd_vex_mem",
            "{vex} vfmsub231sd 32(%rax), %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfnmadd132sd_vex_reg",
            "{vex} vfnmadd132sd %xmm2, %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfnmadd213sd_vex_mem",
            "{vex} vfnmadd213sd 32(%rax), %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfnmsub132sd_vex_reg",
            "{vex} vfnmsub132sd %xmm2, %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfnmsub213sd_vex_mem",
            "{vex} vfnmsub213sd 32(%rax), %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vfnmsub231sd_vex_reg",
            "{vex} vfnmsub231sd %xmm2, %xmm3, %xmm1",
            Fma,
            F64,
        ),
        (
            "vaddsubps_vex_ymm_reg",
            "{vex} vaddsubps %ymm2, %ymm3, %ymm1",
            Avx,
            F32,
        ),
        (
            "vaddsubpd_vex_ymm_mem",
            "{vex} vaddsubpd 32(%rax), %ymm3, %ymm1",
            Avx,
            F64,
        ),
        (
            "vhaddps_vex_xmm_reg",
            "{vex} vhaddps %xmm2, %xmm3, %xmm1",
            Avx,
            F32,
        ),
        (
            "vhsubpd_vex_ymm_mem",
            "{vex} vhsubpd 32(%rax), %ymm3, %ymm1",
            Avx,
            F64,
        ),
        (
            "vroundps_vex_ymm_reg",
            "{vex} vroundps $1, %ymm3, %ymm1",
            Avx,
            F32,
        ),
        (
            "vroundpd_vex_ymm_mem",
            "{vex} vroundpd $2, 32(%rax), %ymm1",
            Avx,
            F64,
        ),
        (
            "vroundss_vex_reg",
            "{vex} vroundss $3, %xmm2, %xmm3, %xmm1",
            Avx,
            F32,
        ),
        (
            "vroundsd_vex_mem",
            "{vex} vroundsd $1, 32(%rax), %xmm3, %xmm1",
            Avx,
            F64,
        ),
        (
            "vdpps_vex_xmm_reg",
            "{vex} vdpps $0xf1, %xmm2, %xmm3, %xmm1",
            Avx,
            F32,
        ),
        (
            "vdpps_vex_ymm_mem",
            "{vex} vdpps $0xff, 32(%rax), %ymm3, %ymm1",
            Avx,
            F32,
        ),
        (
            "vdppd_vex_xmm_reg",
            "{vex} vdppd $0x31, %xmm2, %xmm3, %xmm1",
            Avx,
            F64,
        ),
        (
            "vdppd_vex_xmm_mem",
            "{vex} vdppd $0x31, 32(%rax), %xmm3, %xmm1",
            Avx,
            F64,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile,
        });
    }

    // VEX-encoded crypto/media forms are separate from both the legacy XMM
    // decoders and the EVEX AVX-512 forms generated by `base_table()`.
    for &(mnem, imm, feat) in &[
        ("vgf2p8affineqb", 0x63, Gfni),
        ("vgf2p8affineinvqb", 0xa5, Gfni),
    ] {
        for class in ["xmm", "ymm"] {
            out.push(Case {
                label: format!("{mnem}_vex_{class}_reg"),
                asm: format!("{{vex}} {mnem} ${imm:#x}, %{class}2, %{class}3, %{class}1"),
                feat,
                profile: Int,
            });
            out.push(Case {
                label: format!("{mnem}_vex_{class}_mem"),
                asm: format!("{{vex}} {mnem} ${imm:#x}, 32(%rax), %{class}3, %{class}1"),
                feat,
                profile: Int,
            });
        }
        out.push(Case {
            label: format!("{mnem}_vex_high"),
            asm: format!("{{vex}} {mnem} ${imm:#x}, %xmm10, %xmm11, %xmm9"),
            feat,
            profile: Int,
        });
    }

    for class in ["xmm", "ymm"] {
        out.push(Case {
            label: format!("vgf2p8mulb_vex_{class}_reg"),
            asm: format!("{{vex}} vgf2p8mulb %{class}2, %{class}3, %{class}1"),
            feat: Gfni,
            profile: Int,
        });
        out.push(Case {
            label: format!("vgf2p8mulb_vex_{class}_mem"),
            asm: format!("{{vex}} vgf2p8mulb 32(%rax), %{class}3, %{class}1"),
            feat: Gfni,
            profile: Int,
        });
    }
    out.push(Case {
        label: "vgf2p8mulb_vex_high".to_string(),
        asm: "{vex} vgf2p8mulb %xmm10, %xmm11, %xmm9".to_string(),
        feat: Gfni,
        profile: Int,
    });

    for mnem in ["vaesenc", "vaesenclast", "vaesdec", "vaesdeclast"] {
        for class in ["xmm", "ymm"] {
            out.push(Case {
                label: format!("{mnem}_vex_{class}_reg"),
                asm: format!("{{vex}} {mnem} %{class}2, %{class}3, %{class}1"),
                feat: Vaes,
                profile: Int,
            });
            out.push(Case {
                label: format!("{mnem}_vex_{class}_mem"),
                asm: format!("{{vex}} {mnem} 32(%rax), %{class}3, %{class}1"),
                feat: Vaes,
                profile: Int,
            });
        }
        out.push(Case {
            label: format!("{mnem}_vex_high"),
            asm: format!("{{vex}} {mnem} %xmm10, %xmm11, %xmm9"),
            feat: Vaes,
            profile: Int,
        });
    }

    for &(imm, tag) in &[(0x00, "ll"), (0x01, "hl"), (0x10, "lh"), (0x11, "hh")] {
        for class in ["xmm", "ymm"] {
            out.push(Case {
                label: format!("vpclmulqdq_vex_{class}_{tag}_reg"),
                asm: format!("{{vex}} vpclmulqdq ${imm:#x}, %{class}2, %{class}3, %{class}1"),
                feat: Vpclmulqdq,
                profile: Int,
            });
            out.push(Case {
                label: format!("vpclmulqdq_vex_{class}_{tag}_mem"),
                asm: format!("{{vex}} vpclmulqdq ${imm:#x}, 32(%rax), %{class}3, %{class}1"),
                feat: Vpclmulqdq,
                profile: Int,
            });
        }
    }
    out.push(Case {
        label: "vpclmulqdq_vex_xmm_hh_high".to_string(),
        asm: "{vex} vpclmulqdq $0x11, %xmm10, %xmm11, %xmm9".to_string(),
        feat: Vpclmulqdq,
        profile: Int,
    });
    out.push(Case {
        label: "vpclmulqdq_vex_ymm_hh_high".to_string(),
        asm: "{vex} vpclmulqdq $0x11, %ymm10, %ymm11, %ymm9".to_string(),
        feat: Vpclmulqdq,
        profile: Int,
    });

    // F16C VEX half/single-precision conversion forms are distinct from the
    // AVX-512-FP16 EVEX conversion family above.
    for &(label, asm, profile) in &[
        (
            "vcvtph2ps_f16c_xmm_reg",
            "{vex} vcvtph2ps %xmm3, %xmm1",
            F16,
        ),
        (
            "vcvtph2ps_f16c_xmm_mem",
            "{vex} vcvtph2ps (%rax), %xmm1",
            F16,
        ),
        (
            "vcvtph2ps_f16c_ymm_reg",
            "{vex} vcvtph2ps %xmm3, %ymm1",
            F16,
        ),
        (
            "vcvtph2ps_f16c_ymm_mem",
            "{vex} vcvtph2ps 16(%rax), %ymm1",
            F16,
        ),
        (
            "vcvtph2ps_f16c_ymm_high",
            "{vex} vcvtph2ps %xmm10, %ymm9",
            F16,
        ),
        (
            "vcvtps2ph_f16c_xmm_reg",
            "{vex} vcvtps2ph $0, %xmm3, %xmm1",
            F32,
        ),
        (
            "vcvtps2ph_f16c_xmm_mem",
            "{vex} vcvtps2ph $0, %xmm3, 32(%rax)",
            F32,
        ),
        (
            "vcvtps2ph_f16c_ymm_reg",
            "{vex} vcvtps2ph $0, %ymm3, %xmm1",
            F32,
        ),
        (
            "vcvtps2ph_f16c_ymm_mem",
            "{vex} vcvtps2ph $0, %ymm3, 48(%rax)",
            F32,
        ),
        (
            "vcvtps2ph_f16c_ymm_high",
            "{vex} vcvtps2ph $0, %ymm10, %xmm9",
            F32,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: F16c,
            profile,
        });
    }

    // AES-NI legacy XMM crypto/key-schedule instructions. These check legacy
    // XMM write semantics alongside register, memory, and high-XMM operands.
    for mnem in ["aesenc", "aesenclast", "aesdec", "aesdeclast"] {
        out.push(Case {
            label: format!("{mnem}_legacy_reg"),
            asm: format!("{mnem} %xmm2, %xmm1"),
            feat: Aes,
            profile: Int,
        });
        out.push(Case {
            label: format!("{mnem}_legacy_mem"),
            asm: format!("{mnem} (%rax), %xmm1"),
            feat: Aes,
            profile: Int,
        });
        out.push(Case {
            label: format!("{mnem}_legacy_high"),
            asm: format!("{mnem} %xmm10, %xmm9"),
            feat: Aes,
            profile: Int,
        });
    }

    for &(label, asm) in &[
        ("aesimc_legacy_reg", "aesimc %xmm2, %xmm1"),
        ("aesimc_legacy_mem", "aesimc (%rax), %xmm1"),
        ("aesimc_legacy_high", "aesimc %xmm10, %xmm9"),
        (
            "aeskeygenassist_imm1b_legacy_reg",
            "aeskeygenassist $0x1b, %xmm2, %xmm1",
        ),
        (
            "aeskeygenassist_imm63_legacy_mem",
            "aeskeygenassist $0x63, (%rax), %xmm1",
        ),
        (
            "aeskeygenassist_imm36_legacy_high",
            "aeskeygenassist $0x36, %xmm10, %xmm9",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Aes,
            profile: Int,
        });
    }

    // Legacy PCLMULQDQ selector coverage. The low bits of the immediate choose
    // independent qwords from the destination and source operands.
    for &(imm, tag) in &[(0x00, "ll"), (0x01, "hl"), (0x10, "lh"), (0x11, "hh")] {
        out.push(Case {
            label: format!("pclmulqdq_legacy_{tag}_reg"),
            asm: format!("pclmulqdq ${imm:#x}, %xmm2, %xmm1"),
            feat: Pclmulqdq,
            profile: Int,
        });
        out.push(Case {
            label: format!("pclmulqdq_legacy_{tag}_mem"),
            asm: format!("pclmulqdq ${imm:#x}, 16(%rax), %xmm1"),
            feat: Pclmulqdq,
            profile: Int,
        });
    }
    out.push(Case {
        label: "pclmulqdq_legacy_hh_high".to_string(),
        asm: "pclmulqdq $0x11, %xmm10, %xmm9".to_string(),
        feat: Pclmulqdq,
        profile: Int,
    });

    // Legacy GFNI XMM forms share the GFNI feature with the EVEX vector cases,
    // but exercise separate 0F38/0F3A decoders and legacy XMM write semantics.
    for &(label, asm) in &[
        ("gf2p8mulb_legacy_reg", "gf2p8mulb %xmm2, %xmm1"),
        ("gf2p8mulb_legacy_mem", "gf2p8mulb (%rax), %xmm1"),
        ("gf2p8mulb_legacy_high", "gf2p8mulb %xmm10, %xmm9"),
        (
            "gf2p8affineqb_imm63_legacy_reg",
            "gf2p8affineqb $0x63, %xmm2, %xmm1",
        ),
        (
            "gf2p8affineqb_imm1b_legacy_mem",
            "gf2p8affineqb $0x1b, (%rax), %xmm1",
        ),
        (
            "gf2p8affineqb_imma5_legacy_high",
            "gf2p8affineqb $0xa5, %xmm10, %xmm9",
        ),
        (
            "gf2p8affineinvqb_imm63_legacy_reg",
            "gf2p8affineinvqb $0x63, %xmm2, %xmm1",
        ),
        (
            "gf2p8affineinvqb_imm1b_legacy_mem",
            "gf2p8affineinvqb $0x1b, (%rax), %xmm1",
        ),
        (
            "gf2p8affineinvqb_imma5_legacy_high",
            "gf2p8affineinvqb $0xa5, %xmm10, %xmm9",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Gfni,
            profile: Int,
        });
    }

    // SHA-NI legacy XMM crypto/message-schedule instructions. These exercise
    // non-VEX XMM writes, memory operands, high XMM registers, and all
    // SHA1RNDS4 function selector immediates.
    for mnem in [
        "sha1nexte",
        "sha1msg1",
        "sha1msg2",
        "sha256rnds2",
        "sha256msg1",
        "sha256msg2",
    ] {
        out.push(Case {
            label: format!("{mnem}_reg"),
            asm: format!("{mnem} %xmm2, %xmm1"),
            feat: Sha,
            profile: Int,
        });
        out.push(Case {
            label: format!("{mnem}_mem"),
            asm: format!("{mnem} (%rax), %xmm1"),
            feat: Sha,
            profile: Int,
        });
        out.push(Case {
            label: format!("{mnem}_high"),
            asm: format!("{mnem} %xmm10, %xmm9"),
            feat: Sha,
            profile: Int,
        });
    }

    for imm in 0..=3 {
        out.push(Case {
            label: format!("sha1rnds4_imm{imm}_reg"),
            asm: format!("sha1rnds4 ${imm}, %xmm2, %xmm1"),
            feat: Sha,
            profile: Int,
        });
        out.push(Case {
            label: format!("sha1rnds4_imm{imm}_mem"),
            asm: format!("sha1rnds4 ${imm}, (%rax), %xmm1"),
            feat: Sha,
            profile: Int,
        });
        out.push(Case {
            label: format!("sha1rnds4_imm{imm}_high"),
            asm: format!("sha1rnds4 ${imm}, %xmm10, %xmm9"),
            feat: Sha,
            profile: Int,
        });
    }

    // MOVDIR direct stores. The harness compares the scratch page, so these
    // cases exercise the architectural memory side effects against silicon.
    for &(label, asm, feat) in &[
        ("movdiri_m32_r8d_base", "movdiri %r8d, (%rax)", Movdiri),
        ("movdiri_m64_r8_disp", "movdiri %r8, 8(%rax)", Movdiri),
        ("movdiri_m32_eax_disp", "movdiri %eax, 16(%rax)", Movdiri),
        ("movdiri_m64_rax_disp", "movdiri %rax, 32(%rax)", Movdiri),
        (
            "movdir64b_scratch_64_to_0",
            "movdir64b 64(%rax), %rax",
            Movdir64b,
        ),
        (
            "movdir64b_scratch_128_to_0",
            "movdir64b 128(%rax), %rax",
            Movdir64b,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }

    // ADX dual-carry arithmetic. The initial CF and OF are set, and the harness
    // compares r8 plus all arithmetic status flags, so ADCX/ADOX flag isolation
    // is checked against silicon.
    for &(label, asm) in &[
        ("adcx_r64_rax_r8", "adcx %rax, %r8"),
        ("adcx_r32_eax_r8d", "adcx %eax, %r8d"),
        ("adcx_m64_scratch_r8", "adcx (%rax), %r8"),
        ("adox_r64_rax_r8", "adox %rax, %r8"),
        ("adox_r32_eax_r8d", "adox %eax, %r8d"),
        ("adox_m64_scratch8_r8", "adox 8(%rax), %r8"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Adx,
            profile: Int,
        });
    }

    // MOVBE endian-swapping loads/stores across all operand sizes. Loads are
    // observed through r8; stores are observed through the scratch page diff.
    for &(label, asm) in &[
        ("movbe_r16_m16_r8w", "movbe (%rax), %r8w"),
        ("movbe_r32_m32_r8d", "movbe (%rax), %r8d"),
        ("movbe_r64_m64_r8", "movbe (%rax), %r8"),
        ("movbe_m16_r16_r8w", "movbe %r8w, 2(%rax)"),
        ("movbe_m32_r32_r8d", "movbe %r8d, 4(%rax)"),
        ("movbe_m64_r64_r8", "movbe %r8, 8(%rax)"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Movbe,
            profile: Int,
        });
    }

    // Core data movement, effective-address, stack, table-lookup, and direct
    // flag-state instructions. The harness compares RBX/RBP/RSP plus a stack
    // window so implicit stack effects are visible, not only register results.
    for &(label, asm) in &[
        ("mov_core_r64_reg", "movq %rcx, %r8"),
        ("mov_core_r32_reg_zeroext", "movl %ecx, %r8d"),
        ("mov_core_r16_reg", "movw %cx, %r8w"),
        ("mov_core_r8_reg", "movb %cl, %r8b"),
        ("mov_core_high8_reg", "movb %ch, %dl"),
        ("mov_core_r64_mem", "movq 16(%rax), %r8"),
        ("mov_core_r32_mem", "movl 8(%rax), %r8d"),
        ("mov_core_r16_mem", "movw 4(%rax), %r8w"),
        ("mov_core_r8_mem", "movb 2(%rax), %r8b"),
        ("mov_core_m64_r8", "movq %r8, 16(%rax)"),
        ("mov_core_m32_r8d", "movl %r8d, 8(%rax)"),
        ("mov_core_m16_r8w", "movw %r8w, 4(%rax)"),
        ("mov_core_m8_r8b", "movb %r8b, 2(%rax)"),
        ("mov_core_r8_imm", "movb $0x7f, %r8b"),
        ("mov_core_m16_imm", "movw $0x1234, 4(%rax)"),
        ("mov_core_r32_imm_zeroext", "movl $0x89abcdef, %r8d"),
        ("movabs_core_r64_imm", "movabsq $0x0123456789abcdef, %r8"),
        ("lea_core_r64_indexed", "leaq 16(%rax,%rcx,2), %r8"),
        ("lea_core_r32_indexed", "leal 16(%rax,%rcx,2), %r8d"),
        ("lea_core_r64_rbx_base", "leaq (%rbx,%r9,4), %rcx"),
        ("xlat_core_rbx_table", "xlatb"),
        ("addr32_xlat_core_rbx_table", "addr32 xlatb"),
        ("push_core_r64", "pushq %r8"),
        ("push_core_r16", "pushw %r8w"),
        ("push_core_imm8", "pushq $-7"),
        ("push_core_imm32", "pushq $0x1234"),
        ("pushf_core_flags", "pushfq"),
        ("pop_core_r64", "popq %r8"),
        ("pop_core_r16", "popw %r8w"),
        ("pop_core_m64", "popq 8(%rax)"),
        ("clc_core_flags", "clc"),
        ("stc_core_flags_initial_cf_clear", "stc"),
        ("cmc_core_flags", "cmc"),
        ("cld_core_flags_initial_df", "cld"),
        ("std_core_flags", "std"),
        ("lahf_core_flags", "lahf"),
        ("sahf_core_flags", "sahf"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // Port I/O instructions. The KVM and interpreter test runners both satisfy
    // IN/INS exits with zero bytes and ignore OUT/OUTS payloads, so these cases
    // can compare register, pointer, REP-count, and scratch effects directly.
    for &(label, asm) in &[
        ("in_io_al_imm8", "inb $0x80, %al"),
        ("in_io_ax_imm8", "inw $0x81, %ax"),
        ("in_io_eax_imm8", "inl $0x82, %eax"),
        ("in_io_al_dx", "inb %dx, %al"),
        ("in_io_ax_dx", "inw %dx, %ax"),
        ("in_io_eax_dx", "inl %dx, %eax"),
        ("out_io_al_imm8", "outb %al, $0x80"),
        ("out_io_ax_imm8", "outw %ax, $0x81"),
        ("out_io_eax_imm8", "outl %eax, $0x82"),
        ("out_io_al_dx", "outb %al, %dx"),
        ("out_io_ax_dx", "outw %ax, %dx"),
        ("out_io_eax_dx", "outl %eax, %dx"),
        ("insb_io_string", "insb"),
        ("insw_io_string", "insw"),
        ("insl_io_string", "insl"),
        ("rep_insb_io_string", "rep insb"),
        ("addr32_insb_io_string", "addr32 insb"),
        ("outsb_io_string", "outsb"),
        ("outsw_io_string", "outsw"),
        ("outsl_io_string", "outsl"),
        ("rep_outsb_io_string", "rep outsb"),
        ("addr32_outsb_io_string", "addr32 outsb"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Io,
            profile: Int,
        });
    }

    // Fast system-call transition instructions. The ring-3 return legs do not
    // touch memory; they immediately re-enter ring 0 with SYSCALL so the final
    // HLT executes privileged. RIP-dependent results are normalized to booleans
    // before comparison because KVM and interpreter code bases differ.
    for &(label, asm) in &[
        (
            "syscall_fast_entry",
            "movq %rax, %rdi\nmovl $0xc0000080, %ecx\nrdmsr\norl $1, %eax\nwrmsr\nmovabsq $0x0018000800000000, %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nmovl $0xc0000081, %ecx\nwrmsr\nleaq 1f(%rip), %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nmovl $0xc0000082, %ecx\nwrmsr\nxorq %rax, %rax\nxorq %rdx, %rdx\nmovl $0xc0000084, %ecx\nwrmsr\nleaq 2f(%rip), %r8\ncmpq %r9, %r9\nsyscall\n2:\nmovq $0xbad, %rbx\njmp 3f\n1:\ncmpq %r8, %rcx\nsete %cl\nmovzbl %cl, %ecx\nmovq $0x5151, %rbx\nmovq $0x8888, %r8\ncmpq %r8, %r8\n3:",
        ),
        (
            "sysret_fast_roundtrip",
            "movq %rax, %rdi\nmovl $0xc0000080, %ecx\nrdmsr\norl $1, %eax\nwrmsr\nmovabsq $0x0018000800000000, %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nmovl $0xc0000081, %ecx\nwrmsr\nleaq 1f(%rip), %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nmovl $0xc0000082, %ecx\nwrmsr\nxorq %rax, %rax\nxorq %rdx, %rdx\nmovl $0xc0000084, %ecx\nwrmsr\nleaq 2f(%rip), %rcx\nmovq $0x202, %r11\nsysretq\n2:\nmovq $0x2468, %r8\nsyscall\nmovq $0xbad, %rbx\njmp 3f\n1:\ncmpq $0x2468, %r8\nsete %cl\nmovzbl %cl, %ecx\nmovq $0x6262, %rbx\nmovq $0x8888, %r8\ncmpq %r8, %r8\n3:",
        ),
        (
            "sysenter_fast_entry",
            "movq %rax, %rdi\nmovl $0x174, %ecx\nmovl $0x8, %eax\nxorl %edx, %edx\nwrmsr\nmovl $0x175, %ecx\nmovl $0x20000, %eax\nxorl %edx, %edx\nwrmsr\nleaq 1f(%rip), %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nmovl $0x176, %ecx\nwrmsr\nmovq $0x1111, %r8\nsysenter\nmovq $0xbad, %rbx\njmp 2f\n1:\ncmpq $0x1111, %r8\nsete %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nmovq $0x7373, %rbx\nmovq $0x8888, %r8\ncmpq %r8, %r8\n2:",
        ),
        (
            "sysexit_fast_roundtrip",
            "movq %rax, %rdi\nmovl $0xc0000080, %ecx\nrdmsr\norl $1, %eax\nwrmsr\nmovabsq $0x0018000800000000, %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nmovl $0xc0000081, %ecx\nwrmsr\nleaq 1f(%rip), %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nmovl $0xc0000082, %ecx\nwrmsr\nxorq %rax, %rax\nxorq %rdx, %rdx\nmovl $0xc0000084, %ecx\nwrmsr\nmovl $0x174, %ecx\nmovl $0x8, %eax\nxorl %edx, %edx\nwrmsr\nmovabsq $0x20000, %rcx\nleaq 2f(%rip), %rdx\nsysexitq\n2:\nmovq $0x1357, %r8\nsyscall\nmovq $0xbad, %rbx\njmp 3f\n1:\ncmpq $0x1357, %r8\nsete %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nmovq $0x8484, %rbx\nmovq $0x8888, %r8\ncmpq %r8, %r8\n3:",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: FastSyscall,
            profile: Int,
        });
    }

    // CPUID and RDPMC expose host/model-specific values, so these cases reduce
    // the architectural result to stable booleans before comparing KVM and rax.
    for &(label, asm) in &[
        (
            "cpuid_leaf0_zero_ext",
            "movabsq $0xffffffff00000000, %rax\nmovq $-1, %rbx\nmovq $-1, %rcx\nmovq $-1, %rdx\ncpuid\nmovq %rax, %r8\nshrq $32, %r8\nmovq %rbx, %r9\nshrq $32, %r9\norq %r9, %r8\nmovq %rcx, %r9\nshrq $32, %r9\norq %r9, %r8\nmovq %rdx, %r9\nshrq $32, %r9\norq %r9, %r8\ntestq %r8, %r8\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rbx, %rbx\nxorq %rdx, %rdx\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
        ),
        (
            "cpuid_preserves_status_flags",
            "movl $1, %eax\nmovl $0, %ecx\nmovq $0x10, %r8\nsubq $0x21, %r8\ncpuid\nmovq $0, %rax\nmovq $0, %rbx\nmovq $0, %rcx\nmovq $0, %rdx\nmovq $0, %r8\nmovq $0, %r9",
        ),
        (
            "cpuid_eax_upper_ignored",
            "xorl %eax, %eax\nxorl %ecx, %ecx\ncpuid\nmovl %eax, %r8d\nmovl %ebx, %r9d\nmovl %ecx, %esi\nmovl %edx, %edi\nmovabsq $0xffffffff00000000, %rax\nxorl %ecx, %ecx\ncpuid\nxorl %r8d, %eax\nxorl %r9d, %ebx\nxorl %esi, %ecx\nxorl %edi, %edx\norl %ebx, %eax\norl %ecx, %eax\norl %edx, %eax\ntestl %eax, %eax\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rbx, %rbx\nxorq %rdx, %rdx\nxorq %rsi, %rsi\nxorq %rdi, %rdi\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
        ),
        (
            "cpuid_ecx_upper_ignored",
            "movl $7, %eax\nmovl $1, %ecx\ncpuid\nmovl %eax, %r8d\nmovl %ebx, %r9d\nmovl %ecx, %esi\nmovl %edx, %edi\nmovl $7, %eax\nmovabsq $0xffffffff00000001, %rcx\ncpuid\nxorl %r8d, %eax\nxorl %r9d, %ebx\nxorl %esi, %ecx\nxorl %edi, %edx\norl %ebx, %eax\norl %ecx, %eax\norl %edx, %eax\ntestl %eax, %eax\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rbx, %rbx\nxorq %rdx, %rdx\nxorq %rsi, %rsi\nxorq %rdi, %rdi\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
        ),
        (
            "cpuid_leaf1_zero_ext",
            "movabsq $0xffffffff00000001, %rax\nmovq $-1, %rbx\nmovq $-1, %rcx\nmovq $-1, %rdx\ncpuid\nmovq %rax, %r8\nshrq $32, %r8\nmovq %rbx, %r9\nshrq $32, %r9\norq %r9, %r8\nmovq %rcx, %r9\nshrq $32, %r9\norq %r9, %r8\nmovq %rdx, %r9\nshrq $32, %r9\norq %r9, %r8\ntestq %r8, %r8\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rbx, %rbx\nxorq %rdx, %rdx\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Cpuid,
            profile: Int,
        });
    }

    for &(label, asm) in &[
        (
            "rdpmc_counter0_zero_ext",
            "movq $-1, %rax\nxorq %rcx, %rcx\nmovq $-1, %rdx\nrdpmc\nmovq %rax, %r8\nshrq $32, %r8\nmovq %rdx, %r9\nshrq $32, %r9\norq %r9, %r8\ntestq %r8, %r8\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
        ),
        (
            "rdpmc_preserves_status_flags",
            "xorq %rcx, %rcx\nmovq $0x10, %r8\nsubq $0x21, %r8\nrdpmc\nmovq $0, %rax\nmovq $0, %rdx\nmovq $0, %r8\nmovq $0, %r9",
        ),
        (
            "rdpmc_ecx_upper_ignored_zero_ext",
            "movq $-1, %rax\nmovabsq $0xffffffff00000000, %rcx\nmovq $-1, %rdx\nrdpmc\nmovq %rax, %r8\nshrq $32, %r8\nmovq %rdx, %r9\nshrq $32, %r9\norq %r9, %r8\ntestq %r8, %r8\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Rdpmc,
            profile: Int,
        });
    }

    // x87 FPU stack/data-path cases. Each snippet initializes the FPU and
    // stores the observable result to scratch, avoiding hidden x87 state in the
    // final KVM/interpreter comparison.
    for &(label, asm) in &[
        ("x87_fld1_fstp", "fninit\nfld1\nfstpl 32(%rax)"),
        (
            "x87_fldz_fchs_fabs",
            "fninit\nfldz\nfchs\nfstpl 32(%rax)\nfldl 32(%rax)\nfabs\nfstpl 40(%rax)",
        ),
        (
            "x87_fadd_m64",
            "movabsq $0x3ff8000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4002000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfaddl 40(%rax)\nfstpl 48(%rax)",
        ),
        (
            "x87_fsub_m64",
            "movabsq $0x4016000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfsubl 40(%rax)\nfstpl 48(%rax)",
        ),
        (
            "x87_fmul_m64",
            "movabsq $0x4008000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4004000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfmull 40(%rax)\nfstpl 48(%rax)",
        ),
        (
            "x87_fdiv_m64",
            "movabsq $0x401c000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfdivl 40(%rax)\nfstpl 48(%rax)",
        ),
        (
            "x87_fild_fistp_i32",
            "movl $-12345, 32(%rax)\nfninit\nfildl 32(%rax)\nfistpl 40(%rax)",
        ),
        (
            "x87_fild_widths_exact_store",
            "movw $-1234, 32(%rax)\nmovl $56789, 36(%rax)\nmovabsq $-1234567, %r8\nmovq %r8, 40(%rax)\nfninit\nfilds 32(%rax)\nfstpl 48(%rax)\nfildl 36(%rax)\nfstpl 56(%rax)\nfildll 40(%rax)\nfstpl 64(%rax)",
        ),
        (
            "x87_fist_nonpop_preserves_st0",
            "movabsq $0x4012000000000000, %r8\nmovq %r8, 32(%rax)\nfninit\nfldl 32(%rax)\nfists 40(%rax)\nfistl 44(%rax)\nfstpl 48(%rax)",
        ),
        (
            "x87_fistp_widths_round_nearest_even",
            "movabsq $0x400c000000000000, %r8\nmovq %r8, 32(%rax)\nfninit\nfldl 32(%rax)\nfistps 40(%rax)\nfldl 32(%rax)\nfistpl 44(%rax)\nfldl 32(%rax)\nfistpll 48(%rax)",
        ),
        (
            "x87_fisttp_widths_truncate",
            "movabsq $0xc00e000000000000, %r8\nmovq %r8, 32(%rax)\nfninit\nfldl 32(%rax)\nfisttps 40(%rax)\nfldl 32(%rax)\nfisttpl 44(%rax)\nfldl 32(%rax)\nfisttpll 48(%rax)",
        ),
        (
            "x87_fldcw_controls_fistp_rounding",
            "movw $0x077f, 32(%rax)\nmovw $0x0b7f, 34(%rax)\nmovabsq $0x400e000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldcw 32(%rax)\nfldl 40(%rax)\nfistpl 48(%rax)\nfldcw 34(%rax)\nfldl 40(%rax)\nfistpl 52(%rax)",
        ),
        (
            "x87_fisttp_ignores_rounding_control",
            "movw $0x0b7f, 32(%rax)\nmovabsq $0x400e000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldcw 32(%rax)\nfldl 40(%rax)\nfisttpl 48(%rax)\nfldl 40(%rax)\nfistpl 52(%rax)",
        ),
        (
            "x87_fiadd_fimul_fisub_fidiv_m16",
            "movw $5, 32(%rax)\nmovw $3, 34(%rax)\nmovw $2, 36(%rax)\nmovw $4, 38(%rax)\nmovw $3, 40(%rax)\nfninit\nfilds 32(%rax)\nfiadds 34(%rax)\nfimuls 36(%rax)\nfisubs 38(%rax)\nfidivs 40(%rax)\nfistpl 48(%rax)",
        ),
        (
            "x87_fisubr_fidivr_m32",
            "movl $8, 32(%rax)\nmovl $20, 36(%rax)\nmovl $36, 40(%rax)\nfninit\nfildl 32(%rax)\nfisubrl 36(%rax)\nfidivrl 40(%rax)\nfistpl 48(%rax)",
        ),
        (
            "x87_ficom_m16_m32_status",
            "movw $7, 32(%rax)\nmovl $9, 36(%rax)\nfninit\nfilds 32(%rax)\nficoms 32(%rax)\nfnstsw 40(%rax)\nficoml 36(%rax)\nfnstsw 42(%rax)\nfstpl 48(%rax)",
        ),
        (
            "x87_ficomp_pops_after_compare",
            "movw $4, 32(%rax)\nmovl $5, 36(%rax)\nfninit\nfilds 32(%rax)\nfildl 36(%rax)\nficomps 32(%rax)\nfstpl 40(%rax)\nfildl 36(%rax)\nficompl 36(%rax)\nfnstsw 48(%rax)",
        ),
        (
            "x87_fbld_fbstp_positive_negative",
            "movabsq $0x0000000000012345, %r8\nmovq %r8, 32(%rax)\nmovw $0, 40(%rax)\nmovabsq $0x0000000000000456, %r8\nmovq %r8, 64(%rax)\nmovw $0x8000, 72(%rax)\nfninit\nfbld 32(%rax)\nfbstp 80(%rax)\nfbld 64(%rax)\nfbstp 96(%rax)",
        ),
        (
            "x87_fbld_fiadd_fbstp_result",
            "movabsq $0x0000000000000123, %r8\nmovq %r8, 32(%rax)\nmovw $0, 40(%rax)\nmovw $7, 48(%rax)\nfninit\nfbld 32(%rax)\nfiadds 48(%rax)\nfbstp 64(%rax)",
        ),
        (
            "x87_fcmovcc_taken_conditions",
            "movabsq $0x3ff0000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\nxorl %ecx, %ecx\nstc\nfcmovb %st(1), %st(0)\nfstpl 48(%rax)\nfstpl 56(%rax)\nfldl 32(%rax)\nfldl 40(%rax)\nxorl %ecx, %ecx\nfcmove %st(1), %st(0)\nfstpl 64(%rax)\nfstpl 72(%rax)\nfldl 32(%rax)\nfldl 40(%rax)\nxorl %ecx, %ecx\nfcmovbe %st(1), %st(0)\nfstpl 80(%rax)\nfstpl 88(%rax)\nfldl 32(%rax)\nfldl 40(%rax)\nxorl %ecx, %ecx\nfcmovu %st(1), %st(0)\nfstpl 96(%rax)\nfstpl 104(%rax)",
        ),
        (
            "x87_fcmovncc_taken_conditions",
            "movabsq $0x3ff0000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\nmovl $1, %ecx\norb %cl, %cl\nfcmovnb %st(1), %st(0)\nfstpl 48(%rax)\nfstpl 56(%rax)\nfldl 32(%rax)\nfldl 40(%rax)\nmovl $1, %ecx\norb %cl, %cl\nfcmovne %st(1), %st(0)\nfstpl 64(%rax)\nfstpl 72(%rax)\nfldl 32(%rax)\nfldl 40(%rax)\nmovl $1, %ecx\norb %cl, %cl\nfcmovnbe %st(1), %st(0)\nfstpl 80(%rax)\nfstpl 88(%rax)\nfldl 32(%rax)\nfldl 40(%rax)\nmovl $1, %ecx\norb %cl, %cl\nfcmovnu %st(1), %st(0)\nfstpl 96(%rax)\nfstpl 104(%rax)",
        ),
        (
            "x87_fcmovcc_not_taken_conditions",
            "movabsq $0x3ff0000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\nmovl $1, %ecx\norb %cl, %cl\nfcmovb %st(1), %st(0)\nfstpl 48(%rax)\nfstpl 56(%rax)\nfldl 32(%rax)\nfldl 40(%rax)\nmovl $1, %ecx\norb %cl, %cl\nfcmove %st(1), %st(0)\nfstpl 64(%rax)\nfstpl 72(%rax)\nfldl 32(%rax)\nfldl 40(%rax)\nmovl $1, %ecx\norb %cl, %cl\nfcmovbe %st(1), %st(0)\nfstpl 80(%rax)\nfstpl 88(%rax)\nfldl 32(%rax)\nfldl 40(%rax)\nmovl $1, %ecx\norb %cl, %cl\nfcmovu %st(1), %st(0)\nfstpl 96(%rax)\nfstpl 104(%rax)",
        ),
        (
            "x87_fcmovncc_not_taken_conditions",
            "movabsq $0x3ff0000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\nxorl %ecx, %ecx\nstc\nfcmovnb %st(1), %st(0)\nfstpl 48(%rax)\nfstpl 56(%rax)\nfldl 32(%rax)\nfldl 40(%rax)\nxorl %ecx, %ecx\nstc\nfcmovne %st(1), %st(0)\nfstpl 64(%rax)\nfstpl 72(%rax)\nfldl 32(%rax)\nfldl 40(%rax)\nxorl %ecx, %ecx\nstc\nfcmovnbe %st(1), %st(0)\nfstpl 80(%rax)\nfstpl 88(%rax)\nfldl 32(%rax)\nfldl 40(%rax)\nxorl %ecx, %ecx\nstc\nfcmovnu %st(1), %st(0)\nfstpl 96(%rax)\nfstpl 104(%rax)",
        ),
        (
            "x87_fcomi_flag_outcomes",
            "movabsq $0x3ff0000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 40(%rax)\nfldl 32(%rax)\nfcomi %st(1), %st(0)\nsetb 112(%rax)\nsete 113(%rax)\nsetp 114(%rax)\nfstpl 64(%rax)\nfstpl 72(%rax)\nfldl 40(%rax)\nfldl 40(%rax)\nfcomi %st(1), %st(0)\nsetb 115(%rax)\nsete 116(%rax)\nsetp 117(%rax)\nfstpl 80(%rax)\nfstpl 88(%rax)\nfldl 32(%rax)\nfldl 40(%rax)\nfcomi %st(1), %st(0)\nsetb 118(%rax)\nsete 119(%rax)\nsetp 120(%rax)\nfstpl 96(%rax)\nfstpl 104(%rax)",
        ),
        (
            "x87_fucomi_flag_outcome",
            "movabsq $0x3ff0000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 40(%rax)\nfldl 32(%rax)\nfucomi %st(1), %st(0)\nsetb 112(%rax)\nsete 113(%rax)\nsetp 114(%rax)\nfstpl 64(%rax)\nfstpl 72(%rax)",
        ),
        (
            "x87_fcomip_pops_and_sets_flags",
            "movabsq $0x3ff0000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 40(%rax)\nfldl 32(%rax)\nfcomip %st(1), %st(0)\nsetb 112(%rax)\nsete 113(%rax)\nsetp 114(%rax)\nfstpl 48(%rax)",
        ),
        (
            "x87_fucomip_pops_and_sets_flags",
            "movabsq $0x3ff0000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 40(%rax)\nfldl 32(%rax)\nfucomip %st(1), %st(0)\nsetb 112(%rax)\nsete 113(%rax)\nsetp 114(%rax)\nfstpl 48(%rax)",
        ),
        (
            "x87_fcom_memory_status_and_pop",
            "movl $0x3f800000, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 40(%rax)\nfcoms 32(%rax)\nfnstsw 56(%rax)\nfcomps 32(%rax)\nfnstsw 58(%rax)\nfldl 40(%rax)\nfcoml 40(%rax)\nfnstsw 60(%rax)\nfcompl 40(%rax)\nfnstsw 62(%rax)",
        ),
        (
            "x87_fcom_register_status_variants",
            "movabsq $0x3ff0000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 40(%rax)\nfldl 32(%rax)\nfcom %st(1)\nfnstsw 56(%rax)\nfcomp %st(1)\nfnstsw 58(%rax)\nfstpl 64(%rax)",
        ),
        (
            "x87_fucom_fucomp_status_and_pop",
            "movabsq $0x3ff0000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 40(%rax)\nfldl 32(%rax)\nfucom %st(1)\nfnstsw 56(%rax)\nfucomp %st(1)\nfnstsw 58(%rax)\nfstpl 64(%rax)",
        ),
        (
            "x87_fcompp_fucompp_pop_both",
            "movabsq $0x3ff0000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 40(%rax)\nfldl 32(%rax)\nfcompp\nfnstsw 56(%rax)\nfldl 40(%rax)\nfldl 32(%rax)\nfucompp\nfnstsw 58(%rax)",
        ),
        (
            "x87_fld_constants_store_m64",
            "fninit\nfldl2t\nfstpl 32(%rax)\nfldl2e\nfstpl 40(%rax)\nfldpi\nfstpl 48(%rax)\nfldlg2\nfstpl 56(%rax)\nfldln2\nfstpl 64(%rax)\nfldz\nfstpl 72(%rax)\nfld1\nfstpl 80(%rax)",
        ),
        (
            "x87_ftst_fxam_status",
            "fninit\nfldz\nftst\nfnstsw 32(%rax)\nfxam\nfnstsw 34(%rax)\nfstpl 40(%rax)\nfld1\nfxam\nfnstsw 48(%rax)\nfstpl 56(%rax)",
        ),
        (
            "x87_fsqrt_exact_square",
            "movabsq $0x4022000000000000, %r8\nmovq %r8, 32(%rax)\nfninit\nfldl 32(%rax)\nfsqrt\nfstpl 40(%rax)",
        ),
        (
            "x87_frndint_rounding_modes",
            "movw $0x037f, 32(%rax)\nmovw $0x077f, 34(%rax)\nmovw $0x0b7f, 36(%rax)\nmovw $0x0f7f, 38(%rax)\nmovabsq $0x400c000000000000, %r8\nmovq %r8, 40(%rax)\nmovabsq $0xc00c000000000000, %r8\nmovq %r8, 48(%rax)\nfninit\nfldcw 32(%rax)\nfldl 40(%rax)\nfrndint\nfstpl 56(%rax)\nfldcw 34(%rax)\nfldl 40(%rax)\nfrndint\nfstpl 64(%rax)\nfldcw 36(%rax)\nfldl 48(%rax)\nfrndint\nfstpl 72(%rax)\nfldcw 38(%rax)\nfldl 48(%rax)\nfrndint\nfstpl 80(%rax)",
        ),
        (
            "x87_fscale_exact_positive_negative",
            "movabsq $0x4008000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nmovabsq $0x4020000000000000, %r8\nmovq %r8, 48(%rax)\nmovabsq $0xbff0000000000000, %r8\nmovq %r8, 56(%rax)\nfninit\nfldl 40(%rax)\nfldl 32(%rax)\nfscale\nfstpl 64(%rax)\nfstpl 72(%rax)\nfldl 56(%rax)\nfldl 48(%rax)\nfscale\nfstpl 80(%rax)\nfstpl 88(%rax)",
        ),
        (
            "x87_fxtract_fscale_roundtrip",
            "movabsq $0x4028000000000000, %r8\nmovq %r8, 32(%rax)\nfninit\nfldl 32(%rax)\nfxtract\nfscale\nfstpl 40(%rax)\nfstpl 48(%rax)",
        ),
        (
            "x87_fprem_exact_remainder_status",
            "movabsq $0x4008000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x401c000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\nfprem\nfnstsw 56(%rax)\nfstpl 48(%rax)\nfstpl 64(%rax)",
        ),
        (
            "x87_fprem1_nearest_remainder_status",
            "movabsq $0x4010000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x401c000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\nfprem1\nfnstsw 56(%rax)\nfstpl 48(%rax)\nfstpl 64(%rax)",
        ),
        (
            "x87_f2xm1_exact_edges",
            "fninit\nfldz\nf2xm1\nfstpl 32(%rax)\nfld1\nf2xm1\nfstpl 40(%rax)\nfld1\nfchs\nf2xm1\nfstpl 48(%rax)",
        ),
        (
            "x87_fyl2x_exact_power",
            "movabsq $0x4008000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\nfyl2x\nfstpl 48(%rax)",
        ),
        (
            "x87_fyl2xp1_exact_power",
            "movabsq $0x4008000000000000, %r8\nmovq %r8, 32(%rax)\nfninit\nfldl 32(%rax)\nfld1\nfyl2xp1\nfstpl 48(%rax)",
        ),
        (
            "x87_fptan_zero_stack_result",
            "fninit\nfldz\nfptan\nfstpl 32(%rax)\nfstpl 40(%rax)",
        ),
        (
            "x87_fpatan_zero_ratio",
            "fninit\nfldz\nfld1\nfpatan\nfstpl 32(%rax)",
        ),
        (
            "x87_fsin_fcos_zero",
            "fninit\nfldz\nfsin\nfstpl 32(%rax)\nfldz\nfcos\nfstpl 40(%rax)",
        ),
        (
            "x87_fsincos_zero_stack_result",
            "fninit\nfldz\nfsincos\nfstpl 32(%rax)\nfstpl 40(%rax)",
        ),
        (
            "x87_fxch_stack_order",
            "movabsq $0x3ff0000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\nfxch %st(1)\nfstpl 48(%rax)\nfstpl 56(%rax)",
        ),
        ("x87_fwait_after_fninit", "fninit\nfwait"),
        (
            "x87_wait_alias_between_memory_ops",
            "movq %r8, 32(%rax)\nwait\nmovq 32(%rax), %rcx",
        ),
        (
            "x87_fwait_after_fld1_store",
            "fninit\nfld1\nfwait\nfstpl 32(%rax)",
        ),
        (
            "x87_fwait_preserves_cmp_flags",
            "fninit\ncmpq %rcx, %r8\nfwait",
        ),
        (
            "x87_fnop_preserves_stack_value",
            "fninit\nfld1\nfnop\nfstpl 32(%rax)",
        ),
        ("x87_fnstcw_default", "fninit\nfnstcw 32(%rax)"),
        (
            "x87_fldcw_fnstcw_roundtrip",
            "movw $0x027f, 32(%rax)\nfninit\nfldcw 32(%rax)\nfnstcw 34(%rax)",
        ),
        (
            "x87_fnstsw_ax_after_fninit",
            "fninit\nmovq $-1, %rax\nfnstsw %ax",
        ),
        (
            "x87_fnstsw_memory_after_fld1",
            "fninit\nfld1\nfnstsw 32(%rax)\nfstpl 40(%rax)",
        ),
        (
            "x87_fincstp_fdecstp_visible_top",
            "movabsq $0x4008000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x401c000000000000, %r8\nmovq %r8, 40(%rax)\nmovabsq $0x4026000000000000, %r8\nmovq %r8, 48(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\nfldl 48(%rax)\nfincstp\nfstl 56(%rax)\nfdecstp\nfstpl 64(%rax)",
        ),
        ("x87_fnclex_clean_status", "fninit\nfnclex\nfnstsw 32(%rax)"),
        (
            "x87_fclex_wait_clean_status",
            "fninit\nfclex\nfnstsw 32(%rax)",
        ),
        (
            "x87_fnstenv_fldenv_control_roundtrip",
            "movw $0x0f7f, 32(%rax)\nfninit\nfldcw 32(%rax)\nfnstenv 96(%rax)\nfninit\nfldenv 96(%rax)\nfnstcw 34(%rax)\nmovq $0, 96(%rax)\nmovq $0, 104(%rax)\nmovq $0, 112(%rax)\nmovl $0, 120(%rax)",
        ),
        (
            "x87_fstenv_alias_fldenv_control_roundtrip",
            "movw $0x027f, 32(%rax)\nfninit\nfldcw 32(%rax)\nfstenv 96(%rax)\nfninit\nfldenv 96(%rax)\nfnstcw 34(%rax)\nmovq $0, 96(%rax)\nmovq $0, 104(%rax)\nmovq $0, 112(%rax)\nmovl $0, 120(%rax)",
        ),
        (
            "x87_fldenv_manual_control_word",
            "movq $0, 96(%rax)\nmovq $0, 104(%rax)\nmovq $0, 112(%rax)\nmovq $0, 120(%rax)\nmovw $0x0f7f, 96(%rax)\nfninit\nfldenv 96(%rax)\nfnstcw 32(%rax)",
        ),
        (
            "x87_fnsave_frstor_stack_roundtrip",
            "movabsq $0x402a000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x403d000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\nfnsave 128(%rax)\nfld1\nfrstor 128(%rax)\nfstpl 48(%rax)\nfstpl 56(%rax)\nmovq $0, 128(%rax)\nmovq $0, 136(%rax)\nmovq $0, 144(%rax)\nmovq $0, 152(%rax)\nmovq $0, 160(%rax)\nmovq $0, 168(%rax)\nmovq $0, 176(%rax)\nmovq $0, 184(%rax)\nmovq $0, 192(%rax)\nmovq $0, 200(%rax)\nmovq $0, 208(%rax)\nmovq $0, 216(%rax)\nmovq $0, 224(%rax)\nmovq $0, 232(%rax)",
        ),
        (
            "x87_fnsave_reinitializes_fpu",
            "fninit\nfld1\nfnsave 128(%rax)\nfnstcw 32(%rax)\nfnstsw 34(%rax)\nmovq $0, 128(%rax)\nmovq $0, 136(%rax)\nmovq $0, 144(%rax)\nmovq $0, 152(%rax)\nmovq $0, 160(%rax)\nmovq $0, 168(%rax)\nmovq $0, 176(%rax)\nmovq $0, 184(%rax)\nmovq $0, 192(%rax)\nmovq $0, 200(%rax)\nmovq $0, 208(%rax)\nmovq $0, 216(%rax)\nmovq $0, 224(%rax)\nmovq $0, 232(%rax)",
        ),
        (
            "x87_ffree_full_stack_allows_push",
            "fninit\nfld1\nfld1\nfld1\nfld1\nfld1\nfld1\nfld1\nfld1\nffree %st(7)\nfldz\nfstpl 32(%rax)\nfnstsw 40(%rax)",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: X87,
            profile: Int,
        });
    }

    // Core bit-test and bit-scan instructions. These cover register operands,
    // immediate memory operands, and register-indexed memory bit strings using
    // r9 as a small index that stays within the compared scratch page.
    for &(label, asm) in &[
        ("bt_core_r64_reg", "btq %rcx, %r8"),
        ("bts_core_r64_reg", "btsq %rcx, %r8"),
        ("btr_core_r32_reg", "btrl %ecx, %r8d"),
        ("btc_core_r64_reg", "btcq %rcx, %r8"),
        ("bt_core_m64_imm", "btq $9, 8(%rax)"),
        ("bts_core_m64_imm", "btsq $9, 8(%rax)"),
        ("btr_core_m32_imm", "btrl $15, 16(%rax)"),
        ("btc_core_m64_imm", "btcq $20, 24(%rax)"),
        ("bt_core_m64_r9", "btq %r9, (%rax)"),
        ("bts_core_m64_r9", "btsq %r9, (%rax)"),
        ("btr_core_m32_r9d", "btrl %r9d, (%rax)"),
        ("btc_core_m64_r9_disp", "btcq %r9, 16(%rax)"),
        ("bsf_core_r64_reg", "bsfq %rcx, %r8"),
        ("bsf_core_r32_mem", "bsfl 32(%rax), %r8d"),
        ("bsr_core_r64_reg", "bsrq %r8, %rcx"),
        ("bsr_core_r32_mem", "bsrl 36(%rax), %ecx"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // Core conditional moves and byte condition writes. INITIAL_RFLAGS has all
    // six status flags set, so the all-condition tables cover both true and
    // false destinations without needing a multi-instruction setup sequence.
    for mnem in [
        "cmovo", "cmovno", "cmovb", "cmovae", "cmove", "cmovne", "cmovbe", "cmova", "cmovs",
        "cmovns", "cmovp", "cmovnp", "cmovl", "cmovge", "cmovle", "cmovg",
    ] {
        out.push(Case {
            label: format!("{mnem}_core_r64_reg"),
            asm: format!("{mnem} %rcx, %r8"),
            feat: Core,
            profile: Int,
        });
    }

    for &(label, asm) in &[
        ("cmove_core_m64_true", "cmove 8(%rax), %r8"),
        ("cmovne_core_m64_false", "cmovne 8(%rax), %r8"),
        ("cmovb_core_m64_true", "cmovb 16(%rax), %r8"),
        ("cmova_core_m64_false", "cmova 16(%rax), %r8"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    for mnem in [
        "seto", "setno", "setb", "setae", "sete", "setne", "setbe", "seta", "sets", "setns",
        "setp", "setnp", "setl", "setge", "setle", "setg",
    ] {
        out.push(Case {
            label: format!("{mnem}_core_r8b"),
            asm: format!("{mnem} %r8b"),
            feat: Core,
            profile: Int,
        });
    }

    for &(label, asm) in &[
        ("seto_core_m8_true", "seto 8(%rax)"),
        ("setno_core_m8_false", "setno 9(%rax)"),
        ("sete_core_m8_true", "sete 10(%rax)"),
        ("setne_core_m8_false", "setne 11(%rax)"),
        ("setbe_core_m8_true", "setbe 12(%rax)"),
        ("seta_core_m8_false", "seta 13(%rax)"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // Core direct control-flow. Multi-instruction snippets make the taken and
    // fallthrough paths visibly different in r8 while preserving the seeded
    // status flags for comparison.
    out.push(Case {
        label: "jmp_core_rel8_taken".to_string(),
        asm: "jmp 1f\nmovq $0x1111, %r8\n1:\nmovq $0x2222, %r8".to_string(),
        feat: Core,
        profile: Int,
    });
    let branch = |mnem: &str, tag: &str| Case {
        label: format!("{mnem}_core_rel8_{tag}"),
        asm: format!("{mnem} 1f\nmovq $0x1111, %r8\njmp 2f\n1:\nmovq $0x2222, %r8\n2:"),
        feat: Core,
        profile: Int,
    };
    for &(mnem, tag) in &[
        ("jo", "taken"),
        ("jno", "not_taken"),
        ("jb", "taken"),
        ("jae", "not_taken"),
        ("je", "taken"),
        ("jne", "not_taken"),
        ("jbe", "taken"),
        ("ja", "not_taken"),
        ("js", "taken"),
        ("jns", "not_taken"),
        ("jp", "taken"),
        ("jnp", "not_taken"),
        ("jl", "not_taken"),
        ("jge", "taken"),
        ("jle", "taken"),
        ("jg", "not_taken"),
    ] {
        out.push(branch(mnem, tag));
    }
    for &(label, asm) in &[
        (
            "loop_core_rel8_taken",
            "loop 1f\nmovq $0x1111, %r8\njmp 2f\n1:\nmovq $0x2222, %r8\n2:",
        ),
        (
            "jecxz_core_rel8_not_taken",
            "jecxz 1f\nmovq $0x1111, %r8\njmp 2f\n1:\nmovq $0x2222, %r8\n2:",
        ),
        (
            "jrcxz_core_rel8_not_taken",
            "jrcxz 1f\nmovq $0x1111, %r8\njmp 2f\n1:\nmovq $0x2222, %r8\n2:",
        ),
        (
            "call_core_rel32_ret",
            "callq 1f\nmovq $0x3333, %r8\njmp 2f\n1:\nmovq $0x2222, %r8\nretq\n2:\nmovl $0x08f5e2cf, -8(%rsp)\nmovl $0x54412e1b, -4(%rsp)",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // Legacy x87 starter cases using input profiles from the generic corpus.
    // The harness does not snapshot the x87 register file directly, so results
    // are made visible through scratch-memory stores.
    for &(label, asm, profile) in &[
        ("x87_fld1_fstp_m64", "fninit\nfld1\nfstpl 64(%rax)", F64),
        (
            "x87_fadds_fstp_m32",
            "fninit\nflds 32(%rax)\nfadds 36(%rax)\nfstps 64(%rax)",
            F32,
        ),
        (
            "x87_fmull_fstp_m64",
            "fninit\nfldl 32(%rax)\nfmull 40(%rax)\nfstpl 72(%rax)",
            F64,
        ),
        (
            "x87_fildl_fistpl_m32",
            "fninit\nfildl 32(%rax)\nfistpl 80(%rax)",
            Int,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: X87,
            profile,
        });
    }

    // Legacy MMX packed-integer coverage. Every case stores its result to
    // scratch before EMMS, making MMX effects visible without comparing hidden
    // x87/MMX register-file state.
    for &(label, asm) in &[
        (
            "mmx_movq_load_store",
            "movq 32(%rax), %mm0\nmovq %mm0, 64(%rax)\nemms",
        ),
        (
            "mmx_movq_reg_store",
            "movq 32(%rax), %mm0\nmovq %mm0, %mm1\nmovq %mm1, 64(%rax)\nemms",
        ),
        (
            "mmx_paddb_store",
            "movq 32(%rax), %mm0\npaddb 40(%rax), %mm0\nmovq %mm0, 64(%rax)\nemms",
        ),
        (
            "mmx_paddw_store",
            "movq 32(%rax), %mm0\npaddw 40(%rax), %mm0\nmovq %mm0, 72(%rax)\nemms",
        ),
        (
            "mmx_paddd_store",
            "movq 32(%rax), %mm0\npaddd 40(%rax), %mm0\nmovq %mm0, 80(%rax)\nemms",
        ),
        (
            "mmx_psubb_store",
            "movq 32(%rax), %mm0\npsubb 40(%rax), %mm0\nmovq %mm0, 88(%rax)\nemms",
        ),
        (
            "mmx_psubw_store",
            "movq 32(%rax), %mm0\npsubw 40(%rax), %mm0\nmovq %mm0, 96(%rax)\nemms",
        ),
        (
            "mmx_psubd_store",
            "movq 32(%rax), %mm0\npsubd 40(%rax), %mm0\nmovq %mm0, 104(%rax)\nemms",
        ),
        (
            "mmx_paddsb_store",
            "movq 32(%rax), %mm0\npaddsb 40(%rax), %mm0\nmovq %mm0, 112(%rax)\nemms",
        ),
        (
            "mmx_paddusb_store",
            "movq 32(%rax), %mm0\npaddusb 40(%rax), %mm0\nmovq %mm0, 120(%rax)\nemms",
        ),
        (
            "mmx_psubsb_store",
            "movq 32(%rax), %mm0\npsubsb 40(%rax), %mm0\nmovq %mm0, 128(%rax)\nemms",
        ),
        (
            "mmx_psubusb_store",
            "movq 32(%rax), %mm0\npsubusb 40(%rax), %mm0\nmovq %mm0, 136(%rax)\nemms",
        ),
        (
            "mmx_pmullw_store",
            "movq 32(%rax), %mm0\npmullw 40(%rax), %mm0\nmovq %mm0, 144(%rax)\nemms",
        ),
        (
            "mmx_pmulhw_store",
            "movq 32(%rax), %mm0\npmulhw 40(%rax), %mm0\nmovq %mm0, 152(%rax)\nemms",
        ),
        (
            "mmx_pmaddwd_store",
            "movq 32(%rax), %mm0\npmaddwd 40(%rax), %mm0\nmovq %mm0, 160(%rax)\nemms",
        ),
        (
            "mmx_pand_store",
            "movq 32(%rax), %mm0\npand 40(%rax), %mm0\nmovq %mm0, 168(%rax)\nemms",
        ),
        (
            "mmx_pandn_store",
            "movq 32(%rax), %mm0\npandn 40(%rax), %mm0\nmovq %mm0, 176(%rax)\nemms",
        ),
        (
            "mmx_por_store",
            "movq 32(%rax), %mm0\npor 40(%rax), %mm0\nmovq %mm0, 184(%rax)\nemms",
        ),
        (
            "mmx_pxor_store",
            "movq 32(%rax), %mm0\npxor 40(%rax), %mm0\nmovq %mm0, 192(%rax)\nemms",
        ),
        (
            "mmx_pcmpeqb_store",
            "movq 32(%rax), %mm0\npcmpeqb 32(%rax), %mm0\nmovq %mm0, 200(%rax)\nemms",
        ),
        (
            "mmx_pcmpeqw_store",
            "movq 32(%rax), %mm0\npcmpeqw 32(%rax), %mm0\nmovq %mm0, 208(%rax)\nemms",
        ),
        (
            "mmx_pcmpgtb_store",
            "movq 32(%rax), %mm0\npcmpgtb 40(%rax), %mm0\nmovq %mm0, 216(%rax)\nemms",
        ),
        (
            "mmx_pcmpgtw_store",
            "movq 32(%rax), %mm0\npcmpgtw 40(%rax), %mm0\nmovq %mm0, 224(%rax)\nemms",
        ),
        (
            "mmx_packsswb_store",
            "movq 32(%rax), %mm0\npacksswb 40(%rax), %mm0\nmovq %mm0, 232(%rax)\nemms",
        ),
        (
            "mmx_packuswb_store",
            "movq 32(%rax), %mm0\npackuswb 40(%rax), %mm0\nmovq %mm0, 240(%rax)\nemms",
        ),
        (
            "mmx_packssdw_store",
            "movq 32(%rax), %mm0\npackssdw 40(%rax), %mm0\nmovq %mm0, 64(%rax)\nemms",
        ),
        (
            "mmx_psllw_store",
            "movq 32(%rax), %mm0\npsllw $3, %mm0\nmovq %mm0, 72(%rax)\nemms",
        ),
        (
            "mmx_psrlw_store",
            "movq 32(%rax), %mm0\npsrlw $5, %mm0\nmovq %mm0, 80(%rax)\nemms",
        ),
        (
            "mmx_psraw_store",
            "movq 32(%rax), %mm0\npsraw $4, %mm0\nmovq %mm0, 88(%rax)\nemms",
        ),
        (
            "mmx_pslld_store",
            "movq 32(%rax), %mm0\npslld $7, %mm0\nmovq %mm0, 96(%rax)\nemms",
        ),
        (
            "mmx_psrld_store",
            "movq 32(%rax), %mm0\npsrld $6, %mm0\nmovq %mm0, 104(%rax)\nemms",
        ),
        (
            "mmx_psrad_store",
            "movq 32(%rax), %mm0\npsrad $5, %mm0\nmovq %mm0, 112(%rax)\nemms",
        ),
        (
            "mmx_psllq_store",
            "movq 32(%rax), %mm0\npsllq $9, %mm0\nmovq %mm0, 120(%rax)\nemms",
        ),
        (
            "mmx_psrlq_store",
            "movq 32(%rax), %mm0\npsrlq $11, %mm0\nmovq %mm0, 128(%rax)\nemms",
        ),
        (
            "mmx_punpcklbw_store",
            "movq 32(%rax), %mm0\npunpcklbw 40(%rax), %mm0\nmovq %mm0, 80(%rax)\nemms",
        ),
        (
            "mmx_punpckhbw_store",
            "movq 32(%rax), %mm0\npunpckhbw 40(%rax), %mm0\nmovq %mm0, 136(%rax)\nemms",
        ),
        (
            "mmx_punpcklwd_store",
            "movq 32(%rax), %mm0\npunpcklwd 40(%rax), %mm0\nmovq %mm0, 144(%rax)\nemms",
        ),
        (
            "mmx_punpckhwd_store",
            "movq 32(%rax), %mm0\npunpckhwd 40(%rax), %mm0\nmovq %mm0, 152(%rax)\nemms",
        ),
        (
            "mmx_emms_then_x87_store",
            "movq 32(%rax), %mm0\npaddb 40(%rax), %mm0\nemms\nfld1\nfstpl 160(%rax)",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Mmx,
            profile: Int,
        });
    }

    // Processor state-management forms. Save areas live beyond the compared
    // scratch window; each case exposes the restored state through scratch
    // stores or the architectural vector register comparison.
    for &(label, asm, feat, profile) in &[
        (
            "mxcsr_ldmxcsr_stmxcsr_roundtrip",
            "movl $0x1f80, 48(%rbx)\nldmxcsr 48(%rbx)\nstmxcsr 56(%rbx)",
            Fxsave,
            Int,
        ),
        (
            "mxcsr_ldmxcsr_stmxcsr_round_up",
            "movl $0x5f80, 32(%rax)\nldmxcsr 32(%rax)\nstmxcsr 36(%rax)",
            Fxsave,
            Int,
        ),
        (
            "fxsave64_stores_mxcsr_and_mask",
            "movl $0x5f80, 32(%rax)\nldmxcsr 32(%rax)\nfxsave64 256(%rax)\nmovl 280(%rax), %r8d\nmovl %r8d, 36(%rax)\nmovl 284(%rax), %r8d\nmovl %r8d, 40(%rax)",
            Fxsave,
            Int,
        ),
        (
            "fxsave64_fxrstor_mxcsr_roundtrip",
            "movl $0x5f80, 32(%rax)\nldmxcsr 32(%rax)\nfxsave64 256(%rax)\nmovl $0x1f80, 36(%rax)\nldmxcsr 36(%rax)\nfxrstor64 256(%rax)\nstmxcsr 40(%rax)",
            Fxsave,
            Int,
        ),
        (
            "fxsave64_fxrstor_x87_roundtrip",
            "fninit\nfldl 32(%rax)\nfxsave64 256(%rax)\nfninit\nfxrstor64 256(%rax)\nfstpl 64(%rax)",
            Fxsave,
            F64,
        ),
        (
            "fxsave64_fxrstor_xmm_roundtrip",
            "fxsave64 256(%rax)\npxor %xmm1, %xmm1\nfxrstor64 256(%rax)",
            Fxsave,
            Int,
        ),
        (
            "xgetbv_xsetbv_xcr0_store",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nxgetbv\nmovl %eax, 48(%rbx)\nmovl %edx, 52(%rbx)",
            Xsave,
            Int,
        ),
        (
            "xsave64_xrstor64_zmm_roundtrip",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nxsave64 240(%rbx)\nvpxord %zmm1, %zmm1, %zmm1\nmovl $0xe7, %eax\nxorl %edx, %edx\nxrstor64 240(%rbx)",
            Xsave,
            Int,
        ),
        (
            "xsave64_xrstor64_mxcsr_roundtrip",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nmovl $0x5f80, 48(%rbx)\nldmxcsr 48(%rbx)\nmovl $0xe7, %eax\nxorl %edx, %edx\nxsave64 240(%rbx)\nmovl $0x1f80, 52(%rbx)\nldmxcsr 52(%rbx)\nmovl $0xe7, %eax\nxorl %edx, %edx\nxrstor64 240(%rbx)\nstmxcsr 56(%rbx)",
            Xsave,
            Int,
        ),
        (
            "xsave64_xrstor64_xmm_roundtrip",
            "movdqu 32(%rbx), %xmm2\nmovl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nxsave64 240(%rbx)\npxor %xmm2, %xmm2\nmovl $0xe7, %eax\nxorl %edx, %edx\nxrstor64 240(%rbx)\nmovdqu %xmm2, 64(%rbx)",
            Xsave,
            Int,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile,
        });
    }

    // Cache-maintenance and memory-ordering instructions. The cache side
    // effects are not directly observable, so each case brackets the operation
    // with ordinary loads/stores and relies on the GPR/RFLAGS/scratch diff to
    // catch architectural side effects or address calculation mistakes.
    for &(label, asm, feat) in &[
        (
            "lfence_load_order",
            "movq 32(%rax), %r8\nlfence\nmovq %r8, 64(%rax)",
            Fence,
        ),
        (
            "mfence_store_order",
            "movq %r8, 32(%rax)\nmfence\nmovq 32(%rax), %rcx",
            Fence,
        ),
        (
            "sfence_store_order",
            "movq %r8, 40(%rax)\nsfence\nmovq 40(%rax), %rcx",
            Fence,
        ),
        (
            "clflush_cache_line",
            "movq %r8, 32(%rax)\nclflush 32(%rax)\nmovq 32(%rax), %rcx",
            Clflush,
        ),
        (
            "clflushopt_cache_line",
            "movq %r8, 64(%rax)\nclflushopt 64(%rax)\nsfence\nmovq 64(%rax), %rcx",
            Clflushopt,
        ),
        (
            "clwb_cache_line",
            "movq %r8, 96(%rax)\nclwb 96(%rax)\nsfence\nmovq 96(%rax), %rcx",
            Clwb,
        ),
        (
            "cldemote_cache_line",
            "movq %r8, 128(%rax)\ncldemote 128(%rax)\nmovq 128(%rax), %rcx",
            Cldemote,
        ),
        (
            "invd_preserves_observable_state",
            "movq %r8, 32(%rax)\ninvd\nmovq 32(%rax), %rcx",
            CacheInvd,
        ),
        (
            "wbinvd_preserves_observable_state",
            "movq %r8, 48(%rax)\nwbinvd\nmovq 48(%rax), %rcx",
            CacheInvd,
        ),
        (
            "wbnoinvd_preserves_observable_state",
            "movq %r8, 64(%rax)\nwbnoinvd\nmovq 64(%rax), %rcx",
            Wbnoinvd,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }

    // Hint/no-op instructions must decode addressing forms and preserve all
    // architectural state that is visible to this harness.
    for &(label, asm, feat) in &[
        ("nop_one_byte", "nop", HintNop),
        ("nop_rm_disp", "nopl 32(%rax)", HintNop),
        ("nopw_rm_disp", "nopw 48(%rax)", HintNop),
        (
            "nop_rm_sib_zero_index",
            "xorl %ecx, %ecx\nnopl 64(%rax,%rcx,1)",
            HintNop,
        ),
        (
            "pause_preserves_cmp_flags",
            "cmpq %rcx, %r8\npause",
            HintNop,
        ),
        (
            "pause_between_memory_ops",
            "movq %r8, 80(%rax)\npause\nmovq 80(%rax), %rcx",
            HintNop,
        ),
        ("prefetchnta_memory", "prefetchnta 96(%rax)", HintNop),
        ("prefetcht0_memory", "prefetcht0 112(%rax)", HintNop),
        ("prefetcht1_memory", "prefetcht1 128(%rax)", HintNop),
        ("prefetcht2_memory", "prefetcht2 144(%rax)", HintNop),
        (
            "prefetch_sib_zero_index",
            "xorl %ecx, %ecx\nprefetcht0 160(%rax,%rcx,1)",
            HintNop,
        ),
        ("prefetchw_memory", "prefetchw 176(%rax)", Prefetchw),
        (
            "prefetchw_preserves_cmp_flags",
            "cmpq %rcx, %r8\nprefetchw 192(%rax)",
            Prefetchw,
        ),
        (
            "prefetchw_sib_zero_index",
            "xorl %ecx, %ecx\nprefetchw 208(%rax,%rcx,1)",
            Prefetchw,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }

    // MONITOR arms an address range for subsequent MWAIT. The monitor setup has
    // no directly visible architectural side effect, so these cases exercise
    // operand setup, hint registers, flags, and the addr32 prefix without MWAIT.
    for &(label, asm) in &[
        (
            "monitor_scratch_base",
            "xorl %ecx, %ecx\nxorl %edx, %edx\nmonitor",
        ),
        (
            "monitor_scratch_offset",
            "leaq 64(%rax), %rax\nxorl %ecx, %ecx\nxorl %edx, %edx\nmonitor",
        ),
        (
            "monitor_preserves_cmp_flags",
            "xorl %ecx, %ecx\nxorl %edx, %edx\ncmpq %r8, %r9\nmonitor",
        ),
        (
            "monitor_addr32_zero_ext_address",
            "movabsq $0xffff000000004020, %rax\nxorl %ecx, %ecx\nxorl %edx, %edx\naddr32 monitor",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Monitor,
            profile: Int,
        });
    }

    // Serialization and user-level wait instructions. Their ordering and
    // monitor/wait side effects are not directly visible, so these snippets
    // bracket them with deterministic GPR, flag, and scratch-visible state.
    for &(label, asm, feat) in &[
        ("serialize_basic", "serialize", Serialize),
        (
            "serialize_between_memory_ops",
            "movq %r8, 32(%rax)\nserialize\nmovq 32(%rax), %rcx",
            Serialize,
        ),
        (
            "serialize_preserves_cmp_flags",
            "cmpq %rcx, %r8\nserialize",
            Serialize,
        ),
        (
            "umonitor_r64_address",
            "leaq 32(%rax), %r8\numonitor %r8\nmovq %r8, %rcx",
            Waitpkg,
        ),
        (
            "umwait_zero_deadline",
            "xorl %edx, %edx\nxorl %eax, %eax\nxorl %ecx, %ecx\numwait %ecx",
            Waitpkg,
        ),
        (
            "tpause_zero_deadline",
            "xorl %edx, %edx\nxorl %eax, %eax\nxorl %ecx, %ecx\ntpause %ecx",
            Waitpkg,
        ),
        (
            "umonitor_umwait_zero_deadline",
            "leaq 64(%rax), %r8\numonitor %r8\nxorl %edx, %edx\nxorl %eax, %eax\nxorl %ecx, %ecx\numwait %ecx",
            Waitpkg,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }

    // RDPID reads IA32_TSC_AUX. Fresh KVM vCPUs expose zero here, matching the
    // emulator model; the cases verify destination width, extended registers,
    // and flag preservation. The explicit byte case covers the REX.W form that
    // llvm-mc does not spell directly in 64-bit AT&T syntax.
    for &(label, asm) in &[
        ("rdpid_rax_zeroext", "movabsq $-1, %rax\nrdpid %rax"),
        ("rdpid_r8_zeroext", "movabsq $-1, %r8\nrdpid %r8"),
        ("rdpid_preserves_cmp_flags", "cmpq %rcx, %r8\nrdpid %r9"),
        ("rdpid_rexw_rax", ".byte 0xf3, 0x48, 0x0f, 0xc7, 0xf8\n"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Rdpid,
            profile: Int,
        });
    }

    // RDRAND/RDSEED produce non-deterministic data, so these cases retry until
    // success and then normalize the destination with flag-preserving MOVs.
    // Width cases branch on normalized architectural invariants and finish with
    // deterministic ADD flags, avoiding dependence on the random payload.
    for &(label, asm, feat) in &[
        (
            "rdrand_r64_success_flags",
            "1:\nrdrand %rax\njnc 1b\nmovq $0, %rax",
            Rdrand,
        ),
        (
            "rdrand_r16_preserves_upper",
            "movabsq $0x1122334455667788, %r8\n1:\nrdrand %r8w\njnc 1b\nmovw $0, %r8w",
            Rdrand,
        ),
        (
            "rdrand_r32_zeroext",
            "movabsq $-1, %r9\n1:\nrdrand %r9d\njnc 1b\nmovabsq $0xffffffff00000000, %r10\ntestq %r10, %r9\njz 2f\nmovq $1, %r9\njmp 3f\n2:\nmovq $0, %r9\n3:\naddq $0, %r9",
            Rdrand,
        ),
        (
            "rdseed_r64_success_flags",
            "1:\nrdseed %rax\njnc 1b\nmovq $0, %rax",
            Rdseed,
        ),
        (
            "rdseed_r16_preserves_upper",
            "movabsq $0x8877665544332211, %r8\n1:\nrdseed %r8w\njnc 1b\nmovw $0, %r8w",
            Rdseed,
        ),
        (
            "rdseed_r32_zeroext",
            "movabsq $-1, %r9\n1:\nrdseed %r9d\njnc 1b\nmovabsq $0xffffffff00000000, %r10\ntestq %r10, %r9\njz 2f\nmovq $1, %r9\njmp 3f\n2:\nmovq $0, %r9\n3:\naddq $0, %r9",
            Rdseed,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }

    // INVLPG invalidates TLB state for the addressed page but has no visible
    // architectural output here. These cases cover ordinary, extended-base, and
    // 32-bit address-size forms while bracketing the instruction with visible
    // GPR/memory state.
    for &(label, asm) in &[
        (
            "invlpg_mem",
            "movq %r8, 32(%rax)\ninvlpg 32(%rax)\nmovq 32(%rax), %rcx",
        ),
        (
            "invlpg_extended_base",
            "leaq 64(%rax), %r8\ninvlpg (%r8)\nmovq %r8, %rcx",
        ),
        (
            "invlpg_addr32_mem",
            "movq %r8, 96(%rax)\naddr32\ninvlpg 96(%eax)\nmovq 96(%rax), %rcx",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Invlpg,
            profile: Int,
        });
    }

    // Privileged machine-state instructions. These seed known architectural
    // state before reading it back, avoiding dependence on different initial
    // KVM/interpreter CRx, descriptor-table, or MSR setup.
    for &(label, asm, feat) in &[
        (
            "control_cr2_roundtrip",
            "movabsq $0x0000000000123450, %r8\nmovq %r8, %cr2\nmovq %cr2, %rcx",
            ControlReg,
        ),
        (
            "control_cr4_roundtrip",
            "movabsq $0x0000000000050620, %r8\nmovq %r8, %cr4\nmovq %cr4, %rcx",
            ControlReg,
        ),
        (
            "control_smsw_widths",
            "movabsq $-1, %r8\nsmsw %r8w\nmovabsq $-1, %r9\nsmsw %r9w",
            ControlReg,
        ),
        (
            "control_lmsw_clts_observable_msw",
            "movw $0x000b, 32(%rax)\nlmsw 32(%rax)\nsmsw 40(%rax)\nclts\nsmsw 48(%rax)",
            ControlReg,
        ),
        (
            "descriptor_lgdt_lidt_store_roundtrip",
            "movw $0x001f, 32(%rax)\nmovabsq $0x0000000000006000, %r8\nmovq %r8, 34(%rax)\nmovw $0x0037, 48(%rax)\nmovabsq $0x0000000000007000, %r8\nmovq %r8, 50(%rax)\nlgdt 32(%rax)\nlidt 48(%rax)\nsgdt 64(%rax)\nsidt 80(%rax)",
            DescriptorTable,
        ),
        (
            "descriptor_addr32_roundtrip",
            "movw $0x004f, 96(%rax)\nmovabsq $0x0000000000006100, %r8\nmovq %r8, 98(%rax)\nmovw $0x0057, 112(%rax)\nmovabsq $0x0000000000007100, %r8\nmovq %r8, 114(%rax)\naddr32\nlgdt 96(%eax)\naddr32\nlidt 112(%eax)\naddr32\nsgdt 128(%eax)\naddr32\nsidt 144(%eax)",
            DescriptorTable,
        ),
        (
            "descriptor_extended_base_roundtrip",
            "leaq 160(%rax), %r8\nmovw $0x006f, (%r8)\nmovabsq $0x0000000000006200, %r9\nmovq %r9, 2(%r8)\nmovw $0x0077, 16(%r8)\nmovabsq $0x0000000000007200, %r9\nmovq %r9, 18(%r8)\nlgdt (%r8)\nlidt 16(%r8)\nsgdt 32(%r8)\nsidt 48(%r8)",
            DescriptorTable,
        ),
        (
            "msr_fs_base_roundtrip",
            "movl $0xc0000100, %ecx\nmovl $0xdead0000, %eax\nmovl $0x00007fff, %edx\nwrmsr\nrdmsr\nmovq %rax, %rbx\nmovq %rdx, %rsi",
            Msr,
        ),
        (
            "msr_gs_base_roundtrip",
            "movl $0xc0000101, %ecx\nmovl $0x0badf00d, %eax\nmovl $0, %edx\nwrmsr\nrdmsr\nmovq %rax, %rbx\nmovq %rdx, %rsi",
            Msr,
        ),
        (
            "msr_kernel_gs_base_roundtrip",
            "movl $0xc0000102, %ecx\nmovl $0x00001000, %eax\nmovl $0xffff8800, %edx\nwrmsr\nrdmsr\nmovq %rax, %rbx\nmovq %rdx, %rsi",
            Msr,
        ),
        (
            "debug_dr0_dr1_roundtrip",
            "movabsq $0x0000000000004000, %r8\nmovq %r8, %dr0\nmovq %dr0, %rcx\nmovabsq $0x0000000000004008, %r8\nmovq %r8, %dr1\nmovq %dr1, %rdx",
            DebugReg,
        ),
        (
            "debug_dr2_dr3_roundtrip",
            "movabsq $0x0000000000004010, %r8\nmovq %r8, %dr2\nmovq %dr2, %rcx\nmovabsq $0x0000000000004018, %r8\nmovq %r8, %dr3\nmovq %dr3, %rdx",
            DebugReg,
        ),
        (
            "debug_dr7_zero_roundtrip",
            "movabsq $0x400, %r8\nmovq %r8, %dr7\nmovq %dr7, %r9",
            DebugReg,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }

    // Descriptor access checks. Each case installs a tiny local GDT with one
    // present writable data descriptor, then observes LAR/LSL/VERR/VERW through
    // stable GPR booleans or the architecturally loaded segment limit.
    let descriptor_access_setup = "movq $0, 128(%rax)\nmovabsq $0x0041930000002345, %r8\nmovq %r8, 136(%rax)\nmovw $0x000f, 32(%rax)\nleaq 128(%rax), %r8\nmovq %r8, 34(%rax)\nlgdt 32(%rax)";
    for &(label, check) in &[
        (
            "lsl_descriptor_limit",
            "movw $0x8, %r8w\nlsl %r8w, %r9d\ncmpl $0x12345, %r9d\nsete %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\ncmpq %rcx, %rcx",
        ),
        (
            "lar_descriptor_valid_nonzero",
            "movw $0x8, %r8w\nlar %r8w, %r9d\nsetz %cl\ntestl %r9d, %r9d\nsetnz %dl\nandb %dl, %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r9, %r9\ncmpq %rcx, %rcx",
        ),
        (
            "lar_descriptor_invalid_preserves_dest",
            "movl $0x7777, %r9d\nmovw $0x18, %r8w\nlar %r8w, %r9d\nsetnz %cl\ncmpl $0x7777, %r9d\nsete %dl\nandb %dl, %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r9, %r9\ncmpq %rcx, %rcx",
        ),
        (
            "verr_descriptor_readable",
            "movw $0x8, %r8w\nverr %r8w\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\ncmpq %rcx, %rcx",
        ),
        (
            "verw_descriptor_writable",
            "movw $0x8, %r8w\nverw %r8w\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\ncmpq %rcx, %rcx",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: format!("{descriptor_access_setup}\n{check}"),
            feat: DescriptorAccess,
            profile: Int,
        });
    }

    // STAC/CLAC are SMAP access-control flag operations. The ordinary status
    // flags are preserved while AC is the visible architectural output.
    for &(label, asm) in &[
        ("stac_sets_ac", "stac"),
        ("clac_clears_ac", "stac\nclac"),
        ("stac_clac_repeated", "stac\nstac\nclac\nclac"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Smap,
            profile: Int,
        });
    }

    // PKRU and SWAPGS privileged state instructions. Setup instructions may
    // touch flags, so each snippet establishes deterministic CMP flags
    // immediately before the operation being checked.
    for &(label, asm, feat) in &[
        (
            "pkru_write_read_roundtrip",
            "movq %cr4, %rax\norq $0x400000, %rax\nmovq %rax, %cr4\ncmpq %rcx, %r8\nmovl $0x55555550, %eax\nmovl $0, %ecx\nmovl $0, %edx\nwrpkru\nmovq $-1, %rax\nmovq $-1, %rdx\nrdpkru",
            Pku,
        ),
        (
            "pkru_last_write_visible",
            "movq %cr4, %rax\norq $0x400000, %rax\nmovq %rax, %cr4\ncmpq %rcx, %r8\nmovl $0xaaaaaaa0, %eax\nmovl $0, %ecx\nmovl $0, %edx\nwrpkru\nmovl $0xcafebab0, %eax\nwrpkru\nrdpkru",
            Pku,
        ),
        (
            "swapgs_roundtrip_rdgsbase",
            "movl $0xc0000102, %ecx\nmovabsq $0x0000000000789000, %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nwrmsr\nmovabsq $0x0000000000123000, %rax\nwrgsbase %rax\ncmpq %rcx, %r8\nrdgsbase %rbx\nswapgs\nrdgsbase %rcx\nswapgs\nrdgsbase %rdx",
            Swapgs,
        ),
        (
            "swapgs_extended_rdgsbase",
            "movl $0xc0000102, %ecx\nmovabsq $0x00000000009ab000, %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nwrmsr\nmovabsq $0x0000000000246000, %rax\nwrgsbase %rax\ncmpq %rcx, %r8\nswapgs\nrdgsbase %r8\nswapgs\nrdgsbase %r9",
            Swapgs,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }

    // RDTSC/RDTSCP return time-varying values. These cases either normalize
    // the outputs with flag-preserving MOVs or shift away variable payload bits
    // while restoring the instruction-preserved flags.
    for &(label, asm, feat) in &[
        (
            "rdtsc_preserves_cmp_flags",
            "cmpq %rcx, %r8\nrdtsc\nmovq $0, %rax\nmovq $0, %rdx",
            Tsc,
        ),
        (
            "rdtsc_zero_extends_outputs",
            "rdtsc\npushfq\npopq %rbx\nshrq $32, %rax\nshrq $32, %rdx\npushq %rbx\npopfq",
            Tsc,
        ),
        (
            "rdtscp_preserves_cmp_flags",
            "cmpq %rcx, %r8\nrdtscp\nmovq $0, %rax\nmovq $0, %rdx\nmovq $0, %rcx",
            Rdtscp,
        ),
        (
            "rdtscp_zero_extends_outputs",
            "rdtscp\npushfq\npopq %rbx\nshrq $32, %rax\nshrq $32, %rdx\nshrq $32, %rcx\npushq %rbx\npopfq",
            Rdtscp,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }

    // FSGSBASE instructions. The harness does not snapshot segment bases
    // directly, so each write is immediately read back into compared GPRs;
    // 32-bit read/write forms also check architectural zero-extension.
    for &(label, asm) in &[
        (
            "wrfsbase_r64_rdfsbase",
            "movabsq $0x0000000012345000, %r8\nwrfsbase %r8\nrdfsbase %rcx",
        ),
        (
            "wrgsbase_r64_rdgsbase",
            "movabsq $0x0000000023456000, %r8\nwrgsbase %r8\nrdgsbase %rcx",
        ),
        (
            "wrfsbase_r32_zeroext",
            "movabsq $0xffff800012345678, %r8\nwrfsbase %r8d\nrdfsbase %rcx",
        ),
        (
            "wrgsbase_r32_zeroext",
            "movabsq $0xffff800087654321, %r8\nwrgsbase %r8d\nrdgsbase %rcx",
        ),
        (
            "rdfsbase_r32_zeroext_dest",
            "movabsq $0x00000000fedcba98, %rcx\nwrfsbase %rcx\nmovabsq $0xffffffffffffffff, %r8\nrdfsbase %r8d",
        ),
        (
            "rdgsbase_r32_zeroext_dest",
            "movabsq $0x0000000076543210, %rcx\nwrgsbase %rcx\nmovabsq $0xffffffffffffffff, %r8\nrdgsbase %r8d",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Fsgsbase,
            profile: Int,
        });
    }

    // ENTER/LEAVE implicit stack-frame operations. The harness snapshots the
    // stack window and GPRs, so both pushed frame links and final RBP/RSP are
    // compared against KVM.
    for &(label, asm) in &[
        ("enter_frame_alloc16", "enter $0x10, $0"),
        ("enter_frame_nested1", "enter $0x8, $1"),
        ("enter_leave_roundtrip", "enter $0x20, $0\nleave"),
        (
            "leave_frame_from_scratch",
            "leaq 64(%rax), %rbp\nmovabsq $0x1122334455667788, %r8\nmovq %r8, (%rbp)\nleave",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: StackFrame,
            profile: Int,
        });
    }

    // POPFQ/CLI/STI affect RFLAGS and, for POPFQ, implicit stack state. The
    // flag mask below includes IF/DF for this feature so interrupt/direction
    // flag transitions are part of the KVM comparison, not just status flags.
    for &(label, asm) in &[
        ("popfq_clear_status_flags", "pushq $0x2\npopfq"),
        ("popfq_restore_if_df", "pushq $0x602\npopfq"),
        ("pushfq_popfq_roundtrip", "pushfq\npopfq"),
        ("sti_sets_interrupt_flag", "sti"),
        ("sti_cli_clears_interrupt_flag", "sti\ncli"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: FlagControl,
            profile: Int,
        });
    }

    // Scalar extension, byte-swap, exchange, and compare/exchange forms. These
    // intentionally mix register and memory destinations so GPR, flag, and
    // scratch effects are all checked against silicon.
    for &(label, asm) in &[
        ("movzx_core_r8b_rcx", "movzbq %r8b, %rcx"),
        ("movzx_core_r8w_rcx", "movzwq %r8w, %rcx"),
        ("movsx_core_r8b_rcx", "movsbq %r8b, %rcx"),
        ("movsx_core_r8w_rcx", "movswq %r8w, %rcx"),
        ("movsxd_core_r8d_rcx", "movslq %r8d, %rcx"),
        ("movzx_core_m8_r8", "movzbq 32(%rax), %r8"),
        ("movzx_core_m16_r8", "movzwq 32(%rax), %r8"),
        ("movsx_core_m8_r8", "movsbq 33(%rax), %r8"),
        ("movsx_core_m16_r8", "movswq 34(%rax), %r8"),
        ("movsxd_core_m32_r8", "movslq 36(%rax), %r8"),
        ("bswap_core_r32", "bswapl %r8d"),
        ("bswap_core_r64", "bswapq %r8"),
        ("xchg_core_r64_reg", "xchgq %rcx, %r8"),
        ("xchg_core_r32_reg", "xchgl %ecx, %r8d"),
        ("xchg_core_rax_r8", "xchgq %rax, %r8"),
        ("xchg_core_m64_r8", "xchgq %r8, 8(%rax)"),
        ("xchg_core_m8_r8b", "xchgb %r8b, 2(%rax)"),
        ("xchg_core_m32_r8d", "xchgl %r8d, 16(%rax)"),
        ("xadd_core_r64_reg", "xaddq %rcx, %r8"),
        ("xadd_core_r32_reg", "xaddl %ecx, %r8d"),
        ("xadd_core_m64_r8", "xaddq %r8, 8(%rax)"),
        ("xadd_core_m8_r8b", "xaddb %r8b, 2(%rax)"),
        ("xadd_core_m32_r8d", "xaddl %r8d, 16(%rax)"),
        ("cmpxchg_core_r64_success", "cmpxchgq %r8, %rax"),
        ("cmpxchg_core_r32_success", "cmpxchgl %r8d, %eax"),
        ("cmpxchg_core_r64_fail", "cmpxchgq %rcx, %r8"),
        ("cmpxchg_core_m64_fail", "cmpxchgq %r8, 8(%rax)"),
        ("cmpxchg_core_m8_fail", "cmpxchgb %r8b, 2(%rax)"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // Double-width atomic compare/exchange. These explicitly seed the memory
    // operand plus accumulator/new-value registers, then check success, failure,
    // and LOCK-prefixed memory forms through the scratch/GPR/RFLAGS diff.
    for &(label, asm, feat) in &[
        (
            "cmpxchg8b_success",
            "movl $0x11223344, 32(%rdi)\nmovl $0x55667788, 36(%rdi)\nmovl $0x11223344, %eax\nmovl $0x55667788, %edx\nmovl $0xaabbccdd, %ebx\nmovl $0xeeff0011, %ecx\ncmpxchg8b 32(%rdi)",
            Cx8,
        ),
        (
            "cmpxchg8b_failure",
            "movl $0x01234567, 32(%rdi)\nmovl $0x89abcdef, 36(%rdi)\nmovl $0x11223344, %eax\nmovl $0x55667788, %edx\nmovl $0xaabbccdd, %ebx\nmovl $0xeeff0011, %ecx\ncmpxchg8b 32(%rdi)",
            Cx8,
        ),
        (
            "cmpxchg8b_lock_success",
            "movl $0x10203040, 32(%rdi)\nmovl $0x50607080, 36(%rdi)\nmovl $0x10203040, %eax\nmovl $0x50607080, %edx\nmovl $0x90a0b0c0, %ebx\nmovl $0xd0e0f000, %ecx\nlock cmpxchg8b 32(%rdi)",
            Cx8,
        ),
        (
            "cmpxchg16b_success",
            "movabsq $0x1122334455667788, %r8\nmovq %r8, 64(%rdi)\nmovabsq $0x99aabbccddeeff00, %r8\nmovq %r8, 72(%rdi)\nmovabsq $0x1122334455667788, %rax\nmovabsq $0x99aabbccddeeff00, %rdx\nmovabsq $0x0102030405060708, %rbx\nmovabsq $0x1112131415161718, %rcx\ncmpxchg16b 64(%rdi)",
            Cx16,
        ),
        (
            "cmpxchg16b_failure",
            "movabsq $0x8877665544332211, %r8\nmovq %r8, 64(%rdi)\nmovabsq $0x00ffeeddccbbaa99, %r8\nmovq %r8, 72(%rdi)\nmovabsq $0x1122334455667788, %rax\nmovabsq $0x99aabbccddeeff00, %rdx\nmovabsq $0x0102030405060708, %rbx\nmovabsq $0x1112131415161718, %rcx\ncmpxchg16b 64(%rdi)",
            Cx16,
        ),
        (
            "cmpxchg16b_lock_success",
            "movabsq $0x123456789abcdef0, %r8\nmovq %r8, 64(%rdi)\nmovabsq $0x0fedcba987654321, %r8\nmovq %r8, 72(%rdi)\nmovabsq $0x123456789abcdef0, %rax\nmovabsq $0x0fedcba987654321, %rdx\nmovabsq $0x55aa55aa55aa55aa, %rbx\nmovabsq $0xaa55aa55aa55aa55, %rcx\nlock cmpxchg16b 64(%rdi)",
            Cx16,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }

    // Core integer ALU and logical forms. The table spans register, memory,
    // immediate, accumulator-addressed, and scratch-writing paths while using
    // r8/rcx/rdx plus the scratch page so the harness observes every effect.
    for &(label, asm) in &[
        ("add_core_r64_reg", "addq %rcx, %r8"),
        ("add_core_r32_reg", "addl %ecx, %r8d"),
        ("add_core_r64_mem_src", "addq 8(%rax), %r8"),
        ("add_core_m64_r8", "addq %r8, 8(%rax)"),
        ("add_core_r64_imm32", "addq $0x1234, %r8"),
        ("add_core_r64_imm8", "addq $-7, %r8"),
        ("add_core_m64_imm8", "addq $0x20, 8(%rax)"),
        ("adc_core_r64_reg", "adcq %rcx, %r8"),
        ("adc_core_r64_mem_src", "adcq 8(%rax), %r8"),
        ("sub_core_r64_reg", "subq %rcx, %r8"),
        ("sub_core_r64_mem_src", "subq 8(%rax), %r8"),
        ("sub_core_m64_r8", "subq %r8, 16(%rax)"),
        ("sbb_core_r64_reg", "sbbq %rcx, %r8"),
        ("cmp_core_r64_reg", "cmpq %rcx, %r8"),
        ("cmp_core_r64_mem_src", "cmpq 8(%rax), %r8"),
        ("cmp_core_r64_imm32", "cmpq $0x1234, %r8"),
        ("inc_core_r64", "incq %r8"),
        ("dec_core_r64", "decq %r8"),
        ("inc_core_m64", "incq 8(%rax)"),
        ("dec_core_m64", "decq 16(%rax)"),
        ("neg_core_r64", "negq %r8"),
        ("neg_core_m64", "negq 24(%rax)"),
        ("not_core_r64", "notq %r8"),
        ("not_core_m64", "notq 32(%rax)"),
        ("and_core_r64_reg", "andq %rcx, %r8"),
        ("and_core_r64_mem_src", "andq 8(%rax), %r8"),
        ("and_core_m64_r8", "andq %r8, 40(%rax)"),
        ("or_core_r64_reg", "orq %rcx, %r8"),
        ("xor_core_r64_reg", "xorq %rcx, %r8"),
        ("test_core_r64_reg", "testq %rcx, %r8"),
        ("test_core_m64_r8", "testq 8(%rax), %r8"),
        ("and_core_r64_imm8", "andq $0x7f, %r8"),
        ("or_core_r64_imm32", "orq $0x1234, %r8"),
        ("xor_core_r64_imm32", "xorq $0x55aa, %r8"),
        ("test_core_r64_imm32", "testq $0x55aa, %r8"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // Core rotate, shift, and double-shift forms. These cover one-bit counts
    // with OF defined, multi-bit immediate counts with OF undefined, CL counts
    // sourced from the seeded RCX, and memory destinations in the scratch page.
    for &(label, asm) in &[
        ("rol_core_r64_one", "rolq $1, %r8"),
        ("ror_core_r64_one", "rorq $1, %r8"),
        ("rcl_core_r64_one", "rclq $1, %r8"),
        ("rcr_core_r64_one", "rcrq $1, %r8"),
        ("shl_core_r64_one", "shlq $1, %r8"),
        ("shr_core_r64_one", "shrq $1, %r8"),
        ("sar_core_r64_one", "sarq $1, %r8"),
        ("rol_core_r64_imm5", "rolq $5, %r8"),
        ("ror_core_r64_imm7", "rorq $7, %r8"),
        ("shl_core_r64_imm4", "shlq $4, %r8"),
        ("shr_core_r64_imm6", "shrq $6, %r8"),
        ("sar_core_r64_imm6", "sarq $6, %r8"),
        ("shl_core_r64_cl", "shlq %cl, %r8"),
        ("shr_core_r64_cl", "shrq %cl, %r8"),
        ("sar_core_r64_cl", "sarq %cl, %r8"),
        ("rol_core_r64_cl", "rolq %cl, %r8"),
        ("ror_core_r64_cl", "rorq %cl, %r8"),
        ("shl_core_m64_imm3", "shlq $3, 8(%rax)"),
        ("shr_core_m64_imm5", "shrq $5, 16(%rax)"),
        ("sar_core_m64_imm7", "sarq $7, 24(%rax)"),
        ("shld_core_r64_imm5", "shldq $5, %rcx, %r8"),
        ("shrd_core_r64_imm5", "shrdq $5, %rcx, %r8"),
        ("shld_core_r64_cl", "shldq %cl, %rdx, %r8"),
        ("shrd_core_r64_cl", "shrdq %cl, %rdx, %r8"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // Core integer multiply and divide forms. One-operand MUL/IMUL and
    // DIV/IDIV exercise the implicit RDX:RAX / RAX architectural operands; the
    // explicit IMUL forms cover register, memory, imm8, and imm32 encodings.
    for &(label, asm) in &[
        ("imul_core_r64_reg", "imulq %rcx, %r8"),
        ("imul_core_r32_reg", "imull %ecx, %r8d"),
        ("imul_core_r64_mem_src", "imulq 8(%rax), %r8"),
        ("imul_core_r64_imm8", "imulq $-3, %rcx, %r8"),
        ("imul_core_r64_imm32", "imulq $0x10000, %rcx, %r8"),
        ("imul_core_r64_mem_imm8", "imulq $7, 8(%rax), %r8"),
        ("mul_core_r64_reg", "mulq %r8"),
        ("mul_core_r32_reg", "mull %ecx"),
        ("mul_core_r64_mem", "mulq 8(%rax)"),
        ("imul_one_core_r64_reg", "imulq %rcx"),
        ("imul_one_core_r32_reg", "imull %ecx"),
        ("imul_one_core_r64_mem", "imulq 8(%rax)"),
        ("div_core_r64_reg", "divq %r8"),
        ("div_core_r32_reg", "divl %ecx"),
        ("idiv_core_r64_reg", "idivq %r8"),
        ("idiv_core_r32_reg", "idivl %ecx"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // Core string instructions. These use case-specific RSI/RDI/RCX seeds and
    // a non-repeating scratch pattern so both pointer movement and memory side
    // effects are visible in the KVM diff.
    for &(label, asm) in &[
        ("movsb_core_string", "movsb"),
        ("movsw_core_string", "movsw"),
        ("movsl_core_string", "movsl"),
        ("movsq_core_string", "movsq"),
        ("movsb_core_string_df", "movsb"),
        ("rep_movsb_core_string", "rep movsb"),
        ("rep_movsw_core_string", "rep movsw"),
        ("rep_movsl_core_string", "rep movsl"),
        ("rep_movsq_core_string", "rep movsq"),
        ("repne_movsb_core_string", "repne movsb"),
        ("rep_movsb_core_string_df", "rep movsb"),
        ("rep_movsb_core_string_count_zero", "rep movsb"),
        ("addr32_movsb_core_string", "addr32 movsb"),
        ("addr32_rep_movsb_core_string", "addr32 rep movsb"),
        ("stosb_core_string", "stosb"),
        ("stosw_core_string", "stosw"),
        ("stosl_core_string", "stosl"),
        ("stosq_core_string", "stosq"),
        ("stosb_core_string_df", "stosb"),
        ("rep_stosb_core_string", "rep stosb"),
        ("rep_stosw_core_string", "rep stosw"),
        ("rep_stosl_core_string", "rep stosl"),
        ("rep_stosq_core_string", "rep stosq"),
        ("rep_stosq_core_string_count_zero", "rep stosq"),
        ("addr32_rep_stosq_core_string", "addr32 rep stosq"),
        ("lodsb_core_string", "lodsb"),
        ("lodsw_core_string", "lodsw"),
        ("lodsl_core_string", "lodsl"),
        ("lodsq_core_string", "lodsq"),
        ("lodsb_core_string_df", "lodsb"),
        ("rep_lodsb_core_string", "rep lodsb"),
        ("rep_lodsq_core_string", "rep lodsq"),
        ("rep_lodsq_core_string_count_zero", "rep lodsq"),
        ("addr32_lodsl_core_string", "addr32 lodsl"),
        ("scasb_core_string", "scasb"),
        ("scasw_core_string", "scasw"),
        ("scasl_core_string", "scasl"),
        ("scasq_core_string", "scasq"),
        ("scasb_core_string_df", "scasb"),
        ("repe_scasb_core_string", "repe scasb"),
        ("repne_scasb_core_string", "repne scasb"),
        ("repne_scasq_core_string", "repne scasq"),
        ("repne_scasq_core_string_count_zero", "repne scasq"),
        ("addr32_scasl_core_string", "addr32 scasl"),
        ("cmpsb_core_string", "cmpsb"),
        ("cmpsw_core_string", "cmpsw"),
        ("cmpsl_core_string", "cmpsl"),
        ("cmpsq_core_string", "cmpsq"),
        ("cmpsb_core_string_df", "cmpsb"),
        ("repe_cmpsb_core_string", "repe cmpsb"),
        ("repne_cmpsb_core_string", "repne cmpsb"),
        ("repe_cmpsq_core_string", "repe cmpsq"),
        ("repe_cmpsb_core_string_count_zero", "repe cmpsb"),
        ("addr32_repe_cmpsb_core_string", "addr32 repe cmpsb"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // SSE4.2 CRC32C accumulator forms. These update r8/r8d while leaving flags
    // unchanged, with source coverage across byte/word/dword/qword and memory.
    for &(label, asm) in &[
        ("crc32_r32_m8", "crc32b (%rax), %r8d"),
        ("crc32_r32_m16", "crc32w (%rax), %r8d"),
        ("crc32_r32_m32", "crc32l (%rax), %r8d"),
        ("crc32_r64_m64", "crc32q (%rax), %r8"),
        ("crc32_r32_al", "crc32b %al, %r8d"),
        ("crc32_r32_ax", "crc32w %ax, %r8d"),
        ("crc32_r32_eax", "crc32l %eax, %r8d"),
        ("crc32_r64_rax", "crc32q %rax, %r8"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Crc32,
            profile: Int,
        });
    }

    // POPCNT scalar forms. These cover 16/32/64-bit destinations, register and
    // memory sources, plus the architectural flag result.
    for &(label, asm) in &[
        ("popcnt_r16_ax_r8w", "popcnt %ax, %r8w"),
        ("popcnt_r32_eax_r8d", "popcnt %eax, %r8d"),
        ("popcnt_r64_rax_r8", "popcnt %rax, %r8"),
        ("popcnt_r16_m16_r8w", "popcnt (%rax), %r8w"),
        ("popcnt_r32_m32_r8d", "popcnt (%rax), %r8d"),
        ("popcnt_r64_m64_r8", "popcnt (%rax), %r8"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Popcnt,
            profile: Int,
        });
    }

    // BMI1 scalar bit-manipulation forms. These cover VEX register and memory
    // operands plus flag-producing extract/count behavior with undefined flags
    // masked per instruction.
    for &(label, asm) in &[
        ("andn_bmi1_r64_reg", "andnq %rcx, %r8, %r8"),
        ("andn_bmi1_r32_mem", "andnl 32(%rax), %r8d, %r8d"),
        ("bextr_bmi1_r64_reg", "bextrq %rcx, %r8, %r8"),
        ("bextr_bmi1_r32_mem", "bextrl %ecx, 32(%rax), %r8d"),
        ("blsi_bmi1_r64_reg", "blsiq %rcx, %r8"),
        ("blsi_bmi1_r32_mem", "blsil 32(%rax), %r8d"),
        ("blsr_bmi1_r64_reg", "blsrq %rcx, %r8"),
        ("blsr_bmi1_r32_mem", "blsrl 32(%rax), %r8d"),
        ("blsmsk_bmi1_r64_reg", "blsmskq %rcx, %r8"),
        ("blsmsk_bmi1_r32_mem", "blsmskl 32(%rax), %r8d"),
        ("tzcnt_bmi1_r64_reg", "tzcntq %rcx, %r8"),
        ("tzcnt_bmi1_r32_mem", "tzcntl 32(%rax), %r8d"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Bmi1,
            profile: Int,
        });
    }

    // BMI2 scalar bit-manipulation forms. MULX observes the newly captured RDX
    // seed as its implicit multiplicand; the other BMI2 forms check flag
    // preservation or BZHI's defined flag subset.
    for &(label, asm) in &[
        ("bzhi_bmi2_r64_reg", "bzhiq %rcx, %r8, %r8"),
        ("bzhi_bmi2_r32_mem", "bzhil %ecx, 32(%rax), %r8d"),
        ("pdep_bmi2_r64_reg", "pdepq %rcx, %r8, %r8"),
        ("pdep_bmi2_r32_mem", "pdepl 32(%rax), %r8d, %r8d"),
        ("pext_bmi2_r64_reg", "pextq %rcx, %r8, %r8"),
        ("pext_bmi2_r32_mem", "pextl 32(%rax), %r8d, %r8d"),
        ("mulx_bmi2_r64_mem", "mulxq 32(%rax), %r8, %rcx"),
        ("mulx_bmi2_r32_reg", "mulxl %r8d, %ecx, %r8d"),
        ("rorx_bmi2_r64_reg", "rorxq $13, %rcx, %r8"),
        ("rorx_bmi2_r32_mem", "rorxl $9, 32(%rax), %r8d"),
        ("sarx_bmi2_r64_reg", "sarxq %rcx, %r8, %r8"),
        ("sarx_bmi2_r32_mem", "sarxl %ecx, 32(%rax), %r8d"),
        ("shrx_bmi2_r64_reg", "shrxq %rcx, %r8, %r8"),
        ("shrx_bmi2_r32_mem", "shrxl %ecx, 32(%rax), %r8d"),
        ("shlx_bmi2_r64_reg", "shlxq %rcx, %r8, %r8"),
        ("shlx_bmi2_r32_mem", "shlxl %ecx, 32(%rax), %r8d"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Bmi2,
            profile: Int,
        });
    }

    // LZCNT is advertised separately from BMI1 on Linux (`abm` on Intel hosts)
    // but shares the same F3 0F legacy count/flag shape as TZCNT.
    for &(label, asm) in &[
        ("lzcnt_r64_reg", "lzcntq %rcx, %r8"),
        ("lzcnt_r32_mem", "lzcntl 32(%rax), %r8d"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Lzcnt,
            profile: Int,
        });
    }

    // High-register variants exercising zmm16-31 across the irregular forms.
    for &(label, asm, feat, profile) in &[
        ("vcvtps2pd_high", "vcvtps2pd %ymm16, %zmm17", F, F32),
        ("vcvtpd2ps_high", "vcvtpd2ps %zmm16, %ymm17", F, F64),
        ("vpmovzxbd_high", "vpmovzxbd %xmm16, %zmm17", F, Int),
        ("vpmovdb_high", "vpmovdb %zmm16, %xmm17", F, Int),
        ("vpbroadcastd_high", "vpbroadcastd %xmm16, %zmm17", F, Int),
        (
            "vpcompressd_high",
            "vpcompressd %zmm16, %zmm17 {%k1}",
            F,
            Int,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile,
        });
    }

    out
}

fn generated_cases() -> Vec<Case> {
    let mut out = Vec::new();
    for base in base_table() {
        expand(&base, &mut out);
    }
    out.extend(irregular_cases());
    out
}

// ---------------------------------------------------------------------------
// Case classification.
//
// Most AVX-512 instructions are exactly defined and must match silicon bit for
// bit (Status::Compare). Two categories cannot be asserted that way:
//   * Approx  — approximate instructions (vrcp14/vrsqrt14...) whose low bits are
//     architecturally implementation-defined; we run them (to catch crashes)
//     but do not bit-compare.
//   * Known   — instructions where rax currently *disagrees* with silicon. These
//     are genuine rax bugs this harness uncovered; they are excluded from the
//     green corpus and asserted (currently failing) by the ignored regression
//     test `avx512_kvm_known_divergences`, so a fix flips that test to passing.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Status {
    Compare,
    Approx,
}

/// First mnemonic token of an AT&T line (after any `{evex}` / `{vex}` pseudo-prefix).
fn asm_mnemonic(asm: &str) -> &str {
    asm.strip_prefix("{evex} ")
        .or_else(|| asm.strip_prefix("{vex} "))
        .unwrap_or(asm)
        .split_whitespace()
        .next()
        .unwrap_or("")
}

fn case_status(case: &Case) -> Status {
    let mnem = asm_mnemonic(&case.asm);

    // Approximate reciprocal / reciprocal-sqrt: Intel specifies only a relative
    // error bound (<= 2^-14 / 2^-28), so the exact result is microarchitecture-
    // specific and not reproducible by a software model.
    if matches!(
        mnem,
        "vrcp14ps"
            | "vrcp14pd"
            | "vrcp14ss"
            | "vrcp14sd"
            | "vrsqrt14ps"
            | "vrsqrt14pd"
            | "vrsqrt14ss"
            | "vrsqrt14sd"
            | "vrcp28ps"
            | "vrcp28pd"
            | "vrsqrt28ps"
            | "vrsqrt28pd"
            | "vrcpph"
            | "vrcpsh"
            | "vrsqrtph"
            | "vrsqrtsh"
            | "vexp2ps"
            | "vexp2pd"
            | "rcpps"
            | "rcpss"
            | "rsqrtps"
            | "rsqrtss"
    ) {
        return Status::Approx;
    }

    // NOTE: the following families USED to diverge from silicon and were
    // tracked here as Status::Known; all are now fixed in the interpreter and
    // are part of the green corpus (verified bit-exact against hardware):
    //   * vcvtudq2ps / vcvtuqq2ps / vcvtudq2pd / vcvtuqq2pd (unsigned int->FP)
    //   * vpmovusdb (unsigned saturating narrow)
    //   * masked vmovaps / vmovapd
    //   * vfmsub213ps / vgetmantps / vreduceps / vrangeps / vgetexpps on
    //     NaN/Inf/denormal edge inputs
    //   * VEX writes zeroing the full ZMM upper (bits 511:256)

    Status::Compare
}

fn case_rflags_mask(case: &Case) -> u64 {
    let mnem = asm_mnemonic(&case.asm);

    // Logical integer ops define CF/PF/ZF/SF/OF and leave AF undefined.
    if matches!(
        mnem,
        "andb"
            | "andw"
            | "andl"
            | "andq"
            | "orb"
            | "orw"
            | "orl"
            | "orq"
            | "xorb"
            | "xorw"
            | "xorl"
            | "xorq"
            | "testb"
            | "testw"
            | "testl"
            | "testq"
    ) {
        return RFLAGS_CF | RFLAGS_PF | RFLAGS_ZF | RFLAGS_SF | RFLAGS_OF;
    }

    // Rotate count > 1 leaves OF undefined; all other status flags are
    // unaffected and still worth comparing.
    if matches!(
        case.label.as_str(),
        "rol_core_r64_imm5" | "ror_core_r64_imm7" | "rol_core_r64_cl" | "ror_core_r64_cl"
    ) {
        return STATUS_RFLAGS_MASK & !RFLAGS_OF;
    }

    // One-bit shifts define CF/OF/SF/ZF/PF and leave AF undefined.
    if matches!(
        case.label.as_str(),
        "shl_core_r64_one" | "shr_core_r64_one" | "sar_core_r64_one"
    ) {
        return RFLAGS_CF | RFLAGS_PF | RFLAGS_ZF | RFLAGS_SF | RFLAGS_OF;
    }

    // Multi-bit shifts and SHLD/SHRD leave AF/OF undefined for the counts used
    // below; CF/SF/ZF/PF remain defined.
    if matches!(
        mnem,
        "shlb"
            | "shlw"
            | "shll"
            | "shlq"
            | "shrb"
            | "shrw"
            | "shrl"
            | "shrq"
            | "salb"
            | "salw"
            | "sall"
            | "salq"
            | "sarb"
            | "sarw"
            | "sarl"
            | "sarq"
            | "shldw"
            | "shldl"
            | "shldq"
            | "shrdw"
            | "shrdl"
            | "shrdq"
    ) {
        return RFLAGS_CF | RFLAGS_PF | RFLAGS_ZF | RFLAGS_SF;
    }

    // MUL/IMUL define CF and OF; the other arithmetic flags are undefined.
    if matches!(
        mnem,
        "mulb" | "mulw" | "mull" | "mulq" | "imulb" | "imulw" | "imull" | "imulq"
    ) {
        return RFLAGS_CF | RFLAGS_OF;
    }

    // DIV/IDIV leave all status flags undefined.
    if matches!(
        mnem,
        "divb" | "divw" | "divl" | "divq" | "idivb" | "idivw" | "idivl" | "idivq"
    ) {
        return 0;
    }

    // BT/BTS/BTR/BTC define CF from the selected bit; the other status flags
    // are architecturally undefined.
    if matches!(
        mnem,
        "btw"
            | "btl"
            | "btq"
            | "btsw"
            | "btsl"
            | "btsq"
            | "btrw"
            | "btrl"
            | "btrq"
            | "btcw"
            | "btcl"
            | "btcq"
    ) {
        return RFLAGS_CF;
    }

    // BSF/BSR define ZF; the destination is only compared for non-zero sources.
    if matches!(mnem, "bsfw" | "bsfl" | "bsfq" | "bsrw" | "bsrl" | "bsrq") {
        return RFLAGS_ZF;
    }

    // BMI1/BZHI define CF/ZF/SF/OF and leave AF/PF undefined.
    if matches!(
        mnem,
        "andnl"
            | "andnq"
            | "blsil"
            | "blsiq"
            | "blsrl"
            | "blsrq"
            | "blsmskl"
            | "blsmskq"
            | "bzhil"
            | "bzhiq"
    ) {
        return RFLAGS_CF | RFLAGS_ZF | RFLAGS_SF | RFLAGS_OF;
    }

    // BEXTR defines ZF and clears CF/OF; SF/AF/PF are undefined.
    if matches!(mnem, "bextrl" | "bextrq") {
        return RFLAGS_CF | RFLAGS_ZF | RFLAGS_OF;
    }

    // TZCNT/LZCNT define CF and ZF; the other arithmetic flags are undefined.
    if matches!(mnem, "tzcntl" | "tzcntq" | "lzcntl" | "lzcntq") {
        return RFLAGS_CF | RFLAGS_ZF;
    }

    // CLD/STD are the only core cases here that define DF; status flags are
    // otherwise unchanged and remain comparable.
    if matches!(mnem, "cld" | "std") {
        return STATUS_RFLAGS_MASK | RFLAGS_DF;
    }

    // STAC/CLAC only change AC; status flags remain comparable.
    if matches!(mnem, "stac" | "clac") {
        return STATUS_RFLAGS_MASK | RFLAGS_AC;
    }

    if case.feat == Feat::FlagControl {
        return STATUS_RFLAGS_MASK | RFLAGS_IF | RFLAGS_DF;
    }

    STATUS_RFLAGS_MASK
}

// ---------------------------------------------------------------------------
// Assembler bridge (llvm-mc), mirroring the EVEX qemu harness.
// ---------------------------------------------------------------------------

const LLVM_MATTR: &str = concat!(
    "+avx512f,+avx512bw,+avx512dq,+avx512cd,+avx512vl,+avx,+avx2,+fma,",
    "+avxvnni,",
    "+avx512ifma,+avx512vnni,+avx512vbmi,+avx512vbmi2,",
    "+avx512bitalg,+avx512vpopcntdq,+avx512bf16,+avx512fp16,",
    "+gfni,+vaes,+vpclmulqdq,+aes,+pclmul,+f16c,+sha,+movdiri,+movdir64b,+adx,+movbe,",
    "+clflushopt,+clwb,+cldemote,+fsgsbase,+wbnoinvd,+smap,+pku,+serialize,+waitpkg,+rdpid,+rdrnd,+rdseed,+sse,+sse2,+ssse3,+sse4.1,+sse4.2,+popcnt,+bmi,+bmi2,+lzcnt"
);

fn which(prog: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(prog))
        .find(|candidate| candidate.is_file())
}

fn llvm_mc_path() -> Option<PathBuf> {
    std::env::var_os("LLVM_MC")
        .map(PathBuf::from)
        .or_else(|| which("llvm-mc"))
}

fn llvm_objcopy_path() -> Option<PathBuf> {
    std::env::var_os("LLVM_OBJCOPY")
        .map(PathBuf::from)
        .or_else(|| which("llvm-objcopy"))
}

fn parse_encoding(text: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("encoding: [") {
        rest = &rest[start + "encoding: [".len()..];
        let end = rest.find(']')?;
        for token in rest[..end].split(',') {
            let token = token.trim();
            let token = token.strip_prefix("0x")?;
            bytes.push(u8::from_str_radix(token, 16).ok()?);
        }
        rest = &rest[end + 1..];
    }
    if bytes.is_empty() { None } else { Some(bytes) }
}

#[test]
fn llvm_mc_parse_concatenates_instruction_encodings() {
    let output = "\
\tje\t.Ltmp0                          # encoding: [0x74,0x08]\n\
\tmovq\t$17, %r8                       # encoding: [0x49,0xc7,0xc0,0x11,0x00,0x00,0x00]\n\
.Ltmp0:\n";

    assert_eq!(
        parse_encoding(output),
        Some(vec![0x74, 0x08, 0x49, 0xc7, 0xc0, 0x11, 0x00, 0x00, 0x00])
    );
}

#[test]
fn llvm_mc_parse_rejects_fixup_placeholders() {
    let output = "\
\tjmp\t.Ltmp0                          # encoding: [0xeb,A]\n\
                                        #   fixup A - offset: 1, value: .Ltmp0-1, kind: FK_PCRel_1\n";

    assert_eq!(parse_encoding(output), None);
}

fn assemble_object_text(llvm_mc: &Path, asm: &str) -> Option<Vec<u8>> {
    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    let objcopy = llvm_objcopy_path()?;
    let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let base =
        std::env::temp_dir().join(format!("rax-avx512-kvm-diff-{}-{id}", std::process::id()));
    let obj_path = base.with_extension("o");
    let bin_path = base.with_extension("bin");

    let result = (|| {
        use std::io::Write;
        let mut child = Command::new(llvm_mc)
            .args([
                "-triple=x86_64",
                "-mcpu=skylake-avx512",
                "-mattr",
                LLVM_MATTR,
                "-x86-asm-syntax=att",
                "--filetype=obj",
                "-o",
            ])
            .arg(&obj_path)
            .stdin(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(format!("{asm}\n").as_bytes())
            .ok()?;
        if !child.wait().ok()?.success() {
            return None;
        }
        if !Command::new(&objcopy)
            .args(["-O", "binary", "--only-section=.text"])
            .arg(&obj_path)
            .arg(&bin_path)
            .stderr(Stdio::null())
            .status()
            .ok()?
            .success()
        {
            return None;
        }
        let bytes = std::fs::read(&bin_path).ok()?;
        if bytes.is_empty() { None } else { Some(bytes) }
    })();

    let _ = std::fs::remove_file(&obj_path);
    let _ = std::fs::remove_file(&bin_path);
    result
}

#[test]
fn llvm_mc_assembles_multiline_fixups_to_text_bytes() {
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc not found");
        return;
    };
    if llvm_objcopy_path().is_none() {
        eprintln!("[skip] llvm-objcopy not found");
        return;
    }

    let bytes = assemble_object_text(&llvm_mc, "jmp 1f\nmovq $0x1111, %r8\n1:\nmovq $0x2222, %r8")
        .expect("assemble multi-instruction snippet");

    assert_eq!(
        bytes,
        vec![
            0xeb, 0x07, 0x49, 0xc7, 0xc0, 0x11, 0x11, 0x00, 0x00, 0x49, 0xc7, 0xc0, 0x22, 0x22,
            0x00, 0x00,
        ]
    );
}

fn assemble(llvm_mc: &Path, asm: &str) -> Option<Vec<u8>> {
    if asm.contains('\n') {
        return assemble_object_text(llvm_mc, asm);
    }

    use std::io::Write;
    let mut child = Command::new(llvm_mc)
        .args([
            "-triple=x86_64",
            "-mcpu=skylake-avx512",
            "-mattr",
            LLVM_MATTR,
            "-x86-asm-syntax=att",
            "-show-encoding",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(format!("{asm}\n").as_bytes())
        .ok()?;
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_encoding(&String::from_utf8_lossy(&output.stdout))
}

fn legacy_0f_encoding(op: &[u8]) -> bool {
    let mut i = 0;
    while matches!(op.get(i), Some(0x66 | 0x67 | 0xf2 | 0xf3)) {
        i += 1;
    }
    if matches!(op.get(i), Some(0x40..=0x4f)) {
        i += 1;
    }
    matches!(op.get(i), Some(0x0f))
}

// ---------------------------------------------------------------------------
// Driver.
// ---------------------------------------------------------------------------

fn oracle() -> Option<&'static KvmOracle> {
    static ORACLE: OnceLock<Option<KvmOracle>> = OnceLock::new();
    ORACLE.get_or_init(KvmOracle::try_new).as_ref()
}

/// Outcome tallies for a corpus run.
#[derive(Default)]
struct Tally {
    compared: usize,
    approx: usize,
    faulted: usize,
    skipped_feature: usize,
    skipped_asm: usize,
    interp_err: usize,
    ran_by_feature: BTreeMap<Feat, usize>,
    ran_by_mnemonic: BTreeMap<String, usize>,
}

impl Tally {
    fn record_run(&mut self, case: &Case) {
        *self.ran_by_feature.entry(case.feat).or_default() += 1;
        *self
            .ran_by_mnemonic
            .entry(asm_mnemonic(&case.asm).to_string())
            .or_default() += 1;
    }

    fn ran_for(&self, feat: Feat) -> usize {
        self.ran_by_feature.get(&feat).copied().unwrap_or(0)
    }

    fn ran_mnemonic(&self, mnemonic: &str) -> usize {
        self.ran_by_mnemonic.get(mnemonic).copied().unwrap_or(0)
    }

    fn feature_summary(&self) -> String {
        self.ran_by_feature
            .iter()
            .map(|(feat, count)| format!("{}={count}", feat.name()))
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// Run a corpus, asserting interp == silicon on every comparable case. Returns
/// the tally so callers can assert on coverage / fault counts. Self-skips
/// cleanly (returning `None`) when KVM / llvm-mc / AVX-512 is unavailable.
fn run_corpus(cases: &[Case]) -> Option<Tally> {
    if !is_x86_feature_detected!("avx512f") {
        eprintln!("[skip] host lacks AVX-512F; nothing to diff against silicon");
        return None;
    }
    let Some(oracle) = oracle() else {
        eprintln!("[skip] /dev/kvm unavailable or AVX-512 XSAVE undrivable");
        return None;
    };
    let Some(llvm_mc) = llvm_mc_path() else {
        eprintln!("[skip] llvm-mc not found; cannot assemble the AVX-512 corpus");
        return None;
    };
    let host = HostFeatures::detect();

    let mut tally = Tally::default();
    let mut failures = Vec::new();

    for case in cases {
        let status = case_status(case);
        if !host.supports(case.feat) {
            tally.skipped_feature += 1;
            continue;
        }
        let Some(op) = assemble(&llvm_mc, &case.asm) else {
            tally.skipped_asm += 1;
            eprintln!("[skip] assemble failed: {} = `{}`", case.label, case.asm);
            continue;
        };
        // EVEX (0x62) for AVX-512 vector ops; AVX-512 opmask and AVX-VNNI ops
        // are VEX-encoded (0xC4/0xC5). Legacy SIMD/scalar extensions and
        // scalar feature probes below are intentionally 0F-family encodings.
        let scalar_encoding = matches!(
            case.feat,
            Feat::Core
                | Feat::Fxsave
                | Feat::Xsave
                | Feat::X87
                | Feat::Mmx
                | Feat::StackFrame
                | Feat::FlagControl
                | Feat::Cx8
                | Feat::Cx16
                | Feat::Fence
                | Feat::Clflush
                | Feat::Clflushopt
                | Feat::Clwb
                | Feat::Cldemote
                | Feat::HintNop
                | Feat::Prefetchw
                | Feat::Monitor
                | Feat::Fsgsbase
                | Feat::ControlReg
                | Feat::DescriptorTable
                | Feat::DescriptorAccess
                | Feat::Msr
                | Feat::DebugReg
                | Feat::Io
                | Feat::FastSyscall
                | Feat::Cpuid
                | Feat::Rdpmc
                | Feat::CacheInvd
                | Feat::Wbnoinvd
                | Feat::Invlpg
                | Feat::Smap
                | Feat::Pku
                | Feat::Swapgs
                | Feat::Serialize
                | Feat::Waitpkg
                | Feat::Rdpid
                | Feat::Rdrand
                | Feat::Rdseed
                | Feat::Tsc
                | Feat::Rdtscp
        ) && !matches!(op.first(), Some(0x62) | Some(0xC4) | Some(0xC5));
        let legacy_allowed = scalar_encoding
            || (matches!(
                case.feat,
                Feat::Aes
                    | Feat::Pclmulqdq
                    | Feat::Gfni
                    | Feat::Sse
                    | Feat::Sse2
                    | Feat::Ssse3
                    | Feat::Sse41
                    | Feat::Sse42
                    | Feat::Sha
                    | Feat::Movdiri
                    | Feat::Movdir64b
                    | Feat::Adx
                    | Feat::Movbe
                    | Feat::Crc32
                    | Feat::Popcnt
                    | Feat::Bmi1
                    | Feat::Lzcnt
            ) && legacy_0f_encoding(&op));
        let expected_encoding =
            matches!(op.first(), Some(0x62) | Some(0xC4) | Some(0xC5)) || legacy_allowed;
        assert!(
            expected_encoding,
            "{}: unexpected encoding class, got {:02x?}",
            case.label, op
        );
        let code = build_code(&op);
        let input = input_for_case(case);

        let kvm = match oracle.run(&code, &input) {
            Ok(KvmOutcome::Ran(out)) => out,
            Ok(KvmOutcome::Faulted) => {
                tally.faulted += 1;
                eprintln!(
                    "[fault] silicon #UD/fault on {} = `{}`",
                    case.label, case.asm
                );
                continue;
            }
            Err(e) => panic!("{}: KVM backend failure: {e}", case.label),
        };
        let interp = match run_interp(&code, &input) {
            Ok(out) => out,
            Err(e) => {
                // rax errored on this form (e.g. an unimplemented encoding).
                // Record and continue so one gap can't mask the rest.
                tally.interp_err += 1;
                eprintln!("[interp-err] {} = `{}`: {e}", case.label, case.asm);
                continue;
            }
        };
        tally.record_run(case);

        // Approximate instructions are exercised but not bit-compared.
        if status == Status::Approx {
            tally.approx += 1;
            continue;
        }

        tally.compared += 1;
        let diffs = diff(&interp, &kvm, case_rflags_mask(case));
        if !diffs.is_empty() {
            failures.push(format!(
                "DIVERGENCE in `{}` ({}) [op={:02x?}]:\n  {}",
                case.label,
                case.asm,
                op,
                diffs.join("\n  ")
            ));
        }
    }

    eprintln!(
        "[avx512-kvm-diff] compared={} approx={} faulted={} interp_err={} \
         skip(feat)={} skip(asm)={} features=[{}]",
        tally.compared,
        tally.approx,
        tally.faulted,
        tally.interp_err,
        tally.skipped_feature,
        tally.skipped_asm,
        tally.feature_summary(),
    );

    assert!(
        failures.is_empty(),
        "{} AVX-512 divergence(s) vs silicon:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    Some(tally)
}

/// Self-validation of the cross-KVM plumbing: with an *empty* instruction under
/// test, both backends start from identical injected state and must end in
/// identical state. This proves the XSAVE/XCR0 ZMM+opmask injection/extraction
/// (and the CPUID-derived component offsets) round-trip bit-exactly — if the
/// layout were wrong, the injected vector state would not survive the no-op.
#[test]
fn avx512_kvm_state_roundtrip() {
    if !is_x86_feature_detected!("avx512f") {
        eprintln!("[skip] host lacks AVX-512F");
        return;
    }
    let Some(oracle) = oracle() else {
        eprintln!("[skip] /dev/kvm unavailable or AVX-512 XSAVE undrivable");
        return;
    };
    let code = build_code(&[]); // mov rax, scratch; hlt  (no op under test)
    for profile in [
        InputProfile::Int,
        InputProfile::F32,
        InputProfile::F64,
        InputProfile::F16,
        InputProfile::F32Edge,
        InputProfile::F64Edge,
    ] {
        let input = input_for(profile);
        let kvm = match oracle.run(&code, &input).expect("kvm run") {
            KvmOutcome::Ran(out) => out,
            KvmOutcome::Faulted => panic!("no-op faulted on silicon"),
        };
        let interp = run_interp(&code, &input).expect("interp run");
        let diffs = diff(&interp, &kvm, STATUS_RFLAGS_MASK);
        assert!(
            diffs.is_empty(),
            "state did not round-trip identically through both backends:\n  {}",
            diffs.join("\n  ")
        );
    }
}

#[test]
fn avx512_kvm_starter_corpus() {
    run_corpus(&starter_cases());
}

#[test]
fn avx512_kvm_privileged_machine_state_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| {
            matches!(
                case.feat,
                Feat::ControlReg | Feat::DescriptorTable | Feat::Msr | Feat::DebugReg
            )
        })
        .collect();
    assert_eq!(cases.len(), 13, "unexpected privileged corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on privileged cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a privileged case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "privileged corpus produced assembler-rejected cases"
    );
    assert_eq!(tally.compared, 13, "all privileged cases should compare");
}

#[test]
fn avx512_kvm_descriptor_access_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::DescriptorAccess)
        .collect();
    assert_eq!(cases.len(), 5, "unexpected descriptor-access corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on descriptor-access cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a descriptor-access case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "descriptor-access corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "descriptor-access cases should not feature-skip"
    );
    assert_eq!(
        tally.compared, 5,
        "all descriptor-access cases should compare"
    );
}

#[test]
fn avx512_kvm_hint_nop_prefetch_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| matches!(case.feat, Feat::HintNop | Feat::Prefetchw))
        .collect();
    assert_eq!(cases.len(), 14, "unexpected hint/prefetch corpus size");

    let host = HostFeatures::detect();
    if !host.supports(Feat::Prefetchw) {
        eprintln!("[skip] host lacks PREFETCHW support; PREFETCHW cases will skip");
    }

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on hint/prefetch cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a hint/prefetch case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "hint/prefetch corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.ran_for(Feat::HintNop),
        11,
        "all NOP/PAUSE/PREFETCHh cases should run"
    );
    if host.supports(Feat::Prefetchw) {
        assert_eq!(
            tally.ran_for(Feat::Prefetchw),
            3,
            "all PREFETCHW cases should run"
        );
        assert_eq!(
            tally.skipped_feature, 0,
            "hint/prefetch cases should not feature-skip"
        );
        assert_eq!(tally.compared, 14, "all hint/prefetch cases should compare");
    } else {
        assert_eq!(
            tally.skipped_feature, 3,
            "only PREFETCHW cases should feature-skip"
        );
        assert_eq!(
            tally.compared, 11,
            "all non-PREFETCHW hint cases should compare"
        );
    }
}

#[test]
fn avx512_kvm_monitor_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Monitor)
        .collect();
    assert_eq!(cases.len(), 4, "unexpected MONITOR corpus size");

    let host = HostFeatures::detect();
    if !host.supports(Feat::Monitor) {
        eprintln!("[skip] host lacks MONITOR support; MONITOR cases will skip");
    }

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    if host.supports(Feat::Monitor) {
        assert_eq!(tally.faulted, 0, "silicon faulted on MONITOR cases");
        assert_eq!(tally.interp_err, 0, "rax failed to execute a MONITOR case");
        assert_eq!(
            tally.skipped_asm, 0,
            "MONITOR corpus produced assembler-rejected cases"
        );
        assert_eq!(
            tally.skipped_feature, 0,
            "MONITOR cases should not feature-skip"
        );
        assert_eq!(tally.compared, 4, "all MONITOR cases should compare");
    } else {
        assert_eq!(
            tally.skipped_feature, 4,
            "all MONITOR cases should feature-skip"
        );
        assert_eq!(tally.compared, 0, "MONITOR cases should not run");
    }
}

#[test]
fn avx512_kvm_io_port_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Io)
        .collect();
    assert_eq!(cases.len(), 22, "unexpected I/O corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on I/O cases");
    assert_eq!(tally.interp_err, 0, "rax failed to execute an I/O case");
    assert_eq!(
        tally.skipped_asm, 0,
        "I/O corpus produced assembler-rejected cases"
    );
    assert_eq!(tally.compared, 22, "all I/O cases should compare");
}

#[test]
fn avx512_kvm_fast_syscall_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::FastSyscall)
        .collect();
    assert_eq!(cases.len(), 4, "unexpected fast syscall corpus size");

    if !HostFeatures::detect().supports(Feat::FastSyscall) {
        eprintln!("[skip] host lacks SYSCALL or SYSENTER support");
        return;
    }

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on fast syscall cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a fast syscall case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "fast syscall corpus produced assembler-rejected cases"
    );
    assert_eq!(tally.compared, 4, "all fast syscall cases should compare");
}

#[test]
fn avx512_kvm_processor_query_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| matches!(case.feat, Feat::Cpuid | Feat::Rdpmc))
        .collect();
    assert_eq!(cases.len(), 8, "unexpected processor-query corpus size");

    let host = HostFeatures::detect();
    if !host.supports(Feat::Rdpmc) {
        eprintln!("[skip] host KVM PMU support unavailable; RDPMC cases will skip");
    }

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on processor-query cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a processor-query case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "processor-query corpus produced assembler-rejected cases"
    );
    assert_eq!(tally.ran_for(Feat::Cpuid), 5, "all CPUID cases should run");
    if host.supports(Feat::Rdpmc) {
        assert_eq!(tally.ran_for(Feat::Rdpmc), 3, "all RDPMC cases should run");
        assert_eq!(
            tally.skipped_feature, 0,
            "processor-query cases should not feature-skip"
        );
        assert_eq!(
            tally.compared, 8,
            "all processor-query cases should compare"
        );
    } else {
        assert_eq!(
            tally.skipped_feature, 3,
            "only RDPMC cases should feature-skip"
        );
        assert_eq!(tally.compared, 5, "all CPUID cases should compare");
    }
}

#[test]
fn avx512_kvm_processor_state_management_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| matches!(case.feat, Feat::Fxsave | Feat::Xsave))
        .collect();
    assert_eq!(
        cases.len(),
        10,
        "unexpected processor state-management corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on processor state-management cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a processor state-management case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "processor state-management corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "processor state-management cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Fxsave),
        6,
        "all FXSAVE/MXCSR cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Xsave),
        4,
        "all XSAVE/XRSTOR cases should run"
    );
    assert_eq!(
        tally.compared, 10,
        "all processor state-management cases should compare"
    );
}

#[test]
fn avx512_kvm_stack_frame_flag_control_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| matches!(case.feat, Feat::StackFrame | Feat::FlagControl))
        .collect();
    assert_eq!(
        cases.len(),
        9,
        "unexpected stack-frame/flag-control corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on stack-frame/flag-control cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a stack-frame/flag-control case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "stack-frame/flag-control corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "stack-frame/flag-control cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::StackFrame),
        4,
        "all stack-frame cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::FlagControl),
        5,
        "all flag-control cases should run"
    );
    assert_eq!(
        tally.compared, 9,
        "all stack-frame/flag-control cases should compare"
    );
}

#[test]
fn avx512_kvm_core_string_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_string"))
        .collect();
    assert_eq!(cases.len(), 54, "unexpected core string corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on core string cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core string case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core string corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core string cases should not feature-skip"
    );
    assert_eq!(tally.compared, 54, "all core string cases should compare");
}

#[test]
fn avx512_kvm_x87_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::X87)
        .collect();
    assert_eq!(cases.len(), 69, "unexpected x87 corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on x87 cases");
    assert_eq!(tally.interp_err, 0, "rax failed to execute an x87 case");
    assert_eq!(
        tally.skipped_asm, 0,
        "x87 corpus produced assembler-rejected cases"
    );
    assert_eq!(tally.compared, 69, "all x87 cases should compare");
}

#[test]
fn avx512_kvm_mmx_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Mmx)
        .collect();
    assert_eq!(cases.len(), 39, "unexpected MMX corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on MMX cases");
    assert_eq!(tally.interp_err, 0, "rax failed to execute an MMX case");
    assert_eq!(
        tally.skipped_asm, 0,
        "MMX corpus produced assembler-rejected cases"
    );
    assert_eq!(tally.compared, 39, "all MMX cases should compare");
}

/// The exhaustive corpus: every host-supported AVX-512 mnemonic family rax
/// implements, expanded across masking / width / memory / broadcast / high
/// registers, diffed bit-exactly against the silicon.
#[test]
fn avx512_kvm_generated_corpus() {
    let cases = generated_cases();
    // Guard against the table silently collapsing to nothing.
    assert!(
        cases.len() > 700,
        "generated corpus unexpectedly small: {} cases",
        cases.len()
    );
    let Some(tally) = run_corpus(&cases) else {
        return; // environment without KVM / llvm-mc — skipped cleanly.
    };
    // Everything we fed the silicon was feature-gated, so nothing should fault,
    // and rax should at least decode every comparable instruction.
    assert_eq!(tally.faulted, 0, "silicon faulted on feature-gated cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a comparable case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "generated corpus produced assembler-rejected cases"
    );
    assert!(
        tally.compared > 800,
        "too few comparable cases actually ran: {}",
        tally.compared
    );
    let host = HostFeatures::detect();
    for &feat in Feat::expanded_xeon() {
        if host.supports(feat) {
            assert!(
                tally.ran_for(feat) > 0,
                "host supports {}, but no KVM corpus case ran for it",
                feat.name()
            );
        }
    }
    let mut expected_mnemonics: BTreeMap<Feat, BTreeSet<String>> = BTreeMap::new();
    for case in &cases {
        if host.supports(case.feat) && Feat::expanded_xeon().contains(&case.feat) {
            expected_mnemonics
                .entry(case.feat)
                .or_default()
                .insert(asm_mnemonic(&case.asm).to_string());
        }
    }
    for (feat, mnemonics) in expected_mnemonics {
        for mnemonic in mnemonics {
            assert!(
                tally.ran_mnemonic(&mnemonic) > 0,
                "host supports {}, but no KVM corpus case ran for mnemonic {mnemonic}",
                feat.name()
            );
        }
    }
}

/// On an AVX-512 host every VEX-encoded SIMD write zeros the destination ZMM
/// above the operation width (VEX.128 zeros bits 511:128, VEX.256 zeros bits
/// 511:256). This regression guards that property — the per-op VEX handlers only
/// clear `ymm_high`, so `dispatch/vex/mod.rs` zeros `zmm_high` for any register a
/// VEX instruction writes. Each case seeds non-zero ZMM-high bits (via the
/// injected state) and checks they are cleared, matching silicon.
#[test]
fn avx512_kvm_vex_zero_upper() {
    let c = |label: &str, asm: &str| Case {
        label: label.to_string(),
        asm: asm.to_string(),
        feat: Feat::F,
        profile: InputProfile::Int,
    };
    run_corpus(&[
        c("vex256_vpaddd", "vpaddd %ymm2, %ymm3, %ymm1"),
        c("vex128_vpaddd", "vpaddd %xmm2, %xmm3, %xmm1"),
        c("vex256_vaddps", "vaddps %ymm2, %ymm3, %ymm1"),
        c("vex128_vmovdqa", "vmovdqa %xmm3, %xmm1"),
    ]);
}
