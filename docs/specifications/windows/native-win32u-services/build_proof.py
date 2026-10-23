"""Build a deterministic inventory of the retained admission evidence."""

from pathlib import Path
import hashlib
import json


def main():
    root = Path(__file__).resolve().parent
    rows = []
    for folder in [root / "native", root / "evidence"]:
        for path in sorted(folder.rglob("*")):
            if path.is_file():
                raw = path.read_bytes()
                rows.append(
                    {
                        "path": path.relative_to(root).as_posix(),
                        "bytes": len(raw),
                        "sha256": hashlib.sha256(raw).hexdigest(),
                    }
                )
    result = {
        "format": 1,
        "source-parent": "2ca3e35c54bcf4b64be63f524505a1a3e6b69b8b",
        "source-frame-sha256": "6140e8121129bbd12c871dac8525834b68aacff0b9484b027054ec0b58e45a62",
        "checker-sha256": hashlib.sha256(
            (root / "check_evidence.py").read_bytes()
        ).hexdigest(),
        "scope": "Selected installed Win32u service identities and explicit unsupported frontier. USER/GDI initialization and whole same-host process-userland goal remain incomplete.",
        "files": rows,
    }
    (root / "proof.json").write_text(
        json.dumps(result, indent=2) + "\n", encoding="utf-8", newline="\n"
    )
    print("indexed", len(rows), "files")


if __name__ == "__main__":
    main()
