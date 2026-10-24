//! Complete local profile check and explicitly bounded QEMU register comparison.
use rax::isa::arm::decoder::{Aarch32Decoder, ThumbDecoder};
use rax::isa::arm::execution::FlatMemory;
use rax::isa::arm::{Armv7Cpu, ExecResult, Executor};

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn execute(cpu: &mut Armv7Cpu, memory: &mut FlatMemory, raw: u32) {
    let insn = if cpu.cpsr.t {
        ThumbDecoder::decode_32bit(raw)
    } else {
        Aarch32Decoder::decode(raw)
    }
    .unwrap();
    assert!(matches!(
        Executor::new(cpu, memory).execute(&insn),
        ExecResult::Continue
    ));
}
fn main() {
    let mut args = std::env::args_os().skip(1);
    let input = std::fs::read(args.next().expect("input path")).unwrap();
    let reference = std::fs::read(args.next().expect("reference path")).unwrap();
    assert!(args.next().is_none());
    assert_eq!(input.len(), 4164 * 4);
    assert_eq!(reference.len(), 16 + 2 * 4164 * 12);
    let mut memory = FlatMemory::new(4096, 0);
    let mut capability_failures = 0;
    for thumb in [false, true] {
        let mut cpu = Armv7Cpu::new();
        cpu.cpsr.t = thumb;
        for (raw, offset, want) in [
            (0xeef70a10, 0, 0x10110222),
            (0xeef60a10, 4, 0x13211111),
            (0xeef50a10, 8, 0x43),
        ] {
            assert_eq!(word(&reference, offset), want);
            execute(&mut cpu, &mut memory, raw);
            if cpu.regs[0] != want {
                capability_failures += 1;
            }
        }
        assert_eq!((word(&reference, 12) >> 8) & 15, 1);
        execute(&mut cpu, &mut memory, 0xee100ff2);
        if cpu.regs[0] != 0x100 {
            capability_failures += 1;
        }
    }
    let mut local_failures = 0;
    let mut common_failures = 0;
    let mut raw_profile_differences = 0;
    for state in 0..2 {
        for i in 0..4164 {
            let value = word(&input, i * 4);
            let offset = 16 + (state * 4164 + i) * 12;
            assert_eq!(word(&reference, offset), value);
            let qemu_fpscr = word(&reference, offset + 4);
            let qemu_fpexc = word(&reference, offset + 8);
            let fpscr = value & 0xffc8_009f;
            let fpexc = (value & 0x4000_0000) | 0x700;
            // QEMU11.1.2 retains Len/Stride despite MVFR0.ShortVec=0,
            // and returns zero for FPEXC.RES1 bits10:8. Preserve raw evidence;
            // project only these documented differences out of comparison.
            if qemu_fpscr != fpscr || qemu_fpexc != fpexc {
                raw_profile_differences += 1;
            }
            if qemu_fpscr & !0x0037_0000 != fpscr || qemu_fpexc | 0x700 != fpexc {
                common_failures += 1;
            }
            let mut cpu = Armv7Cpu::new();
            cpu.cpsr.t = state != 0;
            cpu.regs[0] = value;
            execute(&mut cpu, &mut memory, 0xeee1_0a10);
            execute(&mut cpu, &mut memory, 0xeef1_1a10);
            let actual_fpscr = cpu.regs[1];
            let stored_fpscr = cpu.vfp.fpscr.bits();
            execute(&mut cpu, &mut memory, 0xeee8_0a10);
            execute(&mut cpu, &mut memory, 0xeef8_2a10);
            if actual_fpscr != fpscr
                || stored_fpscr != fpscr
                || cpu.regs[2] != fpexc
                || cpu.vfp.fpexc != fpexc
            {
                local_failures += 1;
                if local_failures <= 4 {
                    eprintln!(
                        "state={state} value={value:08x} FPSCR={actual_fpscr:08x}/{fpscr:08x} FPEXC={:08x}/{fpexc:08x}",
                        cpu.regs[2]
                    );
                }
            }
        }
    }
    println!(
        "{{\"cases\":8328,\"capability_failures\":{capability_failures},\"local_profile_failures\":{local_failures},\"independent_common_field_failures\":{common_failures},\"qemu_raw_profile_differences\":{raw_profile_differences}}}"
    );
    if capability_failures != 0 || local_failures != 0 || common_failures != 0 {
        std::process::exit(1);
    }
}
