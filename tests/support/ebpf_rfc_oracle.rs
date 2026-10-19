//! RFC 9669 oracle support shared by `tools/ebpf-conformance` and the
//! engine `ebpf_diff` corpus replay.
//!
//! It provides the flat execution layout and adapters for the direct
//! `EbpfCpu` and SMIR at O0/O1/O2, the deterministic Linux-kernel campaign
//! generator, the `kernel/harness.c` file formats, and the program digest
//! recorded in `tests/generated/ebpf/linux_kernel_{le,be}.txt`.
//!
//! # Kernel campaign
//!
//! The Linux kernel is the reference eBPF implementation. Each campaign
//! program is a `BPF_PROG_TYPE_SYSCALL` program whose context is a plain
//! caller buffer (`kernel/bpf/syscall.c`, `net/bpf/test_run.c`); the kernel
//! copies the buffer back after `BPF_PROG_TEST_RUN`. A program batches many
//! cases of one instruction form. Each case loads `r0..r9` and an optional
//! 64-byte stack window from the context, executes the instruction under
//! test, and stores the observable state back into the context.
//!
//! The kernel verifier admits only safe programs. Register values therefore
//! come from context loads (unknown scalars), stack windows are initialized
//! before use, conditional branches record their outcome on both paths before
//! a join at which every register is dead, and registers the verifier makes
//! unreadable (`r1..r5` after a call) or that hold stack pointers are never
//! stored. Encodings the verifier rejects by policy, such as division by an
//! immediate zero or immediate shift amounts outside the operand width, are
//! excluded here; RFC 9669 still defines them and the engine suites cover
//! them separately.
//!
//! The same program bytes then run on every engine plane with `r1` set to the
//! context address. The complete context buffers and the low 32 bits of `r0`
//! (the test-run return value) must agree byte-for-byte.

#![allow(dead_code)]

use rax::isa::ebpf::{EbpfConfig, EbpfCpu, EbpfDialect, EbpfExit, EbpfHelperKey, EbpfPlatform};
use rax::smir::ir::memory::{FlatMemory, MemoryError, SmirMemory};
use rax::smir::ir::types::{ArchReg, EbpfReg, Endian, GuestAddr};
use rax::smir::lift::{LiftContext, MemoryReader, SmirLifter};
use rax::smir::optimize::{OptLevel, optimize_function};
use rax::smir::{EbpfLifter, ExitReason, SmirContext, SmirInterpreter};

/// Byte address of the first program instruction.
pub const CODE_BASE: u64 = 0;
/// Largest accepted program, in bytes (32768 instruction slots).
pub const CODE_LIMIT: usize = 0x4_0000;
/// Byte address of caller-supplied memory (`r1` at entry).
pub const INPUT_BASE: u64 = 0x4_0000;
/// Largest caller-supplied memory, in bytes.
pub const INPUT_LIMIT: usize = 0x4_0000;
/// Exclusive top of the downward-growing stack (`r10` at entry).
pub const STACK_TOP: u64 = 0x10_0000;
/// Total flat guest memory, in bytes.
pub const MEMORY_SIZE: usize = 0x10_0000;
/// Per-call frame size, in bytes. 64 frames of 4096 bytes occupy
/// `[0x8_0000, 0x10_0000)`, disjoint from code and input memory.
pub const FRAME_SIZE: u64 = 4096;
/// Maximum nested frames, including the entry frame.
pub const MAX_CALL_DEPTH: usize = 64;
/// Guest instruction budget for one run.
pub const INSTRUCTION_LIMIT: u64 = 10_000_000;

/// One execution plane of the engine under test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// Direct ISA interpreter (`EbpfCpu`).
    Direct,
    /// SMIR lifting and canonical interpretation at one optimization level.
    Smir(OptLevel),
}

impl Engine {
    /// Every engine plane in a stable reporting order.
    pub const ALL: [Engine; 4] = [
        Engine::Direct,
        Engine::Smir(OptLevel::O0),
        Engine::Smir(OptLevel::O1),
        Engine::Smir(OptLevel::O2),
    ];

    /// Parse a command-line engine name.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "direct" => Some(Engine::Direct),
            "smir-o0" => Some(Engine::Smir(OptLevel::O0)),
            "smir-o1" => Some(Engine::Smir(OptLevel::O1)),
            "smir-o2" => Some(Engine::Smir(OptLevel::O2)),
            _ => None,
        }
    }

    /// Stable command-line and report name.
    pub fn name(self) -> &'static str {
        match self {
            Engine::Direct => "direct",
            Engine::Smir(OptLevel::O0) => "smir-o0",
            Engine::Smir(OptLevel::O1) => "smir-o1",
            Engine::Smir(OptLevel::O2) => "smir-o2",
        }
    }
}

/// Architectural result of one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResult {
    /// `Ok(r0)` after the root `EXIT`, or a trap description.
    pub exit: Result<u64, String>,
    /// Final `r0..r10`.
    pub regs: [u64; 11],
    /// Final contents of the caller-supplied memory.
    pub input: Vec<u8>,
    /// Executed guest instructions (a wide `LDDW` counts once).
    pub instructions: u64,
}

/// RFC 9669 engine configuration for the shared flat layout.
pub fn rfc9669_config(endian: Endian) -> EbpfConfig {
    EbpfConfig {
        dialect: EbpfDialect::Rfc9669,
        endian,
        max_call_depth: MAX_CALL_DEPTH,
        stack_frame_size: FRAME_SIZE,
        enable_stack_frame_gaps: false,
        max_instructions: Some(INSTRUCTION_LIMIT),
        text_range: None,
        platform: EbpfPlatform::new(),
    }
}

/// Initial registers: `r1` addresses the input, `r2` holds its length in
/// bytes, and `r10` addresses the top of the entry frame.
pub fn initial_registers(input_len: usize) -> [u64; 11] {
    let mut regs = [0; 11];
    regs[1] = INPUT_BASE;
    regs[2] = input_len as u64;
    regs[10] = STACK_TOP;
    regs
}

struct CodeReader<'a>(&'a [u8]);

impl MemoryReader for CodeReader<'_> {
    fn read(&self, addr: GuestAddr, size: usize) -> Result<Vec<u8>, MemoryError> {
        let start = addr
            .checked_sub(CODE_BASE)
            .ok_or(MemoryError::OutOfBounds { addr })? as usize;
        let end = start
            .checked_add(size)
            .ok_or(MemoryError::OutOfBounds { addr })?;
        self.0
            .get(start..end)
            .map(<[u8]>::to_vec)
            .ok_or(MemoryError::OutOfBounds { addr })
    }
}

fn flat_memory(program: &[u8], input: &[u8]) -> Result<FlatMemory, String> {
    if program.len() > CODE_LIMIT || input.len() > INPUT_LIMIT {
        return Err(format!(
            "program ({} bytes) or input ({} bytes) exceeds the flat layout",
            program.len(),
            input.len()
        ));
    }
    let mut memory = FlatMemory::new(MEMORY_SIZE);
    memory.load(CODE_BASE as usize, program);
    memory.load(INPUT_BASE as usize, input);
    Ok(memory)
}

fn read_input(memory: &mut FlatMemory, len: usize) -> Vec<u8> {
    let mut input = vec![0; len];
    SmirMemory::read(memory, INPUT_BASE, &mut input).expect("input region lies inside flat memory");
    input
}

/// Platform helper resolver: static helper ID and `r1..r5` to the new `r0`,
/// or `None` when the platform defines no such helper.
pub type Helpers<'a> = &'a dyn Fn(u32, [u64; 5]) -> Option<u64>;

/// A platform without helper functions.
pub fn no_helpers(_id: u32, _args: [u64; 5]) -> Option<u64> {
    None
}

/// The `bpf_conformance` platform's sole helper. Helper IDs are
/// platform-defined (RFC 9669 section 4.3.1); the uBPF/eBPF-for-Windows
/// conformance platform binds ID 5 to an "unwind" helper returning `r1`. Only
/// its non-zero path is exercised by the suite (`call_unwind_fail.data`).
pub fn conformance_helpers(id: u32, args: [u64; 5]) -> Option<u64> {
    (id == 5 && args[0] != 0).then_some(args[0])
}

/// Execute `program` with RFC 9669 semantics on one engine plane. Helper
/// calls leave the engine at the resumable continuation; `helpers` supplies
/// the platform result, which this adapter writes to `r0` before resuming.
pub fn run_rfc9669(
    engine: Engine,
    endian: Endian,
    program: &[u8],
    input: &[u8],
    regs: [u64; 11],
    helpers: Helpers<'_>,
) -> RunResult {
    let mut memory = match flat_memory(program, input) {
        Ok(memory) => memory,
        Err(error) => {
            return RunResult {
                exit: Err(error),
                regs,
                input: input.to_vec(),
                instructions: 0,
            };
        }
    };
    let config = rfc9669_config(endian);
    match engine {
        Engine::Direct => {
            let mut cpu = EbpfCpu::new(config);
            cpu.regs.r = regs;
            cpu.regs.pc = CODE_BASE;
            let exit = loop {
                match cpu.run(&mut memory) {
                    EbpfExit::Exit { r0 } => break Ok(r0),
                    EbpfExit::Syscall { key, next_pc } => {
                        let args = [1, 2, 3, 4, 5].map(|index| cpu.regs.r[index]);
                        match helpers(key, args) {
                            Some(value) => cpu.regs.r[0] = value,
                            None => {
                                break Err(format!(
                                    "undefined helper {key} at {:#x}",
                                    next_pc.wrapping_sub(8)
                                ));
                            }
                        }
                    }
                    EbpfExit::BtfHelper { id, next_pc } => {
                        break Err(format!(
                            "undefined BTF helper {id} at {:#x}",
                            next_pc.wrapping_sub(8)
                        ));
                    }
                    EbpfExit::Trap(trap) => break Err(format!("{trap:?}")),
                }
            };
            RunResult {
                exit,
                regs: cpu.regs.r,
                input: read_input(&mut memory, input.len()),
                instructions: cpu.insn_count,
            }
        }
        Engine::Smir(level) => {
            let mut lifter = EbpfLifter::new(EbpfDialect::Rfc9669, endian);
            let mut lift_context = LiftContext::new(lifter.source_arch());
            lift_context.endian = endian;
            let functions =
                match lifter.lift_program(CODE_BASE, &CodeReader(program), &mut lift_context) {
                    Ok(functions) => functions,
                    Err(error) => {
                        return RunResult {
                            exit: Err(format!("lift error: {error}")),
                            regs,
                            input: input.to_vec(),
                            instructions: 0,
                        };
                    }
                };
            let mut interpreter = SmirInterpreter::new();
            interpreter.set_max_insns(INSTRUCTION_LIMIT);
            for mut function in functions {
                if level != OptLevel::O0 {
                    optimize_function(&mut function, level);
                }
                interpreter.add_function(function);
            }
            let mut context = SmirContext::new_ebpf_with_config(config);
            for (index, value) in regs.iter().enumerate() {
                context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(index as u8)), *value);
            }
            context.pc = CODE_BASE;
            let read_regs = |context: &SmirContext| {
                let mut values = [0; 11];
                for (index, slot) in values.iter_mut().enumerate() {
                    *slot = context.read_arch_reg(ArchReg::Ebpf(EbpfReg::R(index as u8)));
                }
                values
            };
            let reason = loop {
                match interpreter.run(&mut context, &mut memory) {
                    ExitReason::Syscall { num, args } => {
                        let mut values = [0; 5];
                        for (slot, value) in values.iter_mut().zip(&args) {
                            *slot = *value;
                        }
                        let EbpfHelperKey::Static(key) = EbpfHelperKey::from_smir(num) else {
                            break ExitReason::Syscall { num, args };
                        };
                        match helpers(key, values) {
                            Some(value) => {
                                context.write_arch_reg(ArchReg::Ebpf(EbpfReg::R(0)), value)
                            }
                            None => break ExitReason::Syscall { num, args },
                        }
                    }
                    other => break other,
                }
            };
            let final_regs = read_regs(&context);
            let exit = match reason {
                ExitReason::Return { to } if to == final_regs[0] => Ok(to),
                ExitReason::Return { to } => Err(format!(
                    "return value {to:#x} differs from r0 {:#x}",
                    final_regs[0]
                )),
                ExitReason::EbpfTrap(trap) => Err(format!("{trap:?}")),
                other => Err(format!("{other:?}")),
            };
            RunResult {
                exit,
                regs: final_regs,
                input: read_input(&mut memory, input.len()),
                instructions: context.insn_count,
            }
        }
    }
}

// ============================================================================
// Linux-kernel campaign generator and harness formats
// ============================================================================

/// Byte offset below `r10` of the saved context pointer.
const CTX_SLOT: i16 = -8;
/// Byte offset below `r10` of the body-`r9` spill slot.
const SPILL_SLOT: i16 = -16;
/// Byte offset below `r10` of the 64-byte stack window.
const WINDOW: i16 = -136;
/// Window length in bytes.
const WINDOW_LEN: usize = 64;
/// Offset of the memory-case base pointer below `r10` (window middle).
const WINDOW_BASE: i16 = WINDOW + 32;
/// Largest context: every context access uses `r9 + off` with a signed
/// 16-bit offset, so the last 8-byte slot must start at or below 32759.
const CTX_LIMIT: usize = 32_760;
/// Largest program, in instruction slots. Smaller batches bound the SMIR
/// optimizer's per-function cost; the kernel limit is far larger.
const PROGRAM_LIMIT: usize = 4_000;

/// One 8-byte instruction slot before byte-order encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Insn {
    pub opc: u8,
    pub dst: u8,
    pub src: u8,
    pub off: i16,
    pub imm: i32,
}

impl Insn {
    pub const fn new(opc: u8, dst: u8, src: u8, off: i16, imm: i32) -> Self {
        Self {
            opc,
            dst,
            src,
            off,
            imm,
        }
    }

    /// RFC 9669 section 3 encoding in the selected byte order.
    pub fn encode(self, endian: Endian) -> [u8; 8] {
        let (regs, off, imm) = match endian {
            Endian::Little => (
                (self.src << 4) | (self.dst & 0x0f),
                self.off.to_le_bytes(),
                self.imm.to_le_bytes(),
            ),
            Endian::Big => (
                (self.dst << 4) | (self.src & 0x0f),
                self.off.to_be_bytes(),
                self.imm.to_be_bytes(),
            ),
        };
        [
            self.opc, regs, off[0], off[1], imm[0], imm[1], imm[2], imm[3],
        ]
    }
}

const fn ldx64(dst: u8, base: u8, off: i16) -> Insn {
    Insn::new(0x79, dst, base, off, 0)
}

const fn stx64(base: u8, off: i16, src: u8) -> Insn {
    Insn::new(0x7b, base, src, off, 0)
}

const fn mov64_imm(dst: u8, imm: i32) -> Insn {
    Insn::new(0xb7, dst, 0, 0, imm)
}

const fn mov64_reg(dst: u8, src: u8) -> Insn {
    Insn::new(0xbf, dst, src, 0, 0)
}

const fn add64_imm(dst: u8, imm: i32) -> Insn {
    Insn::new(0x07, dst, 0, 0, imm)
}

const EXIT: Insn = Insn::new(0x95, 0, 0, 0, 0);

/// Symbolic control-flow fixup resolved once label addresses are known.
#[derive(Debug, Clone, Copy)]
enum Fixup {
    /// PC-relative 16-bit offset (conditional jumps and `JA`).
    Offset(usize),
    /// PC-relative 32-bit immediate (`JA32` and local `CALL`).
    Immediate(usize),
}

#[derive(Default)]
struct Asm {
    insns: Vec<Insn>,
    labels: Vec<Option<usize>>,
    fixups: Vec<(usize, Fixup)>,
}

impl Asm {
    fn label(&mut self) -> usize {
        self.labels.push(None);
        self.labels.len() - 1
    }

    fn bind(&mut self, label: usize) {
        self.labels[label] = Some(self.insns.len());
    }

    fn push(&mut self, insn: Insn) {
        self.insns.push(insn);
    }

    fn push_fixup(&mut self, insn: Insn, fixup: Fixup) {
        self.fixups.push((self.insns.len(), fixup));
        self.insns.push(insn);
    }

    fn finish(mut self) -> Vec<Insn> {
        for (at, fixup) in self.fixups {
            let (label, wide) = match fixup {
                Fixup::Offset(label) => (label, false),
                Fixup::Immediate(label) => (label, true),
            };
            let target = self.labels[label].expect("bound label") as i64;
            let delta = target - (at as i64 + 1);
            if wide {
                self.insns[at].imm = i32::try_from(delta).expect("32-bit displacement");
            } else {
                self.insns[at].off = i16::try_from(delta).expect("16-bit displacement");
            }
        }
        self.insns
    }
}

/// Deterministic xorshift64* generator; campaign inputs must be reproducible.
#[derive(Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn pick<T: Copy>(&mut self, values: &[T]) -> T {
        values[(self.next_u64() % values.len() as u64) as usize]
    }
}

fn seed_of(text: &str) -> u64 {
    // FNV-1a, 64-bit.
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// 64-bit operand partitions: zero, one-hot, all-ones, sign boundaries at 8,
/// 16, 32, and 64 bits, shift-count boundaries, and alternating patterns.
pub const VALUES64: [u64; 40] = [
    0,
    1,
    2,
    3,
    7,
    8,
    31,
    32,
    33,
    63,
    64,
    0x7f,
    0x80,
    0xff,
    0x100,
    0x7fff,
    0x8000,
    0xffff,
    0x1_0000,
    0x7fff_ffff,
    0x8000_0000,
    0x8000_0001,
    0xffff_ffff,
    0x1_0000_0000,
    0x1_0000_0001,
    0x7fff_ffff_ffff_ffff,
    0x8000_0000_0000_0000,
    0x8000_0000_0000_0001,
    0xffff_ffff_0000_0000,
    0xffff_ffff_8000_0000,
    0xffff_ffff_ffff_fffe,
    0xffff_ffff_ffff_ffff,
    0x5555_5555_5555_5555,
    0xaaaa_aaaa_aaaa_aaaa,
    0x0123_4567_89ab_cdef,
    0xfedc_ba98_7654_3210,
    0x8000_0000_ffff_ffff,
    0x0000_0001_8000_0000,
    0xffff_fff0_0000_0007,
    0x0000_7fff_ffff_8000,
];

/// 32-bit immediate partitions (sign-extended to 64 bits for most forms).
pub const VALUES32: [i32; 32] = [
    0,
    1,
    2,
    3,
    7,
    8,
    15,
    16,
    31,
    32,
    33,
    63,
    64,
    0x7f,
    0x80,
    0xff,
    0x100,
    0x7fff,
    0x8000,
    0xffff,
    0x1_0000,
    0x7fff_ffff,
    -1,
    -2,
    -7,
    -128,
    -32768,
    -32769,
    i32::MIN,
    i32::MIN + 1,
    0x5555_5555,
    0xaaaa_aaaa_u32 as i32,
];

/// Final-state observation shape of one case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// Straight-line body.
    Straight,
    /// The final body instruction is a conditional branch to `taken`.
    Branch,
    /// The body calls subprogram `index`; `r1..r5` are unreadable afterwards.
    Call(usize),
}

/// One test case: initial state, body, and observation shape.
#[derive(Debug, Clone)]
struct Case {
    regs: [u64; 10],
    window: Option<[u8; WINDOW_LEN]>,
    body: Vec<Insn>,
    shape: Shape,
    /// Registers that hold stack pointers after the body; never stored.
    pointers: u16,
    /// Use `JA32` rather than `JA` for the branch join.
    wide_join: bool,
}

/// Instruction form under test with all of its cases.
pub struct Form {
    pub name: String,
    cases: Vec<Case>,
    /// Subprograms referenced by `Shape::Call`.
    subprograms: Vec<Vec<Insn>>,
}

/// Context-buffer region of one case, for mismatch attribution.
#[derive(Debug, Clone)]
pub struct CaseLayout {
    pub form: String,
    pub index: usize,
    pub output_offset: usize,
    pub output_len: usize,
}

/// One kernel program with its context and attribution table.
pub struct Program {
    pub form: String,
    pub insns: Vec<Insn>,
    pub ctx: Vec<u8>,
    pub cases: Vec<CaseLayout>,
}

const OUTPUT_REGS: usize = 80;
const OUTPUT_MARKER: usize = OUTPUT_REGS + WINDOW_LEN;
const OUTPUT_LEN: usize = OUTPUT_MARKER + 8;

fn input_len(case: &Case) -> usize {
    OUTPUT_REGS + if case.window.is_some() { WINDOW_LEN } else { 0 }
}

fn put_u64(ctx: &mut [u8], at: usize, value: u64, endian: Endian) {
    let bytes = match endian {
        Endian::Little => value.to_le_bytes(),
        Endian::Big => value.to_be_bytes(),
    };
    ctx[at..at + 8].copy_from_slice(&bytes);
}

fn offset(value: usize) -> i16 {
    i16::try_from(value).expect("context offset fits the signed 16-bit field")
}

/// Emit the observation sequence: spill body `r9`, recover the context
/// pointer, store every readable non-pointer register, the window, and the
/// branch marker when present.
fn emit_store(asm: &mut Asm, case: &Case, out: usize, marker: Option<i32>) {
    let readable = |reg: usize| {
        let after_call = matches!(case.shape, Shape::Call(_)) && (1..=5).contains(&reg);
        !after_call && case.pointers & (1 << reg) == 0
    };
    if readable(9) {
        asm.push(stx64(10, SPILL_SLOT, 9));
    }
    asm.push(ldx64(9, 10, CTX_SLOT));
    for reg in 0..9 {
        if readable(reg) {
            asm.push(stx64(9, offset(out + 8 * reg), reg as u8));
        }
    }
    if readable(9) {
        asm.push(ldx64(0, 10, SPILL_SLOT));
        asm.push(stx64(9, offset(out + 72), 0));
    }
    if case.window.is_some() {
        for slot in 0..WINDOW_LEN / 8 {
            asm.push(ldx64(0, 10, WINDOW + 8 * slot as i16));
            asm.push(stx64(9, offset(out + OUTPUT_REGS + 8 * slot), 0));
        }
    }
    if let Some(marker) = marker {
        asm.push(mov64_imm(0, marker));
        asm.push(stx64(9, offset(out + OUTPUT_MARKER), 0));
    }
}

fn emit_case(asm: &mut Asm, case: &Case, input: usize, out: usize, subprograms: &[usize]) {
    asm.push(ldx64(9, 10, CTX_SLOT));
    if case.window.is_some() {
        for slot in 0..WINDOW_LEN / 8 {
            asm.push(ldx64(0, 9, offset(input + OUTPUT_REGS + 8 * slot)));
            asm.push(stx64(10, WINDOW + 8 * slot as i16, 0));
        }
    }
    for reg in 0..9u8 {
        asm.push(ldx64(reg, 9, offset(input + 8 * usize::from(reg))));
    }
    asm.push(ldx64(9, 9, offset(input + 72)));

    match case.shape {
        Shape::Straight => {
            for insn in &case.body {
                asm.push(*insn);
            }
            emit_store(asm, case, out, None);
        }
        Shape::Call(index) => {
            for insn in &case.body[..case.body.len() - 1] {
                asm.push(*insn);
            }
            let call = *case.body.last().expect("call body");
            asm.push_fixup(call, Fixup::Immediate(subprograms[index]));
            emit_store(asm, case, out, None);
        }
        Shape::Branch => {
            let taken = asm.label();
            let join = asm.label();
            for insn in &case.body[..case.body.len() - 1] {
                asm.push(*insn);
            }
            let branch = *case.body.last().expect("branch body");
            asm.push_fixup(branch, Fixup::Offset(taken));
            emit_store(asm, case, out, Some(0));
            if case.wide_join {
                asm.push_fixup(Insn::new(0x06, 0, 0, 0, 0), Fixup::Immediate(join));
            } else {
                asm.push_fixup(Insn::new(0x05, 0, 0, 0, 0), Fixup::Offset(join));
            }
            asm.bind(taken);
            emit_store(asm, case, out, Some(1));
            asm.bind(join);
        }
    }
}

fn case_insn_estimate(case: &Case) -> usize {
    let window = if case.window.is_some() { 32 } else { 0 };
    let store = 14 + window;
    let stores = if case.shape == Shape::Branch {
        2 * store + 1
    } else {
        store
    };
    11 + window + case.body.len() + stores
}

impl Form {
    /// Pack this form's cases into kernel-sized programs.
    pub fn programs(&self, endian: Endian) -> Vec<Program> {
        let mut programs = Vec::new();
        let mut start = 0;
        while start < self.cases.len() {
            let mut ctx_len = 0;
            let mut insn_len = 4 + self.subprograms.iter().map(Vec::len).sum::<usize>();
            let mut end = start;
            while end < self.cases.len() {
                let case = &self.cases[end];
                let next_ctx = ctx_len + input_len(case) + OUTPUT_LEN;
                let next_insns = insn_len + case_insn_estimate(case);
                if end > start && (next_ctx > CTX_LIMIT || next_insns > PROGRAM_LIMIT) {
                    break;
                }
                ctx_len = next_ctx;
                insn_len = next_insns;
                end += 1;
            }
            programs.push(self.program(start..end, endian));
            start = end;
        }
        programs
    }

    fn program(&self, range: std::ops::Range<usize>, endian: Endian) -> Program {
        let mut asm = Asm::default();
        let subprograms: Vec<usize> = self.subprograms.iter().map(|_| asm.label()).collect();
        let mut ctx = Vec::new();
        let mut layouts = Vec::new();

        asm.push(stx64(10, CTX_SLOT, 1));
        for index in range {
            let case = &self.cases[index];
            let input = ctx.len();
            ctx.resize(input + input_len(case), 0);
            for (reg, value) in case.regs.iter().enumerate() {
                put_u64(&mut ctx, input + 8 * reg, *value, endian);
            }
            if let Some(window) = &case.window {
                ctx[input + OUTPUT_REGS..input + OUTPUT_REGS + WINDOW_LEN].copy_from_slice(window);
            }
            let out = ctx.len();
            // Unstored output slots retain this sentinel in every implementation.
            ctx.resize(out + OUTPUT_LEN, 0xa5);
            emit_case(&mut asm, case, input, out, &subprograms);
            layouts.push(CaseLayout {
                form: self.name.clone(),
                index,
                output_offset: out,
                output_len: OUTPUT_LEN,
            });
        }
        // r0 carries a data-dependent return value: the low 32 bits of the
        // final case's first stored output slot.
        asm.push(ldx64(9, 10, CTX_SLOT));
        asm.push(ldx64(
            0,
            9,
            offset(layouts.last().expect("case").output_offset),
        ));
        asm.push(EXIT);
        for (label, body) in subprograms.iter().zip(&self.subprograms) {
            asm.bind(*label);
            for insn in body {
                asm.push(*insn);
            }
        }
        Program {
            form: self.name.clone(),
            insns: asm.finish(),
            ctx,
            cases: layouts,
        }
    }

    pub fn case_count(&self) -> usize {
        self.cases.len()
    }
}

fn registers_with(rng: &mut Rng, assignments: &[(u8, u64)]) -> [u64; 10] {
    let mut regs = [0; 10];
    for slot in &mut regs {
        *slot = rng.next_u64();
    }
    for &(reg, value) in assignments {
        regs[usize::from(reg)] = value;
    }
    regs
}

/// Register pairs covering distinct operands, aliasing, r0, and r9.
const REG_PAIRS: [(u8, u8); 8] = [
    (1, 2),
    (3, 4),
    (5, 5),
    (0, 9),
    (9, 0),
    (6, 7),
    (8, 3),
    (2, 2),
];

fn reg_operand_pairs(rng: &mut Rng, random: usize) -> Vec<(u64, u64)> {
    let mut pairs = Vec::new();
    for &a in &VALUES64 {
        for &b in &VALUES64 {
            pairs.push((a, b));
        }
    }
    for _ in 0..random {
        pairs.push((rng.next_u64(), rng.next_u64()));
    }
    pairs
}

fn imm_operand_pairs(rng: &mut Rng, random: usize) -> Vec<(u64, i32)> {
    let mut pairs = Vec::new();
    for &a in &VALUES64 {
        for &b in &VALUES32 {
            pairs.push((a, b));
        }
    }
    for _ in 0..random {
        pairs.push((rng.next_u64(), rng.next_u64() as i32));
    }
    pairs
}

/// Binary ALU operation descriptor: name, code nibble, offset field.
const ALU_OPS: [(&str, u8, i16); 14] = [
    ("add", 0x00, 0),
    ("sub", 0x10, 0),
    ("mul", 0x20, 0),
    ("div", 0x30, 0),
    ("sdiv", 0x30, 1),
    ("or", 0x40, 0),
    ("and", 0x50, 0),
    ("lsh", 0x60, 0),
    ("rsh", 0x70, 0),
    ("mod", 0x90, 0),
    ("smod", 0x90, 1),
    ("xor", 0xa0, 0),
    ("mov", 0xb0, 0),
    ("arsh", 0xc0, 0),
];

/// Kernel-verifier policy exclusions for immediate operands (not ISA rules).
fn kernel_rejects_imm(code: u8, wide: bool, imm: i32) -> bool {
    match code {
        0x30 | 0x90 => imm == 0,
        0x60 | 0x70 | 0xc0 => imm < 0 || imm >= if wide { 64 } else { 32 },
        _ => false,
    }
}

fn straight(regs: [u64; 10], body: Vec<Insn>) -> Case {
    Case {
        regs,
        window: None,
        body,
        shape: Shape::Straight,
        pointers: 0,
        wide_join: false,
    }
}

fn alu_forms(scale: usize) -> Vec<Form> {
    let mut forms = Vec::new();
    for (class, width) in [(0x07u8, "64"), (0x04u8, "32")] {
        let wide = class == 0x07;
        for (name, code, off) in ALU_OPS {
            // Register source.
            let form_name = format!("{name}{width}_reg");
            let mut rng = Rng::new(seed_of(&form_name));
            let mut cases = Vec::new();
            for (index, (a, b)) in reg_operand_pairs(&mut rng, scale).into_iter().enumerate() {
                let (dst, src) = REG_PAIRS[index % REG_PAIRS.len()];
                let regs = registers_with(&mut rng, &[(dst, a), (src, b)]);
                let insn = Insn::new(class | 0x08 | code, dst, src, off, 0);
                cases.push(straight(regs, vec![insn]));
            }
            forms.push(Form {
                name: form_name,
                cases,
                subprograms: Vec::new(),
            });

            // Immediate source.
            let form_name = format!("{name}{width}_imm");
            let mut rng = Rng::new(seed_of(&form_name));
            let mut cases = Vec::new();
            for (index, (a, imm)) in imm_operand_pairs(&mut rng, scale).into_iter().enumerate() {
                let imm = if matches!(code, 0x60 | 0x70 | 0xc0) {
                    // Cover every in-range shift count, not only the partitions.
                    if index % 3 == 0 {
                        (index as i32 / 3) % if wide { 64 } else { 32 }
                    } else {
                        imm
                    }
                } else {
                    imm
                };
                if kernel_rejects_imm(code, wide, imm) {
                    continue;
                }
                let dst = REG_PAIRS[index % REG_PAIRS.len()].0;
                let regs = registers_with(&mut rng, &[(dst, a)]);
                let insn = Insn::new(class | code, dst, 0, off, imm);
                cases.push(straight(regs, vec![insn]));
            }
            forms.push(Form {
                name: form_name,
                cases,
                subprograms: Vec::new(),
            });
        }

        // NEG (K source; src, offset, and imm are zero).
        let form_name = format!("neg{width}");
        let mut rng = Rng::new(seed_of(&form_name));
        let mut cases = Vec::new();
        for value in VALUES64 {
            for dst in [0u8, 4, 9] {
                let regs = registers_with(&mut rng, &[(dst, value)]);
                cases.push(straight(regs, vec![Insn::new(class | 0x80, dst, 0, 0, 0)]));
            }
        }
        forms.push(Form {
            name: form_name,
            cases,
            subprograms: Vec::new(),
        });
    }

    // MOVSX: ALU32 X with 8/16-bit sources, ALU64 X with 8/16/32-bit sources.
    for (class, width, sizes) in [
        (0x0cu8 | 0xb0, "32", &[8i16, 16][..]),
        (0x0fu8 | 0xb0, "64", &[8i16, 16, 32][..]),
    ] {
        for &bits in sizes {
            let form_name = format!("movsx{width}_s{bits}");
            let mut rng = Rng::new(seed_of(&form_name));
            let mut cases = Vec::new();
            for (index, (a, b)) in reg_operand_pairs(&mut rng, scale / 4)
                .into_iter()
                .enumerate()
                .filter(|(index, _)| index % 4 == 0 || *index >= VALUES64.len().pow(2))
            {
                let (dst, src) = REG_PAIRS[index % REG_PAIRS.len()];
                let regs = registers_with(&mut rng, &[(dst, a), (src, b)]);
                cases.push(straight(regs, vec![Insn::new(class, dst, src, bits, 0)]));
            }
            forms.push(Form {
                name: form_name,
                cases,
                subprograms: Vec::new(),
            });
        }
    }

    // Byte-order conversions.
    for (opc, name) in [(0xd4u8, "le"), (0xdcu8, "be"), (0xd7u8, "bswap")] {
        for bits in [16, 32, 64] {
            let form_name = format!("{name}{bits}");
            let mut rng = Rng::new(seed_of(&form_name));
            let mut cases = Vec::new();
            let extra: Vec<u64> = (0..scale / 8).map(|_| rng.next_u64()).collect();
            for value in VALUES64.iter().copied().chain(extra) {
                let dst = rng.pick(&[0u8, 3, 9]);
                let regs = registers_with(&mut rng, &[(dst, value)]);
                cases.push(straight(regs, vec![Insn::new(opc, dst, 0, 0, bits)]));
            }
            forms.push(Form {
                name: form_name,
                cases,
                subprograms: Vec::new(),
            });
        }
    }

    // LDDW with src = 0: a 64-bit immediate in two slots.
    let form_name = "lddw".to_string();
    let mut rng = Rng::new(seed_of(&form_name));
    let mut cases = Vec::new();
    let extra: Vec<u64> = (0..scale / 4).map(|_| rng.next_u64()).collect();
    for value in VALUES64.iter().copied().chain(extra) {
        let dst = rng.pick(&[0u8, 5, 9]);
        let regs = registers_with(&mut rng, &[]);
        cases.push(straight(
            regs,
            vec![
                Insn::new(0x18, dst, 0, 0, value as u32 as i32),
                Insn::new(0, 0, 0, 0, (value >> 32) as u32 as i32),
            ],
        ));
    }
    forms.push(Form {
        name: form_name,
        cases,
        subprograms: Vec::new(),
    });
    forms
}

const JUMP_CONDS: [(&str, u8); 11] = [
    ("jeq", 0x10),
    ("jgt", 0x20),
    ("jge", 0x30),
    ("jset", 0x40),
    ("jne", 0x50),
    ("jsgt", 0x60),
    ("jsge", 0x70),
    ("jlt", 0xa0),
    ("jle", 0xb0),
    ("jslt", 0xc0),
    ("jsle", 0xd0),
];

fn branch(regs: [u64; 10], body: Vec<Insn>, wide_join: bool) -> Case {
    Case {
        regs,
        window: None,
        body,
        shape: Shape::Branch,
        pointers: 0,
        wide_join,
    }
}

fn jump_forms(scale: usize) -> Vec<Form> {
    let mut forms = Vec::new();
    for (class, width) in [(0x05u8, ""), (0x06u8, "32")] {
        for (name, code) in JUMP_CONDS {
            let form_name = format!("{name}{width}_reg");
            let mut rng = Rng::new(seed_of(&form_name));
            let mut cases = Vec::new();
            for (index, (a, b)) in reg_operand_pairs(&mut rng, scale).into_iter().enumerate() {
                let (dst, src) = REG_PAIRS[index % REG_PAIRS.len()];
                // Bias the second operand toward equality and near-equality.
                let b = match index % 5 {
                    0 => a,
                    1 => a.wrapping_add(1),
                    _ => b,
                };
                let regs = registers_with(&mut rng, &[(dst, a), (src, b)]);
                let insn = Insn::new(class | 0x08 | code, dst, src, 0, 0);
                cases.push(branch(regs, vec![insn], index % 7 == 0));
            }
            forms.push(Form {
                name: form_name,
                cases,
                subprograms: Vec::new(),
            });

            let form_name = format!("{name}{width}_imm");
            let mut rng = Rng::new(seed_of(&form_name));
            let mut cases = Vec::new();
            for (index, (a, imm)) in imm_operand_pairs(&mut rng, scale).into_iter().enumerate() {
                let dst = REG_PAIRS[index % REG_PAIRS.len()].0;
                // Bias the register toward the immediate's 32- and 64-bit images.
                let a = match index % 5 {
                    0 => imm as i64 as u64,
                    1 => u64::from(imm as u32) | (a & 0xffff_ffff_0000_0000),
                    _ => a,
                };
                let regs = registers_with(&mut rng, &[(dst, a)]);
                let insn = Insn::new(class | code, dst, 0, 0, imm);
                cases.push(branch(regs, vec![insn], index % 7 == 0));
            }
            forms.push(Form {
                name: form_name,
                cases,
                subprograms: Vec::new(),
            });
        }
    }
    forms
}

fn window(rng: &mut Rng) -> [u8; WINDOW_LEN] {
    let mut bytes = [0; WINDOW_LEN];
    for chunk in bytes.chunks_mut(8) {
        let value = match rng.next_u64() % 4 {
            0 => rng.pick(&VALUES64),
            _ => rng.next_u64(),
        };
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// Memory case with base register `base` addressing the window middle.
fn memory_case(rng: &mut Rng, base: u8, body_tail: Vec<Insn>, assignments: &[(u8, u64)]) -> Case {
    let mut body = vec![mov64_reg(base, 10), add64_imm(base, i32::from(WINDOW_BASE))];
    body.extend(body_tail);
    let base_is_pointer = !body.iter().skip(2).any(|insn| writes_register(insn, base));
    Case {
        regs: registers_with(rng, assignments),
        window: Some(window(rng)),
        body,
        shape: Shape::Straight,
        pointers: if base_is_pointer { 1 << base } else { 0 },
        wide_join: false,
    }
}

fn writes_register(insn: &Insn, reg: u8) -> bool {
    match insn.opc & 0x07 {
        0x01 => insn.dst == reg,
        // BPF_ATOMIC: FETCH forms and XCHG write src; CMPXCHG writes r0.
        0x03 if insn.opc & 0xe0 == 0xc0 => {
            if insn.imm == 0xf1 {
                reg == 0
            } else {
                insn.imm & 0x01 != 0 && insn.src == reg
            }
        }
        _ => false,
    }
}

/// Aligned offsets (stack accesses must be naturally aligned) relative to
/// the window middle, spanning the whole 64-byte window.
fn aligned_offsets(width: i16) -> Vec<i16> {
    (-32..32).step_by(width as usize).collect()
}

fn memory_forms(scale: usize, endian: Endian) -> Vec<Form> {
    let mut forms = Vec::new();
    let widths: [(&str, u8, i16); 4] = [
        ("w", 0x00, 4),
        ("h", 0x08, 2),
        ("b", 0x10, 1),
        ("dw", 0x18, 8),
    ];
    let pairs: [(u8, u8); 4] = [(1, 2), (3, 3), (0, 9), (7, 6)];

    for (suffix, size, width) in widths {
        for (mode, prefix, sign) in [(0x60u8, "ldx", false), (0x80u8, "ldxs", true)] {
            if sign && size == 0x18 {
                continue;
            }
            let form_name = format!("{prefix}{suffix}");
            let mut rng = Rng::new(seed_of(&form_name));
            let mut cases = Vec::new();
            for repeat in 0..(4 + scale / 64) {
                for off in aligned_offsets(width) {
                    // Register-pair rotation; the wrap of a negative offset
                    // is part of the recorded stream.
                    let (dst, base) = pairs[repeat.wrapping_add(off as usize) % pairs.len()];
                    let insn = Insn::new(0x01 | mode | size, dst, base, off, 0);
                    cases.push(memory_case(&mut rng, base, vec![insn], &[]));
                }
            }
            forms.push(Form {
                name: form_name,
                cases,
                subprograms: Vec::new(),
            });
        }

        let form_name = format!("st{suffix}");
        let mut rng = Rng::new(seed_of(&form_name));
        let mut cases = Vec::new();
        for (index, imm) in VALUES32.iter().enumerate() {
            for off in aligned_offsets(width).into_iter().step_by(3) {
                let base = pairs[index % pairs.len()].1;
                let insn = Insn::new(0x02 | 0x60 | size, base, 0, off, *imm);
                cases.push(memory_case(&mut rng, base, vec![insn], &[]));
            }
        }
        forms.push(Form {
            name: form_name,
            cases,
            subprograms: Vec::new(),
        });

        let form_name = format!("stx{suffix}");
        let mut rng = Rng::new(seed_of(&form_name));
        let mut cases = Vec::new();
        for (index, value) in VALUES64.iter().enumerate() {
            for off in aligned_offsets(width).into_iter().step_by(3) {
                let (src, base) = [(2u8, 1u8), (9, 0), (5, 6)][index % 3];
                let insn = Insn::new(0x03 | 0x60 | size, base, src, off, 0);
                cases.push(memory_case(&mut rng, base, vec![insn], &[(src, *value)]));
            }
        }
        forms.push(Form {
            name: form_name,
            cases,
            subprograms: Vec::new(),
        });
    }

    // Atomic read-modify-write on the stack window, both widths, all ops.
    let atomic_ops: [(&str, i32); 10] = [
        ("add", 0x00),
        ("fetch_add", 0x01),
        ("or", 0x40),
        ("fetch_or", 0x41),
        ("and", 0x50),
        ("fetch_and", 0x51),
        ("xor", 0xa0),
        ("fetch_xor", 0xa1),
        ("xchg", 0xe1),
        ("cmpxchg", 0xf1),
    ];
    for (opc, width, suffix) in [(0xdbu8, 8i16, "64"), (0xc3u8, 4i16, "32")] {
        for (name, imm) in atomic_ops {
            let form_name = format!("atomic{suffix}_{name}");
            let mut rng = Rng::new(seed_of(&form_name));
            let mut cases = Vec::new();
            let extra: Vec<u64> = (0..scale / 4).map(|_| rng.next_u64()).collect();
            for (index, operand) in VALUES64.iter().copied().chain(extra).enumerate() {
                let off = aligned_offsets(width)[index % aligned_offsets(width).len()];
                let (base, src) = [(1u8, 2u8), (3, 4), (6, 9), (7, 8)][index % 4];
                let mut case = memory_case(
                    &mut rng,
                    base,
                    vec![Insn::new(opc, base, src, off, imm)],
                    &[(src, operand)],
                );
                if imm == 0xf1 {
                    // Bias r0 to the current memory word so both CAS outcomes occur.
                    let window = case.window.expect("window");
                    let at = (32 + off) as usize;
                    let current = match (width, endian) {
                        (8, Endian::Little) => {
                            u64::from_le_bytes(window[at..at + 8].try_into().expect("8 bytes"))
                        }
                        (8, Endian::Big) => {
                            u64::from_be_bytes(window[at..at + 8].try_into().expect("8 bytes"))
                        }
                        (_, Endian::Little) => u64::from(u32::from_le_bytes(
                            window[at..at + 4].try_into().expect("4 bytes"),
                        )),
                        (_, Endian::Big) => u64::from(u32::from_be_bytes(
                            window[at..at + 4].try_into().expect("4 bytes"),
                        )),
                    };
                    case.regs[0] = match index % 3 {
                        0 => current,
                        1 => current ^ 0xffff_ffff_0000_0000,
                        _ => rng.next_u64(),
                    };
                }
                cases.push(case);
            }
            forms.push(Form {
                name: form_name,
                cases,
                subprograms: Vec::new(),
            });
        }
    }
    forms
}

fn call_forms(scale: usize) -> Vec<Form> {
    // Subprogram 0: r0 = r1 + 2*r2 - r3 ^ r4 | r5; clobber r6..r9 and r1..r5.
    let sub0 = vec![
        mov64_reg(0, 1),
        Insn::new(0x0f, 0, 2, 0, 0),
        Insn::new(0x0f, 0, 2, 0, 0),
        Insn::new(0x1f, 0, 3, 0, 0),
        Insn::new(0xaf, 0, 4, 0, 0),
        Insn::new(0x4f, 0, 5, 0, 0),
        mov64_imm(6, -6),
        mov64_imm(7, -7),
        mov64_imm(8, -8),
        mov64_imm(9, -9),
        mov64_imm(1, 0),
        mov64_imm(2, 0),
        EXIT,
    ];
    // Subprogram 1: store r2 through the caller's stack pointer in r1, then
    // nested-call subprogram 2 and add its result.
    let sub1 = vec![
        stx64(1, 8, 2),
        Insn::new(0x63, 1, 3, -4, 0),
        mov64_reg(6, 2),
        mov64_reg(1, 3),
        // Local call to subprogram 2 at index 7: 7 - (4 + 1) = 2 slots.
        Insn::new(0x85, 0, 1, 0, 2),
        Insn::new(0x0f, 0, 6, 0, 0),
        EXIT,
        // Subprogram 2 (nested): r0 = r1 * 3 with a callee-saved clobber.
        mov64_reg(0, 1),
        Insn::new(0x27, 0, 0, 0, 3),
        mov64_imm(6, 1234),
        EXIT,
    ];
    let mut forms = Vec::new();
    let form_name = "call_local".to_string();
    let mut rng = Rng::new(seed_of(&form_name));
    let mut cases = Vec::new();
    for _ in 0..(32 + scale / 4) {
        let regs = registers_with(&mut rng, &[]);
        cases.push(Case {
            regs,
            window: None,
            body: vec![Insn::new(0x85, 0, 1, 0, 0)],
            shape: Shape::Call(0),
            pointers: 0,
            wide_join: false,
        });
    }
    forms.push(Form {
        name: form_name,
        cases,
        subprograms: vec![sub0],
    });

    let form_name = "call_stack_pointer_nested".to_string();
    let mut rng = Rng::new(seed_of(&form_name));
    let mut cases = Vec::new();
    for _ in 0..(32 + scale / 4) {
        let mut case = memory_case(&mut rng, 1, vec![Insn::new(0x85, 0, 1, 0, 0)], &[]);
        case.shape = Shape::Call(0);
        cases.push(case);
    }
    forms.push(Form {
        name: form_name,
        cases,
        subprograms: vec![sub1],
    });
    forms
}

/// Every campaign form. `scale` controls the random operand count per form
/// in addition to the full boundary cross product. Byte order only biases
/// CMPXCHG expected values toward the guest-order memory word.
pub fn forms(scale: usize, endian: Endian) -> Vec<Form> {
    let mut forms = alu_forms(scale);
    forms.extend(jump_forms(scale));
    forms.extend(memory_forms(scale, endian));
    forms.extend(call_forms(scale));
    forms
}

/// Serialize programs for `kernel/harness.c`.
pub fn harness_input(programs: &[Program], endian: Endian) -> Vec<u8> {
    let mut out = b"RAXBPFIN".to_vec();
    out.extend_from_slice(&(programs.len() as u32).to_le_bytes());
    for program in programs {
        out.extend_from_slice(&(program.insns.len() as u32).to_le_bytes());
        out.extend_from_slice(&(program.ctx.len() as u32).to_le_bytes());
        for insn in &program.insns {
            out.extend_from_slice(&insn.encode(endian));
        }
        out.extend_from_slice(&program.ctx);
    }
    out
}

/// One program's kernel observation.
#[derive(Debug, Clone)]
pub struct KernelResult {
    pub load_status: i32,
    pub run_status: i32,
    pub retval: u32,
    pub log: String,
    pub ctx: Vec<u8>,
}

/// Parse `kernel/harness.c` output.
pub fn parse_harness_output(bytes: &[u8]) -> Result<Vec<KernelResult>, String> {
    let mut cursor = 0usize;
    let mut take = |len: usize| -> Result<&[u8], String> {
        let slice = bytes
            .get(cursor..cursor + len)
            .ok_or_else(|| format!("truncated harness output at byte {cursor}"))?;
        cursor += len;
        Ok(slice)
    };
    if take(8)? != b"RAXBPFOU" {
        return Err("bad harness output magic".to_string());
    }
    let u32_at = |slice: &[u8]| u32::from_le_bytes(slice.try_into().expect("4 bytes"));
    let count = u32_at(take(4)?);
    let mut results = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let load_status = u32_at(take(4)?) as i32;
        let run_status = u32_at(take(4)?) as i32;
        let retval = u32_at(take(4)?);
        let log_len = u32_at(take(4)?) as usize;
        let log = String::from_utf8_lossy(take(log_len)?).into_owned();
        let ctx_len = u32_at(take(4)?) as usize;
        let ctx = take(ctx_len)?.to_vec();
        results.push(KernelResult {
            load_status,
            run_status,
            retval,
            log,
            ctx,
        });
    }
    Ok(results)
}

pub fn describe_field(offset: usize) -> String {
    if offset < OUTPUT_REGS {
        format!("r{}", offset / 8)
    } else if offset < OUTPUT_MARKER {
        format!("window[{}]", offset - OUTPUT_REGS)
    } else {
        "branch marker".to_string()
    }
}

/// Human-readable description of one case for mismatch reports.
pub fn describe_case(forms: &[Form], form: &str, index: usize, endian: Endian) -> String {
    let Some(form) = forms.iter().find(|candidate| candidate.name == form) else {
        return format!("unknown form {form}");
    };
    let case = &form.cases[index];
    let body: Vec<String> = case
        .body
        .iter()
        .map(|insn| {
            insn.encode(endian)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        })
        .collect();
    format!(
        "body=[{}] regs={:#x?} window={:02x?}",
        body.join(" "),
        case.regs,
        case.window.map(|w| w.to_vec())
    )
}

/// Order-sensitive 128-bit digest (two FNV-1a 64 lanes). It detects any
/// accidental difference; it is not an adversarial hash.
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

/// Digest of one program's observable kernel state: the 32-bit test-run
/// return value and the complete context buffer after the run.
pub fn program_digest(retval: u32, ctx: &[u8]) -> Digest {
    let mut digest = Digest::default();
    digest.update(&retval.to_le_bytes());
    digest.update(ctx);
    digest
}

/// Campaign scale recorded in the checked-in kernel corpora.
pub const CORPUS_SCALE: usize = 0;

// ============================================================================
// Kernel-verifier admission campaign
// ============================================================================

/// Register fields: valid low/high registers, the frame pointer, and invalid.
pub const ADMISSION_REGISTERS: [u8; 7] = [0, 1, 2, 9, 10, 11, 15];
/// Offsets: zero, unit, aligned stack slots, positive frame-escaping values,
/// and the maximum.
pub const ADMISSION_OFFSETS: [i16; 10] = [0, 1, -1, -8, -16, 8, 16, 32, 64, 0x7fff];
/// Immediates: zero, unit, all-ones, byte-order widths, atomic operations,
/// Linux load-acquire/store-release codes, and range extremes.
pub const ADMISSION_IMMEDIATES: [i32; 14] = [
    0,
    1,
    -1,
    8,
    16,
    32,
    64,
    0x51,
    0xe1,
    0xf1,
    0x100,
    0x110,
    0x7fff_ffff,
    -64,
];

/// Immediates of the complete register grid: zero (every LDDW subtype and
/// CALL kind with a null operand), unit, and all-ones.
pub const ADMISSION_GRID_IMMEDIATES: [i32; 3] = [0, 1, -1];

/// Every opcode crossed with the field partitions; LDDW also varies its
/// second slot (canonical, non-zero opcode, registers, or offset). A second
/// pass crosses every opcode with all 16 x 16 register-field values, so the
/// `src_reg` subtypes of LDDW (0x0-0x6) and CALL (0x0-0x2) and every
/// register number meet the verifier.
pub fn admission_candidates() -> Vec<Vec<Insn>> {
    let mut out = admission_partition_candidates();
    for opc in 0..=255u8 {
        for dst in 0..16u8 {
            for src in 0..16u8 {
                for imm in ADMISSION_GRID_IMMEDIATES {
                    let first = Insn::new(opc, dst, src, 0, imm);
                    if opc == 0x18 {
                        out.push(vec![first, Insn::new(0, 0, 0, 0, imm)]);
                    } else {
                        out.push(vec![first]);
                    }
                }
            }
        }
    }
    out
}

fn admission_partition_candidates() -> Vec<Vec<Insn>> {
    let mut out = Vec::new();
    for opc in 0..=255u8 {
        for dst in ADMISSION_REGISTERS {
            for src in ADMISSION_REGISTERS {
                for off in ADMISSION_OFFSETS {
                    for imm in ADMISSION_IMMEDIATES {
                        let first = Insn::new(opc, dst, src, off, imm);
                        if opc == 0x18 {
                            for second in [
                                Insn::new(0, 0, 0, 0, imm),
                                Insn::new(0x18, 0, 0, 0, imm),
                                Insn::new(0, 1, 0, 0, imm),
                                Insn::new(0, 0, 1, 0, imm),
                                Insn::new(0, 0, 0, 1, imm),
                            ] {
                                out.push(vec![first, second]);
                            }
                        } else {
                            out.push(vec![first]);
                        }
                    }
                }
            }
        }
    }
    out
}

/// The admission program: initialize `r0..r9` and eight stack slots, place
/// the candidate, then return 0. The fall-through after the candidate is
/// `mov64 r0, 0`, which the rax verdict also decodes after a narrow slot.
pub fn admission_program(candidate: &[Insn]) -> Vec<Insn> {
    let mut insns: Vec<Insn> = (0..10).map(|reg| Insn::new(0xb7, reg, 0, 0, 0)).collect();
    for slot in 1..=8i16 {
        insns.push(Insn::new(0x7b, 10, 0, -8 * slot, 0));
    }
    insns.extend_from_slice(candidate);
    insns.push(Insn::new(0xb7, 0, 0, 0, 0));
    insns.push(Insn::new(0x95, 0, 0, 0, 0));
    insns
}

/// The rax RFC 9669 decoder verdict for one candidate: `None` when it
/// decodes, otherwise the error variant name.
pub fn admission_verdict(candidate: &[Insn], endian: Endian) -> Option<String> {
    let mut bytes: Vec<u8> = candidate
        .iter()
        .flat_map(|insn| insn.encode(endian))
        .collect();
    bytes.extend_from_slice(&Insn::new(0xb7, 0, 0, 0, 0).encode(endian));
    match rax::isa::ebpf::decode_insn(&bytes, EbpfDialect::Rfc9669, endian) {
        Ok(_) => None,
        Err(error) => Some(
            format!("{error:?}")
                .split([' ', '{', '('])
                .next()
                .unwrap_or("")
                .to_string(),
        ),
    }
}

/// Digest of the decoder verdicts over every admission candidate, in order.
pub fn admission_digest(endian: Endian) -> (usize, Digest) {
    let candidates = admission_candidates();
    let mut digest = Digest::default();
    for candidate in &candidates {
        let verdict = admission_verdict(candidate, endian);
        digest.update(verdict.as_deref().unwrap_or("decodes").as_bytes());
    }
    (candidates.len(), digest)
}
