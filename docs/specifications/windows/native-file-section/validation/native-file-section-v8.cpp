#define NOMINMAX
#include <array>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>
#include <windows.h>
#include <winternl.h>

using Create = LONG(NTAPI *)(PHANDLE, ACCESS_MASK, POBJECT_ATTRIBUTES,
                             PLARGE_INTEGER, ULONG, ULONG, HANDLE);
using Duplicate = LONG(NTAPI *)(HANDLE, HANDLE, HANDLE, PHANDLE, ACCESS_MASK,
                                ULONG, ULONG);
using QueryObject = LONG(NTAPI *)(HANDLE, ULONG, PVOID, ULONG, PULONG);
using Close = LONG(NTAPI *)(HANDLE);

int main(int argc, char **argv) {
  if (argc != 2)
    return 10;
  HMODULE ntdll = GetModuleHandleW(L"ntdll.dll");
  auto create =
      reinterpret_cast<Create>(GetProcAddress(ntdll, "NtCreateSection"));
  auto duplicate =
      reinterpret_cast<Duplicate>(GetProcAddress(ntdll, "NtDuplicateObject"));
  auto query =
      reinterpret_cast<QueryObject>(GetProcAddress(ntdll, "NtQueryObject"));
  auto close = reinterpret_cast<Close>(GetProcAddress(ntdll, "NtClose"));
  if (!create || !duplicate || !query || !close)
    return 11;
  wchar_t root[MAX_PATH]{}, system[MAX_PATH]{};
  if (!GetTempPathW(MAX_PATH, root) || !GetSystemDirectoryW(system, MAX_PATH))
    return 12;
  std::wstring directory = std::wstring(root) + L"native-file-section-v8-" +
                           std::to_wstring(GetCurrentProcessId());
  if (!CreateDirectoryW(directory.c_str(), nullptr))
    return 13;
  std::wstring path = directory + L"\\image.dll";
  std::wstring original = std::wstring(system) + L"\\version.dll";
  if (!CopyFileW(original.c_str(), path.c_str(), TRUE))
    return 14;
  HANDLE file = CreateFileW(path.c_str(), 0x100021, 7, nullptr, OPEN_EXISTING,
                            0, nullptr);
  if (file == INVALID_HANDLE_VALUE)
    return 15;
  DWORD initial = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &initial))
    return 16;
  std::vector<ULONG> masks{0,          0xD,        0x1F,       0xF001F,
                           0x1F001F,   0xFFFFFFFF, 0x03000000, 0x81000000,
                           0x80000001, 0x40000008, 0x20000004};
  for (unsigned bit = 0; bit < 32; ++bit)
    masks.push_back(ULONG(1) << bit);
  unsigned role = 0;
  for (ULONG source_access : {0UL, 0xDUL, 0xF001FUL, 0x100000UL, 0x1000000UL}) {
    HANDLE source = nullptr;
    LONG created = create(&source, source_access, nullptr, nullptr,
                          PAGE_EXECUTE, SEC_IMAGE, file);
    if (created < 0 || !source)
      return 17;
    for (ULONG options : {0UL, 2UL})
      for (ULONG access : masks) {
        HANDLE target = reinterpret_cast<HANDLE>(static_cast<uintptr_t>(-3));
        LONG status =
            duplicate(GetCurrentProcess(), source, GetCurrentProcess(), &target,
                      access, 0, options);
        bool valid = target && target != reinterpret_cast<HANDLE>(
                                             static_cast<uintptr_t>(-3));
        LONG queried = static_cast<LONG>(0xDEADBEEF);
        ULONG returned = 0, grant = 0;
        alignas(8) std::array<unsigned char, 56> basic{};
        if (valid) {
          queried = query(target, 0, basic.data(),
                          static_cast<ULONG>(basic.size()), &returned);
          std::memcpy(&grant, basic.data() + 4, sizeof(grant));
        }
        std::printf(
            "case role=%u source-access=%08lX desired=%08lX options=%08lX "
            "status=%08lX valid=%u query=%08lX returned=%lu grant=%08lX\n",
            role++, source_access, access, options, static_cast<ULONG>(status),
            static_cast<unsigned>(valid), static_cast<ULONG>(queried), returned,
            grant);
        if (valid)
          close(target);
      }
    close(source);
  }
  DWORD final_count = 0;
  if (!GetProcessHandleCount(GetCurrentProcess(), &final_count))
    return 18;
  CloseHandle(file);
  if (!DeleteFileW(path.c_str()) || !RemoveDirectoryW(directory.c_str()))
    return 19;
  std::printf(
      "summary profile=%s cases=%u handles-before=%lu handles-after=%lu\n",
      argv[1], role, initial, final_count);
  return initial == final_count ? 0 : 20;
}
