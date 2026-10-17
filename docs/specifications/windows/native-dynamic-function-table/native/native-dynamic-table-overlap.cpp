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
int main(int argc, char **argv) {
  if (argc != 3)
    return 2;
  unsigned index = static_cast<unsigned>(strtoul(argv[2], nullptr, 10));
  if (index >= 56)
    return 3;
  service = reinterpret_cast<SetInformation>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtSetInformationProcess"));
  if (!service)
    return 4;
  constexpr uint64_t lengths[] = {0, 1, 2, 15, 16, 17, 4095, 4096};
  uint64_t length = lengths[index / 7];
  unsigned scenario = index % 7;
  Table a{};
  a.base = 0x40000000000ULL;
  a.maximum = a.base + length;
  Table b = a;
  bool remove_only = scenario == 1 || scenario == 2;
  if (scenario == 1)
    ++b.maximum;
  if (scenario == 2)
    --b.maximum;
  if (scenario == 3) {
    ++b.base;
    ++b.maximum;
  }
  if (scenario == 4) {
    b.base = a.maximum;
    b.maximum = b.base + length;
  }
  if (scenario == 5) {
    --b.base;
  }
  if (scenario == 6) {
    b.base = a.base + length / 2;
    b.maximum = b.base;
  }
  SetLastError(0x4a2e19f7);
  LONG add_a = call(&a, 0), operation = call(&b, remove_only ? 1 : 0),
       remove_b = call(&b, 1), remove_a = call(&a, 1), again = call(&a, 1);
  DWORD error = GetLastError();
  printf(
      "profile=%s index=%u length=%llu scenario=%u A-base=%016llX "
      "A-max=%016llX B-base=%016llX B-max=%016llX add-A=%08lX operation=%08lX "
      "remove-B=%08lX remove-A=%08lX again-A=%08lX last-error=%08lX\n",
      argv[1], index, static_cast<unsigned long long>(length), scenario,
      static_cast<unsigned long long>(a.base),
      static_cast<unsigned long long>(a.maximum),
      static_cast<unsigned long long>(b.base),
      static_cast<unsigned long long>(b.maximum),
      static_cast<unsigned long>(add_a), static_cast<unsigned long>(operation),
      static_cast<unsigned long>(remove_b),
      static_cast<unsigned long>(remove_a), static_cast<unsigned long>(again),
      error);
  return 0;
}
