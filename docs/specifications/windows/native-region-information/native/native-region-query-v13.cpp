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
  ULONG highest = 0;
  GetNumaHighestNodeNumber(&highest);
  printf("profile=%s pointer=%zu parameter=%zu highest-node=%lu\n",
         argc > 1 ? argv[1] : "unknown", sizeof(void *), sizeof(Parameter),
         highest);
  for (uint64_t node :
       {UINT64_MAX, uint64_t(0), uint64_t(1), uint64_t(0xFFFFFFFF)}) {
    Parameter parameter{2, node};
    void *base = nullptr;
    SIZE_T size = 4096;
    LONG status =
        allocate(GetCurrentProcess(), &base, &size, MEM_RESERVE | MEM_COMMIT,
                 PAGE_READWRITE, node == UINT64_MAX ? nullptr : &parameter,
                 node == UINT64_MAX ? 0 : 1);
    printf("allocate node=%llu status=%08lX base=%p size=%zu\n",
           (unsigned long long)node, status, base, size);
    if (status >= 0) {
      region("numa-allocation", base);
      if (!VirtualFree(base, 0, MEM_RELEASE))
        return 2;
    }
  }
  auto base = VirtualAllocExNuma(GetCurrentProcess(), nullptr, 4096,
                                 MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE, 0);
  printf("win32-numa base=%p error=%lu\n", base, GetLastError());
  if (base) {
    region("win32-numa", base);
    if (!VirtualFree(base, 0, MEM_RELEASE))
      return 3;
  }
  printf("finished=1\n");
  return 0;
}
