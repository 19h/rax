//! Direct-execution regressions for F16C VEX precision conversion.

use std::sync::Arc;

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::vm::vcpu::{Registers, VCpu};

const CODE: u64 = 0x1000;
const SENTINEL: u64 = 0xA55A_6996_F00F_3CC3;

fn vcpu(code: &[u8]) -> X86_64Vcpu {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
    memory.write_slice(code, GuestAddress(CODE)).unwrap();
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.regs.rip = CODE;
    vcpu.regs.rflags = 0x2 | (1 << 0) | (1 << 6) | (1 << 10);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.db = false;
    vcpu
}

fn widen_encoding(ymm: bool, destination: u8, source: u8) -> [u8; 5] {
    assert!(destination < 16 && source < 16);
    let mut p0 = 0xE2;
    if destination >= 8 {
        p0 &= !0x80;
    }
    if source >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        0x79 | (u8::from(ymm) << 2),
        0x13,
        0xC0 | ((destination & 7) << 3) | (source & 7),
    ]
}

fn narrow_encoding(ymm: bool, destination: u8, source: u8, immediate: u8) -> [u8; 6] {
    assert!(destination < 16 && source < 16);
    let mut p0 = 0xE3;
    if source >= 8 {
        p0 &= !0x80;
    }
    if destination >= 8 {
        p0 &= !0x20;
    }
    [
        0xC4,
        p0,
        0x79 | (u8::from(ymm) << 2),
        0x1D,
        0xC0 | ((source & 7) << 3) | (destination & 7),
        immediate,
    ]
}

fn set_fp16_source(vcpu: &mut X86_64Vcpu, register: u8, lanes: [u16; 8]) {
    let mut bytes = [0u8; 16];
    for (lane, value) in lanes.into_iter().enumerate() {
        bytes[lane * 2..lane * 2 + 2].copy_from_slice(&value.to_le_bytes());
    }
    let index = usize::from(register);
    vcpu.regs.xmm[index][0] = u64::from_le_bytes(bytes[..8].try_into().unwrap());
    vcpu.regs.xmm[index][1] = u64::from_le_bytes(bytes[8..].try_into().unwrap());
}

fn set_fp32_source(vcpu: &mut X86_64Vcpu, register: u8, lanes: [u32; 8]) {
    let mut bytes = [0u8; 32];
    for (lane, value) in lanes.into_iter().enumerate() {
        bytes[lane * 4..lane * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    let index = usize::from(register);
    vcpu.regs.xmm[index][0] = u64::from_le_bytes(bytes[..8].try_into().unwrap());
    vcpu.regs.xmm[index][1] = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    vcpu.regs.ymm_high[index][0] = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    vcpu.regs.ymm_high[index][1] = u64::from_le_bytes(bytes[24..].try_into().unwrap());
}

fn fp16_destination(vcpu: &X86_64Vcpu, register: u8) -> [u16; 8] {
    let index = usize::from(register);
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&vcpu.regs.xmm[index][0].to_le_bytes());
    bytes[8..].copy_from_slice(&vcpu.regs.xmm[index][1].to_le_bytes());
    std::array::from_fn(|lane| {
        u16::from_le_bytes(bytes[lane * 2..lane * 2 + 2].try_into().unwrap())
    })
}

fn fill_destination(vcpu: &mut X86_64Vcpu, register: u8) {
    let index = usize::from(register);
    vcpu.regs.xmm[index] = [SENTINEL; 2];
    vcpu.regs.ymm_high[index] = [SENTINEL; 2];
    vcpu.regs.zmm_high[index] = [SENTINEL; 4];
}

fn vector_words(regs: &Registers, register: u8) -> [u64; 8] {
    let index = usize::from(register);
    [
        regs.xmm[index][0],
        regs.xmm[index][1],
        regs.ymm_high[index][0],
        regs.ymm_high[index][1],
        regs.zmm_high[index][0],
        regs.zmm_high[index][1],
        regs.zmm_high[index][2],
        regs.zmm_high[index][3],
    ]
}

fn interpret_narrow(
    code: &[u8],
    initial: &Registers,
    mxcsr: u32,
    destination: u8,
    source: u8,
) -> ([u64; 8], [u64; 8], u32, u64) {
    use crate::smir::interpret::{BlockResult, SmirInterpreter};
    use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext};
    use crate::smir::ir::flags::MaterializedFlags;
    use crate::smir::ir::memory::FlatMemory;
    use crate::smir::ir::types::BlockId;
    use crate::smir::ir::{SmirBlock, Terminator, TrapKind};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    let mut lifter = X86_64Lifter::strict();
    let mut lift_context = LiftContext::new(crate::smir::ir::types::SourceArch::X86_64);
    let result = lifter
        .lift_insn(CODE, code, &mut lift_context)
        .unwrap_or_else(|error| panic!("{code:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, code.len(), "{code:02X?}");

    let mut block = SmirBlock::new(BlockId(0), CODE);
    block.ops = result.ops;
    block.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });

    let mut context = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut context.arch_regs {
        for register in 0..16 {
            x86.xmm[register][..8].copy_from_slice(&vector_words(initial, register as u8));
        }
        x86.rflags = initial.rflags;
        x86.mxcsr = mxcsr;
    }
    context.flags.materialized = MaterializedFlags::from_rflags(initial.rflags);
    context.flags.lazy = None;
    let result =
        SmirInterpreter::new().execute_block(&mut context, &mut FlatMemory::new(1), &block);
    assert!(matches!(result, BlockResult::Exit(ExitReason::Halt)));

    let ArchRegState::X86_64(x86) = &context.arch_regs else {
        unreachable!()
    };
    (
        x86.xmm[usize::from(destination)][..8].try_into().unwrap(),
        x86.xmm[usize::from(source)][..8].try_into().unwrap(),
        x86.mxcsr,
        x86.rflags,
    )
}

fn assert_registers_equal(actual: &Registers, expected: &Registers) {
    let gprs = |regs: &Registers| {
        [
            regs.rax, regs.rbx, regs.rcx, regs.rdx, regs.rsi, regs.rdi, regs.rsp, regs.rbp,
            regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15, regs.r16,
            regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23, regs.r24,
            regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
        ]
    };
    assert_eq!(gprs(actual), gprs(expected));
    assert_eq!(actual.rip, expected.rip);
    assert_eq!(actual.rflags, expected.rflags);
    assert_eq!(actual.xmm, expected.xmm);
    assert_eq!(actual.ymm_high, expected.ymm_high);
    assert_eq!(actual.zmm_high, expected.zmm_high);
    assert_eq!(actual.zmm_ext, expected.zmm_ext);
    assert_eq!(actual.k, expected.k);
    assert_eq!(actual.mm, expected.mm);
}

#[test]
fn vcvtph2ps_masked_invalid_quiets_snan_and_clears_unchanged_low_destination() {
    let code = widen_encoding(false, 1, 2);
    let mut cpu = vcpu(&code);
    set_fp16_source(
        &mut cpu,
        2,
        [
            0x3C00, 0xC000, 0x7C01, 0x0001, 0x7E01, 0x7C00, 0xFC00, 0x8000,
        ],
    );
    let source_before = cpu.regs.xmm[2];

    // Preload the exact low result. A change-detection-only VEX wrapper would
    // see no low-state write and incorrectly retain the stale ZMM upper half.
    cpu.regs.xmm[1] = [0xC000_0000_3F80_0000, 0x3380_0000_7FC0_2000];
    cpu.regs.ymm_high[1] = [0; 2];
    cpu.regs.zmm_high[1] = [SENTINEL; 4];
    let rflags_before = cpu.regs.rflags;

    assert!(cpu.step().unwrap().is_none());
    assert_eq!(
        cpu.regs.xmm[1],
        [0xC000_0000_3F80_0000, 0x3380_0000_7FC0_2000]
    );
    assert_eq!(cpu.regs.ymm_high[1], [0; 2]);
    assert_eq!(cpu.regs.zmm_high[1], [0; 4]);
    assert_eq!(cpu.regs.xmm[2], source_before);
    assert_eq!(cpu.mxcsr, 0x1F81);
    assert_eq!(cpu.regs.rflags, rflags_before);
    assert_eq!(cpu.regs.rip, CODE + code.len() as u64);
}

fn assert_unmasked_invalid_is_precise(vector: u8, cr4: u64) {
    let code = widen_encoding(true, 9, 10);
    let mut cpu = vcpu(&code);
    fill_destination(&mut cpu, 9);
    set_fp16_source(&mut cpu, 10, [0x7C01; 8]);
    cpu.mxcsr = 0x1F80 & !(1 << 7);
    cpu.sregs.cr4 = cr4;
    let registers_before = cpu.regs.clone();

    let error = cpu
        .step()
        .expect_err("unmasked VCVTPH2PS invalid exception must not retire");
    assert!(
        format!("{error:?}").contains(&format!("IDT entry {vector} not present")),
        "wrong exception: {error:?}"
    );
    assert_eq!(cpu.regs.rip, registers_before.rip);
    assert_eq!(cpu.regs.xmm, registers_before.xmm);
    assert_eq!(cpu.regs.ymm_high, registers_before.ymm_high);
    assert_eq!(cpu.regs.zmm_high, registers_before.zmm_high);
    assert_eq!(cpu.regs.zmm_ext, registers_before.zmm_ext);
    assert_ne!(cpu.mxcsr & 1, 0);
}

#[test]
fn vcvtph2ps_unmasked_invalid_obeys_osxmmexcpt_without_destination_commit() {
    for (vector, cr4) in [(19, 1 << 10), (6, 0)] {
        assert_unmasked_invalid_is_precise(vector, cr4);
    }
}

#[test]
fn vcvtph2ps_later_memory_fault_precedes_invalid_status_and_destination_commit() {
    // VEX.128.66.0F38.W0 VCVTPH2PS xmm1, qword ptr [rax].
    let code = [0xC4, 0xE2, 0x79, 0x13, 0x08];
    let mut cpu = vcpu(&code);
    cpu.regs.rax = 0xFFFE;
    cpu.write_mem(0xFFFE, 0x7C01, 2).unwrap();
    fill_destination(&mut cpu, 1);
    let registers_before = cpu.regs.clone();
    let mxcsr_before = cpu.mxcsr;

    assert!(cpu.step().is_err());
    assert_eq!(cpu.regs.rip, registers_before.rip);
    assert_eq!(cpu.regs.xmm, registers_before.xmm);
    assert_eq!(cpu.regs.ymm_high, registers_before.ymm_high);
    assert_eq!(cpu.regs.zmm_high, registers_before.zmm_high);
    assert_eq!(cpu.regs.zmm_ext, registers_before.zmm_ext);
    assert_eq!(cpu.mxcsr, mxcsr_before);
}

#[test]
fn vcvtph2ps_reserved_w_and_vvvv_fault_before_memory_or_state_access() {
    let valid = [0xC4, 0xE2, 0x79, 0x13, 0x08];
    let mut w1 = valid;
    w1[2] |= 0x80;
    let mut vvvv = valid;
    vvvv[2] &= !0x08;

    for code in [w1, vvvv] {
        let mut cpu = vcpu(&code);
        cpu.regs.rax = 0x2_0000;
        fill_destination(&mut cpu, 1);
        let registers_before = cpu.regs.clone();
        let mxcsr_before = cpu.mxcsr;
        let error = cpu.step().expect_err("reserved VCVTPH2PS must #UD");
        assert!(
            format!("{error:?}").contains("IDT entry 6 not present"),
            "{code:02X?}: {error:?}"
        );
        assert_eq!(cpu.regs.rip, registers_before.rip, "{code:02X?}");
        assert_eq!(cpu.regs.xmm, registers_before.xmm, "{code:02X?}");
        assert_eq!(cpu.regs.ymm_high, registers_before.ymm_high, "{code:02X?}");
        assert_eq!(cpu.regs.zmm_high, registers_before.zmm_high, "{code:02X?}");
        assert_eq!(cpu.mxcsr, mxcsr_before, "{code:02X?}");
    }
}

#[test]
fn vcvtps2ph_masked_conversion_reports_all_defined_status_classes() {
    let code = narrow_encoding(true, 9, 10, 0);
    let mut cpu = vcpu(&code);
    set_fp32_source(
        &mut cpu,
        10,
        [
            0x0000_0001,
            0x8000_0000,
            1.0f32.to_bits(),
            (1.0f32 + 2.0f32.powi(-11)).to_bits(),
            2.0f32.powi(-24).to_bits(),
            2.0f32.powi(-25).to_bits(),
            f32::MAX.to_bits(),
            0x7F80_0001,
        ],
    );
    let source_before = (
        cpu.regs.xmm[10],
        cpu.regs.ymm_high[10],
        cpu.regs.zmm_high[10],
    );
    fill_destination(&mut cpu, 9);
    cpu.mxcsr = 0x1F80 | (1 << 15);
    let rflags_before = cpu.regs.rflags;

    assert!(cpu.step().unwrap().is_none());
    assert_eq!(
        fp16_destination(&cpu, 9),
        [0, 0x8000, 0x3C00, 0x3C00, 1, 0, 0x7C00, 0x7E00]
    );
    assert_eq!(cpu.regs.ymm_high[9], [0; 2]);
    assert_eq!(cpu.regs.zmm_high[9], [0; 4]);
    assert_eq!(
        (
            cpu.regs.xmm[10],
            cpu.regs.ymm_high[10],
            cpu.regs.zmm_high[10]
        ),
        source_before
    );
    assert_eq!(cpu.mxcsr & 0x3F, 0x3B);
    assert_ne!(cpu.mxcsr & (1 << 15), 0, "FTZ control must be preserved");
    assert_eq!(cpu.regs.rflags, rflags_before);
    assert_eq!(cpu.regs.rip, CODE + code.len() as u64);
}

#[test]
fn vcvtps2ph_dynamic_rounding_honors_daz_ignores_ftz_and_clears_unchanged_upper() {
    let code = narrow_encoding(false, 1, 2, 0xFF);
    let mut cpu = vcpu(&code);
    let midpoint = 1.0f32 + 2.0f32.powi(-11);
    set_fp32_source(
        &mut cpu,
        2,
        [
            1,
            2.0f32.powi(-24).to_bits(),
            midpoint.to_bits(),
            (-midpoint).to_bits(),
            0,
            0,
            0,
            0,
        ],
    );
    cpu.regs.xmm[1][0] = 0xBC00_3C01_0001_0000;
    cpu.regs.xmm[1][1] = 0;
    cpu.regs.ymm_high[1] = [0; 2];
    cpu.regs.zmm_high[1] = [SENTINEL; 4];
    cpu.mxcsr = 0x1F80 | (2 << 13) | (1 << 6) | (1 << 15);

    assert!(cpu.step().unwrap().is_none());
    assert_eq!(
        fp16_destination(&cpu, 1),
        [0, 1, 0x3C01, 0xBC00, 0, 0, 0, 0]
    );
    assert_eq!(cpu.regs.ymm_high[1], [0; 2]);
    assert_eq!(cpu.regs.zmm_high[1], [0; 4]);
    assert_eq!(cpu.mxcsr & 0x3F, 1 << 5);
    assert_ne!(cpu.mxcsr & (1 << 6), 0, "DAZ control must be preserved");
    assert_ne!(cpu.mxcsr & (1 << 15), 0, "FTZ control must be preserved");
}

#[test]
fn vcvtps2ph_immediate_rounding_modes_cover_ties_and_overflow_direction() {
    let midpoint = 1.0f32 + 2.0f32.powi(-11);
    for (rounding, expected, expected_status) in [
        (0, [0x3C00, 0xBC00, 0x7C00, 0xFC00], (1 << 3) | (1 << 5)),
        (1, [0x3C00, 0xBC01, 0x7BFF, 0xFC00], (1 << 3) | (1 << 5)),
        (2, [0x3C01, 0xBC00, 0x7C00, 0xFBFF], (1 << 3) | (1 << 5)),
        (3, [0x3C00, 0xBC00, 0x7BFF, 0xFBFF], 1 << 5),
    ] {
        // Imm[7:3] are architecturally ignored; Imm[2]=0 selects Imm[1:0].
        let code = narrow_encoding(false, 1, 2, 0xF8 | rounding);
        let mut cpu = vcpu(&code);
        set_fp32_source(
            &mut cpu,
            2,
            [
                midpoint.to_bits(),
                (-midpoint).to_bits(),
                65_520.0f32.to_bits(),
                (-65_520.0f32).to_bits(),
                0,
                0,
                0,
                0,
            ],
        );

        assert!(cpu.step().unwrap().is_none());
        assert_eq!(&fp16_destination(&cpu, 1)[..4], &expected, "RC={rounding}");
        assert_eq!(cpu.mxcsr & 0x3F, expected_status, "RC={rounding}");
    }
}

fn assert_vcvtps2ph_unmasked_invalid_is_precise(vector: u8, cr4: u64, memory: bool) {
    let code = if memory {
        // VCVTPS2PH qword ptr [rax], xmm2, 0.
        [0xC4, 0xE3, 0x79, 0x1D, 0x10, 0]
    } else {
        narrow_encoding(false, 1, 2, 0)
    };
    let mut cpu = vcpu(&code);
    set_fp32_source(&mut cpu, 2, [0x7F80_0001; 8]);
    fill_destination(&mut cpu, 1);
    cpu.regs.rax = 0x2000;
    cpu.write_mem(0x2000, SENTINEL, 8).unwrap();
    cpu.mxcsr = 0x1F80 & !(1 << 7);
    cpu.sregs.cr4 = cr4;
    let registers_before = cpu.regs.clone();

    let error = cpu
        .step()
        .expect_err("unmasked VCVTPS2PH invalid exception must not retire");
    assert!(
        format!("{error:?}").contains(&format!("IDT entry {vector} not present")),
        "wrong exception: {error:?}"
    );
    assert_registers_equal(&cpu.regs, &registers_before);
    assert_ne!(cpu.mxcsr & 1, 0);
    assert_eq!(cpu.read_mem(0x2000, 8).unwrap(), SENTINEL);
}

#[test]
fn vcvtps2ph_unmasked_invalid_obeys_osxmmexcpt_without_any_destination_commit() {
    for memory in [false, true] {
        for (vector, cr4) in [(19, 1 << 10), (6, 0)] {
            assert_vcvtps2ph_unmasked_invalid_is_precise(vector, cr4, memory);
        }
    }
}

#[test]
fn vcvtps2ph_late_memory_fault_precedes_status_and_partial_store() {
    // VCVTPS2PH xmmword ptr [rax], ymm2, 0.
    let code = [0xC4, 0xE3, 0x7D, 0x1D, 0x10, 0];
    let mut cpu = vcpu(&code);
    cpu.regs.rax = 0xFFF8;
    cpu.write_mem(0xFFF8, SENTINEL, 8).unwrap();
    set_fp32_source(&mut cpu, 2, [0x7F80_0001; 8]);
    cpu.mxcsr = 0x1F80 & !(1 << 7);
    let registers_before = cpu.regs.clone();
    let mxcsr_before = cpu.mxcsr;

    assert!(cpu.step().is_err());
    assert_registers_equal(&cpu.regs, &registers_before);
    assert_eq!(cpu.mxcsr, mxcsr_before);
    assert_eq!(cpu.read_mem(0xFFF8, 8).unwrap(), SENTINEL);
}

#[test]
fn vcvtps2ph_direct_matches_smir_for_all_immediates_controls_and_fp32_boundaries() {
    const FP32_BOUNDARIES: [u32; 32] = [
        0x0000_0000,
        0x8000_0000,
        0x0000_0001,
        0x8000_0001,
        0x007F_FFFF,
        0x807F_FFFF,
        0x0080_0000,
        0x8080_0000,
        0x32FF_FFFF,
        0x3300_0000,
        0x3300_0001,
        0x337F_FFFF,
        0x3380_0000,
        0x3380_0001,
        0x387F_FFFF,
        0x3880_0000,
        0x3880_0001,
        0x3F80_1000,
        0xBF80_1000,
        0x477F_DFFF,
        0x477F_E000,
        0x477F_EFFF,
        0x477F_F000,
        0x477F_F001,
        0x7F7F_FFFF,
        0xFF7F_FFFF,
        0x7F80_0000,
        0xFF80_0000,
        0x7FC1_2345,
        0xFFC1_2345,
        0x7F81_2345,
        0xFF81_2345,
    ];

    let destination = 9;
    let source = 10;
    let mut trials = 0usize;
    for ymm in [false, true] {
        for rc in 0u32..4 {
            for daz in [false, true] {
                for ftz in [false, true] {
                    for immediate in u8::MIN..=u8::MAX {
                        let code = narrow_encoding(ymm, destination, source, immediate);
                        let mut cpu = vcpu(&code);
                        fill_destination(&mut cpu, destination);
                        let profile = usize::from(immediate)
                            + usize::try_from(rc).unwrap() * 11
                            + usize::from(daz) * 17
                            + usize::from(ftz) * 23
                            + usize::from(ymm) * 29;
                        let lanes = std::array::from_fn(|lane| {
                            FP32_BOUNDARIES[(profile + lane * 7) % FP32_BOUNDARIES.len()]
                        });
                        set_fp32_source(&mut cpu, source, lanes);
                        let prior_status =
                            (u32::from(immediate).rotate_left(rc) ^ profile as u32) & 0x3F;
                        cpu.mxcsr = 0x1F80
                            | prior_status
                            | (rc << 13)
                            | (u32::from(daz) << 6)
                            | (u32::from(ftz) << 15);
                        let initial = cpu.regs.clone();
                        let expected =
                            interpret_narrow(&code, &initial, cpu.mxcsr, destination, source);

                        assert!(cpu.step().unwrap().is_none(), "{code:02X?}");
                        assert_eq!(
                            vector_words(&cpu.regs, destination),
                            expected.0,
                            "ymm={ymm} rc={rc} daz={daz} ftz={ftz} imm={immediate:#04X}"
                        );
                        assert_eq!(
                            vector_words(&cpu.regs, source),
                            expected.1,
                            "source changed: ymm={ymm} rc={rc} daz={daz} ftz={ftz} \
                             imm={immediate:#04X}"
                        );
                        assert_eq!(
                            cpu.mxcsr, expected.2,
                            "ymm={ymm} rc={rc} daz={daz} ftz={ftz} imm={immediate:#04X}"
                        );
                        assert_eq!(cpu.regs.rflags, expected.3);
                        trials += 1;
                    }
                }
            }
        }
    }
    assert_eq!(trials, 8_192);
}
