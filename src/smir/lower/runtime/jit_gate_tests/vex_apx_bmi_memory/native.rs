use super::*;
use crate::smir::lower::runtime::{ExecMem, GuestRegs};

#[repr(C)]
struct LoadResult {
    value: u64,
    ok: u64,
}

#[derive(Default)]
struct MemoryContext {
    value: u64,
    ok: u64,
    calls: u64,
    last_addr: u64,
    last_size: u64,
    last_signed: u64,
}

extern "C" fn load_helper(
    context: *mut MemoryContext,
    addr: u64,
    size: u64,
    signed: u64,
) -> LoadResult {
    let context = unsafe { &mut *context };
    context.calls += 1;
    context.last_addr = addr;
    context.last_size = size;
    context.last_signed = signed;
    LoadResult {
        value: context.value,
        ok: context.ok,
    }
}

fn host_supports(kind: BmiKind) -> bool {
    match kind.scalar_feature_requirements() {
        (true, false) => std::is_x86_feature_detected!("bmi2"),
        (false, true) => std::is_x86_feature_detected!("bmi1"),
        (false, false) => true,
        (true, true) => unreachable!(),
    }
}

fn full_guest_regs(ordinal: usize) -> GuestRegs {
    let mut registers = GuestRegs {
        gpr: core::array::from_fn(|index| {
            0xA500_0000_0000_0000u64.wrapping_add(
                (index as u64)
                    .wrapping_mul(0x0101_0101_0101_0101)
                    .wrapping_add(ordinal as u64),
            )
        }),
        rflags: 0x2 | (((ordinal as u64).wrapping_mul(0x145)) & 0x8D5),
        exit_pc: 0xAAAA_BBBB_CCCC_DDDD,
        fs_base: 0x1111_0000_0000_0000,
        gs_base: 0x2222_0000_0000_0000,
        vector_active: 0,
        mxcsr: 0x1F80 | ((ordinal as u32) & 0x3F),
        tsc_aux: 0xA5A5_0000 | ordinal as u32,
        pkru: 0x55AA_0000 | ordinal as u32,
        xcr0: 0x0006_02E7,
        xgetbv1: 0x0000_0007,
        cr4: 1 << 18,
        cr0: 0x8000_0011,
        cpl: 3,
        apx_enabled: 1,
        mm: core::array::from_fn(|index| 0xF0E0_D0C0_B0A0_9080u64.wrapping_add(index as u64)),
        x87_tag_word: 0xFFFF,
        ac_flag: (ordinal & 1) as u64,
        cr2: 0x1234_5678_9ABC_DEF0,
        cr3: 0x0000_0001_2345_6000,
        cr8: 0x0A,
        dr0: 0x10,
        dr1: 0x11,
        dr2: 0x12,
        dr3: 0x13,
        dr6: 0xFFFF_0FF0,
        dr7: 0x0000_0400,
        efer: 0x0D01,
        cs_l: 1,
        tr_type: 0x0B,
        interrupt_flags: 0x0003_0200,
        interrupt_inhibit: 1,
        misc_enable: 0x0080_0001,
        pat: 0x0007_0406_0007_0406,
        umwait_control: 0x1234,
        ..GuestRegs::default()
    };
    for (index, vector) in registers.zmm.iter_mut().enumerate() {
        *vector = core::array::from_fn(|lane| {
            0x1122_3344_5566_7788u64.wrapping_add((index * 8 + lane) as u64)
        });
    }
    registers.k = core::array::from_fn(|index| 0x0102_0304_0506_0708u64.rotate_left(index as u32));
    registers
}

fn width_mask(width: OpWidth) -> u64 {
    match width {
        OpWidth::W32 => u64::from(u32::MAX),
        OpWidth::W64 => u64::MAX,
        _ => unreachable!(),
    }
}

fn low_mask(bits: u32) -> u64 {
    match bits {
        0 => 0,
        64.. => u64::MAX,
        _ => (1_u64 << bits) - 1,
    }
}

fn pdep(source: u64, mask: u64, bits: u32) -> u64 {
    let mut result = 0_u64;
    let mut source_bit = 0_u32;
    for bit in 0..bits {
        if mask & (1_u64 << bit) != 0 {
            if source & (1_u64 << source_bit) != 0 {
                result |= 1_u64 << bit;
            }
            source_bit += 1;
        }
    }
    result
}

fn pext(source: u64, mask: u64, bits: u32) -> u64 {
    let mut result = 0_u64;
    let mut destination_bit = 0_u32;
    for bit in 0..bits {
        if mask & (1_u64 << bit) != 0 {
            if source & (1_u64 << bit) != 0 {
                result |= 1_u64 << destination_bit;
            }
            destination_bit += 1;
        }
    }
    result
}

fn expected_result(case: MemoryBmiCase, memory: u64, other: u64) -> u64 {
    let mask = width_mask(case.width);
    let bits = case.width.bits();
    let memory = memory & mask;
    let other = other & mask;
    let result = match case.kind {
        BmiKind::Andn => memory & !other,
        BmiKind::Blsr => memory & memory.wrapping_sub(1),
        BmiKind::Blsmsk => memory ^ memory.wrapping_sub(1),
        BmiKind::Blsi => memory.wrapping_neg() & memory,
        BmiKind::Bzhi => {
            let index = (other & 0xFF) as u32;
            if index >= bits {
                memory
            } else {
                memory & low_mask(index)
            }
        }
        BmiKind::Bextr => {
            let start = (other & 0xFF) as u32;
            let length = ((other >> 8) & 0xFF) as u32;
            if start >= bits {
                0
            } else {
                (memory >> start) & low_mask(length.min(bits - start))
            }
        }
        BmiKind::Pdep => pdep(other, memory, bits),
        BmiKind::Pext => pext(other, memory, bits),
        BmiKind::Rorx => match case.width {
            OpWidth::W32 => u64::from((memory as u32).rotate_right(0xAD & 31)),
            OpWidth::W64 => memory.rotate_right(0xAD & 63),
            _ => unreachable!(),
        },
    };
    result & mask
}

fn expected_rflags(
    case: MemoryBmiCase,
    incoming: u64,
    memory: u64,
    other: u64,
    result: u64,
) -> u64 {
    if case.suppressed || matches!(case.kind, BmiKind::Pdep | BmiKind::Pext | BmiKind::Rorx) {
        return incoming;
    }

    let mask = match case.kind {
        BmiKind::Bextr => 0x841,
        BmiKind::Andn | BmiKind::Blsr | BmiKind::Blsmsk | BmiKind::Blsi | BmiKind::Bzhi => 0x8C1,
        BmiKind::Pdep | BmiKind::Pext | BmiKind::Rorx => unreachable!(),
    };
    let width_mask = width_mask(case.width);
    let source = memory & width_mask;
    let mut outputs = 0_u64;
    let carry = match case.kind {
        BmiKind::Blsr | BmiKind::Blsmsk => source == 0,
        BmiKind::Blsi => source != 0,
        BmiKind::Bzhi => (other & 0xFF) >= u64::from(case.width.bits()),
        BmiKind::Andn | BmiKind::Bextr => false,
        BmiKind::Pdep | BmiKind::Pext | BmiKind::Rorx => unreachable!(),
    };
    if carry {
        outputs |= 1;
    }
    if result == 0 {
        outputs |= 1 << 6;
    }
    if result & (1_u64 << (case.width.bits() - 1)) != 0 {
        outputs |= 1 << 7;
    }
    // OF is defined as zero for each admitted flag-writing BMI consumer.
    (incoming & !mask) | (outputs & mask)
}

#[test]
fn native_bmi_memory_sources_match_primary_semantics_and_are_fault_precise() {
    const TUPLES: [(u8, u8); 12] = [
        (0, 1),
        (8, 10),
        (15, 13),
        (1, 1),
        (3, 1),
        (1, 3),
        (3, 3),
        (4, 5),
        (5, 4),
        (16, 17),
        (20, 20),
        (31, 16),
    ];
    const INPUTS: [(u64, u64); 10] = [
        (0, 0),
        (1, 1),
        (u64::MAX, 0x0100),
        (0x8000_0000, 0x0808),
        (0xFFFF_FFFF, 0x2020),
        (0x8000_0000_0000_0001, 0x4040),
        (0xFEDC_BA98_7654_3210, 0x3F01),
        (0x0101_0101_0101_0101, 0xFFFF_FFFF_FFFF_FFFF),
        (0xAAAA_AAAA_5555_5555, 0x0F0F_F0F0_3333_CCCC),
        (0x0000_0001_0000_0000, 0x0000_0041_0000_0021),
    ];

    let mut successes = 0_usize;
    let mut faults = 0_usize;
    for kind in BmiKind::ALL {
        if !host_supports(kind) {
            eprintln!("skipping native {kind:?}: required host BMI feature is absent");
            continue;
        }
        let suppressed_values: &[bool] = if matches!(
            kind,
            BmiKind::Andn
                | BmiKind::Blsr
                | BmiKind::Blsmsk
                | BmiKind::Blsi
                | BmiKind::Bzhi
                | BmiKind::Bextr
        ) {
            &[false, true]
        } else {
            &[true]
        };
        for width in [OpWidth::W32, OpWidth::W64] {
            for (destination, other) in TUPLES {
                for &suppressed in suppressed_values {
                    let case = MemoryBmiCase {
                        kind,
                        width,
                        destination,
                        other,
                        suppressed,
                    };
                    for level in LEVELS {
                        let function =
                            optimize(manual_function(case, Address::Direct(x86(3))), level);
                        let (code, entry) = lower(&function);
                        let exec = ExecMem::new(&code)
                            .unwrap_or_else(|error| panic!("{level:?} {case:?}: {error:?}"));

                        for (ordinal, (memory, other_value)) in INPUTS.into_iter().enumerate() {
                            let mut context = MemoryContext {
                                value: memory,
                                ok: 1,
                                ..MemoryContext::default()
                            };
                            let mut registers = full_guest_regs(ordinal);
                            registers.gpr[3] =
                                0x4000_0000_0000_1000 + (ordinal as u64).wrapping_mul(0x20);
                            if case.kind.uses_arch_source() {
                                registers.gpr[usize::from(other)] = other_value;
                            }
                            let expected_addr = registers.gpr[3];
                            let effective_other = registers.gpr[usize::from(other)];
                            registers.ctx = (&mut context as *mut MemoryContext) as u64;
                            registers.load_fn = load_helper as usize as u64;
                            let mut expected = registers;
                            let result = expected_result(case, memory, effective_other);
                            expected.gpr[usize::from(destination)] = result;
                            expected.rflags = expected_rflags(
                                case,
                                expected.rflags,
                                memory,
                                effective_other,
                                result,
                            );

                            exec.run(entry, &mut registers);
                            expected.host_mxcsr = registers.host_mxcsr;
                            assert_eq!(
                                registers, expected,
                                "{level:?} {case:?} memory={memory:#018X} other={effective_other:#018X}"
                            );
                            assert_eq!(context.calls, 1, "{level:?} {case:?}");
                            assert_eq!(context.last_addr, expected_addr, "{level:?} {case:?}");
                            assert_eq!(
                                context.last_size,
                                u64::from(width.bits() / 8),
                                "{level:?} {case:?}"
                            );
                            assert_eq!(context.last_signed, 0, "{level:?} {case:?}");
                            successes += 1;
                        }

                        let mut context = MemoryContext {
                            value: u64::MAX,
                            ok: 0,
                            ..MemoryContext::default()
                        };
                        let mut registers = full_guest_regs(0x55);
                        registers.gpr[3] = 0x1234_5000;
                        if case.kind.uses_arch_source() {
                            registers.gpr[usize::from(other)] = 0x4040;
                        }
                        let expected_addr = registers.gpr[3];
                        registers.ctx = (&mut context as *mut MemoryContext) as u64;
                        registers.load_fn = load_helper as usize as u64;
                        let mut expected = registers;
                        expected.exit_pc = PC;

                        exec.run(entry, &mut registers);
                        expected.host_mxcsr = registers.host_mxcsr;
                        assert_eq!(registers, expected, "fault {level:?} {case:?}");
                        assert_eq!(context.calls, 1, "fault {level:?} {case:?}");
                        assert_eq!(context.last_addr, expected_addr, "fault {level:?} {case:?}");
                        assert_eq!(
                            context.last_size,
                            u64::from(width.bits() / 8),
                            "fault {level:?} {case:?}"
                        );
                        assert_eq!(context.last_signed, 0, "fault {level:?} {case:?}");
                        faults += 1;
                    }
                }
            }
        }
    }

    eprintln!("executed {successes} successful and {faults} faulting native BMI memory cases");
    assert!(successes > 0, "host executed no native BMI memory case");
    assert!(faults > 0, "host executed no faulting BMI memory case");
}

#[test]
fn native_apx_egpr_segment_sib_addresses_are_computed_before_commit() {
    let forms: &[(&str, &[u8], BmiKind, u64)] = &[
        (
            "PDEP FS:[R17+R18*4]",
            &[0x64, 0x62, 0xEA, 0xE3, 0x00, 0xF5, 0x24, 0x91],
            BmiKind::Pdep,
            0,
        ),
        (
            "RORX GS:[R17+R18*4+32]",
            &[0x65, 0x62, 0xEB, 0xFB, 0x08, 0xF0, 0x64, 0x91, 0x20, 0x0D],
            BmiKind::Rorx,
            0x20,
        ),
    ];

    for (name, bytes, kind, displacement) in forms {
        if !host_supports(*kind) {
            eprintln!("skipping {name}: required host BMI feature is absent");
            continue;
        }
        for level in [OptLevel::O0, OptLevel::O2] {
            let function = optimize(lift_raw(bytes), level);
            let (code, entry) = lower(&function);
            let exec =
                ExecMem::new(&code).unwrap_or_else(|error| panic!("{name} {level:?}: {error:?}"));
            let mut context = MemoryContext {
                value: 0xF0F0_00FF_AA55_1234,
                ok: 1,
                ..MemoryContext::default()
            };
            let mut registers = full_guest_regs(0x66);
            registers.gpr[17] = 0x1000;
            registers.gpr[18] = 0x20;
            registers.gpr[19] = 0x0123_4567_89AB_CDEF;
            registers.fs_base = 0x1111_0000_0000_0000;
            registers.gs_base = 0x2222_0000_0000_0000;
            registers.ctx = (&mut context as *mut MemoryContext) as u64;
            registers.load_fn = load_helper as usize as u64;
            let mut expected = registers;
            expected.gpr[20] = match kind {
                BmiKind::Pdep => pdep(registers.gpr[19], context.value, 64),
                BmiKind::Rorx => context.value.rotate_right(13),
                _ => unreachable!(),
            };
            let segment = match kind {
                BmiKind::Pdep => registers.fs_base,
                BmiKind::Rorx => registers.gs_base,
                _ => unreachable!(),
            };
            let expected_addr = segment
                .wrapping_add(registers.gpr[17])
                .wrapping_add(registers.gpr[18].wrapping_mul(4))
                .wrapping_add(*displacement);

            exec.run(entry, &mut registers);
            expected.host_mxcsr = registers.host_mxcsr;
            assert_eq!(registers, expected, "{name} {level:?}");
            assert_eq!(context.calls, 1, "{name} {level:?}");
            assert_eq!(context.last_addr, expected_addr, "{name} {level:?}");
            assert_eq!(context.last_size, 8, "{name} {level:?}");
            assert_eq!(context.last_signed, 0, "{name} {level:?}");
        }
    }
}
