//! tests.rs

use crate::smir::lower::x86_64::*;

// ---- split test submodules ----
#[cfg(test)]
mod ac;
#[cfg(test)]
mod addr32_memory;
#[cfg(test)]
mod alu;
#[cfg(test)]
mod apx;
#[cfg(test)]
mod call_helpers;
#[cfg(test)]
mod cli;
#[cfg(test)]
mod clts;
#[cfg(test)]
mod cpuid;
#[cfg(test)]
mod descriptor_table;
#[cfg(test)]
mod far_call;
#[cfg(test)]
mod far_jump;
#[cfg(test)]
mod far_return;
#[cfg(test)]
mod fp_binary;
#[cfg(test)]
mod fsgsbase;
#[cfg(test)]
mod jit;
#[cfg(test)]
mod lmsw;
#[cfg(test)]
mod memory;
#[cfg(test)]
mod misc;
#[cfg(test)]
mod mmx_helpers;
#[cfg(all(test, feature = "smir-jit"))]
mod mmx_memory_source;
#[cfg(test)]
mod monitor_mwait;
#[cfg(test)]
mod msr;
#[cfg(test)]
mod pkru;
#[cfg(test)]
mod pmc;
#[cfg(test)]
mod push_segment;
#[cfg(test)]
mod read_control;
#[cfg(test)]
mod read_debug;
#[cfg(test)]
mod selector;
#[cfg(test)]
mod serialize;
#[cfg(test)]
mod simd;
#[cfg(test)]
mod smsw;
#[cfg(test)]
mod state;
#[cfg(test)]
mod sti;
#[cfg(test)]
mod swapgs;
#[cfg(test)]
mod timing;
#[cfg(test)]
mod tsx;
#[cfg(all(test, feature = "smir-jit"))]
mod vector_maskmov;
#[cfg(test)]
mod write_control;
#[cfg(test)]
mod write_debug;
use crate::smir::ir::flags::{FlagSet, FlagUpdate};
use crate::smir::ir::memory::MemoryError;
use crate::smir::ir::types::{
    Address, ArchReg, DispSize, FunctionId, OpWidth, SourceArch, SrcOperand, VReg, X86Reg,
};
use crate::smir::ir::{FunctionBuilder, SmirFunction, Terminator};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, MemoryReader, SmirLifter};

struct TestReader {
    base: u64,
    bytes: Vec<u8>,
}

impl MemoryReader for TestReader {
    fn read(&self, addr: u64, size: usize) -> Result<Vec<u8>, MemoryError> {
        let off = addr
            .checked_sub(self.base)
            .filter(|&off| (off as usize) < self.bytes.len())
            .ok_or(MemoryError::OutOfBounds { addr })? as usize;
        let n = (self.bytes.len() - off).min(size);
        Ok(self.bytes[off..off + n].to_vec())
    }
}

fn lower_rex2_block_with_options(
    bytes: &[u8],
    mem_helpers: bool,
    jit_fault_deopt_guards: bool,
) -> (Vec<u8>, usize) {
    let reader = TestReader {
        base: 0x1000,
        bytes: bytes.to_vec(),
    };
    let mut lifter = X86_64Lifter::strict();
    let mut lctx = LiftContext::new(SourceArch::X86_64);
    let mut block = lifter
        .lift_block(0x1000, &reader, &mut lctx)
        .expect("lift REX2 block");
    block.set_terminator(Terminator::Return { values: vec![] });
    let ops_debug = format!("{:?}", block.ops);
    let block_id = block.id;
    let mut func = SmirFunction::new(FunctionId(0), block_id, 0x1000);
    func.add_block(block);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(mem_helpers);
    lowerer.set_jit_fault_deopt_guards(jit_fault_deopt_guards);
    let res = lowerer.lower_function(&func).unwrap_or_else(|error| {
        panic!("lower REX2 block {bytes:02X?}: {error:?}; ops={ops_debug}")
    });
    assert!(res.relocations.is_empty(), "REX2 block should not relocate");
    (lowerer.finalize().expect("finalize"), res.entry_offset)
}

fn lower_rex2_block_with_mem_helpers(bytes: &[u8], mem_helpers: bool) -> (Vec<u8>, usize) {
    lower_rex2_block_with_options(bytes, mem_helpers, false)
}

fn lower_jit_guarded_x86_block(bytes: &[u8], mem_helpers: bool) -> (Vec<u8>, usize) {
    lower_rex2_block_with_options(bytes, mem_helpers, true)
}

fn lower_rex2_block(bytes: &[u8]) -> (Vec<u8>, usize) {
    lower_rex2_block_with_mem_helpers(bytes, false)
}

fn lower_rex2_block_err(bytes: &[u8]) -> LowerError {
    let reader = TestReader {
        base: 0x1000,
        bytes: bytes.to_vec(),
    };
    let mut lifter = X86_64Lifter::strict();
    let mut lctx = LiftContext::new(SourceArch::X86_64);
    let mut block = lifter
        .lift_block(0x1000, &reader, &mut lctx)
        .expect("lift REX2 block");
    block.set_terminator(Terminator::Return { values: vec![] });
    let block_id = block.id;
    let mut func = SmirFunction::new(FunctionId(0), block_id, 0x1000);
    func.add_block(block);

    let mut lowerer = X86_64Lowerer::new();
    lowerer
        .lower_function(&func)
        .expect_err("REX2 block should fail to lower")
}

fn lower_single_op(kind: OpKind) -> Vec<u8> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = X86_64Lowerer::new();
    let res = lowerer.lower_function(&func).expect("lower single op");
    assert!(res.relocations.is_empty(), "single op should not relocate");
    lowerer.finalize().expect("finalize")
}

fn lower_single_op_err(kind: OpKind) -> LowerError {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let func = builder.finish();

    let mut lowerer = X86_64Lowerer::new();
    lowerer
        .lower_function(&func)
        .expect_err("single op should fail to lower")
}

fn lower_single_hinted_op(kind: OpKind, hint: X86OpHint) -> Vec<u8> {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut func = builder.finish();
    func.blocks[0].ops[0].x86_hint = Some(hint);

    let mut lowerer = X86_64Lowerer::new();
    let result = lowerer
        .lower_function(&func)
        .expect("lower single hinted op");
    assert!(
        result.relocations.is_empty(),
        "single hinted op should not relocate"
    );
    lowerer.finalize().expect("finalize")
}

fn lower_single_hinted_op_err(kind: OpKind, hint: X86OpHint) -> LowerError {
    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(0x1000, kind);
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut func = builder.finish();
    func.blocks[0].ops[0].x86_hint = Some(hint);

    let mut lowerer = X86_64Lowerer::new();
    lowerer
        .lower_function(&func)
        .expect_err("single hinted op should fail to lower")
}
