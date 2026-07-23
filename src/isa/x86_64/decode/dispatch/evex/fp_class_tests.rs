//! Direct-execution regressions for EVEX VFPCLASS*.

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::vm::vcpu::{Registers, VCpu};
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

const CODE: u64 = 0x1000;
const DATA_END: u64 = 0x3000;
type FpClassShape = (u8, u8, bool, u8, usize, bool);

fn shapes() -> Vec<FpClassShape> {
    let mut shapes = Vec::new();
    for (pp, w, elem_size) in [(0, false, 2), (1, false, 4), (1, true, 8)] {
        for ll in 0u8..=2 {
            shapes.push((0x66, pp, w, ll, elem_size, false));
        }
        for ll in 0u8..=3 {
            shapes.push((0x67, pp, w, ll, elem_size, true));
        }
    }
    shapes
}

fn vcpu_with_memory_size(code: &[u8], memory_size: usize) -> X86_64Vcpu {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), memory_size)]).unwrap());
    memory.write_slice(code, GuestAddress(CODE)).unwrap();

    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.regs.rip = CODE;
    vcpu.regs.rflags = 0x2 | 0x8D5;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.db = false;
    vcpu
}

fn encoding(shape: FpClassShape, destination: u8, source: u8, mask: u8, immediate: u8) -> [u8; 7] {
    let (opcode, pp, w, ll, _, scalar) = shape;
    assert!(matches!(opcode, 0x66 | 0x67) && scalar == (opcode == 0x67));
    assert!(destination < 8 && source < 32 && mask < 8);
    assert!(scalar || ll < 3);
    let mut p0 = 0xF3;
    if source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        0x7C | pp | if w { 0x80 } else { 0 },
        (ll << 5) | 0x08 | mask,
        opcode,
        0xC0 | (destination << 3) | (source & 0x07),
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

fn patterns(elem_size: usize) -> Vec<u64> {
    match elem_size {
        2 => vec![
            0x7E01, 0x0000, 0x8000, 0x7C00, 0xFC00, 0x0001, 0x8001, 0xBC00, 0x7C01, 0x3C00,
        ],
        4 => vec![
            0x7FC0_0001,
            0x0000_0000,
            0x8000_0000,
            0x7F80_0000,
            0xFF80_0000,
            0x0000_0001,
            0x8000_0001,
            0xBF80_0000,
            0x7F80_0001,
            0x3F80_0000,
        ],
        8 => vec![
            0x7FF8_0000_0000_0001,
            0x0000_0000_0000_0000,
            0x8000_0000_0000_0000,
            0x7FF0_0000_0000_0000,
            0xFFF0_0000_0000_0000,
            0x0000_0000_0000_0001,
            0x8000_0000_0000_0001,
            0xBFF0_0000_0000_0000,
            0x7FF0_0000_0000_0001,
            0x3FF0_0000_0000_0000,
        ],
        _ => unreachable!(),
    }
}

fn expected_pattern_class(bits: u64, elem_size: usize, daz: bool) -> u8 {
    // Independent SDM Table 5-11/5-12 oracle for `patterns`: QNaN, +0, -0,
    // +Inf, -Inf, +denormal, -denormal, negative finite, SNaN, positive finite.
    // Negative denormals belong to both denormal and negative-finite classes.
    const CLASSES: [u8; 10] = [1, 2, 4, 8, 16, 32, 32 | 64, 64, 128, 0];
    let index = patterns(elem_size)
        .iter()
        .position(|pattern| *pattern == bits)
        .unwrap_or_else(|| panic!("missing FPCLASS oracle pattern {bits:#018X}"));
    if daz && elem_size != 2 {
        match index {
            5 => 2,
            6 => 4,
            _ => CLASSES[index],
        }
    } else {
        CLASSES[index]
    }
}

fn initialized_vcpu(code: &[u8], elem_size: usize, scalar_sample: usize) -> X86_64Vcpu {
    let mut vcpu = vcpu_with_memory_size(code, 0x10000);
    let values = patterns(elem_size);
    for register in 0u8..32 {
        let mut bytes = [0u8; 64];
        for lane in 0..(64 / elem_size) {
            let value = values[(lane + register as usize) % values.len()];
            let base = lane * elem_size;
            bytes[base..base + elem_size].copy_from_slice(&value.to_le_bytes()[..elem_size]);
        }
        let sample = values[scalar_sample % values.len()];
        bytes[..elem_size].copy_from_slice(&sample.to_le_bytes()[..elem_size]);
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

#[test]
fn fp_class_covers_all_shapes_classes_daz_masks_extensions_and_aliases() {
    let class_cases = [
        (0usize, 1u8),
        (1, 2),
        (2, 4),
        (3, 8),
        (4, 16),
        (5, 32),
        (5, 2),
        (6, 32),
        (6, 64),
        (6, 4),
        (7, 64),
        (8, 128),
        (9, 0xFF),
        (0, 0),
    ];
    let operands = [(0u8, 1u8, 0u8), (1, 9, 0), (2, 17, 1), (1, 25, 1)];

    for shape in shapes() {
        let (_, _, _, ll, elem_size, scalar) = shape;
        for (destination, source, mask) in operands {
            for daz in [false, true] {
                for (sample, immediate) in class_cases {
                    let code = encoding(shape, destination, source, mask, immediate);
                    let mut vcpu = initialized_vcpu(&code, elem_size, sample);
                    if daz {
                        vcpu.mxcsr |= 1 << 6;
                    }
                    let before = vcpu.regs.clone();
                    let mxcsr_before = vcpu.mxcsr;
                    let source_bytes = vector_bytes(zmm(&vcpu, source));
                    let lanes = if scalar {
                        1
                    } else {
                        [16usize, 32, 64][ll as usize] / elem_size
                    };
                    let writemask = if mask == 0 {
                        u64::MAX
                    } else {
                        before.k[mask as usize]
                    };
                    let mut expected_result = 0u64;
                    for lane in 0..lanes {
                        let base = lane * elem_size;
                        let mut raw = [0u8; 8];
                        raw[..elem_size].copy_from_slice(&source_bytes[base..base + elem_size]);
                        if (writemask >> lane) & 1 != 0
                            && expected_pattern_class(u64::from_le_bytes(raw), elem_size, daz)
                                & immediate
                                != 0
                        {
                            expected_result |= 1 << lane;
                        }
                    }
                    let mut expected_masks = before.k;
                    expected_masks[destination as usize] = expected_result;

                    assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");
                    assert_eq!(vcpu.regs.k, expected_masks, "{code:02X?} DAZ={daz}");
                    assert_eq!(vcpu.regs.xmm, before.xmm, "{code:02X?}: XMM state");
                    assert_eq!(vcpu.regs.ymm_high, before.ymm_high, "{code:02X?}");
                    assert_eq!(vcpu.regs.zmm_high, before.zmm_high, "{code:02X?}");
                    assert_eq!(vcpu.regs.zmm_ext, before.zmm_ext, "{code:02X?}");
                    assert_eq!(gprs(&vcpu.regs), gprs(&before), "{code:02X?}");
                    assert_eq!(vcpu.regs.mm, before.mm, "{code:02X?}");
                    assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}");
                    assert_eq!(vcpu.mxcsr, mxcsr_before, "{code:02X?}: MXCSR");
                    assert_eq!(vcpu.regs.rip, CODE + 7, "{code:02X?}: RIP");
                }
            }
        }
    }
}

fn assert_reserved_ud(code: &[u8]) {
    let mut vcpu = initialized_vcpu(code, 4, 0);
    let before = vcpu.regs.clone();
    let mxcsr_before = vcpu.mxcsr;
    let error = match vcpu.step() {
        Err(error) => error,
        Ok(exit) => panic!("reserved FPCLASS committed: {code:02X?}: {exit:?}"),
    };
    assert!(
        format!("{error:?}").contains("IDT entry 6 not present"),
        "wrong exception for {code:02X?}: {error:?}"
    );
    assert_eq!(vcpu.regs.rip, before.rip, "{code:02X?}: fault RIP");
    assert_eq!(vcpu.regs.xmm, before.xmm, "{code:02X?}: XMM state");
    assert_eq!(vcpu.regs.ymm_high, before.ymm_high, "{code:02X?}");
    assert_eq!(vcpu.regs.zmm_high, before.zmm_high, "{code:02X?}");
    assert_eq!(vcpu.regs.zmm_ext, before.zmm_ext, "{code:02X?}");
    assert_eq!(vcpu.regs.k, before.k, "{code:02X?}: opmask state");
    assert_eq!(gprs(&vcpu.regs), gprs(&before), "{code:02X?}");
    assert_eq!(vcpu.regs.mm, before.mm, "{code:02X?}");
    assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}");
    assert_eq!(vcpu.mxcsr, mxcsr_before, "{code:02X?}: MXCSR");
}

#[test]
fn fp_class_reserved_fields_raise_precise_ud_without_commit() {
    for shape in shapes() {
        let valid = encoding(shape, 2, 17, 1, 0xFF);
        let mut invalid = Vec::new();
        for encoded_vvvv in 0u8..=0x0E {
            let mut reserved_vvvv = valid;
            reserved_vvvv[2] = (reserved_vvvv[2] & !0x78) | (encoded_vvvv << 3);
            invalid.push(reserved_vvvv);
        }
        let mut reserved_v_prime = valid;
        reserved_v_prime[3] &= !0x08;
        invalid.push(reserved_v_prime);
        let mut reserved_zeroing = valid;
        reserved_zeroing[3] |= 0x80;
        invalid.push(reserved_zeroing);
        let mut register_b = valid;
        register_b[3] |= 0x10;
        invalid.push(register_b);
        let mut destination_r = valid;
        destination_r[1] &= !0x80;
        invalid.push(destination_r);
        let mut destination_r_prime = valid;
        destination_r_prime[1] &= !0x10;
        invalid.push(destination_r_prime);
        if !shape.5 {
            let mut reserved_ll = valid;
            reserved_ll[3] = (reserved_ll[3] & !0x60) | 0x60;
            invalid.push(reserved_ll);
        }
        for code in invalid {
            assert_reserved_ud(&code);
        }
    }

    for (opcode, p1) in [(0x66, 0x7E), (0x66, 0xFC), (0x67, 0x7E), (0x67, 0xFC)] {
        assert_reserved_ud(&[0x62, 0xF3, p1, 0x08, opcode, 0xD1, 0xFF]);
    }

    let mut scalar_memory_b = encoding((0x67, 1, false, 0, 4, true), 2, 0, 1, 0xFF);
    scalar_memory_b[3] |= 0x10;
    scalar_memory_b[5] &= 0x3F;
    assert_reserved_ud(&scalar_memory_b);
}

#[test]
fn fp_class_masked_memory_suppresses_inactive_faults_and_commits_last() {
    let packed = (0x66, 1, true, 0, 8, false);
    let mut code = encoding(packed, 2, 0, 1, 1 << 1);
    code[5] &= 0x3F;

    let mut one_active = vcpu_with_memory_size(&code, DATA_END as usize);
    one_active.regs.rax = DATA_END - 8;
    one_active.regs.k[1] = 1;
    one_active
        .write_mem(DATA_END - 8, 0x0000_0000_0000_0000, 8)
        .unwrap();
    assert!(one_active.step().unwrap().is_none());
    assert_eq!(one_active.regs.k[2], 1);

    let mut none_active = vcpu_with_memory_size(&code, DATA_END as usize);
    none_active.regs.rax = DATA_END + 0x1000;
    none_active.regs.k[1] = 0;
    assert!(none_active.step().unwrap().is_none());
    assert_eq!(none_active.regs.k[2], 0);

    let mut second_faults = vcpu_with_memory_size(&code, DATA_END as usize);
    second_faults.regs.rax = DATA_END - 8;
    second_faults.regs.k[1] = 3;
    second_faults.regs.k[2] = 0xA5;
    second_faults
        .write_mem(DATA_END - 8, 0x0000_0000_0000_0000, 8)
        .unwrap();
    let before = second_faults.regs.clone();
    assert!(second_faults.step().is_err());
    assert_eq!(second_faults.regs.k, before.k);
    assert_eq!(second_faults.regs.rip, before.rip);

    let mut broadcast_code = code;
    broadcast_code[3] |= 0x10;
    let mut broadcast_inactive = vcpu_with_memory_size(&broadcast_code, DATA_END as usize);
    broadcast_inactive.regs.rax = DATA_END + 0x1000;
    broadcast_inactive.regs.k[1] = 0;
    assert!(broadcast_inactive.step().unwrap().is_none());
    assert_eq!(broadcast_inactive.regs.k[2], 0);

    let scalar = (0x67, 1, false, 3, 4, true);
    let mut scalar_code = encoding(scalar, 2, 0, 1, 1 << 1);
    scalar_code[5] &= 0x3F;
    let mut scalar_inactive = vcpu_with_memory_size(&scalar_code, DATA_END as usize);
    scalar_inactive.regs.rax = DATA_END + 0x1000;
    scalar_inactive.regs.k[1] = 0;
    assert!(scalar_inactive.step().unwrap().is_none());
    assert_eq!(scalar_inactive.regs.k[2], 0);
}
