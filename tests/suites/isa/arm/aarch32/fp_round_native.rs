//! A64 FRINT numerical/status oracle for the shared FPRoundInt operation.
//! Ties-away with exact=true combines FRINTA bits with FRINTX status: the
//! inexact predicate depends on the input's fractional part, not rounding mode.
use rax::isa::arm::aarch32::vfp::{self, Fpscr, RoundingMode};
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

fn native(raw: u64, double: bool, mode: RoundingMode, exact: bool, mut control: u32) -> (u64, u32) {
    if exact && mode == RoundingMode::RoundTiesAway {
        let (bits, _) = native(raw, double, mode, false, control);
        let (_, status) = native(raw, double, RoundingMode::RoundNearest, true, control);
        return (bits, status);
    }
    if exact {
        let rm = match mode {
            RoundingMode::RoundNearest => 0,
            RoundingMode::RoundPlusInf => 1,
            RoundingMode::RoundMinusInf => 2,
            RoundingMode::RoundZero => 3,
            _ => unreachable!(),
        };
        control = (control & !(3 << 22)) | (rm << 22);
    }
    let result: u64;
    let status: u64;
    macro_rules! run {($setup:literal,$instruction:literal,$finish:literal)=>{
        // SAFETY: only this thread's masked FP state changes; v0 is clobbered.
        unsafe {asm!("msr fpcr, {control}","msr fpsr, xzr",$setup,$instruction,$finish,"mrs {status}, fpsr",control=in(reg)u64::from(control),raw=in(reg)raw,result=lateout(reg)result,status=lateout(reg)status,out("v0")_,options(nomem,nostack));}
    }}
    use RoundingMode::*;
    match (double, mode, exact) {
        (false, RoundNearest, false) => {
            run!("fmov s0, {raw:w}", "frintn s0, s0", "fmov {result:w}, s0")
        }
        (false, RoundTiesAway, false) => {
            run!("fmov s0, {raw:w}", "frinta s0, s0", "fmov {result:w}, s0")
        }
        (false, RoundPlusInf, false) => {
            run!("fmov s0, {raw:w}", "frintp s0, s0", "fmov {result:w}, s0")
        }
        (false, RoundMinusInf, false) => {
            run!("fmov s0, {raw:w}", "frintm s0, s0", "fmov {result:w}, s0")
        }
        (false, RoundZero, false) => {
            run!("fmov s0, {raw:w}", "frintz s0, s0", "fmov {result:w}, s0")
        }
        (false, _, true) => run!("fmov s0, {raw:w}", "frintx s0, s0", "fmov {result:w}, s0"),
        (true, RoundNearest, false) => run!("fmov d0, {raw}", "frintn d0, d0", "fmov {result}, d0"),
        (true, RoundTiesAway, false) => {
            run!("fmov d0, {raw}", "frinta d0, d0", "fmov {result}, d0")
        }
        (true, RoundPlusInf, false) => run!("fmov d0, {raw}", "frintp d0, d0", "fmov {result}, d0"),
        (true, RoundMinusInf, false) => {
            run!("fmov d0, {raw}", "frintm d0, d0", "fmov {result}, d0")
        }
        (true, RoundZero, false) => run!("fmov d0, {raw}", "frintz d0, d0", "fmov {result}, d0"),
        (true, _, true) => run!("fmov d0, {raw}", "frintx d0, d0", "fmov {result}, d0"),
    }
    (result, status as u32 & 0x9f)
}
fn helper(raw: u64, double: bool, mode: RoundingMode, exact: bool, control: u32) -> (u64, u32) {
    let mut fpscr = Fpscr::from_bits(control);
    let result = if double {
        vfp::vrint_f64(f64::from_bits(raw), mode, exact, &mut fpscr).to_bits()
    } else {
        u64::from(vfp::vrint_f32(f32::from_bits(raw as u32), mode, exact, &mut fpscr).to_bits())
    };
    (result, fpscr.bits() & 0x9f)
}
fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}
#[test]
fn integral_rounding_matches_native_frint_for_bits_and_status() {
    let _entry = HostState::read();
    let mut seed = 0xd491_7c5b_2a68_f30e;
    let mut comparisons = 0;
    let modes = [
        RoundingMode::RoundNearest,
        RoundingMode::RoundTiesAway,
        RoundingMode::RoundPlusInf,
        RoundingMode::RoundMinusInf,
        RoundingMode::RoundZero,
    ];
    for double in [false, true] {
        let edges: &[u64] = if double {
            &[
                0,
                1,
                0x8000000000000000,
                0x8000000000000001,
                0x000fffffffffffff,
                0x0010000000000000,
                0x3fe0000000000000,
                0xbfe0000000000000,
                0x4004000000000000,
                0xc004000000000000,
                0x432fffffffffffff,
                0xc32fffffffffffff,
                0x4330000000000000,
                0x7fefffffffffffff,
                0x7ff0000000000000,
                0xfff0000000000000,
                0x7ff8000000001234,
                0xfff0000000001234,
            ]
        } else {
            &[
                0, 1, 0x80000000, 0x80000001, 0x007fffff, 0x00800000, 0x3f000000, 0xbf000000,
                0x40200000, 0xc0200000, 0x4affffff, 0xcaffffff, 0x4b000000, 0x7f7fffff, 0x7f800000,
                0xff800000, 0x7fc01234, 0xff801234,
            ]
        };
        for index in 0..20000 {
            let raw = if index < edges.len() {
                edges[index]
            } else {
                let raw = next(&mut seed);
                if double { raw } else { u64::from(raw as u32) }
            };
            for rm in 0..4 {
                for flush in 0..2 {
                    for dn in 0..2 {
                        let control = (rm << 22) | (flush << 24) | (dn << 25);
                        for mode in modes {
                            for exact in [false, true] {
                                assert_eq!(
                                    helper(raw, double, mode, exact, control),
                                    native(raw, double, mode, exact, control),
                                    "raw={raw:016x} double={double} mode={mode:?} exact={exact} control={control:x}"
                                );
                                comparisons += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(comparisons, 6_400_000);
}
#[test]
fn integral_rounding_preserves_host_fp_control_and_status() {
    let _entry = HostState::read();
    HostState::set((1 << 22) | (1 << 24) | (1 << 25), 0x08000004);
    let before = HostState::read();
    for raw in [
        0,
        1,
        0x80000001,
        0x7fc01234,
        0x7f801234,
        0x432fffffffffffff,
        0xfff0000000001234,
    ] {
        let raw = std::hint::black_box(raw);
        for double in [false, true] {
            for mode in [RoundingMode::RoundZero, RoundingMode::RoundTiesAway] {
                for exact in [false, true] {
                    std::hint::black_box(helper(raw, double, mode, exact, 0));
                }
            }
        }
    }
    let after = HostState::read();
    assert_eq!((after.0, after.1), (before.0, before.1));
}
