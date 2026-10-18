//! Every byte offset around a page boundary, using native prefix-copy records.
use super::*;

#[test]
fn native_token_bno_replays_all_measured_copy_boundary_offsets() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../../docs/specifications/windows/native-token-bno-isolation/copy-cases.json"
    )).unwrap();
    let runs = corpus["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 46);
    let mut calls = 0u64;
    for run in runs {
        let (arch, laa) = match run["profile"].as_str().unwrap() {
            "arm64" => (WinArch::Arm64, false),
            "x64" => (WinArch::X64, false),
            "x86" => (WinArch::X86, false),
            "x86-laa" => (WinArch::X86, true),
            _ => panic!("unmeasured profile"),
        };
        let prefix = run["prefix"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
            .collect();
        let (mut process, mut thread, base, handles) = setup(arch, laa);
        let p = process.state_mut();
        select_bno(p, Isolation::new(Some(prefix)).unwrap());
        for (index, row) in run["cases"].as_array().unwrap().iter().enumerate() {
            let text = |key: &str| row[key].as_str().unwrap();
            let number = |key: &str| u32::from_str_radix(text(key), 16).unwrap();
            let remaining: usize = text("remaining").parse().unwrap();
            let (_, returned) = destinations(p, base, 0);
            p.vm.protect(base + PAGE_SIZE * 2, PAGE_SIZE, number("protection"))
                .unwrap();
            let output = base + PAGE_SIZE * 2 - remaining as u64;
            let sp = thread.cpu.sp();
            let actual = invoke(
                p,
                &mut thread,
                &[
                    handles[1],
                    44,
                    output,
                    u64::from(number("length")),
                    returned,
                ],
            );
            assert_eq!(actual, Outcome::Continue, "{run}/{index}");
            thread.cpu.set_sp(sp);
            assert_eq!(
                thread.cpu.gpr(0) as u32,
                number("status"),
                "status {run}/{index}"
            );
            assert_eq!(
                p.space.u32(returned).unwrap(),
                number("returned"),
                "return {run}/{index}"
            );
            assert_eq!(
                p.vm.query(base + PAGE_SIZE * 2).unwrap().protect,
                number("after"),
                "guard {run}/{index}"
            );
            let expected = text("output").as_bytes();
            assert_eq!(expected.len(), remaining * 2);
            let pointer = (output + 2 * arch.ptr_size()).to_le_bytes();
            for (offset, pair) in expected.chunks_exact(2).enumerate() {
                let byte = if pair[0] == b'@' {
                    pointer[usize::from(pair[1] - b'0')]
                } else {
                    u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()
                };
                assert_eq!(
                    p.space.u8(output + offset as u64).unwrap(),
                    byte,
                    "byte{offset} run{} row{index}",
                    run["native-log"]
                );
            }
            calls += 1;
        }
    }
    assert_eq!(calls, corpus["total-calls"].as_u64().unwrap());
}
