//! Architectural coverage for AMD TBM BLCI.
//!
//! BLCI is distinct from BLCIC:
//! `BLCI(src) = src | !(src + 1)`, whereas
//! `BLCIC(src) = !src & (src + 1)`.

use crate::common::*;
use rax::isa::x86_64::flags;
use rax::vm::vcpu::Registers;

const BLCI_EAX_EBX: [u8; 5] = [0x8F, 0xE9, 0x78, 0x02, 0xF3];
const BLCI_RAX_RBX: [u8; 5] = [0x8F, 0xE9, 0xF8, 0x02, 0xF3];

fn run_blci32(src: u32, initial_rflags: u64) -> Registers {
    let mut code = BLCI_EAX_EBX.to_vec();
    code.push(0xF4);
    let mut initial = Registers::default();
    initial.rbx = u64::from(src);
    initial.rax = u64::MAX;
    initial.rflags = initial_rflags;
    let (mut vcpu, _) = setup_tbm_vm(&code, Some(initial));
    run_until_hlt(&mut vcpu).expect("execute BLCI r32")
}

#[test]
fn blci32_matches_amd_pseudocode_and_zero_extends() {
    for src in [0, 1, 0x7E, 0xFD, 0x7FFF_FFFF, 0xFFFF_FFFE, 0xFFFF_FFFF] {
        let regs = run_blci32(src, 0x2);
        let expected = src | !src.wrapping_add(1);
        assert_eq!(regs.rax, u64::from(expected), "src={src:#010x}");
        assert_eq!(regs.rbx, u64::from(src), "source must not commit");
    }
}

#[test]
fn blci64_matches_amd_pseudocode() {
    for src in [
        0,
        0x0123_4567_89AB_CDEF,
        0x7FFF_FFFF_FFFF_FFFF,
        u64::MAX - 1,
        u64::MAX,
    ] {
        let mut code = BLCI_RAX_RBX.to_vec();
        code.push(0xF4);
        let mut initial = Registers::default();
        initial.rbx = src;
        let (mut vcpu, _) = setup_tbm_vm(&code, Some(initial));
        let regs = run_until_hlt(&mut vcpu).expect("execute BLCI r64");
        assert_eq!(regs.rax, src | !src.wrapping_add(1), "src={src:#018x}");
    }
}

#[test]
fn blci_supports_extended_registers_and_memory_sources() {
    // BLCI R8D,R9D: XOP.B extends r/m, XOP.vvvv selects the destination.
    let code = [0x8F, 0xC9, 0x38, 0x02, 0xF1, 0xF4];
    let mut initial = Registers::default();
    initial.r9 = 0xFFFF_FFF7;
    let (mut vcpu, _) = setup_tbm_vm(&code, Some(initial));
    let regs = run_until_hlt(&mut vcpu).expect("execute extended-register BLCI");
    assert_eq!(regs.r8, 0xFFFF_FFF7_u64 | u64::from(!0xFFFF_FFF8_u32));
    assert_eq!(regs.r9, 0xFFFF_FFF7);

    // BLCI EAX,[0x2000].
    let code = [
        0x8F, 0xE9, 0x78, 0x02, 0x34, 0x25, 0x00, 0x20, 0x00, 0x00, 0xF4,
    ];
    let (mut vcpu, memory) = setup_tbm_vm(&code, None);
    let src = 0xFFFF_FEFF_u32;
    write_mem_u32(&memory, src);
    let regs = run_until_hlt(&mut vcpu).expect("execute memory-source BLCI");
    assert_eq!(regs.rax, u64::from(src | !src.wrapping_add(1)));
}

#[test]
fn blci_uses_add_carry_and_logical_result_flags() {
    let preserved = flags::bits::PF | flags::bits::AF | flags::bits::DF;
    let regs = run_blci32(
        u32::MAX,
        0x2 | preserved | flags::bits::OF | flags::bits::ZF,
    );
    assert_ne!(regs.rflags & flags::bits::CF, 0, "CF is ADD carry");
    assert_ne!(
        regs.rflags & flags::bits::SF,
        0,
        "SF is logical-result sign"
    );
    assert_eq!(regs.rflags & flags::bits::ZF, 0);
    assert_eq!(regs.rflags & flags::bits::OF, 0);
    assert_eq!(
        regs.rflags & preserved,
        preserved,
        "PF/AF follow the deterministic undefined-flag policy; DF is unaffected"
    );
}

#[test]
fn disabled_tbm_raises_ud_without_committing() {
    let mut code = BLCI_EAX_EBX.to_vec();
    code.push(0xF4);
    let mut initial = Registers::default();
    initial.rax = 0x0123_4567_89AB_CDEF;
    initial.rbx = 0xFFFF_FFFD;
    initial.rflags = 0x2 | flags::bits::CF | flags::bits::OF;
    let (mut vcpu, _) = setup_vm_no_idt(&code, Some(initial));

    let before = vcpu.get_regs().expect("read initial registers");
    let error = vcpu.step().expect_err("disabled TBM must raise #UD");
    assert!(
        error.to_string().contains("IDT entry 6 not present"),
        "expected #UD delivery failure, got {error}"
    );
    let after = vcpu.get_regs().expect("read fault registers");
    assert_eq!(after.rip, before.rip);
    assert_eq!(after.rax, before.rax);
    assert_eq!(after.rbx, before.rbx);
    assert_eq!(after.rflags, before.rflags);
}

#[test]
fn disabled_tbm_ud_precedes_memory_addressing_and_faults() {
    // BLCI EAX,dword ptr [0xFFFF_F000]. The source is deliberately outside
    // the test mapping; CPUID.TBM=0 must still deliver #UD before ModR/M/SIB
    // address evaluation or a data-memory fault can become observable.
    let code = [0x8F, 0xE9, 0x78, 0x02, 0x34, 0x25, 0x00, 0xF0, 0xFF, 0xFF];
    let initial = Registers {
        rax: 0x0123_4567_89AB_CDEF,
        rflags: 0x2 | flags::bits::CF | flags::bits::OF,
        ..Registers::default()
    };
    let (mut vcpu, _) = setup_vm_no_idt(&code, Some(initial));

    let before = vcpu.get_regs().expect("read initial registers");
    let error = vcpu
        .step()
        .expect_err("disabled memory-source TBM must raise #UD");
    assert!(
        error.to_string().contains("IDT entry 6 not present"),
        "expected #UD before any memory fault, got {error}"
    );
    let after = vcpu.get_regs().expect("read fault registers");
    assert_eq!(after.rip, before.rip);
    assert_eq!(after.rax, before.rax);
    assert_eq!(after.rflags, before.rflags);
}

#[test]
fn tbm_is_rejected_in_real_and_virtual_8086_modes_without_committing() {
    for (name, protected_mode, virtual_8086) in
        [("real", false, false), ("virtual-8086", true, true)]
    {
        let mut code = BLCI_EAX_EBX.to_vec();
        code.push(0xF4);
        let initial = Registers {
            rax: 0x0123_4567_89AB_CDEF,
            rbx: 0xFFFF_FFFD,
            rflags: 0x2
                | flags::bits::CF
                | flags::bits::OF
                | if virtual_8086 { flags::bits::VM } else { 0 },
            ..Registers::default()
        };
        let (mut vcpu, _) = setup_vm_no_idt(&code, Some(initial));
        vcpu.set_tbm_enabled(true);
        let mut sregs = vcpu.get_sregs().expect("read system registers");
        sregs.cs.l = false;
        sregs.cs.db = !virtual_8086;
        sregs.efer &= !(1 << 10);
        if protected_mode {
            sregs.cr0 |= 1;
        } else {
            sregs.cr0 &= !1;
            // Real mode has no present bit in an IVT entry. An undersized
            // IDTR makes failed vector-6 delivery observable without commit.
            sregs.idt.limit = 0;
        }
        vcpu.set_sregs(&sregs).expect("install execution mode");

        let before = vcpu.get_regs().expect("read initial registers");
        let error = vcpu
            .step()
            .expect_err("TBM outside protected mode must raise #UD");
        assert!(
            error.to_string().contains("vector 6") || error.to_string().contains("IDT entry 6"),
            "{name}: expected failed #UD delivery, got {error}"
        );
        let after = vcpu.get_regs().expect("read fault registers");
        assert_eq!(after.rip, before.rip, "{name}");
        assert_eq!(after.rax, before.rax, "{name}");
        assert_eq!(after.rbx, before.rbx, "{name}");
        assert_eq!(after.rflags, before.rflags, "{name}");
    }
}

#[test]
fn xop_w_is_ignored_in_32_bit_compatibility_mode() {
    let mut code = BLCI_RAX_RBX.to_vec();
    code.push(0xF4);
    let source = 0x0123_4567_89AB_CDEF_u64;
    let initial = Registers {
        rax: u64::MAX,
        rbx: source,
        ..Registers::default()
    };
    let (mut vcpu, _) = setup_tbm_vm(&code, Some(initial));
    let mut sregs = vcpu.get_sregs().expect("read system registers");
    sregs.cs.l = false;
    sregs.cs.db = true;
    sregs.efer |= 1 << 10;
    vcpu.set_sregs(&sregs).expect("enter compatibility mode");

    let regs = run_until_hlt(&mut vcpu).expect("execute compatibility-mode BLCI");
    let source32 = source as u32;
    assert_eq!(
        regs.rax,
        u64::from(source32 | !source32.wrapping_add(1)),
        "XOP.W=1 must not select a 64-bit result outside 64-bit mode"
    );
    assert_eq!(regs.rbx, source, "source register must remain unmodified");
}

#[test]
fn compatibility_mode_ignores_xop_b_and_rejects_extended_r_x_vvvv() {
    // XOP.B=0 would extend r/m to R11 in 64-bit mode. In compatibility mode
    // the field is ignored, so this remains BLCI EAX,EBX and XOP.W remains WIG.
    let code = [0x8F, 0xC9, 0xF8, 0x02, 0xF3, 0xF4];
    let source = 0x0123_4567_89AB_CDEF_u64;
    let initial = Registers {
        rax: u64::MAX,
        rbx: source,
        r11: 0,
        ..Registers::default()
    };
    let (mut vcpu, _) = setup_tbm_vm(&code, Some(initial));
    let mut sregs = vcpu.get_sregs().expect("read system registers");
    sregs.cs.l = false;
    sregs.cs.db = true;
    sregs.efer |= 1 << 10;
    vcpu.set_sregs(&sregs).expect("enter compatibility mode");

    let regs = run_until_hlt(&mut vcpu).expect("execute compatibility-mode XOP.B form");
    let source32 = source as u32;
    assert_eq!(regs.rax, u64::from(source32 | !source32.wrapping_add(1)));
    assert_eq!(regs.rbx, source);
    assert_eq!(regs.r11, 0);

    for (name, instruction) in [
        // XOP.R and XOP.X must each encode 1 outside 64-bit mode.
        ("XOP.R=0", [0x8F, 0x69, 0x78, 0x02, 0xF3]),
        ("XOP.X=0", [0x8F, 0xA9, 0x78, 0x02, 0xF3]),
        // Encoded vvvv=0111b decodes destination register 8.
        ("decoded vvvv=8", [0x8F, 0xE9, 0x38, 0x02, 0xF3]),
    ] {
        let initial = Registers {
            rax: 0x0123_4567_89AB_CDEF,
            rbx: 0xFFFF_FFFD,
            r8: 0xCAFE_BABE_DEAD_BEEF,
            rflags: 0x2 | flags::bits::CF | flags::bits::OF,
            ..Registers::default()
        };
        let (mut vcpu, _) = setup_vm_no_idt(&instruction, Some(initial));
        vcpu.set_tbm_enabled(true);
        let mut sregs = vcpu.get_sregs().expect("read system registers");
        sregs.cs.l = false;
        sregs.cs.db = true;
        sregs.efer |= 1 << 10;
        vcpu.set_sregs(&sregs).expect("enter compatibility mode");

        let before = vcpu.get_regs().expect("read initial registers");
        let error = vcpu
            .step()
            .expect_err("reserved compatibility-mode XOP field must raise #UD");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "{name}: expected failed #UD delivery, got {error}"
        );
        let after = vcpu.get_regs().expect("read fault registers");
        assert_eq!(after.rip, before.rip, "{name}");
        assert_eq!(after.rax, before.rax, "{name}");
        assert_eq!(after.rbx, before.rbx, "{name}");
        assert_eq!(after.r8, before.r8, "{name}");
        assert_eq!(after.rflags, before.rflags, "{name}");
    }
}

#[test]
fn rex_before_an_allowed_xop_legacy_prefix_is_still_reserved() {
    for code in [
        &[0x48, 0x67, 0x8F, 0xE9, 0x78, 0x02, 0xF3][..],
        &[0x48, 0x64, 0x8F, 0xE9, 0x78, 0x02, 0xF3],
    ] {
        let initial = Registers {
            rax: 0x0123_4567_89AB_CDEF,
            rbx: 0xFFFF_FFFD,
            ..Registers::default()
        };
        let (mut vcpu, _) = setup_vm_no_idt(code, Some(initial));
        vcpu.set_tbm_enabled(true);
        let before = vcpu.get_regs().expect("read initial registers");
        let error = vcpu
            .step()
            .expect_err("REX before XOP must raise #UD despite later allowed prefix");
        assert!(
            error.to_string().contains("IDT entry 6 not present"),
            "expected failed #UD delivery, got {error}"
        );
        let after = vcpu.get_regs().expect("read fault registers");
        assert_eq!(after.rip, before.rip);
        assert_eq!(after.rax, before.rax);
        assert_eq!(after.rbx, before.rbx);
    }
}
