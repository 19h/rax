//! Hypervisor virtual-memory instruction decoding.

use super::*;

pub(super) fn decode_hypervisor_mem(w: u32, rv64: bool) -> Insn {
    let op = match (funct7(w), rs2(w), rd(w)) {
        (0x30, 0, _) => Op::HlvB,
        (0x30, 1, _) => Op::HlvBu,
        (0x32, 0, _) => Op::HlvH,
        (0x32, 1, _) => Op::HlvHu,
        (0x32, 3, _) => Op::HlvxHu,
        (0x34, 0, _) => Op::HlvW,
        (0x34, 1, _) if rv64 => Op::HlvWu,
        (0x34, 3, _) => Op::HlvxWu,
        (0x36, 0, _) if rv64 => Op::HlvD,
        (0x31, _, 0) => Op::HsvB,
        (0x33, _, 0) => Op::HsvH,
        (0x35, _, 0) => Op::HsvW,
        (0x37, _, 0) if rv64 => Op::HsvD,
        _ => return Insn::illegal(w, 4),
    };
    base(op, w)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(funct7: u32, rs2: u32, rs1: u32, rd: u32) -> u32 {
        (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (0b100 << 12) | (rd << 7) | 0x73
    }

    #[test]
    fn hlvx_wu_is_legal_on_both_xlens_with_h() {
        let raw = enc(0x34, 3, 10, 5);
        let full = Isa::rv64gc();
        assert_eq!(decode(raw, Xlen::Rv32, &full).op, Op::HlvxWu);
        assert_eq!(decode(raw, Xlen::Rv64, &full).op, Op::HlvxWu);

        let mut no_h = full;
        no_h.h = false;
        assert!(decode(raw, Xlen::Rv32, &no_h).is_illegal());
    }
}
