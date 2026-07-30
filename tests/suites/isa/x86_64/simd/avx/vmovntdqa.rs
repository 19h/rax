use crate::common::{CODE_ADDR, run_until_hlt, setup_vm};
use rax::vm::vcpu::Registers;
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

// VMOVNTDQA - Load Non-Temporal Packed Integer

const ALIGNED_ADDR: u64 = 0x3000;
const SOURCE_WORDS: [u64; 4] = [
    0x0123_4567_89ab_cdef,
    0xfedc_ba98_7654_3210,
    0x0f1e_2d3c_4b5a_6978,
    0x8877_6655_4433_2211,
];

fn absolute_vmovntdqa(dest: usize, width: usize, vex_w: bool) -> [u8; 11] {
    assert!(dest < 16);
    assert!(matches!(width, 16 | 32));

    let vex_rxb_map = if dest >= 8 { 0x62 } else { 0xe2 };
    let vex_w_vvvv_l_pp = (u8::from(vex_w) << 7) | 0x78 | (u8::from(width == 32) << 2) | 0x01;
    let modrm = ((dest as u8 & 0x07) << 3) | 0x04;

    [
        0xc4,
        vex_rxb_map,
        vex_w_vvvv_l_pp,
        0x2a,
        modrm,
        0x25, // SIB: no index, no base; absolute disp32.
        0x00,
        0x30,
        0x00,
        0x00,
        0xf4,
    ]
}

fn initialized_vector_state() -> Registers {
    let mut regs = Registers::default();
    for index in 0..regs.xmm.len() {
        let tag = index as u64;
        regs.xmm[index] = [0xa100_0000_0000_0000 | tag, 0xa200_0000_0000_0000 | tag];
        regs.ymm_high[index] = [0xb100_0000_0000_0000 | tag, 0xb200_0000_0000_0000 | tag];
        regs.zmm_high[index] = [
            0xc100_0000_0000_0000 | tag,
            0xc200_0000_0000_0000 | tag,
            0xc300_0000_0000_0000 | tag,
            0xc400_0000_0000_0000 | tag,
        ];
    }
    regs
}

fn write_source(memory: &GuestMemoryMmap) {
    for (index, word) in SOURCE_WORDS.iter().enumerate() {
        memory
            .write_slice(
                &word.to_le_bytes(),
                GuestAddress(ALIGNED_ADDR + (index * 8) as u64),
            )
            .unwrap();
    }
}

fn assert_loaded_state(
    actual: &Registers,
    initial: &Registers,
    dest: usize,
    width: usize,
    case: &str,
) {
    let mut expected_xmm = initial.xmm;
    let mut expected_ymm_high = initial.ymm_high;
    let mut expected_zmm_high = initial.zmm_high;
    expected_xmm[dest] = [SOURCE_WORDS[0], SOURCE_WORDS[1]];
    expected_ymm_high[dest] = if width == 32 {
        [SOURCE_WORDS[2], SOURCE_WORDS[3]]
    } else {
        [0; 2]
    };
    expected_zmm_high[dest] = [0; 4];

    assert_eq!(actual.xmm, expected_xmm, "{case}: XMM state");
    assert_eq!(
        actual.ymm_high, expected_ymm_high,
        "{case}: YMM upper state"
    );
    assert_eq!(
        actual.zmm_high, expected_zmm_high,
        "{case}: ZMM upper state"
    );
}

#[test]
fn vmovntdqa_loads_all_vex_destinations_widths_and_wig_values() {
    for dest in 0..16 {
        for width in [16, 32] {
            for vex_w in [false, true] {
                let code = absolute_vmovntdqa(dest, width, vex_w);
                let initial = initialized_vector_state();
                let (mut vcpu, memory) = setup_vm(&code, Some(initial.clone()));
                write_source(memory.as_ref());

                let actual = run_until_hlt(&mut vcpu).unwrap();
                let case = format!("dest={dest}, width={width}, W={}", u8::from(vex_w));
                assert_loaded_state(&actual, &initial, dest, width, &case);
            }
        }
    }
}

#[test]
fn vmovntdqa_rip_relative_displacement_is_relative_to_next_instruction() {
    const INSTRUCTION_LEN: u64 = 9;
    let displacement = i32::try_from(ALIGNED_ADDR - (CODE_ADDR + INSTRUCTION_LEN)).unwrap();
    let displacement = displacement.to_le_bytes();
    let code = [
        0xc4,
        0x62,
        0x7d,
        0x2a,
        0x35, // VMOVNTDQA ymm14, m256.
        displacement[0],
        displacement[1],
        displacement[2],
        displacement[3],
        0xf4,
    ];
    let initial = initialized_vector_state();
    let (mut vcpu, memory) = setup_vm(&code, Some(initial.clone()));
    write_source(memory.as_ref());

    let actual = run_until_hlt(&mut vcpu).unwrap();
    assert_loaded_state(&actual, &initial, 14, 32, "RIP-relative");
}
