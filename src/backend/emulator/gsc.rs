//! Google Security Chip (GSC) machine: Ti50 / Dauntless "Soteria" RISC-V core.
//!
//! This wires the [`crate::riscv`] interpreter up as a bare-metal RV32 machine
//! for Google's Ti50/Dauntless GSC firmware ("nugget" / Nugget OS). It differs
//! from the generic [`super::riscv::RiscVVcpu`] in three ways:
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
//! | `RAX_GSC_UART` | console UART base (hex) | `0x4060_0000` |
//! | `RAX_GSC_UART_STATE` | value returned by UART STATE reads (hex) | `0xffff_ffff` |
//! | `RAX_GSC_OPENBUS` | value returned for unmodeled MMIO reads (hex) | `0` |
//! | `RAX_GSC_READY` | extra fixed status registers, `addr=val,...` (hex) | — |
//! | `RAX_GSC_ENTRY` | override the boot entry PC (hex; parsed in the loader) | auto |
//! | `RAX_GSC_TRACE` | `mmio` = log first-touch MMIO; `insn` = also trace every instruction | off |
//!
//! The bridge also models persistent register storage (writes read back), a
//! generic spin-breaker for "wait for status bit" boot loops, a built-in map of
//! known clock/PLL "ready" status registers, and the PMU self-reset
//! (`0x4000_0008 <- 0x0704_1776`) as a warm reboot of the hart.

use std::collections::{BTreeSet, HashMap};
use std::io::Write;
use std::sync::{Arc, Mutex};

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use crate::cpu::{CpuState, RiscVRegisters, VCpu, VcpuExit};
use crate::error::{Error, Result};
use crate::riscv::{Isa, MemError, MemResult, Memory, RiscVConfig, RiscVCpu, RiscVExit};

/// SoC MMIO aperture. Peripheral registers live here; the bridge intercepts the
/// whole window so accesses never reach RAM and never raise an access fault.
const MMIO_LO: u64 = 0x4000_0000;
const MMIO_HI: u64 = 0x5000_0000;

/// UART register offsets (Cr50/Ti50 family, from the gscemu reference model).
const UART_RDATA: u64 = 0x00;
const UART_WDATA: u64 = 0x04;
const UART_STATE: u64 = 0x14;
/// Width of the modeled UART register block.
const UART_LEN: u64 = 0x100;

/// Default console UART base (overridable via `RAX_GSC_UART`).
const DEFAULT_UART_BASE: u64 = 0x4060_0000;
/// Default UART STATE read value (overridable via `RAX_GSC_UART_STATE`).
/// All-ones keeps any "transmitter ready" poll from spinning during bring-up.
const DEFAULT_UART_STATE: u32 = 0xffff_ffff;

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
    trace: Trace,
}

impl GscConfig {
    fn from_env() -> Self {
        let uart_base = env_hex("RAX_GSC_UART").unwrap_or(DEFAULT_UART_BASE);
        let uart_state = env_hex("RAX_GSC_UART_STATE")
            .map(|v| v as u32)
            .unwrap_or(DEFAULT_UART_STATE);
        let open_bus = env_hex("RAX_GSC_OPENBUS").map(|v| v as u32).unwrap_or(0);
        let trace = match std::env::var("RAX_GSC_TRACE").as_deref() {
            Ok("insn") => Trace::Insn,
            Ok("mmio") | Ok("1") => Trace::Mmio,
            _ => Trace::Off,
        };
        GscConfig {
            uart_base,
            uart_state,
            open_bus,
            trace,
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
                let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
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
    let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    u64::from_str_radix(s, 16).ok()
}

/// Once an unmodeled status register has been read this many times since the
/// last MMIO write (i.e. with no forward progress), the spin-breaker returns
/// all-ones to satisfy "wait for ready bit" boot loops.
const SPIN_THRESHOLD: u32 = 4096;

/// PMU "global reset" register and its magic value (`0x0704_1776` — a July-4
/// 1776 magic). Writing the magic here reboots the chip; GSC firmware does this
/// as a normal boot step ("configure clocks/fuses, then reset to apply").
const PMU_RESET_REG: u64 = 0x4000_0008;
const PMU_RESET_MAGIC: u32 = 0x0704_1776;

/// Safety bound: if the firmware reboots this many times without producing any
/// console output, give up rather than spin forever.
const MAX_RESETS: u32 = 32;

/// Mutable state shared between the bridge (which mutates it during memory
/// accesses, behind `&self`) and the vCPU (which reads it back out).
#[derive(Default)]
struct GscShared {
    /// Captured UART transmit bytes (the console output).
    console: Vec<u8>,
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
}

type Shared = Arc<Mutex<GscShared>>;

/// Guest memory bridge for the GSC machine: RAM/flash via [`GuestMemoryMmap`],
/// with the SoC MMIO aperture intercepted.
struct GscBridge {
    mem: Arc<GuestMemoryMmap>,
    shared: Shared,
    cfg: GscConfig,
    /// Status registers that read back a fixed "ready" value.
    ready: HashMap<u64, u32>,
}

impl std::fmt::Debug for GscBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GscBridge").finish()
    }
}

#[inline]
fn in_mmio(addr: u64) -> bool {
    (MMIO_LO..MMIO_HI).contains(&addr)
}

impl GscBridge {
    #[inline]
    fn in_uart(&self, addr: u64) -> bool {
        let base = self.cfg.uart_base;
        addr >= base && addr < base + UART_LEN
    }

    /// Record a first-touch MMIO access for the discovery trace.
    fn note(&self, addr: u64, is_write: bool, value: u32) {
        if self.cfg.trace == Trace::Off {
            return;
        }
        let key = (addr << 1) | is_write as u64;
        let mut sh = self.shared.lock().unwrap();
        if sh.seen.insert(key) {
            eprintln!(
                "[gsc] mmio {} {:#010x} {:#010x}",
                if is_write { "WR" } else { "RD" },
                addr,
                value
            );
        }
    }

    /// Read a UART register (no shared state needed).
    fn uart_read(&self, addr: u64) -> u32 {
        match addr - self.cfg.uart_base {
            UART_STATE => self.cfg.uart_state,
            UART_RDATA => 0, // no console input modeled yet
            _ => 0,
        }
    }

    /// Emit a console byte (to stdout and the capture buffer).
    fn console_out(&self, byte: u8) {
        self.shared.lock().unwrap().console.push(byte);
        let mut out = std::io::stdout();
        let _ = out.write_all(&[byte]);
        let _ = out.flush();
    }
}

impl Memory for GscBridge {
    fn read(&self, addr: u64, buf: &mut [u8]) -> MemResult<()> {
        if in_mmio(addr) {
            let word = if self.in_uart(addr) {
                self.uart_read(addr)
            } else if let Some(&v) = self.ready.get(&addr) {
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
            for (i, b) in buf.iter_mut().enumerate() {
                *b = (word >> (8 * (i as u32 & 3))) as u8;
            }
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
        if in_mmio(addr) {
            let mut word = [0u8; 4];
            let n = data.len().min(4);
            word[..n].copy_from_slice(&data[..n]);
            let value = u32::from_le_bytes(word);
            {
                let mut sh = self.shared.lock().unwrap();
                // A write is forward progress: reset the spin-breaker window.
                sh.read_counts.clear();
                sh.store.insert(addr, value);
                if addr == PMU_RESET_REG && value == PMU_RESET_MAGIC {
                    sh.reset_requested = true;
                }
            }
            self.note(addr, true, value);
            if self.in_uart(addr) && addr - self.cfg.uart_base == UART_WDATA {
                self.console_out((value & 0xff) as u8);
            }
            return Ok(());
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
}

impl GscVcpu {
    pub fn new(id: u32, mem: Arc<GuestMemoryMmap>) -> Self {
        let cfg = GscConfig::from_env();
        let shared: Shared = Arc::new(Mutex::new(GscShared::default()));
        let mut ready = builtin_ready_map();
        ready.extend(parse_ready_env());
        let bridge = GscBridge {
            mem,
            shared: shared.clone(),
            cfg,
            ready,
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
        self.cpu.reset(self.reset_entry);
        None
    }

    /// Snapshot of console bytes emitted so far (for tests / introspection).
    pub fn console(&self) -> Vec<u8> {
        self.shared.lock().unwrap().console.clone()
    }

    fn trap_exit(&self, t: crate::riscv::Trap) -> VcpuExit {
        VcpuExit::Unknown(format!(
            "gsc riscv trap: cause={} tval={:#x} pc={:#x} insn=[{}]",
            t.cause,
            t.tval,
            self.cpu.pc(),
            self.cpu.disasm_pc(),
        ))
    }
}

impl VCpu for GscVcpu {
    fn run(&mut self) -> Result<VcpuExit> {
        if self.halted {
            return Ok(VcpuExit::Hlt);
        }
        for _ in 0..MAX_ITERS {
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
                // Treat WFI as a hint: the firmware spins waiting for an
                // interrupt we don't deliver yet; keep going so polling loops
                // around the WFI make progress (bounded by MAX_ITERS).
                RiscVExit::Wfi => {}
                RiscVExit::Ecall => {
                    self.halted = true;
                    return Ok(VcpuExit::Unknown(format!(
                        "gsc riscv ecall at pc={:#x} (a7={:#x} a0={:#x})",
                        self.cpu.pc(),
                        self.cpu.x(17),
                        self.cpu.x(10),
                    )));
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
                self.halted = true;
                Ok(Some(VcpuExit::Unknown(format!(
                    "gsc riscv ecall at pc={:#x}",
                    self.cpu.pc()
                ))))
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
    use crate::cpu::CpuState;

    fn test_cfg() -> GscConfig {
        GscConfig {
            uart_base: DEFAULT_UART_BASE,
            uart_state: DEFAULT_UART_STATE,
            open_bus: 0,
            trace: Trace::Off,
        }
    }

    fn test_bridge() -> GscBridge {
        let mem = GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x1_0000usize)]).unwrap();
        GscBridge {
            mem: Arc::new(mem),
            shared: Arc::new(Mutex::new(GscShared::default())),
            cfg: test_cfg(),
            ready: builtin_ready_map(),
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

    #[test]
    fn ready_map_returns_fixed_status() {
        let b = test_bridge();
        // The built-in clock/PLL "locked" status (fw.bin / nugget).
        assert_eq!(rd32(&b, 0x404d_0014), 0x30);
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
        //   lui   a0, 0x40600        ; UART base
        //   addi  a1, x0, 'O'        ; 0x4f
        //   sw    a1, 4(a0)          ; UART WDATA
        //   addi  a1, x0, 'K'        ; 0x4b
        //   sw    a1, 4(a0)
        //   addi  a2, x0, 7
        //   pcnt  a3, a2             ; Xsoteria: a3 = popcount(7) = 3
        //   addi  a3, a3, 0x30       ; '0' + 3 = '3'
        //   sw    a3, 4(a0)
        //   ebreak
        let program = [
            0x4060_0537u32,
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
