#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <windows.h>
using Open = LONG(NTAPI *)(HANDLE, ACCESS_MASK, BOOLEAN, HANDLE *);
using Rtl = ULONG(NTAPI *)(LONG);
int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto nt = GetModuleHandleW(L"ntdll.dll");
  auto open = Open(GetProcAddress(nt, "NtOpenThreadToken"));
  auto rtl = Rtl(GetProcAddress(nt, "RtlNtStatusToDosError"));
  if (!open || !rtl || !RevertToSelf())
    return 21;
  printf("width=%u rtl-status-no-token-error=%lu\n", unsigned(sizeof(HANDLE)),
         rtl(LONG(0xC000007C)));
  unsigned cases = 0;
  for (unsigned state = 0; state < 2; ++state) {
    if (!RevertToSelf() || (state && !ImpersonateSelf(SecurityImpersonation)))
      return 22;
    HANDLE oracle = nullptr;
    LONG check = open(GetCurrentThread(), TOKEN_QUERY, TRUE, &oracle);
    printf("state=%u oracle-status=%08lX\n", state, ULONG(check));
    if (check >= 0 && !CloseHandle(oracle))
      return 23;
    for (unsigned mode = 0; mode < 2; ++mode)
      for (unsigned negative = 3; negative <= 7; ++negative)
        for (unsigned self = 0; self < 2; ++self) {
          uintptr_t value = mode ? uintptr_t(uint32_t(0) - uint32_t(negative))
                                 : uintptr_t(0) - uintptr_t(negative);
          HANDLE output =
              reinterpret_cast<HANDLE>(uintptr_t(0xA5A5A5A5A5A5A5A5ULL));
          HANDLE before = output;
          LONG status = open(reinterpret_cast<HANDLE>(value), 0x2001C,
                             BOOLEAN(self), &output);
          printf("case=%u state=%u mode=%u negative=%u as-self=%u "
                 "input-thread=%llX status=%08lX output-before=%llX "
                 "output-after=%llX\n",
                 cases++, state, mode, negative, self,
                 (unsigned long long)value, ULONG(status),
                 (unsigned long long)uintptr_t(before),
                 (unsigned long long)uintptr_t(output));
          if (status >= 0 && !CloseHandle(output))
            return 24;
        }
  }
  if (!RevertToSelf())
    return 25;
  printf("complete cases=%u final-impersonation=0\n", cases);
  return 0;
}
