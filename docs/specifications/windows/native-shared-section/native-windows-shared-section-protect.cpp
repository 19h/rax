#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <windows.h>
#include <winternl.h>

using OpenSection = LONG(NTAPI *)(HANDLE *, ULONG, OBJECT_ATTRIBUTES *);
using QuerySection = LONG(NTAPI *)(HANDLE, ULONG, void *, SIZE_T, SIZE_T *);
using MapSection = LONG(NTAPI *)(HANDLE, HANDLE, void **, ULONG_PTR, SIZE_T,
                                 LARGE_INTEGER *, SIZE_T *, ULONG, ULONG,
                                 ULONG);
using UnmapSection = LONG(NTAPI *)(HANDLE, void *);
using Protect = LONG(NTAPI *)(HANDLE, void **, SIZE_T *, ULONG, ULONG *);
using Close = LONG(NTAPI *)(HANDLE);

static void fail(const char *operation) {
  printf("failure=%s error=%lu\n", operation, GetLastError());
  ExitProcess(21);
}

static void dump(const char *name, const void *pointer, size_t bytes) {
  printf("bytes name=%s size=%zu hex=", name, bytes);
  const auto data = static_cast<const unsigned char *>(pointer);
  for (size_t i = 0; i < bytes; ++i)
    printf("%02X", unsigned(data[i]));
  puts("");
}

int main() {
  setvbuf(stdout, nullptr, _IONBF, 0);
  auto ntdll = GetModuleHandleW(L"ntdll.dll");
  auto open =
      reinterpret_cast<OpenSection>(GetProcAddress(ntdll, "NtOpenSection"));
  auto query =
      reinterpret_cast<QuerySection>(GetProcAddress(ntdll, "NtQuerySection"));
  auto map =
      reinterpret_cast<MapSection>(GetProcAddress(ntdll, "NtMapViewOfSection"));
  auto unmap = reinterpret_cast<UnmapSection>(
      GetProcAddress(ntdll, "NtUnmapViewOfSection"));
  auto protect = reinterpret_cast<Protect>(
      GetProcAddress(ntdll, "NtProtectVirtualMemory"));
  auto close = reinterpret_cast<Close>(GetProcAddress(ntdll, "NtClose"));
  if (!open || !query || !map || !unmap || !close)
    fail("installed-exports");
  WCHAR text[] = L"\\Windows\\SharedSection";
  UNICODE_STRING name{};
  name.Length = sizeof(text) - sizeof(WCHAR);
  name.MaximumLength = sizeof(text);
  name.Buffer = text;

  OBJECT_ATTRIBUTES object{};
  object.Length = sizeof(object);
  object.ObjectName = &name;
  object.Attributes = 0x40;
  HANDLE section = nullptr;
  if (open(&section, 4, &object) < 0 || !section)
    fail("open-read-section");
  unsigned ordinal = 0;
  for (ULONG initial : {2u, 8u}) {
    for (ULONG replacement :
         {1u, 2u, 4u, 8u, 0x10u, 0x20u, 0x40u, 0x80u, 0x102u, 0x202u, 0x402u}) {
      void *address = nullptr;
      SIZE_T size = 0;
      LONG mapped = map(section, reinterpret_cast<HANDLE>(intptr_t(-1)),
                        &address, 0, 0, nullptr, &size, 2, 0, initial);
      if (mapped < 0 || !address || !size)
        fail("map-owned-view");
      void *target = address;
      SIZE_T span = 4096;
      ULONG old = 0xA5A5A5A5;
      LONG status = protect(reinterpret_cast<HANDLE>(intptr_t(-1)), &target,
                            &span, replacement, &old);
      MEMORY_BASIC_INFORMATION info{};
      if (!VirtualQuery(address, &info, sizeof info))
        fail("query-owned-protection");
      printf(
          "case=%u width=%zu initial=%08lX replacement=%08lX status=%08lX "
          "old=%08lX size-out=%zu reported=%08lX allocation-protection=%08lX\n",
          ordinal++, sizeof(void *), initial, replacement, ULONG(status), old,
          span, info.Protect, info.AllocationProtect);
      LONG released = unmap(reinterpret_cast<HANDLE>(intptr_t(-1)), address);
      printf("unmap status=%08lX\n", ULONG(released));
      if (released < 0)
        fail("release-owned-view");
    }
  }
  LONG closed = close(section);
  printf("close status=%08lX\n", ULONG(closed));
  if (closed < 0)
    fail("close-owned-handle");
  printf("complete cases=%u mapped-views=%u\n", ordinal, ordinal);
}
