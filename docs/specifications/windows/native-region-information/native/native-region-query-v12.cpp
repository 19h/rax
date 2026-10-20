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

static void pages(const char *stage, unsigned char *view) {
  for (unsigned offset : {0u, 8192u, 12288u}) {
    MEMORY_BASIC_INFORMATION info{};
    VirtualQuery(view + offset, &info, sizeof(info));
    printf("pages stage=%s offset=%u size=%zu state=%08lX protect=%08lX\n",
           stage, offset, info.RegionSize, info.State, info.Protect);
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
  for (DWORD attributes : {SEC_RESERVE, SEC_COMMIT}) {
    for (DWORD access : {FILE_MAP_READ, FILE_MAP_COPY}) {
      auto mapping =
          CreateFileMappingW(INVALID_HANDLE_VALUE, nullptr,
                             PAGE_READWRITE | attributes, 0, 65536, nullptr);
      if (!mapping)
        return 2;
      auto view = static_cast<unsigned char *>(
          MapViewOfFile(mapping, access, 0, 0, 65536));
      if (!view)
        return 3;
      printf("case attributes=%08lX access=%08lX\n", attributes, access);
      region("before", view);
      pages("before", view);
      SetLastError(0);
      auto committed = VirtualAlloc(view + 8192, 4096, MEM_COMMIT,
                                    access == FILE_MAP_COPY ? PAGE_WRITECOPY
                                                            : PAGE_READONLY);
      printf("commit result=%p error=%lu\n", committed, GetLastError());
      region("after-commit", view);
      pages("after-commit", view);
      if (committed && access == FILE_MAP_COPY) {
        region("without-write", view);
        DWORD old;
        if (VirtualProtect(view + 8192, 4096, PAGE_READONLY, &old))
          region("after-readonly", view);
      }
      UnmapViewOfFile(view);
      CloseHandle(mapping);
    }
  }
  printf("finished=1\n");
  return 0;
}
