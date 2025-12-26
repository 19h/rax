//! GDB RSP command handlers.

use std::collections::HashMap;
use std::io::Write;
use std::time::Duration;

use tracing::debug;

use super::protocol::{
    decode_hex, parse_addr, parse_len, send_empty, send_error, send_ok, send_packet,
    send_stop_reply,
};
use super::{GdbChannels, GdbCommand, GdbResponse};

/// Handle a GDB RSP command.
/// Returns Ok(true) to continue, Ok(false) to disconnect.
pub fn handle_command<W: Write>(
    packet: &str,
    writer: &mut W,
    channels: &GdbChannels,
    breakpoints: &mut HashMap<u64, u8>,
) -> std::io::Result<bool> {
    // Get first character (command type)
    let cmd = packet.chars().next().unwrap_or(' ');
    let args = if packet.len() > 1 { &packet[1..] } else { "" };

    match cmd {
        // Query halt reason
        '?' => {
            send_stop_reply(writer, 5)?; // SIGTRAP
        }

        // Read registers
        'g' => {
            channels.cmd_tx.send(GdbCommand::ReadRegisters).ok();
            match channels.resp_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(GdbResponse::Registers(hex)) => {
                    send_packet(writer, &hex)?;
                }
                Ok(GdbResponse::Error(code)) => {
                    send_error(writer, code)?;
                }
                _ => {
                    send_error(writer, 1)?;
                }
            }
        }

        // Write registers
        'G' => {
            let data = match decode_hex(args) {
                Some(d) => d,
                None => {
                    send_error(writer, 1)?;
                    return Ok(true);
                }
            };
            channels.cmd_tx.send(GdbCommand::WriteRegisters(data)).ok();
            match channels.resp_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(GdbResponse::Ok) => send_ok(writer)?,
                _ => send_error(writer, 1)?,
            }
        }

        // Read memory
        'm' => {
            let parts: Vec<&str> = args.split(',').collect();
            if parts.len() != 2 {
                send_error(writer, 1)?;
                return Ok(true);
            }
            let addr = match parse_addr(parts[0]) {
                Some(a) => a,
                None => {
                    send_error(writer, 1)?;
                    return Ok(true);
                }
            };
            let len = match parse_len(parts[1]) {
                Some(l) => l,
                None => {
                    send_error(writer, 1)?;
                    return Ok(true);
                }
            };

            channels
                .cmd_tx
                .send(GdbCommand::ReadMemory { addr, len })
                .ok();
            match channels.resp_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(GdbResponse::Memory(hex)) => {
                    send_packet(writer, &hex)?;
                }
                Ok(GdbResponse::Error(code)) => {
                    send_error(writer, code)?;
                }
                _ => {
                    send_error(writer, 1)?;
                }
            }
        }

        // Write memory
        'M' => {
            let colon_pos = match args.find(':') {
                Some(p) => p,
                None => {
                    send_error(writer, 1)?;
                    return Ok(true);
                }
            };
            let header = &args[..colon_pos];
            let hex_data = &args[colon_pos + 1..];

            let parts: Vec<&str> = header.split(',').collect();
            if parts.len() != 2 {
                send_error(writer, 1)?;
                return Ok(true);
            }

            let addr = match parse_addr(parts[0]) {
                Some(a) => a,
                None => {
                    send_error(writer, 1)?;
                    return Ok(true);
                }
            };

            let data = match decode_hex(hex_data) {
                Some(d) => d,
                None => {
                    send_error(writer, 1)?;
                    return Ok(true);
                }
            };

            channels
                .cmd_tx
                .send(GdbCommand::WriteMemory { addr, data })
                .ok();
            match channels.resp_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(GdbResponse::Ok) => send_ok(writer)?,
                _ => send_error(writer, 1)?,
            }
        }

        // Continue execution
        'c' => {
            // Optional address argument (ignored for now)
            channels.cmd_tx.send(GdbCommand::Continue).ok();
            // Wait for stop
            match channels.resp_rx.recv() {
                Ok(GdbResponse::StopReply(sig)) => {
                    send_stop_reply(writer, sig)?;
                }
                Ok(GdbResponse::Stopped) => {
                    send_stop_reply(writer, 5)?; // SIGTRAP
                }
                _ => {
                    send_stop_reply(writer, 5)?;
                }
            }
        }

        // Single step
        's' => {
            // Optional address argument (ignored for now)
            channels.cmd_tx.send(GdbCommand::Step).ok();
            // Wait for stop
            match channels.resp_rx.recv() {
                Ok(GdbResponse::StopReply(sig)) => {
                    send_stop_reply(writer, sig)?;
                }
                Ok(GdbResponse::Stopped) => {
                    send_stop_reply(writer, 5)?; // SIGTRAP
                }
                _ => {
                    send_stop_reply(writer, 5)?;
                }
            }
        }

        // Set/remove breakpoint
        'Z' | 'z' => {
            let parts: Vec<&str> = args.split(',').collect();
            if parts.len() < 2 {
                send_error(writer, 1)?;
                return Ok(true);
            }

            let bp_type = parts[0];
            let addr = match parse_addr(parts[1]) {
                Some(a) => a,
                None => {
                    send_error(writer, 1)?;
                    return Ok(true);
                }
            };

            // Only support software breakpoints (type 0)
            if bp_type != "0" {
                send_empty(writer)?; // Unsupported
                return Ok(true);
            }

            if cmd == 'Z' {
                channels
                    .cmd_tx
                    .send(GdbCommand::SetBreakpoint { addr })
                    .ok();
            } else {
                channels
                    .cmd_tx
                    .send(GdbCommand::RemoveBreakpoint { addr })
                    .ok();
            }

            match channels.resp_rx.recv_timeout(Duration::from_secs(5)) {
                Ok(GdbResponse::Ok) => send_ok(writer)?,
                _ => send_error(writer, 1)?,
            }
        }

        // Detach
        'D' => {
            channels.cmd_tx.send(GdbCommand::Detach).ok();
            send_ok(writer)?;
            return Ok(false);
        }

        // Kill
        'k' => {
            channels.cmd_tx.send(GdbCommand::Kill).ok();
            return Ok(false);
        }

        // Query commands
        'q' => {
            handle_query(args, writer)?;
        }

        // Set thread (ignored - single threaded)
        'H' => {
            send_ok(writer)?;
        }

        // vCont (extended continue/step)
        'v' => {
            if args.starts_with("Cont?") {
                // Report supported vCont actions
                send_packet(writer, "vCont;c;s")?;
            } else if args.starts_with("Cont;") {
                let action = &args[5..];
                if action.starts_with('c') {
                    channels.cmd_tx.send(GdbCommand::Continue).ok();
                    match channels.resp_rx.recv() {
                        Ok(GdbResponse::StopReply(sig)) => send_stop_reply(writer, sig)?,
                        _ => send_stop_reply(writer, 5)?,
                    }
                } else if action.starts_with('s') {
                    channels.cmd_tx.send(GdbCommand::Step).ok();
                    match channels.resp_rx.recv() {
                        Ok(GdbResponse::StopReply(sig)) => send_stop_reply(writer, sig)?,
                        _ => send_stop_reply(writer, 5)?,
                    }
                } else {
                    send_empty(writer)?;
                }
            } else {
                send_empty(writer)?;
            }
        }

        // Unknown command
        _ => {
            debug!(cmd = %cmd, "Unknown GDB command");
            send_empty(writer)?;
        }
    }

    Ok(true)
}

/// Handle query commands (q...).
fn handle_query<W: Write>(args: &str, writer: &mut W) -> std::io::Result<()> {
    if args.starts_with("Supported") {
        // Report supported features
        send_packet(writer, "PacketSize=4096;swbreak+;vContSupported+")?;
    } else if args.starts_with("Attached") {
        // We attached to an existing process
        send_packet(writer, "1")?;
    } else if args.starts_with("TStatus") {
        // Tracepoint status - not supported
        send_empty(writer)?;
    } else if args.starts_with("fThreadInfo") {
        // First thread info query
        send_packet(writer, "m1")?; // Thread 1
    } else if args.starts_with("sThreadInfo") {
        // Subsequent thread info query
        send_packet(writer, "l")?; // End of list
    } else if args.starts_with("C") {
        // Current thread
        send_packet(writer, "QC1")?; // Thread 1
    } else if args.starts_with("Offsets") {
        // Section offsets - not applicable
        send_empty(writer)?;
    } else if args.starts_with("Symbol") {
        // Symbol lookup - not supported
        send_ok(writer)?;
    } else {
        debug!(query = %args, "Unknown query");
        send_empty(writer)?;
    }

    Ok(())
}
