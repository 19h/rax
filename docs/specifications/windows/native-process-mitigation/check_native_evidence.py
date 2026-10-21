"""Check immutable class52 native controls and the exact compiled overlays."""

from pathlib import Path
import argparse
import hashlib
import json


def need(ok, reason):
    if not ok:
        raise ValueError(reason)


def identity(path):
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            size += len(block)
            digest.update(block)
    return {"bytes": size, "sha256": digest.hexdigest()}


def pair(row):
    return {key: row[key] for key in ("bytes", "sha256")}


def fields(line):
    return dict(part.split("=", 1) for part in line.split()[1:])


def check(directory, source=None, live=False):
    manifest = json.loads((directory / "native-producers.json").read_text())
    need(
        [x["version"] for x in manifest["accepted"]] == [1, 2, 4],
        "accepted producer set",
    )
    live_files = {}
    total = 0
    for producer in manifest["accepted"]:
        native = directory / "native"
        launch_path = native / producer["launch"]
        need(
            identity(launch_path)
            == {"bytes": producer["launch-bytes"], "sha256": producer["launch-sha256"]},
            "launch identity",
        )
        launch = json.loads(launch_path.read_text())
        need(
            launch["state"] == "finished" and launch["exit-code"] == 0,
            "native producer incomplete",
        )
        need(launch["source-before"] == launch["source-after"], "native source changed")
        need(
            identity(native / producer["source"]) == pair(launch["source-before"])
            and launch["source-before"]["sha256"] == producer["source-sha256"],
            "native source identity",
        )
        modules = launch["runtime-modules-after"]
        need(
            modules == launch["runtime-modules-before"] and len(modules) == 8,
            "runtime DLL stability",
        )
        need(
            launch["compiler-environment"]["sha256"]
            == "e6ca43bcdcbd3067b5de05d051259d4d3abf0bfa5283a98daac14fd508a05fad",
            "compiler environment",
        )
        for row in [launch["source-after"], *modules]:
            if row["path"] in live_files:
                need(live_files[row["path"]] == row, "cross-producer runtime stability")
            live_files[row["path"]] = row
        for step in launch["steps"]:
            need(
                step["exit-code"] == 0 and identity(native / step["log"]) == pair(step),
                "native stage",
            )
        need(
            [row["profile"] for row in launch["outcomes"]]
            == ["arm64", "x64", "x86", "x86-laa"],
            "execution views",
        )
        reference = None
        for outcome in launch["outcomes"]:
            profile = outcome["profile"]
            need(
                outcome["exit-code"] == 0
                and outcome["executable-before"] == outcome["executable-after"],
                "executed image stability",
            )
            exe = outcome["executable-after"]
            live_files[exe["path"]] = exe
            prefix = (
                f'native-process-mitigation-v{producer["version"]}-four-views-{profile}'
            )
            before = json.loads((native / (prefix + "-before.log")).read_text())
            after = json.loads((native / (prefix + "-after.log")).read_text())
            need(before == after and pair(before) == pair(exe), "PE image identity")
            need(
                before["machine"]
                == {"arm64": 0xAA64, "x64": 0x8664, "x86": 0x14C, "x86-laa": 0x14C}[
                    profile
                ],
                "PE machine",
            )
            need(profile != "x86-laa" or before["large-address-aware"], "LAA view")
            lines = (native / outcome["log"]).read_text().splitlines()
            rows = [fields(line) for line in lines if line.startswith("QUERY ")]
            need(len(rows) == producer["calls-per-view"], "query matrix extent")
            compared = (
                [row for row in rows if row["policy"] == "11"]
                if producer["version"] == 1
                else rows
            )
            if reference is None:
                reference = compared
            need(compared == reference, "selector11 execution-view semantic parity")
            summary = [fields(line) for line in lines if line.startswith("SUMMARY ")]
            need(
                len(summary) == 1
                and summary[0]["handles-before"] == summary[0]["handles-after"],
                "native handle balance",
            )
            need(
                all(row["exception"] == "00000000" for row in rows),
                "native unhandled exception",
            )
            if producer["version"] == 1:
                keys = {
                    (int(row["policy"]), int(row["length"]), int(row["return-pointer"]))
                    for row in rows
                }
                need(
                    keys
                    == {
                        (p, n, r)
                        for p in range(32)
                        for n in (0, 1, 4, 7, 8, 9, 16)
                        for r in range(2)
                    },
                    "selector/length matrix coverage",
                )
                selected = [row for row in rows if row["policy"] == "11"]
                for row in selected:
                    need(row["returned"] == "BBBBBBBB", "ReturnLength preservation")
                    expected = "00000000" if row["length"] == "8" else "C0000004"
                    need(row["status"] == expected, "selector11 exact-length status")
                    data = bytes.fromhex(row["data"])
                    need(
                        data[:4] == (11).to_bytes(4, "little")
                        and data[8:] == bytes([0xA5]) * 56,
                        "selector/padding preservation",
                    )
                    need(
                        data[4:8]
                        == (bytes(4) if row["length"] == "8" else bytes([0xA5]) * 4),
                        "default guest-profile native flags",
                    )
            else:
                keys = {
                    (int(row["role"]), int(row["case"]), int(row["length"]))
                    for row in rows
                }
                need(
                    keys
                    == {(h, c, n) for h in range(8) for c in range(24) for n in (7, 8)},
                    "handle/fault matrix coverage",
                )
                need(
                    summary[0]["calls"] == "384"
                    and summary[0]["duplicates"] == summary[0]["closed"] == "4"
                    and summary[0]["pages-allocated"]
                    == summary[0]["pages-released"]
                    == "4",
                    "owned resources",
                )

                def row(role, case, length=8):
                    return next(
                        r
                        for r in rows
                        if (int(r["role"]), int(r["case"]), int(r["length"]))
                        == (role, case, length)
                    )

                for role, status in enumerate(
                    (
                        0,
                        0,
                        0,
                        0xC0000022,
                        0xC0000022,
                        0xC0000024,
                        0xC0000008,
                        0xC0000008,
                    )
                ):
                    need(
                        int(row(role, 0)["status"], 16) == status,
                        "handle type/grant result",
                    )
                need(
                    row(0, 18)["status"] == "C0000005"
                    and row(0, 18)["out-protection"] == "00000104",
                    "ReturnLength probe priority",
                )
                need(
                    row(0, 10)["status"] == "80000001"
                    and row(0, 10)["out-protection"] == "00000104"
                    and row(0, 10)["ret-protection"] == "00000004",
                    "two-guard priority",
                )
                need(
                    row(0, 11)["status"] == "80000002"
                    and row(0, 20)["status"] == "80000002",
                    "output alignment priority",
                )
                need(
                    row(6, 2)["status"] == "C0000005"
                    and row(6, 3)["status"] == "80000001",
                    "selector capture precedes handle",
                )
                if producer["version"] == 4:
                    need(
                        row(0, 15)["status"] == "80000001"
                        and row(0, 15)["next-protection"] == "00000002",
                        "read-only flags guard consumption",
                    )
                    need(
                        row(0, 16)["status"] == "80000001"
                        and row(0, 16)["next-protection"] == "00000004",
                        "writable flags guard consumption",
                    )
                    need(
                        row(0, 23)["ret-protection"] == "00000004"
                        and row(0, 23)["next-protection"] == "00000102",
                        "ReturnLength wins over flags guard",
                    )
            total += len(rows)
    need(total == manifest["recorded-calls"] == 4864, "complete recorded-call count")
    if live:
        for path, row in live_files.items():
            need(
                identity(Path(path)) == pair(row), "live native input changed: " + path
            )
    if source is not None:
        ledger = json.loads(
            (
                directory / "validation/process-mitigation-candidate2-source.json"
            ).read_text()
        )
        need(
            len(ledger["files"]) == 26913 and len(ledger["overrides"]) == 14,
            "source overlay extent",
        )
        for row in ledger["overrides"]:
            need(
                identity(source / row["path"]) == pair(row),
                "source overlay: " + row["path"],
            )
        original = (
            directory / "validation/process-mitigation-red-tests.rs"
        ).read_bytes()
        final = (
            source
            / "src/user/windows/process/sched/tests/services_tests/process_mitigation_tests.rs"
        ).read_bytes()
        prefix = json.loads(
            (
                directory / "validation/process-mitigation-red-prefix-binding.json"
            ).read_text()
        )
        need(
            final.startswith(original)
            and hashlib.sha256(original).hexdigest() == prefix["red-test-sha256"],
            "original failing prefix",
        )
    return {
        "state": "verified",
        "recorded-native-queries": total,
        "producers": 3,
        "execution-views": 4,
        "source-overrides": 14 if source is not None else 0,
        "live-native-files": len(live_files) if live else 0,
        "scope": manifest["scope"],
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--directory", type=Path, default=Path(__file__).parent)
    parser.add_argument("--source", type=Path)
    parser.add_argument("--live-native", action="store_true")
    args = parser.parse_args()
    print(json.dumps(check(args.directory, args.source, args.live_native), indent=2))
