#define WIN32_LEAN_AND_MEAN
#include <cstdint>
#include <cstdio>
#include <initializer_list>
#include <windows.h>
using Query = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
using GetStatus = LONG(NTAPI *)();
using SetStatus = void(NTAPI *)(LONG);
int main(int argc, char **argv) {
  auto m = GetModuleHandleW(L"ntdll.dll");
  auto q =
      reinterpret_cast<Query>(GetProcAddress(m, "NtQueryInformationToken"));
  auto get =
      reinterpret_cast<GetStatus>(GetProcAddress(m, "RtlGetLastNtStatus"));
  auto set = reinterpret_cast<SetStatus>(
      GetProcAddress(m, "RtlSetLastWin32ErrorAndNtStatusFromNtStatus"));
  if (!q || !get || !set)
    return 2;
  alignas(void *) BYTE output[256];
  for (unsigned role = 0; role < 3; ++role)
    for (ULONG length : {0UL, 28UL, 0xFFFFFFFFUL}) {
      ULONG returned = 0xA5A5A5A5;
      set(LONG(0xC0000034));
      SetLastError(0x7E11ABCD);
      auto status =
          q(reinterpret_cast<HANDLE>(intptr_t(role == 2 ? 0xDEAD : -4)), 1,
            role == 1 ? nullptr : output, length, &returned);
      auto error = GetLastError();
      auto last = get();
      printf("profile=%s role=%u length=%08lX status=%08lX error=%08lX "
             "ntstatus=%08lX returned=%08lX\n",
             argc > 1 ? argv[1] : "unknown", role, length, ULONG(status), error,
             ULONG(last), returned);
    }
  return 0;
}
