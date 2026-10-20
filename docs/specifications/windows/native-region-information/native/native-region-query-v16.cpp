#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <windows.h>
using Query = LONG(NTAPI *)(HANDLE, const void *, ULONG, void *, SIZE_T,
                            SIZE_T *);
static Query query;
static void region(const char *name, const void *address) {
  for (ULONG kind : {3u, 7u}) {
    alignas(16) unsigned char bytes[64];
    memset(bytes, 0xa5, sizeof(bytes));
    SIZE_T returned = SIZE_T(-1);
    auto status = query(GetCurrentProcess(), address, kind, bytes,
                        sizeof(bytes), &returned);
    printf(
        "region name=%s address=%p class=%lu status=%08lX returned=%zu bytes=",
        name, address, kind, status, returned);
    for (auto b : bytes)
      printf("%02X", b);
    printf("\n");
  }
}

int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  query = reinterpret_cast<Query>(
      GetProcAddress(GetModuleHandleW(L"ntdll.dll"), "NtQueryVirtualMemory"));
  if (!query)
    return 1;
  printf("profile=%s pointer=%zu\n", argc > 1 ? argv[1] : "unknown",
         sizeof(void *));
  auto memory = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 16384, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!memory)
    return 2;
  for (ULONG kind : {3u, 7u}) {
    for (SIZE_T size = 0; size <= 65; ++size) {
      memset(memory, 0xA5, 4096);
      SIZE_T returned = SIZE_T(-1);
      LONG status = query(GetCurrentProcess(), memory + 8192, kind,
                          memory + 128, size, &returned);
      printf("matrix target=3 class=%lu length=%zu status=%08lX "
             "exception=00000000 returned=%zu bytes=",
             kind, size, status, returned);
      for (unsigned i = 128; i < 192; ++i)
        printf("%02X", memory[i]);
      printf("\n");
    }
  }
  VirtualFree(memory, 0, MEM_RELEASE);
  printf("finished=1\n");
  return 0;
}
