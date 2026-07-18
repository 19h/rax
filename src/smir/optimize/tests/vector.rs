//! tests::vector tests

use super::*;
use crate::smir::optimize::*;

    #[test]
    fn accumulating_vcmp_to_q_reports_dst_as_source() {
        let dst = VReg::virt(0);
        let src1 = VReg::virt(1);
        let src2 = VReg::virt(2);
        let make_vcmp = |accumulate| OpKind::VCmpToQ {
            dst,
            src1,
            src2,
            cond: VecCmpCond::Eq,
            elem: VecElementType::I8,
            lanes: 16,
            accumulate,
        };

        let overwrite_sources = make_vcmp(None).source_vregs();
        assert_eq!(overwrite_sources, vec![src1, src2]);

        let accumulate_sources = make_vcmp(Some(VLaneOp::Or)).source_vregs();
        assert_eq!(accumulate_sources, vec![src1, src2, dst]);
    }
