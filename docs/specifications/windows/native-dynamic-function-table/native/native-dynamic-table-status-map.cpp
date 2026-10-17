#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <initializer_list>
#include <windows.h>

using Map = ULONG(NTAPI *)(LONG);
int main(int argc, char **argv) {
  if (argc != 2)
    return 2;
  auto ntdll = GetModuleHandleW(L"ntdll.dll");
  for (const char *name :
       {"RtlNtStatusToDosError", "RtlNtStatusToDosErrorNoTeb"}) {
    auto map = reinterpret_cast<Map>(GetProcAddress(ntdll, name));
    if (!map)
      return 3;
    for (uint32_t status :
         {0u, 0x80000001u, 0x80000002u, 0xc0000004u, 0xc0000005u, 0xc000000du,
          0xc0000017u, 0xc0000018u, 0xc0000095u, 0xc000009au, 0xc00000ffu}) {
      SetLastError(0x4a2e19f7);
      ULONG error = map(static_cast<LONG>(status));
      DWORD last = GetLastError();
      printf("profile=%s function=%s status=%08lX error=%lu last-error=%08lX\n",
             argv[1], name, static_cast<unsigned long>(status), error, last);
    }
  }
  return 0;
}
