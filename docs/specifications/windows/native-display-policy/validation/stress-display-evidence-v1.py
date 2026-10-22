from pathlib import Path
import hashlib, importlib.util, io, json, re, shutil, tarfile

W = Path("/Users/int/.assist-native-display-policy-20261022")
BASE = W / "docs/specifications/windows/native-display-policy"
Q = Path(__file__).parent
record = Q / "display-evidence-stress-v1.json"
assert not record.exists()
path = BASE / "check_native_evidence.py"
spec = importlib.util.spec_from_file_location("display_native", path)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
text = (
    BASE / "native/native-display-policy-20261022-v1-four-views-arm64.log"
).read_text()
results = []


def reject(name, fn, expected):
    try:
        fn()
    except ValueError as error:
        assert str(error) == expected, (name, str(error))
        results.append({"case": name, "result": "rejected", "diagnostic": str(error)})
    else:
        raise AssertionError("accepted corrupt evidence: " + name)


assert len(module.native_rows(text, "arm64")) == 9
reject(
    "all statuses repaired to success",
    lambda: module.native_rows(
        text.replace("status=C0000034", "status=00000000"), "arm64"
    ),
    "native absence status",
)
reject(
    "failed handle became nonzero",
    lambda: module.native_rows(
        text.replace("handle=0000000000000000", "handle=0000000000000001", 1), "arm64"
    ),
    "failed native handle",
)
reject(
    "LastError changed",
    lambda: module.native_rows(
        text.replace("last-error=12345678", "last-error=87654321", 1), "arm64"
    ),
    "native LastError",
)
reject(
    "profile pointer width changed",
    lambda: module.native_rows(
        text.replace("POINTER_BYTES 8", "POINTER_BYTES 4", 1), "arm64"
    ),
    "profile",
)
lines = text.splitlines()
lines[2] = lines[1]
reject(
    "duplicate native case",
    lambda: module.native_rows("\n".join(lines), "arm64"),
    "duplicate native case",
)
leak = re.sub(
    r"handles-after=(\d+)", lambda m: "handles-after=" + str(int(m[1]) + 1), text
)
reject(
    "handle count leak", lambda: module.native_rows(leak, "arm64"), "native handle leak"
)
reject(
    "attribute matrix changed",
    lambda: module.native_rows(
        text.replace("flags=00000240", "flags=00000300"), "arm64"
    ),
    "native matrix",
)
module.check_source(BASE, None)
fixture = W / "target/proof-stress-v1"
assert not fixture.exists()
v = fixture / "validation"
v.mkdir(parents=True)
frame = module.load(BASE / "validation/display-final-compiled-overrides-v1.json")
shutil.copyfile(
    BASE / "validation" / frame["source-ledger"], v / frame["source-ledger"]
)
archive = v / frame["archive"]
with tarfile.open(BASE / "validation" / frame["archive"]) as source, tarfile.open(
    archive, "w:gz"
) as destination:
    for i, member in enumerate(source.getmembers()):
        raw = source.extractfile(member).read()
        if i == 0:
            raw += b"\x00"
        item = tarfile.TarInfo(member.name)
        item.size = len(raw)
        destination.addfile(item, io.BytesIO(raw))
frame.update(module.identity(archive))
(v / "display-final-compiled-overrides-v1.json").write_text(json.dumps(frame))
reject(
    "compiler archive member changed with repaired archive hash",
    lambda: module.check_source(fixture, None),
    "compiler member bytes",
)
shutil.rmtree(fixture)
assert len(module.native_rows(text, "arm64")) == 9
module.check_source(BASE, None)
record.write_text(
    json.dumps(
        {
            "state": "finished",
            "checker-sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "negative-cases": results,
            "fresh-positive": "passed",
            "temporary-fixture-removed": True,
        },
        indent=2,
    )
    + "\n"
)
print("rejected", len(results), "corrupt cases; fresh positive passed")
