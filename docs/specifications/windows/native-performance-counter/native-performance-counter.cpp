#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <windows.h>
#include <winternl.h>

using Query = LONG(NTAPI *)(LARGE_INTEGER *, LARGE_INTEGER *);
static Query query_counter;
static unsigned char *arena;
static unsigned ordinal;
static ULONG escaped_exception;
static constexpr SIZE_T PAGE = 4096;
static constexpr SIZE_T PAGES = 8;
static void fail(const char *operation) {
  printf("failure=%s error=%lu\n", operation, GetLastError());
  ExitProcess(21);
}
static LONG invoke(LARGE_INTEGER *counter, LARGE_INTEGER *frequency) {
  escaped_exception = 0;
  __try {
    return query_counter(counter, frequency);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    escaped_exception = GetExceptionCode();
    return LONG(escaped_exception);
  }
}
static void protect(unsigned page, DWORD value) {
  DWORD old = 0;
  if (!VirtualProtect(arena + page * PAGE, PAGE, value, &old))
    fail("protect");
}
static void reset() {
  for (unsigned page = 0; page < PAGES; ++page)
    protect(page, PAGE_READWRITE);
  memset(arena, 0xA5, PAGE * PAGES);
  protect(1, PAGE_READONLY);
  protect(2, PAGE_NOACCESS);
  protect(3, PAGE_READWRITE | PAGE_GUARD);
  protect(4, PAGE_READONLY | PAGE_GUARD);
  protect(6, PAGE_READONLY);
  protect(7, PAGE_NOACCESS);
}
static uintptr_t pointer(unsigned kind) {
  switch (kind) {
  case 0:
    return uintptr_t(arena + 64);
  case 1:
    return 0;
  case 2:
    return 1;
  case 3:
    return UINTPTR_MAX;
  case 4:
    return uintptr_t(arena + 65);
  case 5:
    return uintptr_t(arena + 68);
  case 6:
    return uintptr_t(arena + PAGE + 64);
  case 7:
    return uintptr_t(arena + 2 * PAGE + 64);
  case 8:
    return uintptr_t(arena + 3 * PAGE + 64);
  case 9:
    return uintptr_t(arena + 4 * PAGE + 64);
  case 10:
    return uintptr_t(arena + PAGE - 4);
  case 11:
    return uintptr_t(arena + 2 * PAGE - 4);
  case 12:
    return uintptr_t(arena + 6 * PAGE - 4);
  case 13:
    return uintptr_t(arena + 8 * PAGE - 4);
  case 14:
    return uintptr_t(arena + PAGE * PAGES);
  default:
    fail("pointer-kind");
    return 0;
  }
}
static void observe(unsigned counter_kind, unsigned frequency_kind,
                    uintptr_t counter, uintptr_t frequency,
                    const char *family) {
  reset();
  LARGE_INTEGER before{}, after{};
  if (!QueryPerformanceCounter(&before))
    fail("public-counter-before");
  LONG status = invoke(reinterpret_cast<LARGE_INTEGER *>(counter),
                       reinterpret_cast<LARGE_INTEGER *>(frequency));
  if (!QueryPerformanceCounter(&after))
    fail("public-counter-after");
  printf("case=%u family=%s counter-kind=%u frequency-kind=%u status=%08lX "
         "escaped=%08lX before=%lld after=%lld",
         ordinal++, family, counter_kind, frequency_kind, ULONG(status),
         escaped_exception, before.QuadPart, after.QuadPart);
  for (unsigned page = 0; page < PAGES; ++page) {
    MEMORY_BASIC_INFORMATION region{};
    if (VirtualQuery(arena + page * PAGE, &region, sizeof(region)) !=
        sizeof(region))
      fail("query-policy");
    printf(" p%u=%lX", page, region.Protect);
  }
  for (unsigned page = 0; page < PAGES; ++page)
    protect(page, PAGE_READWRITE);
  unsigned changed = 0;
  for (unsigned i = 0; i < PAGE * PAGES; ++i) {
    if (arena[i] == 0xA5)
      continue;
    ++changed;
    printf(" b%u=%02X", i, unsigned(arena[i]));
  }
  printf(" changed=%u\n", changed);
}
int main() {
  query_counter = reinterpret_cast<Query>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtQueryPerformanceCounter"));
  if (!query_counter)
    fail("query-export");
  arena = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 16 * PAGE, MEM_RESERVE, PAGE_READWRITE));
  if (!arena || !VirtualAlloc(arena, PAGE * PAGES, MEM_COMMIT, PAGE_READWRITE))
    fail("arena");
  LARGE_INTEGER native{}, frequency{}, public_frequency{};
  LONG status = invoke(&native, &frequency);
  if (status != 0 || escaped_exception || frequency.QuadPart <= 0 ||
      !QueryPerformanceFrequency(&public_frequency) ||
      frequency.QuadPart != public_frequency.QuadPart)
    fail("calibration");
  printf("calibration status=%08lX counter=%lld frequency=%lld "
         "public-frequency=%lld "
         "pointer-bytes=%zu arena=%llX\n",
         ULONG(status), native.QuadPart, frequency.QuadPart,
         public_frequency.QuadPart, sizeof(void *),
         static_cast<unsigned long long>(uintptr_t(arena)));
  for (unsigned counter = 0; counter < 15; ++counter)
    for (unsigned frequency_kind = 0; frequency_kind < 15; ++frequency_kind)
      observe(counter, frequency_kind, pointer(counter),
              pointer(frequency_kind), "matrix");
  for (unsigned delta = 0; delta <= 12; ++delta)
    observe(0, delta, uintptr_t(arena + 64), uintptr_t(arena + 64 + delta),
            "alias");
  for (unsigned delta = 1; delta <= 12; ++delta)
    observe(delta, 0, uintptr_t(arena + 64 + delta), uintptr_t(arena + 64),
            "reverse-alias");
  LARGE_INTEGER previous{}, current{};
  if (invoke(&previous, nullptr) != 0)
    fail("monotonic-initial");
  for (unsigned i = 0; i < 8; ++i) {
    Sleep(1);
    if (invoke(&current, nullptr) != 0 || current.QuadPart < previous.QuadPart)
      fail("monotonic");
    printf("monotonic case=%u previous=%lld current=%lld delta=%lld\n", i,
           previous.QuadPart, current.QuadPart,
           current.QuadPart - previous.QuadPart);
    previous = current;
  }
  if (!VirtualFree(arena, 0, MEM_RELEASE))
    fail("release");
  printf("complete cases=%u monotonic=8 pointer-bytes=%zu\n", ordinal,
         sizeof(void *));
  return 0;
}
