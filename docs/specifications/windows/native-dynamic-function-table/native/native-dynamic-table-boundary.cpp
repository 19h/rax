#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <vector>
#include <windows.h>
#include <winternl.h>
using SetInformation = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG);
struct Information {
  void *table;
  unsigned char remove;
};
struct Result {
  LONG status;
  DWORD exception;
  DWORD error;
};
SetInformation service;
Result call(HANDLE process, void *data, ULONG length) {
  Result r{LONG(0xcccccccc), 0, 0};
  SetLastError(0x4a2e19f7);
  __try {
    r.status = service(process, 53, data, length);
  } __except (EXCEPTION_EXECUTE_HANDLER) {
    r.exception = GetExceptionCode();
  }
  r.error = GetLastError();
  return r;
}
DWORD protection(void *p) {
  MEMORY_BASIC_INFORMATION b{};
  return VirtualQuery(p, &b, sizeof b) == sizeof b ? b.Protect : 0;
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 2;
  service = reinterpret_cast<SetInformation>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtSetInformationProcess"));
  if (!service)
    return 3;
  auto block = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 8192, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
  if (!block)
    return 4;
  alignas(16) unsigned char safe[256]{};
  unsigned char before[8192];
  std::vector<unsigned> outer, inner;
  for (unsigned x = 0; x < 16; ++x) {
    outer.push_back(x);
    outer.push_back(4080 + x);
    inner.push_back(x);
    inner.push_back(4080 + x);
  }
  for (unsigned x = 16; x <= 160; x += 8)
    if (x != 16)
      inner.push_back(4096 - x);
  inner.push_back(4096);
  printf("identity profile=%s width=%zu outer-size=%zu\n", argv[1],
         sizeof(void *), sizeof(Information));
  unsigned ordinal = 0;
  for (unsigned axis = 0; axis < 2; ++axis)
    for (unsigned mode = 0; mode < 10; ++mode)
      for (unsigned offset : axis ? inner : outer) {
        DWORD old{};
        if (!VirtualProtect(block, 8192, PAGE_READWRITE, &old))
          return 5;
        memset(block, axis ? 0 : 0xa5, 8192);
        void *table = axis ? block + offset : safe;
        Information info{};
        info.table = table;
        void *input = axis ? static_cast<void *>(&info) : block + offset;
        if (!axis)
          memcpy(input, &info, sizeof info);
        memcpy(before, block, 8192);
        DWORD p0 = mode == 1                ? PAGE_NOACCESS
                   : mode == 2              ? PAGE_READONLY
                   : mode == 3 || mode == 9 ? PAGE_READWRITE | PAGE_GUARD
                   : mode == 4              ? PAGE_READONLY | PAGE_GUARD
                                            : PAGE_READWRITE;
        DWORD p1 = mode == 5                ? PAGE_NOACCESS
                   : mode == 6              ? PAGE_READONLY
                   : mode == 7 || mode == 9 ? PAGE_READWRITE | PAGE_GUARD
                   : mode == 8              ? PAGE_READONLY | PAGE_GUARD
                                            : PAGE_READWRITE;
        if (!VirtualProtect(block, 4096, p0, &old) ||
            !VirtualProtect(block + 4096, 4096, p1, &old))
          return 6;
        Result r = call(GetCurrentProcess(), input, sizeof info);
        DWORD a0 = protection(block), a1 = protection(block + 4096);
        if (!VirtualProtect(block, 8192, PAGE_READWRITE, &old))
          return 7;
        bool unchanged = memcmp(block, before, 8192) == 0;
        Information remove{};
        remove.table = table;
        remove.remove = 1;
        Result cleanup = call(GetCurrentProcess(), &remove, sizeof remove);
        printf("case ordinal=%u axis=%u mode=%u offset=%u status=%08lX "
               "exception=%08lX last-error=%08lX p0=%lX p1=%lX after0=%lX "
               "after1=%lX unchanged=%u cleanup=%08lX\n",
               ordinal++, axis, mode, offset,
               static_cast<unsigned long>(r.status), r.exception, r.error, p0,
               p1, a0, a1, unchanged,
               static_cast<unsigned long>(cleanup.status));
      }
  Information info{};
  info.table = safe;
  for (unsigned invalid = 0; invalid < 4; ++invalid)
    for (unsigned length = 0; length <= 33; ++length) {
      HANDLE process = invalid >= 2 ? nullptr : GetCurrentProcess();
      void *input = invalid & 1 ? nullptr : &info;
      Result r = call(process, input, length);
      Information remove{};
      remove.table = safe;
      remove.remove = 1;
      Result cleanup = call(GetCurrentProcess(), &remove, sizeof remove);
      printf("length ordinal=%u invalid=%u length=%u status=%08lX "
             "exception=%08lX last-error=%08lX cleanup=%08lX\n",
             ordinal++, invalid, length, static_cast<unsigned long>(r.status),
             r.exception, r.error, static_cast<unsigned long>(cleanup.status));
    }
  printf("complete cases=%u\n", ordinal);
  VirtualFree(block, 0, MEM_RELEASE);
  return 0;
}
