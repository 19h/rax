"""Audit native CurrentEL observations and exact captured identities."""

from pathlib import Path
import argparse
import hashlib
import json
import re


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(condition, detail):
    if not condition:
        raise ValueError(detail)


def check(directory, live_native=False, source=None):
    def read(name):
        return json.loads((directory / name).read_text())

    def stored(row, name=None):
        name = name or Path(row["path"].replace("\\", "/")).name
        path = directory / name
        require(path.stat().st_size == row["bytes"], "size: " + name)
        require(sha(path) == row["sha256"], "digest: " + name)

    launch = read("native-current-el-v3-launch.json")
    require(
        launch["state"] == "finished" and launch["exit-code"] == 0,
        "native producer did not finish",
    )
    require(
        [x["name"] for x in launch["steps"]] == ["build", "before", "native", "after"],
        "native stages",
    )
    for step in launch["steps"]:
        require(step["exit-code"] == 0, "native stage failed")
        stored(step)
    for key in ["source", "executable", "modules"]:
        require(
            launch[key + "-before"] == launch[key + "-after"], "changed native " + key
        )
    require(len(launch["modules-before"]) == 8, "native DLL set")
    stored(launch["source-before"])
    rows = (directory / "native-current-el-v3-native.log").read_text().splitlines()
    require(len(rows) == 37, "native row count")
    pattern = re.compile(
        r"row=(\d+) rt=(\d+) insn=([0-9A-F]{8}) words=(\d+) status=([0-9A-F]{8}) value=([0-9A-F]{16}) stable=1 ok=1 code=([0-9A-F,]+)"
    )
    controls = [0xD5381000, 0xD5382000, 0xD5384200, 0xD5384100]
    for index, line in enumerate(rows[:36]):
        match = pattern.fullmatch(line)
        require(match is not None, "native row shape")
        i, rt, insn, words, status, value, code = match.groups()
        i, rt, insn, words, status, value = (
            int(i),
            int(rt),
            int(insn, 16),
            int(words),
            int(status, 16),
            int(value, 16),
        )
        require(i == index and rt == (i if i < 32 else 0), "native row identity")
        expected = 0xD5384240 | i if i < 32 else controls[i - 32]
        require(insn == expected, "native MRS encoding")
        code = [int(x, 16) for x in code.split(",")]

        def mov(dst, src):
            return 0xAA0003E0 | src << 16 | dst

        if 0 < rt < 31:
            scratch = 16 if rt == 17 else 17
            expected_code = [
                mov(scratch, rt),
                insn,
                mov(0, rt),
                mov(rt, scratch),
                0xD65F03C0,
            ]
        else:
            expected_code = [insn] + ([mov(0, 31)] if rt == 31 else []) + [0xD65F03C0]
        require(code == expected_code and words == len(code), "native code sequence")
        if i < 32:
            require(status == 0, "CurrentEL rejected natively")
            require(
                value != 0 and value % 4096 == 0 if rt == 18 else value == 0,
                "native destination value",
            )
        else:
            require(
                status == 0xC000001D and value == 0xFFFFFFFFFFFFFFFF,
                "privileged negative control",
            )
    require(
        rows[-1] == "summary calls=36 allocations=36 releases=36 failures=0",
        "native resource balance",
    )
    live = read("current-el-native-live-audit-v1.json")
    require(
        live["state"] == "verified" and live["exit-code"] == 0,
        "live native identity audit",
    )
    require(
        live["machine"] == "ARM64" and live["windows-version"]["build"] == 29683,
        "observed native platform",
    )
    require(
        len(live["native-producer-live-identities"]) == 10,
        "live source/exe/DLL identities",
    )
    require(
        len(live["previous-owning-retained-identities"]) == 4,
        "previous owning retention",
    )
    for key in ["native-launch", "native-log", "driver"]:
        stored(live[key])
    require(
        live["native-producer-live-identities"]
        == [
            launch["source-after"],
            launch["executable-after"],
            *launch["modules-after"],
        ],
        "live audit does not match native producer",
    )
    if live_native:
        for row in [
            *live["native-producer-live-identities"],
            *live["previous-owning-retained-identities"],
        ]:
            path = Path(row["path"])
            require(
                path.stat().st_size == row["bytes"] and sha(path) == row["sha256"],
                "live native identity changed",
            )
    red = read("current-el-red-source.json")
    require(red["base"] == "ba278d44871ce08d95a805ac95e80d196b3e3993", "red parent")
    require(
        red["result"]["passed"] == 2
        and red["result"]["failed"] == 1
        and red["result"]["exit-code"] == 101,
        "red result",
    )
    require(
        sha(directory / "current-el-red.log") == red["result"]["log-sha256"],
        "red log digest",
    )
    stored(red["test-source"], "current-el-red-tests.rs")
    binding = read("current-el-red-test-binding.json")
    require(
        binding["unchanged-prefix-in-final"]
        and binding["baseline-sha256"] == red["test-source"]["sha256"],
        "red/final regression binding",
    )
    freeze_path = directory / "current-el-candidate1-source.json"
    require(
        sha(freeze_path)
        == "19a00e62c8665cbbeaa131625af9bd01ddb452f56dd5e7b6c16a5d7600456d49",
        "frozen source ledger",
    )
    freeze = read("current-el-candidate1-source.json")
    require(
        len(freeze["files"]) == 26885 and len(freeze["overrides"]) == 2,
        "source graph extent",
    )
    if source is not None:
        for row in freeze["overrides"]:
            path = source / row["path"]
            require(
                path.stat().st_size == row["bytes"] and sha(path) == row["sha256"],
                "current product source",
            )
        final = (source / red["test-source"]["path"]).read_bytes()
        require(
            final.startswith((directory / "current-el-red-tests.rs").read_bytes()),
            "changed regression prefix",
        )
        require(
            hashlib.sha256(final).hexdigest() == binding["final-sha256"],
            "final regression digest",
        )
    return {
        "state": "verified",
        "native-cases": 36,
        "native-destinations": 32,
        "negative-privileged-controls": 4,
        "previous-owning-artifacts-retained": 4,
        "live-native-files": 14 if live_native else 0,
        "scope": "Native behavior and identity provenance; full-suite, owning-consumer and ordinary-program receipts are separate gates.",
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--directory", type=Path, default=Path(__file__).parent / "validation"
    )
    parser.add_argument("--live-native", action="store_true")
    parser.add_argument("--source", type=Path)
    args = parser.parse_args()
    print(json.dumps(check(args.directory, args.live_native, args.source), indent=2))


if __name__ == "__main__":
    main()
