//! End-to-end lowering equivalence for generation-dependent scalar VEX.L=1.

use super::*;
use crate::smir::ir::types::{BlockId, FunctionId, SourceArch};
use crate::smir::ir::{SmirBlock, SmirFunction, Terminator, X86InstructionBytes};
use crate::smir::lift::x86_64::X86_64Lifter;
use crate::smir::lift::{LiftContext, SmirLifter};
use crate::smir::lower::SmirLowerer;
use crate::smir::lower::x86_64::X86_64Lowerer;
use crate::smir::optimize::OptLevel;
use std::collections::HashMap;

const PC: u64 = 0xC410_1151;
const LEVELS: [OptLevel; 3] = [OptLevel::O0, OptLevel::O1, OptLevel::O2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScalarForm {
    name: &'static str,
    pp: u8,
    opcode: u8,
    immediate: Option<u8>,
    reserved_vvvv: bool,
}

const SCALAR_FORMS: [ScalarForm; 30] = [
    ScalarForm::new("VADDSS", 2, 0x58, false),
    ScalarForm::new("VADDSD", 3, 0x58, false),
    ScalarForm::new("VMULSS", 2, 0x59, false),
    ScalarForm::new("VMULSD", 3, 0x59, false),
    ScalarForm::new("VSUBSS", 2, 0x5C, false),
    ScalarForm::new("VSUBSD", 3, 0x5C, false),
    ScalarForm::new("VMINSS", 2, 0x5D, false),
    ScalarForm::new("VMINSD", 3, 0x5D, false),
    ScalarForm::new("VDIVSS", 2, 0x5E, false),
    ScalarForm::new("VDIVSD", 3, 0x5E, false),
    ScalarForm::new("VMAXSS", 2, 0x5F, false),
    ScalarForm::new("VMAXSD", 3, 0x5F, false),
    ScalarForm::with_immediate("VCMPSS", 2, 0xC2, 0x1F),
    ScalarForm::with_immediate("VCMPSD", 3, 0xC2, 0x1F),
    ScalarForm::new("VUCOMISS", 0, 0x2E, true),
    ScalarForm::new("VUCOMISD", 1, 0x2E, true),
    ScalarForm::new("VCOMISS", 0, 0x2F, true),
    ScalarForm::new("VCOMISD", 1, 0x2F, true),
    ScalarForm::new("VCVTSI2SS", 2, 0x2A, false),
    ScalarForm::new("VCVTSI2SD", 3, 0x2A, false),
    ScalarForm::new("VCVTTSS2SI", 2, 0x2C, true),
    ScalarForm::new("VCVTTSD2SI", 3, 0x2C, true),
    ScalarForm::new("VCVTSS2SI", 2, 0x2D, true),
    ScalarForm::new("VCVTSD2SI", 3, 0x2D, true),
    ScalarForm::new("VCVTSS2SD", 2, 0x5A, false),
    ScalarForm::new("VCVTSD2SS", 3, 0x5A, false),
    ScalarForm::new("VSQRTSS", 2, 0x51, false),
    ScalarForm::new("VSQRTSD", 3, 0x51, false),
    ScalarForm::new("VMOVSS load", 2, 0x10, false),
    ScalarForm::new("VMOVSS store", 2, 0x11, true),
];

impl ScalarForm {
    const fn new(name: &'static str, pp: u8, opcode: u8, reserved_vvvv: bool) -> Self {
        Self {
            name,
            pp,
            opcode,
            immediate: None,
            reserved_vvvv,
        }
    }

    const fn with_immediate(name: &'static str, pp: u8, opcode: u8, immediate: u8) -> Self {
        Self {
            name,
            pp,
            opcode,
            immediate: Some(immediate),
            reserved_vvvv: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VexForm {
    C5,
    C4W0,
    C4W1,
}

impl VexForm {
    const ALL: [Self; 3] = [Self::C5, Self::C4W0, Self::C4W1];
}

fn encoding(form: ScalarForm, vex: VexForm, memory: bool, l: bool) -> Vec<u8> {
    let encoded_vvvv = if form.reserved_vvvv || (memory && matches!(form.opcode, 0x10 | 0x11)) {
        0x78
    } else {
        0x68
    };
    let p1 = encoded_vvvv
        | (if l { 0x04 } else { 0 })
        | form.pp
        | if vex == VexForm::C4W1 { 0x80 } else { 0 };
    let modrm = if memory { 0x01 } else { 0xC1 };
    let mut bytes = match vex {
        VexForm::C5 => vec![0xC5, 0x80 | (p1 & 0x7F), form.opcode, modrm],
        VexForm::C4W0 | VexForm::C4W1 => vec![0xC4, 0xE1, p1, form.opcode, modrm],
    };
    if let Some(immediate) = form.immediate {
        bytes.push(immediate);
    }
    bytes
}

fn function(bytes: &[u8]) -> SmirFunction {
    let mut lifter = X86_64Lifter::strict();
    let mut context = LiftContext::new(SourceArch::X86_64);
    let result = lifter
        .lift_insn(PC, bytes, &mut context)
        .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
    assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");

    let mut block = SmirBlock::new(BlockId(0), PC);
    block.ops = result.ops;
    block.set_terminator(Terminator::Return { values: Vec::new() });
    let mut function = SmirFunction::new(FunctionId(0), block.id, PC);
    function.add_block(block);
    function.x86_instruction_bytes.insert(
        (BlockId(0), PC),
        X86InstructionBytes::new(bytes).expect("x86 instruction is at most 15 bytes"),
    );
    function
}

fn optimize(mut function: SmirFunction, level: OptLevel) -> SmirFunction {
    crate::smir::optimize::optimize_function(&mut function, level);
    function
}

fn lower(function: &SmirFunction) -> (Vec<u8>, usize) {
    let excluded = HashMap::new();
    assert!(is_native_clobber_safe_excluding(function, &excluded, true));
    assert!(!is_x86_aarch64_native_clobber_safe_excluding(
        function, &excluded
    ));
    let requirements = x86_native_replay_feature_requirements(function, &excluded);
    assert!(requirements.any);
    assert!(requirements.needs_avx);

    let mut lowerer = X86_64Lowerer::new();
    lowerer.set_mem_helpers(true);
    lowerer.set_preserve_vector_mem_helpers(true);
    lowerer.set_avx_ymm16_vector_state(requirements.all_spans_support_avx_ymm16);
    let result = lowerer
        .lower_function(function)
        .unwrap_or_else(|error| panic!("scalar VEX lowering failed: {error:?}"));
    assert!(result.relocations.is_empty());
    (
        lowerer
            .finalize()
            .expect("finalize canonical scalar VEX lowering"),
        result.entry_offset,
    )
}

#[test]
fn all_540_optimized_register_and_memory_lowerings_are_byte_identical_to_l0() {
    let mut lowered = 0usize;
    for form in SCALAR_FORMS {
        for vex in VexForm::ALL {
            for memory in [false, true] {
                let l0 = encoding(form, vex, memory, false);
                let l1 = encoding(form, vex, memory, true);
                for level in LEVELS {
                    let canonical = lower(&optimize(function(&l0), level));
                    let generation_dependent = lower(&optimize(function(&l1), level));
                    assert_eq!(
                        generation_dependent, canonical,
                        "{form:?} {vex:?} memory={memory} {level:?}"
                    );
                    if !memory {
                        assert!(
                            canonical.0.windows(l0.len()).any(|window| window == l0),
                            "{form:?} {vex:?} {level:?}: canonical replay absent"
                        );
                        assert!(
                            !canonical.0.windows(l1.len()).any(|window| window == l1),
                            "{form:?} {vex:?} {level:?}: L=1 host replay present"
                        );
                    }
                    lowered += 1;
                }
            }
        }
    }
    assert_eq!(lowered, 30 * 3 * 2 * LEVELS.len());
}
