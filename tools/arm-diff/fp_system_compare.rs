use rax::isa::arm::aarch32::vfp::Fpscr;
use rax::isa::arm::decoder::{Aarch32Decoder, ThumbDecoder};
use rax::isa::arm::execution::FlatMemory;
use rax::isa::arm::{Armv7Cpu, ExecResult, Executor};
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
fn main() {
    let mut args = std::env::args_os().skip(1);
    let cases = std::fs::read(args.next().expect("cases path")).expect("read cases");
    let oracle = std::fs::read(args.next().expect("oracle path")).expect("read oracle");
    assert!(args.next().is_none(), "unexpected arguments");
    assert_eq!(cases.len() % 280, 0);
    let count = cases.len() / 280;
    assert_eq!(oracle.len(), count * 272);
    assert_eq!(count % 2048, 0);
    let mut memory = FlatMemory::new(4096, 0);
    for group in 0..count / 2048 {
        let (mut bad, mut reg_bad, mut flag_bad, mut undefined) = (0, 0, 0, 0);
        for i in group * 2048..(group + 1) * 2048 {
            let b = &cases[i * 280..(i + 1) * 280];
            let q = &oracle[i * 272..(i + 1) * 272];
            let thumb = u32_at(b, 0) != 0;
            let raw = u32_at(b, 4);
            let mut cpu = Armv7Cpu::new();
            cpu.cpsr.t = thumb;
            cpu.vfp.fpscr = Fpscr::from_bits(u32_at(b, 8));
            for j in 0..32 {
                cpu.vfp.dregs[j] = u64_at(b, 12 + j * 8);
            }
            cpu.regs[0] = u32_at(b, 268);
            cpu.regs[1] = u32_at(b, 272);
            let apsr = u32_at(b, 276);
            cpu.cpsr.n = apsr & (1 << 31) != 0;
            cpu.cpsr.z = apsr & (1 << 30) != 0;
            cpu.cpsr.c = apsr & (1 << 29) != 0;
            cpu.cpsr.v = apsr & (1 << 28) != 0;
            cpu.cpsr.q = apsr & (1 << 27) != 0;
            let insn = if thumb {
                ThumbDecoder::decode_32bit(raw)
            } else {
                Aarch32Decoder::decode(raw)
            };
            let admitted = insn.is_ok_and(|insn| {
                matches!(
                    Executor::new(&mut cpu, &mut memory).execute(&insn),
                    ExecResult::Continue
                )
            });
            let registers = (0..32).any(|j| cpu.vfp.dregs[j] != u64_at(q, j * 8))
                || (0..2).any(|j| cpu.regs[j] != u32_at(q, 260 + j * 4));
            let flags = cpu.vfp.fpscr.bits() != u32_at(q, 256)
                || cpu.cpsr.to_u32() & 0xf8000000 != u32_at(q, 268);
            if !admitted {
                undefined += 1;
            }
            if registers {
                reg_bad += 1;
            }
            if flags {
                flag_bad += 1;
            }
            if !admitted || registers || flags {
                bad += 1;
                if bad <= 3 {
                    eprint!(
                        "case={i} group={group} raw={raw:08x} thumb={thumb} admitted={admitted} control={:08x} fpscr={:08x}/{:08x}",
                        u32_at(b, 8),
                        cpu.vfp.fpscr.bits(),
                        u32_at(q, 256)
                    );
                    for j in 0..32 {
                        if cpu.vfp.dregs[j] != u64_at(q, j * 8) {
                            eprint!(" d{j}={:016x}/{:016x}", cpu.vfp.dregs[j], u64_at(q, j * 8));
                        }
                    }
                    for j in 0..2 {
                        if cpu.regs[j] != u32_at(q, 260 + j * 4) {
                            eprint!(" r{j}={:08x}/{:08x}", cpu.regs[j], u32_at(q, 260 + j * 4));
                        }
                    }
                    eprint!(
                        " apsr={:08x}/{:08x}",
                        cpu.cpsr.to_u32() & 0xf8000000,
                        u32_at(q, 268)
                    );
                    eprintln!();
                }
            }
        }
        println!("{group}\t2048\t{bad}\t{reg_bad}\t{flag_bad}\t{undefined}");
    }
}
