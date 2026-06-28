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

use common::{
    run_until_hlt, setup_vm, setup_vm_no_idt, Bytes, GuestAddress, GuestMemoryMmap, Registers, VCpu,
};

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
    /// XGETBV with ECX=1 for enabled xstate queries.
    Xgetbv1,
    /// XSAVEOPT/XSAVEC/XSAVES/XRSTORS extended state-management instructions.
    XsaveExt,
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
    /// INVPCID process-context TLB invalidation.
    Invpcid,
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
    /// Legacy SSE3 horizontal, duplicate, and unaligned-load instructions.
    Sse3,
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
            Feat::Xgetbv1 => "xgetbv1",
            Feat::XsaveExt => "xsave_ext",
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
            Feat::Invpcid => "invpcid",
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
            Feat::Sse3 => "sse3",
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
            Feat::Xgetbv1,
            Feat::XsaveExt,
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
            Feat::Invpcid,
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
            Feat::Sse3,
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
    xgetbv1: bool,
    xsaveopt: bool,
    xsavec: bool,
    xsaves: bool,
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
    invpcid: bool,
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
    sse3: bool,
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
            xgetbv1: host_cpu_flag("xgetbv1"),
            xsaveopt: host_cpu_flag("xsaveopt"),
            xsavec: host_cpu_flag("xsavec"),
            xsaves: host_cpu_flag("xsaves"),
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
            invpcid: host_cpu_flag("invpcid"),
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
            sse3: is_x86_feature_detected!("sse3"),
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
            Feat::Xgetbv1 => self.xsave && self.xgetbv1,
            Feat::XsaveExt => self.xsave && self.xsaveopt && self.xsavec && self.xsaves,
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
            Feat::Invpcid => self.invpcid,
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
            Feat::Sse3 => self.sse3,
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
// Keep this away from the shared test helper's GDT page at 0x10000; the
// interpreter is explicitly redirected here so RIP-relative cases match KVM.
const CODE_ADDR: u64 = 0x30000;
const STACK_ADDR: u64 = 0x20000;
const EXCEPTION_GDT_ADDR: u64 = 0x27000;
const EXCEPTION_IDT_ADDR: u64 = 0x28000;
const EXCEPTION_HANDLER_ADDR: u64 = 0x29000;
const EXCEPTION_CODE_SELECTOR: u16 = 0x8;
const DE_VECTOR: usize = 0;
const UD_VECTOR: usize = 6;
const GP_VECTOR: usize = 13;
const EXCEPTION_MARKER_OFFSET: usize = 0xf0;
const FALLTHROUGH_MARKER: u32 = 0xbaad_0000;

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

fn idt_gate64(handler: u64, selector: u16) -> [u8; 16] {
    let mut gate = [0u8; 16];
    gate[0..2].copy_from_slice(&(handler as u16).to_le_bytes());
    gate[2..4].copy_from_slice(&selector.to_le_bytes());
    gate[4] = 0;
    gate[5] = 0x8e;
    gate[6..8].copy_from_slice(&((handler >> 16) as u16).to_le_bytes());
    gate[8..12].copy_from_slice(&((handler >> 32) as u32).to_le_bytes());
    gate
}

const fn exception_marker(vector: usize) -> u32 {
    0x5544_0000 | vector as u32
}

fn store_marker_code(marker: u32) -> [u8; 18] {
    let marker_addr = SCRATCH_ADDR + EXCEPTION_MARKER_OFFSET as u64;
    let mut code = [0u8; 18];
    code[0..2].copy_from_slice(&[0x49, 0xba]); // movabs r10, marker_addr
    code[2..10].copy_from_slice(&marker_addr.to_le_bytes());
    code[10..13].copy_from_slice(&[0x41, 0xc7, 0x02]); // movl marker, (%r10)
    code[13..17].copy_from_slice(&marker.to_le_bytes());
    code[17] = 0xf4;
    code
}

fn install_exception_trap_kvm(mem: &KvmMem, vector: usize) {
    assert!(vector < 256);
    let null_descriptor = [0u8; 8];
    let code64_descriptor = [0x00, 0x00, 0x00, 0x00, 0x00, 0x9a, 0x20, 0x00];
    mem.write(EXCEPTION_GDT_ADDR, &null_descriptor);
    mem.write(EXCEPTION_GDT_ADDR + 8, &code64_descriptor);

    let gate = idt_gate64(EXCEPTION_HANDLER_ADDR, EXCEPTION_CODE_SELECTOR);
    mem.write(EXCEPTION_IDT_ADDR + (vector as u64) * 16, &gate);
    mem.write(EXCEPTION_HANDLER_ADDR, &store_marker_code(exception_marker(vector)));
}

fn install_exception_trap_interp(mem: &GuestMemoryMmap, vector: usize) -> Result<(), String> {
    assert!(vector < 256);
    let gate = idt_gate64(EXCEPTION_HANDLER_ADDR, EXCEPTION_CODE_SELECTOR);
    mem.write_slice(
        &gate,
        GuestAddress(EXCEPTION_IDT_ADDR + (vector as u64) * 16),
    )
    .map_err(|e| format!("write exception IDT gate {vector}: {e:?}"))?;
    mem.write_slice(
        &store_marker_code(exception_marker(vector)),
        GuestAddress(EXCEPTION_HANDLER_ADDR),
    )
    .map_err(|e| format!("write exception handler {vector}: {e:?}"))?;
    Ok(())
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
        self.run_inner(code, input, None)
    }

    fn run_with_ud_trap(&self, code: &[u8], input: &InCase) -> Result<KvmOutcome, String> {
        self.run_with_exception_trap(code, input, UD_VECTOR)
    }

    fn run_with_exception_trap(
        &self,
        code: &[u8],
        input: &InCase,
        vector: usize,
    ) -> Result<KvmOutcome, String> {
        self.run_inner(code, input, Some(vector))
    }

    fn run_inner(
        &self,
        code: &[u8],
        input: &InCase,
        trap_vector: Option<usize>,
    ) -> Result<KvmOutcome, String> {
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
        if let Some(vector) = trap_vector {
            install_exception_trap_kvm(&mem, vector);
        }

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
        if let Some(vector) = trap_vector {
            sregs.gdt.base = EXCEPTION_GDT_ADDR;
            sregs.gdt.limit = 0x0f;
            sregs.idt.base = EXCEPTION_IDT_ADDR;
            sregs.idt.limit = ((vector + 1) * 16 - 1) as u16;
        }
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
    let mut interp_sregs = vcpu
        .get_sregs()
        .map_err(|e| format!("interp get sregs: {e:?}"))?;
    interp_sregs.cr4 = CR4_VAL;
    vcpu.set_sregs(&interp_sregs)
        .map_err(|e| format!("interp set sregs: {e:?}"))?;
    vcpu.set_xgetbv1_value(XCR0_AVX512);
    mem.write_slice(code, GuestAddress(CODE_ADDR))
        .map_err(|e| format!("write code at diff RIP: {e:?}"))?;
    let mut live_regs = vcpu
        .get_regs()
        .map_err(|e| format!("interp get regs: {e:?}"))?;
    live_regs.rip = CODE_ADDR;
    vcpu.set_regs(&live_regs)
        .map_err(|e| format!("interp set RIP: {e:?}"))?;
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

fn run_interp_with_ud_trap(code: &[u8], input: &InCase) -> Result<OutCase, String> {
    run_interp_with_exception_trap(code, input, UD_VECTOR)
}

fn run_interp_with_exception_trap(
    code: &[u8],
    input: &InCase,
    vector: usize,
) -> Result<OutCase, String> {
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

    let (mut vcpu, mem) = setup_vm_no_idt(&[0xf4], Some(regs));
    let mut interp_sregs = vcpu
        .get_sregs()
        .map_err(|e| format!("interp get sregs: {e:?}"))?;
    interp_sregs.cr4 = CR4_VAL;
    interp_sregs.idt.base = EXCEPTION_IDT_ADDR;
    interp_sregs.idt.limit = ((vector + 1) * 16 - 1) as u16;
    vcpu.set_sregs(&interp_sregs)
        .map_err(|e| format!("interp set sregs: {e:?}"))?;
    vcpu.set_xgetbv1_value(XCR0_AVX512);
    install_exception_trap_interp(&mem, vector)?;
    mem.write_slice(code, GuestAddress(CODE_ADDR))
        .map_err(|e| format!("write code at diff RIP: {e:?}"))?;
    let mut live_regs = vcpu
        .get_regs()
        .map_err(|e| format!("interp get regs: {e:?}"))?;
    live_regs.rip = CODE_ADDR;
    vcpu.set_regs(&live_regs)
        .map_err(|e| format!("interp set RIP: {e:?}"))?;
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

fn build_fault_probe_code(op: &[u8]) -> Vec<u8> {
    let mut code = Vec::new();
    code.push(0x48);
    code.push(0xb8);
    code.extend_from_slice(&SCRATCH_ADDR.to_le_bytes());
    code.extend_from_slice(op);
    code.extend_from_slice(&store_marker_code(FALLTHROUGH_MARKER));
    code
}

fn scratch_marker(scratch: &[u8; SCRATCH_BYTES]) -> u32 {
    u32::from_le_bytes(
        scratch[EXCEPTION_MARKER_OFFSET..EXCEPTION_MARKER_OFFSET + 4]
            .try_into()
            .unwrap(),
    )
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
    /// Integer values around signed/unsigned byte, word, and dword saturation
    /// boundaries, laid out as dwords so pack/narrow and saturated arithmetic
    /// all see clamp-sensitive lanes.
    IntSatEdge,
    /// Integer data plus per-lane shift counts around architectural masking and
    /// zero/sign-fill boundaries for word, dword, and qword vector shifts.
    IntShiftEdge,
    /// Integer data for mask-producing/vector-conflict instructions: duplicate
    /// dword/qword lanes, zero/all-ones values, and alternating bit patterns.
    IntPredicateEdge,
    /// Integer data around conversion-sensitive dword/qword boundaries and
    /// precision cliffs for integer-to-FP conversion forms.
    IntConvertEdge,
    F32,
    F64,
    F16,
    /// f32 lanes drawn from a pool of edge values (NaN/Inf/denormal/zeros/signs/
    /// powers of two), so rounding and special-value handling is stressed.
    F32Edge,
    /// f64 analogue.
    F64Edge,
    /// f32 values around signed/unsigned integer conversion boundaries plus
    /// half-way rounding cases and invalid NaN/Inf inputs.
    F32ConvertEdge,
    /// f64 analogue with qword-sized signed/unsigned conversion boundaries.
    F64ConvertEdge,
    /// f32 values that keep sqrt exact-comparable while still stressing zeros,
    /// denormals, infinities, and large finite magnitudes.
    F32SqrtEdge,
    /// f64 analogue.
    F64SqrtEdge,
    /// FP16 values covering zeros, infinities, quiet NaNs, denormals, and
    /// saturated finite magnitudes without relying on signaling-NaN behavior.
    F16Edge,
    /// FP16 values that keep sqrt exact-comparable while still stressing zeros,
    /// denormals, infinities, and large finite magnitudes.
    F16SqrtEdge,
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

const F32_CONVERT_EDGES: [u32; 16] = [
    0x0000_0000, // +0.0
    0x8000_0000, // -0.0
    0x3f00_0000, // +0.5
    0xbf00_0000, // -0.5
    0x3fc0_0000, // +1.5
    0xbfc0_0000, // -1.5
    0x4eff_ffff, // largest f32 below +2^31
    0x4f00_0000, // +2^31
    0xcf00_0000, // -2^31
    0xcf00_0001, // below -2^31
    0x4f7f_ffff, // largest f32 below +2^32
    0x4f80_0000, // +2^32
    0x5f00_0000, // +2^63
    0x5f80_0000, // +2^64
    0x7f80_0000, // +Inf
    0x7fc0_0000, // qNaN
];

const F64_CONVERT_EDGES: [u64; 16] = [
    0x0000_0000_0000_0000, // +0.0
    0xbff0_0000_0000_0000, // -1.0
    0x3fe0_0000_0000_0000, // +0.5
    0xbfe0_0000_0000_0000, // -0.5
    0x3ff8_0000_0000_0000, // +1.5
    0xbff8_0000_0000_0000, // -1.5
    0x41e0_0000_0000_0000, // +2^31
    0x41f0_0000_0000_0000, // +2^32
    0xc1e0_0000_0000_0000, // -2^31
    0x43df_ffff_ffff_ffff, // largest f64 below +2^63
    0x43e0_0000_0000_0000, // +2^63
    0xc3e0_0000_0000_0000, // -2^63
    0x43ef_ffff_ffff_ffff, // largest f64 below +2^64
    0x43f0_0000_0000_0000, // +2^64
    0x7ff0_0000_0000_0000, // +Inf
    0x7ff8_0000_0000_0000, // qNaN
];

const F32_SQRT_EDGES: [u32; 16] = [
    0x0000_0000,
    0x8000_0000,
    0x3f80_0000,
    0x4080_0000,
    0x7f80_0000,
    0x0000_0001,
    0x0080_0000,
    0x7f7f_ffff,
    0x3f00_0000,
    0x4060_0000,
    0x3fb5_04f3,
    0x4b80_0000,
    0x4110_0000,
    0x4180_0000,
    0x4000_0000,
    0x7e7f_ffff,
];

const F64_SQRT_EDGES: [u64; 8] = [
    0x0000_0000_0000_0000,
    0x8000_0000_0000_0000,
    0x3ff0_0000_0000_0000,
    0x4010_0000_0000_0000,
    0x7ff0_0000_0000_0000,
    0x0000_0000_0000_0001,
    0x0010_0000_0000_0000,
    0x7fef_ffff_ffff_ffff,
];

/// Finite, non-zero half-precision values. Keeping the FP16 corpus away from
/// NaN/Inf/zero denominators makes bit-exact silicon comparison meaningful.
const F16_VALUES: [u16; 16] = [
    0x3c00, 0x4000, 0x4200, 0x4400, 0x3800, 0x3e00, 0xbc00, 0xc000, 0x3555, 0x3a00, 0x4100, 0x4480,
    0x4600, 0x4800, 0x4900, 0x4a00,
];

const F16_EDGES: [u16; 16] = [
    0x0000, 0x8000, 0x3c00, 0xbc00, 0x7c00, 0xfc00, 0x7e00, 0x0001, 0x0400, 0x7bff, 0x3800, 0x4300,
    0x4000, 0xc000, 0x3555, 0x7d00,
];

const F16_SQRT_EDGES: [u16; 16] = [
    0x0000, 0x8000, 0x3c00, 0x4400, 0x7c00, 0x0001, 0x0002, 0x0400, 0x7bff, 0x3800, 0x4300, 0x4000,
    0x4880, 0x4c00, 0x3555, 0x7b00,
];

const INT_SAT_EDGES: [u32; 16] = [
    0x0000_0000,
    0x0000_0001,
    0x0000_007f,
    0x0000_0080,
    0xffff_ff80,
    0xffff_ff7f,
    0x0000_00ff,
    0x0000_0100,
    0xffff_8000,
    0xffff_7fff,
    0x0000_7fff,
    0x0000_8000,
    0x0000_ffff,
    0x0001_0000,
    0x7fff_ffff,
    0x8000_0000,
];

const INT_SHIFT_COUNTS: [u32; 16] = [0, 1, 7, 8, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 255];
const INT_PREDICATE_EDGE_BASE: [u32; 16] = [
    0x0000_0000,
    0x0000_0000,
    0xffff_ffff,
    0xffff_ffff,
    0x8000_0000,
    0x0000_0000,
    0x8000_0000,
    0x0000_0000,
    0x0000_0001,
    0x0000_0000,
    0x0000_0001,
    0x0000_0000,
    0xaaaa_aaaa,
    0x5555_5555,
    0xaaaa_aaaa,
    0x5555_5555,
];
const INT_PREDICATE_EDGE_TESTER: [u32; 16] = [
    0xffff_ffff,
    0x0000_0000,
    0x0000_0000,
    0xffff_ffff,
    0x7fff_ffff,
    0xffff_ffff,
    0x0000_0001,
    0x8000_0000,
    0x00ff_00ff,
    0xff00_ff00,
    0xffff_0000,
    0x0000_ffff,
    0xaaaa_aaaa,
    0xaaaa_aaaa,
    0x5555_5555,
    0x5555_5555,
];
const INT_PREDICATE_EDGE_SCRATCH: [u32; 16] = [
    0x0000_0000,
    0x0000_0000,
    0x0000_0000,
    0x0000_0000,
    0xffff_ffff,
    0xffff_ffff,
    0xffff_ffff,
    0xffff_ffff,
    0x8000_0000,
    0x0000_0000,
    0x8000_0000,
    0x0000_0000,
    0xaaaa_aaaa,
    0x5555_5555,
    0xaaaa_aaaa,
    0x5555_5555,
];
const INT_CONVERT_EDGE_QWORDS: [u64; 8] = [
    0x0000_0001_0000_0000,
    0xffff_ffff_7fff_ffff,
    0x8000_0000_0000_0000,
    0x0000_0000_ffff_ffff,
    0x7fff_ffff_ffff_ffff,
    0x8000_0000_0000_0001,
    0xffff_ffff_ffff_ffff,
    0x0020_0000_0000_0001,
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

fn int_sat_edge_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..16 {
        let value = INT_SAT_EDGES[(reg * 7 + lane) % INT_SAT_EDGES.len()];
        bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn int_shift_edge_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    let values = if matches!(reg, 2 | 31) {
        &INT_SHIFT_COUNTS
    } else {
        &INT_SAT_EDGES
    };
    for lane in 0..16 {
        let value = values[(reg * 7 + lane) % values.len()];
        bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn int_predicate_edge_zmm(reg: usize) -> [u8; 64] {
    let values = match reg {
        2 => INT_PREDICATE_EDGE_BASE,
        3 => INT_PREDICATE_EDGE_TESTER,
        31 => INT_PREDICATE_EDGE_SCRATCH,
        _ => {
            let mut values = [0u32; 16];
            for lane in 0..16 {
                values[lane] = INT_PREDICATE_EDGE_BASE[(lane + reg * 3) % 16];
            }
            values
        }
    };
    let mut bytes = [0u8; 64];
    for (lane, value) in values.iter().enumerate() {
        bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn int_convert_edge_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..8 {
        let value = INT_CONVERT_EDGE_QWORDS[(reg * 3 + lane) % INT_CONVERT_EDGE_QWORDS.len()];
        bytes[lane * 8..lane * 8 + 8].copy_from_slice(&value.to_le_bytes());
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

fn f16_edge_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..32 {
        let value = F16_EDGES[(reg * 5 + lane) % F16_EDGES.len()];
        bytes[lane * 2..lane * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn f16_sqrt_edge_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..32 {
        let value = F16_SQRT_EDGES[(reg * 5 + lane) % F16_SQRT_EDGES.len()];
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

fn f32_convert_edge_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..16 {
        let value = F32_CONVERT_EDGES[(reg * 5 + lane) % F32_CONVERT_EDGES.len()];
        bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn f64_convert_edge_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..8 {
        let value = F64_CONVERT_EDGES[(reg * 3 + lane) % F64_CONVERT_EDGES.len()];
        bytes[lane * 8..lane * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn f32_sqrt_edge_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..16 {
        let value = F32_SQRT_EDGES[(reg * 5 + lane) % F32_SQRT_EDGES.len()];
        bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn f64_sqrt_edge_zmm(reg: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for lane in 0..8 {
        let value = F64_SQRT_EDGES[(reg * 3 + lane) % F64_SQRT_EDGES.len()];
        bytes[lane * 8..lane * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn profile_zmm(profile: InputProfile, reg: usize) -> [u8; 64] {
    match profile {
        InputProfile::Int => int_zmm(reg),
        InputProfile::IntSatEdge => int_sat_edge_zmm(reg),
        InputProfile::IntShiftEdge => int_shift_edge_zmm(reg),
        InputProfile::IntPredicateEdge => int_predicate_edge_zmm(reg),
        InputProfile::IntConvertEdge => int_convert_edge_zmm(reg),
        InputProfile::F32 => f32_zmm(reg),
        InputProfile::F64 => f64_zmm(reg),
        InputProfile::F16 => f16_zmm(reg),
        InputProfile::F32Edge => f32_edge_zmm(reg),
        InputProfile::F64Edge => f64_edge_zmm(reg),
        InputProfile::F32ConvertEdge => f32_convert_edge_zmm(reg),
        InputProfile::F64ConvertEdge => f64_convert_edge_zmm(reg),
        InputProfile::F32SqrtEdge => f32_sqrt_edge_zmm(reg),
        InputProfile::F64SqrtEdge => f64_sqrt_edge_zmm(reg),
        InputProfile::F16Edge => f16_edge_zmm(reg),
        InputProfile::F16SqrtEdge => f16_sqrt_edge_zmm(reg),
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
    if case.label.contains("_stack_frame_edge_") {
        input.rbp = STACK_ADDR + 0x30;
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
    if case.label.contains("count_one") {
        input.rcx = 1;
    }
    if case.label.contains("_df") {
        input.rsi = input.rsi.wrapping_add(STRING_DF_OFFSET);
        input.rdi = input.rdi.wrapping_add(STRING_DF_OFFSET);
        input.rflags |= RFLAGS_DF;
    }
    if case.label.contains("_core_string_edge_same_address") {
        input.rdi = input.rsi;
    }
    if case.label.contains("_core_string_edge_overlap_forward") {
        input.rsi = SCRATCH_ADDR + 64;
        input.rdi = SCRATCH_ADDR + 65;
    }
    if case.label.contains("_core_string_edge_overlap_backward") {
        input.rsi = SCRATCH_ADDR + 68;
        input.rdi = SCRATCH_ADDR + 69;
        input.rflags |= RFLAGS_DF;
    }
    if case.label.contains("_core_string_edge_addr32_high") {
        input.rsi = 0xffff_0000_0000_4080;
        input.rdi = 0xffff_0000_0000_4020;
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

    for &(label, asm, profile) in &[
        ("vdpbf16ps_bf16_edge_reg", "vdpbf16ps %zmm2, %zmm3, %zmm1", F32Edge),
        (
            "vdpbf16ps_bf16_edge_mem",
            "vdpbf16ps 64(%rax), %zmm3, %zmm1",
            F32Edge,
        ),
        (
            "vdpbf16ps_bf16_edge_merge",
            "vdpbf16ps %zmm2, %zmm3, %zmm1 {%k1}",
            F32Edge,
        ),
        (
            "vdpbf16ps_bf16_edge_zero",
            "vdpbf16ps %zmm2, %zmm3, %zmm1 {%k1}{z}",
            F32Edge,
        ),
        (
            "vcvtneps2bf16_bf16_edge_reg",
            "vcvtneps2bf16 %zmm3, %ymm1",
            F32Edge,
        ),
        (
            "vcvtneps2bf16_bf16_edge_mem",
            "vcvtneps2bf16 64(%rax), %ymm1",
            F32Edge,
        ),
        (
            "vcvtneps2bf16_bf16_edge_merge",
            "vcvtneps2bf16 %zmm3, %ymm1 {%k1}",
            F32Edge,
        ),
        (
            "vcvtneps2bf16_bf16_edge_zero",
            "vcvtneps2bf16 %zmm3, %ymm1 {%k1}{z}",
            F32Edge,
        ),
        (
            "vcvtne2ps2bf16_bf16_edge_reg",
            "vcvtne2ps2bf16 %zmm3, %zmm2, %zmm1",
            F32Edge,
        ),
        (
            "vcvtne2ps2bf16_bf16_edge_mem",
            "vcvtne2ps2bf16 64(%rax), %zmm2, %zmm1",
            F32Edge,
        ),
        (
            "vcvtne2ps2bf16_bf16_edge_merge",
            "vcvtne2ps2bf16 %zmm3, %zmm2, %zmm1 {%k1}",
            F32Edge,
        ),
        (
            "vcvtne2ps2bf16_bf16_edge_zero",
            "vcvtne2ps2bf16 %zmm3, %zmm2, %zmm1 {%k1}{z}",
            F32Edge,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Bf16,
            profile,
        });
    }

    for &(label, asm, profile) in &[
        ("vminph_fp16_edge_minmax_reg", "vminph %zmm2, %zmm3, %zmm1", F16Edge),
        ("vminph_fp16_edge_minmax_mem", "vminph 64(%rax), %zmm3, %zmm1", F16Edge),
        ("vmaxph_fp16_edge_minmax_reg", "vmaxph %zmm2, %zmm3, %zmm1", F16Edge),
        ("vmaxph_fp16_edge_minmax_mem", "vmaxph 64(%rax), %zmm3, %zmm1", F16Edge),
        ("vminsh_fp16_edge_minmax_reg", "vminsh %xmm2, %xmm3, %xmm1", F16Edge),
        ("vminsh_fp16_edge_minmax_mem", "vminsh 22(%rax), %xmm3, %xmm1", F16Edge),
        ("vmaxsh_fp16_edge_minmax_reg", "vmaxsh %xmm2, %xmm3, %xmm1", F16Edge),
        ("vmaxsh_fp16_edge_minmax_mem", "vmaxsh 22(%rax), %xmm3, %xmm1", F16Edge),
        ("vsqrtph_fp16_edge_sqrt_reg", "vsqrtph %zmm3, %zmm1", F16SqrtEdge),
        ("vsqrtph_fp16_edge_sqrt_mem", "vsqrtph 64(%rax), %zmm1", F16SqrtEdge),
        ("vsqrtsh_fp16_edge_sqrt_reg", "vsqrtsh %xmm2, %xmm3, %xmm1", F16SqrtEdge),
        ("vsqrtsh_fp16_edge_sqrt_mem", "vsqrtsh 32(%rax), %xmm3, %xmm1", F16SqrtEdge),
        ("vcomish_fp16_edge_compare_qnan_mem", "vcomish 22(%rax), %xmm1", F16Edge),
        ("vucomish_fp16_edge_compare_qnan_mem", "vucomish 22(%rax), %xmm1", F16Edge),
        ("vcmpph_fp16_edge_compare_unord_reg", "vcmpph $0x03, %zmm2, %zmm3, %k5", F16Edge),
        ("vcmpph_fp16_edge_compare_ord_mem", "vcmpph $0x07, 64(%rax), %zmm3, %k5", F16Edge),
        ("vcmpsh_fp16_edge_compare_unord_reg", "vcmpsh $0x03, %xmm2, %xmm3, %k5", F16Edge),
        ("vcmpsh_fp16_edge_compare_ord_mem", "vcmpsh $0x07, 22(%rax), %xmm3, %k5", F16Edge),
        ("vfpclassph_fp16_edge_class_reg", "vfpclassph $0x7f, %zmm3, %k5", F16Edge),
        ("vfpclassph_fp16_edge_class_merge", "vfpclassph $0x7f, %zmm3, %k5 {%k1}", F16Edge),
        ("vfpclasssh_fp16_edge_class_reg", "vfpclasssh $0x7f, %xmm2, %k5", F16Edge),
        ("vfpclasssh_fp16_edge_class_mem", "vfpclasssh $0x7f, 22(%rax), %k5", F16Edge),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Fp16,
            profile,
        });
    }

    for &(label, asm, feat, profile) in &[
        (
            "vpconflictd_avx512_predicate_edge_reg",
            "vpconflictd %zmm2, %zmm1",
            Cd,
            IntPredicateEdge,
        ),
        (
            "vpconflictd_avx512_predicate_edge_mem",
            "vpconflictd 64(%rax), %zmm1",
            Cd,
            IntPredicateEdge,
        ),
        (
            "vpconflictq_avx512_predicate_edge_reg",
            "vpconflictq %zmm2, %zmm1",
            Cd,
            IntPredicateEdge,
        ),
        (
            "vpconflictq_avx512_predicate_edge_mem",
            "vpconflictq 64(%rax), %zmm1",
            Cd,
            IntPredicateEdge,
        ),
        (
            "vplzcntd_avx512_predicate_edge_reg",
            "vplzcntd %zmm2, %zmm1",
            Cd,
            IntPredicateEdge,
        ),
        (
            "vplzcntd_avx512_predicate_edge_mem",
            "vplzcntd 64(%rax), %zmm1",
            Cd,
            IntPredicateEdge,
        ),
        (
            "vplzcntq_avx512_predicate_edge_reg",
            "vplzcntq %zmm2, %zmm1",
            Cd,
            IntPredicateEdge,
        ),
        (
            "vplzcntq_avx512_predicate_edge_mem",
            "vplzcntq 64(%rax), %zmm1",
            Cd,
            IntPredicateEdge,
        ),
        (
            "vptestmd_avx512_predicate_edge_reg",
            "vptestmd %zmm2, %zmm3, %k5",
            F,
            IntPredicateEdge,
        ),
        (
            "vptestnmd_avx512_predicate_edge_mem",
            "vptestnmd 64(%rax), %zmm3, %k5",
            F,
            IntPredicateEdge,
        ),
        (
            "vptestmq_avx512_predicate_edge_reg",
            "vptestmq %zmm2, %zmm3, %k5",
            F,
            IntPredicateEdge,
        ),
        (
            "vptestnmq_avx512_predicate_edge_mem",
            "vptestnmq 64(%rax), %zmm3, %k5",
            F,
            IntPredicateEdge,
        ),
        (
            "vptestmb_avx512_predicate_edge_reg",
            "vptestmb %zmm2, %zmm3, %k5",
            Bw,
            IntPredicateEdge,
        ),
        (
            "vptestnmb_avx512_predicate_edge_mem",
            "vptestnmb 64(%rax), %zmm3, %k5",
            Bw,
            IntPredicateEdge,
        ),
        (
            "vptestmw_avx512_predicate_edge_reg",
            "vptestmw %zmm2, %zmm3, %k5",
            Bw,
            IntPredicateEdge,
        ),
        (
            "vptestnmw_avx512_predicate_edge_mem",
            "vptestnmw 64(%rax), %zmm3, %k5",
            Bw,
            IntPredicateEdge,
        ),
        (
            "vfpclassps_avx512_predicate_edge_reg",
            "vfpclassps $0x7f, %zmm3, %k5",
            Dq,
            F32Edge,
        ),
        (
            "vfpclassps_avx512_predicate_edge_mem_bcst",
            "vfpclassps $0x7f, 44(%rax){1to16}, %k5",
            Dq,
            F32Edge,
        ),
        (
            "vfpclasspd_avx512_predicate_edge_reg",
            "vfpclasspd $0x7f, %zmm3, %k5",
            Dq,
            F64Edge,
        ),
        (
            "vfpclasspd_avx512_predicate_edge_mem_bcst",
            "vfpclasspd $0x7f, 8(%rax){1to8}, %k5",
            Dq,
            F64Edge,
        ),
        (
            "vfpclassss_avx512_predicate_edge_reg",
            "vfpclassss $0x7f, %xmm2, %k5",
            Dq,
            F32Edge,
        ),
        (
            "vfpclassss_avx512_predicate_edge_mem",
            "vfpclassss $0x7f, 44(%rax), %k5",
            Dq,
            F32Edge,
        ),
        (
            "vfpclasssd_avx512_predicate_edge_reg",
            "vfpclasssd $0x7f, %xmm2, %k5",
            Dq,
            F64Edge,
        ),
        (
            "vfpclasssd_avx512_predicate_edge_mem",
            "vfpclasssd $0x7f, 8(%rax), %k5",
            Dq,
            F64Edge,
        ),
        (
            "vreduceps_avx512_predicate_edge_imm_max",
            "vreduceps $0x0f, %zmm3, %zmm1",
            Dq,
            F32,
        ),
        (
            "vreducepd_avx512_predicate_edge_imm_max",
            "vreducepd $0x0f, %zmm3, %zmm1",
            Dq,
            F64,
        ),
        (
            "vreducess_avx512_predicate_edge_imm_max",
            "vreducess $0x0f, %xmm2, %xmm3, %xmm1",
            Dq,
            F32,
        ),
        (
            "vreducesd_avx512_predicate_edge_imm_max",
            "vreducesd $0x0f, %xmm2, %xmm3, %xmm1",
            Dq,
            F64,
        ),
        (
            "vrangeps_avx512_predicate_edge_imm_max",
            "vrangeps $0x0f, %zmm2, %zmm3, %zmm1",
            Dq,
            F32,
        ),
        (
            "vrangepd_avx512_predicate_edge_imm_max",
            "vrangepd $0x0f, %zmm2, %zmm3, %zmm1",
            Dq,
            F64,
        ),
        (
            "vrangess_avx512_predicate_edge_imm_max",
            "vrangess $0x0f, %xmm2, %xmm3, %xmm1",
            Dq,
            F32,
        ),
        (
            "vrangesd_avx512_predicate_edge_imm_max",
            "vrangesd $0x0f, %xmm2, %xmm3, %xmm1",
            Dq,
            F64,
        ),
        (
            "vfixupimmps_avx512_predicate_edge_imm_ff",
            "vfixupimmps $0xff, %zmm2, %zmm3, %zmm1",
            F,
            F32,
        ),
        (
            "vfixupimmpd_avx512_predicate_edge_imm_ff",
            "vfixupimmpd $0xff, %zmm2, %zmm3, %zmm1",
            F,
            F64,
        ),
        (
            "vfixupimmss_avx512_predicate_edge_imm_ff",
            "vfixupimmss $0xff, %xmm2, %xmm3, %xmm1",
            F,
            F32,
        ),
        (
            "vfixupimmsd_avx512_predicate_edge_imm_ff",
            "vfixupimmsd $0xff, %xmm2, %xmm3, %xmm1",
            F,
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

    // AVX-512 sparse memory movement forms. Compress stores compact selected
    // elements to memory, expand loads compact memory into masked lanes, and
    // gather/scatter clear their opmask after one safe VSIB lane.
    for &(label, asm, feat, profile) in &[
        (
            "vpcompressd_avx512_sparse_mem_one",
            "kxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvpcompressd %zmm2, 64(%rax) {%k1}",
            F,
            Int,
        ),
        (
            "vpcompressq_avx512_sparse_mem_one",
            "kxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvpcompressq %zmm2, 64(%rax) {%k1}",
            F,
            Int,
        ),
        (
            "vcompressps_avx512_sparse_mem_one",
            "kxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvcompressps %zmm2, 64(%rax) {%k1}",
            F,
            F32,
        ),
        (
            "vcompresspd_avx512_sparse_mem_one",
            "kxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvcompresspd %zmm2, 64(%rax) {%k1}",
            F,
            F64,
        ),
        (
            "vpexpandd_avx512_sparse_mem_one_merge",
            "kxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvpexpandd 64(%rax), %zmm1 {%k1}",
            F,
            Int,
        ),
        (
            "vpexpandd_avx512_sparse_mem_one_zero",
            "kxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvpexpandd 64(%rax), %zmm1 {%k1}{z}",
            F,
            Int,
        ),
        (
            "vpexpandq_avx512_sparse_mem_one_merge",
            "kxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvpexpandq 64(%rax), %zmm1 {%k1}",
            F,
            Int,
        ),
        (
            "vpexpandq_avx512_sparse_mem_one_zero",
            "kxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvpexpandq 64(%rax), %zmm1 {%k1}{z}",
            F,
            Int,
        ),
        (
            "vexpandps_avx512_sparse_mem_one_merge",
            "kxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvexpandps 64(%rax), %zmm1 {%k1}",
            F,
            F32,
        ),
        (
            "vexpandps_avx512_sparse_mem_one_zero",
            "kxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvexpandps 64(%rax), %zmm1 {%k1}{z}",
            F,
            F32,
        ),
        (
            "vexpandpd_avx512_sparse_mem_one_merge",
            "kxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvexpandpd 64(%rax), %zmm1 {%k1}",
            F,
            F64,
        ),
        (
            "vexpandpd_avx512_sparse_mem_one_zero",
            "kxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvexpandpd 64(%rax), %zmm1 {%k1}{z}",
            F,
            F64,
        ),
        (
            "vgatherdps_avx512_sparse_mem_one",
            "vpxord %zmm2, %zmm2, %zmm2\nkxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvgatherdps 64(%rax,%zmm2,4), %zmm1 {%k1}",
            F,
            F32,
        ),
        (
            "vgatherdpd_avx512_sparse_mem_one",
            "vpxord %zmm2, %zmm2, %zmm2\nkxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvgatherdpd 64(%rax,%ymm2,8), %zmm1 {%k1}",
            F,
            F64,
        ),
        (
            "vgatherqps_avx512_sparse_mem_one",
            "vpxord %zmm2, %zmm2, %zmm2\nkxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvgatherqps 64(%rax,%zmm2,4), %ymm1 {%k1}",
            F,
            F32,
        ),
        (
            "vgatherqpd_avx512_sparse_mem_one",
            "vpxord %zmm2, %zmm2, %zmm2\nkxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvgatherqpd 64(%rax,%zmm2,8), %zmm1 {%k1}",
            F,
            F64,
        ),
        (
            "vpgatherdd_avx512_sparse_mem_one",
            "vpxord %zmm2, %zmm2, %zmm2\nkxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvpgatherdd 64(%rax,%zmm2,4), %zmm1 {%k1}",
            F,
            Int,
        ),
        (
            "vpgatherdq_avx512_sparse_mem_one",
            "vpxord %zmm2, %zmm2, %zmm2\nkxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvpgatherdq 64(%rax,%ymm2,8), %zmm1 {%k1}",
            F,
            Int,
        ),
        (
            "vpgatherqd_avx512_sparse_mem_one",
            "vpxord %zmm2, %zmm2, %zmm2\nkxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvpgatherqd 64(%rax,%zmm2,4), %ymm1 {%k1}",
            F,
            Int,
        ),
        (
            "vpgatherqq_avx512_sparse_mem_one",
            "vpxord %zmm2, %zmm2, %zmm2\nkxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvpgatherqq 64(%rax,%zmm2,8), %zmm1 {%k1}",
            F,
            Int,
        ),
        (
            "vscatterdps_avx512_sparse_mem_one",
            "vpxord %zmm2, %zmm2, %zmm2\nkxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvscatterdps %zmm3, 64(%rax,%zmm2,4) {%k1}",
            F,
            F32,
        ),
        (
            "vscatterdpd_avx512_sparse_mem_one",
            "vpxord %zmm2, %zmm2, %zmm2\nkxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvscatterdpd %zmm3, 64(%rax,%ymm2,8) {%k1}",
            F,
            F64,
        ),
        (
            "vscatterqps_avx512_sparse_mem_one",
            "vpxord %zmm2, %zmm2, %zmm2\nkxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvscatterqps %ymm3, 64(%rax,%zmm2,4) {%k1}",
            F,
            F32,
        ),
        (
            "vscatterqpd_avx512_sparse_mem_one",
            "vpxord %zmm2, %zmm2, %zmm2\nkxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvscatterqpd %zmm3, 64(%rax,%zmm2,8) {%k1}",
            F,
            F64,
        ),
        (
            "vpscatterdd_avx512_sparse_mem_one",
            "vpxord %zmm2, %zmm2, %zmm2\nkxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvpscatterdd %zmm3, 64(%rax,%zmm2,4) {%k1}",
            F,
            Int,
        ),
        (
            "vpscatterdq_avx512_sparse_mem_one",
            "vpxord %zmm2, %zmm2, %zmm2\nkxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvpscatterdq %zmm3, 64(%rax,%ymm2,8) {%k1}",
            F,
            Int,
        ),
        (
            "vpscatterqd_avx512_sparse_mem_one",
            "vpxord %zmm2, %zmm2, %zmm2\nkxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvpscatterqd %ymm3, 64(%rax,%zmm2,4) {%k1}",
            F,
            Int,
        ),
        (
            "vpscatterqq_avx512_sparse_mem_one",
            "vpxord %zmm2, %zmm2, %zmm2\nkxnorw %k1, %k1, %k1\nkshiftrw $15, %k1, %k1\nvpscatterqq %zmm3, 64(%rax,%zmm2,8) {%k1}",
            F,
            Int,
        ),
        (
            "vpcompressb_vbmi2_sparse_mem_one",
            "kxnorq %k1, %k1, %k1\nkshiftrq $63, %k1, %k1\nvpcompressb %zmm2, 64(%rax) {%k1}",
            Vbmi2,
            Int,
        ),
        (
            "vpcompressw_vbmi2_sparse_mem_one",
            "kxnorq %k1, %k1, %k1\nkshiftrq $63, %k1, %k1\nvpcompressw %zmm2, 64(%rax) {%k1}",
            Vbmi2,
            Int,
        ),
        (
            "vpexpandb_vbmi2_sparse_mem_one_merge",
            "kxnorq %k1, %k1, %k1\nkshiftrq $63, %k1, %k1\nvpexpandb 64(%rax), %zmm1 {%k1}",
            Vbmi2,
            Int,
        ),
        (
            "vpexpandb_vbmi2_sparse_mem_one_zero",
            "kxnorq %k1, %k1, %k1\nkshiftrq $63, %k1, %k1\nvpexpandb 64(%rax), %zmm1 {%k1}{z}",
            Vbmi2,
            Int,
        ),
        (
            "vpexpandw_vbmi2_sparse_mem_one_merge",
            "kxnorq %k1, %k1, %k1\nkshiftrq $63, %k1, %k1\nvpexpandw 64(%rax), %zmm1 {%k1}",
            Vbmi2,
            Int,
        ),
        (
            "vpexpandw_vbmi2_sparse_mem_one_zero",
            "kxnorq %k1, %k1, %k1\nkshiftrq $63, %k1, %k1\nvpexpandw 64(%rax), %zmm1 {%k1}{z}",
            Vbmi2,
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

    for &(label, asm, feat, profile) in &[
        (
            "vpcmpd_avx512_cmp_edge_eq_reg",
            "vpcmpd $0, %zmm2, %zmm3, %k5",
            F,
            IntSatEdge,
        ),
        (
            "vpcmpd_avx512_cmp_edge_signed_lt_mem_merge",
            "vpcmpd $1, (%rax), %zmm3, %k5 {%k1}",
            F,
            IntSatEdge,
        ),
        (
            "vpcmpud_avx512_cmp_edge_unsigned_le_reg",
            "vpcmpud $2, %zmm2, %zmm3, %k5",
            F,
            IntSatEdge,
        ),
        (
            "vpcmpud_avx512_cmp_edge_unsigned_nle_mem_merge",
            "vpcmpud $6, (%rax), %zmm3, %k5 {%k1}",
            F,
            IntSatEdge,
        ),
        (
            "vpcmpq_avx512_cmp_edge_signed_lt_reg",
            "vpcmpq $1, %zmm2, %zmm3, %k5",
            F,
            IntSatEdge,
        ),
        (
            "vpcmpq_avx512_cmp_edge_signed_nlt_mem_merge",
            "vpcmpq $5, (%rax), %zmm3, %k5 {%k1}",
            F,
            IntSatEdge,
        ),
        (
            "vpcmpuq_avx512_cmp_edge_unsigned_le_reg",
            "vpcmpuq $2, %zmm2, %zmm3, %k5",
            F,
            IntSatEdge,
        ),
        (
            "vpcmpuq_avx512_cmp_edge_unsigned_nle_mem_merge",
            "vpcmpuq $6, (%rax), %zmm3, %k5 {%k1}",
            F,
            IntSatEdge,
        ),
        (
            "vpcmpb_avx512_cmp_edge_signed_lt_reg",
            "vpcmpb $1, %zmm2, %zmm3, %k5",
            Bw,
            IntSatEdge,
        ),
        (
            "vpcmpb_avx512_cmp_edge_signed_nlt_mem_merge",
            "vpcmpb $5, (%rax), %zmm3, %k5 {%k1}",
            Bw,
            IntSatEdge,
        ),
        (
            "vpcmpub_avx512_cmp_edge_unsigned_le_reg",
            "vpcmpub $2, %zmm2, %zmm3, %k5",
            Bw,
            IntSatEdge,
        ),
        (
            "vpcmpub_avx512_cmp_edge_unsigned_nle_mem_merge",
            "vpcmpub $6, (%rax), %zmm3, %k5 {%k1}",
            Bw,
            IntSatEdge,
        ),
        (
            "vpcmpw_avx512_cmp_edge_signed_lt_reg",
            "vpcmpw $1, %zmm2, %zmm3, %k5",
            Bw,
            IntSatEdge,
        ),
        (
            "vpcmpw_avx512_cmp_edge_signed_nlt_mem_merge",
            "vpcmpw $5, (%rax), %zmm3, %k5 {%k1}",
            Bw,
            IntSatEdge,
        ),
        (
            "vpcmpuw_avx512_cmp_edge_unsigned_le_reg",
            "vpcmpuw $2, %zmm2, %zmm3, %k5",
            Bw,
            IntSatEdge,
        ),
        (
            "vpcmpuw_avx512_cmp_edge_unsigned_nle_mem_merge",
            "vpcmpuw $6, (%rax), %zmm3, %k5 {%k1}",
            Bw,
            IntSatEdge,
        ),
        (
            "vcmpps_avx512_cmp_edge_eq_reg",
            "vcmpps $0x00, %zmm2, %zmm3, %k5",
            F,
            F32Edge,
        ),
        (
            "vcmpps_avx512_cmp_edge_unord_reg",
            "vcmpps $0x03, %zmm2, %zmm3, %k5",
            F,
            F32Edge,
        ),
        (
            "vcmpps_avx512_cmp_edge_gt_oq_reg",
            "vcmpps $0x1e, %zmm2, %zmm3, %k5",
            F,
            F32Edge,
        ),
        (
            "vcmpps_avx512_cmp_edge_ord_mem_merge",
            "vcmpps $0x07, (%rax), %zmm3, %k5 {%k1}",
            F,
            F32Edge,
        ),
        (
            "vcmppd_avx512_cmp_edge_neq_reg",
            "vcmppd $0x04, %zmm2, %zmm3, %k5",
            F,
            F64Edge,
        ),
        (
            "vcmppd_avx512_cmp_edge_unord_reg",
            "vcmppd $0x03, %zmm2, %zmm3, %k5",
            F,
            F64Edge,
        ),
        (
            "vcmppd_avx512_cmp_edge_gt_oq_reg",
            "vcmppd $0x1e, %zmm2, %zmm3, %k5",
            F,
            F64Edge,
        ),
        (
            "vcmppd_avx512_cmp_edge_ord_mem_merge",
            "vcmppd $0x07, (%rax), %zmm3, %k5 {%k1}",
            F,
            F64Edge,
        ),
        (
            "vcmpss_avx512_cmp_edge_unord_reg",
            "vcmpss $0x03, %xmm2, %xmm3, %k5",
            F,
            F32Edge,
        ),
        (
            "vcmpss_avx512_cmp_edge_ord_qnan_mem_merge",
            "vcmpss $0x07, 44(%rax), %xmm3, %k5 {%k1}",
            F,
            F32Edge,
        ),
        (
            "vcmpss_avx512_cmp_edge_true_reg",
            "vcmpss $0x1f, %xmm2, %xmm3, %k5",
            F,
            F32Edge,
        ),
        (
            "vcmpss_avx512_cmp_edge_gt_oq_reg",
            "vcmpss $0x1e, %xmm2, %xmm3, %k5",
            F,
            F32Edge,
        ),
        (
            "vcmpsd_avx512_cmp_edge_unord_reg",
            "vcmpsd $0x03, %xmm2, %xmm3, %k5",
            F,
            F64Edge,
        ),
        (
            "vcmpsd_avx512_cmp_edge_ord_qnan_mem_merge",
            "vcmpsd $0x07, 8(%rax), %xmm3, %k5 {%k1}",
            F,
            F64Edge,
        ),
        (
            "vcmpsd_avx512_cmp_edge_true_reg",
            "vcmpsd $0x1f, %xmm2, %xmm3, %k5",
            F,
            F64Edge,
        ),
        (
            "vcmpsd_avx512_cmp_edge_gt_oq_reg",
            "vcmpsd $0x1e, %xmm2, %xmm3, %k5",
            F,
            F64Edge,
        ),
        (
            "vcmpph_avx512_cmp_edge_eq_reg",
            "vcmpph $0x00, %zmm2, %zmm3, %k5",
            Fp16,
            F16Edge,
        ),
        (
            "vcmpph_avx512_cmp_edge_unord_reg",
            "vcmpph $0x03, %zmm2, %zmm3, %k5",
            Fp16,
            F16Edge,
        ),
        (
            "vcmpph_avx512_cmp_edge_ord_mem_merge",
            "vcmpph $0x07, 64(%rax), %zmm3, %k5 {%k1}",
            Fp16,
            F16Edge,
        ),
        (
            "vcmpph_avx512_cmp_edge_gt_reg",
            "vcmpph $0x1e, %zmm2, %zmm3, %k5",
            Fp16,
            F16Edge,
        ),
        (
            "vcmpsh_avx512_cmp_edge_unord_reg",
            "vcmpsh $0x03, %xmm2, %xmm3, %k5",
            Fp16,
            F16Edge,
        ),
        (
            "vcmpsh_avx512_cmp_edge_ord_qnan_mem_merge",
            "vcmpsh $0x07, 22(%rax), %xmm3, %k5 {%k1}",
            Fp16,
            F16Edge,
        ),
        (
            "vcmpsh_avx512_cmp_edge_gt_reg",
            "vcmpsh $0x1e, %xmm2, %xmm3, %k5",
            Fp16,
            F16Edge,
        ),
        (
            "vcmpsh_avx512_cmp_edge_true_reg",
            "vcmpsh $0x1f, %xmm2, %xmm3, %k5",
            Fp16,
            F16Edge,
        ),
    ] {
        push_compare(label.to_string(), asm.to_string(), feat, profile);
    }

    for &(label, asm, feat, profile) in &[
        (
            "vcvtps2dq_avx512_convert_edge_reg",
            "vcvtps2dq %zmm3, %zmm1",
            F,
            F32ConvertEdge,
        ),
        (
            "vcvtps2dq_avx512_convert_edge_mem",
            "vcvtps2dq 32(%rax), %zmm1",
            F,
            F32ConvertEdge,
        ),
        (
            "vcvtps2dq_avx512_convert_edge_rd_sae",
            "vcvtps2dq {rd-sae}, %zmm3, %zmm1",
            F,
            F32ConvertEdge,
        ),
        (
            "vcvtps2dq_avx512_convert_edge_ru_sae",
            "vcvtps2dq {ru-sae}, %zmm3, %zmm1",
            F,
            F32ConvertEdge,
        ),
        (
            "vcvttps2dq_avx512_convert_edge_sae",
            "vcvttps2dq {sae}, %zmm3, %zmm1",
            F,
            F32ConvertEdge,
        ),
        (
            "vcvtps2udq_avx512_convert_edge_ru_sae",
            "vcvtps2udq {ru-sae}, %zmm3, %zmm1",
            F,
            F32ConvertEdge,
        ),
        (
            "vcvttps2udq_avx512_convert_edge_sae",
            "vcvttps2udq {sae}, %zmm3, %zmm1",
            F,
            F32ConvertEdge,
        ),
        (
            "vcvtpd2dq_avx512_convert_edge_reg",
            "vcvtpd2dq %zmm3, %ymm1",
            F,
            F64ConvertEdge,
        ),
        (
            "vcvtpd2dq_avx512_convert_edge_mem",
            "vcvtpd2dq 32(%rax), %ymm1",
            F,
            F64ConvertEdge,
        ),
        (
            "vcvtpd2dq_avx512_convert_edge_rd_sae",
            "vcvtpd2dq {rd-sae}, %zmm3, %ymm1",
            F,
            F64ConvertEdge,
        ),
        (
            "vcvttpd2dq_avx512_convert_edge_sae",
            "vcvttpd2dq {sae}, %zmm3, %ymm1",
            F,
            F64ConvertEdge,
        ),
        (
            "vcvtpd2udq_avx512_convert_edge_ru_sae",
            "vcvtpd2udq {ru-sae}, %zmm3, %ymm1",
            F,
            F64ConvertEdge,
        ),
        (
            "vcvttpd2udq_avx512_convert_edge_sae",
            "vcvttpd2udq {sae}, %zmm3, %ymm1",
            F,
            F64ConvertEdge,
        ),
        (
            "vcvtps2qq_avx512_convert_edge_reg",
            "vcvtps2qq %ymm3, %zmm1",
            Dq,
            F32ConvertEdge,
        ),
        (
            "vcvtps2qq_avx512_convert_edge_mem",
            "vcvtps2qq 32(%rax), %zmm1",
            Dq,
            F32ConvertEdge,
        ),
        (
            "vcvtps2qq_avx512_convert_edge_rd_sae",
            "vcvtps2qq {rd-sae}, %ymm3, %zmm1",
            Dq,
            F32ConvertEdge,
        ),
        (
            "vcvttps2qq_avx512_convert_edge_sae",
            "vcvttps2qq {sae}, %ymm3, %zmm1",
            Dq,
            F32ConvertEdge,
        ),
        (
            "vcvtps2uqq_avx512_convert_edge_ru_sae",
            "vcvtps2uqq {ru-sae}, %ymm3, %zmm1",
            Dq,
            F32ConvertEdge,
        ),
        (
            "vcvttps2uqq_avx512_convert_edge_sae",
            "vcvttps2uqq {sae}, %ymm3, %zmm1",
            Dq,
            F32ConvertEdge,
        ),
        (
            "vcvtpd2qq_avx512_convert_edge_reg",
            "vcvtpd2qq %zmm3, %zmm1",
            Dq,
            F64ConvertEdge,
        ),
        (
            "vcvtpd2qq_avx512_convert_edge_mem",
            "vcvtpd2qq 32(%rax), %zmm1",
            Dq,
            F64ConvertEdge,
        ),
        (
            "vcvtpd2qq_avx512_convert_edge_rd_sae",
            "vcvtpd2qq {rd-sae}, %zmm3, %zmm1",
            Dq,
            F64ConvertEdge,
        ),
        (
            "vcvttpd2qq_avx512_convert_edge_sae",
            "vcvttpd2qq {sae}, %zmm3, %zmm1",
            Dq,
            F64ConvertEdge,
        ),
        (
            "vcvtpd2uqq_avx512_convert_edge_ru_sae",
            "vcvtpd2uqq {ru-sae}, %zmm3, %zmm1",
            Dq,
            F64ConvertEdge,
        ),
        (
            "vcvttpd2uqq_avx512_convert_edge_sae",
            "vcvttpd2uqq {sae}, %zmm3, %zmm1",
            Dq,
            F64ConvertEdge,
        ),
        (
            "vcvtdq2ps_avx512_convert_edge_reg",
            "vcvtdq2ps %zmm3, %zmm1",
            F,
            IntConvertEdge,
        ),
        (
            "vcvtdq2ps_avx512_convert_edge_mem",
            "vcvtdq2ps 32(%rax), %zmm1",
            F,
            IntConvertEdge,
        ),
        (
            "vcvtdq2ps_avx512_convert_edge_rd_sae",
            "vcvtdq2ps {rd-sae}, %zmm3, %zmm1",
            F,
            IntConvertEdge,
        ),
        (
            "vcvtudq2ps_avx512_convert_edge_ru_sae",
            "vcvtudq2ps {ru-sae}, %zmm3, %zmm1",
            F,
            IntConvertEdge,
        ),
        (
            "vcvtdq2pd_avx512_convert_edge_reg",
            "vcvtdq2pd %ymm3, %zmm1",
            F,
            IntConvertEdge,
        ),
        (
            "vcvtudq2pd_avx512_convert_edge_reg",
            "vcvtudq2pd %ymm3, %zmm1",
            F,
            IntConvertEdge,
        ),
        (
            "vcvtqq2pd_avx512_convert_edge_reg",
            "vcvtqq2pd %zmm3, %zmm1",
            Dq,
            IntConvertEdge,
        ),
        (
            "vcvtqq2pd_avx512_convert_edge_mem",
            "vcvtqq2pd 32(%rax), %zmm1",
            Dq,
            IntConvertEdge,
        ),
        (
            "vcvtuqq2pd_avx512_convert_edge_reg",
            "vcvtuqq2pd %zmm3, %zmm1",
            Dq,
            IntConvertEdge,
        ),
        (
            "vcvtqq2pd_avx512_convert_edge_rd_sae",
            "vcvtqq2pd {rd-sae}, %zmm3, %zmm1",
            Dq,
            IntConvertEdge,
        ),
        (
            "vcvtuqq2pd_avx512_convert_edge_ru_sae",
            "vcvtuqq2pd {ru-sae}, %zmm3, %zmm1",
            Dq,
            IntConvertEdge,
        ),
        (
            "vcvtqq2ps_avx512_convert_edge_reg",
            "vcvtqq2ps %zmm3, %ymm1",
            Dq,
            IntConvertEdge,
        ),
        (
            "vcvtqq2ps_avx512_convert_edge_mem",
            "vcvtqq2ps 32(%rax), %ymm1",
            Dq,
            IntConvertEdge,
        ),
        (
            "vcvtuqq2ps_avx512_convert_edge_reg",
            "vcvtuqq2ps %zmm3, %ymm1",
            Dq,
            IntConvertEdge,
        ),
        (
            "vcvtqq2ps_avx512_convert_edge_rd_sae",
            "vcvtqq2ps {rd-sae}, %zmm3, %ymm1",
            Dq,
            IntConvertEdge,
        ),
        (
            "vcvtuqq2ps_avx512_convert_edge_ru_sae",
            "vcvtuqq2ps {ru-sae}, %zmm3, %ymm1",
            Dq,
            IntConvertEdge,
        ),
        (
            "vcvtss2si_avx512_convert_edge_rd_sae",
            "vcvtss2si {rd-sae}, %xmm3, %r8",
            F,
            F32ConvertEdge,
        ),
        (
            "vcvttss2si_avx512_convert_edge_sae",
            "vcvttss2si {sae}, %xmm3, %r8",
            F,
            F32ConvertEdge,
        ),
        (
            "vcvtss2usi_avx512_convert_edge_ru_sae",
            "vcvtss2usi {ru-sae}, %xmm3, %r8",
            F,
            F32ConvertEdge,
        ),
        (
            "vcvttss2usi_avx512_convert_edge_sae",
            "vcvttss2usi {sae}, %xmm3, %r8",
            F,
            F32ConvertEdge,
        ),
        (
            "vcvtsd2si_avx512_convert_edge_rd_sae",
            "vcvtsd2si {rd-sae}, %xmm3, %r8",
            F,
            F64ConvertEdge,
        ),
        (
            "vcvttsd2si_avx512_convert_edge_sae",
            "vcvttsd2si {sae}, %xmm3, %r8",
            F,
            F64ConvertEdge,
        ),
        (
            "vcvtsd2usi_avx512_convert_edge_ru_sae",
            "vcvtsd2usi {ru-sae}, %xmm3, %r8",
            F,
            F64ConvertEdge,
        ),
        (
            "vcvttsd2usi_avx512_convert_edge_sae",
            "vcvttsd2usi {sae}, %xmm3, %r8",
            F,
            F64ConvertEdge,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile,
        });
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

    for &(label, asm, feat, profile) in &[
        ("minps_sse_minmax_edge_reg", "minps %xmm2, %xmm1", Sse, F32Edge),
        ("minps_sse_minmax_edge_mem", "minps 32(%rax), %xmm1", Sse, F32Edge),
        ("minss_sse_minmax_edge_reg", "minss %xmm2, %xmm1", Sse, F32Edge),
        ("minss_sse_minmax_edge_mem", "minss 32(%rax), %xmm1", Sse, F32Edge),
        ("maxps_sse_minmax_edge_reg", "maxps %xmm2, %xmm1", Sse, F32Edge),
        ("maxps_sse_minmax_edge_mem", "maxps 32(%rax), %xmm1", Sse, F32Edge),
        ("maxss_sse_minmax_edge_reg", "maxss %xmm2, %xmm1", Sse, F32Edge),
        ("maxss_sse_minmax_edge_mem", "maxss 32(%rax), %xmm1", Sse, F32Edge),
        ("minpd_sse_minmax_edge_reg", "minpd %xmm2, %xmm1", Sse2, F64Edge),
        ("minpd_sse_minmax_edge_mem", "minpd 32(%rax), %xmm1", Sse2, F64Edge),
        ("minsd_sse_minmax_edge_reg", "minsd %xmm2, %xmm1", Sse2, F64Edge),
        ("minsd_sse_minmax_edge_mem", "minsd 32(%rax), %xmm1", Sse2, F64Edge),
        ("maxpd_sse_minmax_edge_reg", "maxpd %xmm2, %xmm1", Sse2, F64Edge),
        ("maxpd_sse_minmax_edge_mem", "maxpd 32(%rax), %xmm1", Sse2, F64Edge),
        ("maxsd_sse_minmax_edge_reg", "maxsd %xmm2, %xmm1", Sse2, F64Edge),
        ("maxsd_sse_minmax_edge_mem", "maxsd 32(%rax), %xmm1", Sse2, F64Edge),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile,
        });
    }

    // Add/sub/mul/div with edge FP values. These complement the base arithmetic
    // smoke cases by stressing NaNs, infinities, signed zeros, and denormals
    // across legacy destructive SSE, VEX three-operand AVX, EVEX AVX-512, and
    // AVX-512-FP16 forms.
    for &(label, asm, feat, profile) in &[
        ("addps_sse_simd_fp_arith_edge_reg", "addps %xmm2, %xmm1", Sse, F32Edge),
        ("addps_sse_simd_fp_arith_edge_mem", "addps 32(%rax), %xmm1", Sse, F32Edge),
        ("addss_sse_simd_fp_arith_edge_reg", "addss %xmm2, %xmm1", Sse, F32Edge),
        ("addss_sse_simd_fp_arith_edge_mem", "addss 32(%rax), %xmm1", Sse, F32Edge),
        ("subps_sse_simd_fp_arith_edge_reg", "subps %xmm2, %xmm1", Sse, F32Edge),
        ("subps_sse_simd_fp_arith_edge_mem", "subps 32(%rax), %xmm1", Sse, F32Edge),
        ("subss_sse_simd_fp_arith_edge_reg", "subss %xmm2, %xmm1", Sse, F32Edge),
        ("subss_sse_simd_fp_arith_edge_mem", "subss 32(%rax), %xmm1", Sse, F32Edge),
        ("mulps_sse_simd_fp_arith_edge_reg", "mulps %xmm2, %xmm1", Sse, F32Edge),
        ("mulps_sse_simd_fp_arith_edge_mem", "mulps 32(%rax), %xmm1", Sse, F32Edge),
        ("mulss_sse_simd_fp_arith_edge_reg", "mulss %xmm2, %xmm1", Sse, F32Edge),
        ("mulss_sse_simd_fp_arith_edge_mem", "mulss 32(%rax), %xmm1", Sse, F32Edge),
        ("divps_sse_simd_fp_arith_edge_reg", "divps %xmm2, %xmm1", Sse, F32Edge),
        ("divps_sse_simd_fp_arith_edge_mem", "divps 32(%rax), %xmm1", Sse, F32Edge),
        ("divss_sse_simd_fp_arith_edge_reg", "divss %xmm2, %xmm1", Sse, F32Edge),
        ("divss_sse_simd_fp_arith_edge_mem", "divss 32(%rax), %xmm1", Sse, F32Edge),
        ("addpd_sse2_simd_fp_arith_edge_reg", "addpd %xmm2, %xmm1", Sse2, F64Edge),
        ("addpd_sse2_simd_fp_arith_edge_mem", "addpd 32(%rax), %xmm1", Sse2, F64Edge),
        ("addsd_sse2_simd_fp_arith_edge_reg", "addsd %xmm2, %xmm1", Sse2, F64Edge),
        ("addsd_sse2_simd_fp_arith_edge_mem", "addsd 32(%rax), %xmm1", Sse2, F64Edge),
        ("subpd_sse2_simd_fp_arith_edge_reg", "subpd %xmm2, %xmm1", Sse2, F64Edge),
        ("subpd_sse2_simd_fp_arith_edge_mem", "subpd 32(%rax), %xmm1", Sse2, F64Edge),
        ("subsd_sse2_simd_fp_arith_edge_reg", "subsd %xmm2, %xmm1", Sse2, F64Edge),
        ("subsd_sse2_simd_fp_arith_edge_mem", "subsd 32(%rax), %xmm1", Sse2, F64Edge),
        ("mulpd_sse2_simd_fp_arith_edge_reg", "mulpd %xmm2, %xmm1", Sse2, F64Edge),
        ("mulpd_sse2_simd_fp_arith_edge_mem", "mulpd 32(%rax), %xmm1", Sse2, F64Edge),
        ("mulsd_sse2_simd_fp_arith_edge_reg", "mulsd %xmm2, %xmm1", Sse2, F64Edge),
        ("mulsd_sse2_simd_fp_arith_edge_mem", "mulsd 32(%rax), %xmm1", Sse2, F64Edge),
        ("divpd_sse2_simd_fp_arith_edge_reg", "divpd %xmm2, %xmm1", Sse2, F64Edge),
        ("divpd_sse2_simd_fp_arith_edge_mem", "divpd 32(%rax), %xmm1", Sse2, F64Edge),
        ("divsd_sse2_simd_fp_arith_edge_reg", "divsd %xmm2, %xmm1", Sse2, F64Edge),
        ("divsd_sse2_simd_fp_arith_edge_mem", "divsd 32(%rax), %xmm1", Sse2, F64Edge),
        ("vaddps_avx_simd_fp_arith_edge_reg", "{vex} vaddps %ymm2, %ymm3, %ymm1", Avx, F32Edge),
        ("vaddps_avx_simd_fp_arith_edge_mem", "{vex} vaddps 64(%rax), %ymm3, %ymm1", Avx, F32Edge),
        ("vaddss_avx_simd_fp_arith_edge_reg", "{vex} vaddss %xmm2, %xmm3, %xmm1", Avx, F32Edge),
        ("vaddss_avx_simd_fp_arith_edge_mem", "{vex} vaddss 32(%rax), %xmm3, %xmm1", Avx, F32Edge),
        ("vsubps_avx_simd_fp_arith_edge_reg", "{vex} vsubps %ymm2, %ymm3, %ymm1", Avx, F32Edge),
        ("vsubps_avx_simd_fp_arith_edge_mem", "{vex} vsubps 64(%rax), %ymm3, %ymm1", Avx, F32Edge),
        ("vsubss_avx_simd_fp_arith_edge_reg", "{vex} vsubss %xmm2, %xmm3, %xmm1", Avx, F32Edge),
        ("vsubss_avx_simd_fp_arith_edge_mem", "{vex} vsubss 32(%rax), %xmm3, %xmm1", Avx, F32Edge),
        ("vmulps_avx_simd_fp_arith_edge_reg", "{vex} vmulps %ymm2, %ymm3, %ymm1", Avx, F32Edge),
        ("vmulps_avx_simd_fp_arith_edge_mem", "{vex} vmulps 64(%rax), %ymm3, %ymm1", Avx, F32Edge),
        ("vmulss_avx_simd_fp_arith_edge_reg", "{vex} vmulss %xmm2, %xmm3, %xmm1", Avx, F32Edge),
        ("vmulss_avx_simd_fp_arith_edge_mem", "{vex} vmulss 32(%rax), %xmm3, %xmm1", Avx, F32Edge),
        ("vdivps_avx_simd_fp_arith_edge_reg", "{vex} vdivps %ymm2, %ymm3, %ymm1", Avx, F32Edge),
        ("vdivps_avx_simd_fp_arith_edge_mem", "{vex} vdivps 64(%rax), %ymm3, %ymm1", Avx, F32Edge),
        ("vdivss_avx_simd_fp_arith_edge_reg", "{vex} vdivss %xmm2, %xmm3, %xmm1", Avx, F32Edge),
        ("vdivss_avx_simd_fp_arith_edge_mem", "{vex} vdivss 32(%rax), %xmm3, %xmm1", Avx, F32Edge),
        ("vaddpd_avx_simd_fp_arith_edge_reg", "{vex} vaddpd %ymm2, %ymm3, %ymm1", Avx, F64Edge),
        ("vaddpd_avx_simd_fp_arith_edge_mem", "{vex} vaddpd 64(%rax), %ymm3, %ymm1", Avx, F64Edge),
        ("vaddsd_avx_simd_fp_arith_edge_reg", "{vex} vaddsd %xmm2, %xmm3, %xmm1", Avx, F64Edge),
        ("vaddsd_avx_simd_fp_arith_edge_mem", "{vex} vaddsd 32(%rax), %xmm3, %xmm1", Avx, F64Edge),
        ("vsubpd_avx_simd_fp_arith_edge_reg", "{vex} vsubpd %ymm2, %ymm3, %ymm1", Avx, F64Edge),
        ("vsubpd_avx_simd_fp_arith_edge_mem", "{vex} vsubpd 64(%rax), %ymm3, %ymm1", Avx, F64Edge),
        ("vsubsd_avx_simd_fp_arith_edge_reg", "{vex} vsubsd %xmm2, %xmm3, %xmm1", Avx, F64Edge),
        ("vsubsd_avx_simd_fp_arith_edge_mem", "{vex} vsubsd 32(%rax), %xmm3, %xmm1", Avx, F64Edge),
        ("vmulpd_avx_simd_fp_arith_edge_reg", "{vex} vmulpd %ymm2, %ymm3, %ymm1", Avx, F64Edge),
        ("vmulpd_avx_simd_fp_arith_edge_mem", "{vex} vmulpd 64(%rax), %ymm3, %ymm1", Avx, F64Edge),
        ("vmulsd_avx_simd_fp_arith_edge_reg", "{vex} vmulsd %xmm2, %xmm3, %xmm1", Avx, F64Edge),
        ("vmulsd_avx_simd_fp_arith_edge_mem", "{vex} vmulsd 32(%rax), %xmm3, %xmm1", Avx, F64Edge),
        ("vdivpd_avx_simd_fp_arith_edge_reg", "{vex} vdivpd %ymm2, %ymm3, %ymm1", Avx, F64Edge),
        ("vdivpd_avx_simd_fp_arith_edge_mem", "{vex} vdivpd 64(%rax), %ymm3, %ymm1", Avx, F64Edge),
        ("vdivsd_avx_simd_fp_arith_edge_reg", "{vex} vdivsd %xmm2, %xmm3, %xmm1", Avx, F64Edge),
        ("vdivsd_avx_simd_fp_arith_edge_mem", "{vex} vdivsd 32(%rax), %xmm3, %xmm1", Avx, F64Edge),
        ("vaddps_avx512_simd_fp_arith_edge_reg", "{evex} vaddps %zmm2, %zmm3, %zmm1", F, F32Edge),
        ("vaddps_avx512_simd_fp_arith_edge_mem", "{evex} vaddps 64(%rax), %zmm3, %zmm1", F, F32Edge),
        ("vaddss_avx512_simd_fp_arith_edge_reg", "{evex} vaddss %xmm2, %xmm3, %xmm1", F, F32Edge),
        ("vaddss_avx512_simd_fp_arith_edge_mem", "{evex} vaddss 32(%rax), %xmm3, %xmm1", F, F32Edge),
        ("vsubps_avx512_simd_fp_arith_edge_reg", "{evex} vsubps %zmm2, %zmm3, %zmm1", F, F32Edge),
        ("vsubps_avx512_simd_fp_arith_edge_mem", "{evex} vsubps 64(%rax), %zmm3, %zmm1", F, F32Edge),
        ("vsubss_avx512_simd_fp_arith_edge_reg", "{evex} vsubss %xmm2, %xmm3, %xmm1", F, F32Edge),
        ("vsubss_avx512_simd_fp_arith_edge_mem", "{evex} vsubss 32(%rax), %xmm3, %xmm1", F, F32Edge),
        ("vmulps_avx512_simd_fp_arith_edge_reg", "{evex} vmulps %zmm2, %zmm3, %zmm1", F, F32Edge),
        ("vmulps_avx512_simd_fp_arith_edge_mem", "{evex} vmulps 64(%rax), %zmm3, %zmm1", F, F32Edge),
        ("vmulss_avx512_simd_fp_arith_edge_reg", "{evex} vmulss %xmm2, %xmm3, %xmm1", F, F32Edge),
        ("vmulss_avx512_simd_fp_arith_edge_mem", "{evex} vmulss 32(%rax), %xmm3, %xmm1", F, F32Edge),
        ("vdivps_avx512_simd_fp_arith_edge_reg", "{evex} vdivps %zmm2, %zmm3, %zmm1", F, F32Edge),
        ("vdivps_avx512_simd_fp_arith_edge_mem", "{evex} vdivps 64(%rax), %zmm3, %zmm1", F, F32Edge),
        ("vdivss_avx512_simd_fp_arith_edge_reg", "{evex} vdivss %xmm2, %xmm3, %xmm1", F, F32Edge),
        ("vdivss_avx512_simd_fp_arith_edge_mem", "{evex} vdivss 32(%rax), %xmm3, %xmm1", F, F32Edge),
        ("vaddpd_avx512_simd_fp_arith_edge_reg", "{evex} vaddpd %zmm2, %zmm3, %zmm1", F, F64Edge),
        ("vaddpd_avx512_simd_fp_arith_edge_mem", "{evex} vaddpd 64(%rax), %zmm3, %zmm1", F, F64Edge),
        ("vaddsd_avx512_simd_fp_arith_edge_reg", "{evex} vaddsd %xmm2, %xmm3, %xmm1", F, F64Edge),
        ("vaddsd_avx512_simd_fp_arith_edge_mem", "{evex} vaddsd 32(%rax), %xmm3, %xmm1", F, F64Edge),
        ("vsubpd_avx512_simd_fp_arith_edge_reg", "{evex} vsubpd %zmm2, %zmm3, %zmm1", F, F64Edge),
        ("vsubpd_avx512_simd_fp_arith_edge_mem", "{evex} vsubpd 64(%rax), %zmm3, %zmm1", F, F64Edge),
        ("vsubsd_avx512_simd_fp_arith_edge_reg", "{evex} vsubsd %xmm2, %xmm3, %xmm1", F, F64Edge),
        ("vsubsd_avx512_simd_fp_arith_edge_mem", "{evex} vsubsd 32(%rax), %xmm3, %xmm1", F, F64Edge),
        ("vmulpd_avx512_simd_fp_arith_edge_reg", "{evex} vmulpd %zmm2, %zmm3, %zmm1", F, F64Edge),
        ("vmulpd_avx512_simd_fp_arith_edge_mem", "{evex} vmulpd 64(%rax), %zmm3, %zmm1", F, F64Edge),
        ("vmulsd_avx512_simd_fp_arith_edge_reg", "{evex} vmulsd %xmm2, %xmm3, %xmm1", F, F64Edge),
        ("vmulsd_avx512_simd_fp_arith_edge_mem", "{evex} vmulsd 32(%rax), %xmm3, %xmm1", F, F64Edge),
        ("vdivpd_avx512_simd_fp_arith_edge_reg", "{evex} vdivpd %zmm2, %zmm3, %zmm1", F, F64Edge),
        ("vdivpd_avx512_simd_fp_arith_edge_mem", "{evex} vdivpd 64(%rax), %zmm3, %zmm1", F, F64Edge),
        ("vdivsd_avx512_simd_fp_arith_edge_reg", "{evex} vdivsd %xmm2, %xmm3, %xmm1", F, F64Edge),
        ("vdivsd_avx512_simd_fp_arith_edge_mem", "{evex} vdivsd 32(%rax), %xmm3, %xmm1", F, F64Edge),
        ("vaddph_fp16_simd_fp_arith_edge_reg", "vaddph %zmm2, %zmm3, %zmm1", Fp16, F16Edge),
        ("vaddph_fp16_simd_fp_arith_edge_mem", "vaddph 64(%rax), %zmm3, %zmm1", Fp16, F16Edge),
        ("vaddsh_fp16_simd_fp_arith_edge_reg", "vaddsh %xmm2, %xmm3, %xmm1", Fp16, F16Edge),
        ("vaddsh_fp16_simd_fp_arith_edge_mem", "vaddsh 22(%rax), %xmm3, %xmm1", Fp16, F16Edge),
        ("vsubph_fp16_simd_fp_arith_edge_reg", "vsubph %zmm2, %zmm3, %zmm1", Fp16, F16Edge),
        ("vsubph_fp16_simd_fp_arith_edge_mem", "vsubph 64(%rax), %zmm3, %zmm1", Fp16, F16Edge),
        ("vsubsh_fp16_simd_fp_arith_edge_reg", "vsubsh %xmm2, %xmm3, %xmm1", Fp16, F16Edge),
        ("vsubsh_fp16_simd_fp_arith_edge_mem", "vsubsh 22(%rax), %xmm3, %xmm1", Fp16, F16Edge),
        ("vmulph_fp16_simd_fp_arith_edge_reg", "vmulph %zmm2, %zmm3, %zmm1", Fp16, F16Edge),
        ("vmulph_fp16_simd_fp_arith_edge_mem", "vmulph 64(%rax), %zmm3, %zmm1", Fp16, F16Edge),
        ("vmulsh_fp16_simd_fp_arith_edge_reg", "vmulsh %xmm2, %xmm3, %xmm1", Fp16, F16Edge),
        ("vmulsh_fp16_simd_fp_arith_edge_mem", "vmulsh 22(%rax), %xmm3, %xmm1", Fp16, F16Edge),
        ("vdivph_fp16_simd_fp_arith_edge_reg", "vdivph %zmm2, %zmm3, %zmm1", Fp16, F16Edge),
        ("vdivph_fp16_simd_fp_arith_edge_mem", "vdivph 64(%rax), %zmm3, %zmm1", Fp16, F16Edge),
        ("vdivsh_fp16_simd_fp_arith_edge_reg", "vdivsh %xmm2, %xmm3, %xmm1", Fp16, F16Edge),
        ("vdivsh_fp16_simd_fp_arith_edge_mem", "vdivsh 22(%rax), %xmm3, %xmm1", Fp16, F16Edge),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile,
        });
    }

    // SIMD floating-point edge semantics across legacy SSE/SSE2, VEX AVX, and
    // EVEX AVX-512. Sqrt uses non-NaN inputs so exact comparison is meaningful;
    // min/max and compare intentionally use NaN-capable profiles.
    for &(label, asm, feat, profile) in &[
        ("sqrtps_sse_simd_fp_sqrt_edge_reg", "sqrtps %xmm3, %xmm1", Sse, F32SqrtEdge),
        ("sqrtps_sse_simd_fp_sqrt_edge_mem", "sqrtps 64(%rax), %xmm1", Sse, F32SqrtEdge),
        ("sqrtss_sse_simd_fp_sqrt_edge_reg", "sqrtss %xmm2, %xmm1", Sse, F32SqrtEdge),
        ("sqrtss_sse_simd_fp_sqrt_edge_mem", "sqrtss 32(%rax), %xmm1", Sse, F32SqrtEdge),
        ("sqrtpd_sse2_simd_fp_sqrt_edge_reg", "sqrtpd %xmm3, %xmm1", Sse2, F64SqrtEdge),
        ("sqrtpd_sse2_simd_fp_sqrt_edge_mem", "sqrtpd 64(%rax), %xmm1", Sse2, F64SqrtEdge),
        ("sqrtsd_sse2_simd_fp_sqrt_edge_reg", "sqrtsd %xmm2, %xmm1", Sse2, F64SqrtEdge),
        ("sqrtsd_sse2_simd_fp_sqrt_edge_mem", "sqrtsd 32(%rax), %xmm1", Sse2, F64SqrtEdge),
        ("vsqrtps_avx_simd_fp_sqrt_edge_reg", "{vex} vsqrtps %ymm3, %ymm1", Avx, F32SqrtEdge),
        ("vsqrtps_avx_simd_fp_sqrt_edge_mem", "{vex} vsqrtps 64(%rax), %ymm1", Avx, F32SqrtEdge),
        ("vsqrtss_avx_simd_fp_sqrt_edge_reg", "{vex} vsqrtss %xmm2, %xmm3, %xmm1", Avx, F32SqrtEdge),
        ("vsqrtss_avx_simd_fp_sqrt_edge_mem", "{vex} vsqrtss 32(%rax), %xmm3, %xmm1", Avx, F32SqrtEdge),
        ("vsqrtpd_avx_simd_fp_sqrt_edge_reg", "{vex} vsqrtpd %ymm3, %ymm1", Avx, F64SqrtEdge),
        ("vsqrtpd_avx_simd_fp_sqrt_edge_mem", "{vex} vsqrtpd 64(%rax), %ymm1", Avx, F64SqrtEdge),
        ("vsqrtsd_avx_simd_fp_sqrt_edge_reg", "{vex} vsqrtsd %xmm2, %xmm3, %xmm1", Avx, F64SqrtEdge),
        ("vsqrtsd_avx_simd_fp_sqrt_edge_mem", "{vex} vsqrtsd 32(%rax), %xmm3, %xmm1", Avx, F64SqrtEdge),
        ("vsqrtps_avx512_simd_fp_sqrt_edge_reg", "{evex} vsqrtps %zmm3, %zmm1", F, F32SqrtEdge),
        ("vsqrtps_avx512_simd_fp_sqrt_edge_mem", "{evex} vsqrtps 64(%rax), %zmm1", F, F32SqrtEdge),
        ("vsqrtss_avx512_simd_fp_sqrt_edge_reg", "{evex} vsqrtss %xmm2, %xmm3, %xmm1", F, F32SqrtEdge),
        ("vsqrtss_avx512_simd_fp_sqrt_edge_mem", "{evex} vsqrtss 32(%rax), %xmm3, %xmm1", F, F32SqrtEdge),
        ("vsqrtpd_avx512_simd_fp_sqrt_edge_reg", "{evex} vsqrtpd %zmm3, %zmm1", F, F64SqrtEdge),
        ("vsqrtpd_avx512_simd_fp_sqrt_edge_mem", "{evex} vsqrtpd 64(%rax), %zmm1", F, F64SqrtEdge),
        ("vsqrtsd_avx512_simd_fp_sqrt_edge_reg", "{evex} vsqrtsd %xmm2, %xmm3, %xmm1", F, F64SqrtEdge),
        ("vsqrtsd_avx512_simd_fp_sqrt_edge_mem", "{evex} vsqrtsd 32(%rax), %xmm3, %xmm1", F, F64SqrtEdge),
        ("vminps_avx_simd_fp_minmax_edge_reg", "{vex} vminps %ymm2, %ymm3, %ymm1", Avx, F32Edge),
        ("vmaxps_avx_simd_fp_minmax_edge_mem", "{vex} vmaxps 64(%rax), %ymm3, %ymm1", Avx, F32Edge),
        ("vminss_avx_simd_fp_minmax_edge_reg", "{vex} vminss %xmm2, %xmm3, %xmm1", Avx, F32Edge),
        ("vmaxss_avx_simd_fp_minmax_edge_mem", "{vex} vmaxss 32(%rax), %xmm3, %xmm1", Avx, F32Edge),
        ("vminpd_avx_simd_fp_minmax_edge_reg", "{vex} vminpd %ymm2, %ymm3, %ymm1", Avx, F64Edge),
        ("vmaxpd_avx_simd_fp_minmax_edge_mem", "{vex} vmaxpd 64(%rax), %ymm3, %ymm1", Avx, F64Edge),
        ("vminsd_avx_simd_fp_minmax_edge_reg", "{vex} vminsd %xmm2, %xmm3, %xmm1", Avx, F64Edge),
        ("vmaxsd_avx_simd_fp_minmax_edge_mem", "{vex} vmaxsd 32(%rax), %xmm3, %xmm1", Avx, F64Edge),
        ("vminps_avx512_simd_fp_minmax_edge_reg", "{evex} vminps %zmm2, %zmm3, %zmm1", F, F32Edge),
        ("vmaxps_avx512_simd_fp_minmax_edge_mem", "{evex} vmaxps 64(%rax), %zmm3, %zmm1", F, F32Edge),
        ("vminss_avx512_simd_fp_minmax_edge_reg", "{evex} vminss %xmm2, %xmm3, %xmm1", F, F32Edge),
        ("vmaxss_avx512_simd_fp_minmax_edge_mem", "{evex} vmaxss 32(%rax), %xmm3, %xmm1", F, F32Edge),
        ("vminpd_avx512_simd_fp_minmax_edge_reg", "{evex} vminpd %zmm2, %zmm3, %zmm1", F, F64Edge),
        ("vmaxpd_avx512_simd_fp_minmax_edge_mem", "{evex} vmaxpd 64(%rax), %zmm3, %zmm1", F, F64Edge),
        ("vminsd_avx512_simd_fp_minmax_edge_reg", "{evex} vminsd %xmm2, %xmm3, %xmm1", F, F64Edge),
        ("vmaxsd_avx512_simd_fp_minmax_edge_mem", "{evex} vmaxsd 32(%rax), %xmm3, %xmm1", F, F64Edge),
        ("comiss_sse_simd_fp_compare_edge_qnan_mem", "comiss 44(%rax), %xmm1", Sse, F32Edge),
        ("ucomiss_sse_simd_fp_compare_edge_qnan_mem", "ucomiss 44(%rax), %xmm1", Sse, F32Edge),
        ("comisd_sse2_simd_fp_compare_edge_qnan_mem", "comisd 8(%rax), %xmm1", Sse2, F64Edge),
        ("ucomisd_sse2_simd_fp_compare_edge_qnan_mem", "ucomisd 8(%rax), %xmm1", Sse2, F64Edge),
        ("vcomiss_avx_simd_fp_compare_edge_qnan_mem", "{vex} vcomiss 44(%rax), %xmm1", Avx, F32Edge),
        ("vucomiss_avx_simd_fp_compare_edge_qnan_mem", "{vex} vucomiss 44(%rax), %xmm1", Avx, F32Edge),
        ("vcomisd_avx_simd_fp_compare_edge_qnan_mem", "{vex} vcomisd 8(%rax), %xmm1", Avx, F64Edge),
        ("vucomisd_avx_simd_fp_compare_edge_qnan_mem", "{vex} vucomisd 8(%rax), %xmm1", Avx, F64Edge),
        ("vcmpps_avx_simd_fp_compare_edge_gt_oq_qnan_mem", "{vex} vcmpps $0x1e, 32(%rax), %ymm3, %ymm1", Avx, F32Edge),
        ("vcmppd_avx_simd_fp_compare_edge_gt_oq_qnan_mem", "{vex} vcmppd $0x1e, 32(%rax), %ymm3, %ymm1", Avx, F64Edge),
        ("vcmpss_avx_simd_fp_compare_edge_gt_oq_qnan_mem", "{vex} vcmpss $0x1e, 44(%rax), %xmm3, %xmm1", Avx, F32Edge),
        ("vcmpsd_avx_simd_fp_compare_edge_gt_oq_qnan_mem", "{vex} vcmpsd $0x1e, 8(%rax), %xmm3, %xmm1", Avx, F64Edge),
        ("vcomiss_avx512_simd_fp_compare_edge_qnan_mem", "{evex} vcomiss 44(%rax), %xmm1", F, F32Edge),
        ("vucomiss_avx512_simd_fp_compare_edge_qnan_mem", "{evex} vucomiss 44(%rax), %xmm1", F, F32Edge),
        ("vcomisd_avx512_simd_fp_compare_edge_qnan_mem", "{evex} vcomisd 8(%rax), %xmm1", F, F64Edge),
        ("vucomisd_avx512_simd_fp_compare_edge_qnan_mem", "{evex} vucomisd 8(%rax), %xmm1", F, F64Edge),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
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

    // Scalar conversion width variants. These exercise the REX.W-sensitive
    // r/m32 vs r/m64 and r32 vs r64 paths that share the same mnemonics.
    for &(label, asm, feat, profile) in &[
        (
            "cvtsi2ss_sse_scalar_width_r32",
            "cvtsi2ss %r8d, %xmm1",
            Sse,
            F32,
        ),
        (
            "cvtsi2ss_sse_scalar_width_m64",
            "cvtsi2ssq 16(%rax), %xmm1",
            Sse,
            Int,
        ),
        (
            "cvtss2si_sse_scalar_width_xmm_r32",
            "cvtss2si %xmm1, %r8d",
            Sse,
            F32,
        ),
        (
            "cvtss2si_sse_scalar_width_m32_r32",
            "cvtss2si 16(%rax), %r8d",
            Sse,
            F32,
        ),
        (
            "cvttss2si_sse_scalar_width_xmm_r32",
            "cvttss2si %xmm1, %r8d",
            Sse,
            F32,
        ),
        (
            "cvttss2si_sse_scalar_width_m32_r32",
            "cvttss2si 16(%rax), %r8d",
            Sse,
            F32,
        ),
        (
            "cvtsi2sd_sse2_scalar_width_r32",
            "cvtsi2sd %r8d, %xmm1",
            Sse2,
            F64,
        ),
        (
            "cvtsi2sd_sse2_scalar_width_m64",
            "cvtsi2sdq 16(%rax), %xmm1",
            Sse2,
            Int,
        ),
        (
            "cvtsd2si_sse2_scalar_width_xmm_r32",
            "cvtsd2si %xmm1, %r8d",
            Sse2,
            F64,
        ),
        (
            "cvtsd2si_sse2_scalar_width_m64_r32",
            "cvtsd2si 16(%rax), %r8d",
            Sse2,
            F64,
        ),
        (
            "cvttsd2si_sse2_scalar_width_xmm_r32",
            "cvttsd2si %xmm1, %r8d",
            Sse2,
            F64,
        ),
        (
            "cvttsd2si_sse2_scalar_width_m64_r32",
            "cvttsd2si 16(%rax), %r8d",
            Sse2,
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

    // Legacy packed conversion forms that bridge MMX integer pairs with SSE/SSE2
    // floating-point vectors, plus SSE2 packed dword/single conversions.
    for &(label, asm, feat, profile) in &[
        (
            "cvtpi2ps_sse_legacy_convert_mmx_to_xmm",
            "movq 32(%rax), %mm0\ncvtpi2ps %mm0, %xmm1\nemms",
            Sse,
            Int,
        ),
        (
            "cvtpi2ps_sse_legacy_convert_mem_to_xmm",
            "cvtpi2ps 32(%rax), %xmm1",
            Sse,
            Int,
        ),
        (
            "cvtps2pi_sse_legacy_convert_xmm_to_mmx_store",
            "cvtps2pi %xmm1, %mm0\nmovq %mm0, 64(%rax)\nemms",
            Sse,
            F32,
        ),
        (
            "cvtps2pi_sse_legacy_convert_mem_to_mmx_store",
            "cvtps2pi 32(%rax), %mm0\nmovq %mm0, 72(%rax)\nemms",
            Sse,
            F32,
        ),
        (
            "cvttps2pi_sse_legacy_convert_xmm_to_mmx_store",
            "cvttps2pi %xmm1, %mm0\nmovq %mm0, 80(%rax)\nemms",
            Sse,
            F32,
        ),
        (
            "cvttps2pi_sse_legacy_convert_mem_to_mmx_store",
            "cvttps2pi 32(%rax), %mm0\nmovq %mm0, 88(%rax)\nemms",
            Sse,
            F32,
        ),
        (
            "cvtpi2pd_sse2_legacy_convert_mmx_to_xmm",
            "movq 32(%rax), %mm0\ncvtpi2pd %mm0, %xmm1\nemms",
            Sse2,
            Int,
        ),
        (
            "cvtpi2pd_sse2_legacy_convert_mem_to_xmm",
            "cvtpi2pd 32(%rax), %xmm1",
            Sse2,
            Int,
        ),
        (
            "cvtpd2pi_sse2_legacy_convert_xmm_to_mmx_store",
            "cvtpd2pi %xmm1, %mm0\nmovq %mm0, 96(%rax)\nemms",
            Sse2,
            F64,
        ),
        (
            "cvtpd2pi_sse2_legacy_convert_mem_to_mmx_store",
            "cvtpd2pi 32(%rax), %mm0\nmovq %mm0, 104(%rax)\nemms",
            Sse2,
            F64,
        ),
        (
            "cvttpd2pi_sse2_legacy_convert_xmm_to_mmx_store",
            "cvttpd2pi %xmm1, %mm0\nmovq %mm0, 112(%rax)\nemms",
            Sse2,
            F64,
        ),
        (
            "cvttpd2pi_sse2_legacy_convert_mem_to_mmx_store",
            "cvttpd2pi 32(%rax), %mm0\nmovq %mm0, 120(%rax)\nemms",
            Sse2,
            F64,
        ),
        (
            "cvtdq2ps_sse2_legacy_convert_reg",
            "cvtdq2ps %xmm2, %xmm1",
            Sse2,
            Int,
        ),
        (
            "cvtdq2ps_sse2_legacy_convert_mem",
            "cvtdq2ps 32(%rax), %xmm1",
            Sse2,
            Int,
        ),
        (
            "cvtps2dq_sse2_legacy_convert_reg",
            "cvtps2dq %xmm2, %xmm1",
            Sse2,
            F32,
        ),
        (
            "cvtps2dq_sse2_legacy_convert_mem",
            "cvtps2dq 32(%rax), %xmm1",
            Sse2,
            F32,
        ),
        (
            "cvttps2dq_sse2_legacy_convert_reg",
            "cvttps2dq %xmm2, %xmm1",
            Sse2,
            F32,
        ),
        (
            "cvttps2dq_sse2_legacy_convert_mem",
            "cvttps2dq 32(%rax), %xmm1",
            Sse2,
            F32,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile,
        });
    }

    // Legacy SSE/SSE2 compare-predicate immediate forms. The ordinary compare
    // coverage mostly uses assembler aliases; these force raw CMP* imm8
    // decoding over edge-value profiles with NaN/Inf/zero lanes.
    for &(label, asm, feat, profile) in &[
        (
            "cmpps_legacy_cmp_pred_eq_reg",
            "cmpps $0x0, %xmm2, %xmm1",
            Sse,
            F32Edge,
        ),
        (
            "cmpps_legacy_cmp_pred_unord_mem",
            "cmpps $0x3, 16(%rax), %xmm1",
            Sse,
            F32Edge,
        ),
        (
            "cmpps_legacy_cmp_pred_nle_reg",
            "cmpps $0x6, %xmm2, %xmm1",
            Sse,
            F32Edge,
        ),
        (
            "cmpps_legacy_cmp_pred_ord_mem",
            "cmpps $0x7, 16(%rax), %xmm1",
            Sse,
            F32Edge,
        ),
        (
            "cmpss_legacy_cmp_pred_lt_reg",
            "cmpss $0x1, %xmm2, %xmm1",
            Sse,
            F32Edge,
        ),
        (
            "cmpss_legacy_cmp_pred_le_mem",
            "cmpss $0x2, 16(%rax), %xmm1",
            Sse,
            F32Edge,
        ),
        (
            "cmpss_legacy_cmp_pred_neq_reg",
            "cmpss $0x4, %xmm2, %xmm1",
            Sse,
            F32Edge,
        ),
        (
            "cmpss_legacy_cmp_pred_nlt_mem",
            "cmpss $0x5, 16(%rax), %xmm1",
            Sse,
            F32Edge,
        ),
        (
            "cmppd_legacy_cmp_pred_eq_reg",
            "cmppd $0x0, %xmm2, %xmm1",
            Sse2,
            F64Edge,
        ),
        (
            "cmppd_legacy_cmp_pred_unord_mem",
            "cmppd $0x3, 16(%rax), %xmm1",
            Sse2,
            F64Edge,
        ),
        (
            "cmppd_legacy_cmp_pred_nle_reg",
            "cmppd $0x6, %xmm2, %xmm1",
            Sse2,
            F64Edge,
        ),
        (
            "cmppd_legacy_cmp_pred_ord_mem",
            "cmppd $0x7, 16(%rax), %xmm1",
            Sse2,
            F64Edge,
        ),
        (
            "cmpsd_legacy_cmp_pred_lt_reg",
            "cmpsd $0x1, %xmm2, %xmm1",
            Sse2,
            F64Edge,
        ),
        (
            "cmpsd_legacy_cmp_pred_le_mem",
            "cmpsd $0x2, 16(%rax), %xmm1",
            Sse2,
            F64Edge,
        ),
        (
            "cmpsd_legacy_cmp_pred_neq_reg",
            "cmpsd $0x4, %xmm2, %xmm1",
            Sse2,
            F64Edge,
        ),
        (
            "cmpsd_legacy_cmp_pred_nlt_mem",
            "cmpsd $0x5, 16(%rax), %xmm1",
            Sse2,
            F64Edge,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
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

    // Legacy SSE2 packed shifts need distinct coverage for immediate,
    // XMM-count, and memory-count encodings, including the architectural
    // all-zero/saturating boundary counts.
    for &(label, asm) in &[
        ("psrlw_sse2_shift_xmm_count", "psrlw %xmm2, %xmm1"),
        ("psrld_sse2_shift_xmm_count", "psrld %xmm2, %xmm1"),
        ("psrlq_sse2_shift_xmm_count", "psrlq %xmm2, %xmm1"),
        ("psraw_sse2_shift_xmm_count", "psraw %xmm2, %xmm1"),
        ("psrad_sse2_shift_xmm_count", "psrad %xmm2, %xmm1"),
        ("psllw_sse2_shift_xmm_count", "psllw %xmm2, %xmm1"),
        ("pslld_sse2_shift_xmm_count", "pslld %xmm2, %xmm1"),
        ("psllq_sse2_shift_xmm_count", "psllq %xmm2, %xmm1"),
        ("psllw_sse2_shift_mem_count", "psllw 16(%rax), %xmm1"),
        ("psllq_sse2_shift_mem_count", "psllq 16(%rax), %xmm1"),
        ("psrlw_sse2_shift_mem_count", "psrlw 16(%rax), %xmm1"),
        ("psrlq_sse2_shift_mem_count", "psrlq 16(%rax), %xmm1"),
        ("psraw_sse2_shift_mem_count", "psraw 16(%rax), %xmm1"),
        ("psllw_sse2_shift_imm15_edge", "psllw $15, %xmm1"),
        ("psllw_sse2_shift_imm16_zero", "psllw $16, %xmm1"),
        ("pslld_sse2_shift_imm31_edge", "pslld $31, %xmm1"),
        ("pslld_sse2_shift_imm32_zero", "pslld $32, %xmm1"),
        ("psllq_sse2_shift_imm63_edge", "psllq $63, %xmm1"),
        ("psllq_sse2_shift_imm64_zero", "psllq $64, %xmm1"),
        ("psrlw_sse2_shift_imm16_zero", "psrlw $16, %xmm1"),
        ("psrld_sse2_shift_imm32_zero", "psrld $32, %xmm1"),
        ("psrlq_sse2_shift_imm64_zero", "psrlq $64, %xmm1"),
        ("psraw_sse2_shift_imm15_saturate", "psraw $15, %xmm1"),
        ("psrad_sse2_shift_imm31_saturate", "psrad $31, %xmm1"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Sse2,
            profile: Int,
        });
    }

    // Legacy packed shuffle, mask extraction, average, and SAD instructions.
    // The XMM forms cover SSE2 decoders; the MMX aliases exercise the same
    // opcode families' separate MMX register paths and store results to scratch.
    for &(label, asm, feat) in &[
        (
            "pshufd_sse2_packed_misc_reg",
            "pshufd $0x1b, %xmm2, %xmm1",
            Sse2,
        ),
        (
            "pshufd_sse2_packed_misc_mem",
            "pshufd $0x4e, 32(%rax), %xmm1",
            Sse2,
        ),
        (
            "pshufhw_sse2_packed_misc_reg",
            "pshufhw $0x1b, %xmm2, %xmm1",
            Sse2,
        ),
        (
            "pshufhw_sse2_packed_misc_mem",
            "pshufhw $0xb1, 32(%rax), %xmm1",
            Sse2,
        ),
        (
            "pshuflw_sse2_packed_misc_reg",
            "pshuflw $0x1b, %xmm2, %xmm1",
            Sse2,
        ),
        (
            "pshuflw_sse2_packed_misc_mem",
            "pshuflw $0xb1, 32(%rax), %xmm1",
            Sse2,
        ),
        (
            "pmovmskb_sse2_packed_misc_xmm",
            "pmovmskb %xmm1, %r8d",
            Sse2,
        ),
        ("psadbw_sse2_packed_misc_reg", "psadbw %xmm2, %xmm1", Sse2),
        (
            "psadbw_sse2_packed_misc_mem",
            "psadbw 32(%rax), %xmm1",
            Sse2,
        ),
        ("pavgb_sse2_packed_misc_reg", "pavgb %xmm2, %xmm1", Sse2),
        ("pavgb_sse2_packed_misc_mem", "pavgb 32(%rax), %xmm1", Sse2),
        ("pavgw_sse2_packed_misc_reg", "pavgw %xmm2, %xmm1", Sse2),
        ("pavgw_sse2_packed_misc_mem", "pavgw 32(%rax), %xmm1", Sse2),
        (
            "pshufw_mmx_packed_misc_mem_store",
            "pshufw $0x1b, 40(%rax), %mm1\nmovq %mm1, 64(%rax)\nemms",
            Mmx,
        ),
        (
            "psadbw_mmx_packed_misc_store",
            "movq 32(%rax), %mm0\npsadbw 40(%rax), %mm0\nmovq %mm0, 72(%rax)\nemms",
            Mmx,
        ),
        (
            "pavgb_mmx_packed_misc_store",
            "movq 32(%rax), %mm0\npavgb 40(%rax), %mm0\nmovq %mm0, 80(%rax)\nemms",
            Mmx,
        ),
        (
            "pavgw_mmx_packed_misc_store",
            "movq 32(%rax), %mm0\npavgw 40(%rax), %mm0\nmovq %mm0, 88(%rax)\nemms",
            Mmx,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }

    // Legacy SSE2 scalar/vector transfer forms cover the 0F 6E/7E MOVD/MOVQ
    // GPR paths plus the MMX bridge instructions that feed or consume XMM
    // state.
    for &(label, asm) in &[
        ("movd_sse2_transfer_xmm_from_r8d", "movd %r8d, %xmm1"),
        ("movq_sse2_transfer_xmm_from_r8", "movq %r8, %xmm1"),
        ("movd_sse2_transfer_r8d_from_xmm", "movd %xmm1, %r8d"),
        ("movq_sse2_transfer_r8_from_xmm", "movq %xmm1, %r8"),
        ("movd_sse2_transfer_xmm_from_m32", "movd 12(%rax), %xmm1"),
        ("movd_sse2_transfer_m32_from_xmm", "movd %xmm1, 44(%rax)"),
        (
            "movq2dq_sse2_transfer_mmx_to_xmm",
            "movq 32(%rax), %mm0\nmovq2dq %mm0, %xmm1\nemms",
        ),
        (
            "movdq2q_sse2_transfer_xmm_to_mmx",
            "movdq2q %xmm1, %mm0\nmovq %mm0, 72(%rax)\nemms",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Sse2,
            profile: Int,
        });
    }

    // Legacy SSE2 word insertion/extraction and whole-XMM byte-lane shifts use
    // the compact 0F C4/C5 and group-14 immediate encodings.
    for &(label, asm) in &[
        ("pinsrw_sse2_lane_insert_r8w", "pinsrw $3, %r8d, %xmm1"),
        ("pinsrw_sse2_lane_insert_m16", "pinsrw $5, 32(%rax), %xmm1"),
        ("pextrw_sse2_lane_extract_r8d", "pextrw $6, %xmm1, %r8d"),
        ("pslldq_sse2_lane_shift_left_4", "pslldq $4, %xmm1"),
        ("psrldq_sse2_lane_shift_right_6", "psrldq $6, %xmm1"),
        ("pslldq_sse2_lane_shift_left_zero_17", "pslldq $17, %xmm1"),
        ("psrldq_sse2_lane_shift_right_zero_16", "psrldq $16, %xmm1"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Sse2,
            profile: Int,
        });
    }

    // Legacy SSE3 forms. These cover horizontal add/subtract, alternating
    // add/subtract, duplicate moves, and LDDQU's unaligned/addr32 load paths.
    for &(label, asm, profile) in &[
        ("addsubps_sse3_reg", "addsubps %xmm2, %xmm1", F32),
        ("addsubps_sse3_mem", "addsubps 16(%rax), %xmm1", F32),
        ("addsubpd_sse3_reg", "addsubpd %xmm2, %xmm1", F64),
        ("addsubpd_sse3_mem", "addsubpd 16(%rax), %xmm1", F64),
        ("haddps_sse3_reg", "haddps %xmm2, %xmm1", F32),
        ("haddps_sse3_mem", "haddps 16(%rax), %xmm1", F32),
        ("haddpd_sse3_reg", "haddpd %xmm2, %xmm1", F64),
        ("haddpd_sse3_mem", "haddpd 16(%rax), %xmm1", F64),
        ("hsubps_sse3_reg", "hsubps %xmm2, %xmm1", F32),
        ("hsubps_sse3_mem", "hsubps 16(%rax), %xmm1", F32),
        ("hsubpd_sse3_reg", "hsubpd %xmm2, %xmm1", F64),
        ("hsubpd_sse3_mem", "hsubpd 16(%rax), %xmm1", F64),
        ("movddup_sse3_reg", "movddup %xmm2, %xmm1", F64),
        ("movddup_sse3_mem", "movddup 16(%rax), %xmm1", F64),
        ("movsldup_sse3_reg", "movsldup %xmm2, %xmm1", F32),
        ("movsldup_sse3_mem", "movsldup 16(%rax), %xmm1", F32),
        ("movshdup_sse3_reg", "movshdup %xmm2, %xmm1", F32),
        ("movshdup_sse3_mem", "movshdup 16(%rax), %xmm1", F32),
        ("lddqu_sse3_load_unaligned", "lddqu 17(%rax), %xmm1", Int),
        (
            "addr32_lddqu_sse3_load",
            "addr32 lddqu 17(%eax), %xmm1",
            Int,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Sse3,
            profile,
        });
    }

    for &(label, asm, profile) in &[
        ("addsubps_sse3_edge_self", "addsubps %xmm1, %xmm1", F32),
        ("addsubpd_sse3_edge_self", "addsubpd %xmm1, %xmm1", F64),
        ("haddps_sse3_edge_self", "haddps %xmm1, %xmm1", F32),
        ("haddpd_sse3_edge_self", "haddpd %xmm1, %xmm1", F64),
        ("hsubps_sse3_edge_self", "hsubps %xmm1, %xmm1", F32),
        ("hsubpd_sse3_edge_self", "hsubpd %xmm1, %xmm1", F64),
        ("movddup_sse3_edge_self", "movddup %xmm1, %xmm1", F64),
        ("movddup_sse3_edge_unaligned_mem", "movddup 7(%rax), %xmm1", Int),
        ("movsldup_sse3_edge_self", "movsldup %xmm1, %xmm1", F32),
        ("movshdup_sse3_edge_self", "movshdup %xmm1, %xmm1", F32),
        ("lddqu_sse3_edge_unaligned_1", "lddqu 1(%rax), %xmm1", Int),
        ("lddqu_sse3_edge_high_dest", "lddqu 63(%rax), %xmm15", Int),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Sse3,
            profile,
        });
    }

    // Legacy streaming and masked memory stores. These instructions are mostly
    // memory side effects, so the scratch-page diff directly checks the
    // ordinary-store model used for non-temporal hints and mask-selected bytes.
    for &(label, asm, feat, profile) in &[
        (
            "movntps_legacy_stream_xmm",
            "movntps %xmm1, 32(%rax)",
            Sse,
            F32,
        ),
        (
            "movntpd_legacy_stream_xmm",
            "movntpd %xmm1, 48(%rax)",
            Sse2,
            F64,
        ),
        (
            "movntdq_legacy_stream_xmm",
            "movntdq %xmm1, 64(%rax)",
            Sse2,
            Int,
        ),
        (
            "movnti_legacy_stream_m64_r8",
            "movnti %r8, 80(%rax)",
            Sse2,
            Int,
        ),
        (
            "movnti_legacy_stream_m32_r8d",
            "movnti %r8d, 88(%rax)",
            Sse2,
            Int,
        ),
        (
            "maskmovdqu_legacy_mask_store",
            "maskmovdqu %xmm2, %xmm1",
            Sse2,
            Int,
        ),
        (
            "movntq_legacy_stream_mmx",
            "movq 32(%rax), %mm0\nmovntq %mm0, 96(%rax)\nemms",
            Mmx,
            Int,
        ),
        (
            "maskmovq_legacy_mask_store",
            "movq 32(%rax), %mm0\nmovq 40(%rax), %mm1\nmaskmovq %mm1, %mm0\nemms",
            Mmx,
            Int,
        ),
        (
            "maskmovdqu_legacy_mask_edge_zero_mask",
            "movdqu 64(%rax), %xmm1\npxor %xmm2, %xmm2\nmaskmovdqu %xmm2, %xmm1",
            Sse2,
            Int,
        ),
        (
            "maskmovdqu_legacy_mask_edge_allones_mask",
            "movdqu 64(%rax), %xmm1\npcmpeqb %xmm2, %xmm2\nmaskmovdqu %xmm2, %xmm1",
            Sse2,
            Int,
        ),
        (
            "maskmovq_legacy_mask_edge_zero_mask",
            "movq 64(%rax), %mm0\npxor %mm1, %mm1\nmaskmovq %mm1, %mm0\nemms",
            Mmx,
            Int,
        ),
        (
            "maskmovq_legacy_mask_edge_allones_mask",
            "movq 72(%rax), %mm0\npcmpeqb %mm1, %mm1\nmaskmovq %mm1, %mm0\nemms",
            Mmx,
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

    // SSSE3 edge forms around byte-shuffle high-bit masking, PALIGNR boundary
    // counts, self-sign/absolute operations, and MMX encodings made visible by
    // storing the MMX result before EMMS.
    for &(label, asm) in &[
        (
            "pshufb_ssse3_edge_xmm_mem_highbits",
            "pshufb 48(%rax), %xmm1",
        ),
        ("pshufb_ssse3_edge_xmm_self_mask", "pshufb %xmm1, %xmm1"),
        (
            "palignr_ssse3_edge_xmm_imm0_reg",
            "palignr $0, %xmm2, %xmm1",
        ),
        (
            "palignr_ssse3_edge_xmm_imm15_mem",
            "palignr $15, 32(%rax), %xmm1",
        ),
        (
            "palignr_ssse3_edge_xmm_imm16_reg",
            "palignr $16, %xmm2, %xmm1",
        ),
        (
            "palignr_ssse3_edge_xmm_imm31_mem",
            "palignr $31, 32(%rax), %xmm1",
        ),
        (
            "palignr_ssse3_edge_xmm_imm32_reg",
            "palignr $32, %xmm2, %xmm1",
        ),
        ("psignb_ssse3_edge_xmm_self", "psignb %xmm1, %xmm1"),
        ("psignw_ssse3_edge_xmm_self", "psignw %xmm1, %xmm1"),
        ("psignd_ssse3_edge_xmm_self", "psignd %xmm1, %xmm1"),
        ("pabsb_ssse3_edge_xmm_mem", "pabsb 32(%rax), %xmm1"),
        ("pabsw_ssse3_edge_xmm_self", "pabsw %xmm1, %xmm1"),
        ("pabsd_ssse3_edge_xmm_mem", "pabsd 32(%rax), %xmm1"),
        (
            "pshufb_ssse3_edge_mmx_store",
            "movq 32(%rax), %mm0\npshufb 40(%rax), %mm0\nmovq %mm0, 64(%rax)\nemms",
        ),
        (
            "pmaddubsw_ssse3_edge_mmx_store",
            "movq 32(%rax), %mm0\npmaddubsw 40(%rax), %mm0\nmovq %mm0, 72(%rax)\nemms",
        ),
        (
            "palignr_ssse3_edge_mmx_imm7_store",
            "movq 32(%rax), %mm0\npalignr $7, 40(%rax), %mm0\nmovq %mm0, 80(%rax)\nemms",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Ssse3,
            profile: Int,
        });
    }

    for &(label, asm, feat) in &[
        (
            "paddsb_mmx_int_sat_edge_store",
            "movq 32(%rax), %mm0\npaddsb 40(%rax), %mm0\nmovq %mm0, 64(%rax)\nemms",
            Mmx,
        ),
        (
            "paddusw_mmx_int_sat_edge_store",
            "movq 32(%rax), %mm0\npaddusw 40(%rax), %mm0\nmovq %mm0, 72(%rax)\nemms",
            Mmx,
        ),
        (
            "psubusb_mmx_int_sat_edge_store",
            "movq 32(%rax), %mm0\npsubusb 40(%rax), %mm0\nmovq %mm0, 80(%rax)\nemms",
            Mmx,
        ),
        (
            "packsswb_mmx_int_sat_edge_store",
            "movq 32(%rax), %mm0\npacksswb 40(%rax), %mm0\nmovq %mm0, 88(%rax)\nemms",
            Mmx,
        ),
        (
            "packssdw_mmx_int_sat_edge_store",
            "movq 32(%rax), %mm0\npackssdw 40(%rax), %mm0\nmovq %mm0, 96(%rax)\nemms",
            Mmx,
        ),
        (
            "packuswb_mmx_int_sat_edge_store",
            "movq 32(%rax), %mm0\npackuswb 40(%rax), %mm0\nmovq %mm0, 104(%rax)\nemms",
            Mmx,
        ),
        ("paddsb_sse2_int_sat_edge_reg", "paddsb %xmm2, %xmm1", Sse2),
        ("paddsw_sse2_int_sat_edge_mem", "paddsw 32(%rax), %xmm1", Sse2),
        ("paddusb_sse2_int_sat_edge_reg", "paddusb %xmm2, %xmm1", Sse2),
        ("paddusw_sse2_int_sat_edge_mem", "paddusw 32(%rax), %xmm1", Sse2),
        ("psubsb_sse2_int_sat_edge_reg", "psubsb %xmm2, %xmm1", Sse2),
        ("psubsw_sse2_int_sat_edge_mem", "psubsw 32(%rax), %xmm1", Sse2),
        ("psubusb_sse2_int_sat_edge_reg", "psubusb %xmm2, %xmm1", Sse2),
        ("psubusw_sse2_int_sat_edge_mem", "psubusw 32(%rax), %xmm1", Sse2),
        ("packsswb_sse2_int_sat_edge_reg", "packsswb %xmm2, %xmm1", Sse2),
        ("packssdw_sse2_int_sat_edge_mem", "packssdw 32(%rax), %xmm1", Sse2),
        ("packuswb_sse2_int_sat_edge_reg", "packuswb %xmm2, %xmm1", Sse2),
        ("packuswb_sse2_int_sat_edge_mem", "packuswb 32(%rax), %xmm1", Sse2),
        ("phaddsw_ssse3_int_sat_edge_reg", "phaddsw %xmm2, %xmm1", Ssse3),
        ("phaddsw_ssse3_int_sat_edge_mem", "phaddsw 32(%rax), %xmm1", Ssse3),
        ("phsubsw_ssse3_int_sat_edge_reg", "phsubsw %xmm2, %xmm1", Ssse3),
        ("phsubsw_ssse3_int_sat_edge_mem", "phsubsw 32(%rax), %xmm1", Ssse3),
        (
            "pmaddubsw_ssse3_int_sat_edge_reg",
            "pmaddubsw %xmm2, %xmm1",
            Ssse3,
        ),
        (
            "pmaddubsw_ssse3_int_sat_edge_mem",
            "pmaddubsw 32(%rax), %xmm1",
            Ssse3,
        ),
        ("packusdw_sse41_int_sat_edge_reg", "packusdw %xmm2, %xmm1", Sse41),
        ("packusdw_sse41_int_sat_edge_mem", "packusdw 32(%rax), %xmm1", Sse41),
        (
            "vpaddsb_avx2_int_sat_edge_reg",
            "{vex} vpaddsb %xmm2, %xmm3, %xmm1",
            Avx2,
        ),
        (
            "vpaddsw_avx2_int_sat_edge_mem",
            "{vex} vpaddsw 32(%rax), %ymm3, %ymm1",
            Avx2,
        ),
        (
            "vpaddusb_avx2_int_sat_edge_reg",
            "{vex} vpaddusb %ymm2, %ymm3, %ymm1",
            Avx2,
        ),
        (
            "vpaddusw_avx2_int_sat_edge_mem",
            "{vex} vpaddusw 32(%rax), %xmm3, %xmm1",
            Avx2,
        ),
        (
            "vpsubsb_avx2_int_sat_edge_reg",
            "{vex} vpsubsb %xmm2, %xmm3, %xmm1",
            Avx2,
        ),
        (
            "vpsubsw_avx2_int_sat_edge_mem",
            "{vex} vpsubsw 32(%rax), %ymm3, %ymm1",
            Avx2,
        ),
        (
            "vpsubusb_avx2_int_sat_edge_reg",
            "{vex} vpsubusb %ymm2, %ymm3, %ymm1",
            Avx2,
        ),
        (
            "vpsubusw_avx2_int_sat_edge_mem",
            "{vex} vpsubusw 32(%rax), %xmm3, %xmm1",
            Avx2,
        ),
        (
            "vpacksswb_avx2_int_sat_edge_reg",
            "{vex} vpacksswb %xmm2, %xmm3, %xmm1",
            Avx2,
        ),
        (
            "vpackssdw_avx2_int_sat_edge_mem",
            "{vex} vpackssdw 32(%rax), %ymm3, %ymm1",
            Avx2,
        ),
        (
            "vpackuswb_avx2_int_sat_edge_reg",
            "{vex} vpackuswb %ymm2, %ymm3, %ymm1",
            Avx2,
        ),
        (
            "vpackusdw_avx2_int_sat_edge_mem",
            "{vex} vpackusdw 32(%rax), %xmm3, %xmm1",
            Avx2,
        ),
        (
            "vphaddsw_avx2_int_sat_edge_reg",
            "{vex} vphaddsw %xmm2, %xmm3, %xmm1",
            Avx2,
        ),
        (
            "vphsubsw_avx2_int_sat_edge_mem",
            "{vex} vphsubsw 32(%rax), %xmm3, %xmm1",
            Avx2,
        ),
        (
            "vpmaddubsw_avx2_int_sat_edge_mem",
            "{vex} vpmaddubsw 32(%rax), %ymm3, %ymm1",
            Avx2,
        ),
        ("vpaddsb_avx512_int_sat_edge_reg", "vpaddsb %zmm2, %zmm3, %zmm1", Bw),
        (
            "vpaddsw_avx512_int_sat_edge_mem",
            "vpaddsw 64(%rax), %zmm3, %zmm1",
            Bw,
        ),
        (
            "vpaddusb_avx512_int_sat_edge_reg",
            "vpaddusb %zmm2, %zmm3, %zmm1",
            Bw,
        ),
        (
            "vpaddusw_avx512_int_sat_edge_mem",
            "vpaddusw 64(%rax), %zmm3, %zmm1",
            Bw,
        ),
        ("vpsubsb_avx512_int_sat_edge_reg", "vpsubsb %zmm2, %zmm3, %zmm1", Bw),
        (
            "vpsubsw_avx512_int_sat_edge_mem",
            "vpsubsw 64(%rax), %zmm3, %zmm1",
            Bw,
        ),
        (
            "vpsubusb_avx512_int_sat_edge_reg",
            "vpsubusb %zmm2, %zmm3, %zmm1",
            Bw,
        ),
        (
            "vpsubusw_avx512_int_sat_edge_mem",
            "vpsubusw 64(%rax), %zmm3, %zmm1",
            Bw,
        ),
        (
            "vpacksswb_avx512_int_sat_edge_reg",
            "vpacksswb %zmm2, %zmm3, %zmm1",
            Bw,
        ),
        (
            "vpackuswb_avx512_int_sat_edge_mem",
            "vpackuswb 64(%rax), %zmm3, %zmm1",
            Bw,
        ),
        (
            "vpackssdw_avx512_int_sat_edge_reg",
            "vpackssdw %zmm2, %zmm3, %zmm1",
            F,
        ),
        (
            "vpackusdw_avx512_int_sat_edge_mem",
            "vpackusdw 64(%rax), %zmm3, %zmm1",
            F,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: IntSatEdge,
        });
    }

    for &(label, asm, feat) in &[
        ("pcmpeqb_sse2_int_order_edge_reg", "pcmpeqb %xmm2, %xmm1", Sse2),
        ("pcmpgtb_sse2_int_order_edge_reg", "pcmpgtb %xmm2, %xmm1", Sse2),
        ("pcmpgtw_sse2_int_order_edge_mem", "pcmpgtw 32(%rax), %xmm1", Sse2),
        ("pcmpgtd_sse2_int_order_edge_reg", "pcmpgtd %xmm2, %xmm1", Sse2),
        ("pminub_sse2_int_order_edge_reg", "pminub %xmm2, %xmm1", Sse2),
        ("pminsw_sse2_int_order_edge_mem", "pminsw 32(%rax), %xmm1", Sse2),
        ("pmaxub_sse2_int_order_edge_reg", "pmaxub %xmm2, %xmm1", Sse2),
        ("pmaxsw_sse2_int_order_edge_mem", "pmaxsw 32(%rax), %xmm1", Sse2),
        ("pminsb_sse41_int_order_edge_reg", "pminsb %xmm2, %xmm1", Sse41),
        ("pminuw_sse41_int_order_edge_mem", "pminuw 32(%rax), %xmm1", Sse41),
        ("pminud_sse41_int_order_edge_reg", "pminud %xmm2, %xmm1", Sse41),
        ("pminsd_sse41_int_order_edge_mem", "pminsd 32(%rax), %xmm1", Sse41),
        ("pmaxsb_sse41_int_order_edge_mem", "pmaxsb 32(%rax), %xmm1", Sse41),
        ("pmaxuw_sse41_int_order_edge_reg", "pmaxuw %xmm2, %xmm1", Sse41),
        ("pmaxud_sse41_int_order_edge_mem", "pmaxud 32(%rax), %xmm1", Sse41),
        ("pmaxsd_sse41_int_order_edge_reg", "pmaxsd %xmm2, %xmm1", Sse41),
        ("pcmpgtq_sse42_int_order_edge_reg", "pcmpgtq %xmm2, %xmm1", Sse42),
        ("pcmpgtq_sse42_int_order_edge_mem", "pcmpgtq 32(%rax), %xmm1", Sse42),
        (
            "vpcmpgtb_avx2_int_order_edge_reg",
            "{vex} vpcmpgtb %xmm2, %xmm3, %xmm1",
            Avx2,
        ),
        (
            "vpcmpgtw_avx2_int_order_edge_mem",
            "{vex} vpcmpgtw 32(%rax), %ymm3, %ymm1",
            Avx2,
        ),
        (
            "vpcmpgtd_avx2_int_order_edge_reg",
            "{vex} vpcmpgtd %ymm2, %ymm3, %ymm1",
            Avx2,
        ),
        (
            "vpcmpgtq_avx2_int_order_edge_mem",
            "{vex} vpcmpgtq 32(%rax), %xmm3, %xmm1",
            Avx2,
        ),
        (
            "vpminsb_avx2_int_order_edge_reg",
            "{vex} vpminsb %xmm2, %xmm3, %xmm1",
            Avx2,
        ),
        (
            "vpminsw_avx2_int_order_edge_mem",
            "{vex} vpminsw 32(%rax), %ymm3, %ymm1",
            Avx2,
        ),
        (
            "vpminsd_avx2_int_order_edge_reg",
            "{vex} vpminsd %ymm2, %ymm3, %ymm1",
            Avx2,
        ),
        (
            "vpminub_avx2_int_order_edge_mem",
            "{vex} vpminub 32(%rax), %xmm3, %xmm1",
            Avx2,
        ),
        (
            "vpminuw_avx2_int_order_edge_reg",
            "{vex} vpminuw %ymm2, %ymm3, %ymm1",
            Avx2,
        ),
        (
            "vpminud_avx2_int_order_edge_mem",
            "{vex} vpminud 32(%rax), %xmm3, %xmm1",
            Avx2,
        ),
        (
            "vpmaxsb_avx2_int_order_edge_reg",
            "{vex} vpmaxsb %xmm2, %xmm3, %xmm1",
            Avx2,
        ),
        (
            "vpmaxsw_avx2_int_order_edge_mem",
            "{vex} vpmaxsw 32(%rax), %ymm3, %ymm1",
            Avx2,
        ),
        (
            "vpmaxsd_avx2_int_order_edge_reg",
            "{vex} vpmaxsd %ymm2, %ymm3, %ymm1",
            Avx2,
        ),
        (
            "vpmaxub_avx2_int_order_edge_mem",
            "{vex} vpmaxub 32(%rax), %xmm3, %xmm1",
            Avx2,
        ),
        (
            "vpmaxuw_avx2_int_order_edge_reg",
            "{vex} vpmaxuw %ymm2, %ymm3, %ymm1",
            Avx2,
        ),
        (
            "vpmaxud_avx2_int_order_edge_mem",
            "{vex} vpmaxud 32(%rax), %xmm3, %xmm1",
            Avx2,
        ),
        ("vpcmpb_avx512_int_order_edge_lt", "vpcmpb $1, %zmm2, %zmm3, %k5", Bw),
        (
            "vpcmpub_avx512_int_order_edge_lt",
            "vpcmpub $1, 64(%rax), %zmm3, %k5",
            Bw,
        ),
        ("vpcmpw_avx512_int_order_edge_lt", "vpcmpw $1, %zmm2, %zmm3, %k5", Bw),
        (
            "vpcmpuw_avx512_int_order_edge_lt",
            "vpcmpuw $1, 64(%rax), %zmm3, %k5",
            Bw,
        ),
        ("vpcmpd_avx512_int_order_edge_lt", "vpcmpd $1, %zmm2, %zmm3, %k5", F),
        (
            "vpcmpud_avx512_int_order_edge_lt",
            "vpcmpud $1, 64(%rax), %zmm3, %k5",
            F,
        ),
        ("vpcmpq_avx512_int_order_edge_lt", "vpcmpq $1, %zmm2, %zmm3, %k5", F),
        (
            "vpcmpuq_avx512_int_order_edge_lt",
            "vpcmpuq $1, 64(%rax), %zmm3, %k5",
            F,
        ),
        ("vpminsb_avx512_int_order_edge_reg", "vpminsb %zmm2, %zmm3, %zmm1", Bw),
        ("vpminsw_avx512_int_order_edge_mem", "vpminsw 64(%rax), %zmm3, %zmm1", Bw),
        ("vpminub_avx512_int_order_edge_reg", "vpminub %zmm2, %zmm3, %zmm1", Bw),
        ("vpminuw_avx512_int_order_edge_mem", "vpminuw 64(%rax), %zmm3, %zmm1", Bw),
        ("vpmaxsb_avx512_int_order_edge_reg", "vpmaxsb %zmm2, %zmm3, %zmm1", Bw),
        ("vpmaxsw_avx512_int_order_edge_mem", "vpmaxsw 64(%rax), %zmm3, %zmm1", Bw),
        ("vpmaxub_avx512_int_order_edge_reg", "vpmaxub %zmm2, %zmm3, %zmm1", Bw),
        ("vpmaxuw_avx512_int_order_edge_mem", "vpmaxuw 64(%rax), %zmm3, %zmm1", Bw),
        ("vpminsd_avx512_int_order_edge_reg", "vpminsd %zmm2, %zmm3, %zmm1", F),
        ("vpminsq_avx512_int_order_edge_mem", "vpminsq 64(%rax), %zmm3, %zmm1", F),
        ("vpminud_avx512_int_order_edge_reg", "vpminud %zmm2, %zmm3, %zmm1", F),
        ("vpminuq_avx512_int_order_edge_mem", "vpminuq 64(%rax), %zmm3, %zmm1", F),
        ("vpmaxsd_avx512_int_order_edge_reg", "vpmaxsd %zmm2, %zmm3, %zmm1", F),
        ("vpmaxsq_avx512_int_order_edge_mem", "vpmaxsq 64(%rax), %zmm3, %zmm1", F),
        ("vpmaxud_avx512_int_order_edge_reg", "vpmaxud %zmm2, %zmm3, %zmm1", F),
        ("vpmaxuq_avx512_int_order_edge_mem", "vpmaxuq 64(%rax), %zmm3, %zmm1", F),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: IntSatEdge,
        });
    }

    for &(label, asm, feat) in &[
        (
            "psllw_mmx_int_shift_edge_imm15_store",
            "movq 32(%rax), %mm0\npsllw $15, %mm0\nmovq %mm0, 64(%rax)\nemms",
            Mmx,
        ),
        (
            "psllw_mmx_int_shift_edge_imm16_store",
            "movq 32(%rax), %mm0\npsllw $16, %mm0\nmovq %mm0, 72(%rax)\nemms",
            Mmx,
        ),
        (
            "psrlw_mmx_int_shift_edge_imm15_store",
            "movq 32(%rax), %mm0\npsrlw $15, %mm0\nmovq %mm0, 80(%rax)\nemms",
            Mmx,
        ),
        (
            "psrlw_mmx_int_shift_edge_imm16_store",
            "movq 32(%rax), %mm0\npsrlw $16, %mm0\nmovq %mm0, 88(%rax)\nemms",
            Mmx,
        ),
        (
            "psraw_mmx_int_shift_edge_imm15_store",
            "movq 32(%rax), %mm0\npsraw $15, %mm0\nmovq %mm0, 96(%rax)\nemms",
            Mmx,
        ),
        (
            "psraw_mmx_int_shift_edge_imm16_store",
            "movq 32(%rax), %mm0\npsraw $16, %mm0\nmovq %mm0, 104(%rax)\nemms",
            Mmx,
        ),
        (
            "pslld_mmx_int_shift_edge_imm31_store",
            "movq 32(%rax), %mm0\npslld $31, %mm0\nmovq %mm0, 112(%rax)\nemms",
            Mmx,
        ),
        (
            "pslld_mmx_int_shift_edge_imm32_store",
            "movq 32(%rax), %mm0\npslld $32, %mm0\nmovq %mm0, 120(%rax)\nemms",
            Mmx,
        ),
        (
            "psrld_mmx_int_shift_edge_imm31_store",
            "movq 32(%rax), %mm0\npsrld $31, %mm0\nmovq %mm0, 128(%rax)\nemms",
            Mmx,
        ),
        (
            "psrld_mmx_int_shift_edge_imm32_store",
            "movq 32(%rax), %mm0\npsrld $32, %mm0\nmovq %mm0, 136(%rax)\nemms",
            Mmx,
        ),
        (
            "psrad_mmx_int_shift_edge_imm31_store",
            "movq 32(%rax), %mm0\npsrad $31, %mm0\nmovq %mm0, 144(%rax)\nemms",
            Mmx,
        ),
        (
            "psrad_mmx_int_shift_edge_imm32_store",
            "movq 32(%rax), %mm0\npsrad $32, %mm0\nmovq %mm0, 152(%rax)\nemms",
            Mmx,
        ),
        (
            "psllq_mmx_int_shift_edge_imm63_store",
            "movq 32(%rax), %mm0\npsllq $63, %mm0\nmovq %mm0, 160(%rax)\nemms",
            Mmx,
        ),
        (
            "psllq_mmx_int_shift_edge_imm64_store",
            "movq 32(%rax), %mm0\npsllq $64, %mm0\nmovq %mm0, 168(%rax)\nemms",
            Mmx,
        ),
        (
            "psrlq_mmx_int_shift_edge_imm63_store",
            "movq 32(%rax), %mm0\npsrlq $63, %mm0\nmovq %mm0, 176(%rax)\nemms",
            Mmx,
        ),
        (
            "psrlq_mmx_int_shift_edge_imm64_store",
            "movq 32(%rax), %mm0\npsrlq $64, %mm0\nmovq %mm0, 184(%rax)\nemms",
            Mmx,
        ),
        ("pslldq_sse2_int_shift_edge_imm0", "pslldq $0, %xmm1", Sse2),
        ("pslldq_sse2_int_shift_edge_imm15", "pslldq $15, %xmm1", Sse2),
        ("pslldq_sse2_int_shift_edge_imm16", "pslldq $16, %xmm1", Sse2),
        ("pslldq_sse2_int_shift_edge_imm31", "pslldq $31, %xmm1", Sse2),
        ("psrldq_sse2_int_shift_edge_imm0", "psrldq $0, %xmm1", Sse2),
        ("psrldq_sse2_int_shift_edge_imm15", "psrldq $15, %xmm1", Sse2),
        ("psrldq_sse2_int_shift_edge_imm16", "psrldq $16, %xmm1", Sse2),
        ("psrldq_sse2_int_shift_edge_imm31", "psrldq $31, %xmm1", Sse2),
        ("psllw_sse2_int_shift_edge_xmm_count", "psllw %xmm2, %xmm1", Sse2),
        ("psrlw_sse2_int_shift_edge_xmm_count", "psrlw %xmm2, %xmm1", Sse2),
        ("psraw_sse2_int_shift_edge_xmm_count", "psraw %xmm2, %xmm1", Sse2),
        ("pslld_sse2_int_shift_edge_xmm_count", "pslld %xmm2, %xmm1", Sse2),
        ("psrld_sse2_int_shift_edge_xmm_count", "psrld %xmm2, %xmm1", Sse2),
        ("psrad_sse2_int_shift_edge_xmm_count", "psrad %xmm2, %xmm1", Sse2),
        ("vpsllw_avx2_int_shift_edge_imm15", "{vex} vpsllw $15, %xmm3, %xmm1", Avx2),
        ("vpsllw_avx2_int_shift_edge_imm16", "{vex} vpsllw $16, %xmm3, %xmm1", Avx2),
        ("vpsrlw_avx2_int_shift_edge_imm15", "{vex} vpsrlw $15, %ymm3, %ymm1", Avx2),
        ("vpsrlw_avx2_int_shift_edge_imm16", "{vex} vpsrlw $16, %ymm3, %ymm1", Avx2),
        ("vpsraw_avx2_int_shift_edge_imm15", "{vex} vpsraw $15, %xmm3, %xmm1", Avx2),
        ("vpsraw_avx2_int_shift_edge_imm16", "{vex} vpsraw $16, %ymm3, %ymm1", Avx2),
        ("vpslld_avx2_int_shift_edge_imm31", "{vex} vpslld $31, %ymm3, %ymm1", Avx2),
        ("vpslld_avx2_int_shift_edge_imm32", "{vex} vpslld $32, %xmm3, %xmm1", Avx2),
        ("vpsrld_avx2_int_shift_edge_imm31", "{vex} vpsrld $31, %ymm3, %ymm1", Avx2),
        ("vpsrld_avx2_int_shift_edge_imm32", "{vex} vpsrld $32, %xmm3, %xmm1", Avx2),
        ("vpsrad_avx2_int_shift_edge_imm31", "{vex} vpsrad $31, %ymm3, %ymm1", Avx2),
        ("vpsrad_avx2_int_shift_edge_imm32", "{vex} vpsrad $32, %xmm3, %xmm1", Avx2),
        ("vpsllq_avx2_int_shift_edge_imm63", "{vex} vpsllq $63, %ymm3, %ymm1", Avx2),
        ("vpsllq_avx2_int_shift_edge_imm64", "{vex} vpsllq $64, %xmm3, %xmm1", Avx2),
        ("vpsrlq_avx2_int_shift_edge_imm63", "{vex} vpsrlq $63, %ymm3, %ymm1", Avx2),
        ("vpsrlq_avx2_int_shift_edge_imm64", "{vex} vpsrlq $64, %xmm3, %xmm1", Avx2),
        ("vpsllvd_avx2_int_shift_edge_reg", "{vex} vpsllvd %ymm2, %ymm3, %ymm1", Avx2),
        ("vpsrlvd_avx2_int_shift_edge_mem", "{vex} vpsrlvd 32(%rax), %ymm3, %ymm1", Avx2),
        ("vpsravd_avx2_int_shift_edge_reg", "{vex} vpsravd %ymm2, %ymm3, %ymm1", Avx2),
        ("vpsllvq_avx2_int_shift_edge_mem", "{vex} vpsllvq 32(%rax), %ymm3, %ymm1", Avx2),
        ("vpsrlvq_avx2_int_shift_edge_reg", "{vex} vpsrlvq %ymm2, %ymm3, %ymm1", Avx2),
        ("vpsllw_avx512_int_shift_edge_imm15", "vpsllw $15, %zmm3, %zmm1", Bw),
        ("vpsllw_avx512_int_shift_edge_imm16", "vpsllw $16, %zmm3, %zmm1", Bw),
        ("vpsrlw_avx512_int_shift_edge_imm15", "vpsrlw $15, %zmm3, %zmm1", Bw),
        ("vpsrlw_avx512_int_shift_edge_imm16", "vpsrlw $16, %zmm3, %zmm1", Bw),
        ("vpsraw_avx512_int_shift_edge_imm15", "vpsraw $15, %zmm3, %zmm1", Bw),
        ("vpsraw_avx512_int_shift_edge_imm16", "vpsraw $16, %zmm3, %zmm1", Bw),
        ("vpsllvw_avx512_int_shift_edge_reg", "vpsllvw %zmm2, %zmm3, %zmm1", Bw),
        ("vpsrlvw_avx512_int_shift_edge_mem", "vpsrlvw 64(%rax), %zmm3, %zmm1", Bw),
        ("vpsravw_avx512_int_shift_edge_reg", "vpsravw %zmm2, %zmm3, %zmm1", Bw),
        ("vpslld_avx512_int_shift_edge_imm31", "vpslld $31, %zmm3, %zmm1", F),
        ("vpslld_avx512_int_shift_edge_imm32", "vpslld $32, %zmm3, %zmm1", F),
        ("vpsrld_avx512_int_shift_edge_imm31", "vpsrld $31, %zmm3, %zmm1", F),
        ("vpsrld_avx512_int_shift_edge_imm32", "vpsrld $32, %zmm3, %zmm1", F),
        ("vpsrad_avx512_int_shift_edge_imm31", "vpsrad $31, %zmm3, %zmm1", F),
        ("vpsrad_avx512_int_shift_edge_imm32", "vpsrad $32, %zmm3, %zmm1", F),
        ("vpsllq_avx512_int_shift_edge_imm63", "vpsllq $63, %zmm3, %zmm1", F),
        ("vpsllq_avx512_int_shift_edge_imm64", "vpsllq $64, %zmm3, %zmm1", F),
        ("vpsrlq_avx512_int_shift_edge_imm63", "vpsrlq $63, %zmm3, %zmm1", F),
        ("vpsrlq_avx512_int_shift_edge_imm64", "vpsrlq $64, %zmm3, %zmm1", F),
        ("vpsraq_avx512_int_shift_edge_imm63", "vpsraq $63, %zmm3, %zmm1", F),
        ("vpsraq_avx512_int_shift_edge_imm64", "vpsraq $64, %zmm3, %zmm1", F),
        ("vpsllvd_avx512_int_shift_edge_reg", "vpsllvd %zmm2, %zmm3, %zmm1", F),
        ("vpsrlvd_avx512_int_shift_edge_mem", "vpsrlvd 64(%rax), %zmm3, %zmm1", F),
        ("vpsravd_avx512_int_shift_edge_reg", "vpsravd %zmm2, %zmm3, %zmm1", F),
        ("vpsllvq_avx512_int_shift_edge_mem", "vpsllvq 64(%rax), %zmm3, %zmm1", F),
        ("vpsrlvq_avx512_int_shift_edge_reg", "vpsrlvq %zmm2, %zmm3, %zmm1", F),
        ("vpsravq_avx512_int_shift_edge_mem", "vpsravq 64(%rax), %zmm3, %zmm1", F),
        ("vprold_avx512_int_shift_edge_imm31", "vprold $31, %zmm3, %zmm1", F),
        ("vprold_avx512_int_shift_edge_imm32", "vprold $32, %zmm3, %zmm1", F),
        ("vprord_avx512_int_shift_edge_imm31", "vprord $31, %zmm3, %zmm1", F),
        ("vprord_avx512_int_shift_edge_imm32", "vprord $32, %zmm3, %zmm1", F),
        ("vprolq_avx512_int_shift_edge_imm63", "vprolq $63, %zmm3, %zmm1", F),
        ("vprolq_avx512_int_shift_edge_imm64", "vprolq $64, %zmm3, %zmm1", F),
        ("vprorq_avx512_int_shift_edge_imm63", "vprorq $63, %zmm3, %zmm1", F),
        ("vprorq_avx512_int_shift_edge_imm64", "vprorq $64, %zmm3, %zmm1", F),
        ("vprolvd_avx512_int_shift_edge_reg", "vprolvd %zmm2, %zmm3, %zmm1", F),
        ("vprorvd_avx512_int_shift_edge_mem", "vprorvd 64(%rax), %zmm3, %zmm1", F),
        ("vprolvq_avx512_int_shift_edge_reg", "vprolvq %zmm2, %zmm3, %zmm1", F),
        ("vprorvq_avx512_int_shift_edge_mem", "vprorvq 64(%rax), %zmm3, %zmm1", F),
        ("vpshldw_vbmi2_int_shift_edge_imm15", "vpshldw $15, %zmm2, %zmm3, %zmm1", Vbmi2),
        ("vpshldw_vbmi2_int_shift_edge_imm16", "vpshldw $16, %zmm2, %zmm3, %zmm1", Vbmi2),
        ("vpshrdw_vbmi2_int_shift_edge_imm15", "vpshrdw $15, %zmm2, %zmm3, %zmm1", Vbmi2),
        ("vpshrdw_vbmi2_int_shift_edge_imm16", "vpshrdw $16, %zmm2, %zmm3, %zmm1", Vbmi2),
        ("vpshldd_vbmi2_int_shift_edge_imm31", "vpshldd $31, %zmm2, %zmm3, %zmm1", Vbmi2),
        ("vpshldd_vbmi2_int_shift_edge_imm32", "vpshldd $32, %zmm2, %zmm3, %zmm1", Vbmi2),
        ("vpshrdd_vbmi2_int_shift_edge_imm31", "vpshrdd $31, %zmm2, %zmm3, %zmm1", Vbmi2),
        ("vpshrdd_vbmi2_int_shift_edge_imm32", "vpshrdd $32, %zmm2, %zmm3, %zmm1", Vbmi2),
        ("vpshldvw_vbmi2_int_shift_edge_reg", "vpshldvw %zmm2, %zmm3, %zmm1", Vbmi2),
        ("vpshrdvw_vbmi2_int_shift_edge_mem", "vpshrdvw 64(%rax), %zmm3, %zmm1", Vbmi2),
        ("vpshldvd_vbmi2_int_shift_edge_reg", "vpshldvd %zmm2, %zmm3, %zmm1", Vbmi2),
        ("vpshrdvq_vbmi2_int_shift_edge_mem", "vpshrdvq 64(%rax), %zmm3, %zmm1", Vbmi2),
        ("kshiftlb_int_shift_edge_imm7", "kshiftlb $7, %k2, %k5", Dq),
        ("kshiftlb_int_shift_edge_imm8", "kshiftlb $8, %k2, %k5", Dq),
        ("kshiftrb_int_shift_edge_imm7", "kshiftrb $7, %k2, %k5", Dq),
        ("kshiftrb_int_shift_edge_imm8", "kshiftrb $8, %k2, %k5", Dq),
        ("kshiftlw_int_shift_edge_imm15", "kshiftlw $15, %k2, %k5", F),
        ("kshiftlw_int_shift_edge_imm16", "kshiftlw $16, %k2, %k5", F),
        ("kshiftrw_int_shift_edge_imm15", "kshiftrw $15, %k2, %k5", F),
        ("kshiftrw_int_shift_edge_imm16", "kshiftrw $16, %k2, %k5", F),
        ("kshiftld_int_shift_edge_imm31", "kshiftld $31, %k2, %k5", Dq),
        ("kshiftld_int_shift_edge_imm32", "kshiftld $32, %k2, %k5", Dq),
        ("kshiftrd_int_shift_edge_imm31", "kshiftrd $31, %k2, %k5", Dq),
        ("kshiftrd_int_shift_edge_imm32", "kshiftrd $32, %k2, %k5", Dq),
        ("kshiftlq_int_shift_edge_imm63", "kshiftlq $63, %k2, %k5", Bw),
        ("kshiftlq_int_shift_edge_imm64", "kshiftlq $64, %k2, %k5", Bw),
        ("kshiftrq_int_shift_edge_imm63", "kshiftrq $63, %k2, %k5", Bw),
        ("kshiftrq_int_shift_edge_imm64", "kshiftrq $64, %k2, %k5", Bw),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: IntShiftEdge,
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

    // Legacy SSE4.1 complementary operand forms exercise the alternate
    // register/memory decoder paths for the 0F38 sign/zero-extend, qword
    // compare, and signed/unsigned min/max families.
    for &(label, asm) in &[
        ("pmovsxbw_sse41_operand_mem", "pmovsxbw 32(%rax), %xmm1"),
        ("pmovsxbd_sse41_operand_reg", "pmovsxbd %xmm2, %xmm1"),
        ("pmovsxbq_sse41_operand_mem", "pmovsxbq 32(%rax), %xmm1"),
        ("pmovsxwd_sse41_operand_reg", "pmovsxwd %xmm2, %xmm1"),
        ("pmovsxwq_sse41_operand_mem", "pmovsxwq 32(%rax), %xmm1"),
        ("pmovsxdq_sse41_operand_reg", "pmovsxdq %xmm2, %xmm1"),
        ("pmovzxbw_sse41_operand_mem", "pmovzxbw 32(%rax), %xmm1"),
        ("pmovzxbd_sse41_operand_reg", "pmovzxbd %xmm2, %xmm1"),
        ("pmovzxbq_sse41_operand_mem", "pmovzxbq 32(%rax), %xmm1"),
        ("pmovzxwd_sse41_operand_reg", "pmovzxwd %xmm2, %xmm1"),
        ("pmovzxwq_sse41_operand_mem", "pmovzxwq 32(%rax), %xmm1"),
        ("pmovzxdq_sse41_operand_reg", "pmovzxdq %xmm2, %xmm1"),
        ("pcmpeqq_sse41_operand_mem", "pcmpeqq 32(%rax), %xmm1"),
        ("pminsb_sse41_operand_mem", "pminsb 32(%rax), %xmm1"),
        ("pminsd_sse41_operand_reg", "pminsd %xmm2, %xmm1"),
        ("pminuw_sse41_operand_mem", "pminuw 32(%rax), %xmm1"),
        ("pminud_sse41_operand_reg", "pminud %xmm2, %xmm1"),
        ("pmaxsb_sse41_operand_mem", "pmaxsb 32(%rax), %xmm1"),
        ("pmaxsd_sse41_operand_reg", "pmaxsd %xmm2, %xmm1"),
        ("pmaxuw_sse41_operand_mem", "pmaxuw 32(%rax), %xmm1"),
        ("pmaxud_sse41_operand_reg", "pmaxud %xmm2, %xmm1"),
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

    // SSE4.1 edge forms: PTEST flag outcomes, implicit-XMM0 variable blend
    // masks, all-destination/all-source immediate blends, and INSERTPS zeroing.
    for &(label, asm, profile) in &[
        (
            "ptest_sse41_edge_zero_zero",
            "pxor %xmm1, %xmm1\nptest %xmm1, %xmm1",
            Int,
        ),
        (
            "ptest_sse41_edge_allones_allones",
            "pcmpeqb %xmm1, %xmm1\nptest %xmm1, %xmm1",
            Int,
        ),
        (
            "ptest_sse41_edge_dest_subset",
            "pcmpeqb %xmm1, %xmm1\npxor %xmm2, %xmm2\nptest %xmm2, %xmm1",
            Int,
        ),
        (
            "pblendvb_sse41_edge_zero_mask",
            "pxor %xmm0, %xmm0\npblendvb %xmm2, %xmm1",
            Int,
        ),
        (
            "pblendvb_sse41_edge_allones_mask",
            "pcmpeqb %xmm0, %xmm0\npblendvb %xmm2, %xmm1",
            Int,
        ),
        (
            "pblendvb_sse41_edge_alternating_byte_mask",
            "pcmpeqb %xmm0, %xmm0\npsllw $8, %xmm0\npblendvb %xmm2, %xmm1",
            Int,
        ),
        (
            "blendvps_sse41_edge_zero_mask",
            "pxor %xmm0, %xmm0\nblendvps %xmm2, %xmm1",
            F32,
        ),
        (
            "blendvps_sse41_edge_allones_mask",
            "pcmpeqb %xmm0, %xmm0\nblendvps %xmm2, %xmm1",
            F32,
        ),
        (
            "blendvps_sse41_edge_high_three_lanes_mask",
            "pcmpeqb %xmm0, %xmm0\npslldq $4, %xmm0\nblendvps %xmm2, %xmm1",
            F32,
        ),
        (
            "blendvpd_sse41_edge_zero_mask",
            "pxor %xmm0, %xmm0\nblendvpd %xmm2, %xmm1",
            F64,
        ),
        (
            "blendvpd_sse41_edge_allones_mask",
            "pcmpeqb %xmm0, %xmm0\nblendvpd %xmm2, %xmm1",
            F64,
        ),
        (
            "blendvpd_sse41_edge_high_lane_mask",
            "pcmpeqb %xmm0, %xmm0\npslldq $8, %xmm0\nblendvpd %xmm2, %xmm1",
            F64,
        ),
        ("blendps_sse41_edge_imm0", "blendps $0x00, %xmm2, %xmm1", F32),
        ("blendps_sse41_edge_immf", "blendps $0x0f, %xmm2, %xmm1", F32),
        ("blendpd_sse41_edge_imm0", "blendpd $0x0, %xmm2, %xmm1", F64),
        ("blendpd_sse41_edge_imm3", "blendpd $0x3, %xmm2, %xmm1", F64),
        ("pblendw_sse41_edge_imm0", "pblendw $0x00, %xmm2, %xmm1", Int),
        ("pblendw_sse41_edge_immff", "pblendw $0xff, %xmm2, %xmm1", Int),
        ("insertps_sse41_edge_lane0", "insertps $0x00, %xmm2, %xmm1", F32),
        ("insertps_sse41_edge_zero_all", "insertps $0xff, %xmm2, %xmm1", F32),
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

    // SSE4.2 string compares have unusual control-byte mode selection and
    // implicit EAX/EDX length inputs on the explicit-length forms.
    for &(label, asm) in &[
        (
            "pcmpistri_sse42_string_width_eqany_u8_reg",
            "pcmpistri $0x00, %xmm2, %xmm1",
        ),
        (
            "pcmpistrm_sse42_string_width_ranges_u8_mem",
            "pcmpistrm $0x04, 32(%rax), %xmm1",
        ),
        (
            "pcmpistri_sse42_string_width_eqeach_u8_neg_reg",
            "pcmpistri $0x18, %xmm2, %xmm1",
        ),
        (
            "pcmpistrm_sse42_string_width_eqordered_u8_bitmask_mem",
            "pcmpistrm $0x4c, 48(%rax), %xmm1",
        ),
        (
            "pcmpistri_sse42_string_width_ranges_i8_msb_reg",
            "pcmpistri $0x46, %xmm2, %xmm1",
        ),
        (
            "pcmpistrm_sse42_string_width_eqany_u16_neg_reg",
            "pcmpistrm $0x11, %xmm2, %xmm1",
        ),
        (
            "pcmpistri_sse42_string_width_eqeach_u16_mem",
            "pcmpistri $0x09, 64(%rax), %xmm1",
        ),
        (
            "pcmpistrm_sse42_string_width_eqordered_i16_neg_bitmask_reg",
            "pcmpistrm $0x5f, %xmm2, %xmm1",
        ),
        (
            "pcmpestri_sse42_string_width_eqany_u8_len16_reg",
            "movq %rax, %r10\nmovl $16, %eax\nmovl $16, %edx\npcmpestri $0x00, %xmm2, %xmm1",
        ),
        (
            "pcmpestrm_sse42_string_width_eqany_u8_len16_reg",
            "movq %rax, %r10\nmovl $16, %eax\nmovl $16, %edx\npcmpestrm $0x00, %xmm2, %xmm1",
        ),
        (
            "pcmpestri_sse42_string_width_ranges_u8_len16_mem",
            "movq %rax, %r10\nmovl $16, %eax\nmovl $16, %edx\npcmpestri $0x04, 32(%r10), %xmm1",
        ),
        (
            "pcmpestrm_sse42_string_width_ranges_u8_len16_mem",
            "movq %rax, %r10\nmovl $16, %eax\nmovl $16, %edx\npcmpestrm $0x04, 32(%r10), %xmm1",
        ),
        (
            "pcmpestri_sse42_string_width_eqeach_u8_lenneg_mem",
            "movq %rax, %r10\nmovl $-9, %eax\nmovl $12, %edx\npcmpestri $0x18, 48(%r10), %xmm1",
        ),
        (
            "pcmpestrm_sse42_string_width_eqeach_u8_lenneg_mem",
            "movq %rax, %r10\nmovl $-9, %eax\nmovl $12, %edx\npcmpestrm $0x18, 48(%r10), %xmm1",
        ),
        (
            "pcmpestri_sse42_string_width_eqordered_u8_len8_mem",
            "movq %rax, %r10\nmovl $8, %eax\nmovl $15, %edx\npcmpestri $0x0c, 64(%r10), %xmm1",
        ),
        (
            "pcmpestrm_sse42_string_width_eqordered_u8_len8_bitmask_mem",
            "movq %rax, %r10\nmovl $8, %eax\nmovl $15, %edx\npcmpestrm $0x4c, 64(%r10), %xmm1",
        ),
        (
            "pcmpestri_sse42_string_width_eqany_u16_len8_reg",
            "movq %rax, %r10\nmovl $8, %eax\nmovl $8, %edx\npcmpestri $0x01, %xmm2, %xmm1",
        ),
        (
            "pcmpestrm_sse42_string_width_eqany_u16_len8_bitmask_reg",
            "movq %rax, %r10\nmovl $8, %eax\nmovl $8, %edx\npcmpestrm $0x41, %xmm2, %xmm1",
        ),
        (
            "pcmpestri_sse42_string_width_ranges_i16_lenneg_mem",
            "movq %rax, %r10\nmovl $-4, %eax\nmovl $8, %edx\npcmpestri $0x17, 80(%r10), %xmm1",
        ),
        (
            "pcmpestrm_sse42_string_width_ranges_i16_lenneg_mem",
            "movq %rax, %r10\nmovl $-4, %eax\nmovl $8, %edx\npcmpestrm $0x17, 80(%r10), %xmm1",
        ),
        (
            "pcmpestri_sse42_string_width_eqeach_i16_len4_msb_reg",
            "movq %rax, %r10\nmovl $4, %eax\nmovl $4, %edx\npcmpestri $0x4b, %xmm2, %xmm1",
        ),
        (
            "pcmpestrm_sse42_string_width_eqeach_i16_len4_bitmask_reg",
            "movq %rax, %r10\nmovl $4, %eax\nmovl $4, %edx\npcmpestrm $0x4b, %xmm2, %xmm1",
        ),
        (
            "pcmpestri_sse42_string_width_eqordered_i16_len6_neg_mem",
            "movq %rax, %r10\nmovl $6, %eax\nmovl $6, %edx\npcmpestri $0x1f, 96(%r10), %xmm1",
        ),
        (
            "pcmpestrm_sse42_string_width_eqordered_i16_len6_neg_bitmask_mem",
            "movq %rax, %r10\nmovl $6, %eax\nmovl $6, %edx\npcmpestrm $0x5f, 96(%r10), %xmm1",
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
    for mnem in ["vpdpbusd", "vpdpbusds", "vpdpwssd", "vpdpwssds"] {
        for class in ["xmm", "ymm"] {
            out.push(Case {
                label: format!("{mnem}_vex_avx_vnni_addr_{class}_indexed"),
                asm: format!("{{vex}} {mnem} -32(%rbx,%r9,1), %{class}3, %{class}1"),
                feat: AvxVnni,
                profile: Int,
            });
            out.push(Case {
                label: format!("{mnem}_vex_avx_vnni_addr_{class}_addr32"),
                asm: format!("addr32 {{vex}} {mnem} -32(%rbx,%r9,1), %{class}3, %{class}1"),
                feat: AvxVnni,
                profile: Int,
            });
            out.push(Case {
                label: format!("{mnem}_vex_avx_vnni_addr_{class}_high_indexed"),
                asm: format!("{{vex}} {mnem} -32(%rbx,%r9,1), %{class}11, %{class}9"),
                feat: AvxVnni,
                profile: Int,
            });
        }
    }

    // BITALG/VPOPCNTDQ edge operands. Zero and all-one vectors exercise exact
    // population-count results across every element width, and VPSHUFBITQMB
    // gets explicit zero/all-one selector vectors.
    for &(label, asm, feat) in &[
        (
            "vpopcntb_bitalg_popcnt_edge_zero_reg",
            "vpxord %zmm3, %zmm3, %zmm3\nvpopcntb %zmm3, %zmm1",
            Bitalg,
        ),
        (
            "vpopcntb_bitalg_popcnt_edge_allones_mem",
            "vpternlogd $0xff, %zmm3, %zmm3, %zmm3\nvmovdqu64 %zmm3, 64(%rax)\nvpopcntb 64(%rax), %zmm1",
            Bitalg,
        ),
        (
            "vpopcntw_bitalg_popcnt_edge_zero_mem",
            "vpxord %zmm3, %zmm3, %zmm3\nvmovdqu64 %zmm3, 64(%rax)\nvpopcntw 64(%rax), %zmm1",
            Bitalg,
        ),
        (
            "vpopcntw_bitalg_popcnt_edge_allones_reg",
            "vpternlogd $0xff, %zmm3, %zmm3, %zmm3\nvpopcntw %zmm3, %zmm1",
            Bitalg,
        ),
        (
            "vpshufbitqmb_bitalg_popcnt_edge_zero_selector",
            "vpxord %zmm2, %zmm2, %zmm2\nvpshufbitqmb %zmm2, %zmm3, %k5",
            Bitalg,
        ),
        (
            "vpshufbitqmb_bitalg_popcnt_edge_allones_selector",
            "vpternlogd $0xff, %zmm2, %zmm2, %zmm2\nvpshufbitqmb %zmm2, %zmm3, %k5",
            Bitalg,
        ),
        (
            "vpopcntd_vpopcntdq_popcnt_edge_zero_reg",
            "vpxord %zmm3, %zmm3, %zmm3\nvpopcntd %zmm3, %zmm1",
            Vpopcntdq,
        ),
        (
            "vpopcntd_vpopcntdq_popcnt_edge_allones_mem",
            "vpternlogd $0xff, %zmm3, %zmm3, %zmm3\nvmovdqu64 %zmm3, 64(%rax)\nvpopcntd 64(%rax), %zmm1",
            Vpopcntdq,
        ),
        (
            "vpopcntq_vpopcntdq_popcnt_edge_zero_mem",
            "vpxord %zmm3, %zmm3, %zmm3\nvmovdqu64 %zmm3, 64(%rax)\nvpopcntq 64(%rax), %zmm1",
            Vpopcntdq,
        ),
        (
            "vpopcntq_vpopcntdq_popcnt_edge_allones_reg",
            "vpternlogd $0xff, %zmm3, %zmm3, %zmm3\nvpopcntq %zmm3, %zmm1",
            Vpopcntdq,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }

    // VBMI selector edge operands and VBMI2 qword funnel-shift boundaries.
    // These hit zero/all-one byte selectors and the qword count limits that
    // are easy to miss when only the default generated immediates are used.
    for &(label, asm, feat) in &[
        (
            "vpermb_vbmi_selector_edge_zero_index_reg",
            "vpxord %zmm2, %zmm2, %zmm2\nvpermb %zmm3, %zmm2, %zmm1",
            Vbmi,
        ),
        (
            "vpermb_vbmi_selector_edge_allones_index_mem",
            "vpternlogd $0xff, %zmm2, %zmm2, %zmm2\nvpermb 64(%rax), %zmm2, %zmm1",
            Vbmi,
        ),
        (
            "vpermi2b_vbmi_selector_edge_zero_index_reg",
            "vpxord %zmm1, %zmm1, %zmm1\nvpermi2b %zmm3, %zmm2, %zmm1",
            Vbmi,
        ),
        (
            "vpermi2b_vbmi_selector_edge_allones_index_mem",
            "vpternlogd $0xff, %zmm1, %zmm1, %zmm1\nvpermi2b 64(%rax), %zmm2, %zmm1",
            Vbmi,
        ),
        (
            "vpermt2b_vbmi_selector_edge_zero_index_reg",
            "vpxord %zmm1, %zmm1, %zmm1\nvpermt2b %zmm3, %zmm2, %zmm1",
            Vbmi,
        ),
        (
            "vpermt2b_vbmi_selector_edge_allones_index_mem",
            "vpternlogd $0xff, %zmm1, %zmm1, %zmm1\nvpermt2b 64(%rax), %zmm2, %zmm1",
            Vbmi,
        ),
        (
            "vpmultishiftqb_vbmi_selector_edge_zero_control_reg",
            "vpxord %zmm2, %zmm2, %zmm2\nvpmultishiftqb %zmm3, %zmm2, %zmm1",
            Vbmi,
        ),
        (
            "vpmultishiftqb_vbmi_selector_edge_allones_control_mem",
            "vpternlogd $0xff, %zmm2, %zmm2, %zmm2\nvpmultishiftqb 64(%rax), %zmm2, %zmm1",
            Vbmi,
        ),
        (
            "vpshldq_vbmi2_selector_edge_imm63",
            "vpshldq $63, %zmm2, %zmm3, %zmm1",
            Vbmi2,
        ),
        (
            "vpshldq_vbmi2_selector_edge_imm64",
            "vpshldq $64, %zmm2, %zmm3, %zmm1",
            Vbmi2,
        ),
        (
            "vpshrdq_vbmi2_selector_edge_imm63",
            "vpshrdq $63, %zmm2, %zmm3, %zmm1",
            Vbmi2,
        ),
        (
            "vpshrdq_vbmi2_selector_edge_imm64",
            "vpshrdq $64, %zmm2, %zmm3, %zmm1",
            Vbmi2,
        ),
        (
            "vpshldvq_vbmi2_selector_edge_zero_count",
            "vpxord %zmm2, %zmm2, %zmm2\nvpshldvq %zmm2, %zmm3, %zmm1",
            Vbmi2,
        ),
        (
            "vpshrdvq_vbmi2_selector_edge_allones_count",
            "vpternlogd $0xff, %zmm2, %zmm2, %zmm2\nvpshrdvq %zmm2, %zmm3, %zmm1",
            Vbmi2,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }

    // AVX-512 VNNI and IFMA accumulator edge operands. Zero sources make the
    // accumulator behavior exact, while all-one memory sources exercise the
    // saturating and high-half multiply-add paths with dense input bits.
    for &(label, asm, feat) in &[
        (
            "vpdpbusd_vnni_ifma_edge_zero_acc_src",
            "vpxord %zmm1, %zmm1, %zmm1\nvpxord %zmm2, %zmm2, %zmm2\nvpdpbusd %zmm2, %zmm3, %zmm1",
            Vnni,
        ),
        (
            "vpdpbusds_vnni_ifma_edge_allones_acc_mem",
            "vpternlogd $0xff, %zmm1, %zmm1, %zmm1\nvpternlogd $0xff, %zmm2, %zmm2, %zmm2\nvmovdqu64 %zmm2, 64(%rax)\nvpdpbusds 64(%rax), %zmm3, %zmm1",
            Vnni,
        ),
        (
            "vpdpwssd_vnni_ifma_edge_zero_acc_src",
            "vpxord %zmm1, %zmm1, %zmm1\nvpxord %zmm2, %zmm2, %zmm2\nvpdpwssd %zmm2, %zmm3, %zmm1",
            Vnni,
        ),
        (
            "vpdpwssds_vnni_ifma_edge_allones_acc_mem",
            "vpternlogd $0xff, %zmm1, %zmm1, %zmm1\nvpternlogd $0xff, %zmm2, %zmm2, %zmm2\nvmovdqu64 %zmm2, 64(%rax)\nvpdpwssds 64(%rax), %zmm3, %zmm1",
            Vnni,
        ),
        (
            "vpmadd52luq_vnni_ifma_edge_zero_acc_src",
            "vpxorq %zmm1, %zmm1, %zmm1\nvpxorq %zmm2, %zmm2, %zmm2\nvpmadd52luq %zmm2, %zmm3, %zmm1",
            Ifma,
        ),
        (
            "vpmadd52luq_vnni_ifma_edge_allones_acc_mem",
            "vpternlogq $0xff, %zmm1, %zmm1, %zmm1\nvpternlogq $0xff, %zmm2, %zmm2, %zmm2\nvmovdqu64 %zmm2, 64(%rax)\nvpmadd52luq 64(%rax), %zmm3, %zmm1",
            Ifma,
        ),
        (
            "vpmadd52huq_vnni_ifma_edge_zero_acc_src",
            "vpxorq %zmm1, %zmm1, %zmm1\nvpxorq %zmm2, %zmm2, %zmm2\nvpmadd52huq %zmm2, %zmm3, %zmm1",
            Ifma,
        ),
        (
            "vpmadd52huq_vnni_ifma_edge_allones_acc_mem",
            "vpternlogq $0xff, %zmm1, %zmm1, %zmm1\nvpternlogq $0xff, %zmm2, %zmm2, %zmm2\nvmovdqu64 %zmm2, 64(%rax)\nvpmadd52huq 64(%rax), %zmm3, %zmm1",
            Ifma,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
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

    // VEX permute/blend edge selectors: all-destination/all-source blend
    // immediates, zero/all-one variable blend masks, lane-zeroing 128-bit
    // permutes, duplicated permute selectors, and PALIGNR boundary counts.
    for &(label, asm, feat, profile) in &[
        (
            "vblendps_vex_perm_blend_edge_imm0",
            "{vex} vblendps $0x00, %ymm2, %ymm3, %ymm1",
            Avx,
            F32,
        ),
        (
            "vblendps_vex_perm_blend_edge_immff_mem",
            "{vex} vblendps $0xff, 32(%rax), %ymm3, %ymm1",
            Avx,
            F32,
        ),
        (
            "vblendpd_vex_perm_blend_edge_imm0",
            "{vex} vblendpd $0x0, %ymm2, %ymm3, %ymm1",
            Avx,
            F64,
        ),
        (
            "vblendpd_vex_perm_blend_edge_immf_mem",
            "{vex} vblendpd $0xf, 32(%rax), %ymm3, %ymm1",
            Avx,
            F64,
        ),
        (
            "vblendvps_vex_perm_blend_edge_zero_mask",
            "{vex} vxorps %xmm4, %xmm4, %xmm4\n{vex} vblendvps %xmm4, %xmm2, %xmm3, %xmm1",
            Avx,
            F32,
        ),
        (
            "vblendvps_vex_perm_blend_edge_allones_mask",
            "{vex} vxorps %xmm4, %xmm4, %xmm4\n{vex} vcmpeqps %xmm4, %xmm4, %xmm4\n{vex} vblendvps %xmm4, %xmm2, %xmm3, %xmm1",
            Avx,
            F32,
        ),
        (
            "vblendvps_vex_perm_blend_edge_high_three_lanes_mask",
            "{vex} vxorps %xmm4, %xmm4, %xmm4\n{vex} vcmpeqps %xmm4, %xmm4, %xmm4\n{vex} vpslldq $4, %xmm4, %xmm4\n{vex} vblendvps %xmm4, %xmm2, %xmm3, %xmm1",
            Avx,
            F32,
        ),
        (
            "vblendvpd_vex_perm_blend_edge_zero_mask",
            "{vex} vxorpd %ymm4, %ymm4, %ymm4\n{vex} vblendvpd %ymm4, 32(%rax), %ymm3, %ymm1",
            Avx,
            F64,
        ),
        (
            "vblendvpd_vex_perm_blend_edge_allones_mask",
            "{vex} vxorpd %ymm4, %ymm4, %ymm4\n{vex} vcmpeqpd %ymm4, %ymm4, %ymm4\n{vex} vblendvpd %ymm4, 32(%rax), %ymm3, %ymm1",
            Avx,
            F64,
        ),
        (
            "vblendvpd_vex_perm_blend_edge_high_lanes_mask",
            "{vex} vxorpd %ymm4, %ymm4, %ymm4\n{vex} vcmpeqpd %ymm4, %ymm4, %ymm4\n{vex} vpslldq $8, %ymm4, %ymm4\n{vex} vblendvpd %ymm4, 32(%rax), %ymm3, %ymm1",
            Avx,
            F64,
        ),
        (
            "vperm2f128_vex_perm_blend_edge_zero_low",
            "{vex} vperm2f128 $0x08, %ymm2, %ymm3, %ymm1",
            Avx,
            F32,
        ),
        (
            "vperm2f128_vex_perm_blend_edge_zero_high",
            "{vex} vperm2f128 $0x80, %ymm2, %ymm3, %ymm1",
            Avx,
            F32,
        ),
        (
            "vperm2f128_vex_perm_blend_edge_zero_both",
            "{vex} vperm2f128 $0x88, %ymm2, %ymm3, %ymm1",
            Avx,
            F32,
        ),
        (
            "vpermilps_vex_perm_blend_edge_dup0",
            "{vex} vpermilps $0x00, %ymm3, %ymm1",
            Avx,
            F32,
        ),
        (
            "vpermilps_vex_perm_blend_edge_dup3_mem",
            "{vex} vpermilps $0xff, 32(%rax), %ymm1",
            Avx,
            F32,
        ),
        (
            "vpermilpd_vex_perm_blend_edge_dup0",
            "{vex} vpermilpd $0x0, %ymm3, %ymm1",
            Avx,
            F64,
        ),
        (
            "vpermilpd_vex_perm_blend_edge_dup1_mem",
            "{vex} vpermilpd $0xf, 32(%rax), %ymm1",
            Avx,
            F64,
        ),
        (
            "vpalignr_vex_perm_blend_edge_imm0",
            "{vex} vpalignr $0, %xmm2, %xmm3, %xmm1",
            Avx2,
            Int,
        ),
        (
            "vpalignr_vex_perm_blend_edge_imm16",
            "{vex} vpalignr $16, %xmm2, %xmm3, %xmm1",
            Avx2,
            Int,
        ),
        (
            "vpalignr_vex_perm_blend_edge_imm31_mem",
            "{vex} vpalignr $31, 32(%rax), %ymm3, %ymm1",
            Avx2,
            Int,
        ),
        (
            "vpalignr_vex_perm_blend_edge_imm32_mem",
            "{vex} vpalignr $32, 32(%rax), %ymm3, %ymm1",
            Avx2,
            Int,
        ),
        (
            "vperm2i128_vex_perm_blend_edge_zero_low",
            "{vex} vperm2i128 $0x08, %ymm2, %ymm3, %ymm1",
            Avx2,
            Int,
        ),
        (
            "vperm2i128_vex_perm_blend_edge_zero_high",
            "{vex} vperm2i128 $0x80, %ymm2, %ymm3, %ymm1",
            Avx2,
            Int,
        ),
        (
            "vperm2i128_vex_perm_blend_edge_zero_both",
            "{vex} vperm2i128 $0x88, %ymm2, %ymm3, %ymm1",
            Avx2,
            Int,
        ),
        (
            "vpermq_vex_perm_blend_edge_dup0",
            "{vex} vpermq $0x00, %ymm3, %ymm1",
            Avx2,
            Int,
        ),
        (
            "vpermq_vex_perm_blend_edge_dup3_mem",
            "{vex} vpermq $0xff, 32(%rax), %ymm1",
            Avx2,
            Int,
        ),
        (
            "vpermd_vex_perm_blend_edge_index_zero",
            "{vex} vpxor %ymm2, %ymm2, %ymm2\n{vex} vpermd %ymm3, %ymm2, %ymm1",
            Avx2,
            Int,
        ),
        (
            "vpermd_vex_perm_blend_edge_index_allones_mem",
            "{vex} vpcmpeqd %ymm2, %ymm2, %ymm2\n{vex} vpermd 32(%rax), %ymm2, %ymm1",
            Avx2,
            Int,
        ),
        (
            "vpermps_vex_perm_blend_edge_avx2_selector_zero",
            "{vex} vpxor %ymm2, %ymm2, %ymm2\n{vex} vpermps %ymm3, %ymm2, %ymm1",
            Avx2,
            F32,
        ),
        (
            "vpermps_vex_perm_blend_edge_avx2_selector_allones_mem",
            "{vex} vpcmpeqd %ymm2, %ymm2, %ymm2\n{vex} vpermps 32(%rax), %ymm2, %ymm1",
            Avx2,
            F32,
        ),
        (
            "vpermps_vex_perm_blend_edge_avx2_selector_high_regs",
            "{vex} vpermps %ymm11, %ymm10, %ymm9",
            Avx2,
            F32,
        ),
        (
            "vpermd_vex_perm_blend_edge_avx2_selector_high_regs",
            "{vex} vpermd %ymm11, %ymm10, %ymm9",
            Avx2,
            Int,
        ),
        (
            "vpermpd_vex_perm_blend_edge_avx2_selector_dup0",
            "{vex} vpermpd $0x00, %ymm3, %ymm1",
            Avx2,
            F64,
        ),
        (
            "vpermpd_vex_perm_blend_edge_avx2_selector_dup3_mem",
            "{vex} vpermpd $0xff, 32(%rax), %ymm1",
            Avx2,
            F64,
        ),
        (
            "vpermpd_vex_perm_blend_edge_avx2_selector_cross_lane",
            "{vex} vpermpd $0x4e, %ymm3, %ymm1",
            Avx2,
            F64,
        ),
        (
            "vpermq_vex_perm_blend_edge_avx2_selector_cross_lane",
            "{vex} vpermq $0x4e, %ymm3, %ymm1",
            Avx2,
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

    // VEX AVX data movement and lane forms that sit outside the arithmetic and
    // conversion tables above. These cover aligned/unaligned integer vector
    // moves, LDDQU, low/high scalar-pair loads and stores, and SSE4.1 insert /
    // extract forms in their VEX encodings.
    for &(label, asm, profile) in &[
        (
            "vmovdqa_avx_data_xmm_reg",
            "{vex} vmovdqa %xmm2, %xmm1",
            Int,
        ),
        (
            "vmovdqa_avx_data_ymm_reg",
            "{vex} vmovdqa %ymm2, %ymm1",
            Int,
        ),
        (
            "vmovdqa_avx_data_xmm_load",
            "{vex} vmovdqa 32(%rax), %xmm1",
            Int,
        ),
        (
            "vmovdqa_avx_data_ymm_load",
            "{vex} vmovdqa 64(%rax), %ymm1",
            Int,
        ),
        (
            "vmovdqa_avx_data_xmm_store",
            "{vex} vmovdqa %xmm1, 96(%rax)",
            Int,
        ),
        (
            "vmovdqa_avx_data_ymm_store",
            "{vex} vmovdqa %ymm1, 128(%rax)",
            Int,
        ),
        (
            "vmovdqu_avx_data_xmm_reg",
            "{vex} vmovdqu %xmm2, %xmm1",
            Int,
        ),
        (
            "vmovdqu_avx_data_ymm_reg",
            "{vex} vmovdqu %ymm2, %ymm1",
            Int,
        ),
        (
            "vmovdqu_avx_data_xmm_load_unaligned",
            "{vex} vmovdqu 17(%rax), %xmm1",
            Int,
        ),
        (
            "vmovdqu_avx_data_ymm_load_unaligned",
            "{vex} vmovdqu 33(%rax), %ymm1",
            Int,
        ),
        (
            "vmovdqu_avx_data_xmm_store_unaligned",
            "{vex} vmovdqu %xmm1, 17(%rax)",
            Int,
        ),
        (
            "vmovdqu_avx_data_ymm_store_unaligned",
            "{vex} vmovdqu %ymm1, 33(%rax)",
            Int,
        ),
        (
            "vlddqu_avx_data_xmm_load_unaligned",
            "{vex} vlddqu 17(%rax), %xmm1",
            Int,
        ),
        (
            "vlddqu_avx_data_ymm_load_unaligned",
            "{vex} vlddqu 33(%rax), %ymm1",
            Int,
        ),
        (
            "vmovsldup_avx_data_edge_xmm_self_zero_upper",
            "{vex} vmovsldup %xmm1, %xmm1",
            F32,
        ),
        (
            "vmovshdup_avx_data_edge_ymm_high_regs",
            "{vex} vmovshdup %ymm10, %ymm9",
            F32,
        ),
        (
            "vmovddup_avx_data_edge_xmm_reg_zero_upper",
            "{vex} vmovddup %xmm3, %xmm1",
            F64,
        ),
        (
            "vmovddup_avx_data_edge_ymm_reg_high_lanes",
            "{vex} vmovddup %ymm3, %ymm1",
            F64,
        ),
        (
            "vmovddup_avx_data_edge_xmm_unaligned_mem",
            "{vex} vmovddup 7(%rax), %xmm1",
            F64,
        ),
        (
            "vmovddup_avx_data_edge_ymm_unaligned_mem",
            "{vex} vmovddup 33(%rax), %ymm1",
            F64,
        ),
        (
            "vlddqu_avx_data_edge_xmm_boundary_high_dest",
            "{vex} vlddqu 63(%rax), %xmm15",
            Int,
        ),
        (
            "vlddqu_avx_data_edge_ymm_unaligned_1",
            "{vex} vlddqu 1(%rax), %ymm1",
            Int,
        ),
        (
            "vlddqu_avx_data_edge_ymm_high_dest",
            "{vex} vlddqu 65(%rax), %ymm15",
            Int,
        ),
        (
            "vmovlps_avx_data_load",
            "{vex} vmovlps 32(%rax), %xmm3, %xmm1",
            F32,
        ),
        (
            "vmovlps_avx_data_store",
            "{vex} vmovlps %xmm1, 48(%rax)",
            F32,
        ),
        (
            "vmovhps_avx_data_load",
            "{vex} vmovhps 32(%rax), %xmm3, %xmm1",
            F32,
        ),
        (
            "vmovhps_avx_data_store",
            "{vex} vmovhps %xmm1, 56(%rax)",
            F32,
        ),
        (
            "vmovlhps_avx_data_reg",
            "{vex} vmovlhps %xmm2, %xmm3, %xmm1",
            F32,
        ),
        (
            "vmovhlps_avx_data_reg",
            "{vex} vmovhlps %xmm2, %xmm3, %xmm1",
            F32,
        ),
        (
            "vmovlpd_avx_data_load",
            "{vex} vmovlpd 32(%rax), %xmm3, %xmm1",
            F64,
        ),
        (
            "vmovlpd_avx_data_store",
            "{vex} vmovlpd %xmm1, 48(%rax)",
            F64,
        ),
        (
            "vmovhpd_avx_data_load",
            "{vex} vmovhpd 32(%rax), %xmm3, %xmm1",
            F64,
        ),
        (
            "vmovhpd_avx_data_store",
            "{vex} vmovhpd %xmm1, 56(%rax)",
            F64,
        ),
        (
            "vmovss_avx_data_l1_reg_raw",
            ".byte 0xc5,0xf6,0x10,0xca\n",
            F32,
        ),
        (
            "vmovsd_avx_data_l1_reg_raw",
            ".byte 0xc5,0xf7,0x10,0xca\n",
            F64,
        ),
        (
            "vmovss_avx_data_l1_load_raw",
            ".byte 0xc5,0xfe,0x10,0x48,0x30\n",
            F32,
        ),
        (
            "vmovsd_avx_data_l1_load_raw",
            ".byte 0xc5,0xff,0x10,0x48,0x38\n",
            F64,
        ),
        (
            "vmovss_avx_data_l1_store_raw",
            ".byte 0xc5,0xfe,0x11,0x48,0x30\n",
            F32,
        ),
        (
            "vmovsd_avx_data_l1_store_raw",
            ".byte 0xc5,0xff,0x11,0x48,0x38\n",
            F64,
        ),
        (
            "vinsertps_avx_data_reg",
            "{vex} vinsertps $0x20, %xmm2, %xmm3, %xmm1",
            F32,
        ),
        (
            "vinsertps_avx_data_mem",
            "{vex} vinsertps $0x30, 32(%rax), %xmm3, %xmm1",
            F32,
        ),
        (
            "vextractps_avx_data_r8d",
            "{vex} vextractps $2, %xmm1, %r8d",
            F32,
        ),
        (
            "vextractps_avx_data_mem",
            "{vex} vextractps $3, %xmm1, 44(%rax)",
            F32,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Avx,
            profile,
        });
    }

    // AVX VEX integer lane insert/extract forms cover the 0F C4/C5 word
    // encodings plus the 0F3A byte/dword/qword register and memory forms.
    for &(label, asm) in &[
        (
            "vpextrb_avx_insert_extract_r8d",
            "{vex} vpextrb $10, %xmm1, %r8d",
        ),
        (
            "vpextrb_avx_insert_extract_mem",
            "{vex} vpextrb $5, %xmm1, 32(%rax)",
        ),
        (
            "vpextrw_avx_insert_extract_r8d",
            "{vex} vpextrw $4, %xmm1, %r8d",
        ),
        (
            "vpextrw_avx_insert_extract_mem",
            "{vex} vpextrw $4, %xmm1, 34(%rax)",
        ),
        (
            "vpextrd_avx_insert_extract_r8d",
            "{vex} vpextrd $2, %xmm1, %r8d",
        ),
        (
            "vpextrd_avx_insert_extract_mem",
            "{vex} vpextrd $1, %xmm1, 40(%rax)",
        ),
        (
            "vpextrq_avx_insert_extract_r8",
            "{vex} vpextrq $1, %xmm1, %r8",
        ),
        (
            "vpextrq_avx_insert_extract_mem",
            "{vex} vpextrq $0, %xmm1, 48(%rax)",
        ),
        (
            "vpinsrb_avx_insert_extract_r8d",
            "{vex} vpinsrb $14, %r8d, %xmm3, %xmm1",
        ),
        (
            "vpinsrb_avx_insert_extract_mem",
            "{vex} vpinsrb $5, 31(%rax), %xmm3, %xmm1",
        ),
        (
            "vpinsrw_avx_insert_extract_r8d",
            "{vex} vpinsrw $3, %r8d, %xmm3, %xmm1",
        ),
        (
            "vpinsrw_avx_insert_extract_mem",
            "{vex} vpinsrw $6, 30(%rax), %xmm3, %xmm1",
        ),
        (
            "vpinsrd_avx_insert_extract_r8d",
            "{vex} vpinsrd $2, %r8d, %xmm3, %xmm1",
        ),
        (
            "vpinsrd_avx_insert_extract_mem",
            "{vex} vpinsrd $1, 28(%rax), %xmm3, %xmm1",
        ),
        (
            "vpinsrq_avx_insert_extract_r8",
            "{vex} vpinsrq $1, %r8, %xmm3, %xmm1",
        ),
        (
            "vpinsrq_avx_insert_extract_mem",
            "{vex} vpinsrq $0, 24(%rax), %xmm3, %xmm1",
        ),
        (
            "vpextrb_avx_insert_extract_w1_r8d_raw",
            ".byte 0xc4,0xc3,0xf9,0x14,0xc8,0x0a\n",
        ),
        (
            "vpextrb_avx_insert_extract_w1_mem_raw",
            ".byte 0xc4,0xe3,0xf9,0x14,0x48,0x20,0x05\n",
        ),
        (
            "vpextrw_avx_insert_extract_0f_w1_eax_raw",
            ".byte 0xc4,0xe1,0xf9,0xc5,0xc1,0x04\n",
        ),
        (
            "vpextrw_avx_insert_extract_0f3a_w1_eax_raw",
            ".byte 0xc4,0xe3,0xf9,0x15,0xc8,0x04\n",
        ),
        (
            "vpextrw_avx_insert_extract_0f3a_w1_mem_raw",
            ".byte 0xc4,0xe3,0xf9,0x15,0x48,0x22,0x04\n",
        ),
        (
            "vpinsrb_avx_insert_extract_w1_r8d_raw",
            ".byte 0xc4,0xc3,0xe1,0x20,0xc8,0x0e\n",
        ),
        (
            "vpinsrb_avx_insert_extract_w1_mem_raw",
            ".byte 0xc4,0xe3,0xe1,0x20,0x48,0x1f,0x05\n",
        ),
        (
            "vpinsrw_avx_insert_extract_w1_eax_raw",
            ".byte 0xc4,0xe1,0xe1,0xc4,0xc8,0x03\n",
        ),
        (
            "vpinsrw_avx_insert_extract_w1_mem_raw",
            ".byte 0xc4,0xe1,0xe1,0xc4,0x48,0x1e,0x06\n",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Avx,
            profile: Int,
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

    // Complementary AVX2 VEX integer operand forms. These target already
    // implemented 0F38 horizontal, sign/absolute, extend, compare, pack, and
    // flag-setting paths that were only lightly represented by the base AVX2
    // corpus.
    for &(label, asm) in &[
        (
            "vphaddw_avx2_int_operand_xmm_reg",
            "{vex} vphaddw %xmm2, %xmm3, %xmm1",
        ),
        (
            "vphaddd_avx2_int_operand_ymm_mem",
            "{vex} vphaddd 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vphaddsw_avx2_int_operand_xmm_reg",
            "{vex} vphaddsw %xmm2, %xmm3, %xmm1",
        ),
        (
            "vphsubw_avx2_int_operand_ymm_mem",
            "{vex} vphsubw 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vphsubd_avx2_int_operand_ymm_reg",
            "{vex} vphsubd %ymm2, %ymm3, %ymm1",
        ),
        (
            "vphsubsw_avx2_int_operand_xmm_mem",
            "{vex} vphsubsw 32(%rax), %xmm3, %xmm1",
        ),
        (
            "vpmulhrsw_avx2_int_operand_xmm_reg",
            "{vex} vpmulhrsw %xmm2, %xmm3, %xmm1",
        ),
        (
            "vpmulhrsw_avx2_int_operand_ymm_mem",
            "{vex} vpmulhrsw 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vptest_avx2_int_operand_xmm_reg",
            "{vex} vptest %xmm2, %xmm1",
        ),
        (
            "vptest_avx2_int_operand_ymm_mem",
            "{vex} vptest 32(%rax), %ymm1",
        ),
        (
            "vphminposuw_avx2_int_operand_xmm_reg",
            "{vex} vphminposuw %xmm2, %xmm1",
        ),
        (
            "vphminposuw_avx2_int_operand_xmm_mem",
            "{vex} vphminposuw 32(%rax), %xmm1",
        ),
        (
            "vpmovsxbw_avx2_int_operand_ymm_reg",
            "{vex} vpmovsxbw %xmm2, %ymm1",
        ),
        (
            "vpmovsxbd_avx2_int_operand_ymm_mem",
            "{vex} vpmovsxbd 32(%rax), %ymm1",
        ),
        (
            "vpmovzxbw_avx2_int_operand_ymm_reg",
            "{vex} vpmovzxbw %xmm2, %ymm1",
        ),
        (
            "vpmovzxdq_avx2_int_operand_ymm_mem",
            "{vex} vpmovzxdq 32(%rax), %ymm1",
        ),
        (
            "vpcmpeqq_avx2_int_operand_ymm_reg",
            "{vex} vpcmpeqq %ymm2, %ymm3, %ymm1",
        ),
        (
            "vpcmpgtq_avx2_int_operand_ymm_reg",
            "{vex} vpcmpgtq %ymm2, %ymm3, %ymm1",
        ),
        (
            "vpackusdw_avx2_int_operand_ymm_reg",
            "{vex} vpackusdw %ymm2, %ymm3, %ymm1",
        ),
        (
            "vpackusdw_avx2_int_operand_ymm_mem",
            "{vex} vpackusdw 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vpshufb_avx2_int_operand_ymm_mem",
            "{vex} vpshufb 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vpabsb_avx2_int_operand_xmm_mem",
            "{vex} vpabsb 32(%rax), %xmm1",
        ),
        (
            "vpabsw_avx2_int_operand_xmm_reg",
            "{vex} vpabsw %xmm3, %xmm1",
        ),
        (
            "vpabsd_avx2_int_operand_xmm_mem",
            "{vex} vpabsd 32(%rax), %xmm1",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Avx2,
            profile: Int,
        });
    }

    // VEX AVX2 gathers are VSIB memory operations with an architecturally
    // cleared mask register. Each case creates all-zero indices and an all-one
    // mask, then gathers from a fixed scratch displacement so both destination
    // data and mask side effects are compared against KVM.
    for &(label, asm, profile) in &[
        (
            "vgatherdps_avx2_gather_xmm_i32",
            "{vex} vpxor %xmm2, %xmm2, %xmm2\n{vex} vpcmpeqd %xmm3, %xmm3, %xmm3\n{vex} vgatherdps %xmm3, 64(%rax,%xmm2,4), %xmm1",
            F32,
        ),
        (
            "vgatherdps_avx2_gather_ymm_i32",
            "{vex} vpxor %ymm2, %ymm2, %ymm2\n{vex} vpcmpeqd %ymm3, %ymm3, %ymm3\n{vex} vgatherdps %ymm3, 64(%rax,%ymm2,4), %ymm1",
            F32,
        ),
        (
            "vgatherdpd_avx2_gather_xmm_i32",
            "{vex} vpxor %xmm2, %xmm2, %xmm2\n{vex} vpcmpeqq %xmm3, %xmm3, %xmm3\n{vex} vgatherdpd %xmm3, 64(%rax,%xmm2,8), %xmm1",
            F64,
        ),
        (
            "vgatherdpd_avx2_gather_ymm_i32",
            "{vex} vpxor %xmm2, %xmm2, %xmm2\n{vex} vpcmpeqq %ymm3, %ymm3, %ymm3\n{vex} vgatherdpd %ymm3, 64(%rax,%xmm2,8), %ymm1",
            F64,
        ),
        (
            "vgatherqps_avx2_gather_xmm_i64",
            "{vex} vpxor %xmm2, %xmm2, %xmm2\n{vex} vpcmpeqd %xmm3, %xmm3, %xmm3\n{vex} vgatherqps %xmm3, 64(%rax,%xmm2,4), %xmm1",
            F32,
        ),
        (
            "vgatherqps_avx2_gather_ymm_i64",
            "{vex} vpxor %ymm2, %ymm2, %ymm2\n{vex} vpcmpeqd %xmm3, %xmm3, %xmm3\n{vex} vgatherqps %xmm3, 64(%rax,%ymm2,4), %xmm1",
            F32,
        ),
        (
            "vgatherqpd_avx2_gather_xmm_i64",
            "{vex} vpxor %xmm2, %xmm2, %xmm2\n{vex} vpcmpeqq %xmm3, %xmm3, %xmm3\n{vex} vgatherqpd %xmm3, 64(%rax,%xmm2,8), %xmm1",
            F64,
        ),
        (
            "vgatherqpd_avx2_gather_ymm_i64",
            "{vex} vpxor %ymm2, %ymm2, %ymm2\n{vex} vpcmpeqq %ymm3, %ymm3, %ymm3\n{vex} vgatherqpd %ymm3, 64(%rax,%ymm2,8), %ymm1",
            F64,
        ),
        (
            "vpgatherdd_avx2_gather_xmm_i32",
            "{vex} vpxor %xmm2, %xmm2, %xmm2\n{vex} vpcmpeqd %xmm3, %xmm3, %xmm3\n{vex} vpgatherdd %xmm3, 64(%rax,%xmm2,4), %xmm1",
            Int,
        ),
        (
            "vpgatherdd_avx2_gather_ymm_i32",
            "{vex} vpxor %ymm2, %ymm2, %ymm2\n{vex} vpcmpeqd %ymm3, %ymm3, %ymm3\n{vex} vpgatherdd %ymm3, 64(%rax,%ymm2,4), %ymm1",
            Int,
        ),
        (
            "vpgatherdq_avx2_gather_xmm_i32",
            "{vex} vpxor %xmm2, %xmm2, %xmm2\n{vex} vpcmpeqq %xmm3, %xmm3, %xmm3\n{vex} vpgatherdq %xmm3, 64(%rax,%xmm2,8), %xmm1",
            Int,
        ),
        (
            "vpgatherdq_avx2_gather_ymm_i32",
            "{vex} vpxor %xmm2, %xmm2, %xmm2\n{vex} vpcmpeqq %ymm3, %ymm3, %ymm3\n{vex} vpgatherdq %ymm3, 64(%rax,%xmm2,8), %ymm1",
            Int,
        ),
        (
            "vpgatherqd_avx2_gather_xmm_i64",
            "{vex} vpxor %xmm2, %xmm2, %xmm2\n{vex} vpcmpeqd %xmm3, %xmm3, %xmm3\n{vex} vpgatherqd %xmm3, 64(%rax,%xmm2,4), %xmm1",
            Int,
        ),
        (
            "vpgatherqd_avx2_gather_ymm_i64",
            "{vex} vpxor %ymm2, %ymm2, %ymm2\n{vex} vpcmpeqd %xmm3, %xmm3, %xmm3\n{vex} vpgatherqd %xmm3, 64(%rax,%ymm2,4), %xmm1",
            Int,
        ),
        (
            "vpgatherqq_avx2_gather_xmm_i64",
            "{vex} vpxor %xmm2, %xmm2, %xmm2\n{vex} vpcmpeqq %xmm3, %xmm3, %xmm3\n{vex} vpgatherqq %xmm3, 64(%rax,%xmm2,8), %xmm1",
            Int,
        ),
        (
            "vpgatherqq_avx2_gather_ymm_i64",
            "{vex} vpxor %ymm2, %ymm2, %ymm2\n{vex} vpcmpeqq %ymm3, %ymm3, %ymm3\n{vex} vpgatherqq %ymm3, 64(%rax,%ymm2,8), %ymm1",
            Int,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Avx2,
            profile,
        });
    }

    // AVX2 gather edge cases with partial vector masks and signed/non-zero VSIB
    // indices. These verify inactive-lane preservation, gathered-lane mask
    // clearing, and the narrower q-index forms that zero unused vector lanes.
    for &(label, asm, profile) in &[
        (
            "vgatherdps_avx2_gather_edge_ymm_partial_mask_i32",
            "movl $-1, 320(%rax)\nmovl $0, 324(%rax)\nmovl $1, 328(%rax)\nmovl $2, 332(%rax)\nmovl $-2, 336(%rax)\nmovl $3, 340(%rax)\nmovl $4, 344(%rax)\nmovl $0, 348(%rax)\nmovl $-2147483648, 384(%rax)\nmovl $0, 388(%rax)\nmovl $-2147483648, 392(%rax)\nmovl $0, 396(%rax)\nmovl $-2147483648, 400(%rax)\nmovl $0, 404(%rax)\nmovl $-2147483648, 408(%rax)\nmovl $0, 412(%rax)\n{vex} vmovdqu 320(%rax), %ymm2\n{vex} vmovdqu 384(%rax), %ymm3\n{vex} vgatherdps %ymm3, 128(%rax,%ymm2,4), %ymm1",
            F32,
        ),
        (
            "vgatherdpd_avx2_gather_edge_ymm_partial_mask_i32",
            "movl $-1, 320(%rax)\nmovl $0, 324(%rax)\nmovl $1, 328(%rax)\nmovl $2, 332(%rax)\nmovabsq $0x8000000000000000, %r8\nmovq %r8, 384(%rax)\nmovq $0, 392(%rax)\nmovq %r8, 400(%rax)\nmovq $0, 408(%rax)\n{vex} vmovdqu 320(%rax), %xmm2\n{vex} vmovdqu 384(%rax), %ymm3\n{vex} vgatherdpd %ymm3, 128(%rax,%xmm2,8), %ymm1",
            F64,
        ),
        (
            "vgatherqps_avx2_gather_edge_xmm_zero_upper_i64",
            "movq $-1, 320(%rax)\nmovq $2, 328(%rax)\nmovl $-2147483648, 384(%rax)\nmovl $0, 388(%rax)\nmovl $-2147483648, 392(%rax)\nmovl $-2147483648, 396(%rax)\n{vex} vmovdqu 320(%rax), %xmm2\n{vex} vmovdqu 384(%rax), %xmm3\n{vex} vgatherqps %xmm3, 128(%rax,%xmm2,4), %xmm1",
            F32,
        ),
        (
            "vgatherqpd_avx2_gather_edge_ymm_partial_mask_i64",
            "movq $-1, 320(%rax)\nmovq $0, 328(%rax)\nmovq $1, 336(%rax)\nmovq $2, 344(%rax)\nmovabsq $0x8000000000000000, %r8\nmovq %r8, 384(%rax)\nmovq $0, 392(%rax)\nmovq $0, 400(%rax)\nmovq %r8, 408(%rax)\n{vex} vmovdqu 320(%rax), %ymm2\n{vex} vmovdqu 384(%rax), %ymm3\n{vex} vgatherqpd %ymm3, 128(%rax,%ymm2,8), %ymm1",
            F64,
        ),
        (
            "vpgatherdd_avx2_gather_edge_ymm_partial_mask_i32",
            "movl $-1, 320(%rax)\nmovl $0, 324(%rax)\nmovl $1, 328(%rax)\nmovl $2, 332(%rax)\nmovl $-2, 336(%rax)\nmovl $3, 340(%rax)\nmovl $4, 344(%rax)\nmovl $0, 348(%rax)\nmovl $-2147483648, 384(%rax)\nmovl $0, 388(%rax)\nmovl $0, 392(%rax)\nmovl $-2147483648, 396(%rax)\nmovl $-2147483648, 400(%rax)\nmovl $0, 404(%rax)\nmovl $0, 408(%rax)\nmovl $-2147483648, 412(%rax)\n{vex} vmovdqu 320(%rax), %ymm2\n{vex} vmovdqu 384(%rax), %ymm3\n{vex} vpgatherdd %ymm3, 128(%rax,%ymm2,4), %ymm1",
            Int,
        ),
        (
            "vpgatherdq_avx2_gather_edge_ymm_partial_mask_i32",
            "movl $-1, 320(%rax)\nmovl $0, 324(%rax)\nmovl $1, 328(%rax)\nmovl $2, 332(%rax)\nmovabsq $0x8000000000000000, %r8\nmovq $0, 384(%rax)\nmovq %r8, 392(%rax)\nmovq %r8, 400(%rax)\nmovq $0, 408(%rax)\n{vex} vmovdqu 320(%rax), %xmm2\n{vex} vmovdqu 384(%rax), %ymm3\n{vex} vpgatherdq %ymm3, 128(%rax,%xmm2,8), %ymm1",
            Int,
        ),
        (
            "vpgatherqd_avx2_gather_edge_xmm_zero_upper_i64",
            "movq $-1, 320(%rax)\nmovq $2, 328(%rax)\nmovl $0, 384(%rax)\nmovl $-2147483648, 388(%rax)\nmovl $-2147483648, 392(%rax)\nmovl $0, 396(%rax)\n{vex} vmovdqu 320(%rax), %xmm2\n{vex} vmovdqu 384(%rax), %xmm3\n{vex} vpgatherqd %xmm3, 128(%rax,%xmm2,4), %xmm1",
            Int,
        ),
        (
            "vpgatherqq_avx2_gather_edge_ymm_partial_mask_i64",
            "movq $-1, 320(%rax)\nmovq $0, 328(%rax)\nmovq $1, 336(%rax)\nmovq $2, 344(%rax)\nmovabsq $0x8000000000000000, %r8\nmovq %r8, 384(%rax)\nmovq %r8, 392(%rax)\nmovq $0, 400(%rax)\nmovq %r8, 408(%rax)\n{vex} vmovdqu 320(%rax), %ymm2\n{vex} vmovdqu 384(%rax), %ymm3\n{vex} vpgatherqq %ymm3, 128(%rax,%ymm2,8), %ymm1",
            Int,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Avx2,
            profile,
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
    for &(mnem, imm) in &[
        ("vgf2p8mulb", None),
        ("vgf2p8affineqb", Some(0x9a)),
        ("vgf2p8affineinvqb", Some(0x5a)),
    ] {
        for class in ["xmm", "ymm"] {
            let operands = match imm {
                Some(imm) => format!("${imm:#x}, -32(%rbx,%r9,1), %{class}3, %{class}1"),
                None => format!("-32(%rbx,%r9,1), %{class}3, %{class}1"),
            };
            out.push(Case {
                label: format!("{mnem}_vex_gfni_addr_{class}_indexed"),
                asm: format!("{{vex}} {mnem} {operands}"),
                feat: Gfni,
                profile: Int,
            });

            let operands = match imm {
                Some(imm) => format!("${imm:#x}, -32(%rbx,%r9,1), %{class}3, %{class}1"),
                None => format!("-32(%rbx,%r9,1), %{class}3, %{class}1"),
            };
            out.push(Case {
                label: format!("{mnem}_vex_gfni_addr_{class}_addr32"),
                asm: format!("addr32 {{vex}} {mnem} {operands}"),
                feat: Gfni,
                profile: Int,
            });

            let operands = match imm {
                Some(imm) => format!("${imm:#x}, -32(%rbx,%r9,1), %{class}11, %{class}9"),
                None => format!("-32(%rbx,%r9,1), %{class}11, %{class}9"),
            };
            out.push(Case {
                label: format!("{mnem}_vex_gfni_addr_{class}_high_indexed"),
                asm: format!("{{vex}} {mnem} {operands}"),
                feat: Gfni,
                profile: Int,
            });
        }
    }

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
    for mnem in ["vaesenc", "vaesenclast", "vaesdec", "vaesdeclast"] {
        for class in ["xmm", "ymm"] {
            out.push(Case {
                label: format!("{mnem}_vex_vaes_addr_{class}_indexed"),
                asm: format!("{{vex}} {mnem} -32(%rbx,%r9,1), %{class}3, %{class}1"),
                feat: Vaes,
                profile: Int,
            });
            out.push(Case {
                label: format!("{mnem}_vex_vaes_addr_{class}_addr32"),
                asm: format!("addr32 {{vex}} {mnem} -32(%rbx,%r9,1), %{class}3, %{class}1"),
                feat: Vaes,
                profile: Int,
            });
            out.push(Case {
                label: format!("{mnem}_vex_vaes_addr_{class}_high_indexed"),
                asm: format!("{{vex}} {mnem} -32(%rbx,%r9,1), %{class}11, %{class}9"),
                feat: Vaes,
                profile: Int,
            });
        }
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
    for &(imm, tag) in &[(0x00, "ll"), (0x01, "hl"), (0x10, "lh"), (0x11, "hh")] {
        for class in ["xmm", "ymm"] {
            out.push(Case {
                label: format!("vpclmulqdq_vex_crypto_addr_{class}_{tag}_indexed"),
                asm: format!(
                    "{{vex}} vpclmulqdq ${imm:#x}, -32(%rbx,%r9,1), %{class}3, %{class}1"
                ),
                feat: Vpclmulqdq,
                profile: Int,
            });
            out.push(Case {
                label: format!("vpclmulqdq_vex_crypto_addr_{class}_{tag}_addr32"),
                asm: format!(
                    "addr32 {{vex}} vpclmulqdq ${imm:#x}, -32(%rbx,%r9,1), %{class}3, %{class}1"
                ),
                feat: Vpclmulqdq,
                profile: Int,
            });
            out.push(Case {
                label: format!("vpclmulqdq_vex_crypto_addr_{class}_{tag}_high_indexed"),
                asm: format!(
                    "{{vex}} vpclmulqdq ${imm:#x}, -32(%rbx,%r9,1), %{class}11, %{class}9"
                ),
                feat: Vpclmulqdq,
                profile: Int,
            });
        }
    }

    // Crypto edge operands exercise exact zero/all-one vectors on EVEX zmm
    // paths, complementing the address-form and width coverage above.
    for &(label, asm, feat) in &[
        (
            "vgf2p8mulb_gfni_crypto_edge_zero_reg",
            "vpxord %zmm2, %zmm2, %zmm2\nvgf2p8mulb %zmm2, %zmm3, %zmm1",
            Gfni,
        ),
        (
            "vgf2p8mulb_gfni_crypto_edge_allones_mem",
            "vpternlogd $0xff, %zmm2, %zmm2, %zmm2\nvmovdqu64 %zmm2, 64(%rax)\nvgf2p8mulb 64(%rax), %zmm3, %zmm1",
            Gfni,
        ),
        (
            "vgf2p8affineqb_gfni_crypto_edge_imm0_zero_reg",
            "vpxord %zmm2, %zmm2, %zmm2\nvgf2p8affineqb $0x00, %zmm2, %zmm3, %zmm1",
            Gfni,
        ),
        (
            "vgf2p8affineqb_gfni_crypto_edge_immff_allones_mem",
            "vpternlogd $0xff, %zmm2, %zmm2, %zmm2\nvmovdqu64 %zmm2, 64(%rax)\nvgf2p8affineqb $0xff, 64(%rax), %zmm3, %zmm1",
            Gfni,
        ),
        (
            "vgf2p8affineinvqb_gfni_crypto_edge_imm0_zero_reg",
            "vpxord %zmm2, %zmm2, %zmm2\nvgf2p8affineinvqb $0x00, %zmm2, %zmm3, %zmm1",
            Gfni,
        ),
        (
            "vgf2p8affineinvqb_gfni_crypto_edge_immff_allones_mem",
            "vpternlogd $0xff, %zmm2, %zmm2, %zmm2\nvmovdqu64 %zmm2, 64(%rax)\nvgf2p8affineinvqb $0xff, 64(%rax), %zmm3, %zmm1",
            Gfni,
        ),
        (
            "vaesenc_vaes_crypto_edge_zero_reg",
            "vpxord %zmm2, %zmm2, %zmm2\nvaesenc %zmm2, %zmm3, %zmm1",
            Vaes,
        ),
        (
            "vaesenc_vaes_crypto_edge_allones_mem",
            "vpternlogd $0xff, %zmm2, %zmm2, %zmm2\nvmovdqu64 %zmm2, 64(%rax)\nvaesenc 64(%rax), %zmm3, %zmm1",
            Vaes,
        ),
        (
            "vaesenclast_vaes_crypto_edge_zero_reg",
            "vpxord %zmm2, %zmm2, %zmm2\nvaesenclast %zmm2, %zmm3, %zmm1",
            Vaes,
        ),
        (
            "vaesenclast_vaes_crypto_edge_allones_mem",
            "vpternlogd $0xff, %zmm2, %zmm2, %zmm2\nvmovdqu64 %zmm2, 64(%rax)\nvaesenclast 64(%rax), %zmm3, %zmm1",
            Vaes,
        ),
        (
            "vaesdec_vaes_crypto_edge_zero_reg",
            "vpxord %zmm2, %zmm2, %zmm2\nvaesdec %zmm2, %zmm3, %zmm1",
            Vaes,
        ),
        (
            "vaesdec_vaes_crypto_edge_allones_mem",
            "vpternlogd $0xff, %zmm2, %zmm2, %zmm2\nvmovdqu64 %zmm2, 64(%rax)\nvaesdec 64(%rax), %zmm3, %zmm1",
            Vaes,
        ),
        (
            "vaesdeclast_vaes_crypto_edge_zero_reg",
            "vpxord %zmm2, %zmm2, %zmm2\nvaesdeclast %zmm2, %zmm3, %zmm1",
            Vaes,
        ),
        (
            "vaesdeclast_vaes_crypto_edge_allones_mem",
            "vpternlogd $0xff, %zmm2, %zmm2, %zmm2\nvmovdqu64 %zmm2, 64(%rax)\nvaesdeclast 64(%rax), %zmm3, %zmm1",
            Vaes,
        ),
        (
            "vpclmulqdq_vpclmulqdq_crypto_edge_ll_zero_reg",
            "vpxord %zmm2, %zmm2, %zmm2\nvpclmulqdq $0x00, %zmm2, %zmm3, %zmm1",
            Vpclmulqdq,
        ),
        (
            "vpclmulqdq_vpclmulqdq_crypto_edge_hl_allones_mem",
            "vpternlogd $0xff, %zmm2, %zmm2, %zmm2\nvmovdqu64 %zmm2, 64(%rax)\nvpclmulqdq $0x01, 64(%rax), %zmm3, %zmm1",
            Vpclmulqdq,
        ),
        (
            "vpclmulqdq_vpclmulqdq_crypto_edge_lh_zero_reg",
            "vpxord %zmm2, %zmm2, %zmm2\nvpclmulqdq $0x10, %zmm2, %zmm3, %zmm1",
            Vpclmulqdq,
        ),
        (
            "vpclmulqdq_vpclmulqdq_crypto_edge_hh_allones_mem",
            "vpternlogd $0xff, %zmm2, %zmm2, %zmm2\nvmovdqu64 %zmm2, 64(%rax)\nvpclmulqdq $0x11, 64(%rax), %zmm3, %zmm1",
            Vpclmulqdq,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }

    // VEX-encoded AVX-512 opmask moves. These exercise all KMOV transfer
    // classes: k<-mem, k<-k, mem<-k, k<-GPR, and GPR<-k across b/w/d/q widths.
    for &(label, asm, feat) in &[
        ("kmovb_kmov_mem_load", "kmovb 16(%rax), %k1", Dq),
        ("kmovw_kmov_mem_load", "kmovw 18(%rax), %k1", F),
        ("kmovd_kmov_mem_load", "kmovd 20(%rax), %k1", Dq),
        ("kmovq_kmov_mem_load", "kmovq 24(%rax), %k1", Bw),
        ("kmovb_kmov_kreg_copy", "kmovb %k2, %k1", Dq),
        ("kmovw_kmov_kreg_copy", "kmovw %k2, %k1", F),
        ("kmovd_kmov_kreg_copy", "kmovd %k2, %k1", Dq),
        ("kmovq_kmov_kreg_copy", "kmovq %k2, %k1", Bw),
        ("kmovb_kmov_mem_store", "kmovb %k2, 32(%rax)", Dq),
        ("kmovw_kmov_mem_store", "kmovw %k2, 34(%rax)", F),
        ("kmovd_kmov_mem_store", "kmovd %k2, 36(%rax)", Dq),
        ("kmovq_kmov_mem_store", "kmovq %k2, 40(%rax)", Bw),
        ("kmovb_kmov_gpr_to_k", "kmovb %r8d, %k1", Dq),
        ("kmovw_kmov_gpr_to_k", "kmovw %r8d, %k1", F),
        ("kmovd_kmov_gpr_to_k", "kmovd %r8d, %k1", Dq),
        ("kmovq_kmov_gpr_to_k", "kmovq %r8, %k1", Bw),
        ("kmovb_kmov_k_to_gpr", "kmovb %k2, %r8d", Dq),
        ("kmovw_kmov_k_to_gpr", "kmovw %k2, %r8d", F),
        ("kmovd_kmov_k_to_gpr", "kmovd %k2, %r8d", Dq),
        ("kmovq_kmov_k_to_gpr", "kmovq %k2, %r8", Bw),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }

    // AVX-512 opmask ALU/test edge forms. These force zero/all-one inputs,
    // width truncation for b/w/d/q masks, carry-discarding KADD behavior,
    // KTEST/KORTEST status flags, and KUNPCK packing boundaries.
    for &(label, asm, feat) in &[
        ("kandw_opmask_edge_zero_allones", "kxorw %k1, %k1, %k1\nkxnorw %k2, %k2, %k2\nkandw %k1, %k2, %k5", F),
        ("kandnw_opmask_edge_allones_zero", "kxnorw %k1, %k1, %k1\nkxorw %k2, %k2, %k2\nkandnw %k1, %k2, %k5", F),
        ("korw_opmask_edge_zero_allones", "kxorw %k1, %k1, %k1\nkxnorw %k2, %k2, %k2\nkorw %k1, %k2, %k5", F),
        ("kxorw_opmask_edge_self_zero", "kxnorw %k1, %k1, %k1\nkxorw %k1, %k1, %k5", F),
        ("kxnorw_opmask_edge_self_allones", "kxnorw %k1, %k1, %k1\nkxnorw %k1, %k1, %k5", F),
        ("knotw_opmask_edge_width", "kxnorw %k1, %k1, %k1\nknotw %k1, %k5", F),
        ("ktestw_opmask_edge_zero_zero_flags", "kxorw %k1, %k1, %k1\nktestw %k1, %k1", F),
        ("ktestw_opmask_edge_allones_flags", "kxnorw %k1, %k1, %k1\nktestw %k1, %k1", F),
        ("kortestw_opmask_edge_zero_zero_flags", "kxorw %k1, %k1, %k1\nkortestw %k1, %k1", F),
        ("kortestw_opmask_edge_allones_flags", "kxnorw %k1, %k1, %k1\nkortestw %k1, %k1", F),
        ("kunpckbw_opmask_edge_zero_allones", "kxorb %k1, %k1, %k1\nkxnorb %k2, %k2, %k2\nkunpckbw %k1, %k2, %k5", F),
        ("kunpckwd_opmask_edge_zero_allones", "kxorw %k1, %k1, %k1\nkxnorw %k2, %k2, %k2\nkunpckwd %k1, %k2, %k5", F),
        ("kandb_opmask_edge_width", "kxnorb %k1, %k1, %k1\nkandb %k2, %k1, %k5", Dq),
        ("kandd_opmask_edge_width", "kxnord %k1, %k1, %k1\nkandd %k2, %k1, %k5", Dq),
        ("kandnb_opmask_edge_width", "kxnorb %k1, %k1, %k1\nkandnb %k2, %k1, %k5", Dq),
        ("kandnd_opmask_edge_width", "kxnord %k1, %k1, %k1\nkandnd %k2, %k1, %k5", Dq),
        ("korb_opmask_edge_zero_allones", "kxorb %k1, %k1, %k1\nkxnorb %k2, %k2, %k2\nkorb %k1, %k2, %k5", Dq),
        ("kord_opmask_edge_zero_allones", "kxord %k1, %k1, %k1\nkxnord %k2, %k2, %k2\nkord %k1, %k2, %k5", Dq),
        ("kxorb_opmask_edge_self_zero", "kxnorb %k1, %k1, %k1\nkxorb %k1, %k1, %k5", Dq),
        ("kxord_opmask_edge_self_zero", "kxnord %k1, %k1, %k1\nkxord %k1, %k1, %k5", Dq),
        ("kxnorb_opmask_edge_self_allones", "kxnorb %k1, %k1, %k1\nkxnorb %k1, %k1, %k5", Dq),
        ("kxnord_opmask_edge_self_allones", "kxnord %k1, %k1, %k1\nkxnord %k1, %k1, %k5", Dq),
        ("kaddb_opmask_edge_discard_carry", "kxnorb %k1, %k1, %k1\nkaddb %k1, %k1, %k5", Dq),
        ("kaddd_opmask_edge_discard_carry", "kxnord %k1, %k1, %k1\nkaddd %k1, %k1, %k5", Dq),
        ("knotb_opmask_edge_width", "kxnorb %k1, %k1, %k1\nknotb %k1, %k5", Dq),
        ("knotd_opmask_edge_width", "kxnord %k1, %k1, %k1\nknotd %k1, %k5", Dq),
        ("ktestb_opmask_edge_zero_allones_flags", "kxorb %k1, %k1, %k1\nkxnorb %k2, %k2, %k2\nktestb %k1, %k2", Dq),
        ("kortestd_opmask_edge_zero_allones_flags", "kxord %k1, %k1, %k1\nkxnord %k2, %k2, %k2\nkortestd %k1, %k2", Dq),
        ("kandq_opmask_edge_width", "kxnorq %k1, %k1, %k1\nkandq %k2, %k1, %k5", Bw),
        ("kandnq_opmask_edge_width", "kxnorq %k1, %k1, %k1\nkandnq %k2, %k1, %k5", Bw),
        ("korq_opmask_edge_zero_allones", "kxorq %k1, %k1, %k1\nkxnorq %k2, %k2, %k2\nkorq %k1, %k2, %k5", Bw),
        ("kxorq_opmask_edge_self_zero", "kxnorq %k1, %k1, %k1\nkxorq %k1, %k1, %k5", Bw),
        ("kxnorq_opmask_edge_self_allones", "kxnorq %k1, %k1, %k1\nkxnorq %k1, %k1, %k5", Bw),
        ("kaddq_opmask_edge_discard_carry", "kxnorq %k1, %k1, %k1\nkaddq %k1, %k1, %k5", Bw),
        ("knotq_opmask_edge_width", "kxnorq %k1, %k1, %k1\nknotq %k1, %k5", Bw),
        ("ktestq_opmask_edge_allones_flags", "kxnorq %k1, %k1, %k1\nktestq %k1, %k1", Bw),
        ("kortestq_opmask_edge_zero_zero_flags", "kxorq %k1, %k1, %k1\nkortestq %k1, %k1", Bw),
        ("kunpckdq_opmask_edge_zero_allones", "kxord %k1, %k1, %k1\nkxnord %k2, %k2, %k2\nkunpckdq %k1, %k2, %k5", Bw),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }

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
    for class in ["xmm", "ymm"] {
        for (tag, asm) in [
            (
                "indexed",
                format!("{{vex}} vcvtph2ps -32(%rbx,%r9,1), %{class}1"),
            ),
            (
                "addr32_indexed",
                format!("addr32 {{vex}} vcvtph2ps -32(%rbx,%r9,1), %{class}1"),
            ),
            (
                "high_disp",
                format!("{{vex}} vcvtph2ps -16(%rbx), %{class}9"),
            ),
        ] {
            out.push(Case {
                label: format!("vcvtph2ps_f16c_{class}_addr_{tag}"),
                asm,
                feat: F16c,
                profile: F16,
            });
        }
    }
    for &(class, imm_indexed, imm_addr32, imm_high) in
        &[("xmm", 0x3, 0x7, 0x1), ("ymm", 0x5, 0x7, 0x1)]
    {
        for (tag, asm) in [
            (
                "disp",
                format!("{{vex}} vcvtps2ph ${imm_indexed:#x}, %{class}2, -16(%rbx)"),
            ),
            (
                "addr32_disp",
                format!("addr32 {{vex}} vcvtps2ph ${imm_addr32:#x}, %{class}2, -16(%rbx)"),
            ),
            (
                "high_disp",
                format!("{{vex}} vcvtps2ph ${imm_high:#x}, %{class}9, -16(%rbx)"),
            ),
        ] {
            out.push(Case {
                label: format!("vcvtps2ph_f16c_{class}_addr_{tag}"),
                asm,
                feat: F16c,
                profile: F32,
            });
        }
    }

    for &(label, asm, feat, profile) in &[
        (
            "cvtpi2ps_sse_simd_convert_edge_mem_to_xmm",
            "cvtpi2ps 32(%rax), %xmm1",
            Sse,
            IntConvertEdge,
        ),
        (
            "cvtps2pi_sse_simd_convert_edge_xmm_to_mmx_store",
            "cvtps2pi %xmm3, %mm0\nmovq %mm0, 64(%rax)\nemms",
            Sse,
            F32ConvertEdge,
        ),
        (
            "cvttps2pi_sse_simd_convert_edge_mem_to_mmx_store",
            "cvttps2pi 32(%rax), %mm0\nmovq %mm0, 72(%rax)\nemms",
            Sse,
            F32ConvertEdge,
        ),
        (
            "cvtss2si_sse_simd_convert_edge_reg_r8",
            "cvtss2si %xmm3, %r8",
            Sse,
            F32ConvertEdge,
        ),
        (
            "cvttss2si_sse_simd_convert_edge_mem_r8",
            "cvttss2si 32(%rax), %r8",
            Sse,
            F32ConvertEdge,
        ),
        (
            "cvtsi2ss_sse_simd_convert_edge_m32",
            "cvtsi2ss 32(%rax), %xmm1",
            Sse,
            IntConvertEdge,
        ),
        (
            "cvtpi2pd_sse2_simd_convert_edge_mem_to_xmm",
            "cvtpi2pd 32(%rax), %xmm1",
            Sse2,
            IntConvertEdge,
        ),
        (
            "cvtpd2pi_sse2_simd_convert_edge_xmm_to_mmx_store",
            "cvtpd2pi %xmm3, %mm0\nmovq %mm0, 80(%rax)\nemms",
            Sse2,
            F64ConvertEdge,
        ),
        (
            "cvttpd2pi_sse2_simd_convert_edge_mem_to_mmx_store",
            "cvttpd2pi 32(%rax), %mm0\nmovq %mm0, 88(%rax)\nemms",
            Sse2,
            F64ConvertEdge,
        ),
        (
            "cvtdq2ps_sse2_simd_convert_edge_reg",
            "cvtdq2ps %xmm3, %xmm1",
            Sse2,
            IntConvertEdge,
        ),
        (
            "cvtdq2ps_sse2_simd_convert_edge_mem",
            "cvtdq2ps 32(%rax), %xmm1",
            Sse2,
            IntConvertEdge,
        ),
        (
            "cvtps2dq_sse2_simd_convert_edge_reg",
            "cvtps2dq %xmm3, %xmm1",
            Sse2,
            F32ConvertEdge,
        ),
        (
            "cvtps2dq_sse2_simd_convert_edge_mem",
            "cvtps2dq 32(%rax), %xmm1",
            Sse2,
            F32ConvertEdge,
        ),
        (
            "cvttps2dq_sse2_simd_convert_edge_reg",
            "cvttps2dq %xmm3, %xmm1",
            Sse2,
            F32ConvertEdge,
        ),
        (
            "cvttps2dq_sse2_simd_convert_edge_mem",
            "cvttps2dq 32(%rax), %xmm1",
            Sse2,
            F32ConvertEdge,
        ),
        (
            "cvtsd2si_sse2_simd_convert_edge_reg_r8",
            "cvtsd2si %xmm3, %r8",
            Sse2,
            F64ConvertEdge,
        ),
        (
            "cvttsd2si_sse2_simd_convert_edge_mem_r8",
            "cvttsd2si 32(%rax), %r8",
            Sse2,
            F64ConvertEdge,
        ),
        (
            "cvtsi2sd_sse2_simd_convert_edge_m32",
            "cvtsi2sd 32(%rax), %xmm1",
            Sse2,
            IntConvertEdge,
        ),
        (
            "vcvtps2dq_avx_simd_convert_edge_ymm_reg",
            "{vex} vcvtps2dq %ymm3, %ymm1",
            Avx,
            F32ConvertEdge,
        ),
        (
            "vcvttps2dq_avx_simd_convert_edge_ymm_mem",
            "{vex} vcvttps2dq 32(%rax), %ymm1",
            Avx,
            F32ConvertEdge,
        ),
        (
            "vcvtdq2ps_avx_simd_convert_edge_ymm_reg",
            "{vex} vcvtdq2ps %ymm3, %ymm1",
            Avx,
            IntConvertEdge,
        ),
        (
            "vcvtpd2dq_avx_simd_convert_edge_ymm_reg",
            "{vex} vcvtpd2dq %ymm3, %xmm1",
            Avx,
            F64ConvertEdge,
        ),
        (
            "vcvttpd2dq_avx_simd_convert_edge_ymm_reg",
            "{vex} vcvttpd2dq %ymm3, %xmm1",
            Avx,
            F64ConvertEdge,
        ),
        (
            "vcvtdq2pd_avx_simd_convert_edge_xmm_reg",
            "{vex} vcvtdq2pd %xmm3, %ymm1",
            Avx,
            IntConvertEdge,
        ),
        (
            "vcvtps2pd_avx_simd_convert_edge_xmm_reg",
            "{vex} vcvtps2pd %xmm3, %ymm1",
            Avx,
            F32ConvertEdge,
        ),
        (
            "vcvtpd2ps_avx_simd_convert_edge_ymm_reg",
            "{vex} vcvtpd2ps %ymm3, %xmm1",
            Avx,
            F64ConvertEdge,
        ),
        (
            "vcvtss2si_avx_simd_convert_edge_reg_r8",
            "{vex} vcvtss2si %xmm3, %r8",
            Avx,
            F32ConvertEdge,
        ),
        (
            "vcvttss2si_avx_simd_convert_edge_mem_r8",
            "{vex} vcvttss2si 32(%rax), %r8",
            Avx,
            F32ConvertEdge,
        ),
        (
            "vcvtsd2si_avx_simd_convert_edge_reg_r8",
            "{vex} vcvtsd2si %xmm3, %r8",
            Avx,
            F64ConvertEdge,
        ),
        (
            "vcvttsd2si_avx_simd_convert_edge_mem_r8",
            "{vex} vcvttsd2si 32(%rax), %r8",
            Avx,
            F64ConvertEdge,
        ),
        (
            "vcvtsi2ss_avx_simd_convert_edge_m32",
            "{vex} vcvtsi2ss 32(%rax), %xmm3, %xmm1",
            Avx,
            IntConvertEdge,
        ),
        (
            "vcvtsi2sd_avx_simd_convert_edge_m32",
            "{vex} vcvtsi2sd 32(%rax), %xmm3, %xmm1",
            Avx,
            IntConvertEdge,
        ),
        (
            "vcvtph2ps_f16c_simd_convert_edge_xmm_reg",
            "{vex} vcvtph2ps %xmm3, %xmm1",
            F16c,
            F16Edge,
        ),
        (
            "vcvtph2ps_f16c_simd_convert_edge_ymm_mem",
            "{vex} vcvtph2ps 32(%rax), %ymm1",
            F16c,
            F16Edge,
        ),
        (
            "vcvtps2ph_f16c_simd_convert_edge_xmm_rn",
            "{vex} vcvtps2ph $0, %xmm3, %xmm1",
            F16c,
            F32ConvertEdge,
        ),
        (
            "vcvtps2ph_f16c_simd_convert_edge_xmm_rd",
            "{vex} vcvtps2ph $1, %xmm3, %xmm1",
            F16c,
            F32ConvertEdge,
        ),
        (
            "vcvtps2ph_f16c_simd_convert_edge_ymm_ru_mem",
            "{vex} vcvtps2ph $2, %ymm3, 48(%rax)",
            F16c,
            F32ConvertEdge,
        ),
        (
            "vcvtps2ph_f16c_simd_convert_edge_ymm_rz",
            "{vex} vcvtps2ph $3, %ymm3, %xmm1",
            F16c,
            F32ConvertEdge,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile,
        });
    }

    for &(label, asm, feat, profile) in &[
        (
            "roundps_sse41_simd_round_edge_rn_reg",
            "roundps $0, %xmm3, %xmm1",
            Sse41,
            F32ConvertEdge,
        ),
        (
            "roundps_sse41_simd_round_edge_rd_mem",
            "roundps $1, 32(%rax), %xmm1",
            Sse41,
            F32ConvertEdge,
        ),
        (
            "roundps_sse41_simd_round_edge_mxcsr_rd_bits",
            "roundps $5, %xmm3, %xmm1",
            Sse41,
            F32ConvertEdge,
        ),
        (
            "roundpd_sse41_simd_round_edge_ru_reg",
            "roundpd $2, %xmm3, %xmm1",
            Sse41,
            F64ConvertEdge,
        ),
        (
            "roundpd_sse41_simd_round_edge_rz_mem",
            "roundpd $3, 32(%rax), %xmm1",
            Sse41,
            F64ConvertEdge,
        ),
        (
            "roundpd_sse41_simd_round_edge_mxcsr_ru_bits",
            "roundpd $6, %xmm3, %xmm1",
            Sse41,
            F64ConvertEdge,
        ),
        (
            "roundss_sse41_simd_round_edge_rz_reg",
            "roundss $3, %xmm3, %xmm1",
            Sse41,
            F32ConvertEdge,
        ),
        (
            "roundss_sse41_simd_round_edge_mxcsr_rd_bits_reg",
            "roundss $5, %xmm3, %xmm1",
            Sse41,
            F32ConvertEdge,
        ),
        (
            "roundss_sse41_simd_round_edge_mxcsr_ru_bits_mem",
            "roundss $6, 32(%rax), %xmm1",
            Sse41,
            F32ConvertEdge,
        ),
        (
            "roundsd_sse41_simd_round_edge_rd_reg",
            "roundsd $1, %xmm3, %xmm1",
            Sse41,
            F64ConvertEdge,
        ),
        (
            "roundsd_sse41_simd_round_edge_mxcsr_rz_bits_reg",
            "roundsd $7, %xmm3, %xmm1",
            Sse41,
            F64ConvertEdge,
        ),
        (
            "roundsd_sse41_simd_round_edge_mxcsr_rd_bits_mem",
            "roundsd $5, 32(%rax), %xmm1",
            Sse41,
            F64ConvertEdge,
        ),
        (
            "vroundps_avx_simd_round_edge_rn_reg",
            "{vex} vroundps $0, %ymm3, %ymm1",
            Avx,
            F32ConvertEdge,
        ),
        (
            "vroundps_avx_simd_round_edge_mxcsr_rd_bits_reg",
            "{vex} vroundps $5, %ymm3, %ymm1",
            Avx,
            F32ConvertEdge,
        ),
        (
            "vroundps_avx_simd_round_edge_ru_mem",
            "{vex} vroundps $2, 32(%rax), %ymm1",
            Avx,
            F32ConvertEdge,
        ),
        (
            "vroundpd_avx_simd_round_edge_rz_reg",
            "{vex} vroundpd $3, %ymm3, %ymm1",
            Avx,
            F64ConvertEdge,
        ),
        (
            "vroundpd_avx_simd_round_edge_mxcsr_ru_bits_mem",
            "{vex} vroundpd $6, 32(%rax), %ymm1",
            Avx,
            F64ConvertEdge,
        ),
        (
            "vroundpd_avx_simd_round_edge_rd_reg",
            "{vex} vroundpd $1, %ymm3, %ymm1",
            Avx,
            F64ConvertEdge,
        ),
        (
            "vroundss_avx_simd_round_edge_rz_reg",
            "{vex} vroundss $3, %xmm2, %xmm3, %xmm1",
            Avx,
            F32ConvertEdge,
        ),
        (
            "vroundss_avx_simd_round_edge_mxcsr_rd_bits_reg",
            "{vex} vroundss $5, %xmm2, %xmm3, %xmm1",
            Avx,
            F32ConvertEdge,
        ),
        (
            "vroundss_avx_simd_round_edge_mxcsr_ru_bits_mem",
            "{vex} vroundss $6, 32(%rax), %xmm3, %xmm1",
            Avx,
            F32ConvertEdge,
        ),
        (
            "vroundsd_avx_simd_round_edge_rd_reg",
            "{vex} vroundsd $1, %xmm2, %xmm3, %xmm1",
            Avx,
            F64ConvertEdge,
        ),
        (
            "vroundsd_avx_simd_round_edge_mxcsr_rz_bits_reg",
            "{vex} vroundsd $7, %xmm2, %xmm3, %xmm1",
            Avx,
            F64ConvertEdge,
        ),
        (
            "vroundsd_avx_simd_round_edge_mxcsr_rd_bits_mem",
            "{vex} vroundsd $5, 32(%rax), %xmm3, %xmm1",
            Avx,
            F64ConvertEdge,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile,
        });
    }

    for &(label, asm, feat, profile) in &[
        (
            "dpps_sse41_simd_dot_edge_zero_input",
            "dpps $0x0f, %xmm2, %xmm1",
            Sse41,
            F32ConvertEdge,
        ),
        (
            "dpps_sse41_simd_dot_edge_no_output",
            "dpps $0xf0, %xmm2, %xmm1",
            Sse41,
            F32ConvertEdge,
        ),
        (
            "dpps_sse41_simd_dot_edge_all_reg",
            "dpps $0xff, %xmm2, %xmm1",
            Sse41,
            F32ConvertEdge,
        ),
        (
            "dpps_sse41_simd_dot_edge_alt_reg",
            "dpps $0x5a, %xmm2, %xmm1",
            Sse41,
            F32ConvertEdge,
        ),
        (
            "dpps_sse41_simd_dot_edge_alt_mem",
            "dpps $0xa5, 32(%rax), %xmm1",
            Sse41,
            F32ConvertEdge,
        ),
        (
            "dppd_sse41_simd_dot_edge_zero_input",
            "dppd $0x03, %xmm2, %xmm1",
            Sse41,
            F64ConvertEdge,
        ),
        (
            "dppd_sse41_simd_dot_edge_no_output",
            "dppd $0x30, %xmm2, %xmm1",
            Sse41,
            F64ConvertEdge,
        ),
        (
            "dppd_sse41_simd_dot_edge_all_reg",
            "dppd $0x33, %xmm2, %xmm1",
            Sse41,
            F64ConvertEdge,
        ),
        (
            "dppd_sse41_simd_dot_edge_high_to_high_mem",
            "dppd $0x22, 32(%rax), %xmm1",
            Sse41,
            F64ConvertEdge,
        ),
        (
            "dppd_sse41_simd_dot_edge_low_to_low_mem",
            "dppd $0x11, 32(%rax), %xmm1",
            Sse41,
            F64ConvertEdge,
        ),
        (
            "vdpps_avx_simd_dot_edge_xmm_zero_input",
            "{vex} vdpps $0x0f, %xmm2, %xmm3, %xmm1",
            Avx,
            F32ConvertEdge,
        ),
        (
            "vdpps_avx_simd_dot_edge_xmm_no_output",
            "{vex} vdpps $0xf0, %xmm2, %xmm3, %xmm1",
            Avx,
            F32ConvertEdge,
        ),
        (
            "vdpps_avx_simd_dot_edge_xmm_all_reg",
            "{vex} vdpps $0xff, %xmm2, %xmm3, %xmm1",
            Avx,
            F32ConvertEdge,
        ),
        (
            "vdpps_avx_simd_dot_edge_ymm_all_mem",
            "{vex} vdpps $0xff, 32(%rax), %ymm3, %ymm1",
            Avx,
            F32ConvertEdge,
        ),
        (
            "vdpps_avx_simd_dot_edge_ymm_alt_reg",
            "{vex} vdpps $0x5a, %ymm2, %ymm3, %ymm1",
            Avx,
            F32ConvertEdge,
        ),
        (
            "vdpps_avx_simd_dot_edge_ymm_alt_mem",
            "{vex} vdpps $0xa5, 32(%rax), %ymm3, %ymm1",
            Avx,
            F32ConvertEdge,
        ),
        (
            "vdppd_avx_simd_dot_edge_zero_input",
            "{vex} vdppd $0x03, %xmm2, %xmm3, %xmm1",
            Avx,
            F64ConvertEdge,
        ),
        (
            "vdppd_avx_simd_dot_edge_no_output",
            "{vex} vdppd $0x30, %xmm2, %xmm3, %xmm1",
            Avx,
            F64ConvertEdge,
        ),
        (
            "vdppd_avx_simd_dot_edge_all_reg",
            "{vex} vdppd $0x33, %xmm2, %xmm3, %xmm1",
            Avx,
            F64ConvertEdge,
        ),
        (
            "vdppd_avx_simd_dot_edge_high_to_high_mem",
            "{vex} vdppd $0x22, 32(%rax), %xmm3, %xmm1",
            Avx,
            F64ConvertEdge,
        ),
        (
            "vdppd_avx_simd_dot_edge_low_to_low_mem",
            "{vex} vdppd $0x11, 32(%rax), %xmm3, %xmm1",
            Avx,
            F64ConvertEdge,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile,
        });
    }

    for &(label, asm, feat) in &[
        (
            "mpsadbw_sse41_simd_mpsad_edge_src0_dst0_reg",
            "mpsadbw $0x00, %xmm2, %xmm1",
            Sse41,
        ),
        (
            "mpsadbw_sse41_simd_mpsad_edge_src3_dst0_mem",
            "mpsadbw $0x03, 32(%rax), %xmm1",
            Sse41,
        ),
        (
            "mpsadbw_sse41_simd_mpsad_edge_src0_dst4_reg",
            "mpsadbw $0x04, %xmm2, %xmm1",
            Sse41,
        ),
        (
            "mpsadbw_sse41_simd_mpsad_edge_src3_dst4_mem",
            "mpsadbw $0x07, 32(%rax), %xmm1",
            Sse41,
        ),
        (
            "mpsadbw_sse41_simd_mpsad_edge_ignored_high_bits_reg",
            "mpsadbw $0xff, %xmm2, %xmm1",
            Sse41,
        ),
        (
            "mpsadbw_sse41_simd_mpsad_edge_ignored_high_bits_mem",
            "mpsadbw $0xf8, 32(%rax), %xmm1",
            Sse41,
        ),
        (
            "vmpsadbw_avx2_simd_mpsad_edge_xmm_src0_dst0_reg",
            "{vex} vmpsadbw $0x00, %xmm2, %xmm3, %xmm1",
            Avx2,
        ),
        (
            "vmpsadbw_avx2_simd_mpsad_edge_xmm_src3_dst4_mem",
            "{vex} vmpsadbw $0x07, 32(%rax), %xmm3, %xmm1",
            Avx2,
        ),
        (
            "vmpsadbw_avx2_simd_mpsad_edge_ymm_src0_dst4_reg",
            "{vex} vmpsadbw $0x04, %ymm2, %ymm3, %ymm1",
            Avx2,
        ),
        (
            "vmpsadbw_avx2_simd_mpsad_edge_ymm_src3_dst0_mem",
            "{vex} vmpsadbw $0x03, 32(%rax), %ymm3, %ymm1",
            Avx2,
        ),
        (
            "vmpsadbw_avx2_simd_mpsad_edge_ignored_high_bits_reg",
            "{vex} vmpsadbw $0xff, %ymm2, %ymm3, %ymm1",
            Avx2,
        ),
        (
            "vmpsadbw_avx2_simd_mpsad_edge_ignored_high_bits_mem",
            "{vex} vmpsadbw $0xf8, 32(%rax), %ymm3, %ymm1",
            Avx2,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: IntSatEdge,
        });
    }

    for &(label, asm, profile) in &[
        (
            "vaddsubps_avx_simd_horizontal_edge_xmm_self",
            "{vex} vaddsubps %xmm1, %xmm1, %xmm1",
            F32,
        ),
        (
            "vaddsubpd_avx_simd_horizontal_edge_ymm_unaligned_mem",
            "{vex} vaddsubpd 33(%rax), %ymm3, %ymm1",
            F64,
        ),
        (
            "vaddsubps_avx_simd_horizontal_edge_ymm_high_regs",
            "{vex} vaddsubps %ymm10, %ymm11, %ymm9",
            F32,
        ),
        (
            "vaddsubpd_avx_simd_horizontal_edge_xmm_zero_upper",
            "{vex} vaddsubpd %xmm2, %xmm3, %xmm1",
            F64,
        ),
        (
            "vhaddps_avx_simd_horizontal_edge_ymm_self",
            "{vex} vhaddps %ymm1, %ymm1, %ymm1",
            F32,
        ),
        (
            "vhaddpd_avx_simd_horizontal_edge_ymm_mem",
            "{vex} vhaddpd 32(%rax), %ymm3, %ymm1",
            F64,
        ),
        (
            "vhsubps_avx_simd_horizontal_edge_xmm_self",
            "{vex} vhsubps %xmm1, %xmm1, %xmm1",
            F32,
        ),
        (
            "vhsubps_avx_simd_horizontal_edge_ymm_high_regs",
            "{vex} vhsubps %ymm10, %ymm11, %ymm9",
            F32,
        ),
        (
            "vhsubpd_avx_simd_horizontal_edge_xmm_unaligned_mem",
            "{vex} vhsubpd 17(%rax), %xmm3, %xmm1",
            F64,
        ),
        (
            "vhaddps_avx_simd_horizontal_edge_xmm_zero_upper",
            "{vex} vhaddps %xmm2, %xmm3, %xmm1",
            F32,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Avx,
            profile,
        });
    }

    for &(label, asm) in &[
        (
            "vphaddw_avx2_simd_horizontal_edge_xmm_reg",
            "{vex} vphaddw %xmm2, %xmm3, %xmm1",
        ),
        (
            "vphaddw_avx2_simd_horizontal_edge_ymm_mem",
            "{vex} vphaddw 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vphaddd_avx2_simd_horizontal_edge_xmm_mem",
            "{vex} vphaddd 32(%rax), %xmm3, %xmm1",
        ),
        (
            "vphaddd_avx2_simd_horizontal_edge_ymm_reg",
            "{vex} vphaddd %ymm2, %ymm3, %ymm1",
        ),
        (
            "vphaddsw_avx2_simd_horizontal_edge_xmm_reg",
            "{vex} vphaddsw %xmm2, %xmm3, %xmm1",
        ),
        (
            "vphaddsw_avx2_simd_horizontal_edge_ymm_mem",
            "{vex} vphaddsw 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vphsubw_avx2_simd_horizontal_edge_xmm_mem",
            "{vex} vphsubw 32(%rax), %xmm3, %xmm1",
        ),
        (
            "vphsubw_avx2_simd_horizontal_edge_ymm_reg",
            "{vex} vphsubw %ymm2, %ymm3, %ymm1",
        ),
        (
            "vphsubd_avx2_simd_horizontal_edge_xmm_reg",
            "{vex} vphsubd %xmm2, %xmm3, %xmm1",
        ),
        (
            "vphsubd_avx2_simd_horizontal_edge_ymm_mem",
            "{vex} vphsubd 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vphsubsw_avx2_simd_horizontal_edge_xmm_mem",
            "{vex} vphsubsw 32(%rax), %xmm3, %xmm1",
        ),
        (
            "vphsubsw_avx2_simd_horizontal_edge_ymm_reg",
            "{vex} vphsubsw %ymm2, %ymm3, %ymm1",
        ),
        (
            "vpmaddubsw_avx2_simd_horizontal_edge_xmm_reg",
            "{vex} vpmaddubsw %xmm2, %xmm3, %xmm1",
        ),
        (
            "vpmaddubsw_avx2_simd_horizontal_edge_ymm_mem",
            "{vex} vpmaddubsw 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vpmulhrsw_avx2_simd_horizontal_edge_xmm_mem",
            "{vex} vpmulhrsw 32(%rax), %xmm3, %xmm1",
        ),
        (
            "vpmulhrsw_avx2_simd_horizontal_edge_ymm_reg",
            "{vex} vpmulhrsw %ymm2, %ymm3, %ymm1",
        ),
        (
            "vphminposuw_avx2_simd_horizontal_edge_xmm_reg",
            "{vex} vphminposuw %xmm2, %xmm1",
        ),
        (
            "vphminposuw_avx2_simd_horizontal_edge_xmm_mem",
            "{vex} vphminposuw 32(%rax), %xmm1",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Avx2,
            profile: IntSatEdge,
        });
    }

    for &(label, asm) in &[
        (
            "vpshufb_avx2_simd_shuffle_edge_xmm_reg",
            "{vex} vpshufb %xmm2, %xmm3, %xmm1",
        ),
        (
            "vpshufb_avx2_simd_shuffle_edge_xmm_mem",
            "{vex} vpshufb 32(%rax), %xmm3, %xmm1",
        ),
        (
            "vpshufb_avx2_simd_shuffle_edge_ymm_reg",
            "{vex} vpshufb %ymm2, %ymm3, %ymm1",
        ),
        (
            "vpshufb_avx2_simd_shuffle_edge_ymm_mem",
            "{vex} vpshufb 32(%rax), %ymm3, %ymm1",
        ),
        (
            "vpshufb_avx2_simd_shuffle_edge_xmm_zero_selector",
            "{vex} vpxor %xmm2, %xmm2, %xmm2\n{vex} vpshufb %xmm2, %xmm3, %xmm1",
        ),
        (
            "vpshufb_avx2_simd_shuffle_edge_ymm_zero_selector",
            "{vex} vpxor %ymm2, %ymm2, %ymm2\n{vex} vpshufb %ymm2, %ymm3, %ymm1",
        ),
        (
            "vpshufb_avx2_simd_shuffle_edge_xmm_all_highbits",
            "{vex} vpcmpeqb %xmm2, %xmm2, %xmm2\n{vex} vpshufb %xmm2, %xmm3, %xmm1",
        ),
        (
            "vpshufb_avx2_simd_shuffle_edge_ymm_all_highbits",
            "{vex} vpcmpeqb %ymm2, %ymm2, %ymm2\n{vex} vpshufb %ymm2, %ymm3, %ymm1",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Avx2,
            profile: IntSatEdge,
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
    for mnem in ["aesenc", "aesenclast", "aesdec", "aesdeclast"] {
        for (tag, asm) in [
            (
                "disp",
                format!("{mnem} -16(%rbx), %xmm1"),
            ),
            (
                "addr32_disp",
                format!("addr32 {mnem} -16(%ebx), %xmm1"),
            ),
            (
                "high_disp",
                format!("{mnem} -16(%rbx), %xmm9"),
            ),
        ] {
            out.push(Case {
                label: format!("{mnem}_legacy_aes_addr_{tag}"),
                asm,
                feat: Aes,
                profile: Int,
            });
        }
    }
    for &(label, asm) in &[
        (
            "aesimc_legacy_aes_addr_disp",
            "aesimc -16(%rbx), %xmm1",
        ),
        (
            "aesimc_legacy_aes_addr_addr32_disp",
            "addr32 aesimc -16(%ebx), %xmm1",
        ),
        (
            "aesimc_legacy_aes_addr_high_disp",
            "aesimc -16(%rbx), %xmm9",
        ),
        (
            "aeskeygenassist_legacy_aes_addr_disp",
            "aeskeygenassist $0x00, -16(%rbx), %xmm1",
        ),
        (
            "aeskeygenassist_legacy_aes_addr_addr32_disp",
            "addr32 aeskeygenassist $0xff, -16(%ebx), %xmm1",
        ),
        (
            "aeskeygenassist_legacy_aes_addr_high_disp",
            "aeskeygenassist $0x36, -16(%rbx), %xmm9",
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
    for &(imm, tag) in &[(0x00, "ll"), (0x01, "hl"), (0x10, "lh"), (0x11, "hh")] {
        for (form, asm) in [
            (
                "disp",
                format!("pclmulqdq ${imm:#x}, -16(%rbx), %xmm1"),
            ),
            (
                "addr32_disp",
                format!("addr32 pclmulqdq ${imm:#x}, -16(%ebx), %xmm1"),
            ),
            (
                "high_disp",
                format!("pclmulqdq ${imm:#x}, -16(%rbx), %xmm9"),
            ),
        ] {
            out.push(Case {
                label: format!("pclmulqdq_legacy_crypto_addr_{tag}_{form}"),
                asm,
                feat: Pclmulqdq,
                profile: Int,
            });
        }
    }

    // Legacy AES-NI/PCLMUL XMM edge operands. These keep the legacy decoder
    // and XMM write semantics under exact zero/all-one inputs, separate from
    // the EVEX crypto edge corpus above.
    for &(label, asm, feat) in &[
        (
            "aesenc_legacy_xmm_edge_zero_key",
            "pxor %xmm2, %xmm2\naesenc %xmm2, %xmm1",
            Aes,
        ),
        (
            "aesenc_legacy_xmm_edge_allones_mem",
            "pcmpeqb %xmm2, %xmm2\nmovdqu %xmm2, 64(%rax)\naesenc 64(%rax), %xmm1",
            Aes,
        ),
        (
            "aesenclast_legacy_xmm_edge_zero_key",
            "pxor %xmm2, %xmm2\naesenclast %xmm2, %xmm1",
            Aes,
        ),
        (
            "aesenclast_legacy_xmm_edge_allones_mem",
            "pcmpeqb %xmm2, %xmm2\nmovdqu %xmm2, 64(%rax)\naesenclast 64(%rax), %xmm1",
            Aes,
        ),
        (
            "aesdec_legacy_xmm_edge_zero_key",
            "pxor %xmm2, %xmm2\naesdec %xmm2, %xmm1",
            Aes,
        ),
        (
            "aesdec_legacy_xmm_edge_allones_mem",
            "pcmpeqb %xmm2, %xmm2\nmovdqu %xmm2, 64(%rax)\naesdec 64(%rax), %xmm1",
            Aes,
        ),
        (
            "aesdeclast_legacy_xmm_edge_zero_key",
            "pxor %xmm2, %xmm2\naesdeclast %xmm2, %xmm1",
            Aes,
        ),
        (
            "aesdeclast_legacy_xmm_edge_allones_mem",
            "pcmpeqb %xmm2, %xmm2\nmovdqu %xmm2, 64(%rax)\naesdeclast 64(%rax), %xmm1",
            Aes,
        ),
        (
            "aesimc_legacy_xmm_edge_zero_reg",
            "pxor %xmm2, %xmm2\naesimc %xmm2, %xmm1",
            Aes,
        ),
        (
            "aeskeygenassist_legacy_xmm_edge_immff_allones_mem",
            "pcmpeqb %xmm2, %xmm2\nmovdqu %xmm2, 64(%rax)\naeskeygenassist $0xff, 64(%rax), %xmm1",
            Aes,
        ),
        (
            "pclmulqdq_legacy_xmm_edge_ll_zero_reg",
            "pxor %xmm2, %xmm2\npclmulqdq $0x00, %xmm2, %xmm1",
            Pclmulqdq,
        ),
        (
            "pclmulqdq_legacy_xmm_edge_hl_allones_mem",
            "pcmpeqb %xmm2, %xmm2\nmovdqu %xmm2, 64(%rax)\npclmulqdq $0x01, 64(%rax), %xmm1",
            Pclmulqdq,
        ),
        (
            "pclmulqdq_legacy_xmm_edge_lh_zero_reg",
            "pxor %xmm2, %xmm2\npclmulqdq $0x10, %xmm2, %xmm1",
            Pclmulqdq,
        ),
        (
            "pclmulqdq_legacy_xmm_edge_hh_allones_mem",
            "pcmpeqb %xmm2, %xmm2\nmovdqu %xmm2, 64(%rax)\npclmulqdq $0x11, 64(%rax), %xmm1",
            Pclmulqdq,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }

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
    for mnem in [
        "sha1nexte",
        "sha1msg1",
        "sha1msg2",
        "sha256rnds2",
        "sha256msg1",
        "sha256msg2",
    ] {
        for (tag, asm) in [
            (
                "disp",
                format!("{mnem} -16(%rbx), %xmm1"),
            ),
            (
                "addr32_disp",
                format!("addr32 {mnem} -16(%ebx), %xmm1"),
            ),
            (
                "high_disp",
                format!("{mnem} -16(%rbx), %xmm9"),
            ),
        ] {
            out.push(Case {
                label: format!("{mnem}_sha_ni_addr_{tag}"),
                asm,
                feat: Sha,
                profile: Int,
            });
        }
    }
    for imm in 0..=3 {
        for (tag, asm) in [
            (
                "disp",
                format!("sha1rnds4 ${imm}, -16(%rbx), %xmm1"),
            ),
            (
                "addr32_disp",
                format!("addr32 sha1rnds4 ${imm}, -16(%ebx), %xmm1"),
            ),
            (
                "high_disp",
                format!("sha1rnds4 ${imm}, -16(%rbx), %xmm9"),
            ),
        ] {
            out.push(Case {
                label: format!("sha1rnds4_imm{imm}_sha_ni_addr_{tag}"),
                asm,
                feat: Sha,
                profile: Int,
            });
        }
    }

    // Legacy GFNI/SHA-NI edge operands. These keep non-VEX XMM decoder paths
    // under exact zero/all-one inputs and include SHA1RNDS4 endpoint functions.
    for &(label, asm, feat) in &[
        (
            "gf2p8mulb_legacy_gfni_sha_edge_zero_reg",
            "pxor %xmm2, %xmm2\ngf2p8mulb %xmm2, %xmm1",
            Gfni,
        ),
        (
            "gf2p8mulb_legacy_gfni_sha_edge_allones_mem",
            "pcmpeqb %xmm2, %xmm2\nmovdqu %xmm2, 64(%rax)\ngf2p8mulb 64(%rax), %xmm1",
            Gfni,
        ),
        (
            "gf2p8affineqb_legacy_gfni_sha_edge_imm0_zero_reg",
            "pxor %xmm2, %xmm2\ngf2p8affineqb $0x00, %xmm2, %xmm1",
            Gfni,
        ),
        (
            "gf2p8affineqb_legacy_gfni_sha_edge_immff_allones_mem",
            "pcmpeqb %xmm2, %xmm2\nmovdqu %xmm2, 64(%rax)\ngf2p8affineqb $0xff, 64(%rax), %xmm1",
            Gfni,
        ),
        (
            "gf2p8affineinvqb_legacy_gfni_sha_edge_imm0_zero_reg",
            "pxor %xmm2, %xmm2\ngf2p8affineinvqb $0x00, %xmm2, %xmm1",
            Gfni,
        ),
        (
            "gf2p8affineinvqb_legacy_gfni_sha_edge_immff_allones_mem",
            "pcmpeqb %xmm2, %xmm2\nmovdqu %xmm2, 64(%rax)\ngf2p8affineinvqb $0xff, 64(%rax), %xmm1",
            Gfni,
        ),
        (
            "sha1nexte_legacy_gfni_sha_edge_zero_reg",
            "pxor %xmm2, %xmm2\nsha1nexte %xmm2, %xmm1",
            Sha,
        ),
        (
            "sha1nexte_legacy_gfni_sha_edge_allones_mem",
            "pcmpeqb %xmm2, %xmm2\nmovdqu %xmm2, 64(%rax)\nsha1nexte 64(%rax), %xmm1",
            Sha,
        ),
        (
            "sha1msg1_legacy_gfni_sha_edge_zero_reg",
            "pxor %xmm2, %xmm2\nsha1msg1 %xmm2, %xmm1",
            Sha,
        ),
        (
            "sha1msg1_legacy_gfni_sha_edge_allones_mem",
            "pcmpeqb %xmm2, %xmm2\nmovdqu %xmm2, 64(%rax)\nsha1msg1 64(%rax), %xmm1",
            Sha,
        ),
        (
            "sha1msg2_legacy_gfni_sha_edge_zero_reg",
            "pxor %xmm2, %xmm2\nsha1msg2 %xmm2, %xmm1",
            Sha,
        ),
        (
            "sha1msg2_legacy_gfni_sha_edge_allones_mem",
            "pcmpeqb %xmm2, %xmm2\nmovdqu %xmm2, 64(%rax)\nsha1msg2 64(%rax), %xmm1",
            Sha,
        ),
        (
            "sha256rnds2_legacy_gfni_sha_edge_zero_reg",
            "pxor %xmm2, %xmm2\nsha256rnds2 %xmm2, %xmm1",
            Sha,
        ),
        (
            "sha256rnds2_legacy_gfni_sha_edge_allones_mem",
            "pcmpeqb %xmm2, %xmm2\nmovdqu %xmm2, 64(%rax)\nsha256rnds2 64(%rax), %xmm1",
            Sha,
        ),
        (
            "sha256msg1_legacy_gfni_sha_edge_zero_reg",
            "pxor %xmm2, %xmm2\nsha256msg1 %xmm2, %xmm1",
            Sha,
        ),
        (
            "sha256msg1_legacy_gfni_sha_edge_allones_mem",
            "pcmpeqb %xmm2, %xmm2\nmovdqu %xmm2, 64(%rax)\nsha256msg1 64(%rax), %xmm1",
            Sha,
        ),
        (
            "sha256msg2_legacy_gfni_sha_edge_zero_reg",
            "pxor %xmm2, %xmm2\nsha256msg2 %xmm2, %xmm1",
            Sha,
        ),
        (
            "sha256msg2_legacy_gfni_sha_edge_allones_mem",
            "pcmpeqb %xmm2, %xmm2\nmovdqu %xmm2, 64(%rax)\nsha256msg2 64(%rax), %xmm1",
            Sha,
        ),
        (
            "sha1rnds4_legacy_gfni_sha_edge_imm0_zero_reg",
            "pxor %xmm2, %xmm2\nsha1rnds4 $0, %xmm2, %xmm1",
            Sha,
        ),
        (
            "sha1rnds4_legacy_gfni_sha_edge_imm3_allones_mem",
            "pcmpeqb %xmm2, %xmm2\nmovdqu %xmm2, 64(%rax)\nsha1rnds4 $3, 64(%rax), %xmm1",
            Sha,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
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
    for &(label, asm, feat) in &[
        (
            "movdiri_m32_r9d_indexed",
            "movdiri %r9d, -32(%rbx,%r9,1)",
            Movdiri,
        ),
        (
            "movdiri_m64_r9_indexed",
            "movdiri %r9, -24(%rbx,%r9,1)",
            Movdiri,
        ),
        (
            "movdiri_m32_r9d_addr32_indexed",
            "addr32 movdiri %r9d, -32(%rbx,%r9,1)",
            Movdiri,
        ),
        (
            "movdiri_m64_r9_addr32_indexed",
            "addr32 movdiri %r9, -24(%rbx,%r9,1)",
            Movdiri,
        ),
        (
            "movdir64b_scratch_128_to_0_indexed",
            "movdir64b 42(%rbx,%r9,1), %rax",
            Movdir64b,
        ),
        (
            "movdir64b_scratch_128_to_0_addr32_indexed",
            "addr32 movdir64b 42(%rbx,%r9,1), %rax",
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
    for &(label, asm, feat) in &[
        (
            "movdiri_movdir_edge_m32_unaligned",
            "movdiri %r8d, 3(%rax)",
            Movdiri,
        ),
        (
            "movdiri_movdir_edge_m64_unaligned",
            "movdiri %r8, 5(%rax)",
            Movdiri,
        ),
        (
            "movdiri_movdir_edge_r15_m64_source",
            "movabsq $0x0123456789abcdef, %r15\nmovdiri %r15, 40(%rax)",
            Movdiri,
        ),
        (
            "movdiri_movdir_edge_negative_disp",
            "leaq 80(%rax), %r15\nmovdiri %r9d, -7(%r15)",
            Movdiri,
        ),
        (
            "movdiri_movdir_edge_flags_preserved",
            "cmpq %r8, %r8\nmovdiri %r9, 24(%rax)",
            Movdiri,
        ),
        (
            "movdir64b_movdir_edge_r8_dest",
            "leaq 64(%rax), %r8\nmovdir64b 128(%rax), %r8",
            Movdir64b,
        ),
        (
            "movdir64b_movdir_edge_r15_dest",
            "leaq 128(%rax), %r15\nmovdir64b 64(%rax), %r15",
            Movdir64b,
        ),
        (
            "movdir64b_movdir_edge_unaligned_source",
            "leaq 192(%rax), %r10\nmovdir64b 33(%rax), %r10",
            Movdir64b,
        ),
        (
            "movdir64b_movdir_edge_r9_source_base",
            "leaq 128(%rax), %r9\nmovdir64b (%r9), %rax",
            Movdir64b,
        ),
        (
            "movdir64b_movdir_edge_flags_preserved",
            "cmpq %r8, %r8\nmovdir64b 128(%rax), %rax",
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

    for &(label, asm) in &[
        ("adcx_adx_operand_r64_r9", "adcx %r9, %r8"),
        ("adcx_adx_operand_r32_r9d", "adcx %r9d, %r8d"),
        ("adcx_adx_operand_m64_disp_r8", "adcx 16(%rax), %r8"),
        ("adcx_adx_operand_m32_disp_r8d", "adcx 20(%rax), %r8d"),
        ("adox_adx_operand_r64_r9", "adox %r9, %r8"),
        ("adox_adx_operand_r32_r9d", "adox %r9d, %r8d"),
        ("adox_adx_operand_m64_disp_r8", "adox 24(%rax), %r8"),
        ("adox_adx_operand_m32_disp_r8d", "adox 28(%rax), %r8d"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Adx,
            profile: Int,
        });
    }
    for &(label, asm) in &[
        (
            "adcx_adx_edge_carry_chain_r64",
            "clc\nmovq $-1, %r8\nmovq $1, %r9\nadcx %r9, %r8\nadcx %r9, %r8",
        ),
        (
            "adcx_adx_edge_initial_carry_in_r64",
            "stc\nmovq $-1, %r8\nmovq $0, %r9\nadcx %r9, %r8",
        ),
        (
            "adcx_adx_edge_preserves_of_r32",
            "movl $0x7fffffff, %r8d\naddl $1, %r8d\nclc\nmovl $5, %r8d\nmovl $3, %r9d\nadcx %r9d, %r8d",
        ),
        (
            "adox_adx_edge_overflow_chain_r64",
            "xorl %r10d, %r10d\nmovq $-1, %r8\nmovq $1, %r9\nadox %r9, %r8\nadox %r9, %r8",
        ),
        (
            "adox_adx_edge_initial_overflow_in_r32",
            "movl $0x7fffffff, %r10d\naddl $1, %r10d\nmovl $0, %r8d\nmovl $0, %r9d\nadox %r9d, %r8d",
        ),
        (
            "adox_adx_edge_preserves_cf_r64",
            "xorl %r10d, %r10d\nstc\nmovq $5, %r8\nmovq $3, %r9\nadox %r9, %r8",
        ),
        (
            "adcx_adox_adx_edge_independent_flags_r64",
            "xorl %r10d, %r10d\nmovq $-1, %r8\nmovq $1, %r9\nadcx %r9, %r8\nmovq $-1, %rcx\nmovq $1, %rdx\nadox %rdx, %rcx",
        ),
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

    for &(label, asm) in &[
        ("movbe_movbe_operand_r16_m16_disp", "movbe 16(%rax), %r8w"),
        ("movbe_movbe_operand_r32_m32_disp", "movbe 20(%rax), %r8d"),
        ("movbe_movbe_operand_r64_m64_disp", "movbe 24(%rax), %r8"),
        ("movbe_movbe_operand_m16_r9w_disp", "movbe %r9w, 32(%rax)"),
        ("movbe_movbe_operand_m32_r9d_disp", "movbe %r9d, 40(%rax)"),
        ("movbe_movbe_operand_m64_r9_disp", "movbe %r9, 48(%rax)"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Movbe,
            profile: Int,
        });
    }

    for &(label, asm) in &[
        ("movbe_movbe_edge_r16_unaligned_1", "movbe 1(%rax), %r8w"),
        ("movbe_movbe_edge_r32_unaligned_3", "movbe 3(%rax), %r8d"),
        ("movbe_movbe_edge_r64_unaligned_5", "movbe 5(%rax), %r8"),
        ("movbe_movbe_edge_r16_high_dest", "movbe 2(%rax), %r9w"),
        ("movbe_movbe_edge_r32_high_dest", "movbe 4(%rax), %r9d"),
        ("movbe_movbe_edge_r64_high_dest", "movbe 8(%rax), %r15"),
        ("movbe_movbe_edge_m16_high_src", "movbe %r9w, 57(%rax)"),
        ("movbe_movbe_edge_m32_high_src", "movbe %r9d, 61(%rax)"),
        ("movbe_movbe_edge_m64_high_src", "movbe %r9, 65(%rax)"),
        (
            "movbe_movbe_edge_r64_roundtrip",
            "movbe %r8, 80(%rax)\nmovbe 80(%rax), %rcx",
        ),
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

    // Effective-address edge cases. LEA observes address-size and destination
    // width without touching memory, while XLAT uses AL as an unsigned table
    // index and preserves the rest of RAX.
    for &(label, asm) in &[
        (
            "lea_core_address_edge_addr32_high_base",
            "movabsq $0xffff000000004000, %rbx\nmovl $0x20, %ecx\naddr32 leaq (%ebx,%ecx,4), %r8",
        ),
        (
            "lea_core_address_edge_addr32_negative_disp",
            "movabsq $0xffff000000004010, %rbx\naddr32 leaq -32(%ebx), %r8",
        ),
        (
            "lea_core_address_edge_sib_no_base",
            "leaq 64(,%r9,8), %r8",
        ),
        (
            "lea_core_address_edge_negative_index",
            "movq $-2, %r10\nleaq 128(%rax,%r10,8), %r8",
        ),
        (
            "lea_core_address_edge_r32_dest_zeroext",
            "movabsq $-1, %r8\nleal -16(%rax,%r9,4), %r8d",
        ),
        ("lea_core_address_edge_rip_relative", "leaq 0(%rip), %r8"),
        (
            "mov_core_address_edge_rip_relative_load",
            "movq 1f(%rip), %r8\njmp 2f\n1:\n.quad 0x8877665544332211\n2:",
        ),
        (
            "xlat_core_address_edge_al_ff",
            "movabsq $0x4000, %rbx\nmovb $0xff, %al\nxlatb",
        ),
        (
            "xlat_core_address_edge_al_zero_preserves_high",
            "movabsq $0x4000, %rbx\nmovabsq $0xffff000000000000, %rax\nxlatb",
        ),
        (
            "xlat_core_address_edge_shifted_table",
            "leaq 32(%rax), %rbx\nmovb $0x20, %al\nxlatb",
        ),
        (
            "addr32_xlat_core_address_edge_high_rbx",
            "movabsq $0xffff000000004000, %rbx\nmovb $0x10, %al\naddr32 xlatb",
        ),
        (
            "addr32_xlat_core_address_edge_al_ff",
            "movabsq $0xffff000000004000, %rbx\nmovb $0xff, %al\naddr32 xlatb",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // Direct flag-state edge cases around CF/DF chains and LAHF/SAHF's partial
    // status-byte transfer. These keep OF preservation and AH bit layout
    // visible in the architectural diff.
    for &(label, asm) in &[
        ("clc_cmc_core_flag_edge_cf_set", "clc\ncmc"),
        ("stc_cmc_core_flag_edge_cf_clear", "stc\ncmc"),
        ("std_cld_core_flag_edge_df_clear", "std\ncld"),
        ("cld_std_core_flag_edge_df_set", "cld\nstd"),
        (
            "lahf_core_flag_edge_all_status_bits",
            "pushq $0x8d7\npopfq\nlahf",
        ),
        (
            "sahf_core_flag_edge_all_status_bits",
            "movw $0xd500, %ax\nsahf",
        ),
        (
            "sahf_core_flag_edge_clear_status_preserve_of",
            "movw $0x0200, %ax\nsahf",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // Core data-movement/addressing width variants. These exercise MOV
    // immediate encodings beyond the starter set, SPL/high-byte byte-register
    // selection, RIP-relative and SIB addressing, and stack-width forms with
    // visible scratch/stack effects.
    for &(label, asm) in &[
        ("mov_core_data_width_r16_imm", "movw $0x1234, %r8w"),
        ("mov_core_data_width_r64_imm32_signext", "movq $-7, %r8"),
        ("mov_core_data_width_m8_imm", "movb $0xaa, 24(%rax)"),
        ("mov_core_data_width_m32_imm", "movl $0x89abcdef, 28(%rax)"),
        ("mov_core_data_width_m64_imm32", "movq $-7, 32(%rax)"),
        ("mov_core_data_width_high8_imm", "movb $0x55, %ah"),
        ("mov_core_data_width_spl_imm", "movb $0x66, %spl"),
        ("mov_core_data_width_m8_spl", "movb %spl, 40(%rax)"),
        ("mov_core_data_width_r8_spl", "movb %spl, %r8b"),
        (
            "mov_core_data_width_rip_relative",
            "jmp 2f\n1:\n.quad 0x1122334455667788\n2:\nmovq 1b(%rip), %r8",
        ),
        (
            "mov_core_data_width_sib_no_base",
            "movl 0x3ef0(,%r9,4), %r8d",
        ),
        ("mov_core_data_width_rbp_store", "movq %r8, -96(%rbp)"),
        ("mov_core_data_width_rsp_store", "movq %r8, 16(%rsp)"),
        ("lea_core_data_width_negative_disp", "leaq -16(%rax), %r8"),
        (
            "lea_core_data_width_addr32_indexed",
            "addr32 leaq 16(%eax,%ecx,2), %r8",
        ),
        ("lea_core_data_width_rsp_sib", "leaq 16(%rsp,%r9,2), %r8"),
        ("push_core_data_width_r9_pop", "pushq %r9\npopq %r8"),
        ("push_core_data_width_r9w_pop", "pushw %r9w\npopw %r8w"),
        ("pop_core_data_width_r9", "pushq %r8\npopq %r9"),
        ("pop_core_data_width_m16", "pushw $0x1234\npopw 42(%rax)"),
        ("pop_core_data_width_m64", "pushq %r8\npopq 48(%rax)"),
        ("pop_core_data_width_m16_rbp", "pushw %r8w\npopw -80(%rbp)"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // Scalar integer extension instructions, including the legacy accumulator
    // forms and MOVSXD's accepted non-REX.W encodings in 64-bit mode.
    for &(label, asm) in &[
        ("cbw_core_extend_negative", "movw $0x0080, %ax\ncbtw"),
        ("cwde_core_extend_negative", "movl $0x00008000, %eax\ncwtl"),
        ("cdqe_core_extend_negative", "movl $0x80000000, %eax\ncltq"),
        ("cwd_core_extend_negative", "movw $0x8000, %ax\ncwtd"),
        ("cdq_core_extend_negative", "movl $0x80000000, %eax\ncltd"),
        ("cqo_core_extend_negative", "movq $-1, %rax\ncqto"),
        ("cbw_core_extend_positive", "movw $0x007f, %ax\ncbtw"),
        ("cwde_core_extend_positive", "movl $0x00007fff, %eax\ncwtl"),
        ("cdqe_core_extend_positive", "movl $0x7fffffff, %eax\ncltq"),
        ("cwd_core_extend_positive", "movw $0x7fff, %ax\ncwtd"),
        ("cdq_core_extend_positive", "movl $0x7fffffff, %eax\ncltd"),
        ("cqo_core_extend_positive", "movq $0x7fffffff, %rax\ncqto"),
        ("movzx_core_extend_r8_to_r64", "movzbq %cl, %r8"),
        ("movzx_core_extend_high8_to_r32", "movzbl %ch, %edx"),
        ("movzx_core_extend_r16_to_r64", "movzwq %cx, %r8"),
        ("movzx_core_extend_m8_to_r32", "movzbl (%rax), %r8d"),
        ("movzx_core_extend_m8_to_r16", "movzbw 1(%rax), %r8w"),
        ("movzx_core_extend_m16_to_r32", "movzwl 2(%rax), %r8d"),
        ("movzx_core_extend_m16_to_r64", "movzwq 2(%rax), %r8"),
        ("movsx_core_extend_r8_to_r64", "movsbq %cl, %r8"),
        ("movsx_core_extend_high8_to_r32", "movsbl %ch, %edx"),
        ("movsx_core_extend_r16_to_r64", "movswq %cx, %r8"),
        ("movsx_core_extend_m8_to_r32", "movsbl (%rax), %r8d"),
        ("movsx_core_extend_m8_to_r16", "movsbw (%rax), %r8w"),
        ("movsx_core_extend_m16_to_r32", "movswl 2(%rax), %r8d"),
        ("movsx_core_extend_m16_to_r64", "movswq 2(%rax), %r8"),
        ("movsxd_core_extend_r32_to_r64", "movslq %ecx, %r8"),
        ("movsxd_core_extend_m32_to_r64", "movslq 4(%rax), %r8"),
        ("movsxd_core_extend_r32_default", ".byte 0x63, 0xc1\n"),
        ("movsxd_core_extend_r16_operand", ".byte 0x66, 0x63, 0xc1\n"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // Direct MOV moffs opcodes (A0-A3) carry an absolute offset operand and are
    // distinct from the usual ModRM absolute-address encodings.
    for &(label, asm) in &[
        ("mov_core_moffs_load_al", "movabsb 0x4000, %al"),
        ("mov_core_moffs_load_ax", "movabsw 0x4002, %ax"),
        ("mov_core_moffs_load_eax", "movabsl 0x4004, %eax"),
        ("mov_core_moffs_load_rax", "movabsq 0x4008, %rax"),
        ("mov_core_moffs_store_al", "movabsb %al, 0x4010"),
        ("mov_core_moffs_store_ax", "movabsw %ax, 0x4012"),
        ("mov_core_moffs_store_eax", "movabsl %eax, 0x4014"),
        ("mov_core_moffs_store_rax", "movabsq %rax, 0x4018"),
        (
            "mov_core_moffs_edge_addr32_load_al",
            "addr32 movabsb 0x4001, %al",
        ),
        (
            "mov_core_moffs_edge_addr32_load_ax",
            "addr32 movabsw 0x4002, %ax",
        ),
        (
            "mov_core_moffs_edge_load_al_preserves_high",
            "movabsq $0xffff000000000000, %rax\nmovabsb 0x4001, %al",
        ),
        (
            "mov_core_moffs_edge_load_ax_preserves_high",
            "movabsq $0xffff000000000000, %rax\nmovabsw 0x4002, %ax",
        ),
        (
            "mov_core_moffs_edge_load_eax_zero_ext",
            "movabsq $0xffff000000000000, %rax\nmovabsl 0x4004, %eax",
        ),
        (
            "mov_core_moffs_edge_store_ax_high_source",
            "movabsq $0xffff00000000abcd, %rax\nmovabsw %ax, 0x4038",
        ),
        (
            "mov_core_moffs_edge_store_eax_high_source",
            "movabsq $0xffff000089abcdef, %rax\nmovabsl %eax, 0x403c",
        ),
        (
            "mov_core_moffs_edge_store_rax_high_source",
            "movabsq $0x1122334455667788, %rax\nmovabsq %rax, 0x4040",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // FS/GS segment-register stack and move forms remain valid in 64-bit mode.
    // The cases use null selectors and observe them through GPR or scratch state.
    for &(label, asm) in &[
        (
            "push_core_segment_fs_qword",
            "pushq $0\npopq %fs\npushq %fs\npopq %r8",
        ),
        (
            "push_core_segment_gs_qword",
            "pushq $0\npopq %gs\npushq %gs\npopq %r8",
        ),
        (
            "push_core_segment_fs_word",
            "pushw $0\npopw %fs\npushw %fs\npopw %r8w",
        ),
        (
            "push_core_segment_gs_word",
            "pushw $0\npopw %gs\npushw %gs\npopw %r8w",
        ),
        (
            "pop_core_segment_fs_qword",
            "pushq $0\npopq %fs\nmovw %fs, %r8w",
        ),
        (
            "pop_core_segment_gs_qword",
            "pushq $0\npopq %gs\nmovw %gs, %r8w",
        ),
        (
            "pop_core_segment_fs_word",
            "pushw $0\npopw %fs\nmovw %fs, %r8w",
        ),
        (
            "pop_core_segment_gs_word",
            "pushw $0\npopw %gs\nmovw %gs, %r8w",
        ),
        (
            "mov_core_segment_fs_to_reg",
            "pushq $0\npopq %fs\nmovw %fs, %r8w",
        ),
        (
            "mov_core_segment_gs_to_mem",
            "pushq $0\npopq %gs\nmovw %gs, 32(%rax)",
        ),
        (
            "mov_core_segment_reg_to_fs",
            "xor %ecx, %ecx\nmovw %cx, %fs\nmovw %fs, %r8w",
        ),
        (
            "mov_core_segment_reg_to_gs",
            "xor %ecx, %ecx\nmovw %cx, %gs\nmovw %gs, %r8w",
        ),
        (
            "mov_core_segment_mem_to_fs",
            "movw $0, 32(%rax)\nmovw 32(%rax), %fs\nmovw %fs, %r8w",
        ),
        (
            "mov_core_segment_mem_to_gs",
            "movw $0, 32(%rax)\nmovw 32(%rax), %gs\nmovw %gs, %r8w",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // Far-pointer loads update a GPR from m16:16/32/64 and load the paired
    // segment selector. The snippets install a scratch-local GDT so non-null
    // selector loads are valid, then make the selector visible through a GPR.
    let far_ptr_setup = "movq $0, 128(%rax)\nmovabsq $0x00209a0000000000, %r10\nmovq %r10, 136(%rax)\nmovabsq $0x0000920000000000, %r10\nmovq %r10, 144(%rax)\nmovw $0x0017, 32(%rax)\nleaq 128(%rax), %r10\nmovq %r10, 34(%rax)\nlgdt 32(%rax)";
    for &(label, asm) in &[
        (
            "lfs_core_far_pointer_load_edge_m16_64_selector",
            "movabsq $0x1122334455667788, %r10\nmovq %r10, 176(%rax)\nmovw $0x10, 184(%rax)\nmovabsq $-1, %r8\nlfsq 176(%rax), %r8\nmovw %fs, %r9w\nmovzwq %r9w, %r9",
        ),
        (
            "lgs_core_far_pointer_load_edge_m16_32_zeroext",
            "movl $0x87654321, 176(%rax)\nmovw $0x10, 180(%rax)\nmovabsq $-1, %r8\nlgsl 176(%rax), %r8d\nmovw %gs, %r9w\nmovzwq %r9w, %r9",
        ),
        (
            "lss_core_far_pointer_load_edge_m16_16_preserve_high",
            "movw $0x3456, 176(%rax)\nmovw $0x10, 178(%rax)\nmovabsq $0xfeedbeef00000000, %r8\nlssw 176(%rax), %r8w\nmovw %ss, %r9w\nmovzwq %r9w, %r9",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: format!("{far_ptr_setup}\n{asm}"),
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
    for &(label, asm) in &[
        (
            "inb_io_edge_dirty_rax_imm8",
            "movabsq $0x1122334455667788, %rax\ninb $0x80, %al",
        ),
        (
            "inw_io_edge_dirty_rax_imm8",
            "movabsq $0x1122334455667788, %rax\ninw $0x81, %ax",
        ),
        (
            "inl_io_edge_zeroext_imm8",
            "movabsq $0x1122334455667788, %rax\ninl $0x82, %eax",
        ),
        (
            "inb_io_edge_dirty_rax_dx",
            "movabsq $0x8877665544332211, %rax\nmovw $0x0080, %dx\ninb %dx, %al",
        ),
        (
            "inw_io_edge_dirty_rax_dx",
            "movabsq $0x8877665544332211, %rax\nmovw $0x0081, %dx\ninw %dx, %ax",
        ),
        (
            "inl_io_edge_zeroext_dx",
            "movabsq $0x8877665544332211, %rax\nmovw $0x0082, %dx\ninl %dx, %eax",
        ),
        (
            "outb_io_edge_preserves_rax_imm8",
            "movabsq $0x1020304050607080, %rax\ncmpq %rcx, %r8\noutb %al, $0x80",
        ),
        (
            "outw_io_edge_preserves_rax_dx",
            "movabsq $0x1020304050607080, %rax\nmovw $0x0081, %dx\ncmpq %rcx, %r8\noutw %ax, %dx",
        ),
        (
            "outl_io_edge_preserves_rax_imm8",
            "movabsq $0x1020304050607080, %rax\ncmpq %rcx, %r8\noutl %eax, $0x82",
        ),
        ("rep_insw_io_edge_string", "rep insw"),
        ("rep_insl_io_edge_string", "rep insl"),
        ("rep_insb_io_edge_count_zero", "rep insb"),
        ("rep_insw_io_edge_count_zero", "rep insw"),
        ("insb_io_edge_df", "insb"),
        ("insw_io_edge_df", "insw"),
        ("insl_io_edge_df", "insl"),
        ("addr32_insw_io_edge_string", "addr32 insw"),
        ("addr32_insl_io_edge_string", "addr32 insl"),
        ("rep_outsw_io_edge_string", "rep outsw"),
        ("rep_outsl_io_edge_string", "rep outsl"),
        ("rep_outsb_io_edge_count_zero", "rep outsb"),
        ("rep_outsw_io_edge_count_zero", "rep outsw"),
        ("outsb_io_edge_df", "outsb"),
        ("outsw_io_edge_df", "outsw"),
        ("outsl_io_edge_df", "outsl"),
        ("addr32_outsw_io_edge_string", "addr32 outsw"),
        ("addr32_outsl_io_edge_string", "addr32 outsl"),
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
        (
            "syscall_fast_syscall_edge_saves_rcx_r11_and_masks_flags",
            "movq %rax, %rdi\nmovl $0xc0000080, %ecx\nrdmsr\norl $1, %eax\nwrmsr\nmovabsq $0x0018000800000000, %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nmovl $0xc0000081, %ecx\nwrmsr\nleaq 1f(%rip), %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nmovl $0xc0000082, %ecx\nwrmsr\nmovl $0x600, %eax\nxorl %edx, %edx\nmovl $0xc0000084, %ecx\nwrmsr\nleaq 2f(%rip), %r8\npushq $0x602\npopfq\nsyscall\n2:\nmovq $0xbad, %rbx\njmp 3f\n1:\ncmpq %r8, %rcx\nsete %al\nmovq %r11, %r9\nandq $0x600, %r9\ncmpq $0x600, %r9\nsete %dl\nandb %dl, %al\npushfq\npopq %r9\nandq $0x600, %r9\ncmpq $0, %r9\nsete %dl\nandb %dl, %al\nmovzbl %al, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r9, %r9\nmovq $0x5152, %rbx\nmovq $0x8888, %r8\ncmpq %r8, %r8\n3:",
        ),
        (
            "sysret_fast_syscall_edge_restores_r11_flags",
            "movq %rax, %rdi\nmovl $0xc0000080, %ecx\nrdmsr\norl $1, %eax\nwrmsr\nmovabsq $0x0018000800000000, %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nmovl $0xc0000081, %ecx\nwrmsr\nleaq 1f(%rip), %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nmovl $0xc0000082, %ecx\nwrmsr\nxorq %rax, %rax\nxorq %rdx, %rdx\nmovl $0xc0000084, %ecx\nwrmsr\nleaq 2f(%rip), %rcx\nmovq $0x243, %r11\nsysretq\n2:\npushfq\npopq %r8\nandq $0x41, %r8\ncmpq $0x41, %r8\nsete %r9b\nmovzbq %r9b, %r9\nsyscall\nmovq $0xbad, %rbx\njmp 3f\n1:\nmovq %r9, %rcx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r8, %r8\nxorq %r9, %r9\nmovq $0x6263, %rbx\nmovq $0x8888, %r8\ncmpq %r8, %r8\n3:",
        ),
        (
            "sysenter_fast_syscall_edge_clears_if_and_loads_rsp",
            "movq %rax, %rdi\nmovl $0x174, %ecx\nmovl $0x8, %eax\nxorl %edx, %edx\nwrmsr\nmovl $0x175, %ecx\nmovl $0x20000, %eax\nxorl %edx, %edx\nwrmsr\nleaq 1f(%rip), %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nmovl $0x176, %ecx\nwrmsr\npushq $0x202\npopfq\nsysenter\nmovq $0xbad, %rbx\njmp 2f\n1:\npushfq\npopq %r8\nandq $0x200, %r8\ncmpq $0, %r8\nsete %al\ncmpq $0x20000, %rsp\nsete %dl\nandb %dl, %al\nmovzbl %al, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r8, %r8\nmovq $0x7374, %rbx\nmovq $0x8888, %r8\ncmpq %r8, %r8\n2:",
        ),
        (
            "sysexit_fast_syscall_edge_rexw_loads_rsp",
            "movq %rax, %rdi\nmovl $0xc0000080, %ecx\nrdmsr\norl $1, %eax\nwrmsr\nmovabsq $0x0018000800000000, %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nmovl $0xc0000081, %ecx\nwrmsr\nleaq 1f(%rip), %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nmovl $0xc0000082, %ecx\nwrmsr\nxorq %rax, %rax\nxorq %rdx, %rdx\nmovl $0xc0000084, %ecx\nwrmsr\nmovl $0x174, %ecx\nmovl $0x8, %eax\nxorl %edx, %edx\nwrmsr\nmovabsq $0x20000, %rcx\nleaq 2f(%rip), %rdx\nsysexitq\n2:\ncmpq $0x20000, %rsp\nsete %r9b\nmovzbq %r9b, %r9\nsyscall\nmovq $0xbad, %rbx\njmp 3f\n1:\nmovq %r9, %rcx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r9, %r9\nmovq $0x8485, %rbx\nmovq $0x8888, %r8\ncmpq %r8, %r8\n3:",
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
        (
            "cpuid_cpuid_edge_leaf0_vendor_present",
            "xorl %eax, %eax\nxorl %ecx, %ecx\ncpuid\norl %ecx, %ebx\norl %edx, %ebx\nsetnz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rbx, %rbx\nxorq %rdx, %rdx\ncmpq %rax, %rax",
        ),
        (
            "cpuid_cpuid_edge_ext_leaf_zero_ext",
            "movabsq $0xffffffff80000000, %rax\nmovq $-1, %rbx\nmovq $-1, %rcx\nmovq $-1, %rdx\ncpuid\nmovq %rax, %r8\nshrq $32, %r8\nmovq %rbx, %r9\nshrq $32, %r9\norq %r9, %r8\nmovq %rcx, %r9\nshrq $32, %r9\norq %r9, %r8\nmovq %rdx, %r9\nshrq $32, %r9\norq %r9, %r8\ntestq %r8, %r8\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rbx, %rbx\nxorq %rdx, %rdx\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
        ),
        (
            "cpuid_cpuid_edge_leaf7_zero_ext",
            "movabsq $0xffffffff00000007, %rax\nmovabsq $0xffffffff00000000, %rcx\nmovq $-1, %rbx\nmovq $-1, %rdx\ncpuid\nmovq %rax, %r8\nshrq $32, %r8\nmovq %rbx, %r9\nshrq $32, %r9\norq %r9, %r8\nmovq %rcx, %r9\nshrq $32, %r9\norq %r9, %r8\nmovq %rdx, %r9\nshrq $32, %r9\norq %r9, %r8\ntestq %r8, %r8\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rbx, %rbx\nxorq %rdx, %rdx\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
        ),
        (
            "cpuid_cpuid_edge_preserves_non_query_gprs",
            "movabsq $0x1122334455667788, %r8\nmovabsq $0x8877665544332211, %r9\nxorl %eax, %eax\nxorl %ecx, %ecx\ncpuid\nxorq %rax, %rax\nxorq %rbx, %rbx\nxorq %rcx, %rcx\nxorq %rdx, %rdx\ncmpq %r8, %r8",
        ),
        (
            "cpuid_cpuid_edge_xsave_subleaf_high_ecx_ignored",
            "movl $0xd, %eax\nmovl $1, %ecx\ncpuid\nmovl %eax, %r8d\nmovl %ebx, %r9d\nmovl %ecx, %esi\nmovl %edx, %edi\nmovl $0xd, %eax\nmovabsq $0xffffffff00000001, %rcx\ncpuid\nxorl %r8d, %eax\nxorl %r9d, %ebx\nxorl %esi, %ecx\nxorl %edi, %edx\norl %ebx, %eax\norl %ecx, %eax\norl %edx, %eax\ntestl %eax, %eax\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rbx, %rbx\nxorq %rdx, %rdx\nxorq %rsi, %rsi\nxorq %rdi, %rdi\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
        ),
        (
            "cpuid_cpuid_edge_leaf1_xsave_osxsave_avx",
            "movl $1, %eax\nxorl %ecx, %ecx\ncpuid\nandl $0x1c000000, %ecx\ncmpl $0x1c000000, %ecx\nsete %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rbx, %rbx\nxorq %rdx, %rdx\ncmpq %rax, %rax",
        ),
        (
            "cpuid_cpuid_edge_leaf7_core_feature_bits",
            "movl $7, %eax\nxorl %ecx, %ecx\ncpuid\nmovl %ebx, %r8d\nandl $0x100420, %r8d\ncmpl $0x100420, %r8d\nsete %al\nmovl %ecx, %r9d\nandl $0x100, %r9d\ncmpl $0x100, %r9d\nsete %cl\nandb %cl, %al\nmovl %edx, %r9d\nandl $0x4000, %r9d\ncmpl $0x4000, %r9d\nsete %cl\nandb %cl, %al\nmovzbl %al, %ecx\nxorq %rax, %rax\nxorq %rbx, %rbx\nxorq %rdx, %rdx\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
        ),
        (
            "cpuid_cpuid_edge_xsave_leaf0_avx512_state_bits",
            "movl $0xd, %eax\nxorl %ecx, %ecx\ncpuid\nmovl %ecx, %r9d\nmovl %eax, %r8d\nandl $0xe7, %r8d\ncmpl $0xe7, %r8d\nsete %al\ntestl %ebx, %ebx\nsetnz %cl\nandb %cl, %al\ncmpl %ebx, %r9d\nsetae %cl\nandb %cl, %al\nmovzbl %al, %ecx\nxorq %rax, %rax\nxorq %rbx, %rbx\nxorq %rdx, %rdx\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
        ),
        (
            "cpuid_cpuid_edge_xsave_high_subleaf_zero",
            "movl $0xd, %eax\nmovl $63, %ecx\nmovq $-1, %rbx\nmovq $-1, %rdx\ncpuid\norl %ebx, %eax\norl %ecx, %eax\norl %edx, %eax\ntestl %eax, %eax\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rbx, %rbx\nxorq %rdx, %rdx\ncmpq %rax, %rax",
        ),
        (
            "cpuid_cpuid_edge_extended_address_widths_present",
            "movl $0x80000008, %eax\nxorl %ecx, %ecx\ncpuid\nmovl %eax, %r9d\nmovl %r9d, %r8d\nandl $0xff, %r8d\ncmpl $36, %r8d\nsetae %al\nshrl $8, %r9d\nandl $0xff, %r9d\ncmpl $48, %r9d\nsetae %cl\nandb %cl, %al\nmovzbl %al, %ecx\nxorq %rax, %rax\nxorq %rbx, %rbx\nxorq %rdx, %rdx\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
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
        (
            "rdpmc_rdpmc_edge_counter1_zero_ext",
            "movq $-1, %rax\nmovl $1, %ecx\nmovq $-1, %rdx\nrdpmc\nmovq %rax, %r8\nshrq $32, %r8\nmovq %rdx, %r9\nshrq $32, %r9\norq %r9, %r8\ntestq %r8, %r8\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
        ),
        (
            "rdpmc_rdpmc_edge_counter2_preserves_status_flags",
            "movl $2, %ecx\nmovq $0x10, %r8\nsubq $0x21, %r8\nrdpmc\nmovq $0, %rax\nmovq $0, %rdx\nmovq $0, %r8\nmovq $0, %r9",
        ),
        (
            "rdpmc_rdpmc_edge_fixed0_zero_ext",
            "movq $-1, %rax\nmovl $0x40000000, %ecx\nmovq $-1, %rdx\nrdpmc\nmovq %rax, %r8\nshrq $32, %r8\nmovq %rdx, %r9\nshrq $32, %r9\norq %r9, %r8\ntestq %r8, %r8\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
        ),
        (
            "rdpmc_rdpmc_edge_fixed1_preserves_status_flags",
            "movl $0x40000001, %ecx\nmovq $0x10, %r8\nsubq $0x21, %r8\nrdpmc\nmovq $0, %rax\nmovq $0, %rdx\nmovq $0, %r8\nmovq $0, %r9",
        ),
        (
            "rdpmc_rdpmc_edge_preserves_non_query_gprs",
            "movabsq $0x1122334455667788, %r8\nmovabsq $0x8877665544332211, %r9\nmovl $1, %ecx\nrdpmc\nxorq %rax, %rax\nxorq %rcx, %rcx\nxorq %rdx, %rdx\ncmpq %r8, %r8",
        ),
        (
            "rdpmc_rdpmc_edge_counter0_preserves_rcx",
            "xorl %ecx, %ecx\nrdpmc\nmovq %rcx, %r8\ntestq %r8, %r8\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r8, %r8\ncmpq %rax, %rax",
        ),
        (
            "rdpmc_rdpmc_edge_counter1_high_ecx_ignored",
            "movq $-1, %rax\nmovabsq $0xffffffff00000001, %rcx\nmovq $-1, %rdx\nrdpmc\nmovq %rax, %r8\nshrq $32, %r8\nmovq %rdx, %r9\nshrq $32, %r9\norq %r9, %r8\ntestq %r8, %r8\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
        ),
        (
            "rdpmc_rdpmc_edge_fixed0_high_ecx_ignored",
            "movq $-1, %rax\nmovabsq $0xffffffff40000000, %rcx\nmovq $-1, %rdx\nrdpmc\nmovq %rax, %r8\nshrq $32, %r8\nmovq %rdx, %r9\nshrq $32, %r9\norq %r9, %r8\ntestq %r8, %r8\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
        ),
        (
            "rdpmc_rdpmc_edge_fixed0_preserves_rcx",
            "movl $0x40000000, %ecx\nrdpmc\nmovq %rcx, %r8\ncmpl $0x40000000, %r8d\nsete %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r8, %r8\ncmpq %rax, %rax",
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
        (
            "x87_stack_edge_faddp_st1",
            "movabsq $0x3ff0000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\n.byte 0xde, 0xc1\nfstpl 48(%rax)\nfnstsw 56(%rax)",
        ),
        (
            "x87_stack_edge_fmulp_st1",
            "movabsq $0x4000000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4010000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\n.byte 0xde, 0xc9\nfstpl 48(%rax)\nfnstsw 56(%rax)",
        ),
        (
            "x87_stack_edge_fsubp_st1",
            "movabsq $0x4020000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\n.byte 0xde, 0xe9\nfstpl 48(%rax)\nfnstsw 56(%rax)",
        ),
        (
            "x87_stack_edge_fsubrp_st1",
            "movabsq $0x4020000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\n.byte 0xde, 0xe1\nfstpl 48(%rax)\nfnstsw 56(%rax)",
        ),
        (
            "x87_stack_edge_fdivp_st1",
            "movabsq $0x4020000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\n.byte 0xde, 0xf9\nfstpl 48(%rax)\nfnstsw 56(%rax)",
        ),
        (
            "x87_stack_edge_fdivrp_st1",
            "movabsq $0x4020000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\n.byte 0xde, 0xf1\nfstpl 48(%rax)\nfnstsw 56(%rax)",
        ),
        (
            "x87_stack_edge_fadd_st0_st1_nonpop",
            "movabsq $0x3ff0000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\n.byte 0xd8, 0xc1\nfstpl 48(%rax)\nfstpl 56(%rax)",
        ),
        (
            "x87_stack_edge_fadd_st1_st0_nonpop",
            "movabsq $0x3ff0000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\n.byte 0xdc, 0xc1\nfstpl 48(%rax)\nfstpl 56(%rax)",
        ),
        (
            "x87_stack_edge_fst_st1_no_pop",
            "movabsq $0x3ff0000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\n.byte 0xdd, 0xd1\nfstpl 48(%rax)\nfstpl 56(%rax)\nfnstsw 64(%rax)",
        ),
        (
            "x87_stack_edge_fstp_st1_pop",
            "movabsq $0x3ff0000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nmovabsq $0x4010000000000000, %r8\nmovq %r8, 48(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\nfldl 48(%rax)\n.byte 0xdd, 0xd9\nfstpl 56(%rax)\nfstpl 64(%rax)\nfnstsw 72(%rax)",
        ),
        (
            "x87_stack_edge_fld_st1_duplicate",
            "movabsq $0x3ff0000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x4000000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 32(%rax)\nfldl 40(%rax)\n.byte 0xd9, 0xc1\nfstpl 48(%rax)\nfstpl 56(%rax)\nfstpl 64(%rax)\nfnstsw 72(%rax)",
        ),
        (
            "x87_stack_edge_fucomi_unordered_flags",
            "movabsq $0x7ff8000000000000, %r8\nmovq %r8, 32(%rax)\nmovabsq $0x3ff0000000000000, %r8\nmovq %r8, 40(%rax)\nfninit\nfldl 40(%rax)\nfldl 32(%rax)\n.byte 0xdb, 0xe9\nsetb 48(%rax)\nsete 49(%rax)\nsetp 50(%rax)\nfstpl 56(%rax)\nfstpl 64(%rax)",
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

    // Core bit-test/scan width variants. These cover 16/32/64-bit register
    // forms, group-8 immediate register forms, memory bit-string address
    // adjustment, and nonzero BSF/BSR destinations with explicit sources.
    for &(label, asm) in &[
        (
            "bt_core_bit_width_r16_reg_masked",
            "movw $0x8000, %r8w\nmovw $31, %cx\nbtw %cx, %r8w",
        ),
        (
            "bts_core_bit_width_r16_reg_set",
            "movw $0, %r8w\nmovw $15, %cx\nbtsw %cx, %r8w",
        ),
        (
            "btr_core_bit_width_r16_reg_reset",
            "movw $0xffff, %r8w\nmovw $15, %cx\nbtrw %cx, %r8w",
        ),
        (
            "btc_core_bit_width_r16_reg_toggle",
            "movw $0, %r8w\nmovw $20, %cx\nbtcw %cx, %r8w",
        ),
        (
            "bt_core_bit_width_r32_reg_masked",
            "movl $0x20, %r8d\nmovl $37, %ecx\nbtl %ecx, %r8d",
        ),
        (
            "bts_core_bit_width_r32_reg_high",
            "movl $0, %r8d\nmovl $31, %ecx\nbtsl %ecx, %r8d",
        ),
        (
            "btr_core_bit_width_r64_reg_high",
            "movq $-1, %r8\nmovq $63, %rcx\nbtrq %rcx, %r8",
        ),
        (
            "btc_core_bit_width_r64_reg_masked",
            "movq $0, %r8\nmovq $70, %rcx\nbtcq %rcx, %r8",
        ),
        ("bt_core_bit_width_imm_r64_masked", "btq $70, %r8"),
        (
            "bts_core_bit_width_imm_r32_high",
            "movl $0, %r8d\nbtsl $31, %r8d",
        ),
        (
            "btr_core_bit_width_imm_r16_high",
            "movw $0xffff, %r8w\nbtrw $15, %r8w",
        ),
        (
            "btc_core_bit_width_imm_r64_toggle",
            "movq $0, %r8\nbtcq $40, %r8",
        ),
        (
            "bt_core_bit_width_m16_imm_high",
            "movw $0x8000, 40(%rax)\nbtw $15, 40(%rax)",
        ),
        (
            "bts_core_bit_width_m16_imm_set",
            "movw $0, 42(%rax)\nbtsw $7, 42(%rax)",
        ),
        (
            "btr_core_bit_width_m32_imm_reset",
            "movl $0x80000000, 44(%rax)\nbtrl $31, 44(%rax)",
        ),
        (
            "btc_core_bit_width_m64_imm_toggle",
            "movq $0, 48(%rax)\nbtcq $63, 48(%rax)",
        ),
        (
            "bt_core_bit_width_m64_r9_negative",
            "movabsq $0x8000000000000000, %r10\nmovq %r10, 48(%rax)\nmovq $-1, %r9\nbtq %r9, 56(%rax)",
        ),
        (
            "bts_core_bit_width_m64_r9_negative",
            "movq $0, 56(%rax)\nmovq $-1, %r9\nbtsq %r9, 64(%rax)",
        ),
        (
            "btr_core_bit_width_m32_rcx_positive_span",
            "movl $8, 72(%rax)\nmovl $35, %ecx\nbtrl %ecx, 68(%rax)",
        ),
        (
            "btc_core_bit_width_m16_rcx_positive_span",
            "movw $0, 82(%rax)\nmovw $19, %cx\nbtcw %cx, 80(%rax)",
        ),
        (
            "bsf_core_bit_width_r16_reg",
            "movw $0x0100, %cx\nbsfw %cx, %r8w",
        ),
        (
            "bsf_core_bit_width_r32_reg_zeroext",
            "movabsq $-1, %r8\nmovl $0x00001000, %ecx\nbsfl %ecx, %r8d",
        ),
        (
            "bsf_core_bit_width_m16",
            "movw $0x0040, 84(%rax)\nbsfw 84(%rax), %r8w",
        ),
        (
            "bsr_core_bit_width_r16_reg",
            "movw $0x8000, %cx\nbsrw %cx, %r8w",
        ),
        (
            "bsr_core_bit_width_r32_mem_zeroext",
            "movabsq $-1, %r8\nmovl $0x80000000, 88(%rax)\nbsrl 88(%rax), %r8d",
        ),
        (
            "bsr_core_bit_width_r64_mem",
            "movabsq $0x4000000000000000, %r10\nmovq %r10, 96(%rax)\nbsrq 96(%rax), %r8",
        ),
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

    // Core condition-code width variants. These set flags explicitly before
    // CMOVcc/SETcc so true and false paths exercise word/dword/qword
    // destinations, high-byte register writes, and byte memory writes.
    for &(label, asm) in &[
        (
            "cmove_core_condition_width_r16_mem_true",
            "cmpq %r8, %r8\ncmovew 4(%rax), %r8w",
        ),
        (
            "cmovne_core_condition_width_r16_mem_false",
            "cmpq %r8, %r8\ncmovnew 4(%rax), %r8w",
        ),
        (
            "cmovne_core_condition_width_r32_mem_true_zeroext",
            "movq $1, %r8\nmovq $2, %r9\ncmpq %r9, %r8\ncmovnel 8(%rax), %r8d",
        ),
        (
            "cmovne_core_condition_width_r32_mem_false_preserve",
            "movabsq $0x1234567887654321, %r8\ncmpq %r9, %r9\ncmovnel 8(%rax), %r8d",
        ),
        (
            "cmovb_core_condition_width_r64_reg_true",
            "movq $1, %r8\nmovq $2, %r9\ncmpq %r9, %r8\ncmovbq %r9, %r8",
        ),
        (
            "cmovae_core_condition_width_r64_reg_false",
            "movq $1, %r8\nmovq $2, %r9\ncmpq %r9, %r8\ncmovaeq %r9, %r8",
        ),
        (
            "cmovs_core_condition_width_r64_mem_true",
            "movq $-1, %r8\ntestq %r8, %r8\ncmovsq 16(%rax), %r8",
        ),
        (
            "cmovns_core_condition_width_r64_mem_false",
            "movq $-1, %r8\ntestq %r8, %r8\ncmovnsq 16(%rax), %r8",
        ),
        (
            "cmovg_core_condition_width_r16_reg_true",
            "movw $7, %r8w\nmovw $3, %r9w\ncmpw %r9w, %r8w\ncmovgw %r9w, %r8w",
        ),
        (
            "cmovle_core_condition_width_r16_reg_false",
            "movw $7, %r8w\nmovw $3, %r9w\ncmpw %r9w, %r8w\ncmovlew %r9w, %r8w",
        ),
        (
            "cmovl_core_condition_width_r32_reg_true",
            "movl $-5, %r8d\nmovl $7, %r9d\ncmpl %r9d, %r8d\ncmovll %r9d, %r8d",
        ),
        (
            "cmovge_core_condition_width_r32_reg_false",
            "movl $-5, %r8d\nmovl $7, %r9d\ncmpl %r9d, %r8d\ncmovgel %r9d, %r8d",
        ),
        (
            "cmovo_core_condition_width_r64_mem_true",
            "movb $0x7f, %r8b\naddb $1, %r8b\ncmovoq 24(%rax), %r8",
        ),
        (
            "cmovno_core_condition_width_r64_mem_false",
            "movb $0x7f, %r8b\naddb $1, %r8b\ncmovnoq 24(%rax), %r8",
        ),
        (
            "cmovp_core_condition_width_r32_mem_true_zeroext",
            "xorl %r8d, %r8d\ntestb %r8b, %r8b\ncmovpl 28(%rax), %r8d",
        ),
        (
            "cmovnp_core_condition_width_r32_mem_false",
            "xorl %r8d, %r8d\ntestb %r8b, %r8b\ncmovnpl 28(%rax), %r8d",
        ),
        (
            "setz_core_condition_width_r8_true",
            "cmpq %r8, %r8\nsetz %r8b",
        ),
        (
            "setnz_core_condition_width_r8_false",
            "cmpq %r8, %r8\nsetnz %r8b",
        ),
        ("setc_core_condition_width_ah_true", "stc\nsetc %ah"),
        ("setnc_core_condition_width_ah_false", "stc\nsetnc %ah"),
        (
            "seto_core_condition_width_r9b_true",
            "movb $0x7f, %r8b\naddb $1, %r8b\nseto %r9b",
        ),
        (
            "setno_core_condition_width_r9b_false",
            "movb $0x7f, %r8b\naddb $1, %r8b\nsetno %r9b",
        ),
        (
            "sets_core_condition_width_m8_true",
            "movl $-1, %r8d\ntestl %r8d, %r8d\nsets 48(%rax)",
        ),
        (
            "setns_core_condition_width_m8_false",
            "movl $-1, %r8d\ntestl %r8d, %r8d\nsetns 49(%rax)",
        ),
        (
            "setp_core_condition_width_m8_true",
            "xorl %r8d, %r8d\ntestb %r8b, %r8b\nsetp 50(%rax)",
        ),
        (
            "setnp_core_condition_width_m8_false",
            "xorl %r8d, %r8d\ntestb %r8b, %r8b\nsetnp 51(%rax)",
        ),
        (
            "setg_core_condition_width_r9b_true",
            "movl $5, %r8d\nmovl $3, %r9d\ncmpl %r9d, %r8d\nsetg %r9b",
        ),
        (
            "setle_core_condition_width_r9b_false",
            "movl $5, %r8d\nmovl $3, %r9d\ncmpl %r9d, %r8d\nsetle %r9b",
        ),
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

    // IRETQ is a control transfer that consumes a privileged return frame. The
    // snippets build a scratch-local GDT and frame, return to a local label, and
    // then restore RSP plus scratch/stack bytes before comparison.
    let iretq_setup = "movq $0, 128(%rax)\nmovabsq $0x00209a0000000000, %r8\nmovq %r8, 136(%rax)\nmovabsq $0x0000920000000000, %r8\nmovq %r8, 144(%rax)\nmovw $0x0017, 32(%rax)\nleaq 128(%rax), %r8\nmovq %r8, 34(%rax)\nlgdt 32(%rax)";
    let iretq_clear = "movq $0, 128(%rax)\nmovq $0, 136(%rax)\nmovq $0, 144(%rax)\nmovq $0, 176(%rax)\nmovq $0, 184(%rax)\nmovq $0, 192(%rax)\nmovq $0, 200(%rax)\nmovq $0, 208(%rax)";
    for &(label, flags, body) in &[
        (
            "iretq_core_control_iret_edge_target_roundtrip",
            "0x202",
            "1:\nmovq $0x2222, %r8\nmovabsq $0x20000, %rsp",
        ),
        (
            "iretq_core_control_iret_edge_restores_status_flags",
            "0x247",
            "1:\nmovabsq $0x20000, %rsp\npushfq\npopq %r9\nmovq $0, -8(%rsp)\nandq $0x45, %r9\ncmpq $0x45, %r9\nsete %cl\nmovzbl %cl, %ecx\nxorq %r9, %r9",
        ),
        (
            "iretq_core_control_iret_edge_restored_zf_overrides_stale_flags",
            "0x242",
            "cmpq %r8, %r9\n1:\nsetz %cl\nmovzbl %cl, %ecx\nmovabsq $0x20000, %rsp",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: format!(
                "{iretq_setup}\nleaq 1f(%rip), %r9\nmovq %r9, 176(%rax)\nmovq $0x8, 184(%rax)\nmovq ${flags}, 192(%rax)\nmovabsq $0x20000, %r9\nmovq %r9, 200(%rax)\nmovq $0x10, 208(%rax)\nleaq 176(%rax), %rsp\niretq\nmovq $0xbad, %r8\njmp 2f\n{body}\n{iretq_clear}\n2:\ncmpq %r8, %r8"
            ),
            feat: Core,
            profile: Int,
        });
    }

    // Far returns pop RIP:CS and, for the immediate form, additionally adjust
    // RSP. The snippets build a scratch-local same-ring frame, execute LRETQ,
    // then restore RSP and clear scratch before comparison.
    let lretq_setup = "movq $0, 128(%rax)\nmovabsq $0x00209a0000000000, %r8\nmovq %r8, 136(%rax)\nmovw $0x000f, 32(%rax)\nleaq 128(%rax), %r8\nmovq %r8, 34(%rax)\nlgdt 32(%rax)";
    let lretq_clear = "movq $0, 128(%rax)\nmovq $0, 136(%rax)\nmovq $0, 176(%rax)\nmovq $0, 184(%rax)\nmovq $0, 192(%rax)\nmovq $0, 200(%rax)";
    for &(label, op, body) in &[
        (
            "lretq_core_far_return_edge_target_roundtrip",
            "lretq",
            "1:\nmovq $0x2222, %r8\nmovabsq $0x20000, %rsp",
        ),
        (
            "lretq_core_far_return_edge_preserves_status_flags",
            "lretq",
            "cmpq %r8, %r8\n1:\nsetz %cl\nmovzbl %cl, %ecx\nxorq %r8, %r8\nmovabsq $0x20000, %rsp",
        ),
        (
            "lretq_core_far_return_edge_imm16_stack_adjust",
            "lretq $16",
            "1:\nleaq 208(%rax), %r9\ncmpq %r9, %rsp\nsete %cl\nmovzbl %cl, %ecx\nxorq %r9, %r9\nmovabsq $0x20000, %rsp",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: format!(
                "{lretq_setup}\nleaq 1f(%rip), %r9\nmovq %r9, 176(%rax)\nmovq $0x8, 184(%rax)\nmovq $0x1111, 192(%rax)\nmovq $0x2222, 200(%rax)\nleaq 176(%rax), %rsp\n{op}\nmovq $0xbad, %r8\njmp 2f\n{body}\n{lretq_clear}\n2:\ncmpq %r8, %r8"
            ),
            feat: Core,
            profile: Int,
        });
    }

    // Far JMP/CALL reload CS from a descriptor and mark that code descriptor
    // accessed. The scratch GDT is intentionally left visible in the final
    // memory image so the descriptor access-bit write is part of the diff.
    let far_control_setup = "movq $0, 128(%rax)\nmovabsq $0x00209a0000000000, %r10\nmovq %r10, 136(%rax)\nmovw $0x000f, 32(%rax)\nleaq 128(%rax), %r10\nmovq %r10, 34(%rax)\nlgdt 32(%rax)";
    for &(label, asm) in &[
        (
            "ljmpq_core_far_control_edge_target_roundtrip",
            "leaq 1f(%rip), %r10\nmovq %r10, 176(%rax)\nmovq $0x8, 184(%rax)\nljmpq *176(%rax)\nmovq $0xbad, %r8\njmp 2f\n1:\nmovq $0x2222, %r8\n2:\ncmpq %r8, %r8",
        ),
        (
            "ljmpq_core_far_control_edge_preserves_status_flags",
            "cmpq %r8, %r8\nleaq 1f(%rip), %r10\nmovq %r10, 176(%rax)\nmovq $0x8, 184(%rax)\nljmpq *176(%rax)\nmovq $0xbad, %r8\njmp 2f\n1:\nsetz %cl\nmovzbl %cl, %ecx\n2:\ncmpq %r8, %r8",
        ),
        (
            "lcallq_core_far_control_edge_roundtrip",
            "leaq 1f(%rip), %r10\nmovq %r10, 176(%rax)\nmovq $0x8, 184(%rax)\nlcallq *176(%rax)\nmovq $0, -16(%rsp)\nmovq $0, -8(%rsp)\nmovq $0x2222, %r8\njmp 2f\n1:\nmovq $0x3333, %r9\nlretq\n2:\ncmpq %r8, %r8",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: format!("{far_control_setup}\n{asm}"),
            feat: Core,
            profile: Int,
        });
    }

    // Core near control-transfer and group-5 stack/addressing forms not covered
    // by the direct rel8 starter cases above. These include forced rel32
    // branches, indirect CALL/JMP through registers and memory, RET imm16 stack
    // adjustment, LOOPcc/J*CXZ condition variants, and PUSH r/m memory forms.
    for &(label, asm) in &[
        (
            "jmp_core_control_rel32_taken",
            "jmp 1f\n.space 140, 0x90\n1:\nmovq $0x2222, %r8",
        ),
        (
            "je_core_control_rel32_taken",
            "cmpq %r8, %r8\nje 1f\nmovq $0x1111, %r8\njmp 2f\n.space 140, 0x90\n1:\nmovq $0x2222, %r8\n2:",
        ),
        (
            "jne_core_control_rel32_not_taken",
            "cmpq %r8, %r8\njne 1f\nmovq $0x2222, %r8\njmp 2f\n.space 140, 0x90\n1:\nmovq $0x1111, %r8\n2:",
        ),
        (
            "jmp_core_control_indirect_reg",
            "leaq 1f(%rip), %r8\njmp *%r8\nmovq $0x1111, %r9\n1:\nxorq %r8, %r8\nmovq $0x2222, %r9",
        ),
        (
            "jmp_core_control_indirect_mem",
            "leaq 1f(%rip), %r8\nmovq %r8, 120(%rax)\njmp *120(%rax)\nmovq $0x1111, %r9\n1:\nxorq %r8, %r8\nmovq $0, 120(%rax)\nmovq $0x2222, %r9",
        ),
        (
            "call_core_control_indirect_reg_ret",
            "leaq 1f(%rip), %r8\ncall *%r8\nmovq $0, -8(%rsp)\nmovq $0x3333, %r8\njmp 2f\n1:\nmovq $0x2222, %r9\nretq\n2:",
        ),
        (
            "call_core_control_indirect_mem_ret",
            "leaq 1f(%rip), %r8\nmovq %r8, 128(%rax)\ncall *128(%rax)\nmovq $0, -8(%rsp)\nmovq $0, 128(%rax)\nmovq $0x3333, %r8\njmp 2f\n1:\nmovq $0x2222, %r9\nretq\n2:",
        ),
        (
            "ret_core_control_imm16_stack_adjust",
            "pushq $0x1122\ncall 1f\nmovq $0, -16(%rsp)\nmovq $0, -8(%rsp)\nmovq $0x3333, %r8\njmp 2f\n1:\nmovq $0x2222, %r9\nretq $8\n2:",
        ),
        ("push_core_control_m64_pop", "pushq 32(%rax)\npopq %r8"),
        ("push_core_control_m16_pop", "pushw 34(%rax)\npopw %r8w"),
        (
            "loop_core_control_rel8_not_taken",
            "movl $1, %ecx\nloop 1f\nmovq $0x2222, %r8\njmp 2f\n1:\nmovq $0x1111, %r8\n2:",
        ),
        (
            "loopne_core_control_rel8_taken",
            "movl $2, %ecx\ncmpq %r8, %r9\nloopne 1f\nmovq $0x1111, %r8\njmp 2f\n1:\nmovq $0x2222, %r8\n2:",
        ),
        (
            "loopne_core_control_rel8_not_taken_zf",
            "movl $2, %ecx\ncmpq %r8, %r8\nloopne 1f\nmovq $0x2222, %r8\njmp 2f\n1:\nmovq $0x1111, %r8\n2:",
        ),
        (
            "loope_core_control_rel8_taken",
            "movl $2, %ecx\ncmpq %r8, %r8\nloope 1f\nmovq $0x1111, %r8\njmp 2f\n1:\nmovq $0x2222, %r8\n2:",
        ),
        (
            "loope_core_control_rel8_not_taken_zf_clear",
            "movl $2, %ecx\ncmpq %r8, %r9\nloope 1f\nmovq $0x2222, %r8\njmp 2f\n1:\nmovq $0x1111, %r8\n2:",
        ),
        (
            "jecxz_core_control_rel8_taken",
            "xorl %ecx, %ecx\njecxz 1f\nmovq $0x1111, %r8\njmp 2f\n1:\nmovq $0x2222, %r8\n2:",
        ),
        (
            "jrcxz_core_control_rel8_taken",
            "xorq %rcx, %rcx\njrcxz 1f\nmovq $0x1111, %r8\njmp 2f\n1:\nmovq $0x2222, %r8\n2:",
        ),
        (
            "addr32_jrcxz_core_control_ecx_taken",
            "movabsq $0x0000000100000000, %rcx\naddr32 jrcxz 1f\nmovq $0x1111, %r8\njmp 2f\n1:\nmovq $0x2222, %r8\n2:",
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
            "fxsave_fxsave_edge_legacy32_mxcsr_and_mask",
            "movl $0x5f80, 32(%rax)\nldmxcsr 32(%rax)\nfxsave 256(%rax)\nmovl 280(%rax), %r8d\nmovl %r8d, 36(%rax)\nmovl 284(%rax), %r8d\nmovl %r8d, 40(%rax)",
            Fxsave,
            Int,
        ),
        (
            "fxrstor_fxsave_edge_legacy32_mxcsr_roundtrip",
            "movl $0x3f80, 32(%rax)\nldmxcsr 32(%rax)\nfxsave 256(%rax)\nmovl $0x1f80, 36(%rax)\nldmxcsr 36(%rax)\nfxrstor 256(%rax)\nstmxcsr 40(%rax)",
            Fxsave,
            Int,
        ),
        (
            "fxsave64_fxsave_edge_x87_header_fields",
            "fninit\nfld1\nfldl 32(%rax)\nfxsave64 256(%rax)\nmovw 256(%rax), %r8w\nmovw %r8w, 64(%rax)\nmovw 258(%rax), %r8w\nmovw %r8w, 66(%rax)\nmovw 260(%rax), %r8w\nmovw %r8w, 68(%rax)",
            Fxsave,
            F64,
        ),
        (
            "fxrstor64_fxsave_edge_x87_two_deep_order",
            "fninit\nfld1\nfldl 32(%rax)\nfxsave64 256(%rax)\nfninit\nfldz\nfxrstor64 256(%rax)\nfstpl 64(%rax)\nfstpl 72(%rax)",
            Fxsave,
            F64,
        ),
        (
            "fxrstor64_fxsave_edge_xmm0_roundtrip",
            "movdqu 32(%rax), %xmm0\nfxsave64 256(%rax)\npxor %xmm0, %xmm0\nfxrstor64 256(%rax)\nmovdqu %xmm0, 64(%rax)",
            Fxsave,
            Int,
        ),
        (
            "fxrstor64_fxsave_edge_xmm15_roundtrip",
            "movdqu 32(%rax), %xmm15\nfxsave64 256(%rax)\npxor %xmm15, %xmm15\nfxrstor64 256(%rax)\nmovdqu %xmm15, 64(%rax)",
            Fxsave,
            Int,
        ),
        (
            "fxsave64_fxsave_edge_mxcsr_round_down",
            "movl $0x7f80, 32(%rax)\nldmxcsr 32(%rax)\nfxsave64 256(%rax)\nmovl 280(%rax), %r8d\nmovl %r8d, 36(%rax)",
            Fxsave,
            Int,
        ),
        (
            "fxrstor64_fxsave_edge_mxcsr_round_zero",
            "movl $0x5f80, 32(%rax)\nldmxcsr 32(%rax)\nfxsave64 256(%rax)\nmovl $0x1f80, 36(%rax)\nldmxcsr 36(%rax)\nfxrstor64 256(%rax)\nstmxcsr 40(%rax)",
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
            "xsetbv_xsave_edge_high_halves_ignored",
            "movabsq $0xffffffff000000e7, %rax\nmovabsq $0xffffffff00000000, %rdx\nxorq %rcx, %rcx\nxsetbv\nmovq $-1, %rax\nmovq $-1, %rdx\nxgetbv",
            Xsave,
            Int,
        ),
        (
            "xsetbv_xsave_edge_avx_only_roundtrip",
            "movl $0x7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nxgetbv\nmovl %eax, 48(%rbx)\nmovl %edx, 52(%rbx)\nmovl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nxgetbv",
            Xsave,
            Int,
        ),
        (
            "xgetbv1_xgetbv1_edge_enabled_state_zero_ext",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nmovq $-1, %rax\nmovl $1, %ecx\nmovq $-1, %rdx\nxgetbv",
            Xgetbv1,
            Int,
        ),
        (
            "xgetbv1_xgetbv1_edge_high_rcx_ignored",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nmovq $-1, %rax\nmovabsq $0xffffffff00000001, %rcx\nmovq $-1, %rdx\nxgetbv",
            Xgetbv1,
            Int,
        ),
        (
            "xgetbv1_xgetbv1_edge_flags_preserved",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\ncmpq %r8, %r8\nmovl $1, %ecx\nxgetbv",
            Xgetbv1,
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
        (
            "xsave64_xsave_edge_opmask_roundtrip",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nxsave64 240(%rbx)\nkxorq %k1, %k1, %k1\nmovl $0xe7, %eax\nxorl %edx, %edx\nxrstor64 240(%rbx)",
            Xsave,
            Int,
        ),
        (
            "xsave64_xsave_edge_zmm16_roundtrip",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nxsave64 240(%rbx)\nvpxord %zmm16, %zmm16, %zmm16\nmovl $0xe7, %eax\nxorl %edx, %edx\nxrstor64 240(%rbx)",
            Xsave,
            Int,
        ),
        (
            "xsave64_xsave_edge_ymm2_roundtrip",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nxsave64 240(%rbx)\nvpxord %ymm2, %ymm2, %ymm2\nmovl $0xe7, %eax\nxorl %edx, %edx\nxrstor64 240(%rbx)",
            Xsave,
            Int,
        ),
        (
            "xrstor64_xsave_edge_sse_only_mask",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nxsave64 240(%rbx)\nvpxord %zmm1, %zmm1, %zmm1\nmovl $0x3, %eax\nxorl %edx, %edx\nxrstor64 240(%rbx)",
            Xsave,
            Int,
        ),
        (
            "xrstor64_xsave_edge_missing_opmask_init",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nmovl $0x7, %eax\nxorl %edx, %edx\nxsave64 240(%rbx)\nmovl $0xe7, %eax\nxorl %edx, %edx\nxrstor64 240(%rbx)",
            Xsave,
            Int,
        ),
        (
            "xsaveopt64_xrstor64_zmm_roundtrip",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nxsaveopt64 240(%rbx)\nvpxord %zmm1, %zmm1, %zmm1\nmovl $0xe7, %eax\nxorl %edx, %edx\nxrstor64 240(%rbx)",
            XsaveExt,
            Int,
        ),
        (
            "xsaveopt64_xrstor64_mxcsr_roundtrip",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nmovl $0x5f80, 48(%rbx)\nldmxcsr 48(%rbx)\nmovl $0xe7, %eax\nxorl %edx, %edx\nxsaveopt64 240(%rbx)\nmovl $0x1f80, 52(%rbx)\nldmxcsr 52(%rbx)\nmovl $0xe7, %eax\nxorl %edx, %edx\nxrstor64 240(%rbx)\nstmxcsr 56(%rbx)",
            XsaveExt,
            Int,
        ),
        (
            "xsavec64_xrstors64_zmm_roundtrip",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nxsavec64 240(%rbx)\nvpxord %zmm1, %zmm1, %zmm1\nmovl $0xe7, %eax\nxorl %edx, %edx\nxrstors64 240(%rbx)",
            XsaveExt,
            Int,
        ),
        (
            "xsavec64_xrstors64_xmm_roundtrip",
            "movdqu 32(%rbx), %xmm2\nmovl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nxsavec64 240(%rbx)\npxor %xmm2, %xmm2\nmovl $0xe7, %eax\nxorl %edx, %edx\nxrstors64 240(%rbx)\nmovdqu %xmm2, 64(%rbx)",
            XsaveExt,
            Int,
        ),
        (
            "xsaves64_xrstors64_mxcsr_roundtrip",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nmovl $0x5f80, 48(%rbx)\nldmxcsr 48(%rbx)\nmovl $0xe7, %eax\nxorl %edx, %edx\nxsaves64 240(%rbx)\nmovl $0x1f80, 52(%rbx)\nldmxcsr 52(%rbx)\nmovl $0xe7, %eax\nxorl %edx, %edx\nxrstors64 240(%rbx)\nstmxcsr 56(%rbx)",
            XsaveExt,
            Int,
        ),
        (
            "xsaveopt64_xsave_edge_opmask_roundtrip",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nxsaveopt64 240(%rbx)\nkxorq %k2, %k2, %k2\nmovl $0xe7, %eax\nxorl %edx, %edx\nxrstor64 240(%rbx)",
            XsaveExt,
            Int,
        ),
        (
            "xsavec64_xsave_edge_opmask_roundtrip",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nxsavec64 240(%rbx)\nkxorq %k3, %k3, %k3\nmovl $0xe7, %eax\nxorl %edx, %edx\nxrstors64 240(%rbx)",
            XsaveExt,
            Int,
        ),
        (
            "xsaves64_xsave_edge_zmm16_roundtrip",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nxsaves64 240(%rbx)\nvpxord %zmm16, %zmm16, %zmm16\nmovl $0xe7, %eax\nxorl %edx, %edx\nxrstors64 240(%rbx)",
            XsaveExt,
            Int,
        ),
        (
            "xsavec64_xsave_edge_missing_opmask_init",
            "movl $0xe7, %eax\nxorl %edx, %edx\nxorl %ecx, %ecx\nxsetbv\nmovl $0x7, %eax\nxorl %edx, %edx\nxsavec64 240(%rbx)\nmovl $0xe7, %eax\nxorl %edx, %edx\nxrstors64 240(%rbx)",
            XsaveExt,
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
    for &(label, asm, feat) in &[
        ("lfence_preserves_cmp_flags", "cmpq %rcx, %r8\nlfence", Fence),
        ("mfence_preserves_cmp_flags", "cmpq %rcx, %r8\nmfence", Fence),
        ("sfence_preserves_cmp_flags", "cmpq %rcx, %r8\nsfence", Fence),
        (
            "clflush_cache_line_disp",
            "movq %r8, -16(%rbx)\nclflush -16(%rbx)\nmovq -16(%rbx), %rcx",
            Clflush,
        ),
        (
            "clflush_cache_line_addr32_disp",
            "movq %r8, -16(%rbx)\naddr32 clflush -16(%rbx)\nmovq -16(%rbx), %rcx",
            Clflush,
        ),
        (
            "clflushopt_cache_line_disp",
            "movq %r8, -16(%rbx)\nclflushopt -16(%rbx)\nsfence\nmovq -16(%rbx), %rcx",
            Clflushopt,
        ),
        (
            "clflushopt_cache_line_addr32_disp",
            "movq %r8, -16(%rbx)\naddr32 clflushopt -16(%rbx)\nsfence\nmovq -16(%rbx), %rcx",
            Clflushopt,
        ),
        (
            "clwb_cache_line_disp",
            "movq %r8, -16(%rbx)\nclwb -16(%rbx)\nsfence\nmovq -16(%rbx), %rcx",
            Clwb,
        ),
        (
            "clwb_cache_line_addr32_disp",
            "movq %r8, -16(%rbx)\naddr32 clwb -16(%rbx)\nsfence\nmovq -16(%rbx), %rcx",
            Clwb,
        ),
        (
            "cldemote_cache_line_disp",
            "movq %r8, -16(%rbx)\ncldemote -16(%rbx)\nmovq -16(%rbx), %rcx",
            Cldemote,
        ),
        (
            "cldemote_cache_line_addr32_disp",
            "movq %r8, -16(%rbx)\naddr32 cldemote -16(%rbx)\nmovq -16(%rbx), %rcx",
            Cldemote,
        ),
        (
            "invd_preserves_cmp_flags",
            "cmpq %rcx, %r8\ninvd",
            CacheInvd,
        ),
        (
            "wbinvd_preserves_cmp_flags",
            "cmpq %rcx, %r8\nwbinvd",
            CacheInvd,
        ),
        (
            "wbnoinvd_preserves_cmp_flags",
            "cmpq %rcx, %r8\nwbnoinvd",
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
    for &(label, asm, feat) in &[
        (
            "lfence_cache_tlb_edge_load_store",
            "movq 32(%rax), %r8\nlfence\nmovq %r8, 88(%rax)",
            Fence,
        ),
        (
            "mfence_cache_tlb_edge_two_stores",
            "movq %r8, 32(%rax)\nmovq %rcx, 40(%rax)\nmfence\nmovq 32(%rax), %r9\nmovq 40(%rax), %rcx",
            Fence,
        ),
        (
            "sfence_cache_tlb_edge_store_preserves_flags",
            "movq %r8, 48(%rax)\ncmpq %rcx, %r8\nsfence",
            Fence,
        ),
        (
            "clflush_cache_tlb_edge_unaligned",
            "movq %r8, 33(%rax)\nclflush 33(%rax)\nmovq 33(%rax), %rcx",
            Clflush,
        ),
        (
            "clflush_cache_tlb_edge_r8_base",
            "leaq 64(%rax), %r8\nmovq %rcx, (%r8)\nclflush (%r8)\nmovq (%r8), %r9",
            Clflush,
        ),
        (
            "clflush_cache_tlb_edge_sib_zero_index",
            "leaq 96(%rax), %r8\nxorq %r9, %r9\nmovq %rcx, (%r8,%r9,1)\nclflush (%r8,%r9,1)\nmovq (%r8,%r9,1), %rcx",
            Clflush,
        ),
        (
            "clflushopt_cache_tlb_edge_unaligned",
            "movq %r8, 41(%rax)\nclflushopt 41(%rax)\nsfence\nmovq 41(%rax), %rcx",
            Clflushopt,
        ),
        (
            "clflushopt_cache_tlb_edge_r8_base",
            "leaq 72(%rax), %r8\nmovq %rcx, (%r8)\nclflushopt (%r8)\nsfence\nmovq (%r8), %r9",
            Clflushopt,
        ),
        (
            "clflushopt_cache_tlb_edge_sib_zero_index",
            "leaq 104(%rax), %r8\nxorq %r9, %r9\nmovq %rcx, (%r8,%r9,1)\nclflushopt (%r8,%r9,1)\nsfence\nmovq (%r8,%r9,1), %rcx",
            Clflushopt,
        ),
        (
            "clwb_cache_tlb_edge_unaligned",
            "movq %r8, 49(%rax)\nclwb 49(%rax)\nsfence\nmovq 49(%rax), %rcx",
            Clwb,
        ),
        (
            "clwb_cache_tlb_edge_r8_base",
            "leaq 80(%rax), %r8\nmovq %rcx, (%r8)\nclwb (%r8)\nsfence\nmovq (%r8), %r9",
            Clwb,
        ),
        (
            "clwb_cache_tlb_edge_sib_zero_index",
            "leaq 112(%rax), %r8\nxorq %r9, %r9\nmovq %rcx, (%r8,%r9,1)\nclwb (%r8,%r9,1)\nsfence\nmovq (%r8,%r9,1), %rcx",
            Clwb,
        ),
        (
            "cldemote_cache_tlb_edge_unaligned",
            "movq %r8, 57(%rax)\ncldemote 57(%rax)\nmovq 57(%rax), %rcx",
            Cldemote,
        ),
        (
            "cldemote_cache_tlb_edge_r8_base",
            "leaq 120(%rax), %r8\nmovq %rcx, (%r8)\ncldemote (%r8)\nmovq (%r8), %r9",
            Cldemote,
        ),
        (
            "cldemote_cache_tlb_edge_sib_zero_index",
            "leaq 128(%rax), %r8\nxorq %r9, %r9\nmovq %rcx, (%r8,%r9,1)\ncldemote (%r8,%r9,1)\nmovq (%r8,%r9,1), %rcx",
            Cldemote,
        ),
        (
            "invd_cache_tlb_edge_after_load",
            "movq 32(%rax), %r8\ncmpq %rcx, %r8\ninvd\nmovq %r8, %rcx",
            CacheInvd,
        ),
        (
            "wbinvd_cache_tlb_edge_two_stores",
            "movq %r8, 32(%rax)\nmovq %rcx, 40(%rax)\nwbinvd\nmovq 32(%rax), %r9\nmovq 40(%rax), %rcx",
            CacheInvd,
        ),
        (
            "wbnoinvd_cache_tlb_edge_after_load",
            "movq 48(%rax), %r8\ncmpq %rcx, %r8\nwbnoinvd\nmovq %r8, %rcx",
            Wbnoinvd,
        ),
        (
            "wbnoinvd_cache_tlb_edge_two_stores",
            "movq %r8, 56(%rax)\nmovq %rcx, 64(%rax)\nwbnoinvd\nmovq 56(%rax), %r9\nmovq 64(%rax), %rcx",
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
            "nop_rm_negative_sib",
            "movl $2, %ecx\nnopl -16(%rbx,%rcx,2)",
            HintNop,
        ),
        (
            "nop_rm_addr32_disp",
            "addr32 nopl 48(%eax)",
            HintNop,
        ),
        ("endbr64_hint_nop", "endbr64", HintNop),
        ("endbr32_hint_nop", "endbr32", HintNop),
        (
            "endbr64_hint_preserves_cmp_flags",
            "cmpq %rcx, %r8\nendbr64",
            HintNop,
        ),
        (
            "endbr32_hint_between_memory_ops",
            "movq %r8, 88(%rax)\nendbr32\nmovq 88(%rax), %rcx",
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
        (
            "prefetchnta_prefetch_edge_rex_r8_base",
            "movq %rax, %r8\nprefetchnta (%r8)",
            HintNop,
        ),
        (
            "prefetcht0_prefetch_edge_negative_disp",
            "leaq 224(%rax), %rbp\nprefetcht0 -16(%rbp)",
            HintNop,
        ),
        (
            "addr32_prefetcht1_prefetch_edge_high_rax",
            "movabsq $0xffff000000004000, %rax\naddr32 prefetcht1 (%eax)",
            HintNop,
        ),
        (
            "prefetcht2_prefetch_edge_stack_base",
            "prefetcht2 (%rsp)",
            HintNop,
        ),
        (
            "prefetcht0_prefetch_edge_preserves_cmp_flags",
            "cmpq %rcx, %r8\nprefetcht0 200(%rax)",
            HintNop,
        ),
        (
            "prefetchw_prefetch_edge_rex_r8_base",
            "movq %rax, %r8\nprefetchw (%r8)",
            Prefetchw,
        ),
        (
            "addr32_prefetchw_prefetch_edge_high_rax",
            "movabsq $0xffff000000004000, %rax\naddr32 prefetchw (%eax)",
            Prefetchw,
        ),
        (
            "prefetchwt1_prefetch_edge_memory",
            "prefetchwt1 216(%rax)",
            Prefetchw,
        ),
        (
            "prefetchwt1_prefetch_edge_preserves_cmp_flags",
            "cmpq %rcx, %r8\nprefetchwt1 224(%rax)",
            Prefetchw,
        ),
        (
            "prefetchwt1_prefetch_edge_sib_zero_index",
            "xorl %ecx, %ecx\nprefetchwt1 232(%rax,%rcx,1)",
            Prefetchw,
        ),
        (
            "prefetchwt1_prefetch_edge_rex_r8_base",
            "movq %rax, %r8\nprefetchwt1 (%r8)",
            Prefetchw,
        ),
        (
            "addr32_prefetchwt1_prefetch_edge_high_rax",
            "movabsq $0xffff000000004000, %rax\naddr32 prefetchwt1 (%eax)",
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
    for &(label, asm, feat) in &[
        (
            "monitor_wait_edge_unaligned_address",
            "leaq 33(%rax), %rax\nxorl %ecx, %ecx\nxorl %edx, %edx\nmonitor\nmovq %rax, %rcx",
            Monitor,
        ),
        (
            "monitor_wait_edge_stack_address",
            "movq %rsp, %rax\nxorl %ecx, %ecx\nxorl %edx, %edx\nmonitor\nmovq %rax, %rcx",
            Monitor,
        ),
        (
            "monitor_wait_edge_addr32_high_rax",
            "movabsq $0xffff000000004040, %rax\nxorl %ecx, %ecx\nxorl %edx, %edx\naddr32 monitor\nmovq %rax, %rcx",
            Monitor,
        ),
        (
            "monitor_wait_edge_preserves_cmp_flags",
            "leaq 96(%rax), %rax\nxorl %ecx, %ecx\nxorl %edx, %edx\ncmpq %r8, %r9\nmonitor",
            Monitor,
        ),
        (
            "umonitor_wait_edge_unaligned_r8",
            "leaq 33(%rax), %r8\numonitor %r8\nmovq %r8, %rcx",
            Waitpkg,
        ),
        (
            "umonitor_wait_edge_stack_r10",
            "leaq 16(%rsp), %r10\numonitor %r10\nmovq %r10, %rcx",
            Waitpkg,
        ),
        (
            "umonitor_wait_edge_r11d_zeroext_address",
            "movabsq $0xffff000000004080, %r11\numonitor %r11d\nmovq %r11, %rcx",
            Waitpkg,
        ),
        (
            "umwait_wait_edge_control1_zero_deadline",
            "xorl %edx, %edx\nxorl %eax, %eax\nmovl $1, %ecx\numwait %ecx\ncmpq %r8, %r8",
            Waitpkg,
        ),
        (
            "tpause_wait_edge_control1_zero_deadline",
            "xorl %edx, %edx\nxorl %eax, %eax\nmovl $1, %ecx\ntpause %ecx\ncmpq %r8, %r8",
            Waitpkg,
        ),
        (
            "umwait_wait_edge_r10d_control1",
            "xorl %edx, %edx\nxorl %eax, %eax\nmovl $1, %r10d\numwait %r10d\ncmpq %r8, %r8",
            Waitpkg,
        ),
        (
            "tpause_wait_edge_r10d_control1",
            "xorl %edx, %edx\nxorl %eax, %eax\nmovl $1, %r10d\ntpause %r10d\ncmpq %r8, %r8",
            Waitpkg,
        ),
        (
            "umonitor_umwait_wait_edge_unaligned",
            "leaq 33(%rax), %r8\numonitor %r8\nxorl %edx, %edx\nxorl %eax, %eax\nxorl %ecx, %ecx\numwait %ecx\nmovq %r8, %rcx\ncmpq %rcx, %rcx",
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
    for &(label, asm, feat) in &[
        (
            "serialize_stack_roundtrip",
            "pushq %r8\nserialize\npopq %rcx",
            Serialize,
        ),
        (
            "serialize_after_lfence",
            "movq %r8, 96(%rax)\nlfence\nserialize\nmovq 96(%rax), %rcx",
            Serialize,
        ),
        (
            "umonitor_r32_address",
            "leaq 96(%rax), %r8\numonitor %r8d\nmovq %r8, %rcx",
            Waitpkg,
        ),
        (
            "umonitor_r10_address",
            "leaq 112(%rax), %r10\numonitor %r10\nmovq %r10, %rcx",
            Waitpkg,
        ),
        (
            "umonitor_r10d_address",
            "leaq 120(%rax), %r10\numonitor %r10d\nmovq %r10, %rcx",
            Waitpkg,
        ),
        (
            "umwait_r10d_zero_deadline",
            "xorl %edx, %edx\nxorl %eax, %eax\nxorl %r10d, %r10d\numwait %r10d\naddq $0, %r8",
            Waitpkg,
        ),
        (
            "tpause_r10d_zero_deadline",
            "xorl %edx, %edx\nxorl %eax, %eax\nxorl %r10d, %r10d\ntpause %r10d\naddq $0, %r8",
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
    for &(label, asm) in &[
        ("rdpid_rbx_zeroext", "movabsq $-1, %rbx\nrdpid %rbx"),
        (
            "rdpid_r10_to_rcx_zeroext",
            "movabsq $-1, %r10\nrdpid %r10\nmovq %r10, %rcx",
        ),
        (
            "rdpid_rdpid_edge_r15_zeroext",
            "movabsq $-1, %r15\nrdpid %r15",
        ),
        (
            "rdpid_rdpid_edge_rexw_r15",
            "movabsq $-1, %r15\n.byte 0xf3, 0x49, 0x0f, 0xc7, 0xff",
        ),
        (
            "rdpid_rdpid_edge_r15_preserves_cmp_flags",
            "cmpq %r8, %r8\nrdpid %r15",
        ),
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
    for &(label, asm, feat) in &[
        (
            "rdrand_r64_rbx_success_flags",
            "1:\nrdrand %rbx\njnc 1b\nmovq $0, %rbx",
            Rdrand,
        ),
        (
            "rdrand_r16_cx_preserves_upper",
            "movabsq $0x1020304050607080, %rcx\n1:\nrdrand %cx\njnc 1b\nmovw $0, %cx",
            Rdrand,
        ),
        (
            "rdrand_r32_r8d_zeroext",
            "movabsq $-1, %r8\n1:\nrdrand %r8d\njnc 1b\nmovabsq $0xffffffff00000000, %r10\ntestq %r10, %r8\njz 2f\nmovq $1, %r8\njmp 3f\n2:\nmovq $0, %r8\n3:\naddq $0, %r8",
            Rdrand,
        ),
        (
            "rdseed_r64_rbx_success_flags",
            "1:\nrdseed %rbx\njnc 1b\nmovq $0, %rbx",
            Rdseed,
        ),
        (
            "rdseed_r16_cx_preserves_upper",
            "movabsq $0x8877665544332211, %rcx\n1:\nrdseed %cx\njnc 1b\nmovw $0, %cx",
            Rdseed,
        ),
        (
            "rdseed_r32_r8d_zeroext",
            "movabsq $-1, %r8\n1:\nrdseed %r8d\njnc 1b\nmovabsq $0xffffffff00000000, %r10\ntestq %r10, %r8\njz 2f\nmovq $1, %r8\njmp 3f\n2:\nmovq $0, %r8\n3:\naddq $0, %r8",
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
    for &(label, asm, feat) in &[
        (
            "invlpg_cache_tlb_edge_unaligned",
            "movq %r8, 33(%rax)\ncmpq %rcx, %r8\ninvlpg 33(%rax)\nmovq 33(%rax), %rcx",
            Invlpg,
        ),
        (
            "invlpg_cache_tlb_edge_negative_disp",
            "movq %r8, -16(%rbx)\ncmpq %rcx, %r8\ninvlpg -16(%rbx)\nmovq -16(%rbx), %rcx",
            Invlpg,
        ),
        (
            "invlpg_cache_tlb_edge_sib_zero_index",
            "leaq 128(%rax), %r8\nxorq %r9, %r9\ncmpq %rcx, %r8\ninvlpg (%r8,%r9,1)\nmovq %r8, %rcx",
            Invlpg,
        ),
        (
            "invlpg_cache_tlb_edge_stack_address",
            "movq %rsp, %r8\ncmpq %rcx, %r8\ninvlpg 16(%rsp)\nmovq %r8, %rcx",
            Invlpg,
        ),
        (
            "invpcid_cache_tlb_edge_type0_nonzero_linear",
            "movq $0, 32(%rax)\nmovq %rax, 40(%rax)\nmovq $0, %r8\ncmpq %rcx, %r9\ninvpcid 32(%rax), %r8\nmovq 40(%rax), %rcx",
            Invpcid,
        ),
        (
            "invpcid_cache_tlb_edge_type1_negative_disp",
            "movq $0, -16(%rbx)\nmovq $0, -8(%rbx)\nmovq $1, %r8\ncmpq %rcx, %r9\ninvpcid -16(%rbx), %r8\nmovq -16(%rbx), %rcx",
            Invpcid,
        ),
        (
            "invpcid_cache_tlb_edge_type2_indexed",
            "movq $0, 96(%rax)\nmovq $0, 104(%rax)\nleaq 96(%rax), %r9\nmovq $2, %r8\ncmpq %rcx, %r9\ninvpcid (%r9), %r8\nmovq 104(%rax), %rcx",
            Invpcid,
        ),
        (
            "invpcid_cache_tlb_edge_type3_addr32",
            "movq $0, 112(%rax)\nmovq $0, 120(%rax)\nmovq $3, %r8\ncmpq %rcx, %r9\naddr32 invpcid 112(%eax), %r8\nmovq 120(%rax), %rcx",
            Invpcid,
        ),
        (
            "invpcid_cache_tlb_edge_type_in_r9",
            "movq $0, 144(%rax)\nmovq %rax, 152(%rax)\nmovq $0, %r9\ncmpq %rcx, %r8\ninvpcid 144(%rax), %r9\nmovq 152(%rax), %rcx",
            Invpcid,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }

    // INVPCID has no directly visible TLB output in this harness, so these
    // cases cover all four invalidation types and descriptor addressing forms
    // while comparing preserved GPR, scratch, and RFLAGS state against KVM.
    for &(label, asm) in &[
        (
            "invpcid_type0_individual_address",
            "movq $0, 32(%rax)\nmovq %rax, 40(%rax)\nmovq $0, %r8\ncmpq %rcx, %r9\ninvpcid 32(%rax), %r8\nmovq 40(%rax), %rcx",
        ),
        (
            "invpcid_type1_single_context",
            "movq $0, 48(%rax)\nmovq $0, 56(%rax)\nmovq $1, %r8\ncmpq %rcx, %r9\ninvpcid 48(%rax), %r8\nmovq 48(%rax), %rcx",
        ),
        (
            "invpcid_type2_all_contexts",
            "movq $0, 64(%rax)\nmovq $0, 72(%rax)\nmovq $2, %r8\ncmpq %rcx, %r9\ninvpcid 64(%rax), %r8\nmovq 72(%rax), %rcx",
        ),
        (
            "invpcid_type3_all_nonglobal_contexts",
            "movq $0, 80(%rax)\nmovq $0, 88(%rax)\nmovq $3, %r8\ncmpq %rcx, %r9\ninvpcid 80(%rax), %r8\nmovq 80(%rax), %rcx",
        ),
        (
            "invpcid_indexed_descriptor",
            "movq $0, 96(%rax)\nmovq %rax, 104(%rax)\nleaq 96(%rax), %r9\nmovq $0, %r8\ncmpq %rcx, %r9\ninvpcid (%r9), %r8\nmovq 104(%rax), %rcx",
        ),
        (
            "invpcid_addr32_descriptor",
            "movq $0, 112(%rax)\nmovq %rax, 120(%rax)\nmovq $0, %r8\ncmpq %rcx, %r9\naddr32 invpcid 112(%eax), %r8\nmovq 120(%rax), %rcx",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Invpcid,
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
            "control_priv_state_edge_cr2_high_canonical_roundtrip",
            "movabsq $0xffff800000001230, %r8\nmovq %r8, %cr2\nxorq %r8, %r8\nmovq %cr2, %r8\ncmpq %r8, %r8",
            ControlReg,
        ),
        (
            "control_priv_state_edge_cr3_self_write_preserves",
            "movq %cr3, %r8\nmovq %r8, %cr3\nmovq %cr3, %r9\nxorq %r9, %r8\nsetz %cl\nmovzbl %cl, %ecx\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
            ControlReg,
        ),
        (
            "control_priv_state_edge_smsw_memory_matches_reg",
            "movw $0xffff, 32(%rax)\nsmsw 32(%rax)\nmovabsq $-1, %r8\nsmsw %r8w\nmovzwl 32(%rax), %ecx\nmovzwl %r8w, %r8d\nxorq %r8, %rcx\nsetz %cl\nmovzbl %cl, %ecx\nmovw $0, 32(%rax)\nxorq %r8, %r8\ncmpq %rax, %rax",
            ControlReg,
        ),
        (
            "control_priv_state_edge_lmsw_reg_clts_ts_bit",
            "movw $0x000b, %r8w\nlmsw %r8w\nsmsw %r9w\nclts\nsmsw %r10w\nandw $0x0008, %r9w\nandw $0x0008, %r10w\ncmpw $0x0008, %r9w\nsete %cl\ncmpw $0, %r10w\nsete %r8b\nandb %r8b, %cl\nmovzbl %cl, %ecx\nxorq %r8, %r8\nxorq %r9, %r9\nxorq %r10, %r10\ncmpq %rax, %rax",
            ControlReg,
        ),
        (
            "control_reg_edge_cr2_second_write_overwrites",
            "movabsq $0x0000000000123450, %r8\nmovq %r8, %cr2\nmovabsq $0x00000000006789a0, %r8\nmovq %r8, %cr2\nxorq %r9, %r9\nmovq %cr2, %r9\ncmpq %r9, %r9",
            ControlReg,
        ),
        (
            "control_reg_edge_cr2_zero_roundtrip",
            "xorq %r8, %r8\nmovq %r8, %cr2\nmovq %cr2, %rcx\ncmpq %rcx, %rcx",
            ControlReg,
        ),
        (
            "control_reg_edge_cr2_high_reg_destination",
            "movabsq $0xffff800000005000, %r8\nmovq %r8, %cr2\nxorq %r9, %r9\nmovq %cr2, %r9\ncmpq %r9, %r9",
            ControlReg,
        ),
        (
            "control_reg_edge_cr4_self_write_preserves",
            "movq %cr4, %r8\nmovq %r8, %cr4\nmovq %cr4, %r9\nxorq %r9, %r8\nsetz %cl\nmovzbl %cl, %ecx\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
            ControlReg,
        ),
        (
            "control_reg_edge_lmsw_memory_high_bits_ignored",
            "movw $0xfffb, 32(%rax)\nlmsw 32(%rax)\nsmsw %r9w\nclts\nandw $0x0008, %r9w\ncmpw $0x0008, %r9w\nsete %cl\nmovzbl %cl, %ecx\nxorq %r9, %r9\ncmpq %rax, %rax",
            ControlReg,
        ),
        (
            "control_reg_edge_clts_idempotent_after_lmsw",
            "movw $0x000b, %r8w\nlmsw %r8w\nclts\nclts\nsmsw %r9w\nandw $0x0008, %r9w\ncmpw $0, %r9w\nsete %cl\nmovzbl %cl, %ecx\nxorq %r8, %r8\nxorq %r9, %r9\ncmpq %rax, %rax",
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
            "descriptor_priv_state_edge_unaligned_gdt",
            "movw $0x0017, 33(%rax)\nmovabsq $0x0000000000006300, %r8\nmovq %r8, 35(%rax)\nlgdt 33(%rax)\nsgdt 64(%rax)",
            DescriptorTable,
        ),
        (
            "descriptor_priv_state_edge_unaligned_idt",
            "movw $0x0027, 49(%rax)\nmovabsq $0x0000000000007300, %r8\nmovq %r8, 51(%rax)\nlidt 49(%rax)\nsidt 80(%rax)",
            DescriptorTable,
        ),
        (
            "descriptor_priv_state_edge_indexed_gdt_idt",
            "leaq 96(%rax), %r9\nmovw $0x003f, (%r9)\nmovabsq $0x0000000000006400, %r8\nmovq %r8, 2(%r9)\nmovw $0x0047, 16(%r9)\nmovabsq $0x0000000000007400, %r8\nmovq %r8, 18(%r9)\nlgdt (%r9)\nlidt 16(%r9)\nsgdt 32(%r9)\nsidt 48(%r9)",
            DescriptorTable,
        ),
        (
            "descriptor_priv_state_edge_negative_disp_store",
            "movw $0x005f, 96(%rax)\nmovabsq $0x0000000000006500, %r8\nmovq %r8, 98(%rax)\nlgdt 96(%rax)\nsgdt -16(%rbx)",
            DescriptorTable,
        ),
        (
            "descriptor_priv_state_edge_addr32_store",
            "movw $0x0067, 112(%rax)\nmovabsq $0x0000000000007500, %r8\nmovq %r8, 114(%rax)\naddr32\nlidt 112(%eax)\naddr32\nsidt 144(%eax)",
            DescriptorTable,
        ),
        (
            "descriptor_table_edge_lgdt_second_load_overwrites",
            "movw $0x001f, 32(%rax)\nmovabsq $0x0000000000006000, %r8\nmovq %r8, 34(%rax)\nmovw $0x002f, 48(%rax)\nmovabsq $0x0000000000006800, %r8\nmovq %r8, 50(%rax)\nlgdt 32(%rax)\nlgdt 48(%rax)\nsgdt 64(%rax)",
            DescriptorTable,
        ),
        (
            "descriptor_table_edge_lidt_second_load_overwrites",
            "movw $0x0037, 80(%rax)\nmovabsq $0x0000000000007000, %r8\nmovq %r8, 82(%rax)\nmovw $0x0047, 96(%rax)\nmovabsq $0x0000000000007800, %r8\nmovq %r8, 98(%rax)\nlidt 80(%rax)\nlidt 96(%rax)\nsidt 112(%rax)",
            DescriptorTable,
        ),
        (
            "descriptor_table_edge_sgdt_sidt_sib_zero_index_store",
            "movw $0x0057, 32(%rax)\nmovabsq $0x0000000000006600, %r8\nmovq %r8, 34(%rax)\nmovw $0x0067, 48(%rax)\nmovabsq $0x0000000000007600, %r8\nmovq %r8, 50(%rax)\nlgdt 32(%rax)\nlidt 48(%rax)\nleaq 96(%rax), %r8\nxorq %r9, %r9\nsgdt (%r8,%r9,1)\nsidt 16(%r8,%r9,1)",
            DescriptorTable,
        ),
        (
            "descriptor_table_edge_lgdt_lidt_negative_disp_load",
            "movw $0x0077, -16(%rbx)\nmovabsq $0x0000000000006700, %r8\nmovq %r8, -14(%rbx)\nmovw $0x0087, (%rbx)\nmovabsq $0x0000000000007700, %r8\nmovq %r8, 2(%rbx)\nlgdt -16(%rbx)\nlidt (%rbx)\nsgdt 64(%rax)\nsidt 80(%rax)",
            DescriptorTable,
        ),
        (
            "descriptor_table_edge_lgdt_lidt_stack_operand",
            "movw $0x0097, 16(%rsp)\nmovabsq $0x0000000000006900, %r8\nmovq %r8, 18(%rsp)\nmovw $0x00a7, 32(%rsp)\nmovabsq $0x0000000000007900, %r8\nmovq %r8, 34(%rsp)\nlgdt 16(%rsp)\nlidt 32(%rsp)\nsgdt 96(%rax)\nsidt 112(%rax)",
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
            "msr_priv_state_edge_star_roundtrip",
            "movl $0xc0000081, %ecx\nmovabsq $0x0018000800000000, %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nwrmsr\nrdmsr\nmovq %rax, %rbx\nmovq %rdx, %rsi\ncmpq %rbx, %rbx",
            Msr,
        ),
        (
            "msr_priv_state_edge_lstar_roundtrip",
            "movl $0xc0000082, %ecx\nmovabsq $0x0000000000401230, %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nwrmsr\nrdmsr\nmovq %rax, %rbx\nmovq %rdx, %rsi\ncmpq %rbx, %rbx",
            Msr,
        ),
        (
            "msr_priv_state_edge_fmask_roundtrip",
            "movl $0xc0000084, %ecx\nmovl $0x00047700, %eax\nxorl %edx, %edx\nwrmsr\nrdmsr\nmovq %rax, %rbx\nmovq %rdx, %rsi",
            Msr,
        ),
        (
            "msr_priv_state_edge_efer_sce_roundtrip",
            "movl $0xc0000080, %ecx\nrdmsr\norl $1, %eax\nwrmsr\nrdmsr\nandl $1, %eax\nmovl %eax, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\ncmpq %rax, %rax",
            Msr,
        ),
        (
            "msr_priv_state_edge_fs_base_high_canonical",
            "movl $0xc0000100, %ecx\nmovl $0x00004000, %eax\nmovl $0xffff8000, %edx\nwrmsr\nrdmsr\nmovq %rax, %rbx\nmovq %rdx, %rsi",
            Msr,
        ),
        (
            "msr_edge_cstar_roundtrip",
            "movl $0xc0000083, %ecx\nmovabsq $0x0000000000405678, %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nwrmsr\nrdmsr\nmovq %rax, %rbx\nmovq %rdx, %rsi\ncmpq %rbx, %rbx",
            Msr,
        ),
        (
            "msr_edge_sysenter_cs_roundtrip",
            "movl $0x174, %ecx\nmovl $0x8, %eax\nxorl %edx, %edx\nwrmsr\nrdmsr\nmovq %rax, %rbx\nmovq %rdx, %rsi",
            Msr,
        ),
        (
            "msr_edge_sysenter_esp_roundtrip",
            "movl $0x175, %ecx\nmovl $0x20000, %eax\nxorl %edx, %edx\nwrmsr\nrdmsr\nmovq %rax, %rbx\nmovq %rdx, %rsi",
            Msr,
        ),
        (
            "msr_edge_sysenter_eip_roundtrip",
            "movl $0x176, %ecx\nmovabsq $0x0000000000301230, %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nwrmsr\nrdmsr\nmovq %rax, %rbx\nmovq %rdx, %rsi\ncmpq %rbx, %rbx",
            Msr,
        ),
        (
            "msr_edge_wrmsr_high_halves_ignored",
            "movl $0xc0000100, %ecx\nmovabsq $0xffffffff00005000, %rax\nmovabsq $0xffffffff00000000, %rdx\nwrmsr\nrdmsr\nmovq %rax, %rbx\nmovq %rdx, %rsi\ncmpq %rbx, %rbx",
            Msr,
        ),
        (
            "msr_edge_fmask_second_write_overwrites",
            "movl $0xc0000084, %ecx\nmovl $0x600, %eax\nxorl %edx, %edx\nwrmsr\nmovl $0x200, %eax\nwrmsr\nrdmsr\nmovq %rax, %rbx\nmovq %rdx, %rsi",
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
        (
            "debug_priv_state_edge_dr0_zero_roundtrip",
            "xorq %r8, %r8\nmovq %r8, %dr0\nmovq %dr0, %rcx",
            DebugReg,
        ),
        (
            "debug_priv_state_edge_dr1_dr2_high_roundtrip",
            "movabsq $0x00007fff00004000, %r8\nmovq %r8, %dr1\nmovq %dr1, %rcx\nmovabsq $0x00007fff00004008, %r8\nmovq %r8, %dr2\nmovq %dr2, %rdx",
            DebugReg,
        ),
        (
            "debug_priv_state_edge_dr7_local_exec_enable",
            "movabsq $0x0000000000004000, %r8\nmovq %r8, %dr0\nmovabsq $0x401, %r8\nmovq %r8, %dr7\nmovq %dr7, %r9",
            DebugReg,
        ),
        (
            "debug_priv_state_edge_dr7_write_len_fields",
            "movabsq $0x0000000000004010, %r8\nmovq %r8, %dr0\nmovabsq $0x00000000000d0401, %r8\nmovq %r8, %dr7\nmovq %dr7, %r9",
            DebugReg,
        ),
        (
            "debug_reg_edge_dr3_high_reg_destination",
            "movabsq $0x0000000000004020, %r8\nmovq %r8, %dr3\nxorq %r9, %r9\nmovq %dr3, %r9\ncmpq %r9, %r9",
            DebugReg,
        ),
        (
            "debug_reg_edge_dr0_second_write_overwrites",
            "movabsq $0x0000000000004000, %r8\nmovq %r8, %dr0\nxorq %r8, %r8\nmovq %r8, %dr0\nmovq %dr0, %rcx\ncmpq %rcx, %rcx",
            DebugReg,
        ),
        (
            "debug_reg_edge_dr6_reset_value_roundtrip",
            "movabsq $0x00000000ffff0ff0, %r8\nmovq %r8, %dr6\nxorq %r9, %r9\nmovq %dr6, %r9\ncmpq %r9, %r9",
            DebugReg,
        ),
        (
            "debug_reg_edge_dr7_high_reg_source_roundtrip",
            "movabsq $0x0000000000000400, %r9\nmovq %r9, %dr7\nxorq %r9, %r9\nmovq %dr7, %r9\ncmpq %r9, %r9",
            DebugReg,
        ),
        (
            "debug_reg_edge_dr7_second_write_clears_local_enable",
            "movabsq $0x0000000000004000, %r8\nmovq %r8, %dr0\nmovabsq $0x401, %r8\nmovq %r8, %dr7\nmovabsq $0x400, %r8\nmovq %r8, %dr7\nmovq %dr7, %r9\ncmpq %r9, %r9",
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
        (
            "lsl_descriptor_access_edge_mem_selector_high_reg",
            "movw $0x8, 48(%rax)\nmovq $-1, %r10\nlsl 48(%rax), %r10d\nsetz %cl\ncmpl $0x12345, %r10d\nsete %dl\nandb %dl, %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\ncmpq %rcx, %rcx",
        ),
        (
            "lar_descriptor_access_edge_mem_selector_high_reg",
            "movw $0x8, 50(%rax)\nxorq %r11, %r11\nlar 50(%rax), %r11d\nsetz %cl\ntestl %r11d, %r11d\nsetnz %dl\nandb %dl, %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\ncmpq %rcx, %rcx",
        ),
        (
            "lar_descriptor_access_edge_invalid_mem_preserves_dest",
            "movw $0x18, 52(%rax)\nmovl $0x7777, %r9d\nlar 52(%rax), %r9d\nsetnz %cl\ncmpl $0x7777, %r9d\nsete %dl\nandb %dl, %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r9, %r9\ncmpq %rcx, %rcx",
        ),
        (
            "lsl_descriptor_access_edge_addr32_mem_selector",
            "movw $0x8, 54(%rax)\naddr32 lsl 54(%eax), %r8d\nsetz %cl\ncmpl $0x12345, %r8d\nsete %dl\nandb %dl, %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r8, %r8\ncmpq %rcx, %rcx",
        ),
        (
            "lsl_descriptor_access_edge_invalid_mem_preserves_dest",
            "movw $0x18, 56(%rax)\nmovl $0x7777, %r9d\nlsl 56(%rax), %r9d\nsetnz %cl\ncmpl $0x7777, %r9d\nsete %dl\nandb %dl, %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r9, %r9\ncmpq %rcx, %rcx",
        ),
        (
            "verr_descriptor_access_edge_mem_selector_valid",
            "movw $0x8, 58(%rax)\ncmpq %rcx, %r8\nverr 58(%rax)\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\ncmpq %rcx, %rcx",
        ),
        (
            "verw_descriptor_access_edge_mem_selector_valid",
            "movw $0x8, 60(%rax)\ncmpq %rcx, %r8\nverw 60(%rax)\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\ncmpq %rcx, %rcx",
        ),
        (
            "verr_descriptor_access_edge_null_mem_selector_clears_zf",
            "movw $0, 62(%rax)\ncmpq %r8, %r8\nverr 62(%rax)\nsetnz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\ncmpq %rcx, %rcx",
        ),
        (
            "lar_descriptor_access_edge_stale_flags_valid_reg",
            "movl $0, %r9d\nmovw $0x8, %r8w\ncmpq %rcx, %r9\nlar %r8w, %r9d\nsetz %cl\ntestl %r9d, %r9d\nsetnz %dl\nandb %dl, %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r9, %r9\ncmpq %rcx, %rcx",
        ),
        (
            "lsl_descriptor_access_edge_stale_flags_invalid_mem",
            "movw $0x18, 64(%rax)\nmovl $0x7777, %r9d\ncmpq %r8, %r8\nlsl 64(%rax), %r9d\nsetnz %cl\ncmpl $0x7777, %r9d\nsete %dl\nandb %dl, %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r9, %r9\ncmpq %rcx, %rcx",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: format!("{descriptor_access_setup}\n{check}"),
            feat: DescriptorAccess,
            profile: Int,
        });
    }

    // Group-6 descriptor-register stores. These read the current LDTR/TR
    // selectors into register and memory destinations, covering the SLDT/STR
    // paths that complement the LAR/LSL/VERR/VERW descriptor access cases.
    for &(label, asm) in &[
        (
            "sldt_descriptor_group6_r64_zeroext",
            "movabsq $-1, %r8\nsldt %r8",
        ),
        (
            "str_descriptor_group6_r64_zeroext",
            "movabsq $-1, %r9\nstr %r9",
        ),
        (
            "sldt_descriptor_group6_m16",
            "movw $0xffff, 152(%rax)\nsldt 152(%rax)",
        ),
        (
            "str_descriptor_group6_m16",
            "movw $0xffff, 154(%rax)\nstr 154(%rax)",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: DescriptorAccess,
            profile: Int,
        });
    }

    // Group-6 descriptor-register loads. A scratch-local GDT provides a valid
    // 64-bit available TSS descriptor. LTR marks the TSS descriptor busy on
    // silicon, so the snippets clear the scratch GDT before comparison after
    // observing the loaded selector through STR. LLDT uses null-selector loads:
    // they are architecturally valid and still exercise the LLDT state path
    // without relying on host/KVM acceptance of a guest-provided LDT descriptor.
    let descriptor_group6_load_setup = "movq $0, 128(%rax)\nmovq $0, 144(%rax)\nmovabsq $0x0000890040800067, %r8\nmovq %r8, 152(%rax)\nmovq $0, 160(%rax)\nmovw $0x0027, 32(%rax)\nleaq 128(%rax), %r8\nmovq %r8, 34(%rax)\nlgdt 32(%rax)";
    let descriptor_group6_clear_gdt = "movq $0, 128(%rax)\nmovq $0, 136(%rax)\nmovq $0, 144(%rax)\nmovq $0, 152(%rax)\nmovq $0, 160(%rax)";
    for &(label, check) in &[
        (
            "lldt_descriptor_group6_load_reg_null_selector",
            "xorw %r8w, %r8w\nlldt %r8w\nsldt %r9w\ntestw %r9w, %r9w\nsetz %cl\nmovzbl %cl, %ecx\nxorq %r8, %r8\nxorq %r9, %r9",
        ),
        (
            "lldt_descriptor_group6_load_mem_null_selector",
            "movw $0, 48(%rax)\nlldt 48(%rax)\nsldt %r9w\ntestw %r9w, %r9w\nsetz %cl\nmovzbl %cl, %ecx\nxorq %r9, %r9",
        ),
        (
            "lldt_descriptor_group6_load_preserves_zf",
            "xorw %r8w, %r8w\ncmpq %r9, %r9\nlldt %r8w\nsetz %cl\nmovzbl %cl, %ecx\nxorq %r8, %r8",
        ),
        (
            "ltr_descriptor_group6_load_reg_selector_roundtrip",
            "movw $0x18, %r8w\nltr %r8w\nstr %r9w\ncmpw $0x18, %r9w\nsete %cl\nmovzbl %cl, %ecx\nxorq %r8, %r8\nxorq %r9, %r9",
        ),
        (
            "ltr_descriptor_group6_load_mem_selector_roundtrip",
            "movw $0x18, 50(%rax)\nltr 50(%rax)\nstr %r9w\ncmpw $0x18, %r9w\nsete %cl\nmovzbl %cl, %ecx\nxorq %r9, %r9",
        ),
        (
            "ltr_descriptor_group6_load_preserves_zf",
            "movw $0x18, %r10w\ncmpq %r9, %r9\nltr %r10w\nsetz %cl\nmovzbl %cl, %ecx\nxorq %r10, %r10",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: format!(
                "{descriptor_group6_load_setup}\n{check}\n{descriptor_group6_clear_gdt}"
            ),
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
        ("stac_protection_edge_ac_set_from_clear", "clac\nstac"),
        ("clac_protection_edge_ac_clear_from_set", "stac\nclac"),
        (
            "stac_clac_protection_edge_repeated_idempotent",
            "clac\nclac\nstac\nstac\nclac",
        ),
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
            "rdpkru_protection_edge_zero_ext",
            "movq %cr4, %rax\norq $0x400000, %rax\nmovq %rax, %cr4\nmovl $0, %eax\nmovl $0, %ecx\nmovl $0, %edx\nwrpkru\nmovq $-1, %rax\nmovq $-1, %rdx\nrdpkru",
            Pku,
        ),
        (
            "wrpkru_protection_edge_all_nonzero_keys",
            "movq %cr4, %rax\norq $0x400000, %rax\nmovq %rax, %cr4\nmovl $0xfffffff0, %eax\nmovl $0, %ecx\nmovl $0, %edx\nwrpkru\nrdpkru",
            Pku,
        ),
        (
            "wrpkru_protection_edge_high_halves_ignored",
            "movq %cr4, %rax\norq $0x400000, %rax\nmovq %rax, %cr4\nmovl $0x33333330, %eax\nmovabsq $0xffffffff00000000, %rcx\nmovabsq $0xffffffff00000000, %rdx\nwrpkru\nrdpkru",
            Pku,
        ),
        (
            "wrpkru_protection_edge_second_write_overwrites",
            "movq %cr4, %rax\norq $0x400000, %rax\nmovq %rax, %cr4\nmovl $0x11111110, %eax\nmovl $0, %ecx\nmovl $0, %edx\nwrpkru\nmovl $0x22222220, %eax\nwrpkru\nrdpkru",
            Pku,
        ),
        (
            "wrpkru_protection_edge_high_rax_ignored",
            "movq %cr4, %rax\norq $0x400000, %rax\nmovq %rax, %cr4\nmovabsq $0xffffffff55555550, %rax\nxorl %ecx, %ecx\nxorl %edx, %edx\nwrpkru\nmovq $-1, %rax\nmovq $-1, %rdx\nrdpkru\ncmpq %rax, %rax",
            Pku,
        ),
        (
            "rdpkru_protection_edge_preserves_cmp_flags",
            "movq %cr4, %rax\norq $0x400000, %rax\nmovq %rax, %cr4\nmovl $0x123450, %eax\nxorl %ecx, %ecx\nxorl %edx, %edx\nwrpkru\ncmpq %r8, %r8\nrdpkru",
            Pku,
        ),
        (
            "wrpkru_protection_edge_zero_second_write",
            "movq %cr4, %rax\norq $0x400000, %rax\nmovq %rax, %cr4\nmovl $0xfffffff0, %eax\nxorl %ecx, %ecx\nxorl %edx, %edx\nwrpkru\nxorl %eax, %eax\nwrpkru\nmovq $-1, %rax\nmovq $-1, %rdx\nrdpkru\ncmpq %rax, %rax",
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
        (
            "swapgs_protection_edge_double_swap_restores",
            "movl $0xc0000102, %ecx\nmovabsq $0x0000000000abc000, %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nwrmsr\nmovabsq $0x0000000000def000, %rax\nwrgsbase %rax\ncmpq %rcx, %r8\nswapgs\nswapgs\nrdgsbase %r8",
            Swapgs,
        ),
        (
            "swapgs_protection_edge_kernel_gs_visible",
            "movl $0xc0000102, %ecx\nmovabsq $0x0000000000333000, %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nwrmsr\nmovabsq $0x0000000000444000, %rax\nwrgsbase %rax\ncmpq %rcx, %r8\nswapgs\nrdgsbase %r8\nswapgs\nrdgsbase %r9",
            Swapgs,
        ),
        (
            "swapgs_protection_edge_gs_memory_base",
            "movabsq $0x1111222233334444, %r8\nmovq %r8, 32(%rdi)\nmovabsq $0x5555666677778888, %r8\nmovq %r8, 128(%rdi)\nmovl $0xc0000102, %ecx\nmovabsq $0x40a0, %rax\nmovq %rax, %rdx\nshrq $32, %rdx\nwrmsr\nmovabsq $0x4040, %rax\nwrgsbase %rax\ncmpq %rcx, %r8\nswapgs\nmovq %gs:0, %r8\nswapgs\nmovq %gs:0, %r9",
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
    for &(label, asm, feat) in &[
        (
            "rdtsc_lfence_normalized",
            "lfence\nrdtsc\nmovq $0, %rax\nmovq $0, %rdx",
            Tsc,
        ),
        (
            "rdtsc_serialize_normalized",
            "serialize\nrdtsc\nmovq $0, %rax\nmovq $0, %rdx",
            Tsc,
        ),
        (
            "rdtscp_reads_aux_zero",
            "rdtscp\nmovq $0, %rax\nmovq $0, %rdx",
            Rdtscp,
        ),
        (
            "rdtscp_lfence_normalized",
            "lfence\nrdtscp\nmovq $0, %rax\nmovq $0, %rdx\nmovq $0, %rcx",
            Rdtscp,
        ),
        (
            "rdtsc_tsc_edge_preserves_rcx",
            "movabsq $0x1122334455667788, %rcx\nrdtsc\nmovabsq $0x1122334455667788, %r8\ncmpq %r8, %rcx\nsete %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r8, %r8\ncmpq %rax, %rax",
            Tsc,
        ),
        (
            "rdtsc_tsc_edge_preserves_non_query_gprs",
            "movabsq $0x1122334455667788, %r8\nmovabsq $0x8877665544332211, %r9\nrdtsc\nxorq %rax, %rax\nxorq %rdx, %rdx\ncmpq %r8, %r8",
            Tsc,
        ),
        (
            "rdtsc_tsc_edge_monotonic_pair",
            "rdtsc\nshlq $32, %rdx\norq %rdx, %rax\nmovq %rax, %r8\nrdtsc\nshlq $32, %rdx\norq %rdx, %rax\ncmpq %r8, %rax\nsetae %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r8, %r8\ncmpq %rax, %rax",
            Tsc,
        ),
        (
            "rdtscp_tsc_edge_dirty_rcx_aux_zero",
            "movq $-1, %rcx\nrdtscp\ntestl %ecx, %ecx\nsetz %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\ncmpq %rax, %rax",
            Rdtscp,
        ),
        (
            "rdtscp_tsc_edge_monotonic_pair",
            "rdtscp\nshlq $32, %rdx\norq %rdx, %rax\nmovq %rax, %r8\nrdtscp\nshlq $32, %rdx\norq %rdx, %rax\ncmpq %r8, %rax\nsetae %cl\nmovzbl %cl, %ecx\nxorq %rax, %rax\nxorq %rdx, %rdx\nxorq %r8, %r8\ncmpq %rax, %rax",
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

    // FS/GS segment-override memory references. Each case programs the segment
    // base to the scratch page, then uses a small segment offset so loads,
    // stores, RMW operations, address-size overrides, moffs, and LEA behavior
    // are visible through compared GPRs or scratch bytes.
    for &(label, asm) in &[
        (
            "fs_segment_load_m64",
            "movabsq $0x4000, %r8\nwrfsbase %r8\nxorq %r9, %r9\nmovq %fs:32(%r9), %rcx",
        ),
        (
            "gs_segment_load_m32_zeroext",
            "movabsq $0x4000, %r8\nwrgsbase %r8\nmovabsq $-1, %rcx\nxorq %r9, %r9\nmovl %gs:36(%r9), %ecx",
        ),
        (
            "fs_segment_store_m64",
            "movabsq $0x4000, %r8\nwrfsbase %r8\nxorq %r9, %r9\nmovq %rcx, %fs:48(%r9)",
        ),
        (
            "gs_segment_store_m16",
            "movabsq $0x4000, %r8\nwrgsbase %r8\nxorq %r9, %r9\nmovw %cx, %gs:58(%r9)",
        ),
        (
            "fs_segment_add_m64",
            "movabsq $0x4000, %r8\nwrfsbase %r8\nxorq %r9, %r9\naddq %rcx, %fs:64(%r9)",
        ),
        (
            "gs_segment_addr32_load_m64",
            "movabsq $0x4000, %r8\nwrgsbase %r8\nxorl %eax, %eax\nmovq $-1, %rcx\naddr32 movq %gs:72(%eax), %rcx",
        ),
        (
            "fs_segment_moffs_load_rax",
            "movabsq $0x4000, %r8\nwrfsbase %r8\n.byte 0x64, 0x48, 0xa1\n.quad 80",
        ),
        (
            "gs_segment_moffs_store_rax",
            "movabsq $0x4000, %r8\nwrgsbase %r8\nmovabsq $0x1122334455667788, %rax\n.byte 0x65, 0x48, 0xa3\n.quad 88",
        ),
        (
            "fs_segment_lea_ignores_base",
            "movabsq $0x4000, %r8\nwrfsbase %r8\nxorq %r9, %r9\nleaq %fs:96(%r9), %rcx",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Fsgsbase,
            profile: Int,
        });
    }

    // FS/GS segment overrides on string-source operands. MOVS/LODS/CMPS honor
    // the source segment base while STOS/SCAS destinations remain ES-relative;
    // these snippets seed the string registers explicitly after programming the
    // segment base so pointer movement, loaded accumulator state, flags, and
    // destination scratch effects are all compared.
    for &(label, asm) in &[
        (
            "fs_segstring_movsb",
            "movabsq $0x4000, %r8\nwrfsbase %r8\nmovl $128, %esi\nmovabsq $0x4020, %rdi\n.byte 0x64\nmovsb",
        ),
        (
            "fs_segstring_rep_movsb",
            "movabsq $0x4000, %r8\nwrfsbase %r8\nmovl $128, %esi\nmovabsq $0x4020, %rdi\nmovl $4, %ecx\n.byte 0x64\nrep movsb",
        ),
        (
            "fs_segstring_addr32_movsb",
            "movabsq $0x4000, %r8\nwrfsbase %r8\nmovl $128, %esi\nmovl $0x4020, %edi\n.byte 0x64\naddr32 movsb",
        ),
        (
            "fs_segstring_lodsb",
            "movabsq $0x4000, %r8\nwrfsbase %r8\nmovl $128, %esi\nmovabsq $-1, %rax\n.byte 0x64\nlodsb",
        ),
        (
            "gs_segstring_lodsl_zeroext",
            "movabsq $0x4000, %r8\nwrgsbase %r8\nmovl $132, %esi\nmovabsq $-1, %rax\n.byte 0x65\nlodsl",
        ),
        (
            "fs_segstring_cmpsb_equal",
            "movb 128(%rax), %r10b\nmovb %r10b, 32(%rax)\nmovabsq $0x4000, %r8\nwrfsbase %r8\nmovl $128, %esi\nmovabsq $0x4020, %rdi\n.byte 0x64\ncmpsb",
        ),
        (
            "gs_segstring_repe_cmpsb_equal",
            "movl 128(%rax), %r10d\nmovl %r10d, 32(%rax)\nmovabsq $0x4000, %r8\nwrgsbase %r8\nmovl $128, %esi\nmovabsq $0x4020, %rdi\nmovl $4, %ecx\n.byte 0x65\nrepe cmpsb",
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

    for &(label, asm) in &[
        ("enter_stack_frame_edge_alloc0_nesting0", "enter $0x0, $0"),
        ("enter_stack_frame_edge_alloc0_nesting2", "enter $0x0, $2"),
        ("enter_stack_frame_edge_alloc16_nesting2", "enter $0x10, $2"),
        (
            "enter_leave_stack_frame_edge_nesting2_roundtrip",
            "enter $0x10, $2\nleave",
        ),
        ("enter_stack_frame_edge_nesting_mask_34", "enter $0x0, $34"),
        (
            "enter_stack_frame_edge_data16_alloc4",
            ".byte 0x66, 0xc8, 0x04, 0x00, 0x00\n",
        ),
        (
            "enter_leave_stack_frame_edge_data16_roundtrip",
            ".byte 0x66, 0xc8, 0x04, 0x00, 0x00\n.byte 0x66, 0xc9",
        ),
        (
            "leave_stack_frame_edge_data16_from_scratch",
            "leaq 32(%rax), %rbp\nmovw $0x5678, (%rbp)\n.byte 0x66, 0xc9",
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
        ("cli_sti_flag_control_edge_roundtrip", "cli\nsti"),
        ("pushfw_flag_control_edge_popw_image", "pushfw\npopw %r8w"),
        ("pushfq_flag_control_edge_popq_image", "pushfq\npopq %r8"),
        (
            "popfw_flag_control_edge_low_status_if_df",
            "pushw $0x0ed7\npopfw",
        ),
        (
            "popfw_flag_control_edge_clear_low_flags",
            "pushw $0x0002\npopfw",
        ),
        (
            "popfq_flag_control_edge_all_status_if_df",
            "pushq $0x0ed7\npopfq",
        ),
        (
            "popfq_flag_control_edge_clear_status_if_df",
            "pushq $0x0002\npopfq",
        ),
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

    // Exchange/atomic operand-width variants. These hit high-byte register
    // paths, word memory operands, same-register XADD aliases, CMPXCHG
    // success/failure writeback, and LOCK-prefixed memory RMW forms.
    for &(label, asm) in &[
        ("bswap_core_atomic_width_r9d", "bswapl %r9d"),
        ("bswap_core_atomic_width_rax", "bswapq %rax"),
        ("xchg_core_atomic_width_high8", "xchgb %ch, %dl"),
        ("xchg_core_atomic_width_r16_reg", "xchgw %cx, %r8w"),
        ("xchg_core_atomic_width_m16_r8w", "xchgw %r8w, 18(%rax)"),
        ("xchg_core_atomic_width_rax_r9", "xchgq %r9, %rax"),
        ("xadd_core_atomic_width_high8", "xaddb %ch, %dl"),
        ("xadd_core_atomic_width_same_r8b", "xaddb %r8b, %r8b"),
        ("xadd_core_atomic_width_r16_reg", "xaddw %cx, %r8w"),
        ("xadd_core_atomic_width_m16_r8w", "xaddw %r8w, 18(%rax)"),
        ("xadd_core_atomic_width_same_r32", "xaddl %r8d, %r8d"),
        ("xadd_core_atomic_width_same_r64", "xaddq %r8, %r8"),
        (
            "lock_xadd_core_atomic_width_m16_r8w",
            "lock xaddw %r8w, 20(%rax)",
        ),
        ("cmpxchg_core_atomic_width_r8_success", "cmpxchgb %r8b, %al"),
        ("cmpxchg_core_atomic_width_high8_fail", "cmpxchgb %ch, %dl"),
        ("cmpxchg_core_atomic_width_r16_fail", "cmpxchgw %r8w, %cx"),
        ("cmpxchg_core_atomic_width_r32_fail", "cmpxchgl %r8d, %ecx"),
        (
            "cmpxchg_core_atomic_width_m8_success",
            "movb %al, 24(%rax)\ncmpxchgb %r8b, 24(%rax)",
        ),
        (
            "cmpxchg_core_atomic_width_m16_success",
            "movw %ax, 26(%rax)\ncmpxchgw %r8w, 26(%rax)",
        ),
        (
            "cmpxchg_core_atomic_width_m32_success",
            "movl %eax, 28(%rax)\ncmpxchgl %r8d, 28(%rax)",
        ),
        (
            "cmpxchg_core_atomic_width_m16_fail",
            "cmpxchgw %r8w, 30(%rax)",
        ),
        (
            "lock_cmpxchg_core_atomic_width_m32_success",
            "movl %eax, 36(%rax)\nlock cmpxchgl %r8d, 36(%rax)",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // LOCK-prefixed core read-modify-write forms. Single-vCPU execution makes
    // atomicity unobservable, but the prefix must still be accepted only on
    // legal memory destinations while preserving the ordinary instruction
    // semantics for memory, register, and RFLAGS outputs.
    for &(label, asm) in &[
        ("add_core_lock_m64_r8", "lock addq %r8, 8(%rax)"),
        ("or_core_lock_m32_imm8", "lock orl $0x55, 16(%rax)"),
        ("adc_core_lock_m16_imm8", "lock adcw $0x11, 24(%rax)"),
        ("sub_core_lock_m8_r8b", "lock subb %r8b, 2(%rax)"),
        ("and_core_lock_m64_imm8", "lock andq $0x7f, 32(%rax)"),
        ("xor_core_lock_m32_r8d", "lock xorl %r8d, 40(%rax)"),
        ("not_core_lock_m64", "lock notq 48(%rax)"),
        ("neg_core_lock_m32", "lock negl 56(%rax)"),
        ("inc_core_lock_m64", "lock incq 64(%rax)"),
        ("dec_core_lock_m16", "lock decw 72(%rax)"),
        ("xchg_core_lock_m64_r8", "lock xchgq %r8, 80(%rax)"),
        ("xadd_core_lock_m64_r8", "lock xaddq %r8, 88(%rax)"),
        (
            "cmpxchg_core_lock_m64_success",
            "movq %rax, 96(%rax)\nlock cmpxchgq %r8, 96(%rax)",
        ),
        ("bts_core_lock_m64_imm", "lock btsq $9, 104(%rax)"),
        ("btr_core_lock_m64_r9", "lock btrq %r9, (%rax)"),
        ("btc_core_lock_m64_imm", "lock btcq $20, 112(%rax)"),
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
        (
            "cmpxchg8b_cmpxchg_edge_success_dirty_accumulator",
            "movl $0x89abcdef, -16(%rdi)\nmovl $0x01234567, -12(%rdi)\nmovabsq $0xffff000089abcdef, %rax\nmovabsq $0xeeee000001234567, %rdx\nmovl $0x76543210, %ebx\nmovl $0xfedcba98, %ecx\ncmpxchg8b -16(%rdi)",
            Cx8,
        ),
        (
            "cmpxchg8b_cmpxchg_edge_failure_zero_ext",
            "movl $0x01020304, 40(%rdi)\nmovl $0x05060708, 44(%rdi)\nmovabsq $0xffff0000deadbeef, %rax\nmovabsq $0xeeee0000badc0ffe, %rdx\nmovl $0xa1a2a3a4, %ebx\nmovl $0xb1b2b3b4, %ecx\ncmpxchg8b 40(%rdi)",
            Cx8,
        ),
        (
            "cmpxchg8b_cmpxchg_edge_lock_success_zero_value",
            "movl $0, 48(%rdi)\nmovl $0, 52(%rdi)\nmovabsq $0xaaaa000000000000, %rax\nmovabsq $0xbbbb000000000000, %rdx\nmovl $0xffffffff, %ebx\nmovl $0x80000000, %ecx\nlock cmpxchg8b 48(%rdi)",
            Cx8,
        ),
        (
            "cmpxchg8b_cmpxchg_edge_lock_failure_preserves_memory",
            "movl $0x13579bdf, 56(%rdi)\nmovl $0x2468ace0, 60(%rdi)\nmovabsq $0x11110000aaaaaaaa, %rax\nmovabsq $0x22220000bbbbbbbb, %rdx\nmovl $0xcafebabe, %ebx\nmovl $0x0badf00d, %ecx\nlock cmpxchg8b 56(%rdi)",
            Cx8,
        ),
        (
            "cmpxchg16b_cmpxchg_edge_success_negative_disp",
            "movabsq $0x0123456789abcdef, %r8\nmovq %r8, -16(%rdi)\nmovabsq $0xfedcba9876543210, %r8\nmovq %r8, -8(%rdi)\nmovabsq $0x0123456789abcdef, %rax\nmovabsq $0xfedcba9876543210, %rdx\nmovabsq $0x1111222233334444, %rbx\nmovabsq $0x5555666677778888, %rcx\ncmpxchg16b -16(%rdi)",
            Cx16,
        ),
        (
            "cmpxchg16b_cmpxchg_edge_failure_loads_pair",
            "movabsq $0x1020304050607080, %r8\nmovq %r8, 80(%rdi)\nmovabsq $0x90a0b0c0d0e0f000, %r8\nmovq %r8, 88(%rdi)\nmovabsq $0xffffeeee11112222, %rax\nmovabsq $0xddddcccc33334444, %rdx\nmovabsq $0x0001020304050607, %rbx\nmovabsq $0x08090a0b0c0d0e0f, %rcx\ncmpxchg16b 80(%rdi)",
            Cx16,
        ),
        (
            "cmpxchg16b_cmpxchg_edge_lock_success_high_bits",
            "movabsq $0xaaaaaaaa55555555, %r8\nmovq %r8, 96(%rdi)\nmovabsq $0x123456789abcdef0, %r8\nmovq %r8, 104(%rdi)\nmovabsq $0xaaaaaaaa55555555, %rax\nmovabsq $0x123456789abcdef0, %rdx\nmovabsq $0xffffffffffffffff, %rbx\nmovabsq $0x8000000000000000, %rcx\nlock cmpxchg16b 96(%rdi)",
            Cx16,
        ),
        (
            "cmpxchg16b_cmpxchg_edge_lock_failure_preserves_memory",
            "movabsq $0x0f0e0d0c0b0a0908, %r8\nmovq %r8, 112(%rdi)\nmovabsq $0x0706050403020100, %r8\nmovq %r8, 120(%rdi)\nmovabsq $0x9999999999999999, %rax\nmovabsq $0x8888888888888888, %rdx\nmovabsq $0x7777777777777777, %rbx\nmovabsq $0x6666666666666666, %rcx\nlock cmpxchg16b 112(%rdi)",
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

    // Non-qword legacy group/direct integer forms. These cover byte, word, and
    // dword register and memory operands across group-1 immediate dispatch,
    // direct ALU/logical ModR/M opcodes, high-byte registers, and group-3
    // TEST/NOT/NEG forms.
    for &(label, asm) in &[
        ("add_core_group_width_m8_r8", "addb %r8b, 1(%rax)"),
        ("add_core_group_width_r16_mem_src", "addw 4(%rax), %r8w"),
        ("adc_core_group_width_r8_mem_src", "adcb 2(%rax), %r8b"),
        ("adc_core_group_width_m32_r8", "adcl %r8d, 8(%rax)"),
        ("sbb_core_group_width_m8_r8", "sbbb %r8b, 3(%rax)"),
        ("sbb_core_group_width_r16_mem_src", "sbbw 6(%rax), %r8w"),
        ("sub_core_group_width_r8_mem_src", "subb 4(%rax), %r8b"),
        ("sub_core_group_width_m32_r8", "subl %r8d, 12(%rax)"),
        ("cmp_core_group_width_m8_r8", "cmpb %r8b, 5(%rax)"),
        ("cmp_core_group_width_r16_mem_src", "cmpw 8(%rax), %r8w"),
        ("or_core_group_width_m8_r8", "orb %r8b, 6(%rax)"),
        ("or_core_group_width_r16_mem_src", "orw 10(%rax), %r8w"),
        ("and_core_group_width_r8_mem_src", "andb 7(%rax), %r8b"),
        ("and_core_group_width_m32_r8", "andl %r8d, 16(%rax)"),
        ("xor_core_group_width_m8_r8", "xorb %r8b, 17(%rax)"),
        ("xor_core_group_width_r32_mem_src", "xorl 20(%rax), %r8d"),
        ("test_core_group_width_m8_r8", "testb %r8b, 18(%rax)"),
        ("test_core_group_width_r16_mem_src", "testw 22(%rax), %r8w"),
        ("add_core_group_width_high8", "addb %ch, %dh"),
        ("xor_core_group_width_high8", "xorb %bh, %ah"),
        ("add_core_group_width_r8_imm8", "addb $0x7f, %r8b"),
        ("add_core_group_width_m8_imm8", "addb $0x10, 24(%rax)"),
        ("or_core_group_width_r8_imm8", "orb $0xf0, %r8b"),
        ("or_core_group_width_m8_imm8", "orb $0x0f, 25(%rax)"),
        ("adc_core_group_width_r16_imm8", "adcw $-1, %r8w"),
        ("adc_core_group_width_m16_imm16", "adcw $0x101, 26(%rax)"),
        ("sbb_core_group_width_r32_imm8", "sbbl $-2, %r8d"),
        (
            "sbb_core_group_width_m32_imm32",
            "sbbl $0x1020304, 28(%rax)",
        ),
        ("and_core_group_width_r16_imm16", "andw $0xff0, %r8w"),
        ("and_core_group_width_m16_imm8", "andw $0x7f, 32(%rax)"),
        ("sub_core_group_width_r8_imm8", "subb $0x20, %r8b"),
        ("sub_core_group_width_m32_imm8", "subl $0x20, 36(%rax)"),
        ("xor_core_group_width_r32_imm32", "xorl $0x55aa55aa, %r8d"),
        ("xor_core_group_width_m8_imm8", "xorb $0xaa, 40(%rax)"),
        ("cmp_core_group_width_r16_imm8", "cmpw $-1, %r8w"),
        ("cmp_core_group_width_m32_imm32", "cmpl $0x4000, 44(%rax)"),
        ("test_core_group_width_r8_imm8", "testb $0xf0, %r8b"),
        ("test_core_group_width_m8_imm8", "testb $0x0f, 48(%rax)"),
        ("test_core_group_width_r16_imm16", "testw $0xff0, %r8w"),
        ("test_core_group_width_m16_imm16", "testw $0x101, 50(%rax)"),
        ("test_core_group_width_r32_imm32", "testl $0xff00ff, %r8d"),
        (
            "test_core_group_width_m32_imm32",
            "testl $0x7f00ff00, 52(%rax)",
        ),
        ("not_core_group_width_r8", "notb %r8b"),
        ("not_core_group_width_m8", "notb 56(%rax)"),
        ("not_core_group_width_r16", "notw %r8w"),
        ("not_core_group_width_m16", "notw 58(%rax)"),
        ("not_core_group_width_r32", "notl %r8d"),
        ("not_core_group_width_m32", "notl 60(%rax)"),
        ("neg_core_group_width_r8", "negb %r8b"),
        ("neg_core_group_width_m8", "negb 64(%rax)"),
        ("neg_core_group_width_r16", "negw %r8w"),
        ("neg_core_group_width_m16", "negw 66(%rax)"),
        ("neg_core_group_width_r32", "negl %r8d"),
        ("neg_core_group_width_m32", "negl 68(%rax)"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // INC/DEC width variants. Byte forms dispatch through group 4 (0xFE),
    // including high-byte registers without REX; word/dword forms use group 5
    // with non-qword operand sizes.
    for &(label, asm) in &[
        (
            "inc_core_incdec_width_r8_overflow",
            "movb $0x7f, %r8b\nincb %r8b",
        ),
        (
            "dec_core_incdec_width_r8_overflow",
            "movb $0x80, %r8b\ndecb %r8b",
        ),
        ("inc_core_incdec_width_high8", "incb %ch"),
        ("dec_core_incdec_width_high8", "decb %dh"),
        (
            "inc_core_incdec_width_m8_overflow",
            "movb $0x7f, 2(%rax)\nincb 2(%rax)",
        ),
        (
            "dec_core_incdec_width_m8_overflow",
            "movb $0x80, 3(%rax)\ndecb 3(%rax)",
        ),
        ("inc_core_incdec_width_r16", "incw %r8w"),
        ("dec_core_incdec_width_r16", "decw %r8w"),
        ("inc_core_incdec_width_m16", "incw 4(%rax)"),
        ("dec_core_incdec_width_m16", "decw 6(%rax)"),
        ("inc_core_incdec_width_r32", "incl %r8d"),
        ("dec_core_incdec_width_r32", "decl %r8d"),
        ("inc_core_incdec_width_m32", "incl 8(%rax)"),
        ("dec_core_incdec_width_m32", "decl 12(%rax)"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // Core accumulator-immediate opcodes use dedicated legacy encodings
    // (04/05/0c/0d/.../a8/a9), not the ModRM group-1/group-3 immediate paths.
    for &(label, asm) in &[
        ("add_core_accum_al_imm8", "addb $0x7f, %al"),
        ("add_core_accum_ax_imm16", "addw $0x1234, %ax"),
        ("add_core_accum_eax_imm32", "addl $0x12345678, %eax"),
        ("add_core_accum_rax_imm32", "addq $-7, %rax"),
        ("adc_core_accum_al_imm8", "adcb $0x33, %al"),
        ("adc_core_accum_ax_imm16", "adcw $0x101, %ax"),
        ("adc_core_accum_eax_imm32", "adcl $0x10111213, %eax"),
        ("adc_core_accum_rax_imm32", "adcq $-9, %rax"),
        ("sbb_core_accum_al_imm8", "sbbb $0x11, %al"),
        ("sbb_core_accum_ax_imm16", "sbbw $0x20, %ax"),
        ("sbb_core_accum_eax_imm32", "sbbl $0x1020304, %eax"),
        ("sbb_core_accum_rax_imm32", "sbbq $-5, %rax"),
        ("sub_core_accum_al_imm8", "subb $0x55, %al"),
        ("sub_core_accum_ax_imm16", "subw $0x2222, %ax"),
        ("sub_core_accum_eax_imm32", "subl $0x1020304, %eax"),
        ("sub_core_accum_rax_imm32", "subq $-3, %rax"),
        ("cmp_core_accum_al_imm8", "cmpb $0, %al"),
        ("cmp_core_accum_ax_imm16", "cmpw $0x4000, %ax"),
        ("cmp_core_accum_eax_imm32", "cmpl $0x4000, %eax"),
        ("cmp_core_accum_rax_imm32", "cmpq $0x4000, %rax"),
        ("or_core_accum_al_imm8", "orb $0xf0, %al"),
        ("or_core_accum_ax_imm16", "orw $0xf0, %ax"),
        ("or_core_accum_eax_imm32", "orl $0xf0f0, %eax"),
        ("or_core_accum_rax_imm32", "orq $-4096, %rax"),
        ("and_core_accum_al_imm8", "andb $0xf0, %al"),
        ("and_core_accum_ax_imm16", "andw $0xff0, %ax"),
        ("and_core_accum_eax_imm32", "andl $0xff00ff, %eax"),
        ("and_core_accum_rax_imm32", "andq $-16, %rax"),
        ("xor_core_accum_al_imm8", "xorb $0xaa, %al"),
        ("xor_core_accum_ax_imm16", "xorw $0xaaaa, %ax"),
        ("xor_core_accum_eax_imm32", "xorl $0x55aa55aa, %eax"),
        ("xor_core_accum_rax_imm32", "xorq $-21846, %rax"),
        ("test_core_accum_al_imm8", "testb $0xf0, %al"),
        ("test_core_accum_ax_imm16", "testw $0xff0, %ax"),
        ("test_core_accum_eax_imm32", "testl $0xff00ff, %eax"),
        ("test_core_accum_rax_imm32", "testq $-16, %rax"),
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

    // Non-qword group-2 shift/rotate forms. These exercise the byte helper,
    // word/dword operand sizes, memory destinations with CL counts, and count
    // masking edge cases distinct from the baseline qword shift corpus above.
    for &(label, asm) in &[
        ("rol_core_shift_width_r8_one", "rolb $1, %r8b"),
        ("ror_core_shift_width_r8_one", "rorb $1, %r8b"),
        ("rcl_core_shift_width_r8_one", "rclb $1, %r8b"),
        ("rcr_core_shift_width_r8_one", "rcrb $1, %r8b"),
        ("shl_core_shift_width_r8_one", "shlb $1, %r8b"),
        ("shr_core_shift_width_r8_one", "shrb $1, %r8b"),
        ("sar_core_shift_width_r8_one", "sarb $1, %r8b"),
        ("rol_core_shift_width_r16_one", "rolw $1, %r8w"),
        ("ror_core_shift_width_r16_one", "rorw $1, %r8w"),
        ("rcl_core_shift_width_r16_one", "rclw $1, %r8w"),
        ("rcr_core_shift_width_r32_one", "rcrl $1, %r8d"),
        ("shl_core_shift_width_r16_imm3", "shlw $3, %r8w"),
        ("shr_core_shift_width_r16_imm5", "shrw $5, %r8w"),
        ("sar_core_shift_width_r16_imm7", "sarw $7, %r8w"),
        ("sal_core_shift_width_r32_imm4", "sall $4, %r8d"),
        ("shr_core_shift_width_r32_imm6", "shrl $6, %r8d"),
        ("sar_core_shift_width_r32_imm7", "sarl $7, %r8d"),
        ("shl_core_shift_width_m8_cl", "shlb %cl, 2(%rax)"),
        ("shr_core_shift_width_m16_cl", "shrw %cl, 4(%rax)"),
        ("sar_core_shift_width_m32_cl", "sarl %cl, 8(%rax)"),
        ("sal_core_shift_width_m64_cl", "salq %cl, 16(%rax)"),
        ("shl_core_shift_width_m8_imm3", "shlb $3, 24(%rax)"),
        ("shr_core_shift_width_m16_imm4", "shrw $4, 32(%rax)"),
        ("sar_core_shift_width_m32_imm5", "sarl $5, 40(%rax)"),
        ("shl_core_shift_width_r8_masked_count", "shlb $40, %r8b"),
        ("sar_core_shift_width_r8_saturating_count", "sarb $31, %r8b"),
        ("shl_core_shift_width_r64_zero_count", "shlq $64, %r8"),
        ("shr_core_shift_width_r64_masked_one", "shrq $65, %r8"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // Carry-rotate and double-shift edge cases. These pin down count masking,
    // rotate-through-carry periods, CL counts, memory destinations, and
    // SHLD/SHRD boundary counts where OF is undefined but the data result and
    // other status flags remain observable.
    for &(label, asm) in &[
        (
            "rcl_core_rotate_edge_r8_count8_cf_clear",
            "clc\nmovb $0x81, %r8b\nrclb $8, %r8b",
        ),
        (
            "rcr_core_rotate_edge_r8_count8_cf_set",
            "stc\nmovb $0x81, %r8b\nrcrb $8, %r8b",
        ),
        (
            "rcl_core_rotate_edge_r8_full_period",
            "stc\nmovb $0xa5, %r8b\nrclb $9, %r8b",
        ),
        (
            "rcr_core_rotate_edge_r16_full_period",
            "clc\nmovw $0x8001, %r8w\nrcrw $17, %r8w",
        ),
        (
            "rcl_core_rotate_edge_r16_count16",
            "stc\nmovw $0x8001, %r8w\nrclw $16, %r8w",
        ),
        (
            "rcr_core_rotate_edge_r16_count16",
            "clc\nmovw $0x8001, %r8w\nrcrw $16, %r8w",
        ),
        (
            "rcl_core_rotate_edge_r32_count31",
            "stc\nmovl $0x80000001, %r8d\nrcll $31, %r8d",
        ),
        (
            "rcr_core_rotate_edge_r32_count31",
            "clc\nmovl $0x80000001, %r8d\nrcrl $31, %r8d",
        ),
        (
            "rcl_core_rotate_edge_r64_count63",
            "stc\nmovabsq $0x8000000000000001, %r8\nrclq $63, %r8",
        ),
        (
            "rcr_core_rotate_edge_r64_count63",
            "clc\nmovabsq $0x8000000000000001, %r8\nrcrq $63, %r8",
        ),
        (
            "rcl_core_rotate_edge_m8_cl_count8",
            "stc\nmovb $8, %cl\nmovb $0x81, 32(%rax)\nrclb %cl, 32(%rax)",
        ),
        (
            "rcr_core_rotate_edge_m64_cl_count63",
            "clc\nmovb $63, %cl\nmovabsq $0x8000000000000001, %r8\nmovq %r8, 40(%rax)\nrcrq %cl, 40(%rax)",
        ),
        (
            "shld_core_double_shift_edge_w_imm16",
            "movw $0x8001, %r8w\nmovw $0x7ffe, %cx\nshldw $16, %cx, %r8w",
        ),
        (
            "shrd_core_double_shift_edge_w_imm16",
            "movw $0x8001, %r8w\nmovw $0x7ffe, %cx\nshrdw $16, %cx, %r8w",
        ),
        (
            "shld_core_double_shift_edge_l_imm31",
            "movl $0x80000001, %r8d\nmovl $0x7ffffffe, %ecx\nshldl $31, %ecx, %r8d",
        ),
        (
            "shrd_core_double_shift_edge_l_imm31",
            "movl $0x80000001, %r8d\nmovl $0x7ffffffe, %ecx\nshrdl $31, %ecx, %r8d",
        ),
        (
            "shld_core_double_shift_edge_q_imm63",
            "movabsq $0x8000000000000001, %r8\nmovabsq $0x7ffffffffffffffe, %rcx\nshldq $63, %rcx, %r8",
        ),
        (
            "shrd_core_double_shift_edge_q_imm63",
            "movabsq $0x8000000000000001, %r8\nmovabsq $0x7ffffffffffffffe, %rcx\nshrdq $63, %rcx, %r8",
        ),
        (
            "shld_core_double_shift_edge_m16_imm16",
            "movw $0x8001, 32(%rax)\nmovw $0x7ffe, %cx\nshldw $16, %cx, 32(%rax)",
        ),
        (
            "shrd_core_double_shift_edge_m64_cl_count63",
            "movb $63, %cl\nmovabsq $0x8000000000000001, %r8\nmovq %r8, 40(%rax)\nmovabsq $0x7ffffffffffffffe, %rdx\nshrdq %cl, %rdx, 40(%rax)",
        ),
        (
            "shld_core_double_shift_edge_q_zero_count",
            "movabsq $0x8000000000000001, %r8\nmovabsq $0x7ffffffffffffffe, %rcx\nshldq $64, %rcx, %r8",
        ),
        (
            "shrd_core_double_shift_edge_l_zero_count",
            "movl $0x80000001, %r8d\nmovl $0x7ffffffe, %ecx\nshrdl $32, %ecx, %r8d",
        ),
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

    // Core implicit-operand sign extension and group-3 multiply/divide width
    // forms. MUL/DIV/IDIV leave several status flags undefined, so those cases
    // finish with a deterministic CMP after exposing the implicit RAX/RDX
    // results.
    for &(label, asm) in &[
        (
            "cbw_core_implicit_sign_extend",
            "movabsq $0x7f80, %rax\ncbtw",
        ),
        (
            "cwde_core_implicit_sign_extend",
            "movabsq $0x8000, %rax\ncwtl",
        ),
        (
            "cdqe_core_implicit_sign_extend",
            "movabsq $0x80000001, %rax\ncltq",
        ),
        (
            "cwd_core_implicit_sign_extend",
            "movabsq $0x8001, %rax\nmovabsq $-1, %rdx\ncwtd",
        ),
        (
            "cdq_core_implicit_sign_extend",
            "movabsq $0x80000001, %rax\nmovabsq $-1, %rdx\ncltd",
        ),
        (
            "cqo_core_implicit_sign_extend",
            "movabsq $-5, %rax\nxorq %rdx, %rdx\ncqto",
        ),
        (
            "mul_core_implicit_r8_mem",
            "movb $0x12, %al\nmovb $0x11, 16(%rdi)\nmulb 16(%rdi)\ncmpq %r8, %r8",
        ),
        (
            "imul_core_implicit_r8_mem",
            "movb $-7, %al\nmovb $6, 17(%rdi)\nimulb 17(%rdi)\ncmpq %r8, %r8",
        ),
        (
            "mul_core_implicit_r16_mem",
            "movw $0x1234, %ax\nmovw $0x10, 18(%rdi)\nmulw 18(%rdi)\ncmpq %r8, %r8",
        ),
        (
            "imul_core_implicit_r16_mem",
            "movw $-1234, %ax\nmovw $7, 20(%rdi)\nimulw 20(%rdi)\ncmpq %r8, %r8",
        ),
        (
            "div_core_implicit_r8_mem",
            "movw $0x0123, %ax\nmovb $0x12, 16(%rdi)\ndivb 16(%rdi)\ncmpq %r8, %r8",
        ),
        (
            "idiv_core_implicit_r8_mem",
            "movw $-123, %ax\nmovb $-7, 17(%rdi)\nidivb 17(%rdi)\ncmpq %r8, %r8",
        ),
        (
            "div_core_implicit_r16_mem",
            "xorl %edx, %edx\nmovw $0x1234, %ax\nmovw $0x13, 18(%rdi)\ndivw 18(%rdi)\ncmpq %r8, %r8",
        ),
        (
            "idiv_core_implicit_r16_mem",
            "movw $-1234, %ax\ncwtd\nmovw $-13, 20(%rdi)\nidivw 20(%rdi)\ncmpq %r8, %r8",
        ),
        (
            "div_core_implicit_r32_mem",
            "xorl %edx, %edx\nmovl $0x12345678, %eax\nmovl $0x1234, 24(%rdi)\ndivl 24(%rdi)\ncmpq %r8, %r8",
        ),
        (
            "idiv_core_implicit_r32_mem",
            "movl $-12345678, %eax\ncltd\nmovl $-1234, 28(%rdi)\nidivl 28(%rdi)\ncmpq %r8, %r8",
        ),
        (
            "div_core_implicit_r64_mem",
            "xorq %rdx, %rdx\nmovabsq $0x123456789abcdef, %rax\nmovq $0x12345, 32(%rdi)\ndivq 32(%rdi)\ncmpq %r8, %r8",
        ),
        (
            "idiv_core_implicit_r64_mem",
            "movabsq $-1234567890123, %rax\ncqto\nmovq $-1234567, 40(%rdi)\nidivq 40(%rdi)\ncmpq %r8, %r8",
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Core,
            profile: Int,
        });
    }

    // Core multiply/divide edge forms. These keep quotient overflow and
    // divide-by-zero out of the corpus while stressing implicit RDX:RAX results,
    // signed overflow flags, exact quotients, and signed remainders.
    for &(label, asm) in &[
        (
            "mul_core_muldiv_edge_r8_no_overflow",
            "movb $0x12, %al\nmovb $0x03, 64(%rdi)\nmulb 64(%rdi)",
        ),
        (
            "mul_core_muldiv_edge_r8_overflow",
            "movb $0xff, %al\nmovb $0x02, 65(%rdi)\nmulb 65(%rdi)",
        ),
        (
            "mul_core_muldiv_edge_r16_no_overflow",
            "movw $0x1234, %ax\nmovw $0x0002, 66(%rdi)\nmulw 66(%rdi)",
        ),
        (
            "mul_core_muldiv_edge_r16_overflow",
            "movw $0xffff, %ax\nmovw $0x0002, 68(%rdi)\nmulw 68(%rdi)",
        ),
        (
            "mul_core_muldiv_edge_r32_no_overflow",
            "movl $0x12345678, %eax\nmovl $0x00000002, 72(%rdi)\nmull 72(%rdi)",
        ),
        (
            "mul_core_muldiv_edge_r32_overflow",
            "movl $0x80000000, %eax\nmovl $0x00000002, 76(%rdi)\nmull 76(%rdi)",
        ),
        (
            "mul_core_muldiv_edge_r64_no_overflow",
            "movabsq $0x123456789, %rax\nmovq $0x0000000000000002, 80(%rdi)\nmulq 80(%rdi)",
        ),
        (
            "mul_core_muldiv_edge_r64_overflow",
            "movabsq $0x8000000000000000, %rax\nmovq $0x0000000000000002, 88(%rdi)\nmulq 88(%rdi)",
        ),
        (
            "imul_core_muldiv_edge_r8_no_overflow",
            "movb $-4, %al\nmovb $8, 96(%rdi)\nimulb 96(%rdi)",
        ),
        (
            "imul_core_muldiv_edge_r8_overflow",
            "movb $0x40, %al\nmovb $4, 97(%rdi)\nimulb 97(%rdi)",
        ),
        (
            "imul_core_muldiv_edge_r16_no_overflow",
            "movw $-123, %ax\nmovw $4, 98(%rdi)\nimulw 98(%rdi)",
        ),
        (
            "imul_core_muldiv_edge_r16_overflow",
            "movw $0x4000, %ax\nmovw $4, 100(%rdi)\nimulw 100(%rdi)",
        ),
        (
            "imul_core_muldiv_edge_r32_no_overflow",
            "movl $-123456, %eax\nmovl $17, 104(%rdi)\nimull 104(%rdi)",
        ),
        (
            "imul_core_muldiv_edge_r32_overflow",
            "movl $0x40000000, %eax\nmovl $4, 108(%rdi)\nimull 108(%rdi)",
        ),
        (
            "imul_core_muldiv_edge_r64_no_overflow",
            "movabsq $-123456789, %rax\nmovq $1000, 112(%rdi)\nimulq 112(%rdi)",
        ),
        (
            "imul_core_muldiv_edge_r64_overflow",
            "movabsq $0x4000000000000000, %rax\nmovq $4, 120(%rdi)\nimulq 120(%rdi)",
        ),
        (
            "imul_core_muldiv_edge_r64_two_operand_no_overflow",
            "movq $-9, %r8\nmovq $8, %rcx\nimulq %rcx, %r8",
        ),
        (
            "imul_core_muldiv_edge_r64_two_operand_overflow",
            "movabsq $0x4000000000000000, %r8\nmovq $4, %rcx\nimulq %rcx, %r8",
        ),
        (
            "imul_core_muldiv_edge_r32_three_operand_no_overflow",
            "movl $123456, %ecx\nimull $-17, %ecx, %r8d",
        ),
        (
            "imul_core_muldiv_edge_r32_three_operand_overflow",
            "movl $0x40000000, %ecx\nimull $4, %ecx, %r8d",
        ),
        (
            "div_core_muldiv_edge_r8_exact",
            "movw $0x00f0, %ax\nmovb $0x10, 128(%rdi)\ndivb 128(%rdi)",
        ),
        (
            "div_core_muldiv_edge_r8_max_quotient",
            "movw $0xfe01, %ax\nmovb $0xff, 129(%rdi)\ndivb 129(%rdi)",
        ),
        (
            "div_core_muldiv_edge_r16_remainder",
            "xorl %edx, %edx\nmovw $0xffff, %ax\nmovw $1000, 130(%rdi)\ndivw 130(%rdi)",
        ),
        (
            "div_core_muldiv_edge_r32_high_half",
            "movl $1, %edx\nxorl %eax, %eax\nmovl $0x10000, 132(%rdi)\ndivl 132(%rdi)",
        ),
        (
            "div_core_muldiv_edge_r64_large_exact",
            "xorq %rdx, %rdx\nmovq $-1, %rax\nmovabsq $0xffffffff, %r8\nmovq %r8, 136(%rdi)\ndivq 136(%rdi)",
        ),
        (
            "div_core_muldiv_edge_r64_remainder",
            "xorq %rdx, %rdx\nmovabsq $0x123456789abcdef0, %rax\nmovabsq $0x100000001, %r8\nmovq %r8, 144(%rdi)\ndivq 144(%rdi)",
        ),
        (
            "idiv_core_muldiv_edge_r8_negative_remainder",
            "movw $-127, %ax\nmovb $-3, 152(%rdi)\nidivb 152(%rdi)",
        ),
        (
            "idiv_core_muldiv_edge_r16_negative_remainder",
            "movw $-32768, %ax\ncwtd\nmovw $-7, 154(%rdi)\nidivw 154(%rdi)",
        ),
        (
            "idiv_core_muldiv_edge_r32_min_divisor",
            "movl $0x80000000, %eax\ncltd\nmovl $7, 156(%rdi)\nidivl 156(%rdi)",
        ),
        (
            "idiv_core_muldiv_edge_r64_min_divisor",
            "movabsq $0x8000000000000000, %rax\ncqto\nmovq $2, 160(%rdi)\nidivq 160(%rdi)",
        ),
        (
            "idiv_core_muldiv_edge_r64_negative_divisor",
            "movabsq $-1234567890123, %rax\ncqto\nmovq $-12345, 168(%rdi)\nidivq 168(%rdi)",
        ),
        (
            "idiv_core_muldiv_edge_r64_positive_remainder",
            "movabsq $1234567890123, %rax\ncqto\nmovq $-12345, 176(%rdi)\nidivq 176(%rdi)",
        ),
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
        (
            "addr32_movsb_core_string_edge_addr32_high",
            "addr32 movsb",
        ),
        (
            "addr32_rep_movsq_core_string_edge_addr32_high_count_one",
            "addr32 rep movsq",
        ),
        (
            "rep_movsb_core_string_edge_overlap_forward",
            "rep movsb",
        ),
        (
            "rep_movsb_core_string_edge_overlap_backward",
            "rep movsb",
        ),
        ("rep_movsq_core_string_edge_count_one", "rep movsq"),
        ("rep_stosb_core_string_edge_count_one", "rep stosb"),
        ("rep_stosq_core_string_edge_count_one", "rep stosq"),
        ("rep_lodsw_core_string_edge_count_one", "rep lodsw"),
        (
            "repe_cmpsb_core_string_edge_same_address",
            "repe cmpsb",
        ),
        (
            "repne_cmpsb_core_string_edge_same_address_count_one",
            "repne cmpsb",
        ),
        ("repe_scasb_core_string_edge_count_one", "repe scasb"),
        ("repne_scasq_core_string_edge_count_one", "repne scasq"),
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

    for &(label, asm) in &[
        ("crc32_crc32_operand_r32_cl", "crc32b %cl, %r8d"),
        ("crc32_crc32_operand_r32_r9b", "crc32b %r9b, %r8d"),
        ("crc32_crc32_operand_r64_r9b", "crc32b %r9b, %r8"),
        ("crc32_crc32_operand_r32_r9w", "crc32w %r9w, %r8d"),
        ("crc32_crc32_operand_r32_r9d", "crc32l %r9d, %r8d"),
        ("crc32_crc32_operand_r64_r9", "crc32q %r9, %r8"),
        ("crc32_crc32_operand_r32_m8_disp", "crc32b 16(%rax), %r8d"),
        ("crc32_crc32_operand_r64_m64_disp", "crc32q 24(%rax), %r8"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Crc32,
            profile: Int,
        });
    }

    for &(label, asm) in &[
        ("crc32_crc32_edge_same_r32_b", "crc32b %r8b, %r8d"),
        ("crc32_crc32_edge_same_r32_w", "crc32w %r8w, %r8d"),
        ("crc32_crc32_edge_same_r32_l", "crc32l %r8d, %r8d"),
        ("crc32_crc32_edge_same_r64_b", "crc32b %r8b, %r8"),
        ("crc32_crc32_edge_same_r64_q", "crc32q %r8, %r8"),
        ("crc32_crc32_edge_high8_src", "crc32b %ch, %edx"),
        ("crc32_crc32_edge_r32_src_high_reg", "crc32l %r9d, %r8d"),
        ("crc32_crc32_edge_r64_src_high_reg", "crc32q %r9, %r8"),
        ("crc32_crc32_edge_m8_unaligned", "crc32b 7(%rax), %r8d"),
        ("crc32_crc32_edge_m64_unaligned", "crc32q 9(%rax), %r8"),
        ("crc32_crc32_edge_dest_r9d", "crc32l %r8d, %r9d"),
        ("crc32_crc32_edge_dest_r9", "crc32q %r8, %r9"),
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

    // Complementary BMI1 width/source forms.
    for &(label, asm) in &[
        ("andn_bmi1_operand_r32_reg", "andnl %ecx, %r8d, %r8d"),
        ("andn_bmi1_operand_r64_mem", "andnq 32(%rax), %r8, %r8"),
        ("bextr_bmi1_operand_r32_reg", "bextrl %ecx, %r8d, %r8d"),
        ("bextr_bmi1_operand_r64_mem", "bextrq %rcx, 32(%rax), %r8"),
        ("blsi_bmi1_operand_r32_reg", "blsil %ecx, %r8d"),
        ("blsi_bmi1_operand_r64_mem", "blsiq 32(%rax), %r8"),
        ("blsr_bmi1_operand_r32_reg", "blsrl %ecx, %r8d"),
        ("blsr_bmi1_operand_r64_mem", "blsrq 32(%rax), %r8"),
        ("blsmsk_bmi1_operand_r32_reg", "blsmskl %ecx, %r8d"),
        ("blsmsk_bmi1_operand_r64_mem", "blsmskq 32(%rax), %r8"),
        ("tzcnt_bmi1_operand_r32_reg", "tzcntl %ecx, %r8d"),
        ("tzcnt_bmi1_operand_r64_mem", "tzcntq 32(%rax), %r8"),
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

    // Complementary BMI2 width/source forms, including the alternate MULX
    // source shape.
    for &(label, asm) in &[
        ("bzhi_bmi2_operand_r32_reg", "bzhil %ecx, %r8d, %r8d"),
        ("bzhi_bmi2_operand_r64_mem", "bzhiq %rcx, 32(%rax), %r8"),
        ("pdep_bmi2_operand_r32_reg", "pdepl %ecx, %r8d, %r8d"),
        ("pdep_bmi2_operand_r64_mem", "pdepq 32(%rax), %r8, %r8"),
        ("pext_bmi2_operand_r32_reg", "pextl %ecx, %r8d, %r8d"),
        ("pext_bmi2_operand_r64_mem", "pextq 32(%rax), %r8, %r8"),
        ("mulx_bmi2_operand_r32_mem", "mulxl 32(%rax), %r8d, %ecx"),
        ("mulx_bmi2_operand_r64_reg", "mulxq %r8, %r9, %rcx"),
        ("rorx_bmi2_operand_r32_reg", "rorxl $7, %ecx, %r8d"),
        ("rorx_bmi2_operand_r64_mem", "rorxq $11, 32(%rax), %r8"),
        ("sarx_bmi2_operand_r32_reg", "sarxl %ecx, %r8d, %r8d"),
        ("sarx_bmi2_operand_r64_mem", "sarxq %rcx, 32(%rax), %r8"),
        ("shrx_bmi2_operand_r32_reg", "shrxl %ecx, %r8d, %r8d"),
        ("shrx_bmi2_operand_r64_mem", "shrxq %rcx, 32(%rax), %r8"),
        ("shlx_bmi2_operand_r32_reg", "shlxl %ecx, %r8d, %r8d"),
        ("shlx_bmi2_operand_r64_mem", "shlxq %rcx, 32(%rax), %r8"),
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

    for &(label, asm) in &[
        ("lzcnt_operand_r32_reg", "lzcntl %ecx, %r8d"),
        ("lzcnt_operand_r64_mem", "lzcntq 32(%rax), %r8"),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat: Lzcnt,
            profile: Int,
        });
    }

    // Scalar bit-count/manipulation edge cases. These deliberately force zero
    // sources, boundary counts, and all-one masks so the differential harness
    // checks flag semantics and count masking beyond the ordinary operand-form
    // coverage above.
    for &(label, asm, feat) in &[
        (
            "popcnt_scalar_bit_edge_zero_r64",
            "xorq %rcx, %rcx\npopcntq %rcx, %r8",
            Popcnt,
        ),
        (
            "popcnt_scalar_bit_edge_allones_r64",
            "movq $-1, %rcx\npopcntq %rcx, %r8",
            Popcnt,
        ),
        (
            "popcnt_scalar_bit_edge_zero_m32",
            "movl $0, 32(%rax)\npopcntl 32(%rax), %r8d",
            Popcnt,
        ),
        (
            "popcnt_scalar_bit_edge_highbit_m16",
            "movw $0x8000, 32(%rax)\npopcntw 32(%rax), %r8w",
            Popcnt,
        ),
        (
            "tzcnt_scalar_bit_edge_zero_r64",
            "xorq %rcx, %rcx\ntzcntq %rcx, %r8",
            Bmi1,
        ),
        (
            "tzcnt_scalar_bit_edge_lowbit_r64",
            "movq $1, %rcx\ntzcntq %rcx, %r8",
            Bmi1,
        ),
        (
            "andn_scalar_bit_edge_allones_r64",
            "movq $-1, %rcx\nandnq %rcx, %r8, %r8",
            Bmi1,
        ),
        (
            "bextr_scalar_bit_edge_zero_len_r32",
            "movl $0, %ecx\nbextrl %ecx, %r8d, %r8d",
            Bmi1,
        ),
        (
            "bextr_scalar_bit_edge_start_past_r32",
            "movl $0x0528, %ecx\nbextrl %ecx, %r8d, %r8d",
            Bmi1,
        ),
        (
            "blsi_scalar_bit_edge_zero_r64",
            "xorq %rcx, %rcx\nblsiq %rcx, %r8",
            Bmi1,
        ),
        (
            "blsi_scalar_bit_edge_single_r64",
            "movq $0x1000, %rcx\nblsiq %rcx, %r8",
            Bmi1,
        ),
        (
            "blsr_scalar_bit_edge_zero_r64",
            "xorq %rcx, %rcx\nblsrq %rcx, %r8",
            Bmi1,
        ),
        (
            "blsmsk_scalar_bit_edge_zero_r64",
            "xorq %rcx, %rcx\nblsmskq %rcx, %r8",
            Bmi1,
        ),
        (
            "lzcnt_scalar_bit_edge_zero_r64",
            "xorq %rcx, %rcx\nlzcntq %rcx, %r8",
            Lzcnt,
        ),
        (
            "lzcnt_scalar_bit_edge_highbit_r64",
            "movabsq $0x8000000000000000, %rcx\nlzcntq %rcx, %r8",
            Lzcnt,
        ),
        (
            "lzcnt_scalar_bit_edge_zero_m32",
            "movl $0, 32(%rax)\nlzcntl 32(%rax), %r8d",
            Lzcnt,
        ),
        (
            "lzcnt_scalar_bit_edge_highbit_m32",
            "movl $0x80000000, 32(%rax)\nlzcntl 32(%rax), %r8d",
            Lzcnt,
        ),
        (
            "bzhi_scalar_bit_edge_zero_index_r32",
            "movl $0, %ecx\nbzhil %ecx, %r8d, %r8d",
            Bmi2,
        ),
        (
            "bzhi_scalar_bit_edge_width_index_r32",
            "movl $32, %ecx\nbzhil %ecx, %r8d, %r8d",
            Bmi2,
        ),
        (
            "pdep_scalar_bit_edge_zero_selector_r64",
            "xorq %rcx, %rcx\npdepq %rcx, %r8, %r8",
            Bmi2,
        ),
        (
            "pdep_scalar_bit_edge_zero_source_r64",
            "xorq %r8, %r8\npdepq %rcx, %r8, %r9",
            Bmi2,
        ),
        (
            "pext_scalar_bit_edge_zero_selector_r64",
            "xorq %rcx, %rcx\npextq %rcx, %r8, %r8",
            Bmi2,
        ),
        (
            "pext_scalar_bit_edge_zero_source_r64",
            "xorq %r8, %r8\npextq %rcx, %r8, %r9",
            Bmi2,
        ),
        (
            "mulx_scalar_bit_edge_r64_max_product",
            "movq $-1, %rdx\nmovq $-1, %r8\nmulxq %r8, %r9, %rcx",
            Bmi2,
        ),
        (
            "mulx_scalar_bit_edge_r32_max_product",
            "movl $-1, %edx\nmovl $-1, %r8d\nmulxl %r8d, %r9d, %ecx",
            Bmi2,
        ),
        (
            "mulx_scalar_bit_edge_zero_rdx_r64",
            "xorq %rdx, %rdx\nmulxq %r8, %r9, %rcx",
            Bmi2,
        ),
        (
            "mulx_scalar_bit_edge_same_dest_r32",
            "movl $-1, %edx\nmovl $-1, %ecx\nmulxl %ecx, %r8d, %r8d",
            Bmi2,
        ),
        (
            "mulx_scalar_bit_edge_memory_max_r64",
            "movq $-1, %rdx\nmovq $-1, 40(%rax)\nmulxq 40(%rax), %r9, %rcx",
            Bmi2,
        ),
        (
            "mulx_scalar_bit_edge_sequential_r64",
            "movq $10, %rdx\nmovq $5, %rcx\nmulxq %rcx, %r8, %r9\nmovq %r8, %rdx\nmulxq %rcx, %r8, %r9",
            Bmi2,
        ),
        (
            "mulx_scalar_bit_edge_preserves_cmp_flags",
            "cmpq %r8, %r8\nmulxq %r8, %r9, %rcx",
            Bmi2,
        ),
        (
            "rorx_scalar_bit_edge_zero_count_r64",
            "rorxq $0, %rcx, %r8",
            Bmi2,
        ),
        (
            "rorx_scalar_bit_edge_max_count_r64",
            "rorxq $63, %rcx, %r8",
            Bmi2,
        ),
        (
            "sarx_scalar_bit_edge_masked_count_r64",
            "movl $64, %ecx\nsarxq %rcx, %r8, %r8",
            Bmi2,
        ),
        (
            "shrx_scalar_bit_edge_masked_count_r64",
            "movl $64, %ecx\nshrxq %rcx, %r8, %r8",
            Bmi2,
        ),
        (
            "shlx_scalar_bit_edge_masked_count_r32",
            "movl $32, %ecx\nshlxl %ecx, %r8d, %r8d",
            Bmi2,
        ),
    ] {
        out.push(Case {
            label: label.to_string(),
            asm: asm.to_string(),
            feat,
            profile: Int,
        });
    }
    for &(label, asm, feat) in &[
        (
            "rdrand_random_edge_r15w_preserves_upper",
            "movabsq $0x0123456789abcdef, %r15\n1:\nrdrand %r15w\njnc 1b\nmovw $0, %r15w",
            Rdrand,
        ),
        (
            "rdrand_random_edge_r15d_zeroext",
            "movabsq $-1, %r15\n1:\nrdrand %r15d\njnc 1b\nmovabsq $0xffffffff00000000, %r10\ntestq %r10, %r15\njz 2f\nmovq $1, %r15\njmp 3f\n2:\nmovq $0, %r15\n3:\naddq $0, %r15",
            Rdrand,
        ),
        (
            "rdseed_random_edge_r15w_preserves_upper",
            "movabsq $0xfedcba9876543210, %r15\n1:\nrdseed %r15w\njnc 1b\nmovw $0, %r15w",
            Rdseed,
        ),
        (
            "rdseed_random_edge_r15d_zeroext",
            "movabsq $-1, %r15\n1:\nrdseed %r15d\njnc 1b\nmovabsq $0xffffffff00000000, %r10\ntestq %r10, %r15\njz 2f\nmovq $1, %r15\njmp 3f\n2:\nmovq $0, %r15\n3:\naddq $0, %r15",
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

    if case.label.contains("_core_muldiv_edge_") {
        if case.label.starts_with("div_") || case.label.starts_with("idiv_") {
            return 0;
        }
        return RFLAGS_CF | RFLAGS_OF;
    }

    if case.label.contains("_scalar_bit_edge_") {
        if case.label.starts_with("popcnt_") {
            return STATUS_RFLAGS_MASK;
        }
        if case.label.starts_with("tzcnt_") || case.label.starts_with("lzcnt_") {
            return RFLAGS_CF | RFLAGS_ZF;
        }
        if case.label.starts_with("bextr_") {
            return RFLAGS_CF | RFLAGS_ZF | RFLAGS_OF;
        }
        if case.label.starts_with("andn_")
            || case.label.starts_with("blsi_")
            || case.label.starts_with("blsr_")
            || case.label.starts_with("blsmsk_")
            || case.label.starts_with("bzhi_")
        {
            return RFLAGS_CF | RFLAGS_ZF | RFLAGS_SF | RFLAGS_OF;
        }
    }

    if case.label.contains("_core_bit_width_") {
        if case.label.starts_with("bsf_") || case.label.starts_with("bsr_") {
            return RFLAGS_ZF;
        }
        return RFLAGS_CF;
    }

    if case.label.contains("_sse42_string_width_") {
        return RFLAGS_CF | RFLAGS_ZF | RFLAGS_SF | RFLAGS_OF;
    }

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

    if case.label.contains("_core_rotate_edge_") {
        if case.label.contains("_full_period") {
            return STATUS_RFLAGS_MASK;
        }
        return STATUS_RFLAGS_MASK & !RFLAGS_OF;
    }

    if case.label.contains("_core_double_shift_edge_") {
        if case.label.contains("_zero_count") {
            return STATUS_RFLAGS_MASK;
        }
        return RFLAGS_CF | RFLAGS_PF | RFLAGS_ZF | RFLAGS_SF;
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
                | Feat::Xgetbv1
                | Feat::XsaveExt
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
                | Feat::Invpcid
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
                    | Feat::Sse3
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
        let sse42_string_setup_allowed = case.label.contains("_sse42_string_width_")
            && op.windows(2).any(|bytes| bytes == [0x0f, 0x3a]);
        let scalar_bit_edge_setup_allowed = case.label.contains("_scalar_bit_edge_")
            && (op.windows(2).any(|bytes| bytes[0] == 0x0f)
                || op.iter().any(|byte| matches!(byte, 0xc4 | 0xc5)));
        let movdir_edge_setup_allowed = case.label.contains("_movdir_edge_")
            && op
                .windows(3)
                .any(|bytes| {
                    bytes[0] == 0x0f && bytes[1] == 0x38 && matches!(bytes[2], 0xf8 | 0xf9)
                });
        let adx_edge_setup_allowed = case.label.contains("_adx_edge_")
            && op
                .windows(3)
                .any(|bytes| bytes[0] == 0x0f && bytes[1] == 0x38 && bytes[2] == 0xf6);
        let avx2_gather_edge_setup_allowed = case.label.contains("_avx2_gather_edge_")
            && op
                .windows(4)
                .any(|bytes| bytes[0] == 0xc4 && matches!(bytes[3], 0x90..=0x93));
        let addr32_vex_allowed = matches!(op.first(), Some(0x67))
            && matches!(op.get(1), Some(0x62) | Some(0xc4) | Some(0xc5));
        let expected_encoding = matches!(op.first(), Some(0x62) | Some(0xC4) | Some(0xC5))
            || legacy_allowed
            || sse42_string_setup_allowed
            || scalar_bit_edge_setup_allowed
            || movdir_edge_setup_allowed
            || adx_edge_setup_allowed
            || avx2_gather_edge_setup_allowed
            || addr32_vex_allowed;
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

fn legacy_invalid_long_mode_cases() -> Vec<(&'static str, &'static [u8])> {
    vec![
        ("push_es_invalid_long", &[0x06]),
        ("pop_es_invalid_long", &[0x07]),
        ("push_cs_invalid_long", &[0x0e]),
        ("push_ss_invalid_long", &[0x16]),
        ("pop_ss_invalid_long", &[0x17]),
        ("push_ds_invalid_long", &[0x1e]),
        ("pop_ds_invalid_long", &[0x1f]),
        ("daa_invalid_long", &[0x27]),
        ("das_invalid_long", &[0x2f]),
        ("aaa_invalid_long", &[0x37]),
        ("aas_invalid_long", &[0x3f]),
        ("pushad_invalid_long", &[0x60]),
        ("popad_invalid_long", &[0x61]),
        ("pusha_invalid_long", &[0x66, 0x60]),
        ("popa_invalid_long", &[0x66, 0x61]),
        ("into_invalid_long", &[0xce]),
        ("aam_invalid_long", &[0xd4, 0x0a]),
    ]
}

fn illegal_lock_prefix_cases() -> Vec<(&'static str, &'static [u8])> {
    vec![
        ("lock_nop_illegal", &[0xf0, 0x90]),
        ("lock_pause_illegal", &[0xf0, 0xf3, 0x90]),
        ("lock_mov_reg_illegal", &[0xf0, 0x48, 0x89, 0xc8]),
        ("lock_lea_mem_illegal", &[0xf0, 0x48, 0x8d, 0x08]),
        ("lock_add_reg_dest_illegal", &[0xf0, 0x48, 0x01, 0xc8]),
        ("lock_or_reg_imm_illegal", &[0xf0, 0x48, 0x83, 0xc8, 0x01]),
        ("lock_cmp_mem_imm_illegal", &[0xf0, 0x83, 0x38, 0x01]),
        (
            "lock_test_mem_imm_illegal",
            &[0xf0, 0xf7, 0x00, 0x01, 0x00, 0x00, 0x00],
        ),
        ("lock_imul_mem_illegal", &[0xf0, 0xf7, 0x28]),
        ("lock_inc_reg_illegal", &[0xf0, 0xff, 0xc0]),
        ("lock_push_mem_illegal", &[0xf0, 0xff, 0x30]),
        ("lock_xchg_reg_illegal", &[0xf0, 0x48, 0x87, 0xc8]),
        ("lock_bt_mem_imm_illegal", &[0xf0, 0x0f, 0xba, 0x20, 0x01]),
        ("lock_bts_reg_illegal", &[0xf0, 0x0f, 0xab, 0xc8]),
        ("lock_cmpxchg8b_reg_illegal", &[0xf0, 0x0f, 0xc7, 0xc8]),
    ]
}

fn undefined_opcode_cases() -> Vec<(&'static str, &'static [u8])> {
    vec![
        ("ud2_explicit", &[0x0f, 0x0b]),
        ("ud1_reg_explicit", &[0x0f, 0xb9, 0xc0]),
        ("ud1_mem_explicit", &[0x0f, 0xb9, 0x00]),
        ("ud0_reg_explicit", &[0x0f, 0xff, 0xc0]),
        ("ud0_mem_explicit", &[0x0f, 0xff, 0x00]),
        ("undefined_primary_d6", &[0xd6]),
        ("undefined_two_byte_04", &[0x0f, 0x04]),
        ("group4_fe_reg_undefined", &[0xfe, 0xd0]),
        ("group4_fe_mem_undefined", &[0xfe, 0x10]),
        ("group5_ff_reg_undefined", &[0xff, 0xf8]),
        ("group5_ff_mem_undefined", &[0xff, 0x38]),
    ]
}

fn invalid_extension_encoding_cases() -> Vec<(&'static str, &'static [u8])> {
    vec![
        (
            "movdiri_66_prefix_illegal",
            &[0x66, 0x0f, 0x38, 0xf9, 0x08],
        ),
        ("movdiri_register_dest_illegal", &[0x0f, 0x38, 0xf9, 0xc8]),
        (
            "movdir64b_missing_66_illegal",
            &[0x0f, 0x38, 0xf8, 0x08],
        ),
        (
            "movdir64b_register_source_illegal",
            &[0x66, 0x0f, 0x38, 0xf8, 0xc3],
        ),
        (
            "rex_before_evex_illegal",
            &[0x41, 0x62, 0x91, 0x7c, 0x08, 0x28, 0xc0],
        ),
        (
            "rex2_before_evex_illegal",
            &[0xd5, 0x01, 0x62, 0x91, 0x7c, 0x08, 0x28, 0xc0],
        ),
        (
            "kunpckbw_vvvv_k8_illegal",
            &[0xc5, 0xbd, 0x4b, 0xc0],
        ),
        ("kandw_vvvv_k8_illegal", &[0xc5, 0xbc, 0x41, 0xc0]),
        (
            "kmovw_store_reg_k8_illegal",
            &[0xc5, 0x78, 0x91, 0x00],
        ),
        (
            "kortestw_vvvv_illegal",
            &[0xc5, 0xf0, 0x98, 0xd1],
        ),
        (
            "ktestw_vvvv_illegal",
            &[0xc5, 0xf0, 0x99, 0xd1],
        ),
        (
            "kshiftlw_vvvv_illegal",
            &[0xc4, 0xe3, 0xf1, 0x32, 0xd1, 0x03],
        ),
        (
            "kortestw_memory_illegal",
            &[0xc5, 0xf8, 0x98, 0x10],
        ),
        (
            "ktestw_memory_illegal",
            &[0xc5, 0xf8, 0x99, 0x10],
        ),
        (
            "kandw_memory_illegal",
            &[0xc5, 0xec, 0x41, 0x18],
        ),
        (
            "knotw_memory_illegal",
            &[0xc5, 0xf8, 0x44, 0x10],
        ),
        (
            "kunpckbw_memory_illegal",
            &[0xc5, 0xed, 0x4b, 0x18],
        ),
        (
            "kshiftlw_memory_illegal",
            &[0xc4, 0xe3, 0xf9, 0x32, 0x10, 0x03],
        ),
        (
            "vtestps_vvvv_illegal",
            &[0xc4, 0xe2, 0x71, 0x0e, 0xd1],
        ),
        (
            "vptest_vvvv_illegal",
            &[0xc4, 0xe2, 0x71, 0x17, 0xd1],
        ),
        (
            "vpmovmskb_vvvv_illegal",
            &[0xc5, 0xf1, 0xd7, 0xc1],
        ),
        (
            "vcvtph2ps_vvvv_illegal",
            &[0xc4, 0xe2, 0x71, 0x13, 0xd1],
        ),
        (
            "vcvtps2ph_vvvv_illegal",
            &[0xc4, 0xe3, 0x71, 0x1d, 0xca, 0x00],
        ),
        (
            "vphminposuw_vvvv_illegal",
            &[0xc4, 0xe2, 0x71, 0x41, 0xd1],
        ),
        (
            "vphminposuw_l1_illegal",
            &[0xc4, 0xe2, 0x7d, 0x41, 0xd1],
        ),
        (
            "vmovss_load_vvvv_illegal",
            &[0xc5, 0xf2, 0x10, 0x08],
        ),
        (
            "vmovss_store_vvvv_illegal",
            &[0xc5, 0xf2, 0x11, 0x08],
        ),
        (
            "vmovlps_l1_illegal",
            &[0xc5, 0xf4, 0x12, 0x10],
        ),
        (
            "vmovlpd_l1_illegal",
            &[0xc5, 0xf5, 0x12, 0x10],
        ),
        (
            "vmovlpd_register_source_illegal",
            &[0xc5, 0xf1, 0x12, 0xd1],
        ),
        (
            "vmovlps_store_l1_illegal",
            &[0xc5, 0xfc, 0x13, 0x08],
        ),
        (
            "vmovlps_store_vvvv_illegal",
            &[0xc5, 0xf0, 0x13, 0x08],
        ),
        (
            "vmovlps_store_register_dest_illegal",
            &[0xc5, 0xf8, 0x13, 0xc8],
        ),
        (
            "vmovsldup_vvvv_illegal",
            &[0xc5, 0xf2, 0x12, 0xd1],
        ),
        (
            "vmovshdup_vvvv_illegal",
            &[0xc5, 0xf2, 0x16, 0xd1],
        ),
        (
            "vmovddup_vvvv_illegal",
            &[0xc5, 0xf3, 0x12, 0xd1],
        ),
        (
            "vmovntps_vvvv_illegal",
            &[0xc5, 0xf0, 0x2b, 0x08],
        ),
        (
            "vmovntps_register_dest_illegal",
            &[0xc5, 0xf8, 0x2b, 0xc8],
        ),
        (
            "vmovntdqa_vvvv_illegal",
            &[0xc4, 0xe2, 0x71, 0x2a, 0x10],
        ),
        (
            "vmovntdqa_register_source_illegal",
            &[0xc4, 0xe2, 0x79, 0x2a, 0xd0],
        ),
        (
            "vbroadcastf128_vvvv_illegal",
            &[0xc4, 0xe2, 0x75, 0x1a, 0x10],
        ),
        (
            "vbroadcastf128_l0_illegal",
            &[0xc4, 0xe2, 0x79, 0x1a, 0x10],
        ),
        (
            "vbroadcastf128_register_source_illegal",
            &[0xc4, 0xe2, 0x7d, 0x1a, 0xd0],
        ),
        (
            "vbroadcastss_vvvv_illegal",
            &[0xc4, 0xe2, 0x71, 0x18, 0x10],
        ),
        (
            "vbroadcastsd_vvvv_illegal",
            &[0xc4, 0xe2, 0x75, 0x19, 0x10],
        ),
        (
            "vbroadcastsd_l0_illegal",
            &[0xc4, 0xe2, 0x79, 0x19, 0x10],
        ),
        (
            "vpermilps_imm_vvvv_illegal",
            &[0xc4, 0xe3, 0x71, 0x04, 0xd1, 0x00],
        ),
        (
            "vpermilpd_imm_vvvv_illegal",
            &[0xc4, 0xe3, 0x71, 0x05, 0xd1, 0x00],
        ),
        (
            "vex_0f70_missing_mandatory_prefix_illegal",
            &[0xc5, 0xf8, 0x70, 0xc1, 0x00],
        ),
        (
            "vcvtps2dq_f2_prefix_illegal",
            &[0xc5, 0xfb, 0x5b, 0xc1],
        ),
        (
            "vcvttpd2dq_missing_mandatory_prefix_illegal",
            &[0xc5, 0xf8, 0xe6, 0xc1],
        ),
        (
            "vcomiss_evex_mask_illegal",
            &[0x62, 0xf1, 0x7c, 0x09, 0x2f, 0xca],
        ),
        (
            "vucomiss_evex_zero_illegal",
            &[0x62, 0xf1, 0x7c, 0x88, 0x2e, 0xca],
        ),
        (
            "vcomisd_evex_mask_illegal",
            &[0x62, 0xf1, 0xfd, 0x09, 0x2f, 0xca],
        ),
        (
            "vucomisd_evex_zero_illegal",
            &[0x62, 0xf1, 0xfd, 0x88, 0x2e, 0xca],
        ),
        (
            "vcomiss_evex_memory_b_illegal",
            &[0x62, 0xf1, 0x7c, 0x18, 0x2f, 0x08],
        ),
        (
            "vcomisd_evex_memory_b_illegal",
            &[0x62, 0xf1, 0xfd, 0x18, 0x2f, 0x08],
        ),
        (
            "vcmpps_evex_zero_illegal",
            &[0x62, 0xf1, 0x64, 0xc8, 0xc2, 0xea, 0x01],
        ),
        (
            "vcmpss_evex_zero_illegal",
            &[0x62, 0xf1, 0x66, 0x88, 0xc2, 0xea, 0x01],
        ),
        (
            "vfpclassps_evex_zero_illegal",
            &[0x62, 0xf3, 0x7d, 0xc8, 0x66, 0xea, 0x03],
        ),
        (
            "vfpclassps_evex_vvvv_illegal",
            &[0x62, 0xf3, 0x75, 0x48, 0x66, 0xea, 0x03],
        ),
        (
            "vfpclassss_evex_zero_illegal",
            &[0x62, 0xf3, 0x7d, 0x88, 0x67, 0xea, 0x03],
        ),
        (
            "vfpclassss_evex_vvvv_illegal",
            &[0x62, 0xf3, 0x75, 0x08, 0x67, 0xea, 0x03],
        ),
        (
            "vcvtss2si_evex_mask_illegal",
            &[0x62, 0x71, 0xfe, 0x09, 0x2d, 0xc3],
        ),
        (
            "vcvtss2si_evex_zero_illegal",
            &[0x62, 0x71, 0xfe, 0x88, 0x2d, 0xc3],
        ),
        (
            "vcvtss2si_evex_vvvv_illegal",
            &[0x62, 0x71, 0xf6, 0x08, 0x2d, 0xc3],
        ),
        (
            "vcvtss2si_evex_vprime_illegal",
            &[0x62, 0x71, 0xfe, 0x00, 0x2d, 0xc3],
        ),
        (
            "vcvtsi2ss_evex_mask_illegal",
            &[0x62, 0xd1, 0xe6, 0x09, 0x2a, 0xc8],
        ),
        (
            "vcvtsi2ss_evex_zero_illegal",
            &[0x62, 0xd1, 0xe6, 0x88, 0x2a, 0xc8],
        ),
        (
            "vcvtdq2ps_evex_vvvv_illegal",
            &[0x62, 0xf1, 0x74, 0x48, 0x5b, 0xcb],
        ),
        (
            "vcvtps2dq_evex_vvvv_illegal",
            &[0x62, 0xf1, 0x75, 0x48, 0x5b, 0xcb],
        ),
        (
            "vcvtudq2ps_evex_vvvv_illegal",
            &[0x62, 0xf1, 0x77, 0x48, 0x7a, 0xcb],
        ),
        (
            "vcvttps2dq_evex_vvvv_illegal",
            &[0x62, 0xf1, 0x76, 0x48, 0x5b, 0xcb],
        ),
        (
            "vcvtps2ph_evex_vvvv_illegal",
            &[0x62, 0xf3, 0x75, 0x48, 0x1d, 0xd9, 0x00],
        ),
        (
            "vcvtps2ph_evex_memory_zero_illegal",
            &[0x62, 0xf3, 0x7d, 0xc8, 0x1d, 0x58, 0x02, 0x00],
        ),
        (
            "evex_vmovd_load_mask_illegal",
            &[0x62, 0xd1, 0x7d, 0x09, 0x6e, 0xc8],
        ),
        (
            "evex_vmovd_store_zero_illegal",
            &[0x62, 0xd1, 0x7d, 0x88, 0x7e, 0xc8],
        ),
        (
            "evex_vmovq_vec_load_vvvv_illegal",
            &[0x62, 0xf1, 0xf6, 0x08, 0x7e, 0xcb],
        ),
        (
            "evex_vmovq_vec_store_mask_illegal",
            &[0x62, 0xf1, 0xfd, 0x09, 0xd6, 0x48, 0x08],
        ),
        (
            "evex_vpextrb_mask_illegal",
            &[0x62, 0xd3, 0x7d, 0x09, 0x14, 0xc8, 0x01],
        ),
        (
            "evex_vpextrw_0f_zero_illegal",
            &[0x62, 0x71, 0x7d, 0x88, 0xc5, 0xc1, 0x01],
        ),
        (
            "evex_vpextrw_0f_memory_source_illegal",
            &[0x62, 0x71, 0x7d, 0x08, 0xc5, 0x00, 0x01],
        ),
        (
            "evex_vpinsrb_zero_illegal",
            &[0x62, 0xd3, 0x65, 0x88, 0x20, 0xc8, 0x01],
        ),
        (
            "evex_vpinsrw_l1_illegal",
            &[0x62, 0xd1, 0x65, 0x28, 0xc4, 0xc8, 0x01],
        ),
        (
            "evex_vinsertps_w_illegal",
            &[0x62, 0xf3, 0xe5, 0x08, 0x21, 0xca, 0x20],
        ),
        (
            "evex_vmovss_store_zero_illegal",
            &[0x62, 0xf1, 0x7e, 0x88, 0x11, 0x50, 0x10],
        ),
        (
            "evex_vmovss_store_vvvv_illegal",
            &[0x62, 0xf1, 0x76, 0x08, 0x11, 0x50, 0x10],
        ),
        (
            "evex_vmovss_load_vvvv_illegal",
            &[0x62, 0xf1, 0x76, 0x08, 0x10, 0x48, 0x10],
        ),
        (
            "evex_vmovlps_mask_illegal",
            &[0x62, 0xf1, 0x64, 0x09, 0x12, 0x48, 0x08],
        ),
        (
            "evex_vmovlpd_register_source_illegal",
            &[0x62, 0xf1, 0xe5, 0x08, 0x12, 0xca],
        ),
        (
            "evex_vmovsldup_broadcast_illegal",
            &[0x62, 0xf1, 0x7e, 0x58, 0x12, 0xcb],
        ),
        (
            "evex_vmovddup_vvvv_illegal",
            &[0x62, 0xf1, 0xf7, 0x48, 0x12, 0xcb],
        ),
        (
            "vpermd_l0_illegal",
            &[0xc4, 0xe2, 0x69, 0x36, 0xd9],
        ),
        (
            "vpermps_l0_illegal",
            &[0xc4, 0xe2, 0x69, 0x16, 0xd9],
        ),
        (
            "vpermq_vvvv_illegal",
            &[0xc4, 0xe3, 0xf5, 0x00, 0xd1, 0x00],
        ),
        (
            "vpermq_l0_illegal",
            &[0xc4, 0xe3, 0xf9, 0x00, 0xd1, 0x00],
        ),
        (
            "vpermpd_l0_illegal",
            &[0xc4, 0xe3, 0xf9, 0x01, 0xd1, 0x00],
        ),
        (
            "vpextrb_l1_illegal",
            &[0xc4, 0xe3, 0x7d, 0x14, 0xc8, 0x00],
        ),
        (
            "vpextrw_0f_l1_illegal",
            &[0xc5, 0xfd, 0xc5, 0xc1, 0x00],
        ),
        (
            "vpextrw_0f_memory_source_illegal",
            &[0xc5, 0xf9, 0xc5, 0x00, 0x00],
        ),
        (
            "vpextrw_0f3a_l1_illegal",
            &[0xc4, 0xe3, 0x7d, 0x15, 0x08, 0x00],
        ),
        (
            "vpextrd_l1_illegal",
            &[0xc4, 0xe3, 0x7d, 0x16, 0xc8, 0x00],
        ),
        (
            "vextractps_l1_illegal",
            &[0xc4, 0xe3, 0x7d, 0x17, 0xc8, 0x00],
        ),
        (
            "vpinsrb_l1_illegal",
            &[0xc4, 0xe3, 0x75, 0x20, 0xd0, 0x00],
        ),
        (
            "vpinsrw_l1_illegal",
            &[0xc5, 0xf5, 0xc4, 0xd0, 0x00],
        ),
        (
            "vpinsrd_l1_illegal",
            &[0xc4, 0xe3, 0x75, 0x22, 0xd0, 0x00],
        ),
        (
            "vinsertps_l1_illegal",
            &[0xc4, 0xe3, 0x6d, 0x21, 0xd9, 0x00],
        ),
        (
            "vex_0f71_group0_illegal",
            &[0xc5, 0xe9, 0x71, 0xc1, 0x01],
        ),
        (
            "vex_0f71_group1_illegal",
            &[0xc5, 0xe9, 0x71, 0xc9, 0x01],
        ),
        (
            "vex_0f71_group3_illegal",
            &[0xc5, 0xe9, 0x71, 0xd9, 0x01],
        ),
        (
            "vex_0f71_group5_illegal",
            &[0xc5, 0xe9, 0x71, 0xe9, 0x01],
        ),
        (
            "vex_0f71_group7_illegal",
            &[0xc5, 0xe9, 0x71, 0xf9, 0x01],
        ),
        (
            "vex_0f72_group0_illegal",
            &[0xc5, 0xe9, 0x72, 0xc1, 0x01],
        ),
        (
            "vex_0f72_group1_illegal",
            &[0xc5, 0xe9, 0x72, 0xc9, 0x01],
        ),
        (
            "vex_0f72_group3_illegal",
            &[0xc5, 0xe9, 0x72, 0xd9, 0x01],
        ),
        (
            "vex_0f72_group5_illegal",
            &[0xc5, 0xe9, 0x72, 0xe9, 0x01],
        ),
        (
            "vex_0f72_group7_illegal",
            &[0xc5, 0xe9, 0x72, 0xf9, 0x01],
        ),
        (
            "vex_0f73_group0_illegal",
            &[0xc5, 0xe9, 0x73, 0xc1, 0x01],
        ),
        (
            "vex_0f73_group1_illegal",
            &[0xc5, 0xe9, 0x73, 0xc9, 0x01],
        ),
        (
            "vex_0f73_group4_illegal",
            &[0xc5, 0xe9, 0x73, 0xe1, 0x01],
        ),
        (
            "vex_0f73_group5_illegal",
            &[0xc5, 0xe9, 0x73, 0xe9, 0x01],
        ),
        (
            "evex_0f71_group0_illegal",
            &[0x62, 0xf1, 0x6d, 0x48, 0x71, 0xc1, 0x01],
        ),
        (
            "evex_0f71_group1_illegal",
            &[0x62, 0xf1, 0x6d, 0x48, 0x71, 0xc9, 0x01],
        ),
        (
            "evex_0f71_group3_illegal",
            &[0x62, 0xf1, 0x6d, 0x48, 0x71, 0xd9, 0x01],
        ),
        (
            "evex_0f71_group5_illegal",
            &[0x62, 0xf1, 0x6d, 0x48, 0x71, 0xe9, 0x01],
        ),
        (
            "evex_0f71_group7_illegal",
            &[0x62, 0xf1, 0x6d, 0x48, 0x71, 0xf9, 0x01],
        ),
        (
            "evex_0f72_group3_illegal",
            &[0x62, 0xf1, 0x6d, 0x48, 0x72, 0xd9, 0x01],
        ),
        (
            "evex_0f72_group5_illegal",
            &[0x62, 0xf1, 0x6d, 0x48, 0x72, 0xe9, 0x01],
        ),
        (
            "evex_0f72_group7_illegal",
            &[0x62, 0xf1, 0x6d, 0x48, 0x72, 0xf9, 0x01],
        ),
        (
            "evex_0f73_group0_illegal",
            &[0x62, 0xf1, 0xed, 0x48, 0x73, 0xc1, 0x01],
        ),
        (
            "evex_0f73_group1_illegal",
            &[0x62, 0xf1, 0xed, 0x48, 0x73, 0xc9, 0x01],
        ),
        (
            "evex_0f73_group4_illegal",
            &[0x62, 0xf1, 0xed, 0x48, 0x73, 0xe1, 0x01],
        ),
        (
            "evex_0f73_group5_illegal",
            &[0x62, 0xf1, 0xed, 0x48, 0x73, 0xe9, 0x01],
        ),
        (
            "legacy_0f71_sse2_group0_illegal",
            &[0x66, 0x0f, 0x71, 0xc1, 0x01],
        ),
        (
            "legacy_0f71_sse2_group1_illegal",
            &[0x66, 0x0f, 0x71, 0xc9, 0x01],
        ),
        (
            "legacy_0f71_sse2_group3_illegal",
            &[0x66, 0x0f, 0x71, 0xd9, 0x01],
        ),
        (
            "legacy_0f71_sse2_group5_illegal",
            &[0x66, 0x0f, 0x71, 0xe9, 0x01],
        ),
        (
            "legacy_0f71_sse2_group7_illegal",
            &[0x66, 0x0f, 0x71, 0xf9, 0x01],
        ),
        (
            "legacy_0f71_mmx_group0_illegal",
            &[0x0f, 0x71, 0xc1, 0x01],
        ),
        (
            "legacy_0f71_mmx_group1_illegal",
            &[0x0f, 0x71, 0xc9, 0x01],
        ),
        (
            "legacy_0f71_mmx_group3_illegal",
            &[0x0f, 0x71, 0xd9, 0x01],
        ),
        (
            "legacy_0f71_mmx_group5_illegal",
            &[0x0f, 0x71, 0xe9, 0x01],
        ),
        (
            "legacy_0f71_mmx_group7_illegal",
            &[0x0f, 0x71, 0xf9, 0x01],
        ),
        (
            "legacy_0f72_sse2_group0_illegal",
            &[0x66, 0x0f, 0x72, 0xc1, 0x01],
        ),
        (
            "legacy_0f72_sse2_group1_illegal",
            &[0x66, 0x0f, 0x72, 0xc9, 0x01],
        ),
        (
            "legacy_0f72_sse2_group3_illegal",
            &[0x66, 0x0f, 0x72, 0xd9, 0x01],
        ),
        (
            "legacy_0f72_sse2_group5_illegal",
            &[0x66, 0x0f, 0x72, 0xe9, 0x01],
        ),
        (
            "legacy_0f72_sse2_group7_illegal",
            &[0x66, 0x0f, 0x72, 0xf9, 0x01],
        ),
        (
            "legacy_0f72_mmx_group0_illegal",
            &[0x0f, 0x72, 0xc1, 0x01],
        ),
        (
            "legacy_0f72_mmx_group1_illegal",
            &[0x0f, 0x72, 0xc9, 0x01],
        ),
        (
            "legacy_0f72_mmx_group3_illegal",
            &[0x0f, 0x72, 0xd9, 0x01],
        ),
        (
            "legacy_0f72_mmx_group5_illegal",
            &[0x0f, 0x72, 0xe9, 0x01],
        ),
        (
            "legacy_0f72_mmx_group7_illegal",
            &[0x0f, 0x72, 0xf9, 0x01],
        ),
        (
            "legacy_0f73_sse2_group0_illegal",
            &[0x66, 0x0f, 0x73, 0xc1, 0x01],
        ),
        (
            "legacy_0f73_sse2_group1_illegal",
            &[0x66, 0x0f, 0x73, 0xc9, 0x01],
        ),
        (
            "legacy_0f73_sse2_group4_illegal",
            &[0x66, 0x0f, 0x73, 0xe1, 0x01],
        ),
        (
            "legacy_0f73_sse2_group5_illegal",
            &[0x66, 0x0f, 0x73, 0xe9, 0x01],
        ),
        (
            "legacy_0f73_mmx_group0_illegal",
            &[0x0f, 0x73, 0xc1, 0x01],
        ),
        (
            "legacy_0f73_mmx_group1_illegal",
            &[0x0f, 0x73, 0xc9, 0x01],
        ),
        (
            "legacy_0f73_mmx_group3_illegal",
            &[0x0f, 0x73, 0xd9, 0x01],
        ),
        (
            "legacy_0f73_mmx_group4_illegal",
            &[0x0f, 0x73, 0xe1, 0x01],
        ),
        (
            "legacy_0f73_mmx_group5_illegal",
            &[0x0f, 0x73, 0xe9, 0x01],
        ),
        (
            "legacy_0f73_mmx_group7_illegal",
            &[0x0f, 0x73, 0xf9, 0x01],
        ),
        (
            "vpsravq_vex_illegal",
            &[0xc4, 0xe2, 0xe9, 0x46, 0xd9],
        ),
        (
            "vroundps_vvvv_illegal",
            &[0xc4, 0xe3, 0x71, 0x08, 0xd1, 0x00],
        ),
        (
            "vroundpd_vvvv_illegal",
            &[0xc4, 0xe3, 0x71, 0x09, 0xd1, 0x00],
        ),
        (
            "vdppd_l1_illegal",
            &[0xc4, 0xe3, 0x6d, 0x41, 0xd9, 0xff],
        ),
        (
            "vmovmskps_memory_source_illegal",
            &[0xc5, 0xf8, 0x50, 0x01],
        ),
        (
            "vmovmskpd_memory_source_illegal",
            &[0xc5, 0xf9, 0x50, 0x01],
        ),
        (
            "vpmaskmovd_load_register_operand_illegal",
            &[0xc4, 0xe2, 0x71, 0x8c, 0xd0],
        ),
        (
            "vpmaskmovd_store_register_operand_illegal",
            &[0xc4, 0xe2, 0x69, 0x8e, 0xc8],
        ),
        (
            "vmaskmovps_load_register_operand_illegal",
            &[0xc4, 0xe2, 0x71, 0x2c, 0xd0],
        ),
        (
            "vmaskmovps_store_register_operand_illegal",
            &[0xc4, 0xe2, 0x69, 0x2e, 0xc8],
        ),
        (
            "kmovw_store_register_dest_illegal",
            &[0xc5, 0xf8, 0x91, 0xc8],
        ),
        (
            "kmovw_from_gpr_memory_source_illegal",
            &[0xc5, 0xf8, 0x92, 0x08],
        ),
        (
            "kmovw_to_gpr_memory_source_illegal",
            &[0xc5, 0xf8, 0x93, 0x01],
        ),
        (
            "vldmxcsr_register_operand_illegal",
            &[0xc5, 0xf8, 0xae, 0xd0],
        ),
        (
            "vstmxcsr_register_operand_illegal",
            &[0xc5, 0xf8, 0xae, 0xd8],
        ),
        (
            "vex_mxcsr_group0_illegal",
            &[0xc5, 0xf8, 0xae, 0x00],
        ),
        (
            "vgatherdps_register_operand_illegal",
            &[0xc4, 0xe2, 0x61, 0x92, 0xd1],
        ),
        (
            "vgatherdps_no_sib_illegal",
            &[0xc4, 0xe2, 0x61, 0x92, 0x10],
        ),
        (
            "aesimc_missing_66_prefix_illegal",
            &[0x0f, 0x38, 0xdb, 0xc1],
        ),
        (
            "aesenc_missing_66_prefix_illegal",
            &[0x0f, 0x38, 0xdc, 0xc1],
        ),
        (
            "aesenclast_missing_66_prefix_illegal",
            &[0x0f, 0x38, 0xdd, 0xc1],
        ),
        (
            "aesdec_missing_66_prefix_illegal",
            &[0x0f, 0x38, 0xde, 0xc1],
        ),
        (
            "aesdeclast_missing_66_prefix_illegal",
            &[0x0f, 0x38, 0xdf, 0xc1],
        ),
        (
            "roundps_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x08, 0xc1, 0x00],
        ),
        (
            "roundpd_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x09, 0xc1, 0x00],
        ),
        (
            "roundss_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x0a, 0xc1, 0x00],
        ),
        (
            "roundsd_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x0b, 0xc1, 0x00],
        ),
        (
            "blendps_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x0c, 0xc1, 0x00],
        ),
        (
            "blendpd_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x0d, 0xc1, 0x00],
        ),
        (
            "pblendw_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x0e, 0xc1, 0x00],
        ),
        (
            "pextrb_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x14, 0xc1, 0x00],
        ),
        (
            "pextrw_0f3a_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x15, 0xc1, 0x00],
        ),
        (
            "pextrd_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x16, 0xc1, 0x00],
        ),
        (
            "extractps_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x17, 0xc1, 0x00],
        ),
        (
            "pinsrb_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x20, 0xc1, 0x00],
        ),
        (
            "insertps_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x21, 0xc1, 0x00],
        ),
        (
            "pinsrd_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x22, 0xc1, 0x00],
        ),
        (
            "dpps_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x40, 0xc1, 0x00],
        ),
        (
            "dppd_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x41, 0xc1, 0x00],
        ),
        (
            "mpsadbw_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x42, 0xc1, 0x00],
        ),
        (
            "pclmulqdq_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x44, 0xc1, 0x00],
        ),
        (
            "pcmpestrm_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x60, 0xc1, 0x00],
        ),
        (
            "pcmpestri_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x61, 0xc1, 0x00],
        ),
        (
            "pcmpistrm_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x62, 0xc1, 0x00],
        ),
        (
            "pcmpistri_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0x63, 0xc1, 0x00],
        ),
        (
            "aeskeygenassist_missing_66_prefix_illegal",
            &[0x0f, 0x3a, 0xdf, 0xc1, 0x00],
        ),
        (
            "invpcid_missing_66_prefix_illegal",
            &[0x0f, 0x38, 0x82, 0xc0],
        ),
        (
            "movbe_load_register_source_illegal",
            &[0x0f, 0x38, 0xf0, 0xc1],
        ),
        (
            "movbe_store_register_dest_illegal",
            &[0x0f, 0x38, 0xf1, 0xc1],
        ),
        (
            "invpcid_register_descriptor_illegal",
            &[0x66, 0x0f, 0x38, 0x82, 0xc0],
        ),
        ("fxsave_register_operand_illegal", &[0x0f, 0xae, 0xc0]),
        ("fxrstor_register_operand_illegal", &[0x0f, 0xae, 0xc8]),
        ("ldmxcsr_register_operand_illegal", &[0x0f, 0xae, 0xd0]),
        ("stmxcsr_register_operand_illegal", &[0x0f, 0xae, 0xd8]),
        ("xsave_register_operand_illegal", &[0x0f, 0xae, 0xe0]),
        (
            "xrstors_register_operand_illegal",
            &[0x48, 0x0f, 0xc7, 0xd8],
        ),
        (
            "xsavec_register_operand_illegal",
            &[0x48, 0x0f, 0xc7, 0xe0],
        ),
        (
            "xsaves_register_operand_illegal",
            &[0x48, 0x0f, 0xc7, 0xe8],
        ),
        ("rdrand_memory_operand_illegal", &[0x48, 0x0f, 0xc7, 0x30]),
        ("rdseed_memory_operand_illegal", &[0x48, 0x0f, 0xc7, 0x38]),
        (
            "rdpid_memory_operand_illegal",
            &[0xf3, 0x48, 0x0f, 0xc7, 0x38],
        ),
        ("cmpxchg8b_register_operand_illegal", &[0x0f, 0xc7, 0xc8]),
        (
            "cmpxchg16b_register_operand_illegal",
            &[0x48, 0x0f, 0xc7, 0xc8],
        ),
        (
            "punpcklqdq_missing_66_prefix_illegal",
            &[0x0f, 0x6c, 0xc1],
        ),
        (
            "punpckhqdq_missing_66_prefix_illegal",
            &[0x0f, 0x6d, 0xc1],
        ),
        (
            "pextrw_mmx_memory_source_illegal",
            &[0x0f, 0xc5, 0x00, 0x00],
        ),
        (
            "pextrw_sse2_memory_source_illegal",
            &[0x66, 0x0f, 0xc5, 0x00, 0x00],
        ),
        ("movntps_register_dest_illegal", &[0x0f, 0x2b, 0xc1]),
        (
            "movntpd_register_dest_illegal",
            &[0x66, 0x0f, 0x2b, 0xc1],
        ),
        ("movmskps_memory_source_illegal", &[0x0f, 0x50, 0x01]),
        (
            "movmskpd_memory_source_illegal",
            &[0x66, 0x0f, 0x50, 0x01],
        ),
        (
            "hadd_missing_mandatory_prefix_illegal",
            &[0x0f, 0x7c, 0xc1],
        ),
        (
            "hadd_f3_prefix_illegal",
            &[0xf3, 0x0f, 0x7c, 0xc1],
        ),
        (
            "hsub_missing_mandatory_prefix_illegal",
            &[0x0f, 0x7d, 0xc1],
        ),
        (
            "hsub_f3_prefix_illegal",
            &[0xf3, 0x0f, 0x7d, 0xc1],
        ),
        (
            "addsub_missing_mandatory_prefix_illegal",
            &[0x0f, 0xd0, 0xc1],
        ),
        (
            "addsub_f3_prefix_illegal",
            &[0xf3, 0x0f, 0xd0, 0xc1],
        ),
    ]
}

fn divide_error_exception_cases() -> Vec<(&'static str, &'static [u8])> {
    vec![
        ("divb_zero", &[0x31, 0xc9, 0xf6, 0xf1]),
        (
            "divw_zero",
            &[0x66, 0x31, 0xc9, 0x66, 0xf7, 0xf1],
        ),
        ("divl_zero", &[0x31, 0xc9, 0xf7, 0xf1]),
        ("divq_zero", &[0x31, 0xc9, 0x48, 0xf7, 0xf1]),
        (
            "divb_quotient_overflow",
            &[0x66, 0xb8, 0x00, 0x01, 0xb1, 0x01, 0xf6, 0xf1],
        ),
        (
            "divw_quotient_overflow",
            &[
                0x66, 0xba, 0x01, 0x00, 0x66, 0x31, 0xc0, 0x66, 0xb9, 0x01, 0x00, 0x66, 0xf7,
                0xf1,
            ],
        ),
        (
            "divl_quotient_overflow",
            &[
                0xba, 0x01, 0x00, 0x00, 0x00, 0x31, 0xc0, 0xb9, 0x01, 0x00, 0x00, 0x00, 0xf7,
                0xf1,
            ],
        ),
        (
            "divq_quotient_overflow",
            &[
                0x48, 0xc7, 0xc2, 0x01, 0x00, 0x00, 0x00, 0x48, 0x31, 0xc0, 0x48, 0xc7, 0xc1,
                0x01, 0x00, 0x00, 0x00, 0x48, 0xf7, 0xf1,
            ],
        ),
        ("idivb_zero", &[0x31, 0xc9, 0xf6, 0xf9]),
        (
            "idivw_zero",
            &[0x66, 0x31, 0xc9, 0x66, 0xf7, 0xf9],
        ),
        ("idivl_zero", &[0x31, 0xc9, 0xf7, 0xf9]),
        (
            "idivq_zero",
            &[0x31, 0xd2, 0x31, 0xc9, 0x48, 0xf7, 0xf9],
        ),
        (
            "idivb_min_neg_one_overflow",
            &[0x66, 0xb8, 0x80, 0xff, 0xb1, 0xff, 0xf6, 0xf9],
        ),
        (
            "idivw_min_neg_one_overflow",
            &[
                0x66, 0xb8, 0x00, 0x80, 0x66, 0x99, 0x66, 0xb9, 0xff, 0xff, 0x66, 0xf7, 0xf9,
            ],
        ),
        (
            "idivl_min_neg_one_overflow",
            &[
                0xb8, 0x00, 0x00, 0x00, 0x80, 0x99, 0xb9, 0xff, 0xff, 0xff, 0xff, 0xf7, 0xf9,
            ],
        ),
        (
            "idivq_min_neg_one_overflow",
            &[
                0x48, 0xb8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x48, 0x99, 0x48,
                0xc7, 0xc1, 0xff, 0xff, 0xff, 0xff, 0x48, 0xf7, 0xf9,
            ],
        ),
    ]
}

struct ExceptionMarkerCase {
    label: &'static str,
    vector_name: &'static str,
    vector: usize,
    op: &'static [u8],
}

fn software_interrupt_exception_cases() -> Vec<ExceptionMarkerCase> {
    vec![
        ExceptionMarkerCase {
            label: "int3_short_bp",
            vector_name: "#BP",
            vector: 3,
            op: &[0xcc],
        },
        ExceptionMarkerCase {
            label: "int_3_long_bp",
            vector_name: "#BP",
            vector: 3,
            op: &[0xcd, 0x03],
        },
        ExceptionMarkerCase {
            label: "int_0_de",
            vector_name: "#DE",
            vector: 0,
            op: &[0xcd, 0x00],
        },
        ExceptionMarkerCase {
            label: "int_1_db",
            vector_name: "#DB",
            vector: 1,
            op: &[0xcd, 0x01],
        },
        ExceptionMarkerCase {
            label: "icebp_int1_db",
            vector_name: "#DB",
            vector: 1,
            op: &[0xf1],
        },
        ExceptionMarkerCase {
            label: "int_2_nmi_vector",
            vector_name: "vector 2",
            vector: 2,
            op: &[0xcd, 0x02],
        },
        ExceptionMarkerCase {
            label: "int_4_of",
            vector_name: "#OF",
            vector: 4,
            op: &[0xcd, 0x04],
        },
        ExceptionMarkerCase {
            label: "int_5_br",
            vector_name: "#BR",
            vector: 5,
            op: &[0xcd, 0x05],
        },
        ExceptionMarkerCase {
            label: "int_6_ud_vector",
            vector_name: "#UD",
            vector: 6,
            op: &[0xcd, 0x06],
        },
        ExceptionMarkerCase {
            label: "int_7_nm",
            vector_name: "#NM",
            vector: 7,
            op: &[0xcd, 0x07],
        },
        ExceptionMarkerCase {
            label: "int_8_df",
            vector_name: "#DF",
            vector: 8,
            op: &[0xcd, 0x08],
        },
        ExceptionMarkerCase {
            label: "int_10_ts",
            vector_name: "#TS",
            vector: 10,
            op: &[0xcd, 0x0a],
        },
        ExceptionMarkerCase {
            label: "int_11_np",
            vector_name: "#NP",
            vector: 11,
            op: &[0xcd, 0x0b],
        },
        ExceptionMarkerCase {
            label: "int_12_ss",
            vector_name: "#SS",
            vector: 12,
            op: &[0xcd, 0x0c],
        },
        ExceptionMarkerCase {
            label: "int_13_gp",
            vector_name: "#GP",
            vector: 13,
            op: &[0xcd, 0x0d],
        },
        ExceptionMarkerCase {
            label: "int_14_pf",
            vector_name: "#PF",
            vector: 14,
            op: &[0xcd, 0x0e],
        },
        ExceptionMarkerCase {
            label: "int_16_mf",
            vector_name: "#MF",
            vector: 16,
            op: &[0xcd, 0x10],
        },
        ExceptionMarkerCase {
            label: "int_17_ac",
            vector_name: "#AC",
            vector: 17,
            op: &[0xcd, 0x11],
        },
        ExceptionMarkerCase {
            label: "int_19_xm",
            vector_name: "#XM",
            vector: 19,
            op: &[0xcd, 0x13],
        },
        ExceptionMarkerCase {
            label: "int_0x20_user",
            vector_name: "vector 0x20",
            vector: 0x20,
            op: &[0xcd, 0x20],
        },
        ExceptionMarkerCase {
            label: "int_0x80_syscall",
            vector_name: "vector 0x80",
            vector: 0x80,
            op: &[0xcd, 0x80],
        },
        ExceptionMarkerCase {
            label: "int_0xff_max",
            vector_name: "vector 0xff",
            vector: 0xff,
            op: &[0xcd, 0xff],
        },
    ]
}

fn general_protection_exception_cases() -> Vec<ExceptionMarkerCase> {
    let mut cases = vec![
        ExceptionMarkerCase {
            label: "xsetbv_clear_x87_bit",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0x31, 0xc0, 0x31, 0xd2, 0x31, 0xc9, 0x0f, 0x01, 0xd1],
        },
        ExceptionMarkerCase {
            label: "xsetbv_avx_without_sse",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[
                0xb8, 0x04, 0x00, 0x00, 0x00, 0x31, 0xd2, 0x31, 0xc9, 0x0f, 0x01, 0xd1,
            ],
        },
        ExceptionMarkerCase {
            label: "xsetbv_nonzero_xcr_index",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[
                0xb8, 0x01, 0x00, 0x00, 0x00, 0x31, 0xd2, 0xb9, 0x01, 0x00, 0x00, 0x00, 0x0f,
                0x01, 0xd1,
            ],
        },
        ExceptionMarkerCase {
            label: "xsetbv_unsupported_high_bit",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[
                0xb8, 0x01, 0x00, 0x00, 0x00, 0xba, 0x01, 0x00, 0x00, 0x00, 0x31, 0xc9, 0x0f,
                0x01, 0xd1,
            ],
        },
        ExceptionMarkerCase {
            label: "xgetbv_unsupported_xcr_index",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0xb9, 0x02, 0x00, 0x00, 0x00, 0x0f, 0x01, 0xd0],
        },
        ExceptionMarkerCase {
            label: "rdpkru_nonzero_ecx",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[
                0x0f, 0x20, 0xe0, 0x48, 0x0d, 0x00, 0x00, 0x40, 0x00, 0x0f, 0x22, 0xe0, 0xb9,
                0x01, 0x00, 0x00, 0x00, 0x0f, 0x01, 0xee,
            ],
        },
        ExceptionMarkerCase {
            label: "wrpkru_nonzero_ecx",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[
                0x0f, 0x20, 0xe0, 0x48, 0x0d, 0x00, 0x00, 0x40, 0x00, 0x0f, 0x22, 0xe0, 0x31,
                0xc0, 0xb9, 0x01, 0x00, 0x00, 0x00, 0x31, 0xd2, 0x0f, 0x01, 0xef,
            ],
        },
        ExceptionMarkerCase {
            label: "wrpkru_nonzero_edx",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[
                0x0f, 0x20, 0xe0, 0x48, 0x0d, 0x00, 0x00, 0x40, 0x00, 0x0f, 0x22, 0xe0, 0x31,
                0xc0, 0x31, 0xc9, 0xba, 0x01, 0x00, 0x00, 0x00, 0x0f, 0x01, 0xef,
            ],
        },
        ExceptionMarkerCase {
            label: "xsetbv_partial_avx512_state",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[
                0xb8, 0x27, 0x00, 0x00, 0x00, 0x31, 0xd2, 0x31, 0xc9, 0x0f, 0x01, 0xd1,
            ],
        },
        ExceptionMarkerCase {
            label: "movaps_load_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0x0f, 0x28, 0x40, 0x01],
        },
        ExceptionMarkerCase {
            label: "movaps_store_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0x0f, 0x29, 0x40, 0x01],
        },
        ExceptionMarkerCase {
            label: "movapd_load_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0x66, 0x0f, 0x28, 0x40, 0x01],
        },
        ExceptionMarkerCase {
            label: "movapd_store_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0x66, 0x0f, 0x29, 0x40, 0x01],
        },
        ExceptionMarkerCase {
            label: "movdqa_load_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0x66, 0x0f, 0x6f, 0x40, 0x01],
        },
        ExceptionMarkerCase {
            label: "movdqa_store_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0x66, 0x0f, 0x7f, 0x40, 0x01],
        },
        ExceptionMarkerCase {
            label: "vmovaps_xmm_load_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0xc5, 0xf8, 0x28, 0x40, 0x01],
        },
        ExceptionMarkerCase {
            label: "vmovaps_xmm_store_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0xc5, 0xf8, 0x29, 0x40, 0x01],
        },
        ExceptionMarkerCase {
            label: "vmovaps_ymm_load_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0xc5, 0xfc, 0x28, 0x40, 0x01],
        },
        ExceptionMarkerCase {
            label: "vmovaps_ymm_store_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0xc5, 0xfc, 0x29, 0x40, 0x01],
        },
        ExceptionMarkerCase {
            label: "vmovapd_xmm_load_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0xc5, 0xf9, 0x28, 0x40, 0x01],
        },
        ExceptionMarkerCase {
            label: "vmovapd_xmm_store_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0xc5, 0xf9, 0x29, 0x40, 0x01],
        },
        ExceptionMarkerCase {
            label: "vmovapd_ymm_load_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0xc5, 0xfd, 0x28, 0x40, 0x01],
        },
        ExceptionMarkerCase {
            label: "vmovapd_ymm_store_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0xc5, 0xfd, 0x29, 0x40, 0x01],
        },
        ExceptionMarkerCase {
            label: "vmovdqa_xmm_load_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0xc5, 0xf9, 0x6f, 0x40, 0x01],
        },
        ExceptionMarkerCase {
            label: "vmovdqa_xmm_store_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0xc5, 0xf9, 0x7f, 0x40, 0x01],
        },
        ExceptionMarkerCase {
            label: "vmovdqa_ymm_load_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0xc5, 0xfd, 0x6f, 0x40, 0x01],
        },
        ExceptionMarkerCase {
            label: "vmovdqa_ymm_store_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[0xc5, 0xfd, 0x7f, 0x40, 0x01],
        },
        ExceptionMarkerCase {
            label: "evex_vmovaps_zmm_load_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[
                0x62, 0xf1, 0x7c, 0x48, 0x28, 0x80, 0x01, 0x00, 0x00, 0x00,
            ],
        },
        ExceptionMarkerCase {
            label: "evex_vmovaps_zmm_store_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[
                0x62, 0xf1, 0x7c, 0x48, 0x29, 0x80, 0x01, 0x00, 0x00, 0x00,
            ],
        },
        ExceptionMarkerCase {
            label: "evex_vmovapd_zmm_load_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[
                0x62, 0xf1, 0xfd, 0x48, 0x28, 0x80, 0x01, 0x00, 0x00, 0x00,
            ],
        },
        ExceptionMarkerCase {
            label: "evex_vmovapd_zmm_store_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[
                0x62, 0xf1, 0xfd, 0x48, 0x29, 0x80, 0x01, 0x00, 0x00, 0x00,
            ],
        },
        ExceptionMarkerCase {
            label: "evex_vmovdqa32_zmm_load_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[
                0x62, 0xf1, 0x7d, 0x48, 0x6f, 0x80, 0x01, 0x00, 0x00, 0x00,
            ],
        },
        ExceptionMarkerCase {
            label: "evex_vmovdqa32_zmm_store_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[
                0x62, 0xf1, 0x7d, 0x48, 0x7f, 0x80, 0x01, 0x00, 0x00, 0x00,
            ],
        },
        ExceptionMarkerCase {
            label: "evex_vmovdqa64_zmm_load_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[
                0x62, 0xf1, 0xfd, 0x48, 0x6f, 0x80, 0x01, 0x00, 0x00, 0x00,
            ],
        },
        ExceptionMarkerCase {
            label: "evex_vmovdqa64_zmm_store_unaligned",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[
                0x62, 0xf1, 0xfd, 0x48, 0x7f, 0x80, 0x01, 0x00, 0x00, 0x00,
            ],
        },
    ];

    if host_cpu_flag("movdir64b") {
        cases.push(ExceptionMarkerCase {
            label: "movdir64b_misaligned_destination",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[
                0x48, 0xff, 0xc0, // inc %rax, making the destination 64-byte misaligned
                0x66, 0x0f, 0x38, 0xf8, 0x03,
            ],
        });
    }
    if host_cpu_flag("xsaves") {
        cases.push(ExceptionMarkerCase {
            label: "xrstors_non_compacted_xsave_area",
            vector_name: "#GP",
            vector: GP_VECTOR,
            op: &[
                0x48, 0xc7, 0x83, 0xf0, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0xc7,
                0x83, 0xf8, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xb8, 0xe7, 0x00, 0x00,
                0x00, 0x31, 0xd2, 0x48, 0x0f, 0xc7, 0x9b, 0xf0, 0x00, 0x00, 0x00,
            ],
        });
    }

    cases
}

fn run_exception_marker_cases(name: &str, cases: Vec<ExceptionMarkerCase>, expected: usize) {
    if !is_x86_feature_detected!("avx512f") {
        eprintln!("[skip] host lacks AVX-512F");
        return;
    }
    let Some(oracle) = oracle() else {
        eprintln!("[skip] /dev/kvm unavailable or AVX-512 XSAVE undrivable");
        return;
    };

    let case_count = cases.len();
    assert_eq!(case_count, expected, "unexpected {name} corpus size");

    let input = input_for(InputProfile::Int);
    let mut failures = Vec::new();
    for case in cases {
        let expected_marker = exception_marker(case.vector);
        let code = build_fault_probe_code(case.op);
        let kvm = match oracle.run_with_exception_trap(&code, &input, case.vector) {
            Ok(KvmOutcome::Ran(out)) => out,
            Ok(KvmOutcome::Faulted) => {
                failures.push(format!(
                    "{}: KVM faulted before reaching the {} handler",
                    case.label, case.vector_name
                ));
                continue;
            }
            Err(e) => panic!("{}: KVM backend failure: {e}", case.label),
        };
        let interp = match run_interp_with_exception_trap(&code, &input, case.vector) {
            Ok(out) => out,
            Err(e) => {
                failures.push(format!(
                    "{}: rax failed before reaching {} handler: {e}",
                    case.label, case.vector_name
                ));
                continue;
            }
        };

        let kvm_marker = scratch_marker(&kvm.scratch);
        let interp_marker = scratch_marker(&interp.scratch);
        if kvm_marker != expected_marker {
            failures.push(format!(
                "{}: KVM marker {kvm_marker:#x}, expected {} marker {expected_marker:#x}",
                case.label, case.vector_name
            ));
        }
        if interp_marker != expected_marker {
            failures.push(format!(
                "{}: rax marker {interp_marker:#x}, expected {} marker {expected_marker:#x}",
                case.label, case.vector_name
            ));
        }
    }

    eprintln!(
        "[avx512-kvm-diff] {name} exception markers compared={}",
        case_count.saturating_sub(failures.len())
    );
    assert!(
        failures.is_empty(),
        "{name} exception marker mismatches vs silicon:\n{}",
        failures.join("\n")
    );
}

fn run_exception_marker_corpus(
    name: &str,
    vector_name: &'static str,
    vector: usize,
    cases: Vec<(&'static str, &'static [u8])>,
    expected: usize,
) {
    let marker_cases = cases
        .into_iter()
        .map(|(label, op)| ExceptionMarkerCase {
            label,
            vector_name,
            vector,
            op,
        })
        .collect();
    run_exception_marker_cases(name, marker_cases, expected);
}

fn run_ud_marker_corpus(name: &str, cases: Vec<(&'static str, &'static [u8])>, expected: usize) {
    run_exception_marker_corpus(name, "#UD", UD_VECTOR, cases, expected);
}

#[test]
fn avx512_kvm_legacy_invalid_long_mode_ud_corpus() {
    run_ud_marker_corpus("invalid-long-mode legacy", legacy_invalid_long_mode_cases(), 17);
}

#[test]
fn avx512_kvm_illegal_lock_prefix_ud_corpus() {
    run_ud_marker_corpus("illegal LOCK prefix", illegal_lock_prefix_cases(), 15);
}

#[test]
fn avx512_kvm_undefined_opcode_ud_corpus() {
    run_ud_marker_corpus("undefined opcode", undefined_opcode_cases(), 11);
}

#[test]
fn avx512_kvm_invalid_extension_encoding_ud_corpus() {
    run_ud_marker_corpus(
        "invalid extension encoding",
        invalid_extension_encoding_cases(),
        240,
    );
}

#[test]
fn avx512_kvm_divide_error_exception_corpus() {
    run_exception_marker_corpus(
        "integer divide error",
        "#DE",
        DE_VECTOR,
        divide_error_exception_cases(),
        16,
    );
}

#[test]
fn avx512_kvm_software_interrupt_exception_corpus() {
    run_exception_marker_cases(
        "software interrupt",
        software_interrupt_exception_cases(),
        22,
    );
}

#[test]
fn avx512_kvm_general_protection_exception_corpus() {
    let mut expected = 35;
    if host_cpu_flag("movdir64b") {
        expected += 1;
    }
    if host_cpu_flag("xsaves") {
        expected += 1;
    }
    run_exception_marker_cases(
        "general protection",
        general_protection_exception_cases(),
        expected,
    );
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
        InputProfile::IntPredicateEdge,
        InputProfile::IntConvertEdge,
        InputProfile::F32,
        InputProfile::F64,
        InputProfile::F16,
        InputProfile::F32Edge,
        InputProfile::F64Edge,
        InputProfile::F32ConvertEdge,
        InputProfile::F64ConvertEdge,
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
    assert_eq!(cases.len(), 53, "unexpected privileged corpus size");

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
    assert_eq!(
        tally.ran_for(Feat::ControlReg),
        14,
        "all control-register cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::DescriptorTable),
        13,
        "all descriptor-table cases should run"
    );
    assert_eq!(tally.ran_for(Feat::Msr), 14, "all MSR cases should run");
    assert_eq!(
        tally.ran_for(Feat::DebugReg),
        12,
        "all debug-register cases should run"
    );
    assert_eq!(tally.compared, 53, "all privileged cases should compare");
}

#[test]
fn avx512_kvm_descriptor_table_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| {
            case.feat == Feat::DescriptorTable && case.label.contains("descriptor_table_edge_")
        })
        .collect();
    assert_eq!(
        cases.len(),
        5,
        "unexpected descriptor-table edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on descriptor-table edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a descriptor-table edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "descriptor-table edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "descriptor-table edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::DescriptorTable),
        5,
        "all descriptor-table edge cases should run"
    );
    assert_eq!(
        tally.compared, 5,
        "all descriptor-table edge cases should compare"
    );
}

#[test]
fn avx512_kvm_control_register_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| {
            case.feat == Feat::ControlReg && case.label.contains("control_reg_edge_")
        })
        .collect();
    assert_eq!(
        cases.len(),
        6,
        "unexpected control-register edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on control-register edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a control-register edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "control-register edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "control-register edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::ControlReg),
        6,
        "all control-register edge cases should run"
    );
    assert_eq!(
        tally.compared, 6,
        "all control-register edge cases should compare"
    );
}

#[test]
fn avx512_kvm_msr_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Msr && case.label.contains("msr_edge_"))
        .collect();
    assert_eq!(cases.len(), 6, "unexpected MSR edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on MSR edge cases");
    assert_eq!(tally.interp_err, 0, "rax failed to execute an MSR edge case");
    assert_eq!(
        tally.skipped_asm, 0,
        "MSR edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "MSR edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Msr),
        6,
        "all MSR edge cases should run"
    );
    assert_eq!(tally.compared, 6, "all MSR edge cases should compare");
}

#[test]
fn avx512_kvm_debug_register_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::DebugReg && case.label.contains("debug_reg_edge_"))
        .collect();
    assert_eq!(cases.len(), 5, "unexpected debug-register edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on debug-register edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a debug-register edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "debug-register edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "debug-register edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::DebugReg),
        5,
        "all debug-register edge cases should run"
    );
    assert_eq!(
        tally.compared, 5,
        "all debug-register edge cases should compare"
    );
}

#[test]
fn avx512_kvm_privileged_machine_state_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_priv_state_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        18,
        "unexpected privileged machine-state edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on privileged machine-state edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a privileged machine-state edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "privileged machine-state edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "privileged machine-state edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::ControlReg),
        4,
        "all control-register edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::DescriptorTable),
        5,
        "all descriptor-table edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Msr),
        5,
        "all MSR edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::DebugReg),
        4,
        "all debug-register edge cases should run"
    );
    assert_eq!(
        tally.compared, 18,
        "all privileged machine-state edge cases should compare"
    );
}

#[test]
fn avx512_kvm_descriptor_access_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::DescriptorAccess)
        .collect();
    assert_eq!(cases.len(), 25, "unexpected descriptor-access corpus size");

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
        tally.ran_for(Feat::DescriptorAccess),
        25,
        "all descriptor-access cases should run"
    );
    assert_eq!(
        tally.compared, 25,
        "all descriptor-access cases should compare"
    );
}

#[test]
fn avx512_kvm_descriptor_access_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| {
            case.feat == Feat::DescriptorAccess
                && case.label.contains("_descriptor_access_edge_")
        })
        .collect();
    assert_eq!(
        cases.len(),
        10,
        "unexpected descriptor-access edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on descriptor-access edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a descriptor-access edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "descriptor-access edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "descriptor-access edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::DescriptorAccess),
        10,
        "all descriptor-access edge cases should run"
    );
    assert_eq!(
        tally.compared, 10,
        "all descriptor-access edge cases should compare"
    );
}

#[test]
fn avx512_kvm_descriptor_group6_load_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| {
            case.feat == Feat::DescriptorAccess
                && case.label.contains("_descriptor_group6_load_")
        })
        .collect();
    assert_eq!(
        cases.len(),
        6,
        "unexpected descriptor Group-6 load corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on descriptor Group-6 load cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a descriptor Group-6 load case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "descriptor Group-6 load corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "descriptor Group-6 load cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::DescriptorAccess),
        6,
        "all descriptor Group-6 load cases should run"
    );
    assert_eq!(
        tally.compared, 6,
        "all descriptor Group-6 load cases should compare"
    );
}

#[test]
fn avx512_kvm_hint_nop_prefetch_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| matches!(case.feat, Feat::HintNop | Feat::Prefetchw))
        .collect();
    assert_eq!(cases.len(), 32, "unexpected hint/prefetch corpus size");

    let host = HostFeatures::detect();
    if !host.supports(Feat::Prefetchw) {
        eprintln!("[skip] host lacks PREFETCHW support; PREFETCHW/WT1 cases will skip");
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
        22,
        "all NOP/PAUSE/PREFETCHh cases should run"
    );
    if host.supports(Feat::Prefetchw) {
        assert_eq!(
            tally.ran_for(Feat::Prefetchw),
            10,
            "all PREFETCHW/PREFETCHWT1 cases should run"
        );
        assert_eq!(
            tally.skipped_feature, 0,
            "hint/prefetch cases should not feature-skip"
        );
        assert_eq!(tally.compared, 32, "all hint/prefetch cases should compare");
    } else {
        assert_eq!(
            tally.skipped_feature, 10,
            "only PREFETCHW/PREFETCHWT1 cases should feature-skip"
        );
        assert_eq!(
            tally.compared, 22,
            "all non-PREFETCHW hint cases should compare"
        );
    }
}

#[test]
fn avx512_kvm_prefetch_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_prefetch_edge_"))
        .collect();
    assert_eq!(cases.len(), 12, "unexpected prefetch edge corpus size");

    let host = HostFeatures::detect();
    if !host.supports(Feat::Prefetchw) {
        eprintln!("[skip] host lacks PREFETCHW support; PREFETCHW/WT1 edge cases will skip");
    }

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on prefetch edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a prefetch edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "prefetch edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.ran_for(Feat::HintNop),
        5,
        "all PREFETCHh edge cases should run"
    );
    if host.supports(Feat::Prefetchw) {
        assert_eq!(
            tally.ran_for(Feat::Prefetchw),
            7,
            "all PREFETCHW/PREFETCHWT1 edge cases should run"
        );
        assert_eq!(
            tally.skipped_feature, 0,
            "prefetch edge cases should not feature-skip"
        );
        assert_eq!(
            tally.compared, 12,
            "all prefetch edge cases should compare"
        );
    } else {
        assert_eq!(
            tally.skipped_feature, 7,
            "only PREFETCHW/PREFETCHWT1 edge cases should feature-skip"
        );
        assert_eq!(
            tally.compared, 5,
            "all PREFETCHh edge cases should compare"
        );
    }
}

#[test]
fn avx512_kvm_cache_memory_order_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| {
            matches!(
                case.feat,
                Feat::Fence
                    | Feat::Clflush
                    | Feat::Clflushopt
                    | Feat::Clwb
                    | Feat::Cldemote
                    | Feat::CacheInvd
                    | Feat::Wbnoinvd
            )
        })
        .collect();
    assert_eq!(
        cases.len(),
        43,
        "unexpected cache/memory-order corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on cache/memory-order cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a cache/memory-order case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "cache/memory-order corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "cache/memory-order cases should not feature-skip"
    );
    assert_eq!(tally.ran_for(Feat::Fence), 9, "all fence cases should run");
    assert_eq!(
        tally.ran_for(Feat::Clflush),
        6,
        "all CLFLUSH cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Clflushopt),
        6,
        "all CLFLUSHOPT cases should run"
    );
    assert_eq!(tally.ran_for(Feat::Clwb), 6, "all CLWB cases should run");
    assert_eq!(
        tally.ran_for(Feat::Cldemote),
        6,
        "all CLDEMOTE cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::CacheInvd),
        6,
        "all INVD/WBINVD cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Wbnoinvd),
        4,
        "all WBNOINVD cases should run"
    );
    assert_eq!(
        tally.compared, 43,
        "all cache/memory-order cases should compare"
    );
}

#[test]
fn avx512_kvm_cache_tlb_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_cache_tlb_edge_"))
        .collect();
    assert_eq!(cases.len(), 28, "unexpected cache/TLB edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on cache/TLB edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a cache/TLB edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "cache/TLB edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "cache/TLB edge cases should not feature-skip"
    );
    assert_eq!(tally.ran_for(Feat::Fence), 3, "all fence edge cases should run");
    assert_eq!(
        tally.ran_for(Feat::Clflush),
        3,
        "all CLFLUSH edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Clflushopt),
        3,
        "all CLFLUSHOPT edge cases should run"
    );
    assert_eq!(tally.ran_for(Feat::Clwb), 3, "all CLWB edge cases should run");
    assert_eq!(
        tally.ran_for(Feat::Cldemote),
        3,
        "all CLDEMOTE edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::CacheInvd),
        2,
        "all INVD/WBINVD edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Wbnoinvd),
        2,
        "all WBNOINVD edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Invlpg),
        4,
        "all INVLPG edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Invpcid),
        5,
        "all INVPCID edge cases should run"
    );
    assert_eq!(tally.compared, 28, "all cache/TLB edge cases should compare");
}

#[test]
fn avx512_kvm_monitor_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Monitor)
        .collect();
    assert_eq!(cases.len(), 8, "unexpected MONITOR corpus size");

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
        assert_eq!(tally.compared, 8, "all MONITOR cases should compare");
    } else {
        assert_eq!(
            tally.skipped_feature, 8,
            "all MONITOR cases should feature-skip"
        );
        assert_eq!(tally.compared, 0, "MONITOR cases should not run");
    }
}

#[test]
fn avx512_kvm_monitor_wait_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_wait_edge_"))
        .collect();
    assert_eq!(cases.len(), 12, "unexpected monitor/wait edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on monitor/wait edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a monitor/wait edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "monitor/wait edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "monitor/wait edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Monitor),
        4,
        "all MONITOR edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Waitpkg),
        8,
        "all WAITPKG edge cases should run"
    );
    assert_eq!(
        tally.compared, 12,
        "all monitor/wait edge cases should compare"
    );
}

#[test]
fn avx512_kvm_serialize_waitpkg_rdpid_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| matches!(case.feat, Feat::Serialize | Feat::Waitpkg | Feat::Rdpid))
        .collect();
    assert_eq!(
        cases.len(),
        31,
        "unexpected serialize/WAITPKG/RDPID corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on serialize/WAITPKG/RDPID cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a serialize/WAITPKG/RDPID case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "serialize/WAITPKG/RDPID corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "serialize/WAITPKG/RDPID cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Serialize),
        5,
        "all SERIALIZE cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Waitpkg),
        17,
        "all WAITPKG cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Rdpid),
        9,
        "all RDPID cases should run"
    );
    assert_eq!(
        tally.compared, 31,
        "all serialize/WAITPKG/RDPID cases should compare"
    );
}

#[test]
fn avx512_kvm_rdpid_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_rdpid_edge_"))
        .collect();
    assert_eq!(cases.len(), 3, "unexpected RDPID edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on RDPID edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an RDPID edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "RDPID edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "RDPID edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Rdpid),
        3,
        "all RDPID edge cases should run"
    );
    assert_eq!(tally.compared, 3, "all RDPID edge cases should compare");
}

#[test]
fn avx512_kvm_io_port_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Io)
        .collect();
    assert_eq!(cases.len(), 49, "unexpected I/O corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on I/O cases");
    assert_eq!(tally.interp_err, 0, "rax failed to execute an I/O case");
    assert_eq!(
        tally.skipped_asm, 0,
        "I/O corpus produced assembler-rejected cases"
    );
    assert_eq!(tally.compared, 49, "all I/O cases should compare");
}

#[test]
fn avx512_kvm_io_port_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Io && case.label.contains("_io_edge_"))
        .collect();
    assert_eq!(cases.len(), 27, "unexpected I/O edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on I/O edge cases");
    assert_eq!(tally.interp_err, 0, "rax failed to execute an I/O edge case");
    assert_eq!(
        tally.skipped_asm, 0,
        "I/O edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "I/O edge cases should not feature-skip"
    );
    assert_eq!(tally.ran_for(Feat::Io), 27, "all I/O edge cases should run");
    assert_eq!(tally.compared, 27, "all I/O edge cases should compare");
}

#[test]
fn avx512_kvm_fast_syscall_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::FastSyscall)
        .collect();
    assert_eq!(cases.len(), 8, "unexpected fast syscall corpus size");

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
    assert_eq!(tally.compared, 8, "all fast syscall cases should compare");
}

#[test]
fn avx512_kvm_fast_syscall_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| {
            case.feat == Feat::FastSyscall && case.label.contains("_fast_syscall_edge_")
        })
        .collect();
    assert_eq!(cases.len(), 4, "unexpected fast syscall edge corpus size");

    if !HostFeatures::detect().supports(Feat::FastSyscall) {
        eprintln!("[skip] host lacks SYSCALL or SYSENTER support");
        return;
    }

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on fast syscall edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a fast syscall edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "fast syscall edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "fast syscall edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::FastSyscall),
        4,
        "all fast syscall edge cases should run"
    );
    assert_eq!(
        tally.compared, 4,
        "all fast syscall edge cases should compare"
    );
}

#[test]
fn avx512_kvm_processor_query_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| matches!(case.feat, Feat::Cpuid | Feat::Rdpmc))
        .collect();
    assert_eq!(cases.len(), 27, "unexpected processor-query corpus size");

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
    assert_eq!(tally.ran_for(Feat::Cpuid), 15, "all CPUID cases should run");
    if host.supports(Feat::Rdpmc) {
        assert_eq!(tally.ran_for(Feat::Rdpmc), 12, "all RDPMC cases should run");
        assert_eq!(
            tally.skipped_feature, 0,
            "processor-query cases should not feature-skip"
        );
        assert_eq!(
            tally.compared, 27,
            "all processor-query cases should compare"
        );
    } else {
        assert_eq!(
            tally.skipped_feature, 12,
            "only RDPMC cases should feature-skip"
        );
        assert_eq!(tally.compared, 15, "all CPUID cases should compare");
    }
}

#[test]
fn avx512_kvm_rdpmc_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Rdpmc && case.label.contains("_rdpmc_edge_"))
        .collect();
    assert_eq!(cases.len(), 9, "unexpected RDPMC edge corpus size");

    let host = HostFeatures::detect();
    if !host.supports(Feat::Rdpmc) {
        eprintln!("[skip] host lacks KVM PMU support; RDPMC edge cases will skip");
    }

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on RDPMC edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an RDPMC edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "RDPMC edge corpus produced assembler-rejected cases"
    );
    if host.supports(Feat::Rdpmc) {
        assert_eq!(
            tally.ran_for(Feat::Rdpmc),
            9,
            "all RDPMC edge cases should run"
        );
        assert_eq!(
            tally.skipped_feature, 0,
            "RDPMC edge cases should not feature-skip"
        );
        assert_eq!(tally.compared, 9, "all RDPMC edge cases should compare");
    } else {
        assert_eq!(
            tally.skipped_feature, 9,
            "all RDPMC edge cases should feature-skip"
        );
        assert_eq!(tally.compared, 0, "no RDPMC edge cases should compare");
    }
}

#[test]
fn avx512_kvm_cpuid_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Cpuid && case.label.contains("_cpuid_edge_"))
        .collect();
    assert_eq!(cases.len(), 10, "unexpected CPUID edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on CPUID edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a CPUID edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "CPUID edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "CPUID edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Cpuid),
        10,
        "all CPUID edge cases should run"
    );
    assert_eq!(tally.compared, 10, "all CPUID edge cases should compare");
}

#[test]
fn avx512_kvm_random_tsc_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| matches!(case.feat, Feat::Rdrand | Feat::Rdseed | Feat::Tsc | Feat::Rdtscp))
        .collect();
    assert_eq!(cases.len(), 29, "unexpected random/TSC corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on random/TSC cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a random/TSC case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "random/TSC corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "random/TSC cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Rdrand),
        8,
        "all RDRAND cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Rdseed),
        8,
        "all RDSEED cases should run"
    );
    assert_eq!(tally.ran_for(Feat::Tsc), 7, "all RDTSC cases should run");
    assert_eq!(
        tally.ran_for(Feat::Rdtscp),
        6,
        "all RDTSCP cases should run"
    );
    assert_eq!(tally.compared, 29, "all random/TSC cases should compare");
}

#[test]
fn avx512_kvm_random_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_random_edge_"))
        .collect();
    assert_eq!(cases.len(), 4, "unexpected random edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on random edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a random edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "random edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "random edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Rdrand),
        2,
        "all RDRAND edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Rdseed),
        2,
        "all RDSEED edge cases should run"
    );
    assert_eq!(tally.compared, 4, "all random edge cases should compare");
}

#[test]
fn avx512_kvm_tsc_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_tsc_edge_"))
        .collect();
    assert_eq!(cases.len(), 5, "unexpected TSC edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on TSC edge cases");
    assert_eq!(tally.interp_err, 0, "rax failed to execute a TSC edge case");
    assert_eq!(
        tally.skipped_asm, 0,
        "TSC edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "TSC edge cases should not feature-skip"
    );
    assert_eq!(tally.ran_for(Feat::Tsc), 3, "all RDTSC edge cases should run");
    assert_eq!(
        tally.ran_for(Feat::Rdtscp),
        2,
        "all RDTSCP edge cases should run"
    );
    assert_eq!(tally.compared, 5, "all TSC edge cases should compare");
}

#[test]
fn avx512_kvm_processor_state_management_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| matches!(case.feat, Feat::Fxsave | Feat::Xsave | Feat::Xgetbv1))
        .collect();
    assert_eq!(
        cases.len(),
        28,
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
        14,
        "all FXSAVE/MXCSR cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Xsave),
        11,
        "all XSAVE/XRSTOR cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Xgetbv1),
        3,
        "all XGETBV1 cases should run"
    );
    assert_eq!(
        tally.compared, 28,
        "all processor state-management cases should compare"
    );
}

#[test]
fn avx512_kvm_fxsave_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Fxsave && case.label.contains("_fxsave_edge_"))
        .collect();
    assert_eq!(cases.len(), 8, "unexpected FXSAVE edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on FXSAVE edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a FXSAVE edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "FXSAVE edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "FXSAVE edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Fxsave),
        8,
        "all FXSAVE edge cases should run"
    );
    assert_eq!(tally.compared, 8, "all FXSAVE edge cases should compare");
}

#[test]
fn avx512_kvm_xsave_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_xsave_edge_"))
        .collect();
    assert_eq!(cases.len(), 11, "unexpected XSAVE edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on XSAVE edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an XSAVE edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "XSAVE edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "XSAVE edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Xsave),
        7,
        "all base XSAVE edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::XsaveExt),
        4,
        "all extended XSAVE edge cases should run"
    );
    assert_eq!(tally.compared, 11, "all XSAVE edge cases should compare");
}

#[test]
fn avx512_kvm_xgetbv1_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_xgetbv1_edge_"))
        .collect();
    assert_eq!(cases.len(), 3, "unexpected XGETBV1 edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on XGETBV1 edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an XGETBV1 edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "XGETBV1 edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "XGETBV1 edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Xgetbv1),
        3,
        "all XGETBV1 edge cases should run"
    );
    assert_eq!(tally.compared, 3, "all XGETBV1 edge cases should compare");
}

#[test]
fn avx512_kvm_extended_xsave_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::XsaveExt)
        .collect();
    assert_eq!(cases.len(), 9, "unexpected extended XSAVE corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on extended XSAVE cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an extended XSAVE case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "extended XSAVE corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "extended XSAVE cases should not feature-skip"
    );
    assert_eq!(tally.compared, 9, "all extended XSAVE cases should compare");
}

#[test]
fn avx512_kvm_invpcid_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Invpcid)
        .collect();
    assert_eq!(cases.len(), 11, "unexpected INVPCID corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on INVPCID cases");
    assert_eq!(tally.interp_err, 0, "rax failed to execute an INVPCID case");
    assert_eq!(
        tally.skipped_asm, 0,
        "INVPCID corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "INVPCID cases should not feature-skip"
    );
    assert_eq!(tally.compared, 11, "all INVPCID cases should compare");
}

#[test]
fn avx512_kvm_protection_state_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_protection_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        13,
        "unexpected protection-state edge corpus size"
    );

    let host = HostFeatures::detect();
    if !host.supports(Feat::Smap) {
        eprintln!("[skip] host lacks SMAP support; SMAP edge cases will skip");
    }
    if !host.supports(Feat::Pku) {
        eprintln!("[skip] host lacks PKU support; PKU edge cases will skip");
    }

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on protection-state edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a protection-state edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "protection-state edge corpus produced assembler-rejected cases"
    );

    let expected_smap = if host.supports(Feat::Smap) { 3 } else { 0 };
    let expected_pku = if host.supports(Feat::Pku) { 7 } else { 0 };
    let expected_skips = 10 - expected_smap - expected_pku;
    assert_eq!(
        tally.ran_for(Feat::Smap),
        expected_smap,
        "unexpected SMAP edge run count"
    );
    assert_eq!(
        tally.ran_for(Feat::Pku),
        expected_pku,
        "unexpected PKU edge run count"
    );
    assert_eq!(
        tally.ran_for(Feat::Swapgs),
        3,
        "all SWAPGS edge cases should run"
    );
    assert_eq!(
        tally.skipped_feature, expected_skips,
        "unexpected protection-state feature skips"
    );
    assert_eq!(
        tally.compared,
        expected_smap + expected_pku + 3,
        "all supported protection-state edge cases should compare"
    );
}

#[test]
fn avx512_kvm_pkru_protection_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| {
            case.feat == Feat::Pku && case.label.contains("_protection_edge_")
        })
        .collect();
    assert_eq!(cases.len(), 7, "unexpected PKRU protection edge corpus size");

    if !HostFeatures::detect().supports(Feat::Pku) {
        eprintln!("[skip] host lacks PKU support; PKRU edge cases will skip");
        return;
    }

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on PKRU edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a PKRU edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "PKRU edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "PKRU edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Pku),
        7,
        "all PKRU edge cases should run"
    );
    assert_eq!(tally.compared, 7, "all PKRU edge cases should compare");
}

#[test]
fn avx512_kvm_fsgsbase_segment_memory_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Fsgsbase && case.label.contains("_segment_"))
        .collect();
    assert_eq!(
        cases.len(),
        9,
        "unexpected FSGSBASE segment-memory corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on FSGSBASE segment-memory cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an FSGSBASE segment-memory case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "FSGSBASE segment-memory corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "FSGSBASE segment-memory cases should not feature-skip"
    );
    assert_eq!(
        tally.compared, 9,
        "all FSGSBASE segment-memory cases should compare"
    );
}

#[test]
fn avx512_kvm_fsgsbase_segment_string_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Fsgsbase && case.label.contains("_segstring_"))
        .collect();
    assert_eq!(
        cases.len(),
        7,
        "unexpected FSGSBASE segment-string corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on FSGSBASE segment-string cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an FSGSBASE segment-string case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "FSGSBASE segment-string corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "FSGSBASE segment-string cases should not feature-skip"
    );
    assert_eq!(
        tally.compared, 7,
        "all FSGSBASE segment-string cases should compare"
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
        24,
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
        12,
        "all stack-frame cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::FlagControl),
        12,
        "all flag-control cases should run"
    );
    assert_eq!(
        tally.compared, 24,
        "all stack-frame/flag-control cases should compare"
    );
}

#[test]
fn avx512_kvm_stack_frame_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| {
            case.feat == Feat::StackFrame && case.label.contains("_stack_frame_edge_")
        })
        .collect();
    assert_eq!(cases.len(), 8, "unexpected stack-frame edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on stack-frame edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a stack-frame edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "stack-frame edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "stack-frame edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::StackFrame),
        8,
        "all stack-frame edge cases should run"
    );
    assert_eq!(
        tally.compared, 8,
        "all stack-frame edge cases should compare"
    );
}

#[test]
fn avx512_kvm_flag_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| {
            case.label.contains("_core_flag_edge_")
                || case.label.contains("_flag_control_edge_")
        })
        .collect();
    assert_eq!(cases.len(), 14, "unexpected flag edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on flag edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a flag edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "flag edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "flag edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Core),
        7,
        "all direct core flag edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::FlagControl),
        7,
        "all flag-control edge cases should run"
    );
    assert_eq!(tally.compared, 14, "all flag edge cases should compare");
}

#[test]
fn avx512_kvm_core_moffs_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_moffs_"))
        .collect();
    assert_eq!(cases.len(), 16, "unexpected core moffs corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on core moffs cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core moffs case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core moffs corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core moffs cases should not feature-skip"
    );
    assert_eq!(tally.compared, 16, "all core moffs cases should compare");
}

#[test]
fn avx512_kvm_core_moffs_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_moffs_edge_"))
        .collect();
    assert_eq!(cases.len(), 8, "unexpected core moffs edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on core moffs edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core moffs edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core moffs edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core moffs edge cases should not feature-skip"
    );
    assert_eq!(tally.compared, 8, "all core moffs edge cases should compare");
}

#[test]
fn avx512_kvm_core_address_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_address_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        12,
        "unexpected core effective-address edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on core effective-address edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core effective-address edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core effective-address edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core effective-address edge cases should not feature-skip"
    );
    assert_eq!(
        tally.compared, 12,
        "all core effective-address edge cases should compare"
    );
}

#[test]
fn avx512_kvm_core_data_width_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_data_width_"))
        .collect();
    assert_eq!(cases.len(), 22, "unexpected core data-width corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on core data-width cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core data-width case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core data-width corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core data-width cases should not feature-skip"
    );
    assert_eq!(
        tally.compared, 22,
        "all core data-width cases should compare"
    );
}

#[test]
fn avx512_kvm_core_segment_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_segment_"))
        .collect();
    assert_eq!(cases.len(), 14, "unexpected core segment corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on core segment cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core segment case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core segment corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core segment cases should not feature-skip"
    );
    assert_eq!(tally.compared, 14, "all core segment cases should compare");
}

#[test]
fn avx512_kvm_core_far_pointer_load_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| {
            case.feat == Feat::Core && case.label.contains("_core_far_pointer_load_edge_")
        })
        .collect();
    assert_eq!(
        cases.len(),
        3,
        "unexpected core far-pointer load edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on core far-pointer load edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core far-pointer load edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core far-pointer load edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core far-pointer load edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Core),
        3,
        "all core far-pointer load edge cases should run"
    );
    assert_eq!(
        tally.compared, 3,
        "all core far-pointer load edge cases should compare"
    );
}

#[test]
fn avx512_kvm_core_extend_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_extend_"))
        .collect();
    assert_eq!(cases.len(), 30, "unexpected core extension corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on core extension cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core extension case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core extension corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core extension cases should not feature-skip"
    );
    assert_eq!(
        tally.compared, 30,
        "all core extension cases should compare"
    );
}

#[test]
fn avx512_kvm_core_shift_width_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_shift_width_"))
        .collect();
    assert_eq!(cases.len(), 28, "unexpected core shift-width corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on core shift-width cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core shift-width case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core shift-width corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core shift-width cases should not feature-skip"
    );
    assert_eq!(
        tally.compared, 28,
        "all core shift-width cases should compare"
    );
}

#[test]
fn avx512_kvm_core_rotate_double_shift_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| {
            case.feat == Feat::Core
                && (case.label.contains("_core_rotate_edge_")
                    || case.label.contains("_core_double_shift_edge_"))
        })
        .collect();
    assert_eq!(
        cases.len(),
        22,
        "unexpected core rotate/double-shift edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on core rotate/double-shift edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core rotate/double-shift edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core rotate/double-shift edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core rotate/double-shift edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Core),
        22,
        "all core rotate/double-shift edge cases should run"
    );
    assert_eq!(
        tally.compared, 22,
        "all core rotate/double-shift edge cases should compare"
    );
}

#[test]
fn avx512_kvm_core_incdec_width_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_incdec_width_"))
        .collect();
    assert_eq!(cases.len(), 14, "unexpected core inc/dec-width corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on core inc/dec-width cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core inc/dec-width case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core inc/dec-width corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core inc/dec-width cases should not feature-skip"
    );
    assert_eq!(
        tally.compared, 14,
        "all core inc/dec-width cases should compare"
    );
}

#[test]
fn avx512_kvm_core_group_width_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_group_width_"))
        .collect();
    assert_eq!(cases.len(), 54, "unexpected core group-width corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on core group-width cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core group-width case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core group-width corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core group-width cases should not feature-skip"
    );
    assert_eq!(
        tally.compared, 54,
        "all core group-width cases should compare"
    );
}

#[test]
fn avx512_kvm_core_atomic_width_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_atomic_width_"))
        .collect();
    assert_eq!(cases.len(), 22, "unexpected core atomic-width corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on core atomic-width cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core atomic-width case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core atomic-width corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core atomic-width cases should not feature-skip"
    );
    assert_eq!(
        tally.compared, 22,
        "all core atomic-width cases should compare"
    );
}

#[test]
fn avx512_kvm_cmpxchg_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_cmpxchg_edge_"))
        .collect();
    assert_eq!(cases.len(), 8, "unexpected CMPXCHG edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on CMPXCHG edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a CMPXCHG edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "CMPXCHG edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "CMPXCHG edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Cx8),
        4,
        "all CMPXCHG8B edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Cx16),
        4,
        "all CMPXCHG16B edge cases should run"
    );
    assert_eq!(tally.compared, 8, "all CMPXCHG edge cases should compare");
}

#[test]
fn avx512_kvm_core_bit_width_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_bit_width_"))
        .collect();
    assert_eq!(cases.len(), 26, "unexpected core bit-width corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on core bit-width cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core bit-width case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core bit-width corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core bit-width cases should not feature-skip"
    );
    assert_eq!(
        tally.compared, 26,
        "all core bit-width cases should compare"
    );
}

#[test]
fn avx512_kvm_core_condition_width_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_condition_width_"))
        .collect();
    assert_eq!(
        cases.len(),
        28,
        "unexpected core condition-width corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on core condition-width cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core condition-width case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core condition-width corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core condition-width cases should not feature-skip"
    );
    assert_eq!(
        tally.compared, 28,
        "all core condition-width cases should compare"
    );
}

#[test]
fn avx512_kvm_core_string_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_string"))
        .collect();
    assert_eq!(cases.len(), 66, "unexpected core string corpus size");

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
    assert_eq!(tally.compared, 66, "all core string cases should compare");
}

#[test]
fn avx512_kvm_core_string_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_string_edge_"))
        .collect();
    assert_eq!(cases.len(), 12, "unexpected core string edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on core string edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core string edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core string edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core string edge cases should not feature-skip"
    );
    assert_eq!(
        tally.compared, 12,
        "all core string edge cases should compare"
    );
}

#[test]
fn avx512_kvm_core_lock_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_lock"))
        .collect();
    assert_eq!(cases.len(), 16, "unexpected core LOCK corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on core LOCK cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core LOCK case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core LOCK corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core LOCK cases should not feature-skip"
    );
    assert_eq!(tally.compared, 16, "all core LOCK cases should compare");
}

#[test]
fn avx512_kvm_core_implicit_operand_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_implicit"))
        .collect();
    assert_eq!(
        cases.len(),
        18,
        "unexpected core implicit-operand corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on core implicit-operand cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core implicit-operand case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core implicit-operand corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core implicit-operand cases should not feature-skip"
    );
    assert_eq!(
        tally.compared, 18,
        "all core implicit-operand cases should compare"
    );
}

#[test]
fn avx512_kvm_core_muldiv_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_muldiv_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        32,
        "unexpected core multiply/divide edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on core multiply/divide edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core multiply/divide edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core multiply/divide edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core multiply/divide edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Core),
        32,
        "all core multiply/divide edge cases should run"
    );
    assert_eq!(
        tally.compared, 32,
        "all core multiply/divide edge cases should compare"
    );
}

#[test]
fn avx512_kvm_core_control_transfer_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_control"))
        .collect();
    assert_eq!(
        cases.len(),
        21,
        "unexpected core control-transfer corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on core control-transfer cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core control-transfer case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core control-transfer corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core control-transfer cases should not feature-skip"
    );
    assert_eq!(
        tally.compared, 21,
        "all core control-transfer cases should compare"
    );
}

#[test]
fn avx512_kvm_core_iret_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| {
            case.feat == Feat::Core && case.label.contains("_core_control_iret_edge_")
        })
        .collect();
    assert_eq!(cases.len(), 3, "unexpected core IRET edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on core IRET edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core IRET edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core IRET edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core IRET edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Core),
        3,
        "all core IRET edge cases should run"
    );
    assert_eq!(tally.compared, 3, "all core IRET edge cases should compare");
}

#[test]
fn avx512_kvm_core_far_return_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| {
            case.feat == Feat::Core && case.label.contains("_core_far_return_edge_")
        })
        .collect();
    assert_eq!(cases.len(), 3, "unexpected core far-return edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on core far-return edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core far-return edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core far-return edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core far-return edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Core),
        3,
        "all core far-return edge cases should run"
    );
    assert_eq!(
        tally.compared, 3,
        "all core far-return edge cases should compare"
    );
}

#[test]
fn avx512_kvm_core_far_control_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| {
            case.feat == Feat::Core && case.label.contains("_core_far_control_edge_")
        })
        .collect();
    assert_eq!(
        cases.len(),
        3,
        "unexpected core far-control edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on core far-control edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core far-control edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core far-control edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core far-control edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Core),
        3,
        "all core far-control edge cases should run"
    );
    assert_eq!(
        tally.compared, 3,
        "all core far-control edge cases should compare"
    );
}

#[test]
fn avx512_kvm_core_accumulator_immediate_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Core && case.label.contains("_core_accum_"))
        .collect();
    assert_eq!(
        cases.len(),
        36,
        "unexpected core accumulator-immediate corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on core accumulator-immediate cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a core accumulator-immediate case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "core accumulator-immediate corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "core accumulator-immediate cases should not feature-skip"
    );
    assert_eq!(
        tally.compared, 36,
        "all core accumulator-immediate cases should compare"
    );
}

#[test]
fn avx512_kvm_sse_minmax_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_sse_minmax_edge_"))
        .collect();
    assert_eq!(cases.len(), 16, "unexpected SSE min/max edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on SSE min/max edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an SSE min/max edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SSE min/max edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SSE min/max edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse),
        8,
        "all SSE min/max edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse2),
        8,
        "all SSE2 min/max edge cases should run"
    );
    assert_eq!(
        tally.compared, 16,
        "all SSE min/max edge cases should compare"
    );
}

#[test]
fn avx512_kvm_simd_fp_arith_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_simd_fp_arith_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        112,
        "unexpected SIMD FP arithmetic edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on SIMD FP arithmetic edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a SIMD FP arithmetic edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SIMD FP arithmetic edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SIMD FP arithmetic edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse),
        16,
        "all SSE arithmetic edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse2),
        16,
        "all SSE2 arithmetic edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx),
        32,
        "all AVX arithmetic edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::F),
        32,
        "all AVX-512 arithmetic edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Fp16),
        16,
        "all AVX-512-FP16 arithmetic edge cases should run"
    );
    assert_eq!(
        tally.compared, 112,
        "all SIMD FP arithmetic edge cases should compare"
    );
}

#[test]
fn avx512_kvm_simd_fp_sqrt_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_simd_fp_sqrt_edge_"))
        .collect();
    assert_eq!(cases.len(), 24, "unexpected SIMD FP sqrt edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on SIMD FP sqrt edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a SIMD FP sqrt edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SIMD FP sqrt edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SIMD FP sqrt edge cases should not feature-skip"
    );
    assert_eq!(tally.ran_for(Feat::Sse), 4, "all SSE sqrt edge cases should run");
    assert_eq!(
        tally.ran_for(Feat::Sse2),
        4,
        "all SSE2 sqrt edge cases should run"
    );
    assert_eq!(tally.ran_for(Feat::Avx), 8, "all AVX sqrt edge cases should run");
    assert_eq!(
        tally.ran_for(Feat::F),
        8,
        "all AVX-512 sqrt edge cases should run"
    );
    assert_eq!(
        tally.compared, 24,
        "all SIMD FP sqrt edge cases should compare"
    );
}

#[test]
fn avx512_kvm_simd_fp_minmax_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_simd_fp_minmax_edge_"))
        .collect();
    assert_eq!(cases.len(), 16, "unexpected SIMD FP min/max edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on SIMD FP min/max edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a SIMD FP min/max edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SIMD FP min/max edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SIMD FP min/max edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx),
        8,
        "all AVX min/max edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::F),
        8,
        "all AVX-512 min/max edge cases should run"
    );
    assert_eq!(
        tally.compared, 16,
        "all SIMD FP min/max edge cases should compare"
    );
}

#[test]
fn avx512_kvm_simd_fp_compare_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_simd_fp_compare_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        16,
        "unexpected SIMD FP compare edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on SIMD FP compare edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a SIMD FP compare edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SIMD FP compare edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SIMD FP compare edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse),
        2,
        "all SSE compare edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse2),
        2,
        "all SSE2 compare edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx),
        8,
        "all AVX compare edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::F),
        4,
        "all AVX-512 compare edge cases should run"
    );
    assert_eq!(
        tally.compared, 16,
        "all SIMD FP compare edge cases should compare"
    );
}

#[test]
fn avx512_kvm_sse2_transfer_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_sse2_transfer_"))
        .collect();
    assert_eq!(cases.len(), 8, "unexpected SSE2 transfer corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on SSE2 transfer cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an SSE2 transfer case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SSE2 transfer corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SSE2 transfer cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse2),
        8,
        "all SSE2 transfer cases should run"
    );
    assert_eq!(tally.compared, 8, "all SSE2 transfer cases should compare");
}

#[test]
fn avx512_kvm_sse2_lane_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_sse2_lane_"))
        .collect();
    assert_eq!(cases.len(), 7, "unexpected SSE2 lane corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on SSE2 lane cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an SSE2 lane case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SSE2 lane corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SSE2 lane cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse2),
        7,
        "all SSE2 lane cases should run"
    );
    assert_eq!(tally.compared, 7, "all SSE2 lane cases should compare");
}

#[test]
fn avx512_kvm_sse2_shift_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_sse2_shift_"))
        .collect();
    assert_eq!(cases.len(), 24, "unexpected SSE2 shift corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on SSE2 shift cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an SSE2 shift case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SSE2 shift corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SSE2 shift cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse2),
        24,
        "all SSE2 shift cases should run"
    );
    assert_eq!(tally.compared, 24, "all SSE2 shift cases should compare");
}

#[test]
fn avx512_kvm_legacy_packed_misc_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_packed_misc_"))
        .collect();
    assert_eq!(cases.len(), 17, "unexpected legacy packed misc corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on legacy packed misc cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a legacy packed misc case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "legacy packed misc corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "legacy packed misc cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse2),
        13,
        "all SSE2 packed misc cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Mmx),
        4,
        "all MMX packed misc cases should run"
    );
    assert_eq!(
        tally.compared, 17,
        "all legacy packed misc cases should compare"
    );
}

#[test]
fn avx512_kvm_integer_saturation_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_int_sat_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        53,
        "unexpected integer saturation edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on integer saturation edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an integer saturation edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "integer saturation edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "integer saturation edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Mmx),
        6,
        "all MMX saturation edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse2),
        12,
        "all SSE2 saturation edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Ssse3),
        6,
        "all SSSE3 saturation edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse41),
        2,
        "all SSE4.1 saturation edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx2),
        15,
        "all AVX2 saturation edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Bw),
        10,
        "all AVX-512BW saturation edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::F),
        2,
        "all AVX-512F saturation edge cases should run"
    );
    assert_eq!(
        tally.compared, 53,
        "all integer saturation edge cases should compare"
    );
}

#[test]
fn avx512_kvm_integer_order_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_int_order_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        58,
        "unexpected integer order edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on integer order edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an integer order edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "integer order edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "integer order edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse2),
        8,
        "all SSE2 order edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse41),
        8,
        "all SSE4.1 order edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse42),
        2,
        "all SSE4.2 order edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx2),
        16,
        "all AVX2 order edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Bw),
        12,
        "all AVX-512BW order edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::F),
        12,
        "all AVX-512F order edge cases should run"
    );
    assert_eq!(
        tally.compared, 58,
        "all integer order edge cases should compare"
    );
}

#[test]
fn avx512_kvm_integer_shift_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_int_shift_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        118,
        "unexpected integer shift edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on integer shift edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an integer shift edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "integer shift edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "integer shift edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Mmx),
        16,
        "all MMX shift edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse2),
        14,
        "all SSE2 shift edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx2),
        21,
        "all AVX2 shift edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Bw),
        13,
        "all AVX-512BW shift edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::F),
        34,
        "all AVX-512F shift edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Vbmi2),
        12,
        "all AVX-512 VBMI2 shift edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Dq),
        8,
        "all AVX-512DQ mask shift edge cases should run"
    );
    assert_eq!(
        tally.compared, 118,
        "all integer shift edge cases should compare"
    );
}

#[test]
fn avx512_kvm_legacy_convert_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_legacy_convert_"))
        .collect();
    assert_eq!(cases.len(), 18, "unexpected legacy conversion corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on legacy conversion cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a legacy conversion case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "legacy conversion corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "legacy conversion cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse),
        6,
        "all SSE legacy conversion cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse2),
        12,
        "all SSE2 legacy conversion cases should run"
    );
    assert_eq!(
        tally.compared, 18,
        "all legacy conversion cases should compare"
    );
}

#[test]
fn avx512_kvm_legacy_cmp_predicate_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_legacy_cmp_pred_"))
        .collect();
    assert_eq!(
        cases.len(),
        16,
        "unexpected legacy compare-predicate corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on legacy compare-predicate cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a legacy compare-predicate case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "legacy compare-predicate corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "legacy compare-predicate cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse),
        8,
        "all SSE compare-predicate cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse2),
        8,
        "all SSE2 compare-predicate cases should run"
    );
    assert_eq!(
        tally.ran_mnemonic("cmpps"),
        4,
        "all CMPPS predicate cases should run"
    );
    assert_eq!(
        tally.ran_mnemonic("cmpss"),
        4,
        "all CMPSS predicate cases should run"
    );
    assert_eq!(
        tally.ran_mnemonic("cmppd"),
        4,
        "all CMPPD predicate cases should run"
    );
    assert_eq!(
        tally.ran_mnemonic("cmpsd"),
        4,
        "all CMPSD predicate cases should run"
    );
    assert_eq!(
        tally.compared, 16,
        "all legacy compare-predicate cases should compare"
    );
}

#[test]
fn avx512_kvm_scalar_convert_width_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_scalar_width_"))
        .collect();
    assert_eq!(
        cases.len(),
        12,
        "unexpected scalar conversion width corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on scalar conversion width cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a scalar conversion width case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "scalar conversion width corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "scalar conversion width cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse),
        6,
        "all SSE scalar conversion width cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse2),
        6,
        "all SSE2 scalar conversion width cases should run"
    );
    assert_eq!(
        tally.compared, 12,
        "all scalar conversion width cases should compare"
    );
}

#[test]
fn avx512_kvm_simd_convert_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_simd_convert_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        38,
        "unexpected SIMD conversion edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on SIMD conversion edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a SIMD conversion edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SIMD conversion edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SIMD conversion edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse),
        6,
        "all SSE conversion edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse2),
        12,
        "all SSE2 conversion edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx),
        14,
        "all AVX conversion edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::F16c),
        6,
        "all F16C conversion edge cases should run"
    );
    assert_eq!(
        tally.compared, 38,
        "all SIMD conversion edge cases should compare"
    );
}

#[test]
fn avx512_kvm_simd_round_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_simd_round_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        24,
        "unexpected SIMD rounding edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on SIMD rounding edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a SIMD rounding edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SIMD rounding edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SIMD rounding edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse41),
        12,
        "all SSE4.1 rounding edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx),
        12,
        "all AVX rounding edge cases should run"
    );
    assert_eq!(
        tally.compared, 24,
        "all SIMD rounding edge cases should compare"
    );
}

#[test]
fn avx512_kvm_simd_dot_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_simd_dot_edge_"))
        .collect();
    assert_eq!(cases.len(), 21, "unexpected SIMD dot edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on SIMD dot edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a SIMD dot edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SIMD dot edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SIMD dot edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse41),
        10,
        "all SSE4.1 dot edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx),
        11,
        "all AVX dot edge cases should run"
    );
    assert_eq!(
        tally.compared, 21,
        "all SIMD dot edge cases should compare"
    );
}

#[test]
fn avx512_kvm_simd_mpsad_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_simd_mpsad_edge_"))
        .collect();
    assert_eq!(cases.len(), 12, "unexpected SIMD MPSADBW edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on SIMD MPSADBW edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a SIMD MPSADBW edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SIMD MPSADBW edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SIMD MPSADBW edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse41),
        6,
        "all SSE4.1 MPSADBW edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx2),
        6,
        "all AVX2 MPSADBW edge cases should run"
    );
    assert_eq!(
        tally.compared, 12,
        "all SIMD MPSADBW edge cases should compare"
    );
}

#[test]
fn avx512_kvm_simd_horizontal_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_simd_horizontal_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        28,
        "unexpected SIMD horizontal edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on SIMD horizontal edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a SIMD horizontal edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SIMD horizontal edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SIMD horizontal edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx),
        10,
        "all AVX horizontal edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx2),
        18,
        "all AVX2 horizontal edge cases should run"
    );
    assert_eq!(
        tally.compared, 28,
        "all SIMD horizontal edge cases should compare"
    );
}

#[test]
fn avx512_kvm_simd_shuffle_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_simd_shuffle_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        8,
        "unexpected SIMD shuffle edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on SIMD shuffle edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a SIMD shuffle edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SIMD shuffle edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SIMD shuffle edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx2),
        8,
        "all AVX2 shuffle edge cases should run"
    );
    assert_eq!(
        tally.compared, 8,
        "all SIMD shuffle edge cases should compare"
    );
}

#[test]
fn avx512_kvm_sse41_operand_form_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_sse41_operand_"))
        .collect();
    assert_eq!(
        cases.len(),
        21,
        "unexpected SSE4.1 operand-form corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on SSE4.1 operand-form cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an SSE4.1 operand-form case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SSE4.1 operand-form corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SSE4.1 operand-form cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse41),
        21,
        "all SSE4.1 operand-form cases should run"
    );
    assert_eq!(
        tally.compared, 21,
        "all SSE4.1 operand-form cases should compare"
    );
}

#[test]
fn avx512_kvm_sse41_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_sse41_edge_"))
        .collect();
    assert_eq!(cases.len(), 20, "unexpected SSE4.1 edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on SSE4.1 edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an SSE4.1 edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SSE4.1 edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SSE4.1 edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse41),
        20,
        "all SSE4.1 edge cases should run"
    );
    assert_eq!(tally.compared, 20, "all SSE4.1 edge cases should compare");
}

#[test]
fn avx512_kvm_sse42_string_width_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_sse42_string_width_"))
        .collect();
    assert_eq!(cases.len(), 24, "unexpected SSE4.2 string corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on SSE4.2 string cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an SSE4.2 string case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SSE4.2 string corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SSE4.2 string cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse42),
        24,
        "all SSE4.2 string cases should run"
    );
    assert_eq!(
        tally.compared, 24,
        "all SSE4.2 string cases should compare"
    );
}

#[test]
fn avx512_kvm_scalar_bit_operand_form_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| {
            case.label.contains("_bmi1_operand_")
                || case.label.contains("_bmi2_operand_")
                || case.label.contains("lzcnt_operand_")
        })
        .collect();
    assert_eq!(
        cases.len(),
        30,
        "unexpected scalar bit operand-form corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on scalar bit operand-form cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a scalar bit operand-form case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "scalar bit operand-form corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "scalar bit operand-form cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Bmi1),
        12,
        "all BMI1 operand-form cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Bmi2),
        16,
        "all BMI2 operand-form cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Lzcnt),
        2,
        "all LZCNT operand-form cases should run"
    );
    assert_eq!(
        tally.compared, 30,
        "all scalar bit operand-form cases should compare"
    );
}

#[test]
fn avx512_kvm_scalar_bit_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_scalar_bit_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        35,
        "unexpected scalar bit edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on scalar bit edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a scalar bit edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "scalar bit edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "scalar bit edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Popcnt),
        4,
        "all POPCNT scalar bit edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Bmi1),
        9,
        "all BMI1 scalar bit edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Lzcnt),
        4,
        "all LZCNT scalar bit edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Bmi2),
        18,
        "all BMI2 scalar bit edge cases should run"
    );
    assert_eq!(
        tally.compared, 35,
        "all scalar bit edge cases should compare"
    );
}

#[test]
fn avx512_kvm_scalar_crc_movbe_operand_form_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| {
            case.label.contains("_crc32_operand_") || case.label.contains("_movbe_operand_")
        })
        .collect();
    assert_eq!(
        cases.len(),
        14,
        "unexpected scalar CRC/MOVBE operand-form corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on scalar CRC/MOVBE operand-form cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a scalar CRC/MOVBE operand-form case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "scalar CRC/MOVBE operand-form corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "scalar CRC/MOVBE operand-form cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Crc32),
        8,
        "all CRC32 operand-form cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Movbe),
        6,
        "all MOVBE operand-form cases should run"
    );
    assert_eq!(
        tally.compared, 14,
        "all scalar CRC/MOVBE operand-form cases should compare"
    );
}

#[test]
fn avx512_kvm_movbe_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Movbe && case.label.contains("_movbe_edge_"))
        .collect();
    assert_eq!(cases.len(), 10, "unexpected MOVBE edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on MOVBE edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a MOVBE edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "MOVBE edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "MOVBE edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Movbe),
        10,
        "all MOVBE edge cases should run"
    );
    assert_eq!(tally.compared, 10, "all MOVBE edge cases should compare");
}

#[test]
fn avx512_kvm_crc32_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_crc32_edge_"))
        .collect();
    assert_eq!(cases.len(), 12, "unexpected CRC32 edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on CRC32 edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a CRC32 edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "CRC32 edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "CRC32 edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Crc32),
        12,
        "all CRC32 edge cases should run"
    );
    assert_eq!(
        tally.compared, 12,
        "all CRC32 edge cases should compare"
    );
}

#[test]
fn avx512_kvm_movdir_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| matches!(case.feat, Feat::Movdiri | Feat::Movdir64b))
        .collect();
    assert_eq!(cases.len(), 22, "unexpected MOVDIR corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on MOVDIR cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a MOVDIR case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "MOVDIR corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "MOVDIR cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Movdiri),
        13,
        "all MOVDIRI cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Movdir64b),
        9,
        "all MOVDIR64B cases should run"
    );
    assert_eq!(tally.compared, 22, "all MOVDIR cases should compare");
}

#[test]
fn avx512_kvm_movdir_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_movdir_edge_"))
        .collect();
    assert_eq!(cases.len(), 10, "unexpected MOVDIR edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on MOVDIR edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a MOVDIR edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "MOVDIR edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "MOVDIR edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Movdiri),
        5,
        "all MOVDIRI edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Movdir64b),
        5,
        "all MOVDIR64B edge cases should run"
    );
    assert_eq!(
        tally.compared, 10,
        "all MOVDIR edge cases should compare"
    );
}

#[test]
fn avx512_kvm_adx_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Adx)
        .collect();
    assert_eq!(cases.len(), 21, "unexpected ADX corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on ADX cases");
    assert_eq!(tally.interp_err, 0, "rax failed to execute an ADX case");
    assert_eq!(
        tally.skipped_asm, 0,
        "ADX corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "ADX cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Adx),
        21,
        "all ADX cases should run"
    );
    assert_eq!(tally.compared, 21, "all ADX cases should compare");
}

#[test]
fn avx512_kvm_adx_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_adx_edge_"))
        .collect();
    assert_eq!(cases.len(), 7, "unexpected ADX edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on ADX edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an ADX edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "ADX edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "ADX edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Adx),
        7,
        "all ADX edge cases should run"
    );
    assert_eq!(tally.compared, 7, "all ADX edge cases should compare");
}

#[test]
fn avx512_kvm_adx_operand_form_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_adx_operand_"))
        .collect();
    assert_eq!(cases.len(), 8, "unexpected ADX operand-form corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on ADX operand-form cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an ADX operand-form case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "ADX operand-form corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "ADX operand-form cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Adx),
        8,
        "all ADX operand-form cases should run"
    );
    assert_eq!(
        tally.compared, 8,
        "all ADX operand-form cases should compare"
    );
}

#[test]
fn avx512_kvm_f16c_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::F16c)
        .collect();
    assert_eq!(cases.len(), 28, "unexpected F16C corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on F16C cases");
    assert_eq!(tally.interp_err, 0, "rax failed to execute an F16C case");
    assert_eq!(
        tally.skipped_asm,
        0,
        "F16C corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "F16C cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::F16c),
        28,
        "all F16C cases should run"
    );
    assert_eq!(tally.compared, 28, "all F16C cases should compare");
}

#[test]
fn avx512_kvm_bf16_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_bf16_edge_"))
        .collect();
    assert_eq!(cases.len(), 12, "unexpected AVX-512-BF16 edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on AVX-512-BF16 edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an AVX-512-BF16 edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "AVX-512-BF16 edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "AVX-512-BF16 edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Bf16),
        12,
        "all AVX-512-BF16 edge cases should run"
    );
    assert_eq!(
        tally.compared, 12,
        "all AVX-512-BF16 edge cases should compare"
    );
}

#[test]
fn avx512_kvm_fp16_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_fp16_edge_"))
        .collect();
    assert_eq!(cases.len(), 22, "unexpected AVX-512-FP16 edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on AVX-512-FP16 edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an AVX-512-FP16 edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "AVX-512-FP16 edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "AVX-512-FP16 edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Fp16),
        22,
        "all AVX-512-FP16 edge cases should run"
    );
    assert_eq!(
        tally.compared, 22,
        "all AVX-512-FP16 edge cases should compare"
    );
}

#[test]
fn avx512_kvm_avx512_predicate_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_avx512_predicate_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        36,
        "unexpected AVX-512 predicate/classification edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on AVX-512 predicate/classification edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an AVX-512 predicate/classification edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "AVX-512 predicate/classification edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "AVX-512 predicate/classification edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Cd),
        8,
        "all AVX-512CD predicate edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::F),
        8,
        "all AVX-512F predicate edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Bw),
        4,
        "all AVX-512BW predicate edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Dq),
        16,
        "all AVX-512DQ predicate edge cases should run"
    );
    assert_eq!(
        tally.compared, 36,
        "all AVX-512 predicate/classification edge cases should compare"
    );
}

#[test]
fn avx512_kvm_avx512_compare_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_avx512_cmp_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        40,
        "unexpected AVX-512 compare edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on AVX-512 compare edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an AVX-512 compare edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "AVX-512 compare edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "AVX-512 compare edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::F),
        24,
        "all AVX-512F compare edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Bw),
        8,
        "all AVX-512BW compare edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Fp16),
        8,
        "all AVX-512-FP16 compare edge cases should run"
    );
    assert_eq!(
        tally.compared, 40,
        "all AVX-512 compare edge cases should compare"
    );
}

#[test]
fn avx512_kvm_avx512_convert_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_avx512_convert_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        49,
        "unexpected AVX-512 conversion edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on AVX-512 conversion edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an AVX-512 conversion edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "AVX-512 conversion edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "AVX-512 conversion edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::F),
        27,
        "all AVX-512F conversion edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Dq),
        22,
        "all AVX-512DQ conversion edge cases should run"
    );
    assert_eq!(
        tally.compared, 49,
        "all AVX-512 conversion edge cases should compare"
    );
}

#[test]
fn avx512_kvm_avx_vnni_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::AvxVnni)
        .collect();
    assert_eq!(cases.len(), 48, "unexpected AVX-VNNI corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on AVX-VNNI cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an AVX-VNNI case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "AVX-VNNI corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "AVX-VNNI cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::AvxVnni),
        48,
        "all AVX-VNNI cases should run"
    );
    assert_eq!(tally.compared, 48, "all AVX-VNNI cases should compare");
}

#[test]
fn avx512_kvm_vnni_ifma_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_vnni_ifma_edge_"))
        .collect();
    assert_eq!(cases.len(), 8, "unexpected VNNI/IFMA edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on VNNI/IFMA edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a VNNI/IFMA edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "VNNI/IFMA edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "VNNI/IFMA edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Vnni),
        4,
        "all AVX-512 VNNI edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Ifma),
        4,
        "all AVX-512 IFMA edge cases should run"
    );
    assert_eq!(
        tally.compared, 8,
        "all VNNI/IFMA edge cases should compare"
    );
}

#[test]
fn avx512_kvm_bitalg_popcnt_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_popcnt_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        10,
        "unexpected BITALG/VPOPCNTDQ edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on BITALG/VPOPCNTDQ edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a BITALG/VPOPCNTDQ edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "BITALG/VPOPCNTDQ edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "BITALG/VPOPCNTDQ edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Bitalg),
        6,
        "all BITALG edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Vpopcntdq),
        4,
        "all VPOPCNTDQ edge cases should run"
    );
    assert_eq!(
        tally.compared, 10,
        "all BITALG/VPOPCNTDQ edge cases should compare"
    );
}

#[test]
fn avx512_kvm_vbmi_selector_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_selector_edge_"))
        .collect();
    assert_eq!(cases.len(), 14, "unexpected VBMI selector edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on VBMI selector edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a VBMI selector edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "VBMI selector edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "VBMI selector edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Vbmi),
        8,
        "all AVX-512 VBMI selector edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Vbmi2),
        6,
        "all AVX-512 VBMI2 selector edge cases should run"
    );
    assert_eq!(
        tally.compared, 14,
        "all VBMI selector edge cases should compare"
    );
}

#[test]
fn avx512_kvm_gfni_crypto_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Gfni)
        .collect();
    assert_eq!(cases.len(), 96, "unexpected GFNI corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on GFNI cases");
    assert_eq!(tally.interp_err, 0, "rax failed to execute a GFNI case");
    assert_eq!(
        tally.skipped_asm, 0,
        "GFNI corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "GFNI cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Gfni),
        96,
        "all GFNI cases should run"
    );
    assert_eq!(tally.compared, 96, "all GFNI cases should compare");
}

#[test]
fn avx512_kvm_vaes_crypto_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Vaes)
        .collect();
    assert_eq!(cases.len(), 80, "unexpected VAES corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on VAES cases");
    assert_eq!(tally.interp_err, 0, "rax failed to execute a VAES case");
    assert_eq!(
        tally.skipped_asm, 0,
        "VAES corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "VAES cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Vaes),
        80,
        "all VAES cases should run"
    );
    assert_eq!(tally.compared, 80, "all VAES cases should compare");
}

#[test]
fn avx512_kvm_vpclmulqdq_crypto_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Vpclmulqdq)
        .collect();
    assert_eq!(cases.len(), 53, "unexpected VPCLMULQDQ corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on VPCLMULQDQ cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a VPCLMULQDQ case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "VPCLMULQDQ corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "VPCLMULQDQ cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Vpclmulqdq),
        53,
        "all VPCLMULQDQ cases should run"
    );
    assert_eq!(
        tally.compared, 53,
        "all VPCLMULQDQ cases should compare"
    );
}

#[test]
fn avx512_kvm_crypto_edge_operand_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_crypto_edge_"))
        .collect();
    assert_eq!(cases.len(), 18, "unexpected crypto edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on crypto edge operand cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a crypto edge operand case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "crypto edge operand corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "crypto edge operand cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Gfni),
        6,
        "all GFNI edge operand cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Vaes),
        8,
        "all VAES edge operand cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Vpclmulqdq),
        4,
        "all VPCLMULQDQ edge operand cases should run"
    );
    assert_eq!(
        tally.compared, 18,
        "all crypto edge operand cases should compare"
    );
}

#[test]
fn avx512_kvm_legacy_xmm_crypto_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_legacy_xmm_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        14,
        "unexpected legacy XMM crypto edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on legacy XMM crypto edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a legacy XMM crypto edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "legacy XMM crypto edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "legacy XMM crypto edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Aes),
        10,
        "all AES-NI legacy XMM edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Pclmulqdq),
        4,
        "all PCLMULQDQ legacy XMM edge cases should run"
    );
    assert_eq!(
        tally.compared, 14,
        "all legacy XMM crypto edge cases should compare"
    );
}

#[test]
fn avx512_kvm_legacy_gfni_sha_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_legacy_gfni_sha_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        20,
        "unexpected legacy GFNI/SHA edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on legacy GFNI/SHA edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a legacy GFNI/SHA edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "legacy GFNI/SHA edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "legacy GFNI/SHA edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Gfni),
        6,
        "all legacy GFNI edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Sha),
        14,
        "all SHA-NI edge cases should run"
    );
    assert_eq!(
        tally.compared, 20,
        "all legacy GFNI/SHA edge cases should compare"
    );
}

#[test]
fn avx512_kvm_aes_legacy_crypto_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Aes)
        .collect();
    assert_eq!(cases.len(), 46, "unexpected AES legacy corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on AES legacy cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an AES legacy case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "AES legacy corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "AES legacy cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Aes),
        46,
        "all AES legacy cases should run"
    );
    assert_eq!(
        tally.compared, 46,
        "all AES legacy cases should compare"
    );
}

#[test]
fn avx512_kvm_pclmulqdq_legacy_crypto_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Pclmulqdq)
        .collect();
    assert_eq!(
        cases.len(),
        25,
        "unexpected PCLMULQDQ legacy corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on PCLMULQDQ legacy cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a PCLMULQDQ legacy case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "PCLMULQDQ legacy corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "PCLMULQDQ legacy cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Pclmulqdq),
        25,
        "all PCLMULQDQ legacy cases should run"
    );
    assert_eq!(
        tally.compared, 25,
        "all PCLMULQDQ legacy cases should compare"
    );
}

#[test]
fn avx512_kvm_sha_ni_legacy_crypto_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Sha)
        .collect();
    assert_eq!(cases.len(), 74, "unexpected SHA-NI legacy corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on SHA-NI legacy cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a SHA-NI legacy case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SHA-NI legacy corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SHA-NI legacy cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sha),
        74,
        "all SHA-NI legacy cases should run"
    );
    assert_eq!(
        tally.compared, 74,
        "all SHA-NI legacy cases should compare"
    );
}

#[test]
fn avx512_kvm_vex_permute_blend_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_vex_perm_blend_edge_"))
        .collect();
    assert_eq!(
        cases.len(),
        36,
        "unexpected VEX permute/blend edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on VEX permute/blend edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a VEX permute/blend edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "VEX permute/blend edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "VEX permute/blend edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx),
        17,
        "all AVX permute/blend edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx2),
        19,
        "all AVX2 permute/blend edge cases should run"
    );
    assert_eq!(
        tally.compared, 36,
        "all VEX permute/blend edge cases should compare"
    );
}

#[test]
fn avx512_kvm_vex_avx2_permute_selector_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_vex_perm_blend_edge_avx2_selector_"))
        .collect();
    assert_eq!(
        cases.len(),
        8,
        "unexpected VEX AVX2 permute selector edge corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on VEX AVX2 permute selector edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a VEX AVX2 permute selector edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "VEX AVX2 permute selector edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "VEX AVX2 permute selector edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx2),
        8,
        "all VEX AVX2 permute selector edge cases should run"
    );
    assert_eq!(
        tally.compared, 8,
        "all VEX AVX2 permute selector edge cases should compare"
    );
}

#[test]
fn avx512_kvm_vex_avx_data_movement_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_avx_data_"))
        .collect();
    assert_eq!(
        cases.len(),
        43,
        "unexpected VEX AVX data-movement corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on VEX AVX data-movement cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a VEX AVX data-movement case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "VEX AVX data-movement corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "VEX AVX data-movement cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx),
        43,
        "all VEX AVX data-movement cases should run"
    );
    assert_eq!(
        tally.compared, 43,
        "all VEX AVX data-movement cases should compare"
    );
}

#[test]
fn avx512_kvm_vex_avx_data_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_avx_data_edge_"))
        .collect();
    assert_eq!(cases.len(), 9, "unexpected VEX AVX data edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on VEX AVX data edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a VEX AVX data edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "VEX AVX data edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "VEX AVX data edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx),
        9,
        "all VEX AVX data edge cases should run"
    );
    assert_eq!(
        tally.compared, 9,
        "all VEX AVX data edge cases should compare"
    );
}

#[test]
fn avx512_kvm_vex_insert_extract_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_avx_insert_extract_"))
        .collect();
    assert_eq!(
        cases.len(),
        25,
        "unexpected VEX insert/extract corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on VEX insert/extract cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a VEX insert/extract case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "VEX insert/extract corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "VEX insert/extract cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx),
        25,
        "all VEX insert/extract cases should run"
    );
    assert_eq!(
        tally.compared, 25,
        "all VEX insert/extract cases should compare"
    );
}

#[test]
fn avx512_kvm_vex_fma_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Fma && case.label.contains("_vex_"))
        .collect();
    assert_eq!(cases.len(), 59, "unexpected VEX FMA corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on VEX FMA cases");
    assert_eq!(tally.interp_err, 0, "rax failed to execute a VEX FMA case");
    assert_eq!(
        tally.skipped_asm, 0,
        "VEX FMA corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "VEX FMA cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Fma),
        59,
        "all VEX FMA cases should run"
    );
    assert_eq!(tally.compared, 59, "all VEX FMA cases should compare");
}

#[test]
fn avx512_kvm_avx2_integer_operand_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_avx2_int_operand_"))
        .collect();
    assert_eq!(
        cases.len(),
        24,
        "unexpected AVX2 integer operand-form corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on AVX2 integer operand-form cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an AVX2 integer operand-form case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "AVX2 integer operand-form corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "AVX2 integer operand-form cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx2),
        24,
        "all AVX2 integer operand-form cases should run"
    );
    assert_eq!(
        tally.compared, 24,
        "all AVX2 integer operand-form cases should compare"
    );
}

#[test]
fn avx512_kvm_avx2_gather_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_avx2_gather_"))
        .collect();
    assert_eq!(cases.len(), 24, "unexpected AVX2 gather corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on AVX2 gather cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an AVX2 gather case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "AVX2 gather corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "AVX2 gather cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx2),
        24,
        "all AVX2 gather cases should run"
    );
    assert_eq!(tally.compared, 24, "all AVX2 gather cases should compare");
}

#[test]
fn avx512_kvm_avx2_gather_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_avx2_gather_edge_"))
        .collect();
    assert_eq!(cases.len(), 8, "unexpected AVX2 gather edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on AVX2 gather edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an AVX2 gather edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "AVX2 gather edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "AVX2 gather edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Avx2),
        8,
        "all AVX2 gather edge cases should run"
    );
    assert_eq!(
        tally.compared, 8,
        "all AVX2 gather edge cases should compare"
    );
}

#[test]
fn avx512_kvm_avx512_sparse_memory_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_sparse_mem_"))
        .collect();
    assert_eq!(
        cases.len(),
        34,
        "unexpected AVX-512 sparse memory corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on AVX-512 sparse memory cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an AVX-512 sparse memory case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "AVX-512 sparse memory corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "AVX-512 sparse memory cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::F),
        28,
        "all AVX-512F sparse memory cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Vbmi2),
        6,
        "all AVX-512 VBMI2 sparse memory cases should run"
    );
    assert_eq!(
        tally.compared, 34,
        "all AVX-512 sparse memory cases should compare"
    );
}

#[test]
fn avx512_kvm_kmov_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_kmov_"))
        .collect();
    assert_eq!(cases.len(), 20, "unexpected KMOV corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on KMOV cases");
    assert_eq!(tally.interp_err, 0, "rax failed to execute a KMOV case");
    assert_eq!(
        tally.skipped_asm, 0,
        "KMOV corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "KMOV cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::F),
        5,
        "all AVX-512F KMOV word cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Dq),
        10,
        "all AVX-512DQ KMOV byte/dword cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Bw),
        5,
        "all AVX-512BW KMOV qword cases should run"
    );
    assert_eq!(tally.compared, 20, "all KMOV cases should compare");
}

#[test]
fn avx512_kvm_opmask_logic_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_opmask_edge_"))
        .collect();
    assert_eq!(cases.len(), 38, "unexpected opmask edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on opmask edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an opmask edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "opmask edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "opmask edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::F),
        12,
        "all AVX-512F opmask edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Dq),
        16,
        "all AVX-512DQ opmask edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Bw),
        10,
        "all AVX-512BW opmask edge cases should run"
    );
    assert_eq!(tally.compared, 38, "all opmask edge cases should compare");
}

#[test]
fn avx512_kvm_sse3_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Sse3)
        .collect();
    assert_eq!(cases.len(), 32, "unexpected SSE3 corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on SSE3 cases");
    assert_eq!(tally.interp_err, 0, "rax failed to execute an SSE3 case");
    assert_eq!(
        tally.skipped_asm, 0,
        "SSE3 corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SSE3 cases should not feature-skip"
    );
    assert_eq!(tally.compared, 32, "all SSE3 cases should compare");
}

#[test]
fn avx512_kvm_sse3_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Sse3 && case.label.contains("_sse3_edge_"))
        .collect();
    assert_eq!(cases.len(), 12, "unexpected SSE3 edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on SSE3 edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an SSE3 edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SSE3 edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SSE3 edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse3),
        12,
        "all SSE3 edge cases should run"
    );
    assert_eq!(tally.compared, 12, "all SSE3 edge cases should compare");
}

#[test]
fn avx512_kvm_ssse3_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_ssse3_edge_"))
        .collect();
    assert_eq!(cases.len(), 16, "unexpected SSSE3 edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on SSSE3 edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an SSSE3 edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "SSSE3 edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "SSSE3 edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Ssse3),
        16,
        "all SSSE3 edge cases should run"
    );
    assert_eq!(tally.compared, 16, "all SSSE3 edge cases should compare");
}

#[test]
fn avx512_kvm_legacy_streaming_masked_memory_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_legacy_stream") || case.label.contains("_legacy_mask"))
        .collect();
    assert_eq!(
        cases.len(),
        12,
        "unexpected legacy streaming/masked memory corpus size"
    );

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on legacy streaming/masked memory cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a legacy streaming/masked memory case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "legacy streaming/masked memory corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "legacy streaming/masked memory cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse),
        1,
        "all SSE streaming memory cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse2),
        7,
        "all SSE2 streaming/masked memory cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Mmx),
        4,
        "all MMX streaming/masked memory cases should run"
    );
    assert_eq!(
        tally.compared, 12,
        "all legacy streaming/masked memory cases should compare"
    );
}

#[test]
fn avx512_kvm_legacy_mask_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("_legacy_mask_edge_"))
        .collect();
    assert_eq!(cases.len(), 4, "unexpected legacy mask edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(
        tally.faulted, 0,
        "silicon faulted on legacy mask edge cases"
    );
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute a legacy mask edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "legacy mask edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "legacy mask edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::Sse2),
        2,
        "all SSE2 legacy mask edge cases should run"
    );
    assert_eq!(
        tally.ran_for(Feat::Mmx),
        2,
        "all MMX legacy mask edge cases should run"
    );
    assert_eq!(
        tally.compared, 4,
        "all legacy mask edge cases should compare"
    );
}

#[test]
fn avx512_kvm_x87_stack_edge_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.label.contains("x87_stack_edge_"))
        .collect();
    assert_eq!(cases.len(), 12, "unexpected x87 stack edge corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on x87 stack edge cases");
    assert_eq!(
        tally.interp_err, 0,
        "rax failed to execute an x87 stack edge case"
    );
    assert_eq!(
        tally.skipped_asm, 0,
        "x87 stack edge corpus produced assembler-rejected cases"
    );
    assert_eq!(
        tally.skipped_feature, 0,
        "x87 stack edge cases should not feature-skip"
    );
    assert_eq!(
        tally.ran_for(Feat::X87),
        12,
        "all x87 stack edge cases should run"
    );
    assert_eq!(
        tally.compared, 12,
        "all x87 stack edge cases should compare"
    );
}

#[test]
fn avx512_kvm_x87_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::X87)
        .collect();
    assert_eq!(cases.len(), 81, "unexpected x87 corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on x87 cases");
    assert_eq!(tally.interp_err, 0, "rax failed to execute an x87 case");
    assert_eq!(
        tally.skipped_asm, 0,
        "x87 corpus produced assembler-rejected cases"
    );
    assert_eq!(tally.compared, 81, "all x87 cases should compare");
}

#[test]
fn avx512_kvm_mmx_corpus() {
    let cases: Vec<_> = generated_cases()
        .into_iter()
        .filter(|case| case.feat == Feat::Mmx)
        .collect();
    assert_eq!(cases.len(), 69, "unexpected MMX corpus size");

    let Some(tally) = run_corpus(&cases) else {
        return;
    };
    assert_eq!(tally.faulted, 0, "silicon faulted on MMX cases");
    assert_eq!(tally.interp_err, 0, "rax failed to execute an MMX case");
    assert_eq!(
        tally.skipped_asm, 0,
        "MMX corpus produced assembler-rejected cases"
    );
    assert_eq!(tally.compared, 69, "all MMX cases should compare");
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
