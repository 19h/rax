"""Reject six independently corrupted copies; write only to temporary storage."""
from pathlib import Path
from importlib.util import module_from_spec, spec_from_file_location
import json
import shutil
import sys
import tempfile

sys.dont_write_bytecode = True
BASE = Path(__file__).resolve().parent
spec = spec_from_file_location("apphelp_evidence", BASE / "verify-evidence.py")
module = module_from_spec(spec)
spec.loader.exec_module(module)
results = []
with tempfile.TemporaryDirectory(prefix="rax-apphelp-evidence-") as temporary:
    root = Path(temporary)
    for name, change in [
        ("producer-count", lambda data: data["producers"].pop()),
        ("kernel-build", lambda data: data["kernel"].__setitem__(2, 1)),
        ("large-address-aware", lambda data: data["producers"][0]["executable"].__setitem__("characteristics", data["producers"][0]["executable"]["characteristics"] ^ 0x20)),
    ]:
        target = root / name
        shutil.copytree(BASE, target)
        path = target / "native-apphelp-identities.json"
        data = json.loads(path.read_text())
        change(data)
        path.write_text(json.dumps(data))
        try:
            module.verify(target)
        except ValueError as error:
            results.append({"case": name, "rejected": True, "reason": str(error)})
        else:
            raise RuntimeError("accepted corrupted evidence: " + name)
    for name, filename, change in [
        ("native-output-byte", "native-apphelp-buffers-arm64.log", lambda data: data.replace("status=00000000", "status=C000000D", 1)),
        ("padding-next-tag", "native-sdb-padding.json", lambda data: data.replace("16434", "16435", 1)),
        ("file-walk-failed", "sdb-file-structure.json", lambda data: data.replace('"validation-error": null', '"validation-error": "truncated"', 1)),
    ]:
        target = root / name
        shutil.copytree(BASE, target)
        path = target / filename
        before = path.read_text()
        after = change(before)
        if before == after:
            raise RuntimeError("corruption did not alter its target: " + name)
        path.write_text(after)
        try:
            module.verify(target)
        except ValueError as error:
            results.append({"case": name, "rejected": True, "reason": str(error)})
        else:
            raise RuntimeError("accepted corrupted evidence: " + name)
print(json.dumps(results, indent=2))
