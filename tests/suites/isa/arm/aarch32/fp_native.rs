//! Native AArch64 EL0 controls for the common Arm binary32/binary64 FP profile.
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
    Multiply,
    Divide,
    SquareRoot,
}

fn native32(operation: Operation, first: u32, second: u32, control: u32) -> (u32, u32) {
    macro_rules! execute {
        ($operation:literal) => {{
            let result: u32;
            let status: u64;
            // SAFETY: FP traps are disabled in control; only declared scalar
            // FP/GPR operands and this thread's saved FP state are modified.
            unsafe { asm!(
                "msr fpcr, {control}", "msr fpsr, xzr",
                "fmov s0, {first:w}", "fmov s1, {second:w}", $operation,
                "fmov {result:w}, s0", "mrs {status}, fpsr",
                control = in(reg) u64::from(control), first = in(reg) first, second = in(reg) second,
                result = lateout(reg) result, status = lateout(reg) status,
                out("v0") _, out("v1") _, options(nomem, nostack)
            ); }
            (result, status as u32)
        }};
    }
    match operation {
        Operation::Add => execute!("fadd s0, s0, s1"),
        Operation::Subtract => execute!("fsub s0, s0, s1"),
        Operation::Multiply => execute!("fmul s0, s0, s1"),
        Operation::Divide => execute!("fdiv s0, s0, s1"),
        Operation::SquareRoot => execute!("fsqrt s0, s0"),
    }
}

fn native64(operation: Operation, first: u64, second: u64, control: u32) -> (u64, u32) {
    macro_rules! execute {
        ($operation:literal) => {{
            let result: u64;
            let status: u64;
            // SAFETY: same thread-local, declared-operand contract as native32.
            unsafe { asm!(
                "msr fpcr, {control}", "msr fpsr, xzr",
                "fmov d0, {first}", "fmov d1, {second}", $operation,
                "fmov {result}, d0", "mrs {status}, fpsr",
                control = in(reg) u64::from(control), first = in(reg) first, second = in(reg) second,
                result = lateout(reg) result, status = lateout(reg) status,
                out("v0") _, out("v1") _, options(nomem, nostack)
            ); }
            (result, status as u32)
        }};
    }
    match operation {
        Operation::Add => execute!("fadd d0, d0, d1"),
        Operation::Subtract => execute!("fsub d0, d0, d1"),
        Operation::Multiply => execute!("fmul d0, d0, d1"),
        Operation::Divide => execute!("fdiv d0, d0, d1"),
        Operation::SquareRoot => execute!("fsqrt d0, d0"),
    }
}

fn evaluate32(operation: Operation, a: u32, b: u32, control: u32) -> (u32, u32) {
    let mut fpscr = Fpscr::from_bits(control);
    let a = f32::from_bits(a);
    let b = f32::from_bits(b);
    let result = match operation {
        Operation::Add => vfp::vadd_f32(a, b, &mut fpscr),
        Operation::Subtract => vfp::vsub_f32(a, b, &mut fpscr),
        Operation::Multiply => vfp::vmul_f32(a, b, &mut fpscr),
        Operation::Divide => vfp::vdiv_f32(a, b, &mut fpscr),
        Operation::SquareRoot => vfp::vsqrt_f32(a, &mut fpscr),
    };
    (result.to_bits(), fpscr.bits() & 0x9f)
}

fn evaluate64(operation: Operation, a: u64, b: u64, control: u32) -> (u64, u32) {
    let mut fpscr = Fpscr::from_bits(control);
    let a = f64::from_bits(a);
    let b = f64::from_bits(b);
    let result = match operation {
        Operation::Add => vfp::vadd_f64(a, b, &mut fpscr),
        Operation::Subtract => vfp::vsub_f64(a, b, &mut fpscr),
        Operation::Multiply => vfp::vmul_f64(a, b, &mut fpscr),
        Operation::Divide => vfp::vdiv_f64(a, b, &mut fpscr),
        Operation::SquareRoot => vfp::vsqrt_f64(a, &mut fpscr),
    };
    (result.to_bits(), fpscr.bits() & 0x9f)
}

#[test]
fn seeded_basic_arithmetic_matches_native_common_arm_profile() {
    let _entry = HostState::save();
    let mut seed = 0x07d3_8b92_64a1_ef05_u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for operation in [
        Operation::Add,
        Operation::Subtract,
        Operation::Multiply,
        Operation::Divide,
        Operation::SquareRoot,
    ] {
        for index in 0..10_000 {
            let a = next();
            let b = next();
            let controls = next() as u32;
            let control = ((controls & 3) << 22)
                | (((controls >> 2) & 1) << 24)
                | (((controls >> 3) & 1) << 25);
            let expected = native32(operation, a as u32, b as u32, control);
            assert_eq!(
                evaluate32(operation, a as u32, b as u32, control),
                expected,
                "{operation:?}, f32 case {index}, a={a:x}, b={b:x}, control={control:x}"
            );
            let expected = native64(operation, a, b, control);
            assert_eq!(
                evaluate64(operation, a, b, control),
                expected,
                "{operation:?}, f64 case {index}, a={a:x}, b={b:x}, control={control:x}"
            );
        }
    }
}

#[test]
fn software_results_and_guest_flags_preserve_host_control_and_status() {
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
            Operation::Multiply,
            Operation::Divide,
            Operation::SquareRoot,
        ] {
            evaluate32(operation, 0x3f800000, 0x33800000, 0);
            evaluate64(operation, 0x3ff0000000000000, 0x3ca0000000000000, 0);
            let after = HostState::save();
            assert_eq!(
                (after.control, after.status),
                (control, status),
                "{operation:?}"
            );
        }
    }
}
