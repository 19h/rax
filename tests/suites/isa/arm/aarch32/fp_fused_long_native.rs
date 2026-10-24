//! Real FHM result/status oracle for half-product/single-accumulator arithmetic.
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

fn native(acc: u32, first: u16, second: u16, subtract: bool, control: u32) -> (u32, u32) {
    let result: u32;
    let status: u64;
    macro_rules! run {($op:literal)=>{
  // SAFETY: compile-time native FP16/FHM requirements, explicit vector
  // clobbers, masked thread-local FP state and exact outer state restoration.
  unsafe{asm!("msr fpcr, {control}","msr fpsr, xzr","dup v0.2s, {acc:w}","dup v1.4h, {first:w}","dup v2.4h, {second:w}",$op,"umov {result:w}, v0.s[0]","mrs {status}, fpsr",control=in(reg)u64::from(control),acc=in(reg)acc,first=in(reg)u32::from(first),second=in(reg)u32::from(second),result=lateout(reg)result,status=lateout(reg)status,out("v0")_,out("v1")_,out("v2")_,options(nomem,nostack));}
 }}
    if subtract {
        run!("fmlsl v0.2s, v1.2h, v2.2h");
    } else {
        run!("fmlal v0.2s, v1.2h, v2.2h");
    }
    (result, status as u32 & 0x9f)
}
fn helper(acc: u32, a: u16, b: u16, subtract: bool, control: u32) -> (u32, u32) {
    let initial = control | 0xa8000000;
    let mut f = Fpscr::from_bits(initial);
    let bits = if subtract {
        vfp::vfmsl_f32_f16_bits(acc, a, b, &mut f)
    } else {
        vfp::vfmal_f32_f16_bits(acc, a, b, &mut f)
    };
    assert_eq!(f.bits() & !0x9f, initial & !0x9f);
    (bits, f.bits() & 0x9f)
}
fn control(index: u32) -> u32 {
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
fn mixed_fused_edges_and_seeded_triples_match_native_fhm() {
    let _entry = HostState::read();
    let mut seed = 0x5a8fc271d4e9063b;
    let mut count = 0;
    let half = [
        0, 0x8000, 1, 0x8001, 0x03ff, 0x83ff, 0x0400, 0x8400, 0x3c00, 0xbc00, 0x3c01, 0x7bff,
        0xfbff, 0x7c00, 0xfc00, 0x7c01, 0xfc01, 0x7e01, 0xfe01,
    ];
    let single = [
        0, 0x80000000, 1, 0x80000001, 0x007fffff, 0x00800000, 0x3f800000, 0xbf800000, 0x7f7fffff,
        0xff7fffff, 0x7f800000, 0xff800000, 0x7fc12345, 0xffc12345, 0x7f812345, 0xff812345,
    ];
    for index in 0..20000 {
        let (acc, a, b) = if index < 2048 {
            (
                single[index % single.len()],
                half[(index / half.len()) % half.len()],
                half[index % half.len()],
            )
        } else {
            (
                next(&mut seed),
                next(&mut seed) as u16,
                next(&mut seed) as u16,
            )
        };
        for c in 0..64 {
            for subtract in [false, true] {
                let control = control(c);
                assert_eq!(
                    helper(acc, a, b, subtract, control),
                    native(acc, a, b, subtract, control),
                    "acc={acc:08x} a={a:04x} b={b:04x} subtract={subtract} control={control:08x}"
                );
                count += 1;
            }
        }
    }
    assert_eq!(count, 2_560_000);
}
#[test]
fn every_half_nan_and_flush_encoding_obeys_mixed_nan_priority() {
    let _entry = HostState::read();
    let mut count = 0;
    for raw in 0..=u16::MAX {
        for c in 0..16 {
            for position in [false, true] {
                for subtract in [false, true] {
                    let fz16 = (c >> 2) & 1;
                    let control = (c & 3) << 22
                        | fz16 << 19
                        | ((c >> 3) & 1) << 25
                        | ((1 - fz16) << 24)
                        | (1 << 26);
                    let (a, b) = if position {
                        (raw, 0x7c00)
                    } else {
                        (0x7c00, raw)
                    };
                    assert_eq!(
                        helper(0xffc12345, a, b, subtract, control),
                        native(0xffc12345, a, b, subtract, control),
                        "raw={raw:04x} position={position} subtract={subtract} control={control:08x}"
                    );
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 4_194_304);
}
#[test]
fn mixed_oracle_and_helper_preserve_host_fp_state() {
    let entry = HostState::read();
    {
        let _guard = HostState::read();
        let _ = native(1, 1, 0x7c00, false, 1 << 19);
    }
    let after = HostState::read();
    assert_eq!((entry.0, entry.1), (after.0, after.1));
    {
        let _restore = HostState::read();
        HostState::set(3 << 22, 0x9f);
        let before = HostState::read();
        let _ = helper(0x7fc12345, 0, 0x7c00, false, 0);
        let _ = helper(1, 1, 1, true, 1 << 19);
        let after = HostState::read();
        assert_eq!((before.0, before.1), (after.0, after.1));
    }
}
