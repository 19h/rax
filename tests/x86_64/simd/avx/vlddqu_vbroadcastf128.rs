use crate::common::*;
use rax::cpu::Registers;
use vm_memory::{Bytes, GuestAddress};

const DATA_ADDR: u64 = 0x3001;

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

#[test]
fn test_vlddqu_ymm_mem_loads_32_unaligned_bytes() {
    let code = [
        0xc5, 0xff, 0xf0, 0x08, // VLDDQU ymm1, ymmword ptr [rax]
        0xf4,
    ];
    let regs = Registers {
        rax: DATA_ADDR,
        ..Registers::default()
    };
    let (mut vcpu, mem) = setup_vm(&code, Some(regs));
    let data: Vec<u8> = (0..32).map(|i| 0xa0u8.wrapping_add(i)).collect();
    mem.write_slice(&data, GuestAddress(DATA_ADDR)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.xmm[1][0], u64_at(&data, 0));
    assert_eq!(regs.xmm[1][1], u64_at(&data, 8));
    assert_eq!(regs.ymm_high[1][0], u64_at(&data, 16));
    assert_eq!(regs.ymm_high[1][1], u64_at(&data, 24));
}

#[test]
fn test_vlddqu_xmm_mem_zeroes_upper_ymm_lane() {
    let code = [
        0xc5, 0xfb, 0xf0, 0x10, // VLDDQU xmm2, xmmword ptr [rax]
        0xf4,
    ];
    let mut regs = Registers {
        rax: DATA_ADDR,
        ..Registers::default()
    };
    regs.ymm_high[2] = [0x1111_2222_3333_4444, 0x5555_6666_7777_8888];

    let (mut vcpu, mem) = setup_vm(&code, Some(regs));
    let data: Vec<u8> = (0..16).map(|i| 0x20u8.wrapping_add(i * 3)).collect();
    mem.write_slice(&data, GuestAddress(DATA_ADDR)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();

    assert_eq!(regs.xmm[2][0], u64_at(&data, 0));
    assert_eq!(regs.xmm[2][1], u64_at(&data, 8));
    assert_eq!(regs.ymm_high[2], [0, 0]);
}

#[test]
fn test_vbroadcastf128_duplicates_128_bit_memory_tuple() {
    let code = [
        0xc4, 0xe2, 0x7d, 0x1a, 0x18, // VBROADCASTF128 ymm3, xmmword ptr [rax]
        0xf4,
    ];
    let regs = Registers {
        rax: DATA_ADDR,
        ..Registers::default()
    };
    let (mut vcpu, mem) = setup_vm(&code, Some(regs));
    let data: Vec<u8> = (0..16).map(|i| 0xe1u8.wrapping_sub(i * 7)).collect();
    mem.write_slice(&data, GuestAddress(DATA_ADDR)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();

    let lo = u64_at(&data, 0);
    let hi = u64_at(&data, 8);
    assert_eq!(regs.xmm[3], [lo, hi]);
    assert_eq!(regs.ymm_high[3], [lo, hi]);
}

#[test]
fn test_vbroadcastf128_extended_destination_register() {
    let code = [
        0xc4, 0x62, 0x7d, 0x1a, 0x08, // VBROADCASTF128 ymm9, xmmword ptr [rax]
        0xf4,
    ];
    let regs = Registers {
        rax: DATA_ADDR,
        ..Registers::default()
    };
    let (mut vcpu, mem) = setup_vm(&code, Some(regs));
    let data: Vec<u8> = (0..16).map(|i| 0x11u8.wrapping_add(i * 13)).collect();
    mem.write_slice(&data, GuestAddress(DATA_ADDR)).unwrap();

    let regs = run_until_hlt(&mut vcpu).unwrap();

    let lo = u64_at(&data, 0);
    let hi = u64_at(&data, 8);
    assert_eq!(regs.xmm[9], [lo, hi]);
    assert_eq!(regs.ymm_high[9], [lo, hi]);
}
