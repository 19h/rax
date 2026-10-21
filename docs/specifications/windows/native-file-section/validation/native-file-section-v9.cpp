#define NOMINMAX
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <windows.h>
#include <winternl.h>

using Create = LONG(NTAPI *)(PHANDLE, ACCESS_MASK, POBJECT_ATTRIBUTES,
                             PLARGE_INTEGER, ULONG, ULONG, HANDLE);
using Map = LONG(NTAPI *)(HANDLE, HANDLE, PVOID *, ULONG_PTR, SIZE_T,
                          PLARGE_INTEGER, PSIZE_T, ULONG, ULONG, ULONG);
using Unmap = LONG(NTAPI *)(HANDLE, PVOID);
using Query = LONG(NTAPI *)(HANDLE, PVOID, ULONG, PVOID, SIZE_T, PSIZE_T);
using Close = LONG(NTAPI *)(HANDLE);

struct ImageInformation {
  PVOID base;
  SIZE_T size;
  ULONG flags;
};

int main(int argc, char **argv) {
  if (argc != 2)
    return 10;
  HMODULE ntdll = GetModuleHandleW(L"ntdll.dll");
  auto create =
      reinterpret_cast<Create>(GetProcAddress(ntdll, "NtCreateSection"));
  auto map = reinterpret_cast<Map>(GetProcAddress(ntdll, "NtMapViewOfSection"));
  auto unmap =
      reinterpret_cast<Unmap>(GetProcAddress(ntdll, "NtUnmapViewOfSection"));
  auto query =
      reinterpret_cast<Query>(GetProcAddress(ntdll, "NtQueryVirtualMemory"));
  auto close = reinterpret_cast<Close>(GetProcAddress(ntdll, "NtClose"));
  if (!create || !map || !unmap || !query || !close)
    return 11;
  wchar_t root[MAX_PATH]{}, system[MAX_PATH]{};
  if (!GetTempPathW(MAX_PATH, root) || !GetSystemDirectoryW(system, MAX_PATH))
    return 12;
  std::wstring directory = std::wstring(root) + L"native-file-section-v9-" +
                           std::to_wstring(GetCurrentProcessId());
  if (!CreateDirectoryW(directory.c_str(), nullptr))
    return 13;
  std::wstring original = std::wstring(system) + L"\\version.dll";
  DWORD initial = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &initial))
    return 14;
  unsigned role = 0;
  for (LONGLONG maximum : {0LL, 1LL, 4096LL}) {
    std::wstring path =
        directory + L"\\image-" + std::to_wstring(role) + L".dll";
    if (!CopyFileW(original.c_str(), path.c_str(), TRUE))
      return 15;
    HANDLE file = CreateFileW(path.c_str(), 0x100021, 7, nullptr, OPEN_EXISTING,
                              0, nullptr);
    if (file == INVALID_HANDLE_VALUE)
      return 16;
    LARGE_INTEGER value{};
    value.QuadPart = maximum;
    HANDLE section = nullptr;
    LONG created = create(&section, 0xD, nullptr, maximum ? &value : nullptr,
                          PAGE_EXECUTE, SEC_IMAGE, file);
    if (created < 0 || !section)
      return 17;
    if (!CloseHandle(file))
      return 18;
    PVOID base = nullptr;
    SIZE_T view = 0;
    LONG mapped = map(section, GetCurrentProcess(), &base, 0, 0, nullptr, &view,
                      1, 0, PAGE_READONLY);
    LONG closed = close(section);
    if (mapped < 0 || !base || closed < 0)
      return 19;
    ImageInformation info{};
    SIZE_T returned = 0;
    LONG queried =
        query(GetCurrentProcess(), base, 6, &info, sizeof(info), &returned);
    MEMORY_BASIC_INFORMATION region{};
    SIZE_T basic = VirtualQuery(base, &region, sizeof(region));
    unsigned char header[2]{};
    std::memcpy(header, base, 2);
    SetLastError(0);
    BOOL deleted = DeleteFileW(path.c_str());
    DWORD delete_error = GetLastError();
    LONG unmapped = unmap(GetCurrentProcess(), base);
    BOOL deleted_after = deleted || DeleteFileW(path.c_str());
    std::printf(
        "case role=%u maximum=%lld created=%08lX mapped=%08lX view=%llu "
        "closed=%08lX query=%08lX returned=%llu image-size=%llu flags=%08lX "
        "header=%02X%02X virtual-query=%llu region-size=%llu "
        "region-state=%08lX region-type=%08lX delete=%u delete-error=%lu "
        "unmap=%08lX deleted-after=%u\n",
        role++, maximum, static_cast<ULONG>(created),
        static_cast<ULONG>(mapped), static_cast<unsigned long long>(view),
        static_cast<ULONG>(closed), static_cast<ULONG>(queried),
        static_cast<unsigned long long>(returned),
        static_cast<unsigned long long>(info.size), info.flags,
        static_cast<unsigned>(header[0]), static_cast<unsigned>(header[1]),
        static_cast<unsigned long long>(basic),
        static_cast<unsigned long long>(region.RegionSize), region.State,
        region.Type, static_cast<unsigned>(deleted), delete_error,
        static_cast<ULONG>(unmapped), static_cast<unsigned>(deleted_after));
    if (queried < 0 || !basic || header[0] != 'M' || header[1] != 'Z' ||
        unmapped < 0 || !deleted_after)
      return 20;
  }
  DWORD final_count = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &final_count) ||
      !RemoveDirectoryW(directory.c_str()))
    return 21;
  std::printf(
      "summary profile=%s cases=%u handles-before=%lu handles-after=%lu\n",
      argv[1], role, initial, final_count);
  return initial == final_count ? 0 : 22;
}
