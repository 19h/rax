//! part1 part 1 tests

use super::*;
use crate::smir::optimize::*;
use crate::smir::optimize::tests::*;

    #[test]
    fn bit_test_and_carry_control_metadata_tracks_cf_exactly() {
        let rax = VReg::Arch(ArchReg::X86(X86Reg::Rax));
        let rcx = VReg::Arch(ArchReg::X86(X86Reg::Rcx));
        let forms = [
            OpKind::Bt {
                src: rax,
                index: SrcOperand::Reg(rcx),
                width: OpWidth::W64,
            },
            OpKind::Bts {
                dst: rax,
                src: rax,
                index: SrcOperand::Imm(3),
                width: OpWidth::W64,
            },
            OpKind::Btr {
                dst: rax,
                src: rax,
                index: SrcOperand::Imm(4),
                width: OpWidth::W64,
            },
            OpKind::Btc {
                dst: rax,
                src: rax,
                index: SrcOperand::Imm(5),
                width: OpWidth::W64,
            },
        ];
        for form in forms {
            assert_eq!(form.flags_written(), FlagSet::CF, "{form:?}");
            assert_eq!(form.flags_must_write(), FlagSet::CF, "{form:?}");
            assert_eq!(form.flags_read(), FlagSet::EMPTY, "{form:?}");
        }

        let set = OpKind::SetCF { value: true };
        assert_eq!(set.flags_written(), FlagSet::CF);
        assert_eq!(set.flags_must_write(), FlagSet::CF);
        assert_eq!(set.flags_read(), FlagSet::EMPTY);

        let complement = OpKind::CmcCF;
        assert_eq!(complement.flags_written(), FlagSet::CF);
        assert_eq!(complement.flags_must_write(), FlagSet::CF);
        assert_eq!(complement.flags_read(), FlagSet::CF);
    }
