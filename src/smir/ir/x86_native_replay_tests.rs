//! Exact classifier tests for x86 native source-byte replay.

use super::*;
use crate::smir::ir::ops::{OpKind, SmirOp};
use crate::smir::ir::types::OpId;

const SCALAR_FP16_ARITHMETIC_OPCODES: [u8; 7] = [0x51, 0x58, 0x59, 0x5C, 0x5D, 0x5E, 0x5F];

#[test]
fn scalar_fp16_arithmetic_replay_classifier_is_exact_and_fail_closed() {
    for opcode in SCALAR_FP16_ARITHMETIC_OPCODES {
        // LLIG admits every L'L value. With EVEX.b clear the host observes
        // MXCSR.RC; with EVEX.b set L'L selects embedded rounding or is ignored
        // by the instruction's SAE form.
        for p2 in [0x09, 0x29, 0x49, 0x69, 0x19, 0x39, 0x59, 0x79] {
            let bytes = [0x62, 0xF5, 0x7E, p2, opcode, 0xC8];
            assert_eq!(
                X86InstructionBytes::new(&bytes)
                    .unwrap()
                    .evex_register_scalar_fp16_arithmetic_needs_vl(),
                Some(false),
                "{bytes:02X?}"
            );
        }

        // xmm17{k1}{z}, xmm18, xmm19 exercises every EVEX register-extension
        // channel while remaining a register-only scalar instruction.
        let high_registers = [0x62, 0xA5, 0x6E, 0x81, opcode, 0xCB];
        assert_eq!(
            X86InstructionBytes::new(&high_registers)
                .unwrap()
                .evex_register_scalar_fp16_arithmetic_needs_vl(),
            Some(false),
            "{high_registers:02X?}"
        );
    }

    let invalid: &[&[u8]] = &[
        &[0x61, 0xF5, 0x7E, 0x09, 0x58, 0xC8],       // not EVEX
        &[0x62, 0xF6, 0x7E, 0x09, 0x58, 0xC8],       // MAP6, not MAP5
        &[0x62, 0xF5, 0x7A, 0x09, 0x58, 0xC8],       // missing fixed-one bit
        &[0x62, 0xF5, 0x7D, 0x09, 0x58, 0xC8],       // 66, not F3
        &[0x62, 0xF5, 0xFE, 0x09, 0x58, 0xC8],       // W1, not W0
        &[0x62, 0xF5, 0x7E, 0x09, 0x58, 0x08],       // memory source
        &[0x62, 0xF5, 0x7E, 0x88, 0x58, 0xC8],       // {z} with k0
        &[0x62, 0xF5, 0x7E, 0x09, 0x50, 0xC8],       // unrelated opcode
        &[0x62, 0xF5, 0x7E, 0x09, 0x58],             // missing ModR/M
        &[0x62, 0xF5, 0x7E, 0x09, 0x58, 0xC8, 0x00], // trailing byte
    ];
    for bytes in invalid {
        assert_eq!(
            X86InstructionBytes::new(bytes)
                .unwrap()
                .evex_register_scalar_fp16_arithmetic_needs_vl(),
            None,
            "{bytes:02X?}"
        );
    }
}

#[test]
fn scalar_fp16_arithmetic_replay_spans_require_fp16_without_vl_or_dq() {
    let pc = 0x1000;
    let mut block = SmirBlock::new(BlockId(7), pc);
    block.push_op(SmirOp::new(OpId(0), pc, OpKind::Nop));

    for opcode in SCALAR_FP16_ARITHMETIC_OPCODES {
        let bytes = [0x62, 0xA5, 0x6E, 0xD1, opcode, 0xCB];
        let instruction = X86InstructionBytes::new(&bytes).unwrap();
        let provenance = HashMap::from([((BlockId(7), pc), instruction)]);

        for spans in [
            x86_evex_scalar_fp16_arithmetic_replay_spans(&block, &provenance),
            x86_evex_native_replay_spans(&block, &provenance),
        ] {
            let span = spans.get(&0).unwrap_or_else(|| panic!("{bytes:02X?}"));
            assert_eq!(span.end, 1, "{bytes:02X?}");
            assert_eq!(span.instruction, instruction, "{bytes:02X?}");
            assert!(!span.needs_avx512vl, "{bytes:02X?}");
            assert!(!span.needs_avx512dq, "{bytes:02X?}");
            assert!(span.needs_avx512fp16, "{bytes:02X?}");
        }
    }
}
