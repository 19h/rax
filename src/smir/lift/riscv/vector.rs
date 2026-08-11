//! vector.rs

use crate::isa::riscv::{
    Op as RvOp, Xlen as RvXlen, decode as rv_decode, rvc::decode_rvc as rv_decode_rvc,
};
use crate::smir::ir::flags::FlagUpdate;
use crate::smir::ir::ops::{OpKind, RvVectorState, SmirOp};
use crate::smir::ir::types::*;
use crate::smir::ir::{SmirBlock, SmirFunction};
use crate::smir::lift::riscv::*;

use crate::smir::lift::{
    ControlFlow, LiftContext, LiftError, LiftResult, MemoryReader, SmirLifter,
};

impl RiscVLifter {
    /// OP-V (0x57): RVV arithmetic and `vset{i}vl{i}` configuration. RVV element
    /// width and length are runtime `vtype`/`vl` state unknown at lift time, so
    /// the whole vector ISA is lifted to the opaque [`OpKind::RvVector`] engine.
    pub(crate) fn lift_vector(
        &mut self,
        insn: u32,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let xl = if self.xlen == 64 {
            RvXlen::Rv64
        } else {
            RvXlen::Rv32
        };
        let d = rv_decode(insn, xl, &self.decoder_isa());
        if d.is_illegal() {
            return Err(LiftError::InvalidEncoding {
                addr,
                bytes: insn.to_le_bytes().to_vec(),
            });
        }
        self.emit_rv_vector(insn, &d, addr, ctx)
    }

    /// Emit an opaque [`OpKind::RvVector`] for one RVV instruction. The vector
    /// engine is opaque to SMIR, so scalar x/f/CSR state is explicitly listed
    /// as both input and output architectural registers on the op.
    pub(crate) fn emit_rv_vector(
        &mut self,
        insn: u32,
        d: &crate::isa::riscv::Insn,
        addr: GuestAddr,
        ctx: &mut LiftContext,
    ) -> Result<(Vec<SmirOp>, ControlFlow), LiftError> {
        let x_srcs: [VReg; 32] = std::array::from_fn(|i| self.get_x_reg(i as u8, ctx));
        let f_srcs: [VReg; 32] =
            std::array::from_fn(|i| VReg::Arch(ArchReg::RiscV(RiscVReg::F(i as u8))));
        let fcsr_src = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0x003)));
        let vl_src = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0xc20)));
        let vtype_src = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0xc21)));
        let vstart_src = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0x008)));
        let vcsr_src = VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0x00f)));
        let rs1 = x_srcs[d.rs1 as usize];
        let rs2 = x_srcs[d.rs2 as usize];

        let x_dsts: [VReg; 32] = std::array::from_fn(|i| {
            if i == 0 {
                VReg::Imm(0)
            } else {
                VReg::Arch(ArchReg::RiscV(RiscVReg::X(i as u8)))
            }
        });
        let f_dsts: [VReg; 32] =
            std::array::from_fn(|i| VReg::Arch(ArchReg::RiscV(RiscVReg::F(i as u8))));
        let state = Box::new(RvVectorState {
            x_srcs,
            x_dsts,
            f_srcs,
            f_dsts,
            fcsr_src,
            fcsr_dst: VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0x003))),
            vl_src,
            vl_dst: VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0xc20))),
            vtype_src,
            vtype_dst: VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0xc21))),
            vstart_src,
            vstart_dst: VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0x008))),
            vcsr_src,
            vcsr_dst: VReg::Arch(ArchReg::RiscV(RiscVReg::Csr(0x00f))),
        });
        let ops = vec![SmirOp::new(
            ctx.next_op_id(),
            addr,
            OpKind::RvVector {
                insn,
                xlen: self.xlen,
                rs1,
                rs2,
                state,
            },
        )];
        Ok((ops, ControlFlow::NextInsn))
    }
}
