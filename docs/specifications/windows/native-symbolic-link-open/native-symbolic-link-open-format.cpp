#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <windows.h>
#include <winternl.h>
using Open = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *);
using Close = LONG(NTAPI *)(HANDLE);
using Dos = ULONG(NTAPI *)(LONG);
static Dos dos;
static Open op;
static Close cl;
static unsigned count;
static LONG call(HANDLE *h, ULONG access, OBJECT_ATTRIBUTES *a, ULONG *e) {
  __try {
    return op(h, access, a);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *e = GetExceptionCode();
    return LONG(0xDEADBEEF);
  }
}
static void one(unsigned family, unsigned path, ULONG length, ULONG flags,
                unsigned nk, unsigned bk, unsigned sk, unsigned qk,
                ULONG access) {
  static const wchar_t *paths[] = {L"\\KnownDlls\\KnownDllPath",
                                   L"\\KnownDlls32\\KnownDllPath",
                                   L"\\KnownDlls\\ntdll.dll",
                                   L"\\KnownDlls\\ntdll.dll\\x",
                                   L"\\KnownDlls\\KnownDllPath\\x",
                                   L"\\KnownDlls\\",
                                   L"\\KnownDlls\\\\x",
                                   L"\\\\KnownDlls",
                                   L"\\.\\KnownDlls",
                                   L"\\KnownDlls/ntdll.dll",
                                   L"\\KnownDlls\\..",
                                   L"\\KnownDlls\\.",
                                   L"\\",
                                   L"\\rax-absent-20261012\\x",
                                   L"\\knowndlls"};
  DWORD before = 0, after = 0;
  GetProcessHandleCount(GetCurrentProcess(), &before);
  alignas(8) unsigned char buffer[65536]{}, unicode[32]{}, security[128]{},
      qos[32]{};
  size_t natural = wcslen(paths[path]) * 2;
  memcpy(buffer + bk, paths[path], natural);
  UNICODE_STRING u{};
  u.Length = USHORT(length == 0xFFFFFFFF ? natural : length);
  u.MaximumLength = 0;
  u.Buffer = reinterpret_cast<PWSTR>(buffer + bk);
  memcpy(unicode + nk, &u, sizeof(u));
  SECURITY_QUALITY_OF_SERVICE q{};
  q.Length = sizeof(q);
  q.ImpersonationLevel = SecurityImpersonation;
  memcpy(qos, &q, sizeof(q));
  OBJECT_ATTRIBUTES a{};
  a.Length = sizeof(a);
  a.ObjectName = reinterpret_cast<UNICODE_STRING *>(unicode + nk);
  a.Attributes = flags;
  a.SecurityDescriptor = sk == 0   ? nullptr
                         : sk == 1 ? reinterpret_cast<void *>(1)
                                   : security;
  a.SecurityQualityOfService = qk == 0   ? nullptr
                               : qk == 1 ? reinterpret_cast<void *>(1)
                                         : qos;
  HANDLE h = reinterpret_cast<HANDLE>(uintptr_t(0xA5A5A5A5A5A5A5A5ull));
  ULONG e = 0;
  LONG status = call(&h, access, &a, &e);
  uintptr_t value = uintptr_t(h);
  LONG close = LONG(0xDEADBEEF);
  if (status >= 0 && e == 0 && h)
    close = cl(h);
  GetProcessHandleCount(GetCurrentProcess(), &after);
  printf("case=%u width=%zu family=%u path=%u length=%u flags=%08lX "
         "unicode-offset=%u buffer-offset=%u security=%u qos=%u access=%08lX "
         "status=%08lX dos=%lu exception=%08lX output=%llX close=%08lX "
         "handles-before=%lu handles-after=%lu\n",
         count++, sizeof(void *), family, path, u.Length, flags, nk, bk, sk, qk,
         access, ULONG(status), dos(status), e, (unsigned long long)value,
         ULONG(close), before, after);
  if (before != after)
    ExitProcess(21);
}
int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto n = GetModuleHandleW(L"ntdll.dll");
  op = reinterpret_cast<Open>(GetProcAddress(n, "NtOpenSymbolicLinkObject"));
  dos = reinterpret_cast<Dos>(GetProcAddress(n, "RtlNtStatusToDosError"));
  cl = reinterpret_cast<Close>(GetProcAddress(n, "NtClose"));
  if (!op || !cl)
    return 22;
  for (unsigned path = 0; path < 15; path++)
    for (ULONG flags : {0u, 0x40u, 0x400u, 0x440u, 0x1000u, 0x1040u})
      for (ULONG access : {1u, 0x01000000u, 0x03000000u, 0xFFFFFFFFu})
        one(0, path, 0xFFFFFFFF, flags, 0, 0, 0, 0, access);
  for (ULONG length : {0u, 1u, 2u, 3u, 23u, 24u, 25u, 45u, 46u, 47u, 48u,
                       65530u, 65532u, 65534u, 65535u})
    for (unsigned nk = 0; nk < 4; nk++)
      for (unsigned bk = 0; bk < 4; bk++)
        for (ULONG flags : {0x40u, 1u})
          one(1, 0, length, flags, nk, bk, 0, 0, 1);
  for (unsigned sk = 0; sk < 3; sk++)
    for (unsigned qk = 0; qk < 3; qk++)
      for (ULONG flags : {0x40u, 1u})
        one(2, 0, 0xFFFFFFFF, flags, 0, 0, sk, qk, 3);
  printf("complete cases=%u\n", count);
  return 0;
}
