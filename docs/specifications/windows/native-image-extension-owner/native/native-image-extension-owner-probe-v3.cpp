#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <cwchar>
#include <initializer_list>
#include <windows.h>
#include <winternl.h>
using Open = LONG(NTAPI *)(PHANDLE, ACCESS_MASK, POBJECT_ATTRIBUTES);
using Map = LONG(NTAPI *)(HANDLE, HANDLE, PVOID *, ULONG_PTR, SIZE_T,
                          PLARGE_INTEGER, PSIZE_T, ULONG, ULONG, ULONG);
using Unmap = LONG(NTAPI *)(HANDLE, PVOID);
using Query = LONG(NTAPI *)(HANDLE, PVOID, ULONG, PVOID, SIZE_T, PSIZE_T);
using Protect = LONG(NTAPI *)(HANDLE, PVOID *, PSIZE_T, ULONG, PULONG);
using Close = LONG(NTAPI *)(HANDLE);
using Version = LONG(NTAPI *)(PRTL_OSVERSIONINFOW);
struct Extension {
  ULONG kind, flags;
  ULONG_PTR rva;
  SIZE_T size;
};
static_assert(sizeof(Extension) == (sizeof(void *) == 8 ? 24 : 16));
void hex(const void *p, size_t n) {
  auto b = static_cast<const unsigned char *>(p);
  for (size_t i = 0; i < n; ++i)
    printf("%02X", b[i]);
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  if (argc != 2)
    return 2;
  auto nt = GetModuleHandleW(L"ntdll.dll");
  auto open = reinterpret_cast<Open>(GetProcAddress(nt, "NtOpenSection"));
  auto map = reinterpret_cast<Map>(GetProcAddress(nt, "NtMapViewOfSection"));
  auto unmap =
      reinterpret_cast<Unmap>(GetProcAddress(nt, "NtUnmapViewOfSection"));
  auto query =
      reinterpret_cast<Query>(GetProcAddress(nt, "NtQueryVirtualMemory"));
  auto protect =
      reinterpret_cast<Protect>(GetProcAddress(nt, "NtProtectVirtualMemory"));
  auto close = reinterpret_cast<Close>(GetProcAddress(nt, "NtClose"));
  auto version = reinterpret_cast<Version>(GetProcAddress(nt, "RtlGetVersion"));
  if (!open || !map || !unmap || !query || !protect || !close || !version)
    return 3;
  RTL_OSVERSIONINFOW os{};
  os.dwOSVersionInfoSize = sizeof os;
  if (version(&os) < 0)
    return 4;
  DWORD before{}, after{};
  if (!GetProcessHandleCount(GetCurrentProcess(), &before))
    return 5;
  printf("identity profile=%s width=%zu build=%lu.%lu.%lu\n", argv[1],
         sizeof(void *), os.dwMajorVersion, os.dwMinorVersion,
         os.dwBuildNumber);
  unsigned ordinal = 0, protections = 0, mapped = 0, unmapped = 0, closed = 0;
  bool failed = false;
  const wchar_t *names[] = {L"ntdll.dll", L"kernelbase.dll", L"kernel32.dll"};
  for (unsigned image = 0; image < 3; ++image) {
    wchar_t path[128]{};
    swprintf_s(path, L"\\%s\\%s",
               sizeof(void *) == 4 ? L"KnownDlls32" : L"KnownDlls",
               names[image]);
    UNICODE_STRING name{};
    name.Buffer = path;
    name.Length = USHORT(wcslen(path) * sizeof(wchar_t));
    name.MaximumLength = name.Length + sizeof(wchar_t);
    OBJECT_ATTRIBUTES attributes{};
    attributes.Length = sizeof attributes;
    attributes.ObjectName = &name;
    attributes.Attributes = 0x40;
    HANDLE section{};
    LONG status = open(&section, 5, &attributes);
    if (status < 0) {
      printf("open-failure image=%u status=%08lX\n", image, ULONG(status));
      return 6;
    }
    auto module =
        reinterpret_cast<unsigned char *>(GetModuleHandleW(names[image]));
    auto dos = reinterpret_cast<IMAGE_DOS_HEADER *>(module);
    auto pe = reinterpret_cast<IMAGE_NT_HEADERS *>(module + dos->e_lfanew);
    SIZE_T image_size = pe->OptionalHeader.SizeOfImage;
    for (ULONG flags : {0UL, 0x800000UL, 0x20000000UL, 0x20800000UL})
      for (SIZE_T requested :
           {SIZE_T(0), SIZE_T(4096), image_size, image_size + 4096}) {
        PVOID base{};
        SIZE_T size = requested;
        status = map(section, GetCurrentProcess(), &base, 0, 0, nullptr, &size,
                     2, flags, PAGE_READONLY);
        printf("case ordinal=%u image=%u flags=%08lX requested=%llX "
               "status=%08lX base=%llX size=%llX declared=%llX\n",
               ordinal++, image, flags, (unsigned long long)requested,
               ULONG(status), (unsigned long long)(ULONG_PTR)base,
               (unsigned long long)size, (unsigned long long)image_size);
        if (status < 0)
          continue;
        ++mapped;
        alignas(8) unsigned char image_info[32]{};
        SIZE_T info_returned{};
        LONG info_status = query(GetCurrentProcess(), base, 6, image_info,
                                 sizeof image_info, &info_returned);
        MEMORY_BASIC_INFORMATION basic{};
        SIZE_T basic_returned = VirtualQuery(base, &basic, sizeof basic);
        printf("view image=%u flags=%08lX requested=%llX status=%08lX "
               "returned=%llX bytes=",
               image, flags, (unsigned long long)requested, ULONG(info_status),
               (unsigned long long)info_returned);
        hex(image_info, sizeof(void *) == 8 ? 24 : 12);
        printf(" region-base=%llX allocation=%llX region-size=%llX "
               "protect=%08lX basic-returned=%llX\n",
               (unsigned long long)(ULONG_PTR)basic.BaseAddress,
               (unsigned long long)(ULONG_PTR)basic.AllocationBase,
               (unsigned long long)basic.RegionSize, basic.Protect,
               (unsigned long long)basic_returned);
        PVOID release = base;
        for (ULONG kind = 0; kind < 2; ++kind) {
          Extension info{kind, 0, 0, 0};
          SIZE_T returned = SIZE_T(-1);
          LONG q = query(GetCurrentProcess(), base, 14, &info, sizeof info,
                         &returned);
          printf("extension image=%u flags=%08lX requested=%llX type=%lu "
                 "status=%08lX returned=%llX bytes=",
                 image, flags, (unsigned long long)requested, kind, ULONG(q),
                 (unsigned long long)returned);
          hex(&info, sizeof info);
          puts("");
          if (q >= 0 && info.rva && info.size) {
            release = static_cast<unsigned char *>(base) + info.rva;
            for (ULONG protection :
                 {1UL, 2UL, 4UL, 8UL, 16UL, 32UL, 64UL, 128UL, 0x120UL}) {
              PVOID address = release;
              SIZE_T extent = 1;
              ULONG old = 0xCCCCCCCC;
              LONG result = protect(GetCurrentProcess(), &address, &extent,
                                    protection, &old);
              printf("protect image=%u flags=%08lX requested=%llX type=%lu "
                     "protection=%08lX status=%08lX old=%08lX offset=%llX "
                     "size=%llX\n",
                     image, flags, (unsigned long long)requested, kind,
                     protection, ULONG(result), old,
                     (unsigned long long)((ULONG_PTR)address - (ULONG_PTR)base),
                     (unsigned long long)extent);
              ++protections;
            }
          }
        }
        LONG done = unmap(GetCurrentProcess(), release);
        if (done >= 0)
          ++unmapped;
        else
          failed = true;
        printf("unmap image=%u status=%08lX\n", image, ULONG(done));
      }
    LONG done = close(section);
    if (done >= 0)
      ++closed;
    else
      failed = true;
  }
  if (!GetProcessHandleCount(GetCurrentProcess(), &after))
    return 7;
  failed = failed || before != after || mapped != unmapped || closed != 3 ||
           ordinal != 48;
  printf("complete cases=%u protection-calls=%u mapped=%u unmapped=%u "
         "closed=%u handles-before=%lu handles-after=%lu failed=%u\n",
         ordinal, protections, mapped, unmapped, closed, before, after, failed);
  return failed ? 8 : 0;
}
