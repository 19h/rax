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
/// Architecturally-defined status bits to compare (the 6 arithmetic flags).
const STATUS_RFLAGS_MASK: u64 = 0x8d5;
/// Value seeded into r8 (GPR-source / GPR-dest EVEX and k<->GPR forms read it).
const R8_SEED: u64 = 0x8877_6655_4433_2211;

/// One concrete architectural input: register file + scratch memory.
#[derive(Clone)]
struct InCase {
    zmm: [[u64; 8]; ZMM_REGS],
    k: [u64; K_REGS],
    r8: u64,
    rflags: u64,
    scratch: [u8; SCRATCH_BYTES],
}

/// One captured architectural output.
#[derive(Clone, PartialEq, Eq)]
struct OutCase {
    zmm: [[u64; 8]; ZMM_REGS],
    k: [u64; K_REGS],
    rax: u64,
    r8: u64,
    rflags: u64,
    scratch: [u8; SCRATCH_BYTES],
}

// ---------------------------------------------------------------------------
// Host AVX-512 feature detection. The silicon can only execute what it
// implements; everything else would #UD, so the corpus is gated on this.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Feat {
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
    /// FMA / base AVX (always present alongside AVX-512F).
    Base,
    /// VEX-encoded AVX VNNI dot-product instructions.
    AvxVnni,
    /// SHA-NI XMM crypto/message-schedule instructions.
    Sha,
    /// MOVDIRI direct stores from GPR to memory.
    Movdiri,
    /// MOVDIR64B 64-byte direct stores.
    Movdir64b,
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
            Feat::F => "avx512f",
            Feat::Bw => "avx512bw",
            Feat::Dq => "avx512dq",
            Feat::Cd => "avx512cd",
            Feat::Vl => "avx512vl",
            Feat::Base => "base",
            Feat::AvxVnni => "avx_vnni",
            Feat::Sha => "sha_ni",
            Feat::Movdiri => "movdiri",
            Feat::Movdir64b => "movdir64b",
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
            Feat::AvxVnni,
            Feat::Sha,
            Feat::Movdiri,
            Feat::Movdir64b,
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
    avx_vnni: bool,
    sha: bool,
    movdiri: bool,
    movdir64b: bool,
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
            avx_vnni: host_cpu_flag("avx_vnni"),
            sha: host_cpu_flag("sha_ni"),
            movdiri: host_cpu_flag("movdiri"),
            movdir64b: host_cpu_flag("movdir64b"),
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
            Feat::F | Feat::Base => self.f,
            Feat::Bw => self.bw,
            Feat::Dq => self.dq,
            Feat::Cd => self.cd,
            Feat::Vl => self.vl,
            Feat::AvxVnni => self.avx_vnni,
            Feat::Sha => self.sha,
            Feat::Movdiri => self.movdiri,
            Feat::Movdir64b => self.movdir64b,
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
const CR4_OSFXSR: u64 = 1 << 9;
const CR4_OSXMMEXCPT: u64 = 1 << 10;
const CR4_OSXSAVE: u64 = 1 << 18;
const CR4_VAL: u64 = CR4_PAE | CR4_OSFXSR | CR4_OSXMMEXCPT | CR4_OSXSAVE;
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
    // PML4[0] -> PDPTE (present + writable)
    mem.write(PML4_ADDR, &(PDPTE_ADDR | 0x3).to_le_bytes());
    // PDPTE[i] identity 1GiB huge pages (present + writable + PS), 4 entries.
    for i in 0u64..4 {
        let entry: u64 = (i << 30) | 0x83;
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
        kregs.rsp = STACK_ADDR;
        kregs.r8 = input.r8;
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

        Ok(KvmOutcome::Ran(OutCase {
            zmm,
            k,
            rax: final_regs.rax,
            r8: final_regs.r8,
            rflags: final_regs.rflags,
            scratch,
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
        r8: input.r8,
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
    let out_regs = run_until_hlt(&mut vcpu).map_err(|e| format!("interp run: {e:?}"))?;

    let mut scratch = [0u8; SCRATCH_BYTES];
    mem.read_slice(&mut scratch, GuestAddress(SCRATCH_ADDR))
        .map_err(|e| format!("read scratch: {e:?}"))?;

    let mut zmm = [[0u64; 8]; ZMM_REGS];
    for reg in 0..ZMM_REGS {
        zmm[reg] = get_regs_zmm(&out_regs, reg);
    }
    Ok(OutCase {
        zmm,
        k: out_regs.k,
        rax: out_regs.rax,
        r8: out_regs.r8,
        rflags: out_regs.rflags,
        scratch,
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

fn diff(interp: &OutCase, kvm: &OutCase) -> Vec<String> {
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
    if interp.r8 != kvm.r8 {
        diffs.push(format!("r8: interp={:#x} kvm={:#x}", interp.r8, kvm.r8));
    }
    let im = interp.rflags & STATUS_RFLAGS_MASK;
    let km = kvm.rflags & STATUS_RFLAGS_MASK;
    if im != km {
        diffs.push(format!("rflags(status): interp={im:#x} kvm={km:#x}"));
    }
    if interp.scratch != kvm.scratch {
        diffs.push(format!(
            "scratch differs:\n    interp={:02x?}\n    kvm   ={:02x?}",
            &interp.scratch[..],
            &kvm.scratch[..]
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
        r8: R8_SEED,
        rflags: INITIAL_RFLAGS,
        scratch,
    }
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
        ("vpmovsxbd", "vpmovsxbd %xmm3, %zmm1", F, Int, true),
        ("vpmovsxwd", "vpmovsxwd %ymm3, %zmm1", F, Int, true),
        ("vpmovsxdq", "vpmovsxdq %ymm3, %zmm1", F, Int, true),
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

// ---------------------------------------------------------------------------
// Assembler bridge (llvm-mc), mirroring the EVEX qemu harness.
// ---------------------------------------------------------------------------

const LLVM_MATTR: &str = concat!(
    "+avx512f,+avx512bw,+avx512dq,+avx512cd,+avx512vl,+fma,",
    "+avxvnni,",
    "+avx512ifma,+avx512vnni,+avx512vbmi,+avx512vbmi2,",
    "+avx512bitalg,+avx512vpopcntdq,+avx512bf16,+avx512fp16,",
    "+gfni,+vaes,+vpclmulqdq,+sha,+movdiri,+movdir64b"
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

fn parse_encoding(text: &str) -> Option<Vec<u8>> {
    let start = text.find("encoding: [")? + "encoding: [".len();
    let rest = &text[start..];
    let end = rest.find(']')?;
    let mut bytes = Vec::new();
    for token in rest[..end].split(',') {
        let token = token.trim().trim_start_matches("0x");
        bytes.push(u8::from_str_radix(token, 16).ok()?);
    }
    Some(bytes)
}

fn assemble(llvm_mc: &Path, asm: &str) -> Option<Vec<u8>> {
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
        // are VEX-encoded (0xC4/0xC5). SHA-NI and MOVDIR are intentionally
        // legacy 0F-family encodings.
        let legacy_allowed = matches!(case.feat, Feat::Sha | Feat::Movdiri | Feat::Movdir64b)
            && legacy_0f_encoding(&op);
        let expected_encoding =
            matches!(op.first(), Some(0x62) | Some(0xC4) | Some(0xC5)) || legacy_allowed;
        assert!(
            expected_encoding,
            "{}: unexpected encoding class, got {:02x?}",
            case.label, op
        );
        let code = build_code(&op);
        let input = input_for(case.profile);

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
        let diffs = diff(&interp, &kvm);
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
        let diffs = diff(&interp, &kvm);
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
