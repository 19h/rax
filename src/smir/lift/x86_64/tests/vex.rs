//! tests::vex tests

use super::*;
use crate::smir::ir::ops::X86FmaOp;
use crate::smir::lift::x86_64::*;

#[test]
fn lift_vex_andn_registers_like_llvm() {
    for (bytes, width) in [
        (&[0xC4, 0xE2, 0x70, 0xF2, 0xC2][..], OpWidth::W32),
        (&[0xC4, 0xE2, 0xF0, 0xF2, 0xC2][..], OpWidth::W64),
    ] {
        // LLVM 23 examples:
        //   `andn eax, ecx, edx` => c4 e2 70 f2 c2
        //   `andn rax, rcx, rdx` => c4 e2 f0 f2 c2
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert_eq!(result.ops.len(), 1);
        assert_vex_andn_op(&result.ops, 0, x86_gpr(0), x86_gpr(2), x86_gpr(1), width);
    }
}
#[test]
fn lift_vex_andn_memory_source_like_llvm() {
    // LLVM 23:
    //   `andn eax, r9d, dword ptr [r10 + 4*r11 + 32]`
    //       => c4 82 30 f2 44 9a 20
    let result = lift_single(&[0xC4, 0x82, 0x30, 0xF2, 0x44, 0x9A, 0x20]).unwrap();
    assert_eq!(result.bytes_consumed, 7);
    assert_eq!(result.ops.len(), 2);
    let src = match &result.ops[0].kind {
        OpKind::Load {
            dst,
            addr:
                Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 4,
                    disp: 0x20,
                    disp_size: DispSize::Disp8,
                },
            width: MemWidth::B4,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*base, x86_gpr(10));
            assert_eq!(*index, x86_gpr(11));
            *dst
        }
        other => panic!("expected VEX ANDN memory source load, got {other:?}"),
    };
    assert_vex_andn_op(&result.ops, 1, x86_gpr(0), src, x86_gpr(9), OpWidth::W32);
}
#[test]
fn lift_vex_andn_allows_destination_source_alias_like_llvm() {
    // LLVM 23: `andn ecx, ecx, edx` => c4 e2 70 f2 ca.
    let result = lift_single(&[0xC4, 0xE2, 0x70, 0xF2, 0xCA]).unwrap();
    assert_eq!(result.bytes_consumed, 5);
    assert_eq!(result.ops.len(), 1);
    assert_vex_andn_op(
        &result.ops,
        0,
        x86_gpr(1),
        x86_gpr(2),
        x86_gpr(1),
        OpWidth::W32,
    );
}
#[test]
fn lift_vex_andn_rejects_invalid_forms_like_spec() {
    for bytes in [
        &[0xC4, 0xE2, 0x74, 0xF2, 0xC2][..], // VEX.L=1
        &[0xC4, 0xE2, 0x73, 0xF2, 0xC2][..], // reserved F2 prefix
    ] {
        let result = lift_single(bytes).expect("reserved ANDN must strictly lift to #UD");
        assert_invalid_opcode_trap(&result, 4);
    }
}
#[test]
fn lift_vex_bls_register_alias_and_memory_forms() {
    let defined = x86_bls_flags();
    for (bytes, kind) in [
        (&[0xC4, 0xE2, 0xF8, 0xF3, 0xCB][..], X86BlsKind::Blsr),
        (&[0xC4, 0xE2, 0xF8, 0xF3, 0xD3][..], X86BlsKind::Blsmsk),
        (&[0xC4, 0xE2, 0xF8, 0xF3, 0xDB][..], X86BlsKind::Blsi),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert_eq!(result.ops.len(), 1);
        assert_vex_bls_op(
            &result.ops,
            0,
            x86_gpr(0),
            x86_gpr(3),
            OpWidth::W64,
            kind,
            defined,
        );
    }

    // LLVM: `blsi r15, r15` => c4 42 80 f3 df.
    let alias = lift_single(&[0xC4, 0x42, 0x80, 0xF3, 0xDF]).unwrap();
    assert_vex_bls_op(
        &alias.ops,
        0,
        x86_gpr(15),
        x86_gpr(15),
        OpWidth::W64,
        X86BlsKind::Blsi,
        defined,
    );

    // `blsr eax, dword ptr [r10 + 4*r11 + 32]`.
    let memory = lift_single(&[0xC4, 0x82, 0x78, 0xF3, 0x4C, 0x9A, 0x20]).unwrap();
    assert_eq!(memory.ops.len(), 2);
    let loaded = match &memory.ops[0].kind {
        OpKind::Load {
            dst,
            addr:
                Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 4,
                    disp: 0x20,
                    disp_size: DispSize::Disp8,
                },
            width: MemWidth::B4,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*base, x86_gpr(10));
            assert_eq!(*index, x86_gpr(11));
            *dst
        }
        other => panic!("expected VEX BLS memory load, got {other:?}"),
    };
    assert_vex_bls_op(
        &memory.ops,
        1,
        x86_gpr(0),
        loaded,
        OpWidth::W32,
        X86BlsKind::Blsr,
        defined,
    );
}
#[test]
fn lift_vex_bls_rejects_invalid_group_and_vector_length() {
    for (bytes, expected_len) in [
        (&[0xC4, 0xE2, 0x78, 0xF3, 0xC3][..], 5),
        (&[0xC4, 0xE2, 0x7C, 0xF3, 0xCB][..], 4),
    ] {
        let result = lift_single(bytes).expect("reserved BLS must strictly lift to #UD");
        assert_invalid_opcode_trap(&result, expected_len);
    }
}
#[test]
fn lift_vex_bzhi_bextr_registers_like_llvm() {
    for (bytes, name, width) in [
        (&[0xC4, 0xE2, 0x70, 0xF5, 0xC2][..], "bzhi", OpWidth::W32),
        (&[0xC4, 0xE2, 0x70, 0xF7, 0xC2][..], "bextr", OpWidth::W32),
        (&[0xC4, 0xE2, 0xF0, 0xF5, 0xC2][..], "bzhi", OpWidth::W64),
        (&[0xC4, 0xE2, 0xF0, 0xF7, 0xC2][..], "bextr", OpWidth::W64),
    ] {
        // LLVM 23 examples:
        //   `bzhi eax, edx, ecx`  => c4 e2 70 f5 c2
        //   `bextr eax, edx, ecx` => c4 e2 70 f7 c2
        //   `bzhi rax, rdx, rcx`  => c4 e2 f0 f5 c2
        //   `bextr rax, rdx, rcx` => c4 e2 f0 f7 c2
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{name}");
        assert_eq!(result.ops.len(), 1, "{name}");
        assert_vex_bzhi_bextr_op(
            &result.ops,
            0,
            name,
            x86_gpr(0),
            x86_gpr(2),
            x86_gpr(1),
            width,
        );
    }
}
#[test]
fn lift_vex_bzhi_bextr_memory_source_like_llvm() {
    for (bytes, name) in [
        (&[0xC4, 0x82, 0x30, 0xF5, 0x44, 0x9A, 0x20][..], "bzhi"),
        (&[0xC4, 0x82, 0x30, 0xF7, 0x44, 0x9A, 0x20][..], "bextr"),
    ] {
        // LLVM 23:
        //   `bzhi eax, dword ptr [r10 + 4*r11 + 32], r9d`
        //       => c4 82 30 f5 44 9a 20
        //   `bextr eax, dword ptr [r10 + 4*r11 + 32], r9d`
        //       => c4 82 30 f7 44 9a 20
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{name}");
        assert_eq!(result.ops.len(), 2, "{name}");
        let src = match &result.ops[0].kind {
            OpKind::Load {
                dst,
                addr:
                    Address::BaseIndexScale {
                        base: Some(base),
                        index,
                        scale: 4,
                        disp: 0x20,
                        disp_size: DispSize::Disp8,
                    },
                width: MemWidth::B4,
                sign: SignExtend::Zero,
            } => {
                assert_eq!(*base, x86_gpr(10), "{name}");
                assert_eq!(*index, x86_gpr(11), "{name}");
                *dst
            }
            other => panic!("expected VEX {name} memory source load, got {other:?}"),
        };
        assert_vex_bzhi_bextr_op(
            &result.ops,
            1,
            name,
            x86_gpr(0),
            src,
            x86_gpr(9),
            OpWidth::W32,
        );
    }
}
#[test]
fn lift_vex_bzhi_bextr_rejects_invalid_forms_like_spec() {
    for (bytes, name) in [
        (&[0xC4, 0xE2, 0x74, 0xF5, 0xC2][..], "bzhi VEX.L=1"),
        (&[0xC4, 0xE2, 0x74, 0xF7, 0xC2][..], "bextr VEX.L=1"),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_invalid_opcode_trap(&result, 4);
    }
}
#[test]
fn lift_vex_pdep_pext_registers_like_llvm() {
    for (bytes, name, width) in [
        (&[0xC4, 0xE2, 0x73, 0xF5, 0xC2][..], "pdep", OpWidth::W32),
        (&[0xC4, 0xE2, 0x72, 0xF5, 0xC2][..], "pext", OpWidth::W32),
        (&[0xC4, 0xE2, 0xF3, 0xF5, 0xC2][..], "pdep", OpWidth::W64),
        (&[0xC4, 0xE2, 0xF2, 0xF5, 0xC2][..], "pext", OpWidth::W64),
    ] {
        // LLVM 23 examples:
        //   `pdep eax, ecx, edx` => c4 e2 73 f5 c2
        //   `pext eax, ecx, edx` => c4 e2 72 f5 c2
        //   `pdep rax, rcx, rdx` => c4 e2 f3 f5 c2
        //   `pext rax, rcx, rdx` => c4 e2 f2 f5 c2
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{name}");
        assert_eq!(result.ops.len(), 1, "{name}");
        assert_vex_pdep_pext_op(
            &result.ops,
            0,
            name,
            x86_gpr(0),
            x86_gpr(1),
            x86_gpr(2),
            width,
        );
    }
}
#[test]
fn lift_vex_pdep_pext_memory_mask_like_llvm() {
    for (bytes, name) in [
        (&[0xC4, 0x82, 0x33, 0xF5, 0x44, 0x9A, 0x20][..], "pdep"),
        (&[0xC4, 0x82, 0x32, 0xF5, 0x44, 0x9A, 0x20][..], "pext"),
    ] {
        // LLVM 23:
        //   `pdep eax, r9d, dword ptr [r10 + 4*r11 + 32]`
        //       => c4 82 33 f5 44 9a 20
        //   `pext eax, r9d, dword ptr [r10 + 4*r11 + 32]`
        //       => c4 82 32 f5 44 9a 20
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{name}");
        assert_eq!(result.ops.len(), 2, "{name}");
        let mask = match &result.ops[0].kind {
            OpKind::Load {
                dst,
                addr:
                    Address::BaseIndexScale {
                        base: Some(base),
                        index,
                        scale: 4,
                        disp: 0x20,
                        disp_size: DispSize::Disp8,
                    },
                width: MemWidth::B4,
                sign: SignExtend::Zero,
            } => {
                assert_eq!(*base, x86_gpr(10), "{name}");
                assert_eq!(*index, x86_gpr(11), "{name}");
                *dst
            }
            other => panic!("expected VEX {name} memory mask load, got {other:?}"),
        };
        assert_vex_pdep_pext_op(
            &result.ops,
            1,
            name,
            x86_gpr(0),
            x86_gpr(9),
            mask,
            OpWidth::W32,
        );
    }
}
#[test]
fn lift_vex_pdep_pext_rejects_invalid_l_like_spec() {
    for (bytes, name) in [
        (&[0xC4, 0xE2, 0x77, 0xF5, 0xC2][..], "pdep VEX.L=1"),
        (&[0xC4, 0xE2, 0x76, 0xF5, 0xC2][..], "pext VEX.L=1"),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_invalid_opcode_trap(&result, 4);
    }

    let result = lift_single(&[0xC4, 0xE2, 0x71, 0xF5, 0xC2])
        .expect("reserved 66 PDEP/PEXT cell must strictly lift to #UD");
    assert_invalid_opcode_trap(&result, 4);
}
#[test]
fn lift_vex_mulx_registers_like_llvm() {
    for (bytes, width) in [
        (&[0xC4, 0xE2, 0x73, 0xF6, 0xC3][..], OpWidth::W32),
        (&[0xC4, 0xE2, 0xF3, 0xF6, 0xC3][..], OpWidth::W64),
    ] {
        // LLVM 23 examples:
        //   `mulx eax, ecx, ebx` => c4 e2 73 f6 c3
        //   `mulx rax, rcx, rbx` => c4 e2 f3 f6 c3
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert_eq!(result.ops.len(), 1);
        assert_vex_mulx_op(&result.ops, 0, x86_gpr(0), x86_gpr(1), x86_gpr(3), width);
    }
}
#[test]
fn lift_vex_mulx_memory_source_like_llvm() {
    // LLVM 23:
    //   `mulx eax, r9d, dword ptr [r10 + 4*r11 + 32]`
    //       => c4 82 33 f6 44 9a 20
    let result = lift_single(&[0xC4, 0x82, 0x33, 0xF6, 0x44, 0x9A, 0x20]).unwrap();
    assert_eq!(result.bytes_consumed, 7);
    assert_eq!(result.ops.len(), 2);
    let src2 = match &result.ops[0].kind {
        OpKind::Load {
            dst,
            addr:
                Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 4,
                    disp: 0x20,
                    disp_size: DispSize::Disp8,
                },
            width: MemWidth::B4,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*base, x86_gpr(10));
            assert_eq!(*index, x86_gpr(11));
            *dst
        }
        other => panic!("expected VEX MULX memory source load, got {other:?}"),
    };
    assert_vex_mulx_op(&result.ops, 1, x86_gpr(0), x86_gpr(9), src2, OpWidth::W32);
}
#[test]
fn lift_vex_mulx_alias_destination_keeps_high_half_like_spec() {
    // LLVM 23: `mulx rcx, rcx, rdx` => c4 e2 f3 f6 ca.
    let result = lift_single(&[0xC4, 0xE2, 0xF3, 0xF6, 0xCA]).unwrap();
    assert_eq!(result.bytes_consumed, 5);
    assert_eq!(result.ops.len(), 1);
    assert_vex_mulx_op(
        &result.ops,
        0,
        x86_gpr(1),
        x86_gpr(1),
        x86_gpr(2),
        OpWidth::W64,
    );
}
#[test]
fn lift_vex_mulx_rejects_invalid_forms_like_spec() {
    for bytes in [
        &[0xC4, 0xE2, 0x77, 0xF6, 0xC3][..], // VEX.L=1
        &[0xC4, 0xE2, 0x72, 0xF6, 0xC3][..], // reserved F3 prefix
    ] {
        let result = lift_single(bytes).expect("reserved MULX must strictly lift to #UD");
        assert_invalid_opcode_trap(&result, 4);
    }
}
#[test]
fn lift_vex_bmi2_shift_registers_like_llvm() {
    for (bytes, expected_op, width) in [
        (&[0xC4, 0xE2, 0x72, 0xF7, 0xC3][..], "sarx", OpWidth::W32),
        (&[0xC4, 0xE2, 0x73, 0xF7, 0xC3][..], "shrx", OpWidth::W32),
        (&[0xC4, 0xE2, 0x71, 0xF7, 0xC3][..], "shlx", OpWidth::W32),
        (&[0xC4, 0xE2, 0xF2, 0xF7, 0xC3][..], "sarx", OpWidth::W64),
        (&[0xC4, 0xE2, 0xF3, 0xF7, 0xC3][..], "shrx", OpWidth::W64),
        (&[0xC4, 0xE2, 0xF1, 0xF7, 0xC3][..], "shlx", OpWidth::W64),
    ] {
        // LLVM 23 examples:
        //   `sarx eax, ebx, ecx` => c4 e2 72 f7 c3
        //   `shrx eax, ebx, ecx` => c4 e2 73 f7 c3
        //   `shlx eax, ebx, ecx` => c4 e2 71 f7 c3
        //   `sarx rax, rbx, rcx` => c4 e2 f2 f7 c3
        //   `shrx rax, rbx, rcx` => c4 e2 f3 f7 c3
        //   `shlx rax, rbx, rcx` => c4 e2 f1 f7 c3
        assert_vex_bmi2_shift(
            bytes,
            expected_op,
            x86_gpr(0),
            x86_gpr(3),
            x86_gpr(1),
            width,
        );
    }
}
#[test]
fn lift_vex_bmi2_shift_memory_source_like_llvm() {
    for (bytes, expected_op) in [
        (&[0xC4, 0x82, 0x32, 0xF7, 0x44, 0x9A, 0x20][..], "sarx"),
        (&[0xC4, 0x82, 0x33, 0xF7, 0x44, 0x9A, 0x20][..], "shrx"),
        (&[0xC4, 0x82, 0x31, 0xF7, 0x44, 0x9A, 0x20][..], "shlx"),
    ] {
        // LLVM 23:
        //   `sarx eax, dword ptr [r10 + 4*r11 + 32], r9d`
        //       => c4 82 32 f7 44 9a 20
        //   `shrx eax, dword ptr [r10 + 4*r11 + 32], r9d`
        //       => c4 82 33 f7 44 9a 20
        //   `shlx eax, dword ptr [r10 + 4*r11 + 32], r9d`
        //       => c4 82 31 f7 44 9a 20
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{expected_op}");
        assert_eq!(result.ops.len(), 2, "{expected_op}");
        let src = match &result.ops[0].kind {
            OpKind::Load {
                dst,
                addr:
                    Address::BaseIndexScale {
                        base: Some(base),
                        index,
                        scale: 4,
                        disp: 0x20,
                        disp_size: DispSize::Disp8,
                    },
                width: MemWidth::B4,
                sign: SignExtend::Zero,
            } => {
                assert_eq!(*base, x86_gpr(10), "{expected_op}");
                assert_eq!(*index, x86_gpr(11), "{expected_op}");
                *dst
            }
            other => panic!("expected VEX BMI2 {expected_op} memory load, got {other:?}"),
        };
        assert_vex_bmi2_shift_ops(
            &result.ops,
            1,
            expected_op,
            x86_gpr(0),
            src,
            x86_gpr(9),
            OpWidth::W32,
        );
    }
}
#[test]
fn lift_vex_bmi2_shift_rejects_invalid_forms_like_spec() {
    for (bytes, name) in [
        (&[0xC4, 0xE2, 0x76, 0xF7, 0xC3][..], "sarx VEX.L=1"),
        (&[0xC4, 0xE2, 0x77, 0xF7, 0xC3][..], "shrx VEX.L=1"),
        (&[0xC4, 0xE2, 0x75, 0xF7, 0xC3][..], "shlx VEX.L=1"),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_invalid_opcode_trap(&result, 4);
    }
}
#[test]
fn lift_vex_rorx_registers_like_llvm() {
    for (bytes, width) in [
        (&[0xC4, 0xE3, 0x7B, 0xF0, 0xC3, 0x0D][..], OpWidth::W32),
        (&[0xC4, 0xE3, 0xFB, 0xF0, 0xC3, 0x0D][..], OpWidth::W64),
    ] {
        // LLVM 23 examples:
        //   `rorx eax, ebx, 13` => c4 e3 7b f0 c3 0d
        //   `rorx rax, rbx, 13` => c4 e3 fb f0 c3 0d
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert_eq!(result.ops.len(), 1);
        assert_vex_rorx_op(&result.ops, 0, x86_gpr(0), x86_gpr(3), 13, width);
    }
}
#[test]
fn lift_vex_rorx_memory_source_like_llvm() {
    // LLVM 23:
    //   `rorx eax, dword ptr [r10 + 4*r11 + 32], 13`
    //       => c4 83 7b f0 44 9a 20 0d
    let result = lift_single(&[0xC4, 0x83, 0x7B, 0xF0, 0x44, 0x9A, 0x20, 0x0D]).unwrap();
    assert_eq!(result.bytes_consumed, 8);
    assert_eq!(result.ops.len(), 2);
    let src = match &result.ops[0].kind {
        OpKind::Load {
            dst,
            addr:
                Address::BaseIndexScale {
                    base: Some(base),
                    index,
                    scale: 4,
                    disp: 0x20,
                    disp_size: DispSize::Disp8,
                },
            width: MemWidth::B4,
            sign: SignExtend::Zero,
        } => {
            assert_eq!(*base, x86_gpr(10));
            assert_eq!(*index, x86_gpr(11));
            *dst
        }
        other => panic!("expected VEX RORX memory source load, got {other:?}"),
    };
    assert_vex_rorx_op(&result.ops, 1, x86_gpr(0), src, 13, OpWidth::W32);
}
#[test]
fn lift_vex_rorx_rejects_invalid_forms_like_llvm() {
    for (bytes, name) in [
        (&[0xC4, 0xE3, 0x7F, 0xF0, 0xC3, 0x0D][..], "rorx VEX.L=1"),
        (
            &[0xC4, 0xE3, 0x73, 0xF0, 0xC3, 0x0D][..],
            "rorx reserved vvvv",
        ),
    ] {
        let result = lift_single(bytes).unwrap_or_else(|error| panic!("{name}: {error:?}"));
        assert_invalid_opcode_trap(&result, 4);
    }

    let result = lift_single(&[0xC4, 0xE3, 0x78, 0xF0, 0xC3, 0x0D])
        .expect("reserved RORX mandatory prefix must strictly lift to #UD");
    assert_invalid_opcode_trap(&result, 4);

    let err = lift_single(&[0xC4, 0xE3, 0x7B, 0xF0, 0xC3]).unwrap_err();
    assert!(matches!(err, LiftError::Incomplete { .. }), "{err:?}");
}
#[test]
fn lift_legacy_and_vex_mxcsr_memory_operations() {
    for (bytes, load, vex) in [
        (&[0x0F, 0xAE, 0x10][..], true, false),
        (&[0x0F, 0xAE, 0x58, 0x04][..], false, false),
        (&[0x66, 0x0F, 0xAE, 0x10][..], true, false),
        (&[0xF3, 0x0F, 0xAE, 0x58, 0x04][..], false, false),
        (&[0xC5, 0xF8, 0xAE, 0x10][..], true, true),
        (&[0xC5, 0xF8, 0xAE, 0x58, 0x04][..], false, true),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(result.ops.iter().any(|op| {
            (load && matches!(op.kind, OpKind::X86LoadMxcsr { .. }))
                || (!load && matches!(op.kind, OpKind::X86StoreMxcsr { .. }))
        }));
        if vex {
            assert!(matches!(
                result.ops.last().unwrap().x86_hint,
                Some(X86OpHint::VexOp {
                    map: X86VecMap::Map0F,
                    pp: X86SsePrefix::None,
                    opcode: 0xAE,
                    width: VecWidth::V128,
                    ..
                })
            ));
        }
    }

    let reserved_register = lift_single(&[0x0F, 0xAE, 0xD0])
        .expect("reserved legacy register /2 must strictly lift to #UD");
    assert_invalid_opcode_trap(&reserved_register, 3);

    for bytes in [
        &[0xC5, 0xFC, 0xAE, 0x10][..], // VEX.L=1
        &[0xC5, 0xE8, 0xAE, 0x10][..], // reserved VEX.vvvv
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "accepted reserved VEX MXCSR encoding {bytes:02X?}"
        );
    }

    let addr32 = lift_single(&[0x67, 0x0F, 0xAE, 0x14, 0x77]).unwrap();
    assert_eq!(addr32.bytes_consumed, 5);
    assert!(addr32.ops[..addr32.ops.len() - 1].iter().all(|op| matches!(
        op.kind,
        OpKind::Shl {
            width: OpWidth::W32,
            flags: FlagUpdate::None,
            ..
        } | OpKind::Add {
            width: OpWidth::W32,
            flags: FlagUpdate::None,
            ..
        }
    )));
    assert!(matches!(
        addr32.ops.last().map(|op| &op.kind),
        Some(OpKind::X86LoadMxcsr { .. })
    ));
}
#[test]
fn lift_packed_sse_moves_arithmetic_and_vex_divide() {
    let movups = lift_single(&[0x0F, 0x10, 0xC1]).unwrap();
    assert!(matches!(
        movups.ops.as_slice(),
        [SmirOp {
            kind: OpKind::VMov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                width: VecWidth::V128,
            },
            x86_hint: Some(X86OpHint::SseMov { opcode: 0x10, .. }),
            ..
        }]
    ));

    for (bytes, expected_elem, expected) in [
        (
            &[0x0F, 0x58, 0xC1][..],
            VecElementType::F32,
            X86FpBinaryOp::Add,
        ),
        (
            &[0x66, 0x0F, 0x59, 0xC1][..],
            VecElementType::F64,
            X86FpBinaryOp::Mul,
        ),
        (
            &[0x0F, 0x5C, 0xC1][..],
            VecElementType::F32,
            X86FpBinaryOp::Sub,
        ),
        (
            &[0x66, 0x0F, 0x5E, 0xC1][..],
            VecElementType::F64,
            X86FpBinaryOp::Div,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        let matches_kind = result.ops.iter().any(|op| {
            matches!(
                op.kind,
                OpKind::X86FpBinary {
                    elem,
                    op: actual,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    ..
                } if elem == expected_elem && actual == expected
            )
        });
        assert!(matches_kind, "{bytes:02X?}");
    }

    let vex_divps = lift_single(&[0xC5, 0xF8, 0x5E, 0xC1]).unwrap();
    assert!(matches!(
        vex_divps.ops.last().unwrap().kind,
        OpKind::X86FpBinary {
            elem: VecElementType::F32,
            lanes: 4,
            op: X86FpBinaryOp::Div,
            ..
        }
    ));
    let vex_divpd = lift_single(&[0xC5, 0xF9, 0x5E, 0xC1]).unwrap();
    assert!(matches!(
        vex_divpd.ops.last().unwrap().kind,
        OpKind::X86FpBinary {
            elem: VecElementType::F64,
            lanes: 2,
            op: X86FpBinaryOp::Div,
            ..
        }
    ));

    let xorps = lift_single(&[0x0F, 0x57, 0xC1]).unwrap();
    assert!(xorps.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VXor {
            width: VecWidth::V128,
            ..
        }
    )));
    let andnps = lift_single(&[0x0F, 0x55, 0xC1]).unwrap();
    assert!(matches!(
        andnps.ops.last().unwrap().kind,
        OpKind::VAndNot {
            width: VecWidth::V128,
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            ..
        }
    ));
    let vex_andps = lift_single(&[0xC5, 0xF8, 0x54, 0xC1]).unwrap();
    assert!(matches!(
        vex_andps.ops.last().unwrap().kind,
        OpKind::VAnd {
            width: VecWidth::V128,
            ..
        }
    ));
    let vex_andnps = lift_single(&[0xC5, 0xF8, 0x55, 0xC1]).unwrap();
    assert!(matches!(
        vex_andnps.ops.last().unwrap().kind,
        OpKind::VAndNot {
            width: VecWidth::V128,
            ..
        }
    ));
    let evex_andnps = lift_single(&[0x62, 0xF1, 0x7C, 0x48, 0x55, 0xC1]).unwrap();
    assert!(matches!(
        evex_andnps.ops.last().unwrap().kind,
        OpKind::VAndNot {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            width: VecWidth::V512,
        }
    ));

    let movss = lift_single(&[0xF3, 0x0F, 0x10, 0xC1]).unwrap();
    assert!(matches!(
        movss.ops.as_slice(),
        [
            SmirOp {
                kind: OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    elem: VecElementType::F32,
                    ..
                },
                ..
            },
            SmirOp {
                kind: OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: VecElementType::F32,
                    ..
                },
                ..
            }
        ]
    ));

    let addsd = lift_single(&[0xF2, 0x0F, 0x58, 0xC1]).unwrap();
    assert!(addsd.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86FpBinary {
            elem: VecElementType::F64,
            lanes: 1,
            op: X86FpBinaryOp::Add,
            ..
        }
    )));
    assert!(matches!(
        addsd.ops.last().unwrap().kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            elem: VecElementType::F64,
            ..
        }
    ));

    let movss_mem = lift_single(&[0xF3, 0x0F, 0x10, 0x00]).unwrap();
    assert!(matches!(
        movss_mem.ops.first().unwrap().kind,
        OpKind::Load {
            width: MemWidth::B4,
            ..
        }
    ));
    assert_eq!(
        movss_mem
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::VInsertLane {
                    elem: VecElementType::F32,
                    ..
                }
            ))
            .count(),
        4,
    );
    let movsd_store = lift_single(&[0xF2, 0x0F, 0x11, 0x00]).unwrap();
    assert!(matches!(
        movsd_store.ops.last().unwrap().kind,
        OpKind::Store {
            width: MemWidth::B8,
            ..
        }
    ));
    assert!(matches!(
        lift_single(&[0xF0, 0xF3, 0x0F, 0x58, 0xC1]),
        Err(LiftError::InvalidEncoding { .. })
    ));
}
#[test]
fn lift_legacy_and_vex_movmsk_extracts_exact_lanes_and_rejects_reserved_forms() {
    for (bytes, dst, src, elem, lanes, width) in [
        (
            &[0x0F, 0x50, 0xC1][..],
            X86Reg::Rax,
            X86Reg::Xmm(1),
            VecElementType::F32,
            4u8,
            OpWidth::W32,
        ),
        (
            &[0x66, 0x0F, 0x50, 0xD1][..],
            X86Reg::Rdx,
            X86Reg::Xmm(1),
            VecElementType::F64,
            2,
            OpWidth::W32,
        ),
        (
            &[0x66, 0x48, 0x0F, 0x50, 0xC1][..],
            X86Reg::Rax,
            X86Reg::Xmm(1),
            VecElementType::F64,
            2,
            OpWidth::W64,
        ),
        (
            &[0x45, 0x0F, 0x50, 0xC1][..],
            X86Reg::R8,
            X86Reg::Xmm(9),
            VecElementType::F32,
            4,
            OpWidth::W32,
        ),
        (
            &[0xC5, 0xF8, 0x50, 0xC1][..],
            X86Reg::Rax,
            X86Reg::Xmm(1),
            VecElementType::F32,
            4,
            OpWidth::W32,
        ),
        (
            &[0xC5, 0xFD, 0x50, 0xD1][..],
            X86Reg::Rdx,
            X86Reg::Ymm(1),
            VecElementType::F64,
            4,
            OpWidth::W32,
        ),
        (
            &[0xC4, 0x41, 0x7C, 0x50, 0xC1][..],
            X86Reg::R8,
            X86Reg::Ymm(9),
            VecElementType::F32,
            8,
            OpWidth::W32,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86MovMask {
                dst: VReg::Arch(ArchReg::X86(actual_dst)),
                    src: VReg::Arch(ArchReg::X86(actual_src)),
                    elem: actual_elem,
                    lanes: actual_lanes,
                    dst_width: actual_width,
                },
                x86_hint: Some(_),
                ..
            }] if *actual_dst == dst
                && *actual_src == src
                && *actual_elem == elem
                && *actual_lanes == lanes
                && *actual_width == width
        ));
        assert!(
            result
                .ops
                .iter()
                .all(|op| op.kind.flags_written().is_empty())
        );
    }

    for bytes in [
        &[0x0F, 0x50, 0x01][..],                   // memory source is undefined
        &[0xF3, 0x0F, 0x50, 0xC1][..],             // no F3 legacy form
        &[0xF0, 0x0F, 0x50, 0xC1][..],             // LOCK is undefined
        &[0xC5, 0xF0, 0x50, 0xC1][..],             // VEX.vvvv != 1111b
        &[0xC5, 0xFA, 0x50, 0xC1][..],             // no F3 VEX form
        &[0xC5, 0xF8, 0x50, 0x01][..],             // VEX memory source
        &[0x62, 0xF1, 0x7C, 0x08, 0x50, 0xC1][..], // no EVEX form
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "invalid MOVMSK accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_legacy_and_vex_pmovmskb_extracts_all_bytes_and_rejects_reserved_forms() {
    for (bytes, dst, width) in [
        (&[0x0F, 0xD7, 0xC1][..], X86Reg::Rax, OpWidth::W32),
        (&[0x4C, 0x0F, 0xD7, 0xC1][..], X86Reg::R8, OpWidth::W64),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(
            result.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::X86X87Control {
                        kind: X86X87ControlKind::EnterMmx,
                        addr: None,
                    },
                    ..
                },
                SmirOp {
                    kind: OpKind::X86MovMask {
                        dst: VReg::Arch(ArchReg::X86(actual_dst)),
                        src: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                        elem: VecElementType::I8,
                        lanes: 8,
                        dst_width: actual_width,
                    },
                    x86_hint: Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: 0xD7,
                    }),
                    ..
                }
            ] if *actual_dst == dst && *actual_width == width
        ));
        assert!(
            result
                .ops
                .iter()
                .all(|op| op.kind.flags_written().is_empty())
        );
    }

    for (bytes, dst, src, lanes) in [
        (
            &[0x66, 0x0F, 0xD7, 0xC1][..],
            X86Reg::Rax,
            X86Reg::Xmm(1),
            16u8,
        ),
        (
            &[0x66, 0x45, 0x0F, 0xD7, 0xC1][..],
            X86Reg::R8,
            X86Reg::Xmm(9),
            16,
        ),
        (
            &[0xC5, 0xF9, 0xD7, 0xC1][..],
            X86Reg::Rax,
            X86Reg::Xmm(1),
            16,
        ),
        (
            &[0xC5, 0xFD, 0xD7, 0xC2][..],
            X86Reg::Rax,
            X86Reg::Ymm(2),
            32,
        ),
        (
            &[0xC4, 0x41, 0xFD, 0xD7, 0xCA][..],
            X86Reg::R9,
            X86Reg::Ymm(10),
            32,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(
            result.ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86MovMask {
                    dst: VReg::Arch(ArchReg::X86(actual_dst)),
                    src: VReg::Arch(ArchReg::X86(actual_src)),
                    elem: VecElementType::I8,
                    lanes: actual_lanes,
                    dst_width: OpWidth::W32,
                },
                x86_hint: Some(_),
                ..
            }] if *actual_dst == dst && *actual_src == src && *actual_lanes == lanes
        ));
        assert!(
            result
                .ops
                .iter()
                .all(|op| op.kind.flags_written().is_empty())
        );
    }

    for bytes in [
        &[0x0F, 0xD7, 0x01][..],
        &[0xF3, 0x0F, 0xD7, 0xC1][..],
        &[0x66, 0x0F, 0xD7, 0x01][..],
        &[0xF3, 0x66, 0x0F, 0xD7, 0xC1][..],
        &[0xC5, 0xED, 0xD7, 0xC1][..],
        &[0xC5, 0xFC, 0xD7, 0xC1][..],
        &[0xC5, 0xFD, 0xD7, 0x01][..],
        &[0x62, 0xF1, 0x7D, 0x08, 0xD7, 0xC1][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "invalid PMOVMSKB accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_vex_masked_memory_covers_float_integer_load_store_and_fault_lanes() {
    for (bytes, elem, lanes, load) in [
        (
            &[0xC4, 0xE2, 0x71, 0x2C, 0x17][..],
            VecElementType::F32,
            4usize,
            true,
        ),
        (
            &[0xC4, 0xE2, 0x75, 0x2F, 0x17][..],
            VecElementType::F64,
            4,
            false,
        ),
        (
            &[0xC4, 0xE2, 0x71, 0x8C, 0x17][..],
            VecElementType::I32,
            4,
            true,
        ),
        (
            &[0xC4, 0xE2, 0xF1, 0x8E, 0x17][..],
            VecElementType::I64,
            2,
            false,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert_eq!(
            result
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::VExtractLane {
                        vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1) | X86Reg::Ymm(1))),
                        elem: actual,
                        ..
                    } if actual == elem
                ))
                .count(),
            lanes,
        );
        assert_eq!(
            result
                .ops
                .iter()
                .filter(|op| if load {
                    matches!(op.kind, OpKind::PredLoad { .. })
                } else {
                    matches!(op.kind, OpKind::PredStore { .. })
                })
                .count(),
            lanes,
        );
        assert!(
            result
                .ops
                .iter()
                .all(|op| op.kind.flags_written().is_empty())
        );
    }

    let addr32 = lift_single(&[0x67, 0xC4, 0xE2, 0x71, 0x2C, 0x14, 0x77]).unwrap();
    assert_eq!(addr32.bytes_consumed, 7);
    let addr = addr32
        .ops
        .iter()
        .find_map(|op| match &op.kind {
            OpKind::Lea { addr, .. } => Some(addr),
            _ => None,
        })
        .expect("masked addr32 memory base");
    super::addr32_assertions::sib(addr, Some(X86Reg::Rdi), X86Reg::Rsi, 2, 0);

    for bytes in [
        &[0xC4, 0xE2, 0x71, 0x2C, 0xD2][..], // memory operand required
        &[0xC4, 0xE2, 0xF1, 0x2C, 0x17][..], // VMASKMOVPS W=1
        &[0xC4, 0xE2, 0x70, 0x2C, 0x17][..], // mandatory 66 absent
        &[0x62, 0xF2, 0x75, 0x08, 0x8C, 0x17][..], // no EVEX form
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
#[test]
fn lift_vex_fma3_covers_orders_signs_scalars_alternation_and_addresses() {
    for order in [0x90u8, 0xA0, 0xB0] {
        for low in [0x06u8, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F] {
            let scalar = matches!(low, 0x09 | 0x0B | 0x0D | 0x0F);
            let vex = if scalar { 0x71 } else { 0x75 };
            let bytes = [0xC4, 0xE2, vex, order | low, 0xD3];
            let result = lift_single(&bytes).unwrap();
            assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
            assert!(
                result
                    .ops
                    .iter()
                    .any(|op| matches!(op.kind, OpKind::X86Fma(_)))
            );
            assert!(
                result
                    .ops
                    .iter()
                    .all(|op| op.kind.flags_written().is_empty())
            );
        }
    }

    for (opcode, expected_order) in [
        (0x98, X86FmaOrder::Order132),
        (0xA8, X86FmaOrder::Order213),
        (0xB8, X86FmaOrder::Order231),
    ] {
        let result = lift_single(&[0xC4, 0xE2, 0x75, opcode, 0xD3]).unwrap();
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86Fma(X86FmaOp {
                src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                src3: VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
                elem: VecElementType::F32,
                lanes: 8,
                kind: X86FmaKind::Add,
                order,
                round: FpRoundMode::Dynamic,
                ..
            }) if order == expected_order
        )));
    }

    for (opcode, expected_kind) in [
        (0x9A, X86FmaKind::Sub),
        (0x9C, X86FmaKind::NegativeMultiplyAdd),
        (0x9E, X86FmaKind::NegativeMultiplySub),
    ] {
        let result = lift_single(&[0xC4, 0xE2, 0xF5, opcode, 0xD3]).unwrap();
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86Fma(X86FmaOp {
                elem: VecElementType::F64,
                kind,
                ..
            }) if kind == expected_kind
        )));
    }

    let scalar = lift_single(&[0xC4, 0xE2, 0xF1, 0xB9, 0xD3]).unwrap();
    assert!(scalar.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86Fma(X86FmaOp {
            elem: VecElementType::F64,
            lanes: 1,
            ..
        })
    )));
    assert!(scalar.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            lane: 1,
            elem: VecElementType::F64,
            ..
        }
    )));

    let alternating = lift_single(&[0xC4, 0xE2, 0x75, 0x96, 0xD3]).unwrap();
    assert_eq!(
        alternating
            .ops
            .iter()
            .filter(|op| {
                matches!(
                    op.kind,
                    OpKind::X86Fma(X86FmaOp {
                        kind: X86FmaKind::AddSub,
                        ..
                    })
                )
            })
            .count(),
        1
    );

    let addr32 = lift_single(&[0x67, 0xC4, 0xE2, 0x75, 0x98, 0x54, 0x77, 0x20]).unwrap();
    assert_eq!(addr32.bytes_consumed, 8);
    let addr = addr32
        .ops
        .iter()
        .find_map(|op| match &op.kind {
            OpKind::VLoad {
                addr,
                width: VecWidth::V256,
                ..
            } => Some(addr),
            _ => None,
        })
        .expect("FMA addr32 memory source");
    super::addr32_assertions::sib(addr, Some(X86Reg::Rdi), X86Reg::Rsi, 2, 0x20);

    assert!(matches!(
        lift_single(&[0xC4, 0xE2, 0x74, 0x98, 0xD3]),
        Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
    ));
    assert!(lift_single(&[0xC4, 0xE2, 0x75, 0x99, 0xD3]).is_ok()); // VEX.LIG

    let high_masked = lift_single(&[0x62, 0xA2, 0x75, 0x43, 0x98, 0xC2]).unwrap();
    assert!(high_masked.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86Fma(X86FmaOp {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
            src3: VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
            mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(3)))),
            lanes: 16,
            ..
        })
    )));
    assert_eq!(
        high_masked
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::Select { .. }))
            .count(),
        16
    );

    let masked_memory = lift_single(&[0x62, 0xE2, 0xF5, 0xA3, 0xBE, 0x40, 0x02]).unwrap();
    assert_eq!(
        masked_memory
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B8,
                    ..
                }
            ))
            .count(),
        4
    );
    let broadcast = lift_single(&[0x62, 0xF2, 0x65, 0xD9, 0xA6, 0x10]).unwrap();
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16
    );
    let scalar_masked = lift_single(&[0x62, 0xE2, 0x55, 0x82, 0xB9, 0x20]).unwrap();
    assert!(scalar_masked.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::PredLoad {
            width: MemWidth::B4,
            ..
        }
    )));
    assert!(scalar_masked.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(20))),
            lane: 3,
            elem: VecElementType::F32,
            ..
        }
    )));
    assert!(lift_single(&[0x62, 0xA2, 0xD5, 0x12, 0xB9, 0xE6]).is_ok());

    for bytes in [
        &[0x62, 0xF2, 0x75, 0x88, 0x98, 0xC2][..],
        &[0x62, 0xF2, 0x75, 0x60, 0x98, 0xC2][..],
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
#[test]
fn lift_reciprocal_estimates_covers_legacy_vex_scalar_packed_and_special_encodings() {
    for (bytes, op, lanes, dst) in [
        (
            &[0x0F, 0x53, 0xC1][..],
            VecUnaryOp::FRecipEstimate,
            4u8,
            X86Reg::Xmm(0),
        ),
        (
            &[0xF3, 0x45, 0x0F, 0x52, 0xC1][..],
            VecUnaryOp::FRsqrtEstimate,
            1,
            X86Reg::Xmm(8),
        ),
        (
            &[0xC5, 0xF8, 0x53, 0xD1][..],
            VecUnaryOp::FRecipEstimate,
            4,
            X86Reg::Xmm(2),
        ),
        (
            &[0xC5, 0xFC, 0x52, 0xE1][..],
            VecUnaryOp::FRsqrtEstimate,
            8,
            X86Reg::Ymm(4),
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(result.ops.iter().any(|operation| matches!(
            operation.kind,
            OpKind::VUnary {
                op: actual,
                lanes: actual_lanes,
                ..
            } if actual == op && actual_lanes == lanes
        )));
        assert!(result.ops.iter().any(|operation| {
            operation
                .kind
                .dests()
                .contains(&VReg::Arch(ArchReg::X86(dst)))
        }));
    }

    let legacy_packed_mem = lift_single(&[0x0F, 0x53, 0x00]).unwrap();
    assert!(
        legacy_packed_mem
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
    );
    let legacy_scalar_mem = lift_single(&[0xF3, 0x0F, 0x53, 0x00]).unwrap();
    assert!(
        !legacy_scalar_mem
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );
    let addr32 = lift_single(&[0x67, 0xC5, 0xF8, 0x52, 0x54, 0x77, 0x10]).unwrap();
    assert_eq!(addr32.bytes_consumed, 7);
    let addr = addr32
        .ops
        .iter()
        .find_map(|op| match &op.kind {
            OpKind::VLoad { addr, .. } => Some(addr),
            _ => None,
        })
        .expect("reciprocal addr32 memory source");
    super::addr32_assertions::sib(addr, Some(X86Reg::Rdi), X86Reg::Rsi, 2, 0x10);
    assert!(lift_single(&[0xC5, 0xFE, 0x53, 0xD1]).is_ok()); // scalar VEX.LIG

    for bytes in [
        &[0x66, 0x0F, 0x53, 0xC1][..],
        &[0xF0, 0x0F, 0x52, 0xC1][..],
        &[0xC5, 0xE8, 0x53, 0xC1][..],
        &[0xC5, 0xF9, 0x52, 0xC1][..],
        &[0x62, 0xF1, 0x7C, 0x08, 0x53, 0xC1][..],
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
#[test]
fn lift_legacy_and_vex_lddqu_models_width_extensions_and_reserved_forms() {
    for (bytes, dst, width, expected_offset, legacy) in [
        (
            &[0xF2, 0x0F, 0xF0, 0x40, 0x01][..],
            X86Reg::Xmm(0),
            VecWidth::V128,
            1i64,
            true,
        ),
        (
            &[0xF2, 0x44, 0x0F, 0xF0, 0x40, 0x03][..],
            X86Reg::Xmm(8),
            VecWidth::V128,
            3,
            true,
        ),
        (
            &[0xC5, 0xFB, 0xF0, 0x40, 0x05][..],
            X86Reg::Xmm(0),
            VecWidth::V128,
            5,
            false,
        ),
        (
            &[0xC5, 0xFF, 0xF0, 0x40, 0x07][..],
            X86Reg::Ymm(0),
            VecWidth::V256,
            7,
            false,
        ),
        (
            &[0xC4, 0x61, 0xFB, 0xF0, 0x40, 0x09][..],
            X86Reg::Xmm(8),
            VecWidth::V128,
            9,
            false,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len(), "{bytes:02X?}");
        assert!(matches!(
            result.ops.last(),
            Some(SmirOp {
                kind: OpKind::VLoad {
                    dst: VReg::Arch(ArchReg::X86(actual_dst)),
                    addr: Address::BaseOffset {
                        base: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                        offset,
                        ..
                    },
                    width: actual_width,
                },
                x86_hint,
                ..
            }) if *actual_dst == dst && *actual_width == width && *offset == expected_offset
                && if legacy {
                    matches!(x86_hint, Some(X86OpHint::SseMov {
                        prefix: X86SsePrefix::Repne,
                        opcode: 0xF0,
                    }))
                } else {
                    matches!(x86_hint, Some(X86OpHint::VexOp {
                        map: X86VecMap::Map0F,
                        pp: X86SsePrefix::Repne,
                        opcode: 0xF0,
                        ..
                    }))
                }
        ));
    }

    for bytes in [
        &[0x0F, 0xF0, 0x00][..],                   // F2 is mandatory
        &[0xF3, 0x0F, 0xF0, 0x00][..],             // F3 is not LDDQU
        &[0x66, 0xF2, 0x0F, 0xF0, 0x00][..],       // conflicting 66 prefix
        &[0xF0, 0xF2, 0x0F, 0xF0, 0x00][..],       // LOCK is undefined
        &[0xF2, 0x0F, 0xF0, 0xC1][..],             // register source
        &[0xC5, 0xF3, 0xF0, 0x00][..],             // VEX.vvvv != 1111b
        &[0xC5, 0xFA, 0xF0, 0x00][..],             // VEX.F3 is not VLDDQU
        &[0xC5, 0xFB, 0xF0, 0xC1][..],             // VEX register source
        &[0x62, 0xF1, 0x7F, 0x08, 0xF0, 0x00][..], // no EVEX form
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "invalid LDDQU accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_legacy_and_vex_packed_integer_logic_covers_all_operations_and_forms() {
    for (opcode, expected) in [(0xDB, "and"), (0xDF, "andn"), (0xEB, "or"), (0xEF, "xor")] {
        let mmx = lift_single(&[0x0F, opcode, 0xC1]).unwrap();
        assert_eq!(mmx.bytes_consumed, 3);
        assert!(matches!(
            mmx.ops.first().unwrap().kind,
            OpKind::X86X87Control {
                kind: X86X87ControlKind::EnterMmx,
                addr: None,
            }
        ));
        assert!(mmx.ops.iter().any(|op| match (&op.kind, expected) {
            (
                OpKind::VAnd {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                    width: VecWidth::V64,
                },
                "and",
            )
            | (
                OpKind::VAndNot {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                    width: VecWidth::V64,
                },
                "andn",
            )
            | (
                OpKind::VOr {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                    width: VecWidth::V64,
                },
                "or",
            )
            | (
                OpKind::VXor {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                    width: VecWidth::V64,
                },
                "xor",
            ) => true,
            _ => false,
        }));
        assert!(matches!(
            mmx.ops.last().unwrap().x86_hint,
            Some(X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: actual,
            }) if actual == opcode
        ));

        let legacy = lift_single(&[0x66, 0x0F, opcode, 0xC1]).unwrap();
        assert_eq!(legacy.bytes_consumed, 4);
        assert!(
            matches!(
                legacy.ops.as_slice(),
                [SmirOp {
                    kind: OpKind::VAnd {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                        width: VecWidth::V128,
                    },
                    ..
                }]
            ) == (expected == "and")
        );
        assert!(legacy.ops.iter().any(|op| match (&op.kind, expected) {
            (OpKind::VAnd { .. }, "and")
            | (OpKind::VAndNot { .. }, "andn")
            | (OpKind::VOr { .. }, "or")
            | (OpKind::VXor { .. }, "xor") => true,
            _ => false,
        }));
        assert!(matches!(
            legacy.ops.last().unwrap().x86_hint,
            Some(X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: actual,
            }) if actual == opcode
        ));

        let vex = lift_single(&[0xC5, 0xF5, opcode, 0xC2]).unwrap();
        assert_eq!(vex.bytes_consumed, 4);
        assert!(vex.ops.iter().any(|op| match (&op.kind, expected) {
            (
                OpKind::VAnd {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                    width: VecWidth::V256,
                },
                "and",
            )
            | (
                OpKind::VAndNot {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                    width: VecWidth::V256,
                },
                "andn",
            )
            | (
                OpKind::VOr {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                    width: VecWidth::V256,
                },
                "or",
            )
            | (
                OpKind::VXor {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                    width: VecWidth::V256,
                },
                "xor",
            ) => true,
            _ => false,
        }));
    }

    let memory = lift_single(&[0x66, 0x0F, 0xDF, 0x40, 0x10]).unwrap();
    assert!(matches!(
        memory.ops.first().unwrap().kind,
        OpKind::VLoad {
            addr: Address::BaseOffset {
                base: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                offset: 0x10,
                ..
            },
            width: VecWidth::V128,
            ..
        }
    ));
    assert!(matches!(
        memory.ops.last().unwrap().kind,
        OpKind::VAndNot {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            ..
        }
    ));

    let mmx_memory = lift_single(&[0x0F, 0xDF, 0x40, 0x10]).unwrap();
    assert!(matches!(
        mmx_memory.ops.as_slice(),
        [
            SmirOp {
                kind: OpKind::VLoad {
                    width: VecWidth::V64,
                    ..
                },
                ..
            },
            SmirOp {
                kind: OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    ..
                },
                ..
            },
            SmirOp {
                kind: OpKind::VAndNot {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    width: VecWidth::V64,
                    ..
                },
                ..
            }
        ]
    ));

    // REX.R/REX.B do not extend the three-bit MMX register namespace.
    let mmx_rex = lift_single(&[0x45, 0x0F, 0xEB, 0xC1]).unwrap();
    assert!(matches!(
        mmx_rex.ops.last().unwrap().kind,
        OpKind::VOr {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
            width: VecWidth::V64,
        }
    ));

    let high = lift_single(&[0xC4, 0x41, 0x35, 0xDB, 0xC1]).unwrap();
    assert!(matches!(
        high.ops.as_slice(),
        [SmirOp {
            kind: OpKind::VAnd {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(8))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                width: VecWidth::V256,
            },
            ..
        }]
    ));

    for bytes in [
        &[0xF2, 0x0F, 0xDB, 0xC1][..],       // invalid mandatory prefix
        &[0xF0, 0x66, 0x0F, 0xDB, 0xC1][..], // LOCK is undefined
        &[0xC5, 0xF4, 0xDB, 0xC2][..],       // VEX form requires 66
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "invalid integer logic accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_legacy_and_vex_packed_fp_precision_conversions() {
    for (bytes, dst, src, from, to, lanes, dst_width, zero_upper) in [
        (
            &[0x0F, 0x5A, 0xC1][..],
            X86Reg::Xmm(0),
            X86Reg::Xmm(1),
            VecElementType::F32,
            VecElementType::F64,
            2,
            VecWidth::V128,
            false,
        ),
        (
            &[0x66, 0x0F, 0x5A, 0xC1][..],
            X86Reg::Xmm(0),
            X86Reg::Xmm(1),
            VecElementType::F64,
            VecElementType::F32,
            2,
            VecWidth::V128,
            false,
        ),
        (
            &[0xC5, 0xF8, 0x5A, 0xC1][..],
            X86Reg::Xmm(0),
            X86Reg::Xmm(1),
            VecElementType::F32,
            VecElementType::F64,
            2,
            VecWidth::V128,
            true,
        ),
        (
            &[0xC5, 0xFC, 0x5A, 0xC1][..],
            X86Reg::Ymm(0),
            X86Reg::Xmm(1),
            VecElementType::F32,
            VecElementType::F64,
            4,
            VecWidth::V256,
            true,
        ),
        (
            &[0xC5, 0xF9, 0x5A, 0xC1][..],
            X86Reg::Xmm(0),
            X86Reg::Xmm(1),
            VecElementType::F64,
            VecElementType::F32,
            2,
            VecWidth::V128,
            true,
        ),
        (
            &[0xC5, 0xFD, 0x5A, 0xC1][..],
            X86Reg::Xmm(0),
            X86Reg::Ymm(1),
            VecElementType::F64,
            VecElementType::F32,
            4,
            VecWidth::V128,
            true,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(matches!(
            result.ops.last().unwrap().kind,
            OpKind::X86PackedFpConvert {
                dst: VReg::Arch(ArchReg::X86(actual_dst)),
                src: VReg::Arch(ArchReg::X86(actual_src)),
                from: actual_from,
                to: actual_to,
                lanes: actual_lanes,
                dst_width: actual_width,
                zero_upper: actual_zero,
                ..
            } if actual_dst == dst && actual_src == src && actual_from == from
                && actual_to == to && actual_lanes == lanes && actual_width == dst_width
                && actual_zero == zero_upper
        ));
    }

    for (bytes, width) in [
        (&[0x0F, 0x5A, 0x00][..], VecWidth::V64),
        (&[0x66, 0x0F, 0x5A, 0x00][..], VecWidth::V128),
        (&[0xC5, 0xFC, 0x5A, 0x00][..], VecWidth::V128),
        (&[0xC5, 0xFD, 0x5A, 0x00][..], VecWidth::V256),
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                width: actual_width,
                ..
            } if actual_width == width
        )));
    }

    for bytes in [
        &[0xF0, 0x0F, 0x5A, 0xC1][..], // LOCK
        &[0xC5, 0xE8, 0x5A, 0xC1][..], // reserved VEX.vvvv
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
#[test]
fn lift_legacy_and_vex_packed_integer_compares_cover_all_fixed_predicates() {
    for (opcode, elem, cond) in [
        (0x64, VecElementType::I8, VecCmpCond::Gt),
        (0x65, VecElementType::I16, VecCmpCond::Gt),
        (0x66, VecElementType::I32, VecCmpCond::Gt),
        (0x74, VecElementType::I8, VecCmpCond::Eq),
        (0x75, VecElementType::I16, VecCmpCond::Eq),
        (0x76, VecElementType::I32, VecCmpCond::Eq),
    ] {
        let mmx = lift_single(&[0x0F, opcode, 0xC1]).unwrap();
        assert!(matches!(
            mmx.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::X86X87Control {
                        kind: X86X87ControlKind::EnterMmx,
                        ..
                    },
                    ..
                },
                SmirOp {
                    kind: OpKind::VCmp {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                        cond: actual_cond,
                        elem: actual_elem,
                        lanes,
                    },
                    x86_hint: Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: actual_opcode,
                    }),
                    ..
                }
            ] if *actual_cond == cond && *actual_elem == elem
                && u32::from(*lanes) == VecWidth::V64.lanes(elem)
                && *actual_opcode == opcode
        ));

        let legacy = lift_single(&[0x66, 0x0F, opcode, 0xC1]).unwrap();
        assert!(legacy.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VCmp {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                cond: actual_cond,
                elem: actual_elem,
                lanes,
                ..
            } if actual_cond == cond
                && actual_elem == elem
                && u32::from(lanes) == VecWidth::V128.lanes(elem)
        )));
        assert!(matches!(
            legacy
                .ops
                .iter()
                .find(|op| matches!(op.kind, OpKind::VCmp { .. }))
                .and_then(|op| op.x86_hint),
            Some(X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: actual,
            }) if actual == opcode
        ));
        assert!(!legacy.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VMov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                ..
            }
        )));

        let vex128 = lift_single(&[0xC5, 0xF1, opcode, 0xC2]).unwrap();
        assert!(vex128.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VCmp {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                cond: actual_cond,
                elem: actual_elem,
                ..
            } if actual_cond == cond && actual_elem == elem
        )));
        assert!(matches!(
            vex128.ops.last().and_then(|op| op.x86_hint),
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F,
                pp: X86SsePrefix::OpSize,
                opcode: actual,
                width: VecWidth::V128,
                ..
            }) if actual == opcode
        ));

        let vex256 = lift_single(&[0xC5, 0xF5, opcode, 0x00]).unwrap();
        assert!(matches!(
            vex256.ops.first().unwrap().kind,
            OpKind::VLoad {
                width: VecWidth::V256,
                ..
            }
        ));
        assert!(matches!(
            vex256.ops.last().unwrap().kind,
            OpKind::VCmp { lanes, .. }
                if u32::from(lanes) == VecWidth::V256.lanes(elem)
        ));
    }

    for (opcode, cond) in [(0x29, VecCmpCond::Eq), (0x37, VecCmpCond::Gt)] {
        let legacy = lift_single(&[0x66, 0x0F, 0x38, opcode, 0xC1]).unwrap();
        assert!(legacy.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VCmp {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                cond: actual,
                elem: VecElementType::I64,
                lanes: 2,
                ..
            } if actual == cond
        )));
        assert!(matches!(
            legacy
                .ops
                .iter()
                .find(|op| matches!(op.kind, OpKind::VCmp { .. }))
                .and_then(|op| op.x86_hint),
            Some(X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: actual,
            }) if actual == opcode
        ));
        let vex = lift_single(&[0xC4, 0xE2, 0x71, opcode, 0xC2]).unwrap();
        assert!(matches!(
            vex.ops.last().unwrap().kind,
            OpKind::VCmp {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                cond: actual,
                elem: VecElementType::I64,
                lanes: 2,
            } if actual == cond
        ));
        assert!(matches!(
            vex.ops.last().and_then(|op| op.x86_hint),
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: actual,
                width: VecWidth::V128,
                ..
            }) if actual == opcode
        ));
    }

    let legacy_memory = lift_single(&[0x66, 0x0F, 0x74, 0x00]).unwrap();
    assert!(legacy_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VCmp {
            dst: VReg::Virtual(_),
            src2: VReg::Virtual(_),
            ..
        }
    )));

    let mmx_memory = lift_single(&[0x0F, 0x74, 0x00]).unwrap();
    assert!(matches!(
        mmx_memory.ops.as_slice(),
        [
            SmirOp {
                kind: OpKind::VLoad {
                    width: VecWidth::V64,
                    ..
                },
                ..
            },
            SmirOp {
                kind: OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    ..
                },
                ..
            },
            SmirOp {
                kind: OpKind::VCmp {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                    elem: VecElementType::I8,
                    lanes: 8,
                    ..
                },
                ..
            }
        ]
    ));
    assert!(legacy_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            ..
        }
    )));

    for (opcode, elem, cond) in [
        (0x64, VecElementType::I8, VecCmpCond::Gt),
        (0x65, VecElementType::I16, VecCmpCond::Gt),
        (0x66, VecElementType::I32, VecCmpCond::Gt),
        (0x74, VecElementType::I8, VecCmpCond::Eq),
        (0x75, VecElementType::I16, VecCmpCond::Eq),
        (0x76, VecElementType::I32, VecCmpCond::Eq),
    ] {
        let evex = lift_single(&[0x62, 0xF1, 0x75, 0x09, opcode, 0xD2]).unwrap();
        assert!(evex.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VCmp {
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                cond: actual_cond,
                elem: actual_elem,
                ..
            } if actual_cond == cond && actual_elem == elem
        )));
        assert!(matches!(
            evex.ops.last().unwrap().kind,
            OpKind::And {
                dst: VReg::Arch(ArchReg::X86(X86Reg::K(2))),
                src2: SrcOperand::Reg(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                flags: FlagUpdate::None,
                ..
            }
        ));
    }

    let evex_q = lift_single(&[0x62, 0xF2, 0xF5, 0x08, 0x29, 0xC2]).unwrap();
    assert!(evex_q.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VCmp {
            cond: VecCmpCond::Eq,
            elem: VecElementType::I64,
            lanes: 2,
            ..
        }
    )));

    // EVEX high source registers and all three vector lengths are exposed.
    let high = lift_single(&[0x62, 0xB1, 0x75, 0x00, 0x74, 0xC2]).unwrap();
    assert!(high.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VCmp {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
            ..
        }
    )));
    for (ll, lanes) in [(0x08, 16), (0x28, 32), (0x48, 64)] {
        let result = lift_single(&[0x62, 0xF1, 0x75, ll, 0x74, 0xC2]).unwrap();
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VCmp {
                elem: VecElementType::I8,
                lanes: actual,
                ..
            } if actual == lanes
        )));
    }

    // Masked broadcast uses scalar disp8*N (N=4) and one predicated access
    // per dword lane; inactive lanes are fault-suppressed.
    let broadcast = lift_single(&[0x62, 0xF1, 0x75, 0x59, 0x76, 0x50, 0x10]).unwrap();
    assert!(broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Lea {
            addr: Address::BaseOffset {
                offset: 64,
                disp_size: DispSize::Disp8,
                ..
            },
            ..
        }
    )));
    assert_eq!(
        broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16
    );

    for bytes in [
        &[0xF0, 0x66, 0x0F, 0x74, 0xC1][..],       // LOCK
        &[0xF3, 0x66, 0x0F, 0x74, 0xC1][..],       // conflicting prefix
        &[0xC5, 0xF0, 0x74, 0xC1][..],             // VEX.pp != 66
        &[0xC5, 0xF1, 0x74][..],                   // missing ModR/M
        &[0x62, 0xF1, 0x75, 0x88, 0x74, 0xC1][..], // EVEX.z reserved
        &[0x62, 0xF1, 0x75, 0x68, 0x74, 0xC1][..], // EVEX.L'L=3
        &[0x62, 0xF2, 0x75, 0x08, 0x29, 0xC1][..], // VPCMPEQQ W=0
        &[0x62, 0xF1, 0x75, 0x18, 0x76, 0xC1][..], // broadcast register
        &[0x62, 0xF1, 0x75, 0x18, 0x74, 0x00][..], // byte broadcast
        &[0x62, 0x71, 0x75, 0x08, 0x74, 0xC1][..], // extended k destination
        &[0x62, 0xE1, 0x75, 0x08, 0x74, 0xC1][..], // EVEX.R' on k destination
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid packed compare accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_legacy_and_vex_gfni_cover_widths_aliases_prefixes_and_alignment() {
    for (bytes, width, src1, src2) in [
        (
            &[0xC4, 0x42, 0x31, 0xCF, 0xC2][..],
            VecWidth::V128,
            X86Reg::Xmm(9),
            X86Reg::Xmm(10),
        ),
        (
            &[0xC4, 0x42, 0x35, 0xCF, 0xC2][..],
            VecWidth::V256,
            X86Reg::Ymm(9),
            X86Reg::Ymm(10),
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VAnd {
                src1: VReg::Arch(ArchReg::X86(actual)),
                ..
            } if actual == src1
        )));
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VAnd {
                src1: VReg::Arch(ArchReg::X86(actual)),
                ..
            } if actual == src2
        )));
        assert!(matches!(
            result.ops.last().map(|op| &op.kind),
            Some(OpKind::VMov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(8) | X86Reg::Ymm(8))),
                width: actual,
                ..
            }) if *actual == width
        ));
    }

    for bytes in [
        &[0xC4, 0x43, 0xB5, 0xCE, 0xC2, 0x63][..],
        &[0xC4, 0x43, 0xB1, 0xCF, 0xC2, 0x63][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VByteShuffle {
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(10) | X86Reg::Ymm(10))),
                block_lanes: 8,
                ..
            }
        )));
    }

    // Address-size and segment prefixes before VEX remain legal and are
    // carried into the memory source rather than rejected as SIMD prefixes.
    let addr32 = lift_single(&[0x67, 0xC4, 0xE2, 0x71, 0xCF, 0x00]).unwrap();
    let addr = addr32
        .ops
        .iter()
        .find_map(|op| match &op.kind {
            OpKind::VLoad {
                addr,
                width: VecWidth::V128,
                ..
            } => Some(addr),
            _ => None,
        })
        .expect("GFNI addr32 memory source");
    super::addr32_assertions::direct(addr, X86Reg::Rax);
    let fs = lift_single(&[0x64, 0xC4, 0xE3, 0xF1, 0xCE, 0x00, 0x63]).unwrap();
    assert!(fs.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            addr: Address::SegmentRel {
                segment: VReg::Arch(ArchReg::X86(X86Reg::FsBase)),
                ..
            },
            width: VecWidth::V128,
            ..
        }
    )));

    for bytes in [
        &[0x66, 0x45, 0x0F, 0x38, 0xCF, 0xC1][..],
        &[0x66, 0x45, 0x0F, 0x3A, 0xCE, 0xC1, 0x63][..],
        &[0x66, 0x45, 0x0F, 0x3A, 0xCF, 0xC1, 0x63][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                elem: VecElementType::I8,
                ..
            }
        )));
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(8))),
                elem: VecElementType::I8,
                ..
            }
        )));
    }

    let legacy_memory = lift_single(&[0x66, 0x0F, 0x38, 0xCF, 0x00]).unwrap();
    let alignment = legacy_memory
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .unwrap();
    let load = legacy_memory
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V128,
                    ..
                }
            )
        })
        .unwrap();
    assert!(alignment < load);

    for bytes in [
        &[0xC4, 0xE2, 0xF1, 0xCF, 0xC2][..],       // VEX MULB W=1
        &[0xC4, 0xE3, 0x71, 0xCE, 0xC2, 0][..],    // VEX affine W=0
        &[0xC4, 0xE3, 0xF0, 0xCE, 0xC2, 0][..],    // VEX pp != 66
        &[0x0F, 0x38, 0xCF, 0xC1][..],             // legacy missing 66
        &[0xF3, 0x66, 0x0F, 0x38, 0xCF, 0xC1][..], // legacy REP
        &[0xF0, 0x66, 0x0F, 0x38, 0xCF, 0xC1][..], // legacy LOCK
        &[0x66, 0x0F, 0x3A, 0xCE, 0xC1][..],       // missing imm8
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid legacy/VEX GFNI form accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_legacy_and_vex_horizontal_integer_family_covers_modes_and_invalids() {
    for (opcode, elem, subtract, saturating) in [
        (0x01, VecElementType::I16, false, false),
        (0x02, VecElementType::I32, false, false),
        (0x03, VecElementType::I16, false, true),
        (0x05, VecElementType::I16, true, false),
        (0x06, VecElementType::I32, true, false),
        (0x07, VecElementType::I16, true, true),
    ] {
        let mmx = lift_single(&[0x0F, 0x38, opcode, 0xC1]).unwrap();
        assert!(matches!(
            mmx.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::VHorizontalBin {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                        elem: actual_elem,
                        lanes,
                        block_lanes,
                        subtract: actual_subtract,
                        saturating: actual_saturating,
                    },
                    x86_hint: Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: actual_opcode,
                    }),
                    ..
                },
                SmirOp {
                    kind: OpKind::X86X87Control {
                        kind: X86X87ControlKind::EnterMmx,
                        ..
                    },
                    ..
                }
            ] if *actual_elem == elem
                && *lanes == VecWidth::V64.lanes(elem) as u8
                && lanes == block_lanes
                && *actual_subtract == subtract
                && *actual_saturating == saturating
                && *actual_opcode == opcode
        ));

        let legacy = lift_single(&[0x66, 0x0F, 0x38, opcode, 0xC1]).unwrap();
        assert!(matches!(
            legacy.ops.as_slice(),
            [SmirOp {
                kind: OpKind::VHorizontalBin {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    elem: actual_elem,
                    lanes,
                    block_lanes,
                    subtract: actual_subtract,
                    saturating: actual_saturating,
                },
                x86_hint: Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: actual_opcode,
                }),
                ..
            }] if *actual_elem == elem
                && lanes == block_lanes
                && *actual_subtract == subtract
                && *actual_saturating == saturating
                && *actual_opcode == opcode
        ));

        let vex128 = lift_single(&[0xC4, 0xE2, 0x71, opcode, 0xC2]).unwrap();
        assert!(matches!(
            (&vex128.ops.last().unwrap().kind, vex128.ops.last().unwrap().x86_hint),
            (
                OpKind::VHorizontalBin {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    elem: actual_elem,
                    subtract: actual_subtract,
                    saturating: actual_saturating,
                    ..
                },
                Some(X86OpHint::VexOp {
                    map: X86VecMap::Map0F38,
                    pp: X86SsePrefix::OpSize,
                    opcode: actual_opcode,
                    width: VecWidth::V128,
                    w: false,
                })
            ) if *actual_elem == elem
                && *actual_subtract == subtract
                && *actual_saturating == saturating
                && actual_opcode == opcode
        ));

        let vex256 = lift_single(&[0xC4, 0xE2, 0x75, opcode, 0xC2]).unwrap();
        assert!(matches!(
            vex256.ops.last().unwrap().kind,
            OpKind::VHorizontalBin {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                elem: actual_elem,
                lanes,
                block_lanes,
                ..
            } if actual_elem == elem
                && u32::from(lanes) == VecWidth::V256.lanes(elem)
                && u32::from(block_lanes) == 16 / elem.bytes()
        ));
        assert!(matches!(
            vex256.ops.last().unwrap().x86_hint,
            Some(X86OpHint::VexOp {
                map: X86VecMap::Map0F38,
                pp: X86SsePrefix::OpSize,
                opcode: actual_opcode,
                width: VecWidth::V256,
                w: false,
            }) if actual_opcode == opcode
        ));
    }

    let legacy_mem = lift_single(&[0x66, 0x0F, 0x38, 0x01, 0x00]).unwrap();
    let alignment = legacy_mem
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .unwrap();
    let load = legacy_mem
        .ops
        .iter()
        .position(|op| {
            matches!(
                (&op.kind, op.x86_hint),
                (
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    },
                    Some(X86OpHint::VecAlign(X86VecAlign::Aligned))
                )
            )
        })
        .unwrap();
    assert!(alignment < load);
    assert!(legacy_mem.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            elem: VecElementType::I16,
            ..
        }
    )));

    let mmx_mem = lift_single(&[0x0F, 0x38, 0x07, 0x40, 0x01]).unwrap();
    assert!(mmx_mem.ops.iter().any(|op| matches!(
        (&op.kind, op.x86_hint),
        (
            OpKind::VLoad {
                width: VecWidth::V64,
                ..
            },
            Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
        )
    )));
    assert!(mmx_mem.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VHorizontalBin {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
            elem: VecElementType::I16,
            lanes: 4,
            block_lanes: 4,
            subtract: true,
            saturating: true,
            ..
        }
    )));
    assert!(
        !mmx_mem
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );

    let vex_mem = lift_single(&[0xC4, 0xE2, 0x75, 0x06, 0x00]).unwrap();
    assert!(vex_mem.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            width: VecWidth::V256,
            ..
        }
    )));
    assert!(
        !vex_mem
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );

    // VEX.W is ignored for this family.
    let wig = lift_single(&[0xC4, 0xE2, 0xF5, 0x01, 0xC2]).unwrap();
    assert!(matches!(
        wig.ops.last().and_then(|op| op.x86_hint),
        Some(X86OpHint::VexOp { w: true, .. })
    ));
    for bytes in [
        &[0x0F, 0x38, 0x01][..],                   // missing ModR/M
        &[0xF3, 0x66, 0x0F, 0x38, 0x01, 0xC1][..], // conflicting prefix
        &[0xF0, 0x66, 0x0F, 0x38, 0x07, 0xC1][..], // LOCK
        &[0xC4, 0xE2, 0x70, 0x01, 0xC1][..],       // VEX.pp != 66
        &[0xC4, 0xE2, 0x71, 0x01][..],             // missing ModR/M
        &[0x62, 0xF2, 0x75, 0x08, 0x01, 0xC1][..], // no EVEX PHADDW form
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid horizontal integer encoding accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_psign_family_covers_legacy_vex_alignment_aliases_and_invalids() {
    for (opcode, elem) in [
        (0x08, VecElementType::I8),
        (0x09, VecElementType::I16),
        (0x0A, VecElementType::I32),
    ] {
        let mmx = lift_single(&[0x0F, 0x38, opcode, 0xC1]).unwrap();
        assert!(matches!(
            mmx.ops.as_slice(),
            [
                SmirOp {
                    kind: OpKind::VLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src1: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                        src2: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                        elem: actual_elem,
                        lanes,
                        op: VLaneOp::Sign,
                        signed: true,
                        set_ovf: false,
                    },
                    x86_hint: Some(X86OpHint::SseOp {
                        prefix: X86SsePrefix::None,
                        opcode: actual_opcode,
                    }),
                    ..
                },
                SmirOp {
                    kind: OpKind::X86X87Control {
                        kind: X86X87ControlKind::EnterMmx,
                        ..
                    },
                    ..
                }
            ] if *actual_elem == elem
                && *lanes == VecWidth::V64.lanes(elem) as u8
                && *actual_opcode == opcode
        ));

        let legacy = lift_single(&[0x66, 0x0F, 0x38, opcode, 0xC1]).unwrap();
        assert_eq!(legacy.ops.len(), 1);
        assert!(matches!(
            (&legacy.ops[0].kind, legacy.ops[0].x86_hint),
            (
                OpKind::VLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    elem: actual_elem,
                    lanes,
                    op: VLaneOp::Sign,
                    signed: true,
                    set_ovf: false,
                },
                Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::OpSize,
                    opcode: actual_opcode,
                })
            ) if *actual_elem == elem
                && *lanes == VecWidth::V128.lanes(elem) as u8
                && actual_opcode == opcode
        ));

        for (bytes, width, dst, value, control) in [
            (
                &[0xC4, 0xE2, 0x71, opcode, 0xC2][..],
                VecWidth::V128,
                X86Reg::Xmm(0),
                X86Reg::Xmm(1),
                X86Reg::Xmm(2),
            ),
            (
                &[0xC4, 0xE2, 0x75, opcode, 0xC2][..],
                VecWidth::V256,
                X86Reg::Ymm(0),
                X86Reg::Ymm(1),
                X86Reg::Ymm(2),
            ),
        ] {
            let vex = lift_single(bytes).unwrap();
            assert_eq!(vex.ops.len(), 1);
            assert!(matches!(
                (&vex.ops[0].kind, vex.ops[0].x86_hint),
                (
                    OpKind::VLane {
                        dst: VReg::Arch(ArchReg::X86(actual_dst)),
                        src1: VReg::Arch(ArchReg::X86(actual_value)),
                        src2: VReg::Arch(ArchReg::X86(actual_control)),
                        elem: actual_elem,
                        lanes,
                        op: VLaneOp::Sign,
                        signed: true,
                        set_ovf: false,
                    },
                    Some(X86OpHint::VexOp {
                        map: X86VecMap::Map0F38,
                        pp: X86SsePrefix::OpSize,
                        opcode: actual_opcode,
                        width: actual_width,
                        w: false,
                    })
                ) if *actual_dst == dst
                    && *actual_value == value
                    && *actual_control == control
                    && *actual_elem == elem
                    && *lanes == width.lanes(elem) as u8
                    && actual_opcode == opcode
                    && actual_width == width
            ));
        }
    }

    let rex = lift_single(&[0x66, 0x44, 0x0F, 0x38, 0x08, 0xC1]).unwrap();
    assert_eq!(rex.ops.len(), 1);
    assert!(matches!(
        rex.ops[0].kind,
        OpKind::VLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(8))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(8))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            op: VLaneOp::Sign,
            ..
        }
    ));

    let legacy_mem = lift_single(&[0x66, 0x0F, 0x38, 0x09, 0x00]).unwrap();
    let alignment = legacy_mem
        .ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .unwrap();
    let load = legacy_mem
        .ops
        .iter()
        .position(|op| {
            matches!(
                (&op.kind, op.x86_hint),
                (
                    OpKind::VLoad {
                        width: VecWidth::V128,
                        ..
                    },
                    Some(X86OpHint::VecAlign(X86VecAlign::Aligned))
                )
            )
        })
        .unwrap();
    assert!(alignment < load);

    let mmx_mem = lift_single(&[0x0F, 0x38, 0x09, 0x40, 0x01]).unwrap();
    assert!(mmx_mem.ops.iter().any(|op| matches!(
        (&op.kind, op.x86_hint),
        (
            OpKind::VLoad {
                width: VecWidth::V64,
                ..
            },
            Some(X86OpHint::VecAlign(X86VecAlign::Unaligned))
        )
    )));
    assert!(mmx_mem.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
            elem: VecElementType::I16,
            lanes: 4,
            op: VLaneOp::Sign,
            ..
        }
    )));
    assert!(
        !mmx_mem
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );

    let vex_mem = lift_single(&[0xC4, 0xE2, 0x75, 0x0A, 0x00]).unwrap();
    assert!(vex_mem.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            width: VecWidth::V256,
            ..
        }
    )));
    assert!(
        !vex_mem
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );

    // VEX.W is ignored by the guest ISA but retained in the hint so the
    // lowerer can prove that it canonicalizes the host instruction.
    let wig = lift_single(&[0xC4, 0xE2, 0xF5, 0x08, 0xC2]).unwrap();
    assert!(matches!(
        wig.ops[0].x86_hint,
        Some(X86OpHint::VexOp {
            map: X86VecMap::Map0F38,
            pp: X86SsePrefix::OpSize,
            opcode: 0x08,
            width: VecWidth::V256,
            w: true,
        })
    ));
    for bytes in [
        &[0x0F, 0x38, 0x08][..],                   // missing ModR/M
        &[0xF3, 0x66, 0x0F, 0x38, 0x09, 0xC1][..], // conflicting prefix
        &[0xF0, 0x66, 0x0F, 0x38, 0x0A, 0xC1][..], // LOCK
        &[0xC4, 0xE2, 0x70, 0x08, 0xC2][..],       // VEX.pp != 66
        &[0xC4, 0xE2, 0x71, 0x09][..],             // missing ModR/M
        &[0x62, 0xF2, 0x75, 0x08, 0x08, 0xC2][..], // no EVEX PSIGN form
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid PSIGN encoding accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_avx_vnni_int8_int16_covers_all_signedness_saturation_widths_and_invalids() {
    for (bytes, elem, src1_signed, src2_signed, saturate, width) in [
        (
            &[0xC4, 0xE2, 0x6B, 0x50, 0xCB][..],
            VecElementType::I8,
            true,
            true,
            false,
            VecWidth::V128,
        ),
        (
            &[0xC4, 0xE2, 0x57, 0x51, 0xE6][..],
            VecElementType::I8,
            true,
            true,
            true,
            VecWidth::V256,
        ),
        (
            &[0xC4, 0xE2, 0x6A, 0x50, 0xCB][..],
            VecElementType::I8,
            true,
            false,
            false,
            VecWidth::V128,
        ),
        (
            &[0xC4, 0xE2, 0x56, 0x51, 0xE6][..],
            VecElementType::I8,
            true,
            false,
            true,
            VecWidth::V256,
        ),
        (
            &[0xC4, 0xE2, 0x68, 0x50, 0xCB][..],
            VecElementType::I8,
            false,
            false,
            false,
            VecWidth::V128,
        ),
        (
            &[0xC4, 0xE2, 0x54, 0x51, 0xE6][..],
            VecElementType::I8,
            false,
            false,
            true,
            VecWidth::V256,
        ),
        (
            &[0xC4, 0xE2, 0x6A, 0xD2, 0xCB][..],
            VecElementType::I16,
            true,
            false,
            false,
            VecWidth::V128,
        ),
        (
            &[0xC4, 0xE2, 0x56, 0xD3, 0xE6][..],
            VecElementType::I16,
            true,
            false,
            true,
            VecWidth::V256,
        ),
        (
            &[0xC4, 0xE2, 0x69, 0xD2, 0xCB][..],
            VecElementType::I16,
            false,
            true,
            false,
            VecWidth::V128,
        ),
        (
            &[0xC4, 0xE2, 0x55, 0xD3, 0xE6][..],
            VecElementType::I16,
            false,
            true,
            true,
            VecWidth::V256,
        ),
        (
            &[0xC4, 0xE2, 0x68, 0xD2, 0xCB][..],
            VecElementType::I16,
            false,
            false,
            false,
            VecWidth::V128,
        ),
        (
            &[0xC4, 0xE2, 0x54, 0xD3, 0xE6][..],
            VecElementType::I16,
            false,
            false,
            true,
            VecWidth::V256,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VDotProductExt {
                src_elem: actual_elem,
                acc_elem: VecElementType::I32,
                width: actual_width,
                src1_signed: actual_src1_signed,
                src2_signed: actual_src2_signed,
                saturate: actual_saturate,
                ..
            } if actual_elem == elem
                && actual_width == width
                && actual_src1_signed == src1_signed
                && actual_src2_signed == src2_signed
                && actual_saturate == saturate
        )));
    }

    let high_registers = lift_single(&[0xC4, 0x42, 0x2B, 0x50, 0xCB]).unwrap();
    assert!(matches!(
        high_registers.ops.last().unwrap().kind,
        OpKind::VDotProductExt {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(10))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(11))),
            ..
        }
    ));
    for (bytes, width) in [
        (&[0xC4, 0xE2, 0x54, 0x51, 0x20][..], VecWidth::V256),
        (&[0xC4, 0xE2, 0x69, 0xD3, 0x48, 0x01][..], VecWidth::V128),
    ] {
        let memory = lift_single(bytes).unwrap();
        assert!(memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad { width: actual, .. } if actual == width
        )));
        assert!(memory.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VDotProductExt {
                src2: VReg::Virtual(_),
                ..
            }
        )));
    }

    for bytes in [
        &[0xC4, 0xE2, 0xE8, 0x50, 0xCB][..],       // W=1 is reserved
        &[0xC4, 0xE2, 0x6B, 0xD2, 0xCB][..],       // no F2 word form
        &[0x62, 0xF2, 0x6B, 0x08, 0x50, 0xCB][..], // no EVEX byte form
        &[0x62, 0xF2, 0x6A, 0x08, 0xD2, 0xCB][..], // no EVEX word form
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
#[test]
fn lift_bf16_dot_and_converts_cover_widths_masks_fault_classes_vex_and_invalids() {
    for (bytes, width) in [
        (&[0x62, 0xF2, 0x6E, 0x08, 0x52, 0xCB][..], VecWidth::V128),
        (&[0x62, 0xA2, 0x6E, 0xC2, 0x52, 0xCB][..], VecWidth::V512),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VDotProductBF16 { width: actual, .. } if actual == width
        )));
    }
    let direct_masked = lift_single(&[0x62, 0xF2, 0x6E, 0xCC, 0x52, 0xCB]).unwrap();
    assert_eq!(direct_masked.ops.len(), 1);
    assert!(matches!(
        direct_masked.ops[0].kind,
        OpKind::VDotProductBF16 {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            acc: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
            mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(4)))),
            width: VecWidth::V512,
            zeroing: true,
        }
    ));
    let dot_broadcast = lift_single(&[0x62, 0xE2, 0x56, 0x53, 0x52, 0x60, 0x01]).unwrap();
    assert_eq!(
        dot_broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16
    );

    for (bytes, width, two_source) in [
        (&[0xC4, 0xE2, 0x7A, 0x72, 0xCA][..], VecWidth::V128, false),
        (&[0xC4, 0xE2, 0x7E, 0x72, 0xE6][..], VecWidth::V256, false),
        (
            &[0x62, 0xF2, 0x7E, 0x08, 0x72, 0xCA][..],
            VecWidth::V128,
            false,
        ),
        (
            &[0x62, 0xA2, 0x7E, 0xCA, 0x72, 0xCA][..],
            VecWidth::V512,
            false,
        ),
        (
            &[0x62, 0xF2, 0x6F, 0x08, 0x72, 0xCB][..],
            VecWidth::V128,
            true,
        ),
        (
            &[0x62, 0xA2, 0x6F, 0xC2, 0x72, 0xCB][..],
            VecWidth::V512,
            true,
        ),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        assert!(lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VCvtFP32ToBF16 {
                width: actual,
                src2,
                ..
            } if actual == width && src2.is_some() == two_source
        )));
    }

    let direct_single = lift_single(&[0x62, 0xF2, 0x7E, 0xCC, 0x72, 0xCA]).unwrap();
    assert_eq!(direct_single.ops.len(), 1);
    assert!(matches!(
        direct_single.ops[0].kind,
        OpKind::VCvtFP32ToBF16 {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            src2: None,
            mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(4)))),
            width: VecWidth::V512,
            zeroing: true,
        }
    ));
    let direct_pair = lift_single(&[0x62, 0xD2, 0x3F, 0x8B, 0x72, 0xF9]).unwrap();
    assert_eq!(direct_pair.ops.len(), 1);
    assert!(matches!(
        direct_pair.ops[0].kind,
        OpKind::VCvtFP32ToBF16 {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(7))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(8))),
            src2: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(9)))),
            mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(3)))),
            width: VecWidth::V128,
            zeroing: true,
        }
    ));

    let single_broadcast = lift_single(&[0x62, 0xE2, 0x7E, 0x5B, 0x72, 0x60, 0x01]).unwrap();
    assert_eq!(
        single_broadcast
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16
    );
    let pair_memory = lift_single(&[0x62, 0xE2, 0x57, 0x43, 0x72, 0x60, 0x01]).unwrap();
    assert!(pair_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            width: VecWidth::V512,
            ..
        }
    )));
    assert!(
        !pair_memory
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
    );
    let pair_broadcast = lift_single(&[0x62, 0xE2, 0x57, 0x53, 0x72, 0x60, 0x01]).unwrap();
    assert!(pair_broadcast.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            width: MemWidth::B4,
            ..
        }
    )));
    assert!(
        !pair_broadcast
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
    );

    for bytes in [
        &[0xC4, 0xE2, 0x6A, 0x52, 0xCB][..], // VDPBF16PS has no VEX form
        &[0x62, 0xF2, 0xEE, 0x08, 0x52, 0xCB][..], // W=1
        &[0x62, 0xF2, 0x6E, 0x68, 0x52, 0xCB][..], // L'L=3
        &[0x62, 0xF2, 0x6E, 0x18, 0x52, 0xCB][..], // register broadcast bit
        &[0xC4, 0xE2, 0x6B, 0x72, 0xCB][..], // pair convert has no VEX form
        &[0x62, 0xF2, 0x76, 0x08, 0x72, 0xCA][..], // reserved EVEX.vvvv
        &[0xC4, 0xE2, 0x72, 0x72, 0xCA][..], // reserved VEX.vvvv
        &[0x62, 0xF2, 0xFE, 0x08, 0x72, 0xCA][..], // W=1
        &[0x62, 0xF2, 0x7E, 0x18, 0x72, 0xCA][..], // register broadcast bit
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
        ));
    }
}
#[test]
fn lift_vperm2f128_vperm2i128_covers_controls_memory_aliases_and_invalids() {
    let register = lift_single(&[0xC4, 0xE3, 0x75, 0x06, 0xC2, 0x31]).unwrap();
    assert_eq!(register.bytes_consumed, 6);
    for (reg, lanes) in [(X86Reg::Ymm(1), vec![2u8, 3]), (X86Reg::Ymm(2), vec![2, 3])] {
        assert_eq!(
            register
                .ops
                .iter()
                .filter_map(|op| match op.kind {
                    OpKind::VExtractLane {
                        vec: VReg::Arch(ArchReg::X86(actual)),
                        lane,
                        elem: VecElementType::I64,
                        ..
                    } if actual == reg => Some(lane),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            lanes,
        );
    }
    assert!(matches!(
        register.ops.last().unwrap().kind,
        OpKind::VMov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
            width: VecWidth::V256,
            ..
        }
    ));

    let memory = lift_single(&[0xC4, 0x63, 0x35, 0x46, 0x40, 0x20, 0x82]).unwrap();
    assert_eq!(memory.bytes_consumed, 7);
    assert!(memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            addr: Address::BaseOffset { offset: 32, .. },
            width: VecWidth::V256,
            ..
        }
    )));
    assert!(matches!(
        memory.ops.last().unwrap().kind,
        OpKind::VMov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(8))),
            ..
        }
    ));
    assert_eq!(
        memory
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::VInsertLane { .. }))
            .count(),
        2,
    );

    let all_zero = lift_single(&[0xC4, 0xE3, 0x75, 0x06, 0xC2, 0x88]).unwrap();
    assert!(
        !all_zero
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::VExtractLane { .. }))
    );
    let alias = lift_single(&[0xC4, 0xE3, 0x75, 0x06, 0xC0, 0x01]).unwrap();
    let last_extract = alias
        .ops
        .iter()
        .rposition(|op| matches!(op.kind, OpKind::VExtractLane { .. }))
        .unwrap();
    let write = alias
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VMov {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                    ..
                }
            )
        })
        .unwrap();
    assert!(last_extract < write);

    for bytes in [
        &[0xC4, 0xE3, 0x71, 0x06, 0xC2, 0x31][..],
        &[0xC4, 0xE3, 0xF5, 0x06, 0xC2, 0x31][..],
        &[0xC4, 0xE3, 0x74, 0x46, 0xC2, 0x31][..],
        &[0x62, 0xF3, 0x75, 0x48, 0x06, 0xC2, 0x31][..],
        &[0xC4, 0xE3, 0x75, 0x46, 0xC2][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid VPERM2x128 accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_vtestps_vtestpd_masks_only_sign_bits_and_rejects_reserved_fields() {
    for (bytes, width, first, second, mask, chunks) in [
        (
            &[0xC4, 0xE2, 0x79, 0x0E, 0xD1][..],
            VecWidth::V128,
            X86Reg::Xmm(2),
            X86Reg::Xmm(1),
            0x8000_0000_8000_0000u64,
            2usize,
        ),
        (
            &[0xC4, 0x42, 0x7D, 0x0F, 0xD1][..],
            VecWidth::V256,
            X86Reg::Ymm(10),
            X86Reg::Ymm(9),
            0x8000_0000_0000_0000,
            4,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        for reg in [first, second] {
            assert_eq!(
                result
                    .ops
                    .iter()
                    .filter(|op| matches!(
                        op.kind,
                        OpKind::VExtractLane {
                            vec: VReg::Arch(ArchReg::X86(actual)),
                            ..
                        } if actual == reg
                    ))
                    .count(),
                chunks,
            );
        }
        assert_eq!(
            result
                .ops
                .iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::And {
                        src2: SrcOperand::Imm(actual),
                        flags: FlagUpdate::None,
                        ..
                    } if actual as u64 == mask
                ))
                .count(),
            chunks * 2,
        );
        assert_eq!(width.lanes(VecElementType::I64) as usize, chunks);
        assert!(matches!(
            result.ops.last().unwrap().kind,
            OpKind::WriteFlags { .. }
        ));
    }

    let memory = lift_single(&[0xC4, 0xE2, 0x7D, 0x0E, 0x58, 0x20]).unwrap();
    assert!(memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            width: VecWidth::V256,
            ..
        }
    )));
    assert!(
        !memory
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );

    for bytes in [
        &[0xC4, 0xE2, 0xF9, 0x0E, 0xD1][..],
        &[0xC4, 0xE2, 0x78, 0x0E, 0xD1][..],
        &[0xC4, 0xE2, 0x71, 0x0F, 0xD1][..],
        &[0x62, 0xF2, 0x7D, 0x08, 0x0E, 0xD1][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. } | LiftError::Unsupported { .. })
            ),
            "invalid VTEST encoding accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_mmx_pinsrw_pextrw_masks_lanes_applies_rex_to_gprs_and_preserves_fault_order() {
    let insert = lift_single(&[0x45, 0x0F, 0xC4, 0xC8, 0xFF]).unwrap();
    assert_eq!(insert.bytes_consumed, 5);
    assert!(matches!(
        insert.ops.as_slice(),
        [
            SmirOp {
                kind: OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
                ..
            },
            SmirOp {
                kind: OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                    scalar: VReg::Arch(ArchReg::X86(X86Reg::R8)),
                    lane: 3,
                    elem: VecElementType::I16,
                },
                x86_hint: Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0xC4,
                }),
                ..
            }
        ]
    ));

    let extract = lift_single(&[0x45, 0x0F, 0xC5, 0xC9, 0xFF]).unwrap();
    assert_eq!(extract.bytes_consumed, 5);
    assert!(matches!(
        extract.ops.as_slice(),
        [
            SmirOp {
                kind: OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    addr: None,
                },
                ..
            },
            SmirOp {
                kind: OpKind::VExtractLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::R9)),
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                    lane: 3,
                    elem: VecElementType::I16,
                    sign: SignExtend::Zero,
                },
                x86_hint: Some(X86OpHint::SseOp {
                    prefix: X86SsePrefix::None,
                    opcode: 0xC5,
                }),
                ..
            }
        ]
    ));

    // The memory load precedes both MMX-state entry and architectural
    // destination writeback, and REX.R remains ignored for the MM field.
    let memory = lift_single(&[0x44, 0x0F, 0xC4, 0x08, 0xFE]).unwrap();
    let load = memory
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B2,
                    ..
                }
            )
        })
        .unwrap();
    let enter = memory
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86X87Control {
                    kind: X86X87ControlKind::EnterMmx,
                    ..
                }
            )
        })
        .unwrap();
    let insert = memory
        .ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                    lane: 2,
                    elem: VecElementType::I16,
                    ..
                }
            )
        })
        .unwrap();
    assert!(load < enter && enter + 1 == insert);

    for bytes in [
        &[0x0F, 0xC5, 0x01, 0x00][..],
        &[0xF0, 0x0F, 0xC4, 0xC0, 0x00][..],
        &[0xF3, 0x0F, 0xC5, 0xC0, 0x00][..],
        &[0x0F, 0xC4, 0xC0][..],
    ] {
        assert!(matches!(
            lift_single(bytes),
            Err(LiftError::InvalidEncoding { .. } | LiftError::Incomplete { .. })
        ));
    }
}
#[test]
fn lift_map0f_pinsrw_pextrw_covers_direction_merges_tuples_high_regs_and_invalids() {
    for bytes in [
        &[0x66, 0x45, 0x0F, 0xC5, 0xC1, 0x0F][..],
        &[0xC4, 0x41, 0x79, 0xC5, 0xC1, 0x0F][..],
        &[0x62, 0x31, 0x7D, 0x08, 0xC5, 0xC1, 0x0F][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        let source = if bytes[0] == 0x62 {
            X86Reg::Xmm(17)
        } else {
            X86Reg::Xmm(9)
        };
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(actual_src)),
                lane: 7,
                elem: VecElementType::I16,
                sign: SignExtend::Zero,
                ..
            } if actual_src == source
        )));
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Mov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::R8)),
                width: OpWidth::W32,
                ..
            }
        )));
    }

    for (bytes, merge, dst) in [
        (
            &[0x66, 0x45, 0x0F, 0xC4, 0xC8, 0x0F][..],
            X86Reg::Xmm(9),
            X86Reg::Xmm(9),
        ),
        (
            &[0xC4, 0x41, 0x29, 0xC4, 0xC8, 0x0F][..],
            X86Reg::Xmm(10),
            X86Reg::Xmm(9),
        ),
        (
            &[0x62, 0xC1, 0x6D, 0x00, 0xC4, 0xC8, 0x0F][..],
            X86Reg::Xmm(18),
            X86Reg::Xmm(17),
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(actual_merge)),
                elem: VecElementType::I16,
                ..
            } if actual_merge == merge
        )));
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VInsertLane {
                lane: 7,
                elem: VecElementType::I16,
                ..
            }
        )));
        assert!(
            result.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VMov {
                    dst: VReg::Arch(ArchReg::X86(actual_dst)),
                    width: VecWidth::V128,
                    ..
                } if actual_dst == dst
            )) || merge == dst
        );
        assert!(
            result
                .ops
                .iter()
                .all(|op| op.kind.flags_written().is_empty())
        );
    }

    let memory = lift_single(&[0x62, 0xE1, 0x6D, 0x00, 0xC4, 0x48, 0x09, 0x07]).unwrap();
    assert!(memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Load {
            addr: Address::BaseOffset { offset: 18, .. },
            width: MemWidth::B2,
            ..
        }
    )));
    assert!(
        !memory
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );

    // REX.W/VEX.W/EVEX.W are ignored for both word forms.
    assert!(lift_single(&[0x66, 0x4D, 0x0F, 0xC5, 0xC1, 0x07]).is_ok());
    assert!(lift_single(&[0xC4, 0x41, 0xF9, 0xC4, 0xC8, 0x07]).is_ok());
    assert!(lift_single(&[0x62, 0x31, 0xFD, 0x08, 0xC5, 0xC1, 0x07]).is_ok());

    for bytes in [
        &[0xF0, 0x66, 0x0F, 0xC5, 0xC1, 0x07][..],
        &[0xF3, 0x66, 0x0F, 0xC4, 0xC8, 0x07][..],
        &[0x66, 0x0F, 0xC5, 0x01, 0x07][..],
        &[0x66, 0x0F, 0xC4, 0xC8][..],
        &[0xC4, 0x41, 0x71, 0xC5, 0xC1, 0x07][..],
        &[0xC4, 0x41, 0x2D, 0xC4, 0xC8, 0x07][..],
        &[0x62, 0x31, 0x7D, 0x28, 0xC5, 0xC1, 0x07][..],
        &[0x62, 0x31, 0x7D, 0x09, 0xC5, 0xC1, 0x07][..],
        // EVEX.R' cannot select a fifth-bit GPR destination.
        &[0x62, 0x21, 0x7D, 0x08, 0xC5, 0xC1, 0x07][..],
        // EVEX.X' cannot select a fifth-bit GPR source.
        &[0x62, 0x81, 0x6D, 0x00, 0xC4, 0xC8, 0x07][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid map-0F word insert/extract accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_sha512_covers_all_forms_register_roles_and_reserved_encodings() {
    for (bytes, expected) in [
        (
            &[0xC4, 0x42, 0x7F, 0xCC, 0xCA][..],
            OpKind::X86Sha512Msg1 {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(10))),
            },
        ),
        (
            &[0xC4, 0x42, 0x7F, 0xCD, 0xCA][..],
            OpKind::X86Sha512Msg2 {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Ymm(10))),
            },
        ),
        (
            &[0xC4, 0x42, 0x27, 0xCB, 0xCA][..],
            OpKind::X86Sha512Rounds2 {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                state: VReg::Arch(ArchReg::X86(X86Reg::Ymm(11))),
                wk: VReg::Arch(ArchReg::X86(X86Reg::Xmm(10))),
            },
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert_eq!(result.ops.len(), 1);
        match (&result.ops[0].kind, expected) {
            (
                OpKind::X86Sha512Msg1 { dst, src },
                OpKind::X86Sha512Msg1 {
                    dst: expected_dst,
                    src: expected_src,
                },
            )
            | (
                OpKind::X86Sha512Msg2 { dst, src },
                OpKind::X86Sha512Msg2 {
                    dst: expected_dst,
                    src: expected_src,
                },
            ) => {
                assert_eq!((*dst, *src), (expected_dst, expected_src));
            }
            (
                OpKind::X86Sha512Rounds2 { dst, state, wk },
                OpKind::X86Sha512Rounds2 {
                    dst: expected_dst,
                    state: expected_state,
                    wk: expected_wk,
                },
            ) => {
                assert_eq!(
                    (*dst, *state, *wk),
                    (expected_dst, expected_state, expected_wk)
                );
            }
            (actual, expected) => panic!("unexpected SHA-512 op: {actual:?}, {expected:?}"),
        }
        assert!(result.ops[0].kind.flags_written().is_empty());
    }

    for bytes in [
        &[0xC4, 0x42, 0x7F, 0xCC][..],
        &[0xC4, 0x42, 0x7B, 0xCC, 0xCA][..],
        &[0xC4, 0x42, 0xFF, 0xCC, 0xCA][..],
        &[0xC4, 0x42, 0x7D, 0xCC, 0xCA][..],
        &[0xC4, 0x42, 0x6F, 0xCC, 0xCA][..],
        &[0xC4, 0x42, 0x7F, 0xCC, 0x0A][..],
        &[0xC4, 0x42, 0x7B, 0xCB, 0xCA][..],
        &[0xC4, 0x42, 0xA7, 0xCB, 0xCA][..],
        &[0xC4, 0x42, 0x25, 0xCB, 0xCA][..],
        &[0xC4, 0x42, 0x27, 0xCB, 0x0A][..],
        &[0x62, 0x42, 0x7F, 0x28, 0xCC, 0xCA][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid SHA-512 encoding accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_sm3_covers_forms_memory_immediates_and_reserved_encodings() {
    for bytes in [
        &[0xC4, 0x42, 0x20, 0xDA, 0xCA][..],
        &[0xC4, 0x42, 0x21, 0xDA, 0xCA][..],
        &[0xC4, 0x43, 0x21, 0xDE, 0xCA, 0x3E][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86Sm3Msg1 {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(11))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(10))),
            } | OpKind::X86Sm3Msg2 {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(11))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(10))),
            } | OpKind::X86Sm3Rounds2 {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                state: VReg::Arch(ArchReg::X86(X86Reg::Xmm(11))),
                words: VReg::Arch(ArchReg::X86(X86Reg::Xmm(10))),
                imm: 0x3E,
            }
        )));
    }

    for bytes in [
        &[0xC4, 0x62, 0x20, 0xDA, 0x48, 0x11][..],
        &[0xC4, 0x63, 0x21, 0xDE, 0x48, 0x11, 0x01][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                addr: Address::BaseOffset { offset: 17, .. },
                width: VecWidth::V128,
                ..
            }
        )));
        assert!(
            !result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );
    }

    // Immediate bits 7:6 and bit 0 are ignored by VSM3RNDS2.
    assert!(lift_single(&[0xC4, 0x43, 0x21, 0xDE, 0xCA, 0xFF]).is_ok());
    for bytes in [
        &[0xC4, 0x42, 0x24, 0xDA, 0xCA][..],
        &[0xC4, 0x42, 0xA0, 0xDA, 0xCA][..],
        &[0xC4, 0x43, 0x25, 0xDE, 0xCA, 0x00][..],
        &[0xC4, 0x43, 0xA1, 0xDE, 0xCA, 0x00][..],
        &[0xC4, 0x43, 0x20, 0xDE, 0xCA, 0x00][..],
        &[0xC4, 0x43, 0x21, 0xDE, 0xCA][..],
        &[0x62, 0x43, 0x21, 0x08, 0xDE, 0xCA, 0x00][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid SM3 encoding accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_sm4_covers_operations_widths_memory_and_reserved_encodings() {
    for (bytes, width, key_schedule) in [
        (&[0xC4, 0x42, 0x22, 0xDA, 0xCA][..], VecWidth::V128, true),
        (&[0xC4, 0x42, 0x26, 0xDA, 0xCA][..], VecWidth::V256, true),
        (&[0xC4, 0x42, 0x23, 0xDA, 0xCA][..], VecWidth::V128, false),
        (&[0xC4, 0x42, 0x27, 0xDA, 0xCA][..], VecWidth::V256, false),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        let (expected_dst, expected_src1, expected_src2) = match width {
            VecWidth::V128 => (X86Reg::Xmm(9), X86Reg::Xmm(11), X86Reg::Xmm(10)),
            VecWidth::V256 => (X86Reg::Ymm(9), X86Reg::Ymm(11), X86Reg::Ymm(10)),
            _ => unreachable!(),
        };
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::X86Sm4 {
                dst: VReg::Arch(ArchReg::X86(dst)),
                src1: VReg::Arch(ArchReg::X86(src1)),
                src2: VReg::Arch(ArchReg::X86(src2)),
                width: actual_width,
                key_schedule: actual_key,
            } if dst == expected_dst
                && src1 == expected_src1
                && src2 == expected_src2
                && actual_width == width
                && actual_key == key_schedule
        )));
    }

    let memory = lift_single(&[0xC4, 0x62, 0x26, 0xDA, 0x48, 0x11]).unwrap();
    assert!(memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            addr: Address::BaseOffset { offset: 17, .. },
            width: VecWidth::V256,
            ..
        }
    )));
    assert!(
        !memory
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );

    for bytes in [
        &[0xC4, 0x42, 0xA2, 0xDA, 0xCA][..],
        &[0xC4, 0x42, 0xA7, 0xDA, 0xCA][..],
        &[0x62, 0x42, 0x22, 0x08, 0xDA, 0xCA][..],
        &[0xC4, 0x42, 0x22, 0xDA][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid SM4 encoding accepted: {bytes:02X?}",
        );
    }
}
#[test]
fn lift_packed_immediate_shuffle_covers_legacy_vex_memory_and_invalids() {
    let mmx_register = lift_single(&[0x0F, 0x70, 0xC1, 0x1B]).unwrap();
    assert!(mmx_register.ops.iter().any(|op| matches!(
        (&op.kind, op.x86_hint),
        (
            OpKind::X86PackedShuffleImm {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Mm(0))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Mm(1))),
                width: VecWidth::V64,
                elem: VecElementType::I16,
                imm: 0x1B,
                high_words: None,
            },
            Some(X86OpHint::SseOp {
                prefix: X86SsePrefix::None,
                opcode: 0x70,
            })
        )
    )));
    assert!(mmx_register.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86X87Control {
            kind: X86X87ControlKind::EnterMmx,
            ..
        }
    )));

    let mmx_memory = lift_single(&[0x0F, 0x70, 0x48, 0x11, 0x1B]).unwrap();
    assert!(mmx_memory.ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            addr: Address::BaseOffset { offset: 17, .. },
            width: VecWidth::V64,
            ..
        }
    )));
    assert!(
        !mmx_memory
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );

    for (bytes, width, elem, lanes, vex) in [
        (
            &[0x66, 0x45, 0x0F, 0x70, 0xCA, 0x1B][..],
            VecWidth::V128,
            VecElementType::I32,
            4,
            false,
        ),
        (
            &[0xF3, 0x45, 0x0F, 0x70, 0xCA, 0x1B][..],
            VecWidth::V128,
            VecElementType::I16,
            8,
            false,
        ),
        (
            &[0xF2, 0x45, 0x0F, 0x70, 0xCA, 0x1B][..],
            VecWidth::V128,
            VecElementType::I16,
            8,
            false,
        ),
        (
            &[0xC4, 0x41, 0x79, 0x70, 0xCA, 0x1B][..],
            VecWidth::V128,
            VecElementType::I32,
            4,
            true,
        ),
        (
            &[0xC4, 0x41, 0x7D, 0x70, 0xCA, 0x1B][..],
            VecWidth::V256,
            VecElementType::I32,
            8,
            true,
        ),
        (
            &[0xC4, 0x41, 0x7A, 0x70, 0xCA, 0x1B][..],
            VecWidth::V128,
            VecElementType::I16,
            8,
            true,
        ),
        (
            &[0xC4, 0x41, 0x7E, 0x70, 0xCA, 0x1B][..],
            VecWidth::V256,
            VecElementType::I16,
            16,
            true,
        ),
        (
            &[0xC4, 0x41, 0x7B, 0x70, 0xCA, 0x1B][..],
            VecWidth::V128,
            VecElementType::I16,
            8,
            true,
        ),
        (
            &[0xC4, 0x41, 0x7F, 0x70, 0xCA, 0x1B][..],
            VecWidth::V256,
            VecElementType::I16,
            16,
            true,
        ),
    ] {
        let result = lift_single(bytes).unwrap();
        assert_eq!(result.bytes_consumed, bytes.len());
        assert!(
            result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::VShuffle {
                src1: VReg::Arch(ArchReg::X86(src)), elem: actual_elem, lanes: actual_lanes, ..
            } if src == if width == VecWidth::V128 { X86Reg::Xmm(10) } else { X86Reg::Ymm(10) }
                && actual_elem == elem && actual_lanes == lanes))
        );
        if vex {
            assert!(
                result
                    .ops
                    .iter()
                    .any(|op| matches!(op.kind, OpKind::VShuffle {
                    dst: VReg::Arch(ArchReg::X86(dst)), ..
                } if dst == if width == VecWidth::V128 { X86Reg::Xmm(9) } else { X86Reg::Ymm(9) }))
            );
        }
        assert!(
            result
                .ops
                .iter()
                .all(|op| op.kind.flags_written().is_empty())
        );
    }

    for bytes in [
        &[0x66, 0x44, 0x0F, 0x70, 0x48, 0x11, 0x1B][..],
        &[0xC5, 0x79, 0x70, 0x48, 0x11, 0x1B][..],
        &[0xC5, 0x7E, 0x70, 0x48, 0x11, 0x1B][..],
    ] {
        let result = lift_single(bytes).unwrap();
        assert!(result.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VLoad {
                addr: Address::BaseOffset { offset: 17, .. },
                ..
            }
        )));
        assert!(
            !result
                .ops
                .iter()
                .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
        );
    }

    // VEX.W is ignored.
    assert!(lift_single(&[0xC4, 0x41, 0xF9, 0x70, 0xCA, 0x1B]).is_ok());
    for bytes in [
        &[0x0F, 0x70, 0xC1][..],
        &[0xF0, 0x66, 0x0F, 0x70, 0xCA, 0x1B][..],
        &[0xF3, 0x66, 0x0F, 0x70, 0xCA, 0x1B][..],
        &[0x66, 0x0F, 0x70, 0xCA][..],
        &[0xC4, 0x41, 0x78, 0x70, 0xCA, 0x1B][..],
        &[0xC4, 0x41, 0x71, 0x70, 0xCA, 0x1B][..],
        &[0xC4, 0x41, 0x79, 0x70, 0xCA][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid packed shuffle accepted: {bytes:02X?}"
        );
    }
}
#[test]
fn lift_vzeroupper_vzeroall_exact_register_sets_and_reserved_fields() {
    for (bytes, zero_all) in [
        (&[0xC5, 0xF8, 0x77][..], false),
        (&[0xC4, 0x61, 0xF8, 0x77][..], false),
        (&[0xC5, 0xFC, 0x77][..], true),
        (&[0xC4, 0x61, 0xFC, 0x77][..], true),
    ] {
        let lifted = lift_single(bytes).unwrap();
        assert_eq!(lifted.bytes_consumed, bytes.len());
        for index in 0u8..16 {
            assert!(lifted.ops.iter().any(|op| matches!(
                op.kind,
                OpKind::VMov {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(actual))),
                    width: VecWidth::V512,
                    ..
                } if actual == index
            )));
        }
        assert!(!lifted.ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VMov {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(16..=31))),
                ..
            }
        )));
        let extracts = lifted
            .ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::VExtractLane { .. }))
            .count();
        assert_eq!(extracts, if zero_all { 0 } else { 32 });
    }

    for bytes in [
        &[0xC5, 0xF9, 0x77][..],
        &[0xC5, 0xE8, 0x77][..],
        &[0x62, 0xF1, 0x7C, 0x08, 0x77][..],
    ] {
        assert!(
            matches!(
                lift_single(bytes),
                Err(LiftError::InvalidEncoding { .. }
                    | LiftError::Unsupported { .. }
                    | LiftError::Incomplete { .. })
            ),
            "invalid VZERO encoding accepted: {bytes:02X?}",
        );
    }
}
