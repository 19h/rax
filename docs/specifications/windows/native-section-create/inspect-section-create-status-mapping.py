from pathlib import Path
import ctypes,json,hashlib
s=Path(__file__).parent;n=ctypes.WinDLL('ntdll');f=n.RtlNtStatusToDosError;f.argtypes=[ctypes.c_uint32];f.restype=ctypes.c_uint32;k=ctypes.WinDLL('kernel32');g=k.GetLargePageMinimum;g.argtypes=[];g.restype=ctypes.c_size_t
rows=[{'ntstatus':hex(x),'win32-error':f(x)} for x in [0xC0000020,0xC0000040,0xC00000F4]];record={'mappings':rows,'large-page-minimum-bytes':g(),'inspector-sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest()};(s/'native-section-create-status-mapping.json').write_text(json.dumps(record,indent=2)+'\n');print(json.dumps(record))
