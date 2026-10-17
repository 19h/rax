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
  if (index >= 9)
    return 3;
  service = reinterpret_cast<SetInformation>(GetProcAddress(
      GetModuleHandleW(L"ntdll.dll"), "NtSetInformationProcess"));
  if (!service)
    return 4;
  unsigned length = index % 3;
  int delta = int(index / 3) - 1;
  unsigned scenario = index;
  Table a{};
  a.base = 0x40000000000ULL;
  a.maximum = a.base;
  Table b = a;
  b.base = uint64_t(int64_t(a.base) + delta);
  b.maximum = b.base + length;
  bool remove_only = false;
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
