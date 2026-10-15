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
using QueryObject = LONG(NTAPI *)(HANDLE, ULONG, void *, ULONG, ULONG *);
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
  auto query_object =
      reinterpret_cast<QueryObject>(GetProcAddress(ntdll, "NtQueryObject"));
  auto close = reinterpret_cast<Close>(GetProcAddress(ntdll, "NtClose"));
  if (!open || !query || !map || !unmap || !close)
    fail("installed-exports");
  WCHAR text[] = L"\\Windows\\SharedSection";
  UNICODE_STRING name{};
  name.Length = sizeof(text) - sizeof(WCHAR);
  name.MaximumLength = sizeof(text);
  name.Buffer = text;
  unsigned ordinal = 0;
  unsigned mapped_views = 0;
  for (ULONG attributes : {0u, 0x40u}) {
    for (ULONG access : {0u, 1u, 2u, 4u, 5u, 8u, 16u, 0x20000u, 0x100000u,
                         0xF001Fu, 0x01000000u, 0x02000000u, 0x80000000u,
                         0x40000000u, 0x20000000u, 0x10000000u}) {
      OBJECT_ATTRIBUTES object{};
      object.Length = sizeof(object);
      object.ObjectName = &name;
      object.Attributes = attributes;
      HANDLE section = reinterpret_cast<HANDLE>(uintptr_t(0xA5A5A5A5));
      LONG status = open(&section, access, &object);
      printf("case=%u width=%zu access=%08lX attributes=%08lX open=%08lX "
             "handle=%llX\n",
             ordinal++, sizeof(void *), access, attributes, ULONG(status),
             static_cast<unsigned long long>(uintptr_t(section)));
      if (status < 0)
        continue;
      alignas(16) unsigned char object_basic[56]{};
      ULONG object_returned = 0;
      LONG object_status = query_object(section, 0, object_basic,
                                        sizeof object_basic, &object_returned);
      if (object_status < 0 || object_returned != 56)
        fail("object-basic-information");
      ULONG granted = 0;
      memcpy(&granted, object_basic + 4, sizeof granted);
      printf("grant status=%08lX rights=%08lX returned=%lu\n",
             ULONG(object_status), granted, object_returned);
      for (ULONG information : {0u, 3u}) {
        alignas(16) unsigned char data[64];
        memset(data, 0xA5, sizeof(data));
        SIZE_T returned = SIZE_T(-1);
        LONG result =
            query(section, information, data, sizeof(data), &returned);
        printf("query class=%lu status=%08lX returned=%zu\n", information,
               ULONG(result), returned);
        dump("query-record", data, sizeof(data));
      }
      void *address = nullptr;
      SIZE_T size = 0;
      LONG mapped = map(section, reinterpret_cast<HANDLE>(intptr_t(-1)),
                        &address, 0, 0, nullptr, &size, 2, 0, PAGE_READONLY);
      printf("map status=%08lX address=%llX size=%zu\n", ULONG(mapped),
             static_cast<unsigned long long>(uintptr_t(address)), size);
      if (mapped >= 0) {
        ++mapped_views;
        if (!address || !size)
          fail("invalid-owned-view");
        MEMORY_BASIC_INFORMATION memory{};
        if (!VirtualQuery(address, &memory, sizeof(memory)))
          fail("owned-view-query");
        printf(
            "view base=%llX allocation=%llX bytes=%zu protection=%08lX "
            "type=%08lX\n",
            static_cast<unsigned long long>(uintptr_t(memory.BaseAddress)),
            static_cast<unsigned long long>(uintptr_t(memory.AllocationBase)),
            memory.RegionSize, memory.Protect, memory.Type);
        if (memory.Protect != PAGE_READONLY)
          fail("read-only-view-contract");
        dump("view-prefix", address, std::min<size_t>(128, size));
        size_t offset = 0;
        unsigned regions = 0;
        while (offset < size) {
          MEMORY_BASIC_INFORMATION region{};
          auto current = static_cast<unsigned char *>(address) + offset;
          if (!VirtualQuery(current, &region, sizeof(region)) ||
              !region.RegionSize || ++regions > 256)
            fail("bounded-view-regions");
          size_t within = uintptr_t(current) - uintptr_t(region.BaseAddress);
          size_t span =
              std::min<size_t>(region.RegionSize - within, size - offset);
          printf("region offset=%zu bytes=%zu state=%08lX protection=%08lX "
                 "type=%08lX\n",
                 offset, span, region.State, region.Protect, region.Type);
          if (!span)
            fail("region-progress");
          offset += span;
        }

        LONG released = unmap(reinterpret_cast<HANDLE>(intptr_t(-1)), address);
        printf("unmap status=%08lX\n", ULONG(released));
        if (released < 0)
          fail("owned-view-release");
      }
      LONG closed = close(section);
      printf("close status=%08lX\n", ULONG(closed));
      if (closed < 0)
        fail("owned-handle-close");
    }
  }
  printf("complete cases=%u mapped-views=%u "
         "map-request-protection=PAGE_READONLY\n",
         ordinal, mapped_views);
  return 0;
}
