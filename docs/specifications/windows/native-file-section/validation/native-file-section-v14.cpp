#define NOMINMAX
#include <cstdint>
#include <cstdio>
#include <string>
#include <windows.h>
#include <winternl.h>

#pragma section(".raxshr", read, write, shared)
__declspec(allocate(".raxshr")) volatile LONG shared_cell = 0x13579BDF;
#pragma comment(linker, "/SECTION:.raxshr,RWS")

using Create = LONG(NTAPI *)(PHANDLE, ACCESS_MASK, POBJECT_ATTRIBUTES,
                             PLARGE_INTEGER, ULONG, ULONG, HANDLE);
using Map = LONG(NTAPI *)(HANDLE, HANDLE, PVOID *, ULONG_PTR, SIZE_T,
                          PLARGE_INTEGER, PSIZE_T, ULONG, ULONG, ULONG);
using Unmap = LONG(NTAPI *)(HANDLE, PVOID);
using Close = LONG(NTAPI *)(HANDLE);

int main(int argc, char **argv) {
  if (argc != 2)
    return 10;
  auto ntdll = GetModuleHandleW(L"ntdll.dll");
  auto create =
      reinterpret_cast<Create>(GetProcAddress(ntdll, "NtCreateSection"));
  auto map = reinterpret_cast<Map>(GetProcAddress(ntdll, "NtMapViewOfSection"));
  auto unmap =
      reinterpret_cast<Unmap>(GetProcAddress(ntdll, "NtUnmapViewOfSection"));
  auto close = reinterpret_cast<Close>(GetProcAddress(ntdll, "NtClose"));
  if (!create || !map || !unmap || !close)
    return 11;
  wchar_t root[MAX_PATH]{}, executable[MAX_PATH]{};
  if (!GetTempPathW(MAX_PATH, root) ||
      !GetModuleFileNameW(nullptr, executable, MAX_PATH))
    return 12;
  std::wstring directory = std::wstring(root) + L"native-file-section-v14-" +
                           std::to_wstring(GetCurrentProcessId());
  if (!CreateDirectoryW(directory.c_str(), nullptr))
    return 13;
  std::wstring path = directory + L"\\shared-image.exe";
  if (!CopyFileW(executable, path.c_str(), TRUE))
    return 14;
  HANDLE file = CreateFileW(path.c_str(), 0x100021, 7, nullptr, OPEN_EXISTING,
                            0, nullptr);
  if (file == INVALID_HANDLE_VALUE)
    return 15;
  if (!CloseHandle(file))
    return 25;
  DWORD initial = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &initial))
    return 16;
  file = CreateFileW(path.c_str(), 0x100021, 7, nullptr, OPEN_EXISTING, 0,
                     nullptr);
  if (file == INVALID_HANDLE_VALUE)
    return 26;
  HANDLE sections[2]{};
  PVOID bases[3]{};
  SIZE_T sizes[3]{};
  for (unsigned i = 0; i != 2; ++i)
    if (create(&sections[i], 0xF001F, nullptr, nullptr, PAGE_EXECUTE, SEC_IMAGE,
               file) < 0 ||
        !sections[i])
      return 17;
  for (unsigned i = 0; i != 3; ++i)
    if (map(sections[i == 2 ? 1 : 0], GetCurrentProcess(), &bases[i], 0, 0,
            nullptr, &sizes[i], 1, 0, PAGE_READONLY) < 0 ||
        !bases[i])
      return 18;
  if (close(sections[0]) < 0 || close(sections[1]) < 0 || !CloseHandle(file))
    return 19;
  uintptr_t rva = reinterpret_cast<uintptr_t>(&shared_cell) -
                  reinterpret_cast<uintptr_t>(GetModuleHandleW(nullptr));
  volatile LONG *cells[3]{};
  LONG before[3]{}, after[3]{};
  MEMORY_BASIC_INFORMATION regions[3]{};
  for (unsigned i = 0; i != 3; ++i) {
    if (rva + sizeof(LONG) > sizes[i])
      return 20;
    cells[i] = reinterpret_cast<volatile LONG *>(
        static_cast<unsigned char *>(bases[i]) + rva);
    before[i] = *cells[i];
    if (VirtualQuery(const_cast<LONG *>(cells[i]), &regions[i],
                     sizeof(regions[i])) != sizeof(regions[i]))
      return 21;
  }
  *cells[0] = 0x2468ACE0;
  for (unsigned i = 0; i != 3; ++i) {
    after[i] = *cells[i];
    std::printf("case role=%u rva=%llu view=%llu before=%08lX after=%08lX "
                "protection=%08lX state=%08lX type=%08lX\n",
                i, static_cast<unsigned long long>(rva),
                static_cast<unsigned long long>(sizes[i]),
                static_cast<ULONG>(before[i]), static_cast<ULONG>(after[i]),
                regions[i].Protect, regions[i].State, regions[i].Type);
  }
  for (auto base : bases)
    if (unmap(GetCurrentProcess(), base) < 0)
      return 22;
  DWORD final_count = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &final_count) ||
      !DeleteFileW(path.c_str()) || !RemoveDirectoryW(directory.c_str()))
    return 23;
  std::printf(
      "summary profile=%s cases=3 handles-before=%lu handles-after=%lu\n",
      argv[1], initial, final_count);
  return initial == final_count ? 0 : 24;
}
