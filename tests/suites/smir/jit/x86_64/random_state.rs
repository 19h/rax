//! End-to-end native-tier coverage for state-backed `RDRAND`/`RDSEED`
//! destinations. Random values are nondeterministic, so the independent
//! interpreter and JIT executions are checked independently against
//! architectural invariants: exact flags, zero-on-failure, width semantics,
//! and all unaffected registers.

use super::*;

const STATUS: u64 = 0x08D5;
const NON_STATUS: u64 = 0x2 | (1 << 9) | (1 << 10) | (1 << 21);

fn gprs(regs: &Registers) -> [u64; 32] {
    [
        regs.rax, regs.rcx, regs.rdx, regs.rbx, regs.rsp, regs.rbp, regs.rsi, regs.rdi,
        regs.r8, regs.r9, regs.r10, regs.r11, regs.r12, regs.r13, regs.r14, regs.r15,
        regs.r16, regs.r17, regs.r18, regs.r19, regs.r20, regs.r21, regs.r22, regs.r23,
        regs.r24, regs.r25, regs.r26, regs.r27, regs.r28, regs.r29, regs.r30, regs.r31,
    ]
}

fn set_gpr(regs: &mut Registers, index: u8, value: u64) {
    match index {
        0 => regs.rax = value,
        1 => regs.rcx = value,
        2 => regs.rdx = value,
        3 => regs.rbx = value,
        4 => regs.rsp = value,
        5 => regs.rbp = value,
        6 => regs.rsi = value,
        7 => regs.rdi = value,
        8 => regs.r8 = value,
        9 => regs.r9 = value,
        10 => regs.r10 = value,
        11 => regs.r11 = value,
        12 => regs.r12 = value,
        13 => regs.r13 = value,
        14 => regs.r14 = value,
        15 => regs.r15 = value,
        16 => regs.r16 = value,
        17 => regs.r17 = value,
        18 => regs.r18 = value,
        19 => regs.r19 = value,
        20 => regs.r20 = value,
        21 => regs.r21 = value,
        22 => regs.r22 = value,
        23 => regs.r23 = value,
        24 => regs.r24 = value,
        25 => regs.r25 = value,
        26 => regs.r26 = value,
        27 => regs.r27 = value,
        28 => regs.r28 = value,
        29 => regs.r29 = value,
        30 => regs.r30 = value,
        31 => regs.r31 = value,
        _ => unreachable!(),
    }
}

fn random_encoding(destination: u8, width: u8, seed: bool) -> (Vec<u8>, bool) {
    let digit = if seed { 7 } else { 6 };
    let mut bytes = Vec::with_capacity(5);
    if width == 16 {
        bytes.push(0x66);
    }
    let apx = destination >= 16;
    if apx {
        let payload = 0x80
            | u8::from(width == 64) << 3
            | destination & 0x10
            | u8::from(destination & 8 != 0);
        bytes.extend([0xD5, payload, 0xC7]);
    } else {
        if width == 64 {
            bytes.push(0x48);
        }
        bytes.extend([0x0F, 0xC7]);
    }
    bytes.push(0xC0 | digit << 3 | destination & 7);
    (bytes, apx)
}

fn setup(vcpu: &mut X86_64Vcpu, apx: bool) -> Registers {
    let mut regs = vcpu.get_regs().unwrap();
    for index in 0u8..32 {
        set_gpr(
            &mut regs,
            index,
            0xA5A5_5A5A_C3C3_3C3Cu64.rotate_left(u32::from(index) * 7),
        );
    }
    regs.rflags = NON_STATUS | STATUS;
    let before = regs.clone();
    vcpu.set_regs(&regs).unwrap();
    vcpu.set_apx_enabled(apx);
    before
}

fn assert_random_invariants(
    name: &str,
    before: &Registers,
    after: &Registers,
    destination: u8,
    width: u8,
) {
    let before_gprs = gprs(before);
    let after_gprs = gprs(after);
    let success = after.rflags & 1 != 0;
    assert_eq!(after.rflags & STATUS, u64::from(success), "{name}: status flags");
    assert_eq!(after.rflags & !STATUS, NON_STATUS, "{name}: non-status flags");
    for index in 0usize..32 {
        if index != usize::from(destination) {
            assert_eq!(after_gprs[index], before_gprs[index], "{name}: GPR{index}");
        }
    }
    match width {
        16 => {
            assert_eq!(
                after_gprs[usize::from(destination)] >> 16,
                before_gprs[usize::from(destination)] >> 16,
                "{name}: 16-bit partial write"
            );
            if !success {
                assert_eq!(after_gprs[usize::from(destination)] & 0xFFFF, 0);
            }
        }
        32 => {
            assert_eq!(
                after_gprs[usize::from(destination)] >> 32,
                0,
                "{name}: 32-bit zero extension"
            );
            if !success {
                assert_eq!(after_gprs[usize::from(destination)], 0);
            }
        }
        64 => {
            if !success {
                assert_eq!(after_gprs[usize::from(destination)], 0);
            }
        }
        _ => unreachable!(),
    }
}

#[test]
fn jit_random_state_backed_boundaries_reach_native_tier() {
    let mut executed = 0usize;
    for seed in [false, true] {
        if (seed && !std::is_x86_feature_detected!("rdseed"))
            || (!seed && !std::is_x86_feature_detected!("rdrand"))
        {
            continue;
        }
        for destination in [4u8, 5, 16, 31] {
            for width in [16u8, 32, 64] {
                let name = format!(
                    "{} GPR{destination} W{width}",
                    if seed { "RDSEED" } else { "RDRAND" }
                );
                let (random, apx) = random_encoding(destination, width, seed);
                let mut code = random.clone();
                code.push(0xF4);

                let mut interp = make_vcpu_code(&code);
                let before = setup(&mut interp, apx);
                run_interp(&mut interp);
                let expected = interp.get_regs().unwrap();
                assert_random_invariants(&name, &before, &expected, destination, width);

                let mut jit = make_vcpu_code(&code);
                let jit_before = setup(&mut jit, apx);
                assert_eq!(gprs(&jit_before), gprs(&before));
                assert!(
                    jit.jit_try_block().expect("JIT state-backed random loop"),
                    "{name}: host-supported random source must enter native tier"
                );
                let actual = jit.get_regs().unwrap();
                assert_random_invariants(&name, &jit_before, &actual, destination, width);
                assert_eq!(
                    actual.rip,
                    LOAD_ADDR + random.len() as u64,
                    "{name}: HLT frontier"
                );
                executed += 1;
            }
        }
    }

    let supported_sources = usize::from(std::is_x86_feature_detected!("rdrand"))
        + usize::from(std::is_x86_feature_detected!("rdseed"));
    assert_eq!(executed, supported_sources * 4 * 3);
}
