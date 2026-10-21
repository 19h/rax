from pathlib import Path
import hashlib
import importlib.util
import json
import shutil

Q = Path(__file__).parent
R = Path("/Users/int/.assist-native-current-el-20261021")
D = R / "docs/specifications/windows/native-current-el/validation"
spec = importlib.util.spec_from_file_location(
    "ce_proof", D.parent / "check_current_el_evidence.py"
)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
T = R / "target/current-el-proof-stress-v1"
T.mkdir()


def digest(p):
    return {
        "bytes": p.stat().st_size,
        "sha256": hashlib.sha256(p.read_bytes()).hexdigest(),
    }


def replace_row(text, row, old, new):
    lines = text.splitlines()
    assert old in lines[row]
    lines[row] = lines[row].replace(old, new, 1)
    return "\r\n".join(lines) + "\r\n"


rows = []
for index, (name, row, old, new, expected) in enumerate(
    [
        (
            "wrong-destination-zero",
            16,
            "value=0000000000000000",
            "value=0000000000000001",
            "native destination value",
        ),
        (
            "clobbered-teb",
            18,
            None,
            "value=0000000000000000",
            "native destination value",
        ),
        (
            "privileged-read-admitted",
            32,
            "status=C000001D",
            "status=00000000",
            "privileged negative control",
        ),
        (
            "wrong-system-register",
            16,
            "insn=D5384250",
            "insn=D5384251",
            "native MRS encoding",
        ),
    ]
):
    dst = T / name
    shutil.copytree(D, dst)
    log = dst / "native-current-el-v3-native.log"
    text = log.read_text()
    if old is None:
        old = next(x for x in text.splitlines()[row].split() if x.startswith("value="))
    log.write_bytes(replace_row(text, row, old, new).encode())
    launch_path = dst / "native-current-el-v3-launch.json"
    launch = json.loads(launch_path.read_text())
    step = next(x for x in launch["steps"] if x["name"] == "native")
    step.update(digest(log))
    launch_path.write_text(json.dumps(launch, indent=2) + "\n")
    live_path = dst / "current-el-native-live-audit-v1.json"
    live = json.loads(live_path.read_text())
    live["native-log"].update(digest(log))
    live["native-launch"].update(digest(launch_path))
    live_path.write_text(json.dumps(live, indent=2) + "\n")
    try:
        module.check(dst, False, R)
    except ValueError as e:
        assert str(e) == expected, (name, str(e))
        rows.append(
            {
                "mutation": name,
                "rejected": True,
                "reason": str(e),
                "transitive-log-and-launch-digests-repaired": True,
            }
        )
    else:
        raise AssertionError(name + " incorrectly accepted")
    positive = module.check(D, False, R)
    assert positive["state"] == "verified"
    rows[-1]["fresh-positive-restored"] = True
(Q / "current-el-native-proof-stress-v1.json").write_text(
    json.dumps(
        {
            "state": "verified",
            "negative-cases": len(rows),
            "fresh-positive-cases": len(rows),
            "cases": rows,
        },
        indent=2,
    )
    + "\n"
)
print("verified", len(rows), "semantic corruptions and", len(rows), "fresh positives")
