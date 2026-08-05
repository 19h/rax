//! Direct-execution regressions for scalar EVEX special-function controls.

use std::sync::Arc;

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::vm::vcpu::VCpu;

const CODE: u64 = 0x1000;
const DATA: u64 = 0x3000;
const UNMAPPED: u64 = 0x2_0000;

#[derive(Clone, Copy)]
struct ScalarForm {
    name: &'static str,
    map: u8,
    opcode: u8,
    pp: u8,
    w: bool,
    has_imm: bool,
    register_sae: bool,
}

const FORMS: [ScalarForm; 24] = [
    ScalarForm::new("VGETEXPSH", 6, 0x43, 1, false, false, true),
    ScalarForm::new("VGETEXPSS", 2, 0x43, 1, false, false, true),
    ScalarForm::new("VGETEXPSD", 2, 0x43, 1, true, false, true),
    ScalarForm::new("VRCP14SS", 2, 0x4D, 1, false, false, false),
    ScalarForm::new("VRCP14SD", 2, 0x4D, 1, true, false, false),
    ScalarForm::new("VRSQRT14SS", 2, 0x4F, 1, false, false, false),
    ScalarForm::new("VRSQRT14SD", 2, 0x4F, 1, true, false, false),
    ScalarForm::new("VRCPSH", 6, 0x4D, 1, false, false, false),
    ScalarForm::new("VRSQRTSH", 6, 0x4F, 1, false, false, false),
    ScalarForm::new("VRCP28SS", 2, 0xCB, 1, false, false, true),
    ScalarForm::new("VRCP28SD", 2, 0xCB, 1, true, false, true),
    ScalarForm::new("VRSQRT28SS", 2, 0xCD, 1, false, false, true),
    ScalarForm::new("VRSQRT28SD", 2, 0xCD, 1, true, false, true),
    ScalarForm::new("VRNDSCALESH", 3, 0x0A, 0, false, true, true),
    ScalarForm::new("VRNDSCALESS", 3, 0x0A, 1, false, true, true),
    ScalarForm::new("VRNDSCALESD", 3, 0x0B, 1, true, true, true),
    ScalarForm::new("VGETMANTSH", 3, 0x27, 0, false, true, true),
    ScalarForm::new("VGETMANTSS", 3, 0x27, 1, false, true, true),
    ScalarForm::new("VGETMANTSD", 3, 0x27, 1, true, true, true),
    ScalarForm::new("VREDUCESH", 3, 0x57, 0, false, true, true),
    ScalarForm::new("VREDUCESS", 3, 0x57, 1, false, true, true),
    ScalarForm::new("VREDUCESD", 3, 0x57, 1, true, true, true),
    ScalarForm::new("VFIXUPIMMSS", 3, 0x55, 1, false, true, true),
    ScalarForm::new("VFIXUPIMMSD", 3, 0x55, 1, true, true, true),
];

impl ScalarForm {
    const fn new(
        name: &'static str,
        map: u8,
        opcode: u8,
        pp: u8,
        w: bool,
        has_imm: bool,
        register_sae: bool,
    ) -> Self {
        Self {
            name,
            map,
            opcode,
            pp,
            w,
            has_imm,
            register_sae,
        }
    }
}

fn encoding(form: ScalarForm, evex_b: bool, memory: bool, ll: u8) -> Vec<u8> {
    const MERGE_SOURCE: u8 = 2;
    const DESTINATION: u8 = 1;
    const REGISTER_SOURCE: u8 = 3;

    assert!(ll < 4);
    let mut code = vec![
        0x62,
        0xF0 | form.map,
        (u8::from(form.w) << 7) | (((!MERGE_SOURCE) & 0x0F) << 3) | 0x04 | form.pp,
        0x08 | (u8::from(evex_b) << 4) | (ll << 5),
        form.opcode,
        if memory {
            DESTINATION << 3
        } else {
            0xC0 | (DESTINATION << 3) | REGISTER_SOURCE
        },
    ];
    if form.has_imm {
        code.push(0x77);
    }
    code
}

fn vcpu(code: &[u8]) -> X86_64Vcpu {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(CODE)).unwrap();
    memory
        .write_slice(&0x3ff0_0000_0000_0000u64.to_le_bytes(), GuestAddress(DATA))
        .unwrap();
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.regs.rip = CODE;
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.db = false;
    vcpu.set_xeon_phi_avx512_enabled(true);
    vcpu
}

fn assert_reserved_ud(form: ScalarForm, code: &[u8]) {
    let mut vcpu = vcpu(code);
    vcpu.regs.rax = UNMAPPED;
    vcpu.regs.k[1] = 0xA55A_3CC3_F00F_9696;
    vcpu.regs.xmm[1] = [0x0123_4567_89AB_CDEF, 0xFEDC_BA98_7654_3210];
    let before = vcpu.regs.clone();
    let mxcsr_before = vcpu.mxcsr;
    let error = match vcpu.step() {
        Err(error) => error,
        Ok(exit) => panic!(
            "{} reserved encoding executed with {exit:?}: {code:02X?}",
            form.name
        ),
    };
    assert!(
        format!("{error:?}").contains("IDT entry 6 not present"),
        "{} wrong exception for {code:02X?}: {error:?}",
        form.name
    );
    assert_eq!(vcpu.regs.rip, before.rip, "{} RIP", form.name);
    assert_eq!(vcpu.regs.rflags, before.rflags, "{} RFLAGS", form.name);
    assert_eq!(vcpu.regs.rax, before.rax, "{} RAX", form.name);
    assert_eq!(vcpu.regs.xmm, before.xmm, "{} XMM", form.name);
    assert_eq!(vcpu.regs.ymm_high, before.ymm_high, "{} YMM", form.name);
    assert_eq!(vcpu.regs.zmm_high, before.zmm_high, "{} ZMM", form.name);
    assert_eq!(vcpu.regs.zmm_ext, before.zmm_ext, "{} ZMM16-31", form.name);
    assert_eq!(vcpu.regs.k, before.k, "{} opmasks", form.name);
    assert_eq!(vcpu.mxcsr, mxcsr_before, "{} MXCSR", form.name);
}

#[test]
fn scalar_evex_special_functions_accept_memory_only_when_b_is_clear() {
    for form in FORMS {
        for ll in 0..4 {
            let valid = encoding(form, false, true, ll);
            let mut vcpu = vcpu(&valid);
            vcpu.regs.rax = DATA;
            assert!(
                vcpu.step().unwrap().is_none(),
                "{} valid memory LLIG={ll}: {valid:02X?}",
                form.name
            );
            assert_eq!(
                vcpu.regs.rip,
                CODE + valid.len() as u64,
                "{} LLIG={ll} RIP",
                form.name
            );

            let reserved = encoding(form, true, true, ll);
            assert_reserved_ud(form, &reserved);
        }
    }
}

#[test]
fn scalar_evex_special_functions_apply_exact_register_sae_domain() {
    for form in FORMS {
        for ll in 0..4 {
            let code = encoding(form, true, false, ll);
            if form.register_sae {
                let mut vcpu = vcpu(&code);
                assert!(
                    vcpu.step().unwrap().is_none(),
                    "{} legal register SAE RC={ll}: {code:02X?}",
                    form.name
                );
                assert_eq!(
                    vcpu.regs.rip,
                    CODE + code.len() as u64,
                    "{} RC={ll} RIP",
                    form.name
                );
            } else {
                assert_reserved_ud(form, &code);
            }
        }
    }
}
