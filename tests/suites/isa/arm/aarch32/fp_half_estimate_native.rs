//! Exhaustive actual FP16 estimate result/status oracle under live guest controls.
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

fn native(raw: u16, sqrt: bool, control: u32) -> (u16, u32) {
    let result: u32;
    let status: u64;
    macro_rules! run {($op:literal)=>{
        // SAFETY: compile-time native FP16 requirement; thread-local masked
        // control/status, explicit v0 clobber, and exact outer state restoration.
        unsafe {asm!("msr fpcr, {control}","msr fpsr, xzr","fmov h0, {raw:w}",$op,"fmov {result:w}, h0","mrs {status}, fpsr",control=in(reg)u64::from(control),raw=in(reg)u32::from(raw),result=lateout(reg)result,status=lateout(reg)status,out("v0")_,options(nomem,nostack));}
    }}
    if sqrt {
        run!("frsqrte h0, h0");
    } else {
        run!("frecpe h0, h0");
    }
    (result as u16, status as u32 & 0x9f)
}
fn control(index: u32) -> u32 {
    (index & 3) << 22
        | ((index >> 2) & 1) << 24
        | ((index >> 3) & 1) << 25
        | ((index >> 4) & 1) << 26
        | ((index >> 5) & 1) << 19
}
#[test]
fn all_half_encodings_match_native_estimates_and_status() {
    let _entry = HostState::read();
    let mut count = 0;
    for raw in 0..=u16::MAX {
        for c in 0..64 {
            for sqrt in [false, true] {
                let control = control(c);
                let initial = control | 0xa8000000;
                let mut f = Fpscr::from_bits(initial);
                let result = if sqrt {
                    vfp::vrsqrte_f16_bits(raw, &mut f)
                } else {
                    vfp::vrecpe_f16_bits(raw, &mut f)
                };
                let (bits, status) = native(raw, sqrt, control);
                assert_eq!(
                    (result, f.bits()),
                    (bits, initial | status),
                    "raw={raw:04x} sqrt={sqrt} control={control:08x}"
                );
                count += 1;
            }
        }
    }
    assert_eq!(count, 8_388_608);
}
#[test]
fn estimate_oracle_restores_host_fp_state() {
    let entry = HostState::read();
    {
        let _guard = HostState::read();
        let _ = native(1, false, 1 << 19);
        let _ = native(0xbc00, true, 1 << 25);
    }
    let after = HostState::read();
    assert_eq!((entry.0, entry.1), (after.0, after.1));
}
