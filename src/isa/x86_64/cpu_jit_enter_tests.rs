//! Helper, native, fault-frontier, and direct differentials for x86 `ENTER`.

use super::*;
use crate::smir::lower::runtime::GuestRegs;
use crate::vm::vcpu::{MemAccess, VCpu};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const CR0_PE: u64 = 1;
const EFER_LMA: u64 = 1 << 10;

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x1_0000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

fn test_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.cr0 = CR0_PE;
    vcpu.sregs.efer = EFER_LMA;
    vcpu.sregs.cs.l = true;
    vcpu.regs.rip = 0;
    vcpu.regs.rax = 0x0123_4567_89AB_CDEF;
    vcpu.regs.rcx = 0x1111_2222_3333_4444;
    vcpu.regs.rdx = 0x5555_6666_7777_8888;
    vcpu.regs.rbx = 0xBBBB_BBBB_BBBB_BBBB;
    vcpu.regs.rsp = 0x9000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rsi = 0x9999_AAAA_BBBB_CCCC;
    vcpu.regs.rdi = 0xDDDD_EEEE_FFFF_0000;
    vcpu.regs.r8 = 0x0808_0808_0808_0808;
    vcpu.regs.r9 = 0x0909_0909_0909_0909;
    vcpu.regs.r10 = 0x1010_1010_1010_1010;
    vcpu.regs.r11 = 0x1111_1111_1111_1111;
    vcpu.regs.r12 = 0x1212_1212_1212_1212;
    vcpu.regs.r13 = 0x1313_1313_1313_1313;
    vcpu.regs.r14 = 0x1414_1414_1414_1414;
    vcpu.regs.r15 = 0x1515_1515_1515_1515;
    vcpu.regs.rflags = 0x2 | 0x08D5 | flags::bits::DF;
    vcpu.set_jit_mem(true);
    vcpu.set_jit_call(false);
    vcpu
}

fn helper_state(vcpu: &mut X86_64Vcpu, rsp: u64, rbp: u64) -> GuestRegs {
    let mut state = GuestRegs::default();
    state.ctx = (vcpu as *mut X86_64Vcpu) as u64;
    state.efer = EFER_LMA;
    state.cs_l = 1;
    state.apx_enabled = u64::from(vcpu.apx_enabled());
    state.gpr[4] = rsp;
    state.gpr[5] = rbp;
    state
}

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

fn initialize_display(memory: &GuestMemoryMmap, rbp: u64, width: u8, nesting: u8) {
    for index in 1..nesting {
        let address = rbp - u64::from(index) * u64::from(width);
        let value = 0xA500_u64 | u64::from(index);
        memory
            .write_slice(
                &value.to_le_bytes()[..usize::from(width)],
                GuestAddress(address),
            )
            .unwrap();
    }
}

#[test]
fn enter_helper_preserves_alias_order_and_reports_only_architectural_accesses() {
    let memory = memory_with_code(&[]);
    let mut vcpu = test_vcpu(memory.clone());
    VCpu::set_mem_recording(&mut vcpu, true);
    vcpu.jit_mem_trace = Some(Vec::new());
    vcpu.jit_mem_log = Some(Vec::new());
    let mut state = helper_state(&mut vcpu, 0x8000, 0x8000);

    assert_eq!(unsafe { rax_jit_enter(&mut state, 0x20, 2, 8, 0) }, 1);
    assert_eq!(state.gpr[4], 0x7FC8);
    assert_eq!(state.gpr[5], 0x7FF8);
    assert_eq!(
        memory.read_obj::<u64>(GuestAddress(0x7FF8)).unwrap(),
        0x8000
    );
    assert_eq!(
        memory.read_obj::<u64>(GuestAddress(0x7FF0)).unwrap(),
        0x8000
    );
    assert_eq!(
        memory.read_obj::<u64>(GuestAddress(0x7FE8)).unwrap(),
        0x7FF8
    );
    assert_eq!(
        vcpu.jit_mem_trace.as_deref(),
        Some(
            &[
                (1, 0x7FF8, 8, 0x8000),
                (0, 0x7FF8, 8, 0x8000),
                (1, 0x7FF0, 8, 0x8000),
                (1, 0x7FE8, 8, 0x7FF8),
            ][..]
        )
    );
    assert_eq!(
        vcpu.jit_mem_log.as_deref(),
        Some(&[(0x7FF8, 8, 0), (0x7FF0, 8, 0), (0x7FE8, 8, 0)][..])
    );
    let mut records = Vec::new();
    VCpu::drain_mem_records(&mut vcpu, &mut records);
    assert_eq!(
        records
            .iter()
            .map(|record| (record.access, record.addr, record.size, record.value))
            .collect::<Vec<_>>(),
        [
            (MemAccess::Write, 0x7FF8, 8, 0x8000),
            (MemAccess::Read, 0x7FF8, 8, 0x8000),
            (MemAccess::Write, 0x7FF0, 8, 0x8000),
            (MemAccess::Write, 0x7FE8, 8, 0x7FF8),
        ]
    );
}

#[test]
fn enter_helper_covers_both_widths_and_all_nesting_boundaries() {
    let mut cases = 0;
    for width in [2_u32, 8] {
        for nesting in [0_u32, 1, 31] {
            let memory = memory_with_code(&[]);
            let mut vcpu = test_vcpu(memory.clone());
            let old_rsp = 0x9000;
            let old_rbp = 0x7000;
            initialize_display(&memory, old_rbp, width as u8, nesting as u8);
            let mut state = helper_state(&mut vcpu, old_rsp, old_rbp);

            assert_eq!(
                unsafe { rax_jit_enter(&mut state, 0x20, nesting, width, 0) },
                1,
                "W{} nesting={nesting}",
                width * 8
            );
            let frame_pointer = old_rsp - u64::from(width);
            let stack_slots = if nesting == 0 { 1 } else { nesting + 1 };
            assert_eq!(
                state.gpr[4],
                old_rsp - u64::from(width * stack_slots) - 0x20
            );
            assert_eq!(state.gpr[5], frame_pointer);
            let pushed_rbp = memory
                .read_obj::<u16>(GuestAddress(frame_pointer))
                .map(u64::from)
                .unwrap();
            assert_eq!(pushed_rbp, old_rbp & 0xFFFF);
            if width == 8 {
                assert_eq!(
                    memory.read_obj::<u64>(GuestAddress(frame_pointer)).unwrap(),
                    old_rbp
                );
            }
            if nesting != 0 {
                let final_frame_slot = old_rsp - u64::from(width * (nesting + 1));
                let observed = if width == 2 {
                    u64::from(
                        memory
                            .read_obj::<u16>(GuestAddress(final_frame_slot))
                            .unwrap(),
                    )
                } else {
                    memory
                        .read_obj::<u64>(GuestAddress(final_frame_slot))
                        .unwrap()
                };
                assert_eq!(
                    observed,
                    frame_pointer & (if width == 2 { 0xFFFF } else { u64::MAX })
                );
            }
            cases += 1;
        }
    }
    assert_eq!(cases, 6);

    let memory = memory_with_code(&[]);
    let mut vcpu = test_vcpu(memory.clone());
    let old_rbp = 0x1234_5678_9ABC_7000;
    let mut state = helper_state(&mut vcpu, 0x9000, old_rbp);
    assert_eq!(unsafe { rax_jit_enter(&mut state, 0, 0, 2, 0) }, 1);
    assert_eq!(state.gpr[5], 0x1234_5678_9ABC_8FFE);
}

#[test]
fn enter_helper_rejections_are_noncommitting_and_nonobservable() {
    assert_eq!(
        unsafe { rax_jit_enter(std::ptr::null_mut(), 0, 0, 8, 0) },
        0
    );

    let memory = Arc::new(
        GuestMemoryMmap::<()>::from_ranges(&[
            (GuestAddress(0), 0x1000),
            (GuestAddress(0x7000), 0x1000),
        ])
        .unwrap(),
    );
    let mut vcpu = test_vcpu(memory.clone());
    VCpu::set_mem_recording(&mut vcpu, true);
    let trace_sentinel = (0, 0x1234, 1, 0x56);
    let log_sentinel = (0x5678, 1, 0x9A);
    vcpu.jit_mem_trace = Some(vec![trace_sentinel]);
    vcpu.jit_mem_log = Some(vec![log_sentinel]);
    let mut state = helper_state(&mut vcpu, 0x8000, 0x7000);
    let before = state.gpr;

    // The first push is mapped, but the architecturally required final byte
    // write check is not. Native execution must deopt before any observation.
    assert_eq!(unsafe { rax_jit_enter(&mut state, 0x1000, 0, 8, 0) }, 0);
    assert_eq!(state.gpr, before);
    assert_eq!(memory.read_obj::<u64>(GuestAddress(0x7FF8)).unwrap(), 0);
    assert_eq!(vcpu.jit_mem_trace.as_deref(), Some(&[trace_sentinel][..]));
    assert_eq!(vcpu.jit_mem_log.as_deref(), Some(&[log_sentinel][..]));
    let mut records = Vec::new();
    VCpu::drain_mem_records(&mut vcpu, &mut records);
    assert!(
        records.is_empty(),
        "speculative records leaked: {records:?}"
    );

    for (name, allocation, nesting, width, requires_apx) in [
        ("allocation", 0x1_0000, 0, 8, 0),
        ("nesting", 0, 32, 8, 0),
        ("width", 0, 0, 4, 0),
        ("APX field", 0, 0, 8, 2),
        ("APX disabled", 0, 0, 8, 1),
    ] {
        assert_eq!(
            unsafe { rax_jit_enter(&mut state, allocation, nesting, width, requires_apx) },
            0,
            "{name}"
        );
        assert_eq!(state.gpr, before, "{name}");
    }
    state.gpr[4] = 0x0000_8000_0000_0000;
    assert_eq!(unsafe { rax_jit_enter(&mut state, 0, 0, 8, 0) }, 0);
    state.gpr = before;
    vcpu.mmu.mark_code_page(0x7FF8);
    assert_eq!(unsafe { rax_jit_enter(&mut state, 0, 0, 8, 0) }, 0);
    assert_eq!(state.gpr, before);
}

#[test]
fn enter_helper_deopts_before_saturating_trace_or_undo_buffers() {
    const TRACE_SENTINEL: (u8, u64, u8, u64) = (0, 0x1234, 1, 0x56);
    const LOG_SENTINEL: (u64, u8, u64) = (0x5678, 1, 0x9A);

    for (name, fill_trace, fill_log) in [("trace", true, false), ("undo log", false, true)] {
        let memory = memory_with_code(&[]);
        let mut vcpu = test_vcpu(memory.clone());
        vcpu.jit_mem_trace = fill_trace.then(|| vec![TRACE_SENTINEL; JIT_VERIFY_MEM_TRACE_LIMIT]);
        vcpu.jit_mem_log = fill_log.then(|| vec![LOG_SENTINEL; JIT_VERIFY_MEM_LOG_LIMIT]);
        let trace_before = vcpu.jit_mem_trace.clone();
        let log_before = vcpu.jit_mem_log.clone();
        let mut state = helper_state(&mut vcpu, 0x8000, 0x7000);
        let gprs_before = state.gpr;

        assert_eq!(
            unsafe { rax_jit_enter(&mut state, 0, 2, 8, 0) },
            0,
            "{name}"
        );
        assert_eq!(state.gpr, gprs_before, "{name}: GPR state");
        assert_eq!(vcpu.jit_mem_trace, trace_before, "{name}: trace");
        assert_eq!(vcpu.jit_mem_log, log_before, "{name}: undo log");
        assert_eq!(
            memory.read_obj::<u64>(GuestAddress(0x7FF8)).unwrap(),
            0,
            "{name}: speculative store"
        );
    }
}

#[test]
fn enter_helper_rolls_back_page_table_aliases_by_physical_address() {
    const PML4: u64 = 0x1000;
    const PDPT: u64 = 0x2000;
    const PD: u64 = 0x3000;
    const PT: u64 = 0x4000;
    const DATA: u64 = 0x5000;
    const FLAGS: u64 = 0x3; // Present | writable.

    let memory = memory_with_code(&[]);
    for (address, entry) in [
        (PML4, PDPT | FLAGS),
        (PDPT, PD | FLAGS),
        (PD, PT | FLAGS),
        (PT, DATA | FLAGS),
        // Virtual page 7 aliases the PT page itself. ENTER's first stack
        // store at 0x7000 therefore overwrites PT[0].
        (PT + 7 * 8, PT | FLAGS),
    ] {
        memory.write_obj(entry, GuestAddress(address)).unwrap();
    }

    let mut vcpu = test_vcpu(memory.clone());
    vcpu.sregs.cr0 |= 1 << 31;
    vcpu.sregs.cr3 = PML4;
    vcpu.sregs.cr4 |= 1 << 5;
    vcpu.sregs.efer |= 1 << 8;
    VCpu::set_mem_recording(&mut vcpu, true);
    let trace_sentinel = (0, 0xAA, 1, 0x55);
    let log_sentinel = (0xBB, 1, 0x66);
    vcpu.jit_mem_trace = Some(vec![trace_sentinel]);
    vcpu.jit_mem_log = Some(vec![log_sentinel]);
    let mut state = helper_state(&mut vcpu, 0x7008, 0);
    let before = state.gpr;

    // allocation=0x7000 makes the final write probe target virtual page 0.
    // The first store has just cleared its PTE, so the helper must restore the
    // original physical PT entry and request exact direct replay.
    assert_eq!(unsafe { rax_jit_enter(&mut state, 0x7000, 0, 8, 0) }, 0);
    assert_eq!(state.gpr, before);
    assert_eq!(
        memory.read_obj::<u64>(GuestAddress(PT)).unwrap(),
        DATA | FLAGS
    );
    assert_eq!(vcpu.jit_mem_trace.as_deref(), Some(&[trace_sentinel][..]));
    assert_eq!(vcpu.jit_mem_log.as_deref(), Some(&[log_sentinel][..]));
    let mut records = Vec::new();
    VCpu::drain_mem_records(&mut vcpu, &mut records);
    assert!(
        records.is_empty(),
        "rolled-back alias records leaked: {records:?}"
    );
}

fn run_direct_enter(vcpu: &mut X86_64Vcpu, frontier: u64) {
    assert!(vcpu.step().expect("direct ENTER").is_none());
    assert_eq!(vcpu.regs.rip, frontier);
}

#[test]
fn native_enter_matches_direct_across_prefix_width_nesting_and_apx_forms() {
    for (name, instruction, nesting, width, apx, high_rbp) in [
        ("default W64", &[0xC8, 0x20, 0, 0][..], 0, 8, false, false),
        ("66 W16", &[0x66, 0xC8, 0x10, 0, 0], 0, 2, false, true),
        (
            "66 REX.W W64",
            &[0x66, 0x48, 0xC8, 0, 0, 3],
            3,
            8,
            false,
            false,
        ),
        (
            "REX then 66 W16",
            &[0x48, 0x66, 0xC8, 0, 0, 1],
            1,
            2,
            false,
            false,
        ),
        ("REX2 W64", &[0xD5, 0x00, 0xC8, 0, 0, 1], 1, 8, true, false),
    ] {
        let mut code = instruction.to_vec();
        let frontier = code.len() as u64;
        code.push(0xF4);
        let direct_memory = memory_with_code(&code);
        let native_memory = memory_with_code(&code);
        let mut direct = test_vcpu(direct_memory.clone());
        let mut native = test_vcpu(native_memory.clone());
        for candidate in [&mut direct, &mut native] {
            candidate.set_apx_enabled(apx);
            if high_rbp {
                candidate.regs.rbp = 0x1234_5678_9ABC_7000;
            }
        }
        if !high_rbp {
            initialize_display(&direct_memory, direct.regs.rbp, width, nesting);
            initialize_display(&native_memory, native.regs.rbp, width, nesting);
        }

        run_direct_enter(&mut direct, frontier);
        let region = native
            .jit_compile_region()
            .unwrap_or_else(|error| panic!("{name}: compile failed: {error}"))
            .unwrap_or_else(|| panic!("{name}: not native eligible"));
        native.jit_run_region_native(&region);
        assert_eq!(gprs(&native.regs), gprs(&direct.regs), "{name}: GPRs");
        assert_eq!(native.regs.rflags, direct.regs.rflags, "{name}: RFLAGS");
        assert_eq!(native.regs.rip, direct.regs.rip, "{name}: RIP");
        let mut direct_stack = vec![0; 0x3100];
        let mut native_stack = vec![0; 0x3100];
        direct_memory
            .read_slice(&mut direct_stack, GuestAddress(0x6000))
            .unwrap();
        native_memory
            .read_slice(&mut native_stack, GuestAddress(0x6000))
            .unwrap();
        assert_eq!(native_stack, direct_stack, "{name}: stack/display memory");
    }
}

#[test]
fn native_enter_fault_deopts_at_exact_frontier_and_compatibility_is_rejected() {
    let code = [0xC8, 0x00, 0x10, 0x00, 0xF4];
    let memory = Arc::new(
        GuestMemoryMmap::<()>::from_ranges(&[
            (GuestAddress(0), 0x1000),
            (GuestAddress(0x7000), 0x1000),
        ])
        .unwrap(),
    );
    memory.write_slice(&code, GuestAddress(0)).unwrap();
    let mut native = test_vcpu(memory.clone());
    native.regs.rsp = 0x8000;
    let before = native.regs.clone();
    let region = native
        .jit_compile_region()
        .expect("compile final-probe-fault ENTER")
        .expect("dynamic final probe must remain native eligible");
    native.jit_run_region_native(&region);
    assert_eq!(gprs(&native.regs), gprs(&before));
    assert_eq!(native.regs.rflags, before.rflags);
    assert_eq!(native.regs.rip, 0);
    assert_eq!(memory.read_obj::<u64>(GuestAddress(0x7FF8)).unwrap(), 0);

    assert!(
        native.step().is_err(),
        "direct replay must deliver the final write-check fault"
    );
    assert_eq!(native.regs.rsp, before.rsp);
    assert_eq!(native.regs.rbp, before.rbp);
    assert_eq!(
        memory.read_obj::<u64>(GuestAddress(0x7FF8)).unwrap(),
        before.rbp
    );

    let compatibility_memory = memory_with_code(&code);
    let mut compatibility = test_vcpu(compatibility_memory);
    compatibility.sregs.cs.l = false;
    assert!(
        compatibility.jit_compile_region().unwrap().is_none(),
        "compatibility-mode ENTER must remain on the direct path"
    );
}

#[test]
fn verified_enter_restores_and_replays_the_native_stack_write() {
    let code = [0xC8, 0x20, 0, 0, 0xF4];
    let memory = memory_with_code(&code);
    let mut vcpu = test_vcpu(memory.clone());
    let region = vcpu
        .jit_compile_region()
        .expect("compile verified ENTER")
        .expect("ENTER must be native eligible");
    vcpu.jit_run_region_verified(&region);
    assert_eq!(vcpu.regs.rsp, 0x8FD8);
    assert_eq!(vcpu.regs.rbp, 0x8FF8);
    assert_eq!(
        memory.read_obj::<u64>(GuestAddress(0x8FF8)).unwrap(),
        0x7000
    );
    assert_eq!(vcpu.regs.rip, 4);
}
