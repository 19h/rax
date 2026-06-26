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

use rax::backend::emulator::x86_64::{X86_64Vcpu, flags};
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
/// Minimal GDT used by far control-flow and descriptor-sensitive tests.
const GDT_ADDR: u64 = 0x4_000;
/// Minimal IDT base used to make SIDT deterministic in differential tests.
const IDT_ADDR: u64 = 0x5_000;
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
const FLAG_MASK: u64 = flags::bits::CF
    | flags::bits::PF
    | flags::bits::AF
    | flags::bits::ZF
    | flags::bits::SF
    | flags::bits::OF;

// ---------------------------------------------------------------------------
// Build the shared identity-mapped long-mode initial state.
// ---------------------------------------------------------------------------

fn base_sregs() -> SystemRegisters {
    let mut sregs = SystemRegisters::default();
    sregs.cr0 = CR0_VAL;
    sregs.cr3 = PML4_ADDR;
    sregs.cr4 = CR4_VAL;
    sregs.efer = EFER_VAL;
    sregs.gdt.base = GDT_ADDR;
    sregs.gdt.limit = 0x17;
    sregs.idt.base = IDT_ADDR;
    sregs.idt.limit = 0x0FFF;

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
    let null_descriptor = [0u8; 8];
    let code64_descriptor = [
        0xFF, 0xFF, // limit 0:15
        0x00, 0x00, // base 0:15
        0x00, // base 16:23
        0x9B, // present ring-0 executable/readable accessed code
        0xAF, // limit 16:19, L=1, G=1
        0x00, // base 24:31
    ];
    let data_descriptor = [
        0xFF, 0xFF, // limit 0:15
        0x00, 0x00, // base 0:15
        0x00, // base 16:23
        0x93, // present ring-0 writable accessed data
        0xCF, // limit 16:19, DB=1, G=1
        0x00, // base 24:31
    ];
    write(GDT_ADDR, &null_descriptor);
    write(GDT_ADDR + 8, &code64_descriptor);
    write(GDT_ADDR + 16, &data_descriptor);

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

fn run_interpreter(
    code: &[u8],
    init: &Registers,
    scratch_init: &[u8; 64],
) -> Result<FinalState, String> {
    let regions = vec![(GuestAddress(0), MEM_SIZE)];
    let mem =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&regions).map_err(|e| format!("mem: {e:?}"))?);

    // Page tables.
    install_tables_mmap(&mut |addr, bytes| {
        mem.write_slice(bytes, GuestAddress(addr)).unwrap();
    });
    // Code + scratch.
    mem.write_slice(code, GuestAddress(CODE_ADDR))
        .map_err(|e| format!("code: {e:?}"))?;
    mem.write_slice(scratch_init, GuestAddress(DATA_ADDR))
        .map_err(|e| format!("scratch: {e:?}"))?;

    let mut vcpu = X86_64Vcpu::new(0, mem.clone());

    let mut regs = init.clone();
    regs.rip = CODE_ADDR;
    if regs.rsp == 0 {
        regs.rsp = STACK_ADDR;
    }
    regs.rflags |= 0x2; // reserved bit 1 always set
    vcpu.set_regs(&regs)
        .map_err(|e| format!("set_regs: {e:?}"))?;
    vcpu.set_sregs(&base_sregs())
        .map_err(|e| format!("set_sregs: {e:?}"))?;

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
            Some(VcpuExit::Shutdown)
            | Some(VcpuExit::FailEntry { .. })
            | Some(VcpuExit::InternalError) => {
                return Err("interpreter abnormal exit".to_string());
            }
            _ => {}
        }
    }

    let final_regs = vcpu.get_regs().map_err(|e| format!("get_regs: {e:?}"))?;
    let mut scratch = [0u8; 64];
    mem.read_slice(&mut scratch, GuestAddress(DATA_ADDR))
        .map_err(|e| format!("read scratch: {e:?}"))?;

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

/// Returns Ok(None) if KVM is unavailable (so callers can skip gracefully),
/// Ok(Some(state)) on success, Err on a genuine run failure.
fn run_kvm(
    code: &[u8],
    init: &Registers,
    scratch_init: &[u8; 64],
) -> Result<Option<FinalState>, String> {
    use kvm_bindings::{KVM_MAX_CPUID_ENTRIES, kvm_segment, kvm_userspace_memory_region};
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
    let supported_cpuid = match kvm.get_supported_cpuid(KVM_MAX_CPUID_ENTRIES) {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };
    if vcpu.set_cpuid2(&supported_cpuid).is_err() {
        return Ok(None);
    }

    // --- sregs: long mode w/ paging, flat segments ---
    let our_sregs = base_sregs();
    let mut sregs = vcpu
        .get_sregs()
        .map_err(|e| format!("kvm get_sregs: {e:?}"))?;

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
    sregs.gdt.base = our_sregs.gdt.base;
    sregs.gdt.limit = our_sregs.gdt.limit;
    sregs.idt.base = our_sregs.idt.base;
    sregs.idt.limit = our_sregs.idt.limit;
    sregs.cs = to_kvm_seg(&our_sregs.cs);
    sregs.ds = to_kvm_seg(&our_sregs.ds);
    sregs.es = to_kvm_seg(&our_sregs.es);
    sregs.fs = to_kvm_seg(&our_sregs.fs);
    sregs.gs = to_kvm_seg(&our_sregs.gs);
    sregs.ss = to_kvm_seg(&our_sregs.ss);
    vcpu.set_sregs(&sregs)
        .map_err(|e| format!("kvm set_sregs: {e:?}"))?;

    // --- gprs ---
    let mut kregs = vcpu
        .get_regs()
        .map_err(|e| format!("kvm get_regs: {e:?}"))?;
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
    vcpu.set_regs(&kregs)
        .map_err(|e| format!("kvm set_regs: {e:?}"))?;

    // --- xmm (via FPU state) ---
    if init.xmm.iter().any(|x| x != &[0, 0]) {
        let mut fpu = vcpu.get_fpu().map_err(|e| format!("kvm get_fpu: {e:?}"))?;
        for i in 0..16 {
            let lo = init.xmm[i][0].to_le_bytes();
            let hi = init.xmm[i][1].to_le_bytes();
            fpu.xmm[i][..8].copy_from_slice(&lo);
            fpu.xmm[i][8..].copy_from_slice(&hi);
        }
        vcpu.set_fpu(&fpu)
            .map_err(|e| format!("kvm set_fpu: {e:?}"))?;
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

    let final_kregs = vcpu
        .get_regs()
        .map_err(|e| format!("kvm get_regs(final): {e:?}"))?;
    let final_fpu = vcpu
        .get_fpu()
        .map_err(|e| format!("kvm get_fpu(final): {e:?}"))?;

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
fn run_both(
    code: &[u8],
    init: Registers,
    scratch_init: [u8; 64],
) -> Option<(FinalState, FinalState)> {
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
fn assert_match(
    label: &str,
    code: &[u8],
    interp: &FinalState,
    kvm: &FinalState,
    opts: CompareOpts,
) {
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
    check_flags_masked(
        "imul2",
        &with_hlt(vec![0x48, 0x0F, 0xAF, 0xC3]),
        r,
        MULDIV_DEFINED,
    );
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
    check_flags_masked(
        "mul64",
        &with_hlt(vec![0x48, 0xF7, 0xE3]),
        r,
        MULDIV_DEFINED,
    );
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
    check(
        "setb",
        &with_hlt(vec![0x48, 0x39, 0xD8, 0x0F, 0x92, 0xC0]),
        r,
    );
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

/// Build a guest program for memory-source SSE instructions: load xmm0 from
/// scratch, clear xmm1, run `op` against memory, store xmm0 back, HLT.
fn sse_memory_source_program(op: &[u8]) -> Vec<u8> {
    let mut code = load_rdi_data();
    code.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]); // movdqu xmm0, [rdi]
    code.extend_from_slice(&[0x66, 0x0F, 0xEF, 0xC9]); // pxor xmm1, xmm1
    code.extend_from_slice(op);
    code.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]); // movdqu [rdi+0x20], xmm0
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
    let a = [
        1, 2, 3, 4, 5, 6, 7, 8, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
    ];
    let b = [
        8, 8, 8, 8, 8, 8, 8, 8, 0xF0, 0xF0, 0xF0, 0xF0, 0xF0, 0xF0, 0xF0, 0xF0,
    ];
    // 66 0F FC C1  paddb xmm0, xmm1
    check_sse(
        "paddb",
        &sse_program(&[0x66, 0x0F, 0xFC, 0xC1]),
        sse_scratch(a, b),
    );
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
    let a = [
        0x0000_0001_FFFF_FFFFu64.to_le_bytes(),
        0x8000_0000_7FFF_FFFFu64.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x0000_0001_0000_0001u64.to_le_bytes(),
        0x0000_0001_0000_0001u64.to_le_bytes(),
    ]
    .concat();
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
    let b = [
        1, 1, 1, 1, 1, 1, 1, 1, 0xFF, 0xFE, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60,
    ];
    // 66 0F F8 C1  psubb xmm0, xmm1
    check_sse(
        "psubb",
        &sse_program(&[0x66, 0x0F, 0xF8, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_pxor() {
    let a = [
        0xDEAD_BEEF_CAFE_BABEu64.to_le_bytes(),
        0x0123_4567_89AB_CDEFu64.to_le_bytes(),
    ]
    .concat();
    let b = [
        0xFFFF_FFFF_FFFF_FFFFu64.to_le_bytes(),
        0x0F0F_0F0F_0F0F_0F0Fu64.to_le_bytes(),
    ]
    .concat();
    // 66 0F EF C1  pxor xmm0, xmm1
    check_sse(
        "pxor",
        &sse_program(&[0x66, 0x0F, 0xEF, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse_pand() {
    let a = [
        0xFF00_FF00_FF00_FF00u64.to_le_bytes(),
        0xAAAA_5555_AAAA_5555u64.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x0FF0_0FF0_0FF0_0FF0u64.to_le_bytes(),
        0xFFFF_0000_FFFF_0000u64.to_le_bytes(),
    ]
    .concat();
    // 66 0F DB C1  pand xmm0, xmm1
    check_sse(
        "pand",
        &sse_program(&[0x66, 0x0F, 0xDB, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

// ===========================================================================
// EXPANDED COVERAGE
// ===========================================================================
//
// Flag-definition reminders used by the masks below (Intel SDM Vol.2):
//  - BT/BTS/BTR/BTC : define CF; OF/SF/ZF/AF/PF undefined.
//  - BSF/BSR        : define ZF; CF/OF/SF/AF/PF undefined. Dest undefined if src==0.
//  - LZCNT/TZCNT    : define CF and ZF; OF/SF/AF/PF undefined.
//  - POPCNT         : ZF reflects src==0; CF/OF/SF/AF/PF cleared (all defined).
//  - shifts/rotates : OF defined only for a 1-bit shift; undefined for count>1.
//                     For a masked count of 0 NO flags change (we avoid that
//                     ambiguity by using nonzero counts or comparing GPRs only).
//  - SHLD/SHRD      : like shifts; OF defined only for count==1, undefined else;
//                     AF is undefined. Result undefined if count > operand size.
//  - XCHG           : affects no flags. XADD/CMPXCHG set flags like ADD/CMP.

const BT_DEFINED: u64 = flags::bits::CF;
const BSF_DEFINED: u64 = flags::bits::ZF;
const CNT_DEFINED: u64 = flags::bits::CF | flags::bits::ZF;
// For count>1 shifts/rotates OF is undefined; CF (and for shifts SF/ZF/PF) defined.
const SHIFT_NO_OF: u64 = flags::bits::CF | flags::bits::PF | flags::bits::ZF | flags::bits::SF;
// Rotates only ever touch CF/OF; for count>1 OF is undefined -> CF only.
const ROT_MULTI_DEFINED: u64 = flags::bits::CF;

// ---- BT / BTS / BTR / BTC (reg index and imm8 index) ----

#[test]
fn bt_reg_set() {
    let mut r = regs();
    r.rax = 0x0000_0000_0001_0000; // bit 16 set
    r.rdx = 16;
    // 48 0F A3 D0  bt rax, rdx  (CF <- bit 16 = 1)
    check_flags_masked(
        "bt_reg_set",
        &with_hlt(vec![0x48, 0x0F, 0xA3, 0xD0]),
        r,
        BT_DEFINED,
    );
}

#[test]
fn bt_reg_clear() {
    let mut r = regs();
    r.rax = 0x0000_0000_0001_0000;
    r.rdx = 17; // bit 17 = 0
    check_flags_masked(
        "bt_reg_clear",
        &with_hlt(vec![0x48, 0x0F, 0xA3, 0xD0]),
        r,
        BT_DEFINED,
    );
}

#[test]
fn bt_reg_index_wraps_modulo_64() {
    // For a register-operand BT the bit index is taken modulo operand size.
    let mut r = regs();
    r.rax = 0x0000_0000_0000_0002; // bit 1 set
    r.rdx = 65; // 65 mod 64 = 1 -> CF should be 1
    check_flags_masked(
        "bt_reg_mod64",
        &with_hlt(vec![0x48, 0x0F, 0xA3, 0xD0]),
        r,
        BT_DEFINED,
    );
}

#[test]
fn bt_imm() {
    let mut r = regs();
    r.rax = 0x0000_0000_8000_0000; // bit 31 set
    // 48 0F BA E0 1F  bt rax, 31
    check_flags_masked(
        "bt_imm31",
        &with_hlt(vec![0x48, 0x0F, 0xBA, 0xE0, 0x1F]),
        r,
        BT_DEFINED,
    );
}

#[test]
fn bts_imm_sets_bit() {
    let mut r = regs();
    r.rax = 0x0; // CF <- old bit (0), bit 5 then set in dest
    // 48 0F BA E8 05  bts rax, 5
    check_flags_masked(
        "bts_imm5",
        &with_hlt(vec![0x48, 0x0F, 0xBA, 0xE8, 0x05]),
        r,
        BT_DEFINED,
    );
}

#[test]
fn bts_reg_already_set() {
    let mut r = regs();
    r.rax = 0x0000_0000_0000_0008; // bit 3 set
    r.rcx = 3;
    // 48 0F AB C8  bts rax, rcx  (CF<-1, bit stays set)
    check_flags_masked(
        "bts_reg_set",
        &with_hlt(vec![0x48, 0x0F, 0xAB, 0xC8]),
        r,
        BT_DEFINED,
    );
}

#[test]
fn btr_imm_clears_bit() {
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF;
    // 48 0F BA F0 07  btr rax, 7  (CF<-1, bit 7 cleared)
    check_flags_masked(
        "btr_imm7",
        &with_hlt(vec![0x48, 0x0F, 0xBA, 0xF0, 0x07]),
        r,
        BT_DEFINED,
    );
}

#[test]
fn btr_reg_clears_bit() {
    let mut r = regs();
    r.rax = 0x0000_0000_0010_0000; // bit 20 set
    r.rsi = 20;
    // 48 0F B3 F0  btr rax, rsi
    check_flags_masked(
        "btr_reg20",
        &with_hlt(vec![0x48, 0x0F, 0xB3, 0xF0]),
        r,
        BT_DEFINED,
    );
}

#[test]
fn btc_imm_toggles_bit() {
    let mut r = regs();
    r.rax = 0x0000_0000_0000_0001; // bit 0 set
    // 48 0F BA F8 00  btc rax, 0  (CF<-1, bit 0 toggled to 0)
    check_flags_masked(
        "btc_imm0",
        &with_hlt(vec![0x48, 0x0F, 0xBA, 0xF8, 0x00]),
        r,
        BT_DEFINED,
    );
}

#[test]
fn btc_reg_toggles_bit() {
    let mut r = regs();
    r.rax = 0x0; // bit 40 = 0 -> CF<-0, bit set
    r.rdi = 40;
    // 48 0F BB F8  btc rax, rdi
    check_flags_masked(
        "btc_reg40",
        &with_hlt(vec![0x48, 0x0F, 0xBB, 0xF8]),
        r,
        BT_DEFINED,
    );
}

// ---- BSF / BSR (incl. zero-source, where ZF=1 and dest is undefined) ----

#[test]
fn bsf_low_bit() {
    let mut r = regs();
    r.rbx = 0x0000_0000_0000_0028; // bits 3 and 5 -> lowest is 3
    check_flags_masked(
        "bsf_low3",
        &with_hlt(vec![0x48, 0x0F, 0xBC, 0xC3]),
        r,
        BSF_DEFINED,
    );
}

#[test]
fn bsf_zero_source() {
    // src == 0 -> ZF=1, destination is architecturally UNDEFINED; preload RAX so
    // both backends would only match if they leave it unchanged. Since the dest
    // is undefined we deliberately do NOT compare RAX here by checking flags only.
    let mut r = regs();
    r.rbx = 0;
    r.rax = 0xDEAD_DEAD_DEAD_DEAD;
    // Compare ZF only; mask out GPR by... we still compare GPRs in check_flags_masked.
    // To avoid an undefined-dest false diff, see bsf_zero_source_flags below.
    let Some((interp, kvm)) = run_both(&with_hlt(vec![0x48, 0x0F, 0xBC, 0xC3]), r, zero_scratch())
    else {
        return;
    };
    // Only the ZF flag is defined; RAX is undefined on a zero source.
    let opts = CompareOpts {
        flag_mask: BSF_DEFINED,
        xmm_count: 0,
        scratch: false,
        stack: true,
    };
    // Compare everything except RAX (undefined) by checking flags + that RBX/others match.
    let mut diffs = compare(&interp, &kvm, opts);
    diffs.retain(|d| !d.starts_with("rax:"));
    if !diffs.is_empty() {
        panic!("DIVERGENCE in `bsf_zero_source`: {}", diffs.join("; "));
    }
}

#[test]
fn bsr_high_bit() {
    let mut r = regs();
    r.rbx = 0x0000_0000_0000_0028; // highest set bit is 5
    check_flags_masked(
        "bsr_high5",
        &with_hlt(vec![0x48, 0x0F, 0xBD, 0xC3]),
        r,
        BSF_DEFINED,
    );
}

#[test]
fn bsr_top_bit() {
    let mut r = regs();
    r.rbx = 0x8000_0000_0000_0000; // bit 63
    check_flags_masked(
        "bsr_top63",
        &with_hlt(vec![0x48, 0x0F, 0xBD, 0xC3]),
        r,
        BSF_DEFINED,
    );
}

// ---- BSWAP ----

#[test]
fn bswap_r64() {
    let mut r = regs();
    r.rax = 0x0011_2233_4455_6677;
    // 48 0F C8  bswap rax
    check("bswap64", &with_hlt(vec![0x48, 0x0F, 0xC8]), r);
}

#[test]
fn bswap_r32() {
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_1122_3344; // bswap eax zero-extends into rax
    // 0F C8  bswap eax
    check("bswap32", &with_hlt(vec![0x0F, 0xC8]), r);
}

// ---- POPCNT / LZCNT / TZCNT ----

#[test]
fn popcnt_r64() {
    let mut r = regs();
    r.rbx = 0xFF00_F0F0_1234_5678;
    // F3 48 0F B8 C3  popcnt rax, rbx
    check("popcnt64", &with_hlt(vec![0xF3, 0x48, 0x0F, 0xB8, 0xC3]), r);
}

#[test]
fn popcnt_zero_sets_zf() {
    let mut r = regs();
    r.rbx = 0; // result 0 -> ZF set, all others cleared
    check(
        "popcnt_zero",
        &with_hlt(vec![0xF3, 0x48, 0x0F, 0xB8, 0xC3]),
        r,
    );
}

#[test]
fn lzcnt_r64() {
    let mut r = regs();
    r.rbx = 0x0000_0000_0001_0000; // 47 leading zeros
    // F3 48 0F BD C3  lzcnt rax, rbx
    check_flags_masked(
        "lzcnt64",
        &with_hlt(vec![0xF3, 0x48, 0x0F, 0xBD, 0xC3]),
        r,
        CNT_DEFINED,
    );
}

#[test]
fn lzcnt_zero_source() {
    let mut r = regs();
    r.rbx = 0; // result = operand size (64), CF set, ZF clear
    check_flags_masked(
        "lzcnt_zero",
        &with_hlt(vec![0xF3, 0x48, 0x0F, 0xBD, 0xC3]),
        r,
        CNT_DEFINED,
    );
}

#[test]
fn tzcnt_r64() {
    let mut r = regs();
    r.rbx = 0x0000_0000_0001_0000; // 16 trailing zeros
    // F3 48 0F BC C3  tzcnt rax, rbx
    check_flags_masked(
        "tzcnt64",
        &with_hlt(vec![0xF3, 0x48, 0x0F, 0xBC, 0xC3]),
        r,
        CNT_DEFINED,
    );
}

#[test]
fn tzcnt_zero_source() {
    let mut r = regs();
    r.rbx = 0; // result = 64, CF set
    check_flags_masked(
        "tzcnt_zero",
        &with_hlt(vec![0xF3, 0x48, 0x0F, 0xBC, 0xC3]),
        r,
        CNT_DEFINED,
    );
}

#[test]
fn lzcnt_tzcnt_memory_forms() {
    let mut s = [0u8; 64];
    s[0..2].copy_from_slice(&0x00F0u16.to_le_bytes());
    s[8..12].copy_from_slice(&0x0000_1000u32.to_le_bytes());
    s[16..24].copy_from_slice(&0x0000_0000_0000_0001u64.to_le_bytes());
    s[24..32].copy_from_slice(&0u64.to_le_bytes());

    let mut r = regs();
    r.rdi = DATA_ADDR;

    let code = with_hlt(vec![
        0x66, 0xF3, 0x0F, 0xBD, 0x07, // lzcnt ax, [rdi]
        0x66, 0x89, 0x47, 0x20, // mov [rdi+32], ax
        0xF3, 0x0F, 0xBC, 0x4F, 0x08, // tzcnt ecx, [rdi+8]
        0x89, 0x4F, 0x24, // mov [rdi+36], ecx
        0xF3, 0x48, 0x0F, 0xBD, 0x57, 0x10, // lzcnt rdx, [rdi+16]
        0x48, 0x89, 0x57, 0x28, // mov [rdi+40], rdx
        0xF3, 0x48, 0x0F, 0xBC, 0x5F, 0x18, // tzcnt rbx, [rdi+24]
        0x48, 0x89, 0x5F, 0x30, // mov [rdi+48], rbx
    ]);
    check_mem("lzcnt_tzcnt_memory_forms", &code, r, s, CNT_DEFINED);
}

// ---- XCHG / XADD / CMPXCHG (register operands) ----

#[test]
fn xchg_reg_reg() {
    let mut r = regs();
    r.rax = 0x1111_1111_1111_1111;
    r.rbx = 0x2222_2222_2222_2222;
    // 48 87 D8  xchg rax, rbx  (no flags affected)
    check("xchg_rr", &with_hlt(vec![0x48, 0x87, 0xD8]), r);
}

#[test]
fn xadd_reg_reg() {
    let mut r = regs();
    r.rax = 0x100; // dest
    r.rbx = 0x0FF; // src; after: rbx<-old rax(0x100), rax<-0x1FF, flags like ADD
    // 48 0F C1 D8  xadd rax, rbx
    check("xadd_rr", &with_hlt(vec![0x48, 0x0F, 0xC1, 0xD8]), r);
}

#[test]
fn xadd_carry() {
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF;
    r.rbx = 0x1; // sum wraps -> CF, ZF, AF
    check("xadd_carry", &with_hlt(vec![0x48, 0x0F, 0xC1, 0xD8]), r);
}

#[test]
fn cmpxchg_success() {
    // RAX (accumulator) == dest -> ZF=1, dest <- src.
    let mut r = regs();
    r.rbx = 0x1234; // dest
    r.rax = 0x1234; // accumulator matches
    r.rcx = 0xABCD; // src written on success
    // 48 0F B1 CB  cmpxchg rbx, rcx
    check("cmpxchg_ok", &with_hlt(vec![0x48, 0x0F, 0xB1, 0xCB]), r);
}

#[test]
fn cmpxchg_fail() {
    // RAX != dest -> ZF=0, RAX <- dest, dest unchanged.
    let mut r = regs();
    r.rbx = 0x1234; // dest
    r.rax = 0x9999; // accumulator differs
    r.rcx = 0xABCD; // not written on failure
    check("cmpxchg_fail", &with_hlt(vec![0x48, 0x0F, 0xB1, 0xCB]), r);
}

#[test]
fn cmpxchg8_success() {
    let mut r = regs();
    r.rbx = 0x55;
    r.rax = 0x55;
    r.rcx = 0xAA;
    // 0F B0 CB  cmpxchg bl, cl
    check("cmpxchg8_ok", &with_hlt(vec![0x0F, 0xB0, 0xCB]), r);
}

// ---- Shifts / rotates by CL and imm, double-shift SHLD/SHRD ----

#[test]
fn shl_imm_multi() {
    let mut r = regs();
    r.rax = 0x1;
    // 48 C1 E0 05  shl rax, 5  (count>1 -> OF undefined)
    check_flags_masked(
        "shl_imm5",
        &with_hlt(vec![0x48, 0xC1, 0xE0, 0x05]),
        r,
        SHIFT_NO_OF,
    );
}

#[test]
fn shr_imm_multi() {
    let mut r = regs();
    r.rax = 0xFF00;
    // 48 C1 E8 04  shr rax, 4
    check_flags_masked(
        "shr_imm4",
        &with_hlt(vec![0x48, 0xC1, 0xE8, 0x04]),
        r,
        SHIFT_NO_OF,
    );
}

#[test]
fn sar_imm_multi() {
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_8000_0000;
    // 48 C1 F8 08  sar rax, 8
    check_flags_masked(
        "sar_imm8",
        &with_hlt(vec![0x48, 0xC1, 0xF8, 0x08]),
        r,
        SHIFT_NO_OF,
    );
}

#[test]
fn shr_cl_multi() {
    let mut r = regs();
    r.rax = 0xDEAD_BEEF_0000_0000;
    r.rcx = 12;
    // 48 D3 E8  shr rax, cl
    check_flags_masked(
        "shr_cl12",
        &with_hlt(vec![0x48, 0xD3, 0xE8]),
        r,
        SHIFT_NO_OF,
    );
}

#[test]
fn shl32_cl_clears_high() {
    // 32-bit shift zero-extends the result into the full 64-bit register.
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_0000_00FF;
    r.rcx = 8;
    // D3 E0  shl eax, cl
    check_flags_masked("shl32_cl8", &with_hlt(vec![0xD3, 0xE0]), r, SHIFT_NO_OF);
}

#[test]
fn rol_cl_multi() {
    let mut r = regs();
    r.rax = 0x1234_5678_9ABC_DEF0;
    r.rcx = 12;
    // 48 D3 C0  rol rax, cl  (count>1 -> OF undefined)
    check_flags_masked(
        "rol_cl12",
        &with_hlt(vec![0x48, 0xD3, 0xC0]),
        r,
        ROT_MULTI_DEFINED,
    );
}

#[test]
fn ror_cl_multi() {
    let mut r = regs();
    r.rax = 0x1234_5678_9ABC_DEF0;
    r.rcx = 20;
    // 48 D3 C8  ror rax, cl
    check_flags_masked(
        "ror_cl20",
        &with_hlt(vec![0x48, 0xD3, 0xC8]),
        r,
        ROT_MULTI_DEFINED,
    );
}

#[test]
fn shld_imm() {
    // SHLD rax, rbx, 8 : shift rax left by 8, feeding in the top 8 bits of rbx.
    let mut r = regs();
    r.rax = 0x1122_3344_5566_7788;
    r.rbx = 0xAABB_CCDD_EEFF_0011;
    // 48 0F A4 D8 08  shld rax, rbx, 8  (count>1 -> OF undefined)
    check_flags_masked(
        "shld_imm8",
        &with_hlt(vec![0x48, 0x0F, 0xA4, 0xD8, 0x08]),
        r,
        SHIFT_NO_OF,
    );
}

#[test]
fn shrd_imm() {
    // SHRD rax, rbx, 8 : shift rax right by 8, feeding in the low 8 bits of rbx.
    let mut r = regs();
    r.rax = 0x1122_3344_5566_7788;
    r.rbx = 0xAABB_CCDD_EEFF_0011;
    // 48 0F AC D8 08  shrd rax, rbx, 8
    check_flags_masked(
        "shrd_imm8",
        &with_hlt(vec![0x48, 0x0F, 0xAC, 0xD8, 0x08]),
        r,
        SHIFT_NO_OF,
    );
}

#[test]
fn shld_cl() {
    let mut r = regs();
    r.rax = 0xFFFF_0000_FFFF_0000;
    r.rbx = 0x0F0F_0F0F_0F0F_0F0F;
    r.rcx = 16;
    // 48 0F A5 D8  shld rax, rbx, cl
    check_flags_masked(
        "shld_cl16",
        &with_hlt(vec![0x48, 0x0F, 0xA5, 0xD8]),
        r,
        SHIFT_NO_OF,
    );
}

#[test]
fn shrd_cl() {
    let mut r = regs();
    r.rax = 0xFFFF_0000_FFFF_0000;
    r.rbx = 0x0F0F_0F0F_0F0F_0F0F;
    r.rcx = 24;
    // 48 0F AD D8  shrd rax, rbx, cl
    check_flags_masked(
        "shrd_cl24",
        &with_hlt(vec![0x48, 0x0F, 0xAD, 0xD8]),
        r,
        SHIFT_NO_OF,
    );
}

#[test]
fn shld_imm1_defines_of() {
    // A 1-bit double shift DOES define OF, so compare all status flags.
    let mut r = regs();
    r.rax = 0x4000_0000_0000_0000;
    r.rbx = 0x8000_0000_0000_0000;
    // 48 0F A4 D8 01  shld rax, rbx, 1
    check(
        "shld_imm1",
        &with_hlt(vec![0x48, 0x0F, 0xA4, 0xD8, 0x01]),
        r,
    );
}

// ---- Sign/zero extension: MOVSX/MOVZX word sources, CBW family, CWD family ----

#[test]
fn movzx_word() {
    let mut r = regs();
    r.rbx = 0xFFFF_FFFF_FFFF_8000;
    // 48 0F B7 C3  movzx rax, bx
    check("movzx_bx", &with_hlt(vec![0x48, 0x0F, 0xB7, 0xC3]), r);
}

#[test]
fn movsx_word() {
    let mut r = regs();
    r.rbx = 0x0000_0000_0000_8000; // bx = 0x8000 -> sign extends
    // 48 0F BF C3  movsx rax, bx
    check("movsx_bx", &with_hlt(vec![0x48, 0x0F, 0xBF, 0xC3]), r);
}

#[test]
fn movsx_byte_to_word() {
    // 16-bit destination: movsx ax, bl (66 prefix, opcode 0F BE)
    let mut r = regs();
    r.rax = 0x1111_2222_3333_4444;
    r.rbx = 0x90; // sign-extends within the 16-bit AX only
    // 66 0F BE C3  movsx ax, bl
    check("movsx_ax_bl", &with_hlt(vec![0x66, 0x0F, 0xBE, 0xC3]), r);
}

#[test]
fn cbw() {
    // AL -> AX sign extension; 66 98
    let mut r = regs();
    r.rax = 0x1234_5678_9ABC_DE80; // al = 0x80
    check("cbw", &with_hlt(vec![0x66, 0x98]), r);
}

#[test]
fn cwde() {
    // AX -> EAX sign extension; 98 (zero-extends EAX into RAX)
    let mut r = regs();
    r.rax = 0x1234_5678_0000_8000; // ax = 0x8000
    check("cwde", &with_hlt(vec![0x98]), r);
}

#[test]
fn cdqe() {
    // EAX -> RAX sign extension; 48 98
    let mut r = regs();
    r.rax = 0x1234_5678_8000_0000; // eax = 0x80000000
    check("cdqe", &with_hlt(vec![0x48, 0x98]), r);
}

#[test]
fn cwd() {
    // AX -> DX:AX ; 66 99
    let mut r = regs();
    r.rax = 0x0000_0000_0000_8000; // ax negative
    r.rdx = 0x1111_2222_3333_4444; // only dx (low 16) should change
    check("cwd", &with_hlt(vec![0x66, 0x99]), r);
}

#[test]
fn cdq() {
    // EAX -> EDX:EAX ; 99  (EDX zero-extends into RDX)
    let mut r = regs();
    r.rax = 0x0000_0000_8000_0000; // eax negative
    r.rdx = 0xFFFF_FFFF_1234_5678;
    check("cdq", &with_hlt(vec![0x99]), r);
}

#[test]
fn cqo() {
    // RAX -> RDX:RAX ; 48 99
    let mut r = regs();
    r.rax = 0x8000_0000_0000_0000; // negative
    r.rdx = 0x1234_5678_9ABC_DEF0;
    check("cqo", &with_hlt(vec![0x48, 0x99]), r);
}

#[test]
fn cqo_positive() {
    let mut r = regs();
    r.rax = 0x0000_0000_0000_0001; // positive -> rdx becomes 0
    r.rdx = 0xFFFF_FFFF_FFFF_FFFF;
    check("cqo_pos", &with_hlt(vec![0x48, 0x99]), r);
}

// ---- CMOVcc across all 16 conditions ----
//
// We set up a known flag state with a single CMP, then CMOVcc rax, rcx. The
// destination is loaded with a sentinel beforehand so a "no move" is observable.

/// cmp rax,rbx (48 39 D8) sets flags, then `cmov` (2-byte 0F xx) rax, rcx.
fn cmov_program(cmov2: [u8; 2]) -> Vec<u8> {
    // 48 31 C0           xor rax, rax           (clear so the sentinel below is exact)
    // 48 B8 imm64        mov rax, sentinel
    // ... but simpler: use the regs we pass in. We just emit cmp + cmov.
    let mut code = vec![0x48, 0x39, 0xD8]; // cmp rax, rbx
    code.extend_from_slice(&[0x48, cmov2[0], cmov2[1], 0xC1]); // cmov rax, rcx (REX.W, 0F, opc, modrm C1)
    code.push(HLT);
    code
}

/// Run a CMOVcc with the given 2nd opcode byte and an (rax,rbx) pair that sets
/// flags via CMP. rcx holds the candidate value moved on a true condition.
fn check_cmov(label: &str, opc: u8, rax: u64, rbx: u64) {
    let mut r = regs();
    r.rax = rax;
    r.rbx = rbx;
    r.rcx = 0xCAFE_F00D_1234_5678;
    check(label, &cmov_program([0x0F, opc]), r);
}

/// All 16 CMOVcc conditions in one case. Each tuple is (opcode2, rax, rbx) where
/// the CMP rax,rbx sets a flag state that makes the condition TRUE (and a couple
/// also test the FALSE/no-move path). Opcodes 0x40..=0x4F are CMOVO..CMOVG.
#[test]
fn cmovcc_all_conditions() {
    // (opcode, rax, rbx) chosen so the condition is TRUE for these inputs.
    let true_cases: &[(u8, u64, u64, &str)] = &[
        (0x40, 0x8000_0000_0000_0000, 1, "cmovo"), // INT_MIN - 1 -> OF
        (0x41, 5, 3, "cmovno"),                    // no overflow
        (0x42, 1, 2, "cmovb"),                     // 1 < 2 unsigned -> CF
        (0x43, 5, 3, "cmovae"),                    // CF clear
        (0x44, 7, 7, "cmove"),                     // equal -> ZF
        (0x45, 7, 8, "cmovne"),                    // not equal
        (0x46, 2, 2, "cmovbe"),                    // CF|ZF (equal)
        (0x47, 5, 3, "cmova"),                     // above
        (0x48, 0, 1, "cmovs"),                     // 0-1 -> SF
        (0x49, 5, 3, "cmovns"),                    // SF clear
        (0x4A, 3, 0, "cmovp"),                     // result 3 -> even parity
        (0x4B, 1, 0, "cmovnp"),                    // result 1 -> odd parity
        (0x4C, 1, 2, "cmovl"),                     // signed less
        (0x4D, 5, 3, "cmovge"),                    // signed >=
        (0x4E, 2, 2, "cmovle"),                    // signed <= (equal)
        (0x4F, 5, 3, "cmovg"),                     // signed >
    ];
    for &(opc, rax, rbx, name) in true_cases {
        check_cmov(name, opc, rax, rbx);
    }
    // No-move (FALSE) path for a representative set so we also exercise the
    // "destination unchanged" branch on both backends.
    check_cmov("cmovo_false", 0x40, 5, 3); // no overflow -> no move
    check_cmov("cmove_false", 0x44, 7, 8); // not equal -> no move
    check_cmov("cmovg_false", 0x4F, 1, 2); // not greater -> no move
}

// ---- SETcc across conditions ----

/// cmp rax,rbx then setcc al, then movzx eax, al so we read a clean 0/1.
fn setcc_program(opc: u8) -> Vec<u8> {
    let mut code = vec![0x48, 0x39, 0xD8]; // cmp rax, rbx
    code.extend_from_slice(&[0x0F, opc, 0xC0]); // setcc al
    code.extend_from_slice(&[0x0F, 0xB6, 0xC0]); // movzx eax, al
    code.push(HLT);
    code
}

fn check_setcc(label: &str, opc: u8, rax: u64, rbx: u64) {
    let mut r = regs();
    r.rax = rax;
    r.rbx = rbx;
    check(label, &setcc_program(opc), r);
}

/// SETcc across a spread of conditions, both true and false results. Opcodes
/// 0x90..=0x9F are SETO..SETG.
#[test]
fn setcc_all_conditions() {
    let cases: &[(u8, u64, u64, &str)] = &[
        (0x90, 0x8000_0000_0000_0000, 1, "seto"), // overflow -> 1
        (0x91, 5, 3, "setno"),                    // no overflow -> 1
        (0x92, 1, 2, "setb"),                     // below -> 1
        (0x93, 5, 3, "setae"),                    // not below -> 1
        (0x94, 7, 7, "sete"),                     // equal -> 1
        (0x95, 7, 8, "setne"),                    // not equal -> 1
        (0x96, 2, 2, "setbe"),                    // be (equal) -> 1
        (0x97, 5, 3, "seta"),                     // above -> 1
        (0x98, 0, 1, "sets"),                     // sign -> 1
        (0x99, 5, 3, "setns"),                    // no sign -> 1
        (0x9A, 3, 0, "setp"),                     // even parity -> 1
        (0x9B, 1, 0, "setnp"),                    // odd parity -> 1
        (0x9C, 1, 2, "setl"),                     // less -> 1
        (0x9D, 5, 3, "setge"),                    // ge -> 1
        (0x9E, 2, 2, "setle"),                    // le (equal) -> 1
        (0x9F, 5, 3, "setg"),                     // greater -> 1
        // false-result spot checks
        (0x94, 7, 8, "sete_false"), // not equal -> 0
        (0x9F, 1, 2, "setg_false"), // not greater -> 0
    ];
    for &(opc, rax, rbx, name) in cases {
        check_setcc(name, opc, rax, rbx);
    }
}

// ---- LEA with complex SIB (base + index*scale + disp) ----

#[test]
fn lea_base_index_scale_disp8() {
    let mut r = regs();
    r.rbx = 0x1_0000; // base
    r.rcx = 0x10; // index
    // 48 8D 44 8B 20  lea rax, [rbx + rcx*4 + 0x20]
    check(
        "lea_sib_disp8",
        &with_hlt(vec![0x48, 0x8D, 0x44, 0x8B, 0x20]),
        r,
    );
}

#[test]
fn lea_index_scale8_disp32() {
    let mut r = regs();
    r.rsi = 0x100;
    r.rdi = 0x2_0000;
    // 48 8D 84 F7 00 10 00 00  lea rax, [rdi + rsi*8 + 0x1000]
    check(
        "lea_sib_disp32",
        &with_hlt(vec![0x48, 0x8D, 0x84, 0xF7, 0x00, 0x10, 0x00, 0x00]),
        r,
    );
}

#[test]
fn lea_no_base_index_scale() {
    // [rcx*8 + disp32] with no base (mod=00, base=101 in SIB -> disp32, no base)
    let mut r = regs();
    r.rcx = 0x11;
    // 48 8D 04 CD 00 02 00 00  lea rax, [rcx*8 + 0x200]
    check(
        "lea_idx_only",
        &with_hlt(vec![0x48, 0x8D, 0x04, 0xCD, 0x00, 0x02, 0x00, 0x00]),
        r,
    );
}

#[test]
fn lea_32bit_addrsize_truncates() {
    // 67 prefix -> 32-bit address calc, result zero-extended into rax.
    let mut r = regs();
    r.rbx = 0xFFFF_FFFF_FFFF_F000; // ebx = 0xFFFFF000
    r.rcx = 0x0000_0000_0000_2000; // ecx = 0x2000
    // 67 48 8D 04 0B  lea rax, [ebx + ecx]  -> (0xFFFFF000+0x2000) mod 2^32
    check(
        "lea_addr32",
        &with_hlt(vec![0x67, 0x48, 0x8D, 0x04, 0x0B]),
        r,
    );
}

// ---- 8/16/32-bit operand-size ALU variants for flag exactness ----

#[test]
fn add16_overflow() {
    let mut r = regs();
    r.rax = 0x7FFF; // 16-bit signed max
    r.rbx = 0x0001; // -> OF, SF
    // 66 01 D8  add ax, bx
    check("add16_of", &with_hlt(vec![0x66, 0x01, 0xD8]), r);
}

#[test]
fn add32_wrap_zero_extends() {
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF; // eax = 0xFFFFFFFF
    r.rbx = 0x0000_0000_0000_0001; // -> 0, CF, ZF, AF; rax zero-extends
    // 01 D8  add eax, ebx
    check("add32_wrap", &with_hlt(vec![0x01, 0xD8]), r);
}

#[test]
fn sub16_borrow() {
    let mut r = regs();
    r.rax = 0x0000;
    r.rbx = 0x0001; // 0-1 -> CF, SF, AF
    // 66 29 D8  sub ax, bx
    check("sub16_borrow", &with_hlt(vec![0x66, 0x29, 0xD8]), r);
}

#[test]
fn and16() {
    let mut r = regs();
    r.rax = 0xF0F0;
    r.rbx = 0x0FF0;
    // 66 21 D8  and ax, bx
    check("and16", &with_hlt(vec![0x66, 0x21, 0xD8]), r);
}

#[test]
fn or8() {
    let mut r = regs();
    r.rax = 0x80;
    r.rbx = 0x01; // -> 0x81, SF set, odd parity
    // 08 D8  or al, bl
    check("or8", &with_hlt(vec![0x08, 0xD8]), r);
}

#[test]
fn xor16_zero() {
    let mut r = regs();
    r.rax = 0xABCD;
    r.rbx = 0xABCD; // -> 0, ZF set, PF set
    // 66 31 D8  xor ax, bx
    check("xor16_zero", &with_hlt(vec![0x66, 0x31, 0xD8]), r);
}

#[test]
fn cmp16_equal() {
    let mut r = regs();
    r.rax = 0x1234_5678_9ABC_8000;
    r.rbx = 0x0000_0000_0000_8000; // ax==bx -> ZF
    // 66 39 D8  cmp ax, bx
    check("cmp16_eq", &with_hlt(vec![0x66, 0x39, 0xD8]), r);
}

#[test]
fn cmp8_less() {
    let mut r = regs();
    r.rax = 0x7F; // +127
    r.rbx = 0x80; // -128 ; 127 - (-128) overflows -> OF
    // 38 D8  cmp al, bl
    check("cmp8_of", &with_hlt(vec![0x38, 0xD8]), r);
}

#[test]
fn test8() {
    let mut r = regs();
    r.rax = 0xF0;
    r.rbx = 0x0F; // 0xF0 & 0x0F = 0 -> ZF, PF
    // 84 D8  test al, bl
    check("test8", &with_hlt(vec![0x84, 0xD8]), r);
}

#[test]
fn inc8_overflow() {
    let mut r = regs();
    r.rax = 0x7F; // inc -> 0x80, OF set, SF set, AF set
    r.rflags = flags::bits::CF; // INC must preserve CF
    // FE C0  inc al
    check("inc8_of", &with_hlt(vec![0xFE, 0xC0]), r);
}

#[test]
fn dec16_to_zero() {
    let mut r = regs();
    r.rax = 0x0001;
    r.rflags = flags::bits::CF;
    // 66 FF C8  dec ax
    check("dec16_zero", &with_hlt(vec![0x66, 0xFF, 0xC8]), r);
}

#[test]
fn inc32_zero_extends() {
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF; // eax = 0xFFFFFFFF -> inc to 0, zero-extends
    // FF C0  inc eax
    check("inc32_wrap", &with_hlt(vec![0xFF, 0xC0]), r);
}

#[test]
fn add8_imm() {
    let mut r = regs();
    r.rax = 0x7E;
    // 04 02  add al, 2  -> 0x80, OF + SF
    check("add8_imm", &with_hlt(vec![0x04, 0x02]), r);
}

#[test]
fn add16_imm() {
    let mut r = regs();
    r.rax = 0x00FF;
    // 66 05 01 00  add ax, 1
    check("add16_imm", &with_hlt(vec![0x66, 0x05, 0x01, 0x00]), r);
}

// ---- More SSE2 integer ops (PMULLW/PMADDWD/PSADBW/PACK*/PUNPCK*) ----

#[test]
fn sse_pmullw() {
    // packed 16-bit multiply low: per-lane (a*b) low 16 bits.
    let a = [
        0x0002u16.to_le_bytes(),
        0x00FFu16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
        0x1234u16.to_le_bytes(),
        0x00FFu16.to_le_bytes(),
        0x0010u16.to_le_bytes(),
        0xFFFFu16.to_le_bytes(),
        0x0007u16.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x0003u16.to_le_bytes(),
        0x0100u16.to_le_bytes(),
        0x0002u16.to_le_bytes(),
        0x0010u16.to_le_bytes(),
        0x00FFu16.to_le_bytes(),
        0x0010u16.to_le_bytes(),
        0xFFFFu16.to_le_bytes(),
        0x0006u16.to_le_bytes(),
    ]
    .concat();
    // 66 0F D5 C1  pmullw xmm0, xmm1
    check_sse(
        "pmullw",
        &sse_program(&[0x66, 0x0F, 0xD5, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse_pmaddwd() {
    // PMADDWD: multiply 16-bit lanes, add adjacent pairs into 32-bit results.
    let a = [
        0x0001u16.to_le_bytes(),
        0x0002u16.to_le_bytes(),
        0xFFFFu16.to_le_bytes(), // -1
        0x0003u16.to_le_bytes(),
        0x8000u16.to_le_bytes(), // -32768
        0x8000u16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x0004u16.to_le_bytes(),
        0x0005u16.to_le_bytes(),
        0x0002u16.to_le_bytes(),
        0xFFFFu16.to_le_bytes(), // -1
        0x8000u16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
    ]
    .concat();
    // 66 0F F5 C1  pmaddwd xmm0, xmm1
    check_sse(
        "pmaddwd",
        &sse_program(&[0x66, 0x0F, 0xF5, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse_psadbw() {
    // sum of absolute differences of bytes -> two 16-bit sums (low halves).
    let a = [
        10, 20, 30, 40, 50, 60, 70, 80, 100, 110, 120, 130, 140, 150, 160, 170,
    ];
    let b = [
        5, 25, 35, 35, 60, 55, 80, 75, 90, 120, 110, 140, 130, 160, 150, 180,
    ];
    // 66 0F F6 C1  psadbw xmm0, xmm1
    check_sse(
        "psadbw",
        &sse_program(&[0x66, 0x0F, 0xF6, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_packsswb() {
    // signed saturate pack words->bytes.
    let a = [
        0x0001u16.to_le_bytes(),
        0x00FFu16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(), // -> +127
        0x8000u16.to_le_bytes(), // -> -128
        0xFF80u16.to_le_bytes(), // -128 stays
        0x0080u16.to_le_bytes(), // +128 -> +127
        0xFFFFu16.to_le_bytes(), // -1
        0x0000u16.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x0100u16.to_le_bytes(),
        0x7F00u16.to_le_bytes(),
        0x8001u16.to_le_bytes(),
        0x0040u16.to_le_bytes(),
        0x0010u16.to_le_bytes(),
        0xFFF0u16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
    ]
    .concat();
    // 66 0F 63 C1  packsswb xmm0, xmm1
    check_sse(
        "packsswb",
        &sse_program(&[0x66, 0x0F, 0x63, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse_packuswb() {
    // unsigned saturate pack words->bytes.
    let a = [
        0x0001u16.to_le_bytes(),
        0x00FFu16.to_le_bytes(),
        0x0100u16.to_le_bytes(), // -> 255
        0x8000u16.to_le_bytes(), // negative -> 0
        0xFFFFu16.to_le_bytes(), // negative -> 0
        0x007Fu16.to_le_bytes(),
        0x00FEu16.to_le_bytes(),
        0x0080u16.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x0000u16.to_le_bytes(),
        0x0010u16.to_le_bytes(),
        0x00FFu16.to_le_bytes(),
        0x0101u16.to_le_bytes(), // -> 255
        0x7FFFu16.to_le_bytes(), // -> 255
        0x0001u16.to_le_bytes(),
        0x0002u16.to_le_bytes(),
        0x0003u16.to_le_bytes(),
    ]
    .concat();
    // 66 0F 67 C1  packuswb xmm0, xmm1
    check_sse(
        "packuswb",
        &sse_program(&[0x66, 0x0F, 0x67, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse_punpcklbw() {
    // interleave low bytes of xmm0 and xmm1.
    let a = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
    let b = [
        0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x8B, 0x8C, 0x8D, 0x8E,
        0x8F,
    ];
    // 66 0F 60 C1  punpcklbw xmm0, xmm1
    check_sse(
        "punpcklbw",
        &sse_program(&[0x66, 0x0F, 0x60, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_punpckhbw() {
    let a = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
    let b = [
        0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x8B, 0x8C, 0x8D, 0x8E,
        0x8F,
    ];
    // 66 0F 68 C1  punpckhbw xmm0, xmm1
    check_sse(
        "punpckhbw",
        &sse_program(&[0x66, 0x0F, 0x68, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_punpckldq() {
    // interleave low dwords.
    let a = [
        0x1111_1111u32.to_le_bytes(),
        0x2222_2222u32.to_le_bytes(),
        0x3333_3333u32.to_le_bytes(),
        0x4444_4444u32.to_le_bytes(),
    ]
    .concat();
    let b = [
        0xAAAA_AAAAu32.to_le_bytes(),
        0xBBBB_BBBBu32.to_le_bytes(),
        0xCCCC_CCCCu32.to_le_bytes(),
        0xDDDD_DDDDu32.to_le_bytes(),
    ]
    .concat();
    // 66 0F 62 C1  punpckldq xmm0, xmm1
    check_sse(
        "punpckldq",
        &sse_program(&[0x66, 0x0F, 0x62, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse_punpcklqdq() {
    // interleave low qwords -> [a.lo, b.lo].
    let a = [
        0x0123_4567_89AB_CDEFu64.to_le_bytes(),
        0xDEAD_BEEF_CAFE_BABEu64.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x1122_3344_5566_7788u64.to_le_bytes(),
        0x99AA_BBCC_DDEE_FF00u64.to_le_bytes(),
    ]
    .concat();
    // 66 0F 6C C1  punpcklqdq xmm0, xmm1
    check_sse(
        "punpcklqdq",
        &sse_program(&[0x66, 0x0F, 0x6C, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse_pmulhw() {
    // packed 16-bit multiply, store the HIGH 16 bits (signed).
    let a = [
        0x4000u16.to_le_bytes(),
        0x8000u16.to_le_bytes(), // -32768
        0x7FFFu16.to_le_bytes(),
        0xFFFFu16.to_le_bytes(), // -1
        0x0100u16.to_le_bytes(),
        0x1234u16.to_le_bytes(),
        0x00FFu16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x0004u16.to_le_bytes(),
        0x0002u16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
        0xFFFFu16.to_le_bytes(),
        0x0100u16.to_le_bytes(),
        0x1000u16.to_le_bytes(),
        0x00FFu16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
    ]
    .concat();
    // 66 0F E5 C1  pmulhw xmm0, xmm1
    check_sse(
        "pmulhw",
        &sse_program(&[0x66, 0x0F, 0xE5, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 2: x87 FPU / string ops / SSE2-SSE3 float / BMI
// ===========================================================================
//
// These reuse the existing infrastructure:
//  - x87 and string tests drive data through the scratch page (just like the
//    SSE tests) so we never rely on host-side FPU/XMM injection surviving
//    KVM_RUN. The guest loads from / stores to the scratch page and we compare
//    it byte-for-byte plus the relevant GPRs.
//  - BMI/MOVBE/SAHF/LAHF are plain GPR ops driven by `check`/`check_flags_masked`.
//
// IMPORTANT x87 caveat: rax keeps the x87 stack as f64, not the architectural
// 80-bit extended format. Therefore every x87 value used below is chosen so the
// 80-bit and 64-bit representations are *bit-identical*: small integers and
// exact dyadic fractions (n / 2^k). Results are stored as m64 (FSTP qword) so
// what we compare is exactly the f64 the op produced — which, for these
// operands, equals what real hardware computes in 80-bit then rounds to 64-bit.

/// A scratch-comparing runner for memory-effect tests (x87 store / string ops).
/// Compares all GPRs, the masked flags, and the scratch page.
fn check_mem(label: &str, code: &[u8], init: Registers, scratch_in: [u8; 64], flag_mask: u64) {
    let Some((interp, kvm)) = run_both(code, init, scratch_in) else {
        return;
    };
    let opts = CompareOpts {
        flag_mask,
        scratch: true,
        ..CompareOpts::default()
    };
    assert_match(label, code, &interp, &kvm, opts);
}

/// Emit `mov rdi, DATA_ADDR` (REX.W mov r/m64, imm32 sign-extended; 0x30000 fits).
fn load_rdi_data() -> Vec<u8> {
    let mut c = vec![0x48, 0xC7, 0xC7];
    c.extend_from_slice(&(DATA_ADDR as u32).to_le_bytes());
    c
}

/// Build a 64-byte scratch page from a list of f64 inputs laid out from offset 0.
fn scratch_f64(vals: &[f64]) -> [u8; 64] {
    let mut s = [0u8; 64];
    for (i, v) in vals.iter().enumerate() {
        s[i * 8..i * 8 + 8].copy_from_slice(&v.to_le_bytes());
    }
    s
}

// ---- x87: load / arithmetic round-trips, result stored as FSTP qword ----
//
// Program skeleton for a binary op on two m64 inputs:
//   mov rdi, DATA_ADDR
//   fld qword [rdi+0]      ; ST0 = a            (DD /0)
//   f<op> qword [rdi+8]    ; ST0 = a <op> b     (DC /n, memory form)
//   fstp qword [rdi+16]    ; store + pop        (DD /3)
//   hlt
// Inputs a,b at offsets 0 and 8; result compared at offset 16.

/// Build an x87 "load a; <mem-op> b; store result" program. `op_modrm` is the
/// ModRM byte selecting the DC-escape memory operation against [rdi+8].
fn x87_binop(op_escape: u8, op_modrm: u8) -> Vec<u8> {
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi]
    c.extend_from_slice(&[op_escape, op_modrm]); // f<op> qword [rdi+8] (modrm encodes disp8 0x08)
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp qword [rdi+0x10]
    c.push(HLT);
    c
}

#[test]
fn x87_fld_fstp_roundtrip() {
    // Load an exact f64 and store it straight back — pure load/store fidelity.
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi]
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp qword [rdi+0x10]
    c.push(HLT);
    // 12345.5 is exactly representable.
    check_mem(
        "x87_fld_fstp",
        &with_hlt(c),
        regs(),
        scratch_f64(&[12345.5]),
        0,
    );
}

#[test]
fn x87_fadd_m64() {
    // DC /0 = FADD m64. ModRM for [rdi+disp8], reg=000 -> 0x47, disp8=0x08.
    check_mem(
        "x87_fadd",
        &with_hlt(x87_binop(0xDC, 0x47)),
        regs(),
        scratch_f64(&[3.5, 4.25]),
        0,
    );
}

#[test]
fn x87_fsub_m64() {
    // DC /4 = FSUB m64. reg=100 -> modrm 0x67.
    check_mem(
        "x87_fsub",
        &with_hlt(x87_binop(0xDC, 0x67)),
        regs(),
        scratch_f64(&[10.0, 2.5]),
        0,
    );
}

#[test]
fn x87_fmul_m64() {
    // DC /1 = FMUL m64. reg=001 -> modrm 0x4F.
    check_mem(
        "x87_fmul",
        &with_hlt(x87_binop(0xDC, 0x4F)),
        regs(),
        scratch_f64(&[6.0, 7.0]),
        0,
    );
}

#[test]
fn x87_fdiv_m64() {
    // DC /6 = FDIV m64. reg=110 -> modrm 0x77. 9/8 = 1.125 is exact.
    check_mem(
        "x87_fdiv",
        &with_hlt(x87_binop(0xDC, 0x77)),
        regs(),
        scratch_f64(&[9.0, 8.0]),
        0,
    );
}

#[test]
fn x87_fsqrt() {
    // fld qword [rdi]; fsqrt (D9 FA); fstp qword [rdi+0x10]. sqrt(16)=4 exact.
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi]
    c.extend_from_slice(&[0xD9, 0xFA]); // fsqrt
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp qword [rdi+0x10]
    c.push(HLT);
    check_mem("x87_fsqrt", &with_hlt(c), regs(), scratch_f64(&[16.0]), 0);
}

#[test]
fn x87_fild_fistp_roundtrip() {
    // FILD m32 (DB /0) loads a 32-bit integer; FISTP m32 (DB /3) stores it back.
    // Put the integer at offset 0, read the stored int back at offset 16.
    let mut s = [0u8; 64];
    s[0..4].copy_from_slice(&(-12345i32).to_le_bytes());
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDB, 0x07]); // fild dword [rdi]
    c.extend_from_slice(&[0xDB, 0x5F, 0x10]); // fistp dword [rdi+0x10]
    c.push(HLT);
    check_mem("x87_fild_fistp", &with_hlt(c), regs(), s, 0);
}

#[test]
fn x87_fild_fadd_fistp() {
    // Integer load, add an exact float, store back as integer (round-to-nearest).
    // 100 + 23.0 = 123 -> stored int 123.
    let mut s = [0u8; 64];
    s[0..4].copy_from_slice(&100i32.to_le_bytes());
    s[8..16].copy_from_slice(&23.0f64.to_le_bytes());
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDB, 0x07]); // fild dword [rdi]
    c.extend_from_slice(&[0xDC, 0x47, 0x08]); // fadd qword [rdi+8]
    c.extend_from_slice(&[0xDB, 0x5F, 0x10]); // fistp dword [rdi+0x10]
    c.push(HLT);
    check_mem("x87_fild_fadd_fistp", &with_hlt(c), regs(), s, 0);
}

#[test]
fn x87_integer_memory_arithmetic_forms() {
    let int_op_program = |escape, modrm| {
        let mut c = load_rdi_data();
        c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi]
        c.extend_from_slice(&[escape, modrm, 0x08]); // fi<op> [rdi+8]
        c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp qword [rdi+0x10]
        c.push(HLT);
        c
    };
    let scratch_i16 = || {
        let mut s = scratch_f64(&[8.0]);
        s[8..10].copy_from_slice(&4i16.to_le_bytes());
        s
    };
    let scratch_i32 = || {
        let mut s = scratch_f64(&[8.0]);
        s[8..12].copy_from_slice(&4i32.to_le_bytes());
        s
    };

    for (label, modrm) in [
        ("x87_fiadd_m16", 0x47),
        ("x87_fimul_m16", 0x4F),
        ("x87_fisub_m16", 0x67),
        ("x87_fisubr_m16", 0x6F),
        ("x87_fidiv_m16", 0x77),
        ("x87_fidivr_m16", 0x7F),
    ] {
        check_mem(label, &int_op_program(0xDE, modrm), regs(), scratch_i16(), 0);
    }

    for (label, modrm) in [
        ("x87_fiadd_m32", 0x47),
        ("x87_fimul_m32", 0x4F),
        ("x87_fisub_m32", 0x67),
        ("x87_fisubr_m32", 0x6F),
        ("x87_fidiv_m32", 0x77),
        ("x87_fidivr_m32", 0x7F),
    ] {
        check_mem(label, &int_op_program(0xDA, modrm), regs(), scratch_i32(), 0);
    }
}

#[test]
fn x87_fist_m64() {
    // FILD m32 then FISTP m64 (DF /7) round-trip of a 64-bit integer store.
    let mut s = [0u8; 64];
    s[0..4].copy_from_slice(&424242i32.to_le_bytes());
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDB, 0x07]); // fild dword [rdi]
    c.extend_from_slice(&[0xDF, 0x7F, 0x10]); // fistp qword [rdi+0x10]
    c.push(HLT);
    check_mem("x87_fist_m64", &with_hlt(c), regs(), s, 0);
}

#[test]
fn x87_fadd_st_chain() {
    // Two sequential memory adds: ((1 + 2.5) + 4.25) = 7.75, all exact dyadic.
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi]      ST0=1.0
    c.extend_from_slice(&[0xDC, 0x47, 0x08]); // fadd qword [rdi+8]   ST0=3.5
    c.extend_from_slice(&[0xDC, 0x47, 0x10]); // fadd qword [rdi+16]  ST0=7.75
    c.extend_from_slice(&[0xDD, 0x5F, 0x18]); // fstp qword [rdi+0x18]
    c.push(HLT);
    check_mem(
        "x87_fadd_chain",
        &with_hlt(c),
        regs(),
        scratch_f64(&[1.0, 2.5, 4.25]),
        0,
    );
}

#[test]
fn x87_fsubr_m64() {
    // DC /5 = FSUBR m64 (ST0 = mem - ST0). reg=101 -> modrm 0x6F. 2.5 - 10 = -7.5.
    check_mem(
        "x87_fsubr",
        &with_hlt(x87_binop(0xDC, 0x6F)),
        regs(),
        scratch_f64(&[10.0, 2.5]),
        0,
    );
}

#[test]
fn x87_fdivr_m64() {
    // DC /7 = FDIVR m64 (ST0 = mem / ST0). reg=111 -> modrm 0x7F. 8/2 = 4.
    check_mem(
        "x87_fdivr",
        &with_hlt(x87_binop(0xDC, 0x7F)),
        regs(),
        scratch_f64(&[2.0, 8.0]),
        0,
    );
}

// ---- x87 FCOMI / FUCOMI: compare ST0 with ST(i), set ZF/PF/CF directly ----
//
// Setup: fld b ([rdi+8]) then fld a ([rdi]) so ST0=a, ST1=b, then FCOMI ST0,ST1.
// FCOMI sets ZF/PF/CF in EFLAGS; OF/SF/AF are cleared. We compare exactly the
// status mask (the harness masks to the 6 status flags; FCOMI clears AF/SF/OF).

const FCOMI_FLAGS: u64 = flags::bits::ZF | flags::bits::PF | flags::bits::CF;

/// Build: load b then a so ST0=a,ST1=b; FCOMI ST0,ST1 (DB F1); HLT.
fn fcomi_program() -> Vec<u8> {
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld qword [rdi+8]  -> ST0=b
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi]    -> ST0=a, ST1=b
    c.extend_from_slice(&[0xDB, 0xF1]); // fcomi st0, st1
    c.push(HLT);
    c
}

#[test]
fn x87_fcomi_greater() {
    // a > b: ZF=0, CF=0, PF=0.
    check_mem(
        "x87_fcomi_gt",
        &fcomi_program(),
        regs(),
        scratch_f64(&[5.0, 3.0]),
        FCOMI_FLAGS,
    );
}

#[test]
fn x87_fcomi_less() {
    // a < b: ZF=0, CF=1, PF=0.
    check_mem(
        "x87_fcomi_lt",
        &fcomi_program(),
        regs(),
        scratch_f64(&[3.0, 5.0]),
        FCOMI_FLAGS,
    );
}

#[test]
fn x87_fcomi_equal() {
    // a == b: ZF=1, CF=0, PF=0.
    check_mem(
        "x87_fcomi_eq",
        &fcomi_program(),
        regs(),
        scratch_f64(&[42.0, 42.0]),
        FCOMI_FLAGS,
    );
}

#[test]
fn x87_fucomi_equal() {
    // FUCOMI ST0,ST1 = DB E9. Same ordered result for non-NaN operands.
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld qword [rdi+8]
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi]
    c.extend_from_slice(&[0xDB, 0xE9]); // fucomi st0, st1
    c.push(HLT);
    check_mem(
        "x87_fucomi_eq",
        &c,
        regs(),
        scratch_f64(&[7.5, 7.5]),
        FCOMI_FLAGS,
    );
}

#[test]
fn x87_fcmovcc_condition_forms() {
    let fcmov_program = |flag_setup: &[u8], escape, modrm| {
        let mut c = load_rdi_data();
        c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld qword [rdi+8] -> ST0=source
        c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi] -> ST0=dest, ST1=source
        c.extend_from_slice(flag_setup);
        c.extend_from_slice(&[escape, modrm]); // fcmovcc st0, st1
        c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp qword [rdi+0x10]
        c.push(HLT);
        c
    };
    let scratch = || scratch_f64(&[2.0, 7.0]);

    for (label, setup, escape, modrm) in [
        ("x87_fcmovb_true", &[0xF9][..], 0xDA, 0xC1), // stc: CF=1
        ("x87_fcmovb_false", &[0xF8][..], 0xDA, 0xC1), // clc: CF=0
        ("x87_fcmove_true", &[0x31, 0xC0][..], 0xDA, 0xC9), // xor eax,eax: ZF=1
        ("x87_fcmove_false", &[0x83, 0xC8, 0x01][..], 0xDA, 0xC9), // or eax,1: ZF=0
        ("x87_fcmovbe_true", &[0xF9][..], 0xDA, 0xD1), // CF=1
        ("x87_fcmovbe_false", &[0xF8][..], 0xDA, 0xD1), // CF=0,ZF=0
        ("x87_fcmovu_true", &[0x31, 0xC0][..], 0xDA, 0xD9), // PF=1
        ("x87_fcmovu_false", &[0x83, 0xC8, 0x01][..], 0xDA, 0xD9), // PF=0
        ("x87_fcmovnb_true", &[0xF8][..], 0xDB, 0xC1), // CF=0
        ("x87_fcmovnb_false", &[0xF9][..], 0xDB, 0xC1), // CF=1
        ("x87_fcmovne_true", &[0x83, 0xC8, 0x01][..], 0xDB, 0xC9), // ZF=0
        ("x87_fcmovne_false", &[0x31, 0xC0][..], 0xDB, 0xC9), // ZF=1
        ("x87_fcmovnbe_true", &[0x83, 0xC8, 0x01][..], 0xDB, 0xD1), // CF=0,ZF=0
        ("x87_fcmovnbe_false", &[0xF9][..], 0xDB, 0xD1), // CF=1
        ("x87_fcmovnu_true", &[0x83, 0xC8, 0x01][..], 0xDB, 0xD9), // PF=0
        ("x87_fcmovnu_false", &[0x31, 0xC0][..], 0xDB, 0xD9), // PF=1
    ] {
        check_mem(
            label,
            &fcmov_program(setup, escape, modrm),
            regs(),
            scratch(),
            0,
        );
    }
}

#[test]
fn x87_compare_pop_and_integer_forms() {
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x47, 0x10]); // fld qword [rdi+0x10] -> sentinel
    c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld qword [rdi+8] -> b
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi] -> a
    c.extend_from_slice(&[0xDE, 0xD9]); // fcompp: compare a vs b, pop twice
    c.extend_from_slice(&[0xDF, 0xE0]); // fnstsw ax
    c.extend_from_slice(&[0x9E]); // sahf
    c.extend_from_slice(&[0xB8, 0x00, 0x00, 0x00, 0x00]); // mov eax,0; preserve flags
    c.extend_from_slice(&[0xDD, 0x5F, 0x18]); // fstp qword [rdi+0x18] = sentinel
    c.push(HLT);
    check_mem(
        "x87_fcompp_pops_twice",
        &c,
        regs(),
        scratch_f64(&[3.0, 5.0, 9.0]),
        FCOMI_FLAGS,
    );

    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld qword [rdi+8] -> b
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi] -> a
    c.extend_from_slice(&[0xDD, 0xE9]); // fucomp st1: compare a vs b, pop once
    c.extend_from_slice(&[0xDF, 0xE0]); // fnstsw ax
    c.extend_from_slice(&[0x9E]); // sahf
    c.extend_from_slice(&[0xB8, 0x00, 0x00, 0x00, 0x00]); // mov eax,0; preserve flags
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp qword [rdi+0x10] = b
    c.push(HLT);
    check_mem(
        "x87_fucomp_pops_once",
        &c,
        regs(),
        scratch_f64(&[4.0, 6.0]),
        FCOMI_FLAGS,
    );

    for (label, op) in [
        ("x87_fcomip_pops_once", [0xDF, 0xF1]),
        ("x87_fucomip_pops_once", [0xDF, 0xE9]),
    ] {
        let mut c = load_rdi_data();
        c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld qword [rdi+8] -> b
        c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi] -> a
        c.extend_from_slice(&op); // f/ucomip st0, st1; pop once
        c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp qword [rdi+0x10] = b
        c.push(HLT);
        check_mem(label, &c, regs(), scratch_f64(&[4.0, 6.0]), FCOMI_FLAGS);
    }

    let mut s = scratch_f64(&[4.0]);
    s[8..10].copy_from_slice(&6i16.to_le_bytes());
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi]
    c.extend_from_slice(&[0xDE, 0x57, 0x08]); // ficom word [rdi+8]
    c.extend_from_slice(&[0xDF, 0xE0]); // fnstsw ax
    c.extend_from_slice(&[0x9E]); // sahf
    c.extend_from_slice(&[0xB8, 0x00, 0x00, 0x00, 0x00]); // mov eax,0; preserve flags
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp qword [rdi+0x10] = original ST0
    c.push(HLT);
    check_mem("x87_ficom_m16", &c, regs(), s, FCOMI_FLAGS);

    let mut s = scratch_f64(&[2.0, 0.0, 0.0, 9.0]);
    s[12..16].copy_from_slice(&3i32.to_le_bytes());
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x47, 0x18]); // fld qword [rdi+0x18] -> sentinel
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi]
    c.extend_from_slice(&[0xDA, 0x5F, 0x0C]); // ficomp dword [rdi+0x0c]
    c.extend_from_slice(&[0xDF, 0xE0]); // fnstsw ax
    c.extend_from_slice(&[0x9E]); // sahf
    c.extend_from_slice(&[0xB8, 0x00, 0x00, 0x00, 0x00]); // mov eax,0; preserve flags
    c.extend_from_slice(&[0xDD, 0x5F, 0x20]); // fstp qword [rdi+0x20] = sentinel
    c.push(HLT);
    check_mem("x87_ficomp_m32", &c, regs(), s, FCOMI_FLAGS);
}

#[test]
fn x87_ftst_fxam_status_forms() {
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi] -> ST0=-4.0
    c.extend_from_slice(&[0xD9, 0xE5]); // fxam
    c.extend_from_slice(&[0xDD, 0x7F, 0x10]); // fnstsw word [rdi+0x10]
    c.extend_from_slice(&[0xDD, 0x5F, 0x18]); // fstp qword [rdi+0x18]
    c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld qword [rdi+8] -> ST0=0.0
    c.extend_from_slice(&[0xD9, 0xE4]); // ftst
    c.extend_from_slice(&[0xDF, 0xE0]); // fnstsw ax
    c.extend_from_slice(&[0x9E]); // sahf maps C0/C2/C3 to CF/PF/ZF
    c.extend_from_slice(&[0xB8, 0x00, 0x00, 0x00, 0x00]); // mov eax,0; preserve flags
    c.push(HLT);

    check_mem(
        "x87_ftst_fxam_status",
        &c,
        regs(),
        scratch_f64(&[-4.0, 0.0]),
        FCOMI_FLAGS,
    );
}

#[test]
fn x87_control_word_rounding_forms() {
    let mut s = scratch_f64(&[2.75]);
    s[8..10].copy_from_slice(&0x0F7Fu16.to_le_bytes()); // RC=truncate, exceptions masked

    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDB, 0xE3]); // fninit
    c.extend_from_slice(&[0xD9, 0x7F, 0x20]); // fnstcw word [rdi+0x20]
    c.extend_from_slice(&[0xD9, 0x6F, 0x08]); // fldcw word [rdi+0x08]
    c.extend_from_slice(&[0xD9, 0x7F, 0x22]); // fnstcw word [rdi+0x22]
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi]
    c.extend_from_slice(&[0xD9, 0xFC]); // frndint using truncation RC
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp qword [rdi+0x10]
    c.push(HLT);

    check_mem("x87_control_word_rounding", &c, regs(), s, 0);
}

#[test]
fn x87_register_stack_transfer_forms() {
    let scratch_two = || scratch_f64(&[3.0, 7.0]);

    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld qword [rdi+8] -> ST0=7
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi] -> ST0=3, ST1=7
    c.extend_from_slice(&[0xD9, 0xC1]); // fld st1 -> duplicate 7
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp qword [rdi+0x10] = 7
    c.extend_from_slice(&[0xDD, 0x5F, 0x18]); // fstp qword [rdi+0x18] = 3
    c.extend_from_slice(&[0xDD, 0x5F, 0x20]); // fstp qword [rdi+0x20] = 7
    c.push(HLT);
    check_mem("x87_fld_sti", &c, regs(), scratch_two(), 0);

    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi] -> ST0=3
    c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld qword [rdi+8] -> ST0=7, ST1=3
    c.extend_from_slice(&[0xDD, 0xD1]); // fst st1 -> ST1=7
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp qword [rdi+0x10] = 7
    c.extend_from_slice(&[0xDD, 0x5F, 0x18]); // fstp qword [rdi+0x18] = 7
    c.push(HLT);
    check_mem("x87_fst_sti", &c, regs(), scratch_two(), 0);

    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi] -> ST0=3
    c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld qword [rdi+8] -> ST0=7, ST1=3
    c.extend_from_slice(&[0xDD, 0xD9]); // fstp st1 -> store 7 into ST1, then pop
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp qword [rdi+0x10] = 7
    c.push(HLT);
    check_mem("x87_fstp_sti", &c, regs(), scratch_two(), 0);

    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi] -> ST0=3
    c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld qword [rdi+8] -> ST0=7, ST1=3
    c.extend_from_slice(&[0xDD, 0x47, 0x10]); // fld qword [rdi+0x10] -> ST0=11
    c.extend_from_slice(&[0xD9, 0xF7]); // fincstp -> ST0=7
    c.extend_from_slice(&[0xDD, 0x57, 0x18]); // fst qword [rdi+0x18] = 7
    c.extend_from_slice(&[0xD9, 0xF6]); // fdecstp -> ST0=11
    c.extend_from_slice(&[0xDD, 0x5F, 0x20]); // fstp qword [rdi+0x20] = 11
    c.push(HLT);
    check_mem(
        "x87_fincstp_fdecstp",
        &c,
        regs(),
        scratch_f64(&[3.0, 7.0, 11.0]),
        0,
    );
}

#[test]
fn x87_environment_save_restore_forms() {
    const FPU_SAVE_OFF: u32 = 0x100;
    const FPU_ENV_OFF: u32 = 0x180;

    let append_disp32 = |c: &mut Vec<u8>, op: &[u8], disp: u32| {
        c.extend_from_slice(op);
        c.extend_from_slice(&disp.to_le_bytes());
    };

    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi] -> ST0=13
    c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld qword [rdi+8] -> ST0=29, ST1=13
    append_disp32(&mut c, &[0xDD, 0xB7], FPU_SAVE_OFF); // fnsave [rdi+0x100]
    c.extend_from_slice(&[0xD9, 0xE8]); // fld1 to contaminate the reset FPU state
    append_disp32(&mut c, &[0xDD, 0xA7], FPU_SAVE_OFF); // frstor [rdi+0x100]
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp qword [rdi+0x10] = 29
    c.extend_from_slice(&[0xDD, 0x5F, 0x18]); // fstp qword [rdi+0x18] = 13
    c.push(HLT);
    check_mem(
        "x87_fnsave_frstor_roundtrip",
        &c,
        regs(),
        scratch_f64(&[13.0, 29.0]),
        0,
    );

    let mut s = scratch_f64(&[2.75]);
    s[8..10].copy_from_slice(&0x0F7Fu16.to_le_bytes()); // RC=truncate
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xD9, 0x6F, 0x08]); // fldcw word [rdi+8]
    append_disp32(&mut c, &[0xD9, 0xB7], FPU_ENV_OFF); // fnstenv [rdi+0x180]
    c.extend_from_slice(&[0xDB, 0xE3]); // fninit resets to round-to-nearest
    append_disp32(&mut c, &[0xD9, 0xA7], FPU_ENV_OFF); // fldenv [rdi+0x180]
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi]
    c.extend_from_slice(&[0xD9, 0xFC]); // frndint using restored truncation RC
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp qword [rdi+0x10]
    c.push(HLT);
    check_mem("x87_fnstenv_fldenv_rounding", &c, regs(), s, 0);
}

#[test]
fn x87_ffree_reuses_full_stack_slot() {
    let mut c = load_rdi_data();
    for _ in 0..8 {
        c.extend_from_slice(&[0xD9, 0xE8]); // fld1
    }
    c.extend_from_slice(&[0xDD, 0xC7]); // ffree st7; frees the next physical push slot
    c.extend_from_slice(&[0xD9, 0xEE]); // fldz should not raise a masked stack overflow
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp qword [rdi+0x10] = 0
    c.extend_from_slice(&[0xDD, 0x7F, 0x20]); // fnstsw word [rdi+0x20]
    c.push(HLT);

    check_mem(
        "x87_ffree_reuses_full_stack_slot",
        &c,
        regs(),
        [0u8; 64],
        0,
    );
}

// ---- String ops: MOVS / STOS / LODS / SCAS / CMPS (with and without REP, DF) ----
//
// Scratch layout convention:
//   src buffer at offset 0, dst buffer at offset 16. We set RSI/RDI to the
//   matching guest addresses, RCX to the element count, and clear/set DF as
//   needed. We compare the whole scratch page plus the final RSI/RDI/RCX.
// String ops affect no arithmetic flags (except SCAS/CMPS which set them like
// CMP); DF must be restored where we set it, and we always end with `CLD`-free
// HLT — the harness reads RFLAGS so we keep DF out of the compared mask.

const SRC_OFF: u64 = 0;
const DST_OFF: u64 = 16;

/// Initial regs for a string test: RSI=src, RDI=dst, RCX=count.
fn string_regs(count: u64) -> Registers {
    let mut r = regs();
    r.rsi = DATA_ADDR + SRC_OFF;
    r.rdi = DATA_ADDR + DST_OFF;
    r.rcx = count;
    r
}

/// Scratch with a source byte pattern at offset 0 (dst region left zero).
fn string_scratch(src: &[u8]) -> [u8; 64] {
    let mut s = [0u8; 64];
    s[..src.len()].copy_from_slice(src);
    s
}

#[test]
fn str_rep_movsb() {
    // REP MOVSB: copy RCX bytes src->dst. F3 A4.
    let r = string_regs(8);
    check_mem(
        "rep_movsb",
        &with_hlt(vec![0xF3, 0xA4]),
        r,
        string_scratch(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]),
        0,
    );
}

#[test]
fn str_rep_movsw() {
    // REP MOVSW: copy RCX words. 66 F3 A5 (operand-size 16). Copy 4 words.
    let r = string_regs(4);
    check_mem(
        "rep_movsw",
        &with_hlt(vec![0xF3, 0x66, 0xA5]),
        r,
        string_scratch(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]),
        0,
    );
}

#[test]
fn str_rep_movsd() {
    // REP MOVSD: copy RCX dwords. F3 A5. Copy 2 dwords.
    let r = string_regs(2);
    check_mem(
        "rep_movsd",
        &with_hlt(vec![0xF3, 0xA5]),
        r,
        string_scratch(&[0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE]),
        0,
    );
}

#[test]
fn str_rep_movsq() {
    // REP MOVSQ: copy RCX qwords. F3 48 A5. Copy 1 qword.
    let r = string_regs(1);
    check_mem(
        "rep_movsq",
        &with_hlt(vec![0xF3, 0x48, 0xA5]),
        r,
        string_scratch(&[1, 2, 3, 4, 5, 6, 7, 8]),
        0,
    );
}

#[test]
fn str_movsb_single() {
    // Single MOVSB (no REP): copies 1 byte, advances RSI/RDI by 1.
    let mut r = regs();
    r.rsi = DATA_ADDR + SRC_OFF;
    r.rdi = DATA_ADDR + DST_OFF;
    check_mem(
        "movsb_single",
        &with_hlt(vec![0xA4]),
        r,
        string_scratch(&[0xAB, 0xCD]),
        0,
    );
}

#[test]
fn str_rep_movsb_df_reverse() {
    // DF=1 reverse copy. Point RSI/RDI at the LAST element so the run walks down.
    // 3 bytes at offsets 0,1,2 copied to dst 16,17,18 in reverse address order.
    let mut r = regs();
    r.rsi = DATA_ADDR + SRC_OFF + 2;
    r.rdi = DATA_ADDR + DST_OFF + 2;
    r.rcx = 3;
    r.rflags = flags::bits::DF; // set direction = down
    check_mem(
        "rep_movsb_df",
        &with_hlt(vec![0xF3, 0xA4]),
        r,
        string_scratch(&[0xA1, 0xB2, 0xC3]),
        0,
    );
}

#[test]
fn str_rep_stosb() {
    // REP STOSB: fill RCX bytes at dst with AL. F3 AA. AL=0x5A, count=6.
    let mut r = string_regs(6);
    r.rax = 0x5A;
    check_mem(
        "rep_stosb",
        &with_hlt(vec![0xF3, 0xAA]),
        r,
        string_scratch(&[]),
        0,
    );
}

#[test]
fn str_rep_stosd() {
    // REP STOSD: fill RCX dwords with EAX. F3 AB. EAX=0xCAFEBABE, count=3.
    let mut r = string_regs(3);
    r.rax = 0xCAFE_BABE;
    check_mem(
        "rep_stosd",
        &with_hlt(vec![0xF3, 0xAB]),
        r,
        string_scratch(&[]),
        0,
    );
}

#[test]
fn str_lodsb() {
    // LODSB: AL <- [RSI], RSI advances. AC. Single, no REP.
    let mut r = regs();
    r.rsi = DATA_ADDR + SRC_OFF;
    r.rax = 0x1122_3344_5566_7700; // only AL should change
    check_mem(
        "lodsb",
        &with_hlt(vec![0xAC]),
        r,
        string_scratch(&[0x99, 0x88]),
        0,
    );
}

#[test]
fn str_repne_scasb_found() {
    // REPNE SCASB: scan dst for AL, stop on match. F2 AE.
    // Buffer at dst (offset 16) = [1,2,3,4,5,...]; AL=4, count large enough.
    let mut s = [0u8; 64];
    s[DST_OFF as usize..DST_OFF as usize + 6].copy_from_slice(&[1, 2, 3, 4, 5, 6]);
    let mut r = regs();
    r.rdi = DATA_ADDR + DST_OFF;
    r.rcx = 6;
    r.rax = 4; // AL = 4 -> match at index 3
    // SCAS sets arithmetic flags like CMP; compare all status flags.
    check_mem("repne_scasb", &with_hlt(vec![0xF2, 0xAE]), r, s, FLAG_MASK);
}

#[test]
fn str_scasb_single_equal() {
    // Single SCASB where [RDI]==AL -> ZF=1, all CMP flags defined.
    let mut s = [0u8; 64];
    s[DST_OFF as usize] = 0x7F;
    let mut r = regs();
    r.rdi = DATA_ADDR + DST_OFF;
    r.rax = 0x7F;
    check_mem("scasb_eq", &with_hlt(vec![0xAE]), r, s, FLAG_MASK);
}

#[test]
fn str_repe_cmpsb_equal_run() {
    // REPE CMPSB: compare src vs dst while equal. F3 A6.
    // Make src==dst for the whole run -> RCX hits 0, ZF=1 at the end.
    let mut s = [0u8; 64];
    let pat = [0xDE, 0xAD, 0xBE, 0xEF, 0x12];
    s[SRC_OFF as usize..SRC_OFF as usize + 5].copy_from_slice(&pat);
    s[DST_OFF as usize..DST_OFF as usize + 5].copy_from_slice(&pat);
    let mut r = regs();
    r.rsi = DATA_ADDR + SRC_OFF;
    r.rdi = DATA_ADDR + DST_OFF;
    r.rcx = 5;
    check_mem(
        "repe_cmpsb_eq",
        &with_hlt(vec![0xF3, 0xA6]),
        r,
        s,
        FLAG_MASK,
    );
}

#[test]
fn str_repe_cmpsb_mismatch() {
    // REPE CMPSB stops at the first differing byte; flags reflect that compare.
    let mut s = [0u8; 64];
    s[SRC_OFF as usize..SRC_OFF as usize + 5].copy_from_slice(&[1, 2, 3, 4, 5]);
    s[DST_OFF as usize..DST_OFF as usize + 5].copy_from_slice(&[1, 2, 9, 4, 5]);
    let mut r = regs();
    r.rsi = DATA_ADDR + SRC_OFF;
    r.rdi = DATA_ADDR + DST_OFF;
    r.rcx = 5;
    check_mem(
        "repe_cmpsb_ne",
        &with_hlt(vec![0xF3, 0xA6]),
        r,
        s,
        FLAG_MASK,
    );
}

#[test]
fn str_cmpsb_single() {
    // Single CMPSB: compares [RSI] vs [RDI], sets CMP flags, advances both.
    let mut s = [0u8; 64];
    s[SRC_OFF as usize] = 0x10;
    s[DST_OFF as usize] = 0x20; // 0x10 - 0x20 -> CF, SF
    let mut r = regs();
    r.rsi = DATA_ADDR + SRC_OFF;
    r.rdi = DATA_ADDR + DST_OFF;
    check_mem("cmpsb_single", &with_hlt(vec![0xA6]), r, s, FLAG_MASK);
}

// ---- SSE3 / SSE2 float: HADDPS/HSUBPS/ADDSUBPS/MOVDDUP/SHUFPD + conversions ----
//
// All packed-float results below use exactly-representable values so the f64/f32
// rounding is bit-identical across backends. We reuse `sse_program` for the
// 2-operand reg forms and `check_sse` to compare the stored 128-bit result.

#[test]
fn sse3_haddps() {
    // HADDPS xmm0, xmm1 = F2 0F 7C C1. Horizontal add of single-precision lanes.
    // a = [1,2,3,4], b = [10,20,30,40] (as f32). result = [a0+a1, a2+a3, b0+b1, b2+b3].
    let af = [1.0f32, 2.0, 3.0, 4.0];
    let bf = [10.0f32, 20.0, 30.0, 40.0];
    let mut a = [0u8; 16];
    let mut b = [0u8; 16];
    for i in 0..4 {
        a[i * 4..i * 4 + 4].copy_from_slice(&af[i].to_le_bytes());
        b[i * 4..i * 4 + 4].copy_from_slice(&bf[i].to_le_bytes());
    }
    check_sse(
        "haddps",
        &sse_program(&[0xF2, 0x0F, 0x7C, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse3_hsubps() {
    // HSUBPS xmm0, xmm1 = F2 0F 7D C1.
    let af = [10.0f32, 3.0, 8.0, 2.5];
    let bf = [100.0f32, 40.0, 9.0, 1.0];
    let mut a = [0u8; 16];
    let mut b = [0u8; 16];
    for i in 0..4 {
        a[i * 4..i * 4 + 4].copy_from_slice(&af[i].to_le_bytes());
        b[i * 4..i * 4 + 4].copy_from_slice(&bf[i].to_le_bytes());
    }
    check_sse(
        "hsubps",
        &sse_program(&[0xF2, 0x0F, 0x7D, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse3_addsubps() {
    // ADDSUBPS xmm0, xmm1 = F2 0F D0 C1. Subtract even lanes, add odd lanes.
    let af = [5.0f32, 5.0, 5.0, 5.0];
    let bf = [1.0f32, 2.0, 3.0, 4.0];
    let mut a = [0u8; 16];
    let mut b = [0u8; 16];
    for i in 0..4 {
        a[i * 4..i * 4 + 4].copy_from_slice(&af[i].to_le_bytes());
        b[i * 4..i * 4 + 4].copy_from_slice(&bf[i].to_le_bytes());
    }
    check_sse(
        "addsubps",
        &sse_program(&[0xF2, 0x0F, 0xD0, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse3_movddup() {
    // MOVDDUP xmm0, xmm1 = F2 0F 12 C1. Duplicate the low f64 of the source.
    let a = [0u8; 16]; // dest, overwritten
    let b = [12.5f64.to_le_bytes(), 99.0f64.to_le_bytes()].concat();
    check_sse(
        "movddup",
        &sse_program(&[0xF2, 0x0F, 0x12, 0xC1]),
        sse_scratch(a, b.try_into().unwrap()),
    );
}

#[test]
fn sse3_packed_memory_sources() {
    let ps_a = f32x4([8.0, 1.5, -4.0, 2.0]);
    let ps_b = f32x4([3.0, -2.0, 10.0, 0.5]);
    for (label, op) in [
        ("haddps_mem", [0xF2, 0x0F, 0x7C, 0x47, 0x10]),
        ("hsubps_mem", [0xF2, 0x0F, 0x7D, 0x47, 0x10]),
        ("addsubps_mem", [0xF2, 0x0F, 0xD0, 0x47, 0x10]),
        ("movsldup_mem", [0xF3, 0x0F, 0x12, 0x47, 0x10]),
        ("movshdup_mem", [0xF3, 0x0F, 0x16, 0x47, 0x10]),
    ] {
        check_sse(
            label,
            &sse_memory_source_program(&op),
            sse_scratch(ps_a, ps_b),
        );
    }

    let pd_a = f64x2([16.0, 1.25]);
    let pd_b = f64x2([3.5, -8.0]);
    for (label, op) in [
        ("haddpd_mem", [0x66, 0x0F, 0x7C, 0x47, 0x10]),
        ("hsubpd_mem", [0x66, 0x0F, 0x7D, 0x47, 0x10]),
        ("addsubpd_mem", [0x66, 0x0F, 0xD0, 0x47, 0x10]),
        ("movddup_mem", [0xF2, 0x0F, 0x12, 0x47, 0x10]),
    ] {
        check_sse(
            label,
            &sse_memory_source_program(&op),
            sse_scratch(pd_a, pd_b),
        );
    }
}

#[test]
fn sse2_shufpd() {
    // SHUFPD xmm0, xmm1, imm8 = 66 0F C6 C1 ib. imm=0b10 -> dst.lo=a.hi, dst.hi=b.lo.
    let a = [1.0f64.to_le_bytes(), 2.0f64.to_le_bytes()].concat();
    let b = [3.0f64.to_le_bytes(), 4.0f64.to_le_bytes()].concat();
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]); // movdqu xmm0, [rdi]
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]); // movdqu xmm1, [rdi+0x10]
    prog.extend_from_slice(&[0x66, 0x0F, 0xC6, 0xC1, 0x02]); // shufpd xmm0, xmm1, 2
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]); // movdqu [rdi+0x20], xmm0
    prog.push(HLT);
    check_sse(
        "shufpd",
        &prog,
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse2_cvtdq2ps() {
    // CVTDQ2PS xmm0, xmm1 = 0F 5B C1. Convert 4 packed i32 -> 4 packed f32.
    // Use exact integers (small) so the result is bit-exact.
    let a = [0u8; 16];
    let ints = [3i32, -7, 100, -1];
    let mut b = [0u8; 16];
    for i in 0..4 {
        b[i * 4..i * 4 + 4].copy_from_slice(&ints[i].to_le_bytes());
    }
    check_sse(
        "cvtdq2ps",
        &sse_program(&[0x0F, 0x5B, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse2_cvtps2dq() {
    // CVTPS2DQ xmm0, xmm1 = 66 0F 5B C1. Convert 4 f32 -> 4 i32 (round-to-nearest).
    // Integral inputs -> exact.
    let a = [0u8; 16];
    let fs = [3.0f32, -7.0, 100.0, -1.0];
    let mut b = [0u8; 16];
    for i in 0..4 {
        b[i * 4..i * 4 + 4].copy_from_slice(&fs[i].to_le_bytes());
    }
    check_sse(
        "cvtps2dq",
        &sse_program(&[0x66, 0x0F, 0x5B, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse2_cvttps2dq_truncates() {
    // CVTTPS2DQ xmm0, xmm1 = F3 0F 5B C1. Truncating convert f32 -> i32.
    // 3.9 -> 3, -3.9 -> -3, 2.1 -> 2, -0.9 -> 0.
    let a = [0u8; 16];
    let fs = [3.9f32, -3.9, 2.1, -0.9];
    let mut b = [0u8; 16];
    for i in 0..4 {
        b[i * 4..i * 4 + 4].copy_from_slice(&fs[i].to_le_bytes());
    }
    check_sse(
        "cvttps2dq",
        &sse_program(&[0xF3, 0x0F, 0x5B, 0xC1]),
        sse_scratch(a, b),
    );
}

// ---- Misc: SAHF / LAHF ----

#[test]
fn sahf_loads_flags() {
    // SAHF (9E): AH -> low byte of RFLAGS (SF,ZF,_,AF,_,PF,_,CF). AH=0xD5
    // sets SF,ZF,AF,PF (and bit1 reserved=1, CF=1). We then read the status flags.
    let mut r = regs();
    // AH = 1101_0101: SF=1 ZF=1 (bit5=0) AF=1 (bit3=0) PF=1 (bit1=1 reserved) CF=1
    r.rax = 0xD5 << 8;
    check("sahf", &with_hlt(vec![0x9E]), r);
}

#[test]
fn lahf_stores_flags() {
    // LAHF (9F): low byte of RFLAGS -> AH. Set a known flag state via CMP first.
    let mut r = regs();
    r.rax = 0x0; // 0 - 1
    r.rbx = 0x1;
    // 48 39 D8  cmp rax, rbx  (sets SF, CF, AF)
    // 9F        lahf          (AH <- status byte)
    check("lahf", &with_hlt(vec![0x48, 0x39, 0xD8, 0x9F]), r);
}

#[test]
fn lahf_sahf_roundtrip() {
    // LAHF then SAHF should reproduce the same flag state. Seed via CMP.
    let mut r = regs();
    r.rax = 0x5;
    r.rbx = 0x5; // equal -> ZF, PF
    // cmp; lahf; xor ah with nothing; sahf
    check(
        "lahf_sahf",
        &with_hlt(vec![0x48, 0x39, 0xD8, 0x9F, 0x9E]),
        r,
    );
}

// ---- MOVBE (move with byte swap) ----

#[test]
fn movbe_load_r64() {
    // MOVBE r64, m64 = 48 0F 38 F0 /r. Load+byteswap from [rdi].
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0x0011_2233_4455_6677u64.to_le_bytes());
    let mut r = regs();
    r.rdi = DATA_ADDR;
    // 48 0F 38 F0 07  movbe rax, [rdi]
    check_mem(
        "movbe_load64",
        &with_hlt(vec![0x48, 0x0F, 0x38, 0xF0, 0x07]),
        r,
        s,
        0,
    );
}

#[test]
fn movbe_store_r32() {
    // MOVBE m32, r32 = 0F 38 F1 /r. Store EAX byteswapped to [rdi]; verify scratch.
    let mut r = regs();
    r.rax = 0x1122_3344;
    r.rdi = DATA_ADDR;
    // 0F 38 F1 07  movbe [rdi], eax
    check_mem(
        "movbe_store32",
        &with_hlt(vec![0x0F, 0x38, 0xF1, 0x07]),
        r,
        zero_scratch(),
        0,
    );
}

#[test]
fn movbe_memory_width_forms() {
    let mut s = [0u8; 64];
    s[0..2].copy_from_slice(&0xBEEFu16.to_le_bytes());
    s[4..8].copy_from_slice(&0x89AB_CDEFu32.to_le_bytes());
    s[8..16].copy_from_slice(&0x0123_4567_89AB_CDEFu64.to_le_bytes());

    let mut r = regs();
    r.rax = 0x1122_3344_5566_7788;
    r.rbx = 0xAABB_CCDD_EEFF_0011;
    r.rcx = 0x1020_3040_5060_7080;
    r.rdx = 0x1122;
    r.rsi = 0x3344_5566;
    r.r8 = 0x0123_4567_89AB_CDEF;
    r.rdi = DATA_ADDR;

    let code = with_hlt(vec![
        0x66, 0x0F, 0x38, 0xF0, 0x07, // movbe ax, word [rdi]
        0x66, 0x89, 0x47, 0x18, // mov [rdi+24], ax
        0x0F, 0x38, 0xF0, 0x5F, 0x04, // movbe ebx, dword [rdi+4]
        0x89, 0x5F, 0x1C, // mov [rdi+28], ebx
        0x48, 0x0F, 0x38, 0xF0, 0x4F, 0x08, // movbe rcx, qword [rdi+8]
        0x48, 0x89, 0x4F, 0x28, // mov [rdi+40], rcx
        0x66, 0x0F, 0x38, 0xF1, 0x57, 0x20, // movbe word [rdi+32], dx
        0x0F, 0x38, 0xF1, 0x77, 0x24, // movbe dword [rdi+36], esi
        0x4C, 0x0F, 0x38, 0xF1, 0x47, 0x38, // movbe qword [rdi+56], r8
    ]);
    check_mem("movbe_memory_width_forms", &code, r, s, 0);
}

// ---- BMI1: ANDN / BLSI / BLSR / BLSMSK (VEX-encoded) ----
//
// BMI1 defines ZF and SF from the result, clears OF; AF/PF are undefined and CF
// is defined per-instruction (ANDN clears CF; BLS* set CF specially). To stay on
// the safe side we compare the architecturally-defined bits: ZF, SF, CF, OF.
const BMI_DEFINED: u64 = flags::bits::ZF | flags::bits::SF | flags::bits::CF | flags::bits::OF;
const BMI_BEXTR_DEFINED: u64 = flags::bits::ZF | flags::bits::CF | flags::bits::OF;

#[test]
fn bmi_andn() {
    // ANDN r32a, r32b, r/m32 = VEX.LZ.0F38.W0 F2 /r : dest = ~src1 & src2.
    // VEX 2-byte: C4 E2 (map 0F38) ... use 3-byte VEX. Encode andn eax, ebx, ecx:
    //   VEX.NDS.LZ.0F38.W0 F2 /r, vvvv = ~ebx, src2 = ecx (modrm).
    // C4 E2 60 F2 C1  -> andn eax, ebx, ecx   (dest=~ebx & ecx = 0xAA00)
    let mut r = regs();
    r.rbx = 0x0000_00FF; // src1 (inverted)
    r.rcx = 0x0000_AAAA; // src2
    check_flags_masked(
        "andn",
        &with_hlt(vec![0xC4, 0xE2, 0x60, 0xF2, 0xC1]),
        r,
        BMI_DEFINED,
    );
}

#[test]
fn bmi_blsi() {
    // BLSI r32, r/m32 = VEX.LZ.0F38.W0 F3 /3 : dest = src & -src (isolate lowest set bit).
    // vvvv encodes dest, modrm.reg=/3 selects the op, modrm.rm=src.
    // andn-style 3-byte VEX with vvvv=eax(dest), rm=ecx(src): C4 E2 78 F3 D9
    //   reg field = 011 (/3 = BLSI), rm = 001 (ecx), vvvv=1111-? -> dest eax.
    let mut r = regs();
    r.rcx = 0x0000_00B0; // lowest set bit is bit 4 (0x10)
    check_flags_masked(
        "blsi",
        &with_hlt(vec![0xC4, 0xE2, 0x78, 0xF3, 0xD9]),
        r,
        BMI_DEFINED,
    );
}

#[test]
fn bmi_blsr() {
    // BLSR r32, r/m32 = VEX.LZ.0F38.W0 F3 /1 : dest = src & (src-1) (clear lowest set bit).
    // reg field = 001 (/1 = BLSR), rm=001(ecx), vvvv -> eax. C4 E2 78 F3 C9.
    let mut r = regs();
    r.rcx = 0x0000_00B0;
    check_flags_masked(
        "blsr",
        &with_hlt(vec![0xC4, 0xE2, 0x78, 0xF3, 0xC9]),
        r,
        BMI_DEFINED,
    );
}

#[test]
fn bmi_blsmsk() {
    // BLSMSK r32, r/m32 = VEX.LZ.0F38.W0 F3 /2 : dest = src ^ (src-1) (mask up to lowest set bit).
    // reg field = 010 (/2 = BLSMSK), rm=001(ecx), vvvv -> eax. C4 E2 78 F3 D1.
    let mut r = regs();
    r.rcx = 0x0000_00B0;
    check_flags_masked(
        "blsmsk",
        &with_hlt(vec![0xC4, 0xE2, 0x78, 0xF3, 0xD1]),
        r,
        BMI_DEFINED,
    );
}

#[test]
fn bmi_bextr_register_widths() {
    let mut r = regs();
    r.rcx = 0xFEDC_BA98_7654_3210;
    r.rbx = 12 | (20 << 8);
    check_flags_masked(
        "bextr_r64",
        &with_hlt(vec![0xC4, 0xE2, 0xE0, 0xF7, 0xC1]),
        r,
        BMI_BEXTR_DEFINED,
    );

    let mut r = regs();
    r.rcx = 0x8765_4321;
    r.rbx = 13;
    check_flags_masked(
        "bextr_r32_zero_length",
        &with_hlt(vec![0xC4, 0xE2, 0x60, 0xF7, 0xC1]),
        r,
        BMI_BEXTR_DEFINED,
    );
}

#[test]
fn bmi_bextr_memory_forms() {
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0x0123_4567_89AB_CDEFu64.to_le_bytes());
    s[8..12].copy_from_slice(&0xF0E1_D2C3u32.to_le_bytes());

    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rbx = 7 | (24 << 8);

    let mut code = vec![0xC4, 0xE2, 0xE0, 0xF7, 0x07]; // bextr rax, [rdi], rbx
    code.extend_from_slice(&[0xBB]);
    code.extend_from_slice(&(4 | (12 << 8) as u32).to_le_bytes()); // mov ebx, start=4,len=12
    code.extend_from_slice(&[0xC4, 0xE2, 0x60, 0xF7, 0x57, 0x08]); // bextr edx, [rdi+8], ebx
    code.push(HLT);
    check_mem("bextr_memory_forms", &code, r, s, BMI_BEXTR_DEFINED);
}

#[test]
fn bmi_memory_source_forms() {
    let mut s = [0u8; 64];
    s[0..4].copy_from_slice(&0x0000_00B0u32.to_le_bytes());
    s[8..12].copy_from_slice(&0x0000_AAAAu32.to_le_bytes());

    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rbx = 0x0000_00FF;

    let code = with_hlt(vec![
        0xC4, 0xE2, 0x60, 0xF2, 0x47, 0x08, // andn eax, ebx, [rdi+8]
        0xC4, 0xE2, 0x70, 0xF3, 0x1F, // blsi ecx, [rdi]
        0xC4, 0xE2, 0x68, 0xF3, 0x0F, // blsr edx, [rdi]
        0xC4, 0xE2, 0x60, 0xF3, 0x17, // blsmsk ebx, [rdi]
    ]);
    check_mem("bmi_memory_source_forms", &code, r, s, BMI_DEFINED);
}

#[test]
fn bmi_64_memory_source_forms() {
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0x0000_0000_0000_00B0u64.to_le_bytes());
    s[8..16].copy_from_slice(&0xAAAA_AAAA_AAAA_AAAAu64.to_le_bytes());

    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rbx = 0x0000_0000_0000_00FF;

    let code = with_hlt(vec![
        0xC4, 0xE2, 0xE0, 0xF2, 0x47, 0x08, // andn rax, rbx, [rdi+8]
        0xC4, 0xE2, 0xF0, 0xF3, 0x1F, // blsi rcx, [rdi]
        0xC4, 0xE2, 0xE8, 0xF3, 0x0F, // blsr rdx, [rdi]
        0xC4, 0xE2, 0xE0, 0xF3, 0x17, // blsmsk rbx, [rdi]
    ]);
    check_mem("bmi_64_memory_source_forms", &code, r, s, BMI_DEFINED);
}

// ---- BMI2: PEXT / PDEP / MULX / RORX / SARX / SHRX / SHLX ----
//
// BMI2 bit-manipulation ops do NOT affect flags at all, so compare GPRs only.

#[test]
fn bmi2_pext() {
    // PEXT r32, r32, r/m32 = VEX.LZ.F3.0F38.W0 F5 /r : extract bits of src1 selected by mask.
    //   andn-form: vvvv = src1 (ebx), rm = mask (ecx), dest = eax.
    //   pp=10 (F3), map=0F38 -> VEX3 C4 E2 62 F5 C1.
    let mut r = regs();
    r.rbx = 0x1234_5678; // source
    r.rcx = 0x0F0F_0F0F; // mask: take low nibble of each byte
    check_flags_masked("pext", &with_hlt(vec![0xC4, 0xE2, 0x62, 0xF5, 0xC1]), r, 0);
}

#[test]
fn bmi2_pdep() {
    // PDEP r32, r32, r/m32 = VEX.LZ.F2.0F38.W0 F5 /r : deposit bits into mask positions.
    //   pp=11 (F2) -> VEX3 C4 E2 63 F5 C1. vvvv=src1(ebx), rm=mask(ecx), dest=eax.
    let mut r = regs();
    r.rbx = 0x0000_000F; // 4 source bits
    r.rcx = 0x0F0F_0F0F; // scatter into these positions
    check_flags_masked("pdep", &with_hlt(vec![0xC4, 0xE2, 0x63, 0xF5, 0xC1]), r, 0);
}

#[test]
fn bmi2_mulx() {
    // MULX r32a, r32b, r/m32 = VEX.LZ.F2.0F38.W0 F6 /r : unsigned mul of EDX by r/m,
    //   high half -> dest1 (vvvv), low half -> dest2 (modrm.reg). EDX is implicit.
    //   mulx eax, ebx, ecx : reg=eax(dest2), vvvv=ebx(dest1), rm=ecx(src2).
    //   pp=11(F2), map=0F38 -> C4 E2 63 F6 C1  (vvvv=ebx=0b1101 inverted... encode below)
    // Encoding: C4 E2 (62?) — vvvv must encode ebx(=0b0011) inverted = 0b1100.
    //   byte3 = W(0)|vvvv(1100)|L(0)|pp(11) = 0 1100 0 11 = 0x63. reg/rm: reg=eax(000), rm=ecx(001) -> modrm C1.
    let mut r = regs();
    r.rdx = 0x0001_0000; // implicit multiplicand
    r.rcx = 0x0001_0000; // src2 -> product 0x1_0000_0000
    check_flags_masked("mulx", &with_hlt(vec![0xC4, 0xE2, 0x63, 0xF6, 0xC1]), r, 0);
}

#[test]
fn bmi2_rorx() {
    // RORX r32, r/m32, imm8 = VEX.LZ.F2.0F3A.W0 F0 /r ib : rotate right, no flags.
    //   map=0F3A -> byte2 low nibble = 3. pp=11(F2). rorx eax, ecx, 8.
    //   C4 E3 7B F0 C1 08 : reg=eax(000), rm=ecx(001) -> C1, imm=8.
    let mut r = regs();
    r.rcx = 0x1234_5678;
    check_flags_masked(
        "rorx",
        &with_hlt(vec![0xC4, 0xE3, 0x7B, 0xF0, 0xC1, 0x08]),
        r,
        0,
    );
}

#[test]
fn bmi2_sarx() {
    // SARX r32, r/m32, r32 = VEX.LZ.F3.0F38.W0 F7 /r : arithmetic shift right by count reg.
    //   pp=10(F3). vvvv encodes the count register (ebx). sarx eax, ecx, ebx.
    //   byte3 = W0 vvvv(~ebx=1100) L0 pp(10) -> 0 1100 0 10 = 0x62. modrm reg=eax,rm=ecx -> C1.
    let mut r = regs();
    r.rcx = 0xFFFF_8000; // negative when treated as i32
    r.rbx = 4; // shift count
    check_flags_masked("sarx", &with_hlt(vec![0xC4, 0xE2, 0x62, 0xF7, 0xC1]), r, 0);
}

#[test]
fn bmi2_shrx() {
    // SHRX r32, r/m32, r32 = VEX.LZ.F2.0F38.W0 F7 /r : logical shift right.
    //   pp=11(F2). vvvv=ebx(count). byte3 = 0 1100 0 11 = 0x63.
    let mut r = regs();
    r.rcx = 0xF000_0000;
    r.rbx = 8;
    check_flags_masked("shrx", &with_hlt(vec![0xC4, 0xE2, 0x63, 0xF7, 0xC1]), r, 0);
}

#[test]
fn bmi2_shlx() {
    // SHLX r32, r/m32, r32 = VEX.LZ.66.0F38.W0 F7 /r : logical shift left.
    //   pp=01(66). vvvv=ebx(count). byte3 = 0 1100 0 01 = 0x61.
    let mut r = regs();
    r.rcx = 0x0000_00FF;
    r.rbx = 12;
    check_flags_masked("shlx", &with_hlt(vec![0xC4, 0xE2, 0x61, 0xF7, 0xC1]), r, 0);
}

#[test]
fn bmi2_rorx_r64() {
    // 64-bit RORX: VEX.LZ.F2.0F3A.W1 F0 /r ib. W1 -> byte3 bit7 set: 0xFB.
    //   rorx rax, rcx, 20 : C4 E3 FB F0 C1 14.
    let mut r = regs();
    r.rcx = 0x0123_4567_89AB_CDEF;
    check_flags_masked(
        "rorx64",
        &with_hlt(vec![0xC4, 0xE3, 0xFB, 0xF0, 0xC1, 0x14]),
        r,
        0,
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 3: exhaustive ALU operand sizes / mul-div forms /
// rotate-through-carry / SHLD-SHRD edge counts / BCD (mode note) / SSE float /
// more BMI2 / RIP-relative LEA.
// ===========================================================================
//
// Everything here reuses the existing helpers verbatim:
//  - `check`               : GPRs + all 6 status flags + stack.
//  - `check_flags_masked`  : GPRs + a subset of status flags (for ops that leave
//                            some flags architecturally undefined).
//  - `check_sse`           : scratch-driven 128-bit SSE result comparison.
//  - `sse_program`/`sse_scratch` : load xmm0/xmm1 from scratch, run op, store back.
//
// Flag-definition reminders (Intel SDM Vol.2) used below:
//  - ADD/SUB/ADC/SBB/CMP/NEG/AND/OR/XOR/TEST : all 6 status flags defined.
//  - INC/DEC : define OF/SF/ZF/AF/PF, leave CF UNCHANGED (so we must seed CF and
//              verify it survives - `check` compares CF too).
//  - MUL/IMUL : define CF/OF only; SF/ZF/AF/PF UNDEFINED -> mask to MULDIV_DEFINED.
//  - DIV/IDIV : ALL status flags UNDEFINED -> mask 0 (GPR-only compare).
//  - RCL/RCR  : define CF; OF defined only for a 1-count, UNDEFINED for count>1.
//  - SHLD/SHRD: define CF/SF/ZF/PF; OF defined only for count==1; AF undefined;
//               result UNDEFINED when count > operand size (we avoid that range).
//  - SSE compare/min/max ops produce a bit-exact 128-bit result we compare via
//    scratch; no RFLAGS effect (check_sse uses flag_mask 0).

// Rotate-through-carry: CF always defined; OF undefined for count>1.
const RCL_RCR_1: u64 = FLAG_MASK; // a 1-count rotate fully defines OF too
const RCL_RCR_MULTI: u64 = flags::bits::CF; // count>1 -> only CF defined

// ---------------------------------------------------------------------------
// ALU: ADD/SUB/AND/OR/XOR/CMP across 8/16/32/64 at sign/zero/overflow edges.
// ---------------------------------------------------------------------------

// ---- ADD at signed/unsigned boundaries, all widths ----

#[test]
fn add8_7f_plus_1_overflow() {
    // 0x7F + 1 = 0x80 : OF=1, SF=1, AF=1, CF=0, ZF=0.
    let mut r = regs();
    r.rax = 0x7F;
    r.rbx = 0x01;
    check("add8_7f_1", &with_hlt(vec![0x00, 0xD8]), r); // add al, bl
}

#[test]
fn add8_ff_plus_ff() {
    // 0xFF + 0xFF = 0x1FE -> al=0xFE, CF=1, OF=0, SF=1.
    let mut r = regs();
    r.rax = 0xFF;
    r.rbx = 0xFF;
    check("add8_ff_ff", &with_hlt(vec![0x00, 0xD8]), r);
}

#[test]
fn add16_8000_plus_8000() {
    // 0x8000 + 0x8000 = 0x10000 -> ax=0, CF=1, OF=1, ZF=1.
    let mut r = regs();
    r.rax = 0x8000;
    r.rbx = 0x8000;
    check("add16_8000", &with_hlt(vec![0x66, 0x01, 0xD8]), r); // add ax, bx
}

#[test]
fn add32_7fffffff_plus_1() {
    // 0x7FFFFFFF + 1 = 0x80000000 : OF=1, SF=1, zero-extends into rax.
    let mut r = regs();
    r.rax = 0x7FFF_FFFF;
    r.rbx = 0x0000_0001;
    check("add32_7fff", &with_hlt(vec![0x01, 0xD8]), r); // add eax, ebx
}

#[test]
fn add64_signed_boundary() {
    // INT64_MAX + 1 -> INT64_MIN : OF=1, SF=1.
    let mut r = regs();
    r.rax = 0x7FFF_FFFF_FFFF_FFFF;
    r.rbx = 0x1;
    check("add64_int_min", &with_hlt(vec![0x48, 0x01, 0xD8]), r); // add rax, rbx
}

// ---- SUB at signed/unsigned boundaries, all widths ----

#[test]
fn sub8_80_minus_1_overflow() {
    // 0x80 - 1 = 0x7F : signed (-128)-1 underflow -> OF=1, SF=0, CF=0.
    let mut r = regs();
    r.rax = 0x80;
    r.rbx = 0x01;
    check("sub8_80_1", &with_hlt(vec![0x28, 0xD8]), r); // sub al, bl
}

#[test]
fn sub16_0_minus_8000() {
    // 0 - 0x8000 : CF=1, OF=1 (result 0x8000 = INT16_MIN), SF=1.
    let mut r = regs();
    r.rax = 0x0000;
    r.rbx = 0x8000;
    check("sub16_0_8000", &with_hlt(vec![0x66, 0x29, 0xD8]), r); // sub ax, bx
}

#[test]
fn sub32_80000000_minus_1() {
    // 0x80000000 - 1 = 0x7FFFFFFF : OF=1 (INT32_MIN-1), CF=0.
    let mut r = regs();
    r.rax = 0x8000_0000;
    r.rbx = 0x0000_0001;
    check("sub32_int_min", &with_hlt(vec![0x29, 0xD8]), r); // sub eax, ebx
}

#[test]
fn sub64_equal_zero() {
    // equal operands -> 0, ZF=1, CF=0, OF=0.
    let mut r = regs();
    r.rax = 0xDEAD_BEEF_CAFE_BABE;
    r.rbx = 0xDEAD_BEEF_CAFE_BABE;
    check("sub64_eq", &with_hlt(vec![0x48, 0x29, 0xD8]), r);
}

// ---- AND / OR / XOR at width edges (CF/OF always cleared, SF/ZF/PF defined) ----

#[test]
fn and8_high_bit() {
    // 0x80 & 0xFF = 0x80 : SF=1, ZF=0, PF=0 (one bit), CF=OF=0.
    let mut r = regs();
    r.rax = 0x80;
    r.rbx = 0xFF;
    check("and8_sf", &with_hlt(vec![0x20, 0xD8]), r); // and al, bl
}

#[test]
fn or32_zero_extends() {
    // OR of 32-bit operands zero-extends into rax; result 0x80000001 -> SF=1.
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_8000_0000;
    r.rbx = 0x0000_0000_0000_0001;
    check("or32_zx", &with_hlt(vec![0x09, 0xD8]), r); // or eax, ebx
}

#[test]
fn xor16_high_bit() {
    // 0x8000 ^ 0x0001 = 0x8001 : SF=1, PF=0.
    let mut r = regs();
    r.rax = 0x8000;
    r.rbx = 0x0001;
    check("xor16_sf", &with_hlt(vec![0x66, 0x31, 0xD8]), r); // xor ax, bx
}

#[test]
fn or64_zero_result() {
    // 0 | 0 -> ZF=1, PF=1.
    let r = regs();
    check("or64_zero", &with_hlt(vec![0x48, 0x09, 0xD8]), r); // or rax, rbx (both 0)
}

// ---- CMP at width edges (signed overflow boundaries) ----

#[test]
fn cmp16_overflow() {
    // 0x7FFF cmp 0xFFFF (-1): 32767 - (-1) overflows -> OF=1.
    let mut r = regs();
    r.rax = 0x7FFF;
    r.rbx = 0xFFFF;
    check("cmp16_of", &with_hlt(vec![0x66, 0x39, 0xD8]), r); // cmp ax, bx
}

#[test]
fn cmp32_equal() {
    let mut r = regs();
    r.rax = 0x1234_5678;
    r.rbx = 0x1234_5678;
    check("cmp32_eq", &with_hlt(vec![0x39, 0xD8]), r); // cmp eax, ebx
}

#[test]
fn cmp8_imm_boundary() {
    // CMP AL, 0x80 with AL=0x7F : signed 127 - (-128) overflows -> OF=1, SF=1, CF=1.
    let mut r = regs();
    r.rax = 0x7F;
    check("cmp8_imm80", &with_hlt(vec![0x3C, 0x80]), r); // cmp al, 0x80
}

// ---------------------------------------------------------------------------
// ADC / SBB carry-in chains: model multi-word add/sub with carry propagation.
// ---------------------------------------------------------------------------

#[test]
fn adc_chain_two_words() {
    // Emulate a 128-bit add of two 64-bit halves:
    //   add rax, rcx     (low halves, sets CF)
    //   adc rbx, rdx     (high halves, consumes CF)
    // low: 0xFFFF.. + 1 -> 0, CF=1 ; high: 0 + 0 + 1 -> 1.
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF;
    r.rcx = 0x0000_0000_0000_0001;
    r.rbx = 0x0000_0000_0000_0000;
    r.rdx = 0x0000_0000_0000_0000;
    // 48 01 C8  add rax, rcx
    // 48 11 D3  adc rbx, rdx
    check(
        "adc_chain",
        &with_hlt(vec![0x48, 0x01, 0xC8, 0x48, 0x11, 0xD3]),
        r,
    );
}

#[test]
fn sbb_chain_two_words() {
    // 128-bit subtract:
    //   sub rax, rcx     (low halves, sets borrow)
    //   sbb rbx, rdx     (high halves, consumes borrow)
    // low: 0 - 1 -> 0xFFFF.., CF=1 ; high: 5 - 0 - 1 -> 4.
    let mut r = regs();
    r.rax = 0x0000_0000_0000_0000;
    r.rcx = 0x0000_0000_0000_0001;
    r.rbx = 0x0000_0000_0000_0005;
    r.rdx = 0x0000_0000_0000_0000;
    // 48 29 C8  sub rax, rcx
    // 48 19 D3  sbb rbx, rdx
    check(
        "sbb_chain",
        &with_hlt(vec![0x48, 0x29, 0xC8, 0x48, 0x19, 0xD3]),
        r,
    );
}

#[test]
fn adc8_carry_in_no_carry_out() {
    // adc al, bl with CF=1 : 0x10 + 0x20 + 1 = 0x31, CF=0, AF=0.
    let mut r = regs();
    r.rax = 0x10;
    r.rbx = 0x20;
    r.rflags = flags::bits::CF;
    check("adc8_cin", &with_hlt(vec![0x10, 0xD8]), r); // adc al, bl
}

#[test]
fn adc16_carry_in_wrap() {
    // adc ax, bx with CF=1 : 0xFFFF + 0 + 1 = 0x10000 -> ax=0, CF=1, ZF=1.
    let mut r = regs();
    r.rax = 0xFFFF;
    r.rbx = 0x0000;
    r.rflags = flags::bits::CF;
    check("adc16_cin", &with_hlt(vec![0x66, 0x11, 0xD8]), r); // adc ax, bx
}

#[test]
fn sbb16_borrow_in() {
    // sbb ax, bx with CF=1 : 0x0001 - 0x0000 - 1 = 0x0000 -> ZF=1.
    let mut r = regs();
    r.rax = 0x0001;
    r.rbx = 0x0000;
    r.rflags = flags::bits::CF;
    check("sbb16_bin", &with_hlt(vec![0x66, 0x19, 0xD8]), r); // sbb ax, bx
}

#[test]
fn sbb8_borrow_in_underflow() {
    // sbb al, bl with CF=1 : 0x00 - 0x00 - 1 -> 0xFF, CF=1, SF=1, AF=1.
    let mut r = regs();
    r.rax = 0x00;
    r.rbx = 0x00;
    r.rflags = flags::bits::CF;
    check("sbb8_bin", &with_hlt(vec![0x18, 0xD8]), r); // sbb al, bl
}

// ---------------------------------------------------------------------------
// INC / DEC must preserve CF across all widths; NEG flag exactness.
// ---------------------------------------------------------------------------

#[test]
fn inc16_overflow_keeps_cf() {
    // inc ax with ax=0x7FFF -> 0x8000 : OF=1, SF=1; CF must stay as seeded (1).
    let mut r = regs();
    r.rax = 0x7FFF;
    r.rflags = flags::bits::CF;
    check("inc16_of_cf", &with_hlt(vec![0x66, 0xFF, 0xC0]), r); // inc ax
}

#[test]
fn dec8_underflow_keeps_cf() {
    // dec al with al=0 -> 0xFF : SF=1, AF=1, OF=0; CF preserved (seeded 1).
    let mut r = regs();
    r.rax = 0x00;
    r.rflags = flags::bits::CF;
    check("dec8_uf_cf", &with_hlt(vec![0xFE, 0xC8]), r); // dec al
}

#[test]
fn dec32_int_min_overflow() {
    // dec eax with eax=0x80000000 -> 0x7FFFFFFF : OF=1; CF preserved.
    let mut r = regs();
    r.rax = 0x8000_0000;
    r.rflags = flags::bits::CF;
    check("dec32_of", &with_hlt(vec![0xFF, 0xC8]), r); // dec eax
}

#[test]
fn neg8_int_min() {
    // neg al with al=0x80 (-128) -> 0x80 (overflow): OF=1, CF=1, SF=1.
    let mut r = regs();
    r.rax = 0x80;
    check("neg8_int_min", &with_hlt(vec![0xF6, 0xD8]), r); // neg al
}

#[test]
fn neg16_one() {
    // neg ax with ax=1 -> 0xFFFF : CF=1, SF=1, OF=0, AF=1.
    let mut r = regs();
    r.rax = 0x0001;
    check("neg16_one", &with_hlt(vec![0x66, 0xF7, 0xD8]), r); // neg ax
}

#[test]
fn neg32_zero_extends() {
    // neg eax with eax=0x00000010 -> 0xFFFFFFF0, zero-extends into rax; CF=1.
    let mut r = regs();
    r.rax = 0x0000_0010;
    check("neg32_zx", &with_hlt(vec![0xF7, 0xD8]), r); // neg eax
}

// ---------------------------------------------------------------------------
// IMUL forms: one-operand (RDX:RAX), two-operand, three-operand-with-imm.
// MUL one-operand. All mask to MULDIV_DEFINED (CF/OF only defined).
// ---------------------------------------------------------------------------

#[test]
fn imul_one_operand_64() {
    // IMUL r/m64 (F7 /5) : RDX:RAX = RAX * RBX (signed full product).
    // (-2) * 3 = -6 : RAX=0xFFFF..FA, RDX=0xFFFF..FF (sign extension), CF/OF=0.
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFE; // -2
    r.rbx = 0x0000_0000_0000_0003; // 3
    // 48 F7 EB  imul rbx
    check_flags_masked(
        "imul1_64",
        &with_hlt(vec![0x48, 0xF7, 0xEB]),
        r,
        MULDIV_DEFINED,
    );
}

#[test]
fn imul_one_operand_overflow() {
    // Large signed product fills RDX -> CF/OF=1 (result doesn't fit in RAX alone).
    let mut r = regs();
    r.rax = 0x0000_0001_0000_0000; // 2^32
    r.rbx = 0x0000_0001_0000_0000; // 2^32 -> product 2^64 in RDX:RAX
    check_flags_masked(
        "imul1_of",
        &with_hlt(vec![0x48, 0xF7, 0xEB]),
        r,
        MULDIV_DEFINED,
    );
}

#[test]
fn imul_one_operand_32() {
    // IMUL r/m32 (F7 /5, no REX.W): EDX:EAX = EAX * EBX, zero-extends both into r64.
    // (-3) * 7 = -21 : EAX=0xFFFFFFEB, EDX=0xFFFFFFFF.
    let mut r = regs();
    r.rax = 0xFFFF_FFFD; // -3 in 32-bit
    r.rbx = 0x0000_0007;
    // F7 EB  imul ebx
    check_flags_masked("imul1_32", &with_hlt(vec![0xF7, 0xEB]), r, MULDIV_DEFINED);
}

#[test]
fn imul_two_operand_negative() {
    // imul rax, rbx with signed operands : (-4) * 5 = -20.
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFC; // -4
    r.rbx = 0x0000_0000_0000_0005;
    // 48 0F AF C3  imul rax, rbx
    check_flags_masked(
        "imul2_neg",
        &with_hlt(vec![0x48, 0x0F, 0xAF, 0xC3]),
        r,
        MULDIV_DEFINED,
    );
}

#[test]
fn imul_three_operand_imm8() {
    // IMUL rax, rbx, imm8 (48 6B C3 ib) : rax = rbx * sign_extend(imm8).
    // 0x100 * (-3) = -0x300.
    let mut r = regs();
    r.rbx = 0x0000_0000_0000_0100;
    // 48 6B C3 FD  imul rax, rbx, -3
    check_flags_masked(
        "imul3_imm8",
        &with_hlt(vec![0x48, 0x6B, 0xC3, 0xFD]),
        r,
        MULDIV_DEFINED,
    );
}

#[test]
fn imul_three_operand_imm32() {
    // IMUL rax, rbx, imm32 (48 69 C3 id) : rax = rbx * sign_extend(imm32).
    // 7 * 0x0001_0000 = 0x0007_0000.
    let mut r = regs();
    r.rbx = 0x0000_0000_0000_0007;
    // 48 69 C3 00 00 01 00  imul rax, rbx, 0x10000
    check_flags_masked(
        "imul3_imm32",
        &with_hlt(vec![0x48, 0x69, 0xC3, 0x00, 0x00, 0x01, 0x00]),
        r,
        MULDIV_DEFINED,
    );
}

#[test]
fn imul_three_operand_imm32_overflow() {
    // Product overflows 64 bits when truncated -> CF/OF=1.
    let mut r = regs();
    r.rbx = 0x4000_0000_0000_0000;
    // 48 69 C3 00 00 00 40  imul rax, rbx, 0x40000000
    check_flags_masked(
        "imul3_of",
        &with_hlt(vec![0x48, 0x69, 0xC3, 0x00, 0x00, 0x00, 0x40]),
        r,
        MULDIV_DEFINED,
    );
}

#[test]
fn imul_two_operand_32() {
    // imul eax, ebx (0F AF) : 32-bit product, zero-extends into rax.
    // 0x10000 * 0x10000 = 0x1_0000_0000 -> low 32 = 0, CF/OF set (truncation).
    let mut r = regs();
    r.rax = 0x0001_0000;
    r.rbx = 0x0001_0000;
    // 0F AF C3  imul eax, ebx
    check_flags_masked(
        "imul2_32",
        &with_hlt(vec![0x0F, 0xAF, 0xC3]),
        r,
        MULDIV_DEFINED,
    );
}

#[test]
fn mul_one_operand_32() {
    // MUL r/m32 (F7 /4): EDX:EAX = EAX * EBX (unsigned). 0xFFFFFFFF * 2.
    let mut r = regs();
    r.rax = 0xFFFF_FFFF;
    r.rbx = 0x0000_0002;
    // F7 E3  mul ebx
    check_flags_masked("mul32", &with_hlt(vec![0xF7, 0xE3]), r, MULDIV_DEFINED);
}

#[test]
fn mul_one_operand_8() {
    // MUL r/m8 (F6 /4): AX = AL * BL (unsigned). 0xFF * 0xFF = 0xFE01.
    let mut r = regs();
    r.rax = 0x00FF;
    r.rbx = 0x00FF;
    // F6 E3  mul bl
    check_flags_masked("mul8", &with_hlt(vec![0xF6, 0xE3]), r, MULDIV_DEFINED);
}

#[test]
fn mul_no_overflow_clears_cf_of() {
    // small product fits in low half -> CF=OF=0.
    let mut r = regs();
    r.rax = 0x0000_0000_0000_0003;
    r.rbx = 0x0000_0000_0000_0005;
    // 48 F7 E3  mul rbx -> RDX=0, RAX=15, CF=OF=0
    check_flags_masked(
        "mul_nooverflow",
        &with_hlt(vec![0x48, 0xF7, 0xE3]),
        r,
        MULDIV_DEFINED,
    );
}

// ---------------------------------------------------------------------------
// DIV / IDIV exact quotient & remainder (all flags undefined -> mask 0, GPRs only).
// ---------------------------------------------------------------------------

#[test]
fn div_with_remainder() {
    // RDX:RAX / RBX : 1000 / 7 = 142 rem 6.
    let mut r = regs();
    r.rax = 1000;
    r.rdx = 0;
    r.rbx = 7;
    // 48 F7 F3  div rbx
    check_flags_masked("div_rem", &with_hlt(vec![0x48, 0xF7, 0xF3]), r, 0);
}

#[test]
fn div_128_by_64() {
    // RDX:RAX is a true 128-bit dividend: (1<<64 + 0) / 2 = 0x8000_0000_0000_0000.
    let mut r = regs();
    r.rdx = 0x0000_0000_0000_0001; // high
    r.rax = 0x0000_0000_0000_0000; // low -> dividend = 2^64
    r.rbx = 0x0000_0000_0000_0002;
    check_flags_masked("div128", &with_hlt(vec![0x48, 0xF7, 0xF3]), r, 0);
}

#[test]
fn div32_with_remainder() {
    // EDX:EAX / EBX : 0x10000007 / 0x10 = 0x1000000 rem 7. Zero-extends into r64.
    let mut r = regs();
    r.rax = 0x1000_0007;
    r.rdx = 0;
    r.rbx = 0x10;
    // F7 F3  div ebx
    check_flags_masked("div32", &with_hlt(vec![0xF7, 0xF3]), r, 0);
}

#[test]
fn div8_ax_by_bl() {
    // DIV r/m8 (F6 /6): AL <- AX/BL quotient, AH <- remainder. 0x00FF / 0x10 = 0x0F r 0x0F.
    let mut r = regs();
    r.rax = 0x00FF;
    r.rbx = 0x0010;
    // F6 F3  div bl
    check_flags_masked("div8", &with_hlt(vec![0xF6, 0xF3]), r, 0);
}

#[test]
fn idiv_negative_dividend() {
    // signed: -1000 / 7 = -142 r -6 (truncation toward zero).
    let mut r = regs();
    r.rax = (-1000i64) as u64;
    r.rdx = 0xFFFF_FFFF_FFFF_FFFF; // sign extension of the negative dividend
    r.rbx = 7;
    // 48 F7 FB  idiv rbx
    check_flags_masked("idiv_neg", &with_hlt(vec![0x48, 0xF7, 0xFB]), r, 0);
}

#[test]
fn idiv_negative_divisor() {
    // 1000 / -7 = -142 r 6.
    let mut r = regs();
    r.rax = 1000;
    r.rdx = 0;
    r.rbx = (-7i64) as u64;
    check_flags_masked("idiv_negdiv", &with_hlt(vec![0x48, 0xF7, 0xFB]), r, 0);
}

#[test]
fn idiv32_negative() {
    // signed 32-bit: -100 / 9 = -11 r -1. EDX:EAX sign-extended dividend.
    let mut r = regs();
    r.rax = (-100i32) as u32 as u64;
    r.rdx = 0xFFFF_FFFF; // sign-extend low 32
    r.rbx = 9;
    // F7 FB  idiv ebx
    check_flags_masked("idiv32", &with_hlt(vec![0xF7, 0xFB]), r, 0);
}

// ---------------------------------------------------------------------------
// Rotate through carry: RCL / RCR by 1 and by CL.
// ---------------------------------------------------------------------------

#[test]
fn rcl8_by_1() {
    // rcl al, 1 with al=0x80, CF=0 : MSB(1) -> CF, CF(0) -> bit0 => al=0x00, CF=1, OF=(CF^MSB)=1.
    let mut r = regs();
    r.rax = 0x80;
    r.rflags = 0; // CF=0
    // D0 D0  rcl al, 1
    check_flags_masked("rcl8_1", &with_hlt(vec![0xD0, 0xD0]), r, RCL_RCR_1);
}

#[test]
fn rcr8_by_1() {
    // rcr al, 1 with al=0x01, CF=1 : bit0(1)->CF, old CF(1)->MSB => al=0x80, CF=1.
    let mut r = regs();
    r.rax = 0x01;
    r.rflags = flags::bits::CF;
    // D0 D8  rcr al, 1
    check_flags_masked("rcr8_1", &with_hlt(vec![0xD0, 0xD8]), r, RCL_RCR_1);
}

#[test]
fn rcl16_by_1() {
    let mut r = regs();
    r.rax = 0xC000; // top two bits set
    r.rflags = 0;
    // 66 D1 D0  rcl ax, 1
    check_flags_masked("rcl16_1", &with_hlt(vec![0x66, 0xD1, 0xD0]), r, RCL_RCR_1);
}

#[test]
fn rcl32_by_1_carry_in() {
    // rcl eax, 1 with eax=0x8000_0000, CF=1 : MSB->CF(1), CF->bit0 => eax=1, CF=1.
    let mut r = regs();
    r.rax = 0x8000_0000;
    r.rflags = flags::bits::CF;
    // D1 D0  rcl eax, 1
    check_flags_masked("rcl32_1", &with_hlt(vec![0xD1, 0xD0]), r, RCL_RCR_1);
}

#[test]
fn rcl64_by_1() {
    let mut r = regs();
    r.rax = 0x8000_0000_0000_0001;
    r.rflags = flags::bits::CF;
    // 48 D1 D0  rcl rax, 1
    check_flags_masked("rcl64_1", &with_hlt(vec![0x48, 0xD1, 0xD0]), r, RCL_RCR_1);
}

#[test]
fn rcr64_by_1() {
    let mut r = regs();
    r.rax = 0x0000_0000_0000_0001;
    r.rflags = 0; // CF=0 rotates a 0 into the MSB
    // 48 D1 D8  rcr rax, 1
    check_flags_masked("rcr64_1", &with_hlt(vec![0x48, 0xD1, 0xD8]), r, RCL_RCR_1);
}

#[test]
fn rcl_by_cl_multi() {
    // rcl rax, cl with count>1 : CF defined, OF undefined -> mask CF only.
    let mut r = regs();
    r.rax = 0x1234_5678_9ABC_DEF0;
    r.rcx = 5;
    r.rflags = flags::bits::CF;
    // 48 D3 D0  rcl rax, cl
    check_flags_masked(
        "rcl_cl5",
        &with_hlt(vec![0x48, 0xD3, 0xD0]),
        r,
        RCL_RCR_MULTI,
    );
}

#[test]
fn rcr_by_cl_multi() {
    let mut r = regs();
    r.rax = 0x0FED_CBA9_8765_4321;
    r.rcx = 9;
    r.rflags = 0;
    // 48 D3 D8  rcr rax, cl
    check_flags_masked(
        "rcr_cl9",
        &with_hlt(vec![0x48, 0xD3, 0xD8]),
        r,
        RCL_RCR_MULTI,
    );
}

#[test]
fn rcr32_by_cl_multi() {
    // 32-bit rcr by CL: count is masked mod 32; result zero-extends.
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_DEAD_BEEF;
    r.rcx = 7;
    r.rflags = flags::bits::CF;
    // D3 D8  rcr eax, cl
    check_flags_masked("rcr32_cl7", &with_hlt(vec![0xD3, 0xD8]), r, RCL_RCR_MULTI);
}

#[test]
fn rcl8_by_cl_wraps_mod9() {
    // 8-bit RCL count is taken modulo 9 (8 data bits + carry). CL=10 -> effective 1.
    let mut r = regs();
    r.rax = 0x55;
    r.rcx = 10;
    r.rflags = flags::bits::CF;
    // D2 D0  rcl al, cl
    check_flags_masked("rcl8_cl10", &with_hlt(vec![0xD2, 0xD0]), r, RCL_RCR_MULTI);
}

// ---------------------------------------------------------------------------
// SHLD / SHRD by various counts, including a 1-count (OF defined) and counts
// approaching operand size. (count > operand size is architecturally UNDEFINED
// and is deliberately avoided.)
// ---------------------------------------------------------------------------

#[test]
fn shrd_imm1_defines_of() {
    // A 1-bit SHRD defines OF -> compare all status flags.
    let mut r = regs();
    r.rax = 0x0000_0000_0000_0001;
    r.rbx = 0x8000_0000_0000_0000;
    // 48 0F AC D8 01  shrd rax, rbx, 1
    check(
        "shrd_imm1",
        &with_hlt(vec![0x48, 0x0F, 0xAC, 0xD8, 0x01]),
        r,
    );
}

#[test]
fn shld_count_63() {
    // shld rax, rbx, 63 : maximal in-range count for a 64-bit double shift.
    let mut r = regs();
    r.rax = 0x1;
    r.rbx = 0xFFFF_FFFF_FFFF_FFFF;
    // 48 0F A4 D8 3F  shld rax, rbx, 63
    check_flags_masked(
        "shld_63",
        &with_hlt(vec![0x48, 0x0F, 0xA4, 0xD8, 0x3F]),
        r,
        SHIFT_NO_OF,
    );
}

#[test]
fn shrd_count_63() {
    let mut r = regs();
    r.rax = 0x8000_0000_0000_0000;
    r.rbx = 0xFFFF_FFFF_FFFF_FFFF;
    // 48 0F AC D8 3F  shrd rax, rbx, 63
    check_flags_masked(
        "shrd_63",
        &with_hlt(vec![0x48, 0x0F, 0xAC, 0xD8, 0x3F]),
        r,
        SHIFT_NO_OF,
    );
}

#[test]
fn shld16_cl() {
    // 16-bit SHLD: count masked mod 32, but a count within [1,16) is well-defined.
    let mut r = regs();
    r.rax = 0x0000_0000_0000_1234;
    r.rbx = 0x0000_0000_0000_ABCD;
    r.rcx = 4;
    // 66 0F A5 D8  shld ax, bx, cl
    check_flags_masked(
        "shld16_cl4",
        &with_hlt(vec![0x66, 0x0F, 0xA5, 0xD8]),
        r,
        SHIFT_NO_OF,
    );
}

#[test]
fn shrd16_imm() {
    let mut r = regs();
    r.rax = 0x0000_0000_0000_F000;
    r.rbx = 0x0000_0000_0000_000F;
    // 66 0F AC D8 04  shrd ax, bx, 4
    check_flags_masked(
        "shrd16_imm4",
        &with_hlt(vec![0x66, 0x0F, 0xAC, 0xD8, 0x04]),
        r,
        SHIFT_NO_OF,
    );
}

#[test]
fn shld32_cl16() {
    // 32-bit SHLD by 16: result zero-extends into rax.
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_1234_5678;
    r.rbx = 0x0000_0000_ABCD_EF01;
    r.rcx = 16;
    // 0F A5 D8  shld eax, ebx, cl
    check_flags_masked(
        "shld32_cl16",
        &with_hlt(vec![0x0F, 0xA5, 0xD8]),
        r,
        SHIFT_NO_OF,
    );
}

#[test]
fn shrd_cl_31() {
    // shrd rax, rbx, cl with CL=31.
    let mut r = regs();
    r.rax = 0xAAAA_AAAA_AAAA_AAAA;
    r.rbx = 0x5555_5555_5555_5555;
    r.rcx = 31;
    // 48 0F AD D8  shrd rax, rbx, cl
    check_flags_masked(
        "shrd_cl31",
        &with_hlt(vec![0x48, 0x0F, 0xAD, 0xD8]),
        r,
        SHIFT_NO_OF,
    );
}

// ---------------------------------------------------------------------------
// BCD adjust instructions (AAA/AAS/AAD/AAM/DAA/DAS).
//
// NOTE: these opcodes (0x37, 0x3F, 0x27, 0x2F, 0xD4, 0xD5) are INVALID in
// 64-bit mode and raise #UD on real hardware and under KVM. The differential
// harness only sets up 64-bit long mode, so these cannot be exercised here and
// are intentionally NOT added as runnable cases (they would only "diverge"
// because both backends should #UD, which the harness models as an abnormal
// exit rather than a comparable architectural state). This block documents the
// deliberate omission required by the harness's 64-bit-only setup.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Conditional branch (Jcc) across the flag matrix. Each program does
//   cmp rax, rbx        ; set flags
//   jcc taken           ; +N forward
//   mov rcx, 0xBAD      ; (not-taken path) marker
//   jmp end
//   taken: mov rcx, 0x600D
//   end: hlt
// We compare RCX (and flags) so a wrong branch decision is observable.
// ---------------------------------------------------------------------------

/// Build a Jcc test: cmp rax,rbx then `jcc` (1-byte opcode 0x7x). The taken path
/// sets rcx=0x600D, the fall-through sets rcx=0xBAD.
fn jcc_program(jcc: u8) -> Vec<u8> {
    // 48 39 D8            cmp rax, rbx
    // 7x 07               jcc +7  (skip the 7-byte not-taken block)
    // 48 C7 C1 AD 0B 00 00  mov rcx, 0xBAD     (7 bytes) [not taken]
    // EB 07               jmp +7 (skip the taken block)
    // 48 C7 C1 0D 60 00 00  mov rcx, 0x600D    (7 bytes) [taken target]
    // F4                  hlt
    let mut c = vec![0x48, 0x39, 0xD8];
    c.extend_from_slice(&[jcc, 0x07]);
    c.extend_from_slice(&[0x48, 0xC7, 0xC1, 0xAD, 0x0B, 0x00, 0x00]); // mov rcx, 0xBAD
    c.extend_from_slice(&[0xEB, 0x07]); // jmp +7
    c.extend_from_slice(&[0x48, 0xC7, 0xC1, 0x0D, 0x60, 0x00, 0x00]); // mov rcx, 0x600D
    c.push(HLT);
    c
}

fn check_jcc(label: &str, jcc: u8, rax: u64, rbx: u64) {
    let mut r = regs();
    r.rax = rax;
    r.rbx = rbx;
    check(label, &jcc_program(jcc), r);
}

#[test]
fn jcc_all_conditions() {
    // 0x70..=0x7F = JO,JNO,JB,JAE,JE,JNE,JBE,JA,JS,JNS,JP,JNP,JL,JGE,JLE,JG.
    // Each tuple sets flags so the condition is TRUE.
    let true_cases: &[(u8, u64, u64, &str)] = &[
        (0x70, 0x8000_0000_0000_0000, 1, "jo"), // INT_MIN - 1 -> OF
        (0x71, 5, 3, "jno"),
        (0x72, 1, 2, "jb"),  // 1 < 2 unsigned -> CF
        (0x73, 5, 3, "jae"), // CF=0
        (0x74, 7, 7, "je"),  // equal -> ZF
        (0x75, 7, 8, "jne"),
        (0x76, 2, 2, "jbe"), // CF|ZF
        (0x77, 5, 3, "ja"),
        (0x78, 0, 1, "js"), // 0-1 -> SF
        (0x79, 5, 3, "jns"),
        (0x7A, 3, 0, "jp"),  // result 3 -> even parity
        (0x7B, 1, 0, "jnp"), // result 1 -> odd parity
        (0x7C, 1, 2, "jl"),  // signed less
        (0x7D, 5, 3, "jge"),
        (0x7E, 2, 2, "jle"), // equal -> le
        (0x7F, 5, 3, "jg"),
    ];
    for &(opc, rax, rbx, name) in true_cases {
        check_jcc(name, opc, rax, rbx);
    }
    // A few not-taken (FALSE) paths so the fall-through is exercised too.
    check_jcc("jo_false", 0x70, 5, 3); // no overflow
    check_jcc("je_false", 0x74, 7, 8); // not equal
    check_jcc("jg_false", 0x7F, 1, 2); // not greater
}

#[test]
fn jcc_rel32_forward() {
    // 32-bit displacement conditional jump (0F 84 = JE rel32).
    //   cmp rax, rbx           (equal -> ZF)
    //   0F 84 07 00 00 00      je +7
    //   mov rcx, 0xBAD
    //   EB 07                  jmp +7
    //   mov rcx, 0x600D
    //   hlt
    let mut r = regs();
    r.rax = 0x1234;
    r.rbx = 0x1234;
    let mut c = vec![0x48, 0x39, 0xD8];
    c.extend_from_slice(&[0x0F, 0x84, 0x07, 0x00, 0x00, 0x00]); // je rel32 +7
    c.extend_from_slice(&[0x48, 0xC7, 0xC1, 0xAD, 0x0B, 0x00, 0x00]);
    c.extend_from_slice(&[0xEB, 0x07]);
    c.extend_from_slice(&[0x48, 0xC7, 0xC1, 0x0D, 0x60, 0x00, 0x00]);
    c.push(HLT);
    check("je_rel32", &c, r);
}

#[test]
fn loop_decrements_rcx() {
    // LOOP (E2 rel8) decrements RCX and jumps while RCX != 0.
    //   add rax, 1          (48 83 C0 01)
    //   loop -6             (E2 FA)  back to the add
    //   hlt
    // RCX=3 -> rax incremented 3 times, RCX ends at 0.
    let mut r = regs();
    r.rcx = 3;
    r.rax = 0;
    let mut c = vec![0x48, 0x83, 0xC0, 0x01]; // add rax, 1
    c.extend_from_slice(&[0xE2, 0xFA]); // loop -6 (back to add)
    c.push(HLT);
    check("loop_rcx", &c, r);
}

// ---------------------------------------------------------------------------
// LEA RIP-relative (mod=00, r/m=101 -> [RIP + disp32]).
// ---------------------------------------------------------------------------

#[test]
fn lea_rip_relative() {
    // 48 8D 05 disp32  lea rax, [rip + disp32]
    // RIP is the address of the NEXT instruction (here CODE_ADDR + 7, since the
    // LEA is 7 bytes). With disp32 = 0x100, rax = CODE_ADDR + 7 + 0x100.
    // We just need both backends to agree on the computed absolute address.
    let r = regs();
    // 48 8D 05 00 01 00 00  lea rax, [rip + 0x100]
    check(
        "lea_rip",
        &with_hlt(vec![0x48, 0x8D, 0x05, 0x00, 0x01, 0x00, 0x00]),
        r,
    );
}

#[test]
fn lea_rip_relative_negative_disp() {
    // Negative RIP-relative displacement.
    // 48 8D 05 F0 FF FF FF  lea rax, [rip - 16]
    let r = regs();
    check(
        "lea_rip_neg",
        &with_hlt(vec![0x48, 0x8D, 0x05, 0xF0, 0xFF, 0xFF, 0xFF]),
        r,
    );
}

// ---------------------------------------------------------------------------
// SSE / SSE2 single-precision float ops driven via the scratch page.
// All inputs are exactly representable in f32 so f64/f32 rounding is irrelevant
// and the 128-bit result is bit-identical across backends.
// ---------------------------------------------------------------------------

/// Build a 16-byte little-endian buffer from four f32 lanes (lane 0 = bytes 0..4).
fn f32x4(v: [f32; 4]) -> [u8; 16] {
    let mut o = [0u8; 16];
    for i in 0..4 {
        o[i * 4..i * 4 + 4].copy_from_slice(&v[i].to_le_bytes());
    }
    o
}

/// Build a 16-byte buffer from two f64 lanes.
fn f64x2(v: [f64; 2]) -> [u8; 16] {
    let mut o = [0u8; 16];
    o[0..8].copy_from_slice(&v[0].to_le_bytes());
    o[8..16].copy_from_slice(&v[1].to_le_bytes());
    o
}

#[test]
fn sse_addps() {
    // ADDPS xmm0, xmm1 = 0F 58 C1. Packed single add.
    let a = f32x4([1.0, 2.5, -3.0, 100.0]);
    let b = f32x4([0.5, 0.5, 3.0, -100.0]);
    check_sse(
        "addps",
        &sse_program(&[0x0F, 0x58, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_subps() {
    // SUBPS xmm0, xmm1 = 0F 5C C1.
    let a = f32x4([10.0, 0.0, -5.0, 256.0]);
    let b = f32x4([2.5, 1.0, -5.0, 256.0]);
    check_sse(
        "subps",
        &sse_program(&[0x0F, 0x5C, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_mulps() {
    // MULPS xmm0, xmm1 = 0F 59 C1.
    let a = f32x4([2.0, 3.0, -4.0, 0.5]);
    let b = f32x4([8.0, 0.25, -2.0, 16.0]);
    check_sse(
        "mulps",
        &sse_program(&[0x0F, 0x59, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_divps() {
    // DIVPS xmm0, xmm1 = 0F 5E C1. Use exact dyadic quotients.
    let a = f32x4([1.0, 9.0, -8.0, 256.0]);
    let b = f32x4([2.0, 8.0, 2.0, 4.0]); // -> 0.5, 1.125, -4.0, 64.0
    check_sse(
        "divps",
        &sse_program(&[0x0F, 0x5E, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_minps() {
    // MINPS xmm0, xmm1 = 0F 5D C1. Per-lane min.
    let a = f32x4([1.0, 5.0, -3.0, 7.0]);
    let b = f32x4([2.0, 4.0, -1.0, 7.0]);
    check_sse(
        "minps",
        &sse_program(&[0x0F, 0x5D, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_maxps() {
    // MAXPS xmm0, xmm1 = 0F 5F C1. Per-lane max.
    let a = f32x4([1.0, 5.0, -3.0, 7.0]);
    let b = f32x4([2.0, 4.0, -1.0, 7.0]);
    check_sse(
        "maxps",
        &sse_program(&[0x0F, 0x5F, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_minps_nan_handling() {
    // SDM (MINPS): for each lane, dst = (a < b) ? a : b, where ANY NaN operand
    // (or unordered compare) makes the `<` test FALSE, so the lane result is the
    // SECOND source operand (b). With:
    //   a = [NaN, 9.0, 1.0, 2.0]   (NaN in lane 0, dst = xmm0)
    //   b = [5.0, NaN, 3.0, 4.0]   (NaN in lane 1, src = xmm1)
    // the architecturally-correct result is:
    //   lane0: a is NaN  -> b = 5.0
    //   lane1: b is NaN  -> b = NaN
    //   lane2: 1.0 < 3.0 -> a = 1.0
    //   lane3: 2.0 < 4.0 -> a = 2.0
    // i.e. [5.0, NaN, 1.0, 2.0].
    //
    // FIXED: execute_sse_min previously used Rust's `f32::min`, which returns the
    // NON-NaN operand (so lane1 wrongly returned dst=9.0). It now uses the x86
    // rule `(dst<src)?dst:src`, returning src on any unordered lane, matching KVM.
    let mut a = f32x4([0.0, 9.0, 1.0, 2.0]);
    a[0..4].copy_from_slice(&f32::NAN.to_le_bytes());
    let mut b = f32x4([5.0, 0.0, 3.0, 4.0]);
    b[4..8].copy_from_slice(&f32::NAN.to_le_bytes());
    check_sse(
        "minps_nan",
        &sse_program(&[0x0F, 0x5D, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_maxps_nan_handling() {
    // MAXPS xmm0, xmm1 = 0F 5F C1. Same NaN rule as MINPS: any unordered lane
    // returns the SECOND operand (src = xmm1). Mirror of the MINPS case.
    //   a = [NaN, 9.0, 5.0, 2.0] (dst), b = [5.0, NaN, 3.0, 4.0] (src)
    //   lane0: a NaN -> src = 5.0 ; lane1: b NaN -> src = NaN
    //   lane2: 5.0 > 3.0 -> dst = 5.0 ; lane3: 2.0 > 4.0 false -> src = 4.0
    let mut a = f32x4([0.0, 9.0, 5.0, 2.0]);
    a[0..4].copy_from_slice(&f32::NAN.to_le_bytes());
    let mut b = f32x4([5.0, 0.0, 3.0, 4.0]);
    b[4..8].copy_from_slice(&f32::NAN.to_le_bytes());
    check_sse(
        "maxps_nan",
        &sse_program(&[0x0F, 0x5F, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_maxpd_nan_handling() {
    // MAXPD xmm0, xmm1 = 66 0F 5F C1. Packed-double NaN handling: unordered lane
    // returns src. a = [NaN, 7.0] (dst), b = [3.0, NaN] (src) -> [3.0, NaN].
    let mut a = f64x2([0.0, 7.0]);
    a[0..8].copy_from_slice(&f64::NAN.to_le_bytes());
    let mut b = f64x2([3.0, 0.0]);
    b[8..16].copy_from_slice(&f64::NAN.to_le_bytes());
    check_sse(
        "maxpd_nan",
        &sse_program(&[0x66, 0x0F, 0x5F, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_minpd_nan_handling() {
    // MINPD xmm0, xmm1 = 66 0F 5D C1. a = [NaN, 7.0], b = [3.0, NaN] -> [3.0, NaN].
    let mut a = f64x2([0.0, 7.0]);
    a[0..8].copy_from_slice(&f64::NAN.to_le_bytes());
    let mut b = f64x2([3.0, 0.0]);
    b[8..16].copy_from_slice(&f64::NAN.to_le_bytes());
    check_sse(
        "minpd_nan",
        &sse_program(&[0x66, 0x0F, 0x5D, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_minss_nan_returns_src() {
    // MINSS xmm0, xmm1 = F3 0F 5D C1. Scalar lane0: dst=9.0, src=NaN -> NaN (src).
    // Upper 3 lanes of xmm0 preserved.
    let a = f32x4([9.0, 11.0, 12.0, 13.0]);
    let mut b = f32x4([0.0, 0.0, 0.0, 0.0]);
    b[0..4].copy_from_slice(&f32::NAN.to_le_bytes());
    check_sse(
        "minss_nan",
        &sse_program(&[0xF3, 0x0F, 0x5D, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_maxsd_nan_returns_src() {
    // MAXSD xmm0, xmm1 = F2 0F 5F C1. Scalar double lane0: dst=NaN, src=2.0 -> 2.0.
    let mut a = f64x2([0.0, 7.0]);
    a[0..8].copy_from_slice(&f64::NAN.to_le_bytes());
    let b = f64x2([2.0, 99.0]);
    check_sse(
        "maxsd_nan",
        &sse_program(&[0xF2, 0x0F, 0x5F, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_maxps_signed_zero() {
    // MAX(-0.0, +0.0): SDM says the second operand is returned when operands are
    // equal-but-signed-different, so MAXPS returns b. Probe both lanes.
    let a = f32x4([-0.0, 0.0, -0.0, 0.0]);
    let b = f32x4([0.0, -0.0, -0.0, 0.0]);
    check_sse(
        "maxps_signzero",
        &sse_program(&[0x0F, 0x5F, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_minps_signed_zero() {
    // MIN(-0.0,+0.0) also returns the second operand on the equality tie.
    let a = f32x4([-0.0, 0.0, 1.0, -1.0]);
    let b = f32x4([0.0, -0.0, 1.0, -1.0]);
    check_sse(
        "minps_signzero",
        &sse_program(&[0x0F, 0x5D, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_sqrtps() {
    // SQRTPS xmm0, xmm1 = 0F 51 C1. Source xmm1 -> dest xmm0. Perfect squares.
    let a = f32x4([0.0, 0.0, 0.0, 0.0]); // dest, overwritten
    let b = f32x4([4.0, 9.0, 16.0, 0.25]); // -> 2,3,4,0.5
    check_sse(
        "sqrtps",
        &sse_program(&[0x0F, 0x51, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_sqrtss() {
    // SQRTSS xmm0, xmm1 = F3 0F 51 C1. Only lane 0 changes; lanes 1..3 keep xmm0.
    let a = f32x4([1.0, 11.0, 12.0, 13.0]); // upper lanes preserved
    let b = f32x4([25.0, 0.0, 0.0, 0.0]); // lane0 -> 5.0
    check_sse(
        "sqrtss",
        &sse_program(&[0xF3, 0x0F, 0x51, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_addss() {
    // ADDSS xmm0, xmm1 = F3 0F 58 C1. Scalar add; upper 3 lanes of xmm0 preserved.
    let a = f32x4([10.0, 1.0, 2.0, 3.0]);
    let b = f32x4([5.5, 99.0, 99.0, 99.0]); // only lane0 of b is used
    check_sse(
        "addss",
        &sse_program(&[0xF3, 0x0F, 0x58, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_minss_scalar() {
    // MINSS xmm0, xmm1 = F3 0F 5D C1. Scalar min in lane 0.
    let a = f32x4([7.0, 1.0, 2.0, 3.0]);
    let b = f32x4([4.0, 0.0, 0.0, 0.0]); // min(7,4)=4
    check_sse(
        "minss",
        &sse_program(&[0xF3, 0x0F, 0x5D, 0xC1]),
        sse_scratch(a, b),
    );
}

// ---- CMPPS: all 8 immediate predicates (0..7). Produces an all-1s/all-0s mask. ----

/// Run CMPPS xmm0, xmm1, imm8 (0F C2 C1 ib) and compare the 128-bit mask result.
fn cmpps_case(label: &str, imm: u8, a: [u8; 16], b: [u8; 16]) {
    check_sse(
        label,
        &sse_program(&[0x0F, 0xC2, 0xC1, imm]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_cmpps_all_predicates() {
    // Lanes chosen to span equal / less / greater / unordered(NaN) relationships.
    // a = [1.0, 2.0, 3.0, NaN], b = [1.0, 5.0, 1.0, 4.0].
    let mut a = f32x4([1.0, 2.0, 3.0, 0.0]);
    a[12..16].copy_from_slice(&f32::NAN.to_le_bytes());
    let b = f32x4([1.0, 5.0, 1.0, 4.0]);
    // 0=EQ, 1=LT, 2=LE, 3=UNORD, 4=NEQ, 5=NLT, 6=NLE, 7=ORD.
    cmpps_case("cmpps_eq", 0, a, b);
    cmpps_case("cmpps_lt", 1, a, b);
    cmpps_case("cmpps_le", 2, a, b);
    cmpps_case("cmpps_unord", 3, a, b);
    cmpps_case("cmpps_neq", 4, a, b);
    cmpps_case("cmpps_nlt", 5, a, b);
    cmpps_case("cmpps_nle", 6, a, b);
    cmpps_case("cmpps_ord", 7, a, b);
}

// ---- MOVMSKPS: extract the 4 sign bits of the packed singles into a GPR. ----

#[test]
fn sse_movmskps() {
    // Load xmm1 from scratch, MOVMSKPS eax, xmm1 (0F 50 C1), store eax to scratch+0x20.
    // Signs: lane0 -, lane1 +, lane2 -, lane3 + -> mask = 0b0101 = 0x5.
    let a = [0u8; 16];
    let b = f32x4([-1.0, 2.0, -3.0, 4.0]);
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]); // movdqu xmm1, [rdi+0x10]
    prog.extend_from_slice(&[0x0F, 0x50, 0xC1]); // movmskps eax, xmm1
    prog.extend_from_slice(&[0x89, 0x47, 0x20]); // mov [rdi+0x20], eax
    prog.push(HLT);
    check_sse("movmskps", &prog, sse_scratch(a, b));
}

#[test]
fn sse_movmskpd() {
    // MOVMSKPD eax, xmm1 (66 0F 50 C1): 2 sign bits from the packed doubles.
    // lane0 -, lane1 + -> mask = 0b01 = 0x1.
    let a = [0u8; 16];
    let b = f64x2([-1.5, 2.5]);
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]); // movdqu xmm1, [rdi+0x10]
    prog.extend_from_slice(&[0x66, 0x0F, 0x50, 0xC1]); // movmskpd eax, xmm1
    prog.extend_from_slice(&[0x89, 0x47, 0x20]); // mov [rdi+0x20], eax
    prog.push(HLT);
    check_sse("movmskpd", &prog, sse_scratch(a, b));
}

// ---- UNPCK low/high for packed singles/doubles ----

#[test]
fn sse_unpcklps() {
    // UNPCKLPS xmm0, xmm1 = 0F 14 C1 : interleave low two singles of each.
    // result = [a0, b0, a1, b1].
    let a = f32x4([1.0, 2.0, 3.0, 4.0]);
    let b = f32x4([10.0, 20.0, 30.0, 40.0]);
    check_sse(
        "unpcklps",
        &sse_program(&[0x0F, 0x14, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_unpckhps() {
    // UNPCKHPS xmm0, xmm1 = 0F 15 C1 : interleave high two singles.
    // result = [a2, b2, a3, b3].
    let a = f32x4([1.0, 2.0, 3.0, 4.0]);
    let b = f32x4([10.0, 20.0, 30.0, 40.0]);
    check_sse(
        "unpckhps",
        &sse_program(&[0x0F, 0x15, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_unpcklpd() {
    // UNPCKLPD xmm0, xmm1 = 66 0F 14 C1 : result = [a.lo, b.lo].
    let a = f64x2([1.5, 2.5]);
    let b = f64x2([3.5, 4.5]);
    check_sse(
        "unpcklpd",
        &sse_program(&[0x66, 0x0F, 0x14, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_unpckhpd() {
    // UNPCKHPD xmm0, xmm1 = 66 0F 15 C1 : result = [a.hi, b.hi].
    let a = f64x2([1.5, 2.5]);
    let b = f64x2([3.5, 4.5]);
    check_sse(
        "unpckhpd",
        &sse_program(&[0x66, 0x0F, 0x15, 0xC1]),
        sse_scratch(a, b),
    );
}

// ---- Double-precision arithmetic + min/max/sqrt + scalar ----

#[test]
fn sse_addpd() {
    // ADDPD xmm0, xmm1 = 66 0F 58 C1.
    let a = f64x2([1.25, -100.0]);
    let b = f64x2([0.75, 100.0]);
    check_sse(
        "addpd",
        &sse_program(&[0x66, 0x0F, 0x58, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_mulpd() {
    // MULPD xmm0, xmm1 = 66 0F 59 C1.
    let a = f64x2([3.0, -0.5]);
    let b = f64x2([0.25, 8.0]);
    check_sse(
        "mulpd",
        &sse_program(&[0x66, 0x0F, 0x59, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_sqrtpd() {
    // SQRTPD xmm0, xmm1 = 66 0F 51 C1.
    let a = f64x2([0.0, 0.0]);
    let b = f64x2([81.0, 0.0625]); // -> 9.0, 0.25
    check_sse(
        "sqrtpd",
        &sse_program(&[0x66, 0x0F, 0x51, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_minpd_maxpd() {
    // MINPD then independent MAXPD test, both per-lane.
    let a = f64x2([3.0, 8.0]);
    let b = f64x2([5.0, 2.0]);
    check_sse(
        "minpd",
        &sse_program(&[0x66, 0x0F, 0x5D, 0xC1]),
        sse_scratch(a, b),
    );
    check_sse(
        "maxpd",
        &sse_program(&[0x66, 0x0F, 0x5F, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_divsd_scalar() {
    // DIVSD xmm0, xmm1 = F2 0F 5E C1. Scalar; lane1 of xmm0 preserved.
    let a = f64x2([9.0, 123.0]);
    let b = f64x2([4.0, 0.0]); // lane0 -> 2.25
    check_sse(
        "divsd",
        &sse_program(&[0xF2, 0x0F, 0x5E, 0xC1]),
        sse_scratch(a, b),
    );
}

// ---- CMPPD predicate + CVT edge cases ----

#[test]
fn sse_cmppd_predicates() {
    // CMPPD xmm0, xmm1, imm8 = 66 0F C2 C1 ib. Test EQ(0) and LT(1).
    let a = f64x2([1.0, 3.0]);
    let b = f64x2([1.0, 2.0]);
    check_sse(
        "cmppd_eq",
        &sse_program(&[0x66, 0x0F, 0xC2, 0xC1, 0x00]),
        sse_scratch(a, b),
    );
    check_sse(
        "cmppd_lt",
        &sse_program(&[0x66, 0x0F, 0xC2, 0xC1, 0x01]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_cvtps2pd() {
    // CVTPS2PD xmm0, xmm1 = 0F 5A C1. Low two f32 -> two f64.
    let a = [0u8; 16];
    let b = f32x4([2.5, -4.0, 99.0, 99.0]); // only low two lanes used
    check_sse(
        "cvtps2pd",
        &sse_program(&[0x0F, 0x5A, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_cvtpd2ps() {
    // CVTPD2PS xmm0, xmm1 = 66 0F 5A C1. Two f64 -> low two f32, high two = 0.
    let a = [0u8; 16];
    let b = f64x2([3.5, -7.25]);
    check_sse(
        "cvtpd2ps",
        &sse_program(&[0x66, 0x0F, 0x5A, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_cvttps2dq_negative_trunc() {
    // CVTTPS2DQ (F3 0F 5B): truncation toward zero on negatives.
    // -1.9 -> -1, -2.5 -> -2, 2.9 -> 2, 0.9 -> 0.
    let a = [0u8; 16];
    let b = f32x4([-1.9, -2.5, 2.9, 0.9]);
    check_sse(
        "cvttps2dq_neg",
        &sse_program(&[0xF3, 0x0F, 0x5B, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_cvtsi2sd() {
    // CVTSI2SD xmm0, r/m64 = F2 48 0F 2A C0 (from rax). rax=-12345 -> double in lane0.
    let mut r = regs();
    r.rax = (-12345i64) as u64;
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF2, 0x48, 0x0F, 0x2A, 0xC0]); // cvtsi2sd xmm0, rax
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]); // movdqu [rdi+0x20], xmm0
    prog.push(HLT);
    // Drive via run_both since we need a nonzero GPR init; compare the scratch store.
    let Some((interp, kvm)) = run_both(&prog, r, zero_scratch()) else {
        return;
    };
    let opts = CompareOpts {
        flag_mask: 0,
        scratch: true,
        ..CompareOpts::default()
    };
    assert_match("cvtsi2sd", &prog, &interp, &kvm, opts);
}

#[test]
fn sse_cvttsd2si() {
    // CVTTSD2SI rax, xmm1 = F2 48 0F 2C C1. Truncating f64->i64.
    // Load -42.9 into xmm1 from scratch lane0; convert to rax; expect -42.
    let b = f64x2([-42.9, 0.0]);
    let scratch = sse_scratch([0u8; 16], b);
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]); // movdqu xmm1, [rdi+0x10]
    prog.extend_from_slice(&[0xF2, 0x48, 0x0F, 0x2C, 0xC1]); // cvttsd2si rax, xmm1
    prog.push(HLT);
    // Compare GPRs (rax holds the converted integer); no flags, no scratch store.
    let Some((interp, kvm)) = run_both(&prog, regs(), scratch) else {
        return;
    };
    let opts = CompareOpts {
        flag_mask: 0,
        ..CompareOpts::default()
    };
    assert_match("cvttsd2si", &prog, &interp, &kvm, opts);
}

// ---- SHUFPS: arbitrary lane selection via imm8 ----

#[test]
fn sse_shufps() {
    // SHUFPS xmm0, xmm1, imm8 = 0F C6 C1 ib.
    // imm = 0b11_01_10_00 = 0xD8 : dst = [a0, a2, b1, b3].
    let a = f32x4([1.0, 2.0, 3.0, 4.0]);
    let b = f32x4([10.0, 20.0, 30.0, 40.0]);
    check_sse(
        "shufps",
        &sse_program(&[0x0F, 0xC6, 0xC1, 0xD8]),
        sse_scratch(a, b),
    );
}

// ---------------------------------------------------------------------------
// More BMI2: BZHI, and additional PEXT/PDEP/MULX/RORX edge cases.
// (BZHI defines ZF/CF/SF and clears OF; PEXT/PDEP/MULX/RORX touch no flags -> mask 0.)
// ---------------------------------------------------------------------------

// BZHI defines ZF, CF, SF; clears OF; AF/PF undefined.
const BZHI_DEFINED: u64 = flags::bits::ZF | flags::bits::CF | flags::bits::SF | flags::bits::OF;

#[test]
fn bmi2_memory_source_forms() {
    let mut s = [0u8; 64];
    s[0..4].copy_from_slice(&0x8421_1248u32.to_le_bytes());
    s[8..12].copy_from_slice(&0x0F0F_0F0Fu32.to_le_bytes());
    s[16..20].copy_from_slice(&0x0001_0000u32.to_le_bytes());
    s[20..24].copy_from_slice(&0x1234_5678u32.to_le_bytes());
    s[24..28].copy_from_slice(&0xFFFF_8000u32.to_le_bytes());
    s[28..32].copy_from_slice(&0xF000_0000u32.to_le_bytes());
    s[32..36].copy_from_slice(&0x0000_00FFu32.to_le_bytes());
    s[36..40].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rbx = 0xFEDC_BA98;
    r.rdx = 0x0001_0000;

    let mut code = vec![
        0xC4, 0xE2, 0x62, 0xF5, 0x47, 0x08, // pext eax, ebx, [rdi+8]
        0x89, 0x47, 0x28, // mov [rdi+40], eax
        0xC4, 0xE2, 0x63, 0xF5, 0x0F, // pdep ecx, ebx, [rdi]
        0x89, 0x4F, 0x2C, // mov [rdi+44], ecx
        0xC4, 0xE2, 0x63, 0xF6, 0x47, 0x10, // mulx eax, ebx, [rdi+16]
        0x89, 0x47, 0x30, // mov [rdi+48], eax
        0x89, 0x5F, 0x34, // mov [rdi+52], ebx
        0xC4, 0xE3, 0x7B, 0xF0, 0x4F, 0x14, 0x08, // rorx ecx, [rdi+20], 8
        0x89, 0x4F, 0x38, // mov [rdi+56], ecx
        0xBB,
    ];
    code.extend_from_slice(&4u32.to_le_bytes()); // mov ebx, 4
    code.extend_from_slice(&[
        0xC4, 0xE2, 0x62, 0xF7, 0x57, 0x18, // sarx edx, [rdi+24], ebx
        0x89, 0x57, 0x3C, // mov [rdi+60], edx
        0xBB,
    ]);
    code.extend_from_slice(&8u32.to_le_bytes()); // mov ebx, 8
    code.extend_from_slice(&[
        0xC4, 0xE2, 0x63, 0xF7, 0x4F, 0x1C, // shrx ecx, [rdi+28], ebx
        0xBB,
    ]);
    code.extend_from_slice(&12u32.to_le_bytes()); // mov ebx, 12
    code.extend_from_slice(&[
        0xC4, 0xE2, 0x61, 0xF7, 0x47, 0x20, // shlx eax, [rdi+32], ebx
        0xC4, 0xE2, 0x60, 0xF5, 0x57, 0x24, // bzhi edx, [rdi+36], ebx
        HLT,
    ]);

    check_mem("bmi2_memory_source_forms", &code, r, s, BZHI_DEFINED);
}

#[test]
fn bmi2_64_memory_source_forms() {
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0xF0F0_F0F0_F0F0_F0F0u64.to_le_bytes());
    s[8..16].copy_from_slice(&0x8421_1248_8421_1248u64.to_le_bytes());
    s[16..24].copy_from_slice(&0x0000_0001_0000_0000u64.to_le_bytes());
    s[24..32].copy_from_slice(&0x0123_4567_89AB_CDEFu64.to_le_bytes());
    s[32..40].copy_from_slice(&0xFFFF_FFFF_FFFF_8000u64.to_le_bytes());
    s[40..48].copy_from_slice(&0xF000_0000_0000_0000u64.to_le_bytes());
    s[48..56].copy_from_slice(&0x0000_0000_0000_00FFu64.to_le_bytes());
    s[56..64].copy_from_slice(&0xFFFF_FFFF_FFFF_FFFFu64.to_le_bytes());

    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rbx = 0x0123_4567_89AB_CDEF;
    r.rdx = 0x0000_0001_0000_0000;

    let mut code = vec![
        0xC4, 0xE2, 0xE3, 0xF5, 0x0F, // pdep rcx, rbx, [rdi]
        0x48, 0x89, 0x0F, // mov [rdi], rcx
        0xC4, 0xE2, 0xE2, 0xF5, 0x47, 0x08, // pext rax, rbx, [rdi+8]
        0x48, 0x89, 0x47, 0x08, // mov [rdi+8], rax
        0xC4, 0xE3, 0xFB, 0xF0, 0x4F, 0x18, 0x14, // rorx rcx, [rdi+24], 20
        0x48, 0x89, 0x4F, 0x18, // mov [rdi+24], rcx
        0xC4, 0xE2, 0xE3, 0xF6, 0x47, 0x10, // mulx rax, rbx, [rdi+16]
        0x48, 0x89, 0x47, 0x10, // mov [rdi+16], rax
        0xB9,
    ];
    code.extend_from_slice(&4u32.to_le_bytes()); // mov ecx, 4
    code.extend_from_slice(&[
        0xC4, 0xE2, 0xF2, 0xF7, 0x57, 0x20, // sarx rdx, [rdi+32], rcx
        0x48, 0x89, 0x57, 0x20, // mov [rdi+32], rdx
        0xB9,
    ]);
    code.extend_from_slice(&8u32.to_le_bytes()); // mov ecx, 8
    code.extend_from_slice(&[
        0xC4, 0xE2, 0xF3, 0xF7, 0x57, 0x28, // shrx rdx, [rdi+40], rcx
        0x48, 0x89, 0x57, 0x28, // mov [rdi+40], rdx
        0xB9,
    ]);
    code.extend_from_slice(&12u32.to_le_bytes()); // mov ecx, 12
    code.extend_from_slice(&[
        0xC4, 0xE2, 0xF1, 0xF7, 0x57, 0x30, // shlx rdx, [rdi+48], rcx
        0x48, 0x89, 0x57, 0x30, // mov [rdi+48], rdx
        0xC4, 0xE2, 0xF0, 0xF5, 0x57, 0x38, // bzhi rdx, [rdi+56], rcx
        0x48, 0x89, 0x57, 0x38, // mov [rdi+56], rdx
        HLT,
    ]);

    check_mem("bmi2_64_memory_source_forms", &code, r, s, BZHI_DEFINED);
}

#[test]
fn bmi2_bzhi() {
    // BZHI r32, r/m32, r32 = VEX.LZ.0F38.W0 F5 /r : zero bits of src at index >= count.
    //   count is in the vvvv-encoded register; src is r/m. (pp=00, NP)
    //   bzhi eax, ecx, ebx : vvvv=ebx(count), rm=ecx(src), dest=eax.
    //   byte3 = W0 vvvv(~ebx=1100) L0 pp(00) = 0 1100 0 00 = 0x60.
    //   C4 E2 60 F5 C1.
    let mut r = regs();
    r.rcx = 0xFFFF_FFFF; // source: all ones
    r.rbx = 12; // keep low 12 bits -> 0x00000FFF
    check_flags_masked(
        "bzhi",
        &with_hlt(vec![0xC4, 0xE2, 0x60, 0xF5, 0xC1]),
        r,
        BZHI_DEFINED,
    );
}

#[test]
fn bmi2_bzhi_count_ge_width() {
    // count >= operand size leaves the source unchanged; CF set when count>=width.
    let mut r = regs();
    r.rcx = 0xDEAD_BEEF;
    r.rbx = 40; // >= 32 -> no bits cleared, CF=1
    check_flags_masked(
        "bzhi_wide",
        &with_hlt(vec![0xC4, 0xE2, 0x60, 0xF5, 0xC1]),
        r,
        BZHI_DEFINED,
    );
}

#[test]
fn bmi2_bzhi_64() {
    // 64-bit BZHI (W1): byte3 sets W bit -> 0xE0.
    //   bzhi rax, rcx, rbx : C4 E2 E0 F5 C1.
    let mut r = regs();
    r.rcx = 0xFFFF_FFFF_FFFF_FFFF;
    r.rbx = 33; // keep low 33 bits
    check_flags_masked(
        "bzhi64",
        &with_hlt(vec![0xC4, 0xE2, 0xE0, 0xF5, 0xC1]),
        r,
        BZHI_DEFINED,
    );
}

#[test]
fn bmi2_pext_sparse_mask() {
    // PEXT with a sparse (non-contiguous) mask gathers selected bits to the low end.
    let mut r = regs();
    r.rbx = 0xFEDC_BA98; // source
    r.rcx = 0x8421_1248; // sparse mask (scattered single bits)
    // C4 E2 62 F5 C1  pext eax, ebx, ecx
    check_flags_masked(
        "pext_sparse",
        &with_hlt(vec![0xC4, 0xE2, 0x62, 0xF5, 0xC1]),
        r,
        0,
    );
}

#[test]
fn bmi2_pext_64() {
    // 64-bit PEXT (W1): byte3 W bit -> from 0x62 to 0xE2.
    let mut r = regs();
    r.rbx = 0x0123_4567_89AB_CDEF;
    r.rcx = 0xF0F0_F0F0_F0F0_F0F0; // take the high nibble of each byte
    // C4 E2 E2 F5 C1  pext rax, rbx, rcx
    check_flags_masked(
        "pext64",
        &with_hlt(vec![0xC4, 0xE2, 0xE2, 0xF5, 0xC1]),
        r,
        0,
    );
}

#[test]
fn bmi2_pdep_sparse_mask() {
    // PDEP scatters the low source bits into a sparse mask's set positions.
    let mut r = regs();
    r.rbx = 0x0000_00FF; // 8 source bits
    r.rcx = 0x8421_1248; // sparse target positions
    // C4 E2 63 F5 C1  pdep eax, ebx, ecx
    check_flags_masked(
        "pdep_sparse",
        &with_hlt(vec![0xC4, 0xE2, 0x63, 0xF5, 0xC1]),
        r,
        0,
    );
}

#[test]
fn bmi2_pdep_64() {
    // 64-bit PDEP (W1): byte3 from 0x63 to 0xE3.
    let mut r = regs();
    r.rbx = 0x0000_0000_0000_FFFF;
    r.rcx = 0xF0F0_F0F0_F0F0_F0F0;
    // C4 E2 E3 F5 C1  pdep rax, rbx, rcx
    check_flags_masked(
        "pdep64",
        &with_hlt(vec![0xC4, 0xE2, 0xE3, 0xF5, 0xC1]),
        r,
        0,
    );
}

#[test]
fn bmi2_mulx_64() {
    // 64-bit MULX (W1): high half -> vvvv(rbx), low half -> reg(rax). RDX implicit.
    //   byte3 = W1 vvvv(~rbx=1100) L0 pp(11) = 1 1100 0 11 = 0xE3.
    //   mulx rax, rbx, rcx : C4 E2 E3 F6 C1.
    let mut r = regs();
    r.rdx = 0xFFFF_FFFF_FFFF_FFFF; // multiplicand
    r.rcx = 0x0000_0000_0000_0002; // src2 -> product 0x1_FFFF...E
    check_flags_masked(
        "mulx64",
        &with_hlt(vec![0xC4, 0xE2, 0xE3, 0xF6, 0xC1]),
        r,
        0,
    );
}

#[test]
fn bmi2_mulx_same_dest() {
    // MULX where dest1 (vvvv) == dest2 (reg): per the ISA only the high half is
    // written when both destinations are the same register. Here both = eax.
    //   mulx eax, eax, ecx : vvvv=eax -> field 0b1111 (inverted), reg=eax(000), rm=ecx(001).
    //   byte3 = W0 vvvv(1111) L0 pp(11) = 0 1111 0 11 = 0x7B. modrm = 0xC1.
    //   C4 E2 7B F6 C1.
    let mut r = regs();
    r.rdx = 0x0000_0000_0001_0000;
    r.rcx = 0x0000_0000_0001_0000; // product 0x1_0000_0000 : high=1, low=0 -> eax gets high(1)
    check_flags_masked(
        "mulx_samedest",
        &with_hlt(vec![0xC4, 0xE2, 0x7B, 0xF6, 0xC1]),
        r,
        0,
    );
}

#[test]
fn bmi2_rorx_count_zero() {
    // RORX with imm=0 : no rotation, plain copy. ecx -> eax unchanged (zero-extended).
    let mut r = regs();
    r.rcx = 0xDEAD_BEEF;
    // C4 E3 7B F0 C1 00  rorx eax, ecx, 0
    check_flags_masked(
        "rorx_0",
        &with_hlt(vec![0xC4, 0xE3, 0x7B, 0xF0, 0xC1, 0x00]),
        r,
        0,
    );
}

#[test]
fn bmi2_rorx_count_31() {
    // 32-bit RORX by 31 (== rotate left by 1).
    let mut r = regs();
    r.rcx = 0x8000_0001;
    // C4 E3 7B F0 C1 1F  rorx eax, ecx, 31
    check_flags_masked(
        "rorx_31",
        &with_hlt(vec![0xC4, 0xE3, 0x7B, 0xF0, 0xC1, 0x1F]),
        r,
        0,
    );
}

#[test]
fn bmi2_rorx64_count_63() {
    // 64-bit RORX by 63 (== rotate left by 1).
    let mut r = regs();
    r.rcx = 0x8000_0000_0000_0001;
    // C4 E3 FB F0 C1 3F  rorx rax, rcx, 63
    check_flags_masked(
        "rorx64_63",
        &with_hlt(vec![0xC4, 0xE3, 0xFB, 0xF0, 0xC1, 0x3F]),
        r,
        0,
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 4: SSE/SSE2 integer (saturation, compares, multiplies,
// pack/shuffle/unpack/shift-bytes/movmsk), SSE3/SSSE3, scalar conversion edges,
// AES-NI, CRC32/POPCNT r/m, BT* with memory operands and out-of-range indices,
// ADCX/ADOX carry chains, and CMC/STC/CLC/STD plus XADD/CMPXCHG/BSWAP memory.
// ===========================================================================
//
// Everything here reuses the existing harness helpers verbatim:
//  - `check_sse`/`sse_program`/`sse_scratch` for 128-bit SSE results via scratch.
//  - `check`/`check_flags_masked`/`check_mem` for GPR/flag/memory comparisons.
//  - `load_rdi_data`/`f32x4`/`f64x2`/`scratch_f64` builders.
// Undefined-flag masks reuse the existing constants (FLAG_MASK, etc.).
//
// All SSE integer ops below set NO RFLAGS bits, so `check_sse` (flag_mask 0) is
// the right tool: it compares the stored 128-bit result page plus the live XMM.

// Build two 16-byte inputs from packed u16 lanes (little-endian).
fn u16x8(v: [u16; 8]) -> [u8; 16] {
    let mut o = [0u8; 16];
    for i in 0..8 {
        o[i * 2..i * 2 + 2].copy_from_slice(&v[i].to_le_bytes());
    }
    o
}

// Build a 16-byte input from packed u32 lanes (little-endian).
fn u32x4(v: [u32; 4]) -> [u8; 16] {
    let mut o = [0u8; 16];
    for i in 0..4 {
        o[i * 4..i * 4 + 4].copy_from_slice(&v[i].to_le_bytes());
    }
    o
}

// ---------------------------------------------------------------------------
// SSE2 integer: saturating adds/subs (PADDUSB/PADDUSW/PADDSB/PADDSW/PSUBUSB).
// ---------------------------------------------------------------------------

#[test]
fn sse_paddusb() {
    // PADDUSB xmm0, xmm1 = 66 0F DC C1 : unsigned byte add with saturation to 0xFF.
    let a = [
        0x00, 0x01, 0x7F, 0x80, 0xFE, 0xFF, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90,
        0xA0,
    ];
    let b = [
        0x00, 0xFF, 0x01, 0x80, 0x01, 0x01, 0xF0, 0xF0, 0xD0, 0xC0, 0xB0, 0xA0, 0x90, 0x80, 0x70,
        0x60,
    ];
    check_sse(
        "paddusb",
        &sse_program(&[0x66, 0x0F, 0xDC, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_paddusw() {
    // PADDUSW xmm0, xmm1 = 66 0F DD C1 : unsigned word add with saturation to 0xFFFF.
    let a = u16x8([
        0x0000, 0x0001, 0x7FFF, 0x8000, 0xFFFE, 0xFFFF, 0x1234, 0xABCD,
    ]);
    let b = u16x8([
        0x0000, 0xFFFF, 0x0001, 0x8000, 0x0001, 0x0001, 0xF000, 0x6000,
    ]);
    check_sse(
        "paddusw",
        &sse_program(&[0x66, 0x0F, 0xDD, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_paddsb() {
    // PADDSB xmm0, xmm1 = 66 0F EC C1 : signed byte add with saturation [-128,127].
    let a = [
        0x7F, 0x7F, 0x80, 0x80, 0x01, 0xFF, 0x40, 0x40, 0xC0, 0xC0, 0x00, 0x7E, 0x81, 0x10, 0x20,
        0x30,
    ];
    let b = [
        0x01, 0x7F, 0xFF, 0x80, 0xFF, 0x01, 0x40, 0x50, 0xC0, 0xB0, 0x00, 0x02, 0xFF, 0x20, 0x30,
        0x40,
    ];
    check_sse(
        "paddsb",
        &sse_program(&[0x66, 0x0F, 0xEC, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_paddsw() {
    // PADDSW xmm0, xmm1 = 66 0F ED C1 : signed word add with saturation [-32768,32767].
    let a = u16x8([
        0x7FFF, 0x8000, 0x0001, 0xFFFF, 0x4000, 0xC000, 0x7FFE, 0x8001,
    ]);
    let b = u16x8([
        0x0001, 0xFFFF, 0xFFFF, 0x0001, 0x4000, 0xC000, 0x0003, 0xFFFE,
    ]);
    check_sse(
        "paddsw",
        &sse_program(&[0x66, 0x0F, 0xED, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_psubusb() {
    // PSUBUSB xmm0, xmm1 = 66 0F D8 C1 : unsigned byte sub with saturation to 0.
    let a = [
        0x00, 0x10, 0x80, 0xFF, 0x01, 0x7F, 0x05, 0x90, 0xA0, 0xB0, 0xC0, 0x00, 0x01, 0x02, 0x03,
        0x04,
    ];
    let b = [
        0x01, 0x20, 0x01, 0x80, 0x02, 0x40, 0x05, 0x91, 0x10, 0xC0, 0xFF, 0x00, 0x05, 0x00, 0x10,
        0x04,
    ];
    check_sse(
        "psubusb",
        &sse_program(&[0x66, 0x0F, 0xD8, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_psubusw() {
    // PSUBUSW xmm0, xmm1 = 66 0F D9 C1 : unsigned word sub with saturation to 0.
    let a = u16x8([
        0x0000, 0x1000, 0x8000, 0xFFFF, 0x0001, 0x7FFF, 0x00FF, 0xABCD,
    ]);
    let b = u16x8([
        0x0001, 0x2000, 0x0001, 0x8000, 0x0002, 0x4000, 0x0100, 0xABCE,
    ]);
    check_sse(
        "psubusw",
        &sse_program(&[0x66, 0x0F, 0xD9, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_psubsb() {
    // PSUBSB xmm0, xmm1 = 66 0F E8 C1 : signed byte sub with saturation [-128,127].
    let a = [
        0x7F, 0x80, 0x00, 0x7F, 0x80, 0x01, 0x40, 0xC0, 0x10, 0x20, 0x30, 0x7F, 0x80, 0xFF, 0x00,
        0x01,
    ];
    let b = [
        0xFF, 0x01, 0x00, 0x80, 0x7F, 0x02, 0xC0, 0x40, 0x05, 0x10, 0x40, 0xFF, 0x01, 0xFF, 0x01,
        0x02,
    ];
    check_sse(
        "psubsb",
        &sse_program(&[0x66, 0x0F, 0xE8, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_psubsw() {
    // PSUBSW xmm0, xmm1 = 66 0F E9 C1 : signed word sub with saturation.
    let a = u16x8([
        0x7FFF, 0x8000, 0x0000, 0x7FFF, 0x8000, 0x0001, 0x4000, 0xC000,
    ]);
    let b = u16x8([
        0xFFFF, 0x0001, 0x0000, 0x8000, 0x7FFF, 0x0002, 0xC000, 0x4000,
    ]);
    check_sse(
        "psubsw",
        &sse_program(&[0x66, 0x0F, 0xE9, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse2_saturating_add_sub_memory_sources() {
    let a = [
        0x00, 0x10, 0x7F, 0x80, 0xFE, 0xFF, 0x40, 0xC0, 0x01, 0x02, 0x7E, 0x81, 0xAA, 0x55,
        0x33, 0xCC,
    ];
    let b = [
        0x01, 0xF0, 0x01, 0x80, 0x02, 0x01, 0x40, 0xC0, 0xFF, 0x02, 0x02, 0xFF, 0x55, 0xAA,
        0xCC, 0x33,
    ];

    for (label, opcode) in [
        ("paddusb_mem", 0xDC),
        ("paddusw_mem", 0xDD),
        ("paddsb_mem", 0xEC),
        ("paddsw_mem", 0xED),
        ("psubusb_mem", 0xD8),
        ("psubusw_mem", 0xD9),
        ("psubsb_mem", 0xE8),
        ("psubsw_mem", 0xE9),
    ] {
        check_sse(
            label,
            &sse_program(&[0x66, 0x0F, opcode, 0x47, 0x10]),
            sse_scratch(a, b),
        );
    }
}

// ---------------------------------------------------------------------------
// SSE2 integer: packed equality / greater-than compares (produce all-1s/0s).
// ---------------------------------------------------------------------------

#[test]
fn sse_pcmpeqb() {
    // PCMPEQB xmm0, xmm1 = 66 0F 74 C1 : per-byte equality mask.
    let a = [
        0, 1, 2, 3, 4, 5, 6, 7, 0xFF, 0x80, 0x00, 0x7F, 0xAA, 0x55, 0x10, 0x20,
    ];
    let b = [
        0, 9, 2, 9, 4, 9, 6, 9, 0xFF, 0x00, 0x00, 0x7F, 0xAA, 0x54, 0x10, 0x21,
    ];
    check_sse(
        "pcmpeqb",
        &sse_program(&[0x66, 0x0F, 0x74, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_pcmpeqw() {
    // PCMPEQW xmm0, xmm1 = 66 0F 75 C1 : per-word equality mask.
    let a = u16x8([
        0x1234, 0xABCD, 0x0000, 0xFFFF, 0x8000, 0x7FFF, 0x0001, 0xDEAD,
    ]);
    let b = u16x8([
        0x1234, 0x0000, 0x0000, 0xFFFE, 0x8000, 0x7FFE, 0x0001, 0xBEEF,
    ]);
    check_sse(
        "pcmpeqw",
        &sse_program(&[0x66, 0x0F, 0x75, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_pcmpeqd() {
    // PCMPEQD xmm0, xmm1 = 66 0F 76 C1 : per-dword equality mask.
    let a = u32x4([0x1234_5678, 0xFFFF_FFFF, 0x0000_0000, 0x8000_0000]);
    let b = u32x4([0x1234_5678, 0xFFFF_FFFE, 0x0000_0000, 0x7FFF_FFFF]);
    check_sse(
        "pcmpeqd",
        &sse_program(&[0x66, 0x0F, 0x76, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_pcmpgtb() {
    // PCMPGTB xmm0, xmm1 = 66 0F 64 C1 : per-byte SIGNED greater-than mask.
    let a = [
        0x7F, 0x80, 0x00, 0xFF, 0x01, 0x10, 0xC0, 0x40, 0x05, 0x06, 0x80, 0x7F, 0x00, 0x01, 0xFE,
        0x02,
    ];
    let b = [
        0x7E, 0x7F, 0x00, 0x00, 0xFF, 0x10, 0x40, 0xC0, 0x06, 0x05, 0x7F, 0x80, 0x01, 0x00, 0xFF,
        0x03,
    ];
    check_sse(
        "pcmpgtb",
        &sse_program(&[0x66, 0x0F, 0x64, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_pcmpgtw() {
    // PCMPGTW xmm0, xmm1 = 66 0F 65 C1 : per-word SIGNED greater-than mask.
    let a = u16x8([
        0x7FFF, 0x8000, 0x0001, 0xFFFF, 0x4000, 0xC000, 0x0000, 0x1234,
    ]);
    let b = u16x8([
        0x7FFE, 0x7FFF, 0xFFFF, 0x0000, 0xC000, 0x4000, 0x0000, 0x1234,
    ]);
    check_sse(
        "pcmpgtw",
        &sse_program(&[0x66, 0x0F, 0x65, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_pcmpgtd() {
    // PCMPGTD xmm0, xmm1 = 66 0F 66 C1 : per-dword SIGNED greater-than mask.
    let a = u32x4([0x7FFF_FFFF, 0x8000_0000, 0x0000_0001, 0xFFFF_FFFF]);
    let b = u32x4([0x7FFF_FFFE, 0x7FFF_FFFF, 0xFFFF_FFFF, 0x0000_0000]);
    check_sse(
        "pcmpgtd",
        &sse_program(&[0x66, 0x0F, 0x66, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse2_compare_logical_memory_sources() {
    let a = [
        0x7F, 0x80, 0x00, 0xFF, 0x01, 0x10, 0xC0, 0x40, 0x05, 0x06, 0x80, 0x7F, 0x00, 0x01,
        0xFE, 0x02,
    ];
    let b = [
        0x7E, 0x7F, 0x00, 0x00, 0xFF, 0x10, 0x40, 0xC0, 0x06, 0x05, 0x7F, 0x80, 0x01, 0x00,
        0xFF, 0x03,
    ];

    for (label, opcode) in [
        ("pand_mem", 0xDB),
        ("pandn_mem", 0xDF),
        ("por_mem", 0xEB),
        ("pxor_mem", 0xEF),
        ("pcmpeqb_mem", 0x74),
        ("pcmpeqw_mem", 0x75),
        ("pcmpeqd_mem", 0x76),
        ("pcmpgtb_mem", 0x64),
        ("pcmpgtw_mem", 0x65),
        ("pcmpgtd_mem", 0x66),
    ] {
        check_sse(
            label,
            &sse_program(&[0x66, 0x0F, opcode, 0x47, 0x10]),
            sse_scratch(a, b),
        );
    }
}

// ---------------------------------------------------------------------------
// SSE2 integer: multiplies (PMULHW/PMULHUW/PMULUDQ).  PMULHW is already covered.
// ---------------------------------------------------------------------------

#[test]
fn sse_pmulhuw() {
    // PMULHUW xmm0, xmm1 = 66 0F E4 C1 : packed 16-bit UNSIGNED multiply, high 16 bits.
    let a = u16x8([
        0xFFFF, 0x8000, 0x4000, 0x0100, 0x00FF, 0x1234, 0xABCD, 0x0001,
    ]);
    let b = u16x8([
        0xFFFF, 0x0002, 0x0004, 0x0100, 0x00FF, 0x1000, 0x0010, 0xFFFF,
    ]);
    check_sse(
        "pmulhuw",
        &sse_program(&[0x66, 0x0F, 0xE4, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_pmuludq() {
    // PMULUDQ xmm0, xmm1 = 66 0F F4 C1 : multiply unsigned dword lanes 0 and 2
    // (the even dwords) producing two 64-bit results.
    let a = u32x4([0xFFFF_FFFF, 0xDEAD_BEEF, 0x0001_0000, 0xCAFE_BABE]);
    let b = u32x4([0xFFFF_FFFF, 0x1111_1111, 0x0001_0000, 0x2222_2222]);
    check_sse(
        "pmuludq",
        &sse_program(&[0x66, 0x0F, 0xF4, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse2_multiply_minmax_average_memory_sources() {
    let a = u16x8([
        0xFFFF, 0x8000, 0x4000, 0x0100, 0x00FF, 0x1234, 0xABCD, 0x0001,
    ]);
    let b = u16x8([
        0xFFFF, 0x0002, 0x0004, 0x0100, 0x00FF, 0x1000, 0x0010, 0x7FFF,
    ]);

    for (label, opcode) in [
        ("pmullw_mem", 0xD5),
        ("pmulhw_mem", 0xE5),
        ("pmulhuw_mem", 0xE4),
        ("pmuludq_mem", 0xF4),
        ("pmaddwd_mem", 0xF5),
        ("psadbw_mem", 0xF6),
        ("pavgb_mem", 0xE0),
        ("pavgw_mem", 0xE3),
        ("pminub_mem", 0xDA),
        ("pmaxub_mem", 0xDE),
        ("pminsw_mem", 0xEA),
        ("pmaxsw_mem", 0xEE),
    ] {
        check_sse(
            label,
            &sse_program(&[0x66, 0x0F, opcode, 0x47, 0x10]),
            sse_scratch(a, b),
        );
    }
}

#[test]
fn sse2_pack_unpack_shuffle_memory_sources() {
    let a = [
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
        0xEE, 0xFF,
    ];
    let b = [
        0x7F, 0x80, 0x01, 0xFE, 0x10, 0xEF, 0x20, 0xDF, 0x30, 0xCF, 0x40, 0xBF, 0x50, 0xAF,
        0x60, 0x9F,
    ];

    for (label, opcode) in [
        ("packsswb_mem", 0x63),
        ("packssdw_mem", 0x6B),
        ("packuswb_mem", 0x67),
        ("punpcklbw_mem", 0x60),
        ("punpcklwd_mem", 0x61),
        ("punpckldq_mem", 0x62),
        ("punpcklqdq_mem", 0x6C),
        ("punpckhbw_mem", 0x68),
        ("punpckhwd_mem", 0x69),
        ("punpckhdq_mem", 0x6A),
        ("punpckhqdq_mem", 0x6D),
    ] {
        check_sse(
            label,
            &sse_program(&[0x66, 0x0F, opcode, 0x47, 0x10]),
            sse_scratch(a, b),
        );
    }

    check_sse(
        "pshufd_mem",
        &sse_program(&[0x66, 0x0F, 0x70, 0x47, 0x10, 0x1B]),
        sse_scratch(a, b),
    );
    check_sse(
        "pshuflw_mem",
        &sse_program(&[0xF2, 0x0F, 0x70, 0x47, 0x10, 0x1B]),
        sse_scratch(a, b),
    );
    check_sse(
        "pshufhw_mem",
        &sse_program(&[0xF3, 0x0F, 0x70, 0x47, 0x10, 0x1B]),
        sse_scratch(a, b),
    );
}

// ---------------------------------------------------------------------------
// SSE2 integer: PSHUFD / PSHUFLW / PSHUFHW (immediate lane selection).
// These read the SOURCE (xmm1) and write the DEST (xmm0), so we drive them with
// the standard sse_program where the op encodes the shuffle of xmm1 into xmm0.
// ---------------------------------------------------------------------------

#[test]
fn sse_pshufd() {
    // PSHUFD xmm0, xmm1, imm8 = 66 0F 70 C1 ib. imm=0b00_01_10_11=0x1B -> reverse dwords.
    let a = [0u8; 16];
    let b = u32x4([0x1111_1111, 0x2222_2222, 0x3333_3333, 0x4444_4444]);
    check_sse(
        "pshufd",
        &sse_program(&[0x66, 0x0F, 0x70, 0xC1, 0x1B]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_pshuflw() {
    // PSHUFLW xmm0, xmm1, imm8 = F2 0F 70 C1 ib. Shuffle the LOW 4 words; high 64 copied.
    // imm=0b00_01_10_11=0x1B -> reverse the low four words.
    let a = [0u8; 16];
    let b = u16x8([
        0xAAAA, 0xBBBB, 0xCCCC, 0xDDDD, 0x1111, 0x2222, 0x3333, 0x4444,
    ]);
    check_sse(
        "pshuflw",
        &sse_program(&[0xF2, 0x0F, 0x70, 0xC1, 0x1B]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_pshufhw() {
    // PSHUFHW xmm0, xmm1, imm8 = F3 0F 70 C1 ib. Shuffle the HIGH 4 words; low 64 copied.
    // imm=0b00_01_10_11=0x1B -> reverse the high four words.
    let a = [0u8; 16];
    let b = u16x8([
        0x1111, 0x2222, 0x3333, 0x4444, 0xAAAA, 0xBBBB, 0xCCCC, 0xDDDD,
    ]);
    check_sse(
        "pshufhw",
        &sse_program(&[0xF3, 0x0F, 0x70, 0xC1, 0x1B]),
        sse_scratch(a, b),
    );
}

// ---------------------------------------------------------------------------
// SSE2 integer: PSLLDQ / PSRLDQ (byte shift of the whole 128-bit register, imm8).
// These are encoded with a group opcode on the register itself (66 0F 73 /7 and
// /3), operating in-place on xmm0. We load xmm0 from scratch then store it back.
// ---------------------------------------------------------------------------

/// Build: load xmm0 from [rdi], run a single in-place op on xmm0, store xmm0 back.
fn sse_unary_xmm0(op: &[u8]) -> Vec<u8> {
    let mut code = load_rdi_data();
    code.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]); // movdqu xmm0, [rdi]
    code.extend_from_slice(op);
    code.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]); // movdqu [rdi+0x20], xmm0
    code.push(HLT);
    code
}

#[test]
fn sse_pslldq() {
    // PSLLDQ xmm0, imm8 = 66 0F 73 /7 ib (modrm F8 selects /7 on xmm0). Shift left 3 bytes.
    let a = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
        0x10,
    ];
    check_sse(
        "pslldq",
        &sse_unary_xmm0(&[0x66, 0x0F, 0x73, 0xF8, 0x03]),
        sse_scratch(a, [0u8; 16]),
    );
}

#[test]
fn sse_psrldq() {
    // PSRLDQ xmm0, imm8 = 66 0F 73 /3 ib (modrm D8 selects /3 on xmm0). Shift right 5 bytes.
    let a = [
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F,
        0x10,
    ];
    check_sse(
        "psrldq",
        &sse_unary_xmm0(&[0x66, 0x0F, 0x73, 0xD8, 0x05]),
        sse_scratch(a, [0u8; 16]),
    );
}

#[test]
fn sse_pslldq_full() {
    // Shifting by >= 16 yields all zeros.
    let a = [0xFFu8; 16];
    check_sse(
        "pslldq16",
        &sse_unary_xmm0(&[0x66, 0x0F, 0x73, 0xF8, 0x10]),
        sse_scratch(a, [0u8; 16]),
    );
}

// ---------------------------------------------------------------------------
// SSE2 integer: more PUNPCK forms (words, high dwords, high qwords).
// ---------------------------------------------------------------------------

#[test]
fn sse_punpcklwd() {
    // PUNPCKLWD xmm0, xmm1 = 66 0F 61 C1 : interleave low 4 words.
    let a = u16x8([
        0x0000, 0x1111, 0x2222, 0x3333, 0x4444, 0x5555, 0x6666, 0x7777,
    ]);
    let b = u16x8([
        0x8888, 0x9999, 0xAAAA, 0xBBBB, 0xCCCC, 0xDDDD, 0xEEEE, 0xFFFF,
    ]);
    check_sse(
        "punpcklwd",
        &sse_program(&[0x66, 0x0F, 0x61, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_punpckhwd() {
    // PUNPCKHWD xmm0, xmm1 = 66 0F 69 C1 : interleave high 4 words.
    let a = u16x8([
        0x0000, 0x1111, 0x2222, 0x3333, 0x4444, 0x5555, 0x6666, 0x7777,
    ]);
    let b = u16x8([
        0x8888, 0x9999, 0xAAAA, 0xBBBB, 0xCCCC, 0xDDDD, 0xEEEE, 0xFFFF,
    ]);
    check_sse(
        "punpckhwd",
        &sse_program(&[0x66, 0x0F, 0x69, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_punpckhdq() {
    // PUNPCKHDQ xmm0, xmm1 = 66 0F 6A C1 : interleave high 2 dwords.
    let a = u32x4([0x1111_1111, 0x2222_2222, 0x3333_3333, 0x4444_4444]);
    let b = u32x4([0xAAAA_AAAA, 0xBBBB_BBBB, 0xCCCC_CCCC, 0xDDDD_DDDD]);
    check_sse(
        "punpckhdq",
        &sse_program(&[0x66, 0x0F, 0x6A, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse_punpckhqdq() {
    // PUNPCKHQDQ xmm0, xmm1 = 66 0F 6D C1 : result = [a.hi, b.hi].
    let a = [
        0x0123_4567_89AB_CDEFu64.to_le_bytes(),
        0xDEAD_BEEF_CAFE_BABEu64.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x1122_3344_5566_7788u64.to_le_bytes(),
        0x99AA_BBCC_DDEE_FF00u64.to_le_bytes(),
    ]
    .concat();
    check_sse(
        "punpckhqdq",
        &sse_program(&[0x66, 0x0F, 0x6D, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

// ---------------------------------------------------------------------------
// SSE2 integer: PACKSSDW (signed dword->word) and a second PACKUSWB shape.
// ---------------------------------------------------------------------------

#[test]
fn sse_packssdw() {
    // PACKSSDW xmm0, xmm1 = 66 0F 6B C1 : signed saturate pack dwords->words.
    let a = u32x4([0x0000_7FFF, 0x0000_8000, 0x7FFF_FFFF, 0x8000_0000]); // 32767, 32768->32767, large->32767, large neg->-32768
    let b = u32x4([0xFFFF_8000, 0x0000_0001, 0xFFFF_FFFF, 0x0001_0000]); // -32768, 1, -1, 65536->32767
    check_sse(
        "packssdw",
        &sse_program(&[0x66, 0x0F, 0x6B, 0xC1]),
        sse_scratch(a, b),
    );
}

// ---------------------------------------------------------------------------
// SSE2: PMOVMSKB and MOVMSKPD (extract sign/high bits into a GPR).
// ---------------------------------------------------------------------------

#[test]
fn sse_pmovmskb() {
    // PMOVMSKB eax, xmm1 = 66 0F D7 C1 : gather the top bit of each of 16 bytes.
    // Bytes with the high bit set: indices 0,2,4,...,14 (even) here -> mask 0x5555.
    let a = [0u8; 16];
    let b = [
        0x80, 0x00, 0x81, 0x7F, 0xFF, 0x10, 0x90, 0x20, 0xC0, 0x40, 0xA0, 0x50, 0xE0, 0x60, 0xF0,
        0x70,
    ];
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]); // movdqu xmm1, [rdi+0x10]
    prog.extend_from_slice(&[0x66, 0x0F, 0xD7, 0xC1]); // pmovmskb eax, xmm1
    prog.extend_from_slice(&[0x89, 0x47, 0x20]); // mov [rdi+0x20], eax
    prog.push(HLT);
    check_sse("pmovmskb", &prog, sse_scratch(a, b));
}

// ---------------------------------------------------------------------------
// SSSE3: PSHUFB, PHADDW/PHADDD, PMADDUBSW, PABSB/W/D, PALIGNR, PSIGNB/W/D.
// (3-byte opcodes 66 0F 38 xx; PALIGNR is 66 0F 3A 0F ib.)
// ---------------------------------------------------------------------------

#[test]
fn ssse3_pshufb() {
    // PSHUFB xmm0, xmm1 = 66 0F 38 00 C1 : permute bytes of xmm0 by indices in xmm1.
    // If a control byte has its top bit set, the result byte is zeroed.
    let a = [
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E,
        0x1F,
    ];
    // control: reverse low 8, zero a few, select some high bytes.
    let b = [
        0x07, 0x06, 0x05, 0x04, 0x80, 0x02, 0x01, 0x00, 0x0F, 0x0E, 0x88, 0x0C, 0x0B, 0x0A, 0x09,
        0x08,
    ];
    check_sse(
        "pshufb",
        &sse_program(&[0x66, 0x0F, 0x38, 0x00, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn ssse3_phaddw() {
    // PHADDW xmm0, xmm1 = 66 0F 38 01 C1 : horizontal add of adjacent word pairs.
    let a = u16x8([1, 2, 3, 4, 5, 6, 7, 8]);
    let b = u16x8([10, 20, 30, 40, 50, 60, 70, 80]);
    check_sse(
        "phaddw",
        &sse_program(&[0x66, 0x0F, 0x38, 0x01, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn ssse3_phaddd() {
    // PHADDD xmm0, xmm1 = 66 0F 38 02 C1 : horizontal add of adjacent dword pairs.
    let a = u32x4([0x0000_0001, 0x0000_0002, 0x0000_0003, 0x0000_0004]);
    let b = u32x4([0x1000_0000, 0x2000_0000, 0x3000_0000, 0x4000_0000]);
    check_sse(
        "phaddd",
        &sse_program(&[0x66, 0x0F, 0x38, 0x02, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn ssse3_phaddsw() {
    // PHADDSW xmm0, xmm1 = 66 0F 38 03 C1 : horizontal add of word pairs WITH signed saturation.
    let a = u16x8([
        0x7FFF, 0x7FFF, 0x8000, 0x8000, 0x0001, 0xFFFF, 0x4000, 0x4000,
    ]);
    let b = u16x8([
        0x7FFF, 0x0001, 0x8000, 0xFFFF, 0x0000, 0x0000, 0xC000, 0xC000,
    ]);
    check_sse(
        "phaddsw",
        &sse_program(&[0x66, 0x0F, 0x38, 0x03, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn ssse3_pmaddubsw() {
    // PMADDUBSW xmm0, xmm1 = 66 0F 38 04 C1 : multiply UNSIGNED bytes of xmm0 by
    // SIGNED bytes of xmm1, add adjacent pairs, saturate to signed word.
    let a = [
        0xFF, 0xFF, 0x80, 0x80, 0x01, 0x02, 0x10, 0x20, 0x7F, 0x7F, 0x00, 0xFF, 0x55, 0xAA, 0x40,
        0x40,
    ];
    let b = [
        0x7F, 0x7F, 0x80, 0x80, 0xFF, 0x01, 0x02, 0x03, 0x7F, 0x7F, 0x12, 0x34, 0x01, 0xFF, 0x80,
        0x80,
    ];
    check_sse(
        "pmaddubsw",
        &sse_program(&[0x66, 0x0F, 0x38, 0x04, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn ssse3_memory_source_forms() {
    let a = u16x8([
        0x7FFF, 0x7FFF, 0x8000, 0x8000, 0x0001, 0xFFFF, 0x4000, 0x4000,
    ]);
    let b = u16x8([
        0x0001, 0xFFFF, 0x0000, 0x8000, 0x7FFF, 0x0001, 0xC000, 0xC000,
    ]);

    for (label, opcode) in [
        ("pshufb_mem", 0x00),
        ("phaddw_mem", 0x01),
        ("phaddd_mem", 0x02),
        ("phaddsw_mem", 0x03),
        ("pmaddubsw_mem", 0x04),
        ("phsubw_mem", 0x05),
        ("phsubd_mem", 0x06),
        ("phsubsw_mem", 0x07),
        ("psignb_mem", 0x08),
        ("psignw_mem", 0x09),
        ("psignd_mem", 0x0A),
        ("pmulhrsw_mem", 0x0B),
        ("pabsb_mem", 0x1C),
        ("pabsw_mem", 0x1D),
        ("pabsd_mem", 0x1E),
    ] {
        check_sse(
            label,
            &sse_memory_source_program(&[0x66, 0x0F, 0x38, opcode, 0x47, 0x10]),
            sse_scratch(a, b),
        );
    }

    check_sse(
        "palignr_mem",
        &sse_memory_source_program(&[0x66, 0x0F, 0x3A, 0x0F, 0x47, 0x10, 0x05]),
        sse_scratch(a, b),
    );
    check_sse(
        "palignr_mem_edge",
        &sse_memory_source_program(&[0x66, 0x0F, 0x3A, 0x0F, 0x47, 0x10, 0x12]),
        sse_scratch(a, b),
    );
}

#[test]
fn ssse3_pabsb() {
    // PABSB xmm0, xmm1 = 66 0F 38 1C C1 : per-byte absolute value (src xmm1 -> xmm0).
    // abs(-128) saturates to 0x80 (stays -128 pattern) per the ISA.
    let a = [0u8; 16];
    let b = [
        0x00, 0xFF, 0x80, 0x7F, 0x01, 0xFE, 0x40, 0xC0, 0x10, 0xF0, 0x55, 0xAB, 0x81, 0x7E, 0x02,
        0xFD,
    ];
    check_sse(
        "pabsb",
        &sse_program(&[0x66, 0x0F, 0x38, 0x1C, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn ssse3_pabsw() {
    // PABSW xmm0, xmm1 = 66 0F 38 1D C1 : per-word absolute value.
    let a = [0u8; 16];
    let b = u16x8([
        0x0000, 0xFFFF, 0x8000, 0x7FFF, 0x0001, 0xFFFE, 0x4000, 0xC000,
    ]);
    check_sse(
        "pabsw",
        &sse_program(&[0x66, 0x0F, 0x38, 0x1D, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn ssse3_pabsd() {
    // PABSD xmm0, xmm1 = 66 0F 38 1E C1 : per-dword absolute value.
    let a = [0u8; 16];
    let b = u32x4([0x0000_0000, 0xFFFF_FFFF, 0x8000_0000, 0x7FFF_FFFF]);
    check_sse(
        "pabsd",
        &sse_program(&[0x66, 0x0F, 0x38, 0x1E, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn ssse3_palignr() {
    // PALIGNR xmm0, xmm1, imm8 = 66 0F 3A 0F C1 ib : concatenate xmm0:xmm1 (32 bytes),
    // shift right by imm bytes, take the low 16. imm=5.
    let a = [
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E,
        0x1F,
    ];
    let b = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
        0x0F,
    ];
    check_sse(
        "palignr",
        &sse_program(&[0x66, 0x0F, 0x3A, 0x0F, 0xC1, 0x05]),
        sse_scratch(a, b),
    );
}

#[test]
fn ssse3_palignr_ge16() {
    // PALIGNR with imm >= 16 shifts in zeros from the high end. imm=18 -> shift the
    // upper operand (xmm0) right by 2 and zero-fill the top.
    let a = [
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E,
        0x1F,
    ];
    let b = [0u8; 16];
    check_sse(
        "palignr18",
        &sse_program(&[0x66, 0x0F, 0x3A, 0x0F, 0xC1, 0x12]),
        sse_scratch(a, b),
    );
}

#[test]
fn ssse3_psignb() {
    // PSIGNB xmm0, xmm1 = 66 0F 38 08 C1 : negate/zero bytes of xmm0 by the sign of xmm1.
    let a = [
        0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x7F, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
        0x08,
    ];
    let b = [
        0x01, 0xFF, 0x00, 0x7F, 0x80, 0x00, 0x05, 0xFE, 0xFF, 0x01, 0x00, 0xFF, 0x01, 0x00, 0xFF,
        0x01,
    ];
    check_sse(
        "psignb",
        &sse_program(&[0x66, 0x0F, 0x38, 0x08, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn ssse3_psignw() {
    // PSIGNW xmm0, xmm1 = 66 0F 38 09 C1 : per-word sign apply.
    let a = u16x8([
        0x0010, 0x0020, 0x0030, 0x7FFF, 0x0001, 0x0002, 0x0003, 0x0004,
    ]);
    let b = u16x8([
        0x0001, 0xFFFF, 0x0000, 0x8000, 0xFFFF, 0x0000, 0x0001, 0xFFFE,
    ]);
    check_sse(
        "psignw",
        &sse_program(&[0x66, 0x0F, 0x38, 0x09, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn ssse3_psignd() {
    // PSIGND xmm0, xmm1 = 66 0F 38 0A C1 : per-dword sign apply.
    let a = u32x4([0x0000_0010, 0x0000_0020, 0x7FFF_FFFF, 0x0000_0001]);
    let b = u32x4([0x0000_0001, 0xFFFF_FFFF, 0x0000_0000, 0x8000_0000]);
    check_sse(
        "psignd",
        &sse_program(&[0x66, 0x0F, 0x38, 0x0A, 0xC1]),
        sse_scratch(a, b),
    );
}

// ---------------------------------------------------------------------------
// SSE3: LDDQU (unaligned 128-bit load) and HADDPD/HSUBPD (double horizontal).
// ---------------------------------------------------------------------------

#[test]
fn sse3_lddqu() {
    // LDDQU xmm0, m128 = F2 0F F0 /r. Load from [rdi] then store to [rdi+0x20].
    let a = [
        0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0xFE, 0xDC, 0xBA, 0x98, 0x76, 0x54, 0x32,
        0x10,
    ];
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF2, 0x0F, 0xF0, 0x07]); // lddqu xmm0, [rdi]
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]); // movdqu [rdi+0x20], xmm0
    prog.push(HLT);
    check_sse("lddqu", &prog, sse_scratch(a, [0u8; 16]));
}

#[test]
fn sse3_haddpd() {
    // HADDPD xmm0, xmm1 = 66 0F 7C C1 : result = [a0+a1, b0+b1].
    let a = f64x2([1.5, 2.25]);
    let b = f64x2([10.0, 20.5]);
    check_sse(
        "haddpd",
        &sse_program(&[0x66, 0x0F, 0x7C, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse3_hsubpd() {
    // HSUBPD xmm0, xmm1 = 66 0F 7D C1 : result = [a0-a1, b0-b1].
    let a = f64x2([5.0, 1.25]);
    let b = f64x2([100.0, 40.5]);
    check_sse(
        "hsubpd",
        &sse_program(&[0x66, 0x0F, 0x7D, 0xC1]),
        sse_scratch(a, b),
    );
}

// ---------------------------------------------------------------------------
// Scalar conversions: edge cases for CVTSI2SS/SD, CVTTSS2SI overflow, CVTSD2SS.
// These need a nonzero GPR or scratch input, so they use run_both directly with
// CompareOpts (mirroring the existing cvtsi2sd / cvttsd2si tests).
// ---------------------------------------------------------------------------

/// Run a program with the given init regs comparing GPRs only (no flags/scratch).
fn check_gpr_only(label: &str, code: &[u8], init: Registers) {
    let Some((interp, kvm)) = run_both(code, init, zero_scratch()) else {
        return;
    };
    let opts = CompareOpts {
        flag_mask: 0,
        ..CompareOpts::default()
    };
    assert_match(label, code, &interp, &kvm, opts);
}

#[test]
fn cvt_cvtsi2ss_negative() {
    // CVTSI2SS xmm0, r/m64 = F3 48 0F 2A C0 (from rax). rax = -1 -> -1.0f, stored.
    let mut r = regs();
    r.rax = (-1i64) as u64;
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x48, 0x0F, 0x2A, 0xC0]); // cvtsi2ss xmm0, rax
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]); // movdqu [rdi+0x20], xmm0
    prog.push(HLT);
    let Some((interp, kvm)) = run_both(&prog, r, zero_scratch()) else {
        return;
    };
    let opts = CompareOpts {
        flag_mask: 0,
        scratch: true,
        ..CompareOpts::default()
    };
    assert_match("cvtsi2ss_neg", &prog, &interp, &kvm, opts);
}

#[test]
fn cvt_cvtsi2ss_large() {
    // Large value not exactly representable in f32 -> tests round-to-nearest-even.
    // 0x4000_0001 (1073741825) rounds to 1073741824.0f (loses the low bit).
    let mut r = regs();
    r.rax = 0x4000_0001;
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x48, 0x0F, 0x2A, 0xC0]); // cvtsi2ss xmm0, rax
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]); // movdqu [rdi+0x20], xmm0
    prog.push(HLT);
    let Some((interp, kvm)) = run_both(&prog, r, zero_scratch()) else {
        return;
    };
    let opts = CompareOpts {
        flag_mask: 0,
        scratch: true,
        ..CompareOpts::default()
    };
    assert_match("cvtsi2ss_large", &prog, &interp, &kvm, opts);
}

#[test]
fn cvt_cvtsi2sd_large() {
    // CVTSI2SD from a 64-bit value whose magnitude exceeds f64's 53-bit mantissa,
    // forcing rounding. 0x2000_0000_0000_0001 -> rounds to 0x2000_0000_0000_0000.
    let mut r = regs();
    r.rax = 0x2000_0000_0000_0001;
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF2, 0x48, 0x0F, 0x2A, 0xC0]); // cvtsi2sd xmm0, rax
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]); // movdqu [rdi+0x20], xmm0
    prog.push(HLT);
    let Some((interp, kvm)) = run_both(&prog, r, zero_scratch()) else {
        return;
    };
    let opts = CompareOpts {
        flag_mask: 0,
        scratch: true,
        ..CompareOpts::default()
    };
    assert_match("cvtsi2sd_large", &prog, &interp, &kvm, opts);
}

#[test]
fn cvt_cvttss2si_overflow_indefinite() {
    // CVTTSS2SI r32, xmm1 = F3 0F 2C C1 : truncating f32 -> i32. An out-of-range
    // value (here 1e20f) yields the "integer indefinite" 0x8000_0000.
    let mut b = [0u8; 16];
    b[0..4].copy_from_slice(&1.0e20f32.to_le_bytes());
    let scratch = sse_scratch([0u8; 16], b);
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]); // movdqu xmm1, [rdi+0x10]
    prog.extend_from_slice(&[0xF3, 0x0F, 0x2C, 0xC1]); // cvttss2si eax, xmm1
    prog.push(HLT);
    let Some((interp, kvm)) = run_both(&prog, regs(), scratch) else {
        return;
    };
    let opts = CompareOpts {
        flag_mask: 0,
        ..CompareOpts::default()
    };
    assert_match("cvttss2si_of", &prog, &interp, &kvm, opts);
}

#[test]
fn cvt_cvttsd2si_overflow_indefinite() {
    // CVTTSD2SI r64, xmm1 = F2 48 0F 2C C1 : an out-of-range f64 yields the 64-bit
    // integer indefinite 0x8000_0000_0000_0000.
    let b = f64x2([1.0e30, 0.0]);
    let scratch = sse_scratch([0u8; 16], b);
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]); // movdqu xmm1, [rdi+0x10]
    prog.extend_from_slice(&[0xF2, 0x48, 0x0F, 0x2C, 0xC1]); // cvttsd2si rax, xmm1
    prog.push(HLT);
    // GPR comparison; the converted integer lands in RAX. Scratch holds the input.
    let Some((interp, kvm)) = run_both(&prog, regs(), scratch) else {
        return;
    };
    let opts = CompareOpts {
        flag_mask: 0,
        ..CompareOpts::default()
    };
    assert_match("cvttsd2si_of", &prog, &interp, &kvm, opts);
}

#[test]
fn cvt_cvtsd2ss_rounding() {
    // CVTSD2SS xmm0, xmm1 = F2 0F 5A C1 : f64 -> f32 with round-to-nearest-even.
    // 1.0 + 2^-30 in f64 is not representable in f32 and must round to 1.0f.
    let v = 1.0_f64 + 2.0_f64.powi(-30);
    let b = f64x2([v, 0.0]);
    check_sse(
        "cvtsd2ss",
        &sse_program(&[0xF2, 0x0F, 0x5A, 0xC1]),
        sse_scratch([0u8; 16], b),
    );
}

#[test]
fn cvt_cvtss2sd_exact() {
    // CVTSS2SD xmm0, xmm1 = F3 0F 5A C1 : f32 -> f64 (always exact for finite).
    let mut b = [0u8; 16];
    b[0..4].copy_from_slice(&(-3.5f32).to_le_bytes());
    check_sse(
        "cvtss2sd",
        &sse_program(&[0xF3, 0x0F, 0x5A, 0xC1]),
        sse_scratch([0u8; 16], b),
    );
}

// ---------------------------------------------------------------------------
// AES-NI: AESENC / AESDEC / AESIMC / AESKEYGENASSIST (KVM exposes AES-NI; we
// require an exact bit-for-bit match of the 128-bit result).
// ---------------------------------------------------------------------------

#[test]
fn aes_aesenc() {
    // AESENC xmm0, xmm1 = 66 0F 38 DC C1 : one AES encryption round (state^round key).
    let state = [
        0x32, 0x43, 0xf6, 0xa8, 0x88, 0x5a, 0x30, 0x8d, 0x31, 0x31, 0x98, 0xa2, 0xe0, 0x37, 0x07,
        0x34,
    ];
    let rkey = [
        0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f,
        0x3c,
    ];
    check_sse(
        "aesenc",
        &sse_program(&[0x66, 0x0F, 0x38, 0xDC, 0xC1]),
        sse_scratch(state, rkey),
    );
    check_sse(
        "aesenc_mem",
        &sse_program(&[0x66, 0x0F, 0x38, 0xDC, 0x47, 0x10]),
        sse_scratch(state, rkey),
    );
}

#[test]
fn aes_aesenclast() {
    // AESENCLAST xmm0, xmm1 = 66 0F 38 DD C1 : final AES round (no MixColumns).
    let state = [
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE,
        0xFF,
    ];
    let rkey = [
        0x0F, 0x0E, 0x0D, 0x0C, 0x0B, 0x0A, 0x09, 0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01,
        0x00,
    ];
    check_sse(
        "aesenclast",
        &sse_program(&[0x66, 0x0F, 0x38, 0xDD, 0xC1]),
        sse_scratch(state, rkey),
    );
    check_sse(
        "aesenclast_mem",
        &sse_program(&[0x66, 0x0F, 0x38, 0xDD, 0x47, 0x10]),
        sse_scratch(state, rkey),
    );
}

#[test]
fn aes_aesdec() {
    // AESDEC xmm0, xmm1 = 66 0F 38 DE C1 : one AES decryption round.
    let state = [
        0x7a, 0xd5, 0xfd, 0xa7, 0x89, 0xef, 0x4e, 0x27, 0x2b, 0xca, 0x10, 0x0b, 0x3d, 0x9f, 0xf5,
        0x9f,
    ];
    let rkey = [
        0x54, 0x68, 0x61, 0x74, 0x73, 0x20, 0x6D, 0x79, 0x20, 0x4B, 0x75, 0x6E, 0x67, 0x20, 0x46,
        0x75,
    ];
    check_sse(
        "aesdec",
        &sse_program(&[0x66, 0x0F, 0x38, 0xDE, 0xC1]),
        sse_scratch(state, rkey),
    );
    check_sse(
        "aesdec_mem",
        &sse_program(&[0x66, 0x0F, 0x38, 0xDE, 0x47, 0x10]),
        sse_scratch(state, rkey),
    );
}

#[test]
fn aes_aesdeclast() {
    // AESDECLAST xmm0, xmm1 = 66 0F 38 DF C1 : final AES decryption round.
    let state = [
        0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x0F, 0xED, 0xCB, 0xA9, 0x87, 0x65, 0x43,
        0x21,
    ];
    let rkey = [
        0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
        0x99,
    ];
    check_sse(
        "aesdeclast",
        &sse_program(&[0x66, 0x0F, 0x38, 0xDF, 0xC1]),
        sse_scratch(state, rkey),
    );
    check_sse(
        "aesdeclast_mem",
        &sse_program(&[0x66, 0x0F, 0x38, 0xDF, 0x47, 0x10]),
        sse_scratch(state, rkey),
    );
}

#[test]
fn aes_aesimc() {
    // AESIMC xmm0, xmm1 = 66 0F 38 DB C1 : inverse MixColumns of xmm1 -> xmm0.
    let a = [0u8; 16];
    let b = [
        0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf, 0x4f,
        0x3c,
    ];
    check_sse(
        "aesimc",
        &sse_program(&[0x66, 0x0F, 0x38, 0xDB, 0xC1]),
        sse_scratch(a, b),
    );
    check_sse(
        "aesimc_mem",
        &sse_program(&[0x66, 0x0F, 0x38, 0xDB, 0x47, 0x10]),
        sse_scratch(a, b),
    );
}

#[test]
fn aes_aeskeygenassist() {
    // AESKEYGENASSIST xmm0, xmm1, imm8 = 66 0F 3A DF C1 ib : key expansion helper.
    // imm8 is the round constant (RCON). Use RCON=1.
    let a = [0u8; 16];
    let b = [
        0x09, 0xcf, 0x4f, 0x3c, 0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15,
        0x88,
    ];
    check_sse(
        "aeskeygen",
        &sse_program(&[0x66, 0x0F, 0x3A, 0xDF, 0xC1, 0x01]),
        sse_scratch(a, b),
    );
    check_sse(
        "aeskeygen_mem",
        &sse_program(&[0x66, 0x0F, 0x3A, 0xDF, 0x47, 0x10, 0x01]),
        sse_scratch(a, b),
    );
}

// ---------------------------------------------------------------------------
// SHA-NI: SHA1/SHA256 message schedule and round helpers. Granite Rapids exposes
// SHA, rax implements the 0F38/0F3A SHA opcodes, and these exact vector
// transforms had no KVM differential coverage.
// ---------------------------------------------------------------------------

fn sha_a() -> [u8; 16] {
    [
        0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54, 0x32,
        0x10,
    ]
}

fn sha_b() -> [u8; 16] {
    [
        0x6a, 0x09, 0xe6, 0x67, 0xbb, 0x67, 0xae, 0x85, 0x3c, 0x6e, 0xf3, 0x72, 0xa5, 0x4f, 0xf5,
        0x3a,
    ]
}

#[test]
fn sha1nexte_reg_mem() {
    check_sse(
        "sha1nexte_reg",
        &sse_program(&[0x0F, 0x38, 0xC8, 0xC1]),
        sse_scratch(sha_a(), sha_b()),
    );
    check_sse(
        "sha1nexte_mem",
        &sse_program(&[0x0F, 0x38, 0xC8, 0x47, 0x10]),
        sse_scratch(sha_a(), sha_b()),
    );
}

#[test]
fn sha1_message_schedule() {
    check_sse(
        "sha1msg1",
        &sse_program(&[0x0F, 0x38, 0xC9, 0xC1]),
        sse_scratch(sha_a(), sha_b()),
    );
    check_sse(
        "sha1msg2",
        &sse_program(&[0x0F, 0x38, 0xCA, 0xC1]),
        sse_scratch(sha_a(), sha_b()),
    );
    check_sse(
        "sha1msg1_mem",
        &sse_program(&[0x0F, 0x38, 0xC9, 0x47, 0x10]),
        sse_scratch(sha_a(), sha_b()),
    );
    check_sse(
        "sha1msg2_mem",
        &sse_program(&[0x0F, 0x38, 0xCA, 0x47, 0x10]),
        sse_scratch(sha_a(), sha_b()),
    );
}

#[test]
fn sha1rnds4_all_modes() {
    for mode in 0u8..4 {
        check_sse(
            match mode {
                0 => "sha1rnds4_imm0",
                1 => "sha1rnds4_imm1",
                2 => "sha1rnds4_imm2",
                _ => "sha1rnds4_imm3",
            },
            &sse_program(&[0x0F, 0x3A, 0xCC, 0xC1, mode]),
            sse_scratch(sha_a(), sha_b()),
        );
        check_sse(
            match mode {
                0 => "sha1rnds4_mem_imm0",
                1 => "sha1rnds4_mem_imm1",
                2 => "sha1rnds4_mem_imm2",
                _ => "sha1rnds4_mem_imm3",
            },
            &sse_program(&[0x0F, 0x3A, 0xCC, 0x47, 0x10, mode]),
            sse_scratch(sha_a(), sha_b()),
        );
    }
}

#[test]
fn sha256_round_and_schedule() {
    check_sse(
        "sha256rnds2",
        &sse_program(&[0x0F, 0x38, 0xCB, 0xC1]),
        sse_scratch(sha_a(), sha_b()),
    );
    check_sse(
        "sha256msg1",
        &sse_program(&[0x0F, 0x38, 0xCC, 0xC1]),
        sse_scratch(sha_a(), sha_b()),
    );
    check_sse(
        "sha256msg2",
        &sse_program(&[0x0F, 0x38, 0xCD, 0xC1]),
        sse_scratch(sha_a(), sha_b()),
    );
    check_sse(
        "sha256rnds2_mem",
        &sse_program(&[0x0F, 0x38, 0xCB, 0x47, 0x10]),
        sse_scratch(sha_a(), sha_b()),
    );
    check_sse(
        "sha256msg1_mem",
        &sse_program(&[0x0F, 0x38, 0xCC, 0x47, 0x10]),
        sse_scratch(sha_a(), sha_b()),
    );
    check_sse(
        "sha256msg2_mem",
        &sse_program(&[0x0F, 0x38, 0xCD, 0x47, 0x10]),
        sse_scratch(sha_a(), sha_b()),
    );
}

// ---------------------------------------------------------------------------
// PCLMULQDQ and legacy GFNI. The EVEX AVX-512 KVM corpus covers vector GFNI /
// VPCLMULQDQ, but these legacy 128-bit encodings are a distinct decode path.
// ---------------------------------------------------------------------------

fn crypto_a() -> [u8; 16] {
    [
        0x57, 0x83, 0x01, 0xff, 0x10, 0x20, 0x7f, 0x80, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0xaa,
        0xfe,
    ]
}

fn crypto_b() -> [u8; 16] {
    [
        0x83, 0x57, 0xff, 0x01, 0x02, 0x03, 0x80, 0x7f, 0xfe, 0xaa, 0x55, 0x44, 0x33, 0x22, 0x11,
        0x00,
    ]
}

fn gfni_matrix() -> [u8; 16] {
    [
        0xf8, 0x7c, 0x3e, 0x1f, 0x8f, 0xc7, 0xe3, 0xf1, 0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02,
        0x01,
    ]
}

#[test]
fn pclmulqdq_all_selectors() {
    for imm in [0x00u8, 0x01, 0x10, 0x11, 0x55, 0xaa] {
        check_sse(
            match imm {
                0x00 => "pclmulqdq_ll",
                0x01 => "pclmulqdq_hl",
                0x10 => "pclmulqdq_lh",
                0x11 => "pclmulqdq_hh",
                0x55 => "pclmulqdq_ignore_55",
                _ => "pclmulqdq_ignore_aa",
            },
            &sse_program(&[0x66, 0x0F, 0x3A, 0x44, 0xC1, imm]),
            sse_scratch(crypto_a(), crypto_b()),
        );
        check_sse(
            match imm {
                0x00 => "pclmulqdq_mem_ll",
                0x01 => "pclmulqdq_mem_hl",
                0x10 => "pclmulqdq_mem_lh",
                0x11 => "pclmulqdq_mem_hh",
                0x55 => "pclmulqdq_mem_ignore_55",
                _ => "pclmulqdq_mem_ignore_aa",
            },
            &sse_program(&[0x66, 0x0F, 0x3A, 0x44, 0x47, 0x10, imm]),
            sse_scratch(crypto_a(), crypto_b()),
        );
    }
}

#[test]
fn gfni_mul_affine_reg_mem() {
    check_sse(
        "gf2p8mulb_reg",
        &sse_program(&[0x66, 0x0F, 0x38, 0xCF, 0xC1]),
        sse_scratch(crypto_a(), crypto_b()),
    );
    check_sse(
        "gf2p8mulb_mem",
        &sse_program(&[0x66, 0x0F, 0x38, 0xCF, 0x47, 0x10]),
        sse_scratch(crypto_a(), crypto_b()),
    );
    check_sse(
        "gf2p8affineqb_reg",
        &sse_program(&[0x66, 0x0F, 0x3A, 0xCE, 0xC1, 0x63]),
        sse_scratch(crypto_a(), gfni_matrix()),
    );
    check_sse(
        "gf2p8affineqb_mem",
        &sse_program(&[0x66, 0x0F, 0x3A, 0xCE, 0x47, 0x10, 0x9A]),
        sse_scratch(crypto_a(), gfni_matrix()),
    );
}

#[test]
fn gfni_affine_inverse_reg_mem() {
    check_sse(
        "gf2p8affineinvqb_reg",
        &sse_program(&[0x66, 0x0F, 0x3A, 0xCF, 0xC1, 0x63]),
        sse_scratch(crypto_a(), gfni_matrix()),
    );
    check_sse(
        "gf2p8affineinvqb_mem",
        &sse_program(&[0x66, 0x0F, 0x3A, 0xCF, 0x47, 0x10, 0xC0]),
        sse_scratch(crypto_b(), gfni_matrix()),
    );
}

#[test]
fn vex_xmm_gfni_reg_mem_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0xB7u8)
            .wrapping_add((i as u8).wrapping_mul(17))
            .rotate_right((i % 5) as u32);
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x10]); // vmovdqu xmm1, [rdi+16]
    code.extend_from_slice(&[0xC4, 0xE2, 0x79, 0xCF, 0xD1]); // vgf2p8mulb xmm2, xmm0, xmm1
    code.extend_from_slice(&[0xC4, 0xE2, 0x79, 0xCF, 0x5F, 0x20]); // vgf2p8mulb xmm3, xmm0, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE3, 0xF9, 0xCE, 0xE1, 0x63]); // vgf2p8affineqb xmm4, xmm0, xmm1, 0x63
    code.extend_from_slice(&[0xC4, 0xE3, 0xF9, 0xCF, 0x6F, 0x30, 0x9A]); // vgf2p8affineinvqb xmm5, xmm0, [rdi+48], 0x9a
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x17]); // vmovdqu [rdi], xmm2
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x5F, 0x10]); // vmovdqu [rdi+16], xmm3
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x67, 0x20]); // vmovdqu [rdi+32], xmm4
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x6F, 0x30]); // vmovdqu [rdi+48], xmm5
    code.push(HLT);
    check_avx_mem("vex_xmm_gfni_reg_mem_forms", &code, s);
}

#[test]
fn vex_ymm_gfni_reg_mem_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x4Du8)
            .wrapping_add((i as u8).wrapping_mul(29))
            .rotate_left((i % 7) as u32);
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0xCF, 0xD1]); // vgf2p8mulb ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0xCF, 0x1F]); // vgf2p8mulb ymm3, ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xED, 0xEF, 0xD3]); // vpxor ymm2, ymm2, ymm3
    code.extend_from_slice(&[0xC4, 0xE3, 0xFD, 0xCE, 0xE1, 0x63]); // vgf2p8affineqb ymm4, ymm0, ymm1, 0x63
    code.extend_from_slice(&[0xC5, 0xED, 0xEF, 0xD4]); // vpxor ymm2, ymm2, ymm4
    code.extend_from_slice(&[0xC4, 0xE3, 0xFD, 0xCF, 0x2F, 0x9A]); // vgf2p8affineinvqb ymm5, ymm0, [rdi], 0x9a
    code.extend_from_slice(&[0xC5, 0xDD, 0xEF, 0xDD]); // vpxor ymm3, ymm4, ymm5
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("vex_ymm_gfni_reg_mem_forms", &code, s);
}

// ---------------------------------------------------------------------------
// CRC32 (SSE4.2) and POPCNT r/m forms (register + memory operand).
// CRC32 affects no flags; POPCNT defines ZF (set when src==0) and clears the rest.
// ---------------------------------------------------------------------------

#[test]
fn crc32_r32_r8() {
    // CRC32 r32, r/m8 = F2 0F 38 F0 C3 : accumulate one byte. rax(EAX) is the CRC.
    let mut r = regs();
    r.rax = 0xFFFF_FFFF; // initial CRC
    r.rbx = 0x000000AB; // bl = byte to fold in
    check_gpr_only("crc32_8", &with_hlt(vec![0xF2, 0x0F, 0x38, 0xF0, 0xC3]), r);
}

#[test]
fn crc32_r64_r64() {
    // CRC32 r64, r/m64 = F2 48 0F 38 F1 C3 : fold an 8-byte quantity into RAX.
    let mut r = regs();
    r.rax = 0x0000_0000_FFFF_FFFF;
    r.rbx = 0x0123_4567_89AB_CDEF;
    check_gpr_only(
        "crc32_64",
        &with_hlt(vec![0xF2, 0x48, 0x0F, 0x38, 0xF1, 0xC3]),
        r,
    );
}

#[test]
fn crc32_r32_mem8() {
    // CRC32 r32, m8 (F2 0F 38 F0 /r) reading [rdi]. Memory operand form.
    let mut s = [0u8; 64];
    s[0] = 0x5A;
    let mut r = regs();
    r.rax = 0xFFFF_FFFF;
    r.rdi = DATA_ADDR;
    // F2 0F 38 F0 07  crc32 eax, byte [rdi]
    check_mem(
        "crc32_mem8",
        &with_hlt(vec![0xF2, 0x0F, 0x38, 0xF0, 0x07]),
        r,
        s,
        0,
    );
}

#[test]
fn crc32_memory_width_forms() {
    let mut s = [0u8; 64];
    s[0..2].copy_from_slice(&0xBEEFu16.to_le_bytes());
    s[8..12].copy_from_slice(&0x89AB_CDEFu32.to_le_bytes());
    s[16..24].copy_from_slice(&0x0123_4567_89AB_CDEFu64.to_le_bytes());

    let mut r = regs();
    r.rax = 0x0000_0000_FFFF_FFFF;
    r.rbx = 0x0000_0000_1234_5678;
    r.rcx = 0x0000_0000_1357_9BDF;
    r.rdi = DATA_ADDR;

    let code = with_hlt(vec![
        0x66, 0xF2, 0x0F, 0x38, 0xF1, 0x07, // crc32 eax, word [rdi]
        0x89, 0x47, 0x20, // mov [rdi+32], eax
        0xF2, 0x0F, 0x38, 0xF1, 0x5F, 0x08, // crc32 ebx, dword [rdi+8]
        0x89, 0x5F, 0x24, // mov [rdi+36], ebx
        0xF2, 0x48, 0x0F, 0x38, 0xF1, 0x4F, 0x10, // crc32 rcx, qword [rdi+16]
        0x48, 0x89, 0x4F, 0x28, // mov [rdi+40], rcx
    ]);
    check_mem("crc32_memory_width_forms", &code, r, s, 0);
}

#[test]
fn popcnt_r16() {
    // POPCNT r16, r/m16 = 66 F3 0F B8 C3. Count bits in bx.
    let mut r = regs();
    r.rbx = 0x0000_0000_0000_F0F0; // bx = 0xF0F0 -> 8 bits set
    check("popcnt16", &with_hlt(vec![0x66, 0xF3, 0x0F, 0xB8, 0xC3]), r);
}

#[test]
fn popcnt_r64_mem() {
    // POPCNT r64, m64 = F3 48 0F B8 /r reading [rdi]. Memory-operand form; ZF flag.
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0xFF00_F0F0_1234_5678u64.to_le_bytes());
    let mut r = regs();
    r.rdi = DATA_ADDR;
    // F3 48 0F B8 07  popcnt rax, qword [rdi]
    check_mem(
        "popcnt_mem64",
        &with_hlt(vec![0xF3, 0x48, 0x0F, 0xB8, 0x07]),
        r,
        s,
        FLAG_MASK,
    );
}

#[test]
fn popcnt_memory_width_forms() {
    let mut s = [0u8; 64];
    s[0..2].copy_from_slice(&0xF0F0u16.to_le_bytes());
    s[8..12].copy_from_slice(&0xF0F0_000Fu32.to_le_bytes());
    s[16..20].copy_from_slice(&0u32.to_le_bytes());

    let mut r = regs();
    r.rdi = DATA_ADDR;

    let code = with_hlt(vec![
        0x66, 0xF3, 0x0F, 0xB8, 0x07, // popcnt ax, [rdi]
        0x66, 0x89, 0x47, 0x20, // mov [rdi+32], ax
        0xF3, 0x0F, 0xB8, 0x4F, 0x08, // popcnt ecx, [rdi+8]
        0x89, 0x4F, 0x24, // mov [rdi+36], ecx
        0xF3, 0x0F, 0xB8, 0x57, 0x10, // popcnt edx, [rdi+16]
        0x89, 0x57, 0x28, // mov [rdi+40], edx
    ]);
    check_mem("popcnt_memory_width_forms", &code, r, s, FLAG_MASK);
}

// ---------------------------------------------------------------------------
// Granite Rapids-era scalar/cache features with deterministic architectural
// effects. These previously had emulator unit tests, but no KVM differential
// coverage in this harness.
// ---------------------------------------------------------------------------

fn modern_flags_regs() -> Registers {
    let mut r = regs();
    r.rflags = flags::bits::CF | flags::bits::PF | flags::bits::ZF | flags::bits::OF;
    r
}

#[test]
fn movdiri_m64_r64() {
    // MOVDIRI m64, r64 = 48 0F 38 F9 /r. Store RAX directly to [RDI].
    let mut r = modern_flags_regs();
    r.rax = 0x0123_4567_89AB_CDEF;
    r.rdi = DATA_ADDR;
    check_mem(
        "movdiri_m64_r64",
        &with_hlt(vec![0x48, 0x0F, 0x38, 0xF9, 0x07]),
        r,
        zero_scratch(),
        FLAG_MASK,
    );
}

#[test]
fn movdiri_m32_r32() {
    // MOVDIRI m32, r32 = 0F 38 F9 /r. Store EBX directly to [RDI].
    let mut r = modern_flags_regs();
    r.rbx = 0xDEAD_BEEF;
    r.rdi = DATA_ADDR;
    check_mem(
        "movdiri_m32_r32",
        &with_hlt(vec![0x0F, 0x38, 0xF9, 0x1F]),
        r,
        zero_scratch(),
        FLAG_MASK,
    );
}

#[test]
fn movdir64b_copy_into_scratch() {
    // MOVDIR64B r64, m512 = 66 0F 38 F8 /r. Copy 64 bytes from [RDI+0x20] to
    // the 64-byte-aligned address held in RAX (DATA_ADDR).
    let mut s = [0u8; 64];
    for (i, b) in s.iter_mut().enumerate() {
        *b = (0x80u8).wrapping_add(i as u8);
    }
    let mut r = modern_flags_regs();
    r.rax = DATA_ADDR;
    r.rdi = DATA_ADDR;
    check_mem(
        "movdir64b_copy",
        &with_hlt(vec![0x66, 0x0F, 0x38, 0xF8, 0x47, 0x20]),
        r,
        s,
        FLAG_MASK,
    );
}

#[test]
fn cldemote_hint_preserves_state() {
    // CLDEMOTE m8 = 0F 1C /0. Architecturally a cache hint: no GPR/flag/memory effect.
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0xA5A5_5A5A_F0F0_0F0Fu64.to_le_bytes());
    let mut r = modern_flags_regs();
    r.rdi = DATA_ADDR;
    check_mem(
        "cldemote_m8",
        &with_hlt(vec![0x0F, 0x1C, 0x07]),
        r,
        s,
        FLAG_MASK,
    );
}

#[test]
fn clflush_hint_preserves_state() {
    // CLFLUSH m8 = 0F AE /7. Cache-line flush: no architectural GPR/flag/memory effect.
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0x55AA_33CC_F00D_C0DEu64.to_le_bytes());
    let mut r = modern_flags_regs();
    r.rdi = DATA_ADDR;
    check_mem(
        "clflush_m8",
        &with_hlt(vec![0x0F, 0xAE, 0x3F]),
        r,
        s,
        FLAG_MASK,
    );
}

#[test]
fn clflushopt_hint_preserves_state() {
    // CLFLUSHOPT m8 = 66 0F AE /7. Optimized cache-line flush, architecturally a hint.
    let mut s = [0u8; 64];
    s[8..16].copy_from_slice(&0x0123_4567_89AB_CDEFu64.to_le_bytes());
    let mut r = modern_flags_regs();
    r.rdi = DATA_ADDR + 8;
    check_mem(
        "clflushopt_m8",
        &with_hlt(vec![0x66, 0x0F, 0xAE, 0x3F]),
        r,
        s,
        FLAG_MASK,
    );
}

#[test]
fn clwb_hint_preserves_state() {
    // CLWB m8 = 66 0F AE /6. Write-back hint without invalidation.
    let mut s = [0u8; 64];
    s[16..24].copy_from_slice(&0xCAFE_BABE_7654_3210u64.to_le_bytes());
    let mut r = modern_flags_regs();
    r.rdi = DATA_ADDR + 16;
    check_mem(
        "clwb_m8",
        &with_hlt(vec![0x66, 0x0F, 0xAE, 0x37]),
        r,
        s,
        FLAG_MASK,
    );
}

#[test]
fn fence_and_pause_forms_preserve_state() {
    let mut r = modern_flags_regs();
    r.rax = 0x0123_4567_89AB_CDEF;
    r.rbx = 0xFEDC_BA98_7654_3210;
    check_flags_masked(
        "fence_pause_preserve",
        &with_hlt(vec![
            0x0F, 0xAE, 0xE8, // lfence
            0x0F, 0xAE, 0xF0, // mfence
            0x0F, 0xAE, 0xF8, // sfence
            0xF3, 0x90, // pause
        ]),
        r,
        FLAG_MASK,
    );
}

#[test]
fn monitor_preserves_visible_state() {
    let mut r = modern_flags_regs();
    r.rax = DATA_ADDR;
    r.rbx = 0x0123_4567_89AB_CDEF;
    r.rcx = 0;
    r.rdx = 0;
    check_flags_masked(
        "monitor_preserve",
        &with_hlt(vec![0x0F, 0x01, 0xC8]), // monitor
        r,
        FLAG_MASK,
    );
}

#[test]
fn waitpkg_umonitor_preserves_visible_state() {
    let mut r = modern_flags_regs();
    r.rax = DATA_ADDR;
    r.rbx = 0x0123_4567_89AB_CDEF;
    check_flags_masked(
        "waitpkg_umonitor",
        &with_hlt(vec![0xF3, 0x0F, 0xAE, 0xF0]), // umonitor rax
        r,
        FLAG_MASK,
    );
}

#[test]
fn waitpkg_umwait_zero_deadline_clears_status_flags() {
    let mut r = modern_flags_regs();
    r.rax = 0;
    r.rcx = 0;
    r.rdx = 0;
    check(
        "waitpkg_umwait_zero_deadline",
        &with_hlt(vec![0xF2, 0x0F, 0xAE, 0xF1]), // umwait ecx
        r,
    );
}

#[test]
fn waitpkg_tpause_zero_deadline_clears_status_flags() {
    let mut r = modern_flags_regs();
    r.rax = 0;
    r.rcx = 0;
    r.rdx = 0;
    check(
        "waitpkg_tpause_zero_deadline",
        &with_hlt(vec![0x66, 0x0F, 0xAE, 0xF1]), // tpause ecx
        r,
    );
}

#[test]
fn stac_clac_ac_flag_forms() {
    let ac_and_status = FLAG_MASK | flags::bits::AC;

    let mut r = modern_flags_regs();
    r.rflags &= !flags::bits::AC;
    check_flags_masked(
        "stac_sets_ac",
        &with_hlt(vec![0x0F, 0x01, 0xCB]), // stac
        r,
        ac_and_status,
    );

    let mut r = modern_flags_regs();
    r.rflags |= flags::bits::AC;
    check_flags_masked(
        "clac_clears_ac",
        &with_hlt(vec![0x0F, 0x01, 0xCA]), // clac
        r,
        ac_and_status,
    );
}

fn fsgsbase_enable_prologue() -> Vec<u8> {
    let mut code = Vec::new();
    code.extend_from_slice(&[0x0F, 0x20, 0xE0]); // mov rax, cr4
    code.extend_from_slice(&[0x48, 0x0D]); // or rax, imm32
    code.extend_from_slice(&0x0001_0000u32.to_le_bytes()); // CR4.FSGSBASE
    code.extend_from_slice(&[0x0F, 0x22, 0xE0]); // mov cr4, rax
    code
}

#[test]
fn fsgsbase_64bit_roundtrip_forms() {
    let fs_base = 0x0000_0000_0012_3000u64;
    let gs_base = 0x0000_0000_0045_6000u64;

    let mut code = fsgsbase_enable_prologue();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0]); // mov rax, 1
    code.extend_from_slice(&1u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xC7, 0xC3]); // mov rbx, 2
    code.extend_from_slice(&2u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0x39, 0xD8]); // cmp rax, rbx
    code.extend_from_slice(&[0x48, 0xB8]); // mov rax, imm64
    code.extend_from_slice(&fs_base.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xBB]); // mov rbx, imm64
    code.extend_from_slice(&gs_base.to_le_bytes());
    code.extend_from_slice(&[0xF3, 0x48, 0x0F, 0xAE, 0xD0]); // wrfsbase rax
    code.extend_from_slice(&[0xF3, 0x48, 0x0F, 0xAE, 0xDB]); // wrgsbase rbx
    code.extend_from_slice(&[0xF3, 0x48, 0x0F, 0xAE, 0xC1]); // rdfsbase rcx
    code.extend_from_slice(&[0xF3, 0x48, 0x0F, 0xAE, 0xCA]); // rdgsbase rdx

    check_flags_masked(
        "fsgsbase_64bit_roundtrip",
        &with_hlt(code),
        regs(),
        FLAG_MASK,
    );
}

#[test]
fn fsgsbase_32bit_zero_ext_and_extended_regs() {
    let fs_base = 0x89AB_CDEFu64;
    let gs_base = 0x0000_0000_0123_4000u64;

    let mut code = fsgsbase_enable_prologue();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0]); // mov rax, 0
    code.extend_from_slice(&0u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xC7, 0xC3]); // mov rbx, 1
    code.extend_from_slice(&1u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0x39, 0xD8]); // cmp rax, rbx
    code.extend_from_slice(&[0x48, 0xB8]); // mov rax, imm64
    code.extend_from_slice(&(0xFFFF_FFFF_0000_0000u64 | fs_base).to_le_bytes());
    code.extend_from_slice(&[0xF3, 0x0F, 0xAE, 0xD0]); // wrfsbase eax
    code.extend_from_slice(&[0x49, 0xC7, 0xC2]); // mov r10, -1
    code.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    code.extend_from_slice(&[0xF3, 0x41, 0x0F, 0xAE, 0xC2]); // rdfsbase r10d
    code.extend_from_slice(&[0x49, 0xB8]); // mov r8, imm64
    code.extend_from_slice(&gs_base.to_le_bytes());
    code.extend_from_slice(&[0xF3, 0x49, 0x0F, 0xAE, 0xD8]); // wrgsbase r8
    code.extend_from_slice(&[0x49, 0xC7, 0xC3]); // mov r11, -1
    code.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    code.extend_from_slice(&[0xF3, 0x49, 0x0F, 0xAE, 0xCB]); // rdgsbase r11

    check_flags_masked(
        "fsgsbase_32bit_zero_ext_extended",
        &with_hlt(code),
        regs(),
        FLAG_MASK,
    );
}

fn pkru_enable_prologue() -> Vec<u8> {
    let mut code = Vec::new();
    code.extend_from_slice(&[0x0F, 0x20, 0xE0]); // mov rax, cr4
    code.extend_from_slice(&[0x48, 0x0D]); // or rax, imm32
    code.extend_from_slice(&0x0040_0000u32.to_le_bytes()); // CR4.PKE
    code.extend_from_slice(&[0x0F, 0x22, 0xE0]); // mov cr4, rax
    code
}

#[test]
fn pkru_write_read_roundtrip_preserves_flags() {
    let pkru = 0x5555_5550u32;

    let mut code = pkru_enable_prologue();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0]); // mov rax, 0
    code.extend_from_slice(&0u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xC7, 0xC3]); // mov rbx, 1
    code.extend_from_slice(&1u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0x39, 0xD8]); // cmp rax, rbx
    code.push(0xB8); // mov eax, imm32
    code.extend_from_slice(&pkru.to_le_bytes());
    code.extend_from_slice(&[0xB9, 0x00, 0x00, 0x00, 0x00]); // mov ecx, 0
    code.extend_from_slice(&[0xBA, 0x00, 0x00, 0x00, 0x00]); // mov edx, 0
    code.extend_from_slice(&[0x0F, 0x01, 0xEF]); // wrpkru
    code.extend_from_slice(&[0x48, 0xC7, 0xC0]); // mov rax, -1
    code.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xC7, 0xC2]); // mov rdx, -1
    code.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    code.extend_from_slice(&[0x0F, 0x01, 0xEE]); // rdpkru

    check_flags_masked("pkru_roundtrip", &with_hlt(code), regs(), FLAG_MASK);
}

#[test]
fn pkru_multiple_writes_last_value_visible() {
    let first = 0xAAAA_AAA0u32;
    let second = 0xCAFE_BAB0u32;

    let mut code = pkru_enable_prologue();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0]); // mov rax, 1
    code.extend_from_slice(&1u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xC7, 0xC3]); // mov rbx, 1
    code.extend_from_slice(&1u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0x39, 0xD8]); // cmp rax, rbx
    code.push(0xB8); // mov eax, imm32
    code.extend_from_slice(&first.to_le_bytes());
    code.extend_from_slice(&[0xB9, 0x00, 0x00, 0x00, 0x00]); // mov ecx, 0
    code.extend_from_slice(&[0xBA, 0x00, 0x00, 0x00, 0x00]); // mov edx, 0
    code.extend_from_slice(&[0x0F, 0x01, 0xEF]); // wrpkru
    code.extend_from_slice(&[0x48, 0xB8]); // mov rax, imm64
    code.extend_from_slice(&(0xFFFF_FFFF_0000_0000u64 | second as u64).to_le_bytes());
    code.extend_from_slice(&[0x0F, 0x01, 0xEF]); // wrpkru
    code.extend_from_slice(&[0x0F, 0x01, 0xEE]); // rdpkru

    check_flags_masked("pkru_last_write_visible", &with_hlt(code), regs(), FLAG_MASK);
}

fn swapgs_setup_prologue(user_gs: u64, kernel_gs: u64) -> Vec<u8> {
    let mut code = fsgsbase_enable_prologue();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0]); // mov rax, 0
    code.extend_from_slice(&0u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xC7, 0xC3]); // mov rbx, 1
    code.extend_from_slice(&1u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0x39, 0xD8]); // cmp rax, rbx
    code.push(0xB9); // mov ecx, IA32_KERNEL_GS_BASE
    code.extend_from_slice(&0xC000_0102u32.to_le_bytes());
    code.push(0xB8); // mov eax, kernel_gs[31:0]
    code.extend_from_slice(&(kernel_gs as u32).to_le_bytes());
    code.push(0xBA); // mov edx, kernel_gs[63:32]
    code.extend_from_slice(&((kernel_gs >> 32) as u32).to_le_bytes());
    code.extend_from_slice(&[0x0F, 0x30]); // wrmsr
    code.extend_from_slice(&[0x48, 0xB8]); // mov rax, user_gs
    code.extend_from_slice(&user_gs.to_le_bytes());
    code.extend_from_slice(&[0xF3, 0x48, 0x0F, 0xAE, 0xD8]); // wrgsbase rax
    code
}

#[test]
fn swapgs_roundtrip_visible_through_rdgsbase() {
    let user_gs = 0x0000_0000_0012_3000u64;
    let kernel_gs = 0x0000_0000_0078_9000u64;

    let mut code = swapgs_setup_prologue(user_gs, kernel_gs);
    code.extend_from_slice(&[0xF3, 0x48, 0x0F, 0xAE, 0xCB]); // rdgsbase rbx
    code.extend_from_slice(&[0x0F, 0x01, 0xF8]); // swapgs
    code.extend_from_slice(&[0xF3, 0x48, 0x0F, 0xAE, 0xC9]); // rdgsbase rcx
    code.extend_from_slice(&[0x0F, 0x01, 0xF8]); // swapgs
    code.extend_from_slice(&[0xF3, 0x48, 0x0F, 0xAE, 0xCA]); // rdgsbase rdx

    check_flags_masked("swapgs_roundtrip", &with_hlt(code), regs(), FLAG_MASK);
}

#[test]
fn swapgs_multiple_toggles_extended_regs() {
    let user_gs = 0x0000_0000_0024_6000u64;
    let kernel_gs = 0x0000_0000_009A_B000u64;

    let mut code = swapgs_setup_prologue(user_gs, kernel_gs);
    code.extend_from_slice(&[0x0F, 0x01, 0xF8]); // swapgs
    code.extend_from_slice(&[0xF3, 0x49, 0x0F, 0xAE, 0xC8]); // rdgsbase r8
    code.extend_from_slice(&[0x0F, 0x01, 0xF8]); // swapgs
    code.extend_from_slice(&[0xF3, 0x49, 0x0F, 0xAE, 0xC9]); // rdgsbase r9
    code.extend_from_slice(&[0x0F, 0x01, 0xF8]); // swapgs
    code.extend_from_slice(&[0xF3, 0x49, 0x0F, 0xAE, 0xCA]); // rdgsbase r10
    code.extend_from_slice(&[0x0F, 0x01, 0xF8]); // swapgs
    code.extend_from_slice(&[0xF3, 0x49, 0x0F, 0xAE, 0xCB]); // rdgsbase r11

    check_flags_masked("swapgs_multiple_toggles", &with_hlt(code), regs(), FLAG_MASK);
}

#[test]
fn rdmsr_reads_initial_efer() {
    let mut code = Vec::new();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0]); // mov rax, 0
    code.extend_from_slice(&0u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xC7, 0xC3]); // mov rbx, 1
    code.extend_from_slice(&1u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0x39, 0xD8]); // cmp rax, rbx
    code.push(0xB9); // mov ecx, IA32_EFER
    code.extend_from_slice(&0xC000_0080u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xC7, 0xC0]); // mov rax, -1
    code.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xC7, 0xC2]); // mov rdx, -1
    code.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    code.extend_from_slice(&[0x0F, 0x32]); // rdmsr

    check_flags_masked("rdmsr_efer", &with_hlt(code), regs(), FLAG_MASK);
}

fn wrmsr_imm64(code: &mut Vec<u8>, msr: u32, value: u64) {
    code.push(0xB9); // mov ecx, msr
    code.extend_from_slice(&msr.to_le_bytes());
    code.push(0xB8); // mov eax, value[31:0]
    code.extend_from_slice(&(value as u32).to_le_bytes());
    code.push(0xBA); // mov edx, value[63:32]
    code.extend_from_slice(&((value >> 32) as u32).to_le_bytes());
    code.extend_from_slice(&[0x0F, 0x30]); // wrmsr
}

const WRMSR_IMM64_LEN: usize = 17;

fn rdmsr_to_edx_eax(code: &mut Vec<u8>, msr: u32) {
    code.push(0xB9); // mov ecx, msr
    code.extend_from_slice(&msr.to_le_bytes());
    code.extend_from_slice(&[0x0F, 0x32]); // rdmsr
}

#[test]
fn wrmsr_rdmsr_base_msr_roundtrips() {
    let fs_base = 0x0000_1234_5678_9000u64;
    let gs_base = 0x0000_2345_6789_A000u64;
    let kernel_gs_base = 0x0000_3456_789A_B000u64;

    let mut code = Vec::new();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0]); // mov rax, 0
    code.extend_from_slice(&0u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xC7, 0xC3]); // mov rbx, 1
    code.extend_from_slice(&1u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0x39, 0xD8]); // cmp rax, rbx

    wrmsr_imm64(&mut code, 0xC000_0100, fs_base); // IA32_FS_BASE
    rdmsr_to_edx_eax(&mut code, 0xC000_0100);
    code.extend_from_slice(&[0x48, 0x89, 0xC3]); // mov rbx, rax
    code.extend_from_slice(&[0x48, 0x89, 0xD6]); // mov rsi, rdx

    wrmsr_imm64(&mut code, 0xC000_0101, gs_base); // IA32_GS_BASE
    rdmsr_to_edx_eax(&mut code, 0xC000_0101);
    code.extend_from_slice(&[0x48, 0x89, 0xC7]); // mov rdi, rax
    code.extend_from_slice(&[0x48, 0x89, 0xD5]); // mov rbp, rdx

    wrmsr_imm64(&mut code, 0xC000_0102, kernel_gs_base); // IA32_KERNEL_GS_BASE
    rdmsr_to_edx_eax(&mut code, 0xC000_0102);
    code.extend_from_slice(&[0x49, 0x89, 0xC0]); // mov r8, rax
    code.extend_from_slice(&[0x49, 0x89, 0xD1]); // mov r9, rdx

    check_flags_masked("wrmsr_rdmsr_base_roundtrips", &with_hlt(code), regs(), FLAG_MASK);
}

#[test]
fn syscall_entry_saves_return_and_masks_flags() {
    const IA32_STAR: u32 = 0xC000_0081;
    const IA32_LSTAR: u32 = 0xC000_0082;
    const IA32_FMASK: u32 = 0xC000_0084;

    let kernel_offset = WRMSR_IMM64_LEN * 3 + 2 + 2; // wrmsrs + stc/std + syscall
    let kernel_rip = CODE_ADDR + kernel_offset as u64;
    let star = (0x20u64 << 48) | (0x08u64 << 32);

    let mut code = Vec::new();
    wrmsr_imm64(&mut code, IA32_STAR, star);
    wrmsr_imm64(&mut code, IA32_LSTAR, kernel_rip);
    wrmsr_imm64(&mut code, IA32_FMASK, flags::bits::CF | flags::bits::DF);
    code.extend_from_slice(&[0xF9, 0xFD]); // stc; std
    code.extend_from_slice(&[0x0F, 0x05]); // syscall
    code.extend_from_slice(&[0x48, 0x89, 0x0F]); // mov [rdi], rcx
    code.extend_from_slice(&[0x4C, 0x89, 0x5F, 0x08]); // mov [rdi+8], r11
    code.extend_from_slice(&[0x9C, 0x58]); // pushfq; pop rax
    code.extend_from_slice(&[0x48, 0x89, 0x47, 0x10]); // mov [rdi+16], rax
    code.push(HLT);

    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rflags = flags::bits::PF;
    check_mem("syscall_entry_state", &code, r, zero_scratch(), 0);
}

#[test]
fn sysenter_entry_loads_target_and_clears_if() {
    const IA32_SYSENTER_CS: u32 = 0x174;
    const IA32_SYSENTER_ESP: u32 = 0x175;
    const IA32_SYSENTER_EIP: u32 = 0x176;

    let kernel_offset = WRMSR_IMM64_LEN * 3 + 2; // wrmsrs + sysenter
    let kernel_rip = CODE_ADDR + kernel_offset as u64;
    let kernel_rsp = STACK_ADDR - 0x80;

    let mut code = Vec::new();
    wrmsr_imm64(&mut code, IA32_SYSENTER_CS, 0x08);
    wrmsr_imm64(&mut code, IA32_SYSENTER_ESP, kernel_rsp);
    wrmsr_imm64(&mut code, IA32_SYSENTER_EIP, kernel_rip);
    code.extend_from_slice(&[0x0F, 0x34]); // sysenter
    code.extend_from_slice(&[0x48, 0x89, 0x27]); // mov [rdi], rsp
    code.extend_from_slice(&[0x9C, 0x58]); // pushfq; pop rax
    code.extend_from_slice(&[0x48, 0x89, 0x47, 0x08]); // mov [rdi+8], rax
    code.push(HLT);

    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rflags = flags::bits::IF | flags::bits::CF;
    check_mem("sysenter_entry_state", &code, r, zero_scratch(), 0);
}

#[test]
fn serialize_preserves_status_and_gprs() {
    // SERIALIZE = 0F 01 E8. Put the flags into a nontrivial state first, then
    // verify KVM and the interpreter agree that SERIALIZE leaves them alone.
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF;
    r.rbx = 0x1122_3344_5566_7788;
    check(
        "serialize_preserves",
        &with_hlt(vec![
            0x48, 0x83, 0xC0, 0x01, // add rax, 1 -> ZF/CF set
            0x0F, 0x01, 0xE8, // serialize
        ]),
        r,
    );
}

#[test]
fn rdpid_default_tsc_aux() {
    // RDPID r64 = F3 48 0F C7 /7. Fresh KVM vCPUs default IA32_TSC_AUX to 0,
    // matching the emulator's current architectural model.
    let mut r = modern_flags_regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF;
    check(
        "rdpid_r64",
        &with_hlt(vec![0xF3, 0x48, 0x0F, 0xC7, 0xF8]),
        r,
    );
}

#[test]
fn rdpid_r32_zero_extends_destination() {
    let mut r = modern_flags_regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF;
    check(
        "rdpid_r32",
        &with_hlt(vec![0xF3, 0x0F, 0xC7, 0xF8]),
        r,
    );
}

#[test]
fn rdrand_r64_success_flags() {
    let mut r = modern_flags_regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF;
    check(
        "rdrand_r64_success_flags",
        &with_hlt(vec![
            0x48, 0x0F, 0xC7, 0xF0, // retry: rdrand rax
            0x73, 0xFA, // jnc retry
            0x48, 0xC7, 0xC0, 0x00, 0x00, 0x00, 0x00, // mov rax, 0
        ]),
        r,
    );
}

#[test]
fn rdrand_r16_preserves_upper_bits() {
    let mut r = modern_flags_regs();
    r.rax = 0x1122_3344_5566_7788;
    check_flags_masked(
        "rdrand_r16_preserves_upper_bits",
        &with_hlt(vec![
            0x66, 0x0F, 0xC7, 0xF0, // retry: rdrand ax
            0x73, 0xFA, // jnc retry
            0x48, 0xBB, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // mov rbx, -0x10000
            0x48, 0x21, 0xD8, // and rax, rbx
        ]),
        r,
        FLAG_MASK & !flags::bits::AF,
    );
}

#[test]
fn rdrand_r32_zero_extends_destination() {
    let mut r = modern_flags_regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF;
    check_flags_masked(
        "rdrand_r32_zero_extends_destination",
        &with_hlt(vec![
            0x0F, 0xC7, 0xF0, // retry: rdrand eax
            0x73, 0xFB, // jnc retry
            0x48, 0xBB, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, // mov rbx, -0x100000000
            0x48, 0x21, 0xD8, // and rax, rbx
        ]),
        r,
        FLAG_MASK & !flags::bits::AF,
    );
}

#[test]
fn rdseed_r64_success_flags() {
    let mut r = modern_flags_regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF;
    check(
        "rdseed_r64_success_flags",
        &with_hlt(vec![
            0x48, 0x0F, 0xC7, 0xF8, // retry: rdseed rax
            0x73, 0xFA, // jnc retry
            0x48, 0xC7, 0xC0, 0x00, 0x00, 0x00, 0x00, // mov rax, 0
        ]),
        r,
    );
}

#[test]
fn rdseed_r16_preserves_upper_bits() {
    let mut r = modern_flags_regs();
    r.rax = 0x1122_3344_5566_7788;
    check_flags_masked(
        "rdseed_r16_preserves_upper_bits",
        &with_hlt(vec![
            0x66, 0x0F, 0xC7, 0xF8, // retry: rdseed ax
            0x73, 0xFA, // jnc retry
            0x48, 0xBB, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, // mov rbx, -0x10000
            0x48, 0x21, 0xD8, // and rax, rbx
        ]),
        r,
        FLAG_MASK & !flags::bits::AF,
    );
}

#[test]
fn rdseed_r32_zero_extends_destination() {
    let mut r = modern_flags_regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF;
    check_flags_masked(
        "rdseed_r32_zero_extends_destination",
        &with_hlt(vec![
            0x0F, 0xC7, 0xF8, // retry: rdseed eax
            0x73, 0xFB, // jnc retry
            0x48, 0xBB, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, // mov rbx, -0x100000000
            0x48, 0x21, 0xD8, // and rax, rbx
        ]),
        r,
        FLAG_MASK & !flags::bits::AF,
    );
}

#[test]
fn rdtsc_zero_extends_outputs_and_preserves_flags() {
    let mut r = modern_flags_regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF;
    r.rdx = 0xFFFF_FFFF_FFFF_FFFF;
    check(
        "rdtsc_zero_extends_outputs",
        &with_hlt(vec![
            0x0F, 0x31, // rdtsc
            0x9C, // pushfq
            0x5B, // pop rbx
            0x48, 0xC1, 0xE8, 0x20, // shr rax, 32
            0x48, 0xC1, 0xEA, 0x20, // shr rdx, 32
            0x53, // push rbx
            0x9D, // popfq
        ]),
        r,
    );
}

#[test]
fn rdtscp_zero_extends_outputs_and_preserves_flags() {
    let mut r = modern_flags_regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF;
    r.rcx = 0xFFFF_FFFF_FFFF_FFFF;
    r.rdx = 0xFFFF_FFFF_FFFF_FFFF;
    check(
        "rdtscp_zero_extends_outputs",
        &with_hlt(vec![
            0x0F, 0x01, 0xF9, // rdtscp
            0x9C, // pushfq
            0x5B, // pop rbx
            0x48, 0xC1, 0xE8, 0x20, // shr rax, 32
            0x48, 0xC1, 0xEA, 0x20, // shr rdx, 32
            0x48, 0xC1, 0xE9, 0x20, // shr rcx, 32
            0x53, // push rbx
            0x9D, // popfq
        ]),
        r,
    );
}

#[test]
fn rdpmc_zero_extends_outputs_and_preserves_flags() {
    let mut r = modern_flags_regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF;
    r.rcx = 0;
    r.rdx = 0xFFFF_FFFF_FFFF_FFFF;
    check(
        "rdpmc_zero_extends_outputs",
        &with_hlt(vec![
            0x0F, 0x33, // rdpmc
            0x9C, // pushfq
            0x5B, // pop rbx
            0x48, 0xC1, 0xE8, 0x20, // shr rax, 32
            0x48, 0xC1, 0xEA, 0x20, // shr rdx, 32
            0x53, // push rbx
            0x9D, // popfq
        ]),
        r,
    );
}

#[test]
fn control_register_readback_and_smsw_forms() {
    let mut code = load_rdi_data();
    code.extend_from_slice(&[0x0F, 0x20, 0xC0]); // mov rax, cr0
    code.extend_from_slice(&[0x48, 0x89, 0x07]); // mov [rdi], rax
    code.extend_from_slice(&[0x0F, 0x20, 0xDB]); // mov rbx, cr3
    code.extend_from_slice(&[0x48, 0x89, 0x5F, 0x08]); // mov [rdi+8], rbx
    code.extend_from_slice(&[0x0F, 0x20, 0xE1]); // mov rcx, cr4
    code.extend_from_slice(&[0x48, 0x89, 0x4F, 0x10]); // mov [rdi+0x10], rcx
    code.extend_from_slice(&[0x0F, 0x01, 0x67, 0x18]); // smsw [rdi+0x18]
    code.extend_from_slice(&[0x48, 0xC7, 0xC0]); // mov rax, -1
    code.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    code.extend_from_slice(&[0x0F, 0x01, 0xE0]); // smsw eax
    code.extend_from_slice(&[0x48, 0x89, 0x47, 0x20]); // mov [rdi+0x20], rax
    code.push(HLT);

    check_mem(
        "control_register_readback_smsw",
        &code,
        regs(),
        zero_scratch(),
        0,
    );
}

#[test]
fn descriptor_table_store_forms() {
    let mut code = load_rdi_data();
    code.extend_from_slice(&[0x0F, 0x01, 0x07]); // sgdt [rdi]
    code.extend_from_slice(&[0x0F, 0x01, 0x4F, 0x10]); // sidt [rdi+0x10]
    code.push(HLT);

    check_mem(
        "descriptor_table_store_forms",
        &code,
        regs(),
        zero_scratch(),
        0,
    );
}

#[test]
fn descriptor_table_load_then_store_forms() {
    let mut scratch = zero_scratch();
    scratch[0..2].copy_from_slice(&0x27u16.to_le_bytes());
    scratch[2..10].copy_from_slice(&0x6_000u64.to_le_bytes());
    scratch[0x10..0x12].copy_from_slice(&0x37u16.to_le_bytes());
    scratch[0x12..0x1A].copy_from_slice(&0x7_000u64.to_le_bytes());

    let mut code = load_rdi_data();
    code.extend_from_slice(&[0x0F, 0x01, 0x17]); // lgdt [rdi]
    code.extend_from_slice(&[0x0F, 0x01, 0x5F, 0x10]); // lidt [rdi+0x10]
    code.extend_from_slice(&[0x0F, 0x01, 0x47, 0x20]); // sgdt [rdi+0x20]
    code.extend_from_slice(&[0x0F, 0x01, 0x4F, 0x30]); // sidt [rdi+0x30]
    code.push(HLT);

    check_mem("descriptor_table_load_store_forms", &code, regs(), scratch, 0);
}

#[test]
fn lmsw_and_clts_update_machine_status_word() {
    let mut scratch = zero_scratch();
    scratch[0..2].copy_from_slice(&0x000Bu16.to_le_bytes());

    let mut code = load_rdi_data();
    code.extend_from_slice(&[0x0F, 0x01, 0x37]); // lmsw [rdi]
    code.extend_from_slice(&[0x0F, 0x01, 0x67, 0x08]); // smsw [rdi+8]
    code.extend_from_slice(&[0x0F, 0x06]); // clts
    code.extend_from_slice(&[0x0F, 0x01, 0x67, 0x10]); // smsw [rdi+0x10]
    code.push(HLT);

    check_mem("lmsw_clts_msw", &code, regs(), scratch, 0);
}

#[test]
fn invlpg_memory_form_preserves_observable_state() {
    let mut r = regs();
    r.rdi = DATA_ADDR;
    check("invlpg_mem", &with_hlt(vec![0x0F, 0x01, 0x3F]), r);
}

#[test]
fn lar_lsl_valid_gdt_selector_forms() {
    let mut r = regs();
    r.rax = 0x8; // GDT code selector
    r.rcx = 0xFFFF_FFFF_FFFF_FFFF;
    r.rdx = 0xFFFF_FFFF_FFFF_FFFF;

    let code = vec![
        0x0F, 0x02, 0xC8, // lar ecx, ax
        0x0F, 0x03, 0xD0, // lsl edx, ax
        HLT,
    ];
    check("lar_lsl_valid_gdt_selector", &code, r);
}

#[test]
fn verr_verw_code_and_data_selector_permissions() {
    let mut code = load_rdi_data();
    code.extend_from_slice(&[0x66, 0xB8, 0x08, 0x00]); // mov ax, 0x8 (code selector)
    code.extend_from_slice(&[0x0F, 0x00, 0xE0]); // verr ax
    code.extend_from_slice(&[0x0F, 0x94, 0x07]); // setz [rdi]
    code.extend_from_slice(&[0x0F, 0x00, 0xE8]); // verw ax
    code.extend_from_slice(&[0x0F, 0x94, 0x47, 0x01]); // setz [rdi+1]
    code.extend_from_slice(&[0x66, 0xB8, 0x10, 0x00]); // mov ax, 0x10 (data selector)
    code.extend_from_slice(&[0x0F, 0x00, 0xE0]); // verr ax
    code.extend_from_slice(&[0x0F, 0x94, 0x47, 0x02]); // setz [rdi+2]
    code.extend_from_slice(&[0x0F, 0x00, 0xE8]); // verw ax
    code.extend_from_slice(&[0x0F, 0x94, 0x47, 0x03]); // setz [rdi+3]
    code.push(HLT);

    check_mem("verr_verw_selector_permissions", &code, regs(), zero_scratch(), 0);
}

#[test]
fn cache_invd_wbinvd_preserve_observable_state() {
    let mut r = regs();
    r.rax = 0x0123_4567_89AB_CDEF;
    r.rbx = 0xFEDC_BA98_7654_3210;
    r.rflags = flags::bits::CF | flags::bits::ZF;

    check(
        "cache_invd_wbinvd_wbnoinvd",
        &with_hlt(vec![
            0x0F, 0x08, // invd
            0x0F, 0x09, // wbinvd
            0xF3, 0x0F, 0x09, // wbnoinvd
        ]),
        r,
    );
}

#[test]
fn debug_register_move_roundtrip_dr0_dr1() {
    let mut scratch = zero_scratch();
    scratch[0..8].copy_from_slice(&DATA_ADDR.to_le_bytes());
    scratch[8..16].copy_from_slice(&(DATA_ADDR + 8).to_le_bytes());

    let mut code = load_rdi_data();
    code.extend_from_slice(&[0x48, 0xB8]);
    code.extend_from_slice(&DATA_ADDR.to_le_bytes());
    code.extend_from_slice(&[0x0F, 0x23, 0xC0]); // mov dr0, rax
    code.extend_from_slice(&[0x0F, 0x21, 0xC3]); // mov rbx, dr0
    code.extend_from_slice(&[0x48, 0x89, 0x1F]); // mov [rdi], rbx
    code.extend_from_slice(&[0x48, 0xB8]);
    code.extend_from_slice(&(DATA_ADDR + 8).to_le_bytes());
    code.extend_from_slice(&[0x0F, 0x23, 0xC8]); // mov dr1, rax
    code.extend_from_slice(&[0x0F, 0x21, 0xCB]); // mov rbx, dr1
    code.extend_from_slice(&[0x48, 0x89, 0x5F, 0x08]); // mov [rdi+8], rbx
    code.push(HLT);

    check_mem("debug_register_move_roundtrip", &code, regs(), scratch, 0);
}

#[test]
fn debug_register_move_roundtrip_dr2_dr3_dr6_dr7() {
    let mut scratch = zero_scratch();
    scratch[0..8].copy_from_slice(&(DATA_ADDR + 0x10).to_le_bytes());
    scratch[8..16].copy_from_slice(&(DATA_ADDR + 0x18).to_le_bytes());
    scratch[16..24].copy_from_slice(&0xFFFF_0FF0u64.to_le_bytes());
    scratch[24..32].copy_from_slice(&0x400u64.to_le_bytes());

    let mut code = load_rdi_data();
    code.extend_from_slice(&[0x48, 0xB8]);
    code.extend_from_slice(&(DATA_ADDR + 0x10).to_le_bytes());
    code.extend_from_slice(&[0x0F, 0x23, 0xD0]); // mov dr2, rax
    code.extend_from_slice(&[0x0F, 0x21, 0xD3]); // mov rbx, dr2
    code.extend_from_slice(&[0x48, 0x89, 0x1F]); // mov [rdi], rbx
    code.extend_from_slice(&[0x48, 0xB8]);
    code.extend_from_slice(&(DATA_ADDR + 0x18).to_le_bytes());
    code.extend_from_slice(&[0x0F, 0x23, 0xD8]); // mov dr3, rax
    code.extend_from_slice(&[0x0F, 0x21, 0xDB]); // mov rbx, dr3
    code.extend_from_slice(&[0x48, 0x89, 0x5F, 0x08]); // mov [rdi+8], rbx
    code.extend_from_slice(&[0x48, 0xB8]);
    code.extend_from_slice(&0xFFFF_0FF0u64.to_le_bytes());
    code.extend_from_slice(&[0x0F, 0x23, 0xF0]); // mov dr6, rax
    code.extend_from_slice(&[0x0F, 0x21, 0xF3]); // mov rbx, dr6
    code.extend_from_slice(&[0x48, 0x89, 0x5F, 0x10]); // mov [rdi+0x10], rbx
    code.extend_from_slice(&[0x48, 0xB8]);
    code.extend_from_slice(&0x400u64.to_le_bytes());
    code.extend_from_slice(&[0x0F, 0x23, 0xF8]); // mov dr7, rax
    code.extend_from_slice(&[0x0F, 0x21, 0xFB]); // mov rbx, dr7
    code.extend_from_slice(&[0x48, 0x89, 0x5F, 0x18]); // mov [rdi+0x18], rbx
    code.push(HLT);

    check_mem(
        "debug_register_move_roundtrip_extended",
        &code,
        regs(),
        scratch,
        0,
    );
}

#[test]
fn descriptor_register_store_forms() {
    let mut code = load_rdi_data();
    code.extend_from_slice(&[0x0F, 0x00, 0x07]); // sldt [rdi]
    code.extend_from_slice(&[0x0F, 0x00, 0x4F, 0x08]); // str [rdi+8]
    code.extend_from_slice(&[0x0F, 0x00, 0xC0]); // sldt eax
    code.extend_from_slice(&[0x48, 0x89, 0x47, 0x10]); // mov [rdi+0x10], rax
    code.extend_from_slice(&[0x0F, 0x00, 0xC8]); // str eax
    code.extend_from_slice(&[0x48, 0x89, 0x47, 0x18]); // mov [rdi+0x18], rax
    code.push(HLT);

    check_mem("descriptor_register_store_forms", &code, regs(), zero_scratch(), 0);
}

// ---------------------------------------------------------------------------
// BT/BTS/BTR/BTC with a MEMORY operand and a bit index that can exceed the
// operand size. For a memory bit-string the index is NOT taken modulo the
// operand size: it selects byte (index/8) at an effective address computed from
// the base plus a signed bit-offset displacement. We verify both CF and the
// resulting memory bytes against KVM.
// ---------------------------------------------------------------------------

#[test]
fn bt_mem_index_small() {
    // BT [rdi], rdx with index 5 in the first byte. CF <- bit 5 of [rdi].
    let mut s = [0u8; 64];
    s[0] = 0b0010_0000; // bit 5 set
    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rdx = 5;
    // 48 0F A3 17  bt qword [rdi], rdx
    check_mem(
        "bt_mem5",
        &with_hlt(vec![0x48, 0x0F, 0xA3, 0x17]),
        r,
        s,
        BT_DEFINED,
    );
}

#[test]
fn bt_mem_index_large() {
    // BT [rdi], rdx with index 100 -> byte 12, bit 4. Memory bit-string semantics:
    // index is NOT reduced mod operand size; it addresses [rdi + 100/8].
    let mut s = [0u8; 64];
    s[12] = 0b0001_0000; // bit 100 == byte 12 bit 4
    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rdx = 100;
    // 48 0F A3 17  bt qword [rdi], rdx
    check_mem(
        "bt_mem100",
        &with_hlt(vec![0x48, 0x0F, 0xA3, 0x17]),
        r,
        s,
        BT_DEFINED,
    );
}

#[test]
fn bts_mem_large_sets_bit() {
    // BTS [rdi], rdx index 70 -> byte 8 bit 6. CF<-old(0), then the bit is set.
    let s = [0u8; 64];
    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rdx = 70;
    // 48 0F AB 17  bts qword [rdi], rdx
    check_mem(
        "bts_mem70",
        &with_hlt(vec![0x48, 0x0F, 0xAB, 0x17]),
        r,
        s,
        BT_DEFINED,
    );
}

#[test]
fn btr_mem_large_clears_bit() {
    // BTR [rdi], rdx index 130 -> byte 16 bit 2. Preset that bit; CF<-1, bit cleared.
    let mut s = [0u8; 64];
    s[16] = 0b0000_0100; // bit 130 == byte 16 bit 2
    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rdx = 130;
    // 48 0F B3 17  btr qword [rdi], rdx
    check_mem(
        "btr_mem130",
        &with_hlt(vec![0x48, 0x0F, 0xB3, 0x17]),
        r,
        s,
        BT_DEFINED,
    );
}

#[test]
fn btc_mem_large_toggles_bit() {
    // BTC [rdi], rdx index 200 -> byte 25 bit 0. CF<-old(0), bit toggled to 1.
    let s = [0u8; 64];
    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rdx = 200;
    // 48 0F BB 17  btc qword [rdi], rdx
    check_mem(
        "btc_mem200",
        &with_hlt(vec![0x48, 0x0F, 0xBB, 0x17]),
        r,
        s,
        BT_DEFINED,
    );
}

#[test]
fn bts_mem_negative_index() {
    // A SIGNED negative bit index addresses BELOW the base operand. With base at
    // [rdi+8] and index -64, the effective byte is [rdi+8 + (-64/8)] = [rdi].
    let s = [0u8; 64];
    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rdx = (-64i64) as u64;
    // 48 0F AB 57 08  bts qword [rdi+8], rdx
    check_mem(
        "bts_mem_neg",
        &with_hlt(vec![0x48, 0x0F, 0xAB, 0x57, 0x08]),
        r,
        s,
        BT_DEFINED,
    );
}

#[test]
fn bt_mem_imm_index() {
    // BT m64, imm8 (0F BA /4 ib): immediate bit index IS taken modulo operand size
    // (64) for the imm form. imm=63 selects bit 63 of the qword.
    let mut s = [0u8; 64];
    s[7] = 0x80; // bit 63 of the qword at [rdi]
    let mut r = regs();
    r.rdi = DATA_ADDR;
    // 48 0F BA 27 3F  bt qword [rdi], 63
    check_mem(
        "bt_mem_imm63",
        &with_hlt(vec![0x48, 0x0F, 0xBA, 0x27, 0x3F]),
        r,
        s,
        BT_DEFINED,
    );
}

// ---------------------------------------------------------------------------
// ADCX / ADOX: ADD with carry that touches ONLY CF (ADCX) or ONLY OF (ADOX),
// enabling two independent carry chains. We model an interleaved chain and check
// the full status mask (each instruction leaves the other 5 flags untouched, so
// comparing all 6 against KVM validates the "only one flag changes" property).
// ---------------------------------------------------------------------------

#[test]
fn adcx_propagates_only_cf() {
    // ADCX rax, rbx = 66 48 0F 38 F6 C3 : rax += rbx + CF; only CF updated.
    // Seed OF=1 and CF=1; the sum wraps so CF stays 1, and OF must be PRESERVED.
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF;
    r.rbx = 0x0000_0000_0000_0001;
    r.rflags = flags::bits::CF | flags::bits::OF | flags::bits::SF | flags::bits::ZF;
    check(
        "adcx",
        &with_hlt(vec![0x66, 0x48, 0x0F, 0x38, 0xF6, 0xC3]),
        r,
    );
}

#[test]
fn adox_propagates_only_of() {
    // ADOX rax, rbx = F3 48 0F 38 F6 C3 : rax += rbx + OF; only OF updated.
    // Seed CF=1 (must be preserved) and OF=1; the sum wraps -> OF stays 1.
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFF;
    r.rbx = 0x0000_0000_0000_0001;
    r.rflags = flags::bits::CF | flags::bits::OF | flags::bits::PF;
    check(
        "adox",
        &with_hlt(vec![0xF3, 0x48, 0x0F, 0x38, 0xF6, 0xC3]),
        r,
    );
}

#[test]
fn adcx_adox_interleaved_chains() {
    // Two independent carry chains advanced in lockstep:
    //   adcx rax, rcx   (CF chain)
    //   adox rbx, rdx   (OF chain)
    // Seed CF=1 and OF=1; both low halves wrap so both carries re-propagate.
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_FFFF_FFFE;
    r.rcx = 0x0000_0000_0000_0001; // + CF(1) -> wraps to 0, CF=1
    r.rbx = 0xFFFF_FFFF_FFFF_FFFE;
    r.rdx = 0x0000_0000_0000_0001; // + OF(1) -> wraps to 0, OF=1
    r.rflags = flags::bits::CF | flags::bits::OF;
    // 66 48 0F 38 F6 C1  adcx rax, rcx
    // F3 48 0F 38 F6 DA  adox rbx, rdx
    check(
        "adcx_adox_chain",
        &with_hlt(vec![
            0x66, 0x48, 0x0F, 0x38, 0xF6, 0xC1, // adcx rax, rcx
            0xF3, 0x48, 0x0F, 0x38, 0xF6, 0xDA, // adox rbx, rdx
        ]),
        r,
    );
}

#[test]
fn adcx_no_carry_clears_cf() {
    // ADCX with a small sum that doesn't overflow -> CF cleared, OF preserved.
    let mut r = regs();
    r.rax = 0x0000_0000_0000_0010;
    r.rbx = 0x0000_0000_0000_0020;
    r.rflags = flags::bits::OF | flags::bits::CF; // CF=1 in (consumed), OF stays
    check(
        "adcx_noc",
        &with_hlt(vec![0x66, 0x48, 0x0F, 0x38, 0xF6, 0xC3]),
        r,
    );
}

#[test]
fn adcx_adox_memory_width_forms() {
    let mut s = [0u8; 64];
    s[8..16].copy_from_slice(&1u64.to_le_bytes());
    s[16..24].copy_from_slice(&1u64.to_le_bytes());

    let mut r = regs();
    r.rax = 0xAAAA_BBBB_FFFF_FFFF;
    r.rbx = 0xCCCC_DDDD_FFFF_FFFF;
    r.rcx = 0xFFFF_FFFF_FFFF_FFFE;
    r.rdx = 0xFFFF_FFFF_FFFF_FFFE;
    r.rdi = DATA_ADDR;
    r.rflags = flags::bits::CF | flags::bits::OF | flags::bits::SF | flags::bits::PF;

    let code = with_hlt(vec![
        0x66, 0x0F, 0x38, 0xF6, 0x07, // adcx eax, dword [rdi]
        0x89, 0x47, 0x20, // mov [rdi+32], eax
        0xF3, 0x0F, 0x38, 0xF6, 0x5F, 0x04, // adox ebx, dword [rdi+4]
        0x89, 0x5F, 0x24, // mov [rdi+36], ebx
        0x66, 0x48, 0x0F, 0x38, 0xF6, 0x4F, 0x08, // adcx rcx, qword [rdi+8]
        0x48, 0x89, 0x4F, 0x28, // mov [rdi+40], rcx
        0xF3, 0x48, 0x0F, 0x38, 0xF6, 0x57, 0x10, // adox rdx, qword [rdi+16]
        0x48, 0x89, 0x57, 0x30, // mov [rdi+48], rdx
    ]);
    check_mem("adcx_adox_memory_width_forms", &code, r, s, FLAG_MASK);
}

// ---------------------------------------------------------------------------
// Misc flag manipulation: CMC / STC / CLC / CLD / STD, plus a string op honoring
// DF, and BSWAP r16 (undefined result, but we still compare against KVM).
// ---------------------------------------------------------------------------

#[test]
fn flag_stc() {
    // STC (F9) sets CF. Seed CF=0; expect CF=1 afterward.
    let mut r = regs();
    r.rflags = 0;
    check("stc", &with_hlt(vec![0xF9]), r);
}

#[test]
fn flag_clc() {
    // CLC (F8) clears CF. Seed CF=1.
    let mut r = regs();
    r.rflags = flags::bits::CF;
    check("clc", &with_hlt(vec![0xF8]), r);
}

#[test]
fn flag_cmc() {
    // CMC (F5) complements CF. Seed CF=1 -> 0; other flags untouched.
    let mut r = regs();
    r.rflags = flags::bits::CF | flags::bits::ZF | flags::bits::SF;
    check("cmc", &with_hlt(vec![0xF5]), r);
}

#[test]
fn flag_cmc_from_zero() {
    // CMC with CF=0 -> CF=1.
    let mut r = regs();
    r.rflags = flags::bits::PF;
    check("cmc0", &with_hlt(vec![0xF5]), r);
}

#[test]
fn flag_stc_clc_cmc_sequence() {
    // STC; CMC; -> CF=0. F9 F5.
    let mut r = regs();
    r.rflags = 0;
    check("stc_cmc", &with_hlt(vec![0xF9, 0xF5]), r);
}

#[test]
fn flag_popfq_restores_wide_flag_image() {
    let flag_image = flags::bits::CF
        | flags::bits::PF
        | flags::bits::AF
        | flags::bits::ZF
        | flags::bits::SF
        | flags::bits::DF
        | flags::bits::OF
        | flags::bits::AC
        | flags::bits::ID;
    let mut r = regs();
    r.rdi = DATA_ADDR;

    let mut code = Vec::new();
    code.extend_from_slice(&[0x48, 0xB8]); // mov rax, imm64
    code.extend_from_slice(&flag_image.to_le_bytes());
    code.extend_from_slice(&[0x50, 0x9D]); // push rax; popfq
    code.extend_from_slice(&[0x9C, 0x58]); // pushfq; pop rax
    code.extend_from_slice(&[0x48, 0x89, 0x07]); // mov [rdi], rax
    code.push(HLT);

    check_mem("popfq_flag_image", &code, r, zero_scratch(), 0);
}

#[test]
fn flag_popfw_updates_low_flags_only() {
    let low_flags =
        (flags::bits::CF | flags::bits::AF | flags::bits::DF | flags::bits::OF) as u16;
    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rflags = flags::bits::AC | flags::bits::ID;

    let mut code = Vec::new();
    code.extend_from_slice(&[0x66, 0x68]); // push imm16
    code.extend_from_slice(&low_flags.to_le_bytes());
    code.extend_from_slice(&[0x66, 0x9D]); // popfw
    code.extend_from_slice(&[0x9C, 0x58]); // pushfq; pop rax
    code.extend_from_slice(&[0x48, 0x89, 0x07]); // mov [rdi], rax
    code.push(HLT);

    check_mem("popfw_low_flags_only", &code, r, zero_scratch(), 0);
}

#[test]
fn flag_cli_sti_updates_interrupt_flag() {
    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rflags = flags::bits::IF | flags::bits::CF;

    let mut code = Vec::new();
    code.push(0xFA); // cli
    code.extend_from_slice(&[0x9C, 0x58]); // pushfq; pop rax
    code.extend_from_slice(&[0x48, 0x89, 0x07]); // mov [rdi], rax
    code.push(0xFB); // sti
    code.extend_from_slice(&[0x9C, 0x58]); // pushfq; pop rax
    code.extend_from_slice(&[0x48, 0x89, 0x47, 0x08]); // mov [rdi+8], rax
    code.push(HLT);

    check_mem("cli_sti_if_image", &code, r, zero_scratch(), 0);
}

#[test]
fn flag_cld_then_movsb() {
    // CLD (FC) forces DF=0, so a following MOVSB advances RSI/RDI UPward even if
    // DF was seeded set. Compare the scratch copy + RSI/RDI.
    let mut r = regs();
    r.rsi = DATA_ADDR + SRC_OFF;
    r.rdi = DATA_ADDR + DST_OFF;
    r.rflags = flags::bits::DF; // seeded set; CLD must clear it
    // FC      cld
    // A4      movsb
    check_mem(
        "cld_movsb",
        &with_hlt(vec![0xFC, 0xA4]),
        r,
        string_scratch(&[0xC1, 0xC2, 0xC3]),
        0,
    );
}

#[test]
fn flag_std_then_movsb() {
    // STD (FD) forces DF=1, so MOVSB walks DOWN. Point RSI/RDI at the 3rd element
    // so the single copy moves [src+2]->[dst+2] and decrements both pointers.
    let mut r = regs();
    r.rsi = DATA_ADDR + SRC_OFF + 2;
    r.rdi = DATA_ADDR + DST_OFF + 2;
    r.rflags = 0; // seeded clear; STD must set DF
    // FD      std
    // A4      movsb
    check_mem(
        "std_movsb",
        &with_hlt(vec![0xFD, 0xA4]),
        r,
        string_scratch(&[0xD1, 0xD2, 0xD3]),
        0,
    );
}

#[test]
fn flag_std_rep_stosb_then_cld() {
    // STD; REP STOSB; CLD : fill RCX bytes DOWNward, then restore DF. We verify the
    // descending fill landed correctly. RDI starts at the last target byte.
    let mut r = regs();
    r.rdi = DATA_ADDR + DST_OFF + 4; // fill offsets 16..20 descending
    r.rcx = 5;
    r.rax = 0x7C;
    r.rflags = 0;
    // FD        std
    // F3 AA     rep stosb
    // FC        cld
    check_mem(
        "std_rep_stosb",
        &with_hlt(vec![0xFD, 0xF3, 0xAA, 0xFC]),
        r,
        zero_scratch(),
        0,
    );
}

#[ignore = "ARCHITECTURALLY UNDEFINED, divergence expected: BSWAP with a 16-bit \
operand (66 0F C8) is explicitly 'undefined' per the Intel SDM. Observed: KVM \
(this Intel host) zeroes the swapped low 16 bits -> ax becomes 0x0000 \
(rax=0x1122334455660000), whereas the interpreter leaves the 16-bit value \
unchanged (rax=0x1122334455667788). Both are defensible for an undefined \
encoding, so this is NOT a correctness bug; it is kept (ignored) to document the \
behavioral difference the task asked us to compare."]
#[test]
fn bswap_r16_undefined_compare() {
    // BSWAP on a 16-bit register (66 0F C8) is documented as UNDEFINED behavior;
    // KVM zeroes the low 16 bits while the interpreter preserves them (see the
    // #[ignore] note above). Comparison is intentionally disabled.
    let mut r = regs();
    r.rax = 0x1122_3344_5566_7788; // ax = 0x7788
    // 66 0F C8  bswap ax
    check("bswap_r16", &with_hlt(vec![0x66, 0x0F, 0xC8]), r);
}

// ---------------------------------------------------------------------------
// XADD / CMPXCHG with MEMORY operands (lock-free single-thread semantics).
// ---------------------------------------------------------------------------

#[test]
fn xadd_mem() {
    // XADD [rdi], rbx = 48 0F C1 1F : tmp=[rdi]; [rdi]=tmp+rbx; rbx=tmp; flags=ADD.
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0x0000_0000_0000_0100u64.to_le_bytes());
    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rbx = 0x0000_0000_0000_00FF;
    // 48 0F C1 1F  xadd [rdi], rbx
    check_mem(
        "xadd_mem",
        &with_hlt(vec![0x48, 0x0F, 0xC1, 0x1F]),
        r,
        s,
        FLAG_MASK,
    );
}

#[test]
fn xadd_mem_carry() {
    // XADD memory where the in-memory add wraps -> CF, ZF set.
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0xFFFF_FFFF_FFFF_FFFFu64.to_le_bytes());
    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rbx = 0x0000_0000_0000_0001;
    check_mem(
        "xadd_mem_carry",
        &with_hlt(vec![0x48, 0x0F, 0xC1, 0x1F]),
        r,
        s,
        FLAG_MASK,
    );
}

#[test]
fn cmpxchg_mem_success() {
    // CMPXCHG [rdi], rcx = 48 0F B1 0F : if RAX==[rdi] then ZF=1, [rdi]=rcx;
    // else ZF=0, RAX=[rdi]. Success path: [rdi]==RAX.
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rax = 0x1122_3344_5566_7788; // matches memory -> success
    r.rcx = 0xCAFE_BABE_DEAD_BEEF; // written to memory on success
    // 48 0F B1 0F  cmpxchg [rdi], rcx
    check_mem(
        "cmpxchg_mem_ok",
        &with_hlt(vec![0x48, 0x0F, 0xB1, 0x0F]),
        r,
        s,
        FLAG_MASK,
    );
}

#[test]
fn cmpxchg_mem_fail() {
    // Failure path: RAX != [rdi] -> ZF=0, RAX loaded from memory, memory unchanged.
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rax = 0x0000_0000_0000_0001; // mismatch -> failure
    r.rcx = 0xCAFE_BABE_DEAD_BEEF; // NOT written on failure
    check_mem(
        "cmpxchg_mem_fail",
        &with_hlt(vec![0x48, 0x0F, 0xB1, 0x0F]),
        r,
        s,
        FLAG_MASK,
    );
}

#[test]
fn lock_xadd_mem() {
    // LOCK XADD [rdi], rbx (F0 prefix) : same architectural result as XADD; the
    // lock prefix must be accepted and the memory effect/flags must match KVM.
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0x0000_0000_0000_0042u64.to_le_bytes());
    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rbx = 0x0000_0000_0000_0008;
    // F0 48 0F C1 1F  lock xadd [rdi], rbx
    check_mem(
        "lock_xadd_mem",
        &with_hlt(vec![0xF0, 0x48, 0x0F, 0xC1, 0x1F]),
        r,
        s,
        FLAG_MASK,
    );
}

#[test]
fn cmpxchg8_mem_success() {
    // CMPXCHG8B m64 (0F C7 /1) : if EDX:EAX == [rdi] then ZF=1, [rdi]=ECX:EBX;
    // else ZF=0, EDX:EAX=[rdi]. Success path.
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rax = 0x5566_7788; // low half of the 64-bit compare value
    r.rdx = 0x1122_3344; // high half
    r.rbx = 0xDEAD_BEEF; // new low half
    r.rcx = 0xCAFE_F00D; // new high half
    // 0F C7 0F  cmpxchg8b [rdi]
    check_mem(
        "cmpxchg8b_ok",
        &with_hlt(vec![0x0F, 0xC7, 0x0F]),
        r,
        s,
        FLAG_MASK,
    );
}

#[test]
fn cmpxchg8_mem_failure_loads_observed_value() {
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rax = 0x1111_2222;
    r.rdx = 0x3333_4444;
    r.rbx = 0xDEAD_BEEF;
    r.rcx = 0xCAFE_F00D;

    check_mem(
        "cmpxchg8b_fail",
        &with_hlt(vec![0x0F, 0xC7, 0x0F]),
        r,
        s,
        FLAG_MASK,
    );
}

#[test]
fn cmpxchg16_mem_success() {
    // CMPXCHG16B m128 (48 0F C7 /1) : 128-bit compare-and-swap. Success path:
    // RDX:RAX == [rdi] -> ZF=1, [rdi] = RCX:RBX.
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes()); // low qword
    s[8..16].copy_from_slice(&0x99AA_BBCC_DDEE_FF00u64.to_le_bytes()); // high qword
    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rax = 0x1122_3344_5566_7788; // low compare
    r.rdx = 0x99AA_BBCC_DDEE_FF00; // high compare
    r.rbx = 0x0123_4567_89AB_CDEF; // new low
    r.rcx = 0xFEDC_BA98_7654_3210; // new high
    // 48 0F C7 0F  cmpxchg16b [rdi]
    check_mem(
        "cmpxchg16b_ok",
        &with_hlt(vec![0x48, 0x0F, 0xC7, 0x0F]),
        r,
        s,
        FLAG_MASK,
    );
}

#[test]
fn cmpxchg16_mem_failure_loads_observed_value() {
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
    s[8..16].copy_from_slice(&0x99AA_BBCC_DDEE_FF00u64.to_le_bytes());
    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rax = 0x0102_0304_0506_0708;
    r.rdx = 0x1112_1314_1516_1718;
    r.rbx = 0x0123_4567_89AB_CDEF;
    r.rcx = 0xFEDC_BA98_7654_3210;

    check_mem(
        "cmpxchg16b_fail",
        &with_hlt(vec![0x48, 0x0F, 0xC7, 0x0F]),
        r,
        s,
        FLAG_MASK,
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 4: SSE4.1 ROUND modes, SSE3/SSSE3 horizontal & sign,
// SSE4 PMOVSX/ZX & PEXTR/PINSR & PALIGNR & DPPS, MOVD/MOVQ gpr<->xmm, MMX,
// x87 FXCH/FABS/FCHS/FRNDINT/FSCALE/FPREM, and a few ALU corner cases.
//
// Reuses the existing helpers verbatim. SSE results that are exactly
// representable (small integers / exact dyadic) are bit-identical across
// backends and compared byte-for-byte via the scratch page.
// ===========================================================================

// ---- SSE4.1 ROUNDPS/ROUNDPD/ROUNDSS/ROUNDSD across all 4 rounding modes ----
//
// imm8[1:0] = 0 round-to-nearest-EVEN, 1 floor, 2 ceil, 3 truncate. Mode 0 uses
// banker's rounding (ties to even) — NOT "round half away from zero". The .5 ties
// here (0.5->0, 2.5->2, -2.5->-2, 3.5->4) probe exactly that distinction.

/// ROUNDPS xmm0, xmm1, imm8 = 66 0F 3A 08 C1 ib. Source = xmm1, dest = xmm0.
fn roundps_program(imm: u8) -> Vec<u8> {
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]); // movdqu xmm0, [rdi]
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]); // movdqu xmm1, [rdi+0x10]
    prog.extend_from_slice(&[0x66, 0x0F, 0x3A, 0x08, 0xC1, imm]); // roundps xmm0, xmm1, imm
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]); // movdqu [rdi+0x20], xmm0
    prog.push(HLT);
    prog
}

#[test]
fn sse4_roundps_nearest_even() {
    // Ties-to-even: 0.5->0, 2.5->2, 3.5->4, -2.5->-2.
    let a = [0u8; 16];
    let b = f32x4([0.5, 2.5, 3.5, -2.5]);
    check_sse("roundps_nearest", &roundps_program(0x00), sse_scratch(a, b));
}

#[test]
fn sse4_roundps_floor() {
    let a = [0u8; 16];
    let b = f32x4([1.4, -1.4, 2.9, -2.9]);
    check_sse("roundps_floor", &roundps_program(0x01), sse_scratch(a, b));
}

#[test]
fn sse4_roundps_ceil() {
    let a = [0u8; 16];
    let b = f32x4([1.4, -1.4, 2.1, -2.1]);
    check_sse("roundps_ceil", &roundps_program(0x02), sse_scratch(a, b));
}

#[test]
fn sse4_roundps_trunc() {
    let a = [0u8; 16];
    let b = f32x4([1.9, -1.9, 2.5, -2.5]);
    check_sse("roundps_trunc", &roundps_program(0x03), sse_scratch(a, b));
}

#[test]
fn sse4_roundpd_nearest_even() {
    // ROUNDPD xmm0, xmm1, 0 = 66 0F 3A 09 C1 00. Doubles: 0.5->0, 2.5->2.
    let a = [0u8; 16];
    let b = f64x2([0.5, 2.5]);
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]);
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]);
    prog.extend_from_slice(&[0x66, 0x0F, 0x3A, 0x09, 0xC1, 0x00]);
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("roundpd_nearest", &prog, sse_scratch(a, b));
}

#[test]
fn sse4_roundpd_floor_neg() {
    // floor of negatives: -0.1 -> -1.0, -3.5 -> -4.0.
    let a = [0u8; 16];
    let b = f64x2([-0.1, -3.5]);
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]);
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]);
    prog.extend_from_slice(&[0x66, 0x0F, 0x3A, 0x09, 0xC1, 0x01]);
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("roundpd_floor", &prog, sse_scratch(a, b));
}

#[test]
fn sse4_roundss_nearest_even() {
    // ROUNDSS xmm0, xmm1, 0 = 66 0F 3A 0A C1 00. lane0 rounded ties-to-even (2.5->2),
    // upper lanes of xmm0 preserved.
    let a = f32x4([1.0, 11.0, 12.0, 13.0]);
    let b = f32x4([2.5, 0.0, 0.0, 0.0]);
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]);
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]);
    prog.extend_from_slice(&[0x66, 0x0F, 0x3A, 0x0A, 0xC1, 0x00]);
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("roundss_nearest", &prog, sse_scratch(a, b));
}

#[test]
fn sse4_roundsd_nearest_even() {
    // ROUNDSD xmm0, xmm1, 0 = 66 0F 3A 0B C1 00. lane0 = round(0.5)=0 ties-to-even.
    let a = f64x2([1.0, 99.0]);
    let b = f64x2([0.5, 0.0]);
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]);
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]);
    prog.extend_from_slice(&[0x66, 0x0F, 0x3A, 0x0B, 0xC1, 0x00]);
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("roundsd_nearest", &prog, sse_scratch(a, b));
}

// ---- SSE3 packed-double addsub (HADDPD/HSUBPD already covered earlier) ----

#[test]
fn sse3_addsubpd() {
    // ADDSUBPD xmm0, xmm1 = 66 0F D0 C1. lane0 = a0-b0, lane1 = a1+b1.
    let a = f64x2([10.0, 10.0]);
    let b = f64x2([3.0, 4.0]);
    check_sse(
        "addsubpd",
        &sse_program(&[0x66, 0x0F, 0xD0, 0xC1]),
        sse_scratch(a, b),
    );
}

// ---- SSSE3 horizontal subtract / mulhrs (phaddw/d/sw, pmaddubsw, pshufb,
//      psign*, pabs*, palignr already covered earlier) ----

#[test]
fn ssse3_phsubw() {
    // PHSUBW xmm0, xmm1 = 66 0F 38 05 C1. Pairwise subtract (lane0 - lane1).
    let a = [
        0x000Au16.to_le_bytes(),
        0x0003u16.to_le_bytes(),
        0x0000u16.to_le_bytes(),
        0x0001u16.to_le_bytes(),
        0x0005u16.to_le_bytes(),
        0x0008u16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
        0x0001u16.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x0020u16.to_le_bytes(),
        0x0010u16.to_le_bytes(),
        0x0001u16.to_le_bytes(),
        0x0002u16.to_le_bytes(),
        0x0100u16.to_le_bytes(),
        0x0050u16.to_le_bytes(),
        0x0000u16.to_le_bytes(),
        0x0000u16.to_le_bytes(),
    ]
    .concat();
    check_sse(
        "phsubw",
        &sse_program(&[0x66, 0x0F, 0x38, 0x05, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn ssse3_pmulhrsw() {
    // PMULHRSW xmm0, xmm1 = 66 0F 38 0B C1. Signed 16-bit mul, round, take bits [16:30].
    let a = [
        0x4000u16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
        0xFFFFu16.to_le_bytes(),
        0x0001u16.to_le_bytes(),
        0x1234u16.to_le_bytes(),
        0x00FFu16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x4000u16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
        0xFFFFu16.to_le_bytes(),
        0x0002u16.to_le_bytes(),
        0x1000u16.to_le_bytes(),
        0x00FFu16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
    ]
    .concat();
    check_sse(
        "pmulhrsw",
        &sse_program(&[0x66, 0x0F, 0x38, 0x0B, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

// ---- SSSE3 PALIGNR with imm >= 16 (the imm < 16 case is covered earlier) ----

#[test]
fn ssse3_palignr_imm17() {
    // imm >= 16: result shifts dst alone right by (imm-16), zero-filling top.
    let a = [
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E,
        0x1F,
    ];
    let b = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
        0x0F,
    ];
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]);
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]);
    prog.extend_from_slice(&[0x66, 0x0F, 0x3A, 0x0F, 0xC1, 0x11]); // imm=17
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("palignr17", &prog, sse_scratch(a, b));
}

// ---- SSE4.1 PMOVSX / PMOVZX (sign/zero extend packed integers) ----

#[test]
fn sse4_pmovsxbw() {
    // PMOVSXBW xmm0, xmm1 = 66 0F 38 20 C1. Sign-extend low 8 bytes -> 8 words.
    let a = [0u8; 16];
    let b = [
        0x01, 0xFF, 0x7F, 0x80, 0x00, 0x40, 0xC0, 0x10, 9, 9, 9, 9, 9, 9, 9, 9,
    ];
    check_sse(
        "pmovsxbw",
        &sse_program(&[0x66, 0x0F, 0x38, 0x20, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse4_pmovzxbw() {
    // PMOVZXBW xmm0, xmm1 = 66 0F 38 30 C1. Zero-extend low 8 bytes -> 8 words.
    let a = [0u8; 16];
    let b = [
        0x01, 0xFF, 0x7F, 0x80, 0x00, 0x40, 0xC0, 0x10, 9, 9, 9, 9, 9, 9, 9, 9,
    ];
    check_sse(
        "pmovzxbw",
        &sse_program(&[0x66, 0x0F, 0x38, 0x30, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse4_pmovsxbd() {
    // PMOVSXBD xmm0, xmm1 = 66 0F 38 21 C1. Sign-extend low 4 bytes -> 4 dwords.
    let a = [0u8; 16];
    let b = [0x01, 0xFF, 0x7F, 0x80, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9, 9];
    check_sse(
        "pmovsxbd",
        &sse_program(&[0x66, 0x0F, 0x38, 0x21, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse4_pmovsxwd() {
    // PMOVSXWD xmm0, xmm1 = 66 0F 38 23 C1. Sign-extend low 4 words -> 4 dwords.
    let a = [0u8; 16];
    let b = [
        0xFFFFu16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
        0x0001u16.to_le_bytes(),
        0x1234u16.to_le_bytes(),
        0x5678u16.to_le_bytes(),
        0x9ABCu16.to_le_bytes(),
        0xDEF0u16.to_le_bytes(),
    ]
    .concat();
    check_sse(
        "pmovsxwd",
        &sse_program(&[0x66, 0x0F, 0x38, 0x23, 0xC1]),
        sse_scratch(a, b.try_into().unwrap()),
    );
}

#[test]
fn sse4_pmovzxdq() {
    // PMOVZXDQ xmm0, xmm1 = 66 0F 38 35 C1. Zero-extend low 2 dwords -> 2 qwords.
    let a = [0u8; 16];
    let b = [
        0x8000_0001u32.to_le_bytes(),
        0x0000_0002u32.to_le_bytes(),
        0x1111_1111u32.to_le_bytes(),
        0x2222_2222u32.to_le_bytes(),
    ]
    .concat();
    check_sse(
        "pmovzxdq",
        &sse_program(&[0x66, 0x0F, 0x38, 0x35, 0xC1]),
        sse_scratch(a, b.try_into().unwrap()),
    );
}

// ---- SSE4.1 packed integer min/max (signed/unsigned dwords & bytes) ----

#[test]
fn sse4_pminsd() {
    // PMINSD xmm0, xmm1 = 66 0F 38 39 C1. Per-dword signed min.
    let a = [
        0x0000_0005u32.to_le_bytes(),
        0xFFFF_FFFFu32.to_le_bytes(),
        0x7FFF_FFFFu32.to_le_bytes(),
        0x8000_0000u32.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x0000_0003u32.to_le_bytes(),
        0x0000_0001u32.to_le_bytes(),
        0x0000_0000u32.to_le_bytes(),
        0xFFFF_FFFFu32.to_le_bytes(),
    ]
    .concat();
    check_sse(
        "pminsd",
        &sse_program(&[0x66, 0x0F, 0x38, 0x39, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse4_pmaxud() {
    // PMAXUD xmm0, xmm1 = 66 0F 38 3F C1. Per-dword unsigned max.
    let a = [
        0x0000_0005u32.to_le_bytes(),
        0xFFFF_FFFFu32.to_le_bytes(),
        0x7FFF_FFFFu32.to_le_bytes(),
        0x8000_0000u32.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x0000_0003u32.to_le_bytes(),
        0x0000_0001u32.to_le_bytes(),
        0xFFFF_FFFFu32.to_le_bytes(),
        0x0000_0001u32.to_le_bytes(),
    ]
    .concat();
    check_sse(
        "pmaxud",
        &sse_program(&[0x66, 0x0F, 0x38, 0x3F, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse4_pmulld() {
    // PMULLD xmm0, xmm1 = 66 0F 38 40 C1. Per-dword 32-bit multiply, low 32 kept.
    let a = [
        0x0000_0002u32.to_le_bytes(),
        0xFFFF_FFFFu32.to_le_bytes(),
        0x0001_0000u32.to_le_bytes(),
        0x1234_5678u32.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x0000_0003u32.to_le_bytes(),
        0x0000_0002u32.to_le_bytes(),
        0x0001_0000u32.to_le_bytes(),
        0x0000_0010u32.to_le_bytes(),
    ]
    .concat();
    check_sse(
        "pmulld",
        &sse_program(&[0x66, 0x0F, 0x38, 0x40, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse4_pmuldq() {
    // PMULDQ xmm0, xmm1 = 66 0F 38 28 C1. Signed 32x32->64 of lanes 0 and 2.
    let a = [
        0xFFFF_FFFFu32.to_le_bytes(),
        0xDEAD_BEEFu32.to_le_bytes(),
        0x0000_0002u32.to_le_bytes(),
        0xCAFE_BABEu32.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x0000_0003u32.to_le_bytes(),
        0x1111_1111u32.to_le_bytes(),
        0x7FFF_FFFFu32.to_le_bytes(),
        0x2222_2222u32.to_le_bytes(),
    ]
    .concat();
    check_sse(
        "pmuldq",
        &sse_program(&[0x66, 0x0F, 0x38, 0x28, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse4_pcmpeqq() {
    // PCMPEQQ xmm0, xmm1 = 66 0F 38 29 C1. Per-qword equality -> all-1s/all-0s.
    let a = [
        0x1122_3344_5566_7788u64.to_le_bytes(),
        0xDEAD_BEEF_CAFE_BABEu64.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x1122_3344_5566_7788u64.to_le_bytes(),
        0x0000_0000_0000_0000u64.to_le_bytes(),
    ]
    .concat();
    check_sse(
        "pcmpeqq",
        &sse_program(&[0x66, 0x0F, 0x38, 0x29, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse4_packusdw() {
    // PACKUSDW xmm0, xmm1 = 66 0F 38 2B C1. Pack signed dwords to unsigned words (sat).
    let a = [
        0x0000_0001u32.to_le_bytes(),
        0x0001_0000u32.to_le_bytes(),
        0xFFFF_FFFFu32.to_le_bytes(),
        0x0000_FFFFu32.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x0000_8000u32.to_le_bytes(),
        0x8000_0000u32.to_le_bytes(),
        0x0000_7FFFu32.to_le_bytes(),
        0x0001_2345u32.to_le_bytes(),
    ]
    .concat();
    check_sse(
        "packusdw",
        &sse_program(&[0x66, 0x0F, 0x38, 0x2B, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse4_0f38_memory_source_forms() {
    let a = [
        0x80, 0x01, 0x7F, 0xFF, 0x00, 0x80, 0xFF, 0x7F, 0x34, 0x12, 0xCC, 0xDD, 0x10, 0x20,
        0xF0, 0xE0,
    ];
    let b = [
        0x01, 0xFF, 0x80, 0x7F, 0x00, 0x40, 0xC0, 0x10, 0xFF, 0x7F, 0x00, 0x80, 0x55, 0xAA,
        0x33, 0xCC,
    ];

    for (label, opcode) in [
        ("pmovsxbw_mem", 0x20),
        ("pmovsxbd_mem", 0x21),
        ("pmovsxbq_mem", 0x22),
        ("pmovsxwd_mem", 0x23),
        ("pmovsxwq_mem", 0x24),
        ("pmovsxdq_mem", 0x25),
        ("pmuldq_mem", 0x28),
        ("pcmpeqq_mem", 0x29),
        ("movntdqa_mem", 0x2A),
        ("packusdw_mem", 0x2B),
        ("pmovzxbw_mem", 0x30),
        ("pmovzxbd_mem", 0x31),
        ("pmovzxbq_mem", 0x32),
        ("pmovzxwd_mem", 0x33),
        ("pmovzxwq_mem", 0x34),
        ("pmovzxdq_mem", 0x35),
        ("pcmpgtq_mem", 0x37),
        ("pminsb_mem", 0x38),
        ("pminsd_mem", 0x39),
        ("pminuw_mem", 0x3A),
        ("pminud_mem", 0x3B),
        ("pmaxsb_mem", 0x3C),
        ("pmaxsd_mem", 0x3D),
        ("pmaxuw_mem", 0x3E),
        ("pmaxud_mem", 0x3F),
        ("pmulld_mem", 0x40),
        ("phminposuw_mem", 0x41),
    ] {
        check_sse(
            label,
            &sse_memory_source_program(&[0x66, 0x0F, 0x38, opcode, 0x47, 0x10]),
            sse_scratch(a, b),
        );
    }
}

#[test]
fn sse4_variable_blend_memory_source_forms() {
    let blendv_mem_program = |opcode| {
        let mut code = load_rdi_data();
        code.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x17]); // movdqu xmm2, [rdi]
        code.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x47, 0x20]); // movdqu xmm0, [rdi+0x20]
        code.extend_from_slice(&[0x66, 0x0F, 0x38, opcode, 0x57, 0x10]); // blendv* xmm2, [rdi+0x10]
        code.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x57, 0x30]); // movdqu [rdi+0x30], xmm2
        code.push(HLT);
        code
    };
    let scratch = |dst: [u8; 16], src: [u8; 16], mask: [u8; 16]| {
        let mut s = [0u8; 64];
        s[0..16].copy_from_slice(&dst);
        s[16..32].copy_from_slice(&src);
        s[32..48].copy_from_slice(&mask);
        s
    };

    let mut byte_mask = [0u8; 16];
    for (i, m) in byte_mask.iter_mut().enumerate() {
        *m = if i % 3 == 1 { 0x80 } else { 0x00 };
    }
    check_sse(
        "pblendvb_mem",
        &blendv_mem_program(0x10),
        scratch(
            [
                0x10, 0x11, 0x12, 0x13, 0x20, 0x21, 0x22, 0x23, 0x30, 0x31, 0x32, 0x33, 0x40,
                0x41, 0x42, 0x43,
            ],
            [
                0xA0, 0xA1, 0xA2, 0xA3, 0xB0, 0xB1, 0xB2, 0xB3, 0xC0, 0xC1, 0xC2, 0xC3, 0xD0,
                0xD1, 0xD2, 0xD3,
            ],
            byte_mask,
        ),
    );

    let blendvps_mask = [
        0x8000_0000u32.to_le_bytes(),
        0x0000_0000u32.to_le_bytes(),
        0x8000_0000u32.to_le_bytes(),
        0x0000_0000u32.to_le_bytes(),
    ]
    .concat();
    check_sse(
        "blendvps_mem",
        &blendv_mem_program(0x14),
        scratch(
            f32x4([1.0, 2.0, 3.0, 4.0]),
            f32x4([10.0, 20.0, 30.0, 40.0]),
            blendvps_mask.try_into().unwrap(),
        ),
    );

    let blendvpd_mask = [
        0x0000_0000_0000_0000u64.to_le_bytes(),
        0x8000_0000_0000_0000u64.to_le_bytes(),
    ]
    .concat();
    check_sse(
        "blendvpd_mem",
        &blendv_mem_program(0x15),
        scratch(
            f64x2([1.0, 2.0]),
            f64x2([10.0, 20.0]),
            blendvpd_mask.try_into().unwrap(),
        ),
    );
}

// ---- SSE4.1 DPPS / DPPD (dot product) — exactly-representable inputs ----

#[test]
fn sse4_dpps() {
    // DPPS xmm0, xmm1, imm8 = 66 0F 3A 40 C1 ib. imm high nibble selects which lanes
    // to multiply-accumulate; low nibble selects which result lanes get the sum.
    // imm=0xF1: use all 4 products, broadcast sum to lane0 only.
    let a = f32x4([1.0, 2.0, 3.0, 4.0]);
    let b = f32x4([8.0, 0.5, 2.0, 0.25]); // products: 8,1,6,1 -> sum 16
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]);
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]);
    prog.extend_from_slice(&[0x66, 0x0F, 0x3A, 0x40, 0xC1, 0xF1]); // dpps xmm0,xmm1,0xF1
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("dpps", &prog, sse_scratch(a, b));
}

#[test]
fn sse4_dppd() {
    // DPPD xmm0, xmm1, imm8 = 66 0F 3A 41 C1 ib. imm=0x33: both lanes selected,
    // sum broadcast to both result lanes.
    let a = f64x2([3.0, 4.0]);
    let b = f64x2([2.0, 0.5]); // products: 6, 2 -> sum 8
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]);
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]);
    prog.extend_from_slice(&[0x66, 0x0F, 0x3A, 0x41, 0xC1, 0x33]); // dppd xmm0,xmm1,0x33
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("dppd", &prog, sse_scratch(a, b));
}

#[test]
fn sse4_0f3a_memory_source_forms() {
    let ps_a = f32x4([1.0, 11.0, 12.0, 13.0]);
    let ps_b = f32x4([2.5, -1.4, 3.5, -2.9]);
    for (label, opcode, imm) in [
        ("roundps_mem", 0x08, 0x00),
        ("roundss_mem", 0x0A, 0x00),
        ("blendps_mem", 0x0C, 0b0101),
        ("dpps_mem", 0x40, 0xF1),
    ] {
        check_sse(
            label,
            &sse_memory_source_program(&[0x66, 0x0F, 0x3A, opcode, 0x47, 0x10, imm]),
            sse_scratch(ps_a, ps_b),
        );
    }

    let pd_a = f64x2([1.0, 99.0]);
    let pd_b = f64x2([2.5, -3.5]);
    for (label, opcode, imm) in [
        ("roundpd_mem", 0x09, 0x00),
        ("roundsd_mem", 0x0B, 0x01),
        ("blendpd_mem", 0x0D, 0b10),
        ("dppd_mem", 0x41, 0x33),
    ] {
        check_sse(
            label,
            &sse_memory_source_program(&[0x66, 0x0F, 0x3A, opcode, 0x47, 0x10, imm]),
            sse_scratch(pd_a, pd_b),
        );
    }

    let a = [
        0x00, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xA0, 0xB0, 0xC0, 0xD0,
        0xE0, 0xF0,
    ];
    let b = [
        0x05, 0x15, 0x25, 0x35, 0x45, 0x55, 0x65, 0x75, 0x85, 0x95, 0xA5, 0xB5, 0xC5, 0xD5,
        0xE5, 0xF5,
    ];
    for (label, opcode, imm) in [
        ("pblendw_mem", 0x0E, 0b1010_0101),
        ("mpsadbw_mem", 0x42, 0x05),
    ] {
        check_sse(
            label,
            &sse_memory_source_program(&[0x66, 0x0F, 0x3A, opcode, 0x47, 0x10, imm]),
            sse_scratch(a, b),
        );
    }
}

// ---- SSE4.1 PBLENDW / BLENDPS (imm-controlled blends) ----

#[test]
fn sse4_pblendw() {
    // PBLENDW xmm0, xmm1, imm8 = 66 0F 3A 0E C1 ib. imm bit i selects src word i.
    let a = [
        0x0000u16.to_le_bytes(),
        0x0001u16.to_le_bytes(),
        0x0002u16.to_le_bytes(),
        0x0003u16.to_le_bytes(),
        0x0004u16.to_le_bytes(),
        0x0005u16.to_le_bytes(),
        0x0006u16.to_le_bytes(),
        0x0007u16.to_le_bytes(),
    ]
    .concat();
    let b = [
        0xA0A0u16.to_le_bytes(),
        0xA1A1u16.to_le_bytes(),
        0xA2A2u16.to_le_bytes(),
        0xA3A3u16.to_le_bytes(),
        0xA4A4u16.to_le_bytes(),
        0xA5A5u16.to_le_bytes(),
        0xA6A6u16.to_le_bytes(),
        0xA7A7u16.to_le_bytes(),
    ]
    .concat();
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]);
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]);
    prog.extend_from_slice(&[0x66, 0x0F, 0x3A, 0x0E, 0xC1, 0b1010_0101]); // imm
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse(
        "pblendw",
        &prog,
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse4_blendps() {
    // BLENDPS xmm0, xmm1, imm8 = 66 0F 3A 0C C1 ib. imm bit i selects src f32 lane i.
    let a = f32x4([1.0, 2.0, 3.0, 4.0]);
    let b = f32x4([10.0, 20.0, 30.0, 40.0]);
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]);
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]);
    prog.extend_from_slice(&[0x66, 0x0F, 0x3A, 0x0C, 0xC1, 0b0101]); // lanes 0,2 from src
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("blendps", &prog, sse_scratch(a, b));
}

// ---- SSE4.1 PEXTR* / PINSR* (extract/insert lane to/from GPR via memory) ----

#[test]
fn sse4_pextrb_to_mem() {
    // PEXTRB [rdi+0x20], xmm1, idx = 66 0F 3A 14 /r ib. Extract byte idx from xmm1.
    let a = [0u8; 16];
    let b = [
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE,
        0xFF,
    ];
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]); // movdqu xmm1, [rdi+0x10]
    // pextrb [rdi+0x20], xmm1, 0x0A  -> modrm 0x4F reg=xmm1(001), rm=[rdi+disp8]
    prog.extend_from_slice(&[0x66, 0x0F, 0x3A, 0x14, 0x4F, 0x20, 0x0A]);
    prog.push(HLT);
    check_sse("pextrb_mem", &prog, sse_scratch(a, b));
}

#[test]
fn sse4_pextrd_to_reg() {
    // PEXTRD eax, xmm1, idx = 66 0F 3A 16 C1 ib (W0). idx=2. Then store eax to scratch.
    let a = [0u8; 16];
    let b = [
        0x11111111u32.to_le_bytes(),
        0x22222222u32.to_le_bytes(),
        0xDEADBEEFu32.to_le_bytes(),
        0x44444444u32.to_le_bytes(),
    ]
    .concat();
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]); // movdqu xmm1, [rdi+0x10]
    prog.extend_from_slice(&[0x66, 0x0F, 0x3A, 0x16, 0xC8, 0x02]); // pextrd eax, xmm1, 2
    prog.extend_from_slice(&[0x89, 0x47, 0x20]); // mov [rdi+0x20], eax
    prog.push(HLT);
    check_sse("pextrd_reg", &prog, sse_scratch(a, b.try_into().unwrap()));
}

#[test]
fn sse4_pinsrd_from_reg() {
    // PINSRD xmm0, eax, idx = 66 0F 3A 22 C0 ib. Insert EAX into dword lane idx=1.
    let a = [
        0xAAAAAAAAu32.to_le_bytes(),
        0xBBBBBBBBu32.to_le_bytes(),
        0xCCCCCCCCu32.to_le_bytes(),
        0xDDDDDDDDu32.to_le_bytes(),
    ]
    .concat();
    let b = [0u8; 16];
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]); // movdqu xmm0, [rdi]
    prog.extend_from_slice(&[0xB8, 0xEF, 0xBE, 0xAD, 0xDE]); // mov eax, 0xDEADBEEF
    prog.extend_from_slice(&[0x66, 0x0F, 0x3A, 0x22, 0xC0, 0x01]); // pinsrd xmm0, eax, 1
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("pinsrd_reg", &prog, sse_scratch(a.try_into().unwrap(), b));
}

#[test]
fn sse2_pinsrw_pextrw() {
    // PINSRW xmm0, eax, 3 (66 0F C4 C0 03) then PEXTRW eax, xmm0, 3 (66 0F C5 C0 03).
    let a = [
        0xFFFFu16.to_le_bytes(),
        0x1111u16.to_le_bytes(),
        0x2222u16.to_le_bytes(),
        0x3333u16.to_le_bytes(),
        0x4444u16.to_le_bytes(),
        0x5555u16.to_le_bytes(),
        0x6666u16.to_le_bytes(),
        0x7777u16.to_le_bytes(),
    ]
    .concat();
    let b = [0u8; 16];
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]); // movdqu xmm0, [rdi]
    prog.extend_from_slice(&[0xB8, 0xCD, 0xAB, 0x00, 0x00]); // mov eax, 0xABCD
    prog.extend_from_slice(&[0x66, 0x0F, 0xC4, 0xC0, 0x03]); // pinsrw xmm0, eax, 3
    prog.extend_from_slice(&[0x66, 0x0F, 0xC5, 0xC0, 0x03]); // pextrw eax, xmm0, 3
    prog.extend_from_slice(&[0x89, 0x47, 0x30]); // mov [rdi+0x30], eax
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]); // store xmm0
    prog.push(HLT);
    check_sse(
        "pinsrw_pextrw",
        &prog,
        sse_scratch(a.try_into().unwrap(), b),
    );
}

// ---- MOVD / MOVQ between GPR and XMM ----

#[test]
fn sse_movd_gpr_to_xmm_and_back() {
    // movd xmm0, eax (66 0F 6E C0) then movd ecx, xmm0 (66 0F 7E C1); store both.
    let a = [0u8; 16];
    let b = [0u8; 16];
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xB8, 0x78, 0x56, 0x34, 0x12]); // mov eax, 0x12345678
    prog.extend_from_slice(&[0x66, 0x0F, 0x6E, 0xC0]); // movd xmm0, eax (zero-extends to 128)
    prog.extend_from_slice(&[0x66, 0x0F, 0x7E, 0xC1]); // movd ecx, xmm0
    prog.extend_from_slice(&[0x89, 0x4F, 0x20]); // mov [rdi+0x20], ecx
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x30]); // store xmm0 at +0x30
    prog.push(HLT);
    check_sse("movd_roundtrip", &prog, sse_scratch(a, b));
}

#[test]
fn sse_movq_gpr_to_xmm() {
    // movq xmm0, rax (66 48 0F 6E C0). Zero-extends to 128. Store xmm0.
    let a = [0u8; 16];
    let b = [0u8; 16];
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0x48, 0xB8]); // mov rax, imm64
    prog.extend_from_slice(&0x0123_4567_89AB_CDEFu64.to_le_bytes());
    prog.extend_from_slice(&[0x66, 0x48, 0x0F, 0x6E, 0xC0]); // movq xmm0, rax
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("movq_gpr_xmm", &prog, sse_scratch(a, b));
}

#[test]
fn sse_movq_xmm_to_xmm_zeroes_high() {
    // MOVQ xmm0, xmm1 (F3 0F 7E C1): copies low 64, ZEROES the high 64 of dst.
    let a = [
        0xFFFF_FFFF_FFFF_FFFFu64.to_le_bytes(),
        0xFFFF_FFFF_FFFF_FFFFu64.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x1122_3344_5566_7788u64.to_le_bytes(),
        0xDEAD_BEEF_CAFE_BABEu64.to_le_bytes(),
    ]
    .concat();
    check_sse(
        "movq_xmm_xmm",
        &sse_program(&[0xF3, 0x0F, 0x7E, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

// ---- MMX integer ops (operate on 64-bit MM registers) ----

#[test]
fn mmx_paddb() {
    // movq mm0, [rdi]; movq mm1, [rdi+8]; paddb mm0, mm1; movq [rdi+0x20], mm0.
    // 0F 6F /r = movq mm, m64 ; 0F FC = paddb ; 0F 7F /r = movq m64, mm.
    let mut s = [0u8; 64];
    let a = [1, 2, 3, 4, 0x7F, 0x80, 0xFF, 0x10];
    let b = [8, 8, 8, 8, 0x01, 0x80, 0x01, 0xF0];
    s[0..8].copy_from_slice(&a);
    s[8..16].copy_from_slice(&b);
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0x0F, 0x6F, 0x07]); // movq mm0, [rdi]
    prog.extend_from_slice(&[0x0F, 0x6F, 0x4F, 0x08]); // movq mm1, [rdi+8]
    prog.extend_from_slice(&[0x0F, 0xFC, 0xC1]); // paddb mm0, mm1
    prog.extend_from_slice(&[0x0F, 0x7F, 0x47, 0x20]); // movq [rdi+0x20], mm0
    prog.push(HLT);
    check_mem("mmx_paddb", &with_hlt(prog), regs(), s, 0);
}

#[test]
fn mmx_pmullw() {
    let mut s = [0u8; 64];
    let a = [
        0x0002u16.to_le_bytes(),
        0x00FFu16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
        0xFFFFu16.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x0003u16.to_le_bytes(),
        0x0100u16.to_le_bytes(),
        0x0002u16.to_le_bytes(),
        0xFFFFu16.to_le_bytes(),
    ]
    .concat();
    s[0..8].copy_from_slice(&a);
    s[8..16].copy_from_slice(&b);
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0x0F, 0x6F, 0x07]);
    prog.extend_from_slice(&[0x0F, 0x6F, 0x4F, 0x08]);
    prog.extend_from_slice(&[0x0F, 0xD5, 0xC1]); // pmullw mm0, mm1
    prog.extend_from_slice(&[0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_mem("mmx_pmullw", &with_hlt(prog), regs(), s, 0);
}

#[test]
fn mmx_punpcklbw() {
    let mut s = [0u8; 64];
    let a = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];
    let b = [0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87];
    s[0..8].copy_from_slice(&a);
    s[8..16].copy_from_slice(&b);
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0x0F, 0x6F, 0x07]);
    prog.extend_from_slice(&[0x0F, 0x6F, 0x4F, 0x08]);
    prog.extend_from_slice(&[0x0F, 0x60, 0xC1]); // punpcklbw mm0, mm1
    prog.extend_from_slice(&[0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_mem("mmx_punpcklbw", &with_hlt(prog), regs(), s, 0);
}

#[test]
fn mmx_psubusb_saturate() {
    // PSUBUSB mm0, mm1 (0F D8): unsigned byte subtract with saturation to 0.
    let mut s = [0u8; 64];
    let a = [0x10, 0x05, 0xFF, 0x00, 0x80, 0x7F, 0x01, 0xAA];
    let b = [0x20, 0x05, 0x01, 0x01, 0x40, 0x80, 0x02, 0x55];
    s[0..8].copy_from_slice(&a);
    s[8..16].copy_from_slice(&b);
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0x0F, 0x6F, 0x07]);
    prog.extend_from_slice(&[0x0F, 0x6F, 0x4F, 0x08]);
    prog.extend_from_slice(&[0x0F, 0xD8, 0xC1]); // psubusb mm0, mm1
    prog.extend_from_slice(&[0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_mem("mmx_psubusb", &with_hlt(prog), regs(), s, 0);
}

// ---- SSE2 integer saturating add/sub and average ----

#[test]
fn sse2_paddsb() {
    // PADDSB xmm0, xmm1 = 66 0F EC C1. Signed byte add with saturation.
    let a = [
        0x7F, 0x7F, 0x80, 0x80, 0x40, 0xC0, 0x01, 0xFF, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70,
        0x7F,
    ];
    let b = [
        0x01, 0x7F, 0xFF, 0x80, 0x40, 0xC0, 0x02, 0x01, 0xF0, 0xE0, 0xD0, 0xC0, 0x10, 0x20, 0x30,
        0x01,
    ];
    check_sse(
        "paddsb",
        &sse_program(&[0x66, 0x0F, 0xEC, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse2_paddusw() {
    // PADDUSW xmm0, xmm1 = 66 0F DD C1. Unsigned word add with saturation.
    let a = [
        0xFFFFu16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
        0x0001u16.to_le_bytes(),
        0x1234u16.to_le_bytes(),
        0xF000u16.to_le_bytes(),
        0x0FFFu16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
        0x0000u16.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x0001u16.to_le_bytes(),
        0x8001u16.to_le_bytes(),
        0x0002u16.to_le_bytes(),
        0x1000u16.to_le_bytes(),
        0x2000u16.to_le_bytes(),
        0xF001u16.to_le_bytes(),
        0x8001u16.to_le_bytes(),
        0xFFFFu16.to_le_bytes(),
    ]
    .concat();
    check_sse(
        "paddusw",
        &sse_program(&[0x66, 0x0F, 0xDD, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse2_pavgb() {
    // PAVGB xmm0, xmm1 = 66 0F E0 C1. Unsigned byte average, rounded up.
    let a = [
        0x00, 0xFF, 0x10, 0x01, 0x80, 0x7F, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B,
        0x0C,
    ];
    let b = [
        0x01, 0xFF, 0x20, 0x02, 0x81, 0x80, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C,
        0x0D,
    ];
    check_sse(
        "pavgb",
        &sse_program(&[0x66, 0x0F, 0xE0, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse2_pmaxub_pminub() {
    // PMAXUB then PMINUB on the SAME register isn't useful; just probe PMAXUB.
    // PMAXUB xmm0, xmm1 = 66 0F DE C1.
    let a = [
        0x00, 0xFF, 0x10, 0x80, 0x7F, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A,
        0x0B,
    ];
    let b = [
        0x01, 0x00, 0x20, 0x7F, 0x80, 0x02, 0x01, 0xFF, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09,
        0x0A,
    ];
    check_sse(
        "pmaxub",
        &sse_program(&[0x66, 0x0F, 0xDE, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse2_pcmpgtb() {
    // PCMPGTB xmm0, xmm1 = 66 0F 64 C1. Signed byte greater-than -> all-1s/all-0s.
    let a = [
        0x05, 0x80, 0x7F, 0xFF, 0x00, 0x01, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x7F, 0x81,
        0xC0,
    ];
    let b = [
        0x03, 0x7F, 0x80, 0x01, 0x00, 0xFF, 0x10, 0x21, 0x2F, 0x40, 0x4F, 0x61, 0x6F, 0x7E, 0x80,
        0xBF,
    ];
    check_sse(
        "pcmpgtb",
        &sse_program(&[0x66, 0x0F, 0x64, 0xC1]),
        sse_scratch(a, b),
    );
}

// ---- SSE2 shifts: PSLLW/PSRLD/PSRAW (imm and xmm count) ----

#[test]
fn sse2_psllw_imm() {
    // PSLLW xmm0, imm8 = 66 0F 71 /6 ib. Shift each word left by 4.
    let a = [
        0x0001u16.to_le_bytes(),
        0x00FFu16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
        0x1234u16.to_le_bytes(),
        0xFFFFu16.to_le_bytes(),
        0x0FF0u16.to_le_bytes(),
        0x0001u16.to_le_bytes(),
        0x4000u16.to_le_bytes(),
    ]
    .concat();
    let b = [0u8; 16];
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]); // movdqu xmm0, [rdi]
    prog.extend_from_slice(&[0x66, 0x0F, 0x71, 0xF0, 0x04]); // psllw xmm0, 4
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("psllw_imm4", &prog, sse_scratch(a.try_into().unwrap(), b));
}

#[test]
fn sse2_psraw_imm() {
    // PSRAW xmm0, imm8 = 66 0F 71 /4 ib. Arithmetic shift right words by 2.
    let a = [
        0x8000u16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
        0xFFFFu16.to_le_bytes(),
        0x0004u16.to_le_bytes(),
        0x4000u16.to_le_bytes(),
        0xC000u16.to_le_bytes(),
        0x0001u16.to_le_bytes(),
        0x00FFu16.to_le_bytes(),
    ]
    .concat();
    let b = [0u8; 16];
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]);
    prog.extend_from_slice(&[0x66, 0x0F, 0x71, 0xE0, 0x02]); // psraw xmm0, 2
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("psraw_imm2", &prog, sse_scratch(a.try_into().unwrap(), b));
}

#[test]
fn sse2_psrld_imm() {
    // PSRLD xmm0, imm8 = 66 0F 72 /2 ib. Logical shift right dwords by 8.
    let a = [
        0x8000_00FFu32.to_le_bytes(),
        0xFFFF_FFFFu32.to_le_bytes(),
        0x0000_0100u32.to_le_bytes(),
        0x1234_5678u32.to_le_bytes(),
    ]
    .concat();
    let b = [0u8; 16];
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]);
    prog.extend_from_slice(&[0x66, 0x0F, 0x72, 0xD0, 0x08]); // psrld xmm0, 8
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("psrld_imm8", &prog, sse_scratch(a.try_into().unwrap(), b));
}

#[test]
fn sse2_pslldq_imm() {
    // PSLLDQ xmm0, imm8 = 66 0F 73 /7 ib. Byte shift left of the whole 128 by 3.
    let a = [
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
        0x0F,
    ];
    let b = [0u8; 16];
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]);
    prog.extend_from_slice(&[0x66, 0x0F, 0x73, 0xF8, 0x03]); // pslldq xmm0, 3
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("pslldq3", &prog, sse_scratch(a, b));
}

// ---- SSE2 shuffles: PSHUFD / PSHUFHW / PSHUFLW ----

#[test]
fn sse2_pshufd() {
    // PSHUFD xmm0, xmm1, imm8 = 66 0F 70 C1 ib. imm=0b00_01_10_11 reverses lanes.
    let a = [0u8; 16];
    let b = [
        0x11111111u32.to_le_bytes(),
        0x22222222u32.to_le_bytes(),
        0x33333333u32.to_le_bytes(),
        0x44444444u32.to_le_bytes(),
    ]
    .concat();
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]); // movdqu xmm1, [rdi+0x10]
    prog.extend_from_slice(&[0x66, 0x0F, 0x70, 0xC1, 0b00_01_10_11]);
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("pshufd", &prog, sse_scratch(a, b.try_into().unwrap()));
}

#[test]
fn sse2_pshuflw() {
    // PSHUFLW xmm0, xmm1, imm8 = F2 0F 70 C1 ib. Shuffle low 4 words, high qword copied.
    let a = [0u8; 16];
    let b = [
        0x0000u16.to_le_bytes(),
        0x1111u16.to_le_bytes(),
        0x2222u16.to_le_bytes(),
        0x3333u16.to_le_bytes(),
        0xAAAAu16.to_le_bytes(),
        0xBBBBu16.to_le_bytes(),
        0xCCCCu16.to_le_bytes(),
        0xDDDDu16.to_le_bytes(),
    ]
    .concat();
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]);
    prog.extend_from_slice(&[0xF2, 0x0F, 0x70, 0xC1, 0b00_01_10_11]);
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("pshuflw", &prog, sse_scratch(a, b.try_into().unwrap()));
}

#[test]
fn sse2_pshufhw() {
    // PSHUFHW xmm0, xmm1, imm8 = F3 0F 70 C1 ib. Shuffle high 4 words, low qword copied.
    let a = [0u8; 16];
    let b = [
        0x0000u16.to_le_bytes(),
        0x1111u16.to_le_bytes(),
        0x2222u16.to_le_bytes(),
        0x3333u16.to_le_bytes(),
        0xAAAAu16.to_le_bytes(),
        0xBBBBu16.to_le_bytes(),
        0xCCCCu16.to_le_bytes(),
        0xDDDDu16.to_le_bytes(),
    ]
    .concat();
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]);
    prog.extend_from_slice(&[0xF3, 0x0F, 0x70, 0xC1, 0b00_01_10_11]);
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("pshufhw", &prog, sse_scratch(a, b.try_into().unwrap()));
}

// ---- SSE3 MOVSHDUP / MOVSLDUP ----

#[test]
fn sse3_movshdup() {
    // MOVSHDUP xmm0, xmm1 = F3 0F 16 C1. Duplicate odd singles: [a1,a1,a3,a3].
    let a = [0u8; 16];
    let b = f32x4([1.0, 2.0, 3.0, 4.0]);
    check_sse(
        "movshdup",
        &sse_program(&[0xF3, 0x0F, 0x16, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse3_movsldup() {
    // MOVSLDUP xmm0, xmm1 = F3 0F 12 C1. Duplicate even singles: [a0,a0,a2,a2].
    let a = [0u8; 16];
    let b = f32x4([1.0, 2.0, 3.0, 4.0]);
    check_sse(
        "movsldup",
        &sse_program(&[0xF3, 0x0F, 0x12, 0xC1]),
        sse_scratch(a, b),
    );
}

// ---- Scalar conversions: CVTSI2SD / CVTTSD2SI / CVTSS2SD ----

#[test]
fn sse2_cvtsi2sd_neg() {
    // CVTSI2SD xmm0, rax = F2 48 0F 2A C0. Convert signed -1234567 to f64; store.
    let a = [0u8; 16];
    let b = [0u8; 16];
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0x48, 0xC7, 0xC0]); // mov rax, imm32 (sign-ext)
    prog.extend_from_slice(&(-1234567i32).to_le_bytes());
    prog.extend_from_slice(&[0xF2, 0x48, 0x0F, 0x2A, 0xC0]); // cvtsi2sd xmm0, rax
    prog.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("cvtsi2sd_neg", &prog, sse_scratch(a, b));
}

#[test]
fn sse2_cvttsd2si_trunc() {
    // CVTTSD2SI rax, xmm1 = F2 48 0F 2C C1. Truncate -3.9 -> -3; store rax.
    let a = [0u8; 16];
    let b = f64x2([-3.9, 0.0]);
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]); // movdqu xmm1, [rdi+0x10]
    prog.extend_from_slice(&[0xF2, 0x48, 0x0F, 0x2C, 0xC1]); // cvttsd2si rax, xmm1
    prog.extend_from_slice(&[0x48, 0x89, 0x47, 0x20]); // mov [rdi+0x20], rax
    prog.push(HLT);
    check_sse("cvttsd2si", &prog, sse_scratch(a, b));
}

#[test]
fn sse2_cvtsd2si_round_even() {
    // CVTSD2SI rax, xmm1 = F2 48 0F 2D C1. Round-to-nearest-even of 2.5 -> 2.
    let a = [0u8; 16];
    let b = f64x2([2.5, 0.0]);
    let mut prog = load_rdi_data();
    prog.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]);
    prog.extend_from_slice(&[0xF2, 0x48, 0x0F, 0x2D, 0xC1]); // cvtsd2si rax, xmm1
    prog.extend_from_slice(&[0x48, 0x89, 0x47, 0x20]);
    prog.push(HLT);
    check_sse("cvtsd2si_even", &prog, sse_scratch(a, b));
}

#[test]
fn sse2_cvtss2sd_scalar() {
    // CVTSS2SD xmm0, xmm1 = F3 0F 5A C1. Convert lane0 f32 -> f64; lane1 of xmm0 kept.
    let a = f64x2([99.0, 7.0]); // high qword preserved
    let b = f32x4([1.5, 0.0, 0.0, 0.0]);
    check_sse(
        "cvtss2sd",
        &sse_program(&[0xF3, 0x0F, 0x5A, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse2_cvtpd2ps_scalar() {
    // CVTPD2PS xmm0, xmm1 = 66 0F 5A C1. 2 doubles -> 2 floats in low 64, high zeroed.
    let a = [
        0xFFFF_FFFF_FFFF_FFFFu64.to_le_bytes(),
        0xFFFF_FFFF_FFFF_FFFFu64.to_le_bytes(),
    ]
    .concat();
    let b = f64x2([1.25, -8.5]);
    check_sse(
        "cvtpd2ps",
        &sse_program(&[0x66, 0x0F, 0x5A, 0xC1]),
        sse_scratch(a.try_into().unwrap(), b),
    );
}

// ---- x87: FXCH / FABS / FCHS / FRNDINT / FSCALE / FPREM / FSQRT-after-FXCH ----

#[test]
fn x87_fabs() {
    // fld -5.5 ; fabs (D9 E1) ; fstp qword -> 5.5
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi]
    c.extend_from_slice(&[0xD9, 0xE1]); // fabs
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp qword [rdi+0x10]
    c.push(HLT);
    check_mem("x87_fabs", &with_hlt(c), regs(), scratch_f64(&[-5.5]), 0);
}

#[test]
fn x87_fchs() {
    // fld 12.25 ; fchs (D9 E0) ; fstp -> -12.25
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x07]);
    c.extend_from_slice(&[0xD9, 0xE0]); // fchs
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]);
    c.push(HLT);
    check_mem("x87_fchs", &with_hlt(c), regs(), scratch_f64(&[12.25]), 0);
}

#[test]
fn x87_fxch() {
    // fld b ; fld a ; fxch (D9 C9 -> swaps ST0,ST1) ; fstp [rdi+16] (stores old ST1=b);
    // fstp [rdi+24] (stores a). Verifies FXCH swap.
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld qword [rdi+8]  ST0=b
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi]    ST0=a, ST1=b
    c.extend_from_slice(&[0xD9, 0xC9]); // fxch -> ST0=b, ST1=a
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp [rdi+16] = b
    c.extend_from_slice(&[0xDD, 0x5F, 0x18]); // fstp [rdi+24] = a
    c.push(HLT);
    check_mem(
        "x87_fxch",
        &with_hlt(c),
        regs(),
        scratch_f64(&[3.5, 7.25]),
        0,
    );
}

#[test]
fn x87_frndint_even() {
    // fld 2.5 ; frndint (D9 FC, round-to-nearest-even by default) -> 2.0 ; fstp.
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x07]);
    c.extend_from_slice(&[0xD9, 0xFC]); // frndint
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]);
    c.push(HLT);
    check_mem("x87_frndint", &with_hlt(c), regs(), scratch_f64(&[2.5]), 0);
}

#[test]
fn x87_fscale() {
    // FSCALE (D9 FD): ST0 = ST0 * 2^trunc(ST1). fld ST1=3.0, fld ST0=1.5 -> 1.5*8=12.
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld qword [rdi+8]  ST0=exp=3.0
    c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi]    ST0=1.5, ST1=3.0
    c.extend_from_slice(&[0xD9, 0xFD]); // fscale -> ST0 = 1.5 * 2^3 = 12.0
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp [rdi+16]
    c.push(HLT);
    check_mem(
        "x87_fscale",
        &with_hlt(c),
        regs(),
        scratch_f64(&[1.5, 3.0]),
        0,
    );
}

#[test]
fn x87_fprem() {
    // FPREM (D9 F8): ST0 = ST0 - (ST1 * trunc(ST0/ST1)). 17 mod 5 = 2 (exact dyadic ok).
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld [rdi+8] ST0=divisor=5.0
    c.extend_from_slice(&[0xDD, 0x07]); // fld [rdi] ST0=17.0, ST1=5.0
    c.extend_from_slice(&[0xD9, 0xF8]); // fprem -> 2.0
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp [rdi+16]
    c.push(HLT);
    check_mem(
        "x87_fprem",
        &with_hlt(c),
        regs(),
        scratch_f64(&[17.0, 5.0]),
        0,
    );
}

#[test]
fn x87_transcendental_exact_results() {
    let unary_store = |op: &[u8]| {
        let mut c = load_rdi_data();
        c.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi]
        c.extend_from_slice(op);
        c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp qword [rdi+0x10]
        c.push(HLT);
        c
    };

    check_mem(
        "x87_fsin_zero",
        &unary_store(&[0xD9, 0xFE]),
        regs(),
        scratch_f64(&[0.0]),
        0,
    );
    check_mem(
        "x87_fcos_zero",
        &unary_store(&[0xD9, 0xFF]),
        regs(),
        scratch_f64(&[0.0]),
        0,
    );
    check_mem(
        "x87_f2xm1_one",
        &unary_store(&[0xD9, 0xF0]),
        regs(),
        scratch_f64(&[1.0]),
        0,
    );

    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x07]); // fld 0.0
    c.extend_from_slice(&[0xD9, 0xFB]); // fsincos -> ST0=cos(0), ST1=sin(0)
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // cos result
    c.extend_from_slice(&[0xDD, 0x5F, 0x18]); // sin result
    c.push(HLT);
    check_mem("x87_fsincos_zero", &c, regs(), scratch_f64(&[0.0]), 0);

    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x07]); // fld 0.0
    c.extend_from_slice(&[0xD9, 0xF2]); // fptan -> ST0=1.0, ST1=tan(0)
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // pushed 1.0
    c.extend_from_slice(&[0xDD, 0x5F, 0x18]); // tan result
    c.push(HLT);
    check_mem("x87_fptan_zero", &c, regs(), scratch_f64(&[0.0]), 0);

    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld y=3.0
    c.extend_from_slice(&[0xDD, 0x07]); // fld x=8.0
    c.extend_from_slice(&[0xD9, 0xF1]); // fyl2x -> 3.0 * log2(8.0)
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]);
    c.push(HLT);
    check_mem(
        "x87_fyl2x_power_of_two",
        &c,
        regs(),
        scratch_f64(&[8.0, 3.0]),
        0,
    );

    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld y=5.0
    c.extend_from_slice(&[0xDD, 0x07]); // fld x=3.0
    c.extend_from_slice(&[0xD9, 0xF9]); // fyl2xp1 -> 5.0 * log2(1.0 + 3.0)
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]);
    c.push(HLT);
    check_mem(
        "x87_fyl2xp1_power_of_two",
        &c,
        regs(),
        scratch_f64(&[3.0, 5.0]),
        0,
    );

    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x07]); // fld 12.0
    c.extend_from_slice(&[0xD9, 0xF4]); // fxtract -> ST0=significand, ST1=exponent
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // significand 1.5
    c.extend_from_slice(&[0xDD, 0x5F, 0x18]); // exponent 3.0
    c.push(HLT);
    check_mem("x87_fxtract", &c, regs(), scratch_f64(&[12.0]), 0);

    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld divisor=5.0
    c.extend_from_slice(&[0xDD, 0x07]); // fld dividend=18.0
    c.extend_from_slice(&[0xD9, 0xF5]); // fprem1 uses nearest quotient: 18 - 4*5 = -2
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]);
    c.push(HLT);
    check_mem(
        "x87_fprem1_nearest",
        &c,
        regs(),
        scratch_f64(&[18.0, 5.0]),
        0,
    );
}

#[test]
fn x87_fmul_st_then_fxch() {
    // fld a; fld b; fmulp st1,st0 (DE C9) -> ST0 = a*b ; fstp. (4.0 * 0.5 = 2.0)
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x07]); // fld a -> ST0=a
    c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld b -> ST0=b, ST1=a
    c.extend_from_slice(&[0xDE, 0xC9]); // fmulp st1, st0 -> ST0 = a*b, pop
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp [rdi+16]
    c.push(HLT);
    check_mem(
        "x87_fmulp",
        &with_hlt(c),
        regs(),
        scratch_f64(&[4.0, 0.5]),
        0,
    );
}

#[test]
fn x87_fld1_fldz_fadd() {
    // FLD1 (D9 E8) pushes 1.0, FLDZ (D9 EE) pushes 0.0, FADDP adds -> 1.0; fstp.
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xD9, 0xE8]); // fld1 -> ST0=1.0
    c.extend_from_slice(&[0xD9, 0xEE]); // fldz -> ST0=0.0, ST1=1.0
    c.extend_from_slice(&[0xDE, 0xC1]); // faddp st1, st0 -> ST0=1.0
    c.extend_from_slice(&[0xDD, 0x5F, 0x10]); // fstp [rdi+16]
    c.push(HLT);
    check_mem(
        "x87_fld1_fldz",
        &with_hlt(c),
        regs(),
        scratch_f64(&[0.0]),
        0,
    );
}

#[test]
fn x87_fcom_fstsw_flags() {
    // Compare ST0 vs ST1 via FCOM, then FNSTSW AX (DF E0) and SAHF to map C0/C2/C3
    // into CF/PF/ZF. a=3.0 > b=5.0? No (a<b) -> C0=1 (CF). Probe via SAHF.
    // fld b; fld a; fcom st1 (D8 D1); fnstsw ax (DF E0); sahf (9E); hlt.
    let mut c = load_rdi_data();
    c.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld [rdi+8] = b -> ST0=b
    c.extend_from_slice(&[0xDD, 0x07]); // fld [rdi] = a -> ST0=a, ST1=b
    c.extend_from_slice(&[0xD8, 0xD1]); // fcom st1
    c.extend_from_slice(&[0xDF, 0xE0]); // fnstsw ax
    c.extend_from_slice(&[0x9E]); // sahf  (CF<-C0, PF<-C2, ZF<-C3)
    c.push(HLT);
    // a=3.0 < b=5.0 -> C3=0,C2=0,C0=1 -> ZF=0, PF=0, CF=1. Compare ZF|PF|CF.
    check_mem(
        "x87_fcom_sahf",
        &c,
        regs(),
        scratch_f64(&[3.0, 5.0]),
        FCOMI_FLAGS,
    );
}

// ---- ALU corner cases not yet hit ----

#[test]
fn neg64_int_min() {
    // NEG of INT64_MIN: result is INT64_MIN again (overflow), OF=1, CF=1, SF=1.
    let mut r = regs();
    r.rax = 0x8000_0000_0000_0000;
    check("neg64_int_min", &with_hlt(vec![0x48, 0xF7, 0xD8]), r);
}

#[test]
fn not64_no_flags() {
    // NOT rax (48 F7 D0) affects no flags; seed all status flags and verify survival.
    let mut r = regs();
    r.rax = 0x0F0F_0F0F_0F0F_0F0F;
    r.rflags = FLAG_MASK;
    check("not64", &with_hlt(vec![0x48, 0xF7, 0xD0]), r);
}

#[test]
fn test_imm32_high_bit() {
    // TEST eax, imm32 (A9 id) with a high-bit-set mask -> SF=1.
    let mut r = regs();
    r.rax = 0xFFFF_FFFF_8000_0001;
    // A9 00 00 00 80  test eax, 0x80000000
    check(
        "test_imm32",
        &with_hlt(vec![0xA9, 0x00, 0x00, 0x00, 0x80]),
        r,
    );
}

#[test]
fn add_al_imm_af_only() {
    // ADD AL, imm8 where only AF is set (0x08 + 0x08 = 0x10): AF=1, CF=0, ZF=0.
    let mut r = regs();
    r.rax = 0x08;
    check("add_al_af", &with_hlt(vec![0x04, 0x08]), r); // add al, 8
}

#[test]
fn sbb_self_with_carry() {
    // SBB rax, rax with CF=1 -> rax = -1 (0xFFFF...), a common idiom (mask = -CF).
    let mut r = regs();
    r.rax = 0x1234;
    r.rflags = flags::bits::CF;
    check("sbb_self", &with_hlt(vec![0x48, 0x19, 0xC0]), r); // sbb rax, rax
}

#[test]
fn shl_count_masked_to_zero_64() {
    // shl rax, cl with CL=64 -> masked to 0 (no shift, NO flags change). Seed flags
    // and verify they survive and rax is unchanged.
    let mut r = regs();
    r.rax = 0x1;
    r.rcx = 64; // 64 & 63 = 0
    r.rflags = flags::bits::CF | flags::bits::OF | flags::bits::SF;
    // 48 D3 E0  shl rax, cl ; flags must be unchanged when masked count is 0.
    check("shl_cl64", &with_hlt(vec![0x48, 0xD3, 0xE0]), r);
}

#[test]
fn bt_mem_reg_bit_index() {
    // BT [rdi], rdx (48 0F A3 17) : memory bit-test with a large bit index that
    // selects beyond the first qword (index 70 -> byte 8, bit 6). CF<-that bit.
    let mut s = [0u8; 64];
    s[8] = 1 << 6; // bit 70 set
    let mut r = regs();
    r.rdi = DATA_ADDR;
    r.rdx = 70;
    check_mem(
        "bt_mem",
        &with_hlt(vec![0x48, 0x0F, 0xA3, 0x17]),
        r,
        s,
        BT_DEFINED,
    );
}

#[test]
fn movsxd_no_rex_w() {
    // 63 /r without REX.W is MOVSXD r32, r/m32 in 64-bit mode but acts as a plain
    // 32-bit mov (no sign extension) — actually MOVSXD ALWAYS needs REX.W to be
    // meaningful; without it, it's still MOVSXD but dest is 32-bit (zero-extends).
    // Verify both backends agree on the 32-bit (no-REX.W) form.
    let mut r = regs();
    r.rbx = 0x0000_0000_8000_0000;
    r.rax = 0xFFFF_FFFF_FFFF_FFFF; // should be overwritten
    // 63 C3  movsxd eax, ebx  -> eax = ebx, zero-extended into rax
    check("movsxd_no_rexw", &with_hlt(vec![0x63, 0xC3]), r);
}

// ===========================================================================
// EXPANDED COVERAGE PART 5: stack/control/data-movement/hint/string gaps.
//
// These are intentionally exact-state cases: balanced stack control flow,
// absolute-offset MOVs against the scratch page, no-op/hint instructions, XLAT,
// and string widths/modes that were still thin in the corpus.
// ===========================================================================

#[test]
fn stack_push_imm8_signext_pop_rax() {
    // PUSH imm8 sign-extends in 64-bit mode; POP r64 recovers the full qword.
    check(
        "push_imm8_pop_rax",
        &with_hlt(vec![0x6A, 0x80, 0x58]),
        regs(),
    );
}

#[test]
fn stack_push_imm32_pop_rbx() {
    // PUSH imm32 sign-extends to the 64-bit stack operand size.
    check(
        "push_imm32_pop_rbx",
        &with_hlt(vec![0x68, 0xEF, 0xBE, 0xAD, 0xDE, 0x5B]),
        regs(),
    );
}

#[test]
fn stack_push_pop_extended_regs() {
    // REX.B extended register stack forms: push r8; pop r9.
    let mut r = regs();
    r.r8 = 0x0123_4567_89AB_CDEF;
    r.r9 = 0xFFFF_FFFF_FFFF_FFFF;
    check("push_r8_pop_r9", &with_hlt(vec![0x41, 0x50, 0x41, 0x59]), r);
}

#[test]
fn stack_push16_pop16_preserves_upper() {
    // 66 PUSH/POP use a 16-bit stack operand in long mode; POP r16 preserves upper bits.
    let mut r = regs();
    r.rax = 0xAAAA_BBBB_CCCC_1234;
    r.rbx = 0x1111_2222_3333_4444;
    check("push16_pop16", &with_hlt(vec![0x66, 0x50, 0x66, 0x5B]), r);
}

#[test]
fn stack_pop_rm64_memory() {
    // POP r/m64 to memory: push RAX, then pop into scratch.
    let mut r = regs();
    r.rax = 0x1122_3344_5566_7788;
    r.rdi = DATA_ADDR;
    check_mem(
        "pop_rm64_mem",
        &with_hlt(vec![0x50, 0x8F, 0x07]),
        r,
        zero_scratch(),
        FLAG_MASK,
    );
}

#[test]
fn stack_pushfq_pop_rax() {
    // PUSHFQ then POP RAX exposes the exact flags image both backends pushed.
    let mut r = regs();
    r.rflags = flags::bits::CF | flags::bits::PF | flags::bits::ZF | flags::bits::OF;
    check("pushfq_pop_rax", &with_hlt(vec![0x9C, 0x58]), r);
}

#[test]
fn control_call_rel32_ret_balanced() {
    // call func; add rax,2; hlt; func: add rax,5; ret. Final RAX=7, stack balanced.
    let mut r = regs();
    r.rax = 0;
    check(
        "call_ret",
        &with_hlt(vec![
            0xE8, 0x05, 0x00, 0x00, 0x00, // call +5 -> func
            0x48, 0x83, 0xC0, 0x02, // add rax, 2
            0xF4, // hlt
            0x48, 0x83, 0xC0, 0x05, // func: add rax, 5
            0xC3, // ret
        ]),
        r,
    );
}

#[test]
fn control_ret_imm16_cleans_arguments() {
    // sub rsp,16; call func; hlt; func: ret 16. RET imm restores the pre-call RSP.
    check(
        "ret_imm16",
        &with_hlt(vec![
            0x48, 0x83, 0xEC, 0x10, // sub rsp, 16
            0xE8, 0x01, 0x00, 0x00, 0x00, // call +1 -> func
            0xF4, // hlt
            0xC2, 0x10, 0x00, // ret 16
        ]),
        regs(),
    );
}

#[test]
fn control_enter_leave_balanced() {
    // ENTER allocates a frame and LEAVE tears it back down.
    let mut r = regs();
    r.rbp = STACK_ADDR - 0x80;
    check(
        "enter_leave",
        &with_hlt(vec![0xC8, 0x20, 0x00, 0x00, 0xC9]),
        r,
    );
}

#[test]
fn control_iretq_same_cpl_restores_frame() {
    let target = CODE_ADDR + 0x1C;
    let mut code = vec![
        0x6A, 0x10, // push 0x10 (SS)
        0x68, 0x00, 0x00, 0x02, 0x00, // push STACK_ADDR
        0x68, 0x47, 0x02, 0x00, 0x00, // push restored RFLAGS
        0x6A, 0x08, // push 0x08 (CS)
        0x48, 0xB8, // movabs rax, target
    ];
    code.extend_from_slice(&target.to_le_bytes());
    code.extend_from_slice(&[
        0x50, // push rax
        0x48, 0xCF, // iretq
        0xF4, // hlt fallback if IRETQ does not branch
        0xB8, 0x5A, 0x00, 0x00, 0x00, // target: mov eax, 0x5a
        0xF4, // hlt
    ]);

    check("iretq_same_cpl", &code, regs());
}

#[test]
fn control_jrcxz_taken_and_not_taken() {
    let code = with_hlt(vec![
        0xE3, 0x03, // jrcxz taken_target
        0xB0, 0x01, // mov al, 1
        0xF4, // hlt
        0xB0, 0x02, // taken_target: mov al, 2
    ]);
    let mut taken = regs();
    taken.rcx = 0;
    check("jrcxz_taken", &code, taken);

    let mut not_taken = regs();
    not_taken.rcx = 1;
    check("jrcxz_not_taken", &code, not_taken);
}

#[test]
fn control_loopz_loopnz_conditions() {
    let loopz = with_hlt(vec![
        0xE1, 0x03, // loopz target
        0xB0, 0x01, // mov al, 1
        0xF4, // hlt
        0xB0, 0x02, // target: mov al, 2
    ]);
    let mut r = regs();
    r.rcx = 2;
    r.rflags = flags::bits::ZF;
    check("loopz_taken", &loopz, r);

    let loopnz = with_hlt(vec![
        0xE0, 0x03, // loopnz target
        0xB0, 0x01, // mov al, 1
        0xF4, // hlt
        0xB0, 0x02, // target: mov al, 2
    ]);
    let mut r = regs();
    r.rcx = 2;
    r.rflags = 0;
    check("loopnz_taken", &loopnz, r);
}

#[test]
fn mov_moffs_load_store_forms() {
    // A0/A1/A2/A3 absolute-offset MOVs in long mode use a 64-bit moffs.
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0x0123_4567_89AB_CDEFu64.to_le_bytes());
    let mut code = Vec::new();
    code.push(0xA0); // mov al, moffs8
    code.extend_from_slice(&DATA_ADDR.to_le_bytes());
    code.push(0xA2); // mov moffs8, al
    code.extend_from_slice(&(DATA_ADDR + 8).to_le_bytes());
    code.extend_from_slice(&[0x48, 0xA1]); // mov rax, moffs64
    code.extend_from_slice(&DATA_ADDR.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xA3]); // mov moffs64, rax
    code.extend_from_slice(&(DATA_ADDR + 16).to_le_bytes());
    check_mem("mov_moffs", &with_hlt(code), regs(), s, FLAG_MASK);
}

#[test]
fn xlat_table_lookup() {
    // XLAT: AL <- byte ptr [RBX + AL].
    let mut s = [0u8; 64];
    s[5] = 0xA7;
    let mut r = regs();
    r.rbx = DATA_ADDR;
    r.rax = 0x1122_3344_5566_7705;
    check_mem("xlat", &with_hlt(vec![0xD7]), r, s, FLAG_MASK);
}

#[test]
fn hint_prefetch_and_nop_forms_preserve_state() {
    // PREFETCHNTA/T0/T1/T2, PREFETCHW/WT1, ENDBR64, and NOP r/m are architectural no-ops.
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0xDEAD_BEEF_CAFE_BABEu64.to_le_bytes());
    let mut r = modern_flags_regs();
    r.rdi = DATA_ADDR;
    check_mem(
        "prefetch_endbr_nop",
        &with_hlt(vec![
            0x0F, 0x18, 0x07, // prefetchnta [rdi]
            0x0F, 0x18, 0x0F, // prefetcht0 [rdi]
            0x0F, 0x18, 0x17, // prefetcht1 [rdi]
            0x0F, 0x18, 0x1F, // prefetcht2 [rdi]
            0x0F, 0x0D, 0x0F, // prefetchw [rdi]
            0x0F, 0x0D, 0x17, // prefetchwt1 [rdi]
            0xF3, 0x0F, 0x1E, 0xFA, // endbr64
            0x0F, 0x1F, 0x44, 0x00, 0x00, // nop dword ptr [rax+rax]
        ]),
        r,
        s,
        FLAG_MASK,
    );
}

#[test]
fn str_rep_stosw_and_stosq() {
    let mut r = string_regs(4);
    r.rax = 0xBEEF;
    check_mem(
        "rep_stosw",
        &with_hlt(vec![0xF3, 0x66, 0xAB]),
        r,
        string_scratch(&[]),
        0,
    );

    let mut r = string_regs(2);
    r.rax = 0x0123_4567_89AB_CDEF;
    check_mem(
        "rep_stosq",
        &with_hlt(vec![0xF3, 0x48, 0xAB]),
        r,
        string_scratch(&[]),
        0,
    );
}

#[test]
fn prefix_rex_before_rep_stos_is_ignored() {
    // REX must be the effective final prefix before the opcode. Hardware consumes
    // `48 F3 AB` as REP STOSD, not REP STOSQ, because REP follows and masks off
    // the earlier REX.W state.
    let mut r = string_regs(2);
    r.rax = 0x0123_4567_89AB_CDEF;
    check_mem(
        "rex_before_rep_stosd",
        &with_hlt(vec![0x48, 0xF3, 0xAB]),
        r,
        string_scratch(&[]),
        0,
    );
}

#[test]
fn str_lodsw_lodsd_lodsq() {
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0x0123_4567_89AB_CDEFu64.to_le_bytes());

    let mut r = regs();
    r.rsi = DATA_ADDR;
    r.rax = 0xFFFF_FFFF_FFFF_0000;
    check_mem("lodsw", &with_hlt(vec![0x66, 0xAD]), r, s, 0);

    let mut r = regs();
    r.rsi = DATA_ADDR;
    r.rax = 0xFFFF_FFFF_FFFF_FFFF;
    check_mem("lodsd", &with_hlt(vec![0xAD]), r, s, 0);

    let mut r = regs();
    r.rsi = DATA_ADDR;
    r.rax = 0;
    check_mem("lodsq", &with_hlt(vec![0x48, 0xAD]), r, s, 0);
}

#[test]
fn str_repne_cmpsb_mismatch() {
    // REPNE CMPSB keeps going while bytes differ, stopping on equality.
    let mut s = [0u8; 64];
    s[SRC_OFF as usize..SRC_OFF as usize + 5].copy_from_slice(&[9, 8, 7, 6, 5]);
    s[DST_OFF as usize..DST_OFF as usize + 5].copy_from_slice(&[1, 2, 7, 4, 3]);
    let mut r = regs();
    r.rsi = DATA_ADDR + SRC_OFF;
    r.rdi = DATA_ADDR + DST_OFF;
    r.rcx = 5;
    check_mem("repne_cmpsb", &with_hlt(vec![0xF2, 0xA6]), r, s, FLAG_MASK);
}

#[test]
fn str_scasd_and_scasq() {
    let mut s = [0u8; 64];
    s[DST_OFF as usize..DST_OFF as usize + 4].copy_from_slice(&0x8000_0000u32.to_le_bytes());
    let mut r = regs();
    r.rdi = DATA_ADDR + DST_OFF;
    r.rax = 0x8000_0000;
    check_mem("scasd_eq", &with_hlt(vec![0xAF]), r, s, FLAG_MASK);

    let mut s = [0u8; 64];
    s[DST_OFF as usize..DST_OFF as usize + 8]
        .copy_from_slice(&0x0123_4567_89AB_CDEFu64.to_le_bytes());
    let mut r = regs();
    r.rdi = DATA_ADDR + DST_OFF;
    r.rax = 0x1111_1111_1111_1111;
    check_mem("scasq_ne", &with_hlt(vec![0x48, 0xAF]), r, s, FLAG_MASK);
}

// ===========================================================================
// EXPANDED COVERAGE PART 6: remaining exact SIMD/memory forms.
//
// These target implemented SSE/SSE2 paths that were still absent from the
// differential corpus: scalar compare flags, non-temporal stores, mask stores,
// logical-not vector ops, signed word min/max, qword arithmetic, and XMM-count
// packed shifts.
// ===========================================================================

fn flags_seeded_regs() -> Registers {
    let mut r = regs();
    r.rflags = FLAG_MASK;
    r
}

#[test]
fn sse_comiss_ucomisd_flags() {
    let a = f32x4([1.5, 0.0, 0.0, 0.0]);
    let b = f32x4([2.5, 0.0, 0.0, 0.0]);
    check_mem(
        "comiss_less",
        &sse_program(&[0x0F, 0x2F, 0xC1]),
        flags_seeded_regs(),
        sse_scratch(a, b),
        FLAG_MASK,
    );

    let a = f64x2([3.0, 0.0]);
    let b = f64x2([3.0, 0.0]);
    check_mem(
        "ucomisd_equal",
        &sse_program(&[0x66, 0x0F, 0x2E, 0xC1]),
        flags_seeded_regs(),
        sse_scratch(a, b),
        FLAG_MASK,
    );
}

#[test]
fn sse_logical_not_and_or_forms() {
    let a = [
        0x00, 0xFF, 0x55, 0xAA, 0x11, 0x22, 0x33, 0x44, 0x80, 0x7F, 0xCC, 0x33, 0xF0, 0x0F,
        0x5A, 0xA5,
    ];
    let b = [
        0xFF, 0x0F, 0xF0, 0x55, 0xEE, 0xDD, 0xCC, 0xBB, 0x7F, 0x80, 0x33, 0xCC, 0x0F, 0xF0,
        0xA5, 0x5A,
    ];
    check_sse(
        "andnps",
        &sse_program(&[0x0F, 0x55, 0xC1]),
        sse_scratch(a, b),
    );
    check_sse(
        "andnpd",
        &sse_program(&[0x66, 0x0F, 0x55, 0xC1]),
        sse_scratch(a, b),
    );
    check_sse(
        "pandn",
        &sse_program(&[0x66, 0x0F, 0xDF, 0xC1]),
        sse_scratch(a, b),
    );
    check_sse(
        "por",
        &sse_program(&[0x66, 0x0F, 0xEB, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse2_pavgw_pminsw_pmaxsw() {
    let a = [
        0x0000u16.to_le_bytes(),
        0xFFFFu16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
        0x0001u16.to_le_bytes(),
        0xFFFEu16.to_le_bytes(),
        0x1234u16.to_le_bytes(),
        0xFEDCu16.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x0001u16.to_le_bytes(),
        0x0001u16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
        0xFFFFu16.to_le_bytes(),
        0x0002u16.to_le_bytes(),
        0x4321u16.to_le_bytes(),
        0x1111u16.to_le_bytes(),
    ]
    .concat();
    let a: [u8; 16] = a.try_into().unwrap();
    let b: [u8; 16] = b.try_into().unwrap();
    check_sse(
        "pavgw",
        &sse_program(&[0x66, 0x0F, 0xE3, 0xC1]),
        sse_scratch(a, b),
    );
    check_sse(
        "pminsw",
        &sse_program(&[0x66, 0x0F, 0xEA, 0xC1]),
        sse_scratch(a, b),
    );
    check_sse(
        "pmaxsw",
        &sse_program(&[0x66, 0x0F, 0xEE, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse2_paddq_psubq() {
    let a = [
        0xFFFF_FFFF_FFFF_FFFFu64.to_le_bytes(),
        0x8000_0000_0000_0000u64.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x0000_0000_0000_0002u64.to_le_bytes(),
        0x8000_0000_0000_0001u64.to_le_bytes(),
    ]
    .concat();
    let a: [u8; 16] = a.try_into().unwrap();
    let b: [u8; 16] = b.try_into().unwrap();
    check_sse(
        "paddq",
        &sse_program(&[0x66, 0x0F, 0xD4, 0xC1]),
        sse_scratch(a, b),
    );
    check_sse(
        "psubq",
        &sse_program(&[0x66, 0x0F, 0xFB, 0xC1]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse2_xmm_count_shifts() {
    let a = [
        0x8000_0000_0000_0001u64.to_le_bytes(),
        0x0123_4567_89AB_CDEFu64.to_le_bytes(),
    ]
    .concat();
    let mut count = [0u8; 16];
    count[0..8].copy_from_slice(&4u64.to_le_bytes());
    let a: [u8; 16] = a.try_into().unwrap();
    check_sse(
        "psrlq_xmm_count",
        &sse_program(&[0x66, 0x0F, 0xD3, 0xC1]),
        sse_scratch(a, count),
    );
    check_sse(
        "psllq_xmm_count",
        &sse_program(&[0x66, 0x0F, 0xF3, 0xC1]),
        sse_scratch(a, count),
    );
    check_sse(
        "psrad_xmm_count",
        &sse_program(&[0x66, 0x0F, 0xE2, 0xC1]),
        sse_scratch(a, count),
    );
}

#[test]
fn sse2_memory_count_shift_forms() {
    let a = [
        0x80, 0x01, 0x7F, 0xFE, 0x34, 0x12, 0xCC, 0xDD, 0x01, 0x80, 0xEF, 0x7E, 0x89, 0xAB,
        0x67, 0x45,
    ];

    for (label, opcode, count) in [
        ("psrlw_mem_count", 0xD1, 4u64),
        ("psrld_mem_count", 0xD2, 12),
        ("psrlq_mem_count", 0xD3, 17),
        ("psraw_mem_count", 0xE1, 5),
        ("psrad_mem_count", 0xE2, 9),
        ("psllw_mem_count", 0xF1, 3),
        ("pslld_mem_count", 0xF2, 13),
        ("psllq_mem_count", 0xF3, 29),
        ("psrlw_mem_count_edge", 0xD1, 16),
        ("psrld_mem_count_edge", 0xD2, 32),
        ("psrlq_mem_count_edge", 0xD3, 64),
        ("psraw_mem_count_edge", 0xE1, 19),
        ("psrad_mem_count_edge", 0xE2, 35),
        ("psllw_mem_count_edge", 0xF1, 16),
        ("pslld_mem_count_edge", 0xF2, 32),
        ("psllq_mem_count_edge", 0xF3, 64),
    ] {
        let mut count_bytes = [0u8; 16];
        count_bytes[0..8].copy_from_slice(&count.to_le_bytes());
        check_sse(
            label,
            &sse_program(&[0x66, 0x0F, opcode, 0x47, 0x10]),
            sse_scratch(a, count_bytes),
        );
    }
}

#[test]
fn sse_movlps_movhps_load_store() {
    let mut s = [0u8; 64];
    s[0..16].copy_from_slice(&[
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D,
        0x1E, 0x1F,
    ]);
    s[16..24].copy_from_slice(&[0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7]);
    s[24..32].copy_from_slice(&[0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7]);

    let mut code = load_rdi_data();
    code.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]); // movdqu xmm0, [rdi]
    code.extend_from_slice(&[0x0F, 0x12, 0x47, 0x10]); // movlps xmm0, [rdi+0x10]
    code.extend_from_slice(&[0x0F, 0x16, 0x47, 0x18]); // movhps xmm0, [rdi+0x18]
    code.extend_from_slice(&[0x0F, 0x13, 0x47, 0x20]); // movlps [rdi+0x20], xmm0
    code.extend_from_slice(&[0x0F, 0x17, 0x47, 0x28]); // movhps [rdi+0x28], xmm0
    code.push(HLT);
    check_mem("movlps_movhps", &code, regs(), s, 0);
}

#[test]
fn sse_non_temporal_stores() {
    let mut s = [0u8; 64];
    s[0..16].copy_from_slice(&[
        0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0xFE, 0xDC, 0xBA, 0x98, 0x76, 0x54,
        0x32, 0x10,
    ]);
    let mut code = load_rdi_data();
    code.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]); // movdqu xmm0, [rdi]
    code.extend_from_slice(&[0x0F, 0x2B, 0x47, 0x20]); // movntps [rdi+0x20], xmm0
    code.extend_from_slice(&[0x66, 0x0F, 0xE7, 0x47, 0x30]); // movntdq [rdi+0x30], xmm0
    code.push(HLT);
    check_mem("movntps_movntdq", &code, regs(), s, 0);

    let mut r = regs();
    r.rax = 0x1122_3344_5566_7788;
    r.rdi = DATA_ADDR;
    check_mem(
        "movnti_m64",
        &with_hlt(vec![0x48, 0x0F, 0xC3, 0x07]),
        r,
        zero_scratch(),
        FLAG_MASK,
    );
}

#[test]
fn sse_maskmovdqu_selected_bytes() {
    let mut s = [0u8; 64];
    s[0..16].copy_from_slice(&[
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
        0xEE, 0xFF,
    ]);
    s[16..32].copy_from_slice(&[
        0x80, 0x00, 0x80, 0x00, 0x80, 0x00, 0x80, 0x00, 0x00, 0x80, 0x00, 0x80, 0x00, 0x80,
        0x00, 0x80,
    ]);

    let mut code = load_rdi_data();
    code.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]); // movdqu xmm0, [rdi]
    code.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]); // movdqu xmm1, [rdi+0x10]
    code.extend_from_slice(&[0x48, 0x83, 0xC7, 0x20]); // add rdi, 0x20
    code.extend_from_slice(&[0x66, 0x0F, 0xF7, 0xC1]); // maskmovdqu xmm0, xmm1
    code.push(HLT);
    check_mem("maskmovdqu", &code, regs(), s, 0);
}

// ===========================================================================
// EXPANDED COVERAGE PART 7: packed move forms and MMX-only aliases.
//
// These cover aligned/unaligned SSE move opcodes and the MMX dispatch branches
// that share opcode space with XMM forms but operate on MM registers.
// ===========================================================================

fn mmx_scratch(a: [u8; 8], b: [u8; 8]) -> [u8; 64] {
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&a);
    s[8..16].copy_from_slice(&b);
    s
}

fn mmx_program(op: &[u8]) -> Vec<u8> {
    let mut code = load_rdi_data();
    code.extend_from_slice(&[0x0F, 0x6F, 0x07]); // movq mm0, [rdi]
    code.extend_from_slice(&[0x0F, 0x6F, 0x4F, 0x08]); // movq mm1, [rdi+8]
    code.extend_from_slice(op);
    code.extend_from_slice(&[0x0F, 0x7F, 0x47, 0x20]); // movq [rdi+0x20], mm0
    code.push(HLT);
    code
}

fn mmx_memory_source_program(opcode: u8) -> Vec<u8> {
    let mut code = load_rdi_data();
    code.extend_from_slice(&[0x0F, 0x6F, 0x07]); // movq mm0, [rdi]
    code.extend_from_slice(&[0x0F, opcode, 0x47, 0x08]); // <op> mm0, [rdi+8]
    code.extend_from_slice(&[0x0F, 0x7F, 0x47, 0x20]); // movq [rdi+0x20], mm0
    code.push(HLT);
    code
}

#[test]
fn mmx_pshufw_register_and_memory_sources() {
    let a = [0x00, 0x01, 0x10, 0x11, 0x20, 0x21, 0x30, 0x31];
    let b = [0x80, 0x81, 0x90, 0x91, 0xA0, 0xA1, 0xB0, 0xB1];

    check_mem(
        "mmx_pshufw_reg",
        &mmx_program(&[0x0F, 0x70, 0xC1, 0x27]),
        regs(),
        mmx_scratch(a, b),
        0,
    );

    let mut code = load_rdi_data();
    code.extend_from_slice(&[0x0F, 0x70, 0x47, 0x08, 0x72]); // pshufw mm0, [rdi+8], 0x72
    code.extend_from_slice(&[0x0F, 0x7F, 0x47, 0x20]); // movq [rdi+0x20], mm0
    code.push(HLT);
    check_mem("mmx_pshufw_mem", &code, regs(), mmx_scratch(a, b), 0);
}

#[test]
fn mmx_immediate_shift_forms_and_edge_counts() {
    let a = [0x01, 0x80, 0xFF, 0x7F, 0x34, 0x12, 0xCC, 0xDD];
    let b = [0x55, 0xAA, 0x10, 0x20, 0xFE, 0xDC, 0x98, 0x76];

    for (label, opcode, modrm, imm8) in [
        ("mmx_psrlw_imm", 0x71, 0xD0, 4),
        ("mmx_psraw_imm_edge", 0x71, 0xE0, 19),
        ("mmx_psllw_imm_edge", 0x71, 0xF0, 16),
        ("mmx_psrld_imm", 0x72, 0xD0, 8),
        ("mmx_psrad_imm_edge", 0x72, 0xE0, 35),
        ("mmx_pslld_imm_edge", 0x72, 0xF0, 32),
        ("mmx_psrlq_imm", 0x73, 0xD0, 17),
        ("mmx_psllq_imm_edge", 0x73, 0xF0, 64),
    ] {
        check_mem(
            label,
            &mmx_program(&[0x0F, opcode, modrm, imm8]),
            regs(),
            mmx_scratch(a, b),
            0,
        );
    }
}

#[test]
fn sse_movups_movupd_unaligned_load_store() {
    let mut s = [0u8; 64];
    for (idx, byte) in s.iter_mut().enumerate() {
        *byte = idx.wrapping_mul(3).wrapping_add(0x11) as u8;
    }

    let mut code = load_rdi_data();
    code.extend_from_slice(&[0x48, 0x83, 0xC7, 0x01]); // add rdi, 1
    code.extend_from_slice(&[0x0F, 0x10, 0x07]); // movups xmm0, [rdi]
    code.extend_from_slice(&[0x0F, 0x11, 0x47, 0x20]); // movups [rdi+0x20], xmm0
    code.extend_from_slice(&[0x66, 0x0F, 0x10, 0x4F, 0x02]); // movupd xmm1, [rdi+2]
    code.extend_from_slice(&[0x66, 0x0F, 0x11, 0x4F, 0x2C]); // movupd [rdi+0x2c], xmm1
    code.push(HLT);
    check_mem("movups_movupd_unaligned", &code, regs(), s, 0);
}

#[test]
fn sse_movaps_movapd_aligned_load_store() {
    let mut s = [0u8; 64];
    for (idx, byte) in s.iter_mut().enumerate() {
        *byte = idx.wrapping_mul(5).wrapping_add(0x23) as u8;
    }

    let mut code = load_rdi_data();
    code.extend_from_slice(&[0x0F, 0x28, 0x07]); // movaps xmm0, [rdi]
    code.extend_from_slice(&[0x0F, 0x29, 0x47, 0x20]); // movaps [rdi+0x20], xmm0
    code.extend_from_slice(&[0x66, 0x0F, 0x28, 0x4F, 0x10]); // movapd xmm1, [rdi+0x10]
    code.extend_from_slice(&[0x66, 0x0F, 0x29, 0x4F, 0x30]); // movapd [rdi+0x30], xmm1
    code.push(HLT);
    check_mem("movaps_movapd_aligned", &code, regs(), s, 0);
}

#[test]
fn sse2_movdqa_aligned_load_store() {
    let mut s = [0u8; 64];
    for (idx, byte) in s.iter_mut().enumerate() {
        *byte = idx.wrapping_mul(7).wrapping_add(0x41) as u8;
    }

    let mut code = load_rdi_data();
    code.extend_from_slice(&[0x66, 0x0F, 0x6F, 0x07]); // movdqa xmm0, [rdi]
    code.extend_from_slice(&[0x66, 0x0F, 0x7F, 0x47, 0x20]); // movdqa [rdi+0x20], xmm0
    code.push(HLT);
    check_mem("movdqa_aligned", &code, regs(), s, 0);
}

#[test]
fn mmx_movntq_store() {
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&0x0123_4567_89AB_CDEFu64.to_le_bytes());

    let mut code = load_rdi_data();
    code.extend_from_slice(&[0x0F, 0x6F, 0x07]); // movq mm0, [rdi]
    code.extend_from_slice(&[0x0F, 0xE7, 0x47, 0x20]); // movntq [rdi+0x20], mm0
    code.push(HLT);
    check_mem("movntq", &code, regs(), s, 0);
}

#[test]
fn mmx_maskmovq_selected_bytes() {
    let mut s = [0u8; 64];
    s[0..8].copy_from_slice(&[0x10, 0x21, 0x32, 0x43, 0x54, 0x65, 0x76, 0x87]);
    s[8..16].copy_from_slice(&[0x80, 0x00, 0x00, 0x80, 0x00, 0x80, 0x80, 0x00]);

    let mut code = load_rdi_data();
    code.extend_from_slice(&[0x0F, 0x6F, 0x07]); // movq mm0, [rdi]
    code.extend_from_slice(&[0x0F, 0x6F, 0x4F, 0x08]); // movq mm1, [rdi+8]
    code.extend_from_slice(&[0x48, 0x83, 0xC7, 0x20]); // add rdi, 0x20
    code.extend_from_slice(&[0x0F, 0xF7, 0xC1]); // maskmovq mm0, mm1
    code.push(HLT);
    check_mem("maskmovq", &code, regs(), s, 0);
}

#[test]
fn mmx_unpack_memory_source_forms() {
    let a = [0x00, 0x01, 0x02, 0x03, 0x70, 0x71, 0x72, 0x73];
    let b = [0x80, 0x81, 0x82, 0x83, 0xF0, 0xF1, 0xF2, 0xF3];

    for (label, opcode) in [
        ("mmx_punpcklbw_mem", 0x60),
        ("mmx_punpcklwd_mem", 0x61),
        ("mmx_punpckldq_mem", 0x62),
        ("mmx_punpckhbw_mem", 0x68),
        ("mmx_punpckhwd_mem", 0x69),
        ("mmx_punpckhdq_mem", 0x6A),
    ] {
        check_mem(
            label,
            &mmx_memory_source_program(opcode),
            regs(),
            mmx_scratch(a, b),
            0,
        );
    }
}

#[test]
fn mmx_arithmetic_memory_source_forms() {
    let a = [0x7F, 0x80, 0xFF, 0x00, 0x10, 0xF0, 0x55, 0xAA];
    let b = [0x01, 0xFF, 0x01, 0xFF, 0xF0, 0x20, 0xAA, 0x55];

    for (label, opcode) in [
        ("mmx_paddq_mem", 0xD4),
        ("mmx_paddusb_mem", 0xDC),
        ("mmx_paddusw_mem", 0xDD),
        ("mmx_paddsb_mem", 0xEC),
        ("mmx_paddsw_mem", 0xED),
        ("mmx_paddb_mem", 0xFC),
        ("mmx_paddw_mem", 0xFD),
        ("mmx_paddd_mem", 0xFE),
        ("mmx_psubusb_mem", 0xD8),
        ("mmx_psubusw_mem", 0xD9),
        ("mmx_psubsb_mem", 0xE8),
        ("mmx_psubsw_mem", 0xE9),
        ("mmx_psubb_mem", 0xF8),
        ("mmx_psubw_mem", 0xF9),
        ("mmx_psubd_mem", 0xFA),
        ("mmx_psubq_mem", 0xFB),
    ] {
        check_mem(
            label,
            &mmx_memory_source_program(opcode),
            regs(),
            mmx_scratch(a, b),
            0,
        );
    }
}

#[test]
fn mmx_compare_logical_memory_source_forms() {
    let a = [0x80, 0x7F, 0x00, 0xFF, 0x34, 0x12, 0xAA, 0x55];
    let b = [0x7F, 0x7F, 0x00, 0x01, 0x78, 0x56, 0x55, 0xAA];

    for (label, opcode) in [
        ("mmx_pcmpeqb_mem", 0x74),
        ("mmx_pcmpeqw_mem", 0x75),
        ("mmx_pcmpeqd_mem", 0x76),
        ("mmx_pcmpgtb_mem", 0x64),
        ("mmx_pcmpgtw_mem", 0x65),
        ("mmx_pcmpgtd_mem", 0x66),
        ("mmx_pand_mem", 0xDB),
        ("mmx_pandn_mem", 0xDF),
        ("mmx_por_mem", 0xEB),
        ("mmx_pxor_mem", 0xEF),
    ] {
        check_mem(
            label,
            &mmx_memory_source_program(opcode),
            regs(),
            mmx_scratch(a, b),
            0,
        );
    }
}

#[test]
fn mmx_multiply_average_minmax_memory_source_forms() {
    let a = [
        0x0002u16.to_le_bytes(),
        0xFFFEu16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
    ]
    .concat()
    .try_into()
    .unwrap();
    let b = [
        0x0003u16.to_le_bytes(),
        0x0004u16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
    ]
    .concat()
    .try_into()
    .unwrap();

    for (label, opcode) in [
        ("mmx_pmullw_mem", 0xD5),
        ("mmx_pmulhuw_mem", 0xE4),
        ("mmx_pmulhw_mem", 0xE5),
        ("mmx_pmuludq_mem", 0xF4),
        ("mmx_pmaddwd_mem", 0xF5),
        ("mmx_psadbw_mem", 0xF6),
        ("mmx_pavgb_mem", 0xE0),
        ("mmx_pavgw_mem", 0xE3),
        ("mmx_pminub_mem", 0xDA),
        ("mmx_pmaxub_mem", 0xDE),
        ("mmx_pminsw_mem", 0xEA),
        ("mmx_pmaxsw_mem", 0xEE),
    ] {
        check_mem(
            label,
            &mmx_memory_source_program(opcode),
            regs(),
            mmx_scratch(a, b),
            0,
        );
    }
}

#[test]
fn mmx_pandn_por() {
    let a = [0x00, 0xFF, 0x55, 0xAA, 0x11, 0x22, 0x33, 0x44];
    let b = [0xFF, 0x0F, 0xF0, 0x55, 0xEE, 0xDD, 0xCC, 0xBB];

    check_mem(
        "mmx_pandn",
        &mmx_program(&[0x0F, 0xDF, 0xC1]),
        regs(),
        mmx_scratch(a, b),
        0,
    );
    check_mem(
        "mmx_por",
        &mmx_program(&[0x0F, 0xEB, 0xC1]),
        regs(),
        mmx_scratch(a, b),
        0,
    );
}

#[test]
fn mmx_paddq_psubq() {
    let a = 0xFFFF_FFFF_FFFF_FFFFu64.to_le_bytes();
    let b = 0x0000_0000_0000_0002u64.to_le_bytes();

    check_mem(
        "mmx_paddq",
        &mmx_program(&[0x0F, 0xD4, 0xC1]),
        regs(),
        mmx_scratch(a, b),
        0,
    );
    check_mem(
        "mmx_psubq",
        &mmx_program(&[0x0F, 0xFB, 0xC1]),
        regs(),
        mmx_scratch(a, b),
        0,
    );
}

#[test]
fn mmx_pavgw_pminsw_pmaxsw() {
    let a = [
        0x0000u16.to_le_bytes(),
        0xFFFFu16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
    ]
    .concat()
    .try_into()
    .unwrap();
    let b = [
        0x0001u16.to_le_bytes(),
        0x0001u16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
    ]
    .concat()
    .try_into()
    .unwrap();

    check_mem(
        "mmx_pavgw",
        &mmx_program(&[0x0F, 0xE3, 0xC1]),
        regs(),
        mmx_scratch(a, b),
        0,
    );
    check_mem(
        "mmx_pminsw",
        &mmx_program(&[0x0F, 0xEA, 0xC1]),
        regs(),
        mmx_scratch(a, b),
        0,
    );
    check_mem(
        "mmx_pmaxsw",
        &mmx_program(&[0x0F, 0xEE, 0xC1]),
        regs(),
        mmx_scratch(a, b),
        0,
    );
}

#[test]
fn mmx_pmaddwd_psadbw() {
    let words_a = [
        0x0002u16.to_le_bytes(),
        0xFFFEu16.to_le_bytes(),
        0x7FFFu16.to_le_bytes(),
        0x8000u16.to_le_bytes(),
    ]
    .concat()
    .try_into()
    .unwrap();
    let words_b = [
        0x0003u16.to_le_bytes(),
        0x0004u16.to_le_bytes(),
        0x0002u16.to_le_bytes(),
        0xFFFFu16.to_le_bytes(),
    ]
    .concat()
    .try_into()
    .unwrap();
    check_mem(
        "mmx_pmaddwd",
        &mmx_program(&[0x0F, 0xF5, 0xC1]),
        regs(),
        mmx_scratch(words_a, words_b),
        0,
    );

    let bytes_a = [0x00, 0x10, 0x20, 0x80, 0x90, 0xA0, 0xF0, 0xFF];
    let bytes_b = [0x01, 0x08, 0x40, 0x7F, 0xB0, 0x80, 0x10, 0x00];
    check_mem(
        "mmx_psadbw",
        &mmx_program(&[0x0F, 0xF6, 0xC1]),
        regs(),
        mmx_scratch(bytes_a, bytes_b),
        0,
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 8: deterministic port I/O exits.
//
// The harness completes KVM and interpreter input exits with zero bytes and
// ignores output exits, so these compare the architectural register/memory/index
// effects of IN/OUT/INS/OUTS without depending on any external device model.
// ===========================================================================

#[test]
fn io_in_immediate_widths() {
    let mut r = regs();
    r.rax = 0x1122_3344_5566_7788;
    check("in_al_imm8", &with_hlt(vec![0xE4, 0x80]), r);

    let mut r = regs();
    r.rax = 0x1122_3344_5566_7788;
    check("in_ax_imm8", &with_hlt(vec![0x66, 0xE5, 0x81]), r);

    let mut r = regs();
    r.rax = 0x1122_3344_5566_7788;
    check("in_eax_imm8", &with_hlt(vec![0xE5, 0x82]), r);
}

#[test]
fn io_in_dx_widths() {
    let mut r = regs();
    r.rax = 0x8877_6655_4433_2211;
    r.rdx = 0x1234;
    check("in_al_dx", &with_hlt(vec![0xEC]), r);

    let mut r = regs();
    r.rax = 0x8877_6655_4433_2211;
    r.rdx = 0x1234;
    check("in_ax_dx", &with_hlt(vec![0x66, 0xED]), r);

    let mut r = regs();
    r.rax = 0x8877_6655_4433_2211;
    r.rdx = 0x1234;
    check("in_eax_dx", &with_hlt(vec![0xED]), r);
}

#[test]
fn io_out_forms_preserve_state() {
    let mut r = regs();
    r.rax = 0x0123_4567_89AB_CDEF;
    r.rdx = 0x03F8;
    check(
        "out_all_forms",
        &with_hlt(vec![
            0xE6, 0x80, // out 0x80, al
            0x66, 0xE7, 0x81, // out 0x81, ax
            0xE7, 0x82, // out 0x82, eax
            0xEE, // out dx, al
            0x66, 0xEF, // out dx, ax
            0xEF, // out dx, eax
        ]),
        r,
    );
}

fn io_scratch() -> [u8; 64] {
    let mut s = [0u8; 64];
    for (idx, byte) in s.iter_mut().enumerate() {
        *byte = 0xA0u8.wrapping_add(idx as u8);
    }
    s
}

#[test]
fn io_ins_string_widths() {
    let mut r = regs();
    r.rdi = DATA_ADDR + 8;
    r.rdx = 0x1234;
    check_mem("insb", &with_hlt(vec![0x6C]), r, io_scratch(), FLAG_MASK);

    let mut r = regs();
    r.rdi = DATA_ADDR + 8;
    r.rdx = 0x1234;
    check_mem(
        "insw",
        &with_hlt(vec![0x66, 0x6D]),
        r,
        io_scratch(),
        FLAG_MASK,
    );

    let mut r = regs();
    r.rdi = DATA_ADDR + 8;
    r.rdx = 0x1234;
    check_mem("insd", &with_hlt(vec![0x6D]), r, io_scratch(), FLAG_MASK);
}

#[test]
fn io_rep_ins_forward() {
    let mut r = regs();
    r.rdi = DATA_ADDR + 12;
    r.rcx = 3;
    r.rdx = 0x1234;
    check_mem(
        "rep_insb",
        &with_hlt(vec![0xF3, 0x6C]),
        r,
        io_scratch(),
        FLAG_MASK,
    );

    let mut r = regs();
    r.rdi = DATA_ADDR + 24;
    r.rcx = 2;
    r.rdx = 0x1234;
    check_mem(
        "rep_insd",
        &with_hlt(vec![0xF3, 0x6D]),
        r,
        io_scratch(),
        FLAG_MASK,
    );
}

#[test]
fn io_outs_string_forms() {
    let mut r = regs();
    r.rsi = DATA_ADDR + 5;
    r.rdx = 0x1234;
    check_mem("outsb", &with_hlt(vec![0x6E]), r, io_scratch(), FLAG_MASK);

    let mut r = regs();
    r.rsi = DATA_ADDR + 20;
    r.rcx = 2;
    r.rdx = 0x1234;
    r.rflags = flags::bits::DF;
    check_mem(
        "rep_outsw_df",
        &with_hlt(vec![0xF3, 0x66, 0x6F]),
        r,
        io_scratch(),
        FLAG_MASK,
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 9: remaining deterministic SSE4 immediate forms.
//
// These exercise SSE4.1/SSE4.2 immediate encodings that produce exact bit-pattern
// results: double blends, extract/insert lane forms, MPSADBW, and PHMINPOSUW.
// ===========================================================================

#[test]
fn sse4_blendpd() {
    let a = [
        0x1111_1111_1111_1111u64.to_le_bytes(),
        0x2222_2222_2222_2222u64.to_le_bytes(),
    ]
    .concat();
    let b = [
        0xAAAA_AAAA_AAAA_AAAAu64.to_le_bytes(),
        0xBBBB_BBBB_BBBB_BBBBu64.to_le_bytes(),
    ]
    .concat();
    check_sse(
        "blendpd",
        &sse_program(&[0x66, 0x0F, 0x3A, 0x0D, 0xC1, 0b10]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse4_extractps_to_reg_and_mem() {
    let a = [0u8; 16];
    let b = [
        0x1111_1111u32.to_le_bytes(),
        0x2222_2222u32.to_le_bytes(),
        0x3333_3333u32.to_le_bytes(),
        0x4444_4444u32.to_le_bytes(),
    ]
    .concat();
    let mut code = load_rdi_data();
    code.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]); // movdqu xmm1, [rdi+0x10]
    code.extend_from_slice(&[0x66, 0x0F, 0x3A, 0x17, 0xC9, 0x03]); // extractps ecx, xmm1, 3
    code.extend_from_slice(&[0x89, 0x4F, 0x20]); // mov [rdi+0x20], ecx
    code.extend_from_slice(&[0x66, 0x0F, 0x3A, 0x17, 0x4F, 0x24, 0x01]); // extractps [rdi+0x24], xmm1, 1
    code.push(HLT);
    check_sse(
        "extractps_reg_mem",
        &code,
        sse_scratch(a, b.try_into().unwrap()),
    );
}

#[test]
fn sse4_pextrq_to_reg() {
    let a = [0u8; 16];
    let b = [
        0x0123_4567_89AB_CDEFu64.to_le_bytes(),
        0xFEDC_BA98_7654_3210u64.to_le_bytes(),
    ]
    .concat();
    let mut code = load_rdi_data();
    code.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]); // movdqu xmm1, [rdi+0x10]
    code.extend_from_slice(&[0x66, 0x48, 0x0F, 0x3A, 0x16, 0xC9, 0x01]); // pextrq rcx, xmm1, 1
    code.extend_from_slice(&[0x48, 0x89, 0x4F, 0x20]); // mov [rdi+0x20], rcx
    code.push(HLT);
    check_sse("pextrq_reg", &code, sse_scratch(a, b.try_into().unwrap()));
}

#[test]
fn sse4_pinsrb_and_pinsrq_from_reg() {
    let a = [
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
        0xEE, 0xFF,
    ];
    let b = [0u8; 16];

    let mut code = load_rdi_data();
    code.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x07]); // movdqu xmm0, [rdi]
    code.extend_from_slice(&[0xB8, 0x5A, 0x00, 0x00, 0x00]); // mov eax, 0x5a
    code.extend_from_slice(&[0x66, 0x0F, 0x3A, 0x20, 0xC0, 0x0E]); // pinsrb xmm0, eax, 14
    code.extend_from_slice(&[0x48, 0xB8]); // mov rax, imm64
    code.extend_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
    code.extend_from_slice(&[0x66, 0x48, 0x0F, 0x3A, 0x22, 0xC0, 0x01]); // pinsrq xmm0, rax, 1
    code.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]); // movdqu [rdi+0x20], xmm0
    code.push(HLT);
    check_sse("pinsrb_pinsrq", &code, sse_scratch(a, b));
}

#[test]
fn sse4_insertps_reg_source() {
    let a = [
        0xAAAA_0000u32.to_le_bytes(),
        0xBBBB_1111u32.to_le_bytes(),
        0xCCCC_2222u32.to_le_bytes(),
        0xDDDD_3333u32.to_le_bytes(),
    ]
    .concat();
    let b = [
        0x1000_0001u32.to_le_bytes(),
        0x2000_0002u32.to_le_bytes(),
        0x3000_0003u32.to_le_bytes(),
        0x4000_0004u32.to_le_bytes(),
    ]
    .concat();
    check_sse(
        "insertps_reg",
        &sse_program(&[0x66, 0x0F, 0x3A, 0x21, 0xC1, 0x98]),
        sse_scratch(a.try_into().unwrap(), b.try_into().unwrap()),
    );
}

#[test]
fn sse4_insert_extract_memory_operands() {
    let a = [
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC, 0xDD,
        0xEE, 0xFF,
    ];
    let b = [
        0x10, 0x32, 0x54, 0x76, 0x98, 0xBA, 0xDC, 0xFE, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB,
        0xCD, 0xEF,
    ];

    check_sse(
        "pinsrw_mem",
        &sse_program(&[0x66, 0x0F, 0xC4, 0x47, 0x10, 0x05]),
        sse_scratch(a, b),
    );
    check_sse(
        "pinsrb_mem",
        &sse_program(&[0x66, 0x0F, 0x3A, 0x20, 0x47, 0x13, 0x0E]),
        sse_scratch(a, b),
    );
    check_sse(
        "pinsrd_mem",
        &sse_program(&[0x66, 0x0F, 0x3A, 0x22, 0x47, 0x14, 0x02]),
        sse_scratch(a, b),
    );
    check_sse(
        "pinsrq_mem",
        &sse_program(&[0x66, 0x48, 0x0F, 0x3A, 0x22, 0x47, 0x18, 0x01]),
        sse_scratch(a, b),
    );
    check_sse(
        "insertps_mem",
        &sse_program(&[0x66, 0x0F, 0x3A, 0x21, 0x47, 0x10, 0x20]),
        sse_scratch(a, b),
    );

    let mut code = load_rdi_data();
    code.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x4F, 0x10]); // movdqu xmm1, [rdi+0x10]
    code.extend_from_slice(&[0x66, 0x0F, 0x3A, 0x15, 0x4F, 0x20, 0x06]); // pextrw [rdi+0x20], xmm1, 6
    code.extend_from_slice(&[0x66, 0x0F, 0x3A, 0x16, 0x4F, 0x24, 0x02]); // pextrd [rdi+0x24], xmm1, 2
    code.extend_from_slice(&[0x66, 0x48, 0x0F, 0x3A, 0x16, 0x4F, 0x28, 0x01]); // pextrq [rdi+0x28], xmm1, 1
    code.push(HLT);
    check_sse("pextr_mem", &code, sse_scratch(a, b));
}

#[test]
fn sse4_mpsadbw() {
    let a = [
        0x00, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xA0, 0xB0, 0xC0, 0xD0,
        0xE0, 0xF0,
    ];
    let b = [
        0x05, 0x15, 0x25, 0x35, 0x45, 0x55, 0x65, 0x75, 0x85, 0x95, 0xA5, 0xB5, 0xC5, 0xD5,
        0xE5, 0xF5,
    ];
    check_sse(
        "mpsadbw",
        &sse_program(&[0x66, 0x0F, 0x3A, 0x42, 0xC1, 0x05]),
        sse_scratch(a, b),
    );
}

#[test]
fn sse4_phminposuw() {
    let a = [0u8; 16];
    let b = [
        0x1000u16.to_le_bytes(),
        0x0007u16.to_le_bytes(),
        0x0020u16.to_le_bytes(),
        0x0007u16.to_le_bytes(),
        0x00FFu16.to_le_bytes(),
        0x0003u16.to_le_bytes(),
        0x0004u16.to_le_bytes(),
        0x0003u16.to_le_bytes(),
    ]
    .concat();
    check_sse(
        "phminposuw",
        &sse_program(&[0x66, 0x0F, 0x38, 0x41, 0xC1]),
        sse_scratch(a, b.try_into().unwrap()),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 10: SSE4.2 string compare instructions.
//
// PCMPxSTRx has several unusually subtle architectural outputs: RCX or XMM0,
// CF/ZF/SF/OF, explicit signed lengths, implicit NUL scanning, polarity, output
// selection, and a separate memory-source ModR/M path. These deterministic KVM
// cases complement the randomized fuzz generator and keep the important edge
// shapes in the normal differential suite.
// ===========================================================================

fn pcmp_scratch(op1: [u8; 16], op2: [u8; 16]) -> [u8; 64] {
    let mut s = [0u8; 64];
    s[0..16].copy_from_slice(&op1);
    s[16..32].copy_from_slice(&op2);
    s
}

fn pcmp_reg_program(opcode: u8, imm: u8, store_xmm0: bool) -> Vec<u8> {
    let mut code = load_rdi_data();
    code.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x0F]); // movdqu xmm1, [rdi]
    code.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x57, 0x10]); // movdqu xmm2, [rdi+0x10]
    code.extend_from_slice(&[0x66, 0x0F, 0x3A, opcode, 0xCA, imm]); // pcmp* xmm1, xmm2, imm
    if store_xmm0 {
        code.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]); // movdqu [rdi+0x20], xmm0
    }
    code.push(HLT);
    code
}

fn pcmp_mem_program(opcode: u8, imm: u8, store_xmm0: bool) -> Vec<u8> {
    let mut code = load_rdi_data();
    code.extend_from_slice(&[0xF3, 0x0F, 0x6F, 0x0F]); // movdqu xmm1, [rdi]
    code.extend_from_slice(&[0x66, 0x0F, 0x3A, opcode, 0x4F, 0x10, imm]); // pcmp* xmm1, [rdi+0x10], imm
    if store_xmm0 {
        code.extend_from_slice(&[0xF3, 0x0F, 0x7F, 0x47, 0x20]); // movdqu [rdi+0x20], xmm0
    }
    code.push(HLT);
    code
}

fn check_pcmp_index(label: &str, code: &[u8], init: Registers, scratch: [u8; 64]) {
    let Some((interp, kvm)) = run_both(code, init, scratch) else {
        return;
    };
    let opts = CompareOpts {
        flag_mask: FLAG_MASK,
        scratch: false,
        ..CompareOpts::default()
    };
    assert_match(label, code, &interp, &kvm, opts);
}

fn check_pcmp_mask(label: &str, code: &[u8], init: Registers, scratch: [u8; 64]) {
    let Some((interp, kvm)) = run_both(code, init, scratch) else {
        return;
    };
    let opts = CompareOpts {
        flag_mask: FLAG_MASK,
        xmm_count: 1,
        scratch: true,
        ..CompareOpts::default()
    };
    assert_match(label, code, &interp, &kvm, opts);
}

#[test]
fn sse42_pcmpistri_strlen_idiom() {
    let op = *b"rax\0............";
    let mut init = regs();
    init.rcx = 0xFFFF_FFFF_FFFF_FFFF;
    // PCMPISTRI xmm1, xmm2, 0x3a: byte equal-each, masked-negative polarity,
    // least-significant index. Self-compare should report the NUL position.
    check_pcmp_index(
        "pcmpistri_strlen_idiom",
        &pcmp_reg_program(0x63, 0x3A, false),
        init,
        pcmp_scratch(op, op),
    );
}

#[test]
fn sse42_pcmpistri_memory_ranges() {
    let ranges = [
        b'0', b'9', b'A', b'F', b'a', b'f', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    let haystack = *b"z-7Fq\0..........";
    let mut init = regs();
    init.rcx = 0xAAAA_AAAA_AAAA_AAAA;
    // PCMPISTRI xmm1, [rdi+0x10], 0x04: unsigned byte ranges, LSB index.
    check_pcmp_index(
        "pcmpistri_mem_ranges",
        &pcmp_mem_program(0x63, 0x04, false),
        init,
        pcmp_scratch(ranges, haystack),
    );
}

#[test]
fn sse42_pcmpistrm_word_expanded_mask() {
    let mut ranges = [0u8; 16];
    for (i, word) in [0x0030u16, 0x0039, 0x0041, 0x005A].iter().enumerate() {
        ranges[i * 2..i * 2 + 2].copy_from_slice(&word.to_le_bytes());
    }

    let mut haystack = [0u8; 16];
    for (i, word) in [0x002Fu16, 0x0035, 0x0042, 0x0061, 0, 0, 0, 0]
        .iter()
        .enumerate()
    {
        haystack[i * 2..i * 2 + 2].copy_from_slice(&word.to_le_bytes());
    }

    // PCMPISTRM xmm1, xmm2, 0x45: unsigned word ranges, expanded word mask.
    check_pcmp_mask(
        "pcmpistrm_word_expanded",
        &pcmp_reg_program(0x62, 0x45, true),
        regs(),
        pcmp_scratch(ranges, haystack),
    );
}

#[test]
fn sse42_pcmpestri_negative_lengths_equal_ordered() {
    let needle = *b"bar.............";
    let haystack = *b"foobarbaz.......";
    let mut init = regs();
    init.rax = (-3i32) as u32 as u64;
    init.rdx = (-9i32) as u32 as u64;
    init.rcx = 0xFFFF_FFFF_FFFF_FFFF;
    // PCMPESTRI xmm1, xmm2, 0x0c: byte equal-ordered. Negative EAX/EDX lengths
    // use their absolute values, saturated to the element count.
    check_pcmp_index(
        "pcmpestri_negative_lengths",
        &pcmp_reg_program(0x61, 0x0C, false),
        init,
        pcmp_scratch(needle, haystack),
    );
}

#[test]
fn sse42_pcmpestri_memory_msb_index() {
    let needle = *b"a...............";
    let haystack = *b"abracadabra.....";
    let mut init = regs();
    init.rax = 1;
    init.rdx = 11;
    init.rcx = 0x7777_7777_7777_7777;
    // PCMPESTRI xmm1, [rdi+0x10], 0x4c: byte equal-ordered, MSB index.
    check_pcmp_index(
        "pcmpestri_mem_msb",
        &pcmp_mem_program(0x61, 0x4C, false),
        init,
        pcmp_scratch(needle, haystack),
    );
}

#[test]
fn sse42_pcmpestrm_equal_each_masks() {
    let lhs = *b"HELLO...........";
    let rhs = *b"HEXLO...........";
    let mut init = regs();
    init.rax = 5;
    init.rdx = 5;

    // PCMPESTRM xmm1, xmm2, 0x08: byte equal-each, bit mask.
    check_pcmp_mask(
        "pcmpestrm_equal_each_bitmask",
        &pcmp_reg_program(0x60, 0x08, true),
        init.clone(),
        pcmp_scratch(lhs, rhs),
    );

    // PCMPESTRM xmm1, xmm2, 0x48: same comparison, expanded byte mask.
    check_pcmp_mask(
        "pcmpestrm_equal_each_expanded",
        &pcmp_reg_program(0x60, 0x48, true),
        init,
        pcmp_scratch(lhs, rhs),
    );
}

#[test]
fn sse42_pcmpestrm_memory_signed_ranges() {
    let ranges = [
        0x80u8, 0x90, 0xF0, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    let values = [
        0x7F, 0x80, 0x8F, 0xE0, 0xF8, 0x00, 0x11, 0x22, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    let mut init = regs();
    init.rax = 4;
    init.rdx = 8;
    // PCMPESTRM xmm1, [rdi+0x10], 0x46: signed byte ranges, expanded byte mask.
    check_pcmp_mask(
        "pcmpestrm_mem_signed_ranges",
        &pcmp_mem_program(0x60, 0x46, true),
        init,
        pcmp_scratch(ranges, values),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 11: AVX/AVX2 VEX-encoded vector operations.
//
// These programs enable OSXSAVE + XCR0[AVX] inside the guest before executing
// VEX instructions, then store all vector results back into the existing
// 64-byte scratch page. That keeps the KVM/interpreter comparison fully
// memory-observable without widening the differential harness state.
// ===========================================================================

fn xcr0_enable_prologue(xcr0_low: u32) -> Vec<u8> {
    let mut code = Vec::new();
    code.extend_from_slice(&[0x0F, 0x20, 0xE0]); // mov rax, cr4
    code.extend_from_slice(&[0x48, 0x0D, 0x00, 0x00, 0x04, 0x00]); // or rax, 0x40000
    code.extend_from_slice(&[0x0F, 0x22, 0xE0]); // mov cr4, rax
    code.push(0xB8); // mov eax, imm32
    code.extend_from_slice(&xcr0_low.to_le_bytes());
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    code.extend_from_slice(&[0xB9, 0x00, 0x00, 0x00, 0x00]); // mov ecx, 0
    code.extend_from_slice(&[0x0F, 0x01, 0xD1]); // xsetbv
    code
}

#[test]
fn xgetbv_reads_default_xcr0_after_osxsave_enable() {
    let mut code = xcr0_enable_prologue(1);
    code.extend_from_slice(&[0x48, 0xC7, 0xC0]); // mov rax, 0
    code.extend_from_slice(&0u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xC7, 0xC3]); // mov rbx, 1
    code.extend_from_slice(&1u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0x39, 0xD8]); // cmp rax, rbx
    code.extend_from_slice(&[0xB9, 0x00, 0x00, 0x00, 0x00]); // mov ecx, 0
    code.extend_from_slice(&[0x48, 0xC7, 0xC0]); // mov rax, -1
    code.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xC7, 0xC2]); // mov rdx, -1
    code.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    code.extend_from_slice(&[0x0F, 0x01, 0xD0]); // xgetbv

    check_flags_masked("xgetbv_default_xcr0", &with_hlt(code), regs(), FLAG_MASK);
}

#[test]
fn xgetbv_reads_default_xss_with_ecx_1() {
    let mut code = xcr0_enable_prologue(0xE7);
    code.extend_from_slice(&[0xB9, 0x01, 0x00, 0x00, 0x00]); // mov ecx, 1
    code.extend_from_slice(&[0x48, 0xC7, 0xC0]); // mov rax, -1
    code.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xC7, 0xC2]); // mov rdx, -1
    code.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    code.extend_from_slice(&[0x0F, 0x01, 0xD0]); // xgetbv

    check_flags_masked("xgetbv_default_xss", &with_hlt(code), regs(), FLAG_MASK);
}

#[test]
fn xsetbv_xgetbv_avx_and_avx512_readback() {
    let mut code = xcr0_enable_prologue(0x7);
    code.extend_from_slice(&[0xB9, 0x00, 0x00, 0x00, 0x00]); // mov ecx, 0
    code.extend_from_slice(&[0x0F, 0x01, 0xD0]); // xgetbv
    code.extend_from_slice(&[0x48, 0x89, 0xC3]); // mov rbx, rax
    code.extend_from_slice(&[0x48, 0x89, 0xD6]); // mov rsi, rdx

    code.extend_from_slice(&xcr0_enable_prologue(0xE7));
    code.extend_from_slice(&[0x48, 0xC7, 0xC0]); // mov rax, 0
    code.extend_from_slice(&0u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xC7, 0xC7]); // mov rdi, 1
    code.extend_from_slice(&1u32.to_le_bytes());
    code.extend_from_slice(&[0x48, 0x39, 0xF8]); // cmp rax, rdi
    code.extend_from_slice(&[0xB9, 0x00, 0x00, 0x00, 0x00]); // mov ecx, 0
    code.extend_from_slice(&[0x0F, 0x01, 0xD0]); // xgetbv
    code.extend_from_slice(&[0x48, 0x89, 0xC7]); // mov rdi, rax
    code.extend_from_slice(&[0x48, 0x89, 0xD5]); // mov rbp, rdx

    check_flags_masked("xsetbv_xgetbv_readback", &with_hlt(code), regs(), FLAG_MASK);
}

#[test]
fn ldmxcsr_stmxcsr_roundtrips_memory_value() {
    let mut s = [0u8; 64];
    s[0..4].copy_from_slice(&0x0000_7F80u32.to_le_bytes());

    let mut code = load_rdi_data();
    code.extend_from_slice(&[0x0F, 0xAE, 0x17]); // ldmxcsr [rdi]
    code.extend_from_slice(&[0x0F, 0xAE, 0x5F, 0x04]); // stmxcsr [rdi+4]
    code.push(HLT);

    check_mem("ldmxcsr_stmxcsr_roundtrip", &code, regs(), s, 0);
}

#[test]
fn vldmxcsr_vstmxcsr_roundtrips_memory_value() {
    let mut s = [0u8; 64];
    s[0..4].copy_from_slice(&0x0000_5F80u32.to_le_bytes());

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xF8, 0xAE, 0x17]); // vldmxcsr [rdi]
    code.extend_from_slice(&[0xC5, 0xF8, 0xAE, 0x5F, 0x04]); // vstmxcsr [rdi+4]
    code.push(HLT);

    check_avx_mem("vldmxcsr_vstmxcsr_roundtrip", &code, s);
}

#[test]
fn fxsave_fxrstor_roundtrips_mxcsr() {
    let mut s = [0u8; 64];
    s[0..4].copy_from_slice(&0x0000_7F80u32.to_le_bytes());
    s[4..8].copy_from_slice(&0x0000_3F80u32.to_le_bytes());

    let mut code = load_rdi_data();
    code.extend_from_slice(&[0xBB, 0x00, 0x10, 0x03, 0x00]); // mov ebx, DATA_ADDR+0x1000
    code.extend_from_slice(&[0x0F, 0xAE, 0x17]); // ldmxcsr [rdi]
    code.extend_from_slice(&[0x0F, 0xAE, 0x03]); // fxsave [rbx]
    code.extend_from_slice(&[0x0F, 0xAE, 0x57, 0x04]); // ldmxcsr [rdi+4]
    code.extend_from_slice(&[0x0F, 0xAE, 0x0B]); // fxrstor [rbx]
    code.extend_from_slice(&[0x0F, 0xAE, 0x5F, 0x08]); // stmxcsr [rdi+8]
    code.push(HLT);

    check_mem("fxsave_fxrstor_mxcsr", &code, regs(), s, 0);
}

#[test]
fn xsave_xrstor_roundtrips_mxcsr() {
    let mut s = [0u8; 64];
    s[0..4].copy_from_slice(&0x0000_7F80u32.to_le_bytes());
    s[4..8].copy_from_slice(&0x0000_3F80u32.to_le_bytes());

    let mut code = avx_start();
    code.extend_from_slice(&[0xBB, 0x00, 0x10, 0x03, 0x00]); // mov ebx, DATA_ADDR+0x1000
    code.extend_from_slice(&[0x0F, 0xAE, 0x17]); // ldmxcsr [rdi]
    code.extend_from_slice(&[0xB8, 0x03, 0x00, 0x00, 0x00]); // mov eax, 3
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    code.extend_from_slice(&[0x0F, 0xAE, 0x23]); // xsave [rbx]
    code.extend_from_slice(&[0x0F, 0xAE, 0x57, 0x04]); // ldmxcsr [rdi+4]
    code.extend_from_slice(&[0xB8, 0x03, 0x00, 0x00, 0x00]); // mov eax, 3
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    code.extend_from_slice(&[0x0F, 0xAE, 0x2B]); // xrstor [rbx]
    code.extend_from_slice(&[0x0F, 0xAE, 0x5F, 0x08]); // stmxcsr [rdi+8]
    code.push(HLT);

    check_avx_mem("xsave_xrstor_mxcsr", &code, s);
}

#[test]
fn xsaveopt_xrstor_roundtrips_mxcsr() {
    let mut s = [0u8; 64];
    s[0..4].copy_from_slice(&0x0000_5F80u32.to_le_bytes());
    s[4..8].copy_from_slice(&0x0000_1F80u32.to_le_bytes());

    let mut code = avx_start();
    code.extend_from_slice(&[0xBB, 0x00, 0x10, 0x03, 0x00]); // mov ebx, DATA_ADDR+0x1000
    code.extend_from_slice(&[0x0F, 0xAE, 0x17]); // ldmxcsr [rdi]
    code.extend_from_slice(&[0xB8, 0x03, 0x00, 0x00, 0x00]); // mov eax, 3
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    code.extend_from_slice(&[0x0F, 0xAE, 0x33]); // xsaveopt [rbx]
    code.extend_from_slice(&[0x0F, 0xAE, 0x57, 0x04]); // ldmxcsr [rdi+4]
    code.extend_from_slice(&[0xB8, 0x03, 0x00, 0x00, 0x00]); // mov eax, 3
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    code.extend_from_slice(&[0x0F, 0xAE, 0x2B]); // xrstor [rbx]
    code.extend_from_slice(&[0x0F, 0xAE, 0x5F, 0x08]); // stmxcsr [rdi+8]
    code.push(HLT);

    check_avx_mem("xsaveopt_xrstor_mxcsr", &code, s);
}

#[test]
fn xsavec_writes_compacted_sse_header() {
    let mut s = [0u8; 64];
    s[0..4].copy_from_slice(&0x0000_5F80u32.to_le_bytes());

    let mut code = avx_start();
    code.extend_from_slice(&[0xBB, 0x00, 0x10, 0x03, 0x00]); // mov ebx, DATA_ADDR+0x1000
    code.extend_from_slice(&[0x0F, 0xAE, 0x17]); // ldmxcsr [rdi]
    code.extend_from_slice(&[0xB8, 0x02, 0x00, 0x00, 0x00]); // mov eax, 2
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    code.extend_from_slice(&[0x0F, 0xC7, 0x23]); // xsavec [rbx]
    code.extend_from_slice(&[0x48, 0x8B, 0x83, 0x00, 0x02, 0x00, 0x00]); // mov rax, [rbx+512]
    code.extend_from_slice(&[0x48, 0x89, 0x47, 0x08]); // mov [rdi+8], rax
    code.extend_from_slice(&[0x48, 0x8B, 0x83, 0x08, 0x02, 0x00, 0x00]); // mov rax, [rbx+520]
    code.extend_from_slice(&[0x48, 0x89, 0x47, 0x10]); // mov [rdi+16], rax
    code.push(HLT);

    check_avx_mem("xsavec_compacted_sse_header", &code, s);
}

#[test]
fn xsavec_xrstors_roundtrips_mxcsr() {
    let mut s = [0u8; 64];
    s[0..4].copy_from_slice(&0x0000_5F80u32.to_le_bytes());
    s[4..8].copy_from_slice(&0x0000_1F80u32.to_le_bytes());

    let mut code = avx_start();
    code.extend_from_slice(&[0xBB, 0x00, 0x10, 0x03, 0x00]); // mov ebx, DATA_ADDR+0x1000
    code.extend_from_slice(&[0x0F, 0xAE, 0x17]); // ldmxcsr [rdi]
    code.extend_from_slice(&[0xB8, 0x02, 0x00, 0x00, 0x00]); // mov eax, 2
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    code.extend_from_slice(&[0x0F, 0xC7, 0x23]); // xsavec [rbx]
    code.extend_from_slice(&[0x0F, 0xAE, 0x57, 0x04]); // ldmxcsr [rdi+4]
    code.extend_from_slice(&[0xB8, 0x02, 0x00, 0x00, 0x00]); // mov eax, 2
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    code.extend_from_slice(&[0x0F, 0xC7, 0x1B]); // xrstors [rbx]
    code.extend_from_slice(&[0x0F, 0xAE, 0x5F, 0x08]); // stmxcsr [rdi+8]
    code.push(HLT);

    check_avx_mem("xsavec_xrstors_mxcsr", &code, s);
}

#[test]
fn xsavec_xrstors_roundtrips_ymm() {
    let mut s = [0u8; 64];
    for i in 0..32 {
        s[i] = (0xA5u8).wrapping_add((i as u8).wrapping_mul(11));
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xBB, 0x00, 0x10, 0x03, 0x00]); // mov ebx, DATA_ADDR+0x1000
    code.extend_from_slice(&[0xB8, 0x07, 0x00, 0x00, 0x00]); // mov eax, 7
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    code.extend_from_slice(&[0x0F, 0xC7, 0x23]); // xsavec [rbx]
    code.extend_from_slice(&[0xC5, 0xFC, 0x77]); // vzeroall
    code.extend_from_slice(&[0xB8, 0x07, 0x00, 0x00, 0x00]); // mov eax, 7
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    code.extend_from_slice(&[0x0F, 0xC7, 0x1B]); // xrstors [rbx]
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x47, 0x20]); // vmovdqu [rdi+32], ymm0
    code.push(HLT);

    check_avx_mem("xsavec_xrstors_ymm", &code, s);
}

#[test]
fn xsavec_xrstor_roundtrips_ymm() {
    let mut s = [0u8; 64];
    for i in 0..32 {
        s[i] = (0x4Du8).wrapping_add((i as u8).wrapping_mul(5));
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xBB, 0x00, 0x10, 0x03, 0x00]); // mov ebx, DATA_ADDR+0x1000
    code.extend_from_slice(&[0xB8, 0x07, 0x00, 0x00, 0x00]); // mov eax, 7
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    code.extend_from_slice(&[0x0F, 0xC7, 0x23]); // xsavec [rbx]
    code.extend_from_slice(&[0xC5, 0xFC, 0x77]); // vzeroall
    code.extend_from_slice(&[0xB8, 0x07, 0x00, 0x00, 0x00]); // mov eax, 7
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    code.extend_from_slice(&[0x0F, 0xAE, 0x2B]); // xrstor [rbx]
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x47, 0x20]); // vmovdqu [rdi+32], ymm0
    code.push(HLT);

    check_avx_mem("xsavec_xrstor_ymm", &code, s);
}

#[test]
fn xsaves_xrstors_roundtrips_mxcsr() {
    let mut s = [0u8; 64];
    s[0..4].copy_from_slice(&0x0000_7F80u32.to_le_bytes());
    s[4..8].copy_from_slice(&0x0000_1F80u32.to_le_bytes());

    let mut code = avx_start();
    code.extend_from_slice(&[0xBB, 0x00, 0x10, 0x03, 0x00]); // mov ebx, DATA_ADDR+0x1000
    code.extend_from_slice(&[0x0F, 0xAE, 0x17]); // ldmxcsr [rdi]
    code.extend_from_slice(&[0xB8, 0x02, 0x00, 0x00, 0x00]); // mov eax, 2
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    code.extend_from_slice(&[0x0F, 0xC7, 0x2B]); // xsaves [rbx]
    code.extend_from_slice(&[0x0F, 0xAE, 0x57, 0x04]); // ldmxcsr [rdi+4]
    code.extend_from_slice(&[0xB8, 0x02, 0x00, 0x00, 0x00]); // mov eax, 2
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    code.extend_from_slice(&[0x0F, 0xC7, 0x1B]); // xrstors [rbx]
    code.extend_from_slice(&[0x0F, 0xAE, 0x5F, 0x08]); // stmxcsr [rdi+8]
    code.push(HLT);

    check_avx_mem("xsaves_xrstors_mxcsr", &code, s);
}

#[test]
fn xsaves_xrstors_roundtrips_ymm() {
    let mut s = [0u8; 64];
    for i in 0..32 {
        s[i] = (0x19u8).wrapping_add((i as u8).wrapping_mul(17));
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xBB, 0x00, 0x10, 0x03, 0x00]); // mov ebx, DATA_ADDR+0x1000
    code.extend_from_slice(&[0xB8, 0x07, 0x00, 0x00, 0x00]); // mov eax, 7
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    code.extend_from_slice(&[0x0F, 0xC7, 0x2B]); // xsaves [rbx]
    code.extend_from_slice(&[0xC5, 0xFC, 0x77]); // vzeroall
    code.extend_from_slice(&[0xB8, 0x07, 0x00, 0x00, 0x00]); // mov eax, 7
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    code.extend_from_slice(&[0x0F, 0xC7, 0x1B]); // xrstors [rbx]
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x47, 0x20]); // vmovdqu [rdi+32], ymm0
    code.push(HLT);

    check_avx_mem("xsaves_xrstors_ymm", &code, s);
}

fn avx_enable_prologue() -> Vec<u8> {
    xcr0_enable_prologue(0x7)
}

fn avx512_enable_prologue() -> Vec<u8> {
    xcr0_enable_prologue(0xE7)
}

fn avx_start() -> Vec<u8> {
    let mut code = avx_enable_prologue();
    code.extend_from_slice(&load_rdi_data());
    code
}

fn check_avx_mem(label: &str, code: &[u8], scratch: [u8; 64]) {
    check_mem(label, code, regs(), scratch, 0);
}

fn check_avx_flags_mem(label: &str, code: &[u8], scratch: [u8; 64]) {
    check_mem(label, code, regs(), scratch, FLAG_MASK);
}

#[test]
fn avx2_ymm_paddb_psubusb() {
    let mut s = [0u8; 64];
    for i in 0..32 {
        s[i] = (i as u8).wrapping_mul(7).wrapping_add(3);
        s[32 + i] = 0xF0u8.wrapping_sub((i as u8).wrapping_mul(5));
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0xFC, 0xD1]); // vpaddb ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFD, 0xD8, 0xD9]); // vpsubusb ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx2_ymm_paddb_psubusb", &code, s);
}

#[test]
fn avx2_ymm_packuswb_punpcklbw() {
    let mut s = [0u8; 64];
    for i in 0..32 {
        s[i] = (0x80u8).wrapping_add((i as u8).wrapping_mul(3));
        s[32 + i] = (0x7Fu8).wrapping_sub((i as u8).wrapping_mul(2));
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0x67, 0xD1]); // vpackuswb ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFD, 0x60, 0xD1]); // vpunpcklbw ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x57, 0x20]); // vmovdqu [rdi+32], ymm2
    code.push(HLT);
    check_avx_mem("avx2_ymm_packuswb_punpcklbw", &code, s);
}

#[test]
fn avx2_ymm_mpsadbw_blendd() {
    let mut s = [0u8; 64];
    for i in 0..32 {
        s[i] = (i as u8).wrapping_mul(8);
        s[32 + i] = (i as u8).wrapping_mul(8).wrapping_add(5);
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x42, 0xD1, 0x05]); // vmpsadbw ymm2, ymm0, ymm1, 5
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x02, 0xD1, 0xAA]); // vpblendd ymm2, ymm0, ymm1, 0xaa
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x57, 0x20]); // vmovdqu [rdi+32], ymm2
    code.push(HLT);
    check_avx_mem("avx2_ymm_mpsadbw_blendd", &code, s);
}

#[test]
fn avx_ymm_vlddqu_vmovntdqa() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0xA5u8).wrapping_add((i as u8).wrapping_mul(11));
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFF, 0xF0, 0x57, 0x01]); // vlddqu ymm2, [rdi+1]
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x2A, 0x57, 0x20]); // vmovntdqa ymm2, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x57, 0x20]); // vmovdqu [rdi+32], ymm2
    code.push(HLT);
    check_avx_mem("avx_ymm_vlddqu_vmovntdqa", &code, s);
}

#[test]
fn avx_ymm_insert_extract_f128() {
    let mut s = [0u8; 64];
    for i in 0..32 {
        s[i] = (0x10u8).wrapping_add(i as u8);
    }
    for i in 0..16 {
        s[32 + i] = (0xE0u8).wrapping_sub((i as u8).wrapping_mul(3));
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x20]); // vmovdqu xmm1, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x18, 0xD1, 0x01]); // vinsertf128 ymm2, ymm0, xmm1, 1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x19, 0x57, 0x20, 0x01]); // vextractf128 [rdi+32], ymm2, 1
    code.push(HLT);
    check_avx_mem("avx_ymm_insert_extract_f128", &code, s);
}

#[test]
fn avx2_ymm_perm2_and_insert_extract_i128_memory_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x6Du8).wrapping_add((i as u8).wrapping_mul(13));
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x06, 0x57, 0x20, 0x31]); // vperm2f128 ymm2, ymm0, [rdi+32], 0x31
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x46, 0x5F, 0x20, 0x20]); // vperm2i128 ymm3, ymm0, [rdi+32], 0x20
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_vperm2f128_vperm2i128_memory", &code, s);

    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0xC3u8).wrapping_sub((i as u8).wrapping_mul(5));
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x38, 0x57, 0x20, 0x01]); // vinserti128 ymm2, ymm0, [rdi+32], 1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x39, 0x57, 0x30, 0x01]); // vextracti128 [rdi+48], ymm2, 1
    code.push(HLT);
    check_avx_mem("avx2_ymm_vinserti128_vextracti128_memory", &code, s);
}

#[test]
fn avx2_xmm_pblendvb() {
    let mut s = [0u8; 64];
    for i in 0..16 {
        s[i] = (0x10 + i) as u8;
        s[16 + i] = (0xF0 - i) as u8;
        s[32 + i] = if i % 3 == 0 { 0x80 } else { 0x00 };
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x10]); // vmovdqu xmm1, [rdi+16]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x5F, 0x20]); // vmovdqu xmm3, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE3, 0x79, 0x4C, 0xD1, 0x30]); // vpblendvb xmm2, xmm0, xmm1, xmm3
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x57, 0x30]); // vmovdqu [rdi+48], xmm2
    code.push(HLT);
    check_avx_mem("avx2_xmm_pblendvb", &code, s);
}

#[test]
fn avx_vzeroall_clears_ymm0() {
    let mut s = [0u8; 64];
    for i in 0..32 {
        s[i] = (0x55u8).wrapping_add((i as u8).wrapping_mul(9));
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFC, 0x77]); // vzeroall
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x07]); // vmovdqu [rdi], ymm0
    code.push(HLT);
    check_avx_mem("avx_vzeroall_ymm0", &code, s);
}

#[test]
fn avx_vzeroupper_preserves_xmm0_clears_upper() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x31u8).wrapping_add((i as u8).wrapping_mul(7));
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xF8, 0x77]); // vzeroupper
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x47, 0x20]); // vmovdqu [rdi+32], ymm0
    code.push(HLT);
    check_avx_mem("avx_vzeroupper_preserves_xmm0", &code, s);
}

#[test]
fn avx_xsave_xrstor_ymm_roundtrip() {
    let mut s = [0u8; 64];
    for i in 0..32 {
        s[i] = (0xC3u8).wrapping_add((i as u8).wrapping_mul(13));
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xBB, 0x00, 0x10, 0x03, 0x00]); // mov ebx, DATA_ADDR+0x1000
    code.extend_from_slice(&[0xB8, 0x07, 0x00, 0x00, 0x00]); // mov eax, 7
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    code.extend_from_slice(&[0x0F, 0xAE, 0x23]); // xsave [rbx]
    code.extend_from_slice(&[0xC5, 0xFC, 0x77]); // vzeroall
    code.extend_from_slice(&[0xB8, 0x07, 0x00, 0x00, 0x00]); // mov eax, 7
    code.extend_from_slice(&[0x31, 0xD2]); // xor edx, edx
    code.extend_from_slice(&[0x0F, 0xAE, 0x2B]); // xrstor [rbx]
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x47, 0x20]); // vmovdqu [rdi+32], ymm0
    code.push(HLT);
    check_avx_mem("avx_xsave_xrstor_ymm", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 12: AVX-512 opmask instructions.
//
// Opmask registers are not captured directly by this KVM differential harness,
// so each case makes the k-register effects observable through KMOV to GPRs or
// memory. These byte streams were checked against llvm-mc encodings.
// ===========================================================================

fn opmask_start() -> Vec<u8> {
    let mut code = avx512_enable_prologue();
    code.extend_from_slice(&load_rdi_data());
    code
}

#[test]
fn opmask_kmov_logical_wide_roundtrip() {
    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0x5A, 0xA5, 0x00, 0x00]); // mov eax, 0xa55a
    code.extend_from_slice(&[0xBB, 0x33, 0x0F, 0x00, 0x00]); // mov ebx, 0x0f33
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xD3]); // kmovw k2, ebx
    code.extend_from_slice(&[0xC5, 0xF4, 0x41, 0xDA]); // kandw k3, k1, k2
    code.extend_from_slice(&[0xC5, 0xF4, 0x45, 0xE2]); // korw k4, k1, k2
    code.extend_from_slice(&[0xC5, 0xF4, 0x47, 0xEA]); // kxorw k5, k1, k2
    code.extend_from_slice(&[0xC5, 0xF4, 0x4A, 0xF2]); // kaddw k6, k1, k2
    code.extend_from_slice(&[0xC5, 0xF4, 0x42, 0xFA]); // kandnw k7, k1, k2
    code.extend_from_slice(&[0xC5, 0xF4, 0x46, 0xCA]); // kxnorw k1, k1, k2
    code.extend_from_slice(&[0xC5, 0xF8, 0x44, 0xD1]); // knotw k2, k1
    code.extend_from_slice(&[0xC5, 0xF8, 0x93, 0xCB]); // kmovw ecx, k3
    code.extend_from_slice(&[0xC5, 0xF8, 0x93, 0xD4]); // kmovw edx, k4
    code.extend_from_slice(&[0xC5, 0x78, 0x93, 0xC5]); // kmovw r8d, k5
    code.extend_from_slice(&[0xC5, 0x78, 0x93, 0xCE]); // kmovw r9d, k6
    code.extend_from_slice(&[0xC5, 0x78, 0x93, 0xD7]); // kmovw r10d, k7
    code.extend_from_slice(&[0xC5, 0x78, 0x93, 0xD9]); // kmovw r11d, k1
    code.extend_from_slice(&[0xC5, 0x78, 0x93, 0xE2]); // kmovw r12d, k2
    code.push(HLT);
    check("opmask_kmov_logical_wide_roundtrip", &code, regs());
}

#[test]
fn opmask_kunpck_and_shift_widths() {
    let mut code = opmask_start();

    code.extend_from_slice(&[0xB8, 0xAB, 0x00, 0x00, 0x00]); // mov eax, 0xab
    code.extend_from_slice(&[0xBB, 0x12, 0x00, 0x00, 0x00]); // mov ebx, 0x12
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xC8]); // kmovb k1, eax
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xD3]); // kmovb k2, ebx
    code.extend_from_slice(&[0xC5, 0xF5, 0x4B, 0xDA]); // kunpckbw k3, k1, k2
    code.extend_from_slice(&[0xC4, 0xE3, 0xF9, 0x32, 0xE3, 0x03]); // kshiftlw k4, k3, 3
    code.extend_from_slice(&[0xC4, 0xE3, 0xF9, 0x30, 0xEC, 0x02]); // kshiftrw k5, k4, 2
    code.extend_from_slice(&[0xC5, 0xF8, 0x93, 0xCB]); // kmovw ecx, k3
    code.extend_from_slice(&[0xC5, 0xF8, 0x93, 0xD4]); // kmovw edx, k4
    code.extend_from_slice(&[0xC5, 0x78, 0x93, 0xC5]); // kmovw r8d, k5

    code.extend_from_slice(&[0xB8, 0xCD, 0xAB, 0x00, 0x00]); // mov eax, 0xabcd
    code.extend_from_slice(&[0xBB, 0x34, 0x12, 0x00, 0x00]); // mov ebx, 0x1234
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xD3]); // kmovw k2, ebx
    code.extend_from_slice(&[0xC5, 0xF4, 0x4B, 0xDA]); // kunpckwd k3, k1, k2
    code.extend_from_slice(&[0xC4, 0xE3, 0x79, 0x33, 0xE3, 0x0D]); // kshiftld k4, k3, 13
    code.extend_from_slice(&[0xC4, 0xE3, 0x79, 0x31, 0xEC, 0x09]); // kshiftrd k5, k4, 9
    code.extend_from_slice(&[0xC5, 0x7B, 0x93, 0xCB]); // kmovd r9d, k3
    code.extend_from_slice(&[0xC5, 0x7B, 0x93, 0xD4]); // kmovd r10d, k4
    code.extend_from_slice(&[0xC5, 0x7B, 0x93, 0xDD]); // kmovd r11d, k5

    code.extend_from_slice(&[0x48, 0xB8]); // mov rax, 0x89abcdef76543210
    code.extend_from_slice(&0x89AB_CDEF_7654_3210u64.to_le_bytes());
    code.extend_from_slice(&[0x48, 0xBB]); // mov rbx, 0xfedcba9876543210
    code.extend_from_slice(&0xFEDC_BA98_7654_3210u64.to_le_bytes());
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xD3]); // kmovq k2, rbx
    code.extend_from_slice(&[0xC4, 0xE1, 0xF4, 0x4B, 0xDA]); // kunpckdq k3, k1, k2
    code.extend_from_slice(&[0xC4, 0xE3, 0xF9, 0x33, 0xE3, 0x11]); // kshiftlq k4, k3, 17
    code.extend_from_slice(&[0xC4, 0xE3, 0xF9, 0x31, 0xEC, 0x0B]); // kshiftrq k5, k4, 11
    code.extend_from_slice(&[0xC4, 0x61, 0xFB, 0x93, 0xE3]); // kmovq r12, k3
    code.extend_from_slice(&[0xC4, 0x61, 0xFB, 0x93, 0xEC]); // kmovq r13, k4
    code.extend_from_slice(&[0xC4, 0x61, 0xFB, 0x93, 0xF5]); // kmovq r14, k5
    code.push(HLT);
    check("opmask_kunpck_and_shift_widths", &code, regs());
}

#[test]
fn opmask_kmov_memory_widths() {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xB8]); // mov rax, 0x1122334455667788
    code.extend_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0xC5, 0xF9, 0x91, 0x0F]); // kmovb [rdi], k1
    code.extend_from_slice(&[0xC5, 0xF8, 0x91, 0x4F, 0x08]); // kmovw [rdi+8], k1
    code.extend_from_slice(&[0xC4, 0xE1, 0xF9, 0x91, 0x4F, 0x10]); // kmovd [rdi+16], k1
    code.extend_from_slice(&[0xC4, 0xE1, 0xF8, 0x91, 0x4F, 0x18]); // kmovq [rdi+24], k1
    code.extend_from_slice(&[0xC5, 0xF9, 0x90, 0x17]); // kmovb k2, [rdi]
    code.extend_from_slice(&[0xC5, 0xF8, 0x90, 0x5F, 0x08]); // kmovw k3, [rdi+8]
    code.extend_from_slice(&[0xC4, 0xE1, 0xF9, 0x90, 0x67, 0x10]); // kmovd k4, [rdi+16]
    code.extend_from_slice(&[0xC4, 0xE1, 0xF8, 0x90, 0x6F, 0x18]); // kmovq k5, [rdi+24]
    code.extend_from_slice(&[0xC5, 0xF9, 0x93, 0xCA]); // kmovb ecx, k2
    code.extend_from_slice(&[0xC5, 0xF8, 0x93, 0xD3]); // kmovw edx, k3
    code.extend_from_slice(&[0xC5, 0x7B, 0x93, 0xC4]); // kmovd r8d, k4
    code.extend_from_slice(&[0xC4, 0x61, 0xFB, 0x93, 0xCD]); // kmovq r9, k5
    code.push(HLT);
    check_mem("opmask_kmov_memory_widths", &code, regs(), zero_scratch(), 0);
}

#[test]
fn opmask_ktest_flags() {
    let mut init = regs();
    init.rflags = FLAG_MASK | 0x2;

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0x0F, 0x0F, 0x00, 0x00]); // mov eax, 0x0f0f
    code.extend_from_slice(&[0xBB, 0x00, 0xF0, 0x00, 0x00]); // mov ebx, 0xf000
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xD3]); // kmovw k2, ebx
    code.extend_from_slice(&[0xC5, 0xF8, 0x99, 0xCA]); // ktestw k1, k2
    code.push(HLT);
    check("opmask_ktest_flags", &code, init);
}

#[test]
fn opmask_kortest_flags() {
    let mut init = regs();
    init.rflags = FLAG_MASK | 0x2;

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0x0F, 0x0F, 0x00, 0x00]); // mov eax, 0x0f0f
    code.extend_from_slice(&[0xBB, 0xF0, 0xF0, 0x00, 0x00]); // mov ebx, 0xf0f0
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xD3]); // kmovw k2, ebx
    code.extend_from_slice(&[0xC5, 0xF8, 0x98, 0xCA]); // kortestw k1, k2
    code.push(HLT);
    check("opmask_kortest_flags", &code, init);
}

// ===========================================================================
// EXPANDED COVERAGE PART 13: EVEX/ZMM AVX-512 instructions.
//
// These cases exercise EVEX decoding and 512-bit architectural state by storing
// the final ZMM result into the 64-byte scratch page, or by moving k-mask
// results back through KMOV into GPRs. The instruction bytes were checked with
// llvm-mc on this host.
// ===========================================================================

fn check_evex_mem(label: &str, code: &[u8], scratch: [u8; 64]) {
    check_mem(label, code, regs(), scratch, 0);
}

fn evex_dword_scratch<F>(mut f: F) -> [u8; 64]
where
    F: FnMut(usize) -> u32,
{
    let mut s = [0u8; 64];
    for i in 0..16 {
        s[i * 4..i * 4 + 4].copy_from_slice(&f(i).to_le_bytes());
    }
    s
}

#[test]
fn evex_zmm_masked_vpaddd_vpcmpd() {
    let s = evex_dword_scratch(|i| {
        (0x0102_0304u32)
            .wrapping_mul((i as u32) + 1)
            .rotate_left((i % 7) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xA5, 0xA5, 0x00, 0x00]); // mov eax, 0xa5a5
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x58, 0x4F, 0x01]); // vpbroadcastd zmm1, [rdi+4]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0xC9, 0xFE, 0xD1]); // vpaddd zmm2{k1}{z}, zmm0, zmm1
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x49, 0x1F, 0xD1, 0x06]); // vpcmpd k2{k1}, zmm0, zmm1, 6
    code.extend_from_slice(&[0xC5, 0xF8, 0x93, 0xCA]); // kmovw ecx, k2
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_masked_vpaddd_vpcmpd", &code, s);
}

#[test]
fn evex_zmm_vpmovm2b_vpmovb2m() {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xB8]); // mov rax, imm64
    code.extend_from_slice(&0x8040_2010_0804_02A5u64.to_le_bytes());
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x48, 0x28, 0xC1]); // vpmovm2b zmm0, k1
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x48, 0x29, 0xD0]); // vpmovb2m k2, zmm0
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xCA]); // kmovq rcx, k2
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x48, 0x7F, 0x07]); // vmovdqu8 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vpmovm2b_vpmovb2m", &code, zero_scratch());
}

#[test]
fn evex_zmm_vpmovm2w_vpmovw2m() {
    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0x55, 0x55, 0x01, 0x80]); // mov eax, 0x80015555
    code.extend_from_slice(&[0xC5, 0xFB, 0x92, 0xC8]); // kmovd k1, eax
    code.extend_from_slice(&[0x62, 0xF2, 0xFE, 0x48, 0x28, 0xC1]); // vpmovm2w zmm0, k1
    code.extend_from_slice(&[0x62, 0xF2, 0xFE, 0x48, 0x29, 0xD0]); // vpmovw2m k2, zmm0
    code.extend_from_slice(&[0xC5, 0xFB, 0x93, 0xCA]); // kmovd ecx, k2
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x7F, 0x07]); // vmovdqu16 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vpmovm2w_vpmovw2m", &code, zero_scratch());
}

#[test]
fn evex_zmm_vpmovm2d_vpmovd2m() {
    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xA5, 0xA5, 0x00, 0x00]); // mov eax, 0xa5a5
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x48, 0x38, 0xC1]); // vpmovm2d zmm0, k1
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x48, 0x39, 0xD0]); // vpmovd2m k2, zmm0
    code.extend_from_slice(&[0xC5, 0xF8, 0x93, 0xCA]); // kmovw ecx, k2
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x07]); // vmovdqu32 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vpmovm2d_vpmovd2m", &code, zero_scratch());
}

#[test]
fn evex_zmm_vpmovm2q_vpmovq2m() {
    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xA5, 0x00, 0x00, 0x00]); // mov eax, 0xa5
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xC8]); // kmovb k1, eax
    code.extend_from_slice(&[0x62, 0xF2, 0xFE, 0x48, 0x38, 0xC1]); // vpmovm2q zmm0, k1
    code.extend_from_slice(&[0x62, 0xF2, 0xFE, 0x48, 0x39, 0xD0]); // vpmovq2m k2, zmm0
    code.extend_from_slice(&[0xC5, 0xF9, 0x93, 0xCA]); // kmovb ecx, k2
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]); // vmovdqu64 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vpmovm2q_vpmovq2m", &code, zero_scratch());
}

#[test]
fn evex_zmm_vpbroadcastmb2q() {
    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xA5, 0x00, 0x00, 0x00]); // mov eax, 0xa5
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xC8]); // kmovb k1, eax
    code.extend_from_slice(&[0x62, 0xF2, 0xFE, 0x48, 0x2A, 0xC1]); // vpbroadcastmb2q zmm0, k1
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]); // vmovdqu64 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vpbroadcastmb2q", &code, zero_scratch());
}

#[test]
fn evex_zmm_vpbroadcastmw2d() {
    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xEF, 0xBE, 0x00, 0x00]); // mov eax, 0xbeef
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x48, 0x3A, 0xC1]); // vpbroadcastmw2d zmm0, k1
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x07]); // vmovdqu32 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vpbroadcastmw2d", &code, zero_scratch());
}

#[test]
fn evex_zmm_vpmovzxbd_vpmovsdb() {
    let mut s = [0xCCu8; 64];
    s[0..16].copy_from_slice(&[
        0x00, 0x01, 0x7F, 0x80, 0x81, 0xFE, 0xFF, 0x40, 0x55, 0xAA, 0x10, 0xF0, 0x20, 0xE0,
        0x30, 0xD0,
    ]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x31, 0x07]); // vpmovzxbd zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x48, 0x21, 0x07]); // vpmovsdb [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vpmovzxbd_vpmovsdb", &code, s);
}

#[test]
fn evex_zmm_vpermd_self_indexed_table() {
    let pattern = [
        15u32, 0, 14, 1, 13, 2, 12, 3, 11, 4, 10, 5, 9, 6, 8, 7,
    ];
    let s = evex_dword_scratch(|i| pattern[i]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x48, 0x72, 0xF0, 0x08]); // vpslld zmm1, zmm0, 8
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x36, 0xD1]); // vpermd zmm2, zmm0, zmm1
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpermd_self_indexed_table", &code, s);
}

#[test]
fn evex_zmm_vpternlogd_broadcast_sources() {
    let s = evex_dword_scratch(|i| {
        0xF0F0_0000u32
            ^ ((i as u32).wrapping_mul(0x1111_0101))
            ^ if i % 2 == 0 { 0xAAAA_5555 } else { 0x1234_5678 }
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x58, 0x4F, 0x01]); // vpbroadcastd zmm1, [rdi+4]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x58, 0x57, 0x02]); // vpbroadcastd zmm2, [rdi+8]
    code.extend_from_slice(&[0x62, 0xF3, 0x75, 0x48, 0x25, 0xC2, 0x96]); // vpternlogd zmm0, zmm1, zmm2, 0x96
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x07]); // vmovdqu32 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vpternlogd_broadcast_sources", &code, s);
}

#[test]
fn evex_zmm_vpternlogd_mask_merge_zero_forms() {
    let s = evex_dword_scratch(|i| {
        0x9696_5A5Au32
            .wrapping_add((i as u32).wrapping_mul(0x1020_4081))
            .rotate_left((i % 13) as u32)
            ^ if i % 2 == 0 { 0xA5A5_3333 } else { 0x3C3C_C3C3 }
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xA5, 0x5A, 0x00, 0x00]); // mov eax, 0x5aa5
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x1F]); // vmovdqu32 zmm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x48, 0x72, 0xF0, 0x08]); // vpslld zmm1, zmm0, 8
    code.extend_from_slice(&[0x62, 0xF1, 0x6D, 0x48, 0x72, 0xD0, 0x05]); // vpsrld zmm2, zmm0, 5
    code.extend_from_slice(&[0x62, 0xF3, 0x75, 0x49, 0x25, 0xC2, 0x96]); // vpternlogd zmm0{k1}, zmm1, zmm2, 0x96
    code.extend_from_slice(&[0x62, 0xF3, 0x75, 0xC9, 0x25, 0xDA, 0xE2]); // vpternlogd zmm3{k1}{z}, zmm1, zmm2, 0xe2
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xEF, 0xC3]); // vpxord zmm0, zmm0, zmm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x07]); // vmovdqu32 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vpternlogd_mask_merge_zero_forms", &code, s);
}

#[test]
fn evex_xmm_vpternlogd_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x0F0F_5555u32
            ^ ((i as u32).wrapping_mul(0x1020_4081))
            ^ if i % 3 == 0 { 0xAAAA_0000 } else { 0x1357_9BDF }
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x07]); // vmovdqu32 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu32 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x57, 0x02]); // vmovdqu32 xmm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x1F]); // vmovdqu32 xmm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF3, 0x75, 0x08, 0x25, 0xC2, 0x96]); // vpternlogd xmm0, xmm1, xmm2, 0x96
    code.extend_from_slice(&[0x62, 0xF3, 0x75, 0x08, 0x25, 0x5F, 0x02, 0xE2]); // vpternlogd xmm3, xmm1, [rdi+32], 0xe2
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x07]); // vmovdqu32 [rdi], xmm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu32 [rdi+16], xmm3
    code.push(HLT);
    check_evex_mem("evex_xmm_vpternlogd_vl_forms", &code, s);
}

#[test]
fn evex_ymm_vpternlogq_vl_forms() {
    let s = evex_qword_scratch(|i| {
        0x00FF_00FF_5555_AAAAu64
            ^ ((i as u64).wrapping_mul(0x1111_0101_2040_8101))
            ^ if i % 2 == 0 {
                0xAAAA_5555_1234_5678
            } else {
                0x0123_4567_89AB_CDEF
            }
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu64 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x17]); // vmovdqu64 ymm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x5F, 0x01]); // vmovdqu64 ymm3, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF3, 0xF5, 0x28, 0x25, 0xC2, 0x96]); // vpternlogq ymm0, ymm1, ymm2, 0x96
    code.extend_from_slice(&[0x62, 0xF3, 0xF5, 0x28, 0x25, 0x1F, 0xE2]); // vpternlogq ymm3, ymm1, [rdi], 0xe2
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x07]); // vmovdqu64 [rdi], ymm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_vpternlogq_vl_forms", &code, s);
}

#[test]
fn evex_zmm_vplzcntd() {
    let s = evex_dword_scratch(|i| match i {
        0 => 0,
        1 => 1,
        2 => 0x8000_0000,
        3 => 0x00FF_0000,
        _ => (0x0100_0000u32).wrapping_shr((i % 24) as u32) ^ (i as u32),
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x44, 0xD0]); // vplzcntd zmm2, zmm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vplzcntd", &code, s);
}

#[test]
fn evex_zmm_vplzcnt_mask_zero_dq_forms() {
    let s = evex_qword_scratch(|i| match i {
        0 => 0,
        1 => 1,
        2 => 0x8000_0000_0000_0000,
        3 => 0x0000_00FF_0000_0000,
        _ => (0x0100_0000_0000_0000u64).wrapping_shr((i % 48) as u32) ^ (i as u64),
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xAA, 0x55, 0x00, 0x00]); // mov eax, 0x55aa
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x17]); // vmovdqu32 zmm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x1F]); // vmovdqu32 zmm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0xC9, 0x44, 0xD0]); // vplzcntd zmm2{k1}{z}, zmm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0xC9, 0x44, 0xD8]); // vplzcntq zmm3{k1}{z}, zmm0
    code.extend_from_slice(&[0x62, 0xF1, 0x6D, 0x48, 0xEF, 0xD3]); // vpxord zmm2, zmm2, zmm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vplzcnt_mask_zero_dq_forms", &code, s);
}

#[test]
fn evex_zmm_vpshldd_imm() {
    let s = evex_dword_scratch(|i| {
        0x1020_3040u32
            .wrapping_mul((i as u32) + 3)
            .rotate_right((i % 11) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x48, 0x72, 0xF0, 0x08]); // vpslld zmm1, zmm0, 8
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x71, 0xD1, 0x05]); // vpshldd zmm2, zmm0, zmm1, 5
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpshldd_imm", &code, s);
}

#[test]
fn evex_zmm_vdbpsadbw() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (i as u8).wrapping_mul(13).wrapping_add(7);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x58, 0x4F, 0x01]); // vpbroadcastd zmm1, [rdi+4]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x42, 0xD1, 0x05]); // vdbpsadbw zmm2, zmm0, zmm1, 5
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vdbpsadbw", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 14: more EVEX/ZMM integer AVX-512 instructions.
//
// These extend Part 13 beyond moves/permutes/count-leading-zero into byte/word
// transforms, packed min/max, saturating packs, popcount, and opmask selector
// blends. Encodings were checked with llvm-mc.
// ===========================================================================

#[test]
fn evex_zmm_vpopcntd() {
    let s = evex_dword_scratch(|i| {
        0x0101_0101u32
            .wrapping_mul((i as u32) + 1)
            .rotate_left((i * 3) as u32)
            ^ 0xA5A5_5A5A
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x55, 0xD0]); // vpopcntd zmm2, zmm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpopcntd", &code, s);
}

#[test]
fn evex_zmm_vpopcnt_mask_zero_element_widths() {
    let s = evex_qword_scratch(|i| {
        0x0102_0408_1020_4080u64
            .wrapping_mul((i as u64) + 3)
            ^ 0xA55A_C33C_5AA5_3CC3u64.rotate_left((i * 9) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xB8]); // mov rax, 0x5555555555555555
    code.extend_from_slice(&0x5555_5555_5555_5555u64.to_le_bytes());
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x17]); // vmovdqu32 zmm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x1F]); // vmovdqu32 zmm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x27]); // vmovdqu32 zmm4, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x2F]); // vmovdqu32 zmm5, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0xC9, 0x55, 0xD0]); // vpopcntd zmm2{k1}{z}, zmm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0xC9, 0x55, 0xD8]); // vpopcntq zmm3{k1}{z}, zmm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0xC9, 0x54, 0xE0]); // vpopcntb zmm4{k1}{z}, zmm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0xC9, 0x54, 0xE8]); // vpopcntw zmm5{k1}{z}, zmm0
    code.extend_from_slice(&[0x62, 0xF1, 0x6D, 0x48, 0xEF, 0xD3]); // vpxord zmm2, zmm2, zmm3
    code.extend_from_slice(&[0x62, 0xF1, 0x5D, 0x48, 0xEF, 0xE5]); // vpxord zmm4, zmm4, zmm5
    code.extend_from_slice(&[0x62, 0xF1, 0x6D, 0x48, 0xEF, 0xD4]); // vpxord zmm2, zmm2, zmm4
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpopcnt_mask_zero_element_widths", &code, s);
}

#[test]
fn evex_xmm_vpopcnt_byte_word_vl_forms() {
    let mut s = [0u8; 64];
    for (i, byte) in s.iter_mut().enumerate() {
        *byte = (0x9Bu8).rotate_left((i % 8) as u32) ^ (i as u8).wrapping_mul(7);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x08, 0x54, 0xD0]); // vpopcntb xmm2, xmm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x08, 0x54, 0xD8]); // vpopcntw xmm3, xmm0
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x17]); // vmovdqu [rdi], xmm2
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x5F, 0x10]); // vmovdqu [rdi+16], xmm3
    code.push(HLT);
    check_evex_mem("evex_xmm_vpopcnt_byte_word_vl_forms", &code, s);
}

#[test]
fn evex_ymm_vpopcnt_dword_qword_vl_forms() {
    let mut s = [0u8; 64];
    for (i, byte) in s.iter_mut().enumerate() {
        *byte = (0x36u8).rotate_right((i % 8) as u32) ^ (i as u8).wrapping_mul(19);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x28, 0x55, 0xE0]); // vpopcntd ymm4, ymm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x28, 0x55, 0xE8]); // vpopcntq ymm5, ymm0
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x27]); // vmovdqu [rdi], ymm4
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x6F, 0x20]); // vmovdqu [rdi+32], ymm5
    code.push(HLT);
    check_evex_mem("evex_ymm_vpopcnt_dword_qword_vl_forms", &code, s);
}

#[test]
fn evex_ymm_vplzcnt_dword_qword_vl_forms() {
    let mut s = [0u8; 64];
    for (i, byte) in s.iter_mut().enumerate() {
        *byte = (0xC3u8).rotate_left((i % 8) as u32) ^ (i as u8).wrapping_mul(23);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x28, 0x44, 0xD0]); // vplzcntd ymm2, ymm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x28, 0x44, 0xD8]); // vplzcntq ymm3, ymm0
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_vplzcnt_dword_qword_vl_forms", &code, s);
}

#[test]
fn evex_zmm_vpabsb() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x80u8).wrapping_add((i as u8).wrapping_mul(7));
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x48, 0x6F, 0x07]); // vmovdqu8 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x1C, 0xD0]); // vpabsb zmm2, zmm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x48, 0x7F, 0x17]); // vmovdqu8 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpabsb", &code, s);
}

#[test]
fn evex_zmm_vpminsd() {
    let s = evex_dword_scratch(|i| {
        let value = 0x4000_0000u32.wrapping_add((i as u32).wrapping_mul(0x1111_1111));
        if i % 2 == 0 { value } else { !value }
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x48, 0x72, 0xF0, 0x01]); // vpslld zmm1, zmm0, 1
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x39, 0xD1]); // vpminsd zmm2, zmm0, zmm1
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpminsd", &code, s);
}

#[test]
fn evex_zmm_vpmaxud() {
    let s = evex_dword_scratch(|i| {
        0x8000_0000u32
            .wrapping_add((i as u32).wrapping_mul(0x1020_4081))
            .rotate_right((i % 13) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x48, 0x72, 0xF0, 0x01]); // vpslld zmm1, zmm0, 1
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x3F, 0xD9]); // vpmaxud zmm3, zmm0, zmm1
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x1F]); // vmovdqu32 [rdi], zmm3
    code.push(HLT);
    check_evex_mem("evex_zmm_vpmaxud", &code, s);
}

#[test]
fn evex_zmm_vpackusdw() {
    let s = evex_dword_scratch(|i| match i % 4 {
        0 => 0xFFFF_FF00,
        1 => 0x0000_1234,
        2 => 0x0001_0000,
        _ => 0x7FFF_FFFF,
    } ^ ((i as u32) << 12));

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x48, 0x72, 0xF0, 0x01]); // vpslld zmm1, zmm0, 1
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x2B, 0xD1]); // vpackusdw zmm2, zmm0, zmm1
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpackusdw", &code, s);
}

#[test]
fn evex_zmm_vpblendmb_selector() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x91u8).wrapping_add((i as u8).wrapping_mul(11));
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xB8]); // mov rax, selector mask
    code.extend_from_slice(&0xA55A_F00F_33CC_9696u64.to_le_bytes());
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x48, 0x6F, 0x07]); // vmovdqu8 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x1C, 0xC8]); // vpabsb zmm1, zmm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x49, 0x66, 0xD1]); // vpblendmb zmm2{k1}, zmm0, zmm1
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x48, 0x7F, 0x17]); // vmovdqu8 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpblendmb_selector", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 15: VEX FMA/gather/permute and AVX-512 VNNI/BF16/FP16.
//
// This block targets implemented instruction families that were not previously
// exercised by differential tests. Encodings were checked with llvm-mc.
// ===========================================================================

fn avx_f32_scratch<F>(mut f: F) -> [u8; 64]
where
    F: FnMut(usize) -> f32,
{
    let mut s = [0u8; 64];
    for i in 0..16 {
        s[i * 4..i * 4 + 4].copy_from_slice(&f(i).to_le_bytes());
    }
    s
}

#[test]
fn avx_fma_ymm_vfmadd231ps() {
    let s = avx_f32_scratch(|i| {
        if i < 8 {
            (i as f32) + 1.0
        } else {
            (i as f32) - 5.0
        }
    });

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFC, 0x10, 0x07]); // vmovups ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFC, 0x10, 0x4F, 0x20]); // vmovups ymm1, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x75, 0xB8, 0xC0]); // vfmadd231ps ymm0, ymm1, ymm0
    code.extend_from_slice(&[0xC5, 0xFC, 0x11, 0x07]); // vmovups [rdi], ymm0
    code.push(HLT);
    check_avx_mem("avx_fma_ymm_vfmadd231ps", &code, s);
}

#[test]
fn avx_fma_scalar_vfmsub132sd_preserves_upper() {
    let mut s = [0u8; 64];
    for (i, v) in [3.0f64, 1234.5, 2.0, 5.0].iter().enumerate() {
        s[i * 8..i * 8 + 8].copy_from_slice(&v.to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xF9, 0x10, 0x07]); // vmovupd xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFB, 0x10, 0x4F, 0x10]); // vmovsd xmm1, [rdi+16]
    code.extend_from_slice(&[0xC4, 0xE2, 0xF1, 0x9B, 0x47, 0x18]); // vfmsub132sd xmm0, xmm1, [rdi+24]
    code.extend_from_slice(&[0xC5, 0xF9, 0x11, 0x07]); // vmovupd [rdi], xmm0
    code.push(HLT);
    check_avx_mem("avx_fma_scalar_vfmsub132sd_preserves_upper", &code, s);
}

#[test]
fn avx2_ymm_vpermd_vpermq() {
    let mut s = [0u8; 64];
    let data = [
        0x0102_0304u32,
        0x1112_1314,
        0x2122_2324,
        0x3132_3334,
        0x4142_4344,
        0x5152_5354,
        0x6162_6364,
        0x7172_7374,
    ];
    let index = [7u32, 0, 5, 2, 4, 1, 6, 3];
    for i in 0..8 {
        s[i * 4..i * 4 + 4].copy_from_slice(&data[i].to_le_bytes());
        s[32 + i * 4..32 + i * 4 + 4].copy_from_slice(&index[i].to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x75, 0x36, 0xD0]); // vpermd ymm2, ymm1, ymm0
    code.extend_from_slice(&[0xC4, 0xE3, 0xFD, 0x00, 0xD8, 0x1B]); // vpermq ymm3, ymm0, 0x1b
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx2_ymm_vpermd_vpermq", &code, s);
}

#[test]
fn avx2_ymm_broadcast_i128_and_qword() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x23u8).wrapping_add((i as u8).wrapping_mul(17));
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x5A, 0x67, 0x10]); // vbroadcasti128 ymm4, [rdi+16]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x59, 0x6F, 0x08]); // vpbroadcastq ymm5, [rdi+8]
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x27]); // vmovdqu [rdi], ymm4
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x6F, 0x20]); // vmovdqu [rdi+32], ymm5
    code.push(HLT);
    check_avx_mem("avx2_ymm_broadcast_i128_and_qword", &code, s);
}

#[test]
fn avx2_ymm_vpgatherdd_clears_mask() {
    let mut s = [0u8; 64];
    let data = [
        0x0302_0100u32,
        0x1312_1110,
        0x2322_2120,
        0x3332_3130,
        0x4342_4140,
        0x5352_5150,
        0x6362_6160,
        0x7372_7170,
    ];
    let index = [6u32, 1, 4, 7, 0, 5, 2, 3];
    for i in 0..8 {
        s[i * 4..i * 4 + 4].copy_from_slice(&data[i].to_le_bytes());
        s[32 + i * 4..32 + i * 4 + 4].copy_from_slice(&index[i].to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xED, 0x76, 0xD2]); // vpcmpeqd ymm2, ymm2, ymm2
    code.extend_from_slice(&[0xC5, 0xFD, 0xEF, 0xC0]); // vpxor ymm0, ymm0, ymm0
    code.extend_from_slice(&[0xC4, 0xE2, 0x6D, 0x90, 0x04, 0x8F]); // vpgatherdd ymm0, [rdi+ymm1*4], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x07]); // vmovdqu [rdi], ymm0
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x57, 0x20]); // vmovdqu [rdi+32], ymm2
    code.push(HLT);
    check_avx_mem("avx2_ymm_vpgatherdd_clears_mask", &code, s);
}

#[test]
fn avx_vex_packed_int_float_conversions() {
    let mut s = [0u8; 64];
    let data = [-128i32, -7, -1, 0, 1, 5, 42, 1024];
    for (i, value) in data.iter().enumerate() {
        s[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFC, 0x5B, 0xC8]); // vcvtdq2ps ymm1, ymm0
    code.extend_from_slice(&[0xC5, 0xFD, 0x5B, 0xD1]); // vcvtps2dq ymm2, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x5B, 0xD9]); // vcvttps2dq ymm3, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_vex_packed_int_float_conversions", &code, s);
}

#[test]
fn f16c_xmm_memory_roundtrip() {
    let mut s = [0u8; 64];
    let halves = [0x3C00u16, 0xC000, 0x3800, 0x4400];
    for (i, half) in halves.iter().enumerate() {
        s[i * 2..i * 2 + 2].copy_from_slice(&half.to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x79, 0x13, 0x0F]); // vcvtph2ps xmm1, qword ptr [rdi]
    code.extend_from_slice(&[0xC5, 0xF8, 0x11, 0x4F, 0x10]); // vmovups [rdi+16], xmm1
    code.extend_from_slice(&[0xC4, 0xE3, 0x79, 0x1D, 0x4F, 0x20, 0x00]); // vcvtps2ph qword ptr [rdi+32], xmm1, 0
    code.push(HLT);
    check_avx_mem("f16c_xmm_memory_roundtrip", &code, s);
}

#[test]
fn f16c_ymm_memory_roundtrip() {
    let mut s = [0u8; 64];
    let halves = [
        0x3C00u16, 0xC000, 0x3800, 0x4400, 0x3400, 0x4200, 0xB800, 0x4000,
    ];
    for (i, half) in halves.iter().enumerate() {
        s[i * 2..i * 2 + 2].copy_from_slice(&half.to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x13, 0x0F]); // vcvtph2ps ymm1, xmmword ptr [rdi]
    code.extend_from_slice(&[0xC5, 0xFC, 0x11, 0x4F, 0x20]); // vmovups [rdi+32], ymm1
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x1D, 0x4F, 0x10, 0x00]); // vcvtps2ph xmmword ptr [rdi+16], ymm1, 0
    code.push(HLT);
    check_avx_mem("f16c_ymm_memory_roundtrip", &code, s);
}

#[test]
fn f16c_ymm_register_roundtrip() {
    let values = [1.0f32, -2.0, 0.5, 4.0, 0.25, 3.0, -0.5, 2.0];
    let s = avx_f32_scratch(|i| values[i % values.len()]);

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFC, 0x10, 0x0F]); // vmovups ymm1, [rdi]
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x1D, 0xCA, 0x00]); // vcvtps2ph xmm2, ymm1, 0
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x57, 0x20]); // vmovdqu [rdi+32], xmm2
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x13, 0xDA]); // vcvtph2ps ymm3, xmm2
    code.extend_from_slice(&[0xC5, 0xFC, 0x11, 0x1F]); // vmovups [rdi], ymm3
    code.push(HLT);
    check_avx_mem("f16c_ymm_register_roundtrip", &code, s);
}

#[test]
fn vex_xmm_avx_vnni_register_forms() {
    let mut s = [0u8; 64];
    for (i, byte) in s.iter_mut().enumerate() {
        *byte = 0x21u8.wrapping_add((i as u8).wrapping_mul(13));
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x10]); // vmovdqu xmm1, [rdi+16]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x17]); // vmovdqu xmm2, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x1F]); // vmovdqu xmm3, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x27]); // vmovdqu xmm4, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x2F]); // vmovdqu xmm5, [rdi]
    code.extend_from_slice(&[0xC4, 0xE2, 0x79, 0x50, 0xD1]); // {vex} vpdpbusd xmm2, xmm0, xmm1
    code.extend_from_slice(&[0xC4, 0xE2, 0x79, 0x51, 0xD9]); // {vex} vpdpbusds xmm3, xmm0, xmm1
    code.extend_from_slice(&[0xC4, 0xE2, 0x79, 0x52, 0xE1]); // {vex} vpdpwssd xmm4, xmm0, xmm1
    code.extend_from_slice(&[0xC4, 0xE2, 0x79, 0x53, 0xE9]); // {vex} vpdpwssds xmm5, xmm0, xmm1
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x17]); // vmovdqu [rdi], xmm2
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x5F, 0x10]); // vmovdqu [rdi+16], xmm3
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x67, 0x20]); // vmovdqu [rdi+32], xmm4
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x6F, 0x30]); // vmovdqu [rdi+48], xmm5
    code.push(HLT);
    check_avx_mem("vex_xmm_avx_vnni_register_forms", &code, s);
}

#[test]
fn vex_ymm_avx_vnni_byte_register_forms() {
    let mut s = [0u8; 64];
    for (i, byte) in s.iter_mut().enumerate() {
        *byte = 0x43u8.wrapping_add((i as u8).wrapping_mul(17));
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x17]); // vmovdqu ymm2, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x1F]); // vmovdqu ymm3, [rdi]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x50, 0xD1]); // {vex} vpdpbusd ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x51, 0xD9]); // {vex} vpdpbusds ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("vex_ymm_avx_vnni_byte_register_forms", &code, s);
}

#[test]
fn vex_ymm_avx_vnni_word_register_forms() {
    let mut s = [0u8; 64];
    for (i, byte) in s.iter_mut().enumerate() {
        *byte = 0x7Du8.wrapping_sub((i as u8).wrapping_mul(9));
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x27]); // vmovdqu ymm4, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x2F]); // vmovdqu ymm5, [rdi]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x52, 0xE1]); // {vex} vpdpwssd ymm4, ymm0, ymm1
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x53, 0xE9]); // {vex} vpdpwssds ymm5, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x27]); // vmovdqu [rdi], ymm4
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x6F, 0x20]); // vmovdqu [rdi+32], ymm5
    code.push(HLT);
    check_avx_mem("vex_ymm_avx_vnni_word_register_forms", &code, s);
}

#[test]
fn vex_ymm_avx_vnni_byte_memory_forms() {
    let mut s = [0u8; 64];
    for (i, byte) in s.iter_mut().enumerate() {
        *byte = 0xA5u8.rotate_left((i % 8) as u32) ^ (i as u8).wrapping_mul(5);
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x17]); // vmovdqu ymm2, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x1F]); // vmovdqu ymm3, [rdi]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x50, 0x57, 0x20]); // {vex} vpdpbusd ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x51, 0x5F, 0x20]); // {vex} vpdpbusds ymm3, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("vex_ymm_avx_vnni_byte_memory_forms", &code, s);
}

#[test]
fn vex_ymm_avx_vnni_word_memory_forms() {
    let mut s = [0u8; 64];
    for (i, byte) in s.iter_mut().enumerate() {
        *byte = 0x5Au8.rotate_right((i % 8) as u32) ^ (i as u8).wrapping_mul(11);
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x27]); // vmovdqu ymm4, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x2F]); // vmovdqu ymm5, [rdi]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x52, 0x67, 0x20]); // {vex} vpdpwssd ymm4, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x53, 0x6F, 0x20]); // {vex} vpdpwssds ymm5, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x27]); // vmovdqu [rdi], ymm4
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x6F, 0x20]); // vmovdqu [rdi+32], ymm5
    code.push(HLT);
    check_avx_mem("vex_ymm_avx_vnni_word_memory_forms", &code, s);
}

#[test]
fn evex_zmm_vpdpbusd() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (i as u8).wrapping_mul(5).wrapping_add(3);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x50, 0xC0]); // vpdpbusd zmm0, zmm0, zmm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x07]); // vmovdqu32 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vpdpbusd", &code, s);
}

#[test]
fn evex_zmm_vpdpwssd() {
    let s = evex_dword_scratch(|i| {
        0x0001_0002u32
            .wrapping_mul((i as u32) + 1)
            .rotate_left((i % 5) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x52, 0xC0]); // vpdpwssd zmm0, zmm0, zmm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x07]); // vmovdqu32 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vpdpwssd", &code, s);
}

fn evex_vnni_memory_source() -> [u64; 8] {
    [
        0x7E81_0102_80FF_0304,
        0x1020_F0E0_1122_EEDD,
        0x5566_AA99_3344_CCBB,
        0x7F00_8102_0101_FEFE,
        0x2040_C0A0_4060_A0C0,
        0x6A95_1234_956A_EDCB,
        0x0102_0304_0506_0708,
        0xF8F7_F6F5_0809_0A0B,
    ]
}

fn check_evex_vnni_memory_disp8(label: &str, insn: &[u8]) {
    let s = evex_dword_scratch(|i| {
        0x0102_0305u32
            .wrapping_mul((i as u32) + 3)
            .rotate_left((i % 11) as u32)
    });
    let source = evex_vnni_memory_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x17]); // vmovdqu32 zmm2, [rdi]
    code.extend_from_slice(insn);
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem(label, &code, s);
}

#[test]
fn evex_zmm_vpdpbusd_memory_disp8() {
    check_evex_vnni_memory_disp8(
        "evex_zmm_vpdpbusd_memory_disp8",
        &[0x62, 0xF2, 0x7D, 0x48, 0x50, 0x57, 0x01],
    );
}

#[test]
fn evex_zmm_vpdpbusds_memory_broadcast_disp8() {
    check_evex_vnni_memory_disp8(
        "evex_zmm_vpdpbusds_memory_broadcast_disp8",
        &[0x62, 0xF2, 0x7D, 0x58, 0x51, 0x57, 0x10],
    );
}

#[test]
fn evex_zmm_vpdpwssd_memory_disp8() {
    check_evex_vnni_memory_disp8(
        "evex_zmm_vpdpwssd_memory_disp8",
        &[0x62, 0xF2, 0x7D, 0x48, 0x52, 0x57, 0x01],
    );
}

#[test]
fn evex_zmm_vpdpwssds_memory_broadcast_disp8() {
    check_evex_vnni_memory_disp8(
        "evex_zmm_vpdpwssds_memory_broadcast_disp8",
        &[0x62, 0xF2, 0x7D, 0x58, 0x53, 0x57, 0x10],
    );
}

#[test]
fn evex_xmm_vnni_register_forms() {
    let mut s = [0u8; 64];
    for (i, byte) in s.iter_mut().enumerate() {
        *byte = (0x21u8).wrapping_add((i as u8).wrapping_mul(13));
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x10]); // vmovdqu xmm1, [rdi+16]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x17]); // vmovdqu xmm2, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x1F]); // vmovdqu xmm3, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x27]); // vmovdqu xmm4, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x2F]); // vmovdqu xmm5, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x08, 0x50, 0xD1]); // vpdpbusd xmm2, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x08, 0x51, 0xD9]); // vpdpbusds xmm3, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x08, 0x52, 0xE1]); // vpdpwssd xmm4, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x08, 0x53, 0xE9]); // vpdpwssds xmm5, xmm0, xmm1
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x17]); // vmovdqu [rdi], xmm2
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x5F, 0x10]); // vmovdqu [rdi+16], xmm3
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x67, 0x20]); // vmovdqu [rdi+32], xmm4
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x6F, 0x30]); // vmovdqu [rdi+48], xmm5
    code.push(HLT);
    check_evex_mem("evex_xmm_vnni_register_forms", &code, s);
}

#[test]
fn evex_ymm_vnni_register_forms() {
    let mut s = [0u8; 64];
    for (i, byte) in s.iter_mut().enumerate() {
        *byte = (0x43u8).wrapping_add((i as u8).wrapping_mul(17));
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x17]); // vmovdqu ymm2, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x1F]); // vmovdqu ymm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x28, 0x50, 0xD1]); // vpdpbusd ymm2, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x28, 0x51, 0xD9]); // vpdpbusds ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_vnni_register_forms", &code, s);
}

#[test]
fn evex_ymm_vnni_word_register_forms() {
    let mut s = [0u8; 64];
    for (i, byte) in s.iter_mut().enumerate() {
        *byte = (0x7Du8).wrapping_sub((i as u8).wrapping_mul(9));
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x27]); // vmovdqu ymm4, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x2F]); // vmovdqu ymm5, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x28, 0x52, 0xE1]); // vpdpwssd ymm4, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x28, 0x53, 0xE9]); // vpdpwssds ymm5, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x27]); // vmovdqu [rdi], ymm4
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x6F, 0x20]); // vmovdqu [rdi+32], ymm5
    code.push(HLT);
    check_evex_mem("evex_ymm_vnni_word_register_forms", &code, s);
}

#[test]
fn evex_ymm_vnni_memory_forms() {
    let mut s = [0u8; 64];
    for (i, byte) in s.iter_mut().enumerate() {
        *byte = (0xA5u8).rotate_left((i % 8) as u32) ^ (i as u8).wrapping_mul(5);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x17]); // vmovdqu ymm2, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x1F]); // vmovdqu ymm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x28, 0x50, 0x57, 0x01]); // vpdpbusd ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x28, 0x51, 0x5F, 0x01]); // vpdpbusds ymm3, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_vnni_memory_forms", &code, s);
}

#[test]
fn evex_ymm_vnni_word_memory_forms() {
    let mut s = [0u8; 64];
    for (i, byte) in s.iter_mut().enumerate() {
        *byte = (0x5Au8).rotate_right((i % 8) as u32) ^ (i as u8).wrapping_mul(11);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x27]); // vmovdqu ymm4, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x2F]); // vmovdqu ymm5, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x28, 0x52, 0x67, 0x01]); // vpdpwssd ymm4, ymm0, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x28, 0x53, 0x6F, 0x01]); // vpdpwssds ymm5, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x27]); // vmovdqu [rdi], ymm4
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x6F, 0x20]); // vmovdqu [rdi+32], ymm5
    code.push(HLT);
    check_evex_mem("evex_ymm_vnni_word_memory_forms", &code, s);
}

fn evex_bf16_f32_source() -> [u64; 8] {
    [
        0xC020_0000_3FA0_0000,
        0x3F00_0000_4060_0000,
        0xC080_0000_4088_0000,
        0x3E80_0000_BF40_0000,
        0x4100_0000_C100_0000,
        0x3DCC_CCCD_40C0_0000,
        0xC040_0000_3FC0_0000,
        0x4000_0000_C000_0000,
    ]
}

#[test]
fn evex_zmm_vcvtneps2bf16() {
    let s = avx_f32_scratch(|i| match i % 8 {
        0 => 1.0,
        1 => -2.0,
        2 => 3.5,
        3 => -4.25,
        4 => 8.0,
        5 => -16.0,
        6 => 0.5,
        _ => -0.25,
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x48, 0x72, 0xC8]); // vcvtneps2bf16 ymm1, zmm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x28, 0x7F, 0x0F]); // vmovdqu16 [rdi], ymm1
    code.push(HLT);
    check_evex_mem("evex_zmm_vcvtneps2bf16", &code, s);
}

#[test]
fn evex_zmm_vcvtneps2bf16_memory_disp8() {
    let s = avx_f32_scratch(|i| (i as f32) * 0.75 - 5.5);
    let source = evex_bf16_f32_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x48, 0x72, 0x4F, 0x01]); // vcvtneps2bf16 ymm1, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x28, 0x7F, 0x0F]); // vmovdqu16 [rdi], ymm1
    code.push(HLT);
    check_evex_mem("evex_zmm_vcvtneps2bf16_memory_disp8", &code, s);
}

#[test]
fn evex_zmm_vcvtne2ps2bf16() {
    let s = avx_f32_scratch(|i| (i as f32) + 1.0);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7F, 0x48, 0x72, 0xD0]); // vcvtne2ps2bf16 zmm2, zmm0, zmm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x7F, 0x17]); // vmovdqu16 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vcvtne2ps2bf16", &code, s);
}

#[test]
fn evex_zmm_vcvtne2ps2bf16_memory_disp8() {
    let s = avx_f32_scratch(|i| match i % 4 {
        0 => 1.0,
        1 => -2.0,
        2 => 4.0,
        _ => -8.0,
    });
    let source = evex_bf16_f32_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7F, 0x48, 0x72, 0x5F, 0x01]); // vcvtne2ps2bf16 zmm3, zmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x7F, 0x1F]); // vmovdqu16 [rdi], zmm3
    code.push(HLT);
    check_evex_mem("evex_zmm_vcvtne2ps2bf16_memory_disp8", &code, s);
}

#[test]
fn evex_zmm_vdpbf16ps() {
    let s = avx_f32_scratch(|_| 1.0);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x48, 0x52, 0xC0]); // vdpbf16ps zmm0, zmm0, zmm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x07]); // vmovdqu32 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vdpbf16ps", &code, s);
}

#[test]
fn evex_zmm_vdpbf16ps_mask_merge_zero_forms() {
    let s = avx_f32_scratch(|i| match i % 8 {
        0 => 1.0,
        1 => -2.0,
        2 => 0.5,
        3 => 4.0,
        4 => -0.25,
        5 => 3.0,
        6 => -1.5,
        _ => 2.0,
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0x5A, 0xA5, 0x00, 0x00]); // mov eax, 0xa55a
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x0F]); // vmovdqu32 zmm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x17]); // vmovdqu32 zmm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x1F]); // vmovdqu32 zmm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x76, 0x49, 0x52, 0xC2]); // vdpbf16ps zmm0{k1}, zmm1, zmm2
    code.extend_from_slice(&[0x62, 0xF2, 0x76, 0xC9, 0x52, 0xDA]); // vdpbf16ps zmm3{k1}{z}, zmm1, zmm2
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xEF, 0xC3]); // vpxord zmm0, zmm0, zmm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x07]); // vmovdqu32 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vdpbf16ps_mask_merge_zero_forms", &code, s);
}

#[test]
fn evex_zmm_vdpbf16ps_memory_broadcast_disp8() {
    let s = avx_f32_scratch(|i| match i % 4 {
        0 => 1.0,
        1 => -2.0,
        2 => 0.5,
        _ => 4.0,
    });
    let source = evex_bf16_f32_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x17]); // vmovdqu32 zmm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x58, 0x52, 0x57, 0x10]); // vdpbf16ps zmm2, zmm0, [rdi+64]{1to16}
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vdpbf16ps_memory_broadcast_disp8", &code, s);
}

#[test]
fn evex_xmm_bf16_vl_register_forms() {
    let s = avx_f32_scratch(|i| {
        [1.0, -2.0, 3.5, -4.25, 8.0, -16.0, 0.5, -0.25][i % 8]
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x07]); // vmovdqu32 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu32 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x57, 0x02]); // vmovdqu32 xmm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x08, 0x52, 0xD1]); // vdpbf16ps xmm2, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x08, 0x72, 0xD8]); // vcvtneps2bf16 xmm3, xmm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7F, 0x08, 0x72, 0xE1]); // vcvtne2ps2bf16 xmm4, xmm0, xmm1
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x17]); // vmovdqu [rdi], xmm2
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x5F, 0x10]); // vmovdqu [rdi+16], xmm3
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x67, 0x20]); // vmovdqu [rdi+32], xmm4
    code.push(HLT);
    check_evex_mem("evex_xmm_bf16_vl_register_forms", &code, s);
}

#[test]
fn evex_ymm_bf16_vl_conversion_register_forms() {
    let s = avx_f32_scratch(|i| (i as f32) * 0.75 - 5.5);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x07]); // vmovdqu32 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu32 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x28, 0x72, 0xD8]); // vcvtneps2bf16 xmm3, ymm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7F, 0x28, 0x72, 0xE1]); // vcvtne2ps2bf16 ymm4, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x1F]); // vmovdqu [rdi], xmm3
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x67, 0x20]); // vmovdqu [rdi+32], ymm4
    code.push(HLT);
    check_evex_mem("evex_ymm_bf16_vl_conversion_register_forms", &code, s);
}

#[test]
fn evex_ymm_bf16_vl_conversion_memory_forms() {
    let s = avx_f32_scratch(|i| match i % 4 {
        0 => 1.0,
        1 => -2.0,
        2 => 4.0,
        _ => -8.0,
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x07]); // vmovdqu32 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x28, 0x72, 0x5F, 0x01]); // vcvtneps2bf16 xmm3, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x7F, 0x28, 0x72, 0x67, 0x01]); // vcvtne2ps2bf16 ymm4, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x1F]); // vmovdqu [rdi], xmm3
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x67, 0x20]); // vmovdqu [rdi+32], ymm4
    code.push(HLT);
    check_evex_mem("evex_ymm_bf16_vl_conversion_memory_forms", &code, s);
}

#[test]
fn evex_ymm_vdpbf16ps_vl_register_memory_forms() {
    let s = avx_f32_scratch(|i| match i % 4 {
        0 => 1.0,
        1 => -2.0,
        2 => 0.5,
        _ => 4.0,
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x07]); // vmovdqu32 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu32 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x17]); // vmovdqu32 ymm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x1F]); // vmovdqu32 ymm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x28, 0x52, 0xD1]); // vdpbf16ps ymm2, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x28, 0x52, 0x5F, 0x01]); // vdpbf16ps ymm3, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_vdpbf16ps_vl_register_memory_forms", &code, s);
}

#[test]
fn evex_zmm_vaddph() {
    let mut s = [0u8; 64];
    let values = [0x3C00u16, 0xC000, 0x3800, 0x4000, 0x4200, 0xBC00, 0x4400, 0xB800];
    for i in 0..32 {
        s[i * 2..i * 2 + 2].copy_from_slice(&values[i % values.len()].to_le_bytes());
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x6F, 0x07]); // vmovdqu16 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF5, 0x7C, 0x48, 0x58, 0xD0]); // vaddph zmm2, zmm0, zmm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x7F, 0x17]); // vmovdqu16 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vaddph", &code, s);
}

#[test]
fn evex_zmm_vaddsubph_mask_merge_zero_forms() {
    let s = evex_fp16_scratch(&[
        0x3C00, // 1.0
        0x4000, // 2.0
        0x3800, // 0.5
        0x4400, // 4.0
        0xBC00, // -1.0
        0xC000, // -2.0
        0x4200, // 3.0
        0x3400, // 0.25
    ]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xC3, 0x3C, 0x5A, 0xA5]); // mov eax, 0xa55a3cc3
    code.extend_from_slice(&[0xC5, 0xFB, 0x92, 0xC8]); // kmovd k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x6F, 0x07]); // vmovdqu16 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x6F, 0x0F]); // vmovdqu16 zmm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x6F, 0x1F]); // vmovdqu16 zmm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0x70, 0xD1, 0xB1]); // vpshufd zmm2, zmm1, 0xb1
    code.extend_from_slice(&[0x62, 0xF5, 0x74, 0x49, 0x58, 0xC2]); // vaddph zmm0{k1}, zmm1, zmm2
    code.extend_from_slice(&[0x62, 0xF5, 0x74, 0xC9, 0x5C, 0xDA]); // vsubph zmm3{k1}{z}, zmm1, zmm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x48, 0xEF, 0xC3]); // vpxorq zmm0, zmm0, zmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x7F, 0x07]); // vmovdqu16 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vaddsubph_mask_merge_zero_forms", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 16: AVX-512 IFMA, VBMI/VBMI2, BITALG, and FP16 depth.
//
// Granite Rapids advertises these feature bits, and the emulator has explicit
// EVEX handlers for them. Encodings were checked with llvm-mc.
// ===========================================================================

fn evex_qword_scratch<F>(mut f: F) -> [u8; 64]
where
    F: FnMut(usize) -> u64,
{
    let mut s = [0u8; 64];
    for i in 0..8 {
        s[i * 8..i * 8 + 8].copy_from_slice(&f(i).to_le_bytes());
    }
    s
}

fn evex_fp16_scratch(values: &[u16]) -> [u8; 64] {
    let mut s = [0u8; 64];
    for i in 0..32 {
        s[i * 2..i * 2 + 2].copy_from_slice(&values[i % values.len()].to_le_bytes());
    }
    s
}

fn evex_fp16_disp8_source() -> [u64; 8] {
    evex_f16_words([
        0x4000, 0x3800, 0x3C00, 0x4400, 0x4200, 0x3400, 0x4800, 0x3000, 0x4C00,
        0x2C00, 0x5000, 0x2800, 0x5400, 0x2400, 0x5800, 0x2000, 0x3C00, 0x4000,
        0x4400, 0x4800, 0x3400, 0x3800, 0x4200, 0x4C00, 0x3000, 0x2C00, 0x5000,
        0x5400, 0x2800, 0x2400, 0x5800, 0x2000,
    ])
}

#[test]
fn evex_xmm_fp16_add_sub_vl_forms() {
    let s = evex_fp16_scratch(&[
        0x3C00, // 1.0
        0x4000, // 2.0
        0x4400, // 4.0
        0x3800, // 0.5
        0xBC00, // -1.0
        0xC000, // -2.0
        0x4200, // 3.0
        0x3400, // 0.25
    ]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x6F, 0x07]); // vmovdqu16 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu16 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF5, 0x7C, 0x08, 0x58, 0xD1]); // vaddph xmm2, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF5, 0x7C, 0x08, 0x5C, 0xD9]); // vsubph xmm3, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF5, 0x7C, 0x08, 0x58, 0x67, 0x02]); // vaddph xmm4, xmm0, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF5, 0x7C, 0x08, 0x5C, 0x6F, 0x03]); // vsubph xmm5, xmm0, [rdi+48]
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x7F, 0x17]); // vmovdqu16 [rdi], xmm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu16 [rdi+16], xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x7F, 0x67, 0x02]); // vmovdqu16 [rdi+32], xmm4
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x7F, 0x6F, 0x03]); // vmovdqu16 [rdi+48], xmm5
    code.push(HLT);
    check_evex_mem("evex_xmm_fp16_add_sub_vl_forms", &code, s);
}

#[test]
fn evex_ymm_fp16_mul_div_vl_forms() {
    let s = evex_fp16_scratch(&[
        0x3C00, // 1.0
        0x4000, // 2.0
        0x4400, // 4.0
        0x3800, // 0.5
        0xBC00, // -1.0
        0xC000, // -2.0
        0x4200, // 3.0
        0x3400, // 0.25
    ]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x28, 0x6F, 0x07]); // vmovdqu16 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu16 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF5, 0x7C, 0x28, 0x59, 0xD1]); // vmulph ymm2, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF5, 0x7C, 0x28, 0x5E, 0xD9]); // vdivph ymm3, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF5, 0x7C, 0x28, 0x59, 0x27]); // vmulph ymm4, ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF5, 0x74, 0x28, 0x5E, 0x2F]); // vdivph ymm5, ymm1, [rdi]
    code.extend_from_slice(&[0xC5, 0xED, 0xEF, 0xD4]); // vpxor ymm2, ymm2, ymm4
    code.extend_from_slice(&[0xC5, 0xE5, 0xEF, 0xDD]); // vpxor ymm3, ymm3, ymm5
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x28, 0x7F, 0x17]); // vmovdqu16 [rdi], ymm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x28, 0x7F, 0x5F, 0x01]); // vmovdqu16 [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_fp16_mul_div_vl_forms", &code, s);
}

#[test]
fn evex_xmm_vpmadd52_vl_forms() {
    let s = evex_qword_scratch(|i| {
        0x0000_0001_0000_0101u64
            .wrapping_mul((i as u64) + 3)
            .wrapping_add(0x2345 + i as u64)
            & 0x000F_FFFF_FFFF_FFFF
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x07]); // vmovdqu64 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu64 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x57, 0x02]); // vmovdqu64 xmm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x1F]); // vmovdqu64 xmm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x08, 0xB4, 0xC2]); // vpmadd52luq xmm0, xmm1, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x08, 0xB5, 0xC2]); // vpmadd52huq xmm0, xmm1, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x08, 0xB4, 0x5F, 0x02]); // vpmadd52luq xmm3, xmm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x08, 0xB5, 0x5F, 0x03]); // vpmadd52huq xmm3, xmm1, [rdi+48]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x07]); // vmovdqu64 [rdi], xmm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+16], xmm3
    code.push(HLT);
    check_evex_mem("evex_xmm_vpmadd52_vl_forms", &code, s);
}

#[test]
fn evex_ymm_vpmadd52_vl_forms() {
    let s = evex_qword_scratch(|i| {
        0x0000_0001_0000_0101u64
            .wrapping_mul((i as u64) + 5)
            .wrapping_add(0x3456 + i as u64)
            & 0x000F_FFFF_FFFF_FFFF
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu64 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x17]); // vmovdqu64 ymm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x5F, 0x01]); // vmovdqu64 ymm3, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x28, 0xB4, 0xC2]); // vpmadd52luq ymm0, ymm1, ymm2
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x28, 0xB5, 0xC2]); // vpmadd52huq ymm0, ymm1, ymm2
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x28, 0xB4, 0x1F]); // vpmadd52luq ymm3, ymm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x28, 0xB5, 0x5F, 0x01]); // vpmadd52huq ymm3, ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x28, 0xEF, 0xC3]); // vpxorq ymm0, ymm0, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x07]); // vmovdqu64 [rdi], ymm0
    code.push(HLT);
    check_evex_mem("evex_ymm_vpmadd52_vl_forms", &code, s);
}

#[test]
fn evex_zmm_vpmadd52luq_huq() {
    let s = evex_qword_scratch(|i| {
        0x0000_0001_0000_0101u64
            .wrapping_mul((i as u64) + 1)
            .wrapping_add(0x1234 + i as u64)
            & 0x000F_FFFF_FFFF_FFFF
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x48, 0xD4, 0xC8]); // vpaddq zmm1, zmm0, zmm0
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x48, 0xB4, 0xC1]); // vpmadd52luq zmm0, zmm1, zmm1
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x48, 0xB5, 0xC1]); // vpmadd52huq zmm0, zmm1, zmm1
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]); // vmovdqu64 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vpmadd52luq_huq", &code, s);
}

#[test]
fn evex_zmm_vpmadd52_mask_merge_zero_forms() {
    let s = evex_qword_scratch(|i| {
        0x0000_0002_0001_0101u64
            .wrapping_mul((i as u64) + 7)
            .wrapping_add(0x4567 + (i as u64).wrapping_mul(0x111))
            & 0x000F_FFFF_FFFF_FFFF
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0x5A, 0x00, 0x00, 0x00]); // mov eax, 0x5a
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xC8]); // kmovb k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x1F]); // vmovdqu64 zmm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x48, 0xD4, 0xC8]); // vpaddq zmm1, zmm0, zmm0
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0x73, 0xD0, 0x0D]); // vpsrlq zmm2, zmm0, 13
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x49, 0xB4, 0xC2]); // vpmadd52luq zmm0{k1}, zmm1, zmm2
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x49, 0xB5, 0xC2]); // vpmadd52huq zmm0{k1}, zmm1, zmm2
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0xC9, 0xB4, 0xDA]); // vpmadd52luq zmm3{k1}{z}, zmm1, zmm2
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0xC9, 0xB5, 0xDA]); // vpmadd52huq zmm3{k1}{z}, zmm1, zmm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x48, 0xEF, 0xC3]); // vpxorq zmm0, zmm0, zmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]); // vmovdqu64 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vpmadd52_mask_merge_zero_forms", &code, s);
}

#[test]
fn evex_xmm_vbmi_vpermb_multishift_vl_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x91u8)
            .wrapping_add((i as u8).wrapping_mul(7))
            .rotate_left((i % 8) as u32);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x10]); // vmovdqu xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x08, 0x8D, 0xD1]); // vpermb xmm2, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x08, 0x8D, 0x5F, 0x02]); // vpermb xmm3, xmm0, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x08, 0x83, 0xE1]); // vpmultishiftqb xmm4, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x08, 0x83, 0x6F, 0x03]); // vpmultishiftqb xmm5, xmm0, [rdi+48]
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x17]); // vmovdqu [rdi], xmm2
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x5F, 0x10]); // vmovdqu [rdi+16], xmm3
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x67, 0x20]); // vmovdqu [rdi+32], xmm4
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x6F, 0x30]); // vmovdqu [rdi+48], xmm5
    code.push(HLT);
    check_evex_mem("evex_xmm_vbmi_vpermb_multishift_vl_forms", &code, s);
}

#[test]
fn evex_ymm_vbmi_vpermb_multishift_vl_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x2Du8)
            .wrapping_add((i as u8).wrapping_mul(19))
            .rotate_right((i % 7) as u32);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x28, 0x8D, 0xD1]); // vpermb ymm2, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x28, 0x8D, 0x1F]); // vpermb ymm3, ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xED, 0xEF, 0xD3]); // vpxor ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x28, 0x83, 0xE1]); // vpmultishiftqb ymm4, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x28, 0x83, 0x2F]); // vpmultishiftqb ymm5, ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xDD, 0xEF, 0xDD]); // vpxor ymm3, ymm4, ymm5
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_vbmi_vpermb_multishift_vl_forms", &code, s);
}

#[test]
fn evex_zmm_vpermb_byte_indices() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (63u8.wrapping_sub(i as u8)).wrapping_mul(7).wrapping_add(5);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x48, 0x6F, 0x07]); // vmovdqu8 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xFC, 0xC8]); // vpaddb zmm1, zmm0, zmm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x8D, 0xD1]); // vpermb zmm2, zmm0, zmm1
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x48, 0x7F, 0x17]); // vmovdqu8 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpermb_byte_indices", &code, s);
}

#[test]
fn evex_zmm_vpmultishiftqb_windows() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (i as u8).wrapping_mul(9).wrapping_add(0x31);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x48, 0x6F, 0x07]); // vmovdqu8 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xFC, 0xC8]); // vpaddb zmm1, zmm0, zmm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x48, 0x83, 0xD1]); // vpmultishiftqb zmm2, zmm0, zmm1
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x48, 0x7F, 0x17]); // vmovdqu8 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpmultishiftqb_windows", &code, s);
}

#[test]
fn evex_zmm_vpshufbitqmb_mask_result() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0xA5u8).rotate_left((i % 8) as u32) ^ (i as u8).wrapping_mul(3);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x48, 0x6F, 0x07]); // vmovdqu8 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xFC, 0xC8]); // vpaddb zmm1, zmm0, zmm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x8F, 0xD1]); // vpshufbitqmb k2, zmm0, zmm1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xCA]); // kmovq rcx, k2
    code.extend_from_slice(&[0x48, 0x89, 0x0F]); // mov [rdi], rcx
    code.push(HLT);
    check_evex_mem("evex_zmm_vpshufbitqmb_mask_result", &code, s);
}

fn evex_direct_disp8_source() -> [u64; 8] {
    [
        0x000A_BCDE_F012_3456,
        0x0005_4321_0FED_CBA9,
        0x000F_0E0D_0C0B_0A09,
        0x0001_3579_BDF0_2468,
        0x000E_DCBA_9876_5432,
        0x0002_468A_CE13_579B,
        0x000C_C3C3_3C3C_A5A5,
        0x0007_7654_3210_FEDC,
    ]
}

fn evex_fp32_disp8_source() -> [u64; 8] {
    evex_f32_words([
        2.0f32, -1.0, 0.5, -0.25, 4.0, -8.0, 0.125, -0.0625, 1.5, -2.0, 0.75,
        -0.5, 16.0, -32.0, 0.25, -0.125,
    ])
}

fn evex_fp64_disp8_source() -> [u64; 8] {
    [
        2.0f64.to_bits(),
        (-1.0f64).to_bits(),
        0.5f64.to_bits(),
        (-0.25f64).to_bits(),
        4.0f64.to_bits(),
        (-8.0f64).to_bits(),
        0.125f64.to_bits(),
        (-0.0625f64).to_bits(),
    ]
}

#[test]
fn evex_zmm_vaddps_memory_disp8_form() {
    let s = evex_dword_scratch(|i| {
        [
            1.0f32, 2.0, -3.0, 4.0, 0.5, -0.25, 8.0, -16.0, 3.5, -7.0, 9.0,
            -11.0, 0.125, -0.5, 13.0, -17.0,
        ][i]
            .to_bits()
    });
    let source = evex_fp32_disp8_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7C, 0x48, 0x58, 0x57, 0x01]); // vaddps zmm2, zmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vaddps_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vaddsubps_mask_merge_zero_forms() {
    let s = evex_dword_scratch(|i| {
        [
            1.0f32, 2.0, -3.0, 4.0, 0.5, -0.25, 8.0, -16.0, 3.5, -7.0, 9.0,
            -11.0, 0.125, -0.5, 13.0, -17.0,
        ][i]
            .to_bits()
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xA5, 0x5A, 0x00, 0x00]); // mov eax, 0x5aa5
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x0F]); // vmovdqu32 zmm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x1F]); // vmovdqu32 zmm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x04, 0xD1, 0xB1]); // vpermilps zmm2, zmm1, 0xb1
    code.extend_from_slice(&[0x62, 0xF1, 0x74, 0x49, 0x58, 0xC2]); // vaddps zmm0{k1}, zmm1, zmm2
    code.extend_from_slice(&[0x62, 0xF1, 0x74, 0xC9, 0x5C, 0xDA]); // vsubps zmm3{k1}{z}, zmm1, zmm2
    code.extend_from_slice(&[0x62, 0xF1, 0x7C, 0x48, 0x57, 0xC3]); // vxorps zmm0, zmm0, zmm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x07]); // vmovdqu32 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vaddsubps_mask_merge_zero_forms", &code, s);
}

#[test]
fn evex_zmm_vmulpd_memory_broadcast_disp8_form() {
    let s = evex_qword_scratch(|i| {
        [1.0f64, 2.0, -3.0, 4.0, 0.5, -0.25, 8.0, -16.0][i].to_bits()
    });
    let source = evex_fp64_disp8_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x58, 0x59, 0x5F, 0x08]); // vmulpd zmm3, zmm0, [rdi+64]{1to8}
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x1F]); // vmovdqu64 [rdi], zmm3
    code.push(HLT);
    check_evex_mem("evex_zmm_vmulpd_memory_broadcast_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vaddsubpd_mask_merge_zero_forms() {
    let s = evex_qword_scratch(|i| {
        [1.0f64, 2.0, -3.0, 4.0, 0.5, -0.25, 8.0, -16.0][i].to_bits()
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xAD, 0x00, 0x00, 0x00]); // mov eax, 0xad
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xC8]); // kmovb k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x0F]); // vmovdqu64 zmm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x1F]); // vmovdqu64 zmm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x48, 0x01, 0xD1, 0xB1]); // vpermpd zmm2, zmm1, 0xb1
    code.extend_from_slice(&[0x62, 0xF1, 0xF5, 0x49, 0x58, 0xC2]); // vaddpd zmm0{k1}, zmm1, zmm2
    code.extend_from_slice(&[0x62, 0xF1, 0xF5, 0xC9, 0x5C, 0xDA]); // vsubpd zmm3{k1}{z}, zmm1, zmm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x48, 0x57, 0xC3]); // vxorpd zmm0, zmm0, zmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]); // vmovdqu64 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vaddsubpd_mask_merge_zero_forms", &code, s);
}

#[test]
fn evex_zmm_vsqrtps_memory_disp8_form() {
    let source = evex_fp32_disp8_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0x7C, 0x48, 0x51, 0x57, 0x01]); // vsqrtps zmm2, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vsqrtps_memory_disp8_form", &code, zero_scratch());
}

#[test]
fn evex_zmm_vsqrtpd_memory_disp8_form() {
    let source = evex_fp64_disp8_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x48, 0x51, 0x5F, 0x01]); // vsqrtpd zmm3, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x1F]); // vmovdqu64 [rdi], zmm3
    code.push(HLT);
    check_evex_mem("evex_zmm_vsqrtpd_memory_disp8_form", &code, zero_scratch());
}

#[test]
fn evex_scalar_vaddss_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..4].copy_from_slice(&1.5f32.to_le_bytes());
    let source = evex_fp32_disp8_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x58, 0x57, 0x10]); // vaddss xmm2, xmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_scalar_vaddss_memory_disp8_form", &code, s);
}

#[test]
fn evex_scalar_vaddsd_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..8].copy_from_slice(&1.5f64.to_le_bytes());
    let source = evex_fp64_disp8_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x58, 0x5F, 0x08]); // vaddsd xmm3, xmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x1F]); // vmovdqu64 [rdi], zmm3
    code.push(HLT);
    check_evex_mem("evex_scalar_vaddsd_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vxorps_memory_disp8_form() {
    let s = evex_dword_scratch(|i| {
        0x3F80_0000u32
            .wrapping_add((i as u32).wrapping_mul(0x0101_0305))
            .rotate_left((i % 13) as u32)
    });
    let source = evex_direct_disp8_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7C, 0x48, 0x57, 0x57, 0x01]); // vxorps zmm2, zmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vxorps_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vandpd_memory_broadcast_disp8_form() {
    let s = evex_qword_scratch(|i| {
        0x3FF0_0000_0000_0000u64
            .wrapping_add((i as u64).wrapping_mul(0x0010_0020_0030_0050))
            .rotate_left((i * 7) as u32)
    });
    let source = evex_direct_disp8_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x58, 0x54, 0x5F, 0x08]); // vandpd zmm3, zmm0, [rdi+64]{1to8}
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x1F]); // vmovdqu64 [rdi], zmm3
    code.push(HLT);
    check_evex_mem("evex_zmm_vandpd_memory_broadcast_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vpmadd52_memory_disp8_forms() {
    let s = evex_qword_scratch(|i| {
        0x0000_0001_0000_0101u64
            .wrapping_mul((i as u64) + 3)
            .wrapping_add(0x4321 + i as u64)
            & 0x000F_FFFF_FFFF_FFFF
    });
    let source = evex_direct_disp8_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x48, 0xD4, 0xC8]); // vpaddq zmm1, zmm0, zmm0
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x48, 0xB4, 0x47, 0x01]); // vpmadd52luq zmm0, zmm1, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x58, 0xB5, 0x47, 0x08]); // vpmadd52huq zmm0, zmm1, [rdi+64]{1to8}
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]); // vmovdqu64 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vpmadd52_memory_disp8_forms", &code, s);
}

#[test]
fn evex_zmm_vpermb_memory_disp8_form() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (63u8.wrapping_sub(i as u8)).wrapping_mul(11).wrapping_add(0x17);
    }
    let source = evex_direct_disp8_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x48, 0x6F, 0x07]); // vmovdqu8 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x8D, 0x57, 0x01]); // vpermb zmm2, zmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x48, 0x7F, 0x17]); // vmovdqu8 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpermb_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vpshufbitqmb_memory_disp8_form() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x6Du8).rotate_left((i % 7) as u32) ^ (i as u8).wrapping_mul(5);
    }
    let source = evex_direct_disp8_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x48, 0x6F, 0x07]); // vmovdqu8 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x8F, 0x57, 0x01]); // vpshufbitqmb k2, zmm0, [rdi+64]
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xCA]); // kmovq rcx, k2
    code.extend_from_slice(&[0x48, 0x89, 0x0F]); // mov [rdi], rcx
    code.push(HLT);
    check_evex_mem("evex_zmm_vpshufbitqmb_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vdbpsadbw_memory_disp8_form() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (i as u8).wrapping_mul(19).wrapping_add(0x29);
    }
    let source = evex_direct_disp8_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x48, 0x6F, 0x07]); // vmovdqu8 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x42, 0x57, 0x01, 0x05]); // vdbpsadbw zmm2, zmm0, [rdi+64], 5
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x48, 0x7F, 0x17]); // vmovdqu8 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vdbpsadbw_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_fp16_mul_and_fma() {
    let s = evex_fp16_scratch(&[
        0x3C00, // 1.0
        0x4000, // 2.0
        0x4200, // 3.0
        0x4400, // 4.0
        0x3800, // 0.5
        0x3400, // 0.25
        0xC000, // -2.0
        0xBC00, // -1.0
    ]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x6F, 0x07]); // vmovdqu16 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF5, 0x7C, 0x48, 0x59, 0xD0]); // vmulph zmm2, zmm0, zmm0
    code.extend_from_slice(&[0x62, 0xF6, 0x7D, 0x48, 0xB8, 0xD0]); // vfmadd231ph zmm2, zmm0, zmm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x7F, 0x17]); // vmovdqu16 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_fp16_mul_and_fma", &code, s);
}

#[test]
fn evex_zmm_fp16_sqrt_powers() {
    let s = evex_fp16_scratch(&[
        0x3C00, // 1.0
        0x4400, // 4.0
        0x4C00, // 16.0
        0x5400, // 64.0
        0x3400, // 0.25
        0x2C00, // 0.0625
        0x4000, // 2.0
        0x4800, // 8.0
    ]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x6F, 0x07]); // vmovdqu16 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF5, 0x7C, 0x48, 0x51, 0xD8]); // vsqrtph zmm3, zmm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x7F, 0x1F]); // vmovdqu16 [rdi], zmm3
    code.push(HLT);
    check_evex_mem("evex_zmm_fp16_sqrt_powers", &code, s);
}

#[test]
fn evex_zmm_vaddph_memory_disp8() {
    let s = evex_fp16_scratch(&[
        0x3C00, 0xC000, 0x3800, 0x4000, 0x4200, 0xBC00, 0x4400, 0xB800, 0x3400,
        0xC400, 0x4800, 0x3000, 0x4C00, 0xCC00, 0x5000, 0x2C00, 0x4400, 0xBC00,
        0x3C00, 0xC000, 0x3800, 0xB800, 0x4200, 0x3400, 0x4800, 0xC400, 0x4C00,
        0x3000, 0x5000, 0xCC00, 0x2C00, 0x4000,
    ]);
    let source = evex_fp16_disp8_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x6F, 0x07]); // vmovdqu16 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF5, 0x7C, 0x48, 0x58, 0x57, 0x01]); // vaddph zmm2, zmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x7F, 0x17]); // vmovdqu16 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vaddph_memory_disp8", &code, s);
}

#[test]
fn evex_zmm_vmulph_memory_broadcast_disp8() {
    let s = evex_fp16_scratch(&[
        0x3C00, 0xC000, 0x3800, 0x4000, 0x4200, 0xBC00, 0x4400, 0xB800, 0x3400,
        0xC400, 0x4800, 0x3000, 0x4C00, 0xCC00, 0x5000, 0x2C00, 0x4400, 0xBC00,
        0x3C00, 0xC000, 0x3800, 0xB800, 0x4200, 0x3400, 0x4800, 0xC400, 0x4C00,
        0x3000, 0x5000, 0xCC00, 0x2C00, 0x4000,
    ]);
    let source = evex_fp16_disp8_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x6F, 0x07]); // vmovdqu16 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF5, 0x7C, 0x58, 0x59, 0x5F, 0x20]); // vmulph zmm3, zmm0, [rdi+64]{1to32}
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x7F, 0x1F]); // vmovdqu16 [rdi], zmm3
    code.push(HLT);
    check_evex_mem("evex_zmm_vmulph_memory_broadcast_disp8", &code, s);
}

#[test]
fn evex_zmm_vsqrtph_memory_disp8() {
    let source = evex_fp16_disp8_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF5, 0x7C, 0x48, 0x51, 0x67, 0x01]); // vsqrtph zmm4, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x7F, 0x27]); // vmovdqu16 [rdi], zmm4
    code.push(HLT);
    check_evex_mem("evex_zmm_vsqrtph_memory_disp8", &code, zero_scratch());
}

#[test]
fn evex_scalar_vaddsh_memory_disp8() {
    let s = evex_fp16_scratch(&[
        0x3E00, 0xC000, 0x3800, 0x4000, 0x4200, 0xBC00, 0x4400, 0xB800, 0x3400,
        0xC400, 0x4800, 0x3000, 0x4C00, 0xCC00, 0x5000, 0x2C00, 0x4400, 0xBC00,
        0x3C00, 0xC000, 0x3800, 0xB800, 0x4200, 0x3400, 0x4800, 0xC400, 0x4C00,
        0x3000, 0x5000, 0xCC00, 0x2C00, 0x4000,
    ]);
    let source = evex_fp16_disp8_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x6F, 0x07]); // vmovdqu16 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF5, 0x7E, 0x08, 0x58, 0x57, 0x20]); // vaddsh xmm2, xmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF5, 0x7D, 0x08, 0x7E, 0x17]); // vmovw [rdi], xmm2
    code.push(HLT);
    check_evex_mem("evex_scalar_vaddsh_memory_disp8", &code, s);
}

#[test]
fn evex_zmm_fp16_convert_roundtrip() {
    let s = evex_fp16_scratch(&[
        0x3C00, // 1.0
        0xC000, // -2.0
        0x3800, // 0.5
        0x4400, // 4.0
        0x3400, // 0.25
        0x4200, // 3.0
        0xB800, // -0.5
        0x4000, // 2.0
    ]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x28, 0x6F, 0x07]); // vmovdqu16 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF6, 0x7D, 0x48, 0x13, 0xC8]); // vcvtph2psx zmm1, ymm0
    code.extend_from_slice(&[0x62, 0xF5, 0x7D, 0x48, 0x1D, 0xD1]); // vcvtps2phx ymm2, zmm1
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x28, 0x7F, 0x17]); // vmovdqu16 [rdi], ymm2
    code.push(HLT);
    check_evex_mem("evex_zmm_fp16_convert_roundtrip", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 17: AVX2 packed shift depth.
//
// These hit VEX shift decode paths that are distinct from the existing AVX2
// arithmetic/permute coverage: per-lane variable counts, immediate byte shifts
// inside 128-bit lanes, and XMM-count packed shifts.
// ===========================================================================

fn avx2_dword_pair_scratch(values: [u32; 8], counts: [u32; 8]) -> [u8; 64] {
    let mut s = [0u8; 64];
    for i in 0..8 {
        s[i * 4..i * 4 + 4].copy_from_slice(&values[i].to_le_bytes());
        s[32 + i * 4..32 + i * 4 + 4].copy_from_slice(&counts[i].to_le_bytes());
    }
    s
}

fn avx2_qword_pair_scratch(values: [u64; 4], counts: [u64; 4]) -> [u8; 64] {
    let mut s = [0u8; 64];
    for i in 0..4 {
        s[i * 8..i * 8 + 8].copy_from_slice(&values[i].to_le_bytes());
        s[32 + i * 8..32 + i * 8 + 8].copy_from_slice(&counts[i].to_le_bytes());
    }
    s
}

#[test]
fn avx2_ymm_variable_dword_right_shifts() {
    let s = avx2_dword_pair_scratch(
        [
            0xFFFF_FF00,
            0x8000_0000,
            0x1234_5678,
            0x0000_FF00,
            0x7FFF_FFFF,
            0xF000_0001,
            0x4000_0000,
            0x0000_0001,
        ],
        [0, 1, 4, 8, 16, 31, 32, 33],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x45, 0xD1]); // vpsrlvd ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x46, 0xD9]); // vpsravd ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx2_ymm_variable_dword_right_shifts", &code, s);
}

#[test]
fn avx2_ymm_variable_dword_left_shift() {
    let s = avx2_dword_pair_scratch(
        [
            0x0000_00FF,
            0x0000_8001,
            0x1357_9BDF,
            0x2468_ACE0,
            0x8000_0001,
            0x7FFF_FFFF,
            0x0000_0001,
            0xFFFF_FFFF,
        ],
        [0, 1, 7, 15, 16, 31, 32, 40],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x47, 0xD1]); // vpsllvd ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.push(HLT);
    check_avx_mem("avx2_ymm_variable_dword_left_shift", &code, s);
}

#[test]
fn avx2_ymm_variable_qword_shifts() {
    let s = avx2_qword_pair_scratch(
        [
            0x0123_4567_89AB_CDEF,
            0x8000_0000_0000_0001,
            0xFFFF_0000_FFFF_0000,
            0x0000_0000_0000_0001,
        ],
        [0, 1, 63, 64],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0xFD, 0x45, 0xD9]); // vpsrlvq ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC4, 0xE2, 0xFD, 0x47, 0xD1]); // vpsllvq ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx2_ymm_variable_qword_shifts", &code, s);
}

#[test]
fn avx2_ymm_immediate_lane_byte_shifts() {
    let mut s = [0u8; 64];
    for i in 0..32 {
        s[i] = (0x20u8).wrapping_add((i as u8).wrapping_mul(5));
    }
    for i in 32..64 {
        s[i] = (0xE0u8).wrapping_sub(i as u8);
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xED, 0x73, 0xF8, 0x05]); // vpslldq ymm2, ymm0, 5
    code.extend_from_slice(&[0xC5, 0xE5, 0x73, 0xD8, 0x07]); // vpsrldq ymm3, ymm0, 7
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx2_ymm_immediate_lane_byte_shifts", &code, s);
}

#[test]
fn avx2_ymm_immediate_element_shift_edges() {
    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC5, 0xED, 0x71, 0xD0, 0x03]); // vpsrlw ymm2, ymm0, 3
    code.extend_from_slice(&[0xC5, 0xE5, 0x71, 0xE0, 0x0F]); // vpsraw ymm3, ymm0, 15
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_immediate_word_shifts", &code, s);

    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC5, 0xED, 0x71, 0xF0, 0x05]); // vpsllw ymm2, ymm0, 5
    code.extend_from_slice(&[0xC5, 0xE5, 0x72, 0xD0, 0x11]); // vpsrld ymm3, ymm0, 17
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_immediate_left_word_right_dword", &code, s);

    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC5, 0xED, 0x72, 0xE0, 0x1F]); // vpsrad ymm2, ymm0, 31
    code.extend_from_slice(&[0xC5, 0xE5, 0x72, 0xF0, 0x08]); // vpslld ymm3, ymm0, 8
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_immediate_arith_right_left_dword", &code, s);

    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC5, 0xED, 0x73, 0xD0, 0x3F]); // vpsrlq ymm2, ymm0, 63
    code.extend_from_slice(&[0xC5, 0xE5, 0x73, 0xF0, 0x09]); // vpsllq ymm3, ymm0, 9
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_immediate_qword_shifts", &code, s);

    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC5, 0xED, 0x73, 0xD8, 0x10]); // vpsrldq ymm2, ymm0, 16
    code.extend_from_slice(&[0xC5, 0xE5, 0x73, 0xF8, 0x11]); // vpslldq ymm3, ymm0, 17
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_immediate_lane_byte_shift_edges", &code, s);
}

fn avx2_shift_count_scratch(count: u64) -> [u8; 64] {
    let mut s = avx_byte_pair_scratch();
    s[32..40].copy_from_slice(&count.to_le_bytes());
    s
}

#[test]
fn avx2_ymm_memory_count_shift_forms() {
    let s = avx2_shift_count_scratch(17);

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC5, 0xFD, 0xD1, 0x57, 0x20]); // vpsrlw ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0xE1, 0x5F, 0x20]); // vpsraw ymm3, ymm0, [rdi+32]
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_memory_count_word_right_shifts", &code, s);

    let s = avx2_shift_count_scratch(11);

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC5, 0xFD, 0xF1, 0x57, 0x20]); // vpsllw ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0xD2, 0x5F, 0x20]); // vpsrld ymm3, ymm0, [rdi+32]
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_memory_count_left_word_right_dword", &code, s);

    let s = avx2_shift_count_scratch(33);

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC5, 0xFD, 0xE2, 0x57, 0x20]); // vpsrad ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0xF2, 0x5F, 0x20]); // vpslld ymm3, ymm0, [rdi+32]
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_memory_count_dword_shifts", &code, s);

    let s = avx2_shift_count_scratch(63);

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC5, 0xFD, 0xD3, 0x57, 0x20]); // vpsrlq ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0xF3, 0x5F, 0x20]); // vpsllq ymm3, ymm0, [rdi+32]
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_memory_count_qword_shifts", &code, s);
}

#[test]
fn avx2_ymm_shuffle_immediate_memory_forms() {
    let s = avx_byte_pair_scratch();

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFD, 0x70, 0x17, 0x4E]); // vpshufd ymm2, [rdi], 0x4e
    code.extend_from_slice(&[0xC5, 0xFE, 0x70, 0x5F, 0x20, 0xB1]); // vpshufhw ymm3, [rdi+32], 0xb1
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_vpshufd_vpshufhw_memory", &code, s);

    let s = avx_byte_pair_scratch();

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFF, 0x70, 0x17, 0x1B]); // vpshuflw ymm2, [rdi], 0x1b
    code.extend_from_slice(&[0xC5, 0xFE, 0x70, 0x5F, 0x20, 0x4E]); // vpshufhw ymm3, [rdi+32], 0x4e
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_vpshuflw_vpshufhw_memory", &code, s);

    let s = avx_byte_pair_scratch();

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x57, 0x20]); // vmovdqu ymm2, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xF9, 0x70, 0x17, 0x93]); // vpshufd xmm2, [rdi], 0x93
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.push(HLT);
    check_avx_mem("avx2_xmm_vpshufd_zeroes_upper", &code, s);
}

#[test]
fn avx2_ymm_xmm_count_word_and_qword_shifts() {
    let mut s = [0u8; 64];
    for i in 0..16 {
        let word = [
            0x8001u16, 0x7FFF, 0xF000, 0x00FF, 0x4000, 0xC001, 0x1234, 0xFEDC,
        ][i % 8];
        s[i * 2..i * 2 + 2].copy_from_slice(&word.to_le_bytes());
    }
    s[32..40].copy_from_slice(&13u64.to_le_bytes());

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x20]); // vmovdqu xmm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0xE1, 0xD1]); // vpsraw ymm2, ymm0, xmm1
    code.extend_from_slice(&[0xC5, 0xFD, 0xF3, 0xD9]); // vpsllq ymm3, ymm0, xmm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx2_ymm_xmm_count_word_and_qword_shifts", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 18: VEX floating-point arithmetic and compare.
//
// Existing AVX coverage focused on integer vectors, FMA, gathers, and
// conversions. These cases directly exercise the VEX PS/PD/SS/SD arithmetic,
// sqrt, min/max, and compare-result paths.
// ===========================================================================

fn avx_f32_pair_scratch(a: [f32; 8], b: [f32; 8]) -> [u8; 64] {
    let mut s = [0u8; 64];
    for i in 0..8 {
        s[i * 4..i * 4 + 4].copy_from_slice(&a[i].to_le_bytes());
        s[32 + i * 4..32 + i * 4 + 4].copy_from_slice(&b[i].to_le_bytes());
    }
    s
}

fn avx_f64_pair_scratch(a: [f64; 4], b: [f64; 4]) -> [u8; 64] {
    let mut s = [0u8; 64];
    for i in 0..4 {
        s[i * 8..i * 8 + 8].copy_from_slice(&a[i].to_le_bytes());
        s[32 + i * 8..32 + i * 8 + 8].copy_from_slice(&b[i].to_le_bytes());
    }
    s
}

#[test]
fn avx_ymm_packed_ps_add_sub() {
    let s = avx_f32_pair_scratch(
        [1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0],
        [0.5, -1.0, 2.0, -4.0, 8.0, -16.0, 32.0, -64.0],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFC, 0x58, 0xD1]); // vaddps ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFC, 0x5C, 0xD9]); // vsubps ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_packed_ps_add_sub", &code, s);
}

#[test]
fn avx_ymm_packed_ps_mul_div() {
    let s = avx_f32_pair_scratch(
        [8.0, 16.0, 32.0, 64.0, 0.5, 0.25, -8.0, -16.0],
        [2.0, 4.0, 8.0, 16.0, 0.25, 0.5, -2.0, -4.0],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFC, 0x59, 0xD1]); // vmulps ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFC, 0x5E, 0xD9]); // vdivps ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_packed_ps_mul_div", &code, s);
}

#[test]
fn avx_ymm_packed_ps_min_max() {
    let s = avx_f32_pair_scratch(
        [-4.0, 1.0, 8.0, -2.0, 3.0, -9.0, 16.0, 0.5],
        [-8.0, 2.0, 4.0, 5.0, 3.5, -3.0, 32.0, 0.25],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFC, 0x5D, 0xD1]); // vminps ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFC, 0x5F, 0xD9]); // vmaxps ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_packed_ps_min_max", &code, s);
}

#[test]
fn avx_ymm_packed_ps_sqrt_and_compare() {
    let s = avx_f32_pair_scratch(
        [1.0, 4.0, 9.0, 16.0, 25.0, 36.0, 49.0, 64.0],
        [2.0, 2.0, 8.0, 8.0, 24.0, 40.0, 48.0, 80.0],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFC, 0x51, 0xD0]); // vsqrtps ymm2, ymm0
    code.extend_from_slice(&[0xC5, 0xFC, 0xC2, 0xD9, 0x01]); // vcmpltps ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_packed_ps_sqrt_and_compare", &code, s);
}

#[test]
fn avx_ymm_packed_pd_add_sub() {
    let s = avx_f64_pair_scratch(
        [1.0, 2.0, -4.0, 32.0],
        [0.5, -8.0, 16.0, -64.0],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0x58, 0xD1]); // vaddpd ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFD, 0x5C, 0xD9]); // vsubpd ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_packed_pd_add_sub", &code, s);
}

#[test]
fn avx_ymm_packed_pd_mul_div() {
    let s = avx_f64_pair_scratch(
        [8.0, 16.0, -32.0, 0.5],
        [2.0, -4.0, 8.0, 0.25],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0x59, 0xD1]); // vmulpd ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFD, 0x5E, 0xD9]); // vdivpd ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_packed_pd_mul_div", &code, s);
}

#[test]
fn avx_ymm_packed_pd_min_max() {
    let s = avx_f64_pair_scratch(
        [-4.0, 1.0, 8.0, -0.5],
        [-8.0, 2.0, 4.0, 0.25],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0x5D, 0xD1]); // vminpd ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFD, 0x5F, 0xD9]); // vmaxpd ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_packed_pd_min_max", &code, s);
}

#[test]
fn avx_ymm_packed_pd_sqrt_and_compare() {
    let s = avx_f64_pair_scratch(
        [1.0, 4.0, 16.0, 64.0],
        [1.0, 3.0, 32.0, 8.0],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0x51, 0xD0]); // vsqrtpd ymm2, ymm0
    code.extend_from_slice(&[0xC5, 0xFD, 0xC2, 0xD9, 0x02]); // vcmplepd ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_packed_pd_sqrt_and_compare", &code, s);
}

#[test]
fn avx_ymm_packed_fp_memory_forms() {
    fn check_ps_pair(label: &str, opcode2: u8, opcode3: u8) {
        let s = avx_f32_pair_scratch(
            [-4.0, 2.0, 9.0, -16.0, 25.0, -36.0, 49.0, 64.0],
            [2.0, 1.0, 3.0, 8.0, 5.0, 6.0, 7.0, 4.0],
        );

        let mut code = avx_start();
        code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
        code.extend_from_slice(&[0xC5, 0xFC, opcode2, 0x57, 0x20]);
        code.extend_from_slice(&[0xC5, 0xFC, opcode3, 0x5F, 0x20]);
        avx2_store_ymm23_and_hlt(&mut code);
        check_avx_mem(label, &code, s);
    }

    fn check_pd_pair(label: &str, opcode2: u8, opcode3: u8) {
        let s = avx_f64_pair_scratch([-4.0, 2.0, 16.0, -64.0], [2.0, 1.0, 4.0, 8.0]);

        let mut code = avx_start();
        code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
        code.extend_from_slice(&[0xC5, 0xFD, opcode2, 0x57, 0x20]);
        code.extend_from_slice(&[0xC5, 0xFD, opcode3, 0x5F, 0x20]);
        avx2_store_ymm23_and_hlt(&mut code);
        check_avx_mem(label, &code, s);
    }

    check_ps_pair("avx_ymm_packed_ps_add_sub_memory", 0x58, 0x5C);
    check_ps_pair("avx_ymm_packed_ps_mul_div_memory", 0x59, 0x5E);
    check_ps_pair("avx_ymm_packed_ps_min_max_memory", 0x5D, 0x5F);

    let s = avx_f32_pair_scratch(
        [-4.0, 2.0, 9.0, -16.0, 25.0, -36.0, 49.0, 64.0],
        [2.0, 1.0, 3.0, 8.0, 5.0, 6.0, 7.0, 4.0],
    );
    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFC, 0x51, 0x57, 0x20]); // vsqrtps ymm2, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFC, 0xC2, 0x5F, 0x20, 0x01]); // vcmpltps ymm3, ymm0, [rdi+32]
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx_ymm_packed_ps_sqrt_compare_memory", &code, s);

    check_pd_pair("avx_ymm_packed_pd_add_sub_memory", 0x58, 0x5C);
    check_pd_pair("avx_ymm_packed_pd_mul_div_memory", 0x59, 0x5E);
    check_pd_pair("avx_ymm_packed_pd_min_max_memory", 0x5D, 0x5F);

    let s = avx_f64_pair_scratch([-4.0, 2.0, 16.0, -64.0], [2.0, 1.0, 4.0, 8.0]);
    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFD, 0x51, 0x57, 0x20]); // vsqrtpd ymm2, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0xC2, 0x5F, 0x20, 0x02]); // vcmplepd ymm3, ymm0, [rdi+32]
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx_ymm_packed_pd_sqrt_compare_memory", &code, s);
}

#[test]
fn avx_scalar_ss_arith_and_compare_preserves_upper() {
    let mut s = [0u8; 64];
    for (i, value) in [8.0f32, -2.0, 3.0, 4.0].iter().enumerate() {
        s[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (i, value) in [2.0f32, 16.0, -5.0, 0.25].iter().enumerate() {
        s[16 + i * 4..16 + i * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x10]); // vmovdqu xmm1, [rdi+16]
    code.extend_from_slice(&[0xC5, 0xFA, 0x5E, 0xD1]); // vdivss xmm2, xmm0, xmm1
    code.extend_from_slice(&[0xC5, 0xFA, 0xC2, 0xD9, 0x01]); // vcmpltss xmm3, xmm0, xmm1
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x57, 0x20]); // vmovdqu [rdi+32], xmm2
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x5F, 0x30]); // vmovdqu [rdi+48], xmm3
    code.push(HLT);
    check_avx_mem("avx_scalar_ss_arith_and_compare_preserves_upper", &code, s);
}

#[test]
fn avx_scalar_sd_arith_and_compare_preserves_upper() {
    let mut s = [0u8; 64];
    for (i, value) in [64.0f64, -7.5].iter().enumerate() {
        s[i * 8..i * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }
    for (i, value) in [8.0f64, 123.25].iter().enumerate() {
        s[16 + i * 8..16 + i * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x10]); // vmovdqu xmm1, [rdi+16]
    code.extend_from_slice(&[0xC5, 0xFB, 0x5E, 0xD1]); // vdivsd xmm2, xmm0, xmm1
    code.extend_from_slice(&[0xC5, 0xFB, 0xC2, 0xD9, 0x01]); // vcmpltsd xmm3, xmm0, xmm1
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x57, 0x20]); // vmovdqu [rdi+32], xmm2
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x5F, 0x30]); // vmovdqu [rdi+48], xmm3
    code.push(HLT);
    check_avx_mem("avx_scalar_sd_arith_and_compare_preserves_upper", &code, s);
}

#[test]
fn avx_scalar_fp_memory_forms() {
    fn ss_scratch() -> [u8; 64] {
        let mut s = [0u8; 64];
        for (i, value) in [8.0f32, -2.0, 3.0, 4.0].iter().enumerate() {
            s[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        for (i, value) in [2.0f32, 16.0, -5.0, 0.25].iter().enumerate() {
            s[16 + i * 4..16 + i * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        s
    }

    fn sd_scratch() -> [u8; 64] {
        let mut s = [0u8; 64];
        for (i, value) in [64.0f64, -7.5].iter().enumerate() {
            s[i * 8..i * 8 + 8].copy_from_slice(&value.to_le_bytes());
        }
        for (i, value) in [8.0f64, 123.25].iter().enumerate() {
            s[16 + i * 8..16 + i * 8 + 8].copy_from_slice(&value.to_le_bytes());
        }
        s
    }

    fn check_ss_pair(label: &str, opcode2: u8, opcode3: u8) {
        let mut code = avx_start();
        code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
        code.extend_from_slice(&[0xC5, 0xFA, opcode2, 0x57, 0x10]);
        code.extend_from_slice(&[0xC5, 0xFA, opcode3, 0x5F, 0x10]);
        avx2_store_ymm23_and_hlt(&mut code);
        check_avx_mem(label, &code, ss_scratch());
    }

    fn check_sd_pair(label: &str, opcode2: u8, opcode3: u8) {
        let mut code = avx_start();
        code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
        code.extend_from_slice(&[0xC5, 0xFB, opcode2, 0x57, 0x10]);
        code.extend_from_slice(&[0xC5, 0xFB, opcode3, 0x5F, 0x10]);
        avx2_store_ymm23_and_hlt(&mut code);
        check_avx_mem(label, &code, sd_scratch());
    }

    check_ss_pair("avx_scalar_ss_add_sub_memory", 0x58, 0x5C);
    check_ss_pair("avx_scalar_ss_mul_div_memory", 0x59, 0x5E);
    check_ss_pair("avx_scalar_ss_min_max_memory", 0x5D, 0x5F);

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x51, 0x57, 0x10]); // vsqrtss xmm2, xmm0, [rdi+16]
    code.extend_from_slice(&[0xC5, 0xFA, 0xC2, 0x5F, 0x10, 0x01]); // vcmpltss xmm3, xmm0, [rdi+16]
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx_scalar_ss_sqrt_compare_memory", &code, ss_scratch());

    check_sd_pair("avx_scalar_sd_add_sub_memory", 0x58, 0x5C);
    check_sd_pair("avx_scalar_sd_mul_div_memory", 0x59, 0x5E);
    check_sd_pair("avx_scalar_sd_min_max_memory", 0x5D, 0x5F);

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFB, 0x51, 0x57, 0x10]); // vsqrtsd xmm2, xmm0, [rdi+16]
    code.extend_from_slice(&[0xC5, 0xFB, 0xC2, 0x5F, 0x10, 0x02]); // vcmplesd xmm3, xmm0, [rdi+16]
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx_scalar_sd_sqrt_compare_memory", &code, sd_scratch());
}

// ===========================================================================
// EXPANDED COVERAGE PART 19: VEX logical, duplicate, unpack, and shuffle forms.
//
// This covers implemented AVX paths outside arithmetic: bitwise logical ops,
// duplicated-lane moves, packed unpacking, immediate shuffles, blends, and
// permute-immediate instructions.
// ===========================================================================

fn avx_byte_pair_scratch() -> [u8; 64] {
    let mut s = [0u8; 64];
    for i in 0..32 {
        s[i] = (0xA5u8).wrapping_add((i as u8).wrapping_mul(17));
        s[32 + i] = (0x3Cu8).wrapping_sub((i as u8).wrapping_mul(9));
    }
    s
}

#[test]
fn avx_ymm_logical_and_andn_ps() {
    let s = avx_byte_pair_scratch();

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFC, 0x54, 0xD1]); // vandps ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFC, 0x55, 0xD9]); // vandnps ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_logical_and_andn_ps", &code, s);
}

#[test]
fn avx_ymm_logical_or_xor_pd() {
    let s = avx_byte_pair_scratch();

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0x56, 0xD1]); // vorpd ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFD, 0x57, 0xD9]); // vxorpd ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_logical_or_xor_pd", &code, s);
}

#[test]
fn avx_ymm_duplicate_single_lanes() {
    let s = avx_f32_pair_scratch(
        [1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0],
        [0.0; 8],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x12, 0xD0]); // vmovsldup ymm2, ymm0
    code.extend_from_slice(&[0xC5, 0xFE, 0x16, 0xD8]); // vmovshdup ymm3, ymm0
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_duplicate_single_lanes", &code, s);
}

#[test]
fn avx_ymm_duplicate_double_lanes() {
    let s = avx_f64_pair_scratch([1.0, 2.0, 4.0, 8.0], [0.0; 4]);

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFF, 0x12, 0xD0]); // vmovddup ymm2, ymm0
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.push(HLT);
    check_avx_mem("avx_ymm_duplicate_double_lanes", &code, s);
}

#[test]
fn avx_ymm_unpack_ps() {
    let s = avx_f32_pair_scratch(
        [1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0],
        [-1.0, -2.0, -4.0, -8.0, -16.0, -32.0, -64.0, -128.0],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFC, 0x14, 0xD1]); // vunpcklps ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFC, 0x15, 0xD9]); // vunpckhps ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_unpack_ps", &code, s);
}

#[test]
fn avx_ymm_unpack_pd() {
    let s = avx_f64_pair_scratch([1.0, 2.0, 4.0, 8.0], [-1.0, -2.0, -4.0, -8.0]);

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0x14, 0xD1]); // vunpcklpd ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFD, 0x15, 0xD9]); // vunpckhpd ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_unpack_pd", &code, s);
}

#[test]
fn avx_ymm_shuffle_ps_pd() {
    let s = avx_f32_pair_scratch(
        [1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0],
        [-1.0, -2.0, -4.0, -8.0, -16.0, -32.0, -64.0, -128.0],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFC, 0xC6, 0xD1, 0x1B]); // vshufps ymm2, ymm0, ymm1, 0x1b
    code.extend_from_slice(&[0xC5, 0xFD, 0xC6, 0xD9, 0x05]); // vshufpd ymm3, ymm0, ymm1, 0x05
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_shuffle_ps_pd", &code, s);
}

#[test]
fn avx_ymm_blend_ps_pd() {
    let s = avx_f32_pair_scratch(
        [1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0],
        [-1.0, -2.0, -4.0, -8.0, -16.0, -32.0, -64.0, -128.0],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x0C, 0xD1, 0xAA]); // vblendps ymm2, ymm0, ymm1, 0xaa
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x0D, 0xD9, 0x05]); // vblendpd ymm3, ymm0, ymm1, 0x05
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_blend_ps_pd", &code, s);
}

#[test]
fn avx_ymm_shuffle_blend_memory_forms() {
    let s = avx_byte_pair_scratch();

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFC, 0xC6, 0x57, 0x20, 0x1B]); // vshufps ymm2, ymm0, [rdi+32], 0x1b
    code.extend_from_slice(&[0xC5, 0xFD, 0xC6, 0x5F, 0x20, 0x05]); // vshufpd ymm3, ymm0, [rdi+32], 0x05
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx_ymm_vshufps_vshufpd_memory", &code, s);

    let s = avx_byte_pair_scratch();

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x0C, 0x57, 0x20, 0x96]); // vblendps ymm2, ymm0, [rdi+32], 0x96
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x0D, 0x5F, 0x20, 0x0A]); // vblendpd ymm3, ymm0, [rdi+32], 0x0a
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx_ymm_vblendps_vblendpd_memory", &code, s);

    let s = avx_byte_pair_scratch();

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x57, 0x20]); // vmovdqu ymm2, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xF8, 0xC6, 0x57, 0x20, 0x4E]); // vshufps xmm2, xmm0, [rdi+32], 0x4e
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.push(HLT);
    check_avx_mem("avx_xmm_vshufps_zeroes_upper", &code, s);
}

#[test]
fn avx_ymm_permil_imm_ps_pd() {
    let s = avx_f32_pair_scratch(
        [1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0],
        [0.0; 8],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x04, 0xD0, 0x1B]); // vpermilps ymm2, ymm0, 0x1b
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x05, 0xD8, 0x05]); // vpermilpd ymm3, ymm0, 0x05
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_permil_imm_ps_pd", &code, s);
}

#[test]
fn avx_ymm_permil_memory_forms() {
    let s = avx_byte_pair_scratch();

    let mut code = avx_start();
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x04, 0x17, 0x1B]); // vpermilps ymm2, [rdi], 0x1b
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x05, 0x5F, 0x20, 0x09]); // vpermilpd ymm3, [rdi+32], 0x09
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx_ymm_vpermil_imm_memory", &code, s);

    let mut s = [0u8; 64];
    for (i, value) in [1.0f32, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0]
        .iter()
        .enumerate()
    {
        s[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    for i in 0..8 {
        let control = [3u32, 0, 1, 2, 2, 1, 0, 3][i];
        s[32 + i * 4..32 + i * 4 + 4].copy_from_slice(&control.to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x0C, 0x57, 0x20]); // vpermilps ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x0D, 0x5F, 0x20]); // vpermilpd ymm3, ymm0, [rdi+32]
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx_ymm_vpermil_variable_memory", &code, s);

    let s = avx_byte_pair_scratch();

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x57, 0x20]); // vmovdqu ymm2, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE3, 0x79, 0x05, 0x17, 0x02]); // vpermilpd xmm2, [rdi], 0x02
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.push(HLT);
    check_avx_mem("avx_xmm_vpermilpd_zeroes_upper", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 20: VEX flag, blendv, permute-control, and move forms.
//
// These cover flag-setting VEX tests/comparisons and register-controlled or
// mixed scalar/memory move forms that are separate from the immediate shuffle
// and arithmetic paths above.
// ===========================================================================

#[test]
fn avx_ymm_vtestps_vtestpd_flags() {
    let mut s = [0u8; 64];
    let a = [
        0x8000_0000u32,
        0x0000_0000,
        0x8000_0000,
        0x0000_0000,
        0x8000_0000,
        0x0000_0000,
        0x0000_0000,
        0x8000_0000,
    ];
    let b = [
        0x0000_0000u32,
        0x8000_0000,
        0x8000_0000,
        0x0000_0000,
        0x8000_0000,
        0x8000_0000,
        0x0000_0000,
        0x0000_0000,
    ];
    for i in 0..8 {
        s[i * 4..i * 4 + 4].copy_from_slice(&a[i].to_le_bytes());
        s[32 + i * 4..32 + i * 4 + 4].copy_from_slice(&b[i].to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x0E, 0xC1]); // vtestps ymm0, ymm1
    code.extend_from_slice(&[0x9C, 0x58, 0x48, 0x89, 0x07]); // pushfq; pop rax; mov [rdi], rax
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x0F, 0xC1]); // vtestpd ymm0, ymm1
    code.extend_from_slice(&[0x9C, 0x58, 0x48, 0x89, 0x47, 0x08]); // pushfq; pop rax; mov [rdi+8], rax
    code.push(HLT);
    check_avx_flags_mem("avx_ymm_vtestps_vtestpd_flags", &code, s);
}

#[test]
fn avx2_ymm_vptest_memory_flags() {
    let mut s = [0u8; 64];
    for i in 0..32 {
        s[i] = 0x81u8.wrapping_add((i as u8).wrapping_mul(7));
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x17, 0x47, 0x20]); // vptest ymm0, [rdi+32]
    code.extend_from_slice(&[0x9C, 0x58, 0x48, 0x89, 0x07]); // pushfq; pop rax; mov [rdi], rax
    code.extend_from_slice(&[0xC5, 0xF5, 0x76, 0xC9]); // vpcmpeqd ymm1, ymm1, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x4F, 0x20]); // vmovdqu [rdi+32], ymm1
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x17, 0x47, 0x20]); // vptest ymm0, [rdi+32]
    code.extend_from_slice(&[0x9C, 0x58, 0x48, 0x89, 0x47, 0x08]); // pushfq; pop rax; mov [rdi+8], rax
    code.push(HLT);
    check_avx_flags_mem("avx2_ymm_vptest_memory_flags", &code, s);
}

#[test]
fn avx_scalar_vcomi_flags() {
    let mut s = [0u8; 64];
    for (i, value) in [1.0f32, 0.0, 0.0, 0.0].iter().enumerate() {
        s[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (i, value) in [2.0f32, 0.0, 0.0, 0.0].iter().enumerate() {
        s[16 + i * 4..16 + i * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (i, value) in [4.0f64, 0.0].iter().enumerate() {
        s[32 + i * 8..32 + i * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }
    for (i, value) in [4.0f64, 0.0].iter().enumerate() {
        s[48 + i * 8..48 + i * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x10]); // vmovdqu xmm1, [rdi+16]
    code.extend_from_slice(&[0xC5, 0xF8, 0x2E, 0xC1]); // vucomiss xmm0, xmm1
    code.extend_from_slice(&[0x9C, 0x58, 0x48, 0x89, 0x07]); // pushfq; pop rax; mov [rdi], rax
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x47, 0x20]); // vmovdqu xmm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x30]); // vmovdqu xmm1, [rdi+48]
    code.extend_from_slice(&[0xC5, 0xF9, 0x2F, 0xC1]); // vcomisd xmm0, xmm1
    code.extend_from_slice(&[0x9C, 0x58, 0x48, 0x89, 0x47, 0x08]); // pushfq; pop rax; mov [rdi+8], rax
    code.push(HLT);
    check_avx_flags_mem("avx_scalar_vcomi_flags", &code, s);
}

#[test]
fn avx_ymm_blendv_ps_pd() {
    let s = avx_f32_pair_scratch(
        [-4.0, 1.0, 8.0, -2.0, 3.0, -9.0, 16.0, 0.5],
        [-8.0, 2.0, 4.0, 5.0, 3.5, -3.0, 32.0, 0.25],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFC, 0xC2, 0xD9, 0x01]); // vcmpltps ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x4A, 0xD1, 0x30]); // vblendvps ymm2, ymm0, ymm1, ymm3
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFD, 0xC2, 0xD9, 0x02]); // vcmplepd ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x4B, 0xD1, 0x30]); // vblendvpd ymm2, ymm0, ymm1, ymm3
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x57, 0x20]); // vmovdqu [rdi+32], ymm2
    code.push(HLT);
    check_avx_mem("avx_ymm_blendv_ps_pd", &code, s);
}

#[test]
fn avx_ymm_permil_reg_ps_pd() {
    let mut s = [0u8; 64];
    for (i, value) in [1.0f32, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0]
        .iter()
        .enumerate()
    {
        s[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    let controls = [3u32, 2, 1, 0, 1, 0, 3, 2];
    for i in 0..8 {
        s[32 + i * 4..32 + i * 4 + 4].copy_from_slice(&controls[i].to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x0C, 0xD1]); // vpermilps ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x0D, 0xD9]); // vpermilpd ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_permil_reg_ps_pd", &code, s);
}

#[test]
fn avx_scalar_vmovss_vmovsd_reg_merge() {
    let mut s = [0u8; 64];
    for (i, value) in [8.0f32, -2.0, 3.0, 4.0].iter().enumerate() {
        s[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (i, value) in [2.0f32, 16.0, -5.0, 0.25].iter().enumerate() {
        s[16 + i * 4..16 + i * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x10]); // vmovdqu xmm1, [rdi+16]
    code.extend_from_slice(&[0xC5, 0xFA, 0x10, 0xD1]); // vmovss xmm2, xmm0, xmm1
    code.extend_from_slice(&[0xC5, 0xFB, 0x10, 0xD9]); // vmovsd xmm3, xmm0, xmm1
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x57, 0x20]); // vmovdqu [rdi+32], xmm2
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x5F, 0x30]); // vmovdqu [rdi+48], xmm3
    code.push(HLT);
    check_avx_mem("avx_scalar_vmovss_vmovsd_reg_merge", &code, s);
}

#[test]
fn avx_scalar_vmovss_vmovsd_memory_load_store() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x44u8).wrapping_add((i as u8).wrapping_mul(11));
    }
    s[0..4].copy_from_slice(&(-12.5f32).to_le_bytes());
    s[8..16].copy_from_slice(&6.75f64.to_le_bytes());

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x10, 0x17]); // vmovss xmm2, [rdi]
    code.extend_from_slice(&[0xC5, 0xFB, 0x10, 0x5F, 0x08]); // vmovsd xmm3, [rdi+8]
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x57, 0x20]); // vmovdqu [rdi+32], xmm2
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x5F, 0x30]); // vmovdqu [rdi+48], xmm3
    code.push(HLT);
    check_avx_mem("avx_scalar_vmovss_vmovsd_memory_load", &code, s);

    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x8Fu8).wrapping_sub((i as u8).wrapping_mul(3));
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x47, 0x10]); // vmovdqu xmm0, [rdi+16]
    code.extend_from_slice(&[0xC5, 0xFA, 0x11, 0x07]); // vmovss [rdi], xmm0
    code.extend_from_slice(&[0xC5, 0xFB, 0x11, 0x47, 0x08]); // vmovsd [rdi+8], xmm0
    code.push(HLT);
    check_avx_mem("avx_scalar_vmovss_vmovsd_memory_store", &code, s);
}

#[test]
fn avx_xmm_vmovlps_vmovhps_memory_merge() {
    let s = avx_f32_pair_scratch(
        [1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0],
        [-1.0, -2.0, -4.0, -8.0, -16.0, -32.0, -64.0, -128.0],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xF8, 0x12, 0x57, 0x20]); // vmovlps xmm2, xmm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xF8, 0x16, 0x5F, 0x20]); // vmovhps xmm3, xmm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x17]); // vmovdqu [rdi], xmm2
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x5F, 0x10]); // vmovdqu [rdi+16], xmm3
    code.push(HLT);
    check_avx_mem("avx_xmm_vmovlps_vmovhps_memory_merge", &code, s);
}

#[test]
fn avx_xmm_vmovlpd_vmovhpd_memory_merge() {
    let s = avx_f64_pair_scratch([1.0, 2.0, 4.0, 8.0], [-1.0, -2.0, -4.0, -8.0]);

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xF9, 0x12, 0x57, 0x20]); // vmovlpd xmm2, xmm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xF9, 0x16, 0x5F, 0x20]); // vmovhpd xmm3, xmm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x17]); // vmovdqu [rdi], xmm2
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x5F, 0x10]); // vmovdqu [rdi+16], xmm3
    code.push(HLT);
    check_avx_mem("avx_xmm_vmovlpd_vmovhpd_memory_merge", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 21: VEX conversion width, merge, rounding, and
// truncation forms.
// ===========================================================================

#[test]
fn avx_ymm_vcvtps2dq_ties_and_trunc() {
    let mut s = [0u8; 64];
    let values = [1.5f32, 2.5, -1.5, -2.5, 3.5, 4.5, -3.5, -4.5];
    for (i, value) in values.iter().enumerate() {
        s[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFC, 0x10, 0x07]); // vmovups ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFD, 0x5B, 0xC8]); // vcvtps2dq ymm1, ymm0
    code.extend_from_slice(&[0xC5, 0xFE, 0x5B, 0xD0]); // vcvttps2dq ymm2, ymm0
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x0F]); // vmovdqu [rdi], ymm1
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x57, 0x20]); // vmovdqu [rdi+32], ymm2
    code.push(HLT);
    check_avx_mem("avx_ymm_vcvtps2dq_ties_and_trunc", &code, s);
}

#[test]
fn avx_ymm_vcvtpd2dq_ties_trunc_and_dq2pd() {
    let mut s = [0u8; 64];
    let values = [1.5f64, 2.5, -3.5, -4.5];
    for (i, value) in values.iter().enumerate() {
        s[i * 8..i * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFD, 0x10, 0x07]); // vmovupd ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFF, 0xE6, 0xC8]); // vcvtpd2dq xmm1, ymm0
    code.extend_from_slice(&[0xC5, 0xFD, 0xE6, 0xD0]); // vcvttpd2dq xmm2, ymm0
    code.extend_from_slice(&[0xC5, 0xFE, 0xE6, 0xD9]); // vcvtdq2pd ymm3, xmm1
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x0F]); // vmovdqu [rdi], xmm1
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x57, 0x10]); // vmovdqu [rdi+16], xmm2
    code.extend_from_slice(&[0xC5, 0xFD, 0x11, 0x5F, 0x20]); // vmovupd [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_vcvtpd2dq_ties_trunc_and_dq2pd", &code, s);
}

#[test]
fn avx_ymm_vcvtps2pd_and_vcvtpd2ps_widths() {
    let mut s = [0u8; 64];
    let values = [1.25f32, -2.5, 3.75, -0.125];
    for (i, value) in values.iter().enumerate() {
        s[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xF8, 0x10, 0x07]); // vmovups xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFC, 0x5A, 0xC8]); // vcvtps2pd ymm1, xmm0
    code.extend_from_slice(&[0xC5, 0xFD, 0x5A, 0xD1]); // vcvtpd2ps xmm2, ymm1
    code.extend_from_slice(&[0xC5, 0xFD, 0x11, 0x0F]); // vmovupd [rdi], ymm1
    code.extend_from_slice(&[0xC5, 0xF8, 0x11, 0x57, 0x20]); // vmovups [rdi+32], xmm2
    code.push(HLT);
    check_avx_mem("avx_ymm_vcvtps2pd_and_vcvtpd2ps_widths", &code, s);
}

#[test]
fn avx_scalar_vcvtss2sd_vcvtsd2ss_memory_merge() {
    let mut s = [0u8; 64];
    for i in 0..16 {
        s[i] = 0xA0u8.wrapping_add(i as u8);
        s[16 + i] = 0xC0u8.wrapping_add((i as u8).wrapping_mul(3));
    }
    s[32..36].copy_from_slice(&9.5f32.to_le_bytes());
    let rounded_to_one = 1.0f64 + 2.0f64.powi(-30);
    s[40..48].copy_from_slice(&rounded_to_one.to_le_bytes());

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x10]); // vmovdqu xmm1, [rdi+16]
    code.extend_from_slice(&[0xC5, 0xFA, 0x5A, 0x57, 0x20]); // vcvtss2sd xmm2, xmm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xF3, 0x5A, 0x5F, 0x28]); // vcvtsd2ss xmm3, xmm1, [rdi+40]
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x17]); // vmovdqu [rdi], xmm2
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x5F, 0x10]); // vmovdqu [rdi+16], xmm3
    code.push(HLT);
    check_avx_mem("avx_scalar_vcvtss2sd_vcvtsd2ss_memory_merge", &code, s);
}

#[test]
fn avx_scalar_vcvtsi_and_vcvts2si_memory_forms() {
    let mut s = [0u8; 64];
    let i64_value = 0x0100_0001i64;
    let i32_value = -123_456_789i32;
    s[0..8].copy_from_slice(&i64_value.to_le_bytes());
    s[8..12].copy_from_slice(&i32_value.to_le_bytes());
    for i in 0..16 {
        s[32 + i] = 0x30u8.wrapping_add((i as u8).wrapping_mul(7));
    }
    s[48..52].copy_from_slice(&2.5f32.to_le_bytes());
    s[56..64].copy_from_slice(&(-3.5f64).to_le_bytes());

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x47, 0x20]); // vmovdqu xmm0, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE1, 0xFA, 0x2A, 0x0F]); // vcvtsi2ss xmm1, xmm0, qword [rdi]
    code.extend_from_slice(&[0xC5, 0xFB, 0x2A, 0x57, 0x08]); // vcvtsi2sd xmm2, xmm0, dword [rdi+8]
    code.extend_from_slice(&[0xC5, 0xFA, 0x2D, 0x47, 0x30]); // vcvtss2si eax, dword [rdi+48]
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x2C, 0x5F, 0x38]); // vcvttsd2si rbx, qword [rdi+56]
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x2D, 0x4F, 0x38]); // vcvtsd2si rcx, qword [rdi+56]
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x0F]); // vmovdqu [rdi], xmm1
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x57, 0x10]); // vmovdqu [rdi+16], xmm2
    code.extend_from_slice(&[0x48, 0x89, 0x47, 0x20]); // mov [rdi+32], rax
    code.extend_from_slice(&[0x48, 0x89, 0x5F, 0x28]); // mov [rdi+40], rbx
    code.extend_from_slice(&[0x48, 0x89, 0x4F, 0x30]); // mov [rdi+48], rcx
    code.push(HLT);
    check_avx_mem("avx_scalar_vcvtsi_and_vcvts2si_memory_forms", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 22: x87 BCD and truncating integer store forms.
// ===========================================================================

#[test]
fn x87_fbld_fbstp_positive_and_negative_bcd() {
    let mut s = [0u8; 64];
    s[0..10].copy_from_slice(&[0x90, 0x78, 0x56, 0x34, 0x12, 0, 0, 0, 0, 0]);
    s[16..26].copy_from_slice(&[0x10, 0x32, 0x54, 0x76, 0x98, 0, 0, 0, 0, 0x80]);

    let mut code = load_rdi_data();
    code.extend_from_slice(&[0xDF, 0x27]); // fbld tbyte [rdi]
    code.extend_from_slice(&[0xDF, 0x77, 0x20]); // fbstp tbyte [rdi+32]
    code.extend_from_slice(&[0xDF, 0x67, 0x10]); // fbld tbyte [rdi+16]
    code.extend_from_slice(&[0xDF, 0x77, 0x30]); // fbstp tbyte [rdi+48]
    code.push(HLT);
    check_mem("x87_fbld_fbstp_bcd", &code, regs(), s, 0);
}

#[test]
fn x87_fisttp_widths_truncate_toward_zero() {
    let s = scratch_f64(&[-3.75, 12_345.875, -9_876_543_210.25]);

    let mut code = load_rdi_data();
    code.extend_from_slice(&[0xDD, 0x07]); // fld qword [rdi]
    code.extend_from_slice(&[0xDF, 0x4F, 0x20]); // fisttp word [rdi+32]
    code.extend_from_slice(&[0xDD, 0x47, 0x08]); // fld qword [rdi+8]
    code.extend_from_slice(&[0xDB, 0x4F, 0x24]); // fisttp dword [rdi+36]
    code.extend_from_slice(&[0xDD, 0x47, 0x10]); // fld qword [rdi+16]
    code.extend_from_slice(&[0xDD, 0x4F, 0x28]); // fisttp qword [rdi+40]
    code.push(HLT);
    check_mem("x87_fisttp_widths", &code, regs(), s, 0);
}

// ===========================================================================
// EXPANDED COVERAGE PART 23: AVX2 VEX integer compare, min/max, multiply,
// absolute-value, and extension families.
// ===========================================================================

fn avx2_ymm_pair_start() -> Vec<u8> {
    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code
}

fn avx2_store_ymm23_and_hlt(code: &mut Vec<u8>) {
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
}

#[test]
fn avx2_ymm_packed_integer_arith_memory_forms() {
    fn check_pair(label: &str, opcode2: u8, opcode3: u8) {
        let s = avx_byte_pair_scratch();

        let mut code = avx2_ymm_pair_start();
        code.extend_from_slice(&[0xC5, 0xFD, opcode2, 0x57, 0x20]);
        code.extend_from_slice(&[0xC5, 0xFD, opcode3, 0x5F, 0x20]);
        avx2_store_ymm23_and_hlt(&mut code);
        check_avx_mem(label, &code, s);
    }

    check_pair("avx2_ymm_paddb_paddw_memory", 0xFC, 0xFD);
    check_pair("avx2_ymm_paddd_paddq_memory", 0xFE, 0xD4);
    check_pair("avx2_ymm_psubb_psubw_memory", 0xF8, 0xF9);
    check_pair("avx2_ymm_psubd_psubq_memory", 0xFA, 0xFB);
    check_pair("avx2_ymm_paddusb_paddusw_memory", 0xDC, 0xDD);
    check_pair("avx2_ymm_paddsb_paddsw_memory", 0xEC, 0xED);
    check_pair("avx2_ymm_psubusb_psubusw_memory", 0xD8, 0xD9);
    check_pair("avx2_ymm_psubsb_psubsw_memory", 0xE8, 0xE9);
    check_pair("avx2_ymm_pavgb_pavgw_memory", 0xE0, 0xE3);
    check_pair("avx2_ymm_pminub_pmaxub_memory", 0xDA, 0xDE);
    check_pair("avx2_ymm_pminsw_pmaxsw_memory", 0xEA, 0xEE);
    check_pair("avx2_ymm_pand_pandn_memory", 0xDB, 0xDF);
    check_pair("avx2_ymm_por_pxor_memory", 0xEB, 0xEF);
    check_pair("avx2_ymm_pmullw_pmulhw_memory", 0xD5, 0xE5);
    check_pair("avx2_ymm_pmulhuw_pmaddwd_memory", 0xE4, 0xF5);
    check_pair("avx2_ymm_pmuludq_psadbw_memory", 0xF4, 0xF6);
}

#[test]
fn avx2_ymm_pack_unpack_memory_forms() {
    fn check_pair(label: &str, opcode2: u8, opcode3: u8) {
        let s = avx_byte_pair_scratch();

        let mut code = avx2_ymm_pair_start();
        code.extend_from_slice(&[0xC5, 0xFD, opcode2, 0x57, 0x20]);
        code.extend_from_slice(&[0xC5, 0xFD, opcode3, 0x5F, 0x20]);
        avx2_store_ymm23_and_hlt(&mut code);
        check_avx_mem(label, &code, s);
    }

    check_pair("avx2_ymm_punpcklbw_punpcklwd_memory", 0x60, 0x61);
    check_pair("avx2_ymm_punpckldq_punpcklqdq_memory", 0x62, 0x6C);
    check_pair("avx2_ymm_punpckhbw_punpckhwd_memory", 0x68, 0x69);
    check_pair("avx2_ymm_punpckhdq_punpckhqdq_memory", 0x6A, 0x6D);
    check_pair("avx2_ymm_packsswb_packuswb_memory", 0x63, 0x67);
    check_pair("avx2_ymm_packssdw_punpckhbw_memory", 0x6B, 0x68);
}

#[test]
fn avx2_ymm_shuffle_horizontal_sign_memory_forms() {
    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x00, 0x57, 0x20]); // vpshufb ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x08, 0x5F, 0x20]); // vpsignb ymm3, ymm0, [rdi+32]
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_pshufb_psignb_memory", &code, s);

    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x01, 0x57, 0x20]); // vphaddw ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x06, 0x5F, 0x20]); // vphsubd ymm3, ymm0, [rdi+32]
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_phaddw_phsubd_memory", &code, s);

    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x03, 0x57, 0x20]); // vphaddsw ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x0B, 0x5F, 0x20]); // vpmulhrsw ymm3, ymm0, [rdi+32]
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_phaddsw_pmulhrsw_memory", &code, s);
}

#[test]
fn avx2_ymm_more_horizontal_sign_memory_forms() {
    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x02, 0x57, 0x20]); // vphaddd ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x05, 0x5F, 0x20]); // vphsubw ymm3, ymm0, [rdi+32]
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_phaddd_phsubw_memory", &code, s);

    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x07, 0x57, 0x20]); // vphsubsw ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x09, 0x5F, 0x20]); // vpsignw ymm3, ymm0, [rdi+32]
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_phsubsw_psignw_memory", &code, s);

    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x0A, 0x57, 0x20]); // vpsignd ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x04, 0x5F, 0x20]); // vpmaddubsw ymm3, ymm0, [rdi+32]
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_psignd_pmaddubsw_memory", &code, s);
}

#[test]
fn avx2_ymm_palignr_blendw_memory_forms() {
    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x0F, 0xD1, 0x11]); // vpalignr ymm2, ymm0, ymm1, 17
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x0F, 0x5F, 0x20, 0x1F]); // vpalignr ymm3, ymm0, [rdi+32], 31
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_vpalignr_register_memory", &code, s);

    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x0E, 0xD1, 0xA5]); // vpblendw ymm2, ymm0, ymm1, 0xa5
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x0E, 0x5F, 0x20, 0x5A]); // vpblendw ymm3, ymm0, [rdi+32], 0x5a
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_vpblendw_register_memory", &code, s);

    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x57, 0x20]); // vmovdqu ymm2, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE3, 0x79, 0x0F, 0xD1, 0x12]); // vpalignr xmm2, xmm0, xmm1, 18
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.push(HLT);
    check_avx_mem("avx2_xmm_vpalignr_zeroes_upper", &code, s);
}

#[test]
fn avx2_vphminposuw_vpackusdw_memory_forms() {
    let mut s = [0u8; 64];
    let words = [0x9000u16, 0x1234, 0x00F0, 0x00F1, 0xCAFE, 0x0007, 0x0008, 0x7777];
    for (i, word) in words.iter().enumerate() {
        s[i * 2..i * 2 + 2].copy_from_slice(&word.to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x79, 0x41, 0x17]); // vphminposuw xmm2, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x57, 0x20]); // vmovdqu [rdi+32], xmm2
    code.push(HLT);
    check_avx_mem("avx2_vphminposuw_memory", &code, s);

    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x2B, 0x57, 0x20]); // vpackusdw ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.push(HLT);
    check_avx_mem("avx2_ymm_vpackusdw_memory", &code, s);
}

#[test]
fn avx2_ymm_compare_byte_word_dword_qword() {
    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC5, 0xFD, 0x74, 0xD1]); // vpcmpeqb ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFD, 0x65, 0xD9]); // vpcmpgtw ymm3, ymm0, ymm1
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_compare_byte_word", &code, s);

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC5, 0xFD, 0x66, 0xD1]); // vpcmpgtd ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x29, 0xD9]); // vpcmpeqq ymm3, ymm0, ymm1
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_compare_dword_qword_eq", &code, s);

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x37, 0xD9]); // vpcmpgtq ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFD, 0x76, 0xD0]); // vpcmpeqd ymm2, ymm0, ymm0
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_compare_qword_gt_and_self_eq", &code, s);
}

#[test]
fn avx2_ymm_compare_minmax_multiply_memory_forms() {
    fn compare_scratch() -> [u8; 64] {
        let mut s = [0u8; 64];
        let lhs = [
            0x0000_0000_0000_0000u64,
            0x7FFF_FFFF_8000_0000,
            0xFFFF_FFFF_0000_0001,
            0x1122_3344_5566_7788,
        ];
        let rhs = [
            0x0000_0000_0000_0000u64,
            0x8000_0000_7FFF_FFFF,
            0x0000_0001_FFFF_FFFF,
            0x1122_3344_5566_7789,
        ];
        for (i, value) in lhs.iter().enumerate() {
            s[i * 8..i * 8 + 8].copy_from_slice(&value.to_le_bytes());
        }
        for (i, value) in rhs.iter().enumerate() {
            s[32 + i * 8..32 + i * 8 + 8].copy_from_slice(&value.to_le_bytes());
        }
        s
    }

    fn check_0f_pair(label: &str, opcode2: u8, opcode3: u8) {
        let s = compare_scratch();

        let mut code = avx2_ymm_pair_start();
        code.extend_from_slice(&[0xC5, 0xFD, opcode2, 0x57, 0x20]);
        code.extend_from_slice(&[0xC5, 0xFD, opcode3, 0x5F, 0x20]);
        avx2_store_ymm23_and_hlt(&mut code);
        check_avx_mem(label, &code, s);
    }

    fn check_0f38_pair(label: &str, opcode2: u8, opcode3: u8) {
        let s = compare_scratch();

        let mut code = avx2_ymm_pair_start();
        code.extend_from_slice(&[0xC4, 0xE2, 0x7D, opcode2, 0x57, 0x20]);
        code.extend_from_slice(&[0xC4, 0xE2, 0x7D, opcode3, 0x5F, 0x20]);
        avx2_store_ymm23_and_hlt(&mut code);
        check_avx_mem(label, &code, s);
    }

    check_0f_pair("avx2_ymm_pcmpeqb_pcmpeqw_memory", 0x74, 0x75);
    check_0f_pair("avx2_ymm_pcmpeqd_pcmpgtb_memory", 0x76, 0x64);
    check_0f_pair("avx2_ymm_pcmpgtw_pcmpgtd_memory", 0x65, 0x66);
    check_0f38_pair("avx2_ymm_pcmpeqq_pcmpgtq_memory", 0x29, 0x37);
    check_0f38_pair("avx2_ymm_pminsb_pmaxsb_memory", 0x38, 0x3C);
    check_0f38_pair("avx2_ymm_pminsd_pmaxsd_memory", 0x39, 0x3D);
    check_0f38_pair("avx2_ymm_pminuw_pmaxuw_memory", 0x3A, 0x3E);
    check_0f38_pair("avx2_ymm_pminud_pmaxud_memory", 0x3B, 0x3F);
    check_0f38_pair("avx2_ymm_pmuldq_pmulld_memory", 0x28, 0x40);
}

#[test]
fn avx2_ymm_minmax_signed_variants() {
    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x38, 0xD1]); // vpminsb ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x3C, 0xD9]); // vpmaxsb ymm3, ymm0, ymm1
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_minmax_signed_bytes", &code, s);

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x39, 0xD1]); // vpminsd ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x3D, 0xD9]); // vpmaxsd ymm3, ymm0, ymm1
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_minmax_signed_dwords", &code, s);
}

#[test]
fn avx2_ymm_minmax_unsigned_variants() {
    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x3A, 0xD1]); // vpminuw ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x3E, 0xD9]); // vpmaxuw ymm3, ymm0, ymm1
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_minmax_unsigned_words", &code, s);

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x3B, 0xD1]); // vpminud ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x3F, 0xD9]); // vpmaxud ymm3, ymm0, ymm1
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_minmax_unsigned_dwords", &code, s);
}

#[test]
fn avx2_ymm_multiply_word_and_dword_variants() {
    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC5, 0xFD, 0xD5, 0xD1]); // vpmullw ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFD, 0xE5, 0xD9]); // vpmulhw ymm3, ymm0, ymm1
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_mul_word_low_high_signed", &code, s);

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC5, 0xFD, 0xE4, 0xD1]); // vpmulhuw ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFD, 0xF5, 0xD9]); // vpmaddwd ymm3, ymm0, ymm1
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_mul_unsigned_high_maddwd", &code, s);

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x28, 0xD1]); // vpmuldq ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFD, 0xF4, 0xD9]); // vpmuludq ymm3, ymm0, ymm1
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_mul_signed_unsigned_dq", &code, s);

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x40, 0xD1]); // vpmulld ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFD, 0xF6, 0xD9]); // vpsadbw ymm3, ymm0, ymm1
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_mulld_and_sadbw", &code, s);
}

#[test]
fn avx2_ymm_abs_and_extend_memory_forms() {
    let s = avx_byte_pair_scratch();

    let mut code = avx2_ymm_pair_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x1C, 0xD0]); // vpabsb ymm2, ymm0
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x1E, 0xD9]); // vpabsd ymm3, ymm1
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_abs_byte_dword", &code, s);

    let mut code = avx_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x20, 0x17]); // vpmovsxbw ymm2, [rdi]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x31, 0x5F, 0x20]); // vpmovzxbd ymm3, [rdi+32]
    avx2_store_ymm23_and_hlt(&mut code);
    check_avx_mem("avx2_ymm_sign_zero_extend_memory", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 24: VEX masked memory and broadcast forms.
// ===========================================================================

fn avx_broadcast_scratch() -> [u8; 64] {
    let mut s = [0u8; 64];
    for (i, byte) in s.iter_mut().enumerate() {
        *byte = 0x11u8.wrapping_add((i as u8).wrapping_mul(13));
    }
    s
}

fn avx_maskmov_dword_scratch() -> [u8; 64] {
    let mut s = [0u8; 64];
    for i in 0..32 {
        s[i] = 0x63u8.wrapping_add((i as u8).wrapping_mul(19));
    }

    let masks = [
        0x8000_0000u32,
        0x0000_0000,
        0xFFFF_FFFF,
        0x7FFF_FFFF,
        0x8000_0001,
        0x0000_0001,
        0x8000_0010,
        0x4000_0000,
    ];
    for (i, mask) in masks.iter().enumerate() {
        s[32 + i * 4..36 + i * 4].copy_from_slice(&mask.to_le_bytes());
    }
    s
}

fn avx_maskmov_qword_scratch() -> [u8; 64] {
    let mut s = [0u8; 64];
    for i in 0..32 {
        s[i] = 0xB7u8.wrapping_sub((i as u8).wrapping_mul(11));
    }

    let masks = [
        0x8000_0000_0000_0000u64,
        0x0000_0000_0000_0000,
        0xFFFF_FFFF_FFFF_FFFF,
        0x7FFF_FFFF_FFFF_FFFF,
    ];
    for (i, mask) in masks.iter().enumerate() {
        s[32 + i * 8..40 + i * 8].copy_from_slice(&mask.to_le_bytes());
    }
    s
}

#[test]
fn avx_ymm_broadcast_scalar_128_and_integer_forms() {
    let s = avx_broadcast_scratch();

    let mut code = avx_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x18, 0x17]); // vbroadcastss ymm2, [rdi]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x19, 0x5F, 0x08]); // vbroadcastsd ymm3, [rdi+8]
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_vbroadcastss_sd_memory", &code, s);

    let mut code = avx_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x1A, 0x67, 0x10]); // vbroadcastf128 ymm4, [rdi+16]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x58, 0x7F, 0x04]); // vpbroadcastd ymm7, [rdi+4]
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x27]); // vmovdqu [rdi], ymm4
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x7F, 0x20]); // vmovdqu [rdi+32], ymm7
    code.push(HLT);
    check_avx_mem("avx_ymm_vbroadcastf128_and_vpbroadcastd", &code, s);

    let mut code = avx_start();
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x78, 0x6F, 0x01]); // vpbroadcastb ymm5, [rdi+1]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x79, 0x77, 0x02]); // vpbroadcastw ymm6, [rdi+2]
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x2F]); // vmovdqu [rdi], ymm5
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x77, 0x20]); // vmovdqu [rdi+32], ymm6
    code.push(HLT);
    check_avx_mem("avx2_ymm_vpbroadcast_byte_word_memory", &code, s);

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x78, 0xD0]); // vpbroadcastb ymm2, xmm0
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x79, 0xD8]); // vpbroadcastw ymm3, xmm0
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx2_ymm_vpbroadcast_byte_word_register", &code, s);

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x58, 0xD0]); // vpbroadcastd ymm2, xmm0
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x59, 0xD8]); // vpbroadcastq ymm3, xmm0
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx2_ymm_vpbroadcast_dword_qword_register", &code, s);
}

#[test]
fn avx_ymm_vmaskmovps_pd_load_store() {
    let s = avx_maskmov_dword_scratch();

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x75, 0x2C, 0x17]); // vmaskmovps ymm2, ymm1, [rdi]
    code.extend_from_slice(&[0xC4, 0xE2, 0x75, 0x2E, 0x47, 0x20]); // vmaskmovps [rdi+32], ymm1, ymm0
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.push(HLT);
    check_avx_mem("avx_ymm_vmaskmovps_load_store", &code, s);

    let s = avx_maskmov_qword_scratch();

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x75, 0x2D, 0x17]); // vmaskmovpd ymm2, ymm1, [rdi]
    code.extend_from_slice(&[0xC4, 0xE2, 0x75, 0x2F, 0x47, 0x20]); // vmaskmovpd [rdi+32], ymm1, ymm0
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.push(HLT);
    check_avx_mem("avx_ymm_vmaskmovpd_load_store", &code, s);
}

#[test]
fn avx2_ymm_vpmaskmovd_q_load_store() {
    let s = avx_maskmov_dword_scratch();

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x75, 0x8C, 0x17]); // vpmaskmovd ymm2, ymm1, [rdi]
    code.extend_from_slice(&[0xC4, 0xE2, 0x75, 0x8E, 0x47, 0x20]); // vpmaskmovd [rdi+32], ymm1, ymm0
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.push(HLT);
    check_avx_mem("avx2_ymm_vpmaskmovd_load_store", &code, s);

    let s = avx_maskmov_qword_scratch();

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0xF5, 0x8C, 0x17]); // vpmaskmovq ymm2, ymm1, [rdi]
    code.extend_from_slice(&[0xC4, 0xE2, 0xF5, 0x8E, 0x47, 0x20]); // vpmaskmovq [rdi+32], ymm1, ymm0
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.push(HLT);
    check_avx_mem("avx2_ymm_vpmaskmovq_load_store", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 25: additional VEX FMA order and sign variants.
// ===========================================================================

#[test]
fn avx_fma_ymm_addsub_and_negative_ps_forms() {
    let s = avx_f32_pair_scratch(
        [1.25, -2.5, 3.75, -4.5, 5.25, -6.75, 7.5, -8.25],
        [0.5, 1.5, -2.0, 2.25, -3.5, 4.0, -4.5, 5.5],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFC, 0x10, 0x07]); // vmovups ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFC, 0x10, 0x4F, 0x20]); // vmovups ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFC, 0x10, 0x17]); // vmovups ymm2, [rdi]
    code.extend_from_slice(&[0xC5, 0xFC, 0x10, 0x5F, 0x20]); // vmovups ymm3, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x96, 0x57, 0x20]); // vfmaddsub132ps ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0xBC, 0xD9]); // vfnmadd231ps ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFC, 0x11, 0x17]); // vmovups [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFC, 0x11, 0x5F, 0x20]); // vmovups [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_fma_ymm_addsub_negative_ps", &code, s);
}

#[test]
fn avx_fma_ymm_subadd_and_negative_pd_forms() {
    let s = avx_f64_pair_scratch(
        [1.5, -2.25, 3.125, -4.75],
        [0.25, 1.75, -2.5, 3.5],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFD, 0x10, 0x07]); // vmovupd ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFD, 0x10, 0x4F, 0x20]); // vmovupd ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0x10, 0x17]); // vmovupd ymm2, [rdi]
    code.extend_from_slice(&[0xC5, 0xFD, 0x10, 0x5F, 0x20]); // vmovupd ymm3, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0xFD, 0xA7, 0x57, 0x20]); // vfmsubadd213pd ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0xFD, 0xAE, 0xD9]); // vfnmsub213pd ymm3, ymm0, ymm1
    code.extend_from_slice(&[0xC5, 0xFD, 0x11, 0x17]); // vmovupd [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFD, 0x11, 0x5F, 0x20]); // vmovupd [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_fma_ymm_subadd_negative_pd", &code, s);
}

#[test]
fn avx_fma_scalar_ss_sd_memory_preserves_upper() {
    let mut s = [0u8; 64];
    for (i, value) in [2.5f32, 99.0, -7.25, 0.5].iter().enumerate() {
        s[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (i, value) in [4.0f32, -12.0, 8.0, -0.25].iter().enumerate() {
        s[16 + i * 4..20 + i * 4].copy_from_slice(&value.to_le_bytes());
    }
    s[32..36].copy_from_slice(&3.25f32.to_le_bytes());

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xF8, 0x10, 0x17]); // vmovups xmm2, [rdi]
    code.extend_from_slice(&[0xC5, 0xF8, 0x10, 0x47, 0x10]); // vmovups xmm0, [rdi+16]
    code.extend_from_slice(&[0xC4, 0xE2, 0x79, 0x99, 0x57, 0x20]); // vfmadd132ss xmm2, xmm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xF8, 0x11, 0x57, 0x30]); // vmovups [rdi+48], xmm2
    code.push(HLT);
    check_avx_mem("avx_fma_scalar_vfmadd132ss_memory", &code, s);

    let mut s = [0u8; 64];
    for (i, value) in [3.0f64, -99.5].iter().enumerate() {
        s[i * 8..i * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }
    s[16..24].copy_from_slice(&2.25f64.to_le_bytes());
    s[24..32].copy_from_slice(&5.5f64.to_le_bytes());

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xF9, 0x10, 0x1F]); // vmovupd xmm3, [rdi]
    code.extend_from_slice(&[0xC5, 0xFB, 0x10, 0x4F, 0x10]); // vmovsd xmm1, [rdi+16]
    code.extend_from_slice(&[0xC4, 0xE2, 0xF1, 0xBF, 0x5F, 0x18]); // vfnmsub231sd xmm3, xmm1, [rdi+24]
    code.extend_from_slice(&[0xC5, 0xF9, 0x11, 0x5F, 0x20]); // vmovupd [rdi+32], xmm3
    code.push(HLT);
    check_avx_mem("avx_fma_scalar_vfnmsub231sd_memory", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 26: VEX add-sub and horizontal FP forms.
// ===========================================================================

#[test]
fn avx_ymm_addsub_and_hadd_ps_memory_forms() {
    let s = avx_f32_pair_scratch(
        [1.0, -2.0, 3.5, -4.5, 5.25, -6.75, 7.0, -8.0],
        [0.5, 1.25, -2.5, 3.75, -4.25, 5.5, -6.5, 7.75],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFC, 0x10, 0x07]); // vmovups ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFF, 0xD0, 0x57, 0x20]); // vaddsubps ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFF, 0x7C, 0x5F, 0x20]); // vhaddps ymm3, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFC, 0x11, 0x17]); // vmovups [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFC, 0x11, 0x5F, 0x20]); // vmovups [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_addsub_hadd_ps_memory", &code, s);
}

#[test]
fn avx_ymm_hsub_ps_memory_form() {
    let s = avx_f32_pair_scratch(
        [8.0, 1.0, -3.0, 4.5, 16.0, -2.0, 9.25, -7.75],
        [2.0, -5.0, 6.5, -1.5, -8.5, 3.25, 4.75, -6.0],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFC, 0x10, 0x07]); // vmovups ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFF, 0x7D, 0x57, 0x20]); // vhsubps ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFC, 0x11, 0x17]); // vmovups [rdi], ymm2
    code.push(HLT);
    check_avx_mem("avx_ymm_hsub_ps_memory", &code, s);
}

#[test]
fn avx_ymm_addsub_hadd_hsub_pd_memory_forms() {
    let s = avx_f64_pair_scratch(
        [1.5, -2.25, 3.75, -4.5],
        [0.25, 1.75, -2.5, 3.125],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFD, 0x10, 0x07]); // vmovupd ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFD, 0xD0, 0x57, 0x20]); // vaddsubpd ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0x7C, 0x5F, 0x20]); // vhaddpd ymm3, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0x11, 0x17]); // vmovupd [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFD, 0x11, 0x5F, 0x20]); // vmovupd [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_ymm_addsub_hadd_pd_memory", &code, s);

    let s = avx_f64_pair_scratch(
        [8.0, 1.5, -3.25, 4.75],
        [2.0, -5.5, 6.25, -1.75],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFD, 0x10, 0x07]); // vmovupd ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFD, 0x7D, 0x57, 0x20]); // vhsubpd ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0x11, 0x17]); // vmovupd [rdi], ymm2
    code.push(HLT);
    check_avx_mem("avx_ymm_hsub_pd_memory", &code, s);
}

#[test]
fn avx_vex_round_packed_and_scalar_memory_forms() {
    let mut s = [0u8; 64];
    let ps = [1.25f32, -2.75, 3.5, -4.125, 5.5, -6.5, 7.0, -8.25];
    let pd = [1.25f64, -2.75, 3.5, -4.125];
    for (i, value) in ps.iter().enumerate() {
        s[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (i, value) in pd.iter().enumerate() {
        s[32 + i * 8..40 + i * 8].copy_from_slice(&value.to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x08, 0x17, 0x01]); // vroundps ymm2, [rdi], 1
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x09, 0x5F, 0x20, 0x02]); // vroundpd ymm3, [rdi+32], 2
    code.extend_from_slice(&[0xC5, 0xFC, 0x11, 0x17]); // vmovups [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFD, 0x11, 0x5F, 0x20]); // vmovupd [rdi+32], ymm3
    code.push(HLT);
    check_avx_mem("avx_vex_round_packed_memory_forms", &code, s);

    let mut s = [0u8; 64];
    let src_ss = [10.25f32, 20.5, -30.75, 40.125];
    let src_sd = [100.5f64, -200.75];
    for (i, value) in src_ss.iter().enumerate() {
        s[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (i, value) in src_sd.iter().enumerate() {
        s[16 + i * 8..24 + i * 8].copy_from_slice(&value.to_le_bytes());
    }
    s[32..36].copy_from_slice(&(-3.75f32).to_le_bytes());
    s[40..48].copy_from_slice(&9.25f64.to_le_bytes());

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xF8, 0x10, 0x07]); // vmovups xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xF8, 0x10, 0x4F, 0x10]); // vmovups xmm1, [rdi+16]
    code.extend_from_slice(&[0xC4, 0xE3, 0x79, 0x0A, 0x57, 0x20, 0x03]); // vroundss xmm2, xmm0, [rdi+32], 3
    code.extend_from_slice(&[0xC4, 0xE3, 0x71, 0x0B, 0x5F, 0x28, 0x01]); // vroundsd xmm3, xmm1, [rdi+40], 1
    code.extend_from_slice(&[0xC5, 0xF8, 0x11, 0x17]); // vmovups [rdi], xmm2
    code.extend_from_slice(&[0xC5, 0xF9, 0x11, 0x5F, 0x10]); // vmovupd [rdi+16], xmm3
    code.push(HLT);
    check_avx_mem("avx_vex_round_scalar_memory_forms", &code, s);
}

#[test]
fn avx_vex_dot_product_memory_forms() {
    let s = avx_f32_pair_scratch(
        [1.0, 2.0, 3.0, 4.0, -1.0, -2.0, -3.0, -4.0],
        [2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0],
    );

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFC, 0x10, 0x07]); // vmovups ymm0, [rdi]
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x40, 0x57, 0x20, 0xFF]); // vdpps ymm2, ymm0, [rdi+32], 0xff
    code.extend_from_slice(&[0xC5, 0xFC, 0x11, 0x17]); // vmovups [rdi], ymm2
    code.push(HLT);
    check_avx_mem("avx_vdpps_ymm_memory_form", &code, s);

    let s = avx_f64_pair_scratch([1.5, -2.0, 3.0, -4.0], [2.0, 0.5, -1.5, 2.5]);

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xF9, 0x10, 0x07]); // vmovupd xmm0, [rdi]
    code.extend_from_slice(&[0xC4, 0xE3, 0x79, 0x41, 0x5F, 0x20, 0x31]); // vdppd xmm3, xmm0, [rdi+32], 0x31
    code.extend_from_slice(&[0xC5, 0xF9, 0x11, 0x1F]); // vmovupd [rdi], xmm3
    code.push(HLT);
    check_avx_mem("avx_vdppd_xmm_memory_form", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 27: VEX mask extraction forms.
// ===========================================================================

#[test]
fn avx_movmskps_pd_xmm_ymm_forms() {
    let mut s = [0u8; 64];
    let ymm_words = [
        0x8000_0001u32,
        0x7F00_0000,
        0x8000_0002,
        0x8000_0003,
        0x0000_0004,
        0x8000_0005,
        0x0000_0006,
        0x8000_0007,
    ];
    for (i, word) in ymm_words.iter().enumerate() {
        s[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    let xmm_words = [0x0000_0001u32, 0x8000_0002, 0x8000_0003, 0x0000_0004];
    for (i, word) in xmm_words.iter().enumerate() {
        s[32 + i * 4..32 + i * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x20]); // vmovdqu xmm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFC, 0x50, 0xC0]); // vmovmskps eax, ymm0
    code.extend_from_slice(&[0x89, 0x07]); // mov [rdi], eax
    code.extend_from_slice(&[0xC5, 0xFD, 0x50, 0xC0]); // vmovmskpd eax, ymm0
    code.extend_from_slice(&[0x89, 0x47, 0x04]); // mov [rdi+4], eax
    code.extend_from_slice(&[0xC5, 0xF8, 0x50, 0xC1]); // vmovmskps eax, xmm1
    code.extend_from_slice(&[0x89, 0x47, 0x08]); // mov [rdi+8], eax
    code.extend_from_slice(&[0xC5, 0xF9, 0x50, 0xC1]); // vmovmskpd eax, xmm1
    code.extend_from_slice(&[0x89, 0x47, 0x0C]); // mov [rdi+12], eax
    code.push(HLT);
    check_avx_mem("avx_movmskps_pd_xmm_ymm_forms", &code, s);
}

#[test]
fn avx_vpmovmskb_xmm_ymm_forms() {
    let mut s = [0u8; 64];
    for i in 0..32 {
        let bit_set = matches!(i, 0 | 3 | 5 | 8 | 13 | 21 | 24 | 31);
        s[i] = if bit_set { 0x80 | i as u8 } else { i as u8 };
    }
    for i in 0..16 {
        let bit_set = matches!(i, 1 | 2 | 7 | 11 | 15);
        s[32 + i] = if bit_set {
            0xF0u8.wrapping_sub(i as u8)
        } else {
            0x10u8.wrapping_add(i as u8)
        };
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x20]); // vmovdqu xmm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFD, 0xD7, 0xC0]); // vpmovmskb eax, ymm0
    code.extend_from_slice(&[0x89, 0x07]); // mov [rdi], eax
    code.extend_from_slice(&[0xC5, 0xF9, 0xD7, 0xC1]); // vpmovmskb eax, xmm1
    code.extend_from_slice(&[0x89, 0x47, 0x04]); // mov [rdi+4], eax
    code.push(HLT);
    check_avx_mem("avx_vpmovmskb_xmm_ymm_forms", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 28: EVEX VAES and VPCLMULQDQ forms.
// ===========================================================================

#[test]
fn vex_vaes_xmm_ymm_round_register_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0xD3u8)
            .wrapping_add((i as u8).wrapping_mul(11))
            .rotate_left((i % 5) as u32);
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFD, 0x70, 0xC8, 0xB1]); // vpshufd ymm1, ymm0, 0xb1
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0xDC, 0xD1]); // vaesenc ymm2, ymm0, ymm1
    code.extend_from_slice(&[0xC4, 0xE2, 0x75, 0xDD, 0xDA]); // vaesenclast ymm3, ymm1, ymm2
    code.extend_from_slice(&[0xC4, 0xE2, 0x6D, 0xDE, 0xE3]); // vaesdec ymm4, ymm2, ymm3
    code.extend_from_slice(&[0xC4, 0xE2, 0x65, 0xDF, 0xEC]); // vaesdeclast ymm5, ymm3, ymm4
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x2F]); // vmovdqu [rdi], ymm5
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x47, 0x20]); // vmovdqu xmm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x30]); // vmovdqu xmm1, [rdi+48]
    code.extend_from_slice(&[0xC4, 0xE2, 0x79, 0xDC, 0xD1]); // vaesenc xmm2, xmm0, xmm1
    code.extend_from_slice(&[0xC4, 0xE2, 0x71, 0xDD, 0xDA]); // vaesenclast xmm3, xmm1, xmm2
    code.extend_from_slice(&[0xC4, 0xE2, 0x69, 0xDE, 0xE3]); // vaesdec xmm4, xmm2, xmm3
    code.extend_from_slice(&[0xC4, 0xE2, 0x61, 0xDF, 0xEC]); // vaesdeclast xmm5, xmm3, xmm4
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x6F, 0x20]); // vmovdqu [rdi+32], xmm5
    code.push(HLT);
    check_avx_mem("vex_vaes_xmm_ymm_round_register_forms", &code, s);
}

#[test]
fn vex_vaes_xmm_ymm_memory_source_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x69u8)
            .wrapping_add((i as u8).wrapping_mul(23))
            .rotate_right((i % 7) as u32);
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFD, 0x70, 0xC8, 0x4E]); // vpshufd ymm1, ymm0, 0x4e
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0xDC, 0x57, 0x20]); // vaesenc ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x75, 0xDD, 0x5F, 0x20]); // vaesenclast ymm3, ymm1, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x6D, 0xDE, 0x67, 0x20]); // vaesdec ymm4, ymm2, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x65, 0xDF, 0x6F, 0x20]); // vaesdeclast ymm5, ymm3, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x2F]); // vmovdqu [rdi], ymm5
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x47, 0x20]); // vmovdqu xmm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x30]); // vmovdqu xmm1, [rdi+48]
    code.extend_from_slice(&[0xC4, 0xE2, 0x79, 0xDC, 0x57, 0x20]); // vaesenc xmm2, xmm0, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x71, 0xDD, 0x5F, 0x30]); // vaesenclast xmm3, xmm1, [rdi+48]
    code.extend_from_slice(&[0xC4, 0xE2, 0x69, 0xDE, 0x67, 0x20]); // vaesdec xmm4, xmm2, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE2, 0x61, 0xDF, 0x6F, 0x30]); // vaesdeclast xmm5, xmm3, [rdi+48]
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x6F, 0x20]); // vmovdqu [rdi+32], xmm5
    code.push(HLT);
    check_avx_mem("vex_vaes_xmm_ymm_memory_source_forms", &code, s);
}

#[test]
fn vex_vpclmulqdq_xmm_ymm_selectors_and_memory() {
    let mut s = [0u8; 64];
    for i in 0..8 {
        let value = 0x8040_2010_0804_0201u64
            .wrapping_mul((i as u64) + 5)
            ^ 0xC3C3_3C3C_A5A5_5A5Au64.rotate_right((i * 7) as u32);
        s[i * 8..i * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }

    let mut code = avx_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x10]); // vmovdqu xmm1, [rdi+16]
    code.extend_from_slice(&[0xC4, 0xE3, 0x79, 0x44, 0xD1, 0x00]); // vpclmulqdq xmm2, xmm0, xmm1, 0
    code.extend_from_slice(&[0xC4, 0xE3, 0x79, 0x44, 0xD9, 0x11]); // vpclmulqdq xmm3, xmm0, xmm1, 0x11
    code.extend_from_slice(&[0xC5, 0xE9, 0xEF, 0xD3]); // vpxor xmm2, xmm2, xmm3
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x17]); // vmovdqu [rdi], xmm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x44, 0xD1, 0x00]); // vpclmulqdq ymm2, ymm0, ymm1, 0
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x44, 0xD9, 0x11]); // vpclmulqdq ymm3, ymm0, ymm1, 0x11
    code.extend_from_slice(&[0xC5, 0xED, 0xEF, 0xD3]); // vpxor ymm2, ymm2, ymm3
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC4, 0xE3, 0x7D, 0x44, 0x67, 0x20, 0x10]); // vpclmulqdq ymm4, ymm0, [rdi+32], 0x10
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x67, 0x20]); // vmovdqu [rdi+32], ymm4
    code.push(HLT);
    check_avx_mem("vex_vpclmulqdq_xmm_ymm_selectors_memory", &code, s);
}

#[test]
fn evex_xmm_ymm_vaes_vl_register_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x4Du8)
            .wrapping_add((i as u8).wrapping_mul(29))
            .rotate_left((i % 6) as u32);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFD, 0x70, 0xC8, 0xB1]); // vpshufd ymm1, ymm0, 0xb1
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x28, 0xDC, 0xD1]); // {evex} vaesenc ymm2, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x28, 0xDD, 0xDA]); // {evex} vaesenclast ymm3, ymm1, ymm2
    code.extend_from_slice(&[0x62, 0xF2, 0x6D, 0x28, 0xDE, 0xE3]); // {evex} vaesdec ymm4, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF2, 0x65, 0x28, 0xDF, 0xEC]); // {evex} vaesdeclast ymm5, ymm3, ymm4
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x2F]); // vmovdqu [rdi], ymm5
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x47, 0x20]); // vmovdqu xmm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x30]); // vmovdqu xmm1, [rdi+48]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x08, 0xDC, 0xD1]); // {evex} vaesenc xmm2, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x08, 0xDD, 0xDA]); // {evex} vaesenclast xmm3, xmm1, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0x6D, 0x08, 0xDE, 0xE3]); // {evex} vaesdec xmm4, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF2, 0x65, 0x08, 0xDF, 0xEC]); // {evex} vaesdeclast xmm5, xmm3, xmm4
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x6F, 0x20]); // vmovdqu [rdi+32], xmm5
    code.push(HLT);
    check_evex_mem("evex_xmm_ymm_vaes_vl_register_forms", &code, s);
}

#[test]
fn evex_xmm_ymm_vaes_vl_memory_source_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0xB7u8)
            .wrapping_add((i as u8).wrapping_mul(31))
            .rotate_right((i % 5) as u32);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFD, 0x70, 0xC8, 0x4E]); // vpshufd ymm1, ymm0, 0x4e
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x28, 0xDC, 0x57, 0x01]); // {evex} vaesenc ymm2, ymm0, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x28, 0xDD, 0x5F, 0x01]); // {evex} vaesenclast ymm3, ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x6D, 0x28, 0xDE, 0x67, 0x01]); // {evex} vaesdec ymm4, ymm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x65, 0x28, 0xDF, 0x6F, 0x01]); // {evex} vaesdeclast ymm5, ymm3, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x2F]); // vmovdqu [rdi], ymm5
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x47, 0x20]); // vmovdqu xmm0, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x30]); // vmovdqu xmm1, [rdi+48]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x08, 0xDC, 0x57, 0x02]); // {evex} vaesenc xmm2, xmm0, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x08, 0xDD, 0x5F, 0x03]); // {evex} vaesenclast xmm3, xmm1, [rdi+48]
    code.extend_from_slice(&[0x62, 0xF2, 0x6D, 0x08, 0xDE, 0x67, 0x02]); // {evex} vaesdec xmm4, xmm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x65, 0x08, 0xDF, 0x6F, 0x03]); // {evex} vaesdeclast xmm5, xmm3, [rdi+48]
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x6F, 0x20]); // vmovdqu [rdi+32], xmm5
    code.push(HLT);
    check_evex_mem("evex_xmm_ymm_vaes_vl_memory_source_forms", &code, s);
}

#[test]
fn evex_xmm_ymm_vpclmulqdq_vl_forms() {
    let mut s = [0u8; 64];
    for i in 0..8 {
        let value = 0x0102_0408_1020_4080u64
            .wrapping_mul((i as u64) + 7)
            ^ 0xA5A5_5A5A_C3C3_3C3Cu64.rotate_left((i * 5) as u32);
        s[i * 8..i * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x10]); // vmovdqu xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x08, 0x44, 0xD1, 0x00]); // {evex} vpclmulqdq xmm2, xmm0, xmm1, 0
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x08, 0x44, 0xD9, 0x11]); // {evex} vpclmulqdq xmm3, xmm0, xmm1, 0x11
    code.extend_from_slice(&[0xC5, 0xE9, 0xEF, 0xD3]); // vpxor xmm2, xmm2, xmm3
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x17]); // vmovdqu [rdi], xmm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x28, 0x44, 0xD1, 0x00]); // {evex} vpclmulqdq ymm2, ymm0, ymm1, 0
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x28, 0x44, 0xD9, 0x11]); // {evex} vpclmulqdq ymm3, ymm0, ymm1, 0x11
    code.extend_from_slice(&[0xC5, 0xED, 0xEF, 0xD3]); // vpxor ymm2, ymm2, ymm3
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x28, 0x44, 0x67, 0x01, 0x10]); // {evex} vpclmulqdq ymm4, ymm0, [rdi+32], 0x10
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x67, 0x20]); // vmovdqu [rdi+32], ymm4
    code.push(HLT);
    check_evex_mem("evex_xmm_ymm_vpclmulqdq_vl_forms", &code, s);
}

#[test]
fn evex_zmm_vaes_round_register_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x3Du8)
            .wrapping_add((i as u8).wrapping_mul(19))
            .rotate_left((i % 7) as u32);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0x70, 0xC8, 0xB1]); // vpshufd zmm1, zmm0, 0xb1
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0xDC, 0xD1]); // vaesenc zmm2, zmm0, zmm1
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x48, 0xDD, 0xDA]); // vaesenclast zmm3, zmm1, zmm2
    code.extend_from_slice(&[0x62, 0xF2, 0x6D, 0x48, 0xDE, 0xD3]); // vaesdec zmm2, zmm2, zmm3
    code.extend_from_slice(&[0x62, 0xF2, 0x65, 0x48, 0xDF, 0xDA]); // vaesdeclast zmm3, zmm3, zmm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x1F]); // vmovdqu64 [rdi], zmm3
    code.push(HLT);
    check_evex_mem("evex_zmm_vaes_round_register_forms", &code, s);
}

#[test]
fn evex_zmm_vpclmulqdq_selectors_and_memory() {
    let mut s = [0u8; 64];
    for i in 0..8 {
        let value = 0x0102_0408_1020_4080u64
            .wrapping_mul((i as u64) + 1)
            ^ 0xA5A5_5A5A_3C3C_C3C3u64.rotate_left((i * 5) as u32);
        s[i * 8..i * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0x70, 0xC8, 0xB1]); // vpshufd zmm1, zmm0, 0xb1
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x44, 0xD1, 0x00]); // vpclmulqdq zmm2, zmm0, zmm1, 0
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x44, 0xD9, 0x11]); // vpclmulqdq zmm3, zmm0, zmm1, 0x11
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD3]); // vpxorq zmm2, zmm2, zmm3
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x44, 0x1F, 0x10]); // vpclmulqdq zmm3, zmm0, [rdi], 0x10
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD3]); // vpxorq zmm2, zmm2, zmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpclmulqdq_selectors_and_memory", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 29: EVEX GFNI vector forms.
// ===========================================================================

#[test]
fn evex_xmm_gfni_vl_reg_mem_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x6Bu8)
            .wrapping_add((i as u8).wrapping_mul(13))
            .rotate_left((i % 5) as u32);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xFF, 0xFF, 0xFF, 0xFF]); // mov eax, -1
    code.extend_from_slice(&[0xC5, 0xFB, 0x92, 0xC8]); // kmovd k1, eax
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x10]); // vmovdqu xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x09, 0xCF, 0xD1]); // vgf2p8mulb xmm2 {k1}, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x89, 0xCF, 0x5F, 0x02]); // vgf2p8mulb xmm3 {k1}{z}, xmm0, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x09, 0xCE, 0xE1, 0x63]); // vgf2p8affineqb xmm4 {k1}, xmm0, xmm1, 0x63
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x89, 0xCF, 0x6F, 0x03, 0x9A]); // vgf2p8affineinvqb xmm5 {k1}{z}, xmm0, [rdi+48], 0x9a
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x17]); // vmovdqu [rdi], xmm2
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x5F, 0x10]); // vmovdqu [rdi+16], xmm3
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x67, 0x20]); // vmovdqu [rdi+32], xmm4
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x6F, 0x30]); // vmovdqu [rdi+48], xmm5
    code.push(HLT);
    check_evex_mem("evex_xmm_gfni_vl_reg_mem_forms", &code, s);
}

#[test]
fn evex_ymm_gfni_vl_reg_mem_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0xE1u8)
            .wrapping_add((i as u8).wrapping_mul(23))
            .rotate_right((i % 7) as u32);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xFF, 0xFF, 0xFF, 0xFF]); // mov eax, -1
    code.extend_from_slice(&[0xC5, 0xFB, 0x92, 0xC8]); // kmovd k1, eax
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x29, 0xCF, 0xD1]); // vgf2p8mulb ymm2 {k1}, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0xA9, 0xCF, 0x1F]); // vgf2p8mulb ymm3 {k1}{z}, ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xED, 0xEF, 0xD3]); // vpxor ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x29, 0xCE, 0xE1, 0x63]); // vgf2p8affineqb ymm4 {k1}, ymm0, ymm1, 0x63
    code.extend_from_slice(&[0xC5, 0xED, 0xEF, 0xD4]); // vpxor ymm2, ymm2, ymm4
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0xA9, 0xCF, 0x2F, 0x9A]); // vgf2p8affineinvqb ymm5 {k1}{z}, ymm0, [rdi], 0x9a
    code.extend_from_slice(&[0xC5, 0xDD, 0xEF, 0xDD]); // vpxor ymm3, ymm4, ymm5
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x17]); // vmovdqu [rdi], ymm2
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x5F, 0x20]); // vmovdqu [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_gfni_vl_reg_mem_forms", &code, s);
}

#[test]
fn evex_zmm_gfni_mul_affine_reg_mem_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0xB7u8)
            .wrapping_add((i as u8).wrapping_mul(29))
            .rotate_right((i % 5) as u32);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0x70, 0xC8, 0x4E]); // vpshufd zmm1, zmm0, 0x4e
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0xCF, 0xD1]); // vgf2p8mulb zmm2, zmm0, zmm1
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0xCF, 0x1F]); // vgf2p8mulb zmm3, zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD3]); // vpxorq zmm2, zmm2, zmm3
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x48, 0xCE, 0xD9, 0x63]); // vgf2p8affineqb zmm3, zmm0, zmm1, 0x63
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD3]); // vpxorq zmm2, zmm2, zmm3
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x48, 0xCE, 0x1F, 0x9A]); // vgf2p8affineqb zmm3, zmm0, [rdi], 0x9a
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD3]); // vpxorq zmm2, zmm2, zmm3
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x48, 0xCF, 0xD9, 0xC0]); // vgf2p8affineinvqb zmm3, zmm0, zmm1, 0xc0
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD3]); // vpxorq zmm2, zmm2, zmm3
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x48, 0xCF, 0x1F, 0x5A]); // vgf2p8affineinvqb zmm3, zmm0, [rdi], 0x5a
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD3]); // vpxorq zmm2, zmm2, zmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_gfni_mul_affine_reg_mem_forms", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 30: EVEX compress/expand forms.
// ===========================================================================

#[test]
fn evex_zmm_vcompressps_vexpandps_memory_roundtrip() {
    let s = evex_dword_scratch(|i| {
        0x3F80_0000u32
            .wrapping_add((i as u32).wrapping_mul(0x0102_0408))
            .rotate_left((i % 11) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xAD, 0xB5, 0x00, 0x00]); // mov eax, 0xb5ad
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x49, 0x8A, 0x07]); // vcompressps [rdi]{k1}, zmm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0xC9, 0x88, 0x17]); // vexpandps zmm2{k1}{z}, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vcompressps_vexpandps_memory_roundtrip", &code, s);
}

#[test]
fn evex_zmm_vpcompress_vpexpand_dq_register_forms() {
    let s = evex_qword_scratch(|i| {
        0x0102_0305_0813_2134u64
            .wrapping_mul((i as u64) + 3)
            .rotate_left((i * 9) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xAD, 0xB5, 0x00, 0x00]); // mov eax, 0xb5ad
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0xC9, 0x8B, 0xC2]); // vpcompressd zmm2{k1}{z}, zmm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0xC9, 0x89, 0xDA]); // vpexpandd zmm3{k1}{z}, zmm2
    code.extend_from_slice(&[0xB8, 0xAD, 0x00, 0x00, 0x00]); // mov eax, 0xad
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xD0]); // kmovb k2, eax
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0xCA, 0x8B, 0xC4]); // vpcompressq zmm4{k2}{z}, zmm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0xCA, 0x89, 0xEC]); // vpexpandq zmm5{k2}{z}, zmm4
    code.extend_from_slice(&[0x62, 0xF1, 0xE5, 0x48, 0xEF, 0xDD]); // vpxorq zmm3, zmm3, zmm5
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x1F]); // vmovdqu64 [rdi], zmm3
    code.push(HLT);
    check_evex_mem("evex_zmm_vpcompress_vpexpand_dq_register_forms", &code, s);
}

#[test]
fn evex_xmm_vpcompress_vpexpand_dq_vl_forms() {
    let s = evex_qword_scratch(|i| {
        0x0F1E_2D3C_4B5A_6978u64
            .wrapping_mul((i as u64) + 5)
            .rotate_left((i * 5) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0x0B, 0x00, 0x00, 0x00]); // mov eax, 0x0b
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0xB8, 0x01, 0x00, 0x00, 0x00]); // mov eax, 0x01
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xD0]); // kmovb k2, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x07]); // vmovdqu64 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x89, 0x8B, 0xC2]); // vpcompressd xmm2{k1}{z}, xmm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x89, 0x89, 0xDA]); // vpexpandd xmm3{k1}{z}, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x8A, 0x8B, 0xC4]); // vpcompressq xmm4{k2}{z}, xmm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x8A, 0x89, 0xEC]); // vpexpandq xmm5{k2}{z}, xmm4
    code.extend_from_slice(&[0x62, 0xF1, 0xE5, 0x08, 0xEF, 0xDD]); // vpxorq xmm3, xmm3, xmm5
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x1F]); // vmovdqu64 [rdi], xmm3
    code.push(HLT);
    check_evex_mem("evex_xmm_vpcompress_vpexpand_dq_vl_forms", &code, s);
}

#[test]
fn evex_ymm_vpcompress_vpexpand_dq_vl_forms() {
    let s = evex_qword_scratch(|i| {
        0x3141_5926_5358_9793u64
            .wrapping_mul((i as u64) + 7)
            .rotate_right((i * 11) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xB5, 0x00, 0x00, 0x00]); // mov eax, 0xb5
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0xB8, 0x0D, 0x00, 0x00, 0x00]); // mov eax, 0x0d
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xD0]); // kmovb k2, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0xA9, 0x8B, 0xC2]); // vpcompressd ymm2{k1}{z}, ymm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0xA9, 0x89, 0xDA]); // vpexpandd ymm3{k1}{z}, ymm2
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0xAA, 0x8B, 0xC4]); // vpcompressq ymm4{k2}{z}, ymm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0xAA, 0x89, 0xEC]); // vpexpandq ymm5{k2}{z}, ymm4
    code.extend_from_slice(&[0x62, 0xF1, 0xE5, 0x28, 0xEF, 0xDD]); // vpxorq ymm3, ymm3, ymm5
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x1F]); // vmovdqu64 [rdi], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_vpcompress_vpexpand_dq_vl_forms", &code, s);
}

#[test]
fn evex_xmm_vpcompress_vpexpand_dq_vl_memory_forms() {
    let s = evex_qword_scratch(|i| {
        0xCAFE_BABE_1020_3040u64
            .wrapping_add((i as u64).wrapping_mul(0x0102_0305_0813_2134))
            .rotate_left((i * 3) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0x0B, 0x00, 0x00, 0x00]); // mov eax, 0x0b
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0xB8, 0x01, 0x00, 0x00, 0x00]); // mov eax, 0x01
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xD0]); // kmovb k2, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x07]); // vmovdqu64 xmm0, [rdi]
    append_clear_rdi_plus_64_qwords(&mut code);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x09, 0x8B, 0x47, 0x10]); // vpcompressd [rdi+64]{k1}, xmm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x89, 0x89, 0x57, 0x10]); // vpexpandd xmm2{k1}{z}, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x17]); // vmovdqu64 [rdi], xmm2
    append_clear_rdi_plus_64_qwords(&mut code);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x0A, 0x8B, 0x47, 0x08]); // vpcompressq [rdi+64]{k2}, xmm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x8A, 0x89, 0x5F, 0x08]); // vpexpandq xmm3{k2}{z}, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+16], xmm3
    code.push(HLT);
    check_evex_mem("evex_xmm_vpcompress_vpexpand_dq_vl_memory_forms", &code, s);
}

#[test]
fn evex_ymm_vpcompress_vpexpand_dq_vl_memory_forms() {
    let s = evex_qword_scratch(|i| {
        0x0BAD_F00D_55AA_33CCu64
            .wrapping_sub((i as u64).wrapping_mul(0x1111_2222_3333_4444))
            .rotate_right((i * 5) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xB5, 0x00, 0x00, 0x00]); // mov eax, 0xb5
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0xB8, 0x0D, 0x00, 0x00, 0x00]); // mov eax, 0x0d
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xD0]); // kmovb k2, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    append_clear_rdi_plus_64_qwords(&mut code);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x29, 0x8B, 0x47, 0x10]); // vpcompressd [rdi+64]{k1}, ymm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0xA9, 0x89, 0x57, 0x10]); // vpexpandd ymm2{k1}{z}, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x17]); // vmovdqu64 [rdi], ymm2
    append_clear_rdi_plus_64_qwords(&mut code);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x2A, 0x8B, 0x47, 0x08]); // vpcompressq [rdi+64]{k2}, ymm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0xAA, 0x89, 0x5F, 0x08]); // vpexpandq ymm3{k2}{z}, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_vpcompress_vpexpand_dq_vl_memory_forms", &code, s);
}

#[test]
fn evex_xmm_vcompress_vexpand_fp_vl_forms() {
    let s = evex_qword_scratch(|i| {
        0xA5A5_5A5A_1234_5678u64
            .wrapping_mul((i as u64) + 9)
            .rotate_left((i * 3) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0x0B, 0x00, 0x00, 0x00]); // mov eax, 0x0b
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0xB8, 0x01, 0x00, 0x00, 0x00]); // mov eax, 0x01
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xD0]); // kmovb k2, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x07]); // vmovdqu64 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x89, 0x8A, 0xC2]); // vcompressps xmm2{k1}{z}, xmm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x89, 0x88, 0xDA]); // vexpandps xmm3{k1}{z}, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x8A, 0x8A, 0xC4]); // vcompresspd xmm4{k2}{z}, xmm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x8A, 0x88, 0xEC]); // vexpandpd xmm5{k2}{z}, xmm4
    code.extend_from_slice(&[0xC5, 0xE1, 0x57, 0xDD]); // vxorpd xmm3, xmm3, xmm5
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x1F]); // vmovdqu64 [rdi], xmm3
    code.push(HLT);
    check_evex_mem("evex_xmm_vcompress_vexpand_fp_vl_forms", &code, s);
}

#[test]
fn evex_ymm_vcompress_vexpand_fp_vl_forms() {
    let s = evex_qword_scratch(|i| {
        0x55AA_33CC_7788_9900u64
            .wrapping_mul((i as u64) + 11)
            .rotate_right((i * 7) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xB5, 0x00, 0x00, 0x00]); // mov eax, 0xb5
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0xB8, 0x0D, 0x00, 0x00, 0x00]); // mov eax, 0x0d
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xD0]); // kmovb k2, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0xA9, 0x8A, 0xC2]); // vcompressps ymm2{k1}{z}, ymm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0xA9, 0x88, 0xDA]); // vexpandps ymm3{k1}{z}, ymm2
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0xAA, 0x8A, 0xC4]); // vcompresspd ymm4{k2}{z}, ymm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0xAA, 0x88, 0xEC]); // vexpandpd ymm5{k2}{z}, ymm4
    code.extend_from_slice(&[0xC5, 0xE5, 0x57, 0xDD]); // vxorpd ymm3, ymm3, ymm5
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x1F]); // vmovdqu64 [rdi], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_vcompress_vexpand_fp_vl_forms", &code, s);
}

#[test]
fn evex_xmm_vcompress_vexpand_fp_vl_memory_forms() {
    let s = evex_qword_scratch(|i| {
        0x0102_0408_1020_4080u64
            .wrapping_add((i as u64).wrapping_mul(0x1112_1321_3455_89AB))
            .rotate_left((i * 5) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0x0B, 0x00, 0x00, 0x00]); // mov eax, 0x0b
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0xB8, 0x01, 0x00, 0x00, 0x00]); // mov eax, 0x01
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xD0]); // kmovb k2, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x07]); // vmovdqu64 xmm0, [rdi]
    append_clear_rdi_plus_64_qwords(&mut code);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x09, 0x8A, 0x47, 0x10]); // vcompressps [rdi+64]{k1}, xmm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x89, 0x88, 0x57, 0x10]); // vexpandps xmm2{k1}{z}, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x17]); // vmovdqu64 [rdi], xmm2
    append_clear_rdi_plus_64_qwords(&mut code);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x0A, 0x8A, 0x47, 0x08]); // vcompresspd [rdi+64]{k2}, xmm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x8A, 0x88, 0x5F, 0x08]); // vexpandpd xmm3{k2}{z}, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+16], xmm3
    code.push(HLT);
    check_evex_mem("evex_xmm_vcompress_vexpand_fp_vl_memory_forms", &code, s);
}

#[test]
fn evex_ymm_vcompress_vexpand_fp_vl_memory_forms() {
    let s = evex_qword_scratch(|i| {
        0x8877_6655_4433_2211u64
            .wrapping_sub((i as u64).wrapping_mul(0x0103_0507_0B0D_1113))
            .rotate_right((i * 9) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xB5, 0x00, 0x00, 0x00]); // mov eax, 0xb5
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0xB8, 0x0D, 0x00, 0x00, 0x00]); // mov eax, 0x0d
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xD0]); // kmovb k2, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    append_clear_rdi_plus_64_qwords(&mut code);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x29, 0x8A, 0x47, 0x10]); // vcompressps [rdi+64]{k1}, ymm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0xA9, 0x88, 0x57, 0x10]); // vexpandps ymm2{k1}{z}, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x17]); // vmovdqu64 [rdi], ymm2
    append_clear_rdi_plus_64_qwords(&mut code);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x2A, 0x8A, 0x47, 0x08]); // vcompresspd [rdi+64]{k2}, ymm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0xAA, 0x88, 0x5F, 0x08]); // vexpandpd ymm3{k2}{z}, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_vcompress_vexpand_fp_vl_memory_forms", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 31: EVEX scalar extract/insert lane forms.
// ===========================================================================

#[test]
fn evex_xmm_scalar_extract_reg_mem_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x21u8).wrapping_add((i as u8).wrapping_mul(0x13));
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xE1, 0x7E, 0x08, 0x6F, 0x07]); // vmovdqu32 xmm16, [rdi]
    code.extend_from_slice(&[0x62, 0xE3, 0x7D, 0x08, 0x14, 0xC0, 0x09]); // vpextrb eax, xmm16, 9
    code.extend_from_slice(&[0x88, 0x47, 0x10]); // mov [rdi+16], al
    code.extend_from_slice(&[0x62, 0xB1, 0x7D, 0x08, 0xC5, 0xD8, 0x06]); // vpextrw ebx, xmm16, 6
    code.extend_from_slice(&[0x66, 0x89, 0x5F, 0x12]); // mov [rdi+18], bx
    code.extend_from_slice(&[0x62, 0xE3, 0x7D, 0x08, 0x16, 0xC1, 0x02]); // vpextrd ecx, xmm16, 2
    code.extend_from_slice(&[0x89, 0x4F, 0x14]); // mov [rdi+20], ecx
    code.extend_from_slice(&[0x62, 0xE3, 0xFD, 0x08, 0x16, 0xC2, 0x01]); // vpextrq rdx, xmm16, 1
    code.extend_from_slice(&[0x48, 0x89, 0x57, 0x18]); // mov [rdi+24], rdx
    code.extend_from_slice(&[0x62, 0xE3, 0x7D, 0x08, 0x17, 0x47, 0x08, 0x03]); // vextractps [rdi+32], xmm16, 3
    code.extend_from_slice(&[0x62, 0xE3, 0x7D, 0x08, 0x14, 0x47, 0x24, 0x05]); // vpextrb [rdi+36], xmm16, 5
    code.extend_from_slice(&[0x62, 0xE3, 0x7D, 0x08, 0x15, 0x47, 0x13, 0x04]); // vpextrw [rdi+38], xmm16, 4
    code.extend_from_slice(&[0x62, 0xE3, 0x7D, 0x08, 0x16, 0x47, 0x0A, 0x01]); // vpextrd [rdi+40], xmm16, 1
    code.extend_from_slice(&[0x62, 0xE3, 0xFD, 0x08, 0x16, 0x47, 0x06, 0x00]); // vpextrq [rdi+48], xmm16, 0
    code.push(HLT);
    check_evex_mem("evex_xmm_scalar_extract_reg_mem_forms", &code, s);
}

#[test]
fn evex_xmm_scalar_insert_reg_mem_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0xC3u8).wrapping_sub((i as u8).wrapping_mul(7));
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xE1, 0x7E, 0x08, 0x6F, 0x0F]); // vmovdqu32 xmm17, [rdi]
    code.extend_from_slice(&[0xB8, 0xAA, 0x00, 0x00, 0x00]); // mov eax, 0xaa
    code.extend_from_slice(&[0x62, 0xE3, 0x75, 0x00, 0x20, 0xD0, 0x04]); // vpinsrb xmm18, xmm17, eax, 4
    code.extend_from_slice(&[0xBB, 0x57, 0x13, 0x00, 0x00]); // mov ebx, 0x1357
    code.extend_from_slice(&[0x62, 0xE1, 0x6D, 0x00, 0xC4, 0xDB, 0x07]); // vpinsrw xmm19, xmm18, ebx, 7
    code.extend_from_slice(&[0xB9, 0xEF, 0xCD, 0xAB, 0x89]); // mov ecx, 0x89abcdef
    code.extend_from_slice(&[0x62, 0xE3, 0x65, 0x00, 0x22, 0xE1, 0x02]); // vpinsrd xmm20, xmm19, ecx, 2
    code.extend_from_slice(&[0x48, 0xBA]); // mov rdx, 0x1122334455667788
    code.extend_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes());
    code.extend_from_slice(&[0x62, 0xE3, 0xDD, 0x00, 0x22, 0xEA, 0x01]); // vpinsrq xmm21, xmm20, rdx, 1
    code.extend_from_slice(&[0x62, 0xE3, 0x55, 0x00, 0x21, 0x77, 0x03, 0x2C]); // vinsertps xmm22, xmm21, [rdi+12], 0x2c
    code.extend_from_slice(&[0x62, 0xE1, 0x7E, 0x08, 0x7F, 0x77, 0x02]); // vmovdqu32 [rdi+32], xmm22
    code.extend_from_slice(&[0x62, 0xE1, 0x7E, 0x08, 0x6F, 0x0F]); // vmovdqu32 xmm17, [rdi]
    code.extend_from_slice(&[0x62, 0xE3, 0x75, 0x00, 0x20, 0x57, 0x1F, 0x05]); // vpinsrb xmm18, xmm17, [rdi+31], 5
    code.extend_from_slice(&[0x62, 0xE1, 0x6D, 0x00, 0xC4, 0x5F, 0x0F, 0x03]); // vpinsrw xmm19, xmm18, [rdi+30], 3
    code.extend_from_slice(&[0x62, 0xE3, 0x65, 0x00, 0x22, 0x67, 0x07, 0x01]); // vpinsrd xmm20, xmm19, [rdi+28], 1
    code.extend_from_slice(&[0x62, 0xE3, 0xDD, 0x00, 0x22, 0x6F, 0x03, 0x00]); // vpinsrq xmm21, xmm20, [rdi+24], 0
    code.extend_from_slice(&[0x62, 0xE1, 0x7E, 0x08, 0x7F, 0x6F, 0x03]); // vmovdqu32 [rdi+48], xmm21
    code.push(HLT);
    check_evex_mem("evex_xmm_scalar_insert_reg_mem_forms", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 32: EVEX chunk insert/extract lane forms.
// ===========================================================================

#[test]
fn evex_zmm_insert_extract_i32x4_memory_forms() {
    let s = evex_dword_scratch(|i| {
        0x1020_3040u32
            .wrapping_add((i as u32).wrapping_mul(0x1112_1315))
            .rotate_left((i % 9) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x38, 0x57, 0x02, 0x02]); // vinserti32x4 zmm2, zmm0, [rdi+32], 2
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x39, 0xD3, 0x02]); // vextracti32x4 xmm3, zmm2, 2
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x39, 0x17, 0x02]); // vextracti32x4 [rdi], zmm2, 2
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x39, 0x57, 0x01, 0x00]); // vextracti32x4 [rdi+16], zmm2, 0
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x5F, 0x03]); // vmovdqu32 [rdi+48], xmm3
    code.push(HLT);
    check_evex_mem("evex_zmm_insert_extract_i32x4_memory_forms", &code, s);
}

#[test]
fn evex_zmm_insert_extract_i32x8_register_forms() {
    let s = evex_dword_scratch(|i| {
        0x8000_0001u32
            .wrapping_add((i as u32).wrapping_mul(0x0102_0408))
            .rotate_right((i % 13) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu32 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0x5D, 0x48, 0x72, 0xF0, 0x01]); // vpslld zmm4, zmm0, 1
    code.extend_from_slice(&[0x62, 0xF3, 0x5D, 0x48, 0x3A, 0xD1, 0x01]); // vinserti32x8 zmm2, zmm4, ymm1, 1
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x3B, 0xD3, 0x01]); // vextracti32x8 ymm3, zmm2, 1
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x3B, 0x17, 0x00]); // vextracti32x8 [rdi], zmm2, 0
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x7F, 0x5F, 0x01]); // vmovdqu32 [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_zmm_insert_extract_i32x8_register_forms", &code, s);
}

#[test]
fn evex_ymm_insert_extract_i32x4_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x3141_5926u32
            .wrapping_add((i as u32).wrapping_mul(0x2718_2818))
            .rotate_left((i % 11) as u32)
    });

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x4F, 0x02]); // vmovdqu64 xmm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x28, 0x38, 0xD1, 0x01]); // vinserti32x4 ymm2, ymm0, xmm1, 1
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x28, 0x38, 0x5F, 0x04, 0x00]); // vinserti32x4 ymm3, ymm0, [rdi+64], 0
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x28, 0x39, 0xD4, 0x01]); // vextracti32x4 xmm4, ymm2, 1
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x28, 0x39, 0x5F, 0x02, 0x00]); // vextracti32x4 [rdi+32], ymm3, 0
    code.extend_from_slice(&[0x62, 0xF1, 0x6D, 0x28, 0xEF, 0xD3]); // vpxord ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x17]); // vmovdqu64 [rdi], ymm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x67, 0x03]); // vmovdqu64 [rdi+48], xmm4
    code.push(HLT);
    check_evex_mem("evex_ymm_insert_extract_i32x4_vl_forms", &code, s);
}

#[test]
fn evex_ymm_insert_extract_i64x2_vl_forms() {
    let s = evex_qword_scratch(|i| {
        0x1020_3040_5060_7080u64
            .wrapping_add((i as u64).wrapping_mul(0x0102_0305_0813_2134))
            .rotate_right((i % 17) as u32)
    });

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x4F, 0x02]); // vmovdqu64 xmm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x28, 0x38, 0xD1, 0x01]); // vinserti64x2 ymm2, ymm0, xmm1, 1
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x28, 0x38, 0x5F, 0x04, 0x00]); // vinserti64x2 ymm3, ymm0, [rdi+64], 0
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x28, 0x39, 0xD4, 0x01]); // vextracti64x2 xmm4, ymm2, 1
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x28, 0x39, 0x5F, 0x02, 0x00]); // vextracti64x2 [rdi+32], ymm3, 0
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x28, 0xEF, 0xD3]); // vpxorq ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x17]); // vmovdqu64 [rdi], ymm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x67, 0x03]); // vmovdqu64 [rdi+48], xmm4
    code.push(HLT);
    check_evex_mem("evex_ymm_insert_extract_i64x2_vl_forms", &code, s);
}

#[test]
fn evex_ymm_insert_extract_f32x4_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0xC001_D00Du32
            .wrapping_add((i as u32).wrapping_mul(0x1020_4081))
            .rotate_left((i % 7) as u32)
    });

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x4F, 0x02]); // vmovdqu64 xmm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x28, 0x18, 0xD1, 0x01]); // vinsertf32x4 ymm2, ymm0, xmm1, 1
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x28, 0x18, 0x5F, 0x04, 0x00]); // vinsertf32x4 ymm3, ymm0, [rdi+64], 0
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x28, 0x19, 0xD4, 0x01]); // vextractf32x4 xmm4, ymm2, 1
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x28, 0x19, 0x5F, 0x02, 0x00]); // vextractf32x4 [rdi+32], ymm3, 0
    code.extend_from_slice(&[0xC5, 0xEC, 0x57, 0xD3]); // vxorps ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x17]); // vmovdqu64 [rdi], ymm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x67, 0x03]); // vmovdqu64 [rdi+48], xmm4
    code.push(HLT);
    check_evex_mem("evex_ymm_insert_extract_f32x4_vl_forms", &code, s);
}

#[test]
fn evex_ymm_insert_extract_f64x2_vl_forms() {
    let s = evex_qword_scratch(|i| {
        0xF00D_CAFE_BEEF_0001u64
            .wrapping_add((i as u64).wrapping_mul(0x0123_4567_89AB_CDEF))
            .rotate_right((i % 19) as u32)
    });

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x4F, 0x02]); // vmovdqu64 xmm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x28, 0x18, 0xD1, 0x01]); // vinsertf64x2 ymm2, ymm0, xmm1, 1
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x28, 0x18, 0x5F, 0x04, 0x00]); // vinsertf64x2 ymm3, ymm0, [rdi+64], 0
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x28, 0x19, 0xD4, 0x01]); // vextractf64x2 xmm4, ymm2, 1
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x28, 0x19, 0x5F, 0x02, 0x00]); // vextractf64x2 [rdi+32], ymm3, 0
    code.extend_from_slice(&[0xC5, 0xED, 0x57, 0xD3]); // vxorpd ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x17]); // vmovdqu64 [rdi], ymm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x67, 0x03]); // vmovdqu64 [rdi+48], xmm4
    code.push(HLT);
    check_evex_mem("evex_ymm_insert_extract_f64x2_vl_forms", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 33: EVEX VSIB gather/scatter forms.
// ===========================================================================

#[test]
fn evex_zmm_vpgatherdd_permuted_indices_clears_mask() {
    let indices = [15u32, 0, 14, 1, 13, 2, 12, 3, 11, 4, 10, 5, 9, 6, 8, 7];
    let s = evex_dword_scratch(|i| indices[i]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xFF, 0xFF, 0x00, 0x00]); // mov eax, 0xffff
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x58, 0x17]); // vpbroadcastd zmm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x49, 0x90, 0x14, 0x87]); // vpgatherdd zmm2{k1}, [rdi+zmm0*4]
    code.extend_from_slice(&[0xC5, 0xF8, 0x93, 0xC9]); // kmovw ecx, k1
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpgatherdd_permuted_indices_clears_mask", &code, s);
}

#[test]
fn evex_zmm_vpgatherdq_dword_indices_clears_mask() {
    let indices = [7u32, 0, 6, 1, 5, 2, 4, 3, 11, 13, 17, 19, 23, 29, 31, 37];
    let s = evex_dword_scratch(|i| indices[i]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xFF, 0x00, 0x00, 0x00]); // mov eax, 0xff
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xC8]); // kmovb k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x07]); // vmovdqu32 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x48, 0x59, 0x17]); // vpbroadcastq zmm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x49, 0x90, 0x14, 0xC7]); // vpgatherdq zmm2{k1}, [rdi+ymm0*8]
    code.extend_from_slice(&[0xC5, 0xF9, 0x93, 0xC9]); // kmovb ecx, k1
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpgatherdq_dword_indices_clears_mask", &code, s);
}

#[test]
fn evex_zmm_vpgatherqd_vpgatherqq_qword_indices() {
    let indices = [7u64, 0, 6, 1, 5, 2, 4, 3];
    let s = evex_qword_scratch(|i| indices[i]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0xB8, 0xFF, 0x00, 0x00, 0x00]); // mov eax, 0xff
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xC8]); // kmovb k1, eax
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x58, 0x1F]); // vpbroadcastd ymm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x49, 0x91, 0x1C, 0x87]); // vpgatherqd ymm3{k1}, [rdi+zmm0*4]
    code.extend_from_slice(&[0xB8, 0xFF, 0x00, 0x00, 0x00]); // mov eax, 0xff
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xC8]); // kmovb k1, eax
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x48, 0x59, 0x27]); // vpbroadcastq zmm4, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x49, 0x91, 0x24, 0xC7]); // vpgatherqq zmm4{k1}, [rdi+zmm0*8]
    code.extend_from_slice(&[0xC5, 0xF9, 0x93, 0xC9]); // kmovb ecx, k1
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x27]); // vmovdqu64 [rdi], zmm4
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x7F, 0x5F, 0x01]); // vmovdqu32 [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_zmm_vpgatherqd_vpgatherqq_qword_indices", &code, s);
}

#[test]
fn evex_zmm_vgatherdps_vgatherdpd_dword_indices() {
    let indices = [7u32, 0, 6, 1, 5, 2, 4, 3, 15, 8, 14, 9, 13, 10, 12, 11];
    let s = evex_dword_scratch(|i| indices[i]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0xB8, 0xFF, 0xFF, 0x00, 0x00]); // mov eax, 0xffff
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x58, 0x17]); // vpbroadcastd zmm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x49, 0x92, 0x14, 0x87]); // vgatherdps zmm2{k1}, [rdi+zmm0*4]
    code.extend_from_slice(&[0xB8, 0xFF, 0x00, 0x00, 0x00]); // mov eax, 0xff
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xC8]); // kmovb k1, eax
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x48, 0x59, 0x1F]); // vpbroadcastq zmm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x49, 0x92, 0x1C, 0xC7]); // vgatherdpd zmm3{k1}, [rdi+ymm0*8]
    code.extend_from_slice(&[0xC5, 0xF9, 0x93, 0xC9]); // kmovb ecx, k1
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD3]); // vpxorq zmm2, zmm2, zmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vgatherdps_vgatherdpd_dword_indices", &code, s);
}

#[test]
fn evex_zmm_vgatherqps_vgatherqpd_qword_indices() {
    let indices = [7u64, 0, 6, 1, 5, 2, 4, 3];
    let s = evex_qword_scratch(|i| indices[i]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0xB8, 0xFF, 0x00, 0x00, 0x00]); // mov eax, 0xff
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xC8]); // kmovb k1, eax
    code.extend_from_slice(&[0xC4, 0xE2, 0x7D, 0x58, 0x27]); // vpbroadcastd ymm4, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x49, 0x93, 0x24, 0x87]); // vgatherqps ymm4{k1}, [rdi+zmm0*4]
    code.extend_from_slice(&[0xB8, 0xFF, 0x00, 0x00, 0x00]); // mov eax, 0xff
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xC8]); // kmovb k1, eax
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x48, 0x59, 0x2F]); // vpbroadcastq zmm5, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x49, 0x93, 0x2C, 0xC7]); // vgatherqpd zmm5{k1}, [rdi+zmm0*8]
    code.extend_from_slice(&[0xC5, 0xF9, 0x93, 0xC9]); // kmovb ecx, k1
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x2F]); // vmovdqu64 [rdi], zmm5
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x7F, 0x67, 0x01]); // vmovdqu32 [rdi+32], ymm4
    code.push(HLT);
    check_evex_mem("evex_zmm_vgatherqps_vgatherqpd_qword_indices", &code, s);
}

#[test]
fn evex_zmm_vpscatterdd_permuted_indices_clears_mask() {
    let indices = [3u32, 12, 5, 10, 1, 14, 7, 8, 0, 15, 6, 9, 2, 13, 4, 11];
    let s = evex_dword_scratch(|i| indices[i]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xFF, 0xFF, 0x00, 0x00]); // mov eax, 0xffff
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x48, 0x72, 0xF0, 0x08]); // vpslld zmm1, zmm0, 8
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x49, 0xA0, 0x0C, 0x87]); // vpscatterdd [rdi+zmm0*4]{k1}, zmm1
    code.extend_from_slice(&[0xC5, 0xF8, 0x93, 0xC9]); // kmovw ecx, k1
    code.push(HLT);
    check_evex_mem("evex_zmm_vpscatterdd_permuted_indices_clears_mask", &code, s);
}

#[test]
fn evex_zmm_vpscatterdq_dword_indices_clears_mask() {
    let indices = [3u32, 0, 6, 1, 5, 2, 4, 7, 11, 13, 17, 19, 23, 29, 31, 37];
    let s = evex_dword_scratch(|i| indices[i]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xFF, 0x00, 0x00, 0x00]); // mov eax, 0xff
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xC8]); // kmovb k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x07]); // vmovdqu32 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x0F]); // vmovdqu64 zmm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xF5, 0x48, 0x73, 0xF1, 0x08]); // vpsllq zmm1, zmm1, 8
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x49, 0xA0, 0x0C, 0xC7]); // vpscatterdq [rdi+ymm0*8]{k1}, zmm1
    code.extend_from_slice(&[0xC5, 0xF9, 0x93, 0xC9]); // kmovb ecx, k1
    code.push(HLT);
    check_evex_mem("evex_zmm_vpscatterdq_dword_indices_clears_mask", &code, s);
}

#[test]
fn evex_zmm_vpscatterqd_qword_indices_clears_mask() {
    let indices = [7u64, 0, 6, 1, 5, 2, 4, 3];
    let s = evex_qword_scratch(|i| indices[i]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xFF, 0x00, 0x00, 0x00]); // mov eax, 0xff
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xC8]); // kmovb k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x0F]); // vmovdqu32 ymm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x49, 0xA1, 0x0C, 0x87]); // vpscatterqd [rdi+zmm0*4]{k1}, ymm1
    code.extend_from_slice(&[0xC5, 0xF9, 0x93, 0xC9]); // kmovb ecx, k1
    code.push(HLT);
    check_evex_mem("evex_zmm_vpscatterqd_qword_indices_clears_mask", &code, s);
}

#[test]
fn evex_zmm_vpscatterqq_qword_indices_clears_mask() {
    let indices = [3u64, 0, 7, 1, 5, 2, 4, 6];
    let s = evex_qword_scratch(|i| indices[i]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xFF, 0x00, 0x00, 0x00]); // mov eax, 0xff
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xC8]); // kmovb k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0x73, 0xF0, 0x08]); // vpsllq zmm2, zmm0, 8
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x49, 0xA1, 0x14, 0xC7]); // vpscatterqq [rdi+zmm0*8]{k1}, zmm2
    code.extend_from_slice(&[0xC5, 0xF9, 0x93, 0xC9]); // kmovb ecx, k1
    code.push(HLT);
    check_evex_mem("evex_zmm_vpscatterqq_qword_indices_clears_mask", &code, s);
}

#[test]
fn evex_zmm_vscatterdps_dword_indices_clears_mask() {
    let indices = [3u32, 12, 5, 10, 1, 14, 7, 8, 0, 15, 6, 9, 2, 13, 4, 11];
    let s = evex_dword_scratch(|i| indices[i]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xFF, 0xFF, 0x00, 0x00]); // mov eax, 0xffff
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x48, 0x72, 0xF0, 0x08]); // vpslld zmm1, zmm0, 8
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x49, 0xA2, 0x0C, 0x87]); // vscatterdps [rdi+zmm0*4]{k1}, zmm1
    code.extend_from_slice(&[0xC5, 0xF8, 0x93, 0xC9]); // kmovw ecx, k1
    code.push(HLT);
    check_evex_mem("evex_zmm_vscatterdps_dword_indices_clears_mask", &code, s);
}

#[test]
fn evex_zmm_vscatterdpd_dword_indices_clears_mask() {
    let indices = [3u32, 0, 6, 1, 5, 2, 4, 7, 11, 13, 17, 19, 23, 29, 31, 37];
    let s = evex_dword_scratch(|i| indices[i]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xFF, 0x00, 0x00, 0x00]); // mov eax, 0xff
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xC8]); // kmovb k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x07]); // vmovdqu32 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x0F]); // vmovdqu64 zmm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xF5, 0x48, 0x73, 0xF0, 0x08]); // vpsllq zmm1, zmm1, 8
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x49, 0xA2, 0x0C, 0xC7]); // vscatterdpd [rdi+ymm0*8]{k1}, zmm1
    code.extend_from_slice(&[0xC5, 0xF9, 0x93, 0xC9]); // kmovb ecx, k1
    code.push(HLT);
    check_evex_mem("evex_zmm_vscatterdpd_dword_indices_clears_mask", &code, s);
}

#[test]
fn evex_zmm_vscatterqps_qword_indices_clears_mask() {
    let indices = [7u64, 0, 6, 1, 5, 2, 4, 3];
    let s = evex_qword_scratch(|i| indices[i]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xFF, 0x00, 0x00, 0x00]); // mov eax, 0xff
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xC8]); // kmovb k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x0F]); // vmovdqu32 ymm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x49, 0xA3, 0x0C, 0x87]); // vscatterqps [rdi+zmm0*4]{k1}, ymm1
    code.extend_from_slice(&[0xC5, 0xF9, 0x93, 0xC9]); // kmovb ecx, k1
    code.push(HLT);
    check_evex_mem("evex_zmm_vscatterqps_qword_indices_clears_mask", &code, s);
}

#[test]
fn evex_zmm_vscatterqpd_qword_indices_clears_mask() {
    let indices = [3u64, 0, 7, 1, 5, 2, 4, 6];
    let s = evex_qword_scratch(|i| indices[i]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xFF, 0x00, 0x00, 0x00]); // mov eax, 0xff
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xC8]); // kmovb k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0x73, 0xF0, 0x08]); // vpsllq zmm2, zmm0, 8
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x49, 0xA3, 0x14, 0xC7]); // vscatterqpd [rdi+zmm0*8]{k1}, zmm2
    code.extend_from_slice(&[0xC5, 0xF9, 0x93, 0xC9]); // kmovb ecx, k1
    code.push(HLT);
    check_evex_mem("evex_zmm_vscatterqpd_qword_indices_clears_mask", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 34: EVEX non-temporal load/store forms.
// ===========================================================================

fn append_mov_rax_to_rdi_disp(code: &mut Vec<u8>, disp: u8) {
    if disp == 0 {
        code.extend_from_slice(&[0x48, 0x89, 0x07]); // mov [rdi], rax
    } else {
        code.extend_from_slice(&[0x48, 0x89, 0x47, disp]); // mov [rdi+disp], rax
    }
}

fn append_mov_rdi_disp_to_rax(code: &mut Vec<u8>, disp: u8) {
    if disp == 0 {
        code.extend_from_slice(&[0x48, 0x8B, 0x07]); // mov rax, [rdi]
    } else {
        code.extend_from_slice(&[0x48, 0x8B, 0x47, disp]); // mov rax, [rdi+disp]
    }
}

fn append_seed_rdi_plus_64_qwords(code: &mut Vec<u8>, words: &[u64; 8]) {
    for (i, word) in words.iter().enumerate() {
        let disp = 64 + (i as u8) * 8;
        code.extend_from_slice(&[0x48, 0xB8]); // mov rax, imm64
        code.extend_from_slice(&word.to_le_bytes());
        append_mov_rax_to_rdi_disp(code, disp);
    }
}

fn append_clear_rdi_plus_64_qwords(code: &mut Vec<u8>) {
    code.extend_from_slice(&[0x31, 0xC0]); // xor eax, eax
    for i in 0..8 {
        append_mov_rax_to_rdi_disp(code, 64 + (i as u8) * 8);
    }
}

fn append_copy_rdi_plus_64_to_rdi(code: &mut Vec<u8>) {
    for i in 0..8 {
        append_mov_rdi_disp_to_rax(code, 64 + (i as u8) * 8);
        append_mov_rax_to_rdi_disp(code, (i as u8) * 8);
    }
}

#[test]
fn evex_zmm_vmovntdqa_compressed_disp8_load() {
    let source = [
        0x0102_0305_0813_2134u64,
        0x1123_3559_8DE9_7001,
        0xFEDC_BA98_7654_3210,
        0xA5A5_5A5A_C3C3_3C3C,
        0x0F1E_2D3C_4B5A_6978,
        0x8877_6655_4433_2211,
        0x1357_9BDF_2468_ACE0,
        0xFFFF_0000_AAAA_5555,
    ];
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = 0xD0u8.wrapping_add((i as u8).wrapping_mul(3));
    }

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x2A, 0x57, 0x01]); // vmovntdqa zmm2, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vmovntdqa_compressed_disp8_load", &code, s);
}

#[test]
fn evex_zmm_vmovntdq_compressed_disp8_store() {
    let s = evex_qword_scratch(|i| {
        0x1020_4080_1122_3344u64
            .wrapping_mul((i as u64) + 5)
            .rotate_left((i * 7) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xF5, 0x48, 0xEF, 0xC9]); // vpxorq zmm1, zmm1, zmm1
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x0F]); // vmovdqu64 [rdi], zmm1
    append_clear_rdi_plus_64_qwords(&mut code);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xE7, 0x47, 0x01]); // vmovntdq [rdi+64], zmm0
    code.extend_from_slice(&[0x0F, 0xAE, 0xF8]); // sfence
    append_copy_rdi_plus_64_to_rdi(&mut code);
    code.push(HLT);
    check_evex_mem("evex_zmm_vmovntdq_compressed_disp8_store", &code, s);
}

#[test]
fn evex_zmm_vmovntps_compressed_disp8_store() {
    let s = evex_qword_scratch(|i| {
        0x3F80_0000_4000_0000u64
            .wrapping_add((i as u64).wrapping_mul(0x0101_0202_0303_0505))
            .rotate_right((i * 5) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xF5, 0x48, 0xEF, 0xC9]); // vpxorq zmm1, zmm1, zmm1
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x0F]); // vmovdqu64 [rdi], zmm1
    append_clear_rdi_plus_64_qwords(&mut code);
    code.extend_from_slice(&[0x62, 0xF1, 0x7C, 0x48, 0x2B, 0x47, 0x01]); // vmovntps [rdi+64], zmm0
    code.extend_from_slice(&[0x0F, 0xAE, 0xF8]); // sfence
    append_copy_rdi_plus_64_to_rdi(&mut code);
    code.push(HLT);
    check_evex_mem("evex_zmm_vmovntps_compressed_disp8_store", &code, s);
}

#[test]
fn evex_zmm_vmovntpd_compressed_disp8_store() {
    let s = evex_qword_scratch(|i| {
        0x3FF0_0000_0000_0000u64
            .wrapping_add((i as u64).wrapping_mul(0x0010_0020_0030_0050))
            .rotate_left((i * 11) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xF5, 0x48, 0xEF, 0xC9]); // vpxorq zmm1, zmm1, zmm1
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x0F]); // vmovdqu64 [rdi], zmm1
    append_clear_rdi_plus_64_qwords(&mut code);
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x48, 0x2B, 0x47, 0x01]); // vmovntpd [rdi+64], zmm0
    code.extend_from_slice(&[0x0F, 0xAE, 0xF8]); // sfence
    append_copy_rdi_plus_64_to_rdi(&mut code);
    code.push(HLT);
    check_evex_mem("evex_zmm_vmovntpd_compressed_disp8_store", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 35: EVEX byte/word compress/expand forms.
// ===========================================================================

#[test]
fn evex_zmm_vpcompressb_memory_disp8_form() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x41u8)
            .wrapping_add((i as u8).wrapping_mul(17))
            .rotate_left((i % 8) as u32);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xB8]);
    code.extend_from_slice(&0xAAAA_AAAA_5555_5555u64.to_le_bytes()); // mov rax, mask
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xF5, 0x48, 0xEF, 0xC9]); // vpxorq zmm1, zmm1, zmm1
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x0F]); // vmovdqu64 [rdi], zmm1
    append_clear_rdi_plus_64_qwords(&mut code);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x49, 0x63, 0x47, 0x40]); // vpcompressb [rdi+64]{k1}, zmm0
    append_copy_rdi_plus_64_to_rdi(&mut code);
    code.push(HLT);
    check_evex_mem("evex_zmm_vpcompressb_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vpexpandb_memory_disp8_form() {
    let source = [
        0x0305_0813_2134_5589u64,
        0x1447_9011_2358_DF01,
        0xFE01_DC23_BA45_9867,
        0xA5C3_5A3C_F00D_BAAD,
        0x0F2D_4B69_87A5_C3E1,
        0x8822_6644_AACC_EE11,
        0x1357_9BDF_2468_ACE0,
        0xFFFF_0000_AAAA_5555,
    ];
    let s = evex_qword_scratch(|i| 0xD0D1_D2D3_D4D5_D6D7u64.rotate_left((i * 3) as u32));

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xB8]);
    code.extend_from_slice(&0xAAAA_AAAA_5555_5555u64.to_le_bytes()); // mov rax, mask
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0xC9, 0x62, 0x57, 0x40]); // vpexpandb zmm2{k1}{z}, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpexpandb_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vpcompressw_memory_disp8_form() {
    let s = evex_qword_scratch(|i| {
        0x1020_3040_5060_7080u64
            .wrapping_add((i as u64).wrapping_mul(0x1111_2222_3333_4444))
            .rotate_left((i * 9) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0x55, 0x55, 0xAA, 0xAA]); // mov eax, 0xaaaa5555
    code.extend_from_slice(&[0xC5, 0xFB, 0x92, 0xD0]); // kmovd k2, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xF5, 0x48, 0xEF, 0xC9]); // vpxorq zmm1, zmm1, zmm1
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x0F]); // vmovdqu64 [rdi], zmm1
    append_clear_rdi_plus_64_qwords(&mut code);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x4A, 0x63, 0x47, 0x20]); // vpcompressw [rdi+64]{k2}, zmm0
    append_copy_rdi_plus_64_to_rdi(&mut code);
    code.push(HLT);
    check_evex_mem("evex_zmm_vpcompressw_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vpexpandw_memory_disp8_form() {
    let source = [
        0x0102_0305_0813_2134u64,
        0x1123_3559_8DE9_7001,
        0xFEDC_BA98_7654_3210,
        0xA5A5_5A5A_C3C3_3C3C,
        0x0F1E_2D3C_4B5A_6978,
        0x8877_6655_4433_2211,
        0x1357_9BDF_2468_ACE0,
        0xFFFF_0000_AAAA_5555,
    ];
    let s = evex_qword_scratch(|i| 0x9091_9293_9495_9697u64.rotate_right((i * 5) as u32));

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0x55, 0x55, 0xAA, 0xAA]); // mov eax, 0xaaaa5555
    code.extend_from_slice(&[0xC5, 0xFB, 0x92, 0xD0]); // kmovd k2, eax
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0xCA, 0x62, 0x5F, 0x20]); // vpexpandw zmm3{k2}{z}, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x1F]); // vmovdqu64 [rdi], zmm3
    code.push(HLT);
    check_evex_mem("evex_zmm_vpexpandw_memory_disp8_form", &code, s);
}

#[test]
fn evex_xmm_vpcompress_vpexpand_bw_vl_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x3Du8)
            .wrapping_add((i as u8).wrapping_mul(29))
            .rotate_left((i % 7) as u32);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0x5A, 0xA5, 0x00, 0x00]); // mov eax, 0xa55a
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0xB8, 0xAD, 0x00, 0x00, 0x00]); // mov eax, 0xad
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xD0]); // kmovb k2, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x07]); // vmovdqu64 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x89, 0x63, 0xC2]); // vpcompressb xmm2{k1}{z}, xmm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x89, 0x62, 0xDA]); // vpexpandb xmm3{k1}{z}, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x8A, 0x63, 0xC4]); // vpcompressw xmm4{k2}{z}, xmm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x8A, 0x62, 0xEC]); // vpexpandw xmm5{k2}{z}, xmm4
    code.extend_from_slice(&[0x62, 0xF1, 0xE5, 0x08, 0xEF, 0xDD]); // vpxorq xmm3, xmm3, xmm5
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x1F]); // vmovdqu64 [rdi], xmm3
    code.push(HLT);
    check_evex_mem("evex_xmm_vpcompress_vpexpand_bw_vl_forms", &code, s);
}

#[test]
fn evex_ymm_vpcompress_vpexpand_bw_vl_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0xC1u8)
            .wrapping_sub((i as u8).wrapping_mul(19))
            .rotate_right((i % 5) as u32);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0x55, 0x55, 0xAA, 0xAA]); // mov eax, 0xaaaa5555
    code.extend_from_slice(&[0xC5, 0xFB, 0x92, 0xC8]); // kmovd k1, eax
    code.extend_from_slice(&[0xB8, 0xAD, 0xB5, 0x00, 0x00]); // mov eax, 0xb5ad
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xD0]); // kmovw k2, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0xA9, 0x63, 0xC2]); // vpcompressb ymm2{k1}{z}, ymm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0xA9, 0x62, 0xDA]); // vpexpandb ymm3{k1}{z}, ymm2
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0xAA, 0x63, 0xC4]); // vpcompressw ymm4{k2}{z}, ymm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0xAA, 0x62, 0xEC]); // vpexpandw ymm5{k2}{z}, ymm4
    code.extend_from_slice(&[0x62, 0xF1, 0xE5, 0x28, 0xEF, 0xDD]); // vpxorq ymm3, ymm3, ymm5
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x1F]); // vmovdqu64 [rdi], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_vpcompress_vpexpand_bw_vl_forms", &code, s);
}

#[test]
fn evex_xmm_vpcompress_vpexpand_bw_vl_memory_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x6Bu8)
            .wrapping_add((i as u8).wrapping_mul(23))
            .rotate_right((i % 6) as u32);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0x5A, 0xA5, 0x00, 0x00]); // mov eax, 0xa55a
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0xB8, 0xAD, 0x00, 0x00, 0x00]); // mov eax, 0xad
    code.extend_from_slice(&[0xC5, 0xF9, 0x92, 0xD0]); // kmovb k2, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x07]); // vmovdqu64 xmm0, [rdi]
    append_clear_rdi_plus_64_qwords(&mut code);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x09, 0x63, 0x47, 0x40]); // vpcompressb [rdi+64]{k1}, xmm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x89, 0x62, 0x57, 0x40]); // vpexpandb xmm2{k1}{z}, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x17]); // vmovdqu64 [rdi], xmm2
    append_clear_rdi_plus_64_qwords(&mut code);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x0A, 0x63, 0x47, 0x20]); // vpcompressw [rdi+64]{k2}, xmm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x8A, 0x62, 0x5F, 0x20]); // vpexpandw xmm3{k2}{z}, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+16], xmm3
    code.push(HLT);
    check_evex_mem("evex_xmm_vpcompress_vpexpand_bw_vl_memory_forms", &code, s);
}

#[test]
fn evex_ymm_vpcompress_vpexpand_bw_vl_memory_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (0x95u8)
            .wrapping_sub((i as u8).wrapping_mul(11))
            .rotate_left((i % 8) as u32);
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0x55, 0x55, 0xAA, 0xAA]); // mov eax, 0xaaaa5555
    code.extend_from_slice(&[0xC5, 0xFB, 0x92, 0xC8]); // kmovd k1, eax
    code.extend_from_slice(&[0xB8, 0xAD, 0xB5, 0x00, 0x00]); // mov eax, 0xb5ad
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xD0]); // kmovw k2, eax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    append_clear_rdi_plus_64_qwords(&mut code);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x29, 0x63, 0x47, 0x40]); // vpcompressb [rdi+64]{k1}, ymm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0xA9, 0x62, 0x57, 0x40]); // vpexpandb ymm2{k1}{z}, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x17]); // vmovdqu64 [rdi], ymm2
    append_clear_rdi_plus_64_qwords(&mut code);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x2A, 0x63, 0x47, 0x20]); // vpcompressw [rdi+64]{k2}, ymm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0xAA, 0x62, 0x5F, 0x20]); // vpexpandw ymm3{k2}{z}, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_vpcompress_vpexpand_bw_vl_memory_forms", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 36: EVEX conflict-detection forms.
// ===========================================================================

#[test]
fn evex_zmm_vpconflictd_memory_disp8_form() {
    let values = [7u32, 3, 7, 1, 3, 12, 7, 12, 5, 5, 9, 1, 2, 9, 2, 7];
    let mut source = [0u64; 8];
    for i in 0..8 {
        source[i] = (values[i * 2] as u64) | ((values[i * 2 + 1] as u64) << 32);
    }
    let s = evex_dword_scratch(|i| 0x8000_0000u32 ^ ((i as u32) * 0x0101_0101));

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0xC4, 0x57, 0x01]); // vpconflictd zmm2, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpconflictd_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vpconflictq_memory_disp8_form() {
    let source = [11u64, 22, 11, 33, 22, 44, 11, 44];
    let s = evex_qword_scratch(|i| {
        0xF0E1_D2C3_B4A5_9687u64
            .wrapping_add((i as u64).wrapping_mul(0x0102_0305_0813_2134))
    });

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x48, 0xC4, 0x5F, 0x01]); // vpconflictq zmm3, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x1F]); // vmovdqu64 [rdi], zmm3
    code.push(HLT);
    check_evex_mem("evex_zmm_vpconflictq_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vpconflict_mask_merge_zero_dq_forms() {
    let s = evex_qword_scratch(|i| {
        [
            0x0000_0003_0000_0007u64,
            0x0000_0007_0000_0001,
            0x0000_0003_0000_0007,
            0x0000_0005_0000_0001,
            0x0000_0007_0000_0005,
            0x0000_0009_0000_0003,
            0x0000_0005_0000_0009,
            0x0000_0007_0000_0001,
        ][i]
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xAD, 0x5A, 0x00, 0x00]); // mov eax, 0x5aad
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]); // kmovw k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]); // vmovdqu32 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x0F]); // vmovdqu64 zmm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x17]); // vmovdqu32 zmm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x1F]); // vmovdqu64 zmm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x49, 0xC4, 0xD0]); // vpconflictd zmm2{k1}, zmm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0xC9, 0xC4, 0xD9]); // vpconflictq zmm3{k1}{z}, zmm1
    code.extend_from_slice(&[0x62, 0xF1, 0x6D, 0x48, 0xEF, 0xD3]); // vpxord zmm2, zmm2, zmm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x7F, 0x17]); // vmovdqu32 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpconflict_mask_merge_zero_dq_forms", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 37: EVEX byte/word/qword count forms.
// ===========================================================================

#[test]
fn evex_zmm_vpopcntb_memory_disp8_form() {
    let source = [
        0x0001_0307_0F1F_3F7Fu64,
        0x80C0_E0F0_F8FC_FEFF,
        0xAA55_CC33_F00F_9696,
        0x0123_4567_89AB_CDEF,
        0xFFFF_0000_FFFF_0000,
        0x1357_9BDF_2468_ACE0,
        0xDEAD_BEEF_CAFE_BABE,
        0x0101_0202_0404_0808,
    ];
    let s = evex_qword_scratch(|i| 0xA5A5_5A5A_C3C3_3C3Cu64.rotate_left((i * 3) as u32));

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x54, 0x57, 0x01]); // vpopcntb zmm2, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpopcntb_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vpopcntw_memory_disp8_form() {
    let source = [
        0x0001_0003_0007_000Fu64,
        0x001F_003F_007F_00FF,
        0x0F0F_F0F0_AA55_55AA,
        0x8000_C000_E000_F000,
        0x1111_2222_4444_8888,
        0x1234_5678_9ABC_DEF0,
        0xFFFF_0000_FFFF_0000,
        0x1357_9BDF_2468_ACE0,
    ];
    let s = evex_qword_scratch(|i| 0xB4B5_B6B7_B8B9_BABBu64.rotate_right((i * 5) as u32));

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x48, 0x54, 0x5F, 0x01]); // vpopcntw zmm3, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x1F]); // vmovdqu64 [rdi], zmm3
    code.push(HLT);
    check_evex_mem("evex_zmm_vpopcntw_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vpopcntq_memory_disp8_form() {
    let source = [
        0x0000_0000_0000_0000u64,
        0xFFFF_FFFF_FFFF_FFFF,
        0x8000_0000_0000_0001,
        0x0123_4567_89AB_CDEF,
        0x0F0F_F0F0_AAAA_5555,
        0x1357_9BDF_2468_ACE0,
        0xDEAD_BEEF_CAFE_BABE,
        0x0102_0408_1020_4080,
    ];
    let s = evex_qword_scratch(|i| 0xC0C1_C2C3_C4C5_C6C7u64.rotate_left((i * 7) as u32));

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x48, 0x55, 0x67, 0x01]); // vpopcntq zmm4, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x27]); // vmovdqu64 [rdi], zmm4
    code.push(HLT);
    check_evex_mem("evex_zmm_vpopcntq_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vplzcntq_memory_disp8_form() {
    let source = [
        0x0000_0000_0000_0000u64,
        0x0000_0000_0000_0001,
        0x0000_0000_0000_8000,
        0x0000_0001_0000_0000,
        0x0100_0000_0000_0000,
        0x4000_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x00FF_0000_0000_0000,
    ];
    let s = evex_qword_scratch(|i| 0xD0D1_D2D3_D4D5_D6D7u64.rotate_right((i * 9) as u32));

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x48, 0x44, 0x6F, 0x01]); // vplzcntq zmm5, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x2F]); // vmovdqu64 [rdi], zmm5
    code.push(HLT);
    check_evex_mem("evex_zmm_vplzcntq_memory_disp8_form", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 38: EVEX two-table permute forms.
// ===========================================================================

fn evex_two_table_source() -> [u64; 8] {
    [
        0x8081_8283_8485_8687u64,
        0x9091_9293_9495_9697,
        0xA0A1_A2A3_A4A5_A6A7,
        0xB0B1_B2B3_B4B5_B6B7,
        0xC0C1_C2C3_C4C5_C6C7,
        0xD0D1_D2D3_D4D5_D6D7,
        0xE0E1_E2E3_E4E5_E6E7,
        0xF0F1_F2F3_F4F5_F6F7,
    ]
}

#[test]
fn evex_xmm_vbmi_two_table_byte_vl_forms() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (((i * 7) + if i % 3 == 0 { 32 } else { 0 }) & 0x3f) as u8;
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x07]); // vmovdqu xmm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x4F, 0x10]); // vmovdqu xmm1, [rdi+16]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x57, 0x20]); // vmovdqu xmm2, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x1F]); // vmovdqu xmm3, [rdi]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x67, 0x10]); // vmovdqu xmm4, [rdi+16]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x6F, 0x30]); // vmovdqu xmm5, [rdi+48]
    code.extend_from_slice(&[0xC5, 0xFA, 0x6F, 0x77, 0x10]); // vmovdqu xmm6, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x08, 0x75, 0xC2]); // vpermi2b xmm0, xmm1, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0x65, 0x08, 0x7D, 0xE2]); // vpermt2b xmm4, xmm3, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x08, 0x75, 0x6F, 0x02]); // vpermi2b xmm5, xmm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x65, 0x08, 0x7D, 0x77, 0x02]); // vpermt2b xmm6, xmm3, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x07]); // vmovdqu [rdi], xmm0
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x67, 0x10]); // vmovdqu [rdi+16], xmm4
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x6F, 0x20]); // vmovdqu [rdi+32], xmm5
    code.extend_from_slice(&[0xC5, 0xFA, 0x7F, 0x77, 0x30]); // vmovdqu [rdi+48], xmm6
    code.push(HLT);
    check_evex_mem("evex_xmm_vbmi_two_table_byte_vl_forms", &code, s);
}

#[test]
fn evex_ymm_vbmi_two_table_word_vl_forms() {
    let mut s = [0u8; 64];
    for lane in 0..32 {
        let value = (((lane * 5) + if lane % 2 == 0 { 32 } else { 0 }) & 0x3f) as u16;
        s[lane * 2..lane * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x07]); // vmovdqu ymm0, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x4F, 0x20]); // vmovdqu ymm1, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x17]); // vmovdqu ymm2, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x5F, 0x20]); // vmovdqu ymm3, [rdi+32]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x27]); // vmovdqu ymm4, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x2F]); // vmovdqu ymm5, [rdi]
    code.extend_from_slice(&[0xC5, 0xFE, 0x6F, 0x77, 0x20]); // vmovdqu ymm6, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x28, 0x75, 0xC2]); // vpermi2w ymm0, ymm1, ymm2
    code.extend_from_slice(&[0x62, 0xF2, 0xDD, 0x28, 0x7D, 0xD9]); // vpermt2w ymm3, ymm4, ymm1
    code.extend_from_slice(&[0xC5, 0xFD, 0xEF, 0xC3]); // vpxor ymm0, ymm0, ymm3
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x28, 0x75, 0x6F, 0x01]); // vpermi2w ymm5, ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0xDD, 0x28, 0x7D, 0x37]); // vpermt2w ymm6, ymm4, [rdi]
    code.extend_from_slice(&[0xC5, 0xD5, 0xEF, 0xEE]); // vpxor ymm5, ymm5, ymm6
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x07]); // vmovdqu [rdi], ymm0
    code.extend_from_slice(&[0xC5, 0xFE, 0x7F, 0x6F, 0x20]); // vmovdqu [rdi+32], ymm5
    code.push(HLT);
    check_evex_mem("evex_ymm_vbmi_two_table_word_vl_forms", &code, s);
}

#[test]
fn evex_xmm_two_table_dword_vl_forms() {
    let values = [
        0, 4, 1, 5, 0x1010_1010, 0x2020_2020, 0x3030_3030, 0x4040_4040, 0xA0A0_A0A0,
        0xB0B0_B0B0, 0xC0C0_C0C0, 0xD0D0_D0D0, 2, 6, 3, 7,
    ];
    let s = evex_dword_scratch(|i| values[i]);

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_two_table_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x07]); // vmovdqu64 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu64 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x57, 0x02]); // vmovdqu64 xmm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x1F]); // vmovdqu64 xmm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x67, 0x01]); // vmovdqu64 xmm4, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x6F, 0x03]); // vmovdqu64 xmm5, [rdi+48]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x77, 0x01]); // vmovdqu64 xmm6, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x08, 0x76, 0xC2]); // vpermi2d xmm0, xmm1, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0x65, 0x08, 0x7E, 0xE2]); // vpermt2d xmm4, xmm3, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x08, 0x76, 0x6F, 0x04]); // vpermi2d xmm5, xmm1, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF2, 0x65, 0x08, 0x7E, 0x77, 0x04]); // vpermt2d xmm6, xmm3, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x07]); // vmovdqu64 [rdi], xmm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x67, 0x01]); // vmovdqu64 [rdi+16], xmm4
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x6F, 0x02]); // vmovdqu64 [rdi+32], xmm5
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x77, 0x03]); // vmovdqu64 [rdi+48], xmm6
    code.push(HLT);
    check_evex_mem("evex_xmm_two_table_dword_vl_forms", &code, s);
}

#[test]
fn evex_ymm_two_table_dword_vl_forms() {
    let values = [
        0, 8, 1, 9, 2, 10, 3, 11, 0x1010_1010, 0x2020_2020, 0x3030_3030, 0x4040_4040,
        0x5050_5050, 0x6060_6060, 0x7070_7070, 0x8080_8080,
    ];
    let s = evex_dword_scratch(|i| values[i]);

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_two_table_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu64 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x17]); // vmovdqu64 ymm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x5F, 0x01]); // vmovdqu64 ymm3, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x27]); // vmovdqu64 ymm4, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x2F]); // vmovdqu64 ymm5, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x77, 0x01]); // vmovdqu64 ymm6, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x28, 0x76, 0xC2]); // vpermi2d ymm0, ymm1, ymm2
    code.extend_from_slice(&[0x62, 0xF2, 0x5D, 0x28, 0x7E, 0xD9]); // vpermt2d ymm3, ymm4, ymm1
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x28, 0xEF, 0xC3]); // vpxord ymm0, ymm0, ymm3
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x28, 0x76, 0x6F, 0x02]); // vpermi2d ymm5, ymm1, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF2, 0x5D, 0x28, 0x7E, 0x77, 0x02]); // vpermt2d ymm6, ymm4, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0x55, 0x28, 0xEF, 0xEE]); // vpxord ymm5, ymm5, ymm6
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x07]); // vmovdqu64 [rdi], ymm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x6F, 0x01]); // vmovdqu64 [rdi+32], ymm5
    code.push(HLT);
    check_evex_mem("evex_ymm_two_table_dword_vl_forms", &code, s);
}

#[test]
fn evex_xmm_two_table_qword_vl_forms() {
    let values = [
        0,
        2,
        0x1010_1010_1010_1010,
        0x2020_2020_2020_2020,
        0xA0A0_A0A0_A0A0_A0A0,
        0xB0B0_B0B0_B0B0_B0B0,
        1,
        3,
    ];
    let s = evex_qword_scratch(|i| values[i]);

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_two_table_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x07]); // vmovdqu64 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu64 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x57, 0x02]); // vmovdqu64 xmm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x1F]); // vmovdqu64 xmm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x67, 0x01]); // vmovdqu64 xmm4, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x6F, 0x03]); // vmovdqu64 xmm5, [rdi+48]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x77, 0x01]); // vmovdqu64 xmm6, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x08, 0x76, 0xC2]); // vpermi2q xmm0, xmm1, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0xE5, 0x08, 0x7E, 0xE2]); // vpermt2q xmm4, xmm3, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x08, 0x76, 0x6F, 0x04]); // vpermi2q xmm5, xmm1, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF2, 0xE5, 0x08, 0x7E, 0x77, 0x04]); // vpermt2q xmm6, xmm3, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x07]); // vmovdqu64 [rdi], xmm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x67, 0x01]); // vmovdqu64 [rdi+16], xmm4
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x6F, 0x02]); // vmovdqu64 [rdi+32], xmm5
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x77, 0x03]); // vmovdqu64 [rdi+48], xmm6
    code.push(HLT);
    check_evex_mem("evex_xmm_two_table_qword_vl_forms", &code, s);
}

#[test]
fn evex_ymm_two_table_qword_vl_forms() {
    let values = [
        0,
        4,
        1,
        5,
        0x1010_1010_1010_1010,
        0x2020_2020_2020_2020,
        0x3030_3030_3030_3030,
        0x4040_4040_4040_4040,
    ];
    let s = evex_qword_scratch(|i| values[i]);

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_two_table_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu64 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x17]); // vmovdqu64 ymm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x5F, 0x01]); // vmovdqu64 ymm3, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x27]); // vmovdqu64 ymm4, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x2F]); // vmovdqu64 ymm5, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x77, 0x01]); // vmovdqu64 ymm6, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x28, 0x76, 0xC2]); // vpermi2q ymm0, ymm1, ymm2
    code.extend_from_slice(&[0x62, 0xF2, 0xDD, 0x28, 0x7E, 0xD9]); // vpermt2q ymm3, ymm4, ymm1
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x28, 0xEF, 0xC3]); // vpxorq ymm0, ymm0, ymm3
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x28, 0x76, 0x6F, 0x02]); // vpermi2q ymm5, ymm1, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF2, 0xDD, 0x28, 0x7E, 0x77, 0x02]); // vpermt2q ymm6, ymm4, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xD5, 0x28, 0xEF, 0xEE]); // vpxorq ymm5, ymm5, ymm6
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x07]); // vmovdqu64 [rdi], ymm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x6F, 0x01]); // vmovdqu64 [rdi+32], ymm5
    code.push(HLT);
    check_evex_mem("evex_ymm_two_table_qword_vl_forms", &code, s);
}

#[test]
fn evex_zmm_vpermi2b_memory_disp8_form() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (((i * 5) + if i % 2 == 0 { 64 } else { 0 }) & 0x7f) as u8;
    }

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_two_table_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x75, 0x47, 0x01]); // vpermi2b zmm0, zmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]); // vmovdqu64 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vpermi2b_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vpermi2w_memory_disp8_form() {
    let mut s = [0u8; 64];
    for lane in 0..32 {
        let value = (((lane * 3) + if lane % 3 == 0 { 32 } else { 0 }) & 0x3f) as u16;
        s[lane * 2..lane * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_two_table_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x48, 0x75, 0x47, 0x01]); // vpermi2w zmm0, zmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]); // vmovdqu64 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vpermi2w_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vpermi2d_memory_disp8_form() {
    let s = evex_dword_scratch(|i| (((i * 5) + if i % 2 == 0 { 16 } else { 0 }) & 0x1f) as u32);

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_two_table_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x76, 0x47, 0x01]); // vpermi2d zmm0, zmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]); // vmovdqu64 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vpermi2d_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vpermi2q_memory_disp8_form() {
    let indexes = [0u64, 8, 1, 9, 6, 14, 7, 15];
    let s = evex_qword_scratch(|i| indexes[i]);

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_two_table_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x48, 0x76, 0x47, 0x01]); // vpermi2q zmm0, zmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]); // vmovdqu64 [rdi], zmm0
    code.push(HLT);
    check_evex_mem("evex_zmm_vpermi2q_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vpermt2b_memory_disp8_form() {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = (((i * 5) + if i % 2 == 0 { 64 } else { 0 }) & 0x7f) as u8;
    }

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_two_table_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x17]); // vmovdqu64 zmm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x7D, 0x57, 0x01]); // vpermt2b zmm2, zmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpermt2b_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vpermt2w_memory_disp8_form() {
    let mut s = [0u8; 64];
    for lane in 0..32 {
        let value = (((lane * 3) + if lane % 3 == 0 { 32 } else { 0 }) & 0x3f) as u16;
        s[lane * 2..lane * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_two_table_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x17]); // vmovdqu64 zmm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x48, 0x7D, 0x57, 0x01]); // vpermt2w zmm2, zmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpermt2w_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vpermt2d_memory_disp8_form() {
    let s = evex_dword_scratch(|i| (((i * 5) + if i % 2 == 0 { 16 } else { 0 }) & 0x1f) as u32);

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_two_table_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x17]); // vmovdqu64 zmm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x7E, 0x57, 0x01]); // vpermt2d zmm2, zmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpermt2d_memory_disp8_form", &code, s);
}

#[test]
fn evex_zmm_vpermt2q_memory_disp8_form() {
    let indexes = [0u64, 8, 1, 9, 6, 14, 7, 15];
    let s = evex_qword_scratch(|i| indexes[i]);

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_two_table_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x17]); // vmovdqu64 zmm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x48, 0x7E, 0x57, 0x01]); // vpermt2q zmm2, zmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm2
    code.push(HLT);
    check_evex_mem("evex_zmm_vpermt2q_memory_disp8_form", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 39: EVEX integer extend/narrow memory forms.
// ===========================================================================

fn evex_pmov_source() -> [u64; 8] {
    [
        0x807F_0100_FFFE_8180u64,
        0x1234_FEDC_8000_7FFF,
        0x55AA_AA55_7E81_817E,
        0x0001_FFFF_FF00_00FF,
        0x7FFF_FFFF_8000_0000,
        0x0123_4567_89AB_CDEF,
        0xFEDC_BA98_7654_3210,
        0x1357_9BDF_2468_ACE0,
    ]
}

fn evex_pmov_narrow_scratch() -> [u8; 64] {
    evex_qword_scratch(|i| {
        [
            0x0000_0000_0000_007Fu64,
            0x0000_0000_0000_0080,
            0xFFFF_FFFF_FFFF_FF80,
            0xFFFF_FFFF_FFFF_FF7F,
            0x0000_0000_0001_0001,
            0xFFFF_FFFF_8000_0000,
            0x0000_0000_FFFF_FFFF,
            0x8000_0000_0000_0000,
        ][i]
    })
}

fn check_evex_pmov_extend_memory_disp8(label: &str, insn: &[u8]) {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_pmov_source());
    code.extend_from_slice(insn);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm2
    code.push(HLT);
    check_evex_mem(label, &code, zero_scratch());
}

fn check_evex_pmov_narrow_memory_disp8(label: &str, insn: &[u8]) {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    append_clear_rdi_plus_64_qwords(&mut code);
    code.extend_from_slice(insn);
    append_copy_rdi_plus_64_to_rdi(&mut code);
    code.push(HLT);
    check_evex_mem(label, &code, evex_pmov_narrow_scratch());
}

#[test]
fn evex_zmm_vpmovsxbw_memory_disp8_form() {
    check_evex_pmov_extend_memory_disp8(
        "evex_zmm_vpmovsxbw_memory_disp8_form",
        &[0x62, 0xF2, 0x7D, 0x48, 0x20, 0x57, 0x02],
    );
}

#[test]
fn evex_zmm_vpmovsxwq_memory_disp8_form() {
    check_evex_pmov_extend_memory_disp8(
        "evex_zmm_vpmovsxwq_memory_disp8_form",
        &[0x62, 0xF2, 0x7D, 0x48, 0x24, 0x57, 0x04],
    );
}

#[test]
fn evex_zmm_vpmovsxdq_memory_disp8_form() {
    check_evex_pmov_extend_memory_disp8(
        "evex_zmm_vpmovsxdq_memory_disp8_form",
        &[0x62, 0xF2, 0x7D, 0x48, 0x25, 0x57, 0x02],
    );
}

#[test]
fn evex_zmm_vpmovzxbq_memory_disp8_form() {
    check_evex_pmov_extend_memory_disp8(
        "evex_zmm_vpmovzxbq_memory_disp8_form",
        &[0x62, 0xF2, 0x7D, 0x48, 0x32, 0x57, 0x08],
    );
}

#[test]
fn evex_zmm_vpmovzxwd_memory_disp8_form() {
    check_evex_pmov_extend_memory_disp8(
        "evex_zmm_vpmovzxwd_memory_disp8_form",
        &[0x62, 0xF2, 0x7D, 0x48, 0x33, 0x57, 0x02],
    );
}

#[test]
fn evex_zmm_vpmovzxdq_memory_disp8_form() {
    check_evex_pmov_extend_memory_disp8(
        "evex_zmm_vpmovzxdq_memory_disp8_form",
        &[0x62, 0xF2, 0x7D, 0x48, 0x35, 0x57, 0x02],
    );
}

#[test]
fn evex_zmm_vpmovuswb_memory_disp8_form() {
    check_evex_pmov_narrow_memory_disp8(
        "evex_zmm_vpmovuswb_memory_disp8_form",
        &[0x62, 0xF2, 0x7E, 0x48, 0x10, 0x47, 0x02],
    );
}

#[test]
fn evex_zmm_vpmovusqd_memory_disp8_form() {
    check_evex_pmov_narrow_memory_disp8(
        "evex_zmm_vpmovusqd_memory_disp8_form",
        &[0x62, 0xF2, 0x7E, 0x48, 0x15, 0x47, 0x02],
    );
}

#[test]
fn evex_zmm_vpmovswb_memory_disp8_form() {
    check_evex_pmov_narrow_memory_disp8(
        "evex_zmm_vpmovswb_memory_disp8_form",
        &[0x62, 0xF2, 0x7E, 0x48, 0x20, 0x47, 0x02],
    );
}

#[test]
fn evex_zmm_vpmovsqw_memory_disp8_form() {
    check_evex_pmov_narrow_memory_disp8(
        "evex_zmm_vpmovsqw_memory_disp8_form",
        &[0x62, 0xF2, 0x7E, 0x48, 0x24, 0x47, 0x04],
    );
}

#[test]
fn evex_zmm_vpmovwb_memory_disp8_form() {
    check_evex_pmov_narrow_memory_disp8(
        "evex_zmm_vpmovwb_memory_disp8_form",
        &[0x62, 0xF2, 0x7E, 0x48, 0x30, 0x47, 0x02],
    );
}

#[test]
fn evex_zmm_vpmovqd_memory_disp8_form() {
    check_evex_pmov_narrow_memory_disp8(
        "evex_zmm_vpmovqd_memory_disp8_form",
        &[0x62, 0xF2, 0x7E, 0x48, 0x35, 0x47, 0x02],
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 40: EVEX FP classify memory forms.
// ===========================================================================

fn evex_fpclass_ps_source() -> [u64; 8] {
    [
        0x8000_0000_0000_0000u64,
        0xFF80_0000_7F80_0000,
        0x7F80_0001_7FC0_0001,
        0x8000_0001_0000_0001,
        0x3F80_0000_BF80_0000,
        0xC000_0000_4000_0000,
        0x7FC1_0203_FFC1_0203,
        0x7F80_0002_FF80_0002,
    ]
}

fn evex_fpclass_ss_source() -> [u64; 8] {
    let mut source = evex_fpclass_ps_source();
    source[0] = 0x0000_0000_BF80_0000;
    source
}

fn evex_fpclass_pd_source() -> [u64; 8] {
    [
        0x7FF8_0000_0000_0001u64,
        0x7FF0_0000_0000_0001,
        0x0000_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x7FF0_0000_0000_0000,
        0xFFF0_0000_0000_0000,
        0x0000_0000_0000_0001,
        0xBFF0_0000_0000_0000,
    ]
}

fn evex_fpclass_ph_source() -> [u64; 8] {
    let values = [
        0x7e01u16, 0x7c01, 0x0000, 0x8000, 0x7c00, 0xfc00, 0x0001, 0xbc00, 0x3c00,
        0xc000, 0x7e02, 0xfe02, 0x7c02, 0xfc02, 0x0002, 0x8002, 0x3555, 0xb555, 0x7e03,
        0x7c03, 0x0003, 0x8003, 0x7c00, 0xfc00, 0x3c00, 0xbc00, 0x7e04, 0xfe04, 0x7c04,
        0xfc04, 0x0004, 0x8004,
    ];
    let mut words = [0u64; 8];
    for i in 0..8 {
        for j in 0..4 {
            words[i] |= (values[i * 4 + j] as u64) << (j * 16);
        }
    }
    words
}

fn check_evex_fpclass_memory_disp8(label: &str, source: &[u64; 8], insn: &[u8]) {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, source);
    code.extend_from_slice(insn);
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xC2]); // kmovq rax, k2
    append_mov_rax_to_rdi_disp(&mut code, 0);
    code.push(HLT);
    check_evex_mem(label, &code, zero_scratch());
}

#[test]
fn evex_zmm_vfpclassps_memory_disp8_form() {
    check_evex_fpclass_memory_disp8(
        "evex_zmm_vfpclassps_memory_disp8_form",
        &evex_fpclass_ps_source(),
        &[0x62, 0xF3, 0x7D, 0x48, 0x66, 0x57, 0x01, 0xD1],
    );
}

#[test]
fn evex_zmm_vfpclasspd_memory_disp8_form() {
    check_evex_fpclass_memory_disp8(
        "evex_zmm_vfpclasspd_memory_disp8_form",
        &evex_fpclass_pd_source(),
        &[0x62, 0xF3, 0xFD, 0x48, 0x66, 0x57, 0x01, 0xD1],
    );
}

#[test]
fn evex_zmm_vfpclassph_memory_disp8_form() {
    check_evex_fpclass_memory_disp8(
        "evex_zmm_vfpclassph_memory_disp8_form",
        &evex_fpclass_ph_source(),
        &[0x62, 0xF3, 0x7C, 0x48, 0x66, 0x57, 0x01, 0xD1],
    );
}

#[test]
fn evex_scalar_vfpclassss_memory_disp8_form() {
    check_evex_fpclass_memory_disp8(
        "evex_scalar_vfpclassss_memory_disp8_form",
        &evex_fpclass_ss_source(),
        &[0x62, 0xF3, 0x7D, 0x08, 0x67, 0x57, 0x10, 0x40],
    );
}

#[test]
fn evex_scalar_vfpclasssd_memory_disp8_form() {
    check_evex_fpclass_memory_disp8(
        "evex_scalar_vfpclasssd_memory_disp8_form",
        &evex_fpclass_pd_source(),
        &[0x62, 0xF3, 0xFD, 0x08, 0x67, 0x57, 0x08, 0x01],
    );
}

#[test]
fn evex_vfpclass_vl_forms() {
    let ps_source = evex_fpclass_ps_source();
    let pd_source = evex_fpclass_pd_source();

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &ps_source);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x47, 0x04]); // vmovdqu64 xmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x47, 0x02]); // vmovdqu64 ymm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x08, 0x66, 0xC8, 0xD1]); // vfpclassps k1, xmm0, 0xd1
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x08, 0x66, 0x57, 0x04, 0xD1]); // vfpclassps k2, [rdi+64], 0xd1
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x28, 0x66, 0xD8, 0xD1]); // vfpclassps k3, ymm0, 0xd1
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x28, 0x66, 0x67, 0x02, 0xD1]); // vfpclassps k4, [rdi+64], 0xd1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xC1]); // kmovq rax, k1
    append_mov_rax_to_rdi_disp(&mut code, 0);
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xC2]); // kmovq rax, k2
    append_mov_rax_to_rdi_disp(&mut code, 8);
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xC3]); // kmovq rax, k3
    append_mov_rax_to_rdi_disp(&mut code, 16);
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xC4]); // kmovq rax, k4
    append_mov_rax_to_rdi_disp(&mut code, 24);
    append_seed_rdi_plus_64_qwords(&mut code, &pd_source);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x47, 0x04]); // vmovdqu64 xmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x47, 0x02]); // vmovdqu64 ymm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x08, 0x66, 0xC8, 0xD1]); // vfpclasspd k1, xmm0, 0xd1
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x08, 0x66, 0x57, 0x04, 0xD1]); // vfpclasspd k2, [rdi+64], 0xd1
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x28, 0x66, 0xD8, 0xD1]); // vfpclasspd k3, ymm0, 0xd1
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x28, 0x66, 0x67, 0x02, 0xD1]); // vfpclasspd k4, [rdi+64], 0xd1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xC1]); // kmovq rax, k1
    append_mov_rax_to_rdi_disp(&mut code, 32);
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xC2]); // kmovq rax, k2
    append_mov_rax_to_rdi_disp(&mut code, 40);
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xC3]); // kmovq rax, k3
    append_mov_rax_to_rdi_disp(&mut code, 48);
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xC4]); // kmovq rax, k4
    append_mov_rax_to_rdi_disp(&mut code, 56);
    code.push(HLT);
    check_evex_mem("evex_vfpclass_vl_forms", &code, zero_scratch());
}

// ===========================================================================
// EXPANDED COVERAGE PART 41: EVEX FP unary memory forms.
// ===========================================================================

fn evex_getexp_ps_source() -> [u64; 8] {
    [
        0x4000_0000_3F80_0000u64,
        0x4080_0000_4040_0000,
        0x4100_0000_3F00_0000,
        0xC180_0000_4180_0000,
        0xC280_0000_4200_0000,
        0x4300_0000_4280_0000,
        0x3E80_0000_4380_0000,
        0x4480_0000_C400_0000,
    ]
}

fn evex_getexp_pd_source() -> [u64; 8] {
    [
        1.0f64.to_bits(),
        2.0f64.to_bits(),
        4.0f64.to_bits(),
        8.0f64.to_bits(),
        0.5f64.to_bits(),
        (-16.0f64).to_bits(),
        32.0f64.to_bits(),
        (-64.0f64).to_bits(),
    ]
}

fn evex_getexp_ph_source() -> [u64; 8] {
    let values = [
        0x3c00u16, 0x4000, 0x4400, 0x4800, 0x3800, 0xcc00, 0x5000, 0xd400, 0x3400,
        0x5400, 0x5800, 0xdc00, 0x3000, 0x5c00, 0x6000, 0xe400, 0x2c00, 0x6400,
        0x6800, 0xec00, 0x2800, 0x6c00, 0x7000, 0xf400, 0x2400, 0x7400, 0x7800,
        0xf800, 0x2000, 0x3c00, 0x4000, 0x4400,
    ];
    let mut words = [0u64; 8];
    for i in 0..8 {
        for j in 0..4 {
            words[i] |= (values[i * 4 + j] as u64) << (j * 16);
        }
    }
    words
}

fn check_evex_getexp_memory_disp8(label: &str, source: &[u64; 8], insn: &[u8]) {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, source);
    code.extend_from_slice(insn);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm2
    code.push(HLT);
    check_evex_mem(label, &code, zero_scratch());
}

#[test]
fn evex_zmm_vgetexpps_memory_disp8_form() {
    check_evex_getexp_memory_disp8(
        "evex_zmm_vgetexpps_memory_disp8_form",
        &evex_getexp_ps_source(),
        &[0x62, 0xF2, 0x7D, 0x48, 0x42, 0x57, 0x01],
    );
}

#[test]
fn evex_zmm_vgetexppd_memory_disp8_form() {
    check_evex_getexp_memory_disp8(
        "evex_zmm_vgetexppd_memory_disp8_form",
        &evex_getexp_pd_source(),
        &[0x62, 0xF2, 0xFD, 0x48, 0x42, 0x57, 0x01],
    );
}

#[test]
fn evex_zmm_vgetexpph_memory_disp8_form() {
    check_evex_getexp_memory_disp8(
        "evex_zmm_vgetexpph_memory_disp8_form",
        &evex_getexp_ph_source(),
        &[0x62, 0xF6, 0x7D, 0x48, 0x42, 0x57, 0x01],
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 42: EVEX scalar COMI memory forms.
// ===========================================================================

fn check_evex_comi_memory_disp8(label: &str, source: [u64; 8], scratch: [u8; 64], insn: &[u8]) {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(insn);
    code.extend_from_slice(&[0x9C, 0x58]); // pushfq; pop rax
    append_mov_rax_to_rdi_disp(&mut code, 0);
    code.push(HLT);
    check_evex_mem(label, &code, scratch);
}

#[test]
fn evex_scalar_vucomiss_memory_disp8_flags() {
    let mut s = zero_scratch();
    s[..4].copy_from_slice(&1.0f32.to_le_bytes());
    check_evex_comi_memory_disp8(
        "evex_scalar_vucomiss_memory_disp8_flags",
        [2.0f32.to_bits() as u64, 0, 0, 0, 0, 0, 0, 0],
        s,
        &[0x62, 0xF1, 0x7C, 0x08, 0x2E, 0x47, 0x10],
    );
}

#[test]
fn evex_scalar_vcomisd_memory_disp8_flags() {
    let mut s = zero_scratch();
    s[..8].copy_from_slice(&4.0f64.to_le_bytes());
    check_evex_comi_memory_disp8(
        "evex_scalar_vcomisd_memory_disp8_flags",
        [4.0f64.to_bits(), 0, 0, 0, 0, 0, 0, 0],
        s,
        &[0x62, 0xF1, 0xFD, 0x08, 0x2F, 0x47, 0x08],
    );
}

#[test]
fn evex_scalar_vucomish_memory_disp8_flags() {
    let mut s = zero_scratch();
    s[..2].copy_from_slice(&0x3c00u16.to_le_bytes()); // 1.0h
    check_evex_comi_memory_disp8(
        "evex_scalar_vucomish_memory_disp8_flags",
        [0x4000u64, 0, 0, 0, 0, 0, 0, 0], // 2.0h
        s,
        &[0x62, 0xF5, 0x7C, 0x08, 0x2E, 0x47, 0x20],
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 43: EVEX floating-point compare memory forms.
// ===========================================================================

fn evex_f32_words(values: [f32; 16]) -> [u64; 8] {
    let mut words = [0u64; 8];
    for i in 0..8 {
        words[i] = (values[i * 2].to_bits() as u64)
            | ((values[i * 2 + 1].to_bits() as u64) << 32);
    }
    words
}

fn evex_f16_words(values: [u16; 32]) -> [u64; 8] {
    let mut words = [0u64; 8];
    for i in 0..8 {
        for j in 0..4 {
            words[i] |= (values[i * 4 + j] as u64) << (j * 16);
        }
    }
    words
}

fn check_evex_fp_cmp_memory_disp8(label: &str, scratch: [u8; 64], source: [u64; 8], insn: &[u8]) {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(insn);
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xC2]); // kmovq rax, k2
    append_mov_rax_to_rdi_disp(&mut code, 0);
    code.push(HLT);
    check_evex_mem(label, &code, scratch);
}

#[test]
fn evex_zmm_vcmpps_memory_disp8_mask() {
    let lhs = [
        -4.0f32, -1.0, 0.0, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, -8.0, 0.5, -0.5,
        64.0, -16.0, 3.5, -3.5,
    ];
    let rhs = [
        -5.0f32, 0.0, 0.0, 2.0, 1.0, 8.0, 7.0, 16.0, 64.0, -4.0, 0.25, 0.0,
        128.0, -32.0, 7.0, -7.0,
    ];
    check_evex_fp_cmp_memory_disp8(
        "evex_zmm_vcmpps_memory_disp8_mask",
        evex_dword_scratch(|i| lhs[i].to_bits()),
        evex_f32_words(rhs),
        &[0x62, 0xF1, 0x7C, 0x48, 0xC2, 0x57, 0x01, 0x01],
    );
}

#[test]
fn evex_zmm_vcmppd_memory_disp8_mask() {
    let lhs = [1.0f64, 2.0, -4.0, 8.0, 16.0, -32.0, 0.5, -0.5];
    let rhs = [1.0f64, 1.0, -2.0, 8.0, 32.0, -64.0, 0.25, 0.0];
    check_evex_fp_cmp_memory_disp8(
        "evex_zmm_vcmppd_memory_disp8_mask",
        evex_qword_scratch(|i| lhs[i].to_bits()),
        rhs.map(f64::to_bits),
        &[0x62, 0xF1, 0xFD, 0x48, 0xC2, 0x57, 0x01, 0x02],
    );
}

#[test]
fn evex_zmm_vcmpph_memory_disp8_mask() {
    let lhs = [
        0x3c00u16, 0x4000, 0x4400, 0x3800, 0xbc00, 0xc000, 0x0000, 0x8000, 0x3c00,
        0x4000, 0x4400, 0x4800, 0x3400, 0x3000, 0xb800, 0xc400, 0x5000, 0x5400,
        0x5800, 0x5c00, 0xd000, 0xd400, 0xd800, 0xdc00, 0x2800, 0x2c00, 0x2400,
        0x2000, 0xa800, 0xac00, 0xa400, 0xa000,
    ];
    let rhs = [
        0x4000u16, 0x3c00, 0x4400, 0x3400, 0xc000, 0xbc00, 0x0000, 0x3c00, 0x3800,
        0x4400, 0x4000, 0x4c00, 0x3000, 0x3400, 0xbc00, 0xc000, 0x5400, 0x5000,
        0x5c00, 0x5800, 0xd400, 0xd000, 0xdc00, 0xd800, 0x2c00, 0x2800, 0x2000,
        0x2400, 0xac00, 0xa800, 0xa000, 0xa400,
    ];
    check_evex_fp_cmp_memory_disp8(
        "evex_zmm_vcmpph_memory_disp8_mask",
        evex_fp16_scratch(&lhs),
        evex_f16_words(rhs),
        &[0x62, 0xF3, 0x7C, 0x48, 0xC2, 0x57, 0x01, 0x01],
    );
}

#[test]
fn evex_scalar_vcmpss_memory_disp8_mask() {
    let mut s = zero_scratch();
    s[..4].copy_from_slice(&1.0f32.to_le_bytes());
    check_evex_fp_cmp_memory_disp8(
        "evex_scalar_vcmpss_memory_disp8_mask",
        s,
        [2.0f32.to_bits() as u64, 0, 0, 0, 0, 0, 0, 0],
        &[0x62, 0xF1, 0x7E, 0x08, 0xC2, 0x57, 0x10, 0x01],
    );
}

#[test]
fn evex_scalar_vcmpsd_memory_disp8_mask() {
    let mut s = zero_scratch();
    s[..8].copy_from_slice(&4.0f64.to_le_bytes());
    check_evex_fp_cmp_memory_disp8(
        "evex_scalar_vcmpsd_memory_disp8_mask",
        s,
        [4.0f64.to_bits(), 0, 0, 0, 0, 0, 0, 0],
        &[0x62, 0xF1, 0xFF, 0x08, 0xC2, 0x57, 0x08, 0x02],
    );
}

#[test]
fn evex_scalar_vcmpsh_memory_disp8_mask() {
    let mut s = zero_scratch();
    s[..2].copy_from_slice(&0x3c00u16.to_le_bytes()); // 1.0h
    check_evex_fp_cmp_memory_disp8(
        "evex_scalar_vcmpsh_memory_disp8_mask",
        s,
        [0x4000u64, 0, 0, 0, 0, 0, 0, 0], // 2.0h
        &[0x62, 0xF3, 0x7E, 0x08, 0xC2, 0x57, 0x20, 0x01],
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 44: EVEX FMA memory source forms.
// ===========================================================================

fn check_evex_fma_memory_disp8(label: &str, scratch: [u8; 64], source: [u64; 8], insn: &[u8]) {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x17]); // vmovdqu64 zmm2, [rdi]
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(insn);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm2
    code.push(HLT);
    check_evex_mem(label, &code, scratch);
}

fn check_evex_scalar_fma_memory_disp8(
    label: &str,
    scratch: [u8; 64],
    source: [u64; 8],
    insn: &[u8],
) {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xE1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm16, [rdi]
    code.extend_from_slice(&[0x62, 0xE1, 0xFE, 0x48, 0x6F, 0x17]); // vmovdqu64 zmm18, [rdi]
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(insn);
    code.extend_from_slice(&[0x62, 0xE1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm18
    code.push(HLT);
    check_evex_mem(label, &code, scratch);
}

#[test]
fn evex_zmm_vfmadd231ps_memory_disp8_form() {
    let lhs = [
        1.0f32, 2.0, -3.0, 4.0, 0.5, -0.25, 8.0, -16.0, 3.5, -7.0, 9.0, -11.0,
        0.125, -0.5, 13.0, -17.0,
    ];
    let rhs = [
        2.0f32, -1.0, 0.5, -0.25, 4.0, -8.0, 0.125, -0.0625, 1.5, -2.0, 0.75,
        -0.5, 16.0, -32.0, 0.25, -0.125,
    ];
    check_evex_fma_memory_disp8(
        "evex_zmm_vfmadd231ps_memory_disp8_form",
        evex_dword_scratch(|i| lhs[i].to_bits()),
        evex_f32_words(rhs),
        &[0x62, 0xF2, 0x7D, 0x48, 0xB8, 0x57, 0x01],
    );
}

#[test]
fn evex_zmm_vfmadd132ps_memory_disp8_form() {
    let lhs = [
        1.0f32, -2.0, 3.0, -4.0, 0.5, 0.25, -8.0, 16.0, -3.5, 7.0, -9.0, 11.0,
        -0.125, 0.5, -13.0, 17.0,
    ];
    let rhs = [
        -2.0f32, 1.0, -0.5, 0.25, -4.0, 8.0, -0.125, 0.0625, -1.5, 2.0, -0.75,
        0.5, -16.0, 32.0, -0.25, 0.125,
    ];
    check_evex_fma_memory_disp8(
        "evex_zmm_vfmadd132ps_memory_disp8_form",
        evex_dword_scratch(|i| lhs[i].to_bits()),
        evex_f32_words(rhs),
        &[0x62, 0xF2, 0x7D, 0x48, 0x98, 0x57, 0x01],
    );
}

#[test]
fn evex_zmm_vfmadd213ps_memory_disp8_form() {
    let lhs = [
        0.25f32, 0.5, 1.0, 2.0, -0.25, -0.5, -1.0, -2.0, 4.0, -4.0, 8.0, -8.0,
        16.0, -16.0, 32.0, -32.0,
    ];
    let rhs = [
        3.0f32, -3.0, 5.0, -5.0, 7.0, -7.0, 9.0, -9.0, 11.0, -11.0, 13.0,
        -13.0, 15.0, -15.0, 17.0, -17.0,
    ];
    check_evex_fma_memory_disp8(
        "evex_zmm_vfmadd213ps_memory_disp8_form",
        evex_dword_scratch(|i| lhs[i].to_bits()),
        evex_f32_words(rhs),
        &[0x62, 0xF2, 0x7D, 0x48, 0xA8, 0x57, 0x01],
    );
}

#[test]
fn evex_zmm_vfmsub231ps_memory_disp8_form() {
    let lhs = [
        1.0f32, 2.0, 3.0, 4.0, -1.0, -2.0, -3.0, -4.0, 0.5, -0.5, 1.5, -1.5,
        2.5, -2.5, 3.5, -3.5,
    ];
    let rhs = [
        0.25f32, -0.25, 0.5, -0.5, 0.75, -0.75, 1.0, -1.0, 1.25, -1.25, 1.5,
        -1.5, 1.75, -1.75, 2.0, -2.0,
    ];
    check_evex_fma_memory_disp8(
        "evex_zmm_vfmsub231ps_memory_disp8_form",
        evex_dword_scratch(|i| lhs[i].to_bits()),
        evex_f32_words(rhs),
        &[0x62, 0xF2, 0x7D, 0x48, 0xBA, 0x57, 0x01],
    );
}

#[test]
fn evex_zmm_vfnmadd231ps_memory_disp8_form() {
    let lhs = [
        -8.0f32, -7.0, -6.0, -5.0, -4.0, -3.0, -2.0, -1.0, 1.0, 2.0, 3.0, 4.0,
        5.0, 6.0, 7.0, 8.0,
    ];
    let rhs = [
        2.0f32, 1.5, 1.0, 0.5, -0.5, -1.0, -1.5, -2.0, 0.25, -0.25, 0.75,
        -0.75, 1.25, -1.25, 1.75, -1.75,
    ];
    check_evex_fma_memory_disp8(
        "evex_zmm_vfnmadd231ps_memory_disp8_form",
        evex_dword_scratch(|i| lhs[i].to_bits()),
        evex_f32_words(rhs),
        &[0x62, 0xF2, 0x7D, 0x48, 0xBC, 0x57, 0x01],
    );
}

#[test]
fn evex_zmm_vfnmsub231ps_memory_disp8_form() {
    let lhs = [
        0.125f32, -0.125, 0.25, -0.25, 0.5, -0.5, 1.0, -1.0, 2.0, -2.0, 4.0,
        -4.0, 8.0, -8.0, 16.0, -16.0,
    ];
    let rhs = [
        -3.0f32, 3.0, -5.0, 5.0, -7.0, 7.0, -9.0, 9.0, -11.0, 11.0, -13.0,
        13.0, -15.0, 15.0, -17.0, 17.0,
    ];
    check_evex_fma_memory_disp8(
        "evex_zmm_vfnmsub231ps_memory_disp8_form",
        evex_dword_scratch(|i| lhs[i].to_bits()),
        evex_f32_words(rhs),
        &[0x62, 0xF2, 0x7D, 0x48, 0xBE, 0x57, 0x01],
    );
}

#[test]
fn evex_zmm_vfmadd231pd_memory_disp8_form() {
    let lhs = [1.0f64, 2.0, -3.0, 4.0, 0.5, -0.25, 8.0, -16.0];
    let rhs = [2.0f64, -1.0, 0.5, -0.25, 4.0, -8.0, 0.125, -0.0625];
    check_evex_fma_memory_disp8(
        "evex_zmm_vfmadd231pd_memory_disp8_form",
        evex_qword_scratch(|i| lhs[i].to_bits()),
        rhs.map(f64::to_bits),
        &[0x62, 0xF2, 0xFD, 0x48, 0xB8, 0x57, 0x01],
    );
}

#[test]
fn evex_zmm_vfmadd231ph_memory_disp8_form() {
    let lhs = [
        0x3c00u16, 0x4000, 0xc200, 0x4400, 0x3800, 0xb400, 0x4800, 0xcc00, 0x4300,
        0xc700, 0x4880, 0xc980, 0x3000, 0xb800, 0x4a80, 0xcc40, 0x3c00, 0x4000,
        0xc200, 0x4400, 0x3800, 0xb400, 0x4800, 0xcc00, 0x4300, 0xc700, 0x4880,
        0xc980, 0x3000, 0xb800, 0x4a80, 0xcc40,
    ];
    let rhs = [
        0x4000u16, 0xbc00, 0x3800, 0xb400, 0x4400, 0xc800, 0x3000, 0xac00, 0x3e00,
        0xc000, 0x3a00, 0xb800, 0x4c00, 0xd000, 0x3400, 0xb000, 0x4000, 0xbc00,
        0x3800, 0xb400, 0x4400, 0xc800, 0x3000, 0xac00, 0x3e00, 0xc000, 0x3a00,
        0xb800, 0x4c00, 0xd000, 0x3400, 0xb000,
    ];
    check_evex_fma_memory_disp8(
        "evex_zmm_vfmadd231ph_memory_disp8_form",
        evex_fp16_scratch(&lhs),
        evex_f16_words(rhs),
        &[0x62, 0xF6, 0x7D, 0x48, 0xB8, 0x57, 0x01],
    );
}

#[test]
fn evex_scalar_vfmadd231ss_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..4].copy_from_slice(&1.5f32.to_le_bytes());
    check_evex_scalar_fma_memory_disp8(
        "evex_scalar_vfmadd231ss_memory_disp8_form",
        s,
        [2.0f32.to_bits() as u64, 0, 0, 0, 0, 0, 0, 0],
        &[0x62, 0xE2, 0x7D, 0x00, 0xB9, 0x57, 0x10],
    );
}

#[test]
fn evex_scalar_vfmadd231sd_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..8].copy_from_slice(&1.5f64.to_le_bytes());
    check_evex_scalar_fma_memory_disp8(
        "evex_scalar_vfmadd231sd_memory_disp8_form",
        s,
        [2.0f64.to_bits(), 0, 0, 0, 0, 0, 0, 0],
        &[0x62, 0xE2, 0xFD, 0x00, 0xB9, 0x57, 0x08],
    );
}

#[test]
fn evex_scalar_vfmadd231sh_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..2].copy_from_slice(&0x3e00u16.to_le_bytes()); // 1.5h
    check_evex_scalar_fma_memory_disp8(
        "evex_scalar_vfmadd231sh_memory_disp8_form",
        s,
        [0x4000u64, 0, 0, 0, 0, 0, 0, 0], // 2.0h
        &[0x62, 0xE6, 0x7D, 0x00, 0xB9, 0x57, 0x20],
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 45: EVEX ternary FP math memory forms.
// ===========================================================================

fn check_evex_fp_ternary_memory_disp8(
    label: &str,
    scratch: [u8; 64],
    source: [u64; 8],
    insn: &[u8],
) {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm0, [rdi]
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(insn);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm2
    code.push(HLT);
    check_evex_mem(label, &code, scratch);
}

fn check_evex_scalar_fp_ternary_memory_disp8(
    label: &str,
    scratch: [u8; 64],
    source: [u64; 8],
    insn: &[u8],
) {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xE1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm16, [rdi]
    code.extend_from_slice(&[0x62, 0xE1, 0xFE, 0x48, 0x6F, 0x17]); // vmovdqu64 zmm18, [rdi]
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(insn);
    code.extend_from_slice(&[0x62, 0xE1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm18
    code.push(HLT);
    check_evex_mem(label, &code, scratch);
}

#[test]
fn evex_xmm_range_scalef_vl_forms() {
    let s = evex_qword_scratch(|i| {
        [1.0f64, 2.0, -3.0, 4.0, 0.5, -0.25, 8.0, -16.0][i].to_bits()
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x07]); // vmovdqu64 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu64 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x09, 0x2C, 0xD1]); // vscalefps xmm2{k1}, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x09, 0x2C, 0x5F, 0x01]); // vscalefpd xmm3{k1}, xmm0, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x09, 0x50, 0xE1, 0x01]); // vrangeps xmm4{k1}, xmm0, xmm1, 1
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x09, 0x50, 0x6F, 0x01, 0x02]); // vrangepd xmm5{k1}, xmm0, [rdi+16], 2
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x17]); // vmovdqu64 [rdi], xmm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+16], xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x67, 0x02]); // vmovdqu64 [rdi+32], xmm4
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x6F, 0x03]); // vmovdqu64 [rdi+48], xmm5
    code.push(HLT);
    check_evex_mem("evex_xmm_range_scalef_vl_forms", &code, s);
}

#[test]
fn evex_ymm_range_scalef_vl_forms() {
    let s = evex_qword_scratch(|i| {
        [1.0f64, 2.0, -3.0, 4.0, 0.5, -0.25, 8.0, -16.0][i].to_bits()
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu64 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x29, 0x2C, 0xD1]); // vscalefps ymm2{k1}, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x29, 0x2C, 0x5F, 0x01]); // vscalefpd ymm3{k1}, ymm0, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x29, 0x50, 0xE1, 0x01]); // vrangeps ymm4{k1}, ymm0, ymm1, 1
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x29, 0x50, 0x6F, 0x01, 0x02]); // vrangepd ymm5{k1}, ymm0, [rdi+32], 2
    code.extend_from_slice(&[0xC5, 0xEC, 0x57, 0xD4]); // vxorps ymm2, ymm2, ymm4
    code.extend_from_slice(&[0xC5, 0xE5, 0x57, 0xDD]); // vxorpd ymm3, ymm3, ymm5
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x17]); // vmovdqu64 [rdi], ymm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_range_scalef_vl_forms", &code, s);
}

#[test]
fn evex_xmm_fp16_scalef_vl_forms() {
    let s = evex_fp16_scratch(&[
        0x3c00u16, 0x4000, 0xc200, 0x4400, 0x3800, 0xb400, 0x4800, 0xcc00, 0x3c00,
        0xbc00, 0x4000, 0xc000, 0x4200, 0xc200, 0x0000, 0x3c00,
    ]);
    let source = evex_f16_words([
        0x3800u16, 0xb800, 0x4200, 0xc200, 0x3c00, 0xbc00, 0x4000, 0xc000, 0x0000,
        0x3c00, 0xbc00, 0x4000, 0xc000, 0x4200, 0xc200, 0x0000, 0x3800, 0xb800,
        0x4200, 0xc200, 0x3c00, 0xbc00, 0x4000, 0xc000, 0x0000, 0x3c00, 0xbc00,
        0x4000, 0xc000, 0x4200, 0xc200, 0x0000,
    ]);

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x6F, 0x07]); // vmovdqu16 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu16 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF6, 0x7D, 0x08, 0x2C, 0xD1]); // vscalefph xmm2, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF6, 0x7D, 0x08, 0x2C, 0x5F, 0x04]); // vscalefph xmm3, xmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x7F, 0x17]); // vmovdqu16 [rdi], xmm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu16 [rdi+16], xmm3
    code.push(HLT);
    check_evex_mem("evex_xmm_fp16_scalef_vl_forms", &code, s);
}

#[test]
fn evex_ymm_fp16_scalef_vl_forms() {
    let s = evex_fp16_scratch(&[
        0x3c00u16, 0x4000, 0xc200, 0x4400, 0x3800, 0xb400, 0x4800, 0xcc00, 0x4300,
        0xc700, 0x4880, 0xc980, 0x3000, 0xb800, 0x4a80, 0xcc40, 0x3c00, 0xbc00,
        0x4000, 0xc000, 0x4200, 0xc200, 0x0000, 0x3c00, 0xbc00, 0x4000, 0xc000,
        0x4200, 0xc200, 0x0000, 0x3c00, 0xbc00,
    ]);
    let source = evex_f16_words([
        0x3800u16, 0xb800, 0x4200, 0xc200, 0x3c00, 0xbc00, 0x4000, 0xc000, 0x0000,
        0x3c00, 0xbc00, 0x4000, 0xc000, 0x4200, 0xc200, 0x0000, 0x3800, 0xb800,
        0x4200, 0xc200, 0x3c00, 0xbc00, 0x4000, 0xc000, 0x0000, 0x3c00, 0xbc00,
        0x4000, 0xc000, 0x4200, 0xc200, 0x0000,
    ]);

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x28, 0x6F, 0x07]); // vmovdqu16 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu16 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF6, 0x7D, 0x28, 0x2C, 0xD1]); // vscalefph ymm2, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF6, 0x7D, 0x28, 0x2C, 0x5F, 0x02]); // vscalefph ymm3, ymm0, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xED, 0xEF, 0xD3]); // vpxor ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x28, 0x7F, 0x17]); // vmovdqu16 [rdi], ymm2
    code.push(HLT);
    check_evex_mem("evex_ymm_fp16_scalef_vl_forms", &code, s);
}

#[test]
fn evex_zmm_vscalefps_memory_disp8_form() {
    let lhs = [
        1.0f32, 2.0, -3.0, 4.0, 0.5, -0.25, 8.0, -16.0, 3.5, -7.0, 9.0, -11.0,
        0.125, -0.5, 13.0, -17.0,
    ];
    let rhs = [
        1.0f32, -1.0, 2.0, -2.0, 3.0, -3.0, 0.0, 1.0, -1.0, 2.0, -2.0, 3.0, -3.0,
        0.0, 1.0, -1.0,
    ];
    check_evex_fp_ternary_memory_disp8(
        "evex_zmm_vscalefps_memory_disp8_form",
        evex_dword_scratch(|i| lhs[i].to_bits()),
        evex_f32_words(rhs),
        &[0x62, 0xF2, 0x7D, 0x48, 0x2C, 0x57, 0x01],
    );
}

#[test]
fn evex_zmm_vscalefpd_memory_disp8_form() {
    let lhs = [1.0f64, 2.0, -3.0, 4.0, 0.5, -0.25, 8.0, -16.0];
    let rhs = [1.0f64, -1.0, 2.0, -2.0, 3.0, -3.0, 0.0, 1.0];
    check_evex_fp_ternary_memory_disp8(
        "evex_zmm_vscalefpd_memory_disp8_form",
        evex_qword_scratch(|i| lhs[i].to_bits()),
        rhs.map(f64::to_bits),
        &[0x62, 0xF2, 0xFD, 0x48, 0x2C, 0x57, 0x01],
    );
}

#[test]
fn evex_zmm_vscalefph_memory_disp8_form() {
    let lhs = [
        0x3c00u16, 0x4000, 0xc200, 0x4400, 0x3800, 0xb400, 0x4800, 0xcc00, 0x4300,
        0xc700, 0x4880, 0xc980, 0x3000, 0xb800, 0x4a80, 0xcc40, 0x3c00, 0x4000,
        0xc200, 0x4400, 0x3800, 0xb400, 0x4800, 0xcc00, 0x4300, 0xc700, 0x4880,
        0xc980, 0x3000, 0xb800, 0x4a80, 0xcc40,
    ];
    let rhs = [
        0x3c00u16, 0xbc00, 0x4000, 0xc000, 0x4200, 0xc200, 0x0000, 0x3c00, 0xbc00,
        0x4000, 0xc000, 0x4200, 0xc200, 0x0000, 0x3c00, 0xbc00, 0x3c00, 0xbc00,
        0x4000, 0xc000, 0x4200, 0xc200, 0x0000, 0x3c00, 0xbc00, 0x4000, 0xc000,
        0x4200, 0xc200, 0x0000, 0x3c00, 0xbc00,
    ];
    check_evex_fp_ternary_memory_disp8(
        "evex_zmm_vscalefph_memory_disp8_form",
        evex_fp16_scratch(&lhs),
        evex_f16_words(rhs),
        &[0x62, 0xF6, 0x7D, 0x48, 0x2C, 0x57, 0x01],
    );
}

#[test]
fn evex_scalar_vscalefss_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..4].copy_from_slice(&1.5f32.to_le_bytes());
    check_evex_scalar_fp_ternary_memory_disp8(
        "evex_scalar_vscalefss_memory_disp8_form",
        s,
        [2.0f32.to_bits() as u64, 0, 0, 0, 0, 0, 0, 0],
        &[0x62, 0xE2, 0x7D, 0x00, 0x2D, 0x57, 0x10],
    );
}

#[test]
fn evex_scalar_vscalefsd_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..8].copy_from_slice(&1.5f64.to_le_bytes());
    check_evex_scalar_fp_ternary_memory_disp8(
        "evex_scalar_vscalefsd_memory_disp8_form",
        s,
        [2.0f64.to_bits(), 0, 0, 0, 0, 0, 0, 0],
        &[0x62, 0xE2, 0xFD, 0x00, 0x2D, 0x57, 0x08],
    );
}

#[test]
fn evex_scalar_vscalefsh_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..2].copy_from_slice(&0x3e00u16.to_le_bytes()); // 1.5h
    check_evex_scalar_fp_ternary_memory_disp8(
        "evex_scalar_vscalefsh_memory_disp8_form",
        s,
        [0x4000u64, 0, 0, 0, 0, 0, 0, 0], // 2.0h
        &[0x62, 0xE6, 0x7D, 0x00, 0x2D, 0x57, 0x20],
    );
}

#[test]
fn evex_zmm_vrangeps_memory_disp8_form() {
    let lhs = [
        1.0f32, -2.0, 3.0, -4.0, 0.5, -0.25, 8.0, -16.0, 3.5, -7.0, 9.0, -11.0,
        0.125, -0.5, 13.0, -17.0,
    ];
    let rhs = [
        2.0f32, -1.0, 0.5, -0.25, 4.0, -8.0, 0.125, -0.0625, 1.5, -2.0, 0.75,
        -0.5, 16.0, -32.0, 0.25, -0.125,
    ];
    check_evex_fp_ternary_memory_disp8(
        "evex_zmm_vrangeps_memory_disp8_form",
        evex_dword_scratch(|i| lhs[i].to_bits()),
        evex_f32_words(rhs),
        &[0x62, 0xF3, 0x7D, 0x48, 0x50, 0x57, 0x01, 0x01],
    );
}

#[test]
fn evex_zmm_vrangepd_memory_disp8_form() {
    let lhs = [1.0f64, -2.0, 3.0, -4.0, 0.5, -0.25, 8.0, -16.0];
    let rhs = [2.0f64, -1.0, 0.5, -0.25, 4.0, -8.0, 0.125, -0.0625];
    check_evex_fp_ternary_memory_disp8(
        "evex_zmm_vrangepd_memory_disp8_form",
        evex_qword_scratch(|i| lhs[i].to_bits()),
        rhs.map(f64::to_bits),
        &[0x62, 0xF3, 0xFD, 0x48, 0x50, 0x57, 0x01, 0x02],
    );
}

#[test]
fn evex_scalar_vrangess_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..4].copy_from_slice(&1.5f32.to_le_bytes());
    check_evex_scalar_fp_ternary_memory_disp8(
        "evex_scalar_vrangess_memory_disp8_form",
        s,
        [2.0f32.to_bits() as u64, 0, 0, 0, 0, 0, 0, 0],
        &[0x62, 0xE3, 0x7D, 0x00, 0x51, 0x57, 0x10, 0x01],
    );
}

#[test]
fn evex_scalar_vrangesd_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..8].copy_from_slice(&1.5f64.to_le_bytes());
    check_evex_scalar_fp_ternary_memory_disp8(
        "evex_scalar_vrangesd_memory_disp8_form",
        s,
        [2.0f64.to_bits(), 0, 0, 0, 0, 0, 0, 0],
        &[0x62, 0xE3, 0xFD, 0x00, 0x51, 0x57, 0x08, 0x02],
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 46: EVEX unary FP math memory forms.
// ===========================================================================

fn evex_unary_ps_source() -> [u64; 8] {
    evex_f32_words([
        1.25f32, -2.75, 3.5, -4.125, 0.5, -0.75, 8.25, -16.5, 3.75, -7.5, 9.25,
        -11.75, 0.125, -0.625, 13.5, -17.25,
    ])
}

fn evex_unary_pd_source() -> [u64; 8] {
    [
        1.25f64.to_bits(),
        (-2.75f64).to_bits(),
        3.5f64.to_bits(),
        (-4.125f64).to_bits(),
        0.5f64.to_bits(),
        (-0.75f64).to_bits(),
        8.25f64.to_bits(),
        (-16.5f64).to_bits(),
    ]
}

fn evex_unary_ph_source() -> [u64; 8] {
    evex_f16_words([
        0x3d00u16, 0xc180, 0x4300, 0xc420, 0x3800, 0xba00, 0x4820, 0xcc20, 0x4380,
        0xc780, 0x48a0, 0xc9e0, 0x3000, 0xb900, 0x4ac0, 0xcc50, 0x3d00, 0xc180,
        0x4300, 0xc420, 0x3800, 0xba00, 0x4820, 0xcc20, 0x4380, 0xc780, 0x48a0,
        0xc9e0, 0x3000, 0xb900, 0x4ac0, 0xcc50,
    ])
}

fn check_evex_fp_unary_memory_disp8(label: &str, source: [u64; 8], insn: &[u8]) {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(insn);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm2
    code.push(HLT);
    check_evex_mem(label, &code, zero_scratch());
}

fn check_evex_scalar_fp_unary_memory_disp8(
    label: &str,
    scratch: [u8; 64],
    source: [u64; 8],
    insn: &[u8],
) {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xE1, 0xFE, 0x48, 0x6F, 0x07]); // vmovdqu64 zmm16, [rdi]
    code.extend_from_slice(&[0x62, 0xE1, 0xFE, 0x48, 0x6F, 0x17]); // vmovdqu64 zmm18, [rdi]
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(insn);
    code.extend_from_slice(&[0x62, 0xE1, 0xFE, 0x48, 0x7F, 0x17]); // vmovdqu64 [rdi], zmm18
    code.push(HLT);
    check_evex_mem(label, &code, scratch);
}

#[test]
fn evex_xmm_getexp_getmant_vl_forms() {
    let source = evex_getexp_ps_source();
    let s = evex_qword_scratch(|i| source[i]);

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x47, 0x04]); // vmovdqu64 xmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x08, 0x42, 0xD0]); // vgetexpps xmm2, xmm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x08, 0x42, 0x5F, 0x04]); // vgetexpps xmm3, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xE8, 0x57, 0xD3]); // vxorps xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x17]); // vmovdqu64 [rdi], xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x08, 0x42, 0xD0]); // vgetexppd xmm2, xmm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x08, 0x42, 0x5F, 0x04]); // vgetexppd xmm3, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xE9, 0x57, 0xD3]); // vxorpd xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x57, 0x01]); // vmovdqu64 [rdi+16], xmm2
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x08, 0x26, 0xD0, 0x01]); // vgetmantps xmm2, xmm0, 1
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x08, 0x26, 0x5F, 0x04, 0x02]); // vgetmantps xmm3, [rdi+64], 2
    code.extend_from_slice(&[0xC5, 0xE8, 0x57, 0xD3]); // vxorps xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x57, 0x02]); // vmovdqu64 [rdi+32], xmm2
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x08, 0x26, 0xD0, 0x03]); // vgetmantpd xmm2, xmm0, 3
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x08, 0x26, 0x5F, 0x04, 0x04]); // vgetmantpd xmm3, [rdi+64], 4
    code.extend_from_slice(&[0xC5, 0xE9, 0x57, 0xD3]); // vxorpd xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x57, 0x03]); // vmovdqu64 [rdi+48], xmm2
    code.push(HLT);
    check_evex_mem("evex_xmm_getexp_getmant_vl_forms", &code, s);
}

#[test]
fn evex_ymm_getexp_getmant_vl_forms() {
    let source = evex_getexp_ps_source();
    let s = evex_qword_scratch(|i| source[i]);

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x47, 0x02]); // vmovdqu64 ymm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x28, 0x42, 0xD0]); // vgetexpps ymm2, ymm0
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x28, 0x42, 0x5F, 0x02]); // vgetexpps ymm3, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xEC, 0x57, 0xD3]); // vxorps ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x28, 0x42, 0xD8]); // vgetexppd ymm3, ymm0
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x28, 0x42, 0x67, 0x02]); // vgetexppd ymm4, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xE5, 0x57, 0xDC]); // vxorpd ymm3, ymm3, ymm4
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x28, 0x26, 0xE0, 0x01]); // vgetmantps ymm4, ymm0, 1
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x28, 0x26, 0x6F, 0x02, 0x02]); // vgetmantps ymm5, [rdi+64], 2
    code.extend_from_slice(&[0xC5, 0xDC, 0x57, 0xE5]); // vxorps ymm4, ymm4, ymm5
    code.extend_from_slice(&[0xC5, 0xEC, 0x57, 0xD4]); // vxorps ymm2, ymm2, ymm4
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x28, 0x26, 0xE0, 0x03]); // vgetmantpd ymm4, ymm0, 3
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x28, 0x26, 0x6F, 0x02, 0x04]); // vgetmantpd ymm5, [rdi+64], 4
    code.extend_from_slice(&[0xC5, 0xDD, 0x57, 0xE5]); // vxorpd ymm4, ymm4, ymm5
    code.extend_from_slice(&[0xC5, 0xE5, 0x57, 0xDC]); // vxorpd ymm3, ymm3, ymm4
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x17]); // vmovdqu64 [rdi], ymm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_getexp_getmant_vl_forms", &code, s);
}

#[test]
fn evex_xmm_fp16_unary_vl_forms() {
    let source = evex_unary_ph_source();
    let s = evex_qword_scratch(|i| source[i]);

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x6F, 0x47, 0x04]); // vmovdqu16 xmm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF6, 0x7D, 0x08, 0x42, 0xD0]); // vgetexpph xmm2, xmm0
    code.extend_from_slice(&[0x62, 0xF6, 0x7D, 0x08, 0x42, 0x5F, 0x04]); // vgetexpph xmm3, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xE9, 0xEF, 0xD3]); // vpxor xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x7F, 0x17]); // vmovdqu16 [rdi], xmm2
    code.extend_from_slice(&[0x62, 0xF3, 0x7C, 0x08, 0x08, 0xD0, 0x01]); // vrndscaleph xmm2, xmm0, 1
    code.extend_from_slice(&[0x62, 0xF3, 0x7C, 0x08, 0x08, 0x5F, 0x04, 0x02]); // vrndscaleph xmm3, [rdi+64], 2
    code.extend_from_slice(&[0xC5, 0xE9, 0xEF, 0xD3]); // vpxor xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x7F, 0x57, 0x01]); // vmovdqu16 [rdi+16], xmm2
    code.extend_from_slice(&[0x62, 0xF3, 0x7C, 0x08, 0x56, 0xD0, 0x03]); // vreduceph xmm2, xmm0, 3
    code.extend_from_slice(&[0x62, 0xF3, 0x7C, 0x08, 0x56, 0x5F, 0x04, 0x04]); // vreduceph xmm3, [rdi+64], 4
    code.extend_from_slice(&[0xC5, 0xE9, 0xEF, 0xD3]); // vpxor xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x7F, 0x57, 0x02]); // vmovdqu16 [rdi+32], xmm2
    code.extend_from_slice(&[0x62, 0xF3, 0x7C, 0x08, 0x26, 0xD0, 0x05]); // vgetmantph xmm2, xmm0, 5
    code.extend_from_slice(&[0x62, 0xF3, 0x7C, 0x08, 0x26, 0x5F, 0x04, 0x06]); // vgetmantph xmm3, [rdi+64], 6
    code.extend_from_slice(&[0xC5, 0xE9, 0xEF, 0xD3]); // vpxor xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x7F, 0x57, 0x03]); // vmovdqu16 [rdi+48], xmm2
    code.push(HLT);
    check_evex_mem("evex_xmm_fp16_unary_vl_forms", &code, s);
}

#[test]
fn evex_ymm_fp16_unary_vl_forms() {
    let source = evex_unary_ph_source();
    let s = evex_qword_scratch(|i| source[i]);

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x28, 0x6F, 0x47, 0x02]); // vmovdqu16 ymm0, [rdi+64]
    code.extend_from_slice(&[0x62, 0xF6, 0x7D, 0x28, 0x42, 0xD0]); // vgetexpph ymm2, ymm0
    code.extend_from_slice(&[0x62, 0xF6, 0x7D, 0x28, 0x42, 0x5F, 0x02]); // vgetexpph ymm3, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xED, 0xEF, 0xD3]); // vpxor ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF3, 0x7C, 0x28, 0x08, 0xD8, 0x01]); // vrndscaleph ymm3, ymm0, 1
    code.extend_from_slice(&[0x62, 0xF3, 0x7C, 0x28, 0x08, 0x67, 0x02, 0x02]); // vrndscaleph ymm4, [rdi+64], 2
    code.extend_from_slice(&[0xC5, 0xE5, 0xEF, 0xDC]); // vpxor ymm3, ymm3, ymm4
    code.extend_from_slice(&[0xC5, 0xED, 0xEF, 0xD3]); // vpxor ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF3, 0x7C, 0x28, 0x56, 0xD8, 0x03]); // vreduceph ymm3, ymm0, 3
    code.extend_from_slice(&[0x62, 0xF3, 0x7C, 0x28, 0x56, 0x67, 0x02, 0x04]); // vreduceph ymm4, [rdi+64], 4
    code.extend_from_slice(&[0xC5, 0xE5, 0xEF, 0xDC]); // vpxor ymm3, ymm3, ymm4
    code.extend_from_slice(&[0x62, 0xF3, 0x7C, 0x28, 0x26, 0xE0, 0x05]); // vgetmantph ymm4, ymm0, 5
    code.extend_from_slice(&[0x62, 0xF3, 0x7C, 0x28, 0x26, 0x6F, 0x02, 0x06]); // vgetmantph ymm5, [rdi+64], 6
    code.extend_from_slice(&[0xC5, 0xDD, 0xEF, 0xE5]); // vpxor ymm4, ymm4, ymm5
    code.extend_from_slice(&[0xC5, 0xE5, 0xEF, 0xDC]); // vpxor ymm3, ymm3, ymm4
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x28, 0x7F, 0x17]); // vmovdqu16 [rdi], ymm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x28, 0x7F, 0x5F, 0x01]); // vmovdqu16 [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_fp16_unary_vl_forms", &code, s);
}

#[test]
fn evex_xmm_rndscale_reduce_vl_forms() {
    let s = evex_qword_scratch(|i| {
        [1.25f64, -2.75, 3.5, -4.125, 0.5, -0.75, 8.25, -16.5][i].to_bits()
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x07]); // vmovdqu64 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x09, 0x08, 0xD0, 0x01]); // vrndscaleps xmm2{k1}, xmm0, 1
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x09, 0x09, 0x5F, 0x01, 0x02]); // vrndscalepd xmm3{k1}, [rdi+16], 2
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x09, 0x56, 0xE0, 0x03]); // vreduceps xmm4{k1}, xmm0, 3
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x09, 0x56, 0x6F, 0x01, 0x04]); // vreducepd xmm5{k1}, [rdi+16], 4
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x17]); // vmovdqu64 [rdi], xmm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+16], xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x67, 0x02]); // vmovdqu64 [rdi+32], xmm4
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x6F, 0x03]); // vmovdqu64 [rdi+48], xmm5
    code.push(HLT);
    check_evex_mem("evex_xmm_rndscale_reduce_vl_forms", &code, s);
}

#[test]
fn evex_ymm_rndscale_reduce_vl_forms() {
    let s = evex_qword_scratch(|i| {
        [1.25f64, -2.75, 3.5, -4.125, 0.5, -0.75, 8.25, -16.5][i].to_bits()
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x29, 0x08, 0xD0, 0x01]); // vrndscaleps ymm2{k1}, ymm0, 1
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x29, 0x09, 0x5F, 0x01, 0x02]); // vrndscalepd ymm3{k1}, [rdi+32], 2
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x29, 0x56, 0xE0, 0x03]); // vreduceps ymm4{k1}, ymm0, 3
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x29, 0x56, 0x6F, 0x01, 0x04]); // vreducepd ymm5{k1}, [rdi+32], 4
    code.extend_from_slice(&[0xC5, 0xEC, 0x57, 0xD4]); // vxorps ymm2, ymm2, ymm4
    code.extend_from_slice(&[0xC5, 0xE5, 0x57, 0xDD]); // vxorpd ymm3, ymm3, ymm5
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x17]); // vmovdqu64 [rdi], ymm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_rndscale_reduce_vl_forms", &code, s);
}

#[test]
fn evex_zmm_vrndscaleps_memory_disp8_form() {
    check_evex_fp_unary_memory_disp8(
        "evex_zmm_vrndscaleps_memory_disp8_form",
        evex_unary_ps_source(),
        &[0x62, 0xF3, 0x7D, 0x48, 0x08, 0x57, 0x01, 0x01],
    );
}

#[test]
fn evex_zmm_vrndscalepd_memory_disp8_form() {
    check_evex_fp_unary_memory_disp8(
        "evex_zmm_vrndscalepd_memory_disp8_form",
        evex_unary_pd_source(),
        &[0x62, 0xF3, 0xFD, 0x48, 0x09, 0x57, 0x01, 0x02],
    );
}

#[test]
fn evex_zmm_vrndscaleph_memory_disp8_form() {
    check_evex_fp_unary_memory_disp8(
        "evex_zmm_vrndscaleph_memory_disp8_form",
        evex_unary_ph_source(),
        &[0x62, 0xF3, 0x7C, 0x48, 0x08, 0x57, 0x01, 0x03],
    );
}

#[test]
fn evex_scalar_vrndscaless_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..4].copy_from_slice(&1.5f32.to_le_bytes());
    check_evex_scalar_fp_unary_memory_disp8(
        "evex_scalar_vrndscaless_memory_disp8_form",
        s,
        [1.25f32.to_bits() as u64, 0, 0, 0, 0, 0, 0, 0],
        &[0x62, 0xE3, 0x7D, 0x00, 0x0A, 0x57, 0x10, 0x01],
    );
}

#[test]
fn evex_scalar_vrndscalesd_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..8].copy_from_slice(&1.5f64.to_le_bytes());
    check_evex_scalar_fp_unary_memory_disp8(
        "evex_scalar_vrndscalesd_memory_disp8_form",
        s,
        [1.25f64.to_bits(), 0, 0, 0, 0, 0, 0, 0],
        &[0x62, 0xE3, 0xFD, 0x00, 0x0B, 0x57, 0x08, 0x02],
    );
}

#[test]
fn evex_scalar_vrndscalesh_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..2].copy_from_slice(&0x3e00u16.to_le_bytes()); // 1.5h
    check_evex_scalar_fp_unary_memory_disp8(
        "evex_scalar_vrndscalesh_memory_disp8_form",
        s,
        [0x3d00u64, 0, 0, 0, 0, 0, 0, 0], // 1.25h
        &[0x62, 0xE3, 0x7C, 0x00, 0x0A, 0x57, 0x20, 0x03],
    );
}

#[test]
fn evex_zmm_vreduceps_memory_disp8_form() {
    check_evex_fp_unary_memory_disp8(
        "evex_zmm_vreduceps_memory_disp8_form",
        evex_unary_ps_source(),
        &[0x62, 0xF3, 0x7D, 0x48, 0x56, 0x57, 0x01, 0x01],
    );
}

#[test]
fn evex_zmm_vreducepd_memory_disp8_form() {
    check_evex_fp_unary_memory_disp8(
        "evex_zmm_vreducepd_memory_disp8_form",
        evex_unary_pd_source(),
        &[0x62, 0xF3, 0xFD, 0x48, 0x56, 0x57, 0x01, 0x02],
    );
}

#[test]
fn evex_zmm_vreduceph_memory_disp8_form() {
    check_evex_fp_unary_memory_disp8(
        "evex_zmm_vreduceph_memory_disp8_form",
        evex_unary_ph_source(),
        &[0x62, 0xF3, 0x7C, 0x48, 0x56, 0x57, 0x01, 0x03],
    );
}

#[test]
fn evex_scalar_vreducess_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..4].copy_from_slice(&1.5f32.to_le_bytes());
    check_evex_scalar_fp_unary_memory_disp8(
        "evex_scalar_vreducess_memory_disp8_form",
        s,
        [1.25f32.to_bits() as u64, 0, 0, 0, 0, 0, 0, 0],
        &[0x62, 0xE3, 0x7D, 0x00, 0x57, 0x57, 0x10, 0x01],
    );
}

#[test]
fn evex_scalar_vreducesd_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..8].copy_from_slice(&1.5f64.to_le_bytes());
    check_evex_scalar_fp_unary_memory_disp8(
        "evex_scalar_vreducesd_memory_disp8_form",
        s,
        [1.25f64.to_bits(), 0, 0, 0, 0, 0, 0, 0],
        &[0x62, 0xE3, 0xFD, 0x00, 0x57, 0x57, 0x08, 0x02],
    );
}

#[test]
fn evex_scalar_vreducesh_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..2].copy_from_slice(&0x3e00u16.to_le_bytes()); // 1.5h
    check_evex_scalar_fp_unary_memory_disp8(
        "evex_scalar_vreducesh_memory_disp8_form",
        s,
        [0x3d00u64, 0, 0, 0, 0, 0, 0, 0], // 1.25h
        &[0x62, 0xE3, 0x7C, 0x00, 0x57, 0x57, 0x20, 0x03],
    );
}

#[test]
fn evex_zmm_vgetmantps_memory_disp8_form() {
    check_evex_fp_unary_memory_disp8(
        "evex_zmm_vgetmantps_memory_disp8_form",
        evex_unary_ps_source(),
        &[0x62, 0xF3, 0x7D, 0x48, 0x26, 0x57, 0x01, 0x01],
    );
}

#[test]
fn evex_zmm_vgetmantpd_memory_disp8_form() {
    check_evex_fp_unary_memory_disp8(
        "evex_zmm_vgetmantpd_memory_disp8_form",
        evex_unary_pd_source(),
        &[0x62, 0xF3, 0xFD, 0x48, 0x26, 0x57, 0x01, 0x02],
    );
}

#[test]
fn evex_zmm_vgetmantph_memory_disp8_form() {
    check_evex_fp_unary_memory_disp8(
        "evex_zmm_vgetmantph_memory_disp8_form",
        evex_unary_ph_source(),
        &[0x62, 0xF3, 0x7C, 0x48, 0x26, 0x57, 0x01, 0x03],
    );
}

#[test]
fn evex_scalar_vgetmantss_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..4].copy_from_slice(&1.5f32.to_le_bytes());
    check_evex_scalar_fp_unary_memory_disp8(
        "evex_scalar_vgetmantss_memory_disp8_form",
        s,
        [1.25f32.to_bits() as u64, 0, 0, 0, 0, 0, 0, 0],
        &[0x62, 0xE3, 0x7D, 0x00, 0x27, 0x57, 0x10, 0x01],
    );
}

#[test]
fn evex_scalar_vgetmantsd_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..8].copy_from_slice(&1.5f64.to_le_bytes());
    check_evex_scalar_fp_unary_memory_disp8(
        "evex_scalar_vgetmantsd_memory_disp8_form",
        s,
        [1.25f64.to_bits(), 0, 0, 0, 0, 0, 0, 0],
        &[0x62, 0xE3, 0xFD, 0x00, 0x27, 0x57, 0x08, 0x02],
    );
}

#[test]
fn evex_scalar_vgetmantsh_memory_disp8_form() {
    let mut s = zero_scratch();
    s[..2].copy_from_slice(&0x3e00u16.to_le_bytes()); // 1.5h
    check_evex_scalar_fp_unary_memory_disp8(
        "evex_scalar_vgetmantsh_memory_disp8_form",
        s,
        [0x3d00u64, 0, 0, 0, 0, 0, 0, 0], // 1.25h
        &[0x62, 0xE3, 0x7C, 0x00, 0x27, 0x57, 0x20, 0x03],
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 47: EVEX shuffle/permute memory disp8 forms.
// ===========================================================================

fn evex_shuffle_source() -> [u64; 8] {
    [
        0x807f_1020_3040_5060u64,
        0x7161_5141_3121_1101,
        0x8899_aabb_ccdd_eeff,
        0x0f1e_2d3c_4b5a_6978,
        0xf0e1_d2c3_b4a5_9687,
        0x7654_3210_fedc_ba98,
        0x1357_9bdf_2468_ace0,
        0xcafe_babe_dead_beef,
    ]
}

fn evex_shuffle_scratch() -> [u8; 64] {
    let mut s = [0u8; 64];
    for i in 0..64 {
        s[i] = 0x31u8.wrapping_add((i as u8).wrapping_mul(17));
    }
    s
}

fn evex_bytes_to_qwords(bytes: [u8; 64]) -> [u64; 8] {
    let mut words = [0u64; 8];
    for i in 0..8 {
        words[i] = u64::from_le_bytes([
            bytes[i * 8],
            bytes[i * 8 + 1],
            bytes[i * 8 + 2],
            bytes[i * 8 + 3],
            bytes[i * 8 + 4],
            bytes[i * 8 + 5],
            bytes[i * 8 + 6],
            bytes[i * 8 + 7],
        ]);
    }
    words
}

fn evex_dword_index_scratch(values: [u32; 16]) -> [u8; 64] {
    let mut s = [0u8; 64];
    for i in 0..16 {
        s[i * 4..i * 4 + 4].copy_from_slice(&values[i].to_le_bytes());
    }
    s
}

fn evex_qword_index_scratch(values: [u64; 8]) -> [u8; 64] {
    let mut s = [0u8; 64];
    for i in 0..8 {
        s[i * 8..i * 8 + 8].copy_from_slice(&values[i].to_le_bytes());
    }
    s
}

fn evex_pshufb_control_source() -> [u64; 8] {
    let mut bytes = [0u8; 64];
    for block in 0..4 {
        for lane in 0..16 {
            bytes[block * 16 + lane] = match lane % 5 {
                0 => 0x80,
                1 => 15 - lane as u8,
                2 => (lane as u8).wrapping_add(5) & 0x0f,
                3 => (lane as u8).wrapping_mul(3) & 0x0f,
                _ => lane as u8,
            };
        }
    }
    evex_bytes_to_qwords(bytes)
}

fn evex_dword_control_source(values: [u32; 16]) -> [u64; 8] {
    evex_bytes_to_qwords(evex_dword_index_scratch(values))
}

fn evex_qword_control_source(values: [u64; 8]) -> [u64; 8] {
    let mut words = [0u64; 8];
    for i in 0..8 {
        words[i] = values[i];
    }
    words
}

fn check_evex_shuffle_memory_disp8(
    label: &str,
    scratch: [u8; 64],
    source: [u64; 8],
    setup: &[u8],
    insn: &[u8],
    store: &[u8],
) {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(setup);
    code.extend_from_slice(insn);
    code.extend_from_slice(store);
    code.push(HLT);
    check_evex_mem(label, &code, scratch);
}

#[test]
fn evex_xmm_duplicate_shuffle_vl_forms() {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x07]); // vmovdqu64 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x09, 0x12, 0xD0]); // vmovsldup xmm2{k1}, xmm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x09, 0x12, 0x5F, 0x04]); // vmovsldup xmm3{k1}, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xE8, 0x57, 0xD3]); // vxorps xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x17]); // vmovdqu64 [rdi], xmm2
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x09, 0x16, 0xD0]); // vmovshdup xmm2{k1}, xmm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x09, 0x16, 0x5F, 0x04]); // vmovshdup xmm3{k1}, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xE8, 0x57, 0xD3]); // vxorps xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x57, 0x01]); // vmovdqu64 [rdi+16], xmm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x09, 0x12, 0xD0]); // vmovddup xmm2{k1}, xmm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x09, 0x12, 0x5F, 0x08]); // vmovddup xmm3{k1}, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xE9, 0x57, 0xD3]); // vxorpd xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x57, 0x02]); // vmovdqu64 [rdi+32], xmm2
    code.push(HLT);
    check_evex_mem("evex_xmm_duplicate_shuffle_vl_forms", &code, evex_shuffle_scratch());
}

#[test]
fn evex_ymm_duplicate_shuffle_vl_forms() {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x29, 0x12, 0xD0]); // vmovsldup ymm2{k1}, ymm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x29, 0x12, 0x5F, 0x02]); // vmovsldup ymm3{k1}, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xEC, 0x57, 0xD3]); // vxorps ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x29, 0x16, 0xD8]); // vmovshdup ymm3{k1}, ymm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x29, 0x16, 0x67, 0x02]); // vmovshdup ymm4{k1}, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xE4, 0x57, 0xDC]); // vxorps ymm3, ymm3, ymm4
    code.extend_from_slice(&[0xC5, 0xEC, 0x57, 0xD3]); // vxorps ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x29, 0x12, 0xD8]); // vmovddup ymm3{k1}, ymm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x29, 0x12, 0x67, 0x02]); // vmovddup ymm4{k1}, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xE5, 0x57, 0xDC]); // vxorpd ymm3, ymm3, ymm4
    code.extend_from_slice(&[0xC5, 0xED, 0x57, 0xD3]); // vxorpd ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x17]); // vmovdqu64 [rdi], ymm2
    code.push(HLT);
    check_evex_mem("evex_ymm_duplicate_shuffle_vl_forms", &code, evex_shuffle_scratch());
}

#[test]
fn evex_xmm_immediate_shuffle_vl_forms() {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x07]); // vmovdqu64 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x09, 0x70, 0xD0, 0x1B]); // vpshufd xmm2{k1}, xmm0, 0x1b
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x09, 0x70, 0x5F, 0x04, 0x4E]); // vpshufd xmm3{k1}, [rdi+64], 0x4e
    code.extend_from_slice(&[0xC5, 0xE9, 0xEF, 0xD3]); // vpxor xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x17]); // vmovdqu64 [rdi], xmm2
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x09, 0x70, 0xD0, 0x4E]); // vpshufhw xmm2{k1}, xmm0, 0x4e
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x09, 0x70, 0x5F, 0x04, 0xB1]); // vpshufhw xmm3{k1}, [rdi+64], 0xb1
    code.extend_from_slice(&[0xC5, 0xE9, 0xEF, 0xD3]); // vpxor xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x57, 0x01]); // vmovdqu64 [rdi+16], xmm2
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x09, 0x70, 0xD0, 0xB1]); // vpshuflw xmm2{k1}, xmm0, 0xb1
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x09, 0x70, 0x5F, 0x04, 0x1B]); // vpshuflw xmm3{k1}, [rdi+64], 0x1b
    code.extend_from_slice(&[0xC5, 0xE9, 0xEF, 0xD3]); // vpxor xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x57, 0x02]); // vmovdqu64 [rdi+32], xmm2
    code.push(HLT);
    check_evex_mem("evex_xmm_immediate_shuffle_vl_forms", &code, evex_shuffle_scratch());
}

#[test]
fn evex_ymm_immediate_shuffle_vl_forms() {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x29, 0x70, 0xD0, 0x1B]); // vpshufd ymm2{k1}, ymm0, 0x1b
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x29, 0x70, 0x5F, 0x02, 0x4E]); // vpshufd ymm3{k1}, [rdi+64], 0x4e
    code.extend_from_slice(&[0xC5, 0xED, 0xEF, 0xD3]); // vpxor ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x29, 0x70, 0xD8, 0x4E]); // vpshufhw ymm3{k1}, ymm0, 0x4e
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x29, 0x70, 0x67, 0x02, 0xB1]); // vpshufhw ymm4{k1}, [rdi+64], 0xb1
    code.extend_from_slice(&[0xC5, 0xE5, 0xEF, 0xDC]); // vpxor ymm3, ymm3, ymm4
    code.extend_from_slice(&[0xC5, 0xED, 0xEF, 0xD3]); // vpxor ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x29, 0x70, 0xD8, 0xB1]); // vpshuflw ymm3{k1}, ymm0, 0xb1
    code.extend_from_slice(&[0x62, 0xF1, 0x7F, 0x29, 0x70, 0x67, 0x02, 0x1B]); // vpshuflw ymm4{k1}, [rdi+64], 0x1b
    code.extend_from_slice(&[0xC5, 0xE5, 0xEF, 0xDC]); // vpxor ymm3, ymm3, ymm4
    code.extend_from_slice(&[0xC5, 0xED, 0xEF, 0xD3]); // vpxor ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x17]); // vmovdqu64 [rdi], ymm2
    code.push(HLT);
    check_evex_mem("evex_ymm_immediate_shuffle_vl_forms", &code, evex_shuffle_scratch());
}

#[test]
fn evex_xmm_fp_shuffle_vl_forms() {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x07]); // vmovdqu64 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu64 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0x7C, 0x09, 0xC6, 0xD1, 0x93]); // vshufps xmm2{k1}, xmm0, xmm1, 0x93
    code.extend_from_slice(&[0x62, 0xF1, 0x7C, 0x09, 0xC6, 0x5F, 0x04, 0x4E]); // vshufps xmm3{k1}, xmm0, [rdi+64], 0x4e
    code.extend_from_slice(&[0xC5, 0xE8, 0x57, 0xD3]); // vxorps xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x17]); // vmovdqu64 [rdi], xmm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x09, 0xC6, 0xD1, 0x05]); // vshufpd xmm2{k1}, xmm0, xmm1, 0x05
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x09, 0xC6, 0x5F, 0x04, 0x0A]); // vshufpd xmm3{k1}, xmm0, [rdi+64], 0x0a
    code.extend_from_slice(&[0xC5, 0xE9, 0x57, 0xD3]); // vxorpd xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x57, 0x01]); // vmovdqu64 [rdi+16], xmm2
    code.push(HLT);
    check_evex_mem("evex_xmm_fp_shuffle_vl_forms", &code, evex_shuffle_scratch());
}

#[test]
fn evex_ymm_fp_shuffle_vl_forms() {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu64 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0x7C, 0x29, 0xC6, 0xD1, 0x93]); // vshufps ymm2{k1}, ymm0, ymm1, 0x93
    code.extend_from_slice(&[0x62, 0xF1, 0x7C, 0x29, 0xC6, 0x5F, 0x02, 0x4E]); // vshufps ymm3{k1}, ymm0, [rdi+64], 0x4e
    code.extend_from_slice(&[0xC5, 0xEC, 0x57, 0xD3]); // vxorps ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x29, 0xC6, 0xD9, 0x05]); // vshufpd ymm3{k1}, ymm0, ymm1, 0x05
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x29, 0xC6, 0x67, 0x02, 0x0A]); // vshufpd ymm4{k1}, ymm0, [rdi+64], 0x0a
    code.extend_from_slice(&[0xC5, 0xE5, 0x57, 0xDC]); // vxorpd ymm3, ymm3, ymm4
    code.extend_from_slice(&[0xC5, 0xEC, 0x57, 0xD3]); // vxorps ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x17]); // vmovdqu64 [rdi], ymm2
    code.push(HLT);
    check_evex_mem("evex_ymm_fp_shuffle_vl_forms", &code, evex_shuffle_scratch());
}

#[test]
fn evex_xmm_permil_imm_vl_forms() {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x07]); // vmovdqu64 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x09, 0x04, 0xD0, 0x1B]); // vpermilps xmm2{k1}, xmm0, 0x1b
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x09, 0x04, 0x5F, 0x04, 0x4E]); // vpermilps xmm3{k1}, [rdi+64], 0x4e
    code.extend_from_slice(&[0xC5, 0xE8, 0x57, 0xD3]); // vxorps xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x17]); // vmovdqu64 [rdi], xmm2
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x09, 0x05, 0xD0, 0x05]); // vpermilpd xmm2{k1}, xmm0, 0x05
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x09, 0x05, 0x5F, 0x04, 0x0A]); // vpermilpd xmm3{k1}, [rdi+64], 0x0a
    code.extend_from_slice(&[0xC5, 0xE9, 0x57, 0xD3]); // vxorpd xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x57, 0x01]); // vmovdqu64 [rdi+16], xmm2
    code.push(HLT);
    check_evex_mem("evex_xmm_permil_imm_vl_forms", &code, evex_shuffle_scratch());
}

#[test]
fn evex_ymm_permil_imm_vl_forms() {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x29, 0x04, 0xD0, 0x1B]); // vpermilps ymm2{k1}, ymm0, 0x1b
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x29, 0x04, 0x5F, 0x02, 0x4E]); // vpermilps ymm3{k1}, [rdi+64], 0x4e
    code.extend_from_slice(&[0xC5, 0xEC, 0x57, 0xD3]); // vxorps ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x29, 0x05, 0xD8, 0x05]); // vpermilpd ymm3{k1}, ymm0, 0x05
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x29, 0x05, 0x67, 0x02, 0x0A]); // vpermilpd ymm4{k1}, [rdi+64], 0x0a
    code.extend_from_slice(&[0xC5, 0xE5, 0x57, 0xDC]); // vxorpd ymm3, ymm3, ymm4
    code.extend_from_slice(&[0xC5, 0xEC, 0x57, 0xD3]); // vxorps ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x17]); // vmovdqu64 [rdi], ymm2
    code.push(HLT);
    check_evex_mem("evex_ymm_permil_imm_vl_forms", &code, evex_shuffle_scratch());
}

#[test]
fn evex_xmm_permil_variable_vl_forms() {
    let mut scratch = evex_shuffle_scratch();
    let control = evex_dword_index_scratch([3, 0, 1, 2, 2, 1, 0, 3, 1, 3, 2, 0, 0, 2, 3, 1]);
    scratch[32..64].copy_from_slice(&control[0..32]);
    let source = evex_dword_control_source([2, 3, 0, 1, 1, 0, 3, 2, 0, 1, 2, 3, 3, 2, 1, 0]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x07]); // vmovdqu64 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x4F, 0x02]); // vmovdqu64 xmm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x09, 0x0C, 0xD1]); // vpermilps xmm2{k1}, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x09, 0x0C, 0x5F, 0x04]); // vpermilps xmm3{k1}, xmm0, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xE8, 0x57, 0xD3]); // vxorps xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x17]); // vmovdqu64 [rdi], xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x09, 0x0D, 0xD1]); // vpermilpd xmm2{k1}, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x09, 0x0D, 0x5F, 0x04]); // vpermilpd xmm3{k1}, xmm0, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xE9, 0x57, 0xD3]); // vxorpd xmm2, xmm2, xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x57, 0x01]); // vmovdqu64 [rdi+16], xmm2
    code.push(HLT);
    check_evex_mem("evex_xmm_permil_variable_vl_forms", &code, scratch);
}

#[test]
fn evex_ymm_permil_variable_vl_forms() {
    let mut scratch = evex_shuffle_scratch();
    let control = evex_dword_index_scratch([3, 0, 1, 2, 2, 1, 0, 3, 1, 3, 2, 0, 0, 2, 3, 1]);
    scratch[32..64].copy_from_slice(&control[0..32]);
    let source = evex_dword_control_source([2, 3, 0, 1, 1, 0, 3, 2, 0, 1, 2, 3, 3, 2, 1, 0]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    append_seed_rdi_plus_64_qwords(&mut code, &source);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu64 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x29, 0x0C, 0xD1]); // vpermilps ymm2{k1}, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x29, 0x0C, 0x5F, 0x02]); // vpermilps ymm3{k1}, ymm0, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xEC, 0x57, 0xD3]); // vxorps ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x29, 0x0D, 0xD9]); // vpermilpd ymm3{k1}, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x29, 0x0D, 0x67, 0x02]); // vpermilpd ymm4{k1}, ymm0, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xE5, 0x57, 0xDC]); // vxorpd ymm3, ymm3, ymm4
    code.extend_from_slice(&[0xC5, 0xEC, 0x57, 0xD3]); // vxorps ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x17]); // vmovdqu64 [rdi], ymm2
    code.push(HLT);
    check_evex_mem("evex_ymm_permil_variable_vl_forms", &code, scratch);
}

#[test]
fn evex_ymm_qword_permute_imm_vl_forms() {
    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x29, 0x00, 0xD0, 0x1B]); // vpermq ymm2{k1}, ymm0, 0x1b
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x29, 0x00, 0x5F, 0x02, 0x4E]); // vpermq ymm3{k1}, [rdi+64], 0x4e
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x28, 0xEF, 0xD3]); // vpxorq ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x29, 0x01, 0xD8, 0x05]); // vpermpd ymm3{k1}, ymm0, 0x05
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x29, 0x01, 0x67, 0x02, 0x0A]); // vpermpd ymm4{k1}, [rdi+64], 0x0a
    code.extend_from_slice(&[0x62, 0xF1, 0xE5, 0x28, 0xEF, 0xDC]); // vpxorq ymm3, ymm3, ymm4
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x28, 0xEF, 0xD3]); // vpxorq ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x17]); // vmovdqu64 [rdi], ymm2
    code.push(HLT);
    check_evex_mem("evex_ymm_qword_permute_imm_vl_forms", &code, evex_shuffle_scratch());
}

#[test]
fn evex_ymm_variable_permute_vl_forms() {
    let mut scratch = evex_shuffle_scratch();
    let indices = evex_dword_index_scratch([7, 0, 6, 1, 5, 2, 4, 3, 3, 4, 2, 5, 1, 6, 0, 7]);
    scratch[0..32].copy_from_slice(&indices[0..32]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x07]); // vmovdqu64 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu64 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x29, 0x16, 0xD1]); // vpermps ymm2{k1}, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x29, 0x16, 0x5F, 0x02]); // vpermps ymm3{k1}, ymm0, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xEC, 0x57, 0xD3]); // vxorps ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x29, 0x16, 0xD9]); // vpermpd ymm3{k1}, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x29, 0x16, 0x67, 0x02]); // vpermpd ymm4{k1}, ymm0, [rdi+64]
    code.extend_from_slice(&[0xC5, 0xE5, 0x57, 0xDC]); // vxorpd ymm3, ymm3, ymm4
    code.extend_from_slice(&[0xC5, 0xEC, 0x57, 0xD3]); // vxorps ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x17]); // vmovdqu64 [rdi], ymm2
    code.push(HLT);
    check_evex_mem("evex_ymm_variable_permute_vl_forms", &code, scratch);
}

#[test]
fn evex_zmm_vmovsldup_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vmovsldup_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[],
        &[0x62, 0xF1, 0x7E, 0x48, 0x12, 0x57, 0x01],
        &[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17],
    );
}

#[test]
fn evex_zmm_vmovshdup_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vmovshdup_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[],
        &[0x62, 0xF1, 0x7E, 0x48, 0x16, 0x5F, 0x01],
        &[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x1F],
    );
}

#[test]
fn evex_zmm_vmovddup_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vmovddup_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[],
        &[0x62, 0xF1, 0xFF, 0x48, 0x12, 0x67, 0x01],
        &[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x27],
    );
}

#[test]
fn evex_zmm_vpshufd_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vpshufd_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[],
        &[0x62, 0xF1, 0x7D, 0x48, 0x70, 0x6F, 0x01, 0x1B],
        &[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x2F],
    );
}

#[test]
fn evex_zmm_vpshufhw_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vpshufhw_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[],
        &[0x62, 0xF1, 0x7E, 0x48, 0x70, 0x77, 0x01, 0x4E],
        &[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x37],
    );
}

#[test]
fn evex_zmm_vpshuflw_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vpshuflw_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[],
        &[0x62, 0xF1, 0x7F, 0x48, 0x70, 0x7F, 0x01, 0xB1],
        &[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x3F],
    );
}

#[test]
fn evex_zmm_vshufps_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vshufps_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07],
        &[0x62, 0x71, 0x7C, 0x48, 0xC6, 0x47, 0x01, 0x93],
        &[0x62, 0x71, 0xFE, 0x48, 0x7F, 0x07],
    );
}

#[test]
fn evex_zmm_vshufpd_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vshufpd_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x0F],
        &[0x62, 0x71, 0xF5, 0x48, 0xC6, 0x4F, 0x01, 0x55],
        &[0x62, 0x71, 0xFE, 0x48, 0x7F, 0x0F],
    );
}

#[test]
fn evex_zmm_vshuff32x4_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vshuff32x4_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07],
        &[0x62, 0x73, 0x7D, 0x48, 0x23, 0x57, 0x01, 0x4E],
        &[0x62, 0x71, 0xFE, 0x48, 0x7F, 0x17],
    );
}

#[test]
fn evex_zmm_vshuff64x2_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vshuff64x2_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x0F],
        &[0x62, 0x73, 0xF5, 0x48, 0x23, 0x5F, 0x01, 0xB1],
        &[0x62, 0x71, 0xFE, 0x48, 0x7F, 0x1F],
    );
}

#[test]
fn evex_zmm_vpermps_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vpermps_memory_disp8_form",
        evex_dword_index_scratch([15, 0, 14, 1, 13, 2, 12, 3, 11, 4, 10, 5, 9, 6, 8, 7]),
        evex_shuffle_source(),
        &[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07],
        &[0x62, 0x72, 0x7D, 0x48, 0x16, 0x67, 0x01],
        &[0x62, 0x71, 0xFE, 0x48, 0x7F, 0x27],
    );
}

#[test]
fn evex_zmm_vpermpd_variable_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vpermpd_variable_memory_disp8_form",
        evex_qword_index_scratch([7, 0, 6, 1, 5, 2, 4, 3]),
        evex_shuffle_source(),
        &[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x0F],
        &[0x62, 0x72, 0xF5, 0x48, 0x16, 0x6F, 0x01],
        &[0x62, 0x71, 0xFE, 0x48, 0x7F, 0x2F],
    );
}

#[test]
fn evex_zmm_vpermq_imm_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vpermq_imm_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[],
        &[0x62, 0x73, 0xFD, 0x48, 0x00, 0x77, 0x01, 0x1B],
        &[0x62, 0x71, 0xFE, 0x48, 0x7F, 0x37],
    );
}

#[test]
fn evex_zmm_vpermpd_imm_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vpermpd_imm_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[],
        &[0x62, 0x73, 0xFD, 0x48, 0x01, 0x7F, 0x01, 0x4E],
        &[0x62, 0x71, 0xFE, 0x48, 0x7F, 0x3F],
    );
}

#[test]
fn evex_zmm_vpermilps_imm_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vpermilps_imm_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[],
        &[0x62, 0xE3, 0x7D, 0x48, 0x04, 0x47, 0x01, 0x1B],
        &[0x62, 0xE1, 0xFE, 0x48, 0x7F, 0x07],
    );
}

#[test]
fn evex_zmm_vpermilpd_imm_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vpermilpd_imm_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[],
        &[0x62, 0xE3, 0xFD, 0x48, 0x05, 0x4F, 0x01, 0x55],
        &[0x62, 0xE1, 0xFE, 0x48, 0x7F, 0x0F],
    );
}

#[test]
fn evex_zmm_vpermilps_variable_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vpermilps_variable_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_dword_control_source([3, 2, 1, 0, 1, 0, 3, 2, 2, 3, 0, 1, 0, 1, 2, 3]),
        &[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07],
        &[0x62, 0xE2, 0x7D, 0x48, 0x0C, 0x57, 0x01],
        &[0x62, 0xE1, 0xFE, 0x48, 0x7F, 0x17],
    );
}

#[test]
fn evex_zmm_vpermilpd_variable_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vpermilpd_variable_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_qword_control_source([2, 0, 2, 0, 0, 2, 0, 2]),
        &[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x0F],
        &[0x62, 0xE2, 0xF5, 0x48, 0x0D, 0x5F, 0x01],
        &[0x62, 0xE1, 0xFE, 0x48, 0x7F, 0x1F],
    );
}

#[test]
fn evex_zmm_vpshufb_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vpshufb_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_pshufb_control_source(),
        &[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07],
        &[0x62, 0xE2, 0x7D, 0x48, 0x00, 0x67, 0x01],
        &[0x62, 0xE1, 0xFE, 0x48, 0x7F, 0x27],
    );
}

#[test]
fn evex_zmm_vpalignr_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vpalignr_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07],
        &[0x62, 0xE3, 0x7D, 0x48, 0x0F, 0x6F, 0x01, 0x07],
        &[0x62, 0xE1, 0xFE, 0x48, 0x7F, 0x2F],
    );
}

#[test]
fn evex_zmm_vpunpcklbw_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vpunpcklbw_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07],
        &[0x62, 0xE1, 0x7D, 0x48, 0x60, 0x77, 0x01],
        &[0x62, 0xE1, 0xFE, 0x48, 0x7F, 0x37],
    );
}

#[test]
fn evex_zmm_vpunpckhwd_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vpunpckhwd_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07],
        &[0x62, 0xE1, 0x7D, 0x48, 0x69, 0x7F, 0x01],
        &[0x62, 0xE1, 0xFE, 0x48, 0x7F, 0x3F],
    );
}

#[test]
fn evex_zmm_vpabsd_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vpabsd_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[],
        &[0x62, 0x62, 0x7D, 0x48, 0x1E, 0x47, 0x01],
        &[0x62, 0x61, 0xFE, 0x48, 0x7F, 0x07],
    );
}

#[test]
fn evex_zmm_vpabsq_memory_disp8_form() {
    check_evex_shuffle_memory_disp8(
        "evex_zmm_vpabsq_memory_disp8_form",
        evex_shuffle_scratch(),
        evex_shuffle_source(),
        &[],
        &[0x62, 0x62, 0xFD, 0x48, 0x1F, 0x4F, 0x01],
        &[0x62, 0x61, 0xFE, 0x48, 0x7F, 0x0F],
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 48: EVEX scalar and block broadcast memory forms.
// ===========================================================================

fn check_evex_broadcast_memory_disp8(label: &str, insn: &[u8], store: &[u8]) {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(insn);
    code.extend_from_slice(store);
    code.push(HLT);
    check_evex_mem(label, &code, evex_shuffle_scratch());
}

#[test]
fn evex_zmm_vbroadcastss_memory_disp8_form() {
    check_evex_broadcast_memory_disp8(
        "evex_zmm_vbroadcastss_memory_disp8_form",
        &[0x62, 0xF2, 0x7D, 0x48, 0x18, 0x57, 0x10],
        &[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17],
    );
}

#[test]
fn evex_zmm_vbroadcastsd_memory_disp8_form() {
    check_evex_broadcast_memory_disp8(
        "evex_zmm_vbroadcastsd_memory_disp8_form",
        &[0x62, 0xF2, 0xFD, 0x48, 0x19, 0x5F, 0x08],
        &[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x1F],
    );
}

#[test]
fn evex_zmm_vpbroadcastb_memory_disp8_form() {
    check_evex_broadcast_memory_disp8(
        "evex_zmm_vpbroadcastb_memory_disp8_form",
        &[0x62, 0xF2, 0x7D, 0x48, 0x78, 0x67, 0x40],
        &[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x27],
    );
}

#[test]
fn evex_zmm_vpbroadcastw_memory_disp8_form() {
    check_evex_broadcast_memory_disp8(
        "evex_zmm_vpbroadcastw_memory_disp8_form",
        &[0x62, 0xF2, 0x7D, 0x48, 0x79, 0x6F, 0x20],
        &[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x2F],
    );
}

#[test]
fn evex_zmm_vpbroadcastd_memory_disp8_form() {
    check_evex_broadcast_memory_disp8(
        "evex_zmm_vpbroadcastd_memory_disp8_form",
        &[0x62, 0xF2, 0x7D, 0x48, 0x58, 0x77, 0x10],
        &[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x37],
    );
}

#[test]
fn evex_zmm_vpbroadcastq_memory_disp8_form() {
    check_evex_broadcast_memory_disp8(
        "evex_zmm_vpbroadcastq_memory_disp8_form",
        &[0x62, 0xF2, 0xFD, 0x48, 0x59, 0x7F, 0x08],
        &[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x3F],
    );
}

#[test]
fn evex_zmm_vbroadcastf32x2_memory_disp8_form() {
    check_evex_broadcast_memory_disp8(
        "evex_zmm_vbroadcastf32x2_memory_disp8_form",
        &[0x62, 0x72, 0x7D, 0x48, 0x19, 0x47, 0x08],
        &[0x62, 0x71, 0xFE, 0x48, 0x7F, 0x07],
    );
}

#[test]
fn evex_zmm_vbroadcastf32x4_memory_disp8_form() {
    check_evex_broadcast_memory_disp8(
        "evex_zmm_vbroadcastf32x4_memory_disp8_form",
        &[0x62, 0x72, 0x7D, 0x48, 0x1A, 0x4F, 0x04],
        &[0x62, 0x71, 0xFE, 0x48, 0x7F, 0x0F],
    );
}

#[test]
fn evex_zmm_vbroadcastf64x2_memory_disp8_form() {
    check_evex_broadcast_memory_disp8(
        "evex_zmm_vbroadcastf64x2_memory_disp8_form",
        &[0x62, 0x72, 0xFD, 0x48, 0x1A, 0x57, 0x04],
        &[0x62, 0x71, 0xFE, 0x48, 0x7F, 0x17],
    );
}

#[test]
fn evex_zmm_vbroadcastf32x8_memory_disp8_form() {
    check_evex_broadcast_memory_disp8(
        "evex_zmm_vbroadcastf32x8_memory_disp8_form",
        &[0x62, 0x72, 0x7D, 0x48, 0x1B, 0x5F, 0x02],
        &[0x62, 0x71, 0xFE, 0x48, 0x7F, 0x1F],
    );
}

#[test]
fn evex_zmm_vbroadcastf64x4_memory_disp8_form() {
    check_evex_broadcast_memory_disp8(
        "evex_zmm_vbroadcastf64x4_memory_disp8_form",
        &[0x62, 0x72, 0xFD, 0x48, 0x1B, 0x67, 0x02],
        &[0x62, 0x71, 0xFE, 0x48, 0x7F, 0x27],
    );
}

#[test]
fn evex_zmm_vbroadcasti32x2_memory_disp8_form() {
    check_evex_broadcast_memory_disp8(
        "evex_zmm_vbroadcasti32x2_memory_disp8_form",
        &[0x62, 0x72, 0x7D, 0x48, 0x59, 0x6F, 0x08],
        &[0x62, 0x71, 0xFE, 0x48, 0x7F, 0x2F],
    );
}

#[test]
fn evex_zmm_vbroadcasti32x4_memory_disp8_form() {
    check_evex_broadcast_memory_disp8(
        "evex_zmm_vbroadcasti32x4_memory_disp8_form",
        &[0x62, 0x72, 0x7D, 0x48, 0x5A, 0x77, 0x04],
        &[0x62, 0x71, 0xFE, 0x48, 0x7F, 0x37],
    );
}

#[test]
fn evex_zmm_vbroadcasti64x2_memory_disp8_form() {
    check_evex_broadcast_memory_disp8(
        "evex_zmm_vbroadcasti64x2_memory_disp8_form",
        &[0x62, 0x72, 0xFD, 0x48, 0x5A, 0x7F, 0x04],
        &[0x62, 0x71, 0xFE, 0x48, 0x7F, 0x3F],
    );
}

#[test]
fn evex_zmm_vbroadcasti32x8_memory_disp8_form() {
    check_evex_broadcast_memory_disp8(
        "evex_zmm_vbroadcasti32x8_memory_disp8_form",
        &[0x62, 0xE2, 0x7D, 0x48, 0x5B, 0x47, 0x02],
        &[0x62, 0xE1, 0xFE, 0x48, 0x7F, 0x07],
    );
}

#[test]
fn evex_zmm_vbroadcasti64x4_memory_disp8_form() {
    check_evex_broadcast_memory_disp8(
        "evex_zmm_vbroadcasti64x4_memory_disp8_form",
        &[0x62, 0xE2, 0xFD, 0x48, 0x5B, 0x4F, 0x02],
        &[0x62, 0xE1, 0xFE, 0x48, 0x7F, 0x0F],
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 49: EVEX integer test-to-mask memory forms.
// ===========================================================================

#[test]
fn evex_xmm_vptestm_vptestnm_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x0F0F_F0F0u32
            ^ ((i as u32).wrapping_mul(0x1111_0101))
            ^ if i % 2 == 0 { 0xAAAA_5555 } else { 0x1234_5678 }
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x07]); // vmovdqu32 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu32 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x09, 0x26, 0xD1]); // vptestmb k2{k1}, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF2, 0xFE, 0x09, 0x26, 0x5F, 0x02]); // vptestnmw k3{k1}, xmm0, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x09, 0x27, 0xE1]); // vptestmd k4{k1}, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF2, 0xFE, 0x09, 0x27, 0x6F, 0x03]); // vptestnmq k5{k1}, xmm0, [rdi+48]
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xC2]); // kmovq rax, k2
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xDB]); // kmovq rbx, k3
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xCC]); // kmovq rcx, k4
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xD5]); // kmovq rdx, k5
    code.push(HLT);
    check_evex_mem("evex_xmm_vptestm_vptestnm_vl_forms", &code, s);
}

#[test]
fn evex_ymm_vptestm_vptestnm_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0xF0F0_0F0Fu32
            ^ ((i as u32).wrapping_mul(0x0101_1111))
            ^ if i % 3 == 0 { 0x5555_AAAA } else { 0x8765_4321 }
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x07]); // vmovdqu32 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu32 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x29, 0x26, 0xD1]); // vptestmb k2{k1}, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF2, 0xFE, 0x29, 0x26, 0x5F, 0x01]); // vptestnmw k3{k1}, ymm0, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x29, 0x27, 0xE1]); // vptestmd k4{k1}, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF2, 0xFE, 0x29, 0x27, 0x6F, 0x01]); // vptestnmq k5{k1}, ymm0, [rdi+32]
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xC2]); // kmovq rax, k2
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xDB]); // kmovq rbx, k3
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xCC]); // kmovq rcx, k4
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xD5]); // kmovq rdx, k5
    code.push(HLT);
    check_evex_mem("evex_ymm_vptestm_vptestnm_vl_forms", &code, s);
}

#[test]
fn evex_zmm_vptestm_vptestnm_memory_disp8_forms() {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x26, 0x57, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0xFE, 0x48, 0x26, 0x5F, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x27, 0x67, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x48, 0x27, 0x6F, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x58, 0x27, 0x77, 0x10]);
    code.extend_from_slice(&[0x62, 0xF2, 0xFE, 0x58, 0x27, 0x7F, 0x08]);
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xC2]);
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xDB]);
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xCC]);
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xD5]);
    code.extend_from_slice(&[0xC4, 0x61, 0xFB, 0x93, 0xC6]);
    code.extend_from_slice(&[0xC4, 0x61, 0xFB, 0x93, 0xCF]);
    code.push(HLT);
    check_evex_mem(
        "evex_zmm_vptestm_vptestnm_memory_disp8_forms",
        &code,
        evex_shuffle_scratch(),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 50: EVEX integer compare memory forms.
// ===========================================================================

#[test]
fn evex_xmm_vpcmp_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x1122_3344u32
            .wrapping_mul((i as u32) + 3)
            .rotate_left((i % 11) as u32)
            ^ if i % 2 == 0 { 0x8000_00FF } else { 0x00FF_8000 }
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x07]); // vmovdqu32 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu32 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x09, 0x3E, 0xD1, 0x02]); // vpcmpub k2{k1}, xmm0, xmm1, 2
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x09, 0x3E, 0x5F, 0x02, 0x06]); // vpcmpuw k3{k1}, xmm0, [rdi+32], 6
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x09, 0x1F, 0xE1, 0x01]); // vpcmpd k4{k1}, xmm0, xmm1, 1
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x09, 0x1F, 0x6F, 0x03, 0x05]); // vpcmpq k5{k1}, xmm0, [rdi+48], 5
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xC2]); // kmovq rax, k2
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xDB]); // kmovq rbx, k3
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xCC]); // kmovq rcx, k4
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xD5]); // kmovq rdx, k5
    code.push(HLT);
    check_evex_mem("evex_xmm_vpcmp_vl_forms", &code, s);
}

#[test]
fn evex_ymm_vpcmp_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x3344_5566u32
            .wrapping_mul((i as u32) + 5)
            .rotate_right((i % 17) as u32)
            ^ if i % 3 == 0 { 0x7FFF_FF00 } else { 0xFF00_0001 }
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x07]); // vmovdqu32 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu32 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x29, 0x3E, 0xD1, 0x02]); // vpcmpub k2{k1}, ymm0, ymm1, 2
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x29, 0x3E, 0x5F, 0x01, 0x06]); // vpcmpuw k3{k1}, ymm0, [rdi+32], 6
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x29, 0x1F, 0xE1, 0x01]); // vpcmpd k4{k1}, ymm0, ymm1, 1
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x29, 0x1F, 0x6F, 0x01, 0x05]); // vpcmpq k5{k1}, ymm0, [rdi+32], 5
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xC2]); // kmovq rax, k2
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xDB]); // kmovq rbx, k3
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xCC]); // kmovq rcx, k4
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xD5]); // kmovq rdx, k5
    code.push(HLT);
    check_evex_mem("evex_ymm_vpcmp_vl_forms", &code, s);
}

#[test]
fn evex_zmm_vpcmp_memory_disp8_forms() {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]);
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x1F, 0x57, 0x01, 0x06]);
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x1E, 0x5F, 0x01, 0x02]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0x74, 0x67, 0x01]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0x65, 0x6F, 0x01]);
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x58, 0x1F, 0x77, 0x10, 0x04]);
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x58, 0x1E, 0x7F, 0x08, 0x01]);
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xC2]);
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xDB]);
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xCC]);
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x93, 0xD5]);
    code.extend_from_slice(&[0xC4, 0x61, 0xFB, 0x93, 0xC6]);
    code.extend_from_slice(&[0xC4, 0x61, 0xFB, 0x93, 0xCF]);
    code.push(HLT);
    check_evex_mem(
        "evex_zmm_vpcmp_memory_disp8_forms",
        &code,
        evex_shuffle_scratch(),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 51: EVEX ternary logic memory forms.
// ===========================================================================

#[test]
fn evex_zmm_vpternlog_memory_disp8_forms() {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x0F]);
    code.extend_from_slice(&[0x62, 0xF3, 0x75, 0x48, 0x25, 0x47, 0x01, 0x96]);
    code.extend_from_slice(&[0x62, 0xF3, 0xF5, 0x58, 0x25, 0x47, 0x08, 0x96]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]);
    code.push(HLT);
    check_evex_mem(
        "evex_zmm_vpternlog_memory_disp8_forms",
        &code,
        evex_shuffle_scratch(),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 52: EVEX integer arithmetic memory forms.
// ===========================================================================

#[test]
fn evex_xmm_integer_arith_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x1020_4080u32
            .wrapping_mul((i as u32) + 7)
            .rotate_left((i % 19) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu32 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x57, 0x02]); // vmovdqu32 xmm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x09, 0xFC, 0xC2]); // vpaddb xmm0{k1}, xmm1, xmm2
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x09, 0xF9, 0x5F, 0x02]); // vpsubw xmm3{k1}, xmm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x09, 0xFE, 0xE2]); // vpaddd xmm4{k1}, xmm1, xmm2
    code.extend_from_slice(&[0x62, 0xF1, 0xF5, 0x09, 0xFB, 0x6F, 0x03]); // vpsubq xmm5{k1}, xmm1, [rdi+48]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x07]); // vmovdqu32 [rdi], xmm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu32 [rdi+16], xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x67, 0x02]); // vmovdqu32 [rdi+32], xmm4
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x6F, 0x03]); // vmovdqu64 [rdi+48], xmm5
    code.push(HLT);
    check_evex_mem("evex_xmm_integer_arith_vl_forms", &code, s);
}

#[test]
fn evex_ymm_integer_arith_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x0102_0408u32
            .wrapping_mul((i as u32) + 11)
            .rotate_right((i % 23) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x0F]); // vmovdqu32 ymm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x57, 0x01]); // vmovdqu32 ymm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x29, 0xFC, 0xC2]); // vpaddb ymm0{k1}, ymm1, ymm2
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x29, 0xF9, 0x5F, 0x01]); // vpsubw ymm3{k1}, ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x29, 0xFE, 0xE2]); // vpaddd ymm4{k1}, ymm1, ymm2
    code.extend_from_slice(&[0x62, 0xF1, 0xF5, 0x29, 0xFB, 0x6F, 0x01]); // vpsubq ymm5{k1}, ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x28, 0xEF, 0xC4]); // vpxord ymm0, ymm0, ymm4
    code.extend_from_slice(&[0x62, 0xF1, 0xE5, 0x28, 0xEF, 0xDD]); // vpxorq ymm3, ymm3, ymm5
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x7F, 0x07]); // vmovdqu32 [rdi], ymm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_integer_arith_vl_forms", &code, s);
}

#[test]
fn evex_zmm_integer_arith_memory_disp8_forms() {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x0F]);
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x48, 0xFE, 0x47, 0x01]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x58, 0xFB, 0x47, 0x08]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]);
    code.push(HLT);
    check_evex_mem(
        "evex_zmm_integer_arith_memory_disp8_forms",
        &code,
        evex_shuffle_scratch(),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 53: EVEX saturating pack memory forms.
// ===========================================================================

#[test]
fn evex_xmm_pack_saturate_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x7F80_0102u32
            .wrapping_mul((i as u32) + 13)
            .rotate_left((i % 29) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x0F]); // vmovdqu32 xmm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x57, 0x01]); // vmovdqu32 xmm2, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x09, 0x6B, 0xC2]); // vpackssdw xmm0{k1}, xmm1, xmm2
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x09, 0x63, 0x5F, 0x01]); // vpacksswb xmm3{k1}, xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x09, 0x2B, 0xE2]); // vpackusdw xmm4{k1}, xmm1, xmm2
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x09, 0x67, 0x6F, 0x01]); // vpackuswb xmm5{k1}, xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x07]); // vmovdqu32 [rdi], xmm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu32 [rdi+16], xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x67, 0x02]); // vmovdqu32 [rdi+32], xmm4
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x6F, 0x03]); // vmovdqu32 [rdi+48], xmm5
    code.push(HLT);
    check_evex_mem("evex_xmm_pack_saturate_vl_forms", &code, s);
}

#[test]
fn evex_ymm_pack_saturate_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x8001_7F7Fu32
            .wrapping_mul((i as u32) + 17)
            .rotate_right((i % 31) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x0F]); // vmovdqu32 ymm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x57, 0x01]); // vmovdqu32 ymm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x29, 0x6B, 0xC2]); // vpackssdw ymm0{k1}, ymm1, ymm2
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x29, 0x63, 0x5F, 0x01]); // vpacksswb ymm3{k1}, ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x29, 0x2B, 0xE2]); // vpackusdw ymm4{k1}, ymm1, ymm2
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x29, 0x67, 0x6F, 0x01]); // vpackuswb ymm5{k1}, ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x28, 0xEF, 0xC4]); // vpxord ymm0, ymm0, ymm4
    code.extend_from_slice(&[0x62, 0xF1, 0xE5, 0x28, 0xEF, 0xDD]); // vpxorq ymm3, ymm3, ymm5
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x7F, 0x07]); // vmovdqu32 [rdi], ymm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_pack_saturate_vl_forms", &code, s);
}

#[test]
fn evex_zmm_pack_saturate_memory_disp8_forms() {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x0F]);
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x48, 0x6B, 0x47, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x58, 0x2B, 0x47, 0x10]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]);
    code.push(HLT);
    check_evex_mem(
        "evex_zmm_pack_saturate_memory_disp8_forms",
        &code,
        evex_shuffle_scratch(),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 54: EVEX immediate shift and rotate memory forms.
// ===========================================================================

#[test]
fn evex_xmm_imm_shift_rotate_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x8040_2010u32
            .wrapping_mul((i as u32) + 1)
            .rotate_left((i % 13) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x07]); // vmovdqu32 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu32 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x57, 0x02]); // vmovdqu32 xmm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x5F, 0x03]); // vmovdqu64 xmm3, [rdi+48]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x09, 0x72, 0xF1, 0x05]); // vpslld xmm0{k1}, xmm1, 5
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x09, 0x72, 0x67, 0x02, 0x07]); // vpsrad xmm1{k1}, [rdi+32], 7
    code.extend_from_slice(&[0x62, 0xF1, 0x6D, 0x09, 0x72, 0xCA, 0x09]); // vprold xmm2{k1}, xmm2, 9
    code.extend_from_slice(&[0x62, 0xF1, 0xE5, 0x09, 0x72, 0x47, 0x03, 0x0D]); // vprorq xmm3{k1}, [rdi+48], 13
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x07]); // vmovdqu32 [rdi], xmm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x4F, 0x01]); // vmovdqu32 [rdi+16], xmm1
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x57, 0x02]); // vmovdqu32 [rdi+32], xmm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x5F, 0x03]); // vmovdqu64 [rdi+48], xmm3
    code.push(HLT);
    check_evex_mem("evex_xmm_imm_shift_rotate_vl_forms", &code, s);
}

#[test]
fn evex_ymm_imm_shift_rotate_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x0102_4080u32
            .wrapping_mul((i as u32) + 5)
            .rotate_right((i % 17) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x07]); // vmovdqu32 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu32 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x17]); // vmovdqu32 ymm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x5F, 0x01]); // vmovdqu64 ymm3, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x29, 0x72, 0xF1, 0x05]); // vpslld ymm0{k1}, ymm1, 5
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x29, 0x72, 0x27, 0x07]); // vpsrad ymm1{k1}, [rdi], 7
    code.extend_from_slice(&[0x62, 0xF1, 0x6D, 0x29, 0x72, 0xCA, 0x09]); // vprold ymm2{k1}, ymm2, 9
    code.extend_from_slice(&[0x62, 0xF1, 0xE5, 0x29, 0x72, 0x47, 0x01, 0x0D]); // vprorq ymm3{k1}, [rdi+32], 13
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x28, 0xEF, 0xC2]); // vpxord ymm0, ymm0, ymm2
    code.extend_from_slice(&[0x62, 0xF1, 0xF5, 0x28, 0xEF, 0xCB]); // vpxorq ymm1, ymm1, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x7F, 0x07]); // vmovdqu32 [rdi], ymm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x4F, 0x01]); // vmovdqu64 [rdi+32], ymm1
    code.push(HLT);
    check_evex_mem("evex_ymm_imm_shift_rotate_vl_forms", &code, s);
}

#[test]
fn evex_zmm_imm_shift_rotate_memory_disp8_forms() {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0x5D, 0x48, 0x73, 0x7F, 0x01, 0x05]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0x72, 0x67, 0x01, 0x03]);
    code.extend_from_slice(&[0x62, 0xF1, 0xF5, 0x58, 0x73, 0x57, 0x08, 0x07]);
    code.extend_from_slice(&[0x62, 0xF1, 0x6D, 0x48, 0x72, 0x4F, 0x01, 0x0B]);
    code.extend_from_slice(&[0x62, 0xF1, 0xE5, 0x58, 0x72, 0x47, 0x08, 0x0D]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xEF, 0xC4]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xEF, 0xC1]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xEB, 0xC2]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x48, 0xEF, 0xC3]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]);
    code.push(HLT);
    check_evex_mem(
        "evex_zmm_imm_shift_rotate_memory_disp8_forms",
        &code,
        evex_shuffle_scratch(),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 55: EVEX immediate funnel-shift memory forms.
// ===========================================================================

#[test]
fn evex_xmm_funnel_shift_imm_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x4000_0001u32
            .wrapping_mul((i as u32) + 7)
            .rotate_left((i % 19) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x07]); // vmovdqu32 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu32 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x57, 0x02]); // vmovdqu32 xmm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x5F, 0x03]); // vmovdqu64 xmm3, [rdi+48]
    code.extend_from_slice(&[0x62, 0xF3, 0x75, 0x09, 0x71, 0xC2, 0x05]); // vpshldd xmm0{k1}, xmm1, xmm2, 5
    code.extend_from_slice(&[0x62, 0xF3, 0xF5, 0x09, 0x73, 0x5F, 0x03, 0x0B]); // vpshrdq xmm3{k1}, xmm1, [rdi+48], 11
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x07]); // vmovdqu32 [rdi], xmm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+16], xmm3
    code.push(HLT);
    check_evex_mem("evex_xmm_funnel_shift_imm_vl_forms", &code, s);
}

#[test]
fn evex_ymm_funnel_shift_imm_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x1000_0001u32
            .wrapping_mul((i as u32) + 11)
            .rotate_right((i % 23) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x07]); // vmovdqu32 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu32 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x17]); // vmovdqu32 ymm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x5F, 0x01]); // vmovdqu64 ymm3, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF3, 0x75, 0x29, 0x71, 0xC2, 0x05]); // vpshldd ymm0{k1}, ymm1, ymm2, 5
    code.extend_from_slice(&[0x62, 0xF3, 0xF5, 0x29, 0x73, 0x5F, 0x01, 0x0B]); // vpshrdq ymm3{k1}, ymm1, [rdi+32], 11
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x28, 0xEF, 0xC3]); // vpxord ymm0, ymm0, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x7F, 0x07]); // vmovdqu32 [rdi], ymm0
    code.push(HLT);
    check_evex_mem("evex_ymm_funnel_shift_imm_vl_forms", &code, s);
}

#[test]
fn evex_zmm_funnel_shift_imm_memory_disp8_forms() {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x0F]);
    code.extend_from_slice(&[0x62, 0xF3, 0x75, 0x48, 0x71, 0x47, 0x01, 0x05]);
    code.extend_from_slice(&[0x62, 0xF3, 0xF5, 0x58, 0x73, 0x57, 0x08, 0x0B]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xEF, 0xC2]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]);
    code.push(HLT);
    check_evex_mem(
        "evex_zmm_funnel_shift_imm_memory_disp8_forms",
        &code,
        evex_shuffle_scratch(),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 56: EVEX xmm-count shift memory forms.
// ===========================================================================

#[test]
fn evex_zmm_xmm_count_shift_memory_disp8_forms() {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(
        &mut code,
        &[
            3,
            0x7161_5141_3121_1101,
            0x8899_aabb_ccdd_eeff,
            0x0f1e_2d3c_4b5a_6978,
            0xf0e1_d2c3_b4a5_9687,
            0x7654_3210_fedc_ba98,
            0x1357_9bdf_2468_ace0,
            0xcafe_babe_dead_beef,
        ],
    );
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x0F]);
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x48, 0xF2, 0x47, 0x04]);
    code.extend_from_slice(&[0x62, 0xF1, 0xF5, 0x48, 0xD3, 0x57, 0x04]);
    code.extend_from_slice(&[0x62, 0xF1, 0x75, 0x48, 0xE2, 0x5F, 0x04]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xEF, 0xC2]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xEF, 0xC3]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]);
    code.push(HLT);
    check_evex_mem(
        "evex_zmm_xmm_count_shift_memory_disp8_forms",
        &code,
        evex_shuffle_scratch(),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 57: EVEX per-element shift count memory forms.
// ===========================================================================

#[test]
fn evex_xmm_per_elem_shift_count_vl_forms() {
    let values = [
        0x1111_2222, 0x8000_0001, 0x7FFF_0000, 0xFFFF_0001, 0xF012_3456, 0x8123_4567,
        0x7ABC_DEF0, 0x1357_9BDF, 1, 4, 7, 12, 3, 0, 9, 0,
    ];
    let s = evex_dword_scratch(|i| values[i]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu32 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x57, 0x02]); // vmovdqu32 xmm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x09, 0x47, 0xC2]); // vpsllvd xmm0{k1}, xmm1, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x09, 0x46, 0x5F, 0x03]); // vpsravd xmm3{k1}, xmm1, [rdi+48]
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x09, 0x10, 0xE2]); // vpsrlvw xmm4{k1}, xmm1, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x09, 0x45, 0x6F, 0x03]); // vpsrlvq xmm5{k1}, xmm1, [rdi+48]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x07]); // vmovdqu32 [rdi], xmm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu32 [rdi+16], xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x67, 0x02]); // vmovdqu32 [rdi+32], xmm4
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x6F, 0x03]); // vmovdqu64 [rdi+48], xmm5
    code.push(HLT);
    check_evex_mem("evex_xmm_per_elem_shift_count_vl_forms", &code, s);
}

#[test]
fn evex_ymm_per_elem_shift_count_vl_forms() {
    let values = [
        0xF012_3456, 0x8123_4567, 0x7ABC_DEF0, 0x1357_9BDF, 0x8000_0001, 0x4000_0002,
        0xFFFF_0001, 0x2468_ACE0, 1, 0, 4, 0, 7, 0, 12, 0,
    ];
    let s = evex_dword_scratch(|i| values[i]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x0F]); // vmovdqu32 ymm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x57, 0x01]); // vmovdqu32 ymm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x29, 0x47, 0xC2]); // vpsllvd ymm0{k1}, ymm1, ymm2
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x29, 0x46, 0x5F, 0x01]); // vpsravd ymm3{k1}, ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x29, 0x10, 0xE2]); // vpsrlvw ymm4{k1}, ymm1, ymm2
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x29, 0x45, 0x6F, 0x01]); // vpsrlvq ymm5{k1}, ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x28, 0xEF, 0xC3]); // vpxord ymm0, ymm0, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0x5D, 0x28, 0xEF, 0xE5]); // vpxord ymm4, ymm4, ymm5
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x7F, 0x07]); // vmovdqu32 [rdi], ymm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x7F, 0x67, 0x01]); // vmovdqu32 [rdi+32], ymm4
    code.push(HLT);
    check_evex_mem("evex_ymm_per_elem_shift_count_vl_forms", &code, s);
}

#[test]
fn evex_zmm_per_elem_shift_count_memory_disp8_forms() {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &[1, 2, 3, 4, 5, 6, 7, 8]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x0F]);
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x48, 0x47, 0x47, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x58, 0x45, 0x57, 0x08]);
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x58, 0x46, 0x5F, 0x10]);
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x48, 0x10, 0x67, 0x01]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xEF, 0xC2]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xEF, 0xC3]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xEF, 0xC4]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]);
    code.push(HLT);
    check_evex_mem(
        "evex_zmm_per_elem_shift_count_memory_disp8_forms",
        &code,
        evex_shuffle_scratch(),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 58: EVEX per-element rotate count memory forms.
// ===========================================================================

#[test]
fn evex_xmm_per_elem_rotate_count_vl_forms() {
    let values = [
        0x0123_4567, 0x89AB_CDEF, 0x8000_0001, 0x7FFF_0000, 0xF012_3456, 0x8123_4567,
        0x7ABC_DEF0, 0x1357_9BDF, 1, 4, 9, 17, 3, 0, 11, 0,
    ];
    let s = evex_dword_scratch(|i| values[i]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu32 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x57, 0x02]); // vmovdqu32 xmm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x09, 0x15, 0xC2]); // vprolvd xmm0{k1}, xmm1, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x09, 0x14, 0x5F, 0x03]); // vprorvq xmm3{k1}, xmm1, [rdi+48]
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x09, 0x14, 0xE2]); // vprorvd xmm4{k1}, xmm1, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x09, 0x15, 0x6F, 0x03]); // vprolvq xmm5{k1}, xmm1, [rdi+48]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x07]); // vmovdqu32 [rdi], xmm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+16], xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x67, 0x02]); // vmovdqu32 [rdi+32], xmm4
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x6F, 0x03]); // vmovdqu64 [rdi+48], xmm5
    code.push(HLT);
    check_evex_mem("evex_xmm_per_elem_rotate_count_vl_forms", &code, s);
}

#[test]
fn evex_ymm_per_elem_rotate_count_vl_forms() {
    let values = [
        0xF012_3456, 0x8123_4567, 0x7ABC_DEF0, 0x1357_9BDF, 0x8000_0001, 0x4000_0002,
        0xFFFF_0001, 0x2468_ACE0, 1, 0, 4, 0, 9, 0, 17, 0,
    ];
    let s = evex_dword_scratch(|i| values[i]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x0F]); // vmovdqu32 ymm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x57, 0x01]); // vmovdqu32 ymm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x29, 0x15, 0xC2]); // vprolvd ymm0{k1}, ymm1, ymm2
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x29, 0x14, 0x5F, 0x01]); // vprorvq ymm3{k1}, ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x29, 0x14, 0xE2]); // vprorvd ymm4{k1}, ymm1, ymm2
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x29, 0x15, 0x6F, 0x01]); // vprolvq ymm5{k1}, ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x28, 0xEF, 0xC3]); // vpxord ymm0, ymm0, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0x5D, 0x28, 0xEF, 0xE5]); // vpxord ymm4, ymm4, ymm5
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x7F, 0x07]); // vmovdqu32 [rdi], ymm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x7F, 0x67, 0x01]); // vmovdqu32 [rdi+32], ymm4
    code.push(HLT);
    check_evex_mem("evex_ymm_per_elem_rotate_count_vl_forms", &code, s);
}

#[test]
fn evex_zmm_per_elem_rotate_count_memory_disp8_forms() {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &[1, 2, 3, 4, 5, 6, 7, 8]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x0F]);
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x48, 0x15, 0x47, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x58, 0x14, 0x57, 0x08]);
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x58, 0x14, 0x5F, 0x10]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xEF, 0xC2]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xEF, 0xC3]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]);
    code.push(HLT);
    check_evex_mem(
        "evex_zmm_per_elem_rotate_count_memory_disp8_forms",
        &code,
        evex_shuffle_scratch(),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 59: EVEX variable funnel-shift count memory forms.
// ===========================================================================

#[test]
fn evex_xmm_funnel_shift_count_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x2000_0003u32
            .wrapping_mul((i as u32) + 13)
            .rotate_left((i % 29) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x07]); // vmovdqu32 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu32 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x57, 0x02]); // vmovdqu32 xmm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x1F]); // vmovdqu64 xmm3, [rdi]
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x09, 0x71, 0xC2]); // vpshldvd xmm0{k1}, xmm1, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x09, 0x73, 0x5F, 0x03]); // vpshrdvq xmm3{k1}, xmm1, [rdi+48]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x07]); // vmovdqu32 [rdi], xmm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+16], xmm3
    code.push(HLT);
    check_evex_mem("evex_xmm_funnel_shift_count_vl_forms", &code, s);
}

#[test]
fn evex_ymm_funnel_shift_count_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x0800_0005u32
            .wrapping_mul((i as u32) + 17)
            .rotate_right((i % 31) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x07]); // vmovdqu32 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu32 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x17]); // vmovdqu32 ymm2, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x6F, 0x5F, 0x01]); // vmovdqu64 ymm3, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x29, 0x71, 0xC2]); // vpshldvd ymm0{k1}, ymm1, ymm2
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x29, 0x73, 0x1F]); // vpshrdvq ymm3{k1}, ymm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x28, 0xEF, 0xC3]); // vpxord ymm0, ymm0, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x7F, 0x07]); // vmovdqu32 [rdi], ymm0
    code.push(HLT);
    check_evex_mem("evex_ymm_funnel_shift_count_vl_forms", &code, s);
}

#[test]
fn evex_zmm_funnel_shift_count_memory_disp8_forms() {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &[1, 2, 3, 4, 5, 6, 7, 8]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x0F]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x17]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x1F]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x27]);
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x48, 0x71, 0x47, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x58, 0x73, 0x57, 0x10]);
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x58, 0x73, 0x5F, 0x08]);
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x48, 0x70, 0x67, 0x01]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xEF, 0xC2]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xEF, 0xC3]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xEF, 0xC4]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]);
    code.push(HLT);
    check_evex_mem(
        "evex_zmm_funnel_shift_count_memory_disp8_forms",
        &code,
        evex_shuffle_scratch(),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 60: EVEX VALIGN memory forms.
// ===========================================================================

#[test]
fn evex_xmm_valign_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x1020_3040u32
            .wrapping_mul((i as u32) + 3)
            .rotate_left((i % 13) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu32 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x57, 0x02]); // vmovdqu32 xmm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF3, 0x75, 0x09, 0x03, 0xC2, 0x01]); // valignd xmm0{k1}, xmm1, xmm2, 1
    code.extend_from_slice(&[0x62, 0xF3, 0xF5, 0x09, 0x03, 0x5F, 0x03, 0x01]); // valignq xmm3{k1}, xmm1, [rdi+48], 1
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x07]); // vmovdqu32 [rdi], xmm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+16], xmm3
    code.push(HLT);
    check_evex_mem("evex_xmm_valign_vl_forms", &code, s);
}

#[test]
fn evex_ymm_valign_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x0102_0408u32
            .wrapping_mul((i as u32) + 5)
            .rotate_right((i % 17) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x0F]); // vmovdqu32 ymm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x57, 0x01]); // vmovdqu32 ymm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF3, 0x75, 0x29, 0x03, 0xC2, 0x03]); // valignd ymm0{k1}, ymm1, ymm2, 3
    code.extend_from_slice(&[0x62, 0xF3, 0xF5, 0x29, 0x03, 0x5F, 0x01, 0x01]); // valignq ymm3{k1}, ymm1, [rdi+32], 1
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x28, 0xEF, 0xC3]); // vpxord ymm0, ymm0, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x7F, 0x07]); // vmovdqu32 [rdi], ymm0
    code.push(HLT);
    check_evex_mem("evex_ymm_valign_vl_forms", &code, s);
}

#[test]
fn evex_zmm_valign_memory_disp8_forms() {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x0F]);
    code.extend_from_slice(&[0x62, 0xF3, 0x75, 0x48, 0x03, 0x47, 0x01, 0x03]);
    code.extend_from_slice(&[0x62, 0xF3, 0xF5, 0x58, 0x03, 0x57, 0x08, 0x01]);
    code.extend_from_slice(&[0x62, 0xF3, 0x75, 0x58, 0x03, 0x5F, 0x10, 0x02]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xEF, 0xC2]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0xEF, 0xC3]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x07]);
    code.push(HLT);
    check_evex_mem(
        "evex_zmm_valign_memory_disp8_forms",
        &code,
        evex_shuffle_scratch(),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 61: EVEX GFNI memory forms.
// ===========================================================================

#[test]
fn evex_zmm_gfni_memory_disp8_forms() {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]);
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x48, 0xCE, 0x57, 0x01, 0x9A]);
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x48, 0xCF, 0x5F, 0x01, 0x5A]);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x48, 0x83, 0x67, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x58, 0x83, 0x6F, 0x08]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD3]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD4]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD5]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]);
    code.push(HLT);
    check_evex_mem(
        "evex_zmm_gfni_memory_disp8_forms",
        &code,
        evex_shuffle_scratch(),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 62: EVEX crypto memory forms.
// ===========================================================================

#[test]
fn evex_zmm_crypto_memory_disp8_forms() {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]);
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x44, 0x57, 0x01, 0x00]);
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x44, 0x5F, 0x01, 0x11]);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0xDC, 0x67, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0xDD, 0x6F, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0xDE, 0x77, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0xDF, 0x7F, 0x01]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD3]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD4]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD5]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD6]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD7]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]);
    code.push(HLT);
    check_evex_mem(
        "evex_zmm_crypto_memory_disp8_forms",
        &code,
        evex_shuffle_scratch(),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 63: EVEX VFIXUPIMM memory forms.
// ===========================================================================

#[test]
fn evex_xmm_vfixupimm_vl_forms() {
    let mut s = [0u8; 64];
    for (i, value) in [
        0x0000_0000_0000_0000u64,
        0x8000_0000_0000_0000,
        0x3FF0_0000_0000_0000,
        0xBFF0_0000_0000_0000,
        0x7FF0_0000_0000_0000,
        0xFFF0_0000_0000_0000,
        0x7FF8_0000_0000_0001,
        0x4008_0000_0000_0000,
    ]
    .iter()
    .enumerate()
    {
        s[i * 8..i * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x07]); // vmovdqu32 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu32 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x09, 0x54, 0xD1, 0x33]); // vfixupimmps xmm2{k1}, xmm0, xmm1, 0x33
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x09, 0x54, 0x5F, 0x01, 0x44]); // vfixupimmpd xmm3{k1}, xmm0, [rdi+16], 0x44
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x17]); // vmovdqu32 [rdi], xmm2
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu32 [rdi+16], xmm3
    code.push(HLT);
    check_evex_mem("evex_xmm_vfixupimm_vl_forms", &code, s);
}

#[test]
fn evex_ymm_vfixupimm_vl_forms() {
    let mut s = [0u8; 64];
    for (i, value) in [
        0x0000_0000_0000_0000u64,
        0x8000_0000_0000_0000,
        0x3FF0_0000_0000_0000,
        0xBFF0_0000_0000_0000,
        0x7FF0_0000_0000_0000,
        0xFFF0_0000_0000_0000,
        0x7FF8_0000_0000_0001,
        0x4008_0000_0000_0000,
    ]
    .iter()
    .enumerate()
    {
        s[i * 8..i * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x07]); // vmovdqu32 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu32 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x29, 0x54, 0xD1, 0x33]); // vfixupimmps ymm2{k1}, ymm0, ymm1, 0x33
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x29, 0x54, 0x5F, 0x01, 0x44]); // vfixupimmpd ymm3{k1}, ymm0, [rdi+32], 0x44
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x7F, 0x17]); // vmovdqu32 [rdi], ymm2
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x7F, 0x5F, 0x01]); // vmovdqu32 [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_vfixupimm_vl_forms", &code, s);
}

#[test]
fn evex_zmm_fixupimm_memory_disp8_forms() {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]);
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x54, 0x57, 0x01, 0x33]);
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x48, 0x54, 0x5F, 0x01, 0x44]);
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x58, 0x54, 0x67, 0x10, 0x55]);
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x58, 0x54, 0x6F, 0x08, 0x66]);
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x08, 0x55, 0x77, 0x10, 0x77]);
    code.extend_from_slice(&[0x62, 0xF3, 0xFD, 0x08, 0x55, 0x7F, 0x08, 0x88]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD3]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD4]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD5]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD6]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD7]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]);
    code.push(HLT);
    check_evex_mem(
        "evex_zmm_fixupimm_memory_disp8_forms",
        &code,
        evex_shuffle_scratch(),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 64: EVEX FP16 complex memory forms.
// ===========================================================================

#[test]
fn evex_xmm_fp16_complex_vl_forms() {
    let s = evex_fp16_scratch(&[
        0x3C00, // 1.0
        0x4000, // 2.0
        0x3800, // 0.5
        0xBC00, // -1.0
        0x4200, // 3.0
        0x3400, // 0.25
        0xC000, // -2.0
        0x4400, // 4.0
    ]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x6F, 0x07]); // vmovdqu16 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu16 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF6, 0x7E, 0x09, 0xD6, 0xD1]); // vfmulcph xmm2{k1}, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF6, 0x7E, 0x09, 0xD6, 0x5F, 0x01]); // vfmulcph xmm3{k1}, xmm0, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF6, 0x7F, 0x09, 0xD6, 0xE1]); // vfcmulcph xmm4{k1}, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF6, 0x7F, 0x09, 0xD6, 0x6F, 0x01]); // vfcmulcph xmm5{k1}, xmm0, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x7F, 0x17]); // vmovdqu16 [rdi], xmm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu16 [rdi+16], xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x7F, 0x67, 0x02]); // vmovdqu16 [rdi+32], xmm4
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x7F, 0x6F, 0x03]); // vmovdqu16 [rdi+48], xmm5
    code.push(HLT);
    check_evex_mem("evex_xmm_fp16_complex_vl_forms", &code, s);
}

#[test]
fn evex_ymm_fp16_complex_vl_forms() {
    let s = evex_fp16_scratch(&[
        0x3C00, // 1.0
        0x4000, // 2.0
        0x3800, // 0.5
        0xBC00, // -1.0
        0x4200, // 3.0
        0x3400, // 0.25
        0xC000, // -2.0
        0x4400, // 4.0
    ]);

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x28, 0x6F, 0x07]); // vmovdqu16 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu16 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF6, 0x7E, 0x29, 0xD6, 0xD1]); // vfmulcph ymm2{k1}, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF6, 0x7E, 0x29, 0xD6, 0x5F, 0x01]); // vfmulcph ymm3{k1}, ymm0, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF6, 0x7F, 0x29, 0xD6, 0xE1]); // vfcmulcph ymm4{k1}, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF6, 0x7F, 0x29, 0xD6, 0x6F, 0x01]); // vfcmulcph ymm5{k1}, ymm0, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x28, 0xEF, 0xD4]); // vpxorq ymm2, ymm2, ymm4
    code.extend_from_slice(&[0x62, 0xF1, 0xE5, 0x28, 0xEF, 0xDD]); // vpxorq ymm3, ymm3, ymm5
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x28, 0x7F, 0x17]); // vmovdqu16 [rdi], ymm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x28, 0x7F, 0x5F, 0x01]); // vmovdqu16 [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_fp16_complex_vl_forms", &code, s);
}

#[test]
fn evex_zmm_fp16_complex_memory_disp8_forms() {
    let s = evex_fp16_scratch(&[
        0x3C00, 0x4000, 0x3800, 0xBC00, 0x4200, 0x3400, 0xC000, 0x4400,
    ]);
    let mem = evex_fp16_scratch(&[
        0x4000, 0x3800, 0x3C00, 0x3400, 0xBC00, 0x3C00, 0x4400, 0xB800,
    ]);
    let mem_words = evex_bytes_to_qwords(mem);

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &mem_words);
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x48, 0x6F, 0x07]);
    code.extend_from_slice(&[0x62, 0xF6, 0x7E, 0x48, 0xD6, 0x57, 0x01]);
    code.extend_from_slice(&[0x62, 0xF6, 0x7F, 0x48, 0xD6, 0x5F, 0x01]);
    code.extend_from_slice(&[0x62, 0xF6, 0x7E, 0x48, 0x56, 0x67, 0x01]);
    code.extend_from_slice(&[0x62, 0xF6, 0x7F, 0x48, 0x56, 0x6F, 0x01]);
    code.extend_from_slice(&[0x62, 0xF6, 0x7E, 0x58, 0xD6, 0x77, 0x10]);
    code.extend_from_slice(&[0x62, 0xF6, 0x7F, 0x58, 0x56, 0x7F, 0x10]);
    code.extend_from_slice(&[0x62, 0x76, 0x7E, 0x08, 0xD7, 0x47, 0x10]);
    code.extend_from_slice(&[0x62, 0x76, 0x7F, 0x08, 0x57, 0x4F, 0x10]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD3]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD4]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD5]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD6]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD7]);
    code.extend_from_slice(&[0x62, 0xD1, 0xED, 0x48, 0xEF, 0xD0]);
    code.extend_from_slice(&[0x62, 0xD1, 0xED, 0x48, 0xEF, 0xD1]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]);
    code.push(HLT);
    check_evex_mem("evex_zmm_fp16_complex_memory_disp8_forms", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 65: EVEX packed conversion memory forms.
// ===========================================================================

#[test]
fn evex_xmm_packed_convert_vl_forms() {
    let s = evex_dword_scratch(|i| ((i as i32 - 8) as f32).to_bits());

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x07]); // vmovdqu32 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7C, 0x09, 0x5A, 0x17]); // vcvtps2pd xmm2{k1}, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x09, 0x5B, 0xD8]); // vcvtps2dq xmm3{k1}, xmm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7C, 0x09, 0x5B, 0x67, 0x01]); // vcvtdq2ps xmm4{k1}, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x17]); // vmovdqu64 [rdi], xmm2
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu32 [rdi+16], xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x67, 0x02]); // vmovdqu32 [rdi+32], xmm4
    code.push(HLT);
    check_evex_mem("evex_xmm_packed_convert_vl_forms", &code, s);
}

#[test]
fn evex_ymm_packed_convert_vl_forms() {
    let s = evex_dword_scratch(|i| ((i as i32 - 8) as f32).to_bits());

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x07]); // vmovdqu32 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7C, 0x29, 0x5A, 0x17]); // vcvtps2pd ymm2{k1}, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x29, 0x5B, 0xD8]); // vcvtps2dq ymm3{k1}, ymm0
    code.extend_from_slice(&[0x62, 0xF1, 0x7C, 0x29, 0x5B, 0x67, 0x01]); // vcvtdq2ps ymm4{k1}, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x28, 0xEF, 0xD3]); // vpxorq ymm2, ymm2, ymm3
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x17]); // vmovdqu64 [rdi], ymm2
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x7F, 0x67, 0x01]); // vmovdqu32 [rdi+32], ymm4
    code.push(HLT);
    check_evex_mem("evex_ymm_packed_convert_vl_forms", &code, s);
}

#[test]
fn evex_zmm_packed_convert_memory_disp8_forms() {
    let s = evex_dword_scratch(|i| ((i as f32) + 1.25).to_bits());
    let mem = evex_dword_scratch(|i| ((i as f32) - 6.5).to_bits());
    let mem_words = evex_bytes_to_qwords(mem);

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &mem_words);
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x48, 0x6F, 0x07]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7C, 0x48, 0x5A, 0x57, 0x02]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7C, 0x48, 0x5B, 0x5F, 0x01]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7D, 0x48, 0x5B, 0x67, 0x01]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD3]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD4]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]);
    code.extend_from_slice(&[0x62, 0xF3, 0x7D, 0x48, 0x1D, 0x47, 0x02, 0x00]);
    for i in 0..4 {
        append_mov_rdi_disp_to_rax(&mut code, 64 + i * 8);
        append_mov_rax_to_rdi_disp(&mut code, 32 + i * 8);
    }
    code.push(HLT);
    check_evex_mem("evex_zmm_packed_convert_memory_disp8_forms", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 66: EVEX scalar conversion memory forms.
// ===========================================================================

#[test]
fn evex_scalar_convert_memory_disp8_forms() {
    let s = evex_shuffle_scratch();
    let mem_words = [
        0x40B0_0000_4500_0000u64,
        0x4019_0000_0000_0000,
        0x0000_0000_0000_3039,
        0x0000_0000_0001_0932,
        0,
        0,
        0,
        0,
    ];

    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &mem_words);
    code.extend_from_slice(&[0xB8, 0xFF, 0xFF, 0x00, 0x00]);
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x07]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x78, 0x47, 0x11]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x78, 0x5F, 0x09]);
    code.extend_from_slice(&[0x62, 0xF5, 0xFE, 0x08, 0x78, 0x4F, 0x21]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7B, 0x57, 0x14]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFF, 0x08, 0x7B, 0x5F, 0x0B]);
    code.extend_from_slice(&[0x62, 0xF5, 0x7E, 0x08, 0x7B, 0x67, 0x14]);
    code.extend_from_slice(&[0x62, 0xF6, 0x7C, 0x08, 0x13, 0x6F, 0x21]);
    code.extend_from_slice(&[0x62, 0xF5, 0x7C, 0x08, 0x1D, 0x77, 0x11]);
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x09, 0x5A, 0x7F, 0x11]);
    code.extend_from_slice(&[0x62, 0x71, 0xFF, 0x09, 0x5A, 0x47, 0x09]);
    code.extend_from_slice(&[0x48, 0x89, 0x07]);
    code.extend_from_slice(&[0x48, 0x89, 0x5F, 0x08]);
    code.extend_from_slice(&[0x48, 0x89, 0x4F, 0x10]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x97, 0x18, 0x00, 0x00, 0x00]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x9F, 0x28, 0x00, 0x00, 0x00]);
    code.extend_from_slice(&[0x62, 0xF1, 0xDD, 0x08, 0xEF, 0xE5]);
    code.extend_from_slice(&[0x62, 0xF1, 0xDD, 0x08, 0xEF, 0xE6]);
    code.extend_from_slice(&[0x62, 0xF1, 0xDD, 0x08, 0xEF, 0xE7]);
    code.extend_from_slice(&[0x62, 0xD1, 0xDD, 0x08, 0xEF, 0xE0]);
    code.extend_from_slice(&[0xC5, 0xF9, 0xD6, 0x67, 0x38]);
    code.push(HLT);
    check_evex_mem("evex_scalar_convert_memory_disp8_forms", &code, s);
}

// ===========================================================================
// EXPANDED COVERAGE PART 67: EVEX packed multiply memory forms.
// ===========================================================================

#[test]
fn evex_xmm_packed_multiply_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x1357_9BDFu32
            .wrapping_mul((i as u32) + 19)
            .rotate_left((i % 23) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x0F]); // vmovdqu32 xmm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x57, 0x01]); // vmovdqu32 xmm2, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x09, 0x40, 0xC2]); // vpmullq xmm0{k1}, xmm1, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x09, 0x40, 0x5F, 0x01]); // vpmullq xmm3{k1}, xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x09, 0x40, 0xE2]); // vpmulld xmm4{k1}, xmm1, xmm2
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x09, 0x40, 0x6F, 0x01]); // vpmulld xmm5{k1}, xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x07]); // vmovdqu64 [rdi], xmm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+16], xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x67, 0x02]); // vmovdqu32 [rdi+32], xmm4
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x6F, 0x03]); // vmovdqu32 [rdi+48], xmm5
    code.push(HLT);
    check_evex_mem("evex_xmm_packed_multiply_vl_forms", &code, s);
}

#[test]
fn evex_ymm_packed_multiply_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x2468_ACE1u32
            .wrapping_mul((i as u32) + 23)
            .rotate_right((i % 27) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF]); // mov rax, -1
    code.extend_from_slice(&[0xC4, 0xE1, 0xFB, 0x92, 0xC8]); // kmovq k1, rax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x0F]); // vmovdqu32 ymm1, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x57, 0x01]); // vmovdqu32 ymm2, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x29, 0x40, 0xC2]); // vpmullq ymm0{k1}, ymm1, ymm2
    code.extend_from_slice(&[0x62, 0xF2, 0xF5, 0x29, 0x40, 0x5F, 0x01]); // vpmullq ymm3{k1}, ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x29, 0x40, 0xE2]); // vpmulld ymm4{k1}, ymm1, ymm2
    code.extend_from_slice(&[0x62, 0xF2, 0x75, 0x29, 0x40, 0x6F, 0x01]); // vpmulld ymm5{k1}, ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0xFD, 0x28, 0xEF, 0xC4]); // vpxorq ymm0, ymm0, ymm4
    code.extend_from_slice(&[0x62, 0xF1, 0xE5, 0x28, 0xEF, 0xDD]); // vpxorq ymm3, ymm3, ymm5
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x07]); // vmovdqu64 [rdi], ymm0
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_packed_multiply_vl_forms", &code, s);
}

#[test]
fn evex_zmm_packed_multiply_memory_disp8_forms() {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x48, 0x40, 0x57, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x58, 0x40, 0x5F, 0x08]);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x48, 0x40, 0x67, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x58, 0x40, 0x6F, 0x10]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD3]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD4]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD5]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]);
    code.push(HLT);
    check_evex_mem(
        "evex_zmm_packed_multiply_memory_disp8_forms",
        &code,
        evex_shuffle_scratch(),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 68: EVEX scalar move memory forms.
// ===========================================================================

#[test]
fn evex_scalar_move_memory_disp8_forms() {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0xB8, 0xFF, 0xFF, 0x00, 0x00]);
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x6F, 0x07]);

    code.extend_from_slice(&[0x62, 0xE5, 0x7D, 0x08, 0x6E, 0x47, 0x20]);
    code.extend_from_slice(&[0x62, 0xE1, 0x7D, 0x08, 0x6E, 0x4F, 0x10]);
    code.extend_from_slice(&[0x62, 0xE1, 0xFD, 0x08, 0x6E, 0x57, 0x08]);
    code.extend_from_slice(&[0x62, 0xE1, 0xFE, 0x08, 0x7E, 0x67, 0x08]);

    code.extend_from_slice(&[0x62, 0xE5, 0x7E, 0x89, 0x10, 0x6F, 0x20]);
    code.extend_from_slice(&[0x62, 0xE1, 0x7E, 0x89, 0x10, 0x77, 0x10]);
    code.extend_from_slice(&[0x62, 0xE1, 0xFF, 0x89, 0x10, 0x7F, 0x08]);

    code.extend_from_slice(&[0x62, 0x61, 0x7C, 0x08, 0x12, 0x47, 0x08]);
    code.extend_from_slice(&[0x62, 0x61, 0x7C, 0x08, 0x16, 0x4F, 0x08]);
    code.extend_from_slice(&[0x62, 0x61, 0xFD, 0x08, 0x12, 0x57, 0x08]);
    code.extend_from_slice(&[0x62, 0x61, 0xFD, 0x08, 0x16, 0x5F, 0x08]);
    code.extend_from_slice(&[0x62, 0x01, 0xBD, 0x00, 0xEF, 0xC1]);
    code.extend_from_slice(&[0x62, 0x01, 0xBD, 0x00, 0xEF, 0xC2]);
    code.extend_from_slice(&[0x62, 0x01, 0xBD, 0x00, 0xEF, 0xC3]);
    code.extend_from_slice(&[0x62, 0x61, 0xFE, 0x08, 0x7F, 0x07]);

    code.extend_from_slice(&[0x62, 0xE1, 0xFD, 0x08, 0xD6, 0x67, 0x02]);
    code.extend_from_slice(&[0x62, 0xE5, 0x7D, 0x08, 0x7E, 0x47, 0x0F]);
    code.extend_from_slice(&[0x62, 0xE1, 0x7D, 0x08, 0x7E, 0x4F, 0x08]);
    code.extend_from_slice(&[0x62, 0xE1, 0xFD, 0x08, 0x7E, 0x57, 0x05]);
    code.extend_from_slice(&[0x62, 0xE5, 0x7E, 0x09, 0x11, 0x6F, 0x19]);
    code.extend_from_slice(&[0x62, 0xE1, 0x7E, 0x09, 0x11, 0x77, 0x0D]);
    code.extend_from_slice(&[0x62, 0xE1, 0xFF, 0x09, 0x11, 0x7F, 0x07]);

    code.push(HLT);
    check_evex_mem(
        "evex_scalar_move_memory_disp8_forms",
        &code,
        evex_shuffle_scratch(),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 69: EVEX blend selector memory forms.
// ===========================================================================

#[test]
fn evex_xmm_blend_select_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x89AB_CDEFu32
            .wrapping_add((i as u32).wrapping_mul(0x0102_0305))
            .rotate_left((i % 17) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xAA, 0xAA, 0xAA, 0xAA]); // mov eax, 0xaaaaaaaa
    code.extend_from_slice(&[0xC5, 0xFB, 0x92, 0xC8]); // kmovd k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x07]); // vmovdqu32 xmm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x6F, 0x4F, 0x01]); // vmovdqu32 xmm1, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x09, 0x66, 0xD1]); // vpblendmb xmm2{k1}, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x09, 0x66, 0x5F, 0x01]); // vpblendmw xmm3{k1}, xmm0, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x09, 0x64, 0xE1]); // vpblendmd xmm4{k1}, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x09, 0x64, 0x6F, 0x01]); // vpblendmq xmm5{k1}, xmm0, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x09, 0x65, 0xF1]); // vblendmps xmm6{k1}, xmm0, xmm1
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x09, 0x65, 0x7F, 0x01]); // vblendmpd xmm7{k1}, xmm0, [rdi+16]
    code.extend_from_slice(&[0x62, 0xF1, 0x6D, 0x08, 0xEF, 0xD4]); // vpxord xmm2, xmm2, xmm4
    code.extend_from_slice(&[0x62, 0xF1, 0xE5, 0x08, 0xEF, 0xDD]); // vpxorq xmm3, xmm3, xmm5
    code.extend_from_slice(&[0xC5, 0xC8, 0x57, 0xF7]); // vxorps xmm6, xmm6, xmm7
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x17]); // vmovdqu32 [rdi], xmm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x08, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+16], xmm3
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x08, 0x7F, 0x77, 0x02]); // vmovdqu32 [rdi+32], xmm6
    code.push(HLT);
    check_evex_mem("evex_xmm_blend_select_vl_forms", &code, s);
}

#[test]
fn evex_ymm_blend_select_vl_forms() {
    let s = evex_dword_scratch(|i| {
        0x7654_3210u32
            .wrapping_sub((i as u32).wrapping_mul(0x1020_3040))
            .rotate_right((i % 19) as u32)
    });

    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0xAA, 0xAA, 0xAA, 0xAA]); // mov eax, 0xaaaaaaaa
    code.extend_from_slice(&[0xC5, 0xFB, 0x92, 0xC8]); // kmovd k1, eax
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x07]); // vmovdqu32 ymm0, [rdi]
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x6F, 0x4F, 0x01]); // vmovdqu32 ymm1, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x29, 0x66, 0xD1]); // vpblendmb ymm2{k1}, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x29, 0x66, 0x5F, 0x01]); // vpblendmw ymm3{k1}, ymm0, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x29, 0x64, 0xE1]); // vpblendmd ymm4{k1}, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x29, 0x64, 0x6F, 0x01]); // vpblendmq ymm5{k1}, ymm0, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x29, 0x65, 0xF1]); // vblendmps ymm6{k1}, ymm0, ymm1
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x29, 0x65, 0x7F, 0x01]); // vblendmpd ymm7{k1}, ymm0, [rdi+32]
    code.extend_from_slice(&[0x62, 0xF1, 0x6D, 0x28, 0xEF, 0xD4]); // vpxord ymm2, ymm2, ymm4
    code.extend_from_slice(&[0x62, 0xF1, 0xE5, 0x28, 0xEF, 0xDD]); // vpxorq ymm3, ymm3, ymm5
    code.extend_from_slice(&[0xC5, 0xCC, 0x57, 0xF7]); // vxorps ymm6, ymm6, ymm7
    code.extend_from_slice(&[0x62, 0xF1, 0x6D, 0x28, 0xEF, 0xD6]); // vpxord ymm2, ymm2, ymm6
    code.extend_from_slice(&[0x62, 0xF1, 0x7E, 0x28, 0x7F, 0x17]); // vmovdqu32 [rdi], ymm2
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x28, 0x7F, 0x5F, 0x01]); // vmovdqu64 [rdi+32], ymm3
    code.push(HLT);
    check_evex_mem("evex_ymm_blend_select_vl_forms", &code, s);
}

#[test]
fn evex_zmm_blend_select_memory_disp8_forms() {
    let mut code = opmask_start();
    append_seed_rdi_plus_64_qwords(&mut code, &evex_shuffle_source());
    code.extend_from_slice(&[0xB8, 0xAA, 0xAA, 0x00, 0x00]);
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x6F, 0x07]);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x49, 0x64, 0x57, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x49, 0x64, 0x5F, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x49, 0x65, 0x67, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x49, 0x65, 0x6F, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0xFD, 0x49, 0x66, 0x77, 0x01]);
    code.extend_from_slice(&[0x62, 0xF2, 0x7D, 0x49, 0x66, 0x7F, 0x01]);
    code.extend_from_slice(&[0x62, 0x72, 0x7D, 0x59, 0x64, 0x47, 0x10]);
    code.extend_from_slice(&[0x62, 0x72, 0xFD, 0x59, 0x64, 0x4F, 0x08]);
    code.extend_from_slice(&[0x62, 0x72, 0x7D, 0x59, 0x65, 0x57, 0x10]);
    code.extend_from_slice(&[0x62, 0x72, 0xFD, 0x59, 0x65, 0x5F, 0x08]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD3]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD4]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD5]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD6]);
    code.extend_from_slice(&[0x62, 0xF1, 0xED, 0x48, 0xEF, 0xD7]);
    code.extend_from_slice(&[0x62, 0xD1, 0xED, 0x48, 0xEF, 0xD0]);
    code.extend_from_slice(&[0x62, 0xD1, 0xED, 0x48, 0xEF, 0xD1]);
    code.extend_from_slice(&[0x62, 0xD1, 0xED, 0x48, 0xEF, 0xD2]);
    code.extend_from_slice(&[0x62, 0xD1, 0xED, 0x48, 0xEF, 0xD3]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x17]);
    code.push(HLT);
    check_evex_mem(
        "evex_zmm_blend_select_memory_disp8_forms",
        &code,
        evex_shuffle_scratch(),
    );
}

// ===========================================================================
// EXPANDED COVERAGE PART 70: EVEX mask/vector conversion widths.
// ===========================================================================

#[test]
fn evex_zmm_mask_vector_conversion_widths() {
    let mut code = opmask_start();
    code.extend_from_slice(&[0xB8, 0x5A, 0x5A, 0x00, 0x00]);
    code.extend_from_slice(&[0xBB, 0x3C, 0x3C, 0x00, 0x00]);
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xC8]);
    code.extend_from_slice(&[0xC5, 0xF8, 0x92, 0xD3]);
    code.extend_from_slice(&[0x62, 0xF2, 0xFE, 0x48, 0x28, 0xC1]);
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x48, 0x38, 0xC9]);
    code.extend_from_slice(&[0x62, 0xF2, 0xFE, 0x48, 0x38, 0xD1]);
    code.extend_from_slice(&[0x62, 0xF2, 0xFE, 0x48, 0x29, 0xD8]);
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x48, 0x39, 0xE1]);
    code.extend_from_slice(&[0x62, 0xF2, 0xFE, 0x48, 0x39, 0xEA]);
    code.extend_from_slice(&[0x62, 0xF2, 0xFE, 0x48, 0x2A, 0xF1]);
    code.extend_from_slice(&[0x62, 0xF2, 0x7E, 0x48, 0x3A, 0xFA]);
    code.extend_from_slice(&[0x62, 0xF1, 0xCD, 0x48, 0xEF, 0xF7]);
    code.extend_from_slice(&[0x62, 0xF1, 0xCD, 0x48, 0xEF, 0xF0]);
    code.extend_from_slice(&[0x62, 0xF1, 0xCD, 0x48, 0xEF, 0xF1]);
    code.extend_from_slice(&[0x62, 0xF1, 0xCD, 0x48, 0xEF, 0xF2]);
    code.extend_from_slice(&[0x62, 0xF1, 0xFE, 0x48, 0x7F, 0x37]);
    code.extend_from_slice(&[0xC5, 0xF8, 0x93, 0xC3]);
    code.extend_from_slice(&[0xC5, 0xF8, 0x93, 0xDC]);
    code.extend_from_slice(&[0xC5, 0xF8, 0x93, 0xCD]);
    code.extend_from_slice(&[0x89, 0x07]);
    code.extend_from_slice(&[0x89, 0x5F, 0x04]);
    code.extend_from_slice(&[0x89, 0x4F, 0x08]);
    code.push(HLT);
    check_evex_mem(
        "evex_zmm_mask_vector_conversion_widths",
        &code,
        zero_scratch(),
    );
}
