#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstdlib>
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
unsigned ordinal;
void operation(int delta, uint64_t size, bool remove) {
  Table t{};
  t.base = uint64_t(int64_t(0x40000000000ULL) + delta);
  t.maximum = t.base + size;
  LONG status = call(&t, remove ? 1 : 0);
  printf("operation ordinal=%u delta=%d size=%llu remove=%u status=%08lX\n",
         ordinal++, delta, static_cast<unsigned long long>(size), remove,
         static_cast<unsigned long>(status));
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 3)
    return 2;
  unsigned scenario = static_cast<unsigned>(strtoul(argv[2], nullptr, 10));
  if (scenario >= 26)
    return 3;
  service = reinterpret_cast<SetInformation>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtSetInformationProcess"));
  if (!service)
    return 4;
  unsigned duplicates = scenario % 13;
  bool neighbors = scenario >= 13;
  printf("identity profile=%s scenario=%u duplicates=%u neighbors=%u\n",
         argv[1], scenario, duplicates, neighbors);
  if (neighbors) {
    operation(-4096, 1, false);
    operation(4096, 1, false);
  }
  for (unsigned i = 0; i < duplicates; ++i)
    operation(0, 0, false);
  operation(0, 1, false);
  if (neighbors) {
    operation(-8192, 1, false);
    operation(8192, 1, false);
  }
  for (unsigned i = 0; i < duplicates + 3; ++i) {
    operation(0, 1, true);
    operation(0, 0, true);
  }
  if (neighbors) {
    operation(-4096, 1, true);
    operation(4096, 1, true);
    operation(-8192, 1, true);
    operation(8192, 1, true);
  }
  printf("complete operations=%u\n", ordinal);
  return 0;
}
