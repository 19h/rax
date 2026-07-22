//! Native x86 CRC32 admission tests.

use super::*;

#[test]
fn x86_crc32_gate_covers_register_and_single_use_memory_shapes() {
    for width in [OpWidth::W8, OpWidth::W16, OpWidth::W32, OpWidth::W64] {
        let op = OpKind::Crc32C {
            dst: x86(X86Reg::R8),
            crc: x86(X86Reg::R8),
            data: x86(X86Reg::R9),
            data_width: width,
        };
        assert!(op.is_jit_safe(), "CRC32 must be class-whitelisted");
        assert!(x86_gate(op), "{width:?} register CRC32 must JIT");
    }

    for (dst, data, width) in [
        (X86Reg::Rsp, X86Reg::Rbp, OpWidth::W8),
        (X86Reg::R8, X86Reg::Rbp, OpWidth::W16),
        (X86Reg::Rbp, X86Reg::Rsp, OpWidth::W64),
        (X86Reg::R31, X86Reg::R16, OpWidth::W32),
    ] {
        let dst = x86(dst);
        let op = OpKind::Crc32C {
            dst,
            crc: dst,
            data: x86(data),
            data_width: width,
        };
        assert!(x86_gate(op), "state-backed {width:?} CRC32 must JIT");
    }

    for (name, op) in [
        (
            "non-destructive destination",
            OpKind::Crc32C {
                dst: x86(X86Reg::R8),
                crc: x86(X86Reg::R9),
                data: x86(X86Reg::R10),
                data_width: OpWidth::W64,
            },
        ),
        (
            "state-backed non-destructive accumulator",
            OpKind::Crc32C {
                dst: x86(X86Reg::Rsp),
                crc: x86(X86Reg::Rbp),
                data: x86(X86Reg::R10),
                data_width: OpWidth::W32,
            },
        ),
        (
            "virtual source",
            OpKind::Crc32C {
                dst: x86(X86Reg::R8),
                crc: x86(X86Reg::R8),
                data: VReg::Virtual(VirtualId(1)),
                data_width: OpWidth::W16,
            },
        ),
        (
            "invalid width",
            OpKind::Crc32C {
                dst: x86(X86Reg::R8),
                crc: x86(X86Reg::R8),
                data: x86(X86Reg::R9),
                data_width: OpWidth::W128,
            },
        ),
    ] {
        assert!(op.is_jit_safe(), "{name} remains class-whitelisted");
        assert!(!x86_gate(op), "malformed {name} CRC32 must deopt");
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Crc32C {
            dst: x86(X86Reg::Rbp),
            crc: x86(X86Reg::Rbp),
            data: x86(X86Reg::Rsp),
            data_width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let mut hinted = builder.finish();
    hinted.blocks[0].ops[0].x86_hint = Some(X86OpHint::Mulx);
    assert!(
        !is_native_clobber_safe(&hinted),
        "hinted state-backed CRC32 must fail closed"
    );

    let memory_crc =
        |destination: X86Reg, extra_use: bool, signed: SignExtend, crc_width: OpWidth| {
            let temporary = VReg::Virtual(VirtualId(7));
            let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
            builder.push_op(
                0x1000,
                OpKind::Load {
                    dst: temporary,
                    addr: Address::BaseIndexScale {
                        base: Some(x86(X86Reg::Rsp)),
                        index: x86(X86Reg::R16),
                        scale: 2,
                        disp: 8,
                        disp_size: DispSize::Disp8,
                    },
                    width: MemWidth::B4,
                    sign: signed,
                },
            );
            builder.push_op(
                0x1000,
                OpKind::Crc32C {
                    dst: x86(destination),
                    crc: x86(destination),
                    data: temporary,
                    data_width: crc_width,
                },
            );
            if extra_use {
                builder.push_op(
                    0x1001,
                    OpKind::Mov {
                        dst: x86(X86Reg::R11),
                        src: SrcOperand::Reg(temporary),
                        width: OpWidth::W64,
                    },
                );
            }
            builder.set_terminator(Terminator::Return { values: vec![] });
            builder.finish()
        };

    for destination in [X86Reg::R10, X86Reg::Rsp, X86Reg::Rbp, X86Reg::R31] {
        let valid = memory_crc(destination, false, SignExtend::Zero, OpWidth::W32);
        assert!(
            is_native_clobber_safe_excluding(&valid, &std::collections::HashMap::new(), true),
            "state-backed memory CRC32 destination {destination:?}"
        );
    }
    let valid = memory_crc(X86Reg::R10, false, SignExtend::Zero, OpWidth::W32);
    assert!(
        !is_native_clobber_safe_excluding(&valid, &std::collections::HashMap::new(), false),
        "memory CRC32 requires MMU-helper mode"
    );
    for invalid in [
        memory_crc(X86Reg::R10, true, SignExtend::Zero, OpWidth::W32),
        memory_crc(X86Reg::R10, false, SignExtend::Sign, OpWidth::W32),
        memory_crc(X86Reg::R10, false, SignExtend::Zero, OpWidth::W64),
    ] {
        assert!(!is_native_clobber_safe_excluding(
            &invalid,
            &std::collections::HashMap::new(),
            true
        ));
    }

    let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
    builder.push_op(
        0x1000,
        OpKind::Crc32C {
            dst: x86(X86Reg::R8),
            crc: x86(X86Reg::R8),
            data: x86(X86Reg::R9),
            data_width: OpWidth::W64,
        },
    );
    builder.set_terminator(Terminator::Return { values: vec![] });
    let function = builder.finish();
    #[cfg(target_arch = "x86_64")]
    assert_eq!(
        x86_native_scalar_features_supported_excluding(
            &function,
            &std::collections::HashMap::new()
        ),
        std::is_x86_feature_detected!("sse4.2")
    );
    #[cfg(not(target_arch = "x86_64"))]
    assert!(!x86_native_scalar_features_supported_excluding(
        &function,
        &std::collections::HashMap::new()
    ));
}
