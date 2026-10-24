//! Native half/double precision conversion without an intermediate binary32 value.
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

fn convert(raw: u64, widen: bool, control: u32) -> (u64, u32) {
    let result: u64;
    let status: u64;
    macro_rules! run{($setup:literal,$op:literal,$finish:literal)=>{
  // SAFETY: compile-time FP16, explicit vector clobber and masked thread-local
  // FP state. The owning guard restores exact control/status during unwinding.
  unsafe{asm!("msr fpcr, {control}","msr fpsr, xzr",$setup,$op,$finish,"mrs {status}, fpsr",control=in(reg)u64::from(control),raw=in(reg)raw,result=lateout(reg)result,status=lateout(reg)status,out("v0")_,options(nomem,nostack));}
 }}
    if widen {
        run!("fmov h0, {raw:w}", "fcvt d0, h0", "fmov {result}, d0");
    } else {
        run!("fmov d0, {raw}", "fcvt h0, d0", "fmov {result:w}, h0");
    }
    (result, status as u32 & 0x9f)
}
fn control(c: u32) -> u32 {
    (c & 3) << 22
        | ((c >> 2) & 1) << 24
        | ((c >> 3) & 1) << 25
        | ((c >> 4) & 1) << 26
        | ((c >> 5) & 1) << 19
}
#[test]
fn half_double_precision_conversion_matches_native_exhaustive_half_and_seeded_double() {
    let _entry = HostState::read();
    let mut seed = 0x713fa9652c08de4bu64;
    let mut count = 0;
    let edges = [
        0,
        1,
        0x8000000000000000,
        0x0010000000000000,
        0x000fffffffffffff,
        0x3ff0020000000000,
        0x3ff0020000000001,
        0x3ff001ffffffffff,
        0x40effe0000000000,
        0x40f0000000000000,
        0x7ff0000000000000,
        0xfff0000000000000,
        0x7ff123456789abcd,
        0x7ff8123456789abc,
        0xfff123456789abcd,
    ];
    for widen in [false, true] {
        for index in 0..if widen { 65536 } else { 32768 } {
            let raw = if widen {
                index as u64
            } else if index < edges.len() {
                edges[index]
            } else {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed
            };
            for c in 0..64 {
                let control = control(c);
                let expected = convert(raw, widen, control);
                let mut fpscr = Fpscr::from_bits(control | 0xa8000000);
                let result = if widen {
                    vfp::vcvt_f64_f16_bits_with_fpscr(raw as u16, &mut fpscr).to_bits()
                } else {
                    u64::from(vfp::vcvt_f16_bits_f64(f64::from_bits(raw), &mut fpscr))
                };
                assert_eq!(
                    (result, fpscr.bits() & 0x9f),
                    expected,
                    "raw={raw:016x} widen={widen} control={control:08x}"
                );
                assert_eq!(fpscr.bits() & !0x9f, control | 0xa8000000);
                count += 1;
            }
        }
    }
    assert_eq!(count, 6291456);
}
