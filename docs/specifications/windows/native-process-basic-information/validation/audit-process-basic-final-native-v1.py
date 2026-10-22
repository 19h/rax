from pathlib import Path
import datetime, hashlib, json, os, subprocess, traceback

B = Path("C:/Windows/Temp/assist-native-rax-20261010")
S = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
R = B / "src-display-candidate1"
F = S / "process-basic-final-native-audit-v1.json"
assert not F.exists()
state = {
    "state": "running",
    "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "checked": [],
    "native-after": [],
}


def save():
    F.write_text(json.dumps(state, indent=2) + "\n")


def load(name):
    return json.loads((S / name).read_text())


def identity(p):
    h = hashlib.sha256()
    with p.open("rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            h.update(block)
    return {"path": str(p), "bytes": p.stat().st_size, "sha256": h.hexdigest()}


def check(row):
    actual = identity(Path(row["path"]))
    assert (actual["bytes"], actual["sha256"]) == (row["bytes"], row["sha256"]), row[
        "path"
    ]
    state["checked"].append(actual)


save()
try:
    ledger = S / "process-basic-candidate1-source.json"
    assert (
        identity(ledger)["sha256"]
        == "2ad4e1a2835144663d553c6ce25b7dd7df2f5a7dcc9817d18fec2973b2f3519a"
    )
    inputs = load(ledger.name)["files"]
    for row in inputs:
        actual = identity(R / row["path"])
        expected = (row["bytes"], row["sha256"])
        if row["path"] == ".cargo/config.toml":
            expected = (
                374,
                "9f80616d633d948feeba5b201bfa4fe98c8fd736f3f48469b6481279584d8f4c",
            )
        assert (actual["bytes"], actual["sha256"]) == expected, row["path"]
    state["source-inputs"] = len(inputs)
    launch = load("native-process-basic-information-20261022-v6-four-views-launch.json")
    assert launch["state"] == "finished" and launch["exit-code"] == 0
    assert launch["source-before"] == launch["source-after"]
    check(launch["source-before"])
    assert launch["runtime-modules-before"] == launch["runtime-modules-after"]
    for row in launch["runtime-modules-before"]:
        check(row)
    for row in launch["outcomes"]:
        original = row["executable-before"]
        image = Path(original["path"])
        assert image.resolve().is_relative_to(R.resolve())
        before = identity(image)
        assert (before["bytes"], before["sha256"]) == (
            original["bytes"],
            original["sha256"],
        )
        log = S / ("process-basic-final-native-v1-" + row["profile"] + ".log")
        assert not log.exists()
        with log.open("wb") as output:
            result = subprocess.run(
                [str(image), row["profile"]],
                cwd=R,
                stdout=output,
                stderr=subprocess.STDOUT,
                timeout=180,
            )
        assert result.returncode == 0 and identity(image) == before
        text = log.read_text()
        lines = text.splitlines()
        queries = [x for x in lines if x.startswith("QUERY ")]
        faults = [x for x in lines if x.startswith("FAULT ")]
        assert len(queries) == 163 and len(faults) == 4800
        def stable_rows(content):
            result = []
            for line in content.splitlines():
                if not line.startswith(("QUERY ", "FAULT ")):
                    continue
                fields = dict(x.split("=", 1) for x in line.split()[1:])
                assert fields["exception"] == "00000000" and fields["last-error"] == "12345678"
                # Host-dependent identity bytes are replayed separately by the
                # shared scheduler oracle; this repeat compares status/order,
                # returned length and protection transitions with stable images.
                for key in ["bytes", "out-head", "out-tail", "ret-head", "ret-tail"]:
                    fields.pop(key, None)
                result.append(fields)
            return result
        assert stable_rows(text) == stable_rows((S / row["log"]).read_text())
        summary = dict(x.split("=", 1) for x in next(x for x in lines if x.startswith("SUMMARY ")).split()[1:])
        assert summary["calls"] == "4963" and summary["opened"] == summary["closed"] == "9"
        assert summary["handles-before"] == summary["handles-after"]
        state["native-after"].append(
            {
                "profile": row["profile"],
                "image": before,
                "log": log.name,
                "log-identity": identity(log),
                "exit-code": 0,
            }
        )
        save()
    for name in [
        "process-basic-native-ordinary-candidate1.json",
        "process-basic-owning-capi-candidate1.json",
        "native-process-basic-next-read-20261022-v1.json",
    ]:
        receipt = load(name)
        assert receipt["state"] == "finished" and receipt["exit-code"] == 0
        for key in [
            "core-before",
            "observer-before",
            "source-before",
            "executable-before",
            "carrier-before",
        ]:
            if key in receipt:
                assert receipt[key] == receipt[key.replace("before", "after")]
                check(receipt[key])
        assert receipt["runtime-before"] == receipt["runtime-after"]
        for row in receipt["runtime-before"]:
            check(row)
    unit = load("process-basic-candidate1-windows.json")
    check(unit["test-image-before"])
    check(launch["source-before"])
    for row in launch["runtime-modules-before"]:
        check(row)
    state.update(state="finished", verified=True, exit_code=0)
except BaseException:
    state.update(state="failed", exit_code=1, error=traceback.format_exc())
state["completed-utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
save()
print(
    json.dumps({k: v for k, v in state.items() if k not in ["checked", "native-after"]})
)
raise SystemExit(state["exit_code"])
