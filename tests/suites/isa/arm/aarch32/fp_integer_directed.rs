//! Directed Advanced SIMD opcode selection, pinned to LLVM 23 byte encodings.
use rax::isa::arm::aarch32::vfp::Fpscr;
use rax::isa::arm::decoder::{Aarch32Decoder, Mnemonic, ThumbDecoder};
use rax::isa::arm::execution::FlatMemory;
use rax::isa::arm::{Armv7Cpu, ExecResult, Executor};

fn check(opcode: u32, mnemonic: Mnemonic, expected: [i32; 4]) {
    for thumb in [false, true] {
        let word = if thumb { 0xffbb0042 } else { 0xf3bb0042 } | opcode << 8;
        let insn = if thumb {
            ThumbDecoder::decode_32bit(word)
        } else {
            Aarch32Decoder::decode(word)
        }
        .unwrap();
        assert_eq!(insn.mnemonic, mnemonic);
        let mut cpu = Armv7Cpu::new();
        let mut memory = FlatMemory::new(4096, 0);
        cpu.cpsr.t = thumb;
        cpu.vfp.dregs.fill(0xcafebabe_deadbeef);
        cpu.vfp.dregs[2] = 0xc0200000_40200000;
        cpu.vfp.dregs[3] = 0xc0100000_40100000;
        let initial = 0xa8000004 | (2 << 22);
        cpu.vfp.fpscr = Fpscr::from_bits(initial);
        let cpsr = cpu.cpsr.to_u32();
        let mut want = cpu.vfp.dregs;
        want[0] = u64::from(expected[0] as u32) | (u64::from(expected[1] as u32) << 32);
        want[1] = u64::from(expected[2] as u32) | (u64::from(expected[3] as u32) << 32);
        assert!(matches!(
            Executor::new(&mut cpu, &mut memory).execute(&insn),
            ExecResult::Continue
        ));
        assert_eq!(cpu.vfp.dregs, want, "Thumb={thumb}");
        assert_eq!(cpu.vfp.fpscr.bits(), initial | 0x10);
        assert_eq!(cpu.cpsr.to_u32(), cpsr);
    }
}

#[test]
fn encoding_zero_rounds_ties_away() {
    check(0, Mnemonic::VCVTA_S32_F32, [3, -3, 2, -2]);
}
#[test]
fn encoding_one_rounds_ties_even() {
    check(1, Mnemonic::VCVTN_S32_F32, [2, -2, 2, -2]);
}
#[test]
fn encoding_two_rounds_up() {
    check(2, Mnemonic::VCVTP_S32_F32, [3, -2, 3, -2]);
}
#[test]
fn encoding_three_rounds_down() {
    check(3, Mnemonic::VCVTM_S32_F32, [2, -3, 2, -3]);
}

#[test]
fn directed_conversion_uses_standard_input_flushing() {
    for thumb in [false, true] {
        let word = if thumb { 0xffbb0242 } else { 0xf3bb0242 };
        let insn = if thumb {
            ThumbDecoder::decode_32bit(word)
        } else {
            Aarch32Decoder::decode(word)
        }
        .unwrap();
        let mut cpu = Armv7Cpu::new();
        let mut memory = FlatMemory::new(4096, 0);
        cpu.cpsr.t = thumb;
        cpu.vfp.dregs[2] = 0x80000001_00000001;
        cpu.vfp.dregs[3] = 0xc0500000_40500000;
        cpu.vfp.fpscr = Fpscr::from_bits(2 << 22);
        assert!(matches!(
            Executor::new(&mut cpu, &mut memory).execute(&insn),
            ExecResult::Continue
        ));
        assert_eq!(
            (cpu.vfp.dregs[0], cpu.vfp.dregs[1]),
            (0, 0xfffffffd_00000004)
        );
        assert_eq!(cpu.vfp.fpscr.bits(), (2 << 22) | 0x90);
    }
}
