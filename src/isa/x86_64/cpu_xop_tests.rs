//! CPU-level direct and native-JIT coverage for AMD XOP packed rotate/shift.

use super::*;
use crate::isa::x86_64::flags;
use std::sync::Arc;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

pub(super) const DATA: u64 = 0x3000;
pub(super) const CR0_PE: u64 = 1;
pub(super) const CR0_TS: u64 = 1 << 3;
pub(super) const CR0_AM: u64 = 1 << 18;
pub(super) const CR4_OSFXSR: u64 = 1 << 9;
pub(super) const CR4_OSXSAVE: u64 = 1 << 18;

#[derive(Clone, Copy, Debug)]
enum PackedKind {
    Rotate,
    LogicalShift,
    ArithmeticShift,
}

pub(super) fn memory_with_code(code: &[u8]) -> Arc<GuestMemoryMmap> {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(0)).unwrap();
    memory
}

pub(super) fn test_vcpu(memory: Arc<GuestMemoryMmap>, jit_mem: bool) -> X86_64Vcpu {
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.sregs.cr0 = CR0_PE;
    vcpu.sregs.cr4 = CR4_OSFXSR | CR4_OSXSAVE;
    vcpu.xcr0 = 0b110;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.selector = 0;
    vcpu.regs.rip = 0;
    vcpu.regs.rsp = 0x8000;
    vcpu.regs.rbp = 0x7000;
    vcpu.regs.rflags = 0x2 | 0x08D5 | flags::bits::DF;
    vcpu.mxcsr = 0x5F80;
    vcpu.set_xop_enabled(true);
    #[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
    {
        vcpu.set_jit_mem(jit_mem);
        vcpu.set_jit_call(false);
    }
    let _ = jit_mem;
    vcpu
}

pub(super) fn xop(map: u8, w: bool, l: bool, pp: u8, vvvv: u8, opcode: u8, tail: &[u8]) -> Vec<u8> {
    assert!((8..=31).contains(&map));
    assert!(pp < 4 && vvvv < 16);
    let mut bytes = vec![
        0x8F,
        0xE0 | map,
        (u8::from(w) << 7) | (((!vvvv) & 0x0F) << 3) | (u8::from(l) << 2) | pp,
        opcode,
    ];
    bytes.extend_from_slice(tail);
    bytes
}

pub(super) fn seed_architectural_state(vcpu: &mut X86_64Vcpu) {
    vcpu.regs.rax = 0x0123_4567_89AB_CDEF;
    vcpu.regs.rcx = 0x1111_2222_3333_4444;
    vcpu.regs.rdx = 0x5555_6666_7777_8888;
    vcpu.regs.rbx = DATA;
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
    vcpu.regs.xmm = std::array::from_fn(|index| {
        [
            0x0123_4567_89AB_CDEF_u64.rotate_left((index * 7) as u32),
            0xFEDC_BA98_7654_3210_u64.rotate_right((index * 11) as u32),
        ]
    });
    vcpu.regs.xmm[3] = [0x9234_7E81_9ABC_5678, 0x8123_4567_89AB_CDEF];
    vcpu.regs.xmm[4] = [0x0180_FF7F_1141_10F0, 0x4081_20E0_08F8_04FC];
    vcpu.regs.xmm[5] = [0x100F_0E0D_0C0B_0A09, 0x0807_0605_0403_0201];
    vcpu.regs.xmm[6] = [0x6996_F00F_3CC3_A55A, 0x9669_0FF0_C33C_5AA5];
    vcpu.regs.ymm_high = std::array::from_fn(|index| {
        [
            0x1111_2222_3333_4444_u64.rotate_left((index * 5) as u32),
            0xAAAA_BBBB_CCCC_DDDD_u64.rotate_right((index * 3) as u32),
        ]
    });
    vcpu.regs.zmm_high = std::array::from_fn(|index| {
        std::array::from_fn(|word| {
            0xF0E1_D2C3_B4A5_9687_u64.rotate_left((index * 13 + word * 17) as u32)
        })
    });
    vcpu.regs.zmm_ext = std::array::from_fn(|index| {
        std::array::from_fn(|word| {
            0x6996_F00F_3CC3_A55A_u64.rotate_right((index * 19 + word * 23) as u32)
        })
    });
    vcpu.regs.k = [
        0x6996_F00F_3CC3_A55A,
        0,
        1,
        0x0123_4567_89AB_CDEF,
        0x5555_AAAA_3333_CCCC,
        0x8000_0000_0000_0000,
        0xF0F0_0F0F_A5A5_5A5A,
        u64::MAX,
    ];
    vcpu.regs.mm =
        std::array::from_fn(|index| 0x0123_4567_89AB_CDEF_u64.rotate_left((index * 9) as u32));
}

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi, regs.r8,
        regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16, regs.r17,
        regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24, regs.r25, regs.r26,
        regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

pub(super) fn assert_registers_equal(actual: &Registers, expected: &Registers, context: &str) {
    assert_eq!(gprs(actual), gprs(expected), "{context}: GPRs");
    assert_eq!(actual.xmm, expected.xmm, "{context}: XMM");
    assert_eq!(actual.ymm_high, expected.ymm_high, "{context}: YMM high");
    assert_eq!(actual.zmm_high, expected.zmm_high, "{context}: ZMM high");
    assert_eq!(actual.zmm_ext, expected.zmm_ext, "{context}: ZMM16-ZMM31");
    assert_eq!(actual.k, expected.k, "{context}: opmasks");
    assert_eq!(actual.mm, expected.mm, "{context}: MMX");
    assert_eq!(actual.rflags, expected.rflags, "{context}: RFLAGS");
    assert_eq!(actual.rip, expected.rip, "{context}: RIP");
}

fn words_to_bytes(words: [u64; 2]) -> [u8; 16] {
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&words[0].to_le_bytes());
    bytes[8..].copy_from_slice(&words[1].to_le_bytes());
    bytes
}

fn bytes_to_words(bytes: [u8; 16]) -> [u64; 2] {
    [
        u64::from_le_bytes(bytes[..8].try_into().unwrap()),
        u64::from_le_bytes(bytes[8..].try_into().unwrap()),
    ]
}

fn vector_words(regs: &Registers, register: usize, width: usize) -> [u64; 4] {
    assert!(matches!(width, 16 | 32));
    let mut result = [0_u64; 4];
    result[..2].copy_from_slice(&regs.xmm[register]);
    if width == 32 {
        result[2..].copy_from_slice(&regs.ymm_high[register]);
    }
    result
}

/// Specification-derived oracle for AMD APM Vol. 4 VPCMOV semantics.
fn vpcmov_reference(
    source1: [u64; 4],
    source2: [u64; 4],
    mask: [u64; 4],
    width: usize,
) -> [u64; 4] {
    assert!(matches!(width, 16 | 32));
    let mut result = [0_u64; 4];
    for word in 0..width / 8 {
        result[word] = (source1[word] & mask[word]) | (source2[word] & !mask[word]);
    }
    result
}

fn packed_shape(opcode: u8) -> (PackedKind, usize) {
    match opcode {
        0x90 | 0xC0 => (PackedKind::Rotate, 1),
        0x91 | 0xC1 => (PackedKind::Rotate, 2),
        0x92 | 0xC2 => (PackedKind::Rotate, 4),
        0x93 | 0xC3 => (PackedKind::Rotate, 8),
        0x94 => (PackedKind::LogicalShift, 1),
        0x95 => (PackedKind::LogicalShift, 2),
        0x96 => (PackedKind::LogicalShift, 4),
        0x97 => (PackedKind::LogicalShift, 8),
        0x98 => (PackedKind::ArithmeticShift, 1),
        0x99 => (PackedKind::ArithmeticShift, 2),
        0x9A => (PackedKind::ArithmeticShift, 4),
        0x9B => (PackedKind::ArithmeticShift, 8),
        _ => panic!("unassigned packed XOP opcode {opcode:#04x}"),
    }
}

/// Specification-derived oracle for AMD APM Vol. 4 packed XOP semantics.
fn packed_reference(
    source_words: [u64; 2],
    count_words: Option<[u64; 2]>,
    immediate: Option<u8>,
    opcode: u8,
) -> [u64; 2] {
    let source = words_to_bytes(source_words);
    let counts = count_words.map(words_to_bytes);
    let (kind, element_bytes) = packed_shape(opcode);
    let bits = (element_bytes * 8) as u32;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1_u64 << bits) - 1
    };
    let mut output = [0_u8; 16];
    for offset in (0..16).step_by(element_bytes) {
        let mut lane = [0_u8; 8];
        lane[..element_bytes].copy_from_slice(&source[offset..offset + element_bytes]);
        let value = u64::from_le_bytes(lane);
        let signed_count =
            immediate.unwrap_or_else(|| counts.expect("variable count")[offset]) as i8;
        let amount = u32::from(signed_count.unsigned_abs()) & (bits - 1);
        let result = match (kind, signed_count.is_negative()) {
            (PackedKind::Rotate, false) => {
                if bits == 64 {
                    value.rotate_left(amount)
                } else {
                    ((value << amount) | (value >> ((bits - amount) & (bits - 1)))) & mask
                }
            }
            (PackedKind::Rotate, true) => {
                if bits == 64 {
                    value.rotate_right(amount)
                } else {
                    ((value >> amount) | (value << ((bits - amount) & (bits - 1)))) & mask
                }
            }
            (PackedKind::LogicalShift, false) | (PackedKind::ArithmeticShift, false) => {
                (value << amount) & mask
            }
            (PackedKind::LogicalShift, true) => value >> amount,
            (PackedKind::ArithmeticShift, true) => {
                let signed = if bits == 64 {
                    value as i64
                } else {
                    ((value << (64 - bits)) as i64) >> (64 - bits)
                };
                ((signed >> amount) as u64) & mask
            }
        };
        output[offset..offset + element_bytes]
            .copy_from_slice(&result.to_le_bytes()[..element_bytes]);
    }
    bytes_to_words(output)
}

fn exception_without_idt(vcpu: &mut X86_64Vcpu) -> String {
    format!(
        "{:#}",
        vcpu.step()
            .expect_err("exception delivery must fail against the empty test IDT")
    )
}

pub(super) fn assert_fault_noncommitting(vcpu: &mut X86_64Vcpu, vector: u8, context: &str) {
    let before = vcpu.regs.clone();
    let error = exception_without_idt(vcpu);
    assert!(
        error.contains(&format!("IDT entry {vector} not present")),
        "{context}: expected vector {vector}, got {error}"
    );
    assert_registers_equal(&vcpu.regs, &before, context);
}

fn run_direct_to(vcpu: &mut X86_64Vcpu, target: u64) {
    for _ in 0..32 {
        if vcpu.regs.rip == target {
            return;
        }
        assert!(
            vcpu.step().expect("direct XOP sequence").is_none(),
            "unexpected direct exit at {:#x}",
            vcpu.regs.rip
        );
    }
    panic!("direct execution did not reach {target:#x}");
}

#[test]
fn direct_packed_xop_executes_every_assigned_cell_and_both_w_operand_orders() {
    for opcode in 0xC0..=0xC3 {
        let immediate = 0xA5;
        let code = xop(8, false, false, 0, 0, opcode, &[0xD3, immediate]);
        let mut vcpu = test_vcpu(memory_with_code(&code), false);
        seed_architectural_state(&mut vcpu);
        let before = vcpu.regs.clone();
        let expected = packed_reference(before.xmm[3], None, Some(immediate), opcode);

        assert!(vcpu.step().expect("direct immediate packed XOP").is_none());

        assert_eq!(vcpu.regs.xmm[2], expected, "opcode={opcode:#04x}");
        assert_eq!(vcpu.regs.ymm_high[2], [0; 2], "opcode={opcode:#04x}");
        assert_eq!(vcpu.regs.zmm_high[2], [0; 4], "opcode={opcode:#04x}");
        assert_eq!(vcpu.regs.rflags, before.rflags, "opcode={opcode:#04x}");
        assert_eq!(gprs(&vcpu.regs), gprs(&before), "opcode={opcode:#04x}");
        assert_eq!(vcpu.regs.rip, code.len() as u64, "opcode={opcode:#04x}");
    }

    for opcode in 0x90..=0x9B {
        for w in [false, true] {
            let code = xop(9, w, false, 0, 4, opcode, &[0xD3]);
            let mut vcpu = test_vcpu(memory_with_code(&code), false);
            seed_architectural_state(&mut vcpu);
            let before = vcpu.regs.clone();
            let (source, counts) = if w {
                (before.xmm[4], before.xmm[3])
            } else {
                (before.xmm[3], before.xmm[4])
            };
            let expected = packed_reference(source, Some(counts), None, opcode);

            assert!(vcpu.step().expect("direct variable packed XOP").is_none());

            assert_eq!(vcpu.regs.xmm[2], expected, "opcode={opcode:#04x}, W={w}");
            assert_eq!(vcpu.regs.ymm_high[2], [0; 2], "opcode={opcode:#04x}, W={w}");
            assert_eq!(vcpu.regs.zmm_high[2], [0; 4], "opcode={opcode:#04x}, W={w}");
            assert_eq!(
                vcpu.regs.rflags, before.rflags,
                "opcode={opcode:#04x}, W={w}"
            );
            assert_eq!(
                gprs(&vcpu.regs),
                gprs(&before),
                "opcode={opcode:#04x}, W={w}"
            );
            assert_eq!(
                vcpu.regs.rip,
                code.len() as u64,
                "opcode={opcode:#04x}, W={w}"
            );
        }
    }
}

#[test]
fn direct_vpcmov_executes_both_widths_w_roles_aliases_and_high_registers() {
    for l in [false, true] {
        let width = if l { 32 } else { 16 };
        for w in [false, true] {
            // VPCMOV {X,Y}MM3,{X,Y}MM2,{X,Y}MM1,{X,Y}MM4. The low immediate
            // nibble is architecturally ignored.
            let code = xop(8, w, l, 0, 2, 0xA2, &[0xD9, 0x4D]);
            let mut vcpu = test_vcpu(memory_with_code(&code), false);
            seed_architectural_state(&mut vcpu);
            let before = vcpu.regs.clone();
            let source1 = vector_words(&before, 2, width);
            let rm = vector_words(&before, 1, width);
            let selected = vector_words(&before, 4, width);
            let (source2, mask) = if w { (selected, rm) } else { (rm, selected) };
            let expected = vpcmov_reference(source1, source2, mask, width);

            assert!(vcpu.step().expect("direct VPCMOV").is_none());

            assert_eq!(vcpu.regs.xmm[3], expected[..2], "W={w}, L={l}");
            assert_eq!(
                vcpu.regs.ymm_high[3],
                if l {
                    [expected[2], expected[3]]
                } else {
                    [0; 2]
                },
                "W={w}, L={l}"
            );
            assert_eq!(vcpu.regs.zmm_high[3], [0; 4], "W={w}, L={l}");
            assert_eq!(vcpu.regs.rflags, before.rflags, "W={w}, L={l}");
            assert_eq!(vcpu.mxcsr, 0x5F80, "W={w}, L={l}");
            assert_eq!(gprs(&vcpu.regs), gprs(&before), "W={w}, L={l}");
            assert_eq!(vcpu.regs.rip, code.len() as u64, "W={w}, L={l}");
        }
    }

    // ~R=0 and ~B=0 extend the destination and ModR/M source; vvvv and IS4
    // independently select the other two high registers.
    let code = [0x8F, 0x48, 0x28, 0xA2, 0xD9, 0xF0];
    let mut vcpu = test_vcpu(memory_with_code(&code), false);
    seed_architectural_state(&mut vcpu);
    let before = vcpu.regs.clone();
    let expected = vpcmov_reference(
        vector_words(&before, 10, 16),
        vector_words(&before, 9, 16),
        vector_words(&before, 15, 16),
        16,
    );
    assert!(vcpu.step().expect("high-register VPCMOV").is_none());
    assert_eq!(vcpu.regs.xmm[11], expected[..2]);
    assert_eq!(vcpu.regs.ymm_high[11], [0; 2]);
    assert_eq!(vcpu.regs.zmm_high[11], [0; 4]);

    // Every architectural alias is legal; all inputs must be snapshotted
    // before destination commit.
    for (name, destination, source1, rm, selected) in [
        ("dst=src1", 2, 2, 1, 4),
        ("dst=src2", 1, 2, 1, 4),
        ("dst=mask", 4, 2, 1, 4),
        ("all operands", 2, 2, 2, 2),
    ] {
        let code = xop(
            8,
            false,
            false,
            0,
            source1,
            0xA2,
            &[0xC0 | (destination << 3) | rm, selected << 4],
        );
        let mut vcpu = test_vcpu(memory_with_code(&code), false);
        seed_architectural_state(&mut vcpu);
        let before = vcpu.regs.clone();
        let expected = vpcmov_reference(
            vector_words(&before, usize::from(source1), 16),
            vector_words(&before, usize::from(rm), 16),
            vector_words(&before, usize::from(selected), 16),
            16,
        );
        assert!(vcpu.step().expect("aliased VPCMOV").is_none());
        assert_eq!(
            vcpu.regs.xmm[usize::from(destination)],
            expected[..2],
            "{name}"
        );
    }
}

#[test]
fn direct_vpcmov_memory_forms_preserve_roles_alignment_and_fault_precision() {
    for l in [false, true] {
        let width = if l { 32 } else { 16 };
        for w in [false, true] {
            let code = xop(8, w, l, 0, 2, 0xA2, &[0x1B, 0x40]);
            let memory = memory_with_code(&code);
            let memory_value = [
                0x0123_4567_89AB_CDEF,
                0xFEDC_BA98_7654_3210,
                0x6996_F00F_3CC3_A55A,
                0x9669_0FF0_C33C_5AA5,
            ];
            for (index, value) in memory_value[..width / 8].iter().enumerate() {
                memory
                    .write_obj(*value, GuestAddress(DATA + (index * 8) as u64))
                    .unwrap();
            }
            let mut vcpu = test_vcpu(memory, false);
            seed_architectural_state(&mut vcpu);
            let before = vcpu.regs.clone();
            let source1 = vector_words(&before, 2, width);
            let selected = vector_words(&before, 4, width);
            let (source2, mask) = if w {
                (selected, memory_value)
            } else {
                (memory_value, selected)
            };
            let expected = vpcmov_reference(source1, source2, mask, width);

            assert!(vcpu.step().expect("memory VPCMOV").is_none());
            assert_eq!(vcpu.regs.xmm[3], expected[..2], "W={w}, L={l}");
            assert_eq!(
                vcpu.regs.ymm_high[3],
                if l {
                    [expected[2], expected[3]]
                } else {
                    [0; 2]
                },
                "W={w}, L={l}"
            );
        }
    }

    let instruction_len = 10_u64;
    let displacement = (DATA - instruction_len) as i32;
    let mut tail = vec![0x1D];
    tail.extend_from_slice(&displacement.to_le_bytes());
    tail.push(0x40);
    let code = xop(8, false, false, 0, 2, 0xA2, &tail);
    assert_eq!(code.len() as u64, instruction_len);
    let memory = memory_with_code(&code);
    let rip_relative_value = [0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210];
    for (index, value) in rip_relative_value.iter().enumerate() {
        memory
            .write_obj(*value, GuestAddress(DATA + (index * 8) as u64))
            .unwrap();
    }
    let mut rip_relative = test_vcpu(memory, false);
    seed_architectural_state(&mut rip_relative);
    let before = rip_relative.regs.clone();
    let mut memory_operand = [0_u64; 4];
    memory_operand[..2].copy_from_slice(&rip_relative_value);
    let expected = vpcmov_reference(
        vector_words(&before, 2, 16),
        memory_operand,
        vector_words(&before, 4, 16),
        16,
    );
    assert!(rip_relative.step().expect("RIP-relative VPCMOV").is_none());
    assert_eq!(rip_relative.regs.xmm[3], expected[..2]);
    assert_eq!(rip_relative.regs.rip, instruction_len);

    let code = xop(8, false, true, 0, 2, 0xA2, &[0x1B, 0x40]);
    let mut alignment = test_vcpu(memory_with_code(&code), false);
    seed_architectural_state(&mut alignment);
    alignment.regs.rbx = 0x20_001;
    alignment.sregs.cr0 |= CR0_AM;
    alignment.sregs.cs.selector = 3;
    alignment.regs.rflags |= flags::bits::AC;
    assert_fault_noncommitting(&mut alignment, 17, "VPCMOV #AC precedes #PF");

    let mut range = test_vcpu(memory_with_code(&code), false);
    seed_architectural_state(&mut range);
    range.regs.rbx = 0x0000_7FFF_FFFF_FFF0;
    assert_fault_noncommitting(&mut range, 13, "VPCMOV canonical range crossing");

    let stack_code = xop(8, false, true, 0, 2, 0xA2, &[0x1C, 0x24, 0x40]);
    let mut stack = test_vcpu(memory_with_code(&stack_code), false);
    seed_architectural_state(&mut stack);
    stack.regs.rsp = 0x0000_8000_0000_0000;
    assert_fault_noncommitting(&mut stack, 12, "VPCMOV noncanonical stack address");
}

#[test]
fn direct_vpcmov_reserved_and_dynamic_faults_precede_memory_observation() {
    let memory_code = xop(8, false, true, 0, 2, 0xA2, &[0x1B, 0x40]);
    let mut invalid_pp = test_vcpu(
        memory_with_code(&xop(8, false, true, 1, 2, 0xA2, &[0x1B, 0x40])),
        false,
    );
    seed_architectural_state(&mut invalid_pp);
    invalid_pp.sregs.cr0 |= CR0_TS;
    invalid_pp.regs.rbx = 0x20_000;
    assert_fault_noncommitting(&mut invalid_pp, 6, "VPCMOV pp=01");

    for case in 0..6 {
        let mut vcpu = test_vcpu(memory_with_code(&memory_code), false);
        seed_architectural_state(&mut vcpu);
        vcpu.regs.rbx = 0x20_000;
        vcpu.sregs.cr0 |= CR0_TS;
        let (vector, name) = match case {
            0 => {
                vcpu.set_xop_enabled(false);
                (6, "CPUID.XOP=0")
            }
            1 => {
                vcpu.sregs.cr4 &= !CR4_OSXSAVE;
                (6, "CR4.OSXSAVE=0")
            }
            2 => {
                vcpu.xcr0 &= !0b100;
                (6, "XCR0.YMM=0")
            }
            3 => {
                vcpu.sregs.cr0 &= !CR0_PE;
                (6, "CR0.PE=0")
            }
            4 => {
                vcpu.regs.rflags |= flags::bits::VM;
                (6, "RFLAGS.VM=1")
            }
            5 => (7, "CR0.TS=1"),
            _ => unreachable!(),
        };
        assert_fault_noncommitting(&mut vcpu, vector, name);
    }
}

#[test]
fn direct_vpcmov_compatibility_mode_ignores_is4_high_bit_and_keeps_w_role() {
    for w in [false, true] {
        let code = xop(8, w, true, 0, 2, 0xA2, &[0xD9, 0xF0]);
        let mut vcpu = test_vcpu(memory_with_code(&code), false);
        seed_architectural_state(&mut vcpu);
        vcpu.sregs.cs.l = false;
        let before = vcpu.regs.clone();
        let source1 = vector_words(&before, 2, 32);
        let rm = vector_words(&before, 1, 32);
        let selected = vector_words(&before, 7, 32);
        let (source2, mask) = if w { (selected, rm) } else { (rm, selected) };
        let expected = vpcmov_reference(source1, source2, mask, 32);

        assert!(vcpu.step().expect("compatibility-mode VPCMOV").is_none());
        assert_eq!(vcpu.regs.xmm[3], expected[..2], "W={w}");
        assert_eq!(vcpu.regs.ymm_high[3], [expected[2], expected[3]], "W={w}");
        assert_eq!(vcpu.regs.zmm_high[3], [0; 4], "W={w}");
    }

    // VEX/XOP.R and X must be encoded as 1, and decoded vvvv values 8-15
    // are invalid outside 64-bit mode. B is architecturally ignored.
    for (name, code) in [
        ("R=0", [0x8F, 0x68, 0x6C, 0xA2, 0xD9, 0x40]),
        ("X=0", [0x8F, 0xA8, 0x6C, 0xA2, 0xD9, 0x40]),
        ("vvvv=8", [0x8F, 0xE8, 0xBC, 0xA2, 0xD9, 0x40]),
    ] {
        let mut vcpu = test_vcpu(memory_with_code(&code), false);
        seed_architectural_state(&mut vcpu);
        vcpu.sregs.cs.l = false;
        assert_fault_noncommitting(&mut vcpu, 6, name);
    }

    let b_ignored = [0x8F, 0xC8, 0x6C, 0xA2, 0xD9, 0x40];
    let mut vcpu = test_vcpu(memory_with_code(&b_ignored), false);
    seed_architectural_state(&mut vcpu);
    vcpu.sregs.cs.l = false;
    let before = vcpu.regs.clone();
    let expected = vpcmov_reference(
        vector_words(&before, 2, 32),
        vector_words(&before, 1, 32),
        vector_words(&before, 4, 32),
        32,
    );
    assert!(vcpu.step().expect("compatibility-mode B ignored").is_none());
    assert_eq!(vcpu.regs.xmm[3], expected[..2]);
    assert_eq!(vcpu.regs.ymm_high[3], [expected[2], expected[3]]);
}

#[test]
fn direct_packed_xop_reserved_fields_raise_ud_before_nm_or_memory_observation() {
    let mut forbidden_prefix = vec![0xF2];
    forbidden_prefix.extend_from_slice(&xop(9, false, false, 0, 4, 0x94, &[0x13]));
    for (name, code) in [
        ("immediate W=1", xop(8, true, false, 0, 0, 0xC0, &[0x13, 1])),
        (
            "immediate vvvv nonzero",
            xop(8, false, false, 0, 1, 0xC0, &[0x13, 1]),
        ),
        ("immediate L=1", xop(8, false, true, 0, 0, 0xC0, &[0x13, 1])),
        ("variable pp=01", xop(9, false, false, 1, 4, 0x94, &[0x13])),
        ("variable L=1", xop(9, false, true, 0, 4, 0x94, &[0x13])),
        ("forbidden F2 prefix", forbidden_prefix),
    ] {
        let mut vcpu = test_vcpu(memory_with_code(&code), false);
        seed_architectural_state(&mut vcpu);
        vcpu.sregs.cr0 |= CR0_TS;
        vcpu.regs.rbx = 0x20_000;
        assert_fault_noncommitting(&mut vcpu, 6, name);
    }
}

#[test]
fn direct_packed_xop_feature_faults_precede_nm_and_memory_faults() {
    let code = xop(9, false, false, 0, 4, 0x94, &[0x13]);
    for case in 0..5 {
        let mut vcpu = test_vcpu(memory_with_code(&code), false);
        seed_architectural_state(&mut vcpu);
        vcpu.regs.rbx = 0x20_000;
        vcpu.sregs.cr0 |= CR0_TS;
        let name = match case {
            0 => {
                vcpu.set_xop_enabled(false);
                "CPUID.XOP=0"
            }
            1 => {
                vcpu.sregs.cr4 &= !CR4_OSXSAVE;
                "CR4.OSXSAVE=0"
            }
            2 => {
                vcpu.xcr0 &= !0b100;
                "XCR0.YMM=0"
            }
            3 => {
                vcpu.sregs.cr0 &= !CR0_PE;
                "CR0.PE=0"
            }
            4 => {
                vcpu.regs.rflags |= flags::bits::VM;
                "RFLAGS.VM=1"
            }
            _ => unreachable!(),
        };
        assert_fault_noncommitting(&mut vcpu, 6, name);
    }

    let mut ts = test_vcpu(memory_with_code(&code), false);
    seed_architectural_state(&mut ts);
    ts.regs.rbx = 0x20_000;
    ts.sregs.cr0 |= CR0_TS;
    assert_fault_noncommitting(&mut ts, 7, "CR0.TS must precede memory");

    let mut memory_fault = test_vcpu(memory_with_code(&code), false);
    seed_architectural_state(&mut memory_fault);
    memory_fault.regs.rbx = 0x20_000;
    let before = memory_fault.regs.clone();
    assert!(
        memory_fault.step().is_err(),
        "enabled XOP must reach the bad memory operand"
    );
    assert_registers_equal(
        &memory_fault.regs,
        &before,
        "faulting XOP memory read must not commit",
    );
}

#[test]
fn direct_packed_xop_memory_enforces_canonical_and_alignment_priority() {
    let code = xop(9, false, false, 0, 4, 0x94, &[0x13]);
    let source = [0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210];
    let source_bytes = words_to_bytes(source);

    for (name, address, alignment_enabled) in [
        ("aligned", DATA, true),
        ("misaligned with #AC disabled", DATA + 1, false),
    ] {
        let memory = memory_with_code(&code);
        memory
            .write_slice(&source_bytes, GuestAddress(address))
            .unwrap();
        let mut vcpu = test_vcpu(memory, false);
        seed_architectural_state(&mut vcpu);
        vcpu.sregs.cs.selector = 3;
        vcpu.sregs.cr0 |= CR0_AM;
        if alignment_enabled {
            vcpu.regs.rflags |= flags::bits::AC;
        } else {
            vcpu.regs.rflags &= !flags::bits::AC;
        }
        vcpu.regs.rbx = address;
        let expected = packed_reference(source, Some(vcpu.regs.xmm[4]), None, 0x94);
        assert!(vcpu.step().expect(name).is_none());
        assert_eq!(vcpu.regs.xmm[2], expected, "{name}: destination");
    }

    let memory = memory_with_code(&code);
    memory
        .write_slice(&source_bytes, GuestAddress(DATA + 1))
        .unwrap();
    let mut misaligned = test_vcpu(memory, false);
    seed_architectural_state(&mut misaligned);
    misaligned.sregs.cs.selector = 3;
    misaligned.sregs.cr0 |= CR0_AM;
    misaligned.regs.rflags |= flags::bits::AC;
    misaligned.regs.rbx = DATA + 1;
    assert_fault_noncommitting(&mut misaligned, 17, "misaligned XOP memory source");

    let mut noncanonical = test_vcpu(memory_with_code(&code), false);
    seed_architectural_state(&mut noncanonical);
    noncanonical.sregs.cs.selector = 3;
    noncanonical.sregs.cr0 |= CR0_AM;
    noncanonical.regs.rflags |= flags::bits::AC;
    noncanonical.regs.rbx = 0x0000_8000_0000_0001;
    assert_fault_noncommitting(
        &mut noncanonical,
        13,
        "noncanonical DS address must precede #AC",
    );

    let stack_code = xop(9, false, false, 0, 4, 0x94, &[0x14, 0x24]);
    let mut stack_noncanonical = test_vcpu(memory_with_code(&stack_code), false);
    seed_architectural_state(&mut stack_noncanonical);
    stack_noncanonical.sregs.cs.selector = 3;
    stack_noncanonical.sregs.cr0 |= CR0_AM;
    stack_noncanonical.regs.rflags |= flags::bits::AC;
    stack_noncanonical.regs.rsp = 0x0000_8000_0000_0001;
    assert_fault_noncommitting(
        &mut stack_noncanonical,
        12,
        "noncanonical SS address must precede #AC",
    );
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn jit_state_backed_register_xop_matches_direct_and_clears_upper_state() {
    // VPROTD XMM3,XMM3,0x81; JMP HLT; HLT. Destination/source aliasing also
    // verifies that the state-backed lowerer snapshots its input before write.
    let mut code = xop(8, false, false, 0, 0, 0xC2, &[0xDB, 0x81]);
    code.extend_from_slice(&[0xEB, 0x00, 0xF4]);
    let mut direct = test_vcpu(memory_with_code(&code), false);
    let mut native = test_vcpu(memory_with_code(&code), false);
    seed_architectural_state(&mut direct);
    seed_architectural_state(&mut native);
    let frontier = code.len() as u64 - 1;

    run_direct_to(&mut direct, frontier);
    let region = native
        .jit_compile_region()
        .expect("compile register XOP region")
        .expect("state-backed register XOP must be native eligible");
    assert!(!region.uses_vector);
    assert!(region.uses_xmm_state);
    native.jit_run_region_native(&region);

    assert_registers_equal(&native.regs, &direct.regs, "register XOP");
    assert_eq!(native.mxcsr, direct.mxcsr);
    assert_eq!(native.regs.ymm_high[3], [0; 2]);
    assert_eq!(native.regs.zmm_high[3], [0; 4]);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn jit_xop_synchronizes_both_sides_of_a_mixed_physical_vector_region() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping mixed XOP synchronization: host lacks AVX");
        return;
    }

    // VRCPPS XMM3,XMM5 mutates an XOP source in replayed physical vector
    // state. VPSHLB consumes it through GuestRegs, then VRCPPS XMM6,XMM2
    // consumes the reloaded XOP destination in replayed physical vector state.
    // Reciprocal-estimate replay is valid with the AVX YMM0-YMM15 bridge.
    let mut code = vec![0xC5, 0xF8, 0x53, 0xDD];
    code.extend_from_slice(&xop(9, false, false, 0, 4, 0x94, &[0xD3]));
    code.extend_from_slice(&[0xC5, 0xF8, 0x53, 0xF2, 0xEB, 0x00, 0xF4]);
    let mut direct = test_vcpu(memory_with_code(&code), false);
    let mut native = test_vcpu(memory_with_code(&code), false);
    seed_architectural_state(&mut direct);
    seed_architectural_state(&mut native);
    let frontier = code.len() as u64 - 1;

    run_direct_to(&mut direct, frontier);
    let region = native
        .jit_compile_region()
        .expect("compile mixed physical/state-backed XOP region")
        .expect("mixed XOP region must be native eligible");
    assert!(region.uses_vector);
    assert!(region.uses_xmm_state);
    assert!(region.avx_ymm16_vector_state);
    native.jit_run_region_native(&region);

    assert_registers_equal(&native.regs, &direct.regs, "mixed XOP region");
    assert_eq!(native.mxcsr, direct.mxcsr);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn jit_helper_backed_xop_memory_source_and_count_match_direct() {
    let memory_value = [0x8123_4567_89AB_CDEF, 0x9234_7E81_9ABC_5678];
    for (name, w) in [("memory source", false), ("memory count", true)] {
        let mut code = xop(9, w, false, 0, 4, 0x99, &[0x13]);
        code.extend_from_slice(&[0xEB, 0x00, 0xF4]);
        let direct_memory = memory_with_code(&code);
        let native_memory = memory_with_code(&code);
        for memory in [&direct_memory, &native_memory] {
            memory
                .write_slice(&words_to_bytes(memory_value), GuestAddress(DATA))
                .unwrap();
        }
        let mut direct = test_vcpu(direct_memory, true);
        let mut native = test_vcpu(native_memory, true);
        seed_architectural_state(&mut direct);
        seed_architectural_state(&mut native);
        let frontier = code.len() as u64 - 1;

        run_direct_to(&mut direct, frontier);
        let region = native
            .jit_compile_region()
            .unwrap_or_else(|error| panic!("{name}: compile error: {error:?}"))
            .unwrap_or_else(|| panic!("{name}: helper-backed XOP must be native eligible"));
        assert!(!region.uses_vector, "{name}");
        assert!(region.uses_xmm_state, "{name}");
        native.jit_run_region_native(&region);

        assert_registers_equal(&native.regs, &direct.regs, name);
        assert_eq!(native.mxcsr, direct.mxcsr, "{name}: MXCSR");
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn jit_mixed_vector_xop_memory_syncs_the_live_register_operand() {
    if !std::is_x86_feature_detected!("avx") {
        eprintln!("skipping mixed XOP memory synchronization: host lacks AVX");
        return;
    }

    let memory_value = [0x8123_4567_89AB_CDEF, 0x9234_7E81_9ABC_5678];
    for (name, w) in [("memory source", false), ("memory count", true)] {
        // VRCPPS XMM4,XMM5 updates the non-memory XOP operand in replayed
        // physical state. The helper-backed XOP must publish that register to
        // GuestRegs, and the following VRCPPS must observe the reloaded result.
        let mut code = vec![0xC5, 0xF8, 0x53, 0xE5];
        code.extend_from_slice(&xop(9, w, false, 0, 4, 0x99, &[0x13]));
        code.extend_from_slice(&[0xC5, 0xF8, 0x53, 0xF2, 0xEB, 0x00, 0xF4]);
        let direct_memory = memory_with_code(&code);
        let native_memory = memory_with_code(&code);
        for memory in [&direct_memory, &native_memory] {
            memory
                .write_slice(&words_to_bytes(memory_value), GuestAddress(DATA))
                .unwrap();
        }
        let mut direct = test_vcpu(direct_memory, true);
        let mut native = test_vcpu(native_memory, true);
        seed_architectural_state(&mut direct);
        seed_architectural_state(&mut native);
        let frontier = code.len() as u64 - 1;

        let initial_register_operand = direct.regs.xmm[4];
        assert!(direct.step().expect("direct VRCPPS producer").is_none());
        assert_ne!(
            direct.regs.xmm[4], initial_register_operand,
            "{name}: producer did not make stale GuestRegs state observable"
        );
        run_direct_to(&mut direct, frontier);
        let region = native
            .jit_compile_region()
            .unwrap_or_else(|error| panic!("{name}: compile error: {error:?}"))
            .unwrap_or_else(|| panic!("{name}: mixed helper-backed XOP must be native eligible"));
        assert!(region.uses_vector, "{name}");
        assert!(region.uses_xmm_state, "{name}");
        assert!(region.avx_ymm16_vector_state, "{name}");
        native.jit_run_region_native(&region);

        assert_registers_equal(&native.regs, &direct.regs, name);
        assert_eq!(native.mxcsr, direct.mxcsr, "{name}: MXCSR");
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn jit_xop_alignment_guard_uses_guest_ac_shadow_and_is_noncommitting() {
    let mut code = xop(9, false, false, 0, 4, 0x94, &[0x13]);
    code.extend_from_slice(&[0xEB, 0x00, 0xF4]);
    let memory = memory_with_code(&code);
    memory
        .write_slice(
            &words_to_bytes([0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210]),
            GuestAddress(DATA + 1),
        )
        .unwrap();
    let mut native = test_vcpu(memory, true);
    seed_architectural_state(&mut native);
    native.sregs.cs.selector = 3;
    native.sregs.cr0 |= CR0_AM;
    native.regs.rflags |= flags::bits::AC;
    native.regs.rbx = DATA + 1;

    let region = native
        .jit_compile_region()
        .expect("compile dynamically aligned XOP memory region")
        .expect("dynamic #AC guard must be native eligible");
    assert!(region.uses_xmm_state);
    let before = native.regs.clone();
    native.jit_run_region_native(&region);

    assert_registers_equal(
        &native.regs,
        &before,
        "native #AC deoptimization must not commit",
    );
    assert_fault_noncommitting(&mut native, 17, "direct replay after native #AC");
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn jit_xop_dynamic_state_guards_handoff_to_exact_ud_or_nm_frontier() {
    let mut code = xop(9, false, false, 0, 4, 0x94, &[0xD3]);
    code.extend_from_slice(&[0xEB, 0x00, 0xF4]);
    for case in 0..6 {
        let mut native = test_vcpu(memory_with_code(&code), false);
        seed_architectural_state(&mut native);
        let region = native
            .jit_compile_region()
            .expect("compile dynamically guarded XOP region")
            .expect("dynamic XOP state must not prevent admission");
        let (name, vector) = match case {
            0 => {
                native.set_xop_enabled(false);
                ("CPUID.XOP=0", 6)
            }
            1 => {
                native.sregs.cr4 &= !CR4_OSXSAVE;
                ("CR4.OSXSAVE=0", 6)
            }
            2 => {
                native.xcr0 &= !0b100;
                ("XCR0.YMM=0", 6)
            }
            3 => {
                native.sregs.cr0 &= !CR0_PE;
                ("CR0.PE=0", 6)
            }
            4 => {
                native.regs.rflags |= flags::bits::VM;
                ("RFLAGS.VM=1", 6)
            }
            5 => {
                native.sregs.cr0 |= CR0_TS;
                ("CR0.TS=1", 7)
            }
            _ => unreachable!(),
        };
        let before = native.regs.clone();
        native.jit_run_region_native(&region);
        assert_registers_equal(&native.regs, &before, name);
        assert_fault_noncommitting(&mut native, vector, name);
    }
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn jit_xop_long_mode_guard_deopts_to_supported_compatibility_execution() {
    let mut code = xop(9, false, false, 0, 4, 0x94, &[0xD3]);
    code.extend_from_slice(&[0xEB, 0x00, 0xF4]);
    let mut native = test_vcpu(memory_with_code(&code), false);
    seed_architectural_state(&mut native);
    let region = native
        .jit_compile_region()
        .expect("compile long-mode XOP region")
        .expect("long-mode XOP must be native eligible");
    native.sregs.cs.l = false;
    native.sregs.cs.db = true;
    let before = native.regs.clone();

    native.jit_run_region_native(&region);

    assert_registers_equal(&native.regs, &before, "compatibility-mode native handoff");
    assert!(
        native
            .step()
            .expect("direct compatibility-mode XOP")
            .is_none()
    );
    assert_eq!(native.regs.rip, 5);
}

#[cfg(all(feature = "smir-jit", target_arch = "x86_64"))]
#[test]
fn jit_callout_synchronizes_state_backed_xop_without_vector_trampoline() {
    use crate::smir::lower::runtime::GuestRegs;

    let memory = memory_with_code(&[]);
    // VPROTD XMM1,XMM1,7; RET.
    memory
        .write_slice(
            &xop(8, false, false, 0, 0, 0xC2, &[0xC9, 7])
                .into_iter()
                .chain([0xC3])
                .collect::<Vec<_>>(),
            GuestAddress(0x100),
        )
        .unwrap();
    let mut vcpu = test_vcpu(memory, false);
    let original = [
        0x0123_4567_89AB_CDEF,
        0x9234_7E81_9ABC_5678,
        0x2122_2324_2526_2728,
        0x3132_3334_3536_3738,
        0x4142_4344_4546_4748,
        0x5152_5354_5556_5758,
        0x6162_6364_6566_6768,
        0x7172_7374_7576_7778,
    ];
    let expected_low = packed_reference([original[0], original[1]], None, Some(7), 0xC2);
    let mut gr = GuestRegs {
        ctx: (&mut vcpu as *mut X86_64Vcpu) as u64,
        rflags: 0x2 | 0x08D5 | flags::bits::DF,
        cr0: CR0_PE,
        cr4: CR4_OSFXSR | CR4_OSXSAVE,
        xcr0: 0b110,
        cpuid_xop: 1,
        xmm_state_active: 1,
        ..GuestRegs::default()
    };
    gr.gpr[4] = 0x8000;
    gr.set_zmm(1, original);
    gr.set_zmm(2, [0x2222_2222_2222_2222; 8]);

    let ok = unsafe { rax_jit_call(&mut gr, 0x100, 0x200, 0x80) };

    assert_eq!(ok, 1);
    assert_eq!(
        gr.get_zmm(1),
        [expected_low[0], expected_low[1], 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(gr.get_zmm(2), [0x2222_2222_2222_2222; 8]);
    assert_eq!(gr.gpr[4], 0x8000, "CALL/RET stack balance");
    assert_eq!(gr.cpuid_xop, 1);
    assert_eq!(gr.vector_active, 0);
}
