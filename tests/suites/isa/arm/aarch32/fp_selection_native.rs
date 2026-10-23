//! Native A64 controls for the common binary32/binary64 selection and comparison profile.
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
    MaximumNumber,
    MinimumNumber,
    Compare,
    CompareException,
}

fn native32(operation: Operation, a: u32, b: u32, control: u32) -> (u64, u32) {
    macro_rules! execute { ($instruction:literal,$read:literal)=> {{
        let result:u64;let status:u64;
        // SAFETY: thread-local masked FP state, declared SIMD/GPR and NZCV clobbers.
        unsafe { asm!("msr fpcr, {control}","msr fpsr, xzr",
            "fmov s0, {a:w}","fmov s1, {b:w}",$instruction,$read,"mrs {status}, fpsr",
            control=in(reg) u64::from(control),a=in(reg) a,b=in(reg) b,
            result=lateout(reg) result,status=lateout(reg) status,
            out("v0") _,out("v1") _,options(nomem,nostack)); }
        (result,status as u32)
    }}; }
    match operation {
        Operation::MaximumNumber => execute!("fmaxnm s0, s0, s1", "fmov {result:w}, s0"),
        Operation::MinimumNumber => execute!("fminnm s0, s0, s1", "fmov {result:w}, s0"),
        Operation::Compare => execute!("fcmp s0, s1", "mrs {result}, nzcv"),
        Operation::CompareException => execute!("fcmpe s0, s1", "mrs {result}, nzcv"),
    }
}
fn evaluate32(operation: Operation, a: u32, b: u32, control: u32) -> (u64, u32) {
    let mut fpscr = Fpscr::from_bits(control);
    let (a, b) = (f32::from_bits(a), f32::from_bits(b));
    let bits = match operation {
        Operation::MaximumNumber => vfp::vmaxnm_f32(a, b, &mut fpscr).to_bits() as u64,
        Operation::MinimumNumber => vfp::vminnm_f32(a, b, &mut fpscr).to_bits() as u64,
        Operation::Compare | Operation::CompareException => {
            vfp::vcmp_f32_with_exception(
                a,
                b,
                matches!(operation, Operation::CompareException),
                &mut fpscr,
            );
            u64::from(fpscr.bits() & 0xf0000000)
        }
    };
    (bits, fpscr.bits() & 0x9f)
}

fn native64(operation: Operation, a: u64, b: u64, control: u32) -> (u64, u32) {
    macro_rules! execute { ($instruction:literal,$read:literal)=> {{
        let result:u64;let status:u64;
        // SAFETY: thread-local masked FP state, declared SIMD/GPR and NZCV clobbers.
        unsafe { asm!("msr fpcr, {control}","msr fpsr, xzr",
            "fmov d0, {a}","fmov d1, {b}",$instruction,$read,"mrs {status}, fpsr",
            control=in(reg) u64::from(control),a=in(reg) a,b=in(reg) b,
            result=lateout(reg) result,status=lateout(reg) status,
            out("v0") _,out("v1") _,options(nomem,nostack)); }
        (result,status as u32)
    }}; }
    match operation {
        Operation::MaximumNumber => execute!("fmaxnm d0, d0, d1", "fmov {result}, d0"),
        Operation::MinimumNumber => execute!("fminnm d0, d0, d1", "fmov {result}, d0"),
        Operation::Compare => execute!("fcmp d0, d1", "mrs {result}, nzcv"),
        Operation::CompareException => execute!("fcmpe d0, d1", "mrs {result}, nzcv"),
    }
}
fn evaluate64(operation: Operation, a: u64, b: u64, control: u32) -> (u64, u32) {
    let mut fpscr = Fpscr::from_bits(control);
    let (a, b) = (f64::from_bits(a), f64::from_bits(b));
    let bits = match operation {
        Operation::MaximumNumber => vfp::vmaxnm_f64(a, b, &mut fpscr).to_bits() as u64,
        Operation::MinimumNumber => vfp::vminnm_f64(a, b, &mut fpscr).to_bits() as u64,
        Operation::Compare | Operation::CompareException => {
            vfp::vcmp_f64_with_exception(
                a,
                b,
                matches!(operation, Operation::CompareException),
                &mut fpscr,
            );
            u64::from(fpscr.bits() & 0xf0000000)
        }
    };
    (bits, fpscr.bits() & 0x9f)
}

#[test]
fn seeded_selection_and_comparison_match_native_common_arm_profile() {
    let _entry = HostState::save();
    let mut seed = 0x37c2_54e0_9b6d_18af_u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for operation in [
        Operation::MaximumNumber,
        Operation::MinimumNumber,
        Operation::Compare,
        Operation::CompareException,
    ] {
        for index in 0..25_000 {
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
                "{operation:?} f32 case {index}, a={a:x}, b={b:x}, control={control:x}"
            );
            let expected = native64(operation, a, b, control);
            assert_eq!(
                evaluate64(operation, a, b, control),
                expected,
                "{operation:?} f64 case {index}, a={a:x}, b={b:x}, control={control:x}"
            );
        }
    }
}

#[test]
fn selection_software_preserves_host_control_and_status() {
    let _entry = HostState::save();
    let control = (1_u64 << 22) | (3 << 24);
    for status in [0_u64, 0x80, 0x9f] {
        // SAFETY: thread-local masked controls/status restored by the entry guard.
        unsafe {
            asm!("msr fpcr, {control}","msr fpsr, {status}",control=in(reg) control,status=in(reg) status,options(nomem,nostack));
        }
        for operation in [
            Operation::MaximumNumber,
            Operation::MinimumNumber,
            Operation::Compare,
            Operation::CompareException,
        ] {
            evaluate32(operation, 0x7f801234, 1, 0);
            evaluate64(operation, 0x7ff0000000001234, 1, 0);
            let after = HostState::save();
            assert_eq!(
                (after.control, after.status),
                (control, status),
                "{operation:?}"
            );
        }
    }
}

fn native_standard_minmax32(maximum: bool, a: u32, b: u32) -> (u32, u32) {
    macro_rules! execute { ($instruction:literal)=> {{
        let result:u32;let status:u64;
        // SAFETY: StandardFPSCRValue enables no traps; all clobbers declared.
        unsafe { asm!("msr fpcr, {control}","msr fpsr, xzr",
            "fmov s0, {a:w}","fmov s1, {b:w}",$instruction,
            "fmov {result:w}, s0","mrs {status}, fpsr",
            control=in(reg) 0x03000000_u64,a=in(reg) a,b=in(reg) b,
            result=lateout(reg) result,status=lateout(reg) status,
            out("v0") _,out("v1") _,options(nomem,nostack)); }
        (result,status as u32)
    }}; }
    if maximum {
        execute!("fmax s0, s0, s1")
    } else {
        execute!("fmin s0, s0, s1")
    }
}

#[test]
fn seeded_decoded_neon_selection_matches_native_standard_arm_profile() {
    use rax::isa::arm::decoder::Aarch32Decoder;
    use rax::isa::arm::execution::FlatMemory;
    use rax::isa::arm::{Armv7Cpu, ExecResult, Executor};
    let _entry = HostState::save();
    let mut cpu = Armv7Cpu::new();
    let mut memory = FlatMemory::new(0x1000, 0);
    let mut seed = 0x1bd7_5a93_62e4_0cf8_u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed as u32
    };
    let repeated = |bits: u32| u64::from(bits) | (u64::from(bits) << 32);
    for (word, maximum) in [(0xf2010f02, true), (0xf2210f02, false)] {
        let instruction = Aarch32Decoder::decode(word).unwrap();
        for index in 0..20_000 {
            let a = next();
            let b = next();
            let (bits, status) = native_standard_minmax32(maximum, a, b);
            cpu.vfp.write_d_bits(1, repeated(a));
            cpu.vfp.write_d_bits(2, repeated(b));
            cpu.vfp.fpscr = Fpscr::from_bits(0xa8400000);
            assert!(matches!(
                Executor::new(&mut cpu, &mut memory).execute(&instruction),
                ExecResult::Continue
            ));
            assert_eq!(
                (cpu.vfp.read_d_bits(0), cpu.vfp.fpscr.bits()),
                (repeated(bits), 0xa8400000 | status),
                "word={word:x} case={index} a={a:x} b={b:x}"
            );
        }
    }
}
