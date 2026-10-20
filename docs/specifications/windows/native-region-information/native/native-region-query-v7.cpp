#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <windows.h>
using Query = LONG(NTAPI *)(HANDLE, const void *, ULONG, void *, SIZE_T,
                            SIZE_T *);
static Query query;
#pragma section(".shrw", read, write, shared)
#pragma section(".shro", read, shared)
__declspec(allocate(".shrw")) volatile unsigned char shared_rw[8192] = {0x51};
__declspec(allocate(".shro")) volatile unsigned char shared_ro[8192] = {0x62};

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

static void change(const char *role, unsigned char *allocation,
                   unsigned char *page) {
  region(role, allocation);
  DWORD old = 0;
  SetLastError(0);
  BOOL ok = VirtualProtect(page, 4096, PAGE_READWRITE, &old);
  printf("protect role=%s page=%p ok=%u error=%lu old=%08lX\n", role, page,
         unsigned(ok), GetLastError(), old);
  if (!ok)
    return;
  region("after-writable", allocation);
  auto byte = reinterpret_cast<volatile unsigned char *>(page);
  const auto value = *byte;
  *byte = value;
  region("after-identical-write", allocation);
  DWORD ignored;
  if (!VirtualProtect(page, 4096, old, &ignored)) {
    printf("restore-failed role=%s error=%lu\n", role, GetLastError());
    ExitProcess(20);
  }
  region("after-restored", allocation);
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
  auto main = reinterpret_cast<unsigned char *>(GetModuleHandleW(nullptr));
  image("main-shared", main);
  auto rw = const_cast<unsigned char *>(shared_rw);
  auto ro = const_cast<unsigned char *>(shared_ro);
  const auto rw_offset = rw - main;
  const auto ro_offset = ro - main;
  printf("shared-offsets rw=%zu ro=%zu first=%u%u\n", size_t(rw_offset),
         size_t(ro_offset), unsigned(shared_rw[0]), unsigned(shared_ro[0]));
  change("main-shared-rw", main, rw);
  change("main-shared-ro", main, ro);
  wchar_t path[32768];
  if (!GetModuleFileNameW(nullptr, path, 32768))
    return 3;
  auto file =
      CreateFileW(path, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_DELETE,
                  nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
  if (file == INVALID_HANDLE_VALUE)
    return 4;
  auto mapping = CreateFileMappingW(file, nullptr, PAGE_READONLY | SEC_IMAGE, 0,
                                    0, nullptr);
  if (!mapping)
    return 5;
  auto view = static_cast<unsigned char *>(
      MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, 0));
  if (!view)
    return 6;
  image("mapped-shared", view);
  change("mapped-shared-rw", view, view + rw_offset);
  change("mapped-shared-ro", view, view + ro_offset);
  change("mapped-private-header", view, view);
  UnmapViewOfFile(view);
  CloseHandle(mapping);
  CloseHandle(file);
  printf("finished=1\n");
  return 0;
}
