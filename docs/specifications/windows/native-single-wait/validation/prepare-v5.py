from pathlib import Path
import hashlib
import json
import subprocess

N = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
path = N / 'native-wait-single-v5.cpp'
assert not path.exists(), 'revision5 already prepared'
source = (N / 'native-wait-single-v4.cpp').read_text()
old = '''  auto unbiased = reinterpret_cast<Unbiased>(GetProcAddress(
      GetModuleHandleW(L"kernel32.dll"), "QueryUnbiasedInterruptTimePrecise"));'''
new = '''  auto kernel32_unbiased = reinterpret_cast<Unbiased>(GetProcAddress(
      GetModuleHandleW(L"kernel32.dll"), "QueryUnbiasedInterruptTimePrecise"));
  auto kernelbase_unbiased = reinterpret_cast<Unbiased>(GetProcAddress(
      GetModuleHandleW(L"KernelBase.dll"), "QueryUnbiasedInterruptTimePrecise"));
  auto unbiased = kernel32_unbiased ? kernel32_unbiased : kernelbase_unbiased;
  printf("clock-provider kernel32=%u kernelbase=%u\\n",
         unsigned(kernel32_unbiased != nullptr),
         unsigned(kernelbase_unbiased != nullptr));'''
assert source.count(old) == 1
path.write_text(source.replace(old, new))
subprocess.run(['clang-format', '-i', str(path)], check=True)
raw = path.read_bytes()
sha = hashlib.sha256(raw).hexdigest()
old_sha = json.loads((N / 'probe-v4-source.json').read_text())['sha256']
ps = (N / 'native-wait-single-v4.ps1').read_text().replace('native-wait-single-v4', 'native-wait-single-v5').replace(old_sha, sha)
ps = ps.replace("'C:\\Windows\\SysWOW64\\ntdll.dll')", "'C:\\Windows\\SysWOW64\\ntdll.dll','C:\\Windows\\System32\\KernelBase.dll','C:\\Windows\\SysWOW64\\KernelBase.dll','C:\\Windows\\System32\\kernel32.dll','C:\\Windows\\SysWOW64\\kernel32.dll')")
driver = (N / 'run-native-wait-single-v4.py').read_text().replace('native-wait-single-v4', 'native-wait-single-v5')
for root in [N, S]:
    (root / path.name).write_bytes(raw)
    (root / 'native-wait-single-v5.ps1').write_text(ps)
    (root / 'run-native-wait-single-v5.py').write_text(driver)
(N / 'probe-v5-source.json').write_text(json.dumps({
    'source': path.name, 'sha256': sha, 'bytes': len(raw),
    'scope': 'Retain revision4 matrices and compare precise unbiased clock exports in borrowed kernel32 and KernelBase modules; bind both providers on disk before and after.'
}, indent=2) + '\n')
print(sha)
