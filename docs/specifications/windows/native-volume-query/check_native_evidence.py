"""Verify independent native volume-device control identities and behavior."""

from pathlib import Path
import argparse
import hashlib
import json


def need(ok, reason):
    if not ok:
        raise ValueError(reason)


def identity(path):
    raw = path.read_bytes()
    return {"bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}


def check(directory, source=None, live=False):
    manifest = json.loads((directory.parent / "native-producers.json").read_text())
    count = 0
    live_files = {}
    for record in manifest["accepted"]:
        path = directory / record["launch"]
        need(
            identity(path)
            == {"bytes": record["launch-bytes"], "sha256": record["launch-sha256"]},
            "launch identity",
        )
        launch = json.loads(path.read_text())
        need(
            launch["state"] == "finished" and launch["exit-code"] == 0,
            "native launch incomplete",
        )
        need(launch["source-before"] == launch["source-after"], "native source changed")
        need(
            launch["runtime-modules-before"] == launch["runtime-modules-after"]
            and len(launch["runtime-modules-before"]) == 8,
            "native runtime DLL set changed",
        )
        need(
            identity(directory / record["source"])
            == {k: launch["source-before"][k] for k in ["bytes", "sha256"]},
            "native producer source",
        )
        live_files[launch["source-after"]["path"]] = launch["source-after"]
        for row in launch["runtime-modules-after"]:
            if row["path"] in live_files:
                need(live_files[row["path"]] == row, "cross-producer runtime changed")
            live_files[row["path"]] = row
        for step in launch["steps"]:
            need(step["exit-code"] == 0, "native stage failed")
            need(
                identity(directory / step["log"])
                == {k: step[k] for k in ["bytes", "sha256"]},
                "native stage identity",
            )
        need(
            [x["profile"] for x in launch["outcomes"]]
            == ["arm64", "x64", "x86", "x86-laa"],
            "native view set",
        )
        for outcome in launch["outcomes"]:
            need(
                outcome["exit-code"] == 0
                and outcome["executable-before"] == outcome["executable-after"],
                "native executable changed",
            )
            live_files[outcome["executable-after"]["path"]] = outcome[
                "executable-after"
            ]
            lines = (directory / outcome["log"]).read_text().splitlines()
            rows = [
                dict(x.split("=", 1) for x in line.split())
                for line in lines
                if line.startswith("role=")
            ]
            need(len(rows) == record["calls-per-view"], "native case count")
            count += len(rows)
            if record["version"] in [2, 3, 4, 5]:
                for row in rows:
                    need(row["class"] == "4", "native information class")
                    if record["version"] in [3, 4]:
                        need(
                            row["returned"] == "1",
                            "native query raised instead of returning",
                        )
                        for key, size in [
                            ("io", 96),
                            ("out", 96),
                            ("io-edge", 64),
                            ("out-edge", 64),
                        ]:
                            need(
                                len(bytes.fromhex(row[key])) == size,
                                "native memory window",
                            )
                    if (
                        row["role"] == "conin"
                        and row.get("case", "valid") == "valid"
                        and row["len"] == "8"
                    ):
                        need(
                            row["status"] == "00000000"
                            and row["out"][
                                32 if record["version"] in [3, 4] else 0 :
                            ].startswith("5000000000000200"),
                            "native console result",
                        )
                summary = next(x for x in lines if x.startswith("summary "))
                fields = dict(x.split("=", 1) for x in summary.split()[1:])
                need(
                    int(fields["calls"]) == record["calls-per-view"]
                    and fields["owned"] == fields["closed"],
                    "native resource balance",
                )
                if record["version"] == 5:
                    need(
                        lines[-1] == "duplicates opened=33 closed=33",
                        "native duplicate balance",
                    )
                    reduced = [
                        x
                        for x in rows
                        if x["role"]
                        in [
                            "disk",
                            "directory",
                            "nul",
                            "conin",
                            "conout",
                            "pipe-read",
                            "pipe-write",
                        ]
                        and x["access"] == "00000000"
                        and x["len"] == "8"
                    ]
                    need(
                        len(reduced) == 7
                        and all(x["status"] == "00000000" for x in reduced),
                        "zero-grant metadata query",
                    )
            else:
                need(
                    record["version"] == 7
                    and all(
                        x["ok"] == "1"
                        and x["complete"] == "1"
                        and x["bytes"] == "8"
                        and x["error"] == "0"
                        for x in rows
                    ),
                    "native completion prefix",
                )
                need(
                    lines[-1]
                    == "summary calls=8 duplicates=8 closes=8 original-owned=3 original-closed=3 temporary-removed=1 failures=0",
                    "native completion resources",
                )
    need(
        count == manifest["recorded-four-view-calls"] == 3152, "aggregate native calls"
    )
    for profile in ["arm64", "x64", "x86", "x86-laa"]:
        rows = [
            dict(x.split("=", 1) for x in s.split())
            for s in (
                directory / ("native-volume-query-v4-four-views-" + profile + ".log")
            )
            .read_text()
            .splitlines()
            if s.startswith("role=")
        ]
        both = next(
            x
            for x in rows
            if x["role"] == "conin" and x["case"] == "both-guard" and x["len"] == "8"
        )
        need(both["status"] == "80000001", "two-guard native status")
        need(
            (both["io-protect"], both["out-protect"])
            == (("104", "4") if profile.startswith("x86") else ("4", "104")),
            "ABI guard ordering",
        )
    freeze = directory / "volume-query-candidate1-source.json"
    need(
        identity(freeze)["sha256"]
        == "336e8771aaa128fabb49c5e87cd2629285344f0b04f2de7eee78d851ceab44de",
        "source freeze",
    )
    frozen = json.loads(freeze.read_text())
    need(
        len(frozen["files"]) == 26895 and len(frozen["overrides"]) == 12,
        "source extent",
    )
    red = json.loads((directory / "volume-query-red-source.json").read_text())
    need(
        red["base"] == "3661ff6beec3860e29852d94e38ce5f327bdfbab"
        and red["passed"] == 0
        and red["failed"] == 3,
        "red baseline",
    )
    need(
        identity(directory / "volume-query-red.log")["sha256"] == red["log-sha256"],
        "red log",
    )
    if source is not None:
        for row in frozen["overrides"]:
            need(
                identity(source / row["path"])
                == {k: row[k] for k in ["bytes", "sha256"]},
                "current frozen input",
            )
        final = (source / red["test"]["path"]).read_bytes()
        need(
            final.startswith((directory / "volume-query-red-tests.rs").read_bytes()),
            "changed red regression prefix",
        )
    if live:
        for row in live_files.values():
            need(
                identity(Path(row["path"])) == {k: row[k] for k in ["bytes", "sha256"]},
                "live native artifact changed",
            )
    return {
        "state": "verified",
        "native-recorded-calls": count,
        "native-producers": 5,
        "native-views": 4,
        "current-source-inputs": 12 if source else 0,
        "live-native-files": len(live_files) if live else 0,
        "scope": "One ARM64 Windows kernel with native/emulated execution views; reserved IOSB padding is not a universal native byte contract.",
    }


def main():
    p = argparse.ArgumentParser()
    p.add_argument(
        "--directory", type=Path, default=Path(__file__).parent / "validation"
    )
    p.add_argument("--source", type=Path)
    p.add_argument("--live-native", action="store_true")
    a = p.parse_args()
    print(json.dumps(check(a.directory, a.source, a.live_native), indent=2))


if __name__ == "__main__":
    main()
