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
unsigned selected;
LONG call(Table *table, unsigned char remove) {
  Information info{};
  info.table = table;
  info.remove = remove;
  return service(GetCurrentProcess(), 53, &info, sizeof info);
}
void row(const char *axis, unsigned ordinal, Table a, Table b) {
  if (ordinal != selected)
    return;
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
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 3)
    return 2;
  selected = static_cast<unsigned>(strtoul(argv[2], nullptr, 10));
  if (selected >= 51)
    return 4;
  service = reinterpret_cast<SetInformation>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtSetInformationProcess"));
  if (!service)
    return 3;
  printf("identity profile=%s width=%zu table-bytes=%zu\n", argv[1],
         sizeof(void *), sizeof(Table));
  unsigned ordinal = 0;
  for (uint64_t base :
       {0ULL, 1ULL, 0x10000ULL, 0x100000000ULL, 0x7fff00000000ULL,
        0xfffffffffffffff0ULL, 0xffffffffffffffffULL})
    for (uint64_t length :
         {0ULL, 1ULL, 0x7fffffffULL, 0xffffffffULL, 0x100000000ULL}) {
      Table a{};
      a.base = base;
      a.maximum = base + length;
      row("range", ordinal++, a, a);
    }
  for (uint64_t minimum : {0ULL, 0xfffffULL, 0x100000ULL, 0x100001ULL,
                           0x101001ULL, 0xffffffffffffffffULL}) {
    Table a{};
    a.base = 0x100000;
    a.maximum = 0x101000;
    a.minimum = minimum;
    row("minimum", ordinal++, a, a);
  }
  for (uint64_t pointer : {0ULL, 1ULL, 0xffffffffffffffffULL}) {
    Table a{};
    a.base = 0x100000;
    a.maximum = 0x101000;
    a.functions = pointer;
    row("functions", ordinal++, a, a);
  }
  for (unsigned mutation = 0; mutation < 7; ++mutation) {
    Table a{};
    a.base = 0x100000;
    a.maximum = 0x101000;
    Table b = a;
    if (mutation == 0)
      b.minimum = ~0ULL;
    if (mutation == 1)
      b.functions = ~0ULL;
    if (mutation == 2)
      ++b.base;
    if (mutation == 3)
      ++b.maximum;
    if (mutation == 4) {
      ++b.base;
      ++b.maximum;
    }
    if (mutation == 5)
      b.maximum = b.base - 1;
    if (mutation == 6)
      b.maximum = b.base + 0x100000000ULL;
    row("remove-key", ordinal++, a, b);
  }
  printf("complete cases=1 candidates=%u selected=%u\n", ordinal, selected);
  return 0;
}
