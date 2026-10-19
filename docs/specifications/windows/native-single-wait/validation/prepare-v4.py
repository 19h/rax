from pathlib import Path
import hashlib
import json
import subprocess

N = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
source = (N / 'native-wait-single-v3.cpp').read_text()
added = r'''  using Version = LONG(NTAPI *)(OSVERSIONINFOW *);
  using Unbiased = void(WINAPI *)(PULONGLONG);
  auto version = reinterpret_cast<Version>(GetProcAddress(module, "RtlGetVersion"));
  auto unbiased = reinterpret_cast<Unbiased>(GetProcAddress(GetModuleHandleW(L"kernel32.dll"), "QueryUnbiasedInterruptTimePrecise"));
  OSVERSIONINFOW os{}; os.dwOSVersionInfoSize = sizeof os;
  LONG version_status = version ? version(&os) : LONG(0xC0000002);
  ULONGLONG awake = 0; if (unbiased) unbiased(&awake);
  printf("context version-status=%08lX major=%lu minor=%lu build=%lu precise-unbiased=%u awake-100ns=%llu\n", ULONG(version_status), os.dwMajorVersion, os.dwMinorVersion, os.dwBuildNumber, unsigned(unbiased != nullptr), awake);
  size_t object_apcs = 0;
  for (unsigned kind = 0; kind != 12; ++kind)
    for (ULONG alertable : alerts) {
      const unsigned actual_kind = kind < 8 ? kind : kind == 8 ? 2 : kind == 9 ? 3 : kind == 10 ? 0 : 1;
      Object object(actual_kind, open_directory);
      LARGE_INTEGER zero{}; ULONG escaped = 0;
      if (kind >= 8 && kind <= 10 && invoke(object.handle, 0, &zero, &escaped) != 0) return 54;
      if (kind == 11 && !ResetEvent(object.full)) return 55;
      apc_calls = 0;
      if (!QueueUserAPC(apc, GetCurrentThread(), 0)) return 56;
      printf("object-apc case=%zu kind=%u alertable=%08lX", object_apcs++, kind, alertable);
      object.state("before");
      LONG status = invoke(object.handle, alertable, &zero, &escaped);
      unsigned during = apc_calls;
      object.state("after");
      LONG drained = test_alert();
      printf(" status=%08lX during=%u after-drain=%u drained=%08lX escaped=%08lX\n", ULONG(status), during, apc_calls, ULONG(drained), escaped);
    }
'''
needle = '  if (!GetProcessHandleCount(GetCurrentProcess(), &after))'
assert source.count(needle) == 1
source = source.replace(needle, added + needle)
first = source.rfind('  printf(')
last = source.index('  return before == after', first)
source = source[:first] + r'''  printf("done waits=%zu apcs=%zu win32-apcs=%zu times=%zu multiple-apcs=%zu sleep-apcs=%zu object-apcs=%zu before=%lu after=%lu\n", cases, apcs, win32_apcs, times, multiple_apcs, sleep_apcs, object_apcs, before, after);
''' + source[last:]
path = N / 'native-wait-single-v4.cpp'
if path.exists():
    raise RuntimeError('revision4 already prepared')
path.write_text(source)
subprocess.run(['clang-format', '-i', str(path)], check=True)
raw = path.read_bytes()
sha = hashlib.sha256(raw).hexdigest()
(S / path.name).write_bytes(raw)
old = json.loads((N / 'probe-v3-source.json').read_text())['sha256']
ps = (N / 'native-wait-single-v3.ps1').read_text().replace('native-wait-single-v3', 'native-wait-single-v4').replace(old, sha)
driver = (N / 'run-native-wait-single-v3.py').read_text().replace('native-wait-single-v3', 'native-wait-single-v4')
for root in [N, S]:
    (root / 'native-wait-single-v4.ps1').write_text(ps)
    (root / 'run-native-wait-single-v4.py').write_text(driver)
(N / 'probe-v4-source.json').write_text(json.dumps({'source': path.name, 'sha256': sha, 'bytes': len(raw),
    'scope': 'Retain revision3 matrices; add84 native object/APC rows including denied/invalid handles, recursive owned mutant and nonsignaled objects, plus active version/precise-unbiased-clock context.'}, indent=2) + '\n')
print(sha)
