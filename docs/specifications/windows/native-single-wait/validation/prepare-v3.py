from pathlib import Path
import hashlib
import json
import subprocess

N = Path(__file__).resolve().parent
S = Path('/Users/int/Downloads/assist-native-allocate-ex-transfer-20261010-2154')
source = (N / 'native-wait-single-v2.cpp').read_text()
added = r'''  size_t multiple_apcs = 0;
  for (unsigned mask = 0; mask != 4; ++mask)
    for (ULONG all : {0ul, 1ul, 256ul})
      for (ULONG alertable : alerts) {
        HANDLE events[] = {CreateEventW(nullptr, FALSE, (mask & 1) != 0, nullptr), CreateEventW(nullptr, FALSE, (mask & 2) != 0, nullptr)};
        if (!events[0] || !events[1]) return 50;
        apc_calls = 0;
        if (!QueueUserAPC(apc, GetCurrentThread(), 0)) return 51;
        DWORD status = WaitForMultipleObjectsEx(2, events, BOOL(all), 0, BOOL(alertable));
        unsigned during = apc_calls;
        LONG drained = test_alert();
        LARGE_INTEGER zero{}; ULONG escaped = 0;
        LONG next_a = invoke(events[0], 0, &zero, &escaped);
        LONG next_b = invoke(events[1], 0, &zero, &escaped);
        printf("multiple-apc case=%zu mask=%u all=%08lX alertable=%08lX status=%08lX during=%u after-drain=%u drained=%08lX next-a=%08lX next-b=%08lX escaped=%08lX\n", multiple_apcs++, mask, all, alertable, status, during, apc_calls, ULONG(drained), ULONG(next_a), ULONG(next_b), escaped);
        if (!CloseHandle(events[0]) || !CloseHandle(events[1])) return 52;
      }
  size_t sleep_apcs = 0;
  for (ULONG alertable : alerts) {
    apc_calls = 0;
    if (!QueueUserAPC(apc, GetCurrentThread(), 0)) return 53;
    DWORD status = SleepEx(0, BOOL(alertable));
    unsigned during = apc_calls;
    LONG drained = test_alert();
    printf("sleep-apc case=%zu alertable=%08lX status=%08lX during=%u after-drain=%u drained=%08lX\n", sleep_apcs++, alertable, status, during, apc_calls, ULONG(drained));
  }
'''
needle = '  if (!GetProcessHandleCount(GetCurrentProcess(), &after))'
assert source.count(needle) == 1
source = source.replace(needle, added + needle)
first = source.rfind('  printf(')
last = source.index('  return before == after', first)
source = source[:first] + r'''  printf("done waits=%zu apcs=%zu win32-apcs=%zu times=%zu multiple-apcs=%zu sleep-apcs=%zu before=%lu after=%lu\n", cases, apcs, win32_apcs, times, multiple_apcs, sleep_apcs, before, after);
''' + source[last:]
path = N / 'native-wait-single-v3.cpp'
if path.exists():
    raise RuntimeError('revision3 source already exists')
path.write_text(source)
subprocess.run(['clang-format', '-i', str(path)], check=True)
raw = path.read_bytes()
sha = hashlib.sha256(raw).hexdigest()
(S / path.name).write_bytes(raw)
old = json.loads((N / 'probe-v2-source.json').read_text())['sha256']
ps = (N / 'native-wait-single-v2.ps1').read_text().replace('native-wait-single-v2', 'native-wait-single-v3').replace(old, sha)
driver = (N / 'run-native-wait-single-v2.py').read_text().replace('native-wait-single-v2', 'native-wait-single-v3')
for root in [N, S]:
    (root / 'native-wait-single-v3.ps1').write_text(ps)
    (root / 'run-native-wait-single-v3.py').write_text(driver)
(N / 'probe-v3-source.json').write_text(json.dumps({'source': path.name, 'sha256': sha, 'bytes': len(raw),
    'scope': 'Retain560 single fault,14 native APC,14 Win32Single APC,16 timeout rows; add84 Win32Multiple APC and7 SleepEx APC rows per profile. Only process-private objects and own-thread APCs.'}, indent=2) + '\n')
print(sha)
