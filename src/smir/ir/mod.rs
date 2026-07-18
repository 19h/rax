//! SMIR IR structures.
//!
//! This module defines the hierarchical IR structure:
//! - Module: top-level container
//! - Function: a lifted function
//! - Block: a basic block
//! - Terminator: block terminators (branches, returns, etc.)

pub mod context;
pub mod flags;
pub mod memory;
pub mod ops;
pub mod types;

use std::collections::HashMap;

use crate::smir::ir::ops::SmirOp;
use crate::smir::ir::types::*;

// ============================================================================
// Module
// ============================================================================

/// Top-level IR module containing all lifted code
#[derive(Clone, Debug)]
pub struct SmirModule {
    /// Unique module identifier
    pub id: ModuleId,
    /// Source architecture
    pub source_arch: SourceArch,
    /// Functions in this module
    pub functions: Vec<SmirFunction>,
    /// Global symbol table (name -> address)
    pub symbols: HashMap<String, GuestAddr>,
    /// External references (imports)
    pub externals: Vec<ExternalRef>,
    /// Module-level metadata
    pub metadata: ModuleMetadata,
}

impl SmirModule {
    /// Create a new empty module
    pub fn new(id: ModuleId, source_arch: SourceArch) -> Self {
        SmirModule {
            id,
            source_arch,
            functions: Vec::new(),
            symbols: HashMap::new(),
            externals: Vec::new(),
            metadata: ModuleMetadata::default(),
        }
    }

    /// Add a function to the module
    pub fn add_function(&mut self, func: SmirFunction) {
        self.functions.push(func);
    }

    /// Find a function by its entry address
    pub fn find_function(&self, addr: GuestAddr) -> Option<&SmirFunction> {
        self.functions
            .iter()
            .find(|f| f.guest_range.0 <= addr && addr < f.guest_range.1)
    }

    /// Find a function by its ID
    pub fn get_function(&self, id: FunctionId) -> Option<&SmirFunction> {
        self.functions.iter().find(|f| f.id == id)
    }
}

/// External reference (import)
#[derive(Clone, Debug)]
pub struct ExternalRef {
    /// Symbol name
    pub name: String,
    /// Expected address (if known)
    pub addr: Option<GuestAddr>,
}

/// Module metadata
#[derive(Clone, Debug, Default)]
pub struct ModuleMetadata {
    /// Guest entry point address
    pub entry_point: Option<GuestAddr>,
    /// Guest stack base (if known)
    pub stack_base: Option<GuestAddr>,
    /// Text section range
    pub text_range: Option<(GuestAddr, GuestAddr)>,
}

// ============================================================================
// Function
// ============================================================================

/// A lifted function
#[derive(Clone, Debug)]
pub struct SmirFunction {
    /// Function identifier
    pub id: FunctionId,
    /// Entry block ID
    pub entry: BlockId,
    /// All basic blocks
    pub blocks: Vec<SmirBlock>,
    /// Local variable slots (spills, temporaries)
    pub locals: Vec<LocalSlot>,
    /// Guest address range covered
    pub guest_range: (GuestAddr, GuestAddr),
    /// Calling convention
    pub calling_convention: CallingConv,
    /// Function attributes
    pub attrs: FunctionAttrs,
    /// Exact source bytes for x86 instructions retained in this function,
    /// keyed by `(block, guest PC)`. These bytes are provenance metadata: the
    /// interpreter and architecture-independent optimizers use the semantic
    /// SMIR operations, while the same-architecture x86 lowerer may replay a
    /// narrowly validated register-only instruction to avoid materializing
    /// otherwise unrepresentable vector temporaries.
    pub x86_instruction_bytes: HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
}

impl SmirFunction {
    /// Create a new function
    pub fn new(id: FunctionId, entry: BlockId, guest_start: GuestAddr) -> Self {
        SmirFunction {
            id,
            entry,
            blocks: Vec::new(),
            locals: Vec::new(),
            guest_range: (guest_start, guest_start),
            calling_convention: CallingConv::GuestPreserveAll,
            attrs: FunctionAttrs::default(),
            x86_instruction_bytes: HashMap::new(),
        }
    }

    /// Add a block to the function
    pub fn add_block(&mut self, block: SmirBlock) {
        // Update guest range
        if block.guest_pc < self.guest_range.0 {
            self.guest_range.0 = block.guest_pc;
        }
        if block.guest_pc > self.guest_range.1 {
            self.guest_range.1 = block.guest_pc;
        }
        self.blocks.push(block);
    }

    /// Get a block by ID
    pub fn get_block(&self, id: BlockId) -> Option<&SmirBlock> {
        self.blocks.iter().find(|b| b.id == id)
    }

    /// Get a mutable block by ID
    pub fn get_block_mut(&mut self, id: BlockId) -> Option<&mut SmirBlock> {
        self.blocks.iter_mut().find(|b| b.id == id)
    }

    /// Get the entry block
    pub fn entry_block(&self) -> Option<&SmirBlock> {
        self.get_block(self.entry)
    }

    /// Total number of operations across all blocks
    pub fn op_count(&self) -> usize {
        self.blocks.iter().map(|b| b.ops.len()).sum()
    }
}

/// Exact bytes of one x86 instruction. Architectural x86 instructions are at
/// most 15 bytes; keeping a fixed-size value makes function provenance cheap to
/// clone and prevents metadata from carrying an unbounded byte sequence into a
/// native lowerer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct X86InstructionBytes {
    bytes: [u8; 15],
    len: u8,
}

impl X86InstructionBytes {
    /// Capture one complete x86 instruction.
    pub fn new(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() || bytes.len() > 15 {
            return None;
        }
        let mut captured = [0u8; 15];
        captured[..bytes.len()].copy_from_slice(bytes);
        Some(Self {
            bytes: captured,
            len: bytes.len() as u8,
        })
    }

    /// Return the complete instruction byte sequence.
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    /// Validate the initial native-replay family and return whether its vector
    /// length requires AVX-512VL in addition to AVX-512F. The admitted set is
    /// exactly register-source EVEX VADD*/VMUL*/VSUB*/VMIN*/VDIV*/VMAX* over
    /// binary32/binary64 packed or scalar elements, without EVEX.b embedded
    /// rounding/SAE. Every structural and reserved field relevant to this set
    /// is checked so fabricated metadata fails closed.
    pub fn evex_register_fp_arithmetic_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];

        // Map 1 (0F), EVEX.P1's fixed-one bit, and a register ModR/M source.
        if p0 & 0x0f != 1 || p1 & 0x04 == 0 || modrm >> 6 != 3 {
            return None;
        }
        if !matches!(opcode, 0x58 | 0x59 | 0x5c | 0x5d | 0x5e | 0x5f) {
            return None;
        }

        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        // PS/SS use W0; PD/SD use W1.
        if w != matches!(pp, 1 | 3) {
            return None;
        }
        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }

        let scalar = matches!(pp, 2 | 3);
        if scalar {
            (ll == 0).then_some(false)
        } else {
            match ll {
                0 | 1 => Some(true),
                2 => Some(false),
                _ => None,
            }
        }
    }

    /// Validate register-only EVEX packed logical operations and return
    /// `(needs AVX-512VL, needs AVX-512DQ)`. Floating logical VAND*/VANDN*/
    /// VOR*/VXOR* forms use AVX-512DQ; integer VPANDD/Q, VPANDND/Q, VPORD/Q,
    /// and VPXORD/Q forms use AVX-512F. Memory, EVEX.b, reserved vector lengths,
    /// and malformed masking encodings are rejected.
    pub fn evex_register_logic_requirements(&self) -> Option<(bool, bool)> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p0 & 0x0f != 1 || p1 & 0x04 == 0 || modrm >> 6 != 3 {
            return None;
        }

        let pp = p1 & 0x03;
        let w = p1 & 0x80 != 0;
        let needs_avx512dq = match opcode {
            0x54..=0x57 if matches!(pp, 0 | 1) && w == (pp == 1) => true,
            0xDB | 0xDF | 0xEB | 0xEF if pp == 1 => false,
            _ => return None,
        };
        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        let needs_avx512vl = match ll {
            0 | 1 => true,
            2 => false,
            _ => return None,
        };
        Some((needs_avx512vl, needs_avx512dq))
    }

    /// Validate register-only EVEX packed integer additions/subtractions and
    /// return whether the vector length requires AVX-512VL. Byte/word and all
    /// saturating forms use AVX-512BW; doubleword/quadword wrapping forms use
    /// AVX-512F. The native vector-state trampoline already requires both
    /// feature sets, so only the additional VL requirement is returned here.
    /// Memory, EVEX.b, reserved vector lengths, and malformed masks fail closed.
    pub fn evex_register_integer_arithmetic_needs_vl(&self) -> Option<bool> {
        let bytes = self.as_slice();
        if bytes.len() != 6 || bytes[0] != 0x62 {
            return None;
        }
        let p0 = bytes[1];
        let p1 = bytes[2];
        let p2 = bytes[3];
        let opcode = bytes[4];
        let modrm = bytes[5];
        if p0 & 0x0f != 1 || p1 & 0x04 == 0 || modrm >> 6 != 3 || p1 & 0x03 != 1 {
            return None;
        }

        let w = p1 & 0x80 != 0;
        match opcode {
            // VPADDQ and VPSUBQ are W1; VPADDD and VPSUBD are W0.
            0xD4 | 0xFB if w => {}
            0xFA | 0xFE if !w => {}
            // Byte/word operations specify WIG.
            0xD8 | 0xD9 | 0xDC | 0xDD | 0xE8 | 0xE9 | 0xEC | 0xED | 0xF8 | 0xF9 | 0xFC | 0xFD => {}
            _ => return None,
        }

        let zeroing = p2 & 0x80 != 0;
        let ll = (p2 >> 5) & 0x03;
        let embedded_control = p2 & 0x10 != 0;
        let mask = p2 & 0x07;
        if embedded_control || (zeroing && mask == 0) {
            return None;
        }
        match ll {
            0 | 1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }
}

/// A contiguous semantic-op group that may be replaced by one exact native x86
/// instruction after byte-level validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct X86NativeReplaySpan {
    /// Exclusive semantic-op end index.
    pub end: usize,
    /// Exact source instruction to emit.
    pub instruction: X86InstructionBytes,
    /// Whether native execution requires AVX-512VL.
    pub needs_avx512vl: bool,
    /// Whether native execution requires AVX-512DQ.
    pub needs_avx512dq: bool,
}

/// Compatibility name for the first replay family.
pub type X86EvexFpReplaySpan = X86NativeReplaySpan;

fn x86_evex_replay_spans_where(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
    classify: impl Fn(&X86InstructionBytes) -> Option<(bool, bool)>,
) -> HashMap<usize, X86NativeReplaySpan> {
    let mut groups = HashMap::<GuestAddr, (usize, usize, bool)>::new();
    for (index, op) in block.ops.iter().enumerate() {
        groups
            .entry(op.guest_pc)
            .and_modify(|(_, end, contiguous)| {
                if *end != index {
                    *contiguous = false;
                }
                *end = index + 1;
            })
            .or_insert((index, index + 1, true));
    }

    groups
        .into_iter()
        .filter_map(|(guest_pc, (start, end, contiguous))| {
            if !contiguous {
                return None;
            }
            let instruction = *instruction_bytes.get(&(block.id, guest_pc))?;
            let (needs_avx512vl, needs_avx512dq) = classify(&instruction)?;
            Some((
                start,
                X86NativeReplaySpan {
                    end,
                    instruction,
                    needs_avx512vl,
                    needs_avx512dq,
                },
            ))
        })
        .collect()
}

/// Identify valid register-only EVEX floating-point replay groups in `block`.
/// Construction is O(N) time and O(P) space for N SMIR operations and P unique
/// guest PCs. A guest PC occurring in multiple non-contiguous groups is
/// rejected, preventing one source instruction from replacing reordered or
/// fabricated semantic fragments.
pub fn x86_evex_fp_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86EvexFpReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_fp_arithmetic_needs_vl()
            .map(|needs_vl| (needs_vl, false))
    })
}

/// Identify valid register-only EVEX logical replay groups in `block` in O(N)
/// time and O(P) space for N operations and P unique guest PCs.
pub fn x86_evex_logic_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction.evex_register_logic_requirements()
    })
}

/// Identify valid register-only EVEX packed integer arithmetic replay groups
/// in `block` in O(N) time and O(P) space for N operations and P unique guest
/// PCs.
pub fn x86_evex_integer_arithmetic_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        instruction
            .evex_register_integer_arithmetic_needs_vl()
            .map(|needs_vl| (needs_vl, false))
    })
}

/// Identify every validated native EVEX replay group in one O(N)-time,
/// O(P)-space block pass. Classifiers are intentionally disjoint and ordered
/// explicitly so adding a replay family does not add another scan of the SMIR
/// operation stream.
pub fn x86_evex_native_replay_spans(
    block: &SmirBlock,
    instruction_bytes: &HashMap<(BlockId, GuestAddr), X86InstructionBytes>,
) -> HashMap<usize, X86NativeReplaySpan> {
    x86_evex_replay_spans_where(block, instruction_bytes, |instruction| {
        if let Some(needs_vl) = instruction.evex_register_fp_arithmetic_needs_vl() {
            return Some((needs_vl, false));
        }
        if let Some(requirements) = instruction.evex_register_logic_requirements() {
            return Some(requirements);
        }
        instruction
            .evex_register_integer_arithmetic_needs_vl()
            .map(|needs_vl| (needs_vl, false))
    })
}

/// Calling convention
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CallingConv {
    /// Preserve all guest state
    #[default]
    GuestPreserveAll,
    /// x86_64 System V ABI
    X86SysV,
    /// x86_64 Windows ABI
    X86Win64,
    /// AArch64 AAPCS
    Aarch64Aapcs,
    /// Hexagon standard
    HexagonStd,
    /// RISC-V standard
    RiscVStd,
}

/// Local variable slot
#[derive(Clone, Debug)]
pub struct LocalSlot {
    pub id: LocalId,
    pub size: u32,
    pub align: u32,
}

/// Function attributes
#[derive(Clone, Copy, Debug, Default)]
pub struct FunctionAttrs {
    /// May not return (infinite loop, abort)
    pub no_return: bool,
    /// Has no side effects beyond return value
    pub pure_fn: bool,
    /// Entry point of the guest program
    pub is_entry: bool,
    /// Exception handler
    pub is_exception_handler: bool,
    /// Permit forwarding repeated ordinary loads within a basic block.
    ///
    /// This may be set only when every [`OpKind::Load`](crate::smir::ir::ops::OpKind::Load)
    /// in the function is non-faulting and non-volatile (no MMIO/device read),
    /// and an equal address, width, and extension mode returns the same value
    /// until an intervening SMIR memory write. Guest-memory JIT regions must
    /// normally leave this false because each read is architecturally visible.
    pub allow_redundant_load_elimination: bool,
    /// Preserve zero-operation `Return` blocks used as explicit interpreter
    /// handoff frontiers. Merging such a block into its predecessor would move
    /// the frontier to the predecessor and make the native runtime reject or
    /// re-execute otherwise valid work.
    pub preserve_interpreter_frontiers: bool,
}

// ============================================================================
// Block
// ============================================================================

/// A basic block
#[derive(Clone, Debug)]
pub struct SmirBlock {
    /// Block identifier
    pub id: BlockId,
    /// Guest PC at block entry
    pub guest_pc: GuestAddr,
    /// Phi nodes (for SSA, may be empty)
    pub phis: Vec<PhiNode>,
    /// Operations in this block
    pub ops: Vec<SmirOp>,
    /// Block terminator
    pub terminator: Terminator,
    /// Estimated execution count (for hot path detection)
    pub exec_count: u64,
}

impl SmirBlock {
    /// Create a new block
    pub fn new(id: BlockId, guest_pc: GuestAddr) -> Self {
        SmirBlock {
            id,
            guest_pc,
            phis: Vec::new(),
            ops: Vec::new(),
            terminator: Terminator::Unreachable,
            exec_count: 0,
        }
    }

    /// Add an operation to the block
    pub fn push_op(&mut self, op: SmirOp) {
        self.ops.push(op);
    }

    /// Set the terminator
    pub fn set_terminator(&mut self, term: Terminator) {
        self.terminator = term;
    }

    /// Get successor block IDs
    pub fn successors(&self) -> Vec<BlockId> {
        self.terminator.successors()
    }

    /// Check if block is empty (no ops, unreachable terminator)
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty() && matches!(self.terminator, Terminator::Unreachable)
    }
}

/// Phi node (SSA)
#[derive(Clone, Debug)]
pub struct PhiNode {
    pub dst: VReg,
    pub sources: Vec<(BlockId, VReg)>,
}

// ============================================================================
// Terminator
// ============================================================================

/// Block terminator
#[derive(Clone, Debug)]
pub enum Terminator {
    /// Unconditional branch
    Branch { target: BlockId },

    /// Conditional branch
    CondBranch {
        cond: VReg,
        true_target: BlockId,
        false_target: BlockId,
    },

    /// Multi-way branch (switch)
    Switch {
        index: VReg,
        targets: Vec<BlockId>,
        default: BlockId,
    },

    /// Indirect branch (computed goto)
    IndirectBranch {
        target: VReg,
        /// Possible targets (for analysis, may be incomplete)
        possible_targets: Vec<BlockId>,
    },

    /// Indirect branch through memory
    IndirectBranchMem {
        addr: Address,
        /// Possible targets (for analysis, may be incomplete)
        possible_targets: Vec<BlockId>,
    },

    /// Function return
    Return { values: Vec<VReg> },

    /// Call with continuation
    Call {
        target: CallTarget,
        args: Vec<VReg>,
        continuation: BlockId,
    },

    /// Tail call (no return)
    TailCall { target: CallTarget, args: Vec<VReg> },

    /// Trap (undefined, breakpoint, etc.)
    Trap { kind: TrapKind },

    /// Unreachable (for dead code)
    Unreachable,
}

impl Terminator {
    /// Get successor block IDs
    pub fn successors(&self) -> Vec<BlockId> {
        match self {
            Terminator::Branch { target } => vec![*target],
            Terminator::CondBranch {
                true_target,
                false_target,
                ..
            } => vec![*true_target, *false_target],
            Terminator::Switch {
                targets, default, ..
            } => {
                let mut v = targets.clone();
                v.push(*default);
                v
            }
            Terminator::IndirectBranch {
                possible_targets, ..
            } => possible_targets.clone(),
            Terminator::IndirectBranchMem {
                possible_targets, ..
            } => possible_targets.clone(),
            Terminator::Call { continuation, .. } => vec![*continuation],
            Terminator::Return { .. }
            | Terminator::TailCall { .. }
            | Terminator::Trap { .. }
            | Terminator::Unreachable => vec![],
        }
    }

    /// Check if this is a return
    pub fn is_return(&self) -> bool {
        matches!(self, Terminator::Return { .. })
    }

    /// Check if this is a trap
    pub fn is_trap(&self) -> bool {
        matches!(self, Terminator::Trap { .. })
    }

    /// Check if this terminates the function (no successors)
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Terminator::Return { .. }
                | Terminator::TailCall { .. }
                | Terminator::Trap { .. }
                | Terminator::Unreachable
        )
    }
}

/// Call target
#[derive(Clone, Debug)]
pub enum CallTarget {
    /// Direct call to known function
    Direct(FunctionId),
    /// Direct call to guest address
    GuestAddr(GuestAddr),
    /// Direct AArch32 interworking call. `addr` is the architectural target PC
    /// (with no state tag in bit 0), while `thumb` is the execution state the
    /// dispatcher must install before resuming the guest.
    GuestAddrInterworking { addr: GuestAddr, thumb: bool },
    /// Indirect call through register
    Indirect(VReg),
    /// AArch32 register interworking call. Bit 0 of the W32 target selects the
    /// execution state and is cleared from the architectural target PC.
    IndirectInterworking(VReg),
    /// Indirect call through memory
    IndirectMem(Address),
    /// External runtime function
    Runtime(RuntimeFunc),
}

/// Runtime helper functions
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeFunc {
    /// System call handler
    Syscall,
    /// Page fault handler
    PageFault,
    /// FP exception handler
    FpException,
    /// Undefined instruction handler
    Undefined,
    /// Debug breakpoint
    Breakpoint,
    /// Memory barrier (fence)
    MemoryBarrier,
    /// CPUID (x86)
    Cpuid,
    /// Read timestamp counter
    Rdtsc,
}

/// Trap kinds
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapKind {
    /// Debug breakpoint
    Breakpoint,
    /// Undefined instruction
    Undefined,
    /// Division by zero
    DivideByZero,
    /// Integer overflow
    Overflow,
    /// Bounds check failure
    Bounds,
    /// Invalid opcode
    InvalidOpcode,
    /// System call
    SystemCall,
    /// Halt and wait for interrupt
    Halt,
}

// ============================================================================
// Builder
// ============================================================================

/// Builder for constructing SMIR functions
pub struct FunctionBuilder {
    func: SmirFunction,
    current_block: BlockId,
    next_op_id: u16,
    block_alloc: BlockIdAllocator,
    vreg_alloc: VRegAllocator,
}

impl FunctionBuilder {
    /// Create a new function builder
    pub fn new(func_id: FunctionId, entry_pc: GuestAddr) -> Self {
        let mut block_alloc = BlockIdAllocator::new();
        let entry = block_alloc.alloc();

        let mut func = SmirFunction::new(func_id, entry, entry_pc);
        func.blocks.push(SmirBlock::new(entry, entry_pc));

        FunctionBuilder {
            func,
            current_block: entry,
            next_op_id: 0,
            block_alloc,
            vreg_alloc: VRegAllocator::new(),
        }
    }

    /// Allocate a new virtual register
    pub fn alloc_vreg(&mut self) -> VReg {
        self.vreg_alloc.alloc()
    }

    /// Create a new block and return its ID
    pub fn create_block(&mut self, guest_pc: GuestAddr) -> BlockId {
        let id = self.block_alloc.alloc();
        self.func.blocks.push(SmirBlock::new(id, guest_pc));
        id
    }

    /// Switch to a different block for appending ops
    pub fn switch_to_block(&mut self, block: BlockId) {
        self.current_block = block;
        self.next_op_id = 0;
    }

    /// Get the current block ID
    pub fn current_block(&self) -> BlockId {
        self.current_block
    }

    /// Push an operation to the current block
    pub fn push_op(&mut self, guest_pc: GuestAddr, kind: crate::smir::ir::ops::OpKind) {
        let op = SmirOp::new(OpId(self.next_op_id), guest_pc, kind);
        self.next_op_id += 1;

        if let Some(block) = self.func.get_block_mut(self.current_block) {
            block.push_op(op);
        }
    }

    /// Set the terminator for the current block
    pub fn set_terminator(&mut self, term: Terminator) {
        if let Some(block) = self.func.get_block_mut(self.current_block) {
            block.set_terminator(term);
        }
    }

    /// Finish building and return the function
    pub fn finish(self) -> SmirFunction {
        self.func
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::smir::ir::ops::OpKind;

    #[test]
    fn test_function_builder() {
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);

        let v0 = builder.alloc_vreg();
        let v1 = builder.alloc_vreg();
        let v2 = builder.alloc_vreg();

        builder.push_op(
            0x1000,
            OpKind::Mov {
                dst: v0,
                src: SrcOperand::imm(42),
                width: OpWidth::W64,
            },
        );

        builder.push_op(
            0x1004,
            OpKind::Add {
                dst: v2,
                src1: v0,
                src2: SrcOperand::Reg(v1),
                width: OpWidth::W64,
                flags: crate::smir::ir::flags::FlagUpdate::None,
            },
        );

        builder.set_terminator(Terminator::Return { values: vec![v2] });

        let func = builder.finish();

        assert_eq!(func.blocks.len(), 1);
        assert_eq!(func.blocks[0].ops.len(), 2);
        assert!(func.blocks[0].terminator.is_return());
    }

    #[test]
    fn test_terminator_successors() {
        let term = Terminator::CondBranch {
            cond: VReg::virt(0),
            true_target: BlockId(1),
            false_target: BlockId(2),
        };

        let succs = term.successors();
        assert_eq!(succs.len(), 2);
        assert!(succs.contains(&BlockId(1)));
        assert!(succs.contains(&BlockId(2)));

        let term = Terminator::Return { values: vec![] };
        assert!(term.successors().is_empty());
        assert!(term.is_terminal());
    }

    #[test]
    fn x86_evex_fp_replay_classifier_is_exact_and_fail_closed() {
        let valid = [
            (&[0x62, 0xF1, 0x6C, 0x48, 0x58, 0xCB][..], Some(false)),
            (&[0x62, 0xA1, 0x6C, 0xA1, 0x58, 0xCB][..], Some(true)),
            (&[0x62, 0xF1, 0x6E, 0x89, 0x58, 0xCB][..], Some(false)),
            (&[0x62, 0xF1, 0xEF, 0x09, 0x5F, 0xCB][..], Some(false)),
        ];
        for (bytes, expected) in valid {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .evex_register_fp_arithmetic_needs_vl(),
                expected,
                "{bytes:02X?}"
            );
        }

        let invalid: &[&[u8]] = &[
            &[0x62, 0xF1, 0x6C, 0x48, 0x58],       // incomplete
            &[0x62, 0xF5, 0x6C, 0x48, 0x58, 0xCB], // MAP5 / FP16
            &[0x62, 0xF1, 0x6C, 0x48, 0x58, 0x08], // memory source
            &[0x62, 0xF1, 0x6C, 0x58, 0x58, 0xCB], // EVEX.b
            &[0x62, 0xF1, 0x6C, 0x88, 0x58, 0xCB], // {z} with k0
            &[0x62, 0xF1, 0x68, 0x48, 0x58, 0xCB], // fixed-one bit clear
            &[0x62, 0xF1, 0xEC, 0x48, 0x58, 0xCB], // PS with W1
            &[0x62, 0xF1, 0x6D, 0x48, 0x58, 0xCB], // PD with W0
            &[0x62, 0xF1, 0x6C, 0x68, 0x58, 0xCB], // packed L'L=3
            &[0x62, 0xF1, 0x6E, 0x28, 0x58, 0xCB], // scalar L'L=1
            &[0x62, 0xF1, 0x6C, 0x48, 0x51, 0xCB], // different opcode
        ];
        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .evex_register_fp_arithmetic_needs_vl(),
                None,
                "{bytes:02X?}"
            );
        }
        assert!(X86InstructionBytes::new(&[]).is_none());
        assert!(X86InstructionBytes::new(&[0x90; 16]).is_none());
    }

    #[test]
    fn x86_evex_fp_replay_spans_require_block_provenance_and_contiguity() {
        let mut block = SmirBlock::new(BlockId(7), 0x1000);
        block.push_op(SmirOp::new(OpId(0), 0x1000, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(1), 0x1000, OpKind::Nop));
        block.push_op(SmirOp::new(OpId(2), 0x1006, OpKind::Nop));
        let instruction = X86InstructionBytes::new(&[0x62, 0xF1, 0x6C, 0x48, 0x58, 0xCB]).unwrap();
        let mut provenance = HashMap::from([((BlockId(7), 0x1000), instruction)]);

        let spans = x86_evex_fp_replay_spans(&block, &provenance);
        assert_eq!(spans.get(&0).map(|span| span.end), Some(2));

        provenance.clear();
        provenance.insert((BlockId(8), 0x1000), instruction);
        assert!(x86_evex_fp_replay_spans(&block, &provenance).is_empty());

        provenance.clear();
        provenance.insert((BlockId(7), 0x1000), instruction);
        block.push_op(SmirOp::new(OpId(3), 0x1000, OpKind::Nop));
        assert!(x86_evex_fp_replay_spans(&block, &provenance).is_empty());
    }

    #[test]
    fn x86_evex_logic_replay_classifier_tracks_vl_and_dq_exactly() {
        let valid = [
            (
                &[0x62, 0xF1, 0x7C, 0x09, 0x54, 0xC8][..],
                Some((true, true)),
            ),
            (
                &[0x62, 0xF1, 0xFD, 0x49, 0x57, 0xC8][..],
                Some((false, true)),
            ),
            (
                &[0x62, 0xA1, 0x7D, 0xA1, 0xEF, 0xCB][..],
                Some((true, false)),
            ),
            (
                &[0x62, 0xF1, 0xFD, 0x49, 0xDB, 0xCB][..],
                Some((false, false)),
            ),
        ];
        for (bytes, expected) in valid {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .evex_register_logic_requirements(),
                expected,
                "{bytes:02X?}"
            );
        }

        let invalid: &[&[u8]] = &[
            &[0x62, 0xF2, 0x7C, 0x09, 0x54, 0xC8], // wrong map
            &[0x62, 0xF1, 0x7C, 0x09, 0x54, 0x08], // memory source
            &[0x62, 0xF1, 0x7C, 0x19, 0x54, 0xC8], // EVEX.b
            &[0x62, 0xF1, 0x7C, 0x88, 0x54, 0xC8], // {z} with k0
            &[0x62, 0xF1, 0x7E, 0x09, 0x54, 0xC8], // floating F3 prefix
            &[0x62, 0xF1, 0xFC, 0x09, 0x54, 0xC8], // PS with W1
            &[0x62, 0xF1, 0x7C, 0x09, 0xDB, 0xC8], // integer without 66
            &[0x62, 0xF1, 0x7D, 0x69, 0xDB, 0xC8], // L'L=3
            &[0x62, 0xF1, 0x7D, 0x09, 0x58, 0xC8], // arithmetic opcode
        ];
        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .evex_register_logic_requirements(),
                None,
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn x86_evex_integer_arithmetic_replay_classifier_is_exact_and_fail_closed() {
        let valid = [
            (&[0x62, 0xF1, 0x75, 0x09, 0xFC, 0xC8][..], Some(true)),
            // WIG byte/word operations also admit W1.
            (&[0x62, 0xF1, 0xF5, 0x29, 0xE9, 0xC8][..], Some(true)),
            (&[0x62, 0xF1, 0x7D, 0x49, 0xFA, 0xC8][..], Some(false)),
            (&[0x62, 0xF1, 0xFD, 0xC9, 0xD4, 0xC8][..], Some(false)),
        ];
        for (bytes, expected) in valid {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .evex_register_integer_arithmetic_needs_vl(),
                expected,
                "{bytes:02X?}"
            );
        }

        let invalid: &[&[u8]] = &[
            &[0x62, 0xF2, 0x75, 0x09, 0xFC, 0xC8], // wrong map
            &[0x62, 0xF1, 0x75, 0x09, 0xFC, 0x08], // memory source
            &[0x62, 0xF1, 0x75, 0x19, 0xFC, 0xC8], // EVEX.b
            &[0x62, 0xF1, 0x75, 0x88, 0xFC, 0xC8], // {z} with k0
            &[0x62, 0xF1, 0x74, 0x09, 0xFC, 0xC8], // missing 66 prefix
            &[0x62, 0xF1, 0xFD, 0x09, 0xFA, 0xC8], // VPSUBD with W1
            &[0x62, 0xF1, 0x7D, 0x09, 0xD4, 0xC8], // VPADDQ with W0
            &[0x62, 0xF1, 0x75, 0x69, 0xFC, 0xC8], // L'L=3
            &[0x62, 0xF1, 0x75, 0x09, 0xDB, 0xC8], // logical opcode
        ];
        for bytes in invalid {
            assert_eq!(
                X86InstructionBytes::new(bytes)
                    .unwrap()
                    .evex_register_integer_arithmetic_needs_vl(),
                None,
                "{bytes:02X?}"
            );
        }
    }
}
