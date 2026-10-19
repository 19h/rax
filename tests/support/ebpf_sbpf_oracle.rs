//! Solana SBPF oracle support shared by `tools/ebpf-reference`, which runs
//! the pinned Anza interpreter, and the engine `ebpf_diff` corpus replay.
//!
//! It defines the deterministic case streams, the rax execution planes (the
//! direct `EbpfCpu` and SMIR at O0/O1/O2 with on-demand lifting), the
//! implementation-neutral outcome, and the outcome digest. Programs are
//! admitted with [`verify_sbpf`], which the reference package proves equal
//! to Anza's `RequisiteVerifier`; the streams are therefore identical
//! whichever verifier filters them.

#![allow(dead_code)]

use std::cell::RefCell;

use rax::isa::ebpf::{
    EbpfConfig, EbpfCpu, EbpfExit, EbpfHelperKey, EbpfMemory, EbpfTrap, MM_BYTECODE_START,
    MM_HEAP_START, MM_INPUT_START, MM_STACK_START, SbpfMemoryMapping, SbpfVersion, verify_sbpf,
};
use rax::smir::ir::types::{ArchReg, EbpfReg};
use rax::smir::lift::{LiftContext, SmirLifter};
use rax::smir::optimize::{OptLevel, optimize_function};
use rax::smir::{EbpfLifter, ExitReason, SmirContext, SmirInterpreter};

/// Call depth for every configuration; stack backing is 4 frames x 4096 B.
pub const MAX_CALL_DEPTH: usize = 4;
pub const FRAME_SIZE: u64 = 4096;
pub const HEAP_LEN: usize = 256;
pub const INPUT_LEN: usize = 256;

/// Murmur3 key of the deterministic test syscall `rax_mix`, as upstream
/// `ebpf::hash_symbol_name` computes it (asserted by the reference package).
pub const SYSCALL_KEY: u32 = 0x129a_4bc0;

pub const VERSIONS: [SbpfVersion; 5] = [
    SbpfVersion::V0,
    SbpfVersion::V1,
    SbpfVersion::V2,
    SbpfVersion::V3,
    SbpfVersion::V4,
];

/// Murmur3 key of the metered test syscall `rax_charge`: it consumes
/// [`charge_cost`] compute units and returns [`mix`] of its arguments.
pub const CHARGE_KEY: u32 = 0x6295_6008;

/// Compute units `rax_charge` consumes: the low byte of `r1`.
pub fn charge_cost(r1: u64) -> u64 {
    r1 & 0xff
}

/// The meter after a metered syscall. The reference context consumes the
/// units used so far, then the syscall's cost, saturating at the budget; the
/// next instruction traps if nothing remains. `used` never exceeds `budget`.
pub fn charged(used: u64, budget: u64, cost: u64) -> u64 {
    used.saturating_add(cost).min(budget)
}

/// Pure, zero-cost syscall: `r0 = r1 + 3 * r2 ^ r3 - r4 + (r5 << 1)`.
pub fn mix(a: u64, b: u64, c: u64, d: u64, e: u64) -> u64 {
    a.wrapping_add(b.wrapping_mul(3) ^ c)
        .wrapping_sub(d)
        .wrapping_add(e << 1)
}

/// Implementation-neutral trap class. Upstream `AccessViolation` and
/// `StackAccessViolation` both denote a rejected guest access.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trap {
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
    Other(String),
}

/// Observable result of one execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// `Ok((r0, gprs))` or `Err((trap, faulting instruction slot))`.
    pub result: Result<(u64, [u64; 11]), (Trap, u64)>,
    /// Executed instructions, which equal the consumed compute units.
    pub instructions: u64,
    pub stack: Vec<u8>,
    pub heap: Vec<u8>,
    pub input: Vec<u8>,
}

impl Outcome {
    /// Canonical byte image for digests.
    pub fn bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match &self.result {
            Ok((r0, regs)) => {
                out.push(0);
                out.extend_from_slice(&r0.to_le_bytes());
                for reg in regs {
                    out.extend_from_slice(&reg.to_le_bytes());
                }
            }
            Err((trap, slot)) => {
                out.push(1);
                let (code, payload): (u8, [u64; 3]) = match trap {
                    Trap::DivideByZero => (1, [0; 3]),
                    Trap::DivideOverflow => (2, [0; 3]),
                    Trap::Memory { write, addr, len } => (3, [u64::from(*write), *addr, *len]),
                    Trap::CallDepthExceeded => (4, [0; 3]),
                    Trap::CallOutsideTextSegment => (5, [0; 3]),
                    Trap::ExecutionOverrun => (6, [0; 3]),
                    Trap::ExceededMaxInstructions => (7, [0; 3]),
                    Trap::Unsupported => (8, [0; 3]),
                    Trap::Other(text) => {
                        out.extend_from_slice(text.as_bytes());
                        (255, [0; 3])
                    }
                };
                out.push(code);
                for value in payload {
                    out.extend_from_slice(&value.to_le_bytes());
                }
                out.extend_from_slice(&slot.to_le_bytes());
            }
        }
        out.extend_from_slice(&self.instructions.to_le_bytes());
        for region in [&self.stack, &self.heap, &self.input] {
            out.extend_from_slice(&(region.len() as u64).to_le_bytes());
            out.extend_from_slice(region);
        }
        out
    }
}

/// Order-sensitive 128-bit digest (two independent FNV-1a 64 lanes). It
/// detects any accidental difference; it is not an adversarial hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Digest(u64, u64);

impl Default for Digest {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325, 0x6c62_272e_07bb_0142)
    }
}

impl Digest {
    pub fn update(&mut self, bytes: &[u8]) {
        for &byte in (bytes.len() as u64).to_le_bytes().iter().chain(bytes) {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
            self.1 = (self.1 ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3) ^ 0x9e37;
        }
    }

    pub fn hex(&self) -> String {
        format!("{:016x}{:016x}", self.0, self.1)
    }
}

/// Initial state shared by every implementation.
#[derive(Debug, Clone)]
pub struct State {
    /// `r0..r9`; `r10` is initialized by each implementation's VM policy.
    pub regs: [u64; 10],
    pub heap: Vec<u8>,
    pub input: Vec<u8>,
    /// Instruction-meter budget.
    pub budget: u64,
}

/// One program with its registries and initial states.
#[derive(Debug, Clone)]
pub struct Case {
    pub version: SbpfVersion,
    pub program: Vec<u8>,
    /// Legacy V0-V2 internal functions: key -> instruction slot.
    pub functions: Vec<(u32, usize)>,
    pub states: Vec<State>,
}

/// Text bytes are readable as rodata, at segment zero in V3/V4 and at the
/// bytecode segment in V0-V2 (`Executable::new_from_text_bytes`).
pub fn rodata_address(version: SbpfVersion) -> u64 {
    if version.enable_lower_rodata_vaddr() {
        0
    } else {
        MM_BYTECODE_START
    }
}

pub fn rax_config(version: SbpfVersion, len: usize, budget: u64) -> EbpfConfig {
    EbpfConfig {
        max_call_depth: MAX_CALL_DEPTH,
        stack_frame_size: FRAME_SIZE,
        max_instructions: Some(budget),
        text_range: Some((MM_BYTECODE_START, MM_BYTECODE_START + len as u64)),
        ..EbpfConfig::solana(version)
    }
}

pub fn mapping(
    config: &EbpfConfig,
    program: &[u8],
    heap: &[u8],
    input: &[u8],
) -> SbpfMemoryMapping {
    let version = config.dialect.sbpf_version().expect("SBPF");
    let mut map = SbpfMemoryMapping::new_solana_with_config(
        config,
        program,
        rodata_address(version),
        program,
        MAX_CALL_DEPTH * FRAME_SIZE as usize,
        heap.len(),
        input,
    )
    .expect("valid rax layout");
    // The constructor zero-fills the heap; install the case's heap image.
    map.write_bytes(MM_HEAP_START, heap).expect("heap");
    map
}

fn finish(
    result: Result<(u64, [u64; 11]), (Trap, u64)>,
    instructions: u64,
    memory: &SbpfMemoryMapping,
) -> Outcome {
    let region = |address| {
        memory
            .region_slice(address)
            .map(<[u8]>::to_vec)
            .unwrap_or_default()
    };
    Outcome {
        result,
        instructions,
        stack: region(MM_STACK_START),
        heap: region(MM_HEAP_START),
        input: region(MM_INPUT_START),
    }
}

pub fn slot(pc: u64) -> u64 {
    pc.wrapping_sub(MM_BYTECODE_START) / 8
}

pub fn rax_trap(trap: EbpfTrap) -> (Trap, u64) {
    match trap {
        EbpfTrap::DecodeError { pc, .. } => (Trap::Unsupported, slot(pc)),
        EbpfTrap::DivideByZero { pc } => (Trap::DivideByZero, slot(pc)),
        EbpfTrap::DivideOverflow { pc } => (Trap::DivideOverflow, slot(pc)),
        EbpfTrap::MemoryFault { pc, fault } => (
            Trap::Memory {
                write: fault.write,
                addr: fault.addr,
                len: fault.size as u64,
            },
            slot(pc),
        ),
        EbpfTrap::CallDepthExceeded { pc } => (Trap::CallDepthExceeded, slot(pc)),
        // The fetch of a slot outside the text is the reference
        // ExecutionOverrun; a call or jump target names a different address.
        EbpfTrap::CallOutsideTextSegment { pc, target_pc } if pc == target_pc => {
            (Trap::ExecutionOverrun, slot(pc))
        }
        EbpfTrap::CallOutsideTextSegment { pc, .. } => (Trap::CallOutsideTextSegment, slot(pc)),
        EbpfTrap::UnsupportedSyscall { pc, .. } => (Trap::Unsupported, slot(pc)),
        EbpfTrap::ExceededMaxInstructions { pc } => (Trap::ExceededMaxInstructions, slot(pc)),
        other => (Trap::Other(format!("{other:?}")), 0),
    }
}

/// A program lifted once, with the rax planes that execute it.
pub struct RaxFixture {
    pub version: SbpfVersion,
    pub program: Vec<u8>,
    pub functions: Vec<(u32, usize)>,
    /// Lifted once per program; interpreter state lives in each context.
    smir: [RefCell<SmirInterpreter>; 3],
    lifter: RefCell<EbpfLifter>,
    /// One block-identifier space for the initial and on-demand lifts.
    lift_context: RefCell<LiftContext>,
}

const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

impl RaxFixture {
    /// Lift `program`; a lift failure of an admitted program is a defect.
    pub fn new(
        version: SbpfVersion,
        program: &[u8],
        functions: &[(u32, usize)],
    ) -> Result<Self, String> {
        let config = rax_config(version, program.len(), u64::MAX);
        let mut lifter = EbpfLifter::solana(version);
        for &(key, slot) in functions {
            lifter.register_function(key, MM_BYTECODE_START + 8 * slot as u64);
        }
        lifter.register_syscall(SYSCALL_KEY);
        lifter.register_syscall(CHARGE_KEY);
        let memory = mapping(&config, program, &[0; HEAP_LEN], &[0; INPUT_LEN]);
        let mut lift_context = LiftContext::new(lifter.source_arch());
        let lifted = lifter
            .lift_program(MM_BYTECODE_START, &memory, &mut lift_context)
            .map_err(|error| {
                format!(
                    "SMIR lift error on an admitted {version:?} program {}: {error}",
                    hex(program)
                )
            })?;
        let smir = LEVELS.map(|level| {
            let mut interpreter = SmirInterpreter::new();
            interpreter.set_max_insns(u64::MAX);
            for mut function in lifted.clone() {
                if level != OptLevel::O0 {
                    optimize_function(&mut function, level);
                }
                interpreter.add_function(function);
            }
            RefCell::new(interpreter)
        });
        Ok(Self {
            version,
            program: program.to_vec(),
            functions: functions.to_vec(),
            smir,
            lifter: RefCell::new(lifter),
            lift_context: RefCell::new(lift_context),
        })
    }

    pub fn direct(&self, state: &State) -> Outcome {
        let config = rax_config(self.version, self.program.len(), state.budget);
        let mut memory = mapping(&config, &self.program, &state.heap, &state.input);
        let mut cpu = EbpfCpu::new(config);
        cpu.regs.pc = MM_BYTECODE_START;
        cpu.regs.r[..10].copy_from_slice(&state.regs);
        for &(key, slot) in &self.functions {
            cpu.register_function(key, MM_BYTECODE_START + 8 * slot as u64);
        }
        cpu.register_syscall(SYSCALL_KEY, |regs, _memory| {
            Ok(mix(regs[1], regs[2], regs[3], regs[4], regs[5]))
        });
        let result = loop {
            match cpu.run(&mut memory) {
                EbpfExit::Exit { r0 } => break Ok((r0, cpu.regs.r)),
                // The metered syscall runs in the embedder, which charges
                // its units to the meter before resuming.
                EbpfExit::Syscall {
                    key: CHARGE_KEY, ..
                } => {
                    let r = cpu.regs.r;
                    cpu.insn_count = charged(cpu.insn_count, state.budget, charge_cost(r[1]));
                    cpu.regs.r[0] = mix(r[1], r[2], r[3], r[4], r[5]);
                }
                // The engine hands an unregistered key to its embedder. This
                // embedding behaves as the upstream loader: a key with no
                // syscall is an unsupported instruction at the call.
                EbpfExit::Syscall { next_pc, .. } | EbpfExit::BtfHelper { next_pc, .. } => {
                    break Err((Trap::Unsupported, slot(next_pc.wrapping_sub(8))));
                }
                EbpfExit::Trap(trap) => break Err(rax_trap(trap)),
            }
        };
        finish(result, cpu.insn_count, &memory)
    }

    pub fn smir(&self, index: usize, state: &State) -> Outcome {
        let config = rax_config(self.version, self.program.len(), state.budget);
        let mut memory = mapping(&config, &self.program, &state.heap, &state.input);
        let mut context = SmirContext::new_ebpf_with_config(config);
        for (reg, value) in state.regs.iter().enumerate() {
            context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(reg as u8)), *value);
        }
        context.pc = MM_BYTECODE_START;
        let mut interpreter = self.smir[index].borrow_mut();
        let read = |context: &SmirContext| {
            let mut regs = [0; 11];
            for (reg, slot) in regs.iter_mut().enumerate() {
                *slot = context.read_arch_reg(ArchReg::Ebpf(EbpfReg::R(reg as u8)));
            }
            regs
        };
        let result = loop {
            match interpreter.run(&mut context, &mut memory) {
                ExitReason::Syscall { num, args }
                    if EbpfHelperKey::from_smir(num) == EbpfHelperKey::Static(SYSCALL_KEY) =>
                {
                    let value = mix(args[0], args[1], args[2], args[3], args[4]);
                    context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(0)), value);
                }
                ExitReason::Syscall { num, args }
                    if EbpfHelperKey::from_smir(num) == EbpfHelperKey::Static(CHARGE_KEY) =>
                {
                    context.insn_count =
                        charged(context.insn_count, state.budget, charge_cost(args[0]));
                    let value = mix(args[0], args[1], args[2], args[3], args[4]);
                    context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(0)), value);
                }
                // As for the direct CPU: the loader has no such syscall.
                ExitReason::Syscall { .. } => {
                    break Err((Trap::Unsupported, slot(context.pc.wrapping_sub(8))));
                }
                ExitReason::Return { to } => {
                    let regs = read(&context);
                    assert_eq!(to, regs[0]);
                    break Ok((to, regs));
                }
                ExitReason::EbpfTrap(trap) => break Err(rax_trap(trap)),
                // The SMIR dispatch protocol: lift code first reached
                // dynamically (CALLX targets) and resume at the same PC.
                ExitReason::BlockNotFound { addr } => {
                    let mut lifter = self.lifter.borrow_mut();
                    let mut lift_context = self.lift_context.borrow_mut();
                    match lifter.lift_program(addr, &memory, &mut lift_context) {
                        Ok(functions) => {
                            for mut function in functions {
                                if LEVELS[index] != OptLevel::O0 {
                                    optimize_function(&mut function, LEVELS[index]);
                                }
                                interpreter.add_function(function);
                            }
                        }
                        Err(error) => {
                            break Err((
                                Trap::Other(format!("lift at {addr:#x}: {error}")),
                                slot(addr),
                            ));
                        }
                    }
                }
                other => break Err((Trap::Other(format!("{other:?}")), slot(context.pc))),
            }
        };
        finish(result, context.insn_count, &memory)
    }

    /// Every rax plane, labelled.
    pub fn planes(&self, state: &State) -> [(&'static str, Outcome); 4] {
        [
            ("direct", self.direct(state)),
            ("smir-o0", self.smir(0, state)),
            ("smir-o1", self.smir(1, state)),
            ("smir-o2", self.smir(2, state)),
        ]
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes
        .chunks(8)
        .map(|chunk| chunk.iter().map(|b| format!("{b:02x}")).collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Deterministic xorshift64* stream for reproducible campaigns.
#[derive(Clone)]
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    pub fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    pub fn pick<T: Copy>(&mut self, values: &[T]) -> T {
        values[self.below(values.len() as u64) as usize]
    }
}

/// Operand partitions: zero, one-hot, all-ones, sign boundaries at 8, 16,
/// 32, and 64 bits, shift-count boundaries, and alternating patterns.
pub const VALUES64: [u64; 32] = [
    0,
    1,
    2,
    7,
    31,
    32,
    63,
    64,
    0x7f,
    0x80,
    0xff,
    0x7fff,
    0x8000,
    0xffff,
    0x7fff_ffff,
    0x8000_0000,
    0xffff_ffff,
    0x1_0000_0000,
    0x7fff_ffff_ffff_ffff,
    0x8000_0000_0000_0000,
    0xffff_ffff_ffff_fffe,
    0xffff_ffff_ffff_ffff,
    0x5555_5555_5555_5555,
    0xaaaa_aaaa_aaaa_aaaa,
    0x0123_4567_89ab_cdef,
    0xffff_ffff_8000_0000,
    0x8000_0000_ffff_ffff,
    0xffff_fff0_0000_0007,
    0x0000_0001_8000_0000,
    3,
    0xfedc_ba98_7654_3210,
    0x0000_7fff_ffff_8000,
];

pub const VALUES32: [i32; 24] = [
    0,
    1,
    2,
    7,
    15,
    16,
    31,
    32,
    63,
    64,
    0x7f,
    0x80,
    0x7fff,
    0x8000,
    0xffff,
    0x7fff_ffff,
    -1,
    -2,
    -128,
    -32768,
    i32::MIN,
    i32::MIN + 1,
    0x5555_5555,
    0xaaaa_aaaa_u32 as i32,
];

/// Address-like register values: region bases, region ends, rodata/text,
/// and near-boundary offsets that make loads and stores succeed or fault.
pub fn address_like(rng: &mut Rng) -> u64 {
    let bases = [
        MM_HEAP_START,
        MM_HEAP_START + HEAP_LEN as u64 - 8,
        MM_INPUT_START,
        MM_INPUT_START + INPUT_LEN as u64 - 4,
        MM_STACK_START,
        MM_STACK_START + FRAME_SIZE - 8,
        MM_STACK_START + FRAME_SIZE,
        MM_STACK_START + 2 * FRAME_SIZE + 16,
        MM_BYTECODE_START,
        0,
        8,
    ];
    rng.pick(&bases)
        .wrapping_add(rng.below(48))
        .wrapping_sub(16)
}

pub fn encode(opc: u8, dst: u8, src: u8, off: i16, imm: i32) -> [u8; 8] {
    let off = off.to_le_bytes();
    let imm = imm.to_le_bytes();
    [
        opc,
        (src << 4) | (dst & 0x0f),
        off[0],
        off[1],
        imm[0],
        imm[1],
        imm[2],
        imm[3],
    ]
}

pub fn random_state(rng: &mut Rng, budget: u64) -> State {
    let mut regs = [0; 10];
    for value in &mut regs {
        *value = match rng.below(4) {
            0 => rng.pick(&VALUES64),
            1 | 2 => address_like(rng),
            _ => rng.next(),
        };
    }
    let mut heap = vec![0; HEAP_LEN];
    let mut input = vec![0; INPUT_LEN];
    for byte in heap.iter_mut().chain(input.iter_mut()) {
        *byte = rng.next() as u8;
    }
    State {
        regs,
        heap,
        input,
        budget,
    }
}

pub const EXIT: [u8; 8] = [0x95, 0, 0, 0, 0, 0, 0, 0];
/// `mov64 r0, 0x5a`: a marker skipped by taken branches.
pub const MARKER: [u8; 8] = [0xb7, 0, 0, 0, 0x5a, 0, 0, 0];

pub fn is_jump(version: SbpfVersion, opc: u8) -> bool {
    let class = opc & 0x07;
    let code = opc & 0xf0;
    let conditional = matches!(code, 0x10..=0x70 | 0xa0..=0xd0);
    (class == 0x05 && (opc == 0x05 || conditional))
        || (class == 0x06 && version.enable_jmp32() && conditional)
}

pub fn is_endian(opc: u8) -> bool {
    matches!(opc, 0xd4 | 0xdc)
}

fn random_imm(rng: &mut Rng, opc: u8) -> i32 {
    if is_endian(opc) {
        return rng.pick(&[16, 32, 64, 16, 32, 64, 0, 8, 48, 128]);
    }
    match rng.below(4) {
        0 => rng.pick(&VALUES32),
        1 => rng.below(64) as i32,
        2 => address_like(rng) as i32,
        _ => rng.next() as i32,
    }
}

/// A candidate program around one instruction. Jumps target the marker
/// (`off` 0), skip it (`off` 1), or spin on themselves (`off` -1).
pub fn single_instruction_program(rng: &mut Rng, version: SbpfVersion, opc: u8) -> Vec<u8> {
    let dst = rng.below(11) as u8;
    let src = rng.below(11) as u8;
    let imm = random_imm(rng, opc);
    if opc == 0x18 {
        let high = random_imm(rng, 0);
        return [
            encode(0x18, dst, src, 0, imm).to_vec(),
            encode(0, 0, 0, 0, high).to_vec(),
            EXIT.to_vec(),
        ]
        .concat();
    }
    let off = if is_jump(version, opc) {
        rng.pick(&[-1i16, 0, 1, 1, 1])
    } else {
        match rng.below(3) {
            0 => 0,
            1 => rng.below(49) as i16 - 24,
            _ => rng.next() as i16,
        }
    };
    [encode(opc, dst, src, off, imm), MARKER, EXIT].concat()
}

/// Opcodes (other than CALL_IMM, CALLX, EXIT) for which at least one
/// single-instruction candidate is admitted.
pub fn verified_opcodes(version: SbpfVersion) -> Vec<u8> {
    let mut opcodes = Vec::new();
    for opc in 0..=255u8 {
        if matches!(opc, 0x85 | 0x8d | 0x95) {
            continue;
        }
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ u64::from(opc));
        let accepted = (0..200).any(|_| {
            let program = single_instruction_program(&mut rng, version, opc);
            verify_sbpf(&program, version).is_ok()
        });
        if accepted {
            opcodes.push(opc);
        }
    }
    opcodes
}

/// Random program over the version's verified instruction set, including
/// LDDW, local and legacy calls with every source kind, CALLX, syscalls,
/// and backward branches bounded by the instruction meter.
pub fn random_program(
    rng: &mut Rng,
    version: SbpfVersion,
    opcodes: &[u8],
) -> (Vec<u8>, Vec<(u32, usize)>) {
    let len = 2 + rng.below(18) as usize;
    let mut slots: Vec<[u8; 8]> = Vec::with_capacity(len + 2);
    let legacy = !version.static_syscalls();
    let functions: Vec<(u32, usize)> = if legacy {
        vec![
            (0x1000_0001, rng.below(len as u64) as usize),
            (0x1000_0002, 0),
        ]
    } else {
        Vec::new()
    };
    while slots.len() < len {
        let index = slots.len();
        let choice = rng.below(16);
        let insn = if choice == 0 || choice == 2 {
            // The source field selects the call kind only in V3/V4; legacy
            // versions ignore it, and V3/V4 verify but cannot resolve 2-10.
            let kind = if rng.below(6) == 0 {
                rng.below(11) as u8
            } else {
                u8::from(!legacy)
            };
            match rng.below(3) {
                0 => encode(
                    0x85,
                    0,
                    if legacy { kind } else { 0 },
                    0,
                    SYSCALL_KEY as i32,
                ),
                1 if legacy => encode(0x85, 0, kind, 0, rng.pick(&functions).0 as i32),
                1 => {
                    // In-text slots, the end, and far targets in both directions.
                    let target = match rng.below(4) {
                        0 => rng.next() as i32 as i64,
                        1 => len as i64 + rng.below(4) as i64,
                        _ => rng.below(len as u64 + 1) as i64,
                    };
                    encode(0x85, 0, kind, 0, (target - index as i64 - 1) as i32)
                }
                _ => {
                    let reg = rng.below(10) as u8;
                    match version {
                        SbpfVersion::V0 | SbpfVersion::V1 => encode(0x8d, 0, 0, 0, i32::from(reg)),
                        SbpfVersion::V2 => encode(0x8d, 0, reg, 0, 0),
                        _ => encode(0x8d, reg, 0, 0, 0),
                    }
                }
            }
        } else if choice == 1 {
            EXIT
        } else {
            let opc = rng.pick(opcodes);
            let dst = rng.below(11) as u8;
            let src = rng.below(11) as u8;
            let imm = random_imm(rng, opc);
            if opc == 0x18 {
                slots.push(encode(0x18, dst, src, 0, imm));
                encode(0, 0, 0, 0, random_imm(rng, 0))
            } else {
                let off = if is_jump(version, opc) {
                    (rng.below(len as u64 + 2) as i64 - index as i64 - 1) as i16
                } else {
                    rng.below(49) as i16 - 24
                };
                encode(opc, dst, src, off, imm)
            }
        };
        slots.push(insn);
    }
    if rng.below(8) != 0 {
        slots.push(EXIT);
    }
    (slots.concat(), functions)
}

/// Some registers hold text addresses for CALLX, including misaligned and
/// out-of-text values.
pub fn prime_registers(rng: &mut Rng, state: &mut State, program_len: usize) {
    for _ in 0..2 {
        let reg = rng.below(10) as usize;
        let slot = rng.below(program_len as u64 / 8 + 2);
        state.regs[reg] = MM_BYTECODE_START + 8 * slot + rng.pick(&[0, 0, 0, 3]);
    }
}

/// Single-instruction stream: `programs` admitted candidates per verified
/// opcode, each with `states` random states (meter budgets 1, 2, or 100).
pub fn single_instruction_cases(version: SbpfVersion, programs: usize, states: usize) -> Vec<Case> {
    let mut cases = Vec::new();
    for opc in verified_opcodes(version) {
        let mut rng = Rng(0xa076_1d64_78bd_642f ^ (u64::from(opc) << 8) ^ version as u64);
        for _ in 0..programs {
            let program = single_instruction_program(&mut rng, version, opc);
            if verify_sbpf(&program, version).is_err() {
                continue;
            }
            let states = (0..states)
                .map(|_| {
                    let budget = rng.pick(&[100, 100, 100, 2, 1]);
                    random_state(&mut rng, budget)
                })
                .collect();
            cases.push(Case {
                version,
                program,
                functions: Vec::new(),
                states,
            });
        }
    }
    cases
}

/// Random-program stream: `programs` admitted programs, `states` each.
pub fn random_cases(version: SbpfVersion, programs: usize, states: usize) -> Vec<Case> {
    let opcodes = verified_opcodes(version);
    let mut rng = Rng(0xd1b5_4a32_d192_ed03 ^ ((version as u64 + 1) * 0x9e37_79b9));
    let mut cases = Vec::new();
    while cases.len() < programs {
        let (program, functions) = random_program(&mut rng, version, &opcodes);
        if verify_sbpf(&program, version).is_err() {
            continue;
        }
        let states = (0..states)
            .map(|_| {
                let budget = rng.pick(&[1, 7, 40, 400, 4000]);
                let mut state = random_state(&mut rng, budget);
                prime_registers(&mut rng, &mut state, program.len());
                state
            })
            .collect();
        cases.push(Case {
            version,
            program,
            functions,
            states,
        });
    }
    cases
}

/// Boundary stream: every verified non-LDDW opcode with distinct and aliased
/// registers, each boundary immediate (immediate forms), and the full
/// boundary cross product of register operands (`stride` thins it).
pub fn boundary_cases(version: SbpfVersion, stride: usize) -> Vec<Case> {
    let mut rng = Rng(0x6a09_e667_f3bc_c909 ^ version as u64);
    let mut cases = Vec::new();
    for opc in verified_opcodes(version) {
        if opc == 0x18 {
            continue;
        }
        let register_source = opc & 0x08 != 0;
        let immediates: &[i32] = if register_source { &[0] } else { &VALUES32 };
        let offset = if is_jump(version, opc) { 1 } else { 0 };
        for &(dst, src) in &[(1u8, 2u8), (3, 3)] {
            for &imm in immediates {
                let imm = if is_endian(opc) {
                    [16, 32, 64][imm.unsigned_abs() as usize % 3]
                } else {
                    imm
                };
                let program = [encode(opc, dst, src, offset, imm), MARKER, EXIT].concat();
                if verify_sbpf(&program, version).is_err() {
                    continue;
                }
                let second: &[u64] = if register_source && dst != src {
                    &VALUES64
                } else {
                    &VALUES64[..1]
                };
                let mut states = Vec::new();
                let mut index = 0;
                for &a in &VALUES64 {
                    for &b in second {
                        index += 1;
                        if index % stride != 0 {
                            continue;
                        }
                        let mut state = random_state(&mut rng, 100);
                        state.regs[usize::from(dst)] = a;
                        if dst != src {
                            state.regs[usize::from(src)] = b;
                        }
                        states.push(state);
                    }
                }
                cases.push(Case {
                    version,
                    program,
                    functions: Vec::new(),
                    states,
                });
            }
        }
    }
    cases
}

/// The corpus campaigns: name and case stream per version. The engine
/// replay and the reference export use exactly these streams.
pub fn corpus_campaigns(version: SbpfVersion) -> Vec<(&'static str, Vec<Case>)> {
    vec![
        (
            "single-instruction",
            single_instruction_cases(version, 24, 3),
        ),
        ("random-program", random_cases(version, 800, 3)),
        ("boundary", boundary_cases(version, 7)),
        ("metered-syscall", metered_cases(version, 120)),
    ]
}

/// Metered-syscall stream: programs that call `rax_charge` with costs from
/// zero to 255 (and high `r1` bits the cost ignores) between ordinary
/// instructions, each run at budgets that exhaust the meter before a call,
/// exactly at it, inside its charge (where the reference saturates), and
/// after it, plus an ample budget.
pub fn metered_cases(version: SbpfVersion, programs: usize) -> Vec<Case> {
    let mut rng = Rng(0x3c6e_f372_fe94_f82b ^ version as u64);
    let mut cases = Vec::new();
    while cases.len() < programs {
        let mut slots = Vec::new();
        let mut total = 0u64;
        for _ in 0..1 + rng.below(3) {
            for _ in 0..rng.below(3) {
                slots.push(encode(0x07, 0, 0, 0, 1));
                total += 1;
            }
            let cost = rng.pick(&[0u64, 1, 2, 3, 7, 64, 255]);
            let high = rng.pick(&[0u64, 0x100, 0xffff_ff00]);
            slots.push(encode(0xb7, 1, 0, 0, (cost | high) as u32 as i32));
            slots.push(encode(0x85, 0, 0, 0, CHARGE_KEY as i32));
            total += 2 + cost;
        }
        slots.push(EXIT);
        total += 1;
        let program = slots.concat();
        if verify_sbpf(&program, version).is_err() {
            continue;
        }
        let mut budgets = vec![0, 1, 2, total.saturating_sub(1), total, total + 1, 10_000];
        budgets.push(rng.below(total + 2));
        let states = budgets
            .into_iter()
            .map(|budget| random_state(&mut rng, budget))
            .collect();
        cases.push(Case {
            version,
            program,
            functions: Vec::new(),
            states,
        });
    }
    cases
}
