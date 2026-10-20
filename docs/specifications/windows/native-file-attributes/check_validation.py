"""Audit frozen inputs, independent NT observations and actual owning results."""

from pathlib import Path
import argparse
import hashlib
import importlib.util
import json
import os
import re
import sys

assert not sys.flags.optimize, "Assertions are required for this evidence audit"

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]


def identity(path):
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as stream:
        for raw in iter(lambda: stream.read(1 << 20), b""):
            size += len(raw)
            digest.update(raw)
    return size, digest.hexdigest()


def pair(row):
    return row["bytes"], row["sha256"]


def load(path):
    return json.loads(path.read_text())


def finished(row):
    assert row["state"] == "finished" and row["exit-code"] == 0


def audit_native(validation, receipt, log, quality):
    native = receipt("native-file-attributes-v8-launch.json")
    finished(native)
    assert len(native["steps"]) == 16
    assert all(row["exit-code"] == 0 for row in native["steps"])
    for row in native["steps"]:
        log(row)
    assert native["source-before"] == native["source-after"]
    assert native["runtime-modules-before"] == native["runtime-modules-after"]
    source_row = native["source-before"]
    assert identity(validation / "native-file-attributes-v8.cpp") == pair(source_row)
    expected = {
        1: 0,
        2: 0xC0000034,
        3: 0xC000003A,
        4: 0xC000003A,
        5: 0,
        6: 0,
        7: 0xC000003B,
        8: 0xC000003B,
        9: 0,
        10: 0,
        11: 0xC000000D,
        12: 0xC0000024,
        13: 0xC0000008,
        14: 0xC000000D,
        15: 0xC000003B,
        16: 0xC000000D,
        17: 0xC000000D,
        18: 0xC000000D,
        19: 0,
        20: 0xC000000D,
        21: 0xC0000033,
        22: 0xC0000005,
        23: 0xC000003B,
        24: 0xC0000005,
        25: 0x80000002,
        26: 0x80000002,
        28: 0xC0000005,
        29: 0xC0000005,
        30: 0xC0000005,
        31: 0xC0000005,
        33: 0xC0000005,
        34: 0xC0000005,
        35: 0xC0000005,
        36: 0x80000001,
        37: 0,
        38: 0,
        39: 0,
        40: 0,
        41: 0,
        42: 0,
        44: 0x80000001,
        45: 0x80000001,
        46: 0x80000001,
        47: 0x80000001,
        50: 0xC0000058,
        51: 0,
        52: 0,
        53: 0x80000002,
        54: 0xC0000005,
        55: 0xC0000033,
        56: 0xC0000008,
        57: 0xC0000008,
        58: 0xC000003B,
        59: 0,
        94: 0x80000001,
        95: 0,
        96: 0xC0000058,
        97: 0,
        98: 0,
        99: 0xC0000005,
        100: 0x80000001,
        101: 0xC0000005,
        102: 0xC0000005,
        103: 0xC0000005,
        104: 0,
        105: 0,
        106: 0,
        107: 0,
        108: 0,
        109: 0,
        110: 0,
        111: 0xC00000A5,
        112: 0xC000000D,
        113: 0,
        114: 0x80000001,
        115: 0x80000001,
        116: 0x80000001,
        117: 0xC000003A,
        118: 0xC000003A,
        119: 0xC0000103,
        120: 0xC0000034,
        121: 0xC0000034,
        122: 0xC000003A,
        123: 0xC0000033,
        124: 0xC0000033,
        125: 0,
        126: 0xC000003B,
        127: 0,
        128: 0xC000000D,
        129: 0,
        130: 0xC000003A,
        131: 0xC000003B,
        132: 0xC0000034,
        133: 0xC0000033,
        134: 0,
        135: 0xC000003A,
        136: 0xC000000D,
        137: 0xC000000D,
        138: 0xC0000033,
        139: 0,
        140: 0,
        141: 0,
        142: 0,
        143: 0,
        144: 0x80000001,
        147: 0x80000001,
        148: 0x80000001,
        149: 0x80000001,
        150: 0x80000001,
        157: 0xC0000078,
        158: 0xC0000078,
        159: 0xC0000077,
        160: 0xC0000077,
        161: 0xC0000077,
        162: 0,
        163: 0x80000001,
        164: 0x80000001,
        165: 0x80000001,
        167: 0x80000001,
        168: 0xC000000D,
        170: 0x80000001,
        171: 0x80000001,
        172: 0,
        174: 0xC0000005,
    }
    observations = 0
    for outcome in native["outcomes"]:
        profile = outcome["profile"]
        wow = profile.startswith("x86") and profile != "x64"
        assert (
            outcome["exit-code"] == 0
            and outcome["executable-before"] == outcome["executable-after"]
        )
        row = next(row for row in native["steps"] if row["name"] == profile)
        text = log(row)
        records = re.findall(
            r"^case full=(\d) role=(\d+) status=([0-9A-F]{8}) exception=([0-9A-F]{8}) second-protect=([0-9A-F]{8}) bytes=([0-9A-F]+)$",
            text,
            re.M,
        )
        assert len(records) == 348
        assert {(int(full), int(role)) for full, role, *_ in records} == {
            (full, role) for full in [0, 1] for role in range(1, 175)
        }
        handles = re.search(
            r"finished=1 handle-count-before=(\d+) handle-count-after=(\d+)", text
        )
        assert handles and handles[1] == handles[2]
        for full, role, value, exception, protection, raw in records:
            full, role = int(full), int(role)
            target = expected.get(role)
            if role in [25, 26, 27, 32]:
                target = 0 if wow else 0x80000002
            if role == 43:
                target = 0xC000000D if wow else 0x80000001
            if role in [48, 49, 169]:
                target = 0xC0000005 if wow else 0x80000002
            if role in [92, 93, 145, 146, 151, 152, 166]:
                target = 0xC0000005 if wow else 0x80000001
            if 60 <= role <= 91:
                target = 0 if (1 << (role - 60)) & 0x11FF2 else 0xC000000D
            if 153 <= role <= 156:
                available = [36, 40, 52, 56][role - 153]
                target = (
                    0x80000002
                    if not wow and available % 8
                    else 0xC0000005 if full and available < 56 else 0
                )
            if role == 173:
                target = 0 if wow else 0xC0000005
            assert (
                target is not None
                and int(value, 16) == target
                and int(exception, 16) == 0
            ), (profile, full, role, value, target)
            if role in [
                36,
                43,
                44,
                45,
                46,
                47,
                92,
                93,
                94,
                100,
                114,
                115,
                116,
                144,
                145,
                146,
                148,
                149,
                150,
                151,
                152,
                163,
                164,
                165,
                166,
                167,
                170,
                171,
            ]:
                assert int(protection, 16) == (0x104 if target != 0x80000001 else 4)
            if role == 1:
                data = bytes.fromhex(raw)
                assert [
                    int.from_bytes(data[n : n + 8], "little") for n in [0, 8, 16]
                ] == [132000000000000001, 132000000000000002, 132000000000000003]
                if full:
                    assert (
                        int.from_bytes(data[32:40], "little"),
                        int.from_bytes(data[40:48], "little"),
                    ) == (8, 5)
                    assert data[48:56] == bytes.fromhex("8000000000000000")
                else:
                    assert data[32:36] == bytes.fromhex("80000000")
                    assert data[36:40] == (b"\xa5" * 4 if wow else b"\0" * 4)
            observations += 1
    assert observations == quality["native-observations-v8"] == 1392
    return observations


def audit_pairs(validation, receipt, log, source, quality):
    pattern = r"^case full=(\d) role=(\d+) status=([0-9A-F]{8}) exception=([0-9A-F]{8}) second-protect=([0-9A-F]{8}) bytes=([0-9A-F]+)$"
    versions = {}
    for version, roles in [(9, 195), (10, 426)]:
        capture = receipt(f"native-file-attributes-v{version}-launch.json")
        finished(capture)
        assert len(capture["steps"]) == 16
        assert capture["source-before"] == capture["source-after"]
        assert capture["runtime-modules-before"] == capture["runtime-modules-after"]
        assert identity(validation / f"native-file-attributes-v{version}.cpp") == pair(
            capture["source-before"]
        )
        for step in capture["steps"]:
            assert step["exit-code"] == 0
            log(step)
        assert {x["profile"] for x in capture["outcomes"]} == {
            "arm64",
            "x64",
            "x86",
            "x86-laa",
        }
        profiles = {}
        for outcome in capture["outcomes"]:
            assert (
                outcome["exit-code"] == 0
                and outcome["executable-before"] == outcome["executable-after"]
            )
            text = log(
                next(x for x in capture["steps"] if x["name"] == outcome["profile"])
            )
            records = re.findall(pattern, text, re.M)
            assert len(records) == roles * 2
            rows = {
                (int(full), int(role)): (int(status, 16), int(protection, 16))
                for full, role, status, exception, protection, data in records
            }
            assert set(rows) == {
                (full, role) for full in [0, 1] for role in range(1, roles + 1)
            }
            assert all(int(x[3], 16) == 0 for x in records)
            handles = re.search(
                r"finished=1 handle-count-before=(\d+) handle-count-after=(\d+)", text
            )
            assert handles and handles[1] == handles[2]
            profiles[outcome["profile"]] = rows
        versions[version] = profiles
    oracle = receipt("file-attributes-pair-oracle-v10.json")
    fixture = (
        source
        / "src/user/windows/process/sched/tests/services_tests/file_attributes_tests/paired-v10.json"
    )
    assert identity(fixture) == identity(
        validation / "file-attributes-pair-oracle-v10.json"
    )
    assert len(oracle["cases"]) == 231
    assert [(x["left"], x["right"]) for x in oracle["cases"]] == [
        (i, j) for i in range(21) for j in range(i, 21)
    ]
    for profile, rows in versions[10].items():
        assert {k: v for k, v in rows.items() if k[1] <= 195} == versions[9][profile]
        view = "wow" if profile.startswith("x86") else "native"
        for row in oracle["cases"]:
            assert 196 <= row["role"] <= 426
            assert rows[(0, row["role"])] == rows[(1, row["role"])] == tuple(row[view])
    red = receipt("file-attributes-pair-red-epoch2.json")
    assert red["state"] == "finished" and red["exit-code"] == 101
    assert red["source-before"] == red["source-after"]
    for row in red["source-before"]:
        assert identity(source / row["path"]) == pair(row), row["path"]
    assert (
        "0 passed; 1 failed;"
        in (validation / "file-attributes-pair-red-epoch2.log").read_text()
    )
    assert quality["native-observations-latest"] == 426 * 2 * 4 == 3408
    assert quality["paired-native-observations"] == 231 * 2 * 4 == 1848
    assert quality["compiled-pair-calls-per-host"] == 231 * 2 * 3 == 1386
    return 3408


def audit(directory=HERE, source=ROOT):
    directory, source = Path(directory), Path(source)
    validation = directory / "validation"
    envelope = load(directory / "validation-envelope.json")
    assert {row["path"] for row in envelope["files"]} == {
        p.relative_to(directory).as_posix()
        for p in directory.rglob("*")
        if p.is_file() and p.name != "validation-envelope.json"
    }
    for row in envelope["files"]:
        assert identity(directory / row["path"]) == pair(row), row["path"]
    for row in envelope["projection-files"]:
        assert identity(source / row["path"]) == pair(row), row["path"]

    spec = importlib.util.spec_from_file_location(
        "evidence_projection", source / "tools/evidence/public_projection.py"
    )
    projection = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(projection)
    quality_projection = load(directory / "quality.json")["public-projections"]
    projected = projection.check(source)
    assert (projected["projected-streams"], projected["projection-bindings"]) == (
        quality_projection["streams"],
        quality_projection["bindings"],
    )

    def bound(path, record):
        assert identity(path) == pair(record) or projection.recorded_identity(
            path
        ) == pair(record), path

    def receipt(name):
        return load(validation / name)

    def log(row):
        path = validation / row["log"]
        bound(path, row)
        return path.read_text(errors="replace")

    quality = load(directory / "quality.json")
    assert quality["full-process-objective-complete"] is False
    assert quality["whole-current-root-products-validated"] is False
    assert quality["independent-physical-x64-kernel"] is False
    assert quality["full-library"] == {
        "macos": {"passed": 8116, "failed": 0, "ignored": 2},
        "linux": {"passed": 8110, "failed": 0, "ignored": 2},
        "windows": {"passed": 7578, "failed": 5, "ignored": 2},
    }
    assert quality["integration"] == {"macos": 544, "linux": 544, "windows": 4}
    assert quality["observed-frontier"] == "NtOpenFile (0x33)"
    frozen = receipt("file-attributes-source-epoch3.json")
    assert len(frozen["files"]) == quality["source-files"] == 25097
    assert (
        sum(row["bytes"] for row in frozen["files"])
        == frozen["total-bytes"]
        == quality["source-bytes"]
        == 1835033720
    )
    assert (
        quality["windows-source-bytes"]
        == frozen["total-bytes"] - 552 + 374
        == 1835033542
    )
    for row in frozen["files"]:
        path = source / row["path"]
        if os.name == "nt" and row["path"] == ".cargo/config.toml":
            assert identity(path) == (
                374,
                "9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c",
            )
        elif row["kind"] == "symlink-text" and path.is_symlink():
            raw = os.readlink(path).encode()
            assert (len(raw), hashlib.sha256(raw).hexdigest()) == pair(row)
        else:
            assert identity(path) == pair(row), row["path"]
    owning = receipt("file-attributes-owning-source-hashes-epoch3.json")
    assert len(owning["sources"]) == quality["owning-inputs"] == 3838
    for row in owning["sources"]:
        assert identity(source / row["path"]) == pair(row), row["path"]

    unaligned = receipt("file-attributes-unaligned-red-epoch1.json")
    assert unaligned["state"] == "finished" and unaligned["exit-code"] == 101
    assert (
        "0 passed; 1 failed;"
        in (validation / "file-attributes-unaligned-red-epoch1.log").read_text()
    )
    assert identity(source / unaligned["test-source"]["path"]) == pair(
        unaligned["test-source"]
    )

    red = receipt("file-attributes-baseline-epoch2.json")
    assert red["state"] == "finished" and red["exit-code"] == 101
    assert red["source-before"] == red["source-after"]
    for row in red["source-before"]:
        assert identity(directory / "baseline-source" / row["path"]) == pair(row)
    assert (
        "0 passed; 3 failed;"
        in (validation / "file-attributes-baseline-epoch2.log").read_text()
    )

    gate_names = {
        "macos": "file-attributes-macos-epoch3-gates.json",
        "linux": "file-attributes-linux-epoch3-gates.json",
        "windows": "file-attributes-native-epoch3-gates.json",
    }
    assert set(quality["windows-known-failures"]) == {
        "smir::lower::aarch64::tests::vector::lowers_vector_fp16_arithmetic_runtime",
        "smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_with_flags_sets_carry",
        "smir::lower::aarch64::tests::bit::lowers_bzhi_x_imm_index_at_width_with_flags_sets_carry",
        "smir::lower::aarch64::tests::bit::lowers_bzhi_x_two_imms_all_ones_with_flags_as_movn_sets_carry",
        "smir::lower::aarch64::tests::bit::lowers_bzhi_x_with_flags_and_low_byte_index_guards",
    }
    for host, name in gate_names.items():
        result = receipt(name)
        finished(result)
        steps = {row["name"]: row for row in result["steps"]}
        for row in result["steps"]:
            log(row)
            assert row["exit-code"] == (
                101 if host == "windows" and row["name"] == "full" else 0
            )
        assert "14 passed; 0 failed;" in log(steps["focus"])
        expected = quality["full-library"][host]
        assert (
            f"{expected['passed']} passed; {expected['failed']} failed; 2 ignored; 0 measured; 0 filtered out;"
            in log(steps["full"])
        )
        assert "183 passed; 0 failed;" in log(steps["capi"])
        assert f"{quality['integration'][host]} passed; 0 failed;" in log(
            steps["integration"]
        )
        assert steps["all-targets"]["exit-code"] == 0
        if host == "windows":
            assert (
                "native_file_attributes_installed_leaves_match_native_metadata_and_ret8 ... ok"
                in log(steps["focus"])
            )
            tail = log(steps["full"]).rsplit("failures:\n", 1)[1]
            failures = set(re.findall(r"^    (\S+)\s*$", tail, re.M))
            assert (
                failures
                == set(quality["windows-known-failures"])
                == set(result["exact-existing-failures"])
            )
    cross = receipt("file-attributes-windows-cross-xwin-epoch4.json")
    finished(cross)
    assert cross["inputs-unchanged"] and cross["inputs-before"] == cross["inputs-after"]
    assert [(row["arch"], row["exit-code"]) for row in cross["steps"]] == [
        ("aarch64", 0),
        ("x86_64", 0),
    ]

    audit_native(validation, receipt, log, quality)
    observations = audit_pairs(validation, receipt, log, source, quality)
    prior = receipt("file-attributes-prior-alias-stress-revalidation-epoch1.json")
    assert (
        prior["all-rejected"]
        and prior["positive-restorations-verified"] == len(prior["controls"]) == 13
    )
    assert all(row["rejected"] for row in prior["controls"])
    assert identity(validation / prior["runner"]["path"]) == pair(prior["runner"])
    context = receipt("file-attributes-host-context-epoch1.json")
    finished(context)
    assert context["kernel"] == {"major": 10, "minor": 0, "build": 29683}
    assert context["native-python-machine"] == "ARM64" and context["pointer-bytes"] == 8
    assert identity(validation / context["source"]["path"]) == pair(context["source"])

    for name in [
        "macos-file-attributes-epoch4-owning-gates.json",
        "linux-file-attributes-epoch4-owning-gates.json",
        "file-attributes-native-owning-epoch3.json",
    ]:
        result = receipt(name)
        finished(result)
        assert result["archive-identities-stable"]
        for row in result["steps"]:
            assert row["exit-code"] == 0
            log(row)
    archive_sets = []
    for host, before, after in [
        (
            "macos",
            "macos-file-attributes-epoch3-c-pre-cpp-archive-identities.json",
            "macos-file-attributes-epoch3-c-final-owning-archive-identities.json",
        ),
        (
            "linux",
            "linux-file-attributes-epoch3-pre-cpp-archive-identities.json",
            "linux-file-attributes-epoch3-final-owning-archive-identities.json",
        ),
        (
            "windows",
            "native-file-attributes-epoch3-pre-cpp-archives.json",
            "native-file-attributes-epoch3-final-owning-archive-identities.json",
        ),
    ]:
        left, right = receipt(before), receipt(after)
        assert left["archives"] == right["archives"]
        for row in [left, right]:
            assert row["core-features"] == row["core-artifact"]["features"] == []
            assert row["sources"] == owning["sources"] and len(row["sources"]) == 3838
            assert all(
                item["structural-member-walk"] == "pass" and item["members"] > 0
                for item in row["archives"]
            )
            bound(
                validation
                / Path(row["compiler-messages"]["path"].replace("\\", "/")).name,
                row["compiler-messages"],
            )
        archive_sets.append(right)

    ordinary, capi, retained = [
        receipt(name)
        for name in [
            "file-attributes-native-ordinary-epoch3.json",
            "file-attributes-owning-capi-epoch3.json",
            "file-attributes-retained-owning-epoch3.json",
        ]
    ]
    for result in [ordinary, capi, retained]:
        finished(result)
    for result in [ordinary, capi]:
        for row in result["steps"]:
            assert row["exit-code"] == 0
            log(row)
    for key in ["core", "observer", "executable", "runtime"]:
        assert ordinary[key + "-before"] == ordinary[key + "-after"]
    for key in [
        "native",
        "programs",
        "runtime",
        "source",
        "carrier",
        "adapter-inputs",
        "executable",
    ]:
        assert capi[key + "-before"] == capi[key + "-after"]
    assert (
        "Complete(Exited(0))"
        in (validation / "file-attributes-native-ordinary-epoch3-smoke.log").read_text()
    )
    assert (
        "NtOpenFile (0x33)"
        in (
            validation / "file-attributes-native-ordinary-epoch3-whoami.log"
        ).read_text()
    )
    assert capi["outcomes"][0]["reason"] == 4 and capi["outcomes"][0]["exit-code"] == 0
    assert (
        capi["outcomes"][1]["reason"] == 5
        and "NtOpenFile (0x33)" in capi["outcomes"][1]["diagnostic"]
    )
    assert (
        capi["native-before"][1]["stdout-hex"]
        == "6e7420617574686f726974795c73797374656d0d0a"
    )
    assert all(row["exit-code"] == 0 for row in capi["native-before"])
    windows_archives = archive_sets[2]["archives"]
    assert any(
        pair(row) == pair(ordinary["core-after"])
        and row["path"] == ordinary["core-after"]["path"]
        for row in windows_archives
    )
    assert any(
        pair(row) == pair(capi["carrier-after"])
        and row["path"] == capi["carrier-after"]["path"]
        for row in windows_archives
    )
    expected = {
        "core": ordinary["core-after"],
        "carrier": capi["carrier-after"],
        "ordinary": ordinary["executable-after"],
        "capi": capi["executable-after"],
    }
    assert {row["role"] for row in retained["files"]} == set(expected)
    for row in retained["files"]:
        assert (
            pair(row) == pair(expected[row["role"]])
            and row["source"] == expected[row["role"]]["path"]
        )
        if os.name == "nt":
            assert identity(Path(row["retained"])) == pair(row)
    return {
        "state": "verified",
        "source-files": 25097,
        "owning-inputs": 3838,
        "native-observations": observations,
        "full-process-objective-complete": False,
        "frontier": "NtOpenFile (0x33)",
        "native-retained-bytes-verified": os.name == "nt",
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--directory", type=Path, default=HERE)
    parser.add_argument("--source-root", type=Path, default=ROOT)
    args = parser.parse_args()
    print(json.dumps(audit(args.directory, args.source_root), indent=2))
