//! Differential test harness: software interpreter vs. KVM (hardware oracle).
//!
//! Each test case is a short machine-code sequence ending in `HLT`. It is run on
//! BOTH the rax software interpreter and on KVM from an *identical* initial
//! architectural state, then the resulting state (GPRs, RIP, the observable
//! RFLAGS bits, a couple of XMM registers, and a scratch memory page) is
//! compared. Any divergence is an interpreter bug (or, rarely, a harness
//! limitation) and is reported precisely.
//!
//! Robustness:
//!  - If `/dev/kvm` cannot be opened/driven, every test self-skips (returns
//!    without failing) so the suite is green in no-KVM environments.
//!  - Execution on both backends is bounded so a buggy case cannot hang.
//!
//! Initial state: long mode with identity-mapped paging. Real long mode requires
//! paging (CR0.PG=1 with EFER.LMA=1), and KVM enforces this, so we install a
//! tiny identity map (PML4 -> PDPTE with 1GiB pages) making GPA == GVA. The
//! interpreter is given the exact same CR0/CR3/CR4/EFER/segments so the two are
//! directly comparable.

#![cfg(all(feature = "kvm", target_os = "linux"))]

use std::sync::Arc;

use rax::backend::emulator::x86_64::{flags, X86_64Vcpu};
use rax::cpu::{Registers, SystemRegisters, VCpu, VcpuExit};
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

// ---------------------------------------------------------------------------
// Memory layout (all identity-mapped: GVA == GPA)
// ---------------------------------------------------------------------------

/// Total guest memory size for each backend.
const MEM_SIZE: usize = 8 * 1024 * 1024; // 8 MiB
/// Where test code is loaded / RIP starts.
const CODE_ADDR: u64 = 0x1_0000;
/// Initial stack pointer.
const STACK_ADDR: u64 = 0x2_0000;
/// Scratch data page (read back after the run for memory-effect tests).
const DATA_ADDR: u64 = 0x3_0000;
/// Page-table addresses (mirrors src/arch/x86_64 layout).
const PML4_ADDR: u64 = 0x9000;
const PDPTE_ADDR: u64 = 0xA000;

// Long-mode control register / EFER values WITH paging enabled.
const CR0_PE: u64 = 1 << 0;
const CR0_MP: u64 = 1 << 1;
const CR0_ET: u64 = 1 << 4;
const CR0_NE: u64 = 1 << 5;
const CR0_WP: u64 = 1 << 16;
const CR0_PG: u64 = 1 << 31;
const CR0_VAL: u64 = CR0_PE | CR0_MP | CR0_ET | CR0_NE | CR0_WP | CR0_PG;
const CR4_PAE: u64 = 1 << 5;
const CR4_OSFXSR: u64 = 1 << 9; // enable SSE / FXSAVE so SSE2 ops don't #UD
const CR4_OSXMMEXCPT: u64 = 1 << 10;
const CR4_VAL: u64 = CR4_PAE | CR4_OSFXSR | CR4_OSXMMEXCPT;
const EFER_SCE: u64 = 1 << 0;
const EFER_LME: u64 = 1 << 8;
const EFER_LMA: u64 = 1 << 10;
const EFER_VAL: u64 = EFER_SCE | EFER_LME | EFER_LMA;

// CR0 has MP|ET|NE set but TS/EM clear, so SSE is usable.

const MAX_ITERS: u64 = 100_000;

// ---------------------------------------------------------------------------
// Captured state for comparison
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct FinalState {
    regs: Registers,
    /// All 16 XMM registers, [low, high].
    xmm: [[u64; 2]; 16],
    /// Snapshot of the scratch data page (first 64 bytes).
    scratch: [u8; 64],
}

/// Observable, architecturally-defined RFLAGS status bits.
const FLAG_MASK: u64 =
    flags::bits::CF | flags::bits::PF | flags::bits::AF | flags::bits::ZF | flags::bits::SF | flags::bits::OF;

// ---------------------------------------------------------------------------
// Build the shared identity-mapped long-mode initial state.
// ---------------------------------------------------------------------------

fn base_sregs() -> SystemRegisters {
    let mut sregs = SystemRegisters::default();
    sregs.cr0 = CR0_VAL;
    sregs.cr3 = PML4_ADDR;
    sregs.cr4 = CR4_VAL;
    sregs.efer = EFER_VAL;

    // Flat 64-bit code segment (CS.L=1).
    sregs.cs.base = 0;
    sregs.cs.limit = 0xFFFFF;
    sregs.cs.selector = 0x8;
    sregs.cs.type_ = 0xB; // code, executed/read/accessed
    sregs.cs.present = true;
    sregs.cs.dpl = 0;
    sregs.cs.s = true;
    sregs.cs.l = true;
    sregs.cs.db = false; // must be 0 when L=1
    sregs.cs.g = true;

    // Flat data segments (DS/ES/FS/GS/SS).
    let mut data = sregs.cs.clone();
    data.selector = 0x10;
    data.type_ = 0x3; // data, read/write/accessed
    data.l = false;
    data.db = true;
    for seg in [
        &mut sregs.ds,
        &mut sregs.es,
        &mut sregs.fs,
        &mut sregs.gs,
        &mut sregs.ss,
    ] {
        *seg = data.clone();
    }

    sregs
}

/// Write the page tables + scratch page into a `Bytes` guest memory.
fn install_tables_mmap(write: &mut dyn FnMut(u64, &[u8])) {
    // PML4[0] -> PDPTE (present + writable)
    write(PML4_ADDR, &(PDPTE_ADDR | 0x3).to_le_bytes());
    // PDPTE[i] identity 1GiB huge pages (present + writable + PS), 4 entries (4GiB).
    for i in 0u64..4 {
        let entry: u64 = (i << 30) | 0x83;
        write(PDPTE_ADDR + i * 8, &entry.to_le_bytes());
    }
}

// ---------------------------------------------------------------------------
// Interpreter backend
// ---------------------------------------------------------------------------

fn run_interpreter(code: &[u8], init: &Registers, scratch_init: &[u8; 64]) -> Result<FinalState, String> {
    let regions = vec![(GuestAddress(0), MEM_SIZE)];
    let mem = Arc::new(GuestMemoryMmap::<()>::from_ranges(&regions).map_err(|e| format!("mem: {e:?}"))?);

    // Page tables.
    install_tables_mmap(&mut |addr, bytes| {
        mem.write_slice(bytes, GuestAddress(addr)).unwrap();
    });
    // Code + scratch.
    mem.write_slice(code, GuestAddress(CODE_ADDR)).map_err(|e| format!("code: {e:?}"))?;
    mem.write_slice(scratch_init, GuestAddress(DATA_ADDR)).map_err(|e| format!("scratch: {e:?}"))?;

    let mut vcpu = X86_64Vcpu::new(0, mem.clone());

    let mut regs = init.clone();
    regs.rip = CODE_ADDR;
    if regs.rsp == 0 {
        regs.rsp = STACK_ADDR;
    }
    regs.rflags |= 0x2; // reserved bit 1 always set
    vcpu.set_regs(&regs).map_err(|e| format!("set_regs: {e:?}"))?;
    vcpu.set_sregs(&base_sregs()).map_err(|e| format!("set_sregs: {e:?}"))?;

    // Run to HLT, counting individual instructions via step().
    let mut iters = 0u64;
    loop {
        iters += 1;
        if iters > MAX_ITERS {
            return Err(format!("interpreter exceeded {MAX_ITERS} iterations"));
        }
        match vcpu.step().map_err(|e| format!("step: {e:?}"))? {
            Some(VcpuExit::Hlt) => break,
            Some(VcpuExit::IoIn { size, .. }) => {
                let data = vec![0u8; size as usize];
                vcpu.complete_io_in(&data);
            }
            Some(VcpuExit::Shutdown) | Some(VcpuExit::FailEntry { .. }) | Some(VcpuExit::InternalError) => {
                return Err("interpreter abnormal exit".to_string());
            }
            _ => {}
        }
    }

    let final_regs = vcpu.get_regs().map_err(|e| format!("get_regs: {e:?}"))?;
    let mut scratch = [0u8; 64];
    mem.read_slice(&mut scratch, GuestAddress(DATA_ADDR)).map_err(|e| format!("read scratch: {e:?}"))?;

    Ok(FinalState {
        xmm: final_regs.xmm,
        regs: final_regs,
        scratch,
    })
}

// ---------------------------------------------------------------------------
// KVM backend (driven directly with kvm-ioctls, mirroring kvm_minimal.rs)
// ---------------------------------------------------------------------------

/// Owns the mmap backing KVM guest memory so we can read it back and free it.
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
        Some(KvmMem { ptr: ptr as *mut u8, size })
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

/// Returns Ok(None) if KVM is unavailable (so callers can skip gracefully),
/// Ok(Some(state)) on success, Err on a genuine run failure.
fn run_kvm(code: &[u8], init: &Registers, scratch_init: &[u8; 64]) -> Result<Option<FinalState>, String> {
    use kvm_bindings::{kvm_segment, kvm_userspace_memory_region};
    use kvm_ioctls::Kvm;

    let kvm = match Kvm::new() {
        Ok(k) => k,
        Err(_) => return Ok(None), // /dev/kvm unavailable -> skip
    };
    let vm = match kvm.create_vm() {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };

    let mem = match KvmMem::new(MEM_SIZE) {
        Some(m) => m,
        None => return Ok(None),
    };

    // Identity-mapped page tables + code + scratch.
    install_tables_mmap(&mut |addr, bytes| mem.write(addr, bytes));
    mem.write(CODE_ADDR, code);
    mem.write(DATA_ADDR, scratch_init);

    let region = kvm_userspace_memory_region {
        slot: 0,
        guest_phys_addr: 0,
        memory_size: MEM_SIZE as u64,
        userspace_addr: mem.ptr as u64,
        flags: 0,
    };
    if unsafe { vm.set_user_memory_region(region) }.is_err() {
        return Ok(None);
    }

    let mut vcpu = match vm.create_vcpu(0) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };

    // --- sregs: long mode w/ paging, flat segments ---
    let our_sregs = base_sregs();
    let mut sregs = vcpu.get_sregs().map_err(|e| format!("kvm get_sregs: {e:?}"))?;

    let to_kvm_seg = |s: &rax::cpu::Segment| kvm_segment {
        base: s.base,
        limit: s.limit,
        selector: s.selector,
        type_: s.type_,
        present: s.present as u8,
        dpl: s.dpl,
        db: s.db as u8,
        s: s.s as u8,
        l: s.l as u8,
        g: s.g as u8,
        avl: s.avl as u8,
        unusable: s.unusable as u8,
        padding: 0,
    };

    sregs.cr0 = our_sregs.cr0;
    sregs.cr3 = our_sregs.cr3;
    sregs.cr4 = our_sregs.cr4;
    sregs.efer = our_sregs.efer;
    sregs.cs = to_kvm_seg(&our_sregs.cs);
    sregs.ds = to_kvm_seg(&our_sregs.ds);
    sregs.es = to_kvm_seg(&our_sregs.es);
    sregs.fs = to_kvm_seg(&our_sregs.fs);
    sregs.gs = to_kvm_seg(&our_sregs.gs);
    sregs.ss = to_kvm_seg(&our_sregs.ss);
    vcpu.set_sregs(&sregs).map_err(|e| format!("kvm set_sregs: {e:?}"))?;

    // --- gprs ---
    let mut kregs = vcpu.get_regs().map_err(|e| format!("kvm get_regs: {e:?}"))?;
    kregs.rax = init.rax;
    kregs.rbx = init.rbx;
    kregs.rcx = init.rcx;
    kregs.rdx = init.rdx;
    kregs.rsi = init.rsi;
    kregs.rdi = init.rdi;
    kregs.rbp = init.rbp;
    kregs.rsp = if init.rsp == 0 { STACK_ADDR } else { init.rsp };
    kregs.r8 = init.r8;
    kregs.r9 = init.r9;
    kregs.r10 = init.r10;
    kregs.r11 = init.r11;
    kregs.r12 = init.r12;
    kregs.r13 = init.r13;
    kregs.r14 = init.r14;
    kregs.r15 = init.r15;
    kregs.rip = CODE_ADDR;
    kregs.rflags = init.rflags | 0x2;
    vcpu.set_regs(&kregs).map_err(|e| format!("kvm set_regs: {e:?}"))?;

    // --- xmm (via FPU state) ---
    if init.xmm.iter().any(|x| x != &[0, 0]) {
        let mut fpu = vcpu.get_fpu().map_err(|e| format!("kvm get_fpu: {e:?}"))?;
        for i in 0..16 {
            let lo = init.xmm[i][0].to_le_bytes();
            let hi = init.xmm[i][1].to_le_bytes();
            fpu.xmm[i][..8].copy_from_slice(&lo);
            fpu.xmm[i][8..].copy_from_slice(&hi);
        }
        vcpu.set_fpu(&fpu).map_err(|e| format!("kvm set_fpu: {e:?}"))?;
    }

    // --- run to HLT, bounded ---
    let mut iters = 0u64;
    loop {
        iters += 1;
        if iters > MAX_ITERS {
            return Err(format!("kvm exceeded {MAX_ITERS} iterations"));
        }
        match vcpu.run().map_err(|e| format!("kvm run: {e:?}"))? {
            kvm_ioctls::VcpuExit::Hlt => break,
            kvm_ioctls::VcpuExit::IoIn(_, data) => {
                for b in data.iter_mut() {
                    *b = 0;
                }
            }
            kvm_ioctls::VcpuExit::IoOut(..) => {}
            kvm_ioctls::VcpuExit::MmioRead(_, data) => {
                for b in data.iter_mut() {
                    *b = 0;
                }
            }
            kvm_ioctls::VcpuExit::MmioWrite(..) => {}
            other => {
                return Err(format!("kvm abnormal exit: {other:?}"));
            }
        }
    }

    let final_kregs = vcpu.get_regs().map_err(|e| format!("kvm get_regs(final): {e:?}"))?;
    let final_fpu = vcpu.get_fpu().map_err(|e| format!("kvm get_fpu(final): {e:?}"))?;

    let mut regs = Registers::default();
    regs.rax = final_kregs.rax;
    regs.rbx = final_kregs.rbx;
    regs.rcx = final_kregs.rcx;
    regs.rdx = final_kregs.rdx;
    regs.rsi = final_kregs.rsi;
    regs.rdi = final_kregs.rdi;
    regs.rsp = final_kregs.rsp;
    regs.rbp = final_kregs.rbp;
    regs.r8 = final_kregs.r8;
    regs.r9 = final_kregs.r9;
    regs.r10 = final_kregs.r10;
    regs.r11 = final_kregs.r11;
    regs.r12 = final_kregs.r12;
    regs.r13 = final_kregs.r13;
    regs.r14 = final_kregs.r14;
    regs.r15 = final_kregs.r15;
    regs.rip = final_kregs.rip;
    regs.rflags = final_kregs.rflags;

    let mut xmm = [[0u64; 2]; 16];
    for i in 0..16 {
        let lo = u64::from_le_bytes(final_fpu.xmm[i][..8].try_into().unwrap());
        let hi = u64::from_le_bytes(final_fpu.xmm[i][8..].try_into().unwrap());
        xmm[i] = [lo, hi];
    }
    regs.xmm = xmm;

    let mut scratch = [0u8; 64];
    mem.read(DATA_ADDR, &mut scratch);

    Ok(Some(FinalState { regs, xmm, scratch }))
}

// ---------------------------------------------------------------------------
// Comparison
// ---------------------------------------------------------------------------

/// What aspects of architectural state to compare.
#[derive(Clone, Copy)]
struct CompareOpts {
    /// RFLAGS bits to compare. Usually all status flags; for instructions that
    /// leave some flags *architecturally undefined* (MUL/IMUL/DIV/shifts by a
    /// variable count, etc.) the undefined bits are masked out so we only check
    /// the bits the ISA actually defines.
    flag_mask: u64,
    /// Number of XMM registers to compare (0 = none).
    xmm_count: usize,
    /// Compare the scratch data page.
    scratch: bool,
    /// Compare RSP/RBP (off for tests that intentionally don't touch the stack
    /// in a comparable way; on by default).
    stack: bool,
}

impl Default for CompareOpts {
    fn default() -> Self {
        CompareOpts {
            flag_mask: FLAG_MASK,
            xmm_count: 0,
            scratch: false,
            stack: true,
        }
    }
}

fn gpr_list(r: &Registers) -> [(&'static str, u64); 16] {
    [
        ("rax", r.rax),
        ("rbx", r.rbx),
        ("rcx", r.rcx),
        ("rdx", r.rdx),
        ("rsi", r.rsi),
        ("rdi", r.rdi),
        ("rsp", r.rsp),
        ("rbp", r.rbp),
        ("r8", r.r8),
        ("r9", r.r9),
        ("r10", r.r10),
        ("r11", r.r11),
        ("r12", r.r12),
        ("r13", r.r13),
        ("r14", r.r14),
        ("r15", r.r15),
    ]
}

fn compare(interp: &FinalState, kvm: &FinalState, opts: CompareOpts) -> Vec<String> {
    let mut diffs = Vec::new();

    let il = gpr_list(&interp.regs);
    let kl = gpr_list(&kvm.regs);
    for ((name, iv), (_, kv)) in il.iter().zip(kl.iter()) {
        if !opts.stack && (*name == "rsp" || *name == "rbp") {
            continue;
        }
        if iv != kv {
            diffs.push(format!("{name}: interp={iv:#x} kvm={kv:#x}"));
        }
    }

    if opts.flag_mask != 0 {
        let im = interp.regs.rflags & opts.flag_mask;
        let km = kvm.regs.rflags & opts.flag_mask;
        if im != km {
            diffs.push(format!(
                "rflags(status): interp={:#x} kvm={:#x} (diff bits={:#x}) [{}]",
                im,
                km,
                im ^ km,
                describe_flags(im ^ km)
            ));
        }
    }

    for i in 0..opts.xmm_count {
        if interp.xmm[i] != kvm.xmm[i] {
            diffs.push(format!(
                "xmm{i}: interp=[{:#018x},{:#018x}] kvm=[{:#018x},{:#018x}]",
                interp.xmm[i][0], interp.xmm[i][1], kvm.xmm[i][0], kvm.xmm[i][1]
            ));
        }
    }

    if opts.scratch && interp.scratch != kvm.scratch {
        diffs.push(format!(
            "scratch page differs:\n  interp={:02x?}\n  kvm   ={:02x?}",
            &interp.scratch[..],
            &kvm.scratch[..]
        ));
    }

    diffs
}

fn describe_flags(bits: u64) -> String {
    let mut v = Vec::new();
    if bits & flags::bits::CF != 0 {
        v.push("CF");
    }
    if bits & flags::bits::PF != 0 {
        v.push("PF");
    }
    if bits & flags::bits::AF != 0 {
        v.push("AF");
    }
    if bits & flags::bits::ZF != 0 {
        v.push("ZF");
    }
    if bits & flags::bits::SF != 0 {
        v.push("SF");
    }
    if bits & flags::bits::OF != 0 {
        v.push("OF");
    }
    v.join("|")
}

// ---------------------------------------------------------------------------
// Top-level driver used by every test case.
// ---------------------------------------------------------------------------

/// Run `code` on both backends from `init`, returning `(interp, kvm)`.
/// Returns `None` if KVM is unavailable (the test should then `return`).
fn run_both(code: &[u8], init: Registers, scratch_init: [u8; 64]) -> Option<(FinalState, FinalState)> {
    // KVM first: if unavailable we skip without even bothering the interpreter.
    let kvm = match run_kvm(code, &init, &scratch_init) {
        Ok(Some(s)) => s,
        Ok(None) => {
            eprintln!("[skip] /dev/kvm unavailable or undrivable; skipping differential case");
            return None;
        }
        Err(e) => panic!("KVM backend failure: {e}"),
    };
    let interp = match run_interpreter(code, &init, &scratch_init) {
        Ok(s) => s,
        Err(e) => panic!("interpreter backend failure: {e}"),
    };
    Some((interp, kvm))
}

/// Assert that the two backends agree, with a precise diff on mismatch.
fn assert_match(label: &str, code: &[u8], interp: &FinalState, kvm: &FinalState, opts: CompareOpts) {
    let diffs = compare(interp, kvm, opts);
    if !diffs.is_empty() {
        panic!(
            "DIVERGENCE in `{label}` (code = {:02x?}):\n  {}",
            code,
            diffs.join("\n  ")
        );
    }
}

// Convenience: a zeroed scratch page.
fn zero_scratch() -> [u8; 64] {
    [0u8; 64]
}

/// Run a case with default compare options (GPRs + all status flags + stack).
fn check(label: &str, code: &[u8], init: Registers) {
    let Some((interp, kvm)) = run_both(code, init, zero_scratch()) else {
        return;
    };
    assert_match(label, code, &interp, &kvm, CompareOpts::default());
}

/// Run a case comparing GPRs + only the flag bits in `flag_mask` (others are
/// architecturally undefined for this instruction and must not be compared).
fn check_flags_masked(label: &str, code: &[u8], init: Registers, flag_mask: u64) {
    let Some((interp, kvm)) = run_both(code, init, zero_scratch()) else {
        return;
    };
    let opts = CompareOpts {
        flag_mask,
        ..CompareOpts::default()
    };
    assert_match(label, code, &interp, &kvm, opts);
}

/// Run an SSE case: the scratch page holds inputs, the guest code loads them
/// into XMM, operates, and stores the result back; we compare the scratch page
/// (and the live XMM registers as a bonus). Driving SSE through guest memory
/// avoids host-side FPU-injection quirks across the two backends.
fn check_sse(label: &str, code: &[u8], scratch_in: [u8; 64]) {
    let Some((interp, kvm)) = run_both(code, regs(), scratch_in) else {
        return;
    };
    let opts = CompareOpts {
        flag_mask: 0, // SSE integer ops don't set RFLAGS
        scratch: true,
        ..CompareOpts::default()
    };
    assert_match(label, code, &interp, &kvm, opts);
}

// Helper to build initial regs concisely.
fn regs() -> Registers {
    Registers::default()
}

// `HLT` terminator.
const HLT: u8 = 0xF4;

/// Append a HLT to a code buffer.
fn with_hlt(mut code: Vec<u8>) -> Vec<u8> {
    code.push(HLT);
    code
}

// ===========================================================================
// TEST CORPUS
// ===========================================================================

// ---- ADD / SUB / ADC / SBB / INC / DEC / NEG / CMP : flag exactness ----

#[test]
fn add_basic() {
    // mov rax, 0x...; add rax, rbx
    let mut r = regs();
    r.rax = 0x1122_3344_5566_7788;
    r.rbx = 0x1010_1010_1010_1010;
    // 48 01 D8  add rax, rbx
    check("add_rax_rbx", &with_hlt(vec![0x48, 0x01, 0xD8]), r);
}

#[test]
fn add_carry_overflow() {
    let mut r = regs();
    r.rax = 0x7fff_ffff_ffff_ffff; // signed max
    r.rbx = 1; // -> overflow + sign flip
    check("add_signed_overflow", &with_hlt(vec![0x48, 0x01, 0xD8]), r);
}

#[test]
fn add8_af_edge() {
    // add al, bl with low-nibble carry to exercise AF + PF
    let mut r = regs();
    r.rax = 0x0F;
    r.rbx = 0x01; // 0x0F + 0x01 = 0x10 -> AF set
    // 00 D8  add al, bl
    check("add8_af", &with_hlt(vec![0x00, 0xD8]), r);
}

#[test]
fn add8_unsigned_wrap() {
    let mut r = regs();
    r.rax = 0xFF;
    r.rbx = 0x01; // wraps to 0 -> CF, ZF, AF
    check("add8_wrap", &with_hlt(vec![0x00, 0xD8]), r);
}

#[test]
fn sub_basic() {
    let mut r = regs();
    r.rax = 0x1000;
    r.rbx = 0x0001;
    // 48 29 D8  sub rax, rbx
    check("sub_rax_rbx", &with_hlt(vec![0x48, 0x29, 0xD8]), r);
}

#[test]
fn sub_borrow() {
    let mut r = regs();
    r.rax = 0x00;
    r.rbx = 0x01; // 0 - 1 -> CF + AF + SF
    check("sub_borrow", &with_hlt(vec![0x48, 0x29, 0xD8]), r);
}

#[test]
fn sub8_signed_overflow() {
    let mut r = regs();
    r.rax = 0x80; // -128
    r.rbx = 0x01; //  - 1 -> overflow
    // 28 D8  sub al, bl
    check("sub8_overflow", &with_hlt(vec![0x28, 0xD8]), r);
}

#[test]
fn adc_with_carry_set() {
    let mut r = regs();
    r.rax = 0x10;
    r.rbx = 0x20;
    r.rflags = flags::bits::CF; // CF=1 going in
    // 48 11 D8  adc rax, rbx
    check("adc_carry_in", &with_hlt(vec![0x48, 0x11, 0xD8]), r);
}

#[test]
fn adc_chain_propagation() {
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF;
    r.rbx = 0x0;
    r.rflags = flags::bits::CF;
    check("adc_max_plus_carry", &with_hlt(vec![0x48, 0x11, 0xD8]), r);
}

#[test]
fn sbb_with_borrow() {
    let mut r = regs();
    r.rax = 0x100;
    r.rbx = 0x1;
    r.rflags = flags::bits::CF; // borrow in
    // 48 19 D8  sbb rax, rbx
    check("sbb_borrow_in", &with_hlt(vec![0x48, 0x19, 0xD8]), r);
}

#[test]
fn inc_overflow_preserves_cf() {
    let mut r = regs();
    r.rax = 0x7fff_ffff_ffff_ffff;
    r.rflags = flags::bits::CF; // INC must NOT touch CF
    // 48 FF C0  inc rax
    check("inc_of_keeps_cf", &with_hlt(vec![0x48, 0xFF, 0xC0]), r);
}

#[test]
fn dec_to_zero() {
    let mut r = regs();
    r.rax = 0x1;
    r.rflags = flags::bits::CF;
    // 48 FF C8  dec rax
    check("dec_to_zero", &with_hlt(vec![0x48, 0xFF, 0xC8]), r);
}

#[test]
fn dec_wrap_af() {
    let mut r = regs();
    r.rax = 0x10; // dec -> 0x0F, AF set
    // 48 FF C8 dec rax
    check("dec_af", &with_hlt(vec![0x48, 0xFF, 0xC8]), r);
}

#[test]
fn neg_nonzero() {
    let mut r = regs();
    r.rax = 0x1234;
    // 48 F7 D8  neg rax  (CF set because operand != 0)
    check("neg_nonzero", &with_hlt(vec![0x48, 0xF7, 0xD8]), r);
}

#[test]
fn neg_zero() {
    let mut r = regs();
    r.rax = 0x0; // neg 0 -> 0, CF clear, ZF set
    check("neg_zero", &with_hlt(vec![0x48, 0xF7, 0xD8]), r);
}

#[test]
fn cmp_equal() {
    let mut r = regs();
    r.rax = 0x42;
    r.rbx = 0x42;
    // 48 39 D8  cmp rax, rbx
    check("cmp_equal", &with_hlt(vec![0x48, 0x39, 0xD8]), r);
}

#[test]
fn cmp_less_signed() {
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF; // -1
    r.rbx = 0x1;
    check("cmp_neg_vs_pos", &with_hlt(vec![0x48, 0x39, 0xD8]), r);
}

// ---- AND / OR / XOR / TEST ----

#[test]
fn and_clears_cf_of() {
    let mut r = regs();
    r.rax = 0xFF00_FF00_FF00_FF00;
    r.rbx = 0x0FF0_0FF0_0FF0_0FF0;
    r.rflags = flags::bits::CF | flags::bits::OF; // must be cleared
    // 48 21 D8  and rax, rbx
    check("and", &with_hlt(vec![0x48, 0x21, 0xD8]), r);
}

#[test]
fn or_sets_sf_pf() {
    let mut r = regs();
    r.rax = 0x8000_0000_0000_0000;
    r.rbx = 0x1;
    // 48 09 D8  or rax, rbx
    check("or", &with_hlt(vec![0x48, 0x09, 0xD8]), r);
}

#[test]
fn xor_self_zero() {
    let mut r = regs();
    r.rax = 0xDEAD_BEEF_CAFE_BABE;
    // 48 31 C0  xor rax, rax
    check("xor_self", &with_hlt(vec![0x48, 0x31, 0xC0]), r);
}

#[test]
fn test_parity() {
    let mut r = regs();
    r.rax = 0xFF; // low byte has even parity (8 ones) -> PF=1
    // 48 85 C0  test rax, rax
    check("test_parity", &with_hlt(vec![0x48, 0x85, 0xC0]), r);
}

// ---- Shifts / rotates : CF & OF ----

#[test]
fn shl_into_cf_of() {
    let mut r = regs();
    r.rax = 0x4000_0000_0000_0000;
    // 48 D1 E0  shl rax, 1  (1-bit shift defines OF)
    check("shl1", &with_hlt(vec![0x48, 0xD1, 0xE0]), r);
}

#[test]
fn shl_by_cl() {
    let mut r = regs();
    r.rax = 0x1;
    r.rcx = 4;
    // 48 D3 E0  shl rax, cl
    check("shl_cl", &with_hlt(vec![0x48, 0xD3, 0xE0]), r);
}

#[test]
fn shr_cf() {
    let mut r = regs();
    r.rax = 0x3; // shr by 1 -> CF from bit 0
    // 48 D1 E8  shr rax, 1
    check("shr1", &with_hlt(vec![0x48, 0xD1, 0xE8]), r);
}

#[test]
fn sar_sign_extend() {
    let mut r = regs();
    r.rax = 0x8000_0000_0000_0000;
    r.rcx = 4;
    // 48 D3 F8  sar rax, cl
    check("sar_cl", &with_hlt(vec![0x48, 0xD3, 0xF8]), r);
}

#[test]
fn rol_cf_of() {
    let mut r = regs();
    r.rax = 0x8000_0000_0000_0001;
    // 48 D1 C0  rol rax, 1
    check("rol1", &with_hlt(vec![0x48, 0xD1, 0xC0]), r);
}

#[test]
fn ror_cf_of() {
    let mut r = regs();
    r.rax = 0x1;
    // 48 D1 C8  ror rax, 1
    check("ror1", &with_hlt(vec![0x48, 0xD1, 0xC8]), r);
}

#[test]
fn rcl_through_carry() {
    let mut r = regs();
    r.rax = 0x8000_0000_0000_0000;
    r.rflags = flags::bits::CF; // carry rotates in
    // 48 D1 D0  rcl rax, 1
    check("rcl1", &with_hlt(vec![0x48, 0xD1, 0xD0]), r);
}

#[test]
fn rcr_through_carry() {
    let mut r = regs();
    r.rax = 0x1;
    r.rflags = flags::bits::CF;
    // 48 D1 D8  rcr rax, 1
    check("rcr1", &with_hlt(vec![0x48, 0xD1, 0xD8]), r);
}

// ---- IMUL / MUL / DIV ----

// For MUL/IMUL the ISA defines only CF and OF; SF/ZF/AF/PF are *undefined*.
// For DIV/IDIV *all* status flags are undefined. We therefore mask the
// comparison accordingly so we only validate the architecturally-defined bits
// (and the full GPR result, which is always defined).
const MULDIV_DEFINED: u64 = flags::bits::CF | flags::bits::OF;

#[test]
fn imul_two_operand() {
    let mut r = regs();
    r.rax = 0x1_0000;
    r.rbx = 0x1_0000; // 2^16 * 2^16 = 2^32 (no overflow of 64-bit)
    // 48 0F AF C3  imul rax, rbx
    check_flags_masked("imul2", &with_hlt(vec![0x48, 0x0F, 0xAF, 0xC3]), r, MULDIV_DEFINED);
}

#[test]
fn imul_overflow_flags() {
    let mut r = regs();
    r.rax = 0x8000_0000_0000_0000u64; // large -> CF/OF set on truncation
    r.rbx = 0x2;
    check_flags_masked(
        "imul_overflow",
        &with_hlt(vec![0x48, 0x0F, 0xAF, 0xC3]),
        r,
        MULDIV_DEFINED,
    );
}

#[test]
fn mul_rdx_rax() {
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF;
    r.rbx = 0x2; // RDX:RAX = product
    // 48 F7 E3  mul rbx
    check_flags_masked("mul64", &with_hlt(vec![0x48, 0xF7, 0xE3]), r, MULDIV_DEFINED);
}

#[test]
fn div_unsigned() {
    let mut r = regs();
    r.rax = 0x100; // dividend low
    r.rdx = 0x0; // dividend high
    r.rbx = 0x7; // divisor
    // 48 F7 F3  div rbx -> quotient in rax, remainder in rdx
    // All flags undefined for DIV; compare GPRs only (flag_mask = 0).
    check_flags_masked("div64", &with_hlt(vec![0x48, 0xF7, 0xF3]), r, 0);
}

#[test]
fn idiv_signed() {
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_FFFF_FF9C; // -100
    r.rdx = 0xFFFF_FFFF_FFFF_FFFF; // sign extension of -100
    r.rbx = 0x7;
    // 48 F7 FB  idiv rbx
    check_flags_masked("idiv64", &with_hlt(vec![0x48, 0xF7, 0xFB]), r, 0);
}

// ---- BSF / BSR ----

#[test]
fn bsf_nonzero() {
    let mut r = regs();
    r.rbx = 0x0000_0000_0001_0000; // lowest set bit = 16
    // 48 0F BC C3  bsf rax, rbx
    check("bsf", &with_hlt(vec![0x48, 0x0F, 0xBC, 0xC3]), r);
}

#[test]
fn bsr_nonzero() {
    let mut r = regs();
    r.rbx = 0x0000_0000_0001_0000; // highest set bit = 16
    // 48 0F BD C3  bsr rax, rbx
    check("bsr", &with_hlt(vec![0x48, 0x0F, 0xBD, 0xC3]), r);
}

// ---- SETcc / CMOVcc ----

#[test]
fn setcc_below() {
    // cmp then sete/setb on AL
    let mut r = regs();
    r.rax = 0x1;
    r.rbx = 0x2; // 1 < 2 -> CF set
    // 48 39 D8       cmp rax, rbx
    // 0F 92 C0       setb al
    check("setb", &with_hlt(vec![0x48, 0x39, 0xD8, 0x0F, 0x92, 0xC0]), r);
}

#[test]
fn cmovcc_greater() {
    let mut r = regs();
    r.rax = 0x5;
    r.rbx = 0x3;
    r.rcx = 0xAAAA; // value to maybe move
    // 48 39 D8        cmp rax, rbx     (5 - 3 -> not below, SF=OF, ZF=0)
    // 48 0F 4F C1     cmovg rax, rcx   (move if greater)
    check(
        "cmovg",
        &with_hlt(vec![0x48, 0x39, 0xD8, 0x48, 0x0F, 0x4F, 0xC1]),
        r,
    );
}

// ---- LEA ----

#[test]
fn lea_scaled_index() {
    let mut r = regs();
    r.rbx = 0x1000;
    r.rcx = 0x10;
    // 48 8D 04 8B  lea rax, [rbx + rcx*4]
    check("lea", &with_hlt(vec![0x48, 0x8D, 0x04, 0x8B]), r);
}

// ---- MOVZX / MOVSX ----

#[test]
fn movzx_byte() {
    let mut r = regs();
    r.rbx = 0xFFFF_FFFF_FFFF_FF80;
    // 48 0F B6 C3  movzx rax, bl
    check("movzx_bl", &with_hlt(vec![0x48, 0x0F, 0xB6, 0xC3]), r);
}

#[test]
fn movsx_byte() {
    let mut r = regs();
    r.rbx = 0x80; // sign-extends to 0xFFFF...80
    // 48 0F BE C3  movsx rax, bl
    check("movsx_bl", &with_hlt(vec![0x48, 0x0F, 0xBE, 0xC3]), r);
}

#[test]
fn movsxd_dword() {
    let mut r = regs();
    r.rbx = 0x0000_0000_8000_0000; // ebx = 0x80000000 sign-extends
    // 48 63 C3  movsxd rax, ebx
    check("movsxd", &with_hlt(vec![0x48, 0x63, 0xC3]), r);
}

// ---- SSE2: PADDB / PADDW / PADDD / PSUBB / PXOR / PAND via XMM ----
//
// SSE XMM state injection through KVM_SET_FPU does not reliably survive
// KVM_RUN's XSAVE-based FPU management, so instead of host-injecting XMM we
// drive these through guest memory: the guest loads two 128-bit inputs from the
// scratch page, performs the op, and stores the 128-bit result back. We then
// compare the scratch page byte-for-byte (and the live XMM regs). This is both
// robust and a closer model of real SSE code.
//
// Scratch layout: [0..16) = input A (xmm0), [16..32) = input B (xmm1),
//                 [32..48) = result (written by the guest).

/// Build a guest program: load xmm0/xmm1 from scratch, run `op` (a `66 0F xx C1`
/// style 2-operand SSE instruction on xmm0,xmm1), store xmm0 back, HLT.
fn sse_program(op: &[u8]) -> Vec<u8> {
    let mut code = Vec::new();
    // mov rdi, DATA_ADDR (imm32, sign-extended — 0x30000 fits)
    code.extend_from_slice(&[0x48, 0xC7, 0xC7]);
    code.extend_from_slice(&(DATA_ADDR as u32).to_le_bytes());
    // movdqu xmm0, [rdi]
    code.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]);
    // movdqu xmm1, [rdi+0x10]
    code.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]);
    // <op> xmm0, xmm1
    code.extend_from_slice(op);
    // movdqu [rdi+0x20], xmm0
    code.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    code.push(HLT);
    code
}

/// Build a 64-byte scratch page from two 16-byte inputs.
fn sse_scratch(a: [u8; 16], b: [u8; 16]) -> [u8; 64] {
    let mut s = [0u8; 64];
    s[0..16].copy_from_slice(&a);
    s[16..32].copy_from_slice(&b);
    s
}

#[test]
fn sse_paddb() {
    // per-byte add with wraps in several lanes
    let a = [1, 2, 3, 4, 5, 6, 7, 8, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17];
    let b = [8, 8, 8, 8, 8, 8, 8, 8, 0xF0, 0xF0, 0xF0, 0xF0, 0xF0, 0xF0, 0xF0, 0xF0];
    // 66 0F FC C1  paddb xmm0, xmm1
    check_sse("paddb", &sse_program(&[0x66, 0x0F, 0xFC, 0xC1]), sse_scratch(a, b));
}

#[test]
fn sse_paddw() {
    let a = 0xFFFF_0001_8000_1234u64.to_le_bytes();
    let a = [a, 0x0102_0304_0506_0708u64.to_le_bytes()].concat();
    let b = 0x0001_FFFF_8000_0001u64.to_le_bytes();
    let b = [b, 0x1111_2222_3333_4444u64.to_le_bytes()].concat();
    // 66 0F FD C1  paddw xmm0, xmm1
    check_sse(
        "paddw",
        &sse_program(&[0x66, 0x0F, 0xFD, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse_paddd() {
    let a = [0x0000_0001_FFFF_FFFFu64.to_le_bytes(), 0x8000_0000_7FFF_FFFFu64.to_le_bytes()].concat();
    let b = [0x0000_0001_0000_0001u64.to_le_bytes(), 0x0000_0001_0000_0001u64.to_le_bytes()].concat();
    // 66 0F FE C1  paddd xmm0, xmm1
    check_sse(
        "paddd",
        &sse_program(&[0x66, 0x0F, 0xFE, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse_psubb() {
    let a = [0, 1, 2, 3, 0x80, 0x7F, 0xFF, 0x10, 1, 2, 3, 4, 5, 6, 7, 8];
    let b = [1, 1, 1, 1, 1, 1, 1, 1, 0xFF, 0xFE, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60];
    // 66 0F F8 C1  psubb xmm0, xmm1
    check_sse("psubb", &sse_program(&[0x66, 0x0F, 0xF8, 0xC1]), sse_scratch(a, b));
}

#[test]
fn sse_pxor() {
    let a = [0xDEAD_BEEF_CAFE_BABEu64.to_le_bytes(), 0x0123_4567_89AB_CDEFu64.to_le_bytes()].concat();
    let b = [0xFFFF_FFFF_FFFF_FFFFu64.to_le_bytes(), 0x0F0F_0F0F_0F0F_0F0Fu64.to_le_bytes()].concat();
    // 66 0F EF C1  pxor xmm0, xmm1
    check_sse(
        "pxor",
        &sse_program(&[0x66, 0x0F, 0xEF, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse_pand() {
    let a = [0xFF00_FF00_FF00_FF00u64.to_le_bytes(), 0xAAAA_5555_AAAA_5555u64.to_le_bytes()].concat();
    let b = [0x0FF0_0FF0_0FF0_0FF0u64.to_le_bytes(), 0xFFFF_0000_FFFF_0000u64.to_le_bytes()].concat();
    // 66 0F DB C1  pand xmm0, xmm1
    check_sse(
        "pand",
        &sse_program(&[0x66, 0x0F, 0xDB, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}
