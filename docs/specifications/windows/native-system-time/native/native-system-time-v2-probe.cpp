#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <windows.h>
#include <winternl.h>
using Query = LONG(NTAPI *)(LARGE_INTEGER *);
using Version = LONG(NTAPI *)(PRTL_OSVERSIONINFOW);
uint64_t now() {
  FILETIME v{};
  GetSystemTimeAsFileTime(&v);
  return (uint64_t(v.dwHighDateTime) << 32) | v.dwLowDateTime;
}
DWORD protection(void *p) {
  MEMORY_BASIC_INFORMATION b{};
  return VirtualQuery(p, &b, sizeof b) == sizeof b ? b.Protect : 0;
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 2;
  auto ntdll = GetModuleHandleW(L"ntdll.dll");
  auto query =
      reinterpret_cast<Query>(GetProcAddress(ntdll, "NtQuerySystemTime"));
  auto version =
      reinterpret_cast<Version>(GetProcAddress(ntdll, "RtlGetVersion"));
  if (!query || !version)
    return 3;
  RTL_OSVERSIONINFOW os{};
  os.dwOSVersionInfoSize = sizeof os;
  if (version(&os) < 0)
    return 4;
  auto base = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 8192, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!base)
    return 5;
  printf("identity profile=%s width=%zu build=%lu.%lu.%lu\n", argv[1],
         sizeof(void *), os.dwMajorVersion, os.dwMinorVersion,
         os.dwBuildNumber);
  unsigned ordinal = 0, successful = 0;
  bool failed = false;
  for (unsigned mode = 0; mode < 10; ++mode)
    for (unsigned offset :
         {0U,  1U,  2U,  3U,  4U,  5U,  6U,    7U,    8U,    9U,
          10U, 11U, 12U, 13U, 14U, 15U, 4092U, 4093U, 4094U, 4095U}) {
      DWORD old{};
      if (!VirtualProtect(base, 8192, PAGE_READWRITE, &old))
        return 6;
      memset(base, 0xA5, 8192);
      DWORD p0 = mode == 1                ? PAGE_READONLY
                 : mode == 2              ? PAGE_NOACCESS
                 : mode == 3 || mode == 9 ? (PAGE_READWRITE | PAGE_GUARD)
                 : mode == 7              ? (PAGE_READONLY | PAGE_GUARD)
                                          : PAGE_READWRITE;
      DWORD p1 = mode == 4                ? PAGE_READONLY
                 : mode == 5              ? PAGE_NOACCESS
                 : mode == 6 || mode == 9 ? (PAGE_READWRITE | PAGE_GUARD)
                 : mode == 8              ? (PAGE_READONLY | PAGE_GUARD)
                                          : PAGE_READWRITE;
      if (!VirtualProtect(base, 4096, p0, &old) ||
          !VirtualProtect(base + 4096, 4096, p1, &old))
        return 7;
      uint64_t before = now();
      LONG status = LONG(0xCCCCCCCC);
      DWORD exception = 0;
      SetLastError(0x4A2E19F7);
      __try {
        status = query(reinterpret_cast<LARGE_INTEGER *>(base + offset));
      } __except (EXCEPTION_EXECUTE_HANDLER) {
        exception = GetExceptionCode();
      }
      DWORD last = GetLastError();
      uint64_t after = now();
      DWORD a0 = protection(base), a1 = protection(base + 4096);
      if (!VirtualProtect(base, 8192, PAGE_READWRITE, &old))
        return 8;
      uint64_t sample{};
      memcpy(&sample, base + offset, 8);
      bool bounded = before <= sample && sample <= after;
      bool outside = true;
      for (unsigned i = 0; i < 8192; ++i)
        if ((i < offset || i >= offset + 8) && base[i] != 0xA5)
          outside = false;
      if (status >= 0 && !exception) {
        ++successful;
        if (!bounded)
          failed = true;
      }
      if (!outside)
        failed = true;
      printf("case ordinal=%u mode=%u offset=%u status=%08lX exception=%08lX "
             "p0=%lX p1=%lX after0=%lX after1=%lX before=%016llX after=%016llX "
             "value=%016llX bounded=%u outside=%u last-error=%08lX bytes=",
             ordinal++, mode, offset, ULONG(status), exception, p0, p1, a0, a1,
             (unsigned long long)before, (unsigned long long)after,
             (unsigned long long)sample, bounded, outside, last);
      for (unsigned i = 0; i < 16; ++i)
        printf("%02X", base[offset + i]);
      puts("");
    }
  const uintptr_t invalid[] = {0, 1, UINTPTR_MAX, UINTPTR_MAX & ~uintptr_t(3),
                               UINTPTR_MAX & ~uintptr_t(4095)};
  for (unsigned mode = 0; mode < 5; ++mode) {
    auto pointer = reinterpret_cast<LARGE_INTEGER *>(invalid[mode]);
    LONG status = LONG(0xCCCCCCCC);
    DWORD exception = 0;
    SetLastError(0x4A2E19F7);
    __try {
      status = query(pointer);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
      exception = GetExceptionCode();
    }
    printf("invalid ordinal=%u mode=%u status=%08lX exception=%08lX "
           "last-error=%08lX\n",
           ordinal++, mode, ULONG(status), exception, GetLastError());
  }
  if (!VirtualFree(base, 0, MEM_RELEASE))
    return 9;
  printf("complete cases=%u successful=%u failed=%u\n", ordinal, successful,
         failed);
  return failed ? 10 : 0;
}
