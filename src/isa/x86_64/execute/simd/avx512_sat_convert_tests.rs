//! Direct-execution regressions for AVX10.2 saturating conversions.

use std::sync::Arc;

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use super::avx512::{read_reg_bytes, write_vec_vl};
use super::avx512_sat_convert::SatFpToIntKind;
use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::smir::interpret::{BlockResult, SmirInterpreter};
use crate::smir::ir::context::{ArchRegState, ExitReason, SmirContext, VecValue};
use crate::smir::ir::memory::FlatMemory;
use crate::smir::ir::types::{FunctionId, SourceArch};
use crate::smir::ir::{FunctionBuilder, Terminator, TrapKind};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::vm::vcpu::VCpu;

const CODE: u64 = 0x1000;
const DATA: u64 = 0x3000;
const SENTINEL: u64 = 0xA5A5_5A5A_C3C3_3C3C;
const MXCSR_INVALID: u32 = 1;
const MXCSR_PRECISION: u32 = 1 << 5;

fn vcpu_with_memory_size(code: &[u8], memory_size: usize) -> X86_64Vcpu {
    let memory =
        Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), memory_size)]).unwrap());
    memory.write_slice(code, GuestAddress(CODE)).unwrap();
    let mut vcpu = X86_64Vcpu::new(0, memory);
    vcpu.regs.rip = CODE;
    vcpu.regs.rflags = 0x2 | (1 << 0) | (1 << 6) | (1 << 10);
    vcpu.sregs.efer = 1 << 10;
    vcpu.sregs.cs.l = true;
    vcpu.sregs.cs.db = false;
    vcpu.set_avx10_sat_convert_enabled(true);
    vcpu
}

fn vcpu(code: &[u8]) -> X86_64Vcpu {
    vcpu_with_memory_size(code, 0x10000)
}

fn fields(kind: SatFpToIntKind) -> (u8, bool, usize) {
    match kind {
        SatFpToIntKind::F32ToI8 { signed: true } => (0x68, false, 4),
        SatFpToIntKind::F32ToI8 { signed: false } => (0x6A, false, 4),
        SatFpToIntKind::F64ToI64 { signed: true } => (0x6D, true, 8),
        SatFpToIntKind::F64ToI64 { signed: false } => (0x6C, true, 8),
    }
}

#[allow(clippy::too_many_arguments)]
fn encoding(
    kind: SatFpToIntKind,
    ll: u8,
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
    embedded_control_or_broadcast: bool,
    memory: bool,
    disp8: Option<i8>,
) -> Vec<u8> {
    assert!(ll < 4 && destination < 32 && source < 32 && mask < 8);
    let (opcode, w, _) = fields(kind);
    let mut p0 = 0xF5;
    if destination & 0x08 != 0 {
        p0 &= !0x80;
    }
    if destination & 0x10 != 0 {
        p0 &= !0x10;
    }
    if !memory && source & 0x08 != 0 {
        p0 &= !0x20;
    }
    if !memory && source & 0x10 != 0 {
        p0 &= !0x40;
    }
    let mut bytes = vec![
        0x62,
        p0,
        0x7D | if w { 0x80 } else { 0 },
        (if zeroing { 0x80 } else { 0 })
            | (ll << 5)
            | if embedded_control_or_broadcast {
                0x10
            } else {
                0
            }
            | 0x08
            | mask,
        opcode,
        (if memory {
            if disp8.is_some() { 0x40 } else { 0 }
        } else {
            0xC0
        }) | ((destination & 0x07) << 3)
            | if memory { 0 } else { source & 0x07 },
    ];
    if let Some(displacement) = disp8 {
        bytes.push(displacement as u8);
    }
    bytes
}

fn width(ll: u8, sae: bool) -> usize {
    if sae { 64 } else { 16usize << ll }
}

fn cases(kind: SatFpToIntKind) -> Vec<(u64, u64, u32)> {
    match kind {
        SatFpToIntKind::F32ToI8 { signed: true } => vec![
            (u64::from((-129.0f32).to_bits()), 0x80, MXCSR_INVALID),
            (u64::from((-128.9f32).to_bits()), 0x80, MXCSR_PRECISION),
            (u64::from((-1.9f32).to_bits()), 0xFF, MXCSR_PRECISION),
            (u64::from((-0.5f32).to_bits()), 0, MXCSR_PRECISION),
            (u64::from((-0.0f32).to_bits()), 0, 0),
            (u64::from(1.9f32.to_bits()), 1, MXCSR_PRECISION),
            (u64::from(127.9f32.to_bits()), 0x7F, MXCSR_PRECISION),
            (u64::from(128.0f32.to_bits()), 0x7F, MXCSR_INVALID),
            (u64::from(f32::NAN.to_bits()), 0, MXCSR_INVALID),
            (u64::from(f32::INFINITY.to_bits()), 0x7F, MXCSR_INVALID),
            (u64::from(f32::NEG_INFINITY.to_bits()), 0x80, MXCSR_INVALID),
            (1, 0, MXCSR_PRECISION),
        ],
        SatFpToIntKind::F32ToI8 { signed: false } => vec![
            (u64::from((-1.0f32).to_bits()), 0, MXCSR_INVALID),
            (u64::from((-0.5f32).to_bits()), 0, MXCSR_PRECISION),
            (u64::from((-0.0f32).to_bits()), 0, 0),
            (u64::from(1.9f32.to_bits()), 1, MXCSR_PRECISION),
            (u64::from(255.9f32.to_bits()), 0xFF, MXCSR_PRECISION),
            (u64::from(256.0f32.to_bits()), 0xFF, MXCSR_INVALID),
            (u64::from(f32::NAN.to_bits()), 0, MXCSR_INVALID),
            (u64::from(f32::INFINITY.to_bits()), 0xFF, MXCSR_INVALID),
            (u64::from(f32::NEG_INFINITY.to_bits()), 0, MXCSR_INVALID),
            (1, 0, MXCSR_PRECISION),
        ],
        SatFpToIntKind::F64ToI64 { signed: true } => vec![
            ((-9_223_372_036_854_775_808.0f64).to_bits(), 1 << 63, 0),
            ((-1.9f64).to_bits(), u64::MAX, MXCSR_PRECISION),
            ((-0.5f64).to_bits(), 0, MXCSR_PRECISION),
            ((-0.0f64).to_bits(), 0, 0),
            (1.9f64.to_bits(), 1, MXCSR_PRECISION),
            (
                f64::from_bits(9_223_372_036_854_775_808.0f64.to_bits() - 1).to_bits(),
                0x7FFF_FFFF_FFFF_FC00,
                0,
            ),
            (
                9_223_372_036_854_775_808.0f64.to_bits(),
                i64::MAX as u64,
                MXCSR_INVALID,
            ),
            (f64::NAN.to_bits(), 0, MXCSR_INVALID),
            (f64::INFINITY.to_bits(), i64::MAX as u64, MXCSR_INVALID),
            (f64::NEG_INFINITY.to_bits(), 1 << 63, MXCSR_INVALID),
            (1, 0, MXCSR_PRECISION),
        ],
        SatFpToIntKind::F64ToI64 { signed: false } => vec![
            ((-1.0f64).to_bits(), 0, MXCSR_INVALID),
            ((-0.5f64).to_bits(), 0, MXCSR_PRECISION),
            ((-0.0f64).to_bits(), 0, 0),
            (1.9f64.to_bits(), 1, MXCSR_PRECISION),
            (
                f64::from_bits(18_446_744_073_709_551_616.0f64.to_bits() - 1).to_bits(),
                u64::MAX - 2047,
                0,
            ),
            (
                18_446_744_073_709_551_616.0f64.to_bits(),
                u64::MAX,
                MXCSR_INVALID,
            ),
            (f64::NAN.to_bits(), 0, MXCSR_INVALID),
            (f64::INFINITY.to_bits(), u64::MAX, MXCSR_INVALID),
            (f64::NEG_INFINITY.to_bits(), 0, MXCSR_INVALID),
            (1, 0, MXCSR_PRECISION),
        ],
    }
}

fn set_source(vcpu: &mut X86_64Vcpu, register: u8, kind: SatFpToIntKind, raw: &[u64]) {
    let (_, _, elem_bytes) = fields(kind);
    let mut bytes = [0u8; 64];
    for (lane, value) in raw.iter().copied().enumerate() {
        bytes[lane * elem_bytes..(lane + 1) * elem_bytes]
            .copy_from_slice(&value.to_le_bytes()[..elem_bytes]);
    }
    write_vec_vl(vcpu, register, 64, &bytes);
}

fn fill_destination(vcpu: &mut X86_64Vcpu, register: u8) {
    let mut bytes = [0u8; 64];
    for word in bytes.chunks_exact_mut(8) {
        word.copy_from_slice(&SENTINEL.to_le_bytes());
    }
    write_vec_vl(vcpu, register, 64, &bytes);
}

fn read_lane(bytes: &[u8; 64], lane: usize, elem_bytes: usize) -> u64 {
    let mut raw = [0u8; 8];
    raw[..elem_bytes].copy_from_slice(&bytes[lane * elem_bytes..(lane + 1) * elem_bytes]);
    u64::from_le_bytes(raw)
}

fn sentinel_dword(lane: usize) -> u64 {
    SENTINEL >> ((lane & 1) * 32) & 0xFFFF_FFFF
}

fn gpr_snapshot(vcpu: &X86_64Vcpu) -> [u64; 32] {
    std::array::from_fn(|register| vcpu.get_reg(register as u8, 8))
}

fn vector_snapshot(vcpu: &X86_64Vcpu) -> [[u8; 64]; 32] {
    std::array::from_fn(|register| read_reg_bytes(vcpu, register as u8, 64))
}

#[test]
fn register_forms_cover_widths_extensions_boundaries_status_and_upper_zeroing() {
    for kind in [
        SatFpToIntKind::F32ToI8 { signed: true },
        SatFpToIntKind::F32ToI8 { signed: false },
        SatFpToIntKind::F64ToI64 { signed: true },
        SatFpToIntKind::F64ToI64 { signed: false },
    ] {
        let cases = cases(kind);
        let (_, _, elem_bytes) = fields(kind);
        for ll in 0..=2 {
            let destination = [1, 17, 31][ll as usize];
            let source = [2, 18, 30][ll as usize];
            let operation_bytes = width(ll, false);
            let lanes = operation_bytes / elem_bytes;
            let code = encoding(kind, ll, destination, source, 0, false, false, false, None);
            let mut cpu = vcpu(&code);
            let raw: Vec<_> = (0..lanes).map(|lane| cases[lane % cases.len()].0).collect();
            fill_destination(&mut cpu, destination);
            set_source(&mut cpu, source, kind, &raw);
            let flags_before = cpu.regs.rflags;
            let gprs_before = gpr_snapshot(&cpu);
            let vectors_before = vector_snapshot(&cpu);
            let masks_before = cpu.regs.k;
            assert!(cpu.step().unwrap().is_none(), "{kind:?} {code:02X?}");

            let actual = read_reg_bytes(&cpu, destination, 64);
            let mut expected_status = 0;
            for lane in 0..lanes {
                let (_, expected, lane_status) = cases[lane % cases.len()];
                expected_status |= lane_status;
                assert_eq!(
                    read_lane(&actual, lane, elem_bytes),
                    expected,
                    "{kind:?} L'L={ll} lane={lane} raw={:#x}",
                    raw[lane]
                );
            }
            assert!(actual[operation_bytes..].iter().all(|byte| *byte == 0));
            assert_eq!(cpu.mxcsr & 0x3F, expected_status);
            assert_eq!(cpu.regs.rip, CODE + code.len() as u64);
            assert_eq!(cpu.regs.rflags, flags_before);
            assert_eq!(gpr_snapshot(&cpu), gprs_before);
            assert_eq!(cpu.regs.k, masks_before);
            for register in 0..32 {
                if register != usize::from(destination) {
                    assert_eq!(
                        read_reg_bytes(&cpu, register as u8, 64),
                        vectors_before[register],
                        "{kind:?} unrelated ZMM{register}"
                    );
                }
            }
        }
    }
}

#[test]
fn masks_zero_or_merge_whole_dword_slots_and_aliases_snapshot_the_source() {
    let kind = SatFpToIntKind::F32ToI8 { signed: true };
    let raw = [
        u64::from(1.9f32.to_bits()),
        u64::from(f32::NAN.to_bits()),
        u64::from(127.9f32.to_bits()),
        u64::from(128.0f32.to_bits()),
    ];
    for zeroing in [false, true] {
        let code = encoding(kind, 0, 1, 2, 1, zeroing, false, false, None);
        let mut cpu = vcpu(&code);
        fill_destination(&mut cpu, 1);
        set_source(&mut cpu, 2, kind, &raw);
        cpu.regs.k[1] = 0b0101;
        assert!(cpu.step().unwrap().is_none());
        let actual = read_reg_bytes(&cpu, 1, 64);
        assert_eq!(read_lane(&actual, 0, 4), 1);
        assert_eq!(
            read_lane(&actual, 1, 4),
            if zeroing { 0 } else { sentinel_dword(1) }
        );
        assert_eq!(read_lane(&actual, 2, 4), 0x7F);
        assert_eq!(
            read_lane(&actual, 3, 4),
            if zeroing { 0 } else { sentinel_dword(3) }
        );
        assert_eq!(cpu.mxcsr & 0x3F, MXCSR_PRECISION);
    }

    let code = encoding(kind, 0, 2, 2, 0, false, false, false, None);
    let mut alias = vcpu(&code);
    set_source(&mut alias, 2, kind, &raw);
    assert!(alias.step().unwrap().is_none());
    let actual = read_reg_bytes(&alias, 2, 64);
    for (lane, expected) in [1, 0, 0x7F, 0x7F].into_iter().enumerate() {
        assert_eq!(read_lane(&actual, lane, 4), expected);
    }
    assert_eq!(alias.mxcsr & 0x3F, MXCSR_INVALID | MXCSR_PRECISION);
}

#[test]
fn sae_and_daz_have_exact_dynamic_mxcsr_behavior() {
    for kind in [
        SatFpToIntKind::F32ToI8 { signed: true },
        SatFpToIntKind::F64ToI64 { signed: false },
    ] {
        let code = encoding(kind, 0, 17, 18, 0, false, true, false, None);
        let mut cpu = vcpu(&code);
        let cases = cases(kind);
        let (_, _, elem_bytes) = fields(kind);
        let lanes = 64 / elem_bytes;
        let raw: Vec<_> = (0..lanes).map(|lane| cases[lane % cases.len()].0).collect();
        fill_destination(&mut cpu, 17);
        set_source(&mut cpu, 18, kind, &raw);
        cpu.mxcsr = 0;
        assert!(cpu.step().unwrap().is_none(), "{kind:?} {code:02X?}");
        assert_eq!(cpu.mxcsr, 0, "{kind:?} SAE must suppress status and traps");
        assert_eq!(cpu.regs.rip, CODE + 6);
        let actual = read_reg_bytes(&cpu, 17, 64);
        for lane in 0..lanes {
            assert_eq!(
                read_lane(&actual, lane, elem_bytes),
                cases[lane % cases.len()].1,
                "{kind:?} SAE lane {lane}"
            );
        }
    }

    let kind = SatFpToIntKind::F32ToI8 { signed: true };
    for (mxcsr, expected_status) in [(0x1F80, MXCSR_PRECISION), (0x1F80 | (1 << 6), 0)] {
        let code = encoding(kind, 0, 1, 2, 0, false, false, false, None);
        let mut cpu = vcpu(&code);
        set_source(&mut cpu, 2, kind, &[1, 0, 0, 0]);
        cpu.mxcsr = mxcsr;
        assert!(cpu.step().unwrap().is_none());
        assert_eq!(read_lane(&read_reg_bytes(&cpu, 1, 64), 0, 4), 0);
        assert_eq!(cpu.mxcsr & 0x3F, expected_status);
    }
}

fn assert_unmasked_exception(raw: u64, mask_bit: u32, status_bit: u32, vector: u8, cr4: u64) {
    let kind = SatFpToIntKind::F32ToI8 { signed: true };
    let code = encoding(kind, 0, 1, 2, 0, false, false, false, None);
    let mut cpu = vcpu(&code);
    fill_destination(&mut cpu, 1);
    set_source(&mut cpu, 2, kind, &[raw, 0, 0, 0]);
    cpu.mxcsr = 0x1F80 & !mask_bit;
    cpu.sregs.cr4 = cr4;
    let registers_before = cpu.regs.clone();
    let error = cpu
        .step()
        .expect_err("unmasked saturating conversion exception must not retire");
    assert!(
        format!("{error:?}").contains(&format!("IDT entry {vector} not present")),
        "wrong exception: {error:?}"
    );
    assert_eq!(cpu.regs.rip, registers_before.rip);
    assert_eq!(cpu.regs.xmm, registers_before.xmm);
    assert_eq!(cpu.regs.ymm_high, registers_before.ymm_high);
    assert_eq!(cpu.regs.zmm_high, registers_before.zmm_high);
    assert_eq!(cpu.regs.zmm_ext, registers_before.zmm_ext);
    assert_eq!(cpu.mxcsr & status_bit, status_bit);
}

#[test]
fn unmasked_invalid_and_precision_are_precise_and_obey_osxmmexcpt() {
    for (vector, cr4) in [(19, 1 << 10), (6, 0)] {
        assert_unmasked_exception(
            u64::from(f32::NAN.to_bits()),
            1 << 7,
            MXCSR_INVALID,
            vector,
            cr4,
        );
        assert_unmasked_exception(
            u64::from(1.5f32.to_bits()),
            1 << 12,
            MXCSR_PRECISION,
            vector,
            cr4,
        );
    }

    // IE is pre-computation and PE is post-computation. If both are
    // unmasked, the first execution reports only IE. If IE is masked, the
    // instruction reaches the post-computation phase and reports both flags
    // before the unmasked PE trap.
    for (invalid_masked, expected_status) in [
        (false, MXCSR_INVALID),
        (true, MXCSR_INVALID | MXCSR_PRECISION),
    ] {
        let kind = SatFpToIntKind::F32ToI8 { signed: true };
        let code = encoding(kind, 0, 1, 2, 0, false, false, false, None);
        let mut cpu = vcpu(&code);
        fill_destination(&mut cpu, 1);
        set_source(
            &mut cpu,
            2,
            kind,
            &[
                u64::from(f32::NAN.to_bits()),
                u64::from(1.5f32.to_bits()),
                0,
                0,
            ],
        );
        cpu.mxcsr = (0x1F80 & !(1 << 12)) & if invalid_masked { u32::MAX } else { !(1 << 7) };
        cpu.sregs.cr4 = 1 << 10;
        let registers_before = cpu.regs.clone();
        let error = cpu.step().expect_err("mixed IE/PE must not retire");
        assert!(format!("{error:?}").contains("IDT entry 19 not present"));
        assert_eq!(cpu.mxcsr & 0x3F, expected_status);
        assert_eq!(cpu.regs.rip, registers_before.rip);
        assert_eq!(cpu.regs.xmm, registers_before.xmm);
        assert_eq!(cpu.regs.ymm_high, registers_before.ymm_high);
        assert_eq!(cpu.regs.zmm_high, registers_before.zmm_high);
        assert_eq!(cpu.regs.zmm_ext, registers_before.zmm_ext);
    }
}

#[test]
fn memory_forms_apply_masks_broadcast_and_compressed_disp8_before_conversion() {
    let qword = SatFpToIntKind::F64ToI64 { signed: true };
    for (zeroing, inactive) in [(false, SENTINEL), (true, 0)] {
        let code = encoding(qword, 0, 1, 0, 1, zeroing, false, true, None);
        let mut cpu = vcpu(&code);
        cpu.regs.rax = 0x2_0000;
        cpu.regs.k[1] = 0;
        fill_destination(&mut cpu, 1);
        assert!(cpu.step().unwrap().is_none());
        let actual = read_reg_bytes(&cpu, 1, 64);
        assert_eq!(read_lane(&actual, 0, 8), inactive);
        assert_eq!(read_lane(&actual, 1, 8), inactive);
        assert!(actual[16..].iter().all(|byte| *byte == 0));
    }

    let bytes = SatFpToIntKind::F32ToI8 { signed: false };
    let code = encoding(bytes, 2, 17, 0, 2, false, false, true, Some(1));
    let mut full = vcpu(&code);
    full.regs.rax = DATA;
    full.regs.k[2] = 0x5555;
    fill_destination(&mut full, 17);
    for lane in 0..16 {
        full.write_mem(
            DATA + 64 + (lane * 4) as u64,
            u64::from((lane as f32 + 0.75).to_bits()),
            4,
        )
        .unwrap();
    }
    assert!(full.step().unwrap().is_none());
    let actual = read_reg_bytes(&full, 17, 64);
    for lane in 0..16 {
        assert_eq!(
            read_lane(&actual, lane, 4),
            if lane & 1 == 0 {
                lane as u64
            } else {
                sentinel_dword(lane)
            },
            "full tuple lane {lane}"
        );
    }
    assert_eq!(full.mxcsr & 0x3F, MXCSR_PRECISION);

    let qword = SatFpToIntKind::F64ToI64 { signed: false };
    let code = encoding(qword, 2, 17, 0, 3, true, true, true, Some(1));
    let mut broadcast = vcpu(&code);
    broadcast.regs.rax = DATA;
    broadcast.regs.k[3] = 0x55;
    broadcast.write_mem(DATA + 8, 1.9f64.to_bits(), 8).unwrap();
    assert!(broadcast.step().unwrap().is_none());
    let actual = read_reg_bytes(&broadcast, 17, 64);
    for lane in 0..8 {
        assert_eq!(
            read_lane(&actual, lane, 8),
            if lane & 1 == 0 { 1 } else { 0 }
        );
    }
    assert_eq!(broadcast.mxcsr & 0x3F, MXCSR_PRECISION);
}

#[test]
fn a_later_memory_fault_precedes_status_and_every_destination_commit() {
    let kind = SatFpToIntKind::F64ToI64 { signed: true };
    let code = encoding(kind, 0, 1, 0, 1, false, false, true, None);
    let mut cpu = vcpu(&code);
    cpu.regs.rax = 0xFFF8;
    cpu.regs.k[1] = 0b11;
    cpu.write_mem(0xFFF8, f64::NAN.to_bits(), 8).unwrap();
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

    let mut suppressed = vcpu(&code);
    suppressed.regs.rax = 0xFFF8;
    suppressed.regs.k[1] = 0b01;
    suppressed.write_mem(0xFFF8, f64::NAN.to_bits(), 8).unwrap();
    fill_destination(&mut suppressed, 1);
    assert!(suppressed.step().unwrap().is_none());
    let actual = read_reg_bytes(&suppressed, 1, 64);
    assert_eq!(read_lane(&actual, 0, 8), 0);
    assert_eq!(read_lane(&actual, 1, 8), SENTINEL);
    assert_eq!(suppressed.mxcsr & 0x3F, MXCSR_INVALID);
}

fn assert_ud_before_state_or_memory(code: &[u8], feature_enabled: bool) {
    let mut cpu = vcpu(code);
    cpu.set_avx10_sat_convert_enabled(feature_enabled);
    cpu.regs.rax = 0x2_0000;
    fill_destination(&mut cpu, 0);
    let registers_before = cpu.regs.clone();
    let mxcsr_before = cpu.mxcsr;
    let error = cpu
        .step()
        .expect_err("reserved or disabled saturation conversion must #UD");
    assert!(
        format!("{error:?}").contains("IDT entry 6 not present"),
        "{code:02X?}: {error:?}"
    );
    assert_eq!(cpu.regs.rip, registers_before.rip, "{code:02X?}");
    assert_eq!(cpu.regs.xmm, registers_before.xmm, "{code:02X?}");
    assert_eq!(cpu.regs.ymm_high, registers_before.ymm_high, "{code:02X?}");
    assert_eq!(cpu.regs.zmm_high, registers_before.zmm_high, "{code:02X?}");
    assert_eq!(cpu.regs.zmm_ext, registers_before.zmm_ext, "{code:02X?}");
    assert_eq!(cpu.mxcsr, mxcsr_before, "{code:02X?}");
}

#[test]
fn feature_gate_and_reserved_fields_fail_before_address_or_state_access() {
    let kind = SatFpToIntKind::F32ToI8 { signed: true };
    let memory = encoding(kind, 0, 0, 0, 0, false, false, true, None);
    assert_ud_before_state_or_memory(&memory, false);

    let valid = encoding(kind, 0, 0, 0, 0, false, false, true, None);
    let mut invalid = Vec::new();
    let mut vvvv = valid.clone();
    vvvv[2] &= !0x08;
    invalid.push(vvvv);
    let mut v_prime = valid.clone();
    v_prime[3] &= !0x08;
    invalid.push(v_prime);
    let mut zeroing_k0 = valid;
    zeroing_k0[3] |= 0x80;
    invalid.push(zeroing_k0);

    let register = encoding(kind, 0, 0, 1, 0, false, false, false, None);
    let mut ll3 = register.clone();
    ll3[3] |= 0x60;
    invalid.push(ll3);
    let mut invalid_sae_ll = register;
    invalid_sae_ll[3] |= 0x30;
    invalid.push(invalid_sae_ll);

    for code in invalid {
        assert_ud_before_state_or_memory(&code, true);
    }
}

fn vec_value_bytes(value: &VecValue) -> [u8; 64] {
    let mut bytes = [0u8; 64];
    for (lane, word) in value[..8].iter().enumerate() {
        bytes[lane * 8..(lane + 1) * 8].copy_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn set_direct_vec(vcpu: &mut X86_64Vcpu, register: u8, value: &VecValue) {
    write_vec_vl(vcpu, register, 64, &vec_value_bytes(value));
}

#[allow(clippy::too_many_arguments)]
fn assert_direct_smir_parity(
    bytes: &[u8],
    destination: u8,
    source_register: u8,
    source: VecValue,
    old_destination: VecValue,
    mask_register: Option<(usize, u64)>,
    mxcsr: u32,
) {
    let mut direct = vcpu(bytes);
    direct.mxcsr = mxcsr;
    set_direct_vec(&mut direct, destination, &old_destination);
    set_direct_vec(&mut direct, source_register, &source);
    if let Some((register, value)) = mask_register {
        direct.regs.k[register] = value;
    }
    assert!(direct.step().unwrap().is_none());

    let mut lifter = X86_64Lifter::strict();
    let mut lift_ctx = LiftContext::new(SourceArch::X86_64);
    let lifted = lifter.lift_insn(CODE, bytes, &mut lift_ctx).unwrap();
    assert_eq!(lifted.bytes_consumed, bytes.len());
    let mut builder = FunctionBuilder::new(FunctionId(0), CODE);
    builder.set_terminator(Terminator::Trap {
        kind: TrapKind::Halt,
    });
    let mut function = builder.finish();
    function.blocks[0].ops = lifted.ops;

    let mut smir = SmirContext::new_x86_64();
    if let ArchRegState::X86_64(x86) = &mut smir.arch_regs {
        x86.xmm[destination as usize] = old_destination;
        x86.xmm[source_register as usize] = source;
        if let Some((register, value)) = mask_register {
            x86.k[register] = value;
        }
        x86.mxcsr = mxcsr;
    }
    let exit = SmirInterpreter::new().execute_block(
        &mut smir,
        &mut FlatMemory::new(0x4000),
        &function.blocks[0],
    );
    assert!(matches!(exit, BlockResult::Exit(ExitReason::Halt)));
    let ArchRegState::X86_64(x86) = &smir.arch_regs else {
        unreachable!()
    };
    assert_eq!(
        read_reg_bytes(&direct, destination, 64),
        vec_value_bytes(&x86.xmm[destination as usize])
    );
    assert!(
        x86.xmm[destination as usize][8..]
            .iter()
            .all(|word| *word == 0)
    );
    assert_eq!(direct.mxcsr, x86.mxcsr);
    assert_eq!(direct.regs.rip, CODE + bytes.len() as u64);
}

#[test]
fn direct_execution_and_canonical_smir_match_register_masks_and_sae() {
    let mut f32_source = [0u64; 16];
    for (lane, value) in [1.9, f32::NAN, 127.9, f32::INFINITY]
        .into_iter()
        .enumerate()
    {
        SmirInterpreter::set_lane(&mut f32_source, lane as u8, 32, u64::from(value.to_bits()));
    }
    let old_destination = [SENTINEL; 16];
    assert_direct_smir_parity(
        &[0x62, 0xF5, 0x7D, 0x09, 0x68, 0xCA],
        1,
        2,
        f32_source,
        old_destination,
        Some((1, 0b0101)),
        0x1F80,
    );

    let mut f64_source = [0u64; 16];
    for (lane, value) in [
        -1.0,
        -0.5,
        1.9,
        f64::from_bits(18_446_744_073_709_551_616.0f64.to_bits() - 1),
        18_446_744_073_709_551_616.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ]
    .into_iter()
    .enumerate()
    {
        f64_source[lane] = value.to_bits();
    }
    assert_direct_smir_parity(
        &[0x62, 0xA5, 0xFD, 0xCB, 0x6C, 0xCA],
        17,
        18,
        f64_source,
        old_destination,
        Some((3, 0b1101_1011)),
        0x1F80,
    );

    for (lane, value) in [f32::NAN, f32::INFINITY, -129.0, 1.9]
        .into_iter()
        .enumerate()
    {
        SmirInterpreter::set_lane(&mut f32_source, lane as u8, 32, u64::from(value.to_bits()));
    }
    assert_direct_smir_parity(
        &[0x62, 0xF5, 0x7D, 0x18, 0x68, 0xCA],
        1,
        2,
        f32_source,
        old_destination,
        None,
        0,
    );
}
