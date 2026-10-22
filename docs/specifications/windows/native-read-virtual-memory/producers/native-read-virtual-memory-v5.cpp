#include <cstdio>
#include <initializer_list>
#include <windows.h>
using Translate = ULONG(NTAPI *)(NTSTATUS);
int main(int argc, char **argv) {
  if (argc != 2)
    return 2;
  auto translate = reinterpret_cast<Translate>(
      GetProcAddress(GetModuleHandleW(L"ntdll.dll"), "RtlNtStatusToDosError"));
  if (!translate)
    return 3;
  std::printf("META profile=%s pointer=%zu\n", argv[1], sizeof(void *));
  unsigned count = 0;
  for (ULONG status : {0ul, 0x8000000Dul, 0x80000001ul, 0xC0000005ul,
                       0xC0000022ul, 0xC0000024ul, 0xC0000008ul}) {
    SetLastError(0x12345678);
    ULONG error = translate(static_cast<NTSTATUS>(status));
    std::printf(
        "{\"id\":%u,\"status\":%lu,\"dos-error\":%lu,\"last-error\":%lu}\n",
        count, status, error, GetLastError());
    ++count;
  }
  std::printf("END calls=%u all-returned=1\n", count);
  return 0;
}
