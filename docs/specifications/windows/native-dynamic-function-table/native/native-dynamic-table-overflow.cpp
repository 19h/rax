#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <initializer_list>
#include <windows.h>
#include <winternl.h>
using SetInformation = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG);
struct Information {
  void *table;
  unsigned char remove;
};
struct Table {
  uint64_t list[2], functions, timestamp, minimum, maximum, base;
};
SetInformation service;
LONG call(Table *table, unsigned char remove) {
  Information info{};
  info.table = table;
  info.remove = remove;
  return service(GetCurrentProcess(), 53, &info, sizeof info);
}
void row(const char *axis, unsigned ordinal, Table a, Table b) {
  SetLastError(0x4a2e19f7);
  LONG add = call(&a, 0), remove = call(&b, 1), cleanup = call(&a, 1);
  DWORD error = GetLastError();
  printf("case axis=%s ordinal=%u base=%016llX maximum=%016llX minimum=%016llX "
         "functions=%016llX remove-base=%016llX remove-maximum=%016llX "
         "add=%08lX remove=%08lX cleanup=%08lX last-error=%08lX\n",
         axis, ordinal, static_cast<unsigned long long>(a.base),
         static_cast<unsigned long long>(a.maximum),
         static_cast<unsigned long long>(a.minimum),
         static_cast<unsigned long long>(a.functions),
         static_cast<unsigned long long>(b.base),
         static_cast<unsigned long long>(b.maximum),
         static_cast<unsigned long>(add), static_cast<unsigned long>(remove),
         static_cast<unsigned long>(cleanup), error);
}
int main(int argc, char **argv) {
  if (argc != 3)
    return 2;
  constexpr uint64_t sizes[] = {0x7ffffffeULL, 0x7fffffffULL, 0x80000000ULL,
                                0x80000001ULL, 0xffffdfffULL, 0xffffe000ULL,
                                0xffffefffULL, 0xfffff000ULL, 0xfffff001ULL,
                                0xffffffefULL, 0xfffffff0ULL, 0xfffffff7ULL,
                                0xfffffffeULL, 0xffffffffULL, 0x100000000ULL};
  unsigned selected = static_cast<unsigned>(strtoul(argv[2], nullptr, 10));
  if (selected >= 15)
    return 3;
  service = reinterpret_cast<SetInformation>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtSetInformationProcess"));
  if (!service)
    return 4;
  printf("identity profile=%s index=%u width=%zu\n", argv[1], selected,
         sizeof(void *));
  Table a{};
  a.base = 0x40000000000ULL;
  a.maximum = a.base + sizes[selected];
  row("size-boundary", selected, a, a);
  return 0;
}
