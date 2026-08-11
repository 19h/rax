//! Google Security Chip (GSC) machine: Ti50 / Dauntless "Soteria" RISC-V core.
//!
//! This wires the [`crate::isa::riscv`] interpreter up as a bare-metal RV32 machine
//! for Google's Ti50/Dauntless GSC firmware ("nugget" / Nugget OS). It differs
//! from the generic [`crate::isa::riscv::RiscVCpu`] in three ways:
//!
//! 1. **RV32 + Xsoteria** — the hart runs [`Isa::ti50`] (RV32 IMC + Zicsr +
//!    the Xsoteria vendor bit-manipulation extension, custom opcodes 0x0b/0x2b).
//! 2. **SoC MMIO bridge** — peripheral registers in the `0x4000_0000` aperture
//!    are intercepted by [`GscBridge`] (open-bus by default, with a modeled
//!    console UART) so they never fall through to RAM or fault out-of-bounds.
//! 3. **Direct console** — UART transmit bytes are emitted straight to stdout
//!    and captured for tests, rather than routed through the VMM serial bus.
//!
//! The machine is selected by `RAX_MACHINE=gsc` under `--arch riscv64`. Several
//! knobs are environment-tunable to ease firmware bring-up without rebuilds:
//!
//! | env var | meaning | default |
//! |---|---|---|
//! | `RAX_GSC_UART` | console UART base (hex) | `0x404d_0000` |
//! | `RAX_GSC_UART_STATE` | base UART STATE bits (hex); RX-empty bit is dynamic | `0x30` (TX ready bits) |
//! | `RAX_GSC_UART_RX` | initial console RX bytes (raw env string) | empty |
//! | `RAX_GSC_OPENBUS` | value returned for unmodeled MMIO reads (hex) | `0` |
//! | `RAX_GSC_READY` | extra fixed status registers, `addr=val,...` (hex) | — |
//! | `RAX_GSC_RSTSRC_COLD` | RSTSRC value on the first (cold) boot (hex) | `0x01` (POR) |
//! | `RAX_GSC_RSTSRC_WARM` | RSTSRC value after a warm reset (hex) | `0x22` (SOFTWARE\|EXIT) |
//! | `RAX_GSC_AP_FLASH` | optional external AP SPI flash image backing opcode `0x0b` reads | blank flash |
//! | `RAX_GSC_ENTRY` | override the boot entry PC (hex; parsed in the loader) | auto |
//! | `RAX_GSC_TRACE` | `mmio` = log first-touch + PMU + console-candidate MMIO; `insn` = also trace every instruction | off |
//! | `RAX_GSC_BREAK` | breakpoint PC for register/stack/string dump (hex) | off |
//! | `RAX_GSC_BREAK_HIT` | 1-based breakpoint hit to dump (hex) | `1` |
//! | `RAX_GSC_BREAK_RA` | only count/dump breakpoint hits with this `ra` value (hex) | off |
//! | `RAX_GSC_BREAK_SAVED_RA` | only count/dump hits whose word at `sp+0x11c` matches (hex) | off |
//! | `RAX_GSC_BREAK_STACK` | bytes of stack words to dump at `RAX_GSC_BREAK` (hex) | `0` |
//! | `RAX_GSC_BREAK_STOP` | terminate the run immediately after dumping the breakpoint | off |
//! | `RAX_GSC_SYSCALL_TRACE` | log userspace ecall arguments | off |
//! | `RAX_GSC_PRINT_TRACE` | log firmware debug-print pointer/length calls | off |
//! | `RAX_GSC_CONSOLE_TRACE` | log PC + byte for visible UART console writes | off |
//! | `RAX_GSC_AP_RO_INFO_STUB` | synthesize the AP RO cached INFO record only | off |
//! | `RAX_GSC_AP_RO_CRYPTO_STUB` | synthesize AP RO cryptolib digest results without forcing verifier success | off |
//! | `RAX_GSC_AP_RO_STUB` | synthesize AP RO/GVD success for standalone boot bring-up | off |
//!
//! What's modeled (enough for Ti50 to boot through init into the Tock idle path,
//! and for the `nugget`/Dauntless image to reach its console boot path):
//!  - **Console UART** at `0x404d_0000` (RDATA `+0x00`, WDATA `+0x04`, STATE
//!    `+0x14`: bit0 clear = TX ready, bit7 set = RX empty). Transmit bytes are
//!    emitted to stdout and captured; receive bytes can be seeded with
//!    `RAX_GSC_UART_RX` and become visible once the firmware reaches WFI.
//!  - **Core-local interrupt claim** at `0xe000_e0d0`: source `0` is exposed
//!    as the console UART RX interrupt once the firmware reaches idle/WFI.
//!  - **PMU reset block**: RSTSRC (`+0x00`, cold=POR / warm=SOFTWARE|EXIT),
//!    CLRRST (`+0x04`, write-1-to-clear), GLOBAL_RESET (`+0x08`, key
//!    `0x0704_1776` → warm reboot). The reset-source is what lets the firmware
//!    classify the reset and report `Reset cause: POR/SW` instead of looping.
//!  - **Persistent registers**: writes read back and survive a warm reset, so
//!    the PMU boot-counter / init-flag scratch (`+0x4c`/`+0xb0`/`+0xa0`) stays
//!    sticky across reboots.
//!  - **Flash controller / INFO flash**: the `0x4011_0000` PE controller reads,
//!    programs, and erases the mapped XIP image plus separate blank INFO banks.
//!  - **GLOBALSEC/cryptolib discovery**: active RO/RW windows and a minimal
//!    synthetic cryptolib header/entry let the boot verifier continue.
//!  - **GPIO/sleep qualification**: the `0x4003_046c..0x4003_0488` config and
//!    `0x4052_0000/34/68/9c` input-bank words are stable status/config regs.
//!  - **RBOX** at `0x4009_0000`: interrupt/status W1C groups, init-ready, and
//!    control-status handshakes needed for RBOX init.
//!  - **AP SPI host** at `0x4060_0000`: control/transaction registers and the
//!    byte-addressable SPI data window used by AP flash status and fast-read
//!    traffic.
//!  - **GSC FIFO / TPM-SPI FIFO** at `0x4062_0000`: reset-busy bits clear and
//!    IRQ/status registers complete the TPM FIFO reset path.
//!  - A generic **spin-breaker** for "wait for status bit" boot loops, and a
//!    built-in map of known "ready" status registers.
//!
//! Partially modeled but still incomplete: production crypto/keyladder behavior
//! (AP RO currently uses narrow digest hooks), host-side USB/event delivery, TPM
//! host transactions, AP provisioning/fuses, and detailed timer/interrupt
//! routing.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use vm_memory::{Bytes, GuestAddress, GuestMemory, GuestMemoryMmap};

use crate::error::{Error, Result};
use crate::isa::riscv::{Isa, MemError, MemResult, Memory, RiscVConfig, RiscVCpu, RiscVExit};
use crate::vm::vcpu::{CpuState, RiscVRegisters, VCpu, VcpuExit};

/// SoC MMIO aperture. Peripheral registers live here; the bridge intercepts the
/// whole window so accesses never reach RAM and never raise an access fault.
const MMIO_LO: u64 = 0x4000_0000;
const MMIO_HI: u64 = 0x5000_0000;

/// UART register offsets (Cr50/Ti50 family, from the gscemu reference model and
/// confirmed against the nugget/Dauntless firmware: chars are written to
/// `base+0x04` and the firmware polls `base+0x14` bits [5:4] for "TX ready").
const UART_RDATA: u64 = 0x00;
const UART_WDATA: u64 = 0x04;
const UART_STATE: u64 = 0x14;
const UART_STATE_TX_BUSY: u32 = 0x0000_0001;
const UART_STATE_RX_EMPTY: u32 = 0x0000_0080;
/// Width of the modeled UART register block.
const UART_LEN: u64 = 0x100;

/// Default console UART base — the Ti50/Dauntless console is at 0x404d_0000
/// (discovered from the firmware's character writes). Overridable via
/// `RAX_GSC_UART`.
const DEFAULT_UART_BASE: u64 = 0x404d_0000;
/// Default UART STATE read value: TX-ready bits [5:4] set (overridable via
/// `RAX_GSC_UART_STATE`).
const DEFAULT_UART_STATE: u32 = 0x0000_0030;

/// Bound on instructions executed per `run()` call.
const MAX_ITERS: u64 = 50_000_000;

/// Tracing verbosity.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Trace {
    Off,
    Mmio,
    Insn,
}

/// Static configuration parsed once from the environment.
#[derive(Clone, Copy, Debug)]
struct GscConfig {
    uart_base: u64,
    uart_state: u32,
    open_bus: u32,
    rstsrc_cold: u32,
    rstsrc_warm: u32,
    flash_img_base: u64,
    trace: Trace,
    console_trace: bool,
    ap_ro_info_stub: bool,
    ap_ro_crypto_stub: bool,
    ap_ro_stub: bool,
    timer_div: u64,
}

impl GscConfig {
    fn from_env() -> Self {
        let uart_base = env_hex("RAX_GSC_UART").unwrap_or(DEFAULT_UART_BASE);
        let uart_state = env_hex("RAX_GSC_UART_STATE")
            .map(|v| v as u32)
            .unwrap_or(DEFAULT_UART_STATE);
        let open_bus = env_hex("RAX_GSC_OPENBUS").map(|v| v as u32).unwrap_or(0);
        let rstsrc_cold = env_hex("RAX_GSC_RSTSRC_COLD")
            .map(|v| v as u32)
            .unwrap_or(DEFAULT_RSTSRC_COLD);
        let rstsrc_warm = env_hex("RAX_GSC_RSTSRC_WARM")
            .map(|v| v as u32)
            .unwrap_or(DEFAULT_RSTSRC_WARM);
        let flash_img_base = env_hex("RAX_GSC_FLASH_BASE").unwrap_or(DEFAULT_FLASH_IMG_BASE);
        let trace = match std::env::var("RAX_GSC_TRACE").as_deref() {
            Ok("insn") => Trace::Insn,
            Ok("mmio") | Ok("1") => Trace::Mmio,
            _ => Trace::Off,
        };
        let console_trace = std::env::var("RAX_GSC_CONSOLE_TRACE").is_ok();
        let ap_ro_info_stub = std::env::var("RAX_GSC_AP_RO_INFO_STUB").is_ok();
        let ap_ro_crypto_stub = std::env::var("RAX_GSC_AP_RO_CRYPTO_STUB").is_ok();
        let ap_ro_stub = std::env::var("RAX_GSC_AP_RO_STUB").is_ok();
        let timer_div = env_hex("RAX_GSC_TIMER_DIV")
            .filter(|&v| v != 0)
            .unwrap_or(TIMER_DEFAULT_DIV);
        GscConfig {
            uart_base,
            uart_state,
            open_bus,
            rstsrc_cold,
            rstsrc_warm,
            flash_img_base,
            trace,
            console_trace,
            ap_ro_info_stub,
            ap_ro_crypto_stub,
            ap_ro_stub,
            timer_div,
        }
    }
}

/// Status registers that the firmware polls for a "ready"/"locked"/"done" bit,
/// modeled as constant reads so boot polling loops make progress. These are
/// SoC peripheral status registers discovered from boot traces (clock/PLL lock,
/// peripheral-ready handshakes). Address → value.
///
/// Extend at runtime with `RAX_GSC_READY="addr=val,addr=val"` (hex), which is
/// merged over this table.
fn builtin_ready_map() -> HashMap<u64, u32> {
    let mut m = HashMap::new();
    // fw.bin (nugget/Dauntless): peripheral at 0x404d_0000 — firmware writes a
    // command then waits for status bits [5:4] (mask 0x30) to both be set.
    m.insert(0x404d_0014, 0x0000_0030);
    m
}

/// Parse `RAX_GSC_READY="addr=val,addr=val"` (hex pairs) into address→value.
fn parse_ready_env() -> HashMap<u64, u32> {
    let mut m = HashMap::new();
    let Ok(raw) = std::env::var("RAX_GSC_READY") else {
        return m;
    };
    for pair in raw.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        if let Some((a, v)) = pair.split_once('=') {
            let parse = |s: &str| {
                let s = s.trim();
                let s = s
                    .strip_prefix("0x")
                    .or_else(|| s.strip_prefix("0X"))
                    .unwrap_or(s);
                u64::from_str_radix(s, 16).ok()
            };
            if let (Some(addr), Some(val)) = (parse(a), parse(v)) {
                m.insert(addr, val as u32);
            }
        }
    }
    m
}

/// Parse a hex (or decimal) environment variable, tolerating a `0x` prefix.
fn env_hex(name: &str) -> Option<u64> {
    let raw = std::env::var(name).ok()?;
    let s = raw.trim();
    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    u64::from_str_radix(s, 16).ok()
}

/// Parse `RAX_GSC_TPM_CMD` into one or more raw TPM command byte vectors.
/// Accepts hex bytes with optional whitespace/`0x`; several commands may be
/// separated by commas or semicolons.
fn parse_tpm_cmds_env() -> Vec<Vec<u8>> {
    let Ok(raw) = std::env::var("RAX_GSC_TPM_CMD") else {
        return Vec::new();
    };
    raw.split([',', ';'])
        .filter_map(|part| {
            let hex: String = part.chars().filter(|c| c.is_ascii_hexdigit()).collect();
            if hex.len() < 2 {
                return None;
            }
            let bytes: Vec<u8> = (0..hex.len() - 1)
                .step_by(2)
                .filter_map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
                .collect();
            (!bytes.is_empty()).then_some(bytes)
        })
        .collect()
}

/// Once an unmodeled status register has been read this many times since the
/// last MMIO write (i.e. with no forward progress), the spin-breaker returns
/// all-ones to satisfy "wait for ready bit" boot loops.
const SPIN_THRESHOLD: u32 = 4096;

/// PMU reset-control block (offsets confirmed from the nugget/Dauntless boot
/// handler: it reads RSTSRC at +0x00, clears it via CLRRST at +0x04, and
/// reboots via GLOBAL_RESET at +0x08).
const PMU_RSTSRC_REG: u64 = 0x4000_0000;
const PMU_CLRRST_REG: u64 = 0x4000_0004;
const PMU_RESET_REG: u64 = 0x4000_0008;
const PMU_CHIP_ID_REG: u64 = 0x4001_ffe0;
const PMU_WAKE_EXIT_SRC_REG: u64 = 0x4001_ffe4;
const PMU_CHIP_ID_ALT_REG: u64 = 0x4001_fff8;
/// GLOBAL_RESET magic (`0x0704_1776` — a July-4 1776 magic, == Cr50
/// `GC_PMU_GLOBAL_RESET_KEY`). Writing it reboots the chip.
const PMU_RESET_MAGIC: u32 = 0x0704_1776;
/// Packed PMU chip ID fields from the Cr50/GSC register model:
/// standard=1, mfg=0x4a6, part=0x4856, rev=8. Ti50 firmware accepts this
/// value before falling back to the Dauntless revision.
const PMU_CHIP_ID: u32 = 0x8485_694d;

/// RSTSRC cause bits (Cr50/Ti50 family): POR=bit0, EXIT=bit1, WDOG=bit2,
/// LOCKUP=bit3, SYSRESET=bit4, SOFTWARE=bit5.
const RSTSRC_POR: u32 = 0x01;
/// Default reset-source the firmware sees on the first (cold) boot.
const DEFAULT_RSTSRC_COLD: u32 = RSTSRC_POR;
/// Default reset-source after a firmware-triggered warm reset. `0x22` =
/// SOFTWARE | EXIT, which satisfies both fw.bin's SOFTWARE(0x20) check and the
/// ti50 RO's EXIT(bit1) check, so neither re-runs one-time POR setup.
const DEFAULT_RSTSRC_WARM: u32 = 0x22;
/// Standard RISC-V machine external interrupt pending bit (`mip.MEIP`). The
/// Ti50 firmware enables MSIP/MTIP/MEIP (`mie=0x888`) before entering WFI.
const MIP_MEIP: u64 = 1 << 11;
/// Standard RISC-V machine timer interrupt pending bit (`mip.MTIP`). The Ti50
/// alarm driver programs the 64-bit compare at `0x400C002C/+0x34`; when the
/// free-running counter reaches it the hardware raises MTIP (cause 7, vectored
/// to `0x9551c`). The firmware's ISR re-arms the compare to disarm/reschedule.
const MIP_MTIP: u64 = 1 << 7;

/// Safety bound: if the firmware reboots this many times without producing any
/// console output, give up rather than spin forever.
const MAX_RESETS: u32 = 32;

/// Mutable state shared between the bridge (which mutates it during memory
/// accesses, behind `&self`) and the vCPU (which reads it back out).
#[derive(Default)]
struct GscShared {
    /// Captured UART transmit bytes (the console output).
    console: Vec<u8>,
    /// Optional UART receive bytes exposed through RDATA/STATE.
    uart_rx: VecDeque<u8>,
    /// Seeded UART RX is held until the firmware has reached WFI once. Without
    /// this, preload bytes assert MEIP while the Tock UART interrupt tables are
    /// still being initialized.
    uart_irq_armed: bool,
    /// First-touch MMIO accesses, keyed by `(addr << 1) | is_write`, for the
    /// trace-driven peripheral-map discovery.
    seen: BTreeSet<u64>,
    /// Per-address read counts since the last MMIO write, for the spin-breaker.
    /// Cleared on any MMIO write (forward progress).
    read_counts: HashMap<u64, u32>,
    /// Last value written to each MMIO register, so registers (e.g. PMU scratch)
    /// read back what was written and survive a warm reset.
    store: HashMap<u64, u32>,
    /// Per-address spin-breaker phase: alternated each time a spin is broken so
    /// that both "wait for bit set" and "wait for bit clear" loops are
    /// satisfied (one polarity per break).
    spin_phase: HashMap<u64, bool>,
    /// Set when the firmware writes the PMU reset magic; drained by the run loop.
    reset_requested: bool,
    /// PMU RSTSRC (reset-source) value the firmware reads at boot to classify
    /// why the chip reset. Cleared by CLRRST; reloaded on each warm reset.
    rstsrc: u32,
    /// Flash controller: PE_EN armed (magic written), current program/erase
    /// and read transactions, payload and readback registers.
    flash_pe_en: bool,
    flash_trans: u32,
    flash_read_trans: u32,
    flash_wr_data: [u32; 2],
    flash_dout: [[u32; 2]; 2],
    flash_error: u32,
    /// INFO flash banks are not part of the loaded XIP image. Model them as
    /// blank flash by default and keep programmed words separately.
    flash_info: HashMap<u64, u32>,
    /// AP SPI host transfer window (`base+0x1000`). The firmware copies bytes
    /// through this aperture before starting a transaction.
    ap_spi_data: Vec<u8>,
    /// Deterministic pseudo-random stream for the TRNG data register.
    trng_state: u32,
    /// 64-bit machine-timer alarm compare (`0x400C002C` low / `+0x34` high).
    /// Defaults to all-ones (disarmed); when the free-running counter reaches
    /// this value the run loop asserts `mip.MTIP`. The firmware's timer ISR
    /// rewrites it to re-arm (future deadline) or disarm (all-ones).
    timer_compare: u64,
    /// Set while a TPM command is being processed (for the crypto trace, which
    /// otherwise floods with boot traffic).
    tpm_active: bool,
    /// GscFifo / TPM-SPI dual-port command/response RAM at `0x40621000`. In the
    /// wired transport, the host stages a TPM command here and the firmware
    /// reads it from / writes the response to this real FIFO window via MMIO.
    fifo_ram: Vec<u8>,
}

type Shared = Arc<Mutex<GscShared>>;

/// Guest memory bridge for the GSC machine: RAM/flash via [`GuestMemoryMmap`],
/// with the SoC MMIO aperture intercepted.
struct GscBridge {
    mem: Arc<GuestMemoryMmap>,
    shared: Shared,
    /// Optional external AP SPI flash image. Missing bytes read as erased flash.
    ap_flash: Arc<Vec<u8>>,
    cfg: GscConfig,
    /// Status registers that read back a fixed "ready" value.
    ready: HashMap<u64, u32>,
    /// Current guest PC, published by the run loop before each step so MMIO
    /// trace lines can be correlated to the faulting instruction.
    pc: Arc<AtomicU64>,
    /// Monotonic retired-instruction count, published by the run loop and used
    /// to drive the free-running timer/counter block (`0x400C0014/+0x1C`).
    time: Arc<AtomicU64>,
    /// When set (`RAX_GSC_CRYPTO_TRACE`), log every access to the crypto/DRBG
    /// MMIO window (`0x40200000..0x40260000`) — used to find the entropy source.
    crypto_trace: bool,
}

impl GscBridge {
    /// Current value of the free-running 64-bit timer counter, derived from the
    /// retired-instruction count published by the run loop.
    fn timer_ticks(&self) -> u64 {
        let instret = self.time.load(Ordering::Relaxed);
        instret / self.cfg.timer_div.max(1)
    }

    /// Deterministic non-zero PRNG word for a DRBG output-file address. Mixes
    /// the address with a SplitMix64-style avalanche so the firmware's keymgr
    /// DRBG reads back varied, non-zero entropy/key material instead of zeros.
    fn drbg_word(&self, addr: u64) -> u32 {
        let mut x = addr
            .wrapping_mul(0x9e37_79b9_7f4a_7c15)
            .wrapping_add(0x1234_5678_9abc_def0);
        x ^= x >> 30;
        x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x ^= x >> 27;
        x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
        x ^= x >> 31;
        (x as u32) | 1
    }
}

/// PMU base and control block (reset-source / global-reset registers live here).
const PMU_BASE: u64 = 0x4000_0000;
const PMU_CTRL_TOP: u64 = 0x4000_0100;

/// Flash controller registers. Ti50 uses PE_CONTROL0/1 at `+0x04/+0x08` for
/// command opcodes; payload data lives at `+0x78/+0x7c`. PE_EN magic at `+0x8c`
/// arms a command. Ti50 has a read transaction register at `+0x0c` and a
/// program/erase transaction register at `+0x10`; the latter is encoded by the
/// firmware helper at VA 0x9f9ac as `byte = ((TRANS >> 7) & 0xffff) * 4`.
/// Program operations are applied to the flash image in guest memory so the
/// storage layer reads the result back via XIP.
const FLASH_BASE: u64 = 0x4011_0000;
const FLASH_PE_CONTROL0: u64 = FLASH_BASE + 0x04;
const FLASH_PE_CONTROL1: u64 = FLASH_BASE + 0x08;
const FLASH_READ_TRANS: u64 = FLASH_BASE + 0x0c;
const FLASH_TRANS: u64 = FLASH_BASE + 0x10;
const FLASH_STATUS0: u64 = FLASH_BASE + 0x18;
const FLASH_STATUS1: u64 = FLASH_BASE + 0x1c;
const FLASH_DOUT0_LO: u64 = FLASH_BASE + 0x60;
const FLASH_DOUT0_HI: u64 = FLASH_BASE + 0x64;
const FLASH_DOUT1_LO: u64 = FLASH_BASE + 0x6c;
const FLASH_DOUT1_HI: u64 = FLASH_BASE + 0x70;
const FLASH_WR_DATA0: u64 = FLASH_BASE + 0x78;
const FLASH_WR_DATA1: u64 = FLASH_BASE + 0x7c;
const FLASH_PE_EN: u64 = FLASH_BASE + 0x8c;
const FLASH_ERROR: u64 = FLASH_BASE + 0x9c;
const FLASH_PE_EN_MAGIC: u32 = 0xB119_24E1;
/// Program opcode observed in Ti50 persistent-storage writes.
const FLASH_OP_TI50_PROGRAM: u32 = 0xe89d_48b7;
/// Cr50/H1 opcodes kept for the shared controller family behavior.
const FLASH_OP_CR50_READ: u32 = 0x1602_1765;
const FLASH_OP_CR50_PROGRAM: u32 = 0x2718_2818;
const FLASH_OP_CR50_ERASE: u32 = 0x3141_5927;
/// Default base of the flash image in guest memory (Ti50: 0x80000; the nugget
/// single-slot image loads at 0xa0000). Overridable via `RAX_GSC_FLASH_BASE`.
const DEFAULT_FLASH_IMG_BASE: u64 = 0x8_0000;
/// Ti50 full flash images are two 512 KiB slots. FLASH PE_CONTROL0 targets the
/// first slot and PE_CONTROL1 targets the second slot.
const TI50_FLASH_SLOT_SIZE: u64 = 0x8_0000;

const USB_BASE: u64 = 0x400e_0000;
const USB_SOFT_RESET: u64 = USB_BASE + 0x0d4;
const USB_INT_STATE0: u64 = USB_BASE + 0x148;
const USB_INT_STATE1: u64 = USB_BASE + 0x150;
const USB_INT_STATE2: u64 = USB_BASE + 0x154;

const GSC_EVENT_USB_RESET: u64 = 0x4004_0010;
const GSC_EVENT_USB_TRIGGER: u64 = 0x4008_0000;

const RBOX_BASE: u64 = 0x4009_0000;
const RBOX_TOP: u64 = RBOX_BASE + 0x1000;
const RBOX_INTR0_ENABLE: u64 = RBOX_BASE + 0x04;
const RBOX_INTR0_TEST: u64 = RBOX_BASE + 0x0c;
const RBOX_INTR0_STATE: u64 = RBOX_BASE + 0x10;
const RBOX_INTR1_ENABLE: u64 = RBOX_BASE + 0x18;
const RBOX_INTR1_TEST: u64 = RBOX_BASE + 0x20;
const RBOX_INTR1_STATE: u64 = RBOX_BASE + 0x24;
const RBOX_INTR2_ENABLE: u64 = RBOX_BASE + 0x2c;
const RBOX_INTR2_TEST: u64 = RBOX_BASE + 0x34;
const RBOX_INTR2_STATE: u64 = RBOX_BASE + 0x38;
const RBOX_CONTROL0: u64 = RBOX_BASE + 0x44;
const RBOX_CONTROL1: u64 = RBOX_BASE + 0x48;
const RBOX_STATUS: u64 = RBOX_BASE + 0x54;
const RBOX_INIT_READY: u64 = RBOX_BASE + 0x58;
const RBOX_CMD_STATUS: u64 = RBOX_BASE + 0xa4;

const AP_SPI_BASE: u64 = 0x4060_0000;
const AP_SPI_TOP: u64 = AP_SPI_BASE + 0x2000;
const AP_SPI_XACT: u64 = AP_SPI_BASE + 0x04;
const AP_SPI_XFER_CFG: u64 = AP_SPI_BASE + 0x08;
const AP_SPI_INTR0: u64 = AP_SPI_BASE + 0x14;
const AP_SPI_INTR1: u64 = AP_SPI_BASE + 0x1c;
const AP_SPI_INTR2: u64 = AP_SPI_BASE + 0x20;
const AP_SPI_XACT_START: u32 = 1;
const AP_SPI_DATA_BASE: u64 = AP_SPI_BASE + 0x1000;
const AP_SPI_DATA_LEN: usize = 0x100;
const AP_SPI_OP_READ_SR1: u8 = 0x05;
const AP_SPI_OP_FAST_READ: u8 = 0x0b;
const AP_SPI_OP_READ_SR3: u8 = 0x15;
const AP_SPI_OP_READ_SR2: u8 = 0x35;

const GSC_FIFO_BASE: u64 = 0x4062_0000;
const GSC_FIFO_TOP: u64 = GSC_FIFO_BASE + 0x1000;
const GSC_FIFO_CONTROL: u64 = GSC_FIFO_BASE + 0x10;
const GSC_FIFO_IRQ_ENABLE: u64 = GSC_FIFO_BASE + 0x590;
const GSC_FIFO_IRQ_TEST: u64 = GSC_FIFO_BASE + 0x598;
const GSC_FIFO_IRQ_STATE: u64 = GSC_FIFO_BASE + 0x59c;
const GSC_FIFO_IRQ_STATUS: u64 = GSC_FIFO_BASE + 0x5a0;
const GSC_FIFO_RESET_BUSY_MASK: u32 = 0x9;
/// GscFifo dual-port command/response RAM window (host TPM frame staging).
const GSC_FIFO_RAM_BASE: u64 = GSC_FIFO_BASE + 0x1000;
const GSC_FIFO_RAM_LEN: usize = 0x800;

const GLOBALSEC_BASE: u64 = 0x4010_0000;
const GLOBALSEC_CRYPTOLIB_BASE: u64 = GLOBALSEC_BASE + 0x1c0;
const GLOBALSEC_ACTIVE_RO_BASE: u64 = GLOBALSEC_BASE + 0x270;
const GLOBALSEC_ACTIVE_RO_SIZE: u64 = GLOBALSEC_BASE + 0x274;
const GLOBALSEC_ACTIVE_RW_BASE: u64 = GLOBALSEC_BASE + 0x280;
const GLOBALSEC_ACTIVE_RW_SIZE: u64 = GLOBALSEC_BASE + 0x284;
const TI50_RO_IMAGE_SIZE: u32 = 0x1_4000;
const TI50_RW_SLOT_OFFSET: u64 = 0x1_5000;
const TI50_RW_IMAGE_SIZE: u32 = 0x5_a000;
const CRYPTOLIB_ROM_BASE: u64 = 0;
const CRYPTOLIB_HEADER: u64 = CRYPTOLIB_ROM_BASE + 0x800;
const CRYPTOLIB_ENTRY: u64 = CRYPTOLIB_ROM_BASE + 0x900;
const CRYPTOLIB_MAGIC: u32 = 0xca11_ab1e;

const TRNG_READ_DATA: u64 = 0x4041_00a8;

// GPIO/pad block used by the idle/sleep path. IDA confirms the sleep helper
// programs the 0x4003046c..0x40030488 registers, while GPIO input sampling
// reads one word per bank from 0x40520000/34/68/9c.
const GPIO_CFG_BASE: u64 = 0x4003_0000;
const GPIO_CFG_TOP: u64 = 0x4003_0500;
const GPIO_INTR_STATE0: u64 = 0x4003_0484;
const GPIO_INTR_STATE1: u64 = 0x4003_0488;
const GPIO_INPUT_BANK0: u64 = 0x4052_0000;
const GPIO_INPUT_BANK1: u64 = 0x4052_0034;
const GPIO_INPUT_BANK2: u64 = 0x4052_0068;
const GPIO_INPUT_BANK3: u64 = 0x4052_009c;
/// `plt_rst_l` is GPIO input bank0 (`0x40520000`) bit 11 (active-low: 0 =
/// asserted = AP/host held in reset). Driving it high deasserts PLT_RST_L so
/// the firmware treats the AP as powered on and accepts host TPM traffic
/// (without it the TPM task drops commands "while AP off"). Found by runtime
/// bisection of the `sub_A2F30` pin samples; the bit lives in an RO helper so
/// it is not statically derivable. `RAX_GSC_AP_ON=1` drives this bit.
const GPIO_PLT_RST_L_MASK: u32 = 1 << 11;

const FUSE_BASE: u64 = 0x4045_0000;
const FUSE_TOP: u64 = 0x4046_0000;
const FUSE_DEFAULT: u32 = 0x5555_5555;

const CORE_LOCAL_BASE: u64 = 0xe000_e000;
const CORE_LOCAL_TOP: u64 = CORE_LOCAL_BASE + 0x1000;
const CORE_IRQ_CLAIM: u64 = CORE_LOCAL_BASE + 0x0d0;
const CORE_IRQ_EPOCH: u64 = CORE_LOCAL_BASE + 0x0d8;
const CORE_IRQ_NONE: u32 = 0x8000_0000;
const CORE_IRQ_UART0: u32 = 0;

// Timer / 64-bit free-running counter block (0x400C0000). `sub_8072E` reads the
// monotonic "now" as `PAIR64(+0x1C high, +0x14 low)` (right-shifted by 8 only
// when the prescale at `+0x10` reads zero; the firmware programs it to 261, so
// the raw counter is used). `sub_806D6` initializes the block (prescale `+0x10`,
// masks `+0x2C/+0x34 = -1`, enable `+0x00 = 1`) and `+0x78` is the busy/ready
// status. With the counter frozen at zero every log line prints `[ 0.000]`;
// advancing it makes the firmware's millisecond timestamps progress.
const TIMER_BASE: u64 = 0x400c_0000;
const TIMER_TOP: u64 = TIMER_BASE + 0x100;
const TIMER_COUNT_LO: u64 = TIMER_BASE + 0x14;
const TIMER_COUNT_HI: u64 = TIMER_BASE + 0x1c;
/// 64-bit alarm compare (low/high). `set_alarm` writes these; init disarms by
/// writing all-ones. MTIP asserts while the counter has reached the compare.
const TIMER_COMPARE_LO: u64 = TIMER_BASE + 0x2c;
const TIMER_COMPARE_HI: u64 = TIMER_BASE + 0x34;
const TIMER_BUSY: u64 = TIMER_BASE + 0x78;
/// Retired-instruction-to-timer-tick divisor. The Soteria core retires roughly
/// this many instructions per timer tick; tuned so the boot's millisecond
/// timestamps stay in a realistic range rather than racing ahead. Overridable
/// via `RAX_GSC_TIMER_DIV`.
const TIMER_DEFAULT_DIV: u64 = 24;

// Second timer/counter instance with the same core layout, exposed at window
// base `0x40631000` (block base `0x40630000`, accessed via a RAM pointer so no
// literal appears in the dump). The TPM task polls its 64-bit counter for
// command-timeout deadlines (reload `+0x38` = 0x2710); leaving it frozen at 0
// makes those waits spin until the generic spin-breaker trips. Driving it from
// the same instruction-count source keeps TPM timing live.
const TIMER2_COUNT_LO: u64 = 0x4063_1014;
const TIMER2_COUNT_HI: u64 = 0x4063_101c;
const TIMER2_BUSY: u64 = 0x4063_1070;

// DRBG / CSRNG keymgr block at `0x40250000`. `sub_80434` arms `+0x10`, starts at
// `+0x14`, then spins until `+0x10` bits[2:0] != 0 and treats value `1` as
// success (`return status ^ 1`). The 256-bit generated-output file is at
// `+0xE8`; the second input/output file at `+0xC8`. Modeling "done = 1" plus a
// non-zero PRNG output lets the keymgr DRBG instantiate/generate succeed instead
// of failing (`DDRBG instantiate failed`).
const DRBG_BASE: u64 = 0x4025_0000;
const DRBG_STATUS: u64 = DRBG_BASE + 0x10;
const DRBG_OUT_LO: u64 = DRBG_BASE + 0xc8;
const DRBG_OUT_HI: u64 = DRBG_BASE + 0xe8;
const DRBG_OUT_LEN: u64 = 0x20;

impl std::fmt::Debug for GscBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GscBridge").finish()
    }
}

#[inline]
fn in_mmio(addr: u64) -> bool {
    (MMIO_LO..MMIO_HI).contains(&addr)
}

#[inline]
fn in_core_local(addr: u64) -> bool {
    (CORE_LOCAL_BASE..CORE_LOCAL_TOP).contains(&addr)
}

#[inline]
fn fill_le_word(buf: &mut [u8], word: u32) {
    let bytes = word.to_le_bytes();
    for (i, b) in buf.iter_mut().enumerate() {
        *b = bytes[i & 3];
    }
}

#[inline]
fn is_usb_w1c_status(addr: u64) -> bool {
    matches!(
        addr,
        USB_INT_STATE0
            | USB_INT_STATE1
            | USB_INT_STATE2
            | GSC_EVENT_USB_RESET
            | GSC_EVENT_USB_TRIGGER
    )
}

#[inline]
fn is_rbox(addr: u64) -> bool {
    (RBOX_BASE..RBOX_TOP).contains(&addr)
}

#[inline]
fn is_rbox_w1c_status(addr: u64) -> bool {
    matches!(
        addr,
        RBOX_INTR0_ENABLE
            | RBOX_INTR0_TEST
            | RBOX_INTR0_STATE
            | RBOX_INTR1_ENABLE
            | RBOX_INTR1_TEST
            | RBOX_INTR1_STATE
            | RBOX_INTR2_ENABLE
            | RBOX_INTR2_TEST
            | RBOX_INTR2_STATE
    )
}

#[inline]
fn is_ap_spi(addr: u64) -> bool {
    (AP_SPI_BASE..AP_SPI_TOP).contains(&addr)
}

#[inline]
fn is_ap_spi_w1c_status(addr: u64) -> bool {
    matches!(addr, AP_SPI_INTR0 | AP_SPI_INTR1 | AP_SPI_INTR2)
}

#[inline]
fn ap_spi_data_offset(addr: u64) -> Option<usize> {
    let off = addr.checked_sub(AP_SPI_DATA_BASE)?;
    (off < AP_SPI_DATA_LEN as u64).then_some(off as usize)
}

#[inline]
fn fifo_ram_offset(addr: u64) -> Option<usize> {
    let off = addr.checked_sub(GSC_FIFO_RAM_BASE)?;
    (off < GSC_FIFO_RAM_LEN as u64).then_some(off as usize)
}

#[inline]
fn ap_spi_fast_read_addr(data: &[u8]) -> u32 {
    let b1 = data.get(1).copied().unwrap_or(0) as u32;
    let b2 = data.get(2).copied().unwrap_or(0) as u32;
    let b3 = data.get(3).copied().unwrap_or(0) as u32;
    (b1 << 16) | (b2 << 8) | b3
}

fn load_ap_flash_from_env() -> Arc<Vec<u8>> {
    let Ok(path) = std::env::var("RAX_GSC_AP_FLASH") else {
        return Arc::new(Vec::new());
    };
    if path.trim().is_empty() {
        return Arc::new(Vec::new());
    }
    match std::fs::read(&path) {
        Ok(bytes) => Arc::new(bytes),
        Err(e) => {
            eprintln!("[gsc] failed to read RAX_GSC_AP_FLASH={path:?}: {e}; using blank AP flash");
            Arc::new(Vec::new())
        }
    }
}

#[inline]
fn is_gsc_fifo(addr: u64) -> bool {
    (GSC_FIFO_BASE..GSC_FIFO_TOP).contains(&addr)
}

#[inline]
fn is_gsc_fifo_w1c_status(addr: u64) -> bool {
    matches!(
        addr,
        GSC_FIFO_IRQ_ENABLE | GSC_FIFO_IRQ_TEST | GSC_FIFO_IRQ_STATE
    )
}

#[inline]
fn is_gpio_cfg(addr: u64) -> bool {
    (GPIO_CFG_BASE..GPIO_CFG_TOP).contains(&addr)
}

#[inline]
fn is_gpio_w1c_status(addr: u64) -> bool {
    matches!(addr, GPIO_INTR_STATE0 | GPIO_INTR_STATE1)
}

#[inline]
fn is_gpio_input_bank(addr: u64) -> bool {
    matches!(
        addr,
        GPIO_INPUT_BANK0 | GPIO_INPUT_BANK1 | GPIO_INPUT_BANK2 | GPIO_INPUT_BANK3
    )
}

#[inline]
fn is_flash_control(addr: u64) -> bool {
    matches!(addr, FLASH_PE_CONTROL0 | FLASH_PE_CONTROL1)
}

#[inline]
fn flash_control_index(addr: u64) -> usize {
    usize::from(addr == FLASH_PE_CONTROL1)
}

#[inline]
fn flash_trans_offset(trans: u32) -> u64 {
    (((trans >> 7) & 0xffff) as u64) * 4
}

#[inline]
fn flash_read_trans_offset(trans: u32) -> u64 {
    ((trans & 0xffff) as u64) * 4
}

#[inline]
fn flash_trans_is_info(trans: u32) -> bool {
    trans & 0x8 != 0
}

#[inline]
fn flash_read_trans_is_info(trans: u32) -> bool {
    trans & 0x1_0000 != 0
}

fn install_synthetic_cryptolib(mem: &GuestMemoryMmap) {
    // Ti50 discovers a ROM-resident cryptolib through GLOBALSEC+0x1c0 and
    // tail-calls its entry. The proprietary image does not include that ROM
    // blob, so provide the minimal header/entry needed for the boot verifier to
    // continue.
    let header = [1u32, 0, CRYPTOLIB_MAGIC, CRYPTOLIB_ENTRY as u32];
    for (i, word) in header.iter().enumerate() {
        let _ = mem.write_slice(
            &word.to_le_bytes(),
            GuestAddress(CRYPTOLIB_HEADER + 4 * i as u64),
        );
    }
    let code = [
        0xaa55_b537u32, // lui  a0, 0xaa55b
        0xa555_0513u32, // addi a0, a0, -0x5ab => 0xaa55aa55
        0x0000_8067u32, // ret
    ];
    for (i, word) in code.iter().enumerate() {
        let _ = mem.write_slice(
            &word.to_le_bytes(),
            GuestAddress(CRYPTOLIB_ENTRY + 4 * i as u64),
        );
    }
}

impl GscBridge {
    #[inline]
    fn in_uart(&self, addr: u64) -> bool {
        let base = self.cfg.uart_base;
        addr >= base && addr < base + UART_LEN
    }

    /// Record an MMIO access for the discovery trace. First-touch is logged for
    /// every address; accesses in the PMU control block (reset-source / global
    /// reset) are logged every time, since that region drives the boot path.
    fn note(&self, addr: u64, is_write: bool, value: u32) {
        if self.crypto_trace
            && ((0x4020_0000..0x4026_0000).contains(&addr)
                || (0x4041_0000..0x4042_0000).contains(&addr)
                || self.shared.lock().unwrap().tpm_active)
        {
            eprintln!(
                "[crypto] {:#010x} {} {:#010x} = {:#010x}",
                self.pc.load(Ordering::Relaxed),
                if is_write { "WR" } else { "RD" },
                addr,
                value,
            );
        }
        if self.cfg.trace == Trace::Off {
            return;
        }
        let pmu_ctrl = (PMU_BASE..PMU_CTRL_TOP).contains(&addr);
        let core_local = in_core_local(addr);
        let usb_or_event = (USB_BASE..USB_BASE + 0x200).contains(&addr)
            || matches!(addr, GSC_EVENT_USB_RESET | GSC_EVENT_USB_TRIGGER);
        let rbox = is_rbox(addr);
        let ap_spi = is_ap_spi(addr);
        let gsc_fifo = is_gsc_fifo(addr);
        let key = (addr << 1) | is_write as u64;
        let first_touch = self.shared.lock().unwrap().seen.insert(key);
        if first_touch || pmu_ctrl || core_local || usb_or_event || rbox || ap_spi || gsc_fifo {
            eprintln!(
                "[gsc] {:#010x} mmio {} {:#010x} = {:#010x}{}",
                self.pc.load(Ordering::Relaxed),
                if is_write { "WR" } else { "RD" },
                addr,
                value,
                if pmu_ctrl {
                    "  <PMU>"
                } else if core_local {
                    "  <COREIRQ>"
                } else if usb_or_event {
                    "  <USB>"
                } else if rbox {
                    "  <RBOX>"
                } else if ap_spi {
                    "  <AP_SPI>"
                } else if gsc_fifo {
                    "  <GSC_FIFO>"
                } else {
                    ""
                },
            );
        }
    }

    /// Emit a console byte (to stdout and the capture buffer).
    fn console_out(&self, byte: u8) {
        if self.cfg.console_trace {
            let shown = if byte.is_ascii_graphic() || byte == b' ' {
                byte as char
            } else {
                '.'
            };
            eprintln!(
                "[gsc] console pc={:#010x} byte={:#04x} '{}'",
                self.pc.load(Ordering::Relaxed),
                byte,
                shown
            );
        }
        self.shared.lock().unwrap().console.push(byte);
        let mut out = std::io::stdout();
        let _ = out.write_all(&[byte]);
        let _ = out.flush();
    }

    fn uart_irq_pending(&self) -> bool {
        let sh = self.shared.lock().unwrap();
        sh.uart_irq_armed && !sh.uart_rx.is_empty()
    }

    fn flash_gpa(&self, control: u64, off: u64) -> u64 {
        self.cfg.flash_img_base + flash_control_index(control) as u64 * TI50_FLASH_SLOT_SIZE + off
    }

    fn read_flash_word(&self, control: u64, off: u64) -> u32 {
        let mut bytes = [0xffu8; 4];
        let _ = self
            .mem
            .read_slice(&mut bytes, GuestAddress(self.flash_gpa(control, off)));
        u32::from_le_bytes(bytes)
    }

    fn flash_info_key(control: u64, off: u64) -> u64 {
        ((flash_control_index(control) as u64) << 32) | (off & 0xffff_ffff)
    }

    fn read_flash_info_word(&self, control: u64, off: u64) -> u32 {
        const AP_RO_CACHED_STATUS_OFF: u64 = 0x0c00;
        const AP_RO_CACHED_STATUS: [u8; 0x28] = {
            let mut bytes = [0u8; 0x28];
            bytes[0] = 1;
            bytes[4] = 1;
            // sub_9D33A accepts this status/complement pair and returns
            // success state 0. The following bytes overlap the first AP SPI
            // write-protect policy slot after the firmware's cached-record
            // copy helper repacks the flash-read DOUT lanes.
            bytes[8] = 0;
            bytes[9] = 0xff;
            // AP SPI write-protect policy words for SR-1/SR-2/SR-3. Each
            // word encodes two bytes as (value, ~value): expected, then mask.
            // The AP SPI model returns SR-1=0x02 and SR-2/SR-3=0x00.
            bytes[0x0a] = 0xff;
            bytes[0x0b] = 0x00;
            bytes[0x0c] = 0x02;
            bytes[0x0d] = 0xfd;
            bytes[0x0e] = 0xff;
            bytes[0x0f] = 0x00;
            bytes[0x10] = 0x00;
            bytes[0x11] = 0xff;
            bytes[0x12] = 0xff;
            bytes[0x13] = 0x00;
            bytes[0x14] = 0x00;
            bytes[0x15] = 0xff;
            bytes[0x16] = 0xff;
            bytes[0x17] = 0x00;
            bytes[0x18] = 0x00;
            bytes[0x19] = 0xff;
            bytes[0x1a] = 0xff;
            bytes[0x1b] = 0x00;
            bytes
        };

        let ap_ro_info_word = if (self.cfg.ap_ro_stub || self.cfg.ap_ro_info_stub)
            && flash_control_index(control) == 1
            && (AP_RO_CACHED_STATUS_OFF..AP_RO_CACHED_STATUS_OFF + AP_RO_CACHED_STATUS.len() as u64)
                .contains(&off)
        {
            let start = (off - AP_RO_CACHED_STATUS_OFF) as usize;
            let mut bytes = [0u8; 4];
            let n = (AP_RO_CACHED_STATUS.len() - start).min(4);
            bytes[..n].copy_from_slice(&AP_RO_CACHED_STATUS[start..start + n]);
            Some(u32::from_le_bytes(bytes))
        } else {
            None
        };
        if let Some(value) = ap_ro_info_word {
            if self.cfg.trace != Trace::Off {
                eprintln!("[gsc] AP RO cached INFO off={off:#x} -> {value:#010x}");
            }
            return value;
        }
        let value = self
            .shared
            .lock()
            .unwrap()
            .flash_info
            .get(&Self::flash_info_key(control, off))
            .copied()
            .unwrap_or(0xffff_ffff);
        if (self.cfg.ap_ro_stub || self.cfg.ap_ro_info_stub)
            && flash_control_index(control) == 1
            && (AP_RO_CACHED_STATUS_OFF..AP_RO_CACHED_STATUS_OFF + AP_RO_CACHED_STATUS.len() as u64)
                .contains(&off)
            && self.cfg.trace != Trace::Off
        {
            eprintln!("[gsc] AP RO cached INFO fallback off={off:#x} -> {value:#010x}");
        }
        value
    }

    fn program_flash_info_word(&self, control: u64, off: u64, value: u32) {
        let key = Self::flash_info_key(control, off);
        let mut sh = self.shared.lock().unwrap();
        let old = sh.flash_info.get(&key).copied().unwrap_or(0xffff_ffff);
        let programmed = old & value;
        if self.cfg.trace != Trace::Off {
            eprintln!(
                "[gsc] flash PROGRAM INFO control={} off={off:#x} old={old:#010x} value={value:#010x} -> {programmed:#010x}",
                flash_control_index(control)
            );
        }
        sh.flash_info.insert(key, programmed);
    }

    fn erase_flash_info_range(&self, control: u64, off: u64, len: usize) {
        if self.cfg.trace != Trace::Off {
            eprintln!(
                "[gsc] flash ERASE INFO control={} off={off:#x} len={len:#x}",
                flash_control_index(control)
            );
        }
        let start = off & !3;
        let end = start + len as u64;
        let mut sh = self.shared.lock().unwrap();
        let lane = flash_control_index(control) as u64;
        sh.flash_info.retain(|&key, _| {
            (key >> 32) != lane || (key & 0xffff_ffff) < start || (key & 0xffff_ffff) >= end
        });
    }

    fn program_flash_word(&self, control: u64, off: u64, value: u32) {
        let gpa = self.flash_gpa(control, off);
        let old = self.read_flash_word(control, off);
        let programmed = old & value;
        if self.cfg.trace != Trace::Off {
            eprintln!(
                "[gsc] flash PROGRAM control={} off={off:#x} gpa={gpa:#x} old={old:#010x} value={value:#010x} -> {programmed:#010x}",
                flash_control_index(control)
            );
        }
        let _ = self
            .mem
            .write_slice(&programmed.to_le_bytes(), GuestAddress(gpa));
    }

    fn erase_flash_range(&self, control: u64, off: u64, len: usize) {
        if self.cfg.trace != Trace::Off {
            let gpa = self.flash_gpa(control, off);
            eprintln!(
                "[gsc] flash ERASE control={} off={off:#x} gpa={gpa:#x} len={len:#x}",
                flash_control_index(control)
            );
        }
        let blank = vec![0xffu8; len];
        let _ = self
            .mem
            .write_slice(&blank, GuestAddress(self.flash_gpa(control, off)));
    }

    fn execute_flash_op(
        &self,
        control: u64,
        opcode: u32,
        trans: u32,
        read_trans: u32,
        wr_data: [u32; 2],
    ) -> [u32; 2] {
        let prog_off = flash_trans_offset(trans);
        let read_off = flash_read_trans_offset(read_trans);
        let prog_info = flash_trans_is_info(trans);
        let read_info = flash_read_trans_is_info(read_trans);
        if self.cfg.trace != Trace::Off {
            eprintln!(
                "[gsc] flash OP control={} opcode={opcode:#010x} trans={trans:#010x} off={prog_off:#x} prog_info={} read_trans={read_trans:#010x} read_off={read_off:#x} read_info={} bank_bit={} hi_bit={}",
                flash_control_index(control),
                prog_info,
                read_info,
                (trans >> 6) & 1,
                (trans >> 23) & 1,
            );
        }
        match opcode {
            FLASH_OP_TI50_PROGRAM | FLASH_OP_CR50_PROGRAM => {
                if prog_info {
                    self.program_flash_info_word(control, prog_off, wr_data[0]);
                    self.program_flash_info_word(control, prog_off + 4, wr_data[1]);
                } else {
                    self.program_flash_word(control, prog_off, wr_data[0]);
                    self.program_flash_word(control, prog_off + 4, wr_data[1]);
                }
                [0, 0]
            }
            FLASH_OP_CR50_READ => {
                if read_info {
                    [
                        self.read_flash_info_word(control, read_off),
                        self.read_flash_info_word(control, read_off + 4),
                    ]
                } else {
                    [
                        self.read_flash_word(control, read_off),
                        self.read_flash_word(control, read_off + 4),
                    ]
                }
            }
            FLASH_OP_CR50_ERASE => {
                if prog_info {
                    self.erase_flash_info_range(control, prog_off & !0x7ff, 0x800);
                } else {
                    self.erase_flash_range(control, prog_off & !0x7ff, 0x800);
                }
                [0, 0]
            }
            _ => {
                if self.cfg.trace != Trace::Off {
                    eprintln!(
                        "[gsc] flash op {opcode:#010x} control={} at TRANS {trans:#010x} treated as complete",
                        flash_control_index(control)
                    );
                }
                [0, 0]
            }
        }
    }

    fn ap_spi_tx_len(xfer_cfg: u32) -> usize {
        let tx_bits_minus_one = (xfer_cfg >> 7) & 0x0fff;
        (tx_bits_minus_one as usize + 8) / 8
    }

    fn ap_spi_rx_base_offset(xfer_cfg: u32) -> usize {
        (Self::ap_spi_tx_len(xfer_cfg) + 3) & !3
    }

    fn ap_spi_payload_offset(xfer_cfg: u32) -> usize {
        Self::ap_spi_rx_base_offset(xfer_cfg) + Self::ap_spi_tx_len(xfer_cfg)
    }

    fn ap_spi_rx_capacity(xfer_cfg: u32) -> usize {
        let rx_words = ((xfer_cfg >> 19) & 0x7f) as usize + 1;
        rx_words * 4
    }

    fn execute_ap_spi_transaction(&self, xact: u32) -> u32 {
        let mut sh = self.shared.lock().unwrap();
        if sh.ap_spi_data.len() != AP_SPI_DATA_LEN {
            sh.ap_spi_data.resize(AP_SPI_DATA_LEN, 0);
        }
        let xfer_cfg = sh.store.get(&AP_SPI_XFER_CFG).copied().unwrap_or(0);
        let rx = Self::ap_spi_payload_offset(xfer_cfg).min(AP_SPI_DATA_LEN - 1);
        let tx_len = Self::ap_spi_tx_len(xfer_cfg);
        let payload_len = Self::ap_spi_rx_capacity(xfer_cfg).saturating_sub(tx_len);
        let opcode = sh.ap_spi_data.first().copied().unwrap_or(0);

        match opcode {
            // The AP RO verifier reads the external AP flash write-protect
            // status bytes with opcodes 0x05, 0x35, and 0x15. The first cached
            // policy word in the production image expects SR-1 bit1 set; the
            // remaining status bytes are accepted as zero.
            AP_SPI_OP_READ_SR1 | AP_SPI_OP_READ_SR2 | AP_SPI_OP_READ_SR3 => {
                let status = match opcode {
                    AP_SPI_OP_READ_SR1 => 0x02,
                    _ => 0x00,
                };
                sh.ap_spi_data[rx] = status;
                if self.cfg.trace != Trace::Off {
                    eprintln!(
                        "[gsc] AP SPI xact opcode={opcode:#04x} cfg={xfer_cfg:#010x} rx_off={rx:#x} value={status:#04x}"
                    );
                }
            }
            AP_SPI_OP_FAST_READ => {
                let addr = ap_spi_fast_read_addr(&sh.ap_spi_data);
                let len = payload_len.min(AP_SPI_DATA_LEN - rx);
                for i in 0..len {
                    sh.ap_spi_data[rx + i] = self
                        .ap_flash
                        .get(addr as usize + i)
                        .copied()
                        .unwrap_or(0xff);
                }
                if self.cfg.trace != Trace::Off {
                    let first = sh.ap_spi_data.get(rx).copied().unwrap_or(0xff);
                    eprintln!(
                        "[gsc] AP SPI fast-read addr={addr:#08x} cfg={xfer_cfg:#010x} rx_off={rx:#x} len={len:#x} first={first:#04x}"
                    );
                }
            }
            _ => {
                let len = payload_len.min(AP_SPI_DATA_LEN - rx);
                for i in 0..len {
                    sh.ap_spi_data[rx + i] = 0xff;
                }
                if self.cfg.trace != Trace::Off {
                    eprintln!(
                        "[gsc] AP SPI xact opcode={opcode:#04x} cfg={xfer_cfg:#010x} rx_off={rx:#x} len={len:#x} blank"
                    );
                }
            }
        }
        xact & !AP_SPI_XACT_START
    }

    fn globalsec_region_word(&self, addr: u64) -> Option<u32> {
        match addr {
            GLOBALSEC_CRYPTOLIB_BASE => Some(CRYPTOLIB_ROM_BASE as u32),
            GLOBALSEC_ACTIVE_RO_BASE => Some(self.cfg.flash_img_base as u32),
            GLOBALSEC_ACTIVE_RO_SIZE => Some(TI50_RO_IMAGE_SIZE),
            GLOBALSEC_ACTIVE_RW_BASE => {
                Some((self.cfg.flash_img_base + TI50_RW_SLOT_OFFSET) as u32)
            }
            GLOBALSEC_ACTIVE_RW_SIZE => Some(TI50_RW_IMAGE_SIZE),
            _ => None,
        }
    }
}

impl Memory for GscBridge {
    fn probe(&self, addr: u64, size: usize, _write: bool) -> MemResult<()> {
        if in_core_local(addr) || in_mmio(addr) || self.mem.check_range(GuestAddress(addr), size) {
            Ok(())
        } else {
            Err(MemError::OutOfBounds { addr, size })
        }
    }

    fn read(&self, addr: u64, buf: &mut [u8]) -> MemResult<()> {
        if in_core_local(addr) {
            let v = if addr == CORE_IRQ_CLAIM {
                if self.uart_irq_pending() {
                    CORE_IRQ_UART0
                } else {
                    CORE_IRQ_NONE
                }
            } else if addr == CORE_IRQ_EPOCH {
                0
            } else {
                self.shared
                    .lock()
                    .unwrap()
                    .store
                    .get(&addr)
                    .copied()
                    .unwrap_or(self.cfg.open_bus)
            };
            self.note(addr, false, v);
            fill_le_word(buf, v);
            return Ok(());
        }
        if in_mmio(addr) {
            // The PMU reset-source register is stateful (cold = POR, warm =
            // software reset; cleared by CLRRST), so the firmware classifies the
            // reset cause instead of seeing "Other" and rebooting forever.
            if addr == PMU_RSTSRC_REG {
                let v = self.shared.lock().unwrap().rstsrc;
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            if addr == PMU_CHIP_ID_REG || addr == PMU_CHIP_ID_ALT_REG {
                self.note(addr, false, PMU_CHIP_ID);
                fill_le_word(buf, PMU_CHIP_ID);
                return Ok(());
            }
            if addr == PMU_WAKE_EXIT_SRC_REG {
                self.note(addr, false, 0);
                fill_le_word(buf, 0);
                return Ok(());
            }
            // Flash controller ERROR (+0x9c) and STATUS (+0x18/+0x1c) read 0 (op
            // complete, no error). Read commands fill DOUT0/1; program/erase
            // commands update either the mapped XIP image or the separate blank
            // INFO-bank store.
            if matches!(addr, FLASH_ERROR | FLASH_STATUS0 | FLASH_STATUS1) {
                let v = if addr == FLASH_ERROR {
                    self.shared.lock().unwrap().flash_error
                } else {
                    0
                };
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            if matches!(
                addr,
                FLASH_DOUT0_LO | FLASH_DOUT0_HI | FLASH_DOUT1_LO | FLASH_DOUT1_HI
            ) {
                let lane = usize::from(matches!(addr, FLASH_DOUT1_LO | FLASH_DOUT1_HI));
                let word = usize::from(matches!(addr, FLASH_DOUT0_HI | FLASH_DOUT1_HI));
                let v = self.shared.lock().unwrap().flash_dout[lane][word];
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            if let Some(v) = self.globalsec_region_word(addr) {
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            if addr == GSC_EVENT_USB_RESET {
                let v = 0x8000_0000;
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            if addr == USB_SOFT_RESET || is_usb_w1c_status(addr) {
                self.note(addr, false, 0);
                fill_le_word(buf, 0);
                return Ok(());
            }
            if addr == RBOX_STATUS {
                let sh = self.shared.lock().unwrap();
                let control0_ack = sh.store.get(&RBOX_CONTROL0).copied().unwrap_or(0) & 1;
                let control1 = sh.store.get(&RBOX_CONTROL1).copied().unwrap_or(1) & 1;
                let control1_ack = (control1 ^ 1) << 5;
                let v = control0_ack | control1_ack;
                drop(sh);
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            if addr == RBOX_INIT_READY {
                self.note(addr, false, 1);
                fill_le_word(buf, 1);
                return Ok(());
            }
            if addr == RBOX_CMD_STATUS {
                self.note(addr, false, 0);
                fill_le_word(buf, 0);
                return Ok(());
            }
            if is_rbox_w1c_status(addr) {
                let v = self
                    .shared
                    .lock()
                    .unwrap()
                    .store
                    .get(&addr)
                    .copied()
                    .unwrap_or(0);
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            if let Some(off) = ap_spi_data_offset(addr) {
                let mut word = [0u8; 4];
                {
                    let sh = self.shared.lock().unwrap();
                    for (i, b) in buf.iter_mut().enumerate() {
                        *b = sh.ap_spi_data.get(off + i).copied().unwrap_or(0);
                    }
                    for (i, b) in word.iter_mut().enumerate() {
                        *b = sh.ap_spi_data.get(off + i).copied().unwrap_or(0);
                    }
                }
                self.note(addr, false, u32::from_le_bytes(word));
                return Ok(());
            }
            // GscFifo dual-port command/response RAM: byte-accurate reads so the
            // firmware reads the staged TPM command (and its own response) from
            // the real FIFO window.
            if let Some(off) = fifo_ram_offset(addr) {
                let sh = self.shared.lock().unwrap();
                for (i, b) in buf.iter_mut().enumerate() {
                    *b = sh.fifo_ram.get(off + i).copied().unwrap_or(0);
                }
                return Ok(());
            }
            if addr == AP_SPI_XACT {
                let v = self
                    .shared
                    .lock()
                    .unwrap()
                    .store
                    .get(&addr)
                    .copied()
                    .unwrap_or(0)
                    & !AP_SPI_XACT_START;
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            if is_ap_spi_w1c_status(addr) {
                let v = self
                    .shared
                    .lock()
                    .unwrap()
                    .store
                    .get(&addr)
                    .copied()
                    .unwrap_or(0);
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            if is_ap_spi(addr) {
                let v = self
                    .shared
                    .lock()
                    .unwrap()
                    .store
                    .get(&addr)
                    .copied()
                    .unwrap_or(0);
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            if addr == GSC_FIFO_CONTROL {
                let v = self
                    .shared
                    .lock()
                    .unwrap()
                    .store
                    .get(&addr)
                    .copied()
                    .unwrap_or(0)
                    & !GSC_FIFO_RESET_BUSY_MASK;
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            if addr == GSC_FIFO_IRQ_STATUS {
                self.note(addr, false, 0);
                fill_le_word(buf, 0);
                return Ok(());
            }
            if is_gsc_fifo_w1c_status(addr) {
                let v = self
                    .shared
                    .lock()
                    .unwrap()
                    .store
                    .get(&addr)
                    .copied()
                    .unwrap_or(0);
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            if self.in_uart(addr) {
                let off = addr - self.cfg.uart_base;
                if off == UART_RDATA {
                    let v = {
                        let mut sh = self.shared.lock().unwrap();
                        if sh.uart_irq_armed {
                            sh.uart_rx.pop_front().map(u32::from)
                        } else {
                            None
                        }
                    }
                    .unwrap_or(self.cfg.open_bus);
                    self.note(addr, false, v);
                    fill_le_word(buf, v);
                    return Ok(());
                }
                if off == UART_STATE {
                    let rx_empty = {
                        let sh = self.shared.lock().unwrap();
                        !sh.uart_irq_armed || sh.uart_rx.is_empty()
                    };
                    let mut v = self.cfg.uart_state & !UART_STATE_TX_BUSY;
                    if rx_empty {
                        v |= UART_STATE_RX_EMPTY;
                    } else {
                        v &= !UART_STATE_RX_EMPTY;
                    }
                    self.note(addr, false, v);
                    fill_le_word(buf, v);
                    return Ok(());
                }
            }
            if addr == TRNG_READ_DATA {
                let v = {
                    let mut sh = self.shared.lock().unwrap();
                    if sh.trng_state == 0 {
                        sh.trng_state = 0x6d5a_56a5;
                    }
                    let mut x = sh.trng_state;
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    sh.trng_state = x;
                    x
                };
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            if is_gpio_input_bank(addr) {
                let v = self.ready.get(&addr).copied().unwrap_or_else(|| {
                    self.shared
                        .lock()
                        .unwrap()
                        .store
                        .get(&addr)
                        .copied()
                        .unwrap_or(0)
                });
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            if is_gpio_w1c_status(addr) {
                let v = self
                    .shared
                    .lock()
                    .unwrap()
                    .store
                    .get(&addr)
                    .copied()
                    .unwrap_or(0);
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            if is_gpio_cfg(addr) {
                let v = self
                    .shared
                    .lock()
                    .unwrap()
                    .store
                    .get(&addr)
                    .copied()
                    .unwrap_or(0);
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            if (FUSE_BASE..FUSE_TOP).contains(&addr) {
                let v = self
                    .shared
                    .lock()
                    .unwrap()
                    .store
                    .get(&addr)
                    .copied()
                    .unwrap_or(FUSE_DEFAULT);
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            // Free-running 64-bit timer/counter: the firmware reads `now` as
            // `PAIR64(+0x1C, +0x14)`. Derive it from the retired-instruction
            // count so the firmware's millisecond timestamps advance. The busy
            // status (`+0x78`) always reads "ready" (0). Other timer registers
            // (enable/prescale/masks) fall through to the persistent store,
            // which already holds the values the init routine wrote.
            if matches!(
                addr,
                TIMER_COUNT_LO | TIMER_COUNT_HI | TIMER2_COUNT_LO | TIMER2_COUNT_HI
            ) {
                let ticks = self.timer_ticks();
                let v = if addr == TIMER_COUNT_LO || addr == TIMER2_COUNT_LO {
                    ticks as u32
                } else {
                    (ticks >> 32) as u32
                };
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            if addr == TIMER_BUSY || addr == TIMER2_BUSY {
                self.note(addr, false, 0);
                fill_le_word(buf, 0);
                return Ok(());
            }
            // DRBG/CSRNG keymgr: report "operation done, success" (`+0x10`
            // bits[2:0] == 1) and a non-zero PRNG for the 256-bit output files,
            // so the firmware's instantiate/generate succeeds instead of
            // failing and downstream key/entropy material is non-zero.
            if addr == DRBG_STATUS {
                self.note(addr, false, 1);
                fill_le_word(buf, 1);
                return Ok(());
            }
            if (DRBG_OUT_LO..DRBG_OUT_LO + DRBG_OUT_LEN).contains(&addr)
                || (DRBG_OUT_HI..DRBG_OUT_HI + DRBG_OUT_LEN).contains(&addr)
            {
                let v = self.drbg_word(addr);
                self.note(addr, false, v);
                fill_le_word(buf, v);
                return Ok(());
            }
            // UART registers go through the normal ready/store path so the
            // firmware sees a consistent device (STATE is a fixed "TX ready" in
            // the ready map; WDATA writes are tapped for console output below).
            let word = if let Some(&v) = self.ready.get(&addr) {
                v
            } else {
                // Persistent register read with the generic spin-breaker: an
                // unmodeled register polled many times with no intervening
                // write is almost certainly a "wait for ready" loop, so return
                // all-ones once to break it.
                let mut sh = self.shared.lock().unwrap();
                let base = sh.store.get(&addr).copied().unwrap_or(self.cfg.open_bus);
                let c = sh.read_counts.entry(addr).or_insert(0);
                *c += 1;
                if *c >= SPIN_THRESHOLD {
                    *c = 0;
                    // Alternate the break value so "wait for set" (needs ones)
                    // and "wait for clear" (needs zeros) loops both resolve.
                    let phase = sh.spin_phase.entry(addr).or_insert(false);
                    *phase = !*phase;
                    if *phase { 0xffff_ffff } else { 0x0000_0000 }
                } else {
                    base
                }
            };
            self.note(addr, false, word);
            fill_le_word(buf, word);
            return Ok(());
        }
        self.mem
            .read_slice(buf, GuestAddress(addr))
            .map_err(|_| MemError::OutOfBounds {
                addr,
                size: buf.len(),
            })
    }

    fn write(&mut self, addr: u64, data: &[u8]) -> MemResult<()> {
        if in_core_local(addr) {
            let mut word = [0u8; 4];
            let n = data.len().min(4);
            word[..n].copy_from_slice(&data[..n]);
            let value = u32::from_le_bytes(word);
            self.shared.lock().unwrap().store.insert(addr, value);
            self.note(addr, true, value);
            return Ok(());
        }
        if in_mmio(addr) {
            let mut word = [0u8; 4];
            let n = data.len().min(4);
            word[..n].copy_from_slice(&data[..n]);
            let value = u32::from_le_bytes(word);
            let mut flash_op = None;
            let mut ap_spi_xact = None;
            if let Some(off) = ap_spi_data_offset(addr) {
                {
                    let mut sh = self.shared.lock().unwrap();
                    sh.read_counts.clear();
                    if sh.ap_spi_data.len() != AP_SPI_DATA_LEN {
                        sh.ap_spi_data.resize(AP_SPI_DATA_LEN, 0);
                    }
                    for (i, b) in data.iter().copied().enumerate() {
                        if off + i < AP_SPI_DATA_LEN {
                            sh.ap_spi_data[off + i] = b;
                        }
                    }
                }
                self.note(addr, true, value);
                return Ok(());
            }
            // GscFifo dual-port RAM: byte-accurate writes (the firmware writes
            // its TPM response into this real FIFO window).
            if let Some(off) = fifo_ram_offset(addr) {
                let mut sh = self.shared.lock().unwrap();
                sh.read_counts.clear();
                if sh.fifo_ram.len() != GSC_FIFO_RAM_LEN {
                    sh.fifo_ram.resize(GSC_FIFO_RAM_LEN, 0);
                }
                for (i, b) in data.iter().copied().enumerate() {
                    if off + i < GSC_FIFO_RAM_LEN {
                        sh.fifo_ram[off + i] = b;
                    }
                }
                self.note(addr, true, value);
                return Ok(());
            }
            {
                let mut sh = self.shared.lock().unwrap();
                // A write is forward progress: reset the spin-breaker window.
                sh.read_counts.clear();
                let stored = if is_gpio_w1c_status(addr) {
                    sh.store.get(&addr).copied().unwrap_or(0) & !value
                } else if is_rbox_w1c_status(addr) {
                    sh.store.get(&addr).copied().unwrap_or(0) & !value
                } else if is_ap_spi_w1c_status(addr) {
                    sh.store.get(&addr).copied().unwrap_or(0) & !value
                } else if addr == AP_SPI_XACT && value & AP_SPI_XACT_START != 0 {
                    value & !AP_SPI_XACT_START
                } else if is_gsc_fifo_w1c_status(addr) {
                    sh.store.get(&addr).copied().unwrap_or(0) & !value
                } else if addr == USB_SOFT_RESET || is_usb_w1c_status(addr) {
                    0
                } else {
                    value
                };
                sh.store.insert(addr, stored);
                // Machine-timer alarm compare: the firmware writes the 64-bit
                // deadline (low `+0x2C`, high `+0x34`); init disarms with -1.
                // The run loop raises MTIP once the counter reaches it.
                if addr == TIMER_COMPARE_LO {
                    sh.timer_compare = (sh.timer_compare & 0xffff_ffff_0000_0000) | value as u64;
                } else if addr == TIMER_COMPARE_HI {
                    sh.timer_compare =
                        (sh.timer_compare & 0x0000_0000_ffff_ffff) | ((value as u64) << 32);
                }
                if addr == PMU_CLRRST_REG {
                    // CLRRST: clear the reported reset-source bits.
                    sh.rstsrc &= !value;
                }
                if addr == PMU_RESET_REG && value == PMU_RESET_MAGIC {
                    sh.reset_requested = true;
                }
                if addr == AP_SPI_XACT && value & AP_SPI_XACT_START != 0 {
                    ap_spi_xact = Some(value);
                }
                // Flash controller programming sequence.
                if addr == FLASH_PE_EN {
                    sh.flash_pe_en = value == FLASH_PE_EN_MAGIC;
                } else if addr == FLASH_READ_TRANS {
                    sh.flash_read_trans = value;
                } else if addr == FLASH_TRANS {
                    sh.flash_trans = value;
                } else if addr == FLASH_WR_DATA0 {
                    sh.flash_wr_data[0] = value;
                } else if addr == FLASH_WR_DATA1 {
                    sh.flash_wr_data[1] = value;
                } else if is_flash_control(addr) && sh.flash_pe_en {
                    flash_op = Some((
                        addr,
                        value,
                        sh.flash_trans,
                        sh.flash_read_trans,
                        sh.flash_wr_data,
                    ));
                    sh.flash_pe_en = false;
                    sh.store.insert(addr, 0);
                }
            }
            if let Some((control, opcode, trans, read_trans, wr_data)) = flash_op {
                let dout = self.execute_flash_op(control, opcode, trans, read_trans, wr_data);
                let mut sh = self.shared.lock().unwrap();
                sh.flash_dout[flash_control_index(control)] = dout;
                sh.flash_error = 0;
                sh.store.insert(control, 0);
            }
            if let Some(xact) = ap_spi_xact {
                let stored = self.execute_ap_spi_transaction(xact);
                self.shared
                    .lock()
                    .unwrap()
                    .store
                    .insert(AP_SPI_XACT, stored);
            }
            self.note(addr, true, value);
            // Console sniffer: any byte-sized printable write to an MMIO
            // register is a candidate UART TX — log it so the real console base
            // can be discovered.
            if self.cfg.trace != Trace::Off {
                let b = value & 0xff;
                let printable = (0x20..0x7f).contains(&b) || b == 0x0a || b == 0x0d || b == 0x09;
                if value <= 0xff && printable {
                    eprintln!(
                        "[gsc] {:#010x} CONSOLE? {:#010x} <- {:#04x} '{}'",
                        self.pc.load(Ordering::Relaxed),
                        addr,
                        b,
                        b as u8 as char
                    );
                }
            }
            if self.in_uart(addr) && addr - self.cfg.uart_base == UART_WDATA {
                self.console_out((value & 0xff) as u8);
            }
            return Ok(());
        }
        if self.cfg.trace != Trace::Off
            && (self.cfg.flash_img_base..self.cfg.flash_img_base + 0x10_0000).contains(&addr)
        {
            let mut word = [0u8; 4];
            let n = data.len().min(4);
            word[..n].copy_from_slice(&data[..n]);
            let value = u32::from_le_bytes(word);
            eprintln!(
                "[gsc] {:#010x} XIP WR {:#010x} size={} value={:#010x}",
                self.pc.load(Ordering::Relaxed),
                addr,
                data.len(),
                value
            );
        }
        self.mem
            .write_slice(data, GuestAddress(addr))
            .map_err(|_| MemError::OutOfBounds {
                addr,
                size: data.len(),
            })
    }
}

/// GSC (Ti50/Dauntless) vCPU: an RV32 + Xsoteria hart over [`GscBridge`].
pub struct GscVcpu {
    id: u32,
    cpu: RiscVCpu,
    shared: Shared,
    cfg: GscConfig,
    halted: bool,
    /// Entry PC to jump to on a firmware-requested warm reset (the boot entry).
    reset_entry: u64,
    /// Number of warm resets so far (bounded by [`MAX_RESETS`]).
    reset_count: u32,
    /// Published to [`GscBridge`] before each step for PC-correlated tracing.
    pc_cell: Arc<AtomicU64>,
    /// Published to [`GscBridge`] each step to drive the free-running timer.
    time_cell: Arc<AtomicU64>,
    /// Optional breakpoint PC: dump registers + the call site on first hit
    /// (env `RAX_GSC_BREAK`). Used to diagnose panics/asserts during bring-up.
    break_pc: Option<u64>,
    /// 1-based occurrence of `break_pc` to dump (`RAX_GSC_BREAK_HIT`).
    break_hit: u64,
    /// Optional RA filter for breakpoint hits (`RAX_GSC_BREAK_RA`).
    break_ra: Option<u64>,
    /// Optional saved-RA filter for helper frames (`RAX_GSC_BREAK_SAVED_RA`).
    break_saved_ra: Option<u64>,
    /// Optional number of bytes to dump from `sp` when the breakpoint fires.
    break_stack_bytes: u64,
    break_stop: bool,
    syscall_trace: bool,
    print_trace: bool,
    ap_ro_stub: bool,
    ap_ro_crypto_stub: bool,
    break_hits_seen: u64,
    broke: bool,
    /// TPM command injection (`RAX_GSC_TPM_CMD=<hexbytes>`, comma-separated for
    /// several): once the firmware reaches the idle WFI after boot,
    /// synthetically call the TPM 2.0 ExecuteCommand wrapper (`sub_E06BE`) with
    /// each command and print the response. Demonstrates the firmware
    /// processing real host TPM commands.
    tpm_cmds: Vec<Vec<u8>>,
    tpm_trigger: u64,
    /// `true` (default) drives commands through the firmware task's own yield
    /// (`yield` mode, proper context); `false` uses an out-of-context synthetic
    /// call to `sub_E06BE` (`call` mode, `RAX_GSC_TPM_MODE=call`).
    tpm_yield_mode: bool,
    /// `RAX_GSC_TPM_WIRED=1`: stage the command in (and read the response from)
    /// the real GscFifo dual-port RAM window (`0x40621000`) so the TPM frame
    /// transits the actual host-interface hardware instead of scratch RAM. The
    /// firmware's SPS receive-ISR (which copies FIFO→`0x220C0` and wakes the
    /// task) lives in the RO image, so that wake step is still bridged.
    tpm_wired: bool,
    tpm: TpmInject,
    /// `RAX_GSC_PLT_RST_EVENT=1`: boot with the AP held in reset, then deassert
    /// PLT_RST_L at runtime (after the boot settles) and raise the GPIO event,
    /// modeling the host powering on while the GSC is already running.
    plt_rst_event: bool,
    plt_rst_fired: bool,
    /// Disable the entropy hooks (`RAX_GSC_NO_ENTROPY=1`). By default the
    /// firmware's randomness primitives (`sub_D5558`/`sub_D5606`) are filled
    /// with a PRNG so TPM RNG/key/nonce output is non-zero; the underlying
    /// kernel crypto service writes zeros in this board model.
    entropy_hooks: bool,
    /// SplitMix64 state advanced once per random byte produced by the hooks.
    entropy_state: u64,
}

/// State of an in-flight synthetic TPM command call.
#[derive(Default)]
struct TpmInject {
    /// Index of the next command to send.
    next: usize,
    /// True while a synthetic `sub_E06BE` call is executing (`call` mode).
    in_call: bool,
    /// True after a command was planted at the task yield and we are waiting
    /// for the task to reach the response-send (`yield` mode).
    awaiting: bool,
    /// Remaining bounded trace lines after a yield-mode plant (debug).
    trace_budget: u32,
    /// Instruction-retire count when the in-flight call began (watchdog).
    call_start_instret: u64,
    /// Saved integer registers (x1..x31), pc, mstatus, and mie to restore
    /// after the synthetic call so the firmware resumes its idle loop.
    saved_x: [u64; 32],
    saved_pc: u64,
    saved_mstatus: u64,
    saved_mie: u64,
    /// Scratch slots passed by reference to the wrapper: capacity (a2) and the
    /// response-buffer pointer (a3); both are read back after the call.
    cap_slot: u64,
    bufptr_slot: u64,
}

/// Wrapper VA for the TPM 2.0 ExecuteCommand path (`sub_E06BE`); it unmarshals
/// the command from `a0`/`a1`, runs the dispatcher, and leaves the response in
/// the buffer with the new length/pointer written back through `a2`/`a3`.
const TPM_EXECUTE_WRAPPER: u64 = 0xe06be;
/// `ecall` VA of the TPM task's command-wait yield (`sub_D3904` LABEL_47). The
/// following `c.j` loops back to re-read the command-pending flag at `0x220BC`.
/// Intercepting this yield lets us plant a command and have the task process
/// it *in its own scheduler context* (all upcalls/clients registered).
const TPM_TASK_YIELD_ECALL: u64 = 0xd3cca;
/// Instruction the yield falls through to (`c.j -0x198` back to the loop top).
const TPM_TASK_YIELD_NEXT: u64 = 0xd3cce;
/// Entry of the TPM response-send helper (`sub_D4B80`), called right after
/// `sub_E06BE` with the response in the `0x220C0` buffer and the length in
/// `a1`. We capture the response here, before the (host-less) send would block.
const TPM_RESPONSE_SEND: u64 = 0xd4b80;
/// TPM command-processor globals (`sub_D3904`): pending flag, RX/TX buffer
/// pointer, command length, and capacity descriptor (low 24 bits = size).
const TPM_GLOBAL_PENDING: u64 = 0x2_20bc;
const TPM_GLOBAL_BUF: u64 = 0x2_20c0;
const TPM_GLOBAL_LEN: u64 = 0x2_20c4;
const TPM_GLOBAL_CAP: u64 = 0x2_20c8;
/// Sentinel return address for the synthetic call: chosen outside any mapped
/// code so the run loop detects "the call returned" before fetching there.
const TPM_CALL_SENTINEL: u64 = 0xffff_fff0;
/// Backed-RAM scratch for the injected command/response and the two
/// by-reference argument slots (clear of firmware SRAM and the flash XIP
/// window, inside the flat guest-RAM aperture).
const TPM_SCRATCH_BUF: u64 = 0x0030_0000;
const TPM_SCRATCH_CAP: u64 = 0x0030_1000;
const TPM_SCRATCH_BUFPTR: u64 = 0x0030_1004;
/// Response-buffer capacity advertised to the firmware (low 24 bits of a2).
const TPM_SCRATCH_CAPACITY: u32 = 0x1000;
/// Watchdog: abort a synthetic TPM call that retires more than this many
/// instructions without returning (guards against an unmodeled path looping).
const TPM_CALL_MAX_INSNS: u64 = 200_000_000;

/// TPM randomness primitives. The firmware delegates random generation to a
/// kernel crypto service (#1003) over `ecall`, which writes zeros into the
/// allowed buffer while returning success — so TPM2_GetRandom, nonces, and
/// generated keys come back all-zero. These two functions are the in-firmware
/// chokepoints; the run loop fills their destination buffer with a PRNG and
/// returns the requested length so randomness is non-zero.
/// `sub_D5558(ctx a0, dest a1, count a2)` — CryptRandomGenerate; returns count.
const TPM_RAND_GENERATE: u64 = 0xd5558;
const TPM_RAND_GENERATE_B: u64 = 0xd5558 + 0x80000;
/// `sub_D5606(count a0, dest a1)` — nonce/session fill; returns count.
const TPM_RAND_NONCE: u64 = 0xd5606;
const TPM_RAND_NONCE_B: u64 = 0xd5606 + 0x80000;

impl GscVcpu {
    pub fn new(id: u32, mem: Arc<GuestMemoryMmap>) -> Self {
        let cfg = GscConfig::from_env();
        install_synthetic_cryptolib(&mem);
        let uart_rx = std::env::var("RAX_GSC_UART_RX")
            .map(|s| s.into_bytes().into())
            .unwrap_or_default();
        let shared: Shared = Arc::new(Mutex::new(GscShared {
            rstsrc: cfg.rstsrc_cold,
            uart_rx,
            timer_compare: u64::MAX,
            ..GscShared::default()
        }));
        let mut ready = builtin_ready_map();
        // The console UART STATE register reads back "TX ready" so the
        // firmware's transmit-ready poll always passes, at whatever base.
        ready.insert(cfg.uart_base + UART_STATE, cfg.uart_state);
        // `RAX_GSC_AP_ON=1` deasserts PLT_RST_L (GPIO bank0 bit11) so the
        // firmware sees the AP host powered on and processes TPM commands
        // instead of dropping them "while AP off".
        if std::env::var("RAX_GSC_AP_ON").is_ok() {
            let v = ready.entry(GPIO_INPUT_BANK0).or_insert(0);
            *v |= GPIO_PLT_RST_L_MASK;
        }
        ready.extend(parse_ready_env());
        let pc_cell = Arc::new(AtomicU64::new(0));
        let time_cell = Arc::new(AtomicU64::new(0));
        let bridge = GscBridge {
            mem,
            shared: shared.clone(),
            ap_flash: load_ap_flash_from_env(),
            cfg,
            ready,
            pc: pc_cell.clone(),
            time: time_cell.clone(),
            crypto_trace: std::env::var("RAX_GSC_CRYPTO_TRACE").is_ok(),
        };
        let cpu = RiscVCpu::new(RiscVConfig::rv32(Isa::ti50()), Box::new(bridge));
        GscVcpu {
            id,
            cpu,
            shared,
            cfg,
            halted: false,
            reset_entry: 0,
            reset_count: 0,
            pc_cell,
            time_cell,
            break_pc: env_hex("RAX_GSC_BREAK"),
            break_hit: env_hex("RAX_GSC_BREAK_HIT").unwrap_or(1).max(1),
            break_ra: env_hex("RAX_GSC_BREAK_RA"),
            break_saved_ra: env_hex("RAX_GSC_BREAK_SAVED_RA"),
            break_stack_bytes: env_hex("RAX_GSC_BREAK_STACK").unwrap_or(0),
            break_stop: std::env::var("RAX_GSC_BREAK_STOP").is_ok(),
            syscall_trace: std::env::var("RAX_GSC_SYSCALL_TRACE").is_ok(),
            print_trace: std::env::var("RAX_GSC_PRINT_TRACE").is_ok(),
            ap_ro_stub: cfg.ap_ro_stub,
            ap_ro_crypto_stub: cfg.ap_ro_crypto_stub || cfg.ap_ro_stub,
            break_hits_seen: 0,
            broke: false,
            tpm_cmds: parse_tpm_cmds_env(),
            tpm_trigger: env_hex("RAX_GSC_TPM_TRIGGER").unwrap_or(0xa2d3c),
            tpm_yield_mode: std::env::var("RAX_GSC_TPM_MODE").as_deref() != Ok("call"),
            tpm_wired: std::env::var("RAX_GSC_TPM_WIRED").is_ok(),
            tpm: TpmInject::default(),
            plt_rst_event: std::env::var("RAX_GSC_PLT_RST_EVENT").is_ok(),
            plt_rst_fired: false,
            entropy_hooks: !std::env::var("RAX_GSC_NO_ENTROPY").is_ok(),
            entropy_state: 0x243f_6a88_85a3_08d3,
        }
    }

    /// Drain a pending warm-reset request, returning whether the run loop should
    /// reboot the hart. Enforces the [`MAX_RESETS`] loop guard.
    fn take_reset(&mut self) -> Option<VcpuExit> {
        let requested = {
            let mut sh = self.shared.lock().unwrap();
            std::mem::take(&mut sh.reset_requested)
        };
        if !requested {
            return None;
        }
        self.reset_count += 1;
        if self.reset_count > MAX_RESETS {
            self.halted = true;
            return Some(VcpuExit::Unknown(format!(
                "gsc: firmware reboot loop ({} resets) without reaching console",
                self.reset_count
            )));
        }
        if self.cfg.trace != Trace::Off {
            eprintln!(
                "[gsc] warm reset #{} -> entry {:#x}",
                self.reset_count, self.reset_entry
            );
        }
        // After a software reset the firmware must see a non-POR cause, else it
        // re-runs POR init and reboots again.
        self.shared.lock().unwrap().rstsrc = self.cfg.rstsrc_warm;
        self.cpu.reset(self.reset_entry);
        None
    }

    /// Snapshot of console bytes emitted so far (for tests / introspection).
    pub fn console(&self) -> Vec<u8> {
        self.shared.lock().unwrap().console.clone()
    }

    fn sync_machine_external_irq(&mut self) {
        let (meip, compare) = {
            let sh = self.shared.lock().unwrap();
            (
                sh.uart_irq_armed && !sh.uart_rx.is_empty(),
                sh.timer_compare,
            )
        };
        self.cpu.set_interrupt_pending(MIP_MEIP, meip);
        // Machine timer: assert MTIP while the free-running counter (derived
        // from retired instructions, same source as the 0x400C0014/+0x1C reads)
        // has reached the alarm compare. The firmware's cause-7 ISR re-arms or
        // disarms the compare, which clears MTIP on the next sync.
        let counter = self.cpu.instret() / self.cfg.timer_div.max(1);
        self.cpu.set_interrupt_pending(MIP_MTIP, counter >= compare);
    }

    #[inline]
    fn read_guest_u32(&self, addr: u64) -> u32 {
        let mut w = [0u8; 4];
        if self.cpu.read_memory(addr, &mut w).is_ok() {
            u32::from_le_bytes(w)
        } else {
            0
        }
    }

    /// Fill `count` bytes of guest memory at `dest` with PRNG output (SplitMix64
    /// advanced per call). Used by the TPM randomness hooks so generated random
    /// numbers, nonces, and keys are non-zero.
    fn fill_entropy(&mut self, dest: u64, count: u64) {
        let count = count.min(4096) as usize;
        let mut bytes = Vec::with_capacity(count);
        while bytes.len() < count {
            self.entropy_state = self.entropy_state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = self.entropy_state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^= z >> 31;
            bytes.extend_from_slice(&z.to_le_bytes());
        }
        bytes.truncate(count);
        let _ = self.cpu.write_memory(dest, &bytes);
    }

    /// Service a TPM randomness primitive by guest PC: fill its destination
    /// buffer with PRNG bytes, set the return value to the requested length,
    /// and return to the caller (skipping the zero-writing kernel `ecall`).
    /// Returns `true` if `pc` was a randomness function.
    fn maybe_fill_random(&mut self, pc: u64) -> bool {
        let (dest, count) = match pc {
            // CryptRandomGenerate(ctx=a0, dest=a1, count=a2)
            TPM_RAND_GENERATE | TPM_RAND_GENERATE_B => (self.cpu.x(11), self.cpu.x(12)),
            // nonce fill(count=a0, dest=a1)
            TPM_RAND_NONCE | TPM_RAND_NONCE_B => (self.cpu.x(11), self.cpu.x(10)),
            _ => return false,
        };
        if count == 0 || count > 4096 {
            return false;
        }
        self.fill_entropy(dest, count);
        self.cpu.set_x(10, count); // return the byte count (success)
        let ra = self.cpu.x(1);
        self.cpu.set_pc(ra);
        true
    }

    /// Begin a synthetic call into the TPM 2.0 ExecuteCommand wrapper
    /// (`sub_E06BE`) with `cmd`. Stages the command and the two by-reference
    /// argument slots in scratch RAM, snapshots the CPU state, masks
    /// interrupts, and redirects the hart into the wrapper with a sentinel
    /// return address. The normal run loop then executes the call to
    /// completion; [`finish_tpm_call`] captures the response.
    fn begin_tpm_call(&mut self, cmd: &[u8]) {
        let _ = self.cpu.write_memory(TPM_SCRATCH_BUF, cmd);
        let _ = self
            .cpu
            .write_memory(TPM_SCRATCH_CAP, &TPM_SCRATCH_CAPACITY.to_le_bytes());
        let _ = self
            .cpu
            .write_memory(TPM_SCRATCH_BUFPTR, &(TPM_SCRATCH_BUF as u32).to_le_bytes());

        for i in 0..32u8 {
            self.tpm.saved_x[i as usize] = self.cpu.x(i);
        }
        self.tpm.saved_pc = self.cpu.pc();
        self.tpm.saved_mstatus = self.cpu.csr_read(0x300).unwrap_or(0);
        self.tpm.saved_mie = self.cpu.csr_read(0x304).unwrap_or(0);
        self.tpm.cap_slot = TPM_SCRATCH_CAP;
        self.tpm.bufptr_slot = TPM_SCRATCH_BUFPTR;
        self.tpm.call_start_instret = self.cpu.instret();

        // a0=len, a1=buf, a2=&cap, a3=&bufptr; ra=sentinel.
        self.cpu.set_x(10, cmd.len() as u64);
        self.cpu.set_x(11, TPM_SCRATCH_BUF);
        self.cpu.set_x(12, TPM_SCRATCH_CAP);
        self.cpu.set_x(13, TPM_SCRATCH_BUFPTR);
        self.cpu.set_x(1, TPM_CALL_SENTINEL);
        // Mask interrupts so the timer/UART can't divert the synthetic call.
        let _ = self.cpu.csr_write(0x304, 0);
        let _ = self.cpu.csr_write(0x300, self.tpm.saved_mstatus & !0x8);
        self.cpu.set_pc(TPM_EXECUTE_WRAPPER);
        self.tpm.in_call = true;

        let preview: String = cmd.iter().map(|b| format!("{b:02x}")).collect();
        eprintln!(
            "[gsc] TPM inject #{}: sub_E06BE(len={}, cmd={preview})",
            self.tpm.next + 1,
            cmd.len()
        );
    }

    /// Restore the pre-call register/CSR snapshot so the firmware resumes its
    /// idle loop as if the synthetic call never happened.
    fn restore_tpm_state(&mut self) {
        for i in 1..32u8 {
            self.cpu.set_x(i, self.tpm.saved_x[i as usize]);
        }
        let _ = self.cpu.csr_write(0x304, self.tpm.saved_mie);
        let _ = self.cpu.csr_write(0x300, self.tpm.saved_mstatus);
        self.cpu.set_pc(self.tpm.saved_pc);
        self.tpm.in_call = false;
    }

    /// Capture and print the TPM response left by the synthetic call, then
    /// restore state and advance to the next queued command. Returns `true`
    /// when no commands remain (the run loop should halt).
    fn finish_tpm_call(&mut self) -> bool {
        let rc_reg = self.cpu.x(10);
        let cap = self.read_guest_u32(self.tpm.cap_slot);
        let bufptr = self.read_guest_u32(self.tpm.bufptr_slot) as u64;
        // TPM response framing is big-endian: tag[0:2], size[2:6], code[6:10].
        let mut hdr = [0u8; 10];
        let resp_size = if self.cpu.read_memory(bufptr, &mut hdr).is_ok() {
            u32::from_be_bytes([hdr[2], hdr[3], hdr[4], hdr[5]])
        } else {
            0
        };
        let len = resp_size.min(cap.max(resp_size)).min(512) as usize;
        let mut resp = vec![0u8; len];
        let _ = self.cpu.read_memory(bufptr, &mut resp);
        let code = if resp.len() >= 10 {
            u32::from_be_bytes([resp[6], resp[7], resp[8], resp[9]])
        } else {
            0xffff_ffff
        };
        let hexs: String = resp.iter().map(|b| format!("{b:02x}")).collect();
        let status = if code == 0 { "SUCCESS" } else { "rc!=0" };
        eprintln!(
            "[gsc] TPM resp  #{}: ret_a0={rc_reg:#x} size={resp_size} rc={code:#010x} ({status}) bytes={hexs}",
            self.tpm.next + 1
        );

        self.restore_tpm_state();
        self.tpm.next += 1;
        self.tpm.next >= self.tpm_cmds.len()
    }

    /// `yield` mode: plant a command into the TPM task's command-processor
    /// globals at its command-wait yield. After the caller skips the `ecall`,
    /// the firmware task loops back, sees the pending flag, and runs the real
    /// `sub_E06BE` path in its own scheduler context (all upcalls registered).
    fn plant_tpm_command(&mut self, cmd: &[u8]) {
        // Wired transport stages the frame in the real GscFifo dual-port RAM
        // window; otherwise a backed-RAM scratch buffer is used.
        let (buf, cap) = if self.tpm_wired {
            (GSC_FIFO_RAM_BASE, GSC_FIFO_RAM_LEN as u32)
        } else {
            (TPM_SCRATCH_BUF, TPM_SCRATCH_CAPACITY)
        };
        let _ = self.cpu.write_memory(buf, cmd);
        let _ = self
            .cpu
            .write_memory(TPM_GLOBAL_BUF, &(buf as u32).to_le_bytes());
        let _ = self
            .cpu
            .write_memory(TPM_GLOBAL_LEN, &(cmd.len() as u32).to_le_bytes());
        let _ = self.cpu.write_memory(TPM_GLOBAL_CAP, &cap.to_le_bytes());
        let _ = self
            .cpu
            .write_memory(TPM_GLOBAL_PENDING, &1u32.to_le_bytes());
        self.tpm.awaiting = true;
        self.shared.lock().unwrap().tpm_active = true;
        if std::env::var("RAX_GSC_TPM_TRACE").is_ok() {
            self.tpm.trace_budget = 6000;
        }
        let preview: String = cmd.iter().map(|b| format!("{b:02x}")).collect();
        let via = if self.tpm_wired {
            "wired via GscFifo 0x40621000"
        } else {
            "yield"
        };
        eprintln!(
            "[gsc] TPM inject #{} ({via}): {}-byte command {preview}",
            self.tpm.next + 1,
            cmd.len()
        );
    }

    /// `yield` mode: at the response-send entry (`sub_D4B80`) the response is
    /// already marshalled in the `0x220C0` buffer with its length in `a1`.
    /// Capture and print it. A `>= 10`-byte buffer is a real TPM response
    /// (tag/size/rc); a short buffer means the firmware dropped the command
    /// (e.g. the "while AP off" policy). Returns `true` to halt afterward.
    fn capture_tpm_response(&mut self) -> bool {
        let bufptr = self.read_guest_u32(TPM_GLOBAL_BUF) as u64;
        let len = (self.cpu.x(11) as u32).min(1024) as usize;
        let mut resp = vec![0u8; len];
        let _ = self.cpu.read_memory(bufptr, &mut resp);
        let via = if self.tpm_wired { "wired" } else { "yield" };
        if resp.len() >= 10 {
            let code = u32::from_be_bytes([resp[6], resp[7], resp[8], resp[9]]);
            let hexs: String = resp.iter().map(|b| format!("{b:02x}")).collect();
            let status = if code == 0 { "SUCCESS" } else { "rc!=0" };
            eprintln!(
                "[gsc] TPM resp  #{} ({via}): len={len} rc={code:#010x} ({status}) bytes={hexs}",
                self.tpm.next + 1
            );
        } else {
            eprintln!(
                "[gsc] TPM cmd  #{} ({via}): firmware processed the command but returned \
                 no TPM response (len={len}) — see the firmware log above (e.g. it drops \
                 commands \"while AP off\" when PLT_RST_L is asserted / the host is in reset).",
                self.tpm.next + 1
            );
        }
        self.tpm.awaiting = false;
        self.shared.lock().unwrap().tpm_active = false;
        self.tpm.next += 1;
        if self.tpm.next >= self.tpm_cmds.len() {
            return true;
        }
        // More commands queued: short-circuit the host-less send (return
        // success to the caller) so the task loops back to its command-wait
        // yield, where the next command is planted.
        let ra = self.cpu.x(1);
        self.cpu.set_x(10, 0);
        self.cpu.set_pc(ra);
        false
    }

    fn trap_exit(&self, t: crate::isa::riscv::Trap) -> VcpuExit {
        let mepc = self.cpu.csr_read(0x341).unwrap_or(self.cpu.pc());
        let raw = self.cpu.memory().read_u32(mepc).unwrap_or(0);
        VcpuExit::Unknown(format!(
            "gsc riscv trap: cause={} tval={:#x} pc={:#x} mepc={:#x} raw={:#010x} insn=[{}]",
            t.cause,
            t.tval,
            self.cpu.pc(),
            mepc,
            raw,
            self.cpu.disassemble_at(mepc),
        ))
    }
}

impl VCpu for GscVcpu {
    fn run(&mut self) -> Result<VcpuExit> {
        if self.halted {
            return Ok(VcpuExit::Hlt);
        }
        for _ in 0..MAX_ITERS {
            self.sync_machine_external_irq();
            let pc = self.cpu.pc();
            self.pc_cell.store(pc, Ordering::Relaxed);
            self.time_cell.store(self.cpu.instret(), Ordering::Relaxed);
            // Runtime PLT_RST_L deassert event: once the firmware has settled at
            // the idle WFI with the AP held in reset, deassert PLT_RST_L (drive
            // GPIO bank0 bit11 high) and pulse a GPIO wake so the kernel leaves
            // WFI and re-samples the line — modeling the host powering on while
            // the GSC is already running.
            if self.plt_rst_event && !self.plt_rst_fired && pc == self.tpm_trigger {
                let booted = {
                    let sh = self.shared.lock().unwrap();
                    sh.console.windows(11).any(|w| w == b"TPM SPI dis")
                };
                if booted {
                    self.plt_rst_fired = true;
                    {
                        let mut sh = self.shared.lock().unwrap();
                        // Drive PLT_RST_L high (deasserted) for the rest of the run.
                        let v = sh.store.entry(GPIO_INPUT_BANK0).or_insert(0);
                        *v |= GPIO_PLT_RST_L_MASK;
                        // The GSC reboots on an AP power-state change to re-run AP
                        // RO verification; request that warm reset so the firmware
                        // comes back up with the AP powered on (TPM SPI enabled),
                        // mirroring real "Rebooting GSC for AP RO due to state".
                        sh.reset_requested = true;
                    }
                    eprintln!(
                        "[gsc] runtime event: PLT_RST_L deasserted (host powering on); \
                         warm-resetting so the GSC re-boots AP-on"
                    );
                    continue;
                }
            }
            // Entropy: intercept the TPM randomness primitives and fill their
            // output buffer with a PRNG (the kernel crypto service writes zeros
            // in this board model), so RNG/nonce/key output is non-zero.
            if self.entropy_hooks
                && matches!(
                    pc,
                    TPM_RAND_GENERATE | TPM_RAND_GENERATE_B | TPM_RAND_NONCE | TPM_RAND_NONCE_B
                )
                && self.maybe_fill_random(pc)
            {
                continue;
            }
            // TPM command injection. `yield` mode (default) plants each command
            // into the firmware TPM task at its command-wait yield so the task
            // processes it in its own scheduler context; `call` mode drives a
            // synthetic out-of-context call to the ExecuteCommand wrapper.
            if !self.tpm_cmds.is_empty() {
                if self.tpm_yield_mode {
                    if self.tpm.awaiting
                        && self.tpm.trace_budget > 0
                        && std::env::var("RAX_GSC_TPM_TRACE").is_ok()
                    {
                        self.tpm.trace_budget -= 1;
                        eprintln!(
                            "[tpmtrace] {:#010x}: {}  ra={:#x} a0={:#x} a1={:#x} a2={:#x}",
                            pc,
                            self.cpu.disasm_pc(),
                            self.cpu.x(1),
                            self.cpu.x(10),
                            self.cpu.x(11),
                            self.cpu.x(12)
                        );
                    }
                    // Capture the response at the response-send entry, before
                    // the (host-less) send would block.
                    if self.tpm.awaiting && pc == TPM_RESPONSE_SEND {
                        if self.capture_tpm_response() {
                            self.halted = true;
                            return Ok(VcpuExit::Shutdown);
                        }
                        continue;
                    }
                    // Plant the next command at the task's command-wait yield.
                    // When a runtime PLT_RST event is configured, wait until the
                    // AP-power-on reboot has happened (so the AP is on).
                    if !self.tpm.awaiting
                        && self.tpm.next < self.tpm_cmds.len()
                        && pc == TPM_TASK_YIELD_ECALL
                        && (!self.plt_rst_event || self.reset_count >= 1)
                    {
                        let ready = {
                            let sh = self.shared.lock().unwrap();
                            sh.console.windows(8).any(|w| w == b"TPM SPI ")
                        };
                        if ready {
                            let cmd = self.tpm_cmds[self.tpm.next].clone();
                            self.plant_tpm_command(&cmd);
                            self.cpu.set_pc(TPM_TASK_YIELD_NEXT);
                            continue;
                        }
                    }
                } else if self.tpm.in_call {
                    if std::env::var("RAX_GSC_TPM_TRACE").is_ok() {
                        eprintln!(
                            "[tpmtrace] {:#010x}: {}  ra={:#x} sp={:#x} a0={:#x} a1={:#x}",
                            pc,
                            self.cpu.disasm_pc(),
                            self.cpu.x(1),
                            self.cpu.x(2),
                            self.cpu.x(10),
                            self.cpu.x(11)
                        );
                    }
                    if pc == TPM_CALL_SENTINEL {
                        if self.finish_tpm_call() {
                            self.halted = true;
                            return Ok(VcpuExit::Shutdown);
                        }
                        continue;
                    }
                    // Off-the-rails guard: firmware code lives in the flash XIP
                    // window (>= 0x80000). A jump below it (e.g. a null upcall
                    // pointer this out-of-context call never registered) means
                    // the synthetic call derailed deep inside the TPM stack.
                    // The call already mutated global RAM, so the firmware is
                    // not cleanly resumable: report and halt rather than fault.
                    if pc < 0x8_0000 {
                        eprintln!(
                            "[gsc] TPM call #{} ran the TPM stack but derailed at {pc:#x} \
                             (unregistered upcall/client — the ExecuteCommand path is \
                             user-process code invoked out of its scheduler context). \
                             See the firmware log above for how far it got.",
                            self.tpm.next + 1
                        );
                        self.halted = true;
                        return Ok(VcpuExit::Shutdown);
                    }
                    if self
                        .cpu
                        .instret()
                        .saturating_sub(self.tpm.call_start_instret)
                        > TPM_CALL_MAX_INSNS
                    {
                        eprintln!(
                            "[gsc] TPM call #{} watchdog tripped at pc={pc:#x}; aborting injection",
                            self.tpm.next + 1
                        );
                        self.halted = true;
                        return Ok(VcpuExit::Shutdown);
                    }
                } else if self.tpm.next < self.tpm_cmds.len() && pc == self.tpm_trigger {
                    let booted = {
                        let sh = self.shared.lock().unwrap();
                        sh.console.windows(11).any(|w| w == b"TPM SPI dis")
                    };
                    if booted {
                        let cmd = self.tpm_cmds[self.tpm.next].clone();
                        self.begin_tpm_call(&cmd);
                        continue;
                    }
                }
            }
            // Diagnostic: log each GPIO pin sample (`sub_A2F30` @0xa2f38 has the
            // resolved bank base in a0 and the bit index in s0) to map signals
            // like plt_rst_l / ccd_mode_l to their (bank, bit).
            if pc == 0xa2f38 && std::env::var("RAX_GSC_GPIO_TRACE").is_ok() {
                eprintln!(
                    "[gpio] sample bank={:#x} bit={} ra={:#x}",
                    self.cpu.x(10),
                    self.cpu.x(8),
                    self.cpu.x(1)
                );
            }
            if self.ap_ro_stub && pc == 0xce910 {
                let sp = self.cpu.x(2);
                let mut expected = [0u8; 0x40];
                if self.cpu.read_memory(sp + 0x40, &mut expected).is_ok() {
                    let _ = self.cpu.write_memory(sp, &expected);
                }
            }
            if self.ap_ro_crypto_stub && pc == 0xb2904 {
                let sp = self.cpu.x(2);
                let mut ptr = [0u8; 4];
                if self.cpu.read_memory(sp + 0x68, &mut ptr).is_ok() {
                    let expected = u32::from_le_bytes(ptr) as u64;
                    let mut digest = [0u8; 0x20];
                    if self.cpu.read_memory(expected, &mut digest).is_ok() {
                        let _ = self.cpu.write_memory(sp + 0x250, &digest);
                    }
                }
            }
            if self.ap_ro_crypto_stub && pc == 0xb1efe {
                let actual = self.cpu.x(18); // s2
                let actual_len = self.cpu.x(19); // s3
                let expected = self.cpu.x(21); // s5
                let expected_len = self.cpu.x(9); // s1
                if actual_len == expected_len && (1..=64).contains(&actual_len) {
                    let mut digest = vec![0u8; actual_len as usize];
                    if self.cpu.read_memory(expected, &mut digest).is_ok() {
                        let _ = self.cpu.write_memory(actual, &digest);
                    }
                }
            }
            if self.ap_ro_stub && pc == 0xb7462 {
                // Kernel AP-RO verification returns here after reading AP
                // flash/GVD data over the AP SPI host. Without a real AP flash
                // image signed for this production root, the standalone
                // bring-up stub reports verifier success at the call boundary
                // and lets the firmware's normal success path run.
                self.cpu.set_x(10, 0);
            }
            if self.print_trace && matches!(pc, 0xce272 | 0x14e272) {
                let ptr = self.cpu.x(10);
                let len = self.cpu.x(11).min(96) as usize;
                let mut buf = vec![0u8; len];
                let preview = if self.cpu.read_memory(ptr, &mut buf).is_ok() {
                    String::from_utf8_lossy(&buf)
                        .chars()
                        .map(|c| {
                            if c.is_ascii_graphic() || c == ' ' {
                                c
                            } else {
                                '.'
                            }
                        })
                        .collect::<String>()
                } else {
                    "<unreadable>".to_string()
                };
                eprintln!(
                    "[gsc] print pc={pc:#x} ptr={ptr:#x} len={:#x} {preview:?}",
                    self.cpu.x(11)
                );
            }
            let saved_ra_matches = self
                .break_saved_ra
                .map(|expect| {
                    let mut w = [0u8; 4];
                    self.cpu.read_memory(self.cpu.x(2) + 0x11c, &mut w).is_ok()
                        && u32::from_le_bytes(w) as u64 == expect
                })
                .unwrap_or(true);
            let break_matches = self.break_pc == Some(pc)
                && self.break_ra.map(|ra| self.cpu.x(1) == ra).unwrap_or(true)
                && saved_ra_matches;
            if break_matches && !self.broke {
                self.break_hits_seen += 1;
            }
            if break_matches && !self.broke && self.break_hits_seen >= self.break_hit {
                self.broke = true;
                let x = |i: u8| self.cpu.x(i);
                eprintln!(
                    "[gsc] BREAK @{:#x} hit={} ra={:#x} sp={:#x} a0={:#x} a1={:#x} a2={:#x} a3={:#x} a4={:#x} a5={:#x} s0={:#x} s1={:#x} s2={:#x} s3={:#x} s4={:#x} s5={:#x} s6={:#x} s7={:#x} s8={:#x} s9={:#x} s10={:#x} s11={:#x}",
                    pc,
                    self.break_hits_seen,
                    x(1),
                    x(2),
                    x(10),
                    x(11),
                    x(12),
                    x(13),
                    x(14),
                    x(15),
                    x(8),
                    x(9),
                    x(18),
                    x(19),
                    x(20),
                    x(21),
                    x(22),
                    x(23),
                    x(24),
                    x(25),
                    x(26),
                    x(27)
                );
                let csr = |addr| self.cpu.csr_read(addr).unwrap_or(0);
                eprintln!(
                    "[gsc] BREAK csr mstatus={:#x} mie={:#x} mip={:#x} mideleg={:#x} mtvec={:#x} mepc={:#x} mcause={:#x}",
                    csr(0x300),
                    csr(0x304),
                    csr(0x344),
                    csr(0x303),
                    csr(0x305),
                    csr(0x341),
                    csr(0x342)
                );
                // Dump stack words that look like code return-addresses, to
                // reconstruct the call chain that led here.
                let sp = self.cpu.x(2);
                if self.break_stack_bytes != 0 {
                    let dump_len = self.break_stack_bytes.min(0x400);
                    for row in (0..dump_len).step_by(0x10) {
                        let mut words = Vec::new();
                        for off in (0..0x10u64).step_by(4) {
                            if row + off >= dump_len {
                                break;
                            }
                            let mut w = [0u8; 4];
                            if self.cpu.read_memory(sp + row + off, &mut w).is_ok() {
                                words.push(format!("{:#010x}", u32::from_le_bytes(w)));
                            } else {
                                words.push("????????".to_string());
                            }
                        }
                        eprintln!("[gsc]   stack +{row:03x}: {}", words.join(" "));
                    }
                }
                let mut chain = Vec::new();
                for off in (0..0x80u64).step_by(4) {
                    let mut w = [0u8; 4];
                    if self.cpu.read_memory(sp + off, &mut w).is_ok() {
                        let v = u32::from_le_bytes(w) as u64;
                        if (0x8_0000..0x18_0000).contains(&v) {
                            chain.push(format!("{v:#x}"));
                        }
                    }
                }
                eprintln!("[gsc] BREAK callstack: {}", chain.join(" "));
                // Scan registers + stack for pointers to ASCII strings in the
                // image (panic message / source location).
                let mut probes: Vec<u64> = (0..32u8).map(|i| self.cpu.x(i)).collect();
                for off in (0..0x100u64).step_by(4) {
                    let mut w = [0u8; 4];
                    if self.cpu.read_memory(sp + off, &mut w).is_ok() {
                        probes.push(u32::from_le_bytes(w) as u64);
                    }
                }
                let mut seen_str = std::collections::BTreeSet::new();
                for p in probes {
                    if !(0x8_0000..0x18_0000).contains(&p) {
                        continue;
                    }
                    let mut s = [0u8; 48];
                    if self.cpu.read_memory(p, &mut s).is_ok() {
                        let n = s.iter().take_while(|&&b| (0x20..0x7f).contains(&b)).count();
                        if n >= 6 {
                            let txt = String::from_utf8_lossy(&s[..n]).to_string();
                            if seen_str.insert(txt.clone()) {
                                eprintln!("[gsc]   str@{p:#x}: {txt:?}");
                            }
                        }
                    }
                }
                for (name, base) in [("a0", x(10)), ("a2", x(12)), ("s0", x(8)), ("s3", x(19))] {
                    if !(0x1_0000..0x2_0000).contains(&base) {
                        continue;
                    }
                    let mut words = Vec::new();
                    for off in (0..0x40u64).step_by(4) {
                        let mut w = [0u8; 4];
                        if self.cpu.read_memory(base + off, &mut w).is_ok() {
                            words.push(format!("+{off:02x}={:#010x}", u32::from_le_bytes(w)));
                        }
                    }
                    eprintln!("[gsc]   mem {name}@{base:#x}: {}", words.join(" "));
                }
                if self.break_stop {
                    self.halted = true;
                    return Ok(VcpuExit::Unknown(format!("gsc breakpoint stop at {pc:#x}")));
                }
            }
            if self.cfg.trace == Trace::Insn {
                eprintln!("[gsc] {:#010x}: {}", self.cpu.pc(), self.cpu.disasm_pc());
            }
            let exit = self.cpu.step();
            // A firmware-requested warm reset reboots the hart in place; the
            // next iteration then fetches from the entry point. `take_reset`
            // returns `Some` only when the reboot guard trips.
            if let Some(reset_exit) = self.take_reset() {
                return Ok(reset_exit);
            }
            match exit {
                RiscVExit::Continue => {}
                // WFI marks the point where the Tock driver tables are live.
                // Seeded UART RX bytes can now assert the external interrupt.
                RiscVExit::Wfi => {
                    self.shared.lock().unwrap().uart_irq_armed = true;
                }
                RiscVExit::Ecall => {
                    if self.syscall_trace {
                        let x = |i: u8| self.cpu.x(i);
                        eprintln!(
                            "[gsc] {pc:#010x} ecall a0={:#x} a1={:#x} a2={:#x} a3={:#x} a4={:#x} a5={:#x} a6={:#x} a7={:#x}",
                            x(10),
                            x(11),
                            x(12),
                            x(13),
                            x(14),
                            x(15),
                            x(16),
                            x(17),
                        );
                    }
                    if self.ap_ro_stub
                        && self.cpu.pc() == 0xce778
                        && self.cpu.x(10) == 1003
                        && self.cpu.x(11) == 62
                        && self.cpu.x(12) == 7
                        && self.cpu.x(13) == 0
                        && self.cpu.x(14) == 7
                    {
                        // The AP RO app asks the gscvd driver to verify a
                        // cached GVD. In the current standalone emulator there
                        // is no host/AP flash source to populate that object,
                        // so this bring-up knob returns Tock CommandReturn::
                        // success and lets the caller reach the later path.
                        self.cpu.set_x(10, 129);
                        self.cpu.set_x(11, 0);
                        self.cpu.set_x(12, 0);
                        self.cpu.set_x(13, 0);
                        self.cpu.set_pc(self.cpu.pc() + 4);
                        continue;
                    }
                    self.cpu.deliver_ecall_trap();
                }
                RiscVExit::Ebreak => {
                    self.halted = true;
                    return Ok(VcpuExit::Debug);
                }
                RiscVExit::Trap(t) => {
                    self.halted = true;
                    return Ok(self.trap_exit(t));
                }
            }
        }
        Ok(VcpuExit::Hlt)
    }

    fn get_state(&self) -> Result<CpuState> {
        let mut regs = RiscVRegisters::default();
        for i in 0..32u8 {
            regs.x[i as usize] = self.cpu.x(i);
            regs.f[i as usize] = self.cpu.f(i);
        }
        regs.pc = self.cpu.pc();
        regs.fcsr = self.cpu.fcsr();
        Ok(CpuState::riscv(regs))
    }

    fn set_state(&mut self, state: &CpuState) -> Result<()> {
        let state = match state {
            CpuState::RiscV(s) => s,
            _ => {
                return Err(Error::Emulator(
                    "expected riscv state for gsc vCPU".to_string(),
                ));
            }
        };
        for i in 0..32u8 {
            self.cpu.set_x(i, state.regs.x[i as usize]);
            self.cpu.set_f(i, state.regs.f[i as usize]);
        }
        self.cpu.set_pc(state.regs.pc);
        self.cpu.set_fcsr(state.regs.fcsr);
        // The initial PC is the boot entry; warm resets return here.
        self.reset_entry = state.regs.pc;
        self.halted = false;
        Ok(())
    }

    fn complete_io_in(&mut self, _data: &[u8]) {}

    fn id(&self) -> u32 {
        self.id
    }

    fn instruction_count(&self) -> u64 {
        self.cpu.instret()
    }

    fn supports_stepping(&self) -> bool {
        true
    }

    fn current_pc(&self) -> u64 {
        self.cpu.pc()
    }

    fn step_insn(&mut self) -> Result<Option<VcpuExit>> {
        if self.halted {
            return Ok(Some(VcpuExit::Hlt));
        }
        match self.cpu.step() {
            RiscVExit::Continue | RiscVExit::Wfi => Ok(None),
            RiscVExit::Ecall => {
                self.cpu.deliver_ecall_trap();
                Ok(None)
            }
            RiscVExit::Ebreak => {
                self.halted = true;
                Ok(Some(VcpuExit::Debug))
            }
            RiscVExit::Trap(t) => {
                self.halted = true;
                Ok(Some(self.trap_exit(t)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::vcpu::CpuState;

    fn test_cfg() -> GscConfig {
        GscConfig {
            uart_base: DEFAULT_UART_BASE,
            uart_state: DEFAULT_UART_STATE,
            open_bus: 0,
            rstsrc_cold: DEFAULT_RSTSRC_COLD,
            rstsrc_warm: DEFAULT_RSTSRC_WARM,
            flash_img_base: DEFAULT_FLASH_IMG_BASE,
            trace: Trace::Off,
            console_trace: false,
            ap_ro_info_stub: false,
            ap_ro_crypto_stub: false,
            ap_ro_stub: false,
            timer_div: TIMER_DEFAULT_DIV,
        }
    }

    fn test_bridge() -> GscBridge {
        let mem = GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x20_0000usize)]).unwrap();
        GscBridge {
            mem: Arc::new(mem),
            shared: Arc::new(Mutex::new(GscShared {
                rstsrc: DEFAULT_RSTSRC_COLD,
                timer_compare: u64::MAX,
                ..GscShared::default()
            })),
            ap_flash: Arc::new(Vec::new()),
            cfg: test_cfg(),
            ready: builtin_ready_map(),
            pc: Arc::new(AtomicU64::new(0)),
            time: Arc::new(AtomicU64::new(0)),
            crypto_trace: false,
        }
    }

    fn rd32(b: &GscBridge, addr: u64) -> u32 {
        let mut buf = [0u8; 4];
        b.read(addr, &mut buf).unwrap();
        u32::from_le_bytes(buf)
    }

    fn wr32(b: &mut GscBridge, addr: u64, value: u32) {
        b.write(addr, &value.to_le_bytes()).unwrap();
    }

    fn rd8(b: &GscBridge, addr: u64) -> u8 {
        let mut buf = [0u8; 1];
        b.read(addr, &mut buf).unwrap();
        buf[0]
    }

    fn wr8(b: &mut GscBridge, addr: u64, value: u8) {
        b.write(addr, &[value]).unwrap();
    }

    fn mem_rd32(b: &GscBridge, addr: u64) -> u32 {
        let mut buf = [0u8; 4];
        b.mem.read_slice(&mut buf, GuestAddress(addr)).unwrap();
        u32::from_le_bytes(buf)
    }

    fn mem_wr32(b: &GscBridge, addr: u64, value: u32) {
        b.mem
            .write_slice(&value.to_le_bytes(), GuestAddress(addr))
            .unwrap();
    }

    #[test]
    fn ready_map_returns_fixed_status() {
        let mut b = test_bridge();
        b.ready.insert(0x4000_1234, 0x5a5a_0001);
        assert_eq!(rd32(&b, 0x4000_1234), 0x5a5a_0001);
    }

    #[test]
    fn timer_counter_advances_with_published_time() {
        let b = test_bridge();
        // Frozen at zero before any time is published.
        assert_eq!(rd32(&b, TIMER_COUNT_LO), 0);
        assert_eq!(rd32(&b, TIMER_COUNT_HI), 0);
        // The free-running counter reflects the published instruction count
        // divided by the configured tick divisor.
        let instret = (TIMER_DEFAULT_DIV as u64) * 0x1_2345_6789;
        b.time.store(instret, Ordering::Relaxed);
        let ticks = instret / TIMER_DEFAULT_DIV as u64;
        assert_eq!(rd32(&b, TIMER_COUNT_LO), ticks as u32);
        assert_eq!(rd32(&b, TIMER_COUNT_HI), (ticks >> 32) as u32);
        // Busy/ready status always reads "ready" (0).
        assert_eq!(rd32(&b, TIMER_BUSY), 0);
    }

    #[test]
    fn second_timer_counter_advances() {
        let b = test_bridge();
        assert_eq!(rd32(&b, TIMER2_COUNT_LO), 0);
        let instret = (TIMER_DEFAULT_DIV as u64) * 0xfeed_face;
        b.time.store(instret, Ordering::Relaxed);
        let ticks = instret / TIMER_DEFAULT_DIV as u64;
        assert_eq!(rd32(&b, TIMER2_COUNT_LO), ticks as u32);
        assert_eq!(rd32(&b, TIMER2_COUNT_HI), (ticks >> 32) as u32);
        assert_eq!(rd32(&b, TIMER2_BUSY), 0);
    }

    #[test]
    fn drbg_reports_done_and_nonzero_output() {
        let b = test_bridge();
        // sub_80434 spins until status bits[2:0] != 0 and treats 1 as success.
        assert_eq!(rd32(&b, DRBG_STATUS) & 7, 1);
        // The 256-bit output files read back non-zero, varied entropy.
        let w0 = rd32(&b, DRBG_OUT_HI);
        let w1 = rd32(&b, DRBG_OUT_HI + 4);
        assert_ne!(w0, 0);
        assert_ne!(w1, 0);
        assert_ne!(w0, w1);
    }

    #[test]
    fn fifo_ram_window_round_trips() {
        let mut b = test_bridge();
        // The GscFifo dual-port RAM stages the wired TPM frame.
        wr32(&mut b, GSC_FIFO_RAM_BASE, 0x0c00_0180);
        wr32(&mut b, GSC_FIFO_RAM_BASE + 4, 0x4401_0000);
        assert_eq!(rd32(&b, GSC_FIFO_RAM_BASE), 0x0c00_0180);
        assert_eq!(rd32(&b, GSC_FIFO_RAM_BASE + 4), 0x4401_0000);
        // Byte-accurate access.
        wr8(&mut b, GSC_FIFO_RAM_BASE + 2, 0xab);
        assert_eq!(rd8(&b, GSC_FIFO_RAM_BASE + 2), 0xab);
    }

    #[test]
    fn timer_compare_write_tracks_64bit_deadline() {
        let mut b = test_bridge();
        // Init disarms the compare (all-ones); the alarm driver then writes a
        // 64-bit deadline through the low/high halves.
        assert_eq!(b.shared.lock().unwrap().timer_compare, u64::MAX);
        wr32(&mut b, TIMER_COMPARE_LO, 0xdead_beef);
        wr32(&mut b, TIMER_COMPARE_HI, 0x0000_1234);
        assert_eq!(
            b.shared.lock().unwrap().timer_compare,
            0x0000_1234_dead_beef
        );
    }

    #[test]
    fn parse_tpm_cmds_splits_hex_commands() {
        let cmds = {
            // Mirror parse_tpm_cmds_env's parsing on an explicit string.
            "80010000000c000001440000, 80 01 00 00"
                .split([',', ';'])
                .filter_map(|part| {
                    let hex: String = part.chars().filter(|c| c.is_ascii_hexdigit()).collect();
                    if hex.len() < 2 {
                        return None;
                    }
                    let bytes: Vec<u8> = (0..hex.len() - 1)
                        .step_by(2)
                        .filter_map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
                        .collect();
                    (!bytes.is_empty()).then_some(bytes)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(cmds.len(), 2);
        assert_eq!(
            cmds[0],
            vec![
                0x80, 0x01, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x00, 0x01, 0x44, 0x00, 0x00
            ]
        );
        assert_eq!(cmds[1], vec![0x80, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn uart_state_tracks_rx_queue() {
        let b = test_bridge();
        let state = rd32(&b, DEFAULT_UART_BASE + UART_STATE);
        assert_eq!(state & UART_STATE_TX_BUSY, 0);
        assert_eq!(state & UART_STATE_RX_EMPTY, UART_STATE_RX_EMPTY);

        b.shared.lock().unwrap().uart_rx.extend(b"AB");
        let state = rd32(&b, DEFAULT_UART_BASE + UART_STATE);
        assert_eq!(state & UART_STATE_RX_EMPTY, UART_STATE_RX_EMPTY);

        b.shared.lock().unwrap().uart_irq_armed = true;
        let state = rd32(&b, DEFAULT_UART_BASE + UART_STATE);
        assert_eq!(state & UART_STATE_RX_EMPTY, 0);
        assert_eq!(rd32(&b, DEFAULT_UART_BASE + UART_RDATA), b'A' as u32);
        assert_eq!(rd32(&b, DEFAULT_UART_BASE + UART_RDATA), b'B' as u32);
        let state = rd32(&b, DEFAULT_UART_BASE + UART_STATE);
        assert_eq!(state & UART_STATE_RX_EMPTY, UART_STATE_RX_EMPTY);
    }

    #[test]
    fn core_irq_claim_tracks_armed_uart_rx() {
        let b = test_bridge();
        assert_eq!(rd32(&b, CORE_IRQ_CLAIM), CORE_IRQ_NONE);

        b.shared.lock().unwrap().uart_rx.extend(b"A");
        assert_eq!(rd32(&b, CORE_IRQ_CLAIM), CORE_IRQ_NONE);

        b.shared.lock().unwrap().uart_irq_armed = true;
        assert_eq!(rd32(&b, CORE_IRQ_CLAIM), CORE_IRQ_UART0);
        assert_eq!(rd32(&b, CORE_IRQ_EPOCH), 0);

        assert_eq!(rd32(&b, DEFAULT_UART_BASE + UART_RDATA), b'A' as u32);
        assert_eq!(rd32(&b, CORE_IRQ_CLAIM), CORE_IRQ_NONE);
    }

    #[test]
    fn rstsrc_reads_por_then_clears_via_clrrst() {
        let mut b = test_bridge();
        // Cold boot: RSTSRC (PMU+0x00) reads back POR, so the firmware
        // classifies the reset cause instead of "Other" + reboot.
        assert_eq!(rd32(&b, PMU_RSTSRC_REG), RSTSRC_POR);
        // CLRRST (PMU+0x04, write-1-to-clear) clears the reported bits.
        wr32(&mut b, PMU_CLRRST_REG, RSTSRC_POR);
        assert_eq!(rd32(&b, PMU_RSTSRC_REG), 0);
    }

    #[test]
    fn global_reset_preserves_persistent_pmu_scratch() {
        // The persistent PMU scratch (boot counter / init flags) must survive a
        // warm reset, so the firmware sees "already initialized" on reboot.
        let mut b = test_bridge();
        // Boot-attempt counter.
        wr32(&mut b, 0x4000_00b0, 0x0000_0300);
        // RO init flags (bits 30/31).
        wr32(&mut b, 0x4000_004c, 0xc000_0000);
        // A warm reset clears CPU/arch state but NOT the bridge's store.
        assert_eq!(rd32(&b, 0x4000_00b0), 0x0000_0300);
        assert_eq!(rd32(&b, 0x4000_004c), 0xc000_0000);
    }

    #[test]
    fn mmio_registers_remember_writes() {
        let mut b = test_bridge();
        // An unmodeled register reads back the last value written (scratch).
        assert_eq!(rd32(&b, 0x4000_1000), 0); // open-bus before any write
        wr32(&mut b, 0x4000_1000, 0xabcd_1234);
        assert_eq!(rd32(&b, 0x4000_1000), 0xabcd_1234);
    }

    #[test]
    fn spin_breaker_alternates_polarity() {
        let mut b = test_bridge();
        let addr = 0x4000_2000;
        // Below threshold: open-bus 0.
        for _ in 0..(SPIN_THRESHOLD - 1) {
            assert_eq!(rd32(&b, addr), 0);
        }
        // The threshold-th consecutive read breaks the spin with all-ones.
        assert_eq!(rd32(&b, addr), 0xffff_ffff);
        // The next break alternates to zero (satisfies "wait for clear").
        for _ in 0..(SPIN_THRESHOLD - 1) {
            let _ = rd32(&b, addr);
        }
        assert_eq!(rd32(&b, addr), 0x0000_0000);
        // A write resets the spin window (forward progress).
        wr32(&mut b, addr, 5);
        assert_eq!(rd32(&b, addr), 5);
    }

    #[test]
    fn pmu_reset_magic_is_detected() {
        let mut b = test_bridge();
        wr32(&mut b, PMU_RESET_REG, 0x1234); // wrong value: no reset
        assert!(!b.shared.lock().unwrap().reset_requested);
        wr32(&mut b, PMU_RESET_REG, PMU_RESET_MAGIC);
        assert!(b.shared.lock().unwrap().reset_requested);
    }

    #[test]
    fn pmu_chip_id_and_wake_source_are_fixed() {
        let b = test_bridge();
        assert_eq!(rd32(&b, PMU_CHIP_ID_REG), PMU_CHIP_ID);
        assert_eq!(rd32(&b, PMU_CHIP_ID_ALT_REG), PMU_CHIP_ID);
        assert_eq!(rd32(&b, PMU_WAKE_EXIT_SRC_REG), 0);
    }

    #[test]
    fn flash_program_op_uses_payload_registers_and_clears_control() {
        let mut b = test_bridge();
        let off = 0x3e400;
        let gpa = DEFAULT_FLASH_IMG_BASE + off;
        let trans = ((off / 4) as u32) << 7;

        mem_wr32(&b, gpa, 0xffff_00ff);
        mem_wr32(&b, gpa + 4, 0xffff_ffff);
        wr32(&mut b, FLASH_TRANS, trans);
        wr32(&mut b, FLASH_WR_DATA0, 0x1234_5678);
        wr32(&mut b, FLASH_WR_DATA1, 0x0000_ffff);
        wr32(&mut b, FLASH_PE_EN, FLASH_PE_EN_MAGIC);
        wr32(&mut b, FLASH_PE_CONTROL0, FLASH_OP_TI50_PROGRAM);

        assert_eq!(rd32(&b, FLASH_PE_CONTROL0), 0);
        assert_eq!(rd32(&b, FLASH_ERROR), 0);
        assert_eq!(mem_rd32(&b, gpa), 0x1234_0078);
        assert_eq!(mem_rd32(&b, gpa + 4), 0x0000_ffff);
    }

    #[test]
    fn flash_control1_programs_second_slot() {
        let mut b = test_bridge();
        let off = 0x3b800;
        let slot0_gpa = DEFAULT_FLASH_IMG_BASE + off;
        let slot1_gpa = DEFAULT_FLASH_IMG_BASE + TI50_FLASH_SLOT_SIZE + off;
        let trans = ((off / 4) as u32) << 7;

        mem_wr32(&b, slot0_gpa, 0x4622_4592);
        mem_wr32(&b, slot1_gpa, 0xffff_ffff);
        wr32(&mut b, FLASH_TRANS, trans);
        wr32(&mut b, FLASH_WR_DATA0, 0);
        wr32(&mut b, FLASH_WR_DATA1, 0);
        wr32(&mut b, FLASH_PE_EN, FLASH_PE_EN_MAGIC);
        wr32(&mut b, FLASH_PE_CONTROL1, FLASH_OP_TI50_PROGRAM);

        assert_eq!(mem_rd32(&b, slot0_gpa), 0x4622_4592);
        assert_eq!(mem_rd32(&b, slot1_gpa), 0);
    }

    #[test]
    fn flash_read_and_erase_family_ops_work() {
        let mut b = test_bridge();
        let off = 0x1200;
        let gpa = DEFAULT_FLASH_IMG_BASE + off;
        let read_trans = (off / 4) as u32;
        let trans = read_trans << 7;

        mem_wr32(&b, gpa, 0xa5a5_5a5a);
        mem_wr32(&b, gpa + 4, 0x1122_3344);
        wr32(&mut b, FLASH_READ_TRANS, read_trans);
        wr32(&mut b, FLASH_PE_EN, FLASH_PE_EN_MAGIC);
        wr32(&mut b, FLASH_PE_CONTROL0, FLASH_OP_CR50_READ);
        assert_eq!(rd32(&b, FLASH_DOUT0_LO), 0xa5a5_5a5a);
        assert_eq!(rd32(&b, FLASH_DOUT0_HI), 0x1122_3344);

        let slot1_gpa = DEFAULT_FLASH_IMG_BASE + TI50_FLASH_SLOT_SIZE + off;
        mem_wr32(&b, slot1_gpa, 0x5566_7788);
        mem_wr32(&b, slot1_gpa + 4, 0x99aa_bbcc);
        wr32(&mut b, FLASH_READ_TRANS, read_trans);
        wr32(&mut b, FLASH_PE_EN, FLASH_PE_EN_MAGIC);
        wr32(&mut b, FLASH_PE_CONTROL1, FLASH_OP_CR50_READ);
        assert_eq!(rd32(&b, FLASH_DOUT1_LO), 0x5566_7788);
        assert_eq!(rd32(&b, FLASH_DOUT1_HI), 0x99aa_bbcc);
        assert_eq!(rd32(&b, FLASH_STATUS1), 0);

        wr32(&mut b, FLASH_TRANS, trans);
        wr32(&mut b, FLASH_PE_EN, FLASH_PE_EN_MAGIC);
        wr32(&mut b, FLASH_PE_CONTROL0, FLASH_OP_CR50_ERASE);
        assert_eq!(mem_rd32(&b, gpa), 0xffff_ffff);
        assert_eq!(mem_rd32(&b, gpa + 4), 0xffff_ffff);
    }

    #[test]
    fn flash_info_bank_is_separate_from_xip_flash() {
        let mut b = test_bridge();
        let off = 0x500;
        let gpa = DEFAULT_FLASH_IMG_BASE + off;
        let read_trans = 0x1_0000 | (off / 4) as u32;
        let prog_trans = (((off / 4) as u32) << 7) | 0x8;

        mem_wr32(&b, gpa, 0x1234_5678);
        wr32(&mut b, FLASH_READ_TRANS, read_trans);
        wr32(&mut b, FLASH_PE_EN, FLASH_PE_EN_MAGIC);
        wr32(&mut b, FLASH_PE_CONTROL0, FLASH_OP_CR50_READ);
        assert_eq!(rd32(&b, FLASH_DOUT0_LO), 0xffff_ffff);

        wr32(&mut b, FLASH_TRANS, prog_trans);
        wr32(&mut b, FLASH_WR_DATA0, 0x00ff_00ff);
        wr32(&mut b, FLASH_WR_DATA1, 0xff00_ff00);
        wr32(&mut b, FLASH_PE_EN, FLASH_PE_EN_MAGIC);
        wr32(&mut b, FLASH_PE_CONTROL0, FLASH_OP_CR50_PROGRAM);

        wr32(&mut b, FLASH_READ_TRANS, read_trans);
        wr32(&mut b, FLASH_PE_EN, FLASH_PE_EN_MAGIC);
        wr32(&mut b, FLASH_PE_CONTROL0, FLASH_OP_CR50_READ);
        assert_eq!(rd32(&b, FLASH_DOUT0_LO), 0x00ff_00ff);
        assert_eq!(rd32(&b, FLASH_DOUT0_HI), 0xff00_ff00);
        assert_eq!(mem_rd32(&b, gpa), 0x1234_5678);
    }

    #[test]
    fn ap_ro_stub_supplies_cached_status_info_words() {
        let mut b = test_bridge();
        b.cfg.ap_ro_stub = true;

        let read_info = |b: &mut GscBridge, off: u64| -> [u32; 2] {
            wr32(b, FLASH_READ_TRANS, 0x1_0000 | (off / 4) as u32);
            wr32(b, FLASH_PE_EN, FLASH_PE_EN_MAGIC);
            wr32(b, FLASH_PE_CONTROL1, FLASH_OP_CR50_READ);
            [rd32(b, FLASH_DOUT1_LO), rd32(b, FLASH_DOUT1_HI)]
        };

        assert_eq!(read_info(&mut b, 0x0c00), [0x0000_0001, 0x0000_0001]);
        assert_eq!(read_info(&mut b, 0x0c08), [0x00ff_ff00, 0x00ff_fd02]);
        assert_eq!(read_info(&mut b, 0x0c10), [0x00ff_ff00, 0x00ff_ff00]);
        assert_eq!(read_info(&mut b, 0x0c18), [0x00ff_ff00, 0x0000_0000]);
    }

    #[test]
    fn ap_ro_info_stub_only_supplies_cached_status_info_words() {
        let mut b = test_bridge();
        b.cfg.ap_ro_info_stub = true;

        wr32(&mut b, FLASH_READ_TRANS, 0x1_0000 | (0x0c08 / 4));
        wr32(&mut b, FLASH_PE_EN, FLASH_PE_EN_MAGIC);
        wr32(&mut b, FLASH_PE_CONTROL1, FLASH_OP_CR50_READ);

        assert_eq!(rd32(&b, FLASH_DOUT1_LO), 0x00ff_ff00);
        assert_eq!(rd32(&b, FLASH_DOUT1_HI), 0x00ff_fd02);
    }

    #[test]
    fn ap_ro_stub_forces_kernel_verifier_return_to_ok() {
        let mem =
            Arc::new(GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x20_0000usize)]).unwrap());
        let mut vcpu = GscVcpu::new(0, mem.clone());
        vcpu.ap_ro_stub = true;
        vcpu.cpu.set_x(10, 0xdead_beef);
        vcpu.cpu.set_pc(0xb7462);
        mem.write_slice(&0x0010_0073u32.to_le_bytes(), GuestAddress(0xb7462))
            .unwrap();

        assert!(matches!(vcpu.run().unwrap(), VcpuExit::Debug));
        assert_eq!(vcpu.cpu.x(10), 0);
    }

    #[test]
    fn ap_ro_crypto_stub_supplies_root_key_digest_compare_result() {
        let mem =
            Arc::new(GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x20_0000usize)]).unwrap());
        let mut vcpu = GscVcpu::new(0, mem.clone());
        vcpu.ap_ro_crypto_stub = true;
        vcpu.cpu.set_x(2, 0x12_000);
        vcpu.cpu.set_pc(0xb2904);

        let expected_addr = 0x18_000u32;
        let expected = [0x5au8; 0x20];
        mem.write_slice(&expected_addr.to_le_bytes(), GuestAddress(0x12_000 + 0x68))
            .unwrap();
        mem.write_slice(&expected, GuestAddress(expected_addr as u64))
            .unwrap();
        mem.write_slice(&0x0010_0073u32.to_le_bytes(), GuestAddress(0xb2904))
            .unwrap();

        assert!(matches!(vcpu.run().unwrap(), VcpuExit::Debug));
        let mut actual = [0u8; 0x20];
        mem.read_slice(&mut actual, GuestAddress(0x12_000 + 0x250))
            .unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn ap_ro_crypto_stub_supplies_range_digest_compare_result() {
        let mem =
            Arc::new(GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x20_0000usize)]).unwrap());
        let mut vcpu = GscVcpu::new(0, mem.clone());
        vcpu.ap_ro_crypto_stub = true;
        vcpu.cpu.set_pc(0xb1efe);
        vcpu.cpu.set_x(18, 0x12_000);
        vcpu.cpu.set_x(19, 0x20);
        vcpu.cpu.set_x(21, 0x18_000);
        vcpu.cpu.set_x(9, 0x20);

        let expected = [0xacu8; 0x20];
        mem.write_slice(&expected, GuestAddress(0x18_000)).unwrap();
        mem.write_slice(&[0x55u8; 0x20], GuestAddress(0x12_000))
            .unwrap();
        mem.write_slice(&0x0010_0073u32.to_le_bytes(), GuestAddress(0xb1efe))
            .unwrap();

        assert!(matches!(vcpu.run().unwrap(), VcpuExit::Debug));
        let mut actual = [0u8; 0x20];
        mem.read_slice(&mut actual, GuestAddress(0x12_000)).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn usb_status_and_reset_registers_self_clear() {
        let mut b = test_bridge();
        wr32(&mut b, USB_INT_STATE2, 0xffff_ffff);
        assert_eq!(rd32(&b, USB_INT_STATE2), 0);
        wr32(&mut b, USB_SOFT_RESET, 1);
        assert_eq!(rd32(&b, USB_SOFT_RESET), 0);
        wr32(&mut b, GSC_EVENT_USB_RESET, 1);
        assert_eq!(rd32(&b, GSC_EVENT_USB_RESET), 0x8000_0000);
    }

    #[test]
    fn rbox_interrupt_state_is_write_one_to_clear() {
        let mut b = test_bridge();
        b.shared
            .lock()
            .unwrap()
            .store
            .insert(RBOX_INTR0_STATE, 0x0000_00ff);

        wr32(&mut b, RBOX_INTR0_STATE, 0x0000_000f);
        assert_eq!(rd32(&b, RBOX_INTR0_STATE), 0x0000_00f0);

        wr32(&mut b, RBOX_INTR1_STATE, 0xffff_ffff);
        assert_eq!(rd32(&b, RBOX_INTR1_STATE), 0);
    }

    #[test]
    fn rbox_ready_and_control_status_complete_immediately() {
        let mut b = test_bridge();
        assert_eq!(rd32(&b, RBOX_INIT_READY), 1);
        assert_eq!(rd32(&b, RBOX_CMD_STATUS), 0);

        wr32(&mut b, RBOX_CONTROL0, 0);
        assert_eq!(rd32(&b, RBOX_STATUS) & 1, 0);
        wr32(&mut b, RBOX_CONTROL0, 1);
        assert_eq!(rd32(&b, RBOX_STATUS) & 1, 1);

        wr32(&mut b, RBOX_CONTROL1, 1);
        assert_eq!(rd32(&b, RBOX_STATUS) & 0x20, 0);
        wr32(&mut b, RBOX_CONTROL1, 0);
        assert_eq!(rd32(&b, RBOX_STATUS) & 0x20, 0x20);
    }

    #[test]
    fn ap_spi_status_registers_are_write_one_to_clear() {
        let mut b = test_bridge();
        b.shared
            .lock()
            .unwrap()
            .store
            .insert(AP_SPI_INTR2, 0x0000_00ff);

        wr32(&mut b, AP_SPI_INTR2, 0x0000_000f);
        assert_eq!(rd32(&b, AP_SPI_INTR2), 0x0000_00f0);

        wr32(&mut b, AP_SPI_INTR2, 0xffff_ffff);
        assert_eq!(rd32(&b, AP_SPI_INTR2), 0);
    }

    #[test]
    fn ap_spi_transaction_clears_busy_and_returns_status_byte() {
        let mut b = test_bridge();

        wr8(&mut b, AP_SPI_DATA_BASE, 0x05);
        wr32(&mut b, AP_SPI_XFER_CFG, 0x0000_0380);
        wr32(&mut b, AP_SPI_XACT, 0x000f_0001);

        assert_eq!(rd32(&b, AP_SPI_XACT) & AP_SPI_XACT_START, 0);
        assert_eq!(rd8(&b, AP_SPI_DATA_BASE + 5), 0x02);

        wr8(&mut b, AP_SPI_DATA_BASE, 0x35);
        wr32(&mut b, AP_SPI_XACT, 0x000f_0001);
        assert_eq!(rd32(&b, AP_SPI_XACT) & AP_SPI_XACT_START, 0);
        assert_eq!(rd8(&b, AP_SPI_DATA_BASE + 5), 0);
    }

    #[test]
    fn ap_spi_fast_read_returns_erased_flash_without_image() {
        let mut b = test_bridge();

        wr32(&mut b, AP_SPI_DATA_BASE, 0x3412_000b); // 0x0b fast-read @ 0x001234
        wr32(&mut b, AP_SPI_DATA_BASE + 4, 0);
        wr32(&mut b, AP_SPI_XFER_CFG, 0x0018_1380);
        wr32(&mut b, AP_SPI_XACT, 0x0067_0001);

        assert_eq!(rd32(&b, AP_SPI_XACT) & AP_SPI_XACT_START, 0);
        assert_eq!(rd8(&b, AP_SPI_DATA_BASE + 0x0d), 0xff);
        assert_eq!(rd8(&b, AP_SPI_DATA_BASE + 0x11), 0xff);
        assert_eq!(rd8(&b, AP_SPI_DATA_BASE + 0x17), 0xff);
    }

    #[test]
    fn ap_spi_fast_read_uses_backing_ap_flash_image() {
        let mut b = test_bridge();
        let mut ap_flash = vec![0xff; 0x1244];
        for i in 0..16 {
            ap_flash[0x1234 + i] = 0xa0 + i as u8;
        }
        b.ap_flash = Arc::new(ap_flash);

        wr32(&mut b, AP_SPI_DATA_BASE, 0x3412_000b); // 0x0b fast-read @ 0x001234
        wr32(&mut b, AP_SPI_DATA_BASE + 4, 0);
        wr32(&mut b, AP_SPI_XFER_CFG, 0x0018_1380);
        wr32(&mut b, AP_SPI_XACT, 0x0067_0001);

        assert_eq!(rd8(&b, AP_SPI_DATA_BASE + 0x0d), 0xa0);
        assert_eq!(rd8(&b, AP_SPI_DATA_BASE + 0x11), 0xa4);
        assert_eq!(rd8(&b, AP_SPI_DATA_BASE + 0x17), 0xaa);
    }

    #[test]
    fn gsc_fifo_reset_busy_bits_clear_on_read() {
        let mut b = test_bridge();
        wr32(&mut b, GSC_FIFO_CONTROL, 0x1b);
        assert_eq!(rd32(&b, GSC_FIFO_CONTROL), 0x12);

        wr32(&mut b, GSC_FIFO_CONTROL, 0x12);
        assert_eq!(rd32(&b, GSC_FIFO_CONTROL), 0x12);
        assert_eq!(rd32(&b, GSC_FIFO_IRQ_STATUS), 0);
    }

    #[test]
    fn gsc_fifo_interrupt_state_is_write_one_to_clear() {
        let mut b = test_bridge();
        b.shared
            .lock()
            .unwrap()
            .store
            .insert(GSC_FIFO_IRQ_STATE, 0x0000_ffff);

        wr32(&mut b, GSC_FIFO_IRQ_STATE, 0x0000_00ff);
        assert_eq!(rd32(&b, GSC_FIFO_IRQ_STATE), 0x0000_ff00);

        wr32(&mut b, GSC_FIFO_IRQ_ENABLE, 0xffff_ffff);
        assert_eq!(rd32(&b, GSC_FIFO_IRQ_ENABLE), 0);
    }

    #[test]
    fn gpio_input_banks_are_stable_status_words() {
        let mut b = test_bridge();
        assert_eq!(rd32(&b, GPIO_INPUT_BANK0), 0);
        assert_eq!(rd32(&b, GPIO_INPUT_BANK1), 0);
        assert_eq!(rd32(&b, GPIO_INPUT_BANK2), 0);
        assert_eq!(rd32(&b, GPIO_INPUT_BANK3), 0);

        // Board strap experiments can override levels through the persistent
        // store/ready path without falling back to the generic spin-breaker.
        wr32(&mut b, GPIO_INPUT_BANK1, 0x0000_1000);
        for _ in 0..SPIN_THRESHOLD {
            assert_eq!(rd32(&b, GPIO_INPUT_BANK1), 0x0000_1000);
        }
    }

    #[test]
    fn gpio_interrupt_state_is_write_one_to_clear() {
        let mut b = test_bridge();
        b.shared
            .lock()
            .unwrap()
            .store
            .insert(GPIO_INTR_STATE0, 0x0000_00ff);
        wr32(&mut b, GPIO_INTR_STATE0, 0x0000_000f);
        assert_eq!(rd32(&b, GPIO_INTR_STATE0), 0x0000_00f0);

        wr32(&mut b, GPIO_INTR_STATE0, 0xffff_ffff);
        assert_eq!(rd32(&b, GPIO_INTR_STATE0), 0);
    }

    #[test]
    fn fuse_defaults_and_trng_are_available() {
        let b = test_bridge();
        assert_eq!(rd32(&b, FUSE_BASE + 0x280), FUSE_DEFAULT);
        let first = rd32(&b, TRNG_READ_DATA);
        let second = rd32(&b, TRNG_READ_DATA);
        assert_ne!(first, second);
    }

    #[test]
    fn globalsec_active_image_regions_are_available() {
        let b = test_bridge();
        assert_eq!(
            rd32(&b, GLOBALSEC_CRYPTOLIB_BASE),
            CRYPTOLIB_ROM_BASE as u32
        );
        assert_eq!(
            rd32(&b, GLOBALSEC_ACTIVE_RO_BASE),
            DEFAULT_FLASH_IMG_BASE as u32
        );
        assert_eq!(rd32(&b, GLOBALSEC_ACTIVE_RO_SIZE), TI50_RO_IMAGE_SIZE);
        assert_eq!(
            rd32(&b, GLOBALSEC_ACTIVE_RW_BASE),
            (DEFAULT_FLASH_IMG_BASE + TI50_RW_SLOT_OFFSET) as u32
        );
        assert_eq!(rd32(&b, GLOBALSEC_ACTIVE_RW_SIZE), TI50_RW_IMAGE_SIZE);
    }

    #[test]
    fn synthetic_cryptolib_rom_is_installed() {
        let mem =
            Arc::new(GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x20_0000usize)]).unwrap());
        let _vcpu = GscVcpu::new(0, mem.clone());
        let read_word = |addr| {
            let mut bytes = [0u8; 4];
            mem.read_slice(&mut bytes, GuestAddress(addr)).unwrap();
            u32::from_le_bytes(bytes)
        };

        assert_eq!(read_word(CRYPTOLIB_HEADER), 1);
        assert_eq!(read_word(CRYPTOLIB_HEADER + 8), CRYPTOLIB_MAGIC);
        assert_eq!(read_word(CRYPTOLIB_HEADER + 0x0c), CRYPTOLIB_ENTRY as u32);
        assert_eq!(read_word(CRYPTOLIB_ENTRY), 0xaa55_b537);
    }

    /// Boot a synthetic RV32 program on the GSC machine and return the vCPU
    /// (for `console()`) and the run exit.
    fn boot(base: u64, program: &[u32]) -> (GscVcpu, VcpuExit) {
        let mem = GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x20_0000usize)]).unwrap();
        for (i, w) in program.iter().enumerate() {
            mem.write_slice(&w.to_le_bytes(), GuestAddress(base + 4 * i as u64))
                .unwrap();
        }
        let mut vcpu = GscVcpu::new(0, Arc::new(mem));
        let mut regs = RiscVRegisters::default();
        regs.pc = base;
        vcpu.set_state(&CpuState::riscv(regs)).unwrap();
        let exit = vcpu.run().unwrap();
        (vcpu, exit)
    }

    #[test]
    fn end_to_end_rv32_xsoteria_uart_console() {
        // Program: print "OK", then use the Xsoteria `pcnt` op to compute
        // popcount(7)=3, turn it into '3', and print it. Ends with `ebreak`.
        //   lui   a0, 0x404d0       ; console UART base
        //   addi  a1, x0, 'O'       ; 0x4f
        //   sw    a1, 4(a0)         ; UART WDATA
        //   addi  a1, x0, 'K'       ; 0x4b
        //   sw    a1, 4(a0)
        //   addi  a2, x0, 7
        //   pcnt  a3, a2            ; Xsoteria: a3 = popcount(7) = 3
        //   addi  a3, a3, 0x30      ; '0' + 3 = '3'
        //   sw    a3, 4(a0)
        //   ebreak
        let program = [
            0x404d_0537u32, // lui a0, 0x404d0
            0x04f0_0593,
            0x00b5_2223,
            0x04b0_0593,
            0x00b5_2223,
            0x0070_0613,
            0x0006_368b, // pcnt a3, a2
            0x0306_8693,
            0x00d5_2223,
            0x0010_0073, // ebreak
        ];
        let (vcpu, exit) = boot(0x1000, &program);
        assert!(matches!(exit, VcpuExit::Debug), "exit = {exit:?}");
        assert_eq!(vcpu.console(), b"OK3");
    }

    #[test]
    fn pmu_reset_loop_is_bounded() {
        // Program writes the PMU reset magic immediately, so every boot reboots.
        // The loop guard must terminate with a diagnostic rather than hang.
        //   lui   a0, 0x40000
        //   lui   a1, 0x07041
        //   addi  a1, a1, 0x776      ; a1 = 0x07041776
        //   sw    a1, 8(a0)          ; PMU reset
        //   ebreak                   ; (never reached)
        let program = [
            0x4000_0537u32,
            0x0704_15b7,
            0x7765_8593,
            0x00b5_2423,
            0x0010_0073,
        ];
        let (_vcpu, exit) = boot(0x1000, &program);
        match exit {
            VcpuExit::Unknown(msg) => assert!(
                msg.contains("reboot loop"),
                "expected reboot-loop diagnostic, got: {msg}"
            ),
            other => panic!("expected bounded reboot loop, got {other:?}"),
        }
    }
}
