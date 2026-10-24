use rax::isa::arm::aarch32::vfp::{self, Fpscr};
use rax::isa::arm::decoder::{Aarch32Decoder, ThumbDecoder};
use rax::isa::arm::execution::FlatMemory;
use rax::isa::arm::{Armv7Cpu, Executor};

fn narrow(raw: u64, control: u32, want: u32, status: u32) {
    let mut state = Fpscr::from_bits(control);
    let result = vfp::vcvt_f32_f64(f64::from_bits(raw), &mut state);
    assert_eq!(
        (result.to_bits(), state.bits()),
        (want, control | status),
        "helper {raw:016x} / {control:08x}"
    );
    for thumb in [false, true] {
        let mut cpu = Armv7Cpu::new();
        let mut memory = FlatMemory::new(0x1000, 0);
        cpu.cpsr.t = thumb;
        cpu.vfp.write_d_bits(0, 0xcafebabe_deadbeef);
        cpu.vfp.write_d_bits(1, raw);
        cpu.vfp.fpscr = Fpscr::from_bits(control);
        let cpsr = cpu.cpsr.to_u32();
        let instruction = if thumb {
            ThumbDecoder::decode_32bit(0xeeb70bc1)
        } else {
            Aarch32Decoder::decode(0xeeb70bc1)
        }
        .unwrap();
        assert!(matches!(
            Executor::new(&mut cpu, &mut memory).execute(&instruction),
            rax::isa::arm::aarch32::instructions::ExecResult::Continue
        ));
        assert_eq!(
            (cpu.vfp.read_d_bits(0), cpu.vfp.fpscr.bits()),
            (0xcafebabe_00000000 | u64::from(want), control | status),
            "decoded {raw:016x} / {control:08x}, Thumb={thumb}"
        );
        assert_eq!(cpu.vfp.read_d_bits(1), raw);
        assert_eq!(cpu.cpsr.to_u32(), cpsr);
    }
}
fn widen(raw: u32, control: u32, want: u64, status: u32) {
    for thumb in [false, true] {
        let mut cpu = Armv7Cpu::new();
        let mut memory = FlatMemory::new(0x1000, 0);
        cpu.cpsr.t = thumb;
        cpu.vfp
            .write_d_bits(1, 0x12345678_00000000 | u64::from(raw));
        cpu.vfp.fpscr = Fpscr::from_bits(control);
        let cpsr = cpu.cpsr.to_u32();
        let instruction = if thumb {
            ThumbDecoder::decode_32bit(0xeeb70ac1)
        } else {
            Aarch32Decoder::decode(0xeeb70ac1)
        }
        .unwrap();
        assert!(matches!(
            Executor::new(&mut cpu, &mut memory).execute(&instruction),
            rax::isa::arm::aarch32::instructions::ExecResult::Continue
        ));
        assert_eq!(
            (cpu.vfp.read_d_bits(0), cpu.vfp.fpscr.bits()),
            (want, control | status),
            "decoded {raw:08x} / {control:08x}, Thumb={thumb}"
        );
        assert_eq!(cpu.vfp.read_d_bits(1), 0x12345678_00000000 | u64::from(raw));
        assert_eq!(cpu.cpsr.to_u32(), cpsr);
    }
}
#[test]
fn narrowing_exact_finite() {
    narrow(0x3ff0000000000000, 0, 0x3f800000, 0);
}
#[test]
fn narrowing_infinity() {
    narrow(0x7ff0000000000000, 0, 0x7f800000, 0);
}
#[test]
fn narrowing_quiet_nan() {
    narrow(0xfff8123456789abc, 0, 0xffc091a2, 0);
}
#[test]
fn narrowing_signaling_nan() {
    narrow(0x7ff0000020000000, 0, 0x7fc00001, 1);
}
#[test]
fn narrowing_default_nan() {
    narrow(0xfff8123456789abc, 1 << 25, 0x7fc00000, 0);
}
#[test]
fn narrowing_round_nearest() {
    narrow(0x3ff0000010000000, 0, 0x3f800000, 16);
}
#[test]
fn narrowing_round_up() {
    narrow(0x3ff0000010000000, 1 << 22, 0x3f800001, 16);
}
#[test]
fn narrowing_negative_round_down() {
    narrow(0xbff0000010000000, 2 << 22, 0xbf800001, 16);
}
#[test]
fn narrowing_overflow_infinity() {
    narrow(0x47f0000000000000, 0, 0x7f800000, 20);
}
#[test]
fn narrowing_overflow_finite() {
    narrow(0x47f0000000000000, 3 << 22, 0x7f7fffff, 20);
}
#[test]
fn narrowing_exact_subnormal() {
    narrow(0x36a0000000000000, 0, 1, 0);
}
#[test]
fn narrowing_flush_output() {
    narrow(0x36a0000000000000, 1 << 24, 0, 8);
}
#[test]
fn narrowing_inexact_underflow() {
    narrow(1, 0, 0, 24);
}
#[test]
fn narrowing_flush_input() {
    narrow(1, 1 << 24, 0, 128);
}
#[test]
fn narrowing_existing_status() {
    narrow(0x3ff0000010000000, 0xa0000083, 0x3f800000, 16);
}
#[test]
fn widening_exact_subnormal() {
    widen(1, 0, 0x36a0000000000000, 0);
}
#[test]
fn widening_flush_input() {
    widen(0x80000001, 1 << 24, 0x8000000000000000, 128);
}
#[test]
fn widening_signaling_nan() {
    widen(0xff800001, 0, 0xfff8000020000000, 1);
}
#[test]
fn widening_default_nan() {
    widen(0xffc12345, 1 << 25, 0x7ff8000000000000, 0);
}
#[test]
fn widening_existing_status() {
    widen(0x7f800001, 0xa0000082, 0x7ff8000020000000, 1);
}

#[test]
fn unrepresented_precision_control_stays_out_of_both_lifters() {
    use rax::smir::ir::types::SourceArch;
    use rax::smir::lift::aarch32::Aarch32Lifter;
    use rax::smir::lift::thumb::ThumbLifter;
    use rax::smir::lift::{LiftContext, LiftError, SmirLifter};
    for word in [0xeeb70bc1_u32, 0xeeb70ac1] {
        for thumb in [false, true] {
            let mut lifter: Box<dyn SmirLifter> = if thumb {
                Box::new(ThumbLifter::new())
            } else {
                Box::new(Aarch32Lifter::new())
            };
            let arch = if thumb {
                SourceArch::Thumb
            } else {
                SourceArch::Aarch32
            };
            let mut context = LiftContext::new(arch);
            let bytes = if thumb {
                let high = ((word >> 16) as u16).to_le_bytes();
                let low = (word as u16).to_le_bytes();
                [high[0], high[1], low[0], low[1]]
            } else {
                word.to_le_bytes()
            };
            assert!(matches!(
                lifter.lift_insn(0x1000, &bytes, &mut context),
                Err(LiftError::Unsupported { addr: 0x1000, .. })
            ));
        }
    }
}
