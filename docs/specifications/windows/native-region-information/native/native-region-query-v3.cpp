#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <windows.h>
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
static void image(const char *name, unsigned char *base) {
  auto dos = reinterpret_cast<IMAGE_DOS_HEADER *>(base);
  auto nt = reinterpret_cast<IMAGE_NT_HEADERS *>(base + dos->e_lfanew);
  printf("image name=%s base=%p machine=%04X sections=%u size=%lu headers=%lu "
         "section-align=%lu file-align=%lu\n",
         name, base, nt->FileHeader.Machine, nt->FileHeader.NumberOfSections,
         nt->OptionalHeader.SizeOfImage, nt->OptionalHeader.SizeOfHeaders,
         nt->OptionalHeader.SectionAlignment, nt->OptionalHeader.FileAlignment);
  region(name, base);
  auto section = IMAGE_FIRST_SECTION(nt);
  for (unsigned i = 0; i < nt->FileHeader.NumberOfSections; ++i) {
    char section_name[9] = {};
    memcpy(section_name, section[i].Name, 8);
    printf("section image=%s index=%u name=%s rva=%08lX virtual=%lu raw=%lu "
           "characteristics=%08lX\n",
           name, i, section_name, section[i].VirtualAddress,
           section[i].Misc.VirtualSize, section[i].SizeOfRawData,
           section[i].Characteristics);
  }
  uintptr_t cursor = uintptr_t(base),
            end = cursor + nt->OptionalHeader.SizeOfImage;
  while (cursor < end) {
    MEMORY_BASIC_INFORMATION m{};
    if (!VirtualQuery(reinterpret_cast<void *>(cursor), &m, sizeof(m)) ||
        m.RegionSize == 0)
      break;
    printf("pages image=%s offset=%zu bytes=%zu state=%08lX protect=%08lX "
           "allocation-protect=%08lX type=%08lX\n",
           name, cursor - uintptr_t(base), m.RegionSize, m.State, m.Protect,
           m.AllocationProtect, m.Type);
    cursor = uintptr_t(m.BaseAddress) + m.RegionSize;
  }
}
int main(int argc, char **argv) {
  setvbuf(stdout, nullptr, _IONBF, 0);
  query = reinterpret_cast<Query>(
      GetProcAddress(GetModuleHandleW(L"ntdll.dll"), "NtQueryVirtualMemory"));
  if (!query)
    return 1;
  SYSTEM_INFO info{};
  GetSystemInfo(&info);
  printf("profile=%s pointer=%zu page=%lu allocation-granularity=%lu\n",
         argc > 1 ? argv[1] : "unknown", sizeof(void *), info.dwPageSize,
         info.dwAllocationGranularity);
  if (info.dwPageSize != 4096)
    return 2;
  image("ntdll",
        reinterpret_cast<unsigned char *>(GetModuleHandleW(L"ntdll.dll")));
  image("main", reinterpret_cast<unsigned char *>(GetModuleHandleW(nullptr)));
  auto reserved = static_cast<unsigned char *>(
      VirtualAlloc(nullptr, 65536, MEM_RESERVE, PAGE_READWRITE));
  if (!reserved)
    return 3;
  region("private-reserved", reserved);
  if (!VirtualAlloc(reserved + 4096, 8192, MEM_COMMIT, PAGE_READWRITE) ||
      !VirtualAlloc(reserved + 20480, 4096, MEM_COMMIT, PAGE_READONLY))
    return 4;
  region("private-partial-base", reserved);
  region("private-partial-committed", reserved + 4096);
  region("private-partial-gap", reserved + 16384);
  if (!VirtualFree(reserved + 8192, 4096, MEM_DECOMMIT))
    return 5;
  region("private-decommitted", reserved + 8192);
  VirtualFree(reserved, 0, MEM_RELEASE);
  auto mapping =
      CreateFileMappingW(INVALID_HANDLE_VALUE, nullptr,
                         PAGE_READWRITE | SEC_RESERVE, 0, 65536, nullptr);
  if (!mapping)
    return 6;
  auto view = static_cast<unsigned char *>(
      MapViewOfFile(mapping, FILE_MAP_WRITE, 0, 0, 65536));
  if (!view)
    return 7;
  region("pagefile-reserved", view);
  if (!VirtualAlloc(view + 4096, 12288, MEM_COMMIT, PAGE_READWRITE))
    return 8;
  region("pagefile-partial-base", view);
  region("pagefile-partial-committed", view + 4096);
  UnmapViewOfFile(view);
  CloseHandle(mapping);
  auto file = CreateFileW(L"probe-data.bin", GENERIC_READ | GENERIC_WRITE,
                          FILE_SHARE_READ, nullptr, CREATE_NEW,
                          FILE_ATTRIBUTE_TEMPORARY | FILE_FLAG_DELETE_ON_CLOSE,
                          nullptr);
  if (file == INVALID_HANDLE_VALUE)
    return 9;
  LARGE_INTEGER size{};
  size.QuadPart = 20480;
  if (!SetFilePointerEx(file, size, nullptr, FILE_BEGIN) || !SetEndOfFile(file))
    return 10;
  mapping = CreateFileMappingW(file, nullptr, PAGE_WRITECOPY, 0, 0, nullptr);
  if (!mapping)
    return 11;
  view = static_cast<unsigned char *>(
      MapViewOfFile(mapping, FILE_MAP_COPY, 0, 0, 20480));
  if (!view)
    return 12;
  region("file-copy-before", view);
  view[4096] = 0x42;
  view[16384] = 0x43;
  region("file-copy-after", view);
  region("file-copy-after-page", view + 4096);
  UnmapViewOfFile(view);
  CloseHandle(mapping);
  CloseHandle(file);
  wchar_t path[32768];
  if (!GetModuleFileNameW(nullptr, path, 32768))
    return 13;
  file = CreateFileW(path, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_DELETE,
                     nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
  if (file == INVALID_HANDLE_VALUE)
    return 14;
  mapping = CreateFileMappingW(file, nullptr, PAGE_READONLY | SEC_IMAGE, 0, 0,
                               nullptr);
  if (!mapping)
    return 15;
  view = static_cast<unsigned char *>(
      MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, 0));
  if (!view)
    return 16;
  image("mapped-main-before", view);
  auto dos = reinterpret_cast<IMAGE_DOS_HEADER *>(view);
  auto nt = reinterpret_cast<IMAGE_NT_HEADERS *>(view + dos->e_lfanew);
  auto sections = IMAGE_FIRST_SECTION(nt);
  for (unsigned i = 0; i < nt->FileHeader.NumberOfSections; ++i) {
    if (!(sections[i].Characteristics & IMAGE_SCN_MEM_WRITE) ||
        !sections[i].Misc.VirtualSize)
      continue;
    auto byte = view + sections[i].VirtualAddress;
    DWORD old;
    if (!VirtualProtect(byte, 1, PAGE_READWRITE, &old))
      return 17;
    *static_cast<volatile unsigned char *>(byte) = 0x47;
    region("mapped-main-after-copy", byte);
    break;
  }
  UnmapViewOfFile(view);
  CloseHandle(mapping);
  CloseHandle(file);
  printf("finished=1\n");
  return 0;
}
