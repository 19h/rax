//! Native A64 FCVT controls for the common A32 binary32/binary64 profile.
//! This is a hardware arithmetic oracle, not native A32 instruction execution.
use rax::isa::arm::aarch32::vfp::{self, Fpscr};
use rax::isa::arm::decoder::Aarch32Decoder;
use rax::isa::arm::execution::FlatMemory;
use rax::isa::arm::{Armv7Cpu, ExecResult, Executor};
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

fn native(raw: u64, widen: bool, control: u32) -> (u64, u32) {
    let result: u64;
    let status: u64;
    if widen {
        // SAFETY: traps are disabled, v0 is declared clobbered, inputs/results
        // cross the FP boundary with bit-preserving FMOV instructions only.
        unsafe {
            asm!(
                "msr fpcr, {control}", "msr fpsr, xzr", "fmov s0, {raw:w}",
                "fcvt d0, s0", "fmov {result}, d0", "mrs {status}, fpsr",
                control=in(reg) u64::from(control), raw=in(reg) raw,
                result=lateout(reg) result,status=lateout(reg) status,out("v0") _,options(nomem,nostack)
            );
        }
    } else {
        // SAFETY: same declared operands and masked thread-local FP state.
        unsafe {
            asm!(
                "msr fpcr, {control}", "msr fpsr, xzr", "fmov d0, {raw}",
                "fcvt s0, d0", "fmov {result:w}, s0", "mrs {status}, fpsr",
                control=in(reg) u64::from(control),raw=in(reg) raw,
                result=lateout(reg) result,status=lateout(reg) status,out("v0") _,options(nomem,nostack)
            );
        }
    }
    (result, status as u32 & 0x9f)
}

fn decoded(
    cpu: &mut Armv7Cpu,
    memory: &mut FlatMemory,
    raw: u64,
    widen: bool,
    control: u32,
) -> (u64, u32) {
    cpu.vfp.fpscr = Fpscr::from_bits(control);
    let word = if widen {
        cpu.vfp.write_s_bits(2, raw as u32);
        0xeeb70ac1
    } else {
        cpu.vfp.write_d_bits(1, raw);
        0xeeb70bc1
    };
    let insn = Aarch32Decoder::decode(word).unwrap();
    assert!(matches!(
        Executor::new(cpu, memory).execute(&insn),
        ExecResult::Continue
    ));
    (
        if widen {
            cpu.vfp.read_d_bits(0)
        } else {
            u64::from(cpu.vfp.read_s_bits(0))
        },
        cpu.vfp.fpscr.bits() & 0x9f,
    )
}
fn helper(raw: u64, widen: bool, control: u32) -> (u64, u32) {
    let mut fpscr = Fpscr::from_bits(control);
    let result = if widen {
        vfp::vcvt_f64_f32_with_fpscr(f32::from_bits(raw as u32), &mut fpscr).to_bits()
    } else {
        u64::from(vfp::vcvt_f32_f64(f64::from_bits(raw), &mut fpscr).to_bits())
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
fn precision_changes_match_native_fcvt_for_results_and_status() {
    let _entry = HostState::read();
    let mut seed = 0x6c3a_94f1_d82e_570b;
    let mut cpu = Armv7Cpu::new();
    let mut memory = FlatMemory::new(0x1000, 0);
    let edges = [
        0,
        1,
        0x80000000,
        0x7f800001,
        0xffc12345,
        0x7f7fffff,
        0x007fffff,
        0x00800000,
        0x7ff0000000000001,
        0xfff8123456789abc,
        0x36a0000000000000,
        0x380fffffe0000000,
        0x47f0000000000000,
        u64::MAX,
    ];
    for index in 0..20000 {
        let raw = if index < edges.len() {
            edges[index]
        } else {
            next(&mut seed)
        };
        for mode in 0..16_u32 {
            let control =
                ((mode & 3) << 22) | (((mode >> 2) & 1) << 24) | (((mode >> 3) & 1) << 25);
            for widen in [false, true] {
                let expected = native(raw, widen, control);
                // Software must honor guest controls even with a different
                // host mode. Reading the result must not change host status.
                HostState::set(0, 0);
                for (plane, actual) in [
                    ("helper", helper(raw, widen, control)),
                    (
                        "decoded",
                        decoded(&mut cpu, &mut memory, raw, widen, control),
                    ),
                ] {
                    assert_eq!(
                        actual, expected,
                        "{plane} index={index} raw={raw:016x} widen={widen} control={control:08x}"
                    );
                }
                let state = HostState::read();
                assert_eq!(
                    (state.0, state.1),
                    (0, 0),
                    "host state index={index} widen={widen}"
                );
                // Prevent each sampled state from restoring over later oracle
                // calls; it already equals the intended zero state here.
                drop(state);
            }
        }
    }
}

#[test]
fn precision_helpers_preserve_nondefault_host_control_and_status() {
    let _entry = HostState::read();
    let host_control = (1_u64 << 22) | (3 << 24);
    for status in [0_u64, 0x80, 0x9f] {
        HostState::set(host_control, status);
        for control in [0_u32, 1 << 22, 2 << 22, 3 << 22, 1 << 24, 1 << 25] {
            for raw in [
                1,
                0x3ff0000010000000,
                0xfff0000000000001,
                0x47f0000000000000,
                0x7f800001,
            ] {
                for widen in [false, true] {
                    std::hint::black_box(helper(raw, widen, control));
                    let state = HostState::read();
                    assert_eq!((state.0, state.1), (host_control, status));
                }
                std::hint::black_box(vfp::vcvt_f64_f32(f32::from_bits(raw as u32)));
                let state = HostState::read();
                assert_eq!((state.0, state.1), (host_control, status));
            }
        }
    }
}
