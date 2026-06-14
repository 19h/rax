use crate::common::*;
use rax::backend::emulator::x86_64::flags;
use rax::cpu::Registers;
use vm_memory::{Bytes, GuestAddress};

const DATA_ADDR: u64 = 0x3000;
const STATUS_FLAGS: u64 = flags::bits::CF
    | flags::bits::PF
    | flags::bits::AF
    | flags::bits::ZF
    | flags::bits::SF
    | flags::bits::OF;

fn regs_with_fp16(src1: u16, src2: u16) -> Registers {
    let mut regs = Registers {
        rflags: STATUS_FLAGS | 0x2,
        ..Registers::default()
    };
    regs.xmm[1][0] = src1 as u64;
    regs.xmm[2][0] = src2 as u64;
    regs
}

fn run_flags(code: &[u8], regs: Registers) -> u64 {
    let (mut vcpu, _) = setup_vm(code, Some(regs));
    run_until_hlt(&mut vcpu).unwrap().rflags & STATUS_FLAGS
}

#[test]
fn test_vcomish_sets_zf_for_equal_operands() {
    let flags = run_flags(
        &[
            0x62, 0xf5, 0x7c, 0x08, 0x2f, 0xca, // VCOMISH xmm1, xmm2
            0xf4,
        ],
        regs_with_fp16(0x3c00, 0x3c00),
    );

    assert_eq!(flags, flags::bits::ZF);
}

#[test]
fn test_vcomish_sets_cf_for_less_than() {
    let flags = run_flags(
        &[
            0x62, 0xf5, 0x7c, 0x08, 0x2f, 0xca, // VCOMISH xmm1, xmm2
            0xf4,
        ],
        regs_with_fp16(0x3c00, 0x4000),
    );

    assert_eq!(flags, flags::bits::CF);
}

#[test]
fn test_vcomish_sae_form_compares_register_source() {
    let flags = run_flags(
        &[
            0x62, 0xf5, 0x7c, 0x18, 0x2f, 0xca, // VCOMISH xmm1, xmm2, {sae}
            0xf4,
        ],
        regs_with_fp16(0x4000, 0x3c00),
    );

    assert_eq!(flags, 0);
}

#[test]
fn test_vucomish_clears_status_flags_for_greater_than() {
    let flags = run_flags(
        &[
            0x62, 0xf5, 0x7c, 0x08, 0x2e, 0xca, // VUCOMISH xmm1, xmm2
            0xf4,
        ],
        regs_with_fp16(0x4000, 0x3c00),
    );

    assert_eq!(flags, 0);
}

#[test]
fn test_vucomish_sets_unordered_flags_for_nan() {
    let flags = run_flags(
        &[
            0x62, 0xf5, 0x7c, 0x08, 0x2e, 0xca, // VUCOMISH xmm1, xmm2
            0xf4,
        ],
        regs_with_fp16(0x7e00, 0x3c00),
    );

    assert_eq!(flags, flags::bits::ZF | flags::bits::PF | flags::bits::CF);
}

#[test]
fn test_vucomish_reads_fp16_memory_source() {
    let code = [
        0x62, 0xf5, 0x7c, 0x08, 0x2e, 0x08, // VUCOMISH xmm1, word ptr [rax]
        0xf4,
    ];
    let mut regs = regs_with_fp16(0x3c00, 0);
    regs.rax = DATA_ADDR;

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));
    mem.write_slice(&0x4000u16.to_le_bytes(), GuestAddress(DATA_ADDR))
        .unwrap();
    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.rflags & STATUS_FLAGS, flags::bits::CF);
}

#[test]
fn test_vcomish_uses_evex_x_b_for_high_rm_register() {
    let mut regs = regs_with_fp16(0x4000, 0);
    regs.xmm[2][0] = 0x7e00;
    regs.zmm_ext[2][0] = 0x3c00;

    let flags = run_flags(
        &[
            0x62, 0xb5, 0x7c, 0x08, 0x2f, 0xca, // VCOMISH xmm1, xmm18
            0xf4,
        ],
        regs,
    );

    assert_eq!(flags, 0);
}
