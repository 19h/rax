//! Native A64 estimates and staged arithmetic oracle for A32 refinement.
//! A64 FRECPS/FRSQRTS are used only for the architectural zero*infinity
//! special case. Finite refinement uses FMUL followed by FSUB or a single
//! halved subtraction, preserving A32's intermediate product rounding.
use rax::isa::arm::aarch32::vfp::{self, Fpscr};
use std::arch::asm;
struct HostState(u64, u64);
impl HostState {
    fn read() -> Self {
        let control;
        let status;
        // SAFETY: EL0 accesses this thread's FP registers only.
        unsafe {
            asm!("mrs {control}, fpcr", "mrs {status}, fpsr", control=out(reg) control, status=out(reg) status, options(nomem,nostack));
        }
        Self(control, status)
    }
    fn set(control: u64, status: u64) {
        // SAFETY: masked controls and status belong to this thread; the guard
        // restores its exact entry state, including during panic unwinding.
        unsafe {
            asm!("msr fpcr, {control}", "msr fpsr, {status}",control=in(reg) control,status=in(reg) status,options(nomem,nostack));
        }
    }
}
impl Drop for HostState {
    fn drop(&mut self) {
        Self::set(self.0, self.1);
    }
}

fn native_estimate(raw: u32, sqrt: bool, control: u32) -> (u32, u32) {
    let result: u32;
    let status: u64;
    macro_rules! run {($op:literal)=>{
        // SAFETY: EL0 thread-local masked FP state; v0 is declared clobbered.
        unsafe {asm!("msr fpcr, {control}","msr fpsr, xzr","fmov s0, {raw:w}",$op,
            "fmov {result:w}, s0","mrs {status}, fpsr",control=in(reg)u64::from(control),
            raw=in(reg)raw,result=lateout(reg)result,status=lateout(reg)status,out("v0")_,options(nomem,nostack));}
    }}
    if sqrt {
        run!("frsqrte s0, s0");
    } else {
        run!("frecpe s0, s0");
    }
    (result, status as u32 & 0x9f)
}
fn native_step(first: u32, second: u32, sqrt: bool) -> (u32, u32) {
    let a = first & 0x7fffffff;
    let b = second & 0x7fffffff;
    let special = (a == 0x7f800000 && b < 0x00800000) || (b == 0x7f800000 && a < 0x00800000);
    let result: u32;
    let status: u64;
    macro_rules! run {($($op:literal),+)=>{
        // SAFETY: only this thread's masked FP state is touched. All vector
        // temporaries are explicit clobbers and the outer guard restores FP state.
        unsafe {asm!("msr fpcr, {control}","msr fpsr, xzr","fmov s0, {first:w}","fmov s1, {second:w}",
            $($op,)+"fmov {result:w}, s0","mrs {status}, fpsr",control=in(reg)0x03000000u64,
            first=in(reg)first,second=in(reg)second,result=lateout(reg)result,status=lateout(reg)status,
            out("v0")_,out("v1")_,out("v2")_,options(nomem,nostack));}
    }}
    match (sqrt, special) {
        (false, true) => run!("frecps s0, s0, s1"),
        (true, true) => run!("frsqrts s0, s0, s1"),
        (false, false) => run!("fmul s0, s0, s1", "fmov s2, #2.0", "fsub s0, s2, s0"),
        (true, false) => run!(
            "fmul s0, s0, s1",
            "fmov s1, #0.5",
            "fmov s2, #1.5",
            "fmsub s0, s0, s1, s2"
        ),
    }
    (result, status as u32 & 0x9f)
}
fn helper_estimate(raw: u32, sqrt: bool, control: u32) -> (u32, u32) {
    let mut fpscr = Fpscr::from_bits(control);
    let v = f32::from_bits(raw);
    let result = if sqrt {
        vfp::vrsqrte_f32(v, &mut fpscr)
    } else {
        vfp::vrecpe_f32(v, &mut fpscr)
    };
    (result.to_bits(), fpscr.bits() & 0x9f)
}
fn helper_step(first: u32, second: u32, sqrt: bool, control: u32) -> (u32, u32) {
    let mut fpscr = Fpscr::from_bits(control);
    let a = f32::from_bits(first);
    let b = f32::from_bits(second);
    let result = if sqrt {
        vfp::vrsqrts_f32(a, b, &mut fpscr)
    } else {
        vfp::vrecps_f32(a, b, &mut fpscr)
    };
    assert_eq!(fpscr.bits() & !0x9f, control & !0x9f);
    (result.to_bits(), fpscr.bits() & 0x9f)
}
fn next(seed: &mut u64) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed as u32
}
#[test]
fn estimates_match_native_result_and_cumulative_status() {
    let _entry = HostState::read();
    let mut seed = 0x31b9_8ec2_754a_d60f;
    let edges = [
        0, 0x80000000, 1, 0x80000001, 0x001fffff, 0x00200000, 0x00200001, 0x007fffff, 0x00800000,
        0x3f000000, 0x3f800000, 0x40000000, 0x7e7fffff, 0x7e800000, 0x7f000000, 0x7f7fffff,
        0xff7fffff, 0x7f800000, 0xff800000, 0x7fc12345, 0xff812345,
    ];
    let mut comparisons = 0;
    for index in 0..40000 {
        let raw = if index < edges.len() {
            edges[index]
        } else {
            next(&mut seed)
        };
        for rm in 0..4 {
            for fz in 0..2 {
                for dn in 0..2 {
                    let control = rm << 22 | fz << 24 | dn << 25;
                    for sqrt in [false, true] {
                        assert_eq!(
                            helper_estimate(raw, sqrt, control),
                            native_estimate(raw, sqrt, control),
                            "raw={raw:08x} sqrt={sqrt} control={control:08x}"
                        );
                        comparisons += 1;
                    }
                }
            }
        }
    }
    assert_eq!(comparisons, 1_280_000);
}
#[test]
fn a32_steps_match_native_staged_arithmetic_and_status() {
    let _entry = HostState::read();
    let mut seed = 0xe7a2_6d53_9c40_18bf;
    let edges = [
        (0, 0x7f800000),
        (1, 0xff800000),
        (0x7f812345, 0x3f800000),
        (0x7fc12345, 1),
        (0x3f800001, 0x3ffffffe),
        (0x3f800001, 0x403fffff),
        (0x7f7fffff, 0x40000000),
        (0x00800000, 0x00800000),
        (0x00800000, 0x3f000000),
        (0x7f800000, 0x7f800000),
        (0xff800000, 0x7f800000),
    ];
    let mut comparisons = 0;
    for index in 0..40000 {
        let (a, b) = if index < edges.len() {
            edges[index]
        } else {
            (next(&mut seed), next(&mut seed))
        };
        for rm in 0..4 {
            for sqrt in [false, true] {
                assert_eq!(
                    helper_step(a, b, sqrt, rm << 22),
                    native_step(a, b, sqrt),
                    "a={a:08x} b={b:08x} sqrt={sqrt} live_rm={rm}"
                );
                comparisons += 1;
            }
        }
    }
    assert_eq!(comparisons, 320_000);
}
#[test]
fn reciprocal_helpers_preserve_host_fp_state() {
    let _entry = HostState::read();
    HostState::set(0x03400000, 0x08000004);
    let before = HostState::read();
    for raw in [0, 1, 0x7fc12345, 0x7f812345, 0xff800000, 0x3f800001] {
        let raw = std::hint::black_box(raw);
        for sqrt in [false, true] {
            std::hint::black_box(helper_estimate(raw, sqrt, 0));
            std::hint::black_box(helper_step(raw, 0x3ffffffe, sqrt, 0));
        }
    }
    let after = HostState::read();
    assert_eq!((before.0, before.1), (after.0, after.1));
}

#[test]
fn shared_integer_quantizers_match_native_for_every_index_and_low_bit_frontier() {
    use rax::isa::arm::decoder::Aarch32Decoder;
    use rax::isa::arm::execution::FlatMemory;
    use rax::isa::arm::{Armv7Cpu, ExecResult, Executor};
    let _entry = HostState::read();
    let mut cpu = Armv7Cpu::new();
    let mut memory = FlatMemory::new(4096, 0);
    cpu.vfp.fpscr = Fpscr::from_bits(0xab80_009f);
    let mut comparisons = 0;
    for sqrt in [false, true] {
        let insn = Aarch32Decoder::decode(if sqrt { 0xf3bb0481 } else { 0xf3bb0401 }).unwrap();
        for index in 0..512u32 {
            for low in [0, 1, 0x007fffff] {
                let raw = index << 23 | low;
                let native: u32;
                macro_rules! run {
                    ($op:literal) => {
                        // SAFETY: baseline A64 SIMD integer estimate; v0 is
                        // clobbered, and the selected lane is returned as bits.
                        unsafe {
                            asm!("fmov s0, {raw:w}", $op, "umov {result:w}, v0.s[0]",
                                raw=in(reg)raw,result=lateout(reg)native,out("v0")_,options(nomem,nostack));
                        }
                    };
                }
                if sqrt {
                    run!("ursqrte v0.2s, v0.2s");
                } else {
                    run!("urecpe v0.2s, v0.2s");
                }
                cpu.vfp.dregs[1] = u64::from(raw) << 32 | u64::from(raw);
                assert!(matches!(
                    Executor::new(&mut cpu, &mut memory).execute(&insn),
                    ExecResult::Continue
                ));
                assert_eq!(
                    cpu.vfp.dregs[0],
                    u64::from(native) << 32 | u64::from(native)
                );
                assert_eq!(cpu.vfp.fpscr.bits(), 0xab80_009f);
                comparisons += 1;
            }
        }
    }
    assert_eq!(comparisons, 3072);
}
