//! Architectural encoding and explicit-length-width coverage for PCMPxSTRx.

use crate::common::*;
use rax::vm::vcpu::Registers;
use vm_memory::{Bytes, GuestAddress};

const DATA_ADDR: u64 = 0x3000;
const YMM0_HIGH: [u64; 2] = [0x0123_4567_89ab_cdef, 0xfedc_ba98_7654_3210];
const ZMM0_HIGH: [u64; 4] = [
    0x1111_2222_3333_4444,
    0x5555_6666_7777_8888,
    0x9999_aaaa_bbbb_cccc,
    0xdddd_eeee_ffff_0000,
];

fn run_explicit(opcode: u8, rex_w: bool, rax: u64) -> Registers {
    let mut code = vec![0x48, 0xBB]; // MOV RBX, DATA_ADDR
    code.extend_from_slice(&DATA_ADDR.to_le_bytes());
    code.extend_from_slice(&[
        0xF3, 0x0F, 0x6F, 0x0B, // MOVDQU XMM1, [RBX]
        0xF3, 0x0F, 0x6F, 0x53, 0x10, // MOVDQU XMM2, [RBX+16]
        0x48, 0xB8, // MOV RAX, explicit first-operand length
    ]);
    code.extend_from_slice(&rax.to_le_bytes());
    code.extend_from_slice(&[
        0xBA, 0x01, 0x00, 0x00, 0x00, // MOV EDX, 1
        0x66,
    ]);
    if rex_w {
        code.push(0x48);
    }
    code.extend_from_slice(&[0x0F, 0x3A, opcode, 0xCA, 0x00, 0xF4]);

    let mut initial = Registers::default();
    initial.ymm_high[0] = YMM0_HIGH;
    initial.zmm_high[0] = ZMM0_HIGH;
    let (mut vcpu, memory) = setup_vm(&code, Some(initial));
    let mut strings = [0u8; 32];
    strings[0] = b'A';
    strings[16] = b'A';
    memory
        .write_slice(&strings, GuestAddress(DATA_ADDR))
        .unwrap();
    run_until_hlt(&mut vcpu).unwrap()
}

#[test]
fn pcmpestrx_rex_w_selects_signed_64_bit_lengths_and_preserves_upper_state() {
    // EAX is zero in both cases. Positive 2^32 and negative -2^63 RAX lengths
    // both have absolute magnitudes that saturate to 16 valid bytes.
    for rax in [0x0000_0001_0000_0000, i64::MIN as u64] {
        assert_eq!(run_explicit(0x61, false, rax).rcx, 16);
        assert_eq!(run_explicit(0x61, true, rax).rcx, 0);

        let mask32 = run_explicit(0x60, false, rax);
        assert_eq!(mask32.xmm[0], [0, 0]);
        assert_eq!(mask32.ymm_high[0], YMM0_HIGH);
        assert_eq!(mask32.zmm_high[0], ZMM0_HIGH);

        let mask64 = run_explicit(0x60, true, rax);
        assert_eq!(mask64.xmm[0], [1, 0]);
        assert_eq!(mask64.ymm_high[0], YMM0_HIGH);
        assert_eq!(mask64.zmm_high[0], ZMM0_HIGH);
    }
}

#[test]
fn legacy_pcmpxstrx_rejects_missing_or_conflicting_mandatory_prefixes() {
    for (name, instruction) in [
        ("missing-66", &[0x0F, 0x3A, 0x63, 0xC1, 0x00][..]),
        ("f2-66", &[0xF2, 0x66, 0x0F, 0x3A, 0x63, 0xC1, 0x00][..]),
        ("66-f2", &[0x66, 0xF2, 0x0F, 0x3A, 0x63, 0xC1, 0x00][..]),
        ("f3-66", &[0xF3, 0x66, 0x0F, 0x3A, 0x63, 0xC1, 0x00][..]),
        ("66-f3", &[0x66, 0xF3, 0x0F, 0x3A, 0x63, 0xC1, 0x00][..]),
        ("lock", &[0xF0, 0x66, 0x0F, 0x3A, 0x63, 0xC1, 0x00][..]),
        (
            "rex2",
            &[0x66, 0xD5, 0x00, 0x0F, 0x3A, 0x63, 0xC1, 0x00][..],
        ),
    ] {
        let mut code = instruction.to_vec();
        code.push(0xF4);
        let (mut vcpu, _) = setup_vm_no_idt(&code, None);
        let error = match vcpu.run() {
            Err(error) => error,
            Ok(exit) => panic!("{name}: invalid encoding reached {exit:?}"),
        };
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name}: expected #UD delivery failure, got {error}",
        );
    }
}
