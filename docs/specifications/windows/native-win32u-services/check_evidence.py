"""Verify selected Win32u admission evidence, including explicit failure boundaries."""

from pathlib import Path, PureWindowsPath
import argparse
import hashlib
import json
import re

if not __debug__:
    raise SystemExit("optimized Python cannot execute this assertion-based verifier")

FRAME = "6140e8121129bbd12c871dac8525834b68aacff0b9484b027054ec0b58e45a62"
PARENT = "2ca3e35c54bcf4b64be63f524505a1a3e6b69b8b"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def basename(name):
    return PureWindowsPath(name).name if "\\" in name else Path(name).name


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--source-root", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent
    evidence = root / "evidence"
    native = root / "native"
    proof = json.loads((root / "proof.json").read_text(encoding="utf-8"))
    assert proof["checker-sha256"] == sha(Path(__file__).read_bytes())
    assert proof["source-parent"] == PARENT
    assert proof["source-frame-sha256"] == FRAME
    indexed = {}
    for row in proof["files"]:
        name = row["path"]
        assert name not in indexed and not Path(name).is_absolute()
        assert ".." not in Path(name).parts
        path = root / name
        assert path.resolve().is_relative_to(root)
        data = path.read_bytes()
        assert (len(data), sha(data)) == (row["bytes"], row["sha256"]), name
        indexed[name] = row

    def read(name):
        assert "evidence/" + name in indexed, name
        return json.loads((evidence / name).read_text(encoding="utf-8-sig"))

    def log(name):
        name = basename(name)
        assert "evidence/" + name in indexed, name
        return (evidence / name).read_text(encoding="utf-8", errors="replace")

    def finished(name):
        result = read(name)
        assert result["state"] == "finished" and result["exit-code"] == 0, name
        return result

    def steps(record):
        for row in record["steps"]:
            item = row["log"]
            if isinstance(item, dict):
                name, count, digest = item["path"], item["bytes"], item["sha256"]
            else:
                name, count, digest = item, row["bytes"], row["sha256"]
            data = (evidence / basename(name)).read_bytes()
            assert (len(data), sha(data)) == (count, digest), name

    def producer(record):
        steps(record)
        if "source-before" in record:
            before = record["source-before"]
            name = basename(before["path"])
            source = native / name
            assert source.is_file(), name
            data = source.read_bytes()
            assert (len(data), sha(data)) == (before["bytes"], before["sha256"])
            if "source-after" in record:
                assert before == record["source-after"]
        for key in ["runtime", "runtime-modules", "observer", "executable", "core"]:
            if key + "-before" in record and key + "-after" in record:
                assert record[key + "-before"] == record[key + "-after"]
        for row in record.get("outcomes", []):
            if "executable-before" in row:
                assert row["executable-before"] == row["executable-after"]

    frame = read("win32u-services-candidate1-source.json")
    assert (
        sha((evidence / "win32u-services-candidate1-source.json").read_bytes()) == FRAME
    )
    assert frame["base"] == PARENT and len(frame["files"]) == 29182
    assert len(frame["overrides"]) == 6
    assert all(row["path"].endswith(".rs") for row in frame["overrides"])
    if args.source_root:
        for row in frame["files"]:
            path = args.source_root / row["path"]
            if row["path"] == ".cargo/config.toml" and __import__("os").name == "nt":
                expected = (
                    374,
                    "9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c",
                )
                data = path.read_bytes()
            else:
                expected = row["bytes"], row["sha256"]
                data = (
                    str(path.readlink()).encode()
                    if path.is_symlink()
                    else path.read_bytes()
                )
            assert (len(data), sha(data)) == expected, row["path"]

    quality = finished("win32u-services-quality-v1.json")
    steps(quality)
    assert [row["name"] for row in quality["steps"]] == [
        "rustfmt",
        "strict-lib-clippy",
        "all-target-clippy",
    ]
    assert all(row["exit-code"] == 0 for row in quality["steps"])
    assert quality["owned-rust-files"] == 6

    for host, filename, count, focus in [
        ("macos", "win32u-services-macos-candidate1-retry-v3-gates.json", 8207, 4),
        ("linux", "win32u-services-linux-candidate1-retry-v2-gates.json", 8202, 4),
        ("windows", "win32u-services-candidate2-windows.json", 7684, 6),
    ]:
        result = finished(filename)
        steps(result)
        by_name = {row["name"]: row for row in result["steps"]}
        required = {"ordinals", "focus", "full", "capi", "integration", "all-targets"}
        assert required <= by_name.keys()
        for name, passed in [
            ("ordinals", 4),
            ("focus", focus),
            ("capi", 183),
            ("integration", 4 if host == "windows" else 544),
        ]:
            assert by_name[name]["exit-code"] == 0
            assert any(
                f"{passed} passed; 0 failed;" in value
                for value in by_name[name]["results"]
            )
        full = by_name["full"]
        expected_failed = 5 if host == "windows" else 0
        assert full["exit-code"] == (101 if expected_failed else 0)
        assert any(
            f"{count} passed; {expected_failed} failed; 2 ignored; 0 measured; 0 filtered out;"
            in value
            for value in full["results"]
        )
        assert by_name["all-targets"]["exit-code"] == 0
        if host == "windows":
            baseline = read("mutant-native-baseline-failures.json")
            assert set(result["exact-full-failures"]) == set(baseline["failures"])
            assert result["test-image-before"] == result["test-image-after"]
        else:
            assert result["manifest-sha256"] == FRAME

    original = read("win32u-services-macos-candidate1-gates.json")
    assert original["state"] == "failed" and original["exit-code"] == 1
    original_full = next(row for row in original["steps"] if row["name"] == "full")
    assert original_full["exit-code"] == 101
    assert "8205 passed; 2 failed; 2 ignored;" in log(original_full["log"])
    recheck = finished("win32u-services-macos-socket-recheck-v1.json")
    assert recheck["image-before"] == recheck["image-after"]
    assert len(recheck["steps"]) == 2 and all(
        row["exit-code"] == 0 for row in recheck["steps"]
    )
    lengths = recheck["abstract-path-lengths-bytes"]
    assert lengths["old"] + 1 > lengths["Darwin-sun_path-capacity"]
    assert lengths["short"] + 1 <= lengths["Darwin-sun_path-capacity"]
    original_linux = read("win32u-services-linux-candidate1-gates.json")
    assert original_linux["state"] == "failed" and original_linux["exit-code"] == 1
    assert "docker.sock" in log(original_linux["steps"][0]["log"])

    for name in [
        "macos-win32u-services-candidate1-owning-gates-v1.json",
        "linux-win32u-services-candidate1-owning-gates-v1.json",
        "win32u-services-native-owning-candidate2.json",
    ]:
        result = finished(name)
        steps(result)
        assert result["source-inputs"] == 3983 and result["archive-identities-stable"]
        assert all(row["exit-code"] == 0 for row in result["steps"])
    for first, last in [
        (
            "macos-win32u-services-candidate1-archives-before.json",
            "macos-win32u-services-candidate1-archives-after.json",
        ),
        (
            "linux-win32u-services-candidate1-archives-before.json",
            "linux-win32u-services-candidate1-archives-after.json",
        ),
        (
            "native-win32u-services-candidate1-pre-cpp-archives.json",
            "native-win32u-services-candidate1-final-owning-archive-identities.json",
        ),
    ]:
        before, after = read(first), read(last)
        assert before["archives"] == after["archives"]
        assert len(before["archives"]) == 2 and before["core-features"] == []
        for row in before["archives"]:
            assert row["members"] > 0 and row["structural-member-walk"] == "pass"

    stubs = finished("native-win32u-stubs-v1-four-views-launch.json")
    producer(stubs)
    assert len(stubs["outcomes"]) == 4
    assert stubs["runtime-modules-before"] == stubs["runtime-modules-after"]
    for row in stubs["outcomes"]:
        assert (
            row["exit-code"] == 0
            and row["executable-before"] == row["executable-after"]
        )
        assert "invoked-services=0" in log(row["log"])
    normal = finished("native-gdi-init2-v3-four-views-launch.json")
    repeat = finished("native-gdi-init2-repeat-v1.json")
    producer(normal)
    producer(repeat)
    assert len(normal["outcomes"]) == len(repeat["steps"]) == 4
    for outcome, again in zip(normal["outcomes"], repeat["steps"]):
        assert outcome["profile"] == again["profile"]
        values = []
        for filename in [outcome["log"], again["log"]["path"]]:
            rows = re.findall(
                r"call=([^ ]+) index=(\d+) tid=(\d+) return=([0-9a-f]+) error=([0-9a-f]+)",
                log(filename),
            )
            assert len(rows) == 32 and len({row[0] for row in rows}) == 4
            assert (
                len({row[2] for row in rows}) == 2
                and len({row[3] for row in rows}) == 1
            )
            assert {row[4] for row in rows} == {"12345678"}
            values.append(rows[0][3])
        assert values[0] != values[1]
    uninitialized = finished("native-gdi-init2-v2-four-views-launch.json")
    producer(uninitialized)
    assert len(uninitialized["outcomes"]) == 4
    assert {row["exit-code"] for row in uninitialized["outcomes"]} == {0xC000041D}
    for version in [2, 3]:
        result = finished(f"native-gdi-init2-entry-v{version}-four-views-launch.json")
        producer(result)
        assert result["source-before"] == result["source-after"]
        assert result["runtime-modules-before"] == result["runtime-modules-after"]
        text = log(result["outcomes"][0]["log"])
        before = re.search(
            r"phase=before-first-init2-svc callback=([0-9a-f]+) table=([0-9a-f]+)", text
        )
        after = re.search(
            r"phase=after-first-init2-svc callback=([0-9a-f]+) table=([0-9a-f]+)", text
        )
        assert before and after and int(before[1], 16) != 0 and int(before[2], 16) == 0
        assert before[1] == after[1] and int(after[2], 16) != 0
        assert (
            "restored-instructions=1 closed-module-references=2 removed-observers=1"
            in text
        )
        if version == 3:
            assert "bytes=1576960 state=00001000 protect=00000002 type=00040000" in text
            assert "delta=peb offset=00f8 before=0000000000000000" in text

    before = finished("native-win32u-caller-v2.json")
    producer(before)
    assert before["core-before"] == before["core-after"]
    assert "raw Windows arm64 service 0x12fd" in log(
        "native-win32u-caller-v2-whoami.log"
    )
    after = finished("native-win32u-next-frontier-v3.json")
    producer(after)
    assert after["core-before"] == after["core-after"]
    assert after["runtime-before"] == after["runtime-after"]
    assert after["source-inputs"] == 3983
    text = log("native-win32u-next-frontier-v3-whoami.log")
    assert "terminal turn=2875" in text
    assert (
        "native Win32u service NtGdiInit2 (0x12fd)" in text
        and "is not implemented" in text
    )
    assert "Exited(0)" not in text
    api = finished("win32u-services-owning-capi-candidate3.json")
    producer(api)
    smoke, whoami = api["outcomes"]
    assert smoke["reason"] == 4 and smoke["exit-code"] == 0
    assert (
        whoami["reason"] == 5
        and "native Win32u service NtGdiInit2 (0x12fd)" in whoami["diagnostic"]
    )
    assert "is not implemented" in whoami["diagnostic"]
    for name in [
        "native",
        "programs",
        "runtime",
        "source",
        "carrier",
        "adapter-inputs",
        "executable",
    ]:
        assert api[name + "-before"] == api[name + "-after"]
    runtime = finished("win32u-runtime-inputs-v2.json")
    assert len(runtime["runtime-inputs"]) >= 25
    actual = {
        basename(row["path"]).lower(): row["sha256"] for row in after["runtime-before"]
    }
    assert {"win32u.dll", "gdi32full.dll", "user32.dll"} <= actual.keys()
    boundary = finished("native-win32u-admission-boundary-v2.json")
    producer(boundary)
    assert boundary["source-inputs"] == 3983
    steps(boundary)
    for key in ["source", "core", "executable", "runtime"]:
        assert boundary[key + "-before"] == boundary[key + "-after"]
    selected_archives = read(
        "native-win32u-services-candidate1-final-owning-archive-identities.json"
    )["archives"]
    core = next(row for row in selected_archives if row["path"].endswith(".rlib"))
    carrier = next(row for row in selected_archives if row["path"].endswith(".lib"))
    for actual in [after["core-before"], boundary["core-before"]]:
        assert (actual["bytes"], actual["sha256"]) == (core["bytes"], core["sha256"])
    assert (api["carrier-before"]["bytes"], api["carrier-before"]["sha256"]) == (
        carrier["bytes"],
        carrier["sha256"],
    )
    boundary_text = log("native-win32u-admission-boundary-v2-cases.log")
    cases = [line for line in boundary_text.splitlines() if line.startswith("case=")]
    assert len(cases) in [3, 6]
    for line in cases:
        if "case=selected " in line:
            assert (
                "native Win32u service NtGdiInit2" in line and "not implemented" in line
            )
        else:
            assert "case=supplied " in line or "case=file " in line
            assert "0x1fef" in line and "has no admitted stub" in line
            assert "native Win32u service NtGdiInit2" not in line
    print(
        "PASS: selected Win32u admission; explicit unsupported boundary; native first-entry effects; three-host compiler and retained consumer evidence"
    )


if __name__ == "__main__":
    main()
