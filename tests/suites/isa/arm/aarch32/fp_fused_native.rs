//! Native A64 controls for the common binary32/binary64 fused arithmetic profile.
//! This checks arithmetic results/status, not native A32 instruction execution.
use rax::isa::arm::aarch32::vfp::{self, Fpscr};
use std::arch::asm;

struct HostState {
    control: u64,
    status: u64,
}

impl HostState {
    fn save() -> Self {
        let control;
        let status;
        // SAFETY: these are EL0 thread-local floating-point state registers.
        unsafe {
            asm!("mrs {control}, fpcr", "mrs {status}, fpsr", control = out(reg) control, status = out(reg) status, options(nomem, nostack));
        }
        Self { control, status }
    }
}

impl Drop for HostState {
    fn drop(&mut self) {
        // SAFETY: restore this thread's exact entry state, including on unwind.
        unsafe {
            asm!("msr fpcr, {control}", "msr fpsr, {status}", control = in(reg) self.control, status = in(reg) self.status, options(nomem, nostack));
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Operation {
    Add,
    Subtract,
    NegatedAdd,
    NegatedSubtract,
}

fn native32(operation: Operation, acc: u32, first: u32, second: u32, control: u32) -> (u32, u32) {
    macro_rules! execute { ($instruction:literal)=> {{
        let result:u32;
        let status:u64;
        // SAFETY: traps disabled, declared register clobbers; entry FP state is restored by HostState.
        unsafe { asm!("msr fpcr, {control}", "msr fpsr, xzr",
            "fmov s0, {acc:w}", "fmov s1, {first:w}", "fmov s2, {second:w}",
            $instruction, "fmov {result:w}, s0", "mrs {status}, fpsr",
            control=in(reg) u64::from(control), acc=in(reg) acc, first=in(reg) first, second=in(reg) second,
            result=lateout(reg) result,status=lateout(reg) status,
            out("v0") _,out("v1") _,out("v2") _,options(nomem,nostack)); }
        (result,status as u32)
    }}; }
    match operation {
        Operation::Add => execute!("fmadd s0, s1, s2, s0"),
        Operation::Subtract => execute!("fmsub s0, s1, s2, s0"),
        Operation::NegatedAdd => execute!("fnmadd s0, s1, s2, s0"),
        Operation::NegatedSubtract => execute!("fnmsub s0, s1, s2, s0"),
    }
}
fn evaluate32(operation: Operation, acc: u32, a: u32, b: u32, control: u32) -> (u32, u32) {
    let mut fpscr = Fpscr::from_bits(control);
    let (acc, a, b) = (f32::from_bits(acc), f32::from_bits(a), f32::from_bits(b));
    let result = match operation {
        Operation::Add => vfp::vfma_f32(acc, a, b, &mut fpscr),
        Operation::Subtract => vfp::vfms_f32(acc, a, b, &mut fpscr),
        Operation::NegatedAdd => vfp::vfnma_f32(acc, a, b, &mut fpscr),
        Operation::NegatedSubtract => vfp::vfnms_f32(acc, a, b, &mut fpscr),
    };
    (result.to_bits(), fpscr.bits() & 0x9f)
}

fn native64(operation: Operation, acc: u64, first: u64, second: u64, control: u32) -> (u64, u32) {
    macro_rules! execute { ($instruction:literal)=> {{
        let result:u64;
        let status:u64;
        // SAFETY: traps disabled, declared register clobbers; entry FP state is restored by HostState.
        unsafe { asm!("msr fpcr, {control}", "msr fpsr, xzr",
            "fmov d0, {acc}", "fmov d1, {first}", "fmov d2, {second}",
            $instruction, "fmov {result}, d0", "mrs {status}, fpsr",
            control=in(reg) u64::from(control), acc=in(reg) acc, first=in(reg) first, second=in(reg) second,
            result=lateout(reg) result,status=lateout(reg) status,
            out("v0") _,out("v1") _,out("v2") _,options(nomem,nostack)); }
        (result,status as u32)
    }}; }
    match operation {
        Operation::Add => execute!("fmadd d0, d1, d2, d0"),
        Operation::Subtract => execute!("fmsub d0, d1, d2, d0"),
        Operation::NegatedAdd => execute!("fnmadd d0, d1, d2, d0"),
        Operation::NegatedSubtract => execute!("fnmsub d0, d1, d2, d0"),
    }
}
fn evaluate64(operation: Operation, acc: u64, a: u64, b: u64, control: u32) -> (u64, u32) {
    let mut fpscr = Fpscr::from_bits(control);
    let (acc, a, b) = (f64::from_bits(acc), f64::from_bits(a), f64::from_bits(b));
    let result = match operation {
        Operation::Add => vfp::vfma_f64(acc, a, b, &mut fpscr),
        Operation::Subtract => vfp::vfms_f64(acc, a, b, &mut fpscr),
        Operation::NegatedAdd => vfp::vfnma_f64(acc, a, b, &mut fpscr),
        Operation::NegatedSubtract => vfp::vfnms_f64(acc, a, b, &mut fpscr),
    };
    (result.to_bits(), fpscr.bits() & 0x9f)
}

#[test]
fn seeded_fused_arithmetic_matches_native_common_arm_profile() {
    let _entry = HostState::save();
    let mut seed = 0x05a3_e1d7_b46c_928f_u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for operation in [
        Operation::Add,
        Operation::Subtract,
        Operation::NegatedAdd,
        Operation::NegatedSubtract,
    ] {
        for index in 0..25_000 {
            let acc = next();
            let a = next();
            let b = next();
            let controls = next() as u32;
            let control = ((controls & 3) << 22)
                | (((controls >> 2) & 1) << 24)
                | (((controls >> 3) & 1) << 25);
            let expected = native32(operation, acc as u32, a as u32, b as u32, control);
            assert_eq!(
                evaluate32(operation, acc as u32, a as u32, b as u32, control),
                expected,
                "{operation:?} f32 case {index}, acc={acc:x}, a={a:x}, b={b:x}, control={control:x}"
            );
            let expected = native64(operation, acc, a, b, control);
            assert_eq!(
                evaluate64(operation, acc, a, b, control),
                expected,
                "{operation:?} f64 case {index}, acc={acc:x}, a={a:x}, b={b:x}, control={control:x}"
            );
        }
    }
}
#[test]
fn fused_software_preserves_host_control_and_status() {
    let _entry = HostState::save();
    let control = (1_u64 << 22) | (3 << 24);
    for status in [0_u64, 0x80, 0x9f] {
        // SAFETY: thread-local masked controls/status restored by the entry guard.
        unsafe {
            asm!("msr fpcr, {control}","msr fpsr, {status}",control=in(reg) control,status=in(reg) status,options(nomem,nostack));
        }
        for operation in [
            Operation::Add,
            Operation::Subtract,
            Operation::NegatedAdd,
            Operation::NegatedSubtract,
        ] {
            evaluate32(operation, 0x3f800000, 0x33800000, 0x3f800000, 0);
            evaluate64(
                operation,
                0x3ff0000000000000,
                0x3ca0000000000000,
                0x3ff0000000000000,
                0,
            );
            let after = HostState::save();
            assert_eq!(
                (after.control, after.status),
                (control, status),
                "{operation:?}"
            );
        }
    }
}
