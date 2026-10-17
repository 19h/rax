"""Query and copy the controlled native primary subject; retain no host handle."""
from pathlib import Path
import ctypes as c
from ctypes import wintypes as w
import datetime, hashlib, json, os, platform

S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
O = S / 'native-access-check-subject.json'
if O.exists():
    raise RuntimeError('subject capture already exists')
k = c.WinDLL('kernel32', use_last_error=True)
a = c.WinDLL('advapi32', use_last_error=True)
k.GetCurrentProcess.restype = w.HANDLE
k.CloseHandle.argtypes = [w.HANDLE]
k.CloseHandle.restype = w.BOOL
a.OpenProcessToken.argtypes = [w.HANDLE, w.DWORD, c.POINTER(w.HANDLE)]
a.OpenProcessToken.restype = w.BOOL
a.GetTokenInformation.argtypes = [w.HANDLE, c.c_int, c.c_void_p, w.DWORD, c.POINTER(w.DWORD)]
a.GetTokenInformation.restype = w.BOOL
h = w.HANDLE()
if not a.OpenProcessToken(k.GetCurrentProcess(), 8, c.byref(h)):
    raise c.WinError(c.get_last_error())

def information(kind):
    n = w.DWORD()
    c.set_last_error(0)
    ok = a.GetTokenInformation(h, kind, None, 0, c.byref(n))
    if ok or c.get_last_error() != 122 or not 0 < n.value <= 65536:
        raise RuntimeError(('invalid size result', kind, ok, c.get_last_error(), n.value))
    buf = c.create_string_buffer(n.value)
    if not a.GetTokenInformation(h, kind, buf, len(buf), c.byref(n)):
        raise c.WinError(c.get_last_error())
    if n.value > len(buf):
        raise RuntimeError('returned bytes exceed capacity')
    return buf, bytes(buf[:n.value])

def sid(buf, data, at):
    width = c.sizeof(c.c_void_p)
    if at + width + 4 > len(data):
        raise RuntimeError('truncated SID_AND_ATTRIBUTES')
    ptr = int.from_bytes(data[at:at+width], 'little')
    off = ptr - c.addressof(buf)
    if off < 0 or off + 8 > len(data) or data[off] != 1 or data[off+1] > 15:
        raise RuntimeError('SID pointer or revision outside capture')
    end = off + 8 + 4 * data[off+1]
    if end > len(data):
        raise RuntimeError('SID escapes capture')
    return {'sid': data[off:end].hex(),
            'attributes': int.from_bytes(data[at+width:at+width+4], 'little')}

def groups(kind):
    buf, data = information(kind)
    count = int.from_bytes(data[:4], 'little')
    width = c.sizeof(c.c_void_p)
    start = (4 + width - 1) // width * width
    stride = (width + 4 + width - 1) // width * width
    if count > 4096 or start + count * stride > len(data):
        raise RuntimeError('invalid group count')
    return [sid(buf, data, start + i * stride) for i in range(count)]

def capture():
    buf, data = information(1)
    user = sid(buf, data, 0)
    _, flag = information(29)
    if len(flag) != 4:
        raise RuntimeError('AppContainer flag is not a DWORD')
    _, privileges = information(3)
    count = int.from_bytes(privileges[:4], 'little')
    if count > 4096 or len(privileges) != 4 + count * 12:
        raise RuntimeError('invalid privilege array')
    return {'user': user, 'groups': groups(2), 'restricted': groups(11),
            'app-container': int.from_bytes(flag, 'little'),
            'privileges': [{'luid': int.from_bytes(privileges[4+i*12:12+i*12], 'little'),
                            'attributes': int.from_bytes(privileges[12+i*12:16+i*12], 'little')}
                           for i in range(count)]}

try:
    first, second = capture(), capture()
    if first != second:
        raise RuntimeError('primary subject changed between captures')
finally:
    if not k.CloseHandle(h):
        raise c.WinError(c.get_last_error())
result = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'pid': os.getpid(), 'machine': platform.machine(), 'os-version': platform.version(),
          'pointer-bytes': c.sizeof(c.c_void_p), 'source-sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
          'token-access': 8, 'closed-handles': 1, 'stable-captures': 2, 'subject': first}
O.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
print(json.dumps(result))
