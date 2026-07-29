//! Strict-lift and native-lowering coverage for exact I32-to-F64 EVEX controls.

use super::*;
use rax::smir::{FpRoundMode, VecElementType, VecWidth};

#[test]
fn ignored_embedded_rounding_closes_all_24_remaining_strict_lift_gaps() {
    let mut lifted = 0usize;
    let mut lowered = 0usize;

    for (signed, opcode) in [(true, 0xE6u8), (false, 0x7A)] {
        for ll in 0u8..=3 {
            for (mask, zeroing) in [(0u8, false), (1, false), (1, true)] {
                let bytes = [
                    0x62,
                    0xF1,
                    0x7E,
                    (u8::from(zeroing) << 7) | (ll << 5) | 0x18 | mask,
                    opcode,
                    0xC2,
                ];
                let mut lifter = X86_64Lifter::strict();
                let mut context = LiftContext::new(SourceArch::X86_64);
                let result = lifter
                    .lift_insn(0x1000, &bytes, &mut context)
                    .unwrap_or_else(|error| panic!("{bytes:02X?}: {error:?}"));
                assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
                assert!(matches!(
                    result.ops.last().unwrap().kind,
                    OpKind::X86PackedIntToFp {
                        int_elem: VecElementType::I32,
                        fp_elem: VecElementType::F64,
                        signed: actual_signed,
                        lanes: 8,
                        src_width: VecWidth::V256,
                        dst_width: VecWidth::V512,
                        mask_zeroing: actual_zeroing,
                        zero_upper: true,
                        round: FpRoundMode::Dynamic,
                        suppress_exceptions: false,
                        ..
                    } if actual_signed == signed && actual_zeroing == zeroing
                ));

                let mut block = SmirBlock::new(BlockId(0), 0x1000);
                block.ops = result.ops;
                block.set_terminator(Terminator::Return { values: vec![] });
                let mut function = SmirFunction::new(FunctionId(0), block.id, 0x1000);
                function.add_block(block);
                function.x86_instruction_bytes.insert(
                    (BlockId(0), 0x1000),
                    X86InstructionBytes::new(&bytes).unwrap(),
                );

                let canonical = [
                    bytes[0],
                    bytes[1],
                    bytes[2],
                    (u8::from(zeroing) << 7) | 0x48 | mask,
                    bytes[4],
                    bytes[5],
                ];
                for level in [OptLevel::O0, OptLevel::O1, OptLevel::O2] {
                    let mut optimized = function.clone();
                    optimize_function(&mut optimized, level);
                    #[cfg(feature = "smir-jit")]
                    assert!(
                        is_native_clobber_safe_excluding(
                            &optimized,
                            &std::collections::HashMap::new(),
                            true,
                        ),
                        "{level:?} {bytes:02X?}"
                    );

                    let mut lowerer = X86_64Lowerer::new();
                    lowerer
                        .lower_function(&optimized)
                        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
                    let code = lowerer
                        .finalize()
                        .unwrap_or_else(|error| panic!("{level:?} {bytes:02X?}: {error:?}"));
                    assert!(
                        code.windows(canonical.len())
                            .any(|window| window == canonical),
                        "{level:?} source={bytes:02X?} canonical={canonical:02X?}"
                    );
                    lowered += 1;
                }
                lifted += 1;
            }
        }
    }

    assert_eq!(lifted, 24);
    assert_eq!(lowered, 72);
}
