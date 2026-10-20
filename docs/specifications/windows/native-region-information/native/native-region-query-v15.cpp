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

static LONG invoke(HANDLE process, void *target, void *output, SIZE_T *returned,
                   ULONG *exception) {
  __try {
    return query(process, target, 3, output, 64, returned);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    *exception = GetExceptionCode();
    return LONG(*exception);
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
  wchar_t path[32768];
  if (!GetModuleFileNameW(nullptr, path, 32768))
    return 2;
  auto file =
      CreateFileW(path, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_DELETE,
                  nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
  if (file == INVALID_HANDLE_VALUE)
    return 3;
  auto mapping = CreateFileMappingW(file, nullptr, PAGE_READONLY | SEC_IMAGE, 0,
                                    0, nullptr);
  if (!mapping)
    return 4;
  for (unsigned role = 0; role < 4; ++role) {
    auto view = static_cast<unsigned char *>(
        MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, 0));
    if (!view)
      return 5;
    region("fresh", view);
    DWORD old;
    if (!VirtualProtect(view, 4096, PAGE_READWRITE | PAGE_GUARD, &old))
      return 6;
    region("guarded", view);
    alignas(16) unsigned char output[64] = {};
    SIZE_T returned = 0;
    void *destination = role == 0 || role == 2 ? view : output;
    auto length = role == 1 || role == 2 || role == 3
                      ? reinterpret_cast<SIZE_T *>(view)
                      : &returned;
    ULONG exception = 0;
    LONG status = invoke(role == 3 ? nullptr : GetCurrentProcess(), view,
                         destination, length, &exception);
    MEMORY_BASIC_INFORMATION info{};
    VirtualQuery(view, &info, sizeof(info));
    printf("guard-query role=%u status=%08lX exception=%08lX guard=%u\n", role,
           status, exception, unsigned((info.Protect & PAGE_GUARD) != 0));
    if (!VirtualProtect(view, 4096, PAGE_READONLY, &old))
      return 7;
    region("after-guard-restored", view);
    UnmapViewOfFile(view);
  }
  CloseHandle(mapping);
  CloseHandle(file);
  printf("finished=1\n");
  return 0;
}
