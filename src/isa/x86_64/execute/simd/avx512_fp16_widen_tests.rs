//! Direct-execution regressions for packed FP16 widening conversions.

use std::sync::Arc;

use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

use super::avx512::{f16_to_f32, read_reg_bytes, write_vec_vl};
use super::avx512_fp16_widen::Fp16WidenKind;
use crate::isa::x86_64::cpu::X86_64Vcpu;
use crate::vm::vcpu::VCpu;

const CODE: u64 = 0x1000;
const DATA: u64 = 0x3000;
const SENTINEL: u64 = 0xCAFE_BABE_DEAD_BEEF;

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

fn fields(kind: Fp16WidenKind) -> (u8, u8, u8) {
    match kind {
        Fp16WidenKind::ToF64 => (5, 0, 0x5A),
        Fp16WidenKind::ToF32 => (2, 1, 0x13),
        Fp16WidenKind::ToF32X => (6, 1, 0x13),
    }
}

#[allow(clippy::too_many_arguments)]
fn encoding(
    kind: Fp16WidenKind,
    ll: u8,
    destination: u8,
    source: u8,
    mask: u8,
    zeroing: bool,
    embedded_control: bool,
    memory: bool,
) -> [u8; 6] {
    assert!(ll < 4 && destination < 32 && source < 32 && mask < 8);
    let (map, pp, opcode) = fields(kind);
    let mut p0 = 0xF0 | map;
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
    [
        0x62,
        p0,
        0x7C | pp,
        (if zeroing { 0x80 } else { 0 })
            | (ll << 5)
            | if embedded_control { 0x10 } else { 0 }
            | 0x08
            | mask,
        opcode,
        (if memory { 0 } else { 0xC0 }) | ((destination & 0x07) << 3) | (source & 0x07),
    ]
}

fn destination_bytes(ll: u8, sae: bool) -> usize {
    if sae { 64 } else { 16usize << ll }
}

fn element_bytes(kind: Fp16WidenKind) -> usize {
    if kind == Fp16WidenKind::ToF64 { 8 } else { 4 }
}

fn lane_count(kind: Fp16WidenKind, ll: u8, sae: bool) -> usize {
    destination_bytes(ll, sae) / element_bytes(kind)
}

fn set_fp16_source(vcpu: &mut X86_64Vcpu, register: u8, values: &[u16]) {
    let mut bytes = [0u8; 64];
    for (lane, value) in values.iter().enumerate() {
        bytes[lane * 2..lane * 2 + 2].copy_from_slice(&value.to_le_bytes());
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

fn read_lane(bytes: &[u8; 64], lane: usize, width: usize) -> u64 {
    let mut raw = [0u8; 8];
    raw[..width].copy_from_slice(&bytes[lane * width..lane * width + width]);
    u64::from_le_bytes(raw)
}

fn gpr_snapshot(vcpu: &X86_64Vcpu) -> [u64; 32] {
    std::array::from_fn(|register| vcpu.get_reg(register as u8, 8))
}

fn vector_snapshot(vcpu: &X86_64Vcpu) -> [[u8; 64]; 32] {
    std::array::from_fn(|register| read_reg_bytes(vcpu, register as u8, 64))
}

fn expected(raw: u16, kind: Fp16WidenKind) -> u64 {
    let sign32 = u32::from(raw & 0x8000) << 16;
    let sign64 = u64::from(raw & 0x8000) << 48;
    let fraction = raw & 0x03FF;
    let nan = raw & 0x7C00 == 0x7C00 && fraction != 0;
    if kind == Fp16WidenKind::ToF64 {
        if nan {
            sign64 | 0x7FF0_0000_0000_0000 | (u64::from(fraction) << 42) | 0x0008_0000_0000_0000
        } else {
            f64::from(f16_to_f32(raw)).to_bits()
        }
    } else if nan {
        u64::from(sign32 | 0x7F80_0000 | (u32::from(fraction) << 13) | 0x0040_0000)
    } else {
        u64::from(f16_to_f32(raw).to_bits())
    }
}

#[test]
fn widening_covers_widths_extensions_masks_aliases_specials_and_full_state() {
    let patterns = [
        0x0000, 0x8000, 0x3C00, 0xC000, 0x0001, 0x7BFF, 0x7C00, 0xFC00, 0x7E01, 0x7C01, 0x3555,
        0xB555, 0x0400, 0x8400, 0x03FF, 0x83FF,
    ];
    for kind in [
        Fp16WidenKind::ToF64,
        Fp16WidenKind::ToF32,
        Fp16WidenKind::ToF32X,
    ] {
        for ll in 0..=2 {
            let destination = [1, 17, 31][ll as usize];
            let source = [2, 18, 30][ll as usize];
            let lanes = lane_count(kind, ll, false);
            let mask_bits = (1u64 << lanes) - 1 & !0b10;
            for zeroing in [false, true] {
                let code = encoding(kind, ll, destination, source, 3, zeroing, false, false);
                let mut cpu = vcpu(&code);
                let flags_before = cpu.regs.rflags;
                fill_destination(&mut cpu, destination);
                let old_destination = read_reg_bytes(&cpu, destination, 64);
                set_fp16_source(&mut cpu, source, &patterns[..lanes]);
                cpu.regs.k[3] = mask_bits;
                let gprs_before = gpr_snapshot(&cpu);
                let vectors_before = vector_snapshot(&cpu);
                let masks_before = cpu.regs.k;
                assert!(cpu.step().unwrap().is_none(), "{kind:?} {code:02X?}");

                let actual = read_reg_bytes(&cpu, destination, 64);
                let width = element_bytes(kind);
                for (lane, raw) in patterns[..lanes].iter().copied().enumerate() {
                    let expected_lane = if mask_bits & (1 << lane) != 0 {
                        expected(raw, kind)
                    } else if zeroing {
                        0
                    } else {
                        read_lane(&old_destination, lane, width)
                    };
                    assert_eq!(
                        read_lane(&actual, lane, width),
                        expected_lane,
                        "{kind:?} L'L={ll} lane={lane} {code:02X?}"
                    );
                }
                assert!(
                    actual[destination_bytes(ll, false)..]
                        .iter()
                        .all(|byte| *byte == 0)
                );
                assert_eq!(cpu.regs.rip, CODE + 6);
                assert_eq!(cpu.regs.rflags, flags_before);
                assert_eq!(gpr_snapshot(&cpu), gprs_before);
                assert_eq!(cpu.regs.k, masks_before);
                for register in 0..32 {
                    if register != usize::from(destination) {
                        assert_eq!(
                            read_reg_bytes(&cpu, register as u8, 64),
                            vectors_before[register],
                            "{kind:?} unrelated ZMM{register} {code:02X?}"
                        );
                    }
                }

                let active = patterns[..lanes]
                    .iter()
                    .enumerate()
                    .filter(|(lane, _)| mask_bits & (1 << lane) != 0)
                    .map(|(_, raw)| *raw);
                let mut expected_status = 0;
                for raw in active {
                    if raw & 0x7C00 == 0x7C00 && raw & 0x03FF != 0 && raw & 0x0200 == 0 {
                        expected_status |= 1;
                    }
                    if kind == Fp16WidenKind::ToF64 && raw & 0x7C00 == 0 && raw & 0x03FF != 0 {
                        expected_status |= 1 << 1;
                    }
                }
                assert_eq!(cpu.mxcsr & 0x3F, expected_status);
            }
        }
    }

    // Destination/source aliasing must snapshot both the packed FP16 source
    // and the merge destination before any widened lane is committed.
    let code = encoding(Fp16WidenKind::ToF64, 0, 17, 17, 1, false, false, false);
    let mut alias = vcpu(&code);
    let source = [0x3C00, 0x4000];
    set_fp16_source(&mut alias, 17, &source);
    let old = read_reg_bytes(&alias, 17, 64);
    alias.regs.k[1] = 0b01;
    assert!(alias.step().unwrap().is_none());
    let actual = read_reg_bytes(&alias, 17, 64);
    assert_eq!(read_lane(&actual, 0, 8), 1.0f64.to_bits());
    assert_eq!(read_lane(&actual, 1, 8), read_lane(&old, 1, 8));
}

#[test]
fn canonical_sae_is_512_bit_status_suppressing_and_noncanonical_ll_is_ud() {
    for kind in [
        Fp16WidenKind::ToF64,
        Fp16WidenKind::ToF32,
        Fp16WidenKind::ToF32X,
    ] {
        let code = encoding(kind, 0, 17, 18, 1, false, true, false);
        let mut cpu = vcpu(&code);
        fill_destination(&mut cpu, 17);
        let lanes = lane_count(kind, 0, true);
        let mut source = [0x3C00u16; 16];
        source[0] = 0x7C01;
        source[1] = 0x0001;
        set_fp16_source(&mut cpu, 18, &source[..lanes]);
        cpu.regs.k[1] = u64::MAX;
        cpu.mxcsr = 0;
        assert!(cpu.step().unwrap().is_none(), "{kind:?} {code:02X?}");
        assert_eq!(cpu.mxcsr, 0, "SAE status {kind:?}");
        assert_eq!(cpu.regs.rip, CODE + 6);
        let actual = read_reg_bytes(&cpu, 17, 64);
        assert_eq!(
            read_lane(&actual, 0, element_bytes(kind)),
            expected(0x7C01, kind)
        );
        assert_eq!(
            read_lane(&actual, 1, element_bytes(kind)),
            expected(0x0001, kind)
        );

        for ll in 1..=3 {
            let invalid = encoding(kind, ll, 1, 2, 0, false, true, false);
            assert_ud_before_state_or_memory(&invalid);
        }
    }
}

fn assert_unmasked_exception(kind: Fp16WidenKind, raw: u16, mask_bit: u32, vector: u8, cr4: u64) {
    let code = encoding(kind, 0, 1, 2, 0, false, false, false);
    let mut cpu = vcpu(&code);
    fill_destination(&mut cpu, 1);
    set_fp16_source(&mut cpu, 2, &[raw; 16]);
    cpu.mxcsr = 0x1F80 & !mask_bit;
    cpu.sregs.cr4 = cr4;
    let registers_before = cpu.regs.clone();
    let error = cpu
        .step()
        .expect_err("unmasked widening exception must not retire");
    assert!(
        format!("{error:?}").contains(&format!("IDT entry {vector} not present")),
        "wrong exception: {error:?}"
    );
    assert_eq!(cpu.regs.rip, registers_before.rip);
    assert_eq!(cpu.regs.xmm, registers_before.xmm);
    assert_eq!(cpu.regs.ymm_high, registers_before.ymm_high);
    assert_eq!(cpu.regs.zmm_high, registers_before.zmm_high);
    assert_eq!(cpu.regs.zmm_ext, registers_before.zmm_ext);
    assert_ne!(cpu.mxcsr & (mask_bit >> 7), 0);
}

#[test]
fn unmasked_invalid_and_denormal_exceptions_are_precise_and_obey_osxmmexcpt() {
    for (vector, cr4) in [(19, 1 << 10), (6, 0)] {
        assert_unmasked_exception(Fp16WidenKind::ToF32, 0x7C01, 1 << 7, vector, cr4);
        assert_unmasked_exception(Fp16WidenKind::ToF64, 0x0001, 1 << 8, vector, cr4);
    }
}

fn assert_ud_before_state_or_memory(code: &[u8]) {
    let mut cpu = vcpu(code);
    cpu.regs.rax = 0x2_0000;
    fill_destination(&mut cpu, 0);
    let registers_before = cpu.regs.clone();
    let mxcsr_before = cpu.mxcsr;
    let error = cpu.step().expect_err("reserved widening encoding must #UD");
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
fn reserved_fields_fail_before_effective_address_or_architectural_state_access() {
    let valid = encoding(Fp16WidenKind::ToF64, 0, 0, 0, 0, false, false, true);
    let mut invalid = Vec::new();
    let mut vvvv = valid;
    vvvv[2] &= !0x08;
    invalid.push(vvvv);
    let mut v_prime = valid;
    v_prime[3] &= !0x08;
    invalid.push(v_prime);
    let mut zeroing_k0 = valid;
    zeroing_k0[3] |= 0x80;
    invalid.push(zeroing_k0);
    let mut ll3 = valid;
    ll3[3] |= 0x60;
    invalid.push(ll3);
    let legacy_broadcast = encoding(Fp16WidenKind::ToF32, 0, 0, 0, 0, false, true, true);
    invalid.push(legacy_broadcast);
    for code in invalid {
        assert_ud_before_state_or_memory(&code);
    }
}

#[test]
fn memory_masks_suppress_accesses_broadcast_exactly_and_preserve_fault_priority() {
    // All-masked invalid memory performs no access and retains merging lanes.
    let code = encoding(Fp16WidenKind::ToF64, 0, 1, 0, 1, false, false, true);
    let mut masked = vcpu(&code);
    masked.regs.rax = 0x2_0000;
    fill_destination(&mut masked, 1);
    masked.regs.k[1] = 0;
    assert!(masked.step().unwrap().is_none());
    let actual = read_reg_bytes(&masked, 1, 64);
    assert_eq!(read_lane(&actual, 0, 8), SENTINEL);
    assert_eq!(read_lane(&actual, 1, 8), SENTINEL);
    assert!(actual[16..].iter().all(|byte| *byte == 0));

    // Non-broadcast memory reads preserve unaligned FP16 elements, apply the
    // lane mask before conversion, and use each family's denormal policy.
    let values = [
        0x0001, 0x8001, 0x7C01, 0xC000, 0x3C00, 0x7E01, 0x7C00, 0xFC00,
    ];
    for kind in [
        Fp16WidenKind::ToF64,
        Fp16WidenKind::ToF32,
        Fp16WidenKind::ToF32X,
    ] {
        let code = encoding(kind, 1, 17, 0, 1, false, false, true);
        let mut memory = vcpu(&code);
        memory.regs.rax = DATA + 1;
        let lanes = lane_count(kind, 1, false);
        for (lane, raw) in values[..lanes].iter().copied().enumerate() {
            memory
                .write_mem(DATA + 1 + (lane * 2) as u64, u64::from(raw), 2)
                .unwrap();
        }
        fill_destination(&mut memory, 17);
        let old_destination = read_reg_bytes(&memory, 17, 64);
        memory.regs.k[1] = 0b0101_0101;
        assert!(memory.step().unwrap().is_none(), "{kind:?} {code:02X?}");
        let actual = read_reg_bytes(&memory, 17, 64);
        for (lane, raw) in values[..lanes].iter().copied().enumerate() {
            assert_eq!(
                read_lane(&actual, lane, element_bytes(kind)),
                if memory.regs.k[1] & (1 << lane) != 0 {
                    expected(raw, kind)
                } else {
                    read_lane(&old_destination, lane, element_bytes(kind))
                },
                "{kind:?} memory lane {lane}"
            );
        }
        assert_ne!(memory.mxcsr & 1, 0, "{kind:?} memory invalid");
        assert_eq!(
            memory.mxcsr & (1 << 1),
            if kind == Fp16WidenKind::ToF64 {
                1 << 1
            } else {
                0
            },
            "{kind:?} non-broadcast DE"
        );
    }

    // A broadcast reads one FP16 value and replicates it to every active lane.
    for kind in [Fp16WidenKind::ToF64, Fp16WidenKind::ToF32X] {
        let code = encoding(kind, 1, 17, 0, 2, true, true, true);
        let mut broadcast = vcpu(&code);
        broadcast.regs.rax = DATA;
        broadcast.write_mem(DATA, 0x0001, 2).unwrap();
        broadcast.regs.k[2] = 0b0101_0101_0101_0101;
        assert!(broadcast.step().unwrap().is_none());
        let actual = read_reg_bytes(&broadcast, 17, 64);
        for lane in 0..lane_count(kind, 1, false) {
            assert_eq!(
                read_lane(&actual, lane, element_bytes(kind)),
                if broadcast.regs.k[2] & (1 << lane) != 0 {
                    expected(0x0001, kind)
                } else {
                    0
                }
            );
        }
        assert_ne!(broadcast.mxcsr & (1 << 1), 0, "{kind:?} broadcast DE");
    }

    // A later active memory fault precedes the invalid status from an earlier
    // signaling-NaN lane and prevents every architectural destination commit.
    let code = encoding(Fp16WidenKind::ToF32, 0, 1, 0, 1, false, false, true);
    let mut fault = vcpu(&code);
    fault.regs.rax = 0xFFFE;
    fault.write_mem(0xFFFE, 0x7C01, 2).unwrap();
    fault.regs.k[1] = 0b11;
    fill_destination(&mut fault, 1);
    let before = fault.regs.clone();
    let mxcsr_before = fault.mxcsr;
    assert!(fault.step().is_err());
    assert_eq!(fault.regs.rip, before.rip);
    assert_eq!(fault.regs.xmm, before.xmm);
    assert_eq!(fault.regs.ymm_high, before.ymm_high);
    assert_eq!(fault.regs.zmm_high, before.zmm_high);
    assert_eq!(fault.regs.zmm_ext, before.zmm_ext);
    assert_eq!(fault.mxcsr, mxcsr_before);
}
