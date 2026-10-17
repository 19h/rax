#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <windows.h>
#include <winternl.h>
using SetInformation = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG);
struct Information {
  void *table;
  unsigned char remove;
};
SetInformation service;
LONG call(void *table, unsigned char remove, DWORD &exception) {
  Information info{};
  info.table = table;
  info.remove = remove;
  LONG status = LONG(0xcccccccc);
  exception = 0;
  __try {
    status = service(GetCurrentProcess(), 53, &info, sizeof info);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    exception = GetExceptionCode();
  }
  return status;
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 2;
  service = reinterpret_cast<SetInformation>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtSetInformationProcess"));
  if (!service)
    return 3;
  alignas(16) unsigned char a[256]{}, b[256]{};
  printf("identity profile=%s width=%zu\n", argv[1], sizeof(void *));
  for (int byte = -1; byte < 144; ++byte) {
    memset(a, 0, sizeof a);
    memset(b, 0, sizeof b);
    if (byte >= 0)
      b[byte] = 1;
    DWORD ea{}, eb{}, ec{};
    LONG add = call(a, 0, ea);
    LONG remove = call(b, 1, eb);
    LONG cleanup = call(a, 1, ec);
    printf("key byte=%d add=%08lX remove=%08lX cleanup=%08lX "
           "exceptions=%08lX,%08lX,%08lX\n",
           byte, static_cast<unsigned long>(add),
           static_cast<unsigned long>(remove),
           static_cast<unsigned long>(cleanup), ea, eb, ec);
  }
  printf("complete cases=145\n");
  return 0;
}
