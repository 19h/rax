//! part1 part 3 tests

use super::*;
use crate::smir::optimize::tests::*;
use crate::smir::optimize::*;

#[test]
fn optimizer_preserves_vex_scalar_merge_zeroing_and_load_fault_boundary() {
    use crate::smir::ir::types::{
        FpRoundMode, ShiftOp, SourceArch, VLaneOp, VecCmpCond, VecUnaryOp, VecWidth, X86Reg,
    };
    use crate::smir::ir::{FunctionBuilder, SmirFunction};
    use crate::smir::lift::x86_64::X86_64Lifter;
    use crate::smir::lift::{LiftContext, SmirLifter};

    fn optimized(bytes: &[u8]) -> SmirFunction {
        let mut lifter = X86_64Lifter::new();
        let mut lctx = LiftContext::new(SourceArch::X86_64);
        let result = lifter.lift_insn(0x1000, bytes, &mut lctx).unwrap();
        let mut builder = FunctionBuilder::new(FunctionId(0), 0x1000);
        builder.set_terminator(Terminator::Return { values: vec![] });
        let mut func = builder.finish();
        func.blocks[0].ops = result.ops;
        optimize_function(&mut func, OptLevel::O2);
        func
    }

    let arithmetic = optimized(&[0xC5, 0xF2, 0x58, 0xC2]);
    let ops = &arithmetic.blocks[0].ops;
    let last_upper_extract = ops
        .iter()
        .rposition(|op| {
            matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    lane: 3,
                    ..
                }
            )
        })
        .expect("VADDSS must retain merge-source lane extraction");
    let clear = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VBroadcast {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    ..
                }
            )
        })
        .expect("VADDSS must retain VEX upper-state clearing");
    assert!(
        last_upper_extract < clear,
        "alias-safe merge must precede clear"
    );
    assert!(ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            lane: 0,
            ..
        }
    )));

    let memory = optimized(&[0xC5, 0xFB, 0x10, 0x00]);
    let ops = &memory.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B8,
                    ..
                }
            )
        })
        .expect("faulting VMOVSD load must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VBroadcast {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    ..
                }
            )
        })
        .expect("VMOVSD destination write must survive optimization");
    assert!(
        load < destination_write,
        "destination changed before load fault boundary"
    );

    let movq = optimized(&[0xC4, 0xE1, 0xF9, 0x6E, 0x00]);
    let ops = &movq.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B8,
                    ..
                }
            )
        })
        .expect("faulting VMOVQ load must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VBroadcast {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: VecElementType::I64,
                    ..
                }
            )
        })
        .expect("VMOVQ destination clear must survive optimization");
    assert!(
        load < destination_write,
        "VMOVQ changed its destination before the load fault boundary"
    );
    assert!(ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            elem: VecElementType::I64,
            lane: 0,
            ..
        }
    )));

    let scalar_vec_movq = optimized(&[0xC5, 0xFA, 0x7E, 0x00]);
    let ops = &scalar_vec_movq.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B8,
                    ..
                }
            )
        })
        .expect("faulting scalar-vector VMOVQ load must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VBroadcast {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: VecElementType::I64,
                    ..
                }
            )
        })
        .expect("scalar-vector VMOVQ destination clear must survive optimization");
    assert!(
        load < destination_write,
        "scalar-vector VMOVQ changed its destination before the load fault boundary"
    );

    let alias = optimized(&[0xC5, 0xFA, 0x7E, 0xC0]);
    let ops = &alias.blocks[0].ops;
    let extract = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: VecElementType::I64,
                    lane: 0,
                    ..
                }
            )
        })
        .expect("same-register VMOVQ source extraction must survive optimization");
    let clear = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VBroadcast {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: VecElementType::I64,
                    ..
                }
            )
        })
        .expect("same-register VMOVQ destination clear must survive optimization");
    assert!(extract < clear, "VMOVQ alias extraction must precede clear");

    let packed_compare = optimized(&[0xC5, 0xF5, 0x74, 0x00]);
    let ops = &packed_compare.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("faulting VPCMPEQB source load must survive optimization");
    let compare = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VCmp {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                    cond: VecCmpCond::Eq,
                    elem: VecElementType::I8,
                    ..
                }
            )
        })
        .expect("VPCMPEQB architectural compare write must survive optimization");
    assert!(
        load < compare,
        "VPCMPEQB changed its destination before the source load fault boundary"
    );

    let legacy_compare = optimized(&[0x66, 0x0F, 0x66, 0xC0]);
    let ops = &legacy_compare.blocks[0].ops;
    let compare = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VCmp {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    cond: VecCmpCond::Gt,
                    elem: VecElementType::I32,
                    ..
                }
            )
        })
        .expect("same-register PCMPGTD source compare must survive optimization");
    assert_eq!(
        compare, 0,
        "direct legacy PCMPGTD must remain one atomic op"
    );
    assert!(!ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            ..
        }
    )));

    let legacy_interleave = optimized(&[0x66, 0x0F, 0x68, 0xC0]);
    let ops = &legacy_interleave.blocks[0].ops;
    assert!(matches!(
        ops.first().map(|op| (&op.kind, op.x86_hint)),
        Some((
            OpKind::VInterleave {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                elem: VecElementType::I8,
                lanes: 16,
                block_lanes: 16,
                high: true,
            },
            Some(X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x68,
            })
        ))
    ));
    assert_eq!(ops.len(), 1, "legacy unpack must remain one atomic op");

    let legacy_pack = optimized(&[0x66, 0x0F, 0x63, 0xC1]);
    let ops = &legacy_pack.blocks[0].ops;
    assert!(matches!(
        ops.first().map(|op| (&op.kind, op.x86_hint)),
        Some((
            OpKind::VPackSat {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src_elem: VecElementType::I16,
                to_unsigned: false,
                src_lanes: 8,
                block_lanes: 8,
            },
            Some(X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x63,
            })
        ))
    ));
    assert_eq!(ops.len(), 1, "legacy pack must remain one atomic op");

    let evex_compare = optimized(&[0x62, 0xF1, 0x75, 0x09, 0x76, 0x10]);
    let ops = &evex_compare.blocks[0].ops;
    let first_pred_load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            )
        })
        .expect("masked EVEX VPCMPEQD predicated source loads must survive optimization");
    let k_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::And {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::K(2))),
                    src2: SrcOperand::Reg(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                    flags: FlagUpdate::None,
                    ..
                }
            )
        })
        .expect("EVEX VPCMPEQD masked k-destination write must survive optimization");
    assert!(
        first_pred_load < k_write,
        "EVEX compare committed its k destination before masked memory accesses"
    );
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        4,
        "EVEX.128 VPCMPEQD requires one fault-suppressible load per lane"
    );

    // Immediate TRUE/FALSE predicates still carry Type E4 memory effects.
    // Their source vectors do not feed the constant result, so these cases
    // explicitly pin the optimizer's fault and commit boundaries.
    let masked_true = optimized(&[0x62, 0xF3, 0x75, 0x0C, 0x1F, 0x18, 0x07]);
    let ops = &masked_true.blocks[0].ops;
    let first_pred_load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            )
        })
        .expect("masked VPCMPD TRUE predicated loads must survive optimization");
    let k_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::And {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::K(3))),
                    src2: SrcOperand::Reg(VReg::Arch(ArchReg::X86(X86Reg::K(4)))),
                    flags: FlagUpdate::None,
                    ..
                }
            )
        })
        .expect("masked VPCMPD TRUE k-destination write must survive optimization");
    assert!(first_pred_load < k_write);
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        4,
    );

    let unmasked_false = optimized(&[0x62, 0xF3, 0x75, 0x08, 0x1F, 0x18, 0x03]);
    let ops = &unmasked_false.blocks[0].ops;
    let load = ops
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
        .expect("unmasked VPCMPD FALSE load must survive optimization");
    let k_write = ops
        .iter()
        .rposition(|op| {
            matches!(
                op.kind,
                OpKind::Mov {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::K(3))),
                    ..
                }
            )
        })
        .expect("unmasked VPCMPD FALSE destination write must survive optimization");
    assert!(load < k_write);

    let insert_memory = optimized(&[0x62, 0xF3, 0xDD, 0x2A, 0x18, 0x18, 0x01]);
    let ops = &insert_memory.blocks[0].ops;
    let load = ops
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
        .expect("E6NF VINSERTF64X2 source load must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
                    elem: VecElementType::F64,
                    ..
                }
            )
        })
        .expect("masked VINSERTF64X2 destination write must survive optimization");
    assert!(
        load < destination_write,
        "VINSERTF64X2 committed its destination before the E6NF source access"
    );

    let extract_memory = optimized(&[0x62, 0xF3, 0x7D, 0x2A, 0x39, 0x18, 0x01]);
    let ops = &extract_memory.blocks[0].ops;
    let load = ops
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
        .expect("masked E6NF VEXTRACTI32X4 destination read must survive optimization");
    let store = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VStore {
                    width: VecWidth::V128,
                    ..
                }
            )
        })
        .expect("masked E6NF VEXTRACTI32X4 destination write must survive optimization");
    assert!(
        load < store,
        "VEXTRACTI32X4 must retain the E6NF read/merge/write sequence"
    );

    let chunk_shuffle = optimized(&[0x62, 0xF3, 0x6D, 0x5A, 0x23, 0x08, 0x1B]);
    let ops = &chunk_shuffle.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B4,
                    ..
                }
            )
        })
        .expect("E4NF VSHUFF32X4 broadcast load must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                    elem: VecElementType::F32,
                    ..
                }
            )
        })
        .expect("masked VSHUFF32X4 destination write must survive optimization");
    assert!(
        load < destination_write,
        "VSHUFF32X4 committed its destination before the E4NF broadcast access"
    );
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. })),
        "E4NF VSHUFF32X4 must not turn its source into fault-suppressible loads"
    );

    let fp_class = optimized(&[0x62, 0xF3, 0xFD, 0x5D, 0x66, 0x60, 0x01, 0x20]);
    let ops = &fp_class.blocks[0].ops;
    let last_load = ops
        .iter()
        .rposition(|op| {
            matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B8,
                    ..
                }
            )
        })
        .expect("E4 VFPCLASSPD broadcast PredLoads must survive optimization");
    let daz_classification = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86VectorFpCompare {
                    elem: VecElementType::F64,
                    width: VecWidth::V512,
                    lanes: 8,
                    suppress_exceptions: true,
                    ..
                }
            )
        })
        .expect("VFPCLASSPD DAZ-aware zero classification must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::And {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::K(4))),
                    src2: SrcOperand::Reg(VReg::Arch(ArchReg::X86(X86Reg::K(5)))),
                    flags: FlagUpdate::None,
                    ..
                }
            )
        })
        .expect("masked VFPCLASSPD destination write must survive optimization");
    assert!(last_load < daz_classification && daz_classification < destination_write);
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::Load { .. } | OpKind::VLoad { .. })),
        "E4 VFPCLASSPD broadcast must retain only fault-suppressing loads"
    );

    let fp16_compare = optimized(&[0x62, 0xF3, 0x6C, 0x1A, 0xC2, 0x18, 0]);
    let ops = &fp16_compare.blocks[0].ops;
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        8,
        "VCMPPH broadcast PredLoads must survive optimization",
    );
    let last_load = ops
        .iter()
        .rposition(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .unwrap();
    let compare = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86VectorFpCompare {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::K(3))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F16,
                    width: VecWidth::V128,
                    lanes: 8,
                    predicate: 0,
                    ..
                }
            )
        })
        .expect("VCMPPH comparison must survive optimization");
    assert!(last_load < compare);

    let gfni_multiply = optimized(&[0x62, 0xF2, 0x4D, 0x4D, 0xCF, 0x60, 0x01]);
    let ops = &gfni_multiply.blocks[0].ops;
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B1,
                    ..
                }
            ))
            .count(),
        64,
        "E4 VGF2P8MULB byte PredLoads must survive optimization",
    );
    let last_load = ops
        .iter()
        .rposition(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .unwrap();
    let first_field_op = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VShift {
                    elem: VecElementType::I8,
                    ..
                }
            )
        })
        .expect("VGF2P8MULB field arithmetic removed");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(4))),
                    elem: VecElementType::I8,
                    ..
                }
            )
        })
        .expect("masked VGF2P8MULB destination write removed");
    assert!(last_load < first_field_op && first_field_op < destination_write);
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::Load { .. } | OpKind::VLoad { .. })),
        "E4 VGF2P8MULB must retain only fault-suppressing source loads"
    );

    let gfni_affine = optimized(&[0x62, 0xF3, 0xCD, 0x5D, 0xCE, 0x60, 0x01, 0x63]);
    let ops = &gfni_affine.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B8,
                    ..
                }
            )
        })
        .expect("E4NF VGF2P8AFFINEQB broadcast load removed");
    let affine = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VByteShuffle { block_lanes: 8, .. }))
        .expect("VGF2P8AFFINEQB matrix-row selection removed");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(4))),
                    elem: VecElementType::I8,
                    ..
                }
            )
        })
        .expect("masked VGF2P8AFFINEQB destination write removed");
    assert!(load < affine && affine < destination_write);
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. })),
        "E4NF VGF2P8AFFINEQB must not become fault-suppressible"
    );

    let legacy_gfni = optimized(&[0x66, 0x0F, 0x38, 0xCF, 0x00]);
    let ops = &legacy_gfni.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .expect("legacy GF2P8MULB alignment check removed");
    let load = ops
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
        .expect("legacy GF2P8MULB source load removed");
    let field_op = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VShift {
                    elem: VecElementType::I8,
                    ..
                }
            )
        })
        .expect("legacy GF2P8MULB field arithmetic removed");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: VecElementType::I8,
                    ..
                }
            )
        })
        .expect("legacy GF2P8MULB destination write removed");
    assert!(alignment < load && load < field_op && field_op < destination_write);

    let vex_gfni = optimized(&[0xC4, 0xE2, 0x71, 0xCF, 0x00]);
    let ops = &vex_gfni.blocks[0].ops;
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. })),
        "VEX VGF2P8MULB must accept unaligned memory"
    );
    let load = ops
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
        .expect("VEX VGF2P8MULB source load removed");
    let field_op = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VShift {
                    elem: VecElementType::I8,
                    ..
                }
            )
        })
        .expect("VEX VGF2P8MULB field arithmetic removed");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VMov {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    width: VecWidth::V128,
                    ..
                }
            )
        })
        .expect("VEX VGF2P8MULB destination write removed");
    assert!(load < field_op && field_op < destination_write);

    let vex_unpack = optimized(&[0xC5, 0xF5, 0x60, 0x00]);
    let ops = &vex_unpack.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("faulting VPUNPCKLBW source load must survive optimization");
    let interleave = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInterleave {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                    src2: VReg::Virtual(_),
                    elem: VecElementType::I8,
                    lanes: 32,
                    block_lanes: 16,
                    high: false,
                    ..
                }
            )
        })
        .expect("VPUNPCKLBW architectural interleave write must survive optimization");
    assert!(
        load < interleave,
        "VPUNPCKLBW changed its destination before the memory fault boundary"
    );

    let evex_unpack = optimized(&[0x62, 0xF1, 0xF5, 0x49, 0x6D, 0x00]);
    let ops = &evex_unpack.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V512,
                    ..
                }
            )
        })
        .expect("E4NF VPUNPCKHQDQ complete source load must survive optimization");
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
    );
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    elem: VecElementType::I64,
                    ..
                }
            )
        })
        .expect("masked EVEX unpack destination writes must survive optimization");
    assert!(
        load < destination_write,
        "EVEX unpack committed its destination before the complete E4NF memory access"
    );

    let vex_pack = optimized(&[0xC5, 0xF5, 0x63, 0x00]);
    let ops = &vex_pack.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("faulting VPACKSSWB source load must survive optimization");
    let pack = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VPackSat {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                    src_elem: VecElementType::I16,
                    src_lanes: 16,
                    block_lanes: 8,
                    ..
                }
            )
        })
        .expect("VPACKSSWB architectural pack write must survive optimization");
    assert!(
        load < pack,
        "VPACKSSWB changed its destination before the memory fault boundary"
    );

    let evex_pack = optimized(&[0x62, 0xF1, 0x75, 0x49, 0x6B, 0x00]);
    let ops = &evex_pack.blocks[0].ops;
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16,
        "masked EVEX.512 VPACKSSDW requires one fault-suppressible load per r/m dword"
    );
    let last_load = ops
        .iter()
        .rposition(|op| {
            matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            )
        })
        .unwrap();
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    elem: VecElementType::I16,
                    ..
                }
            )
        })
        .expect("masked EVEX pack destination writes must survive optimization");
    assert!(
        last_load < destination_write,
        "EVEX pack committed its destination before predicated memory accesses"
    );

    let evex_pack_broadcast = optimized(&[0x62, 0xF1, 0x75, 0x59, 0x6B, 0x00]);
    let ops = &evex_pack_broadcast.blocks[0].ops;
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        1,
        "masked EVEX VPACKSSDW broadcast must retain one conditional scalar read"
    );
    assert!(ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VBroadcast {
            elem: VecElementType::I32,
            lanes: 16,
            ..
        }
    )));

    let vex_pshufb = optimized(&[0xC4, 0xE2, 0x75, 0x00, 0x00]);
    let ops = &vex_pshufb.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("faulting VPSHUFB control load must survive optimization");
    let shuffle = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VByteShuffle {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                    lanes: 32,
                    block_lanes: 16,
                    ..
                }
            )
        })
        .expect("VPSHUFB architectural shuffle write must survive optimization");
    assert!(
        load < shuffle,
        "VPSHUFB changed its destination before the memory fault boundary"
    );

    let legacy_pshufb_register = optimized(&[0x66, 0x0F, 0x38, 0x00, 0xC1]);
    let ops = &legacy_pshufb_register.blocks[0].ops;
    assert!(matches!(
        ops.first().map(|op| (&op.kind, op.x86_hint)),
        Some((
            OpKind::VByteShuffle {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                control: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                lanes: 16,
                block_lanes: 16,
            },
            Some(X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x00,
            })
        ))
    ));
    assert_eq!(ops.len(), 1, "legacy PSHUFB must remain one atomic op");

    let legacy_pshufb = optimized(&[0x66, 0x0F, 0x38, 0x00, 0x00]);
    let ops = &legacy_pshufb.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .expect("legacy PSHUFB alignment check must survive optimization");
    let load = ops
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
        .expect("legacy PSHUFB aligned control load must survive optimization");
    assert!(
        alignment < load,
        "legacy PSHUFB loaded memory before its mandatory alignment check"
    );

    let evex_pshufb = optimized(&[0x62, 0xF2, 0x75, 0x49, 0x00, 0x00]);
    let ops = &evex_pshufb.blocks[0].ops;
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B1,
                    ..
                }
            ))
            .count(),
        64,
        "masked EVEX.512 VPSHUFB requires one conditional control-byte load per output"
    );
    let last_load = ops
        .iter()
        .rposition(|op| {
            matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B1,
                    ..
                }
            )
        })
        .unwrap();
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    elem: VecElementType::I8,
                    ..
                }
            )
        })
        .expect("masked EVEX VPSHUFB destination writes must survive optimization");
    assert!(
        last_load < destination_write,
        "EVEX VPSHUFB committed its destination before predicated control-byte accesses"
    );

    let legacy_horizontal_register = optimized(&[0x66, 0x0F, 0x38, 0x03, 0xC1]);
    let ops = &legacy_horizontal_register.blocks[0].ops;
    assert!(matches!(
        ops.as_slice(),
        [SmirOp {
            kind: OpKind::VHorizontalBin {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                elem: VecElementType::I16,
                lanes: 8,
                block_lanes: 8,
                subtract: false,
                saturating: true,
            },
            x86_hint: Some(X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x03,
            }),
            ..
        }]
    ));

    let legacy_horizontal = optimized(&[0x66, 0x0F, 0x38, 0x03, 0x00]);
    let ops = &legacy_horizontal.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .expect("legacy PHADDSW alignment check must survive optimization");
    let load = ops
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
        .expect("legacy PHADDSW source load must survive optimization");
    let horizontal = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VHorizontalBin {
                    elem: VecElementType::I16,
                    saturating: true,
                    subtract: false,
                    ..
                }
            )
        })
        .expect("legacy PHADDSW computation must survive optimization");
    assert!(alignment < load && load < horizontal);

    let vex_horizontal = optimized(&[0xC4, 0xE2, 0x75, 0x06, 0x00]);
    let ops = &vex_horizontal.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("faulting VPHSUBD source load must survive optimization");
    let horizontal = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VHorizontalBin {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                    elem: VecElementType::I32,
                    subtract: true,
                    saturating: false,
                    ..
                }
            )
        })
        .expect("VPHSUBD architectural write must survive optimization");
    assert!(
        load < horizontal,
        "VPHSUBD changed its destination before the memory fault boundary"
    );

    let legacy_maddubs_register = optimized(&[0x66, 0x0F, 0x38, 0x04, 0xC1]);
    assert!(matches!(
        legacy_maddubs_register.blocks[0].ops.as_slice(),
        [SmirOp {
            kind: OpKind::VDotProduct {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                acc: VReg::Imm(0),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                src_elem: VecElementType::I8,
                acc_elem: VecElementType::I16,
                width: VecWidth::V128,
                src1_unsigned: true,
                saturate: true,
                ..
            },
            x86_hint: Some(X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0x04,
            }),
            ..
        }]
    ));

    let legacy_maddubs = optimized(&[0x66, 0x0F, 0x38, 0x04, 0x00]);
    let ops = &legacy_maddubs.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .expect("legacy PMADDUBSW alignment check must survive optimization");
    let load = ops
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
        .expect("legacy PMADDUBSW source load must survive optimization");
    let dot = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VDotProduct {
                    acc_elem: VecElementType::I16,
                    saturate: true,
                    ..
                }
            )
        })
        .expect("legacy PMADDUBSW computation must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: VecElementType::I16,
                    ..
                }
            )
        })
        .expect("legacy PMADDUBSW destination merge must survive optimization");
    assert!(alignment < load && load < dot && dot < destination_write);

    let vex_maddubs = optimized(&[0xC4, 0xE2, 0x75, 0x04, 0x00]);
    let ops = &vex_maddubs.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("VEX VPMADDUBSW source load must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VDotProduct {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                    acc_elem: VecElementType::I16,
                    ..
                }
            )
        })
        .expect("VEX VPMADDUBSW architectural write must survive optimization");
    assert!(load < destination_write);

    let evex_maddubs = optimized(&[0x62, 0xF2, 0x75, 0x49, 0x04, 0x00]);
    let ops = &evex_maddubs.blocks[0].ops;
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. })),
        "E4NF EVEX.512 VPMADDUBSW must not predicate its memory read"
    );
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V512,
                    ..
                }
            )
        })
        .expect("E4NF EVEX.512 VPMADDUBSW full source load must survive optimization");
    let dot = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VDotProduct {
                    acc_elem: VecElementType::I16,
                    width: VecWidth::V512,
                    ..
                }
            )
        })
        .expect("masked EVEX VPMADDUBSW computation must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    elem: VecElementType::I16,
                    ..
                }
            )
        })
        .expect("masked EVEX VPMADDUBSW destination write must survive optimization");
    assert!(load < dot && dot < destination_write);

    let legacy_maddwd_register = optimized(&[0x66, 0x0F, 0xF5, 0xC1]);
    assert!(matches!(
        legacy_maddwd_register.blocks[0].ops.as_slice(),
        [SmirOp {
            kind: OpKind::VDotProduct {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                acc: VReg::Imm(0),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                src_elem: VecElementType::I16,
                acc_elem: VecElementType::I32,
                width: VecWidth::V128,
                src1_unsigned: false,
                saturate: false,
                ..
            },
            x86_hint: Some(X86OpHint::SseOp {
                prefix: X86SsePrefix::OpSize,
                opcode: 0xF5,
            }),
            ..
        }]
    ));

    let legacy_maddwd = optimized(&[0x66, 0x0F, 0xF5, 0x00]);
    let ops = &legacy_maddwd.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .expect("legacy PMADDWD alignment check must survive optimization");
    let load = ops
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
        .expect("legacy PMADDWD source load must survive optimization");
    let dot = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VDotProduct {
                    src_elem: VecElementType::I16,
                    acc_elem: VecElementType::I32,
                    saturate: false,
                    ..
                }
            )
        })
        .expect("legacy PMADDWD computation must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: VecElementType::I32,
                    ..
                }
            )
        })
        .expect("legacy PMADDWD destination merge must survive optimization");
    assert!(alignment < load && load < dot && dot < destination_write);

    let evex_maddwd = optimized(&[0x62, 0xF1, 0x75, 0x49, 0xF5, 0x00]);
    let ops = &evex_maddwd.blocks[0].ops;
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. })),
        "E4NF EVEX.512 VPMADDWD must not predicate its memory read"
    );
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V512,
                    ..
                }
            )
        })
        .expect("E4NF EVEX.512 VPMADDWD full source load must survive optimization");
    let dot = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VDotProduct {
                    src_elem: VecElementType::I16,
                    acc_elem: VecElementType::I32,
                    width: VecWidth::V512,
                    ..
                }
            )
        })
        .expect("masked EVEX VPMADDWD computation must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    elem: VecElementType::I32,
                    ..
                }
            )
        })
        .expect("masked EVEX VPMADDWD destination write must survive optimization");
    assert!(load < dot && dot < destination_write);

    let legacy_psign_register = optimized(&[0x66, 0x0F, 0x38, 0x09, 0xC1]);
    let ops = &legacy_psign_register.blocks[0].ops;
    assert_eq!(ops.len(), 1, "register PSIGNW must remain one atomic op");
    assert!(matches!(
        (&ops[0].kind, ops[0].x86_hint),
        (
            OpKind::VLane {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                elem: VecElementType::I16,
                lanes: 8,
                op: VLaneOp::Sign,
                signed: true,
                set_ovf: false,
            },
            Some(X86OpHint::SseOp {
                prefix: crate::smir::ir::ops::X86SsePrefix::OpSize,
                opcode: 0x09,
            })
        )
    ));

    let vex_psign_register = optimized(&[0xC4, 0xE2, 0x75, 0x0A, 0xC2]);
    let ops = &vex_psign_register.blocks[0].ops;
    assert_eq!(ops.len(), 1, "register VPSIGND must remain one atomic op");
    assert!(matches!(
        ops[0].kind,
        OpKind::VLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
            elem: VecElementType::I32,
            lanes: 8,
            op: VLaneOp::Sign,
            signed: true,
            set_ovf: false,
        }
    ));

    let legacy_psign = optimized(&[0x66, 0x0F, 0x38, 0x09, 0x00]);
    let ops = &legacy_psign.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .expect("legacy PSIGNW alignment check must survive optimization");
    let load = ops
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
        .expect("legacy PSIGNW source load must survive optimization");
    let negation = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VUnary {
                    elem: VecElementType::I16,
                    op: VecUnaryOp::Neg,
                    ..
                }
            )
        })
        .expect("legacy PSIGNW wrapping negation must survive optimization");
    let sign_select = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VBitSelect {
                    width: VecWidth::V128,
                    ..
                }
            )
        })
        .expect("legacy PSIGNW sign selection must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: VecElementType::I16,
                    ..
                }
            )
        })
        .expect("legacy PSIGNW destination merge must survive optimization");
    assert!(alignment < load && load < negation && negation < sign_select);
    assert!(sign_select < destination_write);

    let vex_psign = optimized(&[0xC4, 0xE2, 0x75, 0x0A, 0x00]);
    let ops = &vex_psign.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("VEX VPSIGND source load must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VAndNot {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(0))),
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("VEX VPSIGND architectural write must survive optimization");
    assert!(load < destination_write);

    let legacy_mulhrsw = optimized(&[0x66, 0x0F, 0x38, 0x0B, 0x00]);
    let ops = &legacy_mulhrsw.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .expect("legacy PMULHRSW alignment check must survive optimization");
    let load = ops
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
        .expect("legacy PMULHRSW load must survive optimization");
    let multiply = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VMulShiftSat {
                    lanes: 8,
                    round: true,
                    out_shift: 15,
                    ..
                }
            )
        })
        .expect("legacy PMULHRSW rounded multiply must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: VecElementType::I16,
                    ..
                }
            )
        })
        .expect("legacy PMULHRSW destination merge must survive optimization");
    assert!(alignment < load && load < multiply && multiply < destination_write);

    let evex_mulhrsw = optimized(&[0x62, 0xF2, 0x75, 0x49, 0x0B, 0x00]);
    let ops = &evex_mulhrsw.blocks[0].ops;
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        32
    );
    let last_load = ops
        .iter()
        .rposition(|op| {
            matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B2,
                    ..
                }
            )
        })
        .unwrap();
    let multiply = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VMulShiftSat {
                    lanes: 32,
                    round: true,
                    out_shift: 15,
                    ..
                }
            )
        })
        .expect("EVEX VPMULHRSW rounded multiply must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    elem: VecElementType::I16,
                    ..
                }
            )
        })
        .expect("EVEX VPMULHRSW destination write must survive optimization");
    assert!(last_load < multiply && multiply < destination_write);

    let legacy_pabs = optimized(&[0x66, 0x0F, 0x38, 0x1D, 0x00]);
    let ops = &legacy_pabs.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .expect("legacy PABSW alignment check must survive optimization");
    let load = ops
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
        .expect("legacy PABSW load must survive optimization");
    let absolute = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VUnary {
                    elem: VecElementType::I16,
                    op: VecUnaryOp::Abs,
                    ..
                }
            )
        })
        .expect("legacy PABSW absolute value must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: VecElementType::I16,
                    ..
                }
            )
        })
        .expect("legacy PABSW destination merge must survive optimization");
    assert!(alignment < load && load < absolute && absolute < destination_write);

    let evex_pabs = optimized(&[0x62, 0xF2, 0x7D, 0x49, 0x1C, 0x00]);
    let ops = &evex_pabs.blocks[0].ops;
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B1,
                    ..
                }
            ))
            .count(),
        64
    );
    let last_load = ops
        .iter()
        .rposition(|op| {
            matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B1,
                    ..
                }
            )
        })
        .unwrap();
    let absolute = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VUnary {
                    elem: VecElementType::I8,
                    lanes: 64,
                    op: VecUnaryOp::Abs,
                    ..
                }
            )
        })
        .expect("EVEX VPABSB absolute value must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    elem: VecElementType::I8,
                    ..
                }
            )
        })
        .expect("EVEX VPABSB destination write must survive optimization");
    assert!(last_load < absolute && absolute < destination_write);

    let broadcast_pabs = optimized(&[0x62, 0xF2, 0x7D, 0x59, 0x1E, 0x00]);
    let ops = &broadcast_pabs.blocks[0].ops;
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VBroadcast {
            elem: VecElementType::I32,
            lanes: 16,
            ..
        }
    )));

    let legacy_palignr = optimized(&[0x66, 0x0F, 0x3A, 0x0F, 0x00, 0x01]);
    let ops = &legacy_palignr.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .expect("legacy PALIGNR alignment check must survive optimization");
    let load = ops
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
        .expect("legacy PALIGNR source load must survive optimization");
    let shuffle = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VShuffle {
                    elem: VecElementType::I8,
                    lanes: 16,
                    ..
                }
            )
        })
        .expect("legacy PALIGNR shuffle must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: VecElementType::I8,
                    ..
                }
            )
        })
        .expect("legacy PALIGNR destination merge must survive optimization");
    assert!(alignment < load && load < shuffle && shuffle < destination_write);

    let evex_palignr = optimized(&[0x62, 0xF3, 0x75, 0x49, 0x0F, 0x00, 0x01]);
    let ops = &evex_palignr.blocks[0].ops;
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B1,
                    ..
                }
            ))
            .count(),
        60
    );
    let last_load = ops
        .iter()
        .rposition(|op| {
            matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B1,
                    ..
                }
            )
        })
        .unwrap();
    let shuffle = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VShuffle {
                    elem: VecElementType::I8,
                    lanes: 64,
                    ..
                }
            )
        })
        .expect("EVEX VPALIGNR shuffle must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    elem: VecElementType::I8,
                    ..
                }
            )
        })
        .expect("EVEX VPALIGNR destination write must survive optimization");
    assert!(last_load < shuffle && shuffle < destination_write);

    let high_only_palignr = optimized(&[0x62, 0xF3, 0x75, 0x49, 0x0F, 0x00, 0x10]);
    assert!(
        !high_only_palignr.blocks[0]
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
    );

    let legacy_pmovsxbq = optimized(&[0x66, 0x0F, 0x38, 0x22, 0x00]);
    let ops = &legacy_pmovsxbq.blocks[0].ops;
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B1,
                    ..
                }
            ))
            .count(),
        2,
        "legacy PMOVSXBQ must retain its exact two-byte fault surface"
    );
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. })),
        "legacy packed extension has no aligned-memory requirement"
    );
    let last_load = ops
        .iter()
        .rposition(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B1,
                    ..
                }
            )
        })
        .unwrap();
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: VecElementType::I64,
                    ..
                }
            )
        })
        .expect("legacy PMOVSXBQ destination merge must survive optimization");
    assert!(
        last_load < destination_write,
        "packed-extension destination write crossed its source fault boundary"
    );

    for (opcode, destination_elem, expected_loads) in [
        (0x20, VecElementType::I16, 32usize),
        (0x22, VecElementType::I64, 8usize),
    ] {
        let evex_pmov = optimized(&[0x62, 0xF2, 0x7D, 0x49, opcode, 0x00]);
        let ops = &evex_pmov.blocks[0].ops;
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B1,
                        ..
                    }
                ))
                .count(),
            expected_loads,
            "EVEX packed extension lost per-source-element predication"
        );
        assert!(!ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Load { .. } | OpKind::X86CheckAlignment { .. }
        )));
        let last_load = ops
            .iter()
            .rposition(|op| {
                matches!(
                    op.kind,
                    OpKind::PredLoad {
                        width: MemWidth::B1,
                        ..
                    }
                )
            })
            .unwrap();
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                        elem,
                        ..
                    } if elem == destination_elem
                )
            })
            .expect("EVEX packed-extension destination merge must survive optimization");
        assert!(
            last_load < destination_write,
            "EVEX packed-extension write crossed a conditional source fault boundary"
        );
    }

    for (name, bytes, expected) in [
        (
            "legacy PMINUB",
            &[0x66, 0x0F, 0xDA, 0xC1][..],
            (
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                VecElementType::I8,
                16,
                VLaneOp::Min,
                false,
            ),
        ),
        (
            "VEX.256 VPMAXSW",
            &[0xC5, 0xED, 0xEE, 0xCB][..],
            (
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
                VecElementType::I16,
                16,
                VLaneOp::Max,
                true,
            ),
        ),
        (
            "EVEX.512 VPMAXUQ",
            &[0x62, 0xA2, 0xF5, 0x40, 0x3F, 0xC2][..],
            (
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(16))),
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
                VReg::Arch(ArchReg::X86(X86Reg::Zmm(18))),
                VecElementType::I64,
                8,
                VLaneOp::Max,
                false,
            ),
        ),
    ] {
        let function = optimized(bytes);
        let ops = &function.blocks[0].ops;
        assert_eq!(ops.len(), 1, "register {name} must remain one atomic op");
        assert!(matches!(
            ops[0].kind,
            OpKind::VLane {
                dst,
                src1,
                src2,
                elem,
                lanes,
                op,
                signed,
                set_ovf: false,
            } if (dst, src1, src2, elem, lanes, op, signed) == expected
        ));
        assert!(
            ops[0].x86_hint.is_some(),
            "register {name} lost its encoding hint"
        );
    }

    let legacy_pminsb = optimized(&[0x66, 0x0F, 0x38, 0x38, 0x00]);
    let ops = &legacy_pminsb.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .expect("legacy PMINSB alignment check must survive optimization");
    let load = ops
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
        .expect("legacy PMINSB load must survive optimization");
    let compare = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VCmp {
                    elem: VecElementType::I8,
                    ..
                }
            )
        })
        .expect("legacy PMINSB comparison must survive optimization");
    let select = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VBitSelect {
                    width: VecWidth::V128,
                    ..
                }
            )
        })
        .expect("legacy PMINSB selection must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: VecElementType::I8,
                    ..
                }
            )
        })
        .expect("legacy PMINSB destination merge must survive optimization");
    assert!(alignment < load && load < compare && compare < select && select < destination_write);

    for (bytes, elem, mem_width, expected_loads) in [
        (
            &[0x62, 0xF1, 0x75, 0x49, 0xDA, 0x00][..],
            VecElementType::I8,
            MemWidth::B1,
            64usize,
        ),
        (
            &[0x62, 0xF1, 0x75, 0x49, 0xEA, 0x00][..],
            VecElementType::I16,
            MemWidth::B2,
            32usize,
        ),
        (
            &[0x62, 0xF2, 0x75, 0x49, 0x38, 0x00][..],
            VecElementType::I8,
            MemWidth::B1,
            64usize,
        ),
        (
            &[0x62, 0xF2, 0xF5, 0x59, 0x3F, 0x00][..],
            VecElementType::I64,
            MemWidth::B8,
            8usize,
        ),
    ] {
        let evex_minmax = optimized(bytes);
        let ops = &evex_minmax.blocks[0].ops;
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredLoad { width, .. } if width == mem_width
                ))
                .count(),
            expected_loads,
            "EVEX packed min/max lost elementwise fault suppression"
        );
        assert!(!ops.iter().any(|op| matches!(
            op.kind,
            OpKind::Load { .. } | OpKind::VLoad { .. } | OpKind::X86CheckAlignment { .. }
        )));
        let last_load = ops
            .iter()
            .rposition(|op| {
                matches!(
                    op.kind,
                    OpKind::PredLoad { width, .. } if width == mem_width
                )
            })
            .unwrap();
        let compare = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::VCmp { elem: actual, .. } if actual == elem))
            .unwrap();
        let select = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VBitSelect {
                        width: VecWidth::V512,
                        ..
                    }
                )
            })
            .unwrap();
        let destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VInsertLane {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                        elem: actual,
                        ..
                    } if actual == elem
                )
            })
            .unwrap();
        assert!(last_load < compare && compare < select && select < destination_write);
    }

    let legacy_ptest = optimized(&[0x66, 0x0F, 0x38, 0x17, 0x00]);
    let ops = &legacy_ptest.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .expect("legacy PTEST alignment check must survive optimization");
    let load = ops
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
        .expect("legacy PTEST load must survive optimization");
    let read_flags = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::ReadFlags { .. }))
        .expect("legacy PTEST preserved-flag capture must survive optimization");
    let write_flags = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::WriteFlags { .. }))
        .expect("legacy PTEST flag commit must survive optimization");
    assert!(alignment < load && load < read_flags && read_flags < write_flags);
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    elem: VecElementType::I64,
                    ..
                }
            ))
            .count(),
        4
    );

    let vex_ptest = optimized(&[0xC4, 0xE2, 0x7D, 0x17, 0x00]);
    let ops = &vex_ptest.blocks[0].ops;
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("VPTEST.256 load must survive optimization");
    let read_flags = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::ReadFlags { .. }))
        .unwrap();
    let write_flags = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::WriteFlags { .. }))
        .unwrap();
    assert!(load < read_flags && read_flags < write_flags);
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    elem: VecElementType::I64,
                    ..
                }
            ))
            .count(),
        8
    );
    assert!(
        ops[..load]
            .iter()
            .all(|op| op.kind.flags_written().is_empty())
    );

    let legacy_blend = optimized(&[0x66, 0x0F, 0x38, 0x10, 0x10]);
    let ops = &legacy_blend.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .expect("legacy PBLENDVB alignment check must survive optimization");
    let load = ops
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
        .expect("legacy PBLENDVB source load must survive optimization");
    let mask_compare = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VCmp {
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: VecElementType::I8,
                    cond: VecCmpCond::Lt,
                    ..
                }
            )
        })
        .expect("legacy PBLENDVB implicit mask must survive optimization");
    let select = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VBitSelect {
                    width: VecWidth::V128,
                    ..
                }
            )
        })
        .expect("legacy PBLENDVB selection must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    elem: VecElementType::I8,
                    ..
                }
            )
        })
        .expect("legacy PBLENDVB destination merge must survive optimization");
    assert!(
        alignment < load
            && load < mask_compare
            && mask_compare < select
            && select < destination_write
    );

    let vex_blend = optimized(&[0xC4, 0xE3, 0x65, 0x4A, 0x10, 0x40]);
    let ops = &vex_blend.blocks[0].ops;
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("VBLENDVPS memory source must survive optimization");
    let mask_compare = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VCmp {
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(4))),
                    elem: VecElementType::I32,
                    cond: VecCmpCond::Lt,
                    ..
                }
            )
        })
        .expect("VBLENDVPS explicit mask must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VBitSelect {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("VBLENDVPS destination write must survive optimization");
    assert!(load < mask_compare && mask_compare < destination_write);
    assert!(ops.iter().all(|op| op.kind.flags_written().is_empty()));

    let legacy_pmuldq = optimized(&[0x66, 0x0F, 0x38, 0x28, 0x00]);
    let ops = &legacy_pmuldq.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .unwrap();
    let load = ops
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
    let multiply = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::MulS {
                    width: OpWidth::W64,
                    flags: FlagUpdate::None,
                    ..
                }
            )
        })
        .unwrap();
    let write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: VecElementType::I64,
                    ..
                }
            )
        })
        .unwrap();
    assert!(alignment < load && load < multiply && multiply < write);

    let evex_pmuldq = optimized(&[0x62, 0xF2, 0xF5, 0x49, 0x28, 0x00]);
    let ops = &evex_pmuldq.blocks[0].ops;
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B8,
                    ..
                }
            ))
            .count(),
        8
    );
    let last_load = ops
        .iter()
        .rposition(|op| {
            matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B8,
                    ..
                }
            )
        })
        .unwrap();
    let multiply = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::MulS {
                    width: OpWidth::W64,
                    ..
                }
            )
        })
        .unwrap();
    let write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    elem: VecElementType::I64,
                    ..
                }
            )
        })
        .unwrap();
    assert!(last_load < multiply && multiply < write);

    for (name, bytes, width, alignment, dst) in [
        (
            "legacy MOVNTDQA",
            &[0x66, 0x0F, 0x38, 0x2A, 0x00][..],
            VecWidth::V128,
            16,
            X86Reg::Xmm(0),
        ),
        (
            "VEX.256 VMOVNTDQA",
            &[0xC4, 0xE2, 0x7D, 0x2A, 0x00][..],
            VecWidth::V256,
            32,
            X86Reg::Ymm(0),
        ),
        (
            "EVEX.512 VMOVNTDQA",
            &[0x62, 0xE2, 0x7D, 0x48, 0x2A, 0x00][..],
            VecWidth::V512,
            64,
            X86Reg::Zmm(16),
        ),
    ] {
        let function = optimized(bytes);
        let ops = &function.blocks[0].ops;
        let alignment_check = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::X86CheckAlignment {
                        alignment: actual,
                        ..
                    } if actual == alignment
                )
            })
            .unwrap_or_else(|| panic!("{name}: mandatory alignment check was removed"));
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: actual,
                        ..
                    } if actual == width
                )
            })
            .unwrap_or_else(|| panic!("{name}: memory load was removed"));
        let destination_write = ops
            .iter()
            .position(|op| match op.kind {
                OpKind::VMov {
                    dst: VReg::Arch(ArchReg::X86(actual)),
                    width: actual_width,
                    ..
                } => actual == dst && actual_width == width,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(actual)),
                    ..
                } => actual == dst,
                _ => false,
            })
            .unwrap_or_else(|| panic!("{name}: architectural destination write was removed"));
        assert!(
            alignment_check < load && load < destination_write,
            "{name}: optimizer violated check-before-load-before-write ordering: {ops:?}"
        );
    }

    for bytes in [
        &[0x66, 0x0F, 0x38, 0x41, 0xC1][..],
        &[0xC4, 0xE2, 0x79, 0x41, 0xC1][..],
    ] {
        let register = optimized(bytes);
        let ops = &register.blocks[0].ops;
        assert!(matches!(
            ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86Phminposuw {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                },
                x86_hint: Some(_),
                ..
            }]
        ));
        assert!(ops[0].kind.flags_written().is_empty());
    }

    for (bytes, dst, src, elem, lanes, dst_width) in [
        (
            &[0x0F, 0x50, 0xC1][..],
            X86Reg::Rax,
            X86Reg::Xmm(1),
            VecElementType::F32,
            4,
            OpWidth::W32,
        ),
        (
            &[0x66, 0x48, 0x0F, 0x50, 0xD1][..],
            X86Reg::Rdx,
            X86Reg::Xmm(1),
            VecElementType::F64,
            2,
            OpWidth::W64,
        ),
        (
            &[0x66, 0x45, 0x0F, 0xD7, 0xCA][..],
            X86Reg::R9,
            X86Reg::Xmm(10),
            VecElementType::I8,
            16,
            OpWidth::W32,
        ),
        (
            &[0xC4, 0x41, 0xFC, 0x50, 0xC1][..],
            X86Reg::R8,
            X86Reg::Ymm(9),
            VecElementType::F32,
            8,
            OpWidth::W32,
        ),
        (
            &[0xC4, 0x41, 0xFD, 0xD7, 0xCA][..],
            X86Reg::R9,
            X86Reg::Ymm(10),
            VecElementType::I8,
            32,
            OpWidth::W32,
        ),
    ] {
        let register = optimized(bytes);
        let ops = &register.blocks[0].ops;
        assert!(matches!(
            ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86MovMask {
                    dst: VReg::Arch(ArchReg::X86(actual_dst)),
                    src: VReg::Arch(ArchReg::X86(actual_src)),
                    elem: actual_elem,
                    lanes: actual_lanes,
                    dst_width: actual_dst_width,
                },
                x86_hint: Some(_),
                ..
            }] if *actual_dst == dst
                && *actual_src == src
                && *actual_elem == elem
                && *actual_lanes == lanes
                && *actual_dst_width == dst_width
        ));
        assert!(ops[0].kind.flags_written().is_empty());
    }

    for (bytes, dst, src, width, zero_upper) in [
        (
            &[0x66, 0x0F, 0x6E, 0xC1][..],
            X86Reg::Xmm(0),
            X86Reg::Rcx,
            OpWidth::W32,
            false,
        ),
        (
            &[0x66, 0x4D, 0x0F, 0x7E, 0xCA][..],
            X86Reg::R10,
            X86Reg::Xmm(9),
            OpWidth::W64,
            false,
        ),
        (
            &[0xC5, 0xF9, 0x6E, 0xC1][..],
            X86Reg::Xmm(0),
            X86Reg::Rcx,
            OpWidth::W32,
            true,
        ),
        (
            &[0x62, 0xC1, 0xFD, 0x08, 0x6E, 0xC8][..],
            X86Reg::Xmm(17),
            X86Reg::R8,
            OpWidth::W64,
            true,
        ),
    ] {
        let register = optimized(bytes);
        let ops = &register.blocks[0].ops;
        assert!(matches!(
            ops.as_slice(),
            [SmirOp {
                kind: OpKind::X86MovdQ {
                    dst: VReg::Arch(ArchReg::X86(actual_dst)),
                    src: VReg::Arch(ArchReg::X86(actual_src)),
                    width: actual_width,
                    zero_upper: actual_zero_upper,
                },
                x86_hint: Some(_),
                ..
            }] if *actual_dst == dst
                && *actual_src == src
                && *actual_width == width
                && *actual_zero_upper == zero_upper
        ));
        assert!(ops[0].kind.flags_written().is_empty());
    }

    let vmovw_load = optimized(&[0x62, 0xF5, 0x7D, 0x08, 0x6E, 0x48, 0x7F]);
    let ops = &vmovw_load.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    addr: Address::BaseOffset { offset: 254, .. },
                    width: MemWidth::B2,
                    ..
                }
            )
        })
        .expect("faulting VMOVW compressed-displacement load must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    elem: VecElementType::I16,
                    lane: 0,
                    ..
                }
            )
        })
        .expect("VMOVW vector destination write must survive optimization");
    assert!(load < destination_write);

    let vmovw_store = optimized(&[0x62, 0xF5, 0x7D, 0x08, 0x7E, 0x48, 0x7F]);
    let ops = &vmovw_store.blocks[0].ops;
    let extract = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    elem: VecElementType::I16,
                    lane: 0,
                    ..
                }
            )
        })
        .expect("VMOVW scalar extraction must survive optimization");
    let store = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Store {
                    addr: Address::BaseOffset { offset: 254, .. },
                    width: MemWidth::B2,
                    ..
                }
            )
        })
        .expect("faulting VMOVW compressed-displacement store must survive optimization");
    assert!(extract < store);

    let legacy_phminposuw = optimized(&[0x66, 0x0F, 0x38, 0x41, 0x00]);
    let ops = &legacy_phminposuw.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .expect("legacy PHMINPOSUW alignment check must survive optimization");
    let load = ops
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
        .expect("legacy PHMINPOSUW source load must survive optimization");
    let read_flags = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::ReadFlags { .. }))
        .expect("PHMINPOSUW flag preservation capture must survive optimization");
    let first_compare = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Cmp {
                    width: OpWidth::W16,
                    ..
                }
            )
        })
        .expect("PHMINPOSUW minimum comparisons must survive optimization");
    let write_flags = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::WriteFlags { .. }))
        .expect("PHMINPOSUW flag restoration must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: VecElementType::I64,
                    ..
                }
            )
        })
        .expect("legacy PHMINPOSUW destination write must survive optimization");
    assert!(
        alignment < load
            && load < read_flags
            && read_flags < first_compare
            && first_compare < write_flags
            && write_flags < destination_write
    );
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::SetCC {
                    cond: Condition::Ult,
                    ..
                }
            ))
            .count(),
        7
    );

    let vex_phminposuw = optimized(&[0xC4, 0xE2, 0x79, 0x41, 0x00]);
    let ops = &vex_phminposuw.blocks[0].ops;
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );
    let load = ops
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
        .expect("VEX VPHMINPOSUW unaligned source load must survive optimization");
    let read_flags = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::ReadFlags { .. }))
        .unwrap();
    let write_flags = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::WriteFlags { .. }))
        .unwrap();
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VMov {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    width: VecWidth::V128,
                    ..
                }
            )
        })
        .expect("VEX VPHMINPOSUW zeroing destination write must survive optimization");
    assert!(load < read_flags && read_flags < write_flags && write_flags < destination_write);

    for (name, bytes, width, products, legacy_alignment, dst) in [
        (
            "legacy PCLMULQDQ",
            &[0x66, 0x0F, 0x3A, 0x44, 0x00, 0x11][..],
            VecWidth::V128,
            1usize,
            true,
            X86Reg::Xmm(0),
        ),
        (
            "VEX.256 VPCLMULQDQ",
            &[0xC4, 0xE3, 0x75, 0x44, 0x00, 0x11][..],
            VecWidth::V256,
            2,
            false,
            X86Reg::Ymm(0),
        ),
        (
            "EVEX.512 VPCLMULQDQ",
            &[0x62, 0xF3, 0x75, 0x48, 0x44, 0x00, 0x11][..],
            VecWidth::V512,
            4,
            false,
            X86Reg::Zmm(0),
        ),
    ] {
        let function = optimized(bytes);
        let ops = &function.blocks[0].ops;
        let alignment = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }));
        assert_eq!(alignment.is_some(), legacy_alignment, "{name}");
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: actual,
                        ..
                    } if actual == width
                )
            })
            .unwrap_or_else(|| panic!("{name}: full source load was removed"));
        let first_product = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::ClMul {
                        elem_bits: 64,
                        lanes: 1,
                        acc: false,
                        ..
                    }
                )
            })
            .unwrap_or_else(|| panic!("{name}: carry-less products were removed"));
        let last_product = ops
            .iter()
            .rposition(|op| {
                matches!(
                    op.kind,
                    OpKind::ClMul {
                        elem_bits: 64,
                        lanes: 1,
                        acc: false,
                        ..
                    }
                )
            })
            .unwrap();
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::ClMul {
                        elem_bits: 64,
                        lanes: 1,
                        acc: false,
                        ..
                    }
                ))
                .count(),
            products,
            "{name}"
        );
        let destination_write = ops
            .iter()
            .position(|op| match op.kind {
                OpKind::VMov {
                    dst: VReg::Arch(ArchReg::X86(actual)),
                    width: actual_width,
                    ..
                } => actual == dst && actual_width == width,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(actual)),
                    ..
                } => actual == dst,
                _ => false,
            })
            .unwrap_or_else(|| panic!("{name}: architectural result write was removed"));
        if let Some(alignment) = alignment {
            assert!(alignment < load, "{name}");
        }
        assert!(
            load < first_product
                && first_product <= last_product
                && last_product < destination_write,
            "{name}: optimizer violated load/product/write ordering: {ops:?}"
        );
        assert!(
            !ops.iter()
                .any(|op| matches!(op.kind, OpKind::PredLoad { .. })),
            "{name}: PCLMULQDQ must not acquire memory fault suppression"
        );
        assert!(ops.iter().all(|op| op.kind.flags_written().is_empty()));
    }

    let crc_memory = optimized(&[0xF2, 0x4C, 0x0F, 0x38, 0xF1, 0x00]);
    let ops = &crc_memory.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B8,
                    ..
                }
            )
        })
        .expect("CRC32 qword memory read must survive optimization");
    let crc = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Crc32C {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::R8)),
                    crc: VReg::Arch(ArchReg::X86(X86Reg::R8)),
                    data_width: OpWidth::W64,
                    ..
                }
            )
        })
        .expect("CRC32 architectural result must survive optimization");
    assert!(load < crc);
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );
    assert!(ops.iter().all(|op| op.kind.flags_written().is_empty()));

    let crc_high_byte = optimized(&[0xF2, 0x0F, 0x38, 0xF0, 0xD5]);
    let ops = &crc_high_byte.blocks[0].ops;
    let extraction = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Shr {
                    src: VReg::Arch(ArchReg::X86(X86Reg::Rcx)),
                    amount: SrcOperand::Imm(8),
                    flags: FlagUpdate::None,
                    ..
                }
            )
        })
        .expect("CRC32 CH extraction must survive optimization");
    let crc = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Crc32C {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Rdx)),
                    data_width: OpWidth::W8,
                    ..
                }
            )
        })
        .unwrap();
    assert!(extraction < crc);

    let crc_alias = optimized(&[0xF2, 0x4D, 0x0F, 0x38, 0xF1, 0xC0]);
    assert!(crc_alias.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Crc32C {
            dst: VReg::Arch(ArchReg::X86(X86Reg::R8)),
            crc: VReg::Arch(ArchReg::X86(X86Reg::R8)),
            data: VReg::Arch(ArchReg::X86(X86Reg::R8)),
            data_width: OpWidth::W64,
        }
    )));

    let legacy_blend_imm = optimized(&[0x66, 0x0F, 0x3A, 0x0C, 0x00, 0xA5]);
    let ops = &legacy_blend_imm.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .expect("legacy BLENDPS alignment check must survive optimization");
    let load = ops
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
        .expect("legacy BLENDPS memory source must survive optimization");
    let selection = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VExtractLane {
                    elem: VecElementType::I32,
                    ..
                }
            )
        })
        .expect("legacy BLENDPS lane selection must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    elem: VecElementType::I32,
                    ..
                }
            )
        })
        .expect("legacy BLENDPS destination merge must survive optimization");
    assert!(alignment < load && load < selection && selection < destination_write);

    let vex_blend_imm = optimized(&[0xC4, 0xE3, 0x65, 0x0C, 0x08, 0xA5]);
    let ops = &vex_blend_imm.blocks[0].ops;
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("VEX VBLENDPS unaligned source load must survive optimization");
    let first_selection = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VExtractLane {
                    elem: VecElementType::I32,
                    ..
                }
            )
        })
        .unwrap();
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VMov {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("VEX VBLENDPS destination write must survive optimization");
    assert!(load < first_selection && first_selection < destination_write);
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    elem: VecElementType::I32,
                    ..
                }
            ))
            .count(),
        8
    );
    assert!(ops.iter().all(|op| op.kind.flags_written().is_empty()));

    let legacy_insert = optimized(&[0x66, 0x44, 0x0F, 0x3A, 0x22, 0x08, 0x03]);
    let ops = &legacy_insert.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B4,
                    ..
                }
            )
        })
        .expect("faulting PINSRD scalar load must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                    elem: VecElementType::I32,
                    ..
                }
            )
        })
        .expect("PINSRD architectural merge must survive optimization");
    assert!(load < destination_write);

    let vector_insert = optimized(&[0xC4, 0x63, 0x29, 0x22, 0x48, 0x14, 0x03]);
    let ops = &vector_insert.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B4,
                    ..
                }
            )
        })
        .expect("faulting VPINSRD scalar load must survive optimization");
    let first_merge_read = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(10))),
                    elem: VecElementType::I32,
                    ..
                }
            )
        })
        .expect("VPINSRD merge-source reads must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VMov {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                    width: VecWidth::V128,
                    ..
                }
            )
        })
        .expect("VPINSRD destination write must survive optimization");
    assert!(load < first_merge_read && first_merge_read < destination_write);

    let extract = optimized(&[0x66, 0x44, 0x0F, 0x3A, 0x15, 0x48, 0x22, 0x0F]);
    let ops = &extract.blocks[0].ops;
    let extraction = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                    lane: 7,
                    elem: VecElementType::I16,
                    ..
                }
            )
        })
        .expect("PEXTRW source lane extraction must survive optimization");
    let store = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Store {
                    width: MemWidth::B2,
                    ..
                }
            )
        })
        .expect("PEXTRW scalar store must survive optimization");
    assert!(extraction < store);

    let mpsadbw = optimized(&[0x66, 0x44, 0x0F, 0x3A, 0x42, 0x08, 0x07]);
    let ops = &mpsadbw.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .expect("legacy MPSADBW alignment check must survive optimization");
    let load = ops
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
        .expect("legacy MPSADBW source load must survive optimization");
    let sad = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VMpsadbw {
                    width: VecWidth::V128,
                    ..
                }
            )
        })
        .expect("legacy MPSADBW operation must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                    elem: VecElementType::I16,
                    ..
                }
            )
        })
        .expect("legacy MPSADBW destination merge must survive optimization");
    assert!(alignment < load && load < sad && sad < destination_write);

    let vex_mpsadbw = optimized(&[0xC4, 0x63, 0x25, 0x42, 0x08, 0x38]);
    let ops = &vex_mpsadbw.blocks[0].ops;
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("VEX VMPSADBW unaligned load must survive optimization");
    let sad = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VMpsadbw {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("VEX VMPSADBW destination operation must survive optimization");
    assert!(load < sad);

    let vdbpsadbw = optimized(&[0x62, 0xF3, 0x6D, 0x4A, 0x42, 0x48, 0x01, 0xE4]);
    let ops = &vdbpsadbw.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    addr: Address::BaseOffset { offset: 64, .. },
                    width: VecWidth::V512,
                    ..
                }
            )
        })
        .expect("E4NF VDBPSADBW full source load must survive optimization");
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::VMpsadbw {
                    width: VecWidth::V512,
                    ..
                }
            ))
            .count(),
        4,
        "VDBPSADBW requires all four projected SAD calculations",
    );
    let first_sad = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VMpsadbw { .. }))
        .unwrap();
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                    elem: VecElementType::I16,
                    ..
                }
            )
        })
        .expect("masked VDBPSADBW destination write removed");
    assert!(load < first_sad && first_sad < destination_write);
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. })),
        "E4NF VDBPSADBW must not become fault-suppressible"
    );

    let packed_fp16_sqrt = optimized(&[0x62, 0xF5, 0x7C, 0x4A, 0x51, 0x48, 0x01]);
    let ops = &packed_fp16_sqrt.blocks[0].ops;
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        32,
        "masked VSQRTPH requires one fault-suppressing load per FP16 lane",
    );
    let first_load = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .unwrap();
    let sqrt = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VFP16Arith {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    op: Avx10FP16Op::Sqrt,
                    width: VecWidth::V512,
                    ..
                }
            )
        })
        .expect("masked VSQRTPH operation must survive optimization");
    assert!(first_load < sqrt);

    let packed_fp16_min = optimized(&[0x62, 0xF5, 0x6C, 0x4A, 0x5D, 0x48, 0x01]);
    let ops = &packed_fp16_min.blocks[0].ops;
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        32,
        "masked VMINPH requires one fault-suppressing load per FP16 lane",
    );
    let min = ops
        .iter()
        .find(|op| {
            matches!(
                op.kind,
                OpKind::VFP16Arith {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    op: Avx10FP16Op::Min,
                    round: FpRoundMode::Dynamic,
                    width: VecWidth::V512,
                    ..
                }
            )
        })
        .expect("masked VMINPH operation must survive optimization");
    assert!(min.kind.has_side_effects());

    let scalar_fp16_sqrt = optimized(&[0x62, 0xF5, 0x6E, 0x0A, 0x51, 0x48, 0x7F]);
    let ops = &scalar_fp16_sqrt.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::PredLoad {
                    addr: Address::BaseOffset { offset: 254, .. },
                    width: MemWidth::B2,
                    ..
                }
            )
        })
        .expect("masked VSQRTSH scalar source load must survive optimization");
    let sqrt = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VFP16Arith {
                    dst: VReg::Virtual(_),
                    op: Avx10FP16Op::Sqrt,
                    width: VecWidth::V128,
                    ..
                }
            )
        })
        .expect("VSQRTSH scalar square root must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    lane: 7,
                    elem: VecElementType::F16,
                    ..
                }
            )
        })
        .expect("VSQRTSH scalar merge destination must survive optimization");
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    lane: 1..=7,
                    elem: VecElementType::F16,
                    ..
                }
            ))
            .count(),
        7,
        "VSQRTSH must preserve all seven upper FP16 lanes from SRC1",
    );
    assert!(load < sqrt && sqrt < destination_write);

    let scalar_fp16_div = optimized(&[0x62, 0xF5, 0x6E, 0x0A, 0x5E, 0x48, 0x7F]);
    let ops = &scalar_fp16_div.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::PredLoad {
                    addr: Address::BaseOffset { offset: 254, .. },
                    width: MemWidth::B2,
                    ..
                }
            )
        })
        .expect("masked VDIVSH scalar source load must survive optimization");
    let divide = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VFP16Arith {
                    dst: VReg::Virtual(_),
                    op: Avx10FP16Op::Div,
                    width: VecWidth::V128,
                    ..
                }
            )
        })
        .expect("VDIVSH scalar division must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    lane: 7,
                    elem: VecElementType::F16,
                    ..
                }
            )
        })
        .expect("VDIVSH scalar merge destination must survive optimization");
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    lane: 1..=7,
                    elem: VecElementType::F16,
                    ..
                }
            ))
            .count(),
        7,
        "VDIVSH must preserve all seven upper FP16 lanes from SRC1",
    );
    assert!(load < divide && divide < destination_write);

    let scalar_fp16_move = optimized(&[0x62, 0xA5, 0x6E, 0x83, 0x10, 0xCB]);
    let ops = &scalar_fp16_move.blocks[0].ops;
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(18))),
                    lane: 1..=7,
                    elem: VecElementType::F16,
                    ..
                }
            ))
            .count(),
        7,
        "masked VMOVSH must preserve all seven upper FP16 lanes from SRC1",
    );
    assert!(ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VInsertLane {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(17))),
            lane: 7,
            elem: VecElementType::F16,
            ..
        }
    )));

    let scalar_fp16_load = optimized(&[0x62, 0xF5, 0x7E, 0x0A, 0x10, 0x48, 0x7F]);
    let ops = &scalar_fp16_load.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::PredLoad {
                    addr: Address::BaseOffset { offset: 254, .. },
                    width: MemWidth::B2,
                    ..
                }
            )
        })
        .expect("masked VMOVSH scalar load must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VBroadcast {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    elem: VecElementType::F16,
                    lanes: 1,
                    ..
                }
            )
        })
        .expect("masked VMOVSH load destination write must survive optimization");
    assert!(load < destination_write);

    let scalar_fp16_store = optimized(&[0x62, 0xF5, 0x7E, 0x0A, 0x11, 0x50, 0x7F]);
    let ops = &scalar_fp16_store.blocks[0].ops;
    let source_read = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VExtractLane {
                    vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    lane: 0,
                    elem: VecElementType::F16,
                    ..
                }
            )
        })
        .expect("masked VMOVSH store source read must survive optimization");
    let store = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::PredStore {
                    addr: Address::BaseOffset { offset: 254, .. },
                    width: MemWidth::B2,
                    ..
                }
            )
        })
        .expect("masked VMOVSH scalar store must survive optimization");
    assert!(source_read < store);

    let psadbw = optimized(&[0x66, 0x44, 0x0F, 0xF6, 0x08]);
    let ops = &psadbw.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .expect("legacy PSADBW alignment check must survive optimization");
    let load = ops
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
        .expect("legacy PSADBW source load must survive optimization");
    let sad = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VSadBytes {
                    width: VecWidth::V128,
                    ..
                }
            )
        })
        .expect("legacy PSADBW operation must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                    elem: VecElementType::I64,
                    ..
                }
            )
        })
        .expect("legacy PSADBW destination merge must survive optimization");
    assert!(alignment < load && load < sad && sad < destination_write);

    let evex_psadbw = optimized(&[0x62, 0xE1, 0x5D, 0x40, 0xF6, 0x18]);
    let ops = &evex_psadbw.blocks[0].ops;
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V512,
                    ..
                }
            )
        })
        .expect("EVEX VPSADBW unaligned load must survive optimization");
    let sad = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VSadBytes {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(19))),
                    width: VecWidth::V512,
                    ..
                }
            )
        })
        .expect("EVEX VPSADBW destination operation must survive optimization");
    assert!(load < sad);

    let dpps = optimized(&[0x66, 0x44, 0x0F, 0x3A, 0x40, 0x08, 0xF1]);
    let ops = &dpps.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .expect("legacy DPPS alignment check must survive optimization");
    let load = ops
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
        .expect("legacy DPPS source load must survive optimization");
    let dot = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86DotProduct {
                    elem: VecElementType::F32,
                    width: VecWidth::V128,
                    imm: 0xF1,
                    ..
                }
            )
        })
        .expect("legacy DPPS MXCSR side effect must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                    elem: VecElementType::F32,
                    ..
                }
            )
        })
        .expect("legacy DPPS destination merge must survive optimization");
    assert!(alignment < load && load < dot && dot < destination_write);

    let vdpps = optimized(&[0xC4, 0x63, 0x25, 0x40, 0x08, 0xFF]);
    let ops = &vdpps.blocks[0].ops;
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { .. }))
    );
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("VEX VDPPS unaligned load must survive optimization");
    let dot = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86DotProduct {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                    elem: VecElementType::F32,
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("VEX VDPPS destination and MXCSR operation must survive optimization");
    assert!(load < dot);

    for (bytes, expected_sources) in [
        (
            &[0xC4, 0x42, 0x7F, 0xCC, 0xCA][..],
            vec![
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(10))),
            ],
        ),
        (
            &[0xC4, 0x42, 0x7F, 0xCD, 0xCA][..],
            vec![
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(10))),
            ],
        ),
        (
            &[0xC4, 0x42, 0x27, 0xCB, 0xCA][..],
            vec![
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
                VReg::Arch(ArchReg::X86(X86Reg::Ymm(11))),
                VReg::Arch(ArchReg::X86(X86Reg::Xmm(10))),
            ],
        ),
    ] {
        let sha = optimized(bytes);
        let operation = sha.blocks[0]
            .ops
            .iter()
            .find(|op| {
                matches!(
                    op.kind,
                    OpKind::X86Sha512Msg1 { .. }
                        | OpKind::X86Sha512Msg2 { .. }
                        | OpKind::X86Sha512Rounds2 { .. }
                )
            })
            .expect("SHA-512 operation must survive optimization");
        assert_eq!(operation.kind.source_vregs(), expected_sources);
    }

    for (bytes, rounds) in [
        (&[0xC4, 0x62, 0x20, 0xDA, 0x08][..], false),
        (&[0xC4, 0x63, 0x21, 0xDE, 0x08, 0x3F][..], true),
    ] {
        let sm3 = optimized(bytes);
        let ops = &sm3.blocks[0].ops;
        let load = ops
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
            .expect("SM3 memory source load must survive optimization");
        let operation = ops
            .iter()
            .position(|op| {
                if rounds {
                    matches!(op.kind, OpKind::X86Sm3Rounds2 { .. })
                } else {
                    matches!(op.kind, OpKind::X86Sm3Msg1 { .. })
                }
            })
            .expect("SM3 operation must survive optimization");
        assert!(load < operation);
    }

    for bytes in [
        &[0xC4, 0x62, 0x26, 0xDA, 0x08][..],
        &[0xC4, 0x62, 0x27, 0xDA, 0x08][..],
    ] {
        let sm4 = optimized(bytes);
        let ops = &sm4.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::VLoad {
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("SM4 memory source load must survive optimization");
        let operation = ops
            .iter()
            .position(|op| {
                matches!(
                    op.kind,
                    OpKind::X86Sm4 {
                        width: VecWidth::V256,
                        ..
                    }
                )
            })
            .expect("SM4 operation must survive optimization");
        assert!(load < operation);
    }

    let round = optimized(&[0x66, 0x44, 0x0F, 0x3A, 0x08, 0x08, 0x00]);
    let ops = &round.blocks[0].ops;
    let alignment = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
        .expect("legacy ROUNDPS alignment check must survive optimization");
    let load = ops
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
        .expect("legacy ROUNDPS source load must survive optimization");
    let rounding = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86Round {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                    elem: VecElementType::F32,
                    lanes: 4,
                    ..
                }
            )
        })
        .expect("ROUNDPS MXCSR side effect and destination must survive optimization");
    assert!(alignment < load && load < rounding);

    let vex_round = optimized(&[0xC4, 0x63, 0x21, 0x0B, 0x08, 0x04]);
    let ops = &vex_round.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B8,
                    ..
                }
            )
        })
        .expect("VROUNDSD scalar load must survive optimization");
    let rounding = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86Round {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                    merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(11))),
                    mode: FpRoundMode::Dynamic,
                    ..
                }
            )
        })
        .expect("VROUNDSD merge and MXCSR side effect must survive optimization");
    assert!(load < rounding);

    let evex = optimized(&[0x62, 0xF1, 0x7E, 0x09, 0x58, 0x10]);
    let ops = &evex.blocks[0].ops;
    assert!(ops.iter().any(|op| matches!(
        op.kind,
        OpKind::And {
            src1: VReg::Arch(ArchReg::X86(X86Reg::K(1))),
            ..
        }
    )));
    let pred_load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            )
        })
        .expect("masked EVEX memory source must retain conditional load");
    assert!(!ops.iter().any(|op| matches!(op.kind, OpKind::Load { .. })));
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VBroadcast {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    ..
                }
            )
        })
        .expect("masked EVEX arithmetic must retain destination clear/write");
    assert!(pred_load < destination_write);
    assert!(
        ops.iter()
            .any(|op| matches!(op.kind, OpKind::Select { .. }))
    );

    let legacy_sqrt = optimized(&[0x0F, 0x51, 0x00]);
    let ops = &legacy_sqrt.blocks[0].ops;
    let load = ops
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
        .expect("faulting packed SQRTPS load must survive optimization");
    let first_destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    ..
                }
            )
        })
        .expect("legacy SQRTPS XMM merge must survive optimization");
    assert!(
        load < first_destination_write,
        "legacy SQRTPS changed its destination before the load fault boundary"
    );
    assert!(ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VUnary {
            dst: VReg::Virtual(_),
            op: VecUnaryOp::FSqrt,
            ..
        }
    )));

    let evex_sqrt = optimized(&[0x62, 0xF1, 0x7E, 0x09, 0x51, 0x10]);
    let ops = &evex_sqrt.blocks[0].ops;
    let pred_load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            )
        })
        .expect("masked EVEX VSQRTSS conditional load must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VBroadcast {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    ..
                }
            )
        })
        .expect("masked EVEX VSQRTSS destination clear/write must survive optimization");
    assert!(pred_load < destination_write);
    assert!(ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VUnary {
            op: VecUnaryOp::FSqrt,
            ..
        }
    )));
    assert!(
        ops.iter()
            .any(|op| matches!(op.kind, OpKind::Select { .. }))
    );

    let legacy_min = optimized(&[0xF3, 0x0F, 0x5D, 0x00]);
    let ops = &legacy_min.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B4,
                    ..
                }
            )
        })
        .expect("faulting MINSS load must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    ..
                }
            )
        })
        .expect("MINSS destination merge must survive optimization");
    assert!(load < destination_write);
    assert!(ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VX86MinMax {
            min: true,
            lanes: 1,
            ..
        }
    )));

    let evex_min = optimized(&[0x62, 0xF1, 0x7E, 0x09, 0x5D, 0x10]);
    let ops = &evex_min.blocks[0].ops;
    let pred_load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            )
        })
        .expect("masked EVEX VMINSS conditional load must survive optimization");
    let destination_write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VBroadcast {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    ..
                }
            )
        })
        .expect("masked EVEX VMINSS destination write must survive optimization");
    assert!(pred_load < destination_write);
    assert!(ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VX86MinMax {
            min: true,
            lanes: 1,
            ..
        }
    )));
    assert!(
        ops.iter()
            .any(|op| matches!(op.kind, OpKind::Select { .. }))
    );

    let comi = optimized(&[0x66, 0x0F, 0x2F, 0x00]);
    let ops = &comi.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B8,
                    ..
                }
            )
        })
        .expect("faulting COMISD load must survive optimization");
    let compare = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86FpCompare {
                    elem: VecElementType::F64,
                    signaling: true,
                    ..
                }
            )
        })
        .expect("COMISD flag producer must survive optimization");
    assert!(load < compare);
    assert_eq!(ops[compare].kind.flags_written(), FlagSet::ALL_X86);

    let fp16_comi = optimized(&[0x62, 0xF5, 0x7C, 0x08, 0x2F, 0x50, 0x7F]);
    let ops = &fp16_comi.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    addr: Address::BaseOffset { offset: 254, .. },
                    width: MemWidth::B2,
                    ..
                }
            )
        })
        .expect("faulting VCOMISH compressed-displacement load must survive optimization");
    let compare = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86FpCompare {
                    elem: VecElementType::F16,
                    signaling: true,
                    ..
                }
            )
        })
        .expect("VCOMISH flag producer must survive optimization");
    assert!(load < compare);
    assert_eq!(ops[compare].kind.flags_written(), FlagSet::ALL_X86);

    let fp_to_int = optimized(&[0xF2, 0x48, 0x0F, 0x2D, 0x00]);
    let ops = &fp_to_int.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B8,
                    ..
                }
            )
        })
        .expect("faulting CVTSD2SI load must survive optimization");
    let conversion = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86FpToInt {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                    elem: VecElementType::F64,
                    int_width: OpWidth::W64,
                    signed: true,
                    truncate: false,
                    ..
                }
            )
        })
        .expect("CVTSD2SI conversion must survive optimization");
    assert!(load < conversion);
    assert!(ops[conversion].kind.flags_written().is_empty());

    let fp16_to_int = optimized(&[0x62, 0xF5, 0x7E, 0x08, 0x2D, 0x40, 0x7F]);
    let ops = &fp16_to_int.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    addr: Address::BaseOffset { offset: 254, .. },
                    width: MemWidth::B2,
                    ..
                }
            )
        })
        .expect("faulting VCVTSH2SI compressed-displacement load must survive optimization");
    let conversion = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86FpToInt {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Rax)),
                    elem: VecElementType::F16,
                    int_width: OpWidth::W32,
                    signed: true,
                    truncate: false,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    ..
                }
            )
        })
        .expect("VCVTSH2SI conversion must survive optimization");
    assert!(load < conversion);

    let fp16_er = optimized(&[0x62, 0xF5, 0x7E, 0x38, 0x2D, 0xC3]);
    assert!(fp16_er.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86FpToInt {
            elem: VecElementType::F16,
            int_width: OpWidth::W32,
            signed: true,
            truncate: false,
            round: FpRoundMode::RoundDown,
            suppress_exceptions: true,
            ..
        }
    )));

    let unsigned_fp_to_int = optimized(&[0x62, 0xF1, 0x7E, 0x08, 0x79, 0x40, 0x7F]);
    let ops = &unsigned_fp_to_int.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    addr: Address::BaseOffset { offset: 508, .. },
                    width: MemWidth::B4,
                    ..
                }
            )
        })
        .expect("faulting VCVTSS2USI compressed-displacement load must survive optimization");
    let conversion = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86FpToInt {
                    elem: VecElementType::F32,
                    int_width: OpWidth::W32,
                    signed: false,
                    truncate: false,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    ..
                }
            )
        })
        .expect("VCVTSS2USI conversion must survive optimization");
    assert!(load < conversion);

    let unsigned_er = optimized(&[0x62, 0xF1, 0x7E, 0x38, 0x79, 0xC3]);
    assert!(unsigned_er.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86FpToInt {
            elem: VecElementType::F32,
            signed: false,
            truncate: false,
            round: FpRoundMode::RoundDown,
            suppress_exceptions: true,
            ..
        }
    )));

    let int_to_fp = optimized(&[0xF2, 0x48, 0x0F, 0x2A, 0x08]);
    let ops = &int_to_fp.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B8,
                    sign: SignExtend::Sign,
                    ..
                }
            )
        })
        .expect("faulting CVTSI2SD load must survive optimization");
    let conversion = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86IntToFp {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
                    elem: VecElementType::F64,
                    int_width: OpWidth::W64,
                    signed: true,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    zero_upper: false,
                    ..
                }
            )
        })
        .expect("CVTSI2SD conversion must survive optimization");
    assert!(load < conversion);
    assert!(ops[conversion].kind.flags_written().is_empty());

    let unsigned_int_to_fp = optimized(&[0x62, 0xF1, 0x6E, 0x08, 0x7B, 0x48, 0x7F]);
    let ops = &unsigned_int_to_fp.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    addr: Address::BaseOffset { offset: 508, .. },
                    width: MemWidth::B4,
                    sign: SignExtend::Zero,
                    ..
                }
            )
        })
        .expect("faulting VCVTUSI2SS compressed-displacement load must survive optimization");
    let conversion = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86IntToFp {
                    elem: VecElementType::F32,
                    int_width: OpWidth::W32,
                    signed: false,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    zero_upper: true,
                    ..
                }
            )
        })
        .expect("VCVTUSI2SS conversion must survive optimization");
    assert!(load < conversion);

    let fp16_er = optimized(&[0x62, 0xF5, 0xEE, 0x38, 0x7B, 0xC8]);
    assert!(fp16_er.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86IntToFp {
            elem: VecElementType::F16,
            int_width: OpWidth::W64,
            signed: false,
            round: FpRoundMode::RoundDown,
            suppress_exceptions: true,
            zero_upper: true,
            ..
        }
    )));

    let fp_convert = optimized(&[0xF2, 0x0F, 0x5A, 0x00]);
    let ops = &fp_convert.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::Load {
                    width: MemWidth::B8,
                    ..
                }
            )
        })
        .expect("faulting CVTSD2SS load must survive optimization");
    let conversion = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86FpConvert {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    merge: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    from: VecElementType::F64,
                    to: VecElementType::F32,
                    zero_upper: false,
                    ..
                }
            )
        })
        .expect("CVTSD2SS conversion must survive optimization");
    assert!(load < conversion);
    assert!(ops[conversion].kind.flags_written().is_empty());

    let masked_scalar_fp_convert = optimized(&[0x62, 0xF5, 0x7C, 0x09, 0x1D, 0x08]);
    let ops = &masked_scalar_fp_convert.blocks[0].ops;
    let mask_condition = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::And {
                    src1: VReg::Arch(ArchReg::X86(X86Reg::K(1))),
                    flags: FlagUpdate::None,
                    ..
                }
            )
        })
        .expect("VCVTSS2SH mask condition must survive optimization");
    let predicated_load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    signed: SignExtend::Zero,
                    ..
                }
            )
        })
        .expect("VCVTSS2SH fault-suppressing load must survive optimization");
    let conversion = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86FpConvert {
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                    from: VecElementType::F32,
                    to: VecElementType::F16,
                    mask_zeroing: false,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    zero_upper: true,
                    ..
                }
            )
        })
        .expect("masked VCVTSS2SH conversion must survive optimization");
    assert!(mask_condition < predicated_load && predicated_load < conversion);
    assert!(
        ops[conversion]
            .kind
            .source_vregs()
            .contains(&VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))))
    );
    assert!(ops[..predicated_load].iter().any(|op| matches!(
        op.kind,
        OpKind::Mov {
            src: SrcOperand::Imm(0),
            ..
        }
    )));

    let packed_fp_convert = optimized(&[0x66, 0x0F, 0x5A, 0x00]);
    let ops = &packed_fp_convert.blocks[0].ops;
    let load = ops
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
        .expect("faulting CVTPD2PS load must survive optimization");
    let conversion = ops
        .iter()
        .position(|op| {
            matches!(
                op,
                SmirOp {
                    kind: OpKind::X86PackedFpConvert {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                        from: VecElementType::F64,
                        to: VecElementType::F32,
                        lanes: 2,
                        dst_width: VecWidth::V128,
                        zero_upper: false,
                        ..
                    },
                    x86_hint: Some(X86OpHint::SseOp { .. }),
                    ..
                }
            )
        })
        .expect("CVTPD2PS conversion must survive optimization");
    assert!(load < conversion);
    assert!(ops[conversion].kind.flags_written().is_empty());

    let evex_packed_fp_convert = optimized(&[0x62, 0xF1, 0x7C, 0x4B, 0x5A, 0x00]);
    let ops = &evex_packed_fp_convert.blocks[0].ops;
    let conversion = ops
        .iter()
        .position(|op| {
            matches!(
                op,
                SmirOp {
                    kind: OpKind::X86PackedFpConvert {
                        dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                        mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(3)))),
                        lanes: 8,
                        ..
                    },
                    x86_hint: Some(X86OpHint::EvexOp { .. }),
                    ..
                }
            )
        })
        .expect("masked EVEX packed conversion removed");
    assert_eq!(
        ops[..conversion]
            .iter()
            .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
            .count(),
        8,
        "per-lane fault-suppressing loads must precede conversion"
    );

    let fp16_packed_convert = optimized(&[0x62, 0xF5, 0x7C, 0x09, 0x5A, 0x00]);
    let ops = &fp16_packed_convert.blocks[0].ops;
    let conversion = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86PackedFpConvert {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                    from: VecElementType::F16,
                    to: VecElementType::F64,
                    lanes: 2,
                    mask_zeroing: false,
                    ..
                }
            )
        })
        .expect("masked VCVTPH2PD conversion removed");
    assert!(ops[conversion].kind.has_side_effects());
    assert_eq!(
        ops[..conversion]
            .iter()
            .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
            .count(),
        2
    );
    let sources = ops[conversion].kind.source_vregs();
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::K(1)))));

    let get_exponent = optimized(&[0x62, 0xF2, 0x7D, 0x5A, 0x42, 0x00]);
    let ops = &get_exponent.blocks[0].ops;
    let getexp = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86GetExponent {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F32,
                    width: VecWidth::V512,
                    lanes: 16,
                    scalar: false,
                    mask_zeroing: false,
                    suppress_exceptions: false,
                    ..
                }
            )
        })
        .expect("masked broadcast VGETEXPPS removed");
    assert!(ops[getexp].kind.has_side_effects());
    assert_eq!(
        ops[..getexp]
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        1,
    );
    let sources = ops[getexp].kind.source_vregs();
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Zmm(0)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::K(2)))));
    assert!(
        !OpKind::X86GetExponent {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            merge: None,
            src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            mask: None,
            elem: VecElementType::F32,
            width: VecWidth::V512,
            lanes: 16,
            scalar: false,
            mask_zeroing: false,
            suppress_exceptions: true,
        }
        .has_side_effects()
    );

    let get_mantissa = optimized(&[0x62, 0xF3, 0x7D, 0x5A, 0x26, 0x00, 0x03]);
    let ops = &get_mantissa.blocks[0].ops;
    let getmant = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86GetMantissa {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F32,
                    width: VecWidth::V512,
                    lanes: 16,
                    imm: 3,
                    scalar: false,
                    mask_zeroing: false,
                    suppress_exceptions: false,
                    ..
                }
            )
        })
        .expect("masked broadcast VGETMANTPS removed");
    assert!(ops[getmant].kind.has_side_effects());
    assert_eq!(
        ops[..getmant]
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        1,
    );
    let sources = ops[getmant].kind.source_vregs();
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Zmm(0)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::K(2)))));
    assert!(
        !OpKind::X86GetMantissa {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            merge: None,
            src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            mask: None,
            elem: VecElementType::F32,
            width: VecWidth::V512,
            lanes: 16,
            imm: 3,
            scalar: false,
            mask_zeroing: false,
            suppress_exceptions: true,
        }
        .has_side_effects()
    );

    let round_scale = optimized(&[0x62, 0xF3, 0x7D, 0x5A, 0x08, 0x00, 0x53]);
    let ops = &round_scale.blocks[0].ops;
    let rndscale = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86RoundScale {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F32,
                    width: VecWidth::V512,
                    lanes: 16,
                    imm: 0x53,
                    scalar: false,
                    mask_zeroing: false,
                    suppress_exceptions: false,
                    ..
                }
            )
        })
        .expect("masked broadcast VRNDSCALEPS removed");
    assert!(ops[rndscale].kind.has_side_effects());
    assert_eq!(
        ops[..rndscale]
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        1,
    );
    let sources = ops[rndscale].kind.source_vregs();
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Zmm(0)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::K(2)))));
    assert!(
        !OpKind::X86RoundScale {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            merge: None,
            src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            mask: None,
            elem: VecElementType::F32,
            width: VecWidth::V512,
            lanes: 16,
            imm: 0x53,
            scalar: false,
            mask_zeroing: false,
            suppress_exceptions: true,
        }
        .has_side_effects()
    );

    let reduce = optimized(&[0x62, 0xF3, 0x7D, 0x5A, 0x56, 0x00, 0x53]);
    let ops = &reduce.blocks[0].ops;
    let reduce = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86Reduce {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F32,
                    width: VecWidth::V512,
                    lanes: 16,
                    imm: 0x53,
                    scalar: false,
                    mask_zeroing: false,
                    suppress_exceptions: false,
                    ..
                }
            )
        })
        .expect("masked broadcast VREDUCEPS removed");
    assert!(ops[reduce].kind.has_side_effects());
    assert_eq!(
        ops[..reduce]
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        1,
    );
    let sources = ops[reduce].kind.source_vregs();
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Zmm(0)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::K(2)))));
    assert!(
        !OpKind::X86Reduce {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            merge: None,
            src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            mask: None,
            elem: VecElementType::F32,
            width: VecWidth::V512,
            lanes: 16,
            imm: 0x53,
            scalar: false,
            mask_zeroing: false,
            suppress_exceptions: true,
        }
        .has_side_effects()
    );

    let range = optimized(&[0x62, 0xF3, 0x6D, 0x5A, 0x50, 0x00, 0x05]);
    let ops = &range.blocks[0].ops;
    let range = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86Range {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
                    src2: VReg::Virtual(_),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F32,
                    width: VecWidth::V512,
                    lanes: 16,
                    imm: 0x05,
                    scalar: false,
                    mask_zeroing: false,
                    suppress_exceptions: false,
                }
            )
        })
        .expect("masked broadcast VRANGEPS removed");
    assert!(ops[range].kind.has_side_effects());
    assert_eq!(
        ops[..range]
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        1,
    );
    let sources = ops[range].kind.source_vregs();
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Zmm(0)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Zmm(2)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::K(2)))));
    assert!(
        !OpKind::X86Range {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
            mask: None,
            elem: VecElementType::F32,
            width: VecWidth::V512,
            lanes: 16,
            imm: 0x05,
            scalar: false,
            mask_zeroing: false,
            suppress_exceptions: true,
        }
        .has_side_effects()
    );

    let fixup = optimized(&[0x62, 0xF3, 0x6D, 0x5A, 0x54, 0x00, 0xFF]);
    let ops = &fixup.blocks[0].ops;
    let fixup = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86FixupImm {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
                    src2: VReg::Virtual(_),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F32,
                    width: VecWidth::V512,
                    lanes: 16,
                    imm: 0xFF,
                    scalar: false,
                    mask_zeroing: false,
                    suppress_exceptions: false,
                }
            )
        })
        .expect("masked broadcast VFIXUPIMMPS removed");
    assert!(ops[fixup].kind.has_side_effects());
    assert_eq!(
        ops[..fixup]
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        1,
    );
    let sources = ops[fixup].kind.source_vregs();
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Zmm(0)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Zmm(2)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::K(2)))));
    assert!(
        !OpKind::X86FixupImm {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
            mask: None,
            elem: VecElementType::F32,
            width: VecWidth::V512,
            lanes: 16,
            imm: 0xFF,
            scalar: false,
            mask_zeroing: false,
            suppress_exceptions: true,
        }
        .has_side_effects()
    );
    assert!(
        !OpKind::X86FixupImm {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
            mask: None,
            elem: VecElementType::F32,
            width: VecWidth::V512,
            lanes: 16,
            imm: 0,
            scalar: false,
            mask_zeroing: false,
            suppress_exceptions: false,
        }
        .has_side_effects()
    );

    let exp2 = optimized(&[0x62, 0xF2, 0x7D, 0x5A, 0xC8, 0x00]);
    let ops = &exp2.blocks[0].ops;
    let exp2 = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86Exp2 {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    src: VReg::Virtual(_),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F32,
                    width: VecWidth::V512,
                    lanes: 16,
                    mask_zeroing: false,
                    suppress_exceptions: false,
                }
            )
        })
        .expect("masked broadcast VEXP2PS removed");
    assert!(ops[exp2].kind.has_side_effects());
    assert_eq!(
        ops[..exp2]
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        1,
    );
    let sources = ops[exp2].kind.source_vregs();
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Zmm(0)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::K(2)))));
    assert!(
        !OpKind::X86Exp2 {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            mask: None,
            elem: VecElementType::F32,
            width: VecWidth::V512,
            lanes: 16,
            mask_zeroing: false,
            suppress_exceptions: true,
        }
        .has_side_effects()
    );

    let recip14 = optimized(&[0x62, 0xF2, 0x7D, 0x5A, 0x4C, 0x00]);
    let ops = &recip14.blocks[0].ops;
    let recip14 = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86Recip14 {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    merge: None,
                    src: VReg::Virtual(_),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F32,
                    width: VecWidth::V512,
                    lanes: 16,
                    scalar: false,
                    mask_zeroing: false,
                }
            )
        })
        .expect("masked broadcast VRCP14PS removed");
    assert!(!ops[recip14].kind.has_side_effects());
    assert_eq!(
        ops[..recip14]
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        1,
    );
    let sources = ops[recip14].kind.source_vregs();
    assert!(sources.iter().any(|reg| matches!(reg, VReg::Virtual(_))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Zmm(0)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::K(2)))));

    let scalar_recip14 = OpKind::X86Recip14 {
        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
        src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
        elem: VecElementType::F64,
        width: VecWidth::V128,
        lanes: 1,
        scalar: true,
        mask_zeroing: false,
    };
    assert!(!scalar_recip14.has_side_effects());
    let sources = scalar_recip14.source_vregs();
    for source in [
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        VReg::Arch(ArchReg::X86(X86Reg::K(1))),
    ] {
        assert!(sources.contains(&source));
    }

    let rsqrt14 = optimized(&[0x62, 0xF2, 0x7D, 0x5A, 0x4E, 0x00]);
    let ops = &rsqrt14.blocks[0].ops;
    let rsqrt14 = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86Rsqrt14 {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    merge: None,
                    src: VReg::Virtual(_),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F32,
                    width: VecWidth::V512,
                    lanes: 16,
                    scalar: false,
                    mask_zeroing: false,
                }
            )
        })
        .expect("masked broadcast VRSQRT14PS removed");
    assert!(!ops[rsqrt14].kind.has_side_effects());
    assert_eq!(
        ops[..rsqrt14]
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        1,
    );
    let sources = ops[rsqrt14].kind.source_vregs();
    assert!(sources.iter().any(|reg| matches!(reg, VReg::Virtual(_))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Zmm(0)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::K(2)))));

    let scalar_rsqrt14 = OpKind::X86Rsqrt14 {
        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
        src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
        elem: VecElementType::F64,
        width: VecWidth::V128,
        lanes: 1,
        scalar: true,
        mask_zeroing: false,
    };
    assert!(!scalar_rsqrt14.has_side_effects());
    let sources = scalar_rsqrt14.source_vregs();
    for source in [
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        VReg::Arch(ArchReg::X86(X86Reg::K(1))),
    ] {
        assert!(sources.contains(&source));
    }

    let recip28 = optimized(&[0x62, 0xF2, 0x7D, 0x5A, 0xCA, 0x00]);
    let ops = &recip28.blocks[0].ops;
    let recip28 = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86Recip28 {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    merge: None,
                    src: VReg::Virtual(_),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F32,
                    width: VecWidth::V512,
                    lanes: 16,
                    scalar: false,
                    mask_zeroing: false,
                    suppress_exceptions: false,
                }
            )
        })
        .expect("masked broadcast VRCP28PS removed");
    assert!(ops[recip28].kind.has_side_effects());
    assert_eq!(
        ops[..recip28]
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        1,
    );
    let sources = ops[recip28].kind.source_vregs();
    assert!(sources.iter().any(|reg| matches!(reg, VReg::Virtual(_))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Zmm(0)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::K(2)))));

    let scalar_recip28 = OpKind::X86Recip28 {
        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
        src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
        elem: VecElementType::F64,
        width: VecWidth::V128,
        lanes: 1,
        scalar: true,
        mask_zeroing: false,
        suppress_exceptions: false,
    };
    assert!(scalar_recip28.has_side_effects());
    let sources = scalar_recip28.source_vregs();
    for source in [
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        VReg::Arch(ArchReg::X86(X86Reg::K(1))),
    ] {
        assert!(sources.contains(&source));
    }
    let mut scalar_sae = scalar_recip28;
    let OpKind::X86Recip28 {
        mask,
        mask_zeroing,
        suppress_exceptions,
        ..
    } = &mut scalar_sae
    else {
        unreachable!()
    };
    *mask = None;
    *mask_zeroing = false;
    *suppress_exceptions = true;
    assert!(!scalar_sae.has_side_effects());

    let rsqrt28 = optimized(&[0x62, 0xF2, 0x7D, 0x5A, 0xCC, 0x00]);
    let ops = &rsqrt28.blocks[0].ops;
    let rsqrt28 = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86Rsqrt28 {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    merge: None,
                    src: VReg::Virtual(_),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F32,
                    width: VecWidth::V512,
                    lanes: 16,
                    scalar: false,
                    mask_zeroing: false,
                    suppress_exceptions: false,
                }
            )
        })
        .expect("masked broadcast VRSQRT28PS removed");
    assert!(ops[rsqrt28].kind.has_side_effects());
    assert_eq!(
        ops[..rsqrt28]
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        1,
    );
    let sources = ops[rsqrt28].kind.source_vregs();
    assert!(sources.iter().any(|reg| matches!(reg, VReg::Virtual(_))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Zmm(0)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::K(2)))));

    let scalar_rsqrt28 = OpKind::X86Rsqrt28 {
        dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        merge: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))),
        src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
        elem: VecElementType::F64,
        width: VecWidth::V128,
        lanes: 1,
        scalar: true,
        mask_zeroing: false,
        suppress_exceptions: false,
    };
    assert!(scalar_rsqrt28.has_side_effects());
    let sources = scalar_rsqrt28.source_vregs();
    for source in [
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
        VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
        VReg::Arch(ArchReg::X86(X86Reg::K(1))),
    ] {
        assert!(sources.contains(&source));
    }
    let mut scalar_sae = scalar_rsqrt28;
    let OpKind::X86Rsqrt28 {
        mask,
        mask_zeroing,
        suppress_exceptions,
        ..
    } = &mut scalar_sae
    else {
        unreachable!()
    };
    *mask = None;
    *mask_zeroing = false;
    *suppress_exceptions = true;
    assert!(!scalar_sae.has_side_effects());

    let scale_f = optimized(&[0x62, 0xF2, 0x7D, 0x5A, 0x2C, 0x00]);
    let ops = &scale_f.blocks[0].ops;
    let scale = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86ScaleF {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    src2: VReg::Virtual(_),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    elem: VecElementType::F32,
                    width: VecWidth::V512,
                    lanes: 16,
                    scalar: false,
                    mask_zeroing: false,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                }
            )
        })
        .expect("masked broadcast VSCALEFPS removed");
    assert!(ops[scale].kind.has_side_effects());
    assert_eq!(
        ops[..scale]
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        1,
    );
    let sources = ops[scale].kind.source_vregs();
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Zmm(0)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::K(2)))));
    assert_eq!(
        sources
            .iter()
            .filter(|source| **source == VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))))
            .count(),
        2,
        "src1 and masked-merge destination are both data dependencies"
    );
    assert!(
        !OpKind::X86ScaleF {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Zmm(3))),
            mask: None,
            elem: VecElementType::F32,
            width: VecWidth::V512,
            lanes: 16,
            scalar: false,
            mask_zeroing: false,
            round: FpRoundMode::RoundNearest,
            suppress_exceptions: true,
        }
        .has_side_effects()
    );

    let packed_int_to_fp = optimized(&[0x62, 0xF1, 0x7F, 0x5A, 0x7A, 0x00]);
    let ops = &packed_int_to_fp.blocks[0].ops;
    let conversion = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86PackedIntToFp {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    int_elem: VecElementType::I32,
                    fp_elem: VecElementType::F32,
                    signed: false,
                    lanes: 16,
                    src_width: VecWidth::V512,
                    dst_width: VecWidth::V512,
                    mask_zeroing: false,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    ..
                }
            )
        })
        .expect("masked broadcast VCVTUDQ2PS removed");
    assert!(ops[conversion].kind.has_side_effects());
    assert_eq!(
        ops[..conversion]
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16,
    );
    let sources = ops[conversion].kind.source_vregs();
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Zmm(0)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::K(2)))));
    assert!(
        !OpKind::X86PackedIntToFp {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            mask: None,
            int_elem: VecElementType::I64,
            fp_elem: VecElementType::F32,
            signed: true,
            lanes: 8,
            src_width: VecWidth::V512,
            dst_width: VecWidth::V256,
            mask_zeroing: false,
            zero_upper: true,
            round: FpRoundMode::RoundNearest,
            suppress_exceptions: true,
        }
        .has_side_effects()
    );
    assert!(
        OpKind::X86PackedIntToFp {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            mask: None,
            int_elem: VecElementType::I64,
            fp_elem: VecElementType::F32,
            signed: true,
            lanes: 8,
            src_width: VecWidth::V512,
            dst_width: VecWidth::V256,
            mask_zeroing: false,
            zero_upper: true,
            round: FpRoundMode::RoundNearestTiesAway,
            suppress_exceptions: true,
        }
        .has_side_effects()
    );

    let packed_fp_to_int = optimized(&[0x62, 0xF1, 0xFD, 0x5A, 0x79, 0x00]);
    let ops = &packed_fp_to_int.blocks[0].ops;
    let conversion = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86PackedFpToInt {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    fp_elem: VecElementType::F64,
                    int_elem: VecElementType::I64,
                    signed: false,
                    truncate: false,
                    lanes: 8,
                    src_width: VecWidth::V512,
                    dst_width: VecWidth::V512,
                    mask_zeroing: false,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    ..
                }
            )
        })
        .expect("masked broadcast VCVTPD2UQQ removed");
    assert!(ops[conversion].kind.has_side_effects());
    assert_eq!(
        ops[..conversion]
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B8,
                    ..
                }
            ))
            .count(),
        8,
    );
    let sources = ops[conversion].kind.source_vregs();
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Zmm(0)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::K(2)))));
    assert!(
        !OpKind::X86PackedFpToInt {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            mask: None,
            fp_elem: VecElementType::F64,
            int_elem: VecElementType::I64,
            signed: false,
            truncate: true,
            lanes: 8,
            src_width: VecWidth::V512,
            dst_width: VecWidth::V512,
            mask_zeroing: false,
            zero_upper: true,
            round: FpRoundMode::RoundTowardZero,
            suppress_exceptions: true,
        }
        .has_side_effects()
    );

    let packed_int_to_fp16 = optimized(&[0x62, 0xF5, 0x7C, 0x0A, 0x5B, 0x00]);
    let ops = &packed_int_to_fp16.blocks[0].ops;
    let conversion = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86PackedIntToFp16 {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    int_elem: VecElementType::I32,
                    signed: true,
                    lanes: 4,
                    src_width: VecWidth::V128,
                    dst_width: VecWidth::V64,
                    mask_zeroing: false,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    ..
                }
            )
        })
        .expect("masked VCVTDQ2PH conversion removed");
    assert!(ops[conversion].kind.has_side_effects());
    assert_eq!(
        ops[..conversion]
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        4,
    );
    let sources = ops[conversion].kind.source_vregs();
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::K(2)))));
    assert!(
        !OpKind::X86PackedIntToFp16 {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            mask: None,
            int_elem: VecElementType::I32,
            signed: true,
            lanes: 16,
            src_width: VecWidth::V512,
            dst_width: VecWidth::V256,
            mask_zeroing: false,
            zero_upper: true,
            round: FpRoundMode::RoundNearest,
            suppress_exceptions: true,
        }
        .has_side_effects()
    );
    assert!(
        OpKind::X86PackedIntToFp16 {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Zmm(2))),
            mask: None,
            int_elem: VecElementType::I32,
            signed: true,
            lanes: 16,
            src_width: VecWidth::V512,
            dst_width: VecWidth::V256,
            mask_zeroing: false,
            zero_upper: true,
            round: FpRoundMode::RoundNearestTiesAway,
            suppress_exceptions: true,
        }
        .has_side_effects()
    );

    let packed_fp16_to_int = optimized(&[0x62, 0xF5, 0x7D, 0x0A, 0x7B, 0x00]);
    let ops = &packed_fp16_to_int.blocks[0].ops;
    let conversion = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86PackedFp16ToInt {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(2)))),
                    int_elem: VecElementType::I64,
                    signed: true,
                    truncate: false,
                    lanes: 2,
                    src_width: VecWidth::V64,
                    dst_width: VecWidth::V128,
                    mask_zeroing: false,
                    round: FpRoundMode::Dynamic,
                    suppress_exceptions: false,
                    ..
                }
            )
        })
        .expect("masked VCVTPH2QQ conversion removed");
    assert!(ops[conversion].kind.has_side_effects());
    assert_eq!(
        ops[..conversion]
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B2,
                    ..
                }
            ))
            .count(),
        2,
    );
    let sources = ops[conversion].kind.source_vregs();
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::K(2)))));
    assert!(
        !OpKind::X86PackedFp16ToInt {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(1))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
            mask: None,
            int_elem: VecElementType::I32,
            signed: false,
            truncate: true,
            lanes: 16,
            src_width: VecWidth::V256,
            dst_width: VecWidth::V512,
            mask_zeroing: false,
            zero_upper: true,
            round: FpRoundMode::RoundTowardZero,
            suppress_exceptions: true,
        }
        .has_side_effects()
    );

    let vcvtps2ph_store = optimized(&[0x62, 0xF3, 0x7D, 0x09, 0x1D, 0x10, 0x04]);
    let store = vcvtps2ph_store.blocks[0]
        .ops
        .iter()
        .find(|op| {
            matches!(
                op.kind,
                OpKind::X86PackedFpConvertStore {
                    addr: Address::Direct(VReg::Arch(ArchReg::X86(X86Reg::Rax))),
                    src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    mask: Some(VReg::Arch(ArchReg::X86(X86Reg::K(1)))),
                    lanes: 4,
                    round: FpRoundMode::Dynamic,
                }
            )
        })
        .expect("masked VCVTPS2PH memory conversion removed");
    assert!(store.kind.has_side_effects());
    assert!(store.kind.writes_memory());
    let sources = store.kind.source_vregs();
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Rax))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::Xmm(2)))));
    assert!(sources.contains(&VReg::Arch(ArchReg::X86(X86Reg::K(1)))));

    let legacy_packed_convert = optimized(&[0x0F, 0x5A, 0xC1]);
    let conversion = legacy_packed_convert.blocks[0]
        .ops
        .iter()
        .find(|op| matches!(op.kind, OpKind::X86PackedFpConvert { .. }))
        .expect("legacy CVTPS2PD conversion removed");
    assert!(
        conversion
            .kind
            .source_vregs()
            .contains(&VReg::Arch(ArchReg::X86(X86Reg::Xmm(0)))),
        "legacy packed conversion must preserve vector state above XMM"
    );

    for (name, bytes, load) in [
        ("LDMXCSR", &[0x0F, 0xAE, 0x10][..], true),
        ("VSTMXCSR", &[0xC5, 0xF8, 0xAE, 0x18][..], false),
    ] {
        let function = optimized(bytes);
        assert!(
            function.blocks[0].ops.iter().any(|op| {
                (load && matches!(op.kind, OpKind::X86LoadMxcsr { .. }))
                    || (!load && matches!(op.kind, OpKind::X86StoreMxcsr { .. }))
            }),
            "{name}: architectural MXCSR operation removed"
        );
    }

    let cldemote = optimized(&[0x0F, 0x1C, 0x00]);
    let cldemote = cldemote.blocks[0]
        .ops
        .iter()
        .find(|op| {
            matches!(
                op.kind,
                OpKind::X86CacheControl {
                    kind: X86CacheControlKind::Cldemote,
                    ..
                }
            )
        })
        .expect("CLDEMOTE hint removed by DCE");
    assert!(!cldemote.kind.reads_memory());
    assert!(cldemote.kind.has_side_effects());

    for (name, bytes, expected) in [
        ("FNINIT", &[0xDB, 0xE3][..], X86X87ControlKind::Init),
        (
            "FNCLEX",
            &[0xDB, 0xE2][..],
            X86X87ControlKind::ClearExceptions,
        ),
        (
            "FLDCW",
            &[0xD9, 0x28][..],
            X86X87ControlKind::LoadControlWord,
        ),
        (
            "FNSTCW",
            &[0xD9, 0x38][..],
            X86X87ControlKind::StoreControlWord,
        ),
        (
            "FNSTSW",
            &[0xDD, 0x38][..],
            X86X87ControlKind::StoreStatusWord,
        ),
        (
            "FLDENV m28byte",
            &[0xD9, 0x20][..],
            X86X87ControlKind::LoadEnvironment(crate::smir::ir::ops::X86X87EnvWidth::W32),
        ),
        (
            "FNSTENV m14byte",
            &[0x66, 0xD9, 0x30][..],
            X86X87ControlKind::StoreEnvironment(crate::smir::ir::ops::X86X87EnvWidth::W16),
        ),
        (
            "FRSTOR m108byte",
            &[0xDD, 0x20][..],
            X86X87ControlKind::RestoreState(crate::smir::ir::ops::X86X87EnvWidth::W32),
        ),
        (
            "FNSAVE m94byte",
            &[0x66, 0xDD, 0x30][..],
            X86X87ControlKind::SaveState(crate::smir::ir::ops::X86X87EnvWidth::W16),
        ),
    ] {
        let function = optimized(bytes);
        assert!(
            function.blocks[0].ops.iter().any(|op| matches!(
                op.kind,
                OpKind::X86X87Control { kind, .. } if kind == expected
            )),
            "{name}: x87 environment operation removed"
        );
    }

    for (name, bytes, expected) in [
        ("FLD m32fp", &[0xD9, 0x00][..], X86X87DataKind::LoadSingle),
        ("FLD m64fp", &[0xDD, 0x00][..], X86X87DataKind::LoadDouble),
        ("FILD m64int", &[0xDF, 0x28][..], X86X87DataKind::LoadInt64),
        ("FBLD m80bcd", &[0xDF, 0x20][..], X86X87DataKind::LoadBcd),
        (
            "FISTP m64int",
            &[0xDF, 0x38][..],
            X86X87DataKind::StoreInteger {
                width: crate::smir::ir::ops::X86X87IntWidth::I64,
                pop: true,
                truncate: false,
            },
        ),
        (
            "FSTP m64fp",
            &[0xDD, 0x18][..],
            X86X87DataKind::StoreFloat {
                width: crate::smir::ir::ops::X86X87FloatWidth::F64,
                pop: true,
            },
        ),
        ("FBSTP m80bcd", &[0xDF, 0x30][..], X86X87DataKind::StoreBcd),
        ("FLD m80fp", &[0xDB, 0x28][..], X86X87DataKind::LoadExtended),
        (
            "FSTP m80fp",
            &[0xDB, 0x38][..],
            X86X87DataKind::StorePopExtended,
        ),
        ("FLD ST(3)", &[0xD9, 0xC3][..], X86X87DataKind::LoadRegister),
        ("FXCH ST(1)", &[0xD9, 0xC9][..], X86X87DataKind::Exchange),
        ("FFREE ST(2)", &[0xDD, 0xC2][..], X86X87DataKind::Free),
        ("FCHS", &[0xD9, 0xE0][..], X86X87DataKind::ChangeSign),
        ("FINCSTP", &[0xD9, 0xF7][..], X86X87DataKind::IncrementTop),
        (
            "FLDPI",
            &[0xD9, 0xEB][..],
            X86X87DataKind::LoadConstant(crate::smir::ir::ops::X86X87Constant::Pi),
        ),
        (
            "FCMOVE ST(2)",
            &[0xDA, 0xCA][..],
            X86X87DataKind::ConditionalMove(Condition::Eq),
        ),
        ("FXAM", &[0xD9, 0xE5][..], X86X87DataKind::Examine),
        ("FTST", &[0xD9, 0xE4][..], X86X87DataKind::TestZero),
        ("FRNDINT", &[0xD9, 0xFC][..], X86X87DataKind::RoundInteger),
        ("FXTRACT", &[0xD9, 0xF4][..], X86X87DataKind::Extract),
        (
            "FPREM1",
            &[0xD9, 0xF5][..],
            X86X87DataKind::Remainder { nearest: true },
        ),
        (
            "FPREM",
            &[0xD9, 0xF8][..],
            X86X87DataKind::Remainder { nearest: false },
        ),
        ("FSCALE", &[0xD9, 0xFD][..], X86X87DataKind::Scale),
        ("FSQRT", &[0xD9, 0xFA][..], X86X87DataKind::SquareRoot),
        (
            "FADD m64fp",
            &[0xDC, 0x00][..],
            X86X87DataKind::AddSubtract {
                source: crate::smir::ir::ops::X86X87ArithmeticSource::Double,
                destination: crate::smir::ir::ops::X86X87ArithmeticDestination::St0,
                pop: false,
                subtract: false,
                reverse: false,
            },
        ),
        (
            "FSUB ST(3),ST(0)",
            &[0xDC, 0xEB][..],
            X86X87DataKind::AddSubtract {
                source: crate::smir::ir::ops::X86X87ArithmeticSource::Register,
                destination: crate::smir::ir::ops::X86X87ArithmeticDestination::StI,
                pop: false,
                subtract: true,
                reverse: false,
            },
        ),
        (
            "FSUBRP ST(1),ST(0)",
            &[0xDE, 0xE1][..],
            X86X87DataKind::AddSubtract {
                source: crate::smir::ir::ops::X86X87ArithmeticSource::Register,
                destination: crate::smir::ir::ops::X86X87ArithmeticDestination::StI,
                pop: true,
                subtract: true,
                reverse: true,
            },
        ),
        (
            "FISUBR m32int",
            &[0xDA, 0x28][..],
            X86X87DataKind::AddSubtract {
                source: crate::smir::ir::ops::X86X87ArithmeticSource::Int32,
                destination: crate::smir::ir::ops::X86X87ArithmeticDestination::St0,
                pop: false,
                subtract: true,
                reverse: true,
            },
        ),
        (
            "FDIV m64fp",
            &[0xDC, 0x30][..],
            X86X87DataKind::Divide {
                source: crate::smir::ir::ops::X86X87ArithmeticSource::Double,
                destination: crate::smir::ir::ops::X86X87ArithmeticDestination::St0,
                pop: false,
                reverse: false,
            },
        ),
        (
            "FDIVP ST(1),ST(0)",
            &[0xDE, 0xF9][..],
            X86X87DataKind::Divide {
                source: crate::smir::ir::ops::X86X87ArithmeticSource::Register,
                destination: crate::smir::ir::ops::X86X87ArithmeticDestination::StI,
                pop: true,
                reverse: false,
            },
        ),
        (
            "FIDIVR m32int",
            &[0xDA, 0x38][..],
            X86X87DataKind::Divide {
                source: crate::smir::ir::ops::X86X87ArithmeticSource::Int32,
                destination: crate::smir::ir::ops::X86X87ArithmeticDestination::St0,
                pop: false,
                reverse: true,
            },
        ),
        (
            "FMUL m64fp",
            &[0xDC, 0x08][..],
            X86X87DataKind::Multiply {
                source: crate::smir::ir::ops::X86X87ArithmeticSource::Double,
                destination: crate::smir::ir::ops::X86X87ArithmeticDestination::St0,
                pop: false,
            },
        ),
        (
            "FMULP ST(1),ST(0)",
            &[0xDE, 0xC9][..],
            X86X87DataKind::Multiply {
                source: crate::smir::ir::ops::X86X87ArithmeticSource::Register,
                destination: crate::smir::ir::ops::X86X87ArithmeticDestination::StI,
                pop: true,
            },
        ),
        (
            "FIMUL m32int",
            &[0xDA, 0x08][..],
            X86X87DataKind::Multiply {
                source: crate::smir::ir::ops::X86X87ArithmeticSource::Int32,
                destination: crate::smir::ir::ops::X86X87ArithmeticDestination::St0,
                pop: false,
            },
        ),
        (
            "FCOM m32fp",
            &[0xD8, 0x10][..],
            X86X87DataKind::Compare {
                source: crate::smir::ir::ops::X86X87CompareSource::Single,
                unordered: false,
                pop: 0,
                eflags: false,
            },
        ),
        (
            "FUCOMIP ST(1)",
            &[0xDF, 0xE9][..],
            X86X87DataKind::Compare {
                source: crate::smir::ir::ops::X86X87CompareSource::Register,
                unordered: true,
                pop: 1,
                eflags: true,
            },
        ),
        (
            "FICOMP m32int",
            &[0xDA, 0x18][..],
            X86X87DataKind::Compare {
                source: crate::smir::ir::ops::X86X87CompareSource::Int32,
                unordered: false,
                pop: 1,
                eflags: false,
            },
        ),
    ] {
        let function = optimized(bytes);
        assert!(
            function.blocks[0].ops.iter().any(|op| matches!(
                op.kind,
                OpKind::X86X87Data { kind, .. } if kind == expected
            )),
            "{name}: x87 data operation removed"
        );
    }

    let fcomi = optimized(&[0xDB, 0xF1]);
    let fcomi = fcomi.blocks[0]
        .ops
        .iter()
        .find(|op| {
            matches!(
                op.kind,
                OpKind::X86X87Data {
                    kind: X86X87DataKind::Compare { eflags: true, .. },
                    ..
                }
            )
        })
        .expect("FCOMI removed");
    assert_eq!(fcomi.kind.flags_written(), FlagSet::ALL_X86);
    assert_eq!(
        fcomi.kind.flags_must_write(),
        FlagSet::OF.union(FlagSet::SF).union(FlagSet::AF)
    );
    let fcmovbe = optimized(&[0xDA, 0xD1]);
    let fcmovbe = fcmovbe.blocks[0]
        .ops
        .iter()
        .find(|op| {
            matches!(
                op.kind,
                OpKind::X86X87Data {
                    kind: X86X87DataKind::ConditionalMove(Condition::Ule),
                    ..
                }
            )
        })
        .expect("FCMOVBE removed");
    assert_eq!(fcmovbe.kind.flags_read(), FlagSet::CF.union(FlagSet::ZF));

    for (name, bytes, save) in [
        ("FXSAVE64", &[0x48, 0x0F, 0xAE, 0x00][..], true),
        ("FXRSTOR64", &[0x48, 0x0F, 0xAE, 0x08][..], false),
    ] {
        let function = optimized(bytes);
        assert!(
            function.blocks[0].ops.iter().any(|op| {
                (save && matches!(op.kind, OpKind::X86FxSave { .. }))
                    || (!save && matches!(op.kind, OpKind::X86FxRstor { .. }))
            }),
            "{name}: state operation removed"
        );
    }

    for (name, bytes, get) in [
        ("XGETBV", &[0x0F, 0x01, 0xD0][..], true),
        ("XSETBV", &[0x0F, 0x01, 0xD1][..], false),
    ] {
        let function = optimized(bytes);
        assert!(
            function.blocks[0].ops.iter().any(|op| {
                (get && matches!(op.kind, OpKind::X86XGetBv { .. }))
                    || (!get && matches!(op.kind, OpKind::X86XSetBv { .. }))
            }),
            "{name}: XCR operation removed"
        );
    }

    for (name, bytes, save) in [
        ("XSAVE64", &[0x48, 0x0F, 0xAE, 0x23][..], true),
        ("XSAVEOPT64", &[0x48, 0x0F, 0xAE, 0x33][..], true),
        ("XRSTOR64", &[0x48, 0x0F, 0xAE, 0x2B][..], false),
        ("XSAVEC64", &[0x48, 0x0F, 0xC7, 0x23][..], true),
        ("XSAVES64", &[0x48, 0x0F, 0xC7, 0x2B][..], true),
        ("XRSTORS64", &[0x48, 0x0F, 0xC7, 0x1B][..], false),
    ] {
        let function = optimized(bytes);
        assert!(
            function.blocks[0].ops.iter().any(|op| {
                (save && matches!(op.kind, OpKind::X86XSave { .. }))
                    || (!save && matches!(op.kind, OpKind::X86XRstor { .. }))
            }),
            "{name}: extended-state operation removed"
        );
    }

    for (name, bytes, predicate) in [
        ("CMPXCHG16B", &[0xF0, 0x48, 0x0F, 0xC7, 0x0E][..], 0u8),
        ("RDRAND", &[0x48, 0x0F, 0xC7, 0xF0][..], 1),
        ("RDSEED", &[0x48, 0x0F, 0xC7, 0xF8][..], 2),
    ] {
        let function = optimized(bytes);
        let op = function.blocks[0]
            .ops
            .iter()
            .find(|op| match predicate {
                0 => matches!(op.kind, OpKind::X86Cmpxchg8b16b { .. }),
                1 => matches!(op.kind, OpKind::X86Random { seed: false, .. }),
                _ => matches!(op.kind, OpKind::X86Random { seed: true, .. }),
            })
            .unwrap_or_else(|| panic!("{name}: Group-9 operation removed"));
        assert_eq!(
            op.kind.flags_written(),
            if predicate == 0 {
                FlagSet::ZF
            } else {
                FlagSet::ALL_X86
            }
        );
        assert_eq!(op.kind.flags_must_write(), op.kind.flags_written());
    }

    {
        let name = "ADDPS";
        let function = optimized(&[0x0F, 0x58, 0x00]);
        let ops = &function.blocks[0].ops;
        let load = ops
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
            .unwrap_or_else(|| panic!("{name}: faulting VLoad removed"));
        let first_destination_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op,
                    SmirOp {
                        kind: OpKind::VAdd {
                            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                            ..
                        },
                        x86_hint: Some(X86OpHint::SseOp { .. }),
                        ..
                    }
                )
            })
            .unwrap_or_else(|| panic!("{name}: hinted destination write removed"));
        assert!(
            load < first_destination_write,
            "{name}: write before fault boundary"
        );
    }

    {
        let name = "PADDSB";
        let function = optimized(&[0x66, 0x0F, 0xEC, 0x00]);
        let ops = &function.blocks[0].ops;
        let load = ops
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
            .unwrap_or_else(|| panic!("{name}: faulting VLoad removed"));
        let saturated_write = ops
            .iter()
            .position(|op| {
                matches!(
                    op,
                    SmirOp {
                        kind: OpKind::VAddSubSat {
                            dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                            elem: VecElementType::I8,
                            subtract: false,
                            signed: true,
                            ..
                        },
                        x86_hint: Some(X86OpHint::SseOp { .. }),
                        ..
                    }
                )
            })
            .unwrap_or_else(|| panic!("{name}: saturated destination write removed"));
        assert!(
            load < saturated_write,
            "{name}: write before fault boundary"
        );
    }

    for (name, bytes, vector_load) in [
        ("VBCSTNEBF162PS", &[0xC4, 0x62, 0x7A, 0xB1, 0x08][..], false),
        ("VCVTNEEBF162PS", &[0xC4, 0x62, 0x7E, 0xB0, 0x08][..], true),
    ] {
        let function = optimized(bytes);
        let ops = &function.blocks[0].ops;
        let load = ops
            .iter()
            .position(|op| {
                if vector_load {
                    matches!(
                        op.kind,
                        OpKind::VLoad {
                            width: VecWidth::V256,
                            ..
                        }
                    )
                } else {
                    matches!(
                        op.kind,
                        OpKind::Load {
                            width: MemWidth::B2,
                            ..
                        }
                    )
                }
            })
            .unwrap_or_else(|| panic!("{name}: faulting source load removed"));
        let conversion = ops
            .iter()
            .position(|op| matches!(op.kind, OpKind::X86Convert16ToFp32 { .. }))
            .unwrap_or_else(|| panic!("{name}: conversion removed"));
        assert!(load < conversion, "{name}: write before fault boundary");
    }

    let packed_shift = optimized(&[0xC4, 0xC1, 0x35, 0x73, 0xDA, 0x01]);
    assert!(packed_shift.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86PackedShiftImm {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Ymm(10))),
            width: VecWidth::V256,
            shift: ShiftOp::Lsr,
            amount: 1,
            byte_lane: true,
            ..
        }
    )));

    let legacy_shift = optimized(&[0x66, 0x0F, 0x73, 0xF8, 0x01]);
    assert!(legacy_shift.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86PackedShiftImm {
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
            width: VecWidth::V128,
            shift: ShiftOp::Lsl,
            amount: 1,
            byte_lane: true,
            ..
        }
    )));

    let e4nf_shift = optimized(&[0x62, 0xF1, 0x7D, 0x49, 0x71, 0x10, 0x03]);
    let ops = &e4nf_shift.blocks[0].ops;
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
    );
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V512,
                    ..
                }
            )
        })
        .expect("E4NF immediate word-shift load must survive optimization");
    let shift = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::X86PackedShiftImm { .. }))
        .expect("EVEX immediate word shift must survive optimization");
    let write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    elem: VecElementType::I16,
                    ..
                }
            )
        })
        .expect("EVEX immediate word-shift destination write must survive optimization");
    assert!(load < shift && shift < write);

    let e4_shift = optimized(&[0x62, 0xF1, 0x7D, 0x49, 0x72, 0x10, 0x03]);
    assert_eq!(
        e4_shift.blocks[0]
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
        16,
    );

    let packed_shift_count = optimized(&[0xC4, 0x41, 0x35, 0xD2, 0xC2]);
    assert!(packed_shift_count.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86PackedShift {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(8))),
            src: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
            width: VecWidth::V256,
            elem: VecElementType::I32,
            shift: ShiftOp::Lsr,
            ..
        }
    )));

    let packed_shift_variable = optimized(&[0x62, 0xF2, 0xED, 0x08, 0x10, 0xCB]);
    assert!(
        packed_shift_variable.blocks[0]
            .ops
            .iter()
            .any(|op| matches!(
                op.kind,
                OpKind::X86PackedShiftVariable {
                    src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
                    count: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
                    elem: VecElementType::I16,
                    shift: ShiftOp::Lsr,
                    ..
                }
            ))
    );

    let packed_rotate = optimized(&[0x62, 0xF1, 0x75, 0x08, 0x72, 0xCA, 0x07]);
    assert!(packed_rotate.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86PackedRotate {
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            count: None,
            amount: 7,
            width: VecWidth::V128,
            elem: VecElementType::I32,
            left: true,
            ..
        }
    )));

    let masked_rotate = optimized(&[0x62, 0xF2, 0x4D, 0x5A, 0x14, 0x68, 0x01]);
    let rotate_ops = &masked_rotate.blocks[0].ops;
    assert_eq!(
        rotate_ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16,
    );
    assert!(rotate_ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86PackedRotate {
            count: Some(_),
            width: VecWidth::V512,
            elem: VecElementType::I32,
            left: false,
            ..
        }
    )));

    let ternary = optimized(&[0x62, 0xF3, 0x6D, 0x08, 0x25, 0xCB, 0x96]);
    assert!(ternary.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86TernaryLogic {
            src1: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            src2: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            src3: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
            imm: 0x96,
            width: VecWidth::V128,
            ..
        }
    )));

    let masked_ternary = optimized(&[0x62, 0xC3, 0x6D, 0x57, 0x25, 0x4D, 0x7F, 0xE4]);
    let ternary_ops = &masked_ternary.blocks[0].ops;
    assert_eq!(
        ternary_ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16,
    );
    assert!(
        ternary_ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86TernaryLogic { imm: 0xE4, .. }))
    );

    let funnel = optimized(&[0x62, 0xF3, 0xED, 0x08, 0x70, 0xCB, 0x07]);
    assert!(funnel.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86PackedFunnelShift {
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            fill: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
            count: None,
            amount: 7,
            elem: VecElementType::I16,
            left: true,
            ..
        }
    )));

    let variable_funnel = optimized(&[0x62, 0xF2, 0xED, 0x08, 0x73, 0xCB]);
    assert!(variable_funnel.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86PackedFunnelShift {
            src: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1))),
            fill: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            count: Some(VReg::Arch(ArchReg::X86(X86Reg::Xmm(3)))),
            elem: VecElementType::I64,
            left: false,
            ..
        }
    )));

    let multishift = optimized(&[0x62, 0xF2, 0xED, 0x08, 0x83, 0xCB]);
    assert!(multishift.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::X86MultiShiftQB {
            control: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            source: VReg::Arch(ArchReg::X86(X86Reg::Xmm(3))),
            width: VecWidth::V128,
            ..
        }
    )));

    let e4nf_multishift = optimized(&[0x62, 0x62, 0x8D, 0xC1, 0x83, 0x78, 0x01]);
    assert!(e4nf_multishift.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            width: VecWidth::V512,
            ..
        }
    )));
    assert!(
        !e4nf_multishift.blocks[0]
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
    );

    let vector_align = optimized(&[0x62, 0xF3, 0x6D, 0x08, 0x03, 0xCB, 0x01]);
    assert_eq!(
        vector_align.blocks[0]
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::VExtractLane {
                    elem: VecElementType::I32,
                    ..
                }
            ))
            .count(),
        4,
    );
    let e4nf_align = optimized(&[0x62, 0xC3, 0x6D, 0x47, 0x03, 0x4D, 0x01, 0x1F]);
    assert!(e4nf_align.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VLoad {
            width: VecWidth::V512,
            ..
        }
    )));
    assert!(
        !e4nf_align.blocks[0]
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
    );

    for bytes in [
        &[0x66, 0x45, 0x0F, 0xF7, 0xC1][..],
        &[0xC4, 0x41, 0x79, 0xF7, 0xC1][..],
    ] {
        let maskmov = optimized(bytes);
        let ops = &maskmov.blocks[0].ops;
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(
                    op.kind,
                    OpKind::PredStore {
                        width: MemWidth::B1,
                        ..
                    }
                ))
                .count(),
            16,
            "MASKMOVDQU byte stores removed for {bytes:02X?}",
        );
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(8))),
                lane: 15,
                elem: VecElementType::I8,
                ..
            }
        )));
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(9))),
                lane: 15,
                elem: VecElementType::I8,
                ..
            }
        )));
    }

    let addr32_maskmov = optimized(&[0x67, 0xC4, 0x41, 0x79, 0xF7, 0xC1]);
    let ops = &addr32_maskmov.blocks[0].ops;
    let truncated = ops
        .iter()
        .find_map(|op| match op.kind {
            OpKind::And {
                dst,
                src1: VReg::Arch(ArchReg::X86(X86Reg::Rdi)),
                src2: SrcOperand::Imm(0xFFFF_FFFF),
                width: OpWidth::W64,
                ..
            } => Some(dst),
            _ => None,
        })
        .expect("optimizer removed MASKMOVDQU EDI truncation");
    assert!(ops.iter().any(|op| matches!(
        op.kind,
        OpKind::Add {
            src1,
            src2: SrcOperand::Imm(15),
            width: OpWidth::W32,
            flags: FlagUpdate::None,
            ..
        } if src1 == truncated
    )));

    for (bytes, loads, stores) in [
        (&[0xC4, 0xE2, 0x75, 0x2C, 0x17][..], 8usize, 0usize),
        (&[0xC4, 0xE2, 0xF1, 0x8E, 0x17][..], 0, 2),
    ] {
        let masked_memory = optimized(bytes);
        let ops = &masked_memory.blocks[0].ops;
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(op.kind, OpKind::PredLoad { .. }))
                .count(),
            loads,
        );
        assert_eq!(
            ops.iter()
                .filter(|op| matches!(op.kind, OpKind::PredStore { .. }))
                .count(),
            stores,
        );
        assert!(ops.iter().any(|op| matches!(
            op.kind,
            OpKind::VExtractLane {
                vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(1) | X86Reg::Ymm(1))),
                ..
            }
        )));
    }

    let vex_gather = optimized(&[0xC4, 0xE2, 0x75, 0x90, 0x1C, 0x90]);
    let ops = &vex_gather.blocks[0].ops;
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        8,
    );
    let first_load = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .expect("VPGATHERDD loads removed");
    let first_commit = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(3))),
                    ..
                }
            )
        })
        .expect("VPGATHERDD destination commits removed");
    assert!(first_load < first_commit);
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(1))),
                    ..
                }
            ))
            .count(),
        8,
        "restart mask updates must survive optimization",
    );

    let evex_gather = optimized(&[0x62, 0xE2, 0x7D, 0x43, 0x92, 0x14, 0x88]);
    let ops = &evex_gather.blocks[0].ops;
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16,
    );
    assert!(ops.iter().any(|op| matches!(
        op.kind,
        OpKind::And {
            dst: VReg::Arch(ArchReg::X86(X86Reg::K(3))),
            flags: FlagUpdate::None,
            ..
        }
    )));
    assert!(ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Zmm(17))),
            sign: SignExtend::Sign,
            ..
        }
    )));

    let evex_scatter = optimized(&[0x62, 0xF2, 0x7D, 0x09, 0xA0, 0x0C, 0x90]);
    let ops = &evex_scatter.blocks[0].ops;
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredStore {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        4,
    );
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::And {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::K(1))),
                    flags: FlagUpdate::None,
                    ..
                }
            ))
            .count(),
        5,
        "scatter mask normalization and per-lane restart updates removed",
    );
    assert!(ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VExtractLane {
            vec: VReg::Arch(ArchReg::X86(X86Reg::Xmm(2))),
            sign: SignExtend::Sign,
            ..
        }
    )));

    let evex_aes = optimized(&[0x62, 0xE2, 0x5D, 0x20, 0xDE, 0x68, 0x02]);
    let ops = &evex_aes.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    addr: Address::BaseOffset { offset: 64, .. },
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("EVEX VAESDEC full-tuple load removed");
    let aes = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86Aes {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(21))),
                    src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(20))),
                    op: X86AesOp::Dec,
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("EVEX VAESDEC computation removed");
    assert!(load < aes, "VAESDEC moved before its memory fault boundary");

    let evex_fma = optimized(&[0x62, 0xF2, 0x65, 0xD9, 0xA6, 0x10]);
    let ops = &evex_fma.blocks[0].ops;
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::PredLoad {
                    width: MemWidth::B4,
                    ..
                }
            ))
            .count(),
        16,
    );
    let first_fma = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::VFma { .. }))
        .expect("EVEX FMA computation removed");
    let last_load = ops
        .iter()
        .rposition(|op| matches!(op.kind, OpKind::PredLoad { .. }))
        .expect("EVEX FMA masked broadcast loads removed");
    assert!(last_load < first_fma, "FMA moved before its fault boundary");
    assert_eq!(
        ops.iter()
            .filter(|op| matches!(op.kind, OpKind::Select { .. }))
            .count(),
        16,
    );

    let horizontal = optimized(&[0xC5, 0xFF, 0x7C, 0x50, 0x20]);
    let ops = &horizontal.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("VHADDPS source load removed");
    let arithmetic = ops
        .iter()
        .position(|op| matches!(op.kind, OpKind::FAdd { .. }))
        .expect("VHADDPS arithmetic removed");
    assert!(load < arithmetic);
    assert!(ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VMov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
            width: VecWidth::V256,
            ..
        }
    )));

    let legacy_horizontal = optimized(&[0x66, 0x0F, 0x7D, 0x00]);
    assert!(
        legacy_horizontal.blocks[0]
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
    );

    let reciprocal = optimized(&[0xC5, 0xFC, 0x53, 0x50, 0x20]);
    let ops = &reciprocal.blocks[0].ops;
    let load = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V256,
                    ..
                }
            )
        })
        .expect("VRCPPS source load removed");
    let estimate = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VUnary {
                    op: VecUnaryOp::FRecipEstimate,
                    lanes: 8,
                    ..
                }
            )
        })
        .expect("VRCPPS estimate removed");
    assert!(load < estimate, "VRCPPS moved before its fault boundary");
    assert!(ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VMov {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(2))),
            width: VecWidth::V256,
            ..
        }
    )));

    let legacy_reciprocal = optimized(&[0x0F, 0x52, 0x00]);
    assert!(
        legacy_reciprocal.blocks[0]
            .ops
            .iter()
            .any(|op| matches!(op.kind, OpKind::X86CheckAlignment { alignment: 16, .. }))
    );

    let masked_shift_count = optimized(&[0x62, 0xF1, 0xF5, 0x49, 0xF3, 0x40, 0x04]);
    let ops = &masked_shift_count.blocks[0].ops;
    assert!(
        !ops.iter()
            .any(|op| matches!(op.kind, OpKind::PredLoad { .. }))
    );
    let load = ops
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
        .expect("E4NF packed shift Mem128 load must survive optimization");
    let shift = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::X86PackedShift {
                    width: VecWidth::V512,
                    elem: VecElementType::I64,
                    shift: ShiftOp::Lsl,
                    ..
                }
            )
        })
        .expect("packed shift-by-count computation must survive optimization");
    let write = ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VInsertLane {
                    dst: VReg::Arch(ArchReg::X86(X86Reg::Zmm(0))),
                    elem: VecElementType::I64,
                    ..
                }
            )
        })
        .expect("masked packed shift destination write must survive optimization");
    assert!(load < shift && shift < write);

    let packed_shuffle = optimized(&[0xC4, 0x41, 0x7D, 0x70, 0xCA, 0x1B]);
    assert!(packed_shuffle.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VShuffle {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(10))),
            elem: VecElementType::I32,
            lanes: 8,
            ..
        }
    )));

    let masked_shuffle = optimized(&[0x62, 0xE1, 0x7D, 0x4B, 0x70, 0x08, 0x1B]);
    let masked_ops = &masked_shuffle.blocks[0].ops;
    let load = masked_ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VLoad {
                    width: VecWidth::V512,
                    ..
                }
            )
        })
        .expect("masked VPSHUFD E4NF load removed");
    let shuffle = masked_ops
        .iter()
        .position(|op| {
            matches!(
                op.kind,
                OpKind::VShuffle {
                    elem: VecElementType::I32,
                    lanes: 16,
                    ..
                }
            )
        })
        .expect("masked VPSHUFD shuffle removed");
    assert!(
        load < shuffle,
        "masked VPSHUFD reordered before its E4NF load"
    );
    assert_eq!(
        masked_ops
            .iter()
            .filter(|op| matches!(op.kind, OpKind::Select { .. }))
            .count(),
        16
    );

    let two_source_shuffle = optimized(&[0xC4, 0x41, 0x2C, 0xC6, 0xCB, 0xE4]);
    assert!(two_source_shuffle.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VShuffle {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(10))),
            src2: Some(VReg::Arch(ArchReg::X86(X86Reg::Ymm(11)))),
            elem: VecElementType::F32,
            lanes: 8,
            ..
        }
    )));

    let duplicate_move = optimized(&[0xC4, 0x41, 0x7E, 0x12, 0xCA]);
    assert!(duplicate_move.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VShuffle {
            dst: VReg::Arch(ArchReg::X86(X86Reg::Ymm(9))),
            src1: VReg::Arch(ArchReg::X86(X86Reg::Ymm(10))),
            src2: None,
            elem: VecElementType::F32,
            lanes: 8,
            ..
        }
    )));

    let masked_sat = optimized(&[0x62, 0xF1, 0x7D, 0xC9, 0xEC, 0xD1]);
    assert!(masked_sat.blocks[0].ops.iter().any(|op| matches!(
        op.kind,
        OpKind::VAddSubSat {
            elem: VecElementType::I8,
            lanes: 64,
            subtract: false,
            signed: true,
            ..
        }
    )));
    assert_eq!(
        masked_sat.blocks[0]
            .ops
            .iter()
            .filter(|op| matches!(
                op.kind,
                OpKind::Select {
                    width: OpWidth::W8,
                    ..
                }
            ))
            .count(),
        64,
    );

    let movups = optimized(&[0x0F, 0x10, 0x00]);
    assert!(movups.blocks[0].ops.iter().any(|op| matches!(
        op,
        SmirOp {
            kind: OpKind::VLoad {
                dst: VReg::Arch(ArchReg::X86(X86Reg::Xmm(0))),
                width: VecWidth::V128,
                ..
            },
            x86_hint: Some(X86OpHint::SseMov { .. }),
            ..
        }
    )));
}
