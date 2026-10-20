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

using Allocate = LONG(NTAPI *)(HANDLE, void **, SIZE_T *, ULONG, ULONG, void *,
                               ULONG);
struct alignas(8) Parameter {
  uint64_t type;
  uint64_t value;
};

int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto n = GetModuleHandleW(L"ntdll.dll");
  query = reinterpret_cast<Query>(GetProcAddress(n, "NtQueryVirtualMemory"));
  auto allocate = reinterpret_cast<Allocate>(
      GetProcAddress(n, "NtAllocateVirtualMemoryEx"));
  if (!query || !allocate)
    return 1;
  printf("profile=%s pointer=%zu\n", argc > 1 ? argv[1] : "unknown",
         sizeof(void *));
  Parameter parameter{2, 0};
  for (bool explicit_reservation : {false, true}) {
    void *base = nullptr;
    SIZE_T size = 65536;
    LONG status =
        allocate(GetCurrentProcess(), &base, &size, MEM_RESERVE, PAGE_READWRITE,
                 explicit_reservation ? &parameter : nullptr,
                 explicit_reservation ? 1 : 0);
    printf("reserve explicit=%u status=%08lX base=%p size=%zu\n",
           unsigned(explicit_reservation), status, base, size);
    if (status < 0)
      return 2;
    region("reserved", base);
    void *page = base;
    SIZE_T page_size = 4096;
    status = allocate(GetCurrentProcess(), &page, &page_size, MEM_COMMIT,
                      PAGE_READWRITE, &parameter, 1);
    printf("commit-explicit status=%08lX\n", status);
    region("after-explicit-commit", base);
    if (!VirtualFree(base, 4096, MEM_DECOMMIT))
      return 3;
    region("after-decommit", base);
    if (!VirtualAlloc(base, 4096, MEM_COMMIT, PAGE_READWRITE))
      return 4;
    region("after-ordinary-commit", base);
    if (!VirtualFree(base, 0, MEM_RELEASE))
      return 5;
    auto reused =
        VirtualAlloc(base, 65536, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    if (!reused || reused != base)
      return 6;
    region("reused-ordinary", reused);
    if (!VirtualFree(reused, 0, MEM_RELEASE))
      return 7;
  }
  printf("finished=1\n");
  return 0;
}
