#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <windows.h>
using Open = LONG(NTAPI *)(HANDLE, ACCESS_MASK, BOOLEAN, HANDLE *);
static LONG invoke(Open f, HANDLE thread, HANDLE *out, ULONG *exception) {
  __try {
    return f(thread, 0x2001C, FALSE, out);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
    return LONG(*exception);
  }
}
int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto f =
      Open(GetProcAddress(GetModuleHandleW(L"ntdll.dll"), "NtOpenThreadToken"));
  if (!f || !RevertToSelf())
    return 21;
  auto base = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 8192, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!base)
    return 22;
  unsigned cases = 0;
  DWORD flags[] = {PAGE_READONLY, PAGE_NOACCESS, PAGE_READWRITE | PAGE_GUARD,
                   PAGE_READONLY | PAGE_GUARD};
  for (unsigned thread = 0; thread < 2; ++thread)
    for (unsigned offset = 4093; offset <= 4095; ++offset)
      for (unsigned page = 0; page < 4; ++page) {
        DWORD old = 0;
        if (!VirtualProtect(base, 8192, PAGE_READWRITE, &old))
          return 23;
        memset(base, 0xA5, 8192);
        if (!VirtualProtect(base + 4096, 4096, flags[page], &old))
          return 24;
        ULONG exception = 0;
        LONG status =
            invoke(f, thread ? nullptr : GetCurrentThread(),
                   reinterpret_cast<HANDLE *>(base + offset), &exception);
        MEMORY_BASIC_INFORMATION q{};
        if (!VirtualQuery(base + 4096, &q, sizeof(q)))
          return 25;
        if (!VirtualProtect(base, 8192, PAGE_READWRITE, &old))
          return 26;
        printf("case=%u thread=%u offset=%u page=%u width=%u status=%08lX "
               "exception=%08lX protection=%lX bytes=",
               cases++, thread, offset, page, unsigned(sizeof(HANDLE)),
               ULONG(status), exception, q.Protect);
        for (unsigned at = 4088; at < 4104; ++at)
          printf("%02X", unsigned(base[at]));
        puts("");
      }
  if (!VirtualFree(base, 0, MEM_RELEASE))
    return 27;
  printf("complete cases=%u\n", cases);
  return 0;
}
