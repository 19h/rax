"""Index immutable retained inputs; semantic validation is check_evidence.py."""

from pathlib import Path
import hashlib
import json


def main():
    root = Path(__file__).resolve().parent
    files = [
        path
        for folder in ["native", "evidence"]
        for path in (root / folder).rglob("*")
        if path.is_file()
    ] + [root / "sources.json"]
    rows = []
    for path in sorted(files):
        raw = path.read_bytes()
        rows.append(
            {
                "path": str(path.relative_to(root)).replace("\\", "/"),
                "bytes": len(raw),
                "sha256": hashlib.sha256(raw).hexdigest(),
            }
        )
    proof = {
        "schema": 1,
        "base": "a9ca828fa18ab92a8a3ed866da97f360030b079c",
        "files": rows,
    }
    (root / "proof.json").write_text(
        json.dumps(proof, indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    print(f"Indexed {len(rows)} retained inputs")


if __name__ == "__main__":
    main()
