//! SBPF SMIR planes: one guest step of SMIR at O0, O1, and O2 equals one
//! direct `EbpfCpu::step` on the same text and machine state.
//!
//! The direct step is proven equal to the Anza interpreter step by
//! [`super::check`] over the same symbolic domain ([`super::text`] and
//! [`super::machine`]), so these harnesses carry that equality to SMIR
//! without compiling the reference into the model. The lifter registers the
//! executable's functions ([`super::functions`], proven equal to the
//! reference registry) and both test syscalls; the embedder handles every
//! syscall exit as the oracle support code does.

use rax::isa::ebpf::{EbpfHelperKey, MM_BYTECODE_START, SbpfVersion, verify_sbpf};
use rax::smir::ir::context::ArchRegState;
use rax::smir::ir::types::{ArchReg, EbpfReg};
use rax::smir::lift::{LiftContext, SmirLifter};
use rax::smir::optimize::{OptLevel, optimize_function};
use rax::smir::{EbpfLifter, ExitReason, SmirContext, SmirInterpreter};

use super::{
    AT, After, CHARGE_KEY, Fault, Machine, SYSCALL_KEY, Step, charge_cost, charged, engine,
    engine_fault, engine_frames, engine_memory, functions, gapped, machine, mix, observed_frames,
    probed, rax_config, slot, text, upstream_version,
};

fn read_regs(context: &SmirContext) -> [u64; 11] {
    let mut regs = [0u64; 11];
    for (index, value) in regs.iter_mut().enumerate() {
        *value = context.read_arch_reg(ArchReg::Ebpf(EbpfReg::R(index as u8)));
    }
    regs
}

fn smir_step(
    version: SbpfVersion,
    text: &[u8],
    functions: &[(u32, usize)],
    gapped: bool,
    m: &Machine,
    level: OptLevel,
) -> After {
    let config = rax_config(version, text.len(), m.budget);
    let mut memory = engine_memory(version, text, gapped, m, &config);
    let entry = MM_BYTECODE_START + 8 * AT;

    let mut lifter = EbpfLifter::solana(version);
    for &(key, target) in functions {
        lifter.register_function(key, MM_BYTECODE_START + 8 * target as u64);
    }
    lifter.register_syscall(SYSCALL_KEY);
    lifter.register_syscall(CHARGE_KEY);
    let mut lift_context = LiftContext::new(lifter.source_arch());
    let lifted = lifter.lift_program(entry, &memory, &mut lift_context);
    assert!(lifted.is_ok(), "every admitted text lifts");
    let mut interpreter = SmirInterpreter::new();
    interpreter.set_max_insns(1);
    for mut function in lifted.unwrap() {
        if level != OptLevel::O0 {
            optimize_function(&mut function, level);
        }
        interpreter.add_function(function);
    }

    let mut context = SmirContext::new_ebpf_with_config(config);
    for (index, value) in m.regs.iter().enumerate() {
        context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(index as u8)), *value);
    }
    context.pc = entry;
    context.insn_count = m.used;
    let ArchRegState::Ebpf(state) = &mut context.arch_regs else {
        panic!("eBPF context");
    };
    state.call_frames = engine_frames(m);

    let step = match interpreter.run(&mut context, &mut memory) {
        // The one-instruction slice ends at the next guest boundary; a
        // register call's target that was not lifted yet ends it as well.
        ExitReason::InsnLimit | ExitReason::BlockNotFound { .. } => Step::Continue,
        ExitReason::Return { to } => Step::Exit(to),
        ExitReason::EbpfTrap(trap) => engine_fault(trap),
        // The embedder runs both test syscalls and charges the metered one
        // before resuming, as the oracle support code does.
        ExitReason::Syscall { num, args } => match EbpfHelperKey::from_smir(num) {
            EbpfHelperKey::Static(SYSCALL_KEY) => {
                let value = mix(args[0], args[1], args[2], args[3], args[4]);
                context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(0)), value);
                Step::Continue
            }
            EbpfHelperKey::Static(CHARGE_KEY) => {
                context.insn_count = charged(context.insn_count, m.budget, charge_cost(args[0]));
                let value = mix(args[0], args[1], args[2], args[3], args[4]);
                context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(0)), value);
                Step::Continue
            }
            _ => Step::Trap(Fault::Unsupported, slot(context.pc.wrapping_sub(8))),
        },
        _ => panic!("SMIR exit outside the compared classes"),
    };
    let ArchRegState::Ebpf(state) = &context.arch_regs else {
        panic!("eBPF context");
    };
    After {
        step,
        regs: read_regs(&context),
        next_slot: slot(context.pc),
        frames: observed_frames(&state.call_frames),
        remaining: m.budget.wrapping_sub(context.insn_count),
        bytes: probed(&memory, m.probe),
    }
}

pub(crate) fn check_smir(version: SbpfVersion, opcode: u8, levels: &[OptLevel]) {
    let text = text(opcode);
    // The reference verifier verdict equals `verify_sbpf` (`super::check`).
    if verify_sbpf(&text, version).is_err() {
        return;
    }
    let upstream = upstream_version(version);
    let functions = functions(upstream);
    let gapped = gapped(upstream);
    let m = machine();
    // The interpreter's one-instruction slice ends at `insn_count + 1`
    // (saturating); 2^64 - 1 executed instructions is not reachable.
    kani::assume(m.used < u64::MAX);
    let direct = engine(version, &text, &functions, gapped, &m);
    for &level in levels {
        let actual = smir_step(version, &text, &functions, gapped, &m, level);
        assert!(actual.step == direct.step, "step result");
        assert!(
            actual.remaining == direct.remaining,
            "remaining compute units"
        );
        if !matches!(direct.step, Step::Trap(..)) {
            assert!(actual.regs == direct.regs, "registers");
        }
        if direct.step == Step::Continue {
            assert!(actual.next_slot == direct.next_slot, "next slot");
            assert!(actual.frames == direct.frames, "call frames");
        }
        assert!(actual.bytes == direct.bytes, "stack, heap, and input bytes");
    }
}
