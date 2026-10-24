//! A64 arithmetic oracle for common A32 integer conversion operations.
//! A32 register layout and fixed-point nearest rounding need separate decode tests.
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

fn native_to_integer(
    raw: u64,
    double: bool,
    signed: bool,
    fbits: u32,
    mode: RoundingMode,
    control: u32,
) -> (u32, u32) {
    let result: u32;
    let status: u64;
    macro_rules! run {($setup:literal,$instruction:literal) => {
        // SAFETY: thread-local FP state, traps disabled, all clobbers declared.
        unsafe {asm!("msr fpcr, {control}","msr fpsr, xzr",$setup,$instruction,"mrs {status}, fpsr",control=in(reg) u64::from(control),raw=in(reg)raw,result=lateout(reg)result,status=lateout(reg)status,out("v0")_,options(nomem,nostack));}
    }}
    use RoundingMode::*;
    match (double, signed, fbits, mode) {
        (false, false, 0, RoundNearest) => run!("fmov s0, {raw:w}", "fcvtnu {result:w}, s0"),
        (false, false, 0, RoundPlusInf) => run!("fmov s0, {raw:w}", "fcvtpu {result:w}, s0"),
        (false, false, 0, RoundMinusInf) => run!("fmov s0, {raw:w}", "fcvtmu {result:w}, s0"),
        (false, false, 0, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0"),
        (false, false, 0, RoundTiesAway) => run!("fmov s0, {raw:w}", "fcvtau {result:w}, s0"),
        (false, false, 1, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #1"),
        (false, false, 2, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #2"),
        (false, false, 3, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #3"),
        (false, false, 4, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #4"),
        (false, false, 5, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #5"),
        (false, false, 6, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #6"),
        (false, false, 7, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #7"),
        (false, false, 8, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #8"),
        (false, false, 9, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #9"),
        (false, false, 10, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #10"),
        (false, false, 11, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #11"),
        (false, false, 12, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #12"),
        (false, false, 13, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #13"),
        (false, false, 14, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #14"),
        (false, false, 15, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #15"),
        (false, false, 16, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #16"),
        (false, false, 17, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #17"),
        (false, false, 18, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #18"),
        (false, false, 19, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #19"),
        (false, false, 20, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #20"),
        (false, false, 21, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #21"),
        (false, false, 22, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #22"),
        (false, false, 23, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #23"),
        (false, false, 24, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #24"),
        (false, false, 25, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #25"),
        (false, false, 26, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #26"),
        (false, false, 27, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #27"),
        (false, false, 28, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #28"),
        (false, false, 29, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #29"),
        (false, false, 30, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #30"),
        (false, false, 31, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #31"),
        (false, false, 32, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzu {result:w}, s0, #32"),
        (false, true, 0, RoundNearest) => run!("fmov s0, {raw:w}", "fcvtns {result:w}, s0"),
        (false, true, 0, RoundPlusInf) => run!("fmov s0, {raw:w}", "fcvtps {result:w}, s0"),
        (false, true, 0, RoundMinusInf) => run!("fmov s0, {raw:w}", "fcvtms {result:w}, s0"),
        (false, true, 0, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0"),
        (false, true, 0, RoundTiesAway) => run!("fmov s0, {raw:w}", "fcvtas {result:w}, s0"),
        (false, true, 1, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #1"),
        (false, true, 2, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #2"),
        (false, true, 3, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #3"),
        (false, true, 4, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #4"),
        (false, true, 5, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #5"),
        (false, true, 6, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #6"),
        (false, true, 7, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #7"),
        (false, true, 8, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #8"),
        (false, true, 9, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #9"),
        (false, true, 10, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #10"),
        (false, true, 11, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #11"),
        (false, true, 12, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #12"),
        (false, true, 13, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #13"),
        (false, true, 14, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #14"),
        (false, true, 15, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #15"),
        (false, true, 16, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #16"),
        (false, true, 17, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #17"),
        (false, true, 18, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #18"),
        (false, true, 19, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #19"),
        (false, true, 20, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #20"),
        (false, true, 21, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #21"),
        (false, true, 22, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #22"),
        (false, true, 23, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #23"),
        (false, true, 24, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #24"),
        (false, true, 25, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #25"),
        (false, true, 26, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #26"),
        (false, true, 27, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #27"),
        (false, true, 28, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #28"),
        (false, true, 29, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #29"),
        (false, true, 30, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #30"),
        (false, true, 31, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #31"),
        (false, true, 32, RoundZero) => run!("fmov s0, {raw:w}", "fcvtzs {result:w}, s0, #32"),
        (true, false, 0, RoundNearest) => run!("fmov d0, {raw}", "fcvtnu {result:w}, d0"),
        (true, false, 0, RoundPlusInf) => run!("fmov d0, {raw}", "fcvtpu {result:w}, d0"),
        (true, false, 0, RoundMinusInf) => run!("fmov d0, {raw}", "fcvtmu {result:w}, d0"),
        (true, false, 0, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0"),
        (true, false, 0, RoundTiesAway) => run!("fmov d0, {raw}", "fcvtau {result:w}, d0"),
        (true, false, 1, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #1"),
        (true, false, 2, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #2"),
        (true, false, 3, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #3"),
        (true, false, 4, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #4"),
        (true, false, 5, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #5"),
        (true, false, 6, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #6"),
        (true, false, 7, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #7"),
        (true, false, 8, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #8"),
        (true, false, 9, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #9"),
        (true, false, 10, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #10"),
        (true, false, 11, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #11"),
        (true, false, 12, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #12"),
        (true, false, 13, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #13"),
        (true, false, 14, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #14"),
        (true, false, 15, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #15"),
        (true, false, 16, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #16"),
        (true, false, 17, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #17"),
        (true, false, 18, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #18"),
        (true, false, 19, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #19"),
        (true, false, 20, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #20"),
        (true, false, 21, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #21"),
        (true, false, 22, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #22"),
        (true, false, 23, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #23"),
        (true, false, 24, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #24"),
        (true, false, 25, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #25"),
        (true, false, 26, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #26"),
        (true, false, 27, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #27"),
        (true, false, 28, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #28"),
        (true, false, 29, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #29"),
        (true, false, 30, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #30"),
        (true, false, 31, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #31"),
        (true, false, 32, RoundZero) => run!("fmov d0, {raw}", "fcvtzu {result:w}, d0, #32"),
        (true, true, 0, RoundNearest) => run!("fmov d0, {raw}", "fcvtns {result:w}, d0"),
        (true, true, 0, RoundPlusInf) => run!("fmov d0, {raw}", "fcvtps {result:w}, d0"),
        (true, true, 0, RoundMinusInf) => run!("fmov d0, {raw}", "fcvtms {result:w}, d0"),
        (true, true, 0, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0"),
        (true, true, 0, RoundTiesAway) => run!("fmov d0, {raw}", "fcvtas {result:w}, d0"),
        (true, true, 1, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #1"),
        (true, true, 2, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #2"),
        (true, true, 3, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #3"),
        (true, true, 4, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #4"),
        (true, true, 5, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #5"),
        (true, true, 6, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #6"),
        (true, true, 7, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #7"),
        (true, true, 8, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #8"),
        (true, true, 9, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #9"),
        (true, true, 10, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #10"),
        (true, true, 11, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #11"),
        (true, true, 12, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #12"),
        (true, true, 13, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #13"),
        (true, true, 14, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #14"),
        (true, true, 15, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #15"),
        (true, true, 16, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #16"),
        (true, true, 17, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #17"),
        (true, true, 18, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #18"),
        (true, true, 19, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #19"),
        (true, true, 20, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #20"),
        (true, true, 21, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #21"),
        (true, true, 22, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #22"),
        (true, true, 23, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #23"),
        (true, true, 24, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #24"),
        (true, true, 25, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #25"),
        (true, true, 26, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #26"),
        (true, true, 27, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #27"),
        (true, true, 28, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #28"),
        (true, true, 29, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #29"),
        (true, true, 30, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #30"),
        (true, true, 31, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #31"),
        (true, true, 32, RoundZero) => run!("fmov d0, {raw}", "fcvtzs {result:w}, d0, #32"),
        _ => unreachable!("bounded native conversion profile"),
    }
    (result, status as u32 & 0x9f)
}

fn native_from_integer(
    raw: u32,
    double: bool,
    signed: bool,
    fbits: u32,
    control: u32,
) -> (u64, u32) {
    let result: u64;
    let status: u64;
    // A32 fixed-point conversion specifies nearest, independently of FPSCR RM.
    let control = if fbits == 0 {
        control
    } else {
        control & !(3 << 22)
    };
    macro_rules! run {($instruction:literal,$finish:literal) => {
        // SAFETY: integer bits cross the FP boundary only through this opcode.
        unsafe {asm!("msr fpcr, {control}","msr fpsr, xzr",$instruction,$finish,"mrs {status}, fpsr",control=in(reg)u64::from(control),raw=in(reg)raw,result=lateout(reg)result,status=lateout(reg)status,out("v0")_,options(nomem,nostack));}
    }}
    match (double, signed, fbits) {
        (false, false, 0) => run!("ucvtf s0, {raw:w}", "fmov {result:w}, s0"),
        (false, false, 1) => run!("ucvtf s0, {raw:w}, #1", "fmov {result:w}, s0"),
        (false, false, 2) => run!("ucvtf s0, {raw:w}, #2", "fmov {result:w}, s0"),
        (false, false, 3) => run!("ucvtf s0, {raw:w}, #3", "fmov {result:w}, s0"),
        (false, false, 4) => run!("ucvtf s0, {raw:w}, #4", "fmov {result:w}, s0"),
        (false, false, 5) => run!("ucvtf s0, {raw:w}, #5", "fmov {result:w}, s0"),
        (false, false, 6) => run!("ucvtf s0, {raw:w}, #6", "fmov {result:w}, s0"),
        (false, false, 7) => run!("ucvtf s0, {raw:w}, #7", "fmov {result:w}, s0"),
        (false, false, 8) => run!("ucvtf s0, {raw:w}, #8", "fmov {result:w}, s0"),
        (false, false, 9) => run!("ucvtf s0, {raw:w}, #9", "fmov {result:w}, s0"),
        (false, false, 10) => run!("ucvtf s0, {raw:w}, #10", "fmov {result:w}, s0"),
        (false, false, 11) => run!("ucvtf s0, {raw:w}, #11", "fmov {result:w}, s0"),
        (false, false, 12) => run!("ucvtf s0, {raw:w}, #12", "fmov {result:w}, s0"),
        (false, false, 13) => run!("ucvtf s0, {raw:w}, #13", "fmov {result:w}, s0"),
        (false, false, 14) => run!("ucvtf s0, {raw:w}, #14", "fmov {result:w}, s0"),
        (false, false, 15) => run!("ucvtf s0, {raw:w}, #15", "fmov {result:w}, s0"),
        (false, false, 16) => run!("ucvtf s0, {raw:w}, #16", "fmov {result:w}, s0"),
        (false, false, 17) => run!("ucvtf s0, {raw:w}, #17", "fmov {result:w}, s0"),
        (false, false, 18) => run!("ucvtf s0, {raw:w}, #18", "fmov {result:w}, s0"),
        (false, false, 19) => run!("ucvtf s0, {raw:w}, #19", "fmov {result:w}, s0"),
        (false, false, 20) => run!("ucvtf s0, {raw:w}, #20", "fmov {result:w}, s0"),
        (false, false, 21) => run!("ucvtf s0, {raw:w}, #21", "fmov {result:w}, s0"),
        (false, false, 22) => run!("ucvtf s0, {raw:w}, #22", "fmov {result:w}, s0"),
        (false, false, 23) => run!("ucvtf s0, {raw:w}, #23", "fmov {result:w}, s0"),
        (false, false, 24) => run!("ucvtf s0, {raw:w}, #24", "fmov {result:w}, s0"),
        (false, false, 25) => run!("ucvtf s0, {raw:w}, #25", "fmov {result:w}, s0"),
        (false, false, 26) => run!("ucvtf s0, {raw:w}, #26", "fmov {result:w}, s0"),
        (false, false, 27) => run!("ucvtf s0, {raw:w}, #27", "fmov {result:w}, s0"),
        (false, false, 28) => run!("ucvtf s0, {raw:w}, #28", "fmov {result:w}, s0"),
        (false, false, 29) => run!("ucvtf s0, {raw:w}, #29", "fmov {result:w}, s0"),
        (false, false, 30) => run!("ucvtf s0, {raw:w}, #30", "fmov {result:w}, s0"),
        (false, false, 31) => run!("ucvtf s0, {raw:w}, #31", "fmov {result:w}, s0"),
        (false, false, 32) => run!("ucvtf s0, {raw:w}, #32", "fmov {result:w}, s0"),
        (false, true, 0) => run!("scvtf s0, {raw:w}", "fmov {result:w}, s0"),
        (false, true, 1) => run!("scvtf s0, {raw:w}, #1", "fmov {result:w}, s0"),
        (false, true, 2) => run!("scvtf s0, {raw:w}, #2", "fmov {result:w}, s0"),
        (false, true, 3) => run!("scvtf s0, {raw:w}, #3", "fmov {result:w}, s0"),
        (false, true, 4) => run!("scvtf s0, {raw:w}, #4", "fmov {result:w}, s0"),
        (false, true, 5) => run!("scvtf s0, {raw:w}, #5", "fmov {result:w}, s0"),
        (false, true, 6) => run!("scvtf s0, {raw:w}, #6", "fmov {result:w}, s0"),
        (false, true, 7) => run!("scvtf s0, {raw:w}, #7", "fmov {result:w}, s0"),
        (false, true, 8) => run!("scvtf s0, {raw:w}, #8", "fmov {result:w}, s0"),
        (false, true, 9) => run!("scvtf s0, {raw:w}, #9", "fmov {result:w}, s0"),
        (false, true, 10) => run!("scvtf s0, {raw:w}, #10", "fmov {result:w}, s0"),
        (false, true, 11) => run!("scvtf s0, {raw:w}, #11", "fmov {result:w}, s0"),
        (false, true, 12) => run!("scvtf s0, {raw:w}, #12", "fmov {result:w}, s0"),
        (false, true, 13) => run!("scvtf s0, {raw:w}, #13", "fmov {result:w}, s0"),
        (false, true, 14) => run!("scvtf s0, {raw:w}, #14", "fmov {result:w}, s0"),
        (false, true, 15) => run!("scvtf s0, {raw:w}, #15", "fmov {result:w}, s0"),
        (false, true, 16) => run!("scvtf s0, {raw:w}, #16", "fmov {result:w}, s0"),
        (false, true, 17) => run!("scvtf s0, {raw:w}, #17", "fmov {result:w}, s0"),
        (false, true, 18) => run!("scvtf s0, {raw:w}, #18", "fmov {result:w}, s0"),
        (false, true, 19) => run!("scvtf s0, {raw:w}, #19", "fmov {result:w}, s0"),
        (false, true, 20) => run!("scvtf s0, {raw:w}, #20", "fmov {result:w}, s0"),
        (false, true, 21) => run!("scvtf s0, {raw:w}, #21", "fmov {result:w}, s0"),
        (false, true, 22) => run!("scvtf s0, {raw:w}, #22", "fmov {result:w}, s0"),
        (false, true, 23) => run!("scvtf s0, {raw:w}, #23", "fmov {result:w}, s0"),
        (false, true, 24) => run!("scvtf s0, {raw:w}, #24", "fmov {result:w}, s0"),
        (false, true, 25) => run!("scvtf s0, {raw:w}, #25", "fmov {result:w}, s0"),
        (false, true, 26) => run!("scvtf s0, {raw:w}, #26", "fmov {result:w}, s0"),
        (false, true, 27) => run!("scvtf s0, {raw:w}, #27", "fmov {result:w}, s0"),
        (false, true, 28) => run!("scvtf s0, {raw:w}, #28", "fmov {result:w}, s0"),
        (false, true, 29) => run!("scvtf s0, {raw:w}, #29", "fmov {result:w}, s0"),
        (false, true, 30) => run!("scvtf s0, {raw:w}, #30", "fmov {result:w}, s0"),
        (false, true, 31) => run!("scvtf s0, {raw:w}, #31", "fmov {result:w}, s0"),
        (false, true, 32) => run!("scvtf s0, {raw:w}, #32", "fmov {result:w}, s0"),
        (true, false, 0) => run!("ucvtf d0, {raw:w}", "fmov {result}, d0"),
        (true, false, 1) => run!("ucvtf d0, {raw:w}, #1", "fmov {result}, d0"),
        (true, false, 2) => run!("ucvtf d0, {raw:w}, #2", "fmov {result}, d0"),
        (true, false, 3) => run!("ucvtf d0, {raw:w}, #3", "fmov {result}, d0"),
        (true, false, 4) => run!("ucvtf d0, {raw:w}, #4", "fmov {result}, d0"),
        (true, false, 5) => run!("ucvtf d0, {raw:w}, #5", "fmov {result}, d0"),
        (true, false, 6) => run!("ucvtf d0, {raw:w}, #6", "fmov {result}, d0"),
        (true, false, 7) => run!("ucvtf d0, {raw:w}, #7", "fmov {result}, d0"),
        (true, false, 8) => run!("ucvtf d0, {raw:w}, #8", "fmov {result}, d0"),
        (true, false, 9) => run!("ucvtf d0, {raw:w}, #9", "fmov {result}, d0"),
        (true, false, 10) => run!("ucvtf d0, {raw:w}, #10", "fmov {result}, d0"),
        (true, false, 11) => run!("ucvtf d0, {raw:w}, #11", "fmov {result}, d0"),
        (true, false, 12) => run!("ucvtf d0, {raw:w}, #12", "fmov {result}, d0"),
        (true, false, 13) => run!("ucvtf d0, {raw:w}, #13", "fmov {result}, d0"),
        (true, false, 14) => run!("ucvtf d0, {raw:w}, #14", "fmov {result}, d0"),
        (true, false, 15) => run!("ucvtf d0, {raw:w}, #15", "fmov {result}, d0"),
        (true, false, 16) => run!("ucvtf d0, {raw:w}, #16", "fmov {result}, d0"),
        (true, false, 17) => run!("ucvtf d0, {raw:w}, #17", "fmov {result}, d0"),
        (true, false, 18) => run!("ucvtf d0, {raw:w}, #18", "fmov {result}, d0"),
        (true, false, 19) => run!("ucvtf d0, {raw:w}, #19", "fmov {result}, d0"),
        (true, false, 20) => run!("ucvtf d0, {raw:w}, #20", "fmov {result}, d0"),
        (true, false, 21) => run!("ucvtf d0, {raw:w}, #21", "fmov {result}, d0"),
        (true, false, 22) => run!("ucvtf d0, {raw:w}, #22", "fmov {result}, d0"),
        (true, false, 23) => run!("ucvtf d0, {raw:w}, #23", "fmov {result}, d0"),
        (true, false, 24) => run!("ucvtf d0, {raw:w}, #24", "fmov {result}, d0"),
        (true, false, 25) => run!("ucvtf d0, {raw:w}, #25", "fmov {result}, d0"),
        (true, false, 26) => run!("ucvtf d0, {raw:w}, #26", "fmov {result}, d0"),
        (true, false, 27) => run!("ucvtf d0, {raw:w}, #27", "fmov {result}, d0"),
        (true, false, 28) => run!("ucvtf d0, {raw:w}, #28", "fmov {result}, d0"),
        (true, false, 29) => run!("ucvtf d0, {raw:w}, #29", "fmov {result}, d0"),
        (true, false, 30) => run!("ucvtf d0, {raw:w}, #30", "fmov {result}, d0"),
        (true, false, 31) => run!("ucvtf d0, {raw:w}, #31", "fmov {result}, d0"),
        (true, false, 32) => run!("ucvtf d0, {raw:w}, #32", "fmov {result}, d0"),
        (true, true, 0) => run!("scvtf d0, {raw:w}", "fmov {result}, d0"),
        (true, true, 1) => run!("scvtf d0, {raw:w}, #1", "fmov {result}, d0"),
        (true, true, 2) => run!("scvtf d0, {raw:w}, #2", "fmov {result}, d0"),
        (true, true, 3) => run!("scvtf d0, {raw:w}, #3", "fmov {result}, d0"),
        (true, true, 4) => run!("scvtf d0, {raw:w}, #4", "fmov {result}, d0"),
        (true, true, 5) => run!("scvtf d0, {raw:w}, #5", "fmov {result}, d0"),
        (true, true, 6) => run!("scvtf d0, {raw:w}, #6", "fmov {result}, d0"),
        (true, true, 7) => run!("scvtf d0, {raw:w}, #7", "fmov {result}, d0"),
        (true, true, 8) => run!("scvtf d0, {raw:w}, #8", "fmov {result}, d0"),
        (true, true, 9) => run!("scvtf d0, {raw:w}, #9", "fmov {result}, d0"),
        (true, true, 10) => run!("scvtf d0, {raw:w}, #10", "fmov {result}, d0"),
        (true, true, 11) => run!("scvtf d0, {raw:w}, #11", "fmov {result}, d0"),
        (true, true, 12) => run!("scvtf d0, {raw:w}, #12", "fmov {result}, d0"),
        (true, true, 13) => run!("scvtf d0, {raw:w}, #13", "fmov {result}, d0"),
        (true, true, 14) => run!("scvtf d0, {raw:w}, #14", "fmov {result}, d0"),
        (true, true, 15) => run!("scvtf d0, {raw:w}, #15", "fmov {result}, d0"),
        (true, true, 16) => run!("scvtf d0, {raw:w}, #16", "fmov {result}, d0"),
        (true, true, 17) => run!("scvtf d0, {raw:w}, #17", "fmov {result}, d0"),
        (true, true, 18) => run!("scvtf d0, {raw:w}, #18", "fmov {result}, d0"),
        (true, true, 19) => run!("scvtf d0, {raw:w}, #19", "fmov {result}, d0"),
        (true, true, 20) => run!("scvtf d0, {raw:w}, #20", "fmov {result}, d0"),
        (true, true, 21) => run!("scvtf d0, {raw:w}, #21", "fmov {result}, d0"),
        (true, true, 22) => run!("scvtf d0, {raw:w}, #22", "fmov {result}, d0"),
        (true, true, 23) => run!("scvtf d0, {raw:w}, #23", "fmov {result}, d0"),
        (true, true, 24) => run!("scvtf d0, {raw:w}, #24", "fmov {result}, d0"),
        (true, true, 25) => run!("scvtf d0, {raw:w}, #25", "fmov {result}, d0"),
        (true, true, 26) => run!("scvtf d0, {raw:w}, #26", "fmov {result}, d0"),
        (true, true, 27) => run!("scvtf d0, {raw:w}, #27", "fmov {result}, d0"),
        (true, true, 28) => run!("scvtf d0, {raw:w}, #28", "fmov {result}, d0"),
        (true, true, 29) => run!("scvtf d0, {raw:w}, #29", "fmov {result}, d0"),
        (true, true, 30) => run!("scvtf d0, {raw:w}, #30", "fmov {result}, d0"),
        (true, true, 31) => run!("scvtf d0, {raw:w}, #31", "fmov {result}, d0"),
        (true, true, 32) => run!("scvtf d0, {raw:w}, #32", "fmov {result}, d0"),
        _ => unreachable!("bounded native conversion profile"),
    }
    (result, status as u32 & 0x9f)
}

fn helper_to_integer(
    raw: u64,
    double: bool,
    signed: bool,
    fbits: u32,
    mode: RoundingMode,
    control: u32,
) -> (u32, u32) {
    let mut fpscr = Fpscr::from_bits(control);
    let result = match (double, signed, fbits) {
        (true, true, 0) => vfp::vcvt_s32_f64_round(f64::from_bits(raw), mode, &mut fpscr) as u32,
        (true, false, 0) => vfp::vcvt_u32_f64_round(f64::from_bits(raw), mode, &mut fpscr),
        (false, true, 0) => {
            vfp::vcvt_s32_f32_round(f32::from_bits(raw as u32), mode, &mut fpscr) as u32
        }
        (false, false, 0) => vfp::vcvt_u32_f32_round(f32::from_bits(raw as u32), mode, &mut fpscr),
        (true, true, bits) => vfp::vcvt_s32_f64_fixed(f64::from_bits(raw), bits, &mut fpscr) as u32,
        (true, false, bits) => vfp::vcvt_u32_f64_fixed(f64::from_bits(raw), bits, &mut fpscr),
        (false, true, bits) => {
            vfp::vcvt_s32_f32_fixed(f32::from_bits(raw as u32), bits, &mut fpscr) as u32
        }
        (false, false, bits) => {
            vfp::vcvt_u32_f32_fixed(f32::from_bits(raw as u32), bits, &mut fpscr)
        }
    };
    (result, fpscr.bits() & 0x9f)
}

fn helper_from_integer(
    raw: u32,
    double: bool,
    signed: bool,
    fbits: u32,
    control: u32,
) -> (u64, u32) {
    let mut fpscr = Fpscr::from_bits(control);
    let result = match (double, signed, fbits) {
        (false, false, 0) => u64::from(vfp::vcvt_f32_u32_with_fpscr(raw, &mut fpscr).to_bits()),
        (false, false, bits) => {
            u64::from(vfp::vcvt_f32_u32_fixed_with_fpscr(raw, bits, &mut fpscr).to_bits())
        }
        (false, true, 0) => {
            u64::from(vfp::vcvt_f32_s32_with_fpscr(raw as i32, &mut fpscr).to_bits())
        }
        (false, true, bits) => {
            u64::from(vfp::vcvt_f32_s32_fixed_with_fpscr(raw as i32, bits, &mut fpscr).to_bits())
        }
        (true, false, 0) => vfp::vcvt_f64_u32_with_fpscr(raw, &mut fpscr).to_bits(),
        (true, false, bits) => vfp::vcvt_f64_u32_fixed_with_fpscr(raw, bits, &mut fpscr).to_bits(),
        (true, true, 0) => vfp::vcvt_f64_s32_with_fpscr(raw as i32, &mut fpscr).to_bits(),
        (true, true, bits) => {
            vfp::vcvt_f64_s32_fixed_with_fpscr(raw as i32, bits, &mut fpscr).to_bits()
        }
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
fn binary32_binary64_to_integer_match_native_results_and_status() {
    let _entry = HostState::read();
    let mut seed = 0x72c6_6bad_84d1_f039;
    let modes = [
        RoundingMode::RoundNearest,
        RoundingMode::RoundPlusInf,
        RoundingMode::RoundMinusInf,
        RoundingMode::RoundZero,
        RoundingMode::RoundTiesAway,
    ];
    let mut comparisons = 0;
    for double in [false, true] {
        let edges: &[u64] = if double {
            &[
                0,
                1,
                0x8000000000000000,
                0x8000000000000001,
                0x3fe0000000000000,
                0xbfe0000000000000,
                0x41dfffffffc00000,
                0xc1e0000000000000,
                0x41efffffffe00000,
                0x7ff0000000000000,
                0xfff0000000000000,
                0x7ff8000000000001,
                0x7ff0000000000001,
            ]
        } else {
            &[
                0, 1, 0x80000000, 0x80000001, 0x3f000000, 0xbf000000, 0x4effffff, 0xcf000000,
                0x4f7fffff, 0x7f800000, 0xff800000, 0x7fc00001, 0x7f800001,
            ]
        };
        for index in 0..2000 {
            let raw = if index < edges.len() {
                edges[index]
            } else {
                let bits = next(&mut seed);
                if double { bits } else { u64::from(bits as u32) }
            };
            for signed in [false, true] {
                for rm in 0..4 {
                    for flush in 0..2 {
                        for dn in 0..2 {
                            let control = (rm << 22) | (flush << 24) | (dn << 25);
                            for fbits in 0..=32 {
                                for mode in modes {
                                    if fbits != 0 && mode != RoundingMode::RoundZero {
                                        continue;
                                    }
                                    let actual = helper_to_integer(
                                        raw, double, signed, fbits, mode, control,
                                    );
                                    let expected = native_to_integer(
                                        raw, double, signed, fbits, mode, control,
                                    );
                                    assert_eq!(
                                        actual, expected,
                                        "raw={raw:016x} double={double} signed={signed} fbits={fbits} mode={mode:?} control={control:x}"
                                    );
                                    comparisons += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(comparisons, 4_736_000);
}

#[test]
fn integer_to_binary32_binary64_match_native_results_and_status() {
    let _entry = HostState::read();
    let mut seed = 0x294c_a175_0b93_6fd2;
    let mut comparisons = 0;
    let edges = [
        0,
        1,
        u32::MAX,
        0x80000000,
        0x7fffffff,
        0x01000001,
        0x01000003,
        0xfeffffff,
        0x0000ffff,
    ];
    for index in 0..2000 {
        let raw = if index < edges.len() {
            edges[index]
        } else {
            next(&mut seed) as u32
        };
        for double in [false, true] {
            for signed in [false, true] {
                for rm in 0..4 {
                    for flush in 0..2 {
                        for dn in 0..2 {
                            for fbits in 0..=32 {
                                let control = (rm << 22) | (flush << 24) | (dn << 25);
                                let actual =
                                    helper_from_integer(raw, double, signed, fbits, control);
                                let expected =
                                    native_from_integer(raw, double, signed, fbits, control);
                                assert_eq!(
                                    actual, expected,
                                    "raw={raw:08x} double={double} signed={signed} fbits={fbits} control={control:x}"
                                );
                                comparisons += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(comparisons, 4_224_000);
}

#[test]
fn integer_conversion_helpers_preserve_the_host_fp_control_and_status() {
    let _entry = HostState::read();
    HostState::set((1 << 22) | (1 << 24) | (1 << 25), 0x0800_0004);
    let before = HostState::read();
    for raw in [0, 1, 0x8000_0000, u32::MAX, 0x0100_0001] {
        let raw = std::hint::black_box(raw);
        for double in [false, true] {
            for signed in [false, true] {
                std::hint::black_box(helper_from_integer(raw, double, signed, 0, 1 << 22));
                std::hint::black_box(helper_from_integer(raw, double, signed, 16, 1 << 22));
                std::hint::black_box(helper_to_integer(
                    u64::from(raw),
                    double,
                    signed,
                    0,
                    RoundingMode::RoundTiesAway,
                    0,
                ));
            }
        }
        std::hint::black_box(vfp::vcvt_f32_s32(raw as i32));
        std::hint::black_box(vfp::vcvt_f64_u32_fixed(raw, 32));
    }
    assert_eq!(
        vfp::vcvt_f32_s32(std::hint::black_box(0x0100_0001)).to_bits(),
        0x4b80_0000,
    );
    let after = HostState::read();
    assert_eq!((after.0, after.1), (before.0, before.1));
}
