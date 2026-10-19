//! Exhaustive SBPF V0-V4 differential against the pinned Anza interpreter.
//!
//! The case streams, rax planes, outcomes, and digests are shared with the
//! engine's `ebpf_diff` corpus replay (`tests/support/ebpf_sbpf_oracle.rs`).
//! Every candidate program first passes Anza's `RequisiteVerifier`; programs
//! the verifier rejects are never executed by a Solana runtime and are only
//! counted. Each verified program then runs on the upstream interpreter, the
//! direct `EbpfCpu`, and SMIR at O0/O1/O2. The comparison covers the result
//! (`r0` or the trap class), all eleven GPRs on success, the faulting
//! instruction slot on a trap, the executed instruction count (the compute
//! units the program consumed), and the final stack, heap, and input bytes.

use std::ptr::NonNull;
use std::sync::Arc;

#[path = "../../../tests/support/ebpf_sbpf_oracle.rs"]
pub mod shared;

pub use shared::*;

use rax::isa::ebpf::SbpfVersion;
use solana_sbpf::{
    aligned_memory::AlignedMemory,
    declare_builtin_function, ebpf,
    elf::Executable,
    error::EbpfError,
    memory_region::{MemoryMapping, MemoryRegion},
    program::{BuiltinProgram, FunctionRegistry, SBPFVersion},
    verifier::RequisiteVerifier,
    vm::{CallFrame, Config, ContextObject, EbpfVm, ExecutionMode},
};

pub struct Context {
    remaining: u64,
    mapping: MemoryMapping,
}

impl ContextObject for Context {
    fn consume(&mut self, amount: u64) {
        self.remaining = self.remaining.saturating_sub(amount);
    }
    fn get_remaining(&self) -> u64 {
        self.remaining
    }
    fn active_mapping_ptr(&mut self) -> NonNull<MemoryMapping> {
        NonNull::from(&mut self.mapping)
    }
}

declare_builtin_function!(
    /// Deterministic syscall shared by both implementations.
    RaxMix,
    fn rust(
        _context: &mut Context,
        a: u64,
        b: u64,
        c: u64,
        d: u64,
        e: u64,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        Ok(mix(a, b, c, d, e))
    }
);

declare_builtin_function!(
    /// Metered syscall: consumes `charge_cost(r1)` compute units through the
    /// reference context before returning `mix` of its arguments.
    RaxCharge,
    fn rust(
        context: &mut Context,
        a: u64,
        b: u64,
        c: u64,
        d: u64,
        e: u64,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        context.consume(charge_cost(a));
        Ok(mix(a, b, c, d, e))
    }
);

pub fn upstream_version(version: SbpfVersion) -> SBPFVersion {
    match version {
        SbpfVersion::V0 => SBPFVersion::V0,
        SbpfVersion::V1 => SBPFVersion::V1,
        SbpfVersion::V2 => SBPFVersion::V2,
        SbpfVersion::V3 => SBPFVersion::V3,
        SbpfVersion::V4 => SBPFVersion::V4,
    }
}

pub fn versions() -> [(SbpfVersion, SBPFVersion); 5] {
    VERSIONS.map(|version| (version, upstream_version(version)))
}

/// Why a candidate program was not prepared.
#[derive(Debug, Clone)]
pub enum Rejection {
    /// The upstream `RequisiteVerifier` (or loader) rejected the program; a
    /// Solana runtime never executes it.
    Upstream,
    /// Upstream accepted the program but SMIR could not lift it: a defect.
    Lift(String),
}

/// A verified program on the upstream interpreter and every rax plane.
pub struct Fixture {
    pub rax: RaxFixture,
    pub upstream: SBPFVersion,
    executable: Executable<Context>,
    loader: Arc<BuiltinProgram<Context>>,
}

fn upstream_config() -> Config {
    Config {
        max_call_depth: MAX_CALL_DEPTH,
        ..Config::default()
    }
}

impl Fixture {
    /// Verify `program` upstream and prepare every engine.
    pub fn new(
        version: SbpfVersion,
        upstream: SBPFVersion,
        program: &[u8],
        functions: &[(u32, usize)],
    ) -> Result<Self, Rejection> {
        let mut loader = BuiltinProgram::<Context>::new_loader(upstream_config());
        loader
            .register_definition::<RaxMix>("rax_mix")
            .map_err(|_| Rejection::Upstream)?;
        loader
            .register_definition::<RaxCharge>("rax_charge")
            .map_err(|_| Rejection::Upstream)?;
        let loader = Arc::new(loader);
        let mut registry = FunctionRegistry::default();
        for (index, &(key, slot)) in functions.iter().enumerate() {
            registry
                .register_function(key, format!("f{index}").into_bytes(), slot)
                .map_err(|_| Rejection::Upstream)?;
        }
        let executable = Executable::from_text_bytes(program, loader.clone(), upstream, registry)
            .map_err(|_| Rejection::Upstream)?;
        executable
            .verify::<RequisiteVerifier>()
            .map_err(|_| Rejection::Upstream)?;
        let rax = RaxFixture::new(version, program, functions).map_err(Rejection::Lift)?;
        Ok(Self {
            rax,
            upstream,
            executable,
            loader,
        })
    }

    pub fn anza(&self, state: &State) -> Outcome {
        let config = self.executable.get_config();
        let mut stack = AlignedMemory::<{ ebpf::HOST_ALIGN }>::zero_filled(config.stack_size());
        let stack_len = stack.len();
        let mut heap = AlignedMemory::<{ ebpf::HOST_ALIGN }>::from_slice(&state.heap);
        let mut input = AlignedMemory::<{ ebpf::HOST_ALIGN }>::from_slice(&state.input);
        let stack_region = if self.upstream.stack_frame_gaps() && config.enable_stack_frame_gaps {
            MemoryRegion::new_gapped(
                &mut stack,
                ebpf::MM_STACK_START,
                config.stack_frame_size as u64,
            )
        } else {
            MemoryRegion::new(&mut stack, ebpf::MM_STACK_START)
        };
        let regions = vec![
            self.executable.get_ro_region(),
            stack_region,
            MemoryRegion::new(&mut heap, ebpf::MM_HEAP_START),
            MemoryRegion::new(&mut input, ebpf::MM_INPUT_START),
        ];
        // SAFETY: the executable, stack, heap, and input allocations outlive
        // the mapping and VM, which are dropped at the end of this block before
        // the host reads the buffers. Writable regions are exclusively owned
        // plain bytes; they do not overlap or alias. Execution is synchronous
        // and interpreted, and the only callback (RaxMix) touches no guest
        // memory, so no host access or re-entry overlaps guest execution.
        let mapping = unsafe { MemoryMapping::new(regions, config, self.upstream) }
            .expect("valid upstream regions");
        let (instructions, result, trace_regs, error_pc) = {
            let mut context = Context {
                remaining: state.budget,
                mapping,
            };
            let mut vm = EbpfVm::new(self.loader.clone(), self.upstream, &mut context, stack_len);
            vm.registers[..10].copy_from_slice(&state.regs);
            let mut frames = vec![CallFrame::default(); config.max_call_depth];
            let (instructions, result) = vm.execute_program(
                &self.executable,
                &mut ExecutionMode::Interpreted,
                &mut frames,
            );
            let result: Result<u64, EbpfError> = result.into();
            let trace_regs = vm.register_trace.last().copied();
            (instructions, result, trace_regs, vm.registers[11])
        };
        let result = match result {
            Ok(r0) => {
                // The interpreted register buffer is stale after EXIT; the
                // tracer snapshot taken before the root EXIT holds final GPRs.
                let snapshot = trace_regs.expect("traced EXIT");
                assert_eq!(snapshot[0], r0);
                assert_eq!(self.rax.program[snapshot[11] as usize * 8], 0x95);
                Ok((r0, snapshot[..11].try_into().expect("eleven GPRs")))
            }
            Err(error) => Err((upstream_trap(&error), error_pc)),
        };
        Outcome {
            result,
            instructions,
            stack: stack.as_slice().to_vec(),
            heap: heap.as_slice().to_vec(),
            input: input.as_slice().to_vec(),
        }
    }

    /// Lift and print the program's SMIR at one optimization level.
    pub fn dump(&self, level: rax::smir::optimize::OptLevel) {
        use rax::isa::ebpf::MM_BYTECODE_START;
        use rax::smir::lift::{LiftContext, SmirLifter};
        let version = self.rax.version;
        let program = &self.rax.program;
        let config = rax_config(version, program.len(), u64::MAX);
        let memory = mapping(&config, program, &[0; HEAP_LEN], &[0; INPUT_LEN]);
        let mut lifter = rax::smir::EbpfLifter::solana(version);
        for &(key, slot) in &self.rax.functions {
            lifter.register_function(key, MM_BYTECODE_START + 8 * slot as u64);
        }
        let mut context = LiftContext::new(lifter.source_arch());
        for mut function in lifter
            .lift_program(MM_BYTECODE_START, &memory, &mut context)
            .expect("lift")
        {
            rax::smir::optimize::optimize_function(&mut function, level);
            eprintln!("== {level:?} function {:#x?}", function.guest_range);
            for block in &function.blocks {
                eprintln!("  block {:?} pc={:#x}", block.id, block.guest_pc);
                for op in &block.ops {
                    eprintln!("    {:#x}: {:?}", op.guest_pc, op.kind);
                }
                eprintln!("    -> {:?}", block.terminator);
            }
        }
    }
}

fn upstream_trap(error: &EbpfError) -> Trap {
    use solana_sbpf::memory_region::AccessType;
    match error {
        EbpfError::DivideByZero => Trap::DivideByZero,
        EbpfError::DivideOverflow => Trap::DivideOverflow,
        EbpfError::AccessViolation(access, addr, len, _)
        | EbpfError::StackAccessViolation(access, addr, len, _) => Trap::Memory {
            write: matches!(access, AccessType::Store),
            addr: *addr,
            len: *len,
        },
        EbpfError::CallDepthExceeded => Trap::CallDepthExceeded,
        EbpfError::CallOutsideTextSegment => Trap::CallOutsideTextSegment,
        EbpfError::ExecutionOverrun => Trap::ExecutionOverrun,
        EbpfError::ExceededMaxInstructions => Trap::ExceededMaxInstructions,
        EbpfError::UnsupportedInstruction | EbpfError::InvalidInstruction => Trap::Unsupported,
        other => Trap::Other(format!("{other:?}")),
    }
}

fn kind(result: &Result<(u64, [u64; 11]), (Trap, u64)>) -> String {
    match result {
        Ok(_) => "exit".to_string(),
        Err((Trap::Memory { write, .. }, _)) => format!("memory-{write}"),
        Err((Trap::Other(text), _)) => {
            format!("other:{}", text.split(['(', ' ', '{']).next().unwrap_or(""))
        }
        Err((trap, _)) => format!("{trap:?}"),
    }
}

/// Short classification of a difference, for grouping mismatches.
pub fn signature(plane: &str, expected: &Outcome, actual: &Outcome) -> String {
    let field = if kind(&expected.result) != kind(&actual.result) {
        "kind"
    } else if expected.result != actual.result {
        "result"
    } else if expected.instructions != actual.instructions {
        "instructions"
    } else if expected.stack != actual.stack {
        "stack"
    } else if expected.heap != actual.heap {
        "heap"
    } else {
        "input"
    };
    format!(
        "{plane}: {field} anza={} rax={}",
        kind(&expected.result),
        kind(&actual.result)
    )
}

fn summarize(outcome: &Outcome) -> String {
    let mut digest = Digest::default();
    digest.update(&outcome.bytes());
    format!(
        "{:x?} insns={} outcome#{}",
        outcome.result,
        outcome.instructions,
        digest.hex()
    )
}

/// Compare one state on every plane; returns `(signature, diagnostic)` for
/// the first differing plane, or the upstream outcome.
pub fn compare(fixture: &Fixture, state: &State) -> Result<Outcome, (String, String)> {
    let expected = fixture.anza(state);
    for (plane, actual) in fixture.rax.planes(state) {
        if actual != expected {
            return Err((
                format!(
                    "{:?} {}",
                    fixture.rax.version,
                    signature(plane, &expected, &actual)
                ),
                format!(
                    "{:?} {plane}: program={} functions={:?} budget={} regs={:#x?}\n  heap={}\n  input={}\n  anza={}\n  rax ={}",
                    fixture.rax.version,
                    hex(&fixture.rax.program),
                    fixture.rax.functions,
                    state.budget,
                    state.regs,
                    hex(&state.heap),
                    hex(&state.input),
                    summarize(&expected),
                    summarize(&actual)
                ),
            ));
        }
    }
    Ok(expected)
}
