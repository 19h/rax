//! End-to-end native-JIT coverage for register-only legacy SSE4.1
//! PMOVSX*/PMOVZX* instructions.

use super::*;

#[derive(Clone, Copy, Debug)]
struct Operation {
    name: &'static str,
    opcode: u8,
    source_bits: u8,
    destination_bits: u8,
    signed: bool,
}

const OPERATIONS: [Operation; 12] = [
    Operation {
        name: "PMOVSXBW",
        opcode: 0x20,
        source_bits: 8,
        destination_bits: 16,
        signed: true,
    },
    Operation {
        name: "PMOVSXBD",
        opcode: 0x21,
        source_bits: 8,
        destination_bits: 32,
        signed: true,
    },
    Operation {
        name: "PMOVSXBQ",
        opcode: 0x22,
        source_bits: 8,
        destination_bits: 64,
        signed: true,
    },
    Operation {
        name: "PMOVSXWD",
        opcode: 0x23,
        source_bits: 16,
        destination_bits: 32,
        signed: true,
    },
    Operation {
        name: "PMOVSXWQ",
        opcode: 0x24,
        source_bits: 16,
        destination_bits: 64,
        signed: true,
    },
    Operation {
        name: "PMOVSXDQ",
        opcode: 0x25,
        source_bits: 32,
        destination_bits: 64,
        signed: true,
    },
    Operation {
        name: "PMOVZXBW",
        opcode: 0x30,
        source_bits: 8,
        destination_bits: 16,
        signed: false,
    },
    Operation {
        name: "PMOVZXBD",
        opcode: 0x31,
        source_bits: 8,
        destination_bits: 32,
        signed: false,
    },
    Operation {
        name: "PMOVZXBQ",
        opcode: 0x32,
        source_bits: 8,
        destination_bits: 64,
        signed: false,
    },
    Operation {
        name: "PMOVZXWD",
        opcode: 0x33,
        source_bits: 16,
        destination_bits: 32,
        signed: false,
    },
    Operation {
        name: "PMOVZXWQ",
        opcode: 0x34,
        source_bits: 16,
        destination_bits: 64,
        signed: false,
    },
    Operation {
        name: "PMOVZXDQ",
        opcode: 0x35,
        source_bits: 32,
        destination_bits: 64,
        signed: false,
    },
];

fn extract_lane(vector: &[u64; 2], lane: usize, bits: u8) -> u64 {
    let bit = lane * usize::from(bits);
    let word = bit / 64;
    let shift = bit % 64;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    (vector[word] >> shift) & mask
}

fn insert_lane(vector: &mut [u64; 2], lane: usize, bits: u8, value: u64) {
    let bit = lane * usize::from(bits);
    let word = bit / 64;
    let shift = bit % 64;
    let mask = if bits == 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    vector[word] = (vector[word] & !(mask << shift)) | ((value & mask) << shift);
}

fn apply_operation(xmm: &mut [[u64; 2]; 16], operation: Operation, modrm: u8) {
    let destination = usize::from((modrm >> 3) & 7);
    let source = xmm[usize::from(modrm & 7)];
    let mut result = [0u64; 2];
    let lanes = 128 / usize::from(operation.destination_bits);
    for lane in 0..lanes {
        let raw = extract_lane(&source, lane, operation.source_bits);
        let extended = if operation.signed {
            let shift = 64 - u32::from(operation.source_bits);
            (((raw << shift) as i64) >> shift) as u64
        } else {
            raw
        };
        insert_lane(&mut result, lane, operation.destination_bits, extended);
    }
    xmm[destination] = result;
}

fn setup(vcpu: &mut X86_64Vcpu, profile: usize) -> Registers {
    let mut sregs = vcpu.get_sregs().unwrap();
    sregs.cr4 |= 1 << 9; // CR4.OSFXSR
    vcpu.set_sregs(&sregs).unwrap();

    let mut registers = vcpu.get_regs().unwrap();
    registers.rax = 0x0123_4567_89AB_CDEF ^ profile as u64;
    registers.rbx = 0xFEDC_BA98_7654_3210 ^ (profile as u64).rotate_left(13);
    registers.rcx = 0x8000_0000_0000_0001;
    registers.rdx = 0x7FFF_FFFF_FFFF_FFFE;
    registers.rsi = 0x1111_2222_3333_4444;
    registers.rdi = 0x5555_6666_7777_8888;
    registers.rflags = 0x2 | 0x08D5 | (1 << 10) | (1 << 18) | (3 << 12);
    registers.mm = std::array::from_fn(|index| {
        0xA100_0000_0000_0000 | ((profile as u64) << 16) | index as u64
    });
    registers.k = std::array::from_fn(|index| {
        0x6996_F00F_3CC3_A55Au64.rotate_left((index * 7 + profile) as u32)
    });
    for index in 0..16 {
        registers.xmm[index] = [
            0x0080_7FFF_0181_FEFFu64.rotate_left((index * 5 + profile * 3) as u32)
                ^ (index as u64).wrapping_mul(0x0101_1111_2222_3333),
            0x8000_0001_7FFF_FFFFu64.rotate_left((index * 11 + profile * 7) as u32)
                ^ (index as u64).wrapping_mul(0x8040_2010_0804_0201),
        ];
        registers.ymm_high[index] = [
            0xB100_0000_0000_0000 | ((profile as u64) << 16) | index as u64,
            0xB200_0000_0000_0000 | ((profile as u64) << 16) | index as u64,
        ];
        registers.zmm_high[index] = std::array::from_fn(|word| {
            0xC000_0000_0000_0000 | ((word as u64) << 56) | ((profile as u64) << 16) | index as u64
        });
        registers.zmm_ext[index] = std::array::from_fn(|word| {
            0xD000_0000_0000_0000 | ((word as u64) << 56) | ((profile as u64) << 16) | index as u64
        });
    }
    vcpu.set_regs(&registers).unwrap();
    registers
}

/// The independent scanner enumerates 64 ModR/M register cells for the
/// canonical `66` encoding and the same 64 cells with ignored `REX.W=1`, for
/// each of the twelve legacy opcodes: 12 × 2 × 64 = 1,536 cells.
#[test]
fn jit_all_1536_scanner_legacy_packed_extend_gaps_match_direct_and_intel_equations() {
    assert!(std::is_x86_feature_detected!("sse4.1"));
    assert!(std::is_x86_feature_detected!("avx"));

    let mut cases = 0usize;
    for (operation_index, operation) in OPERATIONS.into_iter().enumerate() {
        for (prefix_index, prefix) in [&[0x66][..], &[0x66, 0x48][..]].into_iter().enumerate() {
            let mut code = Vec::new();
            for modrm in 0xC0..=0xFF {
                code.extend_from_slice(prefix);
                code.extend_from_slice(&[0x0F, 0x38, operation.opcode, modrm]);
                cases += 1;
            }
            code.push(0xF4);
            let profile = operation_index * 2 + prefix_index;

            let mut direct = make_vcpu_code(&code);
            let initial = setup(&mut direct, profile);
            let mut manual_xmm = initial.xmm;
            for modrm in 0xC0..=0xFF {
                apply_operation(&mut manual_xmm, operation, modrm);
            }
            run_interp(&mut direct);
            let expected = direct.get_regs().unwrap();
            assert_eq!(
                expected.xmm, manual_xmm,
                "{} {prefix:02X?}: direct result vs Intel equations",
                operation.name
            );
            assert_eq!(expected.ymm_high, initial.ymm_high, "{}", operation.name);
            assert_eq!(expected.zmm_high, initial.zmm_high, "{}", operation.name);
            assert_eq!(expected.zmm_ext, initial.zmm_ext, "{}", operation.name);
            assert_eq!(expected.rflags, initial.rflags, "{}", operation.name);

            let mut jit = make_vcpu_code(&code);
            setup(&mut jit, profile);
            jit.set_jit_call(false);
            jit.set_jit_mem(false);
            assert!(
                jit.jit_try_block().unwrap_or_else(|error| panic!(
                    "{} {prefix:02X?}: native admission: {error:?}",
                    operation.name
                )),
                "{} {prefix:02X?}: all register cells must enter the native tier:\n{}",
                operation.name,
                jit.jit_dump_region(LOAD_ADDR)
            );
            assert_eq!(
                jit.get_regs().unwrap().rip,
                LOAD_ADDR + code.len() as u64 - 1,
                "{} {prefix:02X?}: HLT frontier",
                operation.name
            );
            run_interp(&mut jit);
            let actual = jit.get_regs().unwrap();

            assert_eq!(actual.xmm, expected.xmm, "{}: XMM", operation.name);
            assert_eq!(
                actual.ymm_high, expected.ymm_high,
                "{}: YMM",
                operation.name
            );
            assert_eq!(
                actual.zmm_high, expected.zmm_high,
                "{}: ZMM",
                operation.name
            );
            assert_eq!(
                actual.zmm_ext, expected.zmm_ext,
                "{}: ZMM16-31",
                operation.name
            );
            assert_eq!(actual.k, expected.k, "{}: opmask", operation.name);
            assert_eq!(actual.mm, expected.mm, "{}: MMX", operation.name);
            assert_eq!(actual.rax, expected.rax, "{}: RAX", operation.name);
            assert_eq!(actual.rbx, expected.rbx, "{}: RBX", operation.name);
            assert_eq!(actual.rcx, expected.rcx, "{}: RCX", operation.name);
            assert_eq!(actual.rdx, expected.rdx, "{}: RDX", operation.name);
            assert_eq!(actual.rsi, expected.rsi, "{}: RSI", operation.name);
            assert_eq!(actual.rdi, expected.rdi, "{}: RDI", operation.name);
            assert_eq!(actual.rflags, expected.rflags, "{}: RFLAGS", operation.name);
            assert_eq!(actual.rip, expected.rip, "{}: RIP", operation.name);
        }
    }
    assert_eq!(cases, 12 * 2 * 64);
}
