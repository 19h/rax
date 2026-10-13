//! Lengths, targets, and stack effects from the stateless metadata decoder
//! for operand-size prefixes in 64-bit mode.

use super::metadata::decode;

#[test]
fn near_branches_ignore_66_in_64_bit_mode() {
    // Intel SDM Vol. 1 section 6.4.7: near-branch operand size is forced to
    // 64 bits and operand-size prefixes are ignored, while a relative
    // displacement stays 32 bits.
    let call = decode(64, 0x1000, &[0x66, 0xE8, 0x10, 0, 0, 0]).unwrap();
    assert_eq!(
        (call.size, call.target, call.stack_pointer_increment),
        (6, Some(0x1016), -8)
    );
    let jmp = decode(64, 0x1000, &[0x66, 0xE9, 0x10, 0, 0, 0]).unwrap();
    assert_eq!((jmp.size, jmp.target), (6, Some(0x1016)));
    let jz = decode(64, 0x1000, &[0x66, 0x0F, 0x84, 0x10, 0, 0, 0]).unwrap();
    assert_eq!((jz.size, jz.target), (7, Some(0x1017)));
    let ret = decode(64, 0x1000, &[0x66, 0xC3]).unwrap();
    assert_eq!(ret.stack_pointer_increment, 8);
    let call_rax = decode(64, 0x1000, &[0x66, 0xFF, 0xD0]).unwrap();
    assert_eq!(call_rax.stack_pointer_increment, -8);

    // Outside 64-bit mode 66 still selects rel16 and a 16-bit return address.
    let call16 = decode(32, 0x1000, &[0x66, 0xE8, 0x10, 0]).unwrap();
    assert_eq!((call16.size, call16.stack_pointer_increment), (4, -2));
}

#[test]
fn stack_operations_honor_66_unless_rex_w() {
    // PUSH imm: 66 selects the 16-bit push of an imm16; REX.W selects the
    // 64-bit push of a sign-extended imm32 and takes precedence over 66
    // (Intel SDM Vol. 2A section 2.2.1.2).
    let push16 = decode(64, 0, &[0x66, 0x68, 0x34, 0x12]).unwrap();
    assert_eq!((push16.size, push16.stack_pointer_increment), (4, -2));
    let push64 = decode(64, 0, &[0x66, 0x48, 0x68, 0x78, 0x56, 0x34, 0x12]).unwrap();
    assert_eq!((push64.size, push64.stack_pointer_increment), (7, -8));
    // PUSH r/m16 (66 FF /6).
    let push_m16 = decode(64, 0, &[0x66, 0xFF, 0x30]).unwrap();
    assert_eq!(push_m16.stack_pointer_increment, -2);
}
