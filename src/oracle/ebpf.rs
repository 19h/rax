//! eBPF decode and dialect-aware call classification.

use serde_json::{Value, json};

use super::{
    OracleOptions, fallthrough_json, hex_u8, hex_u32, hex_u64, input_json, lift_smir,
    side_effects_not_run,
};
use crate::config::Endianness;
use crate::isa::ebpf::{
    EbpfDialect, EbpfOp, INSN_SIZE as EBPF_INSN_SIZE, decode_insn as ebpf_decode_insn,
};
use crate::smir::ir::types::{Endian, SourceArch};

pub(super) fn ebpf_endian(endian: Endianness) -> Endian {
    match endian {
        Endianness::Little => Endian::Little,
        Endianness::Big => Endian::Big,
    }
}

pub(super) fn decode_ebpf(bytes: &[u8], opts: &OracleOptions) -> Result<Value, String> {
    let endian = ebpf_endian(opts.ebpf_endian);
    let insn = ebpf_decode_insn(bytes, opts.ebpf_dialect, endian).map_err(|e| e.to_string())?;
    let consumed = insn.size as usize;
    let control_flow = ebpf_control_flow(&insn, opts.pc, opts.ebpf_dialect);
    let smir = if opts.include_smir {
        lift_smir(SourceArch::Ebpf, &bytes[..bytes.len().min(consumed)], opts)
    } else {
        json!({"available": false, "reason": "disabled"})
    };
    let dialect_name = match opts.ebpf_dialect {
        EbpfDialect::Rfc9669 => "rfc9669".to_string(),
        EbpfDialect::Solana(v) => format!("sbpf-{v:?}").to_lowercase(),
    };

    Ok(json!({
        "isa": "ebpf",
        "dialect": dialect_name,
        "endian": format!("{:?}", opts.ebpf_endian).to_lowercase(),
        "pc": hex_u64(opts.pc),
        "input": input_json(bytes),
        "decoded_ops": [{
            "offset": 0,
            "size": insn.size,
            "opc": hex_u8(insn.opc),
            "dst": insn.dst,
            "src": insn.src,
            "off": insn.off,
            "imm": insn.imm,
            "imm2": if insn.size == 16 { Some(insn.imm2) } else { None },
            "mnemonic": insn.mnemonic(),
            "disasm": insn.disassemble(),
            "op": format!("{:?}", insn.op),
            "debug": format!("{insn:?}"),
        }],
        "packet_flags": null,
        "control_flow": control_flow,
        "smir": smir,
        "side_effects": side_effects_not_run(),
    }))
}

fn ebpf_control_flow(
    insn: &crate::isa::ebpf::decode::EbpfInsn,
    pc: u64,
    dialect: EbpfDialect,
) -> Value {
    let next_pc = pc.wrapping_add(u64::from(insn.size));
    match insn.op {
        EbpfOp::Jump { offset_insns } => {
            let delta = i64::from(offset_insns).wrapping_mul(EBPF_INSN_SIZE as i64);
            let target = (next_pc as i64).wrapping_add(delta) as u64;
            json!({
                "kind": "branch",
                "target": hex_u64(target),
            })
        }
        EbpfOp::JumpCond {
            cond, offset_insns, ..
        } => {
            let delta = i64::from(offset_insns).wrapping_mul(EBPF_INSN_SIZE as i64);
            let target = (next_pc as i64).wrapping_add(delta) as u64;
            json!({
                "kind": "cond_branch",
                "condition": format!("{cond:?}"),
                "target": hex_u64(target),
                "fallthrough": hex_u64(next_pc),
            })
        }
        EbpfOp::CallImm { src_kind, imm } => {
            let static_syscalls = dialect.sbpf_version().is_none_or(|v| v.static_syscalls());
            if src_kind == 1 && static_syscalls {
                let delta = i64::from(imm).wrapping_mul(EBPF_INSN_SIZE as i64);
                let target = (next_pc as i64).wrapping_add(delta) as u64;
                json!({
                    "kind": "call",
                    "target": hex_u64(target),
                    "fallthrough": hex_u64(next_pc),
                })
            } else if !static_syscalls {
                json!({ "kind": "unresolved_call", "key": hex_u32(imm as u32), "fallthrough": hex_u64(next_pc) })
            } else {
                json!({
                    "kind": "syscall",
                    "key": hex_u32(imm as u32),
                    "fallthrough": hex_u64(next_pc),
                })
            }
        }
        EbpfOp::CallReg { target_reg } => json!({
            "kind": "indirect_call",
            "reg": format!("r{target_reg}"),
            "fallthrough": hex_u64(next_pc),
        }),
        EbpfOp::Exit => json!({"kind": "return"}),
        _ => fallthrough_json(next_pc),
    }
}
