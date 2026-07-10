//! Architecture, backend, and ISA selectors.

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArchKind {
    X86_64,
    Hexagon,
    /// ARM 64-bit (AArch64/ARMv8-A 64-bit mode)
    Aarch64,
    /// ARM 32-bit ARMv7-A (Cortex-A series)
    Armv7a,
    /// ARM 32-bit ARMv8-A (AArch32 mode)
    Armv8a32,
    /// ARM Cortex-M (Thumb-2, ARMv6-M/ARMv7-M/ARMv8-M)
    CortexM,
    /// ARM Cortex-R (real-time processors)
    CortexR,
    /// RISC-V 64-bit (RV64GC).
    Riscv64,
}

impl Default for ArchKind {
    fn default() -> Self {
        ArchKind::X86_64
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    Kvm,
    Emulator,
    /// Apple Hypervisor.framework with Rosetta for x86_64 emulation (macOS only)
    Hvf,
}

impl Default for BackendKind {
    fn default() -> Self {
        #[cfg(target_os = "linux")]
        {
            BackendKind::Kvm
        }
        // Intel Mac - use HVF for hardware virtualization
        #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
        {
            BackendKind::Hvf
        }
        // Apple Silicon - HVF can't run x86_64 guests, use emulator
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            BackendKind::Emulator
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            BackendKind::Emulator
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Endianness {
    Little,
    Big,
}

impl Default for Endianness {
    fn default() -> Self {
        Endianness::Little
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HexagonIsa {
    V4,
    V5,
    V55,
    V60,
    V62,
    V65,
    V66,
    V67,
    V68,
    V69,
}

impl Default for HexagonIsa {
    fn default() -> Self {
        HexagonIsa::V68
    }
}

// =============================================================================
// ARM Architecture ISA Versions
// =============================================================================

/// ARM 64-bit (AArch64) architecture version.
/// Based on ARMv8-A and later with various extensions.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Aarch64Isa {
    /// ARMv8.0-A: Base 64-bit ARM (Cortex-A53, A57, A72, A73)
    /// Features: AArch64 execution, AdvSIMD, optional crypto
    V8_0,
    /// ARMv8.1-A: LSE atomics, VHE, PAN, RDMA (Cortex-A75, A76)
    /// Features: Atomic ops (CAS, SWP), Virtualization Host Extensions
    V8_1,
    /// ARMv8.2-A: SVE, FP16, DotProd, RAS (Cortex-A55, A75, A76, A77)
    /// Features: Scalable Vector Extension (optional), half-precision FP
    V8_2,
    /// ARMv8.3-A: PAC, FCMA, NV (Cortex-A77, A78)
    /// Features: Pointer Authentication, complex number multiply
    V8_3,
    /// ARMv8.4-A: Flag manipulation, RCPC2, secure EL2 (Cortex-A78)
    /// Features: CFINV, RMIF, nested virtualization
    V8_4,
    /// ARMv8.5-A: BTI, MTE, SSBS, RNG (Cortex-X1, A78C)
    /// Features: Branch Target ID, Memory Tagging, speculation barrier
    V8_5,
    /// ARMv8.6-A: BFloat16, I8MM, WFET (Cortex-X2, A710)
    /// Features: ML-optimized formats, WFE with timeout
    V8_6,
    /// ARMv8.7-A: WFI with timeout, HBC, enhanced PAC (Cortex-X3)
    /// Features: WFIT, hardware capabilities
    V8_7,
    /// ARMv8.8-A: NMI, MOPS (memory copy/set) (Cortex-X4)
    /// Features: Non-maskable interrupts, memory operations
    V8_8,
    /// ARMv9.0-A: Mandatory SVE2, RME, FEAT_CSV2 (Cortex-A510, A710, X2)
    /// Features: SVE2, Realm Management Extension
    V9_0,
    /// ARMv9.1-A: Enhanced BTI (Cortex-A715, X3)
    V9_1,
    /// ARMv9.2-A: SME (Scalable Matrix Extension) (Cortex-A720, X4)
    /// Features: Matrix operations, streaming SVE mode
    V9_2,
    /// ARMv9.3-A: Enhanced RME, GCS
    /// Features: Guarded Control Stack
    V9_3,
    /// ARMv9.4-A: SME2, multi-vector ops
    /// Features: 16 ZA tiles
    V9_4,
}

impl Default for Aarch64Isa {
    fn default() -> Self {
        Aarch64Isa::V8_0
    }
}

impl Aarch64Isa {
    /// Returns true if this version supports Large System Extensions (atomics)
    pub fn has_lse(&self) -> bool {
        !matches!(self, Aarch64Isa::V8_0)
    }

    /// Returns true if this version supports Pointer Authentication
    pub fn has_pac(&self) -> bool {
        matches!(
            self,
            Aarch64Isa::V8_3
                | Aarch64Isa::V8_4
                | Aarch64Isa::V8_5
                | Aarch64Isa::V8_6
                | Aarch64Isa::V8_7
                | Aarch64Isa::V8_8
                | Aarch64Isa::V9_0
                | Aarch64Isa::V9_1
                | Aarch64Isa::V9_2
                | Aarch64Isa::V9_3
                | Aarch64Isa::V9_4
        )
    }

    /// Returns true if this version supports Branch Target Identification
    pub fn has_bti(&self) -> bool {
        matches!(
            self,
            Aarch64Isa::V8_5
                | Aarch64Isa::V8_6
                | Aarch64Isa::V8_7
                | Aarch64Isa::V8_8
                | Aarch64Isa::V9_0
                | Aarch64Isa::V9_1
                | Aarch64Isa::V9_2
                | Aarch64Isa::V9_3
                | Aarch64Isa::V9_4
        )
    }

    /// Returns true if this is an ARMv9 version (mandatory SVE2)
    pub fn is_v9(&self) -> bool {
        matches!(
            self,
            Aarch64Isa::V9_0
                | Aarch64Isa::V9_1
                | Aarch64Isa::V9_2
                | Aarch64Isa::V9_3
                | Aarch64Isa::V9_4
        )
    }
}

/// ARM 32-bit (AArch32) architecture version.
/// Covers ARMv6 through ARMv8-A in AArch32 mode.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Aarch32Isa {
    /// ARMv6: ARM1136, ARM1176, ARM11MPCore
    /// Features: SIMD in GPRs, exclusive access, TrustZone (v6Z)
    V6,
    /// ARMv6T2: Thumb-2 technology
    /// Features: 32-bit Thumb, IT blocks, bit field ops
    V6T2,
    /// ARMv6K: Kernel extensions
    /// Features: CLREX, memory barriers, multiprocessing
    V6K,
    /// ARMv7-A: Cortex-A5, A7, A8, A9, A15, A17
    /// Features: VFP, NEON, virtualization extensions (optional)
    V7A,
    /// ARMv7-A with virtualization: Cortex-A15, A17
    /// Features: HYP mode, stage-2 translation
    V7AVirt,
    /// ARMv7-A with LPAE: Large Physical Address Extension
    /// Features: 40-bit physical addresses, long descriptors
    V7ALpae,
    /// ARMv8-A AArch32: Cortex-A32, A35 (32-bit only cores)
    /// Features: Crypto extensions, CRC32, all v8 mandatory
    V8A32,
}

impl Default for Aarch32Isa {
    fn default() -> Self {
        Aarch32Isa::V7A
    }
}

impl Aarch32Isa {
    /// Returns true if this version supports Thumb-2
    pub fn has_thumb2(&self) -> bool {
        !matches!(self, Aarch32Isa::V6)
    }

    /// Returns true if this version supports NEON
    pub fn has_neon(&self) -> bool {
        matches!(
            self,
            Aarch32Isa::V7A | Aarch32Isa::V7AVirt | Aarch32Isa::V7ALpae | Aarch32Isa::V8A32
        )
    }

    /// Returns true if this version supports virtualization
    pub fn has_virtualization(&self) -> bool {
        matches!(
            self,
            Aarch32Isa::V7AVirt | Aarch32Isa::V7ALpae | Aarch32Isa::V8A32
        )
    }

    /// Returns true if this version supports 40-bit physical addresses
    pub fn has_lpae(&self) -> bool {
        matches!(self, Aarch32Isa::V7ALpae | Aarch32Isa::V8A32)
    }
}

/// ARM Cortex-M architecture version.
/// Microcontroller profile with different exception model.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CortexMIsa {
    /// ARMv6-M: Cortex-M0, M0+, M1
    /// Features: Subset Thumb, NVIC, no DIV, optional SysTick
    V6M,
    /// ARMv7-M: Cortex-M3
    /// Features: Full Thumb-2, DIV, bit-banding, MPU
    V7M,
    /// ARMv7E-M: Cortex-M4, M7
    /// Features: DSP extensions, optional FPU (VFPv4-D16)
    V7EM,
    /// ARMv8-M Baseline: Cortex-M23
    /// Features: TrustZone, stack limit checking
    V8MBaseline,
    /// ARMv8-M Mainline: Cortex-M33, M35P
    /// Features: TrustZone, DSP, optional FPU/MVE
    V8MMainline,
    /// ARMv8.1-M: Cortex-M55, M85
    /// Features: MVE (Helium), low-overhead loops, half-precision FP
    V8_1M,
}

impl Default for CortexMIsa {
    fn default() -> Self {
        CortexMIsa::V7M
    }
}

impl CortexMIsa {
    /// Returns true if this version has full Thumb-2
    pub fn has_full_thumb2(&self) -> bool {
        !matches!(self, CortexMIsa::V6M | CortexMIsa::V8MBaseline)
    }

    /// Returns true if this version supports TrustZone
    pub fn has_trustzone(&self) -> bool {
        matches!(
            self,
            CortexMIsa::V8MBaseline | CortexMIsa::V8MMainline | CortexMIsa::V8_1M
        )
    }

    /// Returns true if this version supports DSP extensions
    pub fn has_dsp(&self) -> bool {
        matches!(
            self,
            CortexMIsa::V7EM | CortexMIsa::V8MMainline | CortexMIsa::V8_1M
        )
    }

    /// Returns true if this version can have an FPU
    pub fn can_have_fpu(&self) -> bool {
        !matches!(self, CortexMIsa::V6M | CortexMIsa::V8MBaseline)
    }

    /// Returns true if this version supports MVE (Helium)
    pub fn has_mve(&self) -> bool {
        matches!(self, CortexMIsa::V8_1M)
    }
}

/// ARM Cortex-R architecture version.
/// Real-time profile with deterministic interrupt latency.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CortexRIsa {
    /// ARMv7-R: Cortex-R4, R5, R7, R8
    /// Features: MPU, TCM, optional dual-core lockstep
    V7R,
    /// ARMv8-R AArch32: Cortex-R52, R52+
    /// Features: Virtualization, optional EL2, PMSAv8
    V8R,
    /// ARMv8-R AArch64: Cortex-R82
    /// Features: First 64-bit R-profile, optional MMU for Linux
    V8R64,
}

impl Default for CortexRIsa {
    fn default() -> Self {
        CortexRIsa::V7R
    }
}

impl CortexRIsa {
    /// Returns true if this is a 64-bit capable version
    pub fn is_64bit(&self) -> bool {
        matches!(self, CortexRIsa::V8R64)
    }

    /// Returns true if this version supports virtualization
    pub fn has_virtualization(&self) -> bool {
        matches!(self, CortexRIsa::V8R | CortexRIsa::V8R64)
    }
}

// =============================================================================
// ARM Feature Flags (optional extensions)
// =============================================================================

bitflags::bitflags! {
    /// ARM optional feature flags.
    /// These represent ISA extensions that may or may not be present.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
    pub struct ArmFeatures: u64 {
        // Crypto and security
        /// AES instructions (AESE, AESD, AESMC, AESIMC)
        const CRYPTO_AES = 1 << 0;
        /// SHA-1 instructions
        const CRYPTO_SHA1 = 1 << 1;
        /// SHA-256 instructions
        const CRYPTO_SHA256 = 1 << 2;
        /// SHA-512 instructions (v8.2+)
        const CRYPTO_SHA512 = 1 << 3;
        /// SHA-3 instructions (v8.2+)
        const CRYPTO_SHA3 = 1 << 4;
        /// SM3/SM4 Chinese crypto (v8.2+)
        const CRYPTO_SM = 1 << 5;
        /// CRC32 instructions
        const CRC32 = 1 << 6;

        // SIMD/Vector extensions
        /// NEON/AdvSIMD
        const NEON = 1 << 8;
        /// Half-precision FP (FP16)
        const FP16 = 1 << 9;
        /// BFloat16 (v8.6+)
        const BF16 = 1 << 10;
        /// Int8 matrix multiply (v8.6+)
        const I8MM = 1 << 11;
        /// Dot product instructions (v8.2+)
        const DOTPROD = 1 << 12;
        /// SVE (Scalable Vector Extension)
        const SVE = 1 << 13;
        /// SVE2
        const SVE2 = 1 << 14;
        /// SVE2 + AES
        const SVE2_AES = 1 << 15;
        /// SVE2 + SHA3
        const SVE2_SHA3 = 1 << 16;
        /// SVE2 + SM4
        const SVE2_SM4 = 1 << 17;
        /// SVE2 + bit permute
        const SVE2_BITPERM = 1 << 18;
        /// SME (Scalable Matrix Extension)
        const SME = 1 << 19;
        /// SME2
        const SME2 = 1 << 20;

        // Atomics and memory
        /// LSE atomics (v8.1+)
        const LSE = 1 << 24;
        /// LSE2 - larger atomics (v8.4+)
        const LSE2 = 1 << 25;
        /// RCPC (Release Consistent Processor Consistent)
        const RCPC = 1 << 26;
        /// RCPC2 (v8.4+)
        const RCPC2 = 1 << 27;

        // Pointer/Control flow
        /// PAC (Pointer Authentication) - address
        const PACA = 1 << 32;
        /// PAC - generic
        const PACG = 1 << 33;
        /// BTI (Branch Target Identification)
        const BTI = 1 << 34;
        /// MTE (Memory Tagging Extension)
        const MTE = 1 << 35;
        /// MTE2
        const MTE2 = 1 << 36;
        /// GCS (Guarded Control Stack)
        const GCS = 1 << 37;

        // Virtualization
        /// VHE (Virtualization Host Extensions)
        const VHE = 1 << 40;
        /// Nested virtualization
        const NV = 1 << 41;
        /// RME (Realm Management Extension)
        const RME = 1 << 42;

        // Misc
        /// RNG (hardware random number)
        const RNG = 1 << 48;
        /// DIT (Data Independent Timing)
        const DIT = 1 << 49;
        /// SSBS (Speculative Store Bypass Safe)
        const SSBS = 1 << 50;
        /// SB (Speculation Barrier)
        const SB = 1 << 51;
        /// MOPS (Memory copy/set operations)
        const MOPS = 1 << 52;
        /// HBC (Hinted Conditional Branches)
        const HBC = 1 << 53;
        /// NMI (Non-Maskable Interrupts)
        const NMI = 1 << 54;

        // VFP/FPU variants (for 32-bit)
        /// VFPv3-D16 (16 double-precision registers)
        const VFPV3_D16 = 1 << 56;
        /// VFPv3-D32 (32 double-precision registers)
        const VFPV3_D32 = 1 << 57;
        /// VFPv4 (fused multiply-add)
        const VFPV4 = 1 << 58;
        /// FPU single-precision only
        const FP_SP = 1 << 59;
        /// FPU double-precision
        const FP_DP = 1 << 60;

        // Cortex-M specific
        /// MVE (M-profile Vector Extension / Helium)
        const MVE = 1 << 61;
        /// MVE with floating-point
        const MVE_FP = 1 << 62;
    }
}
