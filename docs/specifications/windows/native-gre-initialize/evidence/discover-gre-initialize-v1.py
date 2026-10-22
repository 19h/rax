"""Read-only initial presence/view discovery; strings are winreg-decoded."""

from pathlib import Path
import json
import winreg

S = Path(r"C:\Mac\Home\Downloads\assist-native-allocate-ex-transfer-20261010-2154")
F = S / "native-gre-initialize-discovery-v1.json"
assert not F.exists()
root = r"Software\Microsoft\Windows NT\CurrentVersion\GRE_Initialize"
rows = []
for view in [0, winreg.KEY_WOW64_64KEY, winreg.KEY_WOW64_32KEY]:
    try:
        with winreg.OpenKey(
            winreg.HKEY_LOCAL_MACHINE, root, 0, winreg.KEY_READ | view
        ) as key:
            children, count, time = winreg.QueryInfoKey(key)
            values = []
            for index in range(count):
                name, data, kind = winreg.EnumValue(key, index)
                values.append(
                    {
                        "name": name,
                        "kind": kind,
                        "winreg-decoded-data": (
                            data.hex() if isinstance(data, bytes) else data
                        ),
                    }
                )
            rows.append(
                {
                    "view": view,
                    "status": 0,
                    "children": children,
                    "values": values,
                    "last-write": time,
                }
            )
    except OSError as error:
        rows.append({"view": view, "status": error.winerror})
F.write_text(
    json.dumps(
        {"state": "finished", "discovery-only": True, "root": root, "views": rows},
        indent=2,
    )
    + "\n"
)
print(json.dumps(rows))
