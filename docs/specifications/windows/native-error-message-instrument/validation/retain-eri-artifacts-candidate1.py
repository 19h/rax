from pathlib import Path
import hashlib, json, shutil, datetime, subprocess

B = Path("C:/Windows/Temp/assist-native-rax-20261010")
S = Path("C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154")
T = B / "src-eri-candidate1/target/retained-owning-candidate1"
F = S / "eri-retained-owning-candidate1.json"
assert not T.exists() and not F.exists()
T.mkdir(parents=True)


def identity(p):
    h = hashlib.sha256()
    n = 0
    with p.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1048576), b""):
            h.update(chunk)
            n += len(chunk)
    return {"path": str(p), "bytes": n, "sha256": h.hexdigest()}


archives = json.loads(
    (
        S / "native-eri-candidate1-final-owning-archive-identities.json"
    ).read_text()
)
ordinary = json.loads((S / "eri-native-ordinary-candidate1.json").read_text())
capi = json.loads((S / "eri-owning-capi-candidate1.json").read_text())
assert (
    ordinary["state"] == capi["state"] == "finished"
    and ordinary["exit-code"] == capi["exit-code"] == 0
)
rows = []
for role, source, expected in [
    (
        "core",
        Path(
            next(x["path"] for x in archives["archives"] if x["path"].endswith(".rlib"))
        ),
        next(x for x in archives["archives"] if x["path"].endswith(".rlib")),
    ),
    (
        "carrier",
        Path(
            next(x["path"] for x in archives["archives"] if x["path"].endswith(".lib"))
        ),
        next(x for x in archives["archives"] if x["path"].endswith(".lib")),
    ),
    (
        "ordinary",
        Path(ordinary["executable-before"]["path"]),
        ordinary["executable-before"],
    ),
    ("capi", Path(capi["executable-before"]["path"]), capi["executable-before"]),
]:
    before = identity(source)
    assert (before["bytes"], before["sha256"]) == (
        expected["bytes"],
        expected["sha256"],
    )
    target = T / (role + source.suffix)
    # A separate compressed byte copy preserves each artifact while bounding
    # task disk usage; no hardlinks or mutation of the original file.
    target.touch(exist_ok=False)
    compact = subprocess.run(["compact.exe", "/C", "/I", "/Q", str(target)], capture_output=True, timeout=30)
    assert compact.returncode == 0, compact.stdout
    with source.open("rb") as src, target.open("wb") as dst:
        shutil.copyfileobj(src, dst, 1 << 20)
    shutil.copystat(source, target)
    after = identity(source)
    retained = identity(target)
    assert (
        before == after
        and retained["bytes"] == before["bytes"]
        and retained["sha256"] == before["sha256"]
        and not source.samefile(target)
    )
    rows.append(
        {
            "role": role,
            "source": str(source),
            "retained": str(target),
            "bytes": before["bytes"],
            "sha256": before["sha256"],
        }
    )
F.write_text(
    json.dumps(
        {
            "state": "finished",
            "exit-code": 0,
            "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "files": rows,
            "scope": "Four separate retained byte copies of the actual selected owning core, Assist carrier and two exercised ordinary-program callers. Source identities unchanged; no hardlinks.",
        },
        indent=2,
    )
    + "\n"
)
print(json.dumps(rows))
