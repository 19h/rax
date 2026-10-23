//! Intel SDM Vol. 1 sections 4.9 and 11.5: guest SIMD arithmetic contracts.

use std::sync::Arc;

use rax::isa::x86_64::X86_64Vcpu;
use rax::vm::vcpu::{Registers, SystemRegisters, VCpu};
use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

#[derive(Clone, Copy, Debug)]
struct Form {
    prefix: Option<u8>,
    double: bool,
    scalar: bool,
}

const FORMS: [Form; 4] = [
    Form {
        prefix: Some(0xf3),
        double: false,
        scalar: true,
    },
    Form {
        prefix: Some(0xf2),
        double: true,
        scalar: true,
    },
    Form {
        prefix: None,
        double: false,
        scalar: false,
    },
    Form {
        prefix: Some(0x66),
        double: true,
        scalar: false,
    },
];

fn vector(bits: u64, form: Form) -> [u64; 2] {
    match (form.double, form.scalar) {
        (false, true) => [0x1234_5678_0000_0000 | bits, 0xfedc_ba98_7654_3210],
        (true, true) => [bits, 0xfedc_ba98_7654_3210],
        (false, false) => [bits | bits << 32; 2],
        (true, false) => [bits; 2],
    }
}

fn machine(
    form: Form,
    opcode: u8,
    first: [u64; 2],
    second: [u64; 2],
    mxcsr: u32,
    memory_source: bool,
) -> X86_64Vcpu {
    let memory = Arc::new(GuestMemoryMmap::from_ranges(&[(GuestAddress(0), 0x4000)]).unwrap());
    let mut code = vec![0x0f, 0xae, 0x13]; // LDMXCSR [RBX]
    code.extend(form.prefix);
    code.extend_from_slice(&[0x0f, opcode, if memory_source { 0x02 } else { 0xc1 }, 0xf4]);
    memory.write_slice(&code, GuestAddress(0x1000)).unwrap();
    memory
        .write_obj(mxcsr.to_le(), GuestAddress(0x2000))
        .unwrap();
    memory
        .write_obj(second[0].to_le(), GuestAddress(0x3000))
        .unwrap();
    memory
        .write_obj(second[1].to_le(), GuestAddress(0x3008))
        .unwrap();
    let mut cpu = X86_64Vcpu::new(0, memory);
    let mut system = SystemRegisters::default();
    system.cr0 = 0x50033;
    system.cr4 = (1 << 9) | (1 << 10);
    system.efer = 1 << 10;
    system.cs.l = true;
    system.cs.selector = 8;
    cpu.set_sregs(&system).unwrap();
    let mut regs = Registers::default();
    regs.rip = 0x1000;
    regs.rbx = 0x2000;
    regs.rdx = 0x3000;
    regs.rflags = 0x202;
    regs.xmm[0] = first;
    regs.xmm[1] = second;
    cpu.set_regs(&regs).unwrap();
    assert!(cpu.step().unwrap().is_none());
    assert_eq!(cpu.get_regs().unwrap().rip, 0x1003);
    assert_eq!(cpu.get_emulator_state().unwrap().mxcsr, mxcsr);
    cpu
}

fn check(form: Form, opcode: u8, first: u64, second: u64, mxcsr: u32, expected: u64, flags: u32) {
    for memory_source in [false, true] {
        let mut cpu = machine(
            form,
            opcode,
            vector(first, form),
            vector(second, form),
            mxcsr,
            memory_source,
        );
        assert!(cpu.step().unwrap().is_none(), "{form:?} opcode={opcode:#x}");
        let regs = cpu.get_regs().unwrap();
        assert_eq!(
            regs.xmm[0],
            vector(expected, form),
            "{form:?} opcode={opcode:#x}"
        );
        assert_eq!(regs.xmm[1], vector(second, form));
        assert_eq!(regs.rflags, 0x202);
        assert_eq!(regs.rip, 0x1006 + u64::from(form.prefix.is_some()));
        assert_eq!(
            cpu.get_emulator_state().unwrap().mxcsr,
            mxcsr | flags,
            "{form:?} opcode={opcode:#x}"
        );
    }
}

#[test]
fn upward_rounding_and_precision_status_follow_guest_mxcsr() {
    check(
        FORMS[0],
        0x58,
        0x3f80_0000,
        0x3380_0000,
        0x5f80,
        0x3f80_0001,
        0x20,
    );
}

#[test]
fn masked_division_by_zero_sets_guest_sticky_status() {
    check(FORMS[0], 0x5e, 0x3f80_0000, 0, 0x1f80, 0x7f80_0000, 4);
}

#[test]
fn daz_converts_input_subnormal_to_zero_without_denormal_status() {
    check(FORMS[0], 0x58, 1, 0, 0x1fc0, 0, 0);
}

#[test]
fn all_rounding_directions_cover_both_signs_and_precisions() {
    for form in FORMS {
        let (one, half_ulp, sign) = if form.double {
            (0x3ff0_0000_0000_0000, 0x3ca0_0000_0000_0000, 1 << 63)
        } else {
            (0x3f80_0000, 0x3380_0000, 1 << 31)
        };
        for rounding in 0..4 {
            for negative in [false, true] {
                let signed = if negative { sign } else { 0 };
                let increment = (negative && rounding == 1) || (!negative && rounding == 2);
                check(
                    form,
                    0x58,
                    one | signed,
                    half_ulp | signed,
                    0x1f80 | rounding << 13,
                    (one + u64::from(increment)) | signed,
                    0x20,
                );
            }
        }
    }
}

#[test]
fn daz_and_sticky_denormal_flags_cover_all_legacy_vector_forms() {
    for form in FORMS {
        check(form, 0x58, 1, 0, 0x1f84, 1, 2);
        check(form, 0x58, 1, 0, 0x1fc4, 0, 0);
    }
}

#[test]
fn ftz_flushes_tiny_results_and_preserves_signed_zero() {
    for form in FORMS {
        let (normal, half, tiny, sign) = if form.double {
            (
                0x0010_0000_0000_0000,
                0x3fe0_0000_0000_0000,
                0x0008_0000_0000_0000,
                1 << 63,
            )
        } else {
            (0x0080_0000, 0x3f00_0000, 0x0040_0000, 1 << 31)
        };
        for signed in [0, sign] {
            check(form, 0x59, normal | signed, half, 0x1f80, tiny | signed, 0);
            check(form, 0x59, normal | signed, half, 0x9f80, signed, 0x30);
        }
    }
}

#[test]
fn overflow_uses_guest_rounding_and_reports_overflow_and_precision() {
    for form in FORMS {
        let (maximum, two, infinity) = if form.double {
            (
                0x7fef_ffff_ffff_ffff,
                0x4000_0000_0000_0000,
                0x7ff0_0000_0000_0000,
            )
        } else {
            (0x7f7f_ffff, 0x4000_0000, 0x7f80_0000)
        };
        for rounding in 0..4 {
            let expected = if rounding == 0 || rounding == 2 {
                infinity
            } else {
                maximum
            };
            check(
                form,
                0x59,
                maximum,
                two,
                0x1f80 | rounding << 13,
                expected,
                0x28,
            );
        }
    }
}

#[test]
fn special_values_and_source_selection_preserve_exact_bits_and_status() {
    for form in FORMS {
        let (one, two, four, infinity, indefinite, snan, qnan, sign) = if form.double {
            (
                0x3ff0_0000_0000_0000,
                0x4000_0000_0000_0000,
                0x4010_0000_0000_0000,
                0x7ff0_0000_0000_0000,
                0xfff8_0000_0000_0000,
                0x7ff0_0000_0000_0042,
                0x7ff8_0000_0000_0042,
                1 << 63,
            )
        } else {
            (
                0x3f80_0000,
                0x4000_0000,
                0x4080_0000,
                0x7f80_0000,
                0xffc0_0000,
                0x7f80_0042,
                0x7fc0_0042,
                1 << 31,
            )
        };
        for (opcode, a, b, result, status) in [
            (0x5e, one, 0, infinity, 4),
            (0x5e, 0, 0, indefinite, 1),
            (0x5e, infinity, infinity, indefinite, 1),
            (0x51, 0, one | sign, indefinite, 1),
            (0x51, 0, four, two, 0),
            (0x58, snan, one, qnan, 1),
            (0x58, qnan, 1, qnan, 0),
            (0x5d, 0, sign, sign, 0),
            (0x5f, 0, sign, sign, 0),
            (0x5d, qnan, one, one, 1),
            (0x5f, one, snan, snan, 1),
        ] {
            check(form, opcode, a, b, 0x1f80, result, status);
        }
    }
}

#[test]
fn source_fault_precedes_destination_and_mxcsr_updates() {
    for form in FORMS {
        for opcode in [0x58, 0x5c, 0x59, 0x5e, 0x51, 0x5d, 0x5f] {
            let first = vector(
                if form.double {
                    0x7ff0_0000_0000_0042
                } else {
                    0x7f80_0042
                },
                form,
            );
            for address in [0x5000, 0x3fff] {
                let mut cpu = machine(form, opcode, first, [0; 2], 0x1f00, true);
                let mut regs = cpu.get_regs().unwrap();
                regs.rdx = address;
                cpu.set_regs(&regs).unwrap();
                assert!(cpu.step().is_err());
                assert_eq!(cpu.get_regs().unwrap().xmm[0], first);
                assert_eq!(cpu.get_regs().unwrap().rip, 0x1003);
                assert_eq!(cpu.get_emulator_state().unwrap().mxcsr, 0x1f00);
            }
        }
    }
}

#[test]
fn unmasked_division_faults_at_the_instruction_without_destination_commit() {
    for form in FORMS {
        for osxmmexcpt in [false, true] {
            let one = if form.double {
                0x3ff0_0000_0000_0000
            } else {
                0x3f80_0000
            };
            let first = vector(one, form);
            let mut cpu = machine(form, 0x5e, first, [0; 2], 0x1d80, false);
            let mut system = cpu.get_sregs().unwrap();
            if !osxmmexcpt {
                system.cr4 &= !(1 << 10);
            }
            cpu.set_sregs(&system).unwrap();
            let error = cpu.step().unwrap_err().to_string();
            let vector = if osxmmexcpt { 19 } else { 6 };
            assert!(error.contains(&format!("IDT entry {vector}")), "{error}");
            assert_eq!(cpu.get_regs().unwrap().xmm[0], first);
            assert_eq!(cpu.get_regs().unwrap().rip, 0x1003);
            assert_eq!(cpu.get_emulator_state().unwrap().mxcsr, 0x1d84);
        }
    }
}

#[test]
fn packed_unmasked_precomputation_suppresses_postcomputation_status() {
    for form in FORMS.into_iter().filter(|form| !form.scalar) {
        let (first, second) = if form.double {
            (
                [0x3ff0_0000_0000_0000, 0x7fef_ffff_ffff_ffff],
                [0, 0x3fe0_0000_0000_0000],
            )
        } else {
            (
                [0x7f7f_ffff_3f80_0000, 0x3f80_0000_3f80_0000],
                [0x3f00_0000_0000_0000, 0x3f80_0000_3f80_0000],
            )
        };
        for mxcsr in [0x1f80, 0x1d80] {
            let mut cpu = machine(form, 0x5e, first, second, mxcsr, false);
            if mxcsr == 0x1f80 {
                assert!(cpu.step().unwrap().is_none());
                assert_eq!(cpu.get_emulator_state().unwrap().mxcsr, mxcsr | 0x2c);
            } else {
                assert!(cpu.step().is_err());
                assert_eq!(cpu.get_regs().unwrap().xmm[0], first);
                assert_eq!(cpu.get_regs().unwrap().rip, 0x1003);
                assert_eq!(cpu.get_emulator_state().unwrap().mxcsr, mxcsr | 4);
            }
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn guest_arithmetic_preserves_the_host_mxcsr_control_and_sticky_flags() {
    use std::arch::asm;

    fn read_host() -> u32 {
        let mut value = 0u32;
        // SAFETY: the output is an aligned writable u32; STMXCSR is unprivileged.
        unsafe {
            asm!("stmxcsr [{ptr}]", ptr = in(reg) &mut value, options(nostack, preserves_flags));
        }
        value
    }
    fn write_host(value: u32) {
        // SAFETY: this test supplies only valid masked MXCSR values and restores
        // the original thread-local environment, including during unwinding.
        unsafe {
            asm!("ldmxcsr [{ptr}]", ptr = in(reg) &value, options(nostack, preserves_flags));
        }
    }
    struct Restore(u32);
    impl Drop for Restore {
        fn drop(&mut self) {
            write_host(self.0);
        }
    }
    let _restore = Restore(read_host());
    let sentinel = 0xbfc1;
    write_host(sentinel);
    check(
        FORMS[0],
        0x58,
        0x3f80_0000,
        0x3380_0000,
        0x5f80,
        0x3f80_0001,
        0x20,
    );
    check(FORMS[0], 0x5e, 0x3f80_0000, 0, 0x1f80, 0x7f80_0000, 4);
    assert_eq!(read_host(), sentinel);
}
