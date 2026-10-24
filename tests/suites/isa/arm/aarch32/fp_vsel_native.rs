//! Native FCSEL condition/raw-bit oracle with exact host flag restoration.
use super::fp_vsel_apsr::{set_flags, word};
use rax::isa::arm::aarch32::vfp::Fpscr;
use rax::isa::arm::decoder::{Aarch32Decoder, ThumbDecoder};
use rax::isa::arm::execution::FlatMemory;
use rax::isa::arm::{Armv7Cpu, ExecResult, Executor};
use std::arch::asm;
fn host_fp() -> (u64, u64) {
    let control;
    let status;
    // SAFETY: read thread-local architectural FP registers only.
    unsafe {
        asm!("mrs {control}, fpcr","mrs {status}, fpsr",control=out(reg)control,status=out(reg)status,options(nomem,nostack,preserves_flags));
    }
    (control, status)
}
fn native(fp: u32, cc: u32, n: u64, m: u64, flags: u32) -> u64 {
    let result: u64;
    let _saved: u64;
    macro_rules! run{($n:literal,$m:literal,$op:literal,$result:literal)=>{
  // SAFETY: compile-time FP16, explicit vector clobbers, no memory/stack use.
  // NZCV is saved/restored inside the block, so preserves_flags is exact.
  // FCSEL copies raw bits and does not modify FPCR or FPSR.
  unsafe{asm!("mrs {saved}, nzcv","msr nzcv, {flags}",$n,$m,$op,$result,"msr nzcv, {saved}",saved=out(reg)_saved,flags=in(reg)u64::from(flags),n=in(reg)n,m=in(reg)m,result=lateout(reg)result,out("v0")_,out("v1")_,out("v2")_,options(nomem,nostack,preserves_flags));}
 }}
    match (fp, cc) {
        (16, 0) => run!(
            "fmov h1, {n:w}",
            "fmov h2, {m:w}",
            "fcsel h0, h1, h2, eq",
            "fmov {result:w}, h0"
        ),
        (16, 1) => run!(
            "fmov h1, {n:w}",
            "fmov h2, {m:w}",
            "fcsel h0, h1, h2, vs",
            "fmov {result:w}, h0"
        ),
        (16, 2) => run!(
            "fmov h1, {n:w}",
            "fmov h2, {m:w}",
            "fcsel h0, h1, h2, ge",
            "fmov {result:w}, h0"
        ),
        (16, 3) => run!(
            "fmov h1, {n:w}",
            "fmov h2, {m:w}",
            "fcsel h0, h1, h2, gt",
            "fmov {result:w}, h0"
        ),
        (32, 0) => run!(
            "fmov s1, {n:w}",
            "fmov s2, {m:w}",
            "fcsel s0, s1, s2, eq",
            "fmov {result:w}, s0"
        ),
        (32, 1) => run!(
            "fmov s1, {n:w}",
            "fmov s2, {m:w}",
            "fcsel s0, s1, s2, vs",
            "fmov {result:w}, s0"
        ),
        (32, 2) => run!(
            "fmov s1, {n:w}",
            "fmov s2, {m:w}",
            "fcsel s0, s1, s2, ge",
            "fmov {result:w}, s0"
        ),
        (32, 3) => run!(
            "fmov s1, {n:w}",
            "fmov s2, {m:w}",
            "fcsel s0, s1, s2, gt",
            "fmov {result:w}, s0"
        ),
        (64, 0) => run!(
            "fmov d1, {n}",
            "fmov d2, {m}",
            "fcsel d0, d1, d2, eq",
            "fmov {result}, d0"
        ),
        (64, 1) => run!(
            "fmov d1, {n}",
            "fmov d2, {m}",
            "fcsel d0, d1, d2, vs",
            "fmov {result}, d0"
        ),
        (64, 2) => run!(
            "fmov d1, {n}",
            "fmov d2, {m}",
            "fcsel d0, d1, d2, ge",
            "fmov {result}, d0"
        ),
        (64, 3) => run!(
            "fmov d1, {n}",
            "fmov d2, {m}",
            "fcsel d0, d1, d2, gt",
            "fmov {result}, d0"
        ),
        _ => unreachable!(),
    }
    result
}
#[test]
fn scalar_selection_matches_native_conditions_and_preserves_raw_fp_state() {
    let before = host_fp();
    let mut memory = FlatMemory::new(4096, 0);
    let mut seed = 0x394fe62b1d05a8c7u64;
    let mut native_cases = 0;
    let mut cpu_cases = 0;
    let edges = [
        0,
        0xffffffffffffffff,
        0x8000800080008000,
        0x7c017e017f812345,
        0x7ff123456789abcd,
        0x3c003f803ff00000,
        1,
        0x0001000100010001,
    ];
    for fp in [16, 32, 64] {
        for i in 0..512 {
            let n = if i < edges.len() {
                edges[i]
            } else {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed
            };
            let m = !n;
            for flags in 0..16 {
                for cc in 0..4 {
                    let flags = flags << 28;
                    let expected = native(fp, cc, n, m, flags);
                    native_cases += 1;
                    for thumb in [false, true] {
                        let mut cpu = Armv7Cpu::new();
                        cpu.cpsr.t = thumb;
                        set_flags(&mut cpu, flags);
                        let fpscr = (flags ^ 0xf0000000) | 0x0ec8009f;
                        cpu.vfp.fpscr = Fpscr::from_bits(fpscr);
                        let (nr, mr) = if fp == 64 {
                            cpu.vfp.write_d_bits(1, n);
                            cpu.vfp.write_d_bits(2, m);
                            (1, 2)
                        } else {
                            cpu.vfp.write_s_bits(2, n as u32);
                            cpu.vfp.write_s_bits(4, m as u32);
                            (2, 4)
                        };
                        let raw = word(fp, cc, 0, nr, mr);
                        let insn = if thumb {
                            ThumbDecoder::decode_32bit(raw)
                        } else {
                            Aarch32Decoder::decode(raw)
                        }
                        .unwrap();
                        let cpsr = cpu.cpsr.to_u32();
                        assert!(matches!(
                            Executor::new(&mut cpu, &mut memory).execute(&insn),
                            ExecResult::Continue
                        ));
                        let result = if fp == 64 {
                            cpu.vfp.read_d_bits(0)
                        } else {
                            u64::from(cpu.vfp.read_s_bits(0))
                        };
                        assert_eq!(
                            result, expected,
                            "fp={fp} cc={cc} flags={flags:08x} n={n:016x} m={m:016x} Thumb={thumb}"
                        );
                        assert_eq!(cpu.vfp.fpscr.bits(), fpscr);
                        assert_eq!(cpu.cpsr.to_u32(), cpsr);
                        cpu_cases += 1;
                    }
                }
            }
        }
    }
    assert_eq!(native_cases, 98304);
    assert_eq!(cpu_cases, 196608);
    assert_eq!(host_fp(), before);
}
