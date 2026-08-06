//! Native scalar port-I/O helper, handoff, and fault-deoptimization tests.

use super::*;
use crate::error::Error;
use crate::smir::lower::runtime::GuestRegs;
use crate::vm::vcpu::{Segment, VCpu, VcpuExit};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const MEMORY_BYTES: usize = 0x3_0000;
const TSS_BASE: u64 = 0x1_0000;
const TSS_IO_MAP_BASE_OFFSET: u64 = 0x66;
const IO_MAP_BASE: u16 = 0x68;
const IO_MAP_BYTES: u64 = 65_536 / 8;
const TSS_LIMIT: u32 = IO_MAP_BASE as u32 + IO_MAP_BYTES as u32;

fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), MEMORY_BYTES)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

fn vcpu_with_memory(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.cr0 = 1;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
    vcpu.regs.rip = 0;
    vcpu.regs.rax = 0x0123_4567_89AB_CDEF;
    vcpu.regs.rbx = 9;
    vcpu.regs.rcx = 0x1111_2222_3333_4444;
    vcpu.regs.rdx = 0xAAAA_BBBB_CCCC_03F8;
    vcpu.regs.rsi = 0x5555_6666_7777_8888;
    vcpu.regs.rdi = 0x9999_AAAA_BBBB_CCCC;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.r8 = 0x0808_0808_0808_0808;
    vcpu.regs.r15 = 0x1515_1515_1515_1515;
    vcpu.regs.r16 = 0x1616_1616_1616_1616;
    vcpu.regs.r31 = 0x3131_3131_3131_3131;
    vcpu.regs.rflags = 0x2 | flags::bits::CF | flags::bits::DF | flags::bits::OF;
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

fn test_vcpu_and_memory(code: &[u8]) -> (X86_64Vcpu, Arc<GuestMemoryMmap>) {
    let memory = memory_with_code(code);
    (vcpu_with_memory(memory.clone()), memory)
}

fn test_vcpu(code: &[u8]) -> X86_64Vcpu {
    test_vcpu_and_memory(code).0
}

fn scalar_state(vcpu: &mut X86_64Vcpu) -> Vec<u64> {
    vcpu.materialize_flags();
    let regs = &vcpu.regs;
    vec![
        regs.rax,
        regs.rbx,
        regs.rcx,
        regs.rdx,
        regs.rsi,
        regs.rdi,
        regs.rsp,
        regs.rbp,
        regs.r8,
        regs.r9,
        regs.r10,
        regs.r11,
        regs.r12,
        regs.r13,
        regs.r14,
        regs.r15,
        regs.r16,
        regs.r17,
        regs.r18,
        regs.r19,
        regs.r20,
        regs.r21,
        regs.r22,
        regs.r23,
        regs.r24,
        regs.r25,
        regs.r26,
        regs.r27,
        regs.r28,
        regs.r29,
        regs.r30,
        regs.r31,
        regs.rip,
        regs.rflags,
    ]
}

fn assert_same_io_exit(name: &str, native: &VcpuExit, direct: &VcpuExit) {
    match (native, direct) {
        (
            VcpuExit::IoIn {
                port: native_port,
                size: native_size,
            },
            VcpuExit::IoIn {
                port: direct_port,
                size: direct_size,
            },
        ) => {
            assert_eq!(native_port, direct_port, "{name}: input port");
            assert_eq!(native_size, direct_size, "{name}: input width");
        }
        (
            VcpuExit::IoOut {
                port: native_port,
                data: native_data,
            },
            VcpuExit::IoOut {
                port: direct_port,
                data: direct_data,
            },
        ) => {
            assert_eq!(native_port, direct_port, "{name}: output port");
            assert_eq!(native_data, direct_data, "{name}: output bytes");
        }
        _ => panic!("{name}: native={native:?}, direct={direct:?}"),
    }
}

#[test]
fn native_scalar_io_all_opcodes_widths_and_prefix_orders_match_direct() {
    let cases: &[(&str, &[u8], u16, u8, bool)] = &[
        ("IN AL,imm8", &[0xE4, 0x80], 0x80, 1, false),
        ("IN EAX,imm8", &[0xE5, 0x81], 0x81, 4, false),
        ("OUT imm8,AL", &[0xE6, 0xFE], 0xFE, 1, true),
        ("OUT imm8,EAX", &[0xE7, 0xFF], 0xFF, 4, true),
        ("IN AL,DX", &[0xEC], 0x03F8, 1, false),
        ("IN EAX,DX", &[0xED], 0x03F8, 4, false),
        ("OUT DX,AL", &[0xEE], 0x03F8, 1, true),
        ("OUT DX,EAX", &[0xEF], 0x03F8, 4, true),
        ("IN AX,imm8", &[0x66, 0xE5, 0x82], 0x82, 2, false),
        ("OUT imm8,AX", &[0x66, 0xE7, 0x83], 0x83, 2, true),
        ("IN AX,DX", &[0x66, 0xED], 0x03F8, 2, false),
        ("OUT DX,AX", &[0x66, 0xEF], 0x03F8, 2, true),
        ("66 REX.W IN EAX", &[0x66, 0x48, 0xE5, 0x84], 0x84, 4, false),
        ("REX.W 66 IN AX", &[0x48, 0x66, 0xE5, 0x85], 0x85, 2, false),
        (
            "REPNE REX.W 66 OUT AX",
            &[0xF2, 0x48, 0x66, 0xE7, 0x86],
            0x86,
            2,
            true,
        ),
        (
            "REP 66 REX.W OUT EAX",
            &[0xF3, 0x66, 0x48, 0xEF],
            0x03F8,
            4,
            true,
        ),
    ];

    for &(name, instruction, expected_port, expected_size, output) in cases {
        // ADD EBX,1; scalar I/O; INC ECX; HLT. The instruction following the
        // external exit must not execute in the same native invocation.
        let mut code = vec![0x83, 0xC3, 0x01];
        code.extend_from_slice(instruction);
        code.extend_from_slice(&[0xFF, 0xC1, 0xF4]);
        let mut direct = test_vcpu(&code);
        let mut native = test_vcpu(&code);

        assert!(direct.step().unwrap().is_none(), "{name}: direct prefix");
        let direct_exit = direct
            .step()
            .unwrap_or_else(|error| panic!("{name}: direct I/O: {error:?}"))
            .expect("scalar I/O must exit");
        let region = native
            .jit_compile_region()
            .unwrap_or_else(|error| panic!("{name}: native compile: {error:?}"))
            .expect("scalar I/O region must be native eligible");
        assert!(region.uses_io, "{name}: region metadata");
        native.jit_run_region_native(&region);
        let native_exit = native
            .jit_callout_exit
            .take()
            .unwrap_or_else(|| panic!("{name}: native I/O request"));

        assert_same_io_exit(name, &native_exit, &direct_exit);
        match &native_exit {
            VcpuExit::IoIn { port, size } => {
                assert!(!output, "{name}: direction");
                assert_eq!((*port, *size), (expected_port, expected_size), "{name}");
            }
            VcpuExit::IoOut { port, data } => {
                assert!(output, "{name}: direction");
                assert_eq!(*port, expected_port, "{name}: port");
                assert_eq!(data.len(), usize::from(expected_size), "{name}: width");
            }
            _ => unreachable!("matched scalar I/O exits"),
        }
        assert_eq!(native.regs.rbx, 10, "{name}: native prefix committed");
        assert_eq!(
            native.regs.rcx, 0x1111_2222_3333_4444,
            "{name}: following INC must not execute"
        );
        assert_eq!(
            scalar_state(&mut native),
            scalar_state(&mut direct),
            "{name}"
        );

        if !output {
            let input = [0x5A, 0xA5, 0xC3, 0x3C];
            native.complete_io_in(&input);
            direct.complete_io_in(&input);
            assert_eq!(
                scalar_state(&mut native),
                scalar_state(&mut direct),
                "{name}: input"
            );
            let expected_rax = match expected_size {
                1 => 0x0123_4567_89AB_CD5A,
                2 => 0x0123_4567_89AB_A55A,
                4 => 0x0000_0000_3CC3_A55A,
                _ => unreachable!(),
            };
            assert_eq!(native.regs.rax, expected_rax, "{name}: completed input");
        }
    }
}

#[test]
fn native_scalar_io_is_one_request_per_entry_and_verify_does_not_replay_it() {
    // OUT 80h,AL; OUT 81h,AL; HLT.
    let mut vcpu = test_vcpu(&[0xE6, 0x80, 0xE6, 0x81, 0xF4]);
    for (entry, next, port) in [(0, 2, 0x80), (2, 4, 0x81)] {
        assert_eq!(vcpu.regs.rip, entry);
        let region = vcpu
            .jit_compile_region()
            .unwrap()
            .expect("entry scalar I/O must compile");
        assert!(region.uses_io);
        vcpu.jit_run_region_verified(&region);
        assert_eq!(vcpu.regs.rip, next);
        assert!(matches!(
            vcpu.jit_callout_exit.take(),
            Some(VcpuExit::IoOut { port: actual, data })
                if actual == port && data == [0xEF]
        ));
    }
}

#[test]
fn native_scalar_io_frontier_preserves_state_overwritten_after_the_exit() {
    // MOV EBX,1; CMP EAX,ECX; OUT 80h,AL; MOV EBX,2; CMP EBX,ECX; HLT.
    let code = [
        0xBB, 0x01, 0x00, 0x00, 0x00, 0x39, 0xC8, 0xE6, 0x80, 0xBB, 0x02, 0x00, 0x00, 0x00, 0x39,
        0xCB, 0xF4,
    ];
    let mut direct = test_vcpu(&code);
    let mut native = test_vcpu(&code);
    assert!(direct.step().unwrap().is_none());
    assert!(direct.step().unwrap().is_none());
    let direct_exit = direct.step().unwrap().expect("direct OUT");
    let region = native
        .jit_compile_region()
        .unwrap()
        .expect("O2 scalar-I/O frontier region");
    native.jit_run_region_native(&region);
    let native_exit = native.jit_callout_exit.take().expect("native OUT");

    assert_same_io_exit("O2 frontier liveness", &native_exit, &direct_exit);
    assert_eq!(native.regs.rip, 9);
    assert_eq!(
        native.regs.rbx, 1,
        "post-exit MOV must not kill pre-exit MOV"
    );
    assert_eq!(scalar_state(&mut native), scalar_state(&mut direct));
}

fn configure_valid_tss(vcpu: &mut X86_64Vcpu, memory: &GuestMemoryMmap) {
    vcpu.sregs.cs.selector = 3;
    vcpu.sregs.tr = Segment {
        base: TSS_BASE,
        limit: TSS_LIMIT,
        selector: 0x28,
        type_: 0x9,
        present: true,
        s: false,
        ..Segment::default()
    };
    memory
        .write_slice(
            &IO_MAP_BASE.to_le_bytes(),
            GuestAddress(TSS_BASE + TSS_IO_MAP_BASE_OFFSET),
        )
        .unwrap();
    memory
        .write_slice(
            &[0xFF],
            GuestAddress(TSS_BASE + u64::from(IO_MAP_BASE) + IO_MAP_BYTES),
        )
        .unwrap();
}

fn set_io_bitmap_bit(memory: &GuestMemoryMmap, port: u16, denied: bool) {
    let address = GuestAddress(TSS_BASE + u64::from(IO_MAP_BASE) + u64::from(port >> 3));
    let mut byte = [0_u8; 1];
    memory.read_slice(&mut byte, address).unwrap();
    let mask = 1_u8 << (port & 7);
    if denied {
        byte[0] |= mask;
    } else {
        byte[0] &= !mask;
    }
    memory.write_slice(&byte, address).unwrap();
}

#[test]
fn native_scalar_io_tss_permission_is_dynamic_and_denial_replays_precisely() {
    // ADD EBX,1; IN AL,80h; INC ECX; HLT.
    let code = [0x83, 0xC3, 0x01, 0xE4, 0x80, 0xFF, 0xC1, 0xF4];
    let (mut direct, direct_memory) = test_vcpu_and_memory(&code);
    let (mut native, native_memory) = test_vcpu_and_memory(&code);
    configure_valid_tss(&mut direct, &direct_memory);
    configure_valid_tss(&mut native, &native_memory);

    assert!(direct.step().unwrap().is_none());
    let direct_exit = direct.step().unwrap().expect("allowed direct IN");
    let region = native
        .jit_compile_region()
        .unwrap()
        .expect("CPL3 scalar I/O remains dynamically eligible");
    native.jit_run_region_native(&region);
    let native_exit = native.jit_callout_exit.take().expect("allowed native IN");
    assert_same_io_exit("allowed TSS bitmap", &native_exit, &direct_exit);
    assert_eq!(scalar_state(&mut native), scalar_state(&mut direct));

    let (mut denied_direct, denied_direct_memory) = test_vcpu_and_memory(&code);
    let (mut denied_native, denied_native_memory) = test_vcpu_and_memory(&code);
    configure_valid_tss(&mut denied_direct, &denied_direct_memory);
    configure_valid_tss(&mut denied_native, &denied_native_memory);
    let denied_region = denied_native
        .jit_compile_region()
        .unwrap()
        .expect("permission is checked at execution, not compilation");
    set_io_bitmap_bit(&denied_direct_memory, 0x80, true);
    set_io_bitmap_bit(&denied_native_memory, 0x80, true);

    assert!(denied_direct.step().unwrap().is_none());
    denied_native.jit_run_region_native(&denied_region);
    assert!(denied_native.jit_callout_exit.is_none());
    assert_eq!(denied_native.regs.rip, 3, "precise direct-replay frontier");
    assert_eq!(
        scalar_state(&mut denied_native),
        scalar_state(&mut denied_direct),
        "only the native prefix may commit"
    );
    assert!(matches!(
        denied_direct.step(),
        Err(Error::GeneralProtection { error_code: 0 })
    ));
    assert!(matches!(
        denied_native.step(),
        Err(Error::GeneralProtection { error_code: 0 })
    ));
    let denied_rax = denied_native.regs.rax;
    denied_native.complete_io_in(&[0x5A]);
    assert_eq!(denied_native.regs.rax, denied_rax, "denial stages no input");
}

fn paged_missing_bitmap_vcpu() -> X86_64Vcpu {
    const PML4: u64 = 0x1000;
    const PDPT: u64 = 0x2000;
    const PD: u64 = 0x3000;
    const PT: u64 = 0x4000;
    const CODE_AND_TSS_PAGE: u64 = 0x6000;
    const PAGE_FLAGS: u64 = 0x7;
    const PAGED_TSS_BASE: u64 = 0x0F98;

    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x1_0000)]).unwrap());
    for (address, entry) in [
        (PML4, PDPT | PAGE_FLAGS),
        (PDPT, PD | PAGE_FLAGS),
        (PD, PT | PAGE_FLAGS),
        (PT, CODE_AND_TSS_PAGE | PAGE_FLAGS),
    ] {
        memory
            .write_slice(&entry.to_le_bytes(), GuestAddress(address))
            .unwrap();
    }
    memory
        .write_slice(
            &[0x83, 0xC3, 0x01, 0xE4, 0x80, 0xF4],
            GuestAddress(CODE_AND_TSS_PAGE),
        )
        .unwrap();
    memory
        .write_slice(
            &IO_MAP_BASE.to_le_bytes(),
            GuestAddress(CODE_AND_TSS_PAGE + 0x0FFE),
        )
        .unwrap();

    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.cr0 = 0x8000_0001;
    vcpu.sregs.cr3 = PML4;
    vcpu.sregs.cr4 = 1 << 5;
    vcpu.sregs.efer = 0x500;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 3;
    vcpu.sregs.tr = Segment {
        base: PAGED_TSS_BASE,
        limit: 0x80,
        selector: 0x28,
        type_: 0xB,
        present: true,
        ..Segment::default()
    };
    vcpu.regs.rbx = 9;
    vcpu.regs.rflags = 0x2;
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

#[test]
fn native_scalar_io_deoptimizes_before_faulting_tss_bitmap_fetch() {
    let mut vcpu = paged_missing_bitmap_vcpu();
    let region = vcpu
        .jit_compile_region()
        .unwrap()
        .expect("faulting TSS bitmap remains dynamically eligible");
    vcpu.jit_run_region_native(&region);

    assert!(vcpu.jit_callout_exit.is_none());
    assert_eq!(vcpu.regs.rip, 3);
    assert_eq!(vcpu.regs.rbx, 10, "native prefix committed once");
    let before = scalar_state(&mut vcpu);
    let result = vcpu.step();
    assert!(
        matches!(
            &result,
            Err(Error::PageFault {
                vaddr: 0x1010,
                error_code: 0,
            })
        ),
        "{result:?}"
    );
    assert_eq!(
        scalar_state(&mut vcpu),
        before,
        "faulting IN is noncommitting"
    );
}

fn helper_state(vcpu: &mut X86_64Vcpu) -> GuestRegs {
    GuestRegs {
        ctx: vcpu as *mut X86_64Vcpu as u64,
        cr0: vcpu.sregs.cr0,
        cr3: vcpu.sregs.cr3,
        cr4: vcpu.sregs.cr4,
        efer: vcpu.sregs.efer,
        cpl: u64::from(vcpu.sregs.cs.selector & 3),
        interrupt_flags: vcpu.regs.rflags,
        ..Default::default()
    }
}

#[test]
fn scalar_io_helper_arguments_and_single_request_channel_fail_closed() {
    assert_eq!(unsafe { rax_jit_io(std::ptr::null_mut(), 0, 1, 0) }, 0);

    let mut vcpu = test_vcpu(&[]);
    let mut state = helper_state(&mut vcpu);
    state.gpr[0] = 0x0123_4567_89AB_CDEF;
    assert_eq!(unsafe { rax_jit_io(&mut state, 0xFFFF, 1, 1) }, 1);
    assert_eq!(state.take_io_request(), Some((0xFFFF, 1, true, 0xEF)));

    assert_eq!(unsafe { rax_jit_io(&mut state, 0x80, 4, 0) }, 1);
    assert_eq!(state.take_io_request(), Some((0x80, 4, false, 0)));

    for (name, port, size, output) in [
        ("wide port", 0x1_0000, 1, 0),
        ("zero width", 0, 0, 0),
        ("three-byte width", 0, 3, 0),
        ("invalid direction", 0, 1, 2),
    ] {
        let mut invalid = helper_state(&mut vcpu);
        assert_eq!(
            unsafe { rax_jit_io(&mut invalid, port, size, output) },
            0,
            "{name}"
        );
        assert_eq!(invalid.io_request, 0, "{name}");
    }

    let mut null_context = GuestRegs::default();
    assert_eq!(unsafe { rax_jit_io(&mut null_context, 0, 1, 0) }, 0);
    assert_eq!(null_context.io_request, 0);

    let mut invalid_cpl = helper_state(&mut vcpu);
    invalid_cpl.cpl = 4;
    assert_eq!(unsafe { rax_jit_io(&mut invalid_cpl, 0, 1, 0) }, 0);
    assert_eq!(invalid_cpl.io_request, 0);

    let mut occupied = helper_state(&mut vcpu);
    occupied.io_request = 1 | (1 << 16);
    assert_eq!(unsafe { rax_jit_io(&mut occupied, 0x80, 1, 0) }, 0);
    assert_eq!(occupied.io_request, 1 | (1 << 16));
}
