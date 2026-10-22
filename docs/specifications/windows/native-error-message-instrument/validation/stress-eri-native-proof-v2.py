"""Three rehashed semantic oracle corruptions, each with a restored positive."""

from pathlib import Path
import argparse, datetime, hashlib, importlib.util, json, shutil, sys

sys.dont_write_bytecode = True
p = argparse.ArgumentParser()
p.add_argument("--evidence", type=Path, required=True)
p.add_argument("--work", type=Path, required=True)
p.add_argument("--output", type=Path, required=True)
a = p.parse_args()
assert not a.work.exists() and not a.output.exists()
a.work.mkdir()
source = a.evidence / "check_native_evidence.py"
spec = importlib.util.spec_from_file_location("eri_native_proof", source)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
rows = []
for name, old, new, expected in [
    (
        "wrong-root-status",
        "status=C0000034",
        "status=C0000022",
        "native fixed-root absence",
    ),
    (
        "nonzero-failed-handle",
        "handle=0000000000000000",
        "handle=00000000A5A5A5A5",
        "failure output handle",
    ),
    (
        "changed-last-error",
        "last-error=12345678",
        "last-error=11111111",
        "LastError preservation",
    ),
]:
    d = a.work / name
    d.mkdir()
    shutil.copytree(a.evidence / "native", d / "native")
    before = m.check(d)
    f = d / "native" / (m.PREFIX + "-four-views-launch.json")
    r = json.loads(f.read_bytes())
    for out in r["outcomes"]:
        log = d / "native" / out["log"]
        raw = log.read_bytes()
        assert old.encode() in raw
        log.write_bytes(raw.replace(old.encode(), new.encode()))
        for step in r["steps"]:
            if step["log"] == out["log"]:
                step.update(m.identity(log))
        if "log-sha256" in out:
            out["log-sha256"] = m.identity(log)["sha256"]
    f.write_text(json.dumps(r, indent=2) + "\n")
    try:
        m.check(d)
    except ValueError as error:
        assert str(error) == expected
        reason = str(error)
    else:
        raise AssertionError(name + " accepted")
    for original in (a.evidence / "native").iterdir():
        if original.is_file():
            shutil.copyfile(original, d / "native" / original.name)
    after = m.check(d)
    rows.append(
        {
            "case": name,
            "rejected": True,
            "reason": reason,
            "positive-before": before,
            "fresh-restore": after,
            "scope": "All four execution-view logs changed; stage hashes repaired; original evidence untouched.",
        }
    )
a.output.write_text(
    json.dumps(
        {
            "state": "finished",
            "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "script-sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            "checker-sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "cases": rows,
        },
        indent=2,
    )
    + "\n"
)
print("three semantic proof corruptions rejected, with restored positives")
