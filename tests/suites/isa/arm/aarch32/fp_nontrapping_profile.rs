//! Guest FPSCR system-register writes obey the advertised non-trapping profile.
use rax::isa::arm::aarch32::vfp::Fpscr;
use rax::isa::arm::decoder::{Aarch32Decoder, ThumbDecoder};
use rax::isa::arm::execution::FlatMemory;
use rax::isa::arm::{Armv7Cpu, ExecResult, Executor};
fn execute(cpu: &mut Armv7Cpu, raw: u32) {
    let insn = if cpu.cpsr.t {
        ThumbDecoder::decode_32bit(raw)
    } else {
        Aarch32Decoder::decode(raw)
    }
    .unwrap();
    let mut memory = FlatMemory::new(4096, 0);
    assert!(matches!(
        Executor::new(cpu, &mut memory).execute(&insn),
        ExecResult::Continue
    ));
}
#[test]
fn guest_fpscr_masks_exception_enable_and_reserved_byte_but_preserves_fz16() {
    let mut count = 0;
    for thumb in [false, true] {
        for controls in 0u32..64 {
            for enables in 0u32..256 {
                let mut cpu = Armv7Cpu::new();
                cpu.cpsr.t = thumb;
                assert_eq!((cpu.vfp.mvfr0 >> 12) & 15, 0);
                let bits = 0xa800009f
                    | (controls & 3) << 22
                    | ((controls >> 2) & 1) << 24
                    | ((controls >> 3) & 1) << 25
                    | ((controls >> 4) & 1) << 26
                    | ((controls >> 5) & 1) << 19
                    | (enables << 8);
                cpu.regs[0] = bits;
                let regs = cpu.vfp.dregs;
                let cpsr = cpu.cpsr.to_u32();
                execute(&mut cpu, 0xeee10a10);
                assert_eq!(cpu.vfp.fpscr.bits(), bits & !0xff00);
                execute(&mut cpu, 0xeef11a10);
                assert_eq!(cpu.regs[1], bits & !0xff00);
                assert_eq!(cpu.vfp.dregs, regs);
                assert_eq!(cpu.cpsr.to_u32(), cpsr);
                count += 1;
            }
        }
    }
    assert_eq!(count, 32768);
}
#[test]
fn architectural_read_masks_unsupported_bits_from_raw_restored_state() {
    for thumb in [false, true] {
        let mut cpu = Armv7Cpu::new();
        cpu.cpsr.t = thumb;
        cpu.vfp.fpscr = Fpscr::from_bits(0xffffffff);
        execute(&mut cpu, 0xeef11a10);
        assert_eq!(cpu.regs[1], 0xffff00ff);
        assert_eq!(cpu.vfp.fpscr.bits(), 0xffffffff);
    }
}
#[test]
fn raw_fpscr_helpers_keep_the_unmasked_bit_contract() {
    for bit in 0..32 {
        let raw = 1 << bit;
        assert_eq!(Fpscr::from_bits(raw).bits(), raw);
    }
    assert_eq!(Fpscr::from_bits(0xffffffff).bits(), 0xffffffff);
}

#[test]
fn attempted_enables_leave_invalid_arithmetic_cumulative_and_nontrapping() {
    for thumb in [false, true] {
        let mut cpu = Armv7Cpu::new();
        cpu.cpsr.t = thumb;
        cpu.regs[0] = 0x9f00;
        execute(&mut cpu, 0xeee10a10);
        execute(&mut cpu, 0xee810a02);
        execute(&mut cpu, 0xeef11a10);
        assert_eq!(cpu.regs[1], 1);
        assert_eq!(cpu.vfp.read_s_bits(0), 0x7fc00000);
    }
}
