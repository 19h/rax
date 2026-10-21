from pathlib import Path
import hashlib
import importlib.util
import json
import shutil

R = Path("/Users/int/.assist-native-volume-query-20261021")
Q = Path(__file__).parent
D = R / "docs/specifications/windows/native-volume-query"
spec = importlib.util.spec_from_file_location(
    "vq_native", D / "check_native_evidence.py"
)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
T = R / "target/native-proof-stress-v1"
T.mkdir()
rows = []
for name, version, old, new, reason in [
    (
        "wrong-console-device",
        4,
        "out=" + ("A5" * 16) + "50000000",
        "out=" + ("A5" * 16) + "51000000",
        "native console result",
    ),
    (
        "wrong-abi-guard-order",
        4,
        "io-protect=4 out-protect=104",
        "io-protect=104 out-protect=4",
        "ABI guard ordering",
    ),
    (
        "zero-grant-denied",
        5,
        "status=00000000",
        "status=C0000022",
        "zero-grant metadata query",
    ),
    (
        "completion-not-finished",
        7,
        "complete=1",
        "complete=0",
        "native completion prefix",
    ),
]:
    parent = T / name
    shutil.copytree(D, parent)
    V = parent / "validation"
    manifest = json.loads((parent / "native-producers.json").read_text())
    record = next(x for x in manifest["accepted"] if x["version"] == version)
    lp = V / record["launch"]
    launch = json.loads(lp.read_text())
    out = next(x for x in launch["outcomes"] if x["profile"] == "arm64")
    p = V / out["log"]
    lines = p.read_text().splitlines()
    for i, line in enumerate(lines):
        match = (
            ("role=conin " in line and "case=valid " in line and "len=8 " in line)
            if name == "wrong-console-device"
            else (
                (
                    "role=conin " in line
                    and "case=both-guard " in line
                    and "len=8 " in line
                )
                if name == "wrong-abi-guard-order"
                else (
                    (
                        "role=disk " in line
                        and "access=00000000" in line
                        and "len=8 " in line
                    )
                    if name == "zero-grant-denied"
                    else line.startswith("role=")
                )
            )
        )
        if match:
            assert old in line
            lines[i] = line.replace(old, new, 1)
            break
    else:
        raise AssertionError(name)
    p.write_bytes(("\r\n".join(lines) + "\r\n").encode())
    step = next(x for x in launch["steps"] if x["log"] == out["log"])
    step.update(
        bytes=p.stat().st_size, sha256=hashlib.sha256(p.read_bytes()).hexdigest()
    )
    lp.write_text(json.dumps(launch, indent=2) + "\n")
    record.update(
        {
            "launch-bytes": lp.stat().st_size,
            "launch-sha256": hashlib.sha256(lp.read_bytes()).hexdigest(),
        }
    )
    (parent / "native-producers.json").write_text(json.dumps(manifest, indent=2) + "\n")
    try:
        module.check(V, R)
    except ValueError as e:
        assert str(e) == reason, (name, str(e))
        rows.append(
            {
                "mutation": name,
                "rejected": True,
                "reason": str(e),
                "transitive-identities-repaired": True,
            }
        )
    else:
        raise AssertionError(name + " accepted")
    assert module.check(D / "validation", R)["state"] == "verified"
    rows[-1]["fresh-positive-restored"] = True
(Q / "volume-query-native-proof-stress-v1.json").write_text(
    json.dumps(
        {
            "state": "verified",
            "negative-cases": 4,
            "fresh-positive-cases": 4,
            "cases": rows,
        },
        indent=2,
    )
    + "\n"
)
print("four semantic corruptions rejected; four fresh positives verified")
