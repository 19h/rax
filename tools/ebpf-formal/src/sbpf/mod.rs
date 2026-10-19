//! Solana SBPF V0-V4 proofs: the engine against the pinned Anza `solana-sbpf`
//! 0.25.0 interpreter and `RequisiteVerifier` (`docs/specifications/ebpf/sbpf/`),
//! executed as Rust code under the model checker.
//!
//! Each harness fixes the SBPF version and the opcode byte of the
//! instruction at slot 1 of the text `EXIT; insn; next; EXIT`, where `next`
//! is a symbolic LDDW continuation for opcode `0x18` and `EXIT` otherwise.
//! Every other instruction bit, the eleven registers, the call depth and
//! every saved frame, the compute meter, and the stack, heap, and input
//! bytes are symbolic. A harness proves:
//!
//! 1. `verify_sbpf` returns exactly the reference verdict (error variant and
//!    slot) for the text;
//! 2. for an admitted text, one `EbpfCpu::step` equals one Anza
//!    `Interpreter::step`: the same continuation (registers, PC, call frames),
//!    program exit (`r0` and registers), or trap (class and faulting slot),
//!    with equal remaining compute units and equal stack, heap, and input.
//!
//! The configuration is the one the oracle campaigns use
//! (`tests/support/ebpf_sbpf_oracle.rs`): four 4 KiB frames, the reference
//! default unaligned region mapping, and the two test syscalls. The embedder
//! of the engine charges `rax_charge` exactly as that support code does.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;

use rax::isa::ebpf::{
    AtomicKind, EbpfCallFrame, EbpfConfig, EbpfCpu, EbpfExit, EbpfMemFault, EbpfMemWidth,
    EbpfMemory, EbpfTrap, MM_BYTECODE_START, MM_HEAP_START, MM_INPUT_START, MM_STACK_START,
    SbpfMemoryMapping, SbpfVerifierError, SbpfVersion, verify_sbpf,
};
use rax::smir::ir::types::Endian;
use solana_sbpf::{
    aligned_memory::AlignedMemory,
    declare_builtin_function, ebpf,
    elf::Executable,
    error::{EbpfError, ProgramResult},
    interpreter::Interpreter,
    memory_region::{
        AccessType, AccessViolationHandler, MemoryMapping, MemoryRegion,
        default_access_violation_handler,
    },
    program::{BuiltinProgram, FunctionRegistry, SBPFVersion},
    verifier::{RequisiteVerifier, Verifier, VerifierError},
    vm::{CallFrame, Config, ContextObject, EbpfVm},
};

#[path = "../../../../tests/support/ebpf_sbpf_oracle.rs"]
mod shared;

mod opcodes;
mod smir;

use shared::{
    CHARGE_KEY, FRAME_SIZE, MAX_CALL_DEPTH, SYSCALL_KEY, charge_cost, charged, mix, rax_config,
    rodata_address,
};

/// `rand::thread_rng` for the reference `FunctionRegistry` hasher seed.
///
/// The registry is a `HashMap` keyed by already-hashed `u32`s whose
/// `BuildHasher` draws one multiplier from `thread_rng`; lookups do not
/// depend on its value. Kani models neither the OS entropy nor the
/// thread-local initialization behind `thread_rng`, so this returns a
/// generator whose state is zero: its first `next_u64` reads two zero words
/// of its buffer (index 0 of 64) without generating or reseeding.
pub(crate) fn zeroed_thread_rng() -> rand::rngs::ThreadRng {
    type Generator =
        rand::rngs::adapter::ReseedingRng<rand_chacha::ChaCha12Core, rand::rngs::OsRng>;
    let generator = Rc::new(UnsafeCell::new(MaybeUninit::<Generator>::zeroed()));
    // SAFETY: rand 0.8.8 `ThreadRng` is one `Rc<UnsafeCell<Generator>>` field
    // (src/rngs/thread.rs), and `MaybeUninit<Generator>` has the layout of
    // `Generator`. Every field of `Generator` is an integer, an integer
    // array, or the zero-sized `OsRng`, so the zeroed value is valid, and no
    // field has drop glue.
    unsafe {
        std::mem::transmute::<Rc<UnsafeCell<MaybeUninit<Generator>>>, rand::rngs::ThreadRng>(
            generator,
        )
    }
}

/// The text both implementations fetch from during a step.
///
/// CBMC does not propagate constants through heap objects, so a text held
/// only in the reference `Executable` (an `AlignedMemory`) and the engine's
/// mapping (a `Vec`) makes every dispatch symbolic and each harness explore
/// the whole instruction set. The execution harnesses therefore serve the
/// fetch from this static array: the reference through
/// [`static_text_bytes`], a stub of `Executable::get_text_bytes`, which
/// returns exactly the text `from_text_bytes` stored (`elf.rs`
/// `get_text_bytes`: the text section of the read-only section, at
/// `MM_BYTECODE_START`); the engine through [`StaticFetch`], whose fetch
/// [`fetch_matches_the_mapping`] proves equal to `SbpfMemoryMapping`'s.
static mut TEXT: [u8; 8 * SLOTS] = [0; 8 * SLOTS];

pub(crate) fn static_text_bytes<C: ContextObject>(_executable: &Executable<C>) -> (u64, &[u8]) {
    // SAFETY: Kani executes a harness on one thread; the text is written once
    // before the step and only read afterwards.
    (ebpf::MM_BYTECODE_START, unsafe { &*(&raw const TEXT) })
}

fn install_text(text: &[u8; 8 * SLOTS]) {
    // SAFETY: as in `static_text_bytes`; no reference to TEXT is live here.
    unsafe { *(&raw mut TEXT) = *text };
}

/// The engine's memory with instruction fetches served from [`TEXT`];
/// data accesses go to the segmented mapping.
struct StaticFetch<'a> {
    mapping: &'a mut SbpfMemoryMapping,
}

impl EbpfMemory for StaticFetch<'_> {
    /// `SbpfMemoryMapping::executable_slice` for the text at
    /// `MM_BYTECODE_START`.
    fn fetch_bytes(&mut self, addr: u64, buf: &mut [u8]) -> Result<(), EbpfMemFault> {
        let fault = EbpfMemFault {
            addr,
            size: buf.len(),
            write: false,
        };
        let offset = addr.checked_sub(MM_BYTECODE_START).ok_or(fault)?;
        let end = offset.checked_add(buf.len() as u64).ok_or(fault)?;
        if end > (8 * SLOTS) as u64 {
            return Err(fault);
        }
        // SAFETY: as in `static_text_bytes`.
        let text = unsafe { &*(&raw const TEXT) };
        buf.copy_from_slice(&text[offset as usize..end as usize]);
        Ok(())
    }

    fn atomic(
        &mut self,
        addr: u64,
        width: EbpfMemWidth,
        endian: Endian,
        kind: AtomicKind,
        operand: u64,
        expected: u64,
    ) -> Result<u64, EbpfMemFault> {
        self.mapping
            .atomic(addr, width, endian, kind, operand, expected)
    }

    fn read_bytes(&mut self, addr: u64, buf: &mut [u8]) -> Result<(), EbpfMemFault> {
        self.mapping.read_bytes(addr, buf)
    }

    fn write_bytes(&mut self, addr: u64, data: &[u8]) -> Result<(), EbpfMemFault> {
        self.mapping.write_bytes(addr, data)
    }
}

/// [`StaticFetch`] fetches what `SbpfMemoryMapping` fetches, for every
/// version, address, fetch width, and text. Fetches read only the text, so
/// the data regions are empty here.
#[kani::proof]
#[kani::unwind(17)]
fn fetch_matches_the_mapping() {
    let text: [u8; 8 * SLOTS] = kani::any();
    install_text(&text);
    let version = match kani::any::<u8>() % 5 {
        0 => SbpfVersion::V0,
        1 => SbpfVersion::V1,
        2 => SbpfVersion::V2,
        3 => SbpfVersion::V3,
        _ => SbpfVersion::V4,
    };
    let config = rax_config(version, text.len(), u64::MAX);
    let mut mapping = SbpfMemoryMapping::new_solana_with_config(
        &config,
        &text,
        rodata_address(version),
        &text,
        0,
        0,
        &[],
    )
    .unwrap_or_else(|_| panic!("valid engine layout"));
    let addr: u64 = kani::any();
    let len = if kani::any() { 16 } else { 8 };
    let mut expected = [0u8; 16];
    let mut actual = [0u8; 16];
    let reference = mapping.fetch_bytes(addr, &mut expected[..len]);
    let mut fetch = StaticFetch {
        mapping: &mut mapping,
    };
    assert!(fetch.fetch_bytes(addr, &mut actual[..len]) == reference);
    assert!(expected == actual);
}

/// Slots of the text; the instruction under proof is at [`AT`].
const SLOTS: usize = 4;
const AT: u64 = 1;
/// Stack backing: every frame of the configured call depth.
const STACK: usize = MAX_CALL_DEPTH * FRAME_SIZE as usize;
const HEAP: usize = 16;
const INPUT: usize = 16;
/// Program-local functions of the V0-V2 registry (key, slot): both `EXIT`s.
const FUNCTIONS: [(u32, usize); 2] = [(0x5eed_0000, 0), (0x5eed_0003, 3)];

/// The executable's function registry: [`FUNCTIONS`] and the `entrypoint`
/// that `Executable::from_text_bytes` registers at slot 0, keyed by its
/// symbol hash in V0-V2 and by its slot with static syscalls. The reference
/// harness proves this equals the executable's registry, so the SMIR
/// harnesses use it without constructing an executable.
fn functions(version: SBPFVersion) -> [(u32, usize); 3] {
    let entrypoint = if version.static_syscalls() {
        0
    } else {
        ebpf::hash_symbol_name(b"entrypoint")
    };
    [FUNCTIONS[0], FUNCTIONS[1], (entrypoint, 0)]
}

struct Context {
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
    /// The oracle's zero-cost syscall.
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
    /// The oracle's metered syscall.
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

fn upstream_version(version: SbpfVersion) -> SBPFVersion {
    match version {
        SbpfVersion::V0 => SBPFVersion::V0,
        SbpfVersion::V1 => SBPFVersion::V1,
        SbpfVersion::V2 => SBPFVersion::V2,
        SbpfVersion::V3 => SBPFVersion::V3,
        SbpfVersion::V4 => SBPFVersion::V4,
    }
}

/// The reference verdict in the engine's error type; an error with no
/// engine counterpart fails the proof.
fn upstream_verdict(version: SBPFVersion, text: &[u8]) -> Result<(), SbpfVerifierError> {
    use SbpfVerifierError as E;
    use VerifierError as U;
    let registry = FunctionRegistry::<usize>::default();
    RequisiteVerifier::verify(text, version, &registry).map_err(|error| match error {
        U::ProgramLengthNotMultiple => E::ProgramLengthNotMultiple,
        U::NoProgram => E::NoProgram,
        U::DivisionByZero(i) => E::DivisionByZero(i),
        U::UnsupportedLEBEArgument(i) => E::UnsupportedLeBeArgument(i),
        U::LDDWCannotBeLast => E::LddwCannotBeLast,
        U::IncompleteLDDW(i) => E::IncompleteLddw(i),
        U::JumpOutOfCode(t, i) => E::JumpOutOfCode(t, i),
        U::JumpToMiddleOfLDDW(t, i) => E::JumpToMiddleOfLddw(t, i),
        U::InvalidSourceRegister(i) => E::InvalidSourceRegister(i),
        U::CannotWriteR10(i) => E::CannotWriteR10(i),
        U::InvalidDestinationRegister(i) => E::InvalidDestinationRegister(i),
        U::UnknownOpCode(opc, i) => E::UnknownOpCode(opc, i),
        U::ShiftWithOverflow(amount, bits, i) => E::ShiftWithOverflow(amount, bits, i),
        U::InvalidRegister(i) => E::InvalidRegister(i),
        U::UnalignedImmediate(i) => E::UnalignedImmediate(i),
        _ => panic!("reference verifier error without an engine counterpart"),
    })
}

/// An observable trap class, as the oracle campaigns compare it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    DivideByZero,
    DivideOverflow,
    Memory {
        write: bool,
        addr: u64,
        len: u64,
    },
    CallDepthExceeded,
    CallOutsideTextSegment,
    ExecutionOverrun,
    ExceededMaxInstructions,
    /// Unknown, malformed, or unresolvable instruction.
    Unsupported,
}

/// The result of one step; a trap names the faulting slot.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Continue,
    Exit(u64),
    Trap(Fault, u64),
}

fn upstream_fault(error: &EbpfError) -> Fault {
    match error {
        EbpfError::DivideByZero => Fault::DivideByZero,
        EbpfError::DivideOverflow => Fault::DivideOverflow,
        EbpfError::AccessViolation(access, addr, len, _)
        | EbpfError::StackAccessViolation(access, addr, len, _) => Fault::Memory {
            write: matches!(access, AccessType::Store),
            addr: *addr,
            len: *len,
        },
        EbpfError::CallDepthExceeded => Fault::CallDepthExceeded,
        EbpfError::CallOutsideTextSegment => Fault::CallOutsideTextSegment,
        EbpfError::ExecutionOverrun => Fault::ExecutionOverrun,
        EbpfError::ExceededMaxInstructions => Fault::ExceededMaxInstructions,
        EbpfError::UnsupportedInstruction | EbpfError::InvalidInstruction => Fault::Unsupported,
        _ => panic!("reference error outside the compared classes"),
    }
}

fn slot(pc: u64) -> u64 {
    pc.wrapping_sub(MM_BYTECODE_START) / 8
}

fn engine_fault(trap: EbpfTrap) -> Step {
    let (fault, pc) = match trap {
        EbpfTrap::DecodeError { pc, .. } => (Fault::Unsupported, pc),
        EbpfTrap::DivideByZero { pc } => (Fault::DivideByZero, pc),
        EbpfTrap::DivideOverflow { pc } => (Fault::DivideOverflow, pc),
        EbpfTrap::MemoryFault { pc, fault } => (
            Fault::Memory {
                write: fault.write,
                addr: fault.addr,
                len: fault.size as u64,
            },
            pc,
        ),
        EbpfTrap::CallDepthExceeded { pc } => (Fault::CallDepthExceeded, pc),
        // A fetch outside the text is the reference ExecutionOverrun; a call
        // or jump target names a different address.
        EbpfTrap::CallOutsideTextSegment { pc, target_pc } if pc == target_pc => {
            (Fault::ExecutionOverrun, pc)
        }
        EbpfTrap::CallOutsideTextSegment { pc, .. } => (Fault::CallOutsideTextSegment, pc),
        EbpfTrap::UnsupportedSyscall { pc, .. } => (Fault::Unsupported, pc),
        EbpfTrap::ExceededMaxInstructions { pc } => (Fault::ExceededMaxInstructions, pc),
        _ => panic!("engine trap outside the compared classes"),
    };
    Step::Trap(fault, slot(pc))
}

fn text(opcode: u8) -> [u8; 8 * SLOTS] {
    let exit = [ebpf::EXIT, 0, 0, 0, 0, 0, 0, 0];
    let mut insn: [u8; 8] = kani::any();
    insn[0] = opcode;
    let next: [u8; 8] = if opcode == ebpf::LD_DW_IMM {
        kani::any()
    } else {
        exit
    };
    let mut text = [0u8; 8 * SLOTS];
    text[..8].copy_from_slice(&exit);
    text[8..16].copy_from_slice(&insn);
    text[16..24].copy_from_slice(&next);
    text[24..].copy_from_slice(&exit);
    text
}

/// A saved frame: caller-saved `r6..r9`, frame pointer, and return slot.
#[derive(Clone, Copy, PartialEq, Eq, kani::Arbitrary)]
struct Frame {
    saved: [u64; 4],
    frame_pointer: u64,
    return_slot: u64,
}

/// The state both implementations start from.
struct Machine {
    regs: [u64; 11],
    depth: usize,
    frames: [Frame; MAX_CALL_DEPTH],
    /// Compute-unit budget and units used so far (`used <= budget`).
    budget: u64,
    used: u64,
    stack: [u8; STACK],
    heap: [u8; HEAP],
    input: [u8; INPUT],
    /// One symbolic byte index per region: comparing the bytes there after
    /// the step compares every byte, without copying the regions.
    probe: Probe,
}

#[derive(Clone, Copy)]
struct Probe {
    stack: usize,
    heap: usize,
    input: usize,
}

impl Probe {
    fn any() -> Self {
        let probe = Self {
            stack: kani::any(),
            heap: kani::any(),
            input: kani::any(),
        };
        kani::assume(probe.stack < STACK && probe.heap < HEAP && probe.input < INPUT);
        probe
    }
}

/// The probed byte of each region after a step.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Bytes {
    stack: u8,
    heap: u8,
    input: u8,
}

fn machine() -> Machine {
    let depth: usize = kani::any();
    kani::assume(depth < MAX_CALL_DEPTH);
    let frames: [Frame; MAX_CALL_DEPTH] = kani::any();
    // A return slot is the slot after a call, so it lies in the text.
    for frame in &frames {
        kani::assume(frame.return_slot <= SLOTS as u64);
    }
    let budget: u64 = kani::any();
    let used: u64 = kani::any();
    kani::assume(used <= budget);
    Machine {
        regs: kani::any(),
        depth,
        frames,
        budget,
        used,
        stack: kani::any(),
        heap: kani::any(),
        input: kani::any(),
        probe: Probe::any(),
    }
}

/// The observable state after one step.
struct After {
    step: Step,
    regs: [u64; 11],
    /// Slot of the next instruction (on continuation).
    next_slot: u64,
    frames: Vec<(Frame, u64)>,
    remaining: u64,
    bytes: Bytes,
}

/// Whether the stack has V0 virtual frame gaps in the reference layout.
fn gapped(version: SBPFVersion) -> bool {
    version.stack_frame_gaps() && upstream_config().enable_stack_frame_gaps
}

fn upstream_config() -> Config {
    Config {
        max_call_depth: MAX_CALL_DEPTH,
        stack_frame_size: FRAME_SIZE as usize,
        ..Config::default()
    }
}

fn upstream(
    version: SBPFVersion,
    executable: &Executable<Context>,
    loader: &Arc<BuiltinProgram<Context>>,
    m: &Machine,
) -> After {
    let config = upstream_config();
    let mut stack = AlignedMemory::<{ ebpf::HOST_ALIGN }>::from_slice(&m.stack);
    let mut heap = AlignedMemory::<{ ebpf::HOST_ALIGN }>::from_slice(&m.heap);
    let mut input = AlignedMemory::<{ ebpf::HOST_ALIGN }>::from_slice(&m.input);
    let stack_region = if version.stack_frame_gaps() && config.enable_stack_frame_gaps {
        MemoryRegion::new_gapped(&mut stack, ebpf::MM_STACK_START, FRAME_SIZE)
    } else {
        MemoryRegion::new(&mut stack, ebpf::MM_STACK_START)
    };
    let regions = vec![
        executable.get_ro_region(),
        stack_region,
        MemoryRegion::new(&mut heap, ebpf::MM_HEAP_START),
        MemoryRegion::new(&mut input, ebpf::MM_INPUT_START),
    ];
    // `MemoryMapping::new` with its default (no-op) access-violation handler,
    // passed as a closure: Kani 0.68 cannot box a function item.
    let handler: AccessViolationHandler = Box::new(|region, max_len, access, addr, len| {
        default_access_violation_handler(region, max_len, access, addr, len)
    });
    // SAFETY: the stack, heap, input, and executable outlive the mapping and
    // the VM, which are dropped at the end of the block below before the
    // buffers are read. The writable regions are distinct allocations, and
    // the handler leaves every region unmodified.
    let mapping = unsafe {
        MemoryMapping::new_with_access_violation_handler(regions, &config, version, handler)
    }
    .unwrap_or_else(|_| panic!("valid reference regions"));
    let (step, regs, next_slot, frames, remaining) = {
        let mut context = Context {
            remaining: m.budget,
            mapping,
        };
        let mut vm = EbpfVm::new(loader.clone(), version, &mut context, STACK);
        // `execute_program` starts with `previous_instruction_meter` equal to
        // the context's remaining units; a syscall restores that equality.
        vm.previous_instruction_meter = m.budget;
        vm.due_insn_count = m.used;
        vm.call_depth = m.depth as u64;
        let mut call_frames = vec![CallFrame::default(); MAX_CALL_DEPTH];
        for (index, frame) in m.frames.iter().enumerate().take(m.depth) {
            call_frames[index] = CallFrame {
                caller_saved_registers: frame.saved,
                frame_pointer: frame.frame_pointer,
                target_pc: frame.return_slot,
            };
        }
        let mut registers = [0u64; 12];
        registers[..11].copy_from_slice(&m.regs);
        registers[11] = AT;
        let (more, reg) = {
            let mut interpreter =
                Interpreter::new(&mut vm, executable, registers, &mut call_frames);
            let more = interpreter.step();
            (more, interpreter.reg)
        };
        let step = if more {
            Step::Continue
        } else {
            match &vm.program_result {
                ProgramResult::Ok(r0) => Step::Exit(*r0),
                ProgramResult::Err(error) => Step::Trap(upstream_fault(error), vm.registers[11]),
            }
        };
        let depth = vm.call_depth as usize;
        let frames = call_frames[..depth.min(MAX_CALL_DEPTH)]
            .iter()
            .map(|frame| {
                let frame = Frame {
                    saved: frame.caller_saved_registers,
                    frame_pointer: frame.frame_pointer,
                    return_slot: frame.target_pc,
                };
                (frame, frame.return_slot)
            })
            .collect();
        let remaining = vm
            .previous_instruction_meter
            .wrapping_sub(vm.due_insn_count);
        let mut regs = [0u64; 11];
        regs.copy_from_slice(&reg[..11]);
        (step, regs, reg[11], frames, remaining)
    };
    After {
        step,
        regs,
        next_slot,
        frames,
        remaining,
        bytes: Bytes {
            stack: stack.as_slice()[m.probe.stack],
            heap: heap.as_slice()[m.probe.heap],
            input: input.as_slice()[m.probe.input],
        },
    }
}

/// The engine's segmented memory holding the machine's stack, heap, and
/// input. With V0 frame gaps, backing frame `k` is the virtual frame `2k`.
fn engine_memory(
    version: SbpfVersion,
    text: &[u8],
    gapped: bool,
    m: &Machine,
    config: &EbpfConfig,
) -> SbpfMemoryMapping {
    let mut memory = SbpfMemoryMapping::new_solana_with_config(
        config,
        text,
        rodata_address(version),
        text,
        STACK,
        HEAP,
        &m.input,
    )
    .unwrap_or_else(|_| panic!("valid engine layout"));
    memory
        .write_bytes(MM_HEAP_START, &m.heap)
        .unwrap_or_else(|_| panic!("heap"));
    let frame = FRAME_SIZE as usize;
    for index in 0..MAX_CALL_DEPTH {
        let virtual_frame = if gapped { 2 * index } else { index };
        memory
            .write_bytes(
                MM_STACK_START + (virtual_frame * frame) as u64,
                &m.stack[index * frame..(index + 1) * frame],
            )
            .unwrap_or_else(|_| panic!("stack"));
    }
    memory
}

fn engine_frames(m: &Machine) -> Vec<EbpfCallFrame> {
    m.frames
        .iter()
        .take(m.depth)
        .map(|frame| EbpfCallFrame {
            caller_saved_regs: frame.saved,
            frame_pointer: frame.frame_pointer,
            return_pc: MM_BYTECODE_START + 8 * frame.return_slot,
        })
        .collect()
}

fn observed_frames(frames: &[EbpfCallFrame]) -> Vec<(Frame, u64)> {
    frames
        .iter()
        .map(|frame| {
            let saved = Frame {
                saved: frame.caller_saved_regs,
                frame_pointer: frame.frame_pointer,
                return_slot: slot(frame.return_pc),
            };
            (saved, frame.return_pc)
        })
        .collect()
}

/// The probed bytes of the engine's stack backing, heap, and input.
fn probed(memory: &SbpfMemoryMapping, probe: Probe) -> Bytes {
    let byte = |address, index: usize| {
        memory
            .region_slice(address)
            .unwrap_or_else(|| panic!("mapped region"))[index]
    };
    Bytes {
        stack: byte(MM_STACK_START, probe.stack),
        heap: byte(MM_HEAP_START, probe.heap),
        input: byte(MM_INPUT_START, probe.input),
    }
}

fn engine(
    version: SbpfVersion,
    text: &[u8],
    functions: &[(u32, usize)],
    gapped: bool,
    m: &Machine,
) -> After {
    let config = rax_config(version, text.len(), m.budget);
    let mut memory = engine_memory(version, text, gapped, m, &config);

    let mut cpu = EbpfCpu::new(config);
    cpu.regs.r = m.regs;
    cpu.regs.pc = MM_BYTECODE_START + 8 * AT;
    cpu.insn_count = m.used;
    cpu.call_frames = engine_frames(m);
    // The embedder registers exactly the executable's functions, including
    // the entrypoint the reference registers implicitly.
    for &(key, target) in functions {
        cpu.register_function(key, MM_BYTECODE_START + 8 * target as u64);
    }
    cpu.register_syscall(SYSCALL_KEY, |regs, _memory| {
        Ok(mix(regs[1], regs[2], regs[3], regs[4], regs[5]))
    });

    let step = match cpu.step(&mut StaticFetch {
        mapping: &mut memory,
    }) {
        Ok(None) => Step::Continue,
        Ok(Some(EbpfExit::Exit { r0 })) => Step::Exit(r0),
        // The embedder charges the metered syscall before resuming.
        Ok(Some(EbpfExit::Syscall {
            key: CHARGE_KEY, ..
        })) => {
            let r = cpu.regs.r;
            cpu.insn_count = charged(cpu.insn_count, m.budget, charge_cost(r[1]));
            cpu.regs.r[0] = mix(r[1], r[2], r[3], r[4], r[5]);
            Step::Continue
        }
        // A key with no syscall is an unsupported instruction at the call.
        Ok(Some(EbpfExit::Syscall { next_pc, .. } | EbpfExit::BtfHelper { next_pc, .. })) => {
            Step::Trap(Fault::Unsupported, slot(next_pc.wrapping_sub(8)))
        }
        Ok(Some(EbpfExit::Trap(trap))) | Err(trap) => engine_fault(trap),
    };
    After {
        step,
        regs: cpu.regs.r,
        next_slot: slot(cpu.regs.pc),
        frames: observed_frames(&cpu.call_frames),
        remaining: m.budget.wrapping_sub(cpu.insn_count),
        bytes: probed(&memory, m.probe),
    }
}

/// Admission alone, which both verifiers decide from a stack-resident text
/// (so the opcode stays concrete for the model checker): `verify_sbpf`
/// returns the reference verdict, variant and slot, for every value of the
/// instruction's other bits and of an LDDW second slot, both in the middle of
/// the text ([`text`]) and as its last slot (`EXIT; insn`, which also
/// decides "LDDW cannot be last").
pub(crate) fn check_admission(version: SbpfVersion, opcode: u8) {
    let upstream_version = upstream_version(version);
    let text = text(opcode);
    assert!(verify_sbpf(&text, version) == upstream_verdict(upstream_version, &text));

    let mut last = [0u8; 16];
    last[0] = ebpf::EXIT;
    last[8..].copy_from_slice(&text[8..16]);
    assert!(verify_sbpf(&last, version) == upstream_verdict(upstream_version, &last));
}

pub(crate) fn check(version: SbpfVersion, opcode: u8) {
    let upstream_version = upstream_version(version);
    let text = text(opcode);

    // Admission: the engine verifier returns the reference verdict.
    let verdict = upstream_verdict(upstream_version, &text);
    assert!(verify_sbpf(&text, version) == verdict);
    if verdict.is_err() {
        return;
    }

    let mut loader = BuiltinProgram::<Context>::new_loader(upstream_config());
    loader
        .register_definition::<RaxMix>("rax_mix")
        .unwrap_or_else(|_| panic!("rax_mix"));
    loader
        .register_definition::<RaxCharge>("rax_charge")
        .unwrap_or_else(|_| panic!("rax_charge"));
    let loader = Arc::new(loader);
    let mut registry = FunctionRegistry::default();
    for (index, &(key, target)) in FUNCTIONS.iter().enumerate() {
        registry
            .register_function(key, [b'f', b'0' + index as u8], target)
            .unwrap_or_else(|_| panic!("function"));
    }
    let executable = Executable::from_text_bytes(&text, loader.clone(), upstream_version, registry)
        .unwrap_or_else(|_| panic!("executable"));
    let functions = functions(upstream_version);
    let registry = executable.get_function_registry();
    assert!(registry.iter().count() == functions.len(), "registry size");
    for &(key, target) in &functions {
        assert!(
            registry
                .lookup_by_key(key)
                .is_some_and(|(_, value)| value == target),
            "registry entry"
        );
    }

    install_text(&text);
    let m = machine();
    let gapped = gapped(upstream_version);
    let expected = upstream(upstream_version, &executable, &loader, &m);
    let actual = engine(version, &text, &functions, gapped, &m);

    assert!(actual.step == expected.step, "step result");
    assert!(
        actual.remaining == expected.remaining,
        "remaining compute units"
    );
    match expected.step {
        Step::Continue => {
            assert!(actual.regs == expected.regs, "registers");
            assert!(actual.next_slot == expected.next_slot, "next slot");
            assert!(actual.frames.len() == expected.frames.len(), "call depth");
            for (engine, reference) in actual.frames.iter().zip(&expected.frames) {
                assert!(engine.0.saved == reference.0.saved);
                assert!(engine.0.frame_pointer == reference.0.frame_pointer);
                assert!(engine.1 == MM_BYTECODE_START + 8 * reference.1, "return pc");
            }
        }
        Step::Exit(_) => assert!(actual.regs == expected.regs, "registers at exit"),
        Step::Trap(..) => {}
    }
    assert!(
        actual.bytes == expected.bytes,
        "stack, heap, and input bytes"
    );
}
