//! Native x86-64 JIT differentials for scalar instructions.

use super::*;
use crate::vm::vcpu::VCpu;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn long_mode_vcpu(memory: Arc<GuestMemoryMmap>) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.regs.rflags = 0x246;
    vcpu.set_jit_mem(false);
    vcpu.set_jit_call(false);
    vcpu
}

#[test]
fn native_lea_applies_encoded_destination_width() {
    for (name, instruction, rax, rdx, expected_rdx) in [
        (
            "lea edx,[rax+1]",
            &[0x8d, 0x50, 0x01][..],
            0x0000_0000_ffff_ffff,
            u64::MAX,
            0,
        ),
        (
            "lea dx,[rax+1]",
            &[0x66, 0x8d, 0x50, 0x01][..],
            0x0000_0000_0000_ffff,
            0x1234_5678_9abc_ffff,
            0x1234_5678_9abc_0000,
        ),
        (
            "lea rdx,[rax+1]",
            &[0x48, 0x8d, 0x50, 0x01][..],
            u64::MAX,
            0x1234_5678_9abc_def0,
            0,
        ),
    ] {
        let memory =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
        let mut code = instruction.to_vec();
        code.extend_from_slice(&[0xeb, 0x00, 0xf4]); // jmp next; hlt frontier
        memory.write_slice(&code, GuestAddress(0)).unwrap();

        let mut direct = long_mode_vcpu(memory.clone());
        let mut native = long_mode_vcpu(memory);
        for vcpu in [&mut direct, &mut native] {
            vcpu.regs.rax = rax;
            vcpu.regs.rdx = rdx;
        }

        assert!(direct.step().unwrap().is_none(), "{name}: direct execution");
        assert_eq!(direct.regs.rdx, expected_rdx, "{name}: direct oracle");

        let region = native
            .jit_compile_region()
            .unwrap_or_else(|error| panic!("{name}: compile failed: {error}"))
            .unwrap_or_else(|| panic!("{name}: region was not native eligible"));
        native.jit_run_region_native(&region);

        assert_eq!(native.regs.rdx, direct.regs.rdx, "{name}: destination");
        assert_eq!(native.regs.rax, direct.regs.rax, "{name}: address source");
        assert_eq!(native.regs.rflags, direct.regs.rflags, "{name}: flags");
        assert_eq!(
            native.regs.rip,
            code.len() as u64 - 1,
            "{name}: HLT frontier"
        );
    }
}

#[test]
fn jit_rejects_constant_folded_unencodable_w64_alu_immediate() {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    // mov eax,80000000h; add rbx,rax; jmp next; hlt. O2 propagates RAX into
    // ADD as +80000000h, which cannot use x86-64's sign-extending imm32 form.
    memory
        .write_slice(
            &[
                0xb8, 0x00, 0x00, 0x00, 0x80, 0x48, 0x01, 0xc3, 0xeb, 0x00, 0xf4,
            ],
            GuestAddress(0),
        )
        .unwrap();
    let mut vcpu = long_mode_vcpu(memory);
    vcpu.regs.rbx = 0xffff_8880_0483_f000;

    assert!(
        vcpu.jit_compile_region().unwrap().is_none(),
        "native admission must fail closed for an unencodable W64 immediate"
    );
}

#[test]
fn native_undefined_bswap_r16_matches_direct_and_continues_in_the_region() {
    const HLT_PC: u64 = 17;
    const CODE: &[u8] = &[
        0x66, 0x0F, 0xC8, // architecturally undefined BSWAP AX: deterministic identity
        0x66, 0x41, 0x0F, 0xCF, // architecturally undefined BSWAP R15W: identity
        0x66, 0xD5, 0x90, 0xC8, // architecturally undefined BSWAP R16W: APX identity
        0x48, 0x83, 0xC3, 0x01, // add rbx,1
        0xEB, 0x00, // jmp hlt
        0xF4,
    ];

    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(CODE, GuestAddress(0)).unwrap();
    let mut direct = long_mode_vcpu(memory.clone());
    let mut native = long_mode_vcpu(memory);
    for vcpu in [&mut direct, &mut native] {
        vcpu.set_apx_enabled(true);
        vcpu.regs.rax = 0x0123_4567_89AB_CDEF;
        vcpu.regs.rbx = 0x1111_2222_3333_4444;
        vcpu.regs.r15 = 0xFEDC_BA98_7654_3210;
        vcpu.regs.r16 = 0xA5A5_5A5A_1357_2468;
    }

    for _ in 0..5 {
        assert!(direct.step().expect("direct BSWAP r16 sequence").is_none());
    }
    assert_eq!(direct.regs.rip, HLT_PC);

    let region = native
        .jit_compile_region()
        .expect("compile BSWAP r16 continuation")
        .expect("BSWAP r16 identity profile must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(
        serde_json::to_value(native.get_regs().unwrap()).unwrap(),
        serde_json::to_value(direct.get_regs().unwrap()).unwrap()
    );
    assert_eq!(native.regs.rip, HLT_PC);
    assert_eq!(native.regs.rbx, 0x1111_2222_3333_4445);
}

#[test]
fn native_undefined_rex2_bswap_r16_rechecks_apx_without_commit() {
    const CODE: &[u8] = &[
        0x66, 0xD5, 0x90, 0xC8, // architecturally undefined BSWAP R16W
        0x48, 0x83, 0xC3, 0x01, // add rbx,1
        0xEB, 0x00, // jmp hlt
        0xF4,
    ];

    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(CODE, GuestAddress(0)).unwrap();
    let mut vcpu = long_mode_vcpu(memory);
    vcpu.regs.rbx = 0x1111_2222_3333_4444;
    vcpu.regs.r16 = 0xA5A5_5A5A_1357_2468;
    vcpu.set_apx_enabled(true);
    let region = vcpu
        .jit_compile_region()
        .expect("compile guarded BSWAP r16")
        .expect("guard-only BSWAP r16 must be native eligible");

    vcpu.set_apx_enabled(false);
    let before = serde_json::to_value(vcpu.get_regs().unwrap()).unwrap();
    vcpu.jit_run_region_native(&region);

    assert_eq!(
        serde_json::to_value(vcpu.get_regs().unwrap()).unwrap(),
        before
    );
    assert_eq!(vcpu.regs.rip, 0);
    assert_eq!(vcpu.regs.rbx, 0x1111_2222_3333_4444);
    assert_eq!(vcpu.regs.r16, 0xA5A5_5A5A_1357_2468);
}

#[test]
fn native_rex2_bswap_w64_matches_direct_extended_destination() {
    const HLT_PC: u64 = 5;
    const CODE: &[u8] = &[
        0xD5, 0x98, 0xC8, // BSWAP R16
        0xEB, 0x00, // jmp hlt
        0xF4,
    ];

    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(CODE, GuestAddress(0)).unwrap();
    let mut direct = long_mode_vcpu(memory.clone());
    let mut native = long_mode_vcpu(memory);
    for vcpu in [&mut direct, &mut native] {
        vcpu.set_apx_enabled(true);
        vcpu.regs.rax = 0xA5A5_5A5A_1357_2468;
        vcpu.regs.r16 = 0x0102_0304_0506_0708;
    }

    for _ in 0..2 {
        assert!(direct.step().expect("direct REX2 BSWAP R16").is_none());
    }
    assert_eq!(direct.regs.rip, HLT_PC);

    let region = native
        .jit_compile_region()
        .expect("compile REX2 BSWAP R16")
        .expect("REX2 BSWAP R16 must be native eligible");
    native.jit_run_region_native(&region);

    assert_eq!(
        serde_json::to_value(native.get_regs().unwrap()).unwrap(),
        serde_json::to_value(direct.get_regs().unwrap()).unwrap()
    );
    assert_eq!(native.regs.r16, 0x0807_0605_0403_0201);
    assert_eq!(native.regs.rax, 0xA5A5_5A5A_1357_2468);
}
