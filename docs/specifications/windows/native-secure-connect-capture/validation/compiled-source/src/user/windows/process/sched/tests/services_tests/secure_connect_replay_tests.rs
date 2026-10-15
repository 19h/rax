//! Replay all 352 independently checked native missing-port observations.
use super::*;
use std::collections::HashMap;

fn bytes(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn blob(block: &str, name: &str) -> Vec<u8> {
    let prefix = format!("bytes name={name} ");
    let lines: Vec<_> = block
        .lines()
        .filter(|line| line.starts_with(&prefix))
        .collect();
    assert_eq!(lines.len(), 1, "{name}");
    let row: HashMap<_, _> = lines[0]
        .split_whitespace()
        .filter_map(|value| value.split_once('='))
        .collect();
    let value = bytes(row["hex"]);
    assert_eq!(value.len(), row["size"].parse::<usize>().unwrap());
    value
}

fn number(row: &HashMap<&str, &str>, name: &str) -> u64 {
    row[name].parse().unwrap()
}

pub(super) fn fixture_profile(
    arch: WinArch,
    laa: bool,
) -> (crate::user::windows::process::WindowsProcess, Thread) {
    if !laa {
        return fixture(arch, "NtSecureConnectPort", SERVICE);
    }
    use crate::user::windows::process::{WindowsConfig, WindowsProcess};
    assert_eq!(arch, WinArch::X86);
    let mut image =
        include_bytes!("../../../../../../../tests/fixtures/user/windows/bin/x86/smoke.exe")
            .to_vec();
    let pe = u32::from_le_bytes(image[0x3C..0x40].try_into().unwrap()) as usize;
    let flags = u16::from_le_bytes(image[pe + 22..pe + 24].try_into().unwrap()) | 0x20;
    image[pe + 22..pe + 24].copy_from_slice(&flags.to_le_bytes());
    let mut config =
        WindowsConfig::embedded("C:\\secure-connect-laa.exe", vec![], vec![], 1).unwrap();
    config.arena_bytes = 64 << 20;
    let mut process = WindowsProcess::spawn_image(config, image).unwrap();
    let p = process.state_mut();
    let module = p.modules.by_name("ntdll.dll").unwrap();
    p.modules.nt_services = Some((module, table(arch, "NtSecureConnectPort", SERVICE)));
    let tid = *p.threads.keys().next().unwrap();
    let mut thread = p.threads.remove(&tid).unwrap();
    thread.attached = true;
    thread.main = false;
    (process, thread)
}

fn select(mode: u64, valid: u64, crossed: u64) -> u64 {
    match mode {
        1 => 0,
        2 => 1,
        3 => valid + 1,
        4 => crossed,
        _ => valid,
    }
}

fn replay(arch: WinArch, laa: bool, raw: &str) -> usize {
    let (mut process, mut thread) = fixture_profile(arch, laa);
    let p = process.state_mut();
    p.object_namespace = selected_namespace();
    let arena = scratch(p, 8 * 4096);
    let structures = scratch(p, 4096);
    let (unicode, qos, client, sid, server, text) = (
        structures,
        structures + 64,
        structures + 128,
        structures + 256,
        structures + 384,
        structures + 512,
    );
    let handles = p.objects.handle_count();
    let objects = p.objects.iter().count();
    let committed = p.vm.committed_bytes();
    let mut count = 0;
    for block in raw.split("input case=").skip(1) {
        let status_line = block
            .lines()
            .find(|line| line.starts_with("case="))
            .unwrap();
        let row: HashMap<_, _> = status_line
            .split_whitespace()
            .map(|value| value.split_once('=').unwrap())
            .collect();
        let context = format!("{arch} laa={laa} {}", row["name"]);
        assert_eq!(row["exception"], "00000000", "{context}");
        p.vm.protect(arena, 8 * 4096, prot::READWRITE).unwrap();
        p.space.wr(arena, &[0xA5; 8 * 4096]).unwrap();
        p.space.wr(structures, &[0; 4096]).unwrap();
        for (address, name) in [
            (unicode, "input-name-header"),
            (qos, "input-qos"),
            (client, "input-client-view"),
            (sid, "input-sid"),
            (server, "input-server-view"),
            (text, "input-name-text"),
        ] {
            p.space.wr(address, &blob(block, name)).unwrap();
        }
        let name_mode = number(&row, "name-mode");
        p.space
            .wptr(
                unicode + if arch.is64() { 8 } else { 4 },
                arch.ptr_size(),
                match name_mode {
                    4 => 0,
                    5 => 1,
                    _ => text,
                },
            )
            .unwrap();
        let out_mode = number(&row, "out");
        let message_mode = number(&row, "message");
        let length_mode = number(&row, "length-pointer");
        let data_mode = number(&row, "data");
        let length_at = arena
            + 4 * 4096
            + if length_mode == 4 {
                4096 - 2
            } else {
                64 + u64::from(length_mode == 3)
            };
        p.space
            .w32(length_at, number(&row, "length") as u32)
            .unwrap();
        for (page, enabled) in [
            (1, out_mode == 4),
            (3, message_mode == 4),
            (5, length_mode == 4),
            (7, data_mode == 3),
        ] {
            if enabled {
                p.vm.protect(arena + page * 4096, 4096, prot::NOACCESS)
                    .unwrap();
            }
        }
        if out_mode == 5 {
            p.vm.protect(arena, 4096, prot::READONLY).unwrap();
        }
        let nullable = |mode: u64, address| match mode {
            1 => 0,
            2 => 1,
            _ => address,
        };
        let args = [
            select(out_mode, arena + 64, arena + 4093),
            nullable(if name_mode <= 2 { name_mode } else { 0 }, unicode),
            nullable(number(&row, "qos"), qos),
            nullable(number(&row, "client"), client),
            nullable(number(&row, "sid"), sid),
            nullable(number(&row, "server"), server),
            select(message_mode, arena + 2 * 4096 + 64, arena + 3 * 4096 - 2),
            match data_mode {
                1 => 0,
                2 => 1,
                _ => arena + 6 * 4096 + if data_mode == 3 { 4080 } else { 64 },
            },
            select(length_mode, arena + 4 * 4096 + 64, arena + 5 * 4096 - 2),
        ];
        let outcome = invoke(p, &mut thread, &args);
        assert_eq!(outcome, Outcome::Continue, "{context}");
        let expected = u32::from_str_radix(row["status"], 16).unwrap();
        assert_eq!(thread.cpu.gpr(0) as u32, expected, "{context}");
        p.vm.protect(arena, 8 * 4096, prot::READWRITE).unwrap();
        for (name, page, crossed) in [
            ("output", 0, out_mode == 4),
            ("message", 2, message_mode == 4),
            ("length", 4, length_mode == 4),
            ("information", 6, data_mode == 3),
        ] {
            let address = arena + page * 4096 + if crossed { 4096 - 32 } else { 64 };
            assert_eq!(
                p.space.bytes(address, 64).unwrap(),
                blob(block, name),
                "{context} {name}"
            );
        }
        assert_eq!(
            p.space.bytes(client, 48).unwrap(),
            blob(block, "client-view"),
            "{context}"
        );
        assert_eq!(
            p.space.bytes(server, 24).unwrap(),
            blob(block, "server-view"),
            "{context}"
        );
        assert_eq!(p.objects.handle_count(), handles, "{context}");
        assert_eq!(p.objects.iter().count(), objects, "{context}");
        assert_eq!(p.vm.committed_bytes(), committed, "{context}");
        count += 1;
    }
    assert_eq!(count, 88);
    count
}

macro_rules! corpus {
    ($name:literal) => {
        include_str!(concat!(
            "../../../../../../../docs/specifications/windows/native-secure-connect-capture/",
            $name
        ))
    };
}

#[test]
fn native_secure_connect_replays_352_measured_failures_and_exact_caller_buffers() {
    let sources = [
        (
            WinArch::Arm64,
            false,
            corpus!("native-lpc-missing-controls-arm64.log"),
        ),
        (
            WinArch::X64,
            false,
            corpus!("native-lpc-missing-controls-x64.log"),
        ),
        (
            WinArch::X86,
            false,
            corpus!("native-lpc-missing-controls-x86.log"),
        ),
        (
            WinArch::X86,
            true,
            corpus!("native-lpc-missing-controls-x86-laa.log"),
        ),
    ];
    let count: usize = sources
        .iter()
        .map(|&(arch, laa, raw)| replay(arch, laa, raw))
        .sum();
    assert_eq!(count, 352);
}
