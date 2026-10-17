"""Scope and type/access ordering controls; only probe-owned temporary states."""
from pathlib import Path
import ctypes as c
from ctypes import wintypes as w
import datetime, hashlib, json, platform

S = Path('C:/Mac/Home/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
O = S / 'wnf-private-scope-order.json'
if O.exists():
    raise RuntimeError('private controls already recorded')
n, a, k = [c.WinDLL(name, use_last_error=True) for name in ['ntdll', 'advapi32', 'kernel32']]
n.NtCreateWnfStateName.argtypes = [c.POINTER(c.c_uint64), w.ULONG, w.ULONG, c.c_ubyte, c.c_void_p, w.ULONG, c.c_void_p]
n.NtCreateWnfStateName.restype = w.LONG
n.NtQueryWnfStateData.argtypes = [c.POINTER(c.c_uint64), c.c_void_p, c.c_void_p, c.POINTER(w.ULONG), c.c_void_p, c.POINTER(w.ULONG)]
n.NtQueryWnfStateData.restype = w.LONG
n.NtDeleteWnfStateName.argtypes = [c.POINTER(c.c_uint64)]; n.NtDeleteWnfStateName.restype = w.LONG
a.ConvertStringSecurityDescriptorToSecurityDescriptorW.argtypes = [w.LPCWSTR, w.DWORD, c.POINTER(c.c_void_p), c.POINTER(w.ULONG)]
a.ConvertStringSecurityDescriptorToSecurityDescriptorW.restype = w.BOOL
k.LocalFree.argtypes = [c.c_void_p]; k.LocalFree.restype = c.c_void_p
k.GetCurrentProcess.restype = w.HANDLE
k.GetCurrentProcessId.restype = w.DWORD
k.ProcessIdToSessionId.argtypes = [w.DWORD, c.POINTER(w.DWORD)]; k.ProcessIdToSessionId.restype = w.BOOL
k.CloseHandle.argtypes = [w.HANDLE]; k.CloseHandle.restype = w.BOOL
a.OpenProcessToken.argtypes = [w.HANDLE, w.DWORD, c.POINTER(w.HANDLE)]; a.OpenProcessToken.restype = w.BOOL
a.GetTokenInformation.argtypes = [w.HANDLE, c.c_int, c.c_void_p, w.DWORD, c.POINTER(w.DWORD)]; a.GetTokenInformation.restype = w.BOOL
result = {'utc': datetime.datetime.now(datetime.timezone.utc).isoformat(), 'machine': platform.machine(), 'source-sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), 'created': [], 'rows': [], 'cleanup': []}
def save(): O.write_text(json.dumps(result, indent=2) + '\n')
save()
owned, descriptors = [], []
token = w.HANDLE()
try:
    if not a.OpenProcessToken(k.GetCurrentProcess(), 8, c.byref(token)):
        raise c.WinError(c.get_last_error())
    length = w.DWORD()
    a.GetTokenInformation(token, 1, None, 0, c.byref(length))
    user = c.create_string_buffer(length.value)
    if not a.GetTokenInformation(token, 1, user, length, c.byref(length)):
        raise c.WinError(c.get_last_error())
    user_sid = c.cast(user, c.POINTER(c.c_void_p))[0]
    world = c.create_string_buffer(bytes.fromhex('010100000000000100000000'),12)
    session, pid = w.DWORD(), w.DWORD(k.GetCurrentProcessId())
    if not k.ProcessIdToSessionId(pid, c.byref(session)):
        raise c.WinError(c.get_last_error())
    for mask in ['0x10003', '0x10002']:
        descriptor, size = c.c_void_p(), w.ULONG()
        if not a.ConvertStringSecurityDescriptorToSecurityDescriptorW('O:SYG:SYD:(A;;'+mask+';;;WD)', 1, c.byref(descriptor), c.byref(size)):
            raise c.WinError(c.get_last_error())
        descriptors.append(descriptor)
    type_id = c.create_string_buffer(bytes.fromhex('781b344a2c2b4e4d8182838485868788'),16)
    wrong = c.create_string_buffer(bytes.fromhex('781b347a2c2b4e4d8182838485868789'),16)
    scopes = [('null',None),('current-process',-1),('current-thread',-2),('user-sid',user_sid),('world-sid',c.addressof(world)),('session-id',c.addressof(session)),('pid',c.addressof(pid))]
    for data_scope in [0,1,2,4]:
        for denied in [False,True]:
            name = c.c_uint64()
            status = n.NtCreateWnfStateName(c.byref(name),3,data_scope,0,type_id,64,descriptors[int(denied)])
            result['created'].append({'scope':data_scope,'denied':denied,'name':f'{name.value:016x}','status':status & 0xFFFFFFFF}); save()
            if status < 0:
                raise RuntimeError('private state creation failed')
            owned.append(name)
            for type_role, arg_type in [('null',None),('match',type_id),('wrong',wrong)]:
                for scope_role, explicit_scope in scopes:
                    stamp, length = w.ULONG(0xA5A5A5A5), w.ULONG(64)
                    data = c.create_string_buffer(b'\xA5'*64,64)
                    c.set_last_error(0x7E11ABCD)
                    status = n.NtQueryWnfStateData(c.byref(name),arg_type,explicit_scope,c.byref(stamp),data,c.byref(length))
                    result['rows'].append({'scope':data_scope,'denied':denied,'type':type_role,'explicit':scope_role,'status':status & 0xFFFFFFFF,'stamp':stamp.value,'length':length.value,'bytes':data.raw.hex(),'last-error':c.get_last_error()});save()
except BaseException as error:
    result['error'] = str(error);save();raise
finally:
    for name in owned:
        status=n.NtDeleteWnfStateName(c.byref(name));result['cleanup'].append({'kind':'state','name':f'{name.value:016x}','status':status & 0xFFFFFFFF});save()
    for descriptor in descriptors:
        result['cleanup'].append({'kind':'descriptor','ok':not bool(k.LocalFree(descriptor))});save()
    if token.value:
        result['cleanup'].append({'kind':'token','ok':bool(k.CloseHandle(token))});save()
print('private scope/type controls',len(result['rows']),'calls, cleanup records',len(result['cleanup']))
if any((r.get('status',0)!=0) or r.get('ok',True) is not True for r in result['cleanup']):
    raise RuntimeError('owned cleanup failed')
