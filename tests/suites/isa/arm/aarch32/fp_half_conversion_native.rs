//! Actual FP16 conversion and A32 staged-refinement numerical/status oracle.
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

fn convert(raw: u32, widen: bool, control: u32) -> (u32, u32) {
    let result: u32;
    let status: u64;
    macro_rules! run {($setup:literal,$convert:literal,$finish:literal)=>{
  // SAFETY: this module requires native FP16. Masked thread-local controls,
  // explicit v0 clobber, and the owning guard restores exact entry FP state.
  unsafe{asm!("msr fpcr, {control}","msr fpsr, xzr",$setup,$convert,$finish,"mrs {status}, fpsr",
   control=in(reg)u64::from(control),raw=in(reg)raw,result=lateout(reg)result,status=lateout(reg)status,out("v0")_,options(nomem,nostack));}
 }}
    if widen {
        run!("fmov h0, {raw:w}", "fcvt s0, h0", "fmov {result:w}, s0");
    } else {
        run!("fmov s0, {raw:w}", "fcvt h0, s0", "fmov {result:w}, h0");
    }
    (result, status as u32 & 0x9f)
}
fn helper_convert(raw: u32, widen: bool, control: u32) -> (u32, u32) {
    let mut f = Fpscr::from_bits(control);
    let bits = if widen {
        vfp::vcvt_f32_f16_bits_with_fpscr(raw as u16, &mut f).to_bits()
    } else {
        u32::from(vfp::vcvt_f16_bits_f32(f32::from_bits(raw), &mut f))
    };
    assert_eq!(f.bits() & !0x9f, control & !0x9f);
    (bits, f.bits() & 0x9f)
}
fn controls(index: u32) -> u32 {
    (index & 3) << 22
        | ((index >> 2) & 1) << 24
        | ((index >> 3) & 1) << 25
        | ((index >> 4) & 1) << 26
        | ((index >> 5) & 1) << 19
}
fn next(seed: &mut u64) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed as u32
}
#[test]
fn ieee_and_alternative_precision_conversion_match_native_bits_and_status() {
    let _entry = HostState::read();
    let mut seed = 0x9e37_d4a6_251b_c8f0;
    let mut count = 0;
    let edges = [
        0, 0x80000000, 1, 0x80000001, 0x007fffff, 0x00800000, 0x33800000, 0x33000000, 0x3f801000,
        0x477fe000, 0x47800000, 0x47ffe000, 0x47fff000, 0x48000000, 0x7f7fffff, 0xff7fffff,
        0x7f800000, 0xff800000, 0x7fc12345, 0xffc12345, 0x7f812345, 0xff812345,
    ];
    for widen in [false, true] {
        let size = if widen { 65536 } else { 20000 };
        for index in 0..size {
            let raw = if widen {
                index as u32
            } else if index < edges.len() {
                edges[index]
            } else {
                next(&mut seed)
            };
            for c in 0..64 {
                let control = controls(c);
                assert_eq!(
                    helper_convert(raw, widen, control),
                    convert(raw, widen, control),
                    "raw={raw:08x} widen={widen} control={control:08x}"
                );
                count += 1;
            }
        }
    }
    assert_eq!(count, 5_474_304);
    // The compatibility helper is an IEEE encoding map; it deliberately retains
    // signaling encodings and does not process FPSCR. Undo only hardware quieting.
    for raw in 0..=u16::MAX {
        let (mut expected, _) = convert(u32::from(raw), true, 0);
        if raw & 0x7c00 == 0x7c00 && raw & 0x03ff != 0 && raw & 0x0200 == 0 {
            expected &= !0x00400000;
        }
        assert_eq!(vfp::vcvt_f32_f16_bits(raw).to_bits(), expected);
    }
}
fn native_integer(raw: u32, signed: bool, control: u32) -> (u16, u32) {
    let result: u32;
    let status: u64;
    macro_rules! run {($op:literal)=>{
  // SAFETY: native FP16 target, explicit v0 clobber, masked thread-local FP
  // controls; the entry guard restores FPCR/FPSR after the matrix.
  unsafe{asm!("msr fpcr, {control}","msr fpsr, xzr",$op,"fmov {result:w}, h0","mrs {status}, fpsr",
   control=in(reg)u64::from(control),raw=in(reg)raw,result=lateout(reg)result,status=lateout(reg)status,out("v0")_,options(nomem,nostack));}
 }}
    if signed {
        run!("scvtf h0, {raw:w}");
    } else {
        run!("ucvtf h0, {raw:w}");
    }
    (result as u16, status as u32 & 0x9f)
}
#[test]
fn scalar_integer_to_half_uses_data_profile_including_with_ahp_set() {
    let _entry = HostState::read();
    let mut seed = 0xc6ef_e92f_5138_a4d7;
    let mut count = 0;
    let edges = [
        0,
        1,
        u32::MAX,
        0x80000000,
        0x7fffffff,
        65504,
        65519,
        65520,
        65535,
        65536,
        16777217,
    ];
    for i in 0..20000 {
        let raw = if i < edges.len() {
            edges[i]
        } else {
            next(&mut seed)
        };
        for c in 0..64 {
            let control = controls(c);
            for signed in [false, true] {
                let mut f = Fpscr::from_bits(control);
                let result = if signed {
                    vfp::vcvt_f16_bits_s32(raw as i32, &mut f)
                } else {
                    vfp::vcvt_f16_bits_u32(raw, &mut f)
                };
                assert_eq!(
                    (result, f.bits() & 0x9f),
                    native_integer(raw, signed, control),
                    "raw={raw:08x} signed={signed} control={control:08x}"
                );
                assert_eq!(f.bits() & !0x9f, control & !0x9f);
                count += 1;
            }
        }
    }
    assert_eq!(count, 2_560_000);
}
fn native_step(first: u16, second: u16, sqrt: bool, flush: bool) -> (u16, u32) {
    let a = first & 0x7fff;
    let b = second & 0x7fff;
    let special = (a == 0x7c00 && (b == 0 || (flush && b < 0x0400)))
        || (b == 0x7c00 && (a == 0 || (flush && a < 0x0400)));
    let control = (7u64 << 24) | (u64::from(flush) << 19);
    let result: u32;
    let status: u64;
    macro_rules! run {($($op:literal),+)=>{
  // SAFETY: FP16 is a target requirement. Only masked thread-local FP state
  // and declared v0-v2 temporaries change; exact entry state is restored.
  unsafe{asm!("msr fpcr, {control}","msr fpsr, xzr","fmov h0, {first:w}","fmov h1, {second:w}",$($op,)+
   "fmov {result:w}, h0","mrs {status}, fpsr",control=in(reg)control,first=in(reg)u32::from(first),second=in(reg)u32::from(second),
   result=lateout(reg)result,status=lateout(reg)status,out("v0")_,out("v1")_,out("v2")_,options(nomem,nostack));}
 }}
    match (sqrt, special) {
        (false, true) => run!("frecps h0, h0, h1"),
        (true, true) => run!("frsqrts h0, h0, h1"),
        (false, false) => run!("fmul h0, h0, h1", "fmov h2, #2.0", "fsub h0, h2, h0"),
        (true, false) => run!(
            "fmul h0, h0, h1",
            "fmov h1, #0.5",
            "fmov h2, #1.5",
            "fmsub h0, h0, h1, h2"
        ),
    }
    (result as u16, status as u32 & 0x9f)
}
#[test]
fn half_refinement_matches_native_staged_product_and_subtraction() {
    let _entry = HostState::read();
    let mut seed = 0xa528_1e6c_7fd3_490b;
    let mut count = 0;
    let edges = [
        (0, 0x7c00),
        (1, 0x7c00),
        (0x7e01, 0x3c00),
        (0x7c01, 0x3c00),
        (0x3c01, 0x3ffe),
        (0x3c01, 0x41ff),
        (0x7bff, 0x4000),
        (0x0400, 0x3800),
        (0x0400, 0x0400),
        (0xfc00, 0),
    ];
    for i in 0..40000 {
        let (a, b) = if i < edges.len() {
            edges[i]
        } else {
            (next(&mut seed) as u16, next(&mut seed) as u16)
        };
        for rm in 0..4 {
            for flush in [false, true] {
                for sqrt in [false, true] {
                    let control = (rm << 22) | (u32::from(flush) << 19) | (1 << 26);
                    let mut f = Fpscr::from_bits(control);
                    let bits = if sqrt {
                        vfp::vrsqrts_f16_bits(a, b, &mut f)
                    } else {
                        vfp::vrecps_f16_bits(a, b, &mut f)
                    };
                    assert_eq!(
                        (bits, f.bits() & 0x9f),
                        native_step(a, b, sqrt, flush),
                        "a={a:04x} b={b:04x} sqrt={sqrt} control={control:08x}"
                    );
                    assert_eq!(f.bits() & !0x9f, control & !0x9f);
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 640_000);
}
#[test]
fn conversion_and_refinement_software_preserve_host_fp_control_and_status() {
    let _entry = HostState::read();
    HostState::set(0x07480000, 0x08000004);
    let before = HostState::read();
    for raw in [
        0, 1, 0x80000001, 0x33800000, 0x7fc12345, 0x7f812345, 0x7f800000,
    ] {
        let raw = std::hint::black_box(raw);
        std::hint::black_box(helper_convert(raw, false, 0));
        std::hint::black_box(helper_convert(raw & 0xffff, true, 0));
        std::hint::black_box(vfp::vcvt_f32_f16_bits((raw >> 16) as u16));
        let mut f = Fpscr::default();
        std::hint::black_box(vfp::vcvt_f16_bits_s32(raw as i32, &mut f));
        std::hint::black_box(vfp::vrecps_f16_bits((raw >> 16) as u16, raw as u16, &mut f));
    }
    let after = HostState::read();
    assert_eq!((before.0, before.1), (after.0, after.1));
}
