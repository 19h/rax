//! Direct-execution regressions for EVEX VEXTRACTF*/VEXTRACTI* vector chunks.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::vm::vcpu::{Registers, VCpu};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const CODE: u64 = 0x1000;
const DATA: u64 = 0x2000;
type ExtractShape = (u8, bool, u8, usize);

fn shapes() -> Vec<ExtractShape> {
    let mut shapes = Vec::new();
    for opcode in [0x19, 0x39] {
        for w in [false, true] {
            for ll in [1, 2] {
                shapes.push((opcode, w, ll, 16));
            }
        }
    }
    for opcode in [0x1B, 0x3B] {
        for w in [false, true] {
            shapes.push((opcode, w, 2, 32));
        }
    }
    shapes
}

fn long_mode_vcpu(code: &[u8]) -> X86_64Vcpu {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(CODE)).unwrap();

    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.regs.rip = CODE;
    vcpu.regs.rflags = 0x2 | 0x8D5;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.db = false;
    vcpu
}

fn encoding(
    shape: ExtractShape,
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
    immediate: u8,
) -> [u8; 7] {
    let (opcode, w, ll, chunk_bytes) = shape;
    assert!(
        matches!(opcode, 0x19 | 0x39) && chunk_bytes == 16 && matches!(ll, 1 | 2)
            || matches!(opcode, 0x1B | 0x3B) && chunk_bytes == 32 && ll == 2
    );
    assert!(destination < 32 && source < 32 && mask < 8);
    assert!(!zeroing || mask != 0);
    let mut p0 = 0xF3;
    if source & 0x08 != 0 {
        p0 &= !0x80;
    }
    if source & 0x10 != 0 {
        p0 &= !0x10;
    }
    if destination & 0x08 != 0 {
        p0 &= !0x20;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7D | if w { 0x80 } else { 0 },
        (ll << 5) | 0x08 | mask | if zeroing { 0x80 } else { 0 },
        opcode,
        0xC0 | ((source & 0x07) << 3) | (destination & 0x07),
        immediate,
    ]
}

fn zmm(vcpu: &X86_64Vcpu, register: u8) -> [u64; 8] {
    if register >= 16 {
        return vcpu.regs.zmm_ext[(register - 16) as usize];
    }
    let index = register as usize;
    let mut value = [0u64; 8];
    value[..2].copy_from_slice(&vcpu.regs.xmm[index]);
    value[2..4].copy_from_slice(&vcpu.regs.ymm_high[index]);
    value[4..].copy_from_slice(&vcpu.regs.zmm_high[index]);
    value
}

fn set_zmm(vcpu: &mut X86_64Vcpu, register: u8, value: [u64; 8]) {
    if register >= 16 {
        vcpu.regs.zmm_ext[(register - 16) as usize] = value;
        return;
    }
    let index = register as usize;
    vcpu.regs.xmm[index].copy_from_slice(&value[..2]);
    vcpu.regs.ymm_high[index].copy_from_slice(&value[2..4]);
    vcpu.regs.zmm_high[index].copy_from_slice(&value[4..]);
}

fn vector_bytes(vector: [u64; 8]) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (index, word) in vector.iter().enumerate() {
        bytes[index * 8..index * 8 + 8].copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn vector_words(bytes: [u8; 64]) -> [u64; 8] {
    std::array::from_fn(|index| {
        u64::from_le_bytes(bytes[index * 8..index * 8 + 8].try_into().unwrap())
    })
}

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

fn initialized_vcpu(code: &[u8]) -> X86_64Vcpu {
    let mut vcpu = long_mode_vcpu(code);
    for register in 0u8..32 {
        let mut bytes = [0u8; 64];
        for (lane, byte) in bytes.iter_mut().enumerate() {
            *byte = (lane as u8)
                .wrapping_mul(0x31)
                .wrapping_add(register.wrapping_mul(0x17))
                ^ if lane % 7 == 2 { 0xA5 } else { 0 };
        }
        set_zmm(&mut vcpu, register, vector_words(bytes));
    }
    for register in 0u8..16 {
        vcpu.set_reg(
            register,
            0xFEDC_BA98_7654_3210u64.rotate_left((register * 3) as u32),
            8,
        );
    }
    vcpu.regs.k = std::array::from_fn(|index| {
        0xA55A_3CC3_F00F_9696u64.rotate_left((index * 7) as u32) ^ (1u64 << index)
    });
    vcpu.regs.mm = std::array::from_fn(|index| 0x8877_6655_4433_2211 ^ index as u64);
    vcpu
}

fn extracted_result(
    shape: ExtractShape,
    destination: [u64; 8],
    source: [u64; 8],
    mask: u64,
    zeroing: bool,
    immediate: u8,
) -> [u64; 8] {
    let (_, w, ll, chunk_bytes) = shape;
    let elem_size = if w { 8 } else { 4 };
    let vl_bytes = if ll == 1 { 32 } else { 64 };
    let source = vector_bytes(source);
    let chunk = immediate as usize & (vl_bytes / chunk_bytes - 1);
    let source_base = chunk * chunk_bytes;
    let mut raw = [0u8; 64];
    raw[..chunk_bytes].copy_from_slice(&source[source_base..source_base + chunk_bytes]);

    let mut result = vector_bytes(destination);
    for lane in 0..(chunk_bytes / elem_size) {
        let base = lane * elem_size;
        if (mask >> lane) & 1 != 0 {
            result[base..base + elem_size].copy_from_slice(&raw[base..base + elem_size]);
        } else if zeroing {
            result[base..base + elem_size].fill(0);
        }
    }
    result[chunk_bytes..].fill(0);
    vector_words(result)
}

#[test]
fn chunk_extract_covers_shapes_extensions_aliases_masks_and_selectors() {
    let operands = [
        (1u8, 2u8),
        (9, 10),
        (17, 18),
        (25, 26),
        (1, 1),
        (9, 25),
        (25, 9),
    ];
    for shape in shapes() {
        for (destination, source) in operands {
            for (mask_register, zeroing) in [(0u8, false), (1, false), (2, true)] {
                for immediate in [0u8, 1, 2, 3, 0xFE, 0xFF] {
                    let code = encoding(
                        shape,
                        destination,
                        source,
                        mask_register,
                        zeroing,
                        immediate,
                    );
                    let mut vcpu = initialized_vcpu(&code);
                    let before = vcpu.regs.clone();
                    let before_vectors: [[u64; 8]; 32] =
                        std::array::from_fn(|register| zmm(&vcpu, register as u8));
                    let active_mask = if mask_register == 0 {
                        u64::MAX
                    } else {
                        before.k[mask_register as usize]
                    };
                    let expected = extracted_result(
                        shape,
                        before_vectors[destination as usize],
                        before_vectors[source as usize],
                        active_mask,
                        zeroing,
                        immediate,
                    );

                    assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");
                    assert_eq!(zmm(&vcpu, destination), expected, "{code:02X?}");
                    assert_eq!(vcpu.regs.k, before.k, "{code:02X?}: opmask state");
                    assert_eq!(gprs(&vcpu.regs), gprs(&before), "{code:02X?}: GPR state");
                    assert_eq!(vcpu.regs.mm, before.mm, "{code:02X?}: MMX state");
                    assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}: flags");
                    assert_eq!(vcpu.regs.rip, CODE + 7, "{code:02X?}: RIP");
                    for register in 0u8..32 {
                        if register != destination {
                            assert_eq!(
                                zmm(&vcpu, register),
                                before_vectors[register as usize],
                                "{code:02X?}: zmm{register}"
                            );
                        }
                    }
                }
            }
        }
    }
}

fn assert_reserved_ud(code: &[u8]) {
    let mut vcpu = initialized_vcpu(code);
    let before = vcpu.regs.clone();

    let error = match vcpu.step() {
        Err(error) => error,
        Ok(exit) => panic!("reserved chunk extract committed: {code:02X?}: {exit:?}"),
    };
    assert!(
        format!("{error:?}").contains("IDT entry 6 not present"),
        "wrong exception for {code:02X?}: {error:?}"
    );
    assert_eq!(vcpu.regs.rip, before.rip, "{code:02X?}: fault RIP");
    assert_eq!(vcpu.regs.xmm, before.xmm, "{code:02X?}: XMM state");
    assert_eq!(
        vcpu.regs.ymm_high, before.ymm_high,
        "{code:02X?}: YMM state"
    );
    assert_eq!(
        vcpu.regs.zmm_high, before.zmm_high,
        "{code:02X?}: ZMM state"
    );
    assert_eq!(vcpu.regs.zmm_ext, before.zmm_ext, "{code:02X?}: ZMM16-31");
    assert_eq!(vcpu.regs.k, before.k, "{code:02X?}: opmask state");
    assert_eq!(gprs(&vcpu.regs), gprs(&before), "{code:02X?}: GPR state");
    assert_eq!(vcpu.regs.mm, before.mm, "{code:02X?}: MMX state");
    assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}: flags");
}

#[test]
fn chunk_extract_reserved_fields_raise_precise_ud_without_commit() {
    for shape in shapes() {
        let valid = encoding(shape, 1, 2, 0, false, 0xFF);
        let mut invalid = Vec::new();

        for invalid_ll in 0u8..=3 {
            if invalid_ll != shape.2 {
                let legal_other_width = shape.3 == 16 && matches!(invalid_ll, 1 | 2);
                if !legal_other_width {
                    let mut wrong_ll = valid;
                    wrong_ll[3] = (wrong_ll[3] & !0x60) | (invalid_ll << 5);
                    invalid.push(wrong_ll);
                }
            }
        }
        let mut embedded_broadcast = valid;
        embedded_broadcast[3] |= 0x10;
        invalid.push(embedded_broadcast);
        let mut zeroing_without_mask = valid;
        zeroing_without_mask[3] |= 0x80;
        invalid.push(zeroing_without_mask);
        for encoded_vvvv in 0u8..=0x0E {
            let mut reserved_vvvv = valid;
            reserved_vvvv[2] = (reserved_vvvv[2] & !0x78) | (encoded_vvvv << 3);
            invalid.push(reserved_vvvv);
        }
        let mut reserved_v_prime = valid;
        reserved_v_prime[3] &= !0x08;
        invalid.push(reserved_v_prime);
        for pp in [0u8, 2, 3] {
            let mut wrong_pp = valid;
            wrong_pp[2] = (wrong_pp[2] & !0x03) | pp;
            invalid.push(wrong_pp);
        }

        for code in invalid {
            assert_reserved_ud(&code);
        }

        let mut memory_zeroing = encoding(shape, 0, 2, 1, true, 0xFF);
        memory_zeroing[5] &= 0x3F;
        assert_reserved_ud(&memory_zeroing);
    }
}

fn write_memory_chunk(vcpu: &mut X86_64Vcpu, bytes: &[u8; 64], chunk_bytes: usize) {
    for (index, chunk) in bytes[..chunk_bytes].chunks_exact(8).enumerate() {
        vcpu.write_mem(
            DATA + (index * 8) as u64,
            u64::from_le_bytes(chunk.try_into().unwrap()),
            8,
        )
        .unwrap();
    }
}

fn read_memory_chunk(vcpu: &mut X86_64Vcpu, chunk_bytes: usize) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for index in 0..(chunk_bytes / 8) {
        bytes[index * 8..index * 8 + 8].copy_from_slice(
            &vcpu
                .read_mem(DATA + (index * 8) as u64, 8)
                .unwrap()
                .to_le_bytes(),
        );
    }
    bytes
}

#[test]
fn chunk_extract_memory_destinations_remain_supported() {
    for shape in shapes() {
        let mut code = encoding(shape, 0, 2, 1, false, 0xFF);
        code[5] &= 0x3F;
        let mut vcpu = initialized_vcpu(&code);
        vcpu.regs.rax = DATA;
        let initial_memory = vector_bytes([
            0x800F_0E0D_0C0B_0A09,
            0x0807_0605_0403_0201,
            0x7F80_FF00_0102_FE81,
            0x55AA_33CC_0FF0_6996,
            0xFEDC_BA98_7654_3210,
            0x0123_4567_89AB_CDEF,
            0x8080_7F7F_FFFF_0000,
            0x8000_8000_7FFF_7FFF,
        ]);
        write_memory_chunk(&mut vcpu, &initial_memory, shape.3);
        let before = vcpu.regs.clone();
        let (_, w, ll, chunk_bytes) = shape;
        let elem_size = if w { 8 } else { 4 };
        let vl_bytes = if ll == 1 { 32 } else { 64 };
        let source = vector_bytes(zmm(&vcpu, 2));
        let chunk = 0xFFusize & (vl_bytes / chunk_bytes - 1);
        let source_base = chunk * chunk_bytes;
        let mut expected = initial_memory;
        for lane in 0..(chunk_bytes / elem_size) {
            if (before.k[1] >> lane) & 1 != 0 {
                let base = lane * elem_size;
                expected[base..base + elem_size]
                    .copy_from_slice(&source[source_base + base..source_base + base + elem_size]);
            }
        }

        assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");
        assert_eq!(
            read_memory_chunk(&mut vcpu, chunk_bytes)[..chunk_bytes],
            expected[..chunk_bytes],
            "{code:02X?}"
        );
        assert_eq!(vcpu.regs.xmm, before.xmm, "{code:02X?}: XMM state");
        assert_eq!(
            vcpu.regs.ymm_high, before.ymm_high,
            "{code:02X?}: YMM state"
        );
        assert_eq!(
            vcpu.regs.zmm_high, before.zmm_high,
            "{code:02X?}: ZMM state"
        );
        assert_eq!(vcpu.regs.zmm_ext, before.zmm_ext, "{code:02X?}: ZMM16-31");
        assert_eq!(vcpu.regs.k, before.k, "{code:02X?}: opmask state");
        assert_eq!(gprs(&vcpu.regs), gprs(&before), "{code:02X?}: GPR state");
        assert_eq!(vcpu.regs.mm, before.mm, "{code:02X?}: MMX state");
        assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}: flags");
        assert_eq!(vcpu.regs.rip, CODE + 7, "{code:02X?}");
    }
}
