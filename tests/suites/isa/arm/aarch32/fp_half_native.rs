//! Native FP16 arithmetic oracle; compiled only when FP16 is a target feature.
//! Every comparison checks exact result bits and cumulative status.
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

fn native(op: u8, acc: u16, a: u16, b: u16, control: u32) -> (u16, u32) {
    let result: u32;
    let status: u64;
    macro_rules! run {($($instruction:literal),+)=>{
  // SAFETY: this module requires the FP16 target feature. Only thread-local
  // masked FP state changes; all vector temporaries are clobbers and HostState
  // restores the entry control/status on normal completion or unwinding.
  unsafe {asm!("msr fpcr, {control}","msr fpsr, xzr","fmov h0, {acc:w}","fmov h1, {a:w}","fmov h2, {b:w}",
   $($instruction,)+"fmov {result:w}, h0","mrs {status}, fpsr",control=in(reg)u64::from(control),
   acc=in(reg)u32::from(acc),a=in(reg)u32::from(a),b=in(reg)u32::from(b),result=lateout(reg)result,status=lateout(reg)status,
   out("v0")_,out("v1")_,out("v2")_,out("v3")_,options(nomem,nostack));}
 }}
    match op {
        0 => run!("fadd h0, h1, h2"),
        1 => run!("fsub h0, h1, h2"),
        2 => run!("fmul h0, h1, h2"),
        3 => run!("fdiv h0, h1, h2"),
        4 => run!("fminnm h0, h1, h2"),
        5 => run!("fmaxnm h0, h1, h2"),
        6 => run!("fmadd h0, h1, h2, h0"),
        7 => run!("fmsub h0, h1, h2, h0"),
        8 => run!("fnmadd h0, h1, h2, h0"),
        9 => run!("fnmsub h0, h1, h2, h0"),
        10 => run!("fmul h3, h1, h2", "fadd h0, h0, h3"),
        11 => run!("fmul h3, h1, h2", "fneg h3, h3", "fadd h0, h0, h3"),
        12 => run!(
            "fmul h3, h1, h2",
            "fneg h0, h0",
            "fneg h3, h3",
            "fadd h0, h0, h3"
        ),
        13 => run!("fmul h3, h1, h2", "fneg h0, h0", "fadd h0, h0, h3"),
        14 => run!("fmul h0, h1, h2", "fneg h0, h0"),
        15 => run!("fsqrt h0, h1"),
        _ => unreachable!(),
    }
    (result as u16, status as u32 & 0x9f)
}
fn helper(op: u8, acc: u16, a: u16, b: u16, control: u32) -> (u16, u32) {
    let mut f = Fpscr::from_bits(control);
    let bits = match op {
        0 => vfp::vadd_f16_bits(a, b, &mut f),
        1 => vfp::vsub_f16_bits(a, b, &mut f),
        2 => vfp::vmul_f16_bits(a, b, &mut f),
        3 => vfp::vdiv_f16_bits(a, b, &mut f),
        4 => vfp::vminnm_f16_bits(a, b, &mut f),
        5 => vfp::vmaxnm_f16_bits(a, b, &mut f),
        6 => vfp::vfma_f16_bits(acc, a, b, &mut f),
        7 => vfp::vfms_f16_bits(acc, a, b, &mut f),
        8 => vfp::vfnma_f16_bits(acc, a, b, &mut f),
        9 => vfp::vfnms_f16_bits(acc, a, b, &mut f),
        10 => vfp::vmla_f16_bits(acc, a, b, &mut f),
        11 => vfp::vmls_f16_bits(acc, a, b, &mut f),
        12 => vfp::vnmla_f16_bits(acc, a, b, &mut f),
        13 => vfp::vnmls_f16_bits(acc, a, b, &mut f),
        14 => vfp::vnmul_f16_bits(a, b, &mut f),
        15 => vfp::vsqrt_f16_bits(a, &mut f),
        _ => unreachable!(),
    };
    assert_eq!(f.bits() & !0x9f, control & !0x9f);
    (bits, f.bits() & 0x9f)
}
fn next(seed: &mut u64) -> u16 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed as u16
}
fn control(index: u32) -> u32 {
    (index & 3) << 22
        | ((index >> 2) & 1) << 19
        | ((index >> 3) & 1) << 25
        | ((index >> 4) & 1) << 26
        | ((!(index >> 2)) & 1) << 24
}
#[test]
fn binary_fused_and_staged_half_arithmetic_match_native() {
    let _entry = HostState::read();
    let mut seed = 0x864c_3d2f_a791_5be0;
    let edges = [
        0, 0x8000, 1, 0x8001, 0x03ff, 0x0400, 0x3c00, 0x1000, 0x7bff, 0xfbff, 0x7c00, 0xfc00,
        0x7e01, 0x7c01, 0xfe01, 0xfc01,
    ];
    let mut count = 0;
    for i in 0..20000 {
        let (acc, a, b) = if i < edges.len() * edges.len() {
            (
                edges[(i / edges.len()) % edges.len()],
                edges[i % edges.len()],
                edges[i / edges.len()],
            )
        } else {
            (next(&mut seed), next(&mut seed), next(&mut seed))
        };
        for index in 0..32 {
            let control = control(index);
            for op in 0..15 {
                assert_eq!(
                    helper(op, acc, a, b, control),
                    native(op, acc, a, b, control),
                    "op={op} acc={acc:04x} a={a:04x} b={b:04x} control={control:08x}"
                );
                count += 1;
            }
        }
    }
    assert_eq!(count, 9_600_000);
}

fn native_round(raw: u16, mode: RoundingMode, exact: bool, mut control: u32) -> (u16, u32) {
    if exact && mode == RoundingMode::RoundTiesAway {
        let (bits, _) = native_round(raw, mode, false, control);
        let (_, status) = native_round(raw, RoundingMode::RoundNearest, true, control);
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
        control = (control & !(3 << 22)) | rm << 22;
    }
    let result: u32;
    let status: u64;
    macro_rules! run {($op:literal)=>{
  // SAFETY: FP16 is a compilation requirement, masked thread-local controls,
  // declared v0 clobber; the owning test restores exact entry FP state.
  unsafe {asm!("msr fpcr, {control}","msr fpsr, xzr","fmov h0, {raw:w}",$op,"fmov {result:w}, h0","mrs {status}, fpsr",
   control=in(reg)u64::from(control),raw=in(reg)u32::from(raw),result=lateout(reg)result,status=lateout(reg)status,out("v0")_,options(nomem,nostack));}
 }}
    use RoundingMode::*;
    match (mode, exact) {
        (RoundNearest, false) => run!("frintn h0, h0"),
        (RoundPlusInf, false) => run!("frintp h0, h0"),
        (RoundMinusInf, false) => run!("frintm h0, h0"),
        (RoundZero, false) => run!("frintz h0, h0"),
        (RoundTiesAway, false) => run!("frinta h0, h0"),
        (_, true) => run!("frintx h0, h0"),
    }
    (result as u16, status as u32 & 0x9f)
}
fn native_compare(raw: u16, signal: bool, control: u32) -> (u32, u32) {
    let flags: u64;
    let status: u64;
    macro_rules! run {($op:literal)=>{
  // SAFETY: the module requires FP16. Thread-local controls are masked,
  // v0/v1 are clobbers; NZCV is an ordinary asm flags clobber.
  unsafe {asm!("msr fpcr, {control}","msr fpsr, xzr","fmov h0, {raw:w}","fmov h1, wzr",$op,"mrs {flags}, nzcv","mrs {status}, fpsr",
   control=in(reg)u64::from(control),raw=in(reg)u32::from(raw),flags=lateout(reg)flags,status=lateout(reg)status,out("v0")_,out("v1")_,options(nomem,nostack));}
 }}
    if signal {
        run!("fcmpe h0, h1");
    } else {
        run!("fcmp h0, h1");
    }
    (flags as u32 & 0xf0000000, status as u32 & 0x9f)
}
#[test]
fn every_half_encoding_matches_native_sqrt_rounding_and_compare() {
    let _entry = HostState::read();
    let mut count = 0;
    let modes = [
        RoundingMode::RoundNearest,
        RoundingMode::RoundTiesAway,
        RoundingMode::RoundPlusInf,
        RoundingMode::RoundMinusInf,
        RoundingMode::RoundZero,
    ];
    for raw in 0..=u16::MAX {
        for index in 0..16 {
            let control = control(index) | 1 << 26;
            assert_eq!(
                helper(15, 0, raw, 0, control),
                native(15, 0, raw, 0, control),
                "sqrt raw={raw:04x} control={control:08x}"
            );
            count += 1;
            for mode in modes {
                for exact in [false, true] {
                    let mut f = Fpscr::from_bits(control);
                    let bits = vfp::vrint_f16_bits(raw, mode, exact, &mut f);
                    assert_eq!(
                        (bits, f.bits() & 0x9f),
                        native_round(raw, mode, exact, control),
                        "round raw={raw:04x} mode={mode:?} exact={exact} control={control:08x}"
                    );
                    count += 1;
                }
            }
            for signal in [false, true] {
                let mut f = Fpscr::from_bits(control);
                vfp::vcmp_f16_bits_with_exception(raw, 0, signal, &mut f);
                assert_eq!(
                    (f.bits() & 0xf0000000, f.bits() & 0x9f),
                    native_compare(raw, signal, control),
                    "compare raw={raw:04x} signal={signal} control={control:08x}"
                );
                count += 1;
            }
        }
    }
    assert_eq!(count, 13_631_488);
}
#[test]
fn half_software_preserves_host_fp_control_and_status() {
    let _entry = HostState::read();
    HostState::set(0x07480000, 0x08000004);
    let before = HostState::read();
    for raw in [0, 1, 0x8001, 0x7e01, 0x7c01, 0x7c00, 0x7bff] {
        let raw = std::hint::black_box(raw);
        for op in 0..16 {
            std::hint::black_box(helper(op, 0xbc00, raw, 0x3bfe, 0));
        }
        let mut f = Fpscr::default();
        std::hint::black_box(vfp::vrint_f16_bits(
            raw,
            RoundingMode::RoundMinusInf,
            true,
            &mut f,
        ));
        vfp::vcmp_f16_bits(raw, 0, &mut f);
    }
    let after = HostState::read();
    assert_eq!((before.0, before.1), (after.0, after.1));
}
