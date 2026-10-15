from pathlib import Path
from decimal import Decimal
import hashlib
import json
import re

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
V = HERE / "validation"


def require(ok, message):
    if not ok:
        raise ValueError(message)


def identity(path, row):
    data = path.read_bytes()
    require(len(data) == row["bytes"] and hashlib.sha256(data).hexdigest() == row["sha256"], str(path))


def period(path):
    matches = re.findall(r"^sampling-control (.*)$", path.read_text(), re.M)
    require(len(matches) == 1, "one native/model timing sample")
    row = {k: int(v) for k, v in (item.split("=") for item in matches[0].split())}
    require(row["before"] <= row["after"] and row["numer"] > 0 and row["denom"] > 0, "timing units")
    require(row["pressure-status"] == 0, "measured pressure available")
    scale = Decimal(row["numer"]) / Decimal(row["denom"]) / Decimal(1000000000)
    lower = (Decimal(row["deadline"]) - Decimal(row["after"])) * scale
    upper = (Decimal(row["deadline"]) - Decimal(row["before"])) * scale
    return row, lower, upper


require(__debug__, "optimized validation rejected")
for row in json.loads((HERE / "references/provenance.json").read_text()):
    identity(HERE / "references" / row["title"], row)
    require(row["organization"] == "Apple" and row["revision"] == "xnu-12377.121.6", "primary tag")
spec = json.loads((HERE / "identity.json").read_text())
identity(HERE / "reclaim-pressure-control.c", {"bytes": spec["source-bytes"], "sha256": spec["source-sha256"]})
comparison = json.loads((HERE / "comparison.json").read_text())
for name in ["native", "emulated"]:
    row, lower, upper = period(HERE / (name + ".log"))
    require(row == comparison[name]["raw"], "raw timing provenance")
    require(lower == Decimal(comparison[name]["sampling-period-lower-seconds"]) and upper == Decimal(comparison[name]["sampling-period-upper-seconds"]), "reproducible interval")
    require(row["pressure"] == 2, "warning baseline")
    require(lower <= (1 if name == "native" else 10) <= upper, "independent red baseline")
row, lower, upper = period(V / "native-current-after.log")
require(row["pressure"] in [1, 2, 4] and lower <= {1: 10, 2: 1, 4: 0}[row["pressure"]] <= upper, "current native selected defaults")
outcome = json.loads((V / "native-current-after-outcome.json").read_text())
require(outcome["exit-code"] == 0 and outcome["source-sha256"] == spec["source-sha256"] and outcome["exe-sha256"] == spec["executable-sha256"], "unchanged actual control")
require(outcome["log-sha256"] == hashlib.sha256((V / "native-current-after.log").read_bytes()).hexdigest(), "actual native stdout")
row, lower, upper = period(V / "emulated-current-after.log")
require(row["pressure"] in [1, 2, 4] and lower <= {1: 10, 2: 1, 4: 0}[row["pressure"]] <= upper, "rebuilt CLI selected pressure period")
run = json.loads((V / "emulated-current-after-outcome.json").read_text())
require(run["exit-code"] == 0 and run["control-sha256"] == spec["executable-sha256"], "unchanged native control executed through new CLI")
require(run["log-sha256"] == hashlib.sha256((V / "emulated-current-after.log").read_bytes()).hexdigest(), "actual emulated stdout")
require(run["source-manifest-sha256"] == hashlib.sha256((V / "reclaim-pressure-sources.json").read_bytes()).hexdigest(), "CLI build input identity")
row, lower, upper = period(V / "emulated-pinned-after.log")
require(row["pressure"] in [1, 2, 4] and lower <= {1: 10, 2: 1, 4: 0}[row["pressure"]] <= upper, "pinned CLI selected period")
pinned = json.loads((V / "emulated-pinned-after-outcome.json").read_text())
require(pinned["exit-code"] == 0 and pinned["toolchain"].startswith("rustc 1.95.0 "), "pinned Rust1.95 runtime")
require(pinned["control-sha256"] == spec["executable-sha256"] and pinned["log-sha256"] == hashlib.sha256((V / "emulated-pinned-after.log").read_bytes()).hexdigest(), "pinned control and actual stdout")
require(pinned["source-manifest-sha256"] == hashlib.sha256((V / "reclaim-pressure-sources.json").read_bytes()).hexdigest(), "pinned CLI exact inputs")
for row in json.loads((V / "owned-source-hashes.json").read_text())["files"]:
    identity(ROOT / row["path"], row)
for row in json.loads((V / "reclaim-pressure-sources.json").read_text())["files"]:
    identity(ROOT / row["path"], row)
print("Verified Apple primary hashes,warning1s/native versus10s/model red baseline,current native period and8537 frozen inputs")
