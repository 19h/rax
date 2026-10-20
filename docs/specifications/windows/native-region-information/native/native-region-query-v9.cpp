#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <windows.h>
#include <winternl.h>
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

using Set = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG);
using GetNls = LONG(NTAPI *)(ULONG, ULONG, void *, void **, ULONG *);
using Unmap = LONG(NTAPI *)(HANDLE, void *);
using OpenSection = LONG(NTAPI *)(HANDLE *, ACCESS_MASK, OBJECT_ATTRIBUTES *);
using MapSection = LONG(NTAPI *)(HANDLE, HANDLE, void **, ULONG_PTR, SIZE_T,
                                 LARGE_INTEGER *, SIZE_T *, ULONG, ULONG,
                                 ULONG);
struct Slot {
  ULONG action;
  HANDLE owner;
  void *address;
};
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto n = GetModuleHandleW(L"ntdll.dll");
  query = reinterpret_cast<Query>(GetProcAddress(n, "NtQueryVirtualMemory"));
  auto set_process =
      reinterpret_cast<Set>(GetProcAddress(n, "NtSetInformationProcess"));
  auto set_thread =
      reinterpret_cast<Set>(GetProcAddress(n, "NtSetInformationThread"));
  auto nls = reinterpret_cast<GetNls>(GetProcAddress(n, "NtGetNlsSectionPtr"));
  auto unmap =
      reinterpret_cast<Unmap>(GetProcAddress(n, "NtUnmapViewOfSection"));
  auto open = reinterpret_cast<OpenSection>(GetProcAddress(n, "NtOpenSection"));
  auto map =
      reinterpret_cast<MapSection>(GetProcAddress(n, "NtMapViewOfSection"));
  if (!query || !set_process || !set_thread || !nls || !unmap || !open || !map)
    return 1;
  DWORD handles_before = 0;
  GetProcessHandleCount(GetCurrentProcess(), &handles_before);
  printf("profile=%s pointer=%zu\n", argc > 1 ? argv[1] : "unknown",
         sizeof(void *));
  HANDLE owner = nullptr;
  LONG status = set_process(GetCurrentProcess(), 112, &owner, sizeof(owner));
  printf("scheduler-create status=%08lX owner=%p\n", status, owner);
  if (status < 0 || !owner)
    return 2;
  Slot slot{0, owner, nullptr};
  status = set_thread(GetCurrentThread(), 57, &slot, sizeof(slot));
  printf("scheduler-bind status=%08lX slot=%p\n", status, slot.address);
  if (status < 0 || !slot.address)
    return 3;
  region("scheduler-slot", slot.address);
  MEMORY_BASIC_INFORMATION info{};
  VirtualQuery(slot.address, &info, sizeof(info));
  printf("scheduler-vm allocation=%p region=%zu state=%08lX protect=%08lX "
         "type=%08lX\n",
         info.AllocationBase, info.RegionSize, info.State, info.Protect,
         info.Type);
  slot.action = 1;
  if (set_thread(GetCurrentThread(), 57, &slot, sizeof(slot)) < 0)
    return 4;
  CloseHandle(owner);
  for (ULONG type : {11u, 12u, 14u}) {
    void *address = nullptr;
    ULONG length = 0;
    status = nls(type, type == 11 ? 1252 : 0, nullptr, &address, &length);
    printf("nls type=%lu status=%08lX address=%p length=%lu\n", type, status,
           address, length);
    if (status >= 0 && address) {
      region("nls-file", address);
      if (unmap(GetCurrentProcess(), address) < 0)
        return 5;
    }
  }
  auto mapping = CreateFileMappingW(INVALID_HANDLE_VALUE, nullptr,
                                    PAGE_READWRITE, 0, 16384, nullptr);
  if (!mapping)
    return 6;
  auto view = static_cast<unsigned char *>(
      MapViewOfFile(mapping, FILE_MAP_COPY, 0, 0, 16384));
  if (!view)
    return 7;
  region("pagefile-copy-before", view);
  DWORD old;
  if (!VirtualProtect(view, 16384, PAGE_READONLY, &old))
    return 8;
  region("pagefile-copy-readonly", view);
  if (!VirtualProtect(view, 16384, old, &old))
    return 9;
  view[8192] = view[8192];
  if (!VirtualProtect(view, 16384, PAGE_READONLY, &old))
    return 10;
  region("pagefile-copy-written-readonly", view);
  UnmapViewOfFile(view);
  CloseHandle(mapping);
  wchar_t name[] = L"\\Windows\\SharedSection";
  UNICODE_STRING unicode{USHORT(sizeof(name) - 2), USHORT(sizeof(name)), name};
  OBJECT_ATTRIBUTES attributes{
      sizeof(attributes), nullptr, &unicode, 0x40, nullptr, nullptr};
  HANDLE section = nullptr;
  status = open(&section, 4, &attributes);
  printf("csr-open status=%08lX\n", status);
  if (status >= 0) {
    void *address = nullptr;
    SIZE_T size = 0;
    status = map(section, GetCurrentProcess(), &address, 0, 0, nullptr, &size,
                 1, 0, PAGE_WRITECOPY);
    printf("csr-map status=%08lX address=%p size=%zu\n", status, address, size);
    if (status >= 0) {
      region("csr-copy", address);
      if (unmap(GetCurrentProcess(), address) < 0)
        return 11;
    }
    CloseHandle(section);
  }
  DWORD handles_after = 0;
  GetProcessHandleCount(GetCurrentProcess(), &handles_after);
  printf("finished=1 handle-count-before=%lu handle-count-after=%lu\n",
         handles_before, handles_after);
  return 0;
}
