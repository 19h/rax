//! Native read-only capture: views/data require write; the length need not.
use super::*;
use std::collections::HashMap;

macro_rules! corpus {
    ($name:literal) => {
        include_str!(concat!(
            "../../../../../../../docs/specifications/windows/native-secure-connect-capture/",
            $name
        ))
    };
}

fn blob(block: &str, name: &str) -> Vec<u8> {
    let prefix = format!("bytes name={name} ");
    let line = block
        .lines()
        .find(|line| line.starts_with(&prefix))
        .unwrap();
    let text = line.split("hex=").nth(1).unwrap();
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn replay(arch: WinArch, laa: bool, raw: &str) -> usize {
    let (mut process, mut thread) = super::replay::fixture_profile(arch, laa);
    let p = process.state_mut();
    p.object_namespace = selected_namespace();
    let arena = scratch(p, 16 * 4096);
    let handles = p.objects.handle_count();
    let objects = p.objects.iter().count();
    let committed = p.vm.committed_bytes();
    let mut count = 0;
    for block in raw.split("case=").skip(1) {
        let row: HashMap<_, _> = block
            .lines()
            .next()
            .unwrap()
            .split_whitespace()
            .filter_map(|value| value.split_once('='))
            .collect();
        let context = format!("{arch} laa={laa} readonly={}", row["name"]);
        assert_eq!(row["exception"], "00000000", "{context}");
        p.vm.protect(arena, 16 * 4096, prot::READWRITE).unwrap();
        p.space.wr(arena, &[0xA5; 16 * 4096]).unwrap();
        let at = |page: u64| arena + page * 4096 + 64;
        name(p, at(1), at(2), "\\Windows\\RaxLpcReadonly-model");
        p.space
            .wr(at(3), &[0, 0, 0, 0, 2, 0, 0, 0, 1, 1, 0, 0])
            .unwrap();
        p.space.wr(at(4), &blob(block, "client-view")).unwrap();
        p.space
            .wr(at(5), &[1, 1, 0, 0, 0, 0, 0, 5, 18, 0, 0, 0])
            .unwrap();
        p.space.wr(at(6), &blob(block, "server-view")).unwrap();
        p.space.w32(at(9), 32).unwrap();
        let page = row["readonly-page"].parse::<i32>().unwrap();
        if page >= 0 {
            p.vm.protect(arena + page as u64 * 4096, 4096, prot::READONLY)
                .unwrap();
        }
        let args = [
            at(0),
            at(1),
            at(3),
            at(4),
            at(5),
            at(6),
            at(7),
            at(8),
            at(9),
        ];
        assert_eq!(
            invoke(p, &mut thread, &args),
            Outcome::Continue,
            "{context}"
        );
        assert_eq!(
            thread.cpu.gpr(0) as u32,
            u32::from_str_radix(row["status"], 16).unwrap(),
            "{context}"
        );
        for (page, name, bytes) in [
            (0, "output", 32),
            (4, "client-view", 48),
            (6, "server-view", 24),
            (7, "message", 8),
            (8, "information", 64),
            (9, "length", 8),
        ] {
            assert_eq!(
                p.space.bytes(at(page), bytes).unwrap(),
                blob(block, name),
                "{context} {name}"
            );
        }
        assert_eq!(p.objects.handle_count(), handles, "{context}");
        assert_eq!(p.objects.iter().count(), objects, "{context}");
        assert_eq!(p.vm.committed_bytes(), committed, "{context}");
        count += 1;
    }
    assert_eq!(count, 11);
    count
}

#[test]
fn native_secure_connect_replays_all_44_readonly_probes_and_wow_copyback_rows() {
    let sources = [
        (
            WinArch::Arm64,
            false,
            corpus!("native-lpc-readonly-capture-arm64.log"),
        ),
        (
            WinArch::X64,
            false,
            corpus!("native-lpc-readonly-capture-x64.log"),
        ),
        (
            WinArch::X86,
            false,
            corpus!("native-lpc-readonly-capture-x86.log"),
        ),
        (
            WinArch::X86,
            true,
            corpus!("native-lpc-readonly-capture-x86-laa.log"),
        ),
    ];
    let count: usize = sources
        .iter()
        .map(|&(arch, laa, raw)| replay(arch, laa, raw))
        .sum();
    assert_eq!(count, 44);
}
