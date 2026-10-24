//! Exhaustive native half-to-I16/I32 result/status oracle.
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

fn native(raw: u16, signed: bool, bits: u32, mode: usize, control: u32) -> (u32, u32) {
    let result: u32;
    let status: u64;
    macro_rules! run {($setup:literal,$op:literal,$finish:literal)=>{
        // SAFETY: native FP16 at compile time, masked thread-local FP state,
        // explicit vector clobber; outer guard restores exact entry state.
        unsafe{asm!("msr fpcr, {control}","msr fpsr, xzr",$setup,$op,$finish,"mrs {status}, fpsr",control=in(reg)u64::from(control),raw=in(reg)u32::from(raw),result=lateout(reg)result,status=lateout(reg)status,out("v0")_,options(nomem,nostack));}
    }}
    match (bits, signed) {
        (16, false) => match mode {
            0 => run!(
                "dup v0.4h, {raw:w}",
                "fcvtnu v0.4h, v0.4h",
                "umov {result:w}, v0.h[0]"
            ),
            1 => run!(
                "dup v0.4h, {raw:w}",
                "fcvtpu v0.4h, v0.4h",
                "umov {result:w}, v0.h[0]"
            ),
            2 => run!(
                "dup v0.4h, {raw:w}",
                "fcvtmu v0.4h, v0.4h",
                "umov {result:w}, v0.h[0]"
            ),
            3 => run!(
                "dup v0.4h, {raw:w}",
                "fcvtzu v0.4h, v0.4h",
                "umov {result:w}, v0.h[0]"
            ),
            4 => run!(
                "dup v0.4h, {raw:w}",
                "fcvtau v0.4h, v0.4h",
                "umov {result:w}, v0.h[0]"
            ),
            _ => unreachable!(),
        },
        (16, true) => match mode {
            0 => run!(
                "dup v0.4h, {raw:w}",
                "fcvtns v0.4h, v0.4h",
                "umov {result:w}, v0.h[0]"
            ),
            1 => run!(
                "dup v0.4h, {raw:w}",
                "fcvtps v0.4h, v0.4h",
                "umov {result:w}, v0.h[0]"
            ),
            2 => run!(
                "dup v0.4h, {raw:w}",
                "fcvtms v0.4h, v0.4h",
                "umov {result:w}, v0.h[0]"
            ),
            3 => run!(
                "dup v0.4h, {raw:w}",
                "fcvtzs v0.4h, v0.4h",
                "umov {result:w}, v0.h[0]"
            ),
            4 => run!(
                "dup v0.4h, {raw:w}",
                "fcvtas v0.4h, v0.4h",
                "umov {result:w}, v0.h[0]"
            ),
            _ => unreachable!(),
        },
        (32, false) => match mode {
            0 => run!("fmov h0, {raw:w}", "fcvtnu {result:w}, h0", ""),
            1 => run!("fmov h0, {raw:w}", "fcvtpu {result:w}, h0", ""),
            2 => run!("fmov h0, {raw:w}", "fcvtmu {result:w}, h0", ""),
            3 => run!("fmov h0, {raw:w}", "fcvtzu {result:w}, h0", ""),
            4 => run!("fmov h0, {raw:w}", "fcvtau {result:w}, h0", ""),
            _ => unreachable!(),
        },
        (32, true) => match mode {
            0 => run!("fmov h0, {raw:w}", "fcvtns {result:w}, h0", ""),
            1 => run!("fmov h0, {raw:w}", "fcvtps {result:w}, h0", ""),
            2 => run!("fmov h0, {raw:w}", "fcvtms {result:w}, h0", ""),
            3 => run!("fmov h0, {raw:w}", "fcvtzs {result:w}, h0", ""),
            4 => run!("fmov h0, {raw:w}", "fcvtas {result:w}, h0", ""),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }
    (result, status as u32 & 0x9f)
}
#[test]
fn every_half_encoding_matches_native_integer_width_rounding_and_status() {
    let _entry = HostState::read();
    let mut count = 0;
    let modes = [
        RoundingMode::RoundNearest,
        RoundingMode::RoundPlusInf,
        RoundingMode::RoundMinusInf,
        RoundingMode::RoundZero,
        RoundingMode::RoundTiesAway,
    ];
    for raw in 0..=u16::MAX {
        for c in 0..64 {
            for bits in [16, 32] {
                for signed in [false, true] {
                    for (m, &mode) in modes.iter().enumerate() {
                        let control = (c & 3) << 22
                            | ((c >> 2) & 1) << 24
                            | ((c >> 3) & 1) << 25
                            | ((c >> 4) & 1) << 26
                            | ((c >> 5) & 1) << 19;
                        let initial = control | 0xa8000000;
                        let mut f = Fpscr::from_bits(initial);
                        let value = match (bits, signed) {
                            (16, true) => {
                                u32::from(vfp::vcvt_s16_f16_bits_round(raw, mode, &mut f) as u16)
                            }
                            (16, false) => {
                                u32::from(vfp::vcvt_u16_f16_bits_round(raw, mode, &mut f))
                            }
                            (32, true) => vfp::vcvt_s32_f16_bits_round(raw, mode, &mut f) as u32,
                            (32, false) => vfp::vcvt_u32_f16_bits_round(raw, mode, &mut f),
                            _ => unreachable!(),
                        };
                        let (want, status) = native(raw, signed, bits, m, control);
                        assert_eq!(
                            (value, f.bits()),
                            (want, initial | status),
                            "raw={raw:04x} bits={bits} signed={signed} mode={mode:?} control={control:08x}"
                        );
                        count += 1;
                    }
                }
            }
        }
    }
    assert_eq!(count, 83_886_080);
}
#[test]
fn integer_oracle_restores_host_fp_state() {
    let entry = HostState::read();
    {
        let _guard = HostState::read();
        let _ = native(1, true, 16, 1, 1 << 19);
        let _ = native(0x7c01, false, 32, 0, 1 << 25);
    }
    let after = HostState::read();
    assert_eq!((entry.0, entry.1), (after.0, after.1));
}
