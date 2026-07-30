//! Fault-precise original-VEX CMPccXADD JIT transaction helper.

use super::X86_64Vcpu;
use crate::isa::x86_64::flags;
use crate::smir::lower::runtime::GuestRegs;

/// Execute one complete original-VEX CMPccXADD transaction against ordinary
/// guest RAM. A zero result requests direct replay before any architectural
/// register, flag, memory, verification-log, or access-trace commit.
pub(super) unsafe extern "C" fn rax_jit_cmpccxadd(
    state: *mut GuestRegs,
    address: u64,
    cmp_register: u32,
    add_register: u32,
    size: u32,
    condition_code: u32,
) -> u64 {
    let Some(state) = (unsafe { state.as_mut() }) else {
        return 0;
    };
    if cmp_register >= 16 || add_register >= 16 || !matches!(size, 4 | 8) || condition_code >= 16 {
        return 0;
    }
    let Some(vcpu) = (unsafe { (state.ctx as *mut X86_64Vcpu).as_mut() }) else {
        return 0;
    };

    const CR0_AM: u64 = 1 << 18;
    if address & u64::from(size - 1) != 0
        && state.cr0 & CR0_AM != 0
        && state.ac_flag != 0
        && state.cpl == 3
    {
        // Direct replay performs complete-range canonicality validation before
        // selecting #AC, preserving #SS/#GP priority for noncanonical ranges.
        return 0;
    }

    let Some(last) = address.checked_add(u64::from(size) - 1) else {
        return 0;
    };
    if vcpu.mmu.is_code_page(address)
        || vcpu.mmu.is_code_page(last)
        || !vcpu
            .mmu
            .read_range_is_plain_ram(address, size as usize, &vcpu.sregs)
        || !vcpu
            .mmu
            .write_range_is_plain_ram(address, size as usize, &vcpu.sregs)
    {
        return 0;
    }

    let mask = if size == 4 {
        u64::from(u32::MAX)
    } else {
        u64::MAX
    };
    let cmp = state.gpr[cmp_register as usize] & mask;
    let add = state.gpr[add_register as usize] & mask;
    // Stage trace publication with the transaction. This also keeps an
    // unexpected post-preflight store failure non-observable to direct replay.
    let staged_trace = vcpu.jit_mem_trace.take();
    let old = match vcpu.read_mem(address, size as u8) {
        Ok(old) => old,
        Err(_) => {
            vcpu.jit_mem_trace = staged_trace;
            return 0;
        }
    };
    let old = old & mask;
    let mut candidate_rflags = state.rflags;
    flags::update_flags_sub(
        &mut candidate_rflags,
        old,
        cmp,
        old.wrapping_sub(cmp) & mask,
        size as u8,
    );
    let new = if flags::condition_holds(candidate_rflags, condition_code as u8) {
        old.wrapping_add(add) & mask
    } else {
        old
    };

    // Both outcomes perform the architecturally visible locked write-back.
    if vcpu.write_mem(address, new, size as u8).is_err() {
        vcpu.jit_mem_trace = staged_trace;
        return 0;
    }
    vcpu.jit_mem_trace = staged_trace;
    vcpu.push_jit_mem_trace((0, address, size as u8, old));
    vcpu.push_jit_mem_trace((1, address, size as u8, new));
    if vcpu.jit_mem_log_active() {
        vcpu.push_jit_mem_log((address, size as u8, old));
    }
    state.gpr[cmp_register as usize] = old;
    state.rflags = candidate_rflags;
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};

    const ADDRESS: u64 = 0x2000;

    fn vcpu_and_state() -> (X86_64Vcpu, GuestRegs, Arc<GuestMemoryMmap>) {
        let memory =
            Arc::new(GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(0), 0x10000)]).unwrap());
        let mut vcpu = X86_64Vcpu::new(0, memory.clone());
        vcpu.sregs.efer = 1 << 10;
        vcpu.sregs.cs.l = true;
        vcpu.jit_mem_trace = Some(Vec::new());
        vcpu.jit_mem_log = Some(Vec::new());
        let state = GuestRegs {
            rflags: 0x2 | flags::bits::DF,
            ..GuestRegs::default()
        };
        (vcpu, state, memory)
    }

    fn bind_state(vcpu: &mut X86_64Vcpu, state: &mut GuestRegs) {
        state.ctx = (vcpu as *mut X86_64Vcpu) as u64;
    }

    fn condition_operands(condition_code: u8, truth: bool, width: u32) -> (u64, u64) {
        let minimum = if width == 4 {
            u64::from(1_u32 << 31)
        } else {
            1_u64 << 63
        };
        let (true_pair, false_pair) = match condition_code {
            0x0 => ((minimum, 1), (0, 0)),
            0x1 => ((0, 0), (minimum, 1)),
            0x2 => ((0, 1), (1, 0)),
            0x3 => ((1, 0), (0, 1)),
            0x4 => ((1, 1), (1, 0)),
            0x5 => ((1, 0), (1, 1)),
            0x6 => ((0, 1), (2, 1)),
            0x7 => ((2, 1), (1, 1)),
            0x8 => ((0, 1), (1, 0)),
            0x9 => ((1, 0), (0, 1)),
            0xA => ((1, 1), (2, 1)),
            0xB => ((2, 1), (1, 1)),
            0xC => ((minimum, 1), (0, 0)),
            0xD => ((0, 0), (minimum, 1)),
            0xE => ((0, 0), (2, 1)),
            0xF => ((2, 1), (0, 0)),
            _ => unreachable!("four-bit condition code"),
        };
        if truth { true_pair } else { false_pair }
    }

    #[test]
    fn helper_executes_all_conditions_widths_and_both_writeback_outcomes() {
        let mut cases = 0usize;
        for condition_code in 0..16 {
            for size in [4, 8] {
                for truth in [false, true] {
                    let (mut vcpu, mut state, memory) = vcpu_and_state();
                    bind_state(&mut vcpu, &mut state);
                    let (old, cmp) = condition_operands(condition_code, truth, size);
                    let add = 7_u64;
                    memory
                        .write_slice(&old.to_le_bytes()[..size as usize], GuestAddress(ADDRESS))
                        .unwrap();
                    state.gpr[1] = cmp;
                    state.gpr[2] = add;

                    assert_eq!(
                        unsafe {
                            rax_jit_cmpccxadd(
                                &mut state,
                                ADDRESS,
                                1,
                                2,
                                size,
                                u32::from(condition_code),
                            )
                        },
                        1
                    );
                    let mask = if size == 4 {
                        u64::from(u32::MAX)
                    } else {
                        u64::MAX
                    };
                    let expected = if truth {
                        old.wrapping_add(add) & mask
                    } else {
                        old & mask
                    };
                    let mut stored = [0u8; 8];
                    memory
                        .read_slice(&mut stored[..size as usize], GuestAddress(ADDRESS))
                        .unwrap();
                    assert_eq!(u64::from_le_bytes(stored), expected);
                    assert_eq!(state.gpr[1], old & mask);
                    assert_eq!(
                        vcpu.jit_mem_trace.as_deref(),
                        Some(
                            &[
                                (0, ADDRESS, size as u8, old & mask),
                                (1, ADDRESS, size as u8, expected),
                            ][..]
                        )
                    );
                    assert_eq!(
                        vcpu.jit_mem_log.as_deref(),
                        Some(&[(ADDRESS, size as u8, old & mask)][..])
                    );
                    assert_eq!(
                        flags::condition_holds(state.rflags, condition_code as u8),
                        truth
                    );
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 16 * 2 * 2);
    }

    #[test]
    fn helper_snapshots_aliases_and_zero_extends_the_w32_destination() {
        let (mut vcpu, mut state, memory) = vcpu_and_state();
        bind_state(&mut vcpu, &mut state);
        let old = 0x0000_0000_0000_0003_u64;
        memory
            .write_slice(&old.to_le_bytes()[..4], GuestAddress(ADDRESS))
            .unwrap();
        state.gpr[9] = 0xFFFF_FFFF_0000_0001;

        assert_eq!(
            unsafe { rax_jit_cmpccxadd(&mut state, ADDRESS, 9, 9, 4, 5) },
            1
        );
        assert_eq!(state.gpr[9], old);
        assert_eq!(
            memory.read_obj::<u32>(GuestAddress(ADDRESS)).unwrap(),
            4,
            "aliased addend must be snapshotted before old-value writeback"
        );
        assert_eq!(vcpu.jit_mem_log.as_deref(), Some(&[(ADDRESS, 4, old)][..]));
    }

    #[test]
    fn malformed_alignment_code_and_range_guards_are_noncommitting() {
        for (name, address, configure, arguments) in [
            (
                "alignment check",
                ADDRESS + 1,
                1u8,
                (1_u32, 2_u32, 4_u32, 4_u32),
            ),
            ("code page", ADDRESS, 2, (1_u32, 2_u32, 4_u32, 4_u32)),
            ("unmapped range", 0x20_000, 0, (1_u32, 2_u32, 4_u32, 4_u32)),
            (
                "invalid register",
                ADDRESS,
                0,
                (16_u32, 2_u32, 4_u32, 4_u32),
            ),
            ("invalid width", ADDRESS, 0, (1_u32, 2_u32, 2_u32, 4_u32)),
            (
                "invalid condition",
                ADDRESS,
                0,
                (1_u32, 2_u32, 4_u32, 16_u32),
            ),
        ] {
            let (mut vcpu, mut state, memory) = vcpu_and_state();
            bind_state(&mut vcpu, &mut state);
            memory
                .write_obj(0xA5A5_5A5A_u32, GuestAddress(ADDRESS))
                .unwrap();
            state.gpr[1] = 0x1122_3344_5566_7788;
            state.gpr[2] = 0x99AA_BBCC_DDEE_FF00;
            if configure == 1 {
                state.cr0 |= 1 << 18;
                state.ac_flag = 1;
                state.cpl = 3;
            } else if configure == 2 {
                vcpu.mmu.mark_code_page(ADDRESS);
            }
            let state_before = state;
            let memory_before = memory.read_obj::<u32>(GuestAddress(ADDRESS)).unwrap();
            let trace_before = vcpu.jit_mem_trace.clone();
            let log_before = vcpu.jit_mem_log.clone();
            let (cmp, add, size, cc) = arguments;

            assert_eq!(
                unsafe { rax_jit_cmpccxadd(&mut state, address, cmp, add, size, cc) },
                0,
                "{name}"
            );
            assert_eq!(state, state_before, "{name}: state");
            assert_eq!(
                memory.read_obj::<u32>(GuestAddress(ADDRESS)).unwrap(),
                memory_before,
                "{name}: memory"
            );
            assert_eq!(vcpu.jit_mem_trace, trace_before, "{name}: trace");
            assert_eq!(vcpu.jit_mem_log, log_before, "{name}: undo log");
        }
    }
}
