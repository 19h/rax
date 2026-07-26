//! Direct-execution regressions for EVEX COMI/UCOMI scalar comparisons.

use std::sync::Arc;

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::isa::x86_64::flags;
use crate::vm::vcpu::VCpu;

const CODE: u64 = 0x1000;
const DATA: u64 = 0x3000;
const STATUS_FLAGS: u64 = flags::bits::CF
    | flags::bits::PF
    | flags::bits::AF
    | flags::bits::ZF
    | flags::bits::SF
    | flags::bits::OF;
const INITIAL_FLAGS: u64 = 0x2 | STATUS_FLAGS | flags::bits::DF;

fn vcpu(code: &[u8]) -> X86_64Vcpu {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(CODE)).unwrap();
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.regs.rip = CODE;
    vcpu.regs.rflags = INITIAL_FLAGS;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.db = false;
    vcpu
}

fn encoding(
    elem_size: usize,
    ordered: bool,
    source1: u8,
    source2: u8,
    ll: u8,
    suppress_exceptions: bool,
) -> [u8; 6] {
    assert!(matches!(elem_size, 2 | 4 | 8));
    assert!(source1 < 32 && source2 < 32 && ll < 4);
    let mut p0 = match elem_size {
        2 => 0xF5,
        4 | 8 => 0xF1,
        _ => unreachable!(),
    };
    if source1 & 0x08 != 0 {
        p0 &= !0x80;
    }
    if source1 & 0x10 != 0 {
        p0 &= !0x10;
    }
    if source2 & 0x08 != 0 {
        p0 &= !0x20;
    }
    if source2 & 0x10 != 0 {
        p0 &= !0x40;
    }
    [
        0x62,
        p0,
        if elem_size == 8 { 0xFD } else { 0x7C },
        (ll << 5) | 0x08 | if suppress_exceptions { 0x10 } else { 0 },
        if ordered { 0x2F } else { 0x2E },
        0xC0 | ((source1 & 7) << 3) | (source2 & 7),
    ]
}

fn set_scalar(vcpu: &mut X86_64Vcpu, register: u8, value: u64) {
    if register < 16 {
        vcpu.regs.xmm[register as usize][0] = value;
    } else {
        vcpu.regs.zmm_ext[(register - 16) as usize][0] = value;
    }
}

fn patterns(elem_size: usize) -> (u64, u64, u64, u64, u64) {
    match elem_size {
        // one, two, QNaN, SNaN, minimum positive denormal
        2 => (0x3C00, 0x4000, 0x7E01, 0x7C01, 0x0001),
        4 => (
            0x3F80_0000,
            0x4000_0000,
            0x7FC0_0001,
            0x7F80_0001,
            0x0000_0001,
        ),
        8 => (
            0x3FF0_0000_0000_0000,
            0x4000_0000_0000_0000,
            0x7FF8_0000_0000_0001,
            0x7FF0_0000_0000_0001,
            0x0000_0000_0000_0001,
        ),
        _ => unreachable!(),
    }
}

#[test]
fn evex_comi_covers_truth_table_nan_classes_llig_extensions_aliases_and_state() {
    let register_pairs = [(1, 2), (9, 10), (17, 18), (25, 26), (31, 31)];
    for elem_size in [2, 4, 8] {
        let (one, two, qnan, snan, _) = patterns(elem_size);
        for ll in 0..3 {
            for (case, (first, second, expected)) in [
                (one, one, flags::bits::ZF),
                (one, two, flags::bits::CF),
                (two, one, 0),
                (0, 1u64 << (elem_size * 8 - 1), flags::bits::ZF),
            ]
            .into_iter()
            .enumerate()
            {
                let (source1, source2) = register_pairs[(case + ll as usize) % 4];
                let code = encoding(elem_size, false, source1, source2, ll, false);
                let mut vcpu = vcpu(&code);
                set_scalar(&mut vcpu, source1, first);
                set_scalar(&mut vcpu, source2, second);
                let vectors_before = (
                    vcpu.regs.xmm,
                    vcpu.regs.ymm_high,
                    vcpu.regs.zmm_high,
                    vcpu.regs.zmm_ext,
                );
                assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");
                assert_eq!(vcpu.regs.rflags & STATUS_FLAGS, expected, "{code:02X?}");
                assert_ne!(vcpu.regs.rflags & flags::bits::DF, 0, "{code:02X?}");
                assert_eq!(vcpu.regs.rip, CODE + 6, "{code:02X?}");
                assert_eq!(
                    (
                        vcpu.regs.xmm,
                        vcpu.regs.ymm_high,
                        vcpu.regs.zmm_high,
                        vcpu.regs.zmm_ext,
                    ),
                    vectors_before,
                    "{code:02X?}"
                );
            }

            let (source1, source2) = register_pairs[ll as usize];
            for (ordered, value, invalid) in [
                (true, qnan, true),
                (false, qnan, false),
                (true, snan, true),
                (false, snan, true),
            ] {
                let code = encoding(elem_size, ordered, source1, source2, ll, false);
                let mut vcpu = vcpu(&code);
                set_scalar(&mut vcpu, source1, value);
                set_scalar(&mut vcpu, source2, one);
                assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");
                assert_eq!(
                    vcpu.regs.rflags & STATUS_FLAGS,
                    flags::bits::ZF | flags::bits::PF | flags::bits::CF,
                    "{code:02X?}"
                );
                assert_eq!(vcpu.mxcsr & 1 != 0, invalid, "{code:02X?}");
            }
        }

        let code = encoding(elem_size, false, 31, 31, 3, true);
        let mut alias = vcpu(&code);
        set_scalar(&mut alias, 31, one);
        assert!(alias.step().unwrap().is_none());
        assert_eq!(alias.regs.rflags & STATUS_FLAGS, flags::bits::ZF);
    }
}

#[test]
fn evex_comi_handles_fp16_denormals_fp32_fp64_daz_and_sae() {
    for elem_size in [2, 4, 8] {
        let (_, _, _, _, denormal) = patterns(elem_size);
        for daz in [false, true] {
            let code = encoding(elem_size, true, 17, 25, 2, false);
            let mut vcpu = vcpu(&code);
            vcpu.mxcsr = 0x1F80 | if daz { 1 << 6 } else { 0 };
            set_scalar(&mut vcpu, 17, denormal);
            set_scalar(&mut vcpu, 25, 0);
            assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");
            let fp16_or_no_daz = elem_size == 2 || !daz;
            assert_eq!(
                vcpu.regs.rflags & STATUS_FLAGS,
                if fp16_or_no_daz { 0 } else { flags::bits::ZF },
                "element={elem_size} DAZ={daz}"
            );
            assert_eq!(
                vcpu.mxcsr & (1 << 1) != 0,
                fp16_or_no_daz,
                "element={elem_size} DAZ={daz}"
            );
        }
    }

    let (_, _, qnan, _, _) = patterns(2);
    for ll in 0..4 {
        let code = encoding(2, true, 1, 2, ll, true);
        let mut vcpu = vcpu(&code);
        vcpu.mxcsr = 0x1F80 & !(1 << 7);
        set_scalar(&mut vcpu, 1, qnan);
        set_scalar(&mut vcpu, 2, 0);
        assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");
        assert_eq!(
            vcpu.regs.rflags & STATUS_FLAGS,
            flags::bits::ZF | flags::bits::PF | flags::bits::CF,
            "{code:02X?}"
        );
        assert_eq!(vcpu.mxcsr & 0x3F, 0, "SAE must not accrue status");
    }
}

fn assert_unmasked_exception(vector: u8, cr4: u64) {
    let (_, _, qnan, _, _) = patterns(2);
    let code = encoding(2, true, 1, 2, 0, false);
    let mut vcpu = vcpu(&code);
    vcpu.mxcsr = 0x1F80 & !(1 << 7);
    vcpu.sregs.cr4 = cr4;
    set_scalar(&mut vcpu, 1, qnan);
    set_scalar(&mut vcpu, 2, 0);
    let before = vcpu.regs.clone();
    let error = vcpu
        .step()
        .expect_err("unmasked EVEX COMI exception must not retire");
    assert!(
        format!("{error:?}").contains(&format!("IDT entry {vector} not present")),
        "wrong exception: {error:?}"
    );
    assert_ne!(vcpu.mxcsr & 1, 0, "invalid status must accrue");
    assert_eq!(vcpu.regs.rflags, before.rflags, "RFLAGS committed");
    assert_eq!(vcpu.regs.rip, before.rip, "RIP committed");
}

#[test]
fn evex_comi_unmasked_exception_is_precise_and_obeys_osxmmexcpt() {
    assert_unmasked_exception(19, 1 << 10);
    assert_unmasked_exception(6, 0);
}

fn assert_reserved_ud(code: &[u8]) {
    let mut vcpu = vcpu(code);
    vcpu.regs.rax = 0x2_0000;
    let before = vcpu.regs.clone();
    let mxcsr_before = vcpu.mxcsr;
    let error = vcpu.step().expect_err("reserved EVEX COMI must #UD");
    assert!(
        format!("{error:?}").contains("IDT entry 6 not present"),
        "wrong exception for {code:02X?}: {error:?}"
    );
    assert_eq!(vcpu.regs.rip, before.rip, "{code:02X?}: fault RIP");
    assert_eq!(vcpu.regs.rflags, before.rflags, "{code:02X?}: RFLAGS");
    assert_eq!(vcpu.regs.xmm, before.xmm, "{code:02X?}: XMM");
    assert_eq!(vcpu.regs.zmm_ext, before.zmm_ext, "{code:02X?}: ZMM16-31");
    assert_eq!(vcpu.mxcsr, mxcsr_before, "{code:02X?}: MXCSR");
}

#[test]
fn evex_comi_rejects_reserved_fields_before_address_or_state_access() {
    for elem_size in [2, 4, 8] {
        let valid = encoding(elem_size, true, 17, 25, 2, false);
        let mut invalid = Vec::new();
        let mut vvvv = valid;
        vvvv[2] &= !0x08;
        invalid.push(vvvv);
        let mut v_prime = valid;
        v_prime[3] &= !0x08;
        invalid.push(v_prime);
        let mut writemask = valid;
        writemask[3] |= 1;
        invalid.push(writemask);
        let mut zeroing = valid;
        zeroing[3] |= 0x80;
        invalid.push(zeroing);
        invalid.push(encoding(elem_size, true, 17, 25, 3, false));
        for code in invalid {
            assert_reserved_ud(&code);
        }

        let mut memory_sae = encoding(elem_size, true, 1, 0, 0, true);
        memory_sae[5] &= 0x38;
        assert_reserved_ud(&memory_sae);

        let mut memory_reserved_ll = encoding(elem_size, true, 1, 0, 3, false);
        memory_reserved_ll[5] &= 0x38;
        assert_reserved_ud(&memory_reserved_ll);
    }
}

#[test]
fn evex_comi_memory_form_reads_scalar_and_commits_only_after_access() {
    for elem_size in [2, 4, 8] {
        let (one, two, _, _, _) = patterns(elem_size);
        let mut code = encoding(elem_size, false, 17, 0, 2, false);
        code[5] &= 0x38; // source2 = [RAX]
        let mut vcpu = vcpu(&code);
        vcpu.regs.rax = DATA;
        set_scalar(&mut vcpu, 17, one);
        vcpu.write_mem(DATA, two, elem_size as u8).unwrap();
        assert!(vcpu.step().unwrap().is_none(), "{code:02X?}");
        assert_eq!(vcpu.regs.rflags & STATUS_FLAGS, flags::bits::CF);
        assert_eq!(vcpu.regs.rip, CODE + 6);
    }
}
