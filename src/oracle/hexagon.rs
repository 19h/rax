//! Hexagon packet decoding and shared static/seeded packet lifting.

use super::*;
use crate::smir::lift::{LiftError, LiftResult};

pub(super) fn decode_hexagon_packet(bytes: &[u8], opts: &OracleOptions) -> Result<Value, String> {
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return Err("Hexagon packet bytes must be a non-empty multiple of 4".to_string());
    }

    let mut decoded_ops = Vec::new();
    let mut control_effects = Vec::new();
    let mut parse_bits = Vec::new();
    let mut warnings = Vec::new();
    let mut immext = None;
    let mut first_parse = None;
    let mut second_parse = None;
    let mut has_duplex = false;
    let mut end_seen = false;
    let mut consumed_words = 0usize;

    for (idx, chunk) in bytes.chunks_exact(4).enumerate() {
        let word = word_from_bytes(chunk, opts.hexagon_endian);
        let parse = (word >> 14) & 0x3;
        parse_bits.push(parse);
        if idx == 0 {
            first_parse = Some(parse);
        } else if idx == 1 {
            second_parse = Some(parse);
        }

        let word_pc = opts.pc.wrapping_add((idx * 4) as u64);
        consumed_words = idx + 1;

        if parse == 0 {
            has_duplex = true;
            match hex_decode::decode_duplex(word, opts.hexagon_isa) {
                Some((slot1, slot0)) => {
                    for (slot, sub) in [(1u8, slot1), (0u8, slot0)] {
                        let cf = hex_control_flow(&sub.insn, word_pc, opts.pc, false);
                        if !is_fallthrough(&cf) {
                            control_effects.push(cf.clone());
                        }
                        decoded_ops.push(json!({
                            "offset": idx * 4,
                            "pc": hex_u64(word_pc),
                            "word": hex_u32(word),
                            "parse_bits": parse,
                            "duplex_slot": slot,
                            "opcode": sub.opcode.map(hex_opcode::opcode_name),
                            "op": format!("{:?}", sub.insn),
                            "control_flow": cf,
                        }));
                    }
                }
                None => {
                    decoded_ops.push(json!({
                        "offset": idx * 4,
                        "pc": hex_u64(word_pc),
                        "word": hex_u32(word),
                        "parse_bits": parse,
                        "error": "unknown_duplex",
                    }));
                }
            }
            end_seen = true;
            if consumed_words * 4 != bytes.len() {
                warnings.push("bytes_after_packet_end");
            }
            break;
        }

        let decoded = hex_decode::decode(word, immext, opts.hexagon_isa);
        let fields = hex_fields(word);
        let cf = hex_control_flow(&decoded.insn, word_pc, opts.pc, decoded.used_ext);
        if !is_fallthrough(&cf) {
            control_effects.push(cf.clone());
        }
        decoded_ops.push(json!({
            "offset": idx * 4,
            "pc": hex_u64(word_pc),
            "word": hex_u32(word),
            "parse_bits": parse,
            "opcode": decoded.opcode.map(hex_opcode::opcode_name),
            "op": format!("{:?}", decoded.insn),
            "used_ext": decoded.used_ext,
            "fields": fields,
            "control_flow": cf,
        }));

        immext = match decoded.insn {
            HexInsn::ImmExt { value } => Some(value),
            _ => None,
        };

        if parse == 0x3 {
            end_seen = true;
            if consumed_words * 4 != bytes.len() {
                warnings.push("bytes_after_packet_end");
            }
            break;
        }
    }

    let loop0_end = matches!((first_parse, second_parse), (Some(first), Some(second)) if first == 0b10 && second != 0);
    let loop1_end = matches!((first_parse, second_parse), (Some(first), Some(second)) if second == 0b10 && first != 0);
    let smir = if opts.include_smir {
        lift_smir(SourceArch::Hexagon, &bytes[..consumed_words * 4], opts)
    } else {
        json!({"available": false, "reason": "disabled"})
    };

    Ok(json!({
        "isa": "hexagon",
        "hexagon_isa": format!("{:?}", opts.hexagon_isa),
        "endian": format!("{:?}", opts.hexagon_endian).to_lowercase(),
        "pc": hex_u64(opts.pc),
        "input": input_json(bytes),
        "decoded_ops": decoded_ops,
        "packet_flags": {
            "word_count": consumed_words,
            "parse_bits": parse_bits,
            "end_seen": end_seen,
            "complete": end_seen && consumed_words * 4 == bytes.len(),
            "has_duplex": has_duplex,
            "first_parse": first_parse,
            "second_parse": second_parse,
            "loop0_end": loop0_end,
            "loop1_end": loop1_end,
            "warnings": warnings,
        },
        "control_flow": packet_control_flow(control_effects),
        "smir": smir,
        "side_effects": side_effects_not_run(),
    }))
}

/// Lift one complete ordinary-word packet. A single lifter/context retains
/// extender, producer, packet-PC and predicate state across its words. An error
/// rejects the complete result; callers never receive a successfully lifted
/// prefix after a later unsupported instruction.
pub(super) fn lift_packet(
    bytes: &[u8],
    options: &OracleOptions,
    context: &mut LiftContext,
) -> Result<LiftResult, LiftError> {
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return Err(LiftError::Incomplete {
            addr: options.pc,
            have: bytes.len(),
            need: (bytes.len() / 4 + 1) * 4,
        });
    }
    let mut lifter = HexagonLifter::new(options.hexagon_isa);
    let mut packet = LiftResult::fallthrough(Vec::new(), 0);
    for (index, chunk) in bytes.chunks_exact(4).enumerate() {
        let pc = options.pc.wrapping_add(index as u64 * 4);
        if index == 4 {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: "Hexagon packet longer than four words".into(),
            });
        }
        let word = word_from_bytes(chunk, options.hexagon_endian);
        let parse = (word >> 14) & 3;
        // A loop-end marker changes control flow after packet commit; the word
        // lifter does not expose that effect. Retain decoded loop metadata but
        // reject execution IR until the packet loop effect is represented.
        if parse == 2 {
            return Err(LiftError::Unsupported {
                addr: pc,
                mnemonic: "Hexagon hardware-loop packet".into(),
            });
        }
        let lifted = lifter.lift_insn(pc, &word.to_le_bytes(), context)?;
        if lifted.control_flow.ends_block() {
            if packet.control_flow.ends_block() {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: "multiple Hexagon packet control-flow instructions".into(),
                });
            }
            if parse != 3
                && matches!(
                    lifted.control_flow,
                    ControlFlow::Call { .. }
                        | ControlFlow::Return
                        | ControlFlow::Trap { .. }
                        | ControlFlow::Syscall
                )
            {
                return Err(LiftError::Unsupported {
                    addr: pc,
                    mnemonic: "Hexagon call/return/trap before packet end".into(),
                });
            }
            packet.branch_targets = lifted.control_flow.direct_targets();
            packet.control_flow = lifted.control_flow;
        }
        packet.ops.extend(lifted.ops);
        packet.bytes_consumed += 4;
        if parse == 3 {
            for (index, operation) in packet.ops.iter_mut().enumerate() {
                operation.id = OpId(index as u16);
            }
            return Ok(packet);
        }
    }
    Err(LiftError::Incomplete {
        addr: options.pc.wrapping_add(packet.bytes_consumed as u64),
        have: packet.bytes_consumed,
        need: packet.bytes_consumed + 4,
    })
}

/// Observable commit boundaries are part of the public packet IR.
pub(super) fn packet_boundary_json(begin: bool) -> Value {
    json!({"opcode": if begin { "hexagon_packet_begin" } else { "hexagon_packet_commit" }})
}
