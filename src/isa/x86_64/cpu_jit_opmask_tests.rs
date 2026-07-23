//! Native x86-64 JIT tests for VEX-encoded AVX-512 opmask instructions.

use super::*;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

fn host_has_opmask_state() -> bool {
    std::is_x86_feature_detected!("avx512f") && std::is_x86_feature_detected!("avx512bw")
}

fn test_vcpu_with_mem() -> (X86_64Vcpu, Arc<GuestMemoryMmap>) {
    let mem = Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    (X86_64Vcpu::new(0, mem.clone()), mem)
}

fn configure_long_mode(vcpu: &mut X86_64Vcpu, jit_mem: bool) {
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.regs.rip = 0;
    vcpu.set_jit_mem(jit_mem);
    vcpu.set_jit_call(false);
}

#[test]
fn jit_compiles_and_executes_full_width_opmask_registers_and_flags() {
    if !host_has_opmask_state() {
        return;
    }

    let (mut vcpu, mem) = test_vcpu_with_mem();
    // kmovq k1,rax; kmovq k2,rbx; kxorq k3,k1,k2;
    // kshiftlq k4,k3,17; kmovq r9,k4; ktestq k3,k3; jmp next; ret.
    let code = [
        0xC4, 0xE1, 0xFB, 0x92, 0xC8, 0xC4, 0xE1, 0xFB, 0x92, 0xD3, 0xC4, 0xE1, 0xF4, 0x47, 0xDA,
        0xC4, 0xE3, 0xF9, 0x33, 0xE3, 0x11, 0xC4, 0x61, 0xFB, 0x93, 0xCC, 0xC4, 0xE1, 0xF8, 0x99,
        0xDB, 0xEB, 0x00, 0xC3,
    ];
    mem.write_slice(&code, GuestAddress(0)).unwrap();
    configure_long_mode(&mut vcpu, false);
    vcpu.regs.rax = 0x0123_4567_89AB_CDEF;
    vcpu.regs.rbx = 0xF0E1_D2C3_B4A5_9687;
    vcpu.regs.r9 = 0xDEAD_BEEF_DEAD_BEEF;
    let initial_rflags = 0xED7;
    vcpu.regs.rflags = initial_rflags;
    vcpu.regs.k = std::array::from_fn(|index| 0xA500_0000_0000_0000 | index as u64);
    let original_k = vcpu.regs.k;

    let region = vcpu
        .jit_compile_region()
        .expect("compile opmask register region")
        .expect("full-width opmask register forms should be JIT eligible");
    assert!(region.uses_vector);
    assert!(!region.uses_xmm_state);
    assert!(!region.narrow_vector_opmasks);

    vcpu.jit_run_region_native(&region);

    let xor = 0x0123_4567_89AB_CDEF_u64 ^ 0xF0E1_D2C3_B4A5_9687;
    let shifted = xor << 17;
    assert_eq!(vcpu.regs.k[1], 0x0123_4567_89AB_CDEF);
    assert_eq!(vcpu.regs.k[2], 0xF0E1_D2C3_B4A5_9687);
    assert_eq!(vcpu.regs.k[3], xor);
    assert_eq!(vcpu.regs.k[4], shifted);
    for index in [0, 5, 6, 7] {
        assert_eq!(vcpu.regs.k[index], original_k[index], "K{index}");
    }
    assert_eq!(vcpu.regs.r9, shifted);
    const STATUS: u64 = flags::bits::CF
        | flags::bits::PF
        | flags::bits::AF
        | flags::bits::ZF
        | flags::bits::SF
        | flags::bits::OF;
    assert_eq!(
        vcpu.regs.rflags,
        (initial_rflags & !STATUS) | flags::bits::CF
    );
    assert_eq!(vcpu.regs.rip, code.len() as u64 - 1);
}

#[test]
fn jit_opmask_kmovq_state_backs_rsp_and_rbp_in_both_directions() {
    if !host_has_opmask_state() {
        return;
    }

    let (mut vcpu, mem) = test_vcpu_with_mem();
    // kmovq k1,rsp; kmovq k2,rbp; kmovq rbp,k1; kmovq rsp,k2;
    // kmovq k3,rsp; kmovq k4,rbp; jmp next; ret.
    let code = [
        0xC4, 0xE1, 0xFB, 0x92, 0xCC, 0xC4, 0xE1, 0xFB, 0x92, 0xD5, 0xC4, 0xE1, 0xFB, 0x93, 0xE9,
        0xC4, 0xE1, 0xFB, 0x93, 0xE2, 0xC4, 0xE1, 0xFB, 0x92, 0xDC, 0xC4, 0xE1, 0xFB, 0x92, 0xE5,
        0xEB, 0x00, 0xC3,
    ];
    mem.write_slice(&code, GuestAddress(0)).unwrap();
    configure_long_mode(&mut vcpu, false);
    let original_rsp = 0xFEDC_BA98_7654_3210;
    let original_rbp = 0x0123_4567_89AB_CDEF;
    vcpu.regs.rsp = original_rsp;
    vcpu.regs.rbp = original_rbp;
    vcpu.regs.rflags = 0x246;
    vcpu.regs.k = std::array::from_fn(|index| 0xB600_0000_0000_0000 | index as u64);
    let original_k = vcpu.regs.k;

    let region = vcpu
        .jit_compile_region()
        .expect("compile RSP/RBP opmask region")
        .expect("state-backed KMOVQ forms should be JIT eligible");
    assert!(region.uses_vector);
    assert!(!region.narrow_vector_opmasks);

    vcpu.jit_run_region_native(&region);

    assert_eq!(vcpu.regs.rsp, original_rbp);
    assert_eq!(vcpu.regs.rbp, original_rsp);
    assert_eq!(vcpu.regs.k[1], original_rsp);
    assert_eq!(vcpu.regs.k[2], original_rbp);
    assert_eq!(vcpu.regs.k[3], original_rbp);
    assert_eq!(vcpu.regs.k[4], original_rsp);
    for index in [0, 5, 6, 7] {
        assert_eq!(vcpu.regs.k[index], original_k[index], "K{index}");
    }
    assert_eq!(vcpu.regs.rflags, 0x246);
    assert_eq!(vcpu.regs.rip, code.len() as u64 - 1);
}

#[test]
fn jit_compiles_and_executes_exact_width_kmov_memory_helpers() {
    if !host_has_opmask_state() {
        return;
    }

    let (mut vcpu, mem) = test_vcpu_with_mem();
    // kmovw k1,[rbx]; kmovw [rbx+2],k1;
    // kmovd k2,[rbx+4]; kmovd [rbx+8],k2;
    // kmovq k3,[rbx+16]; kmovq [rbx+24],k3; jmp next; ret.
    let code = [
        0xC5, 0xF8, 0x90, 0x0B, 0xC5, 0xF8, 0x91, 0x4B, 0x02, 0xC4, 0xE1, 0xF9, 0x90, 0x53, 0x04,
        0xC4, 0xE1, 0xF9, 0x91, 0x53, 0x08, 0xC4, 0xE1, 0xF8, 0x90, 0x5B, 0x10, 0xC4, 0xE1, 0xF8,
        0x91, 0x5B, 0x18, 0xEB, 0x00, 0xC3,
    ];
    mem.write_slice(&code, GuestAddress(0)).unwrap();
    let base = 0x2000;
    let word = 0xBEEF_u16;
    let dword = 0x89AB_CDEF_u32;
    let qword = 0x0123_4567_89AB_CDEF_u64;
    mem.write_slice(&word.to_le_bytes(), GuestAddress(base))
        .unwrap();
    mem.write_slice(&[0xCC; 2], GuestAddress(base + 2)).unwrap();
    mem.write_slice(&dword.to_le_bytes(), GuestAddress(base + 4))
        .unwrap();
    mem.write_slice(&[0xCC; 4], GuestAddress(base + 8)).unwrap();
    mem.write_slice(&qword.to_le_bytes(), GuestAddress(base + 16))
        .unwrap();
    mem.write_slice(&[0xCC; 8], GuestAddress(base + 24))
        .unwrap();
    configure_long_mode(&mut vcpu, true);
    vcpu.regs.rbx = base;
    vcpu.regs.rflags = 0xAD7;
    vcpu.regs.k = std::array::from_fn(|index| 0xC700_0000_0000_0000 | index as u64);
    let original_k = vcpu.regs.k;

    let region = vcpu
        .jit_compile_region()
        .expect("compile KMOV memory region")
        .expect("exact KMOV memory forms should be JIT eligible");
    assert!(region.uses_vector);
    assert!(!region.narrow_vector_opmasks);

    vcpu.jit_run_region_native(&region);

    let mut stored_word = [0u8; 2];
    mem.read_slice(&mut stored_word, GuestAddress(base + 2))
        .unwrap();
    let mut stored_dword = [0u8; 4];
    mem.read_slice(&mut stored_dword, GuestAddress(base + 8))
        .unwrap();
    let mut stored_qword = [0u8; 8];
    mem.read_slice(&mut stored_qword, GuestAddress(base + 24))
        .unwrap();
    assert_eq!(u16::from_le_bytes(stored_word), word);
    assert_eq!(u32::from_le_bytes(stored_dword), dword);
    assert_eq!(u64::from_le_bytes(stored_qword), qword);
    assert_eq!(vcpu.regs.k[1], u64::from(word));
    assert_eq!(vcpu.regs.k[2], u64::from(dword));
    assert_eq!(vcpu.regs.k[3], qword);
    for index in [0, 4, 5, 6, 7] {
        assert_eq!(vcpu.regs.k[index], original_k[index], "K{index}");
    }
    assert_eq!(vcpu.regs.rbx, base);
    assert_eq!(vcpu.regs.rflags, 0xAD7);
    assert_eq!(vcpu.regs.rip, code.len() as u64 - 1);
}

#[test]
fn jit_faulting_kmovq_memory_helpers_are_precise_and_noncommitting() {
    if !host_has_opmask_state() {
        return;
    }

    for (opcode, label) in [(0x90, "load"), (0x91, "store")] {
        let (mut vcpu, mem) = test_vcpu_with_mem();
        // kmovq k7,[rbx] or kmovq [rbx],k7; jmp next; ret. Only four of
        // the required eight bytes are mapped.
        let code = [0xC4, 0xE1, 0xF8, opcode, 0x3B, 0xEB, 0x00, 0xC3];
        mem.write_slice(&code, GuestAddress(0)).unwrap();
        let address = 0x10000 - 4;
        let before = [0xA5; 4];
        mem.write_slice(&before, GuestAddress(address)).unwrap();
        configure_long_mode(&mut vcpu, true);
        vcpu.regs.rbx = address;
        vcpu.regs.rflags = 0x8D7;
        vcpu.regs.k = std::array::from_fn(|index| {
            0xD800_0000_0000_0000_u64 | (index as u64 * 0x0101_0101_0101_0101)
        });
        let original_k = vcpu.regs.k;

        let region = vcpu
            .jit_compile_region()
            .unwrap_or_else(|error| panic!("compile faulting KMOVQ {label}: {error}"))
            .unwrap_or_else(|| panic!("fault-capable KMOVQ {label} must retain a native exit"));
        assert!(region.uses_vector, "{label}");
        assert!(!region.narrow_vector_opmasks, "{label}");

        vcpu.jit_run_region_native(&region);

        let mut after = [0u8; 4];
        mem.read_slice(&mut after, GuestAddress(address)).unwrap();
        assert_eq!(after, before, "faulting KMOVQ {label} memory");
        assert_eq!(vcpu.regs.k, original_k, "faulting KMOVQ {label} K state");
        assert_eq!(vcpu.regs.rbx, address, "faulting KMOVQ {label} RBX");
        assert_eq!(vcpu.regs.rflags, 0x8D7, "faulting KMOVQ {label} flags");
        assert_eq!(vcpu.regs.rip, 0, "faulting KMOVQ {label} restart PC");
    }
}
